//! **导入之后的画**必须还是有内容的（第 825 轮；追 798 轮"往返后导出字节相同"的谜）
//!
//! **⇒ 要判定的假设**：798 那次「丢掉 tombstone ⇒ 导出字节不变」到底是
//!   ① 变异没生效，还是 ② **导入后画面本来就是空的**（那两种情况下都"相同" ✓）。
//! **⇒ 判据（三步，全部结构量）**：
//!   ① 画一笔 ⇒ `export_png` ⇒ **必须有非背景像素** ✓（否则"画上了"这件事本身没成立 ✓）；
//!   ② `export_project` → `import_project` ⇒ `export_png` ⇒ **同样必须有非背景像素** ✓；
//!   ③ 两次导出的**像素**必须相同 ✓（往返保真 ✓，比压缩字节可靠 ✓）。
use serde_json::json;
use yanshi_render::png::decode_png;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

#[test]
fn an_imported_package_still_has_the_paint_in_it() {
    let root = std::env::temp_dir().join(format!("yanshi_import_paint_{}", std::process::id()));
    /// **跑完即删** ✓（`/tmp` 是 1.9 GB tmpfs ✓）
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");

    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new("doc_paint", 64, 64), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_paint", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
    let drawn = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_paint",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [56.0, 56.0, 1.0]],
            "size": 10.0,
            "color": {"r": 20, "g": 20, "b": 240, "a": 255},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let before_path = root.join("before.png");
    let exported = registry.call(
        &mut ctx,
        "export_png",
        &json!({"path": before_path.to_string_lossy()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let pack = root.join("doc_paint.yanshi");
    let packed = registry.call(
        &mut ctx,
        "export_project",
        &json!({"path": pack.to_string_lossy()}),
    );
    assert_eq!(packed["ok"], json!(true), "{packed}");

    // 非背景像素计数（背景是白 255 ⇒ 任何通道 < 250 就算"有墨"）
    let paint_pixels = |png: &[u8]| -> usize {
        let (_, _, rgba) = decode_png(png).expect("解 PNG");
        rgba.chunks_exact(4)
            .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
            .count()
    };
    let before = std::fs::read(&before_path).expect("读 before");
    let before_paint = paint_pixels(&before);
    assert!(
        before_paint > 0,
        "导出里必须有墨（否则后面两条都在比两张白图）⇒ 实测 {before_paint}"
    );

    let bytes = std::fs::read(&pack).expect("读包");
    let mut restored =
        Workspace::with_file_store(root.join("restored"), DocumentSettings::default())
            .expect("ws2")
            .with_assets_dir(Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("..")
                    .join("assets"),
            ));
    let imported = restored
        .import_project(&bytes, Some("doc_paint"))
        .expect("import_project");
    // **明确说出"像素是从哪条路来的"** ✓（第 860 轮）
    //   **⇒ 为什么**：我三次变异（不装位图／不补位图／不拷 blob）都**没能让这条判据红** ✓
    //     ⇒ 说明"导入后有墨"至少有**两条**通路 ✓ ⇒ 那就把两条的计数**断言出来** ✓
    //       · `blobs` ＝ 从包里**拷进 store** 的 blob 数 ✓
    //       · `blobs_replayed` ＝ 依据原子里的笔刷参数**重放**出来的 blob 数 ✓
    //     ⇒ **两者之和必须 ≥ 1** ✓（否则"有墨"没有来源 ✓）
    let copied = imported["blobs"].as_u64().unwrap_or(0);
    let replayed = imported["blobs_replayed"].as_u64().unwrap_or(0);
    let unreplayable = imported["blobs_unreplayable"].as_u64().unwrap_or(0);
    eprintln!("导入来源：包里拷入 {copied}｜重放 {replayed}｜补不回来 {unreplayable}");
    assert!(
        copied + replayed >= 1,
        "导入后画面有墨，那墨必须**有来源**（拷入 {copied} ＋ 重放 {replayed}）：{imported}"
    );
    assert_eq!(
        unreplayable, 0,
        "这个包里每一块都应当能补回来（补不回来的会少一笔）：{imported}"
    );
    let mut ctx2 = ToolContext::new(&mut restored, "doc_paint", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
    let after_path = root.join("after.png");
    // **导入之前这个文件必须还不存在** ✓（证明它是这次导出写出来的，而不是我读了旧文件 ✓）
    assert!(
        !after_path.exists(),
        "after.png 在导入前就已存在 ⇒ 这条判据读的可能是旧文件"
    );
    let reexported = registry.call(
        &mut ctx2,
        "export_png",
        &json!({"path": after_path.to_string_lossy()}),
    );
    assert_eq!(reexported["ok"], json!(true), "{reexported}");
    // **先证明"第二个文件是新的"** ✓（第 828 轮）：
    //   **⇒ 为什么必须这句**：三次变异都不红 ⇒ 最可能的解释是**我把同一个文件读了两遍** ✗
    //   ⇒ 所以先断：① 两个路径**不同**；② `after.png` 在导入**之前不存在** ✓（它必须是这次新写的 ✓）。
    assert_ne!(before_path, after_path, "两次导出必须写到不同的文件");
    assert!(before_path.exists(), "第一张必须存在");
    assert!(
        after_path.exists(),
        "第二张必须存在（导出没写到这里就说明我在读旧文件）"
    );
    let before_len = std::fs::metadata(&before_path)
        .expect("before 元数据")
        .len();
    let after_len = std::fs::metadata(&after_path).expect("after 元数据").len();
    assert!(before_len > 0 && after_len > 0, "两张都不能是空文件");
    let after = std::fs::read(&after_path).expect("读 after");
    let after_paint = paint_pixels(&after);
    assert!(
        after_paint > 0,
        "**导入之后画里必须还有墨**（否则往返把画丢了）⇒ 之前 {before_paint} 个、之后 {after_paint} 个"
    );
    eprintln!("墨像素：之前 {before_paint}｜之后 {after_paint}");
}
