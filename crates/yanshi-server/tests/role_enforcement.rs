//! **角色强制必须落在工具层** ✓（真实漏洞 ✓，本轮抓到并修好 ✓）。
//!
//! **漏洞原貌** ✓：检查原来做在 **HTTP 入口** ✓ ⇒ 堵住了 `/api/tools/*` ✓，
//! 而 **WebSocket** 走另一条路 ✓、且**无条件** `with_owner(true)` ✗
//! ⇒ **viewer 令牌可以经 WS 改文档** ✗。**实测复现** ✓（同一服务器、同一令牌、同一工具 ✓）：
//! ```text
//! HTTP      ⇒ {"ok":false,"error_code":"permission_denied"}   ✓
//! WebSocket ⇒ {"ok":true,"layer_id":"L_viewer"}               ✗ 竟然成功
//! ```
//! **修法** ✓：把检查**下沉到 `ToolRegistry::call`** ✓ —— 那是**所有入口的必经之路** ✓
//!（HTTP / WS / MCP / batch 嵌套 ✓）⇒ **一处生效、全部受益** ✓。
//! **本测试就在那一层验** ✓：构一个 viewer 上下文 ⇒ 改文档必须被拒 ✓；editor 上下文 ⇒ 必须能改 ✓。

use serde_json::json;
use yanshi_server::token::Role;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_role", 120, 90),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn context<'a>(workspace: &'a mut Workspace, role: Role) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_role", "human:1", "session:test")
        .with_role(role)
        .with_wait_for_render(true, 4_000)
}

/// **viewer 不得改文档** ✓ —— 这正是目标①的验收条款 ✓，而且**在工具层**成立 ✓（不只 HTTP ✓）。
#[test]
fn a_viewer_cannot_call_a_mutating_tool() {
    let mut workspace = workspace();
    for (tool, args) in [
        ("create_layer", json!({ "layer_id": "L_viewer" })),
        (
            "draw_shape",
            json!({"layer_id": "L", "object_id": "s1",
                   "data": {"geometry": {"kind": "rect", "bbox": {"x": 1, "y": 1, "w": 5, "h": 5}},
                            "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        ),
    ] {
        let refused = {
            let mut ctx = context(&mut workspace, Role::Viewer);
            registry().call(&mut ctx, tool, &args)
        };
        assert_eq!(
            refused["ok"],
            json!(false),
            "viewer 不该能调 {tool}：{refused}"
        );
        assert_eq!(
            refused["error_code"],
            json!("permission_denied"),
            "{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("viewer") || detail.contains("viewer"),
            "要说清角色：{detail}"
        );
    }
    // **且文档真没变** ✓（"报错前已经写进去" ✗ 是最初那个 P0 的味道 ✓）
    let layers = {
        let mut ctx = context(&mut workspace, Role::Viewer);
        registry().call(&mut ctx, "list_layers", &json!({}))
    };
    assert_eq!(layers["ok"], json!(true), "**只读工具不受限** ✓：{layers}");
    let ids: Vec<String> = layers["layers"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|layer| layer["id"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !ids.iter().any(|id| id == "L_viewer"),
        "被拒的图层不该存在：{ids:?}"
    );
}

/// **editor 必须能改** ✓ —— 别把漏洞修成"谁都不能改" ✗。
#[test]
fn an_editor_can_still_edit() {
    let mut workspace = workspace();
    let made = {
        let mut ctx = context(&mut workspace, Role::Editor);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L_editor" }))
    };
    assert_eq!(made["ok"], json!(true), "editor 应当能改文档：{made}");
}

/// **owner 也能改** ✓，且只有 owner 能跨 actor 撤销 ✓（`owner` 与 `role` **各司其职** ✓）。
#[test]
fn an_owner_can_edit_and_revert_others() {
    let mut workspace = workspace();
    let made = {
        let mut ctx = context(&mut workspace, Role::Owner);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L_owner" }))
    };
    assert_eq!(made["ok"], json!(true), "{made}");
    // **owner 标志由调用方显式给** ✓ ⇒ 本地进程（MCP/测试）是 owner ✓，网络入口按角色 ✓。
    assert!(
        Role::Owner.can_edit() && Role::Owner.can_revert_others(),
        "owner 两条都要有 ✓"
    );
    assert!(
        Role::Editor.can_edit() && !Role::Editor.can_revert_others(),
        "editor 只能改、不能撤别人的 ✓"
    );
    assert!(
        !Role::Viewer.can_edit() && !Role::Viewer.can_revert_others(),
        "viewer 两条都不能有 ✓"
    );
}
