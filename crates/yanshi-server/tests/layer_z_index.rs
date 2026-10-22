//! **`update_layer` 必须能改层级序号**（外部 `~/patch/bug_report.md` 的 BUG-20）。
//!
//! **他们报的**：`update_layer` 的属性白名单里没有 `z_index` ⇒ 传它会落进
//! 「patch 中没有可修改的图层属性」⇒ 层级**只能在 `create_layer` 时定一次**，事后不可改；
//! 而唯一的替代 `reorder_layers` 要求提交**全部存活图层的完整序列** ⇒ 构造错就整体错序。
//!
//! **⇒ 内核早就支持**（`yanshi-core/src/fold.rs` 的 `apply_property_to_layer` 里有
//! `"z_index" => layer.z_index = …`）⇒ 只差工具层放行**一个键** ✓。
//!
//! **判据断结构量**（顺序），不断像素：`list_layers` 的 `layer_id` 序列在改完 `z_index` 后必须变 ✓。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_zindex", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

/// `list_layers` 的图层 id 序列（自底向上）。
fn order(ctx: &mut ToolContext<'_>) -> Vec<String> {
    let listed = call(ctx, "list_layers", &json!({}));
    listed
        .get("layers")
        .and_then(Value::as_array)
        .map(|layers| {
            layers
                .iter()
                .filter_map(|layer| {
                    layer
                        .get("layer_id")
                        .or_else(|| layer.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn update_layer_can_move_a_layer_to_the_top_with_z_index() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_zindex", "width": 64, "height": 64}),
        );
        call(&mut ctx, "create_layer", &json!({"layer_id": "L1"}));
        call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L2", "z_index": 5}),
        );
    }
    let mut ctx = context(&mut workspace);
    let before = order(&mut ctx);
    assert_eq!(
        before,
        vec!["layer_default", "L1", "L2"],
        "前置：L2（z_index=5）应在最上 ⇒ 实测 {before:?}"
    );

    // **一次 `update_layer` 把 L1 顶到最上** ✓（这正是不许改 `z_index` 时的死路）。
    let moved = call(
        &mut ctx,
        "update_layer",
        &json!({"layer_id": "L1", "patch": {"z_index": 99}}),
    );
    assert_eq!(
        moved.get("ok").and_then(Value::as_bool),
        Some(true),
        "`update_layer` 必须接受 `z_index`（内核已支持，只差工具层放行）⇒ {moved}"
    );
    let after = order(&mut ctx);
    assert_eq!(
        after,
        vec!["layer_default", "L2", "L1"],
        "改完 `z_index` 之后顺序必须真的变（L1 到顶）⇒ 实测 {after:?}"
    );
    // **恒等式**：图层集合不变、只是顺序变 ⇒ 排序后应与改前相同。
    let mut sorted_before = before.clone();
    let mut sorted_after = after.clone();
    sorted_before.sort();
    sorted_after.sort();
    assert_eq!(
        sorted_before, sorted_after,
        "移动层级不许增减图层 ⇒ 改前 {before:?} / 改后 {after:?}"
    );
}
