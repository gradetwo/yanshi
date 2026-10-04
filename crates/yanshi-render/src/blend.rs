//! 混合模式与合成（设计文档 6.1）。
//!
//! 采用 W3C Compositing and Blending Level 1 的公式，在**线性光空间**、**预乘 alpha**
//! 表示下计算：
//!
//! ```text
//! co = (1 - αb)·αs·Cs + (1 - αs)·αb·Cb + αs·αb·B(Cb, Cs)
//! αo = αs + αb·(1 - αs)
//! ```
//!
//! 其中 `Cs`、`Cb` 为直通（非预乘）分量，`B` 为混合函数；`co` 已是预乘结果。

use crate::color::{unpremultiply, LinearRgba};

/// 混合模式。MVP 提供通用光栅合成所需的子集（设计文档 4.1 `blend_mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BlendMode {
    /// 正常（source-over）。
    Normal,
    /// 正片叠底。
    Multiply,
    /// 滤色。
    Screen,
    /// 叠加。
    Overlay,
    /// 变暗。
    Darken,
    /// 变亮。
    Lighten,
    /// 线性减淡（相加）。
    Add,
    /// 相减。
    Subtract,
    /// 差值。
    Difference,
    /// **颜色减淡**（测试报告 §三.4 ✓）：提亮高光刀痕用 ✓，规范公式 `Cb / (1 - Cs)` ✓。
    ColorDodge,
}

impl BlendMode {
    /// 全部模式。
    pub const ALL: [BlendMode; 9] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Add,
        Self::Subtract,
        Self::Difference,
    ];

    /// **受支持的全部混合模式名** ✓（真实用户报告里"非法值被静默写入" ✓ ⇒ 校验需要一份权威清单 ✓）。
    ///
    /// **为什么集中在这里** ✓：渲染层与工具层**必须用同一份** ✗ ——
    /// 各写一份必然漂移 ✓ ⇒ 于是"工具说合法、渲染不认"或反过来 ✗。
    pub const NAMES: [&'static str; 11] = [
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "add",
        "linear_dodge",
        "subtract",
        "color_dodge",
        "difference",
    ];

    /// **严格解析** ✓：不认识就返回 `None` ✓（供**入口**做校验 ✓）。
    ///
    /// **为什么要与 `from_name` 分开** ✓：两者面向**不同的人** ✗ ——
    /// **渲染层**面对的是**已有的文档** ✓ ⇒ 必须**宽容** ✓（老文档里万一有怪值 ✓ 也要能画出来 ✓）；
    /// **工具层**面对的是**正要写进去的值** ✓ ⇒ 必须**严格** ✓（拒绝并说明可用值 ✓）。
    /// 混成一个函数就必然牺牲一边 ✗。
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "overlay" => Some(Self::Overlay),
            "darken" => Some(Self::Darken),
            "lighten" => Some(Self::Lighten),
            "add" | "linear_dodge" => Some(Self::Add),
            "subtract" => Some(Self::Subtract),
            "difference" => Some(Self::Difference),
            "color_dodge" => Some(Self::ColorDodge),
            _ => None,
        }
    }

    /// 由文档中的 `blend_mode` 字符串解析（未知值退化为 [`BlendMode::Normal`] ✓ —— **渲染层的宽容** ✓）。
    pub fn from_name(name: &str) -> Self {
        Self::parse(name).unwrap_or(Self::Normal)
    }

    /// 文档中使用的字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Difference => "difference",
            Self::ColorDodge => "color_dodge",
        }
    }

    /// 单通道混合函数 `B(Cb, Cs)`（直通分量，输入输出均在 `[0, 1]`）。
    pub fn blend_channel(self, base: f32, source: f32) -> f32 {
        let cb = base.clamp(0.0, 1.0);
        let cs = source.clamp(0.0, 1.0);
        let out = match self {
            Self::Normal => cs,
            Self::Multiply => cb * cs,
            Self::Screen => cb + cs - cb * cs,
            // **颜色减淡**（§三.4）✓：`Cb / (1 - Cs)` ✓；`Cs == 1` 时按规范取 1 ✓（避免除零 ✓）。
            Self::ColorDodge => {
                if cs >= 1.0 {
                    1.0
                } else {
                    (cb / (1.0 - cs)).clamp(0.0, 1.0)
                }
            }
            Self::Overlay => {
                // HardLight(Cs, Cb)：以 base 决定分支。
                if cb <= 0.5 {
                    2.0 * cb * cs
                } else {
                    1.0 - 2.0 * (1.0 - cb) * (1.0 - cs)
                }
            }
            Self::Darken => cb.min(cs),
            Self::Lighten => cb.max(cs),
            Self::Add => (cb + cs).min(1.0),
            Self::Subtract => (cb - cs).max(0.0),
            Self::Difference => (cb - cs).abs(),
        };
        out.clamp(0.0, 1.0)
    }
}

/// 按混合模式把 `source` 合成到 `backdrop` 上（两者均为线性光预乘像素）。
pub fn blend_pixel(mode: BlendMode, source: LinearRgba, backdrop: LinearRgba) -> LinearRgba {
    if mode == BlendMode::Normal {
        // Normal 在预乘表示下无需反预乘，直接叠加（热路径）。
        return over(source, backdrop);
    }
    let alpha_s = source[3].clamp(0.0, 1.0);
    let alpha_b = backdrop[3].clamp(0.0, 1.0);
    if alpha_s <= 0.0 {
        return backdrop;
    }
    if alpha_b <= 0.0 {
        return source;
    }
    let cs = unpremultiply(source);
    let cb = unpremultiply(backdrop);
    let mut out = [0.0f32; 4];
    for channel in 0..3 {
        let blended = mode.blend_channel(cb[channel], cs[channel]);
        out[channel] = (1.0 - alpha_b) * alpha_s * cs[channel]
            + (1.0 - alpha_s) * alpha_b * cb[channel]
            + alpha_s * alpha_b * blended;
    }
    out[3] = alpha_s + alpha_b * (1.0 - alpha_s);
    out
}

/// source-over 合成（等价于 [`BlendMode::Normal`]）。
///
/// 在预乘表示下就是 `co = Ps + Pb·(1 - αs)`、`αo = αs + αb·(1 - αs)`，
/// 与 W3C 公式在实数上等价，且避免了两次反预乘除法（渲染热路径）。
pub fn over(source: LinearRgba, backdrop: LinearRgba) -> LinearRgba {
    let alpha_s = source[3];
    if alpha_s <= 0.0 {
        return backdrop;
    }
    if alpha_s >= 1.0 {
        return source;
    }
    let inverse = 1.0 - alpha_s;
    [
        source[0] + backdrop[0] * inverse,
        source[1] + backdrop[1] * inverse,
        source[2] + backdrop[2] * inverse,
        alpha_s + backdrop[3] * inverse,
    ]
}

/// 按不透明度缩放预乘像素（alpha 与颜色同时缩放）。
pub fn scale_alpha(pixel: LinearRgba, opacity: f32) -> LinearRgba {
    let factor = opacity.clamp(0.0, 1.0);
    [
        pixel[0] * factor,
        pixel[1] * factor,
        pixel[2] * factor,
        pixel[3] * factor,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(r: f32, g: f32, b: f32, a: f32) -> LinearRgba {
        [r, g, b, a]
    }

    #[test]
    fn normal_over_matches_porter_duff() {
        let backdrop = px(0.2, 0.0, 0.0, 0.5);
        let source = px(0.0, 0.4, 0.0, 0.5);
        let out = over(source, backdrop);
        assert!((out[3] - 0.75).abs() < 1e-6);
        // 预乘结果：co = Cs·αs + Cb_premul·(1 - αs)
        assert!((out[1] - 0.4).abs() < 1e-6, "source 绿通道贡献 0.8·0.5");
        assert!(
            (out[0] - 0.1).abs() < 1e-6,
            "backdrop 红通道贡献 0.2·(1-0.5)"
        );
    }

    #[test]
    fn opaque_source_replaces_backdrop_in_every_mode() {
        let backdrop = px(1.0, 0.0, 0.0, 1.0);
        let source = px(0.25, 0.5, 0.75, 1.0);
        for mode in BlendMode::ALL {
            let out = blend_pixel(mode, source, backdrop);
            assert!((out[3] - 1.0).abs() < 1e-6);
        }
        // Normal 模式整体替换。
        let out = over(source, backdrop);
        for channel in 0..3 {
            assert!((out[channel] - source[channel]).abs() < 1e-6);
        }
    }

    #[test]
    fn transparent_source_is_identity() {
        let backdrop = px(0.3, 0.3, 0.3, 0.4);
        for mode in BlendMode::ALL {
            assert_eq!(blend_pixel(mode, [0.0; 4], backdrop), backdrop);
        }
        let source = px(1.0, 1.0, 1.0, 0.0);
        assert_eq!(over(source, backdrop), backdrop);
    }

    #[test]
    fn transparent_backdrop_returns_source() {
        let source = px(0.1, 0.2, 0.3, 0.5);
        for mode in BlendMode::ALL {
            assert_eq!(blend_pixel(mode, source, [0.0; 4]), source);
        }
    }

    #[test]
    fn multiply_and_screen_have_expected_identities() {
        let white = px(1.0, 1.0, 1.0, 1.0);
        let black = px(0.0, 0.0, 0.0, 1.0);
        let color = px(0.4, 0.6, 0.8, 1.0);
        let multiplied = blend_pixel(BlendMode::Multiply, white, color);
        for channel in 0..3 {
            assert!((multiplied[channel] - color[channel]).abs() < 1e-6);
        }
        let screened = blend_pixel(BlendMode::Screen, black, color);
        for channel in 0..3 {
            assert!((screened[channel] - color[channel]).abs() < 1e-6);
        }
    }

    #[test]
    fn premultiplied_invariants_hold() {
        let backdrop = px(0.5, 0.25, 0.125, 0.8);
        let source = px(0.3, 0.3, 0.3, 0.6);
        for mode in BlendMode::ALL {
            let out = blend_pixel(mode, source, backdrop);
            assert!((0.0..=1.0).contains(&out[3]), "{mode:?} alpha={}", out[3]);
            for channel in 0..3 {
                assert!(
                    out[channel] <= out[3] + 1e-5,
                    "{mode:?} 预乘分量 {} > alpha {}",
                    out[channel],
                    out[3]
                );
                assert!(out[channel] >= -1e-6);
            }
        }
    }

    #[test]
    fn mode_names_round_trip() {
        for mode in BlendMode::ALL {
            assert_eq!(BlendMode::from_name(mode.as_str()), mode);
        }
        assert_eq!(BlendMode::from_name("unknown"), BlendMode::Normal);
    }

    #[test]
    fn alpha_scaling_is_linear() {
        let pixel = px(0.4, 0.4, 0.4, 0.8);
        assert_eq!(scale_alpha(pixel, 0.0), [0.0; 4]);
        assert_eq!(scale_alpha(pixel, 1.0), pixel);
        let half = scale_alpha(pixel, 0.5);
        assert!((half[3] - 0.4).abs() < 1e-6);
        assert!((half[0] - 0.2).abs() < 1e-6);
    }
}

#[cfg(test)]
mod mode_name_tests {
    use super::*;

    /// **`from_name` 的清单必须与枚举一样全** ✓ —— 真实用户报告引出的**真 bug** ✓。
    ///
    /// **症状** ✓：`from_name` 的匹配里**漏了 `"difference"`** ✗
    /// ⇒ 设 `blend_mode: "difference"` 会**静默退化成 `normal`** ✗
    /// ⇒ 调用方看到写成功了 ✓、画面却**毫无变化** ✗ —— 正是"**接受了却没用**" ✓。
    /// **本测试覆盖全部 `as_str()` 出来的名字** ✓ ⇒ 以后再加模式而忘了加匹配 ✓ 这里会红 ✓。
    #[test]
    fn every_mode_name_round_trips_through_from_name() {
        for name in BlendMode::NAMES {
            let parsed = BlendMode::parse(name);
            assert!(parsed.is_some(), "{name} 应当是受支持的模式名");
            let mode = parsed.unwrap();
            assert_eq!(
                mode.as_str(),
                // `linear_dodge` 是 `add` 的别名 ✓ ⇒ 归一到规范名 ✓
                if name == "linear_dodge" { "add" } else { name },
                "{name} 解析后应当归一化正确"
            );
            assert_eq!(
                BlendMode::from_name(name),
                mode,
                "{name} 经 from_name 不该退化（这正是 difference 当初栽的地方 ✗）"
            );
        }
    }

    /// **`difference` 曾经被静默降级** ✓ —— 单独钉一条 ✓（回归测试 ✓）。
    #[test]
    fn difference_is_not_silently_downgraded() {
        assert_eq!(BlendMode::from_name("difference"), BlendMode::Difference);
        assert_ne!(BlendMode::from_name("difference"), BlendMode::Normal);
        // **未知值仍然宽容** ✓（渲染层面对老文档 ✓ 必须能画出来 ✓）⇒ 这条是**有意**的行为 ✓。
        assert_eq!(BlendMode::from_name("not_a_mode"), BlendMode::Normal);
        assert!(
            BlendMode::parse("not_a_mode").is_none(),
            "严格解析应当拒绝未知值 ✓（入口用它校验 ✓）"
        );
    }
}

#[cfg(test)]
mod color_dodge_tests {
    use super::BlendMode;

    /// **颜色减淡**（测试报告 §三.4）✓：`Cb / (1 - Cs)` ✓，`Cs = 1` 取 1 ✓，且**绝不越界** ✓。
    #[test]
    fn color_dodge_follows_the_spec_and_stays_in_range() {
        let dodge = BlendMode::ColorDodge;
        assert!((dodge.blend_channel(0.25, 0.5) - 0.5).abs() < 1e-6);
        assert!((dodge.blend_channel(0.0, 0.5) - 0.0).abs() < 1e-6);
        assert!((dodge.blend_channel(0.25, 1.0) - 1.0).abs() < 1e-6); // 规范：Cs=1 ⇒ 1 ✓
        assert!(dodge.blend_channel(0.9, 0.95) <= 1.0); // 不越界 ✓
    }

    /// **清单与解析必须一致** ✓：`NAMES` 里每一个都能解析 ✓、且解析回来名字相同 ✓
    ///（这条守的正是"渲染层与工具层各写一份必然漂移" ✗）。
    #[test]
    fn the_name_list_round_trips_through_parsing() {
        for name in BlendMode::NAMES {
            let parsed = BlendMode::parse(name).unwrap_or_else(|| panic!("{name} 解析不了"));
            assert_eq!(parsed.as_str(), name);
        }
    }
}
