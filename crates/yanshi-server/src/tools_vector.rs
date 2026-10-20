//! **原生矢量图层工具**（路线甲 Phase 2 第一增量）：矢量笔触的写侧校验、
//! 脊线截断橡皮（`vector_erase`）、脊线命中测试（`vector_hit_test`），
//! 以及「光栅通路工具不许落到矢量图层」的守卫。
//!
//! **共同纪律：无静默降级** ✓——矢量层不接受的东西一律显式报错并列全
//! 原因；作用不到的对象逐项计数返回；没碰到的笔触连数据都不重写 ✓。

use super::*;

/// 查一个图层的类型（图层不存在 ⇒ 显式报错，不猜 ✓）。
pub(super) fn layer_type_of(
    ctx: &ToolContext<'_>,
    layer_id: &str,
) -> Result<yanshi_core::LayerType> {
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let layer = document.state().layers.get(layer_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id} 不存在")),
        )
    })?;
    Ok(layer.layer_type)
}

/// 要求目标图层是矢量图层（不是 ⇒ 显式报错并指路 ✓）。
pub(super) fn require_vector_layer(ctx: &ToolContext<'_>, layer_id: &str) -> Result<()> {
    match layer_type_of(ctx, layer_id)? {
        yanshi_core::LayerType::Vector => Ok(()),
        other => Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "图层 {layer_id} 是 {other:?} 图层、不是矢量图层 => 矢量工具不作用于它（建矢量层请用 create_layer 的 kind 参数传 vector）"
            )),
        )),
    }
}

/// **矢量笔触的写侧校验**（`draw_stroke` 落到矢量图层时 ✓）：只接受能被
/// 脊线 + 剖面**完整表示**的笔参数；表示不了的逐项列出、整笔拒绝 ✓
/// （不接受一半、丢掉一半 ✗——那正是静默降级 ✓）。
///
/// 接受：size/color/opacity/flow/hardness/spacing/pressure_size/
/// min_size_ratio/seed/dash/smooth/points（dash 的间断由 dab 簇天然表达 ✓）。
/// 拒绝：无 CSS 对应的混合模式（add/subtract）、不认识的混合模式名、
/// appearance（曲线/动力学/纹理/载墨）、jitter > 0、symmetry ≠ none、
/// medium（介质笔走位图通路、不是脊线对象 ✗）。
pub(super) fn validate_vector_stroke_data(layer_id: &str, data: &Value) -> Result<()> {
    let mut violations: Vec<String> = Vec::new();
    if let Some(mode_name) = data.get("blend_mode").and_then(Value::as_str) {
        let mode = yanshi_render::BlendMode::from_name(mode_name);
        if mode.as_str() != mode_name {
            violations.push(format!(
                "blend_mode=「{mode_name}」不是可识别的混合模式名（会被静默当成 normal ✗）"
            ));
        } else if !yanshi_render::vector::native_blend_supported(mode) {
            violations.push(format!(
                "blend_mode=「{mode_name}」没有 CSS 对应、SVG 表达不了"
            ));
        }
    }
    if yanshi_render::brush::StrokeAppearance::from_data(data).active {
        violations.push(
            "appearance（曲线/动力学/纹理/载墨）启用了：它按像素生效、不是脊线 + 剖面能表示的"
                .to_owned(),
        );
    }
    if data
        .get("jitter")
        .and_then(Value::as_f64)
        .is_some_and(|jitter| jitter > 0.0)
    {
        violations.push("jitter > 0：抖动是逐 dab 的像素偏移、会让脊线与墨迹对不上".to_owned());
    }
    if let Some(symmetry) = data.get("symmetry").and_then(Value::as_str) {
        if symmetry != "none" {
            violations.push(format!(
                "symmetry=「{symmetry}」：镜像半不在存下的脊线里、命中/擦除会漏掉它"
            ));
        }
    }
    if data.get("medium").is_some() {
        violations.push("medium：介质笔走位图补丁通路、不是矢量笔触对象".to_owned());
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "矢量图层 {layer_id} 不接受这笔：{}",
                violations.join("；")
            )),
        ))
    }
}

/// **光栅通路工具落到矢量图层的守卫**（在 `dispatch_inner` 入口统一拦 ✓）：
/// 这些工具按像素工作（介质笔/像素擦除/蒙版涂抹/smart 像素族/位图导入），
/// 对矢量图层执行等于把矢量语义**静默**换成像素语义 ✗ ⇒ 显式拒绝并指路。
/// 只拦带顶层 `layer_id` 且能查到图层的调用（查不到 ⇒ 交给工具自己报错 ✓）。
pub(super) fn vector_layer_tool_guard(
    tool: &str,
    ctx: &ToolContext<'_>,
    args: &Value,
) -> Option<YanshiError> {
    const RASTER_ONLY_TOOLS: &[&str] = &[
        "brush_stroke",
        "medium_stroke",
        "erase",
        "fill",
        "gradient_fill",
        "paint_mask",
        "smart_sample",
        "smart_fill",
        "smart_blend",
        "smart_glaze",
        "smart_edge_soften",
        "smart_texture",
        "smart_hatch_follow",
        "smart_erase_shape",
        "import_image",
    ];
    if !RASTER_ONLY_TOOLS.contains(&tool) {
        return None;
    }
    let layer_id = args.get("layer_id").and_then(Value::as_str)?;
    if !matches!(
        layer_type_of(ctx, layer_id),
        Ok(yanshi_core::LayerType::Vector)
    ) {
        return None;
    }
    Some(YanshiError::new(
        ErrorCode::InvalidArgument,
        ErrorContext::detail(format!(
            "工具 {tool} 是光栅通路、不能作用于矢量图层 {layer_id}（无静默降级：本次调用未执行 ✓）。矢量层请用 draw_stroke 落笔、vector_erase 按脊线擦除"
        )),
    ))
}

/// 把点列参数解析成几何（复用渲染端的解析 ⇒ 接受 `[[x, y, pressure?], …]`
/// 与 `[{x, y, pressure}, …]` 两种写法、与 `draw_stroke` 同口径 ✓）。
fn parse_points_arg(args: &Value, key: &str) -> Result<Vec<yanshi_render::brush::StrokePoint>> {
    let raw = args.get(key).ok_or_else(|| missing(key))?;
    let geometry = yanshi_render::brush::StrokeGeometry::from_value(&json!({ "points": raw }))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{key} 解析不出点列（要 [[x, y], …]、至少一点）")),
            )
        })?;
    Ok(geometry.points)
}

/// 一段脊线段列序列化回对象 `data.points` 的形状（`[[x, y, pressure], …]` ✓）。
fn points_to_value(points: &[yanshi_render::brush::StrokePoint]) -> Value {
    Value::Array(
        points
            .iter()
            .map(|p| json!([p.x, p.y, p.pressure]))
            .collect(),
    )
}

/// **矢量橡皮**（脊线截断 ✓）：擦除轨迹 + 半径扫过的带内的脊线区段被切掉，
/// 一笔可被截成多段（首段以 `supersede` 写回原对象、其余段新建对象、
/// 整笔擦光的走 tombstone ✓）。逐笔结果计数如实返回；橡皮没碰到的笔
/// **不产生任何原子** ✓。
pub(super) fn write_vector_erase(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    require_vector_layer(ctx, &layer_id)?;
    let eraser = parse_points_arg(args, "points")?;
    let radius = args
        .get("radius")
        .and_then(Value::as_f64)
        .ok_or_else(|| missing("radius"))?;
    if radius < 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("radius 不能为负"),
        ));
    }

    // 先快照要处理的对象（提交原子会改状态、不能边读边写 ✓）。
    let snapshot: Vec<(String, Value)> = {
        let state = document_state(ctx)?;
        state
            .objects_in_layer(&layer_id)
            .iter()
            .filter(|object| object.object_type == yanshi_core::ObjectType::Stroke)
            .map(|object| (object.id.clone(), object.data.clone()))
            .collect()
    };
    let strokes_total = snapshot.len();

    let mut strokes_erased = 0usize;
    let mut strokes_truncated = 0usize;
    let mut strokes_unchanged = 0usize;
    let mut strokes_unparseable = 0usize;
    let mut samples_erased_total = 0usize;
    let mut erased_object_ids: Vec<String> = Vec::new();
    let mut created_object_ids: Vec<String> = Vec::new();
    let mut last_result: Option<CommitResult> = None;

    for (object_id, data) in &snapshot {
        let Some(geometry) = yanshi_render::brush::StrokeGeometry::from_value(data) else {
            strokes_unparseable += 1;
            continue;
        };
        let brush = yanshi_render::brush::BrushSpec::from_value(data);
        let outcome = yanshi_render::vector::erase_spine(&brush, &geometry, &eraser, radius);
        samples_erased_total += outcome.samples_erased;
        if !outcome.changed {
            strokes_unchanged += 1;
            continue;
        }
        if outcome.pieces.is_empty() {
            last_result = Some(ctx.commit(AtomKind::Tombstone, json!({ "object_id": object_id }))?);
            strokes_erased += 1;
            erased_object_ids.push(object_id.clone());
            continue;
        }
        strokes_truncated += 1;
        // 首段写回原对象（其余 data 字段逐字保留、只换 points ✓；
        // smooth 的笔已被物化成采样点列 ⇒ 必须关掉 smooth 防二次平滑 ✓）。
        let mut first_data = data.clone();
        first_data["points"] = points_to_value(&outcome.pieces[0]);
        if geometry.smooth {
            first_data["smooth"] = json!(false);
        }
        last_result = Some(ctx.commit(
            AtomKind::Supersede,
            json!({ "object_id": object_id, "data": first_data }),
        )?);
        // 其余段新建对象（剖面参数与原笔逐字相同、只换 points ✓）。
        for piece in &outcome.pieces[1..] {
            let new_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
            let mut piece_data = data.clone();
            piece_data["points"] = points_to_value(piece);
            if geometry.smooth {
                piece_data["smooth"] = json!(false);
            }
            last_result = Some(ctx.commit(
                AtomKind::DrawStroke,
                json!({
                    "object_id": new_id,
                    "layer_id": layer_id,
                    "data": piece_data,
                }),
            )?);
            created_object_ids.push(new_id);
        }
    }

    let counts = json!({
        "layer_id": layer_id,
        "strokes_total": strokes_total,
        "strokes_erased": strokes_erased,
        "strokes_truncated": strokes_truncated,
        "strokes_unchanged": strokes_unchanged,
        "strokes_unparseable": strokes_unparseable,
        "samples_erased": samples_erased_total,
        "erased_object_ids": erased_object_ids,
        "created_object_ids": created_object_ids,
        "changed": strokes_erased + strokes_truncated > 0,
    });
    match last_result {
        Some(result) => {
            let region = region_of(&result);
            let mut value = finish_mutation(ctx, &result, region)?;
            value["vector_erase"] = counts;
            Ok(value)
        }
        // 一笔都没碰到：没有原子、没有预览可附 ⇒ 如实返回计数 ✓
        //（changed = false 写在里面、不假装做了修改 ✓）。
        None => Ok(counts),
    }
}

/// **矢量笔触命中测试**（只读 ✓）：给定文档坐标点，返回该矢量图层里
/// 所有被命中的笔触（按余量升序、同余量按 id 稳定排序 ⇒ 确定性 ✓）。
/// 判定 = 点到脊线的距离 ≤ 该点剖面半径 + tolerance（工具面缺省 0 ✓）。
pub(super) fn read_vector_hit_test(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    require_vector_layer(ctx, &layer_id)?;
    let x = args
        .get("x")
        .and_then(Value::as_f64)
        .ok_or_else(|| missing("x"))?;
    let y = args
        .get("y")
        .and_then(Value::as_f64)
        .ok_or_else(|| missing("y"))?;
    let tolerance = args.get("tolerance").and_then(Value::as_f64).unwrap_or(0.0);
    if tolerance < 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("tolerance 不能为负"),
        ));
    }
    let state = document_state(ctx)?;
    let mut strokes_examined = 0usize;
    let mut hits: Vec<Value> = Vec::new();
    for object in state.objects_in_layer(&layer_id) {
        if object.object_type != yanshi_core::ObjectType::Stroke {
            continue;
        }
        strokes_examined += 1;
        let Some(geometry) = yanshi_render::brush::StrokeGeometry::from_value(&object.data) else {
            continue;
        };
        let brush = yanshi_render::brush::BrushSpec::from_value(&object.data);
        if let Some(hit) = yanshi_render::vector::spine_hit(&brush, &geometry, (x, y)) {
            if hit.clearance <= tolerance {
                hits.push(json!({
                    "object_id": object.id,
                    "distance": hit.distance,
                    "radius": hit.radius,
                    "clearance": hit.clearance,
                }));
            }
        }
    }
    hits.sort_by(|a, b| {
        let ca = a["clearance"].as_f64().unwrap_or(0.0);
        let cb = b["clearance"].as_f64().unwrap_or(0.0);
        ca.partial_cmp(&cb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a["object_id"].as_str().cmp(&b["object_id"].as_str()))
    });
    Ok(json!({
        "layer_id": layer_id,
        "x": x,
        "y": y,
        "tolerance": tolerance,
        "strokes_examined": strokes_examined,
        "hits": hits,
        "count": hits.len(),
    }))
}
