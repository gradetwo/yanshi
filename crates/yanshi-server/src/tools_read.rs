//! **读取类工具**（`read_analyze_region` / `read_render_region` / `read_get_log` /
//! `read_get_job` / `read_get_render_status` / `read_get_checkpoints`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **为什么只有这 6 个**：`read_*` 在 `tools.rs` 里**不连续** —— 前面还有
//! `read_sample_colors`（:1682）等，中间夹着 `require_f64` 这类 helper ⇒
//! 所以**这一刀只切连续的那一段**（`:2683..3383`）⇒ 其余留给后续。

use super::*;

/// **`analyze_region`**（AI 画家需求 P2-10 ✓）：**纯读** ✓ —— 取像素走**既有的**
/// `ctx.workspace.render_region_raw` ✓（与 `render_region(raw:true)` 同一条路 ✓，不另写渲染 ✗）。
pub(super) fn read_analyze_region(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let region = parse_bbox(require_object(args, "region")?)?;
    let (width, height, pixels) = ctx.workspace.render_region_raw(&ctx.doc_id, region)?;
    if pixels.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("这块区域没有像素可分析".to_string()),
        ));
    }
    // 每通道高 4 位 ⇒ 16³ 个桶 ✓；同时累计每桶的通道**和** ✓（以便回"桶内平均色" ✓）。
    const SIDE: usize = 16;
    let mut counts = vec![0u64; SIDE * SIDE * SIDE];
    let mut sums = vec![[0u64; 3]; SIDE * SIDE * SIDE];
    let mut brightness = 0.0_f64;
    let (mut warm, mut cool) = (0u64, 0u64);
    for index in (0..pixels.len()).step_by(4) {
        let (r, g, b) = (
            u64::from(pixels[index]),
            u64::from(pixels[index + 1]),
            u64::from(pixels[index + 2]),
        );
        let bucket = (((r >> 4) as usize) << 8) | (((g >> 4) as usize) << 4) | ((b >> 4) as usize);
        counts[bucket] += 1;
        sums[bucket][0] += r;
        sums[bucket][1] += g;
        sums[bucket][2] += b;
        brightness += 0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64;
        if r > b {
            warm += 1;
        } else if b > r {
            cool += 1;
        }
    }
    let total = (pixels.len() / 4).max(1) as u64;
    let mut order: Vec<usize> = (0..counts.len()).filter(|&i| counts[i] > 0).collect();
    order.sort_by(|a, b| counts[*b].cmp(&counts[*a]));
    let dominant: Vec<Value> = order
        .iter()
        .take(5)
        .map(|&i| {
            let count = counts[i].max(1);
            json!({
                "r": sums[i][0] / count,
                "g": sums[i][1] / count,
                "b": sums[i][2] / count,
                "hex": format!("#{:02x}{:02x}{:02x}", sums[i][0] / count, sums[i][1] / count, sums[i][2] / count),
                "count": count,
                "share": count as f64 / total as f64,
            })
        })
        .collect();
    // **与参考图比 ΔE**（测试报告 §三.2 ✓，可选 ✓）：只有调用方明确要 `compare_with_reference` 时才做 ✓。
    // 路径全部照抄仓库已有写法（`write_import_psd` ✓）：preferences 取哈希 ⇒ `store().get` ⇒ 解码 ⇒ 逐像素 ✓。
    let mut comparison = Value::Null;
    if optional_bool(args, "compare_with_reference").unwrap_or(false) {
        let prefs = ctx
            .workspace
            .preferences(Some(&["reference.blob_hash".to_string()]));
        match prefs.get("reference.blob_hash").and_then(Value::as_str) {
            // **明确作答** ✓：没有参考图就直说 ✗ —— **绝不静默返回 0** ✗（与 §一.1 那条调色板 Bug 同一纪律 ✓）。
            None => {
                comparison = json!({
                    "ok": false,
                    "reason": "没有参考图 ⇒ 先用 set_reference 设一张",
                });
            }
            Some(text) => {
                let hash: yanshi_core::BlobHash = text.parse().map_err(|_| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("reference.blob_hash 不是合法哈希"),
                    )
                })?;
                let bytes = ctx.workspace.store().get(&hash)?;
                match yanshi_render::png::decode_png(&bytes) {
                    None => {
                        comparison = json!({
                            "ok": false,
                            "reason": "参考图不是能解码的 PNG",
                        });
                    }
                    Some((reference_width, reference_height, reference)) => {
                        if reference_width != width || reference_height != height {
                            // **比不了就说比不了** ✓：给 AI 一个假数字比不给更糟 ✗。
                            comparison = json!({
                                "ok": false,
                                "reason": format!(
                                    "参考图 {reference_width}x{reference_height} 与区域 {width}x{height} 尺寸不一致 ⇒ 无法逐像素比"
                                ),
                            });
                        } else {
                            let mut sum = 0.0_f64;
                            let mut worst = 0.0_f64;
                            let mut worst_at = [0_u32, 0_u32];
                            let count = (width as usize) * (height as usize);
                            for index in 0..count {
                                let at = index * 4;
                                let rendered_linear = [
                                    yanshi_render::color::byte_to_linear(pixels[at]),
                                    yanshi_render::color::byte_to_linear(pixels[at + 1]),
                                    yanshi_render::color::byte_to_linear(pixels[at + 2]),
                                ];
                                let reference_linear = [
                                    yanshi_render::color::byte_to_linear(reference[at]),
                                    yanshi_render::color::byte_to_linear(reference[at + 1]),
                                    yanshi_render::color::byte_to_linear(reference[at + 2]),
                                ];
                                let delta = yanshi_render::color::delta_e_linear_rgb(
                                    rendered_linear,
                                    reference_linear,
                                ) as f64;
                                sum += delta;
                                if delta > worst {
                                    worst = delta;
                                    worst_at = [(index as u32) % width, (index as u32) / width];
                                }
                            }
                            comparison = json!({
                                "ok": true,
                                "delta_e_mean": sum / (count.max(1) as f64),
                                "delta_e_max": worst,
                                "delta_e_max_at": worst_at,
                                "reference": {"width": reference_width, "height": reference_height},
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(json!({
        "ok": true,
        "width": width,
        "height": height,
        "pixels": total,
        "comparison_with_reference": comparison,
        "dominant_colors": dominant,
        // **0..1** ✓（÷255 ✓）
        "avg_brightness": brightness / total as f64 / 255.0,
        "warm_pixels": warm,
        "cool_pixels": cool,
        // **暖/(暖+冷)** ✓ —— 恒在 0..1 ✓、不会除零 ✓（相等不计入任何一边 ✓）。
        "warm_cool_ratio": if warm + cool == 0 {
            0.5
        } else {
            warm as f64 / (warm + cool) as f64
        },
    }))
}

/// **`get_histogram`**（包 B，2026-10-21）：**纯读** —— 返回数值直方图，不回图。
///
/// 作用域三选一（缺省 = 整文档渲染结果）：
/// * 只给 `layer_id` ⇒ 只渲染那一层（走 `render_region_raw_layer`，不污染文档级缓存 ✓）；
/// * 只给 `region` ⇒ 整文档渲染后取该区域；
/// * 两者都给 ⇒ 该图层在该区域内的像素；都不给 ⇒ 整幅画布。
///
/// 统计口径在内核 `yanshi_render::histogram` 一处：每通道 256 桶 + 亮度桶
/// （整数 Rec.601）+ 摘要（均值/极值/p5/中位数/p95，下分位口径）。
pub(super) fn read_get_histogram(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = optional_str(args, "layer_id");
    let region = match args.get("region") {
        Some(value) if !value.is_null() => Some(parse_bbox(value)?),
        _ => None,
    };
    let state = document_state(ctx)?;
    if let Some(id) = layer_id.as_deref() {
        if !state.layers.contains_key(id) {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("图层 {id} 不存在")),
            )
            .with_layer(id.to_owned()));
        }
    }
    let bbox = region
        .unwrap_or_else(|| Bbox::new(0.0, 0.0, f64::from(state.width), f64::from(state.height)));
    let scope = match (layer_id.is_some(), region.is_some()) {
        (true, true) => "layer_region",
        (true, false) => "layer",
        (false, true) => "region",
        (false, false) => "document",
    };
    ctx.count_region_render();
    let render_started = std::time::Instant::now();
    let (width, height, pixels) = match layer_id.as_deref() {
        Some(id) => ctx
            .workspace
            .render_region_raw_layer(&ctx.doc_id, bbox, id)?,
        None => ctx.workspace.render_region_raw(&ctx.doc_id, bbox)?,
    };
    ctx.time(Phase::Render, render_started);
    if pixels.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("这块作用域没有像素可统计".to_string()),
        ));
    }
    let histogram = yanshi_render::histogram_rgba8(&pixels);
    let summary = |buckets: &[u64; 256]| {
        let summary = yanshi_render::summarize(buckets, histogram.count);
        json!({
            "mean": summary.mean,
            "min": summary.min,
            "max": summary.max,
            "p5": summary.p5,
            "median": summary.median,
            "p95": summary.p95,
        })
    };
    Ok(json!({
        "ok": true,
        "scope": scope,
        "layer_id": layer_id,
        "region": {"x": bbox.x, "y": bbox.y, "w": bbox.w, "h": bbox.h},
        "width": width,
        "height": height,
        "pixels": histogram.count,
        "channels": {
            "r": histogram.r.as_slice(),
            "g": histogram.g.as_slice(),
            "b": histogram.b.as_slice(),
            "a": histogram.a.as_slice(),
        },
        "luma": histogram.luma.as_slice(),
        "summary": {
            "r": summary(&histogram.r),
            "g": summary(&histogram.g),
            "b": summary(&histogram.b),
            "a": summary(&histogram.a),
            "luma": summary(&histogram.luma),
        },
    }))
}

pub(super) fn read_render_region(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **★ 可选 `active_layer` ✓ ★**（**目标第 4 条 ✓**；第 523 轮 ✓）：**∴ 让"改中间层"
    // 只重渲该层 ✓**（**三段分解的切点 ✓**）；**∴ 不传 ⇒ 退回"最上层以外"✓**（**行为不变 ✓**）。
    // **★ 紧贴这一次渲染读两次计数 ✓ ★**（第 537 轮 ✓）：**∴ 差值 > 0 ⇔ 本次复用了 below ✓**
    let below_before = ctx.workspace.document_below_reuse_count(&ctx.doc_id);
    // **★ 整幅缓存命中的快照 ✗ ★**（第 804 轮 ✓）：**∴ 它与 `below_before` 同法 ✓**
    //   —— **∴ 因为**整幅命中时**渲染器不会被调用**✗ ⇒ **∴ tile 计数不动 ✓**。
    let frame_before = ctx.workspace.document_full_frame_hits(&ctx.doc_id);
    // **★ `above` 的同一套读数 ✗ ★**（第 706 轮 ✓；**目标第 4 条 ✓**）：
    //   **∴ 与 `below` **前后各读一次**✗ ⇒ **∴ 差值 > 0 ⇔ 本次复用了上方合成 ✓**
    //   ⇒ **∴ 于是**判据可以断言"切到半透明层后 `above` 命中上升"✗**（**§14.33 ✓**）。
    let above_before = ctx.workspace.document_above_reuse_count(&ctx.doc_id);
    let active_layer = optional_str(args, "active_layer");
    // **★ `layer_id`：只渲染**指定的一层** ✗ ★**（第 185 轮 ✓；**PWA 实测 P1-2 ✓）：
    //   **∴ 为什么 ✗**：**图层面板**要给**每一行**一张缩略图 ✓
    //     ⇒ **∴ 那**必须能**单独渲染一层** ✓
    //   **∴ 而**后端**早就**有** `render_region_raw_layer`（`set_only_layer` ✓）✗
    //     ⇒ **∴ 只是**工具层**没**透传 ✓（**∴ 上游补丁 README 点到了这一半 ✓）** ✓✓
    //   **★ 但**预览路径**不能**复用 `Document::render_region`**✗
    //     ⇒ **∴ 因为**它**会**更新文档级缓存**✗（`document_thumbnail` / `full_frame_render` ✓）
    //       ⇒ **∴ 于是**单层小图**会**冒充**文档缩略图 ✓ ★**** ✓✓
    //     ⇒ **∴ 所以**走**新方法** `render_region_layer`（**不碰**那些指针 ✓）** ✓✓
    let layer_id = optional_str(args, "layer_id");
    // **★ 归属读数 ✗ ★**（第 993 轮 ✓）：**∴ 渲染序号**的前后差值
    //   ⇔ **"**本次请求**触发了几次渲染 ✓"**
    //     ⇒ **∴ `0`** ⇒ **∴ 被缓存拦掉（**没进渲染器 ✓）**；**`≥1`** ⇒ **∴ 进了 ✓**
    //       ⇒ **★ 于是**：**探针行**可以**按序号**归属到**这一次请求 ✓ ★**** ✓✓
    let renders_before = yanshi_render::render::renders_done();
    // **★ 临时探针 ✓**（第 528 轮 ✓，**查明后删 ✓**）：**打印 `args` 的键 ✗** ⇒
    // **∴ 分清**"框架丢了未知键 ✗"与"`optional_str` 用法错 ✗"**（**症状相同 ✓**）**。
    if let Ok(probe) = std::env::var("YANSHI_BELOW_PROBE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&probe)
        {
            let keys: Vec<String> = args
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let _ = writeln!(
                f,
                "render_region args keys={keys:?} active={active_layer:?}"
            );
        }
    }
    if let Ok(path) = std::env::var("YANSHI_REGION_PROBE") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(f, "TOOL.read_render_region called");
        }
    }
    // `raw: true`：额外把**原始 RGBA8** 存入 CAS 并返回取回地址，供客户端做逐像素比对
    // （哈希相等无法说明差多少；跨客户端/服务端路径的差异属于 D1 的 ±1 LSB）。
    if args.get("raw").and_then(Value::as_bool).unwrap_or(false) {
        // **不静默忽略**：`output_path` 只适用于下面的预览形态（PNG 落盘）；
        // raw 形态回的是原始 RGBA blob，两者同给必须说清，而不是悄悄丢掉参数。
        if optional_str(args, "output_path").is_some() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "output_path 不适用于 raw 形态（raw 回的是原始 RGBA，不是 PNG 文件）；请去掉 raw，或改用 export_png 落盘",
                ),
            ));
        }
        // **★★★ 第 247 轮：**给 raw 路分段计时**✗ ★★★**（**∴ 因为缓存已经生效 ✗ 却还有 94 ms ✓）
        //   ＋ **∴ 实测（**上一轮 ✓）✗**：**同一请求连做三次 **609 → 94 → 99 ms**✗
        //     ＋ **∴ 而**带 `active_layer` 连做两次是 **94 → 94 ms** ✓
        //       ⇒ **∴ 也就是说**区域字节缓存**已经命中**✗ 每次请求仍有**约 94 ms 的固定成本** ✓
        //         ⇒ **∴ 而**它**不在 `render_region` 里** ✗（**∴ 否则命中后该是毫秒级 ✓）★★
        //   ＋ **∴ 所以 ✗**：**在这里**分段**✗ 把 94 ms 归属清楚** ✓：
        //     **∴ ① `render`（**含缓存命中 ✓）｜**② `store.put`（**1 MB 进 CAS ✓）
        //       ｜**③ 其余（**几何 ＋ 组装 ＋ 序列化 ✓）★
        //   ＋ **∴ 门控 ✗**：`YANSHI_RENDER_PROBE`（**∴ 生产路**只多几次 `Instant::now` ✓）★
        let __probe = std::env::var_os("YANSHI_RENDER_PROBE").is_some();
        let __t_all = std::time::Instant::now();
        let region = parse_bbox(require_object(args, "region")?)?;
        let __t_render = std::time::Instant::now();
        let (width, height, pixels) =
            ctx.workspace
                .render_region_raw_active(&ctx.doc_id, region, active_layer.as_deref())?;
        let __render_ms = __t_render.elapsed();
        // **裸像素出口也必须带告警** ✗：它返回的是 `(宽, 高, RGBA)` ✓，没有地方带告警 ✓
        // ⇒ 缺一个补丁时**必须**在这里读一次并报出去 ✓，否则就是"静默的不完整画面" ✗
        //（`Document::last_render_warnings` 的说明 ✓）。
        let render_warnings = ctx.workspace.last_render_warnings(&ctx.doc_id);
        // **★ `show_coords`：把实际导出范围**叠在水印上**（Muse 建议 4，P2）★**
        //
        // **为什么要有它** ✓：`requested_region`／`rendered_region`／`clipped` **早就回报了** ✓
        //   ⇒ "取偏了"能靠**数据**发现 ✓ —— 但**从图上看不出来** ✗（像"把眉毛切在边缘"这种，
        //     人眼/AI 看图的当下就该看见 ✓）⇒ 所以把范围**画进像素** ✓。
        //
        // **三条硬性约束** ✓（判据 `tool-show-coords.mjs` 逐条钉着 ✓）：
        //   ① **缺省不叠** ✓ ⇒ 与"完全不传该参数"**逐字节相同** ✓（默认出图要干净 ✓）；
        //   ② 传了 `true` ⇒ **必须真的变** ✓（"参数被静默忽略"是本仓库最恨的 UX 之一 ✗）；
        //   ③ **只动左上角那条带** ✓ ⇒ 带外像素**逐字节不变** ✓（加个水印不该把画改了 ✗）。
        //
        // **实现只碰"导出像素"** ✓：不写文档、不落额外原子 ✓ ⇒ 画面本身与历史都不受影响 ✓。
        let mut pixels = pixels;
        if args
            .get("show_coords")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            let width_u = width.max(1);
            let height_u = height.max(1);
            let label = format!(
                "x={}..{} y={}..{}  {}x{}",
                region.x.round(),
                (region.x + region.w).round(),
                region.y.round(),
                (region.y + region.h).round(),
                width_u,
                height_u
            );
            // ① 半透明底条（让白字在任何画面上都读得出来 ✓）——只压暗，不改色相 ✓
            let band = 20u32.min(height_u);
            for y in 0..band {
                for x in 0..width_u {
                    let at = ((y * width_u + x) * 4) as usize;
                    if at + 3 < pixels.len() {
                        pixels[at] = (u32::from(pixels[at]) * 45 / 100) as u8;
                        pixels[at + 1] = (u32::from(pixels[at + 1]) * 45 / 100) as u8;
                        pixels[at + 2] = (u32::from(pixels[at + 2]) * 45 / 100) as u8;
                    }
                }
            }
            // ② 用**内核既有的位图字体**画字 ✓（不另造一套 ✗）
            let mut overlay = yanshi_render::Buffer::new(0, 0, width_u, height_u);
            yanshi_render::font::draw_text(
                &mut overlay,
                &label,
                6.0,
                14.0,
                1,
                [1.0, 1.0, 1.0, 1.0],
                "left",
                0.0,
            );
            let glyphs = overlay.to_rgba8(None);
            let mut changed = 0u32;
            for (index, chunk) in glyphs.chunks_exact(4).enumerate() {
                // 字是**不透明白** ✓；其余像素 alpha=0 ⇒ 不动原图 ✓（约束 ③ ✓）
                if chunk[3] > 0 {
                    let at = index * 4;
                    if at + 3 < pixels.len() {
                        pixels[at] = chunk[0];
                        pixels[at + 1] = chunk[1];
                        pixels[at + 2] = chunk[2];
                        pixels[at + 3] = 255;
                        changed += 1;
                    }
                }
            }
            let _ = changed;
        }
        let __t_put = std::time::Instant::now();
        let raw_hash = ctx.workspace.store().put(&pixels)?;
        let __put_ms = __t_put.elapsed();
        let __t_geom = std::time::Instant::now();
        let document = ctx.workspace.document_mut(&ctx.doc_id)?;
        // **越界区域是"被裁掉的"，不许静默** ✗（外部 agent 实测 ✓：900 宽画布上要 `{x:790,w:130}` ✓
        // ⇒ 实际只回 110 宽 ✓、**不报错** ✓ —— 它靠 `raw size != 期望字节数` 才发现 ✗，
        //   而别人很可能把"少了的那块"当成"那里本来就是空的" ✗）。
        // ⇒ **把请求的区域与实际的区域都报出来** ✓，并给一个 `clipped` 布尔 ✓（调用方一眼能判 ✓）。
        let (doc_w, doc_h) = (
            document.state().width as f64,
            document.state().height as f64,
        );
        let actual_x = region.x.max(0.0).min(doc_w);
        let actual_y = region.y.max(0.0).min(doc_h);
        let actual_w = (region.x + region.w).min(doc_w).max(actual_x) - actual_x;
        let actual_h = (region.y + region.h).min(doc_h).max(actual_y) - actual_y;
        let clipped = actual_w != region.w
            || actual_h != region.h
            || actual_x != region.x
            || actual_y != region.y;
        if __probe {
            // **∴ 一行说清"这 94 ms 花在哪"✗ ★**（**∴ 字节数也报 ✗ 免得把"图变大了"当成"变慢了" ✓）
            eprintln!(
                "PROBE raw 路 区域={}x{} 字节={} ｜render={:?}（**含命中 ✓）｜store.put={:?}｜几何+组装={:?}｜合计={:?}",
                width,
                height,
                pixels.len(),
                __render_ms,
                __put_ms,
                __t_geom.elapsed(),
                __t_all.elapsed()
            );
        }
        return Ok(json!({
            "ok": true,
            "head_seq": document.head_seq(),
            "width": width,
            "height": height,
            // **请求的** ✓ 与**实际给的** ✓ —— 两个都写 ✓。
            "requested": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
            "region": {"x": actual_x, "y": actual_y, "w": actual_w, "h": actual_h},
            "canvas": {"w": doc_w, "h": doc_h},
            "clipped": clipped,
            "raw_url": format!("yanshi://blob/{raw_hash}"),
            "mime_type": yanshi_render::RAW_RGBA_MIME,
            "warnings": ({
                let mut warnings = render_warnings.clone();
                if clipped {
                    warnings.push(format!(
                        "区域越界 ⇒ 已裁到画布内：请求 {}×{} @({},{})，实际 {}×{} @({},{}) ✓",
                        region.w, region.h, region.x, region.y, actual_w, actual_h, actual_x, actual_y
                    ));
                }
                warnings
            }),
            // **★ 该次渲染是否复用了下方合成 ✓ ★**（第 539 轮 ✓）：**∴ `raw` 出口也要标 ✗** ——
            // **∴ 就地读 ✓** —— **∴ 本分支在 `below_after` 之前 return ✗** ⇒ **∴ 不能引用它 ✓**。
            // **★ `below_reused` 必须**由 tile 账目决定 ✗ ★**（第 4 轮 ✓；
            //   借用 **GIMP `GimpProjection` 的口径**：**有效性是**tile 的属性**✗，
            //   **不是**整幅的属性 ✓ ⇒ **∴ 报"**复用了 ✓"**必须意味着**至少一格**真的来自缓存 ✓）：
            //   **∴ 为什么改 ✗**：**旧口径**读 `note_below_reuse()` 的**计数差**✗
            //     ⇒ **∴ 而**那条计数**会被**别的渲染路径**加 ✓**（**第 3 轮实测：
            //       `below_reused = true` **而** tile 账目 `available = 0, reused = 0` ✓）
            //       ⇒ **★ 于是**"**语义计数**"**并不语义 ✗** ⇒ **∴ 那是**撒谎 ✓ ★**** ✓✓
            //   **∴ 诚实做法 ✗**：**tile 为准 ✗**，**旧口径**另开一字段**并排报出 ✓**
            //     ⇒ **∴ 差**一眼可见 ✗**，**而不是**被藏起来 ✓**** ✓✓
            "below_reused": ctx.workspace.document_below_tiles(&ctx.doc_id).2 > 0,
            "below_reuse_noted": ctx.workspace.document_below_reuse_count(&ctx.doc_id) > below_before,
            // **∴ §6.3 的 ④ 也要在渲染响应里 ✓**（**∴ 所有出口 ✓）**：
            //   **∴ 没有 GPU ⇒ **没有比对 ⇒ **∴ 报 `null` ＋ 说明 ✓**（**不用 0 冒充 ✓）** ✓✓
            // **★★★ 必须报**实际后端**✗ ★★★**（**第 500 轮实测抓到 ✓）
            //   **∴ 原来的缺陷 ✗**：**这里**硬编码 `"cpu"`**✗
            //     ＋ **∴ 而**实测：**`/health` 报 `gpu`（**`gpu_stage_calls` 从 0 → 3 ✓）
            //       而**同一个渲染的响应**报 `cpu`** ✓
            //         ⇒ **∴ 于是**：**同一个事实**两个口径**✗ ⇒ **∴ 违反**
            //           「**`render_backend` 必须报实际后端**」** ✓ ★**** ✓✓
            "render_backend": backend_label(),
            "max_channel_delta": serde_json::Value::Null,
            "max_channel_delta_note": delta_note(),
            // **★ below tile 账目也要在几乎所有出口出现 ✗ ★**（第 2 轮 ✓）：
            //   **∴ 为什么 ✗**：**上一轮**我**只加了一个出口**✗ ⇒ **∴ 于是**另一条路
            //     **看不到字段 ✓**（**`None` ✓，**第 993 轮实测 ✓）⇒ **∴ 这次**两个出口都加 ✓**** ✓✓
            "below_tiles_wanted": ctx.workspace.document_below_tiles(&ctx.doc_id).0,
            "below_tiles_available": ctx.workspace.document_below_tiles(&ctx.doc_id).1,
            "below_tiles_reused": ctx.workspace.document_below_tiles(&ctx.doc_id).2,
            // **★ 缺格数也要报 ✗ ★**（第 24 轮 ✓）：**∴ 少了它** ⇒ **∴ 读的人**
            //   **只能看到 `available < wanted`**✗ ⇒ **∴ 于是**误以为"**矛盾 ✓" ✓**** ✓✓
            "below_tiles_missing": ctx.workspace.document_below_tiles(&ctx.doc_id).3,
            // **★ 归属字段也要在 `raw` 出口 ✗ ★**（第 994 轮 ✓；**补第 993 轮的半成品 ✓）：
            //   **∴ 为什么 ✗**：**我**上一轮**只加在**非 raw 出口**✗ ⇒ **∴ 于是**：**`raw`
            //     请求**看到的是 `None` ✓**（**实测 ✓）⇒ **∴ 无法**归属 ✓**** ✓✓
            //   **∴ 语义 ✗**：**本次请求期间**发生了几次渲染 ✓**
            //     ⇒ **∴ `0`** ⇒ **∴ 被缓存拦掉（**没进渲染器 ✓）**；**`≥1`** ⇒ **∴ 进了 ✓**** ✓✓
            "renders_done": yanshi_render::render::renders_done()
                .saturating_sub(renders_before),
            // **★ 快路径的指标 ✗ ★**：**∴ 命中整幅缓存 ⇒ 这里为真 ✓**
            "frame_reused": ctx.workspace.document_full_frame_hits(&ctx.doc_id) > frame_before,
            "above_reused": ctx.workspace.document_above_reuse_count(&ctx.doc_id) > above_before,
        }));
    }
    let region = parse_bbox(require_object(args, "region")?)?;
    let include_image = optional_bool(args, "include_image").unwrap_or(false);
    // **∴ 有 `layer_id` ⇒ **只渲那一层**✗；**没有 ⇒ **行为完全不变 ✓**。
    // **★ 埋点：渲染本体 ✗ ★**（**第 505 轮 ✓）
    //   **∴ 为什么 ✗**：**第 503 轮的标度实验说「**80% 是与像素成正比的计算**」 ✗
    //     ＋ **∴ 而** `timings` 里 `render_ms`／`raster_ms` **全是 0** ✓
    //       ⇒ **∴ 于是**：**报告方只能从外部反推** ✓（**`tools.rs:3352` 早就写明 ✓）
    //     ＋ **∴ 所以**：**这里补最要紧的一处 —— 渲染本体** ✓
    //       ⇒ **∴ 它是**图层合成 ＋ tile 管理的总入口** ✓ ★**** ✓✓
    let render_started = std::time::Instant::now();
    let preview = match layer_id.as_deref() {
        Some(id) => ctx.workspace.render_region_layer(&ctx.doc_id, region, id)?,
        None => ctx.workspace.render_region(&ctx.doc_id, region)?,
    };
    // **∴ 记进 `Phase::Render` ✗**：**∴ 于是** `/timings` 的 `render_ms` **第一次有实数** ✓ ★
    ctx.time(Phase::Render, render_started);
    let mut value = json!({
        "region": preview.bbox,
        "width": preview.width,
        "height": preview.height,
        "thumb_url": preview.url,
        "blob_hash": preview.blob_hash,
        "mime_type": preview.mime_type,
        "bytes": preview.bytes,
        "tiles": preview.tiles,
        "filter_padding": preview.filter_padding,
        "warnings": preview.warnings,
    });
    // **★ 该次渲染是否复用了下方合成 ✓ ★**（第 537 轮 ✓）：**∴ 语义计数取代全局计数 ✗**
    // ⇒ **∴ 不受笔触预览等其它渲染影响 ✓**（**目标第 7 条"懒诊断" ✓**）。
    // **∴ `after` 就地读 ✓** —— **∴ 与 `value` 同作用域 ✗**（**放到渲染那行后面会**跨作用域 ⇒ 编译失败 ✓**）。
    let below_after = ctx.workspace.document_below_reuse_count(&ctx.doc_id);
    let above_after = ctx.workspace.document_above_reuse_count(&ctx.doc_id);
    let frame_after = ctx.workspace.document_full_frame_hits(&ctx.doc_id);
    value["frame_reused"] = json!(frame_after > frame_before);
    // **∴ 非 raw 出口同口径 ✗**（**tile 为准 ＋ 并排报旧口径 ✓**）。
    let bt_reused2 = ctx.workspace.document_below_tiles(&ctx.doc_id).2;
    value["below_reused"] = json!(bt_reused2 > 0);
    value["below_reuse_noted"] = json!(below_after > below_before);
    // **∴ 同一组账目在非 raw 出口也要有 ✓**（**∴ 否则**两条路读数不一致 ✓）。
    let (bt_wanted, bt_available, bt_reused, bt_missing) =
        ctx.workspace.document_below_tiles(&ctx.doc_id);
    value["below_tiles_wanted"] = json!(bt_wanted);
    value["below_tiles_available"] = json!(bt_available);
    value["below_tiles_reused"] = json!(bt_reused);
    value["below_tiles_missing"] = json!(bt_missing);
    // **∴ 非 raw 出口同口径 ✗**（**∴ 所有出口 ✓）** ✓✓
    // **∴ 同上：**报实际后端 ＋ 诚实的说明**✗**（**第 500 轮 ✓）★
    value["render_backend"] = json!(backend_label());
    value["max_channel_delta"] = serde_json::Value::Null;
    value["max_channel_delta_note"] = json!(delta_note());
    value["above_reused"] = json!(above_after > above_before);
    // **∴ 归属字段 ✓**：**本次请求期间**发生了几次渲染 ✗（**进程内单调 ✓）**
    //   ⇒ **∴ 判据**可以**区分"**没命中**✗"与"**根本没渲染**✓"** ✓**** ✓✓
    value["renders_done"] =
        json!(yanshi_render::render::renders_done().saturating_sub(renders_before));
    // **出参 path 形态**（设计文档第 4 章 4.2/4.5）：给了 `output_path` ⇒ 把预览的
    // **全尺寸** PNG 直接落盘（守卫与 export_png 共用 `guarded_output_path`），
    // 响应只加 `image_file` 元数据、**不再走下面的内嵌/缩放 machinery**——文件形态
    // 的意义正是绕开 `max_px` 与响应体积。不给 ⇒ 以下行为逐字不变。
    if let Some(raw_path) = optional_str(args, "output_path") {
        let target = guarded_output_path(&raw_path)?;
        let png = ctx.workspace.store().get(&preview.blob_hash)?;
        let overwritten = target.exists();
        std::fs::write(&target, &png).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("写文件失败：{} ⇒ {error}", target.display())),
            )
        })?;
        value["image_file"] = json!({
            "path": target.display().to_string(),
            "bytes": png.len(),
            "width": preview.width,
            "height": preview.height,
            "mime_type": preview.mime_type,
            "overwritten": overwritten,
        });
        return Ok(value);
    }
    // 7.5：MCP 响应可选内嵌 image content ✓ —— **超限不再静默降级** ✗（第三方 MCP 实测报告 P0-1 ✓：
    // "画 50 笔之前看不到任何效果，只能盲画" ✗ ⇒ 调用方拿到 `yanshi://blob/…` 却**不知道为什么不给图** ✗）。
    // `max_px`（缺省 512 ✓）是本工具自己声明的上限；**超了就明说** ✓ + 给出出路 ✓，而不是只回地址 ✗。
    let max_px = optional_count(args, "max_px")?
        .unwrap_or(512)
        .clamp(1, 8192) as f64;
    if include_image && (f64::from(preview.width) > max_px || f64::from(preview.height) > max_px) {
        // **方案 A：超限时默认就给一张缩小的图** ✓（报告 P0-1 ✓："画 50 笔之前看不到任何效果" ✗）。
        // 复用**既有**重采样与编码 ✓（`yanshi_core::resample::resample_rgba` ✓ +
        // `yanshi_render::png::encode_png` ✓）⇒ **不新增第二套缩放实现** ✗（本项目的老规矩 ✓）。
        let ratio = max_px / f64::from(preview.width.max(preview.height));
        let scaled_w = ((f64::from(preview.width) * ratio).round().max(1.0)) as u32;
        let scaled_h = ((f64::from(preview.height) * ratio).round().max(1.0)) as u32;
        // **★ 埋点：缩放 ＋ PNG 编码 ✗ ★**（**第 521 轮 ✓）
        //   **∴ 为什么 ✗**：**实测（4K，`include_image: true`）**比不带图多花 **≈989 ms**✗
        //     ＋ **∴ 而** `/timings` 的 `png_ms` **报 0.0** ✓
        //       ⇒ **∴ 于是**：**那笔钱**完全没被记账**（与 `render_ms` 曾是残差同类 ✓）★
        //   **∴ 说明 ✗**：**这段包含**两件事**✗ ：**`resample_rgba`（缩放 ✓）
        //     ＋ **`encode_png`** ✓ ⇒ **∴ 所以** `png_ms` 报的是**两者之和** ✓（**如实 ✓）★
        let png_started = std::time::Instant::now();
        // **★★★ 先试**复用已经渲好的像素**✗ ★★★**（**第 528 轮 ✓）
        //   **∴ 为什么 ✗**：**`Document::render_region` 内部本来就持有 `rendered.rgba8`**✗
        //     （**紧挨着 `encode_png(…, &rendered.rgba8)` ✓）
        //     ＋ **∴ 而**它**原来返回时丢了它** ✓
        //       ⇒ **∴ 于是**：**这里为了缩略图**又完整渲染一遍** ✓
        //         ⇒ **★ 实测：一个带图请求把 4K 区域**渲染了 2 次**** ✓
        //           ＋ **∴ 而** `png_ms ≈ 1012 ms` ＝ 那一次 ✓（**占该请求 ~47% ✓）★
        //   **∴ 现在 ✗**：**优先用 `preview.rgba8`**✗
        //     ＋ **∴ 万一**它没有（**别的出口 ✓）⇒ **∴ 才**回退到 raw 路** ✓
        //       ⇒ **∴ 所以**：**行为对旧出口**完全不变** ✓ ★
        //   **∴ 正确性 ✗**：**同一区域、同一显示空间 RGBA** ✓
        //     ＋ **∴ 判据 ✗**：`tool-render-count.mjs`（**渲染次数 ≤ 1 ✓）
        //       ＋ **∴ 且**改前存了 PNG 哈希 ⇒ **∴ 要求**逐位相同** ✓ ★
        let scaled = match preview.rgba8.as_deref() {
            // **∴ 快路 ✗**：**已经渲好的像素 ⇒ **∴ 不再渲染第二遍** ✓ ★
            Some(rgba) => yanshi_core::resample::resample_rgba(
                rgba,
                preview.width,
                preview.height,
                scaled_w,
                scaled_h,
                yanshi_core::resample::ResampleFilter::Bilinear,
            ),
            // **∴ 慢路（**旧出口的兜底 ✓）✗**：**它**会**再渲染一遍**✗
            //   ＋ **∴ 保留它 ⇒ **∴ 行为对没有像素的出口**完全不变** ✓ ★
            None => ctx
                .workspace
                .render_region_raw_active(&ctx.doc_id, region, active_layer.as_deref())
                .ok()
                .and_then(|(source_w, source_h, rgba)| {
                    yanshi_core::resample::resample_rgba(
                        &rgba,
                        source_w,
                        source_h,
                        scaled_w,
                        scaled_h,
                        yanshi_core::resample::ResampleFilter::Bilinear,
                    )
                }),
        }
        .and_then(|small| yanshi_render::png::encode_png(scaled_w, scaled_h, &small));
        // **∴ 记进 `Phase::Png` ✗**：**∴ 于是** `/timings` 的 `png_ms` **第一次有实数** ✓ ★
        ctx.time(Phase::Png, png_started);
        if let Some(png) = scaled {
            value["image"] = json!({
                "mime_type": "image/png",
                "data": base64::encode(&png),
                "width": scaled_w,
                "height": scaled_h,
            });
            value["scaled_from"] = json!([preview.width, preview.height]);
            value["scaled_note"] = json!(format!(
                "原区域 {}×{} 超过 max_px={} ⇒ 已**自动缩小**到 {}×{} 内嵌 ✓；要看原尺寸请传 max_px={}",
                preview.width, preview.height, max_px as u64, scaled_w, scaled_h, preview.width.max(preview.height)
            ));
            return Ok(value);
        }
        value["image_omitted"] = json!(true);
        value["image_omitted_reason"] = json!(format!(
            "区域 {}×{} 超过 max_px={} ⇒ 未内嵌 base64（避免超大响应）；\n             出路：① 传更小的 region（例如只取刚画过的那块）✓；② 传 max_px={} 提高上限 ✓；\n             ③ 先用 export_png 落盘再自己看 ✓；④ 只按 thumb_url 走 HTTP 取图 ✓",
            preview.width, preview.height, max_px as u64, preview.width.max(preview.height)
        ));
        value["max_px"] = json!(max_px as u64);
    }
    if include_image && f64::from(preview.width) <= max_px && f64::from(preview.height) <= max_px {
        let png = ctx.workspace.store().get(&preview.blob_hash)?;
        value["image"] = json!({
            "mime_type": preview.mime_type,
            "data": base64::encode(&png),
            "width": preview.width,
            "height": preview.height,
        });
    }
    Ok(value)
}

pub(super) fn read_get_log(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **计数族必须走 `optional_count`** ✓（第 716 轮；外部审计 S-01）
    //   **⇒ 缺陷**：`optional_u64` = `Value::as_u64` ⇒ 对 `-1` 与 `5.0` **一律 `None`** ✗
    //     而 `validate_args` 对 `ParamKind::Integer` **放行**这两类 ✓
    //     ⇒ `.unwrap_or(缺省)` **静默接管** ✗ ⇒ `since_seq: -1` 被当成 `0`（**返回全量、回执一切正常** ✓）
    //     而 `limit: -5` 被当成 `200` ✓ —— 正是本仓的「静默降级形态 4」✓。
    //   **⇒ 修法**：用审计补丁带来的 `optional_count`（负数报错、整数浮点接受 ✓）。
    let since = optional_count(args, "since_seq")?.unwrap_or(0);
    let limit = optional_count(args, "limit")?.unwrap_or(200).min(2000) as usize;
    let kind_filter = optional_str(args, "kind");
    let actor_filter = optional_str(args, "actor");
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    // **★ `order`：**最新的 N 条**且**从新到旧** ✗ ★**（第 187 轮 ✓；**PWA 实测 P2-4 ✓）：
    //   **∴ 为什么需要它 ✗**：**原来的 `.take(limit)` 跑在**升序**迭代器上**✗
    //     ⇒ **∴ 于是** `limit: 200` 取的是**最早的 200 条** ✓
    //       ⇒ **∴ 而**用户在界面上**要的是**最新的 200 条** ✓**** ✓✓
    //       ⇒ **∴ 而**前端**只**做 `.slice().reverse()`**✗
    //         ⇒ **∴ 那**顶多得到**"**最早 200 条里的倒序 ✓"**✗
    //           ⇒ **∴ 而**不是**"**最新 200 条 ✓" ✓（**∴ 上游 README **也**承认这是半修 ✓）** ✓✓
    //   **∴ 做法 ✗**：**按**索引序**取 ✗（**∴ 升序： `0..n` ✗；**降序： `(0..n).rev()` ✓）**
    //     ⇒ **∴ 于是** `take(limit)` **先**截断**✗
    //       ⇒ **∴ 昂贵的那段 `map(json!)` **只**跑 `limit` 次 ✓ ★**** ✓✓
    let log = document.log();
    let log_len = log.atoms().len();
    let newest_first = matches!(
        optional_str(args, "order").as_deref(),
        Some("desc") | Some("newest_first")
    );
    let order_indices: Box<dyn Iterator<Item = usize>> = if newest_first {
        Box::new((0..log_len).rev())
    } else {
        Box::new(0..log_len)
    };
    let atoms: Vec<Value> = order_indices
        .map(|index| &log.atoms()[index])
        .filter(|atom| atom.seq > since)
        .filter(|atom| {
            kind_filter
                .as_deref()
                .is_none_or(|kind| atom.kind.as_str() == kind)
        })
        .filter(|atom| {
            actor_filter
                .as_deref()
                .is_none_or(|actor| atom.actor == actor)
        })
        .take(limit)
        .map(|atom| {
            json!({
                "atom_id": atom.id,
                "seq": atom.seq,
                "kind": atom.kind,
                // **`origin`：来源，不是存储形态** ✓（第 781 轮；外部 `qoder` 的 D5）
                //   **⇒ 他们的实测**：一幅 8K 画的 39,354 条笔触 `kind` **全是 `import_image`** ✗
                //     ⇒ 拿 `kind` 做来源审计会把"画出来的 38k 笔"误判成"导入的一张图" ✓。
                //   **⇒ 为什么这里也要给**：`list_atoms` 早就有 `origin` ✓（第 642／659 轮），
                //     而 `get_log` 没有 ✗ ⇒ 他们的脚本走的是 `get_log` ✓ ⇒ 只改一边 = 没修 ✓
                //     （这正是本仓"两条接口不同形"的老坑 ✓）。
                //   **⇒ 口径与 `list_atoms` 完全一致**：落账时写死的 `payload.origin` 优先 ✓，
                //     老日志退回按 `kind`＋`payload` 派生 ✓（`origin_of` ✓）。
                "origin": payload_str(atom, "origin")
                    .unwrap_or_else(|| crate::tools::origin_of(atom.kind, &atom.payload).to_owned()),
                "actor": atom.actor,
                "session": atom.session,
                "changeset_id": atom.changeset_id,
                "timestamp": atom.timestamp,
                "heavy": atom.is_heavy(),
                "state_effect": atom.kind.is_state_effect(),
                // **★ 历史行需要 `payload` ✓**（用户报告 2026-10-09 ✓／第 449 轮 ✓）：
                // **∴ 实测** ✓：**页面的历史面板走 `get_log`** ✓（`viewer-app.js:3720` ✓），
                // 而它**此前不返回 `payload`** ✗ ⇒ **∴ 前端拿不到 `layer_id`／`width` ⇒
                // **∴ 行里只能显示 `import_image` ✗**（**＝用户的原始抱怨 ✓**）。
                // **⚠️ 两条接口**不同形**是本项调查最贵的教训 ✓**：**我实测的是 `/api/atoms`** ✓
                //（**它含 `payload` ✓**）而改的是这条 ✗ ⇒ **∴ "测了 A 改了 B" ⇒ 判据误绿 ✓**。
                // **∴ 代价**（两面 ✓）：**响应变大 ✗**，但 `payload` 里是**引用与尺寸**（**非像素 ✓**）
                // ⇒ 体积小 ✓；且 `limit` 有上限 ✓。
                "payload": atom.payload,
            })
        })
        .collect();
    let next_since = atoms
        .iter()
        .filter_map(|atom| atom["seq"].as_u64())
        .max()
        .map_or_else(|| json!(since), |seq| json!(seq));
    Ok(json!({
        "atoms": atoms,
        "count": atoms.len(),
        "head_seq": document.head_seq(),
        // **游标必须是"这一页里最大的 seq"** ✓（第 734 轮；外部审计 S-07 第 ① 项）
        //   **⇒ 缺陷**：取 `atoms.last()` ✗ ⇒ `order: "newest"` 时**最后一条是最旧的** ✓
        //     ⇒ 游标**往回退** ✗ ⇒ 调用方拿它续读会**重复**上一页（甚至永远推不动 ✓）。
        //   **⇒ 恒等式**（判据就断它 ✓）：`next_since` = 本页 `seq` 的**最大值** ✓，
        //     与 `order` 无关 ✓ —— "我已经看到哪了"只取决于**最大的那个序号** ✓。
        "next_since": next_since,
    }))
}

pub(super) fn read_get_job(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let job_id = require_str(args, "job_id")?;
    let document = ctx.workspace.document_mut(&ctx.doc_id)?;
    let job = document.jobs_mut().get(&job_id, ctx.now)?;
    Ok(json!({
        "job_id": job.id,
        "kind": job.kind,
        "session": job.session,
        "status": job.status,
        "atom_id": job.atom_id,
        "error": job.error,
        "progress": job.progress,
        "result": job.result,
        "created_at": job.created_at,
        "ttl": job.ttl,
        "terminal": job.status.is_terminal(),
    }))
}

pub(super) fn read_get_render_status(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let atom_id = require_str(args, "atom_id")?;
    let status = ctx.workspace.render_status(&ctx.doc_id, &atom_id)?;
    // **缩略图有没有落后** ✗（本轮新增 ✓）：`wait_for_render=true` 承诺的是
    // **这一笔的渲染**（`rendered` / `rendered_seq` ✓），**不是**文档级缩略图 ✓ ——
    // 后者是一份缓存 ✓，由"谁要新鲜谁显式要"的口径刷新 ✓（`get_document` ✓）。
    // 调用方需要能**看见**这件事 ✓，而不是从 `thumb_url` 里猜 ✗。
    // `false` ⇒ `thumb_url` 指向的是**旧画面** ✓，此刻去 `get_document` 才会当场重建 ✓。
    let thumbnail_current = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| document.document_thumbnail_is_current())
        .unwrap_or(false);
    Ok(json!({
        "rendered": status.rendered,
        "head_seq": status.head_seq,
        "rendered_seq": status.rendered_seq,
        "atom_seq": status.atom_seq,
        "thumb_url": status.thumb_url,
        // **与 `rendered` 是两件事** ✓：`rendered=true` ＋ `thumbnail_current=false`
        // 是完全正常的组合 ✓（这一笔的像素已经渲染出来了 ✓，只是那张 256² 缓存还没跟上 ✓）。
        "thumbnail_current": thumbnail_current,
    }))
}

pub(super) fn read_get_checkpoints(ctx: &mut ToolContext<'_>) -> Result<Value> {
    let state = document_state(ctx)?;
    let checkpoints: Vec<Value> = state
        .checkpoints
        .values()
        .map(|checkpoint| {
            json!({
                "checkpoint_id": checkpoint.id,
                "name": checkpoint.name,
                "created_by": checkpoint.created_by,
                "message": checkpoint.message,
                "anchor_seq": checkpoint.anchor_seq,
                "snapshot_id": checkpoint.snapshot_id,
            })
        })
        .collect();
    Ok(json!({"checkpoints": checkpoints, "count": checkpoints.len()}))
}

/// **`stroke_heatmap`**（反馈 5.6）：**纯读** —— 把一块区域划成网格，每格给
/// 笔数／点数／墨量。笔数与点数走与 `get_stroke_log` **同一扫描**（`source.kind`
/// ∈ {brush, medium, draw} 的原子，`DrawStroke`／`Erase` 回落 `payload.data`，
/// 已撤销原子按最近一次动作排除）；墨量是烘焙位图 tile 的逐像素 alpha 求和
/// （与 `brush_stroke` 回执的 `ink_mass` 同口径：Σalpha），只覆盖带 tile 的笔。
/// **口径缺口必须明说**：`medium_stroke` 经 `write_import_image` 提交的是无
/// `source` 的 `ImportImage` 原子，日志扫描看不见它 ⇒ 介质笔不在热力图里；
/// `draw_stroke` 笔无 tile ⇒ 只计笔数与点数，墨量不含它们（缺口以
/// `strokes_without_ink` 计数明示，不以 0 冒充“没用墨”）。
/// **不改 `analyze_region`**：那只读最终像素颜色，本工具读笔触归属，两者互补。
pub(super) fn read_stroke_heatmap(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let region = parse_bbox(require_object(args, "region")?)?;
    if !region.x.is_finite()
        || !region.y.is_finite()
        || !region.w.is_finite()
        || !region.h.is_finite()
        || region.w <= 0.0
        || region.h <= 0.0
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "region 的宽高必须大于 0（且坐标有限）；区域可部分在画布外，但不能是空盒"
                    .to_string(),
            ),
        ));
    }
    let cell_size = match args.get("cell_size") {
        None | Some(Value::Null) => 64.0,
        Some(value) => value.as_f64().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("cell_size 必须是数字（画布像素，缺省 64）".to_string()),
            )
        })?,
    };
    if !cell_size.is_finite() || cell_size <= 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("cell_size 必须是有限正数（缺省 64）".to_string()),
        ));
    }
    // **格数上限是防炸护栏**（约定值，不是引擎临界值）：`cell_size = 1` 扫 4K
    // 区域会炸出 1600 万格 ⇒ 超限就报错并指路调大 `cell_size`，不静默夹紧
    // （夹紧会让调用方拿到与请求不符的网格而不自知）。
    let cols_f = (region.w / cell_size).ceil();
    let rows_f = (region.h / cell_size).ceil();
    if cols_f * rows_f > 10_000.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "网格 {cols_f}×{rows_f} 超过 10000 格上限 ⇒ 请调大 cell_size（当前 {cell_size}）"
            )),
        ));
    }
    let cols = cols_f as usize;
    let rows = rows_f as usize;
    let document = ctx.workspace.document(&ctx.doc_id).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("文档 {} 未打开", ctx.doc_id)),
        )
    })?;
    // **已撤销集合与 `get_stroke_log` 同规则**：按最近一次动作算，revert 之后
    // 又 reapply 回来的算 live。
    let mut last_action: BTreeMap<String, (u64, bool)> = BTreeMap::new();
    for atom in document.log().iter() {
        if atom.kind != AtomKind::Revert && atom.kind != AtomKind::Reapply {
            continue;
        }
        if let Some(target) = atom.payload.get("target").and_then(Value::as_str) {
            last_action.insert(target.to_owned(), (atom.seq, atom.kind == AtomKind::Revert));
        }
    }
    let reverted: BTreeSet<String> = last_action
        .iter()
        .filter(|(_, (_, is_revert))| *is_revert)
        .map(|(id, _)| id.clone())
        .collect();
    // **半开区间归属**：右／下边界上的点与像素不属于本区域（与
    // `Bbox::intersects`“只贴边不算相交”同精神）。
    let col_of = |x: f64| -> Option<usize> {
        if x < region.x || x >= region.x + region.w {
            return None;
        }
        let col = ((x - region.x) / cell_size).floor() as usize;
        if col < cols {
            Some(col)
        } else {
            None
        }
    };
    let row_of = |y: f64| -> Option<usize> {
        if y < region.y || y >= region.y + region.h {
            return None;
        }
        let row = ((y - region.y) / cell_size).floor() as usize;
        if row < rows {
            Some(row)
        } else {
            None
        }
    };
    let mut cell_strokes = vec![0u64; cols * rows];
    let mut cell_points = vec![0u64; cols * rows];
    let mut cell_ink = vec![0u64; cols * rows];
    let mut total_strokes = 0u64;
    let mut total_points = 0u64;
    let mut strokes_without_ink = 0u64;
    for atom in document.log().iter() {
        // **扫描谓词与 `get_stroke_log` 逐字同款**（见文件头注释的口径缺口）：
        // `source` 不是对象时只对 `DrawStroke`／`Erase` 回落 `payload.data`。
        let owned_source;
        let source = match atom.payload.get("source").filter(|value| value.is_object()) {
            Some(source) => source,
            None => {
                if !matches!(atom.kind, AtomKind::DrawStroke | AtomKind::Erase) {
                    continue;
                }
                let Some(data) = atom.payload.get("data").filter(|value| value.is_object()) else {
                    continue;
                };
                owned_source = json!({
                    "kind": "draw",
                    "points": data.get("points").cloned().unwrap_or(Value::Null),
                });
                &owned_source
            }
        };
        let source_kind = source
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(source_kind, "brush" | "medium" | "draw") {
            continue;
        }
        if reverted.contains(&atom.id) {
            continue;
        }
        let points: Vec<(f64, f64)> = source
            .get("points")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let pair = item.as_array()?;
                        Some((pair.first()?.as_f64()?, pair.get(1)?.as_f64()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // **可读 tile 才算墨**：哈希非法、取不到、字节长 ≠ tile 的 `w × h × 4`
        // 时墨量跳过（不整单报错，缺口计入 `strokes_without_ink`）；分块索引
        // （`payload.tiles`）只追加，不动主 blob ⇒ 主 blob 仍是整幅 RGBA。
        let tile: Option<(f64, f64, usize, usize, Vec<u8>)> = (|| {
            let tile_box = atom.payload.get("region").and_then(Bbox::from_value)?;
            let hash_text = atom.payload.get("bitmap")?.get("blob_hash")?.as_str()?;
            let hash: yanshi_core::BlobHash = hash_text.parse().ok()?;
            let bytes = ctx.workspace.store().get(&hash).ok()?;
            if !tile_box.w.is_finite()
                || !tile_box.h.is_finite()
                || tile_box.w <= 0.0
                || tile_box.h <= 0.0
                || tile_box.w.fract() != 0.0
                || tile_box.h.fract() != 0.0
            {
                return None;
            }
            let width = tile_box.w as usize;
            let height = tile_box.h as usize;
            if width.checked_mul(height)?.checked_mul(4)? != bytes.len() {
                return None;
            }
            Some((tile_box.x, tile_box.y, width, height, bytes))
        })();
        let mut touched: BTreeSet<usize> = BTreeSet::new();
        for (px, py) in &points {
            if let (Some(col), Some(row)) = (col_of(*px), row_of(*py)) {
                let index = row * cols + col;
                touched.insert(index);
                cell_points[index] += 1;
                total_points += 1;
            }
        }
        if let Some((tile_x, tile_y, tile_w, _, bytes)) = &tile {
            for (index, pixel) in bytes.chunks_exact(4).enumerate() {
                // alpha = 0 的像素是没落笔的透明处，不是墨 ⇒ 不计入。
                let alpha = u64::from(pixel[3]);
                if alpha == 0 {
                    continue;
                }
                let gx = tile_x + (index % tile_w) as f64;
                let gy = tile_y + (index / tile_w) as f64;
                if let (Some(col), Some(row)) = (col_of(gx), row_of(gy)) {
                    let cell = row * cols + col;
                    touched.insert(cell);
                    cell_ink[cell] += alpha;
                }
            }
        } else if let Some(bbox) = atom.payload.get("region").and_then(Bbox::from_value) {
            // **无 tile 的笔走 `bbox` 兜底**：`draw_stroke` 的 `bbox` 是点列包围盒
            // 扩 `size / 2 + 2`，即真实落墨范围 ⇒ 笔尖擦边区域也能计入；有 tile
            // 的笔不走这条（tile 像素已是精确归属，`bbox` 会把透明边也算进来）。
            for row in 0..rows {
                for col in 0..cols {
                    let cx = region.x + col as f64 * cell_size;
                    let cy = region.y + row as f64 * cell_size;
                    let cell = Bbox::new(
                        cx,
                        cy,
                        (region.x + region.w - cx).min(cell_size).max(0.0),
                        (region.y + region.h - cy).min(cell_size).max(0.0),
                    );
                    if bbox.intersects(&cell) {
                        touched.insert(row * cols + col);
                    }
                }
            }
        }
        if !touched.is_empty() {
            total_strokes += 1;
            for index in &touched {
                cell_strokes[*index] += 1;
            }
            if tile.is_none() {
                strokes_without_ink += 1;
            }
        }
    }
    let mut cells: Vec<Value> = Vec::with_capacity(cols * rows);
    let mut total_ink_mass = 0u64;
    for row in 0..rows {
        for col in 0..cols {
            let index = row * cols + col;
            let x = region.x + col as f64 * cell_size;
            let y = region.y + row as f64 * cell_size;
            total_ink_mass += cell_ink[index];
            cells.push(json!({
                "col": col,
                "row": row,
                "x": x,
                "y": y,
                "w": (region.x + region.w - x).min(cell_size).max(0.0),
                "h": (region.y + region.h - y).min(cell_size).max(0.0),
                "strokes": cell_strokes[index],
                "points": cell_points[index],
                "ink_mass": cell_ink[index],
            }));
        }
    }
    Ok(json!({
        "ok": true,
        "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
        "cell_size": cell_size,
        "cols": cols,
        "rows": rows,
        "cells": cells,
        "total_strokes": total_strokes,
        "total_points": total_points,
        "total_ink_mass": total_ink_mass,
        "strokes_without_ink": strokes_without_ink,
        "note": "笔数／点数走与 get_stroke_log 同一日志扫描（已撤销排除）；墨量是烘焙位图 tile 的逐像素 alpha 求和，只覆盖带 tile 的笔（实为笔刷笔）。medium_stroke 的介质笔不在扫描内（无 source 的 ImportImage 原子），draw_stroke 笔只计笔数与点数、墨量不含它们；strokes_without_ink 是对区域有贡献但无可读 tile 的去重笔数。一笔进多格时每格计 1，total_strokes 去重只计 1；total_ink_mass = 各格 ink_mass 之和，区域盖住整笔 tile 时等于该笔回执的 ink_mass。",
    }))
}
