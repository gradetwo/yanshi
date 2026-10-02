//! **`.yanshi` 工程包导出** ✓（真实用户提的缺口 ✓）。
//!
//! **用户原话** ✓："缺少一键将 `atoms.jsonl`、`meta.json` 与 CAS blobs 归档为 `.yanshi`
//! 工程文件的内置命令" ✓。
//!
//! **本测试的验收方式** ✓：导出之后**用系统的 `tar` 解开** ✓，逐项检查 ✓ ——
//! **不用我自己写的解析器去读我自己写的包** ✗（那只是自证 ✓，两边一起错也测不出来 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 造一个**干净的**临时目录 ✓ —— **它会先把同名目录删掉** ✗ ⇒ 一个测试里**只能调一次** ✓。
fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_pack_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// **打出来的包：结构齐全、而且里面的预览是"画的"** ✓。
///
/// **最后一条是与用户那四个包的关键区别** ✓：他手工 zip 时装进去的是**过期的缓存** ✓
///（实测 `render.seq` 5–6 ✓ vs 原子 97–217 ✓）⇒ 四个包的预览**全是空白** ✗。
#[test]
fn a_project_package_contains_the_log_the_blobs_and_a_current_preview() {
    let root = temp_dir("export");
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_pack", 200, 160),
            "human:1",
            "session:test",
        )
        .unwrap();
    // 画点东西 ✓（形状 + 一条介质笔触 ✓ ⇒ 既产生 blob 又产生像素 ✓）
    {
        let mut ctx = ToolContext::new(&mut workspace, "doc_pack", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        let shape = registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 80, "h": 60}},
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        assert_eq!(shape["ok"], json!(true), "{shape}");
        let stroke = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "m1", "medium": "oil",
                    "points": [[20.0, 120.0, 1.0], [100.0, 130.0, 0.8], [170.0, 120.0, 0.6]],
                    "size": 20, "color": {"r": 200, "g": 60, "b": 40, "a": 255}}),
        );
        assert_eq!(stroke["ok"], json!(true), "{stroke}");
    }
    // **注意** ✓：`temp_dir` 会**先把目录整个删掉** ✓ ⇒ **一个测试里只能调一次** ✗ ——
    // 我第一版在这里调了第二次 ✓ ⇒ 把工作区的文件（含 CAS blob）**在导出前删光** ✗
    // ⇒ 导出报"blob 不存在" ✓ ⇒ **看起来像产品的 bug** ✓，其实是**我测试自己的 bug** ✗。
    //（产品那边**严格拒绝缺 blob 的包** ✓ —— 那个行为是对的 ✓：缺 blob 的包重放不出来 ✓。）
    let pack = root.join("doc.yanshi");
    let exported = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_pack", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(
            &mut ctx,
            "export_project",
            &json!({ "path": pack.to_string_lossy() }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert_eq!(
        exported["format"],
        json!("tar (uncompressed)"),
        "{exported}"
    );

    // ① **系统 tar 能列出内容** ✓，而且该有的都有 ✓
    let listed = std::process::Command::new("tar")
        .args(["-tf", pack.to_str().unwrap()])
        .output()
        .expect("系统应当有 tar");
    assert!(
        listed.status.success(),
        "{:?}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let names = String::from_utf8_lossy(&listed.stdout);
    for expected in ["atoms.jsonl", "meta.json", "render.png", "blobs/sha256/"] {
        assert!(names.contains(expected), "包里应当有 {expected}：\n{names}");
    }
    // ② **原子日志能解出来、而且是合法 JSONL** ✓（这是包的**唯一权威** ✓）
    let atoms = std::process::Command::new("tar")
        .args(["-xOf", pack.to_str().unwrap(), "atoms.jsonl"])
        .output()
        .expect("tar 应当能解出");
    assert!(atoms.status.success());
    let lines: Vec<&str> = std::str::from_utf8(&atoms.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        lines.len() >= 4,
        "至少要有建文档/建图层/形状/笔触（实测 {} 行）",
        lines.len()
    );
    for line in &lines {
        serde_json::from_str::<serde_json::Value>(line).expect("每条原子都应当是合法 JSON");
    }
    // ③ **包里的预览"不是空白"** ✓ —— 这正是用户那四个包栽的地方 ✓
    let png = std::process::Command::new("tar")
        .args(["-xOf", pack.to_str().unwrap(), "render.png"])
        .output()
        .expect("tar 应当能解出");
    let (width, height, rgba) =
        yanshi_render::png::decode_png(&png.stdout).expect("应当是合法 PNG");
    assert_eq!((width, height), (200, 160));
    let inked = rgba
        .chunks_exact(4)
        .filter(|pixel| {
            (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                / 1000
                < 200
        })
        .count();
    assert!(
        inked > 500,
        "包里的预览必须**是画出来的图**（实测 {inked} 个暗像素）"
    );
    // ④ **每个被引用的 blob 都在包里** ✓（否则收包的人重放不出来 ✓）
    let mut referenced = 0usize;
    for line in &lines {
        let atom: serde_json::Value = serde_json::from_str(line).unwrap();
        if let Some(blobs) = atom
            .get("refs")
            .and_then(|r| r.get("blobs"))
            .and_then(|b| b.as_array())
        {
            for blob in blobs {
                if let Some(hash) = blob.as_str() {
                    let hex = hash.trim_start_matches("sha256:");
                    let path = format!("blobs/sha256/{}/{}/{}", &hex[0..2], &hex[2..4], hex);
                    assert!(
                        names.contains(&path),
                        "被引用的 blob 必须进包：{path}\n{names}"
                    );
                    referenced += 1;
                }
            }
        }
    }
    assert!(
        referenced > 0,
        "至少要有一条原子引用 blob（否则这个测试没测到东西）"
    );
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&pack);
}

/// **没有落盘工作区时明确拒绝** ✓（而不是导出一个不完整的包 ✗）。
#[test]
fn exporting_from_a_memory_workspace_is_refused_with_a_reason() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_mem", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let refused = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_mem", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(
            &mut ctx,
            "export_project",
            &json!({ "path": "/tmp/never.yanshi" }),
        )
    };
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("--root"), "要说清怎么才能导出：{detail}");
}
