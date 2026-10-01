//! `begin_transaction` / `commit_transaction`（设计 777 ✓）。
//!
//! **设计只给了名字、一个字语义都没写 ⇒ 记录选择** ✓：
//! **事务 = 带"失败回滚"的变更集** ✓ —— 既然设计把它们与 changeset 并列 ✓，就必须**有区别** ✓，
//! 否则就该合成一个名字 ✓。我取的区别是：**事务内某次写操作失败时，把事务中已经落下的原子整体撤销** ✓
//! （变更集只分组 ✓、不回滚 ✓）。
//!
//! **两条对立的行为都要钉住** ✓：
//! * **写失败 ⇒ 回滚** ✓（并**如实报告** `rolled_back` ✓，否则调用方会以为只有这一步失败 ✗）；
//! * **读失败 ⇒ 不回滚** ✓ —— 这条同样重要 ✗：查一次东西就把人家的编辑撤了，是灾难 ✗。
//!
//! 判定"写"用的是 `ToolSpec` 现成的 `mutating` 标志 ✓，而不是另列一张表 ✗（那张表迟早会漏 ✓）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_tx", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_tx", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 235 || pixel[1] < 235 || pixel[2] < 235))
        .count()
}

fn good_stroke(id: &str, y: f64) -> serde_json::Value {
    json!({"layer_id": "L", "object_id": id,
           "data": {"points": [[20.0, y], [120.0, y + 10.0]], "size": 6.0,
                    "color": {"r": 20, "g": 20, "b": 20, "a": 255}}})
}

/// **事务里写失败 ⇒ 已落的原子被整体撤销** ✓，并且**如实报告** ✓。
#[test]
fn a_failed_write_inside_a_transaction_rolls_it_back() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tx", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let before;
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        drop(ctx);
        before = ink(&mut workspace);
    }
    {
        let mut ctx = context(&mut workspace);
        let begun = registry.call(&mut ctx, "begin_transaction", &json!({}));
        assert_eq!(begun["ok"], json!(true), "{begun}");
        assert_eq!(begun["transaction"], json!(true), "{begun}");
        // ① 一步**成功**的写 ✓。
        let drawn = registry.call(&mut ctx, "draw_stroke", &good_stroke("s1", 20.0));
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
        // ② 一步**失败**的写 ✓（空点列 ⇒ `draw_stroke` 拒绝 ✓）。
        let failed = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "bad",
                    "data": {"points": [], "size": 6.0,
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        assert_eq!(failed["ok"], json!(false), "空点列应被拒绝：{failed}");
        // ③ **回滚必须被报告** ✓（否则调用方以为只有这一步失败 ✗）。
        let rolled = failed
            .get("rolled_back")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        assert_eq!(rolled["ok"], json!(true), "应如实报告回滚：{failed}");
        assert_eq!(rolled["reverted"], json!(1), "第一笔应被撤销：{failed}");
    }
    // ④ 画布回到事务之前 ✓，事务也已关闭 ✓（再 commit 应报错 ✓）。
    assert_eq!(ink(&mut workspace), before, "回滚之后画面应回到事务之前");
    let mut ctx = context(&mut workspace);
    let again = registry.call(&mut ctx, "commit_transaction", &json!({}));
    assert_eq!(again["ok"], json!(false), "回滚之后事务应已关闭：{again}");
}

/// **事务正常收尾 ⇒ 原子保留** ✓，且它们同属一个变更集 ⇒ 可作为**一组**撤销 ✓。
#[test]
fn a_committed_transaction_keeps_its_atoms_as_one_changeset() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tx", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let before = ink(&mut workspace);
    let (changeset, after) = {
        let mut ctx = context(&mut workspace);
        let begun = registry.call(&mut ctx, "begin_transaction", &json!({}));
        let changeset = begun["changeset_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for (id, y) in [("s1", 20.0), ("s2", 60.0)] {
            let drawn = registry.call(&mut ctx, "draw_stroke", &good_stroke(id, y));
            assert_eq!(drawn["ok"], json!(true), "{drawn}");
        }
        let committed = registry.call(&mut ctx, "commit_transaction", &json!({}));
        assert_eq!(committed["ok"], json!(true), "{committed}");
        assert_eq!(
            committed["atoms"],
            json!(2),
            "两步都该在事务里：{committed}"
        );
        assert_eq!(committed["transaction"], json!(false), "{committed}");
        drop(ctx);
        (changeset, ink(&mut workspace))
    };
    assert!(after > before, "收尾之后画面应保留（{before} → {after}）");
    // **作为一个变更集整体撤销** ✓（这正是"事务同时是变更集"带来的好处 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let undone = registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": changeset}),
        );
        assert_eq!(undone["ok"], json!(true), "{undone}");
        assert_eq!(undone["reverted"], json!(2), "两笔都该被撤销：{undone}");
    }
    assert_eq!(ink(&mut workspace), before, "整体撤销之后应回到事务之前");
}

/// **读操作失败绝不回滚** ✓ —— 这条与上一条同等重要 ✗。
#[test]
fn a_failed_read_does_not_roll_back() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tx", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let painted = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "begin_transaction", &json!({}));
        let drawn = registry.call(&mut ctx, "draw_stroke", &good_stroke("s1", 20.0));
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
        // 一次**失败的读** ✓（对象不存在 ⇒ `get_object_history` 报 reference_not_found ✓，它是只读工具 ✓）。
        let failed = registry.call(
            &mut ctx,
            "get_object_history",
            &json!({"object_id": "nope"}),
        );
        assert_eq!(failed["ok"], json!(false), "不存在的对象应报错：{failed}");
        assert!(
            failed.get("rolled_back").is_none(),
            "读失败**不该**触发回滚：{failed}"
        );
        // 事务仍然开着 ✓ ⇒ 还能继续写、还能正常收尾 ✓。
        let more = registry.call(&mut ctx, "draw_stroke", &good_stroke("s2", 60.0));
        assert_eq!(more["ok"], json!(true), "事务应仍然开着：{more}");
        let committed = registry.call(&mut ctx, "commit_transaction", &json!({}));
        assert_eq!(committed["ok"], json!(true), "{committed}");
        assert_eq!(
            committed["atoms"],
            json!(2),
            "两笔都应在事务里：{committed}"
        );
        drop(ctx);
        ink(&mut workspace)
    };
    assert!(painted > 0, "读失败不该影响已经画下的内容（墨 {painted}）");
}

/// **误用报错** ✓：没 begin 就 commit ✓、连续两次 begin ✓（变更集与事务**互相**占用 ✓）。
#[test]
fn misuse_is_refused_with_a_reason() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tx", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        let refused = registry.call(&mut ctx, "commit_transaction", &json!({}));
        assert_eq!(refused["ok"], json!(false), "{refused}");
        assert!(
            refused["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("begin_transaction"),
            "错误应提示先 begin：{refused}"
        );
        let begun = registry.call(&mut ctx, "begin_transaction", &json!({}));
        assert_eq!(begun["ok"], json!(true), "{begun}");
        // 事务开着时再开**变更集**也应报错 ✓（它们共用同一个"打开"的槽位 ✓）。
        let changeset = registry.call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(
            changeset["ok"],
            json!(false),
            "已有事务时不该再开变更集：{changeset}"
        );
        let again = registry.call(&mut ctx, "begin_transaction", &json!({}));
        assert_eq!(again["ok"], json!(false), "重复 begin 应报错：{again}");
        registry.call(&mut ctx, "commit_transaction", &json!({}));
    }
}
