//! **导出工具**（`export_project` / `export_book_pngs` / `export_png`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项「tools.rs 拆分」）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **注意**：`export_psd_layers` 还在 `tools.rs` 的另一段（不在本段内）⇒ 那是**下一刀**。

use super::*;

pub(super) fn write_export_project(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **`path` 从"必填"改成"可选"** ✓（本轮补的是**浏览器那一半** ✓）：原来只回一个**服务器上的
    // 路径** ✗ ⇒ 浏览器给不出服务器路径、也拿不到那台机器上的文件 ✗ ⇒ 对 Web 端等于**没有出口** ✗。
    // 现在**总是**把包当成 CAS 里的一个 blob ✓ ⇒ 回一个 `yanshi://blob/<hash>` ✓，
    // 由 HTTP 层改写成可直接 GET 的 `/api/blob/<hash>?doc=..&token=..` ✓
    //（与缩略图 / PNG / 诊断包**同一条通道** ✓）⇒ 浏览器一个 `<a download>` 就存到本机 ✓。
    // **给了 `path` 仍照旧写那份文件** ✓（MCP、命令行与既有判据都在用它 ✓ ⇒ 行为不破 ✓）。
    let path = match optional_str(args, "path") {
        Some(text) => Some(guarded_output_path(&text)?),
        None => None,
    };
    let doc_id = optional_str(args, "doc_id").unwrap_or_else(|| ctx.doc_id.clone());
    if ctx.workspace.document(&doc_id).is_none() {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {doc_id} 未打开 ⇒ 无法导出")),
        ));
    }
    // **`include_bitmaps` 缺省 false** ✓：只省掉**能证明重放得出来**的位图 ✓（见 `export_project` ✓）。
    // `true` ⇒ **每个位图都装** ✓（老行为 ✓，排查"是不是我省错了"时用 ✓）。
    //
    // **为什么不再"当场渲染一张整幅图"** ✗：那一张**从来没有进过包** ✓
    //（`export_project` 的 `render.png` 分支早已删掉 ✓）⇒ 每次导出都在**白渲染一张 4K 图**
    // 再把 PNG 编码一遍丢掉 ✓ ⇒ 顺手去掉 ✓（导出更快 ✓，行为不变 ✓ —— 包里本来就没有它 ✓）。
    let include_bitmaps = args
        .get("include_bitmaps")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // **分段探针（第 51 轮 ✓）**：在**调用侧**量 ✓ —— 原因 ✗：构造点 `ExportStats { … }`
    // 位于**表达式位置**（`Ok((…))` ✓），语句插不进去 ✓（我为此失败两次 ✗）。
    // 这里量两段 ✓：① `export_project`（读日志 ＋ 收集 ＋ 组装 tar ✓）② `store.put`（入 CAS ✓）。
    let _probe_t0 = std::time::Instant::now();
    // **导出第三轮分段探针（第 63 轮 ✓）**：`other_ms` ＝ 1254 ms ✗，而已知
    // `export_project` 75 ms ✓ ＋ 存储 18 ms ✓ ⇒ **∴ ~1.16 s 未归属** ✓ ⇒ 必须量 ✓。
    // **锚点纪律** ✗：每处都断言**唯一** ✓（第 382 轮我因锚点不唯一插错过位置 ✗）。
    let _p3 = std::time::Instant::now();
    let _p3_mark = |tag: &str| {
        if std::env::var_os("YANSHI_TRACE_EXPORT3").is_some() {
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/yanshi-export3-trace.log")
                .and_then(|mut f| {
                    use std::io::Write;
                    writeln!(f, "EXPORT3 {tag} {} ms", _p3.elapsed().as_millis())
                });
        }
    };
    // **导出结果缓存（方案 C ✓）**：查出 `head_seq` 作为键，命中即整段跳过。
    let head_seq = ctx
        .workspace
        .document(&doc_id)
        .map(|document| document.head_seq())
        .unwrap_or(0);
    let cache_key: ExportResultKey = (
        env!("CARGO_PKG_VERSION").to_owned(),
        head_seq,
        include_bitmaps,
    );
    let (tar, stats) = match export_result_lookup(&cache_key) {
        Some(hit) => hit,
        None => {
            let built = ctx.workspace.export_project(&doc_id, include_bitmaps)?;
            // **只在"存储没有参与"时才记入** ✗ —— 这正是第 82 轮的漏洞所在 ✓。
            if built.1.kept_no_recipe == 0
                && built.1.kept_mismatch == 0
                && built.1.kept_missing == 0
            {
                export_result_remember(cache_key, built.clone());
            }
            built
        }
    };
    _p3_mark("export_project");
    if std::env::var_os("YANSHI_TRACE_EXPORT2").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/yanshi-export2-trace.log")
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(
                    f,
                    "EXPORT2 export_project={} ms tar_bytes={}",
                    _probe_t0.elapsed().as_millis(),
                    tar.len()
                )
            });
    }
    let _probe_t1 = std::time::Instant::now();

    // **保存工程时顺手把"整幅渲染"落成缓存** ✓（第 3 轮 ✓，"打开太慢"专题 ✓）。
    //
    // **为什么在这里做** ✓：读侧已经就绪 ✓ —— `Workspace::open` 会对持久化结果调
    // `Document::restore_persisted_render` ✓（`service.rs:1112` ✓，注释记着"省掉实测 142 s 的重渲染" ✓）。
    // 但**写侧从来没有写过"整幅"那一份** ✗：`save_render` 只在**整幅**渲染时调用 ✓
    //（`service.rs:1742`，在 `if full_frame` 里 ✓），而绘画只产生**区域**渲染 ✗
    // ⇒ 实测：4K 工程冷启动首次 `get_document` 要 **1.3~1.8 s**，**全部**花在 `preview_ms` ✗
    //（整幅渲染 ＋ PNG 编码 ✓），而那 2 个落盘的 PNG 只是 **256² 的文档预览** ✓ ⇒ 救不了整幅请求 ✗。
    //
    // **两件事必须分开** ✓（尊重前任的决定 ✓）：**不把这张图放进工程包** ✗
    //（`render.png` 分支早已删掉 ✓，包里本来就没有它 ✓）；只是**让这次渲染的结果落成缓存** ✓。
    //
    // **代价两面** ✗：收益＝保存之后任何一次冷启动打开都只需"读 PNG 头 ＋ 入 CAS" ✓；
    // 代价＝保存时多一次整幅渲染（4K ≈ 1.3 s ✓）＋ 一份整幅 PNG 落盘（约 1~10 MB ✓）
    // ⇒ **∴ 只在这一个显式点做** ✓，**绝不放进每一笔** ✓。
    //
    // **已经最新就不重渲** ✓（`render_watermark == head_seq` ⇒ 跳过 ✓），
    // 而且**不牺牲精度** ✓：这是无损的显示缓存 ✓，真值仍是原子日志 ＋ 内容寻址 blob ✓
    //（过期就重算 ✓、丢了就重算 ✓）。
    {
        let (behind, width, height) = {
            let document = ctx.workspace.document(&doc_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {doc_id} 未打开 ⇒ 无法落渲染缓存")),
                )
            })?;
            (
                document.render_watermark() < document.head_seq(),
                document.state().width,
                document.state().height,
            )
        };
        if behind {
            let full = Bbox::new(0.0, 0.0, width as f64, height as f64);
            // 整幅渲染 ⇒ 内部会自动 `save_render` ✓（`service.rs:1742` ✓）—— **不需要新管道** ✓。
            let _ = ctx.workspace.render_region(&doc_id, full)?;
        }
        // **还要那份 256² 的"文档预览"** ✓ —— 实测为据 ✓：只写整幅那份时，冷启动首次
        // `get_document` **仍是 1208 ms** ✗（`preview_ms=1202` ✓），因为 `get_document` 走的是
        // **文档预览**那条路 ✓（`render_document_preview` ✓ ⇒ 它要的就是这份 256² 缩略图 ✓），
        // 而"整幅渲染恢复"只喂了**区域缓存** ✗ ⇒ 救不了它 ✗。
        // ⇒ **∴ 保存点两件都写** ✓（两份都无损 ✓、都只是显示缓存 ✓、都"过期就重算" ✓）。
        // **先要有那份 256² 缩略图** ✓ —— 实测为据 ✓：`cache_document_preview` 里有一道
        // **尺寸检查** ✓（它要求 blob 的 PNG 是 `ThumbKind::Doc256.size()` ✗），而我上一步
        // `render_region(整幅)` 留下的 `document_thumbnail` 是 **4096×3072** ✗
        // ⇒ 尺寸不符 ⇒ **直接 `return Ok(())`** ✗ ⇒ 磁盘上那份 `preview.png` **从来没写出来** ✗
        //（实测：磁盘只有 1 个 PNG ＝ `render.png` ✓，冷启动仍整幅重渲 **1191 ms** ✗）。
        // **∴ 这一步是"lazy 打开"能否成立的最后一环** ✓：有了 256² 基座 ✓，
        // 冷启动 `restore_persisted_preview` ✓ 才能算出 `(seq, HEAD]` 的脏区 ✓
        // ⇒ 第一次预览**只重渲那一块** ✓（正是 GIMP/Krita 的 dirty-rect 复用法 ✓）。
        // 缩略图从**刚填好的整幅缓存**里取 ✓ ⇒ 便宜 ✓（不是再来一次整幅渲染 ✗）。
        if let Ok(document) = ctx.workspace.document_mut(&doc_id) {
            // **按需生成** ✓（第 75 轮 ✓）：已最新就**跳过** ✗ —— 否则每次保存都白付一次整幅渲染 ✗
            //（实测 4K ~1.07 s ✗／8K ~9 s ✗，而画面零变化 ✓）。
            if !document.doc_thumbnail_is_current() {
                // **量"缓存暖不暖"** ✓（第 90 轮 ✓）：B 路（`render_region(raw)` ✓）**只在暖时**才划算 ✓
                //（暖 10.7 ms ✓／冷 1388 ms ✗ ⇒ **∴ 若这里显示整幅 tile 已在缓存 ⇒ B 成立** ✓）。
                if std::env::var_os("YANSHI_TRACE_THUMBWARM").is_some() {
                    let c = document.cache_stats();
                    let _ = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open("/tmp/yanshi-thumbwarm-trace.log")
                        .and_then(|mut f| {
                            use std::io::Write;
                            writeln!(
                                f,
                                "THUMBWARM tiles={} used_bytes={} hits={} misses={} evictions={}",
                                c.tiles, c.used_bytes, c.hits, c.misses, c.evictions
                            )
                        });
                }
                // **量前提** ✓（第 86 轮 ✓）：此刻 `document_thumbnail` 到底是**整幅**还是**已是 256²** ✗
                // —— 这决定"由已有整幅降采样"这条路**是否成立** ✓（**不许在未证实的前提上盖楼** ✗）。
                if std::env::var_os("YANSHI_TRACE_THUMBSRC").is_some() {
                    if let Some(hash) = document.doc_thumbnail_hash_for_probe() {
                        let sz = document.store().size(&hash).unwrap_or(0);
                        let _ = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/yanshi-thumbsrc-trace.log")
                            .and_then(|mut f| {
                                use std::io::Write;
                                writeln!(f, "THUMBSRC doc_thumbnail_bytes={sz}")
                            });
                    } else {
                        let _ = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/yanshi-thumbsrc-trace.log")
                            .and_then(|mut f| {
                                use std::io::Write;
                                writeln!(f, "THUMBSRC none")
                            });
                    }
                }
                let _ = document.thumbnail(yanshi_render::thumb::ThumbKind::Doc256, None);
            }
            _p3_mark("thumbnail_doc256");
        }
        let _ = ctx.workspace.cache_document_preview(&doc_id);
        _p3_mark("cache_document_preview");
    }
    // **先落 `path`** ✓（老行为 ✓：写不进去要**响亮地失败** ✓，而不是回一个 URL 就当成功 ✓）。
    if let Some(path) = &path {
        std::fs::write(path, &tar).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("写文件失败：{} ⇒ {error}", path.display())),
            )
        })?;
    }
    // **再把同一份字节放进 CAS** ✓：`put` 是**内容寻址**的 ✓（同内容幂等去重 ✓），
    // 回的哈希就是 `yanshi://blob/<hash>` 里的那一个 ✓ ⇒ 取回来的**必然**是这份包 ✓。
    // **⚠️ 已否证的假设（第 61 轮实测 ✓）**：曾把这里改成 `put_cache`（跳过 `fsync` ✓），
    // 指望省下探针量到的 **~1.2 s** ✗ ⇒ **实测毫无变化** ✓（4K 2720/1255 ms ✓、8K 12187/9174 ms ✓，
    // 与改前**逐个相同** ✓）⇒ **∴ 那 1.2 s **不是** fsync** ✗ ⇒ **∴ 它在 `put_inner` 里与 `sync` 无关的部分**
    //（`.tmp` ＋ `rename` ＋ 写入路径 ✓）。
    // **∴ 已撤回** ✓ —— **不许**为了零收益去削弱持久性 ✗（那是**双输** ✓）。
    // **∴ 下一步若要省它** ✓：在 `FsBlobStore::put_inner` **内部分段记时** ✓（写字节／改名／同步 ✓）。
    let hash = ctx.workspace.store().put(&tar)?;
    _p3_mark("store_put");
    if std::env::var_os("YANSHI_TRACE_EXPORT2").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/yanshi-export2-trace.log")
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(
                    f,
                    "EXPORT2 store_put={} ms",
                    _probe_t1.elapsed().as_millis()
                )
            });
    }
    let mut value = json!({
        // **浏览器下载用的地址** ✓（服务端 URL 形态会被改写 ✓；`path` 只对同机的调用方有意义 ✓）。
        "url": format!("yanshi://blob/{hash}"),
        "blob_hash": hash.as_str(),
        // 建议的下载文件名 ✓（`<a download>` 用它 ✓；不带路径分隔符 ⇒ 不会写进子目录 ✓）。
        "filename": format!("{doc_id}.yanshi"),
        "doc_id": doc_id,
        "bytes": tar.len(),
        "include_bitmaps": include_bitmaps,
        "format": "tar (uncompressed)",
        // **位图部分的独立读数** ✓（第 1331 轮 ✓）：整包体积会被**原子日志**主导 ✗
        //（实测：12 次覆盖的差值里 **98.3%** 是日志 ✓）⇒ 所以"位图有没有被回收"必须看这三项 ✓。
        "blob_count": stats.blob_count,
        "blob_bytes_plain": stats.blob_bytes_plain,
        "blob_bytes_packed": stats.blob_bytes_packed,
        // **重放验证的读数** ✓（第 35 轮 ✓）：`omitted` ＝ 被"可重放证明"省掉的位图数 ✓
        // ⇒ 它**就是导出慢的来源有多少次** ✓（第 346 轮查明：那 2.65 s／12 s 主要是重放 ✓）。
        "omitted": stats.omitted,
        "kept_no_recipe": stats.kept_no_recipe,
        "kept_mismatch": stats.kept_mismatch,
        // **缺 blob 的读数** ✓（第 70 轮 ✓）：**降级了但必须报出来** ✗ ——
        // 用户实测：15MB 文档引用了一个不存在的 blob ⇒ 从前**整个导出失败** ✗。
        "kept_missing": stats.kept_missing,
        "missing": stats.missing,
    });
    if let Some(path) = &path {
        value["path"] = json!(path);
    }
    Ok(value)
}

/// **`export_png`** ✓ —— 把整幅（或指定区域）渲染成 PNG **落盘** ✓。
///
/// **为什么需要它** ✓（真实用户报的 §P2-6 ✓）：`render_region` 的 `include_image` **上限 512px** ✗
///（`preview.width <= 512 && preview.height <= 512` ✓），超了只回一个 `yanshi://blob/...` ✗
/// —— 那是**伪协议** ✓，进程外**取不到** ✗（用户原话 ✓）。
/// ⇒ agent 既不能抠 base64 ✓、也拿不到文件 ✓ —— 导出这一步**实际上没有出口** ✗。
///
/// **零件其实都齐** ✓，缺的只是把它们接起来 ✓：
/// `render_region_raw` ✓（取像素 ✓）＋ `yanshi_core::resample::resample_rgba` ✓（缩放 ✓）
/// ＋ `yanshi_render::png::encode_png` ✓（`flate2`＋`zlib-rs` ✓）。
///
/// **语义** ✓：`mutating: false` ✓ —— 它**不改文档** ✓；但**它会写文件** ✓
/// ⇒ 这一点写进了 tools.md ✓，不假装它"只是读" ✗。
/// **`export_book_pngs`** ✓：把多个文档**按顺序**各导出一页 PNG ✓（外部复测 `QX-G02` ✓）。
///
/// **为什么先做 PNG 序列、不直接做 PDF** ✗（第 426 轮的决定 ✓）：
/// PDF 要新依赖（`printpdf` 或手写嵌入器 ✓），而「多页」这件事的**判据**只要
/// 「**文件数 = 文档数** ＋ **命名有序** ＋ **每页尺寸 = 文档尺寸**」✓ ⇒
/// **∴ 它**零依赖、可先判 ✓ ⇒ **∴ 而** PDF **由**这串 PNG **拼**出来 ✓。
///
/// **顺序就是页序** ✓：`doc_ids` 的**下标**决定文件名（`p00`／`p01`… ✓），不排序 ✗
/// ⇒ **∴ 于是**调用方**能**用数组表达「封面 → 第 1 页 → …」✓。
///
/// **每页用**该文档自己的**全幅** ✓（不是调用方 `ctx.doc_id` 的尺寸 ✗）⇒
/// **∴ 所以**不同尺寸的页面**各自**原尺寸导出 ✓（书页可以不一致 ✓）。
pub(super) fn write_export_book_pngs(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let doc_ids: Vec<String> = match args.get("doc_ids").and_then(Value::as_array) {
        Some(list) => list
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("doc_ids 里每一项都要是字符串（文档 id）"),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?,
        None => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("doc_ids 必须是数组（顺序就是页序）"),
            ))
        }
    };
    if doc_ids.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("doc_ids 不能为空 ⇒ 一本书至少一页"),
        ));
    }
    let dir = guarded_output_path(&require_str(args, "dir")?)?;
    std::fs::create_dir_all(&dir).map_err(|error| {
        YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("建不了输出目录 {}：{error}", dir.display())),
        )
    })?;
    let mut pages: Vec<Value> = Vec::new();
    for (index, doc_id) in doc_ids.iter().enumerate() {
        let (width, height, pixels) = {
            let document = ctx.workspace.document(doc_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("第 {index} 页的文档 {doc_id} 未打开")),
                )
            })?;
            let state = document.state();
            let region = yanshi_core::Bbox::new(0.0, 0.0, state.width as f64, state.height as f64);
            ctx.workspace.render_region_raw(doc_id, region)?
        };
        let png = yanshi_render::png::encode_png(width, height, &pixels).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("第 {index} 页尺寸不可用（{width}×{height}）")),
            )
        })?;
        let path = dir.join(format!("p{index:02}.png"));
        std::fs::write(&path, &png).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("写不进 {}：{error}", path.display())),
            )
        })?;
        pages.push(json!({
            "index": index,
            "doc_id": doc_id,
            "path": path.display().to_string(),
            "width": width,
            "height": height,
            "bytes": png.len(),
        }));
    }
    Ok(json!({
        "ok": true,
        "dir": dir.display().to_string(),
        "pages": pages,
        "count": doc_ids.len(),
        "note": "文件名的序号就是页序（p00 是第一页）⇒ 拼 PDF 时按这个顺序",
    }))
}

pub(super) fn write_export_png(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let path = guarded_output_path(&require_str(args, "path")?)?;
    // 缺省整幅 ✓：区域没给就用文档尺寸 ✓。
    let region = match args.get("region") {
        Some(value) if !value.is_null() => parse_bbox(value)?,
        _ => {
            let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
                )
            })?;
            let state = document.state();
            yanshi_core::Bbox::new(0.0, 0.0, state.width as f64, state.height as f64)
        }
    };
    // **给了 `layer_id` ⇒ 只渲染那一层** ✓（用户报过的缺口 ✓）。
    // **"忽略可见性"是有意的** ✓ —— 导出某一层是明确要求 ✓（隐藏层也应当能导出 ✓）。
    // **注意它会绕过区域字节缓存** ✓：那张缓存的键里**没有图层** ✗ ⇒
    // 一旦误用就会把别的图层的像素当成这一层的 ✓（见 `render_region_raw_layer` 的说明 ✓）。
    //
    // **`render_ms` 从这里开始** ✓（导出路径的像素产出 ✓）：取像素是一段 ✓，
    // 缩放是另一段 ✓（两段分别计时后累加 ✓ —— 同一个 `Instant` 不能记两次 ✗，
    // 那会把同一段重复累计成两倍 ✓）。
    let render_started = std::time::Instant::now();
    let (width, height, pixels) = match optional_str(args, "layer_id") {
        Some(layer_id) => ctx
            .workspace
            .render_region_raw_layer(&ctx.doc_id, region, &layer_id)?,
        None => ctx.workspace.render_region_raw(&ctx.doc_id, region)?,
    };
    ctx.time(Phase::Render, render_started);
    // **落盘的图也必须把"被跳过的东西"带出去** ✗：这条路同样返回裸像素 ✓，
    // 缺块时**不能**默默写一张不完整的 PNG 却不提 ✓。
    let render_warnings = ctx.workspace.last_render_warnings(&ctx.doc_id);
    if width == 0 || height == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("区域为空 ⇒ 没有可导出的像素"),
        ));
    }
    // **目标尺寸** ✓：显式宽高 ⇒ 用它；`max_edge` ⇒ 等比缩到最长边；都没给 ⇒ 原尺寸 ✓。
    let asked_width = optional_u64(args, "width").map(|value| value as u32);
    let asked_height = optional_u64(args, "height").map(|value| value as u32);
    let max_edge = optional_u64(args, "max_edge").map(|value| value as u32);
    if max_edge.is_some() && (asked_width.is_some() || asked_height.is_some()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("max_edge 与 width/height 二选一 ✓，不要同时给"),
        ));
    }
    let (target_w, target_h) = match (asked_width, asked_height, max_edge) {
        (Some(w), Some(h), _) => (w, h),
        (None, None, Some(edge)) if edge > 0 => {
            let longest = width.max(height) as f64;
            let scale = (edge as f64 / longest).min(1.0);
            (
                ((width as f64 * scale).round() as u32).max(1),
                ((height as f64 * scale).round() as u32).max(1),
            )
        }
        (None, None, None) => (width, height),
        _ => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("width 与 height 要一起给 ✓（或改用 max_edge）"),
            ))
        }
    };
    // **★ `deterministic: true`（第 1199 轮，外部 §164 请求）** ✓
    //   **为什么需要**：外部实测「同一文档连续两次 `export` ⇒ 43,194 像素不同、最大差 25」
    //   ⇒ 而他们同时实测 `render_region(raw=true)` 两次的 CAS 哈希**完全相同**
    //   ⇒ 所以非确定性**不在渲染**，而在**导出路径的后半段**（缩放 or 编码）✓
    //
    //   **本模式的语义**（明确写的取舍，不是「大概确定」）：
    //   · **禁止缩放**：给了不等于源尺寸的 width/height/max_edge ⇒ **报错**
    //     （缩放是逐行浮点重采样 ⇒ 最可能的累加顺序敏感点）
    //   · **强制 `Nearest`**：即使源尺寸不变也锁死算法（防御性 —— 若将来加了别的路径）
    //   ⇒ 满足这两条后，同一文档同一参数两次导出**应当逐像素相同**
    let deterministic = args
        .get("deterministic")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if deterministic && (target_w != width || target_h != height) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "deterministic 模式不做缩放：源 {}×{}，目标 {target_w}×{target_h} ⇒ 请去掉 width/height/max_edge（缩放是逐行浮点重采样，是最可能的非确定源）",
                width, height
            )),
        ));
    }
    let filter = if deterministic {
        yanshi_core::resample::ResampleFilter::Nearest
    } else {
        match optional_str(args, "filter").as_deref() {
            None | Some("bilinear") => yanshi_core::resample::ResampleFilter::Bilinear,
            Some("nearest") => yanshi_core::resample::ResampleFilter::Nearest,
            Some(other) => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "未知缩放算法 {other} ✓（可用 nearest / bilinear）"
                    )),
                ))
            }
        }
    };
    let scaled = if target_w == width && target_h == height {
        pixels
    } else {
        // **缩放也算"产出要编码的像素"** ✓：它同样在 `render_ms` 里 ✓，
        // 否则缩放型导出的这一段时间又会掉回残差 ✓（这里同样只记一次 ✓）。
        let resample_started = std::time::Instant::now();
        let resampled = yanshi_core::resample::resample_rgba(
            &pixels, width, height, target_w, target_h, filter,
        );
        ctx.time(Phase::Render, resample_started);
        resampled.ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "缩放失败 ✓（{width}×{height} ⇒ {target_w}×{target_h}）"
                )),
            )
        })?
    };
    // `encode_png` 对**不可用的尺寸**返回 `None` ✓ ⇒ 按"参数不合法"报 ✓（`ErrorCode` 里没有 Internal ✓）。
    // **`png_ms` 只圈编码这一段** ✓（写文件与刷新缓存都留在残差里 ✓ —— 它们不是编码 ✓）。
    let png_started = std::time::Instant::now();
    let png = yanshi_render::png::encode_png(target_w, target_h, &scaled);
    ctx.time(Phase::Png, png_started);
    let png = png.ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "PNG 编码失败 ✓（{target_w}×{target_h} 不可用 ⇒ 请给一个合理尺寸）"
            )),
        )
    })?;
    // **整幅导出时顺手刷新渲染缓存** ✓（真实用户工程包里那张**空 `render.png`** 的根因 ✓）：
    // 缓存原本只在"整幅渲染 / 文档级缩略图"时更新 ✓，而导出走的是 `render_region_raw` ✓
    // ⇒ **绕过缓存** ✗ ⇒ 画完整幅画之后 ✓，磁盘上的预览**仍停在空白** ✗（他四个包实测 seq 5–6 vs 原子 97–217 ✓）。
    // **只在整幅时做** ✓：局部导出刷新缓存会把"打开即图片"变成一张**局部图** ✗。
    // **拿不到 head 也不阻断导出** ✓（缓存只是加速 ✓，不是产品 ✓）。
    let covers_frame = region.x <= 0.0
        && region.y <= 0.0
        && region.w
            >= ctx
                .workspace
                .document(&ctx.doc_id)
                .map(|document| document.state().width as f64)
                .unwrap_or(f64::MAX)
        && region.h
            >= ctx
                .workspace
                .document(&ctx.doc_id)
                .map(|document| document.state().height as f64)
                .unwrap_or(f64::MAX);
    // **真的落盘** ✓：这是这个工具存在的理由 ✓（不是再给一个拿不到的 URL ✗）。
    std::fs::write(&path, &png).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("写文件失败：{} ⇒ {error}", path.display())),
        )
    })?;
    if covers_frame && target_w == width && target_h == height {
        // 只在**原尺寸整幅**时写缓存 ✓（缩放过的图不该当成"打开即图片" ✗）。
        let _ = ctx.workspace.cache_full_frame_png(&ctx.doc_id, &png);
    }
    // **★ 必须报告「这张图是不是落后于 head」** ✓（第 1346 轮；外部 **X-03** 换来的）
    //   **⇒ 外部实测**：`yanshi-mcp --no-wait` 下连下 10 笔后立刻 `export_png`
    //     ⇒ **只有第 1 笔出现在图里** ⇒ 而响应里**没有任何提示** ✗
    //   ⇒ 所以调用方**无从判断**这张图是「全部内容」还是「部分内容」⇒ 那是静默降级 ✗
    //   **⇒ 修法不是「悄悄等」**（`--no-wait` 的调用方**明确选择了不等** ✓）
    //     ⇒ 而是**把差距说出来**：`head_seq` / `rendered_seq` / `raster_pending` ✓
    //   **⇒ 判据不变量**：`raster_pending === (rendered_seq < head_seq)` ✓（两个数不一致就是矛盾）
    let (head_seq, rendered_seq) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| (document.head_seq(), document.render_watermark()))
        .unwrap_or((0, 0));
    Ok(json!({
        "path": path,
        "width": target_w,
        "height": target_h,
        "bytes": png.len(),
        "source_width": width,
        "source_height": height,
        "scaled": target_w != width || target_h != height,
        // **★ 这里**不许**给 `raster_pending`** ✗（第 1364 轮自查推翻第 1346 轮）
        //   **⇒ 我原先加的 `raster_pending = rendered_seq < head_seq` 是**误报**：
        //     `rendered_seq` 是**文档渲染水位**（预览/缩略图那条线），而 `export_png` 是
        //     **同步重渲**（`render_region_raw` 从原子直接渲）⇒ **与那个水位无关** ✓
        //   **⇒ 实测**：`batch` 50 笔（`wait_for_render:false`）后导出 ⇒
        //     `head=52 / rendered=2`（水位落后）⇒ 而导出的图**非白像素 45100**、
        //     两张（连导两次）**逐像素相同** ⇒ **图是完整的** ⇒ 那个 `true` 会让人**白跑一遍** ✗
        //   **⇒ 所以**只留两个**如实**的数字（head 与渲染水位），并说清它们**不管**导出的完整性 ✓
        "head_seq": head_seq,
        "rendered_seq": rendered_seq,
        // **⇒ 导出完整性**不需要这个字段：`export_png` **总是**从原子同步重渲 ✓

        "warnings": render_warnings,
    }))
}

/// SVG 场景节点（`export_svg` 服务端组装 ⇒ 递归序列化 ✓）。
enum SvgNode {
    /// 矢量填充路径（一笔的一层 ✓）。
    Path {
        /// `d` 属性（相对命令、定点坐标 ✓）。
        d: String,
        /// `#rrggbb` 填充色。
        fill: String,
        /// `fill-opacity`（已含 α_eff ✓；恰为 1 时序列化时省略 ✓）。
        fill_opacity: f64,
        /// CSS 混合模式（`None` = normal ⇒ 不写 ✓）。
        blend: Option<&'static str>,
    },
    /// 原生形状元素（rect/ellipse/polygon 的完整标记 ✓）。
    Shape {
        /// 元素标记（不含缩进）。
        markup: String,
    },
    /// 位图回退图（紧凑包围盒 PNG、base64 内联 ✓）。
    Image {
        /// 左上角 x（文档坐标）。
        x: i64,
        /// 左上角 y。
        y: i64,
        /// 宽（像素）。
        w: u32,
        /// 高（像素）。
        h: u32,
        /// base64 PNG。
        png_base64: String,
        /// CSS 混合模式（图层级回退时用 ✓）。
        blend: Option<&'static str>,
    },
    /// 图层组（`isolation:isolate` ✓ + 图层不透明度/混合 ✓）。
    Group {
        /// 组不透明度（恰为 1 时省略 ✓）。
        opacity: f64,
        /// CSS 混合模式。
        blend: Option<&'static str>,
        /// 子节点（绘制序 ✓）。
        children: Vec<SvgNode>,
    },
    /// 被橡皮掩码的组（掩码定义在节点里、序列化时统一收进 `<defs>` ✓）。
    Masked {
        /// `<mask>` 定义全文。
        def: String,
        /// 掩码 id（`url(#…)` 引用 ✓）。
        mask_id: String,
        /// 被掩码的子节点。
        children: Vec<SvgNode>,
    },
}

/// 序列化一个节点（掩码定义顺手收进 `defs` ✓）。
fn svg_write_node(node: &SvgNode, out: &mut String, defs: &mut Vec<String>) {
    use yanshi_render::svg::fmt_num;
    match node {
        SvgNode::Path {
            d,
            fill,
            fill_opacity,
            blend,
        } => {
            out.push_str("<path d=\"");
            out.push_str(d);
            out.push_str("\" fill=\"");
            out.push_str(fill);
            out.push('"');
            if *fill_opacity < 0.9995 {
                out.push_str(" fill-opacity=\"");
                out.push_str(&fmt_num(*fill_opacity));
                out.push('"');
            }
            if let Some(blend) = blend {
                out.push_str(" style=\"mix-blend-mode:");
                out.push_str(blend);
                out.push('"');
            }
            out.push_str("/>");
        }
        SvgNode::Shape { markup } => out.push_str(markup),
        SvgNode::Image {
            x,
            y,
            w,
            h,
            png_base64,
            blend,
        } => {
            out.push_str("<image x=\"");
            out.push_str(&x.to_string());
            out.push_str("\" y=\"");
            out.push_str(&y.to_string());
            out.push_str("\" width=\"");
            out.push_str(&w.to_string());
            out.push_str("\" height=\"");
            out.push_str(&h.to_string());
            out.push_str("\" href=\"data:image/png;base64,");
            out.push_str(png_base64);
            out.push_str("\" xlink:href=\"data:image/png;base64,");
            out.push_str(png_base64);
            out.push('"');
            if let Some(blend) = blend {
                out.push_str(" style=\"mix-blend-mode:");
                out.push_str(blend);
                out.push('"');
            }
            out.push_str("/>");
        }
        SvgNode::Group {
            opacity,
            blend,
            children,
        } => {
            out.push_str("<g style=\"isolation:isolate");
            if let Some(blend) = blend {
                out.push_str(";mix-blend-mode:");
                out.push_str(blend);
            }
            out.push('"');
            if *opacity < 0.9995 {
                out.push_str(" opacity=\"");
                out.push_str(&fmt_num(*opacity));
                out.push('"');
            }
            out.push('>');
            for child in children {
                svg_write_node(child, out, defs);
            }
            out.push_str("</g>");
        }
        SvgNode::Masked {
            def,
            mask_id,
            children,
        } => {
            defs.push(def.clone());
            out.push_str("<g mask=\"url(#");
            out.push_str(mask_id);
            out.push_str(")\">");
            for child in children {
                svg_write_node(child, out, defs);
            }
            out.push_str("</g>");
        }
    }
}

/// `export_svg` 的计数账（**每种去向都有数 ⇒ 没有静默降级** ✓）。
#[derive(Default)]
struct SvgCounts {
    layers_total: u32,
    layers_exported: u32,
    layers_invisible: u32,
    layers_rasterized: u32,
    strokes_vector: u32,
    strokes_double_layered: u32,
    strokes_raster: u32,
    strokes_empty: u32,
    shapes_vector: u32,
    images_embedded: u32,
    erases_masked: u32,
    layers_collapsed: u32,
    objects_unsupported: u32,
    instances_unresolved: u32,
    validate_failed: u32,
    raster_reasons: std::collections::BTreeMap<&'static str, u32>,
}

impl SvgCounts {
    fn note_reason(&mut self, reason: &'static str) {
        *self.raster_reasons.entry(reason).or_insert(0) += 1;
    }
}

/// 把一段已渲染的 RGBA8 裁到 alpha 非零的紧凑包围盒、编码成一个 [`SvgNode::Image`]。
///
/// 区域原点 `(rx, ry)` 是这段像素在文档坐标里的左上角 ✓。
/// 整段全透明 ⇒ 返回 `None`（调用方已按对象计数，不会静默丢内容：
/// 全透明段本来就没有可见内容 ✓）。
fn svg_image_node(rx: i64, ry: i64, w: u32, h: u32, rgba: &[u8]) -> Option<SvgNode> {
    let (w_us, h_us) = (w as usize, h as usize);
    if rgba.len() < w_us * h_us * 4 {
        return None;
    }
    let (mut min_x, mut min_y) = (w, h);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if rgba[(y as usize * w_us + x as usize) * 4 + 3] > 0 {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if !any {
        return None;
    }
    let (cw, ch) = (max_x - min_x + 1, max_y - min_y + 1);
    let mut cropped = Vec::with_capacity((cw * ch * 4) as usize);
    for y in min_y..=max_y {
        let start = (y as usize * w_us + min_x as usize) * 4;
        cropped.extend_from_slice(&rgba[start..start + (cw as usize) * 4]);
    }
    let png = yanshi_render::png::encode_png(cw, ch, &cropped)?;
    Some(SvgNode::Image {
        x: rx + i64::from(min_x),
        y: ry + i64::from(min_y),
        w: cw,
        h: ch,
        png_base64: crate::base64::encode(&png),
        blend: None,
    })
}

/// **`export_svg`** ✓ —— 把文档导出成**矢量 + 位图混合 SVG** 落盘 ✓。
///
/// 路线乙（专家咨询定版 ✓）：通用光栅笔触走算法转矢量
/// （`yanshi_render::svg`：包络线/alpha 场轮廓 → RDP + Schneider 拟合 ✓）；
/// 转不动的内容（纹理/介质/软边/调整/液化等）按**紧凑包围盒 PNG** 回退，
/// 以原 z 序嵌回 ⇒ 同层其余笔仍是可编辑矢量 ✓。
/// 图层混合不可映射（add/subtract）或带蒙版/剪贴的图层整层回退 ✓。
/// 橡皮（硬边）走 SVG mask、不整层栅格化 ✓。
///
/// **账目**：每笔/每层的去向都在返回值里计数（`strokes_vector` /
/// `strokes_raster` + `raster_reasons` / `layers_rasterized` …）⇒
/// 「导出了但少东西」永远有数字可查、不静默 ✓。
/// `validate: true` 时逐笔跑边缘容忍度量验收，不过线的自动回退位图 ✓。
pub(super) fn write_export_svg(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    use yanshi_render::svg as svgx;
    let path = guarded_output_path(&require_str(args, "path")?)?;
    let validate = args
        .get("validate")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    let state = document.state().clone();
    let (canvas_w, canvas_h) = (state.width, state.height);
    if canvas_w == 0 || canvas_h == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("画布尺寸为 0 ⇒ 没有可导出的内容"),
        ));
    }
    let canvas = (canvas_w, canvas_h);
    let mut counts = SvgCounts::default();
    let mut mask_seq = 0usize;
    let mut root_children: Vec<SvgNode> = Vec::new();

    // 背景：`{r,g,b,a}` sRGB 字节（与 `new_document` 的落盘形状一致 ✓）⇒ 矩形。
    // alpha = 0 或形状不认 ⇒ 不画（透明画布）✓，不猜。
    if let (Some(r), Some(g), Some(b)) = (
        state.background.get("r").and_then(Value::as_u64),
        state.background.get("g").and_then(Value::as_u64),
        state.background.get("b").and_then(Value::as_u64),
    ) {
        let a = state
            .background
            .get("a")
            .and_then(Value::as_u64)
            .unwrap_or(255);
        if a > 0 {
            let hex = format!("#{:02x}{:02x}{:02x}", r.min(255), g.min(255), b.min(255));
            let mut markup =
                format!("<rect width=\"{canvas_w}\" height=\"{canvas_h}\" fill=\"{hex}\"");
            if a < 255 {
                markup.push_str(&format!(
                    " fill-opacity=\"{}\"",
                    svgx::fmt_num(a as f64 / 255.0)
                ));
            }
            markup.push_str("/>");
            root_children.push(SvgNode::Shape { markup });
        }
    }

    // 图层按 z 升序（= 绘制序，与合成一致 ✓）；同 z 按 id 稳定排序（确定性 ✓）。
    let mut layers: Vec<&yanshi_core::Layer> = state
        .layers
        .values()
        .filter(|layer| !layer.is_deleted())
        .collect();
    layers.sort_by(|a, b| (a.z_index, &a.id).cmp(&(b.z_index, &b.id)));
    counts.layers_total = layers.len() as u32;

    for layer in layers {
        if !layer.visible {
            counts.layers_invisible += 1;
            continue;
        }
        let full_region =
            yanshi_core::Bbox::new(0.0, 0.0, f64::from(canvas_w), f64::from(canvas_h));
        // 整层回退条件：混合不可映射 / 蒙版 / 剪贴 / 蒙版层本体 ✓。
        let layer_blend = svgx::blend_css_name(&layer.blend_mode);
        let whole_layer_raster = layer_blend.is_none()
            || layer.mask_id.is_some()
            || layer.clipping_mask
            || layer.layer_type == yanshi_core::LayerType::Mask;
        if whole_layer_raster {
            // 走子集路（keep = 本层全部对象）：它把背景置空 ⇒ 拿到透明底的
            // 图层本地像素；`render_region_raw_layer` 会合成到文档背景上、
            // 段图变白板（同 `render_layer_objects_subset` 的注释 ✓）。
            let keep_all: std::collections::BTreeSet<yanshi_core::ObjectId> = state
                .objects_in_layer(&layer.id)
                .iter()
                .map(|object| object.id.clone())
                .collect();
            let (w, h, rgba) = ctx.workspace.render_layer_objects_subset(
                &ctx.doc_id,
                full_region,
                &layer.id,
                &keep_all,
            )?;
            if let Some(node) = svg_image_node(0, 0, w, h, &rgba) {
                counts.images_embedded += 1;
                counts.layers_rasterized += 1;
                counts.layers_exported += 1;
                root_children.push(SvgNode::Group {
                    opacity: layer.opacity,
                    blend: None,
                    children: vec![node],
                });
            } else {
                counts.layers_rasterized += 1;
            }
            continue;
        }
        let layer_blend_css = layer_blend.filter(|name| *name != "normal");

        let objects = state.objects_in_layer(&layer.id);
        let mut nodes: Vec<SvgNode> = Vec::new();
        let mut pending: Vec<usize> = Vec::new(); // objects 下标（位图段 ✓）

        // 把 pending 段渲染成一张紧凑图插进 nodes（闭包借用太绕 ⇒ 内联小函数式宏替代：
        // 这里直接写一段带标签的块，由调用点以变量驱动）。
        macro_rules! flush_pending {
            () => {
                if !pending.is_empty() {
                    let keep: std::collections::BTreeSet<yanshi_core::ObjectId> = pending
                        .iter()
                        .map(|&i| objects[i].id.clone())
                        .collect();
                    // 段包围盒：各对象 bbox 并集外扩 2px；任一对象没 bbox ⇒ 整幅 ✓。
                    let mut region = full_region;
                    let mut acc: Option<yanshi_core::Bbox> = None;
                    for &i in &pending {
                        match yanshi_render::object::object_bbox_in(&state, objects[i]) {
                            Some(b) => {
                                acc = Some(match acc {
                                    None => b,
                                    Some(a) => yanshi_core::Bbox::new(
                                        a.x.min(b.x),
                                        a.y.min(b.y),
                                        (a.x + a.w).max(b.x + b.w) - a.x.min(b.x),
                                        (a.y + a.h).max(b.y + b.h) - a.y.min(b.y),
                                    ),
                                });
                            }
                            None => {
                                acc = None;
                                break;
                            }
                        }
                    }
                    if let Some(b) = acc {
                        let x0 = (b.x - 2.0).floor().max(0.0);
                        let y0 = (b.y - 2.0).floor().max(0.0);
                        let x1 = (b.x + b.w + 2.0).ceil().min(f64::from(canvas_w));
                        let y1 = (b.y + b.h + 2.0).ceil().min(f64::from(canvas_h));
                        region = yanshi_core::Bbox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0));
                    }
                    let (w, h, rgba) = ctx.workspace.render_layer_objects_subset(
                        &ctx.doc_id,
                        region,
                        &layer.id,
                        &keep,
                    )?;
                    if let Some(node) = svg_image_node(region.x as i64, region.y as i64, w, h, &rgba) {
                        counts.images_embedded += 1;
                        nodes.push(node);
                    }
                    pending.clear();
                }
            };
        }
        // 整层效应对象（调整/滤镜/液化/非擦除修图，或软橡皮）：把**到它为止的
        // 全部对象**（含已发出的矢量笔）一起栅格化 ⇒ 语义与引擎一致 ✓。
        macro_rules! collapse_through {
            ($upto:expr) => {{
                let upto: usize = $upto;
                let keep: std::collections::BTreeSet<yanshi_core::ObjectId> =
                    objects[..=upto].iter().map(|o| o.id.clone()).collect();
                let (w, h, rgba) = ctx.workspace.render_layer_objects_subset(
                    &ctx.doc_id,
                    full_region,
                    &layer.id,
                    &keep,
                )?;
                nodes.clear();
                pending.clear();
                if let Some(node) = svg_image_node(0, 0, w, h, &rgba) {
                    counts.images_embedded += 1;
                    nodes.push(node);
                }
                counts.layers_collapsed += 1;
            }};
        }

        for (index, object) in objects.iter().enumerate() {
            if !object.visible {
                continue;
            }
            // 原语解析与变换：与渲染器同一条路（实例先解析 master ✓）。
            let primitive = if object.object_type == yanshi_core::ObjectType::Instance {
                match yanshi_render::object::resolve_instance(&state, object) {
                    Some((master, transform)) => {
                        yanshi_render::object::transform_primitive(master, &transform)
                    }
                    None => {
                        counts.instances_unresolved += 1;
                        continue;
                    }
                }
            } else {
                yanshi_render::object::transform_primitive(
                    yanshi_render::object::parse_object(object),
                    &object.transform,
                )
            };
            use yanshi_render::object::Primitive as Prim;
            match primitive {
                Prim::Stroke { geometry, brush } => {
                    // 对象不透明度折进笔刷（与渲染器逐字一致 ✓）。
                    let object_opacity = object
                        .data
                        .get("opacity")
                        .and_then(Value::as_f64)
                        .unwrap_or(1.0);
                    let brush = yanshi_render::brush::BrushSpec {
                        opacity: brush.opacity * object_opacity,
                        ..brush
                    };
                    let appearance =
                        yanshi_render::brush::StrokeAppearance::from_data(&object.data);
                    // 选区裁剪过的笔 ⇒ 覆盖度调制、路径表达不了 ⇒ 位图段 ✓。
                    if yanshi_render::render::object_clip(&state, &layer.id, object).is_some() {
                        counts.strokes_raster += 1;
                        counts.note_reason("selection_clip");
                        pending.push(index);
                        continue;
                    }
                    match svgx::export_stroke(&brush, &geometry, &appearance, canvas) {
                        svgx::StrokeExport::Raster { reason } => {
                            counts.strokes_raster += 1;
                            counts.note_reason(reason);
                            pending.push(index);
                        }
                        svgx::StrokeExport::Vector {
                            layers: export_layers,
                            css_blend,
                        } => {
                            if export_layers.is_empty() {
                                counts.strokes_empty += 1;
                                continue;
                            }
                            if validate {
                                let refs: Vec<(&[svgx::FittedContour], f64)> = export_layers
                                    .iter()
                                    .map(|l| (l.contours.as_slice(), l.fill_opacity))
                                    .collect();
                                let failed = svgx::validate_stroke(
                                    &brush,
                                    &geometry,
                                    Some(&appearance),
                                    canvas,
                                    &refs,
                                )
                                .is_some_and(|metrics| !metrics.passes());
                                if failed {
                                    counts.strokes_raster += 1;
                                    counts.validate_failed += 1;
                                    counts.note_reason("validate_failed");
                                    pending.push(index);
                                    continue;
                                }
                            }
                            flush_pending!();
                            let fill =
                                svgx::srgb_hex([brush.color[0], brush.color[1], brush.color[2]]);
                            if export_layers.len() > 1 {
                                counts.strokes_double_layered += 1;
                            }
                            for export_layer in &export_layers {
                                nodes.push(SvgNode::Path {
                                    d: svgx::contours_path_data(&export_layer.contours),
                                    fill: fill.clone(),
                                    fill_opacity: export_layer.fill_opacity,
                                    blend: css_blend.filter(|name| *name != "normal"),
                                });
                            }
                            counts.strokes_vector += 1;
                        }
                    }
                }
                Prim::Shape {
                    kind,
                    bbox,
                    points,
                    color,
                    stroke_width,
                    stroke_color,
                    feather,
                } => {
                    if feather > 0.0 {
                        pending.push(index);
                        continue;
                    }
                    flush_pending!();
                    let fill = svgx::srgb_hex([color[0], color[1], color[2]]);
                    let mut attrs = format!(" fill=\"{fill}\"");
                    if color[3] < 0.9995 {
                        attrs.push_str(&format!(
                            " fill-opacity=\"{}\"",
                            svgx::fmt_num(f64::from(color[3]))
                        ));
                    }
                    if stroke_width > 0.0 {
                        if let Some(sc) = stroke_color {
                            attrs.push_str(&format!(
                                " stroke=\"{}\" stroke-width=\"{}\"",
                                svgx::srgb_hex([sc[0], sc[1], sc[2]]),
                                svgx::fmt_num(stroke_width)
                            ));
                            if sc[3] < 0.9995 {
                                attrs.push_str(&format!(
                                    " stroke-opacity=\"{}\"",
                                    svgx::fmt_num(f64::from(sc[3]))
                                ));
                            }
                        }
                    }
                    let markup = match kind {
                        yanshi_render::object::ShapeKind::Rect => format!(
                            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{attrs}/>",
                            svgx::fmt_num(bbox.x),
                            svgx::fmt_num(bbox.y),
                            svgx::fmt_num(bbox.w),
                            svgx::fmt_num(bbox.h)
                        ),
                        yanshi_render::object::ShapeKind::Ellipse => format!(
                            "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\"{attrs}/>",
                            svgx::fmt_num(bbox.x + bbox.w / 2.0),
                            svgx::fmt_num(bbox.y + bbox.h / 2.0),
                            svgx::fmt_num(bbox.w / 2.0),
                            svgx::fmt_num(bbox.h / 2.0)
                        ),
                        yanshi_render::object::ShapeKind::Polygon => {
                            let mut d = String::new();
                            for (i, (x, y)) in points.iter().enumerate() {
                                if i == 0 {
                                    d.push_str(&format!(
                                        "M {} {}",
                                        svgx::fmt_num(*x),
                                        svgx::fmt_num(*y)
                                    ));
                                } else {
                                    d.push_str(&format!(
                                        " L {} {}",
                                        svgx::fmt_num(*x),
                                        svgx::fmt_num(*y)
                                    ));
                                }
                            }
                            d.push_str(" z");
                            format!("<path d=\"{d}\"{attrs}/>")
                        }
                    };
                    nodes.push(SvgNode::Shape { markup });
                    counts.shapes_vector += 1;
                }
                Prim::Retouch {
                    kind,
                    points,
                    size,
                    hardness,
                    opacity,
                    ..
                } if kind == "erase" => {
                    // 硬边橡皮 + 此前内容已全部发出（无待处理位图段）⇒ mask ✓；
                    // 否则整层塌缩栅格化（橡皮作用于其下全部内容 ✓）。
                    let fringe = (size.max(1.0) / 2.0) * (1.0 - hardness.clamp(0.0, 1.0));
                    let hard = hardness >= 0.75 || fringe <= 2.0;
                    if hard && pending.is_empty() && !nodes.is_empty() {
                        match svgx::erase_mask_contours(&points, size, hardness, opacity, canvas) {
                            Some((contours, strength)) => {
                                mask_seq += 1;
                                let mask_id = format!("m{mask_seq}");
                                let d = svgx::contours_path_data(&contours);
                                let def = format!(
                                    "<mask id=\"{mask_id}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{canvas_w}\" height=\"{canvas_h}\"><rect width=\"{canvas_w}\" height=\"{canvas_h}\" fill=\"#fff\"/><path d=\"{d}\" fill=\"#000\" fill-opacity=\"{}\"/></mask>",
                                    svgx::fmt_num(strength)
                                );
                                let children = std::mem::take(&mut nodes);
                                nodes.push(SvgNode::Masked {
                                    def,
                                    mask_id,
                                    children,
                                });
                                counts.erases_masked += 1;
                            }
                            None => {
                                collapse_through!(index);
                            }
                        }
                    } else if !nodes.is_empty() || !pending.is_empty() {
                        collapse_through!(index);
                    }
                    // 此前无内容时橡皮无对象可擦 ⇒ 跳过（与渲染一致 ✓）。
                }
                Prim::Adjustment { .. }
                | Prim::Filter { .. }
                | Prim::Liquify { .. }
                | Prim::Retouch { .. } => {
                    collapse_through!(index);
                }
                Prim::RasterPatch { .. } | Prim::Text { .. } => {
                    pending.push(index);
                }
                Prim::Unsupported { .. } => {
                    counts.objects_unsupported += 1;
                    pending.push(index);
                }
            }
        }
        flush_pending!();
        if !nodes.is_empty() {
            counts.layers_exported += 1;
            root_children.push(SvgNode::Group {
                opacity: layer.opacity,
                blend: layer_blend_css,
                children: nodes,
            });
        }
    }

    let mut defs: Vec<String> = Vec::new();
    let mut body = String::new();
    for node in &root_children {
        svg_write_node(node, &mut body, &mut defs);
    }
    let mut svg = String::with_capacity(body.len() + 512);
    svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{canvas_w}\" height=\"{canvas_h}\" viewBox=\"0 0 {canvas_w} {canvas_h}\">"
    ));
    if !defs.is_empty() {
        svg.push_str("<defs>");
        for def in &defs {
            svg.push_str(def);
        }
        svg.push_str("</defs>");
    }
    svg.push_str(&body);
    svg.push_str("</svg>\n");
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("建输出目录失败：{} ⇒ {error}", parent.display())),
                )
            })?;
        }
    }
    std::fs::write(&path, &svg).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("写文件失败：{} ⇒ {error}", path.display())),
        )
    })?;
    let render_warnings = ctx.workspace.last_render_warnings(&ctx.doc_id);
    Ok(json!({
        "ok": true,
        "path": path.display().to_string(),
        "bytes": svg.len(),
        "width": canvas_w,
        "height": canvas_h,
        "validated": validate,
        "layers_total": counts.layers_total,
        "layers_exported": counts.layers_exported,
        "layers_invisible_skipped": counts.layers_invisible,
        "layers_rasterized": counts.layers_rasterized,
        "layers_collapsed": counts.layers_collapsed,
        "strokes_vector": counts.strokes_vector,
        "strokes_double_layered": counts.strokes_double_layered,
        "strokes_raster": counts.strokes_raster,
        "strokes_empty": counts.strokes_empty,
        "raster_reasons": counts.raster_reasons,
        "shapes_vector": counts.shapes_vector,
        "images_embedded": counts.images_embedded,
        "erases_masked": counts.erases_masked,
        "objects_unsupported": counts.objects_unsupported,
        "instances_unresolved": counts.instances_unresolved,
        "validate_failed": counts.validate_failed,
        "warnings": render_warnings,
    }))
}
