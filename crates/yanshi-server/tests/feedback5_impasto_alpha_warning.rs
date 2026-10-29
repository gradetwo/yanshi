//! 报告 5+6 §2 第 2 项：impasto/relief 在 `height_source=alpha` 恒定时回执顶层 warning。
//!
//! **四层齐**：实现（`tools.rs` 探针 + `document.rs` f32 alpha 扫描）／输入能触发
//! （全不透明笔触层 + 缺省／显式 alpha；`luminance` 对照；起伏 alpha 对照；
//! `update_filter` 切到 alpha）／断言（见下）／变异（探针恒 `None` ⇒
//! 全不透明三案红，见包说明）。
//!
//! 断言不能空转：全不透明案若画出来的 alpha 其实有起伏，警告就不会出现、
//! 断言红；起伏案若画出来的 alpha 其实恒定，警告就会出现、断言红。
//! 即每条断言的输入都真的制造了它要检查的情况。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是小 tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_root(tag: &str) -> (std::path::PathBuf, Cleanup) {
    let path = std::env::temp_dir().join(format!("yanshi_f5imp_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建临时根");
    let guard = Cleanup(path.clone());
    (path, guard)
}

/// 落盘工作区 + 指向仓库 `assets/`（测的就是真会发出去的那批笔刷）。
fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 一道横向满幅油彩笔（探针 P3 同款：`oil-01-paint`、压感恒定、色值轮换）。
fn row_stroke(
    workspace: &mut Workspace,
    doc: &str,
    id: &str,
    y: f64,
    color: Value,
    pressure: f64,
) -> Value {
    call(
        workspace,
        doc,
        "brush_stroke",
        json!({
            "layer_id": "layer_default", "object_id": id, "brush": "oil-01-paint",
            "points": [[0.0, y, pressure], [256.0, y, pressure]],
            "size": 60.0, "color": color,
        }),
    )
}

/// 铺满 256² 的全不透明层：5 道间距 55、笔宽 60，探针 P3 的 512 版同构。
fn paint_opaque(workspace: &mut Workspace, doc: &str) {
    let colors = [
        (200, 40, 40),
        (40, 160, 60),
        (40, 80, 200),
        (220, 180, 40),
        (150, 60, 180),
    ];
    for (index, y) in [30.0, 85.0, 140.0, 195.0, 250.0].iter().enumerate() {
        let (r, g, b) = colors[index % colors.len()];
        let out = row_stroke(
            workspace,
            doc,
            &format!("stroke_opaque_{index}"),
            *y,
            json!({"r": r, "g": g, "b": b, "a": 255}),
            0.9,
        );
        assert_eq!(out["ok"], json!(true), "铺底笔触应成功：{out}");
    }
}

/// 起伏 alpha 层：同构满幅，但隔道换透明度与压感（任一生效都会让 alpha 起伏）。
fn paint_varying_alpha(workspace: &mut Workspace, doc: &str) {
    let rows = [
        (30.0, 255, 0.9),
        (85.0, 128, 0.5),
        (140.0, 255, 0.9),
        (195.0, 128, 0.5),
        (250.0, 255, 0.9),
    ];
    for (index, (y, alpha, pressure)) in rows.iter().enumerate() {
        let out = row_stroke(
            workspace,
            doc,
            &format!("stroke_vary_{index}"),
            *y,
            json!({"r": 200, "g": 40, "b": 40, "a": alpha}),
            *pressure,
        );
        assert_eq!(out["ok"], json!(true), "起伏笔触应成功：{out}");
    }
}

fn add_filter(workspace: &mut Workspace, doc: &str, name: &str, params: Value) -> Value {
    call(
        workspace,
        doc,
        "add_filter",
        json!({"layer_id": "layer_default", "filter_name": name, "params": params}),
    )
}

/// 回执顶层 `warnings` 里有没有本项警告（对象形态，`kind` 点名）。
fn has_alpha_warning(response: &Value) -> bool {
    response["warnings"]
        .as_array()
        .map(|list| {
            list.iter()
                .any(|warning| warning["kind"] == json!("relief_no_effect_constant_alpha"))
        })
        .unwrap_or(false)
}

fn warning_detail(response: &Value) -> String {
    response["warnings"]
        .as_array()
        .and_then(|list| {
            list.iter()
                .find(|warning| warning["kind"] == json!("relief_no_effect_constant_alpha"))
        })
        .and_then(|warning| warning["detail"].as_str())
        .unwrap_or("")
        .to_owned()
}

fn export_bytes(workspace: &mut Workspace, doc: &str, path: &std::path::Path) -> Vec<u8> {
    let out = call(
        workspace,
        doc,
        "export_png",
        json!({"path": path.to_string_lossy()}),
    );
    assert_eq!(out["ok"], json!(true), "导出应成功：{out}");
    std::fs::read(path).expect("导出文件应存在")
}

#[test]
fn opaque_layer_default_alpha_warns() {
    let (root, _guard) = temp_root("default");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_opaque(&mut workspace, "doc_imp");
    let response = add_filter(&mut workspace, "doc_imp", "relief", json!({}));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        has_alpha_warning(&response),
        "全不透明层 + 缺省 alpha 应有顶层警告：{response}"
    );
    assert!(
        warning_detail(&response).contains("luminance"),
        "警告应指路 luminance：{response}"
    );
}

#[test]
fn opaque_layer_explicit_alpha_warns() {
    let (root, _guard) = temp_root("explicit");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_opaque(&mut workspace, "doc_imp");
    let response = add_filter(
        &mut workspace,
        "doc_imp",
        "relief",
        json!({"height_source": "alpha"}),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        has_alpha_warning(&response),
        "全不透明层 + 显式 alpha 应有顶层警告：{response}"
    );
}

#[test]
fn opaque_layer_impasto_alias_warns() {
    let (root, _guard) = temp_root("impasto");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_opaque(&mut workspace, "doc_imp");
    let response = add_filter(&mut workspace, "doc_imp", "impasto", json!({}));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        has_alpha_warning(&response),
        "impasto 是 relief 别名，全不透明层 + 缺省 alpha 同样应警告：{response}"
    );
}

#[test]
fn opaque_layer_luminance_no_warning_and_has_effect() {
    let (root, _guard) = temp_root("luminance");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_opaque(&mut workspace, "doc_imp");
    let before = export_bytes(&mut workspace, "doc_imp", &root.join("before.png"));
    let response = add_filter(
        &mut workspace,
        "doc_imp",
        "relief",
        json!({"height_source": "luminance"}),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        !has_alpha_warning(&response),
        "luminance 下不得有本项警告：{response}"
    );
    let after = export_bytes(&mut workspace, "doc_imp", &root.join("after.png"));
    assert_ne!(
        before, after,
        "luminance 在全不透明层上应有效果（探针：512² 上 8260 像素变化）"
    );
}

#[test]
fn varying_alpha_no_warning() {
    let (root, _guard) = temp_root("varying");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_varying_alpha(&mut workspace, "doc_imp");
    let response = add_filter(&mut workspace, "doc_imp", "relief", json!({}));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        !has_alpha_warning(&response),
        "起伏 alpha 下不得有本项警告：{response}"
    );
}

#[test]
fn update_filter_to_alpha_warns() {
    let (root, _guard) = temp_root("update");
    let mut workspace = workspace(&root, "doc_imp", 256, 256);
    paint_opaque(&mut workspace, "doc_imp");
    let added = add_filter(
        &mut workspace,
        "doc_imp",
        "relief",
        json!({"height_source": "luminance"}),
    );
    assert_eq!(added["ok"], json!(true), "{added}");
    assert!(
        !has_alpha_warning(&added),
        "先落 luminance 时不得警告：{added}"
    );
    let effects = call(&mut workspace, "doc_imp", "list_effects", json!({}));
    let object_id = effects["effects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|effect| effect["filter_name"] == json!("relief"))
        .and_then(|effect| effect["object_id"].as_str())
        .expect("应能找到刚创建的 relief 对象")
        .to_owned();
    let updated = call(
        &mut workspace,
        "doc_imp",
        "update_filter",
        json!({"object_id": object_id, "params": {"height_source": "alpha"}}),
    );
    assert_eq!(updated["ok"], json!(true), "{updated}");
    assert!(
        has_alpha_warning(&updated),
        "update_filter 切到 alpha 后同样应警告：{updated}"
    );
}
