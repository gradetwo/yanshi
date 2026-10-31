//! **蒙版工具**（`create_mask` / `paint_mask` / `delete_mask`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2**，`tools.rs` 拆分）：
//! `tools.rs` 已有 **30777 行** ⇒ 超出任何单文件可维护规模（第三轮复审也提了这条）。
//! 按工具族拆 ⇒ 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **依赖**：用 `super::*` 拿到 `tools.rs` 里的公共 helper（同 crate 子模块可访问私有项）⇒
//! 所以搬运不需要改那些 helper 的可见性 ✓

use super::*;

/// 蒙版画笔/擦除：在蒙版上按笔刷沉积灰度 patch（`UpdateMask` 原子）。
///
/// - 笔刷强制纯白，只取沉积的 Alpha 通道（颜色恒定白，避免色相污染灰度）。
/// - 合成用 Target-Value Lerp：`m_new = div255(m_old*(255-d) + target*d)`，
///   其中 `d` 为笔刷沉积度，`target` 为本次目标灰度。
/// - patch 按沉积的紧致 bbox 裁剪，L8 PNG 存 blob。
/// - 同一蒙版累积 patch 达 32 个时，自动烘焙 ReplaceAll 快照截断重放链
///   （历史 blob 仍在原子日志里，撤销无损）。
pub(super) fn write_paint_mask(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mask_id = require_str(args, "mask_id")?;
    let mut points = parse_brush_points(args)?;
    // **★ 第 1188 轮恢复**：补丁把这段 nudge 删了（说「已下沉到 brush_deposit_mask」）✗
    //   ⇒ 而实测 `paint_mask_paint_restores_content` 回归：`mode:"erase"` + 单点 ⇒
    //     `deposited: false`（`note: 笔刷沉积为空，未提交原子`）
    //   ⇒ 所以「下沉」没有覆盖 `paint_mask` 的这条路径 ⇒ **恢复原 nudge** ✓
    //   （补丁在 `brush_deposit_mask` 里加的兜底**保留**：两处都兜比一处更稳，
    //    且都只在「单点」时触发 ⇒ 不会重复加位移，因为 `write_paint_mask` 恢复后
    //    points.len() 已是 2 ⇒ `brush_deposit_mask` 不再走 len==1 分支 ✓）
    // 单点笔触：引擎按 dab 间距（≈ size/4）落笔，零位移下不沉积 ⇒
    // 补一段短位移保证单点即落一枚 dab（位移 < 笔刷半径，视觉上仍是点）。
    if points.len() == 1 {
        let (x, y, p) = points[0];
        let size = args.get("size").and_then(Value::as_f64).unwrap_or(20.0);
        // **统一口径** ✓（第 640 轮；外部 D1）：三处各写一个数 ⇒ 大笔刷那两处必然失效 ✗
        let nudge = crate::tools::deposit_nudge(size);
        points.push((x + nudge, y, p));
    }
    let mode = optional_str(args, "mode").unwrap_or_else(|| "paint".to_owned());
    if mode != "paint" && mode != "erase" {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("mode 必须是 paint/erase，得到 {mode}")),
        ));
    }
    // 目标灰度：mask_value 显式覆盖，否则 paint=1.0 / erase=0.0。
    let default_value = if mode == "paint" { 1.0 } else { 0.0 };
    let mask_value = args
        .get("mask_value")
        .and_then(Value::as_f64)
        .unwrap_or(default_value);
    if !(0.0..=1.0).contains(&mask_value) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("mask_value 必须在 [0, 1] 内，得到 {mask_value}")),
        ));
    }
    let target_u8 = (mask_value * 255.0).round() as u8;

    // 笔刷：preset 或默认圆笔；强制纯白；size/softness/opacity/flow 覆盖。
    // （覆盖语义与 paint_brush 一致：调用方给的是"这一笔用什么"。）
    // 无 preset 时用基础圆笔：`Brush::new()` 全零设置不会落笔，
    // 必须显式给出关键设置（对标真实 .myb 的非零项）。
    let mut brush = match optional_str(args, "preset") {
        Some(preset) => load_brush(ctx.workspace, &preset)?.1,
        None => {
            let mut b = hokusai::Brush::new();
            b.set(
                hokusai::BrushSetting::DabsPerBasicRadius,
                hokusai::SettingValue::constant(2.0),
            );
            b.set(
                hokusai::BrushSetting::DabsPerActualRadius,
                hokusai::SettingValue::constant(2.0),
            );
            b.set(
                hokusai::BrushSetting::EllipticalDabRatio,
                hokusai::SettingValue::constant(1.0),
            );
            b.set(
                hokusai::BrushSetting::AntiAliasing,
                hokusai::SettingValue::constant(1.0),
            );
            b
        }
    };
    brush.set(
        hokusai::BrushSetting::ColorH,
        hokusai::SettingValue::constant(0.0),
    );
    brush.set(
        hokusai::BrushSetting::ColorS,
        hokusai::SettingValue::constant(0.0),
    );
    brush.set(
        hokusai::BrushSetting::ColorV,
        hokusai::SettingValue::constant(1.0),
    );
    let size = args.get("size").and_then(Value::as_f64).unwrap_or(20.0);
    if size <= 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("size 必须大于 0，得到 {size}")),
        ));
    }
    brush.set(
        hokusai::BrushSetting::Radius,
        hokusai::SettingValue::constant((size / 2.0).ln() as f32),
    );
    let softness = args
        .get("softness")
        .and_then(Value::as_f64)
        .unwrap_or(0.5)
        .clamp(0.0, 1.0);
    brush.set(
        hokusai::BrushSetting::Hardness,
        hokusai::SettingValue::constant((1.0 - softness) as f32),
    );
    let opacity = args
        .get("opacity")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let flow = args
        .get("flow")
        .and_then(Value::as_f64)
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    // flow 与 opacity 相乘得到每 dab 实际不透明度。
    brush.set(
        hokusai::BrushSetting::Opaque,
        hokusai::SettingValue::constant((opacity * flow) as f32),
    );

    if args
        .get("humanize")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        apply_humanize_envelope(&mut points);
    }

    // 落笔沉积（`brush_deposit_mask` 返回 RGBA，只取 Alpha 通道）。
    let region = PaintRegion::from_bounds(brush_stroke_region(&points, Some(size)));
    let region_width = region.width;
    let region_height = region.height;
    let region_x0 = region.x0;
    let region_y0 = region.y0;
    // `paint_mask` 这条路**没有真实时间戳** ⇒ 传空切片（= 老行为，逐字节不变）✓
    let rgba = brush_deposit_mask(&brush, size, &points, region, &[]);
    let mut deposit = vec![0u8; region_width * region_height];
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        deposit[index] = pixel[3];
    }

    // 紧致裁剪：找沉积非零的 bbox（region 局部坐标）。
    let Some((tx0, ty0, tx1, ty1)) = tight_bbox_u8(&deposit, region_width, region_height) else {
        // 零沉积不提交（静默提交空 patch 是已知 bug 形态，必须可观测）。
        return Ok(json!({
            "ok": true,
            "mask_id": mask_id,
            "deposited": false,
            "note": "笔刷沉积为空，未提交原子",
        }));
    };
    let tw = tx1 - tx0;
    let th = ty1 - ty0;
    let mut trimmed = vec![0u8; tw * th];
    for y in 0..th {
        for x in 0..tw {
            trimmed[y * tw + x] = deposit[(ty0 + y) * region_width + (tx0 + x)];
        }
    }
    // 转画布坐标。
    let canvas_tx0 = region_x0 + tx0 as i32;
    let canvas_ty0 = region_y0 + ty0 as i32;

    let state = document_state(ctx)?;
    // Q2 双表寻址：蒙版表优先，选区表次之（`paint_mask` 可直接修整选区边缘，
    // 即 Photoshop 的「快速蒙版」语义；`Mask = Selection` 同构体）。
    let mask = state
        .masks
        .get(&mask_id)
        .or_else(|| state.selections.get(&mask_id))
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("蒙版或选区 {mask_id} 不存在")),
            )
        })?;

    // 32 笔快照合并：烘焙当前完整蒙版 + 本笔沉积为 ReplaceAll。
    let (op, bbox, png_bytes) = if mask.patches.len() >= 32 {
        match bake_mask_snapshot(
            ctx, &state, mask, &trimmed, tw, th, canvas_tx0, canvas_ty0, target_u8,
        )? {
            Some(snapshot) => snapshot,
            // 快照为空（全默认值）：退化为普通增量 patch，下次再试合并。
            None => {
                let png = yanshi_render::png::encode_png_l8(tw as u32, th as u32, &trimmed)
                    .expect("内存 PNG 编码不会失败");
                (
                    "lerp_deposit",
                    [canvas_tx0, canvas_ty0, tw as i32, th as i32],
                    png,
                )
            }
        }
    } else {
        let png = yanshi_render::png::encode_png_l8(tw as u32, th as u32, &trimmed)
            .expect("内存 PNG 编码不会失败");
        (
            "lerp_deposit",
            [canvas_tx0, canvas_ty0, tw as i32, th as i32],
            png,
        )
    };

    let blob_hash = ctx.workspace.store().put(&png_bytes)?;
    let blob_str = blob_hash.to_string();
    let payload = json!({
        "mask_id": mask_id,
        "patch": {
            "bbox": bbox,
            "target_u8": target_u8,
            "op": op,
            "blob": blob_str,
        },
        "refs": { "blobs": [blob_str] },
    });
    let result = ctx.commit(AtomKind::UpdateMask, payload)?;
    let region = region_of(&result);
    let mut response = finish_mutation(ctx, &result, region)?;
    response["deposited"] = json!(true);
    response["patch_op"] = json!(op);
    Ok(response)
}

/// `bake_mask_snapshot`：把当前完整蒙版与本笔沉积烘焙为一张 ReplaceAll 快照。
///
/// 返回 `(op, bbox, png)`；若结果全为默认值则返回 `None`（调用方退化处理）。
#[allow(clippy::too_many_arguments)]
/// 蒙版快照的一段：`(编码名, bbox, 字节)`。抽出来是为了 clippy 的 `type_complexity`。
type MaskSnapshotBlob = (&'static str, [i32; 4], Vec<u8>);
// 蒙版快照烘焙要 bbox/图层/蒙版/编码/阈值等 9 个入参；拆结构体只是搬家。
#[allow(clippy::too_many_arguments)]
fn bake_mask_snapshot(
    ctx: &mut ToolContext<'_>,
    state: &DocumentState,
    mask: &yanshi_core::Selection,
    trimmed: &[u8],
    tw: usize,
    th: usize,
    canvas_tx0: i32,
    canvas_ty0: i32,
    target_u8: u8,
) -> Result<Option<MaskSnapshotBlob>> {
    use yanshi_render::selection::{div255, CompiledMask};
    let load_blob = |s: &str| {
        s.parse::<yanshi_core::BlobHash>()
            .ok()
            .and_then(|hash| ctx.workspace.store().get(&hash).ok())
    };
    let compiled = CompiledMask::build(mask, state.width as i32, state.height as i32, &load_blob);
    let default_v: u8 = if mask.invert { 255 } else { 0 };

    // 扩展网格：compiled.bbox ∪ 本笔 bbox，填默认值。
    let [cbx, cby, cbw, cbh] = compiled.bbox;
    let sx0 = cbx.min(canvas_tx0);
    let sy0 = cby.min(canvas_ty0);
    let sx1 = (cbx + cbw).max(canvas_tx0 + tw as i32);
    let sy1 = (cby + cbh).max(canvas_ty0 + th as i32);
    let sw = (sx1 - sx0).max(0) as usize;
    let sh = (sy1 - sy0).max(0) as usize;
    if sw == 0 || sh == 0 {
        return Ok(None);
    }
    let mut grid = vec![default_v; sw * sh];
    for y in 0..cbh.max(0) as usize {
        for x in 0..cbw.max(0) as usize {
            let dst = ((cby - sy0) as usize + y) * sw + ((cbx - sx0) as usize + x);
            grid[dst] = compiled.grid[y * (cbw as usize) + x];
        }
    }
    // 本笔沉积按 Target-Value Lerp 合进去。
    let target = u32::from(target_u8);
    for y in 0..th {
        for x in 0..tw {
            let gx = canvas_tx0 + x as i32;
            let gy = canvas_ty0 + y as i32;
            let dst = ((gy - sy0) as usize) * sw + ((gx - sx0) as usize);
            let m_old = u32::from(grid[dst]);
            let d = u32::from(trimmed[y * tw + x]);
            grid[dst] = div255(m_old * (255 - d) + target * d);
        }
    }
    // 紧致裁剪到非默认值的区域。
    let Some((bx0, by0, bx1, by1)) = tight_bbox_u8_ne(&grid, sw, sh, default_v) else {
        return Ok(None);
    };
    let bw = bx1 - bx0;
    let bh = by1 - by0;
    let mut snap = vec![0u8; bw * bh];
    for y in 0..bh {
        for x in 0..bw {
            snap[y * bw + x] = grid[(by0 + y) * sw + (bx0 + x)];
        }
    }
    let png = yanshi_render::png::encode_png_l8(bw as u32, bh as u32, &snap)
        .expect("内存 PNG 编码不会失败");
    Ok(Some((
        "replace_all",
        [sx0 + bx0 as i32, sy0 + by0 as i32, bw as i32, bh as i32],
        png,
    )))
}

/// u8 网格中非零像素的紧致 bbox（网格局部坐标），全零返回 `None`。
fn tight_bbox_u8(grid: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    tight_bbox_u8_ne(grid, w, h, 0)
}

/// u8 网格中不等于 `neq` 的像素的紧致 bbox（网格局部坐标），全等于返回 `None`。
fn tight_bbox_u8_ne(
    grid: &[u8],
    w: usize,
    h: usize,
    neq: u8,
) -> Option<(usize, usize, usize, usize)> {
    let mut x0 = usize::MAX;
    let mut y0 = usize::MAX;
    let mut x1 = 0usize;
    let mut y1 = 0usize;
    for y in 0..h {
        for x in 0..w {
            if grid[y * w + x] != neq {
                if x < x0 {
                    x0 = x;
                }
                if y < y0 {
                    y0 = y;
                }
                if x + 1 > x1 {
                    x1 = x + 1;
                }
                if y + 1 > y1 {
                    y1 = y + 1;
                }
            }
        }
    }
    if x0 == usize::MAX {
        None
    } else {
        Some((x0, y0, x1, y1))
    }
}

/// 删除蒙版（tombstone；不清空 blobs/patches，撤销可无损回退）。
pub(super) fn write_delete_mask(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mask_id = require_str(args, "mask_id")?;
    let payload = json!({ "mask_id": mask_id });
    let result = ctx.commit(AtomKind::DeleteMask, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// `create_mask` 的 path 形状校验（二期：mask 源 = path）。
///
/// 与渲染层共用同一份解析（`object::parse_path_subpaths`，禁止再写第二套）：
/// 内联 `subpaths` 或 `object_id` 引用已有 path 形状对象，二选一；引用在
/// 工具层先验存在性/存活/类型，让「建得出来 ⇒ 渲得出」在入口就成立
/// （渲染层对失效引用另有告警兜底，防的是文档被其它方式改坏）。
pub(super) fn validate_mask_path_shape(ctx: &mut ToolContext<'_>, shape: &Value) -> Result<()> {
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    let inline = shape.get("subpaths");
    let object_id = shape.get("object_id").and_then(Value::as_str);
    match (inline, object_id) {
        (Some(_), Some(_)) => Err(invalid(
            "shape.subpaths 与 shape.object_id 只能给一个（二选一）".to_owned(),
        )),
        (None, None) => Err(invalid(
            "kind=path 的蒙版 shape 必须给 subpaths（内联子路径）、d（SVG path 字符串）或 object_id（引用已有 path 形状）之一"
                .to_owned(),
        )),
        (Some(_), None) => {
            yanshi_render::object::parse_path_subpaths(shape)
                .map_err(|reason| invalid(format!("shape 非法：{reason}")))?;
            Ok(())
        }
        (None, Some(id)) => {
            let state = document_state(ctx)?;
            let Some(object) = state.objects.get(id) else {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "shape.object_id 引用的对象 {id} 不存在"
                    )),
                ));
            };
            if object.deleted_by.is_some() {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "shape.object_id 引用的对象 {id} 已被删除"
                    )),
                ));
            }
            match yanshi_render::object::parse_object(object) {
                yanshi_render::object::Primitive::Shape {
                    kind: yanshi_render::object::ShapeKind::Path,
                    ..
                } => Ok(()),
                _ => Err(invalid(format!(
                    "shape.object_id 引用的对象 {id} 不是 path 形状（只认 draw_shape 建的 kind=path 对象）"
                ))),
            }
        }
    }
}

/// `create_mask` 的形状 kind 白名单校验（D1-b）+ 渐变 kind 的形状参数校验。
///
/// 渲染层对未知 kind 静默退化为覆盖全部（`SelectionGeom::parse_geom` 的安全侧选择，
/// 选区路径需要它）；但蒙版入口必须把拼错的名字拦下来，否则「拼错 kind ⇒ 全画布
/// 蒙版」是静默降级。渐变 kind 的参数同样在入口验完（与 kind=path 同例：
/// 建得出来 ⇒ 渲得出）。
fn validate_mask_shape_kind(kind: &str, shape: &Value) -> Result<()> {
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    match kind {
        // 解析器认的几何 kind 全集（`selection.rs parse_geom`）：circle/lasso 是
        // ellipse/polygon 的同义词，照认。
        "rect" | "ellipse" | "circle" | "polygon" | "lasso" | "path" | "bitmap" => Ok(()),
        "linear_gradient" => {
            if !shape_point_ok(shape.get("start")) || !shape_point_ok(shape.get("end")) {
                return Err(invalid(
                    "linear_gradient 的 shape 必须给 start/end（[x, y] 数组或 {x, y} 对象，均为有限数）"
                        .to_owned(),
                ));
            }
            let (sx, sy) = shape_point_coords(shape.get("start").expect("已验存在"));
            let (ex, ey) = shape_point_coords(shape.get("end").expect("已验存在"));
            if sx == ex && sy == ey {
                return Err(invalid(
                    "linear_gradient 的 start 与 end 重合（零长轴），渐变没有方向".to_owned(),
                ));
            }
            Ok(())
        }
        "radial_gradient" => {
            let bbox = shape.get("bbox").and_then(Value::as_object);
            let Some(bbox) = bbox else {
                return Err(invalid(
                    "radial_gradient 的 shape 必须给 bbox（{x, y, w, h}，w/h 大于 0）".to_owned(),
                ));
            };
            let w = bbox
                .get("w")
                .or_else(|| bbox.get("width"))
                .and_then(Value::as_f64);
            let h = bbox
                .get("h")
                .or_else(|| bbox.get("height"))
                .and_then(Value::as_f64);
            if !matches!((w, h), (Some(w), Some(h)) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0)
            {
                return Err(invalid(
                    "radial_gradient 的 bbox 宽高必须大于 0".to_owned(),
                ));
            }
            if let Some(falloff) = shape.get("falloff").and_then(Value::as_f64) {
                if !falloff.is_finite() || !(0.0..=1.0).contains(&falloff) {
                    return Err(invalid(format!(
                        "radial_gradient 的 falloff 必须在 [0, 1] 内，得到 {falloff}"
                    )));
                }
            }
            Ok(())
        }
        other => Err(invalid(format!(
            "未知蒙版形状 kind「{other}」⇒ 可用：rect / ellipse / polygon / path / bitmap / linear_gradient / radial_gradient"
        ))),
    }
}

/// 单点是否合法：`[x, y]` 数组或 `{x, y}` 对象，坐标均为有限数
///（与 `selection.rs parse_point` 同口径）。
fn shape_point_ok(point: Option<&Value>) -> bool {
    let Some(point) = point else {
        return false;
    };
    if let Some(pair) = point.as_array() {
        return pair.len() >= 2
            && pair[0].as_f64().is_some_and(|x| x.is_finite())
            && pair[1].as_f64().is_some_and(|y| y.is_finite());
    }
    if let Some(object) = point.as_object() {
        return object
            .get("x")
            .and_then(Value::as_f64)
            .is_some_and(|x| x.is_finite())
            && object
                .get("y")
                .and_then(Value::as_f64)
                .is_some_and(|y| y.is_finite());
    }
    false
}

/// `create_mask` 的 rect/ellipse/circle 形状字段校验（静默空覆盖修补）。
///
/// 渲染层 `parse_geom` 只认两组写法：`bbox` 对象（`{x, y, w|width, h|height}`）
/// 或顶层 `x/y/w/h`；其余写法一律不被采纳——
/// - `x0/y0/x1/y1`（角点式）：图层蒙版渲染（`parse_shape_coverage`）读不到
///   bbox ⇒ 零面积矩形 ⇒ 全 0（字面零覆盖，盒上探针实测）；
/// - `cx/cy/rx/ry`（SVG 椭圆式）：选区路径会归一成 bbox，但蒙版入口没有归一
///   ⇒ 图层蒙版渲染同样读不到 bbox ⇒ 全 0；另注意选区/CompiledMask 路径
///   对同一形状会退化为覆盖全部（两路径口径不一致，入口校验把两门一并关上）；
/// - 零/负/非有限宽高：`has_area` 为假 ⇒ 以前静默退化为空。
///   三者都是「`ok=true` 但蒙版不是调用方要的形状」，故在入口具名报错。
///   只验显式 `shape`：`from_selection` 克隆来的形状是选区路径已认的合法形态
///   （选区本就接受 `cx/cy/rx/ry`），那条门的归一缺口另案处理，本函数不拦它；
///   渲染层 `parse_geom` 与选区路径一律不动。
fn validate_mask_rect_ellipse_fields(shape: &Value, kind: &str) -> Result<()> {
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    // “看起来像几何、但解析器不认”的字段：无论 bbox 是否有效，只要出现就说明
    // 调用方有字段没被采纳，必须点名，不能静默忽略（顶层与 bbox 对象内都查）。
    const LOOKALIKES: [&str; 10] = ["x0", "y0", "x1", "y1", "x2", "y2", "cx", "cy", "rx", "ry"];
    let mut found: Vec<&str> = Vec::new();
    for key in LOOKALIKES {
        if shape.get(key).is_some()
            || shape
                .get("bbox")
                .and_then(Value::as_object)
                .is_some_and(|bbox| bbox.contains_key(key))
        {
            found.push(key);
        }
    }
    if !found.is_empty() {
        return Err(invalid(format!(
            "{kind} 的 shape 带 {}，但解析器只认 bbox{{x, y, w|width, h|height}} 或顶层 x/y/w/h（{} 不被采纳）",
            found.join("、"),
            found.join("、")
        )));
    }
    // 解析器读 bbox 的口径（与 `parse_bbox` 同源）：`bbox` 为对象时只读它，
    // 否则读顶层。`bbox` 给了非对象值时同样按顶层读——那本身就是错字段，
    // 在缺失报错里一并点名。
    let (source, source_name) = match shape.get("bbox").filter(|v| v.is_object()) {
        Some(bbox) => (bbox, "bbox"),
        None => (shape, "顶层"),
    };
    let num = |value: &Value, key: &str| value.get(key).and_then(Value::as_f64);
    let x = num(source, "x");
    let y = num(source, "y");
    let w = num(source, "w").or_else(|| num(source, "width"));
    let h = num(source, "h").or_else(|| num(source, "height"));
    match (x, y, w, h) {
        (Some(x), Some(y), Some(w), Some(h)) => {
            if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
                return Err(invalid(format!(
                    "{kind} 的 {source_name} 坐标/宽高必须为有限数，得到 x={x} y={y} w={w} h={h}"
                )));
            }
            if w <= 0.0 || h <= 0.0 {
                return Err(invalid(format!(
                    "{kind} 的 {source_name} 宽高必须大于 0（零/负面积不许静默成空），得到 w={w} h={h}"
                )));
            }
            Ok(())
        }
        _ => {
            let mut missing: Vec<&str> = Vec::new();
            if x.is_none() {
                missing.push("x");
            }
            if y.is_none() {
                missing.push("y");
            }
            if w.is_none() {
                missing.push("w|width");
            }
            if h.is_none() {
                missing.push("h|height");
            }
            let mut message = format!(
                "{kind} 的 shape 缺 {source_name} 几何：缺 {}，须给 bbox{{x, y, w|width, h|height}} 或顶层 x/y/w/h（均为有限数、宽高大于 0）",
                missing.join("、")
            );
            if shape.get("bbox").is_some_and(|v| !v.is_object()) {
                message.push_str("（bbox 给了非对象值，须为 {x, y, w|width, h|height} 对象）");
            }
            Err(invalid(message))
        }
    }
}

/// 单点坐标（调用方已验 `shape_point_ok`，此处缺失分量按 0 计）。
fn shape_point_coords(point: &Value) -> (f64, f64) {
    if let Some(pair) = point.as_array() {
        return (
            pair[0].as_f64().unwrap_or(0.0),
            pair[1].as_f64().unwrap_or(0.0),
        );
    }
    (
        point.get("x").and_then(Value::as_f64).unwrap_or(0.0),
        point.get("y").and_then(Value::as_f64).unwrap_or(0.0),
    )
}

pub(super) fn write_create_mask(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mask_id = require_str(args, "mask_id")?;
    // Q2：`from_selection` 一键转换（传 selection_id 或 "latest"）：克隆该选区的
    // shape/feather/invert/blobs/patches。显式传的 shape/feather/invert 优先。
    #[allow(clippy::type_complexity)]
    let from_sel: Option<(Value, f64, bool, Vec<String>, Vec<Value>)> =
        if let Some(sel_id_in) = optional_str(args, "from_selection") {
            let state = document_state(ctx)?;
            let sel_id = if sel_id_in == "latest" {
                let mut ordered: Vec<(&String, &yanshi_core::Selection)> = state
                    .selections
                    .iter()
                    .filter(|(_, s)| !s.is_deleted())
                    .collect();
                ordered.sort_by(|a, b| a.1.created_by.as_str().cmp(b.1.created_by.as_str()));
                ordered.last().map(|(k, _)| (*k).clone()).ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::ReferenceNotFound,
                        ErrorContext::detail("没有可转换的选区".to_owned()),
                    )
                })?
            } else {
                sel_id_in
            };
            let sel = state.selections.get(&sel_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("选区 {sel_id} 不存在")),
                )
            })?;
            let blobs: Vec<String> = sel.blobs.iter().map(|b| b.to_string()).collect();
            let patches: Vec<Value> = sel
                .patches
                .iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect();
            Some((sel.shape.clone(), sel.feather, sel.invert, blobs, patches))
        } else {
            None
        };
    let mut shape = if let Some((s, _, _, _, _)) = &from_sel {
        if args.get("shape").and_then(Value::as_object).is_some() {
            require_object(args, "shape")?.clone()
        } else {
            s.clone()
        }
    } else {
        // **没给 `from_selection` 时 `shape` 才是必填** ✓：两条出路都要说出来 ✓
        // （只说「缺少必填参数 shape」会把「我其实想从选区转」的人带到错方向 ✓）。
        if args.get("shape").and_then(Value::as_object).is_none() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "缺少 shape（几何），或改用 from_selection（传 selection_id 或 \"latest\" 从已有选区转换，此时形状/羽化/反选/位图一并克隆）"
                        .to_owned(),
                ),
            ));
        }
        require_object(args, "shape")?.clone()
    };
    // kind/type 互为别名：只有 type 时按 type 走，并在入口把 kind 补进落库
    // 形状（渲染层只认 kind——只带 type 的 path 形状不归一化就会绕过下面的
    // 校验、且渲不出 path 覆盖率，是静默降级）。
    let effective_kind = shape
        .get("kind")
        .and_then(Value::as_str)
        .or_else(|| shape.get("type").and_then(Value::as_str))
        .map(str::to_owned);
    let Some(effective_kind) = effective_kind else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "shape 必须带 kind 或 type（rect/ellipse/polygon/path/bitmap/linear_gradient/radial_gradient），from_selection 时可省略"
                    .to_owned(),
            ),
        ));
    };
    if shape.get("kind").and_then(Value::as_str).is_none() {
        if let Some(obj) = shape.as_object_mut() {
            obj.insert("kind".to_owned(), Value::String(effective_kind.clone()));
        }
    }
    // 二期：kind=path 的蒙版形状在入口验完（内联/引用二选一 + 同一份解析），
    // 不许落库之后才在渲染层发现形状是坏的。
    if effective_kind == "path" {
        // `d` 语法糖（与 draw_shape normalize 同口径）：d 在入口转 subpaths
        // 并摘掉；与 subpaths 同给 ⇒ 二选一报错；语法坏 ⇒ 这里就报
        // （带字符位置），不静默落库。
        if let Some(d) = shape.get("d").and_then(Value::as_str).map(str::to_owned) {
            if shape.get("subpaths").is_some() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("shape 的 d 与 subpaths 只能给一个（二选一）".to_owned()),
                ));
            }
            let subpaths = super::tools_pathd::parse_svg_d(&d).map_err(|reason| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("shape 的 d 非法：{reason}")),
                )
            })?;
            if let Some(obj) = shape.as_object_mut() {
                obj.insert("subpaths".to_owned(), subpaths);
                obj.remove("d");
            }
        }
        validate_mask_path_shape(ctx, &shape)?;
    }
    // D1-b：kind 白名单 + 渐变形状参数在入口验完（与 kind=path 同例）。
    validate_mask_shape_kind(&effective_kind, &shape)?;
    // 静默空覆盖修补：rect/ellipse/circle 的显式形状字段在入口验完
    // （错字段名以前在图层蒙版渲染里静默成全 0）。`from_selection` 克隆来的
    // 形状不验（选区合法形态，那条门的归一缺口另案处理）。
    let shape_from_selection =
        from_sel.is_some() && args.get("shape").and_then(Value::as_object).is_none();
    if !shape_from_selection && matches!(effective_kind.as_str(), "rect" | "ellipse" | "circle") {
        validate_mask_rect_ellipse_fields(&shape, &effective_kind)?;
    }
    if matches!(
        effective_kind.as_str(),
        "linear_gradient" | "radial_gradient"
    ) {
        // 渐变 kind 的过渡带就是渐变本身（线性：start→end 的距离；径向：falloff 比例），
        // 顶层 feather 不作用（见 selection.rs）；传了非零 feather 必须报错，不能静默忽略。
        // 生效值 = 显式 feather，否则 from_selection 克隆来的 feather，否则 0。
        let effective_feather = args
            .get("feather")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| from_sel.as_ref().map(|(_, f, _, _, _)| *f).unwrap_or(0.0));
        if effective_feather != 0.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "渐变遮罩（{effective_kind}）不收 feather（过渡带就是渐变本身），得到 feather={effective_feather}"
                )),
            ));
        }
    }
    let feather = args
        .get("feather")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| from_sel.as_ref().map(|(_, f, _, _, _)| *f).unwrap_or(0.0));
    if !(0.0..=512.0).contains(&feather) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("feather 必须在 [0, 512] 内，得到 {feather}")),
        ));
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
    let mut payload = json!({
        "mask_id": mask_id,
        "shape": shape,
        "feather": feather,
        "mode": mode,
        "invert": args.get("invert").and_then(Value::as_bool).unwrap_or_else(|| {
            from_sel.as_ref().map(|(_, _, inv, _, _)| *inv).unwrap_or(false)
        }),
    });
    if let Some(linked) = optional_str(args, "linked_layer") {
        payload["linked_layer"] = json!(linked);
    }
    // Q2：from_selection 时携带选区的 blobs 与 patches。
    if let Some((_, _, _, blobs, patches)) = &from_sel {
        if !blobs.is_empty() {
            payload["refs"] = json!({ "blobs": blobs });
        }
        if !patches.is_empty() {
            payload["patches"] = Value::Array(patches.clone());
        }
    }
    let result = ctx.commit(AtomKind::CreateMask, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// 范围遮罩的烘焙参数（D1-a）：工具层解析一次，烘焙函数只读它。
enum RangeBake {
    /// 明度带：`low..=high`（0..1，sRGB 编码值的 Rec709 加权），
    /// `smoothness` 为带边软化（明度单位）。
    Luminance {
        low: f32,
        high: f32,
        smoothness: f32,
    },
    /// 色相带：中心色相 `hue`（度，0..360），窗全宽 `width`（度），
    /// `smoothness` 为窗宽中用作软边的比例（0..1）。
    Color {
        hue: f32,
        width: f32,
        smoothness: f32,
    },
}

/// `t` ∈ [0, 1] 上的 smoothstep（`3t² - 2t³`，与 `selection.rs` 同式）。
fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// sRGB 编码字节的显示空间明度（Rec709 加权，与 A 族显示空间语义一致）。
fn srgb_luma(r: u8, g: u8, b: u8) -> f32 {
    (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

/// 明度窗函数（D1-a）：`low..=high` 带内为 1，带外为 0，`smoothness`
/// 在上下边各做 smoothstep 过渡。`smoothness` 先钳到 `(high - low) / 2`，
/// 保证带内仍有平顶（否则窄带会被过渡吃掉、峰值到不了 1）。
fn luminance_window(luma: f32, low: f32, high: f32, smoothness: f32) -> f32 {
    let s = smoothness.clamp(0.0, (high - low) / 2.0);
    if s <= 0.0 {
        return if (low..=high).contains(&luma) {
            1.0
        } else {
            0.0
        };
    }
    let up = smoothstep01((luma - (low - s)) / (2.0 * s));
    let down = 1.0 - smoothstep01((luma - (high - s)) / (2.0 * s));
    (up * down).clamp(0.0, 1.0)
}

/// 像素色相（度，0..360）与 HSV 饱和度（0..1）：标准 max/min 公式。
/// 无彩色（max == min）时色相无定义，返回 (0.0, 0.0)（调用方按饱和度门限滤掉）。
fn rgb_hue_sat(r: u8, g: u8, b: u8) -> (f32, f32) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let sat = if max <= 0.0 { 0.0 } else { delta / max };
    if delta <= 0.0 {
        return (0.0, 0.0);
    }
    let hue = if (max - r).abs() < f32::EPSILON {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    (hue.rem_euclid(360.0), sat)
}

/// 色相窗函数（D1-a）：到中心色相的环形距离 `d`；`width` 为窗全宽（度），
/// `smoothness` 为窗宽中用作软边的比例（0..1）。
/// 近灰像素（饱和度 < 0.1）恒为 0——无彩色没有稳定色相，不参与色相选。
fn hue_window(hue: f32, sat: f32, center: f32, width: f32, smoothness: f32) -> f32 {
    if sat < 0.1 {
        return 0.0;
    }
    let mut d = (hue - center).abs();
    if d > 180.0 {
        d = 360.0 - d;
    }
    let half = width / 2.0;
    let inner = half * (1.0 - smoothness.clamp(0.0, 1.0));
    if d <= inner {
        1.0
    } else if d >= half {
        0.0
    } else {
        // 此处必有 half > inner（inner < d < half ⇒ smoothness > 0 且 half > 0），除法安全。
        1.0 - smoothstep01((d - inner) / (half - inner))
    }
}

/// 按 `range` 模式把一幅 RGBA8 烘焙成 L8 覆盖网格（D1-a）。
/// `invert` 在此烤进网格（`CompiledMask` 的 invert 只作用于几何阶段，不翻转 patch）。
/// alpha == 0 的像素恒为 0（无图像即无遮罩；反选也不复活它）。
fn bake_range_grid(
    rgba: &[u8],
    width: usize,
    height: usize,
    range: &RangeBake,
    invert: bool,
) -> Vec<u8> {
    let mut grid = vec![0u8; width * height];
    for (index, pixel) in rgba.chunks_exact(4).enumerate() {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 {
            continue;
        }
        let coverage = match range {
            RangeBake::Luminance {
                low,
                high,
                smoothness,
            } => luminance_window(srgb_luma(r, g, b), *low, *high, *smoothness),
            RangeBake::Color {
                hue,
                width,
                smoothness,
            } => {
                let (h, s) = rgb_hue_sat(r, g, b);
                hue_window(h, s, *hue, *width, *smoothness)
            }
        };
        let value = if invert { 1.0 - coverage } else { coverage };
        grid[index] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    grid
}

/// `create_range_mask`（D1-a）：按像素值烘焙范围遮罩（亮度带 / 颜色带）。
///
/// 采样源是 `layer_id` 的全画布渲染（sRGB 编码 RGBA8，与 `frequency_separation`
/// 同一入口）；遮罩是**烘焙快照**：源图层此后的修改不传播（与 raster patch 同类）。
/// 烘焙结果以 L8 PNG + `ReplaceAll` patch 落库（`CompiledMask` 的 patch 重放口径），
/// 挂接沿用既有范式：`set_property` 把 `mask_id` 挂到调整层（见 `dodge_burn_setup`）。
/// 回执带烘焙后的有效参数与覆盖统计（`coverage_matched_pixels` / `coverage_mean`），
/// 供调用方核对「选到了多少」——为 0 时即「该带内没有像素」，不是静默成功。
pub(super) fn write_create_range_mask(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let mask_id = require_str(args, "mask_id")?;
    let layer_id = require_str(args, "layer_id")?;
    let range_name = optional_str(args, "range").unwrap_or_else(|| "luminance".to_owned());
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    let number_in = |key: &str, lo: f64, hi: f64, default: Option<f64>| -> Result<f64> {
        match args.get(key).and_then(Value::as_f64) {
            Some(value) if value.is_finite() && (lo..=hi).contains(&value) => Ok(value),
            Some(value) => Err(invalid(format!(
                "{key} 必须在 [{lo}，{hi}] 内，得到 {value}"
            ))),
            None => match default {
                Some(value) => Ok(value),
                None => Err(invalid(format!("{key} 是必填参数"))),
            },
        }
    };
    let range = match range_name.as_str() {
        "luminance" => {
            let low = number_in("low", 0.0, 1.0, Some(0.0))? as f32;
            let high = number_in("high", 0.0, 1.0, Some(1.0))? as f32;
            if low > high {
                return Err(invalid(format!(
                    "low 必须 ≤ high，得到 low={low}、high={high}"
                )));
            }
            let smoothness = number_in("smoothness", 0.0, 0.5, Some(0.05))? as f32;
            RangeBake::Luminance {
                low,
                high,
                smoothness,
            }
        }
        "color" => {
            let hue = number_in("hue", 0.0, 360.0, None)? as f32;
            let width = number_in("hue_width", 0.0, 360.0, Some(60.0))? as f32;
            if width <= 0.0 {
                return Err(invalid(format!(
                    "hue_width 必须大于 0（窗全宽），得到 {width}"
                )));
            }
            let smoothness = number_in("smoothness", 0.0, 1.0, Some(0.25))? as f32;
            RangeBake::Color {
                hue,
                width,
                smoothness,
            }
        }
        other => {
            return Err(invalid(format!(
                "range 只收 luminance / color，得到 {other}"
            )));
        }
    };
    let invert = args.get("invert").and_then(Value::as_bool).unwrap_or(false);
    let state = document_state(ctx)?;
    require_alive_layer(&state, &layer_id)?;
    let (canvas_w, canvas_h) = (state.width, state.height);
    // 对象级空层检查（渲染级检查不够：空层单层渲染会落到文档背景
    // ——缺省不透明白底——于是"空层"会伪装成一张纯色图通过自检；与
    // frequency_separation 同口径，对象级才是真空）。
    let alive = state.alive_objects();
    let layer_objects: Vec<_> = alive
        .iter()
        .filter(|object| object.layer_id == layer_id)
        .collect();
    if layer_objects.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("图层 {layer_id} 没有任何对象 ⇒ 没有可采样的像素")),
        ));
    }
    if layer_objects.iter().all(|object| {
        matches!(
            object.object_type,
            ObjectType::Adjustment | ObjectType::Filter
        )
    }) {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "图层 {layer_id} 只有调整/滤镜对象，没有可采样的像素"
            )),
        ));
    }
    // 内存闸（C2 口径，沿 frequency_separation）：源图 4 B/px + 网格 1 B/px。
    let pixels = (canvas_w as u64).saturating_mul(canvas_h as u64);
    let estimated = pixels.saturating_mul(5);
    let budget = Workspace::memory_budget_bytes();
    if estimated > budget {
        return Err(YanshiError::new(
            ErrorCode::ResourceExhausted,
            ErrorContext::detail(format!(
                "范围遮罩估算内存 {} MB 超过文档内存预算 {} MB（{}×{} 像素 × 5 B/px）⇒ \
                 拒绝烘焙（调大 YANSHI_DOC_MEMORY_BUDGET_MB 或先缩小画布）",
                estimated / 1_024 / 1_024,
                budget / 1_024 / 1_024,
                canvas_w,
                canvas_h,
            )),
        ));
    }
    // 渲染源图层（只拍这一层）。对象级空层检查在前；这里的
    // ReferenceNotFound / 全透明是兜底（渲染侧真空时仍拒绝，不烘焙
    // 一张背景色假图），与 frequency_separation 同口径。
    let bbox = Bbox::new(0.0, 0.0, canvas_w as f64, canvas_h as f64);
    let (width, height, source) =
        match ctx
            .workspace
            .render_region_raw_layer(&ctx.doc_id, bbox, &layer_id)
        {
            Ok(rendered) => rendered,
            Err(error) if error.code == ErrorCode::ReferenceNotFound => {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!(
                        "图层 {layer_id} 渲染为空 ⇒ 它是空层或纯调整层，没有可采样的像素：{error}"
                    )),
                ));
            }
            Err(error) => return Err(error),
        };
    if source.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("图层 {layer_id} 全幅透明 ⇒ 没有可采样的像素")),
        ));
    }
    let grid = bake_range_grid(&source, width as usize, height as usize, &range, invert);
    let matched = grid.iter().filter(|&&v| v > 0).count();
    let mean = grid.iter().map(|&v| f64::from(v)).sum::<f64>() / (grid.len() as f64 * 255.0);
    // blob 先落（原子引用它；放 blob 失败 ⇒ 零原子）。
    let png =
        yanshi_render::png::encode_png_l8(width, height, &grid).expect("内存 PNG 编码不会失败");
    let blob_hash = ctx.workspace.put_blob(&png)?;
    let payload = json!({
        "mask_id": mask_id,
        "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": canvas_w, "h": canvas_h}},
        "feather": 0.0,
        "mode": "new",
        "invert": false,
        "patches": [{
            "bbox": [0, 0, width as i32, height as i32],
            "target_u8": 255,
            "op": "replace_all",
            "blob": blob_hash,
        }],
        "refs": { "blobs": [blob_hash] },
    });
    let result = ctx.commit(AtomKind::CreateMask, payload)?;
    let region = region_of(&result);
    let mut response = finish_mutation(ctx, &result, region)?;
    response["range"] = json!(range_name);
    response["source_layer_id"] = json!(layer_id);
    response["coverage_matched_pixels"] = json!(matched);
    response["coverage_matched_ratio"] = json!(matched as f64 / grid.len() as f64);
    response["coverage_mean"] = json!(mean);
    match &range {
        RangeBake::Luminance {
            low,
            high,
            smoothness,
        } => {
            response["baked"] = json!({
                "range": "luminance",
                "low": low,
                "high": high,
                "smoothness": smoothness,
                "invert": invert,
            });
        }
        RangeBake::Color {
            hue,
            width,
            smoothness,
        } => {
            response["baked"] = json!({
                "range": "color",
                "hue": hue,
                "hue_width": width,
                "smoothness": smoothness,
                "invert": invert,
            });
        }
    }
    response["note"] = json!(
        "烘焙快照：遮罩取自源图层当前渲染的像素，源图层此后的修改不传播；明度为 sRGB 编码值的 Rec709 加权（显示空间）；invert 已烤进网格。挂到调整层：set_property{layer_id, key: \"mask_id\", value: <mask_id>}。"
    );
    Ok(response)
}

#[cfg(test)]
mod range_mask_tests {
    use super::{bake_range_grid, hue_window, luminance_window, rgb_hue_sat, srgb_luma, RangeBake};

    #[test]
    fn luminance_window_band_plateau_and_smooth_edges() {
        // 带内平顶、带外 0、边上 smoothstep 中点 0.5。
        assert_eq!(luminance_window(0.3, 0.2, 0.4, 0.05), 1.0);
        assert_eq!(luminance_window(0.1, 0.2, 0.4, 0.05), 0.0);
        assert_eq!(luminance_window(0.5, 0.2, 0.4, 0.05), 0.0);
        assert!((luminance_window(0.2, 0.2, 0.4, 0.05) - 0.5).abs() < 1e-6);
        assert!((luminance_window(0.4, 0.2, 0.4, 0.05) - 0.5).abs() < 1e-6);
        // smoothness=0 时退化为硬窗。
        assert_eq!(luminance_window(0.2, 0.2, 0.4, 0.0), 1.0);
        assert_eq!(luminance_window(0.199, 0.2, 0.4, 0.0), 0.0);
        assert_eq!(luminance_window(0.4, 0.2, 0.4, 0.0), 1.0);
        assert_eq!(luminance_window(0.401, 0.2, 0.4, 0.0), 0.0);
        // 窄带：smoothness 钳到 (high-low)/2，带内仍有平顶（无钳制时峰值到不了 1）。
        assert_eq!(luminance_window(0.32, 0.3, 0.34, 0.05), 1.0);
        assert!((luminance_window(0.3, 0.3, 0.34, 0.05) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn srgb_luma_rec709_weights() {
        assert!((srgb_luma(255, 255, 255) - 1.0).abs() < 1e-6);
        assert_eq!(srgb_luma(0, 0, 0), 0.0);
        assert!((srgb_luma(255, 0, 0) - 0.2126).abs() < 1e-6);
        assert!((srgb_luma(0, 255, 0) - 0.7152).abs() < 1e-6);
        assert!((srgb_luma(0, 0, 255) - 0.0722).abs() < 1e-6);
    }

    #[test]
    fn bake_luminance_grid_selects_dark_band_only() {
        // 4×1：黑 / 暗灰（≈0.30） / 白 / 透明红。
        let rgba = vec![
            0, 0, 0, 255, // 明度 0
            77, 77, 77, 255, // 明度 ≈0.302
            255, 255, 255, 255, // 明度 1
            255, 0, 0, 0, // 透明 ⇒ 恒 0（红本身明度 0.21 在带内也不选）
        ];
        let grid = bake_range_grid(
            &rgba,
            4,
            1,
            &RangeBake::Luminance {
                low: 0.2,
                high: 0.4,
                smoothness: 0.0,
            },
            false,
        );
        assert_eq!(grid, vec![0, 255, 0, 0]);
        // 反选烤进网格，但透明像素不复活。
        let inv = bake_range_grid(
            &rgba,
            4,
            1,
            &RangeBake::Luminance {
                low: 0.2,
                high: 0.4,
                smoothness: 0.0,
            },
            true,
        );
        assert_eq!(inv, vec![255, 0, 255, 0]);
    }

    #[test]
    fn hue_window_selects_hue_band_and_rejects_gray() {
        // 红在窗内、绿/蓝在窗外、灰（无彩色）恒为 0。
        let (red_h, red_s) = rgb_hue_sat(255, 0, 0);
        assert!((red_h - 0.0).abs() < 1e-6);
        assert!((red_s - 1.0).abs() < 1e-6);
        assert_eq!(hue_window(red_h, red_s, 0.0, 60.0, 0.25), 1.0);
        let (green_h, green_s) = rgb_hue_sat(0, 255, 0);
        assert!((green_h - 120.0).abs() < 1e-6);
        assert_eq!(hue_window(green_h, green_s, 0.0, 60.0, 0.25), 0.0);
        let (gray_h, gray_s) = rgb_hue_sat(128, 128, 128);
        assert_eq!(gray_s, 0.0);
        assert_eq!(hue_window(gray_h, gray_s, 0.0, 60.0, 0.25), 0.0);
        // 环形距离：359° 的红离中心 0° 只有 1°。
        let (near_red_h, near_red_s) = rgb_hue_sat(255, 0, 5);
        assert!(
            near_red_h > 355.0,
            "近红的色相应在 355° 以上，实测 {near_red_h}"
        );
        assert_eq!(hue_window(near_red_h, near_red_s, 0.0, 60.0, 0.25), 1.0);
        // 过渡带内取值严格在 (0, 1) 之间（不硬跳）。
        let (yellow_h, yellow_s) = rgb_hue_sat(255, 255, 0);
        assert!((yellow_h - 60.0).abs() < 1e-6);
        let mid = hue_window(yellow_h, yellow_s, 0.0, 140.0, 0.25);
        assert!(mid > 0.0 && mid < 1.0, "过渡带内应为中间值，实测 {mid}");
    }
}
