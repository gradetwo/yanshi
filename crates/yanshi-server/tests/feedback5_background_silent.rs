//! 报告 5/6 第 4 项：图层 `set_property` 改 `background` 不许再静默 ok:true。
//!
//! 机理：图层没有 `background` 字段，`fold::apply_property_to_layer` 的
//! `other` 分支只把值存进图层 metadata；渲染层只读文档底色
//! `state.background`，故像素 0 变化。修法是顶层 `warnings` 给可观测信号，
//! `ok` 语义与落库（metadata 仍写、重放不变）一字不动；文档级与
//! `update_layer` 两路不在本件范围，行为保持原样。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_bg", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup() -> (Workspace, ToolRegistry) {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bg", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    (workspace, registry)
}

fn warnings_of(value: &serde_json::Value) -> Vec<String> {
    value["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_bg", Bbox::new(0.0, 0.0, 64.0, 64.0))
        .expect("渲染应成功")
        .2
}

/// 图层改 background：ok 仍为 true，但顶层 warnings 必须点名未生效，
/// 且渲染像素逐字节不变（复现探针的 0 像素变化，但不再无信号）。
#[test]
fn layer_set_property_background_warns_and_changes_no_pixels() {
    let (mut workspace, registry) = setup();
    let before = render(&mut workspace);
    let mut ctx = context(&mut workspace);
    let result = registry.call(
        &mut ctx,
        "set_property",
        &json!({"layer_id": "L", "key": "background",
                "value": {"r": 0, "g": 255, "b": 0, "a": 255}}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    let warnings = warnings_of(&result);
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("background") && w.contains("未生效")),
        "图层改 background 必须在顶层 warnings 点名未生效：{result}"
    );
    let after = render(&mut workspace);
    assert_eq!(after, before, "图层改 background 不应改变任何像素");
}

/// 正常图层属性修改无回归：改 opacity 不报 background 警告，仍 ok:true。
#[test]
fn normal_layer_property_change_has_no_background_warning() {
    let (mut workspace, registry) = setup();
    let mut ctx = context(&mut workspace);
    let result = registry.call(
        &mut ctx,
        "set_property",
        &json!({"layer_id": "L", "key": "opacity", "value": 0.5}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert!(
        !warnings_of(&result)
            .iter()
            .any(|w| w.contains("background")),
        "正常图层属性修改不应带 background 警告：{result}"
    );
}

/// 文档级与 update_layer 两路不在本件范围：行为保持原样，不被本件带偏。
/// 文档级（无 id）仍报错；update_layer 不认 background 补丁仍报错。
#[test]
fn document_and_update_layer_routes_unchanged() {
    let (mut workspace, registry) = setup();
    let mut ctx = context(&mut workspace);
    let no_id = registry.call(
        &mut ctx,
        "set_property",
        &json!({"key": "background", "value": {"r": 0, "g": 255, "b": 0, "a": 255}}),
    );
    assert_eq!(no_id["ok"], json!(false), "{no_id}");
    let patch = registry.call(
        &mut ctx,
        "update_layer",
        &json!({"layer_id": "L", "patch": {"background": {"r": 0, "g": 255, "b": 0, "a": 255}}}),
    );
    assert_eq!(patch["ok"], json!(false), "{patch}");
}
