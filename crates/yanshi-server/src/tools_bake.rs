//! bake_layer / bake_document / recover_baked_blobs（专家评审定版 + 第二轮 12 条）。
//!
//! 口径要点（与 `docs/` 里的 bake 说明同源，工具回执以这里为准）：
//! * 单层 scope（bake_layer）与文档级 scope（bake_document，单原子、多层
//!   条目）；基底是画布 1:1 直通 RGBA8 PNG，锚点 (0,0)，不接受缩放参数。
//! * 压平渲染用克隆状态去外壳（opacity=1、blend=normal、clipping=false，
//!   默认 mask 剥离），否则外壳会在之后整幅合成里被乘第二次。
//! * 掩膜例外（D11 定夺）：`flatten_mask=true` 时基底像素已含掩膜，折叠期
//!   消费掩膜（清层 `mask_id`），否则合成再乘一次成二次作用；消费后对该层
//!   的掩膜编辑由工具层拒绝（须先 revert 该 Bake），见
//!   `guard_mask_link_not_flatten_baked`。
//! * 门禁（第二轮第 12 条）：`allow_rasterize_editable` / `allow_clip_off_canvas`
//!   默认均为 false；含文本、矢量或越界图元时默认直接返回工具错误
//!   （is_error），绝不只塞进 warnings。`reject_on_lossy` 已废弃，不再接受。
//! * 自愈（第二轮第 4、5 条）：常规折叠/渲染缺 Blob 严格报错，绝不自动重放；
//!   显式入口只有 `recover_baked_blobs` 与导入的 `missing_blob_policy`。
//! * 哈希不一致缺省 strict；显式 `on_hash_mismatch: "append_rebake"` 时才
//!   把真实哈希入库并在日志末尾追加一条新 Bake 原子（单层形态，带
//!   `replaces_bake_seq` 审计元数据），绝不改写历史原子。

use super::*;
use yanshi_core::bake::{is_bake_content_kind, is_bake_kind, Affine2D};
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

/// 有效 Bake 的一行引用（BakeLayer 原子一行，BakeDocument 原子按条目展开
/// 多行；每层取 seq 最大的有效者时不分种类）。
#[derive(Debug, Clone)]
pub(crate) struct BakedBlobCandidate {
    pub(crate) layer_id: String,
    pub(crate) bake_seq: u64,
    pub(crate) up_to_seq: u64,
    pub(crate) blob_hash: String,
    pub(crate) flatten_mask: bool,
}

impl BakedBlobCandidate {
    fn new(
        layer_id: &str,
        bake_seq: u64,
        up_to_seq: u64,
        blob_hash: &str,
        flatten_mask: bool,
    ) -> Self {
        Self {
            layer_id: layer_id.to_owned(),
            bake_seq,
            up_to_seq,
            blob_hash: blob_hash.to_owned(),
            flatten_mask,
        }
    }
}

/// 全部未被抑制的 Bake 原子（每层多条，不只取最新一条；文档级原子按条目展开）。
///
/// recover_baked_blobs 必须用它而不能只用 effective_baked_blobs：
/// Undo 场景要恢复的恰恰是被新 Bake 取代的**历史**基底（Undo 当前 Bake
/// 后它会重新生效）；只取每层最新一条会漏掉它，恢复回执 recovered=0，
/// 随后 Undo 仍被拦截（判据 ⑦ 的红即由此来）。
pub(crate) fn all_valid_baked_blobs(atoms: &[Atom]) -> Vec<BakedBlobCandidate> {
    let suppressed = yanshi_core::fold::compute_suppressed(atoms);
    let mut out: Vec<BakedBlobCandidate> = Vec::new();
    for atom in atoms {
        if !is_bake_kind(atom.kind) || suppressed.contains(&atom.id) {
            continue;
        }
        if atom.kind == AtomKind::BakeDocument {
            for (layer_id, up_to_seq, blob_hash, flatten_mask) in
                yanshi_core::bake::bake_document_entries(atom)
            {
                out.push(BakedBlobCandidate::new(
                    &layer_id,
                    atom.seq,
                    up_to_seq,
                    &blob_hash,
                    flatten_mask,
                ));
            }
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
        let flatten_mask = atom
            .payload
            .get("flatten_mask")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        out.push(BakedBlobCandidate::new(
            layer_id,
            atom.seq,
            up_to_seq,
            blob_hash,
            flatten_mask,
        ));
    }
    out
}

pub(crate) fn effective_baked_blobs(atoms: &[Atom]) -> Vec<BakedBlobCandidate> {
    let mut latest: std::collections::BTreeMap<String, BakedBlobCandidate> =
        std::collections::BTreeMap::new();
    for candidate in all_valid_baked_blobs(atoms) {
        let entry = latest
            .entry(candidate.layer_id.clone())
            .or_insert_with(|| candidate.clone());
        if candidate.bake_seq >= entry.bake_seq {
            *entry = candidate;
        }
    }
    latest.into_values().collect()
}

/// 某层最新有效 Bake（不分种类）；无有效 Bake 时为 None。
/// 掩膜守卫用它判定「最新有效 Bake 是否为 flatten bake」：只有最新者为
/// flatten 时才拒绝掩膜编辑；之后一次普通 bake 会解除守卫（新掩膜只作用一次）。
pub(crate) fn latest_effective_bake_for_layer(
    atoms: &[Atom],
    layer_id: &str,
) -> Option<BakedBlobCandidate> {
    effective_baked_blobs(atoms)
        .into_iter()
        .find(|c| c.layer_id == layer_id)
}

/// 重放自愈核心：用 `seq <= up_to_seq` 且排除目标 Bake 原子（不分种类）
/// 的子集建临时文档（旁路剪枝屏障），渲染目标层透明基底并编码 PNG。
/// 返回 `(png_bytes, blob_hash)`；调用方负责 SHA 校验与入库决策。
fn replay_baked_blob(
    store: &std::sync::Arc<dyn yanshi_core::blob::BlobStore>,
    atoms: &[Atom],
    layer_id: &str,
    up_to_seq: u64,
    flatten_mask: bool,
    target_bake_seq: u64,
) -> Result<(Vec<u8>, BlobHash)> {
    // **重放子集必须含更早的 Bake**（只排除目标 Bake 本身，不分种类）：
    // 压平后追加的 Move/Transform 瞄准的是基底对象（`baked:{layer}`）；
    // 若把全部 Bake 排除，变换原子失去目标、折叠时只剩级联警告，
    // 重放渲染的是未变换的原始笔触，哈希恒不一致（判据 ⑪ 的红即由此来）。
    // 目标 Bake 自身的 seq 恒大于 up_to_seq（up_to_seq 是提交前 head），
    // 这里再显式按 seq 排除一次，防畸形原子（up_to_seq 指到自身之后）。
    // 更早 Bake 的 Blob 必须已在存储中（recover 按 seq 顺序先修更早者；
    // 导入自愈里缺更早 Blob 时渲染直接报 missing_baked_blob，不静默）。
    let subset: Vec<Atom> = atoms
        .iter()
        .filter(|a| a.seq <= up_to_seq && !(is_bake_kind(a.kind) && a.seq == target_bake_seq))
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

/// 一层 live 对象的分项计数（bake_layer 与 bake_document 共用同一口径：
/// 门禁判定必须两工具一致，任一工具私自改口径都会造成另一工具的静默放行）。
struct LayerObjectCounts {
    strokes: usize,
    texts: usize,
    vectors: usize,
    images: usize,
    liquify: usize,
    adjustments: usize,
    filters: usize,
    text_ids: Vec<String>,
    vector_ids: Vec<String>,
    clipped: Vec<Value>,
}

fn count_layer_objects(
    objects: &[yanshi_core::Object],
    canvas_w: u32,
    canvas_h: u32,
) -> LayerObjectCounts {
    let mut counts = LayerObjectCounts {
        strokes: 0,
        texts: 0,
        vectors: 0,
        images: 0,
        liquify: 0,
        adjustments: 0,
        filters: 0,
        text_ids: Vec::new(),
        vector_ids: Vec::new(),
        clipped: Vec::new(),
    };
    for object in objects {
        match object.object_type {
            yanshi_core::ObjectType::Stroke => counts.strokes += 1,
            yanshi_core::ObjectType::Text => {
                counts.texts += 1;
                counts.text_ids.push(object.id.clone());
            }
            yanshi_core::ObjectType::Shape | yanshi_core::ObjectType::Path => {
                counts.vectors += 1;
                counts.vector_ids.push(object.id.clone());
            }
            yanshi_core::ObjectType::RasterPatch => {
                if object.data.get("baked_base").and_then(|v| v.as_bool()) == Some(true) {
                    // 已压平的基底本身不是“新图元”：再压平时它会被旧基底渲染取代。
                } else {
                    counts.images += 1;
                }
            }
            yanshi_core::ObjectType::Liquify => counts.liquify += 1,
            yanshi_core::ObjectType::Adjustment => counts.adjustments += 1,
            yanshi_core::ObjectType::Filter => counts.filters += 1,
            _ => {}
        }
        // 越界判定：用带变换的包围盒与画布求交之外的部分。
        if let Some(bbox) = yanshi_render::object::object_bbox(object) {
            let x0 = bbox.x;
            let y0 = bbox.y;
            let x1 = bbox.x + bbox.w;
            let y1 = bbox.y + bbox.h;
            if x0 < 0.0 || y0 < 0.0 || x1 > canvas_w as f64 || y1 > canvas_h as f64 {
                counts.clipped.push(json!({
                    "object_id": object.id,
                    "bbox": [x0, y0, x1, y1],
                    "canvas": [0.0, 0.0, canvas_w as f64, canvas_h as f64],
                }));
            }
        }
    }
    counts
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
    let LayerObjectCounts {
        strokes,
        texts,
        vectors,
        images,
        liquify,
        adjustments,
        filters,
        text_ids,
        vector_ids,
        clipped,
    } = count_layer_objects(&objects, canvas_w, canvas_h);
    let single_baked = objects.len() == 1
        && objects[0].data.get("baked_base").and_then(|v| v.as_bool()) == Some(true)
        && objects[0].transform.matrix == Affine2D::IDENTITY.0;
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

/// 按快照算层有效可见性：与渲染侧 `effective_layer_factors` 同口径——
/// 本层 visible 与父链 visible 相与；遇缺失/已删除父层断链（不取该父值）；
/// 64 跳上限防环。D13 Q1：父组不可见而子层 visible=true 的子层仍算不可见。
fn snapshot_effective_visible(
    parents: &std::collections::BTreeMap<String, (bool, Option<String>, bool)>,
    layer_id: &str,
) -> bool {
    let mut visible = parents.get(layer_id).map(|(v, _, _)| *v).unwrap_or(false);
    let mut cursor = parents.get(layer_id).and_then(|(_, p, _)| p.clone());
    let mut hops = 0usize;
    while let Some(up) = cursor {
        if hops > 64 {
            break;
        }
        let Some((parent_visible, grandparent, parent_deleted)) = parents.get(&up) else {
            break;
        };
        if *parent_deleted {
            break;
        }
        visible = visible && *parent_visible;
        cursor = grandparent.clone();
        hops += 1;
    }
    visible
}

/// 掩膜编辑守卫（D11 配套规则）：目标层的最新有效 Bake（不分种类）为
/// flatten bake 时，拒绝给该层挂掩膜（`update_layer` 的 `mask_id` patch）。
/// 否则折叠期已消费的 mask_id 会被复活，合成再乘一次成二次作用。
/// 守卫只在工具层：直接经原子提交的旁路不在此拦截（与一期工具层门禁同口径）。
/// 之后一次普通 bake（flatten_mask=false）会解除守卫：新掩膜只作用一次。
pub(crate) fn guard_mask_link_not_flatten_baked(
    ctx: &mut ToolContext<'_>,
    layer_id: &str,
) -> Result<()> {
    let atoms = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        document.log().atoms().to_vec()
    };
    if let Some(latest) = latest_effective_bake_for_layer(&atoms, layer_id) {
        if latest.flatten_mask {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "图层 {layer_id} 的掩膜已压进基底（bake_seq {}，flatten_mask=true）：掩膜编辑已拒绝，须先 revert 该 Bake 原子才能改掩膜",
                    latest.bake_seq
                )),
            ));
        }
    }
    Ok(())
}

/// bake_document 工具（文档级逐层压平，单原子、多层条目）。
///
/// 顺序沿用一期：调用开始一次快照（文档头 seq）→ 枚举分类 → 统一门禁
/// （任一待压平层触发即整单工具错误）→ 逐层孤立渲染 → 逐层写 Blob 并校验
/// → 最后提交单个 BakeDocument 原子。全部层 no-op 时不追加原子、不推进水位。
/// 计数口径：`layers` 含待压平层与 no-op 层（空层 no-op 同时进跳过清单）；
/// 删除层不枚举，组层无自有对象不是候选（D13 Q2）；跳过层不推进水位、
/// 不产生 Blob。no-op 层的取代计数按本次调用计（0），不追溯旧 Bake 的范围。
pub(super) fn write_bake_document(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
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
    // ---- 快照：调用开始时只取一次文档头 seq + 全部层（含 live 对象）----
    struct LayerSnap {
        id: String,
        name: String,
        z_index: i64,
        is_group: bool,
        deleted: bool,
        mask_id: Option<String>,
        objects: Vec<yanshi_core::Object>,
    }
    let (canvas_w, canvas_h, document_up_to_seq, snaps, parents) = {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let state = document.state();
        let mut snaps: Vec<LayerSnap> = Vec::new();
        let mut parents: std::collections::BTreeMap<String, (bool, Option<String>, bool)> =
            std::collections::BTreeMap::new();
        for layer in state.layers.values() {
            parents.insert(
                layer.id.clone(),
                (layer.visible, layer.parent_id.clone(), layer.is_deleted()),
            );
            snaps.push(LayerSnap {
                id: layer.id.clone(),
                name: layer.name.clone(),
                z_index: layer.z_index,
                is_group: layer.layer_type == yanshi_core::LayerType::LayerGroup,
                deleted: layer.is_deleted(),
                mask_id: layer.mask_id.clone(),
                objects: state
                    .objects_in_layer(&layer.id)
                    .into_iter()
                    .cloned()
                    .collect(),
            });
        }
        // Z 序自底向上；z_index 相同按 id 确定顺序。
        snaps.sort_by_key(|a| (a.z_index, a.id.clone()));
        (
            state.width,
            state.height,
            document.head_seq(),
            snaps,
            parents,
        )
    };
    // ---- 枚举分类（Z 序）----
    // 待压平层：(snap 下标, 图元计数)。
    let mut to_bake: Vec<(usize, LayerObjectCounts)> = Vec::new();
    // layers[] 行：(z_index, 行)；no-op 行分类时即填好，applied 行提交后补。
    let mut layer_rows: Vec<(i64, Value)> = Vec::new();
    let mut skipped_layers: Vec<Value> = Vec::new();
    let mut skip_warnings: Vec<Value> = Vec::new();
    let mut no_op_already_count = 0usize;
    for (idx, snap) in snaps.iter().enumerate() {
        // 删除层不枚举（审计完整性见设计 4.4 的 DELETED_LAYER 码位）。
        if snap.deleted {
            continue;
        }
        // 组层无自有对象不是候选，不进跳过清单（容器不是内容层，D13 Q2）。
        if snap.is_group && snap.objects.is_empty() {
            continue;
        }
        let snap_id = snap.id.clone();
        let snap_name = snap.name.clone();
        if !snapshot_effective_visible(&parents, &snap.id) {
            skipped_layers.push(json!({
                "layer_id": snap_id,
                "layer_name": snap_name,
                "reason": "HIDDEN_LAYER",
            }));
            skip_warnings.push(json!({
                "level": yanshi_core::bake::warning_level::INFO,
                "code": "HIDDEN_LAYER_SKIPPED",
                "message": format!("隐藏层 {snap_id} 已跳过：未压平，历史未被剪枝"),
                "layer_id": snap_id,
            }));
            continue;
        }
        if snap.objects.is_empty() {
            layer_rows.push((snap.z_index, json!({
                "layer_id": snap_id,
                "layer_name": snap_name,
                "status": "no_op_empty_layer",
                "bake_atom_seq": Value::Null,
                "blob_hash": Value::Null,
                "blob_byte_len": Value::Null,
                "dimensions": {"width": canvas_w, "height": canvas_h},
                "superseded_seq_range": [document_up_to_seq, document_up_to_seq],
                "superseded_atom_count": 0,
                "collapsed_primitives": {"strokes": 0, "texts": 0, "vectors": 0, "images": 0, "liquify": 0, "adjustments": 0, "filters": 0},
                "warnings": [{
                    "level": yanshi_core::bake::warning_level::INFO,
                    "code": "NO_OP_EMPTY_LAYER",
                    "message": "目标层为空：未生成 Blob，未追加原子",
                    "layer_id": snap_id,
                }],
            })));
            skipped_layers.push(json!({
                "layer_id": snap_id,
                "layer_name": snap_name,
                "reason": "EMPTY_LAYER",
            }));
            continue;
        }
        // 纯调整层：层内只有 Adjustment 或 Filter 对象（设计 1.5 节）。
        if snap.objects.iter().all(|o| {
            matches!(
                o.object_type,
                yanshi_core::ObjectType::Adjustment | yanshi_core::ObjectType::Filter
            )
        }) {
            skipped_layers.push(json!({
                "layer_id": snap_id,
                "layer_name": snap_name,
                "reason": "ADJUSTMENT_ONLY_LAYER",
            }));
            skip_warnings.push(json!({
                "level": yanshi_core::bake::warning_level::INFO,
                "code": "ADJUSTMENT_ONLY_LAYER_SKIPPED",
                "message": format!("纯调整层 {snap_id} 已跳过：孤立压平会冻结无意义基底并丢失对下层的作用"),
                "layer_id": snap_id,
            }));
            continue;
        }
        // no-op 已压平：仅有 1 个未改动 BakedBase 且其后无新内容原子。
        let single_baked = snap.objects.len() == 1
            && snap.objects[0]
                .data
                .get("baked_base")
                .and_then(|v| v.as_bool())
                == Some(true)
            && snap.objects[0].transform.matrix == Affine2D::IDENTITY.0;
        if single_baked {
            let source_up_to_seq = snap.objects[0]
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
                                == Some(snap.id.as_str())
                    })
                })
                .unwrap_or(false);
            if !has_new_content {
                let existing_hash = snap.objects[0]
                    .data
                    .get("blob_hash")
                    .cloned()
                    .unwrap_or(Value::Null);
                let existing_len = existing_hash
                    .as_str()
                    .and_then(|h| h.parse::<BlobHash>().ok())
                    .and_then(|hash| ctx.workspace.store().get(&hash).ok())
                    .map(|bytes| bytes.len());
                layer_rows.push((snap.z_index, json!({
                    "layer_id": snap_id,
                    "layer_name": snap_name,
                    "status": "no_op_already_baked",
                    "bake_atom_seq": Value::Null,
                    "blob_hash": existing_hash,
                    "blob_byte_len": existing_len,
                    "dimensions": {"width": canvas_w, "height": canvas_h},
                    "superseded_seq_range": [document_up_to_seq, document_up_to_seq],
                    "superseded_atom_count": 0,
                    "collapsed_primitives": {"strokes": 0, "texts": 0, "vectors": 0, "images": 0, "liquify": 0, "adjustments": 0, "filters": 0},
                    "warnings": [{
                        "level": yanshi_core::bake::warning_level::INFO,
                        "code": "NO_OP_ALREADY_BAKED",
                        "message": "目标层仅有 1 个未改动 BakedBase：未生成新 Blob，未追加原子",
                        "layer_id": snap_id,
                    }],
                })));
                no_op_already_count += 1;
                continue;
            }
        }
        let counts = count_layer_objects(&snap.objects, canvas_w, canvas_h);
        to_bake.push((idx, counts));
    }
    // ---- 统一门禁：任一待压平层触发即整单工具错误（跳过层不触发）----
    let gate_text_layers: Vec<&str> = to_bake
        .iter()
        .filter(|(_, c)| c.texts > 0 || c.vectors > 0)
        .map(|(idx, _)| snaps[*idx].id.as_str())
        .collect();
    if !gate_text_layers.is_empty() && !allow_rasterize_editable {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "待压平层含文本或矢量图元（层：{}）：默认不栅格化可编辑图元 ⇒ 请显式传 allow_rasterize_editable=true 确认（回执 warnings 将记 TEXT_RASTERIZED / VECTOR_RASTERIZED 审计条目）",
                gate_text_layers.join("、")
            )),
        ));
    }
    let gate_clip_layers: Vec<&str> = to_bake
        .iter()
        .filter(|(_, c)| !c.clipped.is_empty())
        .map(|(idx, _)| snaps[*idx].id.as_str())
        .collect();
    if !gate_clip_layers.is_empty() && !allow_clip_off_canvas {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "待压平层有图元越界（层：{}）：默认不丢弃越界像素 ⇒ 请显式传 allow_clip_off_canvas=true 确认（回执 warnings 将记 CLIPPED_OFF_CANVAS_PIXELS 明细）",
                gate_clip_layers.join("、")
            )),
        ));
    }
    // ---- 逐层孤立渲染 + 写 Blob 并校验（失败则日志零修改）----
    struct BakedLayer {
        snap_idx: usize,
        blob_hash: String,
        byte_len: usize,
        superseded_atom_count: usize,
        superseded_min_seq: u64,
        counts: LayerObjectCounts,
        warnings: Vec<Value>,
    }
    let mut baked: Vec<BakedLayer> = Vec::new();
    for (snap_idx, counts) in to_bake {
        let snap = &snaps[snap_idx];
        let snap_id = snap.id.clone();
        let snap_mask = snap.mask_id.clone();
        let (bw, bh, rgba) = ctx
            .workspace
            .document_mut(&ctx.doc_id)?
            .render_layer_bake_base(&snap.id, flatten_mask)?;
        let mut png_input = try_reserve_bytes(rgba.len())?;
        png_input.extend_from_slice(&rgba);
        let png = yanshi_render::png::encode_png(bw, bh, &png_input).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("bake_document PNG 编码失败：日志未改动，可直接重试"),
            )
        })?;
        let blob_hash = ctx.workspace.store().put(&png)?;
        let stored = ctx.workspace.store().get(&blob_hash)?;
        if stored != png {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "bake_document Blob 写回校验失败：存储内容与编码字节不一致，日志未改动",
                ),
            ));
        }
        if BlobHash::from_bytes(&png) != blob_hash {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail("bake_document Blob 哈希校验失败：日志未改动"),
            ));
        }
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
                if atom.seq > document_up_to_seq || suppressed.contains(&atom.id) {
                    continue;
                }
                if !is_bake_content_kind(atom.kind) {
                    continue;
                }
                let target = yanshi_core::bake::bake_target_layer(atom, |id| {
                    document.state().objects.get(id).map(|o| o.layer_id.clone())
                });
                if target.as_deref() == Some(snap.id.as_str()) {
                    count += 1;
                    min_seq = min_seq.min(atom.seq);
                }
            }
            (
                count,
                if min_seq == u64::MAX {
                    document_up_to_seq
                } else {
                    min_seq
                },
            )
        };
        let mut warnings: Vec<Value> = Vec::new();
        if counts.liquify > 0 {
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::INFO,
                "code": "LIQUIFY_BAKED_INTO_PIXELS",
                "message": format!("层内液化 {} 个已冻结进基底像素（原液化网格被新水位剪枝）", counts.liquify),
                "count": counts.liquify,
                "layer_id": snap_id,
            }));
        }
        if counts.texts > 0 {
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
                "code": "TEXT_RASTERIZED",
                "message": format!("文本 {} 个已栅格化，不可再编辑文本内容", counts.texts),
                "count": counts.texts,
                "object_ids": counts.text_ids.clone(),
                "layer_id": snap_id,
            }));
        }
        if counts.vectors > 0 {
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
                "code": "VECTOR_RASTERIZED",
                "message": format!("矢量 {} 个已栅格化，不可再编辑矢量节点", counts.vectors),
                "count": counts.vectors,
                "object_ids": counts.vector_ids.clone(),
                "layer_id": snap_id,
            }));
        }
        if flatten_mask && snap_mask.is_some() {
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::LOSSY_EDITABILITY,
                "code": "MASK_FLATTENED",
                "message": "图层级独立掩膜已压进基底（flatten_mask=true），掩膜不再独立存在",
                "mask_id": snap_mask,
                "layer_id": snap_id,
            }));
        }
        if !counts.clipped.is_empty() {
            warnings.push(json!({
                "level": yanshi_core::bake::warning_level::LOSSY_PIXELS,
                "code": "CLIPPED_OFF_CANVAS_PIXELS",
                "message": format!("越界图元 {} 个的画布外像素已被裁剪（未静默丢弃：区域坐标见 clipped_regions）", counts.clipped.len()),
                "count": counts.clipped.len(),
                "clipped_regions": counts.clipped.clone(),
                "layer_id": snap_id,
            }));
        }
        baked.push(BakedLayer {
            snap_idx,
            blob_hash: blob_hash.to_string(),
            byte_len: png.len(),
            superseded_atom_count,
            superseded_min_seq,
            counts,
            warnings,
        });
    }
    // ---- 提交单个文档级原子（全部层 no-op 时不提交）----
    let total_effective = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| {
            let suppressed = yanshi_core::fold::compute_suppressed(document.log().atoms());
            document.log().atoms().len() - suppressed.len()
        })
        .unwrap_or(0);
    let superseded_total: usize = baked.iter().map(|b| b.superseded_atom_count).sum();
    let retained_atom_count = total_effective.saturating_sub(superseded_total);
    let total_blob_byte_len: usize = baked.iter().map(|b| b.byte_len).sum();
    let layer_count_applied = baked.len();
    let layer_count_no_op = layer_rows.len();
    let layer_count_skipped = skipped_layers.len();
    if baked.is_empty() {
        // 全部层 no-op：不追加原子、不推进水位（D12）。
        let status = if no_op_already_count > 0 {
            "no_op_all_already_baked"
        } else {
            "no_op_all_empty"
        };
        layer_rows.sort_by_key(|(z, _)| *z);
        let layers: Vec<Value> = layer_rows.into_iter().map(|(_, v)| v).collect();
        let mut warnings: Vec<Value> = Vec::new();
        for row in &layers {
            if let Some(ws) = row.get("warnings").and_then(|w| w.as_array()) {
                warnings.extend(ws.iter().cloned());
            }
        }
        warnings.extend(skip_warnings);
        return Ok(json!({
            "status": status,
            "scope": "document",
            "bake_atom_seq": Value::Null,
            "bake_atom_id": Value::Null,
            "document_up_to_seq": document_up_to_seq,
            "dimensions": {"width": canvas_w, "height": canvas_h},
            "layer_count_applied": 0,
            "layer_count_no_op": layer_count_no_op,
            "layer_count_skipped": layer_count_skipped,
            "superseded_atom_count": 0,
            "retained_atom_count": retained_atom_count,
            "total_blob_byte_len": 0,
            "layers": layers,
            "skipped_layers": skipped_layers,
            "warnings": warnings,
        }));
    }
    let entries: Vec<Value> = baked
        .iter()
        .map(|b| {
            json!({
                "layer_id": snaps[b.snap_idx].id.clone(),
                "up_to_seq": document_up_to_seq,
                "blob_hash": b.blob_hash.clone(),
                "flatten_mask": flatten_mask,
            })
        })
        .collect();
    let result = ctx.commit(
        AtomKind::BakeDocument,
        json!({
            "document_up_to_seq": document_up_to_seq,
            "layers": entries,
        }),
    )?;
    let mut value = finish_mutation(ctx, &result, None)?;
    for b in &baked {
        let snap = &snaps[b.snap_idx];
        let counts = &b.counts;
        layer_rows.push((
            snap.z_index,
            json!({
                "layer_id": snap.id.clone(),
                "layer_name": snap.name.clone(),
                "status": "applied",
                "bake_atom_seq": result.seq,
                "blob_hash": b.blob_hash.clone(),
                "blob_byte_len": b.byte_len,
                "dimensions": {"width": canvas_w, "height": canvas_h},
                "superseded_seq_range": [b.superseded_min_seq, document_up_to_seq],
                "superseded_atom_count": b.superseded_atom_count,
                "collapsed_primitives": {
                    "strokes": counts.strokes,
                    "texts": counts.texts,
                    "vectors": counts.vectors,
                    "images": counts.images,
                    "liquify": counts.liquify,
                    "adjustments": counts.adjustments,
                    "filters": counts.filters,
                },
                "warnings": b.warnings.clone(),
            }),
        ));
    }
    layer_rows.sort_by_key(|(z, _)| *z);
    let layers: Vec<Value> = layer_rows.into_iter().map(|(_, v)| v).collect();
    let mut warnings: Vec<Value> = Vec::new();
    for row in &layers {
        if let Some(ws) = row.get("warnings").and_then(|w| w.as_array()) {
            warnings.extend(ws.iter().cloned());
        }
    }
    warnings.extend(skip_warnings);
    value["status"] = json!("applied");
    value["scope"] = json!("document");
    value["bake_atom_seq"] = json!(result.seq);
    value["bake_atom_id"] = json!(result.atom_id);
    value["document_up_to_seq"] = json!(document_up_to_seq);
    value["dimensions"] = json!({"width": canvas_w, "height": canvas_h});
    value["layer_count_applied"] = json!(layer_count_applied);
    value["layer_count_no_op"] = json!(layer_count_no_op);
    value["layer_count_skipped"] = json!(layer_count_skipped);
    value["superseded_atom_count"] = json!(superseded_total);
    value["retained_atom_count"] = json!(retained_atom_count);
    value["total_blob_byte_len"] = json!(total_blob_byte_len);
    value["layers"] = json!(layers);
    value["skipped_layers"] = json!(skipped_layers);
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
            candidates.retain(|c| &c.layer_id == layer_id);
        }
        if let Some(up_to_seq) = target_seq_filter {
            candidates.retain(|c| c.up_to_seq == up_to_seq);
        }
        (atoms, candidates)
    };
    let mut healed: Vec<Value> = Vec::new();
    let mut replayed_atom_count = 0usize;
    let mut warnings: Vec<Value> = Vec::new();
    for candidate in &candidates {
        let layer_id = &candidate.layer_id;
        let bake_seq = candidate.bake_seq;
        let up_to_seq = candidate.up_to_seq;
        let expected_hash = &candidate.blob_hash;
        // 该 Bake 记录的 flatten_mask（单层载荷或文档条目），重放须同口径。
        let flatten_mask = candidate.flatten_mask;
        let hash: BlobHash = expected_hash.parse().map_err(|_| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("Bake 原子里的 blob_hash 非法：{expected_hash}")),
            )
        })?;
        if store.exists(&hash) {
            continue;
        }
        let (png, actual_hash) =
            replay_baked_blob(&store, &atoms, layer_id, up_to_seq, flatten_mask, bake_seq)?;
        replayed_atom_count += atoms.iter().filter(|a| a.seq <= up_to_seq).count();
        if actual_hash.to_string() != *expected_hash {
            if on_hash_mismatch == "strict" {
                return Err(yanshi_core::bake::hash_mismatch_error(
                    bake_seq,
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
    for candidate in effective_baked_blobs(&simulated) {
        let layer_id = &candidate.layer_id;
        let blob_hash = &candidate.blob_hash;
        let hash: BlobHash = match blob_hash.parse() {
            Ok(hash) => hash,
            Err(_) => continue,
        };
        if store.exists(&hash) {
            continue;
        }
        // 找出该基底对应的 Bake 原子 seq（不分种类；文档条目按条目哈希匹配，
        // 用于指引里的 required_bake_seq）。
        let required_bake_seq = simulated
            .iter()
            .find(|a| {
                if a.kind == AtomKind::BakeLayer {
                    a.payload.get("layer_id").and_then(|v| v.as_str()) == Some(layer_id.as_str())
                        && a.payload.get("blob_hash").and_then(|v| v.as_str())
                            == Some(blob_hash.as_str())
                } else if a.kind == AtomKind::BakeDocument {
                    yanshi_core::bake::bake_document_entries(a)
                        .iter()
                        .any(|(lid, _, hash, _)| lid == layer_id && hash == blob_hash)
                } else {
                    false
                }
            })
            .map(|a| a.seq)
            .unwrap_or(0);
        let undone_bake_seq = atoms
            .iter()
            .find(|a| a.id == target_atom_id)
            .map(|a| a.seq)
            .unwrap_or(0);
        return Err(yanshi_core::bake::undo_missing_historical_blob_error(
            layer_id,
            undone_bake_seq,
            required_bake_seq,
            blob_hash,
        ));
    }
    Ok(())
}
