//! **`get_atom`** ✓ —— 按 id 读一条原子的**完整记录（含净荷）** ✓。
//!
//! **为什么有这条测试** ✓：全项目只有四个只读工具会读净荷 ✓，而 `get_log` / `find_atom`
//! **按设计只给元数据** ✗ ⇒ 界面上"看得到发生了什么 ✓、问不出这一条改了什么" ✗。
//! 这条测试把"**按 id 能把净荷读回来**"钉死 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_atom", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_atom", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
}

/// **净荷读得回来** ✓（评论的正文就是净荷 ✓ —— 与 `get_log` 只有元数据形成对照 ✓）。
#[test]
fn an_atoms_payload_can_be_read_by_id() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let text = "按 id 读回来的正文";
    {
        let mut ctx = context(&mut workspace);
        let posted = registry().call(&mut ctx, "comment", &json!({ "text": text }));
        assert_eq!(posted["ok"], json!(true), "{posted}");
    }
    // 先从日志元数据里取 id ✓（`get_log` 本来就能给 ✓）⇒ 再用 `get_atom` 取净荷 ✓。
    let atom_id = {
        let mut ctx = context(&mut workspace);
        let log = registry().call(&mut ctx, "get_log", &json!({ "kind": "comment" }));
        let atoms = log["atoms"].as_array().expect("应有 atoms");
        assert_eq!(atoms.len(), 1, "{log}");
        // **元数据里没有净荷** ✓ —— 这正是要补它的理由 ✓。
        assert!(
            atoms[0].get("payload").is_none(),
            "get_log 不应带净荷：{log}"
        );
        atoms[0]["atom_id"]
            .as_str()
            .expect("应有 atom_id")
            .to_owned()
    };
    let detail = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "get_atom", &json!({ "atom_id": atom_id }))
    };
    assert_eq!(detail["ok"], json!(true), "{detail}");
    // **统一的拼法** ✓：与 `get_log` 一致 ✓（`comment` ✓，不是 `Comment` ✗）。
    assert_eq!(detail["kind"], json!("comment"), "{detail}");
    assert_eq!(detail["payload"]["text"], json!(text), "{detail}");
    // 其余身份信息也要在 ✓（界面要用来显示"谁、什么时候" ✓）。
    assert!(detail["actor"].as_str().is_some(), "{detail}");
    assert!(detail["seq"].is_number(), "{detail}");
    assert!(detail["timestamp"].is_number(), "{detail}");
}

/// **不存在的 id 要明确报错** ✗（不是返回空记录 ✗）。
#[test]
fn an_unknown_atom_id_is_refused() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let missing = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "get_atom",
            &json!({ "atom_id": "01ZZZZZZZZZZZZZZZZZZZZZZZZ" }),
        )
    };
    assert_eq!(missing["ok"], json!(false), "{missing}");
    let detail = missing["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("不在日志里"), "原因应说明找不到：{detail}");
}
