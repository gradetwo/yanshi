//! **`batch` 的 `silent` 必须真的省掉"每次调用渲染一次"** ✓（外部第 83 次的回放慢，实测在区域渲染）。
//! 断结构量：`silent:true` ⇒ 整批 `preview_calls == 0` ✓；不带 `silent` ⇒ **> 0** ✓（成对）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn source_bytes() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&16u32.to_le_bytes());
    for _ in 0..(16 * 16) {
        bytes.extend_from_slice(&[200, 60, 30, 255]);
    }
    bytes
}

#[test]
fn batch_silent_skips_the_per_call_render() {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let hash = workspace
        .store()
        .put(&source_bytes())
        .expect("blob")
        .to_string();
    {
        let mut ctx = ToolContext::new(
            &mut workspace,
            "doc_batch_silent",
            "human:1",
            "session:test",
        )
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
        registry.call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_batch_silent", "width": 256, "height": 256}),
        );
    }
    let mut ctx = ToolContext::new(
        &mut workspace,
        "doc_batch_silent",
        "human:1",
        "session:test",
    )
    .with_owner(true)
    .with_wait_for_render(true, 8_000);
    let layer = registry
        .call(&mut ctx, "list_layers", &json!({}))
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .and_then(|row| row.get("layer_id"))
        .and_then(Value::as_str)
        .unwrap_or("layer_default")
        .to_string();
    let calls: Vec<Value> = (0..4)
        .map(|index| {
            json!({
                "tool": "import_image",
                "arguments": {
                    "layer_id": layer,
                    "region": {"x": (index % 2) * 24, "y": (index / 2) * 24, "w": 16, "h": 16},
                    "bitmap": {"blob_hash": hash, "width": 16, "height": 16}
                }
            })
        })
        .collect();
    let previews = |reply: &Value| -> u64 {
        reply
            .get("timings")
            .and_then(|t| t.get("preview_calls"))
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX)
    };
    let loud = registry.call(&mut ctx, "batch", &json!({"calls": calls}));
    assert_eq!(
        loud.get("ok").and_then(Value::as_bool),
        Some(true),
        "{loud}"
    );
    let quiet = registry.call(&mut ctx, "batch", &json!({"calls": calls, "silent": true}));
    assert_eq!(
        quiet.get("ok").and_then(Value::as_bool),
        Some(true),
        "{quiet}"
    );
    assert_eq!(
        previews(&quiet),
        0,
        "`silent:true` ⇒ 整批不许有任何「区域渲染」（否则每个原子各渲一次 ⇒ 回放慢）"
    );
    assert!(
        previews(&loud) > 0,
        "不带 `silent` ⇒ 必须有渲染（否则这条判据没有鉴别力）⇒ 实测 {}",
        previews(&loud)
    );
}
