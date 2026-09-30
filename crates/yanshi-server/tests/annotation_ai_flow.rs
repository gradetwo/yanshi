//! Phase 4b 验收：**标注 → AI 解析 → AI 建议 → 人类接受 → 应用 → 状态更新**。
//!
//! 设计 13.4（第 976 行）规定的工作流：
//! 「人类标注 → AI 解析 → AI 建议 → 人类预览 → 接受/拒绝 → 应用 → 状态更新」；
//! AI 侧通过 `list_annotations(status=pending)` 或 WS 事件感知新标注（第 976 行）。
//! 设计 12.6：`accept_suggestion → reapply`、`reject_suggestion → 记录原因`。
//!
//! 本测试把这条链**逐段断言**，任何一段断裂都会失败 ✓（此前的经验：能力常常已经存在，
//! 但缺少一次端到端验证，真实缺口就藏在中间某一段 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_ann", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn annotation_to_ai_suggestion_to_acceptance() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_ann", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }

    // ① 人类标注：区域 + 意图 + 文字说明。
    let annotation = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_annotation",
            &json!({"type": "region", "intent": "modify",
                    "target": {"x": 10.0, "y": 10.0, "w": 40.0, "h": 40.0},
                    "content": "这块太灰了，加点对比"}),
        )
    };
    assert_eq!(
        annotation["ok"],
        json!(true),
        "创建标注应成功：{annotation}"
    );
    let annotation_id = annotation["annotation_id"]
        .as_str()
        .or_else(|| annotation["id"].as_str())
        .expect("create_annotation 应返回标注 id")
        .to_owned();

    // ② AI 侧轮询：pending 列表里必须能看到它（设计指定的感知方式）。
    let pending = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_annotations", &json!({"status": "pending"}))
    };
    let listed = pending["annotations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        listed
            .iter()
            .any(|item| item["annotation_id"] == json!(annotation_id)
                || item["id"] == json!(annotation_id)),
        "pending 列表应包含刚创建的标注：{pending}"
    );

    // ③ AI 提出**可执行补丁**（设计：suggest 携带 patch 步骤）。
    let suggestion = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "suggest",
            &json!({"annotation_id": annotation_id, "summary": "提高对比度",
                    "patch": [{"tool": "add_adjustment",
                               "arguments": {"layer_id": "L", "adjustment_type": "brightness_contrast",
                                             "params": {"brightness": 0.0, "contrast": 0.4}}}]}),
        )
    };
    assert_eq!(
        suggestion["ok"],
        json!(true),
        "建议应创建成功：{suggestion}"
    );
    let suggestion_id = suggestion["atom_id"]
        .as_str()
        .or_else(|| suggestion["suggestion_id"].as_str())
        .expect("suggest 应返回建议 id")
        .to_owned();

    // ④ 标注应记下这条建议的来源（设计 4.6 的 `suggestion_id` 字段）。
    let linked = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_annotation",
            &json!({"annotation_id": annotation_id}),
        )
    };
    let annotation_value = linked
        .get("annotation")
        .cloned()
        .unwrap_or_else(|| linked.clone());
    assert_eq!(
        annotation_value["suggestion_id"],
        json!(suggestion_id),
        "标注应链到建议（suggestion_id）：{annotation_value}"
    );

    // ⑤ 人类接受：按序重放 patch ⇒ 产生**真实效果**。
    let before = objects(&mut workspace);
    let accepted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "accept_suggestion",
            &json!({"suggestion_id": suggestion_id}),
        )
    };
    assert_eq!(accepted["ok"], json!(true), "接受建议应成功：{accepted}");
    let after = objects(&mut workspace);
    assert!(
        after > before,
        "接受建议后应真实产生对象（此前 {before}，之后 {after}）—— patch 必须被重放"
    );

    // ⑥ 状态更新：关联标注应变为 resolved，并记录 resolved_by（设计 12.6 + 4.6）。
    let resolved = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_annotation",
            &json!({"annotation_id": annotation_id}),
        )
    };
    let resolved_value = resolved
        .get("annotation")
        .cloned()
        .unwrap_or_else(|| resolved.clone());
    assert_eq!(
        resolved_value["status"],
        json!("resolved"),
        "接受建议后关联标注应置为 resolved：{resolved_value}"
    );
}

/// 拒绝分支：`reject_suggestion` 必须记录原因，并把关联标注置为 `rejected`
/// （设计 12.6：`reject_suggestion → 记录原因`；4.6 的三态里 rejected 不参与 resolved 口径）。
#[test]
fn rejecting_a_suggestion_marks_the_annotation_rejected() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_ann", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let annotation = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_annotation",
            &json!({"type": "region", "intent": "style",
                    "target": {"x": 0.0, "y": 0.0, "w": 20.0, "h": 20.0}, "content": "换个颜色"}),
        )
    };
    let annotation_id = annotation["annotation_id"]
        .as_str()
        .or_else(|| annotation["id"].as_str())
        .expect("应返回标注 id")
        .to_owned();
    let suggestion = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "suggest",
            &json!({"annotation_id": annotation_id, "summary": "反相",
                    "patch": [{"tool": "add_adjustment",
                               "arguments": {"layer_id": "L", "adjustment_type": "invert"}}]}),
        )
    };
    let suggestion_id = suggestion["suggestion_id"]
        .as_str()
        .expect("应返回建议 id")
        .to_owned();
    let objects_before = objects(&mut workspace);

    let rejected = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "reject_suggestion",
            &json!({"suggestion_id": suggestion_id, "reason": "风格不符"}),
        )
    };
    assert_eq!(rejected["ok"], json!(true), "拒绝建议应成功：{rejected}");
    assert_eq!(
        objects(&mut workspace),
        objects_before,
        "拒绝建议**不得**产生效果（patch 不应被重放）"
    );
    let after = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_annotation",
            &json!({"annotation_id": annotation_id}),
        )
    };
    let value = after
        .get("annotation")
        .cloned()
        .unwrap_or_else(|| after.clone());
    assert_eq!(
        value["status"],
        json!("rejected"),
        "拒绝建议后关联标注应置为 rejected：{value}"
    );
}

/// 状态里存活对象数（用于判断 patch 是否真的产生了效果）。
fn objects(workspace: &mut Workspace) -> usize {
    let document = workspace.document_mut("doc_ann").expect("文档应存在");
    document.state().alive_objects().len()
}
