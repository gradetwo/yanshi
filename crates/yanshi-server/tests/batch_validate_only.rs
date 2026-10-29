//! **报告 4 小修：batch `validate_only` 干跑模式（opt-in，缺省关）**。
//!
//! **⇒ 断什么** ✓：
//!   ① 干跑 + 含非法子调用 ⇒ 整批 fast-fail（`ok:false`、`validated:false`）、
//!      `executed:false`、回执 `failed_calls` 列出校验失败项，**零原子落下**
//!      （前后 `head_seq` 不变，合法子调用也不执行）；
//!   ② 干跑 + 全合法 ⇒ 校验通过（`ok:true`、`validated:true`）且**不执行**
//!      （`executed:false`、`atom_ids` 空、`head_seq` 不变）；
//!   ③ **缺省行为不变**：同一批混合调用不传 `validate_only` ⇒ 仍是老语义——
//!      逐笔执行、合法笔落下、非法笔进 `failed_calls`、不回滚（`head_seq` 恰好 +2）；
//!   ④ 干跑 + 未知工具 / 嵌套 batch ⇒ 均为校验失败项，零原子落下；
//!   ⑤ 干跑**不绕过 P-10**：子调用 doc_id 与本批不一致 ⇒ 仍是整批硬拒
//!     （`invalid_argument`），零原子落下。
//!
//! **⇒ 变异** ✓：删掉 `write_batch` 里的干跑分支 ⇒ ①②④ 里合法笔会落下
//!   （`head_seq` 变）或非法笔不再进 `failed_calls` ⇒ 红；把干跑分支改成
//!   “预校验后照常执行” ⇒ ② 红（`executed` 仍真或 `head_seq` 变）；删掉
//!   `validate_only` 的注册表参数 ⇒ 入口 `validate_args` 拒掉该参数，
//!   ①②④⑤ 全红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// docV（512×512）+ 一层 `L`。
fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("docV", 512, 512),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = ToolContext::new(&mut workspace, "docV", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let layer = registry().call(
        &mut ctx,
        "create_layer",
        &json!({"layer_id": "L", "name": "ground"}),
    );
    assert_eq!(layer["ok"], json!(true), "建层应成功：{layer}");
    workspace
}

fn head_seq(workspace: &Workspace) -> u64 {
    workspace
        .document("docV")
        .map(|document| document.head_seq())
        .unwrap_or(0)
}

fn stroke() -> serde_json::Value {
    json!({"tool": "draw_stroke", "arguments": {
        "layer_id": "L",
        "data": {"points": [[10.0, 10.0], [20.0, 20.0]], "size": 4.0},
    }})
}

fn run_batch(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "docV", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "batch", &args)
}

#[test]
fn validate_only_rejects_mixed_batch_and_lands_zero_atoms() {
    let mut workspace = workspace();
    let before = head_seq(&workspace);
    let mut bad = stroke();
    bad["arguments"]["bogus_param_xyz"] = json!(1);
    let made = run_batch(
        &mut workspace,
        json!({"calls": [stroke(), bad, stroke()], "validate_only": true}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "干跑遇非法子调用应 fast-fail：{made}"
    );
    assert_eq!(made["validate_only"], json!(true));
    assert_eq!(made["validated"], json!(false));
    assert_eq!(made["executed"], json!(false), "干跑必须不执行：{made}");
    assert_eq!(made["count"], json!(3));
    assert_eq!(made["fail_count"], json!(1));
    assert_eq!(made["ok_count"], json!(2));
    assert_eq!(
        made["ok_count"].as_u64().unwrap() + made["fail_count"].as_u64().unwrap(),
        made["count"].as_u64().unwrap(),
        "恒等式 ok_count + fail_count == count：{made}"
    );
    assert_eq!(made["all_ok"], json!(false));
    assert_eq!(made["atom_ids"], json!([]), "干跑不落原子：{made}");
    assert_eq!(made["changeset_id"], json!(null), "干跑不建变更集：{made}");
    let failed = made["failed_calls"].as_array().unwrap();
    assert_eq!(failed.len(), 1, "恰好一笔校验失败：{made}");
    assert_eq!(failed[0]["index"], json!(1), "失败项应是第 1 笔：{made}");
    assert_eq!(failed[0]["name"], json!("draw_stroke"));
    assert_eq!(
        failed[0]["error"]["error_code"],
        json!("invalid_argument"),
        "失败项应带 invalid_argument：{made}"
    );
    let detail = failed[0]["error"]["context"]["detail"]
        .as_str()
        .unwrap_or("");
    assert!(
        detail.contains("bogus_param_xyz"),
        "失败项文案要点名非法参数：{made}"
    );
    let validated = made["validated_calls"].as_array().unwrap();
    assert_eq!(validated.len(), 3, "逐笔一览应与 calls 等长：{made}");
    assert_eq!(validated[0]["ok"], json!(true));
    assert_eq!(validated[1]["ok"], json!(false));
    assert_eq!(validated[2]["ok"], json!(true));
    assert_eq!(
        head_seq(&workspace),
        before,
        "干跑 fast-fail 后原子数不变（合法笔也不执行）"
    );
}

#[test]
fn validate_only_passes_all_valid_calls_without_executing() {
    let mut workspace = workspace();
    let before = head_seq(&workspace);
    let made = run_batch(
        &mut workspace,
        json!({"calls": [stroke(), stroke()], "validate_only": true}),
    );
    assert_eq!(made["ok"], json!(true), "全合法干跑应报告校验通过：{made}");
    assert_eq!(made["validate_only"], json!(true));
    assert_eq!(made["validated"], json!(true));
    assert_eq!(made["executed"], json!(false), "干跑全过也不执行：{made}");
    assert_eq!(made["count"], json!(2));
    assert_eq!(made["fail_count"], json!(0));
    assert_eq!(made["failed_calls"], json!([]));
    assert_eq!(made["atom_ids"], json!([]));
    assert_eq!(
        head_seq(&workspace),
        before,
        "干跑全过后原子数也不变（没有笔落下）"
    );
}

#[test]
fn default_batch_semantics_unchanged_for_the_same_mixed_calls() {
    let mut workspace = workspace();
    let before = head_seq(&workspace);
    let mut bad = stroke();
    bad["arguments"]["bogus_param_xyz"] = json!(1);
    // 注意：与 ① 同一批调用，唯一差别是不传 validate_only。
    let made = run_batch(&mut workspace, json!({"calls": [stroke(), bad, stroke()]}));
    assert_eq!(
        made["ok"],
        json!(false),
        "缺省下混合批仍应顶层 ok:false（A6 全成语义）：{made}"
    );
    assert!(
        made.get("validate_only").is_none(),
        "缺省回执不应带 validate_only 字段：{made}"
    );
    assert_eq!(made["count"], json!(3));
    assert_eq!(made["fail_count"], json!(1));
    assert_eq!(made["ok_count"], json!(2));
    let failed = made["failed_calls"].as_array().unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["index"], json!(1));
    assert_eq!(
        head_seq(&workspace),
        before + 2,
        "缺省下两笔合法笔仍落下、非法笔不落、不回滚"
    );
}

#[test]
fn validate_only_flags_unknown_tool_and_nested_batch() {
    let mut workspace = workspace();
    let before = head_seq(&workspace);
    let made = run_batch(
        &mut workspace,
        json!({"calls": [
            stroke(),
            {"tool": "no_such_tool_xyz", "arguments": {}},
            {"tool": "batch", "arguments": {"calls": []}},
        ], "validate_only": true}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "未知工具/嵌套 batch 应校验失败：{made}"
    );
    assert_eq!(made["validated"], json!(false));
    assert_eq!(made["executed"], json!(false));
    assert_eq!(made["fail_count"], json!(2));
    let failed = made["failed_calls"].as_array().unwrap();
    assert_eq!(failed[0]["index"], json!(1));
    assert_eq!(failed[0]["name"], json!("no_such_tool_xyz"));
    assert_eq!(failed[1]["index"], json!(2));
    assert_eq!(failed[1]["name"], json!("batch"));
    assert_eq!(head_seq(&workspace), before, "干跑后原子数不变");
}

#[test]
fn validate_only_does_not_bypass_subcall_doc_id_check() {
    let mut workspace = workspace();
    let before = head_seq(&workspace);
    let mut mismatched = stroke();
    mismatched["arguments"]["doc_id"] = json!("other-doc");
    let made = run_batch(
        &mut workspace,
        json!({"doc_id": "docV", "calls": [mismatched], "validate_only": true}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "干跑也不绕过 P-10 doc_id 校验：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("invalid_argument"),
        "P-10 拒绝应报 invalid_argument：{made}"
    );
    assert_eq!(head_seq(&workspace), before, "P-10 拒绝后原子数不变");
}
