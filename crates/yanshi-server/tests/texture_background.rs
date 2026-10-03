//! **纹理背景** ✓（目标第 ② 件 ✓）。
//!
//! **它比"导入成图层"多做了什么** ✓：**自己沉到最底** ✓ ——
//! `create_layer` 会把新层放在**最上面** ✗，而"背景"必须在**最下面** ✓
//!（用户先前报过"图层顺序与预期不一致" ✓，根因之一就是新建层的落点 ✓）。
//! ⇒ 这条测试专门断言**背景在最底** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_texbg_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_bg", 120, 80),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_bg", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

fn layer_order(workspace: &mut Workspace) -> Vec<String> {
    let listed = call(workspace, "list_layers", json!({}));
    listed["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|layer| {
            layer["layer_id"]
                .as_str()
                .or_else(|| layer["id"].as_str())
                .unwrap_or_default()
                .to_owned()
        })
        .collect()
}

/// **默认铺法（tile）会把背景沉到最底** ✓ —— 即使之前已经有别的图层 ✓。
#[test]
fn the_background_layer_goes_to_the_bottom() {
    let root = temp_dir("bottom");
    let mut workspace = workspace(&root);
    // 先用一个正常图层占住"上面" ✓。
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "art" }))["ok"],
        json!(true)
    );
    // 用缺省图层参数 ✓ ⇒ 工具应当**自己新建并沉底** ✓。
    let made = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png" }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["mode"], json!("tile"), "缺省应当是平铺：{made}");
    assert_eq!(
        made["region"],
        json!({ "x": 0, "y": 0, "w": 120, "h": 80 }),
        "{made}"
    );
    let created = made["created_layer"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!created.is_empty(), "缺省应当新建一个背景图层：{made}");
    // **自下而上** ✓ ⇒ 第一个就是最底 ✓。
    let order = layer_order(&mut workspace);
    assert_eq!(
        order.first().map(String::as_str),
        Some(created.as_str()),
        "背景必须在最底：{order:?}"
    );
    assert!(
        order.contains(&"art".to_string()),
        "原来的图层还得在：{order:?}"
    );
}

/// **指定图层时不新建、也不重排** ✓（调用方自己安排 ✓）。
#[test]
fn giving_a_layer_does_not_create_or_reorder_anything() {
    let root = temp_dir("given");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(
            &mut workspace,
            "create_layer",
            json!({ "layer_id": "only" })
        )["ok"],
        json!(true)
    );
    let before = layer_order(&mut workspace);
    let made = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png", "layer_id": "only" }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert!(made.get("created_layer").is_none(), "不该新建图层：{made}");
    assert_eq!(layer_order(&mut workspace), before, "不该改动图层顺序");
}

/// **三种铺法都能用** ✓，且都铺满整幅 ✓。
#[test]
fn all_three_modes_cover_the_whole_canvas() {
    for mode in ["tile", "stretch", "cover"] {
        // **每种铺法各自一个工作区** ✗ —— 共用根目录会撞上"文档已存在" ✓
        //（`new_document` 的既定语义就是拒绝覆盖 ✓）；这正是我第一版写错的地方 ✓。
        let root = temp_dir(&format!("modes_{mode}"));
        let mut workspace = workspace(&root);
        let made = call(
            &mut workspace,
            "texture_background",
            json!({ "texture": "Paper003.png", "mode": mode }),
        );
        assert_eq!(made["ok"], json!(true), "{mode} 应当可用：{made}");
        assert_eq!(made["mode"], json!(mode), "{made}");
        assert_eq!(
            made["region"],
            json!({ "x": 0, "y": 0, "w": 120, "h": 80 }),
            "{mode} 应当铺满整幅：{made}"
        );
        assert_eq!(made["tile_size"]["width"], json!(1024), "{mode}: {made}");
    }
}

/// **未知铺法要列出可用值** ✓；**未知纹理要列出可用的** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn bad_arguments_report_the_available_choices() {
    let root = temp_dir("bad");
    let mut workspace = workspace(&root);
    let bad_mode = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png", "mode": "spiral" }),
    );
    assert_eq!(
        bad_mode["error_code"],
        json!("invalid_argument"),
        "{bad_mode}"
    );
    let detail = bad_mode["context"]["detail"].as_str().unwrap_or_default();
    for expected in ["tile", "stretch", "cover"] {
        assert!(
            detail.contains(expected),
            "要列出可用铺法（缺 {expected}）：{detail}"
        );
    }
    let bad_texture = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "no-such.png" }),
    );
    assert_eq!(
        bad_texture["error_code"],
        json!("reference_not_found"),
        "{bad_texture}"
    );
    let detail = bad_texture["context"]["detail"]
        .as_str()
        .unwrap_or_default();
    assert!(detail.contains("Paper001.png"), "要列出可用纹理：{detail}");
}

/// **"指定了非空图层"的警告必须真的会响** ✓ —— 这条测试的存在有原因 ✓：
/// 我第一版用 `layer["object_count"]` 判"图层里有没有东西" ✗，
/// 而 `list_layers` 给的是 **`objects` 数组** ✓、**没有** `object_count` ✗
/// ⇒ `unwrap_or(0)` ⇒ **警告永远不会触发** ✗ ✓（"加了却永远不生效" ✓）。
/// **判据** ✓：同层已有对象 ⇒ 回应里必须有 `warning` ✓；空图层 ⇒ **不该**有 ✓（免得天天喊狼来了 ✓）。
#[test]
fn the_warning_about_a_non_empty_layer_actually_fires() {
    let root = temp_dir("warn");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(
            &mut workspace,
            "create_layer",
            json!({ "layer_id": "busy" })
        )["ok"],
        json!(true)
    );
    // **先在这个图层上画点东西** ✓ ⇒ 它变成"非空" ✓。
    assert_eq!(
        call(
            &mut workspace,
            "draw_shape",
            json!({
                "layer_id": "busy", "object_id": "s1",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 5, "y": 5, "w": 20, "h": 20}},
                    "color": {"r": 0, "g": 0, "b": 0, "a": 255}
                }
            }),
        )["ok"],
        json!(true)
    );
    let warned = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png", "layer_id": "busy" }),
    );
    assert_eq!(warned["ok"], json!(true), "{warned}");
    let warning = warned["warning"].as_str().unwrap_or_default();
    assert!(
        warning.contains("单独占一层") || warning.contains("自建"),
        "非空图层必须给出警告，否则用户会以为背景跑到下面去了：{warned}"
    );

    // **空图层不该警告** ✓（缺省那条路本来就会自建一层并沉底 ✓）。
    let quiet = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper003.png" }),
    );
    assert_eq!(quiet["ok"], json!(true), "{quiet}");
    assert!(
        quiet.get("warning").is_none(),
        "自建图层这条路不该出现「图层非空」的警告：{quiet}"
    );
}

/// **给了 `region` ⇒ 只铺那一块，而且当"补丁"而不是"背景"** ✓。
///
/// **两种用法的区别必须明确** ✓：整幅铺底会**自建一层并沉到最底** ✓；
/// 而一块补丁**不能**沉底 ✗ —— 否则它一定会盖错东西 ✓。
#[test]
fn a_region_makes_a_patch_that_does_not_get_pushed_to_the_bottom() {
    let root = temp_dir("patch");
    let mut workspace = workspace(&root);
    // 先有一层"作品" ✓，补丁应当**留在它上面** ✓，不该跑到它下面 ✗。
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "art" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png", "region": { "x": 20, "y": 10, "w": 60, "h": 40 } }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        made["is_patch"],
        json!(true),
        "给了 region 就是补丁：{made}"
    );
    assert_eq!(
        made["region"],
        json!({ "x": 20, "y": 10, "w": 60, "h": 40 }),
        "区域要照给的来：{made}"
    );
    let order = layer_order(&mut workspace);
    // **补丁层不能在最底** ✓ —— 那说明它被错误地沉底了 ✓。
    let created = made["created_layer"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!created.is_empty(), "{made}");
    assert_ne!(
        order.first().map(String::as_str),
        Some(created.as_str()),
        "补丁不该被沉到最底：{order:?}"
    );
    assert_eq!(
        order.first().map(String::as_str),
        Some("layer_default"),
        "最底现在是新建文档的默认层，art 在它之上：{order:?}"
    );
}

/// **区域超出画布要夹住** ✓（否则会白白生成一堆画布外的像素 ✗）。
#[test]
fn a_region_larger_than_the_canvas_is_clipped() {
    let root = temp_dir("clip");
    let mut workspace = workspace(&root);
    let made = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png", "region": { "x": 100, "y": 60, "w": 500, "h": 500 } }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // 画布是 120×80 ✓ ⇒ 从 (100,60) 起只剩 20×20 ✓。
    assert_eq!(
        made["region"],
        json!({ "x": 100, "y": 60, "w": 20, "h": 20 }),
        "超出画布的部分要夹掉：{made}"
    );
    assert_eq!(made["is_patch"], json!(true), "{made}");
}

/// **整幅（不给 region）仍然是"背景"** ✓ —— 语义不能被这次改动搅混 ✓。
#[test]
fn without_a_region_it_is_still_a_background() {
    let root = temp_dir("still_bg");
    let mut workspace = workspace(&root);
    let made = call(
        &mut workspace,
        "texture_background",
        json!({ "texture": "Paper001.png" }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        made["is_patch"],
        json!(false),
        "不给 region 就是背景：{made}"
    );
    let created = made["created_layer"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let order = layer_order(&mut workspace);
    assert_eq!(
        order.first().map(String::as_str),
        Some(created.as_str()),
        "背景该在最底：{order:?}"
    );
}
