//! 第三包（高光恢复保底机制）判据测试：场景按实证报告 VERIFY-Q2Q3Q4 的
//! W2 定义在已白平衡相机 RGB 域重建，ΔE00 在测试内按 CIEDE2000 实现
//! （f64 + `libm::` 自由函数；libm 纪律对测试代码同样生效）。
//!
//! 测量工具先自校验：ΔE00 对 Sharma 标准测试对前 6 组的误差 < 0.001、
//! 色彩链对 W1 品红陷阱原型输出 (2.72, 0.56, 1.85) 的复现误差 < 0.15，
//! 两把尺子先有牙，下面的 ΔE 断言才作数（仓库纪律：断言必须证明能红）。

use yanshi_raw::develop::{composite_color_matrix, highlight_recover, highlight_softness};
use yanshi_raw::frame::{CalibrationMatrix, RecordContext, WbMode};
use yanshi_raw::{develop, CfaGrid, CfaPattern, Demosaic, DevelopOptions, RawFrame};

/// W2 的日光 WB（balanced 域观测模型 obs = min(true, w_c) 的 w）。
const WB_DAY: [f32; 3] = [2.0, 1.0, 1.6];
/// 对应中性坐标 n = 1/w（复合色彩链的输入）。
const N_DAY: [f32; 3] = [0.5, 1.0, 0.625];

/// Sony A7R III 双标定（rawler 数据库数值，与 Python 原型同源）。
fn sony_calibrations() -> Vec<CalibrationMatrix> {
    vec![
        CalibrationMatrix {
            illuminant_kelvin: Some(2856.0),
            xyz_to_cam: [
                [0.7683, -0.3276, 0.0299],
                [-0.3630, 1.0892, 0.3161],
                [-0.0162, 0.0671, 0.7133],
            ],
        },
        CalibrationMatrix {
            illuminant_kelvin: Some(6504.0),
            xyz_to_cam: [
                [0.6640, -0.1847, -0.0503],
                [-0.5238, 1.3010, 0.2474],
                [-0.0993, 0.1673, 0.6527],
            ],
        },
    ]
}

/// 原型 M_day：复合色彩链在日光中性坐标下的总矩阵（升 f64 供 ΔE 渲染）。
fn m_day() -> [[f64; 3]; 3] {
    let mut notes = Vec::new();
    let composite = composite_color_matrix(&sony_calibrations(), N_DAY, &mut notes).unwrap();
    let mut m = [[0.0f64; 3]; 3];
    for (r, row) in m.iter_mut().enumerate() {
        for (c, cell) in row.iter_mut().enumerate() {
            *cell = composite.matrix[r][c] as f64;
        }
    }
    m
}

/// 线性 sRGB → XYZ（发布值矩阵，与 develop 的 XYZ_D65_TO_SRGB 互逆）。
const SRGB_TO_XYZ: [[f64; 3]; 3] = [
    [0.41239080, 0.35758434, 0.18048079],
    [0.21263901, 0.71516868, 0.07219232],
    [0.01933082, 0.11919478, 0.95053215],
];

fn mat3_mul(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// balanced 相机 RGB → CIE Lab（经 M_day 与 sRGB→XYZ，白点 D65）。
fn cam_to_lab(m: &[[f64; 3]; 3], cam: [f32; 3]) -> [f64; 3] {
    let srgb = mat3_mul(m, [cam[0] as f64, cam[1] as f64, cam[2] as f64]);
    let xyz = mat3_mul(&SRGB_TO_XYZ, srgb);
    xyz_to_lab(xyz)
}

fn xyz_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    const WHITE: [f64; 3] = [0.95047, 1.0, 1.08883];
    const DELTA: f64 = 6.0 / 29.0;
    let f = |t: f64| -> f64 {
        if t > DELTA * DELTA * DELTA {
            libm::cbrt(t)
        } else {
            t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
        }
    };
    let fx = f(xyz[0] / WHITE[0]);
    let fy = f(xyz[1] / WHITE[1]);
    let fz = f(xyz[2] / WHITE[2]);
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000（Sharma 公式，角度量以弧度经 libm 计算）。
fn delta_e00(lab1: [f64; 3], lab2: [f64; 3]) -> f64 {
    let (l1, a1, b1) = (lab1[0], lab1[1], lab1[2]);
    let (l2, a2, b2) = (lab2[0], lab2[1], lab2[2]);
    let c1 = libm::sqrt(a1 * a1 + b1 * b1);
    let c2 = libm::sqrt(a2 * a2 + b2 * b2);
    let c_bar = (c1 + c2) / 2.0;
    let c_bar7 = libm::pow(c_bar, 7.0);
    let g = 0.5 * (1.0 - libm::sqrt(c_bar7 / (c_bar7 + libm::pow(25.0, 7.0))));
    let a1p = (1.0 + g) * a1;
    let a2p = (1.0 + g) * a2;
    let c1p = libm::sqrt(a1p * a1p + b1 * b1);
    let c2p = libm::sqrt(a2p * a2p + b2 * b2);
    let hue = |ap: f64, b: f64| -> f64 {
        if ap == 0.0 && b == 0.0 {
            0.0
        } else {
            let h = libm::atan2(b, ap);
            if h < 0.0 {
                h + 2.0 * core::f64::consts::PI
            } else {
                h
            }
        }
    };
    let h1p = hue(a1p, b1);
    let h2p = hue(a2p, b2);
    let dlp = l2 - l1;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let mut d = h2p - h1p;
        if d > core::f64::consts::PI {
            d -= 2.0 * core::f64::consts::PI;
        } else if d < -core::f64::consts::PI {
            d += 2.0 * core::f64::consts::PI;
        }
        d
    };
    let dhp_big = 2.0 * libm::sqrt(c1p * c2p) * libm::sin(dhp / 2.0);
    let l_bar_p = (l1 + l2) / 2.0;
    let c_bar_p = (c1p + c2p) / 2.0;
    let h_bar_p = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= core::f64::consts::PI {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 2.0 * core::f64::consts::PI {
        (h1p + h2p + 2.0 * core::f64::consts::PI) / 2.0
    } else {
        (h1p + h2p - 2.0 * core::f64::consts::PI) / 2.0
    };
    let t = 1.0 - 0.17 * libm::cos(h_bar_p - core::f64::consts::PI / 6.0)
        + 0.24 * libm::cos(2.0 * h_bar_p)
        + 0.32 * libm::cos(3.0 * h_bar_p + core::f64::consts::PI / 30.0)
        - 0.20 * libm::cos(4.0 * h_bar_p - 7.0 * core::f64::consts::PI / 20.0);
    let d_theta = core::f64::consts::PI / 6.0
        * libm::exp(-libm::pow(
            (h_bar_p * 180.0 / core::f64::consts::PI - 275.0) / 25.0,
            2.0,
        ));
    let c_bar_p7 = libm::pow(c_bar_p, 7.0);
    let rc = 2.0 * libm::sqrt(c_bar_p7 / (c_bar_p7 + libm::pow(25.0, 7.0)));
    let sl = 1.0
        + 0.015 * libm::pow(l_bar_p - 50.0, 2.0)
            / libm::sqrt(20.0 + libm::pow(l_bar_p - 50.0, 2.0));
    let sc = 1.0 + 0.045 * c_bar_p;
    let sh = 1.0 + 0.015 * c_bar_p * t;
    let rt = -libm::sin(2.0 * d_theta) * rc;
    libm::sqrt(
        libm::pow(dlp / sl, 2.0)
            + libm::pow(dcp / sc, 2.0)
            + libm::pow(dhp_big / sh, 2.0)
            + rt * (dcp / sc) * (dhp_big / sh),
    )
}

/// 场景某像素的 ΔE（恢复/硬钳输出 对 真值，各自经完整色彩链渲染）。
fn pixel_delta_e(m: &[[f64; 3]; 3], out: [f32; 3], truth: [f32; 3]) -> f64 {
    delta_e00(cam_to_lab(m, out), cam_to_lab(m, truth))
}

fn px(buf: &[f32], width: usize, x: usize, y: usize) -> [f32; 3] {
    let o = (y * width + x) * 3;
    [buf[o], buf[o + 1], buf[o + 2]]
}

/// 尺子自校验：ΔE00 对 Sharma 标准对前 6 组的最大误差 < 0.001。
#[test]
fn delta_e00_matches_sharma_reference_pairs() {
    let pairs: [([f64; 3], [f64; 3], f64); 6] = [
        ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
        ([50.0, 3.1571, -77.2803], [50.0, 0.0, -82.7485], 2.8615),
        ([50.0, 2.8361, -74.0200], [50.0, 0.0, -82.7485], 3.4412),
        ([50.0, -1.3802, -84.2814], [50.0, 0.0, -82.7485], 1.0000),
        ([50.0, -1.1848, -84.8006], [50.0, 0.0, -82.7485], 1.0000),
        ([50.0, -0.9009, -85.5211], [50.0, 0.0, -82.7485], 1.0000),
    ];
    for (lab1, lab2, expected) in pairs {
        let got = delta_e00(lab1, lab2);
        assert!(
            (got - expected).abs() < 0.001,
            "ΔE00 尺子不准：{lab1:?} vs {lab2:?} 得 {got}，期望 {expected}"
        );
    }
}

/// 尺子自校验二：色彩链复现 W1 品红陷阱的原型输出 (2.72, 0.56, 1.85)。
#[test]
fn color_chain_reproduces_prototype_magenta_trap() {
    let m = m_day();
    let rendered = mat3_mul(&m, [WB_DAY[0] as f64, WB_DAY[1] as f64, WB_DAY[2] as f64]);
    let expected = [2.72, 0.56, 1.85];
    for c in 0..3 {
        assert!(
            (rendered[c] - expected[c]).abs() < 0.15,
            "色彩链与原型对不上：通道 {c} 得 {}，原型 {}",
            rendered[c],
            expected[c]
        );
    }
}

// ---------------------------------------------------------------------------
// 判据 ①：全饱和窗口 ⇒ 严格中性、不放大（防 (5,5,5) 静默错误与品红陷阱
// 复现）；同场景无恢复对照必须输出品红（判据有牙）。
// ---------------------------------------------------------------------------

#[test]
fn criterion1_fully_saturated_block_recovers_to_neutral() {
    let (w, h) = (48usize, 48usize);
    let mut img = vec![0.45f32; w * h * 3];
    for y in 12..36 {
        for x in 12..36 {
            let o = (y * w + x) * 3;
            img[o] = WB_DAY[0];
            img[o + 1] = WB_DAY[1];
            img[o + 2] = WB_DAY[2];
        }
    }
    let mut rec = img.clone();
    highlight_recover(&mut rec, w, h, WB_DAY, None).unwrap();
    let m = m_day();
    // 窗口全饱和的内域（距块边 ≥ 2 px）：严格中性、不放大、收敛到 2.0。
    for y in 14..34 {
        for x in 14..34 {
            let p = px(&rec, w, x, y);
            assert!(
                (p[0] - p[1]).abs() < 1e-4 && (p[1] - p[2]).abs() < 1e-4,
                "全饱和内域 ({x},{y}) 不中性：{p:?}"
            );
            assert!(
                (p[0] - 2.0).abs() < 1e-3,
                "全饱和内域 ({x},{y}) 未收敛到饱和亮度 2.0：{p:?}"
            );
            let rendered = mat3_mul(&m, [p[0] as f64, p[1] as f64, p[2] as f64]);
            assert!(
                (rendered[0] - rendered[1]).abs() < 1e-3
                    && (rendered[1] - rendered[2]).abs() < 1e-3,
                "全饱和内域 ({x},{y}) 经色彩链不中性：{rendered:?}"
            );
        }
    }
    // 全图不放大 + 不被往低拉（单调性下界的全图版）。
    for i in 0..w * h {
        for c in 0..3 {
            assert!(rec[i * 3 + c] >= img[i * 3 + c], "像素被往低拉");
            assert!(rec[i * 3 + c] <= 2.0 * (1.0 + 1e-3), "输出被放大");
        }
    }
    // 判据有牙：无恢复时块中心渲染为品红（G 通道塌陷）。
    let hard = mat3_mul(&m, [2.0, 1.0, 1.6]);
    assert!(
        hard[0] - hard[1] > 1.5 && hard[2] - hard[1] > 1.0,
        "对照未复现品红，判据没牙：{hard:?}"
    );
}

#[test]
fn criterion1_large_saturated_field_neutral_and_not_amplified() {
    let (w, h) = (64usize, 64usize);
    let mut img = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        img[i * 3] = WB_DAY[0];
        img[i * 3 + 1] = WB_DAY[1];
        img[i * 3 + 2] = WB_DAY[2];
    }
    highlight_recover(&mut img, w, h, WB_DAY, None).unwrap();
    for i in 0..w * h {
        let p = [img[i * 3], img[i * 3 + 1], img[i * 3 + 2]];
        assert!(
            (p[0] - p[1]).abs() < 1e-4 && (p[1] - p[2]).abs() < 1e-4,
            "大面积全饱和场不中性：{p:?}"
        );
        assert!(
            (p[0] - 2.0).abs() < 1e-3 && p[0] <= 2.0 * (1.0 + 1e-3),
            "大面积全饱和场被放大或未收敛（防 (5,5,5) 回归）：{p:?}"
        );
    }
}

/// 判据 ① 的链上版：整帧饱和的马赛克经完整 develop 后输出严格中性；
/// 同帧走第二包链路（无 ④′）手动组装时必须非中性（对照有牙）。
#[test]
fn criterion1_full_develop_chain_neutralizes_saturated_frame() {
    let (w, h) = (32usize, 32usize);
    let frame = RawFrame {
        width: w,
        height: h,
        mosaic: vec![16383.0; w * h],
        cfa: CfaGrid::from_bayer(CfaPattern::Rggb),
        black_levels: vec![512.0; 4],
        white_levels: vec![16383.0; 4],
        wb_multipliers: Some([1.72, 1.0, 1.39]),
        as_shot_neutral: None,
        color_matrices: sony_calibrations(),
        baseline_exposure_ev: None,
        orientation: 1,
    };
    let options = DevelopOptions {
        wb: WbMode::AsShot,
        demosaic: Demosaic::MalvarHeCutler,
        pre_exposure_ev: 0.0,
        // 本判据验的是第三包保底链：显式 Floor 档（第五包档位字段
        // 加入前的原口径），不让新档位改变本测试的语义。
        highlight: yanshi_raw::HighlightMode::Floor,
        // Y-03 档位字段加入后补齐穷举字面量：本判据走零调整基线。
        tone: yanshi_raw::ToneMode::Off,
        tone_strength: 1.0,
        // C1-b 档位字段加入后补齐穷举字面量：本判据走全档。
        max_output_edge: None,
    };
    let out = develop(&frame, &options, &RecordContext::default()).unwrap();
    for i in 0..w * h {
        let p = [
            out.rgb_linear[i * 3],
            out.rgb_linear[i * 3 + 1],
            out.rgb_linear[i * 3 + 2],
        ];
        assert!(
            (p[0] - p[1]).abs() < 1e-3 && (p[1] - p[2]).abs() < 1e-3,
            "完整 develop 后饱和帧不中性：{p:?}"
        );
        assert!(
            (p[0] - 1.72).abs() < 0.02,
            "完整 develop 后饱和帧亮度异常（应收敛到最亮通道 1.72）：{p:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 判据 ②：落日场景（R 饱和的强着色光源）ΔE 不劣于硬钳。
// 判据 ②′：两通道饱和的强色度场 —— 门控必须关、下界必须保逐位不变。
// ---------------------------------------------------------------------------

#[test]
fn criterion2_sunset_not_worse_than_hard_clamp() {
    let (w, h) = (32usize, 32usize);
    let truth = [2.4f32, 0.9, 0.6];
    let obs = [
        truth[0].min(WB_DAY[0]),
        truth[1].min(WB_DAY[1]),
        truth[2].min(WB_DAY[2]),
    ];
    let mut img = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        img[i * 3..i * 3 + 3].copy_from_slice(&obs);
    }
    let mut rec = img.clone();
    highlight_recover(&mut rec, w, h, WB_DAY, None).unwrap();
    // 只有 R 一个通道饱和 ⇒ Σs = 1 ⇒ desat 不触发：逐位等于硬钳。
    assert_eq!(rec, img, "落日场景被改动（desat 误触发）");
    let m = m_day();
    let de_rec = pixel_delta_e(&m, px(&rec, w, 16, 16), truth);
    let de_hard = pixel_delta_e(&m, obs, truth);
    assert!(
        de_rec <= de_hard + 1e-9,
        "落日 ΔE 劣化：恢复 {de_rec} vs 硬钳 {de_hard}"
    );
    assert!(
        de_hard < 6.5,
        "落日夹具 ΔE 与原型 5.532 量级不符：{de_hard}"
    );
}

#[test]
fn criterion2_strong_chroma_two_channel_saturation_untouched() {
    let (w, h) = (16usize, 16usize);
    let truth = [3.0f32, 1.5, 0.5];
    let obs = [
        truth[0].min(WB_DAY[0]),
        truth[1].min(WB_DAY[1]),
        truth[2].min(WB_DAY[2]),
    ];
    let mut img = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        img[i * 3..i * 3 + 3].copy_from_slice(&obs);
    }
    let mut rec = img.clone();
    highlight_recover(&mut rec, w, h, WB_DAY, None).unwrap();
    // 强着色光源（R、G 双饱和）：存活的 B 通道明确断言强色度
    // （κ ≈ 0.687 > 门上界）⇒ 门控关；且任何通道不许被往低拉。
    assert_eq!(rec, img, "强色度双饱和场被 desat 误伤");
}

// ---------------------------------------------------------------------------
// 判据 ③：深蓝渐变双通道触顶（W2(a2)）——色度走向可由邻域恢复，
// 恢复 ΔE 不得劣化（门控的回归锁；无门控 desat 在此 ΔE 33.2）。
// ---------------------------------------------------------------------------

fn gradient_scene(
    dir: [f32; 3],
    t0: f32,
    t1: f32,
    rows: usize,
    width: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let mut obs = vec![0.0f32; rows * width * 3];
    let mut truth = vec![0.0f32; rows * width * 3];
    let mut ts = Vec::with_capacity(rows);
    for r in 0..rows {
        let t = t0 + (t1 - t0) * (r as f32) / ((rows - 1) as f32);
        ts.push(t);
        for x in 0..width {
            let o = (r * width + x) * 3;
            for c in 0..3 {
                let v = dir[c] * t;
                truth[o + c] = v;
                obs[o + c] = v.min(WB_DAY[c]);
            }
        }
    }
    (obs, truth, ts)
}

fn nearest_row(ts: &[f32], target: f32) -> usize {
    let mut best = 0usize;
    let mut best_d = f32::MAX;
    for (i, &t) in ts.iter().enumerate() {
        if (t - target).abs() < best_d {
            best_d = (t - target).abs();
            best = i;
        }
    }
    best
}

#[test]
fn criterion3_deep_blue_gradient_not_desaturated() {
    let (w, rows) = (8usize, 240usize);
    let (obs, truth, ts) = gradient_scene([0.22, 0.50, 1.00], 0.8, 3.2, rows, w);
    let mut rec = obs.clone();
    highlight_recover(&mut rec, w, rows, WB_DAY, None).unwrap();
    // 门控全程关：恢复输出与硬钳逐位一致（最强形式的「不劣化」）。
    assert_eq!(rec, obs, "深蓝渐变被 desat 改动（门控失效）");
    let m = m_day();
    let row = nearest_row(&ts, 2.6);
    let de_hard = pixel_delta_e(&m, px(&obs, w, 3, row), px(&truth, w, 3, row));
    let de_rec = pixel_delta_e(&m, px(&rec, w, 3, row), px(&truth, w, 3, row));
    assert!(
        (de_hard - 6.2).abs() < 1.5,
        "深蓝夹具硬钳 ΔE 与原型 6.2 量级不符：{de_hard}"
    );
    assert!(
        de_rec <= de_hard + 0.5,
        "深蓝渐变 ΔE 劣化：恢复 {de_rec} vs 硬钳 {de_hard}"
    );
    // 全裁剪区逐行不劣化。
    for (r, &t) in ts.iter().enumerate() {
        if t < 2.0 {
            continue;
        }
        let dh = pixel_delta_e(&m, px(&obs, w, 3, r), px(&truth, w, 3, r));
        let dr = pixel_delta_e(&m, px(&rec, w, 3, r), px(&truth, w, 3, r));
        assert!(dr <= dh + 0.5, "深蓝渐变 t={t} 行 ΔE 劣化：{dr} vs {dh}");
    }
}

// ---------------------------------------------------------------------------
// 判据 ④：浅色天空双通道场景（W2(a2′)）——色度断言接近中性，desat
// 必须生效，恢复 ΔE 显著低于硬钳（原型 33.0/41.6 → 19.2/19.9）。
// ---------------------------------------------------------------------------

#[test]
fn criterion4_pale_sky_desaturation_works() {
    let (w, rows) = (8usize, 221usize);
    let (obs, truth, ts) = gradient_scene([0.62, 0.78, 1.00], 0.8, 3.0, rows, w);
    let mut rec = obs.clone();
    highlight_recover(&mut rec, w, rows, WB_DAY, None).unwrap();
    let m = m_day();
    // 尺子有牙：硬钳 ΔE 复现原型量级。
    let row22 = nearest_row(&ts, 2.2);
    let row26 = nearest_row(&ts, 2.6);
    let row18 = nearest_row(&ts, 1.8);
    let hard22 = pixel_delta_e(&m, px(&obs, w, 3, row22), px(&truth, w, 3, row22));
    let hard26 = pixel_delta_e(&m, px(&obs, w, 3, row26), px(&truth, w, 3, row26));
    assert!(
        (hard22 - 33.0).abs() < 3.0,
        "天空硬钳 ΔE(2.2) 与原型 33.0 不符：{hard22}"
    );
    assert!(
        (hard26 - 41.6).abs() < 4.0,
        "天空硬钳 ΔE(2.6) 与原型 41.6 不符：{hard26}"
    );
    // desat 生效：显著低于硬钳、达原型恢复量级（+f32 余量）。
    let rec22 = pixel_delta_e(&m, px(&rec, w, 3, row22), px(&truth, w, 3, row22));
    let rec26 = pixel_delta_e(&m, px(&rec, w, 3, row26), px(&truth, w, 3, row26));
    let rec18 = pixel_delta_e(&m, px(&rec, w, 3, row18), px(&truth, w, 3, row18));
    let hard18 = pixel_delta_e(&m, px(&obs, w, 3, row18), px(&truth, w, 3, row18));
    assert!(
        rec22 <= hard22 - 8.0 && rec22 <= 24.0,
        "天空 t=2.2 desat 未达量级：恢复 {rec22} vs 硬钳 {hard22}（原型 19.2）"
    );
    assert!(
        rec26 <= hard26 - 15.0 && rec26 <= 25.0,
        "天空 t=2.6 desat 未达量级：恢复 {rec26} vs 硬钳 {hard26}（原型 19.9）"
    );
    assert!(
        rec18 <= hard18,
        "天空 t=1.8 方向性改善缺失：恢复 {rec18} vs 硬钳 {hard18}"
    );
    // 中性化方向：t=2.6 行恢复像素三通道应已收敛（彼此差 < 0.05）。
    let p = px(&rec, w, 3, row26);
    assert!(
        (p[0] - p[1]).abs() < 0.05 && (p[1] - p[2]).abs() < 0.05,
        "天空 t=2.6 未收敛到中性：{p:?}"
    );
}

// ---------------------------------------------------------------------------
// 判据 ⑤：无高光场景逐位不变（函数级恒等；跨版本整链逐位一致由构建
// 日志中的转储 sha256 对账承载，见 README）。
// ---------------------------------------------------------------------------

#[test]
fn criterion5_no_highlight_scenes_bitwise_unchanged() {
    let check = |img: &[f32], w: usize, h: usize, label: &str| {
        let mut out = img.to_vec();
        highlight_recover(&mut out, w, h, WB_DAY, None).unwrap();
        assert_eq!(out, img, "{label} 被改动");
    };
    // 常量场（中性）。
    check(&vec![0.30f32; 16 * 16 * 3], 16, 16, "常量场");
    // 常量场（带色，但全在软区下）。
    let mut flat = vec![0.0f32; 16 * 16 * 3];
    for i in 0..16 * 16 {
        flat[i * 3] = 0.45;
        flat[i * 3 + 1] = 0.40;
        flat[i * 3 + 2] = 0.35;
    }
    check(&flat, 16, 16, "带色常量场");
    // 渐变场（最大 0.85 < τ_0·T_G = 0.9016）。
    let (w, h) = (64usize, 48usize);
    let mut grad = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / (w - 1) as f32;
            let v = y as f32 / (h - 1) as f32;
            let o = (y * w + x) * 3;
            grad[o] = (0.10 + 0.30 * u + 0.15 * v) * 1.05;
            grad[o + 1] = 0.10 + 0.30 * u + 0.15 * v;
            grad[o + 2] = (0.10 + 0.30 * u + 0.15 * v) * 0.95;
        }
    }
    check(&grad, w, h, "渐变场");
    // 棋盘（0.18/0.36 两档）。
    let (w, h) = (32usize, 32usize);
    let mut board = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let v = if (x / 4 + y / 4) % 2 == 0 { 0.18 } else { 0.36 };
            let o = (y * w + x) * 3;
            board[o] = v * 1.05;
            board[o + 1] = v;
            board[o + 2] = v * 0.95;
        }
    }
    check(&board, w, h, "棋盘场");
    // 软区内的单通道（G = 0.95 迈进软带但 Σs < 1）：仍逐位不变。
    let mut soft = vec![0.0f32; 8 * 8 * 3];
    for i in 0..8 * 8 {
        soft[i * 3] = 0.5;
        soft[i * 3 + 1] = 0.95;
        soft[i * 3 + 2] = 0.5;
    }
    check(&soft, 8, 8, "软区单通道场");
}

// ---------------------------------------------------------------------------
// 判据 ⑥：单通道饱和（W2(a1) 纹理场）——输出与硬钳逐位一致（不劣化、
// 不假恢复）。噪声场为确定性构造（xorshift64* + Irwin–Hall 近似正态 +
// 三次盒平滑），与原型 scipy 高斯场形状不同，但判据断的是恒等关系。
// ---------------------------------------------------------------------------

fn xorshift64star(state: &mut u64) -> f64 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    let v = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
    ((v >> 11) as f64) / ((1u64 << 53) as f64)
}

fn box_blur(field: &mut [f64], w: usize, h: usize, radius: isize) {
    let src = field.to_vec();
    let reflect = |i: isize, n: usize| -> usize {
        let n = n as isize;
        let mut m = i % (2 * n);
        if m < 0 {
            m += 2 * n;
        }
        if m >= n {
            m = 2 * n - 1 - m;
        }
        m as usize
    };
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            let mut cnt = 0.0;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let sx = reflect(x as isize + dx, w);
                    let sy = reflect(y as isize + dy, h);
                    sum += src[sy * w + sx];
                    cnt += 1.0;
                }
            }
            field[y * w + x] = sum / cnt;
        }
    }
}

#[test]
fn criterion6_single_channel_saturation_matches_hard_clamp() {
    let (w, h) = (96usize, 96usize);
    let mut state = 0x2026_1021_5EED_u64;
    let mut noise = vec![0.0f64; w * h];
    for cell in noise.iter_mut() {
        let mut acc = 0.0;
        for _ in 0..12 {
            acc += xorshift64star(&mut state);
        }
        *cell = acc - 6.0;
    }
    for _ in 0..3 {
        box_blur(&mut noise, w, h, 2);
    }
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for &v in &noise {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    let dir = [0.30f32, 0.78, 0.72];
    let mut truth = vec![0.0f32; w * h * 3];
    let mut obs = vec![0.0f32; w * h * 3];
    let mut clipped = 0usize;
    for i in 0..w * h {
        let n01 = (noise[i] - lo) / (hi - lo);
        let t = (0.75 + 1.15 * n01) as f32;
        for c in 0..3 {
            let v = dir[c] * t;
            truth[i * 3 + c] = v;
            obs[i * 3 + c] = v.min(WB_DAY[c]);
        }
        let x = i % w;
        let y = i / w;
        if x >= 3 && x + 3 < w && y >= 3 && y + 3 < h && truth[i * 3 + 1] > WB_DAY[1] {
            clipped += 1;
            // G 真值被裁：观测该像素必须与硬钳逐位一致（单通道饱和
            // ⇒ Σs ≤ 1 ⇒ 本包按定版不做任何「恢复」）。
            assert_eq!(obs[i * 3 + 1], WB_DAY[1]);
        }
    }
    assert!(clipped > 500, "纹理场夹具无牙：被裁像素只有 {clipped} 个");
    let mut rec = obs.clone();
    highlight_recover(&mut rec, w, h, WB_DAY, None).unwrap();
    let mut checked = 0usize;
    for i in 0..w * h {
        let x = i % w;
        let y = i / w;
        if x >= 3 && x + 3 < w && y >= 3 && y + 3 < h && truth[i * 3 + 1] > WB_DAY[1] {
            checked += 1;
            for c in 0..3 {
                assert_eq!(
                    rec[i * 3 + c],
                    obs[i * 3 + c],
                    "单通道饱和像素 ({x},{y}) 通道 {c} 与硬钳不一致"
                );
            }
        }
    }
    assert_eq!(checked, clipped);
}

// ---------------------------------------------------------------------------
// 判据 ⑦：阈值公式单测 + g_vig 端到端接入。
// ---------------------------------------------------------------------------

#[test]
fn criterion7_softness_formula_matches_hand_computation() {
    // T_B = 0.98 · 1.0 · 1.6 = 1.568（日光 B 通道、g_vig = 1）。
    let t = 0.98f32 * 1.6;
    assert_eq!(highlight_softness(0.80 * t, t), 0.0, "τ_0·T 以下应为 0");
    assert_eq!(highlight_softness(0.92 * t, t), 0.0, "τ_0·T 处应为 0");
    let at_top = highlight_softness(t, t);
    assert!((at_top - 1.0).abs() < 1e-6, "T 处应为 1，得 {at_top}");
    let mid = highlight_softness(0.96 * t, t);
    assert!((mid - 0.5).abs() < 1e-6, "软带中点应为 0.5，得 {mid}");
    // 单调性：21 点采样不递减。
    let mut prev = -1.0f32;
    for i in 0..=20 {
        let v = (0.85 + 0.20 * (i as f32) / 20.0) * t;
        let s = highlight_softness(v, t);
        assert!(s >= prev, "s 不单调：{s} < {prev}");
        prev = s;
    }
}

#[test]
fn criterion7_vig_scales_thresholds_end_to_end() {
    // 场值 ∝ WB（I = 0.75 · w）：g_vig = 1 时全通道在软区下 ⇒ 不变；
    // g_vig = 0.7 时天花板同步缩小、三通道齐过阈值 ⇒ desat 到最亮通道。
    let (w, h) = (8usize, 8usize);
    let base = [0.75f32 * WB_DAY[0], 0.75 * WB_DAY[1], 0.75 * WB_DAY[2]];
    let mut img = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        img[i * 3..i * 3 + 3].copy_from_slice(&base);
    }
    let mut no_vig = img.clone();
    highlight_recover(&mut no_vig, w, h, WB_DAY, None).unwrap();
    assert_eq!(no_vig, img, "g_vig = 1 时不应触发");
    let gain = vec![0.7f32; w * h];
    let mut with_vig = img.clone();
    highlight_recover(&mut with_vig, w, h, WB_DAY, Some(&gain)).unwrap();
    for i in 0..w * h {
        let p = [with_vig[i * 3], with_vig[i * 3 + 1], with_vig[i * 3 + 2]];
        for c in 0..3 {
            assert!(
                (p[c] - base[0]).abs() < 1e-4,
                "g_vig 缩小天花板后应收敛到最亮通道 {}，得 {p:?}",
                base[0]
            );
        }
    }
}

#[test]
fn criterion7_invalid_inputs_are_loud_errors() {
    let mut img = vec![0.0f32; 4 * 4 * 3];
    assert!(highlight_recover(&mut img, 4, 4, [0.0, 1.0, 1.6], None).is_err());
    assert!(highlight_recover(&mut img, 4, 4, [f32::NAN, 1.0, 1.6], None).is_err());
    let bad_gain = vec![0.0f32; 4 * 4];
    assert!(highlight_recover(&mut img, 4, 4, WB_DAY, Some(&bad_gain)).is_err());
    let short_gain = vec![1.0f32; 3];
    assert!(highlight_recover(&mut img, 4, 4, WB_DAY, Some(&short_gain)).is_err());
    assert!(highlight_recover(&mut img, 5, 4, WB_DAY, None).is_err());
}

/// 退化尺寸（1×1 全饱和）不许 panic 且收敛中性（reflect 窗口 = 自身）。
#[test]
fn degenerate_single_pixel_saturated() {
    let mut img = vec![WB_DAY[0], WB_DAY[1], WB_DAY[2]];
    highlight_recover(&mut img, 1, 1, WB_DAY, None).unwrap();
    assert!(
        (img[0] - 2.0).abs() < 1e-4 && (img[1] - 2.0).abs() < 1e-4 && (img[2] - 2.0).abs() < 1e-4,
        "1×1 全饱和未收敛中性：{img:?}"
    );
}
