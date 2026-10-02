//! **导入资产** ✓（用户明确要求："**Web 与 MCP 都要能导入**笔刷、纹理、调色板" ✓）。
//!
//! **两条来源** ✓，因为两个调用方的能力不同 ✓：`path`（服务器本地文件 ✓，MCP 顺手 ✓）
//! 与 `blob`（浏览器先上传再用 ✓ —— 它拿不到服务器路径 ✓）。
//! **只做一条路会有一边用不了** ✗ ⇒ 两条都要 ✓，且都汇进**同一个内核方法** ✓
//!（`Workspace::import_asset` ✓）⇒ 校验、目录、覆盖语义**只有一份** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_asset_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(NewDocument::new("doc_a", 32, 32), "human:1", "session:test")
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_a", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **三类资产都能从本地路径导入** ✓，而且**导完立刻能列出来** ✓（闭环 ✓）。
#[test]
fn all_three_kinds_import_from_a_path_and_then_list() {
    let root = temp_dir("three");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("charcoal.myb"), b"{\"name\": \"charcoal\"}").unwrap();
    std::fs::write(source.join("paper.png"), b"\x89PNG\r\n\x1a\nfake").unwrap();
    std::fs::write(source.join("open-color.kpl"), b"GIMP Palette\n").unwrap();
    let mut workspace = workspace(&root);

    for (kind, file) in [
        ("brush", "charcoal.myb"),
        ("texture", "paper.png"),
        ("palette", "open-color.kpl"),
    ] {
        let path = source.join(file).display().to_string();
        let got = call(
            &mut workspace,
            "import_asset",
            json!({ "kind": kind, "name": file, "path": path }),
        );
        assert_eq!(got["ok"], json!(true), "导入 {kind} 应当成功：{got}");
        assert_eq!(got["name"], json!(file), "{got}");
        // **立刻能列出来** ✓ —— 否则"导入成功"对调用方毫无意义 ✓。
        let listed = call(&mut workspace, "list_assets", json!({ "kind": kind }));
        let names: Vec<String> = listed["assets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["name"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert!(
            names.iter().any(|name| name == file),
            "{kind} 列表里应当有 {file}：{names:?}"
        );
        assert_eq!(listed["can_import"], json!(true), "{listed}");
    }
}

/// **扩展名必须与种类相符** ✗（否则就是"导入了用不了的东西" ✓ —— 与"接受了却没用"同类 ✓）。
#[test]
fn the_extension_must_match_the_kind() {
    let root = temp_dir("ext");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("photo.jpg"), b"jpeg").unwrap();
    std::fs::write(source.join("brush.png"), b"png").unwrap();
    let mut workspace = workspace(&root);

    // **纹理只收 PNG** ✗ —— 像素解码器是自己写的 ✓、只解 PNG ✓
    //（JPEG/WebP 会被明确拒绝 ✓）⇒ 收进来用不了的东西是**骗人** ✓。
    let got = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "texture", "name": "photo.jpg", "path": source.join("photo.jpg").display().to_string() }),
    );
    assert_eq!(got["ok"], json!(false), "{got}");
    assert_eq!(got["error_code"], json!("invalid_argument"), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("png"), "要把允许的扩展名说出来：{detail}");

    // 笔刷同理 ✓（只收 `.myb` ✓）。
    let got = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "brush", "name": "brush.png", "path": source.join("brush.png").display().to_string() }),
    );
    assert_eq!(got["error_code"], json!("invalid_argument"), "{got}");
}

/// **默认不覆盖** ✗；要覆盖必须**明说** ✓ —— 静默覆盖会毁掉用户已有的资产 ✓。
#[test]
fn importing_twice_conflicts_unless_overwrite_is_asked_for() {
    let root = temp_dir("overwrite");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("a.png"), b"first").unwrap();
    std::fs::write(source.join("b.png"), b"second").unwrap();
    let mut workspace = workspace(&root);
    let first = source.join("a.png").display().to_string();
    assert_eq!(
        call(
            &mut workspace,
            "import_asset",
            json!({ "kind": "texture", "name": "same.png", "path": first })
        )["ok"],
        json!(true)
    );
    let second = source.join("b.png").display().to_string();
    let refused = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "texture", "name": "same.png", "path": second }),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(refused["error_code"], json!("conflict"), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("overwrite"), "要说清怎么覆盖：{detail}");
    // **明说之后就允许** ✓。
    let replaced = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "texture", "name": "same.png", "path": second, "overwrite": true }),
    );
    assert_eq!(replaced["ok"], json!(true), "{replaced}");
    let cached = std::fs::read(root.join("textures/same.png")).unwrap();
    assert_eq!(cached, b"second", "覆盖之后内容应当是新的 ✓");
}

/// **文件名不许带路径** ✗（挡 `..` 与分隔符 ✓）。
#[test]
fn a_name_cannot_escape_the_asset_directory() {
    let root = temp_dir("name");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("x.png"), b"x").unwrap();
    let mut workspace = workspace(&root);
    let path = source.join("x.png").display().to_string();
    for bad in ["../evil.png", "sub/evil.png", ".hidden.png"] {
        let got = call(
            &mut workspace,
            "import_asset",
            json!({ "kind": "texture", "name": bad, "path": path }),
        );
        assert_eq!(got["ok"], json!(false), "{bad} 应当被拒：{got}");
        assert_eq!(got["error_code"], json!("invalid_argument"), "{bad}: {got}");
    }
    assert!(!root.join("evil.png").exists(), "不许写到目录之外 ✗");
}

/// **未知种类要列出可用值** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn an_unknown_kind_lists_the_supported_ones() {
    let root = temp_dir("kind");
    let mut workspace = workspace(&root);
    let got = call(&mut workspace, "list_assets", json!({ "kind": "sticker" }));
    assert_eq!(got["ok"], json!(false), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    for expected in ["brush", "texture", "palette"] {
        assert!(
            detail.contains(expected),
            "要列出可用种类（缺 {expected}）：{detail}"
        );
    }
}

/// **随发行包发布的资产必须真的可用** ✓ —— 这条守卫如果早存在 ✓，
/// `Paper003.png`（灰度 PNG ✓ 被报成可用 ✗，导入时被解码器拒绝 ✗）**当时就会红** ✓。
///
/// **它读的是仓库里的 `assets/`** ✓（不是临时目录 ✓）⇒ 测的是**我们真正会发出去的东西** ✓。
/// 判据只针对**该类的主扩展名** ✓（`.png` / `.myb` / `.gpl` / `.json` ✓）——
/// 同目录的 `NOTICE.md`、`LICENSE-CC0.txt` 是**说明文件** ✓，不该被当成资产 ✗。
#[test]
fn every_bundled_asset_is_actually_usable() {
    let root = temp_dir("bundled_usable");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = workspace(&root).with_assets_dir(Some(assets));

    for (kind, primary) in [("texture", ".png"), ("brush", ".myb"), ("palette", ".gpl")] {
        let got = call(&mut workspace, "list_assets", json!({ "kind": kind }));
        assert_eq!(got["ok"], json!(true), "{got}");
        let entries = got["assets"].as_array().cloned().unwrap_or_default();
        let primary_entries: Vec<(String, bool)> = entries
            .iter()
            .filter(|entry| {
                entry["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .ends_with(primary)
            })
            .map(|entry| {
                (
                    entry["name"].as_str().unwrap_or_default().to_owned(),
                    entry["usable"] == json!(true),
                )
            })
            .collect();
        assert!(
            !primary_entries.is_empty(),
            "{kind} 一个 {primary} 都没有 ⇒ 守卫没测到东西 ✗"
        );
        let unusable: Vec<&String> = primary_entries
            .iter()
            .filter(|(_, usable)| !*usable)
            .map(|(name, _)| name)
            .collect();
        assert!(
            unusable.is_empty(),
            "{kind} 里有报成不可用的 {primary}：{unusable:?} ⇒ 会引导调用方去导一个必然失败的文件 ✗"
        );
    }
}
