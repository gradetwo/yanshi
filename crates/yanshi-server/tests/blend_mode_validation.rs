//! **`blend_mode` 的值必须校验** ✓（真实用户报告 ✓）。
//!
//! **用户原话** ✓："当向 `update_layer` 传入非法或未实现的混合模式名称（如
//! `patch: {"blend_mode": "invalid_str"}`）时 ✓，API 依然返回 `ok: true`
//! 并把它直接写入不可变原子日志 `atoms.jsonl`，缺乏入参白名单校验" ✓。
//!
//! **为什么这条重要** ✓：渲染层遇到不认识的值会**静默退化为 `normal`** ✓
//! ⇒ 调用方看到成功 ✓、画面**毫无变化** ✗ —— 与最初 `draw_shape` 那个 P0 **同一类病** ✓。
//! **而报告顺带让我抓到一个真 bug** ✓：`difference` **本来就在**受支持清单里 ✓，
//! 但 `from_name` 的匹配漏了它 ✗ ⇒ **写进去等于没写** ✗ ⇒ 本测试把它一起钉住 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_blend", 120, 90),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = ToolContext::new(&mut workspace, "doc_blend", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let made = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    workspace
}

fn set_blend(workspace: &mut Workspace, value: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_blend", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(
        &mut ctx,
        "update_layer",
        &json!({ "layer_id": "L", "patch": { "blend_mode": value } }),
    )
}

/// **非法值被拒，且报错列出可用值** ✓（而不是 `ok: true` 之后静默失效 ✗）。
#[test]
fn an_unknown_blend_mode_is_refused_with_the_available_list() {
    let mut workspace = workspace();
    for bad in [json!("invalid_str"), json!("color_burn"), json!("")] {
        let refused = set_blend(&mut workspace, bad.clone());
        assert_eq!(refused["ok"], json!(false), "{bad} 应当被拒：{refused}");
        assert_eq!(
            refused["error_code"],
            json!("invalid_argument"),
            "{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(detail.contains("normal"), "报错应当列出可用值：{detail}");
        assert!(detail.contains("multiply"), "报错应当列出可用值：{detail}");
    }
    // **而且拒绝之后图层没被动过** ✓（不能"报错前已经写进日志" ✗ —— 那又是最初那个 P0 的味道 ✓）
    let got = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_blend", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "list_layers", &json!({}))
    };
    let mode = got["layers"]
        .as_array()
        .and_then(|layers| layers.first())
        .and_then(|layer| layer["blend_mode"].as_str())
        .unwrap_or("?")
        .to_owned();
    assert_eq!(mode, "normal", "被拒的值不该落进文档（实测 {mode}）");
}

/// **非字符串也被拒** ✓（`{"blend_mode": 42}` 这种写法必须说出来 ✓）。
#[test]
fn a_non_string_blend_mode_is_refused_too() {
    let mut workspace = workspace();
    let refused = set_blend(&mut workspace, json!(42));
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("字符串"), "要说清必须是字符串：{detail}");
}

/// **合法的九种都能设** ✓ —— 尤其 **`difference`** ✓（它在受支持清单里 ✓，但以前**写进去等于没写** ✗）。
#[test]
fn every_supported_blend_mode_can_actually_be_set() {
    let mut workspace = workspace();
    for name in [
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "add",
        "subtract",
        "difference",
    ] {
        let made = set_blend(&mut workspace, json!(name));
        assert_eq!(made["ok"], json!(true), "{name} 应当可设：{made}");
        // **真的落到图层上** ✓（不是"接受但没写" ✗）
        let got = {
            let mut ctx = ToolContext::new(&mut workspace, "doc_blend", "human:1", "session:test")
                .with_owner(true)
                .with_wait_for_render(true, 4_000);
            registry().call(&mut ctx, "list_layers", &json!({}))
        };
        let stored = got["layers"]
            .as_array()
            .and_then(|layers| layers.first())
            .and_then(|layer| layer["blend_mode"].as_str())
            .unwrap_or("?")
            .to_owned();
        assert_eq!(stored, name, "{name} 应当真的写进图层（实测 {stored}）");
    }
}
