//! 展开管线（设计 v3 第 5 章）：归一化 → WB → 解马赛克 → 复合色彩链 →
//! pre_exposure → 出口截零。纯函数、无 IO、确定性（NF1）：浮点超越函数只走
//! `libm::` 自由函数，禁止平台相关的 method 调用（交付前 grep 自查）。
//!
//! 本包范围（第一包）：主链如上。高光恢复（④′）与镜头三项（② 暗角／
//! ⑤ 复合重采样）是后续包，注入点在链上已留位、此处不实现、不冒充。

use crate::error::{RawError, Result};
use crate::frame::{
    CalibrationMatrix, CfaPattern, Channel, Demosaic, DevelopOptions, DevelopedImage,
    RawDevelopRecord, RawFrame, RecordContext, WbMode,
};

/// Bradford 锥响应矩阵（DNG Specification 1.6 第 6 章，设计 5.5 写死常数）。
const BRADFORD_CONE: [[f32; 3]; 3] = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

/// XYZ(D65) → 线性 sRGB 矩阵（IEC 61966-2-1，设计 5.5 写死常数）。
const XYZ_D65_TO_SRGB: [[f32; 3]; 3] = [
    [3.2404542, -1.5371385, -0.4985314],
    [-0.969266, 1.8760108, 0.0415560],
    [0.0556434, -0.2040259, 1.0572252],
];

/// CIE D65 参考白（Y=1 归一）。
const D65_WHITE_XYZ: [f32; 3] = [0.95047, 1.0, 1.08883];

// ---------------------------------------------------------------------------
// 基础矩阵运算（3×3，行主序）
// ---------------------------------------------------------------------------

fn mat3_mul(a: &[[f32; 3]; 3], b: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
        }
    }
    out
}

fn mat3_mul_vec(a: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        a[0][0] * v[0] + a[0][1] * v[1] + a[0][2] * v[2],
        a[1][0] * v[0] + a[1][1] * v[1] + a[1][2] * v[2],
        a[2][0] * v[0] + a[2][1] * v[1] + a[2][2] * v[2],
    ]
}

fn mat3_inverse(a: &[[f32; 3]; 3]) -> Result<[[f32; 3]; 3]> {
    let (a00, a01, a02) = (a[0][0], a[0][1], a[0][2]);
    let (a10, a11, a12) = (a[1][0], a[1][1], a[1][2]);
    let (a20, a21, a22) = (a[2][0], a[2][1], a[2][2]);
    let c00 = a11 * a22 - a12 * a21;
    let c01 = a12 * a20 - a10 * a22;
    let c02 = a10 * a21 - a11 * a20;
    let det = a00 * c00 + a01 * c01 + a02 * c02;
    if !det.is_finite() || det.abs() < 1e-12 {
        return Err(RawError::SingularColorMatrix { det });
    }
    let inv_det = 1.0 / det;
    // 伴随矩阵（余子式的转置）× 1/det。
    Ok([
        [
            c00 * inv_det,
            (a02 * a21 - a01 * a22) * inv_det,
            (a01 * a12 - a02 * a11) * inv_det,
        ],
        [
            c01 * inv_det,
            (a00 * a22 - a02 * a20) * inv_det,
            (a02 * a10 - a00 * a12) * inv_det,
        ],
        [
            c02 * inv_det,
            (a01 * a20 - a00 * a21) * inv_det,
            (a00 * a11 - a01 * a10) * inv_det,
        ],
    ])
}

// ---------------------------------------------------------------------------
// ① 归一化（设计 5.2）：v = min((raw − black[c]) / (white[c] − black[c]), 1)
//    下界不钳：负值带符号穿行整条链，只在 ⑨ 出口截零（专家裁决 Q5）。
// ---------------------------------------------------------------------------

/// 正有限数判定（WB 乘子／中性坐标／矩阵行和的合法域）：NaN 与 ±Inf 都不算合法。
fn is_positive_finite(v: f32) -> bool {
    v.is_finite() && v > 0.0
}

/// 逐相位校验电平合法性（white > black），任一相位非法 ⇒ 结构化报错，不除零兜底。
fn validate_levels(frame: &RawFrame) -> Result<()> {
    for phase in 0..4 {
        let (black, white) = (frame.black_levels[phase], frame.white_levels[phase]);
        if white.partial_cmp(&black) != Some(std::cmp::Ordering::Greater) {
            return Err(RawError::InvalidLevels {
                phase,
                white,
                black,
            });
        }
    }
    Ok(())
}

/// ① 归一化：输出与马赛克同尺寸的 f32 平面（可含负值、上界截 1）。
pub fn normalize_mosaic(frame: &RawFrame) -> Result<Vec<f32>> {
    validate_levels(frame)?;
    let mut out = Vec::with_capacity(frame.mosaic.len());
    for (i, &raw) in frame.mosaic.iter().enumerate() {
        let x = i % frame.width;
        let y = i / frame.width;
        let phase = CfaPattern::phase_at(x, y);
        let black = frame.black_levels[phase];
        let denom = frame.white_levels[phase] - black;
        out.push(((raw - black) / denom).min(1.0));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// ③ WB（设计 5.5）：施加前先重归一化到 min_c w_c = 1.0（硬规范，A28），
//    然后在马赛克域逐像素按采样通道乘（WB 前置，专家裁决 Q1）。
// ---------------------------------------------------------------------------

/// 文件口径的原始 WB 乘子（未归一化）：乘子优先；只有中性坐标时取倒数。
pub fn raw_wb_multipliers(frame: &RawFrame) -> Result<[f32; 3]> {
    if let Some(w) = frame.wb_multipliers {
        return Ok(w);
    }
    if let Some(n) = frame.as_shot_neutral {
        if n.iter().any(|&v| !is_positive_finite(v)) {
            return Err(RawError::MissingWhiteBalance);
        }
        return Ok([1.0 / n[0], 1.0 / n[1], 1.0 / n[2]]);
    }
    Err(RawError::MissingWhiteBalance)
}

/// 相机原生中性坐标 n（复合色彩链的 diag(n) 用）：中性坐标优先；
/// 只有乘子时取倒数。n 的整体尺度会被行和归一化与白点 Y 归一吸收，不影响结果。
pub fn as_shot_neutral(frame: &RawFrame) -> Result<[f32; 3]> {
    if let Some(n) = frame.as_shot_neutral {
        if n.iter().any(|&v| !is_positive_finite(v)) {
            return Err(RawError::MissingWhiteBalance);
        }
        return Ok(n);
    }
    let w = raw_wb_multipliers(frame)?;
    if w.iter().any(|&v| !is_positive_finite(v)) {
        return Err(RawError::MissingWhiteBalance);
    }
    Ok([1.0 / w[0], 1.0 / w[1], 1.0 / w[2]])
}

/// 重归一化（硬规范）：w_c = w0_c / min(w0)，使 min_c w_c = 1.0——
/// 只有此归一化下，最先饱和的通道天花板恰好落在 1.0，
/// [0, 1] 才是三通道未裁剪的有效动态范围（设计 5.5／附带意见②）。
pub fn renormalized_wb_multipliers(frame: &RawFrame) -> Result<[f32; 3]> {
    let w0 = raw_wb_multipliers(frame)?;
    if w0.iter().any(|&v| !is_positive_finite(v)) {
        return Err(RawError::MissingWhiteBalance);
    }
    let min = w0[0].min(w0[1]).min(w0[2]);
    Ok([w0[0] / min, w0[1] / min, w0[2] / min])
}

/// ③ 把重归一化后的乘子逐像素乘到归一化马赛克平面上（按采样通道）。
pub fn apply_wb(plane: &mut [f32], frame: &RawFrame, multipliers: [f32; 3]) {
    for (i, value) in plane.iter_mut().enumerate() {
        let x = i % frame.width;
        let y = i / frame.width;
        let channel = frame.cfa.channel_at(x, y);
        *value *= multipliers[channel.index()];
    }
}

// ---------------------------------------------------------------------------
// ④ 解马赛克（设计 5.4）：MHC 默认、Bilinear 为回归基线（校正系数全零）。
//    边界一律镜像反射（reflect-101，不复制边缘、不补零），支持域 5×5。
// ---------------------------------------------------------------------------

/// reflect-101 索引：把任意整数坐标折回 [0, n)。n=1 时恒 0。
fn reflect_index(i: isize, n: usize) -> usize {
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

/// ④ 解马赛克：输入归一化 + 已乘 WB 的马赛克平面，输出 RGB 三元组平面。
pub fn demosaic(
    plane: &[f32],
    width: usize,
    height: usize,
    cfa: CfaPattern,
    kind: Demosaic,
) -> Vec<f32> {
    debug_assert_eq!(plane.len(), width * height);
    // 校正系数（分母 8 的二进有理数，逐位确定）：Bilinear 即三项全零（设计 5.4）。
    let (alpha, beta, gamma) = match kind {
        Demosaic::MalvarHeCutler => (0.5f32, 0.625f32, 0.75f32),
        Demosaic::Bilinear => (0.0, 0.0, 0.0),
    };
    let sample = |x: isize, y: isize| -> f32 {
        plane[reflect_index(y, height) * width + reflect_index(x, width)]
    };
    let mut out = vec![0.0f32; width * height * 3];
    for y in 0..height as isize {
        for x in 0..width as isize {
            let here = cfa.channel_at(x as usize, y as usize);
            let center = sample(x, y);
            // 同通道轴向距-2 拉普拉斯（步距 2 只落在同通道子网格上；取轴向、非对角——
            // 实证原型的对角误用曾测出假阴性，教训见 VERIFY-Q1Q5 V2）。
            let lap_axial = center
                - (sample(x - 2, y) + sample(x + 2, y) + sample(x, y - 2) + sample(x, y + 2)) / 4.0;
            let mut rgb = [0.0f32; 3];
            rgb[here.index()] = center;
            for channel in [Channel::R, Channel::G, Channel::B] {
                if channel == here {
                    continue;
                }
                let estimate = if channel == Channel::G {
                    // G 在 R/B 采样点：四轴向邻居（全是 G）均值 + α·Δ_本通道。
                    let base =
                        (sample(x - 1, y) + sample(x + 1, y) + sample(x, y - 1) + sample(x, y + 1))
                            / 4.0;
                    base + alpha * lap_axial
                } else if here == Channel::G {
                    // R/B 在 G 采样点：沿该通道所在方向取 2 邻居均值 + β·Δ_G^diag。
                    let horizontal = cfa.channel_at((x + 1) as usize, y as usize) == channel;
                    let base = if horizontal {
                        (sample(x - 1, y) + sample(x + 1, y)) / 2.0
                    } else {
                        (sample(x, y - 1) + sample(x, y + 1)) / 2.0
                    };
                    let lap_diag = center
                        - (sample(x - 1, y - 1)
                            + sample(x - 1, y + 1)
                            + sample(x + 1, y - 1)
                            + sample(x + 1, y + 1))
                            / 4.0;
                    base + beta * lap_diag
                } else {
                    // R↔B 互估：四对角同通道样本均值 + γ·Δ_本通道。
                    let base = (sample(x - 1, y - 1)
                        + sample(x - 1, y + 1)
                        + sample(x + 1, y - 1)
                        + sample(x + 1, y + 1))
                        / 4.0;
                    base + gamma * lap_axial
                };
                rgb[channel.index()] = estimate;
            }
            let o = (y as usize * width + x as usize) * 3;
            out[o..o + 3].copy_from_slice(&rgb);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// ⑥ 复合色彩链（设计 5.5，专家裁决 Q3）：
//    M_total = M_XYZ(D65)→sRGB · M_Bradford(w_XYZ → w_D65) · [CM(α)⁻¹ · diag(n)]
//    构造后逐行行和归一化，保证 [1,1,1] ↦ [1,1,1] 精确成立。
// ---------------------------------------------------------------------------

/// McCamy (1992) 相关色温近似，**标准符号形式**（设计 5.5／附录 A20）：
/// n_e = (x − 0.3320) / (y − 0.1858)；校验点 D65 → 6504K、光源 A → 2857K
/// （专家印式的分母符号与多项式不自洽，系笔误，已由实证 W3 钉死）。
pub fn cct_mccamy(x: f32, y: f32) -> f32 {
    let ne = (x - 0.3320) / (y - 0.1858);
    let t = -449.0 * ne * ne * ne + 3525.0 * ne * ne - 6823.3 * ne + 5520.33;
    t.clamp(1500.0, 25000.0)
}

/// Bradford 色适应矩阵：把参考白从 `src_white` 搬到 `dst_white`（XYZ 域）。
fn bradford_adaptation(src_white: [f32; 3], dst_white: [f32; 3]) -> Result<[[f32; 3]; 3]> {
    let src_cone = mat3_mul_vec(&BRADFORD_CONE, src_white);
    let dst_cone = mat3_mul_vec(&BRADFORD_CONE, dst_white);
    if src_cone.iter().any(|&v| v == 0.0 || !v.is_finite()) {
        return Err(RawError::InvalidFrame(
            "场景白点的 Bradford 锥响应含零／非有限分量".to_string(),
        ));
    }
    let scale = [
        [dst_cone[0] / src_cone[0], 0.0, 0.0],
        [0.0, dst_cone[1] / src_cone[1], 0.0],
        [0.0, 0.0, dst_cone[2] / src_cone[2]],
    ];
    let cone_inv = mat3_inverse(&BRADFORD_CONE)?;
    Ok(mat3_mul(&mat3_mul(&cone_inv, &scale), &BRADFORD_CONE))
}

/// 在两组标定之间按 mired 域权重 α 逐元素线性插值（先插后逆，DNG 标准路线）。
fn interpolate_cm(low: &CalibrationMatrix, high: &CalibrationMatrix, alpha: f32) -> [[f32; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = alpha * low.xyz_to_cam[i][j] + (1.0 - alpha) * high.xyz_to_cam[i][j];
        }
    }
    out
}

/// 复合色彩链的产物：矩阵本体 + 实际插值权重 + 场景白点。
pub struct CompositeColor {
    /// 复合矩阵 M_total（已逐行行和归一化）。
    pub matrix: [[f32; 3]; 3],
    /// 双标定插值实际用的 α（单矩阵时 `None`）。
    pub alpha: Option<f32>,
    /// 场景白点 w_XYZ（Y=1 归一）。
    pub white_xyz: [f32; 3],
}

/// 构造复合色彩矩阵。
/// 双标定且两端色温齐备时走 5 次不动点迭代（设计 5.5）；缺色温时退单矩阵
/// 并往 `notes` 里注明（不静默）。
pub fn composite_color_matrix(
    calibrations: &[CalibrationMatrix],
    neutral: [f32; 3],
    notes: &mut Vec<String>,
) -> Result<CompositeColor> {
    if calibrations.is_empty() {
        return Err(RawError::MissingColorMatrix);
    }
    // 选标定：单组直接用；多组取色温两端（缺色温的端退化为「用第一组」+ 注记）。
    let (cm, alpha) = if calibrations.len() == 1 {
        (calibrations[0].xyz_to_cam, None)
    } else {
        let mut sorted: Vec<&CalibrationMatrix> = calibrations.iter().collect();
        sorted.sort_by(|a, b| {
            a.illuminant_kelvin
                .unwrap_or(f32::INFINITY)
                .partial_cmp(&b.illuminant_kelvin.unwrap_or(f32::INFINITY))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let low = sorted[0];
        let high = sorted[sorted.len() - 1];
        match (low.illuminant_kelvin, high.illuminant_kelvin) {
            (Some(t_low), Some(t_high)) if t_high > t_low => {
                // 二维色度不动点迭代（固定 5 次）：(x, y) → CCT → α → CM(α) → 新白点。
                let m1 = 1.0e6 / t_low;
                let m2 = 1.0e6 / t_high;
                let (mut x, mut y) = (0.3127f32, 0.3290f32); // D65 初值（DNG 规范）
                let mut cm = low.xyz_to_cam;
                let mut alpha = 1.0f32;
                for _ in 0..5 {
                    let t = cct_mccamy(x, y);
                    let mired = 1.0e6 / t;
                    alpha = ((mired - m2) / (m1 - m2)).clamp(0.0, 1.0);
                    cm = interpolate_cm(low, high, alpha);
                    let w = mat3_mul_vec(&mat3_inverse(&cm)?, neutral);
                    let sum = w[0] + w[1] + w[2];
                    if !is_positive_finite(sum) {
                        return Err(RawError::InvalidFrame(
                            "场景白点色度迭代发散（XYZ 和非正）".to_string(),
                        ));
                    }
                    x = w[0] / sum;
                    y = w[1] / sum;
                }
                (cm, Some(alpha))
            }
            _ => {
                notes.push(
                    "双标定矩阵有一端缺色温（无 CCT 定版值），未做插值、退为单矩阵展开".to_string(),
                );
                (low.xyz_to_cam, None)
            }
        }
    };
    let cm_inv = mat3_inverse(&cm)?;
    let w_raw = mat3_mul_vec(&cm_inv, neutral);
    if !is_positive_finite(w_raw[1]) {
        return Err(RawError::InvalidFrame(format!(
            "场景白点 Y 分量非法：{w_raw:?}"
        )));
    }
    let w_xyz = [w_raw[0] / w_raw[1], 1.0, w_raw[2] / w_raw[1]];
    let bradford = bradford_adaptation(w_xyz, D65_WHITE_XYZ)?;
    // CM⁻¹ · diag(n)：先把已白平衡坐标还原回相机原生比例（n 的尺度随后被吸收）。
    let mut cm_inv_diag = cm_inv;
    for row in cm_inv_diag.iter_mut() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell *= neutral[j];
        }
    }
    let mut m_total = mat3_mul(&XYZ_D65_TO_SRGB, &mat3_mul(&bradford, &cm_inv_diag));
    // 行和归一化：每行除以该行三元素之和，保证中性 [1,1,1] ↦ [1,1,1] 精确成立。
    for row in m_total.iter_mut() {
        let sum: f32 = row.iter().sum();
        if !is_positive_finite(sum) {
            return Err(RawError::InvalidFrame(format!(
                "复合色彩矩阵行和非法：{row:?}"
            )));
        }
        for cell in row.iter_mut() {
            *cell /= sum;
        }
    }
    Ok(CompositeColor {
        matrix: m_total,
        alpha,
        white_xyz: w_xyz,
    })
}

// ---------------------------------------------------------------------------
// ⑨ 出口截零（设计 5.5 负值策略）：全程带符号、仅此一处 max(v, 0)。
// ---------------------------------------------------------------------------

/// ⑨ 出口截零 + 有限性守门。`f32::max` 在一侧为 NaN 时返回另一侧，
/// 故 NaN 在此被 0 吃掉、不会出门；+Inf 截零后仍非有限 ⇒ 响亮报错。
pub fn clamp_exit(values: &mut [f32]) -> Result<()> {
    for (i, v) in values.iter_mut().enumerate() {
        *v = v.max(0.0);
        if !v.is_finite() {
            return Err(RawError::NonFiniteOutput(format!("像素分量 #{i} = {v}")));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ⑧ EXIF 方向变换（设计 5.7，第二包接入）：在 ⑦ 之后、⑨ 之前作用于
//    线性 RGB 三元组平面。索引映射走内核共用实现（`yanshi_core::orientation`，
//    与 JPEG 的 u8 路同一份，设计 5.7「应抽共用实现」）；本函数只做 f32 搬运。
//    与 ⑨ 的先后可交换（截零是逐像素运算、与置换可交换），但仍按设计链序
//    放在 ⑨ 之前，让链上每一步的输入输出尺寸都与设计图一致、不靠交换性糊弄。
// ---------------------------------------------------------------------------

/// ⑧ 把方向变换作用于线性 RGB 三元组平面，返回（新宽，新高，新平面）。
/// `orientation` 不在 1..=8 时按 1（恒等）处理（兜底口径同设计 5.7）。
pub fn apply_orientation_rgb(
    rgb: &[f32],
    width: usize,
    height: usize,
    orientation: u32,
) -> (usize, usize, Vec<f32>) {
    debug_assert_eq!(rgb.len(), width * height * 3);
    if !(2..=8).contains(&orientation) {
        return (width, height, rgb.to_vec());
    }
    let (out_w, out_h) = yanshi_core::orientation::output_size(width, height, orientation);
    let mut out = vec![0.0f32; out_w * out_h * 3];
    for y in 0..out_h {
        for x in 0..out_w {
            let (sx, sy) = yanshi_core::orientation::source_coord(x, y, width, height, orientation);
            let src = (sy * width + sx) * 3;
            let dst = (y * out_w + x) * 3;
            out[dst..dst + 3].copy_from_slice(&rgb[src..src + 3]);
        }
    }
    (out_w, out_h, out)
}

/// 方向值的生效口径：文件值在 1..=8 时原样，其余（缺失／损坏）按 1。
pub fn effective_orientation(file_value: u32) -> u32 {
    if (1..=8).contains(&file_value) {
        file_value
    } else {
        1
    }
}

/// 组装 `raw_develop` 留痕记录（设计 5.6）：`develop` 与导入计划
/// （`crate::import::plan_import`）共用这一份组装——记录的字段口径只许
/// 有一个出处，两条路组装出的 record 必须逐字段一致。
#[allow(clippy::too_many_arguments)]
pub fn assemble_record(
    frame: &RawFrame,
    options: &DevelopOptions,
    ctx: &RecordContext,
    wb_multipliers: [f32; 3],
    baseline_exposure_ev: f32,
    color_alpha: Option<f32>,
    lens_match: Option<String>,
) -> RawDevelopRecord {
    RawDevelopRecord {
        version: 1,
        source_blob: None,
        source_format: ctx.source_format.clone(),
        camera_make: ctx.camera_make.clone(),
        camera_model: ctx.camera_model.clone(),
        lens: ctx.lens.clone(),
        wb: options.wb,
        demosaic: options.demosaic,
        pre_exposure_ev: options.pre_exposure_ev,
        baseline_exposure_ev,
        wb_multipliers,
        black_levels: frame.black_levels,
        white_levels: frame.white_levels,
        cfa: frame.cfa.name().to_string(),
        orientation_applied: effective_orientation(frame.orientation),
        decoder_name: ctx.decoder_name.clone(),
        decoder_version: ctx.decoder_version.clone(),
        color_alpha,
        lens_exif: ctx.lens_exif.clone(),
        lens_match,
        applied_lens_profile: None,
    }
}

// ---------------------------------------------------------------------------
// 主链：① → ③ → ④ → ⑥ → ⑦ → ⑧ → ⑨（② 暗角／④′ 高光恢复／⑤ 重采样
// 是后续包的注入点，本包不实现、链上位置在此注明，防误读为遗漏）
// ---------------------------------------------------------------------------

/// 展开一帧：设计 5.1 主链中属本包的步骤全量执行。
pub fn develop(
    frame: &RawFrame,
    options: &DevelopOptions,
    ctx: &RecordContext,
) -> Result<DevelopedImage> {
    options.validate()?;
    if frame.width == 0 || frame.height == 0 || frame.mosaic.len() != frame.width * frame.height {
        return Err(RawError::InvalidFrame(format!(
            "帧尺寸 {}×{} 与像素数 {} 对不上",
            frame.width,
            frame.height,
            frame.mosaic.len()
        )));
    }
    let mut notes = Vec::new();
    if options.wb == WbMode::Auto {
        notes.push(
            "wb=auto：一期等同 as_shot（rawler 0.7.2 无独立 daylight 估计入口，设计 5.5 定版）"
                .to_string(),
        );
    }
    // ① 归一化（下界不钳）。
    let mut plane = normalize_mosaic(frame)?;
    // ③ WB 前置：重归一化后逐像素乘在马赛克域（MHC 的标定前提，专家裁决 Q1）。
    let multipliers = renormalized_wb_multipliers(frame)?;
    apply_wb(&mut plane, frame, multipliers);
    // ④ 解马赛克（纯整数网格 MHC／双线性基线）。
    let rgb_camera = demosaic(
        &plane,
        frame.width,
        frame.height,
        frame.cfa,
        options.demosaic,
    );
    // ⑥ 复合色彩链：已白平衡相机 RGB → 线性 sRGB D65。
    let neutral = as_shot_neutral(frame)?;
    let composite = composite_color_matrix(&frame.color_matrices, neutral, &mut notes)?;
    let m_total = composite.matrix;
    let alpha = composite.alpha;
    let mut rgb = vec![0.0f32; rgb_camera.len()];
    for (dst, src) in rgb.chunks_exact_mut(3).zip(rgb_camera.chunks_exact(3)) {
        let out = mat3_mul_vec(&m_total, [src[0], src[1], src[2]]);
        dst.copy_from_slice(&out);
    }
    // ⑦ pre_exposure = 2^(BaselineExposure + user)（必须在高光恢复之后——
    //    ④′ 是后续包，注入时勿把本步提前；Baseline 缺失按 0 并注明）。
    let baseline = match frame.baseline_exposure_ev {
        Some(ev) => ev,
        None => {
            notes.push(
                "BaselineExposure 不可得（rawler 0.7.2 未暴露该标签），按 0 EV 处理".to_string(),
            );
            0.0
        }
    };
    let gain = libm::powf(2.0, baseline + options.pre_exposure_ev);
    for v in rgb.iter_mut() {
        *v *= gain;
    }
    // ⑧ EXIF 方向变换（设计 5.7）：置于 ⑦ 之后、⑨ 之前；5–8 时宽高互换。
    let orientation = effective_orientation(frame.orientation);
    let (out_w, out_h, mut rgb) =
        apply_orientation_rgb(&rgb, frame.width, frame.height, orientation);
    // ⑨ 出口截零（全程唯一截零点）。
    clamp_exit(&mut rgb)?;
    let record = assemble_record(frame, options, ctx, multipliers, baseline, alpha, None);
    Ok(DevelopedImage {
        width: out_w as u32,
        height: out_h as u32,
        rgb_linear: rgb,
        record,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflect_index_mirrors_without_repeating_edge() {
        // reflect-101：周期 2n−2=6，序列 … 2 3 | 2 1 0 1 2 3 | 2 1 0 …
        //（坐标 -4..=7 依次映到 2,3,2,1,0,1,2,3,2,1,0,1——边缘像素不重复）
        let cases = [
            (-4, 2usize),
            (-3, 3),
            (-2, 2),
            (-1, 1),
            (0, 0),
            (3, 3),
            (4, 2),
            (5, 1),
            (6, 0),
            (7, 1),
        ];
        for (input, expected) in cases {
            assert_eq!(reflect_index(input, 4), expected, "reflect({input})");
        }
        assert_eq!(reflect_index(-7, 1), 0);
        assert_eq!(reflect_index(9, 2), 1);
    }

    #[test]
    fn mccamy_calibration_points() {
        // 设计 5.5 写死的校验点（标准符号形式）：D65 → 6504K、光源 A → 2857K。
        let d65 = cct_mccamy(0.31271, 0.32902);
        assert!((d65 - 6504.0).abs() < 25.0, "D65 算得 {d65}K");
        let a = cct_mccamy(0.44757, 0.40745);
        assert!((a - 2857.0).abs() < 25.0, "光源 A 算得 {a}K");
    }

    #[test]
    fn clamp_exit_zeroes_negatives_and_nan_but_rejects_inf() {
        let mut values = [-0.25, 0.5, f32::NAN, 2.0];
        clamp_exit(&mut values).unwrap();
        assert_eq!(values, [0.0, 0.5, 0.0, 2.0]);
        let mut with_inf = [1.0, f32::INFINITY];
        assert!(clamp_exit(&mut with_inf).is_err());
    }
}
