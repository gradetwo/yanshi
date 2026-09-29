//! 调整与滤镜内核（设计文档 4.2 / 6.1）。
//!
//! 调整（Adjustment）与滤镜（Filter）都是**对象**：作用于同图层中位于其下方（更低 `z_index`）
//! 的内容，符合设计文档「图层只做容器，调整对象化」的定义。
//!
//! 所有运算在**线性光、直通颜色**上执行，再回到预乘表示，保证与合成空间一致（D0 基线）。
//! 需要一个 `radius` 邻域的滤镜（模糊）要求调用方提供带 padding 的缓冲区；
//! [`crate::render`] 会按文档内最大滤镜半径扩展渲染区域后再裁剪。

use crate::buffer::Buffer;
use crate::color::{linear_to_srgb, srgb_to_linear};
use crate::prng::Prng;
use serde_json::Value;

/// 调整类型（`adjustment_type` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentKind {
    /// 亮度/对比度。
    BrightnessContrast,
    /// 饱和度。
    Saturation,
    /// 反相。
    Invert,
    /// 色阶（简化：输入黑白点 + gamma）。
    Levels,
    /// 曝光（EV）。
    Exposure,
    /// 白平衡（色温 + 色调）。
    WhiteBalance,
    /// 曲线（单调三次插值）。
    Curves,
    /// 色相/饱和度/明度。
    Hsl,
    /// 色调分离。
    Posterize,
    /// 色彩平衡（阴影/中间调/高光）。
    ColorBalance,
    /// 分离色调。
    SplitToning,
    /// 自然饱和度。
    Vibrance,
}

impl AdjustmentKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "brightness_contrast" | "brightness" => Some(Self::BrightnessContrast),
            "saturation" | "vibrance" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            "levels" => Some(Self::Levels),
            "exposure" => Some(Self::Exposure),
            "white_balance" => Some(Self::WhiteBalance),
            "curves" => Some(Self::Curves),
            "hsl" | "hue_saturation" => Some(Self::Hsl),
            "posterize" => Some(Self::Posterize),
            "color_balance" => Some(Self::ColorBalance),
            "split_toning" => Some(Self::SplitToning),
            // `saturation` 的同义词之一，保持向后兼容。
            "vibrance_natural" => Some(Self::Vibrance),
            _ => None,
        }
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrightnessContrast => "brightness_contrast",
            Self::Saturation => "saturation",
            Self::Invert => "invert",
            Self::Levels => "levels",
            Self::Exposure => "exposure",
            Self::WhiteBalance => "white_balance",
            Self::Curves => "curves",
            Self::Hsl => "hsl",
            Self::Posterize => "posterize",
            Self::ColorBalance => "color_balance",
            Self::SplitToning => "split_toning",
            Self::Vibrance => "vibrance",
        }
    }

    /// 全部类型。
    pub const ALL: [AdjustmentKind; 12] = [
        Self::BrightnessContrast,
        Self::Saturation,
        Self::Invert,
        Self::Levels,
        Self::Exposure,
        Self::WhiteBalance,
        Self::Curves,
        Self::Hsl,
        Self::Posterize,
        Self::ColorBalance,
        Self::SplitToning,
        Self::Vibrance,
    ];
}

/// 滤镜类型（`filter_name` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// 方框模糊（可多趟近似高斯）。
    BoxBlur,
    /// 高斯模糊（按 sigma 采样，确定性）。
    GaussianBlur,
    /// 亮度/对比度（滤镜形式）。
    BrightnessContrast,
    /// 饱和度。
    Saturation,
    /// 反相。
    Invert,
    /// 动感模糊（方向 + 距离）。
    MotionBlur,
    /// 锐化（非锐化掩模）。
    Sharpen,
    /// 噪点（确定性）。
    Noise,
    /// 暗角。
    Vignette,
    /// 辉光（bloom）。
    Glow,
    /// 局部对比度（clarity）。
    Clarity,
    /// 去雾（暗通道先验，大气光由参数给出）。
    Dehaze,
    /// 胶片颗粒。
    FilmGrain,
}

impl FilterKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "box_blur" | "blur" => Some(Self::BoxBlur),
            "gaussian_blur" => Some(Self::GaussianBlur),
            "motion_blur" => Some(Self::MotionBlur),
            "sharpen" => Some(Self::Sharpen),
            "noise" => Some(Self::Noise),
            "vignette" => Some(Self::Vignette),
            "glow" => Some(Self::Glow),
            "clarity" => Some(Self::Clarity),
            "dehaze" => Some(Self::Dehaze),
            "film_grain" => Some(Self::FilmGrain),
            "brightness_contrast" => Some(Self::BrightnessContrast),
            "saturation" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            _ => None,
        }
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoxBlur => "box_blur",
            Self::GaussianBlur => "gaussian_blur",
            Self::MotionBlur => "motion_blur",
            Self::Sharpen => "sharpen",
            Self::Noise => "noise",
            Self::Vignette => "vignette",
            Self::Glow => "glow",
            Self::Clarity => "clarity",
            Self::Dehaze => "dehaze",
            Self::FilmGrain => "film_grain",
            Self::BrightnessContrast => "brightness_contrast",
            Self::Saturation => "saturation",
            Self::Invert => "invert",
        }
    }

    /// 该滤镜需要的邻域半径（像素）。
    pub fn padding(&self, params: &Value) -> u32 {
        let radius = params
            .get("radius")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let sigma = params
            .get("sigma")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let passes = params
            .get("passes")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 4);
        let distance = params
            .get("distance")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        match self {
            Self::BoxBlur => (radius.ceil() as u32) * passes as u32,
            Self::GaussianBlur => (sigma * 3.0).ceil() as u32,
            // 动感模糊沿 `distance` 采样、锐化需要方框模糊的半径，区域渲染必须外扩，
            // 否则边缘会采到画外产生接缝。
            Self::MotionBlur => (distance / 2.0).ceil() as u32,
            Self::Sharpen => radius.ceil() as u32,
            // 两趟方框模糊，邻域按 2 倍半径申报（glow 与 clarity 同理）。
            Self::Glow | Self::Clarity => (radius.ceil() as u32) * 2,
            // 去雾内部对透射率图做 radius=4 的模糊，固定外扩 8。
            Self::Dehaze => 8,
            // 胶片颗粒逐像素（块坐标）计算，无邻域。
            Self::FilmGrain => 0,
            _ => 0,
        }
    }
}

/// 亮度/对比度（`brightness` 为 `-1..1` 的加性偏移，`contrast` 为 `0..2` 的乘性对比）。
pub fn brightness_contrast(buffer: &mut Buffer, brightness: f32, contrast: f32) {
    let offset = brightness;
    let scale = contrast.max(0.0);
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * scale + offset).clamp(0.0, 1.0);
        }
    });
}

/// 饱和度（`amount`：0 = 灰度，1 = 不变，>1 增强）。
pub fn saturation(buffer: &mut Buffer, amount: f32) {
    // 线性光空间的 Rec.709 亮度权重。
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.max(0.0);
    for_each_straight_color(buffer, |color| {
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        for value in color.iter_mut() {
            *value = (luma + (*value - luma) * amount).clamp(0.0, 1.0);
        }
    });
}

/// 反相（在线性光空间取补）。
pub fn invert(buffer: &mut Buffer) {
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = 1.0 - *value;
        }
    });
}

/// 色阶：`black`/`white` 为输入黑白点（`0..1`），`gamma` 为中间调。
/// 色阶：`channel` 为 `rgb` / `r` / `g` / `b`，可按通道单独调整。
pub fn levels_channel(buffer: &mut Buffer, black: f32, white: f32, gamma: f32, channel: &str) {
    let black = black.clamp(0.0, 0.999);
    let white = white.clamp(black + 1e-3, 1.0);
    let inverse_gamma = 1.0 / gamma.max(1e-3);
    let map = |value: f32| {
        let normalized = ((value - black) / (white - black)).clamp(0.0, 1.0);
        normalized.powf(inverse_gamma)
    };
    for_each_straight_color(buffer, |color| match channel {
        "r" => color[0] = map(color[0]),
        "g" => color[1] = map(color[1]),
        "b" => color[2] = map(color[2]),
        _ => {
            for value in color.iter_mut() {
                *value = map(*value);
            }
        }
    });
}

/// 色阶（三通道同时）。
pub fn levels(buffer: &mut Buffer, black: f32, white: f32, gamma: f32) {
    levels_channel(buffer, black, white, gamma, "rgb");
}

/// 自然饱和度：**低饱和**像素提升更强，高饱和像素几乎不动（避免过饱和与色偏）。
///
/// 权重取 `1 - 当前饱和度`（直通空间的 `chroma / max`），因此灰色不产生色偏（权重为 0
/// 时差值也为 0），已饱和的颜色几乎不变。`amount` 为 0 时不改动。
pub fn vibrance(buffer: &mut Buffer, amount: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 2.0);
    if amount <= 0.0 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        let max = color[0].max(color[1]).max(color[2]);
        let min = color[0].min(color[1]).min(color[2]);
        let saturation = if max <= 0.0 {
            0.0
        } else {
            ((max - min) / max).clamp(0.0, 1.0)
        };
        // 低饱和 → 权重接近 1；已饱和 → 接近 0。
        let weight = 1.0 - saturation;
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        for value in color.iter_mut() {
            let boosted = *value + (*value - luma) * amount * weight;
            *value = boosted.clamp(0.0, 1.0);
        }
    });
}

/// 胶片颗粒：按文档坐标 + `seed` 生成的确定性颗粒，颗粒尺寸可分块（`size` 像素见方）。
///
/// 与 [`noise`] 同一条纪律：随机数序号**只由文档坐标与显式 seed 决定**，
/// 绝不用缓冲内序号 —— 否则同一像素在不同 tile 分块下会得到不同颗粒，
/// 客户端按 tile 组合渲染就会与服务端不一致（这条坑此前踩过）。
///
/// 与 `noise` 的区别：颗粒按 `size` 聚成块（更接近胶片），并按亮度加权
/// （`1-(2·luma-1)²`），使中间调颗粒最明显、纯黑纯白处收敛，避免"塑料噪点"。
pub fn film_grain(buffer: &mut Buffer, amount: f32, size: u32, seed: u64) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let size = size.clamp(1, 8) as i64;
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x as i64 + x as i64;
            let document_y = origin.y as i64 + y as i64;
            // 以 size 为粒度取块坐标：块内所有像素共享同一颗粒值。
            let block_x = document_x.div_euclid(size);
            let block_y = document_y.div_euclid(size);
            let index = (block_y as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(block_x as u64);
            let mut rng = Prng::derive(seed, index);
            let grain = rng.signed();
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            let luma =
                (LUMA[0] * pixel[0] + LUMA[1] * pixel[1] + LUMA[2] * pixel[2]) / alpha.max(1e-3);
            let centered = 2.0 * luma.clamp(0.0, 1.0) - 1.0;
            let weight = 1.0 - centered * centered;
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                // 颗粒为加性（胶片颗粒是密度扰动），按 alpha 缩放以保持预乘不变量。
                let delta = grain * amount * weight * 0.25 * alpha;
                out[channel] = pixel[channel] + delta;
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 色彩平衡：按亮度分档（阴影 / 中间调 / 高光）分别做 RGB 偏移。
///
/// 档位权重由线性亮度推导：`shadows = (1-luma)²`、`highlights = luma²`、
/// `midtones = 1 - shadows - highlights`。每个分量取值 -1..1，按档位加权后加到直通通道上。
pub fn color_balance(
    buffer: &mut Buffer,
    shadows: [f32; 3],
    midtones: [f32; 3],
    highlights: [f32; 3],
) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let clamp_component = |value: f32| value.clamp(-1.0, 1.0);
    let shadows = shadows.map(clamp_component);
    let midtones = midtones.map(clamp_component);
    let highlights = highlights.map(clamp_component);
    for_each_straight_color(buffer, |color| {
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        let luma = luma.clamp(0.0, 1.0);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let midtone_weight = (1.0 - shadow_weight - highlight_weight).max(0.0);
        for channel in 0..3 {
            let delta = shadows[channel] * shadow_weight
                + midtones[channel] * midtone_weight
                + highlights[channel] * highlight_weight;
            color[channel] = (color[channel] + delta).clamp(0.0, 1.0);
        }
    });
}

/// 分离色调：阴影与高光分别着色，`balance` 移动分界点，`amount` 控制总强度。
///
/// 权重用 smoothstep 在 `mid ± 0.25` 之间过渡（`mid = 0.5 + balance × 0.25`），
/// 因此权重单调、确定，且 `balance` 的语义直观（正值把分界推向亮部）。
pub fn split_toning(
    buffer: &mut Buffer,
    shadows: [f32; 3],
    highlights: [f32; 3],
    balance: f32,
    amount: f32,
) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let shadows = shadows.map(|value| value.clamp(-1.0, 1.0));
    let highlights = highlights.map(|value| value.clamp(-1.0, 1.0));
    let mid = 0.5 + balance.clamp(-1.0, 1.0) * 0.25;
    for_each_straight_color(buffer, |color| {
        let luma = (LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2]).clamp(0.0, 1.0);
        let t = ((luma - (mid - 0.25)) / 0.5).clamp(0.0, 1.0);
        let highlight_weight = t * t * (3.0 - 2.0 * t);
        let shadow_weight = 1.0 - highlight_weight;
        for channel in 0..3 {
            let tint = shadows[channel] * shadow_weight + highlights[channel] * highlight_weight;
            color[channel] = (color[channel] + amount * tint).clamp(0.0, 1.0);
        }
    });
}

/// 色调分离：把每个直通通道量化到 `levels` 个台阶。
pub fn posterize(buffer: &mut Buffer, levels: u32) {
    let levels = levels.clamp(2, 64);
    let steps = (levels - 1) as f32;
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * steps).round() / steps;
        }
    });
}

/// HSL 调整：色相旋转（度）+ 饱和度倍率 + 明度偏移。
///
/// 与其它调整一致，作用于**直通线性**通道并钳制到 [0,1]。HSL 采用标准定义：
/// 色相 0°=红、120°=绿、240°=蓝；`lightness` 为 HSL 明度的加性偏移。
pub fn hsl(buffer: &mut Buffer, hue_degrees: f32, saturation: f32, lightness: f32) {
    let hue_shift = hue_degrees.clamp(-360.0, 360.0) / 360.0;
    let saturation = saturation.clamp(0.0, 8.0);
    let lightness = lightness.clamp(-1.0, 1.0);
    for_each_straight_color(buffer, |color| {
        let (r, g, b) = (color[0], color[1], color[2]);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = max - min;
        let l = (max + min) / 2.0;
        let mut h = if chroma <= 0.0 {
            0.0
        } else if max == r {
            ((g - b) / chroma).rem_euclid(6.0) / 6.0
        } else if max == g {
            (((b - r) / chroma) + 2.0) / 6.0
        } else {
            (((r - g) / chroma) + 4.0) / 6.0
        };
        let s = if l <= 0.0 || l >= 1.0 {
            0.0
        } else {
            chroma / (1.0 - (2.0 * l - 1.0).abs())
        };
        // 色相旋转 + 饱和度缩放 + 明度偏移。
        h = (h + hue_shift).rem_euclid(1.0);
        let s = (s * saturation).clamp(0.0, 1.0);
        let l = (l + lightness).clamp(0.0, 1.0);
        // HSL → RGB。
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
        let m = l - c / 2.0;
        let (r1, g1, b1) = match (h * 6.0) as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        color[0] = (r1 + m).clamp(0.0, 1.0);
        color[1] = (g1 + m).clamp(0.0, 1.0);
        color[2] = (b1 + m).clamp(0.0, 1.0);
    });
}

/// 曝光：线性光下按 EV 整体缩放（`2^ev`）。
pub fn exposure(buffer: &mut Buffer, ev: f32) {
    let factor = 2.0f32.powf(ev.clamp(-10.0, 10.0));
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * factor).clamp(0.0, 1.0);
        }
    });
}

/// 白平衡：`temperature` 正=暖（红升蓝降），`tint` 正=品红（绿降）。
pub fn white_balance(buffer: &mut Buffer, temperature: f32, tint: f32) {
    let temperature = temperature.clamp(-1.0, 1.0);
    let tint = tint.clamp(-1.0, 1.0);
    let warm = 0.35 * temperature;
    let magenta = 0.15 * tint;
    for_each_straight_color(buffer, |color| {
        color[0] = (color[0] * (1.0 + warm + magenta)).clamp(0.0, 1.0);
        color[1] = (color[1] * (1.0 - magenta)).clamp(0.0, 1.0);
        color[2] = (color[2] * (1.0 - warm + magenta)).clamp(0.0, 1.0);
    });
}

/// 单调三次插值（Fritsch–Carlson），用于曲线：保证不过冲、不振荡。
fn monotone_curve(points: &[(f32, f32)], x: f32) -> f32 {
    let n = points.len();
    if n == 0 {
        return x;
    }
    if n == 1 {
        return points[0].1;
    }
    if x <= points[0].0 {
        return points[0].1;
    }
    if x >= points[n - 1].0 {
        return points[n - 1].1;
    }
    // 各段斜率。
    let mut slopes = [0.0f32; 64];
    let mut tangents = [0.0f32; 64];
    for i in 0..n - 1 {
        let dx = (points[i + 1].0 - points[i].0).max(1e-6);
        slopes[i] = (points[i + 1].1 - points[i].1) / dx;
    }
    tangents[0] = slopes[0];
    tangents[n - 1] = slopes[n - 2];
    for i in 1..n - 1 {
        if slopes[i - 1] * slopes[i] <= 0.0 {
            tangents[i] = 0.0;
        } else {
            tangents[i] = (slopes[i - 1] + slopes[i]) / 2.0;
        }
    }
    // Fritsch–Carlson 限制，避免过冲。
    for i in 0..n - 1 {
        if slopes[i].abs() < 1e-9 {
            tangents[i] = 0.0;
            tangents[i + 1] = 0.0;
            continue;
        }
        let a = tangents[i] / slopes[i];
        let b = tangents[i + 1] / slopes[i];
        if a < 0.0 {
            tangents[i] = 0.0;
        }
        if b < 0.0 {
            tangents[i + 1] = 0.0;
        }
        let (a, b) = (tangents[i] / slopes[i], tangents[i + 1] / slopes[i]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            tangents[i] = t * a * slopes[i];
            tangents[i + 1] = t * b * slopes[i];
        }
    }
    let mut segment = 0usize;
    for i in 0..n - 1 {
        if x >= points[i].0 && x <= points[i + 1].0 {
            segment = i;
            break;
        }
    }
    let (x0, y0) = points[segment];
    let (x1, y1) = points[segment + 1];
    let h = (x1 - x0).max(1e-6);
    let t = (x - x0) / h;
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    (h00 * y0 + h10 * h * tangents[segment] + h01 * y1 + h11 * h * tangents[segment + 1])
        .clamp(0.0, 1.0)
}

/// 曲线：控制点在 0..1 归一化空间，可按通道（rgb/r/g/b）分别作用。
pub fn curves(buffer: &mut Buffer, points: &[(f32, f32)], channel: &str) {
    if points.len() < 2 {
        return;
    }
    let mut sorted: Vec<(f32, f32)> = points.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let apply = |color: &mut [f32; 3]| match channel {
        "r" => color[0] = monotone_curve(&sorted, color[0].clamp(0.0, 1.0)),
        "g" => color[1] = monotone_curve(&sorted, color[1].clamp(0.0, 1.0)),
        "b" => color[2] = monotone_curve(&sorted, color[2].clamp(0.0, 1.0)),
        _ => {
            for value in color.iter_mut() {
                *value = monotone_curve(&sorted, value.clamp(0.0, 1.0));
            }
        }
    };
    for_each_straight_color(buffer, apply);
}

/// 动感模糊：沿给定角度按 `samples` 个采样点平均（预乘空间平均，边界按 clamp 复制）。
pub fn motion_blur(buffer: &mut Buffer, angle_degrees: f32, distance: f32, samples: u32) {
    let distance = distance.clamp(0.0, 512.0);
    let samples = samples.clamp(2, 64);
    if distance <= 0.0 {
        return;
    }
    let source = buffer.clone();
    let radians = angle_degrees.to_radians();
    let (dx, dy) = (radians.cos(), radians.sin());
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            for i in 0..samples {
                // 采样覆盖 [-distance/2, +distance/2]。
                let t = (i as f32 / (samples - 1) as f32 - 0.5) * distance;
                let sx = (x as f32 + dx * t).round().clamp(0.0, width as f32 - 1.0) as u32;
                let sy = (y as f32 + dy * t).round().clamp(0.0, height as f32 - 1.0) as u32;
                let pixel = source.pixel(sx, sy);
                for channel in 0..4 {
                    sum[channel] += pixel[channel];
                }
            }
            let scale = 1.0 / samples as f32;
            buffer.set_pixel(
                x,
                y,
                [
                    sum[0] * scale,
                    sum[1] * scale,
                    sum[2] * scale,
                    sum[3] * scale,
                ],
            );
        }
    }
}

/// 锐化：非锐化掩模（`src + amount * (src - blur)`），预乘空间并保持 rgb ≤ alpha。
pub fn sharpen(buffer: &mut Buffer, amount: f32, radius: u32) {
    let amount = amount.clamp(0.0, 5.0);
    if amount <= 0.0 {
        return;
    }
    let mut blurred = buffer.clone();
    box_blur(&mut blurred, radius.clamp(1, 8), 1);
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let low = blurred.pixel(x, y);
            let alpha = source[3];
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                let value = source[channel] + amount * (source[channel] - low[channel]);
                out[channel] = value.clamp(0.0, alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 噪点：按 `seed` 逐像素确定性生成（随机量只来自原子 seed，保证可复现）。
pub fn noise(buffer: &mut Buffer, amount: f32, seed: u64) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    // 噪声序号必须由**文档坐标**决定：客户端按 tile 组合渲染，
    // 若用缓冲内序号，同一像素在不同分块下会得到不同噪声（与服务端不一致）。
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x as i64 + x as i64;
            let document_y = origin.y as i64 + y as i64;
            let index = (document_y as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(document_x as u64);
            let mut rng = Prng::derive(seed, index);
            let factor = 1.0 + amount * rng.signed();
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                out[channel] = (pixel[channel] * factor).clamp(0.0, alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 辉光：提取高光区域模糊后**加性**合成（bloom）。
///
/// `threshold` 为直通亮度阈值，`radius` 为方框模糊半径，`intensity` 为加性强度。
/// 合成后把 rgb 钳制到 alpha，保持预乘不变量。
pub fn glow(buffer: &mut Buffer, threshold: f32, radius: u32, intensity: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let threshold = threshold.clamp(0.0, 1.0);
    let intensity = intensity.clamp(0.0, 4.0);
    if intensity <= 0.0 {
        return;
    }
    // 1) 高光掩模（预乘空间，阈值按直通亮度判定）。
    let mut highlights = Buffer::new(
        buffer.origin().0,
        buffer.origin().1,
        buffer.width(),
        buffer.height(),
    );
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let luma = LUMA[0] * pixel[0] + LUMA[1] * pixel[1] + LUMA[2] * pixel[2];
            if luma > threshold * alpha {
                highlights.set_pixel(x, y, pixel);
            }
        }
    }
    // 2) 模糊高光（两趟方框近似高斯）。
    box_blur(&mut highlights, radius.clamp(1, 64), 2);
    // 3) 加性合成。辉光同时增加 alpha（发光本身会让原本透明处出现光），
    //    再按**合成后**的 alpha 钳制，从而既保持预乘不变量又能扩散到笔迹之外。
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let bloom = highlights.pixel(x, y);
            let alpha = (source[3] + bloom[3] * intensity).clamp(0.0, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                out[channel] = (source[channel] + bloom[channel] * intensity).clamp(0.0, alpha);
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 局部对比度（clarity）：大半径非锐化掩模，并按亮度做中间调加权。
///
/// `detail = src - blur`，`out = src + amount × detail × midtone_weight`，
/// 其中 `midtone_weight = 1 - (2·luma - 1)²`（中间调最强，极暗/极亮处抑制），
/// 避免在纯黑纯白处产生光晕。输出钳制到 alpha，保持预乘不变量。
pub fn clarity(buffer: &mut Buffer, amount: f32, radius: u32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 2.0);
    if amount <= 0.0 {
        return;
    }
    let mut blurred = buffer.clone();
    box_blur(&mut blurred, radius.clamp(2, 64), 2);
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let alpha = source[3];
            let low = blurred.pixel(x, y);
            let luma =
                (LUMA[0] * source[0] + LUMA[1] * source[1] + LUMA[2] * source[2]).clamp(0.0, 1.0);
            let centered = 2.0 * luma - 1.0;
            let midtone_weight = (1.0 - centered * centered).clamp(0.0, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                let detail = source[channel] - low[channel];
                out[channel] = (source[channel] + amount * detail * midtone_weight)
                    .max(0.0)
                    .min(alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 去雾（暗通道先验）：`air` 为**调用方给定**的大气光，`omega` 控制去雾强度，
/// `floor` 是透射率下限（避免除零与过度放大噪声）。
///
/// 大气光必须是参数而不是在滤镜内部估计：否则同一像素在分块渲染与整幅渲染下会得到
/// 不同的全局统计量，`分块 == 整幅` 立刻失效（这类问题此前已踩过）。只读工具
/// `estimate_dehaze` 负责扫描文档给出建议值，再由调用方写进原子参数——这与
/// Phase 4b 的「先预览/估计、再作为 patch 应用」是同一套思路。
pub fn dehaze(buffer: &mut Buffer, air: [f32; 3], omega: f32, floor: f32) {
    let omega = omega.clamp(0.0, 1.0);
    let floor = floor.clamp(0.02, 0.8);
    let air = air.map(|value| value.clamp(0.05, 1.0));
    let air_luma = (0.2126 * air[0] + 0.7152 * air[1] + 0.0722 * air[2]).max(1e-3);
    let width = buffer.width();
    let height = buffer.height();
    // 透射率图（先算再模糊，抑制块状伪影）。
    let mut transmission = Buffer::new(buffer.origin().0, buffer.origin().1, width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            // 暗通道：直通空间的最小通道值。
            let dark = pixel[0].min(pixel[1]).min(pixel[2]);
            let t = (1.0 - omega * dark / air_luma).clamp(floor, 1.0);
            transmission.set_pixel(x, y, [t, t, t, 1.0]);
        }
    }
    box_blur(&mut transmission, 4, 1);
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let t = transmission.pixel(x, y)[0].clamp(floor, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                // 直通空间恢复后重新预乘。
                let straight = pixel[channel] / alpha;
                let recovered = (straight - air[channel]) / t + air[channel];
                out[channel] = (recovered.clamp(0.0, 1.0)) * alpha;
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 暗通道均值与「大气光」估计（供只读工具 `estimate_dehaze` 使用）。
///
/// 返回 `(air, dark_mean)`：`air` 取暗通道最亮的那 0.1% 像素的平均颜色。
pub fn estimate_atmospheric_light(
    width: u32,
    height: u32,
    rgba8: &[u8],
) -> Option<([f32; 3], f32)> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba8.len() < expected || width == 0 || height == 0 {
        return None;
    }
    // 用 (暗通道, 索引) 排序取前 0.1%。
    let mut dark_channels: Vec<(f32, usize)> = Vec::with_capacity(expected / 4);
    let mut sum = 0.0f32;
    for index in 0..expected / 4 {
        let base = index * 4;
        let r = rgba8[base] as f32 / 255.0;
        let g = rgba8[base + 1] as f32 / 255.0;
        let b = rgba8[base + 2] as f32 / 255.0;
        let dark = r.min(g).min(b);
        sum += dark;
        dark_channels.push((dark, index));
    }
    let count = dark_channels.len().max(1);
    dark_channels.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let take = ((count as f32 * 0.001).ceil() as usize).max(1);
    let mut air = [0.0f32; 3];
    for (_, index) in dark_channels.iter().take(take) {
        let base = index * 4;
        air[0] += rgba8[base] as f32 / 255.0;
        air[1] += rgba8[base + 1] as f32 / 255.0;
        air[2] += rgba8[base + 2] as f32 / 255.0;
    }
    let scale = 1.0 / take as f32;
    for value in air.iter_mut() {
        *value = (*value * scale).clamp(0.05, 1.0);
    }
    Some((air, sum / count as f32))
}

/// 暗角：按到画布中心的归一化距离做平滑衰减。
pub fn vignette(
    buffer: &mut Buffer,
    strength: f32,
    radius: f32,
    softness: f32,
    canvas: (f64, f64),
) {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, 2.0);
    let softness = softness.clamp(0.02, 2.0);
    // 以**文档**中心与对角线为基准：tile 渲染时每块缓冲只是文档的一部分，
    // 若以缓冲自身为画布裁，块与块之间会出现明显接缝。
    let (canvas_w, canvas_h) = canvas;
    let (cx, cy) = (canvas_w / 2.0, canvas_h / 2.0);
    let max_distance = ((cx * cx + cy * cy).sqrt()).max(1e-3);
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x + x as f64;
            let document_y = origin.y + y as f64;
            let nx = ((document_x - cx) / max_distance) as f32;
            let ny = ((document_y - cy) / max_distance) as f32;
            let distance = (nx * nx + ny * ny).sqrt();
            let t = ((distance - radius) / softness).clamp(0.0, 1.0);
            let falloff = 1.0 - strength * (t * t * (3.0 - 2.0 * t));
            let pixel = buffer.pixel(x, y);
            buffer.set_pixel(
                x,
                y,
                [
                    pixel[0] * falloff,
                    pixel[1] * falloff,
                    pixel[2] * falloff,
                    pixel[3],
                ],
            );
        }
    }
}

/// 可分离方框模糊（`passes` 趟近似高斯；边缘按 clamp 复制）。
pub fn box_blur(buffer: &mut Buffer, radius: u32, passes: u32) {
    if radius == 0 {
        return;
    }
    for _ in 0..passes.max(1) {
        blur_horizontal(buffer, radius);
        blur_vertical(buffer, radius);
    }
}

/// 高斯模糊（按 `sigma` 生成一维核，可分离；核为确定性浮点权重）。
pub fn gaussian_blur(buffer: &mut Buffer, sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut kernel = Vec::with_capacity((radius * 2 + 1) as usize);
    let mut sum = 0.0f32;
    for offset in -radius..=radius {
        let value = (-((offset * offset) as f32) / (2.0 * sigma * sigma)).exp();
        kernel.push(value);
        sum += value;
    }
    for weight in kernel.iter_mut() {
        *weight /= sum;
    }
    convolve_horizontal(buffer, &kernel, radius);
    convolve_vertical(buffer, &kernel, radius);
}

fn blur_horizontal(buffer: &mut Buffer, radius: u32) {
    let width = buffer.width();
    let height = buffer.height();
    let window = (radius * 2 + 1) as f32;
    for y in 0..height {
        let row: Vec<[f32; 4]> = (0..width).map(|x| buffer.pixel(x, y)).collect();
        for x in 0..width {
            let mut acc = [0.0f32; 4];
            for offset in 0..=(radius * 2) {
                let source_x = x as i64 + offset as i64 - radius as i64;
                let clamped = source_x.clamp(0, width as i64 - 1) as usize;
                let pixel = row[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel];
                }
            }
            for value in acc.iter_mut() {
                *value /= window;
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

fn blur_vertical(buffer: &mut Buffer, radius: u32) {
    let width = buffer.width();
    let height = buffer.height();
    let window = (radius * 2 + 1) as f32;
    for x in 0..width {
        let column: Vec<[f32; 4]> = (0..height).map(|y| buffer.pixel(x, y)).collect();
        for y in 0..height {
            let mut acc = [0.0f32; 4];
            for offset in 0..=(radius * 2) {
                let source_y = y as i64 + offset as i64 - radius as i64;
                let clamped = source_y.clamp(0, height as i64 - 1) as usize;
                let pixel = column[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel];
                }
            }
            for value in acc.iter_mut() {
                *value /= window;
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

fn convolve_horizontal(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        let row: Vec<[f32; 4]> = (0..width).map(|x| buffer.pixel(x, y)).collect();
        for x in 0..width {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_x = x as i64 + index as i64 - radius as i64;
                let clamped = source_x.clamp(0, width as i64 - 1) as usize;
                let pixel = row[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

fn convolve_vertical(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width();
    let height = buffer.height();
    for x in 0..width {
        let column: Vec<[f32; 4]> = (0..height).map(|y| buffer.pixel(x, y)).collect();
        for y in 0..height {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_y = y as i64 + index as i64 - radius as i64;
                let clamped = source_y.clamp(0, height as i64 - 1) as usize;
                let pixel = column[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

/// 对缓冲区中每个非全透明像素执行直通颜色变换（变换后重新预乘）。
fn for_each_straight_color(buffer: &mut Buffer, mut transform: impl FnMut(&mut [f32; 3])) {
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let mut color = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
            transform(&mut color);
            buffer.set_pixel(
                x,
                y,
                [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha],
            );
        }
    }
}

/// 内核支持的调整类型名（工具层据此校验参数，避免造出无法渲染的对象）。
pub const ADJUSTMENT_NAMES: [&str; 12] = [
    "brightness_contrast",
    "saturation",
    "invert",
    "levels",
    "exposure",
    "white_balance",
    "curves",
    "hsl",
    "posterize",
    "color_balance",
    "split_toning",
    "vibrance",
];

/// 内核支持的滤镜名。
pub const FILTER_NAMES: [&str; 13] = [
    "box_blur",
    "gaussian_blur",
    "motion_blur",
    "sharpen",
    "noise",
    "vignette",
    "glow",
    "clarity",
    "dehaze",
    "film_grain",
    "brightness_contrast",
    "saturation",
    "invert",
];

/// 解析 `[r,g,b]` 形式的三元组，缺省为零（不改变该档）。
fn triple(value: Option<&Value>) -> [f32; 3] {
    let Some(array) = value.and_then(Value::as_array) else {
        return [0.0, 0.0, 0.0];
    };
    let mut out = [0.0f32; 3];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = array.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    }
    out
}

/// 解析曲线控制点（`[[x,y], ...]`，0..1 归一化）；非法输入退回恒等曲线。
fn curve_points(params: &Value) -> Vec<(f32, f32)> {
    let Some(values) = params.get("points").and_then(Value::as_array) else {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    };
    let mut points: Vec<(f32, f32)> = values
        .iter()
        .filter_map(|value| {
            let pair = value.as_array()?;
            Some((
                pair.first()?.as_f64()? as f32,
                pair.get(1)?.as_f64()? as f32,
            ))
        })
        .map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
        .collect();
    if points.len() < 2 {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    }
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    points
}

/// 应用一个调整对象；返回是否被识别。
pub fn apply_adjustment(
    buffer: &mut Buffer,
    adjustment_type: &str,
    params: &Value,
    opacity: f32,
) -> bool {
    let Some(kind) = AdjustmentKind::from_name(adjustment_type) else {
        return false;
    };
    if opacity <= 0.0 {
        return true;
    }
    // 以副本计算，再按 opacity 插值，保证不透明度语义正确。
    let mut adjusted = buffer.clone();
    match kind {
        AdjustmentKind::BrightnessContrast => brightness_contrast(
            &mut adjusted,
            params
                .get("brightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params
                .get("contrast")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::Saturation => saturation(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::Invert => invert(&mut adjusted),
        AdjustmentKind::Levels => levels_channel(
            &mut adjusted,
            params.get("black").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("white").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("gamma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or("rgb"),
        ),
        AdjustmentKind::Vibrance => vibrance(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.5) as f32,
        ),
        AdjustmentKind::SplitToning => split_toning(
            &mut adjusted,
            triple(params.get("shadows")),
            triple(params.get("highlights")),
            params.get("balance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::ColorBalance => color_balance(
            &mut adjusted,
            triple(params.get("shadows")),
            triple(params.get("midtones")),
            triple(params.get("highlights")),
        ),
        AdjustmentKind::Posterize => posterize(
            &mut adjusted,
            params.get("levels").and_then(Value::as_u64).unwrap_or(6) as u32,
        ),
        AdjustmentKind::Exposure => exposure(
            &mut adjusted,
            params.get("ev").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::WhiteBalance => white_balance(
            &mut adjusted,
            params
                .get("temperature")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params.get("tint").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Hsl => hsl(
            &mut adjusted,
            params.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params
                .get("saturation")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
            params
                .get("lightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Curves => curves(
            &mut adjusted,
            &curve_points(params),
            params
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or("rgb"),
        ),
    }
    if (opacity - 1.0).abs() < f32::EPSILON {
        *buffer = adjusted;
    } else {
        let width = buffer.width();
        let height = buffer.height();
        for y in 0..height {
            for x in 0..width {
                let before = buffer.pixel(x, y);
                let after = adjusted.pixel(x, y);
                let mut mixed = [0.0f32; 4];
                for channel in 0..4 {
                    mixed[channel] = before[channel] + (after[channel] - before[channel]) * opacity;
                }
                buffer.set_pixel(x, y, mixed);
            }
        }
    }
    true
}

/// 应用一个滤镜对象；返回是否被识别。
pub fn apply_filter(
    buffer: &mut Buffer,
    filter_name: &str,
    params: &Value,
    opacity: f32,
    canvas: (f64, f64),
) -> bool {
    let Some(kind) = FilterKind::from_name(filter_name) else {
        return false;
    };
    if opacity <= 0.0 {
        return true;
    }
    let mut filtered = buffer.clone();
    match kind {
        FilterKind::BoxBlur => box_blur(
            &mut filtered,
            params.get("radius").and_then(Value::as_u64).unwrap_or(1) as u32,
            params.get("passes").and_then(Value::as_u64).unwrap_or(1) as u32,
        ),
        FilterKind::GaussianBlur => gaussian_blur(
            &mut filtered,
            params.get("sigma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        FilterKind::MotionBlur => motion_blur(
            &mut filtered,
            params.get("angle").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params
                .get("distance")
                .and_then(Value::as_f64)
                .unwrap_or(8.0) as f32,
            params.get("samples").and_then(Value::as_u64).unwrap_or(16) as u32,
        ),
        FilterKind::Sharpen => sharpen(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(2) as u32,
        ),
        FilterKind::Noise => noise(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.05) as f32,
            params.get("seed").and_then(Value::as_u64).unwrap_or(0),
        ),
        FilterKind::FilmGrain => film_grain(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.3) as f32,
            params.get("size").and_then(Value::as_u64).unwrap_or(2) as u32,
            params.get("seed").and_then(Value::as_u64).unwrap_or(0),
        ),
        FilterKind::Dehaze => dehaze(
            &mut filtered,
            triple(params.get("air")),
            params.get("omega").and_then(Value::as_f64).unwrap_or(0.85) as f32,
            params.get("floor").and_then(Value::as_f64).unwrap_or(0.1) as f32,
        ),
        FilterKind::Clarity => clarity(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.5) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(16) as u32,
        ),
        FilterKind::Glow => glow(
            &mut filtered,
            params
                .get("threshold")
                .and_then(Value::as_f64)
                .unwrap_or(0.6) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(8) as u32,
            params
                .get("intensity")
                .and_then(Value::as_f64)
                .unwrap_or(0.8) as f32,
        ),
        FilterKind::Vignette => vignette(
            &mut filtered,
            params
                .get("strength")
                .and_then(Value::as_f64)
                .unwrap_or(0.4) as f32,
            params.get("radius").and_then(Value::as_f64).unwrap_or(0.75) as f32,
            params
                .get("softness")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32,
            canvas,
        ),
        FilterKind::BrightnessContrast => brightness_contrast(
            &mut filtered,
            params
                .get("brightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params
                .get("contrast")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        ),
        FilterKind::Saturation => saturation(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        FilterKind::Invert => invert(&mut filtered),
    }
    if (opacity - 1.0).abs() < f32::EPSILON {
        *buffer = filtered;
    } else {
        let width = buffer.width();
        let height = buffer.height();
        for y in 0..height {
            for x in 0..width {
                let before = buffer.pixel(x, y);
                let after = filtered.pixel(x, y);
                let mut mixed = [0.0f32; 4];
                for channel in 0..4 {
                    mixed[channel] = before[channel] + (after[channel] - before[channel]) * opacity;
                }
                buffer.set_pixel(x, y, mixed);
            }
        }
    }
    true
}

/// 屏幕空间的辅助：线性值 → sRGB 字节（供调试与测试）。
pub fn linear_to_display_byte(value: f32) -> u8 {
    (linear_to_srgb(value) * 255.0 + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

/// 屏幕空间的辅助：sRGB 字节 → 线性值。
pub fn display_byte_to_linear(byte: u8) -> f32 {
    srgb_to_linear(byte as f32 / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn filled(color: [f32; 4]) -> Buffer {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        buffer.fill(color);
        buffer
    }

    #[test]
    fn brightness_and_contrast_are_linear_in_straight_space() {
        let mut buffer = filled([0.25, 0.25, 0.25, 1.0]);
        brightness_contrast(&mut buffer, 0.25, 1.0);
        assert!((buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-5);
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 2.0);
        assert!((buffer.pixel(0, 0)[0] - 1.0).abs() < 1e-5, "对比度裁到 1.0");
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 0.5);
        assert!((buffer.pixel(0, 0)[0] - 0.25).abs() < 1e-5);
    }

    #[test]
    fn transparency_is_preserved_by_adjustments() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, [0.5, 0.0, 0.0, 0.5]);
        brightness_contrast(&mut buffer, 0.5, 1.0);
        assert_eq!(buffer.pixel(0, 0)[3], 0.5, "alpha 不变");
        assert_eq!(buffer.pixel(1, 0), [0.0; 4], "全透明像素保持全透明");
    }

    #[test]
    fn saturation_zero_is_luma_preserving() {
        let mut buffer = filled([1.0, 0.0, 0.0, 1.0]);
        saturation(&mut buffer, 0.0);
        let pixel = buffer.pixel(0, 0);
        assert!((pixel[0] - 0.2126).abs() < 1e-5, "{pixel:?}");
        assert!((pixel[1] - 0.2126).abs() < 1e-5);
        assert!((pixel[2] - 0.2126).abs() < 1e-5);

        let mut unchanged = filled([0.3, 0.6, 0.9, 1.0]);
        let before = unchanged.pixel(0, 0);
        saturation(&mut unchanged, 1.0);
        let after = unchanged.pixel(0, 0);
        for channel in 0..3 {
            assert!((before[channel] - after[channel]).abs() < 1e-5);
        }
    }

    #[test]
    fn invert_is_involution() {
        let mut buffer = filled([0.2, 0.4, 0.6, 1.0]);
        invert(&mut buffer);
        assert!((buffer.pixel(0, 0)[0] - 0.8).abs() < 1e-5);
        invert(&mut buffer);
        assert!((buffer.pixel(0, 0)[0] - 0.2).abs() < 1e-5);
    }

    /// 新增调整：曝光/白平衡/曲线都必须真的改变像素，且保持预乘不变量 rgb ≤ alpha。
    #[test]
    fn new_adjustments_change_pixels_and_keep_premultiplied() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 2, 1);
            buffer.set_pixel(0, 0, [0.25, 0.25, 0.25, 0.5]);
            buffer.set_pixel(1, 0, [0.1, 0.2, 0.3, 0.4]);
            buffer
        };
        for (name, params) in [
            ("exposure", json!({"ev": 1.0})),
            ("white_balance", json!({"temperature": 0.6, "tint": -0.3})),
            (
                "curves",
                json!({"points": [[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]]}),
            ),
            (
                "curves",
                json!({"points": [[0.0, 0.0], [0.5, 0.2], [1.0, 1.0]], "channel": "r"}),
            ),
        ] {
            let mut buffer = make();
            let before = buffer.pixel(0, 0);
            assert!(
                apply_adjustment(&mut buffer, name, &params, 1.0),
                "{name} 应被识别"
            );
            assert_ne!(before, buffer.pixel(0, 0), "{name} 必须改变像素");
            for (x, y) in [(0u32, 0u32), (1, 0)] {
                let pixel = buffer.pixel(x, y);
                assert!(
                    pixel[0] <= pixel[3] + 1e-3
                        && pixel[1] <= pixel[3] + 1e-3
                        && pixel[2] <= pixel[3] + 1e-3,
                    "{name} 破坏预乘不变量：{pixel:?}"
                );
                assert!(pixel.iter().all(|v| v.is_finite()), "{name} 产出非法值");
            }
        }
        // 曝光 +1EV 在线性光下翻倍。
        let mut buffer = make();
        apply_adjustment(&mut buffer, "exposure", &json!({"ev": 1.0}), 1.0);
        assert!(
            (buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-4,
            "{:?}",
            buffer.pixel(0, 0)
        );
    }

    /// 新增滤镜：动感模糊/锐化/噪点/暗角都要改变像素，噪点必须**由 seed 决定**（可复现）。
    #[test]
    fn new_filters_change_pixels_and_noise_is_seeded() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 4, 1);
            for x in 0..4 {
                // 合法的预乘像素：rgb ≤ alpha。
                buffer.set_pixel(
                    x,
                    0,
                    if x < 2 {
                        [0.2, 0.2, 0.2, 0.5]
                    } else {
                        [0.45, 0.05, 0.05, 0.5]
                    },
                );
            }
            buffer
        };
        for (name, params) in [
            ("motion_blur", json!({"angle": 0.0, "distance": 3.0})),
            ("sharpen", json!({"amount": 1.5, "radius": 1})),
            ("noise", json!({"amount": 0.4, "seed": 7})),
            ("vignette", json!({"strength": 0.8, "radius": 0.1})),
            (
                "glow",
                json!({"threshold": 0.2, "radius": 2, "intensity": 1.0}),
            ),
        ] {
            let mut buffer = make();
            let before: Vec<_> = (0..4).map(|x| buffer.pixel(x, 0)).collect();
            assert!(
                apply_filter(&mut buffer, name, &params, 1.0, (4.0, 1.0)),
                "{name} 应被识别"
            );
            let after: Vec<_> = (0..4).map(|x| buffer.pixel(x, 0)).collect();
            assert_ne!(before, after, "{name} 必须改变像素");
            for pixel in &after {
                assert!(pixel.iter().all(|v| v.is_finite()));
                assert!(
                    pixel[0] <= pixel[3] + 1e-3 && pixel[1] <= pixel[3] + 1e-3,
                    "{name} 破坏预乘不变量：{pixel:?}"
                );
            }
        }
        // 同 seed 同结果，不同 seed 不同结果（确定性来自原子 seed）。
        let run = |seed: u64| {
            let mut buffer = make();
            apply_filter(
                &mut buffer,
                "noise",
                &json!({"amount": 0.6, "seed": seed}),
                1.0,
                (4.0, 1.0),
            );
            (0..4).map(|x| buffer.pixel(x, 0)).collect::<Vec<_>>()
        };
        assert_eq!(run(11), run(11), "同 seed 必须逐位一致");
        assert_ne!(run(11), run(12), "不同 seed 应产生不同噪点");
    }

    /// 动感模糊必须**只沿角度方向**扩散：单个亮点只应沿该方向拉出能量。
    #[test]
    fn motion_blur_spreads_only_along_its_angle() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 16, 16);
            buffer.set_pixel(8, 8, [1.0, 1.0, 1.0, 1.0]);
            buffer
        };
        let blur = |angle: f64| {
            let mut buffer = make();
            apply_filter(
                &mut buffer,
                "motion_blur",
                &json!({"angle": angle, "distance": 8.0, "samples": 8}),
                1.0,
                (16.0, 16.0),
            );
            buffer
        };
        let horizontal = blur(0.0);
        let vertical = blur(90.0);
        // 0°：能量应落在同一行 (4..12, 8)，同一列应基本没有。
        assert!(horizontal.pixel(4, 8)[3] > 0.05, "水平模糊应沿 x 扩散");
        assert!(horizontal.pixel(12, 8)[3] > 0.05, "水平模糊应沿 x 扩散");
        assert!(horizontal.pixel(8, 4)[3] < 0.01, "水平模糊不应沿 y 扩散");
        // 90°：反过来。
        assert!(vertical.pixel(8, 4)[3] > 0.05, "竖直模糊应沿 y 扩散");
        assert!(vertical.pixel(8, 12)[3] > 0.05, "竖直模糊应沿 y 扩散");
        assert!(vertical.pixel(4, 8)[3] < 0.01, "竖直模糊不应沿 x 扩散");
        // 总能量守恒（平均而非叠加）。
        let sum = |buffer: &Buffer| {
            let mut total = 0.0;
            for y in 0..16 {
                for x in 0..16 {
                    total += buffer.pixel(x, y)[3];
                }
            }
            total
        };
        assert!(
            (sum(&horizontal) - 1.0).abs() < 1e-3,
            "模糊应保持总能量：{}",
            sum(&horizontal)
        );
    }

    /// 辉光：亮点应把光晕扩散到邻近暗部，纯暗区域不应被改动，且保持预乘不变量。
    #[test]
    fn glow_bleeds_light_into_neighbours_only() {
        let mut buffer = Buffer::new(0, 0, 16, 16);
        buffer.set_pixel(8, 8, [1.0, 1.0, 1.0, 1.0]);
        let before_corner = buffer.pixel(0, 0);
        glow(&mut buffer, 0.5, 3, 1.0);
        let spark = buffer.pixel(8, 8);
        let around = buffer.pixel(10, 8);
        let corner = buffer.pixel(0, 0);
        assert!(spark[0] > 0.9, "亮点本身应保留：{spark:?}");
        // 单像素高光经 radius 3 两趟模糊后摊得很薄，量级很小但必须非零。
        assert!(around[0] > 0.005, "邻近像素应收到光晕：{around:?}");
        assert!(
            around[3] > 0.005,
            "辉光应同时增加 alpha（否则无法扩散到笔迹外）"
        );
        assert_eq!(corner, before_corner, "远处暗部不应被改动：{corner:?}");
        for y in 0..16 {
            for x in 0..16 {
                let pixel = buffer.pixel(x, y);
                assert!(
                    pixel[0] <= pixel[3] + 1e-3 && pixel[0] >= 0.0 && pixel[0].is_finite(),
                    "辉光破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// HSL：色相旋转 120° 应把红转到绿；饱和度 0 应变灰；明度偏移不该跳出色域。
    #[test]
    fn hsl_rotates_hue_and_desaturates() {
        let red = || {
            let mut buffer = Buffer::new(0, 0, 1, 1);
            buffer.set_pixel(0, 0, [1.0, 0.0, 0.0, 1.0]);
            buffer
        };
        let mut rotated = red();
        hsl(&mut rotated, 120.0, 1.0, 0.0);
        let pixel = rotated.pixel(0, 0);
        assert!(
            pixel[1] > 0.9 && pixel[0] < 0.1,
            "红转 120° 应为绿：{pixel:?}"
        );

        let mut grey = red();
        hsl(&mut grey, 0.0, 0.0, 0.0);
        let pixel = grey.pixel(0, 0);
        assert!(
            (pixel[0] - pixel[1]).abs() < 1e-3 && (pixel[1] - pixel[2]).abs() < 1e-3,
            "饱和度 0 应为中性灰：{pixel:?}"
        );

        // 明度偏移与色相旋转都不该产出越界值（预乘不变量）。
        for (hue, sat, light) in [(0.0, 1.0, 0.9), (200.0, 3.0, -0.5), (359.0, 0.2, 0.4)] {
            let mut buffer = red();
            hsl(&mut buffer, hue, sat, light);
            let pixel = buffer.pixel(0, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0),
                "hsl({hue},{sat},{light}) 越界：{pixel:?}"
            );
        }
    }

    /// 噪点必须**与分块无关**：同一像素在整幅渲染与偏移区域渲染里必须得到同一值。
    #[test]
    fn noise_is_independent_of_the_rendered_region() {
        let full = {
            let mut buffer = Buffer::new(0, 0, 32, 32);
            for y in 0..32 {
                for x in 0..32 {
                    buffer.set_pixel(x, y, [0.5, 0.5, 0.5, 1.0]);
                }
            }
            noise(&mut buffer, 0.5, 99);
            buffer
        };
        // 只渲染右下 16×16（文档坐标偏移 16,16），像素值必须与整幅一致。
        let mut tile = Buffer::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                tile.set_pixel(x, y, [0.5, 0.5, 0.5, 1.0]);
            }
        }
        noise(&mut tile, 0.5, 99);
        for y in 0..16 {
            for x in 0..16 {
                let expected = full.pixel(x + 16, y + 16);
                let actual = tile.pixel(x, y);
                assert!(
                    (expected[0] - actual[0]).abs() < 1e-6,
                    "({x},{y}) 分块噪声 {actual:?} != 整幅 {expected:?}"
                );
            }
        }
    }

    /// 暗角：用**文档尺寸**计算，因此分块渲染与整幅渲染的角落亮度一致，且角落明显暗于中心。
    #[test]
    fn vignette_darkens_corners_and_is_tile_independent() {
        let mut full = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                full.set_pixel(x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        let params = json!({"strength": 0.9, "radius": 0.2, "softness": 0.6});
        vignette(&mut full, 0.9, 0.2, 0.6, (32.0, 32.0));
        let center = full.pixel(16, 16)[0];
        let corner = full.pixel(0, 0)[0];
        assert!(
            corner < center * 0.5,
            "角落应明显暗于中心：{corner} vs {center}"
        );

        // 分块：只渲染右下 16×16 时，几何应与整幅一致（用文档坐标）。
        let mut tile = Buffer::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                tile.set_pixel(x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        vignette(&mut tile, 0.9, 0.2, 0.6, (32.0, 32.0));
        assert!(
            (tile.pixel(0, 0)[0] - full.pixel(16, 16)[0]).abs() < 1e-6,
            "分块渲染的左上像素应与整幅渲染的 (16,16) 一致"
        );
        assert!(
            (tile.pixel(15, 15)[0] - full.pixel(31, 31)[0]).abs() < 1e-6,
            "分块渲染的右下像素应与整幅渲染的 (31,31) 一致"
        );
        let _ = params;
    }

    /// 胶片颗粒：同 seed 可复现、不同 seed 不同、块内一致、**与缓冲分块无关**（文档坐标决定）。
    #[test]
    fn film_grain_is_deterministic_seeded_and_tile_independent() {
        let make = |origin_x: i64| {
            let mut buffer = Buffer::new(origin_x, 0, 32, 8);
            buffer.fill([0.5, 0.5, 0.5, 1.0]);
            buffer
        };
        // 同 seed 两次结果一致。
        let mut first = make(0);
        film_grain(&mut first, 0.5, 2, 7);
        let mut second = make(0);
        film_grain(&mut second, 0.5, 2, 7);
        for y in 0..8 {
            for x in 0..32 {
                assert_eq!(
                    first.pixel(x, y),
                    second.pixel(x, y),
                    "同 seed 必须逐位一致"
                );
            }
        }
        // 不同 seed 结果不同。
        let mut other = make(0);
        film_grain(&mut other, 0.5, 2, 8);
        assert_ne!(
            first.pixel(0, 0),
            other.pixel(0, 0),
            "不同 seed 应产生不同颗粒"
        );
        // 块内一致：size=2 时同一 2×2 块内颗粒值相同。
        assert_eq!(
            first.pixel(4, 4),
            first.pixel(5, 4),
            "size=2 时同一块内应共享颗粒值"
        );
        assert_eq!(first.pixel(4, 4), first.pixel(4, 5));

        // **分块无关**：整幅渲染与「从 x=16 起的小区域」渲染在重叠像素上必须一致。
        let mut whole = make(0);
        film_grain(&mut whole, 0.5, 2, 11);
        let mut right = make(16);
        film_grain(&mut right, 0.5, 2, 11);
        for y in 0..8 {
            for x in 0..16 {
                assert_eq!(
                    whole.pixel(x + 16, y),
                    right.pixel(x, y),
                    "颗粒必须只由文档坐标决定（分块渲染不得改变结果）"
                );
            }
        }
        // amount=0 不改动；越界参数不破坏预乘不变量。
        let mut zero = make(0);
        let before = zero.pixel(3, 3);
        film_grain(&mut zero, 0.0, 99, 1);
        assert_eq!(before, zero.pixel(3, 3));
        let mut extremes = Buffer::new(0, 0, 8, 8);
        extremes.fill([0.2, 0.2, 0.2, 0.5]);
        film_grain(&mut extremes, 9.0, 0, u64::MAX);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite() && *v >= 0.0) && pixel[0] <= pixel[3] + 1e-3,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// 去雾：雾化（与大气光混合）的图像经去雾后对比度应回升；平坦雾区保持不变。
    #[test]
    fn dehaze_restores_contrast_on_a_foggy_image() {
        // 构造「彩色场景混白雾」：暗通道先验要求场景**各通道有差异**，
        // 纯灰度图会让 min(r,g,b) 等于亮度本身，先验退化（第一版测试就踩了这个）。
        let air = [0.85f32, 0.88, 0.9];
        let scene = |x: u32| -> [f32; 3] {
            if x < 16 {
                [0.08, 0.08, 0.10]
            } else {
                [0.60, 0.25, 0.20]
            }
        };
        let mut buffer = Buffer::new(0, 0, 32, 8);
        for y in 0..8 {
            for x in 0..32 {
                let base = scene(x);
                buffer.set_pixel(
                    x,
                    y,
                    [
                        base[0] * 0.5 + air[0] * 0.5,
                        base[1] * 0.5 + air[1] * 0.5,
                        base[2] * 0.5 + air[2] * 0.5,
                        1.0,
                    ],
                );
            }
        }
        // 用红通道衡量对比度：亮部偏红、暗部接近中性。
        let before_diff = buffer.pixel(28, 4)[0] - buffer.pixel(4, 4)[0];
        dehaze(&mut buffer, air, 0.9, 0.1);
        let after_diff = buffer.pixel(28, 4)[0] - buffer.pixel(4, 4)[0];
        assert!(
            after_diff > before_diff,
            "去雾应提升对比度：{before_diff} → {after_diff}"
        );
        // 平坦雾区（全为 air）经去雾后仍接近原值。
        let mut flat = Buffer::new(0, 0, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                flat.set_pixel(x, y, [air[0], air[1], air[2], 1.0]);
            }
        }
        let flat_before = flat.pixel(8, 8)[0];
        dehaze(&mut flat, air, 0.9, 0.1);
        assert!(
            (flat.pixel(8, 8)[0] - flat_before).abs() < 0.05,
            "纯大气光区域不应被大幅改动：{flat_before} → {}",
            flat.pixel(8, 8)[0]
        );
        // 预乘不变量与越界参数。
        let mut extremes = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                extremes.set_pixel(x, y, [0.2, 0.2, 0.2, 0.4]);
            }
        }
        dehaze(&mut extremes, [9.0, -9.0, 0.5], 9.0, -9.0);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite() && *v >= 0.0) && pixel[0] <= pixel[3] + 1e-3,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
        // 估计器：雾图的暗通道均值应明显大于 0，且 air 接近亮部。
        let mut rgba8 = Vec::with_capacity(32 * 8 * 4);
        for _ in 0..8 {
            for x in 0..32 {
                let value = if x < 16 { 0.55f32 } else { 0.8 };
                let byte = (value * 255.0) as u8;
                rgba8.extend_from_slice(&[byte, byte, byte, 255]);
            }
        }
        let (estimated, dark_mean) = estimate_atmospheric_light(32, 8, &rgba8).expect("估计应成功");
        assert!(dark_mean > 0.4, "雾图暗通道均值应偏高：{dark_mean}");
        assert!(estimated[0] > 0.7, "大气光应接近亮部：{estimated:?}");
    }

    /// 局部对比度：软边变陡、平坦区不变、极暗处抑制（中间调加权）。
    #[test]
    fn clarity_steepens_soft_edges_and_spares_flats() {
        // 中灰底上一条模糊的亮带（软边）。
        let mut buffer = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = if (12..20).contains(&x) { 0.6 } else { 0.4 };
                buffer.set_pixel(x, y, [value, value, value, 1.0]);
            }
        }
        // 带内 (16) 比带外 (11) 更亮；clarity 应把这一差拉大。
        let before_left = buffer.pixel(11, 16)[0];
        let before_right = buffer.pixel(16, 16)[0];
        clarity(&mut buffer, 1.0, 8);
        let after_left = buffer.pixel(11, 16)[0];
        let after_right = buffer.pixel(16, 16)[0];
        // 边缘两侧的差应被拉大（左侧更暗、右侧更亮），平坦区基本不变。
        assert!(
            (after_right - after_left) > (before_right - before_left),
            "局部对比度应拉大边缘差：{before_left}/{before_right} → {after_left}/{after_right}"
        );
        let flat_before = 0.4f32;
        let flat_after = buffer.pixel(2, 2)[0];
        assert!(
            (flat_after - flat_before).abs() < 0.02,
            "平坦区不应被改动：{flat_after}"
        );

        // 极暗处的中间调加权应抑制效果。
        let mut dark = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = if (12..20).contains(&x) { 0.08 } else { 0.02 };
                dark.set_pixel(x, y, [value, value, value, 1.0]);
            }
        }
        let dark_before = dark.pixel(11, 16)[0];
        clarity(&mut dark, 1.0, 8);
        let dark_delta = (dark.pixel(11, 16)[0] - dark_before).abs();
        let mid_delta = (after_left - before_left).abs();
        assert!(
            dark_delta < mid_delta,
            "极暗处的提升应弱于中间调：{dark_delta} vs {mid_delta}"
        );

        // 预乘不变量与越界参数。
        let mut extremes = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                extremes.set_pixel(x, y, [0.3, 0.3, 0.3, 0.5]);
            }
        }
        clarity(&mut extremes, 9.0, 999);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite())
                        && pixel[0] <= pixel[3] + 1e-3
                        && pixel[0] >= 0.0,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// 自然饱和度：低饱和像素提升更强；灰色不变；已饱和颜色几乎不动。
    #[test]
    fn vibrance_boosts_low_saturation_more() {
        // 灰 / 低饱和 / 高饱和 三个像素。
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.5, 0.5, 0.5, 1.0]);
            buffer.set_pixel(1, 0, [0.45, 0.5, 0.55, 1.0]);
            buffer.set_pixel(2, 0, [0.9, 0.1, 0.1, 1.0]);
            buffer
        };
        let before = make();
        let mut after = make();
        vibrance(&mut after, 1.0);
        // 灰色完全不变。
        assert_eq!(
            before.pixel(0, 0),
            after.pixel(0, 0),
            "灰色不应产生色偏（权重为 0 且差值为 0）"
        );
        // 低饱和像素的通道差应增大。
        let before_low = before.pixel(1, 0);
        let after_low = after.pixel(1, 0);
        assert!(
            (after_low[2] - after_low[0]).abs() > (before_low[2] - before_low[0]).abs(),
            "低饱和像素应被拉开：{before_low:?} → {after_low:?}"
        );
        // 比的是**相对**增益（增益 ÷ 原通道差）：饱和像素的绝对差本来就大 8 倍，
        // 直接比绝对值会得出相反结论（这是写测试时第三次踩同一个坑）。
        let before_high = before.pixel(2, 0);
        let after_high = after.pixel(2, 0);
        let low_base = (before_low[2] - before_low[0]).abs();
        let high_base = (before_high[0] - before_high[1]).abs();
        let low_relative = ((after_low[2] - after_low[0]).abs() - low_base) / low_base;
        let high_relative = ((after_high[0] - after_high[1]).abs() - high_base) / high_base;
        assert!(
            high_relative < low_relative * 0.5,
            "已饱和颜色的**相对**提升应明显更弱：low={low_relative} high={high_relative}"
        );
        // amount=0 不改动；越界参数不越界。
        let mut zero = make();
        let snapshot = zero.pixel(1, 0);
        vibrance(&mut zero, 0.0);
        assert_eq!(snapshot, zero.pixel(1, 0));
        let mut extremes = make();
        vibrance(&mut extremes, 9.0);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界 amount 不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 分离色调：暗部取阴影色、亮部取高光色；`balance` 移动分界。
    #[test]
    fn split_toning_tints_shadows_and_highlights() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.05, 0.05, 0.05, 1.0]);
            buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]);
            // 亮部用 0.6 而不是 0.95：后者加上着色会直接触顶被钳，看不出通道差。
            buffer.set_pixel(2, 0, [0.6, 0.6, 0.6, 1.0]);
            buffer
        };
        // 阴影加蓝、高光加黄。
        let mut toned = make();
        split_toning(&mut toned, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 0.0, 1.0);
        let dark = toned.pixel(0, 0);
        let bright = toned.pixel(2, 0);
        assert!(dark[2] > dark[0] + 0.2, "暗部应偏蓝：{dark:?}");
        // 高光权重在该亮度约 0.78，所以通道差约 0.17（不是满额 0.3）。
        assert!(bright[0] > bright[2] + 0.1, "亮部应偏黄：{bright:?}");

        // balance 把分界点推向亮部（mid 0.5 → 0.75），因此中间灰落入**阴影侧**。
        let mut shifted = make();
        split_toning(&mut shifted, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 1.0, 1.0);
        let neutral = shifted.pixel(1, 0);
        assert!(
            neutral[2] > neutral[0],
            "balance=+1 把分界推高，中间灰应取阴影色：{neutral:?}"
        );
        // 反向（mid → 0.25）时中间灰落入高光侧。
        let mut back = make();
        split_toning(&mut back, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], -1.0, 1.0);
        let neutral_back = back.pixel(1, 0);
        assert!(
            neutral_back[0] > neutral_back[2],
            "balance=-1 把分界压低，中间灰应取高光色：{neutral_back:?}"
        );

        // amount=0 不改动；越界参数不产生非法值。
        let mut zero = make();
        let before = zero.pixel(1, 0);
        split_toning(&mut zero, [1.0; 3], [1.0; 3], 0.0, 0.0);
        assert_eq!(before, zero.pixel(1, 0));
        let mut extremes = make();
        split_toning(&mut extremes, [9.0; 3], [-9.0; 3], 9.0, 9.0);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界参数不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 色彩平衡：阴影档只影响暗部、高光档只影响亮部，中间调影响两者之间的像素。
    #[test]
    fn color_balance_affects_tone_bands_separately() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.05, 0.05, 0.05, 1.0]); // 暗
            buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]); // 中
            buffer.set_pixel(2, 0, [0.95, 0.95, 0.95, 1.0]); // 亮
            buffer
        };
        // 只加阴影档的红：暗部提升最多，亮部几乎不变。
        let mut shadows = make();
        color_balance(&mut shadows, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3]);
        // 注意比较的是**增量**而不是绝对值：绝对值当然仍随底色增大。
        let dark_delta = shadows.pixel(0, 0)[0] - 0.05;
        let mid_delta = shadows.pixel(1, 0)[0] - 0.5;
        let bright_delta = shadows.pixel(2, 0)[0] - 0.95;
        assert!(dark_delta > 0.2, "暗部应被显著提升：{dark_delta}");
        assert!(
            dark_delta > mid_delta && mid_delta > bright_delta,
            "阴影档增量应随亮度递减：{dark_delta}/{mid_delta}/{bright_delta}"
        );
        assert!(bright_delta < 0.01, "亮部几乎不变：{bright_delta}");

        // 只加高光档的蓝：亮部提升最多。
        let mut highlights = make();
        color_balance(&mut highlights, [0.0; 3], [0.0; 3], [0.0, 0.0, 0.3]);
        // 0.95 加 0.271 会触顶被钳到 1.0，因此断言「饱和到白」而不是比增量。
        let bright_blue = highlights.pixel(2, 0)[2];
        let dark_blue_delta = highlights.pixel(0, 0)[2] - 0.05;
        assert!(
            bright_blue > 0.999,
            "亮部高光档应把蓝推到饱和：{bright_blue}"
        );
        assert!(dark_blue_delta < 0.01, "暗部几乎不变：{dark_blue_delta}");

        // 中间调 + 越界参数不应产出非法值。
        let mut extremes = make();
        color_balance(&mut extremes, [9.0, -9.0, 0.0], [1.0; 3], [0.0, 0.0, -9.0]);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界参数不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 分通道色阶只影响指定通道；色调分离把通道量化到有限台阶。
    #[test]
    fn levels_channel_and_posterize() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 1, 1);
            buffer.set_pixel(0, 0, [0.25, 0.5, 0.75, 1.0]);
            buffer
        };
        let mut red_only = make();
        levels_channel(&mut red_only, 0.0, 1.0, 1.0, "r");
        let pixel = red_only.pixel(0, 0);
        assert!(
            (pixel[1] - 0.5).abs() < 1e-3 && (pixel[2] - 0.75).abs() < 1e-3,
            "只应改红通道：{pixel:?}"
        );

        // 色调分离 3 级（台阶 0 / 0.5 / 1）：用**非中点**输入避免半数舍入的歧义。
        let mut quantized = Buffer::new(0, 0, 1, 1);
        quantized.set_pixel(0, 0, [0.2, 0.5, 0.8, 1.0]);
        posterize(&mut quantized, 3);
        let pixel = quantized.pixel(0, 0);
        assert!((pixel[0] - 0.0).abs() < 1e-3, "0.2 应量化到 0：{pixel:?}");
        assert!((pixel[1] - 0.5).abs() < 1e-3, "0.5 应保持：{pixel:?}");
        assert!((pixel[2] - 1.0).abs() < 1e-3, "0.8 应量化到 1：{pixel:?}");
        // 极端级别数不应产出越界值。
        for levels in [2u32, 4, 64, 1000] {
            let mut buffer = make();
            posterize(&mut buffer, levels);
            let pixel = buffer.pixel(0, 0);
            assert!(
                pixel.iter().all(|v| (0.0..=1.0).contains(v)),
                "levels={levels} 越界：{pixel:?}"
            );
        }
    }

    #[test]
    fn levels_maps_black_and_white_points() {
        let mut buffer = Buffer::new(0, 0, 3, 1);
        buffer.set_pixel(0, 0, [0.25, 0.25, 0.25, 1.0]);
        buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]);
        buffer.set_pixel(2, 0, [0.75, 0.75, 0.75, 1.0]);
        levels(&mut buffer, 0.25, 0.75, 1.0);
        assert!(buffer.pixel(0, 0)[0].abs() < 1e-5);
        assert!((buffer.pixel(1, 0)[0] - 0.5).abs() < 1e-5);
        assert!((buffer.pixel(2, 0)[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn box_blur_distributes_energy_and_keeps_alpha() {
        let mut buffer = Buffer::new(0, 0, 8, 1);
        buffer.set_pixel(4, 0, [1.0, 0.0, 0.0, 1.0]);
        box_blur(&mut buffer, 1, 1);
        // 中心保留大部分能量，相邻像素获得能量。
        assert!(buffer.pixel(4, 0)[0] < 1.0);
        assert!(buffer.pixel(3, 0)[0] > 0.0);
        assert!(buffer.pixel(5, 0)[0] > 0.0);
        let total_alpha: f32 = (0..8).map(|x| buffer.pixel(x, 0)[3]).sum();
        assert!((total_alpha - 1.0).abs() < 0.3, "alpha 总能量近似守恒");

        // 模糊后 alpha 最大值下降，但不会出现负值。
        for x in 0..8 {
            let pixel = buffer.pixel(x, 0);
            assert!(pixel[0] >= 0.0 && pixel[3] >= 0.0);
        }
    }

    #[test]
    fn gaussian_blur_is_symmetric_and_normalized() {
        let mut buffer = Buffer::new(0, 0, 9, 1);
        buffer.set_pixel(4, 0, [1.0, 1.0, 1.0, 1.0]);
        gaussian_blur(&mut buffer, 1.0);
        let center = buffer.pixel(4, 0);
        assert!(center[3] > 0.2 && center[3] < 0.6, "center={center:?}");
        for offset in 1..=3u32 {
            let left = buffer.pixel(4 - offset, 0);
            let right = buffer.pixel(4 + offset, 0);
            assert!((left[3] - right[3]).abs() < 1e-6, "对称性");
        }
        assert_eq!(box_blur_padding(), 0);
    }

    fn box_blur_padding() -> u32 {
        FilterKind::BoxBlur.padding(&json!({"radius": 0}))
    }

    #[test]
    fn adjustment_and_filter_dispatch_by_name() {
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "brightness_contrast",
            &json!({"brightness": 0.1, "contrast": 1.0}),
            1.0
        ));
        assert!((buffer.pixel(0, 0)[0] - 0.6).abs() < 1e-5);
        assert!(!apply_adjustment(&mut buffer, "unknown", &json!({}), 1.0));
        assert!(apply_filter(
            &mut buffer,
            "invert",
            &json!({}),
            1.0,
            (2.0, 1.0)
        ));
        assert!((buffer.pixel(0, 0)[0] - 0.4).abs() < 1e-5);
        assert!(!apply_filter(
            &mut buffer,
            "unknown",
            &json!({}),
            1.0,
            (2.0, 1.0)
        ));

        // opacity 插值：一半强度。
        let mut buffer = filled([0.0, 0.0, 0.0, 1.0]);
        apply_filter(&mut buffer, "invert", &json!({}), 0.5, (2.0, 1.0));
        assert!((buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn filter_padding_reports_neighborhood_needs() {
        assert_eq!(
            FilterKind::BoxBlur.padding(&json!({"radius": 3, "passes": 2})),
            6
        );
        assert_eq!(FilterKind::GaussianBlur.padding(&json!({"sigma": 2.0})), 6);
        assert_eq!(FilterKind::Invert.padding(&json!({})), 0);
    }
}
