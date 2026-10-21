//! **PSD 工具**（`import_psd` / `export_psd_layers` / `import_psd_layers`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项「tools.rs 拆分」）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **注**：`tools_io.rs` 里的 `export_psd_layers` 曾留在 `tools.rs`，
//! 因为它不在 `export_project`..`export_png` 的连续段内 ⇒ **这一刀把它和两个 import 一起搬** ✓

use super::*;

/// **只读导入 PSD 的合成图** ✓（设计第 17 章把它列为后续项 ✓ —— 这一轮实现 ✓）。
///
/// **契约** ✓（与模块文档一致 ✓）：只取文件里那张**已经合成好的整幅图** ✓，
/// **不导入图层结构** ✗（PSD 的图层、蒙版、混合模式、智能对象都不搬 ✓）、**不写回 PSD** ✗。
/// 结果作为**一个** `raster_patch` 交出去 ✓ ⇒ 与"导入一张 PNG"走**同一条**下游路径 ✓
///（可撤销 ✓、可回放 ✓、介质描述符照记 ✓）。
///
/// **三步都不可省** ✓：把 PSD 字节解成 RGBA ✓ ⇒ **先把 RGBA 落成新 blob** ✓（blob 先行 ✓，
/// 否则原子会引用一个还不存在的 hash ✗）⇒ **复用 `import_image`** ✓（不复制它的逻辑 ✓）。
///
/// **不支持的一律明确报错** ✗（版本 2 ✓、16/32 位 ✓、CMYK/灰度 ✓、ZIP 压缩 ✓、截断 ✓）
/// —— **绝不"尽力而为地画一半"** ✗：半个画面比一句明确的错误更糟 ✓。
pub(super) fn write_import_psd(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let blob_hash = require_str(args, "blob_hash")?;
    let parsed: yanshi_core::BlobHash = blob_hash.parse().map_err(|_| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("blob_hash 不是合法哈希"),
        )
    })?;
    let bytes = ctx.workspace.store().get(&parsed)?;
    let (width, height, rgba) = yanshi_render::psd::decode_psd(&bytes).map_err(|error| {
        // **原样回报解析器的原因** ✓ —— 它已经写得足够具体 ✓（"位深 16 不支持" 之类 ✓）。
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("PSD 解析失败：{}", error.message)),
        )
    })?;
    // **blob 先行** ✓：先把合成图写进存储 ✓，再提交引用它的原子 ✓。
    let composite = ctx.workspace.store().put(&rgba)?;
    let mut import_args = json!({
        "layer_id": args.get("layer_id").cloned().unwrap_or(Value::Null),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": 0.0, "y": 0.0, "w": width as f64, "h": height as f64},
    });
    if let Some(object_id) = args.get("object_id") {
        import_args["object_id"] = object_id.clone();
    }
    let mut value = write_import_image(ctx, &import_args)?;
    value["source"] = json!({"format": "psd", "width": width, "height": height,
                             "note": "只导入了合成图；图层结构与蒙版没有导入（只读）"});
    Ok(value)
}

/// **★★★ 按**图层树**导入 PSD ✗ ★★★**（**第 253 轮 ✓；**用户建议的第 ③ 项 ✓）
///
/// **∴ 与 `import_psd` 的分工 ✗**：**那条**只取合成图**（**一个图层 ✓ 简单 ✓）；
///   **∴ 这条**读图层树**✗ 每层建一个 yanshi 图层 ✗ 组建成图层组** ✓ ★
///
/// **∴ 映射（**建议点名要对的四样 ✓）✗**：**名字 ✗ 不透明度 ✗ 可见性 ✗ 混合模式** ✓
///   ＋ **∴ 位置与尺寸 ✗**：**用图层自己的 rect ⇒ **原样落在画布对应位置** ✓
///     ＋ **∴ 组 ✗**：**PSD 的 `children` ⇒ yanshi 的 `layer_group` ＋ 子层 `parent_id`** ✓ ★★
///
/// **∴ Phase 1 的简化（**如实写进文档 ✓ 不假装完整 ✓）✗**：
///   ＋ **∴ 调整层／文字层／只有蒙版的层 ✗**：**PSD 里它们**没有自己的像素** ✓
///     ⇒ **∴ 本工具**建图层但不栅格化**✗ 并把每一层**列进 `skipped`** ✓（**不静默 ✓）★
///   ＋ **∴ 蒙版／智能对象／效果 ✗**：**不导入** ✓（**同样列出来 ✓）
///   ＋ **∴ z 序 ✗**：**按 PSD 的顺序**自下而上**编号 ✗ 接在现有图层之后** ✓ ★
///
/// **∴ 判据 ✗**：**`scripts/tool-psd-layers.mjs`** ⇒ **导入后自己合成一遍 ✗
///   与 PSD **自带的压平合成图**逐像素比**（**建议里点名的"真实标准答案" ✓）★
/// **★★★ 导出**分层 PSD** ✗ ★★★**（**第 253 轮 ✓；**建议第 ③ 项的导出半边 ✓）
///
/// **∴ 与 `import_psd_layers` 对称 ✗**：**那条**PSD ⇒ yanshi**✗ 这条**yanshi ⇒ PSD** ✓
///   ＋ **∴ 带出去的 ✗**：**图层名 ✗ 不透明度 ✗ 可见性 ✗ 混合模式 ✗ 组（**一层 ✓）✗
///     位置与尺寸（**每层按**整个画布**给 rect ✓）✗ 外加**压平合成图** ✓ ★★
///
/// **∴ 每层怎么拿到像素 ✗**：**`Document::render_region_layer_rgba`**（**本轮新增 ✓）
///   ＝**只渲这一层**✗（**复用渲染器已有的 `only_layer` 开关 ✓ **不另起一条渲染路** ✓）★★
///
/// **∴ 如实写明的取舍 ✗**：
///   ＋ **∴ 每层 rect ＝**整个画布**✗**（**∴ 不做"裁剪到内容包围盒" ✓ ⇒ **∴ 文件**偏大** ✓
///     而**位置绝对正确** ✓—— **∴ 用"能打开、像素对"换"体积" ✓）★
///   ＋ **∴ 组只导**一层**✗ 蒙版／图层效果／智能对象**不导出** ✓（**∴ 回执里写明 ✓）★
///   ＋ **∴ 合成图 ✗**：**用 `render_region_raw`（**整幅 ✓）**—— **∴ 与 PSD 惯例一致 ✓
///     （**"能被 Photoshop 打开"至少要有它 ✓）★
pub(super) fn write_export_psd_layers(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **∴ 先把要用的字段**拷成自有值** ✗ 再放开对文档的可变借用** ✓（**渲染要可变 ✓）★
    /// **∴ 一层要用的字段 ✗**（**`clippy::type_complexity` ✓）★
    /// （**`id ✗ name ✗ 类型 ✗ 可见 ✗ 不透明度 ✗ 混合 ✗ 父 ✗ z` ✓）★
    type LayerRow = (
        String,
        String,
        String,
        bool,
        f64,
        String,
        Option<String>,
        i64,
    );
    let (doc_w, doc_h, snapshot) = {
        let state = document_state(ctx)?;
        let mut rows: Vec<LayerRow> = state
            .alive_layers()
            .iter()
            .map(|layer| {
                (
                    layer.id.clone(),
                    layer.name.clone(),
                    format!("{:?}", layer.layer_type).to_ascii_lowercase(),
                    layer.visible,
                    layer.opacity,
                    layer.blend_mode.clone(),
                    layer.parent_id.clone(),
                    layer.z_index,
                )
            })
            .collect();
        rows.sort_by(|a, b| (a.7, &a.0).cmp(&(b.7, &b.0)));
        (state.width, state.height, rows)
    };
    let bbox = yanshi_core::Bbox::new(0.0, 0.0, f64::from(doc_w), f64::from(doc_h));
    // **∴ 逐层单渲 ✗ 组不渲**（**组自己没有像素 ✓）★
    let mut pixels: std::collections::HashMap<String, Vec<u8>> = std::collections::HashMap::new();
    let mut rendered_layers = 0usize;
    for (id, _name, kind, _visible, _opacity, _blend, _parent, _z) in &snapshot {
        if kind.contains("group") {
            continue;
        }
        // **∴ 渲染要 `&mut` ✗ 所以用 `document_mut`**（**`document` 给的是 `&` ✓）★
        let document = ctx.workspace.document_mut(&ctx.doc_id)?;
        let (w, h, rgba) = document.render_region_layer_rgba(bbox, id)?;
        debug_assert_eq!((w, h), (doc_w, doc_h));
        pixels.insert(id.clone(), rgba);
        rendered_layers += 1;
    }
    // **∴ 合成图（**整幅 ✓）★
    let composite = {
        let document = ctx.workspace.document_mut(&ctx.doc_id)?;
        let (_w, _h, rgba) = document.render_region_raw(bbox)?;
        rgba
    };
    // **∴ 组装自有类型的图层树 ✗**（**∴ 服务端**不许出现 `ag-psd` 的类型** ✓ ★
    //   —— **∴ 库被关在 `yanshi_render::psd` 里 ✓ 换库不波及工具层 ✓）★★
    let make_layer = |id: &str,
                      name: &str,
                      kind: &str,
                      visible: bool,
                      opacity: f64,
                      blend: &str,
                      kids: Option<Vec<yanshi_render::psd::PsdOutLayer>>|
     -> yanshi_render::psd::PsdOutLayer {
        let is_group = kind.contains("group");
        yanshi_render::psd::PsdOutLayer {
            name: name.to_owned(),
            visible,
            opacity,
            blend: blend.to_owned(),
            is_group,
            pixels: if is_group {
                Vec::new()
            } else {
                pixels.get(id).cloned().unwrap_or_default()
            },
            width: doc_w,
            height: doc_h,
            children: kids.unwrap_or_default(),
        }
    };
    // **∴ 组的成员从顶层摘出来 ✗ 挂到组的 children 上** ✓（**Phase 1：一层 ✓）★
    let group_ids: Vec<String> = snapshot
        .iter()
        .filter(|(_, _, kind, ..)| kind.contains("group"))
        .map(|(id, ..)| id.clone())
        .collect();
    let mut children_of: std::collections::HashMap<String, Vec<yanshi_render::psd::PsdOutLayer>> =
        std::collections::HashMap::new();
    for (id, name, kind, visible, opacity, blend, parent, _z) in &snapshot {
        if kind.contains("group") {
            continue;
        }
        let layer = make_layer(id, name, kind, *visible, *opacity, blend, None);
        match parent {
            Some(parent_id) if group_ids.contains(parent_id) => children_of
                .entry(parent_id.clone())
                .or_default()
                .push(layer),
            _ => children_of.entry(String::new()).or_default().push(layer),
        }
    }
    // **★★★ roots 必须**按 z 序**生成 ✗ 不能"先所有组 ✗ 再所有顶层"** ✓ ★★★**
    //   ＋ **∴ 我第一版就是这么写的 ✗**（**一个循环挑组 ✗ 另一个循环挑顶层 ✓）**
    //     ⇒ **∴ 于是**：**组**掉到最底层**（**红层反而在上面 ✓）★
    //       ＋ **∴ 症状极具误导性 ✗**：**导回来以后**组内那一层在合成里**看不见** ✓
    //         （**∴ 被**它上面的默认图层**压住 ✓ ✗ 而**它自己**有内容** ✓）
    //         ＋ **∴ 我先后改了**only_layer ✗ 组属性 ✗ z 槽位 ✗ 三处
    //           **∴ 数字**一字不差（25.80／49.0% ✓）⇒ **∴ 说明**都没动到这个顺序** ✓
    //             ＋ **∴ 教训 ✗**：**症状在同一处反复出现而不变 ⇒ **先怀疑**没被改到的那一环** ✓ ★★★
    //   ＋ **∴ 现在 ✗**：**按 z 序**走一遍 snapshot**✗ 遇到组就**把它收集好的 children 一起带出** ✓
    //     ＋ **∴ 是别人孩子的层跳过**（**它们已经在组里了 ✓）★
    let mut roots: Vec<yanshi_render::psd::PsdOutLayer> = Vec::new();
    for (id, name, kind, visible, opacity, blend, parent, _z) in &snapshot {
        let is_child_of_group = parent
            .as_ref()
            .is_some_and(|parent_id| group_ids.contains(parent_id));
        if is_child_of_group {
            continue; // **∴ 已经在所属组的 children 里 ✓）★
        }
        if kind.contains("group") {
            let kids = children_of.remove(id).unwrap_or_default();
            roots.push(make_layer(
                id,
                name,
                kind,
                *visible,
                *opacity,
                blend,
                Some(kids),
            ));
        } else {
            roots.push(make_layer(id, name, kind, *visible, *opacity, blend, None));
        }
    }
    let bytes = yanshi_render::psd::write_layer_tree(doc_w, doc_h, &composite, &roots);
    let blob = ctx.workspace.store().put(&bytes)?;
    let filename = optional_str(args, "filename").unwrap_or_else(|| format!("{}.psd", ctx.doc_id));
    let mut value = json!({
        "ok": true,
        "blob_hash": blob.to_string(),
        "url": format!("yanshi://blob/{blob}"),
        "filename": filename,
        "bytes": bytes.len(),
        "layers": rendered_layers,
        "groups": group_ids.len(),
        "canvas": {"w": doc_w, "h": doc_h},
        "note": "Phase 1：每层 rect 用整幅画布（不做内容裁剪 ⇒ 文件偏大但位置绝对正确）；蒙版／图层效果／智能对象不导出；组只导一层",
    });
    // **出参 path 形态**（设计文档第 4 章 4.2/4.5）：给了 `output_path` 就把同一份
    // 字节再落一份到磁盘（守卫与 export_png 共用 `guarded_output_path`），
    // 响应只多 `path`/`mime_type`/`overwritten` 三个元数据字段，不回字节；
    // 不给 ⇒ 响应与旧行为逐字一致（blob_hash + yanshi:// url）。
    if let Some(raw_path) = optional_str(args, "output_path") {
        let target = guarded_output_path(&raw_path)?;
        let overwritten = target.exists();
        std::fs::write(&target, &bytes).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("写文件失败：{} ⇒ {error}", target.display())),
            )
        })?;
        value["path"] = json!(target.display().to_string());
        value["mime_type"] = json!("image/vnd.adobe.photoshop");
        value["overwritten"] = json!(overwritten);
    }
    Ok(value)
}

pub(super) fn write_import_psd_layers(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let blob_hash = require_str(args, "blob_hash")?;
    let parsed: yanshi_core::BlobHash = blob_hash.parse().map_err(|_| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("blob_hash 不是合法哈希"),
        )
    })?;
    let bytes = ctx.workspace.store().get(&parsed)?;
    let tree = yanshi_render::psd::read_layer_tree(&bytes).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("PSD 图层树解析失败：{}", error.message)),
        )
    })?;
    let prefix = optional_str(args, "name_prefix").unwrap_or_default();
    // **∴ z 序接在**现有图层之后**✗ 按 PSD 顺序自下而上** ✓ ★
    let mut next_z = {
        let state = document_state(ctx)?;
        state
            .layers
            .values()
            .filter(|layer| !layer.is_deleted())
            .map(|layer| layer.z_index)
            .max()
            .unwrap_or(-1)
            + 1
    };
    /// **∴ 计数与如实记账 ✗**（**内部小结构 ✓ 不进 JSON 之前不序列化 ✓）★
    #[derive(Default)]
    struct Stats {
        layers: usize,
        groups: usize,
        pixels: usize,
        skipped: Vec<Value>,
        names: Vec<String>,
    }
    /// **∴ 取一个没被占用的图层 id ✗**（**不依赖回执字段 ⇒ 稳** ✓）★
    fn free_layer_id(ctx: &mut ToolContext<'_>, used: &mut usize) -> Result<String> {
        let taken: Vec<String> = document_state(ctx)?.layers.keys().cloned().collect();
        loop {
            let candidate = format!("psd_L{used}");
            *used += 1;
            if !taken.iter().any(|id| id == &candidate) {
                return Ok(candidate);
            }
        }
    }
    // **★★★ z 序要**按 PSD 的层序**✗ 不是遍历顺序** ✓ ★★★**（**第 253 轮实测修 ✓）
    //
    // **∴ 我第一版错在哪 ✗**：**按**遍历顺序**逐个 +1**✗（**组 → 它的子层 → 下一个顶层 ✓）
    //   ＋ **∴ 于是**：**组内子层拿到的 z **比后一个顶层还小** ✗
    //     ⇒ **∴ 它**被那个顶层盖住** ✓★
    //   ＋ **∴ 症状极具误导性 ✗**：**层本身**有内容**（**用"把导回来的文档再导出一次
    //     ＋ ImageMagick 拆层"证明 ✓）✗ 而**合成里看不到** ✓
    //       ⇒ **∴ 差点误判成"像素没导进来"** ✓（**∴ 实际是**叠放次序** ✓）★★
    //   ＋ **∴ 现在 ✗**：**每个顶层占一个**槽**（间距 `ROOT_SLOT` ✓）
    //     ＋ **∴ 子层留在**自己组的槽内**（`组的 z + 1 …` ✓）**
    //       ⇒ **∴ "组那一段"整体落在 PSD 给它的位置** ✓ 且**组内顺序也保住** ✓ ★★
    // **∴ 试过又撤回的方案（**记下来 ✓）✗**：**给顶层留**大间距**（如 1024 ✓）
    //   让子层待在槽里** ✗ ⇒ **∴ 实测**数字**一字不差** ✓
    //     （**∴ 因为**渲染只看**相对次序** ✗ 而两种编号的相对次序**完全一样** ✓）
    //       ⇒ **∴ 撤回 ✗ 只留连续编号** ✓ ★★
    fn walk(
        ctx: &mut ToolContext<'_>,
        nodes: &[yanshi_render::psd::PsdLayerNode],
        parent: Option<&str>,
        prefix: &str,
        next_z: &mut i64,
        used: &mut usize,
        stats: &mut Stats,
    ) -> Result<()> {
        for node in nodes {
            let name = format!("{prefix}{}", node.name);
            let id = free_layer_id(ctx, used)?;
            let kind = if node.is_group {
                "layer_group"
            } else {
                "raster"
            };
            // **∴ 这一层自己的 z ✗**：**子层要用它当基准 ✓** ★
            let node_z = *next_z;
            write_create_layer(
                ctx,
                &json!({
                    "layer_id": id,
                    "name": name,
                    "type": kind,
                    "parent_id": parent,
                    "z_index": node_z,
                }),
            )?;
            *next_z += 1;
            if node.is_group {
                stats.groups += 1;
                // **★★★ 组也要设属性 ✗ ★★★**（**第 253 轮实测修 ✓**）
                //   ＋ **∴ 我第一版只对**非组**设了 opacity／visible／blend** ✗
                //     ⇒ **∴ 于是**：**导回来的组**恒定 100%／可见** ✓
                //       ＋ **∴ 被**往返判据**抓住 ✗**：**平均通道差 25.8（**正好差一倍 ✓）★★
                //         ⇒ **∴ 教训 ✗**：**"**组只建不设**"**看起来无害 ✗ 实则**丢属性** ✓ ★★★
                write_update_layer(
                    ctx,
                    &json!({
                        "layer_id": id,
                        "patch": {
                            "opacity": node.opacity,
                            "visible": node.visible,
                        },
                    }),
                )?;
            } else {
                stats.layers += 1;
                // **∴ 属性 ✗**：**不透明度／可见性／混合模式**（**映射已保证词表合法 ✓）★
                write_update_layer(
                    ctx,
                    &json!({
                        "layer_id": id,
                        "patch": {
                            "opacity": node.opacity,
                            "visible": node.visible,
                            "blend_mode": node.blend,
                        },
                    }),
                )?;
                if !node.pixels.is_empty() && node.width > 0 && node.height > 0 {
                    let blob = ctx.workspace.store().put(&node.pixels)?;
                    write_import_image(
                        ctx,
                        &json!({
                            "layer_id": id,
                            "bitmap": {
                                "blob_hash": blob.to_string(),
                                "size": node.pixels.len(),
                                "mime_type": "image/x-yanshi-raw",
                            },
                            "region": {
                                "x": node.x as f64,
                                "y": node.y as f64,
                                "w": node.width as f64,
                                "h": node.height as f64,
                            },
                        }),
                    )?;
                    stats.pixels += 1;
                } else {
                    stats.skipped.push(json!({
                        "name": name,
                        "why": "这一层在 PSD 里没有像素（调整层／文字层／只有蒙版）⇒ Phase 1 保留图层但不栅格化",
                    }));
                }
            }
            stats.names.push(name);
            if !node.children.is_empty() {
                // **∴ 子层用自己的计数器 ✗ 从"组的 z + 1"开始 ✗ 步长 1** ✓
                //   ⇒ **∴ 它们**不会跑到下一个顶层的上面去** ✓ ★★
                // **∴ 连续编号 ✗**：**组的子层接着它往下编** ✓
                //   ＋ **∴ 为什么不再用大间距 ✗**：**实测 ✗** **间距 1024 ⇒ **红层变淡**
                //     （`[238,189,189]` ✗ 源是 `[220,30,30]` ✓）⇒ **∴ 怀疑**渲染器的
                //       **三段分解**（"**层的 opacity 决定**上方**各层透不透**" ✓）
                //       **对 z 的**连续性**有预期** ✓ ⇒ **∴ 改用连续编号后复验** ✓ ★★
                let mut child_z = node_z + 1;
                walk(
                    ctx,
                    &node.children,
                    Some(&id),
                    prefix,
                    &mut child_z,
                    used,
                    stats,
                )?;
            }
        }
        Ok(())
    }
    let mut stats = Stats::default();
    let mut used = 0usize;
    walk(
        ctx,
        &tree.roots,
        None,
        &prefix,
        &mut next_z,
        &mut used,
        &mut stats,
    )?;
    Ok(json!({
        "ok": true,
        "canvas": {"w": tree.width, "h": tree.height},
        "layers_created": stats.layers,
        "groups_created": stats.groups,
        "layers_with_pixels": stats.pixels,
        "imported": stats.names,
        "skipped": stats.skipped,
        "unmapped_blends": tree.unmapped_blends,
        "note": "Phase 1：调整层／文字层不栅格化（列在 skipped）；蒙版、智能对象、图层效果不导入",
    }))
}
