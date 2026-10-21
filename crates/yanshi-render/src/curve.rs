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
//! 每个控制点是 `[t, value]`，`t` 与 `value` 都在 `0..1`。曲线求值使用
//! **Fritsch–Carlson 单调三次插值（PCHIP）**（控制点按 `t` 升序内部排序，端点外取端点值），
//! 结果 clamp 到 `0..1`。PCHIP 保证：单调的控制点列求值后仍单调、求值不越出相邻
//! 控制点的值域（零过冲）、控制点处一阶导连续。切线在构造期按 Fritsch–Butland
//! 调和平均式预计算（端点取单侧割线斜率，割线斜率钳到 ±1e4 防极小区间放大），
//! 求值期只做 Hermite 基组合与区间 clamp，全程 f32 四则运算，无 libm 依赖。
//! N=2 时 Hermite 在数学上恒等于直线，求值直接走线性公式，与历史实现逐位一致。
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
#[derive(Debug, Clone)]
pub struct Curve {
    /// `(t, value)` 控制点，按 `t` 升序、`t` 相同者只保留第一个。
    points: Vec<(f32, f32)>,
    // Fritsch–Carlson 切线（与 `points` 等长，构造期由 `compute_tangents` 填充）。
    // 它是 `points` 的纯函数：相等判定只比 `points`（见手写 `PartialEq`），
    // 避免浮点路径差异让语义相同的两条曲线被判不等。
    tangents: Vec<f32>,
}

impl PartialEq for Curve {
    /// 只比控制点：`tangents` 是 `points` 的纯函数导出量，不进相等语义。
    fn eq(&self, other: &Self) -> bool {
        self.points == other.points
    }
}

/// Fritsch–Carlson 切线预计算（构造期一次性）。
///
/// - 割线斜率 `δ_k = (y_{k+1} − y_k) / h_k`，先钳到 ±1e4：去重后 `h_k` 仍可极小，
///   不钳会让 δ 爆大并在调和平均里传播 Inf / NaN。
/// - 端点取单侧割线：`m_0 = δ_0`、`m_{n−1} = δ_{n−2}`。
/// - 内点：相邻割线异号或任一为 0 ⇒ `m_k = 0`（局部极值处导数为零是保单调的关键）；
///   否则用 Fritsch–Butland 调和平均 `m_k = (w1 + w2) / (w1/δ_{k−1} + w2/δ_k)`，
///   其中 `w1 = 2h_k + h_{k−1}`、`w2 = h_k + 2h_{k−1}`（`h` 为相邻区间宽）。
/// - N=2 时 `m_0 = m_1 = δ_0`，Hermite 退化为直线。
fn compute_tangents(points: &[(f32, f32)]) -> Vec<f32> {
    let n = points.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let mut delta = Vec::with_capacity(n - 1);
    for window in points.windows(2) {
        let h = window[1].0 - window[0].0;
        let d = if h > 0.0 {
            (window[1].1 - window[0].1) / h
        } else {
            0.0
        };
        delta.push(d.clamp(-1.0e4, 1.0e4));
    }
    let mut tangents = vec![0.0f32; n];
    tangents[0] = delta[0];
    tangents[n - 1] = delta[n - 2];
    for k in 1..n - 1 {
        let d_prev = delta[k - 1];
        let d_next = delta[k];
        if d_prev * d_next <= 0.0 {
            tangents[k] = 0.0;
        } else {
            let h_prev = points[k].0 - points[k - 1].0;
            let h_next = points[k + 1].0 - points[k].0;
            let w1 = 2.0 * h_next + h_prev;
            let w2 = h_next + 2.0 * h_prev;
            tangents[k] = (w1 + w2) / (w1 / d_prev + w2 / d_next);
        }
    }
    tangents
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
            tangents: vec![0.0],
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
            Some(points) => {
                let tangents = compute_tangents(&points);
                Self { points, tangents }
            }
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

    /// 求值：Fritsch–Carlson 单调三次插值（PCHIP）。
    ///
    /// - `t <=` 首点 `t` ⇒ 首点值；`t >=` 末点 `t` ⇒ 末点值；
    /// - `t` 非有限（NaN / ±Inf）⇒ 首点值；
    /// - 返回值 clamp 到 `0..1`，且不越出所在区间两端控制点的值域（零过冲）；
    /// - N=2 走线性公式，与 PCHIP 前的历史实现逐位一致。
    pub fn eval(&self, t: f32) -> f32 {
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];

        if !t.is_finite() || t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }

        if self.points.len() == 2 {
            // N=2：Hermite 在数学上恒等于直线；直接用线性公式，
            // 避免基函数重排带来的浮点漂移（逐位一致由测试锁死）。
            let (t0, v0) = first;
            let (t1, v1) = last;
            let span = t1 - t0;
            if span <= EPS {
                return v1;
            }
            let k = (t - t0) / span;
            return normalize_unit(v0 + (v1 - v0) * k);
        }

        // 二分定位区间 k（`points[k].0 <= t < points[k+1].0`）；
        // t 恰为内点控制点时 u=0，Hermite 基给出该点值本身。
        let k = self.points.partition_point(|point| point.0 <= t) - 1;
        let (t0, v0) = self.points[k];
        let (t1, v1) = self.points[k + 1];
        let h = t1 - t0;
        if h <= 0.0 {
            return normalize_unit(v1);
        }
        let u = (t - t0) / h;
        let u2 = u * u;
        let u3 = u2 * u;
        let value = (2.0 * u3 - 3.0 * u2 + 1.0) * v0
            + (u3 - 2.0 * u2 + u) * h * self.tangents[k]
            + (-2.0 * u3 + 3.0 * u2) * v1
            + (u3 - u2) * h * self.tangents[k + 1];
        // FC 理论上已保证不越界；clamp 只防浮点毛刺（数值兜底）。
        normalize_unit(value.clamp(v0.min(v1), v0.max(v1)))
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

    /// 把**尺寸与不透明度曲线**作用到基础半径/alpha 上，采样参数是**笔迹进度** ✓。
    ///
    /// **为什么需要这个入口（设计未规定，此处记录选择 ✓）**：
    /// README 说这些曲线用于"塑造笔触" ✓，而子 agent 实测：用文档推荐的
    /// `points: [[x, y], …]` 形式时（每点压力默认 1.0 ✓），[`Self::modulate`] 会把曲线一律
    /// 采样在 `curve(1)` ✗ ⇒ 一条写了 `size_curve: [[0,1],[0.5,0.5],[1,0.1]]` 的"锥形"笔迹
    /// 画出来是**等宽**的 ✓（实测 40px 笔刷得到均匀 4px 色带 ✓）。
    ///
    /// 因此明确区分两个输入 ✓：
    /// * **进度 `progress ∈ [0,1]`** ⇒ 驱动 `size_curve` / `opacity_curve` ✓（"沿笔迹塑形" ✓）；
    /// * **压力 `pressure ∈ [0,1]`** ⇒ 驱动 `pressure_curve` ✓，并作为 `pressure_size` /
    ///   `min_size_ratio` 的输入（由调用方施加 ✓，两条盖章路径**必须一致** ✓）。
    pub fn modulate_shape(&self, base_radius: f64, base_alpha: f32, progress: f64) -> (f64, f32) {
        let t = if progress.is_finite() {
            progress.clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let size_scale = self.size.eval(t) as f64;
        let opacity_scale = self.opacity.eval(t);

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
        // 写死期望值：PCHIP 下 (0, 0.2) → (0.5, 1.0) → (1.0, 0.0)。
        // 控制点处切线为 [1.6, 0, −2.0]（中点两侧割线异号 ⇒ 切线 0），
        // Hermite 手算：eval(0.25) = 0.7、eval(0.75) = 0.625
        //（旧分段线性分别是 0.6 / 0.5 —— N≥3 存量曲线重放数值变化，属预期，见设计 §2.6）。
        assert!((curve.eval(0.0) - 0.2).abs() < 1e-6);
        assert!((curve.eval(0.25) - 0.7).abs() < 1e-6);
        assert!((curve.eval(0.5) - 1.0).abs() < 1e-6);
        assert!((curve.eval(0.75) - 0.625).abs() < 1e-6);
        assert!((curve.eval(1.0) - 0.0).abs() < 1e-6);
        // 端点外取端点值。
        assert!((curve.eval(-1.0) - 0.2).abs() < 1e-6);
        assert!((curve.eval(3.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn control_points_are_sorted_by_t() {
        let curve = Curve::from_value(Some(&json!([[1.0, 0.0], [0.0, 0.2], [0.5, 1.0]])));
        assert!((curve.eval(0.25) - 0.7).abs() < 1e-6);
        assert!((curve.eval(0.75) - 0.625).abs() < 1e-6);
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

    /// PCHIP 前的旧线性求值（逐字复制旧实现），只供 N=2 逐位对照测试当参照。
    fn legacy_linear_eval(points: &[(f32, f32)], t: f32) -> f32 {
        let first = points[0];
        let last = points[points.len() - 1];
        if !t.is_finite() || t <= first.0 {
            return first.1;
        }
        if t >= last.0 {
            return last.1;
        }
        for window in points.windows(2) {
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

    #[test]
    fn pchip_preserves_monotonicity() {
        // 单调控制点列稠密采样 1001 点必须保持单调（容差 0：FC 的核心承诺）。
        let rising = Curve::from_value(Some(&json!([
            [0.0, 0.0],
            [0.3, 0.1],
            [0.6, 0.9],
            [1.0, 1.0]
        ])));
        let mut previous = rising.eval(0.0);
        for i in 1..=1000 {
            let x = i as f32 / 1000.0;
            let value = rising.eval(x);
            assert!(
                value >= previous,
                "递增曲线在 x={x} 处回落：{previous} → {value}"
            );
            previous = value;
        }
        let falling = Curve::from_value(Some(&json!([
            [0.0, 1.0],
            [0.4, 0.85],
            [0.7, 0.2],
            [1.0, 0.0]
        ])));
        let mut previous = falling.eval(0.0);
        for i in 1..=1000 {
            let x = i as f32 / 1000.0;
            let value = falling.eval(x);
            assert!(
                value <= previous,
                "递减曲线在 x={x} 处回升：{previous} → {value}"
            );
            previous = value;
        }
        // 带平台段的单调数据：平台两侧切线为 0，不许在平台边缘鼓包。
        let plateau = Curve::from_value(Some(&json!([
            [0.0, 0.0],
            [0.3, 0.5],
            [0.6, 0.5],
            [1.0, 1.0]
        ])));
        let mut previous = plateau.eval(0.0);
        for i in 1..=1000 {
            let x = i as f32 / 1000.0;
            let value = plateau.eval(x);
            assert!(
                value >= previous,
                "平台曲线在 x={x} 处回落：{previous} → {value}"
            );
            previous = value;
        }
    }

    #[test]
    fn pchip_never_overshoots() {
        // 固定种子伪随机 200 组控制点：每区间采样值必须落在该区间两端值域内（零过冲）。
        let mut state = 0x2545_f491u32;
        let mut next_unit = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state % 10_000) as f32 / 10_000.0
        };
        for case in 0..200 {
            let n = 3 + (next_unit() * 5.0) as usize; // 3..=7 个控制点
            let raw: Vec<[f32; 2]> = (0..n)
                .map(|i| {
                    let t = (i as f32 + next_unit() * 0.9) / n as f32;
                    [t, next_unit()]
                })
                .collect();
            let curve = Curve::from_value(Some(&json!(raw)));
            let points = curve.points.clone();
            for window in points.windows(2) {
                let (t0, v0) = window[0];
                let (t1, v1) = window[1];
                let (lo, hi) = (v0.min(v1), v0.max(v1));
                for s in 0..=20 {
                    let x = t0 + (t1 - t0) * (s as f32 / 20.0);
                    let value = curve.eval(x);
                    assert!(
                        value >= lo && value <= hi,
                        "case {case}：eval({x}) = {value} 越出区间值域 [{lo}, {hi}]"
                    );
                }
            }
        }
    }

    #[test]
    fn pchip_two_points_equals_legacy_linear() {
        // N=2 与旧线性公式在 101 个采样点逐位相等（f32 位模式）。
        for raw in [
            [(0.2f32, 0.15f32), (0.9, 0.85)],
            [(0.0, 1.0), (1.0, 0.0)],
            [(0.35, 0.5), (0.65, 0.5)],
        ] {
            let curve = Curve::from_value(Some(&json!(raw)));
            assert_eq!(curve.points.len(), 2);
            for i in 0..=100 {
                let t = i as f32 / 100.0;
                assert_eq!(
                    curve.eval(t).to_bits(),
                    legacy_linear_eval(&raw, t).to_bits(),
                    "N=2 曲线在 t={t} 与旧线性不一致"
                );
            }
        }
    }

    #[test]
    fn pchip_smooth_at_knots() {
        // [(0,0),(0.5,1),(1,0)] 在 x=0.5 两侧数值导数差 < 0.05（导数连续的定性断言）。
        // 旧线性实现此处左右导数是 +2 / −2，差 4 ⇒ 本测试对旧实现必红。
        let curve = Curve::from_value(Some(&json!([[0.0, 0.0], [0.5, 1.0], [1.0, 0.0]])));
        let eps = 1e-3f32;
        let at = curve.eval(0.5);
        let left = (at - curve.eval(0.5 - eps)) / eps;
        let right = (curve.eval(0.5 + eps) - at) / eps;
        assert!(
            (left - right).abs() < 0.05,
            "控制点处左右导数差过大：left={left} right={right}"
        );
    }
}
