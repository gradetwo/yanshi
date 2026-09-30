//! 笔触曲线：`size_curve` / `opacity_curve` / `pressure_curve`（设计文档 11.1 / 810 行）。
//!
//! 笔刷的 `advanced.appearance` 允许用**控制点数组**描述笔触随压力变化的响应：
//!
//! ```json
//! {
//!   "size_curve": [[0.0, 0.2], [0.5, 1.0], [1.0, 0.0]],
//!   "opacity_curve": [[0.0, 0.0], [1.0, 1.0]],
//!   "pressure_curve": [[0.0, 1.0], [1.0, 0.0]]
//! }
//! ```
//!
//! 每个控制点是 `[t, value]`，`t` 与 `value` 都在 `0..1`。曲线求值使用**单调分段线性插值**
//! （控制点按 `t` 升序内部排序，端点外取端点值），结果 clamp 到 `0..1`。
//!
//! ## 与内核接线的语义
//!
//! [`StrokeCurves::modulate`] 是给 [`crate::brush`] 的接线入口：先用 `pressure` 曲线把
//! 原始压力映射一次，再用**映射后的压力**分别查询 `size` / `opacity` 曲线，得到半径与
//! alpha 的缩放系数，最后把半径钳到 `>= 0`、alpha 钳到 `0..1`。
//!
//! ## 确定性（D0）
//!
//! 本模块是纯函数：不读系统时钟、不做 I/O、不使用浮点环境之外的任何全局状态，
//! 相同输入在任何平台上得到相同输出。非法输入一律**退化为恒等曲线 `constant(1.0)`**，
//! 不 panic。

use serde_json::Value;

/// 浮点比较容差（本项目统一量级）。
const EPS: f32 = 1e-6;

/// 把值规范化到 `0..1`；非有限值（NaN / ±Inf）退化为 `1.0`。
fn normalize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// 单条笔触曲线：一组按 `t` 升序排列的控制点。
///
/// 内部不变量：`points` **永不为空**（空 / 非法输入退化为 [`Curve::constant`]），
/// 且所有坐标都是有限值、`t` 与值都已 clamp 到 `0..1`。因此求值不会 panic。
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    /// `(t, value)` 控制点，按 `t` 升序、`t` 相同者只保留第一个。
    points: Vec<(f32, f32)>,
}

impl Default for Curve {
    /// 默认即恒等曲线 `constant(1.0)`。
    fn default() -> Self {
        Self::constant(1.0)
    }
}

impl Curve {
    /// 构造恒等（常量）曲线：任何 `t` 都返回 `value`（clamp 到 `0..1`，非有限值取 `1.0`）。
    pub fn constant(value: f32) -> Self {
        Self {
            points: vec![(0.0, normalize_unit(value))],
        }
    }

    /// 从 `serde_json::Value` 解析控制点数组。
    ///
    /// 接受 `[[0.0, 0.2], [0.5, 1.0], [1.0, 0.0]]` 形式（`t`、值均在 `0..1`）。
    /// 以下情况一律**退化为 `constant(1.0)`**，不 panic：
    ///
    /// - `None`、非数组、空数组；
    /// - 任一元素不是「恰好两个有限数」的数组（`null` / 字符串 / 嵌套 / NaN / ±Inf）。
    ///
    /// `t` 超出 `0..1` 时**不视为非法**，而是 clamp 到 `0..1`（文档域外的宽容处理）；
    /// 值超出 `0..1` 时 clamp 到 `0..1`。
    pub fn from_value(value: Option<&Value>) -> Self {
        match value.and_then(Self::parse_points) {
            Some(points) => Self { points },
            None => Self::constant(1.0),
        }
    }

    /// 曲线是否「无形状」：只有一个控制点，或所有控制点取值相同（容差 `1e-6`）。
    ///
    /// 为真时内核可以走标量快路径，跳过逐 stamp 的插值。
    pub fn is_identity_or_constant(&self) -> bool {
        let first = self.points[0].1;
        self.points
            .iter()
            .all(|point| (point.1 - first).abs() < EPS)
    }

    /// 求值：单调分段线性插值。
    ///
    /// - `t <=` 首点 `t` ⇒ 首点值；`t >=` 末点 `t` ⇒ 末点值；
    /// - `t` 非有限（NaN / ±Inf）⇒ 首点值；
    /// - 返回值 clamp 到 `0..1`。
    pub fn eval(&self, t: f32) -> f32 {
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];

        if !t.is_finite() || t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }

        for window in self.points.windows(2) {
            let (t0, v0) = window[0];
            let (t1, v1) = window[1];
            if t >= t0 && t <= t1 {
                let span = t1 - t0;
                if span <= EPS {
                    return v1;
                }
                let k = (t - t0) / span;
                return normalize_unit(v0 + (v1 - v0) * k);
            }
        }

        last.1
    }

    /// 解析控制点数组；任何非法输入返回 `None`（由调用方退化为恒等曲线）。
    fn parse_points(value: &Value) -> Option<Vec<(f32, f32)>> {
        let array = value.as_array()?;
        if array.is_empty() {
            return None;
        }

        let mut points = Vec::with_capacity(array.len());
        for item in array {
            let pair = item.as_array()?;
            if pair.len() != 2 {
                return None;
            }
            let t = pair[0].as_f64()?;
            let v = pair[1].as_f64()?;
            if !t.is_finite() || !v.is_finite() {
                return None;
            }
            // t 超出文档域时按端点处理，值直接 clamp 到 0..1。
            let t = normalize_unit(t as f32);
            let v = normalize_unit(v as f32);
            points.push((t, v));
        }

        // f32 无全序，用 total_cmp 保证跨平台一致（此处所有值均有限）。
        points.sort_by(|a, b| a.0.total_cmp(&b.0));
        // 同一 t 只保留排序后的第一个，避免零宽区间导致插值跳变。
        points.dedup_by(|a, b| (a.0 - b.0).abs() < EPS);
        if points.is_empty() {
            return None;
        }
        Some(points)
    }
}

/// 一次笔触的三条曲线，对应 `advanced.appearance` 的三个键。
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeCurves {
    /// 大小曲线：控制半径缩放系数。
    pub size: Curve,
    /// 不透明度曲线：控制 alpha 缩放系数。
    pub opacity: Curve,
    /// 压力曲线：先把原始压力映射一次，再供上述两条曲线查询。
    pub pressure: Curve,
}

impl Default for StrokeCurves {
    /// 三条曲线全部为恒等曲线（即不改变任何输入）。
    fn default() -> Self {
        Self::from_appearance(&Value::Null)
    }
}

impl StrokeCurves {
    /// 从 `advanced.appearance` 对象解析 `size_curve` / `opacity_curve` / `pressure_curve`。
    ///
    /// 键缺失、`appearance` 不是对象、或某条曲线非法时，对应曲线取恒等 `constant(1.0)`。
    pub fn from_appearance(appearance: &Value) -> StrokeCurves {
        let get = |key: &str| appearance.get(key);
        StrokeCurves {
            size: Curve::from_value(get("size_curve")),
            opacity: Curve::from_value(get("opacity_curve")),
            pressure: Curve::from_value(get("pressure_curve")),
        }
    }

    /// 把笔触曲线作用到基础半径与基础 alpha 上（内核接线入口）。
    ///
    /// 语义：`p = pressure.clamp(0, 1)`；`p' = pressure_curve.eval(p)`；
    /// 半径 `= base_radius * size_curve.eval(p')`，alpha `= base_alpha * opacity_curve.eval(p')`。
    /// 结果半径钳到 `>= 0`（NaN 记 0），alpha 钳到 `0..1`（NaN 记 0）。
    pub fn modulate(&self, base_radius: f64, base_alpha: f32, pressure: f64) -> (f64, f32) {
        // 压力先归一化到 0..1（NaN / ±Inf 视为 0），再走压力曲线。
        let normalized = if pressure.is_finite() {
            pressure.clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let mapped = self.pressure.eval(normalized);
        let size_scale = self.size.eval(mapped) as f64;
        let opacity_scale = self.opacity.eval(mapped);

        let radius = base_radius * size_scale;
        let radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };

        let alpha = base_alpha * opacity_scale;
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            0.0
        };

        (radius, alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 三点曲线（0.0→0.2、0.5→1.0、1.0→0.0），期望值写死。
    fn three_point() -> Curve {
        Curve::from_value(Some(&json!([[0.0, 0.2], [0.5, 1.0], [1.0, 0.0]])))
    }

    #[test]
    fn constant_curve_evaluates_to_one_everywhere() {
        let curve = Curve::constant(1.0);
        for t in [-5.0f32, -0.5, 0.0, 0.37, 0.5, 1.0, 2.0, 100.0] {
            assert!((curve.eval(t) - 1.0).abs() < 1e-6, "t={t} 应恒为 1.0");
        }
        assert!(curve.is_identity_or_constant());
        // 非有限 t 也退化到首点值，不 panic。
        assert!((curve.eval(f32::NAN) - 1.0).abs() < 1e-6);
        assert!((curve.eval(f32::INFINITY) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn three_point_curve_has_exact_expected_values() {
        let curve = three_point();
        assert!(!curve.is_identity_or_constant());
        // 写死期望值：折线 (0, 0.2) → (0.5, 1.0) → (1.0, 0.0)。
        assert!((curve.eval(0.0) - 0.2).abs() < 1e-6);
        assert!((curve.eval(0.25) - 0.6).abs() < 1e-6);
        assert!((curve.eval(0.5) - 1.0).abs() < 1e-6);
        assert!((curve.eval(0.75) - 0.5).abs() < 1e-6);
        assert!((curve.eval(1.0) - 0.0).abs() < 1e-6);
        // 端点外取端点值。
        assert!((curve.eval(-1.0) - 0.2).abs() < 1e-6);
        assert!((curve.eval(3.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn control_points_are_sorted_by_t() {
        let curve = Curve::from_value(Some(&json!([[1.0, 0.0], [0.0, 0.2], [0.5, 1.0]])));
        assert!((curve.eval(0.25) - 0.6).abs() < 1e-6);
        assert!((curve.eval(0.75) - 0.5).abs() < 1e-6);
        assert_eq!(curve, three_point());
    }

    #[test]
    fn values_are_clamped_into_unit_range() {
        // 值 1.5 → 1.0，值 -0.5 → 0.0。
        let curve = Curve::from_value(Some(&json!([[0.0, 1.5], [1.0, -0.5]])));
        assert!((curve.eval(0.0) - 1.0).abs() < 1e-6);
        assert!((curve.eval(0.5) - 0.5).abs() < 1e-6);
        assert!((curve.eval(1.0) - 0.0).abs() < 1e-6);
        // 常量曲线同样 clamp。
        assert!((Curve::constant(1.5).eval(0.4) - 1.0).abs() < 1e-6);
        assert!((Curve::constant(-0.5).eval(0.4) - 0.0).abs() < 1e-6);
        assert!((Curve::constant(f32::NAN).eval(0.4) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn single_point_curve_is_constant() {
        let curve = Curve::from_value(Some(&json!([[0.7, 0.3]])));
        assert!((curve.eval(0.0) - 0.3).abs() < 1e-6);
        assert!((curve.eval(1.0) - 0.3).abs() < 1e-6);
        assert!(curve.is_identity_or_constant());
    }

    #[test]
    fn equal_values_curve_is_constant() {
        let curve = Curve::from_value(Some(&json!([[0.0, 0.4], [0.5, 0.4], [1.0, 0.4]])));
        assert!(curve.is_identity_or_constant());
        assert!((curve.eval(0.25) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn illegal_and_empty_inputs_degrade_to_identity_without_panic() {
        let cases: Vec<Option<Value>> = vec![
            None,
            Some(json!({})),
            Some(json!("size_curve")),
            Some(Value::Null),
            Some(json!([])),
            Some(json!([[0.0]])),
            Some(json!([[0.0, 1.0, 2.0]])),
            Some(json!([[0.0, "high"]])),
            Some(json!([[0.0, null]])),
            Some(json!([["a", "b"]])),
            Some(json!([[0.0, 0.5], "tail"])),
            Some(json!([0.0, 0.5])),
            Some(json!([[0.0, 0.5], [1.0, f64::NAN]])),
            Some(json!([[0.0, 0.5], [1.0, f64::INFINITY]])),
        ];
        for case in cases {
            let curve = Curve::from_value(case.as_ref());
            assert_eq!(curve, Curve::constant(1.0), "非法输入应退化为恒等曲线");
            assert!(curve.is_identity_or_constant());
            for t in [0.0f32, 0.25, 0.5, 1.0] {
                assert!((curve.eval(t) - 1.0).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn appearance_without_curve_keys_is_identity() {
        let curves = StrokeCurves::from_appearance(&json!({"wetness": 0.5, "seed": 7}));
        assert_eq!(curves, StrokeCurves::default());
        assert_eq!(curves.size, Curve::constant(1.0));
        assert_eq!(curves.opacity, Curve::constant(1.0));
        assert_eq!(curves.pressure, Curve::constant(1.0));
        // 非对象 appearance 同样退化为恒等。
        assert_eq!(
            StrokeCurves::from_appearance(&Value::Null),
            StrokeCurves::default()
        );
        // 恒等曲线下 modulate 原样返回（只做 clamp）。
        let (radius, alpha) = curves.modulate(5.0, 0.4, 0.3);
        assert!((radius - 5.0).abs() < 1e-6);
        assert!((alpha - 0.4).abs() < 1e-6);
    }

    #[test]
    fn appearance_parses_all_three_curves() {
        let curves = StrokeCurves::from_appearance(&json!({
            "size_curve": [[0.0, 0.2], [0.5, 1.0], [1.0, 0.0]],
            "opacity_curve": [[0.0, 0.0], [1.0, 1.0]],
            "pressure_curve": [[0.0, 1.0], [1.0, 0.0]],
        }));
        assert_eq!(curves.size, three_point());
        assert!((curves.opacity.eval(0.25) - 0.25).abs() < 1e-6);
        assert!((curves.pressure.eval(0.25) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn modulate_combines_pressure_and_size_opacity_curves() {
        let curves = StrokeCurves::from_appearance(&json!({
            "size_curve": [[0.0, 0.5], [1.0, 1.0]],
            "opacity_curve": [[0.0, 0.0], [1.0, 1.0]],
            "pressure_curve": [[0.0, 1.0], [1.0, 0.0]],
        }));

        // pressure=0.25 → 压力曲线映射为 0.75。
        // size(0.75) = 0.5 + 0.5·0.75 = 0.875 → 半径 12.0·0.875 = 10.5。
        // opacity(0.75) = 0.75 → alpha 0.8·0.75 = 0.6。
        let (radius, alpha) = curves.modulate(12.0, 0.8, 0.25);
        assert!((radius - 10.5).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.6).abs() < 1e-6, "alpha={alpha}");

        // pressure=1.0 → 映射为 0.0；size(0.0)=0.5、opacity(0.0)=0.0。
        let (radius, alpha) = curves.modulate(12.0, 0.8, 1.0);
        assert!((radius - 6.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.0).abs() < 1e-6, "alpha={alpha}");

        // 压力越界按 0..1 clamp：-0.5 → 0.0 → 映射为 1.0；size=1.0、opacity=1.0。
        let (radius, alpha) = curves.modulate(12.0, 0.8, -0.5);
        assert!((radius - 12.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.8).abs() < 1e-6, "alpha={alpha}");

        // 非有限压力按 0 处理。
        let (radius, alpha) = curves.modulate(12.0, 0.8, f64::NAN);
        assert!((radius - 12.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.8).abs() < 1e-6, "alpha={alpha}");
    }

    #[test]
    fn modulate_clamps_radius_and_alpha() {
        let curves = StrokeCurves::default();
        // 负半径 ⇒ 0；alpha 1.5 ⇒ 1.0。
        let (radius, alpha) = curves.modulate(-4.0, 1.5, 1.0);
        assert!((radius - 0.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 1.0).abs() < 1e-6, "alpha={alpha}");
        // 负 alpha ⇒ 0；半径保持。
        let (radius, alpha) = curves.modulate(4.0, -0.25, 1.0);
        assert!((radius - 4.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.0).abs() < 1e-6, "alpha={alpha}");
        // 曲线取负值（经 clamp 后为 0）⇒ 半径 0、alpha 0。
        let zero_curves = StrokeCurves::from_appearance(&json!({
            "size_curve": [[0.0, -1.0], [1.0, -1.0]],
            "opacity_curve": [[0.0, -0.5], [1.0, -0.5]],
        }));
        let (radius, alpha) = zero_curves.modulate(8.0, 1.0, 0.5);
        assert!((radius - 0.0).abs() < 1e-6, "radius={radius}");
        assert!((alpha - 0.0).abs() < 1e-6, "alpha={alpha}");
    }
}
