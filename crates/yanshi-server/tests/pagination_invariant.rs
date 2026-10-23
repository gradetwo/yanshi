//! **`truncated` 与 `next_since` 必须自洽**（第 731 轮；外部审计 S-07 第 ③ 项）
//!
//! **⇒ 缺陷**：两个字段各自独立算 ⇒ `limit: 0` 在非空文档上给出
//!   `returned: 0, truncated: true, next_since: null` ✓
//!   ⇒ 调用方读到"还有更多，但没给游标" ⇒ **永远翻不到下一页** ✗（两个字段互相矛盾 ✓）。
//!
//! **⇒ 恒等式**（不是单点断言 ✓）：`truncated` ⇒ **满页** 且 **有游标**；且 `limit: 0` 不报截断 ✓。
//!   对 `limit` 取 0／1／2／5 都成立 ✓ ⇒ 跨输入成立，不怕输入变 ✓。
//!   **⚠ 我第一版写的是「truncated ⇔ 有 next_since」，被 `limit=5` 反例否掉了** ✓
//!   （`next_since` 是**续读游标，总是给** ✓）⇒ 记录在此，别再写回去 ✓。
//! **变异**：改回 `truncated: total > limit` ⇒ `limit: 0` 那条红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_pg", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn find_atom_never_says_truncated_without_a_cursor() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pg", 32, 32),
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

    for limit in [0_u64, 1, 2, 5] {
        let page = registry.call(&mut ctx, "find_atom", &json!({"limit": limit}));
        assert_eq!(page["ok"], json!(true), "limit={limit}: {page}");
        let truncated = page["truncated"].as_bool().unwrap_or(false);
        let has_cursor = page["next_since"].is_u64();
        let returned = page["returned"].as_u64().unwrap_or(0);
        // **恒等式**：`truncated` ⇒ **这一页是满的** 且 **给了游标** ✓
        //   （`next_since` 是"续读游标"，**总是**给 ✓ ⇒ 不能用"有没有游标"当截断判据 ✗ ——
        //    我第一次就是这么写的，被 `limit=5` 反例否掉了 ✓）。
        if truncated {
            assert_eq!(
                returned, limit,
                "limit={limit}：说被截断，却只回了 {returned} 条（截断 = 满了还有剩）"
            );
            assert!(
                has_cursor,
                "limit={limit}：说被截断却没给 next_since ⇒ 永远翻不到下一页"
            );
        }
        // **`limit: 0` 不许报截断** ✓：一条都没给 ⇒ "还有更多"没有意义 ✓（这正是缺陷形态 ✓）
        if limit == 0 {
            assert!(
                !truncated,
                "limit=0 却报 truncated=true ⇒ 与 next_since 一起构成矛盾（审计 S-07 第 ③ 项）"
            );
        }
    }
}
