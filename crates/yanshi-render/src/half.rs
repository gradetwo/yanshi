//! IEEE-754 binary16（f16）存储与转换（设计文档 6.1）。
//!
//! 内存 tile 用 **f16 线性**保存：合成正确性依赖线性空间与足够精度，
//! 而持久缓存与网络传输用 u8/WebP（显示空间）。f16 往返必须确定，
//! 因此这里手写位级转换（round-to-nearest-even），不依赖平台或第三方库。

/// f32 → f16 位模式（round-to-nearest-even；溢出为 ±Inf，过小为 ±0）。
pub fn f32_to_f16_bits(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xFF) as i32;
    let mantissa = bits & 0x007F_FFFF;

    if exponent == 0xFF {
        // Inf / NaN（保留 NaN 的静默化）。
        return sign | 0x7C00 | (u16::from(mantissa != 0) * 0x0200);
    }

    let unbiased = exponent - 127;

    if unbiased > 15 {
        return sign | 0x7C00; // 超出 f16 范围 → Inf
    }

    if unbiased >= -14 {
        // 规格化数。
        let mut half_exp = (unbiased + 15) as u32;
        let mut half_mant = mantissa >> 13;
        let round_bit = (mantissa >> 12) & 1;
        let sticky = (mantissa & 0x0FFF) != 0;
        if round_bit == 1 && (sticky || (half_mant & 1) == 1) {
            half_mant += 1;
            if half_mant == 0x0400 {
                half_mant = 0;
                half_exp += 1;
                if half_exp >= 0x1F {
                    return sign | 0x7C00;
                }
            }
        }
        return sign | ((half_exp as u16) << 10) | half_mant as u16;
    }

    // 次正规数或零。
    if unbiased < -25 {
        return sign;
    }
    let shift = (-unbiased - 14) as u32;
    let total_shift = 13 + shift;
    if total_shift >= 32 {
        return sign;
    }
    let full = mantissa | 0x0080_0000; // 补上隐含的 1
    let mut half_mant = full >> total_shift;
    let remainder = full & ((1u32 << total_shift) - 1);
    let halfway = 1u32 << (total_shift - 1);
    if remainder > halfway || (remainder == halfway && (half_mant & 1) == 1) {
        half_mant += 1;
    }
    sign | half_mant as u16
}

/// f16 位模式 → f32。
pub fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 0x1F);
    let mantissa = u32::from(bits & 0x03FF);

    let out = if exponent == 0 {
        if mantissa == 0 {
            sign
        } else {
            // 次正规数：规格化到 f32。
            let mut exp = 127 - 15 + 1;
            let mut m = mantissa;
            while m & 0x0400 == 0 {
                m <<= 1;
                exp -= 1;
            }
            m &= 0x03FF;
            sign | (exp << 23) | (m << 13)
        }
    } else if exponent == 0x1F {
        sign | 0x7F80_0000 | (mantissa << 13)
    } else {
        sign | ((exponent + 127 - 15) << 23) | (mantissa << 13)
    };
    f32::from_bits(out)
}

/// 便捷：f32 → f16 → 回到 f32（量化后的值）。
pub fn quantize_f16(value: f32) -> f32 {
    f16_bits_to_f32(f32_to_f16_bits(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_bit_patterns() {
        assert_eq!(f32_to_f16_bits(0.0), 0x0000);
        assert_eq!(f32_to_f16_bits(-0.0), 0x8000);
        assert_eq!(f32_to_f16_bits(1.0), 0x3C00);
        assert_eq!(f32_to_f16_bits(-2.0), 0xC000);
        assert_eq!(f32_to_f16_bits(0.5), 0x3800);
        assert_eq!(f32_to_f16_bits(65504.0), 0x7BFF, "f16 最大规格化数");
        assert_eq!(f32_to_f16_bits(100_000.0), 0x7C00, "溢出 → Inf");
        assert_eq!(f32_to_f16_bits(f32::INFINITY), 0x7C00);
        assert_eq!(f32_to_f16_bits(f32::NEG_INFINITY), 0xFC00);
        assert_eq!(f32_to_f16_bits(1e-9), 0x0000, "过小 → 0");

        assert_eq!(f16_bits_to_f32(0x3C00), 1.0);
        assert_eq!(f16_bits_to_f32(0xC000), -2.0);
        assert_eq!(f16_bits_to_f32(0x7BFF), 65504.0);
        // 2^-24 = 0x3380_0000（精确的 2 的幂 ✓）⇒ 写成位模式 ⇒ 内核里**再无 `powi`** ✓
        assert_eq!(
            f16_bits_to_f32(0x0001),
            f32::from_bits(0x3380_0000),
            "最小次正规数"
        );
        assert!(f16_bits_to_f32(0x7C00).is_infinite());
        assert!(f16_bits_to_f32(0x7C01).is_nan());
    }

    #[test]
    fn every_f16_pattern_round_trips() {
        for bits in 0u32..=0xFFFF {
            let bits = bits as u16;
            let value = f16_bits_to_f32(bits);
            if value.is_nan() {
                assert!(f16_bits_to_f32(f32_to_f16_bits(value)).is_nan());
                continue;
            }
            assert_eq!(
                f32_to_f16_bits(value),
                bits,
                "f16 位模式 {bits:#06x}（{value}）往返失败"
            );
        }
    }

    #[test]
    fn rounding_is_round_to_nearest_even() {
        // 1.0 与下一个 f16（0x3C01 = 1.0009765625）之间的中点应落到偶数尾数。
        let next = f16_bits_to_f32(0x3C01);
        let midpoint = (1.0 + next) / 2.0;
        assert_eq!(f32_to_f16_bits(midpoint), 0x3C00, "中点取偶");
        let upper = f16_bits_to_f32(0x3C02);
        let midpoint2 = (next + upper) / 2.0;
        assert_eq!(f32_to_f16_bits(midpoint2), 0x3C02, "中点取偶（进位）");
        assert_eq!(quantize_f16(midpoint), 1.0);
    }

    #[test]
    fn subnormal_boundary_carries_into_normal_range() {
        // 最大次正规数 0x03FF 与最小规格化数 0x0400 之间。
        assert_eq!(f32_to_f16_bits(f16_bits_to_f32(0x03FF)), 0x03FF);
        assert_eq!(f32_to_f16_bits(f16_bits_to_f32(0x0400)), 0x0400);
        let midpoint = (f16_bits_to_f32(0x03FF) + f16_bits_to_f32(0x0400)) / 2.0;
        assert_eq!(f32_to_f16_bits(midpoint), 0x0400, "进位到最小规格化数");
    }

    #[test]
    fn linear_range_is_dense_enough_for_alpha() {
        // 0..1 之间的 alpha 量化误差应小于 1/1024（f16 在 [0.5,1) 的步长为 2^-11）。
        let mut worst = 0.0f32;
        for step in 0..=1000 {
            let value = step as f32 / 1000.0;
            let error = (quantize_f16(value) - value).abs();
            worst = worst.max(error);
        }
        assert!(worst < 1.0 / 1024.0, "最大量化误差 {worst}");
    }
}
