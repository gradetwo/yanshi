//! 渲染内核的属性测试：D0 确定性、分块无关性、增量与全量一致、缓存淘汰安全。
//!
//! 这些不变量把设计文档的若干要求变成可执行断言：
//!
//! - **D0 bit-exact（6.1）**：同一状态重复渲染逐字节一致。
//! - **tile 分块不影响结果（6.4）**：Tile 是缓存与物化形式，不是语义单位。
//! - **O(dirty) 渲染（6.2/6.6）**：dirty 集合之外的像素必须与重算结果完全一致。
//! - **缓存淘汰安全（13.3）**：淘汰只影响性能，不影响像素。

use proptest::prelude::*;
use serde_json::json;
use std::collections::BTreeSet;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{Bbox, DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::dirty::plan_dirty;
use yanshi_render::render::Renderer;
use yanshi_render::thumb::{render_thumbnail, Thumb, ThumbKind};
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

fn shape(id: &str, layer_id: &str, z: i64, bbox: [f64; 4], color: serde_json::Value) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Shape,
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
        data: json!({
            "geometry": {
                "kind": "rect",
                "bbox": {"x": bbox[0], "y": bbox[1], "w": bbox[2], "h": bbox[3]},
            },
            "color": color,
        }),
        blobs: Vec::new(),
    }
}

fn stroke(id: &str, layer_id: &str, z: i64, points: Vec<(f64, f64)>, size: f64) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Stroke,
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
        data: json!({
            "points": points.iter().map(|(x, y)| json!([x, y])).collect::<Vec<_>>(),
            "size": size,
            "color": [0.0, 0.0, 0.0, 1.0],
        }),
        blobs: Vec::new(),
    }
}

/// 由种子构造一个随机小文档（确定性）。
fn random_document(seed: u64, size: u32, objects: usize) -> DocumentState {
    let mut rng = Prng::new(seed);
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_1".to_owned());
    state.width = size;
    state.height = size;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state
        .layers
        .insert("layer_1".to_owned(), layer("layer_1", 0));
    state
        .layers
        .insert("layer_2".to_owned(), layer("layer_2", 1));

    for index in 0..objects {
        let id = format!("obj_{index}");
        match rng.below(3) {
            0 => {
                let x = rng.range(0.0, size as f32 * 0.7);
                let y = rng.range(0.0, size as f32 * 0.7);
                let w = rng.range(4.0, size as f32 * 0.3);
                let h = rng.range(4.0, size as f32 * 0.3);
                let layer_id = if index % 2 == 0 { "layer_1" } else { "layer_2" };
                state.objects.insert(
                    id.clone(),
                    shape(
                        &id,
                        layer_id,
                        index as i64,
                        [x as f64, y as f64, w as f64, h as f64],
                        json!({"r": 255, "g": 0, "b": 0, "a": 200}),
                    ),
                );
            }
            1 => {
                let x0 = rng.range(0.0, size as f32);
                let y0 = rng.range(0.0, size as f32);
                let x1 = rng.range(0.0, size as f32);
                let y1 = rng.range(0.0, size as f32);
                let layer_id = if index % 2 == 0 { "layer_1" } else { "layer_2" };
                state.objects.insert(
                    id.clone(),
                    stroke(
                        &id,
                        layer_id,
                        index as i64,
                        vec![(x0 as f64, y0 as f64), (x1 as f64, y1 as f64)],
                        rng.range(1.0, 8.0) as f64,
                    ),
                );
            }
            _ => {
                let layer_id = if index % 2 == 0 { "layer_1" } else { "layer_2" };
                let mut object = shape(
                    &id,
                    layer_id,
                    index as i64,
                    [0.0, 0.0, size as f64, size as f64],
                    json!({"r": 0, "g": 0, "b": 255, "a": 128}),
                );
                object.object_type = ObjectType::Adjustment;
                object.data = json!({
                    "adjustment_type": "saturation",
                    "params": {"amount": rng.range(0.0, 2.0)},
                });
                state.objects.insert(id, object);
            }
        }
    }
    state
}

fn render_with_tile(state: &DocumentState, tile: u32) -> Vec<u8> {
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(tile, state.width, state.height).unwrap();
    let mut renderer = Renderer::new(grid);
    renderer.render_document(state, &store).unwrap().rgba8
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, max_shrink_iters: 32, ..ProptestConfig::default() })]

    /// D0：同一状态重复渲染逐字节一致。
    #[test]
    fn rendering_is_bit_exact(seed in any::<u64>(), objects in 1usize..12) {
        let state = random_document(seed, 64, objects);
        let first = render_with_tile(&state, 32);
        let second = render_with_tile(&state, 32);
        prop_assert_eq!(first, second);
    }

    /// tile 分块只影响物化形式，不影响像素。
    #[test]
    fn tile_size_does_not_change_pixels(seed in any::<u64>(), objects in 1usize..12) {
        let state = random_document(seed, 96, objects);
        let small = render_with_tile(&state, 32);
        let large = render_with_tile(&state, 64);
        prop_assert_eq!(small, large);
    }

    /// 缓存淘汰只影响性能，不影响像素。
    #[test]
    fn cache_eviction_does_not_change_pixels(seed in any::<u64>(), objects in 1usize..12) {
        let state = random_document(seed, 64, objects);
        let store = MemoryBlobStore::new();
        // 预算仅容纳一个 tile：强制反复淘汰。
        let grid = TileGrid::new(32, 64, 64).unwrap();
        let mut renderer = Renderer::with_budget(grid, 32 * 32 * 4 * 2);
        let thrashing = renderer.render_document(&state, &store).unwrap().rgba8;
        let stable = render_with_tile(&state, 32);
        prop_assert_eq!(thrashing, stable);
    }

    /// O(dirty)：dirty 集合之外的像素必须与重算结果一致，集合之内必须更新。
    #[test]
    fn dirty_set_covers_every_changed_pixel(
        seed in any::<u64>(),
        objects in 1usize..10,
        edit in any::<u64>(),
    ) {
        let before = random_document(seed, 96, objects);
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 96, 96).unwrap());
        let old = renderer.render_document(&before, &store).unwrap().rgba8;

        // 施加一次编辑：新增一条笔触。
        let mut rng = Prng::new(edit);
        let mut after = before.clone();
        // **两个点必须不同** ✗ —— 原来两次独立取随机点 ✓，proptest 会**收缩到"两点重合"** ✓
        // ⇒ 零长度笔触**不产生任何像素** ✗ ⇒ 下面那句"编辑必须改变像素"必然失败 ✓。
        // **这是生成器的漏洞，不是渲染的错** ✓ ⇒ 从生成器上修 ✓（而不是放宽断言 ✗）：
        // 第二点由**第一点 + 一个非零偏移**得到 ✓ ⇒ 这一笔**必然有长度** ✓。
        let (x0, y0) = (rng.range(0.0, 80.0) as f64, rng.range(0.0, 80.0) as f64);
        let dx = rng.range(4.0, 16.0) as f64;
        let dy = rng.range(-16.0, 16.0) as f64;
        let new_object = stroke(
            "obj_edit",
            "layer_1",
            99,
            vec![(x0, y0), (x0 + dx, y0 + dy)],
            rng.range(2.0, 10.0) as f64,
        );
        after.objects.insert("obj_edit".to_owned(), new_object);

        let atom = yanshi_core::Atom::new(
            yanshi_core::AtomKind::DrawStroke,
            "human:1",
            "s",
            json!({"object_id": "obj_edit", "layer_id": "layer_1"}),
        );
        let dirty = plan_dirty(&after, Some(&before), &atom);
        prop_assert!(!dirty.is_none(), "新增笔触必须产生 dirty");

        // 引擎的契约：**失效 tile 集合必须覆盖所有真正变化的像素**。
        let grid = TileGrid::new(32, 96, 96).unwrap();
        let keys = yanshi_render::dirty::invalidated_tiles(&grid, &after, &dirty);
        prop_assert!(!keys.is_empty(), "新增笔触必须失效至少一个 tile");

        let mut covered = vec![false; (96 * 96) as usize];
        for key in &keys {
            let bounds = grid.bounds(*key);
            for y in (bounds.y as u32)..((bounds.y + bounds.h) as u32) {
                for x in (bounds.x as u32)..((bounds.x + bounds.w) as u32) {
                    covered[(y * 96 + x) as usize] = true;
                }
            }
        }

        let mut fresh = Renderer::new(grid.clone());
        let new = fresh.render_document(&after, &store).unwrap().rgba8;
        // **失败信息里不要塞整幅像素** ✗（40KB 会把断言本身淹掉 ✓ —— 我就是这么找了两轮 ✓）：
        // 只报"改了几个像素" ✓ 与两个小样本 ✓，够定位就行 ✓。
        let changed_count = old
            .chunks_exact(4)
            .zip(new.chunks_exact(4))
            .filter(|(a, b)| a != b)
            .count();
        prop_assert!(
            changed_count > 0,
            "编辑必须改变像素（实测改了 {} 个）\n  旧样本={:?}\n  新样本={:?}",
            changed_count,
            &old[..old.len().min(16)],
            &new[..new.len().min(16)]
        );

        let mut changed = 0usize;
        for y in 0..96u32 {
            for x in 0..96u32 {
                let index = ((y * 96 + x) * 4) as usize;
                if old[index..index + 4] != new[index..index + 4] {
                    changed += 1;
                    prop_assert!(
                        covered[(y * 96 + x) as usize],
                        "像素 ({}, {}) 变化但未被 dirty tile 覆盖",
                        x,
                        y
                    );
                }
            }
        }
        prop_assert!(changed > 0, "编辑必须改变至少一个像素");
    }

    /// 缩略图：分块增量更新与整张重建逐字节一致。
    #[test]
    fn thumbnail_blocks_match_full_rebuild(seed in any::<u64>(), objects in 1usize..10) {
        let state = random_document(seed, 96, objects);
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 96, 96).unwrap());
        let full = render_thumbnail(&mut renderer, &state, &store, ThumbKind::Doc64, None)
            .unwrap();

        // 用渲染输出手工重采样，逐字节比较。
        let render = renderer
            .render_document(&state, &store)
            .unwrap();
        let mut buffer = yanshi_render::Buffer::new(0, 0, render.width, render.height);
        for y in 0..render.height {
            for x in 0..render.width {
                let pixel = render.pixel(x, y).unwrap();
                buffer.set_pixel(x, y, yanshi_render::u8x4_to_linear_premul(pixel));
            }
        }
        let mut manual = Thumb::new(ThumbKind::Doc64);
        manual.update_full(&buffer, Bbox::new(0.0, 0.0, 96.0, 96.0));
        prop_assert_eq!(full.rgba8.clone(), manual.rgba8.clone());

        // 只更新部分块：其它块保持原值（未更新的块不写入）。
        let mut incremental = full.clone();
        incremental.rgba8 = vec![0; full.rgba8.len()];
        let dirty_blocks = incremental.dirty_blocks_for(
            &Bbox::new(0.0, 0.0, 32.0, 32.0),
            Bbox::new(0.0, 0.0, 96.0, 96.0),
        );
        prop_assert!(dirty_blocks.len() < incremental.stats.blocks_total);
        incremental.update_blocks(&buffer, Bbox::new(0.0, 0.0, 96.0, 96.0), &dirty_blocks);
        for (block_x, block_y) in &dirty_blocks {
            let bounds = full.block_bounds(*block_x, *block_y);
            for y in (bounds.y as u32)..((bounds.y + bounds.h) as u32) {
                for x in (bounds.x as u32)..((bounds.x + bounds.w) as u32) {
                    prop_assert_eq!(incremental.pixel(x, y), full.pixel(x, y));
                }
            }
        }
    }

    /// PNG 输出结构合法且可字节复现。
    #[test]
    fn png_output_is_valid_and_deterministic(seed in any::<u64>(), objects in 1usize..8) {
        let state = random_document(seed, 48, objects);
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 48, 48).unwrap());
        let render = renderer.render_document(&state, &store).unwrap();
        let first = yanshi_render::encode_png(render.width, render.height, &render.rgba8).unwrap();
        let second = yanshi_render::encode_png(render.width, render.height, &render.rgba8).unwrap();
        prop_assert_eq!(first.clone(), second);
        prop_assert_eq!(&first[0..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        prop_assert!(first.windows(4).any(|w| w == b"IHDR"));
        prop_assert!(first.windows(4).any(|w| w == b"IDAT"));
        prop_assert!(first.windows(4).any(|w| w == b"IEND"));
    }

    /// 渲染结果的结构自洽：tile 集合与请求区域一致，像素数量匹配。
    #[test]
    fn region_render_bookkeeping_is_consistent(seed in any::<u64>(), objects in 1usize..10) {
        let state = random_document(seed, 96, objects);
        let store = MemoryBlobStore::new();
        let grid = TileGrid::new(32, 96, 96).unwrap();
        let mut renderer = Renderer::new(grid.clone());
        let region = Bbox::new(16.0, 16.0, 40.0, 24.0);
        let out = renderer.render_region(&state, &store, region).unwrap();
        prop_assert_eq!(out.width, 40);
        prop_assert_eq!(out.height, 24);
        prop_assert_eq!(out.rgba8.len(), (40 * 24 * 4) as usize);
        let expected: BTreeSet<TileKey> = grid.keys_for_bbox(&region).into_iter().collect();
        let actual: BTreeSet<TileKey> = out.tiles.iter().copied().collect();
        prop_assert_eq!(expected, actual);
        // 状态未被渲染修改（渲染只读状态）。
        prop_assert!(state.is_consistent());
    }
}

/// **移动**同样要满足"失效 tile 覆盖所有变化像素" ✓ —— 用户实测的残影就是这么来的 ✗：
/// 脏区规划只用**新**包围盒 ✓，于是**旧位置**的像素被改回背景却没被失效 ✗，
/// 画布留着残影 ✓（缩略图整幅重绘所以正常 ✓）。
///
/// 这条守卫此前不存在 ✗ —— 原有的 `dirty_set_covers_every_changed_pixel` 只随机做 `DrawStroke` ✓，
/// 从没覆盖过 `Move` ✓，所以这个 bug 从属性测试里溜了过去 ✓。
///
/// 写成确定性的循环（而不是 proptest 宏）✓：覆盖几个 seed × 几个位移即可稳定复现 ✓，
/// 且不必纠结宏的语法位置 ✓（第一版就因为把宏测试追加到 `proptest!` 块外而编译失败 ✗）。
#[test]
fn dirty_set_covers_both_ends_of_a_move() {
    let mut checked = 0usize;
    for seed in [1u64, 7, 42, 99, 1234, 20240930] {
        for (dx, dy) in [
            (24.0f64, 0.0f64),
            (-24.0, 12.0),
            (8.0, -18.0),
            (-40.0, -40.0),
        ] {
            let before = random_document(seed, 96, 4);
            let store = MemoryBlobStore::new();
            let mut renderer = Renderer::new(TileGrid::new(32, 96, 96).unwrap());
            let old = renderer.render_document(&before, &store).unwrap().rgba8;

            let Some(target) = before
                .objects
                .values()
                .find(|object| yanshi_render::object_bbox(object).is_some())
                .map(|object| object.id.clone())
            else {
                continue;
            };

            let mut after = before.clone();
            after.objects.get_mut(&target).unwrap().transform = yanshi_core::Transform {
                matrix: [1.0, 0.0, 0.0, 1.0, dx, dy],
                pivot: [0.0, 0.0],
            };
            let atom = yanshi_core::Atom::new(
                yanshi_core::AtomKind::Move,
                "human:1",
                "s",
                json!({"object_id": target, "delta": {"dx": dx, "dy": dy}}),
            );
            let dirty = plan_dirty(&after, Some(&before), &atom);
            assert!(!dirty.is_none(), "移动必须产生 dirty");

            let grid = TileGrid::new(32, 96, 96).unwrap();
            let keys = yanshi_render::dirty::invalidated_tiles(&grid, &after, &dirty);
            let mut covered = vec![false; (96 * 96) as usize];
            for key in &keys {
                let bounds = grid.bounds(*key);
                for y in (bounds.y as u32)..((bounds.y + bounds.h) as u32) {
                    for x in (bounds.x as u32)..((bounds.x + bounds.w) as u32) {
                        covered[(y * 96 + x) as usize] = true;
                    }
                }
            }

            let mut fresh = Renderer::new(grid.clone());
            let new = fresh.render_document(&after, &store).unwrap().rgba8;
            let (mut changed, mut uncovered) = (0usize, 0usize);
            for y in 0..96u32 {
                for x in 0..96u32 {
                    let index = ((y * 96 + x) * 4) as usize;
                    if old[index..index + 4] != new[index..index + 4] {
                        changed += 1;
                        if !covered[(y * 96 + x) as usize] {
                            uncovered += 1;
                        }
                    }
                }
            }
            // 有些随机对象移到画布外 ⇒ 像素本就不变 ✓（这不代表有问题 ✓）。
            // 因此只统计"确实变了"的组合 ✓，但对它们的要求是硬的 ✓：
            // **每一个变化像素都必须落在失效 tile 里** ✓。
            assert_eq!(
                uncovered, 0,
                "有 {uncovered} 个变化像素未被失效覆盖（共变化 {changed}，seed {seed}，delta {dx},{dy}）"
            );
            if changed > 0 {
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 4,
        "有效组合过少（{checked}）—— 测试可能没真正覆盖到移动 ✓"
    );
}
