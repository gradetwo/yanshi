//! **含 tombstone 的包往返之后，文档状态必须一致**（第 799 轮；外部 qoder 的 D174，换一个可判的量）
//!
//! **⇒ 上一轮为什么失败** ✗：我比的是 `export_png` 的字节 ✓ ⇒ 而导入之后它**很可能直接给包里那份
//!   `render.png` 缓存** ✓ ⇒ "两次相同"证明的是**缓存被搬过来了** ✗，不是"重放一样" ✗
//!   ⇒ 两次变异（导入时丢掉 tombstone）**都不红** ✓ ⇒ 无鉴别力 ⇒ 已删除 ✓。
//!
//! **⇒ 这一轮换成状态量**（可判、而且不经过渲染缓存 ✓）：
//!   **`atoms`／`layers`／`objects`／`head_seq`** 四项在 `import_project` 前后必须**完全相同** ✓。
//!   **⇒ 为什么这能抓 D174 那一类**：round-trip 若把某类原子丢了或顺序搞错 ✓
//!   状态折叠结果就会不同 ✓（例如丢 tombstone ⇒ `layers` 多一层 ✓）。
//! **⇒ 变异**：导入时丢掉含 `tombstone` 的行 ⇒ `layers` 从 1 变 2 ⇒ 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};
/// **测试结束即删掉临时目录** ✓（第 814 轮；`/tmp` 是 1.9 GB 的 tmpfs ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn call(ws: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(ws, "doc_ts", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
    registry.call(&mut ctx, tool, &args)
}

fn summary(ws: &mut Workspace) -> serde_json::Value {
    let out = call(ws, "get_document", json!({}));
    json!({
        "atoms": out["atoms"], "layers": out["layers"],
        "objects": out["objects"], "head_seq": out["head_seq"],
    })
}

#[test]
fn a_package_with_a_tombstone_keeps_the_same_document_state() {
    let root = std::env::temp_dir().join(format!("yanshi_ts_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut ws = Workspace::with_file_store(root.clone(), DocumentSettings::default()).expect("ws");
    ws.create_document(NewDocument::new("doc_ts", 128, 128), "human:1", "s")
        .expect("create");

    // 默认层画一笔；再建 L2、在 L2 里画一笔、然后**删掉 L2**（tombstone）
    let base = call(
        &mut ws,
        "draw_stroke",
        json!({
            "layer_id": "layer_default", "object_id": "o_base",
            "data": {"points": [[10.0, 10.0], [100.0, 100.0]], "size": 6.0, "hardness": 1.0,
                     "color": {"r": 30, "g": 200, "b": 90, "a": 255}},
        }),
    );
    assert_eq!(base["ok"], json!(true), "{base}");
    let made = call(
        &mut ws,
        "create_layer",
        json!({"layer_id": "L2", "name": "L2"}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let inside = call(
        &mut ws,
        "draw_stroke",
        json!({
            "layer_id": "L2", "object_id": "o_l2",
            "data": {"points": [[20.0, 100.0], [110.0, 20.0]], "size": 10.0, "hardness": 1.0,
                     "color": {"r": 240, "g": 30, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(inside["ok"], json!(true), "{inside}");
    let tomb = call(&mut ws, "delete_layer", json!({"layer_id": "L2"}));
    assert_eq!(tomb["ok"], json!(true), "{tomb}");

    let before = summary(&mut ws);

    let pack = root.join("doc_ts.yanshi");
    let exported = call(
        &mut ws,
        "export_project",
        json!({"path": pack.to_string_lossy()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读包");
    let mut restored =
        Workspace::with_file_store(root.join("restored"), DocumentSettings::default())
            .expect("ws2");
    restored
        .import_project(&bytes, Some("doc_ts"))
        .expect("import_project");

    // **强制重放状态**：`get_document` 走的是折叠后的 `state` ✓（不是 PNG 缓存 ✓）
    let after = summary(&mut restored);
    assert_eq!(
        after, before,
        "含 tombstone 的包往返之后文档状态变了（D174 的形态）：\n之前 {before}\n之后 {after}"
    );
}

/// **往返之后重渲染必须逐像素相同**（第 801 轮；外部 qoder D174 的**像素**那一半）
///
/// **⇒ 为什么不用 `export_png`** ✗：导入后它很可能直接给**包里带来的 `render.png`** ✓
///   ⇒ 那比较的是"缓存搬没搬" ✗，不是"重放出来一样不一样" ✓（第 798 轮实测：变异不红 ✓）。
/// **⇒ 这里绕开缓存**：直接拿**折叠后的 `DocumentState`** 交给 `Renderer::render_region` ✓
///   ⇒ 比的是**引擎从状态渲出来的像素** ✓。
///
/// **⇒ 判据**：同尺寸同一 bbox 渲两次（源文档 vs 导入后的文档）⇒ **逐字节相同** ✓。
/// **变异**：导入时丢掉 tombstone ⇒ 被删的层又活了 ⇒ 像素不同 ⇒ 红 ✓。
#[test]
fn a_round_tripped_document_renders_the_same_pixels() {
    use std::sync::Arc;
    use yanshi_core::blob::MemoryBlobStore;
    use yanshi_render::render::Renderer;
    use yanshi_render::tile::TileGrid;

    let root = std::env::temp_dir().join(format!("yanshi_ts_px_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    // **这个测试用 `doc_px`** ✓ ⇒ `call()` 里写死了 `doc_ts` ✗ ⇒ 这里用专用 helper ✓
    let mut ws = Workspace::with_file_store(root.clone(), DocumentSettings::default()).expect("ws");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let call_px = |ws: &mut Workspace, tool: &str, args: serde_json::Value| -> serde_json::Value {
        let mut ctx = ToolContext::new(ws, "doc_px", "human:1", "s")
            .with_owner(true)
            .with_wait_for_render(true, 8_000);
        registry.call(&mut ctx, tool, &args)
    };
    ws.create_document(NewDocument::new("doc_px", 96, 96), "human:1", "s")
        .expect("create");
    for (layer, object, color) in [
        (
            "layer_default",
            "o_base",
            json!({"r": 30, "g": 200, "b": 90, "a": 255}),
        ),
        (
            "layer_default",
            "o_l2",
            json!({"r": 240, "g": 30, "b": 30, "a": 255}),
        ),
    ] {
        let drawn = call_px(
            &mut ws,
            "draw_stroke",
            json!({
                "layer_id": layer, "object_id": object,
                "data": {"points": [[8.0, 8.0], [88.0, 88.0]], "size": 8.0, "hardness": 1.0,
                         "color": color},
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let tomb = call_px(
        &mut ws,
        "delete_layer",
        json!({"layer_id": "layer_default"}),
    );
    assert_eq!(tomb["ok"], json!(true), "{tomb}");

    let pack = root.join("doc_px.yanshi");
    let exported = call_px(
        &mut ws,
        "export_project",
        json!({"path": pack.to_string_lossy()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&pack).expect("读包");
    let mut restored =
        Workspace::with_file_store(root.join("restored"), DocumentSettings::default())
            .expect("ws2");
    restored
        .import_project(&bytes, Some("doc_px"))
        .expect("import");

    let render = |workspace: &mut Workspace| -> Vec<u8> {
        let state = Arc::new(
            workspace
                .document("doc_px")
                .expect("已打开")
                .state()
                .clone(),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, state.width, state.height).unwrap());
        let out = renderer
            .render_region(
                &state,
                &store,
                yanshi_core::Bbox::new(0.0, 0.0, f64::from(state.width), f64::from(state.height)),
            )
            .expect("render");
        out.rgba8
    };
    let before = render(&mut ws);
    let after = render(&mut restored);
    assert_eq!(
        before.len(),
        after.len(),
        "往返之后帧缓冲大小变了：{} vs {}",
        before.len(),
        after.len()
    );
    assert!(
        before == after,
        "含 tombstone 的包往返之后**重渲染的像素不同**（D174 的形态）：不同字节 {}",
        before
            .iter()
            .zip(after.iter())
            .filter(|(a, b)| a != b)
            .count()
    );
}
