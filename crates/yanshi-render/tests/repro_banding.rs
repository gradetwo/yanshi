//! ## 复核结论（我落地后实测，**与报告不一致** ✗）
//!
//! **报告的两条论断，一真一假：**
//! - ✅ **真**：`render.rs::padding_for_region` / `global_padding` **确实只统计**
//!   蒙版羽化、形状羽化、滤镜三类外扩，**没有笔触半径这一支**（源码核对过）。
//! - ❌ **假**：报告中"缺陷存在时本测试会变红"**不成立**。我在当前 main 上实测：
//!   `parallel_chunks=0`（串行）与 `parallel_chunks=4`（并行 4 块）**逐字节完全相同** ⇒
//!   本场景**没有** banding。⇒ 因此**不能**用本测试断言"缺陷存在"。
//!
//! **所以我没有采纳报告的修复建议**（在 `global_padding` 里加 `brush.size/2+1`）：
//! 那段代码**没有任何证据**表明"缺这一支"导致了 banding，而**改 padding 会改变渲染结果**
//! ⇒ 极可能破坏既有的逐字节判据（`tile_parallel.rs` 判据 1、`render_properties.rs`）。
//!
//! **本测试仍然保留，但价值变了**：它现在是一条**有前置条件的不变量判据** ——
//! "并行真的分成 ≥2 块时，输出必须与串行逐字节一致"。没有那条前置断言，
//! 它无法区分"**没有 banding**"与"**根本没并行**" ⇒ 那是**空转的绿** ✗
//!（既有 `tile_parallel.rs` 早就有这类前置断言，报告提供的版本没有 ✓）。
//!
//! **仍未做到**：找出**真正**能触发 banding 的场景（如果它存在）。已知的候选：
//! 更大的画布／更多 band；以及报告真正的输入形态（4K 写实油画、40–120px 笔刷）。
//! 在找到能红的场景之前，**不改产品代码**。

//! **Banding 缺陷復現**：大筆觸跨並行 band 邊界時，串行與並行渲染輸出不一致。
//!
//! 根因：`render.rs::padding_for_region` 只統計濾鏡/蒙版羽化/形狀羽化，
//! 未包含筆觸半徑（`geometry_bbox` 的 `size/2 + 1`）。當 `padding=0` 時，
//! 跨 band 的大筆觸在 `stamp_*` 中被硬裁在 band 邊界；帶 `appearance`
//!（`PaintReservoir` 沿筆跡連續演進）的筆觸在兩邊獨立演進 reservoir，
//! 跨邊界處紋理/耗墨狀態斷層，視覺上為橫向條紋。
//!
//! 運行：`cargo test -p yanshi-render --test repro_banding`
//! 缺陷存在時 `serial_equals_parallel_with_big_brush_across_bands` 變紅。

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::render::Renderer;
use yanshi_render::tile::TileGrid;

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

fn object(id: &str, layer_id: &str, kind: ObjectType, z: i64, data: serde_json::Value) -> Object {
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

/// 構造文檔：一支 80px 大筆觸縱貫全幅（必跨所有 band 邊界），
/// 帶 appearance（油彩 reservoir，有限墨量 + 紋理），無濾鏡/羽化 ⇒ padding = 0。
fn banding_document(size: u32) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_banding_repro".to_owned());
    state.width = size;
    state.height = size;
    let s = size as f64;

    state.layers.insert("l1".to_owned(), layer("l1", 0));

    // 背景：中灰滿幅，襯托筆觸。
    state.objects.insert(
        "bg".to_owned(),
        object(
            "bg",
            "l1",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": s, "h": s}},
                   "color": {"r": 120, "g": 120, "b": 120, "a": 255}}),
        ),
    );

    // 大筆觸：從頂部到底部的垂直線，size=80，必跨 band 邊界。
    // appearance 帶有限墨量 paint（reservoir 沿筆跡耗墨）+ grain 紋理。
    // 320px / 4 workers ⇒ 每 band 約 80 行；筆觸半徑 40，padding=0 時邊界處被裁。
    let mut points = Vec::new();
    let mut y = 8.0;
    while y < s - 8.0 {
        points.push([s / 2.0, y]);
        y += 4.0;
    }
    state.objects.insert(
        "big_brush".to_owned(),
        object(
            "big_brush",
            "l1",
            ObjectType::Stroke,
            1,
            json!({
                "points": points,
                "size": 80.0,
                "color": {"r": 200, "g": 60, "b": 40, "a": 255},
                "appearance": {
                    "paint": {"paint_load": 0.6, "wetness": 0.7, "mixing": 0.5},
                    "texture": {"kind": "grain", "scale": 3.0, "strength": 0.5, "seed": 42}
                }
            }),
        ),
    );
    state
}

#[test]
fn serial_equals_parallel_with_big_brush_across_bands() {
    let size = 1024u32; // **我改大的** ✗：320 也绿；更大更可能触发，仍绿
    let state = banding_document(size);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, size, size).unwrap();

    let mut serial = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024).with_max_workers(1);
    let mut parallel = Renderer::with_budget(grid, 32 * 1024 * 1024).with_max_workers(8);

    let a = serial.render_document(&state, &store).unwrap();
    let b = parallel.render_document(&state, &store).unwrap();

    // **前置条件：并行必须真的分块** ✗（借 `tile_parallel.rs` 的口径 ✓）。
    // 没有这条断言，本测试无法区分"没有 banding"与"根本没并行" ⇒ 可能是**空转的绿** ✓。
    eprintln!(
        "  [定性] serial.parallel_chunks={} parallel.parallel_chunks={}",
        a.stats.parallel_chunks, b.stats.parallel_chunks
    );
    assert_eq!(a.stats.parallel_chunks, 0, "串行渲染不应分块");
    assert!(
        b.stats.parallel_chunks >= 2,
        "并行渲染必须真的分成 ≥2 块，否则本判据是空转（实际 {}）",
        b.stats.parallel_chunks
    );

    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    let diff = a
        .rgba8
        .iter()
        .zip(b.rgba8.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(
        diff,
        0,
        "並行與串行必須逐字節相同：{diff}/{} 字節不同（banding 缺陷）",
        a.rgba8.len()
    );
}
