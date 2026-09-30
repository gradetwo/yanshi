//! `get_document` 的计数必须与 `list_objects` / `list_layers` **同一口径** ✓。
//!
//! 子 agent 实测（本轮的问题清单 ✓）：删掉 6 个图层之后**同一瞬间**，
//! `get_document` 报 501 个对象 / 14 个图层 ✗，而 `list_objects`/`list_layers` 报 259 / 6 ✓。
//! 根因：折叠层**保留墓碑**（`deleted_by` ✓ —— 这是"删除可撤销、日志可重放"的基础 ✓），
//! 而 `summary_json` 直接数 `state.objects.len()` ✗ ⇒ 把墓碑也算了进去 ✓。
//! 同一个界面里两个数不一致 ✓，用户会以为数据坏了 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_summary", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// **删除之后两个口径必须一致** ✓ —— 这正是子 agent 的复现路径 ✓：
/// 建若干图层与对象 ⇒ 删掉一部分 ⇒ 同时读 `get_document` 与 `list_*` ✓。
#[test]
fn the_document_summary_counts_live_entities_only() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_summary", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut expected_layers = 0usize;
    let mut expected_objects = 0usize;
    {
        let mut ctx = context(&mut workspace);
        for index in 0..4 {
            let layer_id = format!("L{index}");
            let created = registry.call(
                &mut ctx,
                "create_layer",
                &json!({"layer_id": layer_id, "name": layer_id}),
            );
            assert_eq!(created["ok"], json!(true), "{created}");
            expected_layers += 1;
            for slot in 0..3 {
                let drawn = registry.call(
                    &mut ctx,
                    "draw_shape",
                    &json!({
                        "layer_id": layer_id,
                        "object_id": format!("{layer_id}_box{slot}"),
                        "data": {
                            "geometry": {"kind": "rect",
                                         "bbox": {"x": 8.0 + slot as f64 * 12.0, "y": 8.0, "w": 10.0, "h": 10.0}},
                            "color": {"r": 0, "g": 0, "b": 0, "a": 255}
                        }
                    }),
                );
                assert_eq!(drawn["ok"], json!(true), "{drawn}");
                expected_objects += 1;
            }
        }
    }

    let summary_before = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };
    assert_eq!(
        (
            summary_before["layers"].as_u64(),
            summary_before["objects"].as_u64()
        ),
        (Some(expected_layers as u64), Some(expected_objects as u64)),
        "删除之前两个口径本来就该一致：{summary_before}"
    );

    // 删掉**一半**图层（连同其中的对象 ✓）。
    {
        let mut ctx = context(&mut workspace);
        for index in 0..2 {
            let removed = registry.call(
                &mut ctx,
                "delete_layer",
                &json!({"layer_id": format!("L{index}")}),
            );
            assert_eq!(removed["ok"], json!(true), "{removed}");
            expected_layers -= 1;
            expected_objects -= 3;
        }
    }

    let listed_layers = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_layers", &json!({}))
    };
    let listed_objects = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let summary_after = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };

    let live_layers = listed_layers["layers"].as_array().map_or(0, Vec::len) as u64;
    let live_objects = listed_objects["objects"].as_array().map_or(0, Vec::len) as u64;
    assert_eq!(
        (live_layers, live_objects),
        (expected_layers as u64, expected_objects as u64),
        "`list_*` 的口径：{listed_layers} / {listed_objects}"
    );

    // **核心断言** ✓：摘要必须与 `list_*` 一致 ✓（此前会多算墓碑 ✗）。
    assert_eq!(
        (
            summary_after["layers"].as_u64(),
            summary_after["objects"].as_u64()
        ),
        (Some(live_layers), Some(live_objects)),
        "`get_document` 必须只数存活实体：{summary_after}"
    );
    // 墓碑数要**单独**可见 ✓（想审计删了多少的人仍看得到 ✓，且不会被误读成当前内容 ✓）。
    assert!(
        summary_after["tombstoned_layers"].as_u64().unwrap_or(0) >= 2,
        "墓碑计数应能反映被删除的图层：{summary_after}"
    );
}
