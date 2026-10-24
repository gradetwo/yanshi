//! **图像统计层**（期 4 D3 前置，纯函数，无 IO）：对 RGBA8 代理图算
//! `auto_retouch` 估计器所需的全部统计量。
//!
//! 口径总则（设计 §5 D3）：除曝光与 WB（线性域）外，统计在显示空间
//! （RGBA8 渲染输出）上算；线性口径参数按 §9.1 的 EOTF 换算转过去，
//! 换算复用 `color` 模块的扩展传输对，不另写。
//!
//! 两个亮度定义，各自有主，不得混用：
//! - 显示亮度：整数 Rec.601 `(299·r + 587·g + 114·b + 500) / 1000`
//!   （与 `histogram` 模块同一公式），用于显示百分位与 clip 比；
//! - 线性亮度：解码后 Rec.709（0.2126/0.7152/0.0722），用于曝光估值。
//!
//! 百分位口径：显示侧用 256 桶下分位（与 `histogram::percentile`
//! 同定义：最小的、累计 ≥ ⌈p/100·N⌉ 的桶）；线性侧对排序样本用
//! 同一下分位（索引 ⌈p/100·N⌉−1）。`p` 为小数时（如 p0.1/p99.9）
//! 同样按 ⌈p/100·N⌉ 取秩。
//!
//! alpha 口径：alpha == 0 的像素不参与任何统计（无颜色信息，计入
//! 只会把黑色当内容）；半透明像素按直出 RGB 原样参与（渲染输出
//! 的 straight 值，见调用方）。

use crate::color::srgb_decode_extended;

/// 白平衡低百分位（期 4 D3 标定常数，设计带 [2, 3] 内取中值 2.5；
/// 回测中以三档（2.0/2.5/3.0）的灵敏度报告复核）。
pub const WB_LOW_PERCENTILE: f32 = 2.5;

/// 噪声估计的校准系数 κ（期 4 D3 标定常数）。
///
/// 取 1.0，不取专家给的 ≈1.25——实测钉死：独立高斯噪声合成图上
/// κ=1.25 恰好高估 25%（σ 0.03 测得 0.0377 ≈ 1.25×0.03），而本
/// 构造按推导对独立噪声无偏（MAD×1.4826/√6 还原二阶差分的 σ，
/// 再乘局部 Anscombe 斜率中位数还原亮度域），κ=1.0 时合成图
/// 读数即真值。stride-3 已越过 demosaic 核支撑（≤ ±2 像素），
/// 故不另给相关性修正；真实照片上残余的弱相关若使读数偏低，
/// 由去噪 ×6 系数的回测标定吸收，不把偏差藏进 κ（若复核方
/// 的 1.25 来自另一差分构造，请以该构造的推导为准，见 gradetwo 说明）。
pub const NOISE_KAPPA: f32 = 1.0;

/// 一幅 RGBA8 图的统计量（期 4 D3，供 `auto_retouch` 估计器用）。
#[derive(Debug, Clone)]
pub struct ImageStats {
    /// 统计图宽（代理图）。
    pub width: u32,
    /// 统计图高（代理图）。
    pub height: u32,
    /// 参与统计的像素数（alpha > 0 者）。
    pub count: usize,
    /// 显示亮度 p0.1（0–255 字节标度）。
    pub luma_p01: u8,
    /// 显示亮度 p1（0–255 字节标度）。
    pub luma_p1: u8,
    /// 显示亮度 p5（0–255 字节标度，逆光场景分类用）。
    pub luma_p5: u8,
    /// 显示亮度 p25（0–255 字节标度）。
    pub luma_p25: u8,
    /// 显示亮度 p50（0–255 字节标度）。
    pub luma_p50: u8,
    /// 显示亮度 p75（0–255 字节标度）。
    pub luma_p75: u8,
    /// 显示亮度 p95（0–255 字节标度，逆光场景分类用）。
    pub luma_p95: u8,
    /// 显示亮度 p99（0–255 字节标度）。
    pub luma_p99: u8,
    /// 显示亮度 p99.9（0–255 字节标度）。
    pub luma_p999: u8,
    /// 显示亮度 ≤ 2 的像素比（暗部 clip）。
    pub clip_low_ratio: f32,
    /// 显示亮度 ≥ 253 的像素比（亮部 clip）。
    pub clip_high_ratio: f32,
    /// 分通道 ≥ 253 的像素比（[R, G, B]）。
    pub channel_clip_high: [f32; 3],
    /// 线性亮度 p0.2（0–1 线性标度；曝光缩放与 EOTF 均为单调变换，
    /// 故曝光后显示黑点 = encode(2^ev × 本值），色阶估计直接用它，
    /// 不必重排样本）。
    pub linear_p02: f32,
    /// 线性亮度 p5（0–1 线性标度）。
    pub linear_p5: f32,
    /// 线性亮度 p25（0–1 线性标度；曝光后显示 p25 同上理，影调用）。
    pub linear_p25: f32,
    /// 线性亮度 p50（0–1 线性标度，曝光估值用）。
    pub linear_p50: f32,
    /// 线性亮度 p95（0–1 线性标度）。
    pub linear_p95: f32,
    /// 线性亮度 p99.5（0–1 线性标度，曝光高光护栏用）。
    pub linear_p995: f32,
    /// 线性亮度 p99.8（0–1 线性标度；曝光后显示白点同黑点理）。
    pub linear_p998: f32,
    /// 平均饱和度 S̄（HSV，0–1）。
    pub mean_saturation: f32,
    /// 饱和度 p90（HSV，0–1）。
    pub saturation_p90: f32,
    /// 36 桶色相直方图（每桶 10°，S·V 加权，归一化和为 1；权重
    /// 全零时全零）。
    pub hue_histogram: [f32; 36],
    /// 肤色代理占比（色相 [8°, 50°]、S [0.15, 0.75]、V [0.15, 0.95]；
    /// 命名与文档都写明是**代理量**，不是肤色检测）。
    pub skin_proxy_ratio: f32,
    /// 白平衡候选像素数（线性域 max ≤ 0.75 掩码后）。
    pub wb_candidate_count: usize,
    /// 白平衡候选像素占比。
    pub wb_candidate_ratio: f32,
    /// 白平衡候选线性通道的低百分位（[R, G, B]，p = 2.0/2.5/3.0
    /// 三档，估计器默认用 p2.5 档）。
    pub wb_channel_p20: [f32; 3],
    /// 白平衡候选线性通道 p2.5（见 `wb_channel_p20`）。
    pub wb_channel_p25: [f32; 3],
    /// 白平衡候选线性通道 p3.0（见 `wb_channel_p20`）。
    pub wb_channel_p30: [f32; 3],
    /// 白平衡 log-chromaticity：候选点到 Planck 轴直线近似
    /// （过 D65 白点、方向 (−1, +1)/√2，见模块内说明）的距离中位数。
    pub wb_planck_distance_median: f32,
    /// 白平衡 log-chromaticity：第一主成分上的 Sarle 双峰系数
    /// （> 0.555 提示双峰 ⇒ 置信低）。
    pub wb_sarle_bc: f32,
    /// 白平衡 log-chromaticity：候选点中 Planck 距离 < 0.12 的
    /// 占比（专家原文未展开 ρ 的确切定义，此为本实现的操作定义，
    /// 待复核；< 1.5% ⇒ 置信低）。
    pub wb_rho: f32,
    /// 暗通道均值（显示空间 min(R, G, B)/255 的均值，与
    /// `estimate_atmospheric_light` 的 dark_mean 同定义，去雾用；
    /// 不透明图上两者相等，透明像素口径不同：本统计跳过 alpha == 0，
    /// 而 `estimate_atmospheric_light` 按 RGB 计入）。
    pub dark_mean: f32,
    /// 微对比量 E：显示亮度（0–1）的平均 |Laplacian|（代理图，
    /// clarity 估计用；边界 1 像素不参与）。
    pub micro_contrast: f32,
    /// 高频比：半径 1 高通能量 / 半径 4 高通能量（盒式模糊近似，
    /// texture 估计用；分母为零时为 0）。
    pub hf_ratio: f32,
}

impl ImageStats {
    /// 全零统计（空图时返回，调用方须先判 count 再用估计器）。
    fn zero(width: u32, height: u32) -> Self {
        ImageStats {
            width,
            height,
            count: 0,
            luma_p01: 0,
            luma_p1: 0,
            luma_p5: 0,
            luma_p25: 0,
            luma_p50: 0,
            luma_p75: 0,
            luma_p95: 0,
            luma_p99: 0,
            luma_p999: 0,
            clip_low_ratio: 0.0,
            clip_high_ratio: 0.0,
            channel_clip_high: [0.0; 3],
            linear_p02: 0.0,
            linear_p5: 0.0,
            linear_p25: 0.0,
            linear_p50: 0.0,
            linear_p95: 0.0,
            linear_p995: 0.0,
            linear_p998: 0.0,
            mean_saturation: 0.0,
            saturation_p90: 0.0,
            hue_histogram: [0.0; 36],
            skin_proxy_ratio: 0.0,
            wb_candidate_count: 0,
            wb_candidate_ratio: 0.0,
            wb_channel_p20: [0.0; 3],
            wb_channel_p25: [0.0; 3],
            wb_channel_p30: [0.0; 3],
            wb_planck_distance_median: 0.0,
            wb_sarle_bc: 0.0,
            wb_rho: 0.0,
            dark_mean: 0.0,
            micro_contrast: 0.0,
            hf_ratio: 0.0,
        }
    }
}

/// 由 256 桶求第 `p`（0 < p ≤ 100，可小数）百分位：下分位口径，
/// 秩 = ⌈p/100·N⌉（至少 1），返回最小的累计 ≥ 秩的桶值。
fn bucket_percentile(buckets: &[u64; 256], count: u64, p: f64) -> u8 {
    if count == 0 || p.is_nan() || p <= 0.0 || p > 100.0 {
        return 0;
    }
    let rank = ((p / 100.0) * count as f64).ceil().max(1.0) as u64;
    let mut cumulative = 0u64;
    for (value, bucket) in buckets.iter().enumerate() {
        cumulative += bucket;
        if cumulative >= rank {
            return value as u8;
        }
    }
    255
}

/// 由排序样本求第 `p`（0 < p ≤ 100）百分位：下分位口径，
/// 索引 = ⌈p/100·N⌉ − 1（钳到 [0, N−1]）。
fn sorted_percentile(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() || p.is_nan() || p <= 0.0 || p > 100.0 {
        return 0.0;
    }
    let rank = ((p / 100.0) * sorted.len() as f32).ceil().max(1.0) as usize;
    sorted[(rank - 1).min(sorted.len() - 1)]
}

/// 显示亮度（整数 Rec.601，与 `histogram` 模块同公式）。
fn display_luma_byte(r: u8, g: u8, b: u8) -> u8 {
    ((299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) + 500) / 1000) as u8
}

/// 显示亮度（0–1 浮点，Rec.601 权重，与字节公式同权重不同舍入）。
fn display_luma_f32(r: f32, g: f32, b: f32) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// 标准 HSV（显示空间，r/g/b 为 0–1）：返回 (hue_deg, s, v)。
fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let s = if max <= 0.0 { 0.0 } else { chroma / max };
    if chroma <= 0.0 {
        return (0.0, s, max);
    }
    let hue = if max == r {
        60.0 * ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / chroma + 2.0)
    } else {
        60.0 * ((r - g) / chroma + 4.0)
    };
    (hue, s, max)
}

/// 对 RGBA8 像素（行优先、每像素 4 字节）算 `ImageStats`。
/// 输入通常是长边 ≤ 1024 的代理图；不足 4 字节的尾巴不计。
pub fn image_stats_rgba8(pixels: &[u8], width: u32, height: u32) -> ImageStats {
    let mut stats = ImageStats::zero(width, height);
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return stats;
    }
    // 第一遍：逐像素累积（亮度桶、通道 clip、线性样本、HSV、暗通道）。
    let mut luma_buckets = [0u64; 256];
    let mut channel_buckets = [[0u64; 256]; 3];
    let mut linear_luma: Vec<f32> = Vec::new();
    let mut wb_channels: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut saturations: Vec<f32> = Vec::new();
    let mut hue_weighted = [0.0f32; 36];
    let mut hue_weight_total = 0.0f32;
    let mut skin_count = 0u64;
    let mut dark_sum = 0.0f32;
    let mut count = 0usize;
    for pixel in pixels.chunks_exact(4) {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 {
            continue;
        }
        count += 1;
        let luma_byte = display_luma_byte(r, g, b);
        luma_buckets[luma_byte as usize] += 1;
        channel_buckets[0][r as usize] += 1;
        channel_buckets[1][g as usize] += 1;
        channel_buckets[2][b as usize] += 1;
        let lin = [
            srgb_decode_extended(r as f32 / 255.0),
            srgb_decode_extended(g as f32 / 255.0),
            srgb_decode_extended(b as f32 / 255.0),
        ];
        linear_luma.push(0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]);
        // 白平衡候选掩码（线性域）：任一通道 > 0.75 者剔除。
        if lin[0] <= 0.75 && lin[1] <= 0.75 && lin[2] <= 0.75 {
            for channel in 0..3 {
                wb_channels[channel].push(lin[channel]);
            }
        }
        let (hue, s, v) = rgb_to_hsv(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        saturations.push(s);
        let weight = s * v;
        hue_weighted[(hue / 10.0).floor() as usize % 36] += weight;
        hue_weight_total += weight;
        if (8.0..=50.0).contains(&hue) && (0.15..=0.75).contains(&s) && (0.15..=0.95).contains(&v) {
            skin_count += 1;
        }
        dark_sum += (r.min(g).min(b)) as f32 / 255.0;
    }
    if count == 0 {
        return stats;
    }
    stats.count = count;
    let n = count as u64;
    let nf = count as f32;
    stats.luma_p01 = bucket_percentile(&luma_buckets, n, 0.1);
    stats.luma_p1 = bucket_percentile(&luma_buckets, n, 1.0);
    stats.luma_p5 = bucket_percentile(&luma_buckets, n, 5.0);
    stats.luma_p25 = bucket_percentile(&luma_buckets, n, 25.0);
    stats.luma_p50 = bucket_percentile(&luma_buckets, n, 50.0);
    stats.luma_p75 = bucket_percentile(&luma_buckets, n, 75.0);
    stats.luma_p95 = bucket_percentile(&luma_buckets, n, 95.0);
    stats.luma_p99 = bucket_percentile(&luma_buckets, n, 99.0);
    stats.luma_p999 = bucket_percentile(&luma_buckets, n, 99.9);
    stats.clip_low_ratio = (luma_buckets[0] + luma_buckets[1] + luma_buckets[2]) as f32 / nf;
    stats.clip_high_ratio = (luma_buckets[253] + luma_buckets[254] + luma_buckets[255]) as f32 / nf;
    for (clip, buckets) in stats
        .channel_clip_high
        .iter_mut()
        .zip(channel_buckets.iter())
    {
        *clip = (buckets[253] + buckets[254] + buckets[255]) as f32 / nf;
    }
    linear_luma.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    stats.linear_p02 = sorted_percentile(&linear_luma, 0.2);
    stats.linear_p5 = sorted_percentile(&linear_luma, 5.0);
    stats.linear_p25 = sorted_percentile(&linear_luma, 25.0);
    stats.linear_p50 = sorted_percentile(&linear_luma, 50.0);
    stats.linear_p95 = sorted_percentile(&linear_luma, 95.0);
    stats.linear_p995 = sorted_percentile(&linear_luma, 99.5);
    stats.linear_p998 = sorted_percentile(&linear_luma, 99.8);
    stats.mean_saturation = saturations.iter().sum::<f32>() / nf;
    saturations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    stats.saturation_p90 = sorted_percentile(&saturations, 90.0);
    if hue_weight_total > 0.0 {
        for (hist, weight) in stats.hue_histogram.iter_mut().zip(hue_weighted.iter()) {
            *hist = *weight / hue_weight_total;
        }
    }
    stats.skin_proxy_ratio = skin_count as f32 / nf;
    stats.dark_mean = dark_sum / nf;
    let wb_count = wb_channels[0].len();
    stats.wb_candidate_count = wb_count;
    stats.wb_candidate_ratio = wb_count as f32 / nf;
    if wb_count > 0 {
        let mut sorted_channels: [Vec<f32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
        for channel in 0..3 {
            sorted_channels[channel] = wb_channels[channel].clone();
            sorted_channels[channel]
                .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            stats.wb_channel_p20[channel] = sorted_percentile(&sorted_channels[channel], 2.0);
            stats.wb_channel_p25[channel] = sorted_percentile(&sorted_channels[channel], 2.5);
            stats.wb_channel_p30[channel] = sorted_percentile(&sorted_channels[channel], 3.0);
        }
        let (planck_median, sarle_bc, rho) = log_chromaticity_confidence(&wb_channels);
        stats.wb_planck_distance_median = planck_median;
        stats.wb_sarle_bc = sarle_bc;
        stats.wb_rho = rho;
    }
    // 第二遍：二维量（微对比、高频比）需要行邻域，单独走。
    stats.micro_contrast = micro_contrast(pixels, w, h);
    stats.hf_ratio = hf_ratio(pixels, w, h);
    stats
}

/// log-chromaticity 置信三量（输入为白平衡候选线性通道样本）。
///
/// 坐标：u = ln(R/G)、v = ln(B/G)（任一通道 < 0.005 者跳过该点，
/// 防对数爆炸）。Planck 轴取过 D65 白点（u = v = 0）、方向
/// (−1, +1)/√2 的直线近似：黑体色温升高时 R/G 降、B/G 升，
/// 故轨迹方向确为 (−1, +1)；精确 Planck 轨迹需 CIE 1931 表，
/// 本直线为一阶近似，阈值 0.12 的行为以回测复核。
/// 点到轴距离 = |u + v|/√2；Sarle BC 取第一主成分投影的
/// 双峰系数；ρ = 距离 < 0.12 的点占比（操作定义，待复核）。
fn log_chromaticity_confidence(wb_channels: &[Vec<f32>; 3]) -> (f32, f32, f32) {
    let mut points: Vec<(f32, f32)> = Vec::new();
    // 三通道样本等长（同一掩码下同步 push），zip 到最短即全量。
    for ((r, g), b) in wb_channels[0]
        .iter()
        .zip(wb_channels[1].iter())
        .zip(wb_channels[2].iter())
    {
        let (r, g, b) = (*r, *g, *b);
        if r < 0.005 || g < 0.005 || b < 0.005 {
            continue;
        }
        points.push(((r / g).ln(), (b / g).ln()));
    }
    if points.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let mut distances: Vec<f32> = points
        .iter()
        .map(|(u, v)| (u + v).abs() / std::f32::consts::SQRT_2)
        .collect();
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted_percentile(&distances, 50.0);
    let rho = distances.iter().filter(|d| **d < 0.12).count() as f32 / distances.len() as f32;
    // 第一主成分：2×2 协方差的最大特征向量方向。
    let count = points.len() as f32;
    let mean_u = points.iter().map(|p| p.0).sum::<f32>() / count;
    let mean_v = points.iter().map(|p| p.1).sum::<f32>() / count;
    let mut cxx = 0.0f32;
    let mut cyy = 0.0f32;
    let mut cxy = 0.0f32;
    for (u, v) in &points {
        let du = u - mean_u;
        let dv = v - mean_v;
        cxx += du * du;
        cyy += dv * dv;
        cxy += du * dv;
    }
    cxx /= count;
    cyy /= count;
    cxy /= count;
    let theta = 0.5 * (2.0 * cxy).atan2(cxx - cyy);
    let (dx, dy) = (theta.cos(), theta.sin());
    let projected: Vec<f32> = points
        .iter()
        .map(|(u, v)| (u - mean_u) * dx + (v - mean_v) * dy)
        .collect();
    let sarle_bc = sarle_bimodality(&projected);
    (median, sarle_bc, rho)
}

/// Sarle 双峰系数：BC = (skew² + 1) / (kurt + 3(n−1)²/((n−2)(n−3)))，
/// 其中 kurt 为超值峰度（减 3 后）。n < 10 时返回 0（样本不足，
/// 不判双峰）。BC > 0.555 提示双峰。
fn sarle_bimodality(values: &[f32]) -> f32 {
    let n = values.len();
    if n < 10 {
        return 0.0;
    }
    let nf = n as f32;
    let mean = values.iter().sum::<f32>() / nf;
    let mut m2 = 0.0f32;
    let mut m3 = 0.0f32;
    let mut m4 = 0.0f32;
    for value in values {
        let d = value - mean;
        let d2 = d * d;
        m2 += d2;
        m3 += d2 * d;
        m4 += d2 * d2;
    }
    m2 /= nf;
    m3 /= nf;
    m4 /= nf;
    if m2 <= 0.0 {
        return 0.0;
    }
    let skew = m3 / m2.powf(1.5);
    let kurt = m4 / (m2 * m2) - 3.0;
    let correction = 3.0 * (nf - 1.0).powi(2) / ((nf - 2.0) * (nf - 3.0));
    (skew * skew + 1.0) / (kurt + correction)
}

/// 微对比量 E：显示亮度（0–1）的平均 |Laplacian|，核
/// [0 1 0; 1 −4 1; 0 1 0]，边界 1 像素不参与；alpha == 0 者按 0 计
/// （透明区无亮度，与统计口径一致：透明像素不参与第一遍，
/// 但二维量按画布几何走，透明区亮度 0 会抬 E——代理图为照片层
/// 时不透明，此分支不触发）。
fn micro_contrast(pixels: &[u8], width: usize, height: usize) -> f32 {
    if width < 3 || height < 3 {
        return 0.0;
    }
    let luma = |x: usize, y: usize| -> f32 {
        let base = (y * width + x) * 4;
        if base + 3 >= pixels.len() || pixels[base + 3] == 0 {
            return 0.0;
        }
        display_luma_f32(
            pixels[base] as f32 / 255.0,
            pixels[base + 1] as f32 / 255.0,
            pixels[base + 2] as f32 / 255.0,
        )
    };
    let mut sum = 0.0f32;
    let mut count = 0usize;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let lap = luma(x, y - 1) + luma(x - 1, y) + luma(x + 1, y) + luma(x, y + 1)
                - 4.0 * luma(x, y);
            sum += lap.abs();
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        sum / count as f32
    }
}

/// 高频比：半径 1 高通能量 / 半径 4 高通能量。盒式模糊近似：
/// hp1 = luma − 3×3 均值，hp4 = luma − 9×9 均值；能量 = 平方均值；
/// 公共内区（去 4 像素边）上算；分母为零时为 0。
fn hf_ratio(pixels: &[u8], width: usize, height: usize) -> f32 {
    if width < 9 || height < 9 {
        return 0.0;
    }
    let luma_grid: Vec<f32> = (0..height)
        .flat_map(|y| {
            (0..width).map(move |x| {
                let base = (y * width + x) * 4;
                if base + 3 >= pixels.len() || pixels[base + 3] == 0 {
                    0.0
                } else {
                    display_luma_f32(
                        pixels[base] as f32 / 255.0,
                        pixels[base + 1] as f32 / 255.0,
                        pixels[base + 2] as f32 / 255.0,
                    )
                }
            })
        })
        .collect();
    let box_mean = |x: usize, y: usize, radius: usize| -> f32 {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for yy in y.saturating_sub(radius)..=(y + radius).min(height - 1) {
            for xx in x.saturating_sub(radius)..=(x + radius).min(width - 1) {
                sum += luma_grid[yy * width + xx];
                count += 1;
            }
        }
        sum / count as f32
    };
    let mut e1 = 0.0f32;
    let mut e4 = 0.0f32;
    let mut count = 0usize;
    for y in 4..height - 4 {
        for x in 4..width - 4 {
            let center = luma_grid[y * width + x];
            let hp1 = center - box_mean(x, y, 1);
            let hp4 = center - box_mean(x, y, 4);
            e1 += hp1 * hp1;
            e4 += hp4 * hp4;
            count += 1;
        }
    }
    if count == 0 || e4 <= 0.0 {
        0.0
    } else {
        (e1 / count as f32) / (e4 / count as f32)
    }
}

/// 噪声 σ̂（期 4 D3，Q2-d 改版；底稿 demosaic 后 MAD 法废止）。
///
/// 构造（开发期定版，见测试）：显示亮度（0–255 计数值）先过
/// Anscombe 平方根稳定化 y = 2√(v + 3/8)，再取 stride-3 二阶差分
/// d = y[i] − 2y[i+3] + y[i+6]（行向与列向）；demosaic 相关长度
/// 约 2 像素，stride-3 使差分样本近乎独立。σ̂ 由 MAD 稳健估计，
/// 并按局部 Anscombe 斜率中位数还原到亮度域：
///
/// σ̂ = κ · 1.4826 · MAD(d) / √6 · median(√(v̄ + 3/8)) / 255
///
/// 其中 v̄ 为每组差分三元组的亮度均值。κ 标定见 `NOISE_KAPPA`。
/// 注意：本估计须在**全分辨率**像素上算——盒式降采样会把噪声
/// 压掉数倍（分辨率相关的因子，κ 吸收不了），代理图只用于其余
/// 统计；工具层在降采样前取全幅像素调本函数。
/// 图像过小（任一方向无 stride-3 三元组）⇒ None；MAD 为零
/// （无噪声平坦图）⇒ Some(0.0)，不是 None。
pub fn noise_sigma_rgba8(pixels: &[u8], width: u32, height: u32, kappa: f32) -> Option<f32> {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 {
        return None;
    }
    let luma_at = |x: usize, y: usize| -> f32 {
        let base = (y * w + x) * 4;
        if base + 3 >= pixels.len() || pixels[base + 3] == 0 {
            return 0.0;
        }
        0.299 * pixels[base] as f32
            + 0.587 * pixels[base + 1] as f32
            + 0.114 * pixels[base + 2] as f32
    };
    let anscombe = |v: f32| -> f32 { 2.0 * (v + 0.375).sqrt() };
    let mut diffs: Vec<f32> = Vec::new();
    let mut slopes: Vec<f32> = Vec::new();
    if w >= 7 {
        for y in 0..h {
            for x in 0..w - 6 {
                let (v0, v1, v2) = (luma_at(x, y), luma_at(x + 3, y), luma_at(x + 6, y));
                diffs.push(anscombe(v0) - 2.0 * anscombe(v1) + anscombe(v2));
                slopes.push(((v0 + v1 + v2) / 3.0 + 0.375).sqrt());
            }
        }
    }
    if h >= 7 {
        for y in 0..h - 6 {
            for x in 0..w {
                let (v0, v1, v2) = (luma_at(x, y), luma_at(x, y + 3), luma_at(x, y + 6));
                diffs.push(anscombe(v0) - 2.0 * anscombe(v1) + anscombe(v2));
                slopes.push(((v0 + v1 + v2) / 3.0 + 0.375).sqrt());
            }
        }
    }
    if diffs.len() < 100 {
        return None;
    }
    let mut sorted = diffs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sorted_percentile(&sorted, 50.0);
    let mut deviations: Vec<f32> = sorted.iter().map(|d| (d - median).abs()).collect();
    deviations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mad = sorted_percentile(&deviations, 50.0);
    if mad <= 0.0 {
        return Some(0.0);
    }
    slopes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let slope_median = sorted_percentile(&slopes, 50.0);
    let sigma_counts = kappa * 1.4826 * mad / 6.0f32.sqrt() * slope_median;
    Some(sigma_counts / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 已知分布像素阵：百分位与解析值逐一相等（变异：索引差一 ⇒ 红）。
    #[test]
    fn percentiles_exact_on_synthetic() {
        // 100 像素：亮度 0×10、64×40、128×30、255×20（下分位口径）。
        let mut pixels = Vec::new();
        for (value, count) in [(0u8, 10usize), (64, 40), (128, 30), (255, 20)] {
            for _ in 0..count {
                pixels.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let stats = image_stats_rgba8(&pixels, 100, 1);
        assert_eq!(stats.count, 100);
        // p0.1：秩 ⌈0.1⌉ = 1 ⇒ 首桶 0。
        assert_eq!(stats.luma_p01, 0);
        // p1：秩 1 ⇒ 0。
        assert_eq!(stats.luma_p1, 0);
        // p5：秩 5 ⇒ 0 桶内 ⇒ 0。
        assert_eq!(stats.luma_p5, 0);
        // p25：秩 25 ⇒ 累计 10（0 桶）< 25 ≤ 50（64 桶）⇒ 64。
        assert_eq!(stats.luma_p25, 64);
        // p50：秩 50 ⇒ 64（累计 50 恰好）。
        assert_eq!(stats.luma_p50, 64);
        // p75：秩 75 ⇒ 累计 80（128 桶）⇒ 128。
        assert_eq!(stats.luma_p75, 128);
        // p95：秩 95 ⇒ 累计 80（128 桶）< 95 ≤ 100（255 桶）⇒ 255。
        assert_eq!(stats.luma_p95, 255);
        // p99：秩 99 ⇒ 255。
        assert_eq!(stats.luma_p99, 255);
        // p99.9：秩 100 ⇒ 255。
        assert_eq!(stats.luma_p999, 255);
        // clip：亮度 ≤2 者 10/100；≥253 者 20/100。
        assert!((stats.clip_low_ratio - 0.10).abs() < 1e-6);
        assert!((stats.clip_high_ratio - 0.20).abs() < 1e-6);
    }

    /// 线性百分位：半黑半白图的线性中位应为 decode(0.5) 与 1 的
    /// 下分位——50 黑（线性 0）+ 50 白（线性 1）⇒ P50 = 0。
    #[test]
    fn linear_percentiles_half_black_half_white() {
        let mut pixels = Vec::new();
        for _ in 0..50 {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        }
        for _ in 0..50 {
            pixels.extend_from_slice(&[255, 255, 255, 255]);
        }
        let stats = image_stats_rgba8(&pixels, 100, 1);
        assert_eq!(stats.linear_p50, 0.0, "下分位 P50 落在黑半");
        assert_eq!(stats.linear_p995, 1.0, "P99.5 落在白半");
        assert_eq!(stats.linear_p02, 0.0, "P0.2 落在黑半");
        assert_eq!(stats.linear_p25, 0.0, "P25 落在黑半");
        assert_eq!(stats.linear_p998, 1.0, "P99.8 落在白半");
    }

    /// 注入 σ 已知的高斯噪声到平坦图：新估计（Anscombe + stride-3
    /// 二阶差分，κ = 1.0）的 σ̂ 误差 ≤ ±20%；变异：换回 demosaic
    /// 后 MAD 法 ⇒ 读数掉到真值 0.32–0.45 倍 ⇒ 红（专家结论钉死）。
    /// 变异 2：κ 改回 1.25 ⇒ 读数恰好高估 25% ⇒ 红（κ 的取值
    /// 由本测试钉死，不许凭专家数改回）。
    #[test]
    fn noise_sigma_recovers_injected() {
        // 确定性伪随机（xorshift，不引新依赖）。
        let mut state: u64 = 0x9E3779B97F4A7C15;
        let mut next_gaussian = || {
            // Box–Muller（u1, u2 ∈ (0, 1]）。
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let u1 = ((state >> 11) as f64 / (1u64 << 53) as f64).clamp(1e-12, 1.0);
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let u2 = (state >> 11) as f64 / (1u64 << 53) as f64;
            ((-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()) as f32
        };
        for (base, sigma) in [(0.5f32, 0.03f32), (0.5, 0.01), (0.2, 0.03)] {
            let (w, h) = (256u32, 256u32);
            let mut pixels = vec![0u8; (w as usize) * (h as usize) * 4];
            for pixel in pixels.chunks_exact_mut(4) {
                let value = (base * 255.0 + next_gaussian() * sigma * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                pixel[0] = value;
                pixel[1] = value;
                pixel[2] = value;
                pixel[3] = 255;
            }
            let estimate =
                noise_sigma_rgba8(&pixels, w, h, NOISE_KAPPA).expect("256² 平坦含噪图应能估出 σ");
            let error = (estimate - sigma).abs() / sigma;
            assert!(
                error <= 0.20,
                "基底 {base}、真值 σ {sigma} 时估计 {estimate} 误差 {error:.3} 超 ±20%"
            );
        }
    }

    /// 暗通道均值与 `estimate_atmospheric_light` 的 dark_mean 同定义：
    /// 同一像素阵两处算出的 dark_mean 必须相等（防漂移）。
    #[test]
    fn dark_mean_matches_atmospheric_light() {
        let mut pixels = Vec::new();
        for index in 0..64 {
            let v = (index * 4) as u8;
            pixels.extend_from_slice(&[v, 255 - v, 128, 255]);
        }
        let stats = image_stats_rgba8(&pixels, 8, 8);
        let (_, dark) =
            crate::filter::estimate_atmospheric_light(8, 8, &pixels).expect("8×8 图应能估大气光");
        assert!(
            (stats.dark_mean - dark).abs() < 1e-6,
            "dark_mean {} 与 estimate_atmospheric_light {} 不一致",
            stats.dark_mean,
            dark
        );
    }
}
