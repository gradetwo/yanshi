//! **报告 5 第 8 项**：batch 回执带实时信号 `server_rss_mb` 与 `memory_pressure`（事前预警）。
//!
//! **⇒ 断什么** ✓：
//!   ① 正常批：回执含两新字段；`server_rss_mb` 是正整数，且与同一进程内
//!     `describe_canvas` 的 `server_rss_mb` 同量级（同一测量口径 `proc_rss_mb`，
//!     两次采样间 RSS 只会漂移几 MB，容差 256MB —— 量级错（KB 当 MB 之类）必超容差 ✓）；
//!     `memory_pressure` 等于按“RSS / 当前限额 4096”分档算出的档位；
//!     旧字段 `memory_limits` 语义不动（仍是 4096 / 2048）；
//!   ② 失败批（1 成 1 败）：顶层 `ok:false`、`fail_count==1`，两新字段照常在，
//!     不因此项崩坏（新字段只加在回执组装处，不碰失败留痕路径 ✓）；
//!   ③ 提前停批（`YANSHI_BATCH_RSS_LIMIT_MB=1`）：`stopped_early==true`，
//!     两新字段照常在，且档位必为 `critical`（真实 RSS 远大于 1MB ✓）；
//!   ④ 空批：仍是原硬拒（`invalid_argument`、“至少需要一个调用”），
//!     不因此项多出字段或改错（硬拒在回执组装前返回，路径不动 ✓）。
//!
//! **⇒ 为什么只有一个测试函数** ✓：③ 改进程全局环境变量，与
//!   `batch_watermark_env.rs` 同款做法 —— 本文件内顺序跑、最后恢复环境，
//!   且本文件是独立测试二进制，不影响其它测试进程 ✓。
//!
//! **⇒ 变异** ✓：删正常路的挂字段调用 ⇒ ① 红；把档位阈值改动 ⇒ ① 的档位恒等式红；
//!   把挂字段改成读 `MemAvailable` ⇒ ① 的同口径容差红（可用内存与 RSS 量级不同 ✓）；
//!   删提前停路的挂字段调用 ⇒ ③ 红 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn make_workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_batch_rss", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// **档位恒等式**（与实现同一阈值定义：<50% low、<75% moderate、<100% high、≥100% critical）——
/// 断的是“回执档位与回执 RSS + 回执限额三者自洽”，不是单点抄值 ✓。
fn expected_level(rss_mb: u64, limit_mb: u64) -> &'static str {
    let ratio_x100 = rss_mb as u128 * 100 / limit_mb as u128;
    if ratio_x100 >= 100 {
        "critical"
    } else if ratio_x100 >= 75 {
        "high"
    } else if ratio_x100 >= 50 {
        "moderate"
    } else {
        "low"
    }
}

fn run_batch(workspace: &mut Workspace, calls: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_batch_rss", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "batch", &json!({ "calls": calls }))
}

fn run_describe(workspace: &mut Workspace) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_batch_rss", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "describe_canvas", &json!({}))
}

/// 两新字段都在、且档位与“RSS / 限额”自洽；返回 RSS 供同口径比对。
fn assert_memory_signal(receipt: &serde_json::Value, limit_mb: u64) -> u64 {
    let rss = receipt["server_rss_mb"]
        .as_u64()
        .unwrap_or_else(|| panic!("batch 回执必须带正整数 server_rss_mb：{receipt}"));
    assert!(rss > 0, "测试进程 RSS 不可能为 0：{receipt}");
    let level = receipt["memory_pressure"]
        .as_str()
        .unwrap_or_else(|| panic!("batch 回执必须带字符串 memory_pressure：{receipt}"));
    assert!(
        ["low", "moderate", "high", "critical", "unknown"].contains(&level),
        "档位必须在取值集合内：{receipt}"
    );
    assert_eq!(
        level,
        expected_level(rss, limit_mb),
        "档位必须等于 RSS / 当前限额的分档：{receipt}"
    );
    rss
}

#[test]
fn batch_receipt_carries_live_rss_and_pressure_level() {
    std::env::remove_var("YANSHI_BATCH_RSS_LIMIT_MB");

    // ① 正常批：两新字段 + 同口径 + 旧限额不动。
    let mut workspace = make_workspace();
    let made = run_batch(
        &mut workspace,
        json!([
            {"tool": "list_layers", "arguments": {}},
            {"tool": "list_layers", "arguments": {}},
            {"tool": "list_layers", "arguments": {}},
            {"tool": "list_layers", "arguments": {}},
            {"tool": "list_layers", "arguments": {}},
        ]),
    );
    assert_eq!(made["ok"], json!(true), "小批应当跑完：{made}");
    assert_eq!(
        made["memory_limits"]["batch_rss_limit_mb"],
        json!(4096),
        "旧限额字段语义不动：{made}"
    );
    let batch_rss = assert_memory_signal(&made, 4096);
    let described = run_describe(&mut workspace);
    let describe_rss = described["server_rss_mb"]
        .as_u64()
        .unwrap_or_else(|| panic!("describe_canvas 必须带 server_rss_mb：{described}"));
    let drift = batch_rss.abs_diff(describe_rss);
    assert!(
        drift <= 256,
        "batch 的 server_rss_mb 必须与 describe_canvas 同口径（同进程两次采样，漂移应远小于 256MB）：batch={batch_rss} describe={describe_rss}"
    );

    // ② 失败批：1 成 1 败，顶层 ok:false，信号字段照常在。
    let mut workspace = make_workspace();
    let failed = run_batch(
        &mut workspace,
        json!([
            {"tool": "list_layers", "arguments": {}},
            {"tool": "no_such_tool_xyz", "arguments": {}},
        ]),
    );
    assert_eq!(
        failed["ok"],
        json!(false),
        "含失败子调用的批顶层 ok 须为 false：{failed}"
    );
    assert_eq!(failed["fail_count"], json!(1), "失败数须为 1：{failed}");
    assert_eq!(failed["count"], json!(2), "总数须为 2：{failed}");
    assert_memory_signal(&failed, 4096);

    // ③ 提前停批：限额 1MB 下真实 RSS 必超限，档位必为 critical。
    std::env::set_var("YANSHI_BATCH_RSS_LIMIT_MB", "1");
    let mut workspace = make_workspace();
    let stopped = run_batch(
        &mut workspace,
        json!([
            {"tool": "list_layers", "arguments": {}},
            {"tool": "list_layers", "arguments": {}},
        ]),
    );
    assert_eq!(
        stopped["stopped_early"],
        json!(true),
        "1MB 限额下批应当提前停：{stopped}"
    );
    assert_eq!(
        stopped["memory_limits"]["batch_rss_limit_mb"],
        json!(1),
        "触发回执报的须是覆盖后的限额：{stopped}"
    );
    let stopped_rss = assert_memory_signal(&stopped, 1);
    assert_eq!(
        stopped["memory_pressure"],
        json!("critical"),
        "真实 RSS {stopped_rss}MB 远大于 1MB 限额，档位必须是 critical：{stopped}"
    );
    std::env::remove_var("YANSHI_BATCH_RSS_LIMIT_MB");

    // ④ 空批：原硬拒不变，不因此项崩坏或改错。
    let mut workspace = make_workspace();
    let empty = run_batch(&mut workspace, json!([]));
    assert_eq!(empty["ok"], json!(false), "空批应当被拒：{empty}");
    assert_eq!(
        empty["error_code"],
        json!("invalid_argument"),
        "空批拒单须是 invalid_argument：{empty}"
    );
    let detail = empty["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("至少需要一个调用"),
        "空批拒单文案须指明至少需要一个调用：{empty}"
    );
}
