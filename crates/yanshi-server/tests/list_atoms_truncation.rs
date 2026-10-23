//! **`list_atoms` 的 `truncated` 不许假阳性**（第 733 轮；外部审计 S-07 第 ② 项）
//!
//! **⇒ 缺陷**：`truncated = hits.len() >= limit` ✗ ⇒
//!   ① 匹配数**正好等于** `limit`、后面**没有**更多 ⇒ 也报 `truncated: true` ✓（假阳性 ✓）；
//!   ② `limit: 0` ⇒ `0 >= 0` ⇒ 同样报截断 ✓（而一条都没返回 ✓）。
//!
//! **⇒ 为什么必须传 `include_non_stroke: true`** ✓（第 733 轮实测）：
//!   缺省**只列带 `payload.source` 的笔迹** ✓ ⇒ 我上一轮的测试拿到的 `count` 是 **0** ✗
//!   ⇒ 断言无从观察 ✓（那次判据被我删掉了 ✓，这是接上它的那次 ✓）。
//!
//! **⇒ 判据**（跨 `limit` 的恒等式 ✓）：先问全量得 `total` ⇒
//!   · `expected = limit > 0 && limit < total`
//!   · `truncated` 必须等于 `expected` ✓，且 `truncated` 与 `has_more` **一致** ✓
//! **变异**：改回 `hits.len() >= limit`（且不截断列表）⇒ `limit == total` 那条红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_la", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn list_atoms_only_reports_truncation_when_something_is_left() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_la", 32, 32),
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

    let all = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"limit": 500, "include_non_stroke": true}),
    );
    let total = all["count"].as_u64().unwrap_or(0);
    assert!(
        total >= 4,
        "整份日志至少 4 条（建文档 + 3 笔），实测 {total} ⇒ 判据测不到东西：{all}"
    );

    for limit in [0_u64, 1, 2, 3, 10] {
        let page = registry.call(
            &mut ctx,
            "list_atoms",
            &json!({"limit": limit, "include_non_stroke": true}),
        );
        assert_eq!(page["ok"], json!(true), "limit={limit}: {page}");
        let count = page["count"].as_u64().unwrap_or(99);
        let truncated = page["truncated"].as_bool().unwrap_or(false);
        let has_more = page["has_more"].as_bool().unwrap_or(false);
        assert_eq!(
            truncated, has_more,
            "limit={limit}：truncated 与 has_more 必须一致（同一件事的两个名字）"
        );
        let expected = limit > 0 && limit < total;
        assert_eq!(
            truncated, expected,
            "limit={limit}：全量 {total} 条、这一页 {count} 条，却说 truncated={truncated}（正确值 {expected}）⇒ 假阳性／假阴性"
        );
    }
}
