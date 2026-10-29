//! **A 期（paint-r2-A）**：batch 水位环境变量覆盖 + 触发回执报两个上限 ✓。
//!
//! **⇒ 断什么** ✓：
//!   ① 缺省：不设 `YANSHI_BATCH_RSS_LIMIT_MB` 时，正常 batch 回执的
//!     `memory_limits.batch_rss_limit_mb == 4096`（旧硬编码值，行为不变 ✓）；
//!   ② 覆盖：设 `YANSHI_BATCH_RSS_LIMIT_MB=1` 时，同一批在第 0 段前即触发
//!     （真实进程 RSS 远大于 1MB），`stopped_early == true`、
//!     `completed_calls == 0`，且触发回执的 `memory_limits` 同时报
//!     `batch_rss_limit_mb == 1` 与当前的文档预算值；
//!   ③ 文档预算本身不动：`doc_memory_budget_mb` 仍由
//!     `YANSHI_DOC_MEMORY_BUDGET_MB`（缺省 2048）决定，与 batch 水位无关 ✓。
//!
//! **⇒ 为什么只有一个测试函数** ✓：环境变量是进程全局的；多函数并行跑会互相干扰
//!   （`doc_budget.rs` 同款做法）。本文件内两段顺序跑、最后恢复环境，
//!   且本文件是独立测试二进制，不影响其它测试进程 ✓。
//!
//! **⇒ 变异** ✓：把缺省改大 ⇒ ① 红；把回执字段删掉 ⇒ ①② 红；
//!   把谓词改回硬编码 4096 ⇒ ② 的 `stopped_early` 断言红（1MB 覆盖失效 ⇒ 批跑完）✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn make_workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_wm_env", 160, 120),
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
    let mut ctx = ToolContext::new(workspace, "doc_wm_env", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "batch", &json!({ "calls": calls }))
}

/// 当前文档预算（MB）：与服务端 `memory_budget_bytes` 同口径（缺省 2048），
///
/// 只用于算期望值，不参与实现 ✓。
fn expected_doc_budget_mb() -> u64 {
    std::env::var("YANSHI_DOC_MEMORY_BUDGET_MB")
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .filter(|mb| *mb >= 1)
        .unwrap_or(2_048)
}

#[test]
fn batch_watermark_env_override_and_trigger_receipt_limits() {
    // ① 缺省：水位 4096，正常跑完，回执含两个上限。
    std::env::remove_var("YANSHI_BATCH_RSS_LIMIT_MB");
    let mut workspace = make_workspace();
    let made = run_batch(&mut workspace, 5);
    assert_eq!(made["ok"], json!(true), "缺省水位下小批应当跑完：{made}");
    assert_eq!(
        made["memory_limits"]["batch_rss_limit_mb"],
        json!(4096),
        "缺省 batch 水位必须是 4096（旧硬编码值）：{made}"
    );
    assert_eq!(
        made["memory_limits"]["doc_memory_budget_mb"],
        json!(expected_doc_budget_mb()),
        "文档预算应当是 YANSHI_DOC_MEMORY_BUDGET_MB 的当前值（缺省 2048），与 batch 水位无关：{made}"
    );

    // ② 覆盖：水位 1MB 时同一批在第 0 段前触发，触发回执同时报两个上限。
    std::env::set_var("YANSHI_BATCH_RSS_LIMIT_MB", "1");
    let mut workspace = make_workspace();
    let made = run_batch(&mut workspace, 5);
    assert_eq!(
        made["stopped_early"],
        json!(true),
        "水位覆盖到 1MB 时应当触发提前停：{made}"
    );
    assert_eq!(
        made["completed_calls"],
        json!(0),
        "第 0 段前触发时已提交 0 笔（与第 2 轮油画 12 批 0 笔提交同形）：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("resource_exhausted"),
        "触发时错误码应当是 resource_exhausted：{made}"
    );
    assert_eq!(
        made["memory_limits"]["batch_rss_limit_mb"],
        json!(1),
        "触发回执应当报覆盖后的水位 1，而不是缺省 4096：{made}"
    );
    assert_eq!(
        made["memory_limits"]["doc_memory_budget_mb"],
        json!(expected_doc_budget_mb()),
        "触发回执应当同时报文档预算当前值：{made}"
    );

    // 恢复环境，不污染同一进程内后续测试 ✓。
    std::env::remove_var("YANSHI_BATCH_RSS_LIMIT_MB");
}
