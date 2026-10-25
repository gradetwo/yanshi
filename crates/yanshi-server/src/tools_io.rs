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
    // **导出失败删目标文件**（P2-12）：守卫在成功返回前 `disarm`，
    // 任何 `Err` 返回都删掉该路径上的文件（旧的也不留 —— 旧文件会冒充新产物）。
    let mut delete_on_failure = DeleteTargetOnFailure::new(path.clone());
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
        // **键里必须有 `doc_id`** ✓（B2 的配套修正 ✓）：旧键只有版本／`head_seq`／
        // `include_bitmaps` ⇒ 两份不同文档只要原子数相同就撞键 ✓，后导出的那份
        // 会拿到前一份的包 ✓ —— 缺本体的重导入文档会直接命中健康包的缓存，
        // B2 的失败永远到不了调用方 ✓。`head_seq` 只标识日志长度，不标识内容 ✓。
        doc_id.clone(),
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
        // **B2 的两栏** ✓：本体缺（`missing_body`）与派生缺（`missing_derived`）分开数 ✓；
        // 成功回执里 `missing_body` 恒为 0 ✓（`> 0` 时上面已直接失败 ✓）。
        "missing_body": stats.missing_body,
        "missing_derived": stats.missing_derived,
    });
    if let Some(path) = &path {
        value["path"] = json!(path);
    }
    delete_on_failure.disarm();
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
    // **导出失败删目标文件**（P2-12）：见 `write_export_project` 的守卫说明。
    let mut delete_on_failure = DeleteTargetOnFailure::new(Some(path.clone()));
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
    // **期 4 `lod`**：取样档位（0 = 全分辨率；1/2/3 ⇒ 输出尺寸逐档减半）。
    // 非整数／超出 0..=3 ⇒ `invalid_argument`（不静默钳制；`optional_count`
    // 本来就把负数／小数报成 `invalid_argument`，整数值浮点按既有口径接受）。
    let lod = match optional_count(args, "lod")? {
        None => 0,
        Some(value) if value <= 3 => value as u32,
        Some(value) => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "lod 必须是 0..=3 的整数（收到 {value}）⇒ 0 = 全分辨率，1/2/3 逐档减半"
                )),
            ));
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
        Some(layer_id) => {
            ctx.workspace
                .render_region_raw_layer_with_lod(&ctx.doc_id, region, &layer_id, lod)?
        }
        None => ctx
            .workspace
            .render_region_raw_with_lod(&ctx.doc_id, region, lod)?,
    };
    ctx.time(Phase::Render, render_started);
    // **落盘的图也必须把"被跳过的东西"带出去** ✗：这条路同样返回裸像素 ✓，
    // 缺块时**不能**默默写一张不完整的 PNG 却不提 ✓。
    let render_warnings = ctx.workspace.last_render_warnings(&ctx.doc_id);
    // **缺本体 ⇒ 导出失败、不产图**（P2-11：`export_png` 纳入 B2）。
    //
    // **定夺**（期 2 块 3）：`export_png` 与 `export_project` /
    // `export_timelapse` 同列 —— 缺本体 blob 时不产文件（期 1 B2 已把
    // 另两处改成失败；`export_png` 是期 1 说明 §4 的已知缺口，本期补上）。
    // 只缺派生 tile 时渲染落回整幅路、画面完整 ⇒ 照常出图（告警已在回执里）。
    // 判定用结构化缺失哈希 + canonical 分类（`crate::verify`），
    // **不解析告警文本**（人话会变，哈希不会）；认不出的哈希按本体处理。
    let missing_blobs = ctx.workspace.last_missing_blobs(&ctx.doc_id);
    if !missing_blobs.is_empty() {
        let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
            )
        })?;
        let mut classes: std::collections::BTreeMap<
            String,
            std::collections::BTreeSet<crate::verify::RefClass>,
        > = std::collections::BTreeMap::new();
        for atom in document.log().atoms() {
            let refs: Vec<String> = atom
                .refs
                .blobs
                .iter()
                .map(|hash| hash.to_string())
                .collect();
            crate::verify::collect_atom_classes(&atom.payload, &refs, &mut classes);
        }
        let missing_body: Vec<String> = missing_blobs
            .iter()
            .filter(|hash| match crate::verify::hash_from_str(hash) {
                Some(bare) => {
                    classes
                        .get(&bare)
                        .map(crate::verify::resolve_class)
                        .unwrap_or(crate::verify::RefClass::Body)
                        == crate::verify::RefClass::Body
                }
                // 认不出的哈希 ⇒ 按本体处理（fail-safe：宁可不产图，不可放过）。
                None => true,
            })
            .cloned()
            .collect();
        if !missing_body.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "缺本体 blob {} 个（首个 {}）⇒ 这张 PNG 会缺一块，不产图；只缺派生 tile 时照常出图",
                    missing_body.len(),
                    missing_body[0]
                )),
            ));
        }
    }
    if width == 0 || height == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("区域为空 ⇒ 没有可导出的像素"),
        ));
    }
    // **目标尺寸** ✓：显式宽高 ⇒ 用它；`max_edge` ⇒ 等比缩到最长边；都没给 ⇒ 原尺寸 ✓。
    let asked_width = optional_count(args, "width")?.map(|value| value as u32);
    let asked_height = optional_count(args, "height")?.map(|value| value as u32);
    let max_edge = optional_count(args, "max_edge")?.map(|value| value as u32);
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
    if covers_frame && lod == 0 && target_w == width && target_h == height {
        // 只在**原尺寸整幅**时写缓存 ✓（缩放过的图不该当成"打开即图片" ✗；
        // 期 4：`lod > 0` 的图是降采样的，同样不许当成全幅缓存 ✗）。
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
    delete_on_failure.disarm();
    Ok(json!({
        "path": path,
        "width": target_w,
        "height": target_h,
        "bytes": png.len(),
        // **`sha256` 让调用方能判"两次导出是否逐字节相同"** ✓（第 1542 轮；外部 `E1613` 的做法）
        //   **⇒ 他们的原话**：「我在 `E1613` 用的检验是**两次输出的 `sha256` 是否相同**」
        //     ⇒ **它立刻证明"改动一个像素都没生效"**，而**指标（local/edge/std）没有任何提示** ✓
        //   **⇒ ⇒ 所以**：**"确定性 + 哈希"是比"统计量"更强的"改动是否生效"判据** ✓
        //     ⇒ 而在此之前他们**只能自己读文件再算** ⇒ 现在**响应里直接给** ✓
        //   **⇒ ⇒ ⇒ 纯增量**：只多一个字段，**不改任何既有行为** ✓
        // **用 core 已有的 `BlobHash::from_bytes`** ✓（它算的就是**同一套 SHA-256**）⇒
        //   **不引入新依赖** ✓（`yanshi-server` 里没有 `sha2`，而 core 有 ✓）
        "sha256": yanshi_core::BlobHash::from_bytes(&png).to_string(),
        "source_width": width,
        "source_height": height,
        // **期 4 `lod`**：回显生效档位（缺省 0）；`scaled` =（目标尺寸 ≠ 渲染尺寸）
        // 或（`lod > 0`——输出本来就是降采样的像素）。
        "lod": lod,
        "scaled": target_w != width || target_h != height || lod > 0,
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
    /// 原生矢量图层的笔触直出数（路线甲 ✓；与事后转矢量的 `strokes_vector` 分开记账）。
    strokes_vector_native: u32,
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
    /// `flatten_opacity` 下**累积压平成功**的笔触数（2026-10-21 Q1 ✓）。
    strokes_flattened: u32,
    /// `flatten_opacity` 下压平成功的形状数（2026-10-21 Q1 ✓）。
    shapes_flattened: u32,
    /// backdrop 异质（通道跨度 > 4/255）⇒ 拒绝单色压平、回退位图的对象数
    /// （笔触+形状合计 ✓；理由另见 `raster_reasons` 的
    /// `flatten_heterogeneous_backdrop` ✓）。
    flatten_heterogeneous_raster: u32,
    /// 想压平但资格不成立（图层组非 plain／backdrop 不可信／取不到）的
    /// 对象数 ⇒ 保留 `fill-opacity` 形式发出、**不**用近似色静默糊 ✓。
    flatten_skipped: u32,
    /// **半透明描边压不了**（描边骑在边缘上 ⇒ 两个 backdrop ⇒ 压成单色必错一半 ⇒ 不压、只计数）
    flatten_stroke_skipped: u32,
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

/// sRGB 字节像素的 source-over（在 linear 空间合成、再编码回字节 ✓）。
///
/// `fg`/`bg` 都是直通（非预乘）RGBA8；返回值同格式。引擎在 linear f32
/// 里合成 ⇒ 这里除了每步的 u8 量化（≤1/255）外与引擎同式 ✓。
fn srgb_over_px(fg: [u8; 4], bg: [u8; 4]) -> [u8; 4] {
    let fa = f32::from(fg[3]) / 255.0;
    let ba = f32::from(bg[3]) / 255.0;
    let out_a = fa + ba * (1.0 - fa);
    if out_a <= 1e-6 {
        return [0, 0, 0, 0];
    }
    let mut out = [0u8; 4];
    for ch in 0..3 {
        let f = yanshi_render::color::srgb_to_linear(f32::from(fg[ch]) / 255.0) * fa;
        let b = yanshi_render::color::srgb_to_linear(f32::from(bg[ch]) / 255.0) * (1.0 - fa);
        let mixed = (f + b) / out_a;
        out[ch] = (yanshi_render::color::linear_to_srgb(mixed) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_a * 255.0).round() as u8;
    out
}

/// 把一块图层本地像素（透明底 RGBA8、原点在文档坐标 `origin` ✓）以
/// `opacity` 盖到整幅 backdrop 上（`export_svg` 的 below 追踪用 ✓）。
fn composite_layer_into_below(
    below: &mut [[u8; 4]],
    canvas: (usize, usize),
    origin: (i64, i64),
    img_w: usize,
    img_h: usize,
    rgba: &[u8],
    opacity: f32,
) {
    for row in 0..img_h {
        let dy = origin.1 + row as i64;
        if dy < 0 || dy >= canvas.1 as i64 {
            continue;
        }
        for col in 0..img_w {
            let dx = origin.0 + col as i64;
            if dx < 0 || dx >= canvas.0 as i64 {
                continue;
            }
            let src = (row * img_w + col) * 4;
            if src + 4 > rgba.len() {
                return;
            }
            let mut fg = [rgba[src], rgba[src + 1], rgba[src + 2], rgba[src + 3]];
            if fg[3] == 0 {
                continue;
            }
            fg[3] = (f32::from(fg[3]) * opacity).round().clamp(0.0, 255.0) as u8;
            let idx = dy as usize * canvas.0 + dx as usize;
            below[idx] = srgb_over_px(fg, below[idx]);
        }
    }
}

/// `flatten_opacity` 的 backdrop 评估结论（2026-10-21 Q1 ✓）。
enum BackdropAssessment {
    /// 均质（支撑集下方各通道跨度 ≤ 4/255 ✓）：linear 空间的均值色。
    Homogeneous([f32; 3]),
    /// 异质 ⇒ 不许单色压平（调用方回退位图并计数 ✓）。
    Heterogeneous,
    /// 取不到（子集渲染失败／无有效支撑像素）⇒ 调用方不压平并计数 ✓。
    Unavailable,
}

/// 取某个压平候选**下方**的真实 backdrop 并判均质 ✓。
///
/// 做法：把候选之前的同层对象（`prior`）在 `region` 内做一次子集渲染
/// （生产渲染器 ⇒ 含橡皮/混合/实例的真实前缀、不是近似 ✗），逐支撑像素
/// 与已追踪的下层合成结果 `below` 做 over，再统计通道跨度与 linear 均值。
/// 跨度按**合成后**的 sRGB 字节算：≤ 4 视为均质（压平误差 ≤ ~2 通道、
/// 与引擎的 AA 边缘误差同量级 ✓）。
#[allow(clippy::too_many_arguments)]
fn assess_backdrop(
    ctx: &mut ToolContext<'_>,
    layer_id: &str,
    prior: &std::collections::BTreeSet<yanshi_core::ObjectId>,
    region: yanshi_core::Bbox,
    support: &[(i64, i64)],
    below: &[[u8; 4]],
    canvas: (usize, usize),
) -> BackdropAssessment {
    let (img_w, img_h, rgba) =
        match ctx
            .workspace
            .render_layer_objects_subset(&ctx.doc_id, region, layer_id, prior)
        {
            Ok(img) => img,
            Err(_) => return BackdropAssessment::Unavailable,
        };
    let (rx, ry) = (region.x as i64, region.y as i64);
    let mut sum = [0.0f32; 3];
    let mut count = 0usize;
    let mut mins = [255u8; 3];
    let mut maxs = [0u8; 3];
    for &(x, y) in support {
        let (lx, ly) = (x - rx, y - ry);
        if lx < 0 || ly < 0 || lx >= img_w as i64 || ly >= img_h as i64 {
            continue;
        }
        if x < 0 || y < 0 || x >= canvas.0 as i64 || y >= canvas.1 as i64 {
            continue;
        }
        let src = ((ly as usize) * (img_w as usize) + (lx as usize)) * 4;
        if src + 4 > rgba.len() {
            continue;
        }
        let fg = [rgba[src], rgba[src + 1], rgba[src + 2], rgba[src + 3]];
        let px = srgb_over_px(fg, below[(y as usize) * canvas.0 + (x as usize)]);
        count += 1;
        for ch in 0..3 {
            sum[ch] += yanshi_render::color::srgb_to_linear(f32::from(px[ch]) / 255.0);
            mins[ch] = mins[ch].min(px[ch]);
            maxs[ch] = maxs[ch].max(px[ch]);
        }
    }
    if count == 0 {
        return BackdropAssessment::Unavailable;
    }
    let spread = (0..3)
        .map(|ch| f64::from(maxs[ch] - mins[ch]))
        .fold(0.0f64, f64::max);
    if spread > 4.0 {
        return BackdropAssessment::Heterogeneous;
    }
    BackdropAssessment::Homogeneous([
        sum[0] / count as f32,
        sum[1] / count as f32,
        sum[2] / count as f32,
    ])
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
/// **矢量图层**（路线甲 Phase 2）的笔触走原生直出（`vector::export_stroke_native`：
/// 解析包络、无光栅化中间步），单列 `strokes_vector_native` 与事后转矢量分开记账 ✓。
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
    // **`flatten_opacity`（第 1409 轮）** ✓：把半透明路径**在线性空间压平到 backdrop**再 emit 不透明色
    //   ⇒ 与引擎的合成**逐像素一致**（否则差最多 73 通道，见 check-svg-opacity-colour-space.mjs）
    //   **⇒ 只有背景**不透明**时才压平**：压平到一个透明背景没有意义（会得到偏黑的颜色 ✗）
    //   ⇒ 所以透明画布 + `flatten_opacity: true` ⇒ **不压平**，并在返回值里
    //     用 `flatten_active: false` 如实报告（2026-10-21 起有这个字段 ✓；
    //     此前注释承诺的 `flattened` 字段从未存在 ✗、已随 Q1 一并纠正 ✓）
    let flatten_requested = args
        .get("flatten_opacity")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let background_bytes: Option<(u64, u64, u64, u64)> = match (
        state.background.get("r").and_then(Value::as_u64),
        state.background.get("g").and_then(Value::as_u64),
        state.background.get("b").and_then(Value::as_u64),
    ) {
        (Some(r), Some(g), Some(b)) => Some((
            r.min(255),
            g.min(255),
            b.min(255),
            state
                .background
                .get("a")
                .and_then(Value::as_u64)
                .unwrap_or(255),
        )),
        _ => None,
    };
    let flatten_active = flatten_requested && background_bytes.is_some_and(|(_, _, _, a)| a == 255);
    let flatten_opacity = flatten_active;
    // 有半透明描边没能压平 ⇒ 结尾加一句 warning ✓
    let mut flatten_stroke_warned = false;
    // **真实 backdrop 追踪（2026-10-21 Q1 ✓）**：`below` = 已处理完的图层
    // 逐层 over 后的整幅合成（sRGB 字节、初值 = 文档背景 ✓）。压平的
    // backdrop **不再**无条件用文档背景字段 ✗ —— 那只对「单层、笔下无物」
    // 成立；多层/垫底对象时要用 `assess_backdrop` 取实测合成 ✓。
    // 任一已处理图层的合成没法用 plain over 镜像（组带 CSS 混合等）⇒
    // `backdrop_exact = false` ⇒ 此后只计数 `flatten_skipped`、不再压平，
    // 绝不用近似 backdrop 静默糊 ✗。
    let mut below: Vec<[u8; 4]> = vec![
        background_bytes.map_or([0, 0, 0, 0], |(r, g, b, a)| {
            [r as u8, g as u8, b as u8, a as u8]
        });
        (canvas_w as usize) * (canvas_h as usize)
    ];
    let mut backdrop_exact = true;
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
                // backdrop 追踪：整层图就是该层的完整像素 ⇒ 合成可 plain
                // 镜像时（normal 混合、无蒙版/剪贴/蒙版层本体）并入 below ✓；
                // 否则 below 不可信 ⇒ 此后停止压平资格（不静默近似 ✗）。
                if flatten_active && backdrop_exact {
                    let plain = layer_blend == Some("normal")
                        && layer.mask_id.is_none()
                        && !layer.clipping_mask
                        && layer.layer_type != yanshi_core::LayerType::Mask;
                    if plain {
                        composite_layer_into_below(
                            &mut below,
                            (canvas_w as usize, canvas_h as usize),
                            (0, 0),
                            w as usize,
                            h as usize,
                            &rgba,
                            layer.opacity as f32,
                        );
                    } else {
                        backdrop_exact = false;
                    }
                }
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
                    // 对象 `opacity` **不在这里折** ✗：`brush` 已经由
                    // `BrushSpec::from_value(data)` 读过 `data.opacity`，
                    // 渲染器对象通路同理（见 render.rs 的 Stroke 分支注释 ✓）。
                    // 曾经两边都再乘一次 ⇒ 导出场与 PNG 一起按 opacity² 变淡、
                    // 只有公式（核心路径校准）是对的 ⇒ 三者互相矛盾 ✗。
                    let appearance =
                        yanshi_render::brush::StrokeAppearance::from_data(&object.data);
                    // 选区裁剪过的笔 ⇒ 覆盖度调制、路径表达不了 ⇒ 位图段 ✓。
                    if yanshi_render::render::object_clip(&state, &layer.id, object).is_some() {
                        counts.strokes_raster += 1;
                        counts.note_reason("selection_clip");
                        pending.push(index);
                        continue;
                    }
                    // **路线甲**：矢量图层的笔触走原生直出（解析包络、不光栅化、
                    // 不做事后拟合 ✓），计数单列 `strokes_vector_native`；
                    // 其余图层仍走路线乙的事后转换 ✓。原生路表达不了的
                    // （软边/自交/抖动…）同样回退位图段、理由是 `native_*` 短码 ✓。
                    let native_layer = layer.layer_type == yanshi_core::LayerType::Vector;
                    let exported = if native_layer {
                        yanshi_render::vector::export_stroke_native(
                            &brush,
                            &geometry,
                            &appearance,
                            canvas,
                        )
                    } else {
                        svgx::export_stroke(&brush, &geometry, &appearance, canvas)
                    };
                    match exported {
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
                                let metrics = svgx::validate_stroke(
                                    &brush,
                                    &geometry,
                                    Some(&appearance),
                                    canvas,
                                    &refs,
                                );
                                let failed =
                                    metrics.as_ref().is_some_and(|metrics| !metrics.passes());
                                if failed {
                                    counts.strokes_raster += 1;
                                    counts.validate_failed += 1;
                                    counts.note_reason("validate_failed");
                                    // **★ 探针：回退时把**为什么**打出来** ✓（第 1439 轮）
                                    //   **⇒ 为什么需要** ✗：回退目前只报**计数** ⇒ 调用方（和我）
                                    //     只知道"有 N 笔回退"，**不知道是哪条指标不过** ✗
                                    //     ⇒ 而"要不要放宽"是个需要**具体数字**的决定 ✓
                                    //   **⇒ 门控**：只在 `YANSHI_SVG_VALIDATE_PROBE=1` 时打印 ✓
                                    //     （生产路零开销 ✓；与 `YANSHI_RENDER_PROBE` 同例 ✓）
                                    if std::env::var("YANSHI_SVG_VALIDATE_PROBE").as_deref()
                                        == Ok("1")
                                    {
                                        if let Some(m) = &metrics {
                                            eprintln!(
                                                "[svg-validate] 回退：e_interior={:.4}（线 0.12）\
                                                 ｜e_geom={:.4}（线 0.15）｜normal_p95={:.4}（线 {:.2}）\
                                                 ｜radius={:.2}｜e_support={:.4}",
                                                m.e_interior,
                                                m.e_geom,
                                                m.normal_p95,
                                                (0.08 * m.radius).max(0.35),
                                                m.radius,
                                                m.e_support
                                            );
                                        }
                                    }
                                    pending.push(index);
                                    continue;
                                }
                            }
                            flush_pending!();
                            let fill_linear = [brush.color[0], brush.color[1], brush.color[2]];
                            let fill = svgx::srgb_hex(fill_linear);
                            if export_layers.len() > 1 {
                                counts.strokes_double_layered += 1;
                            }
                            // **`flatten_opacity` 的压平资格与 backdrop 评估（2026-10-21 Q1 ✓）**：
                            // 旧实现逐层各自对**文档背景字段**压平 ⇒ 第 2 层 emit
                            // 不透明后把第 1 层的贡献整片遮掉（docE 笔触中心实测
                            // 差 20 通道 ✓；形状只有一层 ⇒ 旧实现对形状恰好精确、
                            // 这也是它当时测不出的原因 ✓）。新实现走**累积压平**
                            // （见下面发射循环 ✓），backdrop 由 `assess_backdrop`
                            // 实测（本层前缀子集渲染 over 已追踪的下层合成 ✓）；
                            // 异质（跨度 > 4/255）⇒ **不许单色糊** ⇒ 整笔回退
                            // 位图并计数 ✓；组非 plain / 笔带混合 / backdrop
                            // 不可信 ⇒ 不压平、保留 `fill-opacity` 形式并计数 ✓。
                            let mut flat_backdrop: Option<[f32; 3]> = None;
                            let wants_flatten = flatten_opacity
                                && export_layers.iter().any(|l| l.fill_opacity < 0.9995);
                            if wants_flatten {
                                if layer.opacity < 0.9995
                                    || layer_blend_css.is_some()
                                    || css_blend.is_some_and(|name| name != "normal")
                                    || !backdrop_exact
                                {
                                    counts.flatten_skipped += 1;
                                } else {
                                    let assessment = (|| {
                                        let Some((buffer, (x0, y0, bw, bh))) =
                                            svgx::render_stroke_buffer(
                                                &brush,
                                                &geometry,
                                                Some(&appearance),
                                                canvas,
                                            )
                                        else {
                                            return BackdropAssessment::Unavailable;
                                        };
                                        let mut support: Vec<(i64, i64)> = Vec::new();
                                        for yy in 0..bh {
                                            for xx in 0..bw {
                                                if buffer.pixel(xx, yy)[3] > 0.05 {
                                                    support.push((
                                                        x0 + i64::from(xx),
                                                        y0 + i64::from(yy),
                                                    ));
                                                }
                                            }
                                        }
                                        if support.is_empty() {
                                            return BackdropAssessment::Unavailable;
                                        }
                                        let region = yanshi_core::Bbox::new(
                                            x0 as f64,
                                            y0 as f64,
                                            f64::from(bw),
                                            f64::from(bh),
                                        );
                                        let prior: std::collections::BTreeSet<
                                            yanshi_core::ObjectId,
                                        > = objects[..index].iter().map(|o| o.id.clone()).collect();
                                        assess_backdrop(
                                            ctx,
                                            &layer.id,
                                            &prior,
                                            region,
                                            &support,
                                            &below,
                                            (canvas_w as usize, canvas_h as usize),
                                        )
                                    })();
                                    match assessment {
                                        BackdropAssessment::Homogeneous(mean) => {
                                            flat_backdrop = Some(mean);
                                        }
                                        BackdropAssessment::Heterogeneous => {
                                            counts.strokes_raster += 1;
                                            counts.flatten_heterogeneous_raster += 1;
                                            counts.note_reason("flatten_heterogeneous_backdrop");
                                            pending.push(index);
                                            continue;
                                        }
                                        BackdropAssessment::Unavailable => {
                                            counts.flatten_skipped += 1;
                                        }
                                    }
                                }
                            }
                            let mut a_cum = 0.0f64;
                            for export_layer in &export_layers {
                                // **`flatten_opacity`（第 1409 轮）** ✓：把半透明**在线性空间压平到背景**，
                                //   emit 不透明色 ⇒ 与引擎逐像素一致 ✓
                                //   **⇒ 为什么** ✗：引擎在**线性光**空间合成，而 SVG 的 `fill-opacity`
                                //     在**sRGB 空间**混合 ⇒ 两者数学上不等 ⇒ 实测最坏差 **73 通道**
                                //     （`scripts/check-svg-opacity-colour-space.mjs` 钉住这个数 ✓）
                                //   **⇒ 代价**：那条路径不再保留可编辑的 α（变成压平色）⇒ 取舍已写进文档 ✓
                                // 累积压平：第 k 层 emit 色 = 前 k 层依次合成的
                                // 累积色（`A_k = 1 − Π(1 − o_j)`、`C_k = A_k·C +
                                // (1 − A_k)·C_backdrop`、全部 emit 不透明 ✓）。
                                // tier 支撑严格嵌套 ⇒ 层内精确、只剩 AA 边缘带 ✓。
                                let (fill_out, opacity_out) =
                                    if let Some(backdrop_lin) = flat_backdrop {
                                        a_cum = 1.0
                                            - (1.0 - a_cum)
                                                * (1.0 - export_layer.fill_opacity.clamp(0.0, 1.0));
                                        let a = a_cum as f32;
                                        let mixed = [
                                            fill_linear[0] * a + backdrop_lin[0] * (1.0 - a),
                                            fill_linear[1] * a + backdrop_lin[1] * (1.0 - a),
                                            fill_linear[2] * a + backdrop_lin[2] * (1.0 - a),
                                        ];
                                        (svgx::srgb_hex(mixed), 1.0)
                                    } else {
                                        (fill.clone(), export_layer.fill_opacity)
                                    };
                                nodes.push(SvgNode::Path {
                                    d: svgx::contours_path_data(&export_layer.contours),
                                    fill: fill_out,
                                    fill_opacity: opacity_out,
                                    blend: css_blend.filter(|name| *name != "normal"),
                                });
                            }
                            if native_layer {
                                counts.strokes_vector_native += 1;
                            } else {
                                counts.strokes_vector += 1;
                            }
                            if flat_backdrop.is_some() {
                                counts.strokes_flattened += 1;
                            }
                        }
                    }
                }
                Prim::Shape {
                    kind,
                    bbox,
                    points,
                    subpaths,
                    fill_rule,
                    has_fill,
                    color,
                    stroke_width,
                    stroke_color,
                    feather,
                    texture,
                } => {
                    // **纹理填充也要压平** ✓（`fill_region(texture=…)` ✓）：SVG 的 `<path fill>` 只能
                    // 表达**一个纯色** ✗ ⇒ 把它当纯色导出就是**静默画错** ✗ ⇒ 与羽化同路：
                    // 交给渲染器光栅化 ✓（纹理是逐个像素取色的，压平后**外观与画布一致** ✓）。
                    if feather > 0.0 || texture.is_some() {
                        pending.push(index);
                        continue;
                    }
                    flush_pending!();
                    // **`flatten_opacity` 也覆盖**形状** ✓（第 1411 轮）
                    //   **⇒ 为什么需要** ✗：第 1410 轮修好 `fill_region` 的 `opacity` 之后，
                    //     形状**真的**会带 `fill-opacity` 了 ⇒ 于是「引擎线性合成 vs SVG sRGB 混合」
                    //     这个色差**也出现在形状上** ⇒ 而压平此前只作用于**笔触路径** ⇒ 不一致 ✗
                    //   **⇒ 修法**：同一个压平逻辑（线性空间混到 backdrop ⇒ emit 不透明色）✓
                    //   **2026-10-21 Q1** ✓：backdrop 与笔触同路实测（`assess_backdrop` ✓）；
                    //   形状只有一层 alpha ⇒ 累积退化为单层式、与旧实现同式，
                    //   区别只在 backdrop 的来源（实测合成 vs 文档背景字段 ✓）；
                    //   异质 ⇒ 回退位图并计数（不许单色糊 ✗）；描边 alpha 不压
                    //   （形状描边是独立的一层覆盖 ⇒ 已知界限、见补丁 README ✓）。
                    let shape_alpha = f64::from(color[3]);
                    let mut shape_backdrop: Option<[f32; 3]> = None;
                    let mut shape_raster_fallback = false;
                    // 纯描边 Path（has_fill=false）没有填充可压：跳过压平、下面直出 fill="none"。
                    // has_fill 对其余 kind 恒 true ⇒ 这一条只影响 Path，存量行为不变。
                    if flatten_opacity && has_fill && shape_alpha < 0.9995 {
                        if layer.opacity < 0.9995 || layer_blend_css.is_some() || !backdrop_exact {
                            counts.flatten_skipped += 1;
                        } else {
                            // 支撑采样点：bbox 内网格（≤ ~40×40 ✓）；rect 全取、
                            // ellipse 做点测、polygon 用 bbox 近似（偏保守：
                            // 可能把均质误判异质 ⇒ 只是回退位图、不出错色 ✓）。
                            let mut support: Vec<(i64, i64)> = Vec::new();
                            let step_x = (bbox.w / 40.0).max(1.0);
                            let step_y = (bbox.h / 40.0).max(1.0);
                            let (cx, cy) = (bbox.x + bbox.w / 2.0, bbox.y + bbox.h / 2.0);
                            let mut sy = bbox.y + step_y * 0.5;
                            while sy < bbox.y + bbox.h {
                                let mut sx = bbox.x + step_x * 0.5;
                                while sx < bbox.x + bbox.w {
                                    let inside = match kind {
                                        yanshi_render::object::ShapeKind::Ellipse => {
                                            let dx = (sx - cx) / (bbox.w / 2.0).max(1e-6);
                                            let dy = (sy - cy) / (bbox.h / 2.0).max(1e-6);
                                            dx * dx + dy * dy <= 1.0
                                        }
                                        _ => true,
                                    };
                                    if inside {
                                        support.push((sx.floor() as i64, sy.floor() as i64));
                                    }
                                    sx += step_x;
                                }
                                sy += step_y;
                            }
                            let rx0 = (bbox.x - 2.0).floor().max(0.0);
                            let ry0 = (bbox.y - 2.0).floor().max(0.0);
                            let rx1 = (bbox.x + bbox.w + 2.0).ceil().min(f64::from(canvas_w));
                            let ry1 = (bbox.y + bbox.h + 2.0).ceil().min(f64::from(canvas_h));
                            let region = yanshi_core::Bbox::new(
                                rx0,
                                ry0,
                                (rx1 - rx0).max(0.0),
                                (ry1 - ry0).max(0.0),
                            );
                            let prior: std::collections::BTreeSet<yanshi_core::ObjectId> =
                                objects[..index].iter().map(|o| o.id.clone()).collect();
                            match assess_backdrop(
                                ctx,
                                &layer.id,
                                &prior,
                                region,
                                &support,
                                &below,
                                (canvas_w as usize, canvas_h as usize),
                            ) {
                                BackdropAssessment::Homogeneous(mean) => {
                                    shape_backdrop = Some(mean);
                                }
                                BackdropAssessment::Heterogeneous => {
                                    shape_raster_fallback = true;
                                    counts.flatten_heterogeneous_raster += 1;
                                    counts.note_reason("flatten_heterogeneous_backdrop");
                                }
                                BackdropAssessment::Unavailable => {
                                    counts.flatten_skipped += 1;
                                }
                            }
                        }
                    }
                    if shape_raster_fallback {
                        pending.push(index);
                        continue;
                    }
                    let (fill, emit_opacity) = if let Some(backdrop_lin) = shape_backdrop {
                        counts.shapes_flattened += 1;
                        let a = shape_alpha.clamp(0.0, 1.0) as f32;
                        let mixed = [
                            color[0] * a + backdrop_lin[0] * (1.0 - a),
                            color[1] * a + backdrop_lin[1] * (1.0 - a),
                            color[2] * a + backdrop_lin[2] * (1.0 - a),
                        ];
                        (svgx::srgb_hex(mixed), None)
                    } else {
                        (
                            svgx::srgb_hex([color[0], color[1], color[2]]),
                            Some(shape_alpha),
                        )
                    };
                    let mut attrs = if kind == yanshi_render::object::ShapeKind::Path && !has_fill {
                        " fill=\"none\"".to_string()
                    } else {
                        format!(" fill=\"{fill}\"")
                    };
                    if let Some(op) = emit_opacity {
                        if op < 0.9995 {
                            attrs.push_str(&format!(" fill-opacity=\"{}\"", svgx::fmt_num(op)));
                        }
                    }
                    // fill-rule 透传（§4.7）：nonzero 是 SVG 默认值可省略，evenodd 必须写。
                    if kind == yanshi_render::object::ShapeKind::Path
                        && has_fill
                        && fill_rule == yanshi_render::path::FillRule::EvenOdd
                    {
                        attrs.push_str(" fill-rule=\"evenodd\"");
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
                                // **★ 压平模式下，半透明**描边**压不了 ⇒ 必须**出声**** ✓
                                //   （第 1434 轮；补丁自己列的「已知界限：形状只压 fill，描边不压」✓）
                                //   **⇒ 为什么**压不了** ✗：描边**骑在**形状边缘上 ⇒ 一半压在 fill 上、
                                //     一半压在背景上 ⇒ **两个不同的 backdrop** ⇒ 压成单色**必错一半** ✗
                                //     （而 Q123 给 fill 的规矩正是「异质 ⇒ 回退位图、不许单色糊」✓）
                                //   **⇒ ⇒ 所以**不假装压平**，而是**如实计数 + 警告** ✓：
                                //     `flatten_stroke_skipped`（数）+ `warnings`（一句话，说清只有描边仍会在
                                //     浏览器里按 sRGB 混合 ⇒ 它的颜色仍可能与 PNG 不同）
                                if flatten_active {
                                    counts.flatten_stroke_skipped += 1;
                                    flatten_stroke_warned = true;
                                }
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
                        yanshi_render::object::ShapeKind::Path => {
                            // 原生直出（§4.7）：控制点原样写 M / L / C / Z，**不展平**
                            //（展平点列只活在渲染期；导出写它等于把曲线降级成折线）。
                            let mut d = String::new();
                            for sub in subpaths {
                                d.push_str(&format!(
                                    "M {} {}",
                                    svgx::fmt_num(sub.start.0),
                                    svgx::fmt_num(sub.start.1)
                                ));
                                for seg in &sub.segments {
                                    match seg {
                                        yanshi_render::path::PathSeg::Line { to } => {
                                            d.push_str(&format!(
                                                " L {} {}",
                                                svgx::fmt_num(to.0),
                                                svgx::fmt_num(to.1)
                                            ));
                                        }
                                        yanshi_render::path::PathSeg::Cubic { c1, c2, to } => {
                                            d.push_str(&format!(
                                                " C {} {} {} {} {} {}",
                                                svgx::fmt_num(c1.0),
                                                svgx::fmt_num(c1.1),
                                                svgx::fmt_num(c2.0),
                                                svgx::fmt_num(c2.1),
                                                svgx::fmt_num(to.0),
                                                svgx::fmt_num(to.1)
                                            ));
                                        }
                                    }
                                }
                                if sub.closed {
                                    d.push_str(" Z");
                                }
                            }
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
        // backdrop 追踪（Q1 ✓）：本层处理完 ⇒ 整层子集渲染（透明底 ✓）以
        // 图层不透明度并入 `below`、供后面图层的压平取 backdrop ✓。
        // 组带 CSS 混合时 plain over 镜像不了引擎合成 ⇒ 资格终止（计数在
        // 候选侧的 `flatten_skipped` 体现 ✓）；渲染失败同理（不静默近似 ✗）。
        if flatten_active && backdrop_exact {
            if layer_blend_css.is_some() {
                backdrop_exact = false;
            } else {
                let keep_all: std::collections::BTreeSet<yanshi_core::ObjectId> =
                    objects.iter().map(|o| o.id.clone()).collect();
                match ctx.workspace.render_layer_objects_subset(
                    &ctx.doc_id,
                    full_region,
                    &layer.id,
                    &keep_all,
                ) {
                    Ok((w, h, rgba)) => composite_layer_into_below(
                        &mut below,
                        (canvas_w as usize, canvas_h as usize),
                        (0, 0),
                        w as usize,
                        h as usize,
                        &rgba,
                        layer.opacity as f32,
                    ),
                    Err(_) => backdrop_exact = false,
                }
            }
        }
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
    // **半透明描边压不了** ⇒ 必须在 `warnings` 里说清（第 1434 轮）
    let export_warnings = {
        let mut warnings = render_warnings.clone();
        if flatten_stroke_warned {
            warnings.push(format!(
                "有 {} 处**半透明描边**没能压平（`flatten_stroke_skipped`）⇒ 描边骑在形状边缘上 \
                 （一半压在 fill、一半压在背景）⇒ **压成单色必错一半** ⇒ 所以保留 `stroke-opacity`；\
                 代价是**描边**的颜色仍会在浏览器里按 sRGB 混合（与 PNG 可能有差异），\
                 而 **fill 已经逐通道一致** ✓",
                counts.flatten_stroke_skipped
            ));
        }
        warnings
    };

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
        "strokes_vector_native": counts.strokes_vector_native,
        "strokes_double_layered": counts.strokes_double_layered,
        "strokes_raster": counts.strokes_raster,
        "strokes_empty": counts.strokes_empty,
        "raster_reasons": counts.raster_reasons,
        "shapes_vector": counts.shapes_vector,
        // **`flatten_opacity` 的账目（2026-10-21 Q1 ✓）**：请求了没、实际
        // 生效没、压平了几个、几个因 backdrop 异质回退位图、几个资格不
        // 成立没压 —— 全部如实报、不许调用方猜 ✓。
        "flatten_requested": flatten_requested,
        "flatten_active": flatten_active,
        "strokes_flattened": counts.strokes_flattened,
        "shapes_flattened": counts.shapes_flattened,
        "flatten_heterogeneous_raster": counts.flatten_heterogeneous_raster,
        "flatten_skipped": counts.flatten_skipped,
        "flatten_stroke_skipped": counts.flatten_stroke_skipped,
        "images_embedded": counts.images_embedded,
        "erases_masked": counts.erases_masked,
        "objects_unsupported": counts.objects_unsupported,
        "instances_unresolved": counts.instances_unresolved,
        "validate_failed": counts.validate_failed,
        "warnings": export_warnings,
    }))
}
