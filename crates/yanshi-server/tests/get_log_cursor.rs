//! **`get_log` 的 `next_since` 必须是本页最大的 seq**（第 734 轮；外部审计 S-07 第 ① 项）
//!
//! **⇒ 缺陷**：游标取 `atoms.last()` ✗ ⇒ `order: "newest"` 时**最后一条是最旧的** ✓
//!   ⇒ 游标**往回退** ⇒ 调用方拿它续读会**重复上一页**（甚至永远推不动 ✓）。
//!
//! **⇒ 恒等式**（不是单点 ✓）：**`next_since` = 本页 `seq` 的最大值，与 `order` 无关** ✓
//!   —— "我已经看到哪了"只取决于**最大的那个序号** ✓。
//! **变异**：改回 `atoms.last()` ⇒ `order: newest` 那条红 ✓（游标会比本页最大值小 ✓）。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_gl", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn the_log_cursor_is_the_largest_seq_on_the_page_whatever_the_order() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_gl", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);
    for index in 0..3 {
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("s{index}"),
                "data": {"points": [[4.0, 4.0 + index as f64 * 4.0], [20.0, 4.0 + index as f64 * 4.0]],
                         "size": 3.0, "hardness": 1.0,
                         "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }

    // **`order` 的取值是 `desc`／`newest_first`** ✓（第 734 轮实测：我一开始传 `"newest"`
    //   ⇒ 实现**不认** ⇒ 两种顺序其实都是升序 ⇒ 变异**测不出来** ✗ ⇒ 判据要**用它对的名字** ✓）
    for order in ["desc", "newest_first"] {
        for limit in [1_u64, 2, 3] {
            let page = registry.call(
                &mut ctx,
                "get_log",
                &json!({"order": order, "limit": limit}),
            );
            assert_eq!(page["ok"], json!(true), "{order}/{limit}: {page}");
            let atoms = page["atoms"].as_array().cloned().unwrap_or_default();
            assert!(
                !atoms.is_empty(),
                "{order}/{limit}: 一条都没有 ⇒ 判据测不到东西"
            );
            let max_seq = atoms
                .iter()
                .filter_map(|atom| atom["seq"].as_u64())
                .max()
                .expect("至少一条带 seq");
            assert_eq!(
                page["next_since"].as_u64(),
                Some(max_seq),
                "{order}/{limit}：next_since 不是本页最大 seq（{max_seq}）⇒ 游标会往回退"
            );
        }
    }
}
