//! 参考图只读分析：单质量块中轴骨架提取（`reference_trace_region`）。
//!
//! 本模块只做一件事：给定一幅参考图像与一个显式掩膜（掩膜 PNG 或多边形），
//! 返回该掩膜内最大连通域的中轴直径路径、逐点半径、分层 RDP 简化中心线、
//! 结构张量场角度与掩膜内平均色。工具是只读的：不接受 `doc_id`／`layer_id`，
//! 不调用文档存储，不写任何原子；图像与掩膜路径必须落在工作区 `--root` 内
//!（走 `file_source::resolve_local_path` 的唯一沙箱口径）。
//!
//! 算法来源（全部是已验证过的代码，不另起一套未经验证的实现）：
//! - 欧氏距离变换（Felzenszwalb）、Zhang–Suen 细化、两次 BFS 直径主干、
//!   RDP 简化、Scharr 结构张量：转录自专家修订版 study.rs，
//!   其骨架已在 600×1000 柏树干段上验证（见模块末测试）。
//! - 骨架分段排序与短分支剪除（`path_count` 诊断用）：转录自参考解构实验原型。
//!
//! 刻意不做的事（今天已证伪或未验证，写进来就是误导）：
//! - 不做颜色分割：修订版 OKLab 区域生长在真实厚涂体块上 FAIL
//!   （最大块仅占主干 14.7%／18.0%，是纹理条带不是质量块）。
//!   调用方必须自己给出掩膜；工具只取其中最大连通域。
//! - 不输出接缝：`seam_softness` 的掩膜邻接定义已证伪（硬轮廓报 0.944 软）。
//! - 不输出压感映射：`pressure = r / Rmax` 的宽度映射已证伪
//!   （传 size 则宽度钉死，不传则原生半径太小，两种模式下压感都不控宽度）。
//! - 不输出 coherence：结构张量 coherence 只在内部作场角度的加权权重，
//!   绝不输出、绝不设阈值——真实厚涂区 coherence 仅 0.15~0.25，
//!   绝对二分（定向→1／平涂→0）已证伪。
//! - 不输出作画阶段／媒介／速度步长推断：皆未经验证。
//!
//! 计算窗口：为省内存与耗时，EDT／细化／张量只在掩膜包围盒外扩
//! `tensor_radius + 2` 的裁剪窗内算。外扩保证窗内掩膜像素的邻域完整，
//! 故裁剪结果与整图计算逐位等价（EDT 的最近背景、细化的 1 邻域、
//! 张量的平滑半径皆落在窗内）；包围盒本身由掩膜决定，不依赖调用方裁剪。
//!
//! 输出数字的诚实边界（也在工具 summary 里写明）：
//! - 逐点半径仅量级参考：分叉迂回处噪声大，不可当精确宽度执行。
//! - 中轴端点按半径内收（细化固有行为）。
//! - 宽平区域（如柏树顶带）骨架端点不稳定：掩膜形状的共有局限，不是回归。
//! - 掩膜触及图像边界时，边界处骨架可能被截断。

use super::*;

/// 结构张量场：每像素切线方向角（弧度，[-π/2, π/2]）与 coherence（仅作角度加权用，不输出）。
struct OrientationField {
    tangent_angle: Vec<f32>,
    coherence: Vec<f32>,
}

/// Felzenszwalb O(W·H) 欧氏距离变换：返回每像素到最近背景像素的欧氏距离。
fn euclidean_distance_transform(mask: &[bool], width: usize, height: usize) -> Vec<f32> {
    const INF: f32 = 1e18;
    let mut grid = vec![0.0f32; width * height];
    for i in 0..(width * height) {
        grid[i] = if mask[i] { INF } else { 0.0 };
    }
    let mut f = vec![0.0f32; height.max(width)];
    let mut d = vec![0.0f32; height.max(width)];
    let mut v = vec![0usize; height.max(width)];
    let mut z = vec![0.0f32; height.max(width) + 1];
    for x in 0..width {
        for y in 0..height {
            f[y] = grid[y * width + x];
        }
        edt_1d(
            &f[..height],
            &mut d[..height],
            &mut v[..height],
            &mut z[..=height],
        );
        for y in 0..height {
            grid[y * width + x] = d[y];
        }
    }
    for y in 0..height {
        let row_offset = y * width;
        f[..width].copy_from_slice(&grid[row_offset..row_offset + width]);
        edt_1d(
            &f[..width],
            &mut d[..width],
            &mut v[..width],
            &mut z[..=width],
        );
        for x in 0..width {
            grid[row_offset + x] = d[x].sqrt();
        }
    }
    grid
}

/// Felzenszwalb & Huttenlocher 一维下包络抛物线距离变换。
fn edt_1d(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        let fq = f[q] + (q * q) as f32;
        let mut s;
        loop {
            let vk = v[k];
            let fvk = f[vk] + (vk * vk) as f32;
            s = (fq - fvk) / (2.0 * (q as f32 - vk as f32));
            if s > z[k] {
                break;
            }
            if k == 0 {
                break;
            }
            k -= 1;
        }
        if s > z[k] {
            k += 1;
        }
        v[k] = q;
        z[k] = s;
        z[k + 1] = f32::INFINITY;
    }
    k = 0;
    for (q, slot) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let dx = q as f32 - v[k] as f32;
        *slot = dx * dx + f[v[k]];
    }
}

/// Zhang–Suen 二值细化，返回 1px 宽的拓扑连通骨架。
fn zhang_suen_thinning(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut skel = mask.to_vec();
    let mut to_clear = Vec::new();
    for x in 0..width {
        skel[x] = false;
        skel[(height - 1) * width + x] = false;
    }
    for y in 0..height {
        skel[y * width] = false;
        skel[y * width + (width - 1)] = false;
    }
    loop {
        let mut changed = false;
        for step in 0..2 {
            to_clear.clear();
            for y in 1..(height - 1) {
                for x in 1..(width - 1) {
                    let idx = y * width + x;
                    if !skel[idx] {
                        continue;
                    }
                    // P2..P9 顺时针邻域：P2（上）、P3（右上）、P4（右）、P5（右下）、
                    // P6（下）、P7（左下）、P8（左）、P9（左上）。
                    let p2 = skel[(y - 1) * width + x] as u8;
                    let p3 = skel[(y - 1) * width + x + 1] as u8;
                    let p4 = skel[y * width + x + 1] as u8;
                    let p5 = skel[(y + 1) * width + x + 1] as u8;
                    let p6 = skel[(y + 1) * width + x] as u8;
                    let p7 = skel[(y + 1) * width + x - 1] as u8;
                    let p8 = skel[y * width + x - 1] as u8;
                    let p9 = skel[(y - 1) * width + x - 1] as u8;
                    let b = p2 + p3 + p4 + p5 + p6 + p7 + p8 + p9;
                    if !(2..=6).contains(&b) {
                        continue;
                    }
                    let neighbors = [p2, p3, p4, p5, p6, p7, p8, p9, p2];
                    let mut a = 0u8;
                    for i in 0..8 {
                        if neighbors[i] == 0 && neighbors[i + 1] == 1 {
                            a += 1;
                        }
                    }
                    if a != 1 {
                        continue;
                    }
                    let cond = if step == 0 {
                        (p2 * p4 * p6 == 0) && (p4 * p6 * p8 == 0)
                    } else {
                        (p2 * p4 * p8 == 0) && (p2 * p6 * p8 == 0)
                    };
                    if cond {
                        to_clear.push(idx);
                    }
                }
            }
            if !to_clear.is_empty() {
                changed = true;
                for &idx in &to_clear {
                    skel[idx] = false;
                }
            }
        }
        if !changed {
            break;
        }
    }
    skel
}

/// 骨架图上两次 BFS 的直径主干（树图上精确；含环时为近似，见验证收据局限）。
fn extract_main_trunk_path(skeleton: &[bool], width: usize, height: usize) -> Vec<(usize, usize)> {
    let first_node = match skeleton.iter().position(|&b| b) {
        Some(idx) => idx,
        None => return Vec::new(),
    };
    // 第 1 次 BFS：从任意骨架点出发，找到最远的端点 U。
    let (u, _) = bfs_farthest(first_node, skeleton, width, height);
    // 第 2 次 BFS：从端点 U 出发，找到最远的另一端 V，并回溯出 U → V 的完整主干路径。
    let (v, parent) = bfs_farthest(u, skeleton, width, height);
    let mut path = Vec::new();
    let mut curr = Some(v);
    while let Some(node) = curr {
        let x = node % width;
        let y = node / width;
        path.push((x, y));
        if node == u {
            break;
        }
        curr = parent[node];
    }
    path.reverse();
    path
}

fn bfs_farthest(
    start: usize,
    skeleton: &[bool],
    width: usize,
    height: usize,
) -> (usize, Vec<Option<usize>>) {
    let n = width * height;
    let mut visited = vec![false; n];
    let mut parent = vec![None; n];
    let mut queue = std::collections::VecDeque::new();
    visited[start] = true;
    queue.push_back(start);
    let mut last_popped = start;
    let dirs: [(isize, isize); 8] = [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];
    while let Some(curr) = queue.pop_front() {
        last_popped = curr;
        let cx = (curr % width) as isize;
        let cy = (curr / width) as isize;
        for &(dx, dy) in &dirs {
            let nx = cx + dx;
            let ny = cy + dy;
            if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                let n_idx = (ny as usize) * width + (nx as usize);
                if skeleton[n_idx] && !visited[n_idx] {
                    visited[n_idx] = true;
                    parent[n_idx] = Some(curr);
                    queue.push_back(n_idx);
                }
            }
        }
    }
    (last_popped, parent)
}

/// Ramer–Douglas–Peucker 路径抽稀，返回保留点的下标。
fn rdp_simplify_indices(pts: &[(usize, usize)], epsilon: f32) -> Vec<usize> {
    if pts.len() <= 2 {
        return (0..pts.len()).collect();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    rdp_recursive(pts, 0, pts.len() - 1, epsilon, &mut keep);
    keep.iter()
        .enumerate()
        .filter_map(|(i, &k)| if k { Some(i) } else { None })
        .collect()
}

fn rdp_recursive(
    pts: &[(usize, usize)],
    start: usize,
    end: usize,
    epsilon: f32,
    keep: &mut [bool],
) {
    if end <= start + 1 {
        return;
    }
    let (x1, y1) = (pts[start].0 as f32, pts[start].1 as f32);
    let (x2, y2) = (pts[end].0 as f32, pts[end].1 as f32);
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len_sq = dx * dx + dy * dy;
    let mut max_dist = 0.0f32;
    let mut max_idx = start;
    for (i, &(px_u, py_u)) in pts.iter().enumerate().skip(start + 1).take(end - start - 1) {
        let (px, py) = (px_u as f32, py_u as f32);
        let dist = if len_sq < 1e-6 {
            ((px - x1) * (px - x1) + (py - y1) * (py - y1)).sqrt()
        } else {
            ((dy * px - dx * py + x2 * y1 - y2 * x1).abs()) / len_sq.sqrt()
        };
        if dist > max_dist {
            max_dist = dist;
            max_idx = i;
        }
    }
    if max_dist > epsilon {
        keep[max_idx] = true;
        rdp_recursive(pts, start, max_idx, epsilon, keep);
        rdp_recursive(pts, max_idx, end, epsilon, keep);
    }
}

/// Scharr 梯度 + 可分离均值平滑的结构张量场。
fn compute_structure_tensor(
    rgba: &[u8],
    width: usize,
    height: usize,
    window_radius: usize,
) -> OrientationField {
    let n = width * height;
    let mut lum = vec![0.0f32; n];
    for i in 0..n {
        let r = rgba[i * 4] as f32 / 255.0;
        let g = rgba[i * 4 + 1] as f32 / 255.0;
        let b = rgba[i * 4 + 2] as f32 / 255.0;
        lum[i] = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    let mut j_xx = vec![0.0f32; n];
    let mut j_xy = vec![0.0f32; n];
    let mut j_yy = vec![0.0f32; n];
    for y in 1..height.saturating_sub(1) {
        for x in 1..width.saturating_sub(1) {
            let idx = |dx: isize, dy: isize| -> f32 {
                lum[((y as isize + dy) as usize) * width + (x as isize + dx) as usize]
            };
            // Scharr 3×3 核（比普通 Sobel 旋转不变性更好，角度更准）。
            let gx = -3.0 * idx(-1, -1) + 3.0 * idx(1, -1) - 10.0 * idx(-1, 0) + 10.0 * idx(1, 0)
                - 3.0 * idx(-1, 1)
                + 3.0 * idx(1, 1);
            let gy = -3.0 * idx(-1, -1) - 10.0 * idx(0, -1) - 3.0 * idx(1, -1)
                + 3.0 * idx(-1, 1)
                + 10.0 * idx(0, 1)
                + 3.0 * idx(1, 1);
            let p = y * width + x;
            j_xx[p] = gx * gx;
            j_xy[p] = gx * gy;
            j_yy[p] = gy * gy;
        }
    }
    let s_xx = box_blur_2d(&j_xx, width, height, window_radius);
    let s_xy = box_blur_2d(&j_xy, width, height, window_radius);
    let s_yy = box_blur_2d(&j_yy, width, height, window_radius);
    let mut tangent_angle = vec![0.0f32; n];
    let mut coherence = vec![0.0f32; n];
    for i in 0..n {
        let xx = s_xx[i];
        let xy = s_xy[i];
        let yy = s_yy[i];
        // 梯度主方向 = 0.5 · atan2(2·Jxy, Jxx − Jyy)；
        // 笔触切线方向垂直于梯度主方向（加 π/2），等价于 0.5 · atan2(−2·Jxy, Jyy − Jxx)。
        let angle = 0.5 * (-2.0 * xy).atan2(yy - xx);
        tangent_angle[i] = angle;
        let trace = xx + yy;
        if trace > 1e-6 {
            let diff = ((xx - yy) * (xx - yy) + 4.0 * xy * xy).sqrt();
            let c = diff / trace;
            coherence[i] = (c * c).clamp(0.0, 1.0);
        }
    }
    OrientationField {
        tangent_angle,
        coherence,
    }
}

fn box_blur_2d(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    if r == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let mut tmp = vec![0.0f32; w * h];
    let mut dst = vec![0.0f32; w * h];
    for y in 0..h {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for x in 0..=r.min(w - 1) {
            sum += src[y * w + x];
            count += 1;
        }
        for x in 0..w {
            tmp[y * w + x] = sum / (count as f32);
            if x + r + 1 < w {
                sum += src[y * w + x + r + 1];
                count += 1;
            }
            if x >= r {
                sum -= src[y * w + x - r];
                count -= 1;
            }
        }
    }
    for x in 0..w {
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for y in 0..=r.min(h - 1) {
            sum += tmp[y * w + x];
            count += 1;
        }
        for y in 0..h {
            dst[y * w + x] = sum / (count as f32);
            if y + r + 1 < h {
                sum += tmp[(y + r + 1) * w + x];
                count += 1;
            }
            if y >= r {
                sum -= tmp[(y - r) * w + x];
                count -= 1;
            }
        }
    }
    dst
}

/// 无向角度的 coherence 加权二倍角圆均值，返回度数（0~180）。
/// 权重为 0 的像素不贡献方向；全部权重为 0 时返回 0.0（平坦区无方向，不报错冒充）。
fn weighted_mean_angle_deg(angles: &[f32], weights: &[f32]) -> f32 {
    let mut sin_2a = 0.0f32;
    let mut cos_2a = 0.0f32;
    for (a, w) in angles.iter().zip(weights.iter()) {
        sin_2a += w * (2.0 * a).sin();
        cos_2a += w * (2.0 * a).cos();
    }
    if sin_2a == 0.0 && cos_2a == 0.0 {
        return 0.0;
    }
    let mut deg = (0.5 * sin_2a.atan2(cos_2a)).to_degrees();
    if deg < 0.0 {
        deg += 180.0;
    }
    deg
}

/// 骨架像素的 8 邻域度数。
fn skel_degree(skel: &[bool], w: usize, h: usize, x: usize, y: usize) -> usize {
    let mut n = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0
                && nx < w as i32
                && ny >= 0
                && ny < h as i32
                && skel[ny as usize * w + nx as usize]
            {
                n += 1;
            }
        }
    }
    n
}

/// 把骨架像素走成路径：分叉点（度 ≥ 3）截断路径；环（无端点）从任意未访点起走。
/// 返回的路径互不重叠（分叉像素归属先到的那条路径）。
fn order_skeleton(skel: &[bool], w: usize, h: usize) -> Vec<Vec<(usize, usize)>> {
    let mut visited = vec![false; w * h];
    let mut paths = Vec::new();
    let neighbors = |x: usize, y: usize| -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0
                    && nx < w as i32
                    && ny >= 0
                    && ny < h as i32
                    && skel[ny as usize * w + nx as usize]
                {
                    out.push((nx as usize, ny as usize));
                }
            }
        }
        out
    };
    let mut queue: std::collections::VecDeque<(usize, usize)> = std::collections::VecDeque::new();
    for y in 0..h {
        for x in 0..w {
            if skel[y * w + x] && skel_degree(skel, w, h, x, y) == 1 {
                queue.push_back((x, y));
            }
        }
    }
    while let Some(start) = queue.pop_front() {
        if visited[start.1 * w + start.0] {
            continue;
        }
        let mut path = vec![start];
        visited[start.1 * w + start.0] = true;
        let mut cur = start;
        loop {
            let deg = skel_degree(skel, w, h, cur.0, cur.1);
            if deg != 2 && path.len() > 1 {
                break;
            }
            let mut next = None;
            for nb in neighbors(cur.0, cur.1) {
                if !visited[nb.1 * w + nb.0] {
                    next = Some(nb);
                    break;
                }
            }
            match next {
                Some(nb) => {
                    path.push(nb);
                    visited[nb.1 * w + nb.0] = true;
                    cur = nb;
                }
                None => break,
            }
        }
        if path.len() >= 2 {
            paths.push(path);
        }
    }
    for y in 0..h {
        for x in 0..w {
            if skel[y * w + x] && !visited[y * w + x] {
                let mut path = vec![(x, y)];
                visited[y * w + x] = true;
                let mut cur = (x, y);
                loop {
                    let mut next = None;
                    for nb in neighbors(cur.0, cur.1) {
                        if !visited[nb.1 * w + nb.0] {
                            next = Some(nb);
                            break;
                        }
                    }
                    match next {
                        Some(nb) => {
                            path.push(nb);
                            visited[nb.1 * w + nb.0] = true;
                            cur = nb;
                        }
                        None => break,
                    }
                }
                if path.len() >= 2 {
                    paths.push(path);
                }
            }
        }
    }
    paths
}

/// 4 连通最大连通域：单质量块语义——工具只分析掩膜内最大的一块，
/// 其余碎块不进入骨架（碎块是阈值噪声或邻接体块，不是一笔）。
fn largest_connected_component(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut best: Vec<bool> = vec![false; width * height];
    let mut best_count = 0usize;
    let mut visited = vec![false; width * height];
    let mut queue = std::collections::VecDeque::new();
    for start in 0..(width * height) {
        if !mask[start] || visited[start] {
            continue;
        }
        let mut component = vec![false; width * height];
        let mut count = 0usize;
        visited[start] = true;
        queue.push_back(start);
        while let Some(curr) = queue.pop_front() {
            component[curr] = true;
            count += 1;
            let cx = (curr % width) as isize;
            let cy = (curr / width) as isize;
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let nx = cx + dx;
                let ny = cy + dy;
                if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                    let n_idx = (ny as usize) * width + (nx as usize);
                    if mask[n_idx] && !visited[n_idx] {
                        visited[n_idx] = true;
                        queue.push_back(n_idx);
                    }
                }
            }
        }
        if count > best_count {
            best_count = count;
            best = component;
        }
    }
    best
}

/// 掩膜包围盒 [x0, y0, x1, y1]（含端点）；空掩膜返回 None。
fn mask_bbox(mask: &[bool], width: usize, height: usize) -> Option<[usize; 4]> {
    let mut bbox: Option<[usize; 4]> = None;
    for y in 0..height {
        for x in 0..width {
            if mask[y * width + x] {
                bbox = Some(match bbox {
                    None => [x, y, x, y],
                    Some([x0, y0, x1, y1]) => [x0.min(x), y0.min(y), x1.max(x), y1.max(y)],
                });
            }
        }
    }
    bbox
}

/// 掩膜内平均色（sRGB 字节，截断取整，与两版验证代码一致）。
fn mean_srgb(rgba: &[u8], mask: &[bool], width: usize, height: usize) -> [u8; 3] {
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for y in 0..height {
        for x in 0..width {
            if mask[y * width + x] {
                let i = (y * width + x) * 4;
                sum[0] += rgba[i] as u64;
                sum[1] += rgba[i + 1] as u64;
                sum[2] += rgba[i + 2] as u64;
                n += 1;
            }
        }
    }
    if n == 0 {
        return [0, 0, 0];
    }
    [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8]
}

/// RGBA 掩膜像素规则：alpha ≥ 128 且任一 RGB 通道 > 0。
/// 全透明像素与纯黑像素皆不算掩膜（白底黑字掩膜与透明底白字掩膜皆可用）。
fn mask_from_rgba(rgba: &[u8], width: usize, height: usize) -> Vec<bool> {
    let mut mask = vec![false; width * height];
    for i in 0..(width * height) {
        let r = rgba[i * 4];
        let g = rgba[i * 4 + 1];
        let b = rgba[i * 4 + 2];
        let a = rgba[i * 4 + 3];
        mask[i] = a >= 128 && (r > 0 || g > 0 || b > 0);
    }
    mask
}

/// 多边形填充（even-odd 规则，图像像素坐标，超出图像部分裁掉）。
fn fill_polygon(polygon: &[(f64, f64)], width: usize, height: usize) -> Vec<bool> {
    let mut mask = vec![false; width * height];
    if polygon.len() < 3 {
        return mask;
    }
    let mut min_y = height;
    let mut max_y = 0usize;
    for &(_, y) in polygon {
        let yi = y.floor().max(0.0) as usize;
        min_y = min_y.min(yi);
        max_y = max_y.max(yi.min(height.saturating_sub(1)));
    }
    for y in min_y..=max_y {
        let yc = y as f64 + 0.5;
        let mut crossings: Vec<f64> = Vec::new();
        for i in 0..polygon.len() {
            let (x0, y0) = polygon[i];
            let (x1, y1) = polygon[(i + 1) % polygon.len()];
            if (y0 <= yc && y1 > yc) || (y1 <= yc && y0 > yc) {
                let t = (yc - y0) / (y1 - y0);
                crossings.push(x0 + t * (x1 - x0));
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for pair in crossings.chunks_exact(2) {
            // 像素中心规则：像素 (x, y) 的中心 (x + 0.5) 落在交点区间内才算入。
            let x_start = (pair[0] - 0.5).ceil().max(0.0) as usize;
            let x_end = (pair[1] - 0.5).ceil() as usize;
            for x in x_start..x_end.min(width) {
                mask[y * width + x] = true;
            }
        }
    }
    mask
}

/// 单质量块骨架提取的纯函数输出。
struct TraceOutput {
    /// 未简化的直径主干（图像坐标）与对齐的逐点半径。
    diameter_points: Vec<(usize, usize)>,
    diameter_radii: Vec<f32>,
    /// 细层中心线（RDP，`dp_eps_fine`）与对齐的逐点半径。
    fine_points: Vec<(usize, usize)>,
    fine_radii: Vec<f32>,
    /// 粗层中心线（RDP，`max_radius · 0.15`）：只作 block-in，不可当精细中轴用。
    coarse_points: Vec<(usize, usize)>,
    coarse_radii: Vec<f32>,
    /// 路径上最大半径（即验证口径里的 base_radius）。
    base_radius: f32,
    /// 掩膜内平均色。
    mean_color: [u8; 3],
    /// 掩膜像素上 coherence 加权二倍角圆均值的切线方向（度，0~180）。
    field_angle_deg: f32,
    /// 直径路径点上 coherence 加权二倍角圆均值的切线方向（度，0~180）。
    diameter_angle_deg: f32,
    /// 清理后骨架分段数（最长段＋长度 ≥ 阈值的段）：仅诊断，不参与主干。
    path_count: usize,
    /// 掩膜包围盒（图像坐标，含端点）。
    bbox: [usize; 4],
    /// 最大连通域像素数。
    mask_pixels: usize,
}

/// 单质量块骨架提取：`rgba` 为 RGBA8，`mask` 为调用方给定的显式掩膜。
/// 返回 None 当且仅当掩膜为空、最大连通域为空或骨架退化（直径路径不足 2 点）——
/// 调用方须把 None 报成错误，绝不返回空几何冒充成功。
fn trace_region(
    rgba: &[u8],
    width: usize,
    height: usize,
    mask: &[bool],
    tensor_radius: usize,
    dp_eps_fine: f32,
    spur_prune_min_px: usize,
) -> Option<TraceOutput> {
    let component = largest_connected_component(mask, width, height);
    let bbox = mask_bbox(&component, width, height)?;
    let mask_pixels = component.iter().filter(|&&b| b).count();
    if mask_pixels == 0 {
        return None;
    }
    // 裁剪窗：包围盒外扩 tensor_radius + 2（见模块头注：与整图计算等价）。
    let pad = tensor_radius + 2;
    let cx0 = bbox[0].saturating_sub(pad);
    let cy0 = bbox[1].saturating_sub(pad);
    let cx1 = (bbox[2] + pad).min(width - 1);
    let cy1 = (bbox[3] + pad).min(height - 1);
    let cw = cx1 - cx0 + 1;
    let ch = cy1 - cy0 + 1;
    let mut crop_rgba = vec![0u8; cw * ch * 4];
    let mut crop_mask = vec![false; cw * ch];
    for y in 0..ch {
        for x in 0..cw {
            let gi = (cy0 + y) * width + (cx0 + x);
            let li = y * cw + x;
            crop_rgba[li * 4..li * 4 + 4].copy_from_slice(&rgba[gi * 4..gi * 4 + 4]);
            crop_mask[li] = component[gi];
        }
    }
    let dist = euclidean_distance_transform(&crop_mask, cw, ch);
    let skel = zhang_suen_thinning(&crop_mask, cw, ch);
    let trunk = extract_main_trunk_path(&skel, cw, ch);
    if trunk.len() < 2 {
        return None;
    }
    let field = compute_structure_tensor(&crop_rgba, cw, ch, tensor_radius);
    let trunk_radii: Vec<f32> = trunk.iter().map(|&(x, y)| dist[y * cw + x]).collect();
    let base_radius = trunk_radii.iter().fold(0.0f32, |a, &b| a.max(b));
    let dp_eps_coarse = (base_radius * 0.15).max(1.2);
    let pick = |idx: &[usize]| -> (Vec<(usize, usize)>, Vec<f32>) {
        (
            idx.iter().map(|&i| trunk[i]).collect(),
            idx.iter().map(|&i| trunk_radii[i]).collect(),
        )
    };
    let (fine_points, fine_radii) = pick(&rdp_simplify_indices(&trunk, dp_eps_fine));
    let (coarse_points, coarse_radii) = pick(&rdp_simplify_indices(&trunk, dp_eps_coarse));
    // 场角：掩膜像素上 coherence 加权圆均值；直径角：直径路径点上同法加权。
    let mut f_angles = Vec::new();
    let mut f_weights = Vec::new();
    for y in 0..ch {
        for x in 0..cw {
            if crop_mask[y * cw + x] {
                let i = y * cw + x;
                f_angles.push(field.tangent_angle[i]);
                f_weights.push(field.coherence[i]);
            }
        }
    }
    let field_angle_deg = weighted_mean_angle_deg(&f_angles, &f_weights);
    let mut d_angles = Vec::new();
    let mut d_weights = Vec::new();
    for &(x, y) in &trunk {
        let i = y * cw + x;
        d_angles.push(field.tangent_angle[i]);
        d_weights.push(field.coherence[i]);
    }
    let diameter_angle_deg = weighted_mean_angle_deg(&d_angles, &d_weights);
    // 诊断用分段数：最长段＋长度 ≥ 阈值的段（毛刺段不计）。
    let segments = order_skeleton(&skel, cw, ch);
    let mut seg_lens: Vec<usize> = segments.iter().map(|p| p.len()).collect();
    seg_lens.sort_unstable_by(|a, b| b.cmp(a));
    let path_count = seg_lens
        .iter()
        .enumerate()
        .filter(|(i, len)| *i == 0 || **len >= spur_prune_min_px)
        .count();
    // 输出坐标平移回图像坐标。
    let shift = |(x, y): (usize, usize)| (x + cx0, y + cy0);
    let diameter_points: Vec<(usize, usize)> = trunk.iter().map(|&p| shift(p)).collect();
    let fine_points: Vec<(usize, usize)> = fine_points.iter().map(|&p| shift(p)).collect();
    let coarse_points: Vec<(usize, usize)> = coarse_points.iter().map(|&p| shift(p)).collect();
    let bbox_global = [bbox[0], bbox[1], bbox[2], bbox[3]];
    Some(TraceOutput {
        diameter_points,
        diameter_radii: trunk_radii,
        fine_points,
        fine_radii,
        coarse_points,
        coarse_radii,
        base_radius,
        mean_color: mean_srgb(&crop_rgba, &crop_mask, cw, ch),
        field_angle_deg,
        diameter_angle_deg,
        path_count,
        bbox: bbox_global,
        mask_pixels,
    })
}

/// 参数提取助手：必填字符串，缺失或非字符串时报 `invalid_argument`（不静默缺省）。
fn req_string(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("缺少必填字符串参数 {key}")),
            )
        })
}

/// 可选整数：非整数或负数时报 `invalid_argument`（不静默截断）。
fn opt_usize(args: &Value, key: &str) -> Result<Option<usize>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) => Ok(Some(n as usize)),
            None => Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("参数 {key} 须为非负整数")),
            )),
        },
    }
}

/// 可选浮点：非数字时报 `invalid_argument`。
fn opt_f64(args: &Value, key: &str) -> Result<Option<f64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_f64() {
            Some(n) => Ok(Some(n)),
            None => Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("参数 {key} 须为数字")),
            )),
        },
    }
}

/// `reference_trace_region`：只读，见模块头注。
/// 不碰 `ctx.doc_id` 指向的文档（签名里没有文档寻址能力）；只读 `--root` 内的
/// 图像与掩膜文件，返回 JSON，不写盘、不产生原子。
pub(super) fn read_reference_trace_region(
    ctx: &mut ToolContext<'_>,
    args: &Value,
) -> Result<Value> {
    let image_path = req_string(args, "image_path")?;
    let mask_path = args
        .get("mask_path")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let polygon_raw = args.get("polygon");
    let polygon_given = polygon_raw.is_some_and(|p| !p.is_null());
    match (&mask_path, polygon_given) {
        (Some(_), true) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("mask_path 与 polygon 二选一：同时给两者时不知道用哪块掩膜"),
            ));
        }
        (None, false) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "mask_path 与 polygon 必须给一个：工具不做颜色分割，掩膜须由调用方显式给出",
                ),
            ));
        }
        _ => {}
    }
    let tensor_radius = opt_usize(args, "tensor_radius")?.unwrap_or(2);
    let dp_eps_fine = opt_f64(args, "dp_eps_fine")?.unwrap_or(2.0);
    if dp_eps_fine.is_nan() || dp_eps_fine <= 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("dp_eps_fine 须大于 0"),
        ));
    }
    let spur_prune_min_px = opt_usize(args, "spur_prune_min_px")?.unwrap_or(25);
    let cap = crate::file_source::max_upload_bytes();
    let root = ctx.workspace.root_path();
    let (image_bytes, _) = crate::file_source::resolve_local_path(&image_path, root, cap)?;
    let (iw, ih, image_rgba) = yanshi_render::png::decode_png(&image_bytes).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("image_path 不是可解码的 8 位非交错 RGB/RGBA PNG"),
        )
    })?;
    let (iw, ih) = (iw as usize, ih as usize);
    let mask = if let Some(mp) = &mask_path {
        let (mask_bytes, _) = crate::file_source::resolve_local_path(mp, root, cap)?;
        let (mw, mh, mask_rgba) = yanshi_render::png::decode_png(&mask_bytes).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("mask_path 不是可解码的 8 位非交错 RGB/RGBA PNG"),
            )
        })?;
        if mw as usize != iw || mh as usize != ih {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "mask_path 尺寸 {}×{} 与 image_path 尺寸 {}×{} 不一致",
                    mw, mh, iw, ih
                )),
            ));
        }
        mask_from_rgba(&mask_rgba, iw, ih)
    } else {
        let arr = polygon_raw.and_then(Value::as_array).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("polygon 须为 [[x, y], …] 数组"),
            )
        })?;
        let mut polygon: Vec<(f64, f64)> = Vec::new();
        for pt in arr {
            let pair = pt.as_array().ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("polygon 的每一点须为 [x, y]"),
                )
            })?;
            if pair.len() != 2 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("polygon 的每一点须恰为 [x, y] 两数"),
                ));
            }
            let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("polygon 的坐标须为数字"),
                ));
            };
            polygon.push((x, y));
        }
        if polygon.len() < 3 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("polygon 至少需要 3 个点"),
            ));
        }
        fill_polygon(&polygon, iw, ih)
    };
    let out = trace_region(
        &image_rgba,
        iw,
        ih,
        &mask,
        tensor_radius,
        dp_eps_fine as f32,
        spur_prune_min_px,
    )
    .ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "掩膜内没有可提取的骨架：掩膜为空、最大连通域为空或骨架退化（直径路径不足 2 点）",
            ),
        )
    })?;
    let pts = |ps: &[(usize, usize)]| -> Vec<Value> {
        ps.iter()
            .map(|&(x, y)| json!([x, y]))
            .collect::<Vec<Value>>()
    };
    let radii = |rs: &[f32]| -> Vec<Value> {
        rs.iter()
            .map(|&r| json!(((r * 100.0).round() / 100.0)))
            .collect::<Vec<Value>>()
    };
    let round1 = |v: f32| -> f64 { ((v * 10.0).round() / 10.0) as f64 };
    let round2 = |v: f32| -> f64 { ((v * 100.0).round() / 100.0) as f64 };
    Ok(json!({
        "ok": true,
        "image": {"path": image_path, "width": iw, "height": ih},
        "mask": {
            "source": if mask_path.is_some() { "mask_path" } else { "polygon" },
            "pixels": out.mask_pixels,
            "bbox": out.bbox,
        },
        "diameter_path": {"points": pts(&out.diameter_points), "radii_px": radii(&out.diameter_radii)},
        "centerline_fine": {
            "points": pts(&out.fine_points),
            "radii_px": radii(&out.fine_radii),
            "dp_eps": dp_eps_fine,
        },
        "centerline_coarse": {
            "points": pts(&out.coarse_points),
            "radii_px": radii(&out.coarse_radii),
            "dp_eps": round2(out.base_radius * 0.15),
            "note": "block-in only：粗简化抹掉小幅摆动，不可当精细中轴用",
        },
        "base_radius_px": round2(out.base_radius),
        "mean_srgb": out.mean_color,
        "field_angle_deg": round1(out.field_angle_deg),
        "diameter_angle_deg": round1(out.diameter_angle_deg),
        "path_count": out.path_count,
        "pipeline": {
            "tensor_radius": tensor_radius,
            "dp_eps_fine": dp_eps_fine,
            "dp_eps_coarse": round2(out.base_radius * 0.15),
            "spur_prune_min_px": spur_prune_min_px,
            "crop_pad": tensor_radius + 2,
        },
        "limits": [
            "逐点半径仅量级参考：分叉迂回处噪声大，不可当精确宽度执行",
            "中轴端点按半径内收（细化固有行为）",
            "宽平区域（如柏树顶带）骨架端点不稳定：掩膜形状的共有局限，不是回归",
            "掩膜触及图像边界时，边界处骨架可能被截断",
            "本工具不输出分割、接缝、压感映射：上述量未经验证或已证伪",
            "field_angle_deg 与 diameter_angle_deg 为测量统计，不作方向判定阈值",
        ],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造纯色 RGBA 图与矩形掩膜（掩膜内像素涂给定色，背景纯白）。
    fn rect_fixture(
        w: usize,
        h: usize,
        x0: usize,
        y0: usize,
        x1: usize,
        y1: usize,
        color: [u8; 3],
    ) -> (Vec<u8>, Vec<bool>) {
        let mut rgba = vec![255u8; w * h * 4];
        let mut mask = vec![false; w * h];
        for y in y0..=y1 {
            for x in x0..=x1 {
                let i = y * w + x;
                mask[i] = true;
                rgba[i * 4] = color[0];
                rgba[i * 4 + 1] = color[1];
                rgba[i * 4 + 2] = color[2];
                rgba[i * 4 + 3] = 255;
            }
        }
        (rgba, mask)
    }

    #[test]
    fn edt_matches_bruteforce_on_small_grid() {
        // 7×5 网格，掩膜为两个分离点与一条短线：逐点比对暴力解。
        let (w, h) = (7, 5);
        let mut mask = vec![false; w * h];
        mask[w + 1] = true;
        mask[w + 2] = true;
        mask[3 * w + 5] = true;
        let dist = euclidean_distance_transform(&mask, w, h);
        for y in 0..h {
            for x in 0..w {
                let mut best = f32::INFINITY;
                for by in 0..h {
                    for bx in 0..w {
                        if !mask[by * w + bx] {
                            let d = (((x as f32 - bx as f32).powi(2)
                                + (y as f32 - by as f32).powi(2))
                            .sqrt())
                            .min(best);
                            best = d;
                        }
                    }
                }
                let got = dist[y * w + x];
                assert!(
                    (got - best).abs() < 1e-3,
                    "EDT 在 ({x},{y}) 为 {got}，暴力解为 {best}"
                );
            }
        }
    }

    #[test]
    fn rect_bar_skeleton_is_midline_with_half_height_radius() {
        // 横条 100×20（x 10..=89，y 30..=49）：中轴应沿 y≈39.5，半径≈10。
        let (w, h) = (100, 80);
        let (rgba, mask) = rect_fixture(w, h, 10, 30, 89, 49, [40, 90, 160]);
        let out = trace_region(&rgba, w, h, &mask, 2, 2.0, 25).expect("横条应提得出骨架");
        assert!(!out.diameter_points.is_empty());
        let mid_y: f32 = out
            .diameter_points
            .iter()
            .map(|&(_, y)| y as f32)
            .sum::<f32>()
            / out.diameter_points.len() as f32;
        assert!(
            (mid_y - 39.5).abs() <= 1.5,
            "横条中轴 y 均值 {mid_y}，期望 39.5±1.5"
        );
        let max_r = out.diameter_radii.iter().fold(0.0f32, |a, &b| a.max(b));
        assert!(
            (max_r - 10.0).abs() <= 1.5,
            "横条最大半径 {max_r}，期望 10±1.5"
        );
        assert_eq!(out.mean_color, [40, 90, 160]);
        assert_eq!(out.bbox, [10, 30, 89, 49]);
    }

    #[test]
    fn disk_skeleton_degenerates_to_no_path() {
        // 圆盘（中心 50,50，半径 20）：细化后骨架退化为单点，直径路径不足 2 点，
        // 提取返回 None——与修订版代码行为一致（其注释称圆块用质心构造短笔触，
        // 代码实际直接 return None，见验证收据缺陷 2）。工具对此类质量块应报错，
        // 而不是编造一条中轴冒充成功。
        let (w, h) = (100, 100);
        let mut rgba = vec![255u8; w * h * 4];
        let mut mask = vec![false; w * h];
        for y in 0..h {
            for x in 0..w {
                let dx = x as f32 - 50.0;
                let dy = y as f32 - 50.0;
                if dx * dx + dy * dy <= 20.0 * 20.0 {
                    let i = y * w + x;
                    mask[i] = true;
                    rgba[i * 4] = 200;
                    rgba[i * 4 + 1] = 30;
                    rgba[i * 4 + 2] = 30;
                    rgba[i * 4 + 3] = 255;
                }
            }
        }
        assert!(
            trace_region(&rgba, w, h, &mask, 2, 2.0, 25).is_none(),
            "圆盘骨架退化时应返回 None（调用方报 err），而不是编造路径"
        );
    }

    #[test]
    fn tiered_rdp_fine_keeps_bend_coarse_drops_it() {
        // 折线路径：细层（eps 2.0）保留弯折点，粗层（eps = 最大半径·0.15，取大半径）
        // 只留首尾。直接测 RDP 本身，不依赖整管线。
        let pts: Vec<(usize, usize)> = vec![(0, 0), (50, 10), (100, 0)];
        let fine = rdp_simplify_indices(&pts, 2.0);
        assert_eq!(fine.len(), 3, "细层应保留弯折点");
        let coarse = rdp_simplify_indices(&pts, 24.5);
        assert_eq!(coarse.len(), 2, "粗层应只留首尾（block-in）");
    }

    #[test]
    fn polygon_fill_uses_pixel_centers() {
        // 10×10 正方形多边形（0,0)-(10,0)-(10,10)-(0,10)：像素中心规则下恰为 100 像素。
        let polygon = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let mask = fill_polygon(&polygon, 20, 20);
        assert_eq!(mask.iter().filter(|&&b| b).count(), 100);
        assert!(mask[5 * 20 + 5]);
        assert!(!mask[15 * 20 + 15]);
        assert!(fill_polygon(&[(0.0, 0.0), (5.0, 0.0)], 20, 20)
            .iter()
            .all(|&b| !b));
    }

    #[test]
    fn largest_component_drops_small_fragments() {
        // 大块 20×20 ＋ 碎块 2×2：只保留大块。
        let (w, h) = (40, 40);
        let mut mask = vec![false; w * h];
        for y in 5..25 {
            for x in 5..25 {
                mask[y * w + x] = true;
            }
        }
        mask[35 * w + 35] = true;
        mask[35 * w + 36] = true;
        mask[36 * w + 35] = true;
        mask[36 * w + 36] = true;
        let cc = largest_connected_component(&mask, w, h);
        assert_eq!(cc.iter().filter(|&&b| b).count(), 400);
        assert!(!cc[35 * w + 35]);
    }

    #[test]
    fn mask_from_rgba_rule() {
        // 白字透明底算入；纯黑算不入；全透明算不入；半透明（alpha 200）白字算入。
        let rgba: Vec<u8> = vec![
            255, 255, 255, 255, // 白不透明 → 入
            0, 0, 0, 255, // 纯黑不透明 → 不入
            255, 255, 255, 0, // 白全透明 → 不入
            200, 30, 30, 200, // 彩色半透明 → 入
        ];
        let mask = mask_from_rgba(&rgba, 4, 1);
        assert_eq!(mask, vec![true, false, false, true]);
    }

    #[test]
    fn registry_two_way_study_gating() {
        // 无 Study 时工具不出现；有 Study 时出现且 mutating=false（只读）。
        let core = ToolRegistry::with_profiles(&[Profile::Core]);
        assert!(
            core.get("reference_trace_region").is_none(),
            "Core 组成里不应出现 reference_trace_region"
        );
        let study = ToolRegistry::with_profiles(&[Profile::Study]);
        let spec = study
            .get("reference_trace_region")
            .expect("Study 组成里应出现 reference_trace_region");
        assert!(!spec.mutating, "reference_trace_region 必须是只读工具");
        let all = ToolRegistry::with_profiles(&Profile::ALL);
        assert!(
            all.get("reference_trace_region").is_some(),
            "ALL 全集里应出现 reference_trace_region"
        );
    }

    /// 最小 PGM（P5）解析：仅测试用。注释行以 `#` 起首，逐一跳过。
    fn parse_pgm_p5(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
        let mut pos = 0usize;
        let next_token = |pos: &mut usize| -> Option<String> {
            while *pos < bytes.len() {
                if bytes[*pos] == b'#' {
                    while *pos < bytes.len() && bytes[*pos] != b'\n' {
                        *pos += 1;
                    }
                } else if (bytes[*pos] as char).is_whitespace() {
                    *pos += 1;
                } else {
                    break;
                }
            }
            let start = *pos;
            while *pos < bytes.len() && !(bytes[*pos] as char).is_whitespace() {
                *pos += 1;
            }
            if start == *pos {
                None
            } else {
                Some(String::from_utf8_lossy(&bytes[start..*pos]).into_owned())
            }
        };
        if next_token(&mut pos).as_deref() != Some("P5") {
            return None;
        }
        let w: usize = next_token(&mut pos)?.parse().ok()?;
        let h: usize = next_token(&mut pos)?.parse().ok()?;
        let maxval: usize = next_token(&mut pos)?.parse().ok()?;
        if maxval != 255 {
            return None;
        }
        pos += 1; // maxval 后恰一个空白分隔符
        let pixels = bytes.get(pos..pos + w * h)?.to_vec();
        Some((w, h, pixels))
    }

    #[test]
    fn trunk_regression_against_validated_revision() {
        // 柏树干段回归：与专家修订版验证同一输入（600×1000 裁剪图＋掩膜）。
        // 夹具不在仓库内（分析产物，不分发）：环境变量指向含 trunk_crop.png、
        // trunk_mask.pgm 的目录；未设或文件缺失时跳过（CI 不跑此项，本地实测为准）。
        let dir = match std::env::var("YANSHI_STUDY_TRUNK_DIR") {
            Ok(d) => d,
            Err(_) => {
                eprintln!("跳过 trunk 回归：未设 YANSHI_STUDY_TRUNK_DIR");
                return;
            }
        };
        let crop_path = format!("{dir}/trunk_crop.png");
        let mask_path = format!("{dir}/trunk_mask.pgm");
        let crop_bytes = match std::fs::read(&crop_path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("跳过 trunk 回归：读不到 {crop_path}");
                return;
            }
        };
        let mask_bytes = match std::fs::read(&mask_path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("跳过 trunk 回归：读不到 {mask_path}");
                return;
            }
        };
        let (cw, ch, crop_rgba) =
            yanshi_render::png::decode_png(&crop_bytes).expect("trunk_crop.png 应可解码");
        let (mw, mh, mask_gray) = parse_pgm_p5(&mask_bytes).expect("trunk_mask.pgm 应为 P5");
        assert_eq!((mw, mh), (cw as usize, ch as usize));
        let mask: Vec<bool> = mask_gray.iter().map(|&v| v > 0).collect();
        let out = trace_region(&crop_rgba, cw as usize, ch as usize, &mask, 2, 2.0, 25)
            .expect("柏树干段应提得出骨架");
        eprintln!(
            "trunk 实测：base_radius={:.2} mean_srgb={:?} 直径路径={}点 field_angle={:.1}° diameter_angle={:.1}°",
            out.base_radius,
            out.mean_color,
            out.diameter_points.len(),
            out.field_angle_deg,
            out.diameter_angle_deg
        );
        // 验证口径（修订版验证收据）：base_radius 落在 159–163 区间，mean_srgb 逐值相同。
        assert!(
            (159.0..=163.0).contains(&out.base_radius),
            "base_radius {}，期望落在 159–163",
            out.base_radius
        );
        assert_eq!(out.mean_color, [119, 106, 54]);
        // 直径路径规模与修订版原始主干（1158 点）同量级；顶带分叉不在此断言内。
        assert!(
            (1000..=1300).contains(&out.diameter_points.len()),
            "直径路径 {} 点，期望 1000–1300（修订版 1158 点）",
            out.diameter_points.len()
        );
        // 若同目录有修订版原始主干 JSON，逐点比对最近距离（移植逐位一致时应为 0）。
        let rev_path = format!("{dir}/rev_trunk_rawpath.json");
        if let Ok(rev_bytes) = std::fs::read(&rev_path) {
            let rev: Value =
                serde_json::from_slice(&rev_bytes).expect("rev_trunk_rawpath.json 应可解析");
            let rev_pts: Vec<(f64, f64)> = rev
                .as_array()
                .expect("原始主干应为数组")
                .iter()
                .filter_map(|p| {
                    let a = p.as_array()?;
                    Some((a[0].as_f64()?, a[1].as_f64()?))
                })
                .collect();
            assert!(!rev_pts.is_empty());
            let mut max_nearest = 0.0f64;
            for &(x, y) in &out.diameter_points {
                let mut best = f64::INFINITY;
                for &(rx, ry) in &rev_pts {
                    let d = ((x as f64 - rx).powi(2) + (y as f64 - ry).powi(2)).sqrt();
                    if d < best {
                        best = d;
                    }
                }
                if best > max_nearest {
                    max_nearest = best;
                }
            }
            eprintln!("trunk 回归：与修订版主干的最大最近距离 {max_nearest:.3}px");
            assert!(
                max_nearest <= 1.0,
                "直径路径偏离修订版主干 {max_nearest:.3}px，超过 1px"
            );
        }
    }
}
