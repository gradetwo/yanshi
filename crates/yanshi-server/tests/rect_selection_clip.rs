//! **未知的选区形状必须当场报错，而不是"创建成功、之后没人认领"**（第 778 轮；外部报告）
//!
//! **⇒ 外部现象**：同一个 `selection_id`，`list_selections` 说**有** ✓，而 `clip_to_selection`
//! 报「找不到可裁剪的选区 ⇒ 现有：（一个都没有）」✗ ⇒ 看起来像调用方传错 id ✓。
//! **⇒ 我按他们的建议二选一："让 rect/ellipse 直接报错 或 让它真的能用"** ⇒
//!   源码里 `tools_brush.rs:417-440` **已经**为 rect（`w`/`h`）与 ellipse（`cx/cy/rx/ry`）算 bbox ✓
//!   （第 901／911 轮）⇒ 所以**不能**把它关掉 ✓（那会砍掉一个能用的功能 ✓）。
//! **⇒ 但同类坑真的还存在**：`create_selection` 收**未知 `kind`**（如 `spiral`）时**照样 `ok=true`** ✓
//!   ⇒ 之后**没有任何工具认领它** ✓ ⇒ 正是外部说的「创建成功 ⇒ 之后找不到」✓。
//!
//! **⇒ 为什么这条判据不是"验 rect"** ✗：`brush_stroke` 要真笔刷资源 ✓，而这个测试环境里
//!   `list_assets{kind:brush}` 是 **0 支** ✓ ⇒ 跑不了 ✓（如实说明 ✓，不假装验过 ✓）。
//!
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **不可用的形状必须当场报错，而不是"创建成功、之后找不到"**（第 778 轮；外部建议的后半句）
///
/// **⇒ 外部原话**：「在 `create_selection` 遇到 `rect`/`ellipse` 时直接报错（而不是"创建成功"）」——
/// 这条建议的前提是 rect 真的不可用 ✓。**实测（上一条判据）它在当前 tip 上可用** ✓
/// ⇒ 所以**不该**把它改成报错 ✓（那会把一个能用的功能关掉 ✓）。
/// **⇒ 但有一个形状真的不可用**：未知 `kind` ✓ ⇒ 它**必须当场报错** ✓，
///   否则就是"建成功、之后无人认领"的同一类坑 ✓。本判据钉住这一点 ✓。
#[test]
fn an_unknown_selection_shape_is_refused_at_creation() {
    let root = std::env::temp_dir().join(format!("yanshi_rectsel_bad_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace =
        Workspace::with_file_store(root, DocumentSettings::default()).expect("落盘工作区");
    workspace
        .create_document(NewDocument::new("doc_sel2", 64, 64), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_sel2", "human:1", "s").with_owner(true);
    let created = registry.call(
        &mut ctx,
        "create_selection",
        &json!({
            "selection_id": "sel_bad",
            "shape": {"kind": "spiral", "x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0},
        }),
    );
    assert_eq!(
        created["ok"],
        json!(false),
        "未知形状不许『创建成功』（之后必然没人认领）：{created}"
    );
}
