//! **智能笔触工具**（9 个 `smart_*`）——**本仓最大的一族**（约 5000 行）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **★ 可见性**：`tools.rs` 的 `#[cfg(test)]` 里有 **12 个测试**直接引用本模块的内部
//! helper（`get_texture_profile`／`texture_target_strokes`／`TextureXorShift`／
//! `GuideTable`／`wand_*`／`fold_*`／`rgb_to_hsl` 等）⇒ 所以本模块的顶层项、
//! impl 方法与 struct 字段都是 `pub(super)` ✓
//! （用**括号深度**判定：depth 0 = 顶层项、depth 1 = 方法/字段 ⇒ 不碰函数体）
//!
//! **边界**：起点 `smart_fill` 文档，终点 `selection_affine` 文档之前（后者留 `tools.rs`）。

use super::*;

/// **P2-6（期 2 块 4）**：`smart_*` 前置图层校验 —— fail-fast。
///
/// **为什么必须在工具层先查** ✓：折叠是**容错**的 ⇒ 图层不存在时，`write_brush_stroke`
///   只在**落笔提交**时才报 `ReferenceNotFound` ✗ ⇒ 一个 `smart_hatch_follow` 要先算完
///   1259 笔（`raster_ms=41586`）才报错 ⇒ **50s 空烧** ✗（probe2 实测）。
///   在这里先查 ⇒ 不存在的 `layer_id` **秒级**报错 ✓，一笔都不算 ✓。
/// **错误码与落笔时一致** ✓（`ReferenceNotFound`）⇒ 调用方按同一码处理 ✓；
///   文案附现有图层（照 `write_delete_anchor` 的写法 ✓），免得调用方再查一遍 ✓。
/// **位置** ✓：各 `smart_*` 的**第一行**（在选区缓存预热与任何计算之前 ✓）。
fn require_smart_layer_alive(ctx: &mut ToolContext<'_>, layer_id: &str) -> Result<()> {
    let (alive, existing) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let state = document.state();
        (
            state.layer_alive(layer_id),
            state
                .alive_layers()
                .iter()
                .map(|layer| layer.id.clone())
                .collect::<Vec<_>>(),
        )
    };
    if alive {
        return Ok(());
    }
    Err(YanshiError::new(
        ErrorCode::ReferenceNotFound,
        ErrorContext::detail(format!(
            "图层 {layer_id} 不存在（smart_* 落笔前校验）⇒ 现有图层：{}",
            if existing.is_empty() {
                "（一个都没有）".to_string()
            } else {
                existing.join(", ")
            }
        )),
    ))
}

/// **P2-6**：内层笔触缺省不等渲染 ✓ —— `preview:false` 只省 PNG 编码，不省每笔的
///   脏区渲染（`finish_mutation` 里 `preview:false` 的 else 支仍跑
///   `render_region_complete_jobs`，且记进 `preview_ms` ✓）；tex_fold 66 笔
///   `preview_ms=180021`（占墙钟 94%）正是这段 ✓ —— 定位清单写「内层不关预览」是
///   旧假设：`preview:false` 自 2026-10-22 起已在 7 处内层调用里，probe2 的 180s
///   仍发生在它之后 ⇒ 靶点是每笔等渲染，不是编码 ✓。
///   缺省 `false` ⇒ 内层只落原子、不等渲染 ✓；调用方显式传 `wait_for_render:true` 可恢复 ✓。
///   （`ctx` 随本次调用丢弃，不必还原 ✓。）
fn set_inner_wait_for_render(ctx: &mut ToolContext<'_>, args: &Value) {
    ctx.set_wait_for_render(
        args.get("wait_for_render")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );
}

/// 三种模式：
/// - `hatch`：单向平行排线（素描调子、水彩平涂）
/// - `cross_hatch`：交叉排线（加深调子，两遍垂直）
/// - `contour`：沿形状轮廓向内收缩画多圈（仅 rect/circle)
///
/// `smart_fill` 是一笔一笔用真实笔刷画出来的排线，每条线带方向抖动 +
/// `hand_tremor` + `pressure_shape`，像真人排线。
///
/// 三种模式：
/// - `hatch`：单向平行排线（素描调子、水彩平涂）
/// - `cross_hatch`：交叉排线（35° 菱形交织，非 90°）
/// - `contour`：沿形状轮廓向内收缩画多圈（仅 rect/circle）
///
/// 专家级修正（2026-10-19）：
/// 1. 过冲 + 软裁剪：排线向外延伸 1.5×radius，避免 pressure_shape 导致的边缘漏白
/// 2. 相关角度游走：delta_theta[i] = 0.7*delta[i-1] + 0.3*rand，避免独立随机导致的裂口
/// 3. 往复运笔：偶数笔左→右，奇数笔右→左，平衡墨色
/// 4. 长线分块：>18×size 时切分，重叠 1.5×size
/// 5. 生理微弧：0.015×L 的微拱，消除直尺感
/// 6. 相位偏移：每笔 color_breath 相位 += i×0.618034，防同步条纹
pub(super) fn write_smart_fill(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— 在选区预热之前 ✓。
    let layer_id = require_str(args, "layer_id")?;
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    // Q4：`clip_to_selection` 透传 + 缓存预热（失败时忽略，走慢路径）。
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();
    if !clip_sel.is_empty() {
        let _ = precompute_clip_mask(ctx, &clip_sel);
    }

    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(15.0);
    let fill_direction_deg = args
        .get("fill_direction")
        .and_then(Value::as_f64)
        .unwrap_or(-30.0); // 人类自然角度，非 0°
    let direction_jitter = args
        .get("direction_jitter")
        .and_then(Value::as_f64)
        .unwrap_or(8.0);
    let mode = optional_str(args, "mode").unwrap_or_else(|| "hatch".to_owned());
    let color = args
        .get("color")
        .cloned()
        .unwrap_or(json!([0.0, 0.0, 0.0, 1.0]));
    let clip_to_shape = args
        .get("clip_to_shape")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    // --- 解析形状（含 holes，见 parse_smart_shape）---
    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_fill 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_fill")?;
    // Q4 早退：形状 bbox 与 clip 掩膜无交集时直接返回（不生成任何笔触，不提交原子）。
    if clip_bbox_early_out(
        ctx,
        &clip_sel,
        (
            parsed.bbox.0,
            parsed.bbox.1,
            parsed.bbox.2 - parsed.bbox.0,
            parsed.bbox.3 - parsed.bbox.1,
        ),
    ) {
        return Ok(json!({
            "ok": true,
            "clipped_out": true,
            "note": "形状包围盒与 clip_to_selection 无交集，未生成笔触",
        }));
    }
    let shape_type = parsed.outer.kind_str();
    // 形状顶点（用于 polygon 求交；非多边形为空）
    let poly_pts: Vec<(f64, f64)> = parsed.outer.polygon_points();
    // bbox: (x0, y0, x1, y1)
    let (bx0, by0, bx1, by1): (f64, f64, f64, f64) = parsed.bbox;

    // 点是否在形状内（含孔洞扣除）。排线采样用它做游程切分，
    // 穿过孔洞的线会在孔边缘自动断开成独立笔触。
    let point_in_shape = |px: f64, py: f64| -> bool { parsed.contains(px, py) };

    // 射线与形状求交：返回进入/穿出点的 t 参数列表（已排序）
    // 射线：P = origin + t * dir
    let _ray_shape_intersect = |ox: f64, oy: f64, dx: f64, dy: f64| -> Vec<f64> {
        let mut ts: Vec<f64> = Vec::new();
        match shape_type {
            "rect" => {
                // 与 4 条边求交
                let edges = [
                    ((bx0, by0), (bx1, by0)), // top
                    ((bx1, by0), (bx1, by1)), // right
                    ((bx1, by1), (bx0, by1)), // bottom
                    ((bx0, by1), (bx0, by0)), // left
                ];
                for ((x1, y1), (x2, y2)) in edges {
                    let denom = dx * (y2 - y1) - dy * (x2 - x1);
                    if denom.abs() < 1e-10 {
                        continue;
                    }
                    let _t = ((x1 - ox) * (y2 - y1) - (y1 - oy) * (x2 - x1)) / denom;
                    let _u = ((x1 - ox) * dy - (y1 - oy) * dx) / denom;
                    // 注意符号：这里用标准线段交公式
                    let t2 = ((ox - x1) * (y1 - y2) - (oy - y1) * (x1 - x2))
                        / (dx * (y1 - y2) - dy * (x1 - x2));
                    if t2.abs() < 1e-10 {
                        continue;
                    }
                    let tt = ((x1 - ox) * (y1 - y2) - (y1 - oy) * (x1 - x2))
                        / (dx * (y1 - y2) - dy * (x1 - x2));
                    let uu =
                        -((dx * (y1 - oy) - dy * (x1 - ox)) / (dx * (y1 - y2) - dy * (x1 - x2)));
                    if (-1e-9..=1.0 + 1e-9).contains(&uu) {
                        ts.push(tt);
                    }
                }
            }
            "circle" => {
                let cx = (bx0 + bx1) / 2.0;
                let cy = (by0 + by1) / 2.0;
                let r = (bx1 - bx0) / 2.0;
                let ocx = ox - cx;
                let ocy = oy - cy;
                let a = dx * dx + dy * dy;
                let b = 2.0 * (ocx * dx + ocy * dy);
                let c = ocx * ocx + ocy * ocy - r * r;
                let disc = b * b - 4.0 * a * c;
                if disc >= 0.0 {
                    let sq = disc.sqrt();
                    ts.push((-b - sq) / (2.0 * a));
                    ts.push((-b + sq) / (2.0 * a));
                }
            }
            "polygon" if poly_pts.len() >= 3 => {
                let n = poly_pts.len();
                for i in 0..n {
                    let (x1, y1) = poly_pts[i];
                    let (x2, y2) = poly_pts[(i + 1) % n];
                    let denom = dx * (y1 - y2) - dy * (x1 - x2);
                    if denom.abs() < 1e-10 {
                        continue;
                    }
                    let tt = ((x1 - ox) * (y1 - y2) - (y1 - oy) * (x1 - x2)) / denom;
                    let uu = -((dx * (y1 - oy) - dy * (x1 - ox)) / denom);
                    if (-1e-9..=1.0 + 1e-9).contains(&uu) {
                        ts.push(tt);
                    }
                }
            }
            _ => {}
        }
        ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // 去重（相近的 t 合并）
        let mut dedup: Vec<f64> = Vec::new();
        for t in ts {
            if dedup
                .last()
                .is_none_or(|last: &f64| (t - *last).abs() > 1e-6)
            {
                dedup.push(t);
            }
        }
        dedup
    };

    // --- PRNG ---
    struct XorShift64(u64);
    impl XorShift64 {
        fn next_f64(&mut self) -> f64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            ((x >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next_f64()
        }
        // 近似高斯（Box-Muller 简化版：3 个 uniform 平均）
        fn gaussian(&mut self) -> f64 {
            (self.next_f64() + self.next_f64() + self.next_f64()) / 3.0 * 2.0 - 1.0
        }
    }
    let seed = ((bx0 * 73856093.0 + by0 * 19349663.0 + bx1 * 83492791.0) as u64)
        .wrapping_add(0x9E3779B97F4A7C15);
    let mut rng = XorShift64(seed.max(1));

    // --- 默认参数配方 ---
    let (def_tremor, def_breath, def_pshape, def_taper, def_push) = match mode.as_str() {
        "cross_hatch" => (0.05, 0.06, 0.35, 0.5, 0.05),
        "contour" => (0.04, 0.10, 0.20, 0.45, 0.15),
        _ => (0.045, 0.08, 0.25, 0.20, 0.10), // hatch
    };
    // 调用方显式传的优先
    let use_tremor = args
        .get("hand_tremor")
        .and_then(Value::as_f64)
        .unwrap_or(def_tremor);
    let use_breath = args
        .get("color_breath")
        .and_then(Value::as_f64)
        .unwrap_or(def_breath);
    let use_pshape = args
        .get("pressure_shape")
        .and_then(Value::as_f64)
        .unwrap_or(def_pshape);
    let use_taper = args
        .get("stroke_taper")
        .and_then(Value::as_f64)
        .unwrap_or(def_taper);
    let use_push = args
        .get("paint_push")
        .and_then(Value::as_f64)
        .unwrap_or(def_push);

    // --- 方向场（③）：direction_guides / direction_from；缺省为 fill_direction ---
    let (dir_field, _) = parse_direction_field(args, &parsed, fill_direction_deg);

    // --- 生成排线 ---
    // 每条线：Vec<(x, y)> 点列 + 相位偏移
    struct PlannedLine {
        points: Vec<(f64, f64)>,
        phase_offset: f32,
        opacity_scale: f32,
    }
    let mut planned: Vec<PlannedLine> = Vec::new();
    let radius = size / 2.0;
    // 过冲距离：1.5 × radius（clip_to_shape=false 时改为向内收缩 0.3×radius）
    let overshoot = if clip_to_shape {
        1.5 * radius
    } else {
        -0.3 * radius
    };

    // 生成一个方向的排线（核心算法）
    // 用采样法 + 过冲：沿线采样点，保留形状内的连续段
    // 过冲保证 pressure_shape 变细的两端落在形状外，边界无漏白
    // angle_offset：叠在方向场之上的附加偏角（cross_hatch 第二遍 +35° 用）
    let gen_hatch = |angle_offset: f64,
                     rng: &mut XorShift64,
                     out: &mut Vec<PlannedLine>,
                     opacity_scale: f32,
                     base_phase: f32| {
        let angle_rad = fill_direction_deg.to_radians();
        let dx = angle_rad.cos();
        let dy = angle_rad.sin();
        let nx = -dy;
        let ny = dx;

        let cx = (bx0 + bx1) / 2.0;
        let cy = (by0 + by1) / 2.0;
        let half_diag = ((bx1 - bx0).powi(2) + (by1 - by0).powi(2)).sqrt() / 2.0 + overshoot + size;

        // 相关角度游走（非独立随机！）
        let mut delta_theta = 0.0;
        let base_spacing = size * 0.7;
        let mut offset = -half_diag;
        let mut line_idx = 0usize;

        while offset <= half_diag {
            // 角度游走：0.7 继承 + 0.3 新随机（叠在方向场局部角上）
            let jitter_now = rng.range(-direction_jitter, direction_jitter);
            delta_theta = 0.7 * delta_theta + 0.3 * jitter_now;
            // 方向场：该线中心处的局部走向（Constant 时恒等于 fill_direction_deg）。
            let field_ang = dir_field.angle_at(cx + nx * offset, cy + ny * offset) + angle_offset;
            let la = (field_ang + delta_theta).to_radians();
            let ldx = la.cos();
            let ldy = la.sin();

            // 间距抖动 ±12%
            let spacing_jitter = 1.0 + rng.gaussian() * 0.12;
            let spacing = base_spacing * spacing_jitter;

            // 线的中心
            let line_cx = cx + nx * offset;
            let line_cy = cy + ny * offset;

            // 沿线采样，步长 size/2，收集形状内的连续段
            // 采样范围超出形状（过冲），保证边界覆盖
            let step = (size / 2.0).max(2.0);
            let sample_extent = half_diag + overshoot;
            let mut current_seg: Vec<(f64, f64)> = Vec::new();
            let mut t = -sample_extent;
            while t <= sample_extent {
                let px = line_cx + ldx * t;
                let py = line_cy + ldy * t;
                if point_in_shape(px, py) {
                    current_seg.push((px, py));
                } else if !current_seg.is_empty() {
                    // 离开形状：保存当前段（加过冲延伸）
                    if current_seg.len() >= 2 {
                        // 向两端各延伸 overshoot（用方向向量）
                        let (sx, sy) = current_seg[0];
                        let (ex, ey) = current_seg[current_seg.len() - 1];
                        let mut ext_seg = Vec::with_capacity(current_seg.len() + 4);
                        // 起点向外延伸
                        ext_seg.push((sx - ldx * overshoot, sy - ldy * overshoot));
                        ext_seg.extend(current_seg.iter().cloned());
                        // 终点向外延伸
                        ext_seg.push((ex + ldx * overshoot, ey + ldy * overshoot));
                        // 后处理：微弧度 + 往复运笔
                        let n = ext_seg.len();
                        let seg_len_est = {
                            let (x0, y0) = ext_seg[0];
                            let (x1, y1) = ext_seg[n - 1];
                            ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt()
                        };
                        let mut final_pts: Vec<(f64, f64)> = Vec::with_capacity(n);
                        for (pi, (px, py)) in ext_seg.iter().enumerate() {
                            let u = pi as f64 / (n - 1).max(1) as f64;
                            // 生理微弧：0.015 × L
                            let camber = (std::f64::consts::PI * u).sin() * 0.015 * seg_len_est;
                            final_pts.push((px - ldy * camber, py + ldx * camber));
                        }
                        // 往复运笔：奇数笔反向
                        if out.len() % 2 == 1 {
                            final_pts.reverse();
                        }
                        out.push(PlannedLine {
                            points: final_pts,
                            phase_offset: base_phase + (out.len() as f32) * 0.618034,
                            opacity_scale,
                        });
                    }
                    current_seg.clear();
                }
                t += step;
            }
            // 收尾
            if current_seg.len() >= 2 {
                let (sx, sy) = current_seg[0];
                let (ex, ey) = current_seg[current_seg.len() - 1];
                let mut ext_seg = Vec::with_capacity(current_seg.len() + 4);
                ext_seg.push((sx - ldx * overshoot, sy - ldy * overshoot));
                ext_seg.extend(current_seg.iter().cloned());
                ext_seg.push((ex + ldx * overshoot, ey + ldy * overshoot));
                // 后处理：微弧度 + 往复运笔
                let n = ext_seg.len();
                let seg_len_est = {
                    let (x0, y0) = ext_seg[0];
                    let (x1, y1) = ext_seg[n - 1];
                    ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt()
                };
                let mut final_pts: Vec<(f64, f64)> = Vec::with_capacity(n);
                for (pi, (px, py)) in ext_seg.iter().enumerate() {
                    let u = pi as f64 / (n - 1).max(1) as f64;
                    let camber = (std::f64::consts::PI * u).sin() * 0.015 * seg_len_est;
                    final_pts.push((px - ldy * camber, py + ldx * camber));
                }
                if out.len() % 2 == 1 {
                    final_pts.reverse();
                }
                out.push(PlannedLine {
                    points: final_pts,
                    phase_offset: base_phase + (out.len() as f32) * 0.618034,
                    opacity_scale,
                });
            }

            line_idx += 1;
            offset += spacing;
        }
        let _ = line_idx;
    };

    match mode.as_str() {
        "hatch" => {
            gen_hatch(0.0, &mut rng, &mut planned, 1.0, 0.0);
        }
        "cross_hatch" => {
            // Pass 1: 主方向，65% 铺色
            gen_hatch(0.0, &mut rng, &mut planned, 1.0, 0.0);
            // Pass 2: +35°（非 90°！），75% 透明度
            gen_hatch(35.0, &mut rng, &mut planned, 0.75, 0.309);
        }
        "contour" => {
            // 简化版：rect/circle 向内收缩多圈
            match shape_type {
                "rect" => {
                    let mut inset = 0.0;
                    let _max_inset = (bx1 - bx0).min(bx1 - bx0).min(by1 - by0) / 2.0;
                    // 修正：用宽高中最小值的一半
                    let max_in = (bx1 - bx0).min(by1 - by0) / 2.0;
                    let _ring_idx = 0usize;
                    while inset < max_in {
                        let x0 = bx0 + inset;
                        let y0 = by0 + inset;
                        let x1 = bx1 - inset;
                        let y1 = by1 - inset;
                        if x1 - x0 < size || y1 - y0 < size {
                            break;
                        }
                        // 矩形一圈，断开为 2 段（避免闭合接缝）
                        let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)];
                        // 分成两段
                        let mid = corners.len() / 2;
                        for seg in [&corners[0..=mid], &corners[mid..]] {
                            if seg.len() >= 2 {
                                let pts: Vec<(f64, f64)> = seg.to_vec();
                                planned.push(PlannedLine {
                                    points: pts,
                                    phase_offset: (planned.len() as f32) * 0.618034,
                                    opacity_scale: 1.0,
                                });
                            }
                        }
                        inset += size * 0.68;
                    }
                }
                "circle" => {
                    let ccx = (bx0 + bx1) / 2.0;
                    let ccy = (by0 + by1) / 2.0;
                    let mut r = (bx1 - bx0) / 2.0;
                    while r > size / 2.0 {
                        let _n = 32;
                        // 断开为 2 段弧
                        for seg in 0..2 {
                            let a0 = seg as f64 * std::f64::consts::PI;
                            let a1 = (seg + 1) as f64 * std::f64::consts::PI + 0.1; // 重叠
                            let m = 16;
                            let pts: Vec<(f64, f64)> = (0..=m)
                                .map(|i| {
                                    let a = a0 + (a1 - a0) * (i as f64) / (m as f64);
                                    (ccx + r * a.cos(), ccy + r * a.sin())
                                })
                                .collect();
                            planned.push(PlannedLine {
                                points: pts,
                                phase_offset: (planned.len() as f32) * 0.618034,
                                opacity_scale: 1.0,
                            });
                        }
                        r -= size * 0.68;
                    }
                }
                _ => {
                    gen_hatch(fill_direction_deg, &mut rng, &mut planned, 1.0, 0.0);
                }
            }
        }
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "smart_fill 不支持的 mode：{mode}（支持 hatch/cross_hatch/contour）"
                )),
            ))
        }
    }

    if planned.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "smart_fill 没有生成任何笔触：形状可能太小或完全在画布外".to_owned(),
            ),
        ));
    }

    // --- 逐条调用 brush_stroke ---
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;

    for (li, line) in planned.iter().enumerate() {
        if line.points.len() < 2 {
            continue;
        }
        // 点格式：[x, y, pressure, time_ms]
        // time_ms 自动生成：起笔慢 → 中段快 → 收笔快
        let n = line.points.len();
        let total_ms = 60.0 + (n as f64) * 8.0; // 基础时间
        let points: Vec<Value> = line
            .points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                let u = i as f64 / (n - 1).max(1) as f64;
                // 时间曲线：ease-in-out
                let t_ms = total_ms * (u * u * (3.0 - 2.0 * u));
                // 颜色 alpha 乘 opacity_scale（cross_hatch 第二层）
                let col = match &color {
                    Value::Array(arr) if arr.len() >= 4 => {
                        let a = arr[3].as_f64().unwrap_or(1.0) * line.opacity_scale as f64;
                        json!([arr[0], arr[1], arr[2], a])
                    }
                    _ => color.clone(),
                };
                let _ = col;
                json!([x, y, 0.8, t_ms])
            })
            .collect();

        // 相位偏移通过 color_breath 的 seed 实现
        // 注意：当前 brush_stroke 不支持直接传 phase，这里用 opacity_scale 已处理
        // phase_offset 留待引擎层支持（当前仅记录）
        let _phase = line.phase_offset;
        let _li = li;

        let stroke_args = json!({
            "layer_id": layer_id,
            "brush": brush_name,
            "points": points,
            "size": size,
            "color": color,
            "hand_tremor": use_tremor,
            "color_breath": use_breath,
            "pressure_shape": use_pshape,
            "stroke_taper": use_taper,
            "paint_push": use_push,
            "preview": false,
            "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
        });

        match write_brush_stroke(ctx, &stroke_args) {
            Ok(_) => ok_count += 1,
            Err(e) => {
                fail_count += 1;
                if first_error.is_none() {
                    first_error = Some(format!("{e:?}"));
                }
            }
        }
    }

    let mut value = json!({
        "ok": fail_count == 0,
        "strokes": ok_count,
        "failed": fail_count,
        "mode": mode,
        "bbox": [bx0, by0, bx1, by1],
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_fill 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }
    Ok(value)
}

/// `smart_blend`：两色之间的自然混色过渡（Two-Pass Wet Blend）。
///
/// 解决三大问题：
/// 1. sRGB 直接 lerp 发灰发脏 → OKLab 感知均匀插值（立方根 LMS 空间）
/// 2. 排线条带（Banding）→ 斜切排线（+72°）+ 黄金分割抖动 + 子笔触分段
/// 3. 上游颜色绑架 → Pass 1 低 paint_push（0.08）+ Pass 2 湿笔揉边（0.38）
///
/// 专家修正（2026-10-19）：
/// - 排线方向 = blend_direction + 72°（斜切），不是沿渐变方向！
/// - 不用纯 smoothstep(t)（中段导数 1.5 加重条带），用 80% linear + 20% smoothstep
/// - time_ms 用 u + 0.15*sin(2πu)（两头慢中段快，且单调；0.19 会导致中段导数为负！）
pub(super) fn write_smart_blend(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— 在选区预热之前 ✓。
    let layer_id = require_str(args, "layer_id")?;
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    // Q4：`clip_to_selection` 透传 + 缓存预热（失败时忽略，走慢路径）。
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();
    if !clip_sel.is_empty() {
        let _ = precompute_clip_mask(ctx, &clip_sel);
    }

    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(15.0);
    let blend_direction_opt = args.get("blend_direction").and_then(Value::as_f64); // 缺省 0 = 从左到右（解析形状后，若传了 guides 则取其平均角）

    // --- 解析颜色 ---
    let parse_color = |v: Option<&Value>| -> [f32; 4] {
        match v {
            Some(Value::Array(arr)) => [
                arr.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
                arr.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
                arr.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
                arr.get(3).and_then(Value::as_f64).unwrap_or(1.0) as f32,
            ],
            _ => [0.0, 0.0, 0.0, 1.0],
        }
    };
    let color_a = parse_color(args.get("color_a"));
    let color_b = parse_color(args.get("color_b"));
    if args.get("color_a").is_none() || args.get("color_b").is_none() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_blend 缺少必填参数 color_a / color_b".to_owned()),
        ));
    }

    // --- OKLab 色彩插值 ---
    // **★ Phase 1（色彩体系规划）：删掉这里的两个本地 sRGB 传递函数** ✓
    //   **为什么**：它们是 `yanshi_render::color` 同名函数的**重复实现** ✗
    //     ⇒ 而两份实现**已经不同**：公共版 `srgb_to_linear` 有 `clamp(0,1)`，本地版没有
    //     ⇒ 于是「越界输入」在两个调用点会给出**不同**结果 ⇒ 正是规划说的「散落逻辑」
    //   **修法**：直接用公共的（`yanshi_render::color::{srgb_to_linear, linear_to_srgb}`）
    //     ⇒ 一处定义、全仓一致 ✓
    //   **行为变化**：越界输入现在会被 clamp（这是更正确的语义）；
    //     规划「存量 atoms 接受重放颜色变化」（用户已拍板）⇒ 可接受 ✓
    use yanshi_render::color::{linear_to_srgb, srgb_to_linear};
    /// 在 OKLab 感知均匀空间插值（利用数学性质：在立方根 LMS 空间线性插值
    /// 与转到 (L,a,b) 再插值恒等，省两次 3x3 矩阵乘法）
    pub(super) fn lerp_oklab(rgba_a: [f32; 4], rgba_b: [f32; 4], t: f32) -> [f32; 4] {
        let t = t.clamp(0.0, 1.0);
        let to_lms = |rgb: [f32; 4]| -> [f32; 3] {
            let r = srgb_to_linear(rgb[0]);
            let g = srgb_to_linear(rgb[1]);
            let b = srgb_to_linear(rgb[2]);
            let l = (0.41222147 * r + 0.53633255 * g + 0.05144599 * b)
                .max(0.0)
                .cbrt();
            let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.10739696 * b)
                .max(0.0)
                .cbrt();
            let s = (0.08830246 * r + 0.28171884 * g + 0.629_978_7 * b)
                .max(0.0)
                .cbrt();
            [l, m, s]
        };
        let lms_a = to_lms(rgba_a);
        let lms_b = to_lms(rgba_b);
        let l = lms_a[0] + (lms_b[0] - lms_a[0]) * t;
        let m = lms_a[1] + (lms_b[1] - lms_a[1]) * t;
        let s = lms_a[2] + (lms_b[2] - lms_a[2]) * t;
        let l3 = l * l * l;
        let m3 = m * m * m;
        let s3 = s * s * s;
        let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.23096993 * s3;
        let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3;
        let b = -0.00419609 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
        let a = rgba_a[3] + (rgba_b[3] - rgba_a[3]) * t;
        [
            linear_to_srgb(r),
            linear_to_srgb(g),
            linear_to_srgb(b),
            a.clamp(0.0, 1.0),
        ]
    }

    /// 黄金分割准随机三角波抖动 [-1, 1]，打碎相邻排线的色阶断层
    #[inline]
    pub(super) fn golden_dither(stroke_idx: usize) -> f32 {
        let frac = ((stroke_idx as f32) * 0.618_034).fract();
        if frac < 0.5 {
            frac * 4.0 - 1.0
        } else {
            3.0 - frac * 4.0
        }
    }

    /// 点 (x,y) 在渐变方向上的归一化位置 t in [0,1]
    #[inline]
    pub(super) fn project_blend_t(
        x: f64,
        y: f64,
        cos_d: f64,
        sin_d: f64,
        pmin: f64,
        pmax: f64,
    ) -> f32 {
        let span = (pmax - pmin).max(1.0);
        (((x * cos_d + y * sin_d) - pmin) / span).clamp(0.0, 1.0) as f32
    }

    /// 正确的 time_ms：两头慢（dt/du 大）→ 中段快（dt/du 小），且单调递增
    /// 注意：专家原式的 0.19 系数会导致中段导数为负（时间倒流！），这里用 0.15 保证单调
    #[inline]
    pub(super) fn blend_time_ms(u: f64, total_ms: f64) -> f64 {
        let tp = u + 0.15 * (std::f64::consts::TAU * u).sin();
        total_ms * tp.clamp(0.0, 1.0)
    }

    // --- 解析形状（含 holes，见 parse_smart_shape）---
    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_blend 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_blend")?;
    let shape_type = parsed.outer.kind_str();
    let poly_pts: Vec<(f64, f64)> = parsed.outer.polygon_points();
    let (bx0, by0, bx1, by1): (f64, f64, f64, f64) = parsed.bbox;

    // 点是否在形状内（含孔洞扣除；排线游程切分穿过孔洞自动断开）
    let point_in_shape = |px: f64, py: f64| -> bool { parsed.contains(px, py) };

    // --- 方向场（③）：direction_guides / direction_from ---
    // 只传 guides 未传 blend_direction 时，取引导向量平均角作宏观渐变轴。
    let (dir_field, guides_mean) =
        parse_direction_field(args, &parsed, blend_direction_opt.unwrap_or(0.0));
    let blend_direction_deg = blend_direction_opt.unwrap_or_else(|| guides_mean.unwrap_or(0.0));

    // --- PRNG ---
    struct XorShift64(u64);
    impl XorShift64 {
        fn next_f64(&mut self) -> f64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            ((x >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next_f64()
        }
        fn gaussian(&mut self) -> f64 {
            (self.next_f64() + self.next_f64() + self.next_f64()) / 3.0 * 2.0 - 1.0
        }
    }
    let seed = ((bx0 * 73856093.0 + by0 * 19349663.0 + bx1 * 83492791.0) as u64)
        .wrapping_add(0x9E3779B97F4A7C15);
    let mut rng = XorShift64(seed.max(1));

    // --- 渐变投影范围 ---
    let blend_rad = blend_direction_deg.to_radians();
    let cos_b = blend_rad.cos();
    let sin_b = blend_rad.sin();
    let (proj_min, proj_max): (f64, f64) = match shape_type {
        "rect" => {
            let corners = [(bx0, by0), (bx1, by0), (bx1, by1), (bx0, by1)];
            let mut mn = f64::INFINITY;
            let mut mx = f64::NEG_INFINITY;
            for (x, y) in corners {
                let p = x * cos_b + y * sin_b;
                mn = mn.min(p);
                mx = mx.max(p);
            }
            (mn, mx)
        }
        "circle" => {
            let ccx = (bx0 + bx1) / 2.0;
            let ccy = (by0 + by1) / 2.0;
            let r = (bx1 - bx0) / 2.0;
            let c = ccx * cos_b + ccy * sin_b;
            (c - r, c + r)
        }
        "polygon" => {
            let mut mn = f64::INFINITY;
            let mut mx = f64::NEG_INFINITY;
            for (x, y) in &poly_pts {
                let p = x * cos_b + y * sin_b;
                mn = mn.min(p);
                mx = mx.max(p);
            }
            if mn > mx {
                (0.0, 1.0)
            } else {
                (mn, mx)
            }
        }
        _ => (0.0, 1.0),
    };
    let gradient_span = (proj_max - proj_min).max(1.0);

    // --- 排线生成（简化版：采样法 + 过冲 + 微弧度 + 往复）---
    let radius = size / 2.0;
    let overshoot = 1.5 * radius;
    let direction_jitter = 8.0f64;

    // 返回 Vec<Vec<(x,y)>>
    let gen_lines =
        |angle_offset: f64, spacing: f64, rng: &mut XorShift64| -> Vec<Vec<(f64, f64)>> {
            let angle_rad = blend_direction_deg.to_radians();
            let nx = -angle_rad.sin();
            let ny = angle_rad.cos();
            let cx = (bx0 + bx1) / 2.0;
            let cy = (by0 + by1) / 2.0;
            let half_diag =
                ((bx1 - bx0).powi(2) + (by1 - by0).powi(2)).sqrt() / 2.0 + overshoot + size;

            let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
            let mut delta_theta = 0.0;
            let mut offset = -half_diag;
            while offset <= half_diag {
                let jitter_now = rng.range(-direction_jitter, direction_jitter);
                delta_theta = 0.7 * delta_theta + 0.3 * jitter_now;
                let line_cx = cx + nx * offset;
                let line_cy = cy + ny * offset;
                // 方向场：该线中心处的局部走向（Constant 时恒等于 blend_direction_deg）。
                let la = (dir_field.angle_at(line_cx, line_cy) + angle_offset + delta_theta)
                    .to_radians();
                let ldx = la.cos();
                let ldy = la.sin();
                let sp = spacing * (1.0 + rng.gaussian() * 0.12);

                let step = (size / 2.0).max(2.0);
                let sample_extent = half_diag + overshoot;
                let mut current_seg: Vec<(f64, f64)> = Vec::new();
                let flush_seg = |seg: &mut Vec<(f64, f64)>, out: &mut Vec<Vec<(f64, f64)>>| {
                    if seg.len() >= 2 {
                        let (sx, sy) = seg[0];
                        let (ex, ey) = seg[seg.len() - 1];
                        let mut ext: Vec<(f64, f64)> = Vec::with_capacity(seg.len() + 2);
                        ext.push((sx - ldx * overshoot, sy - ldy * overshoot));
                        ext.extend(seg.iter().cloned());
                        ext.push((ex + ldx * overshoot, ey + ldy * overshoot));
                        let n = ext.len();
                        let (x0, y0) = ext[0];
                        let (x1, y1) = ext[n - 1];
                        let seg_len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                        let mut pts: Vec<(f64, f64)> = Vec::with_capacity(n);
                        for (pi, (px, py)) in ext.iter().enumerate() {
                            let u = pi as f64 / (n - 1).max(1) as f64;
                            let camber = (std::f64::consts::PI * u).sin() * 0.015 * seg_len;
                            pts.push((px - ldy * camber, py + ldx * camber));
                        }
                        if out.len() % 2 == 1 {
                            pts.reverse();
                        }
                        out.push(pts);
                    }
                    seg.clear();
                };

                let mut t = -sample_extent;
                while t <= sample_extent {
                    let px = line_cx + ldx * t;
                    let py = line_cy + ldy * t;
                    if point_in_shape(px, py) {
                        current_seg.push((px, py));
                    } else if !current_seg.is_empty() {
                        flush_seg(&mut current_seg, &mut lines);
                    }
                    t += step;
                }
                flush_seg(&mut current_seg, &mut lines);
                offset += sp;
            }
            lines
        };

    // --- 发射一笔 ---
    // 返回是否成功
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;
    let mut stroke_seq = 0usize; // 全局笔序号（用于 golden_dither）

    let emit_stroke = |points_xy: &[(f64, f64)],
                       stroke_color: [f32; 4],
                       brush_size: f64,
                       paint_push: f64,
                       taper: f64,
                       breath: f64,
                       ctx: &mut ToolContext<'_>,
                       ok_count: &mut usize,
                       fail_count: &mut usize,
                       first_error: &mut Option<String>| {
        let n = points_xy.len();
        if n < 2 {
            return;
        }
        let total_ms = 60.0 + (n as f64) * 8.0;
        let points: Vec<Value> = points_xy
            .iter()
            .enumerate()
            .map(|(i, (x, y))| {
                let u = i as f64 / (n - 1).max(1) as f64;
                let t_ms = blend_time_ms(u, total_ms);
                json!([x, y, 0.8, t_ms])
            })
            .collect();
        let col = json!([
            stroke_color[0],
            stroke_color[1],
            stroke_color[2],
            stroke_color[3]
        ]);
        let stroke_args = json!({
            "layer_id": layer_id,
            "brush": brush_name,
            "points": points,
            "size": brush_size,
            "color": col,
            "hand_tremor": 0.045,
            "color_breath": breath,
            "pressure_shape": 0.25,
            "stroke_taper": taper,
            "paint_push": paint_push,
            "preview": false,
            "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
        });
        match write_brush_stroke(ctx, &stroke_args) {
            Ok(_) => *ok_count += 1,
            Err(e) => {
                *fail_count += 1;
                if first_error.is_none() {
                    *first_error = Some(format!("{e:?}"));
                }
            }
        }
    };

    // dither 幅度：约 0.85 个排线间距占总渐变跨度的比例
    let dither_amp = ((size * 0.3 * 0.85) / gradient_span).clamp(0.015, 0.08) as f32;

    // === Pass 1：斜切铺色（方向场局部角 + 72°）===
    // 间距 size*0.25：实测笔触实际宽度约 0.6*size，0.25 确保 ~2.4x 重叠无漏白且纹理适中
    let pass1_lines = gen_lines(72.0, size * 0.25, &mut rng);
    for line in &pass1_lines {
        let n = line.len();
        if n < 4 {
            continue;
        }
        // 切成 2 段，重叠 25%
        // 段1: [0, 62.5%], 段2: [37.5%, 100%]
        let split1_end = ((n as f64) * 0.625) as usize;
        let split2_start = ((n as f64) * 0.375) as usize;
        let segments = [
            &line[0..split1_end.max(2)],
            &line[split2_start.min(n - 2)..],
        ];
        for seg in segments {
            if seg.len() < 2 {
                continue;
            }
            // 中点投影算 t
            let mid = seg[seg.len() / 2];
            let raw_t = project_blend_t(mid.0, mid.1, cos_b, sin_b, proj_min, proj_max);
            // 80% linear + 20% smoothstep（不用纯 smoothstep！）
            let ss = raw_t * raw_t * (3.0 - 2.0 * raw_t);
            let base_t = 0.80 * raw_t + 0.20 * ss;
            // 黄金分割抖动打破带
            let dither = golden_dither(stroke_seq) * dither_amp;
            let final_t = (base_t + dither).clamp(0.0, 1.0);
            let sc = lerp_oklab(color_a, color_b, final_t);
            emit_stroke(
                seg,
                sc,
                size,
                0.08,
                0.45,
                0.05,
                ctx,
                &mut ok_count,
                &mut fail_count,
                &mut first_error,
            );
            stroke_seq += 1;
        }
    }

    // === Pass 2：湿笔揉边（方向场局部角 - 25°）===
    // 注意：长线必须分段！否则整线用中点 t 会把两端颜色洗掉
    let pass2_lines = gen_lines(-25.0, size * 0.7, &mut rng);
    for line in &pass2_lines {
        let n = line.len();
        if n < 6 {
            continue;
        }
        // 切成 3 段，每段按自己中点取色
        let seg_len = n / 3;
        for s in 0..3 {
            let start = s * seg_len;
            let end = if s == 2 {
                n
            } else {
                (s + 1) * seg_len + seg_len / 2
            }; // 50% 重叠
            let end = end.min(n);
            if end - start < 2 {
                continue;
            }
            let seg = &line[start..end];
            let mid = seg[seg.len() / 2];
            let t = project_blend_t(mid.0, mid.1, cos_b, sin_b, proj_min, proj_max);
            let mut sc = lerp_oklab(color_a, color_b, t);
            sc[3] *= 0.45; // 半透明柔化
            emit_stroke(
                seg,
                sc,
                size * 1.35,
                0.38,
                0.35,
                0.04,
                ctx,
                &mut ok_count,
                &mut fail_count,
                &mut first_error,
            );
            stroke_seq += 1;
        }
    }

    let _ = stroke_seq;

    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_blend 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }

    let mut value = json!({
        "ok": fail_count == 0,
        "strokes": ok_count,
        "failed": fail_count,
        "pass1_lines": pass1_lines.len(),
        "pass2_lines": pass2_lines.len(),
        "bbox": [bx0, by0, bx1, by1],
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    Ok(value)
}

/// 椭圆的归一化距离 `q(p) = sqrt(((x-cx)/rx)² + ((y-cy)/ry)²)`。
///
/// `q <= 1` 即在椭圆内；同一条 `q = const` 等值线上的点离边界「一样远」
/// （按椭圆自身的长短轴度量）。`rx`/`ry` 任一非正时返回无穷大（无内点）。
pub(super) fn glaze_ellipse_q(cx: f64, cy: f64, rx: f64, ry: f64, px: f64, py: f64) -> f64 {
    if rx <= 0.0 || ry <= 0.0 {
        return f64::INFINITY;
    }
    let nx = (px - cx) / rx;
    let ny = (py - cy) / ry;
    (nx * nx + ny * ny).sqrt()
}

/// 点是否在给定半轴的椭圆内（`q <= 1`，与 `PrimShape::Ellipse::contains` 同口径）。
pub(super) fn glaze_ellipse_contains(cx: f64, cy: f64, rx: f64, ry: f64, px: f64, py: f64) -> bool {
    glaze_ellipse_q(cx, cy, rx, ry, px, py) <= 1.0
}

/// 罩染逐层内缩后这一层的椭圆半轴；内缩吃掉整个形状时返回 `None`。
///
/// 两种口径，差别只在真椭圆（`rx != ry`）上：
/// - 圆（`rx == ry`，与 `kind_str` 的 1e-9 阈值同口径）：`r - inset_px`，
///   与旧实现同一算术，逐位一致（回归锁见 tools_tests 的圆恒等式测试）。
/// - 真椭圆：按归一化距离内缩 —— 半轴同比例缩放 `k = 1 - inset_px/min(rx,ry)`，
///   于是这一层覆盖的点集恰好是 `{p : q(p) <= k}`：罩染作用量（覆盖层数）
///   只依赖 `q`，沿长轴与短轴的衰减剖面是同一条 `q` 曲线。若像圆那样对
///   两轴同减 `inset_px`，短轴会先被吃穿、等 `q` 点的覆盖层数也不再相等。
pub(super) fn glaze_ellipse_layer_radii(rx: f64, ry: f64, inset_px: f64) -> Option<(f64, f64)> {
    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    if (rx - ry).abs() < 1e-9 {
        let r = rx - inset_px;
        if r <= 0.0 {
            return None;
        }
        return Some((r, r));
    }
    let shrink = inset_px / rx.min(ry);
    if shrink >= 1.0 {
        return None;
    }
    let k = 1.0 - shrink;
    let out = (rx * k, ry * k);
    if out.0 <= 0.0 || out.1 <= 0.0 {
        None
    } else {
        Some(out)
    }
}

/// `smart_glaze`：透明罩染 / 水彩叠色（Glazing / Layer Wash）。
///
/// 在已画好的底稿上罩一层透明色（夕阳暖光、投影冷光、水彩花瓣、腮红血色）。
/// 与 smart_fill（厚涂铺底）、smart_blend（湿画法揉色）并列为「四大金刚」之三。
///
/// 解决两个致命引擎坑：
/// 1. Per-Dab Alpha 累积爆炸：MyPaint 的 alpha 是**每个 Dab** 的不透明度，
///    同一像素在一层内被约 3.8 个 Dab 叠加。直接传 0.35 会 3 层涂死（0.99）。
///    → 调用方传 `target_opacity`（整次罩染的目标视觉遮盖率），内部反推 dab_alpha。
/// 2. 排线重叠暗带：半透明下 30% 重叠带是 2 遍 Alpha → 斑马暗带。
///    → 间距加密到 size*0.62 + 黄金分割错位 + 柔边笔刷（watercolor_glazing）。
///
/// 核心铁律：paint_push = 0.0 —— 罩染是「干透叠色」（光学滤镜效应），
/// 绝不能搅动底色（搅动是 smart_blend 干的事）。
pub(super) fn write_smart_glaze(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— 在选区预热之前 ✓。
    let layer_id = require_str(args, "layer_id")?;
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    // Q4：`clip_to_selection` 透传 + 缓存预热（失败时忽略，走慢路径）。
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();
    if !clip_sel.is_empty() {
        let _ = precompute_clip_mask(ctx, &clip_sel);
    }

    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "watercolor_glazing".to_owned());
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(15.0);
    let base_direction = args
        .get("fill_direction")
        .and_then(Value::as_f64)
        .unwrap_or(-30.0);
    let direction_jitter = args
        .get("direction_jitter")
        .and_then(Value::as_f64)
        .unwrap_or(8.0);
    let direction_rotate = args
        .get("direction_rotate")
        .and_then(Value::as_f64)
        .unwrap_or(12.0);
    let layers = args
        .get("layers")
        .and_then(Value::as_u64)
        .unwrap_or(3)
        .clamp(1, 8) as usize;

    // --- 颜色（必填）---
    let base_rgba: [f64; 4] = match args.get("color") {
        Some(Value::Array(arr)) => [
            arr.first().and_then(Value::as_f64).unwrap_or(0.0),
            arr.get(1).and_then(Value::as_f64).unwrap_or(0.0),
            arr.get(2).and_then(Value::as_f64).unwrap_or(0.0),
            arr.get(3).and_then(Value::as_f64).unwrap_or(1.0),
        ],
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("smart_glaze 缺少必填参数 color".to_owned()),
            ))
        }
    };

    // --- 目标不透明度 → 每层目标（坑 1 的解法）---
    // AI 传 target_opacity（整次罩染完成后的视觉遮盖率）最直观，
    // 不用自己算 Dab 累积；兼容旧的 layer_opacity 参数。
    let avg_layer_opacity = if let Some(t) = args.get("target_opacity").and_then(Value::as_f64) {
        let tt = t.clamp(0.05, 0.95);
        1.0 - (1.0 - tt).powf(1.0 / layers as f64)
    } else {
        args.get("layer_opacity")
            .and_then(Value::as_f64)
            .unwrap_or(0.28)
            .clamp(0.02, 0.85)
    };
    // 每层内等效 Dab 叠加次数（spacing = 0.62*size 时的补偿系数）。
    // 每层内等效 Dab 叠加次数（spacing = 0.62*size 时的补偿系数）
    // Q8 注：专家建议改用 erf 自适应 K_eff，但其公式在小面积下算出 K≈1.4
    //（与实测 3.8 不符），在大面积下也无法复现预期的 14~18。
    // 公式需实证校准后才能替换，此处保留 3.8（小面积已验证有效）。
    const DAB_ACCUM_COMPENSATION: f64 = 3.8;

    // --- 形状解析（含 holes，见 parse_smart_shape；glaze 的分层内缩模型只用外层）---
    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_glaze 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_glaze")?;
    // Q4 早退：形状 bbox 与 clip 掩膜无交集时直接返回（不生成任何笔触，不提交原子）。
    if clip_bbox_early_out(
        ctx,
        &clip_sel,
        (
            parsed.bbox.0,
            parsed.bbox.1,
            parsed.bbox.2 - parsed.bbox.0,
            parsed.bbox.3 - parsed.bbox.1,
        ),
    ) {
        return Ok(json!({
            "ok": true,
            "clipped_out": true,
            "note": "形状包围盒与 clip_to_selection 无交集，未生成笔触",
        }));
    }
    let shape_type = parsed.outer.kind_str();
    let poly_pts: Vec<(f64, f64)> = parsed.outer.polygon_points();
    // 真椭圆（rx≠ry）的几何参数：内缩与点内判定都走椭圆距离场（见
    // glaze_ellipse_layer_radii），不能落到 bbox / 圆的口径上。
    let ellipse_params: Option<(f64, f64, f64, f64)> = match &parsed.outer {
        PrimShape::Ellipse { cx, cy, rx, ry } => Some((*cx, *cy, *rx, *ry)),
        _ => None,
    };
    // glaze 的分层内缩要一条边界折线：polygon 用顶点，path 用点数最多的
    // 展平环（主边界，与 edge_soften 同口径）；其它形状为空（走各自的臂）。
    let glaze_pts: Vec<(f64, f64)> = match &parsed.outer {
        PrimShape::Path { rings, .. } => rings
            .iter()
            .max_by_key(|ring| ring.len())
            .cloned()
            .unwrap_or_default(),
        _ => poly_pts.clone(),
    };
    let (bx0, by0, bx1, by1): (f64, f64, f64, f64) = parsed.bbox;

    // --- 辅助函数 ---
    /// 多边形顶点向质心内缩 inset_px（消除多层罩染的死板叠边，
    /// 形成中心浓、边缘淡的自然水彩过渡）
    pub(super) fn inset_polygon(pts: &[(f64, f64)], inset_px: f64) -> Vec<(f64, f64)> {
        if pts.is_empty() || inset_px.abs() < 1e-3 {
            return pts.to_vec();
        }
        let n = pts.len() as f64;
        let (sx, sy) = pts
            .iter()
            .fold((0.0, 0.0), |(ax, ay), &(x, y)| (ax + x, ay + y));
        let (cx, cy) = (sx / n, sy / n);
        pts.iter()
            .map(|&(x, y)| {
                let dx = x - cx;
                let dy = y - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist > inset_px * 1.5 {
                    let scale = (dist - inset_px) / dist;
                    (cx + dx * scale, cy + dy * scale)
                } else {
                    (cx + dx * 0.35, cy + dy * 0.35)
                }
            })
            .collect()
    }

    /// Beer-Lambert 罩染色彩演进：后层更浓郁深邃（暗通道衰减更快，
    /// 而不是标准 Alpha 那样永远逼近原色）。
    pub(super) fn evolve_glaze_color(
        base: [f64; 4],
        layer_idx: usize,
        total_layers: usize,
    ) -> [f64; 3] {
        if total_layers <= 1 || layer_idx == 0 {
            return [base[0], base[1], base[2]];
        }
        let tau = layer_idx as f64 / (total_layers - 1) as f64;
        let gamma = 1.0 + 0.28 * tau;
        let darken = 1.0 - 0.12 * tau;
        [
            (base[0].clamp(0.0, 1.0).powf(gamma) * darken).clamp(0.0, 1.0),
            (base[1].clamp(0.0, 1.0).powf(gamma) * darken).clamp(0.0, 1.0),
            (base[2].clamp(0.0, 1.0).powf(gamma) * darken).clamp(0.0, 1.0),
        ]
    }

    /// time_ms：两头慢（落笔/收笔）→ 中段快。
    /// 0.15 系数保证导数恒正（单调递增）；0.19 会导致中段导数为负（时间倒流！）
    #[inline]
    pub(super) fn glaze_time_ms(u: f64, total_ms: f64) -> f64 {
        let tp = u + 0.15 * (std::f64::consts::TAU * u).sin();
        total_ms * tp.clamp(0.0, 1.0)
    }

    // 黄金角摆动：围绕主方向左右交替微倾，避开有理数倍夹角，
    // 防止等差旋转（如每层 +15°）产生的摩尔纹。
    const ANGLE_SWING: [f64; 8] = [0.0, 1.0, -0.618, 0.382, -1.15, 0.75, -0.35, 1.25];

    // --- PRNG（确定性：同输入同输出）---
    struct XorShift64(u64);
    impl XorShift64 {
        fn next_f64(&mut self) -> f64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            ((x >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next_f64()
        }
        fn gaussian(&mut self) -> f64 {
            (self.next_f64() + self.next_f64() + self.next_f64()) / 3.0 * 2.0 - 1.0
        }
    }
    let seed = ((bx0 * 73856093.0 + by0 * 19349663.0 + bx1 * 83492791.0) as u64)
        .wrapping_add(0x9E3779B97F4A7C15);
    let mut rng = XorShift64(seed.max(1));

    // 层权重：前层略厚定调，后层略薄提神
    let weights: Vec<f64> = (0..layers)
        .map(|i| (1.0 - 0.12 * i as f64).max(0.4))
        .collect();
    let weight_mean: f64 = weights.iter().sum::<f64>() / layers as f64;

    let radius = size / 2.0;
    let overshoot = 1.5 * radius;
    let spacing = size * 0.62; // 加密间距，消除半透明排线之间的白缝

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;
    let mut layer_lines: Vec<usize> = Vec::new();
    let mut layer_alphas: Vec<f64> = Vec::new();

    for i in 0..layers {
        // (a) 目标层不透明度（dab_alpha 在内缩 bbox 确定后按 Q8 自适应 K_eff 反推，见下）。
        let target_layer_alpha = (avg_layer_opacity * weights[i] / weight_mean).clamp(0.02, 0.75);
        // (b) Beer-Lambert 色彩演进（后层更浓郁）
        let rgb = evolve_glaze_color(base_rgba, i, layers);
        // layer_color 在 stroke_alpha 确定后定义（见下）。
        // (c) 角度摆动 + 逐层内缩（0.18*size/层）
        let layer_angle = base_direction + direction_rotate * ANGLE_SWING[i % ANGLE_SWING.len()];
        let inset_px = i as f64 * size * 0.18;

        // 内缩后的形状：(x0, y0, x1, y1, polygon 顶点)
        let (lbx0, lby0, lbx1, lby1, lpoly): (f64, f64, f64, f64, Vec<(f64, f64)>) =
            match shape_type {
                "rect" => (
                    bx0 + inset_px,
                    by0 + inset_px,
                    bx1 - inset_px,
                    by1 - inset_px,
                    Vec::new(),
                ),
                "circle" => {
                    let ccx = (bx0 + bx1) / 2.0;
                    let ccy = (by0 + by1) / 2.0;
                    let r = (bx1 - bx0) / 2.0 - inset_px;
                    (ccx - r, ccy - r, ccx + r, ccy + r, Vec::new())
                }
                "ellipse" => {
                    // 真椭圆逐层内缩：半轴按归一化距离同比例收缩（作用量
                    // 只依赖 q）；内缩吃掉整个椭圆时给出无效标记，该层跳过。
                    match ellipse_params {
                        Some((ecx, ecy, erx, ery)) => {
                            match glaze_ellipse_layer_radii(erx, ery, inset_px) {
                                Some((lrx, lry)) => {
                                    (ecx - lrx, ecy - lry, ecx + lrx, ecy + lry, Vec::new())
                                }
                                None => (0.0, 0.0, -1.0, -1.0, Vec::new()),
                            }
                        }
                        None => (0.0, 0.0, -1.0, -1.0, Vec::new()),
                    }
                }
                "polygon" | "path" => {
                    let ip = inset_polygon(&glaze_pts, inset_px);
                    if ip.len() < 3 {
                        (0.0, 0.0, -1.0, -1.0, Vec::new()) // 标记无效
                    } else {
                        let mut x0 = f64::MAX;
                        let mut y0 = f64::MAX;
                        let mut x1 = f64::MIN;
                        let mut y1 = f64::MIN;
                        for (px, py) in &ip {
                            x0 = x0.min(*px);
                            y0 = y0.min(*py);
                            x1 = x1.max(*px);
                            y1 = y1.max(*py);
                        }
                        (x0, y0, x1, y1, ip)
                    }
                }
                _ => (0.0, 0.0, -1.0, -1.0, Vec::new()),
            };
        // 内缩后形状已消失 → 该层跳过（layer_lines 记 0，可观测，不静默）
        if lbx1 <= lbx0 || lby1 <= lby0 {
            layer_lines.push(0);
            layer_alphas.push(0.0);
            continue;
        }

        // (a) 反推 dab_alpha：用固定补偿系数（Q8 的 erf 公式待实证校准）。
        let stroke_alpha = 1.0 - (1.0 - target_layer_alpha).powf(1.0 / DAB_ACCUM_COMPENSATION);
        let layer_color = json!([rgb[0], rgb[1], rgb[2], stroke_alpha]);

        // 本层点是否在形状内
        let ccx = (lbx0 + lbx1) / 2.0;
        let ccy = (lby0 + lby1) / 2.0;
        let crad = (lbx1 - lbx0) / 2.0;
        let point_in = |px: f64, py: f64| -> bool {
            match shape_type {
                "rect" => px >= lbx0 && px <= lbx1 && py >= lby0 && py <= lby1,
                "circle" => (px - ccx).powi(2) + (py - ccy).powi(2) <= crad * crad,
                "ellipse" => {
                    // 本层 bbox 半宽/半高即内缩后的半轴（bbox 由半轴构造），
                    // 中心即椭圆中心：按 q<=1 判定，而不是按圆的 crad。
                    let lrx = (lbx1 - lbx0) / 2.0;
                    let lry = (lby1 - lby0) / 2.0;
                    glaze_ellipse_contains(ccx, ccy, lrx, lry, px, py)
                }
                "polygon" | "path" => {
                    if lpoly.len() < 3 {
                        return px >= lbx0 && px <= lbx1 && py >= lby0 && py <= lby1;
                    }
                    let mut inside = false;
                    let n = lpoly.len();
                    let mut j = n - 1;
                    for k in 0..n {
                        let (xi, yi) = lpoly[k];
                        let (xj, yj) = lpoly[j];
                        let intersect = ((yi > py) != (yj > py))
                            && (px < (xj - xi) * (py - yi) / (yj - yi) + xi);
                        if intersect {
                            inside = !inside;
                        }
                        j = k;
                    }
                    inside
                }
                _ => false,
            }
        };

        // (d) 生成排线：采样法 + 过冲 + 相关角度游走 + 微弧度 + 往复运笔
        //     起始偏移按黄金分割错开，避免层与层排线重合（防摩尔纹）
        let phase_shift = ((i as f64) * 0.6180339887).fract();
        let gen_lines = |angle_deg: f64, rng: &mut XorShift64| -> Vec<Vec<(f64, f64)>> {
            let angle_rad = angle_deg.to_radians();
            let nx = -angle_rad.sin();
            let ny = angle_rad.cos();
            let cx = (lbx0 + lbx1) / 2.0;
            let cy = (lby0 + lby1) / 2.0;
            let half_diag =
                ((lbx1 - lbx0).powi(2) + (lby0 - lby1).powi(2)).sqrt() / 2.0 + overshoot + size;
            let mut lines: Vec<Vec<(f64, f64)>> = Vec::new();
            let mut delta_theta = 0.0;
            let mut offset = -half_diag + phase_shift * spacing;
            while offset <= half_diag {
                // 相关角度游走（非独立随机！）：0.7 继承 + 0.3 新随机
                let jitter_now = rng.range(-direction_jitter, direction_jitter);
                delta_theta = 0.7 * delta_theta + 0.3 * jitter_now;
                let la = (angle_deg + delta_theta).to_radians();
                let ldx = la.cos();
                let ldy = la.sin();
                // 间距 ±12% 高斯微扰，打破机械光栅感
                let sp = spacing * (1.0 + rng.gaussian() * 0.12);
                let line_cx = cx + nx * offset;
                let line_cy = cy + ny * offset;

                let step = (size / 2.0).max(2.0);
                let sample_extent = half_diag + overshoot;
                let mut current_seg: Vec<(f64, f64)> = Vec::new();
                let flush_seg = |seg: &mut Vec<(f64, f64)>, out: &mut Vec<Vec<(f64, f64)>>| {
                    if seg.len() >= 2 {
                        let (sx, sy) = seg[0];
                        let (ex, ey) = seg[seg.len() - 1];
                        let mut ext: Vec<(f64, f64)> = Vec::with_capacity(seg.len() + 2);
                        // 过冲：pressure_shape 变细的两端落在形状外
                        ext.push((sx - ldx * overshoot, sy - ldy * overshoot));
                        ext.extend(seg.iter().cloned());
                        ext.push((ex + ldx * overshoot, ey + ldy * overshoot));
                        // 生理微弧：0.015 × L
                        let n = ext.len();
                        let (x0, y0) = ext[0];
                        let (x1, y1) = ext[n - 1];
                        let seg_len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                        let mut final_pts: Vec<(f64, f64)> = Vec::with_capacity(n);
                        for (pi, (px, py)) in ext.iter().enumerate() {
                            let u = pi as f64 / (n - 1).max(1) as f64;
                            let camber = (std::f64::consts::PI * u).sin() * 0.015 * seg_len;
                            final_pts.push((px - ldy * camber, py + ldx * camber));
                        }
                        // 往复运笔：奇数笔反向，墨色平衡
                        if out.len() % 2 == 1 {
                            final_pts.reverse();
                        }
                        out.push(final_pts);
                    }
                    seg.clear();
                };
                let mut t = -sample_extent;
                while t <= sample_extent {
                    let px = line_cx + ldx * t;
                    let py = line_cy + ldy * t;
                    if point_in(px, py) {
                        current_seg.push((px, py));
                    } else if !current_seg.is_empty() {
                        flush_seg(&mut current_seg, &mut lines);
                    }
                    t += step;
                }
                flush_seg(&mut current_seg, &mut lines);
                offset += sp;
            }
            lines
        };

        let lines = gen_lines(layer_angle, &mut rng);

        // (e) 逐笔绘制：paint_push = 0.0（干透罩染，绝不搅动底色！）
        let mut n_lines = 0usize;
        for line in &lines {
            if line.len() < 2 {
                continue;
            }
            n_lines += 1;
            let n = line.len();
            let total_ms = 60.0 + n as f64 * 8.0;
            let points: Vec<Value> = line
                .iter()
                .enumerate()
                .map(|(pi, (x, y))| {
                    let u = pi as f64 / (n - 1).max(1) as f64;
                    json!([x, y, 0.8, glaze_time_ms(u, total_ms)])
                })
                .collect();
            let stroke_args = json!({
                "layer_id": layer_id,
                "brush": brush_name,
                "points": points,
                "size": size,
                "color": layer_color,
                "hand_tremor": 0.04,
                "color_breath": 0.04,
                "pressure_shape": 0.15,
                "stroke_taper": 0.35,
                "paint_push": 0.0,
                "preview": false,
                "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
            });
            match write_brush_stroke(ctx, &stroke_args) {
                Ok(_) => ok_count += 1,
                Err(e) => {
                    fail_count += 1;
                    if first_error.is_none() {
                        first_error = Some(format!("{e:?}"));
                    }
                }
            }
        }
        layer_lines.push(n_lines);
        layer_alphas.push(stroke_alpha);
    }

    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_glaze 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }

    let mut value = json!({
        "ok": fail_count == 0,
        "strokes": ok_count,
        "failed": fail_count,
        "layers": layers,
        "layer_lines": layer_lines,
        "layer_dab_alphas": layer_alphas,
        "brush": brush_name,
        "bbox": [bx0, by0, bx1, by1],
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    Ok(value)
}

pub(super) fn write_smart_edge_soften(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— 在选区预热之前 ✓。
    let layer_id = require_str(args, "layer_id")?;
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    // Q4：`clip_to_selection` 透传 + 缓存预热（失败时忽略，走慢路径）。
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();
    if !clip_sel.is_empty() {
        let _ = precompute_clip_mask(ctx, &clip_sel);
    }

    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let size = args
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or(12.0)
        .clamp(2.0, 60.0);
    let soften_width = args
        .get("soften_width")
        .and_then(Value::as_f64)
        .unwrap_or(20.0)
        .clamp(2.0, 100.0);
    let strength = args
        .get("soften_strength")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.1, 1.0);
    let direction = optional_str(args, "soften_direction").unwrap_or_else(|| "both".to_owned());

    // --- 颜色（可选）：建议传边缘主体色；不传则用笔刷自带色 ---
    // 注意：不要传纯透明色 —— MyPaint 的透明区保护机制会把边缘抠出锯齿镂空。
    let color_opt: Option<[f32; 4]> = args.get("color").and_then(|v| v.as_array()).map(|arr| {
        [
            arr.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(3).and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ]
    });

    // --- 解析边缘路径：edge_path 或 shape 二选一 ---
    let mut path_pts: Vec<(f64, f64)> = Vec::new();
    let mut closed = false;
    if let Some(ep) = args.get("edge_path").and_then(|v| v.as_array()) {
        for p in ep {
            if let Some(pair) = p.as_array() {
                if pair.len() >= 2 {
                    path_pts.push((
                        pair[0].as_f64().unwrap_or(0.0),
                        pair[1].as_f64().unwrap_or(0.0),
                    ));
                }
            }
        }
        if path_pts.len() >= 3 {
            let f = path_pts[0];
            let l = path_pts[path_pts.len() - 1];
            if (f.0 - l.0).hypot(f.1 - l.1) < 2.0 {
                closed = true;
                path_pts.pop(); // 去掉重复的闭合点
            }
        }
        // **曲线尺吸附**（二期 `path_guide`）：只作用于 `edge_path` 输入点列
        //（`shape` 派生边界是精确几何，不吸附）；只拉落点坐标，柔化宽度/
        // 强度等参数不受影响。缺省不传 ⇒ 点列逐位不变。
        if args.get("path_guide").is_some() {
            path_pts = apply_path_guide_xy(ctx, args, path_pts)?;
        }
    } else if let Some(shape) = args.get("shape") {
        // 形状转边缘路径（只用外层；孔洞边界不在柔化范围）。
        let parsed = parse_smart_shape(shape, "smart_edge_soften")?;
        match &parsed.outer {
            PrimShape::Ellipse { cx, cy, rx, ry } => {
                // circle 即 rx==ry 的 ellipse；非正圆按平均半径近似。
                let r = (rx + ry) / 2.0;
                let r = r.max(1.0);
                for k in 0..96 {
                    let a = (k as f64) / 96.0 * std::f64::consts::TAU;
                    path_pts.push((cx + r * a.cos(), cy + r * a.sin()));
                }
                closed = true;
            }
            PrimShape::Rect { x, y, w, h } => {
                path_pts = vec![(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)];
                closed = true;
            }
            PrimShape::Polygon { points } => {
                for (px, py) in points {
                    path_pts.push((*px, *py));
                }
                closed = true;
            }
            PrimShape::Path {
                subpaths, rings, ..
            } => {
                // 边缘路径只有一条：取点数最多的展平环作主边界（多子路径
                // 时其余环不在柔化范围，与「只用外层」的既有口径一致）；
                // 闭合性随该子路径自身的 closed。
                let main = rings
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, ring)| ring.len())
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                if let Some(ring) = rings.get(main) {
                    path_pts = ring.clone();
                    closed = subpaths.get(main).map(|s| s.closed).unwrap_or(false);
                }
            }
        }
    } else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "smart_edge_soften 需要 edge_path 或 shape 参数（二选一）".to_owned(),
            ),
        ));
    }
    if path_pts.len() < 2 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_edge_soften 的边缘路径至少需要 2 个点".to_owned()),
        ));
    }

    // --- 弧长均匀重采样（步距 = size * 0.45，解决输入点疏密不均）---
    let step = size * 0.45;
    let n_in = path_pts.len();
    let seg_count = if closed { n_in } else { n_in - 1 };
    let mut sampled: Vec<(f64, f64)> = vec![path_pts[0]];
    let mut since = 0.0; // 距上一个采样点的距离
    for s in 0..seg_count {
        let p0 = path_pts[s % n_in];
        let p1 = path_pts[(s + 1) % n_in];
        let dx = p1.0 - p0.0;
        let dy = p1.1 - p0.1;
        let len = dx.hypot(dy);
        if len < 1e-9 {
            continue;
        }
        let mut d = step - since;
        while d < len {
            let t = d / len;
            sampled.push((p0.0 + dx * t, p0.1 + dy * t));
            d += step;
        }
        let last_emit = d - step;
        since = if last_emit < 0.0 {
            since + len
        } else {
            len - last_emit
        };
    }
    // **按 `max_steps` 抽稀**（第 682 轮：矩形 1000px 时会有 ~740 个原子 ⇒ 每次落笔 740 次渲染）。
    // 注意是**等距抽稀**而不是砍尾巴 —— 砍尾巴会让边缘的后半段完全不被柔化，
    // 那是"看起来对了但一半没做"的静默降级。
    let max_steps = optional_count(args, "max_steps")?.unwrap_or(400) as usize;
    let sampled: Vec<(f64, f64)> = if sampled.len() > max_steps && max_steps >= 2 {
        let stride = (sampled.len() as f64) / (max_steps as f64);
        (0..max_steps)
            .map(|k| sampled[((k as f64 * stride) as usize).min(sampled.len() - 1)])
            .collect()
    } else {
        sampled
    };
    let m = sampled.len();
    if m < 2 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("smart_edge_soften 路径太短，无法重采样".to_owned()),
        ));
    }

    // --- 平滑法线：邻点切线 → 候选法线 → 质心定向（闭合） → 3 点移动平均 ---
    let tangent_at = |i: usize| -> (f64, f64) {
        let p_prev = if closed {
            sampled[(i + m - 1) % m]
        } else {
            sampled[i.saturating_sub(1)]
        };
        let p_next = if closed {
            sampled[(i + 1) % m]
        } else {
            sampled[(i + 1).min(m - 1)]
        };
        let dx = p_next.0 - p_prev.0;
        let dy = p_next.1 - p_prev.1;
        let l = dx.hypot(dy).max(1e-9);
        (dx / l, dy / l)
    };
    let mut normals: Vec<(f64, f64)> = (0..m)
        .map(|i| {
            let (tx, ty) = tangent_at(i);
            (-ty, tx)
        })
        .collect();
    if closed {
        // 闭合路径：法线统一朝外（远离质心）
        let (sx, sy) = sampled
            .iter()
            .fold((0.0, 0.0), |(ax, ay), &(x, y)| (ax + x, ay + y));
        let (cx, cy) = (sx / m as f64, sy / m as f64);
        for i in 0..m {
            let (px, py) = sampled[i];
            let (nx, ny) = normals[i];
            if nx * (px - cx) + ny * (py - cy) < 0.0 {
                normals[i] = (-nx, -ny);
            }
        }
    }
    let mut smooth_n: Vec<(f64, f64)> = vec![(0.0, 0.0); m];
    for i in 0..m {
        let at = |j: isize| -> (f64, f64) {
            if closed {
                normals[(i as isize + j).rem_euclid(m as isize) as usize]
            } else {
                normals[(i as isize + j).clamp(0, m as isize - 1) as usize]
            }
        };
        let (ax, ay) = at(-1);
        let (bx, by) = at(0);
        let (cx2, cy2) = at(1);
        let mx = (ax + bx + cx2) / 3.0;
        let my = (ay + by + cy2) / 3.0;
        let l = mx.hypot(my).max(1e-9);
        smooth_n[i] = (mx / l, my / l);
    }

    // --- 方向集合（smooth_n 已朝外；inward 取反）---
    let dir_signs: Vec<f64> = match direction.as_str() {
        "outward" => vec![1.0],
        "inward" => vec![-1.0],
        _ => vec![1.0, -1.0],
    };

    /// 正确的 time_ms：两头慢 → 中段快，且单调递增（0.15 系数保证单调不倒流）
    #[inline]
    pub(super) fn edge_time_ms(u: f64, total_ms: f64) -> f64 {
        let tp = u + 0.15 * (std::f64::consts::TAU * u).sin();
        total_ms * tp.clamp(0.0, 1.0)
    }

    let total_ms = (m as f64 * 12.0).clamp(100.0, 5000.0);
    // 大宽度自动分级接力：每级有效拖拽约 10px，最多 3 级
    let relay_steps = (soften_width / 10.0).ceil().clamp(1.0, 3.0) as usize;

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;

    let emit = |points: Vec<Value>,
                brush_size: f64,
                paint_push: f64,
                pressure_shape: f64,
                taper: f64,
                breath: f64,
                ctx: &mut ToolContext<'_>,
                ok_count: &mut usize,
                fail_count: &mut usize,
                first_error: &mut Option<String>| {
        let mut stroke_args = json!({
            "layer_id": layer_id,
            "brush": brush_name,
            "points": points,
            "size": brush_size,
            "hand_tremor": 0.04,
            "color_breath": breath,
            "pressure_shape": pressure_shape,
            "stroke_taper": taper,
            "paint_push": paint_push,
            "preview": false,
            "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
        });
        if let Some(c) = color_opt {
            stroke_args["color"] = json!([c[0], c[1], c[2], c[3]]);
        }
        match write_brush_stroke(ctx, &stroke_args) {
            Ok(_) => *ok_count += 1,
            Err(e) => {
                *fail_count += 1;
                if first_error.is_none() {
                    *first_error = Some(format!("{e:?}"));
                }
            }
        }
    };

    // === Pass 1：沿法线接力拖曳 ===
    // 关键：pressure_shape=+0.35（起笔轻 → 中段重 → 收笔尖），形成尖尾拖曳；
    // 用 -0.3 会把颜色按死在内侧、拉不出渐变（已踩坑）。
    for &dir_sign in &dir_signs {
        for i in 0..m {
            let (nx, ny) = (smooth_n[i].0 * dir_sign, smooth_n[i].1 * dir_sign);
            let (px, py) = sampled[i];
            let u = i as f64 / (m.max(2) - 1) as f64;
            let t0 = edge_time_ms(u, total_ms);
            for step_idx in 0..relay_steps {
                let inner_off = step_idx as f64 * 8.0;
                let outer_off = inner_off + soften_width / relay_steps as f64;
                let sx = px + nx * inner_off;
                let sy = py + ny * inner_off;
                let ex = px + nx * outer_off;
                let ey = py + ny * outer_off;
                let mx = sx + (ex - sx) * 0.5;
                let my = sy + (ey - sy) * 0.5;
                let points = vec![
                    json!([sx, sy, 0.35, t0]),
                    json!([mx, my, 0.75, t0 + 15.0]),
                    json!([ex, ey, 0.20, t0 + 35.0]),
                ];
                emit(
                    points,
                    size * (1.0 - 0.1 * step_idx as f64),
                    strength,
                    0.35,
                    0.35,
                    0.03,
                    ctx,
                    &mut ok_count,
                    &mut fail_count,
                    &mut first_error,
                );
            }
            // 曲率补偿：外凸角处法线发散，加一笔填扇形缝隙（内凹会聚则天然重叠，无需补）
            if dir_sign > 0.0 {
                let j = if closed {
                    (i + 1) % m
                } else {
                    (i + 1).min(m - 1)
                };
                if j != i {
                    let dot = smooth_n[i].0 * smooth_n[j].0 + smooth_n[i].1 * smooth_n[j].1;
                    if dot < 0.94 {
                        let anx = (smooth_n[i].0 + smooth_n[j].0) / 2.0;
                        let any = (smooth_n[i].1 + smooth_n[j].1) / 2.0;
                        let al = anx.hypot(any).max(1e-9);
                        let (anx, any) = (anx / al, any / al);
                        let qx = (sampled[i].0 + sampled[j].0) / 2.0;
                        let qy = (sampled[i].1 + sampled[j].1) / 2.0;
                        let u2 = ((i as f64 + 0.5) / (m.max(2) - 1) as f64).min(1.0);
                        let t2 = edge_time_ms(u2, total_ms);
                        let ex2 = qx + anx * soften_width * 0.7;
                        let ey2 = qy + any * soften_width * 0.7;
                        let points = vec![
                            json!([qx, qy, 0.35, t2]),
                            json!([(qx + ex2) / 2.0, (qy + ey2) / 2.0, 0.7, t2 + 15.0]),
                            json!([ex2, ey2, 0.20, t2 + 35.0]),
                        ];
                        emit(
                            points,
                            size * 0.8,
                            strength * 0.8,
                            0.35,
                            0.35,
                            0.03,
                            ctx,
                            &mut ok_count,
                            &mut fail_count,
                            &mut first_error,
                        );
                    }
                }
            }
        }
    }

    // === Pass 2：边缘梳理（沿切线轻扫，消除 Pass 1 的梳子纹）===
    // 注意：强度减半、笔刷减半，绝不能与 Pass 1 同强度（否则柔边发灰变脏）。
    {
        let chunk = 8usize;
        let mut c = 0usize;
        while c < m {
            let end = (c + chunk).min(m);
            if end - c < 2 {
                break;
            }
            let points: Vec<Value> = (c..end)
                .map(|k| {
                    let u = k as f64 / (m.max(2) - 1) as f64;
                    json!([sampled[k].0, sampled[k].1, 0.6, edge_time_ms(u, total_ms)])
                })
                .collect();
            emit(
                points,
                size * 0.5,
                strength * 0.45,
                0.2,
                0.3,
                0.03,
                ctx,
                &mut ok_count,
                &mut fail_count,
                &mut first_error,
            );
            if end == m {
                break;
            }
            c += chunk - 2; // 重叠 2 点，无缝衔接
        }
    }

    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_edge_soften 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }

    let mut value = json!({
        "ok": fail_count == 0,
        "strokes": ok_count,
        "failed": fail_count,
        "edge_points": m,
        "relay_steps": relay_steps,
        "soften_width": soften_width,
        "direction": direction,
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    Ok(value)
}

/// `smart_texture` 用的确定性 PRNG（与各 smart_* 工具内的 XorShift64 同算法）。
pub(super) struct TextureXorShift(pub(super) u64);
impl TextureXorShift {
    pub(super) fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 11) as f64) / ((1u64 << 53) as f64)
    }
    pub(super) fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }
    /// 近似高斯（与各 smart_* 内 `XorShift64::gaussian` 同实现：3 个 uniform 平均）。
    pub(super) fn gaussian(&mut self) -> f64 {
        (self.next_f64() + self.next_f64() + self.next_f64()) / 3.0 * 2.0 - 1.0
    }
}

/// 专家方案「一、2」：5 大材质工程参数表（已校准）。
///
/// 设计要点：外层生成器只负责**中观轨迹**（Meso-path），微颗粒由
/// `scatter_texture` 在引擎层打散 —— 单笔过短会印出死板墨点（v1 的教训），
/// 单笔过长会糊成一团（由「一、4」的 N 公式按长度倍数反推笔数来约束）。
#[derive(Clone, Copy)]
pub(super) struct TextureProfile {
    pub(super) len_mult: f64,         // 笔触长度倍数 m = L / size
    pub(super) jitter: f64,           // 网格抖动率 0..1
    pub(super) cluster: f64,          // 低频簇状聚集强度 0..1
    pub(super) scatter: f64,          // scatter_texture 参数
    pub(super) tremor: f64,           // hand_tremor 参数
    pub(super) base_alpha: f64,       // 建议最大不透明度（Alpha 永不超此值）
    pub(super) pair_offset_mult: f64, // ④ fold 偶极子：亮暗笔法向间距 = pair_offset_mult × size（其余材质 0.0）
    pub(super) dark_l_shift: f64,     // ④ fold 偶极子：暗笔明度偏移（其余材质 0.0）
    pub(super) light_l_shift: f64,    // ④ fold 偶极子：亮笔明度偏移（其余材质 0.0）
}

pub(super) fn get_texture_profile(texture_type: &str) -> TextureProfile {
    match texture_type {
        "canvas" => TextureProfile {
            len_mult: 3.5,
            jitter: 0.20,
            cluster: 0.0,
            scatter: 0.55,
            tremor: 0.015,
            base_alpha: 0.28,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        },
        "fabric" => TextureProfile {
            len_mult: 4.5,
            jitter: 0.30,
            cluster: 0.15,
            scatter: 0.50,
            tremor: 0.025,
            base_alpha: 0.30,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        },
        "skin" => TextureProfile {
            len_mult: 4.0,
            jitter: 0.65,
            cluster: 0.20,
            scatter: 0.90,
            tremor: 0.040,
            base_alpha: 0.22,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        },
        "wood" => TextureProfile {
            len_mult: 10.0,
            jitter: 0.45,
            cluster: 0.35,
            scatter: 0.45,
            tremor: 0.030,
            base_alpha: 0.35,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        },
        // ④ fold：衣褶偶极子（受光褶脊 + 背光褶沟，成对生成）。
        "fold" => TextureProfile {
            len_mult: 7.5, // 长条状，不能短
            jitter: 0.45,
            cluster: 0.25,
            scatter: 0.22, // 布褶连续柔和，飞白必须低
            tremor: 0.02,
            base_alpha: 0.35,
            pair_offset_mult: 0.85, // 法向偏移 0.85×size，紧贴暗沟旁形成高光脊
            dark_l_shift: -0.16,
            light_l_shift: 0.13,
        },
        _ /* "stone" */ => TextureProfile {
            len_mult: 3.0,
            jitter: 0.85,
            cluster: 0.65,
            scatter: 0.50,
            tremor: 0.070,
            base_alpha: 0.38,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        },
    }
}

/// 专家方案「一、4」：按目标视觉覆盖率 τ 与单笔有效面积反推笔数 N。
///
/// A_stroke = η_scatter · m · size²，η_scatter = 1 − 0.55·scatter；
/// τ = 0.05 + 0.50·density（density=0.5 时 τ≈0.30 最透气，上限 0.55）；
/// N = clamp(round(Area_shape·τ / A_stroke), 8, 240)。
/// 大区域自适应保护：未截断 N > 240 时把有效 size 按 √(N/240) 放大，
/// 使实际笔数恒 ≤ 240（防原子日志爆炸）。返回 (target_n, effective_size)。
pub(super) fn texture_target_strokes(
    shape_area: f64,
    density: f64,
    size: f64,
    profile: TextureProfile,
) -> (usize, f64) {
    let eta = 1.0 - 0.55 * profile.scatter;
    let tau = 0.05 + 0.50 * density.clamp(0.05, 1.0);
    let raw_n = (shape_area.max(1.0) * tau) / (eta * profile.len_mult * size * size).max(1e-9);
    let target_n = (raw_n.round() as usize).clamp(8, 240);
    let effective_size = if raw_n > 240.0 {
        size * (raw_n / 240.0).sqrt()
    } else {
        size
    };
    (target_n, effective_size)
}

/// 专家方案「一、1」：分层抖动网格 + 低频簇状调制。
///
/// 每个格子取一个中心偏移点（x = x0 + (i + 0.5 + jitter·rand(−0.5, 0.5))·Δ），
/// 再乘以 2 阶正弦低频密度场做簇状过滤 —— 只调 jitter/cluster 即通吃 5 种材质，
/// O(N) 且无拒绝采样的死循环风险。返回的点都在 `inside_shape` 内。
///
/// 与专家原文的偏离（实测依据，见报告）：
/// 专家代码版 `keep_prob = (1−cluster) + cluster·wave` 在 stone（cluster=0.65）下
/// 实测 CV≈0.21，达不到专家自检清单的 CV∈[0.35, 0.75]（谷底 0.35 的 floor 吃掉了
/// 对比度，形不成「石斑簇群 + 留白呼吸区」）；而专家文字版是「`rng.next_f64() < M(x,y)`
/// 时才落笔」（M∈[0,1] 直接做保留概率，无 floor）。此处采用调和式
/// `keep_prob = wave^(2·cluster)`：cluster=0 时指数为 0 → 恒 1（canvas 不过滤 ✓）；
/// cluster 增大时对比度单调增强；stone 在 200×200 下 CV 稳定在 0.46~0.65 ✓。
/// 频率取 1 周期/区域（专家代码 `*2.0` 在 4×4 子格尺度下被平均掉，CV 仍 <0.35；
/// 「低频」本意即周期越大越好）。
// 纹理种子收集需要 rng/profile/尺寸/网格/簇参数等 10 个入参；拆成结构体只是把参数搬个家。
#[allow(clippy::too_many_arguments)]
pub(super) fn collect_texture_seeds(
    rng: &mut TextureXorShift,
    profile: TextureProfile,
    bx0: f64,
    by0: f64,
    bx1: f64,
    by1: f64,
    target_n: usize,
    shape_area: f64,
    effective_size: f64,
    inside_shape: &dyn Fn(f64, f64) -> bool,
) -> Vec<(f64, f64)> {
    let bbox_area = (bx1 - bx0).max(1.0) * (by1 - by0).max(1.0);
    // 网格数考虑两处损耗：簇状过滤 + 形状占 bbox 的面积比。
    // 过滤期望 E[wave^(2c)] ≈ 1−0.85c（经验拟合，保守取大以保证种子充足）。
    let accept_rate =
        (1.0 - 0.85 * profile.cluster).clamp(0.15, 1.0) * (shape_area / bbox_area).clamp(0.2, 1.0);
    let grid_cells = ((target_n as f64) / accept_rate).ceil().max(4.0);
    let step = (bbox_area / grid_cells).sqrt().max(effective_size);
    let cols = ((bx1 - bx0) / step).ceil().max(1.0) as usize;
    let rows = ((by1 - by0) / step).ceil().max(1.0) as usize;
    // 低频簇状调制随机相位（2 阶正弦密度场，无需 Perlin 库）。
    let phase_x = rng.range(0.0, std::f64::consts::TAU);
    let phase_y = rng.range(0.0, std::f64::consts::TAU);
    let freq = std::f64::consts::TAU / (bx1 - bx0).max(by1 - by0).max(32.0);
    let mut seeds = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let jx = (rng.next_f64() - 0.5) * profile.jitter;
            let jy = (rng.next_f64() - 0.5) * profile.jitter;
            let px = bx0 + (c as f64 + 0.5 + jx) * step;
            let py = by0 + (r as f64 + 0.5 + jy) * step;
            if !inside_shape(px, py) {
                continue;
            }
            // 低频簇状调制：石头/木纹的「疏可走马，密不透风」。
            if profile.cluster > 0.0 {
                let wave = 0.5 + 0.5 * ((px * freq + phase_x).sin() * (py * freq + phase_y).cos());
                let keep_prob = wave.max(1e-9).powf(2.0 * profile.cluster);
                if rng.next_f64() > keep_prob {
                    continue;
                }
            }
            seeds.push((px, py));
        }
    }
    seeds
}

/// 专家方案「二」：单笔肌理轨迹点（中观轨迹），返回 (x, y, pressure)。
///
/// 外层只生成符合材质结构力学的轨迹；微颗粒由 `scatter_texture` 在引擎层打散。
/// `is_block`：stone 的 20% 粗斑块模式（短促随机游走，靠 scatter=0.95 散成粗颗粒）。
/// 单笔肌理轨迹生成（中观轨迹；微颗粒由引擎层 scatter_texture 打散）。
#[allow(clippy::too_many_arguments)]
pub(super) fn build_texture_stroke_points(
    texture_type: &str,
    cx: f64,
    cy: f64,
    length: f64,
    main_angle: f64,
    knot_center: (f64, f64),
    idx: usize,
    is_block: bool,
    rng: &mut TextureXorShift,
    dir_field: Option<&DirectionField>,
) -> Vec<(f64, f64, f64)> {
    if is_block {
        let mut pts = Vec::with_capacity(5);
        let (mut x, mut y) = (cx, cy);
        let step = length / 4.0;
        for i in 0..5 {
            let u = i as f64 / 4.0;
            let p = 0.55 + 0.25 * (u * std::f64::consts::PI).sin();
            pts.push((x, y, p));
            let ang = rng.range(0.0, std::f64::consts::TAU);
            x += ang.cos() * step;
            y += ang.sin() * step;
        }
        return pts;
    }
    match texture_type {
        "canvas" => {
            // 正交断续经纬线：奇偶交替 0°/90°（±1.5° 微偏），两端收尖的梭形压感。
            let num_pts = 5usize;
            let mut pts = Vec::with_capacity(num_pts);
            let base_ang = if idx % 2 == 0 {
                0.0
            } else {
                std::f64::consts::FRAC_PI_2
            };
            let ang = base_ang + rng.gaussian() * 0.026;
            let (dx, dy) = (ang.cos(), ang.sin());
            for i in 0..num_pts {
                let t = (i as f64 / (num_pts - 1) as f64) - 0.5; // -0.5..0.5
                let p = 0.3 + 0.4 * (1.0 - (2.0 * t).abs()); // 0.3→0.7→0.3
                pts.push((cx + t * length * dx, cy + t * length * dy, p));
            }
            pts
        }
        "fabric" => {
            // 80% 主方向微弧斜纹 + 20% 垂直纬线暗纹，弓形弯曲，梭形压感。
            let num_pts = 5usize;
            let mut pts = Vec::with_capacity(num_pts);
            let base_ang = if idx % 5 == 0 {
                main_angle + std::f64::consts::FRAC_PI_2
            } else {
                main_angle
            };
            let ang = base_ang + rng.gaussian() * 0.08; // ≈ ±5°
            let (dx, dy) = (ang.cos(), ang.sin());
            let (nx, ny) = (dy, -dx);
            let bow = rng.gaussian() * length * 0.06;
            for i in 0..num_pts {
                let u = i as f64 / (num_pts - 1) as f64;
                let t = u - 0.5;
                let curve = (u * std::f64::consts::PI).sin() * bow;
                let p = 0.25 + 0.55 * (u * std::f64::consts::PI).sin(); // 0.25→0.8→0.25
                pts.push((
                    cx + t * length * dx + curve * nx,
                    cy + t * length * dy + curve * ny,
                    p,
                ));
            }
            pts
        }
        "skin" => {
            // 菱形皮纹漫游微弧线（±35° 双斜向交替 + 局部旋转），
            // 配合 scatter=0.90 在引擎层散化为毛孔簇。
            let num_pts = 5usize;
            let mut pts = Vec::with_capacity(num_pts);
            let base_ang = if idx % 2 == 0 { 0.6 } else { -0.6 } + rng.gaussian() * 0.4;
            let (mut x, mut y) = (cx, cy);
            let mut ang = base_ang;
            let step = length / num_pts as f64;
            for i in 0..num_pts {
                let u = i as f64 / (num_pts - 1) as f64;
                let p = 0.35 + 0.25 * (u * std::f64::consts::PI).sin();
                pts.push((x, y, p));
                ang += rng.gaussian() * 0.5;
                x += ang.cos() * step;
                y += ang.sin() * step;
            }
            pts
        }
        "wood" => {
            // 长流线 + 木节排斥场（Knot Deflection）：到 knot_center 的高斯排斥 + 正弦波。
            let num_wood_pts = 8usize;
            let mut pts = Vec::with_capacity(num_wood_pts);
            let (dx, dy) = (main_angle.cos(), main_angle.sin());
            let (nx, ny) = (-dy, dx);
            let (kx, ky) = knot_center;
            for i in 0..num_wood_pts {
                let u = i as f64 / (num_wood_pts - 1) as f64;
                let t = u - 0.5;
                let bx = cx + t * length * dx;
                let by = cy + t * length * dy;
                // 到木节的距离 → 透镜状绕流弯曲。
                let dist_sq = (bx - kx).powi(2) + (by - ky).powi(2);
                let sigma_sq = (length * 0.8).powi(2).max(100.0);
                let sign = if (bx - kx) * nx + (by - ky) * ny >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let deflect = sign * (length * 0.18) * (-dist_sq / sigma_sq).exp();
                let wave = (u * std::f64::consts::TAU + cx * 0.05).sin() * (length * 0.03);
                let p = 0.3 + 0.5 * (u * std::f64::consts::PI).sin();
                pts.push((bx + (deflect + wave) * nx, by + (deflect + wave) * ny, p));
            }
            pts
        }
        "fold" => {
            // ④ 衣褶：沿方向场的二阶流线积分（中点法），从中心向两端双向推进，
            // 两头正弦压感收尖（0.15→0.85→0.15）。无方向场时沿 main_angle 走直线。
            let angle_at = |x: f64, y: f64| -> f64 {
                dir_field
                    .map(|f| f.angle_at(x, y).to_radians())
                    .unwrap_or(main_angle)
            };
            let num_half = 4usize;
            let step = length / (2 * num_half) as f64;
            let mut fwd: Vec<(f64, f64)> = vec![(cx, cy)];
            let mut bwd: Vec<(f64, f64)> = Vec::with_capacity(num_half);
            let (mut fx, mut fy) = (cx, cy);
            let (mut bx, mut by) = (cx, cy);
            for _ in 0..num_half {
                // 前向中点法
                let a1 = angle_at(fx, fy);
                let mx = fx + a1.cos() * step / 2.0;
                let my = fy + a1.sin() * step / 2.0;
                let a2 = angle_at(mx, my);
                fx += a2.cos() * step;
                fy += a2.sin() * step;
                fwd.push((fx, fy));
                // 后向中点法
                let b1 = angle_at(bx, by);
                let mx2 = bx - b1.cos() * step / 2.0;
                let my2 = by - b1.sin() * step / 2.0;
                let b2 = angle_at(mx2, my2);
                bx -= b2.cos() * step;
                by -= b2.sin() * step;
                bwd.push((bx, by));
            }
            let mut all: Vec<(f64, f64)> = bwd.into_iter().rev().collect();
            all.extend(fwd);
            let n = all.len();
            let mut pts = Vec::with_capacity(n);
            for (i, (x, y)) in all.into_iter().enumerate() {
                let u = i as f64 / (n - 1).max(1) as f64;
                let p = 0.15 + 0.70 * (std::f64::consts::PI * u).sin();
                pts.push((x, y, p.clamp(0.0, 1.0)));
            }
            pts
        }
        _ /* "stone" */ => {
            // 折角碎裂短线：中途 1 次硬折弯（35°~70°），压感由重到轻如刀劈斧凿。
            let num_pts = 5usize;
            let mut pts = Vec::with_capacity(num_pts);
            let (mut x, mut y) = (cx, cy);
            let mut ang = rng.range(0.0, std::f64::consts::TAU);
            let step = length / (num_pts as f64 - 1.0);
            for i in 0..num_pts {
                let u = i as f64 / (num_pts - 1) as f64;
                let p = (0.85 * (1.0 - u * 0.8)).clamp(0.15, 0.95);
                pts.push((x, y, p));
                if i == 2 {
                    // 中途硬折角模拟石材脆性断裂。
                    let turn =
                        rng.range(0.6, 1.2) * if rng.next_f64() > 0.5 { 1.0 } else { -1.0 };
                    ang += turn;
                } else {
                    ang += rng.gaussian() * 0.15;
                }
                x += ang.cos() * step;
                y += ang.sin() * step;
            }
            pts
        }
    }
}

/// 点到线段距离（多边形内缩侵蚀用）。
pub(super) fn point_seg_dist(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> f64 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        return ((px - ax).powi(2) + (py - ay).powi(2)).sqrt();
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0);
    ((px - (ax + t * dx)).powi(2) + (py - (ay + t * dy)).powi(2)).sqrt()
}

/// 一笔已规划好的肌理笔触：规划（含全部随机性）与落笔 I/O 分离，
/// 使笔数上限 / 耗时 / 边界约束都能被单元测试直接断言。
pub(super) struct PlannedTextureStroke {
    pub(super) points: Vec<(f64, f64, f64)>, // (x, y, pressure)，已做形状内缩过滤
    pub(super) size: f64,
    pub(super) scatter: f64,
    pub(super) tremor: f64,
    pub(super) color: [f64; 4],
}

/// 专家方案「二」主循环：20% 骨相层 + 80% 肌理层的同预算双尺度拆分。
///
/// - Pass 1（前 20%）：底层骨相斑块，size×2.0、Alpha=base_alpha×0.45、scatter 0.90，
///   先铺一层若有若无的大尺度明暗斑驳（Mottled Base）；
/// - Pass 2（后 80%）：表层特征肌理，按材质特征表生成裂纹/经纬线/木纹/皮纹。
///
/// 边界保护：每笔轨迹点必须落在形状内缩 `cur_size` 的区域内（`inside_eroded`）。
/// 专家原码只过滤"顶点在形状内"，但 dab 本体有半径（size/2）且 `scatter_texture`
/// 会高斯偏移 dab 位置 —— 不内缩的话，长笔触的 dab 会系统性戳出边界。
/// 内缩一个完整 `cur_size` 是保守但简单的统一规则（判据 2）。
/// ④ fold 偶极子配色：暗笔 = 底色明度 + dark_l_shift、饱和度 +5%（防发灰）；
/// 亮笔 = 底色明度 + light_l_shift、Alpha = base_alpha × 0.85。
pub(super) fn fold_dipole_colors(
    base_rgb: [f64; 3],
    profile: TextureProfile,
    is_macro: bool,
) -> ([f64; 4], [f64; 4]) {
    let (h, s, l) = rgb_to_hsl(base_rgb[0], base_rgb[1], base_rgb[2]);
    let (dr, dg, db) = hsl_to_rgb(
        h,
        (s * 1.05).min(1.0),
        (l + profile.dark_l_shift).clamp(0.0, 1.0),
    );
    let (lr, lg, lb) = hsl_to_rgb(h, s, (l + profile.light_l_shift).clamp(0.0, 1.0));
    let a_scale = if is_macro { 0.45 } else { 1.0 };
    let dark_a = (profile.base_alpha * a_scale).min(1.0);
    let light_a = (profile.base_alpha * 0.85 * a_scale).min(1.0);
    ([dr, dg, db, dark_a], [lr, lg, lb, light_a])
}

#[allow(clippy::too_many_arguments)]
pub(super) fn plan_texture_strokes(
    rng: &mut TextureXorShift,
    texture: &str,
    profile: TextureProfile,
    bx0: f64,
    by0: f64,
    bx1: f64,
    by1: f64,
    shape_area: f64,
    size: f64, // effective_size（已做大区域自适应）
    target_n: usize,
    base_rgb: [f64; 3],
    inside_shape: &dyn Fn(f64, f64) -> bool,
    inside_eroded: &dyn Fn(f64, f64, f64) -> bool,
    dir_field: Option<&DirectionField>,
) -> Vec<PlannedTextureStroke> {
    let mut seeds = collect_texture_seeds(
        rng,
        profile,
        bx0,
        by0,
        bx1,
        by1,
        target_n,
        shape_area,
        size,
        inside_shape,
    );
    // 确定性洗牌后截断：消除行主序偏差（否则前 20% 骨相层会挤在顶部几行）。
    for i in (1..seeds.len()).rev() {
        let j = (rng.next_f64() * (i + 1) as f64) as usize;
        seeds.swap(i, j);
    }
    seeds.truncate(target_n);
    // wood 沿区域长轴；fabric 主方向默认 −30°（材质表）。
    let wood_angle = if (bx1 - bx0) >= (by1 - by0) {
        0.0
    } else {
        std::f64::consts::FRAC_PI_2
    };
    let fabric_angle = -30.0_f64.to_radians();
    let knot_center = (
        rng.range(bx0 + 0.2 * (bx1 - bx0), bx0 + 0.8 * (bx1 - bx0)),
        rng.range(by0 + 0.2 * (by1 - by0), by0 + 0.8 * (by1 - by0)),
    );
    let mut strokes = Vec::with_capacity(target_n);
    for (idx, &(cx, cy)) in seeds.iter().enumerate() {
        let is_macro = idx < (target_n / 5).max(1);
        let cur_size = if is_macro { size * 2.0 } else { size };
        let cur_len = cur_size
            * if is_macro {
                2.0
            } else {
                profile.len_mult * rng.range(0.75, 1.25)
            };
        let main_angle = if let Some(f) = dir_field {
            // ③ 方向场：种子点处的局部走向（弧度）。
            f.angle_at(cx, cy).to_radians()
        } else if texture == "wood" {
            wood_angle
        } else {
            fabric_angle
        };
        // stone 的 20% 粗斑块模式（len_mult 2.0、scatter 0.95）。
        let is_block = texture == "stone" && !is_macro && idx % 5 == 4;
        let (stroke_len, cur_scatter) = if is_block {
            (cur_size * 2.0, 0.95)
        } else if is_macro {
            (cur_len, 0.90)
        } else {
            (cur_len, profile.scatter)
        };
        let raw_pts = build_texture_stroke_points(
            texture,
            cx,
            cy,
            stroke_len,
            main_angle,
            knot_center,
            idx,
            is_block,
            rng,
            dir_field,
        );
        // ④ fold 偶极子：每次迭代连续发射「暗沟笔 + 法向偏移亮脊笔」两笔。
        if profile.pair_offset_mult > 0.0 && !is_block {
            let theta = main_angle;
            let (nx, ny) = (-theta.sin(), theta.cos());
            let (dx, dy) = (theta.cos(), theta.sin());
            let d = profile.pair_offset_mult * cur_size;
            let tang_shift = 0.15 * stroke_len;
            let (dark_c, light_c) = fold_dipole_colors(base_rgb, profile, is_macro);
            // 暗笔（褶沟）：中心在种子点，线宽 1.0×size。
            let dark_clipped: Vec<(f64, f64, f64)> = raw_pts
                .into_iter()
                .filter(|&(x, y, _)| inside_eroded(x, y, cur_size))
                .collect();
            if dark_clipped.len() >= 2 {
                strokes.push(PlannedTextureStroke {
                    points: dark_clipped,
                    size: cur_size,
                    scatter: cur_scatter,
                    tremor: profile.tremor,
                    color: dark_c,
                });
            }
            // 亮笔（褶脊）：法向偏移 d + 切向错开 0.15×length（避两端齐平），
            // 线宽 1.35×size（受光面更宽更柔）。
            let (lcx, lcy) = (cx + d * nx + tang_shift * dx, cy + d * ny + tang_shift * dy);
            let light_raw = build_texture_stroke_points(
                texture,
                lcx,
                lcy,
                stroke_len,
                main_angle,
                knot_center,
                idx,
                false,
                rng,
                dir_field,
            );
            let light_clipped: Vec<(f64, f64, f64)> = light_raw
                .into_iter()
                .filter(|&(x, y, _)| inside_eroded(x, y, cur_size * 1.35))
                .collect();
            if light_clipped.len() >= 2 {
                strokes.push(PlannedTextureStroke {
                    points: light_clipped,
                    size: cur_size * 1.35,
                    scatter: cur_scatter,
                    tremor: profile.tremor,
                    color: light_c,
                });
            }
            continue;
        }
        // 形状内缩过滤：顶点必须比边界内缩至少一个 cur_size，
        // 否则 dab 本体（半径 size/2）+ scatter 高斯偏移会系统性戳出边界（判据 2）。
        let erosion = cur_size;
        let clipped: Vec<(f64, f64, f64)> = raw_pts
            .into_iter()
            .filter(|&(x, y, _)| inside_eroded(x, y, erosion))
            .collect();
        if clipped.len() < 2 {
            continue;
        }
        // 凹凸双极配色：80% 暗部 / 20% 微高光（骨相层永不做高光）。
        let is_highlight = !is_macro && idx % 5 == 3;
        let color = derive_stroke_color(
            rng,
            base_rgb,
            texture,
            is_highlight,
            is_macro,
            profile.base_alpha,
        );
        strokes.push(PlannedTextureStroke {
            points: clipped,
            size: cur_size,
            scatter: cur_scatter,
            tremor: profile.tremor,
            color,
        });
    }
    strokes
}

/// `smart_texture`：智能肌理 —— 在区域内按材质类型自动生成肌理笔触。
///
/// 专家方案（五节）：
/// 1. 分布：分层抖动网格 + 低频簇状调制（O(N)，无拒绝采样死循环；只调
///    jitter/cluster 即通吃 5 种材质）；
/// 2. 轨迹：外层只生成中观轨迹（经纬线/斜纹/皮纹/碎裂线/木纹+木节），微颗粒
///    由 `scatter_texture` 在引擎层打散；
/// 3. 配色：`color` 缺省或 `"auto"` 时采样图层平均色；凹凸双极（80% 暗部 +
///    20% 微高光），Alpha 恒 ≤ base_alpha，绝不 1.0 实色；
/// 4. 笔数：N = clamp(round(Area·τ / (η·m·size²)), 8, 240)，大区域自动放大
///    有效尺寸，保证任何输入下规划可控；
/// 5. 双尺度：20% 骨相层（size×2.0、Alpha×0.45）+ 80% 肌理层，同预算不翻倍。
pub(super) fn write_smart_texture(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— 在选区预热之前 ✓。
    let layer_id = require_str(args, "layer_id")?;
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    // Q4：`clip_to_selection` 透传 + 缓存预热（失败时忽略，走慢路径）。
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();
    if !clip_sel.is_empty() {
        let _ = precompute_clip_mask(ctx, &clip_sel);
    }

    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let size = args
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or(6.0)
        .clamp(1.0, 40.0);
    let density = args
        .get("density")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let texture_type = optional_str(args, "texture_type").unwrap_or_else(|| "stone".to_owned());
    let texture = match texture_type.as_str() {
        "canvas" | "fabric" | "skin" | "stone" | "wood" | "fold" => texture_type.clone(),
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "texture_type 必须是 canvas/fabric/skin/stone/wood/fold，得到 {texture_type}"
                )),
            ))
        }
    };

    // --- 形状解析（含 holes，见 parse_smart_shape）---
    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_texture 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_texture")?;
    // Q4 早退：形状 bbox 与 clip 掩膜无交集时直接返回（不生成任何笔触，不提交原子）。
    if clip_bbox_early_out(
        ctx,
        &clip_sel,
        (
            parsed.bbox.0,
            parsed.bbox.1,
            parsed.bbox.2 - parsed.bbox.0,
            parsed.bbox.3 - parsed.bbox.1,
        ),
    ) {
        return Ok(json!({
            "ok": true,
            "clipped_out": true,
            "note": "形状包围盒与 clip_to_selection 无交集，未生成笔触",
        }));
    }
    // polygon 仍要求 points（与旧行为一致：空多边形报错）。
    if let PrimShape::Polygon { points } = &parsed.outer {
        if points.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("polygon 需要 points".to_owned()),
            ));
        }
    }
    let (bx0, by0, bx1, by1): (f64, f64, f64, f64) = parsed.bbox;
    // 点是否在形状内（含孔洞扣除）。
    let inside_shape = |px: f64, py: f64| -> bool { parsed.contains(px, py) };
    // 点是否在形状内缩 e 的区域内（判据 2 的边界保护；孔洞不内缩，只做扣除）。
    // polygon 用点到各边距离 ≥ e 近似侵蚀。
    let inside_eroded = |px: f64, py: f64, e: f64| -> bool {
        if !parsed.outer.contains(px, py) {
            return false;
        }
        if parsed.holes.iter().any(|h| h.contains(px, py)) {
            return false;
        }
        match &parsed.outer {
            PrimShape::Polygon { points } => {
                let n = points.len();
                for i in 0..n {
                    let (ax, ay) = points[i];
                    let (bx, by) = points[(i + 1) % n];
                    if point_seg_dist(px, py, ax, ay, bx, by) < e {
                        return false;
                    }
                }
                true
            }
            PrimShape::Ellipse { cx, cy, rx, ry } => {
                // 圆/椭圆：沿径向内缩 e（椭圆用 rx,ry 分别缩）。
                let rxe = rx - e;
                let rye = ry - e;
                if rxe <= 0.0 || rye <= 0.0 {
                    return false;
                }
                let dx = (px - cx) / rxe;
                let dy = (py - cy) / rye;
                dx * dx + dy * dy <= 1.0
            }
            PrimShape::Rect { x, y, w, h } => {
                px >= x + e && px <= x + w - e && py >= y + e && py <= y + h - e
            }
            PrimShape::Path { rings, .. } => {
                // 与 polygon 同款近似：到任一环任一边距离 ≥ e（环按闭合折线）。
                for ring in rings {
                    let n = ring.len();
                    if n < 2 {
                        continue;
                    }
                    for i in 0..n {
                        let (ax, ay) = ring[i];
                        let (bx, by) = ring[(i + 1) % n];
                        if point_seg_dist(px, py, ax, ay, bx, by) < e {
                            return false;
                        }
                    }
                }
                true
            }
        }
    };

    // 形状面积（N 公式与网格损耗补偿用；孔洞面积已扣除）。
    let shape_area: f64 = parsed.area();

    // --- 基准色：color 参数或自动采样 ---
    let mut base_rgb: [f64; 3] = match args.get("color") {
        Some(Value::String(s)) if s == "auto" => {
            sample_layer_average(ctx, &layer_id, bx0, by0, bx1, by1)?
        }
        Some(Value::Array(arr)) if arr.len() >= 3 => [
            arr[0].as_f64().unwrap_or(0.5),
            arr[1].as_f64().unwrap_or(0.5),
            arr[2].as_f64().unwrap_or(0.5),
        ],
        Some(_) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("color 应为 [r,g,b] 数组或 \"auto\"".to_owned()),
            ))
        }
        None => sample_layer_average(ctx, &layer_id, bx0, by0, bx1, by1)?,
    };
    // **`neutral`（opt-in）** ✓：把基色乘 0.5 ⇒ **均值基本不变** ✓（第 1382 轮）
    //
    // **⇒ 为什么需要它** ✗：实测（第 1380 轮）肌理**会让画面上移 +25**，而且
    //   **用底的同一颜色**上肌理也是 +25 ⇒ 所以「提亮」来自**混合本身** ✗
    //   ⇒ 而实测**零点在「底色 × 0.5」**（底 0.502 ⇒ color 0.25 时 Δ=+0.7）✓
    // **⇒ 默认关闭** ⇒ **现有行为完全不变** ✓（有人依赖当前观感）
    // **⇒ 开它** ⇒ 只把**基色**减半 ⇒ 笔数/走向/尺寸都不变 ✓
    if args
        .get("neutral")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        // **⇒ 「均值不变」的肌理色与底色的比值随底色变化**（第 1382 轮实测 5 档）：
        //   g 0.235 ⇒ **0.15**｜g 0.353 ⇒ **0.31**｜g 0.502 ⇒ **0.50**｜g 0.667 ⇒ **0.70**｜g 0.824 ⇒ **0.87**
        //   ⇒ **所以固定乘 0.5 只在 g≈0.5 准** ✗（实测 g=0.235 时 Δ=+9.8、g=0.824 时 Δ=−10.6）
        // **⇒ 所以用**查表 ＋ 线性插值**（表外取端点）⇒ 这是**实测表**，不是推导公式 ✓
        const NEUTRAL_RATIOS: [(f64, f64); 5] = [
            (0.235, 0.15),
            (0.353, 0.31),
            (0.502, 0.50),
            (0.667, 0.70),
            (0.824, 0.87),
        ];
        let g = (base_rgb[0] + base_rgb[1] + base_rgb[2]) / 3.0;
        let last = NEUTRAL_RATIOS.len() - 1;
        let ratio = if g <= NEUTRAL_RATIOS[0].0 {
            NEUTRAL_RATIOS[0].1
        } else if g >= NEUTRAL_RATIOS[last].0 {
            NEUTRAL_RATIOS[last].1
        } else {
            let mut r = NEUTRAL_RATIOS[last].1;
            for w in NEUTRAL_RATIOS.windows(2) {
                let (g0, r0) = w[0];
                let (g1, r1) = w[1];
                if g >= g0 && g <= g1 {
                    r = r0 + (r1 - r0) * ((g - g0) / (g1 - g0));
                    break;
                }
            }
            r
        };
        base_rgb = [
            base_rgb[0] * ratio,
            base_rgb[1] * ratio,
            base_rgb[2] * ratio,
        ];
    }

    // --- 笔数与有效尺寸（专家方案「一、4」）---
    let profile = get_texture_profile(&texture);
    let (target_n, effective_size) = texture_target_strokes(shape_area, density, size, profile);

    // --- PRNG（确定性：bbox 哈希 + 黄金分割）---
    let seed = ((bx0 * 73856093.0 + by0 * 19349663.0 + bx1 * 83492791.0 + by1 * 29712150.0) as u64)
        .wrapping_add(0x9E3779B97F4A7C15);
    let mut rng = TextureXorShift(seed);

    // --- 方向场（③）：direction_guides / direction_from；缺省走材质默认走向 ---
    let dir_field_opt: Option<DirectionField> =
        if args.get("direction_guides").is_some() || args.get("direction_from").is_some() {
            Some(parse_direction_field(args, &parsed, 0.0).0)
        } else {
            None
        };

    // --- 规划全部笔触（20% 骨相层 + 80% 肌理层；规划与落笔分离）---
    let plan = plan_texture_strokes(
        &mut rng,
        &texture,
        profile,
        bx0,
        by0,
        bx1,
        by1,
        shape_area,
        effective_size,
        target_n,
        base_rgb,
        &inside_shape,
        &inside_eroded,
        dir_field_opt.as_ref(),
    );

    // --- 逐笔提交 ---
    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;
    for stroke in &plan {
        // 时间：按累计弧长给恒定速度（400 px/s），保证有速度信号（无 time_ms 会 warning）。
        let mut cum_dists: Vec<f64> = Vec::with_capacity(stroke.points.len());
        let mut acc = 0.0;
        let mut prev: Option<(f64, f64)> = None;
        for (x, y, _) in &stroke.points {
            if let Some((px, py)) = prev {
                acc += ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            }
            cum_dists.push(acc);
            prev = Some((*x, *y));
        }
        let points: Vec<Value> = stroke
            .points
            .iter()
            .zip(cum_dists.iter())
            .map(|((x, y, p), d)| {
                json!([x.round(), y.round(), p.clamp(0.0, 1.0), d / 400.0 * 1000.0])
            })
            .collect();
        let c = stroke.color;
        // **P2-6**：内层是否等渲染已在函数入口由 `set_inner_wait_for_render` 定好，
        //   每笔重读 `args` 是冗余的（`ctx` 随本次调用丢弃，值不会漏到别的调用）。
        let stroke_args = json!({
            "layer_id": layer_id,
            "brush": brush_name,
            "points": points,
            "size": stroke.size,
            "color": [c[0], c[1], c[2], c[3]],
            "scatter_texture": stroke.scatter,
            "hand_tremor": stroke.tremor,
            "color_breath": 0.1,
            "preview": false,
            "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
        });
        match write_brush_stroke(ctx, &stroke_args) {
            Ok(_) => ok_count += 1,
            Err(e) => {
                fail_count += 1;
                if first_error.is_none() {
                    first_error = Some(format!("{e:?}"));
                }
            }
        }
    }

    let mut value = json!({
        "ok": fail_count == 0,
        "planned": plan.len(),
        "strokes": ok_count,
        "failed": fail_count,
        "texture_type": texture,
        "bbox": [bx0, by0, bx1, by1],
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_texture 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }
    Ok(value)
}

/// ① smart_sample 的 OKLab 换算（独立函数，供双线性色彩场用）。
#[inline]
pub(super) fn srgb_u8_to_oklab(r: u8, g: u8, b: u8) -> [f32; 3] {
    #[inline]
    pub(super) fn to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    let r = to_linear(r as f32 / 255.0);
    let g = to_linear(g as f32 / 255.0);
    let b = to_linear(b as f32 / 255.0);
    let l = (0.41222147 * r + 0.53633255 * g + 0.05144599 * b).cbrt();
    let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
    let s = (0.08830246 * r + 0.28171885 * g + 0.6299787 * b).cbrt();
    [
        0.21045426 * l + 0.7936178 * m - 0.004072047 * s,
        1.9779985 * l - 2.4285922 * m + 0.4505937 * s,
        0.025904037 * l + 0.78277177 * m - 0.80867577 * s,
    ]
}

/// OKLab → sRGB（0..1）。
pub(super) fn oklab_to_srgb_f32(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;
    let r = 4.0767417 * l3 - 3.3077116 * m3 + 0.23096993 * s3;
    let g = -1.268438 * l3 + 2.6097574 * m3 - 0.3413194 * s3;
    let bl = -0.00419609 * l3 - 0.7034186 * m3 + 1.7076147 * s3;
    #[inline]
    pub(super) fn to_srgb(c: f32) -> f32 {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }
    [to_srgb(r), to_srgb(g), to_srgb(bl)]
}

/// ① smart_sample：S×S 连续双线性 OKLab 色彩场 + 等明度切线场。
///
/// S 只控制色彩场分辨率，**不等于笔数**（解耦「采样密度」与「笔触数量」，
/// 避免退回 2738 笔的手工点画）。
pub(super) struct BilinearOklabField {
    /// 网格阶数。
    pub(super) s: usize,
    /// S×S 格点 OKLab（行主序）。
    pub(super) cells: Vec<[f32; 3]>,
    /// S×S 等明度切向角（度）。
    pub(super) grad_angle: Vec<f32>,
    /// S×S 明度梯度模。
    pub(super) grad_mag: Vec<f32>,
    /// 采样区域 `(x0, y0, x1, y1)`。
    pub(super) bbox: (f64, f64, f64, f64),
}

/// **Catmull-Rom 四个权重**（C¹ 连续）✓（第 1443 轮）
///
/// **⇒ 为什么** ✗：双线性是**分段线性** ⇒ 采样线处一阶导跳变 ⇒ **折痕/竖纹** ✗
///   （外部 `E1408` 的机制诊断正确 ✓）
/// **⇒ ⇒ 只在 `sample_lab` 上换** ✓ —— `sample_at` 内部就调它，rgb 只走这一条路 ✓
/// **⇒ ⇒ ⇒ 代价**：4 点 → **16 点/次**，且会**过冲** ⇒ 索引夹紧 + 调用处夹到合法区间 ✓
fn catmull_rom_weights(t: f32) -> [f32; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}

impl BilinearOklabField {
    /// 从 RGBA8 像素构建。透明像素（a<16）不参与平均。
    pub(super) fn build(
        pixels: &[u8],
        w: usize,
        h: usize,
        s: usize,
        bbox: (f64, f64, f64, f64),
    ) -> Self {
        let s = s.clamp(4, 24);
        let mut cells = vec![[0.0f32; 3]; s * s];
        let mut counts = vec![0u32; s * s];
        for (idx, px) in pixels.chunks_exact(4).enumerate() {
            if px[3] < 16 {
                continue;
            }
            let gx = idx % w.max(1);
            let gy = idx / w.max(1);
            let ci = (gx * s / w.max(1)).min(s - 1);
            let cj = (gy * s / h.max(1)).min(s - 1);
            let lab = srgb_u8_to_oklab(px[0], px[1], px[2]);
            let c = &mut cells[cj * s + ci];
            c[0] += lab[0];
            c[1] += lab[1];
            c[2] += lab[2];
            counts[cj * s + ci] += 1;
        }
        // 平均；空格点用全局平均填充（避免黑洞）。
        let mut total = [0f32; 3];
        let mut tn = 0u32;
        for i in 0..s * s {
            if counts[i] > 0 {
                total[0] += cells[i][0];
                total[1] += cells[i][1];
                total[2] += cells[i][2];
                tn += counts[i];
            }
        }
        let gavg = if tn > 0 {
            [
                total[0] / tn as f32,
                total[1] / tn as f32,
                total[2] / tn as f32,
            ]
        } else {
            [0.5, 0.0, 0.0]
        };
        for i in 0..s * s {
            if counts[i] > 0 {
                let n = counts[i] as f32;
                cells[i][0] /= n;
                cells[i][1] /= n;
                cells[i][2] /= n;
            } else {
                cells[i] = gavg;
            }
        }
        // 等明度切向角 + 梯度模：中心差分求 ∇L，旋转 90°。
        let mut grad_angle = vec![0f32; s * s];
        let mut grad_mag = vec![0f32; s * s];
        for j in 0..s {
            for i in 0..s {
                let xm = cells[j * s + i.saturating_sub(1)][0];
                let xp = cells[j * s + (i + 1).min(s - 1)][0];
                let ym = cells[j.saturating_sub(1) * s + i][0];
                let yp = cells[(j + 1).min(s - 1) * s + i][0];
                let dx = (xp - xm) / 2.0;
                let dy = (yp - ym) / 2.0;
                grad_angle[j * s + i] = dx.atan2(-dy).to_degrees();
                grad_mag[j * s + i] = (dx * dx + dy * dy).sqrt();
            }
        }
        Self {
            s,
            cells,
            grad_angle,
            grad_mag,
            bbox,
        }
    }

    /// 网格双线性插值（标量场用）。
    pub(super) fn bilerp_field(&self, field: &[f32], u: f64, v: f64) -> f32 {
        let s = self.s;
        let fx = (u.clamp(0.0, 1.0) * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let fy = (v.clamp(0.0, 1.0) * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(s - 1);
        let y1 = (y0 + 1).min(s - 1);
        let tx = (fx - x0 as f64) as f32;
        let ty = (fy - y0 as f64) as f32;
        let a = field[y0 * s + x0];
        let b = field[y0 * s + x1];
        let c = field[y1 * s + x0];
        let d = field[y1 * s + x1];
        a * (1.0 - tx) * (1.0 - ty) + b * tx * (1.0 - ty) + c * (1.0 - tx) * ty + d * tx * ty
    }

    /// 归一化坐标 (u,v)∈[0,1]² 处的 OKLab 双线性采样。
    pub(super) fn sample_lab(&self, u: f64, v: f64) -> [f32; 3] {
        let s = self.s;
        let fx = (u.clamp(0.0, 1.0) * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let fy = (v.clamp(0.0, 1.0) * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let tx = (fx - x0 as f64) as f32;
        let ty = (fy - y0 as f64) as f32;
        let mut out = [0f32; 3];
        let wx = catmull_rom_weights(tx);
        let wy = catmull_rom_weights(ty);
        for (k, slot) in out.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for (j, wyj) in wy.iter().enumerate() {
                let yy = (y0 as isize + j as isize - 1).clamp(0, s as isize - 1) as usize;
                let mut row = 0.0f32;
                for (i, wxi) in wx.iter().enumerate() {
                    let xx = (x0 as isize + i as isize - 1).clamp(0, s as isize - 1) as usize;
                    row += self.cells[yy * s + xx][k] * wxi;
                }
                acc += row * wyj;
            }
            *slot = acc;
        }
        out
    }

    /// 文档坐标 → 归一化场坐标。
    pub(super) fn to_uv(&self, x: f64, y: f64) -> (f64, f64) {
        let (bx0, by0, bx1, by1) = self.bbox;
        let u = (x - bx0) / (bx1 - bx0).max(1e-9);
        let v = (y - by0) / (by1 - by0).max(1e-9);
        (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
    }

    /// 文档坐标处的采样色（sRGB 0..1）与等明度角（度）、梯度模。
    pub(super) fn sample_at(&self, x: f64, y: f64) -> ([f32; 3], f64, f32) {
        let (u, v) = self.to_uv(x, y);
        let lab = self.sample_lab(u, v);
        let rgb = oklab_to_srgb_f32(lab[0], lab[1], lab[2]);
        // 角度场用单位向量双线性（防 355°/5°  wrapping）。
        let s = self.s;
        let fx = (u * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let fy = (v * (s - 1) as f64).clamp(0.0, (s - 1) as f64);
        let x0 = fx.floor() as usize;
        let y0 = fy.floor() as usize;
        let x1 = (x0 + 1).min(s - 1);
        let y1 = (y0 + 1).min(s - 1);
        let tx = (fx - x0 as f64) as f32;
        let ty = (fy - y0 as f64) as f32;
        let mut vx = 0f32;
        let mut vy = 0f32;
        for (gx, gy, w) in [
            (x0, y0, (1.0 - tx) * (1.0 - ty)),
            (x1, y0, tx * (1.0 - ty)),
            (x0, y1, (1.0 - tx) * ty),
            (x1, y1, tx * ty),
        ] {
            let rad = self.grad_angle[gy * s + gx].to_radians();
            vx += w * rad.cos();
            vy += w * rad.sin();
        }
        let ang = if vx.abs() < 1e-9 && vy.abs() < 1e-9 {
            0.0
        } else {
            vy.atan2(vx).to_degrees() as f64
        };
        let mag = self.bilerp_field(&self.grad_mag, u, v);
        (rgb, ang, mag)
    }
}

/// 读取参考源的 RGBA8 像素。
/// `ref_src`：文档内参考图层 `layer_id`，或 `blob_hash`（`sha256:…`），或 PNG 文件路径。
/// **把整幅 RGBA 裁到 `region`** ✓（第 1422 轮；外部 §213）
///
/// **⇒ 为什么需要** ✗：`read_reference_rgba` 的**图层**分支走 `render_region_raw_layer`
///   ⇒ 天然只拿 `region` ✓；而 **blob 与文件**分支原来 `decode_png` 后**直接整幅返回**
///   ⇒ **`ref_bbox` 对它们完全无效** ✗✗
///   **⇒ 实测**（外部）：同一 shape、`ref_bbox` 给**像素**与给**归一化**两种写法
///     ⇒ 结果**逐字节一致**（164.7 / 1.02 / 0.62 完全相同）⇒ 那就说明它被忽略了 ✓
/// **⇒ ⇒ 所以**在这里裁**，让三条参考来路**行为一致** ✓
fn crop_rgba_to_region(
    w: u32,
    h: u32,
    px: Vec<u8>,
    region: yanshi_core::Bbox,
) -> (u32, u32, Vec<u8>) {
    let x0 = region.x.floor().max(0.0) as u32;
    let y0 = region.y.floor().max(0.0) as u32;
    let x1 = (region.x + region.w).ceil().max(0.0) as u32;
    let y1 = (region.y + region.h).ceil().max(0.0) as u32;
    let x1 = x1.min(w);
    let y1 = y1.min(h);
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, Vec::new());
    }
    if x0 == 0 && y0 == 0 && x1 == w && y1 == h {
        return (w, h, px);
    }
    let (cw, ch) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..y1 {
        let start = ((y * w + x0) * 4) as usize;
        let end = start + (cw * 4) as usize;
        if end <= px.len() {
            out.extend_from_slice(&px[start..end]);
        }
    }
    (cw, ch, out)
}

pub(super) fn read_reference_rgba(
    ctx: &mut ToolContext<'_>,
    ref_src: &str,
    region: yanshi_core::Bbox,
) -> Result<(u32, u32, Vec<u8>)> {
    // 1. 文档内图层（即使 visible=false 也能读出像素）。
    {
        let state = document_state(ctx)?;
        if state.layers.contains_key(ref_src) {
            return ctx
                .workspace
                .render_region_raw_layer(&ctx.doc_id, region, ref_src);
        }
    }
    // 2. blob_hash。
    if ref_src.starts_with("sha256:") {
        if let Ok(hash) = ref_src.parse::<yanshi_core::BlobHash>() {
            if let Ok(bytes) = ctx.workspace.store().get(&hash) {
                if let Some((w, h, px)) = yanshi_render::png::decode_png(&bytes) {
                    // **按 `region` 裁剪** ✓（第 1422 轮）：blob 是**整幅图** ⇒ 不裁就等于忽略 `ref_bbox` ✗
                    return Ok(crop_rgba_to_region(w, h, px, region));
                }
            }
        }
    }
    // 3. 文件路径（PNG）。
    let bytes = std::fs::read(ref_src).map_err(|e| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "「ref」既不是图层也不是 blob_hash，文件也读取失败：{e}"
            )),
        )
    })?;
    // **同样按 `region` 裁剪** ✓（第 1422 轮）
    yanshi_render::png::decode_png(&bytes)
        .map(|(w, h, px)| crop_rgba_to_region(w, h, px, region))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("「ref」文件不是有效 PNG".to_owned()),
            )
        })
}

/// ① `smart_sample`：从参考图/参考图层采样区域内的明暗与色彩分布，
/// 自动生成顺应等明度线的多尺度笔触。
///
/// 两阶段（总笔数 35~160，拒绝退回 2738 笔手工点画）：
/// - Pass 1：大笔软边铺底（约 2S 笔），单笔跨越 ΔL>0.06 时按弧长拆 2~3 段；
/// - Pass 2：梯度加权特征肌理笔触（40~120 笔），落笔概率 ∝ ‖∇L‖，方向取等明度切向。
pub(super) fn write_smart_sample(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）。
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    let ref_src = require_str(args, "ref")?;
    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let samples = args
        .get("samples")
        .and_then(Value::as_u64)
        .unwrap_or(8)
        .clamp(4, 24) as usize;
    let density = args
        .get("density")
        .and_then(Value::as_f64)
        .unwrap_or(0.6)
        .clamp(0.0, 1.0);
    let texture_type = optional_str(args, "texture_type").unwrap_or_else(|| "fabric".to_owned());
    // texture_type 校验：smart_texture 的 6 种 + smooth（柔笔）。
    let texture_for_profile = match texture_type.as_str() {
        "canvas" | "fabric" | "skin" | "stone" | "wood" | "fold" => texture_type.clone(),
        "smooth" => "smooth".to_owned(),
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "texture_type 必须是 smooth/fabric/fold/canvas/skin/stone/wood，得到 {texture_type}"
                )),
            ))
        }
    };

    // --- 形状解析（含 holes）---
    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_sample 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_sample")?;
    let (bx0, by0, bx1, by1) = parsed.bbox;
    let sw = (bx1 - bx0).max(1.0);
    let sh = (by1 - by0).max(1.0);

    // --- 参考采样框（缺省与 shape bbox 同尺寸对齐）---
    let (rx0, ry0, rx1, ry1) = match args.get("ref_bbox").and_then(Value::as_array) {
        Some(arr) if arr.len() >= 4 => {
            let x = arr[0].as_f64().unwrap_or(bx0);
            let y = arr[1].as_f64().unwrap_or(by0);
            let w = arr[2].as_f64().unwrap_or(sw).max(1.0);
            let h = arr[3].as_f64().unwrap_or(sh).max(1.0);
            (x, y, x + w, y + h)
        }
        _ => (bx0, by0, bx1, by1),
    };
    // 裁剪到画布（读图层像素用）。
    let (doc_w, doc_h) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|d| (d.state().width as f64, d.state().height as f64))
        .unwrap_or((4096.0, 4096.0));
    let crx0 = rx0.clamp(0.0, doc_w);
    let cry0 = ry0.clamp(0.0, doc_h);
    let crx1 = rx1.clamp(0.0, doc_w).max(crx0 + 1.0);
    let cry1 = ry1.clamp(0.0, doc_h).max(cry0 + 1.0);

    // --- 读参考像素并建场 ---
    let region = yanshi_core::Bbox::new(crx0, cry0, crx1, cry1);
    let (rw, rh, rpixels) = read_reference_rgba(ctx, &ref_src, region)?;
    if rw == 0 || rh == 0 || rpixels.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("参考源无像素".to_owned()),
        ));
    }
    let field = BilinearOklabField::build(
        &rpixels,
        rw as usize,
        rh as usize,
        samples,
        (crx0, cry0, crx1, cry1),
    );
    // sRGB(0..1) → OKLab 明度 L（Pass 1 拆段判据用）。
    let srgb_f32_l = |r: f32, g: f32, b: f32| -> f32 {
        let to_linear = |c: f32| -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let r = to_linear(r);
        let g = to_linear(g);
        let b = to_linear(b);
        let l = (0.41222147 * r + 0.53633255 * g + 0.05144599 * b).cbrt();
        let m = (0.2119035 * r + 0.6806995 * g + 0.10739696 * b).cbrt();
        let s = (0.08830246 * r + 0.28171884 * g + 0.6299787 * b).cbrt();
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s
    };
    // 目标点 (x,y) 的参考采样（经 ref_bbox→shape bbox 映射）。
    let sample_ref = |x: f64, y: f64| -> ([f32; 3], f64, f32) {
        let u = ((x - bx0) / sw).clamp(0.0, 1.0);
        let v = ((y - by0) / sh).clamp(0.0, 1.0);
        let fx = crx0 + u * (crx1 - crx0);
        let fy = cry0 + v * (cry1 - cry0);
        field.sample_at(fx, fy)
    };

    let size = args
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or((sw.min(sh) / 20.0).clamp(4.0, 30.0))
        .clamp(1.0, 60.0);
    let profile = if texture_for_profile == "smooth" {
        TextureProfile {
            len_mult: 6.0,
            jitter: 0.15,
            cluster: 0.0,
            scatter: 0.25,
            tremor: 0.01,
            base_alpha: 0.30,
            pair_offset_mult: 0.0,
            dark_l_shift: 0.0,
            light_l_shift: 0.0,
        }
    } else {
        get_texture_profile(&texture_for_profile)
    };
    let seed = ((bx0 * 73856093.0 + by0 * 19349663.0 + bx1 * 83492791.0) as u64)
        .wrapping_add(0x9E3779B97F4A7C15);
    let mut rng = TextureXorShift(seed.max(1));
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;
    // 落笔辅助：points 必须带 time_ms（恒定速度 400px/s）。
    let emit = |ctx: &mut ToolContext<'_>,
                pts: &[(f64, f64, f64)],
                size: f64,
                color: [f32; 4],
                hardness: f64,
                ok_count: &mut usize,
                fail_count: &mut usize,
                first_error: &mut Option<String>| {
        let mut acc = 0.0;
        let mut prev: Option<(f64, f64)> = None;
        let points: Vec<Value> = pts
            .iter()
            .map(|(x, y, p)| {
                if let Some((px, py)) = prev {
                    acc += ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                }
                prev = Some((*x, *y));
                json!([
                    x.round(),
                    y.round(),
                    p.clamp(0.0, 1.0),
                    acc / 400.0 * 1000.0
                ])
            })
            .collect();
        if points.len() < 2 {
            return;
        }
        let stroke_args = json!({
            "layer_id": layer_id,
            "brush": brush_name,
            "points": points,
            "size": size,
            "color": [color[0], color[1], color[2], color[3]],
            "hardness": hardness,
            "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
            "preview": false,
        });
        match write_brush_stroke(ctx, &stroke_args) {
            Ok(_) => *ok_count += 1,
            Err(e) => {
                *fail_count += 1;
                if first_error.is_none() {
                    *first_error = Some(format!("{e:?}"));
                }
            }
        }
    };

    // === Pass 1：大笔软边铺底（约 2S 笔）===
    //
    // **★★★ 方向取**等明度切向**（第 1423 轮；外部 §213）★★★**
    //
    // **⇒ 原缺陷** ✗：这里原本只有一行 `let horizontal = sw >= sh;` ⇒ **方向只由形状长宽比决定**，
    //   **与参考图毫无关系** ✗✗ ⇒ 于是**高位画布（如 600×1066）上永远是竖条** ✓
    //   **⇒ ⇒ 而 summary 明确承诺**「自动生成**顺应等明度线**的多尺度笔触…**方向取等明度切向**」✗✗✗
    //     ⇒ **文档对、代码错**（与本场 `fill_region.opacity`、掩膜遍时间基同一类 ✓）
    //   **⇒ ⇒ ⇒ 而外部用三组独立实验（画布差 3.3 倍、`ref_bbox` 三种写法、`samples/size` 三组）
    //     都得到竖条** ⇒ 他排除掉参数之后**正确地**判定是方向场的问题 ✓（他的判断对 ✓）
    //
    // **⇒ 现在怎么做**：每个笔触在**自己的中心点**取参考图的**有限差分梯度**（ΔL/Δx、ΔL/Δy）⇒
    //   **等明度切向 = 梯度旋转 90°** ⇒ `(-gy, gx)` ✓
    //   **⇒ 梯度≈0（平坦区）⇒ 回退到长轴方向**（确定性、可复现 ✓，且平坦区本来就没有"等明度线"可言 ✓）
    let long_axis_horizontal = sw >= sh;
    let iso_tangent = |cx: f64, cy: f64| -> Option<(f64, f64)> {
        // 1 px 的有限差分（与参考采样同坐标系 = 文档坐标 ✓）
        let e = 1.0;
        let l = |c: [f32; 3]| srgb_f32_l(c[0], c[1], c[2]);
        let (ca, _, _) = sample_ref(cx - e, cy);
        let (cb, _, _) = sample_ref(cx + e, cy);
        let (cc, _, _) = sample_ref(cx, cy - e);
        let (cd, _, _) = sample_ref(cx, cy + e);
        let span = 2.0 * e; // f64
        let gx = f64::from(l(cb) - l(ca)) / span;
        let gy = f64::from(l(cd) - l(cc)) / span;
        let mag = (gx * gx + gy * gy).sqrt();
        // 平均明度差 < 0.5% ⇒ 视为平坦（噪声级），不用它定方向 ✓
        if mag < 0.005 {
            return None;
        }
        Some((-gy / mag, gx / mag))
    };
    let size_base = sw.min(sh) / (samples as f64 * 0.65);
    let n1 = (2 * samples).clamp(8, 48);
    for i in 0..n1 {
        // 沿长轴的一条长笔触，位置在短轴上均匀铺开（+ 抖动）。
        let t = (i as f64 + 0.5) / n1 as f64;
        let jitter = rng.gaussian() * 0.02;
        let tc = (t + jitter).clamp(0.0, 1.0);
        // 长轴方向的基线（平坦区的回退 ✓）
        let (lx0, ly0, lx1, ly1) = if long_axis_horizontal {
            (bx0, by0 + tc * sh, bx1, by0 + tc * sh)
        } else {
            (bx0 + tc * sw, by0, bx0 + tc * sw, by1)
        };
        // **有梯度 ⇒ 沿等明度切向**（长度取长轴的 80%，中心不变 ✓）
        let (cx0, cy0) = ((lx0 + lx1) / 2.0, (ly0 + ly1) / 2.0);
        let (x0s, y0s, x1s, y1s) = match iso_tangent(cx0, cy0) {
            Some((tx, ty)) => {
                let half = 0.4 * (sw.max(sh));
                let (ax, ay) = (cx0 + tx * half, cy0 + ty * half);
                let (bx_, by_) = (cx0 - tx * half, cy0 - ty * half);
                // 裁到形状 bbox 内（贴着边缘走时不许把笔甩到外面 ✗）
                (
                    ax.clamp(bx0, bx1),
                    ay.clamp(by0, by1),
                    bx_.clamp(bx0, bx1),
                    by_.clamp(by0, by1),
                )
            }
            None => (lx0, ly0, lx1, ly1),
        };
        // 起止点明度差 → 决定拆段数。
        let (c0, _, _) = sample_ref(x0s, y0s);
        let (c1, _, _) = sample_ref(x1s, y1s);
        let dl = (srgb_f32_l(c1[0], c1[1], c1[2]) - srgb_f32_l(c0[0], c0[1], c0[2])).abs();
        let nseg = if dl > 0.12 {
            3
        } else if dl > 0.06 {
            2
        } else {
            1
        };
        for s in 0..nseg {
            let u0 = s as f64 / nseg as f64;
            let u1 = (s + 1) as f64 / nseg as f64;
            let um = (u0 + u1) / 2.0;
            let mx = x0s + (x1s - x0s) * um;
            let my = y0s + (y1s - y0s) * um;
            // 孔洞内的段直接跳过。
            if !parsed.contains(mx, my) {
                continue;
            }
            let (rgb, _, _) = sample_ref(mx, my);
            let npts = 8usize;
            let mut pts = Vec::with_capacity(npts);
            for k in 0..npts {
                let u = u0 + (u1 - u0) * k as f64 / (npts - 1) as f64;
                let px = x0s + (x1s - x0s) * u;
                let py = y0s + (y1s - y0s) * u;
                let uu = k as f64 / (npts - 1) as f64;
                let p = 0.25 + 0.55 * (std::f64::consts::PI * uu).sin();
                pts.push((px, py, p));
            }
            emit(
                ctx,
                &pts,
                size_base,
                [rgb[0], rgb[1], rgb[2], 0.55],
                0.25, // softness 0.75 → hardness 0.25
                &mut ok_count,
                &mut fail_count,
                &mut first_error,
            );
        }
    }

    // === Pass 2：梯度加权特征肌理笔触（40~120 笔）===
    // 梯度模归一化（取场内最大值）。
    let max_mag = field.grad_mag.iter().fold(0f32, |a, &b| a.max(b)).max(1e-6);
    let target2 = (40.0 + 80.0 * density) as usize;
    let mut made2 = 0usize;
    let mut tries = 0usize;
    // 候选种子：抖动网格（复用 texture 思路的简化版）。
    let grid_n = ((target2 as f64 * 2.5).sqrt().ceil() as usize).max(8);
    let mut candidates = Vec::new();
    for gy in 0..grid_n {
        for gx in 0..grid_n {
            let jx = rng.gaussian() * 0.3;
            let jy = rng.gaussian() * 0.3;
            let x = bx0 + ((gx as f64 + 0.5 + jx) / grid_n as f64) * sw;
            let y = by0 + ((gy as f64 + 0.5 + jy) / grid_n as f64) * sh;
            if parsed.contains(x, y) {
                candidates.push((x, y));
            }
        }
    }
    // 确定性洗牌。
    for i in (1..candidates.len()).rev() {
        let j = (rng.next_f64() * (i + 1) as f64) as usize;
        candidates.swap(i, j);
    }
    for (sx, sy) in candidates {
        if made2 >= target2 || tries > target2 * 4 {
            break;
        }
        tries += 1;
        let (rgb, ang_deg, mag) = sample_ref(sx, sy);
        // 落笔概率 ∝ ‖∇L‖（明暗交界密集，平涂区稀疏）。
        let accept = 0.15 + 0.85 * (mag / max_mag).clamp(0.0, 1.0);
        if rng.next_f64() > accept as f64 {
            continue;
        }
        let ang = ang_deg.to_radians();
        let len = size * profile.len_mult * rng.range(0.75, 1.25);
        let npts = 6usize;
        let mut pts = Vec::with_capacity(npts);
        for k in 0..npts {
            let u = k as f64 / (npts - 1) as f64;
            let t = u - 0.5;
            // 沿等明度切向，叠加微小法向抖动。
            let nx = -ang.sin();
            let ny = ang.cos();
            let wob = rng.gaussian() * len * 0.03;
            let px = sx + t * len * ang.cos() + wob * nx;
            let py = sy + t * len * ang.sin() + wob * ny;
            let p = 0.3 + 0.5 * (std::f64::consts::PI * u).sin();
            pts.push((px, py, p));
        }
        let alpha = rng.range(0.18, profile.base_alpha.max(0.18)).min(1.0) as f32;
        emit(
            ctx,
            &pts,
            size,
            [rgb[0], rgb[1], rgb[2], alpha],
            0.6,
            &mut ok_count,
            &mut fail_count,
            &mut first_error,
        );
        made2 += 1;
    }

    let total = ok_count + fail_count;
    let mut value = json!({
        "ok": fail_count == 0 && ok_count > 0,
        "strokes": ok_count,
        "failed": fail_count,
        "planned": total,
        "texture_type": texture_for_profile,
        "samples": samples,
        "bbox": [bx0, by0, bx1, by1],
        "ref": ref_src,
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_sample 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }
    Ok(value)
}

/// ⑤ smart_erase_shape 的纯逻辑：按行计算擦除游程（可单元测试）。
/// 返回每行的 `[(x_start, x_end)]` 列表。
#[allow(clippy::too_many_arguments)]
pub(super) fn erase_shape_runs(
    parsed: &ParsedSmartShape,
    invert: bool,
    rx0: f64,
    ry0: f64,
    rx1: f64,
    ry1: f64,
    row_step: f64,
    col_step: f64,
) -> Vec<Vec<(f64, f64)>> {
    let want_erase = |x: f64, y: f64| -> bool {
        let inside = parsed.contains(x, y);
        if invert {
            !inside
        } else {
            inside
        }
    };
    let mut rows = Vec::new();
    let mut y = ry0;
    while y <= ry1 {
        let mut runs: Vec<(f64, f64)> = Vec::new();
        let mut x = rx0;
        let mut run_start: Option<f64> = None;
        while x <= rx1 {
            if want_erase(x, y) {
                if run_start.is_none() {
                    run_start = Some(x);
                }
            } else if let Some(s) = run_start.take() {
                runs.push((s, x - col_step));
            }
            x += col_step;
        }
        if let Some(s) = run_start.take() {
            runs.push((s, rx1));
        }
        rows.push(runs);
        y += row_step;
    }
    rows
}

/// ⑤ `smart_erase_shape`：按几何形状（支持带孔多边形与反相）精确擦除图层内容。
///
/// 不经过选区栈（不提交 CreateSelection 原子，不污染 `state.selections`），
/// 直接构造内存中的形状判定，用紧密蛇形扫描线生成一条擦除路径，
/// 合并为单次 `AtomKind::Erase` 提交 ⇒ 1 个 Tool Call = 1 个 Undo 原子。
pub(super) fn write_smart_erase_shape(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）。
    require_smart_layer_alive(ctx, &layer_id)?;
    let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
    let feather = args
        .get("feather")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 256.0);
    let opacity = args
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let size = args
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or(24.0)
        .clamp(4.0, 128.0);

    let shape = args.get("shape").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_erase_shape 缺少必填参数 shape".to_owned()),
        )
    })?;
    let parsed = parse_smart_shape(shape, "smart_erase_shape")?;
    let (bx0, by0, bx1, by1) = parsed.bbox;

    // 擦除区域：invert=false 时 shape.bbox 外扩 feather；invert=true 时整幅画布。
    // **★ N3D-03（第三轮复审 P1）：非反相分支也要裁到画布** ✓
    //   **问题**：原来 `invert=true` 用整幅画布（有界 ✓），而 `invert=false` 用
    //   `shape.bbox 外扩 feather`，**完全不裁画布** ✗
    //   ⇒ shape 放在画布外（如 x=1e7）⇒ 蛇形扫描线会走 1e7 × 1e7 ⇒
    //     `path` 无上界 ⇒ **一次调用持锁挂死服务端** ✗
    //   **修法**：两分支**共用**画布裁剪（这正是报告建议的「与非反相分支共用同一裁剪函数」）；
    //   若裁剪后区域为空 ⇒ **明确返回 clipped_out**（不是静默成功）
    let (dw, dh) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|d| (d.state().width as f64, d.state().height as f64))
        .unwrap_or((4096.0, 4096.0));
    let (rx0, ry0, rx1, ry1) = if invert {
        (0.0, 0.0, dw, dh)
    } else {
        (
            (bx0 - feather).max(0.0),
            (by0 - feather).max(0.0),
            (bx1 + feather).min(dw),
            (by1 + feather).min(dh),
        )
    };
    if rx1 <= rx0 || ry1 <= ry0 {
        return Ok(json!({
            "ok": true,
            "clipped_out": true,
            "note": "擦除区域与画布无交集 ⇒ 没有可擦除的像素（未提交原子）",
        }));
    }
    // 擦除区域：invert=false 时 shape.bbox 外扩 feather；invert=true 时整幅画布。
    // （行游程计算见 erase_shape_runs。）

    // 蛇形扫描线：行距 0.5×size，行内 2px 步进找游程，交替换向连成一条路径。
    // 擦除盖章间距 = size×0.15，半径 = size/2，2px 行内精度足够。
    let row_step = (size * 0.5).max(2.0);
    let col_step = 2.0;
    let runs_per_row = erase_shape_runs(&parsed, invert, rx0, ry0, rx1, ry1, row_step, col_step);
    let mut path: Vec<(f64, f64)> = Vec::new();
    let mut left_to_right = true;
    let mut y = ry0;
    for runs in &runs_per_row {
        if !runs.is_empty() {
            if left_to_right {
                for (s, e) in runs {
                    path.push((*s, y));
                    path.push((*e, y));
                }
            } else {
                for (s, e) in runs.iter().rev() {
                    path.push((*e, y));
                    path.push((*s, y));
                }
            }
            left_to_right = !left_to_right;
        }
        y += row_step;
    }
    if path.len() < 2 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("smart_erase_shape：形状内没有可擦除的游程".to_owned()),
        ));
    }
    // 羽化 → 硬度：feather=1px 时 hardness≈0.96（1px 抗锯齿软边）。
    let hardness = (1.0 - (feather / size).clamp(0.0, 1.0) * 0.9).clamp(0.0, 1.0);
    let points: Vec<Value> = path.iter().map(|(x, y)| json!([x, y])).collect();
    let object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
    let payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "data": {
            "points": points,
            "size": size,
            "hardness": hardness,
            "opacity": opacity,
        },
    });
    let result = ctx.commit(AtomKind::Erase, payload)?;
    let region = region_of(&result);
    let mut value = finish_mutation(ctx, &result, region)?;
    value["erased_runs"] = json!(path.len() / 2);
    value["invert"] = json!(invert);
    Ok(value)
}

/// ⑥ `smart_hatch_follow`：沿一条或多条引导曲线生成顺应流向的排线笔触。
///
/// 根治"鳞片感"（法向分层 + 黄金分割相位错位）与"小叉"（AR(1) 相关角度游走
/// 叠在曲线切向场上，宏观切向锁死引导线）。
pub(super) struct GuideTable {
    /// 弧长表：(s, x, y, tangent_rad, curvature)。
    pub(super) rows: Vec<(f64, f64, f64, f64, f64)>,
    /// 总弧长。
    pub(super) total: f64,
}

impl GuideTable {
    /// 从折线点列构建。点数 ≥3 时用向心 Catmull-Rom 加密，再按 2px 步长
    /// 重采样为弧长表；2 点时直接取直线。
    pub(super) fn build(pts: &[(f64, f64)]) -> Option<Self> {
        if pts.len() < 2 {
            return None;
        }
        // 密集采样（向心 CR，无需均匀）。
        let mut dense: Vec<(f64, f64)> = Vec::new();
        if pts.len() == 2 {
            dense.push(pts[0]);
            dense.push(pts[1]);
        } else {
            for i in 0..pts.len() - 1 {
                let p0 = pts[i.saturating_sub(1)];
                let p1 = pts[i];
                let p2 = pts[i + 1];
                let p3 = pts[(i + 2).min(pts.len() - 1)];
                // 向心参数化 knot。
                fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
                    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
                }
                let t0 = 0.0;
                let t1 = t0 + dist(p0, p1).sqrt();
                let t2 = t1 + dist(p1, p2).sqrt();
                let t3 = t2 + dist(p2, p3).sqrt();
                // Barry-Goldman 递推。
                let n = 16usize;
                for k in 0..=n {
                    if i > 0 && k == 0 {
                        continue; // 避免段间重复点
                    }
                    let t = t1 + (t2 - t1) * k as f64 / n as f64;
                    // 一阶
                    let a1x = (t1 - t) / (t1 - t0).max(1e-9) * p0.0
                        + (t - t0) / (t1 - t0).max(1e-9) * p1.0;
                    let a1y = (t1 - t) / (t1 - t0).max(1e-9) * p0.1
                        + (t - t0) / (t1 - t0).max(1e-9) * p1.1;
                    let a2x = (t2 - t) / (t2 - t1).max(1e-9) * p1.0
                        + (t - t1) / (t2 - t1).max(1e-9) * p2.0;
                    let a2y = (t2 - t) / (t2 - t1).max(1e-9) * p1.1
                        + (t - t1) / (t2 - t1).max(1e-9) * p2.1;
                    let a3x = (t3 - t) / (t3 - t2).max(1e-9) * p2.0
                        + (t - t2) / (t3 - t2).max(1e-9) * p3.0;
                    let a3y = (t3 - t) / (t3 - t2).max(1e-9) * p2.1
                        + (t - t2) / (t3 - t2).max(1e-9) * p3.1;
                    // 二阶
                    let b1x =
                        (t2 - t) / (t2 - t0).max(1e-9) * a1x + (t - t0) / (t2 - t0).max(1e-9) * a2x;
                    let b1y =
                        (t2 - t) / (t2 - t0).max(1e-9) * a1y + (t - t0) / (t2 - t0).max(1e-9) * a2y;
                    let b2x =
                        (t3 - t) / (t3 - t1).max(1e-9) * a2x + (t - t1) / (t3 - t1).max(1e-9) * a3x;
                    let b2y =
                        (t3 - t) / (t3 - t1).max(1e-9) * a2y + (t - t1) / (t3 - t1).max(1e-9) * a3y;
                    // 三阶
                    let cx =
                        (t2 - t) / (t2 - t1).max(1e-9) * b1x + (t - t1) / (t2 - t1).max(1e-9) * b2x;
                    let cy =
                        (t2 - t) / (t2 - t1).max(1e-9) * b1y + (t - t1) / (t2 - t1).max(1e-9) * b2y;
                    dense.push((cx, cy));
                }
            }
        }
        if dense.len() < 2 {
            return None;
        }
        // 弧长重采样（步长 2px）。
        let mut cum = vec![0.0];
        for i in 1..dense.len() {
            let d = ((dense[i].0 - dense[i - 1].0).powi(2) + (dense[i].1 - dense[i - 1].1).powi(2))
                .sqrt();
            cum.push(cum[i - 1] + d);
        }
        let total = *cum.last().unwrap_or(&0.0);
        if total < 1.0 {
            return None;
        }
        // **★ N3D-02（第三轮复审 P1）：三重上界** ✓
        //   **问题**：`total` 是 guide 的**总弧长**，**无上界** ⇒
        //   `n = ceil(total / 2px)` ⇒ `Vec::with_capacity(n)` **直接按调用方的曲线长度分配**
        //   ⇒ 一条很长的 guide + 2px 步长 = 巨表；随后 `for k in 0..n` 又是**持锁长循环** ✗
        //   **修法**：弧长、表大小、点数**各设上限**，超限返回 `None`
        //   （调用方 `write_smart_hatch_follow` 会把「无可行引导线」报出去，不是静默）
        const MAX_GUIDE_POINTS: usize = 10_000;
        const MAX_GUIDE_ARC: f64 = 400_000.0; // 400k px 弧长
        const MAX_GUIDE_ROWS: usize = 200_001; // 400k / 2px + 1
        if pts.len() > MAX_GUIDE_POINTS {
            return None;
        }
        if total > MAX_GUIDE_ARC {
            return None;
        }
        let step = 2.0;
        let n = (total / step).ceil() as usize + 1;
        if n > MAX_GUIDE_ROWS {
            return None;
        }
        let mut rows = Vec::with_capacity(n);
        let mut di = 0usize;
        for k in 0..n {
            let s = (k as f64 * step).min(total);
            while di + 1 < dense.len() && cum[di + 1] < s {
                di += 1;
            }
            let i2 = (di + 1).min(dense.len() - 1);
            let seg = (cum[i2] - cum[di]).max(1e-9);
            let t = ((s - cum[di]) / seg).clamp(0.0, 1.0);
            let x = dense[di].0 + (dense[i2].0 - dense[di].0) * t;
            let y = dense[di].1 + (dense[i2].1 - dense[di].1) * t;
            // 切向（中心差分）。
            let ip = di.saturating_sub(1);
            let in_ = (di + 2).min(dense.len() - 1);
            let tx = dense[in_].0 - dense[ip].0;
            let ty = dense[in_].1 - dense[ip].1;
            let tangent = ty.atan2(tx);
            rows.push((s, x, y, tangent, 0.0));
        }
        // 曲率（弧长表上中心差分）。
        let m = rows.len();
        let mut with_k = Vec::with_capacity(m);
        for i in 0..m {
            let ip = rows[i.saturating_sub(1)];
            let in_ = rows[(i + 1).min(m - 1)];
            let ds = (in_.0 - ip.0).max(1e-9);
            // 一阶导（单位切向）
            let t0x = ip.3.cos();
            let t0y = ip.3.sin();
            let t1x = in_.3.cos();
            let t1y = in_.3.sin();
            // 二阶导 ≈ Δtangent/ds
            let ax = (t1x - t0x) / ds;
            let ay = (t1y - t0y) / ds;
            // κ = |t × t'|（二维叉乘模）
            let kappa = (t0x * ay - t0y * ax).abs();
            let (s, x, y, tangent, _) = rows[i];
            with_k.push((s, x, y, tangent, kappa));
        }
        Some(Self {
            rows: with_k,
            total,
        })
    }

    /// 弧长 s 处的 (x, y, tangent, curvature)（线性插值）。
    pub(super) fn at(&self, s: f64) -> (f64, f64, f64, f64) {
        let s = s.clamp(0.0, self.total);
        let n = self.rows.len();
        if n == 0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        // 二分（表已按 s 排序）。
        let mut lo = 0usize;
        let mut hi = n - 1;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.rows[mid].0 < s {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let i1 = lo.min(n - 1);
        let i0 = i1.saturating_sub(1);
        let (s0, x0, y0, t0, k0) = self.rows[i0];
        let (s1, x1, y1, t1, k1) = self.rows[i1];
        let tt = if (s1 - s0).abs() < 1e-9 {
            0.0
        } else {
            (s - s0) / (s1 - s0)
        };
        (
            x0 + (x1 - x0) * tt,
            y0 + (y1 - y0) * tt,
            t0 + (t1 - t0) * tt,
            k0 + (k1 - k0) * tt,
        )
    }
}

pub(super) fn write_smart_hatch_follow(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    // **P2-6**：先验图层（fail-fast，见 `require_smart_layer_alive`）—— probe2 在此空烧 50s 才报错 ✓。
    require_smart_layer_alive(ctx, &layer_id)?;
    // **P2-6**：内层笔触缺省不等渲染（见 `set_inner_wait_for_render`）。
    set_inner_wait_for_render(ctx, args);
    let brush_name = optional_str(args, "brush").unwrap_or_else(|| "calligraphy".to_owned());
    let color = args.get("color").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("smart_hatch_follow 缺少必填参数 color".to_owned()),
        )
    })?;
    let color_arr: [f32; 4] = match color {
        Value::Array(arr) => [
            arr.first().and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(1).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(2).and_then(Value::as_f64).unwrap_or(0.0) as f32,
            arr.get(3).and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ],
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("color 应为 [r,g,b,a] 数组".to_owned()),
            ))
        }
    };
    let width = args
        .get("width")
        .and_then(Value::as_f64)
        .unwrap_or(40.0)
        .clamp(4.0, 500.0);
    let density = args
        .get("density")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    let size = args
        .get("size")
        .and_then(Value::as_f64)
        .unwrap_or(5.0)
        .clamp(1.0, 40.0);

    // 引导线。
    let guides_v = args
        .get("guides")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("smart_hatch_follow 缺少必填参数 guides".to_owned()),
            )
        })?;
    let mut tables = Vec::new();
    for g in guides_v {
        let pts: Vec<(f64, f64)> = g
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        p.as_array().and_then(|pair| {
                            if pair.len() >= 2 {
                                Some((
                                    pair[0].as_f64().unwrap_or(0.0),
                                    pair[1].as_f64().unwrap_or(0.0),
                                ))
                            } else {
                                None
                            }
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if let Some(t) = GuideTable::build(&pts) {
            tables.push(t);
        }
    }
    if tables.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("guides 中没有可用的引导线（每条至少 2 个点）".to_owned()),
        ));
    }

    // 可选边界裁剪（含 holes）。
    let clip_shape: Option<ParsedSmartShape> = args
        .get("shape")
        .map(|s| parse_smart_shape(s, "smart_hatch_follow"))
        .transpose()?;
    let inside_clip =
        |x: f64, y: f64| -> bool { clip_shape.as_ref().is_none_or(|p| p.contains(x, y)) };

    let seed = {
        let mut h = 0u64;
        for t in &tables {
            h = h.wrapping_add((t.total * 1000.0) as u64);
        }
        h.wrapping_add(0x9E3779B97F4A7C15).max(1)
    };
    let mut rng = TextureXorShift(seed);
    let clip_sel = optional_str(args, "clip_to_selection").unwrap_or_default();

    let mut ok_count = 0usize;
    let mut fail_count = 0usize;
    let mut first_error: Option<String> = None;
    let l_base = size * 6.0;

    for table in &tables {
        // 法向分层：行数 = width / (size*0.7)。
        let rows_n = ((width / (size * 0.7)).round() as usize).clamp(1, 40);
        for r in 0..rows_n {
            let n_off = if rows_n == 1 {
                0.0
            } else {
                -width / 2.0 + r as f64 * width / (rows_n - 1) as f64
            };
            // 黄金分割相位错位：相邻两排起止点永远交错（消灭"鳞片感"）。
            let delta_s = l_base * 0.7;
            let s_offset = ((r as f64 * 0.6180339887) % 1.0) * delta_s;
            let mut s = s_offset;
            // AR(1) 相关角度游走状态（每排独立）。
            let mut delta_theta = 0.0;
            while s < table.total {
                // density 稀疏。
                if rng.next_f64() > density {
                    s += delta_s;
                    continue;
                }
                let (gx, gy, tangent, kappa) = table.at(s);
                let nx = -tangent.sin();
                let ny = tangent.cos();
                let sx = gx + n_off * nx;
                let sy = gy + n_off * ny;
                if !inside_clip(sx, sy) {
                    s += delta_s;
                    continue;
                }
                // 曲率自适应长度：转急弯缩短，直道拉长。
                let len = l_base / (1.0 + 12.0 * kappa) * rng.range(0.70, 1.35);
                // 沿切向 + AR(1) 游走推进（宏观切向锁死引导线，消灭"小叉"）。
                let step = (size * 0.4).max(2.0);
                let npts = ((len / step).ceil() as usize).clamp(3, 60);
                let mut pts = Vec::with_capacity(npts);
                let (mut px, mut py) = (sx, sy);
                let mut cs = s; // 当前弧长（用于查曲率/切向）
                let mut ok = true;
                for k in 0..npts {
                    if !inside_clip(px, py) {
                        // 出边界：若已有点则收尾，否则整笔丢弃。
                        if pts.len() >= 2 {
                            break;
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    let u = k as f64 / (npts - 1).max(1) as f64;
                    // 梭形压感：0.15 + 0.70·sin^0.8(πu)。
                    let p = 0.15 + 0.70 * (std::f64::consts::PI * u).sin().powf(0.8);
                    pts.push((px, py, p));
                    // 下一步方向：引导线切向 + AR(1)。
                    let (_, _, tan2, _) = table.at(cs);
                    delta_theta = 0.75 * delta_theta + 0.025 * rng.gaussian();
                    let dir = tan2 + delta_theta;
                    px += dir.cos() * step;
                    py += dir.sin() * step;
                    cs += step;
                }
                if ok && pts.len() >= 2 {
                    // 落笔（time_ms 按恒定速度）。
                    let mut acc = 0.0;
                    let mut prev: Option<(f64, f64)> = None;
                    let points: Vec<Value> = pts
                        .iter()
                        .map(|(x, y, p)| {
                            if let Some((ox, oy)) = prev {
                                acc += ((x - ox).powi(2) + (y - oy).powi(2)).sqrt();
                            }
                            prev = Some((*x, *y));
                            json!([
                                x.round(),
                                y.round(),
                                p.clamp(0.0, 1.0),
                                acc / 400.0 * 1000.0
                            ])
                        })
                        .collect();
                    let stroke_args = json!({
                        "layer_id": layer_id,
                        "brush": brush_name,
                        "points": points,
                        "size": size,
                        "color": [color_arr[0], color_arr[1], color_arr[2], color_arr[3]],
                        "clip_to_selection": if clip_sel.is_empty() { Value::Null } else { json!(clip_sel) },
                        "preview": false,
                    });
                    match write_brush_stroke(ctx, &stroke_args) {
                        Ok(_) => ok_count += 1,
                        Err(e) => {
                            fail_count += 1;
                            if first_error.is_none() {
                                first_error = Some(format!("{e:?}"));
                            }
                        }
                    }
                }
                s += delta_s;
            }
        }
    }

    let mut value = json!({
        "ok": fail_count == 0 && ok_count > 0,
        "strokes": ok_count,
        "failed": fail_count,
        "guides": tables.len(),
    });
    if let Some(err) = first_error.as_ref() {
        value["first_error"] = json!(err);
    }
    if ok_count == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "smart_hatch_follow 所有 {fail_count} 笔都失败：{}",
                first_error.unwrap_or_default()
            )),
        ));
    }
    Ok(value)
}

/// 采样图层在区域内的非透明像素平均色（`color: "auto"` 用）。
/// 全透明时回退中灰。
pub(super) fn sample_layer_average(
    ctx: &mut ToolContext<'_>,
    layer_id: &str,
    bx0: f64,
    by0: f64,
    bx1: f64,
    by1: f64,
) -> Result<[f64; 3]> {
    use yanshi_core::Bbox;
    let region = Bbox::new(bx0, by0, bx1, by1);
    let (w, h, pixels) = ctx
        .workspace
        .render_region_raw_layer(&ctx.doc_id, region, layer_id)?;
    if w == 0 || h == 0 || pixels.is_empty() {
        return Ok([0.5, 0.5, 0.5]);
    }
    let mut r_sum = 0u64;
    let mut g_sum = 0u64;
    let mut b_sum = 0u64;
    let mut count = 0u64;
    for px in pixels.chunks_exact(4) {
        // 非透明（alpha > 16）才计入。
        if px[3] > 16 {
            r_sum += u64::from(px[0]);
            g_sum += u64::from(px[1]);
            b_sum += u64::from(px[2]);
            count += 1;
        }
    }
    if count == 0 {
        return Ok([0.5, 0.5, 0.5]);
    }
    Ok([
        r_sum as f64 / count as f64 / 255.0,
        g_sum as f64 / count as f64 / 255.0,
        b_sum as f64 / count as f64 / 255.0,
    ])
}

/// RGB ↔ HSL 互转（`derive_stroke_color` 的明度/饱和度调整用）。
pub(super) fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h, s, l)
}

pub(super) fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s.abs() < f64::EPSILON {
        return (l, l, l);
    }
    let hue_to_rgb = |p: f64, q: f64, mut t: f64| -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    };
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    (
        hue_to_rgb(p, q, h + 1.0 / 3.0),
        hue_to_rgb(p, q, h),
        hue_to_rgb(p, q, h - 1.0 / 3.0),
    )
}

/// 专家方案「一、3」：凹凸双极配色（Bas-Relief Dual Polarity）。
///
/// - 80% 暗部（凹槽）：L 降 12%~18%（skin 额外 R+=0.03/B−=0.02 模拟血色感；
///   stone 饱和度降 15%）；
/// - 20% 微高光（受光颗粒）：L 升 8%~12%，Alpha 减半 —— 侧光浮雕感；
/// - 强制不透明度保护：Alpha 恒 ≤ base_alpha（0.18~0.38），绝不 1.0 实色；
/// - 骨相层（Pass 1）：Alpha = base_alpha × 0.45，若有若无的大尺度斑驳。
pub(super) fn derive_stroke_color(
    rng: &mut TextureXorShift,
    base_rgb: [f64; 3],
    texture: &str,
    is_highlight: bool,
    is_macro: bool,
    base_alpha: f64,
) -> [f64; 4] {
    let (h, s, l) = rgb_to_hsl(base_rgb[0], base_rgb[1], base_rgb[2]);
    let (r, g, b) = if is_highlight {
        let l2 = (l * rng.range(1.08, 1.12)).clamp(0.0, 1.0);
        hsl_to_rgb(h, s, l2)
    } else {
        let l2 = (l * rng.range(0.82, 0.88)).clamp(0.0, 1.0);
        let s2 = if texture == "stone" { s * 0.85 } else { s };
        let (mut r, g, mut b) = hsl_to_rgb(h, s2, l2);
        if texture == "skin" {
            r = (r + 0.03).clamp(0.0, 1.0);
            b = (b - 0.02).clamp(0.0, 1.0);
        }
        (r, g, b)
    };
    let alpha = if is_macro {
        base_alpha * 0.45
    } else {
        let mut a = rng.range(0.18, base_alpha.max(0.18)).min(1.0);
        if is_highlight {
            a *= 0.5;
        }
        a
    };
    [
        r.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        alpha,
    ]
}
/// ============================================================================
/// `smart_select`：OKLab 魔棒智能选取（连通泛洪 + 抗锯齿软边 + 磁力边缘细化）
/// ============================================================================
/// RGBA → Alpha 预乘的 4D 感知向量 `(αL, 2αa, 2αb, α)`。
///
/// 全透明像素（`α = 0`）恒为零向量 ⇒ 任意两个全透明像素的距离恒为 0，
/// 点空白背景不会只选出一半（naive 灾难 #2）。
#[inline]
pub(super) fn rgba_to_oklaba(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    if a == 0 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let alpha = a as f32 * (1.0 / 255.0);
    let lab = srgb_u8_to_oklab(r, g, b);
    [
        alpha * lab[0],
        alpha * 2.0 * lab[1],
        alpha * 2.0 * lab[2],
        alpha,
    ]
}

/// 4D 预乘 OKLabA 向量的平方距离。
#[inline]
pub(super) fn oklaba_dist_sq(c1: [f32; 4], c2: [f32; 4]) -> f32 {
    let d0 = c1[0] - c2[0];
    let d1 = c1[1] - c2[1];
    let d2 = c1[2] - c2[2];
    let d3 = c1[3] - c2[3];
    d0 * d0 + d1 * d1 + d2 * d2 + d3 * d3
}

/// 距离 → `u8` 覆盖率：`[0, de_in]` 内严格 255，`[de_out, ∞)` 为 0，
/// 过渡带按颜色混合比例给亚像素软边（naive 灾难 #4 的解药）。
#[inline]
pub(super) fn coverage_u8_from_dist(dist: f32, de_in: f32, de_out: f32) -> u8 {
    if dist <= de_in {
        255
    } else if dist >= de_out {
        0
    } else {
        let t = (de_out - dist) / (de_out - de_in).max(1e-5);
        (t * 255.0 + 0.5) as u8
    }
}

/// `tolerance ∈ [0, 1]` → `(ΔE_max, ΔE_in, ΔE_out, ΔE_max²)`：二次感知标定曲线。
///
/// 人眼对低容差区极敏感，线性映射会让前半段跨度太大；二次曲线让 `t = 0.2`
/// （默认）恰好吃下 `color_breath = 0.08` 的手绘色块且不穿线稿。
pub(super) fn wand_thresholds(tolerance: f64) -> (f32, f32, f32, f32) {
    let t = tolerance.clamp(0.0, 1.0) as f32;
    let de_max = 0.005 + 0.35 * t + 0.85 * t * t;
    let sigma_aa = (0.20 * de_max).max(0.025);
    let de_in = (de_max - sigma_aa).max(0.0);
    let de_out = de_max + sigma_aa;
    (de_max, de_in, de_out, de_max * de_max)
}

/// 扫描线区间泛洪（Scanline Span Flood Fill）+ 1px 抗锯齿边界环。
///
/// - `contiguous = true`：从种子做 4-连通泛洪；`false`：全区域线性扫描
///   （"色彩范围选取"）。
/// - 绝不降采样：1~2px 的线稿缝降采样后会被冲淡，导致漏墨穿出（致命）。
/// - 按需 OKLab 求值：只在访问像素时转色，不全图转 `f32`（4K 下省 132MB）。
/// - 返回 `(mask, bbox)`：`mask` 为 `w*h` 的 `u8` 覆盖率，`bbox` 为
///   `coverage > 0` 的紧致 `[x, y, w, h]`（无选中时 `[0, 0, 0, 0]`）。
pub(super) fn magic_wand_scanline(
    raw: &[u8],
    w: usize,
    h: usize,
    seed_x: usize,
    seed_y: usize,
    tolerance: f64,
    contiguous: bool,
) -> (Vec<u8>, [i32; 4]) {
    let (_de_max, de_in, de_out, de_max_sq) = wand_thresholds(tolerance);
    let de_out_sq = de_out * de_out;

    let seed_off = (seed_y * w + seed_x) * 4;
    let c_seed = rgba_to_oklaba(
        raw[seed_off],
        raw[seed_off + 1],
        raw[seed_off + 2],
        raw[seed_off + 3],
    );

    let mut mask = vec![0u8; w * h];
    let (mut min_x, mut min_y) = (w as i32, h as i32);
    let (mut max_x, mut max_y) = (-1i32, -1i32);
    macro_rules! put {
        ($x:expr, $y:expr, $cov:expr) => {{
            let _i = ($y) * w + ($x);
            if $cov > mask[_i] {
                mask[_i] = $cov;
            }
            let _x = ($x) as i32;
            let _y = ($y) as i32;
            if _x < min_x {
                min_x = _x;
            }
            if _x > max_x {
                max_x = _x;
            }
            if _y < min_y {
                min_y = _y;
            }
            if _y > max_y {
                max_y = _y;
            }
        }};
    }

    // 单像素求值：(是否连通核, 覆盖率)。连通核继续生长；边界环只记覆盖率。
    let eval_px = |x: usize, y: usize| -> (bool, u8) {
        let p = (y * w + x) * 4;
        let c = rgba_to_oklaba(raw[p], raw[p + 1], raw[p + 2], raw[p + 3]);
        let d_sq = oklaba_dist_sq(c, c_seed);
        if d_sq <= de_max_sq {
            (
                true,
                coverage_u8_from_dist(d_sq.sqrt(), de_in, de_out).max(1),
            )
        } else if d_sq < de_out_sq {
            (false, coverage_u8_from_dist(d_sq.sqrt(), de_in, de_out))
        } else {
            (false, 0)
        }
    };

    if !contiguous {
        for y in 0..h {
            for x in 0..w {
                let (core, cov) = eval_px(x, y);
                let _ = core;
                if cov > 0 {
                    put!(x, y, cov);
                }
            }
        }
    } else {
        // visited: 0 = 未访问，1 = 连通核，2 = 边界环。
        let mut visited = vec![0u8; w * h];
        let mut stack: Vec<(usize, usize)> = Vec::with_capacity(256);
        visited[seed_y * w + seed_x] = 1;
        stack.push((seed_y, seed_x));
        // 种子即参考色，距离恒 0，覆盖率恒 255。
        put!(seed_x, seed_y, 255);

        while let Some((y, sx)) = stack.pop() {
            // 本行向左右扩展核心区间。
            let mut xl = sx;
            while xl > 0 {
                let nx = xl - 1;
                let idx = y * w + nx;
                if visited[idx] != 0 {
                    break;
                }
                let (is_core, cov) = eval_px(nx, y);
                if cov > 0 {
                    put!(nx, y, cov);
                }
                if is_core {
                    visited[idx] = 1;
                    xl = nx;
                } else {
                    visited[idx] = 2;
                    break;
                }
            }
            let mut xr = sx;
            while xr + 1 < w {
                let nx = xr + 1;
                let idx = y * w + nx;
                if visited[idx] != 0 {
                    break;
                }
                let (is_core, cov) = eval_px(nx, y);
                if cov > 0 {
                    put!(nx, y, cov);
                }
                if is_core {
                    visited[idx] = 1;
                    xr = nx;
                } else {
                    visited[idx] = 2;
                    break;
                }
            }
            // 上下行：在 [xl-1, xr+1] 内找新核心区间（一次只压一个种子，
            // 区间内其余像素向前吞掉并标记，避免栈膨胀）。
            for ny in [y.wrapping_sub(1), y + 1] {
                if ny >= h {
                    continue;
                }
                let mut x = xl.saturating_sub(1);
                let end = (xr + 1).min(w - 1);
                while x <= end {
                    let idx = ny * w + x;
                    if visited[idx] != 0 {
                        x += 1;
                        continue;
                    }
                    let (is_core, cov) = eval_px(x, ny);
                    if cov > 0 {
                        put!(x, ny, cov);
                    }
                    if !is_core {
                        visited[idx] = 2;
                        x += 1;
                        continue;
                    }
                    // 找到游程起点：向右探测游程终点（只求值记覆盖率，
                    // 不预标记——游程内部留给种子弹出后的左右扩展自然覆盖，
                    // 否则扩展会被 visited=1 截断、洪水退化成梳子形状）。
                    let run_l = x;
                    let mut x2 = x + 1;
                    while x2 <= end {
                        let idx2 = ny * w + x2;
                        if visited[idx2] != 0 {
                            break;
                        }
                        let (c2, cov2) = eval_px(x2, ny);
                        if cov2 > 0 {
                            put!(x2, ny, cov2);
                        }
                        if !c2 {
                            visited[idx2] = 2;
                            x2 += 1;
                            break;
                        }
                        x2 += 1;
                    }
                    visited[ny * w + run_l] = 1;
                    stack.push((ny, run_l));
                    x = x2;
                }
            }
        }
    }

    if max_x < min_x {
        return (mask, [0, 0, 0, 0]);
    }
    let bbox = [min_x, min_y, max_x - min_x + 1, max_y - min_y + 1];
    (mask, bbox)
}

/// 微孔闭合（Pinhole Closing）：填平选区内部面积 ≤ 8px² 的 4-连通暗孔。
///
/// 专治 `smart_fill` / `scatter_texture` 飞白与 `color_breath` 留下的筛子眼。
/// 从边界 flood fill 背景，剩下的 `mask == 0` 连通域即内部孔。
pub(super) fn pinhole_close(mask: &mut [u8], w: usize, h: usize) {
    if w == 0 || h == 0 {
        return;
    }
    // 从四边 flood fill 可达背景。
    let mut bg = vec![false; w * h];
    let mut stack = Vec::with_capacity(w + h);
    for x in 0..w {
        stack.push((x, 0));
        stack.push((x, h - 1));
    }
    for y in 0..h {
        stack.push((0, y));
        stack.push((w - 1, y));
    }
    while let Some((x, y)) = stack.pop() {
        let idx = y * w + x;
        if bg[idx] || mask[idx] > 0 {
            continue;
        }
        bg[idx] = true;
        if x > 0 {
            stack.push((x - 1, y));
        }
        if x + 1 < w {
            stack.push((x + 1, y));
        }
        if y > 0 {
            stack.push((x, y - 1));
        }
        if y + 1 < h {
            stack.push((x, y + 1));
        }
    }
    // 内部孔按连通域填 ≤ 8px² 的。
    let mut seen = vec![false; w * h];
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            if mask[idx] > 0 || bg[idx] || seen[idx] {
                continue;
            }
            let mut comp = Vec::new();
            let mut st = vec![(x, y)];
            seen[idx] = true;
            while let Some((cx, cy)) = st.pop() {
                comp.push((cx, cy));
                if comp.len() > 8 {
                    // 超限：继续标记 seen 但不填（避免重复 BFS）。
                    // 为简单起见仍走完 BFS。
                }
                let neighbors = [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ];
                for (nx, ny) in neighbors {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let nidx = ny * w + nx;
                    if mask[nidx] > 0 || bg[nidx] || seen[nidx] {
                        continue;
                    }
                    seen[nidx] = true;
                    st.push((nx, ny));
                }
            }
            if comp.len() <= 8 {
                for (cx, cy) in comp {
                    mask[cy * w + cx] = 255;
                }
            }
        }
    }
}

/// `u8` 灰度的双线性采样（越界钳制到边缘）。
pub(super) fn bilinear_sample_u8(data: &[u8], w: usize, h: usize, x: f32, y: f32) -> u8 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let at = |ix: i32, iy: i32| -> f32 {
        let cx = ix.clamp(0, w as i32 - 1) as usize;
        let cy = iy.clamp(0, h as i32 - 1) as usize;
        data[cy * w + cx] as f32
    };
    let v = at(x0, y0) * (1.0 - fx) * (1.0 - fy)
        + at(x0 + 1, y0) * fx * (1.0 - fy)
        + at(x0, y0 + 1) * (1.0 - fx) * fy
        + at(x0 + 1, y0 + 1) * fx * fy;
    v.round().clamp(0.0, 255.0) as u8
}

/// 可分离高斯模糊（`u8` 灰度，`sigma` 像素）：`feather` 烘焙用。
pub(super) fn separable_gaussian_blur_u8(data: &[u8], w: usize, h: usize, sigma: f32) -> Vec<u8> {
    if sigma <= 0.0 || w == 0 || h == 0 {
        return data.to_vec();
    }
    let radius = (sigma * 3.0).ceil() as i32;
    let mut kernel = Vec::new();
    let mut sum = 0f32;
    for i in -radius..=radius {
        let v = (-(i as f32) * (i as f32) / (2.0 * sigma * sigma)).exp();
        kernel.push(v);
        sum += v;
    }
    for v in kernel.iter_mut() {
        *v /= sum;
    }
    // 横向。
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0f32;
            for (ki, &kv) in kernel.iter().enumerate() {
                let sx = (x as i32 + ki as i32 - radius).clamp(0, w as i32 - 1) as usize;
                acc += kv * data[y * w + sx] as f32;
            }
            tmp[y * w + x] = acc;
        }
    }
    // 纵向。
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0f32;
            for (ki, &kv) in kernel.iter().enumerate() {
                let sy = (y as i32 + ki as i32 - radius).clamp(0, h as i32 - 1) as usize;
                acc += kv * tmp[sy * w + x];
            }
            out[y * w + x] = acc.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// 窄带边缘细化（Q6）：法向 Sobel 磁力吸附 + 联合双边滤波。
///
/// 只在掩膜 `bbox`（外扩 8px）内运行：
///
/// 1. **磁力吸附**：对轮廓窄带像素，用掩膜梯度估外法线，沿法线 ±6px 找
///    带距离衰减的最大图像梯度响应（`G > 0.08` 才吸附），把边界推/收到真实
///    物象边缘（双线性重采样实现位移）。
/// 2. **联合双边滤波**：2 轮 5×5（`σs = 1.5px`，`σc = 0.06`），以原图 OKLabA
///    颜色为引导权重，让过渡带覆盖率对齐前景/背景混合率，消灭 1px 灰白边。
pub(super) fn refine_mask_edges(raw: &[u8], w: usize, h: usize, mask: &mut [u8], bbox: [i32; 4]) {
    const R: i32 = 6;
    let bx0 = (bbox[0] - R - 2).max(0) as usize;
    let by0 = (bbox[1] - R - 2).max(0) as usize;
    let bx1 = (bbox[0] + bbox[2] + R + 2).min(w as i32) as usize;
    let by1 = (bbox[1] + bbox[3] + R + 2).min(h as i32) as usize;
    if bx1 <= bx0 + 2 || by1 <= by0 + 2 {
        return;
    }
    let rw = bx1 - bx0;
    let rh = by1 - by0;

    // 裁剪区掩膜快照。
    let mut sub = vec![0u8; rw * rh];
    for y in 0..rh {
        let src_off = (by0 + y) * w + bx0;
        sub[y * rw..(y + 1) * rw].copy_from_slice(&mask[src_off..src_off + rw]);
    }

    // --- 1. 图像梯度幅值（4D OKLabA 中心差分，L1 近似）---
    let mut grad = vec![0f32; rw * rh];
    for y in 1..rh - 1 {
        for x in 1..rw - 1 {
            let c_at = |dx: i32, dy: i32| -> [f32; 4] {
                let cx = ((bx0 + x) as i32 + dx).clamp(0, w as i32 - 1) as usize;
                let cy = ((by0 + y) as i32 + dy).clamp(0, h as i32 - 1) as usize;
                let p = (cy * w + cx) * 4;
                rgba_to_oklaba(raw[p], raw[p + 1], raw[p + 2], raw[p + 3])
            };
            let cl = c_at(-1, 0);
            let cr = c_at(1, 0);
            let cu = c_at(0, -1);
            let cd = c_at(0, 1);
            let mut g = 0f32;
            for k in 0..4 {
                g += (cr[k] - cl[k]).abs() + (cd[k] - cu[k]).abs();
            }
            grad[y * rw + x] = g * 0.25;
        }
    }

    // --- 2. 轮廓窄带（mask ∈ (0,255) 外扩 2px）---
    let mut band = vec![false; rw * rh];
    for y in 0..rh {
        for x in 0..rw {
            let v = sub[y * rw + x];
            if v > 0 && v < 255 {
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx >= 0 && ny >= 0 && nx < rw as i32 && ny < rh as i32 {
                            band[ny as usize * rw + nx as usize] = true;
                        }
                    }
                }
            }
        }
    }

    // --- 3. 磁力吸附：对外法线方向找最强边缘 ---
    let m_at = |gx: i32, gy: i32| -> f32 {
        let cx = (gx - bx0 as i32).clamp(0, rw as i32 - 1) as usize;
        let cy = (gy - by0 as i32).clamp(0, rh as i32 - 1) as usize;
        sub[cy * rw + cx] as f32 / 255.0
    };
    let mut snapped = sub.clone();
    for y in 0..rh {
        for x in 0..rw {
            if !band[y * rw + x] {
                continue;
            }
            let gx = (bx0 + x) as i32;
            let gy = (by0 + y) as i32;
            // 外法线 = -∇M/|∇M|（M 内高外低，梯度指向内）。
            let dx = m_at(gx + 1, gy) - m_at(gx - 1, gy);
            let dy = m_at(gx, gy + 1) - m_at(gx, gy - 1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                continue;
            }
            let (nx, ny) = (-dx / len, -dy / len);
            let mut best_t = 0f32;
            let mut best_g = 0f32;
            let mut t = -(R as f32);
            while t <= R as f32 {
                let sx = (x as f32 + t * nx).round() as i32;
                let sy = (y as f32 + t * ny).round() as i32;
                let cx = sx.clamp(0, rw as i32 - 1) as usize;
                let cy = sy.clamp(0, rh as i32 - 1) as usize;
                let g = grad[cy * rw + cx] * (-t * t / 18.0).exp();
                if g > best_g {
                    best_g = g;
                    best_t = t;
                }
                t += 1.0;
            }
            if best_g > 0.08 && best_t.abs() > 0.5 {
                snapped[y * rw + x] = bilinear_sample_u8(
                    &sub,
                    rw,
                    rh,
                    x as f32 - best_t * nx,
                    y as f32 - best_t * ny,
                );
            }
        }
    }

    // --- 4. 联合双边滤波（2 轮）---
    let mut cur = snapped;
    for _ in 0..2 {
        let src = cur.clone();
        for y in 0..rh {
            for x in 0..rw {
                if !band[y * rw + x] {
                    continue;
                }
                let cp = {
                    let p = ((by0 + y) * w + (bx0 + x)) * 4;
                    rgba_to_oklaba(raw[p], raw[p + 1], raw[p + 2], raw[p + 3])
                };
                let mut num = 0f32;
                let mut den = 0f32;
                for dy in -2..=2 {
                    for dx in -2..=2 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= rw as i32 || ny >= rh as i32 {
                            continue;
                        }
                        let (nx, ny) = (nx as usize, ny as usize);
                        let ws = (-((dx * dx + dy * dy) as f32) / 4.5).exp();
                        let cq = {
                            let p = ((by0 + ny) * w + (bx0 + nx)) * 4;
                            rgba_to_oklaba(raw[p], raw[p + 1], raw[p + 2], raw[p + 3])
                        };
                        let mut dc2 = 0f32;
                        for k in 0..4 {
                            let d = cq[k] - cp[k];
                            dc2 += d * d;
                        }
                        let wc = (-dc2 / 0.0072).exp();
                        let wt = ws * wc;
                        num += wt * src[ny * rw + nx] as f32;
                        den += wt;
                    }
                }
                cur[y * rw + x] = (num / den.max(1e-6)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    // 写回。
    for y in 0..rh {
        let dst_off = (by0 + y) * w + bx0;
        mask[dst_off..dst_off + rw].copy_from_slice(&cur[y * rw..(y + 1) * rw]);
    }
}

/// Quick-Mask 预览缩略图（`preview: true` 用）：长边 ≤ 384px。
///
/// 未选中区压暗 + 40% 品红遮罩，选区边缘描 1px 亮青色轮廓线。
/// 返回 PNG 字节（RGBA）。
pub(super) fn quick_mask_preview(
    raw: &[u8],
    w: usize,
    _h: usize,
    mask: &[u8],
    bbox: [i32; 4],
) -> Option<Vec<u8>> {
    let (bx, by, bw, bh) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    if bw <= 0 || bh <= 0 {
        return None;
    }
    let scale = 384.0 / (bw.max(bh) as f32);
    let tw = ((bw as f32 * scale).round() as usize).max(1);
    let th = ((bh as f32 * scale).round() as usize).max(1);
    let mut thumb = vec![0u8; tw * th * 4];
    let cov_at = |tx: usize, ty: usize| -> f32 {
        let sx = ((tx as f32 / scale).floor() as i32).clamp(0, bw - 1) as usize;
        let sy = ((ty as f32 / scale).floor() as i32).clamp(0, bh - 1) as usize;
        mask[sy * (bw as usize) + sx] as f32 / 255.0
    };
    let src_at = |tx: usize, ty: usize| -> [u8; 4] {
        let sx = ((tx as f32 / scale).floor() as i32).clamp(0, bw - 1) as usize;
        let sy = ((ty as f32 / scale).floor() as i32).clamp(0, bh - 1) as usize;
        let p = ((by as usize + sy) * w + (bx as usize + sx)) * 4;
        [raw[p], raw[p + 1], raw[p + 2], raw[p + 3]]
    };
    for ty in 0..th {
        for tx in 0..tw {
            let c = cov_at(tx, ty);
            let s = src_at(tx, ty);
            // 边缘检测：任一 4-邻域跨过 0.5 即轮廓。
            let mut edge = false;
            for (dx, dy) in [(1i32, 0i32), (0, 1)] {
                let nx = (tx as i32 + dx).clamp(0, tw as i32 - 1) as usize;
                let ny = (ty as i32 + dy).clamp(0, th as i32 - 1) as usize;
                let nc = cov_at(nx, ny);
                if (c >= 0.5) != (nc >= 0.5) {
                    edge = true;
                    break;
                }
            }
            let o = (ty * tw + tx) * 4;
            if edge {
                thumb[o..o + 4].copy_from_slice(&[0, 255, 255, 255]);
            } else if c >= 0.5 {
                thumb[o..o + 4].copy_from_slice(&[s[0], s[1], s[2], 255]);
            } else {
                // 压暗 55% + 40% 品红。
                let r = (s[0] as f32 * 0.45 * 0.6 + 255.0 * 0.4) as u8;
                let g = (s[1] as f32 * 0.45 * 0.6) as u8;
                let b = (s[2] as f32 * 0.45 * 0.6 + 255.0 * 0.4) as u8;
                thumb[o..o + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
    yanshi_render::png::encode_png(tw as u32, th as u32, &thumb)
}

/// 从全图 u8 覆盖率掩膜提交位图选区（Q1：`smart_select` 与 `select_opaque_pixels` 共用）。
///
/// 流程：紧致 BBox → 可选羽化烘焙 → 裁剪 → L8 PNG → Blob 入库 + 解码注册 → `CreateSelection`。
/// `mask` 会被羽化烘焙原地修改（写回全图，供调用方继续做诊断统计）。
/// 返回 `(bx, by, bw, bh, blob_str, png_len, atom_seq)`；空掩膜时 bbox 为 `(0,0,0,0)` 且 blob 为空串，
/// 此时仍提交 `CreateSelection`（调用方自行决定空选区的诊断语义）。
#[allow(clippy::too_many_arguments)]
pub(super) fn commit_bitmap_selection(
    ctx: &mut ToolContext<'_>,
    selection_id: &str,
    mask: &mut [u8],
    rw: usize,
    rh: usize,
    feather: f64,
    mode: &str,
    invert: bool,
    refined_edges: bool,
) -> Result<(i32, i32, i32, i32, String, usize, u64)> {
    // ---- 紧致 bbox ----
    let mut tb = [rw as i32, rh as i32, -1i32, -1i32];
    for y in 0..rh {
        for x in 0..rw {
            if mask[y * rw + x] > 0 {
                let (ix, iy) = (x as i32, y as i32);
                tb[0] = tb[0].min(ix);
                tb[1] = tb[1].min(iy);
                tb[2] = tb[2].max(ix);
                tb[3] = tb[3].max(iy);
            }
        }
    }
    let (mut bx, mut by, mut bw, mut bh) = if tb[2] < tb[0] {
        (0, 0, 0, 0)
    } else {
        (tb[0], tb[1], tb[2] - tb[0] + 1, tb[3] - tb[1] + 1)
    };

    // ---- 羽化烘焙（可选）：外扩 ceil(feather) 后做可分离高斯模糊 ----
    let feather_baked = feather > 0.0;
    if feather_baked && bw > 0 {
        let pad = feather.ceil() as i32;
        let ex0 = (bx - pad).max(0);
        let ey0 = (by - pad).max(0);
        let ex1 = (bx + bw + pad).min(rw as i32);
        let ey1 = (by + bh + pad).min(rh as i32);
        let (ew, eh) = ((ex1 - ex0) as usize, (ey1 - ey0) as usize);
        let mut expanded = vec![0u8; ew * eh];
        for y in 0..eh {
            let src_off = ((ey0 as usize) + y) * rw + (ex0 as usize);
            expanded[y * ew..(y + 1) * ew].copy_from_slice(&mask[src_off..src_off + ew]);
        }
        let blurred = separable_gaussian_blur_u8(&expanded, ew, eh, (feather / 2.0) as f32);
        // 写回全图掩膜（供诊断统计），blob 用裁剪后的模糊块。
        for y in 0..eh {
            let dst_off = ((ey0 as usize) + y) * rw + (ex0 as usize);
            mask[dst_off..dst_off + ew].copy_from_slice(&blurred[y * ew..(y + 1) * ew]);
        }
        bx = ex0;
        by = ey0;
        bw = ex1 - ex0;
        bh = ey1 - ey0;
    }

    // ---- 裁剪 → L8 PNG → blob → 注册 ----
    let (blob_str, png_len) = if bw > 0 && bh > 0 {
        let mut cropped = vec![0u8; (bw * bh) as usize];
        for y in 0..bh as usize {
            let src_off = ((by as usize) + y) * rw + (bx as usize);
            cropped[y * (bw as usize)..(y + 1) * (bw as usize)]
                .copy_from_slice(&mask[src_off..src_off + (bw as usize)]);
        }
        let png = yanshi_render::png::encode_png_l8(bw as u32, bh as u32, &cropped)
            .expect("内存 PNG 编码不会失败");
        let png_len = png.len();
        let blob_hash = ctx.workspace.store().put(&png)?;
        let blob_str = blob_hash.to_string();
        yanshi_render::selection::register_bitmap_mask(
            blob_str.clone(),
            Arc::new(yanshi_render::selection::DecodedBitmapMask {
                width: bw as u32,
                height: bh as u32,
                data: cropped,
            }),
        );
        (blob_str, png_len)
    } else {
        (String::new(), 0)
    };

    // **★ N3D-04：`mode != "new"` 时必须**真的合并**（第 1288 轮）** ✓
    //   **缺陷**：`fold.rs:683` 只做「允许覆盖」⇒ 而**真正的布尔合并**在
    //     `tools.rs` 的 `merge_selection_shapes` ⇒ 而这里**从不调它** ⇒
    //     于是 `mode=add/subtract/intersect` **静默覆盖**旧选区（旧工具修过的缺陷在新工具复发）✗
    //   **修法**：`mode != "new"` 时先取旧选区的 shape ⇒ 合并 ⇒ 用合并结果替换 payload 的 shape/blob
    if mode != "new" {
        // **★ 照抄 `create_selection`（`tools_selection.rs:125-133`）的成功路径** ✓（第 1300 轮）
        //   **我原来的两个错**：
        //     ① `document_state(ctx).ok()` ⇒ 若 Err 会**静默变 None**（静默降级 ✗）
        //     ② `serde_json::to_value(&old_shape)` ⇒ 而 `existing.shape` **已经是 `Value`**
        //   ⇒ 于是合并分支**从不执行**（判据实测 `merged_from=None`、`r1 == r2`）✗
        let state = document_state(ctx)?;
        if let Some(existing) = state.selections.get(selection_id) {
            let old_json = existing.shape.clone();
            let new_json = json!({
                "kind": "bitmap",
                "bbox": [bx, by, bw, bh],
                "blob": blob_str,
                "feather_baked": feather_baked,
            });
            if let Ok(Some((mb, mblob, mlen))) =
                merge_selection_shapes(ctx, &old_json, &new_json, mode)
            {
                // 合并成功 ⇒ 用合并后的 bbox/blob 覆盖（`mblob` 是新的 blob hash）
                let merged_shape = json!({
                    "kind": "bitmap",
                    "bbox": mb,
                    "blob": mblob,
                    "feather_baked": feather_baked,
                });
                let payload = json!({
                    "selection_id": selection_id,
                    "shape": merged_shape,
                    "feather": 0.0,
                    // **★ 必须是原 mode，不是 "new"** ✓（第 1300 轮实测）
                    //    的语义是「mode == "new" 且已存在 ⇒ **拒绝**」
                    //   ⇒ 所以写 "new" 会让**已存在的选区被拒**（precondition_failed）✗
                    "mode": mode,
                    "invert": invert,
                    "refined_edges": refined_edges,
                    "blobs": [mblob],
                    "refs": { "blobs": [mblob] },
                    "merged_from": mode,
                    "merged_bytes": mlen,
                });
                let _result = ctx.commit(AtomKind::CreateSelection, payload)?;
                return Ok((mb[0], mb[1], mb[2], mb[3], mblob, mlen, _result.seq));
            }
        }
    }

    // ---- 提交 CreateSelection 原子 ----
    // shape 的 feather_baked 标记防渲染期重复羽化；顶层 feather 置 0
    //（`SelectionShape::from_selection` 会并入，但 Bitmap 的 coverage 本就跳过羽化）。
    let payload = json!({
        "selection_id": selection_id,
        "shape": {
            "kind": "bitmap",
            "bbox": [bx, by, bw, bh],
            "blob": blob_str,
            "feather_baked": feather_baked,
        },
        "feather": 0.0,
        "mode": mode,
        "invert": invert,
        "refined_edges": refined_edges,
        "blobs": [blob_str],
        "refs": { "blobs": [blob_str] },
    });
    let _result = ctx.commit(AtomKind::CreateSelection, payload)?;
    Ok((bx, by, bw, bh, blob_str, png_len, _result.seq))
}

/// 智能选取（`smart_select`）：OKLab 魔棒 + 抗锯齿软边 + 磁力边缘细化。
///
/// 流程：安全裁剪 → 读像素（`render_region_raw[_layer]`，严格用返回的
/// `(rw, rh)` 算 stride，见 tools.rs:4049 大坑）→ Scanline 泛洪 → 微孔闭合 →
/// 边缘细化（可选）→ 羽化烘焙（可选）→ 紧致 BBox → L8 PNG blob →
/// `CreateSelection` 原子（`kind: "bitmap"`）→ 诊断 JSON。
///
/// `"kind"` 必须用 `"bitmap"`：`"magic_wand"` 有现成单测断言退化全选，
/// 复用它会直接变红。
pub(super) fn write_smart_select(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // ---- 参数 ----
    let selection_id = optional_str(args, "selection_id").unwrap_or_else(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("sel_wand_{n}")
    });
    // seed: [x, y] 或 seeds: [[x1, y1], ...]。
    let mut seeds: Vec<(f64, f64)> = Vec::new();
    if let Some(pair) = args.get("seed").and_then(Value::as_array) {
        if let (Some(x), Some(y)) = (
            pair.first().and_then(Value::as_f64),
            pair.get(1).and_then(Value::as_f64),
        ) {
            seeds.push((x, y));
        }
    }
    if let Some(list) = args.get("seeds").and_then(Value::as_array) {
        for item in list {
            if let Some(pair) = item.as_array() {
                if let (Some(x), Some(y)) = (
                    pair.first().and_then(Value::as_f64),
                    pair.get(1).and_then(Value::as_f64),
                ) {
                    seeds.push((x, y));
                }
            }
        }
    }
    if seeds.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "smart_select 需要 seed: [x, y]（或 seeds: [[x, y], ...]）".to_owned(),
            ),
        ));
    }
    let tolerance = args.get("tolerance").and_then(Value::as_f64).unwrap_or(0.2);
    if !(0.0..=1.0).contains(&tolerance) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("tolerance 必须在 [0, 1] 内，得到 {tolerance}")),
        ));
    }
    let contiguous = args
        .get("contiguous")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let refine_edges = args
        .get("refine_edges")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let layer_id = optional_str(args, "layer_id");
    // **P2-6**：给了图层就先验（fail-fast，见 `require_smart_layer_alive`）。
    if let Some(layer_id) = layer_id.as_deref() {
        require_smart_layer_alive(ctx, layer_id)?;
    }
    let mode = optional_str(args, "mode").unwrap_or_else(|| "new".to_owned());
    if !["new", "add", "subtract", "intersect"].contains(&mode.as_str()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "mode 必须是 new/add/subtract/intersect，得到 {mode}"
            )),
        ));
    }
    let feather = args.get("feather").and_then(Value::as_f64).unwrap_or(0.0);
    if !(0.0..=512.0).contains(&feather) || !feather.is_finite() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("feather 必须在 [0, 512] 内，得到 {feather}")),
        ));
    }
    let want_preview = args
        .get("preview")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // ---- 画布尺寸与安全裁剪（tools.rs:4049 大坑）----
    let state = document_state(ctx)?;
    let (canvas_w, canvas_h) = (state.width as usize, state.height as usize);
    if canvas_w == 0 || canvas_h == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("画布尺寸为 0，无法选取".to_owned()),
        ));
    }
    // seed 必须在画布内（不静默钳制：点错位置要让调用方知道）。
    for (sx, sy) in &seeds {
        if !(0.0 <= *sx && *sx < canvas_w as f64 && 0.0 <= *sy && *sy < canvas_h as f64) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "seed [{sx}, {sy}] 不在画布 [0, 0, {canvas_w}, {canvas_h}] 内"
                )),
            ));
        }
    }
    // region 先裁到画布内；严格用返回的 (rw, rh) 算 stride。
    let region = Bbox::new(0.0, 0.0, canvas_w as f64, canvas_h as f64);
    let (rw, rh, raw) = match &layer_id {
        Some(lid) => ctx
            .workspace
            .render_region_raw_layer(&ctx.doc_id, region, lid)?,
        None => ctx.workspace.render_region_raw(&ctx.doc_id, region)?,
    };
    let (rw, rh) = (rw as usize, rh as usize);
    if raw.len() < rw * rh * 4 {
        return Err(YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!(
                "render_region_raw 返回字节不足：{rw}×{rh} 需要 {}，实际 {}",
                rw * rh * 4,
                raw.len()
            )),
        ));
    }
    // seed 颜色（诊断用，取第一个种子）。
    let (s0x, s0y) = (seeds[0].0.floor() as usize, seeds[0].1.floor() as usize);
    let s0p = (s0y.min(rh - 1) * rw + s0x.min(rw - 1)) * 4;
    let seed_rgba = [raw[s0p], raw[s0p + 1], raw[s0p + 2], raw[s0p + 3]];

    // ---- 泛洪（多种子取并集）----
    let mut mask = vec![0u8; rw * rh];
    let mut union_bbox = [0i32, 0i32, 0i32, 0i32];
    let mut any = false;
    for (sx, sy) in &seeds {
        let ix = (sx.floor() as usize).min(rw - 1);
        let iy = (sy.floor() as usize).min(rh - 1);
        let (m, b) = magic_wand_scanline(&raw, rw, rh, ix, iy, tolerance, contiguous);
        if b[2] > 0 && b[3] > 0 {
            for (dst, src) in mask.iter_mut().zip(m.iter()) {
                if *src > *dst {
                    *dst = *src;
                }
            }
            if !any {
                union_bbox = b;
                any = true;
            } else {
                union_bbox[0] = union_bbox[0].min(b[0]);
                union_bbox[1] = union_bbox[1].min(b[1]);
                let x1 = (union_bbox[0] + union_bbox[2]).max(b[0] + b[2]);
                let y1 = (union_bbox[1] + union_bbox[3]).max(b[1] + b[3]);
                union_bbox[2] = x1 - union_bbox[0];
                union_bbox[3] = y1 - union_bbox[1];
            }
        }
    }
    if !any {
        // 零选中：仍提交一个空位图选区（调用方可据诊断判断），不报错。
        // （与"静默降级"纪律的平衡：diagnostic_warning 会明确提示。）
    }

    // ---- 微孔闭合 ----
    pinhole_close(&mut mask, rw, rh);

    // ---- 边缘细化（可选）----
    if refine_edges && any {
        refine_mask_edges(&raw, rw, rh, &mut mask, union_bbox);
    }

    // ---- 紧致 BBox → 羽化烘焙 → L8 PNG → CreateSelection（Q1 公共函数）----
    // `mask` 会被羽化烘焙原地修改（写回全图），下面的诊断统计读的是烘焙后的值，与原来一致。
    let (bx, by, bw, bh, _blob_str, png_len, atom_seq) = commit_bitmap_selection(
        ctx,
        &selection_id,
        &mut mask,
        rw,
        rh,
        feather,
        &mode,
        false,
        refine_edges,
    )?;

    // ---- 诊断指标 ----
    let mut selected_area = 0f64;
    for &v in &mask {
        selected_area += f64::from(v) / 255.0;
    }
    let bbox_area = (bw.max(0) * bh.max(0)) as f64;
    let bbox_fill_ratio = if bbox_area > 0.0 {
        selected_area / bbox_area
    } else {
        0.0
    };
    let canvas_coverage_ratio = selected_area / (canvas_w * canvas_h) as f64;
    let (_, de_max, _, _) = wand_thresholds(tolerance);
    let mut diagnostic_warning: Option<String> = None;
    if !any {
        diagnostic_warning = Some(
            "选区面积为 0：种子点颜色在容差内无连通区域，可能 tolerance 过低或点中了孤立像素"
                .to_owned(),
        );
    } else if canvas_coverage_ratio > 0.85 {
        diagnostic_warning = Some(
            "选区覆盖了超过 85% 的画布，可能发生了边缘漏出（leak），建议降低 tolerance 或检查线稿缺口".to_owned(),
        );
    } else if selected_area < 16.0 {
        diagnostic_warning = Some(
            "选区面积不足 16 像素，可能点中了孤立噪点或 tolerance 过低，建议调高 tolerance 至 0.20~0.35".to_owned(),
        );
    }

    // ---- 预览缩略图（可选）----
    let mut preview_blob: Option<String> = None;
    if want_preview && bw > 0 && bh > 0 {
        if let Some(png) = quick_mask_preview(&raw, rw, rh, &mask, [bx, by, bw, bh]) {
            if let Ok(hash) = ctx.workspace.store().put(&png) {
                preview_blob = Some(hash.to_string());
            }
        }
    }

    Ok(json!({
        "ok": true,
        "selection_id": selection_id,
        "bbox": [bx, by, bw, bh],
        "selected_area_px": selected_area,
        "bbox_fill_ratio": bbox_fill_ratio,
        "canvas_coverage_ratio": canvas_coverage_ratio,
        "seed_color_rgba": seed_rgba,
        "effective_delta_e": de_max,
        "refined_edges": refine_edges,
        "blob_bytes": png_len,
        "diagnostic_warning": diagnostic_warning,
        "preview_blob": preview_blob,
        "atom_seq": atom_seq,
    }))
}

/// 2x3 仿射矩阵 `[a,b,c,d,e,f]`：`x' = a*x + b*y + c`；`y' = d*x + e*y + f`（Q1 选区变换用）。
pub(super) type Affine2x3 = [f64; 6];
