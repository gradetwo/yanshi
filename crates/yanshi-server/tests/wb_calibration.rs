//! C1 阶段 3 标定网格（`wb_calibration`）：信号带 × Planck 门的回测标定。
//!
//! **门控**：输入在仓库外，CI 没有它们。
//! - `YANSHI_C1_GOLD_DIR`：5 张金参考 RAW 所在目录（`downloads-arw/` 口径；
//!   DSC01242/01244 ARW、forest DNG、bathroom CR2、beamcreek ARW）。
//! - `YANSHI_AUTO_BACKTEST_DIR`：showcase 根目录（`downloads/` 下 3 张回测
//!   触点 RAW：ricketts CR2、blown_sky ARW、night ORF）。
//!
//! 任一变量未设置/指向不存在的目录时**跳过**（打印原因后返回）；指向存在
//! 的目录但文件缺失时**响亮失败**。
//!
//! 方法：每张图导入后全幅渲染一次（as-shot，`lens_correction=auto`，与 R10
//! 探针同口径），再对同一像素阵列扫 27 组统计参数
//! （band_low {0.01, 0.02, 0.03} × band_high {0.40, 0.50, 0.60} ×
//!  δ {0.08, 0.12, 0.16}），逐组打印锚点表（CALGRID 行，供
//!  `wb_calibration_counterfactual.py` 反事实计算落成 WB 与 Δ金）。
//! 统计参数只改变锚点集（count/band/recall/median/spread）；舞台标志
//! （hue 直方图 max + luma p50）与锚点参数无关，每图只算一次并随行打印。
//!
//! 本测试不判阈值——阈值定版由标定报告（回测证据）决定；它只钉住两点：
//! 1. 缺省组合 (0.02, 0.50, 0.12) 的锚点表须与 R10 基线探针逐项一致
//!    （测量工具先被测量；不一致即停，不标定）；
//! 2. 5 张金 + colorcast 的舞台标志须与基线一致（01242 与 colorcast 为
//!    舞台，其余非舞台）。
//!
//! 阈值定版后，本测试的缺省组合断言即为该定版的回归守卫。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_render::stats::{image_stats_rgba8, image_stats_rgba8_with_params, WbAnchorParams};
use yanshi_server::tools::box_downsample;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 代理图长边上限（须与 `tools_auto.rs` 的 `PROXY_MAX_EDGE` 一致；
/// 缺省组合钉住 R10 基线会捕获漂移）。
const PROXY_MAX_EDGE: u32 = 1024;

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_cal_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 标定图：金参考（有金 WB + 基线锚点表钉住）或回测触点（只打印）。
struct CalImage {
    name: &'static str,
    /// 所在目录下文件名。
    file: &'static str,
    /// 画布（rawpy `sizes` 宽高，未换轴；探针口径——`drive.native_dims` 的
    /// `raw.sizes` + flip 换轴。rawler 的 `decode_bytes` 帧尺寸与 rawpy
    /// `sizes` 在多数图上一致，唯 beamcreek 差 24px（6024 vs 6048），
    /// 画布不同 ⇒ 降采样网格错位 ⇒ 暗图带计数差 0.55%（实测），故画布
    /// 必须用探针口径，不能用帧尺寸。方向换轴（5–8）运行时按
    /// `decode_bytes` 的 orientation 做，与探针的 flip 换轴同义。
    /// 注意：`raw_dims.py` 量的不是 `sizes`（它是另一口径），不许用它。）
    canvas: (u32, u32),
    /// 金 WB (T, Ti)；触点为 None。
    gold: Option<(f32, f32)>,
    /// 基线锚点表（R10 探针，缺省组合）：(band, count, median_u, median_v, spread)。
    /// 有基线者钉住；无基线者只打印。
    baseline: Option<(usize, usize, f32, f32, f32)>,
    /// 舞台标志期望值（None = 只打印不判）。
    stage_expected: Option<bool>,
}

const GOLD_IMAGES: [CalImage; 5] = [
    CalImage {
        name: "DSC01242",
        file: "DSC01242.ARW",
        canvas: (7968, 5320),
        gold: Some((0.02, 0.0)),
        baseline: Some((466817, 295298, 0.174, -0.208, 0.083)),
        stage_expected: Some(true),
    },
    CalImage {
        name: "DSC01244",
        file: "DSC01244.ARW",
        canvas: (7968, 5320),
        gold: Some((0.02, 0.0)),
        baseline: Some((421218, 349957, 0.155, -0.183, 0.072)),
        stage_expected: Some(false),
    },
    CalImage {
        name: "forest",
        file: "leicaSL_Lechtaler_001.DNG",
        canvas: (6016, 4016),
        gold: Some((0.03, 0.0)),
        baseline: Some((658716, 276421, 0.024, -0.067, 0.088)),
        stage_expected: Some(false),
    },
    CalImage {
        name: "bathroom",
        file: "canon5d3_5G4A9394.CR2",
        canvas: (5796, 3870),
        gold: Some((0.02, 0.0)),
        baseline: Some((633368, 360716, 0.584, -0.646, 0.089)),
        stage_expected: Some(false),
    },
    CalImage {
        name: "beamcreek",
        file: "hdr_beamcreek.ARW",
        canvas: (6024, 4024),
        gold: Some((0.0, 0.0)),
        baseline: Some((246636, 157337, 0.027, -0.030, 0.123)),
        stage_expected: Some(false),
    },
];

const TOUCHPOINT_IMAGES: [CalImage; 3] = [
    CalImage {
        name: "underexposed",
        file: "ricketts_glen_0006.CR2",
        canvas: (6024, 4020),
        gold: None,
        baseline: None,
        stage_expected: None,
    },
    CalImage {
        name: "overexposed",
        file: "blown_sky_a6000.ARW",
        canvas: (6024, 4024),
        gold: None,
        baseline: None,
        stage_expected: None,
    },
    CalImage {
        name: "colorcast",
        file: "20221118_0018.ORF",
        canvas: (5240, 3912),
        gold: None,
        // facade 探针口径（lens=auto）的基线锚点表。
        baseline: Some((209930, 99675, 0.226, -0.263, 0.155)),
        stage_expected: Some(true),
    },
];

fn run_one(dir: &std::path::Path, image: &CalImage) {
    let raw_path = dir.join(image.file);
    assert!(
        raw_path.is_file(),
        "标定输入缺失（承诺了目录却没有文件）：{}",
        raw_path.display()
    );
    let raw_bytes = std::fs::read(&raw_path).expect("读 RAW");
    let decoded = yanshi_raw::decode_bytes(&raw_bytes).expect("解码 RAW 取方向");
    // 画布 = 硬编码的 rawpy 活动区尺寸（见 `CalImage::canvas`）+ 方向换轴；
    // 不用 `decoded.frame` 的全幅尺寸（与 rawpy 差数十至上百像素，见上）。
    let (mut dev_w, mut dev_h) = image.canvas;
    if (5..=8).contains(&decoded.file_meta.orientation) {
        std::mem::swap(&mut dev_w, &mut dev_h);
    }
    eprintln!(
        "CALGRID {}：{} 帧 {}×{} orientation={} ⇒ 画布 {}×{}",
        image.name,
        image.file,
        decoded.frame.width,
        decoded.frame.height,
        decoded.file_meta.orientation,
        dev_w,
        dev_h
    );

    let dir = temp_dir(image.name);
    let _cleanup = Cleanup(dir.clone());
    let doc = format!("doc_cal_{}", image.name);
    let mut workspace = Workspace::with_file_store(dir.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new(&doc, dev_w, dev_h),
            "human:1",
            "session:test",
        )
        .unwrap();
    let layer = call(
        &mut workspace,
        &doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let hash = workspace.put_blob(&raw_bytes).expect("放 RAW blob");
    let imported = call(
        &mut workspace,
        &doc,
        "import_image",
        json!({
            "layer_id": "src",
            "object_id": "photo",
            "bitmap": {"blob_hash": hash, "size": raw_bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0, "y": 0, "w": dev_w, "h": dev_h},
            "raw_options": {"lens_correction": "auto"},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "RAW 导入应成功：{imported}");
    // 删缺省曝光对象（与 R10 探针 `retouch.import_photo` 同口径）：
    // RAW 导入缺省烘焙 +0.5EV（`spawn_default_exposure_adjustment`），
    // 不删则统计口径为 harness 口径（带烘焙），与探针口径的基线锚点表
    // 不一致——本测试第二版即因此在 01242 上 p50 差 1 LSB（64 vs ≤63），
    // 舞台标志翻转。删后渲染即探针口径（无烘焙）。
    if let Some(object_id) = imported["default_adjustment"]["object_id"].as_str() {
        let deleted = call(
            &mut workspace,
            &doc,
            "delete_object",
            json!({"object_id": object_id}),
        );
        assert_eq!(
            deleted["ok"],
            json!(true),
            "删缺省曝光对象应成功：{deleted}"
        );
    }

    // 全幅渲染一次（as-shot），再按生产口径降采样到 ≤1024 代理；
    // 27 组参数共用同一代理像素阵列（统计层只在代理上算，见 tools_auto.rs
    // 的 PROXY_MAX_EDGE；全幅像素直接算会得到另一套 hue/p50，舞台标志
    // 与锚点数皆与生产不一致——本测试的第一版即因此在 01242 上失败）。
    // 渲染用 `render_layer_objects_subset`（生产 auto 的同一入口：只渲
    // 源图层 RasterPatch 对象、背景置空），不用 `render_region_raw`
    // （后者会把缺省背景与调整对象合成进来；beamcreek 等暗图在带沿
    // 附近的像素多，0.55% 的带计数差即由此来）。
    let bbox = Bbox::new(0.0, 0.0, dev_w as f64, dev_h as f64);
    let mut keep = std::collections::BTreeSet::new();
    keep.insert("photo".to_string());
    let (fw, fh, full_pixels) = workspace
        .render_layer_objects_subset(&doc, bbox, "src", &keep)
        .expect("全幅渲染");
    let (pixels, rw, rh) = box_downsample(&full_pixels, fw, fh, PROXY_MAX_EDGE);
    drop(full_pixels);
    let default = image_stats_rgba8(&pixels, rw, rh);
    assert!(
        default.wb_anchor_count > 0,
        "CALGRID {}：缺省组合锚点集不许为空（基线探针有锚点）",
        image.name
    );
    // 舞台标志（与锚点参数无关）：hue 直方图最大桶 > 0.45 且 luma p50 < 0.25。
    let max_hue = default
        .hue_histogram
        .iter()
        .fold(0.0f32, |acc, &share| acc.max(share));
    let p50 = default.luma_p50 as f32 / 255.0;
    let stage = max_hue > 0.45 && p50 < 0.25;
    eprintln!(
        "CALGRID {}：stage={}（max_hue={:.3} p50={:.3}）",
        image.name, stage, max_hue, p50
    );
    if let Some(expected) = image.stage_expected {
        assert_eq!(
            stage, expected,
            "CALGRID {}：舞台标志应与基线一致（期望 {expected}，实得 {stage}；max_hue={max_hue:.3} p50={p50:.3}）",
            image.name
        );
    }
    if let Some((t, ti)) = image.gold {
        eprintln!("CALGOLD {} T={:.2} Ti={:.2}", image.name, t, ti);
    }
    // 缺省组合钉住基线（测量工具先被测量）。
    if let Some((band, count, mu, mv, spread)) = image.baseline {
        assert_eq!(
            default.wb_anchor_band_count, band,
            "CALGRID {}：缺省组合 band 像素数应与 R10 基线一致",
            image.name
        );
        assert_eq!(
            default.wb_anchor_count, count,
            "CALGRID {}：缺省组合锚点数应与 R10 基线一致",
            image.name
        );
        for (label, got, want) in [
            ("median_u", default.wb_anchor_chroma_median[0], mu),
            ("median_v", default.wb_anchor_chroma_median[1], mv),
            ("spread", default.wb_anchor_chroma_spread, spread),
        ] {
            assert!(
                (got - want).abs() <= 0.002,
                "CALGRID {}：缺省组合 {label} 应与 R10 基线一致（期望 {want:.3}，实得 {got:.3}）",
                image.name
            );
        }
    }

    for lo in [0.01f32, 0.02, 0.03] {
        for hi in [0.40f32, 0.50, 0.60] {
            for delta in [0.08f32, 0.12, 0.16] {
                let params = WbAnchorParams {
                    luma_band: (lo, hi),
                    planck_delta: delta,
                };
                let stats = image_stats_rgba8_with_params(&pixels, rw, rh, &params);
                let recall = if stats.wb_anchor_band_count > 0 {
                    stats.wb_anchor_count as f32 / stats.wb_anchor_band_count as f32
                } else {
                    0.0
                };
                eprintln!(
                    "CALGRID {} lo={:.2} hi={:.2} delta={:.2} count={} band={} recall={:.4} mu={:.4} mv={:.4} spread={:.4} stage={}",
                    image.name,
                    lo,
                    hi,
                    delta,
                    stats.wb_anchor_count,
                    stats.wb_anchor_band_count,
                    recall,
                    stats.wb_anchor_chroma_median[0],
                    stats.wb_anchor_chroma_median[1],
                    stats.wb_anchor_chroma_spread,
                    stage as u8
                );
            }
        }
    }
}

#[test]
fn wb_calibration_grid() {
    let Ok(gold_dir) = std::env::var("YANSHI_C1_GOLD_DIR") else {
        eprintln!("CALGRID SKIP：未设置 YANSHI_C1_GOLD_DIR（金参考 RAW 在仓库外，CI 无此数据）");
        return;
    };
    let gold_dir = std::path::PathBuf::from(gold_dir);
    if !gold_dir.is_dir() {
        eprintln!(
            "CALGRID SKIP：YANSHI_C1_GOLD_DIR 指向不存在的目录：{}",
            gold_dir.display()
        );
        return;
    }
    let Ok(backtest_dir) = std::env::var("YANSHI_AUTO_BACKTEST_DIR") else {
        eprintln!("CALGRID SKIP：未设置 YANSHI_AUTO_BACKTEST_DIR（回测触点 RAW 在仓库外）");
        return;
    };
    let backtest_dir = std::path::PathBuf::from(backtest_dir);
    if !backtest_dir.is_dir() {
        eprintln!(
            "CALGRID SKIP：YANSHI_AUTO_BACKTEST_DIR 指向不存在的目录：{}",
            backtest_dir.display()
        );
        return;
    }
    let downloads = backtest_dir.join("downloads");
    for image in GOLD_IMAGES.iter() {
        run_one(&gold_dir, image);
    }
    for image in TOUCHPOINT_IMAGES.iter() {
        run_one(&downloads, image);
    }
    eprintln!("CALGRID DONE：8 图 × 27 组合");
}
