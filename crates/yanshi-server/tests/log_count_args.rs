//! **`get_log` 的计数参数：负数要响亮、整数浮点要接受**（第 716 轮；外部审计 S-01）
//!
//! **⇒ 缺陷**：`validate_args` 对 `ParamKind::Integer` 放行 `-1` 与 `5.0` ✓，
//!   而取值用 `optional_u64`（＝`Value::as_u64`）对这两类**一律返回 `None`** ✗
//!   ⇒ `.unwrap_or(缺省)` **静默接管**：`since_seq: -1` 被当成 `0`（**返回全量、回执一切正常** ✓）、
//!   `limit: -5` 被当成 `200` ✓ —— 本仓「静默降级形态 4」✓。
//!
//! **⇒ 判据**：
//! ① `since_seq: -1` ⇒ **必须 `invalid_argument`**（不许静默当 0）✓
//! ② `limit: 5.0` ⇒ **接受**（整数浮点就是 5 ✓；原来会被当成 200 ✗）
//! ③ 不传 ⇒ 缺省行为不变（`ok: true`）✓
//!
//! **变异**：把取值改回 `optional_u64(...).unwrap_or(…)` ⇒ ① 红（它会给 `ok: true`）✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_log", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_log", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    for index in 0..3 {
        let registry = registry();
        let mut ctx = context(workspace);
        let response = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "layer_default", "object_id": format!("s{index}"),
                    "data": {"points": [[4.0, 4.0 + index as f64 * 4.0],
                                        [20.0, 4.0 + index as f64 * 4.0]],
                             "size": 4.0, "hardness": 1.0,
                             "color": {"r": 10, "g": 20, "b": 30, "a": 255}}}),
        );
        assert_eq!(response["ok"], json!(true), "{response}");
    }
}

#[test]
fn a_negative_since_seq_is_rejected_instead_of_silently_meaning_zero() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let registry = registry();
    let mut ctx = context(&mut workspace);
    let response = registry.call(&mut ctx, "get_log", &json!({"since_seq": -1}));
    assert_eq!(
        response["ok"],
        json!(false),
        "since_seq: -1 被接受了 ⇒ 静默当成 0（返回全量而回执正常）：{response}"
    );
    assert_eq!(
        response["error_code"],
        json!("invalid_argument"),
        "错误码必须是 invalid_argument：{response}"
    );
    // ② 整数浮点必须被当成整数 ✓
    let mut ctx = context(&mut workspace);
    let response = registry.call(&mut ctx, "get_log", &json!({"limit": 5.0}));
    assert_eq!(
        response["ok"],
        json!(true),
        "limit: 5.0 应当被接受：{response}"
    );
    // ③ 不传仍然正常 ✓
    let mut ctx = context(&mut workspace);
    let response = registry.call(&mut ctx, "get_log", &json!({}));
    assert_eq!(response["ok"], json!(true), "不传参数应当照常：{response}");
}
