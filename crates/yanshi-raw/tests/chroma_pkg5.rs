//! 第五包判据（真色度恢复）：单通道饱和残粉的色度修复、高光层次
//! （对比第三包保底的平坦化）、无锚点/无饱和时的逐位不变。
//! 形状按仓库纪律：全部用**合成 GT**——真值先定、观测按传感器口径
//! 硬钳生成，恢复误差直接对真值量，不拿实现比实现。

use yanshi_raw::develop::{highlight_chroma_recover, highlight_recover};
use yanshi_raw::frame::CalibrationMatrix;
use yanshi_raw::{
    develop, CfaGrid, CfaPattern, DevelopOptions, HighlightMode, RawFrame, RecordContext,
};

/// 与第三包判据同一套日光 WB（重归一化口径）：天花板 T = 0.98·w。
const WB_DAY: [f32; 3] = [2.0, 1.0, 1.6];

fn realistic_calibrations() -> Vec<CalibrationMatrix> {
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

/// 传感器口径的观测：逐通道硬钳到 WB 天花板（与第三包判据同口径）。
fn observe(truth: [f32; 3]) -> [f32; 3] {
    [
        truth[0].min(WB_DAY[0]),
        truth[1].min(WB_DAY[1]),
        truth[2].min(WB_DAY[2]),
    ]
}

// ---------------------------------------------------------------------------
// 判据 A：单通道饱和残粉——R 独饱和时，保底机制逐位不动（第三包
// 设计如此），真色度恢复应把 R 按邻域色度比估回真值附近。
// ---------------------------------------------------------------------------

#[test]
fn single_channel_residual_pink_is_repaired_by_local_chroma() {
    let (w, h) = (48usize, 48usize);
    // 全场同一色度比（云带的中性偏暖白）：q = [1, 0.419, 0.305]。
    // 左半亮度 0.8（不饱和锚点），右半亮度 2.03（只有 R 触顶）。
    let ratio = [1.0f32, 0.419, 0.305];
    let mut img = vec![0.0f32; w * h * 3];
    let mut truth_r = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let l = if x < w / 2 { 0.8 } else { 2.03 };
            let truth = [l * ratio[0], l * ratio[1], l * ratio[2]];
            truth_r[y * w + x] = truth[0];
            let obs = observe(truth);
            img[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&obs);
        }
    }
    // 保底：右半 R 停在硬钳 2.0（真值 2.03），误差 0.03 且色度比失真。
    let mut floor = img.clone();
    highlight_recover(&mut floor, w, h, WB_DAY, None).unwrap();
    let mut chroma = floor.clone();
    highlight_chroma_recover(&mut chroma, w, h, WB_DAY, None).unwrap();
    let mut err_floor = 0.0f32;
    let mut err_chroma = 0.0f32;
    let mut n = 0f32;
    for y in 4..h - 4 {
        for x in w / 2 + 6..w - 4 {
            let i = y * w + x;
            err_floor += (floor[i * 3] - truth_r[i]).abs();
            err_chroma += (chroma[i * 3] - truth_r[i]).abs();
            n += 1.0;
        }
    }
    let (ef, ec) = (err_floor / n, err_chroma / n);
    assert!(
        ec < ef * 0.4,
        "真色度恢复的 R 误差应显著小于保底：chroma {ec} vs floor {ef}"
    );
    assert!(ec < 0.03, "恢复值应贴近真值 2.03，平均误差 {ec}");
    // 左半锚点区逐位不动（s=0 通道不许被重建碰到）。
    for y in 0..h {
        for x in 0..w / 2 - 2 {
            let i = (y * w + x) * 3;
            assert_eq!(chroma[i..i + 3], img[i..i + 3], "锚点区被改动");
        }
    }
}

// ---------------------------------------------------------------------------
// 判据 B：高光层次——亮度渐变的高光区，保底输出平坦（G 被钳在
// 天花板纹丝不动），真色度恢复应跟随渐变（用未饱和的 R/B 反推）。
// ---------------------------------------------------------------------------

#[test]
fn highlight_gradation_is_restored_not_flat() {
    let (w, h) = (64usize, 32usize);
    let ratio = [1.02f32, 1.0, 0.96];
    let mut img = vec![0.0f32; w * h * 3];
    let mut truth_g = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let l = 0.5 + (x as f32 / (w - 1) as f32) * 1.5; // 0.5 → 2.0
            let truth = [l * ratio[0], l * ratio[1], l * ratio[2]];
            truth_g[y * w + x] = truth[1];
            let obs = observe(truth);
            img[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&obs);
        }
    }
    let mut floor = img.clone();
    highlight_recover(&mut floor, w, h, WB_DAY, None).unwrap();
    let mut chroma = floor.clone();
    highlight_chroma_recover(&mut chroma, w, h, WB_DAY, None).unwrap();
    // 观察带：x∈[16,30]（G 已触顶、R/B 仍可信的渐变区中段）。
    let mut sum_floor = 0.0f32;
    let mut sum_chroma = 0.0f32;
    let mut sum_truth = 0.0f32;
    let mut err_floor = 0.0f32;
    let mut err_chroma = 0.0f32;
    let mut n = 0f32;
    for x in 16..=30usize {
        let i = (16 * w + x) * 3 + 1;
        sum_floor += floor[i];
        sum_chroma += chroma[i];
        sum_truth += truth_g[16 * w + x];
        err_floor += (floor[i] - truth_g[16 * w + x]).abs();
        err_chroma += (chroma[i] - truth_g[16 * w + x]).abs();
        n += 1.0;
    }
    assert!(
        sum_chroma > sum_floor + 0.5,
        "渐变区 G 总量应被抬回：chroma {sum_chroma} vs floor {sum_floor}（真值 {sum_truth}）"
    );
    assert!(
        err_chroma / n < (err_floor / n) * 0.5,
        "渐变区 G 误差应至少减半：chroma {} vs floor {}",
        err_chroma / n,
        err_floor / n
    );
    // 单调性跟随：带内 chroma 的 G 应随 x 递增（层次而非平坦）。
    let first = chroma[(16 * w + 17) * 3 + 1];
    let last = chroma[(16 * w + 29) * 3 + 1];
    assert!(last > first + 0.05, "恢复应呈渐变：{first} → {last}");
}

// ---------------------------------------------------------------------------
// 判据 C/D：无锚点（整帧饱和）与无饱和时，真色度恢复逐位不改。
// ---------------------------------------------------------------------------

#[test]
fn no_anchor_and_no_saturation_are_bitwise_unchanged() {
    let (w, h) = (16usize, 16usize);
    // 整帧全饱和：保底会 desat 到中性，真色度无锚点可用 ⇒ 与保底逐位同。
    let mut saturated = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        saturated[i * 3..i * 3 + 3].copy_from_slice(&WB_DAY);
    }
    let mut floor = saturated.clone();
    highlight_recover(&mut floor, w, h, WB_DAY, None).unwrap();
    let mut chroma = floor.clone();
    highlight_chroma_recover(&mut chroma, w, h, WB_DAY, None).unwrap();
    assert_eq!(chroma, floor, "无锚点时不许编造色度，必须保留保底结果");
    // 无饱和：早退，逐位不变。
    let mut calm = vec![0.0f32; w * h * 3];
    for i in 0..w * h {
        calm[i * 3..i * 3 + 3].copy_from_slice(&[0.4, 0.3, 0.2]);
    }
    let before = calm.clone();
    highlight_chroma_recover(&mut calm, w, h, WB_DAY, None).unwrap();
    assert_eq!(calm, before);
}

// ---------------------------------------------------------------------------
// 判据 E：端到端——develop 的 Chroma 档只在饱和区与 Floor 不同，
// 暗区逐位一致；档位如实落账。
// ---------------------------------------------------------------------------

#[test]
fn develop_chroma_differs_only_in_saturated_region() {
    let (w, h) = (48usize, 32usize);
    let mut frame = RawFrame {
        width: w,
        height: h,
        mosaic: vec![0.0; w * h],
        cfa: CfaGrid::from_bayer(CfaPattern::Rggb),
        black_levels: vec![512.0; 4],
        white_levels: vec![16383.0; 4],
        wb_multipliers: Some([2.0, 1.0, 1.6]),
        as_shot_neutral: None,
        color_matrices: realistic_calibrations(),
        baseline_exposure_ev: None,
        orientation: 1,
    };
    for y in 0..h {
        for x in 0..w {
            // 全幅亮度斜坡：左侧中灰（不饱和）、向右渐入饱和——
            // 三个通道因 WB 乘子不同先后触顶，形成真实照片里高光
            // 边缘那种「部分通道先饱和」的过渡带。注：整带均匀全
            // 饱和的场景里真色度恢复按设计不动（无证据、单调性
            // 下界拦着），那种场景由判据 C 覆盖，不在此断言。
            frame.mosaic[y * w + x] = 3000.0 + (x as f32) * (16383.0 - 3000.0) / (w - 1) as f32;
        }
    }
    let floor_options = DevelopOptions {
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let chroma_options = DevelopOptions {
        highlight: HighlightMode::Chroma,
        ..DevelopOptions::default()
    };
    let floor = develop(&frame, &floor_options, &RecordContext::default()).unwrap();
    let chroma = develop(&frame, &chroma_options, &RecordContext::default()).unwrap();
    assert_eq!(floor.record.highlight_recovery, HighlightMode::Floor);
    assert_eq!(chroma.record.highlight_recovery, HighlightMode::Chroma);
    // 暗区（远离饱和带 ≥ 8px，避开解马赛克 5×5 与扩散半径的边缘效应
    // 断言范围取 x<16）逐位一致。
    for y in 0..h {
        for x in 0..16usize {
            let i = (y * w + x) * 3;
            assert_eq!(
                floor.rgb_linear[i..i + 3],
                chroma.rgb_linear[i..i + 3],
                "暗区 ({x},{y}) 不许被真色度恢复改动"
            );
        }
    }
    // 过渡带内必须有像素被恢复改变（否则等于没接线）。
    let mut changed = 0usize;
    for y in 0..h {
        for x in 28..w {
            let i = (y * w + x) * 3;
            if floor.rgb_linear[i..i + 3] != chroma.rgb_linear[i..i + 3] {
                changed += 1;
            }
        }
    }
    assert!(changed > 0, "过渡带内真色度恢复应实际改变像素");
}

// ---------------------------------------------------------------------------
// 分块等价：生产实现（行向分块 + halo）与旧全幅实现逐位一致。
// 参照是旧版 `highlight_chroma_recover` 的逐字复刻（soft/field/next
// 三个全幅数组），只做测试用；生产代码不再保留全幅路径。
// 触发条件（四层齐第 2 层）：图高 600 > CHROMA_TILE_ROWS(256) ⇒
// 3 个中心块，边界 y=256/512；右半亮度锯齿让饱和带每 100 行出现
// 一次、横跨两个边界 —— 晕边/钳制/求和顺序任一处错，边界附近
// 的像素就会逐位不同，测试恒绿不了。
// ---------------------------------------------------------------------------

/// reflect-101（与 develop.rs 的 `reflect_index` 同式：周期 2n−2，
/// 边缘不重复；测试内复刻，只为参照实现能独立跑）。
fn reflect_101(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let period = (2 * n - 2) as isize;
    let m = i.rem_euclid(period);
    if m >= n as isize {
        (period - m) as usize
    } else {
        m as usize
    }
}

/// 旧全幅实现的逐字复刻（测试专用参照）。
fn highlight_chroma_recover_full_frame_ref(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    wb: [f32; 3],
    vig: Option<&[f32]>,
) {
    use yanshi_raw::develop::{
        highlight_softness, CHROMA_DIFFUSION_STRIDES, CHROMA_MIN_ANCHOR_WEIGHT, HIGHLIGHT_TAU_CLIP,
    };
    let npix = width * height;
    let ceiling_at = |i: usize| -> [f32; 3] {
        let g = vig.map_or(1.0, |field| field[i]);
        [
            HIGHLIGHT_TAU_CLIP * g * wb[0],
            HIGHLIGHT_TAU_CLIP * g * wb[1],
            HIGHLIGHT_TAU_CLIP * g * wb[2],
        ]
    };
    let mut soft = vec![0.0f32; npix * 3];
    let mut any_clipped = false;
    for i in 0..npix {
        let t = ceiling_at(i);
        for c in 0..3 {
            let s = highlight_softness(rgb[i * 3 + c], t[c]);
            soft[i * 3 + c] = s;
            any_clipped |= s > 0.0;
        }
    }
    if !any_clipped {
        return;
    }
    let mut field = vec![0.0f32; npix * 5];
    let mut anchors = 0usize;
    for i in 0..npix {
        if soft[i * 3] == 0.0 && soft[i * 3 + 1] == 0.0 && soft[i * 3 + 2] == 0.0 {
            let (r, g, b) = (rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]);
            let l = r.max(g).max(b);
            if l > 0.0 {
                field[i * 5] = r / l;
                field[i * 5 + 1] = g / l;
                field[i * 5 + 2] = b / l;
                field[i * 5 + 3] = l;
                field[i * 5 + 4] = 1.0;
                anchors += 1;
            }
        }
    }
    if anchors == 0 {
        return;
    }
    let mut next = vec![0.0f32; npix * 5];
    for &stride in CHROMA_DIFFUSION_STRIDES.iter() {
        let s = stride as isize;
        for y in 0..height {
            let ym = reflect_101(y as isize - s, height);
            let yp = reflect_101(y as isize + s, height);
            for x in 0..width {
                let xm = reflect_101(x as isize - s, width);
                let xp = reflect_101(x as isize + s, width);
                let mut acc = [0.0f32; 5];
                for sy in [ym, y, yp] {
                    for sx in [xm, x, xp] {
                        let src = (sy * width + sx) * 5;
                        for k in 0..5 {
                            acc[k] += field[src + k];
                        }
                    }
                }
                let dst = (y * width + x) * 5;
                for k in 0..5 {
                    next[dst + k] = acc[k] / 9.0;
                }
            }
        }
        std::mem::swap(&mut field, &mut next);
    }
    for i in 0..npix {
        let s = [soft[i * 3], soft[i * 3 + 1], soft[i * 3 + 2]];
        if s[0] == 0.0 && s[1] == 0.0 && s[2] == 0.0 {
            continue;
        }
        let w = field[i * 5 + 4];
        if w <= CHROMA_MIN_ANCHOR_WEIGHT {
            continue;
        }
        let q = [
            (field[i * 5] / w).max(0.0),
            (field[i * 5 + 1] / w).max(0.0),
            (field[i * 5 + 2] / w).max(0.0),
        ];
        let value = [rgb[i * 3], rgb[i * 3 + 1], rgb[i * 3 + 2]];
        let mut l_est: Option<f32> = None;
        for c in 0..3 {
            if s[c] < 1.0 && q[c] > 1e-6 {
                let candidate = value[c] / q[c];
                l_est = Some(l_est.map_or(candidate, |l: f32| l.max(candidate)));
            }
        }
        let l = match l_est {
            Some(l) => l,
            None => field[i * 5 + 3] / w,
        };
        if !l.is_finite() || l <= 0.0 {
            continue;
        }
        for c in 0..3 {
            if s[c] <= 0.0 {
                continue;
            }
            let estimate = value[c].max(l * q[c]);
            rgb[i * 3 + c] = (1.0 - s[c]) * value[c] + s[c] * estimate;
        }
    }
}

#[test]
fn tiled_matches_full_frame() {
    let (w, h) = (64usize, 600usize);
    // 左半锚点（亮度 0.8，不饱和）；右半亮度锯齿 0.9 → 2.3（每
    // 100 行一个齿），只有 R 触顶。锯齿保证饱和带横跨 y=256/512
    // 两个块边界（触发条件，见文件头注释）。
    let ratio = [1.0f32, 0.419, 0.305];
    let mut img = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            let l = if x < w / 2 {
                0.8
            } else {
                0.9 + ((y % 100) as f32 / 99.0) * 1.4
            };
            let truth = [l * ratio[0], l * ratio[1], l * ratio[2]];
            let obs = observe(truth);
            img[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&obs);
        }
    }
    // 先过保底（与判据 A 同口径），再分两路跑真色度恢复。
    let mut floor = img.clone();
    highlight_recover(&mut floor, w, h, WB_DAY, None).unwrap();
    // 断言输入确实触发了要检查的情况：右半最右列确有饱和行、且
    // 饱和带横跨两个块边界（否则测试恒绿）。
    let clipped = |y: usize| img[(y * w + w - 1) * 3] >= WB_DAY[0];
    assert!((0..h).any(clipped), "测试图右半应有饱和行（R 触顶）");
    assert!(
        (0..256).any(clipped) && (256..h).any(clipped),
        "饱和带应横跨 y=256 块边界"
    );
    assert!(
        (0..512).any(clipped) && (512..h).any(clipped),
        "饱和带应横跨 y=512 块边界"
    );
    let mut tiled = floor.clone();
    highlight_chroma_recover(&mut tiled, w, h, WB_DAY, None).unwrap();
    let mut reference = floor.clone();
    highlight_chroma_recover_full_frame_ref(&mut reference, w, h, WB_DAY, None);
    assert_eq!(
        tiled, reference,
        "分块实现必须与全幅实现逐位一致（块边界/晕边处最先露馅）"
    );
}
