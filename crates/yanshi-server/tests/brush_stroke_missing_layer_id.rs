//! **brush_stroke 缺 layer_id 时，报错要附当前文档的图层 id 列表**（报告 4 P1 残留小项）。
//!
//! **原症状**：detail 只有“缺少必填参数 layer_id”，调用方不知道该填哪个 id，
//! 只能再调一次列表接口（或乱试几个 id）。
//! **判据**：① 单调 registry.call 缺 layer_id ⇒ ok=false、invalid_argument，
//! detail 保留原句且列出已建的两层；② batch 里同款缺失 ⇒
//! failed_calls[0].error.context.detail 同样列出；③ 文档无存活图层时单独说
//! （一个都没有），不给空列表。①②③ 都不改报错码与原句，只在后面附列表。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_p1", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_p1", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
}

fn detail_of(value: &serde_json::Value) -> String {
    value["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn create_layers(workspace: &mut Workspace, ids: &[&str]) {
    let mut ctx = context(workspace);
    for id in ids {
        let made = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": id }));
        assert_eq!(made["ok"], json!(true), "建层 {id} 应成功：{made}");
    }
}

/// **单调缺 layer_id ⇒ 报错列出可用图层**。
#[test]
fn brush_stroke_without_layer_id_lists_available_layers() {
    let mut workspace = workspace();
    setup(&mut workspace);
    create_layers(&mut workspace, &["L_alpha", "L_beta"]);
    let mut ctx = context(&mut workspace);
    let refused = registry().call(
        &mut ctx,
        "brush_stroke",
        &json!({"brush": "2B_pencil", "size": 10,
                "points": [[10.0, 10.0, 0.5], [20.0, 10.0, 0.5]]}),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "缺 layer_id 应当被拒：{refused}"
    );
    assert_eq!(
        refused["error_code"],
        json!("invalid_argument"),
        "{refused}"
    );
    let detail = detail_of(&refused);
    assert!(
        detail.contains("缺少必填参数 layer_id"),
        "原句必须保留：{detail}"
    );
    assert!(
        detail.contains("L_alpha"),
        "应当列出可用图层 L_alpha：{detail}"
    );
    assert!(
        detail.contains("L_beta"),
        "应当列出可用图层 L_beta：{detail}"
    );
}

/// **batch 里的同款缺失 ⇒ failed_calls 的错误同样列出**。
#[test]
fn batch_brush_stroke_without_layer_id_lists_available_layers() {
    let mut workspace = workspace();
    setup(&mut workspace);
    create_layers(&mut workspace, &["L_alpha", "L_beta"]);
    let mut ctx = context(&mut workspace);
    let made = registry().call(
        &mut ctx,
        "batch",
        &json!({"calls": [{"tool": "brush_stroke", "arguments": {
            "brush": "2B_pencil", "size": 10,
            "points": [[10.0, 10.0, 0.5], [20.0, 10.0, 0.5]]}}]}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "批里一笔失败 ⇒ 顶层 ok=false：{made}"
    );
    assert_eq!(made["fail_count"], json!(1), "{made}");
    let failed = &made["failed_calls"][0];
    assert_eq!(failed["name"], json!("brush_stroke"), "{made}");
    let detail = detail_of(&failed["error"]);
    assert!(
        detail.contains("缺少必填参数 layer_id"),
        "原句必须保留：{detail}"
    );
    assert!(
        detail.contains("L_alpha"),
        "应当列出可用图层 L_alpha：{detail}"
    );
    assert!(
        detail.contains("L_beta"),
        "应当列出可用图层 L_beta：{detail}"
    );
}

/// **一个图层都没有时 ⇒ 单独说清楚**（而不是给一句空的“现有：”）。
#[test]
fn brush_stroke_without_layer_id_and_no_layers_says_so_plainly() {
    let mut workspace = workspace();
    setup(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry().call(
            &mut ctx,
            "delete_layer",
            &json!({"layer_id": "layer_default"}),
        );
        assert_eq!(deleted["ok"], json!(true), "删默认层应成功：{deleted}");
    }
    let mut ctx = context(&mut workspace);
    let refused = registry().call(
        &mut ctx,
        "brush_stroke",
        &json!({"brush": "2B_pencil", "size": 10,
                "points": [[10.0, 10.0, 0.5], [20.0, 10.0, 0.5]]}),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = detail_of(&refused);
    assert!(
        detail.contains("缺少必填参数 layer_id"),
        "原句必须保留：{detail}"
    );
    assert!(detail.contains("一个都没有"), "空集合要单独说：{detail}");
}
