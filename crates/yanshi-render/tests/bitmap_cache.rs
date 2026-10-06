//! **解码位图缓存不得改变像素** ✓（性能修复的正确性判据 ✓）。
//!
//! 背景：一份带位图补丁（`raster_patch` ✓）的文档，在**每一次**渲染时都会
//! `store.get` ＋ 解压相交的每个补丁 ✓ ⇒ 一张覆盖整幅画布的背景补丁每笔都被
//! 完整解压一遍 ✓（外部性能报告里那个恒定 ~11 s ✓）。修复是**跨渲染缓存已解码的
//! 字节** ✓（`Renderer::bitmap_cache_stats` ✓）—— 而缓存最怕的是"给出与冷路径
//! 不同的像素" ✗（内容寻址让这不可能 ✓，但**判据要自己说出来** ✓，不靠推理 ✓）。
//!
//! **判据（能红 ✓）**：同一个区域
//! ① 冷缓存渲染 ✓、② 另一块先渲染把缓存捂热再渲染同一块 ✓、
//! ③ 换一个全新渲染器（冷缓存）渲染同一块 ✓
//! —— 三者必须**逐字节相同** ✓。
//!
//! **变异（让它红）** ✗：把缓存里存下的字节改一个（例如解压后把某个像素 +1 ✓）
//! ⇒ ②与①③立刻不同 ⇒ 本条红 ✓。

use serde_json::json;
use yanshi_core::blob::{stage_blob, MemoryBlobStore};
use yanshi_core::{Bbox, DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::render::RAW_RGBA_MIME;
use yanshi_render::{Renderer, TileGrid};

fn layer(id: &str) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: 0,
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

/// 一张覆盖整幅画布的补丁（背景的等价物 ✓）：成本 ∝ 画布面积 ✓，与请求区域无关 ✓。
fn background_document(side: u32) -> (DocumentState, MemoryBlobStore) {
    let store = MemoryBlobStore::new();
    let pixels: Vec<u8> = (0..(side * side))
        .flat_map(|index| {
            let value = (index % 251) as u8;
            [value, value.wrapping_mul(3), value.wrapping_mul(7), 255]
        })
        .collect();
    let blob = stage_blob(&store, &pixels, RAW_RGBA_MIME).unwrap();
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_bg".to_owned());
    state.width = side;
    state.height = side;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state
        .layers
        .insert("layer_bg".to_owned(), layer("layer_bg"));
    state.objects.insert(
        "obj_bg".to_owned(),
        Object {
            id: "obj_bg".to_owned(),
            layer_id: "layer_bg".to_owned(),
            object_type: ObjectType::RasterPatch,
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
                "bitmap": blob,
                "width": side,
                "height": side,
                "region": {"x": 0, "y": 0, "w": side, "h": side},
            }),
            blobs: Vec::new(),
        },
    );
    (state, store)
}

#[test]
fn the_decoded_bitmap_cache_never_changes_pixels() {
    let side = 256u32;
    let (state, store) = background_document(side);
    let grid = TileGrid::new(side, 64, 64).unwrap();
    // **整幅**比较 ✓：缓存里任何一个字节被改坏都必须被抓住 ✓
    //（只比一小块的话，坏在块外的变异会静默通过 ✗ —— 本条第一版正是这样漏过一次 ✓）。
    let whole = Bbox::new(0.0, 0.0, side as f64, side as f64);

    // ① 冷缓存：同一台渲染器第一次渲染整幅。
    //
    // **强制串行** ✗：并行分块下，"第一次渲染"里会**同时**发生未命中与命中 ✓（各块抢锁 ✓）
    // ⇒ 冷/热两条路径在一次渲染里就混在一起 ✓ ⇒ 变异会变得不可判定 ✗。
    // 串行 ⇒ 冷渲染是纯未命中（只用刚解出来的字节 ✓）、热渲染是纯命中 ✓，判据才干净 ✓。
    let mut reused = Renderer::new(grid.clone()).with_max_workers(1);
    let cold = reused.render_region(&state, &store, whole).unwrap().rgba8;
    assert_eq!(
        reused.bitmap_cache_stats().misses,
        1,
        "第一次渲染应当只解压一个补丁"
    );

    // ② 先渲染一小块（同一张背景补丁 ⇒ 必须命中缓存 ✓），再渲染整幅。
    let elsewhere = Bbox::new(120.0, 140.0, 16.0, 16.0);
    let _ = reused.render_region(&state, &store, elsewhere).unwrap();
    assert_eq!(
        reused.bitmap_cache_stats().misses,
        1,
        "第二块用的是同一张补丁 ⇒ 不该再解压一次"
    );
    assert!(
        reused.bitmap_cache_stats().hits >= 1,
        "第二块应当命中缓存：{:?}",
        reused.bitmap_cache_stats()
    );
    let warm = reused.render_region(&state, &store, whole).unwrap().rgba8;

    // ③ 全新渲染器（冷缓存）渲染整幅。
    let mut fresh = Renderer::new(grid).with_max_workers(1);
    let fresh_bytes = fresh.render_region(&state, &store, whole).unwrap().rgba8;

    assert_eq!(cold.len(), warm.len());
    assert_eq!(
        cold, warm,
        "冷缓存与热缓存必须逐字节相同（缓存只搬字节，不改字节）"
    );
    assert_eq!(cold, fresh_bytes, "热缓存渲染器与全新渲染器必须逐字节相同");
    assert_eq!(fresh.bitmap_cache_stats().misses, 1);
}
