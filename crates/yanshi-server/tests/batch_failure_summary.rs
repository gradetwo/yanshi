//! **报告 4 P5 残留极小项**：`batch` 部分失败时顶层给极小失败摘要。
//!
//! **断什么** ✓：
//!   ① 部分失败批 ⇒ 顶层 `failure_summary` 非空：`fail_count` 与顶层
//!      `fail_count` 一致，`first_index` / `first_tool` / `first_error_code` /
//!      `first_error` 与 `failed_calls[0]` 的下标/工具名/错误码/原因概要一致
//!      （只取首条，不复制 `failed_calls` 全量：摘要键集合恰为这 5 个）；
//!   ② 全成批 ⇒ `failure_summary` 为 `null`，不造无意义摘要；
//!   ③ 两批里 `ok` / `all_ok` / `ok_count` / `fail_count` / `failed_calls`
//!      的既有语义一字不动：`ok == all_ok`，`ok_count + fail_count == count`。
//!
//! **变异** ✓：把 `failure_summary` 的取值改成 `failed_calls[1]` ⇒ ① 红
//!   （`first_index` 不再是首条失败下标）；把全成时的 `null` 改成空对象 ⇒ ② 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（与 batch_stroke_evidence 同款：第 817 轮；`/tmp` 是小 tmpfs ✓）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `brush_stroke` 要从那儿读 `.myb` 笔刷 ✓。
fn workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_bfs_{name}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_bfs", 320, 240),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_bfs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn stroke_call(i: u32) -> serde_json::Value {
    let y = 30.0 + (i as f64) * 18.0;
    json!({"tool": "brush_stroke", "arguments": {
        "layer_id": "L", "brush": "100%_Opaque",
        "points": [[40.0, y, 1.0], [280.0, y, 1.0]], "size": 30,
        "color": [0.45, 0.38, 0.30],
    }})
}

#[test]
fn partial_failure_batch_carries_a_minimal_top_level_summary() {
    let mut workspace = workspace("partial");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let mut calls: Vec<_> = (0..10).map(stroke_call).collect();
    // 两笔故意失败（下标 4 与 7），首条失败是下标 4 那笔 ⇒ 摘要必须取首条，不是任一条 ✓。
    calls[4]["arguments"]["brush"] = json!("不存在的笔刷.myb");
    calls[7]["arguments"]["brush"] = json!("也不存在的笔刷.myb");
    let made = call(
        &mut workspace,
        "batch",
        json!({ "calls": calls, "silent": true }),
    );
    // ③ 既有语义不动 ✓。
    assert_eq!(
        made["ok"],
        json!(false),
        "部分失败 ⇒ 顶层 ok 应为 false（A6）：{made}"
    );
    assert_eq!(made["all_ok"], json!(false), "{made}");
    assert_eq!(made["ok"], made["all_ok"], "ok 与 all_ok 必须恒等：{made}");
    assert_eq!(made["count"], json!(10), "{made}");
    assert_eq!(made["ok_count"], json!(8), "{made}");
    assert_eq!(made["fail_count"], json!(2), "{made}");
    assert_eq!(
        made["ok_count"].as_u64().unwrap() + made["fail_count"].as_u64().unwrap(),
        10,
        "ok_count + fail_count == count：{made}"
    );
    let failed = made["failed_calls"]
        .as_array()
        .expect("failed_calls 必须是数组");
    assert_eq!(failed.len(), 2, "{made}");
    // ① 摘要存在且内容与 failed_calls 首条一致 ✓。
    let summary = &made["failure_summary"];
    assert!(
        summary.is_object(),
        "部分失败批顶层应有对象形 failure_summary：{made}"
    );
    assert_eq!(
        summary["fail_count"],
        json!(2),
        "摘要条数应与顶层 fail_count 一致：{made}"
    );
    assert_eq!(
        summary["first_index"], failed[0]["index"],
        "摘要首条下标应取首条失败：{made}"
    );
    assert_eq!(
        summary["first_tool"], failed[0]["name"],
        "摘要首条工具应取首条失败：{made}"
    );
    assert_eq!(
        summary["first_error_code"], failed[0]["error"]["error_code"],
        "摘要首条错误码应取首条失败：{made}"
    );
    assert_eq!(
        summary["first_error_code"],
        json!("reference_not_found"),
        "不存在的笔刷应报 reference_not_found：{made}"
    );
    assert_eq!(
        summary["first_error"], failed[0]["error"]["context"]["detail"],
        "摘要首条原因概要应与首条失败的 detail 一致：{made}"
    );
    assert!(
        summary["first_error"]
            .as_str()
            .unwrap_or("")
            .contains("不存在的笔刷"),
        "原因概要应写明是哪支笔刷失败：{made}"
    );
    // ① 极小：摘要键集合恰为这 5 个，不复制 failed_calls 全量 ✓。
    let mut keys: Vec<&str> = summary
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "fail_count",
            "first_error",
            "first_error_code",
            "first_index",
            "first_tool"
        ],
        "failure_summary 只能有 5 个键（失败条数 + 首条四项）：{made}"
    );
}

#[test]
fn all_success_batch_has_a_clean_null_summary() {
    let mut workspace = workspace("allok");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let calls: Vec<_> = (0..3).map(stroke_call).collect();
    let made = call(
        &mut workspace,
        "batch",
        json!({ "calls": calls, "silent": true }),
    );
    // ③ 既有语义不动 ✓。
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["all_ok"], json!(true), "{made}");
    assert_eq!(made["ok"], made["all_ok"], "ok 与 all_ok 必须恒等：{made}");
    assert_eq!(made["count"], json!(3), "{made}");
    assert_eq!(made["ok_count"], json!(3), "{made}");
    assert_eq!(made["fail_count"], json!(0), "{made}");
    assert_eq!(
        made["failed_calls"],
        json!([]),
        "全成批 failed_calls 应为空数组：{made}"
    );
    // ② 全成批摘要为空值语义干净 ✓（null，与同回执 warnings 的空值语义一致）。
    assert_eq!(
        made["failure_summary"],
        json!(null),
        "全成批 failure_summary 应为 null，不造无意义摘要：{made}"
    );
}
