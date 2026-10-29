//! **batch 顶层 `wait_for_render`**（反馈报告 2 期 1）。
//!
//! **判据**：批值是不是真的被读、是不是只当缺省、是不是不漏给下一笔。
//! 观测点不用掐表：`brush_stroke` 回执的 `applied.wait_for_render`
//! 就是子调用生效的值（`ctx` 读数）；批回执的 `wait_for_render`
//! 就是批值本身（`null` = 没给）。
//! 另有一条 `preview` 隔离判据：`brush_stroke` 的 `preview: false`
//! 只写 `ctx` 不读回执，不隔离就会漏给下一个带预览回执的子调用。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **跑完即删临时目录**（`brush_stroke.rs` 同款）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("yanshi_batch_wait_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 仓库 `assets/`（`brush_stroke.rs` 同款），建层 L。
fn brush_workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_bw", 400, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let made = call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    assert_eq!(made["ok"], json!(true), "{made}");
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_bw", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn brush_call(brush_args: serde_json::Value) -> serde_json::Value {
    let mut base = json!({
        "layer_id": "L", "brush": "2B_pencil", "size": 24,
        "points": [[40.0, 60.0, 0.4], [200.0, 120.0, 0.9], [360.0, 70.0, 0.4]],
    });
    if let serde_json::Value::Object(map) = brush_args {
        for (key, value) in map {
            base[key] = value;
        }
    }
    base
}

fn run_brush_batch(workspace: &mut Workspace, batch_args: serde_json::Value) -> serde_json::Value {
    let mut args = json!({
        "calls": [
            {"tool": "brush_stroke", "arguments": brush_call(json!({"object_id": "b1"}))},
            {"tool": "brush_stroke", "arguments": brush_call(json!({"object_id": "b2"}))},
        ],
    });
    if let serde_json::Value::Object(map) = batch_args {
        for (key, value) in map {
            args[key] = value;
        }
    }
    call(workspace, "batch", args)
}

fn applied_wait(results: &serde_json::Value) -> Vec<Option<bool>> {
    results["calls"]
        .as_array()
        .map(|calls| {
            calls
                .iter()
                .map(|entry| entry["result"]["applied"]["wait_for_render"].as_bool())
                .collect()
        })
        .unwrap_or_default()
}

/// **批值 `false` ⇒ 未显式指定的子调用都生效 `false`**，批回执原样回显 `false`。
#[test]
fn batch_wait_false_is_the_default_for_subcalls_without_their_own_value() {
    let root = temp_dir("default_false");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let made = run_brush_batch(&mut workspace, json!({ "wait_for_render": false }));
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        made["wait_for_render"],
        json!(false),
        "批回执应当原样回显批值 false：{made}"
    );
    assert_eq!(
        applied_wait(&made),
        vec![Some(false), Some(false)],
        "两个未显式指定的子调用都应当生效批值 false：{made}"
    );
}

/// **不给批值 ⇒ 各子调用按单笔缺省 `true`**，批回执回显 `null`（= 没给）。
#[test]
fn batch_wait_absent_keeps_single_defaults() {
    let root = temp_dir("absent");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let made = run_brush_batch(&mut workspace, json!({}));
    assert_eq!(made["ok"], json!(true), "{made}");
    assert!(
        made["wait_for_render"].is_null(),
        "没给批值 ⇒ 批回显应当是 null（不是偷偷写 true）：{made}"
    );
    assert_eq!(
        applied_wait(&made),
        vec![Some(true), Some(true)],
        "没给批值 ⇒ 子调用应当按单笔缺省 true：{made}"
    );
}

/// **子调用显式值只对自己那一笔生效**，下一笔回到批值（不漏）。
#[test]
fn explicit_subcall_wait_wins_for_that_call_only() {
    let root = temp_dir("explicit_wins");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let args = json!({
        "calls": [
            {"tool": "brush_stroke", "arguments": brush_call(json!({"object_id": "b1", "wait_for_render": true}))},
            {"tool": "brush_stroke", "arguments": brush_call(json!({"object_id": "b2"}))},
        ],
        "wait_for_render": false,
    });
    let made = call(&mut workspace, "batch", args);
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        applied_wait(&made),
        vec![Some(true), Some(false)],
        "显式 true 只对第一笔生效，第二笔应当回到批值 false（显式值漏给下一笔就红）：{made}"
    );
}

/// **批值类型错 ⇒ 类型错误**，不是静默忽略、也不是「未知参数」。
#[test]
fn batch_wait_wrong_type_is_rejected_as_a_type_error() {
    let root = temp_dir("wrong_type");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let made = run_brush_batch(&mut workspace, json!({ "wait_for_render": "yes" }));
    assert_eq!(made["ok"], json!(false), "字符串批值应当被拒绝：{made}");
    assert!(
        format!("{made}").contains("类型应为 boolean"),
        "应当是类型错误（不是未知参数、不是静默忽略）：{made}"
    );
}

/// **`preview: false` 不漏给下一个子调用**：前一笔是 `brush_stroke(preview: false)`，
/// 后一笔是不带 `preview` 的 `draw_shape`（`silent: false` 让预览可见）。
/// 漏了 ⇒ 后一笔的回执没有 `preview`（基线行为，红）；隔离了 ⇒ 有 `preview`（绿）。
#[test]
fn preview_does_not_leak_to_the_next_subcall() {
    let root = temp_dir("preview_leak");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = brush_workspace(&root);
    let made = call(
        &mut workspace,
        "batch",
        json!({
            "silent": false,
            "calls": [
                {"tool": "brush_stroke", "arguments": brush_call(json!({"object_id": "b1", "preview": false}))},
                {"tool": "draw_shape", "arguments": {"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 40, "h": 30}},
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}},
            ],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let shape = &made["calls"][1]["result"];
    assert!(
        shape.get("preview").is_some_and(|value| !value.is_null()),
        "后一笔没给 preview ⇒ 应当照常带预览（前一笔的 preview: false 漏过来就红）：{shape}"
    );
}
