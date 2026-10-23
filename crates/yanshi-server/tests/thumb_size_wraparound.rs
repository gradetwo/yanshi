//! **`thumb_size` 超过 `u32` 必须报错，不许回绕成 0**（第 738 轮；外部审计 S-12）
//!
//! **⇒ 缺陷**：`optional_count(...) as u32` ✗ ⇒ `4294967296`（`1<<32`）回绕成 **0** ✓
//!   ⇒ 调用方**要**缩略图，却拿到「不要缩略图」的语义 ✓（静默降级 ＋ 数值回绕 ✓）。
//!
//! **⇒ 判据**：`thumb_size: 1<<32` ⇒ **`invalid_argument`** 且文案点出上限 ✓；
//!   **对照**：合法的 `thumb_size: 64` 不许被拦（防修过头 ✓）。
//! **变异**：改回 `as u32` ⇒ 第一条红（会成功且缩略图字段变成 0 ✓）。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_thumb", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn a_thumb_size_beyond_u32_is_refused_instead_of_wrapping_to_zero() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_thumb", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);

    let huge = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"thumb_size": 4_294_967_296_u64, "include_non_stroke": true}),
    );
    assert_eq!(
        huge["ok"],
        json!(false),
        "thumb_size = 1<<32 被接受了 ⇒ 回绕成 0（要缩略图却拿到「不要」）：{huge}"
    );
    assert_eq!(huge["error_code"], json!("invalid_argument"), "{huge}");
    assert!(
        huge.to_string().contains("thumb_size"),
        "文案要点出是哪个参数：{huge}"
    );

    // 合法值不许被拦 ✓
    let fine = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"thumb_size": 64, "include_non_stroke": true}),
    );
    assert_eq!(
        fine["ok"],
        json!(true),
        "合法的 thumb_size 必须照常：{fine}"
    );
}
