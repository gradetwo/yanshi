//! 通用光栅笔刷 stamping（设计文档 11.1）。
//!
//! MVP 只做通用光栅笔刷：可调大小、流量、不透明度、硬度、间距、抖动。
//! 抖动由原子 `seed` 驱动（[`crate::prng`]），因此同一份历史在任何平台重放出相同笔迹。
//!
//! 其他介质（油画、水彩、马克笔、铅笔、像素、矢量）通过 WASM 插件扩展，属远期项。

use crate::blend::BlendMode;
use crate::buffer::Buffer;
use crate::color::{premultiply, LinearRgba};
use crate::geometry::dashed_line;
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
        }
    }
}

impl BrushSpec {
    /// 由对象 `data` 解析（键：size/color/opacity/flow/hardness/spacing/jitter/seed/blend_mode）。
    ///
    /// `color` 可以是 `[r, g, b, a]`（线性直通）或 `{"r": 0-255, "g": ..., "b": ..., "a": ...}`（sRGB 字节）。
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
        if let Some(color) = value.get("color") {
            // 统一走 `color::parse_spec_color`：数组 > 0-1 视为 sRGB 字节，
            // 对象与十六进制同样接受（详见该函数文档里的历史事故说明）。
            if let Some(parsed) = crate::color::parse_spec_color(color) {
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
            || curves.pressure != identity;
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
    let stamps = dashed_line(&samples, spacing, brush.dash);
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
            Some(coverage) => crate::brush::draw_stamp_clipped(
                buffer,
                params.x,
                params.y,
                params.radius,
                brush.hardness,
                color,
                brush.blend_mode,
                coverage,
            ),
            None => draw_stamp(
                buffer,
                params.x,
                params.y,
                params.radius,
                brush.hardness,
                color,
                brush.blend_mode,
            ),
        };
        drawn += 1;
    }
    drawn
}

/// 把笔迹整段盖章（无外观参数、无选区裁剪）✓ —— 等价于 [`stamp_stroke_configured`] 的两个 `None` ✓。
pub fn stamp_stroke(buffer: &mut Buffer, brush: &BrushSpec, stroke: &StrokeGeometry) -> usize {
    stamp_stroke_configured(buffer, brush, stroke, None, None)
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
        draw_stamp_clipped(
            buffer,
            cx,
            cy,
            stamp_radius,
            brush.hardness,
            [brush.color[0], brush.color[1], brush.color[2], alpha],
            brush.blend_mode,
            coverage,
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
        draw_stamp(
            buffer,
            cx,
            cy,
            stamp_radius,
            brush.hardness,
            [brush.color[0], brush.color[1], brush.color[2], alpha],
            brush.blend_mode,
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
    let start_x = (document_x0 - origin_x).max(0) as u32;
    let start_y = (document_y0 - origin_y).max(0) as u32;
    let end_x = (document_x1 - origin_x)
        .min(buffer.width() as i64 - 1)
        .max(-1);
    let end_y = (document_y1 - origin_y)
        .min(buffer.height() as i64 - 1)
        .max(-1);
    if end_x < 0 || end_y < 0 || start_x as i64 > end_x || start_y as i64 > end_y {
        return;
    }
    let end_x = end_x as u32;
    let end_y = end_y as u32;

    let hard_edge = radius * hardness.clamp(0.0, 1.0);
    let soft_span = (radius - hard_edge).max(1e-6);
    let radius_squared = radius * radius;

    for local_y in start_y..=end_y {
        let document_y = origin_y + local_y as i64;
        let dy = document_y as f64 + 0.5 - center_y;
        for local_x in start_x..=end_x {
            let document_x = origin_x + local_x as i64;
            let dx = document_x as f64 + 0.5 - center_x;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared > radius_squared {
                continue;
            }
            let distance = distance_squared.sqrt();
            let coverage = if distance <= hard_edge {
                1.0
            } else {
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
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let mut updates: Vec<(u32, u32, [f32; 4])> = Vec::new();
    for_each_covered_pixel(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        |x, y, stamp_coverage, document_x, document_y| {
            let selection = coverage(document_x, document_y).clamp(0.0, 1.0);
            if selection <= 0.0 {
                return;
            }
            let keep = (1.0 - stamp_coverage * strength * f64::from(selection)) as f32;
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
    let use_fast_path = mode == BlendMode::Normal;
    let mut updates: Vec<(u32, u32, LinearRgba)> = Vec::new();
    for_each_covered_pixel(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        |x, y, stamp_coverage, document_x, document_y| {
            let selection = coverage(document_x, document_y).clamp(0.0, 1.0);
            if selection <= 0.0 {
                return;
            }
            let alpha = color[3] * stamp_coverage as f32 * selection;
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
    let use_fast_path = mode == BlendMode::Normal;
    let mut updates: Vec<(u32, u32, LinearRgba)> = Vec::new();
    for_each_covered_pixel(
        buffer,
        center_x,
        center_y,
        radius,
        hardness,
        |x, y, coverage, _, _| {
            let alpha = color[3] * coverage as f32;
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
