//! **`brush_stroke` 回执 `max_channel_delta` 必须与导出光栅同一口径**
//! （名画临摹第 3 轮 §7 glazing 项；第 3 轮量测 `MEASUREMENTS-ROUND3.md`
//! 「D 期遗留：glazing 回执 max_channel_delta 254」）。
//!
//! **复现（修复前，9045faeb 量测值）**：p4d 几何（纸色底 215/198/168，
//! 线性中墨，size 90，opacity 0.6，dabs 3/3，对角 130px 笔）——
//! 纸色底 glazing 回执 delta 254（median 2，p90 244，p99 254），
//! 导出走廊亮于纸色像素 0 个、最大差仅 4 级；
//! Round_Bl 回执 delta 205，导出最大差 18 级。
//!
//! **根因**：`paint_brush` 的喂底图分支拿直通 tile 的 RGBA 与落笔前像素
//! 逐通道直比（含 alpha 通道、未做 source-over 合成），而导出图是
//! tile 按 Normal source-over 合成后的光栅。涂抹笔晕边低 alpha 像素
//! 的 alpha 差（|a − 255| ≈ 244–254）在导出 RGB 里根本不出现。
//! draw 路（`draw_evidence_compare`）早就是“合成后再比 RGB”口径，
//! 本测试钉住 brush 路与它一致。
//!
//! **判据（恒等式，跨输入成立）**：同一笔的回执 `max_channel_delta`
//! 与导出 PNG 对底色的最大 RGB 通道差之差 ≤ 容差。容差取 8 级：
//! 回执侧底图经 fix15 读回（±1–2 级量化）与导出侧折叠渲染
//! （±1–2 级）各有量化噪声，8 级是两者噪声之和再留余量；
//! 修复前差值是 250 级（glazing）与 187 级（Round_Bl），
//! 容差 8 级不可能把旧口径放过去。
//!
//! **变异**：把喂底图分支改回“直通 RGBA 逐通道直比（含 alpha）”⇒
//! 两测试都红（glazing 回执 254 对导出 4，Round_Bl 回执 205 对导出 18）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录**（与 `ink_pixels` 同款；导出 PNG 落临时路径）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace(tag: &str) -> (Workspace, Cleanup) {
    let root = std::env::temp_dir().join(format!(
        "yanshi_receipt_caliber_{tag}_{}",
        std::process::id()
    ));
    let cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    assert!(assets.is_dir(), "找不到资产根：{}", assets.display());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_caliber", 400, 720)
                .with_background(json!({"r": 215, "g": 198, "b": 168, "a": 255})),
            "human:1",
            "s",
        )
        .expect("create_document");
    (workspace, cleanup)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_caliber", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    ToolRegistry::with_profiles(&Profile::ALL).call(&mut ctx, tool, &args)
}

/// 第 3 轮 p4d 几何的点列（量测脚本 `line_pts4` 的输出值；第 4 分量
/// time_ms 在本引擎 `dabs_per_second = 0` 时不影响出 dab，故只传前三项）。
const GLAZING_POINTS: [[f64; 3]; 4] = [
    [150.0, 570.0, 0.496],
    [193.3, 590.0, 0.656],
    [236.7, 610.0, 0.786],
    [280.0, 630.0, 0.35],
];
const ROUND_BL_POINTS: [[f64; 3]; 4] = [
    [150.0, 270.0, 0.496],
    [193.3, 290.0, 0.656],
    [236.7, 310.0, 0.786],
    [280.0, 330.0, 0.35],
];

/// 一笔的回执 delta 与导出 PNG 对底色的最大 RGB 差必须在容差内。
fn check_case(tag: &str, brush: &str, points: [[f64; 3]; 4]) {
    let (mut workspace, _cleanup) = workspace(tag);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": brush,
            // 线性中墨（量测脚本 MID = lin(148, 136, 114)）。
            "color": [0.2961, 0.2462, 0.1683],
            "size": 90.0, "points": points,
            "opacity": 0.6,
            "brush_params": {"dabs_per_actual_radius": 3, "dabs_per_basic_radius": 3},
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // **必须走喂底图分支**，否则本测试量的是另一条路（无基准分支本来就
    // 合成到背景再比，旧口径下也会“碰巧”过）。
    assert_eq!(
        made["fed_base"],
        json!(true),
        "涂抹笔必须喂底图（fed_base=true），否则本测试没有量到目标分支：{made}"
    );
    assert_eq!(
        made["delta_regime"],
        json!("canvas_base"),
        "喂底图分支的 regime 必须是 canvas_base：{made}"
    );
    let receipt_delta = made["max_channel_delta"]
        .as_u64()
        .expect("max_channel_delta 必须是数值");
    let path = std::env::temp_dir().join(format!(
        "yanshi_receipt_caliber_{tag}_{}.png",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let exported = call(
        &mut workspace,
        "export_png",
        json!({ "path": path.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let (width, height, rgba) =
        yanshi_render::png::decode_png(&std::fs::read(&path).expect("导出 PNG 必须落盘"))
            .expect("导出必须是能解码的 PNG");
    let _ = std::fs::remove_file(&path);
    assert_eq!((width, height), (400, 720), "导出尺寸必须是画布尺寸");
    let mut export_max: u64 = 0;
    for px in rgba.chunks_exact(4) {
        for (channel, base) in [(0, 215u16), (1, 198u16), (2, 168u16)] {
            let diff = (i32::from(px[channel]) - base as i32).unsigned_abs() as u64;
            export_max = export_max.max(diff);
        }
    }
    println!("{brush}：回执 max_channel_delta={receipt_delta}，导出最大 RGB 差={export_max}");
    assert!(
        receipt_delta.abs_diff(export_max) <= 8,
        "{brush}：回执 max_channel_delta（{receipt_delta}）与导出最大 RGB 差（{export_max}）\
         必须在 8 级内（同一口径），实测差 {}",
        receipt_delta.abs_diff(export_max)
    );
}

/// **glazing 案**：修复前回执 254、导出最大差 4 级（差 250 级）。
#[test]
fn glazing_receipt_delta_matches_export_raster() {
    check_case("glazing", "watercolor_glazing.myb", GLAZING_POINTS);
}

/// **Round_Bl 案**：修复前回执 205、导出最大差 18 级（差 187 级）。
#[test]
fn round_bl_receipt_delta_matches_export_raster() {
    check_case("roundbl", "Round_Bl.myb", ROUND_BL_POINTS);
}
