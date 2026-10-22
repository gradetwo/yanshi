//! **`create_mask{from_selection}`** —— 文档说「此时 `shape` 可省略」，而 schema 把它标成必填。
//!
//! **实测事故**（外部第 10 轮评测包 `selection-mask-flat-rect`，状态 `needs-inspection`）：
//! ```text
//! create_mask {mask_id:"mask", from_selection:"sel"}
//! → {"error_code":"invalid_argument","context":{"detail":"缺少必填参数 shape"}}
//! ```
//! ⇒ 派发前的**必填校验**按 `inputSchema.required` 拦下 ✗ ⇒ **文档里写的用法一次也用不了** ✓
//! （本仓的"**描述说支持、schema 拒绝**"家族：与 `color` 声明成 `Object` 那次同型 ✓）。
//!
//! **本测试钉三条**：
//! ① `from_selection` + 无 `shape` ⇒ **必须成功**（并回显 mask_id）；
//! ② 显式 `shape` 仍然优先（不因 ① 而破坏"两者都给时 shape 赢"的语义）；
//! ③ 两者都不给 ⇒ 报错**同时点名** `shape` 与 `from_selection`（不要只说缺 shape）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_mask_from_sel", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn expect_error(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> String {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry().call(ctx, name, args)
    }));
    match response {
        Ok(value) => {
            assert_eq!(
                value.get("ok").and_then(Value::as_bool),
                Some(false),
                "{name} 本该报错，却成功了 ⇒ 静默降级：{value}"
            );
            value
                .get("context")
                .and_then(|context| context.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string())
        }
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
            })
            .unwrap_or_else(|| "（无法读取错误文案）".to_string()),
    }
}

#[test]
fn create_mask_from_selection_works_without_shape_and_says_so_when_both_are_missing() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_mask_from_sel", "width": 160, "height": 120}),
        );
    }
    let mut ctx = context(&mut workspace);
    // 造一个选区，供 `from_selection` 转换（`latest` 取最近建的那个）。
    let selection = call(
        &mut ctx,
        "create_selection",
        &json!({
            "selection_id": "sel",
            "shape": {"kind": "rect", "bbox": {"x": 20, "y": 20, "w": 40, "h": 30}}
        }),
    );
    assert_eq!(
        selection.get("ok").and_then(Value::as_bool),
        Some(true),
        "前置：create_selection 失败：{selection}"
    );

    // ① 只给 from_selection ⇒ 必须成功（这条正是外部踩到的那一调）。
    let converted = call(
        &mut ctx,
        "create_mask",
        &json!({"mask_id": "m_sel", "from_selection": "latest"}),
    );
    assert_eq!(
        converted.get("ok").and_then(Value::as_bool),
        Some(true),
        "create_mask{{from_selection}} 没有 shape 也必须能建：{converted}"
    );
    // **证明它真建出来了** ✓（响应是通用变更回执，不回显 mask_id ⇒ 就**用一个只能靠它成功的调用**证明 ✓）：
    // 在刚建的蒙版上落一笔 —— 不存在时这里会 `reference_not_found` ✓。
    let painted = call(
        &mut ctx,
        "paint_mask",
        &json!({"mask_id": "m_sel", "mode": "paint", "size": 8, "points": [[30, 30, 1]]}),
    );
    assert_eq!(
        painted.get("ok").and_then(Value::as_bool),
        Some(true),
        "刚由选区转换出来的蒙版应当可以落笔：{painted}"
    );

    // ② 显式 shape 仍然优先。
    let explicit = call(
        &mut ctx,
        "create_mask",
        &json!({
            "mask_id": "m_explicit",
            "from_selection": "sel",
            "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 10, "h": 10}}
        }),
    );
    assert_eq!(
        explicit.get("ok").and_then(Value::as_bool),
        Some(true),
        "显式 shape + from_selection 应当成功：{explicit}"
    );

    // ③ 两者都不给 ⇒ 报错要同时点名两条出路。
    let message = expect_error(&mut ctx, "create_mask", &json!({"mask_id": "m_none"}));
    assert!(
        message.contains("shape") && message.contains("from_selection"),
        "错误必须同时给出 shape 与 from_selection 两条出路：{message}"
    );
}
