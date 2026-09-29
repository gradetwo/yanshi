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
}

impl AdjustmentKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "brightness_contrast" | "brightness" => Some(Self::BrightnessContrast),
            "saturation" | "vibrance" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            "levels" => Some(Self::Levels),
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
        }
    }

    /// 全部类型。
    pub const ALL: [AdjustmentKind; 4] = [
        Self::BrightnessContrast,
        Self::Saturation,
        Self::Invert,
        Self::Levels,
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
}

impl FilterKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "box_blur" | "blur" => Some(Self::BoxBlur),
            "gaussian_blur" => Some(Self::GaussianBlur),
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
        match self {
            Self::BoxBlur => (radius.ceil() as u32) * passes as u32,
            Self::GaussianBlur => (sigma * 3.0).ceil() as u32,
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
pub fn levels(buffer: &mut Buffer, black: f32, white: f32, gamma: f32) {
    let black = black.clamp(0.0, 0.999);
    let white = white.clamp(black + 1e-3, 1.0);
    let inverse_gamma = 1.0 / gamma.max(1e-3);
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            let normalized = ((*value - black) / (white - black)).clamp(0.0, 1.0);
            *value = normalized.powf(inverse_gamma);
        }
    });
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
        AdjustmentKind::Levels => levels(
            &mut adjusted,
            params.get("black").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("white").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("gamma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
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
pub fn apply_filter(buffer: &mut Buffer, filter_name: &str, params: &Value, opacity: f32) -> bool {
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
        assert!(apply_filter(&mut buffer, "invert", &json!({}), 1.0));
        assert!((buffer.pixel(0, 0)[0] - 0.4).abs() < 1e-5);
        assert!(!apply_filter(&mut buffer, "unknown", &json!({}), 1.0));

        // opacity 插值：一半强度。
        let mut buffer = filled([0.0, 0.0, 0.0, 1.0]);
        apply_filter(&mut buffer, "invert", &json!({}), 0.5);
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
