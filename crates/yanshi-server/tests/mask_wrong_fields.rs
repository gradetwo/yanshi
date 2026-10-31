//! **`create_mask` 错字段名必须当场报错，而不是 `ok=true` 配一个不是调用方要的蒙版**
//! （掩膜静默空覆盖稳健性缺陷，LOW，v0.3.2 范围）。
//!
//! **机制**：图层蒙版渲染（`render.rs parse_shape_coverage`）只认两组写法——
//! `bbox` 对象（`{x, y, w|width, h|height}`）或顶层 `x/y/w/h`，读不到就退回
//! 零面积矩形（`Bbox::new(0, 0, 0, 0)`）⇒ 覆盖全 0 ⇒ 整层被藏住。
//! - 角点式 `x0/y0/x1/y1`：不被读 ⇒ 全 0（字面零覆盖，盒上探针实测）；
//! - SVG 椭圆式 `cx/cy/rx/ry`：选区路径会归一成 bbox，但蒙版入口没有归一
//!   ⇒ 同样全 0；另注意选区/CompiledMask 路径对同一形状会退化为覆盖全部
//!   （两路径口径不一致，入口校验把两门一并关上）；
//! - 零/负宽高：`has_area` 为假 ⇒ 退化为空。
//!
//! 三者此前在入口全不校验，`create_mask` 一律 `ok=true`。
//! **修法**：入口具名报错（点名未被采纳的字段／缺失的字段／非正宽高），
//! 渲染层 `parse_geom` 与选区路径不动；`from_selection` 克隆来的形状不验
//! （选区合法形态，那条门的归一缺口另案处理）。
//! **已确认不碰的既有行为**：无 shape ⇒ 报错；`kind: "bbox"` ⇒ 报错（白名单拒）。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_mask_fields", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn expect_invalid(ctx: &mut ToolContext<'_>, args: &Value) -> String {
    let response = call(ctx, "create_mask", args);
    assert_eq!(
        response["ok"],
        json!(false),
        "错字段调用必须报错，不能 `ok=true`：{response}"
    );
    assert_eq!(
        response["error_code"],
        json!("invalid_argument"),
        "错字段调用的错误码必须是 invalid_argument：{response}"
    );
    response
        .get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| response.to_string())
}

fn fresh_doc() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(NewDocument::new("doc_mask_fields", 128, 64), "human:1", "s")
        .expect("create_document");
    workspace
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_mask_fields", Bbox::new(0.0, 0.0, 128.0, 64.0))
        .expect("渲染应成功")
        .2
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
    let index = (y * 128 + x) * 4;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

/// 横贯深色笔触 + 左半蒙版的场景（沿用 mask_paint.rs 的搭法）。
fn fresh_with_stroke_and_mask(shape: Value) -> (Workspace, Value) {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let created_layer = call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(created_layer["ok"], json!(true), "{created_layer}");
    let drawn = call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "L",
            "object_id": "s1",
            "data": {
                "points": [[8.0, 32.0], [120.0, 32.0]],
                "size": 12.0,
                "hardness": 1.0,
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}
            }
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let created = call(
        &mut ctx,
        "create_mask",
        &json!({"mask_id": "m1", "shape": shape}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let attached = call(
        &mut ctx,
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "m1"}),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
    (workspace, created)
}

#[test]
fn rect_with_corner_fields_is_refused_naming_them() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let detail = expect_invalid(
        &mut ctx,
        &json!({"mask_id": "m_bad",
                 "shape": {"kind": "rect", "x0": 0, "y0": 0, "x1": 64, "y1": 64}}),
    );
    assert!(
        detail.contains("x0") && detail.contains("x1"),
        "错误必须点名未被采纳的角点字段：{detail}"
    );
}

#[test]
fn ellipse_with_svg_center_fields_is_refused_naming_them() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let detail = expect_invalid(
        &mut ctx,
        &json!({"mask_id": "m_bad",
                 "shape": {"kind": "ellipse", "cx": 32, "cy": 32, "rx": 32, "ry": 32}}),
    );
    assert!(
        detail.contains("cx") && detail.contains("rx"),
        "错误必须点名未被采纳的 SVG 椭圆字段：{detail}"
    );
}

#[test]
fn rect_with_corner_fields_inside_bbox_is_refused() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let detail = expect_invalid(
        &mut ctx,
        &json!({"mask_id": "m_bad",
                 "shape": {"kind": "rect", "bbox": {"x0": 0, "y0": 0, "x1": 64, "y1": 64}}}),
    );
    assert!(
        detail.contains("x0"),
        "bbox 对象内的角点字段同样必须点名：{detail}"
    );
}

#[test]
fn bare_rect_without_geometry_is_refused_naming_missing_fields() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let detail = expect_invalid(
        &mut ctx,
        &json!({"mask_id": "m_bad", "shape": {"kind": "rect"}}),
    );
    assert!(
        detail.contains('x') && detail.contains('w'),
        "错误必须点名缺失的几何字段：{detail}"
    );
}

#[test]
fn zero_area_rect_is_refused_not_silently_empty() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let detail = expect_invalid(
        &mut ctx,
        &json!({"mask_id": "m_bad",
                 "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 0, "h": 64}}}),
    );
    assert!(
        detail.contains("大于 0"),
        "零面积必须报错为宽高非正，不能静默成空：{detail}"
    );
}

#[test]
fn correct_rect_bbox_still_works_and_clips() {
    let (mut workspace, _) = fresh_with_stroke_and_mask(
        json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}}),
    );
    let data = render(&mut workspace);
    assert!(
        pixel(&data, 16, 32)[0] < 120,
        "蒙版内应是深色笔触：{:?}",
        pixel(&data, 16, 32)
    );
    assert!(
        pixel(&data, 96, 32)[0] > 200,
        "蒙版外应是背景：{:?}",
        pixel(&data, 96, 32)
    );
}

#[test]
fn correct_rect_flat_fields_still_work() {
    let (mut workspace, _) =
        fresh_with_stroke_and_mask(json!({"kind": "rect", "x": 0, "y": 0, "w": 64, "h": 64}));
    let data = render(&mut workspace);
    assert!(
        pixel(&data, 16, 32)[0] < 120,
        "蒙版内应是深色笔触：{:?}",
        pixel(&data, 16, 32)
    );
    assert!(
        pixel(&data, 96, 32)[0] > 200,
        "蒙版外应是背景：{:?}",
        pixel(&data, 96, 32)
    );
}

#[test]
fn correct_ellipse_bbox_still_works() {
    let (mut workspace, _) = fresh_with_stroke_and_mask(
        json!({"kind": "ellipse", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}}),
    );
    let data = render(&mut workspace);
    assert!(
        pixel(&data, 32, 32)[0] < 120,
        "椭圆中心应是深色笔触：{:?}",
        pixel(&data, 32, 32)
    );
    assert!(
        pixel(&data, 96, 32)[0] > 200,
        "椭圆外应是背景：{:?}",
        pixel(&data, 96, 32)
    );
}

#[test]
fn already_visible_failures_stay_visible() {
    let mut workspace = fresh_doc();
    let mut ctx = context(&mut workspace);
    let no_shape = call(&mut ctx, "create_mask", &json!({"mask_id": "m_none"}));
    assert_eq!(
        no_shape["ok"],
        json!(false),
        "无 shape 必须报错：{no_shape}"
    );
    let bad_kind = call(
        &mut ctx,
        "create_mask",
        &json!({"mask_id": "m_bad", "shape": {"kind": "bbox", "x": 0, "y": 0, "w": 10, "h": 10}}),
    );
    assert_eq!(
        bad_kind["ok"],
        json!(false),
        "kind=bbox 必须被白名单拒：{bad_kind}"
    );
}

/// 渲染层读 bbox 的口径是「对象四元齐全才用它，否则回退顶层」
/// （`parse_shape_coverage` 的 `Bbox::from_value(bbox).or_else(top-level)`）；
/// 入口校验必须同口径，否则会误拒一个渲染层本来认得的顶层写法。
#[test]
fn incomplete_bbox_object_falls_back_to_flat_fields_like_the_renderer() {
    let (mut workspace, _) = fresh_with_stroke_and_mask(json!({
        "kind": "rect",
        "bbox": {"x": 0, "y": 0, "w": 64},
        "x": 0,
        "y": 0,
        "w": 64,
        "h": 64
    }));
    let data = render(&mut workspace);
    assert!(
        pixel(&data, 16, 32)[0] < 120,
        "顶层几何应生效（蒙版内是深色笔触）：{:?}",
        pixel(&data, 16, 32)
    );
    assert!(
        pixel(&data, 96, 32)[0] > 200,
        "顶层几何应生效（蒙版外是背景）：{:?}",
        pixel(&data, 96, 32)
    );
}
