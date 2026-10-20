//! **SVG 矢量导出（路线乙：光栅笔触算法转矢量）** ✓
//!
//! 把通用光栅笔触（[`crate::brush`] 的圆形 dab 盖章路径）转成 SVG 填充路径：
//! 脊线解析包络线为主、局部 alpha 场亚像素 Marching Squares 兜底复杂自交，
//! 轮廓经 RDP 简化与 Schneider 三次 Bézier 拟合后写出。
//!
//! **本模块只做纯几何与纯函数**（笔触 → 轮廓 / 分类 / 度量）✓；
//! 文档遍历、图层分组、位图回退的渲染在服务端 `export_svg` 工具里 ✓。
//!
//! **确定性** ✓：全程只用 `f64` 加减乘除与 `sqrt`（IEEE 精确舍入）✓；
//! 坐标写出走定点整数化（[`fmt_num`] ✓），同输入必同输出字节 ✓。
//!
//! **α_eff 的来源是实测、不是专家公式** ✓：专家给出
//! `α_eff = min(opacity, 1-(1-o)^(0.5/spacing))` ✗，本机实测（`size=40`、
//! `hardness=1`、直线笔触中心线）表明本引擎**没有 opacity 上限**且指数是
//! `1/spacing`：`spacing ∈ {0.05,0.1,0.25,0.5,1.0}` 时由实测 alpha 反推的
//! 覆盖数 `K` 分别是 `20.00 / 10.00 / 4.00 / 2.00 / 1.00` ⇒ **`K = 1/spacing`** ✓
//! （`spacing ≥ 1` 时 dab 不重叠 ⇒ `K = 1` ✓）。见 [`effective_alpha`]。

use crate::brush::{BrushSpec, StrokeAppearance, StrokeGeometry};
use crate::buffer::Buffer;
use crate::geometry::dashed_line;
use crate::prng::Prng;
use crate::BlendMode;

/// 二维点（文档坐标）。
pub type Pt = (f64, f64);

/// 把一个数按 **0.01 定点** 写进字符串（SVG 坐标专用 ✓）。
///
/// **为什么不用 `format!("{:.2}")`** ✗：标准库浮点格式化在边界值上受尾数
/// 影响、还会输出 `-0.00` ⇒ 跨平台字节可能漂移 ✗。
/// 本函数先缩放取整成 `i64` 再拼字符 ⇒ 全平台逐字节一致 ✓，
/// 且自动把 `12.00` 缩成 `12`、`0.50` 缩成 `.5` ✓。
pub fn fmt_num(value: f64) -> String {
    let scaled = (value * 100.0).round() as i64;
    if scaled == 0 {
        return "0".to_owned();
    }
    let mut out = String::with_capacity(12);
    if scaled < 0 {
        out.push('-');
    }
    let abs = scaled.unsigned_abs();
    let int_part = abs / 100;
    let frac_part = abs % 100;
    if int_part > 0 {
        out.push_str(&int_part.to_string());
    }
    if frac_part > 0 {
        out.push('.');
        out.push((b'0' + (frac_part / 10) as u8) as char);
        if frac_part % 10 != 0 {
            out.push((b'0' + (frac_part % 10) as u8) as char);
        }
    }
    out
}

/// 线性 RGB → SVG 的 sRGB 十六进制色（`#rrggbb`，可缩写时缩成 `#rgb` ✓）。
pub fn srgb_hex(linear_rgb: [f32; 3]) -> String {
    let mut bytes = [0u8; 3];
    for (slot, value) in bytes.iter_mut().zip(linear_rgb.iter()) {
        let srgb = crate::color::linear_to_srgb(*value).clamp(0.0, 1.0);
        *slot = (srgb * 255.0).round() as u8;
    }
    if bytes[0] >> 4 == bytes[0] & 0x0f
        && bytes[1] >> 4 == bytes[1] & 0x0f
        && bytes[2] >> 4 == bytes[2] & 0x0f
    {
        format!("#{:x}{:x}{:x}", bytes[0] >> 4, bytes[1] >> 4, bytes[2] >> 4)
    } else {
        format!("#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])
    }
}

/// 混合模式 → CSS `mix-blend-mode` 名（**映射不上的返回 `None`** ⇒ 调用方回退位图 ✓）。
///
/// 本引擎共 10 种（[`BlendMode::ALL`] ✓）：其中 `add`（线性减淡）与 `subtract`
/// **没有 CSS 对应值**（Figma 也不支持 `feComposite` 拼法）⇒ `None` ✓。
/// 专家答复里的映射表还列了 `exclusion`／`color_burn`／`hard_light`／`soft_light`，
/// **本引擎没有这四种** ⇒ 不在表里（以代码为准 ✓）。
pub fn blend_css(mode: BlendMode) -> Option<&'static str> {
    match mode {
        BlendMode::Normal => Some("normal"),
        BlendMode::Multiply => Some("multiply"),
        BlendMode::Screen => Some("screen"),
        BlendMode::Overlay => Some("overlay"),
        BlendMode::Darken => Some("darken"),
        BlendMode::Lighten => Some("lighten"),
        BlendMode::Difference => Some("difference"),
        BlendMode::ColorDodge => Some("color-dodge"),
        BlendMode::Add | BlendMode::Subtract => None,
    }
}

/// 图层混合模式名（字符串）→ CSS 名（同 [`blend_css`] 的表 ✓）。
pub fn blend_css_name(name: &str) -> Option<&'static str> {
    blend_css(BlendMode::from_name(name))
}

/// 单枚 dab 的导出几何（**只有中心与半径** ✓：本引擎通用光栅笔的 dab 是圆 ✓；
/// 椭圆 dab 只出现在介质笔路径里，而介质笔一律回退位图 ⇒ 这里不需要长宽比 ✓）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgDab {
    /// 中心 x（文档坐标）。
    pub x: f64,
    /// 中心 y（文档坐标）。
    pub y: f64,
    /// 半径（像素）。
    pub radius: f64,
}

/// 单枚 dab 的 alpha（**整笔恒定** ✓：`color.a × flow × opacity`，
/// 与 `stamp_samples_from` 的算法逐字一致 ✓）。
pub fn dab_alpha(brush: &BrushSpec) -> f64 {
    f64::from(brush.color[3]) * brush.flow * brush.opacity
}

/// 笔触中心线的**稳态累积 alpha**（把这一笔填成一条路径时该用的 `fill-opacity`）。
///
/// **公式来自实测校准**（见模块头 ✓）：覆盖数 `K = 2·r / spacing_px`
/// （`r = size/2` 时即 `1/spacing` ✓），下限 1（dab 不重叠时就是单枚 alpha ✓）：
/// `α_eff = 1 − (1 − a)^K`，`a` 为 [`dab_alpha`]。
/// **没有**专家公式里的 `min(opacity, …)` 上限 —— 本引擎实测能累积到远超
/// `opacity`（`o=0.2, spacing=0.05` 时实测 `0.9885`）✓。
pub fn effective_alpha(brush: &BrushSpec) -> f64 {
    let a = dab_alpha(brush).clamp(0.0, 1.0);
    if a <= 0.0 {
        return 0.0;
    }
    if a >= 1.0 {
        return 1.0;
    }
    let spacing_px = brush.spacing_pixels();
    let k = if spacing_px > 0.0 {
        (brush.size / spacing_px).max(1.0)
    } else {
        1.0
    };
    1.0 - (1.0 - a).powf(k)
}

/// 复刻引擎盖章路径，算出这一笔的 dab 序列（**按簇分组** ✓）。
///
/// 与 `stamp_stroke_configured_on` 的**无外观、无选区**路径逐字同构：
/// 平滑（`smooth` 时 Catmull-Rom 8 段）→ [`dashed_line`] 按 `spacing_pixels`
/// 采样 → 逐枚施加与 `stamp_samples_from` 相同的抖动（`Prng::derive(seed, index)` ✓）
/// 与压力半径（`min_size_ratio + (1−ratio)·p` ✓），半径 ≤ 0 的跳过 ✓。
///
/// **簇**：虚线（`dash`）的间断与**对称镜像**都会产生几何上分离的 dab 群 ⇒
/// 相邻 dab 距离 `> 1.5 × spacing_px` 处断簇 ✓；镜像 dab 群单独成簇
/// （顺序与原群一致 ✓，与引擎「原群后追加镜像群」的顺序对应 ✓）。
pub fn stroke_dab_clusters(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    canvas: (u32, u32),
) -> Vec<Vec<SvgDab>> {
    let spacing = brush.spacing_pixels();
    let smoothed;
    let source: &[crate::brush::StrokePoint] = if geometry.smooth {
        smoothed = crate::brush::catmull_rom_smooth(&geometry.points, 8);
        &smoothed
    } else {
        &geometry.points
    };
    let samples: Vec<(f64, f64, f64)> = source
        .iter()
        .map(|point| (point.x, point.y, point.pressure))
        .collect();
    let stamps = dashed_line(&samples, spacing, brush.dash);
    let base_radius = brush.size / 2.0;
    let mut dabs: Vec<SvgDab> = Vec::with_capacity(stamps.len());
    for (index, (x, y, pressure)) in stamps.iter().enumerate() {
        let (mut cx, mut cy) = (*x, *y);
        if brush.jitter > 0.0 {
            let mut stamp_rng = Prng::derive(brush.seed, index as u64);
            cx += stamp_rng.signed() as f64 * brush.jitter;
            cy += stamp_rng.signed() as f64 * brush.jitter;
        }
        let size_scale = if brush.pressure_size {
            let p = pressure.clamp(0.0, 1.0);
            brush.min_size_ratio + (1.0 - brush.min_size_ratio) * p
        } else {
            1.0
        };
        let radius = base_radius * size_scale;
        if radius <= 0.0 {
            continue;
        }
        dabs.push(SvgDab {
            x: cx,
            y: cy,
            radius,
        });
    }
    // 对称：镜像群追加在原群之后（与引擎一致 ✓），但导出时自成一簇 ✓。
    let mut groups: Vec<Vec<SvgDab>> = vec![dabs];
    if brush.symmetry != crate::brush::Symmetry::None {
        let mirrored: Vec<SvgDab> = groups[0]
            .iter()
            .map(|dab| {
                let (mx, my) = brush.symmetry.mirror(dab.x, dab.y, canvas);
                SvgDab {
                    x: mx,
                    y: my,
                    radius: dab.radius,
                }
            })
            .collect();
        groups.push(mirrored);
    }
    // 按间断断簇（虚线间断 ⇒ 相邻 dab 远隔 ✓）。
    let gap = spacing * 1.5;
    let mut clusters: Vec<Vec<SvgDab>> = Vec::new();
    for group in groups {
        let mut current: Vec<SvgDab> = Vec::new();
        for dab in group {
            if let Some(last) = current.last() {
                let dx = dab.x - last.x;
                let dy = dab.y - last.y;
                if (dx * dx + dy * dy).sqrt() > gap {
                    clusters.push(std::mem::take(&mut current));
                }
            }
            current.push(dab);
        }
        if !current.is_empty() {
            clusters.push(current);
        }
    }
    clusters
}

/// 脊线（平滑后采样点列）的**自交次数**（非相邻线段对的交点数 ✓）。
///
/// 用于分类（`> 2` ⇒ 走 alpha 场兜底 ✓）与半透明双层判定 ✓。
/// 端点相接的相邻段不计 ✓；共线重叠按不交计（导出场景里无害 ✓）。
pub fn spine_self_intersections(points: &[Pt]) -> usize {
    let mut count = 0usize;
    let n = points.len();
    if n < 4 {
        return 0;
    }
    for i in 0..n - 1 {
        for j in i + 2..n - 1 {
            if segments_cross(points[i], points[i + 1], points[j], points[j + 1]) {
                count += 1;
            }
        }
    }
    count
}

/// 两线段是否**真交叉**（交点在两段内部 ✓；端点相触不计 ⇒ 回笔尖点不误报 ✓）。
fn segments_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let d1 = cross(sub(d, c), sub(a, c));
    let d2 = cross(sub(d, c), sub(b, c));
    let d3 = cross(sub(b, a), sub(c, a));
    let d4 = cross(sub(b, a), sub(d, a));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}

fn cross(a: Pt, b: Pt) -> f64 {
    a.0 * b.1 - a.1 * b.0
}

fn dist(a: Pt, b: Pt) -> f64 {
    let d = sub(a, b);
    (d.0 * d.0 + d.1 * d.1).sqrt()
}

/// 多边形有向面积（鞋带公式 ✓；符号随点序与 y 向下而定，本模块内自洽即可 ✓）。
pub fn signed_area(loop_pts: &[Pt]) -> f64 {
    let mut sum = 0.0;
    let n = loop_pts.len();
    for i in 0..n {
        let (x0, y0) = loop_pts[i];
        let (x1, y1) = loop_pts[(i + 1) % n];
        sum += x0 * y1 - x1 * y0;
    }
    sum / 2.0
}

/// **路线 A：解析广义包络线** —— 由一簇 dab 生成闭合轮廓点列（不重复首点 ✓）。
///
/// 圆截面公式（专家 P0-1 的矩阵式在 `a = b = r` 时的化简 ✓）：
/// 第 `i` 枚 dab 的切向 `T_i` 取中心差分，半径变化率
/// `r'_i = (r_{i+1} − r_{i−1}) / |C_{i+1} − C_{i−1}|`；
/// `|r'_i| ≥ 1`（后圆吞没前圆，常见于顿笔）⇒ 跳过该 dab 的包络点 ✓；
/// 否则左右包络点在方向角 `atan2(T_i) ± arccos(−r'_i)` 处、距中心 `r_i` ✓。
/// 两侧各自过**尖点过滤**（新线段与上一段反向 `dot < −0.2` 的候选点丢弃 ✓），
/// 首尾以半圆端帽（10 段采样）闭合 ✓。
pub fn envelope_contour(dabs: &[SvgDab]) -> Vec<Pt> {
    if dabs.is_empty() {
        return Vec::new();
    }
    if dabs.len() == 1 {
        // 单枚 dab ⇒ 正 24 边形近似圆（后续拟合会把它收成 4 段弧 ✓）。
        let dab = dabs[0];
        return (0..24)
            .map(|i| {
                let angle = (i as f64) * std::f64::consts::TAU / 24.0;
                (
                    dab.x + dab.radius * angle.cos(),
                    dab.y + dab.radius * angle.sin(),
                )
            })
            .collect();
    }
    let n = dabs.len();
    // 切向（一端用单侧差分 ✓）。
    let tangents: Vec<Pt> = (0..n)
        .map(|i| {
            let (a, b) = if i == 0 {
                (dabs[0], dabs[1])
            } else if i == n - 1 {
                (dabs[n - 2], dabs[n - 1])
            } else {
                (dabs[i - 1], dabs[i + 1])
            };
            let d = (b.x - a.x, b.y - a.y);
            let len = (d.0 * d.0 + d.1 * d.1).sqrt();
            if len < 1e-12 {
                (1.0, 0.0)
            } else {
                (d.0 / len, d.1 / len)
            }
        })
        .collect();
    let mut left: Vec<Pt> = Vec::with_capacity(n);
    let mut right: Vec<Pt> = Vec::with_capacity(n);
    for i in 0..n {
        let dab = dabs[i];
        let (r_prev, r_next, span) = if i == 0 {
            (
                dab.radius,
                dabs[1].radius,
                dist((dab.x, dab.y), (dabs[1].x, dabs[1].y)),
            )
        } else if i == n - 1 {
            (
                dabs[n - 2].radius,
                dab.radius,
                dist((dabs[n - 2].x, dabs[n - 2].y), (dab.x, dab.y)),
            )
        } else {
            (
                dabs[i - 1].radius,
                dabs[i + 1].radius,
                dist(
                    (dabs[i - 1].x, dabs[i - 1].y),
                    (dabs[i + 1].x, dabs[i + 1].y),
                ),
            )
        };
        let r_slope = if span > 1e-12 {
            (r_next - r_prev) / span
        } else {
            0.0
        };
        if r_slope.abs() >= 1.0 && i != 0 && i != n - 1 {
            continue; // 被邻圆吞没 ⇒ 无包络点 ✓。
        }
        let r_slope = r_slope.clamp(-0.999, 0.999);
        let theta = tangents[i].1.atan2(tangents[i].0);
        let tilt = (-r_slope).acos();
        let phi_plus = theta + tilt;
        let phi_minus = theta - tilt;
        let lp = (
            dab.x + dab.radius * phi_plus.cos(),
            dab.y + dab.radius * phi_plus.sin(),
        );
        let rp = (
            dab.x + dab.radius * phi_minus.cos(),
            dab.y + dab.radius * phi_minus.sin(),
        );
        push_with_cusp_filter(&mut left, lp);
        push_with_cusp_filter(&mut right, rp);
    }
    if left.is_empty() || right.is_empty() {
        // 极端退化（全程吞没）：退回首 dab 的圆 ⇒ 至少有形状、不静默丢笔 ✓。
        return envelope_contour(&dabs[..1]);
    }
    // 端帽：起始帽从 left[0] 绕**行进反方向**到 right[0]；终点帽从 right[last]
    // 绕行进方向到 left[last]（各取弧内采样点，不含端点本身 ✓）。
    let start_cap = cap_arc(dabs[0], left[0], right[0], tangents[0], true);
    let end_cap = cap_arc(
        dabs[n - 1],
        right[right.len() - 1],
        left[left.len() - 1],
        tangents[n - 1],
        false,
    );
    let mut contour: Vec<Pt> = Vec::with_capacity(left.len() + right.len() + 22);
    contour.extend_from_slice(&left);
    // **端帽内点必须反序** ✓：`cap_arc` 按「from → to」生成，而轮廓遍历到
    // 终点帽时是从 left 端进入、到起始帽时是从 right 端进入 ⇒ 都是反着走的；
    // 不反序轮廓会在帽处自交、面积自相抵消（实测胶囊 954 vs 应有 1079 ✗）。
    contour.extend(end_cap.iter().rev());
    contour.extend(right.iter().rev());
    contour.extend(start_cap.iter().rev());
    contour
}

/// 尖点过滤：候选点与上一点构成的线段若与再上一段**明显反向**，丢弃候选 ✓。
fn push_with_cusp_filter(points: &mut Vec<Pt>, candidate: Pt) {
    if points.len() >= 2 {
        let last = points[points.len() - 1];
        let prev = points[points.len() - 2];
        let seg_new = sub(candidate, last);
        let seg_old = sub(last, prev);
        let len_new = (seg_new.0 * seg_new.0 + seg_new.1 * seg_new.1).sqrt();
        let len_old = (seg_old.0 * seg_old.0 + seg_old.1 * seg_old.1).sqrt();
        if len_new > 1e-12 && len_old > 1e-12 {
            let dot = (seg_new.0 * seg_old.0 + seg_new.1 * seg_old.1) / (len_new * len_old);
            if dot < -0.2 {
                return;
            }
        }
    }
    if let Some(last) = points.last() {
        if dist(*last, candidate) < 1e-9 {
            return;
        }
    }
    points.push(candidate);
}

/// 端帽圆弧的**内点**（不含两端 ✓）：从 `from` 绕 dab 中心经 `through_dir`
/// 一侧到 `to`，等角采样 10 段 ⇒ 返回 9 个内点 ✓。
fn cap_arc(dab: SvgDab, from: Pt, to: Pt, tangent: Pt, start: bool) -> Vec<Pt> {
    let center = (dab.x, dab.y);
    let a_from = (from.1 - center.1).atan2(from.0 - center.0);
    let a_to = (to.1 - center.1).atan2(to.0 - center.0);
    // 帽的朝向：起始帽朝行进反方向、终点帽朝行进方向 ✓。
    let through = if start {
        (-tangent.0, -tangent.1)
    } else {
        tangent
    };
    let a_through = through.1.atan2(through.0);
    // 选扫掠方向：从 a_from 出发、经过 a_through 附近到达 a_to 的那一条 ✓。
    let tau = std::f64::consts::TAU;
    let norm = |a: f64| {
        let mut a = a % tau;
        if a < 0.0 {
            a += tau;
        }
        a
    };
    let ccw_total = norm(a_to - a_from);
    let ccw_through = norm(a_through - a_from);
    let sweep = if ccw_through <= ccw_total {
        ccw_total
    } else {
        ccw_total - tau
    };
    let steps = 10;
    (1..steps)
        .map(|k| {
            let angle = a_from + sweep * (k as f64) / (steps as f64);
            (
                center.0 + dab.radius * angle.cos(),
                center.1 + dab.radius * angle.sin(),
            )
        })
        .collect()
}

/// 把一笔**单独**渲染进紧凑包围盒缓冲，取它的 alpha 场（路线 C 的原料 ✓）。
///
/// 直接走引擎自己的盖章路径（`stamp_stroke_configured_on` ✓）⇒ 场里包含
/// DabCache 掩膜量化等**引擎真实行为**，提的轮廓与实际渲染同源 ✓。
/// 返回 `(缓冲, 文档坐标包围盒 (x, y, w, h))`；笔完全在画布外时返回 `None` ✓。
pub fn render_stroke_buffer(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    canvas: (u32, u32),
) -> Option<(Buffer, (i64, i64, u32, u32))> {
    if geometry.points.is_empty() {
        return None;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in &geometry.points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    let pad = brush.size / 2.0 + 3.0;
    let x0 = ((min_x - pad).floor() as i64).max(0);
    let y0 = ((min_y - pad).floor() as i64).max(0);
    let x1 = ((max_x + pad).ceil() as i64).min(i64::from(canvas.0));
    let y1 = ((max_y + pad).ceil() as i64).min(i64::from(canvas.1));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (w, h) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let mut buffer = Buffer::new(x0, y0, w, h);
    crate::brush::stamp_stroke_configured_on(
        &mut buffer,
        brush,
        geometry,
        appearance,
        None,
        canvas,
    );
    Some((buffer, (x0, y0, w, h)))
}

/// 由 alpha 场提等值轮廓（**路线 C：亚像素 Marching Squares** ✓）。
///
/// `field` 是行主序的 alpha 值（节点 `(i, j)` 对应文档坐标
/// `(origin_x + i + 0.5, origin_y + j + 0.5)` 的像素中心采样 ✓）；
/// 场外补一圈 0 ⇒ 所有轮廓必然闭合 ✓。
/// 边交点按线性插值定位（亚像素 ✓）；鞍点（case 5/10）按中心值决定配对 ✓。
/// 返回的轮廓点列已平移到文档坐标、且按**包含深度**归一化了方向
/// （偶深度正面积、奇深度负面积 ⇒ `nonzero` 填充规则下孔洞成立 ✓）。
pub fn marching_squares(
    field: &[f32],
    width: usize,
    height: usize,
    origin: (i64, i64),
    threshold: f32,
) -> Vec<Vec<Pt>> {
    if width < 2 || height < 2 || field.len() < width * height {
        return Vec::new();
    }
    // 补零边：节点坐标整体 +1。
    let value = |i: usize, j: usize| -> f32 {
        if i == 0 || j == 0 || i > width || j > height {
            0.0
        } else {
            field[(j - 1) * width + (i - 1)]
        }
    };
    // 交点：横边 H(i,j) 在节点 (i,j)-(i+1,j) 上；竖边 V(i,j) 在 (i,j)-(i,j+1) 上。
    // 用 (is_vertical, i, j) 编号 ⇒ 相邻两格共享同一个交点编号、可直接链环 ✓。
    let cross_h = |i: usize, j: usize| -> Option<Pt> {
        let (a, b) = (value(i, j), value(i + 1, j));
        if (a < threshold) == (b < threshold) {
            return None;
        }
        let t = (f64::from(threshold - a) / f64::from(b - a)).clamp(0.0, 1.0);
        Some((i as f64 + t, j as f64))
    };
    let cross_v = |i: usize, j: usize| -> Option<Pt> {
        let (a, b) = (value(i, j), value(i, j + 1));
        if (a < threshold) == (b < threshold) {
            return None;
        }
        let t = (f64::from(threshold - a) / f64::from(b - a)).clamp(0.0, 1.0);
        Some((i as f64, j as f64 + t))
    };
    #[derive(Clone, Copy, PartialEq, Eq, Hash)]
    struct CrossId(u8, usize, usize); // 0 = 横边 H，1 = 竖边 V
    let mut segments: Vec<(CrossId, CrossId)> = Vec::new();
    let mut points: std::collections::HashMap<CrossId, Pt> = std::collections::HashMap::new();
    let register = |id: CrossId, pt: Pt, points: &mut std::collections::HashMap<CrossId, Pt>| {
        points.insert(id, pt);
    };
    for j in 0..height + 1 {
        for i in 0..width + 1 {
            let tl = value(i, j);
            let tr = value(i + 1, j);
            let br = value(i + 1, j + 1);
            let bl = value(i, j + 1);
            let mut case = 0u8;
            if tl >= threshold {
                case |= 1;
            }
            if tr >= threshold {
                case |= 2;
            }
            if br >= threshold {
                case |= 4;
            }
            if bl >= threshold {
                case |= 8;
            }
            if case == 0 || case == 15 {
                continue;
            }
            let top = CrossId(0, i, j);
            let right = CrossId(1, i + 1, j);
            let bottom = CrossId(0, i, j + 1);
            let left = CrossId(1, i, j);
            if let Some(p) = cross_h(i, j) {
                register(top, p, &mut points);
            }
            if let Some(p) = cross_v(i + 1, j) {
                register(right, p, &mut points);
            }
            if let Some(p) = cross_h(i, j + 1) {
                register(bottom, p, &mut points);
            }
            if let Some(p) = cross_v(i, j) {
                register(left, p, &mut points);
            }
            let mut add = |a: CrossId, b: CrossId| segments.push((a, b));
            match case {
                1 | 14 => add(left, top),
                2 | 13 => add(top, right),
                3 | 12 => add(left, right),
                4 | 11 => add(right, bottom),
                6 | 9 => add(top, bottom),
                7 | 8 => add(left, bottom),
                5 => {
                    let center = (tl + tr + br + bl) / 4.0;
                    if center >= threshold {
                        add(left, bottom);
                        add(top, right);
                    } else {
                        add(left, top);
                        add(right, bottom);
                    }
                }
                10 => {
                    let center = (tl + tr + br + bl) / 4.0;
                    if center >= threshold {
                        add(left, top);
                        add(right, bottom);
                    } else {
                        add(left, bottom);
                        add(top, right);
                    }
                }
                _ => {}
            }
        }
    }
    // 链环：每个交点恰有 2 条线段端点（补零边保证 ✓）⇒ 沿邻接表走圈 ✓。
    let mut adjacency: std::collections::HashMap<CrossId, Vec<CrossId>> =
        std::collections::HashMap::new();
    for (a, b) in &segments {
        adjacency.entry(*a).or_default().push(*b);
        adjacency.entry(*b).or_default().push(*a);
    }
    let mut visited: std::collections::HashSet<(CrossId, CrossId)> =
        std::collections::HashSet::new();
    let mut loops: Vec<Vec<Pt>> = Vec::new();
    for (a, b) in &segments {
        if visited.contains(&(*a, *b)) {
            continue;
        }
        let mut loop_pts: Vec<Pt> = Vec::new();
        let (mut prev, mut cur) = (*a, *b);
        loop_pts.push(points[&prev]);
        loop {
            visited.insert((prev, cur));
            visited.insert((cur, prev));
            loop_pts.push(points[&cur]);
            let next = adjacency[&cur].iter().find(|next| **next != prev).copied();
            match next {
                Some(next) if next != *a => {
                    prev = cur;
                    cur = next;
                }
                Some(next) => {
                    // 闭合段同样标 visited ⇒ 否则同一圈会被外层循环再走一遍 ✗。
                    visited.insert((cur, next));
                    visited.insert((next, cur));
                    break;
                }
                None => break,
            }
        }
        // 闭合点与首点重复 ⇒ 去掉末点 ✓。
        if loop_pts.len() > 1 && dist(loop_pts[0], loop_pts[loop_pts.len() - 1]) < 1e-9 {
            loop_pts.pop();
        }
        if loop_pts.len() >= 3 {
            // 平移到文档坐标：节点 (i, j) ↔ 文档 (origin + i − 1 + 0.5, …)（补零边 −1 ✓）。
            let offset = (origin.0 as f64 - 0.5, origin.1 as f64 - 0.5);
            for pt in &mut loop_pts {
                pt.0 += offset.0;
                pt.1 += offset.1;
            }
            loops.push(loop_pts);
        }
    }
    normalize_loop_orientations(&mut loops);
    loops
}

/// 按包含深度归一化轮廓方向：偶深度（外轮廓）正面积、奇深度（孔）负面积 ✓。
pub fn normalize_loop_orientations(loops: &mut [Vec<Pt>]) {
    let areas: Vec<f64> = loops.iter().map(|l| signed_area(l)).collect();
    let mut order: Vec<usize> = (0..loops.len()).collect();
    order.sort_by(|a, b| {
        areas[*b]
            .abs()
            .partial_cmp(&areas[*a].abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut depth = vec![0usize; loops.len()];
    for (pos, &idx) in order.iter().enumerate() {
        let probe = loops[idx][0];
        for &other in order.iter().take(pos) {
            if point_in_loop(probe, &loops[other]) {
                depth[idx] += 1;
            }
        }
    }
    for (idx, loop_pts) in loops.iter_mut().enumerate() {
        let want_positive = depth[idx] % 2 == 0;
        let area = signed_area(loop_pts);
        if (area > 0.0) != want_positive && area.abs() > 0.0 {
            loop_pts.reverse();
        }
    }
}

/// 点是否在闭合点列内（射线法 ✓；只用于包含深度判定，边界情形无害 ✓）。
pub fn point_in_loop(point: Pt, loop_pts: &[Pt]) -> bool {
    let mut inside = false;
    let n = loop_pts.len();
    for i in 0..n {
        let (x0, y0) = loop_pts[i];
        let (x1, y1) = loop_pts[(i + 1) % n];
        if (y0 > point.1) != (y1 > point.1) {
            let x_hit = x0 + (point.1 - y0) / (y1 - y0) * (x1 - x0);
            if point.0 < x_hit {
                inside = !inside;
            }
        }
    }
    inside
}

/// 一条三次 Bézier 段（起点隐含 = 上一段终点或轮廓起点 ✓）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cubic {
    /// 控制点 1。
    pub p1: Pt,
    /// 控制点 2。
    pub p2: Pt,
    /// 终点。
    pub p3: Pt,
}

/// 一条**闭合**拟合轮廓：`cubics` 依次相连，最后一段的终点回到 `start` ✓。
#[derive(Debug, Clone, PartialEq)]
pub struct FittedContour {
    /// 起点（= 末段终点 ✓）。
    pub start: Pt,
    /// 三次段列。
    pub cubics: Vec<Cubic>,
}

/// Ramer–Douglas–Peucker 简化（开链 ✓；容差 `eps` 单位像素）。
pub fn rdp(points: &[Pt], eps: f64) -> Vec<Pt> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0usize, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        if b <= a + 1 {
            continue;
        }
        let (pa, pb) = (points[a], points[b]);
        let ab = sub(pb, pa);
        let ab_len = (ab.0 * ab.0 + ab.1 * ab.1).sqrt();
        let mut max_d = 0.0;
        let mut max_i = a;
        for (i, point) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = if ab_len < 1e-12 {
                dist(*point, pa)
            } else {
                cross(ab, sub(*point, pa)).abs() / ab_len
            };
            if d > max_d {
                max_d = d;
                max_i = i;
            }
        }
        if max_d > eps {
            keep[max_i] = true;
            stack.push((a, max_i));
            stack.push((max_i, b));
        }
    }
    points
        .iter()
        .zip(keep.iter())
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

/// 把一条闭合轮廓点列拟合成三次 Bézier 轮廓（**导出管线的主拟合入口** ✓）。
///
/// 流程（专家 P0-2 ✓）：RDP 预抽稀（`ε=0.20px`）→ 转角 `> 55°` 的点与
/// `forced` 位置（taper 尖端等）作**强制分段点** → 逐段 Schneider 拟合
/// （最大误差 `0.6px`、Newton 重参数化 ≤ 4 轮 ✓）。
/// 起点取离质心最远的轮廓点 ⇒ 输出稳定（确定性 ✓）。
pub fn fit_closed_contour(loop_pts: &[Pt], forced: &[Pt]) -> Option<FittedContour> {
    if loop_pts.len() < 3 {
        return None;
    }
    // 起点：离质心最远（轮廓的极值点，稳定且通常在形状端部 ✓）。
    let centroid = (
        loop_pts.iter().map(|p| p.0).sum::<f64>() / loop_pts.len() as f64,
        loop_pts.iter().map(|p| p.1).sum::<f64>() / loop_pts.len() as f64,
    );
    let mut start_idx = 0usize;
    let mut best = -1.0;
    for (i, p) in loop_pts.iter().enumerate() {
        let d = dist(*p, centroid);
        if d > best {
            best = d;
            start_idx = i;
        }
    }
    // 旋转成开链并在末尾重复起点 ⇒ 闭合拟合 ✓。
    let mut chain: Vec<Pt> = Vec::with_capacity(loop_pts.len() + 1);
    for k in 0..loop_pts.len() {
        chain.push(loop_pts[(start_idx + k) % loop_pts.len()]);
    }
    chain.push(chain[0]);
    let simplified = rdp(&chain, 0.20);
    if simplified.len() < 2 {
        return None;
    }
    // 分段点：转角 > 55° 的内点 + forced 位置的最近点 ✓（首尾恒为分段点 ✓）。
    let mut split = vec![false; simplified.len()];
    split[0] = true;
    split[simplified.len() - 1] = true;
    for i in 1..simplified.len() - 1 {
        let v1 = sub(simplified[i], simplified[i - 1]);
        let v2 = sub(simplified[i + 1], simplified[i]);
        let l1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
        let l2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();
        if l1 > 1e-12 && l2 > 1e-12 {
            let cos = ((v1.0 * v2.0 + v1.1 * v2.1) / (l1 * l2)).clamp(-1.0, 1.0);
            if cos.acos() > 55.0f64.to_radians() {
                split[i] = true;
            }
        }
    }
    for target in forced {
        let mut nearest = 0usize;
        let mut best = f64::INFINITY;
        for (i, p) in simplified.iter().enumerate() {
            let d = dist(*p, *target);
            if d < best {
                best = d;
                nearest = i;
            }
        }
        if nearest > 0 && nearest < simplified.len() - 1 {
            split[nearest] = true;
        }
    }
    let bounds: Vec<usize> = (0..simplified.len()).filter(|i| split[*i]).collect();
    let mut cubics: Vec<Cubic> = Vec::new();
    for pair in bounds.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let segment = &simplified[a..=b];
        cubics.extend(fit_chain(segment, 0.6));
    }
    if cubics.is_empty() {
        return None;
    }
    Some(FittedContour {
        start: simplified[0],
        cubics,
    })
}

/// Schneider 三次拟合（开链 ✓）：误差超限就在最大误差点劈开递归 ✓。
fn fit_chain(points: &[Pt], max_error: f64) -> Vec<Cubic> {
    if points.len() < 2 {
        return Vec::new();
    }
    if points.len() == 2 {
        return vec![line_cubic(points[0], points[1])];
    }
    let mut params = chord_params(points);
    let mut bezier = fit_once(points, &params);
    for _ in 0..4 {
        let (err, _) = max_error_of(points, &params, &bezier);
        if err <= max_error {
            break;
        }
        params = reparameterize(points, &params, &bezier);
        bezier = fit_once(points, &params);
    }
    let (err, split_at) = max_error_of(points, &params, &bezier);
    if err <= max_error || split_at == 0 || split_at >= points.len() - 1 {
        return vec![bezier];
    }
    let mut out = fit_chain(&points[..=split_at], max_error);
    out.extend(fit_chain(&points[split_at..], max_error));
    out
}

/// 直线段的等价三次（控制点在 1/3、2/3 处 ✓）。
fn line_cubic(p0: Pt, p3: Pt) -> Cubic {
    Cubic {
        p1: (p0.0 + (p3.0 - p0.0) / 3.0, p0.1 + (p3.1 - p0.1) / 3.0),
        p2: (
            p0.0 + 2.0 * (p3.0 - p0.0) / 3.0,
            p0.1 + 2.0 * (p3.1 - p0.1) / 3.0,
        ),
        p3,
    }
}

/// 弦长参数化。
fn chord_params(points: &[Pt]) -> Vec<f64> {
    let mut params = Vec::with_capacity(points.len());
    params.push(0.0);
    for i in 1..points.len() {
        params.push(params[i - 1] + dist(points[i], points[i - 1]));
    }
    let total = *params.last().unwrap_or(&1.0);
    if total > 0.0 {
        for p in &mut params {
            *p /= total;
        }
    }
    params
}

fn cubic_at(p0: Pt, c: &Cubic, t: f64) -> Pt {
    let mt = 1.0 - t;
    let a = mt * mt * mt;
    let b = 3.0 * mt * mt * t;
    let cc = 3.0 * mt * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * c.p1.0 + cc * c.p2.0 + d * c.p3.0,
        a * p0.1 + b * c.p1.1 + cc * c.p2.1 + d * c.p3.1,
    )
}

fn cubic_deriv1(p0: Pt, c: &Cubic, t: f64) -> Pt {
    let mt = 1.0 - t;
    (
        3.0 * mt * mt * (c.p1.0 - p0.0)
            + 6.0 * mt * t * (c.p2.0 - c.p1.0)
            + 3.0 * t * t * (c.p3.0 - c.p2.0),
        3.0 * mt * mt * (c.p1.1 - p0.1)
            + 6.0 * mt * t * (c.p2.1 - c.p1.1)
            + 3.0 * t * t * (c.p3.1 - c.p2.1),
    )
}

fn cubic_deriv2(p0: Pt, c: &Cubic, t: f64) -> Pt {
    let mt = 1.0 - t;
    (
        6.0 * mt * (c.p2.0 - 2.0 * c.p1.0 + p0.0) + 6.0 * t * (c.p3.0 - 2.0 * c.p2.0 + c.p1.0),
        6.0 * mt * (c.p2.1 - 2.0 * c.p1.1 + p0.1) + 6.0 * t * (c.p3.1 - 2.0 * c.p2.1 + c.p1.1),
    )
}

/// 一次最小二乘拟合（端点固定、端点切向取首尾弦向 ✓，Schneider 原法 ✓）。
fn fit_once(points: &[Pt], params: &[f64]) -> Cubic {
    let p0 = points[0];
    let p3 = points[points.len() - 1];
    let t_hat1 = {
        let d = sub(points[1], p0);
        let l = (d.0 * d.0 + d.1 * d.1).sqrt();
        if l < 1e-12 {
            (1.0, 0.0)
        } else {
            (d.0 / l, d.1 / l)
        }
    };
    let t_hat2 = {
        let d = sub(p3, points[points.len() - 2]);
        let l = (d.0 * d.0 + d.1 * d.1).sqrt();
        if l < 1e-12 {
            (1.0, 0.0)
        } else {
            (d.0 / l, d.1 / l)
        }
    };
    let mut c00 = 0.0;
    let mut c01 = 0.0;
    let mut c11 = 0.0;
    let mut x0 = 0.0;
    let mut x1 = 0.0;
    for (i, &t) in params.iter().enumerate() {
        let mt = 1.0 - t;
        let b1 = 3.0 * mt * mt * t;
        let b2 = 3.0 * mt * t * t;
        let a1 = (t_hat1.0 * b1, t_hat1.1 * b1);
        let a2 = (-t_hat2.0 * b2, -t_hat2.1 * b2);
        c00 += a1.0 * a1.0 + a1.1 * a1.1;
        c01 += a1.0 * a2.0 + a1.1 * a2.1;
        c11 += a2.0 * a2.0 + a2.1 * a2.1;
        // 目标点扣掉端点基底的贡献 ✓。
        let base = (
            (mt * mt * mt + 3.0 * mt * mt * t) * p0.0 + (3.0 * mt * t * t + t * t * t) * p3.0,
            (mt * mt * mt + 3.0 * mt * mt * t) * p0.1 + (3.0 * mt * t * t + t * t * t) * p3.1,
        );
        let tmp = sub(points[i], base);
        x0 += a1.0 * tmp.0 + a1.1 * tmp.1;
        x1 += a2.0 * tmp.0 + a2.1 * tmp.1;
    }
    let det = c00 * c11 - c01 * c01;
    let chord = dist(p0, p3);
    let (alpha1, alpha2) = if det.abs() < 1e-12 {
        (chord / 3.0, chord / 3.0)
    } else {
        let a1 = (x0 * c11 - x1 * c01) / det;
        let a2 = (c00 * x1 - c01 * x0) / det;
        // 负长度/过大长度 ⇒ 回退到弦长三分之一（Schneider 的钳制 ✓）。
        let fallback = chord / 3.0;
        (
            if a1 <= 0.0 || a1 > chord * 3.0 {
                fallback
            } else {
                a1
            },
            if a2 <= 0.0 || a2 > chord * 3.0 {
                fallback
            } else {
                a2
            },
        )
    };
    Cubic {
        p1: (p0.0 + t_hat1.0 * alpha1, p0.1 + t_hat1.1 * alpha1),
        p2: (p3.0 - t_hat2.0 * alpha2, p3.1 - t_hat2.1 * alpha2),
        p3,
    }
}

/// 当前拟合的最大误差（平方距离）与最差点下标。
fn max_error_of(points: &[Pt], params: &[f64], bezier: &Cubic) -> (f64, usize) {
    let p0 = points[0];
    let mut max_d2 = 0.0;
    let mut split_at = 0usize;
    for (i, &t) in params.iter().enumerate() {
        let q = cubic_at(p0, bezier, t);
        let d = sub(points[i], q);
        let d2 = d.0 * d.0 + d.1 * d.1;
        if d2 > max_d2 {
            max_d2 = d2;
            split_at = i;
        }
    }
    (max_d2.sqrt(), split_at)
}

/// Newton 重参数化（一步 ✓；Schneider 原法 ✓）。
fn reparameterize(points: &[Pt], params: &[f64], bezier: &Cubic) -> Vec<f64> {
    let p0 = points[0];
    params
        .iter()
        .zip(points.iter())
        .map(|(&t, &target)| {
            let q = cubic_at(p0, bezier, t);
            let d1 = cubic_deriv1(p0, bezier, t);
            let d2 = cubic_deriv2(p0, bezier, t);
            let diff = sub(q, target);
            let numerator = diff.0 * d1.0 + diff.1 * d1.1;
            let denominator = d1.0 * d1.0 + d1.1 * d1.1 + diff.0 * d2.0 + diff.1 * d2.1;
            if denominator.abs() < 1e-12 {
                t
            } else {
                (t - numerator / denominator).clamp(0.0, 1.0)
            }
        })
        .collect()
}

/// 把拟合轮廓展平成点列（每段均匀 16 分 ⇒ 度量/光栅化用 ✓，确定性 ✓）。
pub fn flatten_contour(contour: &FittedContour) -> Vec<Pt> {
    let mut out = Vec::with_capacity(contour.cubics.len() * 16 + 1);
    let mut p0 = contour.start;
    out.push(p0);
    for cubic in &contour.cubics {
        for k in 1..=16 {
            out.push(cubic_at(p0, cubic, f64::from(k) / 16.0));
        }
        p0 = cubic.p3;
    }
    // 末点与起点重合 ⇒ 去掉（闭合点列不重复首点 ✓）。
    if out.len() > 1 && dist(out[0], out[out.len() - 1]) < 1e-9 {
        out.pop();
    }
    out
}

/// 一组闭合轮廓 → SVG `d` 属性（相对命令 + 同命令字母省略 ✓；
/// 坐标全走 [`fmt_num`] 定点格式化 ⇒ 字节确定 ✓）。
pub fn contours_path_data(contours: &[FittedContour]) -> String {
    let mut out = String::new();
    for contour in contours {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("M ");
        out.push_str(&fmt_num(contour.start.0));
        out.push(' ');
        out.push_str(&fmt_num(contour.start.1));
        let mut current = contour.start;
        let mut first_cubic = true;
        for cubic in &contour.cubics {
            out.push(' ');
            if first_cubic {
                out.push_str("c ");
                first_cubic = false;
            }
            // SVG 相对 `c` 的三个点**全部**相对当前点（段起点）✓。
            // 教训（2026-10-20 端到端独立渲染器实测抓到）：曾写成链式增量
            // （p2 相对 p1、p3 相对 p2）⇒ 段终点靠望远镜求和偶然正确、
            // 控制点全错 ⇒ 形状似是而非、resvg/cairosvg 双双 E≈0.9；
            // 而内核 validate 验的是轮廓数据、绕过文本 ⇒ 没抓到。
            // 往返单测 `path_data_round_trips_geometry` 守住这条 ✓。
            let deltas = [
                (cubic.p1.0 - current.0, cubic.p1.1 - current.1),
                (cubic.p2.0 - current.0, cubic.p2.1 - current.1),
                (cubic.p3.0 - current.0, cubic.p3.1 - current.1),
            ];
            let mut first_token = true;
            for (dx, dy) in deltas {
                if !first_token {
                    out.push(' ');
                }
                first_token = false;
                out.push_str(&fmt_num(dx));
                out.push(' ');
                out.push_str(&fmt_num(dy));
            }
            current = cubic.p3;
        }
        out.push_str(" z");
    }
    out
}

/// 一层待光栅化的填充：（展平轮廓组、直通线性 RGB、该层不透明度 ✓）。
pub type FillLayer<'a> = (&'a [Vec<Pt>], [f32; 3], f64);

/// 预乘 RGBA 浮点图（度量与光栅化用 ✓；坐标 = 文档坐标 − `origin` ✓）。
#[derive(Debug, Clone)]
pub struct PremultImage {
    /// 包围盒左上角（文档坐标）。
    pub origin: (i64, i64),
    /// 宽（像素）。
    pub width: usize,
    /// 高（像素）。
    pub height: usize,
    /// 行主序预乘像素。
    pub rgba: Vec<[f32; 4]>,
}

/// 引擎缓冲 → [`PremultImage`]（原样搬运浮点像素 ✓）。
pub fn buffer_to_premult(buffer: &Buffer) -> PremultImage {
    let (ox, oy) = buffer.origin();
    let (w, h) = (buffer.width() as usize, buffer.height() as usize);
    let mut rgba = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            rgba.push(buffer.pixel(x as u32, y as u32));
        }
    }
    PremultImage {
        origin: (ox, oy),
        width: w,
        height: h,
        rgba,
    }
}

/// **解析覆盖率光栅化**（验收度量专用 ✓）：把若干填充层
/// （展平轮廓组 + 直通色 + 不透明度）按序 source-over 合成。
///
/// 覆盖率算法（梯形有向面积法的等价实现 ✓）：非边界像素用中心点绕数判定
/// （0 或满覆盖）；边界像素把每个轮廓**裁剪到像素方格**后取鞋带面积求和
/// ⇒ 亚像素级精确、天然支持自交与孔洞（`nonzero` 语义 ✓）。
pub fn rasterize_filled(
    layers: &[FillLayer<'_>],
    origin: (i64, i64),
    width: usize,
    height: usize,
) -> PremultImage {
    let mut out = PremultImage {
        origin,
        width,
        height,
        rgba: vec![[0.0; 4]; width * height],
    };
    let (ox, oy) = (origin.0 as f64, origin.1 as f64);
    for (loops, rgb, alpha) in layers {
        if *alpha <= 0.0 || loops.is_empty() {
            continue;
        }
        // 平移到图内坐标 ✓。
        let local: Vec<Vec<Pt>> = loops
            .iter()
            .map(|l| l.iter().map(|p| (p.0 - ox, p.1 - oy)).collect())
            .collect();
        // 边界像素集合（任一边的包围盒扫过的像素 ✓，保守取大 ⇒ 只影响速度 ✓）。
        let mut boundary: std::collections::HashSet<(i32, i32)> = std::collections::HashSet::new();
        for loop_pts in &local {
            let n = loop_pts.len();
            for i in 0..n {
                let a = loop_pts[i];
                let b = loop_pts[(i + 1) % n];
                let x0 = (a.0.min(b.0).floor() as i32).max(0);
                let x1 = (a.0.max(b.0).ceil() as i32).min(width as i32 - 1);
                let y0 = (a.1.min(b.1).floor() as i32).max(0);
                let y1 = (a.1.max(b.1).ceil() as i32).min(height as i32 - 1);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        boundary.insert((x, y));
                    }
                }
            }
        }
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let coverage = if boundary.contains(&(x, y)) {
                    let mut sum = 0.0;
                    for loop_pts in &local {
                        sum += clipped_signed_area(loop_pts, f64::from(x), f64::from(y));
                    }
                    sum.abs().clamp(0.0, 1.0)
                } else {
                    let wn = winding_number((f64::from(x) + 0.5, f64::from(y) + 0.5), &local);
                    if wn != 0 {
                        1.0
                    } else {
                        0.0
                    }
                };
                if coverage <= 0.0 {
                    continue;
                }
                let a = (coverage * alpha) as f32;
                let idx = y as usize * width + x as usize;
                let dst = out.rgba[idx];
                let inv = 1.0 - a;
                out.rgba[idx] = [
                    rgb[0] * a + dst[0] * inv,
                    rgb[1] * a + dst[1] * inv,
                    rgb[2] * a + dst[2] * inv,
                    a + dst[3] * inv,
                ];
            }
        }
    }
    out
}

/// 点对一组轮廓的绕数（非零填充判定 ✓）。
fn winding_number(point: Pt, loops: &[Vec<Pt>]) -> i32 {
    let mut wn = 0i32;
    for loop_pts in loops {
        let n = loop_pts.len();
        for i in 0..n {
            let a = loop_pts[i];
            let b = loop_pts[(i + 1) % n];
            if a.1 <= point.1 {
                if b.1 > point.1 && cross(sub(b, a), sub(point, a)) > 0.0 {
                    wn += 1;
                }
            } else if b.1 <= point.1 && cross(sub(b, a), sub(point, a)) < 0.0 {
                wn -= 1;
            }
        }
    }
    wn
}

/// 轮廓裁剪到像素方格 `[px, px+1] × [py, py+1]` 后的有向面积
/// （Sutherland–Hodgman ✓；整像素被包含时恰为 ±1 ✓）。
fn clipped_signed_area(loop_pts: &[Pt], px: f64, py: f64) -> f64 {
    let mut poly: Vec<Pt> = loop_pts.to_vec();
    // 四条边依次裁：x ≥ px、x ≤ px+1、y ≥ py、y ≤ py+1。
    for (axis, bound, keep_greater) in [
        (0usize, px, true),
        (0, px + 1.0, false),
        (1, py, true),
        (1, py + 1.0, false),
    ] {
        if poly.is_empty() {
            break;
        }
        let coord = |p: Pt| if axis == 0 { p.0 } else { p.1 };
        let inside = |p: Pt| {
            if keep_greater {
                coord(p) >= bound
            } else {
                coord(p) <= bound
            }
        };
        let mut next: Vec<Pt> = Vec::with_capacity(poly.len() + 1);
        let n = poly.len();
        for i in 0..n {
            let cur = poly[i];
            let prev = poly[(i + n - 1) % n];
            let (cur_in, prev_in) = (inside(cur), inside(prev));
            if cur_in != prev_in {
                let t = (bound - coord(prev)) / (coord(cur) - coord(prev));
                next.push((prev.0 + (cur.0 - prev.0) * t, prev.1 + (cur.1 - prev.1) * t));
            }
            if cur_in {
                next.push(cur);
            }
        }
        poly = next;
    }
    if poly.len() < 3 {
        return 0.0;
    }
    signed_area(&poly)
}

/// 一笔的验收度量（专家 P0-实2 ✓）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeMetrics {
    /// 内部区域误差（非边缘像素上预乘 RGBA 最大通道差的 P95 ✓）。
    pub e_interior: f64,
    /// 几何误差（alpha 对称差面积比 ✓）。
    pub e_geom: f64,
    /// 参与 `e_interior` 统计的内域像素数（**防真空** ✓：边缘带把全图吃光时
    /// 这个数会暴露 ⇒ `e_interior = 0` 不再能冒充「完美」✗）。
    pub interior_px: usize,
}

impl StrokeMetrics {
    /// 合格线（专家给定 ✓）：`E_interior ≤ 0.12` 且 `E_geom ≤ 0.15`。
    pub fn passes(&self) -> bool {
        self.e_interior <= 0.12 && self.e_geom <= 0.15
    }
}

/// 比较参考图（引擎渲染）与候选图（SVG 重光栅化），算验收度量 ✓。
///
/// **为什么不用逐像素 MSE** ✗（专家警告 ✓）：细线偏移 0.3px 就会在边缘
/// 产生巨大的逐像素差、把好拟合误判成坏 ⇒ 边缘像素从内部误差里剔除，
/// 几何偏差单独由 `E_geom` 承担 ✓。**边缘的定义**：任一图中 alpha 在
/// 3×3 邻域内跳变 `> 0.05` 的像素、再膨胀一圈（≈ ±1.5px ✓）——阈值必须
/// 低到能抓住**多级 alpha 爬坡**（半透明笔的累积台阶可低至 1/16 ✓）：
/// 实测教训：阈值 0.2 时台阶 0.1875 的爬坡带漏进内域 ⇒ 一支几何正确的
/// 双层笔被误判 `E_interior = 0.1875` 回退（2026-10-20 端到端冒烟实测 ✓）。
/// 两图尺寸/原点必须一致（调用方保证 ✓）。
pub fn stroke_metrics(reference: &PremultImage, candidate: &PremultImage) -> StrokeMetrics {
    debug_assert_eq!(reference.width, candidate.width);
    debug_assert_eq!(reference.height, candidate.height);
    let (w, h) = (reference.width, reference.height);
    let alpha = |img: &PremultImage, x: usize, y: usize| f64::from(img.rgba[y * w + x][3]);
    let mut edge = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut edgy = false;
            for img in [reference, candidate] {
                let center = alpha(img, x, y);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        if (alpha(img, nx as usize, ny as usize) - center).abs() > 0.05 {
                            edgy = true;
                        }
                    }
                }
            }
            edge[y * w + x] = edgy;
        }
    }
    // 膨胀一圈（±1.5px 的近似 ✓）。
    let edge_wide = edge.clone();
    for y in 0..h {
        for x in 0..w {
            if !edge_wide[y * w + x] {
                continue;
            }
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                        edge[ny as usize * w + nx as usize] = true;
                    }
                }
            }
        }
    }
    let mut errors: Vec<f64> = Vec::new();
    let mut geom_num = 0.0f64;
    let mut geom_den = 0.0f64;
    for (i, (ref_px, cand_px)) in reference.rgba.iter().zip(candidate.rgba.iter()).enumerate() {
        let ra = f64::from(ref_px[3]);
        let ca = f64::from(cand_px[3]);
        geom_num += (ra - ca).abs();
        geom_den += ra.max(ca);
        if ra.max(ca) > 0.05 && !edge[i] {
            let mut err = (ra - ca).abs();
            for ch in 0..3 {
                err = err.max(f64::from((ref_px[ch] - cand_px[ch]).abs()));
            }
            errors.push(err);
        }
    }
    let e_interior = if errors.is_empty() {
        0.0
    } else {
        errors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((errors.len() as f64 * 0.95).ceil() as usize).clamp(1, errors.len()) - 1;
        errors[idx]
    };
    let e_geom = if geom_den <= 0.0 {
        0.0
    } else {
        geom_num / geom_den
    };
    StrokeMetrics {
        e_interior,
        e_geom,
        interior_px: errors.len(),
    }
}

/// 对一笔已生成的矢量层做**事后验收**：引擎单笔渲染 vs SVG 重光栅化 ✓。
///
/// `layers` 是 `(拟合轮廓组, 该层 fill-opacity)` ✓（双层时两项 ✓）。
/// 笔在画布外（渲染不出参考图）时返回 `None` ⇒ 调用方按「无法验收」处理 ✓。
pub fn validate_stroke(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    canvas: (u32, u32),
    layers: &[(&[FittedContour], f64)],
) -> Option<StrokeMetrics> {
    let (buffer, (x0, y0, w, h)) = render_stroke_buffer(brush, geometry, appearance, canvas)?;
    let reference = buffer_to_premult(&buffer);
    let (w, h) = (w as usize, h as usize);
    let flat: Vec<(Vec<Vec<Pt>>, f64)> = layers
        .iter()
        .map(|(contours, alpha)| {
            (
                contours.iter().map(flatten_contour).collect::<Vec<_>>(),
                *alpha,
            )
        })
        .collect();
    let rgb = [brush.color[0], brush.color[1], brush.color[2]];
    let layer_refs: Vec<FillLayer<'_>> = flat
        .iter()
        .map(|(loops, alpha)| (loops.as_slice(), rgb, *alpha))
        .collect();
    let candidate = rasterize_filled(&layer_refs, (x0, y0), w, h);
    Some(stroke_metrics(&reference, &candidate))
}

/// 一层矢量填充（轮廓组 + 该层的不透明度 ✓；双层近似时两层同色叠加 ✓）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLayer {
    /// 拟合轮廓组（同层多轮廓 = 一条 `<path>` 的多个子路径 ✓）。
    pub contours: Vec<FittedContour>,
    /// `fill-opacity`（已含 α_eff 累积 ✓）。
    pub fill_opacity: f64,
}

/// 一笔的导出判定结果。
#[derive(Debug, Clone, PartialEq)]
pub enum StrokeExport {
    /// 转矢量（`layers` 为空 = 这一笔在画布上没有可见墨迹 ✓）。
    Vector {
        /// 填充层（通常一层；半透明自交时两层 ✓）。
        layers: Vec<ExportLayer>,
        /// CSS 混合模式名（`normal` 时调用方可以省略 ✓）。
        css_blend: Option<&'static str>,
    },
    /// 回退位图（`reason` 是机器可读的短码 ⇒ 工具结果里如实计数、不静默 ✓）。
    Raster {
        /// 回退原因短码。
        reason: &'static str,
    },
}

/// 笔触的脊线点列（与盖章同源：`smooth` 时是平滑后的采样 ✓）。
pub fn stroke_spine(geometry: &StrokeGeometry) -> Vec<Pt> {
    if geometry.smooth {
        crate::brush::catmull_rom_smooth(&geometry.points, 8)
            .iter()
            .map(|p| (p.x, p.y))
            .collect()
    } else {
        geometry.points.iter().map(|p| (p.x, p.y)).collect()
    }
}

/// taper 尖端位置（起收笔处半径 `< 1.5px` 的极值点 ⇒ 拟合的强制分段点 ✓，
/// 防止三次拟合把锋利的笔锋抹圆（专家 P0-2 ✓））。
fn taper_tips(clusters: &[Vec<SvgDab>]) -> Vec<Pt> {
    let mut tips = Vec::new();
    for cluster in clusters {
        if cluster.len() >= 2 {
            let (a, b) = (cluster[0], cluster[1]);
            if a.radius < 1.5 {
                let d = (b.x - a.x, b.y - a.y);
                let len = (d.0 * d.0 + d.1 * d.1).sqrt();
                if len > 1e-12 {
                    tips.push((a.x - d.0 / len * a.radius, a.y - d.1 / len * a.radius));
                }
            }
            let n = cluster.len();
            let (a, b) = (cluster[n - 1], cluster[n - 2]);
            if a.radius < 1.5 {
                let d = (a.x - b.x, a.y - b.y);
                let len = (d.0 * d.0 + d.1 * d.1).sqrt();
                if len > 1e-12 {
                    tips.push((a.x + d.0 / len * a.radius, a.y + d.1 / len * a.radius));
                }
            }
        } else if cluster.len() == 1 && cluster[0].radius < 1.5 {
            tips.push((cluster[0].x, cluster[0].y));
        }
    }
    tips
}

/// 一组点列轮廓 → 拟合轮廓（拟合失败的单个轮廓丢弃 ⇒ 计数由调用方的
/// 度量验收兜底，不在这里静默吞整笔 ✓）。
fn fit_loops(loops: &[Vec<Pt>], forced: &[Pt]) -> Vec<FittedContour> {
    loops
        .iter()
        .filter_map(|l| fit_closed_contour(l, forced))
        .collect()
}

/// 由引擎 alpha 场在给定阈值提轮廓并拟合（路线 C 与双层的共用件 ✓）。
fn field_contours(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    canvas: (u32, u32),
    threshold_ratio: f64,
    alpha_eff: f64,
    forced: &[Pt],
) -> Option<Vec<FittedContour>> {
    field_contours_abs(
        brush,
        geometry,
        appearance,
        canvas,
        threshold_ratio * alpha_eff,
        forced,
    )
}

/// 按**绝对 alpha 阈值**从引擎实渲的 alpha 场提轮廓 ✓。
fn field_contours_abs(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    appearance: Option<&StrokeAppearance>,
    canvas: (u32, u32),
    threshold_abs: f64,
    forced: &[Pt],
) -> Option<Vec<FittedContour>> {
    let (buffer, (x0, y0, w, h)) = render_stroke_buffer(brush, geometry, appearance, canvas)?;
    let field: Vec<f32> = buffer_to_premult(&buffer)
        .rgba
        .iter()
        .map(|px| px[3])
        .collect();
    let loops = marching_squares(
        &field,
        w as usize,
        h as usize,
        (x0, y0),
        threshold_abs as f32,
    );
    Some(fit_loops(&loops, forced))
}

/// **一笔的导出主入口**：分类（专家 P1-3 ✓）→ 轮廓提取（A 主 C 兜底 ✓）
/// → 简化拟合 ⇒ [`StrokeExport`]。
///
/// 分类顺序即判定函数：外观（曲线/动力学/纹理/载墨）→ 不可映射混合 →
/// 软边三档 → 极淡反复涂抹 ⇒ 位图；其余按脊线形态选 A/C、
/// 半透明自交走双等值层（专家 P0-实1 ✓）。
pub fn export_stroke(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    appearance: &StrokeAppearance,
    canvas: (u32, u32),
) -> StrokeExport {
    if appearance.active {
        return StrokeExport::Raster {
            reason: "appearance",
        };
    }
    let Some(css_blend) = blend_css(brush.blend_mode) else {
        return StrokeExport::Raster {
            reason: "blend_mode",
        };
    };
    let css_blend = Some(css_blend);
    let clusters = stroke_dab_clusters(brush, geometry, canvas);
    let dab_count: usize = clusters.iter().map(Vec::len).sum();
    if dab_count == 0 {
        return StrokeExport::Vector {
            layers: Vec::new(),
            css_blend,
        };
    }
    let r_max = clusters
        .iter()
        .flatten()
        .map(|d| d.radius)
        .fold(0.0f64, f64::max);
    // 软边判据（专家 P1-1 ✓）：fringe = r_max × (1 − hardness)；
    // hardness ≥ 0.75 或 fringe ≤ 2px 走硬轮廓；2–5px 中间档默认回退
    // （专家的双层同心近似列为可选进阶，本期不做 ⇒ 如实回退 ✓）；
    // hardness < 0.45 或 fringe > 5px 必须回退 ✓。
    let fringe = r_max * (1.0 - brush.hardness);
    if brush.hardness < 0.75 && fringe > 2.0 {
        return StrokeExport::Raster {
            reason: "soft_edge",
        };
    }
    let spine = stroke_spine(geometry);
    let crossings = spine_self_intersections(&spine);
    let segments = spine.len().saturating_sub(1).max(1);
    let crossing_ratio = crossings as f64 / segments as f64;
    let alpha_eff = effective_alpha(brush);
    // 极低不透明度 + 反复自交涂抹 ⇒ 矢量分层会出等高线伪影 ⇒ 回退（专家 P1-3 第 6 条 ✓）。
    if alpha_eff < 0.25 && crossing_ratio > 0.35 {
        return StrokeExport::Raster {
            reason: "faint_scribble",
        };
    }
    let forced = taper_tips(&clusters);
    // 半透明自交：双等值层（底层 T1 = 0.5·α_eff、加深层只在自交区出现
    // ⇒ 还原二次叠色；专家 P0-实1，层数限死 2 ✓）。
    // **T2 的系数按几何改了** ✗：专家给 `1.35·α_eff`，但双 pass 交叉区的
    // alpha 上限是 `1−(1−α_eff)²`，`α_eff > 0.63` 时 `1.35·α_eff` 已超过
    // 它（`α_eff = 0.75` 时 T2 = 1.01 > 1）⇒ 阈值永远达不到、深层恒空 ✗。
    // 改取单 pass 平台与双 pass 平台的中点：`T2 = α_eff·(3 − α_eff)/2` ✓
    // （`α_eff = 0.5` 时 0.625、与专家的 0.675 同量级 ✓）。
    // 半透明多级等值层（2026-10-20 独立渲染器实测后升级 ✓）：
    // 单层/双层模型把**边缘累积环**（真 alpha 只有单 dab 的 `o`、不到平台
    // `α_eff`）整片按平台涂 ⇒ 整文档 E_geom 实测 0.22（专家线 0.15 ✗）。
    // 改按场的三级平台逐级建层（三级处处精确、合成恒等式可验 ✓）：
    //   环层：阈值 o/2、opacity = o（外轮廓 = 单 dab 覆盖区 ✓）
    //   平台层：阈值 (o+α)/2、opacity = (α−o)/(1−o)（环⊕平台 = α ✓）
    //   自交深层：阈值 (α+D)/2、opacity = α（平台⊕深 = D = 1−(1−α)² ✓）
    // 层数 ≤ 3。`α − o ≤ 0.05`（环可忽略）或 K = 1（无环）时不走这里，
    // 落回下面的双层/单层路 ✓。这是对专家「双层」方案的**实测驱动偏离**，
    // 理由与数字写在补丁 README 里 ✓。
    let dab_alpha = brush.opacity.clamp(0.0, 1.0);
    if alpha_eff < 0.999 && alpha_eff - dab_alpha > 0.05 {
        let deep_alpha = 1.0 - (1.0 - alpha_eff) * (1.0 - alpha_eff);
        let ring = field_contours_abs(
            brush,
            geometry,
            Some(appearance),
            canvas,
            dab_alpha / 2.0,
            &forced,
        );
        match ring {
            Some(ring) if !ring.is_empty() => {
                let mut layers = vec![ExportLayer {
                    contours: ring,
                    fill_opacity: dab_alpha,
                }];
                let plateau_opacity = (alpha_eff - dab_alpha) / (1.0 - dab_alpha).max(1e-9);
                if let Some(plateau) = field_contours_abs(
                    brush,
                    geometry,
                    Some(appearance),
                    canvas,
                    (dab_alpha + alpha_eff) / 2.0,
                    &forced,
                ) {
                    if !plateau.is_empty() {
                        layers.push(ExportLayer {
                            contours: plateau,
                            fill_opacity: plateau_opacity,
                        });
                    }
                }
                if crossings >= 1 {
                    if let Some(deep) = field_contours_abs(
                        brush,
                        geometry,
                        Some(appearance),
                        canvas,
                        (alpha_eff + deep_alpha) / 2.0,
                        &forced,
                    ) {
                        if !deep.is_empty() {
                            layers.push(ExportLayer {
                                contours: deep,
                                fill_opacity: alpha_eff,
                            });
                        }
                    }
                }
                return StrokeExport::Vector { layers, css_blend };
            }
            _ => {
                return StrokeExport::Raster {
                    reason: "field_empty",
                };
            }
        }
    }
    if (0.25..0.85).contains(&alpha_eff) && crossings >= 1 {
        let t2_ratio = (3.0 - alpha_eff) / 2.0;
        let base = field_contours(
            brush,
            geometry,
            Some(appearance),
            canvas,
            0.5,
            alpha_eff,
            &forced,
        );
        let deep = field_contours(
            brush,
            geometry,
            Some(appearance),
            canvas,
            t2_ratio,
            alpha_eff,
            &forced,
        );
        if let Some(base) = base {
            if base.is_empty() {
                return StrokeExport::Raster {
                    reason: "field_empty",
                };
            }
            let mut layers = vec![ExportLayer {
                contours: base,
                fill_opacity: alpha_eff,
            }];
            if let Some(deep) = deep {
                if !deep.is_empty() {
                    layers.push(ExportLayer {
                        contours: deep,
                        fill_opacity: alpha_eff,
                    });
                }
            }
            return StrokeExport::Vector { layers, css_blend };
        }
        return StrokeExport::Raster {
            reason: "field_empty",
        };
    }
    // 路线选择：抖动 / 自交 > 2 / 急转弯折返 ⇒ C；其余 A（专家 P0-1 判据 ✓）。
    let use_field = brush.jitter > 0.0 || crossings > 2 || has_sharp_fold(&clusters);
    if use_field {
        match field_contours(
            brush,
            geometry,
            Some(appearance),
            canvas,
            0.5,
            alpha_eff,
            &forced,
        ) {
            Some(contours) if !contours.is_empty() => {
                return StrokeExport::Vector {
                    layers: vec![ExportLayer {
                        contours,
                        fill_opacity: alpha_eff,
                    }],
                    css_blend,
                };
            }
            _ => {
                return StrokeExport::Raster {
                    reason: "field_empty",
                }
            }
        }
    }
    let mut contours: Vec<FittedContour> = Vec::new();
    for cluster in &clusters {
        let loop_pts = envelope_contour(cluster);
        if let Some(fitted) = fit_closed_contour(&loop_pts, &forced) {
            contours.push(fitted);
        }
    }
    if contours.is_empty() {
        return StrokeExport::Raster {
            reason: "fit_failed",
        };
    }
    StrokeExport::Vector {
        layers: vec![ExportLayer {
            contours,
            fill_opacity: alpha_eff,
        }],
        css_blend,
    }
}

/// 急转弯折返判据：相邻切向夹角 `> 45°` 且步长 `< r_i`（专家 P0-1 ✓）。
fn has_sharp_fold(clusters: &[Vec<SvgDab>]) -> bool {
    for cluster in clusters {
        for i in 1..cluster.len().saturating_sub(1) {
            let v1 = (
                cluster[i].x - cluster[i - 1].x,
                cluster[i].y - cluster[i - 1].y,
            );
            let v2 = (
                cluster[i + 1].x - cluster[i].x,
                cluster[i + 1].y - cluster[i].y,
            );
            let l1 = (v1.0 * v1.0 + v1.1 * v1.1).sqrt();
            let l2 = (v2.0 * v2.0 + v2.1 * v2.1).sqrt();
            if l1 < 1e-12 || l2 < 1e-12 {
                continue;
            }
            let cos = ((v1.0 * v2.0 + v1.1 * v2.1) / (l1 * l2)).clamp(-1.0, 1.0);
            if cos.acos() > 45.0f64.to_radians() && l1 < cluster[i].radius {
                return true;
            }
        }
    }
    false
}

/// 橡皮（`Retouch` 的 `erase`）的掩码轮廓：把擦除 dab 序列当作一笔普通
/// 笔触走 alpha 场提轮廓（引擎的擦除间距是 `size × 0.15` ⇒ 这里用等价的
/// 临时笔刷复刻 ✓）。`strength` 是单枚擦除强度（`opacity` ✓）；
/// 返回 `(轮廓组, 累积擦除强度)` ⇒ 调用方据此生成 SVG mask ✓。
pub fn erase_mask_contours(
    points: &[Pt],
    size: f64,
    hardness: f64,
    strength: f64,
    canvas: (u32, u32),
) -> Option<(Vec<FittedContour>, f64)> {
    if points.len() < 2 || size <= 0.0 {
        return None;
    }
    let brush = BrushSpec {
        size,
        hardness,
        flow: 1.0,
        opacity: strength.clamp(0.0, 1.0),
        spacing: 0.15,
        color: [0.0, 0.0, 0.0, 1.0],
        pressure_size: false,
        ..BrushSpec::default()
    };
    let geometry = StrokeGeometry {
        points: points
            .iter()
            .map(|&(x, y)| crate::brush::StrokePoint {
                x,
                y,
                pressure: 1.0,
            })
            .collect(),
        smooth: false,
    };
    let alpha_eff = effective_alpha(&brush);
    let contours = field_contours(&brush, &geometry, None, canvas, 0.5, alpha_eff, &[])?;
    if contours.is_empty() {
        return None;
    }
    Some((contours, alpha_eff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brush::StrokePoint;
    use serde_json::json;

    fn brush_with(size: f64, spacing: f64, opacity: f64) -> BrushSpec {
        BrushSpec {
            size,
            spacing,
            opacity,
            hardness: 1.0,
            flow: 1.0,
            color: [0.0, 0.0, 0.0, 1.0],
            pressure_size: false,
            ..BrushSpec::default()
        }
    }

    fn line_geometry(x0: f64, y0: f64, x1: f64, y1: f64) -> StrokeGeometry {
        StrokeGeometry {
            points: vec![
                StrokePoint {
                    x: x0,
                    y: y0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: x1,
                    y: y1,
                    pressure: 1.0,
                },
            ],
            smooth: false,
        }
    }

    /// 引擎实测中心线 alpha（校准实验的同款量法 ✓）。
    fn measured_center_alpha(brush: &BrushSpec) -> f32 {
        let mut buffer = Buffer::new(0, 0, 600, 100);
        let stroke = line_geometry(60.0, 50.0, 540.0, 50.0);
        crate::brush::stamp_stroke(&mut buffer, brush, &stroke);
        (296..301).map(|x| buffer.pixel(x, 50)[3]).sum::<f32>() / 5.0
    }

    #[test]
    fn fmt_num_is_fixed_point_and_deterministic() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(-0.0001), "0"); // -0 归一 ✓
        assert_eq!(fmt_num(12.0), "12");
        assert_eq!(fmt_num(0.5), ".5");
        assert_eq!(fmt_num(-0.5), "-.5");
        assert_eq!(fmt_num(12.34), "12.34");
        assert_eq!(fmt_num(0.05), ".05");
        assert_eq!(fmt_num(100.1), "100.1");
        assert_eq!(fmt_num(3.999), "4");
        // 关系断言：往返误差 ≤ 半个定点单位 ✓。
        for v in [0.001, 1.234, 57.891, 300.005, 4096.99] {
            let parsed: f64 = fmt_num(v).parse().unwrap();
            assert!((parsed - v).abs() <= 0.0050001, "{v} → {parsed}");
        }
    }

    /// 往返自检：序列化出的 path 文本按 SVG 语义重新解析、采样，
    /// 几何必须与原轮廓一致（守链式增量那个 bug ✓：段终点会偏出 100+px）。
    #[test]
    fn path_data_round_trips_geometry() {
        // 一段长直线边：起点 (365.5, 50)，终点 (42.7, 49.3)，控制点在 1/3、2/3 处。
        let contour = FittedContour {
            start: (365.5, 50.0),
            cubics: vec![Cubic {
                p1: (257.9, 49.77),
                p2: (150.3, 49.53),
                p3: (42.7, 49.3),
            }],
        };
        let d = contours_path_data(&[contour]);
        // 极简解析：抽出全部数字 token（支持 "-.68"/".5" 形），按 M + c 语义走。
        let mut nums: Vec<f64> = Vec::new();
        let mut cur = String::new();
        for ch in d.chars() {
            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && cur.is_empty()) {
                cur.push(ch);
            } else {
                if !cur.is_empty() {
                    nums.push(cur.parse().unwrap());
                    cur.clear();
                }
                if ch == '-' {
                    cur.push(ch);
                }
            }
        }
        if !cur.is_empty() {
            nums.push(cur.parse().unwrap());
        }
        assert_eq!(nums.len(), 8, "M(2) + c(6) 应共 8 个数：{d}");
        let (sx, sy) = (nums[0], nums[1]);
        let (c1, c2, en) = (
            (sx + nums[2], sy + nums[3]),
            (sx + nums[4], sy + nums[5]),
            (sx + nums[6], sy + nums[7]),
        );
        // 终点必须回到 (42.7, 49.3)（链式增量 bug 下会落在 ≈259 ⇒ 红 ✓）。
        assert!((en.0 - 42.7).abs() < 0.011, "终点 x={} d={d}", en.0);
        assert!((en.1 - 49.3).abs() < 0.011, "终点 y={} d={d}", en.1);
        // 控制点也必须落在原位置（不只是终点对）。
        assert!((c1.0 - 257.9).abs() < 0.011, "c1 x={} d={d}", c1.0);
        assert!((c2.0 - 150.3).abs() < 0.011, "c2 x={} d={d}", c2.0);
        // 中点采样 ≈ 三次 Bezier 在 t=0.5 的真值。
        let mid_x = 0.125 * sx + 0.375 * c1.0 + 0.375 * c2.0 + 0.125 * en.0;
        assert!((mid_x - 204.1).abs() < 0.05, "中点 x={mid_x} d={d}");
    }

    #[test]
    fn srgb_hex_known_values() {
        assert_eq!(srgb_hex([0.0, 0.0, 0.0]), "#000");
        assert_eq!(srgb_hex([1.0, 1.0, 1.0]), "#fff");
        // 线性 0.5 → sRGB ≈ 0.7354 → 188 = 0xbc ✓。
        assert_eq!(srgb_hex([0.5, 0.5, 0.5]), "#bcbcbc");
    }

    #[test]
    fn blend_mapping_covers_all_but_add_and_subtract() {
        let mut mapped = 0;
        for mode in BlendMode::ALL {
            match blend_css(mode) {
                Some(_) => mapped += 1,
                None => assert!(matches!(mode, BlendMode::Add | BlendMode::Subtract)),
            }
        }
        assert_eq!(mapped, 8);
        assert_eq!(BlendMode::ALL.len(), 10);
        // 字符串入口同表 ✓。
        assert_eq!(blend_css_name("multiply"), Some("multiply"));
        assert_eq!(blend_css_name("add"), None);
        assert_eq!(blend_css_name("linear_dodge"), None);
    }

    /// **校准恒等式**：`effective_alpha` 的公式必须与引擎实测一致（≤ 0.01）✓。
    /// 公式若被改坏（例如指数写回专家的 0.5/spacing）⇒ 这里立刻红 ✓。
    #[test]
    fn effective_alpha_matches_engine_measurement() {
        for (spacing, opacity) in [
            (0.05, 0.2),
            (0.05, 0.5),
            (0.1, 0.35),
            (0.25, 0.2),
            (0.25, 0.5),
            (0.5, 0.35),
            (0.5, 0.8),
            (1.0, 0.5),
            (2.0, 0.5),
        ] {
            let brush = brush_with(40.0, spacing, opacity);
            let measured = f64::from(measured_center_alpha(&brush));
            let formula = effective_alpha(&brush);
            assert!(
                (measured - formula).abs() <= 0.01,
                "spacing={spacing} opacity={opacity}：实测 {measured:.4} vs 公式 {formula:.4}"
            );
            // 关系：累积 alpha 不小于单枚 alpha、且 spacing 越密越大 ✓。
            assert!(formula >= dab_alpha(&brush) - 1e-12);
        }
        // **没有 opacity 上限**（专家公式的 min 帽在本引擎不存在 ✓）：
        let brush = brush_with(40.0, 0.05, 0.2);
        assert!(
            effective_alpha(&brush) > 0.9,
            "实测 {}",
            effective_alpha(&brush)
        );
    }

    #[test]
    fn dab_clusters_match_engine_sampling() {
        // 直线长 100、size 10、spacing 0.25 ⇒ 间距 2.5px ⇒ 41 枚（含两端 ✓）。
        let brush = brush_with(10.0, 0.25, 1.0);
        let clusters =
            stroke_dab_clusters(&brush, &line_geometry(0.0, 0.0, 100.0, 0.0), (200, 100));
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 41);
        assert!(clusters[0].iter().all(|d| (d.radius - 5.0).abs() < 1e-9));
        // 压力渐变 ⇒ 半径从 min_ratio 端到满端单调不减 ✓。
        let mut brush2 = brush_with(10.0, 0.25, 1.0);
        brush2.pressure_size = true;
        let geometry = StrokeGeometry {
            points: vec![
                StrokePoint {
                    x: 0.0,
                    y: 0.0,
                    pressure: 0.0,
                },
                StrokePoint {
                    x: 100.0,
                    y: 0.0,
                    pressure: 1.0,
                },
            ],
            smooth: false,
        };
        let clusters = stroke_dab_clusters(&brush2, &geometry, (200, 100));
        let radii: Vec<f64> = clusters[0].iter().map(|d| d.radius).collect();
        assert!((radii[0] - 1.25).abs() < 1e-9, "首半径 {}", radii[0]);
        assert!((radii[radii.len() - 1] - 5.0).abs() < 1e-9);
        assert!(radii.windows(2).all(|w| w[1] >= w[0] - 1e-12));
        // 虚线 ⇒ 断簇 ✓。
        let mut dash_brush = brush_with(10.0, 0.25, 1.0);
        dash_brush.dash = Some(10.0);
        let clusters = stroke_dab_clusters(
            &dash_brush,
            &line_geometry(0.0, 0.0, 100.0, 0.0),
            (200, 100),
        );
        assert!(clusters.len() >= 3, "虚线簇数 {}", clusters.len());
    }

    #[test]
    fn envelope_of_constant_radius_line_is_capsule() {
        let dabs: Vec<SvgDab> = (0..=40)
            .map(|i| SvgDab {
                x: f64::from(i) * 2.5,
                y: 0.0,
                radius: 5.0,
            })
            .collect();
        let contour = envelope_contour(&dabs);
        let area = signed_area(&contour).abs();
        // 胶囊面积 = 矩形 100×10 + 圆 π·25 ≈ 1078.5（允差 2% ⇒ 端帽采样误差 ✓）。
        let expected = 1000.0 + std::f64::consts::PI * 25.0;
        assert!(
            (area - expected).abs() / expected < 0.02,
            "面积 {area} vs {expected}"
        );
        let (mut min_x, mut max_x, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for p in &contour {
            min_x = min_x.min(p.0);
            max_x = max_x.max(p.0);
            max_y = max_y.max(p.1.abs());
        }
        assert!(
            (min_x + 5.0).abs() < 0.6 && (max_x - 105.0).abs() < 0.6 && (max_y - 5.0).abs() < 0.3
        );
    }

    #[test]
    fn marching_squares_circle_and_ring() {
        // 40×40 场：圆盘 alpha=1（r=10 @ (20,20)）。
        let (w, h) = (40usize, 40usize);
        let mut field = vec![0.0f32; w * h];
        for j in 0..h {
            for i in 0..w {
                let d = ((i as f64 - 19.5).powi(2) + (j as f64 - 19.5).powi(2)).sqrt();
                if d <= 10.0 {
                    field[j * w + i] = 1.0;
                }
            }
        }
        let loops = marching_squares(&field, w, h, (0, 0), 0.5);
        assert_eq!(loops.len(), 1);
        let area = signed_area(&loops[0]).abs();
        let expected = std::f64::consts::PI * 100.0;
        assert!(
            (area - expected).abs() / expected < 0.06,
            "圆面积 {area} vs {expected}"
        );
        // 圆环（6 ≤ r ≤ 10）⇒ 两条轮廓、方向相反（孔 ✓）。
        let mut ring = vec![0.0f32; w * h];
        for j in 0..h {
            for i in 0..w {
                let d = ((i as f64 - 19.5).powi(2) + (j as f64 - 19.5).powi(2)).sqrt();
                if (6.0..=10.0).contains(&d) {
                    ring[j * w + i] = 1.0;
                }
            }
        }
        let loops = marching_squares(&ring, w, h, (0, 0), 0.5);
        assert_eq!(loops.len(), 2, "环轮廓数 {}", loops.len());
        assert!(
            signed_area(&loops[0]) * signed_area(&loops[1]) < 0.0,
            "外环与孔方向应相反"
        );
        let total: f64 = loops.iter().map(|l| signed_area(l)).sum();
        let expected = std::f64::consts::PI * (100.0 - 36.0);
        assert!(
            (total.abs() - expected).abs() / expected < 0.08,
            "环面积 {} vs {expected}",
            total.abs()
        );
    }

    #[test]
    fn rdp_and_fit_preserve_shape() {
        // 圆轮廓拟合：面积误差 < 1%、段数远小于采样点数 ✓。
        let circle: Vec<Pt> = (0..128)
            .map(|i| {
                let a = f64::from(i) * std::f64::consts::TAU / 128.0;
                (50.0 + 20.0 * a.cos(), 50.0 + 20.0 * a.sin())
            })
            .collect();
        let fitted = fit_closed_contour(&circle, &[]).expect("圆应可拟合");
        let flat = flatten_contour(&fitted);
        let area = signed_area(&flat).abs();
        let expected = std::f64::consts::PI * 400.0;
        assert!(
            (area - expected).abs() / expected < 0.035,
            "拟合圆面积 {area} vs {expected}"
        );
        // 径向偏差才是 ε_fit 的直接口径：每点距真圆 ≤ 0.75px ✓。
        for p in &flat {
            let r = dist(*p, (50.0, 50.0));
            assert!((r - 20.0).abs() <= 0.75, "展平点半径 {r}");
        }
        assert!(fitted.cubics.len() <= 12, "圆段数 {}", fitted.cubics.len());
        // 矩形：角点保住（展平点应贴近四个角 ✓）、面积误差 < 1% ✓。
        let rect: Vec<Pt> = vec![(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)];
        let mut dense: Vec<Pt> = Vec::new();
        for w in 0..4 {
            let (a, b) = (rect[w], rect[(w + 1) % 4]);
            for k in 0..10 {
                let t = f64::from(k) / 10.0;
                dense.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
        let fitted = fit_closed_contour(&dense, &[]).expect("矩形应可拟合");
        let flat = flatten_contour(&fitted);
        let area = signed_area(&flat).abs();
        assert!((area - 800.0).abs() / 800.0 < 0.01, "矩形面积 {area}");
        for corner in rect {
            let nearest = flat
                .iter()
                .map(|p| dist(*p, corner))
                .fold(f64::INFINITY, f64::min);
            assert!(nearest < 0.7, "角 {corner:?} 最近展平点 {nearest}");
        }
        // RDP：共线点列只剩两端 ✓。
        let line: Vec<Pt> = (0..50).map(|i| (f64::from(i), 0.0)).collect();
        assert_eq!(rdp(&line, 0.2).len(), 2);
        // 路径写出：同输入两次字节一致 ✓（确定性）。
        let contours = vec![fit_closed_contour(&circle, &[]).unwrap()];
        assert_eq!(contours_path_data(&contours), contours_path_data(&contours));
        assert!(contours_path_data(&contours).starts_with("M "));
    }

    #[test]
    fn rasterize_and_metrics_identities() {
        let rect_loop = vec![vec![(0.0, 0.0), (40.0, 0.0), (40.0, 20.0), (0.0, 20.0)]];
        let black = [0.0f32, 0.0, 0.0];
        let img = rasterize_filled(&[(&rect_loop, black, 1.0)], (0, 0), 50, 30);
        // 全包含像素 alpha = 1、外部 = 0 ✓。
        assert_eq!(img.rgba[10 * 50 + 20][3], 1.0);
        assert_eq!(img.rgba[5 * 50 + 45][3], 0.0);
        let same = stroke_metrics(&img, &img);
        assert_eq!(same.e_interior, 0.0);
        assert_eq!(same.e_geom, 0.0);
        assert!(same.passes());
        // 平移 2px 的矩形 ⇒ E_geom 显著 > 0（度量必须能红 ✓）。
        let shifted = vec![vec![(2.0, 0.0), (42.0, 0.0), (42.0, 20.0), (2.0, 20.0)]];
        let img2 = rasterize_filled(&[(&shifted, black, 1.0)], (0, 0), 50, 30);
        let diff = stroke_metrics(&img, &img2);
        assert!(diff.e_geom > 0.05, "平移后 E_geom {}", diff.e_geom);
        // 孔：外环 + 反向内环 ⇒ 中心 alpha = 0 ✓（nonzero 抵消）。
        let donut = vec![
            vec![(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)],
            vec![(15.0, 15.0), (15.0, 25.0), (25.0, 25.0), (25.0, 15.0)],
        ];
        let img = rasterize_filled(&[(&donut, black, 1.0)], (0, 0), 40, 40);
        assert_eq!(img.rgba[20 * 40 + 20][3], 0.0, "孔中心应为空");
        assert_eq!(img.rgba[5 * 40 + 5][3], 1.0);
    }

    #[test]
    fn export_stroke_classification_and_validation() {
        let canvas = (300, 200);
        // 硬边直线 ⇒ 矢量、单层、验收通过 ✓。
        let brush = brush_with(12.0, 0.25, 1.0);
        let geometry = line_geometry(30.0, 100.0, 270.0, 100.0);
        let appearance = StrokeAppearance::default();
        match export_stroke(&brush, &geometry, &appearance, canvas) {
            StrokeExport::Vector { layers, .. } => {
                assert_eq!(layers.len(), 1);
                let refs: Vec<(&[FittedContour], f64)> = layers
                    .iter()
                    .map(|l| (l.contours.as_slice(), l.fill_opacity))
                    .collect();
                let metrics = validate_stroke(&brush, &geometry, Some(&appearance), canvas, &refs)
                    .expect("应能验收");
                assert!(metrics.passes(), "直线验收 {metrics:?}");
            }
            other => panic!("硬边直线应转矢量，得到 {other:?}"),
        }
        // 纹理外观 ⇒ 位图 ✓（先断言外观真的 active ⇒ 输入确实制造了情况 ✓）。
        let textured = StrokeAppearance::from_data(&json!({
            "appearance": { "texture": { "kind": "grain", "strength": 0.8 } }
        }));
        assert!(textured.active, "纹理外观应为 active");
        assert_eq!(
            export_stroke(&brush, &geometry, &textured, canvas),
            StrokeExport::Raster {
                reason: "appearance"
            }
        );
        // add 混合 ⇒ 位图 ✓。
        let mut add_brush = brush_with(12.0, 0.25, 1.0);
        add_brush.blend_mode = BlendMode::Add;
        assert_eq!(
            export_stroke(&add_brush, &geometry, &appearance, canvas),
            StrokeExport::Raster {
                reason: "blend_mode"
            }
        );
        // 软边大笔（hardness 0.3、fringe = 30×0.7 = 21px）⇒ 位图 ✓。
        let mut soft = brush_with(60.0, 0.25, 1.0);
        soft.hardness = 0.3;
        assert_eq!(
            export_stroke(&soft, &geometry, &appearance, canvas),
            StrokeExport::Raster {
                reason: "soft_edge"
            }
        );
        // 抖动 ⇒ 仍是矢量（走路线 C ✓）。
        let mut jittery = brush_with(12.0, 0.25, 1.0);
        jittery.jitter = 1.5;
        match export_stroke(&jittery, &geometry, &appearance, canvas) {
            StrokeExport::Vector { layers, .. } => assert_eq!(layers.len(), 1),
            other => panic!("抖动笔应走路线 C 转矢量，得到 {other:?}"),
        }
        // 半透明自交（opacity 0.5、spacing 0.5 ⇒ α_eff = 1−0.5² = 0.75、十字交叉）
        // ⇒ 三级等值层（环 0.5 / 平台 (0.75−0.5)/(1−0.5) = 0.5 / 深层 0.75 ✓）。
        let faint = brush_with(14.0, 0.5, 0.5);
        let cross_geometry = StrokeGeometry {
            points: vec![
                StrokePoint {
                    x: 60.0,
                    y: 60.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 240.0,
                    y: 140.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 240.0,
                    y: 60.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 60.0,
                    y: 140.0,
                    pressure: 1.0,
                },
            ],
            smooth: false,
        };
        match export_stroke(&faint, &cross_geometry, &appearance, canvas) {
            StrokeExport::Vector { layers, .. } => {
                assert_eq!(layers.len(), 3, "半透明自交应三级等值层（环/平台/深层）");
                let opacities: Vec<f64> = layers.iter().map(|l| l.fill_opacity).collect();
                assert_eq!(opacities, vec![0.5, 0.5, 0.75], "三级 opacity 恒等式");
                let refs: Vec<(&[FittedContour], f64)> = layers
                    .iter()
                    .map(|l| (l.contours.as_slice(), l.fill_opacity))
                    .collect();
                let metrics =
                    validate_stroke(&faint, &cross_geometry, Some(&appearance), canvas, &refs)
                        .expect("应能验收");
                assert!(metrics.passes(), "三级层验收 {metrics:?}");
            }
            other => panic!("半透明自交应转三级等值层矢量，得到 {other:?}"),
        }
    }
}
