//! 路径几何：子路径（Subpath）模型与**规范折线**（曲线/路径一期，详细设计 §3）。
//!
//! 一条子路径 = 起点 + 段序列（`L` 直段 / `C` 三次 Bézier 段）+ closed 标志。
//! 渲染、命中、选区、沿路径描边四处共用同一份几何真源：[`flatten_subpath`]
//! 在生产容差 [`FLATTEN_TOL`] 下的输出。展平是纯函数：不读时钟 / 环境 / 全局状态，
//! 同输入同输出（逐位），且**展平点列只活在内存里，绝不写入 atom / 工程文件**
//! （双存必然失步，文档里只存控制点）。
//!
//! 数值纪律（D0）：全程 f64 四则运算，只许 IEEE 精确的 `.sqrt()`；
//! 弧长 / 最近点只在展平折线上做线性扫描，禁止对三次式做数值积分或方程求根。

/// 填充规则（SVG 同名语义）：决定多环嵌套时哪些区域算「内」。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FillRule {
    /// 奇偶规则：射线穿越次数为奇数才在内（嵌套环互相挖洞，与方向无关）。
    #[default]
    EvenOdd,
    /// 非零绕向规则：带符号绕向数 ≠ 0 在内（同向嵌套叠加，反向嵌套抵消）。
    NonZero,
}

impl FillRule {
    /// 载荷里的字符串名（`fill_rule` 字段 / SVG `fill-rule` 属性）。
    pub fn as_str(&self) -> &'static str {
        match self {
            FillRule::EvenOdd => "evenodd",
            FillRule::NonZero => "nonzero",
        }
    }

    /// 从载荷字符串解析；未知值返回 `None`（调用方报错，不静默降级）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "evenodd" => Some(FillRule::EvenOdd),
            "nonzero" => Some(FillRule::NonZero),
            _ => None,
        }
    }
}

/// 路径段：`L` 直段或 `C` 三次 Bézier 段。段的起点是上一段的终点（首段为子路径起点）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSeg {
    /// 直线段到 `to`。
    Line {
        /// 段终点（文档坐标）。
        to: (f64, f64),
    },
    /// 三次 Bézier 段：两个控制点 + 终点。
    Cubic {
        /// 第一控制点。
        c1: (f64, f64),
        /// 第二控制点。
        c2: (f64, f64),
        /// 段终点。
        to: (f64, f64),
    },
}

/// 一条子路径（运行期结构；atom 里存的是同构 JSON，解析在 `object::parse_path_subpaths`）。
#[derive(Clone, Debug, PartialEq)]
pub struct Subpath {
    /// 起点（文档坐标）。
    pub start: (f64, f64),
    /// 段序列（入库校验保证非空；展平对空段做防御处理）。
    pub segments: Vec<PathSeg>,
    /// 是否逻辑闭合（渲染 / 命中 / 导出时以直段连回起点，与 SVG `Z` 一致）。
    pub closed: bool,
}

/// 生产展平容差（画布 px）：规范折线与真曲线的弦差目标上限。
pub const FLATTEN_TOL: f64 = 0.10;

/// Wang 公式给出的单段采样数上限（防极端曲线把点数撑爆）。
const MAX_STEPS_PER_SEG: usize = 256;

/// 三次 Bézier 在 `t` 处的值（Horner 形式，系数逐坐标算）。
fn cubic_at(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), t: f64) -> (f64, f64) {
    let eval = |a: f64, b: f64, c: f64, d: f64| {
        let a3 = -a + 3.0 * b - 3.0 * c + d;
        let a2 = 3.0 * a - 6.0 * b + 3.0 * c;
        let a1 = -3.0 * a + 3.0 * b;
        ((a3 * t + a2) * t + a1) * t + a
    };
    (eval(p0.0, p1.0, p2.0, p3.0), eval(p0.1, p1.1, p2.1, p3.1))
}

/// Wang 的 L：两个二阶差分向量的 L1 范数取大者。
/// Bézier 曲线恒在控制多边形凸包内，L 度量曲线偏离弦的量级。
fn wang_l(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    let d1 = (p0.0 - 2.0 * p1.0 + p2.0).abs() + (p0.1 - 2.0 * p1.1 + p2.1).abs();
    let d2 = (p1.0 - 2.0 * p2.0 + p3.0).abs() + (p1.1 - 2.0 * p2.1 + p3.1).abs();
    d1.max(d2)
}

/// 由 L 与容差定采样数 N（未钳）：`ceil(sqrt(3L / (4·tol)))`。
fn wang_steps_raw(l: f64, tol: f64) -> f64 {
    if l <= 0.0 {
        return 1.0;
    }
    (3.0 * l / (4.0 * tol)).sqrt().ceil()
}

/// 一段三次 Bézier 的四个控制点（起点、c1、c2、终点）。
type CubicControls = ((f64, f64), (f64, f64), (f64, f64), (f64, f64));

/// de Casteljau 对半分治（t=0.5，除以 2 在二进制浮点下精确、确定性）。
fn split_cubic_half(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
) -> (CubicControls, CubicControls) {
    let mid = |a: (f64, f64), b: (f64, f64)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let m = mid(r0, r1);
    ((p0, q0, r0, m), (m, r1, q2, p3))
}

/// 把一段三次 Bézier 采样进 `out`（不含起点，含终点）。
///
/// Wang 定 N 超过 [`MAX_STEPS_PER_SEG`] 时先对半分治再各自定 N：
/// 钳 N 会让误差界失效（±4000 域随机实测 max 弦差 0.1102 > 0.11 即此因，
/// 分治后每半段 L 约缩到 1/4、N 约减半）。分治深度上限 8（L 每层约 ÷4，
/// 足够覆盖任何文档坐标量级），到顶后按钳后的 N 采样。
fn flatten_cubic_into(
    out: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    tol: f64,
    depth: u32,
) {
    let raw = wang_steps_raw(wang_l(p0, p1, p2, p3), tol);
    if raw > MAX_STEPS_PER_SEG as f64 && depth > 0 {
        let ((a0, a1, a2, a3), (b0, b1, b2, b3)) = split_cubic_half(p0, p1, p2, p3);
        flatten_cubic_into(out, a0, a1, a2, a3, tol, depth - 1);
        flatten_cubic_into(out, b0, b1, b2, b3, tol, depth - 1);
        return;
    }
    let n = if raw <= 1.0 {
        1
    } else if raw >= MAX_STEPS_PER_SEG as f64 {
        MAX_STEPS_PER_SEG
    } else {
        raw as usize
    };
    for i in 1..n {
        let t = i as f64 / n as f64;
        out.push(cubic_at(p0, p1, p2, p3, t));
    }
    // 末点直出 `p3`：B(1) 数学上恒等于 P3，Horner 求值会有 1-ulp 漂移，
    // 而闭合回首判定 / 段间衔接都按端点精确相等设计。
    out.push(p3);
}

/// 把一条子路径展平成折线点列（首项恒为 `start`）。
///
/// - `L` 段直接输出其 `to`；`C` 段按 Wang 定 N 均匀采样（跳过与上一点重合的首点）。
/// - `closed=true` 时末尾回到 `start`（与末点距离 > 1e-9 才追加），输出可直接当闭合环；
///   `closed=false` 不回首。
/// - 退化：`segments` 为空 ⇒ 返回 `[start]`（入库校验已拒绝空段，这里只作防御）；
///   控制点全重合的 C 段 L=0 ⇒ N=1，只输出 `to`，不除零。
/// - `tol <= 0` 或非有限 ⇒ 按 [`FLATTEN_TOL`] 处理（纯函数层不报错，校验在工具 / 解析层）。
pub fn flatten_subpath(sub: &Subpath, tol: f64) -> Vec<(f64, f64)> {
    let tol = if tol.is_finite() && tol > 0.0 {
        tol
    } else {
        FLATTEN_TOL
    };
    let mut out: Vec<(f64, f64)> = vec![sub.start];
    let mut cur = sub.start;
    for seg in &sub.segments {
        match *seg {
            PathSeg::Line { to } => {
                out.push(to);
                cur = to;
            }
            PathSeg::Cubic { c1, c2, to } => {
                flatten_cubic_into(&mut out, cur, c1, c2, to, tol, 8);
                cur = to;
            }
        }
    }
    if sub.closed {
        let (lx, ly) = *out.last().unwrap_or(&sub.start);
        let dx = lx - sub.start.0;
        let dy = ly - sub.start.1;
        if dx * dx + dy * dy > 1e-18 {
            out.push(sub.start);
        }
    }
    out
}

/// 折线总长（逐段欧氏距离累加；空 / 单点折线为 0）。
pub fn polyline_length(pts: &[(f64, f64)]) -> f64 {
    pts.windows(2)
        .map(|w| {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// 按弧长等距重采样：返回 `(x, y, 累计弧长)` 三元组。
///
/// 采样点位于 `s = 0, step, 2·step, …`，末点（`s = 总长`）在与最后一个
/// 等距采样点不重合（差 > 1e-9）时追加，保证剖面函数的尾端定义域完整。
/// `step <= 0` 或非有限 ⇒ 不重采样，原样返回点列并附累计弧长（防御性退化）。
pub fn resample_by_arclength(pts: &[(f64, f64)], step: f64) -> Vec<(f64, f64, f64)> {
    if pts.is_empty() {
        return Vec::new();
    }
    if pts.len() == 1 || !step.is_finite() || step <= 0.0 {
        let mut out = Vec::with_capacity(pts.len());
        let mut s = 0.0;
        out.push((pts[0].0, pts[0].1, 0.0));
        for w in pts.windows(2) {
            let dx = w[1].0 - w[0].0;
            let dy = w[1].1 - w[0].1;
            s += (dx * dx + dy * dy).sqrt();
            out.push((w[1].0, w[1].1, s));
        }
        return out;
    }
    let mut out: Vec<(f64, f64, f64)> = vec![(pts[0].0, pts[0].1, 0.0)];
    let mut next_s = step;
    let mut walked = 0.0;
    for w in pts.windows(2) {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let dx = bx - ax;
        let dy = by - ay;
        let seg_len = (dx * dx + dy * dy).sqrt();
        if seg_len <= 0.0 {
            continue;
        }
        while walked + seg_len >= next_s {
            let t = (next_s - walked) / seg_len;
            out.push((ax + dx * t, ay + dy * t, next_s));
            next_s += step;
        }
        walked += seg_len;
    }
    let (lx, ly) = pts[pts.len() - 1];
    if walked - out.last().map(|p| p.2).unwrap_or(0.0) > 1e-9 {
        out.push((lx, ly, walked));
    }
    out
}

/// 折线上距 `p` 最近的点：返回 `(段号, 段内参数 t, 投影点)`（线性扫描）。
///
/// 空折线返回 `(0, 0, p)`，单点折线返回 `(0, 0, 该点)`（防御性退化）。
pub fn nearest_on_polyline(pts: &[(f64, f64)], p: (f64, f64)) -> (usize, f64, (f64, f64)) {
    if pts.is_empty() {
        return (0, 0.0, p);
    }
    if pts.len() == 1 {
        return (0, 0.0, pts[0]);
    }
    let mut best_index = 0usize;
    let mut best_t = 0.0f64;
    let mut best_point = pts[0];
    let mut best_d2 = f64::INFINITY;
    for (i, w) in pts.windows(2).enumerate() {
        let (ax, ay) = w[0];
        let (bx, by) = w[1];
        let dx = bx - ax;
        let dy = by - ay;
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((p.0 - ax) * dx + (p.1 - ay) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let proj = (ax + dx * t, ay + dy * t);
        let ex = p.0 - proj.0;
        let ey = p.1 - proj.1;
        let d2 = ex * ex + ey * ey;
        if d2 < best_d2 {
            best_d2 = d2;
            best_index = i;
            best_t = t;
            best_point = proj;
        }
    }
    (best_index, best_t, best_point)
}

/// 多环填充命中：点 `p` 是否在 `rings` 按 `rule` 围成的填充区内。
///
/// 每个环按折线点列理解，末点以直段连回首点（环不必显式回首）。
/// 射线向 +x 方向数穿越：evenodd 看奇偶，nonzero 看带符号绕向和
/// （边向下穿过射线记 +1、向上记 −1，与 `geometry` 的覆盖率阈值同约定）。
pub fn rings_fill_contains(rings: &[&[(f64, f64)]], rule: FillRule, p: (f64, f64)) -> bool {
    let mut crossings: i64 = 0;
    let mut winding: i64 = 0;
    for ring in rings {
        let count = ring.len();
        if count < 2 {
            continue;
        }
        let mut j = count - 1;
        for i in 0..count {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > p.1) != (yj > p.1) {
                let t = (p.1 - yi) / (yj - yi);
                if p.0 < xi + t * (xj - xi) {
                    crossings += 1;
                    winding += if yj > yi { 1 } else { -1 };
                }
            }
            j = i;
        }
    }
    match rule {
        FillRule::EvenOdd => crossings % 2 == 1,
        FillRule::NonZero => winding != 0,
    }
}

/// 填充命中（§4.6）：点是否落在路径的填充区内（规范折线 + `fill_rule` 判定多环）。
///
/// 开放子路径按填充语义隐式闭合（折线末点连回起点），与 SVG 一致。
pub fn path_fill_contains(subpaths: &[Subpath], fill_rule: FillRule, p: (f64, f64)) -> bool {
    let flattened: Vec<Vec<(f64, f64)>> = subpaths
        .iter()
        .map(|sub| flatten_subpath(sub, FLATTEN_TOL))
        .collect();
    let rings: Vec<&[(f64, f64)]> = flattened.iter().map(Vec::as_slice).collect();
    rings_fill_contains(&rings, fill_rule, p)
}

/// 曲线尺吸附（二期 `path_guide`）：把点列往一组引导折线的最近点拉。
///
/// 每个点独立处理：在所有折线里找最近投影点 `Q`（[`nearest_on_polyline`]，
/// 全部在线性化折线上算，不解曲线方程），距离 `d = |P − Q|`；`d >= radius`
/// 时**逐位原样返回**（吸附半径外零影响，回归锁靠这一条）；`d < radius` 时
/// `P' = P + w·(Q − P)`，权重 `w = (1 − d/radius)²`（`d = 0` 时 `w = 1`，
/// 点恰落在引导线上）。纯函数、无状态；`radius <= 0` 或非有限 ⇒ 恒等返回
/// （防御性退化，工具层在调用前已把非法半径报成参数错误）。
pub fn snap_points_to_polylines(
    points: &[(f64, f64)],
    polylines: &[Vec<(f64, f64)>],
    radius: f64,
) -> Vec<(f64, f64)> {
    if !radius.is_finite() || radius <= 0.0 || polylines.is_empty() {
        return points.to_vec();
    }
    points
        .iter()
        .map(|&p| {
            let mut best: Option<((f64, f64), f64)> = None;
            for line in polylines {
                if line.is_empty() {
                    continue;
                }
                let (_, _, proj) = nearest_on_polyline(line, p);
                let dx = p.0 - proj.0;
                let dy = p.1 - proj.1;
                let d2 = dx * dx + dy * dy;
                if best.map(|(_, bd2)| d2 < bd2).unwrap_or(true) {
                    best = Some((proj, d2));
                }
            }
            let Some((proj, d2)) = best else {
                return p;
            };
            let d = d2.sqrt();
            if d >= radius || d == 0.0 {
                return p;
            }
            let w = (1.0 - d / radius) * (1.0 - d / radius);
            (p.0 + w * (proj.0 - p.0), p.1 + w * (proj.1 - p.1))
        })
        .collect()
}

/// 描边命中（§4.6）：点到任一子路径规范折线的最近距离 ≤ `stroke_width / 2 + 2px 容差`。
pub fn path_stroke_contains(subpaths: &[Subpath], stroke_width: f64, p: (f64, f64)) -> bool {
    let limit = stroke_width / 2.0 + 2.0;
    subpaths.iter().any(|sub| {
        let flat = flatten_subpath(sub, FLATTEN_TOL);
        let (_, _, proj) = nearest_on_polyline(&flat, p);
        let dx = p.0 - proj.0;
        let dy = p.1 - proj.1;
        (dx * dx + dy * dy).sqrt() <= limit
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定种子伪随机（xorshift64），只出现在测试里。
    struct Rng(u64);

    impl Rng {
        fn next_unit(&mut self) -> f64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            (x % 1_000_000) as f64 / 1_000_000.0
        }

        fn coord(&mut self) -> f64 {
            self.next_unit() * 8000.0 - 4000.0
        }
    }

    fn point_to_polyline_distance(pts: &[(f64, f64)], p: (f64, f64)) -> f64 {
        let (_, _, proj) = nearest_on_polyline(pts, p);
        let dx = p.0 - proj.0;
        let dy = p.1 - proj.1;
        (dx * dx + dy * dy).sqrt()
    }

    #[test]
    fn flatten_straight_and_degenerate() {
        // L 段：端点直出。
        let line = Subpath {
            start: (0.0, 0.0),
            segments: vec![
                PathSeg::Line { to: (10.0, 0.0) },
                PathSeg::Line { to: (10.0, 10.0) },
            ],
            closed: false,
        };
        assert_eq!(
            flatten_subpath(&line, FLATTEN_TOL),
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]
        );
        // 零长 C 段（控制点全重合）：L=0 ⇒ N=1，不 panic，只输出 to。
        let dot = Subpath {
            start: (5.0, 5.0),
            segments: vec![PathSeg::Cubic {
                c1: (5.0, 5.0),
                c2: (5.0, 5.0),
                to: (5.0, 5.0),
            }],
            closed: false,
        };
        let flat = flatten_subpath(&dot, FLATTEN_TOL);
        assert!(flat.iter().all(|&pt| pt == (5.0, 5.0)));
        // 空段防御：返回 [start]。
        let empty = Subpath {
            start: (1.0, 2.0),
            segments: vec![],
            closed: false,
        };
        assert_eq!(flatten_subpath(&empty, FLATTEN_TOL), vec![(1.0, 2.0)]);
        // tol ≤ 0 / NaN 按 FLATTEN_TOL 处理：与显式 0.10 逐点一致。
        let curve = Subpath {
            start: (0.0, 0.0),
            segments: vec![PathSeg::Cubic {
                c1: (0.0, 100.0),
                c2: (100.0, 100.0),
                to: (100.0, 0.0),
            }],
            closed: false,
        };
        assert_eq!(
            flatten_subpath(&curve, 0.0),
            flatten_subpath(&curve, FLATTEN_TOL)
        );
        assert_eq!(
            flatten_subpath(&curve, f64::NAN),
            flatten_subpath(&curve, FLATTEN_TOL)
        );
    }

    #[test]
    fn flatten_accuracy_property() {
        // 伪随机 1000 条三次段（坐标域 ±4000，固定种子）：
        // 真曲线每段采样 200 点，到展平折线的距离 max ≤ 0.11（容差 0.10 + 10% 裕量）。
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut max_deviation: f64 = 0.0;
        for case in 0..1000 {
            let p0 = (rng.coord(), rng.coord());
            let sub = Subpath {
                start: p0,
                segments: vec![PathSeg::Cubic {
                    c1: (rng.coord(), rng.coord()),
                    c2: (rng.coord(), rng.coord()),
                    to: (rng.coord(), rng.coord()),
                }],
                closed: false,
            };
            let PathSeg::Cubic { c1, c2, to } = sub.segments[0] else {
                unreachable!()
            };
            let flat = flatten_subpath(&sub, 0.10);
            assert!(flat.len() >= 2, "case {case}：展平点列过短");
            assert_eq!(flat[0], p0);
            assert_eq!(*flat.last().unwrap(), to);
            for s in 0..=200 {
                let t = s as f64 / 200.0;
                let point = cubic_at(p0, c1, c2, to, t);
                let d = point_to_polyline_distance(&flat, point);
                if d > max_deviation {
                    max_deviation = d;
                }
            }
        }
        println!("flatten_accuracy_property: max_deviation = {max_deviation}");
        assert!(
            max_deviation <= 0.11,
            "展平弦差 max = {max_deviation} 超过 0.11（先调 Wang 常数再谈其他）"
        );
    }

    #[test]
    fn flatten_deterministic_bitwise() {
        let sub = Subpath {
            start: (12.5, -40.25),
            segments: vec![
                PathSeg::Cubic {
                    c1: (300.0, 400.0),
                    c2: (-500.0, 100.0),
                    to: (700.0, -800.0),
                },
                PathSeg::Line { to: (0.0, 0.0) },
                PathSeg::Cubic {
                    c1: (1.0, 2.0),
                    c2: (3.0, 4.0),
                    to: (12.5, -40.25),
                },
            ],
            closed: true,
        };
        let a = flatten_subpath(&sub, FLATTEN_TOL);
        let b = flatten_subpath(&sub, FLATTEN_TOL);
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.0.to_bits(), pb.0.to_bits());
            assert_eq!(pa.1.to_bits(), pb.1.to_bits());
        }
    }

    #[test]
    fn flatten_closed_returns_to_start() {
        let mut sub = Subpath {
            start: (0.0, 0.0),
            segments: vec![
                PathSeg::Line { to: (50.0, 0.0) },
                PathSeg::Cubic {
                    c1: (50.0, 50.0),
                    c2: (0.0, 50.0),
                    to: (10.0, 10.0),
                },
            ],
            closed: true,
        };
        let flat = flatten_subpath(&sub, FLATTEN_TOL);
        assert_eq!(*flat.last().unwrap(), sub.start, "闭合路径必须回首点");
        sub.closed = false;
        let flat = flatten_subpath(&sub, FLATTEN_TOL);
        assert_eq!(*flat.last().unwrap(), (10.0, 10.0), "开放路径不得回首点");
    }

    #[test]
    fn resample_uniform_on_circle() {
        // 四段三次弧近似圆（κ = 0.552284749830951），半径 100、圆心 (100, 100)。
        let k = 0.552284749830951f64 * 100.0;
        let arc = |c1: (f64, f64), c2: (f64, f64), to: (f64, f64)| PathSeg::Cubic { c1, c2, to };
        let sub = Subpath {
            start: (200.0, 100.0),
            segments: vec![
                arc((200.0, 100.0 + k), (100.0 + k, 200.0), (100.0, 200.0)),
                arc((100.0 - k, 200.0), (0.0, 100.0 + k), (0.0, 100.0)),
                arc((0.0, 100.0 - k), (100.0 - k, 0.0), (100.0, 0.0)),
                arc((100.0 + k, 0.0), (200.0, 100.0 - k), (200.0, 100.0)),
            ],
            closed: true,
        };
        let flat = flatten_subpath(&sub, FLATTEN_TOL);
        let total = polyline_length(&flat);
        // 圆周长 2πr ≈ 628.3；三次近似 + 展平后误差应在 1% 内。
        assert!((total - 628.318).abs() < 6.3, "圆近似总长异常：{total}");
        let step = 25.0;
        let samples = resample_by_arclength(&flat, step);
        assert!(samples.len() > 20);
        for pair in samples.windows(2) {
            let ds = pair[1].2 - pair[0].2;
            if pair[1].2 < total - 1e-9 {
                assert!(
                    (ds - step).abs() <= step * 0.01,
                    "重采样弧长不均匀：ds = {ds}（step = {step}）"
                );
            }
        }
        // 采样点落在圆周附近（展平容差 + 弧近似误差内）。
        for (x, y, _) in &samples {
            let r = ((x - 100.0) * (x - 100.0) + (y - 100.0) * (y - 100.0)).sqrt();
            assert!((r - 100.0).abs() < 0.5, "采样点偏离圆周：r = {r}");
        }
    }

    #[test]
    fn fill_contains_respects_both_rules() {
        fn square(x: f64, y: f64, size: f64, reversed: bool) -> Subpath {
            let corners = [(x, y), (x + size, y), (x + size, y + size), (x, y + size)];
            let order: Vec<(f64, f64)> = if reversed {
                vec![corners[0], corners[3], corners[2], corners[1]]
            } else {
                corners.to_vec()
            };
            Subpath {
                start: order[0],
                segments: order[1..].iter().map(|&to| PathSeg::Line { to }).collect(),
                closed: true,
            }
        }
        let outer = square(0.0, 0.0, 100.0, false);
        let inner_same = square(25.0, 25.0, 50.0, false);
        let inner_reversed = square(25.0, 25.0, 50.0, true);
        let center = (50.0, 50.0);
        let ring_only = (10.0, 10.0);
        let outside = (200.0, 200.0);
        // 同向嵌套：evenodd 挖洞、nonzero 叠加。
        let subs = vec![outer.clone(), inner_same];
        assert!(!path_fill_contains(&subs, FillRule::EvenOdd, center));
        assert!(path_fill_contains(&subs, FillRule::NonZero, center));
        // 反向嵌套：两种规则都挖洞。
        let subs = vec![outer.clone(), inner_reversed];
        assert!(!path_fill_contains(&subs, FillRule::EvenOdd, center));
        assert!(!path_fill_contains(&subs, FillRule::NonZero, center));
        // 只在外环内的点：两种规则都在内；画布外的点都不在内。
        let subs = vec![outer];
        assert!(path_fill_contains(&subs, FillRule::EvenOdd, ring_only));
        assert!(path_fill_contains(&subs, FillRule::NonZero, ring_only));
        assert!(!path_fill_contains(&subs, FillRule::EvenOdd, outside));
        assert!(!path_fill_contains(&subs, FillRule::NonZero, outside));
    }

    #[test]
    fn stroke_contains_uses_distance_to_polyline() {
        let line = Subpath {
            start: (0.0, 0.0),
            segments: vec![PathSeg::Line { to: (100.0, 0.0) }],
            closed: false,
        };
        // 线宽 10 ⇒ 半宽 5 + 容差 2 = 7。
        assert!(path_stroke_contains(
            std::slice::from_ref(&line),
            10.0,
            (50.0, 6.0)
        ));
        assert!(!path_stroke_contains(
            std::slice::from_ref(&line),
            10.0,
            (50.0, 8.0)
        ));
        // 端点外侧按最近点（端帽）算距离。
        assert!(path_stroke_contains(&[line], 10.0, (106.0, 0.0)));
    }

    #[test]
    fn snap_pulls_points_by_quadratic_weight() {
        // 引导线：y=0 的直段，吸附半径 10。
        let guide = vec![vec![(0.0, 0.0), (100.0, 0.0)]];
        // d = 5 = R/2 ⇒ w = (1 − 0.5)² = 0.25 ⇒ y: 5 → 5 − 1.25 = 3.75。
        let out = snap_points_to_polylines(&[(50.0, 5.0)], &guide, 10.0);
        assert_eq!(out, vec![(50.0, 3.75)]);
        // d = 0 ⇒ 恰在引导线上，原样。
        let out = snap_points_to_polylines(&[(50.0, 0.0)], &guide, 10.0);
        assert_eq!(out, vec![(50.0, 0.0)]);
        // d >= R ⇒ 逐位不变（吸附关/远距离的回归锁靠这一条）。
        let out = snap_points_to_polylines(&[(50.0, 10.0), (50.0, 25.0)], &guide, 10.0);
        assert_eq!(out, vec![(50.0, 10.0), (50.0, 25.0)]);
        // 多条引导线取最近的那条：最近点在 y=20 线上，d=4
        // ⇒ w = (1 − 0.4)² = 0.36 ⇒ y = 16 + 0.36 × 4 = 17.44。
        let guides = vec![
            vec![(0.0, 0.0), (100.0, 0.0)],
            vec![(0.0, 20.0), (100.0, 20.0)],
        ];
        let out = snap_points_to_polylines(&[(50.0, 16.0)], &guides, 10.0);
        assert!((out[0].1 - 17.44).abs() < 1e-9, "实测 {}", out[0].1);
        // 半径非法 ⇒ 恒等（防御性退化；工具层先报错、到不了这里）。
        let out = snap_points_to_polylines(&[(50.0, 5.0)], &guide, 0.0);
        assert_eq!(out, vec![(50.0, 5.0)]);
    }
}
