//! **原生矢量图层的单笔光栅缓存（路线甲 Phase 2）**：vector 图层的笔触
//! 第二遍渲染只合成不重盖章、新增一笔只盖那一笔、缓存开/关像素一致 ✓。
//!
//! 证据面是渲染器暴露的计数（`vector_raster_stats`）：命中/未命中/实际
//! 盖章 dab 数——断的全是**结构量与恒等式**，不断具体像素数 ✓。

use serde_json::json;
use yanshi_core::{
    DocumentState, Layer, LayerType, MemoryBlobStore, Object, ObjectType, Transform,
};
use yanshi_render::brush::{BrushSpec, StrokeGeometry};
use yanshi_render::render::Renderer;
use yanshi_render::svg;
use yanshi_render::tile::TileGrid;

fn layer(id: &str, layer_type: LayerType) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type,
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

fn stroke_object(id: &str, seq: u64, points: serde_json::Value, size: f64) -> Object {
    Object {
        seq,
        id: id.to_owned(),
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
            "color": [200, 60, 40, 255],
            "hardness": 1.0,
            "opacity": 1.0,
            "smooth": false,
        }),
        blobs: Vec::new(),
    }
}

fn state_with(layer_type: LayerType, objects: Vec<Object>) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_vector".to_owned());
    state.width = 256;
    state.height = 256;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state.layers.insert("L".to_owned(), layer("L", layer_type));
    for object in objects {
        state.objects.insert(object.id.clone(), object);
    }
    state
}

fn new_renderer() -> Renderer {
    Renderer::new(TileGrid::new(64, 256, 256).unwrap())
}

/// 一笔的 dab 数（按导出侧的 dab 序列复刻口径，与缓存记账同一口径 ✓）。
fn dab_count(object: &Object) -> usize {
    let brush = BrushSpec::from_value(&object.data);
    let geometry = StrokeGeometry::from_value(&object.data).expect("几何应可解析");
    svg::stroke_dab_clusters(&brush, &geometry, (256, 256))
        .iter()
        .map(Vec::len)
        .sum()
}

#[test]
fn vector_layer_raster_cache_end_to_end() {
    let store = MemoryBlobStore::new();
    let s1 = stroke_object("s1", 1, json!([[20.0, 60.0], [140.0, 60.0]]), 20.0);
    let s2 = stroke_object("s2", 2, json!([[20.0, 140.0], [140.0, 140.0]]), 20.0);
    let dabs_s1 = dab_count(&s1);
    let dabs_s2 = dab_count(&s2);
    assert!(dabs_s1 > 0 && dabs_s2 > 0);
    let mut state = state_with(LayerType::Vector, vec![s1, s2]);

    // 对照组：同样的笔放在 raster 图层 ⇒ 缓存完全不介入（计数恒 0 ✓）。
    let raster_state = state_with(
        LayerType::Raster,
        vec![
            stroke_object("s1", 1, json!([[20.0, 60.0], [140.0, 60.0]]), 20.0),
            stroke_object("s2", 2, json!([[20.0, 140.0], [140.0, 140.0]]), 20.0),
        ],
    );
    let mut raster_renderer = new_renderer();
    let raster_rgba = raster_renderer
        .render_document(&raster_state, &store)
        .unwrap()
        .rgba8;
    assert_eq!(raster_renderer.vector_raster_stats().misses, 0);
    assert_eq!(raster_renderer.vector_raster_stats().dabs_stamped, 0);

    // 第一遍：两笔各未命中一次、盖章数 = 两笔 dab 数之和 ✓；
    // 且像素与 raster 图层的原路径**逐字节一致**（缓存不改语义 ✓）。
    let mut renderer = new_renderer();
    let first = renderer.render_document(&state, &store).unwrap().rgba8;
    assert_eq!(first, raster_rgba, "缓存路径与原路径像素必须一致");
    let stats1 = renderer.vector_raster_stats();
    assert_eq!(stats1.misses, 2, "两笔应各光栅化一次：{stats1:?}");
    assert_eq!(
        stats1.dabs_stamped,
        (dabs_s1 + dabs_s2) as u64,
        "盖章数应恰为两笔 dab 数之和：{stats1:?}"
    );
    assert_eq!(stats1.entries, 2, "两笔各一个缓存条目：{stats1:?}");

    // 第二遍（文档未变）：像素逐字节相同、盖章数与未命中数都不再增长 ✓。
    let second = renderer.render_document(&state, &store).unwrap().rgba8;
    assert_eq!(second, first, "第二遍渲染像素必须逐字节一致");
    let stats2 = renderer.vector_raster_stats();
    assert_eq!(
        stats2.dabs_stamped, stats1.dabs_stamped,
        "未变笔触不得重盖章"
    );
    assert_eq!(stats2.misses, stats1.misses, "未变笔触不得再未命中");
    assert!(
        stats2.hits >= stats1.hits + 2,
        "第二遍每笔至少命中一次：{stats1:?} → {stats2:?}"
    );

    // 新增一笔：只有新笔被盖章（增量 = 新笔 dab 数，恒等式 ✓），旧笔仍冻结。
    let s3 = stroke_object("s3", 3, json!([[20.0, 210.0], [140.0, 210.0]]), 20.0);
    let dabs_s3 = dab_count(&s3);
    state.objects.insert("s3".to_owned(), s3);
    let third = renderer.render_document(&state, &store).unwrap().rgba8;
    assert_ne!(third, first, "新增一笔画面必须变");
    let stats3 = renderer.vector_raster_stats();
    assert_eq!(stats3.misses, stats2.misses + 1, "只有新笔未命中");
    assert_eq!(
        stats3.dabs_stamped,
        stats2.dabs_stamped + dabs_s3 as u64,
        "盖章增量应恰为新笔的 dab 数"
    );

    // 全量失效（clear_all_caches）必须一并清掉单笔光栅缓存（仓库约定：
    // 新增缓存要登记进 clear_all_caches ✓）——条目数归零为证 ✓。
    renderer.clear_all_caches();
    assert_eq!(
        renderer.vector_raster_stats().entries,
        0,
        "clear_all_caches 后矢量光栅缓存应为空"
    );

    // A/B 开关关掉缓存（同进程、同状态、新渲染器）：像素仍逐字节一致 ✓。
    std::env::set_var("YANSHI_VECTOR_RASTER_CACHE", "0");
    let mut off_renderer = new_renderer();
    let off = off_renderer.render_document(&state, &store).unwrap().rgba8;
    std::env::remove_var("YANSHI_VECTOR_RASTER_CACHE");
    assert_eq!(off, third, "关缓存的原路径与缓存路径像素必须一致");
    assert_eq!(off_renderer.vector_raster_stats().misses, 0);
}
