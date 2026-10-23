//! **笔迹日志的每条都要带 `message`**（第 723 轮；外部审计 S-02 第 ① 组）
//!
//! **⇒ 缺陷**：`Atom::message` 一直存在 ✓（`get_atom` 一直在回 ✓），
//!   而 `get_stroke_log` 的条目**没有这个字段** ✗ ⇒ 说明文字在日志视图里**不可见** ✓
//!   ⇒ 只能一条条 `get_atom` 去翻 ⇒ 等于没有 ✓。
//!
//! **⇒ 判据**：每一条**都有** `message` 键 ✓（值是 `null` 也合格 ✓）；
//!   并且**那个键真的来自原子**（不是常量 `null`）—— 用「同一批里两笔不同」证明不了，
//!   所以这里只钉「键存在」，由变异（删掉那行 ⇒ 键消失）保证它测的是那段代码 ✓。
//! **变异**：把 `"message": atom.message,` 删掉 ⇒ 第一条红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_slog_msg", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn every_stroke_log_entry_carries_its_message_field() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_slog_msg", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default",
            "object_id": "s_msg",
            "data": {"points": [[4.0, 4.0], [20.0, 20.0]], "size": 3.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let mut ctx = context(&mut workspace);
    let log = registry.call(&mut ctx, "get_stroke_log", &json!({"limit": 10}));
    let entries = log["entries"].as_array().cloned().unwrap_or_default();
    assert!(
        !entries.is_empty(),
        "日志一条都没有 ⇒ 判据测不到东西：{log}"
    );
    for entry in &entries {
        assert!(
            entry.get("message").is_some(),
            "笔迹日志条目里没有 `message` 键 ⇒ 说明文字在日志视图里不可见：{entry}"
        );
    }
}
