//! 端到端示例：从原子日志渲染出一张 PNG（设计文档 6.1 / 7 章 / 8.3）。
//!
//! ```bash
//! cargo run -p yanshi-render --example render_demo
//! ```
//!
//! 示例覆盖：原子提交 → 折叠 → 计算内核层渲染（线性光、预乘 alpha、f16 tile）→
//! 图层混合与蒙版 → 调整与滤镜对象 → dirty 失效 → 缩略图 → PNG 输出。

use serde_json::json;
use std::path::PathBuf;
use yanshi_core::blob::{BlobStore, MemoryBlobStore};
use yanshi_core::{Atom, AtomKind, AtomLog, Bbox, FoldEngine};
use yanshi_render::dirty::plan_dirty;
use yanshi_render::render::Renderer;
use yanshi_render::thumb::{render_thumbnail, ThumbKind};
use yanshi_render::tile::TileGrid;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryBlobStore::new();
    let mut log = AtomLog::new();

    // 1) 提交：文档 + 两个图层 + 笔触 + 形状 + 蒙版 + 调整 + 滤镜。
    let atoms = vec![
        Atom::new(
            AtomKind::CreateDocument,
            "human:1",
            "session:web",
            json!({
                "doc_id": "doc_demo",
                "width": 256,
                "height": 192,
                "color_space": "srgb",
                "background": {"r": 250, "g": 248, "b": 244, "a": 255},
            }),
        ),
        Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:web",
            json!({"layer_id": "layer_sky", "name": "sky"}),
        ),
        Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:web",
            json!({"layer_id": "layer_ink", "name": "ink"}),
        ),
        // 天空：几个半透明椭圆叠出渐变感。
        Atom::new(
            AtomKind::DrawShape,
            "ai:1",
            "session:mcp",
            json!({
                "object_id": "sky_base",
                "layer_id": "layer_sky",
                "data": {
                    "geometry": {"kind": "ellipse", "bbox": {"x": 8, "y": 8, "w": 240, "h": 140}},
                    "color": {"r": 120, "g": 170, "b": 230, "a": 255},
                },
            }),
        ),
        Atom::new(
            AtomKind::DrawShape,
            "ai:1",
            "session:mcp",
            json!({
                "object_id": "sky_sun",
                "layer_id": "layer_sky",
                "data": {
                    "geometry": {"kind": "ellipse", "bbox": {"x": 150, "y": 24, "w": 72, "h": 72}},
                    "color": {"r": 250, "g": 210, "b": 120, "a": 200},
                    "blend_mode": "normal",
                },
            }),
        ),
        // 山：多边形。
        Atom::new(
            AtomKind::DrawShape,
            "ai:1",
            "session:mcp",
            json!({
                "object_id": "mountain",
                "layer_id": "layer_sky",
                "data": {
                    "geometry": {
                        "kind": "polygon",
                        "points": [[0, 150], [70, 78], [120, 150], [180, 96], [256, 160], [256, 192], [0, 192]],
                    },
                    "color": {"r": 70, "g": 96, "b": 120, "a": 255},
                },
            }),
        ),
        // 笔触：带压力的两段线。
        Atom::new(
            AtomKind::DrawStroke,
            "human:1",
            "session:web",
            json!({
                "object_id": "stroke_bird_1",
                "layer_id": "layer_ink",
                "brush": "round",
                "seed": 42,
                "data": {
                    "points": [[96, 52, 0.4], [112, 40, 1.0], [128, 52, 0.4]],
                    "size": 4.0,
                    "hardness": 0.7,
                    "jitter": 0.0,
                    "color": {"r": 25, "g": 25, "b": 30, "a": 255},
                },
            }),
        ),
        Atom::new(
            AtomKind::DrawStroke,
            "human:1",
            "session:web",
            json!({
                "object_id": "stroke_bird_2",
                "layer_id": "layer_ink",
                "data": {
                    "points": [[150, 44, 0.4], [164, 34, 1.0], [178, 44, 0.4]],
                    "size": 4.0,
                    "hardness": 0.7,
                    "color": {"r": 25, "g": 25, "b": 30, "a": 255},
                },
            }),
        ),
        // 形状描边：矩形边框。
        Atom::new(
            AtomKind::DrawShape,
            "human:1",
            "session:web",
            json!({
                "object_id": "frame",
                "layer_id": "layer_ink",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 6, "y": 6, "w": 244, "h": 180}},
                    "color": {"r": 0, "g": 0, "b": 0, "a": 0},
                    "stroke_width": 2.0,
                    "stroke_color": {"r": 40, "g": 40, "b": 40, "a": 255},
                },
            }),
        ),
        // 调整对象：整层提饱和（作用于此对象下方的内容）。
        Atom::new(
            AtomKind::CreateObject,
            "human:1",
            "session:web",
            json!({
                "object_id": "boost_saturation",
                "layer_id": "layer_sky",
                "type": "adjustment",
                "z_index": 40,
                "data": {"adjustment_type": "saturation", "params": {"amount": 1.3}},
            }),
        ),
        // 滤镜对象：轻高斯模糊压在天空层上。
        Atom::new(
            AtomKind::CreateObject,
            "human:1",
            "session:web",
            json!({
                "object_id": "soft_sky",
                "layer_id": "layer_sky",
                "type": "filter",
                "z_index": 39,
                "data": {"filter_name": "gaussian_blur", "params": {"sigma": 1.2}},
            }),
        ),
        // 蒙版：把天空层右侧裁掉一块（演示图层蒙版）。
        Atom::new(
            AtomKind::CreateMask,
            "human:1",
            "session:web",
            json!({
                "mask_id": "mask_sky",
                "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 232, "h": 192}},
            }),
        ),
        Atom::new(
            AtomKind::SetProperty,
            "human:1",
            "session:web",
            json!({"layer_id": "layer_sky", "key": "mask_id", "value": "mask_sky"}),
        ),
    ];

    for atom in atoms {
        let state = FoldEngine::new().fold(&log)?.state;
        let exists = |hash: &yanshi_core::BlobHash| store.exists(hash);
        let actor = atom.actor.clone();
        let session = atom.session.clone();
        log.append_validated(
            atom,
            &yanshi_core::CommitContext::new(&state, &exists, &actor, &session),
        )?;
    }
    println!("已提交 {} 个原子", log.len());

    // 2) 折叠到 HEAD。
    let state = FoldEngine::new().fold(&log)?.state;
    println!(
        "HEAD seq={}：图层 {}，对象 {}，活跃 blob {}",
        state.head_seq,
        state.alive_layers().len(),
        state.alive_objects().len(),
        state.active_blob_manifest().len()
    );

    // 3) 渲染整篇文档。
    let grid =
        TileGrid::new(64, state.width, state.height).ok_or("tile 边长必须是 32/64/128/256/512")?;
    let mut renderer = Renderer::new(grid);
    let render = renderer.render_document(&state, &store)?;
    println!(
        "渲染完成：{}×{}，tile {}，图层 {}，对象 {}，滤镜 padding {}，告警 {}",
        render.width,
        render.height,
        render.tiles.len(),
        render.stats.layers,
        render.stats.objects,
        render.stats.filter_padding,
        render.stats.unsupported.len()
    );
    for warning in &render.stats.unsupported {
        println!("  告警：{warning}");
    }

    // 4) 写 PNG（全尺寸 + 缩略图）。
    let out_dir = PathBuf::from("target/render-demo");
    std::fs::create_dir_all(&out_dir)?;
    let full_path = out_dir.join("yanshi-demo.png");
    yanshi_render::write_png_file(&full_path, render.width, render.height, &render.rgba8)?;
    println!("写出 {}", full_path.display());

    let thumb = render_thumbnail(&mut renderer, &state, &store, ThumbKind::Doc128, None)?;
    let thumb_path = out_dir.join("yanshi-demo-thumb128.png");
    yanshi_render::write_png_file(&thumb_path, thumb.size, thumb.size, &thumb.rgba8)?;
    println!("写出 {}", thumb_path.display());

    // 5) dirty 传播演示：模拟一次笔触编辑，看看失效了哪些 tile。
    let mut edited = state.clone();
    let new_object = yanshi_core::Object {
        id: "stroke_edit".to_owned(),
        layer_id: "layer_ink".to_owned(),
        object_type: yanshi_core::ObjectType::Stroke,
        z_index: 99,
        visible: true,
        locked: false,
        metadata: json!(null),
        transform: yanshi_core::Transform::IDENTITY,
        style: None,
        versions: vec!["edit".to_owned()],
        current_version: Some("edit".to_owned()),
        created_by: "human:1".to_owned(),
        deleted_by: None,
        data: json!({
            "points": [[20, 170], [90, 150], [160, 176]],
            "size": 6.0,
            "color": {"r": 200, "g": 60, "b": 60, "a": 255},
        }),
        blobs: Vec::new(),
    };
    edited.objects.insert("stroke_edit".to_owned(), new_object);
    let atom = Atom::new(
        AtomKind::DrawStroke,
        "human:1",
        "session:web",
        json!({"object_id": "stroke_edit", "layer_id": "layer_ink"}),
    );
    let dirty = plan_dirty(&edited, Some(&state), &atom);
    let tiles = renderer.apply_dirty(&edited, &dirty);
    println!(
        "dirty：{:?}（{}），失效 tile {:?}",
        dirty.kind, dirty.reason, tiles
    );

    // 6) 只重渲染失效 tile 的区域，验证增量路径可用。
    let region = Bbox::new(0.0, 128.0, 256.0, 64.0);
    let incremental = renderer.render_region(&edited, &store, region)?;
    println!(
        "增量渲染 {}×{} 完成（tile {}，缓存命中 {} / 未命中 {}）",
        incremental.width,
        incremental.height,
        incremental.tiles.len(),
        renderer.cache().stats().hits,
        renderer.cache().stats().misses
    );
    let incremental_path = out_dir.join("yanshi-demo-incremental.png");
    yanshi_render::write_png_file(
        &incremental_path,
        incremental.width,
        incremental.height,
        &incremental.rgba8,
    )?;
    println!("写出 {}", incremental_path.display());
    Ok(())
}
