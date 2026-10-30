//! **P-15：`max_channel_delta` 给分布，不只给单像素最大值** ✓
//!
//! **判据** ✓：
//! ① 淡色笔（分诊的输入：`[0.98, 0.98, 0.98]`、size 30、oil-06-paint）⇒
//!    实测 `max_channel_delta == 2`、`delta_median == 2`、`delta_p90 == 2`、
//!    `delta_p99 == 2`、`changed_pixels > 0`（D4b 后 oil-06 的 smudge 喂底图，delta 按
//!    canvas_base 算，全是 1，证明直方图真的在数数；Y26 pale_wash 修复后参照改画前
//!    合成（真白 255，非喂底图读回的量化 254）⇒ 四个数由 1 变 2，档位仍是淡色小差）。
//!    注意：分诊报的是 `max == 1`（`painted == 1110`），本输入实测 `max == 2`
//!   （`painted == 8270`，几何不同：240px 长线 vs 分诊的短笔）；数值的档位（淡色小差）
//!    一致，断言以本轮实测为准。
//! ② 软笔深色大笔（`100%_Opaque`、黑色、size 60、hardness 0.5）⇒ `max == 255` 而
//!    `0 < delta_median < 255`（边缘渐变 ⇒ 中位值被最大值甩开，证明 median 不是 max 的别名）、
//!    `changed_pixels > 0`、`changed_pixels <= painted_pixels_clipped`。
//!    注意：`hardness == 0.0` 时这支笔一墨不出（`precondition_failed`，
//!    报错把原因写成“路径撑不满 dab 间距”——那是报错文案的老问题，与本期无关，
//!    见 NOTES；本测试用 `hardness == 0.5`，实测 median 253）。
//! ③ `medium_stroke`（oil）⇒ `paint_delta_median`／`paint_delta_p90`／`paint_delta_p99`／
//!    `changed_pixels` 都是数值，`median <= paint_delta`，`changed_pixels > 0`。
//! ④ `brush_preview` ⇒ 同一套分布字段都是数值（与 `brush_stroke` 同一套字段）。
//!
//! **变异** ✓：把 `delta_distribution` 的分位数改成恒返 `max` ⇒ ② 红（median 变 255）；
//! 把直方图累加删掉 ⇒ ①②③④ 红（changed 变 0）；
//! 把 `count_painted_and_delta` 的直方图删掉 ⇒ ③ 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `brush_stroke` 要从那儿读 `.myb` 笔刷 ✓。
fn workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_p15_{name}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_p15", 320, 240),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_p15", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn setup_layer(workspace: &mut Workspace) {
    assert_eq!(
        call(workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
}

/// **① 淡色笔 ⇒ `max == 2` 且分布全是 2** ✓（分诊复现的原样输入）。
/// **① 淡色笔 ⇒ `max == 2`、`delta_median == 2`、`delta_p90/p99 == 2** ✓
/// （D4b 后 canvas_base 口径；Y26 后画前合成参照，实测值见文件头注）。
#[test]
fn pale_wash_reports_small_uniform_delta() {
    let mut workspace = workspace("pale");
    setup_layer(&mut workspace);
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "oil-06-paint",
            "points": [[40.0, 60.0, 1.0], [280.0, 60.0, 1.0]], "size": 30,
            "color": [0.98, 0.98, 0.98],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        made["max_channel_delta"],
        json!(2),
        "淡色笔只得 2（本轮实测）：{made}"
    );
    // D4b 后：oil-06 的 smudge（base 0 + stroke 曲线输入）翻为 fed_base=true，
    // delta 按 canvas_base 算 ⇒ 四个数全由 2 落到 1（预期后果，不是回归）。
    // Y26 pale_wash：参照改画前合成（真白 255）后四个数由 1 变 2——漆色
    // sRGB ≈ 253（0.98 linear），对真白差 2；旧 1 是喂底图读回的量化白 254
    // 与截断合成共同作用的结果，不是另一档位（changed 像素数与均匀性不变）。
    assert_eq!(made["delta_median"], json!(2), "{made}");
    assert_eq!(made["delta_p90"], json!(2), "{made}");
    assert_eq!(made["delta_p99"], json!(2), "{made}");
    let changed = made["changed_pixels"]
        .as_u64()
        .expect("changed_pixels 必须是数值");
    assert!(changed > 0, "淡色笔应改了像素：{made}");
}

/// **② 软笔深色大笔 ⇒ `max == 255` 而 `median < 255`** ✓（median 不是 max 的别名）。
///
/// 大笔（size 60）的 dab 间距大 ⇒ 两点直线撑不满间距时反馈期 1 会兜底落一枚 dab
/// （带提示，不再拒）⇒ 为画出完整笔形，点列仍每 15px 加密一个点，让路径达到间距的数倍。
/// 另：`hardness == 0.0` 时这支笔一墨不出（探针实测，四个 size 全拒）⇒ 用 0.5。
#[test]
fn soft_dark_stroke_median_is_below_max() {
    let mut workspace = workspace("soft");
    setup_layer(&mut workspace);
    let points: Vec<[f64; 3]> = (0..=18)
        .map(|i| [20.0 + 15.0 * i as f64, 120.0, 1.0])
        .collect();
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque",
            "points": points, "size": 60,
            "hardness": 0.5,
            "color": [0.0, 0.0, 0.0],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["max_channel_delta"], json!(255), "{made}");
    let median = made["delta_median"]
        .as_u64()
        .expect("delta_median 必须是数值");
    assert!(
        median > 0 && median < 255,
        "软笔边缘渐变 ⇒ 中位值应在 (0, 255) 内（本轮实测 253）：{made}"
    );
    let p90 = made["delta_p90"].as_u64().expect("delta_p90 必须是数值");
    let p99 = made["delta_p99"].as_u64().expect("delta_p99 必须是数值");
    assert!(median <= p90 && p90 <= p99 && p99 <= 255, "{made}");
    let changed = made["changed_pixels"]
        .as_u64()
        .expect("changed_pixels 必须是数值");
    assert!(changed > 0, "{made}");
    assert!(
        changed <= made["painted_pixels_clipped"].as_u64().unwrap(),
        "改了的像素不应超过覆盖：{made}"
    );
}

/// **③ `medium_stroke` ⇒ 同一套分布字段（`paint_delta_*` 命名）** ✓。
#[test]
fn medium_stroke_reports_paint_delta_distribution() {
    let mut workspace = workspace("medium");
    setup_layer(&mut workspace);
    let made = call(
        &mut workspace,
        "medium_stroke",
        json!({
            "layer_id": "L", "medium": "oil",
            "points": [[40.0, 120.0, 1.0], [280.0, 120.0, 1.0]], "size": 30,
            "color": [0.0, 0.0, 0.0],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let delta = made["paint_delta"]
        .as_u64()
        .expect("paint_delta 必须是数值");
    assert!(delta > 0, "{made}");
    let median = made["paint_delta_median"]
        .as_u64()
        .expect("paint_delta_median 必须是数值");
    let p90 = made["paint_delta_p90"]
        .as_u64()
        .expect("paint_delta_p90 必须是数值");
    let p99 = made["paint_delta_p99"]
        .as_u64()
        .expect("paint_delta_p99 必须是数值");
    assert!(median <= p90 && p90 <= p99 && p99 <= delta, "{made}");
    let changed = made["changed_pixels"]
        .as_u64()
        .expect("changed_pixels 必须是数值");
    assert!(changed > 0, "{made}");
}

/// **④ `brush_preview` ⇒ 同一套分布字段** ✓（与 `brush_stroke` 同一套字段）。
#[test]
fn brush_preview_reports_the_same_distribution_fields() {
    let mut workspace = workspace("preview");
    setup_layer(&mut workspace);
    let made = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "100%_Opaque",
            "points": [[40.0, 120.0, 1.0], [280.0, 120.0, 1.0]], "size": 30,
            "color": [0.0, 0.0, 0.0],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert!(made["max_channel_delta"].is_number(), "{made}");
    for key in ["delta_median", "delta_p90", "delta_p99", "changed_pixels"] {
        assert!(made[key].is_number(), "{key} 必须是数值：{made}");
    }
}

/// **反馈期 1（3.6a）：短笔触兜底——工具级** ✓。
///
/// 口径：`100%_Opaque`、size 400（dab 间距约 45.7px）＋ 两点 4.2px 短路径，
/// 旧行为是 `precondition_failed` 拒绝；新行为是 ok:true、painted_pixels > 0，
/// 且 `warnings` 里有兜底提示（含“dab 间距”与“路径总长”诊断事实）。
/// 对照：280px 正常路径同样 ok，但 `warnings` 里不许出现 dab 间距提示
/// （正常笔画行为不变、也不带兜底提示）。
#[test]
fn short_stroke_lands_one_dab_with_hint() {
    let mut workspace = workspace("shortdab");
    setup_layer(&mut workspace);
    let short = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque",
            "points": [[100.0, 100.0, 1.0], [103.0, 103.0, 1.0]], "size": 400,
            "color": [0.0, 0.0, 0.0], "preview": false,
        }),
    );
    assert_eq!(short["ok"], json!(true), "短路径不再拒绝：{short}");
    assert!(
        short["painted_pixels"].as_u64().unwrap_or(0) > 0,
        "短路径至少落一枚 dab ⇒ painted_pixels > 0：{short}"
    );
    let warnings: Vec<String> = short["warnings"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("dab 间距") && w.contains("路径总长")),
        "短笔触回执必须带兜底提示（诊断事实保留在 warnings 里）：{warnings:?}"
    );
    let normal = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque",
            "points": [[20.0, 120.0, 1.0], [300.0, 120.0, 1.0]], "size": 400,
            "color": [0.0, 0.0, 0.0], "preview": false,
        }),
    );
    assert_eq!(normal["ok"], json!(true), "正常路径仍成功：{normal}");
    let normal_warnings: Vec<String> = normal["warnings"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !normal_warnings.iter().any(|w| w.contains("dab 间距")),
        "正常路径不许带兜底提示：{normal_warnings:?}"
    );
}
