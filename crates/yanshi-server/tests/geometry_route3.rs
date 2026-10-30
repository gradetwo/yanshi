//! **几何（路线三）**：六个画布几何工具的服务端集成测试。
//!
//! 每操作一测：工具调用 + 回执尺寸/比例 + `get_object` 变换断言；
//! 另测显式拒绝（非 90° 旋转、非均匀缩放未开 flag、越界 crop、
//! anchor 误用于 crop）与每操作撤销往返（`revert` 后尺寸与变换复原）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_geometry_route3", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn ok(value: &Value, what: &str) -> Value {
    assert_eq!(
        value.get("ok").and_then(Value::as_bool),
        Some(true),
        "{what} 失败：{value}"
    );
    value.clone()
}

/// 建 200×100 文档 + 一层 + 一笔 + 一段文本，返回笔触对象 id。
fn setup(workspace: &mut Workspace) -> String {
    let mut ctx = context(workspace);
    ok(
        &call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_geometry_route3", "width": 200, "height": 100}),
        ),
        "new_document",
    );
    ok(
        &call(&mut ctx, "create_layer", &json!({"layer_id": "L1"})),
        "create_layer",
    );
    ok(
        &call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L1", "data": {"points": [[20, 60], [140, 60]], "size": 20, "color": [200, 60, 40, 255]}}),
        ),
        "draw_stroke",
    );
    ok(
        &call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L1", "object_id": "t1",
                    "data": {"text": "AB", "font": "builtin", "size": 7.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [10.0, 10.0], "align": "left"}}),
        ),
        "draw_text",
    );
    let objects = call(&mut ctx, "list_objects", &json!({}));
    let stroke = objects["objects"]
        .as_array()
        .expect("list_objects 应返回数组")
        .iter()
        .find(|object| object["type"].as_str() == Some("stroke"))
        .expect("应有一笔笔触对象");
    stroke["object_id"]
        .as_str()
        .expect("笔触应有 id")
        .to_owned()
}

fn get_object(ctx: &mut ToolContext<'_>, object_id: &str) -> Value {
    call(ctx, "get_object", &json!({"object_id": object_id}))
}

fn doc_size(ctx: &mut ToolContext<'_>) -> (u64, u64) {
    let doc = call(ctx, "get_document", &json!({"preview_size": false}));
    (
        doc["width"].as_u64().expect("get_document 应有 width"),
        doc["height"].as_u64().expect("get_document 应有 height"),
    )
}

fn expect_error(value: &Value, what: &str) -> String {
    assert_eq!(
        value.get("ok").and_then(Value::as_bool),
        Some(false),
        "{what} 本该报错，却成功了：{value}"
    );
    value
        .get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[test]
fn geometry_scale_receipt_and_transform_and_undo() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let stroke_id = setup(&mut workspace);
    let mut ctx = context(&mut workspace);
    let scaled = ok(
        &call(
            &mut ctx,
            "scale_canvas",
            &json!({"scale": 2, "anchor_name": "center"}),
        ),
        "scale_canvas",
    );
    assert_eq!(scaled["width"], json!(400));
    assert_eq!(scaled["height"], json!(200));
    assert_eq!(scaled["previous_width"], json!(200));
    assert_eq!(scaled["previous_height"], json!(100));
    assert_eq!(scaled["effective_scale_x"], json!(2.0));
    assert_eq!(scaled["effective_scale_y"], json!(2.0));
    assert_eq!(scaled["anchor"], json!([100.0, 50.0]));
    // 锚点 A 的直接后果：绕中心缩放后旧画布矩形超出新画布，裁剪必须可观测。
    assert!(
        scaled["clip_note"]
            .as_str()
            .is_some_and(|note| note.contains("裁剪")),
        "绕中心缩放的裁剪提示必须在回执里：{scaled}"
    );
    // 笔触精确，文本受限标注。
    assert_eq!(
        scaled["limited_objects"]
            .as_array()
            .expect("回执应有 limited_objects")
            .iter()
            .filter(|entry| entry["object_id"].as_str() == Some("t1"))
            .count(),
        1,
        "文本对象必须在受限标注里：{scaled}"
    );
    assert_eq!(doc_size(&mut ctx), (400, 200));
    let object = get_object(&mut ctx, &stroke_id);
    // 绕中心 (100,50) 缩 2 倍：x' = 2x-100，y' = 2y-50。
    assert_eq!(
        object["transform"]["matrix"],
        json!([2.0, 0.0, 0.0, 2.0, -100.0, -50.0]),
        "笔触变换应复合缩放：{object}"
    );
    // 撤销往返：尺寸与变换都复原。
    let atom_id = scaled["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert scale_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));
    let restored = get_object(&mut ctx, &stroke_id);
    assert_eq!(
        restored["transform"]["matrix"],
        json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        "撤销后笔触变换应复原：{restored}"
    );
}

#[test]
fn geometry_rotate_90_swaps_size_and_undo() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let stroke_id = setup(&mut workspace);
    let mut ctx = context(&mut workspace);
    let rotated = ok(
        &call(
            &mut ctx,
            "rotate_canvas",
            &json!({"degrees": 90, "anchor_name": "center"}),
        ),
        "rotate_canvas",
    );
    assert_eq!(rotated["width"], json!(100));
    assert_eq!(rotated["height"], json!(200));
    assert!(
        rotated["clip_note"]
            .as_str()
            .is_some_and(|note| note.contains("裁剪")),
        "绕中心旋转的裁剪提示必须在回执里：{rotated}"
    );
    assert_eq!(doc_size(&mut ctx), (100, 200));
    let object = get_object(&mut ctx, &stroke_id);
    // 顺时针 90° 绕中心 (100,50)：x' = -(y-50)+100 = -y+150；y' = (x-100)+50 = x-50。
    assert_eq!(
        object["transform"]["matrix"],
        json!([0.0, 1.0, -1.0, 0.0, 150.0, -50.0]),
        "笔触变换应复合旋转：{object}"
    );
    let atom_id = rotated["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert rotate_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));
}

#[test]
fn geometry_flip_crop_extend_resize_each_round_trip() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let stroke_id = setup(&mut workspace);
    let mut ctx = context(&mut workspace);

    let flipped = ok(
        &call(
            &mut ctx,
            "flip_canvas",
            &json!({"direction": "horizontal", "anchor_name": "center"}),
        ),
        "flip_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));
    let object = get_object(&mut ctx, &stroke_id);
    // 左右翻绕中心 (100,50)：x' = -x+200。
    assert_eq!(
        object["transform"]["matrix"],
        json!([-1.0, 0.0, 0.0, 1.0, 200.0, 0.0]),
        "笔触变换应复合翻转：{object}"
    );
    let atom_id = flipped["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert flip_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));

    let cropped = ok(
        &call(
            &mut ctx,
            "crop_canvas",
            &json!({"region": {"x": 10, "y": 20, "w": 100, "h": 50}}),
        ),
        "crop_canvas",
    );
    assert_eq!(cropped["width"], json!(100));
    assert_eq!(cropped["height"], json!(50));
    assert_eq!(doc_size(&mut ctx), (100, 50));
    let object = get_object(&mut ctx, &stroke_id);
    assert_eq!(
        object["transform"]["matrix"],
        json!([1.0, 0.0, 0.0, 1.0, -10.0, -20.0]),
        "笔触变换应复合裁剪平移：{object}"
    );
    let atom_id = cropped["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert crop_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));

    let extended = ok(
        &call(
            &mut ctx,
            "extend_canvas",
            &json!({"margins": {"left": 5, "top": 5, "right": 5, "bottom": 5}}),
        ),
        "extend_canvas",
    );
    assert_eq!(extended["width"], json!(210));
    assert_eq!(extended["height"], json!(110));
    assert_eq!(doc_size(&mut ctx), (210, 110));
    let atom_id = extended["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert extend_canvas",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));

    let resized = ok(
        &call(
            &mut ctx,
            "resize_canvas_bounds",
            &json!({"width": 300, "height": 150}),
        ),
        "resize_canvas_bounds",
    );
    assert_eq!(resized["width"], json!(300));
    assert_eq!(resized["height"], json!(150));
    assert_eq!(doc_size(&mut ctx), (300, 150));
    let object = get_object(&mut ctx, &stroke_id);
    assert_eq!(
        object["transform"]["matrix"],
        json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),
        "改界不移动内容：{object}"
    );
    let atom_id = resized["atom_id"]
        .as_str()
        .expect("回执应有 atom_id")
        .to_owned();
    ok(
        &call(&mut ctx, "revert", &json!({"atom_id": atom_id})),
        "revert resize_canvas_bounds",
    );
    assert_eq!(doc_size(&mut ctx), (200, 100));
}

#[test]
fn geometry_rejections_are_explicit() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let _stroke_id = setup(&mut workspace);
    let mut ctx = context(&mut workspace);

    let bad_angle = call(&mut ctx, "rotate_canvas", &json!({"degrees": 45}));
    let detail = expect_error(&bad_angle, "45° 旋转");
    assert!(detail.contains("90"), "拒绝文案应点名 90° 倍数：{detail}");

    let bad_non_uniform = call(
        &mut ctx,
        "scale_canvas",
        &json!({"scale_x": 2, "scale_y": 3}),
    );
    let detail = expect_error(&bad_non_uniform, "未开 flag 的非均匀缩放");
    assert!(
        detail.contains("non_uniform"),
        "拒绝文案应点名 non_uniform：{detail}"
    );

    let bad_crop = call(
        &mut ctx,
        "crop_canvas",
        &json!({"region": {"x": 150, "y": 0, "w": 100, "h": 50}}),
    );
    expect_error(&bad_crop, "越界 crop");

    let bad_anchor = call(
        &mut ctx,
        "crop_canvas",
        &json!({"region": {"x": 0, "y": 0, "w": 100, "h": 50}, "anchor_name": "center"}),
    );
    let detail = expect_error(&bad_anchor, "crop 带 anchor");
    assert!(
        detail.contains("anchor"),
        "拒绝文案应点名 anchor 不接受：{detail}"
    );

    let bad_scale = call(&mut ctx, "scale_canvas", &json!({"scale": 0}));
    expect_error(&bad_scale, "scale 为 0");

    // 拒绝之后文档尺寸不变（没有静默落原子）。
    assert_eq!(doc_size(&mut ctx), (200, 100));
}
