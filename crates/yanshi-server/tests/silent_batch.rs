//! **批量静默提交** ✓（真实用户 §五-5 ✓）。
//!
//! **用户原话** ✓："批处理提交时为每个细分笔触**实时生成预览**与原子快照 ✓，
//! 在大画布大规模排线时产生**额外的 CAS 临时 IO**" ✗ ⇒ 建议支持批次静默提交 ✓。
//!
//! **本测试的判据** ✓：`batch` 里**每一个子结果**带不带 `preview` ✓ ——
//! 那是"省下来了没有"最直接的观测点 ✓（不用去数 IO ✓）。
//! **而且必须验"不静默时仍然有"** ✓ —— 否则我可能只是把预览**整个删掉了** ✗，
//! 那会静默改变所有既有调用方的返回 ✓（比不做更糟 ✗）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_silent", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = ToolContext::new(&mut workspace, "doc_silent", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let made = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    workspace
}

/// 跑一个 batch ✓，返回其中每个子结果的 `preview` 是否**缺失** ✓。
fn run_batch(workspace: &mut Workspace, silent: Option<bool>) -> (bool, Vec<bool>) {
    let mut args = json!({
        "calls": [
            {"tool": "draw_shape", "arguments": {"layer_id": "L", "object_id": "s1",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 40, "h": 30}},
                         "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}},
            {"tool": "draw_shape", "arguments": {"layer_id": "L", "object_id": "s2",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 60, "y": 40, "w": 40, "h": 30}},
                         "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}}
        ]
    });
    if let Some(silent) = silent {
        args["silent"] = json!(silent);
    }
    let made = {
        let mut ctx = ToolContext::new(workspace, "doc_silent", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "batch", &args)
    };
    let ok = made["ok"] == json!(true);
    // **键名是 `calls`** ✓（不是 `results` ✗ —— 我又猜了一次返回结构 ✓；照代码写 ✓）。
    let missing: Vec<bool> = made["calls"]
        .as_array()
        .map(|results| {
            results
                .iter()
                .map(|entry| {
                    let value = &entry["result"];
                    value.get("preview").is_none() || value["preview"].is_null()
                })
                .collect()
        })
        .unwrap_or_default();
    (ok, missing)
}

/// **`silent: true` ⇒ 子结果不带预览** ✓；**不写这个键 ⇒ 照旧带** ✓（行为不变 ✓）。
#[test]
fn a_silent_batch_skips_the_per_call_previews() {
    let mut workspace = workspace();
    // ① **缺省（不写 silent）** ✓：每个子结果**都应当有预览** ✓ —— 这条是"没把功能删掉"的护栏 ✓。
    let (ok, missing) = run_batch(&mut workspace, None);
    assert!(ok, "缺省 batch 应当成功");
    assert_eq!(missing.len(), 2, "应当有两个子结果");
    assert!(
        missing.iter().all(|absent| !absent),
        "缺省时每个子结果都应当带预览（否则我可能把预览整个删了 ✗）：{missing:?}"
    );
    // ② **`silent: true`** ✓：两个都**不该**带预览 ✓。
    let (ok, missing) = run_batch(&mut workspace, Some(true));
    assert!(ok, "静默 batch 应当成功（只是不预览 ✓，不是失败 ✗）");
    assert!(
        missing.iter().all(|absent| *absent),
        "静默时每个子结果都不该带预览：{missing:?}"
    );
    // ③ **`silent: false`** ✓：显式关掉 ⇒ 与缺省一致 ✓。
    let (ok, missing) = run_batch(&mut workspace, Some(false));
    assert!(ok);
    assert!(
        missing.iter().all(|absent| !absent),
        "显式 false 应当与缺省一致：{missing:?}"
    );
}

/// **静默只影响预览，不影响结果本身** ✓ —— 原子照样落、形状照样画 ✓。
#[test]
fn a_silent_batch_still_commits_the_work() {
    let mut workspace = workspace();
    let (ok, _) = run_batch(&mut workspace, Some(true));
    assert!(ok);
    // 两块形状都应当在文档里 ✓（而且都带 atom_id ✓）
    for object in ["s1", "s2"] {
        let got = {
            let mut ctx = ToolContext::new(&mut workspace, "doc_silent", "human:1", "session:test")
                .with_owner(true)
                .with_wait_for_render(true, 4_000);
            registry().call(&mut ctx, "get_object", &json!({ "object_id": object }))
        };
        assert_eq!(got["ok"], json!(true), "{object} 应当真的落进文档：{got}");
    }
    // **而且画出来了** ✓（不只是原子落了 ✓）
    let (_, _, pixels) = workspace
        .document_mut("doc_silent")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    let inked = pixels
        .chunks_exact(4)
        .filter(|pixel| {
            (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                / 1000
                < 200
        })
        .count();
    assert!(inked > 500, "两块形状都该画出来（实测 {inked} 个暗像素）");
}
