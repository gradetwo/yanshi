//! 跨工具校验审计：**接受 id 的工具在 id 不存在时必须报错，而不是静默成功**。
//!
//! 起因：`reject_suggestion` 曾对不存在的建议返回 `ok`（写入了无意义的原子）。
//! 单个缺陷可以补一个测试，但更值得做的是**系统性地**扫这一整类问题：
//! 「功能能跑、边界静默错误」是最难靠人工发现的一类 bug。
//!
//! 两个部分：
//! 1. **空参扫描**：对注册表里每个工具用 `{}` 调用，只要求"不 panic、返回结构化结果" ——
//!    防止 `unwrap`/越界之类的崩溃；
//! 2. **伪 id 表**：对每个接受实体 id 的工具传入不存在的 id，断言返回错误码。

use serde_json::{json, Value};
use yanshi_server::DocumentSettings;
use yanshi_server::{NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    // 全量 profile：审计要覆盖所有已实现工具。
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc_id: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc_id, "human:1", "session:audit")
        .with_owner(true)
        .with_wait_for_render(true, 500)
}

/// 返回 (工具, 参数, 允许的错误码集合)。
fn bogus_id_cases() -> Vec<(&'static str, Value, Vec<&'static str>)> {
    let missing_layer = json!({"layer_id": "no_such_layer"});
    let missing_object = json!({"object_id": "no_such_object", "layer_id": "layer_1"});
    let missing_annotation = json!({"annotation_id": "no_such_annotation"});
    let missing_suggestion = json!({"suggestion_id": "01ZZZZZZZZZZZZZZZZZZZZZZZZZZ"});
    let cases: Vec<(&'static str, Value, Vec<&'static str>)> = vec![
        // 图层类
        (
            "delete_layer",
            missing_layer.clone(),
            vec!["reference_not_found"],
        ),
        // 注：`reorder_layers` 先做「order 必须是全部存活图层的完整顺序」校验，
        // 因此不存在的图层会以 `invalid_argument` 被拒（同样是正确行为）。
        (
            "reorder_layers",
            json!({"order": ["no_such_layer"]}),
            vec!["reference_not_found", "invalid_argument"],
        ),
        (
            "set_property",
            json!({"layer_id": "no_such_layer", "key": "visible", "value": {"visible": false}}),
            vec!["reference_not_found"],
        ),
        // 对象类
        (
            "move_object",
            json!({"object_id": "no_such_object", "delta": {"x": 1.0, "y": 1.0}}),
            vec!["reference_not_found"],
        ),
        (
            "transform_object",
            json!({"object_id": "no_such_object", "scale": 2.0}),
            vec!["reference_not_found", "invalid_argument"],
        ),
        (
            "update_adjustment",
            json!({"object_id": "no_such_object", "params": {}}),
            vec!["reference_not_found"],
        ),
        (
            "update_filter",
            json!({"object_id": "no_such_object", "params": {}}),
            vec!["reference_not_found"],
        ),
        (
            "delete_object",
            json!({"object_id": "no_such_object"}),
            vec!["reference_not_found"],
        ),
        // 绘画/修图类（引用不存在的图层）
        (
            "draw_shape",
            json!({"layer_id": "no_such_layer", "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 4, "h": 4}}, "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
            vec!["reference_not_found"],
        ),
        (
            "draw_stroke",
            json!({"layer_id": "no_such_layer", "data": {"points": [[0, 0], [4, 4]], "size": 2}}),
            vec!["reference_not_found"],
        ),
        (
            "fill",
            json!({"layer_id": "no_such_layer", "data": {"color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
            vec!["reference_not_found", "invalid_argument"],
        ),
        (
            "add_adjustment",
            json!({"layer_id": "no_such_layer", "adjustment_type": "invert"}),
            vec!["reference_not_found"],
        ),
        (
            "add_filter",
            json!({"layer_id": "no_such_layer", "filter_name": "invert"}),
            vec!["reference_not_found"],
        ),
        (
            "clone_stamp",
            json!({"layer_id": "no_such_layer", "points": [[1, 1]], "source_offset": [-4, -4]}),
            vec!["reference_not_found"],
        ),
        (
            "liquify_push",
            json!({"layer_id": "no_such_layer", "points": [[1, 1]], "direction": [1.0, 0.0]}),
            vec!["reference_not_found"],
        ),
        // 标注/建议类
        (
            "get_annotation",
            missing_annotation.clone(),
            vec!["reference_not_found"],
        ),
        (
            "resolve_annotation",
            missing_annotation.clone(),
            vec!["reference_not_found"],
        ),
        (
            "reject_annotation",
            missing_annotation.clone(),
            vec!["reference_not_found"],
        ),
        (
            "delete_annotation",
            missing_annotation.clone(),
            vec!["reference_not_found"],
        ),
        (
            "update_annotation",
            missing_annotation.clone(),
            vec!["reference_not_found"],
        ),
        (
            "accept_suggestion",
            missing_suggestion.clone(),
            vec!["reference_not_found"],
        ),
        (
            "reject_suggestion",
            missing_suggestion.clone(),
            vec!["reference_not_found"],
        ),
        (
            "preview_suggestion",
            missing_suggestion.clone(),
            vec!["reference_not_found"],
        ),
    ];
    let _ = missing_object;
    cases
}

#[test]
fn tools_reject_nonexistent_entity_ids() {
    let registry = registry();
    let mut failures = Vec::new();
    for (tool, args, allowed) in bogus_id_cases() {
        // 每个用例用独立文档，避免相互影响（例如 delete_layer 会改变后续状态）。
        let mut workspace = workspace();
        workspace
            .create_document(
                NewDocument::new("doc_audit", 64, 64),
                "human:1",
                "session:audit",
            )
            .unwrap();
        {
            let mut ctx = context(&mut workspace, "doc_audit");
            registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
        }
        let response = {
            let mut ctx = context(&mut workspace, "doc_audit");
            registry.call(&mut ctx, tool, &args)
        };
        if response["ok"] == json!(true) {
            failures.push(format!("{tool} 对不存在的实体返回了 ok=true：{response}"));
            continue;
        }
        let code = response["error_code"].as_str().unwrap_or_default();
        if !allowed.contains(&code) {
            failures.push(format!(
                "{tool} 的错误码 {code} 不在允许集合 {allowed:?}：{response}"
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "以下工具未正确校验实体存在性（共 {} 处）：\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// `preview_suggestion` 是**校验报告**：补丁里引用了未知工具时它**成功返回**，
/// 但必须把该步标为非法（`applicable=false`）—— 这条断言把这个语义固定住。
#[test]
fn preview_reports_unknown_tools_instead_of_failing() {
    let registry = registry();
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_audit", 16, 16),
            "human:1",
            "session:audit",
        )
        .unwrap();
    let mut ctx = context(&mut workspace, "doc_audit");
    let response = registry.call(
        &mut ctx,
        "preview_suggestion",
        &json!({"patch": [{"tool": "no_such_tool", "arguments": {}}]}),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    assert_eq!(response["applicable"], json!(false), "{response}");
    assert_eq!(response["steps"][0]["valid"], json!(false), "{response}");
    assert!(
        response["steps"][0]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("未知工具"),
        "{response}"
    );
}

/// 空参扫描：任何工具都不应因缺参而崩溃（必须返回结构化错误）。
#[test]
fn no_tool_panics_on_empty_arguments() {
    let registry = registry();
    let names: Vec<String> = registry
        .tools()
        .iter()
        .map(|spec| spec.name.to_owned())
        .collect();
    let mut panicked = Vec::new();
    for name in &names {
        let mut workspace = workspace();
        workspace
            .create_document(
                NewDocument::new("doc_audit", 16, 16),
                "human:1",
                "session:audit",
            )
            .unwrap();
        let mut ctx = context(&mut workspace, "doc_audit");
        let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            registry.call(&mut ctx, name, &json!({}))
        }));
        match response {
            Ok(value) => {
                assert!(
                    value.get("ok").is_some(),
                    "工具 {name} 在空参下未返回结构化结果：{value}"
                );
            }
            Err(_) => panicked.push(name.clone()),
        }
    }
    assert!(
        panicked.is_empty(),
        "以下工具在空参下 panic（共 {} 个）：{panicked:?}",
        panicked.len()
    );
}

/// 参数形状审计：类型错误、空数组、越界数值、**未知参数名**。
///
/// 最后一类最危险：若拼错的参数名被静默忽略，调用方会以为设置生效了，
/// 实际却按默认值渲染 —— 属于"看起来成功、结果不对"的静默错误。
#[test]
fn malformed_arguments_are_rejected() {
    let registry = registry();
    let cases: Vec<(&str, Value, &str)> = vec![
        // 类型错误：应为字符串却给数字。
        ("create_layer", json!({"layer_id": 42}), "layer_id 类型错误"),
        (
            "delete_layer",
            json!({"layer_id": ["a"]}),
            "layer_id 类型错误（数组）",
        ),
        // 空数组：需要非空/有意义的数组。
        (
            "draw_stroke",
            json!({"layer_id": "layer_1", "data": {"points": [], "size": 2}}),
            "空 points",
        ),
        ("suggest", json!({"patch": []}), "空 patch"),
        (
            "accept_suggestions",
            json!({"suggestion_ids": []}),
            "空 suggestion_ids",
        ),
        (
            "reject_suggestions",
            json!({"suggestion_ids": []}),
            "空 suggestion_ids",
        ),
        // 越界数值。
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "noise", "params": {"amount": 99}}),
            "noise.amount 越界",
        ),
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "film_grain", "params": {"size": 999}}),
            "film_grain.size 越界",
        ),
        (
            "add_adjustment",
            json!({"layer_id": "layer_1", "adjustment_type": "posterize", "params": {"levels": 1}}),
            "posterize.levels 越界",
        ),
        // 未知参数名（拼写错误）：**若被静默忽略即为缺陷**。
        (
            "add_filter",
            json!({"layer_id": "layer_1", "filter_name": "noise", "param": {"amount": 0.1}}),
            "params 拼写错误",
        ),
        (
            "create_layer",
            json!({"layer_id": "L2", "nmae": "typo"}),
            "顶层未知参数",
        ),
    ];

    let mut silent = Vec::new();
    let mut wrong_code = Vec::new();
    for (tool, args, label) in cases {
        let mut workspace = workspace();
        workspace
            .create_document(
                NewDocument::new("doc_shape", 32, 32),
                "human:1",
                "session:audit",
            )
            .unwrap();
        {
            let mut ctx = context(&mut workspace, "doc_shape");
            registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
        }
        let response = {
            let mut ctx = context(&mut workspace, "doc_shape");
            registry.call(&mut ctx, tool, &args)
        };
        if response["ok"] == json!(true) {
            silent.push(format!("{label} → {tool} 竟然成功：{response}"));
            continue;
        }
        let code = response["error_code"].as_str().unwrap_or_default();
        if ![
            "invalid_argument",
            "reference_not_found",
            "precondition_failed",
        ]
        .contains(&code)
        {
            wrong_code.push(format!("{label} → {tool} 错误码 {code}：{response}"));
        }
    }
    assert!(
        wrong_code.is_empty(),
        "错误码不符合预期：\n{}",
        wrong_code.join("\n")
    );
    assert!(
        silent.is_empty(),
        "以下畸形参数被**静默接受**（共 {} 处）：\n{}",
        silent.len(),
        silent.join("\n")
    );
}
