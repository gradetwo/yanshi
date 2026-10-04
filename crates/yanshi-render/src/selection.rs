//! 选区几何覆盖度（设计文档 4.4）。
//!
//! 本模块只做**纯几何**：把 `Selection.shape`（`serde_json::Value`）解析成可求值的
//! 形状，并在文档坐标上给出 `0.0 – 1.0` 的覆盖度。它不读系统时间、不碰
//! [`crate::buffer::Buffer`]、不修改任何状态，因此同一输入永远得到同一结果（D0）。
//!
//! 语义采用设计文档的**路线 A：选区约束落笔**——选区只决定"新落笔"的像素权重，
//! 不改写已有内容；组合（`new`/`add`/`subtract`/`intersect`）与反选都在覆盖度上
//! 表达，由调用方乘以笔刷 alpha 即可。本模块不负责与渲染管线接线。
//!
//! 几何契约与蒙版一致：
//!
//! ```json
//! {"kind": "rect" | "ellipse" | "polygon" | "lasso",
//!  "bbox": {"x": 0, "y": 0, "w": 10, "h": 10},
//!  "points": [[x, y], ...]}
//! ```
//!
//! 退化输入一律**不 panic**：非法（kind 缺失/未知、缺少 bbox 等）退化为「覆盖全部」，
//! 面积为零或顶点不足则退化为「空」（覆盖度为 0）。
//!
//! ```
//! use serde_json::json;
//! use yanshi_render::selection::{SelectionSet, SelectionShape};
//!
//! let rect = SelectionShape::from_value(&json!({
//!     "kind": "rect",
//!     "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
//! }));
//! assert_eq!(rect.coverage(2.0, 2.0), 1.0);
//! assert_eq!(rect.coverage(9.0, 9.0), 0.0);
//!
//! // 缺省模式为 new，加一个 subtract 选区挖洞。
//! let mut set = SelectionSet::new();
//! set.push(rect);
//! set.push(SelectionShape::from_value(&json!({
//!     "mode": "subtract",
//!     "kind": "rect",
//!     "bbox": {"x": 0.0, "y": 0.0, "w": 2.0, "h": 4.0},
//! })));
//! assert_eq!(set.coverage(1.0, 2.0), 0.0);
//! assert_eq!(set.coverage(3.0, 2.0), 1.0);
//! ```

use serde_json::Value;

/// 组合模式（设计文档 4.4 的 `mode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// `new`（缺省）：清空之前的结果，再设为该形状。
    #[default]
    New,
    /// `add`：并集，`max(基准, 形状)`。
    Add,
    /// `subtract`：差集，`max(0, 基准 - 形状)`。
    Subtract,
    /// `intersect`：交集，`基准 * 形状`。
    Intersect,
}

impl SelectionMode {
    /// 由 `mode` 字段解析；未知或缺失按设计文档缺省为 [`SelectionMode::New`]。
    pub fn from_str_lossy(mode: Option<&str>) -> Self {
        match mode {
            Some("add") => Self::Add,
            Some("subtract") => Self::Subtract,
            Some("intersect") => Self::Intersect,
            // `new` 与任何未知取值都退化为缺省的替换语义。
            _ => Self::New,
        }
    }
}

/// 解析后的单一选区形状：几何 + 羽化 + 反选 + 组合模式。
///
/// 由 [`SelectionShape::from_value`] 构造；字段公开以便调用方检查与调试，
/// 但覆盖率只由 [`SelectionShape::coverage`] 计算。
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionShape {
    /// 解析后的几何（见 [`SelectionGeom`]）。
    pub geom: SelectionGeom,
    /// 羽化半径（像素）。`<= 0` 视为不羽化；非有限值同样不羽化。
    pub feather: f64,
    /// 反选：为真时覆盖度取 `1 - coverage`。
    pub invert: bool,
    /// 组合模式。
    pub mode: SelectionMode,
}

/// 解析后的选区几何。
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionGeom {
    /// 覆盖全部（非法输入的安全退化：宁可多选，不可误删内容）。
    CoverAll,
    /// 空（面积为 0、顶点不足等退化输入）。
    Empty,
    /// 轴对齐矩形，`(x, y, w, h)`，`w`/`h` 均为正。
    Rect {
        /// 外接盒 `(x, y, w, h)`。
        bbox: (f64, f64, f64, f64),
    },
    /// 椭圆的 bbox 内切椭圆，`(x, y, w, h)`，`w`/`h` 均为正。
    Ellipse {
        /// 外接盒 `(x, y, w, h)`。
        bbox: (f64, f64, f64, f64),
    },
    /// 任意多边形（`polygon` / `lasso`），顶点数 `>= 3`。
    Polygon {
        /// 顶点序列，按输入顺序，隐式闭合。
        points: Vec<(f64, f64)>,
    },
}

impl SelectionShape {
    /// 容错解析 `Selection.shape`（外加 `feather` / `mode` / `invert`）。
    ///
    /// - `kind` 为 `polygon` / `lasso`：顶点少于 3 个退化为空；
    ///   缺少 `points` 则把 `bbox` 当作退化多边形处理（仍为空）。
    /// - `kind` 为 `rect` / `ellipse`：`bbox`（或顶层 `x/y/w/h`）面积为零 / 非有限 → 空。
    /// - `kind` 缺失或未知、或形状对象无法识别 → 覆盖全部（安全侧退化）。
    /// - `feather` 非有限或为负按 0 处理；`invert` 非布尔按 `false`；`mode` 未知按 `new`。
    pub fn from_value(value: &Value) -> Self {
        Self::from_value_with_default(value, SelectionMode::New)
    }

    /// 同 [`SelectionShape::from_value`]，但可以指定 `mode` 缺失时的缺省模式。
    ///
    /// 用于把同一份形状描述直接压入某个组合语境（例如缺省按 `add` 叠加）。
    pub fn from_value_with_default(value: &Value, default_mode: SelectionMode) -> Self {
        let geom = Self::parse_geom(value);
        let feather = value.get("feather").and_then(Value::as_f64).unwrap_or(0.0);
        let mode = SelectionMode::from_str_lossy(value.get("mode").and_then(Value::as_str));
        let mode = if value.get("mode").and_then(Value::as_str).is_none() {
            default_mode
        } else {
            mode
        };
        Self {
            geom,
            feather: if feather.is_finite() && feather > 0.0 {
                feather
            } else {
                0.0
            },
            invert: value
                .get("invert")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            mode,
        }
    }

    /// 解析几何部分。
    fn parse_geom(value: &Value) -> SelectionGeom {
        let Some(kind) = value.get("kind").and_then(Value::as_str) else {
            return SelectionGeom::CoverAll;
        };
        match kind {
            "rect" | "ellipse" => {
                // bbox 缺失说明这不是一个可识别的形状描述；退化为覆盖全部。
                let Some(bbox) = parse_bbox(value) else {
                    return SelectionGeom::CoverAll;
                };
                // 非有限 bbox（NaN 坐标等）与零面积一样是"空选区"，而不是"未指定"。
                if !has_area(bbox) {
                    return SelectionGeom::Empty;
                }
                if kind == "rect" {
                    SelectionGeom::Rect { bbox }
                } else {
                    SelectionGeom::Ellipse { bbox }
                }
            }
            "polygon" | "lasso" => {
                let points = parse_points(value);
                if points.len() < 3 {
                    SelectionGeom::Empty
                } else {
                    SelectionGeom::Polygon { points }
                }
            }
            // 未知 kind（例如魔棒 / 按颜色需要外部解析）：安全侧退化为覆盖全部。
            _ => SelectionGeom::CoverAll,
        }
    }

    /// 覆盖度，`0.0 – 1.0`，含羽化与反选。
    ///
    /// 形状内为 `1.0`、远离形状为 `0.0`；`feather > 0` 时在边界内外各
    /// `feather / 2` 的带宽内用 smoothstep 平滑过渡。`x` / `y` 为 `NaN` 时返回 0。
    pub fn coverage(&self, x: f64, y: f64) -> f32 {
        if x.is_nan() || y.is_nan() {
            // 无法定位的采样点：视为未覆盖（反选后为全覆盖）。
            return if self.invert { 1.0 } else { 0.0 };
        }
        let base = match &self.geom {
            SelectionGeom::CoverAll => 1.0,
            SelectionGeom::Empty => 0.0,
            _ => feather_coverage(self.signed_distance(x, y), self.feather),
        };
        let value = if self.invert { 1.0 - base } else { base };
        clamp01(value)
    }

    /// 到形状边界的有符号距离：内为负、外为正、边界为 0。
    ///
    /// 椭圆的距离用 `|f| / |∇f|` 估计（见函数内注释），多边形用到最近边的
    /// 欧氏距离。`CoverAll` 返回 `f64::NEG_INFINITY`、`Empty` 返回
    /// `f64::INFINITY`，两者都不受羽化影响。
    pub fn signed_distance(&self, x: f64, y: f64) -> f64 {
        match &self.geom {
            SelectionGeom::CoverAll => f64::NEG_INFINITY,
            SelectionGeom::Empty => f64::INFINITY,
            SelectionGeom::Rect { bbox } => rect_signed_distance(*bbox, x, y),
            SelectionGeom::Ellipse { bbox } => ellipse_signed_distance(*bbox, x, y),
            SelectionGeom::Polygon { points } => polygon_signed_distance(points, x, y),
        }
    }
}

/// 多个选区的**按序**组合（设计文档 4.4 的选区栈）。
///
/// 从覆盖度 0 起算，逐个应用 [`SelectionShape::mode`]：`new` 替换、`add` 并集、
/// `subtract` 差集、`intersect` 交集。整个计算是纯函数式的，不含内部缓存，
/// 因此同一组形状与坐标永远得到同一结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SelectionSet {
    /// 按序排列的选区形状。
    shapes: Vec<SelectionShape>,
}

impl SelectionSet {
    /// 空集合（任何位置覆盖度都是 0）。
    pub fn new() -> Self {
        Self { shapes: Vec::new() }
    }

    /// 追加一个选区形状（按序参与组合）。
    pub fn push(&mut self, shape: SelectionShape) {
        self.shapes.push(shape);
    }

    /// 是否没有任何选区形状。
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// 已追加的选区数量。
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// 按序组合后的覆盖度，`0.0 – 1.0`。
    pub fn coverage(&self, x: f64, y: f64) -> f32 {
        let mut base: f32 = 0.0;
        for shape in &self.shapes {
            base = combine(base, shape.coverage(x, y), shape.mode);
        }
        clamp01(base)
    }

    /// 整个包围盒是否**完全覆盖**：返回完全覆盖的包围盒 `(x, y, w, h)`，
    /// 否则返回 `None`。
    ///
    /// 调用方可以用它跳过逐像素裁剪。判定的对象是**有效包围盒**（所有形状
    /// bbox 依 `new`/`add`/`intersect` 组合出的区域；`subtract` 不会扩大它），
    /// 该区域之外覆盖度必然为 0。区域内部用确定性采样验证覆盖度是否恒为 1：
    /// 完全覆盖只可能由不羽化的轴对齐矩形达成，采样足够判定，且不会因浮点
    /// 舍入而误判边界。
    pub fn full_coverage_bbox(&self) -> Option<(f64, f64, f64, f64)> {
        let bbox = self.effective_bbox()?;
        if !has_area(bbox) {
            return None;
        }
        if region_fully_covered(self, bbox, 0) {
            Some(bbox)
        } else {
            None
        }
    }

    /// 覆盖度是否**处处为 0**（完全为空）。
    ///
    /// 在有效包围盒上做确定性采样：只要所有采样点都是 0 就判定为空。空集合、
    /// 退化输入、以及 `subtract` 挖空的结果都会返回 `true`。
    pub fn is_empty_coverage(&self) -> bool {
        const EPS: f32 = 1e-6;
        if self.shapes.is_empty() {
            return true;
        }
        let Some(bbox) = self.effective_bbox() else {
            // 没有任何有限区域可采样：检查形状自身给定的采样点即可。
            return self.shapes.iter().all(|shape| {
                shape_sample_points(shape)
                    .iter()
                    .all(|p| shape.coverage(p.0, p.1) <= EPS)
            });
        };
        if !bbox.2.is_finite() || !bbox.3.is_finite() {
            // 覆盖全部（或某个无界形状）：直接采样原点。
            return self.coverage(0.0, 0.0) <= EPS;
        }
        !region_has_coverage(self, bbox, 0, EPS)
    }

    /// 有效包围盒：按序组合各形状 bbox 得到的保守区域。
    fn effective_bbox(&self) -> Option<(f64, f64, f64, f64)> {
        let mut acc: Option<(f64, f64, f64, f64)> = None;
        for shape in &self.shapes {
            let shape_bbox = shape.geom.bbox()?;
            acc = Some(match (acc, shape.mode) {
                (None, _) | (_, SelectionMode::New) => shape_bbox,
                (Some(base), SelectionMode::Add) => union_bbox(base, shape_bbox),
                (Some(base), SelectionMode::Intersect) => intersect_bbox(base, shape_bbox)?,
                (Some(base), SelectionMode::Subtract) => base,
            });
        }
        acc
    }
}

impl SelectionGeom {
    /// 几何的有限 bbox；`CoverAll` 无有限 bbox（返回 `None` 表示"覆盖全部"）。
    fn bbox(&self) -> Option<(f64, f64, f64, f64)> {
        match self {
            SelectionGeom::Rect { bbox } | SelectionGeom::Ellipse { bbox } => Some(*bbox),
            SelectionGeom::Polygon { points } => {
                let mut min_x = f64::INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for &(x, y) in points {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
                Some((min_x, min_y, max_x - min_x, max_y - min_y))
            }
            SelectionGeom::Empty => Some((0.0, 0.0, 0.0, 0.0)),
            SelectionGeom::CoverAll => None,
        }
    }
}

/// 应用一次组合。
fn combine(base: f32, shape: f32, mode: SelectionMode) -> f32 {
    match mode {
        SelectionMode::New => shape,
        SelectionMode::Add => base.max(shape),
        SelectionMode::Subtract => (base - shape).max(0.0),
        SelectionMode::Intersect => base * shape,
    }
}

/// 由有符号距离与羽化半径给出覆盖度。
///
/// `sdf <= -feather/2` 为 1，`sdf >= +feather/2` 为 0，中间用 smoothstep
/// （`3t² - 2t³`）过渡：单调、连续，且在 `feather = 0` 时退化为硬边阶跃。
fn feather_coverage(sdf: f64, feather: f64) -> f32 {
    if feather <= 0.0 || !feather.is_finite() {
        return if sdf <= 0.0 { 1.0 } else { 0.0 };
    }
    let half = feather / 2.0;
    if sdf <= -half {
        return 1.0;
    }
    if sdf >= half {
        return 0.0;
    }
    // `t` 从内侧的 0 走到外侧的 1；smoothstep 后取反，得到单调递减的覆盖度。
    let t = ((sdf + half) / feather).clamp(0.0, 1.0);
    (1.0 - t * t * (3.0 - 2.0 * t)) as f32
}

/// 轴对齐矩形的有符号距离（外正内负，边界为 0）。
fn rect_signed_distance(bbox: (f64, f64, f64, f64), x: f64, y: f64) -> f64 {
    let (bx, by, bw, bh) = bbox;
    let dx = (bx - x).max(x - (bx + bw));
    let dy = (by - y).max(y - (by + bh));
    match (dx > 0.0, dy > 0.0) {
        (true, true) => (dx * dx + dy * dy).sqrt(),
        (true, false) => dx,
        (false, true) => dy,
        (false, false) => dx.max(dy),
    }
}

/// 椭圆（bbox 内切）的有符号距离估计。
///
/// 用隐函数 `f = (x/a)² + (y/b)² - 1` 与一阶近似 `d ≈ |f| / |∇f|`，
/// 梯度为零时（圆心）退回径向距离。结果是确定性、连续、单调的近似：
/// bbox 角落处为正，圆心处为负，边界处为 0。`a` / `b` 至少一个为正
/// （零面积的 bbox 在解析阶段已被判为空）。
fn ellipse_signed_distance(bbox: (f64, f64, f64, f64), x: f64, y: f64) -> f64 {
    let (bx, by, bw, bh) = bbox;
    let a = bw / 2.0;
    let b = bh / 2.0;
    let nx = (x - (bx + a)) / a;
    let ny = (y - (by + b)) / b;
    let f = nx * nx + ny * ny - 1.0;
    // 椭圆方程 e = (u/a)² + (v/b)²（=1 为边界），∇e = (2u/a², 2v/b²)。
    let gx = 2.0 * nx / a;
    let gy = 2.0 * ny / b;
    let grad = (gx * gx + gy * gy).sqrt();
    if grad > 0.0 && grad.is_finite() {
        f / grad
    } else {
        // 圆心：到边界的距离取半径中的较小者（负号表示在内）。
        -a.min(b)
    }
}

/// 多边形的有符号距离：内为负、外为正。
fn polygon_signed_distance(points: &[(f64, f64)], x: f64, y: f64) -> f64 {
    let mut min_dist = f64::INFINITY;
    let count = points.len();
    let mut j = count - 1;
    for i in 0..count {
        let d = segment_distance(x, y, points[j], points[i]);
        if d < min_dist {
            min_dist = d;
        }
        j = i;
    }
    if in_polygon(x, y, points) {
        -min_dist
    } else {
        min_dist
    }
}

/// 点到线段的最短距离。
fn segment_distance(px: f64, py: f64, a: (f64, f64), b: (f64, f64)) -> f64 {
    let (ax, ay) = a;
    let (bx, by) = b;
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 0.0 {
        // 退化边（重合顶点）：退化为点距。
        return (((px - ax) * (px - ax)) + ((py - ay) * (py - ay))).sqrt();
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    (((px - cx) * (px - cx)) + ((py - cy) * (py - cy))).sqrt()
}

/// 射线法判定内外（水平向右的射线，奇数为内）；**边界点算内**。
fn in_polygon(x: f64, y: f64, points: &[(f64, f64)]) -> bool {
    let count = points.len();
    if count < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = count - 1;
    for i in 0..count {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if point_on_segment(x, y, points[j], points[i]) {
            return true;
        }
        // 半开区间判定，避免顶点被数两次。
        if (yi > y) != (yj > y) {
            let t = (y - yi) / (yj - yi);
            if x < xi + t * (xj - xi) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// 点是否落在线段上（含端点，用相对容差，避免浮点噪声误判）。
fn point_on_segment(px: f64, py: f64, a: (f64, f64), b: (f64, f64)) -> bool {
    let (ax, ay) = a;
    let (bx, by) = b;
    let dx = bx - ax;
    let dy = by - ay;
    let cross = (px - ax) * dy - (py - ay) * dx;
    let scale = 1.0 + dx.abs() + dy.abs();
    if (cross / scale).abs() > 1e-12 {
        return false;
    }
    let dot = (px - ax) * dx + (py - ay) * dy;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 0.0 {
        return px == ax && py == ay;
    }
    dot >= 0.0 && dot <= len_sq
}

/// 解析 bbox：优先 `bbox` 对象，其次顶层 `x`/`y`/`w`/`h`（也接受 `width`/`height`）。
fn parse_bbox(value: &Value) -> Option<(f64, f64, f64, f64)> {
    let source = value.get("bbox").filter(|v| v.is_object()).unwrap_or(value);
    let x = source.get("x").and_then(Value::as_f64);
    let y = source.get("y").and_then(Value::as_f64);
    let w = source
        .get("w")
        .or_else(|| source.get("width"))
        .and_then(Value::as_f64);
    let h = source
        .get("h")
        .or_else(|| source.get("height"))
        .and_then(Value::as_f64);
    match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) => Some((x, y, w, h)),
        _ => None,
    }
}

/// bbox 是否是有有限面积的矩形；NaN 与零/负面积都返回 `false`。
///
/// 用 `x.is_finite()` 显式排除 NaN / 无穷，再用 `w > 0.0` 排除零与负面积。
fn has_area(bbox: (f64, f64, f64, f64)) -> bool {
    let (x, y, w, h) = bbox;
    x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0
}

/// 解析 `points`，跳过非法项（非数组、长度不足、非数值、非有限坐标）。
fn parse_points(value: &Value) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    if let Some(array) = value.get("points").and_then(Value::as_array) {
        for item in array {
            if let Some(pair) = item.as_array() {
                if pair.len() >= 2 {
                    if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                        if x.is_finite() && y.is_finite() {
                            points.push((x, y));
                        }
                    }
                }
            }
        }
    }
    points
}

/// 钳到 `0..=1`，把 `NaN` 归为 0（保证覆盖度永远是有限值）。
fn clamp01(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// 两个 bbox 的并集。
fn union_bbox(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let x0 = a.0.min(b.0);
    let y0 = a.1.min(b.1);
    let x1 = (a.0 + a.2).max(b.0 + b.2);
    let y1 = (a.1 + a.3).max(b.1 + b.3);
    (x0, y0, x1 - x0, y1 - y0)
}

/// 两个 bbox 的交集；无交集返回 `None`。
fn intersect_bbox(
    a: (f64, f64, f64, f64),
    b: (f64, f64, f64, f64),
) -> Option<(f64, f64, f64, f64)> {
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    if x1 <= x0 || y1 <= y0 {
        None
    } else {
        Some((x0, y0, x1 - x0, y1 - y0))
    }
}

/// 一个区域内用于采样判定的关键点：四角、四边中点、中心。
fn region_samples(bbox: (f64, f64, f64, f64)) -> [(f64, f64); 9] {
    let (x, y, w, h) = bbox;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;
    [
        (x, y),
        (x + w, y),
        (x, y + h),
        (x + w, y + h),
        (cx, y),
        (cx, y + h),
        (x, cy),
        (x + w, cy),
        (cx, cy),
    ]
}

/// 某个形状自身的采样点：bbox 角点 / 中心 + 多边形顶点与边中点。
fn shape_sample_points(shape: &SelectionShape) -> Vec<(f64, f64)> {
    let mut points = Vec::new();
    if let Some(bbox) = shape.geom.bbox() {
        points.extend(region_samples(bbox));
    } else {
        points.push((0.0, 0.0));
    }
    if let SelectionGeom::Polygon { points: vertices } = &shape.geom {
        let count = vertices.len();
        for i in 0..count {
            let a = vertices[i];
            let b = vertices[(i + 1) % count];
            points.push(a);
            points.push(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0));
        }
    }
    points
}

/// 区域是否**完全覆盖**（覆盖度恒为 1）；子区域递归细化。
fn region_fully_covered(set: &SelectionSet, bbox: (f64, f64, f64, f64), depth: u32) -> bool {
    const MAX_DEPTH: u32 = 6;
    let covered = region_samples(bbox)
        .iter()
        .all(|&(x, y)| set.coverage(x, y) >= 1.0);
    if !covered {
        return false;
    }
    if depth >= MAX_DEPTH || bbox.2 <= 0.5 || bbox.3 <= 0.5 {
        return true;
    }
    let half_w = bbox.2 / 2.0;
    let half_h = bbox.3 / 2.0;
    let cells = [
        (bbox.0, bbox.1, half_w, half_h),
        (bbox.0 + half_w, bbox.1, half_w, half_h),
        (bbox.0, bbox.1 + half_h, half_w, half_h),
        (bbox.0 + half_w, bbox.1 + half_h, half_w, half_h),
    ];
    cells
        .iter()
        .all(|&cell| region_fully_covered(set, cell, depth + 1))
}

/// 区域是否存在覆盖度大于 `eps` 的点；子区域递归细化。
fn region_has_coverage(
    set: &SelectionSet,
    bbox: (f64, f64, f64, f64),
    depth: u32,
    eps: f32,
) -> bool {
    const MAX_DEPTH: u32 = 6;
    if region_samples(bbox)
        .iter()
        .any(|&(x, y)| set.coverage(x, y) > eps)
    {
        return true;
    }
    if depth >= MAX_DEPTH || bbox.2 <= 0.5 || bbox.3 <= 0.5 {
        return false;
    }
    let half_w = bbox.2 / 2.0;
    let half_h = bbox.3 / 2.0;
    let cells = [
        (bbox.0, bbox.1, half_w, half_h),
        (bbox.0 + half_w, bbox.1, half_w, half_h),
        (bbox.0, bbox.1 + half_h, half_w, half_h),
        (bbox.0 + half_w, bbox.1 + half_h, half_w, half_h),
    ];
    cells
        .iter()
        .any(|&cell| region_has_coverage(set, cell, depth + 1, eps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 采样点比较的容差。
    const TOL: f32 = 1e-6;

    fn shape(value: Value) -> SelectionShape {
        SelectionShape::from_value(&value)
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= TOL,
            "期望 {expected}，实际 {actual}"
        );
    }

    fn rect4() -> SelectionShape {
        shape(json!({"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}}))
    }

    #[test]
    fn rect_coverage_at_corners_center_and_outside() {
        let s = rect4();
        // 四角与中心都是 1.0。
        assert_eq!(s.coverage(0.0, 0.0), 1.0);
        assert_eq!(s.coverage(4.0, 0.0), 1.0);
        assert_eq!(s.coverage(0.0, 4.0), 1.0);
        assert_eq!(s.coverage(4.0, 4.0), 1.0);
        assert_eq!(s.coverage(2.0, 2.0), 1.0);
        // 边界内外的覆盖度。
        assert_eq!(s.coverage(3.999, 2.0), 1.0);
        assert_eq!(s.coverage(-0.001, 2.0), 0.0);
        assert_eq!(s.coverage(4.001, 2.0), 0.0);
        assert_eq!(s.coverage(2.0, -1.0), 0.0);
        assert_eq!(s.coverage(9.0, 9.0), 0.0);
    }

    #[test]
    fn rect_parses_top_level_bbox_aliases() {
        let s = shape(json!({"kind": "rect", "x": 1.0, "y": 1.0, "width": 2.0, "height": 2.0}));
        assert_eq!(s.coverage(2.0, 2.0), 1.0);
        assert_eq!(s.coverage(0.5, 0.5), 0.0);
        // 缺少 bbox 的 rect：安全侧退化为覆盖全部。
        let all = shape(json!({"kind": "rect"}));
        assert_eq!(all.coverage(-100.0, 500.0), 1.0);
    }

    #[test]
    fn ellipse_center_inside_and_corner() {
        // 4×4 的 bbox：圆心 (2,2)、半径 2。
        let s = shape(json!({"kind": "ellipse", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}}));
        // 中心。
        assert_eq!(s.coverage(2.0, 2.0), 1.0);
        // 内切点（上下左右四个切点恰在边界上，算内）。
        assert_eq!(s.coverage(1.0, 2.0), 1.0);
        assert_eq!(s.coverage(3.0, 2.0), 1.0);
        assert_eq!(s.coverage(2.0, 0.0), 1.0);
        assert_eq!(s.coverage(2.0, 4.0), 1.0);
        // bbox 四角必须严格小于 1（落在椭圆外）。
        for &(x, y) in &[(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (4.0, 4.0)] {
            assert!(s.coverage(x, y) < 1.0, "角落 ({x},{y}) 不应完全覆盖");
            assert_eq!(s.coverage(x, y), 0.0);
        }
        // 长轴示例：4×2 的椭圆，角落在椭圆外、中心在内。
        let e2 =
            shape(json!({"kind": "ellipse", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 2.0}}));
        assert_eq!(e2.coverage(2.0, 1.0), 1.0);
        assert_eq!(e2.coverage(1.0, 1.0), 1.0);
        assert_eq!(e2.coverage(0.0, 0.0), 0.0);
        assert_eq!(e2.coverage(2.0, 0.0), 1.0);
    }

    #[test]
    fn polygon_inside_outside_and_edge() {
        let s = shape(json!({
            "kind": "polygon",
            "points": [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]
        }));
        // 内点。
        assert_eq!(s.coverage(2.0, 2.0), 1.0);
        assert_eq!(s.coverage(1.5, 3.5), 1.0);
        // 外点。
        assert_eq!(s.coverage(5.0, 2.0), 0.0);
        assert_eq!(s.coverage(2.0, -0.5), 0.0);
        assert_eq!(s.coverage(-1.0, -1.0), 0.0);
        // 边界点算内：边中点、顶点、边上的非顶点。
        assert_eq!(s.coverage(2.0, 0.0), 1.0);
        assert_eq!(s.coverage(0.0, 0.0), 1.0);
        assert_eq!(s.coverage(0.0, 2.5), 1.0);
        assert_eq!(s.coverage(4.0, 4.0), 1.0);
    }

    #[test]
    fn polygon_boundary_vertex_on_edge_counts_as_inside() {
        // W 形（凹）多边形：顶点 (1,2) 恰好落在边 (1,2)-(1,5) 的端点上，
        // 属于"顶点落在另一条边上"的退化情形。显式的边界判定保证它算内，
        // 不依赖射线法在这种顶点上的具体数法。
        let s = shape(json!({
            "kind": "polygon",
            "points": [[0.0, 0.0], [4.0, 0.0], [2.0, 1.0], [1.0, 2.0],
                       [1.0, 5.0], [3.0, 5.0], [3.0, 2.0], [2.0, 3.0]]
        }));
        assert_eq!(s.signed_distance(1.0, 2.0), 0.0); // 落在边界上
        assert_eq!(s.coverage(1.0, 2.0), 1.0); // 边界算内
        assert_eq!(s.coverage(1.0, 3.5), 1.0); // 同一条边上的普通点
        assert_eq!(s.coverage(2.5, 2.2), 0.0); // 凹口（形状外）
        assert_eq!(s.coverage(2.0, 4.0), 1.0); // 凹口上方的本体
    }

    #[test]
    fn lasso_is_treated_as_closed_polygon() {
        // 三角形：(0,0) (4,0) (0,4)。
        let s = shape(json!({"kind": "lasso", "points": [[0.0, 0.0], [4.0, 0.0], [0.0, 4.0]]}));
        assert_eq!(s.coverage(1.0, 1.0), 1.0);
        assert_eq!(s.coverage(1.5, 1.5), 1.0); // 恰在斜边 x+y=4 上 → 算内
        assert_eq!(s.coverage(2.0, 2.0), 1.0); // 斜边外侧（x+y=4 之上）
        assert_eq!(s.coverage(3.5, 3.5), 0.0);
        assert_eq!(s.coverage(-0.5, 1.0), 0.0);
    }

    #[test]
    fn feather_is_monotonic_and_continuous() {
        // 4×4 矩形，羽化 2.0：内缩 1.0 以内为满，外扩 1.0 以外为 0。
        let s = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 2.0
        }));
        // 边界内在半带宽（1.0）以内为满覆盖。
        assert_eq!(s.coverage(1.5, 2.0), 1.0); // sdf = -1.5
        assert_eq!(s.coverage(1.0, 2.0), 1.0); // sdf = -1.0 → 恰好满覆盖
        assert_near(s.coverage(0.9, 2.0), 0.99275); // sdf = -0.9, t = 0.05
        assert_near(s.coverage(0.75, 2.0), 0.957_031_25); // sdf = -0.75, t = 0.125
        assert_near(s.coverage(0.5, 2.0), 0.84375); // sdf = -0.5, t = 0.25
        assert_near(s.coverage(0.25, 2.0), 0.683_593_75); // sdf = -0.25, t = 0.375
                                                          // 边界上正好一半。
        assert_near(s.coverage(0.0, 2.0), 0.5);
        // 边界外随距离单调下降。
        assert_near(s.coverage(-0.25, 2.0), 0.316_406_25); // sdf = 0.25, t = 0.625
        assert_near(s.coverage(-0.5, 2.0), 0.15625); // sdf = 0.5, t = 0.75
        assert_near(s.coverage(-0.75, 2.0), 0.042_968_75); // sdf = 0.75, t = 0.875
        assert_eq!(s.coverage(-1.0, 2.0), 0.0); // sdf = 1.0 → 恰好在外部半带宽处
        let mut previous = 1.0f32;
        for step in 0..=40 {
            // 从矩形内部 (1.5, 2.0) 一路穿过左边界到 (-0.5, 2.0)：覆盖度只降不升。
            let x = 1.5 - step as f64 * 0.05;
            let c = s.coverage(x, 2.0);
            assert!(
                c <= previous + TOL,
                "x={x} 处覆盖度非单调：{previous} → {c}"
            );
            assert!((0.0..=1.0).contains(&c));
            previous = c;
        }
        // 远离处为 0。
        assert_eq!(s.coverage(-1.5, 2.0), 0.0);
        assert_eq!(s.coverage(-3.0, 2.0), 0.0);
        assert_eq!(s.coverage(2.0, 9.0), 0.0);
        // 连续：相邻采样差值有界。
        let a = s.coverage(0.01, 2.0);
        let b = s.coverage(0.02, 2.0);
        assert!((a - b).abs() < 0.01, "0.01 步长内跳变过大：{a} vs {b}");
    }

    #[test]
    fn feather_on_ellipse_and_polygon_shrinks_and_grows() {
        // 椭圆羽化 1.0：切点 (0,2) 恰在边界 → 0.5；内缩 0.5 以上为满。
        let e = shape(json!({
            "kind": "ellipse",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 1.0
        }));
        assert_near(e.coverage(0.0, 2.0), 0.5);
        assert_eq!(e.coverage(0.6, 2.0), 1.0);
        assert_eq!(e.coverage(-1.0, 2.0), 0.0);
        // 多边形羽化 2.0：正方形边界处 0.5，顶点外对角方向逐渐归零。
        let p = shape(json!({
            "kind": "polygon",
            "points": [[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]],
            "feather": 2.0
        }));
        assert_near(p.coverage(0.0, 2.0), 0.5);
        assert_eq!(p.coverage(1.0, 2.0), 1.0);
        assert_eq!(p.coverage(-1.0, 2.0), 0.0);
    }

    #[test]
    fn invert_is_exactly_one_minus_coverage() {
        let plain = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 2.0
        }));
        let inverted = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 2.0,
            "invert": true
        }));
        for &(x, y) in &[
            (2.0, 2.0),
            (0.5, 2.0),
            (0.0, 2.0),
            (-0.5, 2.0),
            (-0.9, 2.0),
            (-3.0, 2.0),
            (9.0, 9.0),
        ] {
            assert_eq!(
                inverted.coverage(x, y),
                1.0 - plain.coverage(x, y),
                "({x},{y}) 处反选不等于 1 - coverage"
            );
        }
        // 具体数值：边界上 0.5 → 0.5，内部 1 → 0，外部 0 → 1。
        assert_near(inverted.coverage(0.0, 2.0), 0.5);
        assert_eq!(inverted.coverage(2.0, 2.0), 0.0);
        assert_eq!(inverted.coverage(-3.0, 2.0), 1.0);
    }

    #[test]
    fn selection_set_combine_modes() {
        // 基准 A：[0,4]×[0,4]；形状 B：[2,6]×[0,4]。
        let a = rect4();
        let b = shape(json!({"kind": "rect", "bbox": {"x": 2.0, "y": 0.0, "w": 4.0, "h": 4.0}}));

        // add：并集。
        let mut add = SelectionSet::new();
        add.push(a.clone());
        let mut b_add = b.clone();
        b_add.mode = SelectionMode::Add;
        add.push(b_add);
        assert_eq!(add.coverage(1.0, 2.0), 1.0); // 只在 A
        assert_eq!(add.coverage(5.0, 2.0), 1.0); // 只在 B
        assert_eq!(add.coverage(3.0, 2.0), 1.0); // 交集
        assert_eq!(add.coverage(7.0, 2.0), 0.0); // 都不在

        // subtract：差集。
        let mut sub = SelectionSet::new();
        sub.push(a.clone());
        let mut b_sub = b.clone();
        b_sub.mode = SelectionMode::Subtract;
        sub.push(b_sub);
        assert_eq!(sub.coverage(1.0, 2.0), 1.0);
        assert_eq!(sub.coverage(3.0, 2.0), 0.0);
        assert_eq!(sub.coverage(5.0, 2.0), 0.0);
        assert_eq!(sub.coverage(2.0, 2.0), 0.0); // 边界被减去
        assert_eq!(sub.coverage(1.999, 2.0), 1.0);

        // intersect：交集。
        let mut inter = SelectionSet::new();
        inter.push(a.clone());
        let mut b_inter = b.clone();
        b_inter.mode = SelectionMode::Intersect;
        inter.push(b_inter);
        assert_eq!(inter.coverage(1.0, 2.0), 0.0);
        assert_eq!(inter.coverage(3.0, 2.0), 1.0);
        assert_eq!(inter.coverage(5.0, 2.0), 0.0);

        // new（缺省）：替换之前的结果。
        let mut fresh = SelectionSet::new();
        fresh.push(a.clone());
        fresh.push(b.clone()); // mode 缺省 → new
        assert_eq!(fresh.coverage(1.0, 2.0), 0.0);
        assert_eq!(fresh.coverage(5.0, 2.0), 1.0);

        // 显式 new 也替换。
        let mut explicit = SelectionSet::new();
        explicit.push(a.clone());
        let mut b_new = b.clone();
        b_new.mode = SelectionMode::New;
        explicit.push(b_new);
        assert_eq!(explicit.coverage(1.0, 2.0), 0.0);
        assert_eq!(explicit.coverage(5.0, 2.0), 1.0);

        // 空集合。
        let empty = SelectionSet::new();
        assert!(empty.is_empty());
        assert_eq!(empty.coverage(2.0, 2.0), 0.0);

        // 未知 mode 退化为 new。
        let mut unknown = SelectionSet::new();
        unknown.push(a);
        unknown.push(shape(json!({
            "mode": "multiply",
            "kind": "rect",
            "bbox": {"x": 2.0, "y": 0.0, "w": 4.0, "h": 4.0}
        })));
        assert_eq!(unknown.coverage(1.0, 2.0), 0.0);
        assert_eq!(unknown.coverage(5.0, 2.0), 1.0);
    }

    #[test]
    fn selection_set_full_coverage_and_empty_queries() {
        let mut set = SelectionSet::new();
        set.push(shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0}
        })));
        assert_eq!(set.full_coverage_bbox(), Some((0.0, 0.0, 8.0, 8.0)));
        assert!(!set.is_empty_coverage());

        // 挖掉一角后不再是完全覆盖。
        set.push(shape(json!({
            "mode": "subtract",
            "kind": "rect",
            "bbox": {"x": 6.0, "y": 6.0, "w": 2.0, "h": 2.0}
        })));
        assert_eq!(set.full_coverage_bbox(), None);
        assert!(!set.is_empty_coverage());

        // 完全挖空。
        let mut hollow = SelectionSet::new();
        hollow.push(shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}
        })));
        hollow.push(shape(json!({
            "mode": "subtract",
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}
        })));
        assert!(hollow.is_empty_coverage());
        assert_eq!(hollow.full_coverage_bbox(), None);

        // 羽化后的矩形不再"完全覆盖"。
        let mut feathered = SelectionSet::new();
        feathered.push(shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 1.0
        })));
        assert_eq!(feathered.full_coverage_bbox(), None);
        assert!(!feathered.is_empty_coverage());

        // 非矩形（椭圆）永不完全覆盖自己的 bbox。
        let mut ellipse = SelectionSet::new();
        ellipse.push(shape(json!({
            "kind": "ellipse",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}
        })));
        assert_eq!(ellipse.full_coverage_bbox(), None);
        assert_eq!(SelectionSet::new().full_coverage_bbox(), None);
        assert!(SelectionSet::new().is_empty_coverage());
    }

    #[test]
    fn degenerate_inputs_do_not_panic() {
        // 空 bbox / 零面积 → 空选区，不 panic。
        let zero = shape(json!({"kind": "rect", "bbox": {"x": 1.0, "y": 1.0, "w": 0.0, "h": 4.0}}));
        assert_eq!(zero.coverage(1.0, 2.0), 0.0);
        assert_eq!(zero.coverage(1.0, 1.0), 0.0);
        let negative =
            shape(json!({"kind": "ellipse", "bbox": {"x": 0.0, "y": 0.0, "w": -3.0, "h": 2.0}}));
        assert_eq!(negative.coverage(0.0, 0.0), 0.0);
        // 单点 / 两点 polygon → 空。
        let one = shape(json!({"kind": "polygon", "points": [[1.0, 1.0]]}));
        assert_eq!(one.coverage(1.0, 1.0), 0.0);
        let two = shape(json!({"kind": "lasso", "points": [[0.0, 0.0], [1.0, 1.0]]}));
        assert_eq!(two.coverage(0.5, 0.5), 0.0);
        // 空 points 数组 / 非数组 points。
        let none = shape(json!({"kind": "polygon", "points": []}));
        assert_eq!(none.coverage(0.0, 0.0), 0.0);
        let junk = shape(json!({"kind": "polygon", "points": "oops"}));
        assert_eq!(junk.coverage(0.0, 0.0), 0.0);
        // 完全非法的输入 → 覆盖全部。
        assert_eq!(shape(json!({})).coverage(3.0, 3.0), 1.0);
        assert_eq!(shape(json!(42)).coverage(3.0, 3.0), 1.0);
        assert_eq!(shape(json!(null)).coverage(0.0, 0.0), 1.0);
        assert_eq!(shape(json!({"kind": "magic_wand"})).coverage(0.0, 0.0), 1.0);
        assert_eq!(
            shape(json!({"kind": "rect", "bbox": "no"})).coverage(0.0, 0.0),
            1.0
        );
        // NaN 输入不 panic。
        let rect = rect4();
        assert_eq!(rect.coverage(f64::NAN, 2.0), 0.0);
        assert_eq!(rect.coverage(2.0, f64::NAN), 0.0);
        assert_eq!(rect.coverage(f64::NAN, f64::NAN), 0.0);
        assert_eq!(rect.coverage(f64::INFINITY, 2.0), 0.0);
        assert_eq!(rect.coverage(f64::NEG_INFINITY, 2.0), 0.0);
        // NaN 反选：未覆盖 → 全覆盖。
        let inv = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "invert": true
        }));
        assert_eq!(inv.coverage(f64::NAN, 0.0), 1.0);
        // 说明：serde_json 无法表示 NaN（`json!(f64::NAN)` 会变成 `null`），
        // 因此"NaN bbox"只能以 null 的形式进入解析——此时按"缺字段"退化为覆盖全部。
        let null_bbox = shape(json!({
            "kind": "rect",
            "bbox": {"x": null, "y": 0.0, "w": 4.0, "h": 4.0}
        }));
        assert_eq!(null_bbox.coverage(2.0, 2.0), 1.0);
        // NaN 只能出现在坐标参数上：不 panic，且对整个 SelectionSet 也安全。
        let mut nan_set = SelectionSet::new();
        nan_set.push(rect4());
        assert_eq!(nan_set.coverage(f64::NAN, 2.0), 0.0);
        assert_eq!(nan_set.coverage(2.0, f64::NAN), 0.0);
        assert_eq!(nan_set.coverage(f64::INFINITY, f64::NEG_INFINITY), 0.0);
        // NaN feather 按不羽化处理。
        let nan_feather = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": f64::NAN
        }));
        assert_eq!(nan_feather.coverage(0.0, 0.0), 1.0);
        assert_eq!(nan_feather.coverage(4.01, 0.0), 0.0);
        // 退化多边形的覆盖度查询也不 panic。
        let set = {
            let mut set = SelectionSet::new();
            set.push(one);
            set.push(none);
            set.push(zero);
            set
        };
        let _ = set.coverage(f64::NAN, 3.0);
    }

    #[test]
    fn feather_zero_matches_hard_edge() {
        let hard = rect4();
        let soft_zero = shape(json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0},
            "feather": 0.0
        }));
        for &(x, y) in &[(0.0, 0.0), (2.0, 2.0), (4.0, 4.0), (4.5, 2.0), (-1.0, 2.0)] {
            assert_eq!(hard.coverage(x, y), soft_zero.coverage(x, y));
        }
    }
}
