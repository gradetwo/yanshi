//! **直方图统计**（纯函数，无 IO）：对 RGBA8 像素算每通道 256 桶计数、
//! 亮度桶与摘要（均值／最小／最大／百分位）。
//!
//! 亮度用**整数** Rec.601 近似 `(299·r + 587·g + 114·b + 500) / 1000`：
//! 全程整数运算 ⇒ 跨平台逐位一致（不用浮点舍入当判据 ✓）。
//! 百分位定义钉死为「最小的、累计计数 ≥ ⌈p/100·N⌉ 的桶值」（下分位口径），
//! 测试按这条定义断精确值。

/// 一条通道（或亮度）的 256 桶摘要。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelSummary {
    /// 算术均值（0..255 标度，f64）。
    pub mean: f64,
    /// 最小非空桶（空直方图时为 0）。
    pub min: u8,
    /// 最大非空桶（空直方图时为 0）。
    pub max: u8,
    /// 第 5 百分位（下分位口径，见模块文档）。
    pub p5: u8,
    /// 中位数（第 50 百分位，下分位口径）。
    pub median: u8,
    /// 第 95 百分位（下分位口径）。
    pub p95: u8,
}

/// 一幅 RGBA8 图的直方图：每通道 256 桶 + 亮度桶 + 像素数。
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    /// 红通道桶。
    pub r: [u64; 256],
    /// 绿通道桶。
    pub g: [u64; 256],
    /// 蓝通道桶。
    pub b: [u64; 256],
    /// alpha 通道桶。
    pub a: [u64; 256],
    /// 亮度桶（整数 Rec.601，见模块文档）。
    pub luma: [u64; 256],
    /// 参与统计的像素数（不足 4 字节的尾巴不计）。
    pub count: u64,
}

/// 由桶求第 `percent` 百分位（下分位口径）：最小的、累计计数 ≥ ⌈percent/100·count⌉ 的桶值。
/// `count == 0` 或 `percent` 不在 (0, 100] 时返回 0（percent=100 ⇒ max）。
pub fn percentile(buckets: &[u64; 256], count: u64, percent: u32) -> u8 {
    if count == 0 || percent == 0 || percent > 100 {
        return 0;
    }
    // ⌈percent·count / 100⌉，整数运算避免浮点边界漂移。
    let rank = (u128::from(percent) * u128::from(count)).div_ceil(100) as u64;
    let mut cumulative = 0u64;
    for (value, bucket) in buckets.iter().enumerate() {
        cumulative += bucket;
        if cumulative >= rank {
            return value as u8;
        }
    }
    255
}

/// 由桶算摘要（均值／极值／p5／中位数／p95）。
pub fn summarize(buckets: &[u64; 256], count: u64) -> ChannelSummary {
    if count == 0 {
        return ChannelSummary {
            mean: 0.0,
            min: 0,
            max: 0,
            p5: 0,
            median: 0,
            p95: 0,
        };
    }
    let mut weighted = 0u128;
    let mut min = 0u8;
    let mut max = 0u8;
    let mut seen_any = false;
    for (value, bucket) in buckets.iter().enumerate() {
        if *bucket == 0 {
            continue;
        }
        if !seen_any {
            min = value as u8;
            seen_any = true;
        }
        max = value as u8;
        weighted += u128::from(value as u64) * u128::from(*bucket);
    }
    ChannelSummary {
        mean: weighted as f64 / count as f64,
        min,
        max,
        p5: percentile(buckets, count, 5),
        median: percentile(buckets, count, 50),
        p95: percentile(buckets, count, 95),
    }
}

/// 对 RGBA8 字节（行优先、每像素 4 字节）算直方图。
pub fn histogram_rgba8(pixels: &[u8]) -> Histogram {
    let mut histogram = Histogram {
        r: [0; 256],
        g: [0; 256],
        b: [0; 256],
        a: [0; 256],
        luma: [0; 256],
        count: 0,
    };
    for pixel in pixels.chunks_exact(4) {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        histogram.r[usize::from(r)] += 1;
        histogram.g[usize::from(g)] += 1;
        histogram.b[usize::from(b)] += 1;
        histogram.a[usize::from(a)] += 1;
        let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b) + 500) / 1000;
        histogram.luma[luma as usize] += 1;
        histogram.count += 1;
    }
    histogram
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 半黑半白（4 黑 + 4 白，alpha 全 255）：断精确桶值与摘要。
    #[test]
    fn half_black_half_white_exact_buckets_and_summary() {
        let mut pixels = Vec::new();
        for _ in 0..4 {
            pixels.extend_from_slice(&[0, 0, 0, 255]);
        }
        for _ in 0..4 {
            pixels.extend_from_slice(&[255, 255, 255, 255]);
        }
        let histogram = histogram_rgba8(&pixels);
        assert_eq!(histogram.count, 8);
        for buckets in [&histogram.r, &histogram.g, &histogram.b, &histogram.luma] {
            assert_eq!(buckets[0], 4, "黑半应恰好 4 个落在 0 桶");
            assert_eq!(buckets[255], 4, "白半应恰好 4 个落在 255 桶");
            assert_eq!(buckets.iter().sum::<u64>(), 8, "每通道桶总和必须等于像素数");
        }
        assert_eq!(histogram.a[255], 8);
        assert_eq!(histogram.a.iter().sum::<u64>(), 8);
        let summary = summarize(&histogram.r, histogram.count);
        assert_eq!(summary.mean, 127.5);
        assert_eq!(summary.min, 0);
        assert_eq!(summary.max, 255);
        // 下分位口径：p5 的 rank=⌈0.4⌉=1 ⇒ 0；中位数 rank=⌈4⌉=4 ⇒ 累计在 0 桶恰达 4 ⇒ 0；
        // p95 的 rank=⌈7.6⌉=8 ⇒ 255。
        assert_eq!(summary.p5, 0);
        assert_eq!(summary.median, 0);
        assert_eq!(summary.p95, 255);
    }

    /// 已知线性渐变：r = 0..=255 各恰好一个像素（g=b=0，a=255）。
    #[test]
    fn linear_gradient_exact_buckets_and_summary() {
        let mut pixels = Vec::new();
        for value in 0..=255u8 {
            pixels.extend_from_slice(&[value, 0, 0, 255]);
        }
        let histogram = histogram_rgba8(&pixels);
        assert_eq!(histogram.count, 256);
        for (value, bucket) in histogram.r.iter().enumerate() {
            assert_eq!(*bucket, 1, "渐变里 r={value} 应恰好出现 1 次");
        }
        assert_eq!(histogram.g[0], 256);
        assert_eq!(histogram.b[0], 256);
        assert_eq!(histogram.a[255], 256);
        assert_eq!(histogram.luma.iter().sum::<u64>(), 256);
        // 纯红像素的亮度 = (299·r + 500) / 1000（整数除）：亮度 76 ⟺ 75500 ≤ 299r < 76500
        // ⟺ r ∈ {253, 254, 255}，恰 3 个。
        assert_eq!(histogram.luma[76], 3, "亮度 76 的纯红像素应恰为 3 个");
        assert_eq!(histogram.luma[0], 2, "亮度 0 的 r 取值应为 0..=1 共 2 个");
        let summary = summarize(&histogram.r, histogram.count);
        assert_eq!(summary.mean, 127.5);
        assert_eq!(summary.min, 0);
        assert_eq!(summary.max, 255);
        // rank：p5=⌈12.8⌉=13 ⇒ 值 12；中位数 rank=128 ⇒ 值 127；p95 rank=⌈243.2⌉=244 ⇒ 值 243。
        assert_eq!(summary.p5, 12);
        assert_eq!(summary.median, 127);
        assert_eq!(summary.p95, 243);
        let green = summarize(&histogram.g, histogram.count);
        assert_eq!(green.mean, 0.0);
        assert_eq!(green.median, 0);
        assert_eq!(green.max, 0);
    }

    /// 空输入与残缺尾巴：不 panic，count 只计完整像素。
    #[test]
    fn empty_and_trailing_bytes_are_safe() {
        let empty = histogram_rgba8(&[]);
        assert_eq!(empty.count, 0);
        assert_eq!(summarize(&empty.r, 0).mean, 0.0);
        let partial = histogram_rgba8(&[10, 20, 30]);
        assert_eq!(partial.count, 0, "不足 4 字节不算一个像素");
    }
}
