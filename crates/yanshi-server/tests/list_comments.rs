//! **`list_comments`** ✓ —— 协作通道的**可读一侧** ✓。
//!
//! **为什么有这条测试** ✓：`comment` 一直**能写** ✓，但此前**没有任何读工具** ✗ ——
//! `get_log` 与 `find_atom` 都只返回**元数据** ✓（不含净荷 ✗）⇒ 评论**写进去读不回来** ✗。
//! 这条测试把"**写进去、读回来、正文一致**"钉死 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_cm", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_cm", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
}

/// **写两条、读回来、正文与顺序都对** ✓（最新的在前 ✓）。
#[test]
fn comments_can_be_read_back_with_their_text() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let texts = ["第一条：肩线压深一点", "第二条：这里的轮廓光可以更亮"];
    for text in texts {
        let mut ctx = context(&mut workspace);
        let posted = registry().call(&mut ctx, "comment", &json!({ "text": text }));
        assert_eq!(posted["ok"], json!(true), "{posted}");
    }
    let listed = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "list_comments", &json!({}))
    };
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let comments = listed["comments"].as_array().expect("应返回 comments 数组");
    assert_eq!(comments.len(), 2, "两条评论都应读回来：{listed}");
    // **最新的在前** ✓。
    assert_eq!(comments[0]["text"], json!(texts[1]), "{listed}");
    assert_eq!(comments[1]["text"], json!(texts[0]), "{listed}");
    assert_eq!(listed["count"], json!(2));
    // `since_seq` 增量 ✓：拿最大的 seq 再查 ⇒ 应为空 ✓。
    let next = listed["next_since"].as_u64().expect("应回报 next_since");
    let incremental = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "list_comments", &json!({ "since_seq": next }))
    };
    assert_eq!(
        incremental["comments"].as_array().map(Vec::len),
        Some(0),
        "{incremental}"
    );
}

/// **按被评论对象过滤** ✓（评论可以挂在某个对象上 ✓）。
#[test]
fn comments_can_be_filtered_by_target_object() {
    let mut workspace = workspace();
    setup(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
    }
    for (text, object) in [("挂在 A 上", "obj_a"), ("挂在 B 上", "obj_b")] {
        let mut ctx = context(&mut workspace);
        let posted = registry().call(
            &mut ctx,
            "comment",
            &json!({ "text": text, "object_id": object }),
        );
        assert_eq!(posted["ok"], json!(true), "{posted}");
    }
    let filtered = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "list_comments", &json!({ "object_id": "obj_a" }))
    };
    let comments = filtered["comments"].as_array().expect("应返回数组");
    assert_eq!(comments.len(), 1, "{filtered}");
    assert_eq!(comments[0]["text"], json!("挂在 A 上"), "{filtered}");
    assert_eq!(comments[0]["object_id"], json!("obj_a"), "{filtered}");
}
