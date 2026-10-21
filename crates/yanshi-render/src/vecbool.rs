//! **矢量布尔（多环 + 填充规则）** —— `combine_shapes` 的 vector 引擎后端。
//!
//! 与 [`crate::polygon`]（Greiner–Hormann、单环、退化即拒绝）的分工：
//! 本模块吃**多环**操作数（带孔形状就是两环）、各自带填充规则
//! （evenodd / nonzero），输出仍是多环（调用方以 evenodd 落成 path，
//! 孔洞天然保留）。G–H 在这里撑不住的输入——共线重叠边、顶点相切、
//! 一方包含另一方、操作数自交——本模块都要么给出正确结果、要么
//! 明确报错，**不静默出空、不给近似轮廓冒充精确结果**。
//!
//! ## 算法（平面排列 / arrangement + 面分类，结构同 Bentley–Ottmann
//! 之后的经典 arrangement 遍历，不是手搓扫描线）
//!
//! 1. **切边**：收集两操作数的全部边，两两求交（正交交叉、端点落边、
//!    共线重叠的端点），在交点处把边切成原子段；重合点按 1e-7 网格
//!    聚类成同一节点。共线重叠段按方向抵消计数（同操作数内反向共用
//!    的边是内边、净计数为零 ⇒ 不是边界）。
//! 2. **半边分类**：每条原子段生两条半边。对半边取「中点 + 左法线
//!    ε」采样点（ε 自适应取到最近无关几何距离的 1/4），用**原始环组**
//!    按各自填充规则判定采样点在 A / B 内的真伪，按 mode 合成
//!    「左面被选中」。
//! 3. **缝合边界**：左面选中、右面未选中的半边即结果边界；在节点处
//!    按极角「逆向的顺时针前驱」规则续接成环（面恒在左 ⇒ 外环与孔
//!    环方向自动相反），输出全部环。
//!
//! 采样分类的正确性依赖 ε 小于采样点到任何无关边的距离——切边后
//! 任何边都不会穿过原子段中点（穿过必在该处成节点、段已被切），
//! 所以 dmin > 0，ε = dmin/4 恒落在同一面内。这是可验证的结构
//! 性质，不是经验参数。
//!
//! ## 数值纪律
//! 全程 f64；角度比较只用象限 + 叉积（不用 atan2）；距离用 `.sqrt()`
//! （IEEE 精确）。同输入同输出（HashMap 只做累加计数，遍历顺序由
//! 节点/半边编号决定，结果环再按首点排序归一）。
//!
//! ## 复杂度与上限
//! 切边是 O(S²) 两两求交、分类是 O(H·(S+R))，S 为总边数。形状展平
//! 后边数通常在数百量级；硬上限 [`MAX_SEGMENTS`]，超限明确报错
//! （TooComplex），不许跑到天荒地老也不许截断了事。

use std::collections::HashMap;

use crate::path::{rings_fill_contains, FillRule};
use crate::polygon::BooleanMode;

/// 一个顶点（文档坐标）。
pub type Point = (f64, f64);

/// 总边数硬上限（两操作数合计，切边前）。
pub const MAX_SEGMENTS: usize = 20_000;

/// 重合点聚类半径（px）：展平容差默认 0.15，几何特征远大于此。
const SNAP: f64 = 1e-7;
/// 参数域 / 共线判定容差（无量纲，相对量另行缩放）。
const T_EPS: f64 = 1e-9;

/// 矢量布尔的失败：输入不合法或超出复杂度上限。一律显式报错。
#[derive(Debug, Clone, PartialEq)]
pub enum VecBoolError {
    /// 输入不合法（空环组、退化环、非有限坐标）。
    InvalidInput {
        /// 描述文字。
        detail: String,
    },
    /// 输入合法但超出本实现的复杂度上限。
    TooComplex {
        /// 描述文字。
        detail: String,
    },
    /// 内部结构不一致（缝合走不通）：宁可报错，不给错环。
    Internal {
        /// 描述文字。
        detail: String,
    },
}

impl VecBoolError {
    /// 人类可读的原因（工具层直接回给调用方）。
    pub fn detail(&self) -> &str {
        match self {
            Self::InvalidInput { detail }
            | Self::TooComplex { detail }
            | Self::Internal { detail } => detail,
        }
    }
}

fn invalid(detail: String) -> VecBoolError {
    VecBoolError::InvalidInput { detail }
}

/// mode 的真值合成。
fn combine(mode: BooleanMode, in_a: bool, in_b: bool) -> bool {
    match mode {
        BooleanMode::Union => in_a || in_b,
        BooleanMode::Intersect => in_a && in_b,
        BooleanMode::Subtract => in_a && !in_b,
        BooleanMode::Xor => in_a != in_b,
    }
}

/// 清洗一条环：去连续重复点、去结尾回首点。清洗后即调用方校验。
fn clean_ring(ring: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(ring.len());
    for &p in ring {
        if out
            .last()
            .is_some_and(|&q| (q.0 - p.0).abs() <= SNAP && (q.1 - p.1).abs() <= SNAP)
        {
            continue;
        }
        out.push(p);
    }
    if out.len() > 1 {
        let first = out[0];
        let last = *out.last().expect("len > 1");
        if (first.0 - last.0).abs() <= SNAP && (first.1 - last.1).abs() <= SNAP {
            out.pop();
        }
    }
    out
}

/// 校验一侧操作数并清洗。`which` 是报错文案里的操作数名。
fn validate_operand(rings: &[Vec<Point>], which: &str) -> Result<Vec<Vec<Point>>, VecBoolError> {
    if rings.is_empty() {
        return Err(invalid(format!("{which} 没有任何环（空路径没有布尔意义）")));
    }
    let mut out = Vec::with_capacity(rings.len());
    for (index, ring) in rings.iter().enumerate() {
        for &(x, y) in ring {
            if !x.is_finite() || !y.is_finite() {
                return Err(invalid(format!(
                    "{which} 第 {index} 环含非有限坐标 ({x}, {y})"
                )));
            }
        }
        let cleaned = clean_ring(ring);
        if cleaned.len() < 3 {
            return Err(invalid(format!(
                "{which} 第 {index} 环清洗后只有 {} 个互异点（至少 3 个）",
                cleaned.len()
            )));
        }
        // 全共线 ⇒ 面积恒零、没有内部：退化环明确拒绝（自交环的点
        // 不全共线，不受此限）。
        let p0 = cleaned[0];
        let p1 = *cleaned
            .iter()
            .max_by(|a, b| {
                let da = (a.0 - p0.0) * (a.0 - p0.0) + (a.1 - p0.1) * (a.1 - p0.1);
                let db = (b.0 - p0.0) * (b.0 - p0.0) + (b.1 - p0.1) * (b.1 - p0.1);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("len >= 3");
        let collinear = cleaned.iter().all(|&p| {
            let cross = (p1.0 - p0.0) * (p.1 - p0.1) - (p1.1 - p0.1) * (p.0 - p0.0);
            cross.abs() <= 1e-9
        });
        if collinear {
            return Err(invalid(format!(
                "{which} 第 {index} 环的点全在一条直线上（退化环，没有内部）"
            )));
        }
        out.push(cleaned);
    }
    Ok(out)
}

/// 节点池：按 SNAP 网格聚类去重。
struct NodePool {
    points: Vec<Point>,
    grid: HashMap<(i64, i64), Vec<usize>>,
}

impl NodePool {
    fn new() -> Self {
        Self {
            points: Vec::new(),
            grid: HashMap::new(),
        }
    }

    fn cell(value: f64) -> i64 {
        (value / SNAP).floor() as i64
    }

    fn intern(&mut self, p: Point) -> usize {
        let (cx, cy) = (Self::cell(p.0), Self::cell(p.1));
        for gx in cx - 1..=cx + 1 {
            for gy in cy - 1..=cy + 1 {
                if let Some(ids) = self.grid.get(&(gx, gy)) {
                    for &id in ids {
                        let q = self.points[id];
                        let dx = q.0 - p.0;
                        let dy = q.1 - p.1;
                        if dx * dx + dy * dy <= SNAP * SNAP {
                            return id;
                        }
                    }
                }
            }
        }
        let id = self.points.len();
        self.points.push(p);
        self.grid.entry((cx, cy)).or_default().push(id);
        id
    }
}

/// 一条原始边（操作数 0 = A、1 = B）。
#[derive(Clone, Copy)]
struct Seg {
    a: Point,
    b: Point,
    operand: u8,
}

/// 点是否落在线段上（含端点，容差 T_EPS 的带）。
fn point_on_segment(p: Point, a: Point, b: Point) -> bool {
    let cross = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
    let len = ((b.0 - a.0) * (b.0 - a.0) + (b.1 - a.1) * (b.1 - a.1)).sqrt();
    if len <= 0.0 || cross.abs() > T_EPS * len.max(1.0) {
        return false;
    }
    p.0 >= a.0.min(b.0) - T_EPS
        && p.0 <= a.0.max(b.0) + T_EPS
        && p.1 >= a.1.min(b.1) - T_EPS
        && p.1 <= a.1.max(b.1) + T_EPS
}

/// 点到线段的距离。
fn point_segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 <= 0.0 {
        return ((p.0 - a.0) * (p.0 - a.0) + (p.1 - a.1) * (p.1 - a.1)).sqrt();
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - qx) * (p.0 - qx) + (p.1 - qy) * (p.1 - qy)).sqrt()
}

/// 极角比较（不用 atan2）：`dir` 相对参考向量 `reference` 的
/// 「顺时针转角」排序键比较——先按半平面、再按叉积。
/// 返回 true 表示 `x` 的方向在从 `reference` 顺时针旋转时先于 `y` 到达。
fn clockwise_before(reference: Point, x: Point, y: Point) -> bool {
    // 把向量转到以 reference 为 0 角、顺时针为正的坐标系：
    // 分量 (沿 reference, 顺时针垂直分量)。
    let rl = (reference.0 * reference.0 + reference.1 * reference.1).sqrt();
    let (ux, uy) = (reference.0 / rl, reference.1 / rl);
    let project = |v: Point| (v.0 * ux + v.1 * uy, v.0 * uy - v.1 * ux);
    let (ax, ay) = project(x);
    let (bx, by) = project(y);
    // 顺时针角在 [0, 2π)：上半（ay<0，即逆时针侧）排在下半之后。
    let half = |vy: f64| {
        if vy > 0.0 {
            0 // 顺时针半圈 (0, π)
        } else if vy < 0.0 {
            1 // 逆时针半圈 (π, 2π)
        } else {
            0
        }
    };
    let (ha, hb) = (half(ay), half(by));
    if ha != hb {
        return ha < hb;
    }
    // 同半圈内定序：投影坐标里顺时针角 θ 的坐标是 (cosθ, sinθ)，
    // 所以 x 先于 y（顺时针）⇔ cross(x, y) > 0。
    let cross = ax * by - ay * bx;
    if cross != 0.0 {
        return cross > 0.0;
    }
    false
}

/// 多环矢量布尔：`a` / `b` 是环组（展平后的折线环，环不必回首），
/// 各自按 `rule_a` / `rule_b` 解释填充；返回结果环组（evenodd 语义，
/// 环方向混合：外环与孔环方向相反），结果为空集时返回空 Vec
/// （那是合法结果，不是错误——调用方决定空集怎么报）。
pub fn boolean_rings(
    a: &[Vec<Point>],
    rule_a: FillRule,
    b: &[Vec<Point>],
    rule_b: FillRule,
    mode: BooleanMode,
) -> Result<Vec<Vec<Point>>, VecBoolError> {
    let a_rings = validate_operand(a, "操作数 A")?;
    let b_rings = validate_operand(b, "操作数 B")?;

    // ---- 1. 收集边 ----
    let mut segs: Vec<Seg> = Vec::new();
    for (operand, rings) in [(0u8, &a_rings), (1u8, &b_rings)] {
        for ring in rings {
            for i in 0..ring.len() {
                let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
                if (p.0 - q.0).abs() <= SNAP && (p.1 - q.1).abs() <= SNAP {
                    continue;
                }
                segs.push(Seg {
                    a: p,
                    b: q,
                    operand,
                });
            }
        }
    }
    if segs.len() > MAX_SEGMENTS {
        return Err(VecBoolError::TooComplex {
            detail: format!(
                "展平后总边数 {} 超过矢量布尔上限 {MAX_SEGMENTS}——请增大展平容差或改用位图引擎",
                segs.len()
            ),
        });
    }

    // ---- 2. 两两求交、切边 ----
    let mut pool = NodePool::new();
    // 每条边的切分点：(t, 节点 id)，含 t=0/1。
    let mut atomic: HashMap<(usize, usize), (i32, i32)> = HashMap::new();
    for i in 0..segs.len() {
        let s = segs[i];
        let (dx, dy) = (s.b.0 - s.a.0, s.b.1 - s.a.1);
        let mut cuts: Vec<(f64, usize)> = vec![(0.0, pool.intern(s.a)), (1.0, pool.intern(s.b))];
        for (j, &o) in segs.iter().enumerate() {
            if i == j {
                continue;
            }
            let (ex, ey) = (o.b.0 - o.a.0, o.b.1 - o.a.1);
            let denom = dx * ey - dy * ex;
            if denom.abs() > 1e-12 {
                // 非平行：唯一交点（含端点接触的 T 形 / 顶点相触）。
                let (fx, fy) = (o.a.0 - s.a.0, o.a.1 - s.a.1);
                let t = (fx * ey - fy * ex) / denom;
                let u = (fx * dy - fy * dx) / denom;
                if (-T_EPS..=1.0 + T_EPS).contains(&t) && (-T_EPS..=1.0 + T_EPS).contains(&u) {
                    let tc = t.clamp(0.0, 1.0);
                    let p = (s.a.0 + tc * dx, s.a.1 + tc * dy);
                    let id = pool.intern(p);
                    cuts.push((tc, id));
                }
            } else {
                // 平行：只在共线时处理——把对方端点投到本边上切分。
                let cross = dx * (o.a.1 - s.a.1) - dy * (o.a.0 - s.a.0);
                let scale = (dx * dx + dy * dy).sqrt().max(1.0);
                if cross.abs() <= T_EPS * scale {
                    for endpoint in [o.a, o.b] {
                        if point_on_segment(endpoint, s.a, s.b) {
                            let len2 = dx * dx + dy * dy;
                            let t = ((endpoint.0 - s.a.0) * dx + (endpoint.1 - s.a.1) * dy) / len2;
                            let id = pool.intern(endpoint);
                            cuts.push((t.clamp(0.0, 1.0), id));
                        }
                    }
                }
            }
        }
        cuts.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        // 合并同 t（同节点）的重复切分。
        let mut merged: Vec<(f64, usize)> = Vec::with_capacity(cuts.len());
        for cut in cuts {
            if let Some(last) = merged.last() {
                if (last.0 - cut.0).abs() <= T_EPS {
                    continue;
                }
            }
            merged.push(cut);
        }
        for pair in merged.windows(2) {
            let (u, v) = (pair[0].1, pair[1].1);
            if u == v {
                continue;
            }
            let (key, sign) = if u < v {
                ((u, v), 1i32)
            } else {
                ((v, u), -1i32)
            };
            let entry = atomic.entry(key).or_insert((0, 0));
            if s.operand == 0 {
                entry.0 += sign;
            } else {
                entry.1 += sign;
            }
        }
    }
    // 净计数双零的段不是任何区域的边界（内边 / 尖刺），剔除。
    let mut edges: Vec<(usize, usize)> = atomic
        .into_iter()
        .filter(|(_, nets)| *nets != (0, 0))
        .map(|(key, _)| key)
        .collect();
    edges.sort_unstable();
    if edges.is_empty() {
        // 两操作数的边全部互相抵消（同环组反向叠合一类）：区域关系
        // 由分类决定——没有任何边界段意味着结果要么空、要么是某
        // 操作数全域。用采样点直接判：取 A 第一环的内部点试。
        // 走到这里的情形极罕见（完全重合的环组），用包含关系兜底。
        return fallback_coincident(&a_rings, rule_a, &b_rings, rule_b, mode);
    }

    // ---- 3. 半边与分类 ----
    // 半边 2k: edges[k].0 -> edges[k].1；2k+1 反向。
    let half_count = edges.len() * 2;
    let he_from = |he: usize| -> usize {
        if he % 2 == 0 {
            edges[he / 2].0
        } else {
            edges[he / 2].1
        }
    };
    let he_to = |he: usize| -> usize {
        if he % 2 == 0 {
            edges[he / 2].1
        } else {
            edges[he / 2].0
        }
    };
    let a_refs: Vec<&[Point]> = a_rings.iter().map(Vec::as_slice).collect();
    let b_refs: Vec<&[Point]> = b_rings.iter().map(Vec::as_slice).collect();
    let mut selected_left: Vec<bool> = vec![false; half_count];
    for (he, slot) in selected_left.iter_mut().enumerate() {
        let (from, to) = (pool.points[he_from(he)], pool.points[he_to(he)]);
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let len = (dx * dx + dy * dy).sqrt();
        let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
        // 自适应 ε：到最近「无关几何」的距离的 1/4。
        let mut dmin = len / 2.0;
        for (id, &node) in pool.points.iter().enumerate() {
            if id == he_from(he) || id == he_to(he) {
                continue;
            }
            let d =
                ((node.0 - mid.0) * (node.0 - mid.0) + (node.1 - mid.1) * (node.1 - mid.1)).sqrt();
            if d < dmin {
                dmin = d;
            }
        }
        for &(u, v) in &edges {
            if u == he_from(he) && v == he_to(he) || u == he_to(he) && v == he_from(he) {
                continue;
            }
            let d = point_segment_distance(mid, pool.points[u], pool.points[v]);
            if d < dmin {
                dmin = d;
            }
        }
        let eps = if dmin > 1e-9 { dmin / 4.0 } else { 1e-9 };
        let sample = (mid.0 - dy / len * eps, mid.1 + dx / len * eps);
        let in_a = rings_fill_contains(&a_refs, rule_a, sample);
        let in_b = rings_fill_contains(&b_refs, rule_b, sample);
        *slot = combine(mode, in_a, in_b);
    }

    // ---- 4. 缝合边界 ----
    let is_boundary = |he: usize| -> bool { selected_left[he] && !selected_left[he ^ 1] };
    // 每节点的出向边界半边。
    let mut outgoing: HashMap<usize, Vec<usize>> = HashMap::new();
    for he in 0..half_count {
        if is_boundary(he) {
            outgoing.entry(he_from(he)).or_default().push(he);
        }
    }
    let mut visited: Vec<bool> = vec![false; half_count];
    let mut rings: Vec<Vec<Point>> = Vec::new();
    for start in 0..half_count {
        if visited[start] || !is_boundary(start) {
            continue;
        }
        let mut ring: Vec<Point> = Vec::new();
        let mut cur = start;
        let mut guard = 0usize;
        loop {
            visited[cur] = true;
            ring.push(pool.points[he_from(cur)]);
            let node = he_to(cur);
            if node == he_from(start) && ring.len() >= 2 {
                break;
            }
            // 在 node 处选下一条：方向从「cur 的逆向」顺时针数第一条
            // 出向边界半边（面恒在左的续接规则）。
            let rev = (
                pool.points[he_from(cur)].0 - pool.points[node].0,
                pool.points[he_from(cur)].1 - pool.points[node].1,
            );
            let candidates = outgoing.get(&node).cloned().unwrap_or_default();
            let mut next: Option<usize> = None;
            for &cand in &candidates {
                if cand == start && ring.len() >= 2 {
                    next = Some(cand);
                    break;
                }
                if visited[cand] {
                    continue;
                }
                let dir = (
                    pool.points[he_to(cand)].0 - pool.points[node].0,
                    pool.points[he_to(cand)].1 - pool.points[node].1,
                );
                match next {
                    None => next = Some(cand),
                    Some(prev) => {
                        let prev_dir = (
                            pool.points[he_to(prev)].0 - pool.points[node].0,
                            pool.points[he_to(prev)].1 - pool.points[node].1,
                        );
                        if clockwise_before(rev, dir, prev_dir) {
                            next = Some(cand);
                        }
                    }
                }
            }
            let Some(next) = next else {
                return Err(VecBoolError::Internal {
                    detail: format!(
                        "边界缝合在节点 ({:.3}, {:.3}) 断链——输入几何超出本实现可处理的退化程度",
                        pool.points[node].0, pool.points[node].1
                    ),
                });
            };
            if next == start {
                break;
            }
            cur = next;
            guard += 1;
            if guard > half_count + 1 {
                return Err(VecBoolError::Internal {
                    detail: "边界缝合步数超过半边总数——结构不一致".to_owned(),
                });
            }
        }
        let cleaned = clean_ring(&ring);
        if cleaned.len() >= 3 && crate::polygon::area(&cleaned) > 1e-9 {
            rings.push(cleaned);
        }
    }
    // 确定性输出：按首点坐标排序。
    rings.sort_by(|x, y| {
        x[0].0
            .partial_cmp(&y[0].0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                x[0].1
                    .partial_cmp(&y[0].1)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(x.len().cmp(&y.len()))
    });
    Ok(rings)
}

/// 边全部抵消时的兜底（两操作数边界完全重合，如同环或同环反向）：
/// 区域只可能「相等 / 互补 / 一方为空」，用采样分类直接组装。
fn fallback_coincident(
    a: &[Vec<Point>],
    rule_a: FillRule,
    b: &[Vec<Point>],
    rule_b: FillRule,
    mode: BooleanMode,
) -> Result<Vec<Vec<Point>>, VecBoolError> {
    let a_refs: Vec<&[Point]> = a.iter().map(Vec::as_slice).collect();
    let b_refs: Vec<&[Point]> = b.iter().map(Vec::as_slice).collect();
    // 用 A 第一环某条边的左采样点探 A 的内部是否非空、与 B 的关系。
    let probe = a[0][0];
    let next = a[0][1];
    let (dx, dy) = (next.0 - probe.0, next.1 - probe.1);
    let len = (dx * dx + dy * dy).sqrt();
    let mid = ((probe.0 + next.0) / 2.0, (probe.1 + next.1) / 2.0);
    let mut any_selected = false;
    for side in [1.0f64, -1.0] {
        let sample = (
            mid.0 - dy / len * side * 0.01,
            mid.1 + dx / len * side * 0.01,
        );
        let in_a = rings_fill_contains(&a_refs, rule_a, sample);
        let in_b = rings_fill_contains(&b_refs, rule_b, sample);
        if combine(mode, in_a, in_b) {
            any_selected = true;
        }
        let _ = (in_a, in_b);
    }
    if !any_selected {
        return Ok(Vec::new());
    }
    // 选中区域与 A 或 B 之一同界：直接回那一侧的环组（evenodd 归一
    // 由调用方按 evenodd 落成；两侧填充规则不同而边界相同时，区
    // 域集合相同，环组任选其一都对）。
    match mode {
        BooleanMode::Union | BooleanMode::Intersect => {
            // 边界重合 ⇒ 两区域要么同集要么一空；非空一侧即结果。
            let a_area = rings_evenodd_area(a);
            if a_area > 0.0 {
                Ok(a.to_vec())
            } else {
                Ok(b.to_vec())
            }
        }
        BooleanMode::Subtract | BooleanMode::Xor => Ok(a.to_vec()),
    }
}

/// 环组在 evenodd 语义下的净面积（嵌套深度定符号）。
/// 供恒等式测试与兜底路径用；环组的环应互不相交（可嵌套）。
pub fn rings_evenodd_area(rings: &[Vec<Point>]) -> f64 {
    let mut total = 0.0;
    for (i, ring) in rings.iter().enumerate() {
        let mut depth = 0usize;
        'outer: for (j, other) in rings.iter().enumerate() {
            if i == j || other.len() < 3 {
                continue;
            }
            // 用 ring 的顶点探包含：只认「严格在内」的顶点（在对方
            // 边上的顶点不算，相切环靠其余顶点判定）。
            for &p in ring {
                let on_edge = other
                    .iter()
                    .enumerate()
                    .any(|(k, &q)| point_on_segment(p, q, other[(k + 1) % other.len()]));
                if on_edge {
                    continue;
                }
                let refs: Vec<&[Point]> = vec![other.as_slice()];
                if rings_fill_contains(&refs, FillRule::EvenOdd, p) {
                    depth += 1;
                    continue 'outer;
                }
            }
        }
        let area = crate::polygon::area(ring);
        if depth % 2 == 0 {
            total += area;
        } else {
            total -= area;
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_ring(x: f64, y: f64, w: f64, h: f64) -> Vec<Point> {
        vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
    }

    fn circle_ring(cx: f64, cy: f64, r: f64, tol: f64) -> Vec<Point> {
        // 用 path 的 κ 圆弧展平（与 server 层椭圆转环同一条路）。
        let kappa = 0.552_284_749_830_79;
        let d = r * kappa;
        let sub = crate::path::Subpath {
            start: (cx + r, cy),
            closed: true,
            segments: vec![
                crate::path::PathSeg::Cubic {
                    c1: (cx + r, cy + d),
                    c2: (cx + d, cy + r),
                    to: (cx, cy + r),
                },
                crate::path::PathSeg::Cubic {
                    c1: (cx - d, cy + r),
                    c2: (cx - r, cy + d),
                    to: (cx - r, cy),
                },
                crate::path::PathSeg::Cubic {
                    c1: (cx - r, cy - d),
                    c2: (cx - d, cy - r),
                    to: (cx, cy - r),
                },
                crate::path::PathSeg::Cubic {
                    c1: (cx + d, cy - r),
                    c2: (cx + r, cy - d),
                    to: (cx + r, cy),
                },
            ],
        };
        crate::path::flatten_subpath(&sub, tol)
    }

    fn run(
        a: Vec<Vec<Point>>,
        b: Vec<Vec<Point>>,
        mode: BooleanMode,
    ) -> Result<Vec<Vec<Point>>, VecBoolError> {
        boolean_rings(&a, FillRule::EvenOdd, &b, FillRule::EvenOdd, mode)
    }

    #[test]
    fn rect_pairs_match_analytic_areas_exactly() {
        let a = vec![rect_ring(0.0, 0.0, 10.0, 10.0)];
        let b = vec![rect_ring(4.0, 4.0, 10.0, 10.0)];
        // 交叠 6×6 = 36。
        let cases = [
            (BooleanMode::Union, 164.0),
            (BooleanMode::Intersect, 36.0),
            (BooleanMode::Subtract, 64.0),
            (BooleanMode::Xor, 128.0),
        ];
        for (mode, expect) in cases {
            let out = run(a.clone(), b.clone(), mode).expect("矩形布尔应成功");
            let got = rings_evenodd_area(&out);
            assert!(
                (got - expect).abs() < 1e-6,
                "{mode:?} 面积应 {expect}，实得 {got}（环：{out:?}）"
            );
        }
    }

    #[test]
    fn circle_circle_intersection_within_one_percent() {
        // 两圆 r=50，圆心距 60：交集解析值
        // 2r²·acos(d/2r) − (d/2)·√(4r²−d²)。
        let (r, d) = (50.0f64, 60.0f64);
        let a = vec![circle_ring(0.0, 0.0, r, 0.15)];
        let b = vec![circle_ring(d, 0.0, r, 0.15)];
        let analytic =
            2.0 * r * r * libm::acos(d / (2.0 * r)) - (d / 2.0) * (4.0 * r * r - d * d).sqrt();
        let out = run(a.clone(), b.clone(), BooleanMode::Intersect).expect("圆∩圆应成功");
        let got = rings_evenodd_area(&out);
        let rel = (got - analytic).abs() / analytic;
        println!("MEASURE circle_intersect got={got:.3} analytic={analytic:.3} rel={rel:.5}");
        assert!(
            rel <= 0.01,
            "圆∩圆面积 {got} vs 解析 {analytic}，相对偏差 {rel:.4} 超 1%"
        );
        // 并集恒等式：|A∪B| = |A| + |B| − |A∩B|（同一输出口径内自洽）。
        let union = run(a.clone(), b.clone(), BooleanMode::Union).expect("并集应成功");
        let union_area = rings_evenodd_area(&union);
        let single = rings_evenodd_area(&a);
        assert!(
            (union_area - (2.0 * single - got)).abs() / union_area < 1e-6,
            "并集恒等式不成立：{union_area} vs {}",
            2.0 * single - got
        );
        // 圆面积本身对 πr² 的偏差（展平口径）也应在 1% 内。
        let circle_rel =
            (single - std::f64::consts::PI * r * r).abs() / (std::f64::consts::PI * r * r);
        assert!(circle_rel <= 0.01, "展平圆面积偏差 {circle_rel:.4}");
    }

    #[test]
    fn circle_rect_quarter_intersection_within_one_percent() {
        // 圆心在矩形角上、r=50 ⇒ 交集 = 四分之一圆。
        let a = vec![circle_ring(0.0, 0.0, 50.0, 0.15)];
        let b = vec![rect_ring(0.0, 0.0, 100.0, 100.0)];
        let analytic = std::f64::consts::PI * 2500.0 / 4.0;
        let out = run(a, b, BooleanMode::Intersect).expect("圆∩矩形应成功");
        let got = rings_evenodd_area(&out);
        let rel = (got - analytic).abs() / analytic;
        println!("MEASURE circle_rect got={got:.3} analytic={analytic:.3} rel={rel:.5}");
        assert!(
            rel <= 0.01,
            "圆∩矩形面积 {got} vs 解析 {analytic}，偏差 {rel:.4}"
        );
    }

    #[test]
    fn holed_shape_boolean_keeps_the_hole() {
        // A = 100×100 方块挖 40×40 洞（evenodd 两环）；B = 右半大矩形。
        let a = vec![
            rect_ring(0.0, 0.0, 100.0, 100.0),
            rect_ring(30.0, 30.0, 40.0, 40.0),
        ];
        let b = vec![rect_ring(50.0, -10.0, 100.0, 120.0)];
        // 交集 = 右半 (50×100=5000) 减洞的右半 (20×40=800) = 4200。
        let out = run(a.clone(), b.clone(), BooleanMode::Intersect).expect("交集应成功");
        let got = rings_evenodd_area(&out);
        assert!(
            (got - 4200.0).abs() < 1e-6,
            "带孔交集应 4200，实得 {got}（{out:?}）"
        );
        // A − B = 左半 (50×100=5000) 减洞左半 (20×40=800) = 4200。
        // 洞被 B 切开、在 x=50 处敞成缺口 ⇒ 结果是 1 条环（不是保孔）。
        let out = run(a.clone(), b, BooleanMode::Subtract).expect("差集应成功");
        let got = rings_evenodd_area(&out);
        assert!((got - 4200.0).abs() < 1e-6, "带孔差集应 4200，实得 {got}");
        assert_eq!(out.len(), 1, "洞被切开后应为 1 环，实得 {out:?}");
        // 真保孔差集：B 只切左上角、不碰洞 ⇒ 面积 10000−1600−400=8000、仍 2 环。
        let corner = vec![rect_ring(0.0, 0.0, 20.0, 20.0)];
        let out = run(a, corner, BooleanMode::Subtract).expect("保孔差集应成功");
        let got = rings_evenodd_area(&out);
        assert!((got - 8000.0).abs() < 1e-6, "保孔差集应 8000，实得 {got}");
        assert_eq!(out.len(), 2, "孔应保留为第二环，实得 {out:?}");
        // 包含式挖洞：大方 − 内含小方 = 84（钥匙孔情形，输出必须是外环+孔环）。
        let outer = vec![rect_ring(0.0, 0.0, 10.0, 10.0)];
        let inner = vec![rect_ring(3.0, 3.0, 4.0, 4.0)];
        let out = run(outer, inner, BooleanMode::Subtract).expect("包含差集应成功");
        assert_eq!(out.len(), 2, "挖洞应输出 2 环，实得 {out:?}");
        assert!((rings_evenodd_area(&out) - 84.0).abs() < 1e-6);
    }

    #[test]
    fn degenerate_inputs_error_or_answer_never_panic_never_silent() {
        let sq = vec![rect_ring(0.0, 0.0, 10.0, 10.0)];
        // 相切（共享一条边、共线重叠）：并集 = 200，且必须是两个方
        // 拼成的 20×10 区域——G–H 在这里直接拒绝，本实现要给对。
        let touching = vec![rect_ring(10.0, 0.0, 10.0, 10.0)];
        let out = run(sq.clone(), touching, BooleanMode::Union).expect("共边并集应成功");
        assert!(
            (rings_evenodd_area(&out) - 200.0).abs() < 1e-6,
            "共边并集 {out:?}"
        );
        // 顶点相切（只共享一个角点）：并集 = 200、交集为空集（合法空，不是错误）。
        let corner = vec![rect_ring(10.0, 10.0, 10.0, 10.0)];
        let out = run(sq.clone(), corner.clone(), BooleanMode::Union).expect("点切并集应成功");
        assert!((rings_evenodd_area(&out) - 200.0).abs() < 1e-6);
        let out = run(sq.clone(), corner, BooleanMode::Intersect).expect("点切交集应成功（空集）");
        assert!(out.is_empty(), "点切交集应为空集，实得 {out:?}");
        // 完全重合：并集 = 交集 = 100、差集 = 空。
        let out = run(sq.clone(), sq.clone(), BooleanMode::Union).expect("重合并集应成功");
        assert!(
            (rings_evenodd_area(&out) - 100.0).abs() < 1e-6,
            "重合并集 {out:?}"
        );
        let out =
            run(sq.clone(), sq.clone(), BooleanMode::Subtract).expect("重合差集应成功（空集）");
        assert!(out.is_empty(), "重合差集应为空集，实得 {out:?}");
        // 空环组 ⇒ 明确报错。
        assert!(run(vec![], sq.clone(), BooleanMode::Union).is_err());
        // 全共线退化环 ⇒ 明确报错。
        let flat = vec![vec![(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)]];
        assert!(run(flat, sq.clone(), BooleanMode::Union).is_err());
        // 自交蝴蝶结（evenodd = 两三角形，各 25）：与自己并集 = 50。
        let bowtie = vec![vec![(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)]];
        let out = run(bowtie.clone(), bowtie, BooleanMode::Union).expect("自交并集应成功");
        let got = rings_evenodd_area(&out);
        assert!(
            (got - 50.0).abs() < 1e-6,
            "蝴蝶结并集应 50，实得 {got}（{out:?}）"
        );
    }

    #[test]
    fn disjoint_and_containment_identities() {
        let a = vec![rect_ring(0.0, 0.0, 10.0, 10.0)];
        let far = vec![rect_ring(50.0, 50.0, 10.0, 10.0)];
        let out = run(a.clone(), far, BooleanMode::Union).expect("不相交并集应成功");
        assert!((rings_evenodd_area(&out) - 200.0).abs() < 1e-6);
        assert_eq!(out.len(), 2, "不相交并集应 2 环");
        let inner = vec![rect_ring(2.0, 2.0, 4.0, 4.0)];
        let out = run(a.clone(), inner.clone(), BooleanMode::Intersect).expect("包含交集应成功");
        assert!((rings_evenodd_area(&out) - 16.0).abs() < 1e-6);
        let out = run(a, inner, BooleanMode::Union).expect("包含并集应成功");
        assert!((rings_evenodd_area(&out) - 100.0).abs() < 1e-6);
        assert_eq!(out.len(), 1, "包含并集应只剩外环");
    }
}
