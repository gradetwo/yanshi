//! **`clear_reference` 的 `cleared` 必须说真话**（第 876 轮）
//!
//! **⇒ 缺陷**：原来**无条件**返回 `cleared: true` ✗ ⇒ 从没设过参考图时也报"清掉了" ✓
//!   —— 本仓头号病根「假成功」的形态 ✓（"做了 0 次"冒充"不需要做" ✓）。
//!
//! **⇒ 判据**：① 没设过 ⇒ `clear_reference` 返回 `ok: true` 且 **`cleared: false`** ✓；
//!   ② 设过之后 ⇒ `cleared: true` ✓；③ 再清一次 ⇒ 又回 `false` ✓（幂等且如实 ✓）。
//! **⇒ 变异**：把 `cleared` 写成常量 `true` ⇒ ① 与 ③ 同时红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

#[test]
fn clearing_a_reference_that_was_never_set_says_so() {
    let root = std::env::temp_dir().join(format!("yanshi_clear_ref_{}", std::process::id()));
    /// **跑完即删** ✓
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("根");
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).expect("工作区");
    workspace
        .create_document(NewDocument::new("doc_ref", 128, 128), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_ref", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    // ① 没设过 ⇒ cleared 必须是 **false**
    let first = registry.call(&mut ctx, "clear_reference", &json!({}));
    assert_eq!(first["ok"], json!(true), "没设过也不算错：{first}");
    assert_eq!(
        first["cleared"],
        json!(false),
        "从没设过参考图 ⇒ `cleared` 必须是 **false**（不许假报成功）：{first}"
    );
    // ② 设过 ⇒ true
    let put = registry.call(
        &mut ctx,
        "put_blob",
        &json!({"data_base64": "yMjIyMjIyMjIyMjIyMjIyA==", "mime_type": "image/x-yanshi-raw"}),
    );
    assert_eq!(put["ok"], json!(true), "{put}");
    let hash = put["blob_hash"].as_str().expect("blob_hash").to_owned();
    let set = registry.call(
        &mut ctx,
        "set_reference",
        &json!({"blob_hash": hash, "opacity": 0.5}),
    );
    assert_eq!(set["ok"], json!(true), "{set}");
    let second = registry.call(&mut ctx, "clear_reference", &json!({}));
    assert_eq!(
        second["cleared"],
        json!(true),
        "设过参考图之后清掉 ⇒ `cleared` 必须是 true：{second}"
    );
    // ③ 再清一次 ⇒ 又回 false（幂等且如实）
    let third = registry.call(&mut ctx, "clear_reference", &json!({}));
    assert_eq!(
        third["cleared"],
        json!(false),
        "第二次清 ⇒ 已经没有参考图了 ⇒ 必须报 false：{third}"
    );
}
