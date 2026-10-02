//! **工程包导入** ✓（与 `export_project` 配对 ✓ —— 用户当初报的正是"没有导入导出工程" ✓）。
//!
//! **本测试的核心是"往返之后像素一致"** ✓：导出 ⇒ 导入到**另一个工作区** ⇒ 两边各渲染一次 ⇒
//! **逐字节比对** ✓。**为什么用像素而不是"导入成功了"** ✗：
//! 一个只回了 `ok: true` 而内容其实丢了的导入 ✓ 与"接受了却没用"是同一类病 ✓
//!（用户当初手工 zip 就得到过"预览全空白"的包 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_imp_{}_{}", name, std::process::id()));
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
            NewDocument::new("doc_src", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_src", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **导出 ⇒ 导入 ⇒ 两边渲染逐字节相同** ✓（本测试的全部价值在这里 ✓）。
#[test]
fn a_project_package_round_trips_to_identical_pixels() {
    let root_a = temp_dir("round_a");
    let mut source = workspace(&root_a);
    let pack = root_a.join("pack.yanshi");
    // 画两种东西 ✓：形状（矢量 ✓）+ 介质笔触（会产生 blob ✓，正是要还原的那部分 ✓）。
    assert_eq!(
        call(&mut source, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    assert_eq!(
        call(
            &mut source,
            "draw_shape",
            json!({
                "layer_id": "L", "object_id": "rect",
                "data": {
                    "geometry": { "kind": "rect", "bbox": { "x": 20, "y": 20, "w": 100, "h": 60 } },
                    "color": { "r": 220, "g": 60, "b": 40, "a": 255 }
                }
            }),
        )["ok"],
        json!(true)
    );
    let stroked = call(
        &mut source,
        "medium_stroke",
        json!({
            "layer_id": "L", "object_id": "stroke", "medium": "oil",
            "points": [[20.0, 95.0, 0.7], [140.0, 105.0, 0.7]], "size": 18
        }),
    );
    assert_eq!(
        stroked["ok"],
        json!(true),
        "介质笔触应当成功（它会产生 blob）：{stroked}"
    );
    let exported = call(
        &mut source,
        "export_project",
        json!({ "path": pack.display().to_string() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let source_png = root_a.join("source.png");
    assert_eq!(
        call(
            &mut source,
            "export_png",
            json!({ "path": source_png.display().to_string() })
        )["ok"],
        json!(true)
    );

    // **导入到一个全新的工作区** ✓（另一台机器的等价物 ✓）。
    let root_b = temp_dir("round_b");
    let mut target = workspace(&root_b);
    let imported = call(
        &mut target,
        "import_project",
        json!({ "path": pack.display().to_string(), "doc_id": "doc_copy" }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    assert_eq!(imported["doc_id"], json!("doc_copy"), "{imported}");
    assert!(
        imported["blobs"].as_u64().unwrap_or(0) > 0,
        "应当还原出 blob（介质笔触的像素就在里面）：{imported}"
    );

    // **两边各渲染一次，逐字节比** ✓。
    let copied_png = root_b.join("copied.png");
    let mut ctx = ToolContext::new(&mut target, "doc_copy", "human:1", "session:test");
    let rendered = registry().call(
        &mut ctx,
        "export_png",
        &json!({ "path": copied_png.display().to_string() }),
    );
    assert_eq!(
        rendered["ok"],
        json!(true),
        "导入的文档应当能渲染：{rendered}"
    );
    let left = yanshi_render::png::decode_png(&std::fs::read(&source_png).unwrap()).unwrap();
    let right = yanshi_render::png::decode_png(&std::fs::read(&copied_png).unwrap()).unwrap();
    assert_eq!(left.0, right.0, "宽应当一致");
    assert_eq!(left.1, right.1, "高应当一致");
    assert_eq!(
        left.2, right.2,
        "**往返之后像素必须逐字节一致** ✗ —— 不一致说明导入丢了东西（这是本测试存在的理由 ✓）"
    );
}

/// **绝不覆盖已存在的文档** ✗（不可逆的事不做 ✓）。
#[test]
fn importing_never_overwrites_an_existing_document() {
    let root = temp_dir("no_overwrite");
    let mut workspace = workspace(&root);
    let pack = root.join("pack.yanshi");
    assert_eq!(
        call(
            &mut workspace,
            "export_project",
            json!({ "path": pack.display().to_string() })
        )["ok"],
        json!(true)
    );
    // `doc_src` 本来就存在 ✓ ⇒ 用同一个 id 导入必须被拒 ✓。
    let refused = call(
        &mut workspace,
        "import_project",
        json!({ "path": pack.display().to_string(), "doc_id": "doc_src" }),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert_eq!(refused["error_code"], json!("conflict"), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("不会覆盖"), "要说清是不会覆盖：{detail}");
}

/// **坏掉的包要明确拒绝** ✗ —— 校验和就是为这个存在的 ✓。
#[test]
fn a_corrupted_package_is_refused_with_a_checksum_reason() {
    let root = temp_dir("corrupt");
    let mut workspace = workspace(&root);
    let pack = root.join("pack.yanshi");
    assert_eq!(
        call(
            &mut workspace,
            "export_project",
            json!({ "path": pack.display().to_string() })
        )["ok"],
        json!(true)
    );
    // **改动头部的字节** ✓（把第 4 个字节翻一下 ✓ —— 一定落在第一个文件的字段里 ✓）。
    let mut bytes = std::fs::read(&pack).unwrap();
    bytes[4] ^= 0xff;
    let broken = root.join("broken.yanshi");
    std::fs::write(&broken, &bytes).unwrap();
    let refused = call(
        &mut workspace,
        "import_project",
        json!({ "path": broken.display().to_string(), "doc_id": "doc_broken" }),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("校验和"), "要说清是校验和不对：{detail}");
}

/// **不是工程包 ⇒ 说清里面有什么** ✓（"缺东西"要能照着查 ✓）。
#[test]
fn something_that_is_not_a_project_package_says_what_it_contains() {
    let root = temp_dir("not_a_pack");
    let mut workspace = workspace(&root);
    // **自己造一个 tar** ✓：只有一个说明文件 ✓ ⇒ 缺 atoms.jsonl ✓。
    let tar = yanshi_server::archive::write_tar(&[yanshi_server::archive::TarEntry {
        path: "README.txt".to_string(),
        bytes: b"not a project".to_vec(),
    }]);
    let path = root.join("wrong.yanshi");
    std::fs::write(&path, &tar).unwrap();
    let refused = call(
        &mut workspace,
        "import_project",
        json!({ "path": path.display().to_string(), "doc_id": "doc_wrong" }),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("atoms.jsonl"), "要说清缺的是它：{detail}");
    assert!(
        detail.contains("README.txt"),
        "还要说清里面有什么：{detail}"
    );
}

/// **blob 的内容必须与它在包里的哈希一致** ✗（"看起来导进去了、其实字节坏了"最糟 ✓）。
#[test]
fn a_blob_whose_content_does_not_match_its_hash_is_refused() {
    let root = temp_dir("bad_blob");
    let mut workspace = workspace(&root);
    let tar = yanshi_server::archive::write_tar(&[
        yanshi_server::archive::TarEntry {
            path: "atoms.jsonl".to_string(),
            bytes: b"{\"seq\":1}\n".to_vec(),
        },
        yanshi_server::archive::TarEntry {
            // 路径写着"全零"的哈希 ✓，但内容不是它 ✓ ⇒ 必须被拒 ✓。
            path: format!("blobs/sha256/00/00/{}", "0".repeat(64)),
            bytes: b"this is not the content of that hash".to_vec(),
        },
    ]);
    let path = root.join("bad_blob.yanshi");
    std::fs::write(&path, &tar).unwrap();
    let refused = call(
        &mut workspace,
        "import_project",
        json!({ "path": path.display().to_string(), "doc_id": "doc_bad_blob" }),
    );
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("blob"), "要说清是 blob 坏了：{detail}");
    // **而且不许留下半成品** ✓（文档根本没有被建出来 ✓）。
    assert!(
        !root.join("docs/doc_bad_blob").exists(),
        "被拒的导入不该留下文档目录 ✗"
    );
}
