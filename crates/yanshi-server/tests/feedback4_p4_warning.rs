//! **报告 4 P4（第五件小修）：`brush_stroke` 缺省线性直通 warning**。
//!
//! 只加回执字段，不改缺省语义、不改像素：
//! * 不传 `color_space` + 浮点色 ⇒ 回执 `warnings` 里有一条含「线性」与「更亮」的提示；
//! * 显式传 `color_space="srgb"` ⇒ 无本条；字节色（无空间）⇒ 无本条；
//! * 像素等价性：缺省浮点色与显式 `color_space="linear"` 同一笔，渲染出的整幅 RGBA
//!   逐字节相同（warning 代码在提交之后、只改回执 `Value`，落库像素不受影响）。
//!   两笔各用独立工作区／独立文档（上一笔的墨不许污染下一笔的采样，见仓库 AGENTS.md 第 916 轮教训）。
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
    let root = std::env::temp_dir().join(format!("yanshi_f4p4_{tag}_{}", std::process::id()));
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
        .create_document(NewDocument::new("doc_f4p4", 200, 200), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_f4p4", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let layer = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(layer["ok"], json!(true), "{layer}");
    (Cleanup(root), workspace)
}

fn stroke(
    workspace: &mut Workspace,
    object_id: &str,
    color: Value,
    color_space: Option<&str>,
) -> Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(workspace, "doc_f4p4", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let mut args = json!({
        "layer_id": "L", "object_id": object_id, "brush": "2B_pencil", "size": 12.0,
        "points": [[40.0, 100.0, 0.8], [160.0, 100.0, 0.8]],
        "color": color,
    });
    if let Some(space) = color_space {
        args["color_space"] = json!(space);
    }
    registry.call(&mut ctx, "brush_stroke", &args)
}

fn has_p4_warning(out: &Value) -> bool {
    out["warnings"]
        .as_array()
        .map(|items| {
            items.iter().any(|item| {
                item.as_str()
                    .unwrap_or_default()
                    .contains("未传 color_space")
            })
        })
        .unwrap_or(false)
}

fn p4_note(out: &Value) -> String {
    out["warnings"]
        .as_array()
        .and_then(|items| {
            items.iter().find_map(|item| {
                let text = item.as_str().unwrap_or_default();
                text.contains("未传 color_space").then(|| text.to_owned())
            })
        })
        .unwrap_or_default()
}

#[test]
fn default_float_color_warns_linear_and_brighter() {
    let (_cleanup, mut workspace) = workspace("default");
    let out = stroke(&mut workspace, "o_default", json!([0.5, 0.2, 0.2]), None);
    assert_eq!(out["ok"], json!(true), "{out}");
    assert_eq!(
        out["applied"]["color_space"],
        json!("linear"),
        "回显仍应报 linear（既有字段不动）：{out}"
    );
    let note = p4_note(&out);
    assert!(
        note.contains("线性") && note.contains("更亮"),
        "warning 文案必须同时说明线性直通与同值更亮：{out}"
    );
}

#[test]
fn explicit_srgb_and_byte_color_do_not_warn() {
    let (_cleanup, mut workspace) = workspace("explicit");
    let srgb = stroke(
        &mut workspace,
        "o_srgb",
        json!([0.5, 0.2, 0.2]),
        Some("srgb"),
    );
    assert_eq!(srgb["ok"], json!(true), "{srgb}");
    assert!(
        !has_p4_warning(&srgb),
        "显式 srgb 不应触发本 warning：{srgb}"
    );
    let bytes = stroke(&mut workspace, "o_bytes", json!([128, 51, 51]), None);
    assert_eq!(bytes["ok"], json!(true), "{bytes}");
    assert!(
        !has_p4_warning(&bytes),
        "字节色走 0–255 解释、不是线性直通，不应触发：{bytes}"
    );
}

#[test]
fn default_float_pixels_equal_explicit_linear_pixels() {
    // 缺省浮点 vs 显式 linear：同一笔、两个独立工作区，整幅渲染 RGBA 逐字节相同；
    // 差别只在回执：缺省带 warning，显式不带。
    let (_cleanup_a, mut plain) = workspace("plain");
    let defaulted = stroke(&mut plain, "o_a", json!([0.5, 0.2, 0.2]), None);
    assert_eq!(defaulted["ok"], json!(true), "{defaulted}");
    let (_cleanup_b, mut explicit) = workspace("explicit_linear");
    let lined = stroke(&mut explicit, "o_b", json!([0.5, 0.2, 0.2]), Some("linear"));
    assert_eq!(lined["ok"], json!(true), "{lined}");
    assert!(
        has_p4_warning(&defaulted),
        "缺省浮点应带 warning：{defaulted}"
    );
    assert!(
        !has_p4_warning(&lined),
        "显式 linear 不应带本 warning：{lined}"
    );
    let pixels_a = plain
        .render_region_raw("doc_f4p4", Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("渲染缺省笔")
        .2;
    let pixels_b = explicit
        .render_region_raw("doc_f4p4", Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("渲染显式 linear 笔")
        .2;
    assert_eq!(
        pixels_a, pixels_b,
        "缺省浮点与显式 linear 必须是同一线性解释：渲染像素逐字节相同"
    );
    assert!(
        defaulted["painted_pixels"] == lined["painted_pixels"]
            && defaulted["ink_footprint_pixels"] == lined["ink_footprint_pixels"]
            && defaulted["max_channel_delta"] == lined["max_channel_delta"],
        "像素派生回执量也应一致：{defaulted} vs {lined}"
    );
}
