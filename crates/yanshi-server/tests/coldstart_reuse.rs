//! **冷启动复用判据**（性能专题）：打开一份已有持久化渲染的文档时，
//! **不许**整幅重渲染画布、**不许**重放整篇日志。
//!
//! ## 为什么判据要数"次数"，不量"墙钟"
//!
//! 真实 4K 工程（494 atom / 318 个存活位图对象 / 395 个 `import_image`）实测（debug 构建）：
//!
//! | 阶段 | 耗时 |
//! |---|---|
//! | 读 494 行 atoms.jsonl | 31 ms |
//! | **折叠整篇日志（`state@HEAD`）** | **30 ms** |
//! | 首次文档预览（`render_document_preview`，整幅） | **122 s** |
//! | 首次整幅区域渲染（`render_region`，整幅） | **142~202 s** |
//! | 解码已落盘的整幅 4K PNG（想拿它当预览基座） | **153.7 s** ✗ |
//!
//! ⇒ **∴ "36 秒一次性重放"不是日志折叠** ✗（它是 **30 毫秒** ✓），
//! 而是**整幅全分辨率渲染** ✓；而且**每个新连接**只要磁盘缓存的 seq 一落后就重来一遍 ✗。
//! 墙钟随机器差两个数量级 ✓ ⇒ 判据只数**语义次数** ✓（整幅渲染几次、全量折叠几次 ✓），
//! 在慢机器上也**秒级**跑完 ✓。
//!
//! ## 什么像素可以安全复用 ✓ / 什么绝对不行 ✗
//!
//! **只有"小图文档预览"（256² `preview.png`）可以做增量基座** ✓，理由是三条都可证 ✓：
//! 1. **尺寸**：它由 `render_document_preview` 自己产生 ✓ ⇒ 与整幅重算的下采样**同一条代码** ✓；
//! 2. **新鲜度**：`preview.seq` 与 `(seq, HEAD]` 的脏区一起用 ✓ —— 脏区外扩到**整块**后才刷新 ✓
//!    ⇒ 缩略图与整幅重算**逐像素一致** ✓（判据里有逐字节比对 ✓）；
//! 3. **可证伪**：脏区算不出来（`Unknown`）时**不建基座** ✓ ⇒ 退回整幅 ✓，绝不拿旧像素充新 ✗。
//!
//! **整幅 4K `render.png` 绝不能当预览基座** ✗：解码它 **153.7s** ＋ u8→线性 **26.4s**
//! ＝ **183.6s** ✓，比整幅重渲染（122s）还贵 ✗ ⇒ 那是**负优化** ✓。它只用来回答
//! "给我整幅画布" ✓，而且**只在 `seq == HEAD` 时** ✓（只需读 PNG 头里的尺寸 ✓，不解码 ✗）。

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use yanshi_core::Bbox;
use yanshi_render::thumb::ThumbKind;
use yanshi_server::persist::FileStore;
use yanshi_server::{
    DocThumbSize, DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace,
};

/// 判据用的文档 id。
const DOC: &str = "doc_cold";
/// 画布边长（够小 ⇒ 判据秒级；够大 ⇒ 缩略图有 8×8 块，脏区只会覆盖其中几块）。
const SIDE: u32 = 512;

fn temp_root(tag: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-coldstart-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn file_workspace(root: &Path) -> Workspace {
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("打开落盘工作区")
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn new_document(workspace: &mut Workspace) {
    workspace
        .create_document(NewDocument::new(DOC, SIDE, SIDE), "human:1", "session:test")
        .expect("新建文档");
    let mut ctx = ToolContext::new(workspace, DOC, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let response = registry().call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
    assert_eq!(response["ok"], json!(true), "{response}");
}

fn draw(workspace: &mut Workspace, object_id: &str, points: Value) {
    let mut ctx = ToolContext::new(workspace, DOC, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let response = registry().call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_1",
            "object_id": object_id,
            "data": {"points": points, "size": 18.0,
                     "color": {"r": 210, "g": 40, "b": 40, "a": 255}}
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG"), "应为 PNG");
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

fn canvas() -> Bbox {
    Bbox::new(0.0, 0.0, f64::from(SIDE), f64::from(SIDE))
}

fn blob_bytes(workspace: &Workspace, url: &str) -> Vec<u8> {
    let raw = url.trim_start_matches("yanshi://blob/");
    let hash: yanshi_core::atom::BlobHash = raw.parse().expect("blob 地址应合法");
    workspace.store().get(&hash).expect("取 blob")
}

/// `(整幅渲染次数, 全量折叠次数)` ✓ —— **语义计数** ✓，不看墙钟 ✗。
fn counters(workspace: &Workspace, doc_id: &str) -> (usize, usize) {
    let document = workspace.document(doc_id).expect("文档应已打开");
    (
        document.full_canvas_render_count(),
        document.full_fold_count(),
    )
}

/// 只做**整幅重算**得到一张 256² 文档预览，**不落盘** ✓（参考值用 ✓）。
fn full_render_reference(workspace: &mut Workspace) -> Vec<u8> {
    let preview = workspace
        .document_mut(DOC)
        .expect("文档")
        .thumbnail(ThumbKind::Doc256, None)
        .expect("整幅重算缩略图");
    workspace
        .store()
        .get(&preview.blob_hash)
        .expect("取参考 PNG")
}

/// **判据 1**：持久化预览落后 HEAD 时，它仍要作为**增量基座**被复用
/// ⇒ 新连接的第一次文档预览**只渲染脏区**，且结果与整幅重算**逐字节一致**。
///
/// **怎么变红** ✗：把 `ensure_document_thumbnail` 的 256² 分支改回
/// `self.thumbnail(doc_id, kind, None)`（旧行为）⇒ 计数变成 1 ⇒ 第一条断言失败；
/// 或者让 `restore_persisted_preview` 不建像素基座（`document_thumb = None`）
/// ⇒ 同样退回整幅 ✗。
#[test]
fn reopened_document_reuses_the_persisted_preview_instead_of_rendering_the_canvas() {
    let root = temp_root("reuse");
    let mut workspace = file_workspace(&root);
    new_document(&mut workspace);
    draw(
        &mut workspace,
        "obj_base",
        json!([[40.0, 40.0], [210.0, 130.0]]),
    );

    // 磁盘上的**小图**预览缓存（`preview.png` + `preview.seq`）＝ 当时 HEAD。
    workspace
        .ensure_document_thumbnail(DOC, DocThumbSize::S256)
        .expect("生成并落盘文档预览");
    let base_head = workspace.document(DOC).expect("文档").head_seq();
    let base = FileStore::open(&root)
        .expect("文件存储")
        .load_preview(DOC)
        .expect("读预览缓存")
        .expect("应有 preview.png");
    assert_eq!(base.0, base_head);
    assert_eq!(png_size(&base.1), (256, 256), "预览基座必须是 256² 小图");

    // HEAD 前进（磁盘预览因此**落后**）。
    draw(
        &mut workspace,
        "obj_extra",
        json!([[310.0, 370.0], [470.0, 470.0]]),
    );
    let head = workspace.document(DOC).expect("文档").head_seq();
    assert!(head > base_head, "HEAD 应已前进");
    // 参考值：**整幅重算**的文档预览（不落盘 ⇒ 磁盘预览仍是旧基座）。
    let reference_png = full_render_reference(&mut workspace);
    assert_ne!(
        reference_png, base.1,
        "参考值应与旧基座不同（否则这条判据证明不了什么）"
    );
    drop(workspace); // 新连接 ⇒ 新 `Workspace` ⇒ 重新打开

    let mut workspace = file_workspace(&root);
    {
        let document = workspace.open_document(DOC).expect("重新打开");
        assert_eq!(
            document.document_thumbnail_seq(),
            base_head,
            "文档预览应仍是磁盘上那份旧基座"
        );
        assert!(
            !document.document_thumbnail_is_current(),
            "基座应落后于 HEAD"
        );
    }
    assert_eq!(
        counters(&workspace, DOC),
        (0, 1),
        "打开只允许折叠一次（重建 HEAD 状态）、不许渲染画布"
    );

    let url = workspace
        .ensure_document_thumbnail(DOC, DocThumbSize::S256)
        .expect("生成预览")
        .expect("应有预览地址");
    assert_eq!(
        counters(&workspace, DOC).0,
        0,
        "**复用基座之后只许渲染脏区**：整幅渲染计数必须是 0（旧行为在这里是 1 ⇒ 变红）"
    );
    assert_eq!(
        counters(&workspace, DOC).1,
        1,
        "预览与渲染都不许重放日志（打开那一次之后计数必须不动）"
    );
    {
        let document = workspace.document(DOC).expect("文档");
        assert!(
            document.document_thumbnail_is_current(),
            "预览必须推进到 HEAD"
        );
        assert_eq!(document.document_thumbnail_seq(), head);
    }
    assert_eq!(
        blob_bytes(&workspace, &url),
        reference_png,
        "**复用基座 + 只渲染脏区** 必须与整幅重算**逐字节一致**（脏区外扩到整块保证这一点）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 2**：持久化**整幅**渲染与 HEAD **一致**时，整幅 `render_region` **直接命中**那份缓存
/// ⇒ 一次画布渲染都不发生，且输出**逐字节相同**。
///
/// **怎么变红** ✗：删掉 `render_region` 里的 `current_full_frame_preview()` 快路径
/// （或让 `current_full_frame_preview` 恒返回 `None`）⇒ 计数变成 1 ⇒ 第一条断言失败。
#[test]
fn current_persisted_full_frame_is_served_without_rendering() {
    let root = temp_root("current");
    let mut workspace = file_workspace(&root);
    new_document(&mut workspace);
    draw(
        &mut workspace,
        "obj_a",
        json!([[60.0, 90.0], [420.0, 300.0]]),
    );
    // 整幅渲染 ⇒ `Workspace::render_region` 会把整幅 PNG 落盘为 HEAD 渲染缓存。
    let first = workspace.render_region(DOC, canvas()).expect("整幅渲染");
    let first_png = workspace.store().get(&first.blob_hash).expect("取整幅 PNG");
    assert_eq!((first.width, first.height), (SIDE, SIDE));
    let head = workspace.document(DOC).expect("文档").head_seq();
    drop(workspace);

    let mut workspace = file_workspace(&root);
    {
        let document = workspace.open_document(DOC).expect("重新打开");
        assert!(
            document.full_frame_render_is_current(),
            "磁盘上的整幅渲染应被认成与 HEAD 一致"
        );
    }
    assert_eq!(counters(&workspace, DOC), (0, 1));

    let again = workspace.render_region(DOC, canvas()).expect("整幅渲染");
    assert_eq!(
        counters(&workspace, DOC).0,
        0,
        "**与 HEAD 一致的持久化整幅渲染必须直接命中**（旧行为在这里是 1 ⇒ 变红）"
    );
    assert_eq!(
        workspace.store().get(&again.blob_hash).expect("取整幅 PNG"),
        first_png,
        "缓存命中的输出必须与原渲染**逐字节相同**"
    );
    assert_eq!((again.width, again.height), (SIDE, SIDE));
    {
        let document = workspace.document(DOC).expect("文档");
        assert_eq!(document.render_watermark(), head);
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 2 的反例（负例）**：持久化渲染**落后** HEAD 时**绝不许**把它当成 HEAD 返回 ✗。
///
/// 这是"复用"最容易出的 bug：少比一个 seq ⇒ 用户看到的是**旧画面** ✓，
/// 而且**只在缓存碰巧落后时**发作 ✓（最难查的一类 ✓）。
///
/// **怎么变红** ✗：把 `current_full_frame_preview` 里的
/// `self.full_frame_render_seq != head` 判断删掉（只留 `is_some()`）⇒
/// 落后的旧图被当成 HEAD 返回 ⇒ `assert_ne!` 与计数断言同时失败 ✓。
#[test]
fn stale_persisted_render_is_never_served_as_the_current_canvas() {
    let root = temp_root("stale");
    let mut workspace = file_workspace(&root);
    new_document(&mut workspace);
    draw(
        &mut workspace,
        "obj_a",
        json!([[40.0, 40.0], [200.0, 160.0]]),
    );
    let stale = workspace.render_region(DOC, canvas()).expect("整幅渲染");
    let stale_png = workspace.store().get(&stale.blob_hash).expect("取旧图");
    let stale_head = workspace.document(DOC).expect("文档").head_seq();
    // HEAD 前进：再画一笔 ⇒ 磁盘上那份整幅渲染**落后**了。
    draw(
        &mut workspace,
        "obj_b",
        json!([[300.0, 300.0], [480.0, 480.0]]),
    );
    drop(workspace);

    let mut workspace = file_workspace(&root);
    {
        let document = workspace.open_document(DOC).expect("重新打开");
        assert!(
            !document.full_frame_render_is_current(),
            "落后于 HEAD 的整幅渲染**不许**被认成当前"
        );
        assert!(document.head_seq() > stale_head);
    }
    let rendered = workspace.render_region(DOC, canvas()).expect("整幅渲染");
    assert_eq!(
        counters(&workspace, DOC).0,
        1,
        "基座落后 ⇒ **必须真渲染一次**（把旧图当缓存用会在这里变成 0 ⇒ 变红）"
    );
    assert_ne!(
        workspace.store().get(&rendered.blob_hash).expect("取新图"),
        stale_png,
        "**落后的整幅渲染绝不许当成 HEAD 返回**（用户会看到旧画面）"
    );
    {
        let document = workspace.document(DOC).expect("文档");
        assert_eq!(document.render_watermark(), document.head_seq());
        assert!(
            document.full_frame_render_is_current(),
            "渲染后缓存应追平 HEAD"
        );
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// **判据 3**：磁盘上的 `render.png` 也可能是 **256² 缩略图**
/// ⇒ 它**只能**当"打开即图片"的指针 ✓，**绝不能**拿去回答"给我整幅画布" ✗
///（那会返回一张 256² 的图，分辨率都不对 ✗）。
///
/// **怎么变红** ✗：把 `restore_persisted_render` 里的
/// `(width, height) != (self.state.width, self.state.height)` 尺寸判断删掉
/// （只按"文件存在"就认成整幅）⇒ 整幅请求会命中 256² 的 blob ⇒ 尺寸断言失败 ✓。
#[test]
fn persisted_thumbnail_is_not_mistaken_for_a_full_frame_render() {
    let root = temp_root("thumb");
    let mut workspace = file_workspace(&root);
    new_document(&mut workspace);
    draw(
        &mut workspace,
        "obj_a",
        json!([[70.0, 70.0], [430.0, 260.0]]),
    );
    // 只落盘一张 256² 文档缩略图（`Workspace::thumbnail` 会写 `render.png` ✓）。
    let thumb = workspace
        .thumbnail(DOC, ThumbKind::Doc256, None)
        .expect("生成文档缩略图");
    assert_eq!(png_size(&blob_bytes(&workspace, &thumb.url)), (256, 256));
    drop(workspace);

    let mut workspace = file_workspace(&root);
    {
        let document = workspace.open_document(DOC).expect("重新打开");
        assert!(
            !document.full_frame_render_is_current(),
            "256² 缩略图**不是**整幅渲染 ⇒ 不许当整幅缓存"
        );
        assert!(
            document.document_thumbnail_is_current(),
            "但它仍然是一张与 HEAD 一致的文档缩略图（打开即图片 ✓）"
        );
    }
    let rendered = workspace.render_region(DOC, canvas()).expect("整幅渲染");
    assert_eq!(
        counters(&workspace, DOC).0,
        1,
        "256² 缩略图回答不了整幅请求 ⇒ 必须真渲染（错实现会是 0 ⇒ 变红）"
    );
    assert_eq!(
        (rendered.width, rendered.height),
        (SIDE, SIDE),
        "整幅请求必须给出整幅分辨率"
    );
    let _ = std::fs::remove_dir_all(&root);
}
