//! `begin_changeset` / `commit_changeset` / `abort_changeset`（设计 793 的 changeset 组 ✓）。
//!
//! **设计只给了名字 ⇒ 记录选择** ✓：
//! * **变更集 id 由服务端生成** ✓（让调用方指定需要一套校验与冲突规则 ✓，设计没说 ✗ ⇒ 不擅自发明 ✗）；
//! * **重复 `begin` ⇒ 报错** ✓：静默复用会让调用方以为"新开了一个" ✓、
//!   静默新建会让前一个永远挂在打开状态 ✗ ⇒ 报错最诚实 ✓；
//! * **`abort` 是"撤销"而不是"删除"** ✓：本项目的日志是**追加式**的 ✓（删除不是一种操作 ✓），
//!   撤销既保留历史 ✓、又让"放弃"这件事本身**可再撤销** ✓；
//! * **打开状态按 (文档 + 会话) 存** ✓（眼下 HTTP 会话固定 ✓ ⇒ 等价于按文档 ✓；
//!   将来真按连接区分会话时 ✓ 这里不用改 ✓）。
//!
//! 断言的落点 ✓：**"并入"看 `get_changesets` 的原子数** ✓、**"放弃"看画面回到原样** ✓ ——
//! 都是用户/调用方**看得见**的量 ✓，不是内部字段 ✓。

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
    ToolContext::new(workspace, "doc_cb", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_cb", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 235 || pixel[1] < 235 || pixel[2] < 235))
        .count()
}

fn draw(workspace: &mut Workspace, registry: &ToolRegistry, id: &str, y: f64) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": id,
                "data": {"points": [[20.0, y], [120.0, y + 10.0]], "size": 6.0,
                         "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    )
}

fn setup(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
}

/// **`begin` 之后的多步操作并入同一个变更集；`commit` 之后不再并入** ✓。
#[test]
fn begin_gathers_following_commits_until_commit_closes_it() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cb", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    let changeset = {
        let mut ctx = context(&mut workspace);
        let begun = registry.call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(begun["ok"], json!(true), "{begun}");
        assert_eq!(begun["open"], json!(true), "{begun}");
        begun["changeset_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    assert!(!changeset.is_empty(), "应返回服务端生成的变更集 id");
    // 两步操作 ✓（它们自己并不知道变更集的存在 ✓ —— 这正是"收口点生效"的证据 ✓）。
    for (id, y) in [("s1", 20.0), ("s2", 60.0)] {
        let drawn = draw(&mut workspace, &registry, id, y);
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_changesets", &json!({"limit": 10}))
    };
    let entry = listed["changesets"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["changeset_id"] == json!(changeset))
        })
        .cloned()
        .unwrap_or_else(|| panic!("变更集应出现在列表里：{listed}"));
    assert_eq!(
        entry["atoms"],
        json!(2),
        "两笔都应并入同一个变更集：{entry}"
    );
    // `commit` ⇒ 收尾 ✓。
    let committed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "commit_changeset", &json!({}))
    };
    assert_eq!(committed["ok"], json!(true), "{committed}");
    assert_eq!(committed["atoms"], json!(2), "{committed}");
    assert_eq!(committed["open"], json!(false), "{committed}");
    // **收尾之后的那一笔不再并入** ✓（它没有 changeset_id ✓）。
    let drawn = draw(&mut workspace, &registry, "s3", 90.0);
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let state = workspace.document_mut("doc_cb").unwrap().state().clone();
    let outsider = state
        .objects
        .values()
        .find(|object| object.id == "s3")
        .and_then(|object| object.versions.first())
        .cloned()
        .unwrap_or_default();
    let document = workspace.document("doc_cb").unwrap();
    let atom = document
        .log()
        .iter()
        .find(|atom| atom.id == outsider)
        .expect("应能找到那一笔的原子");
    assert_eq!(atom.changeset_id, None, "收尾之后的提交不该再有变更集");
}

/// **`abort` 让画面回到 `begin` 之前** ✓（撤销而不是删除 ✓），并关闭变更集 ✓。
#[test]
fn abort_takes_the_picture_back_to_before_begin() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cb", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    let before = ink(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "begin_changeset", &json!({}));
    }
    for (id, y) in [("s1", 20.0), ("s2", 60.0), ("s3", 90.0)] {
        draw(&mut workspace, &registry, id, y);
    }
    let painted = ink(&mut workspace);
    assert!(painted > before, "三笔之后应当有墨（{before} → {painted}）");
    let aborted = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "abort_changeset", &json!({}))
    };
    assert_eq!(aborted["ok"], json!(true), "{aborted}");
    assert_eq!(aborted["reverted"], json!(3), "三笔都应被撤销：{aborted}");
    assert!(
        !aborted["revert_changeset_id"]
            .as_str()
            .unwrap_or_default()
            .is_empty(),
        "撤销本身也归一个变更集（可再撤销 ✓）：{aborted}"
    );
    assert_eq!(ink(&mut workspace), before, "放弃之后画面应回到 begin 之前");
    // 关闭之后再用 ⇒ 明确报错 ✓（不静默 ✓）。
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "abort_changeset", &json!({}))
    };
    assert_eq!(again["ok"], json!(false), "{again}");
    assert_eq!(
        again["error_code"].as_str().unwrap_or_default(),
        "precondition_failed",
        "没有打开的变更集时应给出 precondition_failed：{again}"
    );
}

/// **误用必须报错** ✓：没 `begin` 就 `commit`/`abort` ✓、连续两次 `begin` ✓。
#[test]
fn misuse_is_refused_with_a_reason() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cb", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        for op in ["commit_changeset", "abort_changeset"] {
            let refused = registry.call(&mut ctx, op, &json!({}));
            assert_eq!(
                refused["ok"],
                json!(false),
                "{op} 在没有 begin 时应被拒绝：{refused}"
            );
            assert!(
                refused["context"]["detail"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("begin_changeset"),
                "{op} 的错误应提示先 begin：{refused}"
            );
        }
        let first = registry.call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(first["ok"], json!(true), "{first}");
        let second = registry.call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(second["ok"], json!(false), "重复 begin 应报错：{second}");
        assert!(
            second["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("已经有一个打开的变更集"),
            "错误应说清原因：{second}"
        );
        // 收尾之后可以再开 ✓。
        registry.call(&mut ctx, "commit_changeset", &json!({}));
        let third = registry.call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(third["ok"], json!(true), "收尾之后应能再开一个：{third}");
        registry.call(&mut ctx, "abort_changeset", &json!({}));
    }
}
