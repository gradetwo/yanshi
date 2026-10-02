//! **按点压力真的会改变渲染** ✓ —— 这是"面条线"问题的答案 ✓。
//!
//! **背景** ✓（真实用户报的 §四-12 ✓）：`draw_stroke` 的 `data` 说明里只写了
//! `{points,size,color,hardness,opacity,seed}` ✗ ⇒ 看起来**没有**提按
//! ⇒ agent 只好画**均匀粗细**的"面条线" ✗、油画的厚薄与水彩的枯湿都做不出来 ✗。
//! **而内核其实早就支持** ✓（`StrokeGeometry::from_value` ✓：`pair.get(2)` 就是压力 ✓，
//! 也接受 `{x,y,pressure}` 对象 ✓）⇒ **缺的是把它写出来** ✗，不是能力 ✗。
//! 本测试把它**钉在代码里** ✓：以后谁把压力弄丢，这里会红 ✓。

use serde_json::json;
use yanshi_core::{
    DocumentState, Layer, LayerType, MemoryBlobStore, Object, ObjectType, Transform,
};
use yanshi_render::brush::StrokeGeometry;
use yanshi_render::render::Renderer;
use yanshi_render::tile::TileGrid;

fn stroke_object(points: serde_json::Value, size: f64) -> Object {
    Object {
        id: "s1".to_owned(),
        layer_id: "L".to_owned(),
        object_type: ObjectType::Stroke,
        z_index: 0,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec!["a1".to_owned()],
        current_version: Some("a1".to_owned()),
        created_by: "a1".to_owned(),
        deleted_by: None,
        data: json!({
            "points": points,
            "size": size,
            "color": [0.9, 0.2, 0.15, 1.0],
            // **关掉平滑** ✓ ⇒ 两种写法之间唯一的差别就是压力 ✓。
            "smooth": false,
        }),
        blobs: Vec::new(),
    }
}

fn layer(id: &str, z: i64) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: z,
        blend_mode: "normal".to_owned(),
        opacity: 1.0,
        visible: true,
        locked: false,
        alpha_lock: false,
        clipping_mask: false,
        mask_id: None,
        transform: Transform::IDENTITY,
        medium: None,
        style: None,
        metadata: serde_json::Value::Null,
        blobs: Vec::new(),
        created_by: "a1".to_owned(),
        updated_by: None,
        deleted_by: None,
    }
}

fn render_with(object: Object) -> Vec<u8> {
    let store = MemoryBlobStore::new();
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_pressure".to_owned());
    state.width = 256;
    state.height = 256;
    state.background = json!({"r": 0, "g": 0, "b": 0, "a": 0});
    state.layers.insert("L".to_owned(), layer("L", 0));
    state.objects.insert("s1".to_owned(), object);
    let grid = TileGrid::new(64, state.width, state.height).unwrap();
    let mut renderer = Renderer::new(grid);
    renderer.render_document(&state, &store).unwrap().rgba8
}

fn opaque(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count()
}

fn pressures(geometry: &StrokeGeometry) -> Vec<f64> {
    geometry.points.iter().map(|point| point.pressure).collect()
}

/// **同一根线：带压力 ⇒ 上墨更少** ✓（细笔尖 ⇒ 覆盖面积更小 ✓）。
#[test]
fn per_point_pressure_thins_the_stroke() {
    let full = stroke_object(
        json!([[20.0, 60.0], [60.0, 60.0], [100.0, 60.0], [140.0, 60.0]]),
        24.0,
    );
    let ramped = stroke_object(
        json!([
            [20.0, 60.0, 1.0],
            [60.0, 60.0, 0.5],
            [100.0, 60.0, 0.2],
            [140.0, 60.0, 0.0]
        ]),
        24.0,
    );
    let full_ink = opaque(&render_with(full));
    let ramped_ink = opaque(&render_with(ramped));
    assert!(full_ink > 0, "全压力应当有墨（实测 {full_ink}）");
    assert!(
        ramped_ink < full_ink,
        "带压力的笔应当更细 ⇒ 墨量应更少（全 {full_ink} vs 带压力 {ramped_ink}）"
    );
    // **不能细到消失** ✓：压力 0 时仍有 0.45 倍下限 ✓。
    assert!(
        ramped_ink * 3 > full_ink,
        "最轻也不该消失（实测 {ramped_ink} vs 全 {full_ink}）"
    );
}

/// **两种写法都认** ✓：`[x, y, pressure]` 与 `{x, y, pressure}` ✓。
#[test]
fn both_pressure_spellings_are_accepted() {
    let from_array = StrokeGeometry::from_value(&json!({
        "points": [[0.0, 0.0, 0.25], [30.0, 0.0, 0.75]]
    }))
    .expect("数组写法应当能解析");
    let from_objects = StrokeGeometry::from_value(&json!({
        "points": [{"x": 0.0, "y": 0.0, "pressure": 0.25}, {"x": 30.0, "y": 0.0, "pressure": 0.75}]
    }))
    .expect("对象写法应当能解析");
    assert_eq!(pressures(&from_array), vec![0.25, 0.75]);
    assert_eq!(pressures(&from_objects), vec![0.25, 0.75]);
}

/// **不写压力时缺省为满** ✓（老文档观感不能变 ✗）。
#[test]
fn omitting_pressure_defaults_to_full() {
    let geometry = StrokeGeometry::from_value(&json!({"points": [[0.0, 0.0], [10.0, 0.0]]}))
        .expect("应当能解析");
    assert_eq!(pressures(&geometry), vec![1.0, 1.0], "缺省压力必须是 1.0");
}
