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

/// 字节 → 线性光分量。
pub fn byte_to_linear(byte: u8) -> f32 {
    srgb_to_linear(byte as f32 / 255.0)
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
const SRGB_ENCODE_LUT_SIZE: usize = 4097;

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
                let bytes = [byte(0, 0)?, byte(1, 0)?, byte(2, 0)?, byte(3, 255)?];
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
    use super::{delta_e_cie76, delta_e_linear_rgb, linear_to_lab};

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
