//! **工作区偏好** ✓（目标 ⑧-1：笔刷收藏 / 最近使用 ✓）。
//!
//! **为什么它该在工具层** ✓：能力要**两边都能用** ✓ —— MCP 端也要能读出"这个人常用哪些笔刷" ✓，
//! 所以它不是界面本地存储 ✗（`localStorage` 那种东西 MCP 端永远看不到 ✓）。
//! **判据重点**：**重开工作区之后还在** ✓ —— 否则"收藏"只是本次会话的错觉 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_pref_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_any", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **合并写入 + 按 `null` 删除 + 按键取回** ✓（三条语义一次测清 ✓）。
#[test]
fn preferences_merge_delete_and_filter_by_key() {
    let root = temp_dir("merge");
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    let written = call(
        &mut workspace,
        "set_preferences",
        json!({ "values": { "brush_favorites": ["Pen", "ramon-Knife"], "stroke_size": 18 } }),
    );
    assert_eq!(written["ok"], json!(true), "{written}");
    assert_eq!(
        written["persisted"],
        json!(true),
        "落盘工作区应当报 persisted=true：{written}"
    );
    assert_eq!(
        written["preferences"]["stroke_size"],
        json!(18),
        "{written}"
    );

    // **合并** ✓：只提到 `stroke_size` ⇒ 收藏那项**不动** ✓。
    let merged = call(
        &mut workspace,
        "set_preferences",
        json!({ "values": { "stroke_size": 24 } }),
    );
    assert_eq!(merged["preferences"]["stroke_size"], json!(24), "{merged}");
    assert_eq!(
        merged["preferences"]["brush_favorites"],
        json!(["Pen", "ramon-Knife"]),
        "没提到的键不该被抹掉：{merged}"
    );

    // **`null` = 删掉这个键** ✓（而不是留一个 null ✓）。
    let deleted = call(
        &mut workspace,
        "set_preferences",
        json!({ "values": { "stroke_size": null } }),
    );
    assert!(
        deleted["preferences"].get("stroke_size").is_none(),
        "null 应当**删掉键**，不是留下一个 null：{deleted}"
    );

    // **按键取回** ✓。
    let picked = call(
        &mut workspace,
        "get_preferences",
        json!({ "keys": ["brush_favorites"] }),
    );
    assert_eq!(picked["count"], json!(1), "只要了一个键：{picked}");
    assert_eq!(
        picked["preferences"]["brush_favorites"],
        json!(["Pen", "ramon-Knife"]),
        "{picked}"
    );
}

/// **重开工作区之后偏好还在** ✓ —— 这条才是"收藏"的意思 ✓。
#[test]
fn preferences_survive_reopening_the_workspace() {
    let root = temp_dir("reopen");
    {
        let mut workspace =
            Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
        let written = call(
            &mut workspace,
            "set_preferences",
            json!({ "values": { "brush_favorites": ["classic-knife"] } }),
        );
        assert_eq!(written["ok"], json!(true), "{written}");
    }
    // **换一个进程/换一个工作区对象** ✓ —— 磁盘上那份就是唯一的凭据 ✓。
    let mut reopened =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    let read = call(&mut reopened, "get_preferences", json!({}));
    assert_eq!(
        read["preferences"]["brush_favorites"],
        json!(["classic-knife"]),
        "重开之后收藏应当还在：{read}"
    );
    assert!(
        root.join("preferences.json").is_file(),
        "应当写在 <root>/preferences.json ✓"
    );
}

/// **纯内存工作区：能用，但必须说清"不会留下"** ✗。
#[test]
fn an_in_memory_workspace_says_preferences_will_not_persist() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let written = call(
        &mut workspace,
        "set_preferences",
        json!({ "values": { "brush_favorites": ["Pen"] } }),
    );
    assert_eq!(written["ok"], json!(true), "{written}");
    assert_eq!(
        written["persisted"],
        json!(false),
        "纯内存模式必须**如实报出**不会落盘 ✗（否则用户以为重启后还在 ✓）：{written}"
    );
    // **读得到** ✓：本次进程内是一致的 ✓。
    let read = call(&mut workspace, "get_preferences", json!({}));
    assert_eq!(
        read["preferences"]["brush_favorites"],
        json!(["Pen"]),
        "{read}"
    );
}
