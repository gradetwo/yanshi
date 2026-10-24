//! **`export_png` 必须反映 tombstone**（第 823 轮；台账第四百七十九节的二分实验）
//!
//! **⇒ 要回答的问题**：第 798 轮我做了变异「导入时丢掉 tombstone 行」⇒ **PNG 字节不变** ✗
//!   而我在台账里给出的解释（"导出拿到包里带来的 `render.png` 缓存"）**是错的** ✗
//!   —— `service.rs:3098` 写着 `export_png` 走 `render_region_raw` ✓ ⇒ **绕过缓存** ✓。
//!
//! **⇒ 这个判据比的是**像素**而不是压缩字节** ✓：
//!   ① 画两笔到 `layer_default` ⇒ 删掉该层（tombstone）⇒ `export_png`
//!   ② 同一个根里**再建一份文档**，同样的两笔但**不删层** ⇒ `export_png`
//!   ③ 两份 PNG 解成 RGBA ⇒ **必须不同** ✓（删层之后那两笔看不见 ✓）
//!
//! **⇒ 变异**：让 `delete_layer` 不落 tombstone（或让渲染忽略它）⇒ 两份相同 ⇒ 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

#[test]
fn exporting_a_document_with_a_tombstone_differs_from_the_same_document_without_one() {
    use yanshi_render::png::decode_png;

    let root = std::env::temp_dir().join(format!("yanshi_export_tomb_{}", std::process::id()));
    // **跑完即删** ✓（`/tmp` 是 1.9 GB tmpfs ✓）
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
    let registry = ToolRegistry::with_profiles(&Profile::ALL);

    let paint = |workspace: &mut Workspace, doc: &str, delete_layer: bool, tag: &str| -> Vec<u8> {
        workspace
            .create_document(NewDocument::new(doc, 64, 64), "human:1", "s")
            .expect("create_document");
        let mut ctx = ToolContext::new(workspace, doc, "human:1", "s")
            .with_owner(true)
            .with_wait_for_render(true, 8_000);
        let drawn = registry.call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("o_{tag}"),
                "brush": "8B_Pencil#1",
                "points": [[8.0, 8.0, 1.0], [50.0, 50.0, 1.0]],
                "size": 8.0,
                "color": {"r": 240, "g": 20, "b": 20, "a": 255},
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
        if delete_layer {
            let removed = registry.call(
                &mut ctx,
                "delete_layer",
                &json!({"layer_id": "layer_default"}),
            );
            assert_eq!(removed["ok"], json!(true), "{removed}");
        }
        let path = root.join(format!("{tag}.png"));
        let exported = registry.call(
            &mut ctx,
            "export_png",
            &json!({"path": path.to_string_lossy()}),
        );
        assert_eq!(exported["ok"], json!(true), "{exported}");
        std::fs::read(&path).expect("读导出 PNG")
    };

    let with_tombstone = paint(&mut workspace, "doc_tomb", true, "tomb");
    let without_tombstone = paint(&mut workspace, "doc_plain", false, "plain");

    let (w0, h0, a) = decode_png(&with_tombstone).expect("解第一张");
    let (w1, h1, b) = decode_png(&without_tombstone).expect("解第二张");
    assert_eq!((w0, h0), (w1, h1), "两张图尺寸必须相同");
    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    assert!(
        differing > 0,
        "删掉画了内容的图层之后，导出必须**看得见差别**（否则 tombstone 被忽略或导出走了缓存）"
    );
    eprintln!("两张 PNG 的像素差 {differing} 字节（共 {}）", a.len());
}
