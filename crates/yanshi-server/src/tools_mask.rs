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
                "shape 必须带 kind 或 type（rect/ellipse/polygon/path），from_selection 时可省略"
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
