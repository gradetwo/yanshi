//! 展开管线（设计 v3 第 5 章）：归一化 → ② 暗角 → WB → 解马赛克 →
//! 高光恢复（④′ 保底 + ④′′ 真色度）→ ⑤ 畸变/TCA 复合重采样 →
//! 复合色彩链 → pre_exposure → 方向 → 出口截零。纯函数、无 IO、
//! 确定性（NF1）：浮点超越函数只走 `libm::` 自由函数，禁止平台
//! 相关的 method 调用（交付前 grep 自查）。
//!
//! ④′ 高光恢复为第三包实现（保底机制：解析阈值 + 单调性下界 + μ_ref
//! 下限钳 + 色度门控 desat，见 `highlight_recover`）；④′′ 真色度恢复
//! 为第五包（见 `highlight_chroma_recover`），由 `DevelopOptions.
//! highlight` 档位控制，缺省 Floor 时整链与第三包逐位一致。
//! 镜头三项（② 暗角／⑤ 复合重采样）为第四包：参数以
//! `LensCorrectionPlan` 注入（`develop_with_lens`），无 plan 时
//! `develop` 的每一步与前三包逐位一致（D5 纪律：缺省 off 不动像素）。

use crate::error::{RawError, Result};
use crate::frame::{
    AppliedLensProfile, CalibrationMatrix, CfaGrid, CfaPattern, Channel, Demosaic, DevelopOptions,
    DevelopedImage, HighlightMode, LensCorrectionPlan, RawDevelopRecord, RawFrame, RecordContext,
    WarpModel, WbMode,
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
/// 相位数 = CFA 周期²（Bayer 4、X-Trans 36），黑／白两数组须同长且与周期对齐。
fn validate_levels(frame: &RawFrame) -> Result<()> {
    let phases = frame.cfa.period() * frame.cfa.period();
    if frame.black_levels.len() != phases || frame.white_levels.len() != phases {
        return Err(RawError::InvalidFrame(format!(
            "电平数组长度（黑 {}／白 {}）与 CFA 相位数 {phases} 对不上",
            frame.black_levels.len(),
            frame.white_levels.len()
        )));
    }
    for phase in 0..phases {
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
        let phase = frame.cfa.phase_at(x, y);
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
/// 按 CFA 网格分流：Bayer（period=2）走 MHC／Bilinear 原路（逐位不变）；
/// 其余周期网格（X-Trans 等）走 `demosaic_grid` 色差插值路。
pub fn demosaic(
    plane: &[f32],
    width: usize,
    height: usize,
    cfa: CfaGrid,
    kind: Demosaic,
) -> Vec<f32> {
    match cfa.bayer() {
        Some(pattern) => demosaic_bayer(plane, width, height, pattern, kind),
        None => demosaic_grid(plane, width, height, &cfa),
    }
}

/// ④ 解马赛克（Bayer 路）：MHC 默认、Bilinear 为回归基线。
fn demosaic_bayer(
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

/// ④ 解马赛克（通用网格路，ISSUE-3）：非 Bayer 周期网格（X-Trans 等）走
/// 色差插值——先重建 G 全场，再对 R−G／B−G 差分面同法插值，最后
/// R = G + ΔR、B = G + ΔB；采样位本通道取原样本（与 Bayer 路
/// `rgb[here] = center` 同口径，保证高光天花板语义不被插值稀释）。
/// 插值核为距离加权同色均值，权重 `1/(1+d²)` 纯算术、无超越函数
/// （守本包 libm 纪律）；边界 reflect-101（与 Bayer 路同）。
/// 窗口边长取 period+1：任意此大小的窗必含一整周期 ⇒ 目标通道采样
/// 必存在（X-Trans 6×6 内 R／B 各 8 格、G 20 格）。
/// `kind`（MHC／Bilinear 固定核）是 Bayer 专用标定，本路不消费。
/// 已知边界：细密高频纹理的分辨力弱于 Markesteijn 类 X-Trans 专用
/// 算法——属质量边界、非正确性边界（颜色与几何由构造保证）。
///
/// **期 2 C3 的性能形态（语义不变）**：原实现每像素三趟全窗扫描、
/// 每 tap 现算 `chan()`／`sample()`（各带 reflect + 取模）+ `&dyn Fn`
/// 分发，是「展开 18 s vs 解码 0.15 s」的根因。现改为：
/// (a) **相位抽表**（C3-a）：CFA 周期内 period² 种相位，每相位每
/// 目标通道预compute tap 表 `[(dx, dy, w)]`，**按现行 dy/dx 升序
/// 排列**——内部区域（整窗不越界）直索引累加，与旧路加法次序逐项
/// 相同 ⇒ 同平台逐位一致；边界带（reflect 会改变 tap 的实际
/// 通道归属，表覆盖不了）保留逐 tap 现算的慢路，口径与旧实现
/// 逐字对齐。(b) **行带并行**（C3-b）：两趟都按输出行切带，
/// `std::thread::scope` 并行；带间只读共享输入、无跨像素归约
/// ⇒ 与带数无关地逐位一致。wasm 目标整条切除线程路、标量兜底。
fn demosaic_grid(plane: &[f32], width: usize, height: usize, cfa: &CfaGrid) -> Vec<f32> {
    demosaic_grid_impl(
        plane,
        width,
        height,
        cfa,
        GridTapSet::Full,
        grid_band_count(height),
    )
}

/// C3-d 预览快速档：简化 tap 集（d²≤5、空集回退全窗），只供
/// C1-b 预览档展开（record 已记 `developed_max_edge`，预览像素
/// 本就不冒充成品）。全档路径永不走这里。
fn demosaic_grid_quick(plane: &[f32], width: usize, height: usize, cfa: &CfaGrid) -> Vec<f32> {
    demosaic_grid_impl(
        plane,
        width,
        height,
        cfa,
        GridTapSet::Quick,
        grid_band_count(height),
    )
}

/// 网格路的 tap 集：Full = 全窗（位精确口径）；Quick = 预览简化集。
#[derive(Clone, Copy, PartialEq, Eq)]
enum GridTapSet {
    Full,
    Quick,
}

/// 相位 tap 表：一相位 × 三目标通道的 `(dx, dy, w)` 列表，
/// 恒按 dy→dx 升序（= 旧路的累加次序，逐位一致的命门）。
type PhaseTaps = [Vec<(isize, isize, f32)>; 3];

/// 预compute 全部相位的 tap 表。权重表达式与旧路逐字相同
/// （`1.0 / (1.0 + (dx*dx + dy*dy) as f32)`）。Quick 集先按
/// d²≤5 过滤；某（相位，通道）过滤后为空 ⇒ 回退该（相位，
/// 通道）的全窗表（差分面没有同色采样时不许编造 0 值）。
fn build_phase_taps(cfa: &CfaGrid, set: GridTapSet) -> Vec<PhaseTaps> {
    let period = cfa.period();
    let radius = period.div_ceil(2) as isize;
    let mut phases: Vec<PhaseTaps> = Vec::with_capacity(period * period);
    for py in 0..period as isize {
        for px in 0..period as isize {
            let mut per_channel: PhaseTaps = [Vec::new(), Vec::new(), Vec::new()];
            for target in [Channel::R, Channel::G, Channel::B] {
                let mut full: Vec<(isize, isize, f32)> = Vec::new();
                let mut quick: Vec<(isize, isize, f32)> = Vec::new();
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        // 相位内的通道归属只看相对坐标取模——与
                        // channel_at 对窗内坐标的求值一致。
                        let cx = (px + dx).rem_euclid(period as isize) as usize;
                        let cy = (py + dy).rem_euclid(period as isize) as usize;
                        if cfa.channel_at(cx, cy) == target {
                            let w = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
                            full.push((dx, dy, w));
                            if dx * dx + dy * dy <= 5 {
                                quick.push((dx, dy, w));
                            }
                        }
                    }
                }
                per_channel[target.index()] = match set {
                    GridTapSet::Full => full,
                    GridTapSet::Quick => {
                        if quick.is_empty() {
                            full
                        } else {
                            quick
                        }
                    }
                };
            }
            phases.push(per_channel);
        }
    }
    phases
}

/// 慢路（边界带专用，与旧实现逐字同口径）：reflect 坐标下逐 tap
/// 现算通道归属与采样。`diff` = 差分面（采样 − G 面）；Quick 集
/// 过滤后无 tap ⇒ 整窗重扫（与抽表的回退口径一致）。
#[allow(clippy::too_many_arguments)]
fn grid_weighted_slow(
    plane: &[f32],
    green: Option<&[f32]>,
    width: usize,
    height: usize,
    cfa: &CfaGrid,
    radius: isize,
    x: isize,
    y: isize,
    want: Channel,
    diff: bool,
    set: GridTapSet,
) -> f32 {
    let sample = |sx: isize, sy: isize| -> f32 {
        plane[reflect_index(sy, height) * width + reflect_index(sx, width)]
    };
    let value = |sx: isize, sy: isize| -> f32 {
        let base = sample(sx, sy);
        if diff {
            let g = green.expect("差分面必须带 G 面")
                [reflect_index(sy, height) * width + reflect_index(sx, width)];
            base - g
        } else {
            base
        }
    };
    let mut sum = 0.0f32;
    let mut wsum = 0.0f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if set == GridTapSet::Quick && dx * dx + dy * dy > 5 {
                continue;
            }
            let cx = reflect_index(x + dx, width);
            let cy = reflect_index(y + dy, height);
            if cfa.channel_at(cx, cy) == want {
                let w = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
                sum += w * value(x + dx, y + dy);
                wsum += w;
            }
        }
    }
    if wsum == 0.0 && set == GridTapSet::Quick {
        // 过滤后无同色采样 ⇒ 回退全窗（与抽表口径一致）。
        return grid_weighted_slow(
            plane,
            green,
            width,
            height,
            cfa,
            radius,
            x,
            y,
            want,
            diff,
            GridTapSet::Full,
        );
    }
    if wsum > 0.0 {
        sum / wsum
    } else {
        0.0
    }
}

/// 行带数：原生目标按可用并行度（上限 8）与高度决定（矮帧不值
/// 得起线程）；wasm 目标恒 1（线程路整条 cfg 切除）。
#[cfg(not(target_arch = "wasm32"))]
fn grid_band_count(height: usize) -> usize {
    if height < 64 {
        return 1;
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8);
    threads.min((height / 32).max(1))
}

/// wasm 目标：无线程，标量单带（C3-b 的 wasm 纪律）。
#[cfg(target_arch = "wasm32")]
fn grid_band_count(_height: usize) -> usize {
    1
}

/// 按行带执行 `job(y0, chunk)`：chunk 是该带对应的可变切片
/// （由调用方以 chunks_mut 切出，带间不重叠）。原生多带时走
/// `std::thread::scope`；单带与 wasm 直调。
fn run_row_bands<T: Send>(chunks: Vec<&mut [T]>, job: impl Fn(&mut [T], usize) + Sync) {
    if chunks.len() <= 1 {
        for (index, chunk) in chunks.into_iter().enumerate() {
            job(chunk, index);
        }
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let job_ref = &job;
        std::thread::scope(|scope| {
            for (index, chunk) in chunks.into_iter().enumerate() {
                scope.spawn(move || job_ref(chunk, index));
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    {
        for (index, chunk) in chunks.into_iter().enumerate() {
            job(chunk, index);
        }
    }
}

/// 网格路共用实现：两趟（G 全场 → R／B 差分）× 行带并行；
/// 像素级 interior 走相位表、边界带走慢路。`bands` 由入口按
/// `grid_band_count` 给定（判据可显式指定带数验逐位一致）。
fn demosaic_grid_impl(
    plane: &[f32],
    width: usize,
    height: usize,
    cfa: &CfaGrid,
    set: GridTapSet,
    bands: usize,
) -> Vec<f32> {
    debug_assert_eq!(plane.len(), width * height);
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let period = cfa.period();
    let radius = period.div_ceil(2) as isize;
    let taps = build_phase_taps(cfa, set);
    let bands = bands.max(1).min(height.max(1));
    let band_rows = height.div_ceil(bands);
    let (w_i, h_i) = (width as isize, height as isize);
    // interior 判定：整窗不越界 ⇒ reflect 恒等、tap 归属只看相位。
    let interior = |x: isize, y: isize| -> bool {
        x - radius >= 0 && x + radius < w_i && y - radius >= 0 && y + radius < h_i
    };

    // 1) G 全场：G 采样位取原值，其余位置同色加权均值。
    let mut green = vec![0.0f32; width * height];
    {
        let chunks: Vec<&mut [f32]> = green.chunks_mut(band_rows * width).collect();
        run_row_bands(chunks, |chunk, band| {
            let y_start = band * band_rows;
            let rows = chunk.len() / width;
            for ly in 0..rows {
                let y = (y_start + ly) as isize;
                for x in 0..width as isize {
                    let idx = ly * width + x as usize;
                    chunk[idx] = if cfa.channel_at(x as usize, y as usize) == Channel::G {
                        plane[y as usize * width + x as usize]
                    } else if interior(x, y) {
                        let phase = &taps[cfa.phase_at(x as usize, y as usize)];
                        let mut sum = 0.0f32;
                        let mut wsum = 0.0f32;
                        for &(dx, dy, w) in &phase[Channel::G.index()] {
                            sum += w * plane[(y + dy) as usize * width + (x + dx) as usize];
                            wsum += w;
                        }
                        if wsum > 0.0 {
                            sum / wsum
                        } else {
                            0.0
                        }
                    } else {
                        grid_weighted_slow(
                            plane,
                            None,
                            width,
                            height,
                            cfa,
                            radius,
                            x,
                            y,
                            Channel::G,
                            false,
                            set,
                        )
                    };
                }
            }
        });
    }

    // 2) R／B：差分面插值后加回 G；本通道采样位取原值。
    let green_ref = &green;
    let mut out = vec![0.0f32; width * height * 3];
    {
        let chunks: Vec<&mut [f32]> = out.chunks_mut(band_rows * width * 3).collect();
        run_row_bands(chunks, |chunk, band| {
            let y_start = band * band_rows;
            let rows = chunk.len() / (width * 3);
            for ly in 0..rows {
                let y = (y_start + ly) as isize;
                for x in 0..width as isize {
                    let here = cfa.channel_at(x as usize, y as usize);
                    let center = y as usize * width + x as usize;
                    let g = green_ref[center];
                    let mut rgb = [0.0f32; 3];
                    rgb[Channel::G.index()] = g;
                    for target in [Channel::R, Channel::B] {
                        rgb[target.index()] = if here == target {
                            plane[center]
                        } else if interior(x, y) {
                            let phase = &taps[cfa.phase_at(x as usize, y as usize)];
                            let mut sum = 0.0f32;
                            let mut wsum = 0.0f32;
                            for &(dx, dy, w) in &phase[target.index()] {
                                let at = (y + dy) as usize * width + (x + dx) as usize;
                                sum += w * (plane[at] - green_ref[at]);
                                wsum += w;
                            }
                            // 与旧路逐字同形：g + weighted，weighted
                            // 无 tap 时为 0.0（不许写成条件回 g——
                            // -0.0 + 0.0 的位级差异不许引入）。
                            let delta = if wsum > 0.0 { sum / wsum } else { 0.0 };
                            g + delta
                        } else {
                            g + grid_weighted_slow(
                                plane,
                                Some(green_ref),
                                width,
                                height,
                                cfa,
                                radius,
                                x,
                                y,
                                target,
                                true,
                                set,
                            )
                        };
                    }
                    rgb[here.index()] = plane[center];
                    let o = (ly * width + x as usize) * 3;
                    chunk[o..o + 3].copy_from_slice(&rgb);
                }
            }
        });
    }
    out
}

// ---------------------------------------------------------------------------
// ④′ 高光恢复（设计 5.5 保底机制，第三包）：解马赛克之后、色彩矩阵之前，
//    在已白平衡的线性相机 RGB 域作用。纯局部 3×3、零迭代、确定性。
//
//    保底机制只有三件（附录 A14–A16 定版）：单调性下界、μ_ref 下限钳、
//    带色度门控的渐入中性白（desat）。专家原案的 stage-1「3×3 引导比值
//    重建」按公式字面实现在 s=1 处是恒等映射、无恢复效果（实证 W2(a1)
//    解析证明 + 98.5% 逐位相同），**不写成能力**：单通道饱和像素在本包
//    口径下保持硬钳结果，只由单调性下界保证不劣化。真色度恢复（dcraw
//    替换式／RT 传播式）是独立增强项，不在本包。
// ---------------------------------------------------------------------------

/// 解析阈值的饱和系数 τ_clip（设计 5.5 定版）：物理饱和天花板
/// `T_c = τ_clip · g_vig · w_c`，无 clip_mask 侧通道。
pub const HIGHLIGHT_TAU_CLIP: f32 = 0.98;

/// 软过渡起点 τ_0（设计 5.5 定版）：`s_c` 在 `τ_0·T_c` 以下为 0、
/// 在 `T_c` 处为 1，区间宽 6%，严禁硬布尔分支。
pub const HIGHLIGHT_TAU_0: f32 = 0.92;

/// μ_ref 下限钳 ε_floor（设计 5.5「规格必补」，附录 A13）：归一化域内
/// 约 −13.3 EV（相对满量程 1.0），远低于任何真实场景信号，只在窗口
/// 未饱和权重为零（全饱和窗口）时生效——防专家原式在 μ_ref=0 处的
/// 0/0 退化成「裁剪值 2.5 倍」的静默错误（实证 W2(b)）。
pub const HIGHLIGHT_MU_REF_FLOOR: f32 = 1e-4;

/// 色度门控过渡带下界（本包定版，依据见 `highlight_recover` 门控注释）：
/// 相对色度断言量 κ ≤ 本值 ⇒ 门全开（允许 desat）。
pub const HIGHLIGHT_GATE_CHROMA_LO: f32 = 0.48;

/// 色度门控过渡带上界：κ ≥ 本值 ⇒ 门全关（色度可恢复，禁止 desat）。
pub const HIGHLIGHT_GATE_CHROMA_HI: f32 = 0.62;

/// 单通道软饱和权重 `s = clamp((I − τ_0·T) / ((1 − τ_0)·T), 0, 1)`
/// （设计 5.5 定版式）。`ceiling` 是该通道的物理饱和天花板 T_c。
pub fn highlight_softness(value: f32, ceiling: f32) -> f32 {
    ((value - HIGHLIGHT_TAU_0 * ceiling) / ((1.0 - HIGHLIGHT_TAU_0) * ceiling)).clamp(0.0, 1.0)
}

/// 逐像素派生量（④′ 内部）：软饱和权重 s 与其补 u = 1 − s。
fn highlight_terms(value: f32, ceiling: f32) -> (f32, f32) {
    let s = highlight_softness(value, ceiling);
    (s, 1.0 - s)
}

/// ④′ 高光恢复（保底机制）：就地作用于已白平衡的线性相机 RGB 平面。
///
/// - `wb`：重归一化后（min_c w_c = 1.0）的 WB 乘子，阈值
///   `T_c(r) = τ_clip · g_vig(r) · w_c` 的 w 分量。
/// - `vig`：步骤 ② 的暗角增益场（逐像素，与 ② 施加的乘子同一份）；
///   镜头包（第四包）落地前恒传 `None`（全场 1.0）。增益场必须以参数
///   形式预留：② 对饱和像素**照常乘增益、禁止跳过**（跳过会在高光
///   边缘造一圈暗环），阈值同步乘 g_vig 后，饱和像素在恢复眼里仍恰
///   位于其天花板（设计 5.5／附录 A12）。
///
/// 门控定版（附录 A15：触发不许只数饱和通道个数）：先由 3×3 窗口内
/// **未饱和样本的天花板归一化均值** m_c（u 加权；某通道在窗口内无未
/// 饱和样本时取像素自身 Ĩ_c = I_c/T_c，那时该通道已饱和、自身值 ≥
/// 天花板）算相对色度断言量 κ = 1 − min_c m_c / max_c m_c。κ 小 ⇒
/// 存活证据断言的颜色接近中性，观测到的色差主要是差分裁剪伪影 ⇒
/// 许 desat；κ 大 ⇒ 存活通道明确断言强色度、色度走向可由邻域连续
/// 恢复 ⇒ 禁 desat。过渡带 [0.48, 0.62] 由 W2 两组对偶夹具的 κ 实测
/// 分布定版：浅色天空（a2′）κ = 0.442/0.318/0.194（t = 1.8/2.2/2.6）
/// 全在带下，深蓝渐变（a2）κ = 0.648–0.780（t ∈ [2.0, 3.2]）全在带
/// 上，带宽落在两组分布之间的空白区。
///
/// desat 的公共高光亮度 L = max(max_c I_c, μ_ref)：至少是像素自身最
/// 亮通道（保单调精神），且不低于窗口未饱和参考亮度 μ_ref（3×3 内
/// u 加权池化均值，下限钳 ε_floor）。收敛估计再过单调性下界
/// `Î_c = max(I_c, ·)`：任何通道都不许被往低拉（附录 A17 承重）。
pub fn highlight_recover(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    wb: [f32; 3],
    vig: Option<&[f32]>,
) -> Result<()> {
    if rgb.len() != width * height * 3 {
        return Err(RawError::InvalidFrame(format!(
            "高光恢复输入长度 {} 与尺寸 {width}×{height}×3 对不上",
            rgb.len()
        )));
    }
    if wb.iter().any(|&v| !is_positive_finite(v)) {
        return Err(RawError::InvalidFrame(format!(
            "高光恢复的 WB 乘子非法（须正有限）：{wb:?}"
        )));
    }
    if let Some(gain) = vig {
        if gain.len() != width * height || gain.iter().any(|&v| !is_positive_finite(v)) {
            return Err(RawError::InvalidFrame(
                "暗角增益场长度不对或含非正／非有限值".to_string(),
            ));
        }
    }
    if width == 0 || height == 0 {
        return Ok(());
    }
    let ceilings = |x: usize, y: usize| -> [f32; 3] {
        let g = vig.map_or(1.0, |field| field[y * width + x]);
        [
            HIGHLIGHT_TAU_CLIP * g * wb[0],
            HIGHLIGHT_TAU_CLIP * g * wb[1],
            HIGHLIGHT_TAU_CLIP * g * wb[2],
        ]
    };
    // 逐像素字段（8 分量）：[池化未饱和能量, 池化未饱和权重,
    // R/G/B 各自的未饱和权重与未饱和天花板归一化能量]。窗口聚合全部
    // 由这 8 个字段的 3×3 盒和推出（μ_ref 与门控证据同源，不两处统计）。
    let pixel_fields = |rgb: &[f32], x: usize, y: usize| -> [f32; 8] {
        let o = (y * width + x) * 3;
        let t = ceilings(x, y);
        let mut fields = [0.0f32; 8];
        for c in 0..3 {
            let value = rgb[o + c];
            let (_, u) = highlight_terms(value, t[c]);
            fields[0] += u * value;
            fields[1] += u;
            fields[2 + 2 * c] = u;
            fields[3 + 2 * c] = u * (value / t[c]);
        }
        fields
    };
    // 行字段的水平 3 点 reflect-101 盒和。
    let hsum_row = |fields: &[[f32; 8]]| -> Vec<[f32; 8]> {
        let mut out = vec![[0.0f32; 8]; width];
        for (x, cell) in out.iter_mut().enumerate() {
            let xm = reflect_index(x as isize - 1, width);
            let xp = reflect_index(x as isize + 1, width);
            for k in 0..8 {
                cell[k] = fields[xm][k] + fields[x][k] + fields[xp][k];
            }
        }
        out
    };
    // 三行环形缓冲（slot = 源行号 % 3；同一时刻需要的三行行号互不相同
    // 且两两差 ≤ 2，模 3 必不冲突，reflect 重复行是同一行、复用同 slot）。
    // 字段一律在该行被写回之前生成，写回不污染后续行的字段来源。
    let mut slots: [Vec<[f32; 8]>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut slot_row = [usize::MAX; 3];
    let mut raw_fields: Vec<[f32; 8]> = Vec::new();
    for y in 0..height {
        let rows = [
            reflect_index(y as isize - 1, height),
            y,
            reflect_index(y as isize + 1, height),
        ];
        for &src_row in &rows {
            let slot = src_row % 3;
            if slot_row[slot] != src_row {
                raw_fields.clear();
                raw_fields.reserve(width);
                for x in 0..width {
                    raw_fields.push(pixel_fields(rgb, x, src_row));
                }
                slots[slot] = hsum_row(&raw_fields);
                slot_row[slot] = src_row;
            }
        }
        for x in 0..width {
            // 竖直盒和：三行 hsum 相加（reflect 重复行在三行集合里出现
            // 两次，与 reflect-101 的窗口语义一致）。
            let mut win = [0.0f32; 8];
            for &src_row in &rows {
                let slot = &slots[src_row % 3];
                for k in 0..8 {
                    win[k] += slot[x][k];
                }
            }
            let o = (y * width + x) * 3;
            let value = [rgb[o], rgb[o + 1], rgb[o + 2]];
            let t = ceilings(x, y);
            let mut s_sum = 0.0f32;
            for c in 0..3 {
                let (sc, _) = highlight_terms(value[c], t[c]);
                s_sum += sc;
            }
            let alpha_base = (s_sum - 1.0).clamp(0.0, 1.0);
            if alpha_base <= 0.0 {
                continue; // 至多一个通道进软区：desat 不触发，逐位不变。
            }
            // μ_ref：窗口池化未饱和参考亮度（下限钳必补，附录 A13）。
            let mu_ref = if win[1] > 0.0 {
                (win[0] / win[1]).max(HIGHLIGHT_MU_REF_FLOOR)
            } else {
                HIGHLIGHT_MU_REF_FLOOR
            };
            // 门控证据 m_c：窗口未饱和样本的天花板归一化均值；该通道
            // 在窗口内无未饱和样本时取像素自身 Ĩ_c（已饱和，≥ 1）。
            let mut m = [0.0f32; 3];
            for c in 0..3 {
                let weight = win[2 + 2 * c];
                m[c] = if weight > 0.0 {
                    win[3 + 2 * c] / weight
                } else {
                    value[c] / t[c]
                };
            }
            let m_max = m[0].max(m[1]).max(m[2]);
            let m_min = m[0].min(m[1]).min(m[2]);
            let kappa = if m_max > 0.0 {
                1.0 - m_min / m_max
            } else {
                0.0 // 窗口全负（极端暗场）：无色度断言可依，不拦 desat。
            };
            let gate = ((HIGHLIGHT_GATE_CHROMA_HI - kappa)
                / (HIGHLIGHT_GATE_CHROMA_HI - HIGHLIGHT_GATE_CHROMA_LO))
                .clamp(0.0, 1.0);
            let alpha = alpha_base * gate;
            if alpha <= 0.0 {
                continue; // 门控关：色度可恢复，不许脱色，逐位不变。
            }
            let l_hi = value[0].max(value[1]).max(value[2]).max(mu_ref);
            for c in 0..3 {
                let estimate = (1.0 - alpha) * value[c] + alpha * l_hi;
                // 单调性下界（附录 A17）：饱和只会把读数截小，估计把
                // 已饱和通道往低拉一律拒绝。下界在此不可删。
                rgb[o + c] = value[c].max(estimate);
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ④′′ 真色度恢复（第五包）：在 ④′ 保底机制之后、同一相机 RGB 域作用。
//
// 第三包的两个已知视觉缺陷在此治：
// 1. **单通道饱和残粉**：只有一个通道触顶的像素，④′ 按设计不动它
//    （Σs ≤ 1 不触发 desat），被钳住的通道停在天花板、与其余两通道
//    的真实比例脱钩，云带边缘留一圈粉。本步用**邻域未饱和像素的
//    色度比**把该通道估回去。
// 2. **高光区平无层次**：④′ 的 desat 把饱和区推向同一个公共亮度
//    L（止损口径），灰度过渡被抹平。本步的亮度估计带空间变化
//    （锚点亮度经扩散传播进饱和区），恢复出渐变而非一块平白。
//
// 方法（传播式，RT/dcraw 一族的口径在本仓的实现）：
// - **锚点** = 三通道软饱和权重全为 0 的像素（数据完整可信），
//   锚点给出色度比 q_c = I_c / max_c I_c 与锚点亮度 L = max_c I_c。
// - **扩散**：q 与 L 以锚点权重场 W 做固定 12 轮 3×3 盒扩散
//   （加权归一化，reflect 边界，迭代数写死 ⇒ 确定性），把邻域的
//   色度与亮度结构传播进饱和区。W 太小（远离一切锚点，如整帧
//   饱和）⇒ 该像素不动，保留 ④′ 结果——无证据时不编造。
// - **重建**：像素尚有未饱和通道时，亮度由这些通道按 q̂ 反推取
//   最大（与单调性精神一致）；三通道全饱和时才用扩散亮度 L̂。
//   饱和通道的估计 Î_c = L·q̂_c，过单调性下界 max(I_c, ·) 后按
//   软权重 s_c 与原值混合（s=0 的通道逐位不动，s=1 全量采用）。
//
// 诚实边界（立项时已对明）：三通道全饱和处的「细节」是邻域纹理
// 的合理推断，不是还原——原始信息在传感器处已不存在。本步只把
// 推断做得连续、合色度，不声称找回信息。
// ---------------------------------------------------------------------------

/// ④′′ 扩散步长表（写死，确定性纪律）：3×3 盒扩散先密后疏
/// （步长 1,1,2,4,8,16,32 的膨胀采样），总传播半径约 60px——
/// 普通盒扩散 12 轮的有效半径只有 σ≈3px（首版实测：饱和带深处
/// 权重低于可信下限、恢复不生效），膨胀步长以同样的 9 抽头成本
/// 把邻域证据送进大面积高光区。超出此半径的全饱和内陆保留
/// ④′ 结果（见上「诚实边界」）。
pub const CHROMA_DIFFUSION_STRIDES: [usize; 7] = [1, 1, 2, 4, 8, 16, 32];

/// 锚点影响的可信下限：扩散权重 W ≤ 此值视为无证据，像素不动。
pub const CHROMA_MIN_ANCHOR_WEIGHT: f32 = 0.02;

/// ④′′ 真色度恢复：就地作用于已白平衡的线性相机 RGB 平面（④′ 之后）。
/// 入参口径与 `highlight_recover` 相同（wb 为重归一化乘子、vig 为
/// 步骤 ② 的增益场），天花板公式与 ④′ 共用同一份常数，不许两处漂移。
pub fn highlight_chroma_recover(
    rgb: &mut [f32],
    width: usize,
    height: usize,
    wb: [f32; 3],
    vig: Option<&[f32]>,
) -> Result<()> {
    if rgb.len() != width * height * 3 {
        return Err(RawError::InvalidFrame(format!(
            "真色度恢复输入长度 {} 与尺寸 {width}×{height}×3 对不上",
            rgb.len()
        )));
    }
    if wb.iter().any(|&v| !is_positive_finite(v)) {
        return Err(RawError::InvalidFrame(format!(
            "真色度恢复的 WB 乘子非法（须正有限）：{wb:?}"
        )));
    }
    if let Some(gain) = vig {
        if gain.len() != width * height || gain.iter().any(|&v| !is_positive_finite(v)) {
            return Err(RawError::InvalidFrame(
                "暗角增益场长度不对或含非正／非有限值".to_string(),
            ));
        }
    }
    if width == 0 || height == 0 {
        return Ok(());
    }
    let npix = width * height;
    let ceiling_at = |i: usize| -> [f32; 3] {
        let g = vig.map_or(1.0, |field| field[i]);
        [
            HIGHLIGHT_TAU_CLIP * g * wb[0],
            HIGHLIGHT_TAU_CLIP * g * wb[1],
            HIGHLIGHT_TAU_CLIP * g * wb[2],
        ]
    };
    // 软饱和权重（与 ④′ 同源公式）。
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
        return Ok(()); // 无饱和：逐位不变，早退（常态路径零成本之外的保证）。
    }
    // 扩散场：[q_R·W, q_G·W, q_B·W, L·W, W] 五平面，锚点处赋值、
    // 其余为 0；每轮对五平面同做一遍 3×3 盒和平均（加权扩散的
    // Jacobi 迭代，双缓冲防自污染）。
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
        return Ok(()); // 整帧无锚点（全饱和）：无证据，保留 ④′ 结果。
    }
    let mut next = vec![0.0f32; npix * 5];
    for &stride in CHROMA_DIFFUSION_STRIDES.iter() {
        let s = stride as isize;
        for y in 0..height {
            let ym = reflect_index(y as isize - s, height);
            let yp = reflect_index(y as isize + s, height);
            for x in 0..width {
                let xm = reflect_index(x as isize - s, width);
                let xp = reflect_index(x as isize + s, width);
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
    // 重建：只动 s>0 的通道；无扩散证据处不动。
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
        // 亮度估计：未（完全）饱和通道按 q̂ 反推、取最大；全饱和才
        // 用扩散亮度。q̂_c 为 0 的通道不参与反推（无该通道色度证据）。
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
    Ok(())
}

// ---------------------------------------------------------------------------
// ⑤ 畸变 + TCA 复合重采样（第四包，设计 5.9.2 定版口径）：在已白
//    平衡的线性相机 RGB 域、色彩矩阵之前，一次重采样同时完成
//    PTLens 畸变与 TCA（或文件 WarpRectilinear 的逐平面 warp）。
//    采样核为双线性 + reflect-101 边界（与 ④ 的边界口径一致）；
//    自动裁剪 scale 由 `lens::auto_crop_scale` 与导入期落账共用
//    同一个函数现算，两处同值。
// ---------------------------------------------------------------------------

/// 双线性采样单通道（reflect-101 边界）。
fn sample_bilinear(
    plane: &[f32],
    width: usize,
    height: usize,
    channel: usize,
    sx: f32,
    sy: f32,
) -> f32 {
    let x0 = libm::floorf(sx);
    let y0 = libm::floorf(sy);
    let fx = sx - x0;
    let fy = sy - y0;
    let xi = [x0 as isize, x0 as isize + 1];
    let yi = [y0 as isize, y0 as isize + 1];
    let mut acc = 0.0f32;
    for (row, &yy) in yi.iter().enumerate() {
        let ry = reflect_index(yy, height);
        for (col, &xx) in xi.iter().enumerate() {
            let rx = reflect_index(xx, width);
            let weight =
                (if col == 0 { 1.0 - fx } else { fx }) * (if row == 0 { 1.0 - fy } else { fy });
            acc += weight * plane[(ry * width + rx) * 3 + channel];
        }
    }
    acc
}

/// ⑤ 把 warp 模型作用于 RGB 三元组平面（一次重采样，逐通道各自
/// 的源坐标——TCA 的横向色差正来自三通道采样位置不同）。
pub fn warp_rgb(rgb: &[f32], width: usize, height: usize, model: &WarpModel) -> Vec<f32> {
    debug_assert_eq!(rgb.len(), width * height * 3);
    if model.is_identity() {
        return rgb.to_vec();
    }
    let scale = crate::lens::auto_crop_scale(model, width, height);
    let mut out = vec![0.0f32; rgb.len()];
    for y in 0..height {
        for x in 0..width {
            for channel in 0..3 {
                let (sx, sy) = crate::lens::warp_source_coord(
                    model, channel, x as f32, y as f32, width, height, scale,
                );
                out[(y * width + x) * 3 + channel] =
                    sample_bilinear(rgb, width, height, channel, sx, sy);
            }
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
    applied_lens_profile: Option<AppliedLensProfile>,
) -> RawDevelopRecord {
    RawDevelopRecord {
        version: 1,
        source_blob: None,
        source_format: ctx.source_format.clone(),
        camera_make: ctx.camera_make.clone(),
        camera_model: ctx.camera_model.clone(),
        lens: ctx.lens.clone(),
        wb: options.wb,
        highlight_recovery: options.highlight,
        demosaic: options.demosaic,
        pre_exposure_ev: options.pre_exposure_ev,
        baseline_exposure_ev,
        wb_multipliers,
        black_levels: frame.black_levels.clone(),
        white_levels: frame.white_levels.clone(),
        cfa: frame.cfa.name().to_string(),
        orientation_applied: effective_orientation(frame.orientation),
        decoder_name: ctx.decoder_name.clone(),
        decoder_version: ctx.decoder_version.clone(),
        color_alpha,
        lens_exif: ctx.lens_exif.clone(),
        lens_match,
        applied_lens_profile,
        developed_max_edge: options.max_output_edge,
    }
}

/// C1-b 预览档的链尾降采样：面积加权盒式滤波把三元组平面压到
/// 长边 ≤ `max_edge`。新尺寸用整数口径（dim × edge ÷ max_dim，
/// 向下取整、至少 1）⇒ 长边恰为 edge、短边不越界；逐输出像素对
/// 源矩形做面积加权平均，f64 累加且求和次序固定（行→列）⇒
/// 确定性（NF1：只用算术，不用超越函数）。输入长边已 ≤ edge
/// 时原样返回（档位仍由 `assemble_record` 记进 record——记的是
/// 「按预览档展开」这件事实，不只是「缩了」）。
fn downscale_rgb_to_edge(
    rgb: Vec<f32>,
    width: usize,
    height: usize,
    max_edge: u32,
) -> (usize, usize, Vec<f32>) {
    let max_dim = width.max(height);
    if max_dim <= max_edge as usize {
        return (width, height, rgb);
    }
    let new_w = ((width as u64 * u64::from(max_edge)) / max_dim as u64).max(1) as usize;
    let new_h = ((height as u64 * u64::from(max_edge)) / max_dim as u64).max(1) as usize;
    let scale_x = width as f64 / new_w as f64;
    let scale_y = height as f64 / new_h as f64;
    let mut out = vec![0.0f32; new_w * new_h * 3];
    for oy in 0..new_h {
        let y0 = oy as f64 * scale_y;
        let y1 = (oy + 1) as f64 * scale_y;
        let sy_from = y0.floor() as usize;
        let sy_to = (y1.ceil() as usize).min(height);
        for ox in 0..new_w {
            let x0 = ox as f64 * scale_x;
            let x1 = (ox + 1) as f64 * scale_x;
            let sx_from = x0.floor() as usize;
            let sx_to = (x1.ceil() as usize).min(width);
            let mut acc = [0.0f64; 3];
            let mut wsum = 0.0f64;
            for sy in sy_from..sy_to {
                let wy = (y1.min((sy + 1) as f64) - y0.max(sy as f64)).max(0.0);
                if wy == 0.0 {
                    continue;
                }
                for sx in sx_from..sx_to {
                    let wx = (x1.min((sx + 1) as f64) - x0.max(sx as f64)).max(0.0);
                    if wx == 0.0 {
                        continue;
                    }
                    let wgt = wx * wy;
                    let src = (sy * width + sx) * 3;
                    for (c, slot) in acc.iter_mut().enumerate() {
                        *slot += f64::from(rgb[src + c]) * wgt;
                    }
                    wsum += wgt;
                }
            }
            let dst = (oy * new_w + ox) * 3;
            for (c, slot) in out[dst..dst + 3].iter_mut().enumerate() {
                *slot = (acc[c] / wsum) as f32;
            }
        }
    }
    (new_w, new_h, out)
}

// ---------------------------------------------------------------------------
// 主链：① → ② → ③ → ④ → ④′（+④′′）→ ⑤ → ⑥ → ⑦ → ⑧ → ⑨。
// 无镜头 plan 时 ②/⑤ 整步跳过、④′ 的增益场传 None，與前三包逐位
// 一致；④′′ 由 options.highlight 档位控制（缺省 Floor = 不启用）。
// ---------------------------------------------------------------------------

/// 展开一帧（无镜头校正）：设计 5.1 主链，行为与前三包逐位一致。
pub fn develop(
    frame: &RawFrame,
    options: &DevelopOptions,
    ctx: &RecordContext,
) -> Result<DevelopedImage> {
    develop_with_lens(frame, options, ctx, None)
}

/// 一次带镜头校正的展开所需的三件套（计划 + 留痕 + 匹配状态标签）：
/// 三者同源（导入期解析产出，或取图期凭落账记录重建），成组传入，
/// 不许调用方拆开拼——record 里的 profile 必须正是像素上施加的 plan。
pub struct LensExecution<'a> {
    pub plan: &'a LensCorrectionPlan,
    pub profile: &'a AppliedLensProfile,
    /// 五态标签：matched / manual（develop 路径只会出现这两种；
    /// unmatched/db_unavailable/off 是「无 plan」的世界，由导入层落账）。
    pub lens_match: &'a str,
}

/// 展开一帧（可带镜头校正计划）：`lens` 为 None 时与 `develop`
/// 完全同路、逐位一致。
pub fn develop_with_lens(
    frame: &RawFrame,
    options: &DevelopOptions,
    ctx: &RecordContext,
    lens: Option<LensExecution<'_>>,
) -> Result<DevelopedImage> {
    let lens_plan = lens.as_ref().map(|exec| exec.plan);
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
    // ② 暗角增益（第四包）：马赛克域逐像素乘增益场——对饱和像素
    // **照常乘、禁止跳过**（跳过会在高光边缘造一圈暗环，设计 5.5）；
    // 同一份场随后喂 ④′/④′′ 的天花板（增益场接线义务，第三包留、
    // 第四包履约）。增益场只算一次，两处共用同一份 Vec。
    let vig_field: Option<Vec<f32>> = lens_plan.and_then(|plan| {
        plan.vignette
            .as_ref()
            .map(|model| crate::lens::vignette_gain_field(model, frame.width, frame.height))
    });
    if let Some(field) = &vig_field {
        for (value, gain) in plane.iter_mut().zip(field.iter()) {
            *value *= gain;
        }
    }
    // ③ WB 前置：重归一化后逐像素乘在马赛克域（MHC 的标定前提，专家裁决 Q1）。
    let multipliers = renormalized_wb_multipliers(frame)?;
    apply_wb(&mut plane, frame, multipliers);
    // ④ 解马赛克（纯整数网格 MHC／双线性基线）。
    // C3-d（期 2）：预览档 + 网格 CFA ⇒ 简化 tap 快速路（预览像素
    // 本就记档、勿当成品）；Bayer 路与全档网格路口径不变。
    let quick_grid = options.max_output_edge.is_some() && frame.cfa.bayer().is_none();
    if quick_grid {
        notes.push(
            "预览档快速解马赛克（C3-d 简化 tap 集）：预览像素与全档成品不同源，勿当成品用"
                .to_string(),
        );
    }
    let mut rgb_camera = if quick_grid {
        demosaic_grid_quick(&plane, frame.width, frame.height, &frame.cfa)
    } else {
        demosaic(
            &plane,
            frame.width,
            frame.height,
            frame.cfa.clone(),
            options.demosaic,
        )
    };
    // C1-a（期 2）：马赛克平面在此之后再无用处，显式释放——不然它的
    // 4 B/px 会陪 ⑥／⑧ 的大缓冲一路活到函数尾，白抬峰值。
    drop(plane);
    // ④′ 高光恢复（保底机制，第三包）：已白平衡相机 RGB 域、无条件
    // 环节（不进 raw_options）。天花板里的 g_vig 即 ② 的增益场。
    highlight_recover(
        &mut rgb_camera,
        frame.width,
        frame.height,
        multipliers,
        vig_field.as_deref(),
    )?;
    // ④′′ 真色度恢复（第五包）：档位 Chroma 时在保底之后追加；
    // Floor 档整步跳过（第三包行为逐位不变）。
    if options.highlight == HighlightMode::Chroma {
        highlight_chroma_recover(
            &mut rgb_camera,
            frame.width,
            frame.height,
            multipliers,
            vig_field.as_deref(),
        )?;
    }
    // C1-a（期 2）：暗角增益场的最后一位消费者是 ④′′，此处释放
    // （理由同上，4 B/px）。
    drop(vig_field);
    // ⑤ 畸变 + TCA 复合重采样（第四包）：一次重采样、逐通道源坐标；
    // 排在色彩矩阵之前（相机 RGB 域的几何才是镜头几何）。
    let rgb_camera = match lens_plan.and_then(|plan| plan.warp.as_ref()) {
        Some(model) => warp_rgb(&rgb_camera, frame.width, frame.height, model),
        None => rgb_camera,
    };
    // ⑥ 复合色彩链：已白平衡相机 RGB → 线性 sRGB D65。
    let neutral = as_shot_neutral(frame)?;
    let composite = composite_color_matrix(&frame.color_matrices, neutral, &mut notes)?;
    let m_total = composite.matrix;
    let alpha = composite.alpha;
    // C1-a（期 2）：色彩矩阵是逐像素独立运算 ⇒ 原地变换（三元先读进
    // 局部量再写回同一槽位），省掉与 rgb_camera 并存的第二份
    // 12 B/px 缓冲。每像素的算式与旧双缓冲形态逐项同式同序 ⇒
    // 逐位守恒（tests/buffer_reuse_golden.rs 的金值守这条）。
    let mut rgb = rgb_camera;
    for pixel in rgb.chunks_exact_mut(3) {
        let out = mat3_mul_vec(&m_total, [pixel[0], pixel[1], pixel[2]]);
        pixel.copy_from_slice(&out);
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
    // C1-a（期 2）：恒等方向不再整幅拷一份（`apply_orientation_rgb`
    // 对 1 档本就是逐值复制）；转置／翻转类保留一份输出拷贝——设计
    // C1-a 的低风险支（⑧ 前上游缓冲已全部释放，峰值只剩输入＋输出
    // 两份 12 B/px），不做原地重排的高风险索引手术。
    let (out_w, out_h, rgb) = if orientation == 1 {
        (frame.width, frame.height, rgb)
    } else {
        apply_orientation_rgb(&rgb, frame.width, frame.height, orientation)
    };
    // C1-b（期 2）：预览档在链尾降采样（⑧ 之后、⑨ 之前——把
    // 「全档／预览」的分界钉在最后一步，record 的
    // `developed_max_edge` 与这里的实际输出同源）。全档（None）
    // 整步跳过、逐位不变；预览档在注记里点名，不许冒充成品。
    let (out_w, out_h, mut rgb) = match options.max_output_edge {
        Some(edge) => {
            let shrunk = downscale_rgb_to_edge(rgb, out_w, out_h, edge);
            notes.push(format!(
                "预览档展开（max_output_edge={edge}）：输出 {}×{}，预览像素与全档成品不同源，勿当成品用",
                shrunk.0, shrunk.1
            ));
            shrunk
        }
        None => (out_w, out_h, rgb),
    };
    // ⑨ 出口截零（全程唯一截零点）。
    clamp_exit(&mut rgb)?;
    let (lens_match, applied) = match &lens {
        Some(exec) => (
            Some(exec.lens_match.to_string()),
            Some(exec.profile.clone()),
        ),
        None => (None, None),
    };
    let record = assemble_record(
        frame,
        options,
        ctx,
        multipliers,
        baseline,
        alpha,
        lens_match,
        applied,
    );
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

    // -----------------------------------------------------------------
    // C3 判据（期 2）：相位抽表 + 行带并行与旧实现逐位一致。
    // 参考实现 = 期 2 改造前的 demosaic_grid 原样拷贝（每像素三趟
    // 全窗扫描 + 闭包现算），只许测试引用、不许生产路径调用。
    // -----------------------------------------------------------------

    fn xtrans_grid_for_test() -> CfaGrid {
        let pattern = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG";
        let cells: Vec<Channel> = pattern
            .chars()
            .map(|c| match c {
                'R' => Channel::R,
                'G' => Channel::G,
                'B' => Channel::B,
                other => panic!("pattern 含未知字符 {other}"),
            })
            .collect();
        CfaGrid::new(6, cells, pattern.to_string()).expect("6×6 网格必须可构造")
    }

    fn random_plane(w: usize, h: usize, seed: u32) -> Vec<f32> {
        // xorshift32 确定性伪随机（不引依赖、跨平台同序列）。
        let mut state = seed.max(1);
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state % 10_000) as f32 / 10_000.0
        };
        (0..w * h).map(|_| next()).collect()
    }

    fn demosaic_grid_reference(
        plane: &[f32],
        width: usize,
        height: usize,
        cfa: &CfaGrid,
    ) -> Vec<f32> {
        let radius = cfa.period().div_ceil(2) as isize;
        let sample = |x: isize, y: isize| -> f32 {
            plane[reflect_index(y, height) * width + reflect_index(x, width)]
        };
        let chan = |x: isize, y: isize| -> Channel {
            cfa.channel_at(reflect_index(x, width), reflect_index(y, height))
        };
        let weighted =
            |x: isize, y: isize, want: Channel, value: &dyn Fn(isize, isize) -> f32| -> f32 {
                let mut sum = 0.0f32;
                let mut wsum = 0.0f32;
                for dy in -radius..=radius {
                    for dx in -radius..=radius {
                        if chan(x + dx, y + dy) == want {
                            let w = 1.0 / (1.0 + (dx * dx + dy * dy) as f32);
                            sum += w * value(x + dx, y + dy);
                            wsum += w;
                        }
                    }
                }
                if wsum > 0.0 {
                    sum / wsum
                } else {
                    0.0
                }
            };
        let mut green = vec![0.0f32; width * height];
        for y in 0..height as isize {
            for x in 0..width as isize {
                green[y as usize * width + x as usize] = if chan(x, y) == Channel::G {
                    sample(x, y)
                } else {
                    weighted(x, y, Channel::G, &|sx, sy| sample(sx, sy))
                };
            }
        }
        let g_at = |x: isize, y: isize| -> f32 {
            green[reflect_index(y, height) * width + reflect_index(x, width)]
        };
        let mut out = vec![0.0f32; width * height * 3];
        for y in 0..height as isize {
            for x in 0..width as isize {
                let here = chan(x, y);
                let g = g_at(x, y);
                let mut rgb = [0.0f32; 3];
                rgb[Channel::G.index()] = g;
                for target in [Channel::R, Channel::B] {
                    rgb[target.index()] = if here == target {
                        sample(x, y)
                    } else {
                        g + weighted(x, y, target, &|sx, sy| sample(sx, sy) - g_at(sx, sy))
                    };
                }
                rgb[here.index()] = sample(x, y);
                let o = (y as usize * width + x as usize) * 3;
                out[o..o + 3].copy_from_slice(&rgb);
            }
        }
        out
    }

    #[test]
    fn demosaic_grid_phase_table_bit_identical() {
        let grid = xtrans_grid_for_test();
        // 尺寸覆盖：全边界小帧（无 interior）、窄带帧、多 interior 帧。
        for (w, h, seed) in [
            (7usize, 5usize, 11u32),
            (13, 11, 22),
            (36, 24, 33),
            (61, 47, 44),
        ] {
            let plane = random_plane(w, h, seed);
            let reference = demosaic_grid_reference(&plane, w, h, &grid);
            let tabled = demosaic_grid_impl(&plane, w, h, &grid, GridTapSet::Full, 1);
            assert_eq!(
                tabled, reference,
                "相位表路径与旧实现必须逐位一致（{w}×{h}）"
            );
            // 公开入口（自动带数）同样逐位一致。
            let via_entry = demosaic_grid(&plane, w, h, &grid);
            assert_eq!(via_entry, reference, "公开入口必须逐位一致（{w}×{h}）");
        }
    }

    #[test]
    fn demosaic_grid_parallel_bit_identical() {
        let grid = xtrans_grid_for_test();
        let (w, h) = (120usize, 90usize);
        let plane = random_plane(w, h, 77);
        let reference = demosaic_grid_reference(&plane, w, h, &grid);
        for bands in [1usize, 2, 4, 7] {
            let out = demosaic_grid_impl(&plane, w, h, &grid, GridTapSet::Full, bands);
            assert_eq!(out, reference, "带数 {bands} 必须与标量旧实现逐位一致");
        }
    }

    #[test]
    fn demosaic_grid_quick_keeps_constant_field_and_differs_on_texture() {
        let grid = xtrans_grid_for_test();
        // 恒色场：简化 tap 集平均的仍是同一常数 ⇒ 逐像素精确。
        let (w, h) = (24usize, 18usize);
        let mut plane = vec![0.0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                plane[y * w + x] = match grid.channel_at(x, y) {
                    Channel::R => 0.8,
                    Channel::G => 0.5,
                    Channel::B => 0.3,
                };
            }
        }
        let quick = demosaic_grid_impl(&plane, w, h, &grid, GridTapSet::Quick, 2);
        for pixel in quick.chunks_exact(3) {
            // 与既有 xtrans 恒色场判据同口径（1e-6）：加权平均对
            // 常数场只保证舍入级精确（差分面 s−g 的浮点抵消），
            // 全档旧路亦然——这里断的是「快速档不系统性改色」。
            assert!(
                (pixel[0] - 0.8).abs() < 1e-6
                    && (pixel[1] - 0.5).abs() < 1e-6
                    && (pixel[2] - 0.3).abs() < 1e-6,
                "快速档恒色场应逐像素 ≈ [0.8, 0.5, 0.3]，实得 {pixel:?}"
            );
        }
        // 随机纹理：快速档必须真的少用 tap（与全档有别）——
        // 若过滤失效（快速档退化为全档），这里会逐位相等而红。
        let plane = random_plane(48, 36, 99);
        let full = demosaic_grid_impl(&plane, 48, 36, &grid, GridTapSet::Full, 1);
        let quick = demosaic_grid_impl(&plane, 48, 36, &grid, GridTapSet::Quick, 1);
        assert_ne!(quick, full, "快速档在纹理上应与全档不同（过滤未生效？）");
    }
}
