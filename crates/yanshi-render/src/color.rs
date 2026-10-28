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
        libm::powf((v + 0.055) / 1.055, 2.4)
    }
}

/// 线性光 → sRGB 传递函数（输入 `[0, 1]`）。
pub fn linear_to_srgb(value: f32) -> f32 {
    let v = value.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * libm::powf(v, 1.0 / 2.4) - 0.055
    }
}

/// **扩展（保符号）sRGB 编码**：线性 → 显示，定义域为全体实数。
///
/// 与 [`linear_to_srgb`] 的区别只在**不钳 `[0, 1]`**：`> 1` 的 HDR 线性值沿
/// 幂段自然延拓（编码值 > 1），负值镜像延拓（`enc(−v) = −enc(v)`，保符号）。
/// 显示空间算子（期 3 A 族：brightness_contrast／color_balance／
/// selective_color／clarity／vignette）必须走这一对函数——若在编码前钳
/// `[0, 1]`，第一个显示算子就会把 HDR 高光压成 1.0 后再也回不来
/// （专家 Q1-a 红线）。`[0, 1]` 内与 [`linear_to_srgb`] **逐位一致**。
pub fn srgb_encode_extended(value: f32) -> f32 {
    if value < 0.0 {
        -srgb_encode_extended(-value)
    } else if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * libm::powf(value, 1.0 / 2.4) - 0.055
    }
}

/// **扩展（保符号）sRGB 解码**：[`srgb_encode_extended`] 的逆。
///
/// `[0, 1]` 内与 [`srgb_to_linear`] **逐位一致**；显示值 > 1 解出 > 1 的
/// 线性值，负显示值镜像（保符号）。
pub fn srgb_decode_extended(value: f32) -> f32 {
    if value < 0.0 {
        -srgb_decode_extended(-value)
    } else if value <= 0.040_45 {
        value / 12.92
    } else {
        libm::powf((value + 0.055) / 1.055, 2.4)
    }
}

/// 解码方向 sRGB 查找表的项数：输入是 **u8** ⇒ 定义域只有 **256** 个值。
///
/// 与编码方向的 [`SRGB_ENCODE_LUT_SIZE`]（4097 项、属 D1 显示路径、允许 ±1 LSB）本质不同：
/// 这里的表是 [`srgb_to_linear`] 在 256 个字节上的**原值**，没有索引量化 ⇒ **精确**，
/// 查表与直接计算**逐位相同**（D0 基线不受影响）。
const SRGB_DECODE_LUT_SIZE: usize = 256;

/// 字节 → 线性光的**精确**查找表（表项由**同一个** [`srgb_to_linear`] 逐项生成）。
///
/// 为什么不能用近似公式：color.rs 头注释把该转换列入 **D0 基线**（同一份代码在任何平台
/// 必须给出相同位结果）。表项既然就是该函数的值，查表只是省掉每分量一次 `libm::powf`，
/// **不引入任何新算式**。与编码表同构，用 `OnceLock` 只算一次。
fn srgb_decode_lut() -> &'static [f32; SRGB_DECODE_LUT_SIZE] {
    static LUT: std::sync::OnceLock<[f32; SRGB_DECODE_LUT_SIZE]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut table = [0.0f32; SRGB_DECODE_LUT_SIZE];
        for (index, slot) in table.iter_mut().enumerate() {
            *slot = srgb_to_linear(index as f32 / 255.0);
        }
        table
    })
}

/// 解码方向 sRGB 查找表（fix15 输入）：输入是 **u16**（`u8 << 7`，满量程 32640）⇒ 定义域只有 **32641** 个值。
///
/// 与编码方向的 [`SRGB_ENCODE_LUT_SIZE`]（4097 项、属 D1 显示路径、允许 ±1 LSB）本质不同：
/// 这里的表是 [`srgb_to_linear`] 在 32641 个 fix15 值上的**原值**，没有索引量化 ⇒ **精确**，
/// 查表与直接计算**逐位相同**（D0 基线不受影响）。
/// 用处：`smudge_dab` 每像素调 6 次 `srgb_to_linear`（3 通道 × 2），查表省掉这 6 次 `powf`。
const SRGB_DECODE_FIX15_LUT_SIZE: usize = 32641;

/// fix15（`u8 << 7`）→ 线性光的**精确**查找表（表项由**同一个** [`srgb_to_linear`] 逐项生成）。
///
/// 与 [`srgb_decode_lut`] 同构，用 `OnceLock` 只算一次。
fn srgb_decode_fix15_lut() -> &'static [f32; SRGB_DECODE_FIX15_LUT_SIZE] {
    static LUT: std::sync::OnceLock<[f32; SRGB_DECODE_FIX15_LUT_SIZE]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut table = [0.0f32; SRGB_DECODE_FIX15_LUT_SIZE];
        for (index, slot) in table.iter_mut().enumerate() {
            *slot = srgb_to_linear(index as f32 / 32640.0);
        }
        table
    })
}

/// **取出 fix15 解码查表**（供逐像素循环**只取一次**）。
///
/// 动机与 [`srgb_decode_table`] 相同：循环外取一次，省掉每像素的 `OnceLock` 原子检查。
pub fn srgb_decode_fix15_table() -> &'static [f32; SRGB_DECODE_FIX15_LUT_SIZE] {
    srgb_decode_fix15_lut()
}

/// 用**已取好的表**做一次 fix15 解码（与 `srgb_to_linear(value as f32 / 32640.0)` 逐位一致）。
///
/// 超量程值钳到表尾：`srgb_to_linear` 本就把输入钳到 `[0, 1]`，而表尾正是
/// `srgb_to_linear(1.0)`，两者一致。
#[inline]
pub fn decode_fix15_with_table(table: &[f32; SRGB_DECODE_FIX15_LUT_SIZE], value: u16) -> f32 {
    table[(value as usize).min(SRGB_DECODE_FIX15_LUT_SIZE - 1)]
}

/// 字节 → 线性光分量（查 [`srgb_decode_lut`]；**逐位等于** `srgb_to_linear(byte as f32 / 255.0)`）。
pub fn byte_to_linear(byte: u8) -> f32 {
    srgb_decode_lut()[byte as usize]
}

/// **取出解码查表**（供逐像素循环**只取一次**）。
///
/// 动机：`byte_to_linear()` 每次调用都要过一遍 `OnceLock` 的原子检查，
/// 而 `u8x4_to_linear_premul` 每像素调 3 次 ⇒ 在 512² 循环里就是 78 万次原子检查。
/// 表本身是 `'static` ⇒ 循环外取一次即可，**逐位等价**。
/// （与 [`srgb_encode_table`] 同构，见其注释。）
pub fn srgb_decode_table() -> &'static [f32; SRGB_DECODE_LUT_SIZE] {
    srgb_decode_lut()
}

/// 用**已取好的表**做一次解码（与 [`byte_to_linear`] 逐位一致，只是省掉取表）。
#[inline]
pub fn decode_with_table(table: &[f32; SRGB_DECODE_LUT_SIZE], byte: u8) -> f32 {
    table[byte as usize]
}

/// 用**已取好的表**做一次 u8x4 → 线性预乘（与 [`u8x4_to_linear_premul`] 逐位一致）。
#[inline]
pub fn u8x4_to_linear_premul_with_table(
    table: &[f32; SRGB_DECODE_LUT_SIZE],
    pixel: [u8; 4],
) -> LinearRgba {
    let alpha = pixel[3] as f32 / 255.0;
    [
        table[pixel[0] as usize] * alpha,
        table[pixel[1] as usize] * alpha,
        table[pixel[2] as usize] * alpha,
        alpha,
    ]
}

/// 显示编码用的 sRGB 查找表（**仅用于显示/预览/缩略图等输出编码**）。
///
/// 设计 6.1 把「合成后端层、预览、缩略图」列为 **D1（允许 ±1 LSB）**，且设计方已批准显示路径
/// 使用该精度；计算内核的 tile 数据（f16 线性预乘）仍是 **D0**，不受影响。
///
/// 表项由**同一个** `linear_to_srgb` 公式生成（不是另一套近似），误差只来自索引量化：
/// 4096 档 → 编码后误差 ≤1 个 8 位台阶 ✓。索引只依赖输入的确定值，因此
/// **分块与整幅、服务端与内核仍然逐字节一致** ✓（两端共用同一张表）。
///
/// 实测（同批次对照，1024² 合成到背景并编码）：135.9ms → **74.1ms（1.84×）**，最大字节差 1 ✓。
pub const SRGB_ENCODE_LUT_SIZE: usize = 4097;

fn srgb_encode_lut() -> &'static [f32; SRGB_ENCODE_LUT_SIZE] {
    static LUT: std::sync::OnceLock<[f32; SRGB_ENCODE_LUT_SIZE]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut table = [0.0f32; SRGB_ENCODE_LUT_SIZE];
        for (index, slot) in table.iter_mut().enumerate() {
            let linear = index as f32 / (SRGB_ENCODE_LUT_SIZE - 1) as f32;
            *slot = linear_to_srgb(linear);
        }
        table
    })
}

/// 查表版 sRGB 编码（显示路径；误差 ≤1 LSB，见 [`srgb_encode_lut`]）。
pub fn linear_to_srgb_fast(value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    let index = (clamped * (SRGB_ENCODE_LUT_SIZE - 1) as f32).round() as usize;
    srgb_encode_lut()[index.min(SRGB_ENCODE_LUT_SIZE - 1)]
}

/// 线性光分量 → 字节（四舍五入到最近整数）。
///
/// 显示/预览/缩略图走查表版（D1，±1 LSB）；需要严格等于数学定义的调用方用
/// [`linear_to_byte_exact`]。
pub fn linear_to_byte(value: f32) -> u8 {
    (linear_to_srgb_fast(value) * 255.0 + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

/// 精确版（逐次 `powf`），供对精度敏感的路径与对照测试使用。
pub fn linear_to_byte_exact(value: f32) -> u8 {
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

/// **取出显示编码查表** ✓（供逐像素循环**只取一次** ✓）。
///
/// 实测动机 ✓：`srgb_encode_lut()` 每次调用都要过一遍 `OnceLock` 的原子检查 ✓，
/// 而每个像素要查 3 次 ✓ ⇒ 在 512² 循环里就是 78 万次原子检查 ✗。
/// 表本身是 `'static` ✓ ⇒ 循环外取一次即可 ✓，**逐位等价** ✓。
pub fn srgb_encode_table() -> &'static [f32; SRGB_ENCODE_LUT_SIZE] {
    srgb_encode_lut()
}

/// 用**已取好的表**做一次编码 ✓（与 [`linear_to_srgb_fast`] 逐位一致 ✓，只是省掉取表 ✓）。
#[inline]
pub fn encode_with_table(table: &[f32; SRGB_ENCODE_LUT_SIZE], value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    let index = (clamped * (SRGB_ENCODE_LUT_SIZE - 1) as f32).round() as usize;
    table[index.min(SRGB_ENCODE_LUT_SIZE - 1)]
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

/// **带表版本** ✓：与 [`linear_premul_to_u8x4`] 逐位一致 ✓（同一套算式 ✓），
/// 只是把表当参数传进来 ✓ ⇒ 逐像素循环可以只取一次表 ✓（实测动机见 [`srgb_encode_table`] ✓）。
#[inline]
pub fn linear_premul_to_u8x4_with(
    table: &[f32; SRGB_ENCODE_LUT_SIZE],
    pixel: LinearRgba,
) -> [u8; 4] {
    let alpha = pixel[3].clamp(0.0, 1.0);
    if alpha <= 0.0 {
        return [0, 0, 0, 0];
    }
    let byte = |value: f32| {
        (encode_with_table(table, value) * 255.0 + 0.5)
            .floor()
            .clamp(0.0, 255.0) as u8
    };
    [
        byte(pixel[0] / alpha),
        byte(pixel[1] / alpha),
        byte(pixel[2] / alpha),
        (alpha * 255.0 + 0.5).floor().clamp(0.0, 255.0) as u8,
    ]
}

/// **带表版本**的背景合成 ✓（与 [`composite_over_linear`] 逐位一致 ✓）。
#[inline]
pub fn composite_over_linear_with(
    table: &[f32; SRGB_ENCODE_LUT_SIZE],
    pixel: LinearRgba,
    bg: LinearRgba,
) -> [u8; 4] {
    let one_minus_alpha = 1.0 - pixel[3];
    linear_premul_to_u8x4_with(
        table,
        [
            pixel[0] + bg[0] * one_minus_alpha,
            pixel[1] + bg[1] * one_minus_alpha,
            pixel[2] + bg[2] * one_minus_alpha,
            pixel[3] + bg[3] * one_minus_alpha,
        ],
    )
}

/// 把 u8 背景色解码为**线性预乘**（供逐像素合成复用 ✓）。
///
/// **为什么要单独暴露它** ✓（本轮实测换来的 ✓）：
/// 原来每个像素都走 [`composite_over_background`] ✓ ⇒ 那里面**每像素都重新解码一次背景** ✗
/// ⇒ 背景解码里有 **4 次 `srgb_to_linear`** ✓（每次一个 `powf` ✓）⇒ **每像素 4 次 `powf`** ✗。
/// 实测（512² 区域、**零图层**、纯管线）：**30.5ms** ✓，约 116ns/像素 ✓，
/// 且与像素数严格线性 ✓、与内容几乎无关 ✓（每加一层只多约 1.3ms ✓）
/// ⇒ 成本就在这个循环里 ✓，而**背景色在整块区域里通常是同一个** ✓。
///
/// 把它提到循环外 ✓ 是**逐位等价**的优化 ✓：同一个函数 ✓、同一个输入 ✓、确定性 ✓
/// ⇒ **不改变任何像素** ✓、**不涉及确定性等级** ✓（不需要 D1 放宽 ✓）。
pub fn background_linear_premul(background: [u8; 4]) -> LinearRgba {
    u8x4_to_linear_premul(background)
}

/// 把预乘像素合成到**已经解码好**的线性背景上，输出显示空间 u8 RGBA ✓。
pub fn composite_over_linear(pixel: LinearRgba, bg: LinearRgba) -> [u8; 4] {
    let out = [
        pixel[0] + bg[0] * (1.0 - pixel[3]),
        pixel[1] + bg[1] * (1.0 - pixel[3]),
        pixel[2] + bg[2] * (1.0 - pixel[3]),
        pixel[3] + bg[3] * (1.0 - pixel[3]),
    ];
    linear_premul_to_u8x4(out)
}

/// 把预乘像素合成到不透明背景上，输出显示空间 u8 RGBA。
///
/// 便利版本 ✓（一次性合成单个像素时用它 ✓）；**逐像素循环请用**
/// [`background_linear_premul`] + [`composite_over_linear`] ✓，把背景解码提到循环外 ✓。
pub fn composite_over_background(pixel: LinearRgba, background: [u8; 4]) -> [u8; 4] {
    composite_over_linear(pixel, background_linear_premul(background))
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

    /// 扩展传递：[0,1] 内与钳制版逐位一致；域外不钳、保符号、往返恒等。
    /// 变异：给 enc/dec 加回 clamp(0,1) ⇒ HDR 与负值两组断言立刻红。
    #[test]
    fn extended_transfer_matches_clamped_inside_and_extends_outside() {
        for v in [0.0f32, 0.003_130_8, 0.040_45, 0.214_041, 0.5, 0.735_4, 1.0] {
            assert_eq!(srgb_encode_extended(v), linear_to_srgb(v), "enc {v}");
            assert_eq!(srgb_decode_extended(v), srgb_to_linear(v), "dec {v}");
        }
        // HDR：线性 4.0 编码 > 1（不被钳到 1），且往返还原。
        let encoded = srgb_encode_extended(4.0);
        assert!(encoded > 1.7, "HDR 编码不得钳到 1：{encoded}");
        assert!((srgb_decode_extended(encoded) - 4.0).abs() < 1e-4);
        // 负值镜像保符号。
        assert_eq!(srgb_encode_extended(-0.5), -srgb_encode_extended(0.5));
        assert!((srgb_decode_extended(-0.735_4) + 0.5).abs() < 1e-3);
        assert!((srgb_decode_extended(srgb_encode_extended(-2.5)) + 2.5).abs() < 1e-4);
    }

    #[test]
    fn transfer_function_round_trip_quantizes_to_same_byte() {
        for byte in 0..=255u8 {
            let linear = byte_to_linear(byte);
            assert_eq!(linear_to_byte(linear), byte, "byte={byte}");
        }
    }

    /// fix15 解码表**逐位等于**直接计算：表项就是 `srgb_to_linear` 的原值，查表只是省掉 `powf`。
    /// 变异：把表生成换成近似公式（或把 `decode_fix15_with_table` 换成线性插值）⇒ 本测试立刻红。
    #[test]
    fn fix15_decode_table_is_bitwise_exact() {
        let table = srgb_decode_fix15_table();
        for value in 0..=32640u16 {
            assert_eq!(
                decode_fix15_with_table(table, value),
                srgb_to_linear(value as f32 / 32640.0),
                "fix15={value}"
            );
        }
        // 超量程值钳到表尾（即 `srgb_to_linear(1.0)`），与直接计算的钳制一致。
        assert_eq!(
            decode_fix15_with_table(table, u16::MAX),
            srgb_to_linear(1.0)
        );
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

    /// **核心判据**：解码表对**全部 256 个字节**都必须与直接计算**逐位相同**（D0）。
    ///
    /// 为什么这条能红：表项一旦不来自同一个 `srgb_to_linear`（换成近似、或索引差一档），
    /// 至少一个字节的 `to_bits()` 就会不同 ⇒ `assert_eq!` 当场失败并点名该字节。
    /// 它守的性质正是 color.rs 头注释写进 D0 基线的那条：查表结果 == 函数值，不是"接近"。
    #[test]
    fn decode_lut_is_bit_identical_to_direct_formula() {
        for byte in 0..=255u8 {
            let direct = srgb_to_linear(byte as f32 / 255.0);
            let tabled = byte_to_linear(byte);
            assert_eq!(
                tabled.to_bits(),
                direct.to_bits(),
                "byte={byte}：查表 {tabled:?} != 直接计算 {direct:?}（逐位比较）"
            );
        }
    }
}

/// **显式颜色单位**（C1）：调用方用 `color_space` 参数说清浮点数组是什么空间。
///
/// 只有**数组写法**受它影响 —— 对象式 `{r,g,b,a}` 与 hex 字符串恒为 sRGB 字节，
/// 与参数无关（与 `parse_spec_color` 的既有口径一致）：
/// * `None`（不传参数）⇒ 就是 `parse_spec_color` 本身：量级切换原样保留；
/// * `Srgb` ⇒ 数组按 sRGB 解释：分量 ≤ 1 时 `v*255` 取整再走字节分支，
///   已有分量 > 1 时直接就是字节分支（与不传时同一条路）；
/// * `Linear` ⇒ 数组按线性直通：**即使分量 > 1 也不 reroute 到字节分支**，
///   浮点原样取（负值 / 非有限仍拒绝，与既有线性分支同一条校验）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColorSpace {
    /// 0..1 浮点数组按 sRGB 字节解释（`v*255` 取整再解码）。
    Srgb,
    /// 浮点数组按线性直通（即使分量 > 1 也不 reroute 到字节分支）。
    Linear,
}

impl ColorSpace {
    /// **按名解析**（P2-9）：`update_stroke` 写进对象 `data.color_space` 的字符串，
    /// 渲染侧（`BrushSpec::from_value`）读回时用它 —— 与服务端
    /// `color_space_from_value` 认同一套名字（`"srgb"` / `"linear"`），
    /// 其它值 ⇒ `None`（调用方那一层已在写进 `data` 之前校验过非法值，
    /// 这里只做防御性回落：不认的名字按缺省量级切换解释，不静默编一个空间）。
    pub fn from_name(text: &str) -> Option<Self> {
        match text {
            "srgb" => Some(Self::Srgb),
            "linear" => Some(Self::Linear),
            _ => None,
        }
    }
}

/// 带显式空间的颜色解析（C1）：`parse_spec_color` 的带参形态。
pub fn parse_spec_color_with_space(
    value: &serde_json::Value,
    space: Option<ColorSpace>,
) -> Option<LinearRgba> {
    use serde_json::Value;
    let space = match space {
        None => return parse_spec_color(value),
        Some(space) => space,
    };
    // **对象式 / hex 不受参数影响**：直接走既有解析（恒为字节）。
    if !matches!(value, Value::Array(_)) {
        return parse_spec_color(value);
    }
    let items = value.as_array()?;
    let numbers: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
    if numbers.len() < 3 || numbers.len() != items.len() {
        return None;
    }
    match space {
        ColorSpace::Srgb => {
            // **分量全 ≤ 1 ⇒ 先 `v*255` 取整，再走既有字节分支**（含 `a=1.0` 按不透明
            // 处理的那条特殊情形 ⇒ 复用 `parse_spec_color`，不另写一份字节解析）。
            // 已有分量 > 1 ⇒ 本来就是字节写法 ⇒ 直接走既有解析。
            if numbers.iter().all(|component| *component <= 1.0) {
                // **不夹紧**：负分量要留给字节分支的范围校验去拒绝（夹紧会把它静默吃成 0）。
                let bytes: Vec<Value> = numbers
                    .iter()
                    .map(|component| Value::from((component * 255.0).round()))
                    .collect();
                parse_spec_color(&Value::Array(bytes))
            } else {
                parse_spec_color(value)
            }
        }
        ColorSpace::Linear => {
            // **线性直通**：与 `parse_spec_color` 线性分支同一段逻辑，唯独**不做量级切换**。
            let channel = |index: usize, default: f32| {
                numbers.get(index).copied().unwrap_or(default as f64) as f32
            };
            let color = [
                channel(0, 0.0),
                channel(1, 0.0),
                channel(2, 0.0),
                channel(3, 1.0),
            ];
            if color
                .iter()
                .any(|component| !component.is_finite() || *component < 0.0)
            {
                return None;
            }
            Some(color)
        }
    }
}

/// 解析工具/原子里的颜色描述，统一为**直通线性** RGBA（`0..1`）。
///
/// 接受三种写法（工具层与渲染层共用同一实现，避免出现两套解析）：
///
/// | 写法 | 含义 |
/// |---|---|
/// | `[r, g, b]` / `[r, g, b, a]`，所有分量 ≤ 1 | 直通线性（alpha 缺省 1.0） |
/// | `[r, g, b, a]`，任一分量 > 1 | sRGB 字节 `0-255`（Agent 常写 `[40,120,60,255]`） |
/// | `{"r": 0-255, "g": …, "b": …, "a": …}` | sRGB 字节（alpha 缺省 255） |
/// | `"#RRGGBB"` / `"#RRGGBBAA"` / `"#RGB"` | sRGB 十六进制 |
///
/// 历史事故：`[40, 120, 60, 255]` 曾被当作线性浮点直通，alpha = 255 直接饱和成白色，
/// 于是「画了一笔却什么都没看见」。字节写法与 `{"r": …}` 必须等价。
pub fn parse_spec_color(value: &serde_json::Value) -> Option<LinearRgba> {
    use serde_json::Value;
    match value {
        Value::Array(items) => {
            let numbers: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
            if numbers.len() < 3 || numbers.len() != items.len() {
                return None;
            }
            if numbers.iter().any(|component| *component > 1.0) {
                let byte = |index: usize, default: u8| -> Option<u8> {
                    let raw = numbers.get(index).copied().unwrap_or(default as f64);
                    if !(0.0..=255.0).contains(&raw) {
                        return None;
                    }
                    Some(raw.round() as u8)
                };
                // ★ **混合域的陷阱**（第 948 轮实测）：`[200,200,200,1]`
                //   前三个通道是**字节域**（200）⇒ 走本分支；
                //   而第 4 个 `1` 是**线性域**（用户想表达「不透明」）⇒
                //   若直接按字节解释 ⇒ alpha = 1/255 ⇒ **几乎全透明** ⇒
                //   合成后只剩背景 ⇒ 采样恒为 (255,255,255) ✗
                //   ⇒ 于是 `[200,…]` 与 `[100,…]` **给出同一个结果** ⇒ 外部 §145 的整组测试失效。
                //   ⇒ 修：**RGB 走字节分支、且 a 恰好是 1.0 时，按「不透明」= 255 处理** ✓
                let alpha_raw = numbers.get(3).copied().unwrap_or(255.0);
                let alpha_byte = if alpha_raw == 1.0 { 255 } else { byte(3, 255)? };
                let bytes = [byte(0, 0)?, byte(1, 0)?, byte(2, 0)?, alpha_byte];
                Some(straight_linear_from_bytes(bytes))
            } else {
                let channel = |index: usize, default: f32| {
                    numbers.get(index).copied().unwrap_or(default as f64) as f32
                };
                let color = [
                    channel(0, 0.0),
                    channel(1, 0.0),
                    channel(2, 0.0),
                    channel(3, 1.0),
                ];
                if color
                    .iter()
                    .any(|component| !component.is_finite() || *component < 0.0)
                {
                    return None;
                }
                Some(color)
            }
        }
        Value::Object(map) => {
            // **未知键必须拒绝** ✗（真实报告 ✓，2026-10-03 ✓）：`from:{color:"#FF0000"}` 这种写法
            // 以前会**静默变成黑色** ✓ —— `r`/`g`/`b` 全部取默认 0 ✓、`ok` 还是 `true` ✓
            // ⇒ 调用方拿到一张**全黑**的图却以为成功了 ✗（比报错危险得多 ✓）。
            // 与"参数被静默忽略"是本项目的头号病症 ✓ ⇒ 这里**按名拒绝** ✓，
            // 由调用方给出"可用写法"的错误信息 ✓（三个工具共用这一个解析器 ✓，一处修好全都受益 ✓）。
            for key in map.keys() {
                if !matches!(key.as_str(), "r" | "g" | "b" | "a") {
                    return None;
                }
            }
            let channel = |key: &str, default: u8| -> Option<u8> {
                match map.get(key) {
                    None => Some(default),
                    Some(Value::Number(number)) => {
                        let raw = number.as_f64()?;
                        if !(0.0..=255.0).contains(&raw) {
                            return None;
                        }
                        Some(raw.round() as u8)
                    }
                    Some(_) => None,
                }
            };
            Some(straight_linear_from_bytes([
                channel("r", 0)?,
                channel("g", 0)?,
                channel("b", 0)?,
                channel("a", 255)?,
            ]))
        }
        Value::String(text) => parse_hex_color(text),
        _ => None,
    }
}

/// `#RGB` / `#RRGGBB` / `#RRGGBBAA` → 直通线性。
pub fn parse_hex_color(text: &str) -> Option<LinearRgba> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    let expand = |pair: &str| u8::from_str_radix(pair, 16).ok();
    let bytes = match hex.len() {
        3 => [
            expand(&hex[0..1].repeat(2))?,
            expand(&hex[1..2].repeat(2))?,
            expand(&hex[2..3].repeat(2))?,
            255,
        ],
        6 | 8 => [
            expand(&hex[0..2])?,
            expand(&hex[2..4])?,
            expand(&hex[4..6])?,
            if hex.len() == 8 {
                expand(&hex[6..8])?
            } else {
                255
            },
        ],
        _ => return None,
    };
    Some(straight_linear_from_bytes(bytes))
}

/// sRGB 字节 → 直通线性 RGBA。
pub fn straight_linear_from_bytes(bytes: [u8; 4]) -> LinearRgba {
    let premultiplied = u8x4_to_linear_premul(bytes);
    unpremultiply(premultiplied)
}

/// 颜色描述是否可解析；不可解析时给出给人看的错误说明（5.7 `invalid_argument`）。
pub fn color_error(value: &serde_json::Value) -> Option<String> {
    if parse_spec_color(value).is_some() {
        return None;
    }
    Some(format!(
        "颜色格式非法：{}；支持 [r,g,b,a]（0-1 线性或 0-255 sRGB 字节）、\
         {{\"r\":0-255,\"g\":…,\"b\":…,\"a\":…}} 或 \"#RRGGBB\"",
        compact(value)
    ))
}

fn compact(value: &serde_json::Value) -> String {
    let text = value.to_string();
    if text.len() > 48 {
        format!("{}…", &text[..48])
    } else {
        text
    }
}

#[cfg(test)]
mod spec_color_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn linear_and_byte_forms_are_equivalent() {
        let linear = parse_spec_color(&json!([0.5, 0.25, 0.0, 1.0])).unwrap();
        assert_eq!(linear, [0.5, 0.25, 0.0, 1.0]);

        let bytes = parse_spec_color(&json!([128, 64, 0, 255])).unwrap();
        let object = parse_spec_color(&json!({"r": 128, "g": 64, "b": 0, "a": 255})).unwrap();
        assert_eq!(bytes, object, "字节数组与对象写法必须等价");

        // 历史事故：255 的 alpha 曾被当成线性浮点 → 饱和成白色。
        let green = parse_spec_color(&json!([40, 120, 60, 255])).unwrap();
        assert!((green[3] - 1.0).abs() < 1e-6, "alpha 应为 1：{green:?}");
        assert!(
            green[1] > green[0] && green[1] > green[2],
            "应仍是绿色：{green:?}"
        );
        assert!(green[0] < 0.1 && green[1] < 0.4, "不应饱和：{green:?}");
    }

    #[test]
    fn alpha_defaults_and_hex_forms() {
        assert_eq!(parse_spec_color(&json!([0.0, 0.0, 0.0])).unwrap()[3], 1.0);
        assert_eq!(
            parse_spec_color(&json!({"r": 255, "g": 0, "b": 0})).unwrap()[3],
            1.0
        );
        assert_eq!(
            parse_spec_color(&json!("#ff0000")).unwrap()[0..3],
            parse_spec_color(&json!({"r": 255, "g": 0, "b": 0, "a": 255})).unwrap()[0..3]
        );
        assert_eq!(
            parse_spec_color(&json!("#f00")).unwrap(),
            parse_spec_color(&json!("#ff0000")).unwrap()
        );
        let half = parse_spec_color(&json!("#00000080")).unwrap();
        assert!((half[3] - 128.0 / 255.0).abs() < 1e-3, "{half:?}");
    }

    #[test]
    fn malformed_colors_are_reported() {
        for bad in [
            json!([0.0, 0.0]),
            json!([1.0, 2.0, 3.0, 999.0]),
            json!({"r": -5, "g": 0, "b": 0}),
            json!({"r": "red"}),
            json!("#gg0000"),
            json!("#12345"),
            json!("red"),
            json!(null),
            json!(7),
        ] {
            assert!(parse_spec_color(&bad).is_none(), "{bad} 应被拒绝");
            assert!(color_error(&bad).is_some(), "{bad} 应给出错误说明");
        }
        assert!(color_error(&json!([30, 30, 40, 255])).is_none());
    }

    /// **C1：显式 `color_space` 的三路语义**（变异：把 `Srgb` 分支改成直通线性
    /// ⇒ srgb 臂红；把 `Linear` 分支改成量级切换 ⇒ linear 大分量臂红；
    /// 把 `None` 改成 `Srgb` ⇒ 缺省臂红）。
    #[test]
    fn color_space_explicit_srgb_matches_bytes() {
        // srgb 浮点 [0.45,0.38,0.30] ⇒ 与字节 [115,97,77] **逐位相同**。
        let srgb = parse_spec_color_with_space(&json!([0.45, 0.38, 0.30]), Some(ColorSpace::Srgb))
            .unwrap();
        let bytes = parse_spec_color(&json!([115, 97, 77])).unwrap();
        assert_eq!(
            srgb, bytes,
            "srgb 浮点臂必须与字节臂逐位相同：{srgb:?} vs {bytes:?}"
        );
        // srgb + 已是字节的数组 ⇒ 与不传时同一条路。
        assert_eq!(
            parse_spec_color_with_space(&json!([115, 97, 77]), Some(ColorSpace::Srgb)).unwrap(),
            bytes,
            "srgb + 字节数组 ⇒ 就是字节分支"
        );
        // srgb + 4 元浮点：alpha 1.0 ⇒ 不透明（复用字节分支的特殊情形）。
        let with_alpha =
            parse_spec_color_with_space(&json!([0.45, 0.38, 0.30, 1.0]), Some(ColorSpace::Srgb))
                .unwrap();
        assert!(
            (with_alpha[3] - 1.0).abs() < 1e-6,
            "srgb 4 元、a=1.0 ⇒ 不透明：{with_alpha:?}"
        );
    }

    #[test]
    fn color_space_explicit_linear_passes_through() {
        // linear + 浮点 ⇒ 与不传时相同（量级切换的线性分支）。
        let linear =
            parse_spec_color_with_space(&json!([0.45, 0.38, 0.30]), Some(ColorSpace::Linear))
                .unwrap();
        assert_eq!(
            linear,
            parse_spec_color(&json!([0.45, 0.38, 0.30])).unwrap(),
            "linear + 浮点 ⇒ 就是既有线性分支"
        );
        // linear + 分量 > 1 ⇒ **不 reroute 到字节分支**：原样直通。
        let big =
            parse_spec_color_with_space(&json!([200.0, 100.0, 50.0]), Some(ColorSpace::Linear))
                .unwrap();
        assert_eq!(
            big[0..3],
            [200.0f32, 100.0, 50.0][..],
            "linear 大分量必须直通：{big:?}"
        );
        assert_ne!(
            big,
            parse_spec_color(&json!([200, 100, 50])).unwrap(),
            "linear 大分量绝不能等于字节分支的解码值"
        );
    }

    #[test]
    fn color_space_default_is_magnitude_switch() {
        // 不传 ⇒ 就是 parse_spec_color 本身（缺省行为一位不动）。
        for value in [
            json!([0.45, 0.38, 0.30]),
            json!([115, 97, 77]),
            json!({"r": 115, "g": 97, "b": 77}),
            json!("#73391F"),
        ] {
            assert_eq!(
                parse_spec_color_with_space(&value, None).unwrap(),
                parse_spec_color(&value).unwrap(),
                "不传 color_space ⇒ 与既有解析逐位相同：{value}"
            );
        }
    }

    #[test]
    fn color_space_leaves_object_and_hex_alone() {
        // 对象式 / hex 不受参数影响（恒为字节）。
        for space in [Some(ColorSpace::Srgb), Some(ColorSpace::Linear)] {
            assert_eq!(
                parse_spec_color_with_space(&json!({"r": 115, "g": 97, "b": 77}), space).unwrap(),
                parse_spec_color(&json!({"r": 115, "g": 97, "b": 77})).unwrap(),
                "对象式 + {space:?} ⇒ 仍是字节解释"
            );
            assert_eq!(
                parse_spec_color_with_space(&json!("#73391F"), space).unwrap(),
                parse_spec_color(&json!("#73391F")).unwrap(),
                "hex + {space:?} ⇒ 仍是字节解释"
            );
        }
    }

    #[test]
    fn color_space_rejects_bad_components() {
        // 负分量：srgb（交给字节分支拒）与 linear（线性分支拒）都拒绝。
        assert!(
            parse_spec_color_with_space(&json!([-0.1, 0.5, 0.5]), Some(ColorSpace::Srgb)).is_none(),
            "srgb 负分量应被拒绝"
        );
        assert!(
            parse_spec_color_with_space(&json!([-0.1, 0.5, 0.5]), Some(ColorSpace::Linear))
                .is_none(),
            "linear 负分量应被拒绝"
        );
        assert!(
            parse_spec_color_with_space(&json!([0.0, 0.0]), Some(ColorSpace::Srgb)).is_none(),
            "srgb 2 元应被拒绝"
        );
    }
}

#[cfg(test)]
mod lut_tests {
    use super::*;

    /// 查找表与精确公式的差必须 ≤1 个 8 位台阶（设计 D1：±1 LSB）。
    #[test]
    fn encode_lut_stays_within_one_byte_step() {
        let mut worst = 0i32;
        let mut worst_input = 0.0f32;
        for step in 0..=20_000u32 {
            let value = step as f32 / 20_000.0;
            let delta = (linear_to_byte(value) as i32 - linear_to_byte_exact(value) as i32).abs();
            if delta > worst {
                worst = delta;
                worst_input = value;
            }
        }
        assert!(
            worst <= 1,
            "查找表误差 {worst} 个字节台阶 > 1 LSB（最差输入 {worst_input}）"
        );
    }

    /// 确定性：同一输入必得同一结果（分块/整幅、服务端/内核都依赖这一点）。
    #[test]
    fn encode_lut_is_deterministic() {
        for value in [0.0f32, 0.001, 0.01, 0.2, 0.5, 0.9, 0.999, 1.0] {
            assert_eq!(linear_to_byte(value), linear_to_byte(value));
            assert_eq!(
                linear_to_srgb_fast(value).to_bits(),
                linear_to_srgb_fast(value).to_bits()
            );
        }
    }
}

/// **RGB 字节 → HSV（色相是 0–1 的圆周分数）** ✓ —— 从 `yanshi-server` 搬来的**唯一一处**实现 ✓。
///
/// **为什么必须只有一处** ✓：服务端与浏览器门面各算过一遍 ✓，两份实现必然漂移 ✓（本项目最吃过的亏 ✓）。
/// **约定** ✓（容易踩的坑 ✓，服务端源码里已写明 ✓）：Hokusai 的 `hsv_to_rgb` 第一行是
/// `hsv.h.rem_euclid(1.0) * 6.0` ✓ ⇒ **色相必须是 0–1 的分数** ✗ **不是度数** ✓；
/// 传 60（黄）或 240（蓝）都会被 `rem_euclid(1.0)` 整除成 0 ✓ = **红色** ✗。
/// **口径** ✓：MyPaint / `.myb` 的字段是 **H 用度（0..360）、S/V 用 0..1** ✓；
/// 所以本函数**最后除以 360** ✓ —— **调用方不要再除一次** ✗（第 73 轮就是这么错的 ✓：
/// 门面多除了一次 ✓ ⇒ 蓝被画成红 ✓ ⇒ 15 组判据里恰好只有 blue 三组红 ✗）。
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;
    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if max == gf {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    let sat = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    // **⚠️ 引擎要的是 0..1 的"圆周分数"，不是度数** ✗ —— 这是**查到源码才定下的** ✓：
    // `hokusai-core-0.3.0/src/color.rs` 里 `hsv_to_rgb` 第一行就是 `let h = (hsv.h.rem_euclid(1.0)) * 6.0;`
    // ⇒ 传**度数**（0..360 ✓）会被 `rem_euclid(1.0)` 整除 ✓
    // ⇒ **除不尽的小数部分才是色相** ✓ ⇒ 我传 60（黄 ✓）与 240（蓝 ✓）都得到 **0.0 = 红色** ✗ ✓
    // —— 这正是实测"黄和蓝都画成红"的**确切原因** ✓（不是随机 ✓，是**整除余数** ✓）。
    (hue / 360.0, sat, max)
}

/// **HSL → sRGB（u8）**。
///
/// **口径**：`h` 用**度**（0..360，会自动回绕，360=0）；`s`、`l` 用 0..1 的分数。
/// 与 `rgb_to_hsv` 的"H 用度"口径一致，调用方传人类直觉的度数即可。
pub fn hsl_to_srgb(h_deg: f64, s: f64, l: f64) -> [u8; 3] {
    let h = h_deg.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s <= f64::EPSILON {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return [v, v, v];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue_to_rgb = |t: f64| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [
        (hue_to_rgb(h + 1.0 / 3.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
        (hue_to_rgb(h) * 255.0).round().clamp(0.0, 255.0) as u8,
        (hue_to_rgb(h - 1.0 / 3.0) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8,
    ]
}

/// **线性 sRGB → CIE Lab（D65 白点）**（测试报告 §三.2 ✓）。
///
/// **为什么需要它** ✓：`analyze_region` 现在只描述"画面是什么" ✓（直方图/亮度/冷暖/主色 ✓），
/// **没有"和参考图比差多少"** ✗ ⇒ 报告说的"AI 纯盲调参数"就来自这里 ✓。
/// ΔE 必须在**感知均匀**的空间里算 ✓ ⇒ 先把线性 RGB 变成 Lab ✓（线性值是本仓库的通用表示 ✓）。
pub fn linear_to_lab(linear: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = linear;
    // sRGB 基色 + D65：RGB → XYZ ✓
    let x = 0.4124 * r + 0.3576 * g + 0.1805 * b;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = 0.0193 * r + 0.1192 * g + 0.9505 * b;
    // D65 白点 ✓
    let (xn, yn, zn) = (0.95047_f32, 1.0_f32, 1.08883_f32);
    let f = |t: f32| {
        if t > 0.008_856 {
            libm::cbrtf(t)
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / xn), f(y / yn), f(z / zn));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// **CIE Lab → 线性 sRGB（D65 白点）**：`linear_to_lab` 的逆 ✓。
///
/// **为什么需要它**：`curves` 的 `"l"` 通道（Lab 明度曲线）要逐像素"转 Lab→套曲线→转回" ✓，
/// 没有逆变换就只能单向分析、不能调色 ✗。
pub fn lab_to_linear(lab: [f32; 3]) -> [f32; 3] {
    let (l, a, b) = (lab[0], lab[1], lab[2]);
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (a / 500.0 + fy, fy - b / 200.0);
    let finv = |t: f32| {
        let t3 = t * t * t;
        if t3 > 0.008_856 {
            t3
        } else {
            (t - 16.0 / 116.0) / 7.787
        }
    };
    // D65 白点 ✓（与 linear_to_lab 同一组 ✓）。
    let (x, y, z) = (
        0.95047_f32 * finv(fx),
        1.0_f32 * finv(fy),
        1.08883_f32 * finv(fz),
    );
    // XYZ → 线性 sRGB（linear_to_lab 里那个矩阵的逆 ✓）。
    [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ]
}
/// **CIE76 色差**（§三.2 ✓）：Lab 空间里的欧氏距离 ✓。**同一个颜色恒为 0** ✓（判据的一侧靠它 ✓）。
pub fn delta_e_cie76(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (dl, da, db) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    (dl * dl + da * da + db * db).sqrt()
}

/// 便捷入口：直接比较两个**线性 RGB** 的色差 ✓（调用方不必自己转 Lab ✓）。
pub fn delta_e_linear_rgb(a: [f32; 3], b: [f32; 3]) -> f32 {
    delta_e_cie76(linear_to_lab(a), linear_to_lab(b))
}

#[cfg(test)]
mod lab_delta_e_tests {
    use super::{delta_e_cie76, delta_e_linear_rgb, lab_to_linear, linear_to_lab};

    /// **Lab 往返恒等** ✓ —— `lab_to_linear` 是 `linear_to_lab` 的逆 ✓。
    /// 这是 curves "l" 通道的正确性根基（转 Lab→套曲线→转回不能漂移 ✓）。
    #[test]
    fn lab_roundtrip_is_identity() {
        for color in [
            [0.2, 0.5, 0.8],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.18, 0.18, 0.18],
        ] {
            let back = lab_to_linear(linear_to_lab(color));
            for channel in 0..3 {
                assert!(
                    (back[channel] - color[channel]).abs() < 1e-4,
                    "{color:?} 往返：{back:?}"
                );
            }
        }
    }

    /// **同一颜色 ΔE = 0** ✓ —— 这是判据"自比必须≈0"那一侧的依据 ✓。
    #[test]
    fn identical_colours_have_zero_difference() {
        let lab = linear_to_lab([0.2, 0.5, 0.8]);
        assert!(delta_e_cie76(lab, lab).abs() < 1e-6);
        assert!(delta_e_linear_rgb([0.2, 0.5, 0.8], [0.2, 0.5, 0.8]).abs() < 1e-6);
    }

    /// **黑白的 ΔE ≈ 100** ✓ —— 已知量级 ⇒ 说明亮度轴落在**规范**尺度上 ✓（不是自造单位 ✓）。
    #[test]
    fn black_and_white_differ_by_about_one_hundred() {
        let d = delta_e_linear_rgb([0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
        assert!((d - 100.0).abs() < 1.0, "黑白 ΔE 应为 ~100，实得 {d}");
    }

    /// **微小变化 ⇒ 小但非零** ✓ —— 判据另一侧"改色后必须 > 阈值"要的就是这个 ✓。
    #[test]
    fn a_small_change_is_small_but_not_zero() {
        let d = delta_e_linear_rgb([0.5, 0.5, 0.5], [0.6, 0.5, 0.5]);
        assert!(d > 0.5 && d < 30.0, "小幅变亮的 ΔE 应在小量级，实得 {d}");
    }
}

/// **★ 颜色混合模式（P2 #16）★**（第 254 轮）
///
/// 需求要 `color_mixing: "rgb"|"pigment"`，**默认 rgb**；`rgb` 必须与今天**逐字节相同**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMixing {
    /// sRGB 空间线性插值 —— **缺省**，与今天的画布逐字节相同。
    Rgb,
    /// **线性光混色**（sRGB → linear → 混合 → sRGB）：**物理正确**的混色。
    ///
    /// **为什么需要它** ✗（色彩体系规划 **Phase 1**）：`Rgb` 是**在 sRGB 空间**做插值
    /// ⇒ 而 sRGB 是**感知编码**（有 gamma）⇒ 于是「红 + 蓝各 50%」会得到**偏暗**的结果 ✗
    /// **实测**（第 1290 轮）：红 `(0.8,0.1,0.1)` + 蓝 `(0.1,0.1,0.8)` 各 50%：
    ///   · sRGB 空间 ⇒ `(0.450, 0.100, 0.450)`
    ///   · linear 空间 ⇒ `(0.590, 0.100, 0.590)`
    ///   · **差 35 级（8-bit）** = 13.7% 满量程 ⇒ **视觉上很明显**
    /// ⇒ 所以线性混合是**另一个模式**（不是替换 `Rgb`）⇒ `Rgb` 保持「与今天逐字节相同」的承诺 ✓
    Linear,
    /// **光谱颜料混色**（Kubelka-Munk 的 K/S 空间）：黄 + 蓝 => **绿**，而不是灰。
    Pigment,
}

impl ColorMixing {
    /// 解析工具参数里的名字（不认识就返回 `None`，由调用方报 `invalid_argument`）。
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "rgb" => Some(Self::Rgb),
            "linear" => Some(Self::Linear),
            "pigment" => Some(Self::Pigment),
            _ => None,
        }
    }

    /// 规范名（回执里原样回给调用方）。
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Rgb => "rgb",
            Self::Linear => "linear",
            Self::Pigment => "pigment",
        }
    }
}

/// 按模式混合两个 sRGB 字节色：`t=0` 返回 `a`、`t=1` 返回 `b`。
///
/// **`Rgb`**：与仓库既有的 `lerp_srgb` 同一口径（逐通道线性插值、四舍五入）。
/// **`Pigment`**：交给 MIT 的 `neco-pigment`（41 采样光谱 ＋ K/S 空间混合；纯函数、无随机）。
/// *为什么用库而不是自实现*：本会话实测过"**逐通道** K-M 自实现"——黄+蓝会得到 `[2,2,2]`
/// （近黑、无偏色），因为逐通道模型无法表达真实颜料那种**宽而平滑**的光谱反射。
/// 详见 `docs/design/pigment-mixing.md`（含两面评估与供应链风险）。
#[must_use]
pub fn mix_colors(mode: ColorMixing, a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return a;
    }
    if t >= 1.0 {
        return b;
    }
    match mode {
        ColorMixing::Rgb => {
            let mix = |x: u8, y: u8| {
                (f32::from(x) + (f32::from(y) - f32::from(x)) * t)
                    .round()
                    .clamp(0.0, 255.0) as u8
            };
            [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
        }
        ColorMixing::Linear => {
            // **sRGB 字节 ⇒ linear ⇒ 插值 ⇒ 回 sRGB** ✓（物理正确的混色）
            //   ⇒ 「红 + 蓝各 50%」给出 `(0.590, 0.100, 0.590)`（而不是 sRGB 空间的 `(0.450, …, 0.450)`）
            let to_lin = |c: u8| srgb_to_linear(f32::from(c) / 255.0);
            let mix = |x: u8, y: u8| {
                let lx = to_lin(x);
                let ly = to_lin(y);
                let l = lx + (ly - lx) * t;
                (linear_to_srgb(l).clamp(0.0, 1.0) * 255.0).round() as u8
            };
            [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
        }
        ColorMixing::Pigment => {
            // sRGB 字节 => [0,1] 线性（库自己的口径），混合后再编回字节。
            let to_unit = |c: u8| f32::from(c) / 255.0;
            let (from, to) = (
                [to_unit(a[0]), to_unit(a[1]), to_unit(a[2])],
                [to_unit(b[0]), to_unit(b[1]), to_unit(b[2])],
            );
            let (Ok(from_ks), Ok(to_ks)) = (
                neco_pigment::rgb_to_ks(from[0], from[1], from[2]),
                neco_pigment::rgb_to_ks(to[0], to[1], to[2]),
            ) else {
                // 极端输入（NaN 等）=> 退回 rgb 口径，绝不 panic
                return mix_colors(ColorMixing::Rgb, a, b, t);
            };
            let mixed = neco_pigment::ks_mix(&from_ks, &to_ks, t);
            let rgb = neco_pigment::ks_to_srgb(&mixed, neco_pigment::illuminant_d65());
            let to_byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            [to_byte(rgb[0]), to_byte(rgb[1]), to_byte(rgb[2])]
        }
    }
}

/// **★ P2 #16 的决定性判据（数据层）★**（第 254 轮）
///
/// **要证的命题**：**颜料模式给"绿"，RGB 模式给"灰"**。
/// 判据对 `ColorMixing::Rgb` **天生为红**（灰的三分量必然几乎相等）=> 有鉴别力。
#[cfg(test)]
mod pigment_mixing_tests {
    use super::{mix_colors, ColorMixing};

    fn spread(rgb: [u8; 3]) -> i32 {
        i32::from(*rgb.iter().max().expect("非空")) - i32::from(*rgb.iter().min().expect("非空"))
    }

    #[test]
    fn pigment_mode_makes_green_where_rgb_mode_makes_grey() {
        let yellow = [255, 255, 0];
        let blue = [0, 0, 255];
        // 对照：rgb 模式必然是灰
        let rgb = mix_colors(ColorMixing::Rgb, yellow, blue, 0.5);
        assert!(spread(rgb) <= 2, "rgb 模式必须是灰：{rgb:?}");
        // 颜料模式：明显偏绿 + 明显有色
        let pigment = mix_colors(ColorMixing::Pigment, yellow, blue, 0.5);
        assert!(
            i32::from(pigment[1]) > i32::from(pigment[0]) + 30,
            "颜料模式必须明显偏绿（G 远高于 R）：{pigment:?}"
        );
        assert!(
            spread(pigment) > 40,
            "颜料模式必须**有色**（不是灰）：{pigment:?}；rgb 模式给 {rgb:?}"
        );
        // 端点不漂
        assert_eq!(mix_colors(ColorMixing::Pigment, yellow, blue, 0.0), yellow);
        assert_eq!(mix_colors(ColorMixing::Pigment, yellow, blue, 1.0), blue);
        // 确定性
        assert_eq!(
            mix_colors(ColorMixing::Pigment, yellow, blue, 0.5),
            pigment,
            "同输入必须逐位可复现"
        );
        // 名字解析
        assert_eq!(ColorMixing::parse("Pigment"), Some(ColorMixing::Pigment));
        assert_eq!(ColorMixing::parse("rgb"), Some(ColorMixing::Rgb));
        assert_eq!(ColorMixing::parse("mixbox"), None);
        assert_eq!(ColorMixing::Pigment.name(), "pigment");
    }
}

// **★ 介质渐变用的那对颜色：`pigment` 与 `rgb` 必须不同 ★**（第 254 轮）
//
// 背景：端到端判据 `tool-medium-color-mixing.mjs` 报"pigment 与 rgb **逐字节相同**" ✗。
// 库本身有测试（黄+蓝差别很大 ✓），但**判据用的那对颜色**（低饱和的暖黄 → 冷蓝）没被钉过
// => 这里把它钉住：如果这条红了，说明**这对颜色**在光谱路径上退化了（例如 K-S 反解失败后回落 rgb）。
#[cfg(test)]
mod medium_gradient_mixing_tests {
    use super::{mix_colors, ColorMixing};

    #[test]
    fn the_medium_gradient_pair_differs_between_pigment_and_rgb() {
        // 与 `scripts/tool-medium-color-mixing.mjs` 用的是同一对颜色
        let warm = [240_u8, 210, 60];
        let cool = [60_u8, 90, 210];
        let rgb = mix_colors(ColorMixing::Rgb, warm, cool, 0.5);
        let pigment = mix_colors(ColorMixing::Pigment, warm, cool, 0.5);
        assert_ne!(
            rgb, pigment,
            "中段颜色：pigment 与 rgb 不该相同（rgb={rgb:?} / pigment={pigment:?}）\
             —— 若相同，说明光谱路径在这对颜色上退化了（回落 rgb）"
        );
    }
}

#[cfg(test)]
mod mixed_domain_probe {
    use super::parse_spec_color;
    use serde_json::json;

    // ★ 字节数组的**契约**（第 954 轮探针转成断言）：
    //   a=1 ⇒ 不透明；200 与 100 必须反解成不同的线性值（同值 = 幅度被吃）。
    //   注意：这条测不出「下游那处 alpha 修复」的变异 —— 那个修复影响不在这层。
    #[test]
    fn byte_array_alpha_and_magnitude_are_sane() {
        let a = parse_spec_color(&json!([200, 200, 200, 1])).unwrap();
        let b = parse_spec_color(&json!([100, 100, 100, 1])).unwrap();
        let c = parse_spec_color(&json!([40, 120, 60, 255])).unwrap();
        // 字节数组：alpha=1 应视为**不透明**
        assert_eq!(a[3], 1.0, "a=1 在字节数组里必须是不透明");
        // 200 与 100 必须反解成**不同**的线性值（同值 = alpha/幅度被吃了）
        assert!(
            (a[0] - b[0]).abs() > 0.1,
            "200 与 100 必须不同：{:?} vs {:?}",
            a[0],
            b[0]
        );
        assert!(
            (a[0] - 0.5776).abs() < 0.02,
            "字节 200 反解 ≈ 0.5776，实测 {}",
            a[0]
        );
        assert!(
            (b[0] - 0.1274).abs() < 0.02,
            "字节 100 反解 ≈ 0.1274，实测 {}",
            b[0]
        );
        assert_eq!(c[3], 1.0, "a=255 必须不透明");
    }
}

#[cfg(test)]
mod linear_mixing_tests {
    //! **`ColorMixing::Linear` 的判据**（第 1291 轮；色彩体系规划 Phase 1）
    //!
    //! **要证的命题**：线性混色与 sRGB 混色**必须不同**，且**差 35 级左右**。
    //! ⇒ 所以这条断言**有鉴别力**（若 `Linear` 被误实现成 `Rgb`，它会红）。

    use super::{mix_colors, ColorMixing};

    /// **红 + 蓝各 50%**：线性空间给出更亮的结果（`(0.590, …, 0.590)` vs `(0.450, …, 0.450)`）。
    ///
    /// **输入选这两个颜色的理由**：它们在 sRGB 空间的亮度差很大 ⇒ 线性化后差异最大 ⇒
    ///   判据**最敏感**（若选两个灰，线性与 sRGB 几乎相同 ⇒ 判据无鉴别力）。
    #[test]
    fn linear_mixing_differs_from_srgb_mixing_by_about_35_levels() {
        let red = [204u8, 25, 25]; // ≈ (0.8, 0.1, 0.1)
        let blue = [25u8, 25, 204]; // ≈ (0.1, 0.1, 0.8)

        let srgb = mix_colors(ColorMixing::Rgb, red, blue, 0.5);
        let linear = mix_colors(ColorMixing::Linear, red, blue, 0.5);

        // **线性必须更亮**（红/蓝分量）—— 这就是「物理正确」的方向
        assert!(
            linear[0] > srgb[0],
            "线性混色的红分量必须大于 sRGB：linear={linear:?} srgb={srgb:?}"
        );
        assert!(
            linear[2] > srgb[2],
            "线性混色的蓝分量必须大于 sRGB：linear={linear:?} srgb={srgb:?}"
        );

        // **量级**：预期差 ~35 级（8-bit）⇒ 允许 ±6（不同取整/输入微差）
        let d = i32::from(linear[0]) - i32::from(srgb[0]);
        assert!(
            (29..=41).contains(&d),
            "线性与 sRGB 的红分量差应为 35±6 级（实测 {d}）"
        );
    }

    /// **端点必须精确**：`t=0` 给 `a`，`t=1` 给 `b`（两种模式都是 ⇒ 这条守住「共同的契约」）。
    #[test]
    fn linear_mixing_endpoints_are_exact() {
        let a = [10u8, 200, 30];
        let b = [250u8, 5, 240];
        assert_eq!(mix_colors(ColorMixing::Linear, a, b, 0.0), a);
        assert_eq!(mix_colors(ColorMixing::Linear, a, b, 1.0), b);
    }

    /// **与自身混合必须恒等**（`a == b` 时任何 `t` 都给 `a`）⇒ 线性往返不引入漂移。
    #[test]
    fn linear_mixing_self_is_identity() {
        let a = [137u8, 88, 201];
        for t in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(mix_colors(ColorMixing::Linear, a, a, t), a, "t={t}");
        }
    }

    /// **`parse` / `name` 往返**（`"linear"` ⇒ `Linear` ⇒ `"linear"`）。
    #[test]
    fn linear_name_round_trips() {
        assert_eq!(ColorMixing::parse("linear"), Some(ColorMixing::Linear));
        assert_eq!(ColorMixing::Linear.name(), "linear");
    }
}
