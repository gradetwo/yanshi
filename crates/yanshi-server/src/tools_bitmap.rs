//! **位图工具**（`put_blob` / `import_image`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项「tools.rs 拆分」）：
//! `tools.rs` 已超 30000 行。本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **依赖**：`use super::*` 拿 `tools.rs` 的公共 helper（同 crate 子模块可访问私有项）。

use super::*;

/// **把一个**服务端本地文件**放进内容寻址存储，返回 `blob_hash`** ✓
/// （第 1228 轮；外部 §176/177 连续四次要求这个工具）
///
/// **为什么必须有它** ✗：`import_image` 要求先有 `blob_hash`，而调用方手里**只有文件**。
///   手写 `blobs/sha256/xx/yy/hex` **不够** —— 服务端用的是 `RenderCodec`
///   （落盘格式 `[0x00][TAG][body]`，TAG=0 原始字节 / TAG=1 zlib）
///   ⇒ 裸 PNG 的第一个字节 `0x89` 会让 `decode` 返回 `None` ⇒
///   报「blob 无法按当前存储编码解出」⇒ **那一块永远不画** ✗（这正是外部 5 次失败的真因）
///   ⇒ 而 `BlobStore::put` **内部会过 `encode`** ⇒ 所以本工具替调用方把编码也做了 ✓
///
/// **路径限制**：只允许**服务端工作区 root 下**的文件（防任意文件读）。
///
/// **双通道扩展**（设计文档第 4 章）：本工具仍是 path 通道的入库端，新增两条：
///   ① `path` 以 `http://`／`https://` 开头 ⇒ URL 拉取（细则在
///      `crate::file_source::fetch_url`：超时／重定向／体积／类型五类分开报错）；
///   ② 平铺参数 `data_base64`（与 `path` 二选一，同给 ⇒ 互斥报错，不静默取舍）。
/// 三条通道的字节都过同一体积上限（`file_source::max_upload_bytes`，与 HTTP
/// 上传同源），且都汇进同一个 `put_blob`（编码口径只有一份 ⇒ 下游零改动）。
/// 本地 path 的沙箱口径（第 1230/1232 轮）原样保留，实现收进
/// `file_source::resolve_local_path` 全仓共用（错误码与触发条件不变）。
pub(super) fn write_put_blob(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let cap = crate::file_source::max_upload_bytes();
    let path_arg = optional_str(args, "path");
    let has_b64 = args.get("data_base64").and_then(Value::as_str).is_some();
    if path_arg.is_some() && has_b64 {
        crate::file_source::check_exclusive(&["path", "data_base64"])?;
    }
    if let Some(bytes) = crate::file_source::decode_base64_param(args, "data_base64", cap)? {
        let hash = ctx.workspace.put_blob(&bytes)?;
        return Ok(json!({
            "ok": true,
            "blob_hash": hash.to_string(),
            "size": bytes.len(),
            "mime_type": optional_str(args, "mime_type").unwrap_or_else(|| "image/png".to_owned()),
            "source": "base64",
        }));
    }
    // 一个来源都没给 ⇒ 仍走 require_str 的原报错（与旧行为同文案）。
    let path = match path_arg {
        Some(path) => path,
        None => require_str(args, "path")?,
    };
    if crate::file_source::is_http_url(&path) {
        let fetched = crate::file_source::fetch_url(&path)?;
        let hash = ctx.workspace.put_blob(&fetched.bytes)?;
        let mut value = json!({
            "ok": true,
            "blob_hash": hash.to_string(),
            "size": fetched.bytes.len(),
            "mime_type": optional_str(args, "mime_type").unwrap_or_else(|| "image/png".to_owned()),
            "source": "url",
            "source_url": path,
            "final_url": fetched.final_url,
        });
        if let Some(content_type) = &fetched.content_type {
            value["content_type"] = json!(content_type);
        }
        if let Some(detected) = fetched.detected_format {
            value["detected_format"] = json!(detected);
        }
        if fetched.content_type_mismatch {
            value["content_type_mismatch"] = json!(true);
        }
        return Ok(value);
    }
    if let Some(scheme) = crate::file_source::url_scheme(&path) {
        // file:// 之类想用 URL 形态绕本地路径 ⇒ 明确拒绝（设计 4.3 协议白名单）。
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "path 的 scheme 不支持：{scheme}://（只接受 http:// 与 https://；本地文件请直接给路径，不要用 file://）"
            )),
        ));
    }
    // **路径防护（第 1230/1232 轮的口径，实现收进 file_source 全仓共用）** ✓
    //   拒 `..` 组件（早退）＋ canonicalize 后必须落在工作区 root 之内（权威校验）。
    let (bytes, _resolved) =
        crate::file_source::resolve_local_path(&path, ctx.workspace.root_path(), cap)?;
    let hash = ctx.workspace.put_blob(&bytes)?;
    Ok(json!({
        "ok": true,
        "blob_hash": hash.to_string(),
        "size": bytes.len(),
        "mime_type": optional_str(args, "mime_type").unwrap_or_else(|| "image/png".to_owned()),
    }))
}

pub(super) fn write_import_image(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let bitmap = require_object(args, "bitmap")?.clone();
    let region = parse_bbox(require_object(args, "region")?)?;
    let object_id = optional_str(args, "object_id")
        .unwrap_or_else(|| format!("obj_{}", yanshi_core::Ulid::new().encode()));
    // **介质描述符可以随这一条原子一起记下** ✓（设计 11.1 的硬要求："插件 id + version 随原子记录，
    // 升级不自动改变旧文档渲染" ✓）。
    //
    // **为什么加这个参数** ✓：此前查看器要提交**两条**原子 ✗ —— `import_image` 建对象 ✓、
    // 再用 `replace_object_data` 把 `{bitmap, region, width, height, medium}` 整体钉上去 ✓。
    // 实测（无内核实例、端到端）**每次原子提交都不是免费的** ✓ ⇒ 一条能表达清楚的事分成两条 ✗
    // 就是白花一笔钱 ✓。现在描述符随导入一起提交 ✓ ⇒ **一笔介质只产生一条原子** ✓。
    //
    // **为什么放在净荷顶层** ✓（而不是塞进一个 `data` 子对象 ✗）：折叠器 `object_data` 的规则是
    // "有 `data` 就用 `data` ✓，否则用整个净荷" ✓ ⇒ 顶层放 ✓ 则对象数据 = 净荷（含 `medium` ✓），
    // 与原来"导入 + 替换"之后的**渲染相关字段逐字一致** ✓（只多一个描述符键 ✓ ——
    // 那正是设计要求记下的东西 ✓）；而 `data.blob_hash` 若被挪进子对象 ✓ 会让
    // `all_blob_refs` 的净荷扫描**找不到 blob** ✗ ⇒ 校验与保留都会出错 ✗。
    let mut payload = json!({
        "object_id": object_id,
        "layer_id": layer_id,
        "type": "raster_patch",
        "bitmap": bitmap,
        // **★ P0：必须写 `region`（或 width/height），否则位图**永远不可见** ✗ ★**
        //   **根因**（第 1223 轮实测）：`yanshi-render/src/object.rs` 的 `parse_raster_patch`
        //     从 `data["region"]`（或 `data["width"]`/`height`）取位图尺寸 ⇒
        //     **两个都没有 ⇒ 返回 `Primitive::Unsupported`**
        //     ⇒ 对象**建成了**（`describe_canvas` 实测 `L1.objects = 1`）、`ok:true`、
        //       `dirty_bbox` 也对 ⇒ **但渲染器把它当"不支持的原语"跳过** ⇒
        //       导出 PNG 里**一个像素都没有**（实测：墨 0、红块 0，应为 64）✗
        //   **修法**：把 `region` 一起写进净荷（`parse_raster_patch` 优先读它）
        "region": {
            "x": region.x,
            "y": region.y,
            "w": region.w,
            "h": region.h,
        },
        // **★ 登记**结构化的 blob 引用** ✗ ★**（第 227 轮 ✓；**∴ 导出丢位图的修复 ✓）。
        //
        // **∴ 为什么必须写 ✗**：**实测**（**第 226 轮 ✓）**✗**：
        //   **∴ `import_image` 的净荷里**没有 `refs`**** ✗
        //     ⇒ **∴ 而** `service.rs:2547` **读的正是 `payload.refs.blobs`** ✓
        //       ⇒ **∴ 于是** `refs_by_blob` **空** ⇒ **∴ 导出时**任何 blob**都不装** ✓
        //         ⇒ **∴ 包**14 KiB、`blobs/` ＝ **0 条** ✓
        //           ⇒ **★ 那**就是 `tool-export-bitmaps.mjs` 的红灯 ✓ ★**** ✓✓
        //   **∴ 而**本文件第 4749 行附近的注释**早已点出**：
        //     「**`all_blob_refs` 的净荷扫描**找不到 blob** ⇒ **∴ 校验与保留**都会出错** ✓」
        //     ⇒ **∴ 全仓**却**没有一处**写 `refs`** ✗ ⇒ **∴ 所以那个约定**从未实现** ✓**** ✓✓
        //
        // **∴ 写法 ✗**：**与读取方一致**（**`refs.blobs` ＝ **哈希字符串数组** ✓）
        // **∴ 好处（**两面 ✓）**：
        //   **∴ ①** 导出**能收到**位图** ⇒ **∴ 工程包**可独立重建** ✓
        //   **∴ ②** GC／校验**也能看见**这个引用** ✓
        // **∴ 代价**：**净荷多一个数组**（**∴ 一个哈希字符串 ✓，**可忽略 ✓）** ✓✓
        "bitmap": bitmap,
        "refs": {"blobs": [bitmap
            .get("blob_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()]},
        // **★ `region` + `width`/`height` 都要写** ✗→✓（第 1242 轮：我第 1225 轮删它们**错了**）
        //   **第 1225 轮我的理由**：同一事实写两遍 ⇒ 单一来源 ⇒ 只留 `region`
        //   **那条理由被 CI 推翻了**（第 1242 轮）：两个测试直接断言净荷里的 `width`/`height`
        //     · `one_atom_records_the_bitmap_and_the_medium_descriptor`
        //     · `the_one_atom_form_matches_the_old_two_step_form_on_every_rendering_field`
        //   ⇒ 所以它们**不是**「派生冗余」，而是**协议的一部分**（旧的两步形式
        //     `replace_object_data` 也写 `{bitmap, region, width, height, medium}`）✗
        //   ⇒ **教训**：判「冗余」之前要 grep **测试与文档**，不能只看代码里谁读它
        //     （我 1225 轮只看了 `parse_raster_patch` 的读取顺序）✗
        //
        //   **而这也是我 1225 轮「删 region 变异不红」的真正原因** ✓：
        //     因为 `width`/`height` 一直在，`parse_raster_patch` 优先读它们 ⇒ 删 `region` 看不出变化
        "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
        "width": region.w as u64,
        "height": region.h as u64,
    });
    if let Some(medium) = args.get("medium") {
        if !medium.is_null() {
            payload["medium"] = medium.clone();
        }
    }
    // **"这一笔是用什么画出来的"也随原子记下** ✓（本轮 ✓）——
    // 光栅补丁的颜色是**烘进像素**的 ✓ ⇒ 想改色只能**按来源重跑一遍** ✓
    // ⇒ 那就必须把来源（笔刷名 ✓、控制点 ✓、当时的 size / color / smooth ✓）留在对象上 ✓，
    // 而不是只留在**已经关掉的那个调用方**心里 ✗（这正是 `update_stroke` 一直改不动色的根 ✓）。
    if let Some(source) = args.get("source") {
        if !source.is_null() {
            payload["source"] = source.clone();
        }
    }
    // **位图分块（(b1) 导入侧 ✓，第 170 轮 ✓）** —— 目标第 1 条"懒解码"的**落盘点** ✓。
    //
    // **为什么在这里** ✓：整体式 deflate 流**取任意一段都要先解整条** ✗ ⇒ 实测**为渲 64² 而解 265 MB**
    //（**16200× 区域像素** ✗）⇒ **∴ 大位图在**导入时**切成 256² 块** ✓ ⇒ 渲染侧只解**覆盖请求区域**的块 ✓。
    // **阈值** ✓：~~只在 `max(width,height) ≥ 512` 时才切~~ ⇒ **★ 第 430 轮：门限降到 64 ★** ✓
    //   小图整幅解码本就便宜 ✓，切块反而多花 blob ＋ 索引开销 ✗ —— **两面** ✓。
    // **★ 为什么把 512 降到 64 ✗ ★**（第 430 轮 ✓；**判据实测逼出来的 ✓）：
    //   **∴ 症状 ✗**：**判据 `tool-bitmap-decode-scope` 长期红**✗
    //     ＋ **探针（`YANSHI_BITMAP_PROBE` ✓）实测**：
    //       ```
    //       BITMAP decode key=…|★ 512x512 ★|image/png 512x512 len=★ 1048576 ★
    //       ```
    //       ⇒ **∴ 即**：**一个 `512²` 的 PNG **整幅解码**✗（**＝ 它的完整明文 ✓）
    //   **∴ 根因 ✗**：**读侧**（**`render.rs:2973` 的分块路 ✓）**早就实现了**✗
    //     ＋ **∴ 而**它**靠对象数据里的 `tiles` 索引** ✓
    //       ⇒ **∴ 而**索引**只在 `≥ 512` 时写**✗
    //         ⇒ **∴ 于是**：**`region < 512` 的位图**永不建索引**✗
    //           ⇒ **∴ 分块路**永不生效** ✓ ⇒ **∴ 判据**永远红** ✓ ★**** ✓✓
    //   **∴ 修法 ✗**：**门限降到 64**✗
    //     ⇒ **∴ 于是**：**小位图**也建索引**✗ ⇒ **∴ 分块路**生效** ✓
    //       ⇒ **∴ 解码量**从**整幅**降到**覆盖块** ✓ ★**** ✓✓
    //   **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
    //     **∴ 收益 ✗**：**小位图不再整幅解码**✗（**实测那条 `512² PNG` ⇒ **约 1 块 262,144 B** ✓）
    //       ＋ **∴ 且**：**大位图**原本就受益** ✓ ★**** ✓✓
    //     **∴ 代价 ✗**：**小图**多一个索引 blob**✗（**∴ 64² 图 ⇒ 1 块 ＋ 一份小 JSON ✓）
    //       ⇒ **∴ 对**极小图（**< 64 ✓）**几乎无变化** ✓
    //         ＋ **∴ 且**：**旧工程**没有 `tiles`**✗ ⇒ **∴ 读侧**回退整幅** ✓（**∴ 行为**一字不变 ✓）★**** ✓✓
    if region.w.max(region.h) >= 64.0 {
        let blob_hash = bitmap
            .get("blob_hash")
            .and_then(Value::as_str)
            .unwrap_or("");
        let mime = bitmap
            .get("mime_type")
            .and_then(Value::as_str)
            .unwrap_or("");
        if let Some(index_hash) =
            build_bitmap_tile_index(ctx, blob_hash, mime, region.w as u32, region.h as u32)
        {
            payload["tiles"] = json!(index_hash);
        }
    }

    // **★ 把「blob 能不能解码」提前报出来**（第 1236 轮）
    //   **为什么** ✗：外部用户 5 次失败，而唯一能看见原因的地方是
    //     `export_png` 的 `render_warnings`（「位图补丁缺少 blob ⇒ 这一块没有画」）
    //     ⇒ 而调用方只看 `import_image` 的返回 ⇒ 把「原子提交成功」误读成「位图导入成功」✗
    //   **语义**：`ok` 不变（导入本身成功）；`blob_decodable=false` 是**能力信号**
    let hash_for_check = payload
        .get("bitmap")
        .and_then(|b| b.get("blob_hash"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let mime_for_check = payload
        .get("bitmap")
        .and_then(|b| b.get("mime_type"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    // **`blob_decodable` 必须名副其实**：除了 blob 存在/信封可解，**格式**也要在内核
    // 支持清单里（`raster_mime_supported` 单一事实来源 ✓）——曾只查存在性，JPEG 导入
    // 全程报 true 却被内核按「格式未实现」跳过、渲染纯白 ✗。
    let format_supported = yanshi_render::raster_mime_supported(&mime_for_check);
    let decodable = ctx.workspace.blob_decodable(&hash_for_check) && format_supported;
    let result = ctx.commit(AtomKind::ImportImage, payload)?;
    let bbox = region_of(&result).or(Some(region));
    let mut out = finish_mutation(ctx, &result, bbox)?;
    if let Some(object) = out.as_object_mut() {
        object.insert("blob_decodable".to_owned(), json!(decodable));
        if !decodable && !format_supported {
            object.insert(
                "note".to_owned(),
                json!(format!(
                    "格式 {mime_for_check} 内核解不开 ⇒ 这一块不会画出来（内核仅支持 raw/PNG/JPEG，请先转码）"
                )),
            );
        } else if !decodable {
            object.insert(
                "note".to_owned(),
                json!(format!(
                    "blob {hash_for_check} 解不开 ⇒ 这一块不会画出来（手写文件要包 RenderCodec 头 [0x00][TAG]，或用 put_blob）"
                )),
            );
        }
    }
    Ok(out)
}
