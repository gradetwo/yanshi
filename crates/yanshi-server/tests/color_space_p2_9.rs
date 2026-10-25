//! **P2-9：`color_space` 补齐 `try_stroke` / `brush_preview`** ✓
//!
//! 与期 1 C1（`brush_stroke` / `medium_stroke`）同口径：显式参数只认 `"srgb"` / `"linear"`，
//! 不传 ⇒ 现行量级切换；非法值 ⇒ `invalid_argument`；回执 `applied.color_space` 回显
//! 实际生效的单位。`update_stroke` 的三路见 `tests/update_stroke.rs`。
//!
//! **判据** ✓：① `try_stroke` srgb 臂回显 `srgb_bytes`、缺省臂回显 `linear`，
//! 两臂的区域图 base64 **不同**（行为真有区别，不只是回显换了个词 ✓）；
//! ② 非法值 ⇒ `ok:false` + `invalid_argument`；③ `based_on` 继承空间；
//! ④ `brush_preview` srgb/linear 两臂 `max_channel_delta` 分开（白底 178 vs 106），
//! 顶层 `color_space` 仍是 `"srgb"`（混色与输出的空间，不受参数影响 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `try_stroke` / `brush_preview` 要从那儿读 `.myb` 笔刷 ✓。
fn workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_cs29_{name}_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_cs", 320, 240),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_cs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn points() -> serde_json::Value {
    json!([[40.0, 120.0, 1.0], [280.0, 120.0, 1.0]])
}

/// **`try_stroke` 认 `color_space`** ✓：srgb 臂与缺省臂回显分开、区域图真的不同 ✓。
#[test]
fn try_stroke_honors_color_space() {
    let mut workspace = workspace("try");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let srgb = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "srgb",
        }),
    );
    assert_eq!(srgb["ok"], json!(true), "{srgb}");
    assert_eq!(
        srgb["applied"]["color_space"],
        json!("srgb_bytes"),
        "srgb 浮点臂回显应为 srgb_bytes：{srgb}"
    );
    let def = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30],
        }),
    );
    assert_eq!(def["ok"], json!(true), "{def}");
    assert_eq!(
        def["applied"]["color_space"],
        json!("linear"),
        "缺省浮点臂回显应为 linear：{def}"
    );
    // **行为真有区别** ✓：回显换词不够，区域图的字节必须不同 ✓。
    assert_ne!(
        srgb["image"], def["image"],
        "srgb 与缺省两臂的区域图必须不同（否则参数没生效）"
    );
    let linear = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "linear",
        }),
    );
    assert_eq!(linear["applied"]["color_space"], json!("linear"));
    assert_eq!(
        linear["image"], def["image"],
        "显式 linear 应与缺省同一条路（区域图逐字节相同）"
    );
}

/// **`try_stroke` 非法值 ⇒ `invalid_argument`** ✓（与 `brush_stroke` 同一条校验 ✓）。
#[test]
fn try_stroke_rejects_an_unknown_color_space() {
    let mut workspace = workspace("try_bad");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let bad = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "lab",
        }),
    );
    assert_eq!(bad["ok"], json!(false), "非法值必须失败：{bad}");
    assert_eq!(bad["error_code"], json!("invalid_argument"), "{bad}");
    let detail = bad["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("srgb") && detail.contains("linear"),
        "报错里要列出两个允许值：{detail}"
    );
}

/// **`try_stroke` 的 `based_on` 继承空间** ✓（首笔 srgb ⇒ 次笔只换色不给空间，仍是 srgb ✓）。
#[test]
fn try_stroke_based_on_inherits_color_space() {
    let mut workspace = workspace("try_inh");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let first = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "srgb",
        }),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let attempt_id = first["attempt_id"].as_str().unwrap().to_owned();
    let second = call(
        &mut workspace,
        "try_stroke",
        json!({ "based_on": attempt_id, "color": [0.45, 0.38, 0.30] }),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        second["applied"]["color_space"],
        json!("srgb_bytes"),
        "based_on 次笔应继承首笔的空间：{second}"
    );
    assert_eq!(
        second["image"], first["image"],
        "同色同空间 ⇒ 次笔区域图应与首笔逐字节相同"
    );
    // **继承的空间必须真的生效** ✓：再试一笔不带空间的（缺省线性），它的图必须与次笔不同 ✓
    //（只断"次笔回显是 srgb"不够 —— 回显读的是参数，参数被忽略时回显照样写 srgb ✗）。
    let third = call(
        &mut workspace,
        "try_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30],
        }),
    );
    assert_eq!(third["ok"], json!(true), "{third}");
    assert_ne!(
        second["image"], third["image"],
        "继承的 srgb 必须真的生效（次笔的图应与缺省线性臂不同）"
    );
}

/// **`brush_preview` 认 `color_space`** ✓：回显分开、读数分开，顶层空间仍是 `srgb` ✓。
#[test]
fn brush_preview_honors_color_space() {
    let mut workspace = workspace("preview");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let srgb = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "srgb",
        }),
    );
    assert_eq!(srgb["ok"], json!(true), "{srgb}");
    assert_eq!(
        srgb["applied"]["color_space"],
        json!("srgb_bytes"),
        "srgb 浮点臂回显应为 srgb_bytes：{srgb}"
    );
    // 白底：(115,97,77) ⇒ max 255−77 = 178。
    let srgb_delta = srgb["max_channel_delta"].as_u64().unwrap_or(0);
    assert!(
        (173..=183).contains(&srgb_delta),
        "srgb 臂 max_channel_delta 应 ≈178（实测 {srgb_delta}）"
    );
    assert_eq!(
        srgb["color_space"],
        json!("srgb"),
        "顶层 color_space 仍是混色与输出的空间：{srgb}"
    );
    let linear = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "linear",
        }),
    );
    assert_eq!(linear["applied"]["color_space"], json!("linear"));
    // 白底：(179,166,149) ⇒ max 255−149 = 106。
    let linear_delta = linear["max_channel_delta"].as_u64().unwrap_or(0);
    assert!(
        (101..=111).contains(&linear_delta),
        "linear 臂 max_channel_delta 应 ≈106（实测 {linear_delta}）"
    );
    assert!(
        srgb_delta.saturating_sub(linear_delta) >= 50,
        "两臂读数必须分开 ≥50（实测 {srgb_delta} vs {linear_delta}）"
    );
    let def = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30],
        }),
    );
    assert_eq!(def["applied"]["color_space"], json!("linear"));
    assert_eq!(
        def["max_channel_delta"], linear["max_channel_delta"],
        "缺省臂应与 linear 臂同一条路"
    );
}

/// **`brush_preview` 非法值 ⇒ `invalid_argument`** ✓。
#[test]
fn brush_preview_rejects_an_unknown_color_space() {
    let mut workspace = workspace("preview_bad");
    let bad = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "100%_Opaque", "points": points(), "size": 30,
            "color": [0.45, 0.38, 0.30], "color_space": "lab",
        }),
    );
    assert_eq!(bad["ok"], json!(false), "非法值必须失败：{bad}");
    assert_eq!(bad["error_code"], json!("invalid_argument"), "{bad}");
}
