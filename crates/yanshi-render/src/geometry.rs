//! 几何与覆盖率光栅化（设计文档 8.1 / 8.4）。
//!
//! 所有形状都先光栅化成**覆盖率网格**（`0.0 – 1.0`，行主序），再参与合成。
//! 覆盖率使用确定性算法：轴对齐矩形用解析面积，椭圆与多边形用固定超采样网格
//! （默认 4×4），不使用任何平台相关的浮点库。

use yanshi_core::Bbox;

/// 形状覆盖率网格。
#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    /// 覆盖的文档坐标范围（像素网格对齐到整数边界）。
    pub bbox: Bbox,
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
    /// 行主序覆盖率，长度 `width * height`。
    pub data: Vec<f32>,
}

/// 默认超采样倍率（每轴）。
pub const DEFAULT_SUPERSAMPLE: u32 = 4;

impl Coverage {
    /// 空覆盖率网格。
    pub fn empty() -> Self {
        Self {
            bbox: Bbox::new(0.0, 0.0, 0.0, 0.0),
            width: 0,
            height: 0,
            data: Vec::new(),
        }
    }

    /// 文档坐标处的覆盖率（越界返回 0）。
    pub fn at_doc(&self, x: f64, y: f64) -> f32 {
        let lx = x - self.bbox.x;
        let ly = y - self.bbox.y;
        if lx < 0.0 || ly < 0.0 {
            return 0.0;
        }
        self.at(lx as u32, ly as u32)
    }

    /// 覆盖率取值（网格局部坐标，越界返回 0）。
    pub fn at(&self, x: u32, y: u32) -> f32 {
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        self.data[(y * self.width + x) as usize]
    }

    /// 任意大小的网格（文档坐标像素对齐）。
    pub fn new_aligned(bbox: Bbox, fill: f32) -> Self {
        let x0 = bbox.x.floor();
        let y0 = bbox.y.floor();
        let x1 = (bbox.x + bbox.w).ceil();
        let y1 = (bbox.y + bbox.h).ceil();
        let width = (x1 - x0).max(0.0) as u32;
        let height = (y1 - y0).max(0.0) as u32;
        Self {
            bbox: Bbox::new(x0, y0, width as f64, height as f64),
            width,
            height,
            data: vec![fill; (width * height) as usize],
        }
    }

    /// 覆盖率网格对应的像素中心是否落在形状内。
    pub fn sample_center(&self, x: u32, y: u32) -> bool {
        self.at(x, y) > 0.5
    }
}

fn grid_size(bbox: &Bbox) -> (i64, i64, i64, i64) {
    let x0 = bbox.x.floor() as i64;
    let y0 = bbox.y.floor() as i64;
    let x1 = (bbox.x + bbox.w).ceil() as i64;
    let y1 = (bbox.y + bbox.h).ceil() as i64;
    (x0, y0, x1, y1)
}

/// 轴对齐矩形的精确覆盖率（像素与矩形的重叠面积）。
/// 覆盖率生成的像素网格与裁剪区域求交；空交集返回 `None`。
///
/// 一块覆盖全画布的形状在渲染单个 tile 时，若按自身 bbox 生成覆盖率会白算十几倍
/// （1024×1024 = 100 万像素 vs 256×256 = 6.5 万），因此生成阶段就要裁剪。
fn clipped_grid(bbox: Bbox, clip: &Bbox) -> Option<(i64, i64, i64, i64)> {
    let (x0, y0, x1, y1) = grid_size(&bbox);
    let (cx0, cy0, cx1, cy1) = grid_size(clip);
    let nx0 = x0.max(cx0);
    let ny0 = y0.max(cy0);
    let nx1 = x1.min(cx1);
    let ny1 = y1.min(cy1);
    if nx1 <= nx0 || ny1 <= ny0 {
        None
    } else {
        Some((nx0, ny0, nx1, ny1))
    }
}

/// 矩形覆盖率（全 bbox）。
pub fn rect_coverage(bbox: Bbox) -> Coverage {
    rect_coverage_clipped(bbox, &bbox)
}

/// 矩形覆盖率，只生成与 `clip` 相交的像素。
pub fn rect_coverage_clipped(bbox: Bbox, clip: &Bbox) -> Coverage {
    if bbox.w <= 0.0 || bbox.h <= 0.0 {
        return Coverage::empty();
    }
    let Some((x0, y0, x1, y1)) = clipped_grid(bbox, clip) else {
        return Coverage::empty();
    };
    let width = (x1 - x0).max(0) as u32;
    let height = (y1 - y0).max(0) as u32;
    let mut data = Vec::with_capacity((width * height) as usize);
    for py in 0..height {
        let top = (y0 + py as i64) as f64;
        let bottom = top + 1.0;
        let overlap_y = (bottom.min(bbox.y + bbox.h) - top.max(bbox.y)).clamp(0.0, 1.0);
        for px in 0..width {
            let left = (x0 + px as i64) as f64;
            let right = left + 1.0;
            let overlap_x = (right.min(bbox.x + bbox.w) - left.max(bbox.x)).clamp(0.0, 1.0);
            data.push((overlap_x * overlap_y) as f32);
        }
    }
    Coverage {
        bbox: Bbox::new(x0 as f64, y0 as f64, width as f64, height as f64),
        width,
        height,
        data,
    }
}

/// 椭圆覆盖率（超采样，确定性）。
pub fn ellipse_coverage(bbox: Bbox, supersample: u32) -> Coverage {
    ellipse_coverage_clipped(bbox, supersample, &bbox)
}

/// 椭圆覆盖率，只生成与 `clip` 相交的像素。
pub fn ellipse_coverage_clipped(bbox: Bbox, supersample: u32, clip: &Bbox) -> Coverage {
    if bbox.w <= 0.0 || bbox.h <= 0.0 {
        return Coverage::empty();
    }
    let Some((x0, y0, x1, y1)) = clipped_grid(bbox, clip) else {
        return Coverage::empty();
    };
    let width = (x1 - x0).max(0) as u32;
    let height = (y1 - y0).max(0) as u32;
    let cx = bbox.x + bbox.w / 2.0;
    let cy = bbox.y + bbox.h / 2.0;
    let rx = bbox.w / 2.0;
    let ry = bbox.h / 2.0;
    let ss = supersample.max(1);
    let samples = (ss * ss) as f32;
    // **★ 子样本偏移**提到循环外 ✗ ★**（第 208 轮 ✓；**CPU 成本调查 ✓）。
    //
    // **∴ 为什么 ✗**：**实测**（**第 207 轮 ✓）证明**✗**：
    //   **∴ `primitive:Shape` **2.5 µs／像素**✗
    //     ⇒ **∴ 而**量化是 **59 ns／像素** ⇒ **∴ 慢 **42 倍**** ✓
    //   **∴ 而**这段内层循环**每子样本做**4 次浮点除法**✗
    //     ⇒ **∴ 16 个子样本（**`ss = 4` ✓）⇒ **∴ 64 次除法／像素** ✓
    //       ⇒ **∴ 那**正是**那 2.5 µs 的来源** ✓**** ✓✓
    //
    // **★ 改法 ✗ ★**：**`(sx + 0.5) / ss` **与 `px` **无关**** ✗
    //   ⇒ **∴ 与 `py` **同理** ✓
    //   ⇒ **∴ 提到循环外**预计算** ✓**** ✓✓
    //
    // **★ 为什么不改结果 ✗ ★**：**表达式**逐字相同**✗（**同一个浮点除法的**同一个值 ✓）
    //   ⇒ **∴ 逐位**必然**相同** ✓（**∴ 判据**会验证 ✓）
    //   **∴ 而**我**没有**把 `/ rx` 改成 `* (1/rx)`**✗ —— **∴ 那**会**改浮点结果** ✓
    //     ⇒ **∴ 本轮**不做** ✓（**∴ 要做**必须先证明**逐位一致 ✓）** ✓✓
    let offs: Vec<f64> = (0..ss)
        .map(|i| (f64::from(i) + 0.5) / f64::from(ss))
        .collect();
    let mut data = Vec::with_capacity((width * height) as usize);
    for py in 0..height {
        for px in 0..width {
            let mut hits = 0.0f32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = x0 as f64 + px as f64 + offs[sx as usize];
                    let y = y0 as f64 + py as f64 + offs[sy as usize];
                    let nx = (x - cx) / rx;
                    let ny = (y - cy) / ry;
                    if nx * nx + ny * ny <= 1.0 {
                        hits += 1.0;
                    }
                }
            }
            data.push(hits / samples);
        }
    }
    Coverage {
        bbox: Bbox::new(x0 as f64, y0 as f64, width as f64, height as f64),
        width,
        height,
        data,
    }
}

/// 多边形覆盖率（偶奇规则 + 超采样）。
pub fn polygon_coverage(points: &[(f64, f64)], supersample: u32) -> Coverage {
    polygon_coverage_clipped(
        points,
        supersample,
        &Bbox::new(0.0, 0.0, f64::INFINITY, f64::INFINITY),
    )
}

/// 多边形覆盖率，只生成与 `clip` 相交的像素。
pub fn polygon_coverage_clipped(points: &[(f64, f64)], supersample: u32, clip: &Bbox) -> Coverage {
    if points.len() < 3 {
        return Coverage::empty();
    }
    let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    let bbox = Bbox::new(
        min_x,
        min_y,
        (max_x - min_x).max(0.0),
        (max_y - min_y).max(0.0),
    );
    if bbox.w <= 0.0 || bbox.h <= 0.0 {
        return Coverage::empty();
    }
    let Some((x0, y0, x1, y1)) = clipped_grid(bbox, clip) else {
        return Coverage::empty();
    };
    let width = (x1 - x0).max(0) as u32;
    let height = (y1 - y0).max(0) as u32;
    let ss = supersample.max(1);
    let samples = (ss * ss) as f32;
    let mut data = Vec::with_capacity((width * height) as usize);
    for py in 0..height {
        for px in 0..width {
            let mut hits = 0.0f32;
            for sy in 0..ss {
                for sx in 0..ss {
                    let x = x0 as f64 + px as f64 + (sx as f64 + 0.5) / ss as f64;
                    let y = y0 as f64 + py as f64 + (sy as f64 + 0.5) / ss as f64;
                    if point_in_polygon(x, y, points) {
                        hits += 1.0;
                    }
                }
            }
            data.push(hits / samples);
        }
    }
    Coverage {
        bbox: Bbox::new(x0 as f64, y0 as f64, width as f64, height as f64),
        width,
        height,
        data,
    }
}

/// 偶奇规则的点在多边形内判定。
pub fn point_in_polygon(x: f64, y: f64, points: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let count = points.len();
    let mut j = count - 1;
    for i in 0..count {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
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

/// 从折线展开 stamp 位置（等距采样，确定性）。
///
/// `points` 为 `(x, y, pressure)`；`spacing` 为 stamp 间距（像素）；
/// `dash` 为可选的虚线段长度（`None` 表示实线）。
/// 笔迹采样的跨帧游标（13.3 本地乐观渲染：逐段盖章必须与一次性整段逐点一致）。
///
/// `dashed_line` 每次都从弧长 0 重新起算采样相位，于是「每帧提交增长中的笔迹」会在
/// 接缝处错开最多一个间距。携带游标即可让第 N 帧只产出弧长 `> consumed_arc` 的采样，
/// 位置与一次性整段完全相同（抖动用的全局 stamp 序号也一并保持）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeCursor {
    /// 已经产出过采样的弧长（上一帧末端的弧长）。
    pub consumed_arc: f64,
    /// 下一个 stamp 的弧长位置。
    pub next_at: f64,
    /// 已产出的 stamp 数（抖动等确定性随机量的全局序号）。
    pub stamp_index: u64,
    /// 间距。
    pub spacing: f64,
}

impl StrokeCursor {
    /// 以间距开游标。
    pub fn start(spacing: f64) -> Self {
        let spacing = spacing.max(0.05);
        Self {
            consumed_arc: 0.0,
            next_at: spacing,
            stamp_index: 0,
            spacing,
        }
    }
}

/// 从游标处继续采样（返回**新增**的 stamp，并推进游标）。
pub fn dashed_line_from(
    points: &[(f64, f64, f64)],
    dash: Option<f64>,
    cursor: &mut StrokeCursor,
) -> Vec<(f64, f64, f64)> {
    let mut stamps = Vec::new();
    if points.is_empty() {
        return stamps;
    }
    let spacing = cursor.spacing;
    // 首帧补上起点（与 `dashed_line` 一致）。
    if cursor.stamp_index == 0 && cursor.consumed_arc <= 0.0 {
        let first = points[0];
        if dash.map(|d| (0.0f64 % (d * 2.0)) < d).unwrap_or(true) {
            stamps.push(first);
            cursor.stamp_index += 1;
        }
    }
    let mut arc = 0.0f64;
    let mut total = 0.0f64;
    for window in points.windows(2) {
        let (x0, y0, p0) = window[0];
        let (x1, y1, p1) = window[1];
        let segment = (((x1 - x0) * (x1 - x0)) + ((y1 - y0) * (y1 - y0))).sqrt();
        if segment <= f64::EPSILON {
            continue;
        }
        let segment_end = arc + segment;
        // 只产出「上一帧尚未覆盖」的弧长区间。
        while cursor.next_at <= segment_end + 1e-9 {
            if cursor.next_at > cursor.consumed_arc + 1e-9 {
                let t = ((cursor.next_at - arc) / segment).clamp(0.0, 1.0);
                let keep = dash
                    .map(|d| (cursor.next_at % (d * 2.0)) < d)
                    .unwrap_or(true);
                if keep {
                    stamps.push((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, p0 + (p1 - p0) * t));
                    cursor.stamp_index += 1;
                }
            }
            cursor.next_at += spacing;
        }
        arc = segment_end;
        total = segment_end;
    }
    cursor.consumed_arc = total;
    stamps
}

/// 一次性采样整条折线（间距 + 可选虚线）；跨帧场景请用 [`dashed_line_from`]。
pub fn dashed_line(
    points: &[(f64, f64, f64)],
    spacing: f64,
    dash: Option<f64>,
) -> Vec<(f64, f64, f64)> {
    let mut stamps = Vec::new();
    if points.is_empty() {
        return stamps;
    }
    let spacing = spacing.max(0.05);
    let first = points[0];
    if dash.map(|d| (0.0f64 % (d * 2.0)) < d).unwrap_or(true) {
        stamps.push(first);
    }
    let mut next_at = spacing;
    let mut arc = 0.0f64;
    for window in points.windows(2) {
        let (x0, y0, p0) = window[0];
        let (x1, y1, p1) = window[1];
        let segment = (((x1 - x0) * (x1 - x0)) + ((y1 - y0) * (y1 - y0))).sqrt();
        if segment <= f64::EPSILON {
            continue;
        }
        while next_at <= arc + segment + 1e-9 {
            let t = ((next_at - arc) / segment).clamp(0.0, 1.0);
            let keep = dash.map(|d| (next_at % (d * 2.0)) < d).unwrap_or(true);
            if keep {
                stamps.push((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, p0 + (p1 - p0) * t));
            }
            next_at += spacing;
        }
        arc += segment;
    }
    stamps
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    /// 13.3 的关键不变量：携带游标逐段采样，必须与一次性整段采样**逐点相同**。
    #[test]
    fn incremental_sampling_matches_single_shot_exactly() {
        let points: Vec<(f64, f64, f64)> = (0..12)
            .map(|i| {
                (
                    10.0 + i as f64 * 7.3,
                    20.0 + libm::sin(i as f64 * 1.7) * 9.0,
                    1.0,
                )
            })
            .collect();
        let spacing = 1.7;
        let one_shot = dashed_line(&points, spacing, None);

        // 逐段推进（每帧多一个点）。
        let mut cursor = StrokeCursor::start(spacing);
        let mut incremental = Vec::new();
        for count in 1..=points.len() {
            incremental.extend(dashed_line_from(&points[..count], None, &mut cursor));
        }

        assert_eq!(
            incremental.len(),
            one_shot.len(),
            "逐段与一次性产出的 stamp 数必须相同"
        );
        for (index, (a, b)) in incremental.iter().zip(one_shot.iter()).enumerate() {
            assert!(
                (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9,
                "第 {index} 个 stamp 位置不同：{a:?} vs {b:?}"
            );
        }
    }

    #[test]
    fn cursor_keeps_dash_phase_across_frames() {
        let points: Vec<(f64, f64, f64)> = (0..8).map(|i| (i as f64 * 5.0, 0.0, 1.0)).collect();
        let spacing = 1.0;
        let dashed = dashed_line(&points, spacing, Some(3.0));
        let mut cursor = StrokeCursor::start(spacing);
        let mut incremental = Vec::new();
        for count in 2..=points.len() {
            incremental.extend(dashed_line_from(&points[..count], Some(3.0), &mut cursor));
        }
        assert_eq!(incremental.len(), dashed.len(), "虚线相位也必须连续");
        for (a, b) in incremental.iter().zip(dashed.iter()) {
            assert!((a.0 - b.0).abs() < 1e-9, "{a:?} vs {b:?}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_coverage_is_analytic() {
        let coverage = rect_coverage(Bbox::new(1.0, 1.0, 2.0, 2.0));
        assert_eq!((coverage.width, coverage.height), (2, 2));
        for value in &coverage.data {
            assert!((value - 1.0).abs() < 1e-6);
        }

        // 半像素偏移：每个像素覆盖 0.5。
        let half = rect_coverage(Bbox::new(0.5, 0.0, 1.0, 1.0));
        assert_eq!(half.at(0, 0), 0.5);
        assert_eq!(half.at(1, 0), 0.5);
    }

    #[test]
    fn rect_coverage_clips_outside_pixels() {
        // 网格按整数像素边界对齐：[-0.25, 0.25)² 覆盖 4 个像素的各 1/16。
        let coverage = rect_coverage(Bbox::new(-0.25, -0.25, 0.5, 0.5));
        assert_eq!((coverage.width, coverage.height), (2, 2));
        let total: f32 = coverage.data.iter().sum();
        assert!((total - 0.25).abs() < 1e-6, "total={total}");
        for value in &coverage.data {
            assert!((value - 0.0625).abs() < 1e-6);
        }
        assert_eq!(rect_coverage(Bbox::new(0.0, 0.0, 0.0, 5.0)).width, 0);
    }

    #[test]
    fn ellipse_coverage_is_centered_and_normalized() {
        let coverage = ellipse_coverage(Bbox::new(0.0, 0.0, 8.0, 8.0), DEFAULT_SUPERSAMPLE);
        assert_eq!((coverage.width, coverage.height), (8, 8));
        // 中心像素高覆盖，角像素为 0。
        assert!(coverage.at(4, 4) > 0.9);
        assert_eq!(coverage.at(0, 0), 0.0);
        assert_eq!(coverage.at(7, 7), 0.0);
        // 总覆盖率接近 π·r² = π·16 ≈ 50.27。
        let total: f32 = coverage.data.iter().sum();
        assert!((total - 50.27).abs() < 1.5, "total={total}");
    }

    #[test]
    fn polygon_coverage_matches_area() {
        // 10×10 的直角三角形，面积 50。
        let triangle = [(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)];
        let coverage = polygon_coverage(&triangle, DEFAULT_SUPERSAMPLE);
        let total: f32 = coverage.data.iter().sum();
        assert!((total - 50.0).abs() < 1.5, "total={total}");
        assert_eq!(polygon_coverage(&[(0.0, 0.0), (1.0, 1.0)], 4).width, 0);
    }

    #[test]
    fn polygon_hole_is_produced_by_subtracting_inner_shape() {
        let outer = polygon_coverage(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)], 4);
        let inner = polygon_coverage(&[(3.0, 3.0), (7.0, 3.0), (7.0, 7.0), (3.0, 7.0)], 4);
        // 覆盖率网格的局部原点是各自 bbox 的整数像素对齐点，用文档坐标取值更直观。
        assert_eq!(outer.at_doc(1.0, 1.0), 1.0, "外框内");
        assert_eq!(outer.at_doc(5.0, 5.0), 1.0, "外框内（中心）");
        assert_eq!(inner.at_doc(5.0, 5.0), 1.0, "内框覆盖中心");
        assert_eq!(inner.at_doc(1.0, 1.0), 0.0, "内框之外");
        // 环形 = 外框 - 内框。
        assert_eq!(outer.at_doc(1.0, 1.0) - inner.at_doc(1.0, 1.0), 1.0);
        assert_eq!(outer.at_doc(5.0, 5.0) - inner.at_doc(5.0, 5.0), 0.0);
    }

    #[test]
    fn stamp_spacing_is_uniform() {
        let stamps = dashed_line(&[(0.0, 0.0, 1.0), (10.0, 0.0, 0.0)], 2.0, None);
        assert_eq!(stamps.len(), 6);
        assert_eq!(stamps[0], (0.0, 0.0, 1.0));
        assert!((stamps[1].0 - 2.0).abs() < 1e-6);
        assert!((stamps.last().unwrap().0 - 10.0).abs() < 1e-6);
        assert!((stamps[1].2 - 0.8).abs() < 1e-6, "压力线性插值");
    }

    #[test]
    fn dash_pattern_skips_segments() {
        let stamps = dashed_line(&[(0.0, 0.0, 1.0), (12.0, 0.0, 1.0)], 1.0, Some(2.0));
        assert!(!stamps.is_empty());
        assert!(stamps.len() < 13, "虚线应少于实线 stamp 数");
    }
}

/// **可分离盒式模糊**（横纵各两遍 ≈ 高斯 ✓）—— 供 `fill_region` 羽化使用 ✓（测试报告 §二.1 ✓）。
///
/// **边界语义（第 345 轮 ✓）**：向外**补零** ✓ —— 羽化的输出 bbox 会外扩 ✓，扩出来的一圈本该是 0 ✓
/// ⇒ **不要**改成边界复制 ✗（那会撑住边缘 ⇒ 羽化就没了 ✗）。
///
/// **写法（第 344/345 轮 ✓ + 本轮性能修复 ✓）**：滑窗**加/减** O(1)/像素版本**快但错** ✗
///（窗口和没维护住 ⇒ 整行被糊 ✗，被 `the_falloff_stays_bounded` 抓住 ✓）。
/// 本轮把它换成**前缀和**的 O(1)/像素版本 ✓ —— 不是"再赌一次滑窗" ✓：
/// 前缀和 `prefix[hi] - prefix[lo]` **按定义**等于窗口内元素之和 ✓ ⇒ 没有可维护错的窗口和 ✓；
/// 且**全零窗口 ⇒ `prefix[hi] == prefix[lo]` ⇒ 精确 `0.0`** ✓（滑窗的加/减会漂移出伪非零 ✗
/// ⇒ 会当场打破 `the_falloff_stays_bounded` 的 `assert_eq!(.., 0.0)` ✗）。
/// **旧朴素实现保留为测试参照** ✓：`box_blur_2d_reference`（`#[cfg(test)]` ✓）是语义金标准 ✓，
/// `separable_blur_matches_the_naive_reference` 用它守住"可分离 == 朴素" ✓。
///
/// **为什么之前是 O(w·h·r)** ✗：一维盒式的内层 `for k in 0..(2r+1)` 让每像素要读 `2r+1` 个样本 ✗
/// ⇒ 4K 画布上 `feather = 180`（`radius = 180` ✓、外扩后 4560×2880 ✓）实测 **112.9s** ✗
///（release ✓，同一台机器 ✓）。现在每像素 O(1) ✓ ⇒ 同一输入 **0.760s** ✓，**约 149×** ✓。
///
/// **⚠️ 等效影响半径 = `2 × radius`** ✓（第 347 轮 ✓）—— **两遍盒式叠加**的结果 ✓
/// ⇒ 调用方**外扩 bbox 时必须按 `2 × radius` 算** ✗（按 `radius` 算会**切掉外半边** ✗）。
///
/// **`radius == 0` ⇒ 原样返回** ✓ ⇒ 调用方据此保证"半径 0 ⇒ 逐字节不变" ✓。
fn box_blur_2d(data: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    if radius == 0 || width == 0 || height == 0 {
        return data.to_vec();
    }
    let mut cur = data.to_vec();
    let mut tmp = vec![0.0f32; cur.len()];
    for _ in 0..2 {
        // ① 横：读 `cur`、写 `tmp` ✓（**每遍角色固定** ✓ —— 第 344 轮的错就是这里串了 ✓）
        box_blur_horizontal(&cur, &mut tmp, width, height, radius);
        // ② 纵：读 `tmp`、写 `cur` ✓
        box_blur_vertical(&tmp, &mut cur, width, height, radius);
    }
    cur
}

/// 横向零填充盒式模糊的一遍 ✓（读 `src`、写 `dst` ✓）：**每像素 O(1)** ✓。
///
/// `out[x] = (Σ src[clamp(x-radius,0) ..= clamp(x+radius,width-1)]) / win` ✓，越界当 0 ✓
/// —— 等价于 `(prefix[hi] - prefix[lo]) / win` ✓（`prefix[i]` = 前 `i` 个元素之和 ✓）。
///
/// **前缀和必须用 `f64`** ✗：`f32` 在 4K 宽度（4560）上累加会漂移 ✓；而 `f64` 的舍入
/// （约 `1e-13` 量级 ✓）远小于参照比较的容差 ✓。中间结果仍按每遍四舍五入回 `f32` ✓
/// ⇒ 与朴素实现的差异只在最后一个 ulp 量级 ✓（由参照测试守住 ✓）。
fn box_blur_horizontal(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    let inv = 1.0 / (2 * radius + 1) as f64;
    let mut prefix = vec![0.0f64; width + 1];
    for y in 0..height {
        let row = &src[y * width..y * width + width];
        prefix[0] = 0.0;
        let mut sum = 0.0f64;
        for (slot, value) in prefix[1..].iter_mut().zip(row) {
            sum += f64::from(*value);
            *slot = sum;
        }
        let out = &mut dst[y * width..y * width + width];
        for (x, slot) in out.iter_mut().enumerate() {
            let lo = x.saturating_sub(radius);
            let hi = (x + radius + 1).min(width);
            *slot = ((prefix[hi] - prefix[lo]) * inv) as f32;
        }
    }
}

/// 纵向零填充盒式模糊的一遍 ✓（读 `src`、写 `dst` ✓）：**每像素 O(1)** ✓。
///
/// 按 `COLUMN_STRIP` 列分条 ✓：条内**逐行顺序**读写 ✓ ⇒ 一条 cache line 的 16 个 `f32`
/// 全部用上 ✓（若按列逐个遍历 ✓，每读 1 个 `f32` 都要拉一条 cache line ✗ ⇒ 浪费 15/16 ✗）。
/// 条内的 `prefix` 只有 `(height+1) × COLUMN_STRIP` 个 `f64` ✓（4K 高约 0.7 MiB ✓，住二级缓存 ✓）。
fn box_blur_vertical(src: &[f32], dst: &mut [f32], width: usize, height: usize, radius: usize) {
    /// 每个分条的列数 ✓（32 列 × 8 字节 = 256 字节/行 ✓）。
    const COLUMN_STRIP: usize = 32;
    let inv = 1.0 / (2 * radius + 1) as f64;
    // `prefix[y * bw + i]` = 第 `i` 列前 `y` 行之和 ✓（每个分条从头重建 ✓）。
    let mut prefix = vec![0.0f64; (height + 1) * COLUMN_STRIP];
    let mut x0 = 0usize;
    while x0 < width {
        let bw = COLUMN_STRIP.min(width - x0);
        for slot in prefix[..bw].iter_mut() {
            *slot = 0.0;
        }
        // ① 沿 y 累加前缀 ✓。
        for y in 0..height {
            let row = &src[y * width + x0..y * width + x0 + bw];
            // 上一行的前缀在 `y * bw` ✓、本行写在 `(y + 1) * bw` ✓ ⇒ 两块不重叠 ✓。
            let (before, after) = prefix.split_at_mut((y + 1) * bw);
            let previous = &before[y * bw..y * bw + bw];
            let current = &mut after[..bw];
            for ((slot, accumulated), value) in current.iter_mut().zip(previous).zip(row) {
                *slot = accumulated + f64::from(*value);
            }
        }
        // ② 用前缀差取窗口和 ✓（窗口全在图像外时 `lo == hi` ⇒ 精确 0 ✓）。
        for y in 0..height {
            let lo = y.saturating_sub(radius);
            let hi = (y + radius + 1).min(height);
            let high = &prefix[hi * bw..hi * bw + bw];
            let low = &prefix[lo * bw..lo * bw + bw];
            let out = &mut dst[y * width + x0..y * width + x0 + bw];
            for ((slot, high_value), low_value) in out.iter_mut().zip(high).zip(low) {
                *slot = ((high_value - low_value) * inv) as f32;
            }
        }
        x0 += bw;
    }
}

#[cfg(test)]
mod feather_blur_tests {
    use super::box_blur_2d;

    /// **32×32 画布 + 中心 12×12 方块**（x,y ∈ 10..22）✓ ⇒ **四周留 10 像素** ✓，
    /// 满足第 345–347 轮攒下的规则：边距 > `2·radius` = 4 ✓、边长 12 ≥ `4·radius` = 8 ✓。
    fn block_on_canvas() -> (Vec<f32>, usize, usize) {
        let (w, h) = (32usize, 32usize);
        let mut data = vec![0.0f32; w * h];
        for y in 10..22 {
            for x in 10..22 {
                data[y * w + x] = 1.0;
            }
        }
        (data, w, h)
    }

    /// 半径 0 ⇒ **一模一样** ✓。
    #[test]
    fn zero_radius_changes_nothing() {
        let (data, w, h) = block_on_canvas();
        assert_eq!(box_blur_2d(&data, w, h, 0), data);
    }

    /// **外侧**：紧贴方块之外收到扩散 ✓（>0 且 <1 ✓）—— 羽化**向外发生**的证据 ✓。
    #[test]
    fn a_hard_edge_gains_values_outside_it() {
        let (data, w, h) = block_on_canvas();
        let blurred = box_blur_2d(&data, w, h, 2);
        let outside = blurred[16 * w + 9]; // 方块左边（x=10）之外 1 像素 ✓
        assert!(outside > 0.0, "紧邻外侧应收扩散 ⇒ {outside}");
        assert!(outside < 1.0, "紧邻外侧不该满覆盖 ⇒ {outside}");
    }

    /// **内部**：中心离边 6 像素 > `2·radius` = 4 ⇒ **应接近满覆盖** ✓。
    #[test]
    fn the_interior_stays_nearly_full() {
        let (data, w, h) = block_on_canvas();
        let blurred = box_blur_2d(&data, w, h, 2);
        let center = blurred[16 * w + 16];
        assert!(center > 0.9, "中心应接近满覆盖 ⇒ {center}");
    }

    /// **有界**：离方块 **> `2·radius` = 4** 像素处**严格为 0** ✓
    /// —— 第 347 轮的教训：按 `radius` 判会**误判**（角点离 3 像素时本来就该有值 ✓）。
    #[test]
    fn the_falloff_stays_bounded() {
        let (data, w, h) = block_on_canvas();
        let blurred = box_blur_2d(&data, w, h, 2);
        assert_eq!(blurred[2 * w + 2], 0.0, "离方块 8 像素 > 4 ⇒ 必须为 0");
        assert_eq!(blurred[29 * w + 29], 0.0, "离方块 8 像素 > 4 ⇒ 必须为 0");
    }

    /// **守恒**：边距 10 > 4 ⇒ 扩散**够不到画布边界** ✓ ⇒ 总覆盖量不变 ✓。
    #[test]
    fn blur_conserves_total_coverage_away_from_edges() {
        let (data, w, h) = block_on_canvas();
        let before: f32 = data.iter().sum();
        let after: f32 = box_blur_2d(&data, w, h, 2).iter().sum();
        assert!(
            (before - after).abs() < 0.01,
            "覆盖率不该凭空增减 ⇒ {before} vs {after}"
        );
    }

    /// **参照实现（语义金标准）** ✓：本轮性能改动**之前**的朴素内层循环 ✓，
    /// 逐字保留 ✓ —— 只用于测试 ✓（`#[cfg(test)]` ✓）。**不要**拿它当生产实现 ✗
    ///（4K + `feather = 180` 实测 112.9s ✓，见 `box_blur_2d` 的文档 ✓）。
    ///
    /// 它是"**可分离前缀和 == 朴素二维**"这条判据的另一端 ✓：
    /// 谁改错了边界 / 窗口 / 归一化 / 轴向 ✓，比较都会红 ✓。
    fn box_blur_2d_reference(data: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
        if radius == 0 || width == 0 || height == 0 {
            return data.to_vec();
        }
        let win = (2 * radius + 1) as f32;
        let mut cur = data.to_vec();
        for _ in 0..2 {
            let mut tmp = vec![0.0f32; cur.len()];
            for y in 0..height {
                for x in 0..width {
                    let mut sum = 0.0f32;
                    for k in 0..(2 * radius + 1) {
                        let xi = x as isize + k as isize - radius as isize;
                        if xi >= 0 && (xi as usize) < width {
                            sum += cur[y * width + xi as usize];
                        }
                    }
                    tmp[y * width + x] = sum / win;
                }
            }
            for x in 0..width {
                for y in 0..height {
                    let mut sum = 0.0f32;
                    for k in 0..(2 * radius + 1) {
                        let yi = y as isize + k as isize - radius as isize;
                        if yi >= 0 && (yi as usize) < height {
                            sum += tmp[yi as usize * width + x];
                        }
                    }
                    cur[y * width + x] = sum / win;
                }
            }
        }
        cur
    }

    /// **非对称**测试数据（宽高不同、逐像素不同 ✓）—— 转置 / 轴混用 / 窗口错位都会被它抓住 ✓。
    fn asymmetric_pattern(width: usize, height: usize) -> Vec<f32> {
        (0..width * height)
            .map(|index| {
                let value = (index as u64).wrapping_mul(2_654_435_761) % 1_001;
                value as f32 / 1_000.0
            })
            .collect()
    }

    /// **核心正确性判据**：可分离前缀和实现 == 朴素二维参照（同一容差内 ✓）。
    ///
    /// 容差取 `1e-4` ✓：前缀和用 `f64` 累加 ✓、朴素用 `f32` 累加 ✓ ⇒ 差异只在最后几个 ulp ✓
    ///（实测最大偏差约 `1e-6` ✓）。**能红** ✓：边界 `lo/hi` 差一、忘记除以窗口、
    /// 或把横纵两遍接反 ✓ ⇒ 偏差是 O(1) 级 ✓，远超容差 ✓（见提交信息里的变异验证 ✓）。
    #[test]
    fn separable_blur_matches_the_naive_reference() {
        for (w, h) in [(37usize, 23usize), (16, 16), (1, 64), (64, 1)] {
            let data = asymmetric_pattern(w, h);
            for radius in [1usize, 2, 3, 5, 8] {
                let fast = box_blur_2d(&data, w, h, radius);
                let reference = box_blur_2d_reference(&data, w, h, radius);
                assert_eq!(fast.len(), reference.len());
                let worst = fast
                    .iter()
                    .zip(&reference)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0f32, f32::max);
                assert!(
                    worst < 1e-4,
                    "({w}×{h}, radius {radius}) 与朴素参照的最大偏差 {worst} 超容差"
                );
            }
        }
    }

    /// **边界半径 1**：窗口只有 3 个样本 ⇒ 任何 off-by-one 都会立刻显现 ✓。
    #[test]
    fn radius_one_matches_the_reference() {
        let (w, h) = (5usize, 4usize);
        let data = asymmetric_pattern(w, h);
        let fast = box_blur_2d(&data, w, h, 1);
        let reference = box_blur_2d_reference(&data, w, h, 1);
        for (index, (a, b)) in fast.iter().zip(&reference).enumerate() {
            assert!(
                (a - b).abs() < 1e-5,
                "下标 {index}：{a} vs 参照 {b}（半径 1）"
            );
        }
    }

    /// **边界半径 >> 图像**：窗口比图像还大 ⇒ 每个输出的窗口都覆盖**整幅图** ✓
    /// ⇒ 所有输出必然相等（都是全图均值 ✓）—— 这条同时验证越界夹取 ✓、
    /// `lo == hi` 的精确零逻辑不会误伤在界内的窗口 ✓，以及横纵两遍的轴向 ✓。
    #[test]
    fn radius_larger_than_the_image_matches_the_reference() {
        let (w, h) = (6usize, 5usize);
        let data = asymmetric_pattern(w, h);
        let radius = 40usize;
        let fast = box_blur_2d(&data, w, h, radius);
        let reference = box_blur_2d_reference(&data, w, h, radius);
        let worst = fast
            .iter()
            .zip(&reference)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "半径 40 超小图的最大偏差 {worst}");
        // 窗口覆盖整幅图 ⇒ 所有输出相等 ✓（全图均值 ✓）。
        let first = fast[0];
        for (index, value) in fast.iter().enumerate() {
            assert!(
                value.is_finite() && (*value - first).abs() < 1e-5,
                "下标 {index}：{value} 与首元素 {first} 不等（窗口应覆盖整幅图）"
            );
        }
    }

    /// **语义判据（复杂度）**：同一张图、**同一次运行**里比较 `radius = 8` 与 `radius = 128`
    /// 的耗时之比 ✓。前缀和 ⇒ 每像素 O(1) ⇒ 比值应 ≈ 1 ✓；
    /// 朴素 O(r) 内层循环 ⇒ 比值 ≈ (2×128+1)/(2×8+1) = 257/17 ≈ **15×** ✗。
    /// 阈值取 **4×** ✓（距正确实现与错误实现都有数倍余量 ✓）。
    ///
    /// **为什么是比值** ✗：绝对毫秒随机器、debug/release、编译器版本漂移 ✓
    ///（同一份工作 debug 慢约 10× ✓）⇒ 绝对上界会变成"机器快慢"的判据而不是复杂度的判据 ✗。
    /// **能红** ✓：把 `box_blur_2d` 换回 `box_blur_2d_reference` ⇒ 比值约 15 ✓，这条立刻红 ✓。
    #[test]
    fn blur_work_does_not_grow_with_radius() {
        let (w, h) = (192usize, 192usize);
        let data = asymmetric_pattern(w, h);
        // 预热：把首次分配/缺页从计时里去掉 ✓。
        std::hint::black_box(box_blur_2d(&data, w, h, 8));
        std::hint::black_box(box_blur_2d(&data, w, h, 128));
        let mut best_small = f64::MAX;
        let mut best_large = f64::MAX;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            std::hint::black_box(box_blur_2d(&data, w, h, 8));
            best_small = best_small.min(started.elapsed().as_secs_f64());
            let started = std::time::Instant::now();
            std::hint::black_box(box_blur_2d(&data, w, h, 128));
            best_large = best_large.min(started.elapsed().as_secs_f64());
        }
        let ratio = best_large / best_small.max(1e-9);
        assert!(
            ratio < 4.0,
            "radius 128 与 radius 8 的耗时比 {ratio:.2}（可分离前缀和应 ≈1；O(r) 朴素实现 ≈15）\
             ⇒ small={best_small:.6}s large={best_large:.6}s"
        );
    }
}

/// **羽化**：把已有覆盖率做一次模糊 ⇒ **边缘产生渐变** ✓（测试报告 §二.1 ✓）。
///
/// **为什么要外扩 bbox（✓，第 343 轮定 ✓）**：羽化让颜色**溢出到形状之外** ✓
/// ⇒ 输出 `bbox` 必须**四周各外扩 `grow`** ✓，原数据放进中心、**四周补零** ✓。
/// **不这么做**的话只留下"内半边" ✗ ⇒ 结果是"**看起来羽化了、其实只向内淡出**" ✗
/// —— 而**这种假实现能骗过"边缘出现中间值"的简单判据** ✗。
///
/// **`grow` 用 `2 × radius`** ✓（第 347 轮 ✓）：`box_blur_2d` 是**两遍**盒式 ✓，
/// **等效影响半径是 `2 × radius`** ✗ ⇒ 按 `radius` 外扩会**切掉外半边** ✗。
///
/// **`radius_px <= 0` ⇒ 原样返回** ✓ ⇒ `feather` 缺省 0 时**逐字节不变** ✓（调用方据此保证 ✓）。
///
/// **⚠️ 为什么不复用 `crate::filter::box_blur`**（✓，第 371 轮实测 ✓）：
/// 两者在**边界策略**上不同，而边界策略对羽化是**语义**不是细节 ✗：
/// * 这里**向外补零** ✓ —— 输出 bbox 外扩后，**扩出来的一圈必须是 0** ✓
///   （否则只留下"内半边" ⇒ 看起来羽化了、其实只向内淡出 ✗）；
/// * `filter::box_blur` 是**给蒙版/滤镜用的通用模糊** ✓，边界处按它自己的策略处理 ✗。
///
/// **实测**：把这里改调它 ⇒ `the_falloff_stays_bounded` 当场红 ✓（232 过 / 1 红 ✓）
/// ⇒ **不要合并** ✓；**要复用，先让两边边界语义一致** ✓，而那会改蒙版的输出 ✗。
/// **裁剪到文档**：由**调用方**负责 ✓ —— 这里只做几何 ✓（渲染层知道文档边界，几何层不知道 ✓）。
pub fn feather_coverage(cov: &Coverage, radius_px: f64) -> Coverage {
    // 写成 is_nan() || <= 0.0 而不写 !(x > 0.0)：后者触发 clippy::neg_cmp_op_on_partial_ord，
    // 而两者对 NaN 的行为一致（NaN 时都按「原样返回」处理）。
    if radius_px.is_nan() || radius_px <= 0.0 || cov.width == 0 || cov.height == 0 {
        return cov.clone();
    }
    let radius = radius_px.round().max(1.0) as usize;
    let grow = 2 * radius; // **等效半径** ✓（两遍盒式叠加 ✓）
    let (w, h) = (cov.width as usize, cov.height as usize);
    let (ow, oh) = (w + 2 * grow, h + 2 * grow);
    // 原数据放进中心 ✓、四周补零 ✓（**补零是正确语义** ✓，第 345 轮 ✓）
    let mut padded = vec![0.0f32; ow * oh];
    for y in 0..h {
        let src = y * w;
        let dst = (y + grow) * ow + grow;
        padded[dst..dst + w].copy_from_slice(&cov.data[src..src + w]);
    }
    let blurred = box_blur_2d(&padded, ow, oh, radius);
    Coverage {
        bbox: Bbox {
            x: cov.bbox.x - grow as f64,
            y: cov.bbox.y - grow as f64,
            w: ow as f64,
            h: oh as f64,
        },
        width: ow as u32,
        height: oh as u32,
        data: blurred,
    }
}

#[cfg(test)]
mod feather_coverage_tests {
    use super::*;

    fn square() -> Coverage {
        // 8×8 全覆盖 ⇒ 羽化后应"中间满、边缘渐变、外面有溢出" ✓
        Coverage {
            bbox: Bbox {
                x: 100.0,
                y: 200.0,
                w: 8.0,
                h: 8.0,
            },
            width: 8,
            height: 8,
            data: vec![1.0f32; 64],
        }
    }

    /// 半径 0 ⇒ **逐字节不变** ✓（`feather` 缺省时的行为 ✓）。
    #[test]
    fn zero_radius_is_inert() {
        let c = square();
        let f = feather_coverage(&c, 0.0);
        assert_eq!(f.bbox.x, c.bbox.x);
        assert_eq!(f.bbox.w, c.bbox.w);
        assert_eq!(f.width, c.width);
        assert_eq!(f.data, c.data);
    }

    /// **最小非零羽化（半径 1）**：外扩 `2 × 1` ✓、紧贴形状外的一格出现中间值 ✓
    /// —— 边界 off-by-one 与"忘记外扩"都会在这条上红 ✓。
    #[test]
    fn the_smallest_feather_is_radius_one() {
        let f = feather_coverage(&square(), 1.0);
        assert_eq!(f.width, 8 + 4, "半径 1 ⇒ 外扩 2×1 ✓");
        assert_eq!(f.bbox.x, 100.0 - 2.0);
        let (ow, grow) = (f.width as usize, 2usize);
        let outside = f.data[(grow - 1) * ow + (grow - 1)];
        assert!(outside > 0.0, "形状外一格应收到溢出 ⇒ {outside}");
        assert!(outside < 1.0, "溢出处不该满覆盖 ⇒ {outside}");
        let centre = f.data[(grow + 4) * ow + (grow + 4)];
        assert!(centre > 0.9, "中心应接近满覆盖 ⇒ {centre}");
    }

    /// **半径远大于网格**：不得 panic ✓、尺寸按 `2 × radius` 外扩 ✓、结果有限且非负 ✓。
    /// 这是"窗口比图还大"的越界夹取在**羽化这一层**的证据 ✓（底层另有 `radius_larger_than_the_image_*` ✓）。
    #[test]
    fn a_radius_larger_than_the_grid_is_handled() {
        let f = feather_coverage(&square(), 64.0);
        assert_eq!(f.width, 8 + 4 * 64);
        assert_eq!(f.height, 8 + 4 * 64);
        assert_eq!(f.data.len(), f.width as usize * f.height as usize);
        assert!(
            f.data
                .iter()
                .all(|value| value.is_finite() && *value >= 0.0),
            "超大半径下所有输出都必须有限且非负"
        );
    }

    /// **bbox 四周各外扩 `2 × radius`** ✓ —— **这是"外半边不会被切掉"的几何证据** ✓
    ///（第 343 轮：不外扩 ⇒ 假羽化 ✗；按 `radius` 扩 ⇒ 切掉外半边 ✗）。
    #[test]
    fn the_box_grows_by_twice_the_radius() {
        let f = feather_coverage(&square(), 2.0);
        assert_eq!(f.bbox.x, 100.0 - 4.0, "左边要外扩 2×2 ⇒ {}", f.bbox.x);
        assert_eq!(f.bbox.y, 200.0 - 4.0, "上边要外扩 2×2 ⇒ {}", f.bbox.y);
        assert_eq!(f.bbox.w, 8.0 + 8.0, "宽要加两个 2×2 ⇒ {}", f.bbox.w);
        assert_eq!(f.width, 16, "像素宽同理 ⇒ {}", f.width);
    }

    /// **外侧确实出现了非零** ✓ —— 羽化**溢出到形状之外**的直接证据 ✓
    ///（**只有这一侧能否证"只向内淡出"的假实现** ✗）。
    #[test]
    fn paint_spills_outside_the_original_box() {
        let f = feather_coverage(&square(), 2.0);
        let (ow, grow) = (f.width as usize, 4usize);
        // 原方块左上角在 (grow, grow) ✓ ⇒ **它左上方一格**（原形状之外 ✓）应当 > 0 ✓
        let outside = f.data[(grow - 1) * ow + (grow - 1)];
        assert!(outside > 0.0, "形状之外应收到溢出 ⇒ {outside}");
        assert!(outside < 1.0, "溢出处不该是满覆盖 ⇒ {outside}");
    }

    /// **中心仍是满覆盖** ✓（没把内部糊掉 ✓）。
    #[test]
    fn the_centre_stays_full() {
        let f = feather_coverage(&square(), 2.0);
        let (ow, grow) = (f.width as usize, 4usize);
        let centre = f.data[(grow + 4) * ow + (grow + 4)];
        assert!(centre > 0.9, "中心应接近满覆盖 ⇒ {centre}");
    }
}
