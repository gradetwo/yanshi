//! 湿笔、载墨量与混色（设计文档 10.3 `advanced.appearance` 与 11.1）。
//!
//! 设计文档在 `advanced.appearance` 中规定了 `wetness`（湿度）、`paint_load`（载墨量）
//! 与 `mixing`（混色）三个字段，11.1 要求 MVP 的通用光栅笔刷必须支持湿度与载墨量。
//! 本模块把这三个字段变成可解释、可复现的**落笔状态机** [`PaintReservoir`]：
//!
//! - **载墨量**：一支笔携带的墨量随每个印章衰减，[`PaintReservoir::consume`] 返回本印章
//!   的不透明度乘数（`0..1`）；`paint_load = 0` 表示**无限墨**（恒返回 `1.0`），
//!   因此没有这些字段的旧笔刷行为完全不变。
//! - **湿度**：越湿的笔在连续拖动（[`PaintReservoir::advance`] 累计像素距离）中耗墨越快，
//!   混色也越强；干笔（`wetness = 0`）拖动不额外耗墨。
//! - **混色**：[`PaintReservoir::mix`] 把画布已有颜色按 `mixing × 当前湿度` 混进笔尖颜色。
//!
//! 本模块**不读系统时间、不联网、零新依赖**：全部状态都在 [`PaintReservoir`] 内，
//! 同一调用序列在任何平台、任意次数重放都逐位一致（D0 基线要求）；所有返回值都被清洗到
//! 合法范围且不含 `NaN`。
//!
//! ```
//! use yanshi_render::paint::{PaintReservoir, PaintSettings};
//!
//! // 满墨湿笔：首个印章全量落笔，拖动 10px 后墨量与后续乘数一起下降。
//! let mut reservoir = PaintReservoir::new(PaintSettings {
//!     paint_load: 1.0,
//!     wetness: 1.0,
//!     mixing: 0.5,
//! });
//! assert_eq!(reservoir.consume(), 1.0);
//! reservoir.advance(10.0);
//! assert!((reservoir.remaining() - 0.88).abs() < 1e-6);
//! ```

use crate::color::LinearRgba;
use serde_json::Value;

/// 每个印章的基础耗墨量（占满墨量的比例，`0.02` 即 50 个印章耗尽）。
pub const STAMP_DRAIN: f32 = 0.02;

/// 每像素拖动距离的基础耗墨量（还要乘以 `wetness`，`0.01` 即每 100px 耗完满墨）。
pub const DISTANCE_DRAIN: f32 = 0.01;

/// 墨量耗尽后的不透明度乘数下限，保证乘数恒 `> 0`、永不为负或 `NaN`。
pub const MIN_MULTIPLIER: f32 = 0.05;

/// 把标量清洗为合法的 `0..1`：`NaN` → `0`，`±∞` 与越界值 clamp 到边界。
fn clamp_unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// 把直通线性颜色逐分量清洗到 `0..1`（含 alpha），保证输出不含 `NaN`。
fn clamp_color(color: LinearRgba) -> LinearRgba {
    [
        clamp_unit(color[0]),
        clamp_unit(color[1]),
        clamp_unit(color[2]),
        clamp_unit(color[3]),
    ]
}

/// 笔刷外观里的湿笔参数（设计文档 10.3 `advanced.appearance`）。
///
/// 三个字段都在 `0..1`。缺省（字段不存在）全为 `0.0`，即关闭湿笔、载墨量与混色，
/// 与既有静态笔刷行为一致——`paint_load = 0` 是"无限墨"，见
/// [`PaintSettings::from_appearance`]。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PaintSettings {
    /// 载墨量：`0` 表示无限墨（不受墨量限制），越大携带的墨越多、越晚见底。
    pub paint_load: f32,
    /// 湿度：越大则连续拖动耗墨越快、混色越强；`0` 为干笔。
    pub wetness: f32,
    /// 混色强度：越大则落笔颜色越靠近画布已有颜色；`0` 表示不混色。
    pub mixing: f32,
}

impl PaintSettings {
    /// 从 `advanced.appearance` 对象解析（键：`paint_load` / `wetness` / `mixing`）。
    ///
    /// - 非数字、越界值都会被清洗到 `0..1`；负值与 `0` 等价，`NaN` 归零。
    /// - 字段缺失时保持 [`PaintSettings::default`] 的 `0.0`：`paint_load = 0` 是
    ///   "无限墨"，所以没有这三个字段的旧笔刷渲染结果完全不变。
    pub fn from_appearance(appearance: &Value) -> Self {
        let mut settings = Self::default();
        if let Some(value) = appearance.get("paint_load").and_then(Value::as_f64) {
            settings.paint_load = clamp_unit(value as f32);
        }
        if let Some(value) = appearance.get("wetness").and_then(Value::as_f64) {
            settings.wetness = clamp_unit(value as f32);
        }
        if let Some(value) = appearance.get("mixing").and_then(Value::as_f64) {
            settings.mixing = clamp_unit(value as f32);
        }
        settings
    }
}

/// 湿笔落笔状态机：墨量、湿度与混色（设计文档 11.1）。
///
/// 由 [`PaintReservoir::new`] 从 [`PaintSettings`] 建立，之后每次落笔调用
/// [`PaintReservoir::consume`]，每次拖动调用 [`PaintReservoir::advance`]，
/// 需要混色时调用 [`PaintReservoir::mix`]。状态只有剩余墨量、累计距离与印章数，
/// 不读写任何全局量，因此完全确定性。
#[derive(Debug, Clone, PartialEq)]
pub struct PaintReservoir {
    /// 清洗后的湿笔参数。
    settings: PaintSettings,
    /// 剩余墨量比例（`0..1`）；无限墨模式恒为 `1.0`。
    load: f32,
    /// 累计落笔距离（像素），用于可观测性与测试。
    distance: f64,
    /// 已落印章数，用于可观测性与测试。
    stamps: u64,
}

impl PaintReservoir {
    /// 以给定参数创建蓄墨池：满墨（`paint_load = 1`）时首个 [`Self::consume`] 返回 `1.0`。
    ///
    /// 参数会先被清洗到 `0..1`（`NaN` → `0`）：`paint_load = 0` 进入无限墨模式。
    pub fn new(settings: PaintSettings) -> Self {
        let settings = PaintSettings {
            paint_load: clamp_unit(settings.paint_load),
            wetness: clamp_unit(settings.wetness),
            mixing: clamp_unit(settings.mixing),
        };
        // 无限墨时内部墨量恒为满，便于混色权重使用"当前湿度"。
        let load = if settings.paint_load <= 0.0 {
            1.0
        } else {
            settings.paint_load
        };
        Self {
            settings,
            load,
            distance: 0.0,
            stamps: 0,
        }
    }

    /// 是否为无限墨（`paint_load = 0`）：[`Self::consume`] 恒返回 `1.0`，墨量不衰减。
    pub fn is_unlimited(&self) -> bool {
        self.settings.paint_load <= 0.0
    }

    /// 每落一个印章调用一次，返回本印章的**不透明度乘数**（`0..1`）。
    ///
    /// - 无限墨（`paint_load = 0`）恒返回 `1.0`，与既有静态笔刷行为兼容。
    /// - 否则乘数 = `MIN_MULTIPLIER + (1 - MIN_MULTIPLIER) × 剩余墨量`：
    ///   满墨首个印章恰为 `1.0`；每调用一次剩余墨量减少 [`STAMP_DRAIN`]，
    ///   因此乘数严格递减，耗尽后稳定在下限 [`MIN_MULTIPLIER`]（恒 `> 0`，不会为负或 `NaN`）。
    pub fn consume(&mut self) -> f32 {
        self.stamps += 1;
        if self.is_unlimited() {
            return 1.0;
        }
        let multiplier = if self.load >= 1.0 {
            1.0
        } else {
            MIN_MULTIPLIER + (1.0 - MIN_MULTIPLIER) * self.load
        };
        self.load = (self.load - STAMP_DRAIN).max(0.0);
        clamp_unit(multiplier)
    }

    /// 累计落笔"距离"（像素）：湿笔在连续拖动中比点按更耗墨。
    ///
    /// 耗墨量 = [`DISTANCE_DRAIN`] × `wetness` × `distance`：
    /// 干笔（`wetness = 0`）不额外耗墨；负值、`NaN`、`±∞` 一律按 `0` 忽略（但仍计入累计距离）。
    pub fn advance(&mut self, distance: f64) {
        if !distance.is_finite() || distance <= 0.0 {
            return;
        }
        self.distance += distance;
        if self.is_unlimited() {
            return;
        }
        let drain =
            (f64::from(DISTANCE_DRAIN) * f64::from(self.settings.wetness) * distance) as f32;
        self.load = (self.load - drain).max(0.0);
    }

    /// 混色：把目标（画布已有）颜色按 `mixing × 当前湿度` 混进笔尖颜色，返回实际落笔颜色。
    ///
    /// 当前湿度 = `wetness × 剩余墨量`（见 [`Self::current_wetness`]）：
    /// 越湿、墨越满，混色越强；快见底的笔混色变弱。
    /// 权重为 `0` 时**原样**返回 `brush_color`，为 `1` 时返回 `dest_color`；
    /// 中间值对三色与 alpha 一起线性插值。输入先被清洗到 `0..1`，输出不含 `NaN`。
    ///
    /// 本方法只读取内部状态、不修改它，因此同参数调用结果逐位一致。
    pub fn mix(&mut self, brush_color: LinearRgba, dest_color: LinearRgba) -> LinearRgba {
        let brush = clamp_color(brush_color);
        let dest = clamp_color(dest_color);
        let weight = clamp_unit(self.settings.mixing * self.current_wetness());
        if weight <= 0.0 {
            return brush;
        }
        if weight >= 1.0 {
            return dest;
        }
        let mut mixed = [0.0_f32; 4];
        for (channel, out) in mixed.iter_mut().enumerate() {
            *out = clamp_unit(brush[channel] + (dest[channel] - brush[channel]) * weight);
        }
        mixed
    }

    /// 当前湿度：`wetness × 剩余墨量`（无限墨时恒为 `wetness`）。
    ///
    /// 混色权重取 `mixing × current_wetness()`，因此本方法就是混色里的"当前湿度"。
    pub fn current_wetness(&self) -> f32 {
        clamp_unit(self.settings.wetness * self.load)
    }

    /// 剩余墨量比例（`0..1`）：无限墨恒为 `1.0`，墨尽后为 `0.0`。
    pub fn remaining(&self) -> f32 {
        clamp_unit(self.load)
    }

    /// 累计落笔距离（像素），只统计合法（有限且为正）的 [`Self::advance`] 输入。
    pub fn distance(&self) -> f64 {
        self.distance
    }

    /// 已落印章数（[`Self::consume`] 的调用次数，包含无限墨模式）。
    pub fn stamps(&self) -> u64 {
        self.stamps
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 湿笔/载墨量测试用容差（设计文档要求 `1e-6` 量级）。
    const EPS: f32 = 1e-6;

    fn approx(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < EPS,
            "期望 {expected}，实得 {actual}"
        );
    }

    fn settings(paint_load: f32, wetness: f32, mixing: f32) -> PaintSettings {
        PaintSettings {
            paint_load,
            wetness,
            mixing,
        }
    }

    #[test]
    fn unlimited_load_always_returns_one() {
        let mut reservoir = PaintReservoir::new(settings(0.0, 1.0, 0.0));
        assert!(reservoir.is_unlimited());
        for stamp in 0..10_000 {
            approx(reservoir.consume(), 1.0);
            assert_eq!(reservoir.stamps(), stamp + 1);
        }
        // 无限墨即使长时间拖动也永远满墨。
        for _ in 0..100 {
            reservoir.advance(100.0);
        }
        approx(reservoir.remaining(), 1.0);
        approx(reservoir.consume(), 1.0);
        approx(reservoir.current_wetness(), 1.0);
        approx(reservoir.distance() as f32, 10_000.0);
    }

    #[test]
    fn full_load_first_stamp_is_one_then_decays() {
        let mut reservoir = PaintReservoir::new(settings(1.0, 0.0, 0.0));
        assert!(!reservoir.is_unlimited());

        let first = reservoir.consume();
        approx(first, 1.0);
        approx(reservoir.remaining(), 0.98);

        // 前 19 个后续印章严格递减：0.981、0.962、0.943 ……
        let mut previous = first;
        for stamp in 1..20 {
            let current = reservoir.consume();
            assert!(
                current < previous,
                "第 {stamp} 个后续印章应严格递减：{current} 不小于 {previous}"
            );
            previous = current;
        }
        approx(reservoir.remaining(), 0.60);

        // 大量印章后仍落在 (0, 1] 内：不会变成负数或 NaN。
        for _ in 0..2_000 {
            let current = reservoir.consume();
            assert!(
                current.is_finite() && current > 0.0 && current <= 1.0,
                "乘数越界：{current}"
            );
        }
        approx(reservoir.remaining(), 0.0);
        approx(reservoir.consume(), MIN_MULTIPLIER);
    }

    #[test]
    fn wet_brush_drains_faster_while_dragging() {
        let mut dry = PaintReservoir::new(settings(1.0, 0.0, 0.0));
        let mut wet = PaintReservoir::new(settings(1.0, 1.0, 0.0));

        // 两边各累计拖动 50px（5 × 10px）。
        for _ in 0..5 {
            dry.advance(10.0);
            wet.advance(10.0);
        }

        // 写死的具体数值：干笔不额外耗墨，湿笔每像素耗 0.01 满墨量。
        approx(dry.remaining(), 1.0);
        approx(wet.remaining(), 0.5);
        assert!(wet.remaining() < dry.remaining(), "湿笔耗墨必须更快");

        // 乘数 = 0.05 + 0.95 × 剩余墨量。
        approx(dry.consume(), 1.0);
        approx(wet.consume(), 0.525);

        // 非有限/非正距离一律忽略，不改变状态。
        let before = wet.remaining();
        wet.advance(-10.0);
        wet.advance(f64::NAN);
        wet.advance(f64::INFINITY);
        approx(wet.remaining(), before);
    }

    #[test]
    fn mixing_blends_toward_destination() {
        let brush = [1.0_f32, 0.0, 0.0, 1.0];
        let dest = [0.0_f32, 0.0, 1.0, 0.5];

        // mixing = 0.5、wetness = 1、满墨 ⇒ 权重 0.5 的线性插值。
        let mut half = PaintReservoir::new(settings(1.0, 1.0, 0.5));
        let mixed = half.mix(brush, dest);
        approx(mixed[0], 0.5);
        approx(mixed[1], 0.0);
        approx(mixed[2], 0.5);
        approx(mixed[3], 0.75);

        // 更靠近目标色：与目标色的通道距离必须更小。
        let before: f32 = (0..4).map(|i| (brush[i] - dest[i]).abs()).sum();
        let after: f32 = (0..4).map(|i| (mixed[i] - dest[i]).abs()).sum();
        assert!(after < before, "混色后应更靠近目标色：{after} !< {before}");

        // mixing = 1、wetness = 1、满墨 ⇒ 权重 1，直接落到目标色。
        let mut full = PaintReservoir::new(settings(1.0, 1.0, 1.0));
        assert_eq!(full.mix(brush, dest), dest);

        // 干笔（wetness = 0）不混色，即使 mixing = 1。
        let mut dry = PaintReservoir::new(settings(1.0, 0.0, 1.0));
        assert_eq!(dry.mix(brush, dest), brush);
    }

    #[test]
    fn mixing_zero_returns_brush_color_verbatim() {
        let brush = [0.125_f32, 0.25, 0.5, 0.75];
        let dest = [1.0_f32, 1.0, 0.0, 0.0];
        let mut reservoir = PaintReservoir::new(settings(1.0, 1.0, 0.0));
        // 先消耗墨量：mixing = 0 与湿度状态无关，仍须逐位原样返回。
        reservoir.consume();
        reservoir.advance(250.0);
        assert_eq!(reservoir.mix(brush, dest), brush);
    }

    #[test]
    fn extreme_inputs_are_clamped_without_panic() {
        // 构造期清洗：NaN 载墨量 → 0（无限墨），∞ 湿度 → 1，负 mixing → 0。
        let mut reservoir = PaintReservoir::new(PaintSettings {
            paint_load: f32::NAN,
            wetness: f32::INFINITY,
            mixing: -1.0,
        });
        assert!(reservoir.is_unlimited());
        approx(reservoir.current_wetness(), 1.0);
        approx(reservoir.consume(), 1.0);

        // 越界与 NaN 颜色输入被 clamp，且 mixing = 0 时原样返回清洗后的笔色。
        let mixed = reservoir.mix(
            [f32::NAN, 2.0, -1.0, 0.5],
            [1.0, 1.0, 1.0, f32::NEG_INFINITY],
        );
        for channel in mixed {
            assert!(
                channel.is_finite() && (0.0..=1.0).contains(&channel),
                "颜色分量越界：{channel}"
            );
        }
        approx(mixed[3], 0.5);

        // 大数值输入：越界/非有限值全部被压回合法范围。
        let mut limited = PaintReservoir::new(settings(5.0, 1.0, 9.0));
        assert!(!limited.is_unlimited());
        approx(limited.remaining(), 1.0);
        approx(limited.current_wetness(), 1.0);
        limited.advance(f64::NAN);
        approx(limited.remaining(), 1.0);

        // 权重 1：直接落到目标色，笔色的越界值不影响结果。
        let saturated = limited.mix([f32::MAX, f32::MIN, 0.5, 1.0], [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(saturated, [0.0, 0.0, 0.0, 0.0]);

        // 权重 0.5：笔色先被 clamp 到 [1.0, 0.0, 0.5, 1.0] 再插值。
        let mut half = PaintReservoir::new(settings(0.0, 1.0, 0.5));
        let clamped = half.mix([f32::MAX, f32::MIN, 0.5, 1.0], [0.0, 0.0, 0.0, 0.0]);
        approx(clamped[0], 0.5);
        approx(clamped[1], 0.0);
        approx(clamped[2], 0.25);
        approx(clamped[3], 0.5);
    }

    #[test]
    fn from_appearance_defaults_keep_static_brush_behaviour() {
        // 缺省全 0：旧笔刷（没有这三个字段）行为不变。
        assert_eq!(
            PaintSettings::from_appearance(&json!({})),
            PaintSettings::default()
        );
        assert_eq!(
            PaintSettings::from_appearance(&json!({"size": 12.0})),
            settings(0.0, 0.0, 0.0)
        );

        let parsed = PaintSettings::from_appearance(
            &json!({"paint_load": 0.75, "wetness": 0.5, "mixing": 0.25}),
        );
        assert_eq!(parsed, settings(0.75, 0.5, 0.25));

        // 越界与非数字被清洗。
        let clamped = PaintSettings::from_appearance(
            &json!({"paint_load": -2.0, "wetness": 3.0, "mixing": "强"}),
        );
        assert_eq!(clamped, settings(0.0, 1.0, 0.0));

        // 非对象输入不 panic，回落到缺省。
        assert_eq!(
            PaintSettings::from_appearance(&json!([1, 2, 3])),
            PaintSettings::default()
        );
        assert_eq!(
            PaintSettings::from_appearance(&json!(null)),
            PaintSettings::default()
        );
    }
}
