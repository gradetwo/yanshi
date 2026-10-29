//! 原生矢量笔触（路线甲 Phase 2 第一增量）：**脊线几何 + 原生直出 + 单笔光栅缓存**。
//!
//! **真相源是表示 A**（脊线 + 剖面：`StrokeGeometry` 的点列与压力 + `BrushSpec`
//! 的尺寸/硬度/流量等剖面参数，与 `draw_stroke` 落盘的对象 `data` 逐字对应 ✓）；
//! 轮廓（表示 B）永远是**派生**：
//! - 渲染时由引擎盖章路径从脊线重新光栅化（见 [`VectorRasterCache`]）；
//! - 导出 SVG 时由 [`export_stroke_native`] 解析地从脊线生成（路线 A 包络线），
//!   **不经过光栅场**——这正是它与 Phase 1 路线乙（先光栅化、再从 alpha 场
//!   提轮廓拟合）的区别 ✓。
//!
//! 本模块提供四件事：
//! 1. [`spine_hit`] / [`hit_test`]：按脊线距离 + 剖面半径的 stroke 级命中测试；
//! 2. [`erase_spine`]：矢量橡皮的脊线截断（擦除带内的脊线区段被切掉，
//!    一笔可被截成多段，每段保留原剖面参数）；
//! 3. [`export_stroke_native`]：原生矢量笔触的 SVG 直出（分类不过线时
//!    **显式**回退并给出机器可读原因，不静默降级 ✓）；
//! 4. [`VectorRasterCache`]：矢量图层的单笔光栅缓存——已提交笔触冻结成
//!    缓存位图，单笔新增/修改只光栅化该笔（「冻结前缀 + 活跃尾部增量」
//!    在工具面的落法 ✓），命中/盖章计数可观测。
//!
//! **与 `svg.rs` 的关系**：本模块**不改** Phase 1 的任何既有函数，只复用其
//! 公开件（dab 序列复刻、解析包络线、轮廓拟合、α_eff 校准公式）⇒
//! Phase 1 被上游修复时，本模块的 rebase 面只剩本文件与挂载点 ✓。

use crate::blend::BlendMode;
use crate::brush::{BrushSpec, StrokeAppearance, StrokeGeometry, StrokePoint, Symmetry};
use crate::buffer::Buffer;
use crate::svg::{self, Pt, StrokeExport, SvgDab};
use serde_json::Value;
use std::collections::{HashMap, HashSet, VecDeque};

/// 脊线上一点的**剖面半径**（与盖章路径 `stamp_samples_from` 的
/// `size_scale` 算法逐字一致：压力只缩放半径 ✓）。
pub fn spine_radius(brush: &BrushSpec, pressure: f64) -> f64 {
    let scale = if brush.pressure_size {
        let p = pressure.clamp(0.0, 1.0);
        brush.min_size_ratio + (1.0 - brush.min_size_ratio) * p
    } else {
        1.0
    };
    (brush.size / 2.0 * scale).max(0.0)
}

/// 笔触的**有效脊线**：`smooth` 时是渲染端实际使用的 Catmull-Rom 采样
/// （与盖章同源 ✓），否则是存储点列本身。
///
/// 命中/截断都作用在有效脊线上——否则平滑笔的命中范围会与画面对不上 ✗。
pub fn effective_spine(geometry: &StrokeGeometry) -> Vec<StrokePoint> {
    if geometry.smooth {
        crate::brush::catmull_rom_smooth(&geometry.points, crate::brush::SMOOTH_SUBDIVISIONS)
    } else {
        geometry.points.clone()
    }
}

/// 点 `q` 到线段 `a→b` 的最近点参数与距离（`t ∈ [0, 1]` ✓）。
fn segment_closest(a: Pt, b: Pt, q: Pt) -> (f64, Pt, f64) {
    let ab = (b.0 - a.0, b.1 - a.1);
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if len2 <= 1e-24 {
        0.0
    } else {
        (((q.0 - a.0) * ab.0 + (q.1 - a.1) * ab.1) / len2).clamp(0.0, 1.0)
    };
    let closest = (a.0 + ab.0 * t, a.1 + ab.1 * t);
    let d = (q.0 - closest.0, q.1 - closest.1);
    (t, closest, (d.0 * d.0 + d.1 * d.1).sqrt())
}

/// 两线段的最短距离（相交 ⇒ 0 ✓）。
fn segment_segment_distance(a: Pt, b: Pt, c: Pt, d: Pt) -> f64 {
    if segments_intersect(a, b, c, d) {
        return 0.0;
    }
    segment_closest(a, b, c)
        .2
        .min(segment_closest(a, b, d).2)
        .min(segment_closest(c, d, a).2)
        .min(segment_closest(c, d, b).2)
}

/// 两线段是否相交（含端点相触与共线重叠 ⇒ 距离 0 的判定用 ✓）。
fn segments_intersect(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    fn cross(o: Pt, p: Pt, q: Pt) -> f64 {
        (p.0 - o.0) * (q.1 - o.1) - (p.1 - o.1) * (q.0 - o.0)
    }
    fn on_segment(p: Pt, q: Pt, r: Pt) -> bool {
        // q 在线段 pr 上（共线前提下按包围盒判 ✓）。
        q.0 <= p.0.max(r.0) + 1e-12
            && q.0 >= p.0.min(r.0) - 1e-12
            && q.1 <= p.1.max(r.1) + 1e-12
            && q.1 >= p.1.min(r.1) - 1e-12
    }
    let d1 = cross(c, d, a);
    let d2 = cross(c, d, b);
    let d3 = cross(a, b, c);
    let d4 = cross(a, b, d);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1.abs() <= 1e-12 && on_segment(c, a, d))
        || (d2.abs() <= 1e-12 && on_segment(c, b, d))
        || (d3.abs() <= 1e-12 && on_segment(a, c, b))
        || (d4.abs() <= 1e-12 && on_segment(a, d, b))
}

/// 点到折线的最短距离（折线只有一点时退化为点距 ✓）。
fn polyline_distance(points: &[StrokePoint], q: Pt) -> f64 {
    if points.is_empty() {
        return f64::INFINITY;
    }
    if points.len() == 1 {
        let p = points[0];
        let d = (q.0 - p.x, q.1 - p.y);
        return (d.0 * d.0 + d.1 * d.1).sqrt();
    }
    let mut best = f64::INFINITY;
    for window in points.windows(2) {
        let (a, b) = (window[0], window[1]);
        let (_, _, dist) = segment_closest((a.x, a.y), (b.x, b.y), q);
        best = best.min(dist);
    }
    best
}

/// 一次命中测量的结果（取整条脊线上**余量最小**的采样点 ✓）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpineHit {
    /// 查询点到脊线的最短距离（像素 ✓）。
    pub distance: f64,
    /// 最近点处的剖面半径（按该点压力插值 ✓）。
    pub radius: f64,
    /// 余量 = 距离 − 半径（`≤ 0` 表示点落在笔触墨迹内 ✓）。
    pub clearance: f64,
}

/// 按脊线距离 + 剖面半径测量一笔与查询点的关系（stroke 级命中测试的内核 ✓）。
///
/// 脊线为空 ⇒ `None`（无可命中之物，调用方据此如实返回未命中 ✓）。
pub fn spine_hit(brush: &BrushSpec, geometry: &StrokeGeometry, query: Pt) -> Option<SpineHit> {
    let spine = effective_spine(geometry);
    if spine.is_empty() {
        return None;
    }
    let mut best: Option<SpineHit> = None;
    let consider = |distance: f64, pressure: f64, best: &mut Option<SpineHit>| {
        let radius = spine_radius(brush, pressure);
        let hit = SpineHit {
            distance,
            radius,
            clearance: distance - radius,
        };
        if best.is_none_or(|b: SpineHit| hit.clearance < b.clearance) {
            *best = Some(hit);
        }
    };
    if spine.len() == 1 {
        let p = spine[0];
        let d = (query.0 - p.x, query.1 - p.y);
        consider((d.0 * d.0 + d.1 * d.1).sqrt(), p.pressure, &mut best);
        return best;
    }
    for window in spine.windows(2) {
        let (a, b) = (window[0], window[1]);
        let (t, _, distance) = segment_closest((a.x, a.y), (b.x, b.y), query);
        let pressure = a.pressure + (b.pressure - a.pressure) * t;
        consider(distance, pressure, &mut best);
    }
    best
}

/// 命中判定：`clearance ≤ tolerance`（容差由调用方给，缺省 0 ✓）。
pub fn hit_test(brush: &BrushSpec, geometry: &StrokeGeometry, query: Pt, tolerance: f64) -> bool {
    spine_hit(brush, geometry, query).is_some_and(|hit| hit.clearance <= tolerance)
}

/// 脊线截断（矢量橡皮）的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct SpineErase {
    /// 截断后剩余的脊线段列（每段 ≥ 1 个点 ✓；空 = 整笔被擦除 ✓）。
    /// **未触及时**这里是原始存储点列（一段、逐点相同 ✓），见 `changed`。
    pub pieces: Vec<Vec<StrokePoint>>,
    /// 这一笔是否真的被改动（橡皮带完全没碰到 ⇒ `false`，
    /// 调用方据此**不重写对象**，保持其原子历史干净 ✓）。
    pub changed: bool,
    /// 被擦除的采样数（只统计细分采样过的段；见 [`erase_spine`] 的采样说明 ✓）。
    pub samples_erased: usize,
    /// 参与判定的采样总数（同上口径 ✓）。
    pub samples_total: usize,
}

/// **矢量橡皮：按脊线截断**——擦除轨迹（折线）半径 `eraser_radius` 扫过的带
/// （带宽 = 擦除半径 + 笔触自身剖面半径）内的脊线区段被切掉 ✓。
///
/// 算法：
/// - 与擦除带**肯定不相交**的脊线段（段到擦除折线的最短距离 > 带宽上限）
///   **逐字保留**原顶点——擦不到的笔触连点列都不动 ✓；
/// - 可能相交的段按 `≤ eraser_radius/2`（钳制在 `[0.1, 1.0]` px）细分采样，
///   逐采样判定 `dist(采样, 擦除折线) ≤ eraser_radius + 该点剖面半径`，
///   状态切换处用二分把切点定位到 `≤ 0.02` px，切点作为新段端点插入 ✓；
/// - 剩余采样按连续性分组为段；单点段合法（一枚 dab 的点笔 ✓）。
///
/// `smooth` 笔触先物化有效脊线再截断（截断结果应由调用方以 `smooth = false`
/// 落盘——采样已经把曲线物化了，再平滑会二次弯曲 ✗）。
pub fn erase_spine(
    brush: &BrushSpec,
    geometry: &StrokeGeometry,
    eraser: &[StrokePoint],
    eraser_radius: f64,
) -> SpineErase {
    let untouched = SpineErase {
        pieces: vec![geometry.points.clone()],
        changed: false,
        samples_erased: 0,
        samples_total: 0,
    };
    if geometry.points.is_empty() || eraser.is_empty() || eraser_radius < 0.0 {
        return untouched;
    }
    let spine = effective_spine(geometry);
    // 带宽判定函数：`f(p) = dist(p, 擦除折线) − (eraser_radius + 该点剖面半径)`，
    // `f ≤ 0` = 该点被擦除 ✓。
    let band = |p: StrokePoint| -> f64 {
        polyline_distance(eraser, (p.x, p.y)) - (eraser_radius + spine_radius(brush, p.pressure))
    };
    let mut samples_erased = 0usize;
    let mut samples_total = 0usize;

    // 单点笔（点 dab）：整笔只看那一点 ✓。
    if spine.len() == 1 {
        samples_total = 1;
        if band(spine[0]) <= 0.0 {
            samples_erased = 1;
            return SpineErase {
                pieces: Vec::new(),
                changed: true,
                samples_erased,
                samples_total,
            };
        }
        return untouched;
    }

    let mut pieces: Vec<Vec<StrokePoint>> = Vec::new();
    let mut run: Vec<StrokePoint> = Vec::new();
    let step = (eraser_radius * 0.5).clamp(0.1, 1.0);

    for window in spine.windows(2) {
        let (a, b) = (window[0], window[1]);
        let seg_len = {
            let d = (b.x - a.x, b.y - a.y);
            (d.0 * d.0 + d.1 * d.1).sqrt()
        };
        let r_max = spine_radius(brush, a.pressure).max(spine_radius(brush, b.pressure));
        // 段到擦除折线的最短距离（擦除折线逐段取 min ✓）。
        let mut seg_min_dist = f64::INFINITY;
        if eraser.len() == 1 {
            let e = eraser[0];
            seg_min_dist = segment_closest((a.x, a.y), (b.x, b.y), (e.x, e.y)).2;
        } else {
            for ewin in eraser.windows(2) {
                let (c, d) = (ewin[0], ewin[1]);
                seg_min_dist = seg_min_dist.min(segment_segment_distance(
                    (a.x, a.y),
                    (b.x, b.y),
                    (c.x, c.y),
                    (d.x, d.y),
                ));
            }
        }
        if seg_min_dist > eraser_radius + r_max {
            // 肯定擦不到：原顶点逐字进 run ✓。
            if run.is_empty() {
                run.push(a);
            }
            run.push(b);
            samples_total += 1;
            continue;
        }
        // 可能相交：细分采样 + 边界二分 ✓。
        let steps = ((seg_len / step).ceil() as usize).max(1);
        let point_at = |t: f64| StrokePoint {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            pressure: a.pressure + (b.pressure - a.pressure) * t,
        };
        let mut prev_t = 0.0f64;
        let mut prev_erased = band(a) <= 0.0;
        samples_total += 1;
        if prev_erased {
            samples_erased += 1;
        } else if run.is_empty() {
            run.push(a);
        }
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            let p = point_at(t);
            let erased = band(p) <= 0.0;
            samples_total += 1;
            if erased {
                samples_erased += 1;
            }
            if erased != prev_erased {
                // 二分定位切点（f 在 [prev_t, t] 上变号 ✓）。
                let (mut lo, mut hi) = (prev_t, t);
                for _ in 0..24 {
                    let mid = (lo + hi) / 2.0;
                    if (band(point_at(mid)) <= 0.0) == prev_erased {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let cut = point_at((lo + hi) / 2.0);
                if prev_erased {
                    // 擦除 → 保留：新段以切点起头 ✓。
                    run.push(cut);
                } else {
                    // 保留 → 擦除：当前段以切点收尾并封存 ✓。
                    run.push(cut);
                    if !run.is_empty() {
                        pieces.push(std::mem::take(&mut run));
                    }
                }
            }
            if !erased {
                run.push(p);
            }
            prev_t = t;
            prev_erased = erased;
        }
    }
    if !run.is_empty() {
        pieces.push(run);
    }
    // 一个采样都没擦中 ⇒ 这一笔实际未被触及：返回原始点列、changed = false，
    // 调用方不重写对象（细分采样产生的稠密点列只在真截断时才物化落盘 ✓）。
    if samples_erased == 0 {
        return untouched;
    }
    SpineErase {
        pieces,
        changed: true,
        samples_erased,
        samples_total,
    }
}

/// taper 尖端位置（与 `svg.rs` 的同名私有函数同算法——那边不公开，
/// 这里按公开的 `SvgDab` 字段复刻一份：起收笔处半径 `< 1.5px` 的极值点
/// ⇒ 拟合的强制分段点，防止三次拟合抹圆笔锋 ✓）。
fn native_taper_tips(clusters: &[Vec<SvgDab>]) -> Vec<Pt> {
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

/// **原生矢量笔触的 SVG 直出**：只走解析路线（dab 序列复刻 → 解析包络线 →
/// RDP/Schneider 拟合），**从不把笔触光栅化再提轮廓** ✓——这是它与
/// Phase 1 `svg::export_stroke`（路线乙，含 alpha 场兜底）的根本区别。
///
/// 分类（任一不过线 ⇒ `Raster` + 机器可读原因，调用方如实计数、不静默 ✓）：
/// 外观（曲线/动力学/纹理）启用、混合模式无 CSS 对应、抖动 > 0、对称镜像、
/// 软边（与 Phase 1 同判据：`hardness < 0.75` 且 fringe > 2px）、脊线自交
/// （包络线表达不了自交处的叠色）、任一簇拟合失败。
pub fn export_stroke_native(
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
    let Some(css_blend) = svg::blend_css(brush.blend_mode) else {
        return StrokeExport::Raster {
            reason: "blend_mode",
        };
    };
    let css_blend = Some(css_blend);
    if brush.jitter > 0.0 {
        return StrokeExport::Raster {
            reason: "native_jitter",
        };
    }
    if brush.symmetry != Symmetry::None {
        return StrokeExport::Raster {
            reason: "native_symmetry",
        };
    }
    let clusters = svg::stroke_dab_clusters(brush, geometry, canvas);
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
    // 软边判据与 Phase 1 逐字一致（硬轮廓的前提 ✓）：包络线是硬边轮廓，
    // 软边笔的羽化带它表达不了 ⇒ 如实回退 ✓。
    let fringe = r_max * (1.0 - brush.hardness);
    if brush.hardness < 0.75 && fringe > 2.0 {
        return StrokeExport::Raster {
            reason: "soft_edge",
        };
    }
    let spine = svg::stroke_spine(geometry);
    if svg::spine_self_intersections(&spine) >= 1 {
        return StrokeExport::Raster {
            reason: "native_self_intersection",
        };
    }
    let forced = native_taper_tips(&clusters);
    let mut contours = Vec::new();
    for cluster in &clusters {
        let loop_pts = svg::envelope_contour(cluster);
        // 容差按**本簇** dab 半径的中位数调度（与 Phase 1 路线 A 逐字一致 ✓）：
        // 本函数曾是全仓唯一还走旧固定容差入口（`fit_closed_contour`，
        // ε_fit = 0.6px）的生产调用——对 r = 1.5 的分隔线级细线那是半径的
        // 40%，轮廓偏差实测达 0.57px、过不了 validate 的 P95 线
        // （`max(0.35, 0.08r)`）✗；`fit_closed_contour_r` 在 r ≥ 8.6 时与
        // 旧入口逐字一致 ⇒ 粗线输出不回归 ✓。
        let mut radii: Vec<f64> = cluster.iter().map(|d| d.radius).collect();
        radii.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let cluster_radius = radii
            .get(radii.len() / 2)
            .copied()
            .unwrap_or(brush.size / 2.0);
        match svg::fit_closed_contour_r(&loop_pts, &forced, cluster_radius) {
            Some(fitted) => contours.push(fitted),
            None => {
                return StrokeExport::Raster {
                    reason: "native_fit_failed",
                };
            }
        }
    }
    if contours.is_empty() {
        return StrokeExport::Raster {
            reason: "native_fit_failed",
        };
    }
    StrokeExport::Vector {
        layers: vec![svg::ExportLayer {
            contours,
            fill_opacity: svg::effective_alpha(brush),
        }],
        css_blend,
    }
}

/// 对象数据的指纹（FNV-1a over 规范 JSON 串 + 变换参数的位模式 ✓）。
///
/// 用途：[`VectorRasterCache`] 的失效键——对象数据或变换任一变化 ⇒
/// 指纹变化 ⇒ 该笔重栅格化；两者都不变 ⇒ 缓存必然仍有效（同一份输入
/// 走同一条盖章路径 ⇒ 同一份像素 ✓，由缓存测试以逐像素断言守住 ✓）。
pub fn stroke_fingerprint(data: &Value, transform: &[f64]) -> u64 {
    fn write_hash(hash: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *hash ^= u64::from(*byte);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    write_hash(&mut hash, data.to_string().as_bytes());
    for value in transform {
        write_hash(&mut hash, &value.to_bits().to_le_bytes());
    }
    hash
}

/// [`VectorRasterCache`] 的可观测计数（增量性的证据面 ✓）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VectorRasterStats {
    /// 缓存命中次数（这一笔**没有**重新盖章、只被合成 ✓）。
    pub hits: u64,
    /// 缓存未命中次数（这一笔被重新盖章光栅化 ✓）。
    pub misses: u64,
    /// 累计实际盖章的 dab 数（只在未命中时增长 ✓）。
    pub dabs_stamped: u64,
    /// 当前缓存条目数。
    pub entries: usize,
    /// 累计因字节上限被 LRU 淘汰的条目数（反馈期 3 ✓）。
    pub evicted: u64,
    /// 当前缓存的单笔光栅字节之和（按缓冲容量计，与 [`VectorRasterCache::bytes`] 一致 ✓）。
    pub bytes: usize,
}

/// 单笔光栅缓存字节上限的环境变量（反馈期 3 ✓；单位兆字节）。
///
/// 参照批水位 `YANSHI_BATCH_RSS_LIMIT_MB` 的先例：环境变量覆盖、缺省不变、
/// 非法值静默回落缺省。缺省 = 不设上限（与旧行为一致：只增不减，close 再 open
/// 才释放）；配了正整数后缓存按 LRU 逐出，被逐条目下次渲染按需重算、像素不变。
pub const VECTOR_RASTER_CACHE_ENV_VAR: &str = "YANSHI_VECTOR_RASTER_CACHE_MB";

/// 缺省上限：无上限（旧行为 ✓）。
pub const VECTOR_RASTER_CACHE_DEFAULT_BYTES: usize = usize::MAX;

/// 解析上限文本（单位 MB）：空、非法、0 ⇒ 缺省（无上限）；正整数 ⇒ MB 换算字节。
/// 纯函数（不读环境），供测试直接断言解析口径 ✓。
pub fn parse_vector_raster_cache_bytes(text: Option<&str>) -> usize {
    text.and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|mb| *mb >= 1)
        .map(|mb| usize::try_from(mb.saturating_mul(1024 * 1024)).unwrap_or(usize::MAX))
        .unwrap_or(VECTOR_RASTER_CACHE_DEFAULT_BYTES)
}

/// 从环境读上限（文档打开时由 [`VectorRasterCache::new`] 调一次 ✓）。
fn vector_raster_cache_bytes_from_env() -> usize {
    parse_vector_raster_cache_bytes(std::env::var(VECTOR_RASTER_CACHE_ENV_VAR).ok().as_deref())
}

struct CacheEntry {
    fingerprint: u64,
    buffer: Buffer,
}

/// **矢量图层的单笔光栅缓存**（冻结前缀 + 活跃尾部增量的落点 ✓）。
///
/// 以 `(图层 id, 对象 id)` 为键缓存每笔的光栅（紧凑包围盒缓冲、文档坐标
/// 原点 ✓）；渲染矢量图层时，已提交笔触从缓存合成、只有新增/被改的笔
/// 才重新走引擎盖章路径。图层/文档未变 ⇒ 第二遍渲染盖章数为 0 ✓
/// （由测试以 [`VectorRasterStats`] 断言 ✓）。
///
/// **字节上限与 LRU 逐出（反馈期 3 ✓）**：旧实现是无界 `HashMap`——每新增
/// 一笔就多一表项，只能靠删对象或关文档释放，这正是“内存只增不减、close
/// 再 open 才释放”的无界分量（tile／区域／位图／缓冲池早已有界 LRU）。
/// 现加 `max_bytes`：缺省无上限（旧行为不变）；`YANSHI_VECTOR_RASTER_CACHE_MB`
/// 配了正整数后，插入与调小上限时都按 LRU（`order` 队首最旧）逐出，直到
/// 字节达标或只剩刚放进的一块（单块超预算仍保留，与区域缓存同例——缓存
/// “有”比“全无”好，超预算的块不挤光别的块）。逐出只影响速度：被逐条目
/// 下次渲染走同一条盖章路径重算（指纹门控不变），像素与冷路径逐位相同，
/// 绝不静默丢像素 ✓。
pub struct VectorRasterCache {
    entries: HashMap<(String, String), CacheEntry>,
    /// LRU 顺序：队首最旧、队尾最新；命中与插入都把键移到队尾 ✓。
    order: VecDeque<(String, String)>,
    /// 当前所有条目缓冲容量之和（`put`／逐出／`retain`／`clear` 同步维护 ✓）。
    bytes: usize,
    /// 字节上限（缺省 [`VECTOR_RASTER_CACHE_DEFAULT_BYTES`] = 无上限 ✓）。
    max_bytes: usize,
    hits: u64,
    misses: u64,
    dabs_stamped: u64,
    evicted: u64,
}

impl Default for VectorRasterCache {
    /// 缺省 = 环境上限（未配 ⇒ 无上限，旧行为 ✓）。
    fn default() -> Self {
        Self::with_byte_budget(vector_raster_cache_bytes_from_env())
    }
}

impl VectorRasterCache {
    /// 新建空缓存（上限取环境变量，未配 ⇒ 无上限 ✓）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 新建空缓存并显式给字节上限（测试与程序化调上限用；0 ⇒ 夹到 1 ✓）。
    pub fn with_byte_budget(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            max_bytes: max_bytes.max(1),
            hits: 0,
            misses: 0,
            dabs_stamped: 0,
            evicted: 0,
        }
    }

    /// 当前字节上限。
    pub const fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    /// 调整字节上限并立即按 LRU 逐出到新上限内（单块超预算仍保留一块 ✓）。
    pub fn set_max_bytes(&mut self, max_bytes: usize) {
        self.max_bytes = max_bytes.max(1);
        self.evict_to_fit(None);
    }

    /// 把键移到 LRU 队尾（最新 ✓）。
    fn touch(&mut self, key: &(String, String)) {
        self.order.retain(|old| old != key);
        self.order.push_back(key.clone());
    }

    /// 按 LRU 逐出到字节上限内；`keep` 指定的键永不被逐（刚放进的那块 ✓）。
    /// 只剩一块时停（单块超预算仍保留 ✓）。返回逐出数。
    fn evict_to_fit(&mut self, keep: Option<&(String, String)>) -> usize {
        let mut evicted = 0;
        while self.bytes > self.max_bytes && self.entries.len() > 1 {
            let Some(victim) = self.order.pop_front() else {
                break;
            };
            if Some(&victim) == keep {
                self.order.push_back(victim);
                break;
            }
            if let Some(old) = self.entries.remove(&victim) {
                self.bytes = self.bytes.saturating_sub(old.buffer.capacity_bytes());
                self.evicted += 1;
                evicted += 1;
            }
        }
        evicted
    }

    /// 当前计数快照。
    pub fn stats(&self) -> VectorRasterStats {
        VectorRasterStats {
            hits: self.hits,
            misses: self.misses,
            dabs_stamped: self.dabs_stamped,
            entries: self.entries.len(),
            evicted: self.evicted,
            bytes: self.bytes,
        }
    }

    /// 当前缓存的单笔光栅字节之和（按缓冲容量计；RSS 修复 paint-rss：
    /// C2 预算闸估算口径的一部分）。
    pub const fn bytes(&self) -> usize {
        self.bytes
    }

    /// 取一笔的光栅：指纹一致 ⇒ 命中直接返回缓存（不盖章 ✓）；
    /// 否则走引擎盖章路径（[`svg::render_stroke_buffer`]，与渲染器同源 ✓）
    /// 重新光栅化并入缓存。笔完全在画布外 ⇒ `None`（无可缓存之物 ✓）。
    // **`key` 把 `layer_id` 与 `object_id` 合成一个参数** ✓（第 1421 轮：clippy 的
    //   `too_many_arguments` 上限是 7，而这两个**本来就是同一个缓存键的两半** ⇒
    //   合成 `(&str, &str)` 比加 `#[allow]` 更诚实：它让「键是两段」这件事在签名里就看得见 ✓）
    pub fn get_or_raster(
        &mut self,
        key: (&str, &str),
        fingerprint: u64,
        brush: &BrushSpec,
        geometry: &StrokeGeometry,
        appearance: Option<&StrokeAppearance>,
        canvas: (u32, u32),
    ) -> Option<&Buffer> {
        let key = (key.0.to_owned(), key.1.to_owned());
        if self
            .entries
            .get(&key)
            .is_some_and(|entry| entry.fingerprint == fingerprint)
        {
            self.hits += 1;
            self.touch(&key);
            return self.entries.get(&key).map(|entry| &entry.buffer);
        }
        let (buffer, _bbox) = svg::render_stroke_buffer(brush, geometry, appearance, canvas)?;
        self.misses += 1;
        self.dabs_stamped += svg::stroke_dab_clusters(brush, geometry, canvas)
            .iter()
            .map(Vec::len)
            .sum::<usize>() as u64;
        // 同键旧条目（指纹已变 ⇒ 重算）先扣旧字节，账面不重复计 ✓。
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.buffer.capacity_bytes());
        }
        self.bytes = self.bytes.saturating_add(buffer.capacity_bytes());
        self.entries.insert(
            key.clone(),
            CacheEntry {
                fingerprint,
                buffer,
            },
        );
        self.touch(&key);
        // 超上限 ⇒ 按 LRU 逐出旧条目（刚放进的这块不逐 ✓）；被逐条目下次
        // 渲染走同一条盖章路径重算，像素不变 ✓。
        self.evict_to_fit(Some(&key));
        self.entries.get(&key).map(|entry| &entry.buffer)
    }

    /// 淘汰某图层中不在 `alive` 集合里的对象条目（对象被删/被擦除替换时
    /// 由调用方在渲染前同步 ✓，防缓存无限增长与幽灵条目 ✓）。
    pub fn retain_layer_objects(&mut self, layer_id: &str, alive: &HashSet<String>) {
        let removed: Vec<(String, String)> = self
            .entries
            .keys()
            .filter(|(layer, object)| layer == layer_id && !alive.contains(object))
            .cloned()
            .collect();
        for key in &removed {
            if let Some(old) = self.entries.remove(key) {
                self.bytes = self.bytes.saturating_sub(old.buffer.capacity_bytes());
            }
        }
        if !removed.is_empty() {
            self.order.retain(|key| !removed.contains(key));
        }
    }

    /// 清空（文档切换/关闭时 ✓）。计数（命中／未命中／逐出）是累计证据面，
    /// 清空不重置，只清条目与字节账 ✓。
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
}

/// 混合模式是否可被原生矢量笔触使用（写侧校验与导出分类共用同一张表：
/// 有 CSS 对应即可 ✓；`Add`/`Subtract` 无对应 ⇒ 不可 ✓）。
pub fn native_blend_supported(mode: BlendMode) -> bool {
    svg::blend_css(mode).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blend::BlendMode;
    use serde_json::json;

    fn brush(size: f64) -> BrushSpec {
        BrushSpec {
            size,
            hardness: 1.0,
            flow: 1.0,
            opacity: 1.0,
            spacing: 0.25,
            color: [0.5, 0.05, 0.05, 1.0],
            jitter: 0.0,
            pressure_size: false,
            min_size_ratio: 0.25,
            blend_mode: BlendMode::Normal,
            seed: 0,
            dash: None,
            symmetry: Symmetry::None,
        }
    }

    fn pt(x: f64, y: f64) -> StrokePoint {
        StrokePoint {
            x,
            y,
            pressure: 1.0,
        }
    }

    fn line(points: &[(f64, f64)]) -> StrokeGeometry {
        StrokeGeometry {
            points: points.iter().map(|&(x, y)| pt(x, y)).collect(),
            smooth: false,
        }
    }

    #[test]
    fn spine_radius_matches_stamp_pressure_scale() {
        let mut b = brush(40.0);
        b.pressure_size = true;
        // 恒等式：半径 = size/2 × (min_ratio + (1 − min_ratio)·p) ✓。
        assert_eq!(spine_radius(&b, 1.0), 20.0);
        assert_eq!(spine_radius(&b, 0.0), 5.0);
        assert_eq!(spine_radius(&b, 0.5), 12.5);
        // 关掉压力尺寸 ⇒ 半径与压力无关 ✓。
        b.pressure_size = false;
        assert_eq!(spine_radius(&b, 0.0), 20.0);
        assert_eq!(spine_radius(&b, 1.0), 20.0);
    }

    #[test]
    fn hit_test_measures_clearance_against_profile_radius() {
        let b = brush(20.0); // 半径 10 ✓
        let g = line(&[(0.0, 0.0), (100.0, 0.0)]);
        let center = spine_hit(&b, &g, (50.0, 0.0)).expect("有点必有命中测量");
        assert_eq!(center.distance, 0.0);
        assert_eq!(center.radius, 10.0);
        assert_eq!(center.clearance, -10.0);
        // 恰在墨迹边缘：clearance = 0 ⇒ 命中（容差 0 ✓）。
        assert!(hit_test(&b, &g, (50.0, 10.0), 0.0));
        // 边缘外 1px：容差 0 不命中、容差 1 命中（关系断言 ✓）。
        assert!(!hit_test(&b, &g, (50.0, 11.0), 0.0));
        assert!(hit_test(&b, &g, (50.0, 11.0), 1.0));
        // 端点外侧按端点距离算（不是无限延长线 ✓）：(−10, 0) 距端点 10 = 半径 ⇒ 命中；
        // (−10, 5) 距端点 √125 > 10 ⇒ 不命中（延长线距离只有 5，会误命中 ✗）。
        assert!(hit_test(&b, &g, (-10.0, 0.0), 0.0));
        assert!(!hit_test(&b, &g, (-10.0, 5.0), 0.0));
    }

    #[test]
    fn hit_test_radius_follows_pressure_along_spine() {
        let mut b = brush(20.0);
        b.pressure_size = true; // 半径随压力在 [2.5, 10] 间变 ✓
        let g = StrokeGeometry {
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
        // 起点处半径只有 2.5：距脊线 3px ⇒ clearance = +0.5 ⇒ 不命中 ✓。
        let at_start = spine_hit(&b, &g, (0.0, 3.0)).expect("测量");
        assert!((at_start.radius - 2.5).abs() < 1e-9);
        assert!(!hit_test(&b, &g, (0.0, 3.0), 0.0));
        // 终点处半径 10：同样的 3px 偏移 ⇒ 命中 ✓（同一偏移、两端结论相反 ⇒
        // 断言测的确实是剖面半径而不是固定值 ✓）。
        assert!(hit_test(&b, &g, (100.0, 3.0), 0.0));
    }

    #[test]
    fn erase_miss_leaves_stroke_byte_identical() {
        let b = brush(10.0);
        let g = line(&[(0.0, 0.0), (100.0, 0.0), (200.0, 10.0)]);
        let eraser = vec![pt(0.0, 100.0), pt(200.0, 100.0)];
        let outcome = erase_spine(&b, &g, &eraser, 5.0);
        assert!(!outcome.changed);
        assert_eq!(outcome.samples_erased, 0);
        // 未触及 ⇒ pieces 就是原始点列本身（逐点相等 ⇒ 对象不会被重写 ✓）。
        assert_eq!(outcome.pieces, vec![g.points.clone()]);
    }

    #[test]
    fn erase_cuts_stroke_into_two_pieces_at_band_edges() {
        let b = brush(10.0); // 笔半径 5 ✓
        let g = line(&[(0.0, 0.0), (200.0, 0.0)]);
        // 擦除带：竖线 x=100、半径 5 ⇒ 带内 |x − 100| ≤ 5 + 5 = 10 ✓。
        let eraser = vec![pt(100.0, -30.0), pt(100.0, 30.0)];
        let outcome = erase_spine(&b, &g, &eraser, 5.0);
        assert!(outcome.changed);
        assert_eq!(
            outcome.pieces.len(),
            2,
            "应被截成两段：{:?}",
            outcome.pieces
        );
        assert!(outcome.samples_erased > 0);
        assert!(outcome.samples_total > outcome.samples_erased);
        let (first, second) = (&outcome.pieces[0], &outcome.pieces[1]);
        // 两段都在原脊线上（y ≡ 0 ✓），缺口跨过 x=100、宽 ≈ 20（±1 容差 ✓）。
        for piece in [first, second] {
            for p in piece {
                assert!(p.y.abs() < 1e-9, "截断点必须落在原脊线上：{p:?}");
            }
        }
        assert_eq!(first[0], pt(0.0, 0.0));
        assert_eq!(second[second.len() - 1], pt(200.0, 0.0));
        let gap_start = first[first.len() - 1].x;
        let gap_end = second[0].x;
        assert!(gap_start < 100.0 && gap_end > 100.0);
        assert!(
            (gap_start - 90.0).abs() <= 1.0,
            "左切点应在 90 附近：{gap_start}"
        );
        assert!(
            (gap_end - 110.0).abs() <= 1.0,
            "右切点应在 110 附近：{gap_end}"
        );
    }

    #[test]
    fn erase_keeps_untouched_segments_verbatim() {
        let b = brush(10.0);
        let g = line(&[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)]);
        // 只擦第二段中部（带 |x − 150| ≤ 10）；第一段连同中点顶点必须逐字保留 ✓。
        let eraser = vec![pt(150.0, -20.0), pt(150.0, 20.0)];
        let outcome = erase_spine(&b, &g, &eraser, 5.0);
        assert!(outcome.changed);
        assert_eq!(outcome.pieces.len(), 2);
        assert_eq!(outcome.pieces[0][0], pt(0.0, 0.0));
        assert!(
            outcome.pieces[0].contains(&pt(100.0, 0.0)),
            "未触及段的原顶点必须逐字保留：{:?}",
            outcome.pieces[0]
        );
        let gap_start = outcome.pieces[0][outcome.pieces[0].len() - 1].x;
        let gap_end = outcome.pieces[1][0].x;
        assert!((gap_start - 140.0).abs() <= 1.0);
        assert!((gap_end - 160.0).abs() <= 1.0);
    }

    #[test]
    fn erase_covering_whole_stroke_removes_it() {
        let b = brush(10.0);
        let g = line(&[(0.0, 0.0), (50.0, 0.0)]);
        let eraser = vec![pt(25.0, -10.0), pt(25.0, 10.0)];
        let outcome = erase_spine(&b, &g, &eraser, 30.0);
        assert!(outcome.changed);
        assert!(outcome.pieces.is_empty());
        // 全擦 ⇒ 参与判定的每个采样都被擦（恒等式 ✓）。
        assert!(outcome.samples_total > 0);
        assert_eq!(outcome.samples_erased, outcome.samples_total);
    }

    #[test]
    fn erase_single_point_stroke() {
        let b = brush(10.0);
        let g = line(&[(10.0, 10.0)]);
        // 擦除点落在带内 ⇒ 整笔消失 ✓。
        let hit = erase_spine(&b, &g, &[pt(12.0, 10.0)], 5.0);
        assert!(hit.changed && hit.pieces.is_empty());
        // 擦除点远离 ⇒ 原样 ✓。
        let miss = erase_spine(&b, &g, &[pt(100.0, 100.0)], 5.0);
        assert!(!miss.changed);
        assert_eq!(miss.pieces, vec![g.points.clone()]);
    }

    #[test]
    fn native_export_straight_stroke_passes_engine_validation() {
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let appearance = StrokeAppearance::default();
        let exported = export_stroke_native(&b, &g, &appearance, (200, 100));
        let StrokeExport::Vector { layers, .. } = exported else {
            panic!("直出应成功：{exported:?}");
        };
        assert_eq!(layers.len(), 1);
        // opacity = 1 且 dab 全重叠 ⇒ α_eff 恒等式 = 1 ✓。
        assert!((layers[0].fill_opacity - 1.0).abs() < 1e-12);
        assert!(!layers[0].contours.is_empty());
        // 用 Phase 1 的验收度量（引擎实渲为参考）验原生轮廓：不过线就是红 ✓。
        let refs: Vec<(&[svg::FittedContour], f64)> = layers
            .iter()
            .map(|l| (l.contours.as_slice(), l.fill_opacity))
            .collect();
        let metrics = svg::validate_stroke(&b, &g, Some(&appearance), (200, 100), &refs)
            .expect("验收应可计算");
        assert!(
            metrics.passes(),
            "原生轮廓未过验收线：E_interior={} E_geom={}",
            metrics.e_interior,
            metrics.e_geom
        );
    }

    #[test]
    fn native_export_rejects_what_it_cannot_represent() {
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let appearance = StrokeAppearance::default();
        // 无 CSS 对应的混合模式 ⇒ blend_mode ✓。
        let mut b = brush(20.0);
        b.blend_mode = BlendMode::Add;
        assert_eq!(
            export_stroke_native(&b, &g, &appearance, (200, 100)),
            StrokeExport::Raster {
                reason: "blend_mode"
            }
        );
        // 抖动 ⇒ native_jitter ✓。
        let mut b = brush(20.0);
        b.jitter = 1.0;
        assert_eq!(
            export_stroke_native(&b, &g, &appearance, (200, 100)),
            StrokeExport::Raster {
                reason: "native_jitter"
            }
        );
        // 对称镜像 ⇒ native_symmetry ✓（镜像半不在脊线里 ✗）。
        let mut b = brush(20.0);
        b.symmetry = Symmetry::Vertical;
        assert_eq!(
            export_stroke_native(&b, &g, &appearance, (200, 100)),
            StrokeExport::Raster {
                reason: "native_symmetry"
            }
        );
        // 软边（hardness 0.5、size 40 ⇒ fringe = 10 > 2）⇒ soft_edge ✓。
        let mut b = brush(40.0);
        b.hardness = 0.5;
        assert_eq!(
            export_stroke_native(&b, &g, &appearance, (200, 100)),
            StrokeExport::Raster {
                reason: "soft_edge"
            }
        );
        // 自交脊线（X 形）⇒ native_self_intersection ✓。
        let b = brush(20.0);
        let cross = line(&[(20.0, 20.0), (180.0, 80.0), (20.0, 80.0), (180.0, 20.0)]);
        assert_eq!(
            export_stroke_native(&b, &cross, &appearance, (200, 100)),
            StrokeExport::Raster {
                reason: "native_self_intersection"
            }
        );
    }

    #[test]
    fn raster_cache_freezes_committed_strokes() {
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let mut cache = VectorRasterCache::new();
        let fp = stroke_fingerprint(&json!({"size": 20.0, "opacity": 1.0}), &[]);
        let first_pixels: Vec<f32> = {
            let buffer = cache
                .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
                .expect("画布内笔触应可光栅化");
            buffer.pixels().to_vec()
        };
        let after_first = cache.stats();
        assert_eq!(after_first.misses, 1);
        assert_eq!(after_first.hits, 0);
        assert!(after_first.dabs_stamped > 0);
        // 第二遍同指纹：命中、盖章数不增、像素逐位相同（恒等式 ✓）。
        {
            let buffer = cache
                .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
                .expect("缓存应可取");
            assert_eq!(buffer.pixels(), first_pixels.as_slice());
        }
        let after_second = cache.stats();
        assert_eq!(after_second.hits, 1);
        assert_eq!(after_second.misses, 1);
        assert_eq!(after_second.dabs_stamped, after_first.dabs_stamped);
        // 指纹变（数据被改）⇒ 重新盖章、dab 数再增一份 ✓。
        let fp2 = stroke_fingerprint(&json!({"size": 20.0, "opacity": 0.5}), &[]);
        assert_ne!(fp, fp2);
        cache
            .get_or_raster(("L1", "obj1"), fp2, &b, &g, None, (200, 100))
            .expect("应可光栅化");
        let after_third = cache.stats();
        assert_eq!(after_third.misses, 2);
        assert_eq!(
            after_third.dabs_stamped,
            after_first.dabs_stamped * 2,
            "同笔重栅格化应再盖同样多的 dab"
        );
        // 对象消失 ⇒ 条目被淘汰 ✓。
        cache.retain_layer_objects("L1", &HashSet::new());
        assert_eq!(cache.stats().entries, 0);
    }

    #[test]
    fn raster_cache_budget_parses_like_batch_watermark() {
        // 空／非法／0 ⇒ 缺省（无上限）；正整数 ⇒ MB 换算字节（批水位同例 ✓）。
        assert_eq!(parse_vector_raster_cache_bytes(None), usize::MAX);
        assert_eq!(parse_vector_raster_cache_bytes(Some("")), usize::MAX);
        assert_eq!(parse_vector_raster_cache_bytes(Some("abc")), usize::MAX);
        assert_eq!(parse_vector_raster_cache_bytes(Some("0")), usize::MAX);
        assert_eq!(
            parse_vector_raster_cache_bytes(Some("64")),
            64 * 1024 * 1024
        );
        assert_eq!(
            parse_vector_raster_cache_bytes(Some(" 128 ")),
            128 * 1024 * 1024
        );
    }

    #[test]
    fn raster_cache_evicts_lru_within_budget_and_rerasters_identically() {
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let fp = stroke_fingerprint(&json!({"size": 20.0, "opacity": 1.0}), &[]);
        // 一笔约 77 KiB（186×26 像素 × RGBA f32）；预算只容两笔 ✓。
        let mut cache = VectorRasterCache::with_byte_budget(160_000);
        let first_pixels: Vec<f32> = {
            let buffer = cache
                .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
                .expect("画布内笔触应可光栅化");
            buffer.pixels().to_vec()
        };
        let one_entry_bytes = cache.bytes();
        assert!(one_entry_bytes > 0);
        cache
            .get_or_raster(("L1", "obj2"), fp, &b, &g, None, (200, 100))
            .expect("第二笔应可光栅化");
        assert_eq!(cache.stats().entries, 2);
        // 第三笔 ⇒ 最旧（obj1）被逐出，字节回到上限内 ✓。
        cache
            .get_or_raster(("L1", "obj3"), fp, &b, &g, None, (200, 100))
            .expect("第三笔应可光栅化");
        let after_evict = cache.stats();
        assert_eq!(after_evict.entries, 2);
        assert!(after_evict.evicted >= 1);
        assert!(after_evict.bytes <= 160_000);
        // 被逐条目再取 ⇒ 未命中、重算、像素与逐出前逐位相同（只慢不坏图 ✓）。
        let misses_before = cache.stats().misses;
        let buffer = cache
            .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
            .expect("被逐笔触应可重算");
        assert_eq!(buffer.pixels(), first_pixels.as_slice());
        assert_eq!(cache.stats().misses, misses_before + 1);
    }

    #[test]
    fn raster_cache_evicts_least_recently_used_not_oldest_inserted() {
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let fp = stroke_fingerprint(&json!({"size": 20.0, "opacity": 1.0}), &[]);
        let mut cache = VectorRasterCache::with_byte_budget(160_000);
        for obj in ["obj1", "obj2"] {
            cache
                .get_or_raster(("L1", obj), fp, &b, &g, None, (200, 100))
                .expect("笔触应可光栅化");
        }
        // 再命中 obj1 ⇒ 它变最新；下一笔应逐出 obj2（LRU），不是 obj1 ✓。
        cache
            .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
            .expect("命中应可取");
        cache
            .get_or_raster(("L1", "obj3"), fp, &b, &g, None, (200, 100))
            .expect("第三笔应可光栅化");
        assert_eq!(cache.stats().evicted, 1);
        let hits_before = cache.stats().hits;
        cache
            .get_or_raster(("L1", "obj1"), fp, &b, &g, None, (200, 100))
            .expect("obj1 应仍在缓存");
        assert_eq!(cache.stats().hits, hits_before + 1, "obj1 被命中才算 LRU");
        let misses_before = cache.stats().misses;
        cache
            .get_or_raster(("L1", "obj2"), fp, &b, &g, None, (200, 100))
            .expect("obj2 应已被逐出、可重算");
        assert_eq!(cache.stats().misses, misses_before + 1);
    }

    #[test]
    fn raster_cache_set_max_bytes_shrinks_live_cache_and_keeps_accounting() {
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let fp = stroke_fingerprint(&json!({"size": 20.0, "opacity": 1.0}), &[]);
        let mut cache = VectorRasterCache::with_byte_budget(usize::MAX);
        assert_eq!(cache.max_bytes(), usize::MAX, "缺省口径 = 无上限");
        for obj in ["obj1", "obj2", "obj3"] {
            cache
                .get_or_raster(("L1", obj), fp, &b, &g, None, (200, 100))
                .expect("笔触应可光栅化");
        }
        assert_eq!(cache.stats().evicted, 0, "无上限时不逐出");
        assert_eq!(cache.stats().entries, 3);
        // 调小上限 ⇒ 立即逐出到上限内（恒等式：字节 ≤ 上限且条目减少 ✓）。
        cache.set_max_bytes(160_000);
        assert!(cache.bytes() <= 160_000);
        assert_eq!(cache.stats().entries, 2);
        assert!(cache.stats().evicted >= 1);
        // 指纹替换同键 ⇒ 条目数不变、字节不重复计 ✓。
        let bytes_before = cache.bytes();
        let fp2 = stroke_fingerprint(&json!({"size": 20.0, "opacity": 0.5}), &[]);
        cache
            .get_or_raster(("L1", "obj3"), fp2, &b, &g, None, (200, 100))
            .expect("指纹变应重算");
        assert_eq!(cache.stats().entries, 2);
        assert_eq!(
            cache.bytes(),
            bytes_before,
            "同几何同笔尺 ⇒ 替换前后字节应相等（不双倍计）"
        );
    }

    #[test]
    fn fingerprint_is_stable_and_sensitive() {
        let data = json!({"size": 20.0, "points": [[0, 0], [10, 10]]});
        // 同输入同指纹（确定性 ✓）；任一字段/变换变化 ⇒ 指纹变化 ✓。
        assert_eq!(
            stroke_fingerprint(&data, &[1.0, 0.0]),
            stroke_fingerprint(&data, &[1.0, 0.0])
        );
        assert_ne!(
            stroke_fingerprint(&data, &[1.0, 0.0]),
            stroke_fingerprint(
                &json!({"size": 21.0, "points": [[0, 0], [10, 10]]}),
                &[1.0, 0.0]
            )
        );
        assert_ne!(
            stroke_fingerprint(&data, &[1.0, 0.0]),
            stroke_fingerprint(&data, &[2.0, 0.0])
        );
    }

    #[test]
    fn native_export_thin_line_contour_stays_on_capsule_across_phases() {
        // 回归（2026-10-21 logo 实验渲染级对比抓到的真缺陷）：size 3 /
        // spacing 0.1 的两点直线（logo 横版分隔线的构造）在原生直出里
        // 畸变成水滴形——本函数当时是全仓唯一还走旧固定容差入口
        // （`fit_closed_contour`、ε_fit = 0.6px = r 的 40%）的生产调用，
        // 轮廓偏差随绝对位置的浮点相位漂移（旧分支构建实测：同一条线在
        // y0 = 88 处拟合控制点被拉出 −29.37px、渲回墨宽 3 → 14px；
        // y0 = 68 处却正常）。判据是胶囊边界恒等式：采样拟合轮廓、逐点
        // 量「到脊线段的距离 − r」，全相位 ≤ 0.25px（修复前实测最坏
        // 0.57px ⇒ 本测试在旧入口上必红 ✓；修复后最坏 0.13px ✓）。
        let appearance = StrokeAppearance::default();
        let capsule_dev = |b: &BrushSpec, g: &StrokeGeometry, canvas: (u32, u32), r: f64| {
            let exported = export_stroke_native(b, g, &appearance, canvas);
            let StrokeExport::Vector { layers, .. } = exported else {
                panic!("细线应可直出：{exported:?}");
            };
            let a = (g.points[0].x, g.points[0].y);
            let z = (
                g.points[g.points.len() - 1].x,
                g.points[g.points.len() - 1].y,
            );
            let mut worst = 0.0f64;
            for layer in &layers {
                for c in &layer.contours {
                    let mut cur = c.start;
                    for cu in &c.cubics {
                        for i in 1..=40 {
                            let t = f64::from(i) / 40.0;
                            let mt = 1.0 - t;
                            let p = (
                                mt * mt * mt * cur.0
                                    + 3.0 * mt * mt * t * cu.p1.0
                                    + 3.0 * mt * t * t * cu.p2.0
                                    + t * t * t * cu.p3.0,
                                mt * mt * mt * cur.1
                                    + 3.0 * mt * mt * t * cu.p1.1
                                    + 3.0 * mt * t * t * cu.p2.1
                                    + t * t * t * cu.p3.1,
                            );
                            let dev = (segment_closest(a, z, p).2 - r).abs();
                            if dev > worst {
                                worst = dev;
                            }
                        }
                        cur = cu.p3;
                    }
                }
            }
            worst
        };
        let mut thin = brush(3.0);
        thin.spacing = 0.1;
        // 实验原案逐字：(356, 88) → (356, 272)、画布 1120×360 ✓。
        let original = line(&[(356.0, 88.0), (356.0, 272.0)]);
        let dev = capsule_dev(&thin, &original, (1120, 360), 1.5);
        assert!(dev <= 0.25, "原案轮廓偏差 {dev:.3}px 超线");
        // 相位扫：竖线 y0 = 40..=130、横线 x0 = 40..=120（步长 1/2）——
        // 缺陷是相位依赖的，只测一个位置锁不住 ✓。
        for k in 0..=90 {
            let y0 = 40.0 + f64::from(k);
            let g = line(&[(356.0, y0), (356.0, y0 + 184.0)]);
            let dev = capsule_dev(&thin, &g, (1120, 400), 1.5);
            assert!(dev <= 0.25, "竖线 y0={y0} 轮廓偏差 {dev:.3}px 超线");
        }
        for k in 0..=40 {
            let x0 = 40.0 + f64::from(k) * 2.0;
            let g = line(&[(x0, 200.0), (x0 + 184.0, 200.0)]);
            let dev = capsule_dev(&thin, &g, (1120, 400), 1.5);
            assert!(dev <= 0.25, "横线 x0={x0} 轮廓偏差 {dev:.3}px 超线");
        }
    }

    #[test]
    fn native_export_thick_line_matches_legacy_fit() {
        // 粗线不回归（恒等式 ✓）：r ≥ 8.6 时相对容差被钳到旧固定值 ⇒
        // 原生直出的轮廓必须与旧入口对同一包络线的拟合逐点一致。
        let appearance = StrokeAppearance::default();
        let b = brush(20.0);
        let g = line(&[(20.0, 50.0), (180.0, 50.0)]);
        let exported = export_stroke_native(&b, &g, &appearance, (200, 100));
        let StrokeExport::Vector { layers, .. } = exported else {
            panic!("粗线应可直出：{exported:?}");
        };
        let clusters = svg::stroke_dab_clusters(&b, &g, (200, 100));
        assert_eq!(clusters.len(), 1);
        let loop_pts = svg::envelope_contour(&clusters[0]);
        let legacy = svg::fit_closed_contour(&loop_pts, &[]).expect("包络线应可拟合");
        assert_eq!(layers[0].contours, vec![legacy]);
    }
}
