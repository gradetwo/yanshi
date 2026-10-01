//! **变更集** ✓（设计 793）—— "**把接下来这一串动作打包 ✓，不满意就整体撤销**" ✓。
//!
//! **为什么有这条测试** ✓：`begin/commit/abort/get_changesets/revert_changeset` 此前在查看器里**零引用** ✗
//! ⇒ 用户拿不到"**成组撤销**"这个能力 ✓。先把行为钉死 ✓，再让它出现在界面上 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_cs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_cs", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
}

fn layer_count(workspace: &mut Workspace) -> usize {
    let mut ctx = context(workspace);
    let listed = registry().call(&mut ctx, "list_layers", &json!({}));
    listed["layers"].as_array().map(Vec::len).unwrap_or(0)
}

/// **提交之后能在列表里看到它，并且带上了里面的原子** ✓。
#[test]
fn a_committed_changeset_is_listed_with_its_atoms() {
    let mut workspace = workspace();
    setup(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let began = registry().call(&mut ctx, "begin_changeset", &json!({}));
        assert_eq!(began["ok"], json!(true), "{began}");
    }
    // 变更集里做两步 ✓
    for layer in ["L_one", "L_two"] {
        let mut ctx = context(&mut workspace);
        let made = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": layer }));
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    {
        let mut ctx = context(&mut workspace);
        let done = registry().call(&mut ctx, "commit_changeset", &json!({}));
        assert_eq!(done["ok"], json!(true), "{done}");
    }
    let listed = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "get_changesets", &json!({}))
    };
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let changesets = listed["changesets"].as_array().expect("应返回 changesets");
    assert_eq!(changesets.len(), 1, "应当只有刚提交的一个：{listed}");
    // **变更集里应当包含那两条 create_layer** ✓（外加 begin 自己 ✓）。
    let kinds = changesets[0]["kinds"].as_array().expect("应有 kinds");
    let create_layers = kinds
        .iter()
        .filter(|k| k.as_str() == Some("create_layer"))
        .count();
    assert_eq!(create_layers, 2, "两步 create_layer 都应在其中：{listed}");
    assert!(changesets[0]["changeset_id"].as_str().is_some(), "{listed}");
}

/// **放弃会把变更集里的原子整体撤销** ✓（这就是"成组撤销" ✓）。
#[test]
fn aborting_a_changeset_undoes_everything_inside_it() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let before = layer_count(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "begin_changeset", &json!({}));
    }
    {
        let mut ctx = context(&mut workspace);
        let made = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L_doomed" }));
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    assert_eq!(
        layer_count(&mut workspace),
        before + 1,
        "新建的图层应先在 ✓"
    );
    let aborted = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "abort_changeset", &json!({}))
    };
    assert_eq!(aborted["ok"], json!(true), "{aborted}");
    // **整体撤销之后，那一层应当不在** ✓（"不在"比"数量对了"更强 ✓）。
    let after = {
        let mut ctx = context(&mut workspace);
        let listed = registry().call(&mut ctx, "list_layers", &json!({}));
        listed["layers"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|item| item["layer_id"].as_str() == Some("L_doomed"))
                    .count()
            })
            .unwrap_or(0)
    };
    assert_eq!(after, 0, "放弃后 L_doomed 不应还在 ✓");
}
