//! Y-08 期 4：`reorder_objects` 工具层 + 渲染层验收（R9–R12）。
//!
//! 构造与 `photo_adjust_package.rs` 同源：16×16 黑底文档 + `L` 图层整幅
//! 低饱和填充 [160,150,140]，其上两条调整（`invert` 与
//! `brightness_contrast{brightness:0.1}`）——两者不交换（先反色再提亮
//! 与先提亮再反色差两倍提亮量），是 R9 像素判据的最小对子。

use serde_json::{json, Value};
use yanshi_core::{AtomKind, Bbox};
use yanshi_render::dirty::plan_dirty;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = context(workspace, doc);
    registry().call(&mut ctx, tool, &args)
}

/// 建 doc（黑底）+ 图层 L + 整幅填充 [160,150,140]。
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

fn add_adjustment(
    workspace: &mut Workspace,
    doc: &str,
    object_id: &str,
    name: &str,
    params: Value,
) -> Value {
    call(
        workspace,
        doc,
        "add_adjustment",
        json!({"layer_id": "L", "object_id": object_id, "adjustment_type": name, "params": params}),
    )
}

/// L 图层内对象 id（自底向上）+ 填充对象 id。
fn layer_object_ids(workspace: &mut Workspace, doc: &str) -> Vec<String> {
    workspace
        .document(doc)
        .expect("文档应存在")
        .state()
        .objects_in_layer("L")
        .iter()
        .map(|object| object.id.clone())
        .collect()
}

fn render(workspace: &mut Workspace, doc: &str) -> Vec<u8> {
    let (_, _, pixels) = workspace
        .render_region_raw(doc, Bbox::new(0.0, 0.0, 16.0, 16.0))
        .expect("应能渲染");
    pixels
}

fn head_seq(workspace: &mut Workspace, doc: &str) -> u64 {
    workspace
        .document(doc)
        .expect("文档应存在")
        .state()
        .head_seq
}

/// R9（硬判据）：两条调整交换顺序前后渲染像素不同；重排后渲染等于
/// 一开始就按新顺序创建的渲染。只测 z_index 数字不算过。
#[test]
fn reorder_adjustments_changes_pixels() {
    // A 顺序：invert 在下，brightness_contrast 在上。
    let mut ws_a = workspace();
    doc_with_fill(&mut ws_a, "doc_a");
    let inv = add_adjustment(&mut ws_a, "doc_a", "adj_inv", "invert", json!({}));
    assert_eq!(inv["ok"], json!(true), "{inv}");
    let bc = add_adjustment(
        &mut ws_a,
        "doc_a",
        "adj_bc",
        "brightness_contrast",
        json!({"brightness": 0.1}),
    );
    assert_eq!(bc["ok"], json!(true), "{bc}");
    let ids_a = layer_object_ids(&mut ws_a, "doc_a");
    assert_eq!(ids_a.len(), 3, "L 层应有填充 + 2 调整，实得 {ids_a:?}");
    let fill_id = ids_a[0].clone();
    let pixels_a = render(&mut ws_a, "doc_a");

    // B 顺序：建的时候就反过来。
    let mut ws_b = workspace();
    doc_with_fill(&mut ws_b, "doc_b");
    let bc = add_adjustment(
        &mut ws_b,
        "doc_b",
        "adj_bc",
        "brightness_contrast",
        json!({"brightness": 0.1}),
    );
    assert_eq!(bc["ok"], json!(true), "{bc}");
    let inv = add_adjustment(&mut ws_b, "doc_b", "adj_inv", "invert", json!({}));
    assert_eq!(inv["ok"], json!(true), "{inv}");
    let pixels_b = render(&mut ws_b, "doc_b");
    assert_ne!(
        pixels_a, pixels_b,
        "invert 与 brightness_contrast 交换顺序，渲染像素必须不同（相同 ⇒ 顺序没进渲染）"
    );

    // A 重排成 B 顺序后，渲染必须等于 B。
    let reordered = call(
        &mut ws_a,
        "doc_a",
        "reorder_objects",
        json!({"layer_id": "L", "order": [fill_id, "adj_bc", "adj_inv"]}),
    );
    assert_eq!(reordered["ok"], json!(true), "{reordered}");
    assert_eq!(
        layer_object_ids(&mut ws_a, "doc_a"),
        vec![fill_id, "adj_bc".to_owned(), "adj_inv".to_owned()],
        "重排后对象顺序须等于提交的 order"
    );
    let pixels_a2 = render(&mut ws_a, "doc_a");
    assert_eq!(
        pixels_a2, pixels_b,
        "重排后的渲染必须等于一开始就按新顺序创建的渲染（逐像素）"
    );
}

/// R8（工具层）：重排后新建对象 ⇒ 缺省 z_index = max+1，落在栈顶。
/// （max+1 是工具层 `write_add_effect` 的行为，fold 层缺省 z 是 0，
/// 见 `fold.rs` 的 `reorder_objects_new_object_without_z_index_defaults_to_zero_at_fold_level`。）
#[test]
fn reorder_objects_new_object_lands_on_top() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_new");
    let inv = add_adjustment(&mut ws, "doc_new", "adj_inv", "invert", json!({}));
    assert_eq!(inv["ok"], json!(true), "{inv}");
    let bc = add_adjustment(
        &mut ws,
        "doc_new",
        "adj_bc",
        "brightness_contrast",
        json!({"brightness": 0.1}),
    );
    assert_eq!(bc["ok"], json!(true), "{bc}");
    let ids = layer_object_ids(&mut ws, "doc_new");
    let fill_id = ids[0].clone();
    let reordered = call(
        &mut ws,
        "doc_new",
        "reorder_objects",
        json!({"layer_id": "L", "order": [fill_id, "adj_bc", "adj_inv"]}),
    );
    assert_eq!(reordered["ok"], json!(true), "{reordered}");
    let added = add_adjustment(
        &mut ws,
        "doc_new",
        "adj_new",
        "saturation",
        json!({"amount": 1.0}),
    );
    assert_eq!(added["ok"], json!(true), "{added}");
    let ids_after = layer_object_ids(&mut ws, "doc_new");
    assert_eq!(
        ids_after.last().map(String::as_str),
        Some("adj_new"),
        "重排后新建对象须落在栈顶，实得 {ids_after:?}"
    );
    let z_new = ws
        .document("doc_new")
        .expect("文档应存在")
        .state()
        .objects
        .get("adj_new")
        .expect("新对象应存在")
        .z_index;
    assert_eq!(z_new, 3, "新对象缺省 z 应为 max+1（重排后 max=2）");
}

/// R10：重排的脏集是整文档；重排后渲染真的更新（不许"排了但画面不动"）。
#[test]
fn reorder_objects_dirties_whole_document() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_dirty");
    let inv = add_adjustment(&mut ws, "doc_dirty", "adj_inv", "invert", json!({}));
    assert_eq!(inv["ok"], json!(true), "{inv}");
    let bc = add_adjustment(
        &mut ws,
        "doc_dirty",
        "adj_bc",
        "brightness_contrast",
        json!({"brightness": 0.1}),
    );
    assert_eq!(bc["ok"], json!(true), "{bc}");
    let before_pixels = render(&mut ws, "doc_dirty");
    let state_before = ws
        .document("doc_dirty")
        .expect("文档应存在")
        .state()
        .clone();
    let ids = layer_object_ids(&mut ws, "doc_dirty");
    let fill_id = ids[0].clone();
    let reordered = call(
        &mut ws,
        "doc_dirty",
        "reorder_objects",
        json!({"layer_id": "L", "order": [fill_id, "adj_bc", "adj_inv"]}),
    );
    assert_eq!(reordered["ok"], json!(true), "{reordered}");
    // 脏集：取日志里真正落成的重排原子，plan_dirty 必须是整幅。
    let state_after = ws
        .document("doc_dirty")
        .expect("文档应存在")
        .state()
        .clone();
    let atom = ws
        .document("doc_dirty")
        .expect("文档应存在")
        .log_since(0)
        .into_iter()
        .rev()
        .find(|atom| atom.kind == AtomKind::ReorderObjects)
        .expect("日志里应有重排原子")
        .clone();
    let dirty = plan_dirty(&state_after, Some(&state_before), &atom);
    assert!(
        dirty.is_full(),
        "重排的脏集必须是整文档（v1 有意保守），实得 kind={:?}",
        dirty.kind
    );
    // 画面更新：重排后渲染必须变。
    let after_pixels = render(&mut ws, "doc_dirty");
    assert_ne!(
        before_pixels, after_pixels,
        "重排后画面必须更新（像素不变 ⇒ 脏集/缓存没跟上）"
    );
}

/// R11：工具层传缺失一员的 order ⇒ 友好报错（期望 N 个、实际 M 个 +
/// 缺失 id 清单），不提交原子。
#[test]
fn reorder_objects_tool_rejects_partial_order() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_tool");
    for (index, id) in ["s1", "s2", "s3"].iter().enumerate() {
        let shape = call(
            &mut ws,
            "doc_tool",
            "draw_shape",
            json!({"layer_id": "L", "object_id": id, "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }}),
        );
        assert_eq!(shape["ok"], json!(true), "{shape}");
        let _ = index;
    }
    let before_seq = head_seq(&mut ws, "doc_tool");
    let result = call(
        &mut ws,
        "doc_tool",
        "reorder_objects",
        json!({"layer_id": "L", "order": ["s1", "s2"]}),
    );
    assert_eq!(
        result.get("ok").and_then(Value::as_bool),
        Some(false),
        "缺一员的 order 必须被工具层拒绝 ⇒ {result}"
    );
    let detail = result["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        detail.contains("期望 4 个") && detail.contains("实际 2 个"),
        "报错须写期望/实际个数，实得：{detail}"
    );
    assert!(
        detail.contains("s3"),
        "报错须列出缺失的 id（s3），实得：{detail}"
    );
    assert_eq!(
        head_seq(&mut ws, "doc_tool"),
        before_seq,
        "被拒的调用不得提交原子（head_seq 不变）"
    );
}

/// R12：空图层调工具 ⇒ 拒绝，不产生空原子。
#[test]
fn reorder_objects_tool_rejects_empty_layer() {
    let mut ws = workspace();
    doc_with_fill(&mut ws, "doc_empty");
    let created = call(
        &mut ws,
        "doc_empty",
        "create_layer",
        json!({"layer_id": "E"}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let before_seq = head_seq(&mut ws, "doc_empty");
    let result = call(
        &mut ws,
        "doc_empty",
        "reorder_objects",
        json!({"layer_id": "E", "order": []}),
    );
    assert_eq!(
        result.get("ok").and_then(Value::as_bool),
        Some(false),
        "空图层调用必须被拒绝 ⇒ {result}"
    );
    let detail = result["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        detail.contains("没有对象"),
        "报错须点名空图层无可重排，实得：{detail}"
    );
    assert_eq!(
        head_seq(&mut ws, "doc_empty"),
        before_seq,
        "空图层调用不得产生空原子（head_seq 不变）"
    );
}
