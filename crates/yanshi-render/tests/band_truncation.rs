//! **带截断（band truncation）复现**：蒙版羽化半径超过 `MAX_EFFECT_PADDING`（512；
//! 反馈期 2 前为 128）时，并行分带与串行整幅的渲染差超过已批准的 D1（±1 LSB）。
//!
//! 机制（源码核对，非猜测；以下为修前 128 上限下的失效模式，现带外扩已不钳）：
//! * `declared = padding_for_region`（蒙版羽化取 `feather/2 + 1`）在
//!   `render_region_with_lod` 被钳到 `max_filter_padding`（默认 512；修前 128）；
//! * 并行每带 `render_accumulation(band, padding = 128)`，带缓冲只覆「带 ±128」；
//! * 蒙版羽化在层缓冲内做 `box_blur`（`filter.rs`，边界语义是 **clamp 复制**），
//!   带界附近 `radius − padding` 行内窗口越出缓冲边、缺失行被边行复制值替代；
//! * 串行整幅缓冲含全画布，同一位置看到真值 ⇒ 两边算出的蒙版 alpha 不同，
//!   合成后像素差可达十几 LSB。
//!
//! 这**不是** D1 文档里说的「滑动窗口浮点求和顺序」那 ±1 LSB（见
//! `filter.rs::blur_horizontal` 的说明）：那一条只在窗口起点不同时产生累积
//! 误差；截断是缺失真邻域，是另一路误差源，且只在 `radius > padding` 时出现
//! （`tile_parallel.rs` 的 D1 判据用羽化 12 ⇒ 半径 6 ≪ 128，截断从未被触发）。
//!
//! 判据：羽化 400（半径 200，声明 201）的文档，串行与并行（2 带，界 y=700）
//! 渲染的 `rgba8` 必须落在 D1 内（最大差 ≤1 LSB，且差异字节 <1%）。
//! 修前红（差超过 D1），修后绿。另附一个更宽画布（2000×1400）的同形 case。
//!
//! 视觉产物：设环境变量 `BANDTRUNC_OUT=<目录>` 时，把串行 / 并行 / 差值图
//! （差值 ×20 放大的灰度）写成 PNG，供 1:1 与 3x 目检；CI 默认不写文件。

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{DocumentState, Layer, LayerType, Object, ObjectType, Selection, Transform};
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
        seq: 0,
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: kind,
        z_index: z,
        visible: true,
        enabled: true,
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

/// 截断复现文档：底色满幅矩形 + 一层同尺寸强对比矩形，其蒙版是纵贯带界的椭圆。
///
/// 椭圆中心 `(w/2, 650)`、竖半径 200 ⇒ 在中心列上蒙版为 1 的区间是 y∈(450, 850)，
/// 下过渡边（y=850）落在带 0 缓冲边（700+128=828）之外、模糊窗口（y=699 时
/// 上沿 899）之内 ⇒ 带 0 的模糊在该列上拿不到 850 之后的真值，只能复制 828 行；
/// 串行则看到真值。羽化 400 ⇒ 半径 200、声明外扩 201（> 128，被钳）。
fn truncation_document(width: u32, height: u32, feather: f64) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_band_truncation".to_owned());
    state.width = width;
    state.height = height;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    let w = width as f64;
    let h = height as f64;

    state
        .layers
        .insert("layer_bottom".to_owned(), layer("layer_bottom", 0));
    let mut top = layer("layer_top", 1);
    top.mask_id = Some("mask_1".to_owned());
    state.layers.insert(top.id.clone(), top);

    state.objects.insert(
        "bg".to_owned(),
        object(
            "bg",
            "layer_bottom",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": w, "h": h}},
                   "color": {"r": 40, "g": 70, "b": 110, "a": 255}}),
        ),
    );
    state.objects.insert(
        "wash".to_owned(),
        object(
            "wash",
            "layer_top",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": w, "h": h}},
                   "color": {"r": 240, "g": 200, "b": 60, "a": 255}}),
        ),
    );
    // 椭圆：x 方向留 5% 边，y 方向中心 650、竖半径 200（下过渡边 y=850）。
    let ex = w * 0.05;
    state.masks.insert(
        "mask_1".to_owned(),
        Selection {
            patches: Default::default(),
            id: "mask_1".to_owned(),
            shape: json!({"kind": "ellipse",
                          "bbox": {"x": ex, "y": 450, "w": w - ex * 2.0, "h": 400}}),
            feather,
            mode: "new".to_owned(),
            invert: false,
            linked_layer: Some("layer_top".to_owned()),
            refined_edges: false,
            blobs: Vec::new(),
            created_by: "human:1".to_owned(),
            deleted_by: None,
        },
    );
    state
}

fn renderers(width: u32, height: u32) -> (Renderer, Renderer) {
    let grid = TileGrid::new(64, width, height).unwrap();
    (
        Renderer::with_budget(grid.clone(), 256 * 1024 * 1024).with_max_workers(1),
        Renderer::with_budget(grid, 256 * 1024 * 1024).with_max_workers(2),
    )
}

/// 串行与并行渲染的差异统计（字节级）：差异字节数、最大差、差异行范围。
fn diff_stats(a: &[u8], b: &[u8], width: u32) -> (usize, i32, Option<(u32, u32)>) {
    let mut count = 0usize;
    let mut max_delta = 0i32;
    let mut min_row: Option<u32> = None;
    let mut max_row: Option<u32> = None;
    for (index, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            count += 1;
            let delta = (*x as i32 - *y as i32).abs();
            max_delta = max_delta.max(delta);
            let row = (index / 4 / width as usize) as u32;
            min_row = Some(min_row.map_or(row, |m: u32| m.min(row)));
            max_row = Some(max_row.map_or(row, |m: u32| m.max(row)));
        }
    }
    let rows = match (min_row, max_row) {
        (Some(lo), Some(hi)) => Some((lo, hi)),
        _ => None,
    };
    (count, max_delta, rows)
}

fn dump_pngs(tag: &str, width: u32, height: u32, serial: &[u8], parallel: &[u8]) {
    let Some(dir) = std::env::var_os("BANDTRUNC_OUT") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let serial_png = yanshi_render::encode_png(width, height, serial).unwrap();
    let parallel_png = yanshi_render::encode_png(width, height, parallel).unwrap();
    let mut diff = vec![0u8; serial.len()];
    for (index, (x, y)) in serial.iter().zip(parallel.iter()).enumerate() {
        let d = (*x as i32 - *y as i32).abs();
        diff[index] = (d * 20).min(255) as u8;
    }
    let diff_png = yanshi_render::encode_png(width, height, &diff).unwrap();
    let _ = std::fs::write(dir.join(format!("{tag}-serial.png")), serial_png);
    let _ = std::fs::write(dir.join(format!("{tag}-parallel.png")), parallel_png);
    let _ = std::fs::write(dir.join(format!("{tag}-diff.png")), diff_png);
}

fn assert_within_d1(tag: &str, width: u32, height: u32, feather: f64) {
    let state = truncation_document(width, height, feather);
    let store = MemoryBlobStore::new();
    let (mut serial, mut parallel) = renderers(width, height);
    let a = serial.render_document(&state, &store).unwrap();
    let b = parallel.render_document(&state, &store).unwrap();
    // 前置条件：并行真的分了 2 带（否则是空转的绿）。
    assert_eq!(a.stats.parallel_chunks, 0, "串行不应分块");
    assert_eq!(
        b.stats.parallel_chunks, 2,
        "并行应恰好分 2 带（界 y=700），实测 {}",
        b.stats.parallel_chunks
    );
    let (count, max_delta, rows) = diff_stats(&a.rgba8, &b.rgba8, width);
    println!(
        "{tag}: 羽化 {feather}（半径 {}）串行 vs 并行 2 带：{count} 字节不同 / {}，最大差 {max_delta} LSB，差异行 {rows:?}",
        (feather / 2.0).round() as u32,
        a.rgba8.len()
    );
    dump_pngs(tag, width, height, &a.rgba8, &b.rgba8);
    assert!(
        max_delta <= 1,
        "{tag}: D1 只允许 ±1 LSB，实测最大 {max_delta} LSB（差异字节 {count}，行 {rows:?}）——带截断未消除"
    );
    assert!(
        count * 100 <= a.rgba8.len(),
        "{tag}: D1 差异应稀疏（<1% 字节），实测 {}/{}",
        count,
        a.rgba8.len()
    );
}

/// 主 case：700×1400，2 带，界 y=700；羽化 400（半径 200，声明 201 > 128）。
#[test]
fn feather_beyond_padding_matches_serial_within_d1() {
    assert_within_d1("700x1400", 700, 1400, 400.0);
}

/// 更宽画布：2000×1400，同形同羽化；界仍在 y=700。
#[test]
fn wider_canvas_feather_beyond_padding_matches_serial_within_d1() {
    assert_within_d1("2000x1400", 2000, 1400, 400.0);
}

/// 大半径应力：羽化 1000（半径 500，声明 501）——带外扩不设上限的凭据。
/// 若带外扩在某处被再钳一次，本判据立刻红（最大差回到两位数）。
#[test]
fn huge_feather_matches_serial_within_d1() {
    assert_within_d1("700x1400-f1000", 700, 1400, 1000.0);
}

/// 超新上限应力：羽化 1200（半径 600，声明 601 > 512）——串行区域外扩仍被
/// 新上限钳住，但并行分带按声明外扩；整幅串行与并行仍须落在 D1 内。
/// （反馈期 2 后旧 501 声明已落入上限内，本 case 接替“超上限”覆盖。）
#[test]
fn feather_beyond_new_padding_matches_serial_within_d1() {
    assert_within_d1("700x1400-f1200", 700, 1400, 1200.0);
}
