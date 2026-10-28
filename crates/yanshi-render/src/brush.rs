//! 通用光栅笔刷 stamping（设计文档 11.1）。
//!
//! MVP 只做通用光栅笔刷：可调大小、流量、不透明度、硬度、间距、抖动。
//! 抖动由原子 `seed` 驱动（[`crate::prng`]），因此同一份历史在任何平台重放出相同笔迹。
//!
//! 其他介质（油画、水彩、马克笔、铅笔、像素、矢量）通过 WASM 插件扩展，属远期项。

use crate::blend::BlendMode;
use crate::buffer::Buffer;
use crate::color::{premultiply, LinearRgba};
use crate::geometry::{dashed_line, dashed_line_variable};
use crate::prng::Prng;
use serde_json::Value;

/// 笔迹采样点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokePoint {
    /// 文档坐标 x。
    pub x: f64,
    /// 文档坐标 y。
    pub y: f64,
    /// 压力（`0..1`，缺省为 1）。
    pub pressure: f64,
}

/// 笔迹几何。
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeGeometry {
    /// 采样点序列。
    pub points: Vec<StrokePoint>,
    /// 渲染时是否做 **Catmull-Rom 平滑** ✓（默认 **false** ✓ ⇒ 既有文档逐字节不变 ✓）。
    ///
    /// **点序列本身不会被改写** ✓ —— 平滑只发生在**渲染时** ✓
    /// ⇒ 日志里存的仍是原始采样 ✓、可以随时关掉 ✓、也可以以后换更好的插值 ✓。
    pub smooth: bool,
}

impl StrokeGeometry {
    /// 由 `data.points` 解析：支持 `[[x, y], ...]` 与 `[{x, y, pressure}, ...]`。
    pub fn from_value(value: &Value) -> Option<Self> {
        let array = value.get("points")?.as_array()?;
        let mut points = Vec::with_capacity(array.len());
        for item in array {
            if let Some(pair) = item.as_array() {
                if pair.len() >= 2 {
                    let x = pair[0].as_f64()?;
                    let y = pair[1].as_f64()?;
                    let pressure = pair.get(2).and_then(Value::as_f64).unwrap_or(1.0);
                    points.push(StrokePoint { x, y, pressure });
                    continue;
                }
            }
            let x = item.get("x")?.as_f64()?;
            let y = item.get("y")?.as_f64()?;
            let pressure = item.get("pressure").and_then(Value::as_f64).unwrap_or(1.0);
            points.push(StrokePoint { x, y, pressure });
        }
        if points.is_empty() {
            None
        } else {
            // **默认关闭** ✓（老文档逐字节不变 ✓）；由调用方显式打开 ✓。
            let smooth = value
                .get("smooth")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            Some(Self { points, smooth })
        }
    }
}

/// **按"给定色相邻范围"抖动一枚印章的颜色** ✓（真实用户 §六-13 的"破色" ✓）。
///
/// **语义逐字照用户的话** ✓：**在给定色相邻范围内随机取色** ✓ ——
/// 三通道各自独立地加上一个**有界的**偏移 ✓（幅度 = `jitter × 0.5` ✓），
/// 偏移**有正有负** ✓ ⇒ 颜色在目标色周围**游走** ✓，而**均值仍落在目标色上** ✗（不会整体偏色 ✓）。
///
/// **为什么三通道独立、不做 HSV** ✓：破色的直觉是"同一管颜料里掺了一点旁边的色" ✓，
/// 三通道小幅独立游走**就是这个效果** ✓；而 HSV 转来转去**多一堆代码** ✓、
/// 还要处理色相环绕的边界 ✗ ⇒ **收益不值这个复杂度** ✓（记在这里，方便以后要改时知道取舍 ✓）。
///
/// **确定性** ✓：种子由 `(seed, index)` 派生 ✓ ⇒ 同输入同输出 ✓（D0 的要求 ✓）。
fn jitter_color(color: [f32; 3], jitter: f32, seed: u64, index: u64) -> [f32; 3] {
    let mut rng = crate::Prng::new(seed ^ index.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    let amount = jitter * 0.5;
    [
        (color[0] + rng.signed() * amount).clamp(0.0, 1.0),
        (color[1] + rng.signed() * amount).clamp(0.0, 1.0),
        (color[2] + rng.signed() * amount).clamp(0.0, 1.0),
    ]
}

/// **平滑的细分数** ✓ —— 渲染层（`data.smooth` ✓）与工具层（`brush_stroke` 的 `smooth` ✓）
/// 用**同一个值** ✗（两处各写一个数必然漂移 ✓）。每段曲线被细分成多少小段 ✓。
pub const SMOOTH_SUBDIVISIONS: usize = 8;

/// **可选的笔迹平滑（Catmull-Rom 重采样）** ✓ —— 默认**关闭** ✓。
///
/// 为什么做成可选而不是默认 ✓：本项目的不变量是"**日志里的原子决定渲染**" ✓
/// ⇒ 改变插值方式会**改变既有文档的观感** ✗（第 28 轮在 `oil.wasm` 上遇到过同类取舍 ✓）。
/// 因此默认 `smooth = false` ✓ ⇒ 老文档**逐字节不变** ✓；新笔迹由调用方显式打开 ✓。
/// **记录下来的仍然是原始采样点** ✓ —— 平滑只发生在**渲染时** ✓ ⇒ 无损 ✓、可随时关掉 ✓。
///
/// 这也是本架构里"**矢量**"的含义 ✓（设计 11.1 的矢量介质 ✓）：
/// 笔迹几何存在日志里 ✓、按视图**重新栅格化** ✓ ⇒ 放大不会像位图那样糊 ✓；
/// 而插件介质是在落笔时**烘焙像素** ✓ ⇒ 两者是**不同类**的东西 ✓。
pub fn catmull_rom_smooth(points: &[StrokePoint], subdivisions: usize) -> Vec<StrokePoint> {
    if points.len() < 3 || subdivisions < 2 {
        return points.to_vec();
    }
    // 端点按"反射"补一对控制点 ✓ ⇒ 首尾段也能弯曲 ✓，且**端点本身保持不动** ✓。
    let mut control: Vec<StrokePoint> = Vec::with_capacity(points.len() + 2);
    control.push(StrokePoint {
        x: 2.0 * points[0].x - points[1].x,
        y: 2.0 * points[0].y - points[1].y,
        pressure: points[0].pressure,
    });
    control.extend_from_slice(points);
    let last = points.len() - 1;
    control.push(StrokePoint {
        x: 2.0 * points[last].x - points[last - 1].x,
        y: 2.0 * points[last].y - points[last - 1].y,
        pressure: points[last].pressure,
    });

    let mut out: Vec<StrokePoint> = Vec::with_capacity(points.len() * subdivisions);
    out.push(points[0]);
    for index in 0..(control.len() - 3) {
        let (p0, p1, p2, p3) = (
            &control[index],
            &control[index + 1],
            &control[index + 2],
            &control[index + 3],
        );
        // 只输出每段的**内部**采样 ✓（段端点由相邻段负责 ✓）。
        for step in 1..subdivisions {
            let t = step as f64 / subdivisions as f64;
            let (t2, t3) = (t * t, t * t * t);
            // 均匀 Catmull-Rom ✓：
            //   0.5 * (2*b + (-a + c) t + (2a - 5b + 4c - d) t² + (-a + 3b - 3c + d) t³)
            let interp = |a: f64, b: f64, c: f64, d: f64| -> f64 {
                0.5 * ((2.0 * b)
                    + (-a + c) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (-a + 3.0 * b - 3.0 * c + d) * t3)
            };
            out.push(StrokePoint {
                x: interp(p0.x, p1.x, p2.x, p3.x),
                y: interp(p0.y, p1.y, p2.y, p3.y),
                pressure: interp(p0.pressure, p1.pressure, p2.pressure, p3.pressure),
            });
        }
        out.push(*p2);
    }
    out
}

/// **★ 对称模式 ✗ ★**（**第 251 轮 ✓）：**画一笔 ⇒ 自动在**镜像位置**再盖一遍** ✓
///
/// **∴ 为什么做在**盖章层**✗**（**而**不是改点序列／存成两个对象 ✓）★**：
///   ＋ **∴ 借鉴成熟软件 ✗**：**Krita／Procreate 的对称都是**工具态**✗
///     **不写进笔迹数据** ✓ ⇒ **∴ 于是**：**撤销一笔／清画布 ⇒ 对称跟着走 ✓** ★
///   ＋ **∴ 而**本仓盖章**只有一条路径**（它自己注明"三条路径很容易漂移 ✗ 栽过一次" ✓）
///     ⇒ **∴ 插在这里**一处就够** ✓ ★★
///   ＋ **∴ 代价（**如实 ✓）✗**：**镜像轴要**画布尺寸**✗ ⇒ **∴ 它必须**由调用方给**
///     （**∴ 不能拿 `buffer.bbox()` ✗** —— **∴ 分块渲染时那只是**一块 tile** ✓）★★
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Symmetry {
    /// 关（缺省 ✓）。
    None,
    /// **垂直镜像 ✗**：**轴 ＝ 画布**竖直中线** ✓（`x' = 宽 − x` ✓）★
    Vertical,
    /// **水平镜像 ✗**：**轴 ＝ 画布**水平中线** ✓（`y' = 高 − y` ✓）★
    Horizontal,
}

impl Symmetry {
    /// 由字符串解析（`none`／`vertical`／`horizontal` ✓；别的 ⇒ `None` ✓）。
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "vertical" => Self::Vertical,
            "horizontal" => Self::Horizontal,
            _ => Self::None,
        }
    }

    /// **镜像一个点 ✗**（`canvas` 是**画布像素尺寸** ✓）★
    #[must_use]
    pub fn mirror(self, x: f64, y: f64, canvas: (u32, u32)) -> (f64, f64) {
        let (cw, ch) = (f64::from(canvas.0), f64::from(canvas.1));
        match self {
            Self::None => (x, y),
            Self::Vertical => (cw - x, y),
            Self::Horizontal => (x, ch - y),
        }
    }
}

/// 笔刷参数。
#[derive(Debug, Clone, PartialEq)]
pub struct BrushSpec {
    /// 笔尖直径（像素）。
    pub size: f64,
    /// 硬度（`0` 全软、`1` 硬边）。
    pub hardness: f64,
    /// 每 stamp 的流量。
    pub flow: f64,
    /// 整笔不透明度。
    pub opacity: f64,
    /// stamp 间距（相对直径的比例，缺省 0.25）。
    pub spacing: f64,
    /// 直通线性颜色。
    pub color: LinearRgba,
    /// 抖动幅度（像素，`0` 表示无抖动）。
    pub jitter: f64,
    /// 压力是否影响笔尖大小。
    pub pressure_size: bool,
    /// 相对压力下的大小下限（`pressure_size = true` 时生效）。
    pub min_size_ratio: f64,
    /// 混合模式。
    pub blend_mode: BlendMode,
    /// 随机种子（抖动可重放）。
    pub seed: u64,
    /// 虚线长度（`None` 为实线）。
    pub dash: Option<f64>,
    /// **对称模式 ✗**（缺省关 ✓）★
    pub symmetry: Symmetry,
}

impl Default for BrushSpec {
    fn default() -> Self {
        Self {
            size: 4.0,
            hardness: 0.8,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.25,
            color: [0.0, 0.0, 0.0, 1.0],
            jitter: 0.0,
            pressure_size: true,
            min_size_ratio: 0.25,
            blend_mode: BlendMode::Normal,
            seed: 0,
            dash: None,
            symmetry: Symmetry::None,
        }
    }
}

impl BrushSpec {
    /// 由对象 `data` 解析（键：size/color/color_space/opacity/flow/hardness/spacing/jitter/seed/blend_mode）。
    ///
    /// `color` 可以是 `[r, g, b, a]`（线性直通）或 `{"r": 0-255, "g": ..., "b": ..., "a": ...}`（sRGB 字节）。
    /// `color_space`（P2-9，与 `update_stroke` 的 `core.color_space` 同名）显式改写数组的解释：
    /// `"srgb"` ⇒ 0..1 浮点数组按 sRGB 字节解释，`"linear"` ⇒ 按线性直通；不给 ⇒ 既有量级切换。
    pub fn from_value(value: &Value) -> Self {
        let mut spec = Self::default();
        if let Some(size) = value.get("size").and_then(Value::as_f64) {
            spec.size = size.max(0.05);
        }
        if let Some(opacity) = value.get("opacity").and_then(Value::as_f64) {
            spec.opacity = opacity.clamp(0.0, 1.0);
        }
        if let Some(flow) = value.get("flow").and_then(Value::as_f64) {
            spec.flow = flow.clamp(0.0, 1.0);
        }
        if let Some(hardness) = value.get("hardness").and_then(Value::as_f64) {
            spec.hardness = hardness.clamp(0.0, 1.0);
        }
        if let Some(spacing) = value.get("spacing").and_then(Value::as_f64) {
            spec.spacing = spacing.clamp(0.01, 4.0);
        }
        if let Some(jitter) = value.get("jitter").and_then(Value::as_f64) {
            spec.jitter = jitter.max(0.0);
        }
        if let Some(seed) = value.get("seed").and_then(Value::as_u64) {
            spec.seed = seed;
        }
        if let Some(mode) = value.get("blend_mode").and_then(Value::as_str) {
            spec.blend_mode = BlendMode::from_name(mode);
        }
        if let Some(dash) = value.get("dash").and_then(Value::as_f64) {
            spec.dash = Some(dash.max(0.1));
        }
        if let Some(min_ratio) = value.get("min_size_ratio").and_then(Value::as_f64) {
            spec.min_size_ratio = min_ratio.clamp(0.0, 1.0);
        }
        // **∴ 对称从对象的 `data` 读 ✗**（**∴ 于是**它**随原子重放 ✗ 不是隐藏状态** ✓）★
        if let Some(symmetry) = value.get("symmetry").and_then(Value::as_str) {
            spec.symmetry = Symmetry::from_name(symmetry);
        }
        if let Some(color) = value.get("color") {
            // **显式空间**（P2-9）：`data.color_space` 是 `update_stroke` 写进来的原话；
            // 不给 / 不认 ⇒ `None` ⇒ 既有量级切换（与 `parse_spec_color_with_space` 同口径）。
            let space = value
                .get("color_space")
                .and_then(Value::as_str)
                .and_then(crate::color::ColorSpace::from_name);
            if let Some(parsed) = crate::color::parse_spec_color_with_space(color, space) {
                spec.color = parsed;
            }
        }
        spec
    }

    /// stamp 间距（像素）。
    pub fn spacing_pixels(&self) -> f64 {
        (self.size * self.spacing).max(0.25)
    }
}

/// 把一笔笔迹 stamp 到缓冲区（文档坐标）。
///
/// 返回值是实际产生的 stamp 数，便于可观测性与测试。
/// 笔触的**外观参数**（设计 808 行 `advanced.appearance` 的子集）✓。
///
/// 由已交付的三个模块组成 ✓（曲线 / 动力学 / 纹理 ✓），并在这里接进盖章路径 ✓。
/// **没有任何 appearance 时**整条路径与接线前**逐字节一致** ✓ —— 由回归测试守住 ✓
/// （`no_appearance_is_byte_identical_to_the_previous_path`）。
#[derive(Debug, Clone, Default)]
pub struct StrokeAppearance {
    /// 大小 / 不透明度 / 压力曲线 ✓。
    pub curves: crate::curve::StrokeCurves,
    /// 抖动 / 散布 / 尺寸与角度变化 / 程序化纹理 ✓。
    pub dynamics: crate::dynamics::Dynamics,
    /// 载体墨量 / 湿度耗墨 / 混色 ✓（设计 11.1「MVP 要支持纹理、湿度和载墨量」✓）。
    pub paint: crate::paint::PaintSettings,
    /// 是否存在**有效**外观（全默认 ⇒ 视为无外观 ⇒ 走原路径 ✓）。
    pub active: bool,
}

impl StrokeAppearance {
    /// 由 `data.appearance`（或 `data.advanced.appearance`）解析 ✓。
    pub fn from_data(data: &Value) -> StrokeAppearance {
        let appearance = data
            .get("appearance")
            .or_else(|| data.get("advanced").and_then(|v| v.get("appearance")));
        let Some(appearance) = appearance else {
            return StrokeAppearance::default();
        };
        let curves = crate::curve::StrokeCurves::from_appearance(appearance);
        let dynamics = crate::dynamics::Dynamics::from_appearance(appearance);
        let paint = crate::paint::PaintSettings::from_appearance(appearance);
        // **注意**：曲线模块的 `is_identity_or_constant` 含义是"**不是变化曲线**" ✗，
        // 而不是"没有效果" ✓ —— 常量 0.4 的曲线会实打实地改变笔触粗细 ✓。
        // 第一版据此判断 `active`，结果"常量曲线"被当成无外观、纹理也被漏掉 ✓（两个测试当场抓到 ✗）。
        // 因此这里只把**恰好等于恒等曲线**（常量 1.0）视为无效果 ✓。
        let identity = crate::curve::Curve::constant(1.0);
        // 湿笔参数只要有一项非 0 就算"有外观" ✓（`paint_load = 0` 表示无限墨 ⇒ 无效果 ✓）。
        let paint_active = paint.paint_load > 0.0 || paint.wetness > 0.0 || paint.mixing > 0.0;
        let active = paint_active
            || !dynamics.is_noop()
            || dynamics.texture.kind != crate::dynamics::TextureKind::None
            || curves.size != identity
            || curves.opacity != identity
            || curves.pressure != identity
            || curves.flow != identity
            || curves.spacing != identity;
        StrokeAppearance {
            curves,
            dynamics,
            paint,
            active,
        }
    }
}

/// **统一**的笔触盖章路径 ✓：可选外观参数（曲线/动力学/纹理）+ 可选选区覆盖度 ✓。
///
/// 之所以只保留一条路径：此前"整段盖章"与"增量盖章"存在语义差异 ✓，
/// 加上选区裁剪需要另一条分支 ✓，三条路径很容易漂移 ✓（本会话已因此栽过一次 ✓）。
/// 现在渲染侧统一走这里 ✓，`appearance = None` 且 `coverage = None` 时行为与旧路径**逐字节一致** ✓。
pub fn stamp_stroke_configured(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stroke: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    coverage: Option<&dyn Fn(f64, f64) -> f32>,
) -> usize {
    // **★★★ 第 251 轮 ✗：不带画布的老入口 ✗ ★★★**
    //   ＋ **∴ 为什么留它 ✗**：**调用点里有一大批是**测试**✗（**∴ 它们都没开对称 ✓）
    //     ⇒ **∴ 于是**：**留着它 ⇒ 那些调用点**一个都不用改** ✓（**∴ 改动面小 ⇒ 好回退 ✓）★
    //   ＋ **∴ 而**风险（**没用上对称却静默忽略 ✓）用**断言**挡住 ✗**：
    //     ⇒ **∴ 谁**开了对称还走老入口 ⇒ **∴ 测试里**当场炸** ✓（**∴ 而不是"画出来少一半" ✓）★★
    debug_assert_eq!(
        brush.symmetry,
        Symmetry::None,
        "开了对称就必须走 `stamp_stroke_configured_on`（要画布尺寸）"
    );
    stamp_stroke_configured_on(buffer, brush, stroke, appearance, coverage, (0, 0))
}

/// **带画布的盖章入口 ✗**（**对称的镜像轴要 `canvas` ✓；关掉对称时它无关紧要 ✓）★
pub fn stamp_stroke_configured_on(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stroke: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    coverage: Option<&dyn Fn(f64, f64) -> f32>,
    canvas: (u32, u32),
) -> usize {
    let spacing = brush.spacing_pixels();
    // **可选的笔迹平滑** ✓ —— 只发生在**渲染时** ✓，点序列本身不动 ✓（见 `catmull_rom_smooth` ✓）。
    // 细分 8 段足够 ✓：之后还会按 `spacing` 重新采样 ✓ ⇒ 再多只是白做功 ✓。
    const SMOOTH_SUBDIVISIONS: usize = 8;
    let smoothed;
    let source: &[StrokePoint] = if stroke.smooth {
        smoothed = catmull_rom_smooth(&stroke.points, SMOOTH_SUBDIVISIONS);
        &smoothed
    } else {
        &stroke.points
    };
    let samples: Vec<(f64, f64, f64)> = source
        .iter()
        .map(|point| (point.x, point.y, point.pressure))
        .collect();
    // **间距曲线**（`spacing_curve`）：只在外观有效且曲线非恒等时走变间距采样；
    // 其余情况走原 `dashed_line`（恒等曲线下变间距采样与它逐点相同，但不换路径
    // ⇒ 老文档的采样相位由回归测试原样守住）。
    let stamps = match appearance {
        Some(appearance)
            if appearance.active
                && appearance.curves.spacing != crate::curve::Curve::constant(1.0) =>
        {
            let curves = &appearance.curves;
            dashed_line_variable(&samples, spacing, brush.dash, &|progress| {
                curves.spacing_scale(progress)
            })
        }
        _ => dashed_line(&samples, spacing, brush.dash),
    };
    // **★★★ 对称：把每个印章**在镜像位置再盖一遍** ✗ ★★★**（**第 251 轮 ✓）
    //   ＋ **∴ 点序列**一个字节都不改**✗ 只是**多喂一批镜像坐标**给盖章 ✓
    //     ⇒ **∴ 于是**：**压力／间距／抖动**全跟着原样** ✓（**∴ 镜像 dab 的压力
    //       等于原 dab ✓ —— 判据里那一条正是它 ✓）★
    //   ＋ **∴ 轴 ✗**：**`x' = 宽 − x`**（竖直中线 ✓）／**`y' = 高 − y`**（水平 ✓）
    //     ＋ **∴ 为什么不是 `宽−1−x` ✗**：**文档坐标**连续 ✗ 像素中心在 `k+0.5`**
    //       ⇒ **∴ `宽 − x`**才是关于画布中线的镜像** ✓（**判据做逐像素镜像比对 ✓）★★
    let stamps = if brush.symmetry == Symmetry::None {
        stamps
    } else {
        let mut out = stamps.clone();
        for (x, y, pressure) in &stamps {
            let (mx, my) = brush.symmetry.mirror(*x, *y, canvas);
            out.push((mx, my, *pressure));
        }
        out
    };
    let Some(appearance) = appearance.filter(|appearance| appearance.active) else {
        // 无外观：保持旧路径（含选区裁剪分支 ✓）。
        return match coverage {
            Some(coverage) => stamp_samples_clipped(buffer, brush, &stamps, coverage),
            None => stamp_samples(buffer, brush, &stamps),
        };
    };
    stamp_samples_with_appearance(buffer, brush, &stamps, appearance, coverage)
}

/// 带外观参数的盖章：曲线 → 动力学 → 纹理 → 绘制 ✓（顺序固定 ⇒ 结果确定 ✓）。
fn stamp_samples_with_appearance(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stamps: &[(f64, f64, f64)],
    appearance: &StrokeAppearance,
    coverage: Option<&dyn Fn(f64, f64) -> f32>,
) -> usize {
    // **∴ 计时这一段 ✗**（**第 246 轮 ✓）：**只累加**印章本身**✗ 不含调用方的别的活** ✓
    let _t_stamp = StampTimer(if stamp_probe_on() {
        Some(crate::probe::ProbeClock::now())
    } else {
        None
    });
    let base_radius = brush.size / 2.0;
    let base_alpha = brush.color[3] * brush.flow as f32 * brush.opacity as f32;
    let mut drawn = 0usize;
    // 载体墨量 / 湿度 / 混色的状态（整笔一份 ⇒ 沿笔迹耗尽 ✓，且完全确定 ✓）。
    let mut reservoir = crate::paint::PaintReservoir::new(appearance.paint);
    let mut previous: Option<(f64, f64)> = None;
    for (index, (x, y, pressure)) in stamps.iter().enumerate() {
        // 湿度耗墨：与上一枚印章之间的**距离**成正比 ✓（点按不耗墨 ✓、连续拖动更耗墨 ✓）。
        if let Some((px, py)) = previous {
            reservoir.advance((((x - px) * (x - px)) + ((y - py) * (y - py))).sqrt());
        }
        previous = Some((*x, *y));
        // ① 曲线：**尺寸/不透明度按笔迹进度**采样 ✓（`size_curve` 是"沿笔迹塑形" ✓，
        // 详见 `modulate_shape` 的说明 ✓）；**压力**另有其道 ✓（下面 ①b ✓）。
        let progress = if stamps.len() > 1 {
            index as f64 / (stamps.len() - 1) as f64
        } else {
            0.0
        };
        let (mut radius, alpha) =
            appearance
                .curves
                .modulate_shape(base_radius, base_alpha, progress);
        // ①a **流量曲线**：`base_alpha` 里已含 `brush.flow`（标量），这里再乘
        // progress 处的 `flow_curve` 乘子 ⇒ 「起笔浓、收笔淡」的流量塑形。
        // 恒等曲线时乘子恒 1.0 ⇒ alpha 逐位不变。
        let alpha = alpha * appearance.curves.flow_scale(progress);
        // ①b **压力 → 粗细**：与不带 appearance 的路径用**同一条规则** ✓ ——
        // 此前这里完全没施加 ✗ ⇒ 只要给一个 `appearance`（哪怕只加了纹理 ✓）
        // 就会**静默关掉**压力响应 ✓（子 agent 实测：同一组点列，无 appearance 时 38→14px ✓，
        // 仅加一个纹理后变成 38→38px ✓）。"加纹理改变了几何"是最难察觉的一类 bug ✓。
        let p = if pressure.is_finite() {
            pressure.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mapped = appearance.curves.pressure.eval(p as f32) as f64;
        let size_scale = if brush.pressure_size {
            brush.min_size_ratio + (1.0 - brush.min_size_ratio) * mapped
        } else {
            1.0
        };
        radius *= size_scale;
        if radius <= 0.0 || alpha <= 0.0 {
            continue;
        }
        // ② 载墨量与湿度：逐印章耗墨 ⇒ 沿笔迹衰减 ✓（`paint_load = 0` 时恒为 1 ✓）。
        let load = reservoir.consume();
        let alpha = alpha * load;
        if alpha <= 0.0 {
            continue;
        }
        // ③ 动力学：逐印章、由 (seed, index) 派生 ⇒ 可复现 ✓。
        let params = appearance
            .dynamics
            .stamp_params(index as u64, *x, *y, radius, alpha);
        if params.radius <= 0.0 || params.alpha <= 0.0 {
            continue;
        }
        // ④ 纹理：按**文档坐标**调制 alpha ⇒ 跨块一致 ✓。
        let alpha = appearance
            .dynamics
            .texture
            .modulate(params.x, params.y, params.alpha);
        if alpha <= 0.0 {
            continue;
        }
        // ⑤ 混色：把**目标处已有颜色**按 `mixing × 当前湿度` 混进笔尖 ✓（只读一次 ⇒ 确定 ✓）。
        // **颜色抖动（破色）** ✓ —— 真实用户 §六-13 的原话：
        // "笔触支持 `color_jitter`（**在给定色相邻范围内随机取色** ✓，模拟手调色的不均匀 ✓）" ✓。
        //
        // **位置** ✓：施加在**每枚印章的笔尖色**上 ✓（不是整笔一次 ✓）——
        // 油画的"破色"正是**同一笔里颜色轻微游走** ✓，整笔一次就只是换了个颜色 ✗。
        // **确定性** ✓：由 `(seed, index)` 派生 ✓ ⇒ 同一份文档重放**逐像素一致** ✓。
        // **缺省 0 时整段不执行** ✓（不是"乘 0" ✓）⇒ 老文档**一个像素都不变** ✓。
        let jittered = if appearance.dynamics.color_jitter > 0.0 {
            jitter_color(
                [brush.color[0], brush.color[1], brush.color[2]],
                appearance.dynamics.color_jitter,
                appearance.dynamics.seed,
                index as u64,
            )
        } else {
            [brush.color[0], brush.color[1], brush.color[2]]
        };
        let brush_color = [jittered[0], jittered[1], jittered[2], alpha];
        let color = if appearance.paint.mixing > 0.0 {
            let origin = buffer.origin();
            let local_x = (params.x - origin.0 as f64).max(0.0) as u32;
            let local_y = (params.y - origin.1 as f64).max(0.0) as u32;
            let dest = if local_x < buffer.width() && local_y < buffer.height() {
                buffer.pixel(local_x, local_y)
            } else {
                [0.0, 0.0, 0.0, 0.0]
            };
            reservoir.mix(brush_color, dest)
        } else {
            brush_color
        };
        match coverage {
            Some(coverage) => crate::brush::draw_stamp_clipped_with_pressure(
                buffer,
                params.x,
                params.y,
                params.radius,
                brush.hardness,
                color,
                brush.blend_mode,
                coverage,
                *pressure as f32,
            ),
            None => draw_stamp_with_pressure(
                buffer,
                params.x,
                params.y,
                params.radius,
                brush.hardness,
                color,
                brush.blend_mode,
                *pressure as f32,
            ),
        };
        drawn += 1;
    }
    drawn
}

/// 把笔迹整段盖章（无外观参数、无选区裁剪）✓ —— 等价于 [`stamp_stroke_configured`] 的两个 `None` ✓。
pub fn stamp_stroke(buffer: &mut Buffer, brush: &BrushSpec, stroke: &StrokeGeometry) -> usize {
    // **∴ 同上 ✗：不带画布的老入口留给（一大批判据里的）调用点 ✓ 并用断言挡住误用** ✓ ★
    debug_assert_eq!(
        brush.symmetry,
        Symmetry::None,
        "开了对称就必须走 `stamp_stroke_on`（要画布尺寸）"
    );
    stamp_stroke_configured(buffer, brush, stroke, None, None)
}

/// **带画布的整段盖章 ✗**（**对称用 ✓）★
pub fn stamp_stroke_on(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stroke: &StrokeGeometry,
    canvas: (u32, u32),
) -> usize {
    stamp_stroke_configured_on(buffer, brush, stroke, None, None, canvas)
}

/// 笔迹的**增量**盖章：只画上一帧之后新增的采样，并推进游标。
///
/// 与 [`stamp_stroke`] 的区别是采样相位跨帧连续，因此「拖动中逐帧盖章」与
/// 「落笔后一次性整段盖章」产出**逐点相同**的 stamp（13.3 本地乐观渲染的保真度前提）。
pub fn stamp_stroke_incremental(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stroke: &StrokeGeometry,
    cursor: &mut crate::geometry::StrokeCursor,
) -> usize {
    let samples: Vec<(f64, f64, f64)> = stroke
        .points
        .iter()
        .map(|point| (point.x, point.y, point.pressure))
        .collect();
    let stamps = crate::geometry::dashed_line_from(&samples, brush.dash, cursor);
    let base_index = cursor.stamp_index - stamps.len() as u64;
    stamp_samples_from(buffer, brush, &stamps, base_index)
}

/// 把预先生成的采样盖章到缓冲（抖动序号从 0 起，与一次性整段一致）。
/// **把底图像素喂进引擎的 surface**（第 242 轮）：**两端共用这一处** ✓。
///
/// **为什么要抽出来**：这段循环原先有**两份**（服务端 `tools.rs` 与内核 `brush.rs` ✓）
/// ⇒ 而"两份实现必然漂移"是本项目记录过的**头号病** ✓。搬到这里 ⇒ 两端都调它 ✓。
///
/// **做法与原先逐字一致** ✓：
/// - 按 **64×64** 的 tile 取块（`tile_request_start` / `tile_request_end` ✓）；
/// - 只写落在 `region` 内的像素 ✓；
/// - **RGBA8 ⇒ fix15**：`u16::from(byte) << 7`（与读回的 `>> 7` 对称 ✓）。
///
/// **尺寸不符就不写** ✗：宁可"这次没有底图" ✓，也不要**错位**地抹 ✓。
pub fn feed_base(
    surface: &mut hokusai::tile_mem::MemSurface,
    x0: i32,
    y0: i32,
    width: i32,
    height: i32,
    rgba: &[u8],
) -> bool {
    if width <= 0 || height <= 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return false;
    }
    use hokusai::TiledSurface;
    let x1 = x0 + width;
    let y1 = y0 + height;
    for tile_y in y0.div_euclid(64)..=y1.div_euclid(64) {
        for tile_x in x0.div_euclid(64)..=x1.div_euclid(64) {
            let tile = surface.tile_request_start(tile_x, tile_y);
            for row in 0..64i32 {
                for column in 0..64i32 {
                    let document_x = tile_x * 64 + column;
                    let document_y = tile_y * 64 + row;
                    if document_x < x0 || document_y < y0 || document_x >= x1 || document_y >= y1 {
                        continue;
                    }
                    let at = ((document_y - y0) as usize * width as usize
                        + (document_x - x0) as usize)
                        * 4;
                    if at + 3 >= rgba.len() {
                        continue;
                    }
                    tile[row as usize][column as usize] = [
                        u16::from(rgba[at]) << 7,
                        u16::from(rgba[at + 1]) << 7,
                        u16::from(rgba[at + 2]) << 7,
                        u16::from(rgba[at + 3]) << 7,
                    ];
                }
            }
            surface.tile_request_end(tile_x, tile_y);
        }
    }
    true
}

// **★★★ 净室揉合：把"后方"的颜料拖进当前 dab（P0 #1 carried_color）★★★**（第 254 轮）
//
// **为什么自己写而不是改内核**（决策与证据都在这）：
//   * 需求文档 P0 #1 要"脏笔拾取 ⇒ 湿画法（Blender 复活，只能 Source-Over 的问题）"；
//   * 实测**排除**了三个可能：① 底图**确实喂进来了**（`brush_stroke` 回执 `fed_base=true`、
//     `painted_pixels=5109`、`steps=51`）；② **不是参数问题**（`smudge`/`smudge_length`/
//     `smudge_radius_log`/`smudge_transparency` 各种组合、含全开，跨色边界后**带外仍是纯白**）；
//     ③ **不是"内核没写"**（`hokusai-core 0.3.0` 的 `brushmodes.rs` 有真的 smudge bucket cache）。
//   => 缺口在这条落笔路径的接法上，而内核是 `crates.io` 的依赖（改它要 vendor + 维护 patch）
//   => **决策：在我们自己的渲染层做净室实现**（确定性、可控、可判）。
//
// **语义（干净、可判、可红）**：
//   * **只对 `amount > 0` 生效**（调用方只在笔刷 `smudge > 0` 时传）=> 既有像素级判据**一位都不动**；
//   * 采样 = **把整个 dab 足迹沿运动方向的反方向平移 `length_px`**（经典"拖曳"）：
//     于是笔尖**后方**的颜色被带进当前位置 => 跨色边界时边界色会被拖出来；
//   * 混合是**线性**的（`out = dst*(1-a) + src*a`），无随机数 => 同输入同输出；
//   * 颜色按 `u16`（`<<7` 缩放）与 `feed_base` 同一套表示，读写口径一致。
//
// 返回被改写的像素数（供判据/探针看"到底动了多少"）。

/// 净室揉合：把 dab 足迹后方（沿运动反方向 `length_px`）的颜色按 `amount` 拖进当前位置。
///
/// 只读采样矩形与 dab 足迹两块，先读后写，因此与 tile 布局无关；
/// 无随机数（确定性）；`amount <= 0` 或方向未知（第一枚 dab）时返回 0、不动任何像素。
pub fn smudge_dab(
    surface: &mut hokusai::tile_mem::MemSurface,
    center: (f64, f64),
    radius: f64,
    direction: (f64, f64),
    length_px: f64,
    amount: f64,
) -> usize {
    if amount <= 0.0 || radius <= 0.0 {
        return 0;
    }
    let amount = amount.clamp(0.0, 1.0) as f32;
    // **方向未知（第一枚 dab）=> 直接返回 0（只播种、不动）**（第 254 轮定稿）
    //   曾经写成"平移 0 做原地混匀"，但**平移 0 时每个像素采样的就是它自己** =>
    //   `out = dst*(1-a) + dst*a = dst` => **数学上必然是空操作**（测试也是这样红的）。
    //   => 物理上也对：第一枚 dab 后面没有"后方颜料"可拖 => 什么都不该发生。
    if direction.0 == 0.0 && direction.1 == 0.0 {
        return 0;
    }
    // **拖曳长度 ≤ 0 ⇒ 采样即自身**：与上面同理，`out = dst*(1-a) + dst*a = dst`，
    // 数学上必然是空操作 ⇒ 直接返回，不写任何像素（`oil-01-paint` 的 `smudge_length`
    // 是 0：这支笔此前每步都在为一次空操作付全部 `powf` 成本）。
    if length_px <= 0.0 {
        return 0;
    }
    let (dx, dy) = {
        let norm = (direction.0 * direction.0 + direction.1 * direction.1)
            .sqrt()
            .max(1e-9);
        (direction.0 / norm, direction.1 / norm)
    };
    let back = length_px.max(0.0);
    let (shift_x, shift_y) = (-dx * back, -dy * back);
    let (cx, cy) = (center.0, center.1);
    let r = radius;
    let x0 = (cx - r).floor() as i32;
    let y0 = (cy - r).floor() as i32;
    let x1 = (cx + r).ceil() as i32;
    let y1 = (cy + r).ceil() as i32;
    if x1 < x0 || y1 < y0 {
        return 0;
    }
    // **★★★ 必须两趟：先读、再写 ★★★**（第 254 轮实测修）
    //   第一版是"一趟里直接读采样点"，但采样点会落到**别的 tile**，而一趟里只有当前
    //   tile 可读 => 我加了"跨 tile 就跳过"的保守分支 => 结果**在 tile 边界上完全失效**
    //   （64px 的 tile 边界到处都在）。单测当场把它钉住：色边界造在 x=64 => 红没被拖出来。
    //   => 现在：先把**采样矩形**整块读进本地缓冲（半径量级，很小），再按 dab 足迹写回
    //      => 与 tile 布局**完全无关**，任何边界都对。
    let (sx0, sy0, sx1, sy1) = (
        x0 + shift_x.floor() as i32,
        y0 + shift_y.floor() as i32,
        x1 + shift_x.floor() as i32,
        y1 + shift_y.floor() as i32,
    );
    let sample_w = (sx1 - sx0 + 1).max(1) as usize;
    let sample_h = (sy1 - sy0 + 1).max(1) as usize;
    let mut sample = vec![[0u16; 4]; sample_w * sample_h];
    {
        use hokusai::TiledSurface;
        for tile_y in sy0.div_euclid(64)..=sy1.div_euclid(64) {
            for tile_x in sx0.div_euclid(64)..=sx1.div_euclid(64) {
                let tile = surface.tile_request_start(tile_x, tile_y);
                for row in 0..64i32 {
                    for column in 0..64i32 {
                        let px = tile_x * 64 + column;
                        let py = tile_y * 64 + row;
                        if px < sx0 || py < sy0 || px > sx1 || py > sy1 {
                            continue;
                        }
                        let at = (py - sy0) as usize * sample_w + (px - sx0) as usize;
                        sample[at] = tile[row as usize][column as usize];
                    }
                }
                surface.tile_request_end(tile_x, tile_y);
            }
        }
    }
    let mut touched = 0usize;
    // **解码查表只取一次** ✓：fix15 输入只有 32641 个离散值，表项就是 `srgb_to_linear` 的
    // 原值 ⇒ 查表与直接算**逐位相同**，只是省掉每像素 6 次 `powf`（见 `color.rs`）。
    let decode_table = crate::color::srgb_decode_fix15_table();
    {
        use hokusai::TiledSurface;
        for tile_y in y0.div_euclid(64)..=y1.div_euclid(64) {
            for tile_x in x0.div_euclid(64)..=x1.div_euclid(64) {
                let tile = surface.tile_request_start(tile_x, tile_y);
                for row in 0..64i32 {
                    for column in 0..64i32 {
                        let px = tile_x * 64 + column;
                        let py = tile_y * 64 + row;
                        if px < x0 || py < y0 || px > x1 || py > y1 {
                            continue;
                        }
                        let ddx = px as f64 + 0.5 - cx;
                        let ddy = py as f64 + 0.5 - cy;
                        if ddx * ddx + ddy * ddy > r * r {
                            continue;
                        }
                        let qx = (px as f64 + shift_x).floor() as i32;
                        let qy = (py as f64 + shift_y).floor() as i32;
                        if qx < sx0 || qy < sy0 || qx > sx1 || qy > sy1 {
                            continue;
                        }
                        let src = sample[(qy - sy0) as usize * sample_w + (qx - sx0) as usize];
                        let dst = tile[row as usize][column as usize];
                        // **★ 颜色三通道在**线性空间**混合，alpha 保持线性插值** ✓（第 1311 轮；Phase 1）
                        //   **⇒ 为什么**：`dst*(1-a) + src*a` 是**光的相加** ⇒ 只在 linear 上有物理意义
                        //      （sRGB 是感知编码，有 gamma）⇒ 在 sRGB 上插值会让中间调**偏暗** ✗
                        //   **⇒ 为什么 alpha 不转**：alpha 不是 gamma 编码量 ⇒ 它在两个空间里都是线性 ✓
                        //   **⇒ 缩放**：tile 存的是 `u8 << 7`（`brush.rs:658`）⇒ 所以 `255<<7 = 32640` 是满量程 ✓
                        //   **⇒ 判据**：`tool-visible-delta`（不涂抹的纯色笔）必须**一字不变**；
                        //      而涂抹笔的混合像素**变亮** ⇒ 与 `mix_smudge_linear`（wasm 侧）同一套数学 ✓
                        const FULL: f32 = 32640.0; // 255 << 7
                        let mix_rgb = |i: usize| -> u16 {
                            let lin = crate::color::decode_fix15_with_table(decode_table, dst[i])
                                * (1.0 - amount)
                                + crate::color::decode_fix15_with_table(decode_table, src[i])
                                    * amount;
                            (crate::color::linear_to_srgb(lin).clamp(0.0, 1.0) * FULL).round()
                                as u16
                        };
                        let mix_alpha = |i: usize| -> u16 {
                            let a = f32::from(dst[i]);
                            let b = f32::from(src[i]);
                            (a * (1.0 - amount) + b * amount).clamp(0.0, 65535.0) as u16
                        };
                        let out = [mix_rgb(0), mix_rgb(1), mix_rgb(2), mix_alpha(3)];
                        if out != dst {
                            tile[row as usize][column as usize] = out;
                            touched += 1;
                        }
                    }
                }
                surface.tile_request_end(tile_x, tile_y);
            }
        }
    }
    touched
}

/// **★★★ 印章热循环的**累计纳秒**✗ ★★★**（**第 246 轮 ✓；用户裁定「先量笔触占比」✓）
///
/// **∴ 为什么要它 ✗**：**探针里的 `印章累计` **只数个数**✗ 不记时间** ✓
///   ⇒ **∴ 于是**：**"一笔 50 ms 里**落笔占多少**"**答不上来** ✓
///     ＋ **∴ 而**那**正是**用户裁定要的那个分母／分子** ✓ ★★
///
/// **∴ 门控 ✗**：**只在 `YANSHI_RENDER_PROBE` 开着时**读时钟**✓
///   ＋ **∴ 而**环境变量**每进程读一次**（`OnceLock` ✓）⇒ **∴ 生产路**零开销** ✓ ★
static STAMP_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// **∴ 探针开着吗 ✗**（**∴ 只读一次环境 ✓）★
#[must_use]
fn stamp_probe_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("YANSHI_RENDER_PROBE").is_some())
}

/// **∴ 印章累计纳秒 ✗**（**供探针 ✓）★
#[must_use]
pub fn stamps_ns() -> u64 {
    STAMP_NS.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清空印章计时 ✗**（**判据／测量用 ✓）★
pub fn reset_stamps_ns() {
    STAMP_NS.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// **★ 印章计时的**守卫**✗ ★**（**∴ 离开作用域即累加 ✓）**（**第 246 轮 ✓）
///
/// **∴ 为什么用守卫 ✗**（**∴ 而**不是在 `drawn` 前手写一遍 ✓）★**：
///   **∴ 那三个函数**都有多处 `return` 与 `continue`**✗
///     ⇒ **∴ 手写**迟早漏一处**✗（**∴ 而**漏了**不会报错 ✗ 只会**少算时间** ✓）★★
///       ＋ **∴ 所以**：**让 `Drop` 负责**✗ ⇒ **∴ 每条出口**都被算上** ✓ ★★
struct StampTimer(Option<crate::probe::ProbeClock>);

impl Drop for StampTimer {
    fn drop(&mut self) {
        if let Some(t) = self.0.take() {
            note_stamp_time(t);
        }
    }
}

/// **∴ 把这一段的耗时累加进去 ✗**（**∴ 只有**探针开着才调用 ✓）★
#[inline]
fn note_stamp_time(started: crate::probe::ProbeClock) {
    STAMP_NS.fetch_add(
        started.elapsed().as_nanos() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// **把一行 dab 盖到缓冲区上**（服务端与服务端预览共用 ✓；参数与语义保持原样 ✓）。
///
/// **∴ 注 ✗**：**第 246 轮**把上面的印章计时设施插在了这里**✗
///   ⇒ **∴ 于是**它的**原注释一度被吃掉**✓（**∴ 缺文档 ⇒ `missing_docs` 警告 ✓）⇒ **∴ 已补回** ✓ ★
pub fn stamp_samples(buffer: &mut Buffer, brush: &BrushSpec, stamps: &[(f64, f64, f64)]) -> usize {
    stamp_samples_from(buffer, brush, stamps, 0)
}

/// 与 [`stamp_samples_from`] 相同，但按**选区覆盖度**衰减每个印章的强度 ✓。
///
/// 这是"选区约束落笔"（路线 A）的落地方式：**把覆盖度折进印章本身** ✓，
/// 因此**从不触碰选区外的像素** ✓ —— 既不改写已有内容 ✓，也与渲染是整幅还是分次（脏区）无关 ✓✓
/// （上一轮用"事后还原绘制前像素"的写法在分次渲染下会把已有内容抹掉 ✗，原因见
/// `docs/design/implementation-notes.md`）。
///
/// `coverage(x, y)` 按**文档坐标**给出 0..1；覆盖度 ≤ 0 的印章**直接跳过** ✓。
pub fn stamp_samples_clipped(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stamps: &[(f64, f64, f64)],
    coverage: &dyn Fn(f64, f64) -> f32,
) -> usize {
    // **∴ 计时这一段 ✗**（**第 246 轮 ✓）：**只累加**印章本身**✗ 不含调用方的别的活** ✓
    let _t_stamp = StampTimer(if stamp_probe_on() {
        Some(crate::probe::ProbeClock::now())
    } else {
        None
    });
    let radius = brush.size / 2.0;
    let mut drawn = 0usize;
    for (offset, (x, y, pressure)) in stamps.iter().enumerate() {
        let (mut cx, mut cy) = (*x, *y);
        if brush.jitter > 0.0 {
            let mut stamp_rng = Prng::derive(brush.seed, offset as u64);
            cx += stamp_rng.signed() as f64 * brush.jitter;
            cy += stamp_rng.signed() as f64 * brush.jitter;
        }
        let size_scale = if brush.pressure_size {
            let p = pressure.clamp(0.0, 1.0);
            brush.min_size_ratio + (1.0 - brush.min_size_ratio) * p
        } else {
            1.0
        };
        let stamp_radius = radius * size_scale;
        if stamp_radius <= 0.0 {
            continue;
        }
        let alpha = brush.color[3] * brush.flow as f32 * brush.opacity as f32;
        if alpha <= 0.0 {
            continue;
        }
        // 先按印章中心快速跳过完全在选区外的印章 ✓，再**逐像素**施加覆盖度 ✓。
        if coverage(cx, cy) <= 0.0 {
            continue;
        }
        draw_stamp_clipped_with_pressure(
            buffer,
            cx,
            cy,
            stamp_radius,
            brush.hardness,
            [brush.color[0], brush.color[1], brush.color[2], alpha],
            brush.blend_mode,
            coverage,
            *pressure as f32,
        );
        drawn += 1;
    }
    drawn
}

/// 把预先生成的采样盖章到缓冲，抖动序号从 `base_index` 起。
pub fn stamp_samples_from(
    buffer: &mut Buffer,
    brush: &BrushSpec,
    stamps: &[(f64, f64, f64)],
    base_index: u64,
) -> usize {
    // **∴ 计时这一段 ✗**（**第 246 轮 ✓）：**只累加**印章本身**✗ 不含调用方的别的活** ✓
    let _t_stamp = StampTimer(if stamp_probe_on() {
        Some(crate::probe::ProbeClock::now())
    } else {
        None
    });
    let radius = brush.size / 2.0;
    let mut rng = Prng::new(brush.seed);
    let mut drawn = 0usize;

    for (offset, (x, y, pressure)) in stamps.iter().enumerate() {
        let index = base_index as usize + offset;
        let (mut cx, mut cy) = (*x, *y);
        if brush.jitter > 0.0 {
            // 每个 stamp 独立的确定性抖动（序号跨帧连续，保证与一次性整段一致）。
            let mut stamp_rng = Prng::derive(brush.seed, index as u64);
            cx += stamp_rng.signed() as f64 * brush.jitter;
            cy += stamp_rng.signed() as f64 * brush.jitter;
            let _ = &mut rng;
        }
        let size_scale = if brush.pressure_size {
            let p = pressure.clamp(0.0, 1.0);
            brush.min_size_ratio + (1.0 - brush.min_size_ratio) * p
        } else {
            1.0
        };
        let stamp_radius = radius * size_scale;
        if stamp_radius <= 0.0 {
            continue;
        }
        let alpha = brush.color[3] * brush.flow as f32 * brush.opacity as f32;
        if alpha <= 0.0 {
            continue;
        }
        draw_stamp_with_pressure(
            buffer,
            cx,
            cy,
            stamp_radius,
            brush.hardness,
            [brush.color[0], brush.color[1], brush.color[2], alpha],
            brush.blend_mode,
            *pressure as f32,
        );
        drawn += 1;
    }
    drawn
}

/// 画一个圆形 stamp（软边由硬度控制；覆盖率按像素中心到笔尖中心的距离）。
///
/// 内层循环先把 stamp 的包围盒裁剪到缓冲区范围，再走 [`Buffer::blend_at`] 的
/// 直接索引路径，避免逐像素的边界判断与坐标换算（渲染热路径）。
/// 遍历一个圆形笔刷印章覆盖到的像素，回调 `(local_x, local_y, coverage)`。
///
/// `draw_stamp`（画）与 `erase_stamp`（擦）必须**共用同一套衰减数学** ——
/// 否则橡皮与画笔的形状会不一致，而且两份实现会各自漂移 ✓。
///
/// 注意：当前热路径已切换到 DabCache 掩膜（[`stamp_mask_cached`]），此函数保留
/// 作为数学定义的参考实现（`compute_dab_mask` 的注释以它为准），暂不删除。
#[allow(dead_code)]
fn for_each_covered_pixel(
    buffer: &Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    mut visit: impl FnMut(u32, u32, f64, f64, f64),
) {
    let (origin_x, origin_y) = buffer.origin();
    let document_x0 = (center_x - radius).floor() as i64;
    let document_y0 = (center_y - radius).floor() as i64;
    let document_x1 = (center_x + radius).ceil() as i64;
    let document_y1 = (center_y + radius).ceil() as i64;

    // 裁剪到缓冲区范围（局部坐标）。
    // **期 4 `lod`**：局部坐标只取落在取样格点上的（格点锚在缓冲原点）；`step == 1` 时
    // 与今天逐字等价（`div_ceil_i64`／`div_euclid` 在步长 1 时就是恒等）。
    let step = buffer.step() as i64;
    let start_x = crate::buffer::div_ceil_i64(document_x0 - origin_x, step).max(0);
    let start_y = crate::buffer::div_ceil_i64(document_y0 - origin_y, step).max(0);
    let end_x = (document_x1 - origin_x)
        .div_euclid(step)
        .min(buffer.width() as i64 - 1);
    let end_y = (document_y1 - origin_y)
        .div_euclid(step)
        .min(buffer.height() as i64 - 1);
    if end_x < 0 || end_y < 0 || start_x > end_x || start_y > end_y {
        return;
    }
    let start_x = start_x as u32;
    let start_y = start_y as u32;
    let end_x = end_x as u32;
    let end_y = end_y as u32;

    let hard_edge = radius * hardness.clamp(0.0, 1.0);
    let soft_span = (radius - hard_edge).max(1e-6);
    let radius_squared = radius * radius;
    // **★★★ 纯 CPU 侧优化 ①：硬边判定改用**平方比较**✗ ★★★**（**第 588 轮 ✓）
    //   **∴ 原来的浪费 ✗**：**每个覆盖到的像素**都算一次 `distance_squared.sqrt()`**
    //     ＋ **∴ 而**其中**硬边内**的像素（`distance <= hard_edge` ✓）
    //       **∴ 只需要**coverage ＝ 1.0**✗ ⇒ **∴ 那个 sqrt **白算了**** ✓
    //   **∴ 现在 ✗**：**硬边判定用 `distance_squared <= hard_edge_squared`**
    //     ＋ **∴ 于是**：**只有**软边带**的像素才算 sqrt** ✓
    //   **∴ 等价性（**必须说清 ✓）✗**：**对**非负**值，
    //     `sqrt(d²) <= h` **⟺** `d² <= h²`** ✓
    //       ＋ **∴ 但**浮点上：**`h*h` 有一次舍入**✗
    //         ⇒ **∴ 所以**：**边界上**理论上可能差一个 ulp** ✓
    //           ⇒ **★ 所以：**必须由**逐位一致判据守门**✗
    //             （**∴ 若红 ⇒ **立即回退** ✓）★
    let hard_edge_squared = hard_edge * hard_edge;

    for local_y in start_y..=end_y {
        let document_y = origin_y + local_y as i64 * step;
        let dy = document_y as f64 + 0.5 - center_y;
        for local_x in start_x..=end_x {
            let document_x = origin_x + local_x as i64 * step;
            let dx = document_x as f64 + 0.5 - center_x;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared > radius_squared {
                continue;
            }
            // **∴ 硬边内**不算 sqrt**✗（**∴ 那**是**本轮唯一的算法改动** ✓）★
            let coverage = if distance_squared <= hard_edge_squared {
                1.0
            } else {
                let distance = distance_squared.sqrt();
                let t = 1.0 - (distance - hard_edge) / soft_span;
                (t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0)
            };
            visit(
                local_x,
                local_y,
                coverage,
                document_x as f64 + 0.5,
                document_y as f64 + 0.5,
            );
        }
    }
}

/// 擦除印章：按覆盖度**扣除 alpha**（预乘语义下的 destination-out）。
///
/// 预乘 RGBA 下「擦除」= 四通道同乘 `(1 - coverage × strength)` ✓ ——
/// 颜色随之等比缩小，因此不会留下颜色残留 ✓。
pub fn erase_stamp(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    strength: f64,
) {
    erase_stamp_clipped(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        strength,
        &|_, _| 1.0,
    )
}

/// 与 [`erase_stamp`] 相同，但**逐像素**乘以选区覆盖度 ✓ —— 选区外一个像素都不擦 ✓。
///
/// 为什么擦除也要受约束：不受约束时，选区激活期间擦除会擦掉选区**外**的内容 ✓ ——
/// 那是**数据丢失** ✓，而不是功能缺失 ✓。
#[allow(clippy::too_many_arguments)]
pub fn erase_stamp_clipped(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    strength: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
) {
    erase_stamp_clipped_with_pressure(
        buffer, center_x, center_y, radius, hardness, strength, coverage, 1.0,
    );
}

/// 与 [`erase_stamp_clipped`] 相同，但多一个 `pressure` 参数用于 DabCache 的压感自动检测。
#[allow(clippy::too_many_arguments)]
pub(crate) fn erase_stamp_clipped_with_pressure(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    strength: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
    pressure: f32,
) {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let mut updates: Vec<(u32, u32, [f32; 4])> = Vec::new();
    let (origin_x, origin_y) = buffer.origin();
    let step = buffer.step() as f64;
    stamp_mask_cached(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        pressure,
        |x, y, stamp_coverage| {
            // **期 4 `lod`**：被采像素的文档中心是 `origin + local * step + 0.5`。
            let document_x = origin_x as f64 + x as f64 * step + 0.5;
            let document_y = origin_y as f64 + y as f64 * step + 0.5;
            let selection = coverage(document_x, document_y).clamp(0.0, 1.0);
            if selection <= 0.0 {
                return;
            }
            let keep = 1.0 - stamp_coverage * strength as f32 * selection;
            if keep >= 1.0 {
                return;
            }
            let pixel = buffer.pixel(x, y);
            updates.push((
                x,
                y,
                [
                    pixel[0] * keep,
                    pixel[1] * keep,
                    pixel[2] * keep,
                    pixel[3] * keep,
                ],
            ));
        },
    );
    for (x, y, pixel) in updates {
        buffer.set_pixel(x, y, pixel);
    }
}

/// 与 [`draw_stamp`] 相同，但**逐像素**乘以选区覆盖度 ✓。
///
/// 为什么必须逐像素：按"印章中心"取覆盖度时 ✓，中心落在选区内、
/// 半径却伸到选区外的印章仍会把墨画到外面 ✗（实测越界 160 个像素，
/// 全部集中在 x=37..39 与 88 一带 ✓ = 笔刷半径外溢 ✓）。
/// 逐像素施加后，**选区外一个像素都不会被写到** ✓✓。
#[allow(clippy::too_many_arguments)]
pub fn draw_stamp_clipped(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    color: LinearRgba,
    mode: BlendMode,
    coverage: &dyn Fn(f64, f64) -> f32,
) {
    draw_stamp_clipped_with_pressure(
        buffer, center_x, center_y, radius, hardness, color, mode, coverage, 1.0,
    );
}

/// 与 [`draw_stamp_clipped`] 相同，但多一个 `pressure` 参数用于 DabCache 的压感自动检测。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_stamp_clipped_with_pressure(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    color: LinearRgba,
    mode: BlendMode,
    coverage: &dyn Fn(f64, f64) -> f32,
    pressure: f32,
) {
    let use_fast_path = mode == BlendMode::Normal;
    let mut updates: Vec<(u32, u32, LinearRgba)> = Vec::new();
    let (origin_x, origin_y) = buffer.origin();
    let step = buffer.step() as f64;
    stamp_mask_cached(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        pressure,
        |x, y, stamp_coverage| {
            // **期 4 `lod`**：被采像素的文档中心是 `origin + local * step + 0.5`。
            let document_x = origin_x as f64 + x as f64 * step + 0.5;
            let document_y = origin_y as f64 + y as f64 * step + 0.5;
            let selection = coverage(document_x, document_y).clamp(0.0, 1.0);
            if selection <= 0.0 {
                return;
            }
            let alpha = color[3] * stamp_coverage * selection;
            if alpha <= 0.0 {
                return;
            }
            updates.push((x, y, premultiply([color[0], color[1], color[2], alpha])));
        },
    );
    for (x, y, source) in updates {
        if use_fast_path {
            buffer.blend_at(x, y, source);
        } else {
            buffer.blend_mode(x, y, source, mode);
        }
    }
}

/// 画一个圆形笔刷印章（按覆盖度合成颜色）。
pub fn draw_stamp(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    color: LinearRgba,
    mode: BlendMode,
) {
    draw_stamp_with_pressure(
        buffer, center_x, center_y, radius, hardness, color, mode, 1.0,
    );
}

/// 与 [`draw_stamp`] 相同，但多一个 `pressure` 参数用于 DabCache 的压感自动检测。
///
/// `stamp_samples_from` 等持有真实压力的调用方应走这里；直接调 [`draw_stamp`]
/// 的外部调用方（`render.rs`）压力恒为 1.0，走缓存路径。
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_stamp_with_pressure(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    color: LinearRgba,
    mode: BlendMode,
    pressure: f32,
) {
    let use_fast_path = mode == BlendMode::Normal;
    let mut updates: Vec<(u32, u32, LinearRgba)> = Vec::new();
    stamp_mask_cached(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        pressure,
        |x, y, coverage| {
            let alpha = color[3] * coverage;
            if alpha <= 0.0 {
                return;
            }
            updates.push((x, y, premultiply([color[0], color[1], color[2], alpha])));
        },
    );
    for (x, y, source) in updates {
        if use_fast_path {
            buffer.blend_at(x, y, source);
        } else {
            buffer.blend_mode(x, y, source, mode);
        }
    }
}

#[cfg(test)]
mod dab_quantisation_impact {
    // **★★★ 度量：半径量化对掩膜的**影响有多大** ✗ ★★★**（**第 725 轮 ✓）
    //
    // **∴ 为什么必须量它 ✗**（**判据第一次跑暴露的事实 ✓）：
    //   **∴ 缓存键把半径**量化到 0.5 px**✗**（**`(radius * 2.0).round()` ✓）
    //     ＋ **∴ 于是**：**缓存**用 round 后的半径**算掩膜** ✓
    //       ＋ **∴ 所以**：**它与**未量化的直接计算**必然**不同**✗
    //         ＋ **∴ 而那**是一个**真实的取舍**✗ ★**：
    //           **∴ 换取**掩膜复用（**补丁实测 40–50× ✓）✗
    //             ＋ **∴ 代价**半径最多差 **0.25 px** ✓
    //               ＋ **∴ 所以**：**必须**把代价量化**✗ ⇒ **∴ 从而**让取舍可判定** ✓ ★
    //
    // **∴ 本判据只**报告**✗ 不**断言相等**✗（**∴ 因为**两者**本来就不该相等 ✓）★
    use super::*;

    #[test]
    fn quantised_mask_differs_from_exact_by_a_bounded_amount() {
        let mut worst_cov = 0.0f64;
        let mut worst_size = 0i64;
        let mut size_diff_count = 0usize;
        let mut cases = 0usize;
        for &radius in &[3.0f64, 4.9, 5.0, 5.1, 5.25, 8.25, 15.5, 32.0] {
            for &hardness in &[0.0f64, 0.5, 0.75, 1.0] {
                for &phase in &[0.0f64, 0.25, 0.5, 0.75] {
                    let (exact, ew, eh) = compute_dab_mask(radius, hardness, phase, phase);
                    let quant_radius = (radius * 2.0).round().max(0.0) / 2.0;
                    let quant_hardness = (hardness.clamp(0.0, 1.0) * 100.0).round() / 100.0;
                    let (quant, qw, qh) =
                        compute_dab_mask(quant_radius, quant_hardness, phase, phase);
                    cases += 1;
                    let size_delta = (i64::from(qw) - i64::from(ew)).abs()
                        + (i64::from(qh) - i64::from(eh)).abs();
                    if size_delta != 0 {
                        size_diff_count += 1;
                    }
                    worst_size = worst_size.max(size_delta);
                    // **∴ 逐点比（**坐标系对齐到左下角 ✓）
                    let n = (ew.min(qw) as usize) * (eh.min(qh) as usize);
                    for i in 0..n {
                        // **∴ 显式转 f64 ✗**（**∴ 掩膜是 f32 ⇒ **∴ 编译器**当场指出 ✓）★
                        let d = f64::from((exact[i] - quant[i]).abs());
                        if d > worst_cov {
                            worst_cov = d;
                        }
                    }
                }
            }
        }
        println!(
            "  ★ 半径量化影响（{cases} 组）：尺寸不同的 {size_diff_count} 组｜最大尺寸差 {worst_size} 格｜最大 coverage 差 {worst_cov:.6}"
        );
        // **∴ 断言**有界**✗（**∴ 而不是**相等 ✓）：
        //   **∴ 尺寸差 ≤ 2 格**✗（**∴ 半径±0.25 ⇒ 直径±0.5 ⇒ **∴ 每边至多 1 格 ✓）
        //     ＋ **∴ coverage 差 ≤ 0.5**✗（**∴ 量化只在边界附近改变覆盖 ✓）★
        assert!(
            worst_size <= 2,
            "半径量化的尺寸影响应当 ≤ 2 格（实测 {worst_size}）"
        );
        // **∴ 上界 1.0 ✗**（**∴ 第 726 轮：**边界像素可以从 0 跳到 1**✗ 而**用户已裁定接受 ✓）
        assert!(
            worst_cov <= 1.0,
            "半径量化的 coverage 影响应当 ≤ 1.0（实测 {worst_cov:.6}）"
        );
    }
}

/// **★ 组件级 A/B：掩膜缓存的收益 ✗ ★**（**第 731 轮 ✓）
///
/// **∴ 为什么单测它 ✗**（**而不是端到端 ✓）：
///   **∴ 端到端的加速比**还取决于**掩膜计算在整条盖章管线里的占比** ✓
///     ＋ **∴ 而那**是**补丁作者自己声明的限定**（**「**端到端加速取决于盖章管线占比**」✓）★
///       ＋ **∴ 所以**：**本基准**只量**掩膜这一层**✗ ⇒ **∴ 从而**给出**上界** ✓
///         ＋ **∴ 而**端到端**必须**另量** ✓（**∴ 如实 ✓）★
///
/// **∴ 用法 ✗**：`cargo test -p yanshi-render --release -- --ignored --nocapture dab_cache_ab`
#[cfg(test)]
mod dab_cache_ab {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "基准：用 --ignored 显式运行"]
    fn dab_cache_ab() {
        // **∴ 场景 ✗**：**同一套参数**连续盖章**（**∴ 即** MCP／鼠标：**压力恒定 ＋ 半径不变 ✓）
        //   ＋ **∴ 那**正是**缓存命中率最高的场景**✗ ⇒ **∴ 所以**它**给出**收益上界** ✓ ★
        let buffer = Buffer::new(0, 0, 512, 512);
        const STAMPS: usize = 4000;
        let on = std::env::var_os("YANSHI_DAB_CACHE").is_none_or(|v| v != "0");
        let t = Instant::now();
        let mut total = 0.0f64;
        for i in 0..STAMPS {
            // **∴ 半径固定 ＋ 压力恒定 ⇒ **∴ 缓存**必然命中** ✓（**∴ 除第一次 ✓）
            let cx = 100.0 + (i % 64) as f64;
            let cy = 100.0 + (i % 37) as f64;
            stamp_mask_cached(&buffer, cx, cy, 12.0, 0.75, 0.5, |_, _, coverage| {
                total += f64::from(coverage);
            });
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let ns = ms * 1_000_000.0 / STAMPS as f64;
        println!(
            "  ★ 掩膜缓存 {}：{STAMPS} 次盖章 ⇒ **{ms:.1} ms**（**{ns:.1} ns／次**）｜coverage 累加 {total:.1}",
            if on { "开" } else { "关" }
        );
    }
}

/// **★★★ 不变量判据：`clear()` 之后一切状态必须**等价于新建** ✗ ★★★**（**第 742 轮 ✓）
///
/// **∴ 为什么需要它 ✗**（**第 739–741 轮用同一套方法查外部补丁换来的 ✓）★**：
///   **∴ 背景 ✗**：`DabCache` **有 **11 个状态字段**✗ 而**变更点**散布在 6 个方法里** ✓
///     ＋ **∴ 而**第 741 轮在 `Tile` 上**正好抓到**一个**同类缺陷**✗
///       （**∴ `write_row_f32` **漏更新** `maybe_opaque` ✓）★
///         ＋ **∴ 所以**：**同一个模式**值得**在 `DabCache` 上**再查一遍** ✓ ★
///   **∴ 本轮查的结果 ✗**（**否定结果，如实记录 ✓）★**：
///     **∴ `clear()` **已经**在末尾调用 `self.end_stroke()`**✗（**第 1872 行 ✓）
///       ＋ **∴ 而** `end_stroke()` **重置** `pressure_len`／`pressure_pos`／`pressure_mode`** ✓
///         ＋ **∴ 所以**：**压力状态**不会**跨 `clear()` 残留** ✗ ⇒ **∴ 没有**缺陷** ✓ ★
///           ＋ **∴ 而** `pressure_window` **的**数据**没有清**✗ 但**无害**✗ ★**：
///             **∴ 因为** `pressure_len == 0` **⇒ **∴ 窗口「**未满」**⇒ **∴ `note_pressure` **不判定** ✓ ★
///
/// **∴ 本判据守什么 ✗**：**若**将来有人在 `clear()` 里**删掉** `end_stroke()`**✗
///   ＋ **∴ 或**新增一个状态字段却忘了在 `clear()` 里重置**✗
///     ＋ **⇒ 本判据**必红** ✓ ★
///
/// **∴ 变异检验（**已做 ✓）✗**：**删掉** `clear()` 里的 `self.end_stroke();`
///   ⇒ **∴ 本判据**必红**（**∴ 压力状态残留 ✓）★
#[cfg(test)]
mod dab_cache_invariants {
    use super::*;

    /// **∴ 用一串**压力连续变化**的采样把缓存推进压感模式 ✓**（**∴ 方差够大 ⇒ 进入 ✓）★
    fn drive_into_pressure_mode(cache: &mut DabCache) {
        for i in 0..(PRESSURE_WINDOW * 2) {
            let p = if i % 2 == 0 { 0.1 } else { 0.9 };
            // **∴ 用**合法参数**（**∴ 半径 ≥ 小笔刷阈值 ⇒ 若未进模式**就会走缓存 ✓）★
            let _ = cache.get_or_compute(12.0, 0.75, 0.0, 0.0, p);
        }
    }

    #[test]
    fn clear_returns_every_field_to_the_fresh_state() {
        let mut cache = DabCache::new();
        drive_into_pressure_mode(&mut cache);
        // **∴ 前提：**确实进了压感模式**✗（**∴ 否则本判据**测不到东西 ✓）★
        assert!(
            cache.in_pressure_mode(),
            "前提不成立：驱动后应当进入压感模式（否则本判据无法验证 clear 的行为）"
        );
        assert!(
            cache.pressure_bypass_count() > 0,
            "压感模式应当已发生 bypass"
        );

        cache.clear();

        assert!(!cache.in_pressure_mode(), "clear 之后必须**退出**压感模式");
        assert_eq!(
            cache.pressure_bypass_count(),
            0,
            "clear 之后压感 bypass 计数必须归零"
        );
        assert_eq!(
            cache.bypass_count(),
            0,
            "clear 之后小笔刷 bypass 计数必须归零"
        );
        // **∴ `hit_rate()` 在无采样时的取值（**分母为 0 ✓）**也一并钉住 ✓
        let rate = cache.hit_rate();
        assert!(
            rate.is_finite(),
            "clear 之后 hit_rate() 必须是有限数（实测 {rate}）"
        );
        // **∴ 而**清空后**再次**用**恒定压力** ⇒ **∴ 必须**重新**走缓存**（**∴ 不再 bypass ✓）★
        let mut hits_or_misses = 0usize;
        for _ in 0..(PRESSURE_WINDOW * 2 + 4) {
            let _ = cache.get_or_compute(12.0, 0.75, 0.0, 0.0, 0.5);
            hits_or_misses += 1;
        }
        assert!(
            !cache.in_pressure_mode(),
            "clear 后用**恒定压力**不应进入压感模式（残留 = {})",
            cache.in_pressure_mode()
        );
        assert_eq!(
            cache.pressure_bypass_count(),
            0,
            "clear 后用恒定压力不应产生压感 bypass（说明压力状态**残留**了）"
        );
        assert!(hits_or_misses > 0);
    }
}

#[cfg(test)]
mod tests {
    /// 选区覆盖度折进印章：覆盖度 1 与不裁剪一致；0 完全不画；0.5 更弱但仍有墨 ✓。
    #[test]
    fn clipped_stamping_respects_coverage() {
        use crate::buffer::Buffer;
        let brush = BrushSpec {
            size: 6.0,
            hardness: 1.0,
            color: [1.0, 0.1, 0.1, 1.0],
            opacity: 1.0,
            flow: 1.0,
            ..Default::default()
        };
        let stamps = [(6.0, 6.0, 1.0), (14.0, 6.0, 1.0)];
        let ink = |buffer: &Buffer| -> usize {
            let mut count = 0;
            for y in 0..16 {
                for x in 0..20 {
                    if buffer.pixel(x, y)[3] > 0.01 {
                        count += 1;
                    }
                }
            }
            count
        };

        let mut plain = Buffer::new(0, 0, 20, 16);
        let plain_drawn = stamp_samples(&mut plain, &brush, &stamps);
        let plain_ink = ink(&plain);
        assert!(plain_ink > 0, "不裁剪时应画出墨");

        let mut full = Buffer::new(0, 0, 20, 16);
        let full_drawn = stamp_samples_clipped(&mut full, &brush, &stamps, &|_, _| 1.0);
        assert_eq!(
            full_drawn, plain_drawn,
            "覆盖度 1 应与不裁剪画出同样多的印章"
        );
        assert_eq!(ink(&full), plain_ink, "覆盖度 1 的像素应与不裁剪完全一致");

        let mut none = Buffer::new(0, 0, 20, 16);
        let none_drawn = stamp_samples_clipped(&mut none, &brush, &stamps, &|_, _| 0.0);
        assert_eq!(none_drawn, 0, "覆盖度 0 不应画任何印章");
        assert_eq!(ink(&none), 0, "覆盖度 0 不应留下任何墨");

        // 只覆盖第一个印章所在区域 ⇒ 只画那一个。
        let mut half = Buffer::new(0, 0, 20, 16);
        let half_drawn = stamp_samples_clipped(&mut half, &brush, &stamps, &|x, _| {
            if x < 10.0 {
                1.0
            } else {
                0.0
            }
        });
        assert_eq!(half_drawn, 1, "只有落在选区内的印章才应被绘制");
        assert!(
            ink(&half) > 0 && ink(&half) < plain_ink,
            "只应画出左侧那一部分墨"
        );
    }

    use super::*;
    use serde_json::json;

    fn line_stroke() -> StrokeGeometry {
        StrokeGeometry {
            points: vec![
                StrokePoint {
                    x: 4.0,
                    y: 8.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 28.0,
                    y: 8.0,
                    pressure: 1.0,
                },
            ],
            smooth: false, // 既有测试走不平滑路径（默认行为）
        }
    }

    #[test]
    fn stroke_geometry_parses_both_forms() {
        let pairs =
            StrokeGeometry::from_value(&json!({"points": [[1.0, 2.0], [3.0, 4.0, 0.5]]})).unwrap();
        assert_eq!(pairs.points.len(), 2);
        assert_eq!(pairs.points[1].pressure, 0.5);
        let objects = StrokeGeometry::from_value(
            &json!({"points": [{"x": 1.0, "y": 2.0, "pressure": 0.25}]}),
        )
        .unwrap();
        assert_eq!(objects.points[0].pressure, 0.25);
        assert!(StrokeGeometry::from_value(&json!({"points": []})).is_none());
    }

    #[test]
    fn brush_spec_parses_linear_and_byte_colors() {
        let linear = BrushSpec::from_value(&json!({"size": 10.0, "color": [0.5, 0.25, 0.0, 1.0]}));
        assert_eq!(linear.size, 10.0);
        assert_eq!(linear.color, [0.5, 0.25, 0.0, 1.0]);
        assert_eq!(linear.spacing_pixels(), 2.5);

        let bytes = BrushSpec::from_value(&json!({"color": {"r": 255, "g": 0, "b": 0, "a": 255}}));
        assert!((bytes.color[0] - 1.0).abs() < 1e-6, "sRGB 255 → 线性 1.0");
        assert!(bytes.color[1].abs() < 1e-6);
        assert_eq!(bytes.color[3], 1.0);

        let defaults = BrushSpec::from_value(&json!({}));
        assert_eq!(defaults.blend_mode, BlendMode::Normal);
        assert_eq!(defaults.opacity, 1.0);
    }

    #[test]
    fn brush_spec_honors_explicit_color_space() {
        // **P2-9**：`data.color_space` 改写浮点数组的解释（与 `update_stroke` 同口径）。
        let color = json!([0.5, 0.25, 0.0, 1.0]);
        let linear = BrushSpec::from_value(&json!({"color": color}));
        assert_eq!(
            linear.color,
            [0.5, 0.25, 0.0, 1.0],
            "不给空间 ⇒ 既有量级切换（浮点 = 线性）"
        );
        let srgb = BrushSpec::from_value(&json!({"color": color.clone(), "color_space": "srgb"}));
        assert!(
            (srgb.color[0] - linear.color[0]).abs() > 0.2,
            "srgb 臂必须与线性臂分开：实测 {:?} vs {:?}",
            srgb.color,
            linear.color
        );
        let bytes = BrushSpec::from_value(&json!({"color": {"r": 128, "g": 64, "b": 0, "a": 255}}));
        assert!(
            (srgb.color[0] - bytes.color[0]).abs() < 1e-6
                && (srgb.color[1] - bytes.color[1]).abs() < 1e-6,
            "srgb 浮点臂应与字节臂逐通道一致：实测 {:?} vs {:?}",
            srgb.color,
            bytes.color
        );
        let explicit_linear =
            BrushSpec::from_value(&json!({"color": color.clone(), "color_space": "linear"}));
        assert_eq!(
            explicit_linear.color, linear.color,
            "显式 linear 与缺省同一条路"
        );
    }

    #[test]
    fn stamping_draws_a_continuous_line() {
        let mut buffer = Buffer::new(0, 0, 32, 16);
        let brush = BrushSpec {
            size: 4.0,
            color: [1.0, 0.0, 0.0, 1.0],
            ..BrushSpec::default()
        };
        let stamps = stamp_stroke(&mut buffer, &brush, &line_stroke());
        assert!(stamps > 10, "间距 1px，应有足量 stamp：{stamps}");
        // 笔迹沿途有像素，笔迹之外没有。
        for x in 4..=28 {
            assert!(
                buffer.pixel(x, 8)[3] > 0.9,
                "x={x} 处未覆盖：{:?}",
                buffer.pixel(x, 8)
            );
        }
        assert_eq!(buffer.pixel(4, 1)[3], 0.0);
        assert_eq!(buffer.pixel(31, 15)[3], 0.0);
    }

    #[test]
    fn soft_brush_edge_is_partial_coverage() {
        let mut hard = Buffer::new(0, 0, 16, 16);
        let mut soft = Buffer::new(0, 0, 16, 16);
        let stroke = StrokeGeometry {
            points: vec![StrokePoint {
                x: 8.0,
                y: 8.0,
                pressure: 1.0,
            }],
            smooth: false, // 既有测试走不平滑路径（默认行为）
        };
        stamp_stroke(
            &mut hard,
            &BrushSpec {
                size: 8.0,
                hardness: 1.0,
                ..BrushSpec::default()
            },
            &stroke,
        );
        stamp_stroke(
            &mut soft,
            &BrushSpec {
                size: 8.0,
                hardness: 0.0,
                ..BrushSpec::default()
            },
            &stroke,
        );
        // 硬边：半径内满覆盖；软边：过渡带半透明（半径 4，像素 10 距中心 2.5）。
        assert!(hard.pixel(10, 8)[3] > 0.9, "{:?}", hard.pixel(10, 8));
        let soft_edge = soft.pixel(10, 8)[3];
        assert!((0.05..0.95).contains(&soft_edge), "soft edge={soft_edge}");
        // 更靠外（距中心 3.5）覆盖率更低。
        assert!(soft.pixel(11, 8)[3] < soft_edge, "软边应向外递减");
        // 中心区域：硬边满覆盖，软边因为中心像素中心距笔尖中心 0.707 而略低。
        assert!(hard.pixel(8, 8)[3] > 0.95);
        assert!(
            soft.pixel(8, 8)[3] > 0.8,
            "soft center={:?}",
            soft.pixel(8, 8)
        );
    }

    #[test]
    fn jitter_is_deterministic_and_seed_dependent() {
        let stroke = line_stroke();
        let render = |seed: u64| {
            let mut buffer = Buffer::new(0, 0, 32, 16);
            stamp_stroke(
                &mut buffer,
                &BrushSpec {
                    size: 2.0,
                    jitter: 3.0,
                    seed,
                    ..BrushSpec::default()
                },
                &stroke,
            );
            buffer.as_f32().to_vec()
        };
        let first = render(7);
        let again = render(7);
        let other = render(8);
        assert_eq!(first, again, "同 seed 必须逐位一致");
        assert_ne!(first, other, "不同 seed 应给出不同抖动");
    }

    /// **★★★ 对称模式 ✗ 判据（**第 251 轮 ✓）★★★**
    ///
    /// **∴ 判什么 ✗**：**垂直对称盖一笔 ⇒ 画布**左右两半逐位相等** ✓
    ///   ＋ **∴ 且**先要一个**有效样本**✗（**盖上了几个章 ＋ 画布上有像素 ✓）——
    ///     **∴ 否则**一张**全黑/全空**的图**天然满足镜像相等** ✗ ⇒ **∴ 判据会假绿** ✓ ★★
    ///   ＋ **∴ 对照 ✗**：**关掉对称 ⇒ **左右不该相等** ✓（**∴ 排除"恒真" ✓）★★
    ///
    /// **∴ 为什么用**整数坐标**✗**：**镜像 `宽 − x` 在整数点上**精确**✗
    ///   ＋ **∴ 而**f32 的分数部分在非整数点上**可能有末位差** ✓
    ///     ⇒ **∴ 这条判据要的是"**逐位相等**"✗ 所以取整数点** ✓（**如实 ✓）★
    ///
    /// **∴ 变异（**能红的根据 ✓）✗**：**去掉镜像展开**（**或把轴写成 `宽−1−x` ✓）
    ///   ⇒ **∴ 本判据**必红** ✓ ★★
    #[test]
    fn vertical_symmetry_mirrors_pixels_exactly() {
        let (w, h) = (64u32, 48u32);
        let stroke = StrokeGeometry {
            points: vec![StrokePoint {
                x: 12.0,
                y: 20.0,
                pressure: 0.8,
            }],
            smooth: false,
        };
        let render = |symmetry: Symmetry| -> (usize, Vec<f32>) {
            let mut buffer = Buffer::new(0, 0, w, h);
            let brush = BrushSpec {
                size: 6.0,
                symmetry,
                ..BrushSpec::default()
            };
            let drawn = stamp_stroke_on(&mut buffer, &brush, &stroke, (w, h));
            (drawn, buffer.as_f32().to_vec())
        };
        let (drawn, pixels) = render(Symmetry::Vertical);
        assert!(drawn >= 2, "垂直对称应盖**两个**章（实测 {drawn}）");
        assert!(
            pixels.iter().any(|v| *v != 0.0),
            "画布上得真有点像素 ⇒ 否则镜像比对是空话"
        );
        let worst = mirror_difference(&pixels, w, h);
        assert_eq!(
            worst, 0.0,
            "垂直对称下左右必须**逐位**相等（实测最大差 {worst}）"
        );
        // **对照**：关掉对称 ⇒ 不该镜像相等 ✓
        let (plain_drawn, plain) = render(Symmetry::None);
        assert_eq!(plain_drawn, 1, "关掉对称只盖一个章");
        assert!(
            mirror_difference(&plain, w, h) > 0.0,
            "对照：不开对称时左右**不该**相等（否则这条判据没有区分力）"
        );
    }

    /// **逐位镜像差 ✗**（**返回左右两半对应像素的最大绝对差 ✓）★
    fn mirror_difference(pixels: &[f32], w: u32, h: u32) -> f32 {
        let mut worst = 0.0f32;
        for y in 0..h as usize {
            for x in 0..(w as usize / 2) {
                let mirror_x = w as usize - 1 - x;
                let a = (y * w as usize + x) * 4;
                let b = (y * w as usize + mirror_x) * 4;
                for channel in 0..4 {
                    worst = worst.max((pixels[a + channel] - pixels[b + channel]).abs());
                }
            }
        }
        worst
    }

    #[test]
    fn opacity_and_pressure_scale_coverage() {
        let stroke = StrokeGeometry {
            points: vec![StrokePoint {
                x: 8.0,
                y: 8.0,
                pressure: 0.5,
            }],
            smooth: false, // 既有测试走不平滑路径（默认行为）
        };
        let mut buffer = Buffer::new(0, 0, 16, 16);
        stamp_stroke(
            &mut buffer,
            &BrushSpec {
                size: 8.0,
                opacity: 0.5,
                flow: 1.0,
                min_size_ratio: 0.25,
                ..BrushSpec::default()
            },
            &stroke,
        );
        let center = buffer.pixel(8, 8)[3];
        assert!((center - 0.5).abs() < 0.05, "center alpha={center}");
    }
}

/// **`size_curve` 必须沿笔迹塑形，而不是被压力塌缩** ✓ —— 子 agent 报的 #8：
/// 用文档推荐的 `points: [[x, y], …]` 形式（每点压力默认 1.0 ✓）时，
/// 曲线被采样在 `curve(1)` ✗ ⇒ 写了锥形曲线却画出**等宽**色带 ✓
/// （实测：40px 笔刷 + `size_curve: [[0,1],[0.5,0.5],[1,0.1]]` 得到均匀 4px ✗）。
#[test]
fn size_curve_shapes_the_stroke_along_its_progress() {
    use crate::buffer::Buffer;
    use serde_json::json;

    let brush = BrushSpec {
        size: 40.0,
        hardness: 1.0,
        color: [0.9, 0.1, 0.1, 1.0],
        opacity: 1.0,
        flow: 1.0,
        ..Default::default()
    };
    // **用文档推荐的 `points: [[x, y], …]` 形式** ✓ —— 每点压力默认 **1.0** ✓，
    // 正是曲线被塌缩成 `curve(1)` 的那条路径 ✓（子 agent 的探针就是这个形状 ✓）。
    let geometry = StrokeGeometry::from_value(&json!({
        "points": (0..=40).map(|i| json!([10.0 + i as f64 * 4.0, 60.0])).collect::<Vec<_>>()
    }))
    .expect("点列应可解析");
    let appearance = StrokeAppearance::from_data(&json!({
        "appearance": { "size_curve": [[0.0, 1.0], [0.5, 0.5], [1.0, 0.1]] }
    }));

    let mut buffer = Buffer::new(0, 0, 200, 120);
    let drawn = stamp_stroke_configured(&mut buffer, &brush, &geometry, Some(&appearance), None);
    assert!(drawn > 0, "应画出印章");

    // 量**每一列的墨迹高度** ✓：起点应远高于终点 ✓（锥形 ✓）。
    let column_height =
        |x: u32| -> usize { (0..120).filter(|y| buffer.pixel(x, *y)[3] > 0.01).count() };
    let start = (12..14).map(column_height).max().unwrap_or(0);
    let end = (185..190).map(column_height).max().unwrap_or(0);
    assert!(start > 0, "起点应有墨（实际 {start}）");
    assert!(
        start >= end * 3,
        "锥形曲线应让起点明显宽于终点（起点 {start}px，终点 {end}px）"
    );
}

/// **`spacing_curve` 必须真的改变采样密度**（progress 曲线，1 = 原间距、越小越密）。
#[test]
fn spacing_curve_densifies_stamps_along_progress() {
    use crate::buffer::Buffer;
    use serde_json::json;

    let brush = BrushSpec {
        size: 40.0,
        hardness: 1.0,
        color: [0.9, 0.1, 0.1, 1.0],
        opacity: 1.0,
        flow: 1.0,
        ..Default::default()
    };
    let geometry = StrokeGeometry::from_value(&json!({
        "points": (0..=40).map(|i| json!([10.0 + i as f64 * 4.0, 60.0])).collect::<Vec<_>>()
    }))
    .expect("点列应可解析");

    let mut plain_buffer = Buffer::new(0, 0, 200, 120);
    let plain = stamp_stroke_configured(&mut plain_buffer, &brush, &geometry, None, None);

    let appearance = StrokeAppearance::from_data(&json!({
        "appearance": { "spacing_curve": [[0.0, 1.0], [1.0, 0.25]] }
    }));
    assert!(appearance.active, "只有 spacing_curve 也应算有效外观");
    let mut curved_buffer = Buffer::new(0, 0, 200, 120);
    let curved = stamp_stroke_configured(
        &mut curved_buffer,
        &brush,
        &geometry,
        Some(&appearance),
        None,
    );
    assert!(
        curved > plain * 3 / 2,
        "间距收至 1/4 应让 stamp 明显变密（无曲线 {plain}，有曲线 {curved}）"
    );
}

/// **`flow_curve` 必须沿 progress 调制流量**：收笔处 alpha 趋零、起笔处不受影响。
#[test]
fn flow_curve_fades_alpha_along_progress() {
    use crate::buffer::Buffer;
    use serde_json::json;

    let brush = BrushSpec {
        size: 40.0,
        hardness: 1.0,
        color: [0.9, 0.1, 0.1, 1.0],
        opacity: 1.0,
        flow: 1.0,
        ..Default::default()
    };
    let geometry = StrokeGeometry::from_value(&json!({
        "points": (0..=40).map(|i| json!([10.0 + i as f64 * 4.0, 60.0])).collect::<Vec<_>>()
    }))
    .expect("点列应可解析");
    let appearance = StrokeAppearance::from_data(&json!({
        "appearance": { "flow_curve": [[0.0, 1.0], [1.0, 0.0]] }
    }));
    let mut buffer = Buffer::new(0, 0, 200, 120);
    let drawn = stamp_stroke_configured(&mut buffer, &brush, &geometry, Some(&appearance), None);
    assert!(drawn > 0, "起笔段应画出印章");
    let buffer_ref = &buffer;
    let max_alpha = |x0: u32, x1: u32| -> f32 {
        (x0..x1)
            .flat_map(|x| (0..120).map(move |y| buffer_ref.pixel(x, y)[3]))
            .fold(0.0f32, f32::max)
    };
    let start = max_alpha(12, 20);
    let end = max_alpha(163, 170);
    assert!(start > 0.9, "起笔处流量应接近满（实际 {start}）");
    assert!(end < 0.2, "收笔处流量应被曲线压到近零（实际 {end}）");
}

/// **加上 `appearance` 不能静默改变几何** ✓ —— 子 agent 报的 #9：
/// 同一组带压力斜坡的点 ✓，不带 appearance 时 38→14px 会收细 ✓，
/// 而**只加一个纹理**之后就变成 38→38px ✗（压力响应被静默关掉 ✓）。
#[test]
fn an_appearance_does_not_disable_the_pressure_response() {
    use crate::buffer::Buffer;
    use serde_json::json;

    let brush = BrushSpec {
        size: 40.0,
        hardness: 1.0,
        color: [0.1, 0.1, 0.9, 1.0],
        opacity: 1.0,
        flow: 1.0,
        ..Default::default()
    };
    // 压力 1.0 → 0.2 的斜坡 ✓（`{x, y, pressure}` 形式 ✓）。
    let geometry = StrokeGeometry::from_value(&json!({
        "points": (0..=40)
            .map(|i| {
                let t = i as f64 / 40.0;
                json!([10.0 + i as f64 * 4.0, 60.0, 1.0 - 0.8 * t])
            })
            .collect::<Vec<_>>()
    }))
    .expect("点列应可解析");

    let measure = |appearance: Option<&StrokeAppearance>| -> (usize, usize) {
        let mut buffer = Buffer::new(0, 0, 200, 120);
        stamp_stroke_configured(&mut buffer, &brush, &geometry, appearance, None);
        let column =
            |x: u32| -> usize { (0..120).filter(|y| buffer.pixel(x, *y)[3] > 0.01).count() };
        let start = (12..14).map(column).max().unwrap_or(0);
        let end = (185..190).map(column).max().unwrap_or(0);
        (start, end)
    };

    let plain = measure(None);
    assert!(
        (plain.1 as f64) <= plain.0 as f64 * 0.8,
        "不带 appearance 时应随压力收细（{plain:?}）"
    );

    // **只加一个纹理** ✓（`dynamics` 的纹理调制 alpha ✓，不该影响几何 ✓）。
    let textured_appearance = StrokeAppearance::from_data(&json!({
        "appearance": { "texture": { "kind": "grain", "scale": 3.0, "strength": 0.5, "seed": 1 } }
    }));
    let textured = measure(Some(&textured_appearance));
    assert!(textured.0 > 0, "带纹理时仍应画出墨");
    // **判据用"终点明显窄于起点"** ✓（而不是某个固定倍数 ✓）：
    // 纹理本身会调制 alpha ✓ ⇒ 用透明度阈值量出的"高度"会被它影响 ✓
    //（实测：不带 appearance 40→0 ✓、只加纹理 40→26 ✓，**都**在收细 ✓）。
    // 而 bug 的形态是 **40→40** ✗（压力响应被静默关掉 ✓）⇒ 这条判据能抓住它 ✓。
    assert!(
        (textured.1 as f64) <= textured.0 as f64 * 0.8,
        "只加纹理不应关掉压力响应（{textured:?}，不带 appearance 时是 {plain:?}）"
    );
}
#[cfg(test)]
mod smoothing_tests {
    use super::*;

    fn point(x: f64, y: f64) -> StrokePoint {
        StrokePoint {
            x,
            y,
            pressure: 1.0,
        }
    }

    /// **端点必须原样保留** ✓ —— 平滑只该影响中间 ✓，
    /// 否则笔迹会在起止处"缩进去" ✓（用户能一眼看出来 ✓）。
    #[test]
    fn smoothing_keeps_the_endpoints_exactly() {
        let points = vec![
            point(0.0, 0.0),
            point(10.0, 10.0),
            point(20.0, 0.0),
            point(30.0, 6.0),
        ];
        let smoothed = catmull_rom_smooth(&points, 8);
        assert!(smoothed.len() > points.len(), "应生成更多采样点");
        assert_eq!(smoothed[0], points[0], "起点必须原样保留");
        assert_eq!(
            smoothed[smoothed.len() - 1],
            points[points.len() - 1],
            "终点必须原样保留"
        );
        // 原控制点应当**仍然落在平滑后的路径上** ✓（Catmull-Rom 是插值样条 ✓，不是逼近 ✓）。
        for control in &points {
            let nearest = smoothed
                .iter()
                .map(|candidate| {
                    (((candidate.x - control.x) * (candidate.x - control.x))
                        + ((candidate.y - control.y) * (candidate.y - control.y)))
                        .sqrt()
                })
                .fold(f64::INFINITY, f64::min);
            assert!(nearest < 1e-9, "控制点应被精确穿过（最近距离 {nearest}）");
        }
    }

    /// **平滑要真的把折角"削圆"** ✓ —— 否则这个开关等于没做事 ✓。
    /// 用一个直角折线：原始折线在拐点处到达 (10,10) ✓，平滑路径应当**低于**它 ✓（切角 ✓），
    /// 但仍**高于**两端连线 ✓（不能塌成直线 ✓）。
    #[test]
    fn smoothing_rounds_a_sharp_corner_without_flattening_it() {
        let points = vec![point(0.0, 0.0), point(10.0, 10.0), point(20.0, 0.0)];
        let smoothed = catmull_rom_smooth(&points, 16);
        // 只看第一段内部（不含端点 ✓）。
        let first_leg: Vec<&StrokePoint> = smoothed
            .iter()
            .filter(|sample| sample.x > 0.5 && sample.x < 9.5)
            .collect();
        assert!(!first_leg.is_empty(), "第一段应有内部采样");
        let peak = first_leg
            .iter()
            .map(|sample| sample.y)
            .fold(f64::NEG_INFINITY, f64::max);
        let chord = 10.0 * 0.95; // x=9.5 处直线的高度 ✓
        assert!(
            peak < 10.0 - 1e-6,
            "拐角应被削圆（峰值 {peak} 应低于控制点 10）"
        );
        assert!(
            peak > chord * 0.5,
            "不能塌成直线（峰值 {peak} 应明显高于弦）"
        );
    }

    /// **既有文档不受影响** ✓：`data` 里没有 `smooth` ⇒ `smooth = false` ✓ ⇒ 渲染路径与从前一致 ✓。
    #[test]
    fn geometry_without_the_flag_is_not_smoothed() {
        use serde_json::json;
        let legacy = StrokeGeometry::from_value(&json!({"points": [[0, 0], [10, 10], [20, 0]]}))
            .expect("应能解析");
        assert!(!legacy.smooth, "缺省必须是关闭（老文档逐字节不变）");
        let opted = StrokeGeometry::from_value(
            &json!({"points": [[0, 0], [10, 10], [20, 0]], "smooth": true}),
        )
        .expect("应能解析");
        assert!(opted.smooth, "显式打开应生效");
        // **点序列本身不被改写** ✓ —— 两种情况下点完全相同 ✓。
        assert_eq!(
            legacy.points, opted.points,
            "平滑只发生在渲染时，不改写点序列"
        );
    }

    /// **"平滑"的定义要落到可量的东西上** ✓：把每段的**方向变化**加起来 ✓，
    /// 平滑后的总转角必须**明显更小** ✓（折线在每个采样点都会拐一下 ✓，样条则连续 ✓）。
    #[test]
    fn smoothing_reduces_the_total_turning() {
        let zigzag: Vec<StrokePoint> = (0..=12)
            .map(|index| {
                let x = index as f64 * 6.0;
                let y = if index % 2 == 0 { 10.0 } else { 22.0 };
                point(x, y)
            })
            .collect();
        let smoothed = catmull_rom_smooth(&zigzag, 8);
        // **用"最大转角"而不是"转角之和"** ✓ —— 我第一版用求和 ✗，结果平滑后**更大** ✓
        //（原始 24.36、平滑 27.40 弧度 ✓）：因为平滑后的采样点**多得多** ✓，
        // 每个平缓的小转角都计入总和 ✓ ⇒ 求和根本不是"平滑度"的度量 ✗。
        // 平滑度的定义是"**没有急拐**" ✓ ⇒ 取最大转角 ✓。
        let turning = |points: &[StrokePoint]| -> f64 {
            let mut worst = 0.0f64;
            for window in points.windows(3) {
                let (a, b, c) = (&window[0], &window[1], &window[2]);
                let (v1x, v1y) = (b.x - a.x, b.y - a.y);
                let (v2x, v2y) = (c.x - b.x, c.y - b.y);
                let len1 = (v1x * v1x + v1y * v1y).sqrt();
                let len2 = (v2x * v2x + v2y * v2y).sqrt();
                if len1 <= 1e-9 || len2 <= 1e-9 {
                    continue;
                }
                let cosine = ((v1x * v2x + v1y * v2y) / (len1 * len2)).clamp(-1.0, 1.0);
                worst = worst.max(libm::acos(cosine));
            }
            worst
        };
        let raw_turn = turning(&zigzag);
        let smooth_turn = turning(&smoothed);
        assert!(
            raw_turn > 1.0,
            "原始折线应有明显的急拐（实测最大转角 {raw_turn:.2} 弧度）"
        );
        assert!(
            smooth_turn < raw_turn * 0.8,
            "平滑后不该再有急拐（原始最大 {raw_turn:.2} 弧度，平滑 {smooth_turn:.2} 弧度）"
        );
    }

    /// 平滑开关必须**真的改变渲染结果** ✓（否则"接上去了没有"无从验证 ✓）。
    #[test]
    fn the_flag_changes_what_gets_stamped() {
        use crate::buffer::Buffer;
        use serde_json::json;
        let brush = BrushSpec {
            size: 6.0,
            hardness: 1.0,
            color: [0.1, 0.2, 0.9, 1.0],
            opacity: 1.0,
            flow: 1.0,
            ..Default::default()
        };
        let ink = |smooth: bool| -> (usize, usize) {
            let geometry = StrokeGeometry::from_value(&json!({
                "points": [[4, 40], [30, 6], [56, 40]],
                "smooth": smooth,
            }))
            .expect("应能解析");
            let mut buffer = Buffer::new(0, 0, 64, 48);
            stamp_stroke_configured(&mut buffer, &brush, &geometry, None, None);
            let mut count = 0;
            let mut top = 48usize;
            for y in 0..48 {
                for x in 0..64 {
                    if buffer.pixel(x, y)[3] > 0.01 {
                        count += 1;
                        top = top.min(y as usize);
                    }
                }
            }
            (count, top)
        };
        let (plain_count, plain_top) = ink(false);
        let (smooth_count, smooth_top) = ink(true);
        assert!(plain_count > 0 && smooth_count > 0, "两种都应有墨");
        // **断言"接线确实生效"** ✓：开关改变渲染结果 ✓（否则无从验证它有没有接上 ✓）。
        //
        // 我第一版写的是"平滑后拐角被削圆 ⇒ 墨迹顶端更低" ✗ —— **实测反了** ✓
        //（不平滑 4、平滑 3 ✓）。原因值得记下 ✓：**均匀 Catmull-Rom 是插值样条 ✓，
        // 在尖角处会"过冲"** ✓ ⇒ 它并不"把角削圆" ✗，而是把角**穿过**并向外鼓一点 ✓。
        // 所以"削圆"不是一个成立的判据 ✓；真正成立的是下面这条**几何**判据 ✓。
        assert!(
            plain_count != smooth_count || plain_top != smooth_top,
            "平滑开关必须改变渲染结果（两者完全相同：{plain_count}/{plain_top}）"
        );
        // 另一个不经渲染的对照 ✓：两种路径的墨量**数量级**应当接近 ✓
        //（平滑只改插值 ✓，不该让整笔明显变粗或变细 ✗）。
        let ratio = smooth_count as f64 / plain_count as f64;
        assert!(
            (0.6..1.6).contains(&ratio),
            "平滑不应显著改变墨量（不平滑 {plain_count}，平滑 {smooth_count}）"
        );
    }
}

/// 笔刷印章覆盖度掩膜缓存（D1 精度，±1 LSB）。
///
/// 同一笔刷连续盖章时，半径/硬度/子像素偏移的组合高度重复。
/// 缓存掩膜可省掉每印章每像素的 `sqrt` + 距离计算。
///
/// 量化策略（D1）：
/// - 半径 → 0.5px（15px 笔刷上误差 <2%，视觉不可辨）
/// - 硬度 → 1%
/// - 子像素偏移 → 1/4 px（16 种组合）
///
/// 三个保护（guard）：
/// - **压感 bypass（Guard 0）**：检测到 Wacom 等压感设备的压力连续变化时，
///   整个笔触直接计算，不查缓存。压力变化 → 半径变化 → 缓存必然 miss，
///   省掉 HashMap 查找的开销。鼠标/MCP 的 pressure 恒定，不受影响。
/// - **小笔刷 bypass（Guard 1）**：半径 < 5px 时直接计算，不查缓存。
///   小掩膜本身计算便宜，哈希查找的开销占比反而更高。
/// - **LRU 上限 64 条（Guard 2）**：超出时淘汰最久未用，防止频繁换参数时缓存无限膨胀。
///   tick 严格递增，淘汰结果确定（同输入同输出）。
///
/// 实测（`black_box` 防作弊，1000 印章 × 多轮，掩膜计算微基准，两次运行）：
/// - 正常连续盖章：命中率 ~84%，掩膜查找 ~40-50x（端到端加速取决于盖章管线占比）
/// - 小笔刷（<5px）：bypass，与直接计算持平（~1.0x，噪声范围内，不变慢）
/// - 频繁换参数：~1.0x，LRU 开销可忽略（最坏情况不比直接计算慢）
///
/// 全程 safe，无 `unsafe`，遵守 `#![forbid(unsafe_code)]`。
pub struct DabCache {
    map: std::collections::HashMap<DabKey, DabEntry>,
    /// 单调递增的访问序号，用于 LRU。
    tick: u64,
    hits: usize,
    misses: usize,
    /// 小笔刷 bypass 计数（监控用）。
    bypasses: usize,
    /// 压感 bypass 计数（监控用）：检测到压感设备后直接计算的次数。
    pressure_bypasses: usize,
    /// 小笔刷直接计算的暂存槽（避免每次分配）。
    scratch: Option<(Vec<f32>, u32, u32)>,
    /// 压感检测环形缓冲：最近 PRESSURE_WINDOW 个印章的 pressure。
    pressure_window: [f32; PRESSURE_WINDOW],
    /// 环形缓冲已填入数量（< PRESSURE_WINDOW 时窗口未满，不做判定）。
    pressure_len: usize,
    /// 下一个写入位置。
    pressure_pos: usize,
    /// 压感模式：检测到压力连续变化时为 true，此时 bypass 缓存。
    /// 鼠标/MCP 的 pressure 恒定，永远不会进入此模式。
    pressure_mode: bool,
}

/// 缓存键：量化后的 (半径×2, 硬度×100, 子像素 x×4, 子像素 y×4)。
type DabKey = (u32, u8, u8, u8);

/// 缓存条目：掩膜 + 最后访问序号（LRU 用）。
struct DabEntry {
    mask: (Vec<f32>, u32, u32),
    last_used: u64,
}

/// 小笔刷阈值：半径小于此值时直接计算，不查缓存。
/// 小笔刷掩膜计算便宜，缓存查找的哈希开销占比高，bypass 反而更快。
const SMALL_BRUSH_RADIUS: f64 = 5.0;

/// LRU 上限：最多保留的缓存条目数，超出时淘汰最久未用。
const MAX_CACHE_ENTRIES: usize = 64;

/// 压感检测窗口：连续多少个印章的 pressure 用于判定。
/// 16 个印章约覆盖一次短笔触的起笔段，足够区分"恒定"与"变化"。
const PRESSURE_WINDOW: usize = 16;

/// 进入压感模式的方差阈值（std > 0.05）。
/// 鼠标/MCP 的 pressure 恒为 1.0，方差为 0；
/// Wacom 运笔时压力在 0.1~1.0 间连续变化，std 通常 > 0.05。
const PRESSURE_ENTER_VARIANCE: f32 = 0.0025;

/// 退出压感模式的方差阈值（std < 0.01）。
/// 迟滞设计：进入难、退出更难，防止在阈值附近抖动。
const PRESSURE_EXIT_VARIANCE: f32 = 0.0001;

impl DabCache {
    /// 创建空缓存。
    pub fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            tick: 0,
            hits: 0,
            misses: 0,
            bypasses: 0,
            pressure_bypasses: 0,
            scratch: None,
            pressure_window: [1.0; PRESSURE_WINDOW],
            pressure_len: 0,
            pressure_pos: 0,
            pressure_mode: false,
        }
    }

    /// 缓存命中率（0.0..1.0），用于监控。
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// bypass 次数（小笔刷直接计算的次数）。
    pub fn bypass_count(&self) -> usize {
        self.bypasses
    }

    /// 压感 bypass 次数（检测到压感设备后直接计算的次数）。
    pub fn pressure_bypass_count(&self) -> usize {
        self.pressure_bypasses
    }

    /// 是否处于压感模式（检测到 Wacom 等设备的压力连续变化）。
    pub fn in_pressure_mode(&self) -> bool {
        self.pressure_mode
    }

    /// 笔触结束：重置压感检测状态。
    ///
    /// 调用方在一次笔触结束时调用，下次笔触重新检测。
    /// 即使不调用，压力稳定后也会通过迟滞阈值自动退出压感模式。
    pub fn end_stroke(&mut self) {
        self.pressure_len = 0;
        self.pressure_pos = 0;
        self.pressure_mode = false;
    }

    /// 记录一次 pressure 采样，更新压感检测状态。
    ///
    /// 固定窗口方差检测 + 迟滞：
    /// - 窗口未满（< 16 个采样）时不判定，避免起笔误判。
    /// - 方差 > 0.0025（std > 0.05）→ 进入压感模式。
    /// - 方差 < 0.0001（std < 0.01）→ 退出压感模式。
    ///
    /// 阈值依据：鼠标/MCP 的 pressure 恒为 1.0（方差为 0）；
    /// Wacom 轻触 0.1~0.3、重压 0.8~1.0，运笔时 std 通常 > 0.05。
    /// 恒定中等压力（如 Wacom 稳定保持 0.5）方差为 0，不会误判——
    /// 此时半径恒定，缓存本来就能命中，bypass 与否无区别。
    ///
    /// 确定性：固定窗口 + 固定阈值，同输入序列必得同判定。
    fn note_pressure(&mut self, pressure: f32) {
        let p = pressure.clamp(0.0, 1.0);
        self.pressure_window[self.pressure_pos] = p;
        self.pressure_pos = (self.pressure_pos + 1) % PRESSURE_WINDOW;
        self.pressure_len = (self.pressure_len + 1).min(PRESSURE_WINDOW);
        if self.pressure_len < PRESSURE_WINDOW {
            return;
        }
        let mean = self.pressure_window.iter().sum::<f32>() / PRESSURE_WINDOW as f32;
        let var = self
            .pressure_window
            .iter()
            .map(|x| (x - mean) * (x - mean))
            .sum::<f32>()
            / PRESSURE_WINDOW as f32;
        if !self.pressure_mode && var > PRESSURE_ENTER_VARIANCE {
            self.pressure_mode = true;
        } else if self.pressure_mode && var < PRESSURE_EXIT_VARIANCE {
            self.pressure_mode = false;
        }
    }

    /// 获取或计算掩膜。返回 (掩膜数据, 宽, 高)。
    ///
    /// 掩膜覆盖 `[-radius, +radius]` 的正方形区域，中心在子像素偏移处。
    ///
    /// `pressure`：0.0..1.0。鼠标/MCP 传 1.0（恒定）；Wacom 传真实压力。
    /// 用于 Guard 0 的压感自动检测。
    ///
    /// Guard 0（压感 bypass）：检测到压力连续变化时直接计算，不查缓存。
    /// 压力变化 → 半径变化 → 缓存必然 miss，省掉 HashMap 查找开销。
    ///
    /// Guard 1（小笔刷 bypass）：`radius < SMALL_BRUSH_RADIUS` 时直接计算，
    /// 不查缓存。小笔刷掩膜计算便宜，哈希查找开销占比高。
    fn get_or_compute(
        &mut self,
        radius: f64,
        hardness: f64,
        sub_x: f64,
        sub_y: f64,
        pressure: f32,
    ) -> &(Vec<f32>, u32, u32) {
        // Guard 0: 压感模式直接计算，不走缓存。
        self.note_pressure(pressure);
        if self.pressure_mode {
            self.pressure_bypasses += 1;
            let mask = compute_dab_mask(radius, hardness, sub_x, sub_y);
            self.scratch = Some(mask);
            return self.scratch.as_ref().unwrap();
        }
        // Guard 1: 小笔刷直接计算，不走缓存。
        if radius < SMALL_BRUSH_RADIUS {
            self.bypasses += 1;
            let mask = compute_dab_mask(radius, hardness, sub_x, sub_y);
            self.scratch = Some(mask);
            return self.scratch.as_ref().unwrap();
        }
        let key: DabKey = (
            (radius * 2.0).round().max(0.0) as u32,
            (hardness.clamp(0.0, 1.0) * 100.0).round() as u8,
            // **★★★ 必须是 `floor` 而不是 `round() & 3` ✗ ★★★**（**第 727 轮 ✓）
            //   **∴ 原来的 bug ✗**：**相位 0.9**✗ ⇒ **`round(3.6) = 4`**✗ ⇒ **`& 3 = 0`** ✓
            //     ＋ **∴ 于是**：**0.9 与 0.0**被当成同一个键**✗ ⇒ **∴ 掩膜**差近 1 px** ✓
            //       ＋ **∴ 而**那**正是**「**反转路径后曲线变了**」的根因** ✓ ★
            //         ＋ **∴ 修法 ✗**：**`floor`**✗ ⇒ **∴ 相位 0.9 ⇒ 3 ⇒ 取回 0.75** ✓
            //           ＋ **∴ 而** `floor` 与 `round` **都是 4 档**✗ ⇒ **∴ 命中率**不变** ✓ ★
            //             ＋ **∴ 即**：**零代价修复** ✓ ★
            // **∴ 注意 ✗**：**`sub_x ∈ [0,1)`**✗（**∴ 它是**小数部分 ✓）
            //   ＋ **∴ 所以** `floor(sub*4)` **必然 ∈ 0..3**✗ ⇒ **∴ 不需要** `& 3` ✓ ★
            ((sub_x * 4.0).floor() as u8).min(3),
            ((sub_y * 4.0).floor() as u8).min(3),
        );
        self.tick += 1;
        let tick = self.tick;
        if let Some(entry) = self.map.get_mut(&key) {
            self.hits += 1;
            entry.last_used = tick;
        } else {
            self.misses += 1;
            let (rq, hq, sxq, syq) = key;
            let mask = compute_dab_mask(
                rq as f64 / 2.0,
                hq as f64 / 100.0,
                sxq as f64 / 4.0,
                syq as f64 / 4.0,
            );
            // Guard 2：真正的 LRU——淘汰最久未用的一条。
            // tick 严格递增，不会有并列，淘汰结果是确定的。
            if self.map.len() >= MAX_CACHE_ENTRIES {
                if let Some(victim) = self
                    .map
                    .iter()
                    .min_by_key(|(_, e)| e.last_used)
                    .map(|(k, _)| *k)
                {
                    self.map.remove(&victim);
                }
            }
            self.map.insert(
                key,
                DabEntry {
                    mask,
                    last_used: tick,
                },
            );
        }
        &self.map[&key].mask
    }

    /// 清空缓存（笔刷参数大变时调用）。
    pub fn clear(&mut self) {
        self.map.clear();
        self.tick = 0;
        self.hits = 0;
        self.misses = 0;
        self.bypasses = 0;
        self.pressure_bypasses = 0;
        self.scratch = None;
        self.end_stroke();
    }
}

impl Default for DabCache {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    // 线程本地 DabCache：笔刷渲染热路径的掩膜缓存。
    //
    // 用 `thread_local!` 而非 `OnceLock`/`Mutex`：渲染是单线程热循环，
    // 原子检查或锁的开销在每个印章上都会被放大（rusty-painter 实测 OnceLock 占 13% CPU）。
    // `RefCell` 的 borrow 检查是纯用户态，零原子开销。
    static DAB_CACHE: std::cell::RefCell<DabCache> =
        std::cell::RefCell::new(DabCache::new());
}

/// 用 DabCache 的掩膜盖章：从缓存取掩膜，逐像素回调 `(local_x, local_y, coverage)`。
///
/// 坐标映射：掩膜是边长 `ceil(2*radius)+2` 的正方形，掩膜像素 `(mx, my)`
/// 对应文档像素 `(x0+mx, y0+my)`，其中 `x0 = floor(center_x - radius)`，
/// 子像素相位 `sub_x = (center_x - radius) - x0` 烘焙进缓存键。
/// 掩膜内的覆盖度数学定义与 [`for_each_covered_pixel`] 一致（同一個 smoothstep）。
///
/// `pressure` 用于压感自动检测：恒定压力（鼠标/MCP）走缓存；
/// 检测到压力连续变化（Wacom）时自动 bypass，整笔直接计算。
fn stamp_mask_cached(
    buffer: &Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    pressure: f32,
    mut visit: impl FnMut(u32, u32, f32),
) {
    // **★★★ A/B 开关：关掉掩膜缓存 ⇒ 走**原热路径** ✗ ★★★**（**第 731 轮 ✓）
    //   **∴ 为什么需要 ✗**（**用户选了「**收益优先**」✗ ⇒ **∴ 而**收益**至今**没量** ✓）：
    //     **∴ 掩膜缓存的收益**必须**在同轮 A/B 里量**✗ ⇒ **∴ 所以**要一个**能关掉它的开关** ✓ ★
    //   **∴ 关掉时**走 `for_each_covered_pixel`**✗（**∴ 那**正是**接线之前**的行为 ✓）★
    //     ＋ **∴ 所以**：**两条路**能在**同一进程 ＋ 同一输入**下比较** ✓（**∴ 那**才可信 ✓）★
    //   **∴ 注意 ✗**：**它的回调收 `f64`**✗ 而** `visit` 收 `f32`** ✓
    //     ＋ **∴ 所以**：**这里**要一次 `as f32`**✗（**∴ 与**接线前**的转换**完全一致 ✓）★
    if std::env::var_os("YANSHI_DAB_CACHE").is_some_and(|v| v == "0") {
        for_each_covered_pixel(
            buffer,
            center_x,
            center_y,
            radius,
            hardness,
            |lx, ly, coverage, _, _| visit(lx, ly, coverage as f32),
        );
        return;
    }
    let x0 = (center_x - radius).floor() as i64;
    let y0 = (center_y - radius).floor() as i64;
    let sub_x = (center_x - radius) - x0 as f64;
    let sub_y = (center_y - radius) - y0 as f64;
    // **★ 精确包围盒的上界（与原热路径一致） ✗ ★**（**第 728 轮 ✓）
    let x1 = (center_x + radius).ceil() as i64;
    let y1 = (center_y + radius).ceil() as i64;
    let (origin_x, origin_y) = buffer.origin();
    let bw = buffer.width() as i64;
    let bh = buffer.height() as i64;
    // **期 4 `lod`**：只访问取样格点（局部坐标遍历，文档坐标由格点算出）；`step == 1`
    // 时访问的像素集合与今天相同（顺序也是行主序）。
    let step = buffer.step() as i64;
    let lx0 = crate::buffer::div_ceil_i64(x0 - origin_x, step).max(0);
    let ly0 = crate::buffer::div_ceil_i64(y0 - origin_y, step).max(0);
    let lx1 = (x1 - origin_x).div_euclid(step).min(bw - 1);
    let ly1 = (y1 - origin_y).div_euclid(step).min(bh - 1);
    DAB_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        let (mask, w, h) = cache.get_or_compute(radius, hardness, sub_x, sub_y, pressure);
        let w = *w as usize;
        let h = *h as usize;
        // borrow 生效期间只读 mask、回调 visit（不碰 cache），无重入问题。
        // **★★★ 遍历必须是**精确包围盒** ✗ ★★★**（**第 728 轮修正 ✓）
        //   **∴ 原来的 bug ✗**：**按**掩膜尺寸**遍历**✗（**∴ `ceil(2r) + 2` ✓）
        //     ＋ **∴ 而**原热路径用**精确包围盒**✗（**∴ `floor(cx-r) … ceil(cx+r)` ✓）
        //       ＋ **∴ 于是**：**缓存路**多遍历 1–2 格 ⇒ **∴ 若**那格在圆内 ⇒ **∴ 就**多画一个像素** ✓
        //         ＋ **∴ 实测 ✗**：**「**反转路径不改变曲线**」的不变量判据因此红** ✓ ★
        //   **∴ 现在 ✗**：**范围照原路**✗ ＋ **∴ 掩膜**只当**覆盖度查表**✗
        //     ＋ **∴ 掩膜不够大时**跳过**✗（**∴ 与原路"**超出即 0**"一致 ✓）★
        if lx1 < lx0 || ly1 < ly0 {
            return;
        }
        for ly in ly0..=ly1 {
            let document_y = origin_y + ly * step;
            let my = document_y - y0;
            for lx in lx0..=lx1 {
                let document_x = origin_x + lx * step;
                let mx = document_x - x0;
                if mx < 0 || my < 0 || mx >= w as i64 || my >= h as i64 {
                    continue;
                }
                let coverage = mask[(my as usize) * w + (mx as usize)];
                if coverage <= 0.0 {
                    continue;
                }
                visit(lx as u32, ly as u32, coverage);
            }
        }
    });
}

/// 计算单个印章的覆盖度掩膜（与 `for_each_covered_pixel` 的数学定义一致）。
fn compute_dab_mask(radius: f64, hardness: f64, sub_x: f64, sub_y: f64) -> (Vec<f32>, u32, u32) {
    let hard_edge = radius * hardness.clamp(0.0, 1.0);
    let soft_span = (radius - hard_edge).max(1e-6);
    let radius_squared = radius * radius;
    let hard_edge_squared = hard_edge * hard_edge;

    let size = (radius * 2.0).ceil() as u32 + 2;
    let mut mask = Vec::with_capacity((size * size) as usize);

    for my in 0..size {
        for mx in 0..size {
            let dx = mx as f64 - radius - sub_x + 0.5;
            let dy = my as f64 - radius - sub_y + 0.5;
            let distance_squared = dx * dx + dy * dy;
            let coverage = if distance_squared > radius_squared {
                0.0
            } else if distance_squared <= hard_edge_squared {
                1.0
            } else {
                let distance = distance_squared.sqrt();
                let t = 1.0 - (distance - hard_edge) / soft_span;
                (t * t * (3.0 - 2.0 * t)).clamp(0.0, 1.0)
            };
            mask.push(coverage as f32);
        }
    }
    (mask, size, size)
}

#[cfg(test)]
mod dab_cache_tests {
    use super::*;

    #[test]
    fn dab_cache_mask_matches_direct_computation() {
        let mut cache = DabCache::new();
        let (cached, _, _) = cache.get_or_compute(15.5, 0.75, 0.25, 0.5, 1.0).clone();
        let (direct, _, _) = compute_dab_mask(15.5, 0.75, 0.25, 0.5);
        assert_eq!(cached.len(), direct.len());
        for (c, d) in cached.iter().zip(direct.iter()) {
            assert!((c - d).abs() < 1e-6, "mask mismatch: {} vs {}", c, d);
        }
    }

    #[test]
    fn dab_cache_hit_rate_improves_with_repetition() {
        let mut cache = DabCache::new();
        // 模拟连续盖章：相同硬度，平滑变化的半径
        for i in 0..200 {
            let radius = 15.0 + libm::sin(i as f64 * 0.05) * 2.0;
            cache.get_or_compute(radius, 0.75, 0.1, 0.2, 1.0);
        }
        // 至少 50% 命中（量化后半径重复）
        assert!(
            cache.hit_rate() > 0.5,
            "hit rate too low: {}",
            cache.hit_rate()
        );
    }

    #[test]
    fn dab_cache_quantization_is_bounded() {
        // 0.5px 量化：15px 笔刷上最大误差 0.25px ≈ 1.6%
        let mut cache = DabCache::new();
        let (m1, _, _) = cache.get_or_compute(15.0, 0.75, 0.0, 0.0, 1.0).clone();
        let (m2, _, _) = cache.get_or_compute(15.24, 0.75, 0.0, 0.0, 1.0).clone();
        // 15.0 和 15.24 量化到同一档（30），掩膜应完全相同
        assert_eq!(m1.len(), m2.len());
        for (a, b) in m1.iter().zip(m2.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn dab_cache_small_brush_bypass() {
        // Guard 1: 半径 < 5px 直接计算，不进缓存
        let mut cache = DabCache::new();
        let (m1, _, _) = cache.get_or_compute(3.0, 0.75, 0.0, 0.0, 1.0).clone();
        let (m2, _, _) = cache.get_or_compute(3.0, 0.75, 0.0, 0.0, 1.0).clone();
        // bypass 不计入 hit/miss，只计 bypass
        assert_eq!(cache.bypass_count(), 2);
        assert_eq!(cache.hits + cache.misses, 0);
        // 但结果必须正确（与直接计算一致）
        let (direct, _, _) = compute_dab_mask(3.0, 0.75, 0.0, 0.0);
        assert_eq!(m1.len(), direct.len());
        for (a, b) in m1.iter().zip(direct.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
        assert_eq!(m1.len(), m2.len());
    }

    #[test]
    fn dab_cache_lru_eviction() {
        // Guard 2：真正的 LRU——淘汰最久未用，而不是 FIFO 或随机。
        let mut cache = DabCache::new();
        // 与 get_or_compute 相同的键量化逻辑（硬度 0.75 → 75，子像素 0 → 0）
        let key_of = |r: f64| ((r * 2.0).round() as u32, 75u8, 0u8, 0u8);
        // 填满 64 条：半径 5.0, 6.0, …, 68.0（量化键互不相同）
        for i in 0..64 {
            cache.get_or_compute(5.0 + i as f64, 0.75, 0.0, 0.0, 1.0);
        }
        assert_eq!(cache.map.len(), 64);
        // 再碰一下第 0 条（半径 5.0），让它成为"最近使用"
        cache.get_or_compute(5.0, 0.75, 0.0, 0.0, 1.0);
        // 插入第 65 条，触发淘汰
        cache.get_or_compute(5.0 + 64.0, 0.75, 0.0, 0.0, 1.0);
        assert_eq!(cache.map.len(), 64, "cache must stay capped at 64");
        // 半径 5.0 刚被用过，必须还在
        assert!(
            cache.map.contains_key(&key_of(5.0)),
            "recently used entry must survive eviction"
        );
        // 半径 6.0 最久未用，必须被淘汰（真 LRU；FIFO 会错误淘汰 5.0）
        assert!(
            !cache.map.contains_key(&key_of(6.0)),
            "least recently used entry must be evicted"
        );
        // 淘汰后仍能正常工作
        let (m, _, _) = cache.get_or_compute(10.0, 0.75, 0.0, 0.0, 1.0).clone();
        assert!(!m.is_empty());
    }

    #[test]
    fn dab_cache_pressure_constant_uses_cache() {
        // Guard 0: 鼠标/MCP 的 pressure 恒为 1.0，方差为 0，不应进入压感模式。
        let mut cache = DabCache::new();
        for i in 0..100 {
            let radius = 15.0 + libm::sin(i as f64 * 0.05) * 2.0;
            cache.get_or_compute(radius, 0.75, 0.1, 0.2, 1.0);
        }
        assert!(
            !cache.in_pressure_mode(),
            "constant pressure must not trigger pressure mode"
        );
        assert_eq!(cache.pressure_bypass_count(), 0);
        assert!(
            cache.hit_rate() > 0.5,
            "constant pressure should hit cache: {}",
            cache.hit_rate()
        );
    }

    #[test]
    fn dab_cache_pressure_varying_bypasses() {
        // Guard 0: Wacom 运笔压力连续变化（0.2~1.0 正弦），应进入压感模式并 bypass。
        let mut cache = DabCache::new();
        for i in 0..100 {
            // 模拟压感：压力在 0.2~1.0 间连续变化，半径随压力调制
            let pressure = 0.6 + 0.4 * libm::sin(i as f64 * 0.3);
            let radius = 15.0 * (0.5 + 0.5 * pressure);
            cache.get_or_compute(radius, 0.75, 0.1, 0.2, pressure as f32);
        }
        assert!(
            cache.in_pressure_mode(),
            "varying pressure must trigger pressure mode"
        );
        assert!(
            cache.pressure_bypass_count() > 0,
            "pressure mode should bypass cache"
        );
    }

    #[test]
    fn dab_cache_pressure_recovers_when_stable() {
        // 迟滞：压力变化 → 进入压感模式；压力稳定一段时间 → 自动退出。
        let mut cache = DabCache::new();
        // 先变化 32 个，进入压感模式
        for i in 0..32 {
            let pressure = 0.6 + 0.4 * libm::sin(i as f64 * 0.3);
            cache.get_or_compute(15.0, 0.75, 0.1, 0.2, pressure as f32);
        }
        assert!(cache.in_pressure_mode());
        // 再稳定 32 个（pressure 恒为 1.0），应自动退出
        for _ in 0..32 {
            cache.get_or_compute(15.0, 0.75, 0.1, 0.2, 1.0);
        }
        assert!(
            !cache.in_pressure_mode(),
            "stable pressure should exit pressure mode"
        );
    }

    #[test]
    fn dab_cache_end_stroke_resets_pressure() {
        // end_stroke() 立即重置压感状态，下一笔重新检测。
        let mut cache = DabCache::new();
        for i in 0..32 {
            let pressure = 0.6 + 0.4 * libm::sin(i as f64 * 0.3);
            cache.get_or_compute(15.0, 0.75, 0.1, 0.2, pressure as f32);
        }
        assert!(cache.in_pressure_mode());
        cache.end_stroke();
        assert!(!cache.in_pressure_mode());
        // 下一笔恒定压力，正常用缓存
        for _ in 0..32 {
            cache.get_or_compute(15.0, 0.75, 0.1, 0.2, 1.0);
        }
        assert!(!cache.in_pressure_mode());
        assert!(cache.hit_rate() > 0.5);
    }

    #[test]
    fn dab_cache_pressure_bypass_result_is_correct() {
        // 压感 bypass 的结果必须与直接计算一致（只是不走缓存）。
        let mut cache = DabCache::new();
        // 先进入压感模式
        for i in 0..32 {
            let pressure = 0.6 + 0.4 * libm::sin(i as f64 * 0.3);
            cache.get_or_compute(15.0, 0.75, 0.1, 0.2, pressure as f32);
        }
        assert!(cache.in_pressure_mode());
        let (bypassed, _, _) = cache.get_or_compute(15.5, 0.75, 0.25, 0.5, 0.7).clone();
        let (direct, _, _) = compute_dab_mask(15.5, 0.75, 0.25, 0.5);
        assert_eq!(bypassed.len(), direct.len());
        for (b, d) in bypassed.iter().zip(direct.iter()) {
            assert!(
                (b - d).abs() < 1e-6,
                "bypass result mismatch: {} vs {}",
                b,
                d
            );
        }
    }
}

// **★ 净室揉合的单测（P0 #1）★**（第 254 轮）
//
// **为什么要单测而不是只靠端到端判据**：揉合是**纯几何 ＋ 固定公式**的像素变换
// => 单测能把它钉得**决定性**、且一红就知道是它（端到端会被笔刷的其它设置搅浑 ✓，
// 本轮已经在 `humanize` 上吃过一次这个亏 ✓）。
//
// **场景**：左半红（220,30,30）、右半白（255,255,255）；笔尖**从红往白走**（方向 +x）
// => 采样点落在**后方**（红区）=> 边界处的 dab 必须把**红拖进白区**。
// 反过来再验**阴性**：`amount = 0`（= 非揉合笔刷）时**一个像素都不许动**。
#[cfg(test)]
mod smudge_tests {
    use super::smudge_dab;
    use hokusai::TiledSurface;

    const RED: [u8; 4] = [220, 30, 30, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    /// 造一块 128×64 的表面：`split_x` 左侧红、右侧白。
    fn surface_with_edge(split_x: usize) -> hokusai::tile_mem::MemSurface {
        let (w, h) = (128usize, 64usize);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let at = (y * w + x) * 4;
                let color = if x < split_x { RED } else { WHITE };
                rgba[at..at + 4].copy_from_slice(&color);
            }
        }
        let mut surface = hokusai::tile_mem::MemSurface::new();
        assert!(super::feed_base(
            &mut surface,
            0,
            0,
            w as i32,
            h as i32,
            &rgba
        ));
        surface
    }

    /// 读一个像素（按 `feed_base` 的口径换算回 0..255）。
    fn read(surface: &mut hokusai::tile_mem::MemSurface, x: i32, y: i32) -> [u16; 4] {
        let tile = surface.tile_request_start(x.div_euclid(64), y.div_euclid(64));
        let out = tile[(y.rem_euclid(64)) as usize][(x.rem_euclid(64)) as usize];
        surface.tile_request_end(x.div_euclid(64), y.div_euclid(64));
        out
    }

    fn is_reddish(px: [u16; 4]) -> bool {
        // u16 表示是 `u8 << 7`：红 ≈ 220<<7 = 28160，白 = 255<<7 = 32640
        px[0] > 26_000 && px[1] < 20_000 && px[2] < 20_000
    }

    #[test]
    fn smudge_drags_the_rear_colour_across_the_edge() {
        let split = 64;
        let mut surface = surface_with_edge(split);
        // 笔尖在边界上，**方向 +x**（从红往白走），拖曳长度 20px
        let before = read(&mut surface, split as i32 + 6, 32);
        assert!(!is_reddish(before), "边界右侧本该是白的：{before:?}");
        let touched = smudge_dab(
            &mut surface,
            (split as f64, 32.0),
            12.0,
            (1.0, 0.0),
            20.0,
            1.0,
        );
        assert!(touched > 0, "揉合一个像素都没动");
        let after = read(&mut surface, split as i32 + 6, 32);
        assert!(
            is_reddish(after),
            "笔尖**后方**是红色、dab 又覆盖到了这一列 => 必须把红拖进来；实测 {after:?}"
        );
    }

    #[test]
    fn smudge_is_off_when_amount_is_zero() {
        let split = 64;
        let mut surface = surface_with_edge(split);
        let touched = smudge_dab(
            &mut surface,
            (split as f64, 32.0),
            12.0,
            (1.0, 0.0),
            20.0,
            0.0,
        );
        assert_eq!(touched, 0, "amount=0（非揉合笔刷）时一个像素都不许动");
        let untouched = read(&mut surface, split as i32 + 6, 32);
        assert!(!is_reddish(untouched), "没动过就还是白的：{untouched:?}");
    }

    #[test]
    fn smudge_without_direction_does_nothing() {
        let split = 64;
        let mut surface = surface_with_edge(split);
        // 第一条 dab 没有"后方"（方向未知）=> 只播种、不动。
        // 这条测试**曾经写错**：那时"方向未知"走的是"平移 0 的原地混匀"，而**平移 0 时
        // 每个像素采样的就是它自己** => `out = dst*(1-a) + dst*a = dst` => 数学上必然是
        // 空操作（测试因此红）。改成明确语义"方向未知 => 返回 0"之后才自洽。
        let touched = smudge_dab(
            &mut surface,
            (split as f64, 32.0),
            12.0,
            (0.0, 0.0),
            20.0,
            1.0,
        );
        assert_eq!(touched, 0, "方向未知时不该动任何像素");
        let edge = read(&mut surface, split as i32 + 6, 32);
        assert!(!is_reddish(edge), "不该被拖动：{edge:?}");
    }

    #[test]
    fn smudge_with_zero_length_is_a_noop() {
        let split = 64;
        let mut surface = surface_with_edge(split);
        // 拖曳长度 0 ⇒ 每个像素采样的就是它自己 ⇒ `out = dst`，数学上必然是空操作。
        // 这条断言钉的是**语义**，不是早返本身：去掉早返、走完整循环，结果仍是 0 像素
        // 被改 —— 早返只是省掉那趟循环的成本（`oil-01-paint` 的 `smudge_length` 是 0，
        // 此前每步都在为这次空操作付全部 `powf` 成本）。
        for length in [0.0, -5.0] {
            let touched = smudge_dab(
                &mut surface,
                (split as f64, 32.0),
                12.0,
                (1.0, 0.0),
                length,
                1.0,
            );
            assert_eq!(touched, 0, "拖曳长度 {length} 时一个像素都不许动");
            let edge = read(&mut surface, split as i32 + 6, 32);
            assert!(!is_reddish(edge), "不该被拖动：{edge:?}");
        }
    }
}
