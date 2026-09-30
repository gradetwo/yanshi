//! **被拒绝的操作不该在日志里留下痕迹** ✓ —— 子 agent 报的 G2 后半：
//! 6 次被拒的 `create_layer` 把 `head_seq` 从 255 推到了 261 ✗。
//!
//! 这条要求很实在 ✓：
//! * 客户端会为**无内容变化**的 head 白白重同步一次 ✗（介质那轮刚把重同步改成增量 ✓）；
//! * 日志被垃圾填满 ✓，重放成本随之上升 ✓（而重放成本正是文档规模的主要开销 ✓）；
//! * 用户看到"操作失败"但历史里多了一条 ✓，无法理解 ✓。
//!
//! `AtomLog::append_validated` 的设计已经写明"校验失败时原子**不进日志**" ✓，
//! 所以这里量的是**实际行为**是否与设计一致 ✓，并覆盖两条拒绝路径：
//! **提交时校验**（`MustNotExist` ✓）与**折叠时校验**（墓碑 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_reject", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn head(workspace: &mut Workspace) -> u64 {
    let mut ctx = context(workspace);
    let summary = registry().call(&mut ctx, "get_document", &json!({"preview_size": false}));
    summary["head_seq"].as_u64().unwrap_or(0)
}

/// 被拒的操作必须**完全不改变** `head_seq` ✓（提交时校验与折叠时校验两条路径都算 ✓）。
#[test]
fn rejected_operations_do_not_advance_the_head() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_reject", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L", "name": "L"}),
        );
    }

    // ① **提交时校验**（MustNotExist ✓）：存活图层不得重复创建 ✓。
    let before = head(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let rejected = registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L", "name": "dup"}),
        );
        assert_eq!(
            rejected["ok"],
            json!(false),
            "存活图层不该能重复创建：{rejected}"
        );
    }
    let after = head(&mut workspace);
    assert_eq!(
        after, before,
        "被拒的 create_layer 不该推进 head（{before} → {after}）"
    );

    // ② **折叠时校验**（墓碑 ✓）：删除后**未重建**就落笔 ⇒ 必须被拒 ✓，且同样不留痕迹 ✓。
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "delete_layer", &json!({"layer_id": "L"}));
    }
    let before_deleted = head(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let rejected = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "x",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 1.0, "y": 1.0, "w": 4.0, "h": 4.0}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        );
        assert_eq!(
            rejected["ok"],
            json!(false),
            "已删除图层不该能落笔：{rejected}"
        );
    }
    let after_deleted = head(&mut workspace);
    assert_eq!(
        after_deleted, before_deleted,
        "被拒的 draw_shape 不该推进 head（{before_deleted} → {after_deleted}）"
    );

    // ③ 反向对照 ✓：**成功**的操作当然要推进 head ✓（否则上面两条断言会因为"head 根本不动"而假通过 ✗）。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "M", "name": "M"}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }
    assert!(
        head(&mut workspace) > after_deleted,
        "成功的操作必须推进 head（否则本用例会假通过 ✗）"
    );
}
