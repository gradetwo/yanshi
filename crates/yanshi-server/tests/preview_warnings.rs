//! 判据：**预览也必须说清"哪些补丁没画"** ✗。
//!
//! 用户看到的常常不是 `render_region` 的结果，而是**缩略图预览**（打开文档、提交一笔
//! 之后的 `preview.thumb_url`）✓。真实事故复盘暴露的缺口是：`render_document_preview`
//! 渲染时跳过了补丁，却**既不留 `last_render_warnings`、也不把告警放进返回的
//! `warnings`** ✗，而 `PreviewInfo::to_json` 也**整个丢掉** `warnings` ✗
//! ⇒ 一张"缺了补丁的画"被静默地交给用户 ✓ —— 它和"数据丢了"长得一模一样 ✗。
//!
//! 变异（任一处即可让本判据红 ✓）：
//! * 把 `Document::render_document_preview` 里新加的 `self.last_render_warnings = …`
//!   与返回值的 `warnings: render_warnings` 去掉；
//! * 或把 `PreviewInfo::to_json` 里新加的 `"warnings"` 键去掉。

use serde_json::{json, Value};
use yanshi_server::archive::{read_tar, write_tar};
use yanshi_server::{DocumentSettings, PreviewInfo, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yanshi_preview_warn_{}_{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            yanshi_server::NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

#[test]
fn document_preview_reports_patches_it_could_not_draw() {
    let root = temp_dir("src");
    let doc = "doc_preview_warn";
    let mut source = workspace(&root, doc, 400, 300);
    assert_eq!(
        call(&mut source, doc, "create_layer", json!({"layer_id": "L"}))["ok"],
        json!(true)
    );
    // **故意用读画布的笔刷** ✓（`oil-01-paint` 的 `smudge > 0` ✓）：导入端重放不出它 ✓
    // ⇒ 包里那条 blob 被抠掉之后，就是**真缺** ✓。
    let made = call(
        &mut source,
        doc,
        "brush_stroke",
        json!({"layer_id": "L", "object_id": "s1", "brush": "oil-01-paint.myb", "size": 40,
               "color": {"r": 20, "g": 120, "b": 220, "a": 255},
               "points": [[60.0, 100.0, 1.0], [340.0, 180.0, 1.0]]}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let pack = root.join("pack.yanshi");
    let exported = call(
        &mut source,
        doc,
        "export_project",
        json!({"path": pack.to_string_lossy(), "doc_id": doc, "include_bitmaps": false}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");

    // 抠掉那条 blob ⇒ 真缺。
    let bytes = std::fs::read(&pack).expect("读回包");
    let mut entries = read_tar(&bytes).expect("解包");
    let dropped = entries
        .iter()
        .find(|entry| entry.path.starts_with("blobs/sha256/"))
        .map(|entry| entry.path.clone())
        .expect("这个文档应当有 blob");
    let dropped_hash = format!("sha256:{}", dropped.rsplit('/').next().unwrap());
    entries.retain(|entry| entry.path != dropped);
    let doctored = root.join("doctored.yanshi");
    std::fs::write(&doctored, write_tar(&entries)).expect("写回包");

    // 导入到新工作区：文档照开（缺一个补丁不该让整包失败）。
    let restore_root = temp_dir("restore");
    let mut restored =
        Workspace::with_file_store(restore_root.clone(), DocumentSettings::default())
            .expect("落盘工作区应当能建")
            .with_assets_dir(Some(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ));
    let imported = call(
        &mut restored,
        doc,
        "import_project",
        json!({"path": doctored.to_string_lossy(), "doc_id": doc}),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");

    // **打开文档时用户看到的那张预览**：必须报出"这块没画"。
    let preview = restored
        .document_mut(doc)
        .expect("文档已打开")
        .render_document_preview()
        .expect("预览渲染");
    let joined = preview.warnings.join(" | ");
    assert!(
        joined.contains("缺少 blob") && joined.contains(&dropped_hash),
        "预览跳过了补丁却一个告警都没有 ⇒ 静默的不完整画面：{joined}"
    );

    // 提交响应里的 `preview.warnings` 也必须带上（`PreviewInfo::to_json`）。
    let as_json = PreviewInfo::Fresh(Box::new(preview)).to_json();
    let warnings = as_json["warnings"].to_string();
    assert!(
        warnings.contains("缺少 blob") && warnings.contains(&dropped_hash),
        "`preview` 字段丢了告警 ⇒ 客户端看不到：{as_json}"
    );

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&restore_root);
}
