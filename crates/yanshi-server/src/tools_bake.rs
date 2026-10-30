//! bake_layer / recover_baked_blobs（专家评审定版 + 第二轮 12 条）。
//!
//! 口径要点（与 `docs/` 里的 bake 说明同源，工具回执以这里为准）：
//! * 单层 scope；基底是画布 1:1 直通 RGBA8 PNG，锚点 (0,0)，不接受缩放参数。
//! * 压平渲染用克隆状态去外壳（opacity=1、blend=normal、clipping=false，
//!   默认 mask 剥离），否则外壳会在之后整幅合成里被乘第二次。
//! * 门禁（第二轮第 12 条）：`allow_rasterize_editable` / `allow_clip_off_canvas`
//!   默认均为 false；含文本、矢量或越界图元时默认直接返回工具错误
//!   （is_error），绝不只塞进 warnings。`reject_on_lossy` 已废弃，不再接受。
//! * 自愈（第二轮第 4、5 条）：常规折叠/渲染缺 Blob 严格报错，绝不自动重放；
//!   显式入口只有 `recover_baked_blobs` 与导入的 `missing_blob_policy`。
//! * 哈希不一致缺省 strict；显式 `on_hash_mismatch: "append_rebake"` 时才
//!   把真实哈希入库并在日志末尾追加一条新 BakeLayer 原子（带
//!   `replaces_bake_seq` 审计元数据），绝不改写历史原子。

use super::*;
use yanshi_core::bake::{is_bake_content_kind, Affine2D};
use yanshi_core::{Atom, AtomKind, BlobHash};

/// 单次 bake 的 OOM 守卫（第二轮第 11 条）：离屏/PNG 输入缓冲用
/// `try_reserve_exact`，失败返回可观测的资源错误且原状态不动。
/// 渲染器内部的 Buffer 与 PNG 编码输出分配仍走既有 infallible 路径
/// （与 export_png 同口径），此处如实记录为未决缺口，不谎称已覆盖。
fn try_reserve_bytes(len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len).map_err(|_| {
        YanshiError::new(
            ErrorCode::ResourceExhausted,
            ErrorContext::detail("BakeError::OutOfMemory：压平缓冲分配失败，原文档状态未改动"),
        )
    })?;
    Ok(buf)
}

/// 有效 BakeLayer（每层取 seq 最大的有效者；被 revert 的不算）。
/// 返回 `(layer_id, bake_atom_seq, up_to_seq, blob_hash, bake_atom_id)`。
/// 有效 Bake 的一行引用（每层取 seq 最大的有效者）：`(layer_id, bake_seq,
/// up_to_seq, blob_hash, bake_atom_id)`；不用裸 5 元组，避免 clippy type_complexity。
pub(crate) type BakedBlobCandidate = (String, u64, u64, String, String);

/// 全部未被抑制的 BakeLayer 原子（每层多条，不只取最新一条）。
///
/// recover_baked_blobs 必须用它而不能只用 effective_baked_blobs：
/// Undo 场景要恢复的恰恰是被新 Bake 取代的**历史**基底（Undo 当前 Bake
/// 后它会重新生效）；只取每层最新一条会漏掉它，恢复回执 recovered=0，
/// 随后 Undo 仍被拦截（判据 ⑦ 的红即由此来）。
pub(crate) fn all_valid_baked_blobs(atoms: &[Atom]) -> Vec<BakedBlobCandidate> {
    let suppressed = yanshi_core::fold::compute_suppressed(atoms);
    let mut out: Vec<BakedBlobCandidate> = Vec::new();
    for atom in atoms {
        if atom.kind != AtomKind::BakeLayer || suppressed.contains(&atom.id) {
            continue;
        }
        let Some(layer_id) = atom.payload.get("layer_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(blob_hash) = atom.payload.get("blob_hash").and_then(|v| v.as_str()) else {
            continue;
        };
        let up_to_seq = atom
            .payload
            .get("up_to_seq")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        out.push((
            layer_id.to_owned(),
            atom.seq,
            up_to_seq,
            blob_hash.to_owned(),
            atom.id.clone(),
        ));
    }
    out
}

pub(crate) fn effective_baked_blobs(atoms: &[Atom]) -> Vec<BakedBlobCandidate> {
    let suppressed = yanshi_core::fold::compute_suppressed(atoms);
    let mut latest: std::collections::BTreeMap<String, (u64, u64, String, String)> =
        std::collections::BTreeMap::new();
    for atom in atoms {
        if atom.kind != AtomKind::BakeLayer || suppressed.contains(&atom.id) {
            continue;
        }
        let Some(layer_id) = atom.payload.get("layer_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(blob_hash) = atom.payload.get("blob_hash").and_then(|v| v.as_str()) else {
            continue;
        };
        let up_to_seq = atom
            .payload
            .get("up_to_seq")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let entry =
            latest
                .entry(layer_id.to_owned())
                .or_insert((0, 0, String::new(), String::new()));
        if atom.seq >= entry.0 {
            *entry = (atom.seq, up_to_seq, blob_hash.to_owned(), atom.id.clone());
        }
    }
    latest
        .into_iter()
        .map(|(layer_id, (bake_seq, up_to_seq, blob_hash, atom_id))| {
            (layer_id, bake_seq, up_to_seq, blob_hash, atom_id)
        })
        .collect()
}

/// 重放自愈核心：用 `seq <= up_to_seq` 且排除全部 BakeLayer 原子的
/// 子集建临时文档（旁路剪枝屏障），渲染目标层透明基底并编码 PNG。
/// 返回 `(png_bytes, blob_hash)`；调用方负责 SHA 校验与入库决策。
fn replay_baked_blob(
    store: &std::sync::Arc<dyn yanshi_core::blob::BlobStore>,
    atoms: &[Atom],
    layer_id: &str,
    up_to_seq: u64,
    flatten_mask: bool,
    target_bake_seq: u64,
) -> Result<(Vec<u8>, BlobHash)> {
    // **重放子集必须含更早的 BakeLayer**（只排除目标 Bake 本身）：
    // 压平后追加的 Move/Transform 瞄准的是基底对象（`baked:{layer}`）；
    // 若把全部 Bake 排除，变换原子失去目标、折叠时只剩级联警告，
    // 重放渲染的是未变换的原始笔触，哈希恒不一致（判据 ⑪ 的红即由此来）。
    // 目标 Bake 自身的 seq 恒大于 up_to_seq（up_to_seq 是提交前 head），
    // 这里再显式按 seq 排除一次，防畸形原子（up_to_seq 指到自身之后）。
    // 更早 Bake 的 Blob 必须已在存储中（recover 按 seq 顺序先修更早者；
    // 导入自愈里缺更早 Blob 时渲染直接报 missing_baked_blob，不静默）。
    let subset: Vec<Atom> = atoms
        .iter()
        .filter(|a| {
            a.seq <= up_to_seq && !(a.kind == AtomKind::BakeLayer && a.seq == target_bake_seq)
        })
        .cloned()
        .collect();
    let mut temp = crate::document::Document::open(
        std::sync::Arc::clone(store),
        format!("__bake_recovery_{layer_id}"),
        subset,
        crate::document::DocumentSettings::default(),
    )?;
    let (w, h, rgba) = temp.render_layer_bake_base(layer_id, flatten_mask)?;
    let mut input = try_reserve_bytes(rgba.len())?;
    input.extend_from_slice(&rgba);
    let png = yanshi_render::png::encode_png(w, h, &input).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("重放自愈 PNG 编码失败"),
        )
    })?;
    let hash = BlobHash::from_bytes(&png);
    Ok((png, hash))
}

/// bake_layer 工具。
pub(super) fn write_bake_layer(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let allow_rasterize_editable = optional_bool(args, "allow_rasterize_editable").unwrap_or(false);
    let allow_clip_off_canvas = optional_bool(args, "allow_clip_off_canvas").unwrap_or(false);
    let flatten_mask = optional_bool(args, "flatten_mask").unwrap_or(false);
    if args.get("reject_on_lossy").is_some() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "reject_on_lossy 已废弃：请用 allow_rasterize_editable / allow_clip_off_canvas（默认均为 false）",
            ),
        ));
    }
    // ---- 快照：层内对象分项计数 + 门禁判定（只读，不渲染）----
    let (canvas_w, canvas_h, layer_name, layer_mask, objects) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let state = document.state();
        let layer = state.layers.get(&layer_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("图层 {layer_id} 不存在")),
            )
        })?;
        if layer.is_deleted() {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("图层 {layer_id} 已删除")),
            ));
        }
        let objects: Vec<yanshi_core::Object> = state
            .objects_in_layer(&layer_id)
            .into_iter()
            .cloned()
            .collect();
        (
            state.width,
            state.height,
            layer.name.clone(),
            layer.mask_id.clone(),
            objects,
        )
    };
    if objects.is_empty() {
        return Ok(json!({
            "status": "no_op_empty_layer",
            "scope": "layer",
            "layer_id": layer_id,
            "layer_name": layer_name,
            "blob_hash": Value::Null,
            "warnings": [{
                "level": yanshi_core::bake::warning_level::INFO,
                "code": "NO_OP_EMPTY_LAYER",
                "message": "目标层为空：未生成 Blob，未追加原子",
            }],
        }));
    }
    let mut strokes = 0usize;
    let mut texts = 0usize;
    let mut vectors = 0usize;
    let mut images = 0usize;
    let mut liquify = 0usize;
    let mut adjustments = 0usize;
    let mut filters = 0usize;
    let mut text_ids: Vec<String> = Vec::new();
    let mut vector_ids: Vec<String> = Vec::new();
    let mut clipped: Vec<Value> = Vec::new();
    let single_baked = objects.len() == 1
        && objects[0].data.get("baked_base").and_then(|v| v.as_bool()) == Some(true)
        && objects[0].transform.matrix == Affine2D::IDENTITY.0;
    for object in &objects {
        match object.object_type {
            yanshi_core::ObjectType::Stroke => strokes += 1,
            yanshi_core::ObjectType::Text => {
                texts += 1;
                text_ids.push(object.id.clone());
            }
            yanshi_core::ObjectType::Shape | yanshi_core::ObjectType::Path => {
                vectors += 1;
                vector_ids.push(object.id.clone());
            }
            yanshi_core::ObjectType::RasterPatch => {
                if object.data.get("baked_base").and_then(|v| v.as_bool()) == Some(true) {
                    // 已压平的基底本身不是“新图元”：再压平时它会被旧基底渲染取代。
                } else {
                    images += 1;
                }
            }
            yanshi_core::ObjectType::Liquify => liquify += 1,
            yanshi_core::ObjectType::Adjustment => adjustments += 1,
            yanshi_core::ObjectType::Filter => filters += 1,
            _ => {}
        }
        // 越界判定：用带变换的包围盒与画布求交之外的部分。
        if let Some(bbox) = yanshi_render::object::object_bbox(object) {
            let x0 = bbox.x;
            let y0 = bbox.y;
            let x1 = bbox.x + bbox.w;
            let y1 = bbox.y + bbox.h;
            if x0 < 0.0 || y0 < 0.0 || x1 > canvas_w as f64 || y1 > canvas_h as f64 {
                clipped.push(json!({
                    "object_id": object.id,
                    "bbox": [x0, y0, x1, y1],
                    "canvas": [0.0, 0.0, canvas_w as f64, canvas_h as f64],
                }));
            }
        }
    }
    if single_baked {
        // 仅有 1 个未改动 BakedBase 且其后无新内容原子：no-op（不查日志外的状态）。
        let source_up_to_seq = objects[0]
            .data
            .get("source_up_to_seq")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let has_new_content = ctx
            .workspace
            .document(&ctx.doc_id)
            .map(|document| {
                document.log().iter().any(|atom| {
                    atom.seq > source_up_to_seq
                        && is_bake_content_kind(atom.kind)
                        && yanshi_core::bake::bake_target_layer(atom, |id| {
                            document.state().objects.get(id).map(|o| o.layer_id.clone())
                        })
                        .as_deref()
                            == Some(layer_id.as_str())
                })
            })
            .unwrap_or(false);
        if !has_new_content {
            return Ok(json!({
                "status": "no_op_already_baked",
                "scope": "layer",
                "layer_id": layer_id,
                "layer_name": layer_name,
                "blob_hash": objects[0].data.get("blob_hash").cloned().unwrap_or(Value::Null),
                "warnings": [{
                    "level": yanshi_core::bake::warning_level::INFO,
                    "code": "NO_OP_ALREADY_BAKED",
                    "message": "目标层仅有 1 个未改动 BakedBase：未生成新 Blob，未追加原子",
                }],
            }));
        }
    }
    // ---- 门禁：默认拦截（工具错误，不是 warnings）----
    if (texts > 0 || vectors > 0) && !allow_rasterize_editable {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "图层 {layer_id} 含文本 {texts} 个、矢量 {vectors} 个：默认不栅格化可编辑图元 ⇒ 请显式传 allow_rasterize_editable=true 确认（回执 warnings 将记 TEXT_RASTERIZED / VECTOR_RASTERIZED 审计条目）"
            )),
        ));
    }
    if !clipped.is_empty() && !allow_clip_off_canvas {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "图层 {layer_id} 有 {} 个图元越界：默认不丢弃越界像素 ⇒ 请显式传 allow_clip_off_canvas=true 确认（回执 warnings 将记 CLIPPED_OFF_CANVAS_PIXELS 明细）",
                clipped.len()
            )),
        ));
    }
    // ---- 只读渲染（失败则日志零修改）----
    let up_to_seq = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|d| d.head_seq())
        .unwrap_or(0);
    let (bw, bh, rgba) = ctx
        .workspace
        .document_mut(&ctx.doc_id)?
        .render_layer_bake_base(&layer_id, flatten_mask)?;
    let mut png_input = try_reserve_bytes(rgba.len())?;
    png_input.extend_from_slice(&rgba);
    let png = yanshi_render::png::encode_png(bw, bh, &png_input).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("bake_layer PNG 编码失败：日志未改动，可直接重试"),
        )
    })?;
    // ---- 写 Blob 并校验（失败则日志零修改）----
    let blob_hash = ctx.workspace.store().put(&png)?;
    let stored = ctx.workspace.store().get(&blob_hash)?;
    if stored != png {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "bake_layer Blob 写回校验失败：存储内容与编码字节不一致，日志未改动",
            ),
        ));
    }
    if BlobHash::from_bytes(&png) != blob_hash {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("bake_layer Blob 哈希校验失败：日志未改动"),
        ));
    }
    // ---- 追加原子并更新内存折叠态 ----
    let (superseded_atom_count, superseded_min_seq) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let suppressed = yanshi_core::fold::compute_suppressed(document.log().atoms());
        let mut count = 0usize;
        let mut min_seq = u64::MAX;
        for atom in document.log().iter() {
            if atom.seq > up_to_seq || suppressed.contains(&atom.id) {
                continue;
            }
            if !is_bake_content_kind(atom.kind) {
                continue;
            }
            let target = yanshi_core::bake::bake_target_layer(atom, |id| {
                document.state().objects.get(id).map(|o| o.layer_id.clone())
            });
            if target.as_deref() == Some(layer_id.as_str()) {
                count += 1;
                min_seq = min_seq.min(atom.seq);
            }
        }
        (
            count,
            if min_seq == u64::MAX {
                up_to_seq
            } else {
                min_seq
            },
        )
    };
    let total_effective = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| {
            let suppressed = yanshi_core::fold::compute_suppressed(document.log().atoms());
            document.log().atoms().len() - suppressed.len()
        })
        .unwrap_or(0);
    let retained_atom_count = total_effective.saturating_sub(superseded_atom_count);
    let result = ctx.commit(
        AtomKind::BakeLayer,
        json!({
            "layer_id": layer_id,
            "up_to_seq": up_to_seq,
            "blob_hash": blob_hash.to_string(),
            "flatten_mask": flatten_mask,
        }),
    )?;
    let mut value = finish_mutation(ctx, &result, None)?;
    // ---- 回执（必填字段 + 警告分级）----
    let mut warnings: Vec<Value> = Vec::new();
    if liquify > 0 {
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::INFO,
            "code": "LIQUIFY_BAKED_INTO_PIXELS",
            "message": format!("层内液化 {liquify} 个已冻结进基底像素（原液化网格被新水位剪枝）"),
            "count": liquify,
        }));
    }
    if texts > 0 {
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
            "code": "TEXT_RASTERIZED",
            "message": format!("文本 {texts} 个已栅格化，不可再编辑文本内容"),
            "count": texts,
            "object_ids": text_ids,
        }));
    }
    if vectors > 0 {
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
            "code": "VECTOR_RASTERIZED",
            "message": format!("矢量 {vectors} 个已栅格化，不可再编辑矢量节点"),
            "count": vectors,
            "object_ids": vector_ids,
        }));
    }
    if flatten_mask && layer_mask.is_some() {
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
            "code": "MASK_FLATTENED",
            "message": "图层级独立掩膜已压进基底（flatten_mask=true），掩膜不再独立存在",
            "mask_id": layer_mask,
        }));
    }
    if !clipped.is_empty() {
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::LOSSY_PIXELS,
            "code": "CLIPPED_OFF_CANVAS_PIXELS",
            "message": format!("越界图元 {} 个的画布外像素已被裁剪（未静默丢弃：区域坐标见 clipped_regions）", clipped.len()),
            "count": clipped.len(),
            "clipped_regions": clipped,
        }));
    }
    value["status"] = json!("applied");
    value["scope"] = json!("layer");
    value["layer_id"] = json!(layer_id);
    value["layer_name"] = json!(layer_name);
    value["bake_atom_seq"] = json!(result.seq);
    value["bake_atom_id"] = json!(result.atom_id);
    value["superseded_seq_range"] = json!([superseded_min_seq, up_to_seq]);
    value["blob_hash"] = json!(blob_hash.to_string());
    value["blob_byte_len"] = json!(png.len());
    value["dimensions"] = json!({"width": bw, "height": bh});
    value["superseded_atom_count"] = json!(superseded_atom_count);
    value["retained_atom_count"] = json!(retained_atom_count);
    value["collapsed_primitives"] = json!({
        "strokes": strokes,
        "texts": texts,
        "vectors": vectors,
        "images": images,
        "liquify": liquify,
        "adjustments": adjustments,
        "filters": filters,
    });
    value["warnings"] = json!(warnings);
    Ok(value)
}

/// recover_baked_blobs 工具（第二轮第 4、5 条：唯一常规自愈入口）。
///
/// 参数：`layer_id?`（缺省全部层）、`up_to_seq?`（缺省按层取最新有效 Bake）、
/// `on_hash_mismatch?`（缺省 `"strict"`；显式 `"append_rebake"` 才追加新 Bake 原子）。
/// 常规折叠/渲染缺 Blob 时**不**走这里，仍严格报错；成本挂在本调用回执。
pub(super) fn write_recover_baked_blobs(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_filter = optional_str(args, "layer_id");
    let target_seq_filter = args.get("up_to_seq").and_then(|v| v.as_u64());
    let on_hash_mismatch =
        optional_str(args, "on_hash_mismatch").unwrap_or_else(|| "strict".to_owned());
    if !matches!(on_hash_mismatch.as_str(), "strict" | "append_rebake") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("on_hash_mismatch 只接受 strict（默认）或 append_rebake"),
        ));
    }
    let started = std::time::Instant::now();
    let store = ctx.workspace.store();
    // 先收齐待修复项（避免在遍历日志时持有文档借用）。
    let (atoms, candidates): (Vec<Atom>, Vec<BakedBlobCandidate>) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let atoms = document.log().atoms().to_vec();
        // 恢复面是全部未被抑制的 Bake（含历史基底），不是只取每层最新。
        let mut candidates = all_valid_baked_blobs(&atoms);
        if let Some(layer_id) = &layer_filter {
            candidates.retain(|(lid, _, _, _, _)| lid == layer_id);
        }
        if let Some(up_to_seq) = target_seq_filter {
            candidates.retain(|(_, _, useq, _, _)| *useq == up_to_seq);
        }
        (atoms, candidates)
    };
    let mut healed: Vec<Value> = Vec::new();
    let mut replayed_atom_count = 0usize;
    let mut warnings: Vec<Value> = Vec::new();
    for (layer_id, bake_seq, up_to_seq, expected_hash, _bake_atom_id) in &candidates {
        let hash: BlobHash = expected_hash.parse().map_err(|_| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("Bake 原子里的 blob_hash 非法：{expected_hash}")),
            )
        })?;
        if store.exists(&hash) {
            continue;
        }
        // 该 Bake 原子记录的 flatten_mask（缺省 false），重放须同口径。
        let flatten_mask = atoms
            .iter()
            .find(|a| a.seq == *bake_seq && a.kind == AtomKind::BakeLayer)
            .and_then(|a| a.payload.get("flatten_mask").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        let (png, actual_hash) = replay_baked_blob(
            &store,
            &atoms,
            layer_id,
            *up_to_seq,
            flatten_mask,
            *bake_seq,
        )?;
        replayed_atom_count += atoms.iter().filter(|a| a.seq <= *up_to_seq).count();
        if actual_hash.to_string() != *expected_hash {
            if on_hash_mismatch == "strict" {
                return Err(yanshi_core::bake::hash_mismatch_error(
                    *bake_seq,
                    expected_hash,
                    &actual_hash.to_string(),
                ));
            }
            // append_rebake：真实哈希入库 + 日志末尾新 Bake 原子，绝不改历史。
            let new_hash = ctx.workspace.store().put(&png)?;
            debug_assert_eq!(new_hash, actual_hash);
            let result = ctx.commit(
                AtomKind::BakeLayer,
                json!({
                    "layer_id": layer_id,
                    "up_to_seq": up_to_seq,
                    "blob_hash": actual_hash.to_string(),
                    "flatten_mask": flatten_mask,
                    "replaces_bake_seq": bake_seq,
                }),
            )?;
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::LOSSY_PIXELS,
                "code": "HashMismatchRebaked",
                "message": format!("重放哈希不一致：期望 {expected_hash}，重建 {actual_hash}，已追加新 Bake 原子（seq {}），历史原子未改写", result.seq),
                "bake_seq": bake_seq,
                "expected_hash": expected_hash,
                "actual_hash": actual_hash.to_string(),
            }));
            healed.push(json!({
                "layer_id": layer_id,
                "bake_seq": bake_seq,
                "expected_hash": expected_hash,
                "actual_hash": actual_hash.to_string(),
                "mode": "append_rebake",
                "new_bake_atom_seq": result.seq,
            }));
            continue;
        }
        ctx.workspace.store().put(&png)?;
        healed.push(json!({
            "layer_id": layer_id,
            "bake_seq": bake_seq,
            "blob_hash": expected_hash,
            "mode": "healed",
        }));
        warnings.push(json!({
            "level": yanshi_core::bake::warning_level::INFO,
            "code": "ReconstructedMissingBlobFromLog",
            "message": format!("缺失压平基底已由原子日志重放重建并写回（图层 {layer_id}，bake_seq {bake_seq}）"),
            "layer_id": layer_id,
            "bake_seq": bake_seq,
            "blob_hash": expected_hash,
        }));
    }
    Ok(json!({
        "recovered": healed.len(),
        "healed_blobs": healed,
        "replayed_ms": started.elapsed().as_millis(),
        "replayed_atom_count": replayed_atom_count,
        "warnings": warnings,
    }))
}

/// Undo 原子级拦截（第二轮第 10 条）：在 Revert 提交**之前**模拟提交后
/// 的有效 Bake 集；若其中任一有效基底缺 Blob，拒绝提交（状态不回退、
/// 有效集不污染），并返回结构化指引。
pub(crate) fn guard_revert_missing_historical_blob(
    ctx: &mut ToolContext<'_>,
    target_atom_id: &str,
) -> Result<()> {
    let (atoms, head_seq) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        (document.log().atoms().to_vec(), document.head_seq())
    };
    // 虚拟 Revert 原子（seq = head+1，不进日志，只用于有效集模拟）。
    let mut virtual_revert = Atom::new(
        AtomKind::Revert,
        "guard".to_owned(),
        "guard".to_owned(),
        serde_json::json!({"target": target_atom_id}),
    );
    virtual_revert.id = "__guard_revert__".to_owned();
    virtual_revert.seq = head_seq + 1;
    let mut simulated = atoms.clone();
    simulated.push(virtual_revert);
    let store = ctx.workspace.store();
    for (layer_id, _bake_seq, _up_to_seq, blob_hash, _atom_id) in effective_baked_blobs(&simulated)
    {
        let hash: BlobHash = match blob_hash.parse() {
            Ok(hash) => hash,
            Err(_) => continue,
        };
        if store.exists(&hash) {
            continue;
        }
        // 找出该基底对应的 Bake 原子 seq（用于指引里的 required_bake_seq）。
        let required_bake_seq = simulated
            .iter()
            .find(|a| {
                a.kind == AtomKind::BakeLayer
                    && a.payload.get("layer_id").and_then(|v| v.as_str()) == Some(layer_id.as_str())
                    && a.payload.get("blob_hash").and_then(|v| v.as_str())
                        == Some(blob_hash.as_str())
            })
            .map(|a| a.seq)
            .unwrap_or(0);
        let undone_bake_seq = atoms
            .iter()
            .find(|a| a.id == target_atom_id)
            .map(|a| a.seq)
            .unwrap_or(0);
        return Err(yanshi_core::bake::undo_missing_historical_blob_error(
            &layer_id,
            undone_bake_seq,
            required_bake_seq,
            &blob_hash,
        ));
    }
    Ok(())
}
