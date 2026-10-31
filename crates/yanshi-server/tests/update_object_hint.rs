//! update_object 空补丁报错必须指路 update_stroke（报告 5 分诊 §1）。
//!
//! 探针（基线 91d1d454）：`update_object` 只覆盖 7 个属性键，`patch {"opacity": 0.5}`
//! 报错“patch 中没有可修改的对象属性”且不指向改笔入口。本件只改报错文本：
//! 原文保留、原错误码不变，末尾追加“改笔用 update_stroke”；属性覆盖集合不动。

use serde_json::json;
use yanshi_core::{Atom, AtomKind, DocId};
use yanshi_server::tools::{ToolContext, ToolRegistry};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings {
        tile_size: 32,
        ..DocumentSettings::default()
    })
}

fn setup(workspace: &mut Workspace) -> DocId {
    workspace
        .create_document(NewDocument::new("doc_1", 96, 96), "human:1", "session:a")
        .unwrap();
    for payload_atom in [
        Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1"}),
        ),
        Atom::new(
            AtomKind::DrawStroke,
            "human:1",
            "session:a",
            json!({
                "object_id": "obj_1",
                "layer_id": "layer_1",
                "data": {"points": [[8.0, 8.0], [48.0, 40.0]], "size": 5.0},
            }),
        ),
    ] {
        workspace
            .commit("doc_1", payload_atom, "human:1", false)
            .unwrap();
    }
    "doc_1".to_owned()
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:web").with_owner(true)
}

#[test]
fn opacity_patch_error_points_to_update_stroke_and_keeps_error_code() {
    let mut workspace = workspace();
    let doc = setup(&mut workspace);
    let registry = ToolRegistry::core();
    let mut context = context(&mut workspace, &doc);
    let rejected = registry.call(
        &mut context,
        "update_object",
        &json!({"object_id": "obj_1", "patch": {"opacity": 0.5}}),
    );
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    assert_eq!(
        rejected["error_code"],
        json!("invalid_argument"),
        "{rejected}"
    );
    let detail = rejected["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("patch 中没有可修改的对象属性"),
        "原文必须保留，实测 detail={detail}"
    );
    assert!(
        detail.contains("update_stroke"),
        "报错必须指路 update_stroke，实测 detail={detail}"
    );
}

#[test]
fn normal_property_patch_still_succeeds() {
    let mut workspace = workspace();
    let doc = setup(&mut workspace);
    let registry = ToolRegistry::core();
    let mut context = context(&mut workspace, &doc);
    let changed = registry.call(
        &mut context,
        "update_object",
        &json!({"object_id": "obj_1", "patch": {"visible": false}}),
    );
    assert_eq!(changed["ok"], json!(true), "{changed}");
}
