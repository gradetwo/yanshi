//! **`wait_for_render` 等的是"这一笔的渲染"，不是那张 256² 文档缩略图** ✓
//! （外部 4K/8K 性能报告 `apple2011:YANSHI_401AC89_4K_8K_PERFORMANCE_REPORT.md` 的结构性那一半 ✓）。
//!
//! ## 报告里的证据（本文件要守的东西）
//!
//! 8K 画布上一笔 `2B_pencil`：
//!
//! | 模式 | 墙钟 | 像素 |
//! |---|---|---|
//! | 默认（同步） | **39,505.8 ms** | —— |
//! | `wait_for_render=false` | **95.1 ms**（**415.8×** ✓） | **与上面逐像素完全一致** ✓ |
//!
//! 10 笔 `batch`：**360,418 ms（6.0 分钟）** vs **1,250 ms（288.3×）**，同样逐像素一致 ✓。
//!
//! 报告自己的根因陈述 ✓：笔刷光栅化**不是**慢的那一半 ✓，而是**提交收尾**在同一调用里
//! 走了一遍**文档级预览**（`render_document_preview` ✓ —— 只为产出一张 256² 缩略图 ✓）。
//! "两种模式像素完全一致"这句话本身就是证据 ✓：那一次渲染**不是**调用方要的东西 ✓。
//!
//! ## 本文件之前的那一半（已合并）与本轮的那一半
//!
//! * **已合并**（第 1182 轮）：渲染器对相交位图补丁**每次渲染都 `store.get` ＋ 整块 inflate** ✗
//!   ⇒ 加了跨渲染 `BitmapCache` ✓。它让**每一次预览都变便宜** ✓，
//!   但**没有取消那次预览** ✗ —— 4K 工程包实测：`other_ms` 11,000 → 1.1-1.8 ms ✓，
//!   而那 10.4 秒**换了个地方出现**：新的 `preview_ms` 阶段 ✓（第一笔整幅 inflate ✓）。
//! * **本轮**（本文件守的）：那次**文档级预览**本身不再由提交收尾无条件触发 ✓ ——
//!   它改为**按需**产生 ✓（`ensure_document_thumbnail` / `get_document` / `GET …/preview` ✓）。
//!
//! ## 契约（调用方现在观察到什么）
//!
//! * `wait_for_render=true` **仍然是**"返回时这一笔的渲染已完成" ✓ ——
//!   兑现它的是**调用方要的那次渲染**（这一笔的脏区 ✓，走 `Workspace::render_region` ✓）：
//!   它推进渲染水位 ✓、跑完 `complete_render_jobs()` ✓ ⇒ `render_status.rendered == true` ✓、
//!   `job_status == "committed"` ✓（判据 3 守着 ✓）。
//! * 响应里的 `preview` **仍是这一次改动脏区的真实渲染** ✓，**逐字节与改动前相同** ✓（判据 2 ✓）。
//! * **文档级缩略图会滞后** ✗：它是一份**缓存** ✓，落后时 `document_thumbnail_is_current()` 为 false ✓
//!   ⇒ **谁要新鲜谁显式要** ✓：`get_document`（`preview_size` ✓）/ `GET /api/documents/<id>/preview`
//!   / `ensure_document_thumbnail` ✓ —— 它们本来就写着"落后就重建" ✓，
//!   而且**绝不会拿旧图冒充 HEAD** ✗（判据 2 用"让缓存检查恒真"这个变异把它变红 ✓）。
//! * `wait_for_render=false`：**一个字都没变** ✓（本来就不走任何渲染 ✓，判据 1 的第一段守着 ✓）。
//!
//! ## 判据（每条都实测过"怎么变红"，红法写在各自的注释里）
//!
//! 1. [`a_stroke_that_did_not_ask_for_the_document_preview_never_runs_one`]：**语义计数** ✓
//!    （`document_preview_render_count()` ＋ `full_canvas_render_count()` ＋ 位图解码字节数 ✓），
//!    **不是墙钟** ✗（debug 下同一段能差 3-5 倍 ✓，见 `tests/background_stroke_cost.rs` 的记录 ✓）。
//! 2. [`the_final_pixels_and_thumbnail_match_the_eagerly_refreshed_ones`]：像素与缩略图**逐字节** ✓
//!    与"改动前那种每笔都刷缩略图"的做法比 ✓。
//! 3. [`wait_for_render_true_still_returns_after_the_render_is_complete`]：契约**没被偷偷削弱** ✓。
//! 4. 既有的判据（**点名，不另写第三套** ✓，见本文件末尾的清单 ✓）：
//!    `tests/background_stroke_cost.rs`（跨渲染缓存的语义计数 ✓）✓、
//!    `tests/render_parity.rs`（服务端/内核逐位一致 ✓）✓、
//!    `tests/timings_and_cancel.rs`（各相之和 == 总时长 ✓）✓、
//!    `document::tests::heavy_atoms_create_jobs_and_render_completes_them` ✓、
//!    `service_flow::jobs_ttl_cancel_and_render_watermark` ✓、
//!    `service_flow::document_thumbnail_is_cover_whole_canvas_not_the_last_region` ✓。

use serde_json::{json, Value};
use yanshi_server::{
    DocThumbSize, DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace,
};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yanshi_preview_wait_{}_{}_{}",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 ＋ 仓库里的 `assets/`（`brush_stroke` 与 `texture_background` 都要真实资产 ✓）。
fn workspace(root: &std::path::Path, doc_id: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new(doc_id, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// 一次工具调用（`wait_for_render` 与 `silent` 都显式给 ✓ —— 本文件测的就是这两个开关的语义 ✓）。
fn call(
    workspace: &mut Workspace,
    doc_id: &str,
    tool: &str,
    args: Value,
    wait_for_render: bool,
    silent: bool,
) -> Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(wait_for_render, 4_000);
    ctx.silent = silent;
    registry().call(&mut ctx, tool, &args)
}

/// 建"art 图层 ＋ 整幅纹理背景"——与报告的场景同形（**一笔无关的整幅补丁** ✓）。
fn document_with_background(workspace: &mut Workspace, doc_id: &str) {
    let made = call(
        workspace,
        doc_id,
        "create_layer",
        json!({"layer_id": "art"}),
        true,
        false,
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let made = call(
        workspace,
        doc_id,
        "texture_background",
        json!({"texture": "Paper001.png"}),
        true,
        false,
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// 一笔（位置随笔号平移 ✓，其余参数完全一致 ✓ ⇒ 唯一变量是"第几笔"✓）。
fn stroke_args(index: i32) -> Value {
    let y = 60.0 + f64::from(index) * 24.0;
    json!({
        "layer_id": "art",
        "object_id": format!("stroke{index}"),
        "brush": "2B_pencil",
        "size": 16,
        "points": [[40.0, y, 0.6], [200.0, y + 8.0, 0.8]],
    })
}

/// **语义计数三件套** ✓（都不是墙钟 ✗）：
/// (文档级预览渲染次数, 真正整幅渲染次数, 未命中时解出的明文字节数)。
#[derive(Debug, PartialEq, Eq)]
struct Counters {
    document_previews: usize,
    full_canvas: usize,
    missed_bytes: u64,
}

fn counters(workspace: &Workspace, doc_id: &str) -> Counters {
    let document = workspace.document(doc_id).expect("文档应当在");
    Counters {
        document_previews: document.document_preview_render_count(),
        full_canvas: document.full_canvas_render_count(),
        missed_bytes: document.bitmap_cache_stats().missed_bytes,
    }
}

fn blob_of(workspace: &Workspace, url: &str) -> Vec<u8> {
    let hash: yanshi_core::atom::BlobHash = url
        .trim_start_matches("yanshi://blob/")
        .parse()
        .unwrap_or_else(|error| panic!("blob 地址 {url} 非法：{error}"));
    workspace
        .store()
        .get(&hash)
        .unwrap_or_else(|error| panic!("取 blob 失败：{error}"))
}

/// PNG 头里的宽高 ✓（判据要确认自己比的是"缩略图 vs 缩略图"✓，不是"整幅 vs 缩略图"✗）。
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(
        bytes.len() >= 24 && bytes.starts_with(&[0x89, b'P', b'N', b'G']),
        "不是一张 PNG"
    );
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

// ---------------------------------------------------------------------------
// 判据 1：没有请求文档级预览的那一笔，绝不再跑一次文档级预览
// ---------------------------------------------------------------------------

/// **判据 1（语义计数）**：一笔"没有请求文档级预览"的笔触，**一次文档级预览都不跑** ✓。
///
/// **三种调用形态都验** ✓（它们都**没有**向服务端要"刷新那张 256² 缩略图"✓）：
/// * `wait_for_render=false`：调用方**明确说不要渲染** ✓ ⇒ 连**任何**渲染都不许发生 ✓；
/// * `wait_for_render=true`（缺省）：调用方要的是**这一笔的渲染** ✓ ⇒ 响应里的 `preview`
///   就是它 ✓，而**文档级预览**（缩略图那一次）不该被顺带跑掉 ✗；
/// * `silent=true`：调用方说"批量静默提交" ✓ —— 连响应里的预览都不要 ✓，
///   那就更不该为它跑一次文档级预览 ✗（它此前**照样跑** ✗）。
///
/// 另外单独钉一条 ✗：`silent=true` 打**轻型原子**（没有 job 要跑 ✓，例如 `draw_shape` ✓）时，
/// **一个像素都不该渲染** ✓ —— `silent` 的初衷就是"批量静默提交**不**产生额外 IO"✓，
/// 修这一轮时很容易顺手写成"静默也渲一下、只是不放进响应"✗
///（我第一版就是这样的：它会降低重型笔触的成本 ✓，却把轻型原子的成本**加上去** ✗）。
/// 它的红法：把 `caller_render_wanted` 里的 `!ctx.silent || result.job_id.is_some()`
/// 改成恒 `true` ✓ ⇒ `missed_bytes` 增长 ⇒ 当场红 ✓。
///
/// **为什么用计数而不是墙钟** ✗：debug 档下 512² 的一次 preview 里"缩略图编码 / 图层合成"
/// 等固定开销占比很大 ✓，同机重复跑能差 3-5 倍 ✓（`background_stroke_cost.rs` 记录过
/// 5.05 与 0.45 两个比值 ✓）。`document_preview_render_count()` 是**"这条路走没走"** ✓，
/// 与区域大小、与机器、与档位都无关 ✓。
///
/// **怎么变红（实测过，见本轮报告）** ✗：
/// * 在 `finish_mutation` 的脏区渲染**之前**恢复那次急切的文档级预览
///   （`let _ = ctx.workspace.document_mut(&ctx.doc_id)?.run_pending_jobs()?;` ✓）——
///   这就是改动前的顺序与效果 ✓ ⇒ **形态二、三立刻红** ✓
///   （计数 +1 ✓）。**必须放在前面** ✗：放在脏区渲染之后就晚了 ✓ ——
///   `run_pending_jobs` 见 `jobs.pending()` 已空会**直接返回** ✓ ⇒ 计数不动 ✓，
///   变异因而"看起来没生效" ✓（这一条踩过一次，记在这里 ✓）。
/// * 形态一（`wait_for_render=false`）红于**去掉**脏区渲染外面的
///   `if ctx.wait_for_render` 那一层 ✓ ⇒ 解出的明文字节数 > 0 ✓（真的渲染了东西 ✓）。
/// * 轻型 ＋ 静默那一段红于**去掉** `!ctx.silent || result.job_id.is_some()` 这个条件
///   （`caller_render_wanted` 只剩 `preview_region.is_some()` ✓）
///   ⇒ 红在"静默的轻型原子居然推进了渲染水位"✓。
/// * **对照段落自己也会抓空转** ✗：我第一版拿 `missed_bytes` 当"渲染过没有"的探测器 ✓，
///   而 `draw_stroke` 是**矢量**原子 ✓ ⇒ 渲染它**不需要解压任何位图补丁** ✗
///   ⇒ 探测器全程不动 ✓、正题那条断言就成了空转 ✓ —— 对照当场红 ✓
///   （现在改用**渲染水位**：它才是"有没有渲染过"的定义式 ✓）。
#[test]
fn a_stroke_that_did_not_ask_for_the_document_preview_never_runs_one() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Mode {
        NoWait,
        Wait,
        WaitSilent,
    }
    let modes = [Mode::NoWait, Mode::Wait, Mode::WaitSilent];

    for mode in modes {
        let root = temp_dir(&format!("criterion1_{mode:?}"));
        let mut workspace = workspace(&root, "doc", 512, 512);
        document_with_background(&mut workspace, "doc");

        let before = counters(&workspace, "doc");
        let response = call(
            &mut workspace,
            "doc",
            "brush_stroke",
            stroke_args(0),
            mode != Mode::NoWait,
            mode == Mode::WaitSilent,
        );
        assert_eq!(response["ok"], json!(true), "{mode:?}: {response}");
        let after = counters(&workspace, "doc");

        // **文档级预览一次都不许跑** ✓（三种形态都一样 ✓）。
        assert_eq!(
            after.document_previews, before.document_previews,
            "{mode:?}：这一笔跑了一次**文档级预览**（缩略图那一次）⇒ 那正是报告里那个\
             “与笔刷大小无关”的固定开销。计数 {before:?} → {after:?}"
        );
        // **也不许顺带整幅渲染** ✓（冷文档上那一次就是整幅 ✓）。
        assert_eq!(
            after.full_canvas, before.full_canvas,
            "{mode:?}：这一笔顺带整幅渲染了画布：{before:?} → {after:?}"
        );

        match mode {
            Mode::NoWait => {
                // **不要渲染 ⇒ 真的什么都没渲染** ✓：没有新解出任何字节 ✓、没有预览字段 ✓、
                // job 留在队列里 ✓（下面那次渲染才跑它 ✓）。
                assert_eq!(
                    after.missed_bytes, before.missed_bytes,
                    "{mode:?}：调用方说了不要渲染，却仍然解出了东西来渲：{before:?} → {after:?}"
                );
                assert!(
                    response["preview"].is_null(),
                    "{mode:?}：不要渲染却给了预览：{response}"
                );
                assert_eq!(response["job_status"], json!("submitted"), "{response}");
                assert_eq!(
                    response["timings"]["preview_ms"],
                    json!(0.0),
                    "{mode:?}：preview_ms 必须为 0：{response}"
                );
            }
            Mode::Wait | Mode::WaitSilent => {
                // **要渲染 ⇒ 这一笔的脏区真渲染过** ✓：一定解出过东西（这一笔自己的补丁 ✓）。
                assert!(
                    after.missed_bytes > before.missed_bytes,
                    "{mode:?}：请求了渲染，却没有渲染这一笔：{before:?} → {after:?}"
                );
                assert!(
                    response["timings"]["preview_ms"].as_f64().unwrap_or(0.0) > 0.0,
                    "{mode:?}：preview_ms 必须有值：{response}"
                );
                assert_eq!(
                    response["job_status"],
                    json!("committed"),
                    "{mode:?}：wait_for_render=true 时 job 应当已完成：{response}"
                );
            }
        }

        match mode {
            // **静默：响应里不许有预览** ✓（用户 §五-5 的原话就是不要那 500 份预览 ✓）。
            Mode::WaitSilent => assert!(
                response["preview"].is_null(),
                "{mode:?}：静默调用不许带预览：{response}"
            ),
            // **非静默：响应里必须有那张"这一笔的脏区"的渲染** ✓（下一段判据把它逐字节比掉 ✓）。
            Mode::Wait => {
                let preview = &response["preview"];
                assert!(
                    preview["blob_hash"].is_string(),
                    "{mode:?}：响应里必须有这一次改动脏区的真实渲染：{response}"
                );
                assert!(
                    preview["width"].as_u64().unwrap_or(0) > 0
                        && preview["height"].as_u64().unwrap_or(0) > 0,
                    "{mode:?}：预览尺寸非法：{response}"
                );
            }
            Mode::NoWait => {}
        }
    }

    // **静默 ＋ 轻型原子**（没有 job 要跑 ✓）：**一个像素都不许渲染** ✓。
    //
    // 这一条不是"顺手加的" ✗：修这一轮时最容易写错的就是它 ✓ ——
    // "静默也渲一下、只是不放进响应"能降低重型笔触的成本 ✓，却把轻型原子的成本加上去 ✗，
    // 而 `silent` 的初衷**正是**"批量静默提交不要额外的 CAS IO"✓。
    let root = temp_dir("criterion1_light_silent");
    let mut workspace = workspace(&root, "doc", 512, 512);
    document_with_background(&mut workspace, "doc");
    let before = counters(&workspace, "doc");
    let response = call(
        &mut workspace,
        "doc",
        "draw_stroke",
        json!({
            "layer_id": "art",
            "object_id": "silent_stroke",
            "data": {
                "points": [[100.0, 100.0], [160.0, 140.0]],
                "size": 6.0,
                "color": {"r": 60, "g": 120, "b": 200, "a": 255},
            },
        }),
        true,
        true,
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    let after = counters(&workspace, "doc");
    assert_eq!(
        after, before,
        "静默的轻型原子（没有 job 要跑）不该渲染任何东西 —— \
         `silent` 的初衷是批量提交**不**产生额外的 CAS IO：{before:?} → {after:?}"
    );
    assert!(
        response["preview"].is_null(),
        "静默调用不许带预览：{response}"
    );
    // **"到底渲染了没有"的判据用渲染水位** ✓（不用 `missed_bytes` ✗）：
    // `draw_stroke` 是**矢量**原子 ✓ ⇒ 渲染它**不需要解压任何位图补丁** ✗
    // ⇒ 解码字节数对它是**瞎的** ✓（我第一版就是拿它当探测器的，对照当场红 ✓ —— 那正是对照的价值 ✓）。
    // 水位是"有没有渲染过"的**定义式** ✓：没渲染 ⇒ 水位落后于 HEAD ⇒ `rendered == false` ✓。
    let silent_atom = response["atom_id"].as_str().expect("应有原子 id");
    assert!(
        !workspace
            .render_status("doc", silent_atom)
            .unwrap()
            .rendered,
        "静默的轻型原子居然推进了渲染水位 ⇒ 它真的渲染了东西"
    );
    assert_eq!(
        response["timings"]["preview_ms"],
        json!(0.0),
        "静默的轻型原子 preview_ms 必须为 0：{response}"
    );
    // **对照** ✓：同一支工具、同一种原子，**不静默**时**必须**渲染 ✓ ——
    // 否则上面那条"什么都没渲"可能只是因为这条原子根本没有脏区 ✓（那它就什么都没测到 ✗）。
    let control = call(
        &mut workspace,
        "doc",
        "draw_stroke",
        json!({
            "layer_id": "art",
            "object_id": "loud_stroke",
            "data": {
                "points": [[200.0, 160.0], [260.0, 200.0]],
                "size": 6.0,
                "color": {"r": 200, "g": 80, "b": 40, "a": 255},
            },
        }),
        true,
        false,
    );
    assert_eq!(control["ok"], json!(true), "{control}");
    let control_after = counters(&workspace, "doc");
    let control_atom = control["atom_id"].as_str().expect("应有原子 id");
    assert!(
        workspace
            .render_status("doc", control_atom)
            .unwrap()
            .rendered,
        "对照失败：这条轻型原子不静默时应当渲染，水位却没推进 ⇒ 上面那条断言是空转的"
    );
    assert!(
        control["preview"]["blob_hash"].is_string(),
        "对照失败：不静默时应当给出这一笔脏区的**真实渲染**（不是 `Cached`）：{control}"
    );
    assert!(
        control["timings"]["preview_ms"].as_f64().unwrap_or(0.0) > 0.0,
        "对照失败：不静默时 preview_ms 应当有值：{control}"
    );
    // 而**文档级预览**一次都不许跑 ✓（正题）。
    assert_eq!(
        control_after.document_previews, before.document_previews,
        "轻型原子的提交也不该跑文档级预览：{before:?} → {control_after:?}"
    );
}

// ---------------------------------------------------------------------------
// 判据 2：最终像素与最终缩略图，与"改动前每笔都刷缩略图"逐字节相同
// ---------------------------------------------------------------------------

/// **判据 2（逐字节）**：把缩略图延后到**被请求时**再产生 ✓，
/// 最终**像素**与最终**缩略图**都与改动前**逐字节相同** ✓。
///
/// **参照物怎么造** ✓（不靠"我觉得应该一样" ✓）：
/// * `eager` 文档：每落一笔就用 `wait_for_render=false` 提交 ✓（**不渲染** ✓），
///   然后**手动**调 `Document::render_document_preview()` ✓ ——
///   那正是改动前 `run_pending_jobs()` 在提交收尾做的事**同一句** ✓
///   ⇒ 它的缩略图 = **改动前的行为** ✓（每笔刷一次 ✓，块的刷新顺序与当时一致 ✓）；
/// * `lazy` 文档：同样的笔用**缺省**（`wait_for_render=true`）落 ✓（本轮的新路径 ✓），
///   中间**一个缩略图都不产生** ✓，最后**一次性**要一张 ✓。
///
/// 然后比两样东西 ✓：最终**整幅像素**（`render_region_raw` ✓）与最终**256² 缩略图 PNG** ✓。
/// **为什么这两样一起比** ✗：只比像素会漏掉"缩略图块被按错误的顺序/区域刷新"✓
///（`Thumb::update_blocks_from_region` 的边界 clamp 就是这类错 ✓）；
/// 只比缩略图会漏掉"文档本身画错了"✓。
///
/// **怎么变红（实测过）** ✗：让 `ensure_document_thumbnail` 无条件信任缓存
/// （把 `if document.document_thumbnail_is_current()` 改成 `if true` ✓）——
/// 这就是"拿旧图冒充 HEAD" ✓ ⇒ `lazy` 交回的是**落笔之前**那张缩略图 ✓
/// ⇒ 逐字节比较当场红 ✓（尺寸相同 ✓，所以尺寸断言抓不到它 ✗，只有逐字节能 ✓）。
/// 上半段还会先红在"延后 ⇒ 必须**可见地**落后"那条断言上 ✓（`is_current() == false` ✓）——
/// 那一条是**可见性**的守卫 ✓：滞后可以 ✓，**偷偷说自己是最新的**不行 ✗。
#[test]
fn the_final_pixels_and_thumbnail_match_the_eagerly_refreshed_ones() {
    let root_eager = temp_dir("criterion2_eager");
    let root_lazy = temp_dir("criterion2_lazy");
    let mut eager = workspace(&root_eager, "doc", 512, 512);
    let mut lazy = workspace(&root_lazy, "doc", 512, 512);
    document_with_background(&mut eager, "doc");
    document_with_background(&mut lazy, "doc");

    // 两边都先**真正产生**一张 256² 文档缩略图 ✓ —— 用的就是改动前提交收尾跑的那一句
    // `render_document_preview()` ✓。这样延后那一份手里就有一张**同尺寸、会变旧**的图 ✓
    // ⇒ 那个"拿旧图冒充 HEAD"的变异**只会被逐字节抓住** ✗（尺寸断言抓不到它 ✓）。
    //
    // **不能图省事用 `ensure_document_thumbnail`** ✗：刚铺完整幅背景时
    // `document_thumbnail` 被整幅区域渲染**换成了 512² 的整幅 PNG** ✓，
    // 而那时它"与 HEAD 一致" ✓ ⇒ `ensure_document_thumbnail` 会**直接返回那张整幅图** ✓
    // ⇒ 起点就不是 256² 了 ✓，后面的逐字节比较也就变成了"整幅 PNG vs 缩略图" ✓
    //（尺寸不同 ⇒ 变异红在一个**较弱**的断言上 ✗，证不到"同尺寸的旧图"✓）。
    eager
        .document_mut("doc")
        .unwrap()
        .render_document_preview()
        .expect("改动前的提交收尾");
    lazy.document_mut("doc")
        .unwrap()
        .render_document_preview()
        .expect("改动前的提交收尾");
    let eager_seed_url = eager
        .document("doc")
        .unwrap()
        .document_thumbnail_url()
        .expect("初始缩略图");
    let lazy_seed_url = lazy
        .document("doc")
        .unwrap()
        .document_thumbnail_url()
        .expect("初始缩略图");
    let eager_seed = blob_of(&eager, &eager_seed_url);
    let lazy_seed = blob_of(&lazy, &lazy_seed_url);
    assert_eq!(
        eager_seed, lazy_seed,
        "两边起点必须相同（同一份文档 ＋ 同一张初始缩略图）"
    );
    assert_eq!(
        png_size(&eager_seed),
        (256, 256),
        "起点必须是一张 256² 文档缩略图（否则下面的比较不是在比缩略图）"
    );

    for index in 0..3 {
        // **参照物**：不渲染地提交 ✓，然后手动跑一次当时提交收尾跑的那一句 ✓。
        let made = call(
            &mut eager,
            "doc",
            "brush_stroke",
            stroke_args(index),
            false,
            false,
        );
        assert_eq!(made["ok"], json!(true), "{made}");
        eager
            .document_mut("doc")
            .unwrap()
            .render_document_preview()
            .expect("改动前的提交收尾就是这一句");

        // **被测**：缺省路径 ✓（本轮的新行为 ✓）——中间不产生任何缩略图 ✓。
        let made = call(
            &mut lazy,
            "doc",
            "brush_stroke",
            stroke_args(index),
            true,
            false,
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }

    // **滞后必须是可见的** ✓（不许"偷偷说自己是最新的" ✗）。
    assert!(
        !lazy
            .document("doc")
            .unwrap()
            .document_thumbnail_is_current(),
        "延后产生 ⇒ 缩略图此刻必须**明确**落后于 HEAD（否则调用方无从知道它该不该重新要）"
    );
    // 而参照物那边每笔都刷过 ⇒ 是最新的 ✓。
    assert!(
        eager
            .document("doc")
            .unwrap()
            .document_thumbnail_is_current(),
        "参照物每笔都刷了缩略图，应当与 HEAD 一致"
    );

    // **要新鲜就显式要** ✓ —— 这一步就是"调用方怎么拿到保证"的答案 ✓。
    let lazy_url = lazy
        .ensure_document_thumbnail("doc", DocThumbSize::S256)
        .unwrap()
        .expect("按需产生的缩略图");
    let lazy_png = blob_of(&lazy, &lazy_url);
    let eager_png = blob_of(
        &eager,
        &eager
            .document("doc")
            .unwrap()
            .document_thumbnail_url()
            .expect("缩略图地址"),
    );
    // **逐字节**比 ✓（不先比长度 ✓：长度一样却内容不同的"旧缩略图"正是这条判据要抓的东西 ✓）。
    let differing = lazy_png
        .iter()
        .zip(&eager_png)
        .filter(|(a, b)| a != b)
        .count()
        + lazy_png.len().abs_diff(eager_png.len());
    assert_eq!(
        differing,
        0,
        "最终缩略图与改动前**不是同一张**：{differing} / {} 字节不同\
         （按需产生 {} 字节，逐笔刷新 {} 字节）⇒ 要么延后改变了缩略图像素，要么交回了一张旧的",
        eager_png.len(),
        lazy_png.len(),
        eager_png.len()
    );
    assert!(
        lazy.document("doc")
            .unwrap()
            .document_thumbnail_is_current(),
        "按需要过一次之后，缩略图必须与 HEAD 一致"
    );

    // **最终像素**：两边整幅裸 RGBA 逐字节比 ✓（渲染判据不用 PNG 编码器做中介 ✓）。
    let bbox = yanshi_core::Bbox::new(0.0, 0.0, 512.0, 512.0);
    let (lazy_w, lazy_h, lazy_px) = lazy.render_region_raw("doc", bbox).unwrap();
    let (eager_w, eager_h, eager_px) = eager.render_region_raw("doc", bbox).unwrap();
    assert_eq!((lazy_w, lazy_h), (eager_w, eager_h));
    assert_eq!(
        lazy_px.len(),
        eager_px.len(),
        "整幅像素字节数不同：{} vs {}",
        lazy_px.len(),
        eager_px.len()
    );
    let diff = lazy_px
        .iter()
        .zip(&eager_px)
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        diff,
        0,
        "最终像素与改动前不同：{diff} / {} 字节不一样",
        lazy_px.len()
    );
}

// ---------------------------------------------------------------------------
// 判据 3：wait_for_render=true 仍然意味着"返回时渲染已完成"
// ---------------------------------------------------------------------------

/// **判据 3（契约没被偷偷削弱）**：`wait_for_render=true` 返回时 ✓：
/// ① `job_status == "committed"` ✓、② 该原子 `render_status.rendered == true` 且
/// `rendered_seq == head_seq` ✓、③ 响应里的 `preview` **就是当前文档那一片像素** ✓
/// （用一个独立的 `render_region` 再渲一次 ✓，**内容寻址的 blob 地址必须相同** ✓）。
///
/// ③ 是这条判据的要害 ✗："渲染完成"如果只是把某个计数器往前挪 ✓，调用方拿到的图却可以是
/// **上一次的** ✗ —— 那正是本专题要避免的"用旧图冒充"✓。内容寻址让"像素是否相同"
/// 变成一次字符串比较 ✓，不需要解码 PNG ✓。
///
/// **怎么变红（实测过）** ✗：让 `finish_mutation` **提前返回** ——
/// 在函数开头插一句
/// `if ctx.wait_for_render { return Ok(commit_response(result, None, Some("submitted"), Vec::new())); }`
/// ⇒ `preview` 是 `null` ✓、`job_status` 是 `submitted` ✓、水位没推进 ⇒ `rendered == false` ✓
/// ⇒ 三条断言全红 ✓（实测红在①的 `job_status` ✓）。
#[test]
fn wait_for_render_true_still_returns_after_the_render_is_complete() {
    let root = temp_dir("criterion3");
    let mut workspace = workspace(&root, "doc", 512, 512);
    document_with_background(&mut workspace, "doc");

    let response = call(
        &mut workspace,
        "doc",
        "brush_stroke",
        stroke_args(0),
        true,
        false,
    );
    assert_eq!(response["ok"], json!(true), "{response}");

    // ① job 真的跑完了 ✓（不是"留在队列里"✗）。
    assert_eq!(
        response["job_status"],
        json!("committed"),
        "wait_for_render=true 提前返回了：{response}"
    );
    let job_id = response["job_id"].as_str().expect("重型原子应有 job");
    let queried = call(
        &mut workspace,
        "doc",
        "get_job",
        json!({"job_id": job_id}),
        true,
        false,
    );
    assert_eq!(
        queried["status"],
        json!("committed"),
        "重新查一次 job 也应当已完成：{queried}"
    );

    // ② 渲染水位推到 HEAD ✓。
    let atom_id = response["atom_id"].as_str().expect("应有原子 id");
    let head = response["head"].as_u64().expect("应有 head");
    let status = workspace.render_status("doc", atom_id).unwrap();
    assert!(
        status.rendered,
        "原子 {atom_id} 报 rendered=false ⇒ 水位没推进（提前返回了）"
    );
    assert_eq!(status.rendered_seq, status.head_seq, "水位必须等于 HEAD");
    assert_eq!(status.head_seq, head, "响应里的 head 与文档状态必须一致");

    // ③ 响应里的那张图就是**当前**这一片像素 ✓（内容寻址 ⇒ 一次字符串比较 ✓）。
    let preview = &response["preview"];
    assert!(
        !preview.is_null() && preview["blob_hash"].is_string(),
        "wait_for_render=true 必须交回这一笔的渲染：{response}"
    );
    let bbox = response["dirty_bbox"]
        .as_array()
        .map(|values| {
            let number = |index: usize| values[index].as_f64().unwrap_or(0.0);
            yanshi_core::Bbox::new(number(0), number(1), number(2).max(1.0), number(3).max(1.0))
        })
        .expect("这一笔应当有 dirty_bbox");
    let again = workspace.render_region("doc", bbox).unwrap();
    assert_eq!(
        again.blob_hash.to_string(),
        preview["blob_hash"].as_str().unwrap_or_default(),
        "响应里的预览与此刻重新渲染同一片区域**不是同一份像素** ⇒ 交回的是旧图或别的区域"
    );
    let png = blob_of(&workspace, preview["thumb_url"].as_str().unwrap());
    assert!(
        png.starts_with(&[0x89, b'P', b'N', b'G']),
        "预览必须是一张 PNG"
    );
    assert_eq!(
        preview["width"].as_u64().unwrap_or(0),
        u64::from(again.width),
        "预览宽度与重新渲染不一致"
    );
    assert_eq!(
        preview["height"].as_u64().unwrap_or(0),
        u64::from(again.height),
        "预览高度与重新渲染不一致"
    );
}

/// **判据 4 的"点名"** ✓（不另写第三套 harness ✗）：
///
/// * 跨渲染位图缓存的语义计数（**本专题之前的那一半**，必须保持绿 ✓）：
///   `cargo test -p yanshi-server --test background_stroke_cost`
///   —— `the_background_patch_is_decoded_once_not_per_stroke` ✓、
///   `a_warm_stroke_never_decodes_a_canvas_sized_blob` ✓、
///   `a_background_stroke_populates_the_preview_phase` ✓；
/// * 服务端/内核**逐位一致**（离线/在线同一份像素 ✓）：
///   `cargo test -p yanshi-server --test render_parity`；
/// * **计时不变式**（各相 ＋ 残差 == `total_ms` ✓）：
///   `cargo test -p yanshi-server --test timings_and_cancel`；
/// * job 完成 / 渲染水位两条契约（**它们正是第 1038 轮挡住"直接删掉渲染"的那两条** ✓）：
///   `cargo test -p yanshi-server document::tests::heavy_atoms_create_jobs_and_render_completes_them`
///   与 `service_flow::jobs_ttl_cancel_and_render_watermark`；
/// * 缩略图"落后就重建、绝不拿旧图冒充"：
///   `service_flow::document_thumbnail_is_cover_whole_canvas_not_the_last_region`。
///
/// 这条测试自己只做一件事 ✓：把上面那张清单**钉在代码里** ✓ ——
/// 有人删掉某条判据时，至少这里还留着"本专题靠哪些判据守着"的答案 ✓。
#[test]
fn criterion_four_names_the_existing_harnesses_instead_of_writing_a_third_one() {
    let named = [
        "tests/background_stroke_cost.rs",
        "tests/render_parity.rs",
        "tests/timings_and_cancel.rs",
        "document::tests::heavy_atoms_create_jobs_and_render_completes_them",
        "service_flow::jobs_ttl_cancel_and_render_watermark",
        "service_flow::document_thumbnail_is_cover_whole_canvas_not_the_last_region",
    ];
    let server = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for entry in &named[..3] {
        assert!(
            server.join(entry).exists(),
            "本轮点名的既有判据不见了：{entry}"
        );
    }
    for entry in &named[3..] {
        let (file, path) = entry.split_once("::").expect("形如 file::function");
        let source = match file {
            "document" => server.join("src/document.rs"),
            "service_flow" => server.join("tests/service_flow.rs"),
            other => panic!("没登记过 {other}"),
        };
        // 只看**最后一段**函数名 ✓：`document::tests::foo` 与 `service_flow::foo` 都指向 `fn foo(` ✓。
        let function = path.rsplit("::").next().expect("函数名");
        let text = std::fs::read_to_string(&source).expect("读判据源文件");
        assert!(text.contains(function), "本轮点名的既有判据不见了：{entry}");
    }
}
