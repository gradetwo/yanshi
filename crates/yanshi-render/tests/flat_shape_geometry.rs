//! **渲染内核也认扁平几何** ✓（真实用户第二次报告 ✓）。
//!
//! **为什么内核层也要单独测** ✓：工具层会把扁平写法归一化成 `bbox` ✓ ⇒ 只测工具层的话 ✓，
//! **内核那条路仍然是坏的** ✗ —— 而任何绕过工具层的文档（页面上直接改 ✓、手工编辑原子日志 ✓）
//! 都会**画不出来却看起来一切正常** ✗ ⇒ 那正是最初 P0 的病根 ✓。
//! **"校验通过的写法一定能画"必须全局成立** ✓，不能只在某一个入口成立 ✗。

use serde_json::json;
use yanshi_core::{
    DocumentState, Layer, LayerType, MemoryBlobStore, Object, ObjectType, Transform,
};
use yanshi_render::render::Renderer;
use yanshi_render::tile::TileGrid;

fn object_with_flat_geometry() -> Object {
    Object {
        id: "flat".to_owned(),
        layer_id: "L".to_owned(),
        object_type: ObjectType::Shape,
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
        // **扁平写法** ✓：`x`/`y`/`w`/`h` 直接写在 geometry 上 ✓（没有 `bbox` ✓）。
        data: json!({
            "geometry": {"kind": "rect", "x": 20, "y": 30, "w": 60, "h": 40},
            "color": {"r": 0, "g": 0, "b": 0, "a": 255},
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

/// **扁平几何真的渲染出像素** ✓（不是"解析成功但一片空白" ✗）。
#[test]
fn a_flat_geometry_shape_renders_pixels() {
    let store = MemoryBlobStore::new();
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_flat".to_owned());
    state.width = 200;
    state.height = 200;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state.layers.insert("L".to_owned(), layer("L", 0));
    state
        .objects
        .insert("flat".to_owned(), object_with_flat_geometry());
    let grid = TileGrid::new(64, 200, 200).unwrap();
    let mut renderer = Renderer::new(grid);
    let rgba = renderer.render_document(&state, &store).unwrap().rgba8;
    // 数**暗像素** ✓（背景是不透明白 ✓ ⇒ 只数 alpha 会把整幅都算上 ✗，这个坑我踩过 ✓）。
    let inked = rgba
        .chunks_exact(4)
        .filter(|pixel| {
            (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                / 1000
                < 200
        })
        .count();
    assert!(
        inked > 500,
        "扁平几何应当渲染出矩形（60×40 = 2400 像素 ⇒ 实测只有 {inked} 个暗像素）\
         —— 若为 0 就说明内核仍然不认这种写法 ✗"
    );
}
