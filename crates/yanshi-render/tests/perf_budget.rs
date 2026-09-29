//! 性能预算验收（设计文档 8.5 / 14.10），由 CI 的 `--ignored` 长跑作业执行。
//!
//! 预算表关键行：区域渲染（缓存命中 < 10ms / 未命中简单 < 100ms / 未命中含滤镜 < 300ms）、
//! 区域缩略图更新（缓存命中 < 5ms，需要重渲染 < 15ms）。
//!
//! 共享 CI 运行器的抖动较大，因此断言留了数倍余量，同时把实测值打印出来供回归对比。

use serde_json::json;
use std::time::{Duration, Instant};
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{Bbox, DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::render::Renderer;
use yanshi_render::thumb::{render_thumbnail, ThumbKind};
use yanshi_render::tile::{TileGrid, TileKey};
use yanshi_render::Prng;

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

fn base_object(
    id: &str,
    layer_id: &str,
    kind: ObjectType,
    z: i64,
    data: serde_json::Value,
) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: kind,
        z_index: z,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec!["a1".to_owned()],
        current_version: Some("a1".to_owned()),
        created_by: "a1".to_owned(),
        deleted_by: None,
        data,
        blobs: Vec::new(),
    }
}

/// 构造一个 1024×1024、`strokes` 笔笔触 + 若干形状的文档。
fn benchmark_document(strokes: usize, with_blur: bool) -> DocumentState {
    let mut rng = Prng::new(2026);
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_bench".to_owned());
    state.width = 1024;
    state.height = 1024;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state
        .layers
        .insert("layer_1".to_owned(), layer("layer_1", 0));
    state
        .layers
        .insert("layer_2".to_owned(), layer("layer_2", 1));

    for index in 0..strokes {
        let points: Vec<serde_json::Value> = (0..12)
            .map(|_| json!([rng.range(0.0, 1024.0) as f64, rng.range(0.0, 1024.0) as f64]))
            .collect();
        let layer_id = if index % 2 == 0 { "layer_1" } else { "layer_2" };
        state.objects.insert(
            format!("stroke_{index}"),
            base_object(
                &format!("stroke_{index}"),
                layer_id,
                ObjectType::Stroke,
                index as i64,
                json!({
                    "points": points,
                    "size": rng.range(2.0, 24.0) as f64,
                    "hardness": 0.6,
                    "color": {"r": 20, "g": 30, "b": 40, "a": 200},
                }),
            ),
        );
    }
    // 一些半透明形状，制造混合与 overdraw。
    for index in 0..8 {
        state.objects.insert(
            format!("shape_{index}"),
            base_object(
                &format!("shape_{index}"),
                if index % 2 == 0 { "layer_1" } else { "layer_2" },
                ObjectType::Shape,
                100 + index as i64,
                json!({
                    "geometry": {
                        "kind": if index % 2 == 0 { "rect" } else { "ellipse" },
                        "bbox": {
                            "x": rng.range(0.0, 700.0) as f64,
                            "y": rng.range(0.0, 700.0) as f64,
                            "w": rng.range(80.0, 300.0) as f64,
                            "h": rng.range(80.0, 300.0) as f64,
                        },
                    },
                    "color": {"r": 200, "g": 40, "b": 80, "a": 120},
                }),
            ),
        );
    }
    if with_blur {
        state.objects.insert(
            "blur".to_owned(),
            base_object(
                "blur",
                "layer_2",
                ObjectType::Filter,
                500,
                json!({"filter_name": "gaussian_blur", "params": {"sigma": 3.0}}),
            ),
        );
    }
    state
}

fn bench(label: &str, iterations: u32, mut body: impl FnMut()) -> Duration {
    // 预热一次，避免把首次分配算进预算。
    body();
    let started = Instant::now();
    for _ in 0..iterations {
        body();
    }
    let elapsed = started.elapsed() / iterations;
    println!("{label}: {elapsed:?}（{iterations} 次平均）");
    elapsed
}

#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（8.5 / 14.10）"]
fn perf_budget_region_render_and_thumbnails() {
    let store = MemoryBlobStore::new();
    let state = benchmark_document(60, false);
    let grid = TileGrid::new(256, 1024, 1024).unwrap();

    // 1) 区域渲染（缓存命中）：第二次取同一 tile 应走缓存。
    let mut renderer = Renderer::new(grid.clone());
    let region = Bbox::new(0.0, 0.0, 256.0, 256.0);
    renderer.render_region(&state, &store, region).unwrap();
    let cached = bench("区域渲染（缓存命中，256×256）", 5, || {
        let tile = renderer
            .render_tile(&state, &store, TileKey::new(0, 0))
            .unwrap();
        std::hint::black_box(tile.byte_len());
    });

    // 2) 区域渲染（未命中，简单区域）：每次新建渲染器，强制全量重算。
    let simple = bench("区域渲染（未命中，256×256，60 笔）", 3, || {
        let mut fresh = Renderer::new(grid.clone());
        let out = fresh.render_region(&state, &store, region).unwrap();
        std::hint::black_box(out.rgba8.len());
    });

    // 3) 区域渲染（未命中，含滤镜）：高斯模糊需要 padding。
    let filtered_state = benchmark_document(60, true);
    let filtered = bench("区域渲染（未命中，含高斯模糊）", 3, || {
        let mut fresh = Renderer::new(grid.clone());
        let out = fresh
            .render_region(&filtered_state, &store, region)
            .unwrap();
        std::hint::black_box(out.rgba8.len());
    });

    // 4) 区域缩略图更新（8.5：缓存命中 < 5ms，需要重渲染 < 15ms）。
    //    区域取 64×64（“区域缩略图”的典型粒度），另打印整篇文档缩略图作为信息项。
    let thumb_region = Bbox::new(64.0, 64.0, 64.0, 64.0);
    let mut thumb_renderer = Renderer::new(grid.clone());
    render_thumbnail(
        &mut thumb_renderer,
        &state,
        &store,
        ThumbKind::Layer64,
        Some(thumb_region),
    )
    .unwrap();
    let thumb = bench("区域缩略图更新（64×64 区域 → 64）", 3, || {
        let mut fresh = Renderer::new(TileGrid::new(256, 1024, 1024).unwrap());
        let thumb = render_thumbnail(
            &mut fresh,
            &state,
            &store,
            ThumbKind::Layer64,
            Some(thumb_region),
        )
        .unwrap();
        std::hint::black_box(thumb.rgba8.len());
    });
    let doc_thumb = bench(
        "整篇文档缩略图（128，含全图渲染）",
        1,
        || {
            let mut fresh = Renderer::new(TileGrid::new(256, 1024, 1024).unwrap());
            let thumb =
                render_thumbnail(&mut fresh, &state, &store, ThumbKind::Doc128, None).unwrap();
            std::hint::black_box(thumb.rgba8.len());
        },
    );
    println!("整篇文档缩略图 {doc_thumb:?}（信息项）");

    // 预算断言（留 5 倍余量以吸收共享运行器抖动）。
    assert!(
        cached < Duration::from_millis(50),
        "缓存命中区域渲染超预算：{cached:?}（设计目标 < 10ms）"
    );
    assert!(
        simple < Duration::from_millis(500),
        "未命中简单区域渲染超预算：{simple:?}（设计目标 < 100ms）"
    );
    assert!(
        filtered < Duration::from_millis(1500),
        "未命中含滤镜区域渲染超预算：{filtered:?}（设计目标 < 300ms）"
    );
    assert!(
        thumb < Duration::from_millis(200),
        "区域缩略图更新超预算：{thumb:?}（设计目标 < 15ms，留 5 倍以上余量吸收 CI 抖动）"
    );

    // 5) 整文档渲染（信息项，用于跟踪吞吐回归）。
    let mut full = Renderer::new(grid);
    let whole = bench(
        "整文档渲染（1024×1024，60 笔 + 8 形状）",
        1,
        || {
            let out = full.render_document(&state, &store).unwrap();
            std::hint::black_box(out.rgba8.len());
        },
    );
    println!("整文档渲染耗时 {whole:?}（信息项，不设硬预算）");
}

#[test]
#[ignore = "Overdraw 与 tile 命中率（Phase 0 验证项 / 14.9）"]
fn perf_overdraw_and_cache_hit_rate() {
    let store = MemoryBlobStore::new();
    let state = benchmark_document(200, false);
    let grid = TileGrid::new(256, 1024, 1024).unwrap();
    let mut renderer = Renderer::new(grid);

    let start = Instant::now();
    let out = renderer.render_document(&state, &store).unwrap();
    let elapsed = start.elapsed();
    let painted = out.stats.objects;
    let tiles = out.tiles.len();
    println!(
        "200 笔 overdraw：渲染 {elapsed:?}，对象 {painted}，tile {tiles}，像素 {}",
        out.rgba8.len()
    );
    assert_eq!(tiles, 16, "1024×1024 / 256 = 16 个 tile");

    // 第二次渲染：命中的 tile 不需要重算（这里比较的是缓存命中计数增长）。
    let before = renderer.cache().stats().hits;
    for key in [TileKey::new(0, 0), TileKey::new(1, 1), TileKey::new(2, 2)] {
        renderer.render_tile(&state, &store, key).unwrap();
    }
    let after = renderer.cache().stats().hits;
    assert!(after > before, "重复访问应命中 tile 缓存");
    println!(
        "tile 缓存：命中 {} 次，未命中 {} 次",
        after,
        renderer.cache().stats().misses
    );
}
