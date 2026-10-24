//! **选区工具**（`create_selection` / `delete_selection` + 它们用的 `selection_area_px`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2**）：`tools.rs` 已超 30000 行。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **依赖**：`use super::*` 拿 `tools.rs` 的公共 helper（同 crate 子模块可访问私有项）。

use super::*;

/// 把 `rect`／`ellipse`／`circle` 选区形状里的**数组写法** `bbox: [x, y, w, h]`
/// 归一成**平铺写法** `x, y, w, h` ✓（并摘掉 `bbox`）。
///
/// **为什么是平铺、不是对象**：同一个 shape 会被**四处**读，四处的口径还各不相同 ✓
/// * 渲染层 `selection::parse_bbox` ⇒ `bbox` 对象 **或** 平铺 ✓
/// * 服务端 `tools::selection_shape_bbox` ⇒ `bbox` 对象 **或** 平铺 ✓
/// * 裁剪入口 `tools_brush.rs` 的 `alive` 包围盒 ⇒ `bbox` 对象 **或** 平铺 ✓
/// * 面积上报 `selection_area_px` ⇒ **只读平铺 `w`/`h`** ✗
///
/// ⇒ 只有**平铺**这一种能同时满足四处 ✓（对象写法会让 `area` 恒为 0，
///   而数组写法会让前三处全部读不到 ⇒ 见调用点的说明）。
///
/// 只碰这三种形状：`bitmap` 的 `bbox` **本来就该是数组**（`selection_shape_bbox` 的
/// `"bitmap"` 分支按 `arr.get(i)` 读 ✓）⇒ 归一它会改坏语义 ✓ 所以必须跳过。
fn normalize_bbox_shape(shape: &mut Value) -> Result<()> {
    let kind = shape
        .get("kind")
        .or_else(|| shape.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if !matches!(kind, "rect" | "ellipse" | "circle") {
        return Ok(());
    }
    let Some(array) = shape.get("bbox").and_then(Value::as_array).cloned() else {
        return Ok(());
    };
    let reject = |why: String| {
        Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "shape.bbox 数组写法必须是 [x, y, w, h] 四个有限数且 w、h > 0（{why}）；\
                 或者用平铺写法 {{kind, x, y, w, h}} ✓"
            )),
        ))
    };
    if array.len() != 4 {
        return reject(format!("得到 {} 个数", array.len()));
    }
    let mut n = [0.0_f64; 4];
    for (i, value) in array.iter().enumerate() {
        let Some(value) = value.as_f64() else {
            return reject(format!("第 {} 个不是数字：{value}", i + 1));
        };
        n[i] = value;
    }
    if !n.iter().all(|v| v.is_finite()) {
        return reject("含 NaN/Inf".to_owned());
    }
    if n[2] <= 0.0 || n[3] <= 0.0 {
        return reject(format!("w={}、h={}", n[2], n[3]));
    }
    let Some(object) = shape.as_object_mut() else {
        return Ok(());
    };
    object.remove("bbox");
    object.insert("x".to_owned(), json!(n[0]));
    object.insert("y".to_owned(), json!(n[1]));
    object.insert("w".to_owned(), json!(n[2]));
    object.insert("h".to_owned(), json!(n[3]));
    Ok(())
}

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
    // **未知形状当场拒绝** ✓（第 778 轮；外部报告「创建成功 ⇒ 之后找不到」）
    //   **⇒ 现象**：`create_selection(shape.kind=spiral)` 回 `ok=true` ✗ ⇒ 而**没有任何工具认领它**
    //     （`clip_to_selection` 找不到 bbox ⇒ 报「一支都没有」✓）⇒ 调用方**以为建好了** ✓。
    //   **⇒ 为什么不是"把 rect/ellipse 也报错"** ✗：它们**真的能用** ✓
    //     （`tools_brush.rs:417-440` 已为 rect 的 `w`/`h` 与 ellipse 的 `cx/cy/rx/ry` 算 bbox ✓，
    //     第 901／911 轮 ✓）⇒ 关掉它们会砍掉一个能用的功能 ✗。
    //   **⇒ 所以只拒绝"真的没人认领"的形状** ✓ —— 已知集合：`rect`／`ellipse`／`polygon`／`bitmap` ✓。
    if from_mask_shape.is_none() {
        let kind = shape
            .get("kind")
            .or_else(|| shape.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("rect");
        // **已知集合必须与渲染层的解析分支逐一对齐** ✓（第 790 轮；CI 抓到的）
        //   **⇒ 缺陷（我第 778 轮引入的）**：我只列了 `rect`／`ellipse`／`polygon`／`bitmap` ✗
        //     ⇒ 把 **`path`**（按 `object_id` 引用的曲线路径 ✓）和 **`lasso`**、
        //     **`circle`**（`selection.rs:215` 会把 `circle` 归一成 `ellipse` ✓）一起拒了 ✓
        //     ⇒ `path_data_a1` 的 `selection_by_object_id_matches_inline_and_errors_explicitly` 红 ✓。
        //   **⇒ 教训**：**"未知形状"的名单只能从渲染层的分支抄** ✓（`selection.rs:215-266` ✓），
        //     不许凭印象写 ✓ —— 否则"防静默"的检查自己造新的静默失败 ✓。
        const KNOWN: [&str; 6] = ["rect", "ellipse", "circle", "polygon", "lasso", "path"];
        if !KNOWN.contains(&kind) && kind != "bitmap" {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "未知的选区形状 {kind:?} ⇒ 可用：rect／ellipse／polygon／bitmap \
                     （**建一个没人认领的选区比报错更坏**：之后 `clip_to_selection` 只会说\
                     「一支都没有」，看起来像调用方传错了 id）"
                )),
            ));
        }
    }

    // **★ `bitmap` 选区必须补 `bbox` ★**（第 1609 轮；外部 `E1742` 的**第三处**）
    //   **⇒ 现象**：`create_mask(shape.kind=bitmap)` ✓、`from_mask` ✓、`list_selections` ✓
    //     **而裁剪 `clip_coverage` 恒 0**（**`ok=true` 但一个像素不落**）✗
    //   **⇒ ⇒ 根因**：渲染层 `SelectionShape::parse_geom` 的 `bitmap` 分支**要求 `shape.bbox`**
    //     （**没 bbox ⇒ `SelectionGeom::Empty` ⇒ coverage 0**）✗
    //     而**掩膜的 shape 只有 `{kind, blob_hash}`** ⇒ **`from_mask` 克隆后也没有 bbox** ✗
    //   **⇒ ⇒ ⇒ 所以**：**位图选区在这里补 `bbox`** —— 优先用掩膜给的尺寸，
    //     没有就用**整幅**（**位图掩膜按约定覆盖全画布**）✓
    let bitmap_kind = shape
        .get("kind")
        .or_else(|| shape.get("type"))
        .and_then(Value::as_str)
        == Some("bitmap");
    if bitmap_kind && shape.get("bbox").is_none() {
        let num = |k: &str| shape.get(k).and_then(Value::as_f64);
        let (w, h) = match (num("width"), num("height")) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => (w, h),
            _ => {
                let document = ctx.workspace.document(&ctx.doc_id);
                match document {
                    Some(d) => (d.state().width as f64, d.state().height as f64),
                    None => (0.0, 0.0),
                }
            }
        };
        if w > 0.0 && h > 0.0 {
            if let Some(obj) = shape.as_object_mut() {
                obj.insert(
                    "bbox".to_owned(),
                    json!([0, 0, w.round() as i64, h.round() as i64]),
                );
            }
        }
    }
    if shape.get("kind").and_then(Value::as_str).is_none()
        && shape.get("type").and_then(Value::as_str).is_none()
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "shape 必须带 kind（rect/ellipse/polygon/path），from_mask 时可省略".to_owned(),
            ),
        ));
    }
    // **`bbox` 的两种写法必须在入口归一**（实测缺陷 ✓）：工具面别处到处是**数组**包围盒
    // （`export_png`/`render_region` 的 `region: [0,0,64,64]` ✓、位图选区的 `shape.bbox` ✓）
    // ⇒ 调用者把它写进 rect 选区是**必然会发生的**事。
    // 而 `tools_brush.rs` 的裁剪入口先 `shape.get("bbox")` **拿到数组就当对象用** ✓
    // ⇒ `bbox.get("x")` 在数组上恒 `None` ⇒ `unwrap_or(0.0)` ⇒
    // **选区包围盒变成贴在原点的 0×0** ✓ ⇒ 与笔迹矩形求交为空 ⇒ 覆盖度全 0 ⇒
    // **整笔印章被跳过** ✗。表现是：`create_selection` 回 `ok: true` ✓、
    // `list_selections` 里 `area: 0` ✓、之后每一笔带 `clip_to_selection` 的墨**全被吃掉** ✗
    // —— 与 `erase{region}` 同一类：**契约承诺了形状，读取方只认另一种写法** ✓。
    // 修在**入口**而不是四处读取方 ⇒ 落库形态与旧数据一致 ✓、回放逐字节不变 ✓；
    // 数组长度/数值非法时**明确拒绝**，不再静默存一个永远框不住东西的空选区 ✗。
    normalize_bbox_shape(&mut shape)?;
    // F2：kind=path 的选区形状在入口处理 `d` 语法糖（与 `create_mask` 同
    // 口径，只用既有 `parse_svg_d` 一份解析器）：d 转 subpaths 并摘掉
    // （落库只存结构化形态），再走渲染层同一份校验
    // （`object::parse_path_subpaths`）——此前带 d 的选区形状落库后在
    // 渲染层被判非法、静默退化成空选（v8 实测只能先建蒙版中转）。
    // 带 `object_id` 的是引用形态（校验归引用解析侧），不在这里验。
    let path_kind = shape
        .get("kind")
        .or_else(|| shape.get("type"))
        .and_then(Value::as_str)
        == Some("path");
    // A1：`shape:{kind:"path", object_id}` 引用已有 path 对象建选区——与
    // `create_mask` 的 object_id **同一口径**：二选一（与 subpaths / d 同给
    // 报错）、引用先验存在性 / 存活 / 是 path 形状（复用蒙版的同一份
    // `validate_mask_path_shape`，不许写第二套）、坏 id 明确报错不静默回退。
    // 与蒙版的差别只在落库形态：蒙版在渲染期按引用现解（`mask_path_coverage`
    // 拿得到文档状态），而选区覆盖率走无状态的 `SelectionShape::from_value`
    // ⇒ 引用在这里**展开成内联 subpaths 快照**落库（另记 `resolved_from`
    // 留痕来源），渲染口径因此与内联写法逐像素同源；被引用对象之后再改，
    // 本选区不跟随（要跟随请用 create_mask 的引用形态）。
    if path_kind && shape.get("object_id").is_some() {
        if shape.get("subpaths").is_some() || shape.get("d").is_some() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "shape 的 object_id 与 subpaths / d 只能给一个（二选一）".to_owned(),
                ),
            ));
        }
        validate_mask_path_shape(ctx, &shape)?;
        let object_id = shape
            .get("object_id")
            .and_then(Value::as_str)
            .expect("上面已确认 object_id 存在")
            .to_owned();
        let state = document_state(ctx)?;
        let object = state
            .objects
            .get(&object_id)
            .expect("validate_mask_path_shape 已验存在性");
        let yanshi_render::object::Primitive::Shape {
            subpaths,
            fill_rule,
            ..
        } = yanshi_render::object::parse_object(object)
        else {
            unreachable!("validate_mask_path_shape 已验是 path 形状")
        };
        shape = json!({
            "kind": "path",
            "fill_rule": fill_rule.as_str(),
            "subpaths": yanshi_render::object::subpaths_to_json(&subpaths),
            "resolved_from": object_id,
        });
    }
    if path_kind && shape.get("object_id").is_none() {
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
        yanshi_render::object::parse_path_subpaths(&shape).map_err(|reason| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("shape 的 path 非法：{reason}")),
            )
        })?;
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
    // **`bbox` 对象写法与平铺写法都要读** ✓（缺陷 ✓）：原来只读平铺 `w`/`h`
    // ⇒ 而渲染层 `selection::parse_bbox` **两种都认** ✓ ⇒
    // 于是 `{kind:"rect", bbox:{x,y,w,h}}` 这种**完全合法、裁剪也确实生效** ✓ 的选区
    // 在 `list_selections` 里报 `area: 0` ✗ ⇒ 调用者据此判断"选区是空的"并被误导 ✗
    // （我就是这样一路追到 `clip_to_selection` 的 ✓）。
    // 报 0 比不报更糟：它长得像一个**诊断结论** ✓。
    let source = shape.get("bbox").filter(|v| v.is_object()).unwrap_or(shape);
    let num = |key: &str| source.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    match kind {
        "rect" => (num("w") * num("h"), true),
        "ellipse" => (
            std::f64::consts::PI * (num("w") / 2.0) * (num("h") / 2.0),
            true,
        ),
        "bitmap" => {
            // 位图选区（布尔合并 / smart_select 产出）：解 L8 blob，数非零像素。
            let Some(text) = shape
                .get("blob")
                .or_else(|| shape.get("blob_hash"))
                .and_then(Value::as_str)
            else {
                return (0.0, false);
            };
            let Ok(hash) = text.parse::<yanshi_core::BlobHash>() else {
                return (0.0, false);
            };
            let Ok(bytes) = ctx.workspace.store().get(&hash) else {
                return (0.0, false);
            };
            let dims = (
                shape.get("width").and_then(Value::as_u64).map(|v| v as u32),
                shape
                    .get("height")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32),
            );
            let decoded = yanshi_render::png::decode_png_l8(&bytes)
                .or_else(|| {
                    yanshi_render::png::decode_png(&bytes).map(|(w, h, rgba)| {
                        let data = rgba.chunks(4).map(|px| px[3]).collect();
                        (w, h, data)
                    })
                })
                .or_else(|| {
                    let (w, h) = (dims.0?, dims.1?);
                    let want_rgba = (w as usize) * (h as usize) * 4;
                    let want_l8 = (w as usize) * (h as usize);
                    if bytes.len() == want_rgba {
                        Some((w, h, bytes.chunks(4).map(|px| px[3]).collect()))
                    } else if bytes.len() == want_l8 {
                        Some((w, h, bytes.clone()))
                    } else {
                        None
                    }
                });
            match decoded {
                Some((_, _, data)) => (data.iter().filter(|&&v| v > 0).count() as f64, true),
                None => (0.0, false),
            }
        }
        // **多边形选区的面积** ✓（`bug_report.md` BUG-06 那一族：**报 0 比不报更糟** ✗ ——
        //   它长得像"这个选区是空的" ✓）。用**鞋带公式** ✓（顶点按顺序 ✓，绝对值的一半 ✓）。
        "polygon" | "lasso" => {
            let Some(points) = shape.get("points").and_then(Value::as_array) else {
                return (0.0, false);
            };
            let mut area = 0.0_f64;
            let mut count = 0usize;
            let mut previous: Option<(f64, f64)> = None;
            let mut first: Option<(f64, f64)> = None;
            for point in points {
                let Some(pair) = point.as_array() else {
                    return (0.0, false);
                };
                let (Some(x), Some(y)) = (
                    pair.first().and_then(Value::as_f64),
                    pair.get(1).and_then(Value::as_f64),
                ) else {
                    return (0.0, false);
                };
                if let Some((px, py)) = previous {
                    area += px * y - x * py;
                } else {
                    first = Some((x, y));
                }
                previous = Some((x, y));
                count += 1;
            }
            if count < 3 {
                return (0.0, false);
            }
            if let (Some((fx, fy)), Some((lx, ly))) = (first, previous) {
                area += lx * fy - fx * ly;
            }
            (area.abs() / 2.0, true)
        }
        _ => (0.0, false),
    }
}

// ===== 同族追加（第 1272 轮）：transform_selection + select_opaque_pixels =====

/// 对 `[x, y]` 形式的 JSON 坐标点原地施加仿射（非该形态的值不动）。
fn affine_json_point(value: &mut Value, apply: &dyn Fn(f64, f64) -> (f64, f64)) {
    if let Some(arr) = value.as_array_mut() {
        if arr.len() == 2 {
            if let (Some(x), Some(y)) = (arr[0].as_f64(), arr[1].as_f64()) {
                let (nx, ny) = apply(x, y);
                arr[0] = json!(nx);
                arr[1] = json!(ny);
            }
        }
    }
}

/// 选区变换（`transform_selection`）：Q1 双轨制。
///
/// 纯几何选区（`patches` 为空且 `kind != "bitmap"`）在 JSON 几何层原地精确变换；
/// 位图选区（`kind == "bitmap"` 或 `patches` 非空）做逆向双线性重采样。
/// `feather` 乘面积缩放因子 `sqrt(|sx*sy|)`，`invert`/`mode`/`id` 不动。
/// 通过 `AtomKind::UpdateSelection` 提交。
pub(super) fn write_transform_selection(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let selection_id_in = require_str(args, "selection_id")?;
    let state = document_state(ctx)?;
    // 解析 selection_id（含 "latest"：按 created_by 取最新；必须用 map 的 key）。
    let selection_id = if selection_id_in == "latest" {
        let mut ordered: Vec<(&String, &yanshi_core::Selection)> = state
            .selections
            .iter()
            .filter(|(_, s)| !s.is_deleted())
            .collect();
        ordered.sort_by(|a, b| a.1.created_by.as_str().cmp(b.1.created_by.as_str()));
        ordered.last().map(|(k, _)| (*k).clone()).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail("没有可变换的选区".to_owned()),
            )
        })?
    } else {
        selection_id_in.to_owned()
    };
    let sel = state.selections.get(&selection_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("选区 {selection_id} 不存在")),
        )
    })?;
    if sel.is_deleted() {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("选区 {selection_id} 已删除")),
        ));
    }

    // ---- 参数 ----
    let (dx, dy) = args
        .get("translate")
        .and_then(Value::as_array)
        .map(|a| {
            (
                a.first().and_then(Value::as_f64).unwrap_or(0.0),
                a.get(1).and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .unwrap_or((0.0, 0.0));
    let (sx, sy) = args
        .get("scale")
        .and_then(Value::as_array)
        .map(|a| {
            (
                a.first().and_then(Value::as_f64).unwrap_or(1.0),
                a.get(1).and_then(Value::as_f64).unwrap_or(1.0),
            )
        })
        .unwrap_or((1.0, 1.0));
    if !sx.is_finite() || !sy.is_finite() || sx == 0.0 || sy == 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("scale 必须是非零有限值".to_owned()),
        ));
    }
    let rotate_deg = args
        .get("rotate_deg")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if !rotate_deg.is_finite() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("rotate_deg 必须是有限值".to_owned()),
        ));
    }
    let theta = rotate_deg.to_radians();

    let shape = sel.shape.clone();
    let kind = shape
        .get("kind")
        .or_else(|| shape.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("rect");
    let bbox = selection_shape_bbox(&shape).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "选区 {selection_id} 的形状无法解析包围盒（kind={kind}）"
            )),
        )
    })?;
    // pivot 缺省为包围盒中心。
    let (px, py) = args
        .get("pivot")
        .and_then(Value::as_array)
        .map(|a| {
            (
                a.first()
                    .and_then(Value::as_f64)
                    .unwrap_or(bbox.0 + bbox.2 / 2.0),
                a.get(1)
                    .and_then(Value::as_f64)
                    .unwrap_or(bbox.1 + bbox.3 / 2.0),
            )
        })
        .unwrap_or((bbox.0 + bbox.2 / 2.0, bbox.1 + bbox.3 / 2.0));

    let m = selection_affine(dx, dy, sx, sy, theta, px, py);
    let apply =
        |x: f64, y: f64| -> (f64, f64) { (m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]) };
    // feather 按面积缩放因子调整（专家方案）；invert 不动。
    let new_feather = sel.feather * (sx.abs() * sy.abs()).sqrt();

    let is_bitmap = kind == "bitmap" || !sel.patches.is_empty();
    let new_shape: Value;
    let note: String;
    if is_bitmap {
        // ---- 位图轨：逆向双线性重采样 ----
        // 源位图：kind=="bitmap" 时读 shape.blob；几何+patches 时先用 CompiledMask
        // 把「几何底座 + patch 链」烘焙成位图（否则 patches 会丢失）。
        let (src_data, sw, sh, src_ox, src_oy): (Vec<u8>, usize, usize, f64, f64) = if kind
            == "bitmap"
        {
            let blob = shape.get("blob").and_then(Value::as_str).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("选区 {selection_id} 是位图选区但缺少 blob")),
                )
            })?;
            let src_mask = yanshi_render::selection::get_bitmap_mask(blob).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("选区 {selection_id} 的位图 blob 未注册：{blob}")),
                )
            })?;
            let (bx0, _, _, _) = bbox;
            (
                src_mask.data.clone(),
                src_mask.width as usize,
                src_mask.height as usize,
                bx0,
                bbox.1,
            )
        } else {
            // 几何底座 + patches：烘焙 bbox 区域。
            let load_blob = |s: &str| {
                s.parse::<yanshi_core::BlobHash>()
                    .ok()
                    .and_then(|hash| ctx.workspace.store().get(&hash).ok())
            };
            let compiled = yanshi_render::selection::CompiledMask::build(
                sel,
                state.width as i32,
                state.height as i32,
                &load_blob,
            );
            let (bx0, by0, bw0, bh0) = bbox;
            let (ix0, iy0) = (bx0.floor() as i32, by0.floor() as i32);
            let (iw, ih) = (bw0.ceil() as usize, bh0.ceil() as usize);
            let mut data = vec![0u8; iw * ih];
            for y in 0..ih {
                for x in 0..iw {
                    data[y * iw + x] = (compiled.sample(ix0 + x as i32, iy0 + y as i32) * 255.0)
                        .round()
                        .clamp(0.0, 255.0) as u8;
                }
            }
            (data, iw, ih, ix0 as f64, iy0 as f64)
        };
        // 变换后包围盒：4 角点变换后取 min/max。
        let (bx0, by0, bw0, bh0) = bbox;
        let corners = [
            apply(bx0, by0),
            apply(bx0 + bw0, by0),
            apply(bx0 + bw0, by0 + bh0),
            apply(bx0, by0 + bh0),
        ];
        let (mut nx0, mut ny0, mut nx1, mut ny1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (cx, cy) in corners {
            nx0 = nx0.min(cx);
            ny0 = ny0.min(cy);
            nx1 = nx1.max(cx);
            ny1 = ny1.max(cy);
        }
        let (ix0, iy0) = (nx0.floor() as i32, ny0.floor() as i32);
        let (iw, ih) = ((nx1 - nx0).ceil() as i32, (ny1 - ny0).ceil() as i32);
        if iw <= 0 || ih <= 0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("变换后选区面积为 0".to_owned()),
            ));
        }
        let inv = invert_affine(&m).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("变换矩阵退化（行列式为 0），无法求逆".to_owned()),
            )
        })?;
        // 源位图是 bbox 原点为 (src_ox, src_oy) 的裁剪块；源坐标 = 逆矩阵(目标画布坐标) - 原点。
        let mut out = vec![0u8; (iw as usize) * (ih as usize)];
        for y in 0..ih as usize {
            for x in 0..iw as usize {
                let tx = ix0 as f64 + x as f64;
                let ty = iy0 as f64 + y as f64;
                let sx_f = inv[0] * tx + inv[1] * ty + inv[2] - src_ox;
                let sy_f = inv[3] * tx + inv[4] * ty + inv[5] - src_oy;
                out[y * iw as usize + x] =
                    bilinear_sample_u8_zero_fill(&src_data, sw, sh, sx_f, sy_f);
            }
        }
        let png = yanshi_render::png::encode_png_l8(iw as u32, ih as u32, &out)
            .expect("内存 PNG 编码不会失败");
        let blob_hash = ctx.workspace.store().put(&png)?;
        let blob_str = blob_hash.to_string();
        yanshi_render::selection::register_bitmap_mask(
            blob_str.clone(),
            Arc::new(yanshi_render::selection::DecodedBitmapMask {
                width: iw as u32,
                height: ih as u32,
                data: out,
            }),
        );
        let mut s = shape.clone();
        if let Some(obj) = s.as_object_mut() {
            obj.insert("kind".to_owned(), json!("bitmap"));
            obj.insert("bbox".to_owned(), json!([ix0, iy0, iw, ih]));
            obj.insert("blob".to_owned(), json!(blob_str.clone()));
            // 注意：patches 非空的几何选区转位图后，patches 语义已烘焙进新位图，
            // 故不再保留旧 patches（由 UpdateSelection 的 shape 替换体现）。
        }
        new_shape = s;
        note = format!("位图逆向双线性重采样：{sw}x{sh} → {iw}x{ih}");
        // patches 已烘焙：通过 shape 替换体现，见下。
    } else if kind == "path" {
        // ---- 几何轨：path → 全部坐标点施同一仿射 ----
        // Bézier 的仿射不变性：变换 start 与每段的 c1/c2/to 即精确变换整条曲线，
        // 所以平移 / 缩放 / 旋转同一条路（不许分叉，分叉必然漂移）；
        // `kind` 与 `fill_rule` 保持，零精度损失（不像旋转轨要转 polygon）。
        let mut s = shape.clone();
        if let Some(subs) = s.get_mut("subpaths").and_then(Value::as_array_mut) {
            for sub in subs.iter_mut() {
                if let Some(start) = sub.get_mut("start") {
                    affine_json_point(start, &apply);
                }
                if let Some(segs) = sub.get_mut("segments").and_then(Value::as_array_mut) {
                    for seg in segs.iter_mut() {
                        for key in ["c1", "c2", "to"] {
                            if let Some(point) = seg.get_mut(key) {
                                affine_json_point(point, &apply);
                            }
                        }
                    }
                }
            }
        }
        new_shape = s;
        note = "几何精确变换（path 全部坐标点仿射，kind/fill_rule 保持）".to_owned();
    } else if theta.abs() < 1e-9 {
        // ---- 几何轨：无旋转 → 直接改 bbox（零精度损失）----
        let (x, y, w, h) = bbox;
        let (nx, ny) = apply(x, y);
        // 负缩放时规范化（宽高取绝对值，原点取最小角）。
        let (nx2, ny2) = apply(x + w, y + h);
        let (fx0, fx1) = (nx.min(nx2), nx.max(nx2));
        let (fy0, fy1) = (ny.min(ny2), ny.max(ny2));
        let mut s = shape.clone();
        if let Some(obj) = s.as_object_mut() {
            // 统一写成规范 x/y/w/h（解析器兼容旧式 width/height）。
            obj.insert("x".to_owned(), json!(fx0));
            obj.insert("y".to_owned(), json!(fy0));
            obj.insert("w".to_owned(), json!(fx1 - fx0));
            obj.insert("h".to_owned(), json!(fy1 - fy0));
            obj.remove("bbox");
            obj.remove("width");
            obj.remove("height");
            obj.remove("cx");
            obj.remove("cy");
            obj.remove("rx");
            obj.remove("ry");
        }
        new_shape = s;
        note = "几何精确变换（bbox 原地更新）".to_owned();
    } else {
        // ---- 几何轨：含旋转 → 顶点左乘矩阵 ----
        // rect → 4 顶点 polygon；ellipse → 64 顶点 polygon；polygon 直接变换顶点。
        let pts = selection_shape_points(&shape, bbox).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "选区 {selection_id} 的形状不支持旋转变换（kind={kind}）"
                )),
            )
        })?;
        let new_pts: Vec<Value> = pts
            .iter()
            .map(|(x, y)| {
                let (nx, ny) = apply(*x, *y);
                json!([nx, ny])
            })
            .collect();
        let mut s = shape.clone();
        if let Some(obj) = s.as_object_mut() {
            obj.insert("kind".to_owned(), json!("polygon"));
            obj.remove("type");
            obj.insert("points".to_owned(), Value::Array(new_pts));
            obj.remove("x");
            obj.remove("y");
            obj.remove("w");
            obj.remove("h");
            obj.remove("width");
            obj.remove("height");
            obj.remove("bbox");
            obj.remove("cx");
            obj.remove("cy");
            obj.remove("rx");
            obj.remove("ry");
        }
        new_shape = s;
        note = format!(
            "旋转变换：{kind} → polygon（{} 顶点，零精度损失）",
            pts.len()
        );
    }

    // 通过 UpdateSelection 提交（不改 id/mode/patches；位图轨的 patches 烘焙已在 shape 里体现，
    // 但 patches 字段本身保留——其求值由 clip 路径的 CompiledMask 钩子处理，见 Q2）。
    let payload = json!({
        "selection_id": selection_id,
        "shape": new_shape,
        "feather": new_feather,
    });
    let result = ctx.commit(AtomKind::UpdateSelection, payload)?;
    Ok(json!({
        "ok": true,
        "selection_id": selection_id,
        "kind": if is_bitmap { "bitmap" } else { kind },
        "feather": new_feather,
        "note": note,
        "atom_seq": result.seq,
    }))
}

/// 按图层不透明像素建选区（`select_opaque_pixels`）：Q1。
///
/// Photoshop「Ctrl+点击图层缩略图」语义：`coverage(x,y) = alpha(x,y)` 直接映射，
/// 不做硬二值化（无 1px 白边/黑边）。可选 `min_alpha` 过滤飞白、`binarize` 硬二值化
///（不推荐）、`close_pinholes` 闭合 ≤8px² 微孔（复用 `smart_select` 的 `pinhole_close`）。
/// 落盘复用 `commit_bitmap_selection`。
pub(super) fn write_select_opaque_pixels(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let selection_id = optional_str(args, "selection_id").unwrap_or_else(|| {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("sel_opaque_{n}")
    });
    let min_alpha = args.get("min_alpha").and_then(Value::as_f64).unwrap_or(0.0);
    if !(0.0..=1.0).contains(&min_alpha) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("min_alpha 必须在 [0, 1] 内，得到 {min_alpha}")),
        ));
    }
    let binarize = args
        .get("binarize")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let close_pinholes = args
        .get("close_pinholes")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mode = optional_str(args, "mode").unwrap_or_else(|| "new".to_owned());
    let feather = args.get("feather").and_then(Value::as_f64).unwrap_or(0.0);

    // 读整图层像素（与 smart_select 同一入口，严格用返回的 (rw, rh) 算 stride）。
    let state = document_state(ctx)?;
    let canvas_w = state.width as usize;
    let canvas_h = state.height as usize;
    let region = yanshi_core::Bbox::new(0.0, 0.0, canvas_w as f64, canvas_h as f64);
    let (rw, rh, raw) = ctx
        .workspace
        .render_region_raw_layer(&ctx.doc_id, region, &layer_id)
        .map_err(|e| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读取图层 {layer_id} 像素失败：{e}")),
            )
        })?;
    let (rw, rh) = (rw as usize, rh as usize);
    if raw.len() < rw * rh * 4 {
        return Err(YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!(
                "render_region_raw_layer 返回字节不足：{rw}x{rh} 需要 {}，实际 {}",
                rw * rh * 4,
                raw.len()
            )),
        ));
    }

    // coverage = alpha 直接映射（关键：不做硬二值化）。
    let min_a_u8 = (min_alpha * 255.0).round() as u8;
    let mut mask = vec![0u8; rw * rh];
    let mut any = false;
    for i in 0..rw * rh {
        let a = raw[i * 4 + 3];
        let v = if a < min_a_u8 {
            0
        } else if binarize {
            if a > 0 {
                255
            } else {
                0
            }
        } else {
            a
        };
        mask[i] = v;
        if v > 0 {
            any = true;
        }
    }
    if close_pinholes {
        pinhole_close(&mut mask, rw, rh);
    }

    let (bx, by, bw, bh, _blob_str, png_len, atom_seq) = commit_bitmap_selection(
        ctx,
        &selection_id,
        &mut mask,
        rw,
        rh,
        feather,
        &mode,
        false,
        false,
    )?;

    let mut selected_area = 0f64;
    for &v in &mask {
        selected_area += f64::from(v) / 255.0;
    }
    Ok(json!({
        "ok": true,
        "selection_id": selection_id,
        "bbox": [bx, by, bw, bh],
        "selected_area_px": selected_area,
        "any": any,
        "blob_bytes": png_len,
        "note": if binarize { "已硬二值化（不推荐：边缘有 1px 误差）" } else { "alpha 直接映射（无二值化）" },
        "atom_seq": atom_seq,
    }))
}
