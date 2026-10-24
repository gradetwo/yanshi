//! C1-a 判据：展开缓冲复用（⑥ 原地矩阵 + ⑧ 前释放上游缓冲 + 方向
//! 恒等跳拷贝）必须**逐位守恒**。金值 = 改造前实现在同一合成帧上的
//! 输出 FNV-1a 哈希（盒 yanshi-dev、release、基线 d9bb487 实测回填）；
//! 任一像素漂移都会翻哈希。探针帧（左红右蓝）专守「原地变换误读
//! 已写像素」这类跨像素污染——污染一出现，哈希与探针断言一起红。
//!
//! 变异：把 ⑥ 改成「先写 R 再用新 R 算 G」的跨分量误读 ⇒ 本文件红。

use yanshi_raw::develop::{develop, develop_with_lens, LensExecution};
use yanshi_raw::frame::{
    AppliedLensProfile, CalibrationMatrix, CfaGrid, CfaPattern, Channel, LensCorrectionPlan,
    RecordContext, VignetteModel, WarpModel,
};
use yanshi_raw::{DevelopOptions, HighlightMode, RawFrame};

fn fnv1a(values: &[f32]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for v in values {
        for byte in v.to_bits().to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// 确定性伪随机马赛克（LCG，原始计数域 0..16000）。
fn lcg_mosaic(w: usize, h: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..w * h)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((state >> 8) & 0x3fff) as f32
        })
        .collect()
}

fn calibration(kelvin: f32, m: [[f32; 3]; 3]) -> CalibrationMatrix {
    CalibrationMatrix {
        illuminant_kelvin: Some(kelvin),
        xyz_to_cam: m,
    }
}

fn bayer_frame(orientation: u32) -> RawFrame {
    let (w, h) = (64usize, 48usize);
    RawFrame {
        width: w,
        height: h,
        mosaic: lcg_mosaic(w, h, 0x2545_0001),
        cfa: CfaGrid::from_bayer(CfaPattern::Rggb),
        black_levels: vec![512.0, 500.0, 520.0, 508.0],
        white_levels: vec![16383.0, 16000.0, 16383.0, 16100.0],
        wb_multipliers: Some([1.8, 1.0, 1.5]),
        as_shot_neutral: None,
        color_matrices: vec![
            calibration(
                2856.0,
                [
                    [0.6813, -0.1485, -0.0701],
                    [-0.3975, 1.2170, 0.2892],
                    [-0.0493, 0.0813, 0.7378],
                ],
            ),
            calibration(
                6504.0,
                [
                    [0.7355, -0.1828, -0.0836],
                    [-0.4262, 1.2845, 0.2576],
                    [-0.0698, 0.1123, 0.7846],
                ],
            ),
        ],
        baseline_exposure_ev: Some(0.25),
        orientation,
    }
}

fn xtrans_frame(orientation: u32) -> RawFrame {
    let (w, h) = (60usize, 42usize);
    let cells: Vec<Channel> = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG"
        .chars()
        .map(|c| match c {
            'R' => Channel::R,
            'G' => Channel::G,
            _ => Channel::B,
        })
        .collect();
    RawFrame {
        width: w,
        height: h,
        mosaic: lcg_mosaic(w, h, 0x0bad_f00d),
        cfa: CfaGrid::new(6, cells, "xtrans-golden".to_string()).expect("grid"),
        black_levels: (0..36).map(|p| 512.0 + p as f32).collect(),
        white_levels: vec![16383.0; 36],
        wb_multipliers: Some([1.9, 1.0, 1.4]),
        as_shot_neutral: None,
        color_matrices: vec![calibration(
            6504.0,
            [
                [0.7355, -0.1828, -0.0836],
                [-0.4262, 1.2845, 0.2576],
                [-0.0698, 0.1123, 0.7846],
            ],
        )],
        baseline_exposure_ev: None,
        orientation,
    }
}

fn dummy_profile() -> AppliedLensProfile {
    serde_json::from_value(serde_json::json!({
        "source_db": "golden-fixture",
        "db_version": "1",
        "lens_id": "Golden 24mm",
        "matched_via": "manual",
        "focal_mm_effective": 24.0,
        "sources": { "vignetting": "lensfun", "distortion": "lensfun", "tca": "none" },
        "distortion_ptlens": [0.01, -0.02, 0.0],
        "vignetting_pa": [-0.30, 0.05, 0.0],
        "tca": null,
        "crop_scale_applied": 1.0,
        "crop_ratio": 1.0,
        "opcode": null,
    }))
    .expect("profile 形状合法")
}

fn chroma_options() -> DevelopOptions {
    DevelopOptions {
        highlight: HighlightMode::Chroma,
        pre_exposure_ev: 0.35,
        ..DevelopOptions::default()
    }
}

#[test]
fn develop_bayer_transpose_bit_identical() {
    // Bayer + 双标定 + Chroma 高光 + orientation=8（转置类）。
    let frame = bayer_frame(8);
    let image = develop(&frame, &chroma_options(), &RecordContext::default()).expect("展开");
    assert_eq!(
        (image.width, image.height),
        (48, 64),
        "orientation=8 宽高互换"
    );
    assert_eq!(
        fnv1a(&image.rgb_linear),
        GOLD_BAYER_ORIENT8,
        "Bayer 转置变体漂移"
    );
}

#[test]
fn develop_lens_plan_bit_identical() {
    // 镜头计划（暗角场 + warp）+ orientation=6。
    let frame = bayer_frame(6);
    let options = chroma_options();
    let plan = LensCorrectionPlan {
        vignette: Some(VignetteModel::Pa([-0.30, 0.05, 0.0])),
        warp: Some(WarpModel::PtlensTca {
            ptlens: [0.01, -0.02, 0.0],
            tca: None,
            crop_ratio: 1.0,
        }),
    };
    let profile = dummy_profile();
    let image = develop_with_lens(
        &frame,
        &options,
        &RecordContext::default(),
        Some(LensExecution {
            plan: &plan,
            profile: &profile,
            lens_match: "manual",
        }),
    )
    .expect("带镜头展开");
    assert_eq!((image.width, image.height), (48, 64));
    assert_eq!(fnv1a(&image.rgb_linear), GOLD_LENS_ORIENT6, "镜头变体漂移");
}

#[test]
fn develop_xtrans_rotate180_bit_identical() {
    // X-Trans + orientation=3（180°）。
    let frame = xtrans_frame(3);
    let image = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect("X-Trans 展开");
    assert_eq!((image.width, image.height), (60, 42));
    assert_eq!(
        fnv1a(&image.rgb_linear),
        GOLD_XTRANS_ORIENT3,
        "X-Trans 变体漂移"
    );
}

#[test]
fn develop_in_place_probe_frame_has_no_cross_pixel_bleed() {
    // 左半 R 采样位高值、右半 B 采样位高值、G 全场中值：原地矩阵步
    // 若误读已写像素，左右两半的输出会互相污染出可测的量级差。
    let (w, h) = (32usize, 16usize);
    let grid = CfaGrid::from_bayer(CfaPattern::Rggb);
    let mut mosaic = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            mosaic[y * w + x] = match (grid.channel_at(x, y), x < w / 2) {
                (Channel::R, true) => 12000.0,
                (Channel::B, false) => 12000.0,
                (Channel::G, _) => 6000.0,
                _ => 800.0,
            };
        }
    }
    let mut frame = bayer_frame(1);
    frame.width = w;
    frame.height = h;
    frame.mosaic = mosaic;
    frame.baseline_exposure_ev = Some(0.0);
    let image = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect("探针帧展开");
    assert_eq!(fnv1a(&image.rgb_linear), GOLD_PROBE, "探针帧漂移");
    // 结构断言（与金值互为表里）：左半均值 R 应显著大于右半均值 R。
    let mean_r = |x0: usize, x1: usize| -> f32 {
        let mut sum = 0.0f32;
        let mut n = 0usize;
        for y in 0..h {
            for x in x0..x1 {
                sum += image.rgb_linear[(y * w + x) * 3];
                n += 1;
            }
        }
        sum / n as f32
    };
    assert!(
        mean_r(2, w / 2 - 2) > 2.0 * mean_r(w / 2 + 2, w - 2),
        "左半 R 均值应显著大于右半（无跨像素污染）"
    );
}

// 金值（基线实现实测回填；占位 0 时对应条目必然红——故意的，先红后填）。
const GOLD_BAYER_ORIENT8: u64 = 2806226866006716599;
const GOLD_LENS_ORIENT6: u64 = 9668569528247697681;
const GOLD_XTRANS_ORIENT3: u64 = 9977003406416021175;
const GOLD_PROBE: u64 = 14573267147107860869;
