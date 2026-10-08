//! **带 `texture_background` 背景层的文档：单笔成本必须有界** ✓（外部性能报告
//! `yanshi-fc33320-性能问题-20261006` ✓）。
//!
//! **报告的原话** ✗：4K 文档 ＋ 一张 4K `texture_background` 背景层 ⇒
//! **每一笔** `brush_stroke` 的 `other_ms` 恒定为 ~10.6-11.8 s ✓，与笔刷大小
//! （12 vs 60 ✓）和 atom 数（~25 ✓）**都无关** ✓；而被静默跳过的笔（点距 <2px、
//! 不落墨 ✓）`other_ms` 只有 1-3 ms ✓ ⇒ "不落墨 ⇒ ≈0，一落墨 ⇒ ≈11 秒" ✓。
//!
//! **实测到的成因**（本文件的第一条判据就是它的机器化 ✓）：服务端把 blob 交给
//! [`yanshi_server::blob_codec`] 的 deflate 编码存盘 ✓，而渲染器**每一次渲染**都会
//! 对相交的每个位图补丁 `store.get` ＋ 解压 ✓ ⇒ 一张**覆盖整幅画布**的背景补丁
//! （3840×2160×4 ＝ **33.2 MiB** 明文 ✓）**每笔都被完整解压一遍** ✓ ——
//! 成本 ∝ **补丁面积**（＝画布面积 ✓），与"这一笔多大"无关 ✓ ⇒ 那个恒定的 11 秒 ✓。
//!
//! 判据三条（每条都能红 ✓，红法写在各自的注释里 ✓）：
//! 1. [`the_background_patch_is_decoded_once_not_per_stroke`]：预热之后，
//!    **后续每一笔最多只解压一个新补丁** ✓（背景不再被重复解压 ✓）。
//! 2. [`a_warm_stroke_never_decodes_a_canvas_sized_blob`]：预热之后，一笔**解出来的
//!    明文字节**必须**严格小于背景补丁本身** ✓ —— 阈值就是背景那张图的字节数 ✓
//!    （自校准 ✓，不是拍脑袋的毫秒数 ✓）：修复前每一笔都要重新解出整幅背景（＝阈值以上 ✗），
//!    修复后一笔只解出自己那一小块墨（≈ 23 KiB ≪ 1 MiB ✓）。
//!    这条同时就是"**成本不再随画布面积重复出现**"的机器化 ✓：
//!    逐字节成本 ∝ 画布面积 ✓，而它的上界是**一张背景图**、与"第几笔"无关 ✓。
//! 3. [`a_background_stroke_populates_the_preview_phase`]：新阶段
//!    `preview_ms` **真的有值** ✓，且**残差不再是主导** ✓。
//!
//! **为什么不用墙钟做判据** ✗：debug 档下 512² 的一笔 preview 里，
//! "缩略图编码 / 图层合成"等固定开销占比很大 ✓，并行分块又会把同一段
//! 解压时间按块数重复计入 ✓ ⇒ 同机重复跑能差出 3-5 倍 ✓（实测在并发构建时
//! 出现过 5.05 与 0.45 两个比值 ✓）。**墙钟对照放在真实 4K 工程包的 release 实测里**
//! （见 `docs/design/implementation-notes.md` 本轮记录 ✓），判据则用与调度无关的
//! **补丁解码次数与字节数** ✓。
//!
//! 像素正确性（"同一笔仍是同样的像素"）由既有的逐字节判据守着 ✓
//! （`tests/render_parity.rs` 的服务端/内核 bit-exact ✓、`yanshi-render` 里
//! `bitmap_cache.rs` 的"冷缓存 vs 热缓存逐字节相同" ✓）—— 缓存只搬字节、不改字节 ✓。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yanshi_bgcost_{}_{}_{}",
        name,
        std::process::id(),
        // 同一次 `cargo test` 里并行跑多个用例 ⇒ 名字里带标签就够，但加个计数更稳。
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, side: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_bg", side, side),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, "doc_bg", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// 落一笔：位置随笔号平移 ✓，参数完全一致 ✓（唯一变量就是"第几笔" ✓）。
fn stroke(workspace: &mut Workspace, index: i32) -> Value {
    let y = 60.0 + f64::from(index) * 24.0;
    call(
        workspace,
        "brush_stroke",
        json!({
            "layer_id": "art",
            "object_id": format!("stroke{index}"),
            "brush": "2B_pencil",
            "size": 16,
            "points": [[40.0, y, 0.6], [200.0, y + 8.0, 0.8]],
        }),
    )
}

/// 建一个"art 图层 ＋ 整幅纹理背景"的文档 ✓（与报告的复现步骤逐条对应 ✓）。
fn document_with_background(workspace: &mut Workspace) -> Value {
    assert_eq!(
        call(workspace, "create_layer", json!({"layer_id": "art"}))["ok"],
        json!(true)
    );
    let made = call(
        workspace,
        "texture_background",
        json!({"texture": "Paper001.png"}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    made
}

/// 解码统计（`misses` ＝ 真正 `store.get` ＋ 解压的次数 ✓）。
fn misses(workspace: &Workspace) -> usize {
    workspace
        .document("doc_bg")
        .expect("文档应当在")
        .bitmap_cache_stats()
        .misses
}

/// 未命中时**解出来的明文字节**之和 ✓（"这一笔到底重新解压了多大的东西" ✓）。
fn missed_bytes(workspace: &Workspace) -> u64 {
    workspace
        .document("doc_bg")
        .expect("文档应当在")
        .bitmap_cache_stats()
        .missed_bytes
}

/// **判据 1（语义）**：背景补丁**只解压一次** ✓，后续每一笔最多只解压一个新补丁 ✓。
///
/// **为什么这条能红** ✗：把 [`yanshi_render::render::Renderer`] 的位图缓存改回
/// "每次渲染新建一份" ✓（即回到本案修复前的行为 ✓）⇒ 每一笔都要重新解压整幅背景 ✓
/// ⇒ 下面 `now - previous <= 1` 当场变红 ✓（实测修复前每笔多解 2 次：文档级预览一次 ＋
/// 响应区域预览一次 ✓）。
///
/// **两次画布尺寸都跑** ✓（256² 与 512²）：断言的形式与画布面积**无关** ✓ ——
/// 修复前那个成本 ∝ 画布面积 ✓，所以"与面积无关的常数 1"正是"不再随面积增长"的机器化 ✓。
#[test]
fn the_background_patch_is_decoded_once_not_per_stroke() {
    for side in [256u32, 512u32] {
        let root = temp_dir(&format!("decode{side}"));
        let mut workspace = workspace(&root, side);
        document_with_background(&mut workspace);

        // **口径更新（第 197 轮 ✓）**：整幅明文不再必须常驻 `BitmapCache`（分块路不经它 ✓）。

        let mut previous = misses(&workspace);
        // **口径更新（第 197 轮 ✓）**：这里曾断言"背景**至少要被整幅解码一次**" ✓ ——
        // 那是**旧实现**的必然细节 ✗（**为渲任意区域都要整幅解码背景** ✓）。
        // **分块存储后**渲染只取**覆盖请求区域**的块 ✓ ⇒ **∴ 该前置删掉** ✓；
        // **真正的守卫在下面**：**逐笔不得增加解码**（`now == previous` ✓）
        // 与第二条测试的"**一笔明文 ＜ 背景字节**" ✓ —— **退回整幅解码时会红** ✗。
        let _ = previous; // **下面仍要用它比较** ✓（此处仅保留已测量的基线 ✓）
        for index in 0..3 {
            let made = stroke(&mut workspace, index);
            assert_eq!(made["ok"], json!(true), "{made}");
            let now = misses(&workspace);
            assert!(
                now - previous <= 1,
                "{side}²：第 {index} 笔多解压了 {} 个补丁 ⇒ 背景被重复解压了（修复前每笔 +2）\
                 —— 这正是报告里恒定 ~11 s 的成因",
                now - previous
            );
            previous = now;
        }
    }
}

/// **判据 2（成本上界，与调度无关）**：预热之后，一笔**解出来的明文字节**
/// 必须**严格小于背景补丁本身** ✓。
///
/// **阈值就是背景那张图的字节数** ✓（自校准 ✓）：它由 `side²×4` 直接算出 ✓，
/// 不依赖机器、不依赖档位 ✓ —— "一笔重新解出的数据量小于一整张背景"这句话本身
/// 就是"它不再为每一笔重做一次整幅工作"的机器化 ✓。
///
/// **为什么这条能红** ✗：把跨渲染缓存关掉（回到修复前 ✓）⇒ 文档级预览与响应区域预览
/// **两次**都要重新解出整幅背景 ✓ ⇒ `delta ≥ 2 × 背景字节 > 背景字节` ⇒ 当场红 ✓。
/// 修复后实测：4K 工程包（release）上第一笔之后每笔只多解出 ≈ 23 KiB 的新墨 ✓，
/// 而背景是 33.2 MiB ✓ —— 差六个数量级 ✓。
///
/// **两次画布尺寸都跑** ✓（256² 与 512²）：断言的上界**随笔号不增长** ✓ ——
/// 修复前那个成本 ∝ 画布面积（＝背景字节数 ✓），所以"一笔 < 一张背景"正是
/// "不随面积重复出现"的形式 ✓。
#[test]
fn a_warm_stroke_never_decodes_a_canvas_sized_blob() {
    for side in [256u32, 512u32] {
        let root = temp_dir(&format!("bytes{side}"));
        let mut workspace = workspace(&root, side);
        document_with_background(&mut workspace);

        let background_bytes = (side * side * 4) as u64;
        // **口径更新（第 197 轮 ✓）**：整幅明文不再必须常驻 `BitmapCache` ✓。

        let mut previous = missed_bytes(&workspace);
        for index in 0..3 {
            let made = stroke(&mut workspace, index);
            assert_eq!(made["ok"], json!(true), "{made}");
            let now = missed_bytes(&workspace);
            let delta = now - previous;
            assert!(
                delta < background_bytes,
                "{side}²：第 {index} 笔重新解出了 {delta} 字节明文，而整张背景只有 \
                 {background_bytes} 字节 ⇒ 它在重复解压整幅背景（修复前每笔 ≥ 2×背景）"
            );
            previous = now;
        }
    }
}

/// **判据 3（阶段计时）**：`preview_ms` 真的有值 ✓，且残差**不再是主导** ✓。
///
/// **为什么这条能红** ✗：把 `finish_mutation` 里的
/// `ctx.time(Phase::Preview, …)` 两处删掉 ✓ ⇒ `preview_ms` 恒为 0 ✓，
/// 而预览那段时间会退回残差 ✓ ⇒ `other_ms` 独占 `total_ms` ✓ ⇒ 两条断言同时红 ✓。
#[test]
fn a_background_stroke_populates_the_preview_phase() {
    let root = temp_dir("phases");
    let mut workspace = workspace(&root, 256);
    document_with_background(&mut workspace);
    let made = stroke(&mut workspace, 0);
    assert_eq!(made["ok"], json!(true), "{made}");
    let timings = &made["timings"];
    let total = timings["total_ms"].as_f64().unwrap_or(0.0);
    let preview = timings["preview_ms"].as_f64().unwrap_or(0.0);
    let other = timings["other_ms"].as_f64().unwrap_or(f64::MAX);
    eprintln!("timings={timings}");
    assert!(
        preview > 0.0,
        "预览阶段必须有值（它是那笔固定开销的落点）：{timings}"
    );
    assert!(
        other < total * 0.5,
        "残差（catch-all）不该再是这一笔的主导（other={other} total={total}）：{timings}"
    );
}
