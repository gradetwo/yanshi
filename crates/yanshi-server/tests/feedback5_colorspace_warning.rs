//! **报告 5/6 第 6 项：`brush_stroke` 显式 linear 下渐变两端混写 warning**。
//!
//! 钉死定义（与报告 4 P4 分工）：只在显式 `color_space="linear"` 且 `color` 与
//! `color_to` 一端数组、一端对象式／hex 时报；不传空间（P4 的地盘）、显式 srgb、
//! 两端同族、只给一端时不报。只加回执字段，不改缺省语义、不改像素：
//! * 混写笔 ⇒ 回执 `warnings` 里有一条含「混写」与「linear」的提示；
//! * 混写笔与两端同族对照笔（端点解码字节相同）⇒ 整幅渲染 RGBA 逐字节相同，
//!   且像素派生回执量一致（warning 代码在提交之后、只改回执 `Value`，落库像素不受影响）。
//! * 两笔各用独立工作区／独立文档（上一笔的墨不许污染下一笔的采样，见仓库 AGENTS.md 第 916 轮教训）。
use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录**（`/tmp` 是 tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace(tag: &str) -> (Cleanup, Workspace) {
    let root = std::env::temp_dir().join(format!("yanshi_f5cs_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    assert!(assets.is_dir(), "找不到资产根：{}", assets.display());
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new("doc_f5cs", 200, 200), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_f5cs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let layer = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(layer["ok"], json!(true), "{layer}");
    // 显式绑定守卫：判据 check-tests-clean-temp 只认 `x = Cleanup(` 的绑定形态，
    // 元组里直接写 `Cleanup(root)` 会被判为“从未绑定”（语义相同，但结构检查认不出）。
    let cleanup = Cleanup(root);
    (cleanup, workspace)
}

fn stroke(
    workspace: &mut Workspace,
    object_id: &str,
    color: Value,
    color_to: Option<Value>,
    color_space: Option<&str>,
) -> Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(workspace, "doc_f5cs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let mut args = json!({
        "layer_id": "L", "object_id": object_id, "brush": "2B_pencil", "size": 12.0,
        "points": [[40.0, 100.0, 0.8], [160.0, 100.0, 0.8]],
        "color": color,
    });
    if let Some(to) = color_to {
        args["color_to"] = to;
    }
    if let Some(space) = color_space {
        args["color_space"] = json!(space);
    }
    registry.call(&mut ctx, "brush_stroke", &args)
}

fn has_mixed_warning(out: &Value) -> bool {
    out["warnings"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .any(|item| item.as_str().unwrap_or_default().contains("混写"))
        })
        .unwrap_or(false)
}

fn mixed_note(out: &Value) -> String {
    out["warnings"]
        .as_array()
        .and_then(|items| {
            items.iter().find_map(|item| {
                let text = item.as_str().unwrap_or_default();
                text.contains("混写").then(|| text.to_owned())
            })
        })
        .unwrap_or_default()
}

#[test]
fn explicit_linear_gradient_mix_warns() {
    let (_cleanup, mut workspace) = workspace("mixed");
    // 混写：显式 linear＋数组起点（线性直通）＋对象式末端（恒为 sRGB 字节）。
    let out = stroke(
        &mut workspace,
        "o_mixed",
        json!([0.47, 0.353, 0.235]),
        Some(json!({"r": 120, "g": 90, "b": 60})),
        Some("linear"),
    );
    assert_eq!(out["ok"], json!(true), "{out}");
    // 回执 applied 只回显 color 一端的空间（linear），末端的 sRGB 解释被藏住——正是警告的理由。
    assert_eq!(
        out["applied"]["color_space"],
        json!("linear"),
        "回显仍应报 color 一端的 linear（既有字段不动）：{out}"
    );
    let note = mixed_note(&out);
    assert!(
        note.contains("混写") && note.contains("linear") && note.contains("同一写法"),
        "warning 文案必须同时说明混写、显式 linear 与统一写法的出路：{out}"
    );
}

#[test]
fn non_mixed_gradients_do_not_warn() {
    let (_cleanup, mut workspace) = workspace("nonmixed");
    // 纯 linear：显式 linear＋数组＋数组。
    let pure_linear = stroke(
        &mut workspace,
        "o_pure_linear",
        json!([0.47, 0.353, 0.235]),
        Some(json!([0.5, 0.2, 0.2])),
        Some("linear"),
    );
    assert_eq!(pure_linear["ok"], json!(true), "{pure_linear}");
    assert!(
        !has_mixed_warning(&pure_linear),
        "纯 linear 两端不应触发本 warning：{pure_linear}"
    );
    // 纯 sRGB 字节族：显式 linear＋对象式＋hex（回显词不同，但同为字节解释）。
    let pure_srgb = stroke(
        &mut workspace,
        "o_pure_srgb",
        json!({"r": 182, "g": 160, "b": 133}),
        Some(json!("#785A3C")),
        Some("linear"),
    );
    assert_eq!(pure_srgb["ok"], json!(true), "{pure_srgb}");
    assert!(
        !has_mixed_warning(&pure_srgb),
        "对象式＋hex 同为 sRGB 字节族，不应触发本 warning：{pure_srgb}"
    );
    // 显式 srgb：数组端也被解释为 sRGB 字节，不存在混写。
    let explicit_srgb = stroke(
        &mut workspace,
        "o_explicit_srgb",
        json!([0.47, 0.353, 0.235]),
        Some(json!({"r": 120, "g": 90, "b": 60})),
        Some("srgb"),
    );
    assert_eq!(explicit_srgb["ok"], json!(true), "{explicit_srgb}");
    assert!(
        !has_mixed_warning(&explicit_srgb),
        "显式 srgb 下两端同为 sRGB 字节，不应触发本 warning：{explicit_srgb}"
    );
    // 不传空间＋字节起点＋浮点末端：两端口径不同，但属 P4（缺省线性）覆盖，
    // 本判据不抢报——合入后该笔只得 P4 一条，不重复。
    let defaulted = stroke(
        &mut workspace,
        "o_defaulted",
        json!([120, 90, 60]),
        Some(json!([0.5, 0.2, 0.2])),
        None,
    );
    assert_eq!(defaulted["ok"], json!(true), "{defaulted}");
    assert!(
        !has_mixed_warning(&defaulted),
        "不传空间时本 warning 不报（P4 的地盘）：{defaulted}"
    );
}

#[test]
fn mixed_pixels_equal_same_bytes_control() {
    // 混写笔：显式 linear，数组浮点起点解码为 [182,160,133]（探针 07 同值），
    // 对象式末端解码为 [120,90,60]。
    let (_cleanup_a, mut mixed_ws) = workspace("mixed_px");
    let mixed = stroke(
        &mut mixed_ws,
        "o_a",
        json!([0.47, 0.353, 0.235]),
        Some(json!({"r": 120, "g": 90, "b": 60})),
        Some("linear"),
    );
    assert_eq!(mixed["ok"], json!(true), "{mixed}");
    assert!(has_mixed_warning(&mixed), "混写笔应带 warning：{mixed}");
    // 对照笔：显式 linear，两端都用对象式（同为 sRGB 字节族，不触发本 warning），
    // 端点字节与混写笔的解码结果相同。
    let (_cleanup_b, mut control_ws) = workspace("control_px");
    let control = stroke(
        &mut control_ws,
        "o_b",
        json!({"r": 182, "g": 160, "b": 133}),
        Some(json!({"r": 120, "g": 90, "b": 60})),
        Some("linear"),
    );
    assert_eq!(control["ok"], json!(true), "{control}");
    assert!(
        !has_mixed_warning(&control),
        "对照笔两端同族，不应带本 warning：{control}"
    );
    let pixels_a = mixed_ws
        .render_region_raw("doc_f5cs", Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("渲染混写笔")
        .2;
    let pixels_b = control_ws
        .render_region_raw("doc_f5cs", Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("渲染对照笔")
        .2;
    assert_eq!(
        pixels_a, pixels_b,
        "混写笔与端点字节相同的对照笔必须是同一解码：渲染像素逐字节相同"
    );
    assert!(
        mixed["painted_pixels"] == control["painted_pixels"]
            && mixed["ink_footprint_pixels"] == control["ink_footprint_pixels"]
            && mixed["max_channel_delta"] == control["max_channel_delta"],
        "像素派生回执量也应一致：{mixed} vs {control}"
    );
}
