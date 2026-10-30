//! **几何工具（路线三）**：画布级 resize / scale / rotate / flip / crop / extend。
//!
//! 折叠语义在 `yanshi_core::fold`（`apply_geometry`）：画布尺寸 + 逐对象变换
//! 复合 + 锚点/选区/蒙版跟随；渲染不动。本模块只做工具层的事：参数校验、
//! 仿射构造、受限对象标注、位图蒙版重采样、回执。
//!
//! 位图蒙版重采样的架构说明（折叠层是纯状态函数，没有 blob 存储）：
//! 缩放/旋转/翻转时，工具层在提交前用旧编译蒙版采样出新网格，编成一张
//! ReplaceAll patch 随原子提交；折叠层按载荷替换 patch，撤销本原子即恢复
//! 原 patch（原 patch 仍在日志历史里，原 blob 不删）。这与 `import_image`
//!「先落 blob、原子只引哈希」同一模式，不是折叠期写存储。

use super::*;
use yanshi_core::geometry::{
    apply_flat, flatten_transform, geometry_moves_content, is_identity_flat, translation_of_flat,
};

/// 新文档同款画布上限（`service.rs` 的 `new_document` 校验）：单边与总像素。
const MAX_CANVAS_EDGE: u32 = 16_384;
const MAX_CANVAS_PIXELS: u64 = 1 << 28;

fn invalid(detail: impl Into<String>) -> YanshiError {
    YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(detail))
}

fn finite(value: f64, name: &str) -> Result<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(invalid(format!("{name} 必须是有限数")))
    }
}

fn uint_param(args: &Value, key: &str) -> Result<u64> {
    args.get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| invalid(format!("缺少或非法的整数参数 {key}")))
}

fn number_param(args: &Value, key: &str) -> Result<f64> {
    let value = args
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid(format!("缺少或非法的数值参数 {key}")))?;
    finite(value, key)
}

/// 命名锚点换算（锚点 A 语义的调用方便利层，不污染引擎语义）。
///
/// `anchor: {x, y}` 与 `anchor_name` 二选一；都不给时调用方传缺省（画布中心）。
/// 预设名按当前画布尺寸算；其余名字查文档状态的命名锚点表。
fn resolve_anchor(args: &Value, state: &DocumentState, default: [f64; 2]) -> Result<[f64; 2]> {
    let has_anchor = args.get("anchor").is_some();
    let has_name = args.get("anchor_name").is_some();
    if has_anchor && has_name {
        return Err(invalid("anchor 与 anchor_name 只能给一个"));
    }
    if let Some(anchor) = args.get("anchor") {
        let x = anchor
            .get("x")
            .and_then(Value::as_f64)
            .ok_or_else(|| invalid("anchor 需要 {x, y}"))?;
        let y = anchor
            .get("y")
            .and_then(Value::as_f64)
            .ok_or_else(|| invalid("anchor 需要 {x, y}"))?;
        return Ok([finite(x, "anchor.x")?, finite(y, "anchor.y")?]);
    }
    if let Some(name) = args.get("anchor_name").and_then(Value::as_str) {
        let (w, h) = (state.width as f64, state.height as f64);
        let preset = match name {
            "center" => Some([w / 2.0, h / 2.0]),
            "top_left" => Some([0.0, 0.0]),
            "top_right" => Some([w, 0.0]),
            "bottom_left" => Some([0.0, h]),
            "bottom_right" => Some([w, h]),
            "top_center" => Some([w / 2.0, 0.0]),
            "bottom_center" => Some([w / 2.0, h]),
            "left_center" => Some([0.0, h / 2.0]),
            "right_center" => Some([w, h / 2.0]),
            _ => None,
        };
        if let Some(point) = preset {
            return Ok(point);
        }
        if let Some(anchor) = state.anchors.get(name) {
            return Ok([anchor.x, anchor.y]);
        }
        return Err(invalid(format!("找不到命名锚点 {name}")));
    }
    Ok(default)
}

/// 该操作不接受锚点时，显式拒绝（不静默忽略）。
fn reject_anchor(args: &Value, operation: &str) -> Result<()> {
    if args.get("anchor").is_some() || args.get("anchor_name").is_some() {
        return Err(invalid(format!(
            "{operation} 不接受 anchor（锚点只用于 scale / rotate / flip）"
        )));
    }
    Ok(())
}

fn check_canvas_size(width: u32, height: u32) -> Result<()> {
    if width < 1 || height < 1 {
        return Err(invalid("画布尺寸必须 ≥ 1"));
    }
    if width > MAX_CANVAS_EDGE || height > MAX_CANVAS_EDGE {
        return Err(invalid(format!(
            "画布单边上限 {MAX_CANVAS_EDGE}（请求 {width}×{height}）"
        )));
    }
    if width as u64 * height as u64 > MAX_CANVAS_PIXELS {
        return Err(invalid(format!(
            "画布总像素上限 {MAX_CANVAS_PIXELS}（请求 {width}×{height}）"
        )));
    }
    Ok(())
}

/// 受限对象标注，按操作类分（`transform_primitive` 的既有缺口，不静默）。
///
/// 旋转/缩放/翻转：文本只平移位置、液化变换不施加，两者标注；
/// 位图补丁不再受限——非平移仿射复合进图元 `transform`、由渲染期一次采样
/// （与 BakedBase 同口径），包围盒按变换后四角算。裁剪与扩展都是纯平移：
/// 文本与导入图精确平移，不标注；液化必须标注，
/// 因为 `transform_primitive` 对液化是 NO-OP（位移场不跟随平移，液化 bbox
/// 的点会动、像素不动），平移类操作遇液化不再是“精确”。扩展的仿射是纯平移
/// （left, top），会复合进对象变换、内容相对画布原点平移，与裁剪情形相同。
/// 改界是单位仿射、内容不动，不标注。
fn limited_objects(state: &DocumentState, kind: yanshi_core::atom::AtomKind) -> Vec<Value> {
    if kind == yanshi_core::atom::AtomKind::ResizeCanvasBounds {
        return Vec::new();
    }
    let is_translation = matches!(
        kind,
        yanshi_core::atom::AtomKind::CropCanvas | yanshi_core::atom::AtomKind::ExtendCanvas
    );
    let mut out = Vec::new();
    for object in state.objects.values() {
        if object.is_deleted() {
            continue;
        }
        let reason = match object.object_type {
            yanshi_core::state::ObjectType::Text if !is_translation => {
                Some("文本在旋转/缩放/翻转下只平移位置，需要重排与重新栅格化（后续项）")
            }
            yanshi_core::state::ObjectType::Liquify if is_translation => {
                Some("液化在平移下位移场不跟随（transform_primitive 对液化是 NO-OP：bbox 点会动、像素不动，后续项）")
            }
            yanshi_core::state::ObjectType::Liquify => {
                Some("液化在旋转/缩放/翻转下变换不施加（后续项）")
            }
            _ => None,
        };
        if let Some(reason) = reason {
            out.push(json!({
                "object_id": object.id,
                "object_type": object_type_name(object.object_type),
                "reason": reason,
            }));
        }
    }
    out
}

/// 快照实例（`sync_policy: none`）在内容移动的几何操作下必须先拒绝：
///
/// 它的内容已冻结在快照里、不跟随 master，而折叠层对实例不复合变换
/// （内容经由 master 移动）⇒ 快照实例会被落在原地。显式拒绝，不静默错位。
fn reject_snapshot_instances(state: &DocumentState) -> Result<()> {
    let stuck: Vec<&str> = state
        .objects
        .values()
        .filter(|object| {
            !object.is_deleted()
                && object.object_type == yanshi_core::state::ObjectType::Instance
                && object.data.get("sync_policy").and_then(Value::as_str) == Some("none")
        })
        .map(|object| object.id.as_str())
        .collect();
    if stuck.is_empty() {
        Ok(())
    } else {
        Err(invalid(format!(
            "快照实例 {} 的内容已冻结、不跟随几何移动：先把 sync_policy 改回 all 再做几何",
            stuck.join(", ")
        )))
    }
}

fn affine_json(matrix: [f64; 6], pivot: [f64; 2]) -> Value {
    json!({ "matrix": matrix, "pivot": pivot })
}

/// 2×3 仿射求逆（行列式为零时报错；几何矩阵的行列式恒非零）。
fn invert_flat(flat: &[f64; 6]) -> Result<[f64; 6]> {
    let det = flat[0] * flat[3] - flat[1] * flat[2];
    if det == 0.0 || !det.is_finite() {
        return Err(invalid("几何仿射矩阵不可逆"));
    }
    Ok([
        flat[3] / det,
        -flat[1] / det,
        -flat[2] / det,
        flat[0] / det,
        (flat[2] * flat[5] - flat[3] * flat[4]) / det,
        (flat[1] * flat[4] - flat[0] * flat[5]) / det,
    ])
}

/// 对编译蒙版网格做纯函数重采样（最近邻，像素中心映射）。
///
/// 返回 `(bbox, grid)`：紧致裁剪到非默认值区域；全为默认值时返回
/// 原点 1×1 的默认值 patch（语义与全画布默认值等价，见调用方说明）。
/// 90° 旋转/翻转下最近邻是精确置换，无插值损失。
pub(super) fn resample_compiled(
    compiled: &yanshi_render::selection::CompiledMask,
    default_v: u8,
    inv_flat: &[f64; 6],
    new_w: i32,
    new_h: i32,
) -> ([i32; 4], Vec<u8>) {
    if new_w <= 0 || new_h <= 0 {
        return ([0, 0, 1, 1], vec![default_v]);
    }
    let mut grid = vec![default_v; (new_w as usize) * (new_h as usize)];
    for ny in 0..new_h {
        for nx in 0..new_w {
            let (ox_c, oy_c) = apply_flat(inv_flat, nx as f64 + 0.5, ny as f64 + 0.5);
            let ox = ox_c.floor() as i32;
            let oy = oy_c.floor() as i32;
            let v = (compiled.sample(ox, oy) * 255.0).round().clamp(0.0, 255.0) as u8;
            grid[(ny as usize) * (new_w as usize) + (nx as usize)] = v;
        }
    }
    let mut x0 = new_w;
    let mut y0 = new_h;
    let mut x1 = 0i32;
    let mut y1 = 0i32;
    for y in 0..new_h {
        for x in 0..new_w {
            if grid[(y as usize) * (new_w as usize) + (x as usize)] != default_v {
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
    if x1 <= x0 || y1 <= y0 {
        return ([0, 0, 1, 1], vec![default_v]);
    }
    let (bw, bh) = (x1 - x0, y1 - y0);
    let mut cropped = vec![0u8; (bw as usize) * (bh as usize)];
    for y in 0..bh {
        for x in 0..bw {
            cropped[(y as usize) * (bw as usize) + (x as usize)] =
                grid[((y0 + y) as usize) * (new_w as usize) + ((x0 + x) as usize)];
        }
    }
    ([x0, y0, bw, bh], cropped)
}

/// 对全部带 patch 的选区/蒙版做重采样，返回载荷用的 `mask_patches` 与 blob 引用。
///
/// 只在缩放/旋转/翻转下调用（平移由折叠层移 bbox，改界不动 patch）。
fn resample_patched_masks(
    ctx: &ToolContext<'_>,
    state: &DocumentState,
    flat: &[f64; 6],
    new_width: u32,
    new_height: u32,
) -> Result<(serde_json::Map<String, Value>, Vec<String>)> {
    use yanshi_render::selection::CompiledMask;
    let inv_flat = invert_flat(flat)?;
    let mut patches_map = serde_json::Map::new();
    let mut refs: Vec<String> = Vec::new();
    let load_blob = |s: &str| {
        s.parse::<yanshi_core::BlobHash>()
            .ok()
            .and_then(|hash| ctx.workspace.store().get(&hash).ok())
    };
    for selection in state.selections.values().chain(state.masks.values()) {
        if selection.is_deleted() || selection.patches.is_empty() {
            continue;
        }
        let compiled = CompiledMask::build(
            selection,
            state.width as i32,
            state.height as i32,
            &load_blob,
        );
        let default_v: u8 = if selection.invert { 255 } else { 0 };
        let (bbox, grid) = resample_compiled(
            &compiled,
            default_v,
            &inv_flat,
            new_width as i32,
            new_height as i32,
        );
        let png = yanshi_render::png::encode_png_l8(bbox[2] as u32, bbox[3] as u32, &grid)
            .expect("内存 PNG 编码不会失败");
        let blob_hash = ctx.workspace.store().put(&png)?;
        let blob_str = blob_hash.to_string();
        refs.push(blob_str.clone());
        // ReplaceAll 回放忽略 target_u8（见 CompiledMask::build），此处填 255 仅占位。
        patches_map.insert(
            selection.id.clone(),
            json!([{
                "bbox": bbox,
                "target_u8": 255,
                "op": "replace_all",
                "blob": blob_str,
            }]),
        );
    }
    Ok((patches_map, refs))
}

/// 六个几何工具的公共提交路径。
#[allow(clippy::too_many_arguments)]
fn commit_geometry(
    ctx: &mut ToolContext<'_>,
    kind: yanshi_core::atom::AtomKind,
    operation: &str,
    affine: Value,
    new_width: u32,
    new_height: u32,
    anchor: Option<[f64; 2]>,
    extra: serde_json::Map<String, Value>,
    moving: bool,
) -> Result<Value> {
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    if moving {
        reject_snapshot_instances(&state)?;
    }
    let limited = if moving && geometry_moves_content(kind) {
        limited_objects(&state, kind)
    } else {
        Vec::new()
    };
    // 变换后旧画布角点是否超出新画布：超出时记一条可观测的裁剪提示（不静默）。
    //
    // 锚点 A 的直接后果：缩放绕非原点锚点、旋转绕任何锚点时，变换后的旧画布
    // 矩形都不等于新画布矩形，超出部分渲染时裁剪。这是锁定的字面语义，
    // 不是 bug；是否要加居中平移（选项 B 行为）由父 agent 定夺，不在此擅自改。
    let mut clip_note: Option<String> = None;
    if matches!(operation, "scale_canvas" | "rotate_canvas" | "flip_canvas") {
        let flat = flatten_transform(&yanshi_core::geometry::transform_from_affine_payload(
            &json!({ "affine": affine }),
        ));
        let corners = [
            apply_flat(&flat, 0.0, 0.0),
            apply_flat(&flat, old_width as f64, 0.0),
            apply_flat(&flat, 0.0, old_height as f64),
            apply_flat(&flat, old_width as f64, old_height as f64),
        ];
        let outside = corners
            .iter()
            .any(|(x, y)| *x < 0.0 || *y < 0.0 || *x > new_width as f64 || *y > new_height as f64);
        if outside {
            clip_note = Some(format!(
                "{operation} 后部分内容超出新画布范围，渲染时裁剪（锚点 A 下绕枢轴变换的已知行为）"
            ));
        }
    }
    let mut payload = json!({
        "affine": affine,
        "new_width": new_width,
        "new_height": new_height,
    });
    if let Some(anchor) = anchor {
        payload["anchor"] = json!([anchor[0], anchor[1]]);
    }
    for (key, value) in extra {
        payload[key] = value;
    }
    // 位图蒙版：非平移的移动操作必须随原子提交重采样 patch。
    let affine_value = payload["affine"].clone();
    let transform =
        yanshi_core::geometry::transform_from_affine_payload(&json!({ "affine": affine_value }));
    let flat = flatten_transform(&transform);
    let needs_resample = moving && !is_identity_flat(&flat) && translation_of_flat(&flat).is_none();
    if needs_resample {
        let (patches_map, refs) =
            resample_patched_masks(ctx, &state, &flat, new_width, new_height)?;
        payload["mask_patches"] = Value::Object(patches_map);
        if !refs.is_empty() {
            payload["refs"] = json!({ "blobs": refs });
        }
    }
    let result = ctx.commit(kind, payload)?;
    let region = region_of(&result);
    let mut value = finish_mutation(ctx, &result, region)?;
    value["operation"] = json!(operation);
    value["previous_width"] = json!(old_width);
    value["previous_height"] = json!(old_height);
    value["width"] = json!(new_width);
    value["height"] = json!(new_height);
    value["effective_scale_x"] = json!(new_width as f64 / old_width.max(1) as f64);
    value["effective_scale_y"] = json!(new_height as f64 / old_height.max(1) as f64);
    if let Some(anchor) = anchor {
        value["anchor"] = json!([anchor[0], anchor[1]]);
    }
    value["limited_objects"] = Value::Array(limited);
    if let Some(note) = clip_note {
        value["clip_note"] = json!(note);
    }
    Ok(value)
}

pub(super) fn write_resize_canvas_bounds(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    reject_anchor(args, "resize_canvas_bounds")?;
    let width = uint_param(args, "width")? as u32;
    let height = uint_param(args, "height")? as u32;
    check_canvas_size(width, height)?;
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::ResizeCanvasBounds,
        "resize_canvas_bounds",
        affine_json([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [0.0, 0.0]),
        width,
        height,
        None,
        serde_json::Map::new(),
        false,
    )
}

pub(super) fn write_scale_canvas(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    let default_anchor = [old_width as f64 / 2.0, old_height as f64 / 2.0];
    let anchor = resolve_anchor(args, &state, default_anchor)?;
    let non_uniform = args
        .get("non_uniform")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let (sx, sy) = if let Some(scale) = args.get("scale") {
        if args.get("scale_x").is_some() || args.get("scale_y").is_some() {
            return Err(invalid("scale 与 scale_x/scale_y 只能给一组"));
        }
        let scale = finite(
            scale.as_f64().ok_or_else(|| invalid("scale 必须是数值"))?,
            "scale",
        )?;
        if scale <= 0.0 {
            return Err(invalid("scale 必须 > 0"));
        }
        (scale, scale)
    } else {
        let sx = number_param(args, "scale_x")?;
        let sy = number_param(args, "scale_y")?;
        if sx <= 0.0 || sy <= 0.0 {
            return Err(invalid("scale_x / scale_y 必须 > 0"));
        }
        if !non_uniform && sx != sy {
            return Err(invalid(
                "非均匀缩放默认关闭：sx != sy 时必须显式传 non_uniform: true",
            ));
        }
        (sx, sy)
    };
    // 宽高用 f64::round（半远离零），下限 1。
    let new_width = ((old_width as f64 * sx).round() as i64).max(1) as u32;
    let new_height = ((old_height as f64 * sy).round() as i64).max(1) as u32;
    check_canvas_size(new_width, new_height)?;
    let mut extra = serde_json::Map::new();
    extra.insert("scale_x".to_owned(), json!(sx));
    extra.insert("scale_y".to_owned(), json!(sy));
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::ScaleCanvas,
        "scale_canvas",
        affine_json([sx, 0.0, 0.0, sy, 0.0, 0.0], anchor),
        new_width,
        new_height,
        Some(anchor),
        extra,
        true,
    )
}

pub(super) fn write_rotate_canvas(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    let default_anchor = [old_width as f64 / 2.0, old_height as f64 / 2.0];
    let anchor = resolve_anchor(args, &state, default_anchor)?;
    let degrees = number_param(args, "degrees")?;
    if (degrees / 90.0).fract() != 0.0 {
        return Err(invalid("旋转只接受 90° 倍数（任意角度显式拒绝）"));
    }
    let normalized = ((degrees % 360.0) + 360.0) % 360.0;
    let (matrix, swapped) = match normalized as i64 {
        0 => ([1.0, 0.0, 0.0, 1.0, 0.0, 0.0], false),
        90 => ([0.0, 1.0, -1.0, 0.0, 0.0, 0.0], true),
        180 => ([-1.0, 0.0, 0.0, -1.0, 0.0, 0.0], false),
        270 => ([0.0, -1.0, 1.0, 0.0, 0.0, 0.0], true),
        _ => return Err(invalid("旋转只接受 90° 倍数（任意角度显式拒绝）")),
    };
    let (new_width, new_height) = if swapped {
        (old_height, old_width)
    } else {
        (old_width, old_height)
    };
    check_canvas_size(new_width, new_height)?;
    let mut extra = serde_json::Map::new();
    extra.insert("degrees".to_owned(), json!(degrees));
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::RotateCanvas,
        "rotate_canvas",
        affine_json(matrix, anchor),
        new_width,
        new_height,
        Some(anchor),
        extra,
        true,
    )
}

pub(super) fn write_flip_canvas(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    let default_anchor = [old_width as f64 / 2.0, old_height as f64 / 2.0];
    let anchor = resolve_anchor(args, &state, default_anchor)?;
    let direction = require_str(args, "direction")?;
    let matrix = match direction.as_str() {
        "horizontal" => [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        "vertical" => [1.0, 0.0, 0.0, -1.0, 0.0, 0.0],
        _ => {
            return Err(invalid(
                "direction 只能是 horizontal（左右翻）或 vertical（上下翻）",
            ));
        }
    };
    let mut extra = serde_json::Map::new();
    extra.insert("direction".to_owned(), json!(direction));
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::FlipCanvas,
        "flip_canvas",
        affine_json(matrix, anchor),
        old_width,
        old_height,
        Some(anchor),
        extra,
        true,
    )
}

pub(super) fn write_crop_canvas(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    reject_anchor(args, "crop_canvas")?;
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    let region_value = args
        .get("region")
        .ok_or_else(|| invalid("crop 需要 region: {x, y, w, h}"))?;
    // 数组形 [x, y, w, h] 在现有逻辑前归一化为 object 形（与 render_region /
    // analyze_region 的 parse_bbox 口径一致）；后续整数像素校验仍走同一条路。
    let normalized;
    let region = if let Some(array) = region_value.as_array() {
        if array.len() != 4 {
            return Err(invalid("region 数组形必须是 [x, y, w, h] 4 个数值"));
        }
        let numbers: Option<Vec<f64>> = array.iter().map(Value::as_f64).collect();
        let numbers = numbers.ok_or_else(|| invalid("region 数组元素必须是数值"))?;
        normalized = json!({
            "x": numbers[0],
            "y": numbers[1],
            "w": numbers[2],
            "h": numbers[3],
        });
        normalized
            .as_object()
            .ok_or_else(|| invalid("crop 需要 region: {x, y, w, h}"))?
    } else {
        region_value
            .as_object()
            .ok_or_else(|| invalid("crop 需要 region: {x, y, w, h}"))?
    };
    let get_int = |key: &str| -> Result<i64> {
        let value = region
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| invalid(format!("region.{key} 必须是数值")))?;
        if value.fract() != 0.0 {
            return Err(invalid(format!(
                "region.{key} 必须是整数像素（位图 patch 平移要求）"
            )));
        }
        Ok(value as i64)
    };
    let (x, y, w, h) = (get_int("x")?, get_int("y")?, get_int("w")?, get_int("h")?);
    if w < 1 || h < 1 || x < 0 || y < 0 || x + w > old_width as i64 || y + h > old_height as i64 {
        return Err(invalid(format!(
            "crop 区域必须完全位于画布内（画布 {old_width}×{old_height}，区域 x={x} y={y} w={w} h={h}）"
        )));
    }
    let mut extra = serde_json::Map::new();
    extra.insert(
        "region".to_owned(),
        json!({ "x": x, "y": y, "w": w, "h": h }),
    );
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::CropCanvas,
        "crop_canvas",
        affine_json([1.0, 0.0, 0.0, 1.0, -(x as f64), -(y as f64)], [0.0, 0.0]),
        w as u32,
        h as u32,
        None,
        extra,
        true,
    )
}

pub(super) fn write_extend_canvas(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    reject_anchor(args, "extend_canvas")?;
    let state = document_state(ctx)?;
    let (old_width, old_height) = (state.width, state.height);
    let margins = args
        .get("margins")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("extend 需要 margins: {left, top, right, bottom}"))?;
    let get_margin = |key: &str| -> Result<i64> {
        let value = margins
            .get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| invalid(format!("margins.{key} 必须是数值")))?;
        if value.fract() != 0.0 || value < 0.0 {
            return Err(invalid(format!(
                "margins.{key} 必须是非负整数像素（位图 patch 平移要求）"
            )));
        }
        Ok(value as i64)
    };
    let (left, top, right, bottom) = (
        get_margin("left")?,
        get_margin("top")?,
        get_margin("right")?,
        get_margin("bottom")?,
    );
    if left + top + right + bottom <= 0 {
        return Err(invalid("extend 至少一边边距 > 0"));
    }
    let new_width = (old_width as i64 + left + right) as u32;
    let new_height = (old_height as i64 + top + bottom) as u32;
    check_canvas_size(new_width, new_height)?;
    let mut extra = serde_json::Map::new();
    extra.insert(
        "margins".to_owned(),
        json!({ "left": left, "top": top, "right": right, "bottom": bottom }),
    );
    commit_geometry(
        ctx,
        yanshi_core::atom::AtomKind::ExtendCanvas,
        "extend_canvas",
        affine_json([1.0, 0.0, 0.0, 1.0, left as f64, top as f64], [0.0, 0.0]),
        new_width,
        new_height,
        None,
        extra,
        true,
    )
}
