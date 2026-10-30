//! auto-bake（bake 二期 item 2）：阈值触发的搭便车压平。
//!
//! 口径（设计 `bake-phase2/designs/autobake.md` §9／§10，阈值经 2026-10-30 标定锁定）：
//! * 硬阈值 2048 未 bake 内容原子／层（标定拐点），软线 1433（硬阈值的 70%）。
//! * 开关两级：全局环境变量（缺省关）＋层元数据（显式值优先）；文档级暂缓，读全局。
//! * 触发是搭便车：触发调用完成后同回执报告；批量内不评估、整批后一次、一次最多一层。
//! * 门禁预授权与手动 bake 分离、缺省 false；门禁失败跳过＋告警且计数不重置。
//! * bake 失败进 sticky 受阻态：手动 bake／recover／改设置清除；v1 不自动 recover。
//! * v1 自动 bake 的 flatten_mask 恒 false（期 1 D11 双掩膜问题未修，修在 bake-document
//!   分支的共用折叠臂；本分支不碰那条臂，免得合入冲突）。
//! * 首版 Native／服务端，不含 WASM。

use super::tools_bake::{census_layer_objects, effective_baked_blobs, render_and_commit_bake};
use super::*;
use yanshi_core::bake::{bake_target_layer, compute_bake_horizon, is_bake_content_kind};
use yanshi_core::fold::compute_suppressed;
use yanshi_core::AtomKind;

/// 硬阈值：每层未 bake 内容原子数达到即触发自动 bake（标定锁定 2026-10-30）。
pub(crate) const AUTO_BAKE_HARD_THRESHOLD: u64 = 2048;
/// 软线：硬阈值的 70%（2048 × 70% = 1433.6，向下取整）。
pub(crate) const AUTO_BAKE_SOFT_LINE: u64 = 1433;
/// 全局开关环境变量：`1`／`true` 为开，缺省关（每次评估现读）。
pub(crate) const AUTO_BAKE_GLOBAL_ENV: &str = "YANSHI_AUTO_BAKE_ENABLED";

/// 层元数据键：层开关与三项预授权（走 update_layer SetProperty，见 LAYER_PATCH_KEYS）。
pub(crate) const META_AUTO_BAKE: &str = "auto_bake";
pub(crate) const META_ALLOW_AUTO_RASTERIZE: &str = "allow_auto_rasterize_editable";
pub(crate) const META_ALLOW_AUTO_CLIP: &str = "allow_auto_clip_off_canvas";
pub(crate) const META_ALLOW_AUTO_FLATTEN: &str = "allow_auto_flatten_mask";

/// 永不触发评估的工具（设计 §9 决定 8 的排除表＋update_layer：SetProperty 是内容类
/// 原子，但 update_layer 本身是元数据操作，评估它会把“改设置”误当成“写内容”；
/// 批量不在此表——批量子调用不走 registry.call，整批只在 registry.call 看到一次）。
const NEVER_EVAL_TOOLS: [&str; 17] = [
    "bake_layer",
    "recover_baked_blobs",
    "import_image",
    "import_project",
    "import_psd",
    "import_psd_layers",
    "revert",
    "revert_changeset",
    "revert_group",
    "revert_to_seq",
    "revert_atoms",
    "revert_to",
    "undo_last",
    "redo_last",
    "reapply",
    "restore_checkpoint",
    "update_layer",
];

/// 全局开关（每次评估现读环境变量，不缓存——开关翻转对下一次调用立即生效）。
pub(crate) fn auto_bake_global_enabled() -> bool {
    matches!(
        std::env::var(AUTO_BAKE_GLOBAL_ENV).as_deref(),
        Ok("1") | Ok("true")
    )
}

/// 层元数据里的布尔值（非布尔一律当作未设置：fold 侧非布尔保持旧值，工具层在
/// update_layer 已把非布尔挡在外面，这里不猜默认值）。
fn layer_metadata_bool(
    state: &yanshi_core::DocumentState,
    layer_id: &str,
    key: &str,
) -> Option<bool> {
    state
        .layers
        .get(layer_id)
        .and_then(|layer| layer.metadata.get(key))
        .and_then(Value::as_bool)
}

/// 层开关解析：层显式值优先，否则读全局；返回（是否启用，来源）。
fn effective_enabled(state: &yanshi_core::DocumentState, layer_id: &str) -> (bool, &'static str) {
    match layer_metadata_bool(state, layer_id, META_AUTO_BAKE) {
        Some(value) => (value, "layer"),
        None => (auto_bake_global_enabled(), "global"),
    }
}

/// 层预授权三项（缺省均为 false；文档级预授权与文档开关一并暂缓）。
fn layer_preauth(state: &yanshi_core::DocumentState, layer_id: &str) -> (bool, bool, bool) {
    (
        layer_metadata_bool(state, layer_id, META_ALLOW_AUTO_RASTERIZE).unwrap_or(false),
        layer_metadata_bool(state, layer_id, META_ALLOW_AUTO_CLIP).unwrap_or(false),
        layer_metadata_bool(state, layer_id, META_ALLOW_AUTO_FLATTEN).unwrap_or(false),
    )
}

/// 每层未 bake 内容原子数（与手动 bake 的水位口径一致：有效 Bake 的 max up_to_seq
/// 之上、未被抑制的内容类原子，按当前对象归属落层；已删除层不计）。
fn unbaked_counts(document: &crate::document::Document) -> std::collections::BTreeMap<String, u64> {
    let atoms = document.log().atoms();
    let suppressed = compute_suppressed(atoms);
    let horizon = compute_bake_horizon(atoms, &suppressed);
    let mut counts: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for atom in atoms {
        if suppressed.contains(&atom.id) || !is_bake_content_kind(atom.kind) {
            continue;
        }
        let Some(layer_id) = bake_target_layer(atom, |id| {
            document.state().objects.get(id).map(|o| o.layer_id.clone())
        }) else {
            continue;
        };
        if document
            .state()
            .layers
            .get(&layer_id)
            .map(|l| l.is_deleted())
            .unwrap_or(true)
        {
            continue;
        }
        let water = horizon.get(&layer_id).copied().unwrap_or(0);
        if atom.seq > water {
            *counts.entry(layer_id).or_insert(0) += 1;
        }
    }
    counts
}

/// 某层的最新有效 Bake（用于 get_bake_status 的 last_bake 与 revert 拦截的注脚）。
/// origin 读法：载荷 origin == "auto" 为自动，否则为手动（含期 1 老原子那种
/// commit 填的 "generated"——老原子没有自动 bake 一说，归手动是对的）。
fn last_bake_for_layer(
    atoms: &[yanshi_core::Atom],
    layer_id: &str,
) -> Option<(u64, String, String, String, String)> {
    let mut found: Option<(u64, String, String, String, String)> = None;
    for candidate in effective_baked_blobs(atoms) {
        if candidate.layer_id != layer_id {
            continue;
        }
        let bake_seq = candidate.bake_seq;
        if found.as_ref().map(|(seq, _, _, _, _)| *seq).unwrap_or(0) > bake_seq {
            continue;
        }
        // 烘焙原子按 seq 找回：BakeLayer 候选对应其 BakeLayer 原子（旧元组
        // 的 atom_id 即此），BakeDocument 候选对应其 BakeDocument 原子。
        let bake_atom = atoms.iter().find(|a| a.seq == bake_seq);
        let atom_id = bake_atom.map(|a| a.id.clone()).unwrap_or_default();
        let origin = atoms
            .iter()
            .find(|a| a.seq == bake_seq && a.kind == AtomKind::BakeLayer)
            .and_then(|a| a.payload.get("origin").and_then(|v| v.as_str()))
            .unwrap_or("manual");
        let origin = if origin == "auto" { "auto" } else { "manual" };
        // trigger 是对象（自动 bake）或缺失（手动 bake）：取 kind，无 kind 则读 manual。
        let trigger = atoms
            .iter()
            .find(|a| a.seq == bake_seq && a.kind == AtomKind::BakeLayer)
            .and_then(|a| a.payload.get("trigger"))
            .and_then(|t| t.get("kind").and_then(|v| v.as_str()))
            .unwrap_or("manual");
        found = Some((
            bake_seq,
            atom_id,
            candidate.blob_hash,
            origin.to_owned(),
            trigger.to_owned(),
        ));
    }
    found
}

/// 评估用每层一行（开关／计数／软线已发／受阻；只留评估读的字段，多余字段会触 dead_code）。
struct LayerEvalRow {
    layer_id: String,
    unbaked: u64,
    enabled: bool,
    soft_fired: bool,
    blocked: bool,
}

/// 搭便车评估（registry.call 的钩子，见 tools.rs）。
///
/// 只在触发调用**成功提交**后跑：调用本身失败（Err）不评估；调用成功但未追加
/// 内容类原子（读／元数据／被排除的工具）也不评估。评估本身的 bake 失败**绝不**
/// 让触发调用失败——失败转成 auto_bake 节里的 failed 状态＋受阻态。
pub(crate) fn evaluate_auto_bake(
    ctx: &mut ToolContext<'_>,
    tool_name: &str,
    head_before: Option<u64>,
) -> Option<Value> {
    if NEVER_EVAL_TOOLS.contains(&tool_name) {
        return None;
    }
    let head_before = head_before?;
    // ---- 只读快照（一次借用内算完，放开后再做写操作）----
    let (head_after, appended_content, counts, layer_rows) = {
        let document = ctx.workspace.document(&ctx.doc_id)?;
        let head_after = document.head_seq();
        let atoms = document.log().atoms();
        let appended_content = atoms.iter().any(|atom| {
            atom.seq > head_before && atom.seq <= head_after && is_bake_content_kind(atom.kind)
        });
        let counts = unbaked_counts(document);
        // 每层一行：开关／来源／预授权／运行时态（get_bake_status 与评估共用口径）。
        let mut rows: Vec<LayerEvalRow> = Vec::new();
        for (layer_id, layer) in &document.state().layers {
            if layer.is_deleted() {
                continue;
            }
            let unbaked = counts.get(layer_id).copied().unwrap_or(0);
            let (enabled, _source) = effective_enabled(document.state(), layer_id);
            let runtime = document
                .auto_bake_runtime()
                .layers
                .get(layer_id)
                .cloned()
                .unwrap_or_default();
            rows.push(LayerEvalRow {
                layer_id: layer_id.clone(),
                unbaked,
                enabled,
                soft_fired: runtime.soft_fired,
                blocked: runtime.blocked.is_some(),
            });
        }
        (head_after, appended_content, counts, rows)
    };
    if head_after <= head_before || !appended_content {
        return None;
    }
    // ---- 同步运行时态（计数回落即清旗，不靠调用方记得清）----
    if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
        let runtime = document.auto_bake_runtime_mut();
        for row in &layer_rows {
            let entry = runtime.layers.entry(row.layer_id.clone()).or_default();
            if row.unbaked < AUTO_BAKE_SOFT_LINE {
                entry.soft_fired = false;
            }
            if row.unbaked < AUTO_BAKE_HARD_THRESHOLD {
                entry.blocked = None;
                entry.skip_reason = None;
            }
        }
    }
    // ---- 硬线候选：已启用、未受阻、计数达标；取最大计数，持平取层 id 最小 ----
    let mut hard: Vec<(u64, String)> = Vec::new();
    for row in &layer_rows {
        if row.enabled && !row.blocked && row.unbaked >= AUTO_BAKE_HARD_THRESHOLD {
            hard.push((row.unbaked, row.layer_id.clone()));
        }
    }
    hard.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if let Some((unbaked_before, layer_id)) = hard.first().cloned() {
        // 跳过去重时返回 None（无新信息就不挂节，调用方用 get_bake_status 查旧原因）；
        // 此时顺落到软线检查：另一层若正好首越软线，仍可报它（一次调用最多一节不变）。
        if let Some(section) = bake_selected_layer(ctx, &layer_id, unbaked_before, &counts) {
            return Some(section);
        }
    }
    // ---- 软线：已启用、未受阻、计数达标且本次越线未报过；同样一次最多一层 ----
    let mut soft: Vec<(u64, String)> = Vec::new();
    for row in &layer_rows {
        if row.enabled && !row.blocked && !row.soft_fired && row.unbaked >= AUTO_BAKE_SOFT_LINE {
            soft.push((row.unbaked, row.layer_id.clone()));
        }
    }
    soft.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    if let Some((unbaked, layer_id)) = soft.first().cloned() {
        if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
            document
                .auto_bake_runtime_mut()
                .layers
                .entry(layer_id.clone())
                .or_default()
                .soft_fired = true;
        }
        let mut section = base_section_read(&counts);
        section["status"] = json!("soft_line");
        section["scope"] = json!("layer");
        section["layer_id"] = json!(layer_id);
        section["unbaked_before"] = json!(unbaked);
        section["trigger"] = json!("soft_line");
        section["warnings"] = json!([{
            "level": "WARN",
            "code": "AUTO_BAKE_SOFT_LINE",
            "message": format!("图层 {layer_id} 未 bake 内容原子 {unbaked} 个，已过软线 {AUTO_BAKE_SOFT_LINE}（硬阈值 {AUTO_BAKE_HARD_THRESHOLD} 时自动 bake）"),
        }]);
        return Some(section);
    }
    None
}

/// auto_bake 回执节的公共读数字段（每次出现都带，调用方不必再查一遍状态）。
fn base_section_read(counts: &std::collections::BTreeMap<String, u64>) -> Value {
    let document_unbaked: u64 = counts.values().sum();
    json!({
        "threshold": AUTO_BAKE_HARD_THRESHOLD,
        "soft_line": AUTO_BAKE_SOFT_LINE,
        "global_enabled": auto_bake_global_enabled(),
        "document_unbaked": document_unbaked,
        "document_level": "v1 文档级开关不存在，文档级读全局值（设计 §10 暂缓）",
    })
}

/// 对选中层做一次自动 bake：门禁失败 → 跳过（原因不变不重复报，返回 None）；
/// 成功 → baked 节；渲染／写盘／提交失败 → failed 节＋受阻态（绝不抛给触发调用）。
fn bake_selected_layer(
    ctx: &mut ToolContext<'_>,
    layer_id: &str,
    unbaked_before: u64,
    counts: &std::collections::BTreeMap<String, u64>,
) -> Option<Value> {
    // 门禁普查（与手动 bake 同一份计数函数，同口径）。
    let (canvas_w, canvas_h, objects) = match ctx.workspace.document(&ctx.doc_id) {
        Some(document) => {
            let state = document.state();
            match state.layers.get(layer_id) {
                Some(layer) if !layer.is_deleted() => (
                    state.width,
                    state.height,
                    state
                        .objects_in_layer(layer_id)
                        .into_iter()
                        .cloned()
                        .collect::<Vec<_>>(),
                ),
                _ => {
                    let mut section = base_section_read(counts);
                    section["status"] = json!("skipped");
                    section["scope"] = json!("layer");
                    section["layer_id"] = json!(layer_id);
                    section["reason"] = json!("目标层不存在或已删除");
                    return Some(section);
                }
            }
        }
        None => {
            let mut section = base_section_read(counts);
            section["status"] = json!("skipped");
            section["scope"] = json!("layer");
            section["layer_id"] = json!(layer_id);
            section["reason"] = json!("文档未打开");
            return Some(section);
        }
    };
    let census = census_layer_objects(&objects, canvas_w, canvas_h);
    let (allow_rasterize, allow_clip, _allow_flatten) = match ctx.workspace.document(&ctx.doc_id) {
        Some(document) => layer_preauth(document.state(), layer_id),
        None => (false, false, false),
    };
    // 门禁失败 → 跳过＋告警，计数不重置（原因不变时不重复报）。
    let mut gate_reason: Option<String> = None;
    if (census.texts > 0 || census.vectors > 0) && !allow_rasterize {
        gate_reason = Some(format!(
            "含文本 {} 个、矢量 {} 个，未预授权自动栅格化（{}=true 才放行）",
            census.texts, census.vectors, META_ALLOW_AUTO_RASTERIZE
        ));
    } else if !census.clipped.is_empty() && !allow_clip {
        gate_reason = Some(format!(
            "有 {} 个图元越界，未预授权自动裁剪越界像素（{}=true 才放行）",
            census.clipped.len(),
            META_ALLOW_AUTO_CLIP
        ));
    }
    if let Some(reason) = gate_reason {
        let already_reported = ctx
            .workspace
            .document(&ctx.doc_id)
            .and_then(|d| d.auto_bake_runtime().layers.get(layer_id))
            .and_then(|r| r.skip_reason.clone())
            == Some(reason.clone());
        if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
            document
                .auto_bake_runtime_mut()
                .layers
                .entry(layer_id.to_owned())
                .or_default()
                .skip_reason = Some(reason.clone());
        }
        if already_reported {
            return None;
        }
        let mut section = base_section_read(counts);
        section["status"] = json!("skipped");
        section["scope"] = json!("layer");
        section["layer_id"] = json!(layer_id);
        section["unbaked_before"] = json!(unbaked_before);
        section["trigger"] = json!("layer_waterline");
        section["reason"] = json!(reason);
        section["warnings"] = json!([{
            "level": "WARN",
            "code": "AUTO_BAKE_SKIPPED_GATE",
            "message": format!("图层 {layer_id} 已过硬阈值但门禁未通过，本次跳过自动 bake（计数不重置，预授权后下次内容调用重试）"),
        }]);
        return Some(section);
    }
    // v1 flatten_mask 恒 false（D11 未修，见模块头注）；预授权值照常存、照常报，
    // 但不生效——报出来，不静默吞掉用户的设置。
    let trigger = json!({
        "kind": "layer_waterline",
        "unbaked_before": unbaked_before,
        "waterline": AUTO_BAKE_HARD_THRESHOLD,
    });
    match render_and_commit_bake(ctx, layer_id, false, Some("auto"), Some(trigger)) {
        Ok(performed) => {
            if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
                let entry = document
                    .auto_bake_runtime_mut()
                    .layers
                    .entry(layer_id.to_owned())
                    .or_default();
                entry.blocked = None;
                entry.skip_reason = None;
            }
            let mut warnings: Vec<Value> = Vec::new();
            if census.texts > 0 {
                warnings.push(json!({
                    "level": "LOSSY_EDITABILITY",
                    "code": "TEXT_RASTERIZED",
                    "message": format!("文本 {} 个已栅格化，不可再编辑文本内容", census.texts),
                    "count": census.texts,
                    "object_ids": census.text_ids,
                }));
            }
            if census.vectors > 0 {
                warnings.push(json!({
                    "level": "LOSSY_EDITABILITY",
                    "code": "VECTOR_RASTERIZED",
                    "message": format!("矢量 {} 个已栅格化，不可再编辑矢量节点", census.vectors),
                    "count": census.vectors,
                    "object_ids": census.vector_ids,
                }));
            }
            if !census.clipped.is_empty() {
                warnings.push(json!({
                    "level": "LOSSY_PIXELS",
                    "code": "CLIPPED_OFF_CANVAS_PIXELS",
                    "message": format!("越界图元 {} 个的画布外像素已被裁剪", census.clipped.len()),
                    "count": census.clipped.len(),
                    "clipped_regions": census.clipped,
                }));
            }
            let mut section = base_section_read(counts);
            section["status"] = json!("baked");
            section["scope"] = json!("layer");
            section["layer_id"] = json!(layer_id);
            section["unbaked_before"] = json!(unbaked_before);
            section["trigger"] = json!("layer_waterline");
            section["bake_atom_seq"] = json!(performed.bake_seq);
            section["bake_atom_id"] = json!(performed.bake_atom_id);
            section["blob_hash"] = json!(performed.blob_hash);
            section["blob_byte_len"] = json!(performed.blob_byte_len);
            section["dimensions"] = json!({"width": performed.width, "height": performed.height});
            section["superseded_atom_count"] = json!(performed.superseded_atom_count);
            section["retained_atom_count"] = json!(performed.retained_atom_count);
            section["flatten_mask"] = json!(false);
            section["flatten_mask_note"] = json!("v1 自动 bake 不压平掩膜（期 1 D11 双掩膜问题未修，修在 bake-document 分支）；allow_auto_flatten_mask 的设置已存但本版不生效");
            section["warnings"] = json!(warnings);
            Some(section)
        }
        Err(error) => {
            let reason = format!("{error}");
            if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
                document
                    .auto_bake_runtime_mut()
                    .layers
                    .entry(layer_id.to_owned())
                    .or_default()
                    .blocked = Some(reason.clone());
            }
            let mut section = base_section_read(counts);
            section["status"] = json!("failed");
            section["scope"] = json!("layer");
            section["layer_id"] = json!(layer_id);
            section["unbaked_before"] = json!(unbaked_before);
            section["trigger"] = json!("layer_waterline");
            section["reason"] = json!(reason);
            section["warnings"] = json!([{
                "level": "WARN",
                "code": "AUTO_BAKE_FAILED",
                "message": format!("图层 {layer_id} 自动 bake 失败，已进受阻态（不再自动重试；手动 bake／recover／改设置清除）"),
            }]);
            Some(section)
        }
    }
}

/// 手动 bake 成功后清除该层的受阻态（设计 §9 决定 10：手动 bake 证明该层可 bake）。
pub(crate) fn clear_auto_bake_blocked_for_layer(ctx: &mut ToolContext<'_>, layer_id: &str) {
    if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
        if let Some(entry) = document.auto_bake_runtime_mut().layers.get_mut(layer_id) {
            entry.blocked = None;
            entry.skip_reason = None;
        }
    }
}

/// recover 成功后清除全部存活层的受阻态（设计 §9 决定 10；清除后下次内容调用
/// 重新尝试，仍失败则重新受阻——清除本身不掩盖故障）。
pub(crate) fn clear_auto_bake_blocked_all(ctx: &mut ToolContext<'_>) {
    if let Ok(document) = ctx.workspace.document_mut(&ctx.doc_id) {
        let alive: Vec<String> = document
            .state()
            .layers
            .iter()
            .filter(|(_, layer)| !layer.is_deleted())
            .map(|(id, _)| id.clone())
            .collect();
        let runtime = document.auto_bake_runtime_mut();
        for layer_id in alive {
            if let Some(entry) = runtime.layers.get_mut(&layer_id) {
                entry.blocked = None;
                entry.skip_reason = None;
            }
        }
    }
}

/// update_layer 改了 auto_bake 系设置后清除该层的受阻态与跳过原因
/// （设置变了 ⇒ 门禁输入变了 ⇒ 下次内容调用重新评估，不过期的旧原因不该再拦）。
pub(crate) fn clear_auto_bake_state_for_settings_change(ctx: &mut ToolContext<'_>, layer_id: &str) {
    clear_auto_bake_blocked_for_layer(ctx, layer_id);
}

/// revert 拦截注脚（设计 §9 决定 12）：目标是被有效 Bake 剪枝的内容类原子时，
/// 在 revert 回执上附 Bake 原子 seq 与说明——只加说明，不改 revert 语义
/// （undo_last 不走这里：被剪枝的手势已计入 ignored，无需再注）。
pub(crate) fn bake_fold_note_for_revert(
    ctx: &mut ToolContext<'_>,
    target_atom_id: &str,
) -> Option<(u64, String)> {
    let document = ctx.workspace.document(&ctx.doc_id)?;
    let target = document.log().get(target_atom_id)?;
    if !is_bake_content_kind(target.kind) {
        return None;
    }
    let layer_id = bake_target_layer(target, |id| {
        document.state().objects.get(id).map(|o| o.layer_id.clone())
    })?;
    let atoms = document.log().atoms();
    let suppressed = compute_suppressed(atoms);
    let horizon = compute_bake_horizon(atoms, &suppressed);
    let water = horizon.get(&layer_id).copied().unwrap_or(0);
    if target.seq > water {
        return None;
    }
    let (bake_seq, _, _, _, _) = last_bake_for_layer(atoms, &layer_id)?;
    Some((
        bake_seq,
        format!(
            "该原子已被图层 {layer_id} 的 Bake（seq {bake_seq}）折叠剪枝：revert 它只影响水位之上的状态，基底像素本身不受影响；要整段撤掉请先 revert 该 Bake 原子"
        ),
    ))
}

/// get_bake_status 工具（只读，Core）：查自动 bake 的开关、水位与每层状态。
pub(super) fn read_get_bake_status(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_filter = optional_str(args, "layer_id");
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let counts = unbaked_counts(document);
    let horizon = compute_bake_horizon(
        document.log().atoms(),
        &compute_suppressed(document.log().atoms()),
    );
    let mut layers: Vec<Value> = Vec::new();
    for (layer_id, layer) in &document.state().layers {
        if layer.is_deleted() {
            continue;
        }
        if let Some(filter) = &layer_filter {
            if filter != layer_id {
                continue;
            }
        }
        let unbaked = counts.get(layer_id).copied().unwrap_or(0);
        let (enabled, source) = effective_enabled(document.state(), layer_id);
        let (allow_rasterize, allow_clip, allow_flatten) =
            layer_preauth(document.state(), layer_id);
        let runtime = document
            .auto_bake_runtime()
            .layers
            .get(layer_id)
            .cloned()
            .unwrap_or_default();
        let last_bake = last_bake_for_layer(document.log().atoms(), layer_id).map(
            |(bake_seq, atom_id, blob_hash, origin, trigger)| {
                json!({
                    "bake_atom_seq": bake_seq,
                    "bake_atom_id": atom_id,
                    "blob_hash": blob_hash,
                    "origin": origin,
                    "trigger": trigger,
                })
            },
        );
        layers.push(json!({
            "layer_id": layer_id,
            "layer_name": layer.name,
            "unbaked": unbaked,
            "waterline": horizon.get(layer_id).copied().unwrap_or(0),
            "soft_line": AUTO_BAKE_SOFT_LINE,
            "threshold": AUTO_BAKE_HARD_THRESHOLD,
            "enabled": enabled,
            "enabled_source": source,
            "allow_auto_rasterize_editable": allow_rasterize,
            "allow_auto_clip_off_canvas": allow_clip,
            "allow_auto_flatten_mask": allow_flatten,
            "flatten_mask_note": "v1 自动 bake 不压平掩膜（期 1 D11 未修）；该设置已存但本版不生效",
            "soft_fired": runtime.soft_fired,
            "blocked": runtime.blocked,
            "skipped_gate": runtime.skip_reason,
            "last_bake": last_bake,
        }));
    }
    if let Some(filter) = &layer_filter {
        if layers.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("图层 {filter} 不存在或已删除")),
            ));
        }
    }
    let document_unbaked: u64 = counts.values().sum();
    Ok(json!({
        "global_enabled": auto_bake_global_enabled(),
        "threshold": AUTO_BAKE_HARD_THRESHOLD,
        "soft_line": AUTO_BAKE_SOFT_LINE,
        "document_unbaked": document_unbaked,
        "document_level": "v1 文档级开关不存在，文档级读全局值（设计 §10 暂缓）",
        "layers": layers,
    }))
}
