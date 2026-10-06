//! **只读的撤销状态**（F02）—— `get_undo_status` 必须与 `undo_last` 同源，且**什么都不改**。
//!
//! **为什么需要它**：`remaining_gestures` 原先**只由 `undo_last` / `redo_last` 的响应回报**，
//! 于是"落笔之后到用户点撤销之前"，查看器**没有新数字** ⇒ 按钮停在禁用 ⇒「刚画完却不能撤销」。
//! 修法是让查看器**随时问一次**只读状态 ⇒ **但前提是这个查询必须真的只读、且报同一个数**。
//!
//! **本测试钉住三条**（每条都能红）：
//! ① 空文档 ⇒ `remaining_gestures == 0`（与 `undo_last` 的行为一致，**不是** `null` 也不是省略）；
//! ② **连查两次 ⇒ 结果完全相同**（它不该消耗任何东西）；
//! ③ **查它不改变文档** ⇒ 前后 `get_log` 的原子数一致（若它偷偷发 revert，这条立刻红）。

use serde_json::json;
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_undo_status", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 原子数（用 `get_log` 读 ⇒ **黑盒地**判断文档有没有被改）。
fn atom_count(ctx: &mut ToolContext<'_>) -> usize {
    let log = registry().call(ctx, "get_log", &json!({}));
    log.get("atoms")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .or_else(|| {
            log.get("count")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
        })
        .unwrap_or_else(|| panic!("get_log 的形状变了：{log}"))
}

#[test]
fn undo_status_is_read_only_and_reports_a_number() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "new_document",
            &json!({ "doc_id": "doc_undo_status", "width": 160, "height": 120 }),
        );
    }

    let mut ctx = context(&mut workspace);
    let before = atom_count(&mut ctx);

    // ① 空文档 ⇒ 0（**不是** null、**不是**缺失）
    let first = registry().call(&mut ctx, "get_undo_status", &json!({}));
    let remaining = first
        .get("remaining_gestures")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| panic!("get_undo_status 没给 remaining_gestures：{first}"));
    assert_eq!(
        remaining, 0,
        "空文档应当报 0 笔可撤销（实得 {remaining}）⇒ {first}"
    );

    // ② 连查两次 ⇒ 完全相同（它不消耗任何东西）
    let second = registry().call(&mut ctx, "get_undo_status", &json!({}));
    assert_eq!(
        first.get("remaining_gestures"),
        second.get("remaining_gestures"),
        "两次只读查询给出了不同的数 ⇒ 它不是只读的：{first} vs {second}"
    );

    // ③ 查它不改变文档
    let after = atom_count(&mut ctx);
    assert_eq!(
        before, after,
        "查询撤销状态改变了文档（原子数 {before} ⇒ {after}）⇒ 它必须只读"
    );
}
