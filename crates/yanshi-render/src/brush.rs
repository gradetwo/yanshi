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
            Some(Self { points })
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
pub fn stamp_stroke(buffer: &mut Buffer, brush: &BrushSpec, stroke: &StrokeGeometry) -> usize {
    let spacing = brush.spacing_pixels();
    let samples: Vec<(f64, f64, f64)> = stroke
        .points
        .iter()
        .map(|point| (point.x, point.y, point.pressure))
        .collect();
    let stamps = dashed_line(&samples, spacing, brush.dash);
    stamp_samples(buffer, brush, &stamps)
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
pub fn draw_stamp(
    buffer: &mut Buffer,
    center_x: f64,
    center_y: f64,
    radius: f64,
    hardness: f64,
    color: LinearRgba,
    mode: BlendMode,
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
    let use_fast_path = mode == BlendMode::Normal;

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
            let alpha = color[3] * coverage as f32;
            if alpha <= 0.0 {
                continue;
            }
            let source = premultiply([color[0], color[1], color[2], alpha]);
            if use_fast_path {
                buffer.blend_at(local_x, local_y, source);
            } else {
                buffer.blend_mode(local_x, local_y, source, mode);
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
