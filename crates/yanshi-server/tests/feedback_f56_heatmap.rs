//! **区域作画量热力图 `stroke_heatmap`**（反馈报告 §5.6；分诊：无热力图工具，
//! `analyze_region` 只给主色／亮度／冷暖比，不含笔数与墨量分布）。
//!
//! **⇒ 语义**：参数形状与 `render_region`／`analyze_region` 一致，`{x, y, w, h}`
//! 或 `[x, y, w, h]`，画布像素、左上角原点；网格缺省边长 64，格数上限 10000。
//! 每格给 `strokes`（进该格的去重前笔数：一笔进多格时每格计 1）／`points`（落
//! 在该格的控制点数）／`ink_mass`（烘焙位图 tile 在该格内的 alpha 求和）。
//! **⇒ 合计恒等式**：`total_strokes` 是去重笔数（≤ 各格 `strokes` 之和）；
//! `total_points = Σ 格 points`；`total_ink_mass = Σ 格 ink_mass`；区域盖住整笔
//! tile 时 `total_ink_mass` 等于该笔回执的 `ink_mass`。
//! **⇒ 口径缺口**：`medium_stroke` 的介质笔是无 `source` 的 `ImportImage` 原子，
//! 不在日志扫描内 ⇒ 热力图漏介质笔（本文件以同形的 `import_image` 钉住“导入图
//! 不是笔”）；`draw_stroke` 笔无 tile ⇒ 只计笔数与点数，墨量不含它们
//! （`strokes_without_ink` 计缺口笔数）。
//! **⇒ 几何口径**（画布 200×200，`draw_stroke` 的 `region` =
//! 点列包围盒向外扩 `size / 2 + 2`）：笔 A `[[10,10],[30,10]] size 5` ⇒
//! `bbox {x: 5.5, y: 5.5, w: 29, h: 9}`；笔 B `[[150,150],[170,150]] size 5` ⇒
//! `bbox {x: 145.5, y: 145.5, w: 29, h: 9}`（与 f48 同口径）。
//!
//! **⇒ 变异**：把墨量改成“整笔回执值平摊到各格”⇒ 笔刷双格测试红（格 ink 不再等于
//! 回执）；把扫描改成“只认点列”⇒ 跨格笔的 `total_strokes` 去重测试红；把
//! `strokes_without_ink` 改成恒 0 ⇒ 矢量笔测试红；把上限 10000 删掉 ⇒ 超限测试红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_f56", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_f56", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(&mut ctx, tool, &args)
}

fn draw(
    workspace: &mut Workspace,
    object_id: &str,
    points: serde_json::Value,
) -> serde_json::Value {
    let response = call(
        workspace,
        "draw_stroke",
        json!({
            "layer_id": "layer_default",
            "object_id": object_id,
            "data": {"points": points, "size": 5.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    response
}

fn heatmap(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    call(workspace, "stroke_heatmap", args)
}

fn cell(map: &serde_json::Value, col: u64, row: u64) -> &serde_json::Value {
    let cols = map["cols"].as_u64().unwrap();
    let index = (row * cols + col) as usize;
    &map["cells"][index]
}

fn two_strokes(workspace: &mut Workspace) {
    draw(workspace, "near", json!([[10.0, 10.0], [30.0, 10.0]]));
    draw(workspace, "far", json!([[150.0, 150.0], [170.0, 150.0]]));
}

#[test]
fn known_draw_strokes_land_in_the_right_cells_with_zero_ink() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let map = heatmap(
        &mut workspace,
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}, "cell_size": 50.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(map["cols"], json!(2), "{map}");
    assert_eq!(map["rows"], json!(2), "{map}");
    assert_eq!(map["cells"].as_array().unwrap().len(), 4, "{map}");
    // 近处一笔：两点都在 (0,0) 格，矢量笔无墨量，缺口计 1。
    assert_eq!(map["total_strokes"], json!(1), "{map}");
    assert_eq!(map["total_points"], json!(2), "{map}");
    assert_eq!(map["total_ink_mass"], json!(0), "{map}");
    assert_eq!(map["strokes_without_ink"], json!(1), "{map}");
    assert_eq!(cell(&map, 0, 0)["strokes"], json!(1), "{map}");
    assert_eq!(cell(&map, 0, 0)["points"], json!(2), "{map}");
    assert_eq!(cell(&map, 0, 0)["ink_mass"], json!(0), "{map}");
    // 其余三格全 0：远处一笔在区域外，空白格也是 0。
    for (col, row) in [(1, 0), (0, 1), (1, 1)] {
        assert_eq!(cell(&map, col, row)["strokes"], json!(0), "{map}");
        assert_eq!(cell(&map, col, row)["points"], json!(0), "{map}");
        assert_eq!(cell(&map, col, row)["ink_mass"], json!(0), "{map}");
    }
}

#[test]
fn a_stroke_across_cells_counts_once_in_total_but_once_per_cell() {
    let mut workspace = workspace();
    draw(&mut workspace, "wide", json!([[10.0, 10.0], [90.0, 10.0]]));

    let map = heatmap(
        &mut workspace,
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}, "cell_size": 50.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    // 两点分属 (0,0) 与 (1,0) 两格；合计去重仍是 1 笔。
    assert_eq!(cell(&map, 0, 0)["strokes"], json!(1), "{map}");
    assert_eq!(cell(&map, 0, 0)["points"], json!(1), "{map}");
    assert_eq!(cell(&map, 1, 0)["strokes"], json!(1), "{map}");
    assert_eq!(cell(&map, 1, 0)["points"], json!(1), "{map}");
    assert_eq!(map["total_strokes"], json!(1), "{map}");
    assert_eq!(map["total_points"], json!(2), "{map}");
    let cell_stroke_sum: u64 = map["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["strokes"].as_u64().unwrap())
        .sum();
    assert_eq!(
        cell_stroke_sum, 2,
        "一笔进两格 ⇒ 各格之和 2，大于去重合计 1：{map}"
    );
}

#[test]
fn region_outside_canvas_and_blank_cells_are_zero_not_error() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let outside = heatmap(
        &mut workspace,
        json!({"region": {"x": 300.0, "y": 300.0, "w": 50.0, "h": 50.0}}),
    );
    assert_eq!(outside["ok"], json!(true), "{outside}");
    assert_eq!(outside["total_strokes"], json!(0), "{outside}");
    assert_eq!(outside["total_points"], json!(0), "{outside}");
    assert_eq!(outside["total_ink_mass"], json!(0), "{outside}");
    assert!(
        outside["cells"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["strokes"] == json!(0) && c["ink_mass"] == json!(0)),
        "{outside}"
    );
}

#[test]
fn array_form_region_is_accepted_like_render_region() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let map = heatmap(
        &mut workspace,
        json!({"region": [0.0, 0.0, 100.0, 100.0], "cell_size": 50.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(map["total_strokes"], json!(1), "{map}");
    assert_eq!(map["total_points"], json!(2), "{map}");
}

#[test]
fn default_cell_size_is_64_and_edge_cells_are_clipped() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let map = heatmap(
        &mut workspace,
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(map["cell_size"], json!(64.0), "{map}");
    assert_eq!(map["cols"], json!(2), "{map}");
    assert_eq!(map["rows"], json!(2), "{map}");
    // 边缘格按区域边界裁剪：100 = 64 + 36。
    assert_eq!(cell(&map, 1, 0)["x"], json!(64.0), "{map}");
    assert_eq!(cell(&map, 1, 0)["w"], json!(36.0), "{map}");
    assert_eq!(cell(&map, 1, 1)["y"], json!(64.0), "{map}");
    assert_eq!(cell(&map, 1, 1)["h"], json!(36.0), "{map}");
}

#[test]
fn malformed_region_and_cell_size_error_loudly_without_cells() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let malformed = [
        // 负宽。
        json!({"region": {"x": 0.0, "y": 0.0, "w": -10.0, "h": 50.0}}),
        // 零高。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 50.0, "h": 0.0}}),
        // 缺字段。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 50.0}}),
        // 非数字。
        json!({"region": {"x": 0.0, "y": 0.0, "w": "wide", "h": 50.0}}),
        // 数组长度不是 4。
        json!({"region": [0.0, 0.0, 50.0]}),
        // 根本不是区域形状。
        json!({"region": "top-left"}),
        // cell_size 为 0。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0}, "cell_size": 0.0}),
        // cell_size 为负。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0}, "cell_size": -5.0}),
        // cell_size 非数字。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0}, "cell_size": "wide"}),
        // 格数超 10000：200×200 / 1 = 40000 格。
        json!({"region": {"x": 0.0, "y": 0.0, "w": 200.0, "h": 200.0}, "cell_size": 1.0}),
    ];
    for args in malformed {
        let map = heatmap(&mut workspace, args.clone());
        assert_eq!(
            map["ok"],
            json!(false),
            "畸形输入 {args} 被接受了 ⇒ 静默返回：{map}"
        );
        assert_eq!(
            map["error_code"],
            json!("invalid_argument"),
            "畸形输入 {args} 的错误码必须是 invalid_argument：{map}"
        );
        assert!(
            map.get("cells").is_none(),
            "报错时不应附带 cells（否则调用方可能误用）：{map}"
        );
    }
}

#[test]
fn reverted_stroke_is_excluded_like_get_stroke_log() {
    let mut workspace = workspace();
    draw(&mut workspace, "near", json!([[10.0, 10.0], [30.0, 10.0]]));
    let log = call(&mut workspace, "get_stroke_log", json!({}));
    assert_eq!(log["total"], json!(1), "{log}");
    let atom_id = log["entries"][0]["atom_id"].as_str().unwrap().to_owned();
    let undone = call(&mut workspace, "revert", json!({"atom_id": atom_id}));
    assert_eq!(undone["ok"], json!(true), "{undone}");

    let map = heatmap(
        &mut workspace,
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}, "cell_size": 50.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(map["total_strokes"], json!(0), "{map}");
    assert_eq!(map["total_points"], json!(0), "{map}");
    assert_eq!(map["strokes_without_ink"], json!(0), "{map}");
}

#[test]
fn imported_image_without_source_is_not_a_stroke() {
    let mut workspace = workspace();
    let created = call(&mut workspace, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    // 16×16 全不透明原图：与 medium_stroke 的原子同形（ImportImage、无 source）。
    let bytes: Vec<u8> = (0..16 * 16).flat_map(|_| [200u8, 30, 40, 255]).collect();
    let blob = workspace
        .store()
        .put(&bytes)
        .expect("blob 应能入库")
        .to_string();
    let imported = call(
        &mut workspace,
        "import_image",
        json!({"layer_id": "L", "object_id": "img1",
               "bitmap": {"blob_hash": blob, "size": 1024, "mime_type": "image/x-yanshi-raw"},
               "region": {"x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0}}),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");

    let map = heatmap(
        &mut workspace,
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}, "cell_size": 50.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(
        map["total_strokes"],
        json!(0),
        "导入图没有 source ⇒ 不是笔，热力图必须为 0：{map}"
    );
    assert_eq!(map["total_ink_mass"], json!(0), "{map}");
}

#[test]
fn analyze_region_receipt_is_untouched() {
    let mut workspace = workspace();
    two_strokes(&mut workspace);

    let analyzed = call(
        &mut workspace,
        "analyze_region",
        json!({"region": {"x": 0.0, "y": 0.0, "w": 100.0, "h": 100.0}}),
    );
    assert_eq!(analyzed["ok"], json!(true), "{analyzed}");
    assert!(analyzed["dominant_colors"].is_array(), "{analyzed}");
    assert!(analyzed["avg_brightness"].is_number(), "{analyzed}");
    assert!(analyzed["warm_cool_ratio"].is_number(), "{analyzed}");
    assert!(analyzed["warm_pixels"].is_number(), "{analyzed}");
    assert!(analyzed["cool_pixels"].is_number(), "{analyzed}");
    assert!(
        analyzed.get("cells").is_none() && analyzed.get("total_strokes").is_none(),
        "analyze_region 不应沾上热力图字段：{analyzed}"
    );
}

/// **跑完即删临时目录**（第 817 轮；`/tmp` 是 1.9 GB tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn brush_workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_f56_brush", 400, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn brush_call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_f56_brush", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

#[test]
fn brush_strokes_report_exact_receipt_ink_per_cell() {
    let root = std::env::temp_dir().join(format!("yanshi_f56_brush_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let created = brush_call(&mut workspace, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    // 两笔分处两格：b1 的 tile 在 x 10..90、y 30..90（(0,0) 格内）；b2 的 tile
    // 在 x 310..390、y 120..180（(3,1) 格内）。边距留足，tile 不会跨格。
    let first = brush_call(
        &mut workspace,
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "b1", "brush": "2B_pencil", "size": 24,
               "points": [[40.0, 60.0, 0.4], [60.0, 60.0, 0.9]]}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let ink_first = first["ink_mass"].as_u64().expect("笔刷回执应有 ink_mass");
    assert!(ink_first > 0, "笔刷应落下墨：{first}");
    let second = brush_call(
        &mut workspace,
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "b2", "brush": "2B_pencil", "size": 24,
               "points": [[340.0, 150.0, 0.4], [360.0, 150.0, 0.9]]}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    let ink_second = second["ink_mass"].as_u64().expect("笔刷回执应有 ink_mass");
    assert!(ink_second > 0, "笔刷应落下墨：{second}");

    let map = brush_call(
        &mut workspace,
        "stroke_heatmap",
        json!({"region": {"x": 0.0, "y": 0.0, "w": 400.0, "h": 200.0}, "cell_size": 100.0}),
    );
    assert_eq!(map["ok"], json!(true), "{map}");
    assert_eq!(map["cols"], json!(4), "{map}");
    assert_eq!(map["rows"], json!(2), "{map}");
    assert_eq!(map["total_strokes"], json!(2), "{map}");
    assert_eq!(map["strokes_without_ink"], json!(0), "{map}");
    // **数据级恒等式**：区域盖住整幅画布 ⇒ 热力图墨量之和等于两笔回执墨量之和，
    // 且每格墨量精确等于对应那笔的回执值（不是“大于 0”这种安慰剂断言）。
    assert_eq!(
        map["total_ink_mass"],
        json!(ink_first + ink_second),
        "全画布区域 ⇒ 热力图总墨量必须等于两笔回执之和：{map}"
    );
    assert_eq!(cell(&map, 0, 0)["ink_mass"], json!(ink_first), "{map}");
    assert_eq!(cell(&map, 0, 0)["strokes"], json!(1), "{map}");
    assert_eq!(cell(&map, 3, 1)["ink_mass"], json!(ink_second), "{map}");
    assert_eq!(cell(&map, 3, 1)["strokes"], json!(1), "{map}");
    // 其余格全 0（笔数与墨量双 0）。
    for (col, row) in [(1, 0), (2, 0), (3, 0), (0, 1), (1, 1), (2, 1)] {
        assert_eq!(cell(&map, col, row)["strokes"], json!(0), "{map}");
        assert_eq!(cell(&map, col, row)["ink_mass"], json!(0), "{map}");
    }
    let cell_ink_sum: u64 = map["cells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["ink_mass"].as_u64().unwrap())
        .sum();
    assert_eq!(
        cell_ink_sum,
        map["total_ink_mass"].as_u64().unwrap(),
        "各格墨量之和必须等于合计：{map}"
    );
}
