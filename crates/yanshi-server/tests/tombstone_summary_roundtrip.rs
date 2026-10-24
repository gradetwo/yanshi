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
