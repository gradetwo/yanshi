//! 设计 v3 的判据测试：每条都对着两份实证报告（VERIFY-Q1Q5／VERIFY-Q2Q3Q4）
//! 的**方法与恒等式**断，不对着单点数字断（仓库纪律：恒等式优于单点断言；
//! 断言必须能红——钨丝灯一组特意保留了「WB 后置」的反面对照路，它必须
//! 复现出 0.95/0.225 的撕裂，证明测量本身有牙，前置路再断撕裂不存在）。

use yanshi_raw::develop::{
    apply_wb, composite_color_matrix, demosaic, normalize_mosaic, renormalized_wb_multipliers,
};
use yanshi_raw::frame::{CalibrationMatrix, RecordContext};
use yanshi_raw::{develop, CfaPattern, Channel, Demosaic, DevelopOptions, RawError, RawFrame};

/// 现实量纲的双标定（与合成 DNG 夹具同源数值，D65／A 两端）。
fn realistic_calibrations() -> Vec<CalibrationMatrix> {
    vec![
        CalibrationMatrix {
            illuminant_kelvin: Some(2856.0),
            xyz_to_cam: [
                [0.5844, -0.0506, -0.1067],
                [-0.6724, 1.5265, 0.2291],
                [-0.0837, 0.1436, 0.6759],
            ],
        },
        CalibrationMatrix {
            illuminant_kelvin: Some(6504.0),
            xyz_to_cam: [
                [0.7355, -0.1828, -0.0836],
                [-0.4262, 1.2845, 0.2576],
                [-0.0698, 0.1123, 0.7846],
            ],
        },
    ]
}

fn base_frame(width: usize, height: usize) -> RawFrame {
    RawFrame {
        width,
        height,
        mosaic: vec![0.0; width * height],
        cfa: CfaPattern::Rggb,
        black_levels: [512.0; 4],
        white_levels: [16383.0; 4],
        wb_multipliers: Some([1.72, 1.0, 1.39]),
        as_shot_neutral: None,
        color_matrices: realistic_calibrations(),
        baseline_exposure_ev: None,
        orientation: 1,
    }
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values[values.len() / 2]
}

// ---------------------------------------------------------------------------
// 判据 1（VERIFY V3）：MHC 钨丝灯反例——WB 后置时校正项被原生响应撕裂
// （0.95 vs 0.225、比 4.22、B@R 系数 3.17），WB 前置（本包管线顺序）时撕裂消失。
// ---------------------------------------------------------------------------

const T_W: usize = 48;
const T_H: usize = 48;

/// 已知场 E（二次曲面 ⇒ 轴向拉普拉斯为解析常数）：
/// E = 0.25 + 0.08u + 0.06v + 0.30(u²+v²)，u=x/47、v=y/47。
/// 轴向拉普拉斯 lapE = −2(qx+qy)，qx=qy=0.30/47² ⇒ 常数。
fn known_field(x: usize, y: usize) -> f32 {
    let u = x as f32 / 47.0;
    let v = y as f32 / 47.0;
    0.25 + 0.08 * u + 0.06 * v + 0.30 * (u * u + v * v)
}

fn lap_e_axial() -> f32 {
    let q = 0.30f32 / (47.0 * 47.0);
    -2.0 * (q + q)
}

/// 钨丝灯原生响应（R=1.9E、G=1.0E、B=0.45E）采样出的马赛克平面。
fn tungsten_native_plane() -> Vec<f32> {
    let mut plane = vec![0.0f32; T_W * T_H];
    for y in 0..T_H {
        for x in 0..T_W {
            let response = match CfaPattern::Rggb.channel_at(x, y) {
                Channel::R => 1.9,
                Channel::G => 1.0,
                Channel::B => 0.45,
            };
            plane[y * T_W + x] = response * known_field(x, y);
        }
    }
    plane
}

/// 量 (MHC − Bilinear) 在指定通道／指定采样位上除以 lapE 的中位比。
/// `post_gains` 给定时先对两个输出逐通道乘增益再作差（WB 后置对照路）。
fn correction_ratio(
    plane: &[f32],
    channel: Channel,
    site: Channel,
    post_gains: Option<[f32; 3]>,
) -> f32 {
    let mhc = demosaic(plane, T_W, T_H, CfaPattern::Rggb, Demosaic::MalvarHeCutler);
    let bilin = demosaic(plane, T_W, T_H, CfaPattern::Rggb, Demosaic::Bilinear);
    let mut ratios = Vec::new();
    for y in 3..T_H - 3 {
        for x in 3..T_W - 3 {
            if CfaPattern::Rggb.channel_at(x, y) != site {
                continue;
            }
            let o = (y * T_W + x) * 3 + channel.index();
            let mut diff = mhc[o] - bilin[o];
            if let Some(gains) = post_gains {
                diff *= gains[channel.index()];
            }
            ratios.push(diff / lap_e_axial());
        }
    }
    median(&mut ratios)
}

#[test]
fn tungsten_post_wb_control_reproduces_the_tear() {
    // 对照路（专家反例的原始形态）：先解马赛克、后乘 WB 增益。
    // 校正项里的拉普拉斯取自**未白平衡**的原生平面，于是 G 通道校正量
    // 在 R 位是 α·1.9 = 0.95、在 B 位是 α·0.45 = 0.225 —— 比值恰为原生
    // 响应比 1.9/0.45 = 4.22；B 通道在 R 位的校正系数是 gB·γ·1.9 = 3.17。
    // 这一组必须成立，否则是测量没牙、下一条的「无撕裂」不算数。
    let plane = tungsten_native_plane();
    let gains = [1.0 / 1.9, 1.0, 1.0 / 0.45];
    let g_at_r = correction_ratio(&plane, Channel::G, Channel::R, Some(gains));
    let g_at_b = correction_ratio(&plane, Channel::G, Channel::B, Some(gains));
    let b_at_r = correction_ratio(&plane, Channel::B, Channel::R, Some(gains));
    assert!(
        (g_at_r - 0.95).abs() < 0.095,
        "后置路 G@R 应 ≈0.95，实得 {g_at_r}"
    );
    assert!(
        (g_at_b - 0.225).abs() < 0.0225,
        "后置路 G@B 应 ≈0.225，实得 {g_at_b}"
    );
    assert!(
        ((g_at_r / g_at_b) - 4.2222).abs() < 0.43,
        "后置路撕裂比应 ≈4.22，实得 {}",
        g_at_r / g_at_b
    );
    assert!(
        (b_at_r - 3.1667).abs() < 0.32,
        "后置路 B@R 系数应 ≈3.17，实得 {b_at_r}"
    );
}

#[test]
fn tungsten_pre_wb_pipeline_has_no_tear() {
    // 管线实际顺序（设计定版）：归一化 → WB（重归一化 min=1）→ MHC。
    // WB 后三通道平面同为 s·E（s = 公共标量），校正量在 R 位与 B 位
    // **相等**——0.95/0.225 的现象不出现、4.22 撕裂不存在。
    let native = tungsten_native_plane();
    let mut frame = base_frame(T_W, T_H);
    frame.black_levels = [0.0; 4];
    frame.white_levels = [20000.0; 4];
    frame.mosaic = native.iter().map(|&v| v * 10000.0).collect();
    frame.wb_multipliers = Some([1.0 / 1.9, 1.0, 1.0 / 0.45]);
    let mut plane = normalize_mosaic(&frame).unwrap();
    let multipliers = renormalized_wb_multipliers(&frame).unwrap();
    apply_wb(&mut plane, &frame, multipliers);
    let g_at_r = correction_ratio(&plane, Channel::G, Channel::R, None);
    let g_at_b = correction_ratio(&plane, Channel::G, Channel::B, None);
    let b_at_r = correction_ratio(&plane, Channel::B, Channel::R, None);
    let r_at_b = correction_ratio(&plane, Channel::R, Channel::B, None);
    // 公共标量 s 的解析值：归一化把原生平面除以 20000/10000 = 2，
    // min 重归一化把增益 [1/1.9, 1, 1/0.45] 变成 [1, 1.9, 4.222…]，
    // 于是三通道平面同为 (1.9/2)·E ⇒ s = 0.95。
    // 解析期望：G 校正 α·s = 0.475（两位相等）、R↔B 互校正 γ·s = 0.7125。
    let s = 1.9f32 * 10000.0 / 20000.0;
    assert!(
        (g_at_r - 0.5 * s).abs() < 0.03,
        "前置路 G@R 应 ≈{}，实得 {g_at_r}",
        0.5 * s
    );
    assert!(
        (g_at_b - 0.5 * s).abs() < 0.03,
        "前置路 G@B 应 ≈{}（无撕裂），实得 {g_at_b}",
        0.5 * s
    );
    let tear = g_at_r / g_at_b;
    assert!(
        (0.95..=1.05).contains(&tear),
        "前置路撕裂比应 ≈1，实得 {tear}"
    );
    assert!(
        (b_at_r - 0.75 * s).abs() < 0.04,
        "前置路 B@R 应 ≈{}，实得 {b_at_r}",
        0.75 * s
    );
    assert!(
        (r_at_b - 0.75 * s).abs() < 0.04,
        "前置路 R@B 应 ≈{}，实得 {r_at_b}",
        0.75 * s
    );
}

// ---------------------------------------------------------------------------
// 判据 2（VERIFY W3）：中性 [1,1,1] ↦ [1,1,1]，误差 < 1e-5。
// 用现实双标定 + 非平凡 WB 走完整 develop：常数场经全链必须仍是常数，
// 这条恒等式由复合矩阵的行和归一化保证（单点断言换成全场逐像素断）。
// ---------------------------------------------------------------------------

#[test]
fn neutral_constant_field_survives_full_develop() {
    // ★ 中性场景的造法：原始采样必须 ∝ AsShotNeutral n（中性反射面在相机
    // 上的响应就是 n 的形状），WB 后才成为 [v, v, v]。若把马赛克写成跨通道
    // 常数，那是「相机原生恰好等响应」的非中性色，链输出本就不该中性——
    // 第一版测试正是这样写错、被实测数字当场纠正（0.876 ≠ 0.346 是链对、
    // 测试错）。此处 n 由 wb_multipliers [1.72, 1, 1.39] 导出。
    let mut frame = base_frame(16, 16);
    let n = [1.0f32 / 1.72, 1.0, 1.0 / 1.39];
    let v = 0.3458f32;
    for y in 0..16 {
        for x in 0..16 {
            let channel = CfaPattern::Rggb.channel_at(x, y);
            // 归一化后该通道采样 = v · n_c（逐相位同值，黑电平全相位同为 512）。
            frame.mosaic[y * 16 + x] = 512.0 + v * n[channel.index()] * (16383.0 - 512.0);
        }
    }
    let out = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .unwrap();
    for (i, pixel) in out.rgb_linear.chunks_exact(3).enumerate() {
        for (c, &value) in pixel.iter().enumerate() {
            assert!(
                (value - v).abs() < 1e-5,
                "像素 {i} 通道 {c} 应为中性 {v}，实得 {value}"
            );
        }
    }
    // 双标定走了插值路（α 有值），且留痕字段与输入一致。
    assert!(out.record.color_alpha.is_some(), "双标定应产生插值权重 α");
    assert_eq!(out.record.cfa, "RGGB");
    assert_eq!(out.record.version, 1);
}

// ---------------------------------------------------------------------------
// 判据 3（VERIFY V5）：黑电平四相位独立——逐相位对解析值，
// 且 G1/G2 相位差恰为 δ 的解析效应（合并成标量的实现会让调制归零 ⇒ 红）。
// ---------------------------------------------------------------------------

#[test]
fn four_phase_black_levels_stay_independent() {
    let mut frame = base_frame(16, 16);
    frame.black_levels = [512.0, 544.0, 512.0, 512.0]; // 相位 1（G 位之一）+32
    frame.white_levels = [16383.0; 4];
    frame.mosaic = vec![3000.0; 16 * 16];
    let plane = normalize_mosaic(&frame).unwrap();
    let mut per_phase = [f32::NAN; 4];
    for y in 0..16 {
        for x in 0..16 {
            let phase = CfaPattern::phase_at(x, y);
            let value = plane[y * 16 + x];
            if per_phase[phase].is_nan() {
                per_phase[phase] = value;
            } else {
                assert_eq!(per_phase[phase], value, "同相位内必须恒定");
            }
        }
    }
    for (phase, &actual) in per_phase.iter().enumerate() {
        let black = frame.black_levels[phase];
        let expected = (3000.0f32 - black) / (16383.0 - black);
        assert!(
            (actual - expected).abs() < 1e-6,
            "相位 {phase} 应为解析值 {expected}，实得 {actual}"
        );
    }
    // 棋盘调制：两 G 相位之差应等于 δ 的解析效应（≈0.0017），不是 0。
    let modulation = (per_phase[1] - per_phase[2]).abs();
    let expected_modulation =
        ((3000.0f32 - 512.0) / (16383.0 - 512.0) - (3000.0f32 - 544.0) / (16383.0 - 544.0)).abs();
    assert!(
        (modulation - expected_modulation).abs() < 1e-6 && modulation > 0.001,
        "G 相位调制应为 {expected_modulation}，实得 {modulation}（合并黑电平会让它归零）"
    );
}

// ---------------------------------------------------------------------------
// 判据 4（设计 5.2／5.5）：出口无负值、NaN 不出门。
// ---------------------------------------------------------------------------

#[test]
fn exit_has_no_negatives_and_nan_never_leaves() {
    let mut frame = base_frame(16, 16);
    // 高反差棋盘（拉普拉斯符号交替 ⇒ 解马赛克与色彩链必然产生负中间值），
    // 再注入 NaN／+Inf 坏读数：出口仍必须全有限、非负。
    for y in 0..16 {
        for x in 0..16 {
            frame.mosaic[y * 16 + x] = if (x + y) % 2 == 0 { 16000.0 } else { 600.0 };
        }
    }
    frame.mosaic[0] = f32::NAN;
    frame.mosaic[37] = f32::INFINITY;
    let out = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .unwrap();
    assert!(
        out.rgb_linear.iter().all(|&v| v.is_finite()),
        "出口不许有 NaN/Inf"
    );
    assert!(out.rgb_linear.iter().all(|&v| v >= 0.0), "出口不许有负值");
}

// ---------------------------------------------------------------------------
// 判据 5（设计 5.5）：复合色彩链——Bradford 在位（只测中性会被行和归一
// 掩盖，VERIFY W3(c) 教训）：已知 XYZ 色经前向模型造相机 RGB，链输出
// 必须与「独立用公布常数算出的 Bradford 适应期望」一致；且断言去掉
// Bradford 时误差会大一个量级（判据有牙）。
// ---------------------------------------------------------------------------

fn mat_vec(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

#[test]
fn composite_chain_applies_bradford_for_non_d65_white() {
    let cm = [
        [0.7355, -0.1828, -0.0836],
        [-0.4262, 1.2845, 0.2576],
        [-0.0698, 0.1123, 0.7846],
    ];
    let w_d50 = [0.9642f32, 1.0, 0.8251]; // 场景白（D50），明显不是 D65
    let neutral = mat_vec(&cm, w_d50); // n = CM · w ⇒ CM⁻¹ · n = w
    let cals = [CalibrationMatrix {
        illuminant_kelvin: Some(5000.0),
        xyz_to_cam: cm,
    }];
    let mut notes = Vec::new();
    let composite = composite_color_matrix(&cals, neutral, &mut notes).unwrap();
    let (m_total, alpha, w_xyz) = (composite.matrix, composite.alpha, composite.white_xyz);
    assert!(alpha.is_none(), "单标定不应有插值权重");
    for i in 0..3 {
        assert!(
            (w_xyz[i] - w_d50[i]).abs() < 1e-4,
            "链应还原场景白点：w_xyz={w_xyz:?}，期望 {w_d50:?}"
        );
    }
    // 已知 XYZ 色 → 前向造「已白平衡相机 RGB」u = (CM·c) ./ n。
    let c = [0.30f32, 0.12, 0.02];
    let cam = mat_vec(&cm, c);
    let u = [
        cam[0] / neutral[0],
        cam[1] / neutral[1],
        cam[2] / neutral[2],
    ];
    let got = mat_vec(&m_total, u);
    // 独立期望：Bradford 用公布的锥矩阵与其逆矩阵字面值在测试内另算一遍，
    // 再乘公布的 XYZ(D65)→sRGB 常数（行和归一在合法输入下 ≈ 恒等，只差
    // 公布常数自身的舍入 ~1e-4，故容差取 3e-3；而漏掉 Bradford 的误差
    // 在 D50 白下是百分量级，下面单独断言这个差距存在）。
    const BRADFORD: [[f32; 3]; 3] = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    const BRADFORD_INV: [[f32; 3]; 3] = [
        [0.9869929, -0.1470543, 0.1599627],
        [0.4323053, 0.5183603, 0.0492912],
        [-0.0085287, 0.0400428, 0.9684867],
    ];
    const SRGB: [[f32; 3]; 3] = [
        [3.2404542, -1.5371385, -0.4985314],
        [-0.969266, 1.8760108, 0.0415560],
        [0.0556434, -0.2040259, 1.0572252],
    ];
    let d65 = [0.95047f32, 1.0, 1.08883];
    let cone_src = mat_vec(&BRADFORD, w_d50);
    let cone_dst = mat_vec(&BRADFORD, d65);
    let cone_c = mat_vec(&BRADFORD, c);
    let adapted = mat_vec(
        &BRADFORD_INV,
        [
            cone_c[0] * cone_dst[0] / cone_src[0],
            cone_c[1] * cone_dst[1] / cone_src[1],
            cone_c[2] * cone_dst[2] / cone_src[2],
        ],
    );
    let expected = mat_vec(&SRGB, adapted);
    let no_bradford = mat_vec(&SRGB, c);
    for i in 0..3 {
        assert!(
            (got[i] - expected[i]).abs() < 3e-3,
            "通道 {i} 应与 Bradford 期望 {expected:?} 一致，实得 {got:?}"
        );
    }
    let tear = (0..3)
        .map(|i| (expected[i] - no_bradford[i]).abs())
        .fold(0.0f32, f32::max);
    assert!(
        tear > 0.02,
        "本场景下 Bradford 不可或缺（与无适应之差应 >0.02，实得 {tear}）"
    );
}

// ---------------------------------------------------------------------------
// 其余行为锁：四排列恒等场、WB 重归一化、pre_exposure 契约、电平非法、
// 极小图不 panic、McCamy 在 develop.rs 的单元测试里（此处不重复）。
// ---------------------------------------------------------------------------

#[test]
fn constant_color_field_is_exact_for_every_pattern_and_kind() {
    for pattern in [
        CfaPattern::Rggb,
        CfaPattern::Bggr,
        CfaPattern::Grbg,
        CfaPattern::Gbrg,
    ] {
        let mut plane = vec![0.0f32; 12 * 12];
        for y in 0..12 {
            for x in 0..12 {
                plane[y * 12 + x] = match pattern.channel_at(x, y) {
                    Channel::R => 0.8,
                    Channel::G => 0.5,
                    Channel::B => 0.3,
                };
            }
        }
        for kind in [Demosaic::MalvarHeCutler, Demosaic::Bilinear] {
            let out = demosaic(&plane, 12, 12, pattern, kind);
            for pixel in out.chunks_exact(3) {
                assert!(
                    (pixel[0] - 0.8).abs() < 1e-6
                        && (pixel[1] - 0.5).abs() < 1e-6
                        && (pixel[2] - 0.3).abs() < 1e-6,
                    "{pattern:?}/{kind:?} 恒色场应逐像素 [0.8, 0.5, 0.3]，实得 {pixel:?}"
                );
            }
        }
    }
}

#[test]
fn wb_renormalization_locks_min_to_one() {
    let mut frame = base_frame(4, 4);
    frame.wb_multipliers = Some([0.5, 0.25, 1.0]);
    assert_eq!(
        renormalized_wb_multipliers(&frame).unwrap(),
        [2.0, 1.0, 4.0]
    );
    frame.wb_multipliers = Some([2.0, 1.0, 4.0]);
    assert_eq!(
        renormalized_wb_multipliers(&frame).unwrap(),
        [2.0, 1.0, 4.0]
    );
    // 只有中性坐标时：n=[0.5, 1, 0.25] ⇒ 原始乘子 [2, 1, 4] ⇒ 已归一。
    frame.wb_multipliers = None;
    frame.as_shot_neutral = Some([0.5, 1.0, 0.25]);
    assert_eq!(
        renormalized_wb_multipliers(&frame).unwrap(),
        [2.0, 1.0, 4.0]
    );
}

#[test]
fn pre_exposure_gain_and_contract_range() {
    let mut frame = base_frame(8, 8);
    frame.mosaic = vec![6000.0; 64];
    let base = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .unwrap();
    let mut options = DevelopOptions {
        pre_exposure_ev: 1.0,
        ..Default::default()
    };
    let doubled = develop(&frame, &options, &RecordContext::default()).unwrap();
    let ratio = doubled.rgb_linear[0] / base.rgb_linear[0];
    assert!((ratio - 2.0).abs() < 1e-5, "+1EV 应恰好 ×2，实得 {ratio}");
    // BaselineExposure 生效：−0.5EV ⇒ ×2^-0.5（(2^(−1/2)) 的 f32 期望）。
    frame.baseline_exposure_ev = Some(-0.5);
    let based = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .unwrap();
    let ratio = based.rgb_linear[0] / base.rgb_linear[0];
    assert!(
        (ratio - 0.70710677).abs() < 1e-6,
        "Baseline −0.5EV 应 ×0.7071，实得 {ratio}"
    );
    assert_eq!(based.record.baseline_exposure_ev, -0.5);
    // 契约越界：报错不 clamp。
    options.pre_exposure_ev = 4.5;
    assert!(develop(&frame, &options, &RecordContext::default()).is_err());
    options.pre_exposure_ev = -4.0;
    assert!(develop(&frame, &options, &RecordContext::default()).is_ok());
}

#[test]
fn invalid_levels_are_rejected_not_divided_through() {
    let mut frame = base_frame(8, 8);
    frame.white_levels[2] = frame.black_levels[2];
    let error = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect_err("white ≤ black 必须报错");
    assert!(
        matches!(error, RawError::InvalidLevels { phase: 2, .. }),
        "实得 {error:?}"
    );
}

#[test]
fn tiny_images_never_panic() {
    for (w, h) in [(1usize, 1usize), (2, 2), (3, 3), (1, 7), (7, 1)] {
        let plane = vec![0.5f32; w * h];
        for kind in [Demosaic::MalvarHeCutler, Demosaic::Bilinear] {
            let out = demosaic(&plane, w, h, CfaPattern::Rggb, kind);
            assert_eq!(out.len(), w * h * 3);
            assert!(out.iter().all(|v| v.is_finite()));
        }
    }
}
