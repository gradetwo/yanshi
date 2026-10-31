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
//! {"kind": "rect" | "ellipse" | "polygon" | "lasso" | "path",
//!  "bbox": {"x": 0, "y": 0, "w": 10, "h": 10},
//!  "points": [[x, y], ...],
//!  "subpaths": [{"start": [x, y], "closed": true, "segments": [...]}],
//!  "fill_rule": "evenodd" | "nonzero"}
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
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::png::decode_png_l8;
use yanshi_core::{MaskPatchOp, Selection};

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
    /// 线性渐变遮罩（D1-b）：沿 `start`→`end` 的投影做 smoothstep 过渡。
    ///
    /// `start` 一侧为 1、`end` 一侧为 0；过渡带就是 `start`→`end` 的距离本身，
    /// 不另设羽化（顶层 `feather` 不作用于渐变 kind，见 [`SelectionShape::coverage`]；
    /// `create_mask` 入口要求渐变 kind 的 `feather` 为 0）。
    LinearGradient {
        /// 渐变起点（覆盖度 1 的一侧），文档坐标。
        start: (f64, f64),
        /// 渐变终点（覆盖度 0 的一侧），文档坐标。
        end: (f64, f64),
    },
    /// 径向渐变遮罩（D1-b）：`bbox` 内切椭圆，中心为 1、边缘外为 0。
    ///
    /// `falloff` ∈ [0, 1] 是过渡带占半径的比例（LR feather 口径）：
    /// 0 = 半径处硬边，1 = 从中心到边缘全程渐变；顶层 `feather` 同样不作用于
    /// 渐变 kind（与 [`SelectionGeom::LinearGradient`] 同例）。
    RadialGradient {
        /// 外接盒 `(x, y, w, h)`，文档坐标。
        bbox: (f64, f64, f64, f64),
        /// 过渡带占半径比例，`0..=1`。
        falloff: f64,
    },
    /// 任意多边形（`polygon` / `lasso`），顶点数 `>= 3`。
    Polygon {
        /// 顶点序列，按输入顺序，隐式闭合。
        points: Vec<(f64, f64)>,
    },
    /// 曲线路径（`kind == "path"`，与一期 path 形状同构的载荷）。
    ///
    /// 解析时走 `object::parse_path_subpaths`（与 `draw_shape` 共用的同一份
    /// 校验），再用 `path::flatten_subpath`（[`crate::path::FLATTEN_TOL`]）在
    /// 解析期展平一次存环；覆盖率的内外判定走 `path::rings_fill_contains`——
    /// 与 `path::path_fill_contains` 同一条填充路径，禁止另写展平 / 填充。
    /// 校验失败（载荷非法）退化为 [`Self::Empty`]：已识别的 kind 载荷坏掉时
    /// 宁可不选、不可错选（与 `Bitmap` 缺 blob 同例），绝不退化为覆盖全部。
    Path {
        /// 展平后的环列（每条子路径一环；填充语义隐式闭合，与 SVG 一致）。
        rings: Vec<Vec<(f64, f64)>>,
        /// 填充规则（`evenodd` / `nonzero`）。
        fill_rule: crate::path::FillRule,
    },
    /// 位图选区（`smart_select` 魔棒生成）：`kind == "bitmap"`。
    ///
    /// 覆盖率走 O(1) 查表（`coverage` 内零锁、零解码）；`feather` 已烘焙进位图
    /// （`shape` 标记 `"feather_baked": true`），渲染期不再重复羽化。
    /// `mask` 为 `None` 表示 blob 未注册成功，覆盖率退化为 0
    /// （宁可不选、不可错选）。
    Bitmap {
        /// 位图在文档坐标中的紧致外接盒 `[bx, by, bw, bh]`（像素）。
        bbox: [i32; 4],
        /// 已解码的位图（`from_value` 构造时由注册表单次绑定）。
        mask: Option<Arc<DecodedBitmapMask>>,
    },
}

impl SelectionShape {
    /// 容错解析 `Selection.shape`（外加 `feather` / `mode` / `invert`）。
    ///
    /// - `kind` 为 `polygon` / `lasso`：顶点少于 3 个退化为空；
    ///   缺少 `points` 则把 `bbox` 当作退化多边形处理（仍为空）。
    /// - `kind` 为 `path`：载荷走 `object::parse_path_subpaths` 校验，校验失败
    ///   或展平后无有效环退化为空（不退化为覆盖全部，理由见 [`SelectionGeom::Path`]）。
    /// - `kind` 为 `rect` / `ellipse`：`bbox`（或顶层 `x/y/w/h`）面积为零 / 非有限 → 空。
    /// - `kind` 缺失或未知、或形状对象无法识别 → 覆盖全部（安全侧退化）。
    /// - `feather` 非有限或为负按 0 处理；`invert` 非布尔按 `false`；`mode` 未知按 `new`。
    pub fn from_value(value: &Value) -> Self {
        Self::from_value_with_default(value, SelectionMode::New)
    }

    /// 由完整的 `yanshi_core::Selection` 构造（推荐渲染路径使用）。
    ///
    /// 背景：`create_selection` 把 `feather`/`mode`/`invert` 存在原子 payload
    /// **顶层**，`fold` 存进 `Selection` struct 的同名字段，而 `Selection.shape`
    /// 只有几何。直接调 `from_value(&sel.shape)` 会静默丢弃三者
    /// （退化为 feather=0 硬边、mode=new 替换、invert=false），
    /// 即"选区合并/排除/相交/反选/羽化参数收了但渲染时不生效"。
    /// 本函数先把三者并入 JSON 再解析。
    pub fn from_selection(sel: &yanshi_core::Selection) -> Self {
        let mut v = sel.shape.clone();
        if let Some(obj) = v.as_object_mut() {
            obj.insert("feather".to_owned(), serde_json::json!(sel.feather));
            obj.insert("mode".to_owned(), serde_json::json!(sel.mode));
            obj.insert("invert".to_owned(), serde_json::json!(sel.invert));
        }
        Self::from_value(&v)
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
    ///
    /// Q2 schema 统一：`kind` 与 `type` 兼容读取（`smart_*` 用 `type`），
    /// `circle` 与 `ellipse` 视为同义词。
    fn parse_geom(value: &Value) -> SelectionGeom {
        let Some(kind) = value
            .get("kind")
            .or_else(|| value.get("type"))
            .and_then(Value::as_str)
            .map(|k| if k == "circle" { "ellipse" } else { k })
        else {
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
            "linear_gradient" => {
                // 已识别 kind 但载荷坏掉 ⇒ 空（宁可不选、不可错选；与 path/bitmap 同例）。
                // 起点/终点缺失、非有限、或重合（零长轴）都算坏载荷。
                let (Some(start), Some(end)) =
                    (parse_point(value, "start"), parse_point(value, "end"))
                else {
                    return SelectionGeom::Empty;
                };
                if start == end {
                    return SelectionGeom::Empty;
                }
                SelectionGeom::LinearGradient { start, end }
            }
            "radial_gradient" => {
                // bbox 缺失/零面积 ⇒ 空；falloff 缺失按 0（硬边），非法值钳到 [0, 1]。
                let Some(bbox) = parse_bbox(value) else {
                    return SelectionGeom::Empty;
                };
                if !has_area(bbox) {
                    return SelectionGeom::Empty;
                }
                let falloff = value.get("falloff").and_then(Value::as_f64).unwrap_or(0.0);
                SelectionGeom::RadialGradient {
                    bbox,
                    falloff: if falloff.is_finite() {
                        falloff.clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                }
            }
            "path" => {
                // 与一期 path 形状共用同一份校验（`object::parse_path_subpaths`，
                // 禁止再写第二套）；展平只在解析期做一次（`path::FLATTEN_TOL`）。
                match crate::object::parse_path_subpaths(value) {
                    Ok(parsed) => {
                        let rings: Vec<Vec<(f64, f64)>> = parsed
                            .subpaths
                            .iter()
                            .map(|sub| crate::path::flatten_subpath(sub, crate::path::FLATTEN_TOL))
                            .collect();
                        if rings.iter().all(|ring| ring.len() < 2) {
                            SelectionGeom::Empty
                        } else {
                            SelectionGeom::Path {
                                rings,
                                fill_rule: parsed.fill_rule,
                            }
                        }
                    }
                    // 已识别的 kind 载荷非法 ⇒ 空（宁可不选、不可错选）。
                    Err(_) => SelectionGeom::Empty,
                }
            }
            "bitmap" => {
                // 位图选区：紧致 bbox + blob 哈希；掩膜由全局注册表单次绑定。
                // 注意：绝不能复用 "magic_wand" 这个 kind（现有单测断言它退化全选）。
                let bbox = value
                    .get("bbox")
                    .and_then(Value::as_array)
                    .map(|arr| {
                        let n = |i: usize| arr.get(i).and_then(Value::as_i64).unwrap_or(0) as i32;
                        [n(0), n(1), n(2).max(0), n(3).max(0)]
                    })
                    .unwrap_or([0, 0, 0, 0]);
                if bbox[2] <= 0 || bbox[3] <= 0 {
                    return SelectionGeom::Empty;
                }
                // **`blob` 与 `blob_hash` 都要认** ✗（第 2665 轮；与第 1607 轮**同一族**的第三次 ✓）：
                //   `ensure_bitmap_masks_registered` 那边**已经**两种键都收 ✓，
                //   而**这里**（真正查表的地方）**只认 `blob`** ✗ ⇒ 选区存的是 `blob_hash`
                //   ⇒ `mask = None` ⇒ **覆盖率恒 0 ⇒ `ok:true` 而一个像素不落** ✗✗
                //   （实测：`create_mask{kind:bitmap}` ＋ `from_mask` ＋ `clip_to_selection` ⇒
                //    `painted_pixels_clipped=0`、`clip_coverage=0.0`、选区 `area=0.0` ✓）
                let mask = value
                    .get("blob")
                    .or_else(|| value.get("blob_hash"))
                    .and_then(Value::as_str)
                    .and_then(get_bitmap_mask);
                SelectionGeom::Bitmap { bbox, mask }
            }
            // 未知 kind（例如魔棒 / 按颜色需要外部解析）：安全侧退化为覆盖全部。
            _ => SelectionGeom::CoverAll,
        }
    }

    /// 覆盖度，`0.0 – 1.0`，含羽化与反选。
    ///
    /// 形状内为 `1.0`、远离形状为 `0.0`；`feather > 0` 时在边界内外各
    /// `feather / 2` 的带宽内用 smoothstep 平滑过渡。`x` / `y` 为 `NaN` 时返回 0。
    ///
    /// 渐变 kind（`linear_gradient` / `radial_gradient`）与位图一样不走羽化：
    /// 过渡带由形状自身定义（线性：`start`→`end` 的距离；径向：`falloff` 比例）。
    pub fn coverage(&self, x: f64, y: f64) -> f32 {
        if x.is_nan() || y.is_nan() {
            // 无法定位的采样点：视为未覆盖（反选后为全覆盖）。
            return if self.invert { 1.0 } else { 0.0 };
        }
        // 位图选区：O(1) 查表（2 次整数比较 + 1 次数组下标），零锁、零解码。
        // `feather` 已烘焙进位图（`feather_baked`），这里不再重复羽化。
        if let SelectionGeom::Bitmap { bbox, mask } = &self.geom {
            let base = bitmap_coverage(bbox, mask.as_deref(), x, y);
            let value = if self.invert { 1.0 - base } else { base };
            return clamp01(value);
        }
        // 渐变遮罩（D1-b）：过渡带就是渐变本身，顶层 `feather` 不再重复作用
        // （与位图同例；`create_mask` 入口要求渐变 kind 的 `feather` 为 0）。
        if let SelectionGeom::LinearGradient { start, end } = &self.geom {
            let base = linear_gradient_coverage(*start, *end, x, y);
            let value = if self.invert { 1.0 - base } else { base };
            return clamp01(value);
        }
        if let SelectionGeom::RadialGradient { bbox, falloff } = &self.geom {
            let base = radial_gradient_coverage(*bbox, *falloff, x, y);
            let value = if self.invert { 1.0 - base } else { base };
            return clamp01(value);
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
            SelectionGeom::Path { rings, fill_rule } => {
                path_signed_distance(rings, *fill_rule, x, y)
            }
            // 位图选区不走有符号距离（`coverage` 直接查表）；这里返回 +∞
            // 表示「无穷远外部」，避免任何调用方误算出覆盖。
            SelectionGeom::Bitmap { .. } => f64::INFINITY,
            // 渐变遮罩不走有符号距离（`coverage` 直接按渐变求值）；这里返回 +∞
            // 表示「无穷远外部」，避免任何调用方误算出覆盖。
            SelectionGeom::LinearGradient { .. } | SelectionGeom::RadialGradient { .. } => {
                f64::INFINITY
            }
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
            // 线性渐变无界（覆盖度 1 的半平面延伸到画布外）：与 CoverAll 同例返回 None。
            SelectionGeom::LinearGradient { .. } => None,
            SelectionGeom::RadialGradient { bbox, .. } => Some(*bbox),
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
            SelectionGeom::Path { rings, .. } => {
                let mut min_x = f64::INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for ring in rings {
                    for &(x, y) in ring {
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                }
                Some((min_x, min_y, max_x - min_x, max_y - min_y))
            }
            SelectionGeom::Empty => Some((0.0, 0.0, 0.0, 0.0)),
            SelectionGeom::CoverAll => None,
            SelectionGeom::Bitmap { bbox, .. } => Some((
                bbox[0] as f64,
                bbox[1] as f64,
                bbox[2] as f64,
                bbox[3] as f64,
            )),
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

/// `t` ∈ [0, 1] 上的 smoothstep（`3t² - 2t³`）：单调、连续，两端导数为 0。
fn smoothstep01(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 线性渐变覆盖度（D1-b）：沿 `start`→`end` 轴的投影 `t`（`start` 处 0、`end` 处 1），
/// 覆盖度 = `1 - smoothstep(t)`：`start` 一侧为 1、`end` 一侧为 0。
/// 零长轴（解析期已判为空，防御性再判）返回 0。
fn linear_gradient_coverage(start: (f64, f64), end: (f64, f64), x: f64, y: f64) -> f32 {
    let dx = end.0 - start.0;
    let dy = end.1 - start.1;
    let len_sq = dx * dx + dy * dy;
    if len_sq <= 0.0 || !len_sq.is_finite() {
        return 0.0;
    }
    let t = ((x - start.0) * dx + (y - start.1) * dy) / len_sq;
    (1.0 - smoothstep01(t)) as f32
}

/// 径向渐变覆盖度（D1-b）：归一化椭圆距离 `d`（中心 0、边缘 1），
/// `falloff` ∈ [0, 1] 为过渡带占半径比例：`d ≤ 1 - falloff` 时为 1，
/// `d ≥ 1` 时为 0，中间 smoothstep。`falloff` = 0 为半径处硬边，
/// = 1 为从中心到边缘全程渐变。
fn radial_gradient_coverage(bbox: (f64, f64, f64, f64), falloff: f64, x: f64, y: f64) -> f32 {
    let (bx, by, bw, bh) = bbox;
    let a = bw / 2.0;
    let b = bh / 2.0;
    if a <= 0.0 || b <= 0.0 || !a.is_finite() || !b.is_finite() {
        return 0.0;
    }
    let nx = (x - (bx + a)) / a;
    let ny = (y - (by + b)) / b;
    let d = (nx * nx + ny * ny).sqrt();
    let f = falloff.clamp(0.0, 1.0);
    if d <= 1.0 - f {
        1.0
    } else if d >= 1.0 {
        0.0
    } else {
        // 此处必有 f > 0（d > 1 - f 且 d < 1 ⇒ f > 0），除法安全。
        (1.0 - smoothstep01((d - (1.0 - f)) / f)) as f32
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

/// 路径（多环）的有符号距离：内为负、外为正。
///
/// 内外判定用 [`crate::path::rings_fill_contains`]（与形状填充同一条路径，
/// `fill_rule` 决定嵌套环的挖孔 / 叠加语义）；距离取到全部环折线段
/// （含填充语义的隐式闭合段）的最短距离。
fn path_signed_distance(
    rings: &[Vec<(f64, f64)>],
    fill_rule: crate::path::FillRule,
    x: f64,
    y: f64,
) -> f64 {
    let mut min_dist = f64::INFINITY;
    for ring in rings {
        let count = ring.len();
        if count < 2 {
            continue;
        }
        let mut j = count - 1;
        for i in 0..count {
            let d = segment_distance(x, y, ring[j], ring[i]);
            if d < min_dist {
                min_dist = d;
            }
            j = i;
        }
    }
    let ring_refs: Vec<&[(f64, f64)]> = rings.iter().map(Vec::as_slice).collect();
    if crate::path::rings_fill_contains(&ring_refs, fill_rule, (x, y)) {
        -min_dist
    } else {
        min_dist
    }
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

/// 解析单个点：`{key: [x, y]}` 或 `{key: {x, y}}`；缺失/非数值/非有限返回 `None`。
fn parse_point(value: &Value, key: &str) -> Option<(f64, f64)> {
    let point = value.get(key)?;
    if let Some(pair) = point.as_array() {
        if pair.len() >= 2 {
            if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                if x.is_finite() && y.is_finite() {
                    return Some((x, y));
                }
            }
        }
        return None;
    }
    if let Some(object) = point.as_object() {
        if let (Some(x), Some(y)) = (
            object.get("x").and_then(Value::as_f64),
            object.get("y").and_then(Value::as_f64),
        ) {
            if x.is_finite() && y.is_finite() {
                return Some((x, y));
            }
        }
    }
    None
}

/// 钳到 `0..=1`，把 `NaN` 归为 0（保证覆盖度永远是有限值）。
fn clamp01(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// 位图选区的 O(1) 覆盖率采样：包围盒整数快速剔除 + 一维数组下标。
///
/// `mask` 为 `None`（blob 未注册）时返回 0.0（宁可不选、不可错选）。
fn bitmap_coverage(bbox: &[i32; 4], mask: Option<&DecodedBitmapMask>, x: f64, y: f64) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    if ix < bbox[0] || iy < bbox[1] || ix >= bbox[0] + bbox[2] || iy >= bbox[1] + bbox[3] {
        return 0.0;
    }
    let Some(m) = mask else {
        return 0.0;
    };
    let dx = (ix - bbox[0]) as usize;
    let dy = (iy - bbox[1]) as usize;
    // 防御：位图尺寸与 bbox 不一致时（脏数据）按位图实际尺寸钳制，绝不越界。
    if dx >= m.width as usize || dy >= m.height as usize {
        return 0.0;
    }
    let idx = dy * (m.width as usize) + dx;
    match m.data.get(idx) {
        Some(&v) => f32::from(v) * (1.0 / 255.0),
        None => 0.0,
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
    if let SelectionGeom::Path { rings, .. } = &shape.geom {
        for ring in rings {
            let count = ring.len();
            if count < 2 {
                continue;
            }
            for i in 0..count {
                let a = ring[i];
                let b = ring[(i + 1) % count];
                points.push(a);
                points.push(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0));
            }
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

// ============================================================================
// 蒙版画笔：编译蒙版（CompiledMask）
//
// 设计：几何底座 + 笔刷 patch 链一次光栅化成 u8 网格，渲染期逐像素 O(1)
// 查表。按 (mask_id, 指纹) 缓存（MaskCache），避免分块并行渲染时每块都
// 重解 PNG + 重光栅化（`build` 纯内存、WASM 兼容，无文件 I/O）。
// ============================================================================

/// 整数精确除 255（`round(x / 255)`）。
///
/// 标准 libpng `div255`：`((x + 128 + ((x + 128) >> 8)) >> 8`。
/// 调用处的输入上界：`m_old*(255-d) + target*d ≤ 65025`
/// （两项共享 `d`，`d=0` 时 `m_old*255`、`d=255` 时 `target*255`），`u32` 不溢出。
#[inline(always)]
pub const fn div255(x: u32) -> u8 {
    debug_assert!(x <= 65025);
    let t = x + 128;
    ((t + (t >> 8)) >> 8) as u8
}

/// 编译后的蒙版位图。
#[derive(Debug, Clone)]
pub struct CompiledMask {
    /// `[x, y, w, h]` 画布像素坐标（几何 ∪ patch 的并集，裁剪到画布）。
    pub bbox: [i32; 4],
    /// bbox 之外的默认值（`0.0` 或 `1.0`，由 `invert` 决定）。
    pub default_outside: f32,
    /// 行主序灰度值，`len = w * h`。
    pub grid: Vec<u8>,
}

/// 几何形状的外接盒（f64 画布坐标）。
/// `CoverAll` → 全画布（`None` + 标志）；`Empty` → 无几何贡献（`None`）。
fn geom_bbox_extent(
    geom: &SelectionGeom,
    canvas_w: i32,
    canvas_h: i32,
) -> Option<(f64, f64, f64, f64)> {
    match geom {
        SelectionGeom::CoverAll => Some((0.0, 0.0, canvas_w as f64, canvas_h as f64)),
        SelectionGeom::Empty => None,
        SelectionGeom::Rect { bbox } | SelectionGeom::Ellipse { bbox } => Some(*bbox),
        // 线性渐变：覆盖度 1 的半平面无界 ⇒ 取全画布（与 CoverAll 同口径）。
        SelectionGeom::LinearGradient { .. } => Some((0.0, 0.0, canvas_w as f64, canvas_h as f64)),
        SelectionGeom::RadialGradient { bbox, .. } => Some(*bbox),
        SelectionGeom::Polygon { points } => {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for &(x, y) in points {
                if x < min_x {
                    min_x = x;
                }
                if y < min_y {
                    min_y = y;
                }
                if x > max_x {
                    max_x = x;
                }
                if y > max_y {
                    max_y = y;
                }
            }
            if min_x > max_x {
                None
            } else {
                Some((min_x, min_y, max_x - min_x, max_y - min_y))
            }
        }
        SelectionGeom::Path { rings, .. } => {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for ring in rings {
                for &(x, y) in ring {
                    if x < min_x {
                        min_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
            if min_x > max_x {
                None
            } else {
                Some((min_x, min_y, max_x - min_x, max_y - min_y))
            }
        }
        SelectionGeom::Bitmap { bbox, .. } => Some((
            bbox[0] as f64,
            bbox[1] as f64,
            bbox[2] as f64,
            bbox[3] as f64,
        )),
    }
}

impl CompiledMask {
    /// 由蒙版状态编译。`load_blob` 按 blob 哈希取字节（`&str` 为
    /// `"sha256:…"` 形式）；解不出的 patch 静默跳过（渲染不炸）。
    pub fn build(
        mask: &Selection,
        canvas_w: i32,
        canvas_h: i32,
        load_blob: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Self {
        // 从最后一个 ReplaceAll 开始重放（快照截断链）。
        let start_idx = mask
            .patches
            .iter()
            .rposition(|p| p.op == MaskPatchOp::ReplaceAll)
            .unwrap_or(0);
        let has_replace_all =
            mask.patches.get(start_idx).map(|p| p.op) == Some(MaskPatchOp::ReplaceAll);

        // invert 只作用于几何阶段（笔刷沉积是"目标值"语义，不翻转）。
        let default_outside_u8: u8 = if mask.invert { 255 } else { 0 };

        // union bbox：几何（feather 扩展）∪ patch bboxes，裁剪到画布。
        let mut ux0 = i32::MAX;
        let mut uy0 = i32::MAX;
        let mut ux1 = i32::MIN;
        let mut uy1 = i32::MIN;
        let mut union_box = |x0: i32, y0: i32, x1: i32, y1: i32| {
            if x0 < ux0 {
                ux0 = x0;
            }
            if y0 < uy0 {
                uy0 = y0;
            }
            if x1 > ux1 {
                ux1 = x1;
            }
            if y1 > uy1 {
                uy1 = y1;
            }
        };
        // 几何底座用的形状：struct 顶层字段为准（shape JSON 里没有这三者）。
        // mode 在蒙版语义下恒为 new（与现有 `apply_layer_mask` 一致：忽略）。
        let mut base_shape = SelectionShape::from_value(&mask.shape);
        base_shape.feather = if mask.feather.is_finite() && mask.feather > 0.0 {
            mask.feather
        } else {
            0.0
        };
        base_shape.invert = mask.invert;
        if !has_replace_all {
            if let Some((gx, gy, gw, gh)) = geom_bbox_extent(&base_shape.geom, canvas_w, canvas_h) {
                let f = base_shape.feather;
                union_box(
                    (gx - f).floor() as i32,
                    (gy - f).floor() as i32,
                    (gx + gw + f).ceil() as i32,
                    (gy + gh + f).ceil() as i32,
                );
            }
        }
        for patch in &mask.patches[start_idx..] {
            let [px, py, pw, ph] = patch.bbox;
            if pw > 0 && ph > 0 {
                union_box(px, py, px + pw, py + ph);
            }
        }
        // 裁剪到画布。
        ux0 = ux0.clamp(0, canvas_w);
        uy0 = uy0.clamp(0, canvas_h);
        ux1 = ux1.clamp(0, canvas_w);
        uy1 = uy1.clamp(0, canvas_h);
        let bw = (ux1 - ux0).max(0);
        let bh = (uy1 - uy0).max(0);
        let mut grid = vec![default_outside_u8; (bw as usize) * (bh as usize)];

        // 阶段 1：几何底座（无 ReplaceAll 时）。
        if !has_replace_all && bw > 0 && bh > 0 {
            for yy in 0..bh {
                for xx in 0..bw {
                    let c = base_shape.coverage((ux0 + xx) as f64, (uy0 + yy) as f64);
                    grid[(yy as usize) * (bw as usize) + (xx as usize)] =
                        (c * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }

        // 阶段 2：重放 patch 链。
        if bw > 0 && bh > 0 {
            for patch in &mask.patches[start_idx..] {
                let blob_bytes = match load_blob(patch.blob.as_str()) {
                    Some(b) => b,
                    None => continue,
                };
                let (pw, ph, gray) = match decode_png_l8(&blob_bytes) {
                    Some(t) => t,
                    None => continue,
                };
                let [px, py, bw2, bh2] = patch.bbox;
                if bw2 != pw as i32 || bh2 != ph as i32 {
                    continue; // 尺寸不符：坏数据，跳过
                }
                let target = u32::from(patch.target_u8);
                for yy in 0..ph as i32 {
                    for xx in 0..pw as i32 {
                        let gx = px + xx;
                        let gy = py + yy;
                        if gx < ux0 || gy < uy0 || gx >= ux1 || gy >= uy1 {
                            continue;
                        }
                        let dst_idx = ((gy - uy0) as usize) * (bw as usize) + ((gx - ux0) as usize);
                        let d = u32::from(gray[(yy as usize) * (pw as usize) + (xx as usize)]);
                        match patch.op {
                            MaskPatchOp::ReplaceAll => {
                                grid[dst_idx] = d as u8;
                            }
                            MaskPatchOp::LerpDeposit => {
                                let m_old = u32::from(grid[dst_idx]);
                                grid[dst_idx] = div255(m_old * (255 - d) + target * d);
                            }
                        }
                    }
                }
            }
        }

        Self {
            bbox: [ux0, uy0, bw, bh],
            default_outside: f32::from(default_outside_u8) * (1.0 / 255.0),
            grid,
        }
    }

    /// 采样画布坐标 (x, y) 的蒙版值（0.0..1.0）。
    #[inline(always)]
    pub fn sample(&self, x: i32, y: i32) -> f32 {
        let [bx, by, bw, bh] = self.bbox;
        let dx = x.wrapping_sub(bx);
        let dy = y.wrapping_sub(by);
        if dx < 0 || dy < 0 || dx >= bw || dy >= bh {
            return self.default_outside;
        }
        let idx = (dy as usize) * (bw as usize) + (dx as usize);
        match self.grid.get(idx) {
            Some(&v) => f32::from(v) * (1.0 / 255.0),
            None => self.default_outside,
        }
    }
}

/// 解码后的位图选区掩膜（`smart_select` 生成，L8 灰度）。
///
/// `data` 为行主序 `u8` 覆盖率，长度恒为 `width * height`。
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedBitmapMask {
    /// 位图宽（像素）。
    pub width: u32,
    /// 位图高（像素）。
    pub height: u32,
    /// 行主序覆盖率（0~255）。
    pub data: Vec<u8>,
}

/// 已解码位图选区的全局注册表：`blob_hash -> DecodedBitmapMask`。
///
/// `SelectionShape::from_value` 是纯函数、拿不到 blob 存储，只能在这里做一次
/// 查表绑定（`coverage(x,y)` 内零锁、零哈希、零解码）。写入侧（`write_smart_select`）
/// 在生成位图时注册；渲染准备阶段（`ensure_bitmap_masks_registered`）对 fold
/// 重放 / 跨进程的情形从存储按需解码注册，保证重载后不静默退化为空选区。
/// 渲染是分块并行的 ⇒ `Mutex`（与 `MaskCache` 同一口径）。
static BITMAP_MASK_REGISTRY: std::sync::LazyLock<Mutex<HashMap<String, Arc<DecodedBitmapMask>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// **位图掩膜注册表的条数上限** ✓（第三轮复审 **N3B-01**，P1）
///
/// **为什么必须有** ✗：注册表原来是**只增不减**（`register_bitmap_mask` 只有 `insert`，
/// 没有 remove / 预算 / LRU）⇒ 4K 画布单条约 **16 MB**，而布尔合并每次产出新 blob 条目
/// ⇒ **服务端长会话内存耗尽**；wasm 32 位固定堆下可直接 abort 实例 ✗
///
/// **为什么是 8 条**：4K 一条 ~16 MB ⇒ 8 条 ~128 MB ⇒ 已经是「一个桌面进程可接受的上限」；
/// 而正常用法同时引用的位图选区很少超过 3–4 条（作者面板最多同时显示几个文档）。
pub const MAX_REGISTERED_MASKS: usize = 8;

/// 注册顺序（FIFO 淘汰用）。与 `BITMAP_MASK_REGISTRY` **同锁序**：
/// 两个 `lock()` 都在 `register_bitmap_mask` 里、且顺序固定（先 REGISTRY 后 ORDER），
/// 别处不锁 ORDER ⇒ 无死锁。
static REGISTRY_ORDER: std::sync::LazyLock<Mutex<Vec<String>>> =
    std::sync::LazyLock::new(|| Mutex::new(Vec::new()));

/// 把解码后的位图掩膜注册进全局表（写时 / 渲染准备阶段调用）。
///
/// **超上限时淘汰最旧的一条**（FIFO，不是 LRU —— 「存活期长但不再用」的条目先走，
/// 而重载会按需重新解码 ⇒ 代价是一次 `store.get`，不是错误）。
pub fn register_bitmap_mask(blob_hash: String, mask: Arc<DecodedBitmapMask>) {
    let mut guard = BITMAP_MASK_REGISTRY.lock().expect("位图选区注册表锁中毒");
    let mut order = REGISTRY_ORDER.lock().expect("位图选区注册顺序锁中毒");
    if !guard.contains_key(&blob_hash) {
        order.push(blob_hash.clone());
    }
    guard.insert(blob_hash, mask);
    // **淘汰到上限以内** ✓：while 而不是 if —— 上限可能被下调过，也可能是首次大量注册
    while guard.len() > MAX_REGISTERED_MASKS {
        let Some(oldest) = order.first().cloned() else {
            break;
        };
        order.remove(0);
        guard.remove(&oldest);
    }
}

/// **显式移除一条**（选区被删除时调用 ⇒ 立刻释放，不等淘汰）✓
pub fn unregister_bitmap_mask(blob_hash: &str) {
    BITMAP_MASK_REGISTRY
        .lock()
        .expect("位图选区注册表锁中毒")
        .remove(blob_hash);
    REGISTRY_ORDER
        .lock()
        .expect("位图选区注册顺序锁中毒")
        .retain(|h| h != blob_hash);
}

/// 当前注册条数（判据用：证明上限生效）。
pub fn registered_bitmap_mask_count() -> usize {
    BITMAP_MASK_REGISTRY
        .lock()
        .expect("位图选区注册表锁中毒")
        .len()
}

/// 查已注册的位图掩膜（`from_value` 构造时单次绑定用）。
pub fn get_bitmap_mask(blob_hash: &str) -> Option<Arc<DecodedBitmapMask>> {
    BITMAP_MASK_REGISTRY
        .lock()
        .expect("位图选区注册表锁中毒")
        .get(blob_hash)
        .map(Arc::clone)
}

/// `ensure_bitmap_masks_registered` 的预热结果。
///
/// `failed` 是解不开的 blob（取不到字节或解码失败）。
/// `evicted` 是本次请求但预热后仍未注册的 blob：它们多半因 8 条上限
/// 在本次预热中被 FIFO 淘汰，随后 `from_value` 会绑定 `mask = None`、
/// 覆盖率退化为 0——即本次渲染真正会丢像素的那些。
/// 注意：`evicted` 不含注册表里原有的陈旧条目被淘汰的情况（那不影响
/// 本次渲染）；也不含 `failed`（解不开的只进 `failed`，不重复报）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BitmapMaskEnsureReport {
    /// 解不开（取不到字节或解码失败）的 blob。
    pub failed: Vec<String>,
    /// 本次请求但预热后仍未注册的 blob（多半因 8 条上限被 FIFO 淘汰）。
    pub evicted: Vec<String>,
}

/// 渲染准备阶段：确保给定形状里引用的位图 blob 都已解码注册。
///
/// `load` 只在注册表未命中时调用（通常是 `store.get(&hash)`）。已注册的直接跳过，
/// 解码失败的也跳过（`from_value` 会把 `mask` 置 `None`，覆盖率退化为 0，
/// 调用方按「宁可不选、不可错选」处理）。
///
/// 返回预热结果 [`BitmapMaskEnsureReport`]：`failed` 是解不开的 blob，
/// `evicted` 是本次请求但预热后仍未注册的 blob（多半因 8 条上限被
/// FIFO 淘汰——随后 `from_value` 绑定 `mask = None`、覆盖率退化为 0，
/// 即本次渲染真正会丢像素的那些；调用方必须把它们报出来，不许静默）。
pub fn ensure_bitmap_masks_registered(
    shapes: &[&Value],
    load: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> BitmapMaskEnsureReport {
    // **返回值见 `BitmapMaskEnsureReport`** ✓（第 2665 轮；外部 `E1742` 一族）：
    //   原来它**只认 `decode_png_l8`（8 位灰度 PNG）** ✗ ⇒ 传 **raw RGBA** 或 **RGBA PNG** 的蒙版
    //   ⇒ **静默不注册 ⇒ 覆盖率恒 0 ⇒ `ok:true` 但一个像素都不落** ✗✗
    //   ⇒ **现在**：① 多格式解码（L8 PNG ✓ / RGBA PNG 取 alpha ✓ / raw RGBA 取 alpha ✓ / raw L8 ✓）
    //            ② **解不开就**回报名单** ⇒ 调用方必须**响亮报错** ✓（不许再静默 ✓）
    let mut failed: Vec<String> = Vec::new();
    let mut requested: Vec<String> = Vec::new();
    for shape in shapes {
        if shape.get("kind").and_then(Value::as_str) != Some("bitmap") {
            continue;
        }
        // **★ `blob` **或** `blob_hash` —— 掩膜存的是后者 ★**（第 1607 轮）
        //   **⇒ 外部 `E1742` 收窄后的真因**：`create_mask(shape.kind=bitmap)` 存的是
        //     **`{kind:"bitmap", blob_hash:"sha256:…"}`** ⇒ 而这里**只找 `blob`** ✗
        //     ⇒ **`continue` 静默跳过 ⇒ 从不注册 ⇒ 覆盖率恒 0** ⇒
        //     `brush_stroke(clip_to_selection=…)` **`ok=true` 而一个像素不落** ✗✗
        //   **⇒ ⇒ 而这是**同族第三次**：`w`/`width`（rect）、`points`（polygon）、现在 `blob`/`blob_hash`** ✓
        //   **⇒ ⇒ ⇒ 所以**：**两种键都收** ⇒ 用**同一个** blob 字符串 ✓
        let Some(blob) = shape
            .get("blob")
            .or_else(|| shape.get("blob_hash"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if !requested.iter().any(|b| b == blob) {
            requested.push(blob.to_owned());
        }
        {
            let guard = BITMAP_MASK_REGISTRY.lock().expect("位图选区注册表锁中毒");
            if guard.contains_key(blob) {
                continue;
            }
        }
        let Some(bytes) = load(blob) else {
            failed.push(blob.to_owned());
            continue;
        };
        let dims = (
            shape.get("width").and_then(Value::as_u64).map(|v| v as u32),
            shape
                .get("height")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
        );
        let decoded = decode_png_l8(&bytes)
            .or_else(|| {
                // RGBA PNG ⇒ 取 **alpha** 通道（蒙版的语义就是"哪些像素参与"✓）
                crate::png::decode_png(&bytes).map(|(w, h, rgba)| {
                    let data = rgba.chunks(4).map(|px| px[3]).collect();
                    (w, h, data)
                })
            })
            .or_else(|| {
                // **裸字节**（没有 PNG 头）：按 shape 的 `width`/`height` 认
                //   `w*h*4` ⇒ raw RGBA 取 alpha ✓；`w*h` ⇒ raw L8 ✓。
                let (w, h) = (dims.0?, dims.1?);
                let want_rgba = (w as usize) * (h as usize) * 4;
                let want_l8 = (w as usize) * (h as usize);
                if bytes.len() == want_rgba {
                    Some((w, h, bytes.chunks(4).map(|px| px[3]).collect()))
                } else if bytes.len() == want_l8 {
                    Some((w, h, bytes.clone()))
                } else {
                    None
                }
            });
        match decoded {
            Some((w, h, data)) => register_bitmap_mask(
                blob.to_owned(),
                Arc::new(DecodedBitmapMask {
                    width: w,
                    height: h,
                    data,
                }),
            ),
            None => failed.push(blob.to_owned()),
        }
    }
    // 本次请求但预热后仍未注册的 = 真正会丢像素的：解不开的只进 failed；
    // 已注册又被本次预热淘汰的（含预热前已在表里、又被后注册者挤掉的）进 evicted。
    let guard = BITMAP_MASK_REGISTRY.lock().expect("位图选区注册表锁中毒");
    let evicted: Vec<String> = requested
        .into_iter()
        .filter(|blob| !failed.contains(blob) && !guard.contains_key(blob))
        .collect();
    BitmapMaskEnsureReport { failed, evicted }
}

/// 蒙版编译缓存：按 `mask.id` 存 `(指纹, 编译结果)`。
///
/// 渲染是分块并行的（`thread::scope`），同一蒙版会被多块同时查询 ⇒ `Mutex`。
/// 指纹用 `Selection::hash_fingerprint`（`Debug` 字符串位图化后会爆炸）。
#[derive(Debug, Default)]
pub struct MaskCache {
    inner: Mutex<HashMap<String, (u64, Arc<CompiledMask>)>>,
}

impl MaskCache {
    /// 新建空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 取缓存或编译。`load_blob` 只在缓存未命中时调用。
    pub fn get_or_build(
        &self,
        mask: &Selection,
        canvas_w: i32,
        canvas_h: i32,
        load_blob: &dyn Fn(&str) -> Option<Vec<u8>>,
    ) -> Arc<CompiledMask> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        let mut hasher = DefaultHasher::new();
        mask.hash_fingerprint(&mut hasher);
        let fingerprint = hasher.finish();
        let mut guard = self.inner.lock().expect("蒙版缓存锁中毒");
        if let Some((fp, compiled)) = guard.get(&mask.id) {
            if *fp == fingerprint {
                return Arc::clone(compiled);
            }
        }
        let compiled = Arc::new(CompiledMask::build(mask, canvas_w, canvas_h, load_blob));
        guard.insert(mask.id.clone(), (fingerprint, Arc::clone(&compiled)));
        compiled
    }

    /// 当前缓存的编译蒙版字节之和（`grid` 按分配容量计；RSS 修复 paint-rss：
    /// C2 预算闸估算口径的一部分——单张全画布蒙版即数十 MB，不可漏记）。
    pub fn bytes(&self) -> usize {
        let guard = self.inner.lock().expect("蒙版缓存锁中毒");
        guard
            .values()
            .map(|(_, compiled)| compiled.grid.capacity())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use yanshi_core::MaskPatchMeta;

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

    /// 圆弧段的四点组（起点， c1, c2, 终点），`circle_subpath` 用。
    type Arc4 = ((f64, f64), (f64, f64), (f64, f64), (f64, f64));

    /// 圆弧近似的圆子路径（4 段三次 Bézier，κ 弧常数）：圆心 (cx, cy)、半径 r。
    /// `reverse` 为真时整条子路径绕向反转（段序逆转 + 每段起终点互换），
    /// nonzero 用例靠它区分「同向叠加 / 反向抵消」。
    fn circle_subpath(cx: f64, cy: f64, r: f64, reverse: bool) -> Value {
        const KAPPA: f64 = 0.552_284_749_830_79;
        let d = r * KAPPA;
        // 每段 (起点, c1, c2, 终点)，首尾相接成环。
        let mut arcs: Vec<Arc4> = vec![
            (
                (cx + r, cy),
                (cx + r, cy + d),
                (cx + d, cy + r),
                (cx, cy + r),
            ),
            (
                (cx, cy + r),
                (cx - d, cy + r),
                (cx - r, cy + d),
                (cx - r, cy),
            ),
            (
                (cx - r, cy),
                (cx - r, cy - d),
                (cx - d, cy - r),
                (cx, cy - r),
            ),
            (
                (cx, cy - r),
                (cx + d, cy - r),
                (cx + r, cy - d),
                (cx + r, cy),
            ),
        ];
        if reverse {
            arcs = arcs
                .iter()
                .rev()
                .map(|&(p0, c1, c2, p1)| (p1, c2, c1, p0))
                .collect();
        }
        let segments: Vec<Value> = arcs
            .iter()
            .map(|&(_, c1, c2, to)| {
                json!({"op": "C", "c1": [c1.0, c1.1], "c2": [c2.0, c2.1], "to": [to.0, to.1]})
            })
            .collect();
        let start = arcs[0].0;
        json!({"start": [start.0, start.1], "closed": true, "segments": segments})
    }

    #[test]
    fn path_circle_coverage_and_area_match_analytic() {
        // r=16 的圆（圆弧近似 path）：覆盖率对解析圆，面积对 πr²。
        let s = shape(json!({
            "kind": "path",
            "subpaths": [circle_subpath(32.0, 32.0, 16.0, false)],
        }));
        assert_eq!(s.coverage(32.0, 32.0), 1.0, "圆心应在内");
        assert_eq!(s.coverage(47.0, 32.0), 1.0, "距圆心 15 < r 应在内");
        assert_eq!(s.coverage(49.0, 32.0), 0.0, "距圆心 17 > r 应在外");
        assert_eq!(s.coverage(0.0, 0.0), 0.0);
        // 面积：0.5 步长采样求和（采样误差 ~周长×步长/2，远小于 5% 容差）。
        let mut area = 0.0f64;
        let mut y = 12.0;
        while y < 52.0 {
            let mut x = 12.0;
            while x < 52.0 {
                area += f64::from(s.coverage(x, y)) * 0.25;
                x += 0.5;
            }
            y += 0.5;
        }
        let analytic = std::f64::consts::PI * 16.0 * 16.0;
        assert!(
            (area - analytic).abs() <= analytic * 0.05,
            "path 圆面积 {area} 与解析值 {analytic} 偏差超过 5%"
        );
    }

    #[test]
    fn path_evenodd_concentric_rings_punch_hole() {
        // 同心双环 + evenodd：内环是洞——中心不覆盖、环带覆盖、外侧不覆盖。
        let s = shape(json!({
            "kind": "path",
            "fill_rule": "evenodd",
            "subpaths": [
                circle_subpath(32.0, 32.0, 16.0, false),
                circle_subpath(32.0, 32.0, 6.0, false),
            ],
        }));
        assert_eq!(s.coverage(32.0, 32.0), 0.0, "evenodd 内环应挖空中心");
        assert_eq!(s.coverage(43.0, 32.0), 1.0, "两环之间的环带应覆盖");
        assert_eq!(s.coverage(52.0, 32.0), 0.0, "外环之外不覆盖");
    }

    #[test]
    fn path_nonzero_direction_decides_stacking() {
        // nonzero + 同向内环：绕数叠加 ⇒ 中心被覆盖（与 evenodd 的关系差异）。
        let stacked = shape(json!({
            "kind": "path",
            "fill_rule": "nonzero",
            "subpaths": [
                circle_subpath(32.0, 32.0, 16.0, false),
                circle_subpath(32.0, 32.0, 6.0, false),
            ],
        }));
        assert_eq!(stacked.coverage(32.0, 32.0), 1.0, "nonzero 同向应叠加");
        assert_eq!(stacked.coverage(43.0, 32.0), 1.0);
        // nonzero + 反向内环：绕数抵消 ⇒ 中心成洞。
        let cancelled = shape(json!({
            "kind": "path",
            "fill_rule": "nonzero",
            "subpaths": [
                circle_subpath(32.0, 32.0, 16.0, false),
                circle_subpath(32.0, 32.0, 6.0, true),
            ],
        }));
        assert_eq!(
            cancelled.coverage(32.0, 32.0),
            0.0,
            "nonzero 反向应抵消成洞"
        );
        assert_eq!(cancelled.coverage(43.0, 32.0), 1.0);
        // evenodd 与绕向无关：反向内环照样挖洞。
        let evenodd_reversed = shape(json!({
            "kind": "path",
            "fill_rule": "evenodd",
            "subpaths": [
                circle_subpath(32.0, 32.0, 16.0, false),
                circle_subpath(32.0, 32.0, 6.0, true),
            ],
        }));
        assert_eq!(evenodd_reversed.coverage(32.0, 32.0), 0.0);
    }

    #[test]
    fn path_invalid_payload_is_empty_not_cover_all() {
        // 回归锁：path 在本包之前落进未知 kind 的「覆盖全部」分支 ⇒ 传坏载荷
        // 会把整个画布选上。修后必须是空：以下每种坏载荷在任何采样点都是 0。
        for bad in [
            json!({"kind": "path"}),
            json!({"kind": "path", "subpaths": []}),
            json!({"kind": "path", "subpaths": [{"start": [0.0, 0.0], "segments": [{"op": "X", "to": [1.0, 1.0]}]}]}),
            json!({"kind": "path", "fill_rule": "bogus", "subpaths": [circle_subpath(8.0, 8.0, 4.0, false)]}),
        ] {
            let s = shape(bad);
            for &(x, y) in &[(0.0, 0.0), (8.0, 8.0), (32.0, 32.0), (500.0, 500.0)] {
                assert_eq!(s.coverage(x, y), 0.0, "坏 path 载荷应在 ({x},{y}) 为空");
            }
        }
    }

    #[test]
    fn path_signed_distance_sign_and_feather_band() {
        // L 形（纯直线段）：左柱 + 顶横，右下是凹口。
        let l_shape = json!({
            "kind": "path",
            "subpaths": [{
                "start": [0.0, 0.0], "closed": true,
                "segments": [
                    {"op": "L", "to": [20.0, 0.0]},
                    {"op": "L", "to": [20.0, 10.0]},
                    {"op": "L", "to": [10.0, 10.0]},
                    {"op": "L", "to": [10.0, 20.0]},
                    {"op": "L", "to": [0.0, 20.0]}
                ]
            }],
        });
        let s = shape(l_shape.clone());
        assert_eq!(s.coverage(5.0, 15.0), 1.0, "左柱内");
        assert_eq!(s.coverage(15.0, 5.0), 1.0, "顶横内");
        assert_eq!(s.coverage(15.0, 15.0), 0.0, "凹口外");
        // 有符号距离的符号关系 + 数值：(15,12) 到最近边 (10..20, 10) 为 2。
        assert!(s.signed_distance(5.0, 15.0) < 0.0, "内点距离应为负");
        assert!(s.signed_distance(15.0, 15.0) > 0.0, "外点距离应为正");
        assert!(
            (s.signed_distance(15.0, 12.0) - 2.0).abs() < 0.3,
            "凹口内点到边的距离应约 2，实际 {}",
            s.signed_distance(15.0, 12.0)
        );
        // 羽化带：边界上（距离 0）的点覆盖度恰为 0.5（smoothstep 中点）。
        let mut feathered = l_shape;
        feathered["feather"] = json!(4.0);
        let sf = shape(feathered);
        assert_near(sf.coverage(5.0, 0.0), 0.5);
    }

    #[test]
    fn path_open_subpath_fill_closes_implicitly() {
        // 开放子路径（closed=false）按填充语义隐式闭合（与 SVG / 形状填充一致）。
        let s = shape(json!({
            "kind": "path",
            "subpaths": [{
                "start": [0.0, 0.0], "closed": false,
                "segments": [
                    {"op": "L", "to": [12.0, 0.0]},
                    {"op": "L", "to": [0.0, 12.0]}
                ]
            }],
        }));
        assert_eq!(s.coverage(2.0, 2.0), 1.0, "隐式闭合的三角内");
        assert_eq!(s.coverage(10.0, 10.0), 0.0, "斜边外");
    }

    #[test]
    fn path_selection_set_subtract_combine() {
        // 选区栈组合：rect(new) − path 圆(subtract)：圆内被挖、圆外保留。
        let mut set = SelectionSet::new();
        set.push(shape(json!({
            "kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 64.0, "h": 64.0}
        })));
        set.push(shape(json!({
            "kind": "path", "mode": "subtract",
            "subpaths": [circle_subpath(32.0, 32.0, 10.0, false)],
        })));
        assert_eq!(set.coverage(32.0, 32.0), 0.0, "subtract 的圆心应被挖掉");
        assert_eq!(set.coverage(4.0, 4.0), 1.0, "圆外矩形内应保留");
        assert_eq!(set.coverage(70.0, 70.0), 0.0, "矩形外本就无覆盖");
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

    /// 构造"fold 之后"的 Selection：shape 只有几何，
    /// feather/mode/invert 在 struct 顶层字段（与真实原子日志一致）。
    fn struct_selection(
        shape: Value,
        mode: &str,
        feather: f64,
        invert: bool,
    ) -> yanshi_core::Selection {
        yanshi_core::Selection {
            id: "sel_test".to_string(),
            shape,
            feather,
            mode: mode.to_string(),
            invert,
            linked_layer: None,
            refined_edges: false,
            blobs: vec![],
            patches: vec![],
            created_by: "atom_test".to_string(),
            deleted_by: None,
        }
    }

    // ---- 蒙版画笔：CompiledMask ----

    fn test_mask(shape: Value) -> Selection {
        Selection {
            id: "mask_test".to_owned(),
            shape,
            feather: 0.0,
            mode: "new".to_owned(),
            invert: false,
            linked_layer: None,
            refined_edges: false,
            blobs: Vec::new(),
            patches: Vec::new(),
            created_by: "atom_0".to_owned(),
            deleted_by: None,
        }
    }

    #[test]
    fn from_selection_keeps_struct_fields() {
        // 回归：from_value(&sel.shape) 会静默丢弃 feather/mode/invert
        //（退化为 new/0.0/false），from_selection 必须保留。
        let sel = struct_selection(
            json!({"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}}),
            "add",
            8.0,
            true,
        );
        let s = SelectionShape::from_selection(&sel);
        assert_eq!(s.mode, SelectionMode::Add);
        assert!(
            (s.feather - 8.0).abs() < 1e-9,
            "feather 应为 8.0，实际 {}",
            s.feather
        );
        assert!(s.invert, "invert 应为 true");
        // 对照：旧路径确实会丢（三者全退化），证明这条测试不是安慰剂
        let dropped = SelectionShape::from_value(&sel.shape);
        assert_eq!(dropped.mode, SelectionMode::New);
        assert_eq!(dropped.feather, 0.0);
        assert!(!dropped.invert);
    }

    #[test]
    fn from_selection_modes_combine_end_to_end() {
        // 两个选区经 from_selection 进 SelectionSet：add/subtract 真生效
        let base = struct_selection(
            json!({"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}}),
            "new",
            0.0,
            false,
        );
        let add = struct_selection(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 4.0, "h": 4.0}}),
            "add",
            0.0,
            false,
        );
        let mut set = SelectionSet::new();
        set.push(SelectionShape::from_selection(&base));
        set.push(SelectionShape::from_selection(&add));
        assert_near(set.coverage(11.0, 11.0), 1.0); // 只在 add 区
        assert_near(set.coverage(2.0, 2.0), 1.0); // base 区
        assert_near(set.coverage(7.0, 7.0), 0.0); // 都不在

        let sub = struct_selection(
            json!({"kind": "rect", "bbox": {"x": 1.0, "y": 1.0, "w": 2.0, "h": 2.0}}),
            "subtract",
            0.0,
            false,
        );
        let mut set2 = SelectionSet::new();
        set2.push(SelectionShape::from_selection(&base));
        set2.push(SelectionShape::from_selection(&sub));
        assert_near(set2.coverage(2.0, 2.0), 0.0); // 被挖掉
        assert_near(set2.coverage(0.5, 0.5), 1.0); // base 剩余
    }

    #[test]
    fn from_selection_invert_flips_coverage() {
        let sel = struct_selection(
            json!({"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 4.0, "h": 4.0}}),
            "new",
            0.0,
            true,
        );
        let s = SelectionShape::from_selection(&sel);
        assert_near(s.coverage(2.0, 2.0), 0.0); // 矩形内被反选掉
        assert_near(s.coverage(10.0, 10.0), 1.0); // 矩形外被选中
    }

    fn l8_patch(
        blobs: &mut std::collections::HashMap<String, Vec<u8>>,
        bbox: [i32; 4],
        target_u8: u8,
        op: MaskPatchOp,
        gray_value: u8,
    ) -> MaskPatchMeta {
        let w = bbox[2] as usize;
        let h = bbox[3] as usize;
        let gray = vec![gray_value; w * h];
        let png = crate::png::encode_png_l8(bbox[2] as u32, bbox[3] as u32, &gray)
            .expect("测试 PNG 编码不会失败");
        // 内容寻址的假哈希（测试内自洽即可；须满足 64 位小写十六进制）。
        let fake_hash = format!(
            "sha256:{:064x}",
            u64::from(target_u8) * 256 + u64::from(gray_value)
        );
        blobs.insert(fake_hash.clone(), png);
        MaskPatchMeta {
            bbox,
            target_u8,
            op,
            blob: fake_hash.parse().expect("测试哈希合法"),
        }
    }

    fn build_with_blobs(
        mask: &Selection,
        blobs: &std::collections::HashMap<String, Vec<u8>>,
    ) -> CompiledMask {
        let load = |s: &str| blobs.get(s).cloned();
        CompiledMask::build(mask, 64, 64, &load)
    }

    #[test]
    fn div255_matches_round_div() {
        // 定义域是调用处的真实上界 [0, 65025]（见 div255 文档）。
        for x in [0u32, 1, 127, 128, 129, 254, 255, 256, 1000, 32768, 65025] {
            let expected = (f64::from(x) / 255.0).round() as u8;
            assert_eq!(div255(x), expected, "x={x}");
        }
        // 全量扫一遍（65026 个值，毫秒级）。
        for x in 0..=65025u32 {
            let expected = (f64::from(x) / 255.0).round() as u8;
            assert_eq!(div255(x), expected, "x={x}");
        }
    }

    #[test]
    fn compiled_mask_geometric_base() {
        let mask = test_mask(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}}),
        );
        let compiled = build_with_blobs(&mask, &std::collections::HashMap::new());
        assert_eq!(compiled.bbox, [10, 10, 20, 20]);
        assert_near(compiled.sample(15, 15), 1.0);
        assert_near(compiled.sample(10, 10), 1.0);
        assert_near(compiled.sample(5, 5), 0.0);
        assert_near(compiled.sample(40, 40), 0.0);
        assert_eq!(compiled.default_outside, 0.0);
    }

    #[test]
    fn compiled_mask_lerp_deposit_chains() {
        let mut blobs = std::collections::HashMap::new();
        let mut mask = test_mask(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}}),
        );
        // 第一笔：全沉积擦除 → (14,14) 从 255 变 0。
        mask.patches.push(l8_patch(
            &mut blobs,
            [12, 12, 4, 4],
            0,
            MaskPatchOp::LerpDeposit,
            255,
        ));
        // 第二笔：半沉积加选 → (15,15) 从 0 变 128。
        mask.patches.push(l8_patch(
            &mut blobs,
            [14, 14, 4, 4],
            255,
            MaskPatchOp::LerpDeposit,
            128,
        ));
        let compiled = build_with_blobs(&mask, &blobs);
        // 第一笔的 bbox 并进 union。
        assert_eq!(compiled.bbox, [10, 10, 20, 20]);
        // (13,13) 只在第一笔内：255 → 0。
        assert_near(compiled.sample(13, 13), 0.0);
        // (15,15) 被两笔覆盖：先 255→0，再 0→128。
        assert_near(compiled.sample(15, 15), 128.0 / 255.0);
        // 未被笔触碰到的几何区保持 1.0。
        assert_near(compiled.sample(25, 25), 1.0);
    }

    #[test]
    fn compiled_mask_replace_all_truncates_chain() {
        let mut blobs = std::collections::HashMap::new();
        let mut mask = test_mask(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}}),
        );
        // 快照前的增量必须被截断（重放从 ReplaceAll 开始）。
        mask.patches.push(l8_patch(
            &mut blobs,
            [12, 12, 4, 4],
            0,
            MaskPatchOp::LerpDeposit,
            255,
        ));
        // 真实快照：覆盖全几何区的全量烘焙值（把第一笔的"洞"填成 200）。
        mask.patches.push(l8_patch(
            &mut blobs,
            [10, 10, 20, 20],
            200,
            MaskPatchOp::ReplaceAll,
            200,
        ));
        let compiled = build_with_blobs(&mask, &blobs);
        // (14,14) 若重放了第一笔会是 0；截断后应为快照值 200/255。
        assert_near(compiled.sample(14, 14), 200.0 / 255.0);
        assert_near(compiled.sample(25, 25), 200.0 / 255.0);
        assert_near(compiled.sample(5, 5), 0.0);
    }

    #[test]
    fn compiled_mask_invert_applies_to_geometry_only() {
        let mut blobs = std::collections::HashMap::new();
        let mut mask = test_mask(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}}),
        );
        mask.invert = true;
        // 几何反选：形内 0、形外 1。
        let compiled = build_with_blobs(&mask, &blobs);
        assert_near(compiled.sample(15, 15), 0.0);
        assert_near(compiled.sample(5, 5), 1.0);
        assert_eq!(compiled.default_outside, 1.0);
        // 笔刷沉积不反选：向 target=255 全沉积 → 回到 255（不是"反选的反选"）。
        mask.patches.push(l8_patch(
            &mut blobs,
            [12, 12, 4, 4],
            255,
            MaskPatchOp::LerpDeposit,
            255,
        ));
        let compiled2 = build_with_blobs(&mask, &blobs);
        assert_near(compiled2.sample(14, 14), 1.0);
    }

    #[test]
    fn mask_cache_hit_returns_same_arc() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::Hasher;
        let cache = MaskCache::new();
        let mask = test_mask(
            json!({"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}}),
        );
        let blobs: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
        let load = |s: &str| blobs.get(s).cloned();
        let a = cache.get_or_build(&mask, 64, 64, &load);
        let b = cache.get_or_build(&mask, 64, 64, &load);
        assert!(std::sync::Arc::ptr_eq(&a, &b), "指纹未变必须命中缓存");
        // 改一个 patch → 指纹变 → 必须重新编译。
        let mut mask2 = mask.clone();
        let mut blobs2 = std::collections::HashMap::new();
        mask2.patches.push(l8_patch(
            &mut blobs2,
            [12, 12, 4, 4],
            0,
            MaskPatchOp::LerpDeposit,
            255,
        ));
        let load2 = |s: &str| blobs2.get(s).cloned();
        let c = cache.get_or_build(&mask2, 64, 64, &load2);
        assert!(!std::sync::Arc::ptr_eq(&a, &c), "指纹变了必须重新编译");
        // 指纹确实变了（sanity：hash_fingerprint 覆盖 patches）。
        let mut h1 = DefaultHasher::new();
        let mut h2 = DefaultHasher::new();
        mask.hash_fingerprint(&mut h1);
        mask2.hash_fingerprint(&mut h2);
        assert_ne!(h1.finish(), h2.finish());
    }

    // ---- smart_select 位图选区 ----

    fn register_test_bitmap(name: &str, w: u32, h: u32, value: u8) {
        let data = vec![value; (w * h) as usize];
        register_bitmap_mask(
            name.to_owned(),
            Arc::new(DecodedBitmapMask {
                width: w,
                height: h,
                data,
            }),
        );
    }

    fn bitmap_shape(bbox: [i32; 4], blob: &str, mode: &str) -> SelectionShape {
        SelectionShape::from_value(&json!({
            "kind": "bitmap",
            "bbox": bbox,
            "blob": blob,
            "mode": mode,
        }))
    }

    #[test]
    fn bitmap_coverage_samples_registered_mask() {
        register_test_bitmap("sel_test_bm1", 40, 40, 255);
        let s = bitmap_shape([20, 20, 40, 40], "sel_test_bm1", "new");
        // 内点满选。
        assert_eq!(s.coverage(30.0, 30.0), 1.0);
        assert_eq!(s.coverage(59.0, 59.0), 1.0);
        // 外点为 0（包围盒快速剔除）。
        assert_eq!(s.coverage(10.0, 10.0), 0.0);
        assert_eq!(s.coverage(70.0, 70.0), 0.0);
        // 未注册的 blob 退化为 0（宁可不选）。
        let missing = bitmap_shape([20, 20, 40, 40], "sel_test_no_such_blob", "new");
        assert_eq!(missing.coverage(30.0, 30.0), 0.0);
    }

    #[test]
    fn bitmap_half_coverage_interpolates() {
        register_test_bitmap("sel_test_bm_half", 10, 10, 128);
        let s = bitmap_shape([0, 0, 10, 10], "sel_test_bm_half", "new");
        let c = s.coverage(5.0, 5.0);
        assert!(
            (c - 128.0 / 255.0).abs() < 1e-6,
            "半覆盖应为 128/255，实际 {c}"
        );
    }

    /// 判据 3：几何 + 位图、位图 + 位图的布尔组合。
    #[test]
    fn bitmap_boolean_ops_with_geometry() {
        register_test_bitmap("sel_test_b1", 40, 40, 255);
        register_test_bitmap("sel_test_b2", 40, 40, 255);
        // B1 = [20,20,40,40] new，B2 = [100,100,40,40] add。
        let mut set = SelectionSet::new();
        set.push(bitmap_shape([20, 20, 40, 40], "sel_test_b1", "new"));
        set.push(bitmap_shape([100, 100, 40, 40], "sel_test_b2", "add"));
        assert_eq!(set.coverage(30.0, 30.0), 1.0); // B1 内
        assert_eq!(set.coverage(110.0, 110.0), 1.0); // B2 内
        assert_eq!(set.coverage(70.0, 70.0), 0.0); // 中间空白

        // 几何矩形与位图相交。
        let mut set2 = SelectionSet::new();
        set2.push(SelectionShape::from_value(&json!({
            "kind": "rect",
            "bbox": {"x": 0.0, "y": 0.0, "w": 50.0, "h": 50.0},
            "mode": "new",
        })));
        set2.push(bitmap_shape([20, 20, 40, 40], "sel_test_b1", "intersect"));
        assert_eq!(set2.coverage(30.0, 30.0), 1.0); // 交集内
        assert_eq!(set2.coverage(10.0, 10.0), 0.0); // 矩形内但位图外
        assert_eq!(set2.coverage(70.0, 70.0), 0.0); // 都在外

        // subtract 减去 B1 后仅 B2 保持。
        let mut set3 = SelectionSet::new();
        set3.push(bitmap_shape([20, 20, 40, 40], "sel_test_b1", "new"));
        set3.push(bitmap_shape([100, 100, 40, 40], "sel_test_b2", "add"));
        set3.push(bitmap_shape([20, 20, 40, 40], "sel_test_b1", "subtract"));
        assert_eq!(set3.coverage(30.0, 30.0), 0.0); // B1 被减掉
        assert_eq!(set3.coverage(110.0, 110.0), 1.0); // B2 保留

        // invert 照常用。
        let inv = SelectionShape {
            geom: SelectionGeom::Bitmap {
                bbox: [20, 20, 40, 40],
                mask: get_bitmap_mask("sel_test_b1"),
            },
            feather: 0.0,
            invert: true,
            mode: SelectionMode::New,
        };
        assert_eq!(inv.coverage(30.0, 30.0), 0.0); // 内点被反选掉
        assert_eq!(inv.coverage(70.0, 70.0), 1.0); // 外点被选中
    }

    #[test]
    fn ensure_bitmap_masks_registered_loads_from_store() {
        // 模拟存储：L8 PNG 字节。
        let data = vec![200u8; 16 * 16];
        let png = decode_png_l8(&crate::png::encode_png_l8(16, 16, &data).expect("编码不会失败"))
            .expect("自己编的必须能解");
        assert_eq!(png.2.len(), 16 * 16);
        let shape = json!({
            "kind": "bitmap",
            "bbox": [5, 5, 16, 16],
            "blob": "sel_test_ensure_blob",
        });
        // 未注册前 from_value 拿不到掩膜。
        let before = SelectionShape::from_value(&shape);
        assert_eq!(before.coverage(10.0, 10.0), 0.0);
        // 注册后绑定成功。
        ensure_bitmap_masks_registered(&[&shape], &|h| {
            if h == "sel_test_ensure_blob" {
                crate::png::encode_png_l8(16, 16, &data)
            } else {
                None
            }
        });
        let after = SelectionShape::from_value(&shape);
        let c = after.coverage(10.0, 10.0);
        assert!((c - 200.0 / 255.0).abs() < 1e-6, "注册后应查到表，实际 {c}");
    }
    // ========== Q2：shape schema 统一 ==========

    #[test]
    fn q2_kind_type_compat() {
        // `type` 应与 `kind` 等价（smart_* 用 type）。
        let via_kind = SelectionShape::from_value(
            &json!({"kind": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}),
        );
        let via_type = SelectionShape::from_value(
            &json!({"type": "rect", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}),
        );
        // 两者都应解析为 Rect（coverage 在中心为 1）。
        assert!((via_kind.coverage(5.0, 5.0) - 1.0).abs() < 1e-6);
        assert!((via_type.coverage(5.0, 5.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn q2_circle_ellipse_synonym() {
        // `circle` 应视为 `ellipse` 同义词。
        let circle = SelectionShape::from_value(
            &json!({"kind": "circle", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}),
        );
        let ellipse = SelectionShape::from_value(
            &json!({"kind": "ellipse", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}),
        );
        // 中心覆盖率应一致。
        let c1 = circle.coverage(5.0, 5.0);
        let c2 = ellipse.coverage(5.0, 5.0);
        assert!((c1 - c2).abs() < 1e-6, "circle={c1} ellipse={c2}");
        // 中心都应为 1。
        assert!((c1 - 1.0).abs() < 1e-6);
    }

    // ========== D1-b：渐变遮罩 ==========

    #[test]
    fn linear_gradient_coverage_ramps_along_axis() {
        // 轴 (0,0)→(10,0)：start 一侧 1、end 一侧 0、中点 0.5；离轴距离不影响投影。
        let s = shape(json!({"kind": "linear_gradient", "start": [0.0, 0.0], "end": [10.0, 0.0]}));
        assert_eq!(s.coverage(-5.0, 0.0), 1.0);
        assert_eq!(s.coverage(0.0, 0.0), 1.0);
        assert_near(s.coverage(5.0, 0.0), 0.5);
        assert_eq!(s.coverage(10.0, 0.0), 0.0);
        assert_eq!(s.coverage(15.0, 0.0), 0.0);
        assert_near(s.coverage(5.0, 7.0), 0.5); // 离轴点投影相同
                                                // smoothstep(0.25) = 0.15625 ⇒ 覆盖度 0.84375。
        assert_near(s.coverage(2.5, 0.0), 0.84375);
        // 斜轴：(10,0) 在 (0,0)→(10,10) 轴上的投影也是 0.5。
        let diag =
            shape(json!({"kind": "linear_gradient", "start": [0.0, 0.0], "end": [10.0, 10.0]}));
        assert_near(diag.coverage(10.0, 0.0), 0.5);
        assert_near(diag.coverage(5.0, 5.0), 0.5);
        // 反选翻转。
        let inv = shape(
            json!({"kind": "linear_gradient", "start": [0.0, 0.0], "end": [10.0, 0.0], "invert": true}),
        );
        assert_eq!(inv.coverage(0.0, 0.0), 0.0);
        assert_eq!(inv.coverage(10.0, 0.0), 1.0);
        assert_near(inv.coverage(5.0, 0.0), 0.5);
    }

    #[test]
    fn linear_gradient_bad_payload_is_empty_not_cover_all() {
        // 已识别 kind 但载荷坏掉 ⇒ 空（不是覆盖全部）：缺 end、重合轴。
        // 对照：未知 kind 仍退化为覆盖全部（既有行为不动）。
        let missing = shape(json!({"kind": "linear_gradient", "start": [0.0, 0.0]}));
        assert_eq!(missing.coverage(5.0, 5.0), 0.0);
        let zero =
            shape(json!({"kind": "linear_gradient", "start": [3.0, 3.0], "end": [3.0, 3.0]}));
        assert_eq!(zero.coverage(3.0, 3.0), 0.0);
        let unknown = shape(json!({"kind": "no_such_kind"}));
        assert_eq!(unknown.coverage(5.0, 5.0), 1.0);
    }

    #[test]
    fn radial_gradient_coverage_falloff_bands() {
        // bbox 10×10、中心 (5,5)、半径 5。
        let hard = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "falloff": 0.0}),
        );
        assert_eq!(hard.coverage(5.0, 5.0), 1.0);
        assert_eq!(hard.coverage(9.0, 5.0), 1.0); // d=0.8 < 1
        assert_eq!(hard.coverage(10.0, 5.0), 1.0); // 边界算内（与椭圆同例）
        assert_eq!(hard.coverage(10.5, 5.0), 0.0); // d=1.1 > 1
        assert_eq!(hard.coverage(0.0, 0.0), 0.0); // 角落 d≈1.41
                                                  // falloff=1：从中心到边缘全程渐变：d=0.5 → 0.5。
        let full = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "falloff": 1.0}),
        );
        assert_eq!(full.coverage(5.0, 5.0), 1.0);
        assert_near(full.coverage(7.5, 5.0), 0.5); // d=0.5
        assert_eq!(full.coverage(10.0, 5.0), 0.0);
        // falloff=0.5：d ≤ 0.5 为 1，d=0.75 → 0.5，d ≥ 1 为 0。
        let half = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "falloff": 0.5}),
        );
        assert_eq!(half.coverage(7.0, 5.0), 1.0); // d=0.4
        assert_near(half.coverage(8.75, 5.0), 0.5); // d=0.75
        assert_eq!(half.coverage(10.0, 5.0), 0.0);
        // falloff 缺失按 0；越界值钳到 [0,1]（2.0 ⇒ 1.0 全程渐变）。
        let def = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}}),
        );
        assert_eq!(def.coverage(9.0, 5.0), 1.0);
        let clamped = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}, "falloff": 2.0}),
        );
        assert_near(clamped.coverage(7.5, 5.0), 0.5);
        // 坏载荷 ⇒ 空：缺 bbox、零面积。
        let no_bbox = shape(json!({"kind": "radial_gradient"}));
        assert_eq!(no_bbox.coverage(5.0, 5.0), 0.0);
        let flat = shape(
            json!({"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 0.0}}),
        );
        assert_eq!(flat.coverage(5.0, 5.0), 0.0);
    }

    #[test]
    fn compiled_mask_linear_gradient_spans_full_canvas() {
        // 线性渐变无界 ⇒ union bbox 取全画布；grid 按投影取值（u8 量化 ±1/255）。
        let mask =
            test_mask(json!({"kind": "linear_gradient", "start": [0.0, 0.0], "end": [64.0, 0.0]}));
        let load = |_s: &str| None;
        let compiled = CompiledMask::build(&mask, 64, 64, &load);
        assert_eq!(compiled.bbox, [0, 0, 64, 64]);
        assert_eq!(compiled.sample(0, 32), 1.0); // start 一侧
        assert!((compiled.sample(32, 32) - 0.5).abs() < 0.01); // 中点
        assert_eq!(compiled.sample(63, 32), 0.0); // end 一侧
                                                  // 恒等式：沿轴单调不增（左 ≥ 中 ≥ 右）。
        assert!(compiled.sample(0, 32) >= compiled.sample(32, 32));
        assert!(compiled.sample(32, 32) >= compiled.sample(63, 32));
        // 反选翻转两端，bbox 仍为全画布。
        let mut inv_mask =
            test_mask(json!({"kind": "linear_gradient", "start": [0.0, 0.0], "end": [64.0, 0.0]}));
        inv_mask.invert = true;
        let inv = CompiledMask::build(&inv_mask, 64, 64, &load);
        assert_eq!(inv.bbox, [0, 0, 64, 64]);
        assert_eq!(inv.sample(0, 32), 0.0);
        assert_eq!(inv.sample(63, 32), 1.0);
    }

    #[test]
    fn compiled_mask_radial_gradient_uses_bbox() {
        // 径向渐变 union bbox 即椭圆 bbox；bbox 外为 default_outside。
        let mask = test_mask(
            json!({"kind": "radial_gradient", "bbox": {"x": 8.0, "y": 8.0, "w": 48.0, "h": 48.0}, "falloff": 0.5}),
        );
        let load = |_s: &str| None;
        let compiled = CompiledMask::build(&mask, 64, 64, &load);
        assert_eq!(compiled.bbox, [8, 8, 48, 48]);
        assert_eq!(compiled.sample(32, 32), 1.0); // 中心
        assert_eq!(compiled.sample(0, 0), 0.0); // bbox 外
                                                // 恒等式：沿半径单调不增（中心 ≥ 中途 > 边缘）。
        let mid = compiled.sample(52, 32); // d≈0.833 ⇒ 约 0.26
        assert!(compiled.sample(32, 32) > mid);
        assert!(mid > compiled.sample(56, 32)); // d=1.0 ⇒ 0
        assert_eq!(compiled.sample(56, 32), 0.0);
    }
}

#[cfg(test)]
mod bitmap_mask_registry_tests {
    //! **N3B-01 判据**（第三轮复审 P1）：位图掩膜注册表必须有上限。
    //!
    //! 判据不是「注册表能注册」而是**它不会无限长**：
    //! 三条断言各抓一种回归 —— 没上限 / 淘汰的不是最旧的 / 显式移除失效。

    use super::{
        ensure_bitmap_masks_registered, get_bitmap_mask, register_bitmap_mask,
        registered_bitmap_mask_count, unregister_bitmap_mask, DecodedBitmapMask, SelectionShape,
        Value,
    };
    use serde_json::json;
    use std::sync::Arc;

    fn dummy(w: u32) -> Arc<DecodedBitmapMask> {
        Arc::new(DecodedBitmapMask {
            width: w,
            height: 1,
            data: vec![255; w as usize],
        })
    }

    #[test]
    fn count_never_exceeds_the_cap() {
        // **核心**：注册远超上限的条数 ⇒ 计数必须被压回上限
        for i in 0..24 {
            register_bitmap_mask(format!("cap_{i}"), dummy(1));
        }
        let n = registered_bitmap_mask_count();
        // **★ 断言必须写死 8**（不能写 `MAX_REGISTERED_MASKS`）——
        //   否则「把常量改大」的变异会让断言**跟着变** ⇒ 判据永远绿 ✗
        //   （我这一轮就是这么错的：变异 `MAX = usize::MAX` ⇒ 断言变成 `n <= usize::MAX` 恒真）
        assert!(n <= 8, "注册表超过上限：{n} > 8");
        // 清干净，避免影响别的测试（同进程共享静态表）
        for i in 0..24 {
            unregister_bitmap_mask(&format!("cap_{i}"));
        }
    }

    #[test]
    fn oldest_is_evicted_first() {
        for i in 0..16 {
            register_bitmap_mask(format!("fifo_{i}"), dummy(1));
        }
        // 最旧的那几条必须已被淘汰（`get` 不到）
        assert!(
            get_bitmap_mask("fifo_0").is_none(),
            "最旧的 fifo_0 应当已被淘汰"
        );
        // 最新的那条必须在
        let newest = "fifo_15".to_owned();
        assert!(
            get_bitmap_mask(&newest).is_some(),
            "最新的 {newest} 必须还在"
        );
        for i in 0..16 {
            unregister_bitmap_mask(&format!("fifo_{i}"));
        }
    }

    /// 复现：一次批量预热 9 条位图选区 ⇒ 最早的一条被 FIFO 淘汰，
    /// 而预热返回值（解不开的名单）为空——丢像素没有任何信号。
    ///
    /// 机制链：`render_region_with_lod` 对全部位图选区调一次
    /// `ensure_bitmap_masks_registered`；注册表上限 8 条 ⇒ 第 9 条注册时
    /// 淘汰最旧的一条；随后 `from_value` 对被淘汰的 blob 绑定 `mask = None`
    /// ⇒ 覆盖率恒 0 ⇒ 该选区裁剪到的对象一个像素不落，而渲染仍返回成功。
    #[test]
    fn bulk_ensure_over_capacity_silently_drops_earliest_masks() {
        let names: Vec<String> = (0..9).map(|i| format!("repro_drop_{i}")).collect();
        for name in &names {
            unregister_bitmap_mask(name);
        }
        let shapes: Vec<Value> = names
            .iter()
            .map(|name| {
                json!({
                    "kind": "bitmap",
                    "bbox": [0, 0, 4, 4],
                    "blob_hash": name,
                    "width": 4,
                    "height": 4,
                })
            })
            .collect();
        let refs: Vec<&Value> = shapes.iter().collect();
        // 全部解码成功（4×4 raw L8，全 255）⇒ 解不开的名单为空。
        let report = ensure_bitmap_masks_registered(&refs, &|_| Some(vec![255u8; 16]));
        // 修前静默的证据：解码全成功，返回值空——没有任何信号说有选区被淘汰。
        // 修后：被淘汰的必须进 `evicted`，调用方据此报错或告警，不许静默。
        assert!(
            report.failed.is_empty(),
            "9 条应全部解码成功，解不开的名单应为空，实际 {:?}",
            report.failed
        );
        assert!(
            report.evicted.contains(&names[0]),
            "最早的 {} 被 FIFO 淘汰后必须进 evicted，实际 {:?}",
            names[0],
            report.evicted
        );
        // 最早的一条已被淘汰：查不到，覆盖率 0。
        assert!(
            get_bitmap_mask(&names[0]).is_none(),
            "最早的 {} 应已被 FIFO 淘汰",
            names[0]
        );
        let first = SelectionShape::from_value(&shapes[0]);
        assert_eq!(
            first.coverage(1.0, 1.0),
            0.0,
            "被淘汰选区的覆盖率应退化为 0（即该选区裁剪到的对象不落笔）"
        );
        // 最新的一条还在：覆盖率 1。
        assert!(
            get_bitmap_mask(&names[8]).is_some(),
            "最新的 {} 必须还在",
            names[8]
        );
        let last = SelectionShape::from_value(&shapes[8]);
        assert_eq!(last.coverage(1.0, 1.0), 1.0);
        for name in &names {
            unregister_bitmap_mask(name);
        }
    }

    #[test]
    fn explicit_unregister_frees_immediately() {
        register_bitmap_mask("solo".to_owned(), dummy(2));
        assert!(get_bitmap_mask("solo").is_some(), "注册后应能查到");
        unregister_bitmap_mask("solo");
        assert!(get_bitmap_mask("solo").is_none(), "移除后不该还能查到");
    }
}
#[test]
fn selection_from_value_path_gives_nonempty_rings() {
    // **`subpaths` 载荷 ⇒ `Path{rings}` 非空，且覆盖度内 1 外 0** ✓（第 1548 轮）
    //   **⇒ 为什么留这条**：外部 `B1`（路径选区在裁剪里"找不到"）时会怀疑**这一层** ✗
    //     ⇒ 这条单测证明**这一层是好的** ⇒ 于是搜索面直接缩小到"裁剪侧" ✓
    let payload = serde_json::json!({
        "kind": "path",
        "fill_rule": "nonzero",
        "subpaths": [{
            "start": [90.0, 90.0],
            "closed": true,
            "segments": [
                {"op": "L", "to": [210.0, 90.0]},
                {"op": "L", "to": [210.0, 210.0]},
                {"op": "L", "to": [90.0, 210.0]}
            ]
        }]
    });
    let shape = SelectionShape::from_value(&payload);
    let inside = shape.coverage(150.0, 150.0);
    let outside = shape.coverage(20.0, 150.0);
    assert!(inside > 0.5, "中心必须被覆盖 ⇒ 实测 {inside}");
    assert!(outside < 0.5, "外部不该被覆盖 ⇒ 实测 {outside}");
}
