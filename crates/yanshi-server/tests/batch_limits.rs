//! **P2-7**：batch 上限与分段 ✓。
//!
//! **⇒ 断什么** ✓：
//!   ① 501 个调用 ⇒ 硬拒（`invalid_argument`，文案指路"拆成多个 batch" ✓）；
//!   ② 500 个调用 ⇒ 通过，且 `segments_completed == 5`（500/100 ✓）、`stopped_early == false` ✓；
//!   ③ 250 个调用 ⇒ `segments_completed == 2`（完整跑完的段数，向下取整 ✓）。
//!   水位提前停的那条路（`stopped_early: true` 的响应形状）在这里**测不到**
//!   （真实阈值 RSS > 4 GiB / MemAvailable < 512MB 在测试机上触发不了 ✓）⇒
//!   谓词本身由 `tools_tests.rs` 的单测守着，真实阈值靠大文档对照实测 ✓。
//!
//! **⇒ 变异** ✓：把 `BATCH_MAX_CALLS` 改大 ⇒ ① 红；把 `BATCH_SEGMENT_CALLS` 改成 50 ⇒ ② 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_batch", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn run_batch(workspace: &mut Workspace, count: usize) -> serde_json::Value {
    let calls: Vec<serde_json::Value> = (0..count)
        .map(|_| json!({"tool": "list_layers", "arguments": {}}))
        .collect();
    let mut ctx = ToolContext::new(workspace, "doc_batch", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "batch", &json!({ "calls": calls }))
}

#[test]
fn batch_rejects_more_than_500_calls() {
    let mut workspace = workspace();
    let made = run_batch(&mut workspace, 501);
    assert_eq!(made["ok"], json!(false), "501 个调用应当被硬拒：{made}");
    assert_eq!(
        made["error_code"],
        json!("invalid_argument"),
        "超限应当报 invalid_argument：{made}"
    );
    let detail = made["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("500") && detail.contains("拆成多个 batch"),
        "超限文案应当给出上限并指路拆分：{made}"
    );
}

#[test]
fn batch_accepts_500_calls_and_reports_segments() {
    let mut workspace = workspace();
    let made = run_batch(&mut workspace, 500);
    assert_eq!(made["ok"], json!(true), "500 个调用应当通过：{made}");
    assert_eq!(
        made["segments_completed"],
        json!(5),
        "500 个调用 = 5 个完整段：{made}"
    );
    assert_eq!(
        made["stopped_early"],
        json!(false),
        "正常跑完时 stopped_early 恒 false：{made}"
    );
    assert_eq!(made["count"], json!(500), "子结果数应当 == 调用数：{made}");
}

#[test]
fn batch_segments_completed_counts_only_full_segments() {
    let mut workspace = workspace();
    let made = run_batch(&mut workspace, 250);
    assert_eq!(made["ok"], json!(true), "250 个调用应当通过：{made}");
    assert_eq!(
        made["segments_completed"],
        json!(2),
        "250 个调用 = 2 个完整段 + 50 个在第三段里：{made}"
    );
}
