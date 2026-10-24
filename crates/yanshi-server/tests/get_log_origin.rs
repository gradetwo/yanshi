//! **`get_log` 必须给出 `origin`（来源），不能只有 `kind`（存储形态）**（第 781 轮；外部 D5）
//!
//! **⇒ 他们的实测**：一幅 8K 画的 39,354 条笔触原子 `kind` **全是 `"import_image"`** ✗
//! ⇒ 拿 `kind` 做来源审计会把「画出来的 38k 笔」误判成「导入了一张图」✓
//! ⇒ 他们能用的唯一抓手是 `brush != null` ✓。
//!
//! **⇒ 现状**：`list_atoms` **早就有** `origin` ✓（第 642／659 轮），而 `get_log` **没有** ✗
//! ⇒ 他们的脚本走 `get_log` ✓ ⇒ **只改一边等于没修** ✓（本仓「两条接口不同形」的老坑 ✓）。
//!
//! **⇒ 判据**：画一笔 ⇒ `get_log` 里那一笔的 `origin` 必须是 **`drawn`** ✓（不是 `imported` ✓），
//! 而 `kind` 仍然是存储形态（`import_image` ✓ —— 这条**不要求改** ✓，它是存储事实 ✓）。
//! **变异**：删掉 `origin` 字段 ⇒ 红 ✓；把它写成常量 `"imported"` ⇒ 也红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn get_log_says_where_a_stroke_came_from() {
    let root = std::env::temp_dir().join(format!("yanshi_getlog_origin_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace =
        Workspace::with_file_store(root, DocumentSettings::default()).expect("落盘工作区");
    workspace
        .create_document(NewDocument::new("doc_org", 64, 64), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_org", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_drawn",
            "data": {"points": [[4.0, 4.0], [40.0, 40.0]], "size": 3.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let mut ctx = ToolContext::new(&mut workspace, "doc_org", "human:1", "s").with_owner(true);
    let log = registry.call(&mut ctx, "get_log", &json!({"limit": 50}));
    let entries = log["atoms"].as_array().cloned().unwrap_or_default();
    assert!(!entries.is_empty(), "`get_log` 一条都没有：{log}");
    for entry in &entries {
        assert!(
            entry.get("origin").is_some(),
            "`get_log` 的每条都要带 `origin`（来源）⇒ 否则调用方只能拿 `kind` 猜：{entry}"
        );
    }
    let stroke = entries
        .iter()
        .find(|entry| {
            entry["object_id"] == json!("o_drawn")
                || entry["payload"]["object_id"] == json!("o_drawn")
        })
        .cloned();
    let Some(stroke) = stroke else {
        panic!("`get_log` 里找不到刚画的那一笔：{log}");
    };
    assert_eq!(
        stroke["origin"],
        json!("drawn"),
        "画出来的一笔，`origin` 必须是 `drawn`（`kind` 是存储形态，两者不同）：{stroke}"
    );
}
