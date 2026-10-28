//! **P-8（paint-fix 期 5）**：`smart_select` 回执的 `effective_delta_e` 与
//! `chain_step_delta_e`。
//!
//! **⇒ 两条行为** ✓（各有判据 ✓）：
//!   ① `effective_delta_e` 必须是 ΔE_max（`wand_thresholds` 的第 1 个元素），
//!      t=0.25 → 0.1456，t=0.45 → 0.3346 —— 旧代码取第 2 个元素 ΔE_in
//!      （0.1165 @ t=0.25）错标成 ΔE_max；
//!   ② `chain_step_delta_e` 必须是 0.5×ΔE_max（链式生长步限），
//!      t=0.25 → 0.0728，t=0.45 → 0.1673。
//!
//! **⇒ 变异** ✓：把 `write_smart_select` 的解构改回 `(_, de_max, _, _)`
//!   ⇒ ① 红（0.1165 ≠ 0.1456）；删 `chain_step_delta_e` 字段 ⇒ ② 红
//!   （字段缺失）；把 `WAND_CHAIN_STEP_RATIO` 改成 0.25 ⇒ ② 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_smart", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_smart", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 在空白文档上调 `smart_select`，返回回执。
fn select_at(workspace: &mut Workspace, tolerance: f64) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(
        &mut ctx,
        "smart_select",
        &json!({
            "layer_id": "layer_default",
            "seed": [10, 10],
            "tolerance": tolerance,
            "refine_edges": false,
        }),
    )
}

/// **① + ② @ t=0.25** ✓：`effective_delta_e` = ΔE_max = 0.1456，
/// `chain_step_delta_e` = 0.5×ΔE_max = 0.0728。
#[test]
fn receipt_delta_e_at_quarter_tolerance() {
    let mut workspace = workspace();
    let made = select_at(&mut workspace, 0.25);
    assert_eq!(made["ok"], json!(true), "smart_select 应当成功：{made}");
    let effective = made["effective_delta_e"]
        .as_f64()
        .expect("回执应有 effective_delta_e");
    assert!(
        (effective - 0.1456).abs() < 1e-4,
        "t=0.25 时 effective_delta_e 应为 ΔE_max=0.1456（不是 ΔE_in=0.1165），实测 {effective}"
    );
    let step = made["chain_step_delta_e"]
        .as_f64()
        .expect("回执应有 chain_step_delta_e");
    assert!(
        (step - 0.0728).abs() < 1e-4,
        "t=0.25 时 chain_step_delta_e 应为 0.5×ΔE_max=0.0728，实测 {step}"
    );
}

/// **① + ② @ t=0.45** ✓：`effective_delta_e` = ΔE_max = 0.3346，
/// `chain_step_delta_e` = 0.5×ΔE_max = 0.1673。
#[test]
fn receipt_delta_e_at_high_tolerance() {
    let mut workspace = workspace();
    let made = select_at(&mut workspace, 0.45);
    assert_eq!(made["ok"], json!(true), "smart_select 应当成功：{made}");
    let effective = made["effective_delta_e"]
        .as_f64()
        .expect("回执应有 effective_delta_e");
    assert!(
        (effective - 0.3346).abs() < 1e-4,
        "t=0.45 时 effective_delta_e 应为 ΔE_max=0.3346，实测 {effective}"
    );
    let step = made["chain_step_delta_e"]
        .as_f64()
        .expect("回执应有 chain_step_delta_e");
    assert!(
        (step - 0.1673).abs() < 1e-4,
        "t=0.45 时 chain_step_delta_e 应为 0.5×ΔE_max=0.1673，实测 {step}"
    );
}
