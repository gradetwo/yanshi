//! 期 1 E-b 回归（墓碑复活并替换，fold 不动、工具层实现）。
//!
//! 背景：确定性 object_id（预设展开产物）撞已删除对象的墓碑时，fold 的
//! CreateObject 用 contains_key 判冲突 ⇒ 二次 apply_preset 报「对象 X
//! 已存在」整组失败。E-b 后：写入路径先 Revert 墓碑原子复活对象、再走
//! 更新整组换新参数，响应点名 revived；复活与更新同属一个 changeset、
//! 可整体撤销回墓碑态。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = context(workspace, doc);
    registry.call(&mut ctx, tool, &args)
}

/// 建 doc（黑底）+ 图层 L + 整幅填充 [160,150,140]（与 style_preset_package 同骨架）。
fn doc_with_fill(workspace: &mut Workspace, doc: &str) {
    workspace
        .create_document(
            NewDocument::new(doc, 16, 16)
                .with_background(json!({"r": 0, "g": 0, "b": 0, "a": 255})),
            "human:1",
            "session:test",
        )
        .unwrap();
    let created = call(workspace, doc, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    let filled = call(
        workspace,
        doc,
        "fill_region",
        json!({
            "layer_id": "L",
            "shape": {"type": "rect", "x": 0, "y": 0, "w": 16, "h": 16},
            "color": [160, 150, 140],
            "opacity": 1.0
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
}

fn center_pixel(workspace: &mut Workspace, doc: &str) -> Vec<i64> {
    let sample = call(workspace, doc, "sample_color", json!({"x": 8, "y": 8}));
    assert_eq!(sample["ok"], json!(true), "{sample}");
    sample["rgba"]
        .as_array()
        .expect("sample_color 应回 rgba")
        .iter()
        .map(|v| v.as_i64().expect("rgba 分量应为整数"))
        .collect()
}

fn object_params(workspace: &mut Workspace, doc: &str, object_id: &str) -> Value {
    let object = call(
        workspace,
        doc,
        "get_object",
        json!({"object_id": object_id}),
    );
    assert_eq!(object["ok"], json!(true), "{object}");
    object["data"]["params"].clone()
}

/// ccd 风格段第一笔（确定性 id，展开口径见 presets.rs 模块注）。
const TARGET: &str = "preset_ccd_s_00_brightness_contrast";

#[test]
fn apply_preset_revives_tombstoned_object() {
    // 路径甲（出事路径）：落预设 → 删一笔 → 再落 ⇒ 复活 + 换新 + 点名。
    let mut ws_a = workspace();
    doc_with_fill(&mut ws_a, "docA");
    let first = call(
        &mut ws_a,
        "docA",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 1.0}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    assert!(
        first["created_object_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == TARGET),
        "首落应建出 {TARGET}：{first}"
    );
    let deleted = call(
        &mut ws_a,
        "docA",
        "delete_object",
        json!({"object_id": TARGET}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let second = call(
        &mut ws_a,
        "docA",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 0.5}),
    );
    assert_eq!(
        second["ok"],
        json!(true),
        "墓碑 id 二次 apply 必须成功（复活路）：{second}"
    );
    let revived = second["revived_object_ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        revived.iter().any(|id| id == TARGET),
        "复活必须在响应里点名 {TARGET}：{second}"
    );
    assert!(
        !second["created_object_ids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|id| id == TARGET),
        "复活的对象不许冒充新建：{second}"
    );

    // 路径乙（从未删除）：同序列但不删 ⇒ 两路终态必须一致（参数 + 像素）。
    let mut ws_b = workspace();
    doc_with_fill(&mut ws_b, "docB");
    let b_first = call(
        &mut ws_b,
        "docB",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 1.0}),
    );
    assert_eq!(b_first["ok"], json!(true), "{b_first}");
    let b_second = call(
        &mut ws_b,
        "docB",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 0.5}),
    );
    assert_eq!(b_second["ok"], json!(true), "{b_second}");
    assert_eq!(
        object_params(&mut ws_a, "docA", TARGET),
        object_params(&mut ws_b, "docB", TARGET),
        "复活路的参数终态须与从未删除路一致"
    );
    assert_eq!(
        center_pixel(&mut ws_a, "docA"),
        center_pixel(&mut ws_b, "docB"),
        "复活路的渲染终态须与从未删除路逐像素一致"
    );
}

#[test]
fn revive_is_one_changeset_and_undoable() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc");
    let applied = call(
        &mut ws,
        "doc",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 1.0}),
    );
    assert_eq!(applied["ok"], json!(true), "{applied}");
    let full_pixel = center_pixel(&mut ws, "doc");
    let deleted = call(
        &mut ws,
        "doc",
        "delete_object",
        json!({"object_id": TARGET}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let deleted_pixel = center_pixel(&mut ws, "doc");
    assert_ne!(full_pixel, deleted_pixel, "删掉一笔后渲染必须不同（前提）");

    let begun = call(&mut ws, "doc", "begin_changeset", json!({}));
    assert_eq!(begun["ok"], json!(true), "{begun}");
    let changeset = begun["changeset_id"]
        .as_str()
        .expect("应回 changeset_id")
        .to_owned();
    let revived = call(
        &mut ws,
        "doc",
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": "L", "strength": 1.0}),
    );
    assert_eq!(revived["ok"], json!(true), "{revived}");
    let committed = call(&mut ws, "doc", "commit_changeset", json!({}));
    assert_eq!(committed["ok"], json!(true), "{committed}");
    assert_eq!(
        center_pixel(&mut ws, "doc"),
        full_pixel,
        "复活后渲染应回到完整预设态"
    );
    let reverted = call(
        &mut ws,
        "doc",
        "revert_changeset",
        json!({"changeset_id": changeset}),
    );
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    assert_eq!(
        center_pixel(&mut ws, "doc"),
        deleted_pixel,
        "整组撤销变更集后对象必须回到墓碑态（渲染与删除后一致）"
    );
}

#[test]
fn add_adjustment_revives_tombstoned_object() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc");
    let added = call(
        &mut ws,
        "doc",
        "add_adjustment",
        json!({
            "layer_id": "L",
            "adjustment_type": "brightness_contrast",
            "object_id": "adj_1",
            "params": {"brightness": 0.2, "contrast": 0.3}
        }),
    );
    assert_eq!(added["ok"], json!(true), "{added}");
    let deleted = call(
        &mut ws,
        "doc",
        "delete_object",
        json!({"object_id": "adj_1"}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let revived = call(
        &mut ws,
        "doc",
        "add_adjustment",
        json!({
            "layer_id": "L",
            "adjustment_type": "brightness_contrast",
            "object_id": "adj_1",
            "params": {"brightness": -0.25}
        }),
    );
    assert_eq!(
        revived["ok"],
        json!(true),
        "同 id 重建必须走复活路成功：{revived}"
    );
    assert_eq!(
        revived["revived"],
        json!(true),
        "写入路径响应必须以 revived 点名该对象：{revived}"
    );
    // 整组换新（不是合并）：旧参数里的 contrast 不许残留。
    let params = object_params(&mut ws, "doc", "adj_1");
    assert_eq!(params["brightness"], json!(-0.25), "参数应为新值：{params}");
    assert!(
        params.get("contrast").is_none(),
        "复活换新是整组替换，旧键 contrast 不许残留（合并语义）：{params}"
    );
}
