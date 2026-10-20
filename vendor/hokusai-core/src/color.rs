//! Color types. Internal pipeline is linear sRGB; HSV is used at brush
//! configuration boundaries (matches libmypaint).

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RgbaF32 {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl RgbaF32 {
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hsv {
    pub h: f32, // [0, 1)
    pub s: f32, // [0, 1]
    pub v: f32, // [0, 1]
}

// sRGB transfer fns. libmypaint uses the standard piecewise IEC 61966-2-1.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        libm::powf((c + 0.055) / 1.055, 2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * libm::powf(c, 1.0 / 2.4) - 0.055
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hsl {
    pub h: f32, // [0, 1)
    pub s: f32, // [0, 1]
    pub l: f32, // [0, 1]
}

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> Hsv {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max <= 0.0 { 0.0 } else { d / max };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    Hsv { h, s, v }
}

pub fn rgb_to_hsl(r: f32, g: f32, b: f32) -> Hsl {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    let d = max - min;
    let s = if d <= 0.0 {
        0.0
    } else if l < 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    Hsl { h, s, l }
}

pub fn hsl_to_rgb(hsl: Hsl) -> RgbaF32 {
    if hsl.s <= 0.0 {
        return RgbaF32::new(hsl.l, hsl.l, hsl.l, 1.0);
    }
    let q = if hsl.l < 0.5 {
        hsl.l * (1.0 + hsl.s)
    } else {
        hsl.l + hsl.s - hsl.l * hsl.s
    };
    let p = 2.0 * hsl.l - q;
    let h = hsl.h.rem_euclid(1.0);
    let to_rgb = |t: f32| {
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
    RgbaF32::new(to_rgb(h + 1.0 / 3.0), to_rgb(h), to_rgb(h - 1.0 / 3.0), 1.0)
}

pub fn hsv_to_rgb(hsv: Hsv) -> RgbaF32 {
    let h = (hsv.h.rem_euclid(1.0)) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let p = hsv.v * (1.0 - hsv.s);
    let q = hsv.v * (1.0 - hsv.s * f);
    let t = hsv.v * (1.0 - hsv.s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (hsv.v, t, p),
        1 => (q, hsv.v, p),
        2 => (p, hsv.v, t),
        3 => (p, q, hsv.v),
        4 => (t, p, hsv.v),
        _ => (hsv.v, p, q),
    };
    RgbaF32::new(r, g, b, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn hsv_roundtrip() {
        for &(r, g, b) in &[
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.3, 0.6, 0.9),
            (0.5, 0.5, 0.5),
        ] {
            let back = hsv_to_rgb(rgb_to_hsv(r, g, b));
            assert!(
                approx(back.r, r) && approx(back.g, g) && approx(back.b, b),
                "HSV roundtrip lost ({r},{g},{b}) → ({},{},{})",
                back.r,
                back.g,
                back.b
            );
        }
    }

    #[test]
    fn hsl_roundtrip() {
        for &(r, g, b) in &[
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.5),
            (0.2, 0.4, 0.7),
            (0.5, 0.5, 0.5),
            (0.0, 0.0, 0.0),
        ] {
            let back = hsl_to_rgb(rgb_to_hsl(r, g, b));
            assert!(
                approx(back.r, r) && approx(back.g, g) && approx(back.b, b),
                "HSL roundtrip lost ({r},{g},{b}) → ({},{},{})",
                back.r,
                back.g,
                back.b
            );
        }
    }
}

/// **加权混合（涂抹）在**线性空间**做** ✓（Phase 1，第 1306 轮）
///
/// **⇒ 为什么必须在这个空间** ✗：`smudge*amt + brush*(1-amt)` 是**光的相加** ⇒
///   而加权和**只在 linear 上有物理意义**（sRGB 是感知编码，有 gamma）⇒
///   在 sRGB 上做会让**中间调偏暗** ✗
///
/// **⇒ 接口为什么仍是 sRGB** ✓：入参、出参都是 **sRGB** ⇒ 所以它**不改** `MemSurface` 的语义
///   ⇒ 于是**不需要**新通道，也不会与 `blit_rgba8` 的解码叠加成双解码 ✓
///
/// **⇒ 与「把 `base` 转 linear」的区别**（我上一轮的错误做法）：
///   那种做法让**整条下游**都变 linear ⇒ 而下游有一处解码 ⇒ **双解码** ⇒ 颜色暗 61–65 级 ✗
///
/// `alpha` 是 `eraser_target_alpha`（alpha 不做 gamma 转换 ⇒ 它在两个空间里都是线性量）。
#[must_use]
pub fn mix_smudge_linear(
    smudge: [f32; 3],
    brush: [f32; 3],
    amt: f32,
    alpha: f32,
) -> [f32; 3] {
    let mix = |s: f32, b: f32| {
        let sl = srgb_to_linear(s);
        let bl = srgb_to_linear(b);
        linear_to_srgb(((amt * sl + (1.0 - amt) * bl) / alpha).clamp(0.0, 1.0))
    };
    [
        mix(smudge[0], brush[0]),
        mix(smudge[1], brush[1]),
        mix(smudge[2], brush[2]),
    ]
}

#[cfg(test)]
mod phase1_pipeline_space {
    //! **Phase 1（色彩体系规划）的判据**：`hsv_to_rgb` 的输出**是 sRGB，不是 linear** ✓
    //!
    //! **⇒ 为什么这条判据必须先存在**（第 1298 轮）：
    //!   Phase 1 要改的是「`base` 应在 linear 空间参与混合」⇒ 而**改之前必须先能证明现状**。
    //!   我用端到端像素测了**五次都失败**（笔刷透明把信号埋了，见 HANDOFF「测量失败记录」）⇒
    //!   所以判据只能在这里：**两个确定的数字**。
    //!
    //! **⇒ 三个数字**（`v=0.8`、`s=1.0`、`h=0.0`）：
    //!   · sRGB 空间：`r = 0.8`（标准 HSV→RGB 直接给 `v`）
    //!   · linear 空间：`r = srgb_to_linear(0.8) = 0.603827`
    //!   · **差 0.196** ⇒ 所以两者**不可能混淆** ✓
    //!
    //! **⇒ 变异**：把 `hsv_to_rgb` 的返回改成 `srgb_to_linear` 处理过 ⇒ 这条判据必红。

    use super::{hsv_to_rgb, srgb_to_linear, Hsv};

    /// **饱和红在 `v=0.8` 时，`hsv_to_rgb` 给 `0.8`（sRGB），不是 `0.6038`（linear）。**
    #[test]
    fn hsv_to_rgb_returns_srgb_not_linear() {
        let c = hsv_to_rgb(Hsv {
            h: 0.0,
            s: 1.0,
            v: 0.8,
        });
        // sRGB：直接就是 v
        assert!(
            (c.r - 0.8).abs() < 0.001,
            "hsv_to_rgb 必须返回 **sRGB**（r 应 0.8），实测 {}",
            c.r
        );
        assert!((c.g - 0.0).abs() < 0.001, "g 应 0，实测 {}", c.g);
        assert!((c.b - 0.0).abs() < 0.001, "b 应 0，实测 {}", c.b);
        // 而 linear 的值明显不同 ⇒ 证明两者不混淆
        let lin = srgb_to_linear(c.r);
        assert!(
            (lin - 0.603_827).abs() < 0.001,
            "srgb_to_linear(0.8) 应 0.603827，实测 {lin}"
        );
        assert!(
            (c.r - lin).abs() > 0.15,
            "sRGB 与 linear 的差应 > 0.15（实测 {:.4}）⇒ 否则判据无鉴别力",
            (c.r - lin).abs()
        );
    }

    /// **灰阶的三个值都相等**（`s=0` 时 HSV→RGB 给 `v,v,v`）⇒ 守住「无色彩偏移」。
    #[test]
    fn grey_stays_grey_in_srgb() {
        for v in [0.0f32, 0.25, 0.5, 0.8, 1.0] {
            let c = hsv_to_rgb(Hsv { h: 0.0, s: 0.0, v });
            assert!((c.r - v).abs() < 0.001, "v={v}：r 应 {v}，实测 {}", c.r);
            assert!((c.g - v).abs() < 0.001, "v={v}：g 应 {v}，实测 {}", c.g);
            assert!((c.b - v).abs() < 0.001, "v={v}：b 应 {v}，实测 {}", c.b);
        }
    }

    /// **`srgb_to_linear` 与 `linear_to_srgb` 互逆**（往返误差 < 1e-5）。
    ///
    /// **⇒ 为什么重要**：Phase 1 若在 `base` 之后转 linear、在导出时转回 sRGB ⇒
    ///   那**必须是互逆的**，否则往返会引入漂移。
    #[test]
    fn srgb_round_trip_is_lossless() {
        for c in [0.0f32, 0.001, 0.04045, 0.1, 0.5, 0.8, 1.0] {
            let back = super::linear_to_srgb(srgb_to_linear(c));
            assert!(
                (back - c).abs() < 1e-5,
                "c={c}：往返后 {back}（应相等）"
            );
        }
    }
}

#[cfg(test)]
mod phase1_smudge_mix {
    //! **线性涂抹混合的判据**（第 1306 轮；色彩体系规划 Phase 1）
    //!
    //! **⇒ 要证的命题**：加权混合在**线性空间**做 ⇒ 结果**比 sRGB 空间的混合更亮**。
    //!
    //! **⇒ 三个确定的数字**（`smudge=0.8`、`brush=0.1`、`amt=0.5`、`alpha=1`）：
    //!   · **sRGB 空间**混合 = `(0.8 + 0.1)/2 = 0.45`
    //!   · **linear 空间**混合 = `linear_to_srgb((srgb_to_linear(0.8)+srgb_to_linear(0.1))/2)`
    //!     = `linear_to_srgb((0.603827 + 0.010023)/2)` = `linear_to_srgb(0.306925)` = **`0.5911`**
    //!   · **差 0.1411** ⇒ 换算 8-bit 是 **36 级** ⇒ 两者**不可能混淆** ✓
    //!
    //! **⇒ 变异**：把 `mix_smudge_linear` 改成「直接在 sRGB 上插值」⇒ 这条必红。

    use super::mix_smudge_linear;

    /// **线性混合给出 0.5911，不是 0.45。**
    #[test]
    fn smudge_mix_is_brighter_than_srgb_mix() {
        let out = mix_smudge_linear([0.8, 0.8, 0.8], [0.1, 0.1, 0.1], 0.5, 1.0);
        assert!(
            (out[0] - 0.5911).abs() < 0.002,
            "线性混合应给 0.5911，实测 {:.4}",
            out[0]
        );
        let srgb_mix = (0.8 + 0.1) / 2.0;
        assert!(
            out[0] > srgb_mix + 0.1,
            "线性必须比 sRGB 混合（0.45）亮 0.1 以上，实测 {:.4}",
            out[0]
        );
    }

    /// **端点与恒等**：`amt=0` ⇒ 取笔色；`amt=1` ⇒ 取涂抹色；两色相同 ⇒ 原值。
    #[test]
    fn smudge_mix_endpoints_and_identity() {
        let brush = [0.25, 0.5, 0.75];
        let smudge = [0.9, 0.1, 0.4];
        let at0 = mix_smudge_linear(smudge, brush, 0.0, 1.0);
        for i in 0..3 {
            assert!((at0[i] - brush[i]).abs() < 1e-4, "amt=0 应给笔色（{i}）");
        }
        let at1 = mix_smudge_linear(smudge, brush, 1.0, 1.0);
        for i in 0..3 {
            assert!((at1[i] - smudge[i]).abs() < 1e-4, "amt=1 应给涂抹色（{i}）");
        }
        let same = mix_smudge_linear(brush, brush, 0.37, 1.0);
        for i in 0..3 {
            assert!((same[i] - brush[i]).abs() < 1e-4, "同色混合应恒等（{i}）");
        }
    }

    /// **单调性**：`amt` 从 0 到 1 时，结果单调靠近涂抹色（不许出现回摆）。
    #[test]
    fn smudge_mix_is_monotonic() {
        let mut prev = mix_smudge_linear([0.9, 0.9, 0.9], [0.1, 0.1, 0.1], 0.0, 1.0)[0];
        for k in 1..=10 {
            let v = mix_smudge_linear([0.9, 0.9, 0.9], [0.1, 0.1, 0.1], k as f32 / 10.0, 1.0)[0];
            assert!(v >= prev - 1e-5, "amt={} 时结果回摆了（{prev} → {v}）", k as f32 / 10.0);
            prev = v;
        }
    }
}
