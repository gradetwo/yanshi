//! 色彩空间与 alpha 表示（设计文档 6.1）。
//!
//! - 合成在**线性光空间**进行，alpha 使用**预乘**表示。
//! - 输出转换到目标色彩空间（sRGB 传递函数）后才量化到 u8。
//! - 该定义进入 D0 基线：任何平台上的同一份代码必须得到相同的位结果。

/// 线性光、预乘 alpha 的 RGBA 像素。
pub type LinearRgba = [f32; 4];

/// sRGB 传递函数的逆（字节 → 线性光，输入归一化到 `[0, 1]`）。
pub fn srgb_to_linear(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// 线性光 → sRGB 传递函数（输入 `[0, 1]`）。
pub fn linear_to_srgb(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// 字节 → 线性光分量。
pub fn byte_to_linear(byte: u8) -> f32 {
    srgb_to_linear(byte as f32 / 255.0)
}

/// 线性光分量 → 字节（四舍五入到最近整数）。
pub fn linear_to_byte(value: f32) -> u8 {
    (linear_to_srgb(value) * 255.0 + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

/// u8 RGBA（直通 alpha，显示空间）→ 线性光预乘像素。
pub fn u8x4_to_linear_premul(pixel: [u8; 4]) -> LinearRgba {
    let alpha = pixel[3] as f32 / 255.0;
    [
        byte_to_linear(pixel[0]) * alpha,
        byte_to_linear(pixel[1]) * alpha,
        byte_to_linear(pixel[2]) * alpha,
        alpha,
    ]
}

/// 线性光预乘像素 → u8 RGBA（直通 alpha，显示空间）。
pub fn linear_premul_to_u8x4(pixel: LinearRgba) -> [u8; 4] {
    let alpha = pixel[3].clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    [
        linear_to_byte(pixel[0] / alpha),
        linear_to_byte(pixel[1] / alpha),
        linear_to_byte(pixel[2] / alpha),
        (alpha * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8,
    ]
}

/// 把预乘像素合成到不透明背景上，输出显示空间 u8 RGBA。
pub fn composite_over_background(pixel: LinearRgba, background: [u8; 4]) -> [u8; 4] {
    let bg = u8x4_to_linear_premul(background);
    let out = [
        pixel[0] + bg[0] * (1.0 - pixel[3]),
        pixel[1] + bg[1] * (1.0 - pixel[3]),
        pixel[2] + bg[2] * (1.0 - pixel[3]),
        pixel[3] + bg[3] * (1.0 - pixel[3]),
    ];
    linear_premul_to_u8x4(out)
}

/// 直通（非预乘）线性颜色 → 预乘。
pub fn premultiply(straight: LinearRgba) -> LinearRgba {
    let alpha = straight[3];
    [
        straight[0] * alpha,
        straight[1] * alpha,
        straight[2] * alpha,
        alpha,
    ]
}

/// 预乘 → 直通（alpha 为 0 时返回全透明黑）。
pub fn unpremultiply(premultiplied: LinearRgba) -> LinearRgba {
    let alpha = premultiplied[3];
    if alpha <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    [
        premultiplied[0] / alpha,
        premultiplied[1] / alpha,
        premultiplied[2] / alpha,
        alpha,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_endpoints_and_midpoint() {
        assert_eq!(srgb_to_linear(0.0), 0.0);
        assert!((srgb_to_linear(1.0) - 1.0).abs() < 1e-6);
        assert_eq!(linear_to_srgb(0.0), 0.0);
        assert!((linear_to_srgb(1.0) - 1.0).abs() < 1e-6);
        // 中灰：sRGB 0.5 ≈ 线性 0.2140。
        assert!((srgb_to_linear(0.5) - 0.214_041).abs() < 1e-4);
    }

    #[test]
    fn transfer_function_round_trip_quantizes_to_same_byte() {
        for byte in 0..=255u8 {
            let linear = byte_to_linear(byte);
            assert_eq!(linear_to_byte(linear), byte, "byte={byte}");
        }
    }

    #[test]
    fn premultiply_round_trip() {
        let straight = [0.4f32, 0.2, 0.1, 0.5];
        let back = unpremultiply(premultiply(straight));
        for (a, b) in straight.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
        assert_eq!(unpremultiply([0.0, 0.0, 0.0, 0.0]), [0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn u8_round_trip_is_exact_for_opaque_pixels() {
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(51) {
                for b in (0..=255).step_by(85) {
                    let pixel = [r as u8, g as u8, b as u8, 255];
                    let linear = u8x4_to_linear_premul(pixel);
                    assert_eq!(linear[3], 1.0);
                    assert_eq!(linear_premul_to_u8x4(linear), pixel);
                }
            }
        }
    }

    #[test]
    fn compositing_over_opaque_background_replaces_color() {
        let opaque_red = u8x4_to_linear_premul([255, 0, 0, 255]);
        let white = [255u8, 255, 255, 255];
        assert_eq!(
            composite_over_background(opaque_red, white),
            [255, 0, 0, 255]
        );
        // 全透明像素 = 背景。
        assert_eq!(
            composite_over_background([0.0, 0.0, 0.0, 0.0], white),
            [255, 255, 255, 255]
        );
    }
}
