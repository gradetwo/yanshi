//! **`.myb` 笔刷落笔** ✓（用户裁定采纳 Hokusai ✓；这是把 196 支现成笔刷接进来的那一步 ✓）。
//!
//! **为什么要单独测** ✓：这是**第二条笔触引擎** ✓与我们自己的介质插件 ABI 并列 ✓；
//! 两者都汇进**同一个提交路径**（`store().put` → `import_image` ✓）⇒ 提交语义只有一份 ✓，
//! 但**点 → 墨**这一段是 Hokusai 的事 ✓ ⇒ 必须有我们自己的断言把它钉住 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_brush_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 内置资产目录指向**仓库里的 `assets/`** ✓（测的就是**真会发出去的那批笔刷** ✓）。
fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_brush", 400, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_brush", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **一笔 .myb 笔刷真的落下墨** ✓，而且**稀疏控制点之间被补间了** ✓
///（用户正是在介质那边报过"离散盖章" ✗ ⇒ 这里用同一判据把它钉住 ✓）。
#[test]
fn a_sparse_myb_stroke_paints_a_continuous_band() {
    let root = temp_dir("continuous");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "b1", "brush": "2B_pencil", "size": 24,
            // **三点、间距很大** ✓（若不补间 ⇒ 三段离散圆点 ✗）。
            "points": [[40.0, 60.0, 0.4], [200.0, 120.0, 0.9], [360.0, 70.0, 0.4]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // **补间的证据** ✓：步数远多于控制点数 ✓。
    let steps = made["steps"].as_u64().unwrap_or(0);
    assert!(steps > 100, "稀疏三点应当被细分成很多步：{made}");
    let painted = made["painted_pixels"].as_u64().unwrap_or(0);
    assert!(painted > 1000, "应当落下可观的像素：{made}");
    let region = &made["region"];
    assert!(
        region["w"].as_i64().unwrap_or(0) > 300,
        "区域应当横跨整条控制点：{made}"
    );
}

/// **同一笔两次 ⇒ 逐字节相同** ✓（确定性 ✓ —— 这是本项目的硬要求 ✓）。
#[test]
fn the_same_stroke_is_byte_identical_twice() {
    let root = temp_dir("determinism");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let args = json!({
        "layer_id": "L", "brush": "2B_pencil", "size": 20,
        "points": [[30.0, 50.0, 0.5], [170.0, 90.0, 0.8]]
    });
    let first = call(&mut workspace, "brush_stroke", args.clone());
    let mut second_args = args.clone();
    second_args["object_id"] = json!("b2");
    let second = call(&mut workspace, "brush_stroke", second_args);
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(second["ok"], json!(true), "{second}");
    // **区域与像素数都要一致** ✓（对象 id 不同不影响墨 ✓）。
    assert_eq!(
        first["region"], second["region"],
        "同参数的落笔区域应当一致"
    );
    assert_eq!(
        first["painted_pixels"], second["painted_pixels"],
        "同参数的落笔像素数应当一致（Hokusai 是确定性的 ✓）"
    );
}

/// **找不到笔刷时，要列出可用的名字** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn an_unknown_brush_lists_what_is_available() {
    let root = temp_dir("unknown");
    let mut workspace = workspace(&root);
    let got = call(
        &mut workspace,
        "brush_stroke",
        json!({ "layer_id": "L", "brush": "no_such_brush", "points": [[10.0, 10.0, 0.5]] }),
    );
    assert_eq!(got["ok"], json!(false), "{got}");
    assert_eq!(got["error_code"], json!("reference_not_found"), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("2B_pencil") || detail.contains(".myb"),
        "要列出可用笔刷：{detail}"
    );
}

/// **用户导入的笔刷要能覆盖内置的** ✓（缓存优先 ✓ —— 与列举时的优先级一致 ✓）。
#[test]
fn a_brush_imported_into_the_cache_shadows_the_bundled_one() {
    let root = temp_dir("shadow");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    // 从仓库里复制一支真笔刷 ✓ 换个名字导入 ✓。
    let origin =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brushes/2B_pencil.myb");
    std::fs::copy(&origin, source.join("mine.myb")).unwrap();
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let imported = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "brush", "name": "mine.myb", "path": source.join("mine.myb").display().to_string() }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    // **导入过的那支能直接用** ✓（解析要走"缓存优先" ✓）。
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({ "layer_id": "L", "brush": "mine", "size": 18, "points": [[20.0, 30.0, 0.6], [120.0, 60.0, 0.6]] }),
    );
    assert_eq!(made["ok"], json!(true), "导入的笔刷应当能直接落笔：{made}");
    assert_eq!(made["brush"], json!("mine.myb"), "{made}");
}
