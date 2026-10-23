//! **`get_stroke_log.group_id` 必须真的筛**（第 721 轮；外部审计 S-02 第 ① 组）
//!
//! **⇒ 缺陷**：`group_id` 在 `tools_registry.rs` 声明了 ✓（文档说"可按它筛 ✓"），
//!   而 handler **从没读过**它 ✗ ⇒ 传了也**返回全部** ✓ —— 「声明了但不生效」= 静默降级 ✓。
//!
//! **⇒ 判据**：两组笔（`group_id` 不同）⇒ 按其中一组筛，**只返回那一组** ✓；
//!   不传 `group_id` ⇒ 两组都在 ✓（缺省行为不变 ✓）。
//! **变异**：去掉那段 `if group_filter … { continue; }` ⇒ 第一条红（两组都回来）✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_group", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn stroke(workspace: &mut Workspace, object_id: &str, group: &str) {
    let registry = registry();
    let mut ctx = context(workspace);
    let response = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default",
            "object_id": object_id,
            "group_id": group,
            "data": {"points": [[4.0, 4.0], [20.0, 20.0]], "size": 3.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
}

#[test]
fn the_stroke_log_filters_by_group_id() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_group", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    stroke(&mut workspace, "s_a", "grp_a");
    stroke(&mut workspace, "s_b", "grp_b");

    let registry = registry();
    let mut ctx = context(&mut workspace);
    let filtered = registry.call(&mut ctx, "get_stroke_log", &json!({"group_id": "grp_a"}));
    assert_eq!(filtered["ok"], json!(true), "{filtered}");
    let entries = filtered["entries"].as_array().cloned().unwrap_or_default();
    assert!(
        !entries.is_empty(),
        "按 grp_a 筛之后**一条都没有** ⇒ 要么筛错了、要么 group_id 没写进原子：{filtered}"
    );
    let ids: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry["object_id"].as_str().map(str::to_owned))
        .collect();
    assert!(
        ids.iter().all(|id| id == "s_a"),
        "按 grp_a 筛却拿到了别的组：{ids:?}（缺陷形态：声明了但没读 ⇒ 返回全部）"
    );

    // 不传 ⇒ 两组都在 ✓
    let mut ctx = context(&mut workspace);
    let all = registry.call(&mut ctx, "get_stroke_log", &json!({}));
    let total = all["entries"].as_array().map(Vec::len).unwrap_or(0);
    assert!(
        total >= 2,
        "不传 group_id 时应当两组都在，实测 {total} 条：{all}"
    );
}
