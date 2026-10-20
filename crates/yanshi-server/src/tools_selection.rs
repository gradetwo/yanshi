//! **选区工具**（`create_selection` / `delete_selection` + 它们用的 `selection_area_px`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2**）：`tools.rs` 已超 30000 行。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **依赖**：`use super::*` 拿 `tools.rs` 的公共 helper（同 crate 子模块可访问私有项）。

use super::*;

/// 创建蒙版：内核按形状覆盖率调制图层 alpha（支持羽化与反选）。
/// 选区语义（设计 4.4）：用户确认采用**路线 A「约束落笔」** ✓ ——
/// 选区只约束**之后新落笔**的像素 ✓，不改写已有内容 ✓，删掉选区后已画内容**保持不变** ✓
/// （因为约束是在渲染期按日志重新计算的 ✓）。设计只给了数据模型与类型清单、
/// 未规定其对落笔的作用，这一语义是用户拍板的，已记入 implementation-notes ✓。
pub(super) fn write_create_selection(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let selection_id = require_str(args, "selection_id")?;
    // Q2：`from_mask` 一键转换（传 mask_id）：克隆该蒙版的 shape/feather/invert/blobs/patches。
    // 显式传的 shape/feather/invert 优先于蒙版的值。
    #[allow(clippy::type_complexity)]
    let from_mask_shape: Option<(Value, f64, bool, Vec<String>, Vec<Value>)> =
        if let Some(mask_id) = optional_str(args, "from_mask") {
            let state = document_state(ctx)?;
            let mask = state.masks.get(&mask_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("蒙版 {mask_id} 不存在")),
                )
            })?;
            let blobs: Vec<String> = mask.blobs.iter().map(|b| b.to_string()).collect();
            let patches: Vec<Value> = mask
                .patches
                .iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect();
            Some((
                mask.shape.clone(),
                mask.feather,
                mask.invert,
                blobs,
                patches,
            ))
        } else {
            None
        };
    let mut shape = if let Some((s, _, _, _, _)) = &from_mask_shape {
        // from_mask 时 shape 可选：没传就用蒙版的。
        if args.get("shape").and_then(Value::as_object).is_some() {
            require_object(args, "shape")?.clone()
        } else {
            s.clone()
        }
    } else {
        require_object(args, "shape")?.clone()
    };
    if shape.get("kind").and_then(Value::as_str).is_none()
        && shape.get("type").and_then(Value::as_str).is_none()
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "shape 必须带 kind（rect/ellipse/polygon），from_mask 时可省略".to_owned(),
            ),
        ));
    }
    let feather = args
        .get("feather")
        .and_then(Value::as_f64)
        .unwrap_or_else(|| {
            from_mask_shape
                .as_ref()
                .map(|(_, f, _, _, _)| *f)
                .unwrap_or(0.0)
        });
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
        "selection_id": selection_id,
        "shape": shape,
        "feather": feather,
        "mode": mode,
        "invert": args.get("invert").and_then(Value::as_bool).unwrap_or_else(|| {
            from_mask_shape
                .as_ref()
                .map(|(_, _, inv, _, _)| *inv)
                .unwrap_or(false)
        }),
    });
    if let Some(linked) = optional_str(args, "linked_layer") {
        payload["linked_layer"] = json!(linked);
    }
    // Q2：from_mask 时携带蒙版的 blobs 与 patches。
    if let Some((_, _, _, blobs, patches)) = &from_mask_shape {
        if !blobs.is_empty() {
            payload["refs"] = json!({ "blobs": blobs });
        }
        if !patches.is_empty() {
            payload["patches"] = Value::Array(patches.clone());
        }
    }
    // **`mode != "new"` 时合并尚未实现**（第 807 轮，外部 §136 反馈）：
    // 落盘是 `state.selections.insert(…)`（覆盖式），而 spec 承诺的是「组合」。
    // ⇒ 外部原话：「它现在会『静默不生效』（比报错更危险）」。
    // ⇒ 所以这里**明确拒绝**，而不是让他以为合并成功了。
    //   （真正的布尔合并见 docs/design/brush-capability-gap.md 三百二十七节的 4 步清单，
    //     第 1、4 步已完成，第 2、3 步待做。）
    // **`mode != "new"` 时做位图布尔合并**（第 825 轮，台账三百二十七节第 2、3 步）。
    // 三条 mode **同一条路线**（不许分叉）：光栅化旧形状 → 光栅化新形状 → 按 mode 合成 →
    // L8 PNG → 注册 → 把 payload 的 shape 换成 `{kind:"bitmap", bbox, blob}`。
    // ⇒ 为什么必须位图：`subtract` 的结果是「带洞的矩形」、`add` 是「两个分离矩形」，
    //   都不是矩形 ⇒ 解析式只能覆盖 `intersect` 的 1/3（见台账三百二十七节的补充）。
    if mode != "new" {
        let state = document_state(ctx)?;
        if let Some(existing) = state.selections.get(&selection_id) {
            if existing.is_deleted() {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!("选区 {selection_id} 已删除，无法 mode={mode}")),
                ));
            }
            let old_json = existing.shape.clone();
            let new_json = shape.clone();
            if let Some((bbox, blob_hash, png_len)) =
                merge_selection_shapes(ctx, &old_json, &new_json, &mode)?
            {
                shape = json!({"kind": "bitmap", "bbox": [bbox[0], bbox[1], bbox[2], bbox[3]],
                               "blob": blob_hash});
                payload["shape"] = shape.clone();
                payload["merged_from"] = json!(mode);
                payload["merged_bytes"] = json!(png_len);
            } else {
                // 两者都空 ⇒ 合成之后也是空。**明说**，不要留一个空位图让人误读。
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "mode={mode} 的合并结果是**空选区**（两个形状没有重叠或都是空的）\
                         ⇒ 没有可提交的选区。若你要的是清空，请用 delete_selection。"
                    )),
                ));
            }
        }
    }
    let result = ctx.commit(AtomKind::CreateSelection, payload)?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

pub(super) fn write_delete_selection(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let selection_id = require_str(args, "selection_id")?;
    let result = ctx.commit(AtomKind::Tombstone, json!({ "selection_id": selection_id }))?;
    let region = region_of(&result);
    finish_mutation(ctx, &result, region)
}

/// 选区面积（像素）与其是否**只能取近似**。
///
/// `selection.shape` 是落盘后的 JSON（`rect`／`polygon`／`bitmap`），不是 `SelectionShape`
/// ⇒ 这里按 `kind` 分支算：
/// - `rect`：`w * h`，**精确**
/// - `ellipse`：`π * (w/2) * (h/2)`，**精确**（相对解析式）
/// - 其余（`polygon`／`bitmap`）：**算不出来** ⇒ 返回 `(0.0, false)`
///   ⇒ 调用方必须看第二个返回值，不许把 0 当成「空选区」（**静默降级**那条规矩）。
pub(super) fn selection_area_px(ctx: &mut ToolContext<'_>, shape: &Value) -> (f64, bool) {
    let kind = shape.get("kind").and_then(Value::as_str).unwrap_or("");
    let num = |key: &str| shape.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    match kind {
        "rect" => (num("w") * num("h"), true),
        "ellipse" => (
            std::f64::consts::PI * (num("w") / 2.0) * (num("h") / 2.0),
            true,
        ),
        "bitmap" => {
            // 位图选区（布尔合并 / smart_select 产出）：解 L8 blob，数非零像素。
            let Some(text) = shape.get("blob").and_then(Value::as_str) else {
                return (0.0, false);
            };
            let Ok(hash) = text.parse::<yanshi_core::BlobHash>() else {
                return (0.0, false);
            };
            let Ok(bytes) = ctx.workspace.store().get(&hash) else {
                return (0.0, false);
            };
            match yanshi_render::png::decode_png_l8(&bytes) {
                Some((_, _, data)) => (data.iter().filter(|&&v| v > 0).count() as f64, true),
                None => (0.0, false),
            }
        }
        _ => (0.0, false),
    }
}
