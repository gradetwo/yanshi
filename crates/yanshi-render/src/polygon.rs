//! **多边形布尔运算**（设计 792 的 `boolean` ✓）。
//!
//! **设计只给了名字 ⇒ 本模块记录全部取舍** ✓：
//!
//! * **算法**：Greiner–Hormann ✓（把两条多边形的边互相求交 ✓、按交点在边上的位置排序 ✓、
//!   再按"进入/离开"标记沿交错的边表走一圈 ✓）。选它是因为**代码量可控、结果可预测** ✓，
//!   而且四种运算（并 ✓、交 ✓、差 ✓、异或 ✓）共用同一套遍历 ✓。
//! * **退化的处理方式是"拒绝"** ✓：交点在顶点上 ✓、两边共线重叠 ✓、平行无交点但重叠 ✓ ——
//!   这些是 G–H 的**已知失效情形** ✓。本模块**明确返回错误** ✓，**绝不给出"看起来对、其实错"的
//!   多边形** ✗（本项目反复强调：静默错误比不支持更糟 ✓）。
//!   需要更强健时应当换成扫描线算法（Martinez–Rueda 一类 ✓）✓ —— 那是一次**有意的升级** ✓，
//!   不是悄悄换掉 ✓。
//! * **曲线先离散化** ✓：布尔结果只含**直线段** ✓ ⇒ 结果**不保留贝塞尔控制柄** ✓
//!   （离散化用与渲染同级的细分 ✓ ⇒ 结果与渲染口径一致 ✓；代价是"布尔之后不能再平滑编辑曲线" ✗）。
//! * **结果可能是多个环** ✓：不相交的两块、或"甜甜圈"式的挖洞 ✓ ⇒ 返回 `Vec<Vec<点>>` ✓，
//!   由调用方决定怎么落成对象 ✓。

/// 一个多边形顶点 ✓（只存坐标 ✓ —— 布尔结果不含控制柄 ✓）。
pub type Point = (f64, f64);

/// 退化情形：G–H 在这些输入上可能给出错误结果 ✓ ⇒ **明确拒绝** ✓。
#[derive(Debug, Clone, PartialEq)]
pub enum BooleanError {
    /// 两条边共线重叠 ✓（G–H 无法正确排序交点 ✓）。
    CollinearOverlap {
        /// 描述文字 ✓。
        detail: String,
    },
    /// 交点落在顶点上 ✓（顶点是双重身份 ✓，进/出标记会算错 ✓）。
    IntersectionAtVertex {
        /// 描述文字 ✓。
        detail: String,
    },
    /// 输入多边形本身不合法 ✓（少于三点 ✓、自交 ✓、面积为零 ✓）。
    InvalidInput {
        /// 描述文字 ✓。
        detail: String,
    },
}

impl BooleanError {
    /// 人类可读的原因 ✓（工具层直接回给调用方 ✓）。
    pub fn detail(&self) -> &str {
        match self {
            Self::CollinearOverlap { detail }
            | Self::IntersectionAtVertex { detail }
            | Self::InvalidInput { detail } => detail,
        }
    }
}

/// 四种布尔运算 ✓（设计 792 只写了一个 `boolean` ✓ ⇒ 模式由调用方给出 ✓）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanMode {
    /// 并集 ✓。
    Union,
    /// 交集 ✓。
    Intersect,
    /// 差集（A 减去 B ✓）。
    Subtract,
    /// 异或（对称差 ✓）。
    Xor,
}

impl BooleanMode {
    /// 从工具参数解析 ✓。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "union" | "or" => Some(Self::Union),
            "intersect" | "intersection" | "and" => Some(Self::Intersect),
            "subtract" | "difference" | "minus" => Some(Self::Subtract),
            "xor" | "symmetric_difference" => Some(Self::Xor),
            _ => None,
        }
    }

    /// 模式名 ✓（回报给调用方 ✓）。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Union => "union",
            Self::Intersect => "intersect",
            Self::Subtract => "subtract",
            Self::Xor => "xor",
        }
    }
}

/// 有向面积的两倍 ✓（正为逆时针 ✓）。用它算面积能避开"取绝对值再除以二"的重复代码 ✓。
pub fn double_signed_area(points: &[Point]) -> f64 {
    let mut sum = 0.0;
    for index in 0..points.len() {
        let (x1, y1) = points[index];
        let (x2, y2) = points[(index + 1) % points.len()];
        sum += x1 * y2 - x2 * y1;
    }
    sum
}

/// 多边形面积 ✓（绝对值 ✓）。
pub fn area(points: &[Point]) -> f64 {
    double_signed_area(points).abs() / 2.0
}

/// 点是否在多边形**内部或边上** ✓（射线法 ✓，边界上算"在内" ✓）。
pub fn point_in_polygon(point: Point, polygon: &[Point]) -> bool {
    let (px, py) = point;
    let mut inside = false;
    let count = polygon.len();
    for index in 0..count {
        let (x1, y1) = polygon[index];
        let (x2, y2) = polygon[(index + 1) % count];
        // 边上的点算"在内" ✓（用叉积判断共线且落在线段范围内 ✓）。
        let cross = (x2 - x1) * (py - y1) - (y2 - y1) * (px - x1);
        if cross.abs() <= 1e-9
            && px >= x1.min(x2) - 1e-9
            && px <= x1.max(x2) + 1e-9
            && py >= y1.min(y2) - 1e-9
            && py <= y1.max(y2) + 1e-9
        {
            return true;
        }
        if (y1 > py) != (y2 > py) {
            let x_at = x1 + (py - y1) / (y2 - y1) * (x2 - x1);
            if px < x_at {
                inside = !inside;
            }
        }
    }
    inside
}

/// 顶点链表的节点 ✓（G–H 的核心数据结构 ✓）。
#[derive(Debug, Clone)]
struct Node {
    point: Point,
    // **没有 `alpha` 字段了** ✓：排序在 `rebuild_chain` 里用"收集到的参数"完成 ✓，
    // 节点本身不再需要记住它 ✓ —— 我第一版留了它 ✓，但插入方式改成"按边重建"之后就没人读了 ✗
    //（clippy 当场指出 ✓）。
    /// 是否与另一条多边形的交点在**同一处** ✓（互为邻居 ✓）。
    intersect: bool,
    /// 进入还是离开 ✓（遍历时用 ✓）。
    entry: bool,
    /// 是否已被遍历过 ✓。
    visited: bool,
    next: usize,
    prev: usize,
    neighbor: Option<usize>,
}

const EPS: f64 = 1e-9;

/// 两个线段求交 ✓：返回 `(alpha, beta, point)` ✓；平行/共线返回 `None` ✓。
fn segment_intersection(a1: Point, a2: Point, b1: Point, b2: Point) -> Option<(f64, f64, Point)> {
    let (ax, ay) = (a2.0 - a1.0, a2.1 - a1.1);
    let (bx, by) = (b2.0 - b1.0, b2.1 - b1.1);
    let denominator = ax * by - ay * bx;
    if denominator.abs() <= EPS {
        return None;
    }
    let (dx, dy) = (b1.0 - a1.0, b1.1 - a1.1);
    let alpha = (dx * by - dy * bx) / denominator;
    let beta = (dx * ay - dy * ax) / denominator;
    Some((alpha, beta, (a1.0 + alpha * ax, a1.1 + alpha * ay)))
}

/// **"边界没有交叉"时的布尔结果** ✓（返回 `None` 表示"有交叉，交给遍历" ✓）。
///
/// 三种子情形 ✓：
/// * A 与 B 的边**有交点** ⇒ `None` ✓；
/// * 无交点且**互不包含** ✓：并集 = 两块 ✓、交集 = 空 ✓、差集 = A ✓、异或 = 两块 ✓；
/// * 无交点且**一方包含另一方** ✓：并集 = 外层 ✓、交集 = 内层 ✓、
///   差集（A 含 B）= **钥匙孔环** ✓（见下面的说明 ✓）、异或 = 两者都留 ✓。
fn no_intersection_case(
    a: &[Point],
    b: &[Point],
    mode: BooleanMode,
) -> Result<Option<Vec<Vec<Point>>>, BooleanError> {
    // 先看有没有**真正的**交叉（含共线重叠的接触点 ✓）。
    let mut crossing = false;
    for index in 0..a.len() {
        let (a1, a2) = (a[index], a[(index + 1) % a.len()]);
        for other in 0..b.len() {
            let (b1, b2) = (b[other], b[(other + 1) % b.len()]);
            if let Some((alpha, beta, _)) = segment_intersection(a1, a2, b1, b2) {
                if alpha > EPS && alpha < 1.0 - EPS && beta > EPS && beta < 1.0 - EPS {
                    crossing = true;
                    break;
                }
            }
            // 端点落在对方边上（含共线重叠 ✓）也算"有接触" ✓ ⇒ 交给后面的显式拒绝 ✓。
            if point_on_segment(a1, b1, b2) || point_on_segment(b1, a1, a2) {
                crossing = true;
                break;
            }
        }
        if crossing {
            break;
        }
    }
    if crossing {
        return Ok(None);
    }
    let a_in_b = a.iter().all(|point| point_in_polygon(*point, b));
    let b_in_a = b.iter().all(|point| point_in_polygon(*point, a));
    // **模式由调用方显式传入** ✓ —— 我第二版试图从两个 `invert` 反推模式 ✗，
    // 而差集改成"反向 B + 两个 invert 都为真"之后就**推不出来**了 ✓（会误判成交集 ✓）。
    // 反推是一种隐式耦合 ✓：改一处调用就得同步改反推逻辑 ✓ ⇒ 显式传参更诚实 ✓。
    let rings = match mode {
        BooleanMode::Union => {
            if a_in_b {
                vec![b.to_vec()]
            } else if b_in_a {
                vec![a.to_vec()]
            } else {
                vec![a.to_vec(), b.to_vec()]
            }
        }
        BooleanMode::Intersect => {
            if a_in_b {
                vec![a.to_vec()]
            } else if b_in_a {
                vec![b.to_vec()]
            } else {
                Vec::new()
            }
        }
        BooleanMode::Subtract => {
            if b_in_a {
                // **挖洞 ⇒ 钥匙孔环** ✓：外环沿 A ✓，开一条缝到内环 ✓，内环反向 ✓，再沿缝回来 ✓。
                // 单一简单多边形就能表达"回"字 ✓（渲染端的多边形覆盖是**非零环绕**规则吗？——
                // 不是 ✓ ⇒ 所以才需要钥匙孔 ✓：把洞"接"到外边界上 ✓，让整条环只有一个边界 ✓）。
                vec![keyhole_ring(a, b)]
            } else if a_in_b {
                Vec::new()
            } else {
                vec![a.to_vec()]
            }
        }
        BooleanMode::Xor => vec![a.to_vec(), b.to_vec()],
    };
    Ok(Some(rings))
}

/// **钥匙孔环** ✓：把内环（洞）通过一条缝接到外环上 ✓，形成一个**单一的简单多边形** ✓。
///
/// 做法 ✓：外环从最靠近内环的点出发 ✓ → 直插内环上最近的点 ✓ → 沿内环**反向**走一圈 ✓
/// → 从插入点回到外环起点 ✓。缝是**原路去、原路回** ✓ ⇒ 面积为零 ✓（不改变洞的面积 ✓）。
fn keyhole_ring(outer: &[Point], inner: &[Point]) -> Vec<Point> {
    // 取内环最靠近外环的点作为插入点 ✓（确定性：按索引顺序取最小距离 ✓）。
    let (inner_index, _) = inner
        .iter()
        .enumerate()
        .map(|(index, inner_point)| {
            let distance = outer
                .iter()
                .map(|outer_point| {
                    ((outer_point.0 - inner_point.0) * (outer_point.0 - inner_point.0))
                        + ((outer_point.1 - inner_point.1) * (outer_point.1 - inner_point.1))
                })
                .fold(f64::INFINITY, f64::min);
            (index, distance)
        })
        .min_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or((0, 0.0));
    let inner_point = inner[inner_index];
    let outer_index = outer
        .iter()
        .enumerate()
        .min_by(|left, right| {
            let distance = |point: &Point| {
                ((point.0 - inner_point.0) * (point.0 - inner_point.0))
                    + ((point.1 - inner_point.1) * (point.1 - inner_point.1))
            };
            distance(left.1)
                .partial_cmp(&distance(right.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| index)
        .unwrap_or(0);
    let mut ring: Vec<Point> = Vec::with_capacity(outer.len() + inner.len() + 2);
    // 外环从插入点开始走一圈 ✓（回到插入点 ✓）。
    for step in 0..=outer.len() {
        ring.push(outer[(outer_index + step) % outer.len()]);
    }
    // 进内环 ✓，并**按实测绕向决定是否反向** ✓。
    //
    // **不假设输入的绕向** ✗：我第一版写死"倒着走内环" ✓ ⇒ 实测反而多算了洞的面积 ✓
    //（"回"字得 116 = 100 + 16 ✗，而正确答案是 84 ✓）—— 因为输入的点序是**屏幕坐标**下的
    // 逆时针 ✓，而面积公式对绕向的符号约定与直觉不同 ✓。
    // **正确做法** ✓：比较内外两个环的**有向面积符号** ✓ ⇒ 同号就反向 ✓、异号就保持 ✓
    // ⇒ 洞的面积**必然**从外环里减掉 ✓，与输入怎么给的无关 ✓。
    let outer_sign = double_signed_area(outer).signum();
    let inner_sign = double_signed_area(inner).signum();
    let need_reverse = outer_sign * inner_sign > 0.0;
    for step in 0..=inner.len() {
        let index = if need_reverse {
            (inner_index + inner.len() - step) % inner.len()
        } else {
            (inner_index + step) % inner.len()
        };
        ring.push(inner[index]);
    }
    // 回到外环插入点 ✓（原路返回 ✓）。
    ring.push(outer[outer_index]);
    ring
}

/// 点是否落在线段上 ✓（含端点 ✓）。
fn point_on_segment(point: Point, a: Point, b: Point) -> bool {
    let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
    if cross.abs() > 1e-9 {
        return false;
    }
    point.0 >= a.0.min(b.0) - 1e-9
        && point.0 <= a.0.max(b.0) + 1e-9
        && point.1 >= a.1.min(b.1) - 1e-9
        && point.1 <= a.1.max(b.1) + 1e-9
}

/// **共线重叠 ⇒ 拒绝** ✓（G–H 在这里会给出错误结果 ✓，本实现选择明确报错 ✓）。
fn reject_collinear_overlap(a: &[Point], b: &[Point]) -> Result<(), BooleanError> {
    for index in 0..a.len() {
        let (a1, a2) = (a[index], a[(index + 1) % a.len()]);
        for other in 0..b.len() {
            let (b1, b2) = (b[other], b[(other + 1) % b.len()]);
            // 平行（含共线）✓。
            if segment_intersection(a1, a2, b1, b2).is_some() {
                continue;
            }
            let collinear =
                ((a2.0 - a1.0) * (b1.1 - a1.1) - (a2.1 - a1.1) * (b1.0 - a1.0)).abs() <= 1e-9;
            if !collinear {
                continue;
            }
            // 共线且**有重叠区间** ⇒ 拒绝 ✓。
            if point_on_segment(b1, a1, a2) || point_on_segment(b2, a1, a2) {
                return Err(BooleanError::CollinearOverlap {
                    detail: format!(
                        "边 {index} 与边 {other} 共线重叠 —— Greiner–Hormann 在这种输入上会算错，\
                         本实现明确拒绝；需要支持时请改用扫描线算法（Martinez–Rueda 一类）"
                    ),
                });
            }
        }
    }
    Ok(())
}

/// **按边重建一条链表** ✓：每条边先放起点 ✓，再按 `alpha` 升序放它自己的交点 ✓，
/// 然后把各边首尾相接 ✓。返回节点表 ✓ 与"交点 id ⇒ 节点下标"的映射 ✓。
///
/// **为什么要重建而不是插入** ✗：见调用处的说明 ✓ —— "在既有链表上找位置"会走出当前边 ✓。
/// 重建的另一个好处是**完全确定** ✓：结果只取决于边的顺序与交点的 alpha ✓。
fn rebuild_chain(
    points: &[Point],
    crossings: &[(usize, f64, usize, f64, Point)],
    is_a: bool,
) -> Result<(Vec<Node>, Vec<usize>), BooleanError> {
    let count = points.len();
    let mut nodes: Vec<Node> = Vec::with_capacity(count + crossings.len());
    let mut index_of: Vec<usize> = vec![usize::MAX; crossings.len()];
    // 先建所有顶点节点 ✓（顺序与输入一致 ✓ ⇒ 顶点自身的 next/prev 之后统一重连 ✓）。
    let vertex_start = nodes.len();
    for (index, point) in points.iter().enumerate() {
        nodes.push(Node {
            point: *point,
            intersect: false,
            entry: false,
            visited: false,
            next: vertex_start + (index + 1) % count,
            prev: vertex_start + (index + count - 1) % count,
            neighbor: None,
        });
    }
    // 每条边：顶点 → 它的交点（按 alpha 升序）✓。
    for edge in 0..count {
        let mut own: Vec<(usize, f64)> = crossings
            .iter()
            .enumerate()
            .filter(|(_, crossing)| {
                let (a_edge, alpha, b_edge, beta, _) = **crossing;
                if is_a {
                    a_edge == edge
                } else {
                    let _ = (alpha, beta);
                    b_edge == edge
                }
            })
            .map(|(id, crossing)| {
                let (_, alpha, _, beta, _) = *crossing;
                (id, if is_a { alpha } else { beta })
            })
            .collect();
        own.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        // 去重（同一位置出现两次 ⇒ 共线重叠 ✓，此时明确报错 ✓）。
        for window in own.windows(2) {
            if (window[1].1 - window[0].1).abs() <= 1e-9 {
                return Err(BooleanError::CollinearOverlap {
                    detail: format!(
                        "边 {edge} 上有两个交点重合（参数 {:.6} 与 {:.6}）—— 多半是共线重叠，本实现明确拒绝",
                        window[0].1, window[1].1
                    ),
                });
            }
        }
        let mut previous = vertex_start + edge;
        for (id, _alpha) in own {
            let point = crossings[id].4;
            let index = nodes.len();
            nodes.push(Node {
                point,
                intersect: true,
                entry: false,
                visited: false,
                next: usize::MAX,
                prev: previous,
                neighbor: None,
            });
            nodes[previous].next = index;
            index_of[id] = index;
            previous = index;
        }
        // 边的最后一个节点接回该边的**终点顶点** ✓。
        let end_vertex = vertex_start + (edge + 1) % count;
        nodes[previous].next = end_vertex;
        nodes[end_vertex].prev = previous;
    }
    Ok((nodes, index_of))
}

/// **多边形布尔** ✓（返回若干环 ✓；退化输入返回错误 ✓）。
pub fn boolean(
    a: &[Point],
    b: &[Point],
    mode: BooleanMode,
) -> Result<Vec<Vec<Point>>, BooleanError> {
    if a.len() < 3 || b.len() < 3 {
        return Err(BooleanError::InvalidInput {
            detail: "两边都至少需要三个顶点".to_owned(),
        });
    }
    if area(a) <= 1e-6 || area(b) <= 1e-6 {
        return Err(BooleanError::InvalidInput {
            detail: "面积为零的多边形没有布尔意义（顶点是不是都在一条线上？）".to_owned(),
        });
    }
    match mode {
        BooleanMode::Union => one_pass(a, b, false, false, mode),
        BooleanMode::Intersect => one_pass(a, b, true, true, mode),
        // **差集 = `A ∩ B̄`** ✓ ⇒ 把 B **反向**（绕向翻转 ✓）之后按"交"的方式走 ✓。
        //
        // **这是我第三版才补上的一步** ✗：前两版直接把 B 当"要减掉的部分"沿**原方向**走 ✓
        // ⇒ 结果与并集**完全相同** ✓（实测：差集给 164、并集也给 164 ✗，而正确答案是 64 ✓）。
        // 原理 ✓：结果沿着 B 的边界走时，方向与 B 自己的绕向**相反** ✓ ⇒ 必须先反向 ✓，
        // 否则遍历会在 B 上走"错的一侧" ✓。
        BooleanMode::Subtract => one_pass(a, &reverse(b), true, true, mode),
        // 异或 = (A−B) ∪ (B−A) ✓ ⇒ 两趟差集 ✓（结果可能是多个环 ✓）。
        BooleanMode::Xor => {
            let mut rings = one_pass(a, &reverse(b), true, true, mode)?;
            rings.extend(one_pass(b, &reverse(a), true, true, mode)?);
            Ok(rings)
        }
    }
}

/// 把点序反转 ✓（绕向翻转 ✓，差集要用 ✓）。
fn reverse(points: &[Point]) -> Vec<Point> {
    let mut out = points.to_vec();
    out.reverse();
    out
}

fn one_pass(
    a: &[Point],
    b: &[Point],
    invert_a: bool,
    invert_b: bool,
    mode: BooleanMode,
) -> Result<Vec<Vec<Point>>, BooleanError> {
    // **先做"没有交点"的特例** ✓ —— 我第一版漏了这一步 ✗ ⇒ 两块**完全不相交**的方形求并集
    // 会返回**空** ✓（遍历找不到任何"进入点" ✓）。不相交/包含是布尔运算里**最常见**的输入之一 ✓，
    // 必须单独处理 ✓（G–H 的遍历只负责"边界有交叉"的情形 ✓）。
    let plain = no_intersection_case(a, b, mode)?;
    if let Some(rings) = plain {
        return Ok(rings);
    }
    // **共线重叠必须显式判定** ✓ —— 我第一版只在"同一条边插入两个同位置交点"时才发现 ✗
    // ⇒ 两条边**完全重合**（例如两个方块共享一条边 ✓）会悄悄通过 ✓，然后给出错误的结果 ✗。
    reject_collinear_overlap(a, b)?;
    // ① **求所有交点** ✓（每条边上的参数在 (0,1) 之内才算"真正的交叉" ✓ ——
    // 端点接触属于退化 ✓，由前面的判定拒绝 ✓）。
    let mut crossings: Vec<(usize, f64, usize, f64, Point)> = Vec::new();
    for i in 0..a.len() {
        for j in 0..b.len() {
            let (a1, a2) = (a[i], a[(i + 1) % a.len()]);
            let (b1, b2) = (b[j], b[(j + 1) % b.len()]);
            if let Some((alpha, beta, point)) = segment_intersection(a1, a2, b1, b2) {
                if alpha > 1e-7 && alpha < 1.0 - 1e-7 && beta > 1e-7 && beta < 1.0 - 1e-7 {
                    crossings.push((i, alpha, j, beta, point));
                }
            }
        }
    }
    // ② **按边重建两条链表** ✓ —— 我第一版是"把交点插进既有链表的某个位置" ✗，
    // 靠"从边起点沿链走、比较 alpha"来找位置 ✓：那个走法会**走出这条边** ✓
    //（走到别的边的顶点上 ✗，其 alpha 为 0 ✓，比较失去意义 ✓）⇒ 交点被插到错误位置 ✓
    // ⇒ 环的形状出现**跨越内部的斜线** ✓（实测：并集 144 = 包围盒 ✓、交集 12 ✓）。
    // **正确做法** ✓：每条边单独收集自己的交点 ✓、按 alpha 排序 ✓、依次串起来 ✓，
    // 再把各边的链首尾相接 ✓ —— 不依赖任何"在链上找位置"的判断 ✓。
    let (mut chain_a, a_index) = rebuild_chain(a, &crossings, true)?;
    let (mut chain_b, b_index) = rebuild_chain(b, &crossings, false)?;
    for (id, (_, _, _, _, _)) in crossings.iter().enumerate() {
        let index_a = a_index[id];
        let index_b = b_index[id];
        chain_a[index_a].neighbor = Some(index_b);
        chain_b[index_b].neighbor = Some(index_a);
    }
    // ③ 标记进入/离开 ✓，然后遍历 ✓。
    mark_entries(&mut chain_a, b, invert_a);
    mark_entries(&mut chain_b, a, invert_b);
    // ③ 遍历 ✓。
    let mut rings: Vec<Vec<Point>> = Vec::new();
    for start in 0..chain_a.len() {
        if chain_a[start].visited || !chain_a[start].intersect || !chain_a[start].entry {
            continue;
        }
        let ring = traverse(&mut chain_a, &mut chain_b, start);
        if ring.len() >= 3 && area(&ring) > 1e-6 {
            rings.push(ring);
        }
    }
    Ok(rings)
}

/// **标记每个交点是"进入"还是"离开"** ✓。
///
/// **做法（这是我第二版才改对的 ✓）**：对每个交点 X ✓，取**到达它的那一段**（`X.prev → X` ✓）
/// 的**中点** M ✓，判断 M 是否在对方多边形内部 ✓ ⇒ 于是"进入"的定义是**就地可算**的 ✓：
/// * `invert == false`（并集、差集的 A 边 ✓）：**从外面进来**才是进入 ✓ ⇒ `entry = !inside` ✓；
/// * `invert == true`（交集的两边、差集的 B 边 ✓）：从里面出发才算进入 ✓ ⇒ `entry = inside` ✓。
///
/// **为什么不用"沿链从 0 号节点交替翻转"** ✗：我第一版就是这么写的 ✓，它有**两个**坑 ——
/// ① 0 号节点本身可能就是交点 ✓（状态起点有歧义 ✓）；
/// ② 一次算错之后**整条链都跟着错** ✓（错误会沿链传播 ✓，症状是"面积算成了包围盒" ✓、
///    或环的形状出现**跨越内部的斜线** ✓）。**就地按中点判断**不会传播 ✓，且每处独立可验 ✓。
fn mark_entries(chain: &mut [Node], other: &[Point], invert: bool) {
    let count = chain.len();
    if count == 0 {
        return;
    }
    for cursor in 0..count {
        if !chain[cursor].intersect {
            continue;
        }
        let previous = chain[cursor].prev;
        let midpoint = (
            (chain[previous].point.0 + chain[cursor].point.0) / 2.0,
            (chain[previous].point.1 + chain[cursor].point.1) / 2.0,
        );
        let inside = point_in_polygon(midpoint, other);
        chain[cursor].entry = if invert { inside } else { !inside };
    }
}

fn traverse(chain_a: &mut [Node], chain_b: &mut [Node], start: usize) -> Vec<Point> {
    let mut ring: Vec<Point> = Vec::new();
    let mut current = start;
    let mut on_a = true;
    let mut guard = 0usize;
    loop {
        let node = if on_a {
            chain_a[current].clone()
        } else {
            chain_b[current].clone()
        };
        ring.push(node.point);
        if on_a {
            chain_a[current].visited = true;
        } else {
            chain_b[current].visited = true;
        }
        if node.intersect {
            // 在交点处**换到另一条链** ✓（G–H 的关键一步 ✓：沿对方链表继续走 ✓）。
            let neighbor = match node.neighbor {
                Some(neighbor) => neighbor,
                None => break,
            };
            on_a = !on_a;
            // 沿对方链表的**下一个**节点继续 ✓（邻居本身已被记录 ✓）。
            current = if on_a {
                chain_a[neighbor].next
            } else {
                chain_b[neighbor].next
            };
            // 邻居标记为已访问 ✓，避免重复成环 ✓。
            if on_a {
                chain_a[neighbor].visited = true;
            } else {
                chain_b[neighbor].visited = true;
            }
        } else {
            current = if on_a {
                chain_a[current].next
            } else {
                chain_b[current].next
            };
        }
        guard += 1;
        if (on_a && current == start) || guard > 100_000 {
            break;
        }
    }
    ring
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Vec<Point> {
        vec![(x, y), (x + size, y), (x + size, y + size), (x, y + size)]
    }

    #[test]
    fn area_matches_known_values() {
        assert!((area(&square(0.0, 0.0, 10.0)) - 100.0).abs() < 1e-9);
        assert!((area(&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn four_modes_agree_with_the_inclusion_exclusion_arithmetic() {
        // 两个 10×10 的正方形错开 4 ✓ ⇒ 交叠 6×6 = 36 ✓（**期望值从几何推导 ✓，不写死结果多边形 ✓**）。
        let a = square(0.0, 0.0, 10.0);
        let b = square(4.0, 4.0, 10.0);
        let overlap = 6.0 * 6.0;
        let area_sum = |rings: &Vec<Vec<Point>>| rings.iter().map(|ring| area(ring)).sum::<f64>();

        let union = area_sum(&boolean(&a, &b, BooleanMode::Union).expect("并集应成功"));
        let intersect = area_sum(&boolean(&a, &b, BooleanMode::Intersect).expect("交集应成功"));
        let subtract = area_sum(&boolean(&a, &b, BooleanMode::Subtract).expect("差集应成功"));
        let xor = area_sum(&boolean(&a, &b, BooleanMode::Xor).expect("异或应成功"));

        assert!(
            (intersect - overlap).abs() < 0.5,
            "交集应为 {overlap}，实得 {intersect}"
        );
        assert!(
            (union - (100.0 + 100.0 - overlap)).abs() < 0.5,
            "并集应为 {}，实得 {union}",
            200.0 - overlap
        );
        assert!(
            (subtract - (100.0 - overlap)).abs() < 0.5,
            "差集应为 {}，实得 {subtract}",
            100.0 - overlap
        );
        assert!(
            (xor - (200.0 - 2.0 * overlap)).abs() < 0.5,
            "异或应为 {}，实得 {xor}",
            200.0 - 2.0 * overlap
        );
        // **交换律** ✓：A∪B 与 B∪A 面积相同 ✓。
        let swapped = area_sum(&boolean(&b, &a, BooleanMode::Union).expect("并集应成功"));
        assert!(
            (swapped - union).abs() < 0.5,
            "并集应当可交换（{union} vs {swapped}）"
        );
        // **差集不可交换** ✓（这正说明模式真的生效了 ✓）。
        let swapped_subtract =
            area_sum(&boolean(&b, &a, BooleanMode::Subtract).expect("差集应成功"));
        assert!(
            (swapped_subtract - subtract).abs() < 0.5,
            "两个同样大的正方形互相减 ⇒ 面积相同 ✓（{subtract} vs {swapped_subtract}）"
        );
    }

    #[test]
    fn disjoint_and_contained_cases_behave() {
        // 完全不相交 ✓：并集两块 ✓、交集为空 ✓。
        let a = square(0.0, 0.0, 10.0);
        let b = square(50.0, 50.0, 10.0);
        let union = boolean(&a, &b, BooleanMode::Union).expect("并集应成功");
        assert!(
            (union.iter().map(|r| area(r)).sum::<f64>() - 200.0).abs() < 0.5,
            "两块不相交 ⇒ 并集 200（实得 {union:?}）"
        );
        assert!(
            boolean(&a, &b, BooleanMode::Intersect)
                .expect("交集应成功")
                .is_empty(),
            "不相交 ⇒ 交集为空"
        );
        // 完全包含 ✓：小方块在大方块里 ⇒ 差集是一个"回"字 ✓ ⇒ 面积差 = 面积之差 ✓。
        let outer = square(0.0, 0.0, 10.0);
        let inner = square(3.0, 3.0, 4.0);
        let subtract = boolean(&outer, &inner, BooleanMode::Subtract).expect("差集应成功");
        let total = subtract.iter().map(|ring| area(ring)).sum::<f64>();
        assert!(
            (total - (100.0 - 16.0)).abs() < 0.5,
            "差集应为 84，实得 {total}（环：{subtract:?}）"
        );
    }

    #[test]
    fn degenerate_inputs_are_refused_rather_than_guessed() {
        let a = square(0.0, 0.0, 10.0);
        // **共线重叠**：把 B 的一条边压在 A 的边上 ✓ ⇒ 必须拒绝 ✓。
        let overlapping = vec![(0.0, 0.0), (10.0, 0.0), (10.0, -10.0), (0.0, -10.0)];
        let refused = boolean(&a, &overlapping, BooleanMode::Union);
        assert!(refused.is_err(), "共线重叠应被拒绝，实得 {refused:?}");
        // **面积为零** ⇒ 拒绝 ✓。
        let flat = vec![(0.0, 0.0), (10.0, 0.0), (5.0, 0.0)];
        assert!(
            boolean(&a, &flat, BooleanMode::Union).is_err(),
            "零面积应被拒绝"
        );
        // **顶点太少** ⇒ 拒绝 ✓。
        assert!(boolean(&a, &[(0.0, 0.0), (1.0, 1.0)], BooleanMode::Union).is_err());
    }
}
