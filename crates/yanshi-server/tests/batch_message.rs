//! **`batch.message` 必须真的写进原子**（第 724 轮；外部审计 S-02 第 ① 组）
//!
//! **⇒ 缺陷**：`message` 在注册表里声明了 ✓，而 handler **从没读过** ✗
//!   ⇒ 调用方写了说明，**原子上什么也没有** ✓ —— 而 `Atom::message` 早就有、
//!   `get_atom`／`get_stroke_log` 也都会回它 ⇒ 「声明了但不生效」= 静默降级 ✓。
//!
//! **⇒ 判据**（读侧用第 723 轮打通的 `get_stroke_log.message` ✓）：
//! ① `batch` 带 `message` 画一笔 ⇒ 日志里那条的 `message` **等于**写入值 ✓；
//! ② 再跑一个**不带** `message` 的批 ⇒ 那条的 `message` 必须是 `null` ✓（不许泄漏 ✓）。
//! **变异**：让 handler 不读 `message` ⇒ ① 红 ✓；忘记还原 `ctx` ⇒ ② 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_bmsg", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// **同一个 `ctx` 连跑两批** ✓（第 724 轮）：这样「忘了还原」才**看得见** ✓ ——
/// 每条调用都新建 `ctx` 的话，泄漏永远观察不到（我第一次就是这么写的 ✗）。
fn draw_in_batch(ctx: &mut ToolContext<'_>, object_id: &str, message: Option<&str>) {
    let registry = registry();
    let mut args = json!({
        "calls": [{
            "tool": "draw_stroke",
            "args": {"layer_id": "layer_default", "object_id": object_id,
                     "data": {"points": [[4.0, 4.0], [20.0, 20.0]], "size": 3.0,
                              "hardness": 1.0, "color": {"r": 10, "g": 20, "b": 30, "a": 255}}}
        }],
        "silent": true,
    });
    if let Some(message) = message {
        args["message"] = json!(message);
    }
    let response = registry.call(ctx, "batch", &args);
    assert_eq!(response["ok"], json!(true), "{response}");
}

fn message_of(ctx: &mut ToolContext<'_>, object_id: &str) -> serde_json::Value {
    let registry = registry();
    let log = registry.call(ctx, "get_stroke_log", &json!({"limit": 20}));
    let entries = log["entries"].as_array().cloned().unwrap_or_default();
    entries
        .iter()
        .find(|entry| entry["object_id"] == json!(object_id))
        .map(|entry| entry["message"].clone())
        .unwrap_or_else(|| panic!("日志里找不到 {object_id}：{log}"))
}

#[test]
fn the_batch_message_reaches_the_strokes_it_draws() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bmsg", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    // ① 带 message ⇒ 那一笔上必须有 ✓
    let mut ctx = context(&mut workspace);
    draw_in_batch(&mut ctx, "s_with", Some("三步收尾：树冠明暗统一"));
    assert_eq!(
        message_of(&mut ctx, "s_with"),
        json!("三步收尾：树冠明暗统一"),
        "batch.message 没写进原子 ⇒ 声明了却没人读"
    );
    // ② 不带 message ⇒ 下一批不许继承上一批的说明 ✓
    draw_in_batch(&mut ctx, "s_without", None);
    assert!(
        message_of(&mut ctx, "s_without").is_null(),
        "不带 message 的那一批却带了说明 ⇒ `ctx` 没还原（上一批的说明泄漏了）"
    );
    // ③ **批外的直接调用不许沾上批的说明** ✓ —— 这才是「还原 `ctx`」的用处 ✓
    //   （新建 ctx 的话泄漏观察不到：`batch` 自己会把槽位置 `None` ✗）
    let registry = registry();
    let direct = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default",
            "object_id": "s_direct",
            "data": {"points": [[4.0, 24.0], [20.0, 24.0]], "size": 3.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(direct["ok"], json!(true), "{direct}");
    assert!(
        message_of(&mut ctx, "s_direct").is_null(),
        "批**之后**的直接调用沾上了批的说明 ⇒ `batch` 忘了还原 `ctx.batch_message`"
    );
}
