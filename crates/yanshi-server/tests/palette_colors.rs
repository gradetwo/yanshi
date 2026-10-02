//! **调色板取色** ✓（目标的第 ① 件 ✓）。
//!
//! **两种来源都要测** ✓：内置的（`assets/palettes` ✓ —— 我们真会发出去的那批 ✓）
//! 与工作区缓存里的（用户导入的 ✓）；**解析器要宽容但绝不骗人** ✗ ——
//! 认不出的行跳过 ✓，可整份都认不出时必须**明确报错** ✓（而不是回一个空表让人以为"这个板是空的" ✗）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_palette_{}_{}", name, std::process::id()));
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
            NewDocument::new("doc_pal", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_pal", "human:1", "session:test");
    registry().call(&mut ctx, "list_palette_colors", &args)
}

/// **内置的 Open Colors 必须读全** ✓ —— **132 色** ✓，而且抽样对得上 ✓（白与黑 ✓）。
#[test]
fn the_bundled_open_colors_palette_reads_all_its_colors() {
    let root = temp_dir("open_color");
    let mut workspace = workspace(&root);
    let got = call(
        &mut workspace,
        json!({ "palette": "open-color.json", "limit": 0 }),
    );
    assert_eq!(got["ok"], json!(true), "{got}");
    assert_eq!(
        got["total"],
        json!(132),
        "Open Colors 是 15 系 × 10 阶 + 黑白：{got}"
    );
    assert_eq!(got["truncated"], json!(false), "{got}");
    let colors = got["colors"].as_array().unwrap();
    let find = |hex: &str| colors.iter().find(|c| c["hex"] == json!(hex)).cloned();
    assert!(find("#ffffff").is_some(), "应当有白色：{got}");
    let black = find("#000000").expect("应当有黑色");
    // **名字要带上色系** ✓（同一色系十阶 ✓ ⇒ 只给键名会十条同名 ✗）。
    let name = black["name"].as_str().unwrap_or_default();
    assert!(name.contains("black"), "黑色应当有名字：{name}");
    // **十六进制与分量必须自洽** ✓（界面直接把它写进取色器 ✓，错了就是错色 ✗）。
    let red = colors
        .iter()
        .find(|c| c["hex"] == json!("#fa5252"))
        .expect("应当有 fa5252");
    assert_eq!(red["r"], json!(250), "{red}");
    assert_eq!(red["g"], json!(82), "{red}");
    assert_eq!(red["b"], json!(82), "{red}");
}

/// **一份 `.gpl`** ✓：标准 `R G B [名字]` ✓、注释与 `Name:` 行要跳过 ✓。
#[test]
fn a_gimp_palette_parses_with_names_and_skips_metadata() {
    let root = temp_dir("gpl");
    let cache = root.join("palettes");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(
        cache.join("mine.gpl"),
        "GIMP Palette\nName: 手写板\n#\n# 注释也要跳过\n255   0   0\t红\n  0 128 255   海蓝\n\n不是颜色的一行\n",
    )
    .unwrap();
    let mut workspace = workspace(&root);
    let got = call(&mut workspace, json!({ "palette": "mine.gpl", "limit": 0 }));
    assert_eq!(got["ok"], json!(true), "{got}");
    assert_eq!(got["total"], json!(2), "只有两行是颜色：{got}");
    let colors = got["colors"].as_array().unwrap();
    assert_eq!(
        colors[0],
        json!({ "r": 255, "g": 0, "b": 0, "hex": "#ff0000", "name": "红" })
    );
    assert_eq!(colors[1]["hex"], json!("#0080ff"), "{got}");
    assert_eq!(colors[1]["name"], json!("海蓝"), "多词名字要拼回来：{got}");
}

/// **内置那 41 套 sK1 调色板都得能读** ✓（它们是随包发布的 ✓ ⇒ 读不了就是发了个坏资产 ✗）。
#[test]
fn every_bundled_sk1_palette_parses() {
    let root = temp_dir("sk1");
    let mut workspace = workspace(&root);
    let listed = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_pal", "human:1", "session:test");
        registry().call(&mut ctx, "list_assets", &json!({ "kind": "palette" }))
    };
    let names: Vec<String> = listed["assets"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| {
            entry["usable"] == json!(true)
                && entry["name"].as_str().unwrap_or_default().ends_with(".gpl")
        })
        .map(|entry| entry["name"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        names.len() >= 41,
        "应当有 41 套 sK1 调色板：{}",
        names.len()
    );
    for name in &names {
        let got = call(&mut workspace, json!({ "palette": name, "limit": 3 }));
        assert_eq!(got["ok"], json!(true), "{name} 读不了：{got}");
        assert!(
            got["total"].as_u64().unwrap_or(0) > 0,
            "{name} 一个颜色都没解析出来：{got}"
        );
    }
}

/// **截断必须在响应里说清** ✗（"给了一半却不说"会让调用方以为板就那么大 ✓）。
#[test]
fn truncation_is_reported_rather_than_hidden() {
    let root = temp_dir("limit");
    let mut workspace = workspace(&root);
    let got = call(
        &mut workspace,
        json!({ "palette": "open-color.json", "limit": 10 }),
    );
    assert_eq!(got["count"], json!(10), "{got}");
    assert_eq!(got["total"], json!(132), "{got}");
    assert_eq!(got["truncated"], json!(true), "{got}");
    assert!(
        got["hint"].as_str().unwrap_or_default().contains("limit"),
        "要告诉调用方怎么办：{got}"
    );
}

/// **认不出的文件要明确报错** ✓（不能回一个空表 ✓ —— 那会让人以为"这个板是空的" ✗）。
#[test]
fn a_file_that_is_not_a_palette_is_reported_not_treated_as_empty() {
    let root = temp_dir("garbage");
    let cache = root.join("palettes");
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("junk.gpl"), "这根本不是调色板\n随便写点什么\n").unwrap();
    std::fs::write(cache.join("broken.json"), "{ 这不是 JSON").unwrap();
    let mut workspace = workspace(&root);
    for (name, expected) in [
        ("junk.gpl", "precondition_failed"),
        ("broken.json", "invalid_argument"),
    ] {
        let got = call(&mut workspace, json!({ "palette": name, "limit": 0 }));
        assert_eq!(got["ok"], json!(false), "{name} 应当报错：{got}");
        assert_eq!(got["error_code"], json!(expected), "{name}: {got}");
    }
}

/// **找不到调色板时列出可用的** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn an_unknown_palette_lists_the_available_ones() {
    let root = temp_dir("unknown");
    let mut workspace = workspace(&root);
    let got = call(&mut workspace, json!({ "palette": "nope", "limit": 0 }));
    assert_eq!(got["ok"], json!(false), "{got}");
    assert_eq!(got["error_code"], json!("reference_not_found"), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("open-color.json"),
        "要列出可用的调色板：{detail}"
    );
}
