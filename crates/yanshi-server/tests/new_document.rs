//! **`new_document`** ✓ —— 一键拿到**空白画布** ✓（真实用户 §六-4 ✓）。
//!
//! **用户原话** ✓："同一 `--root` 下重复打开页面会载入同一文档，三次绘画叠在一张画布上；
//! 自动化测试/创作时必须手动删 `--root` 目录或点新建，**没有 `new_document` 的 MCP 工具**" ✓。
//! **本测试钉住** ✓：① 新建后是**干净、尺寸正确**的画布 ✓；② **同 id 再建 ⇒ 覆盖** ✓（语义就是"给我一块新画布" ✓）；
//! ③ 缺尺寸时的报错**能照做** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_new", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// **新建 ⇒ 干净、尺寸正确的画布** ✓；**同 id 再建 ⇒ 覆盖** ✓（这正是自动化需要的语义 ✓）。
#[test]
fn a_new_document_is_blank_and_recreating_replaces_it() {
    let mut workspace = workspace();
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "new_document",
            &json!({ "doc_id": "doc_new", "width": 320, "height": 240 }),
        )
    };
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["blank"], json!(true), "{made}");
    // ① 尺寸对 ✓
    let (width, height) = {
        let document = workspace.document("doc_new").expect("文档应当建好");
        (document.state().width, document.state().height)
    };
    assert_eq!((width, height), (320, 240));
    // ② 画一笔 ⇒ 有内容 ✓
    // **先建图层** ✓：新文档是**干净**的 ✓ ⇒ **一个图层都没有** ✓
    //（这条我第一版漏了 ✗ ⇒ 报错正好是上一轮刚富化的那句
    // `"图层 L 不存在（当前没有任何图层）"` ✓ —— **它一眼就告诉了我下一步该做什么** ✓，
    // 这就是 §五-18 想要的效果 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let layer = registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        assert_eq!(layer["ok"], json!(true), "{layer}");
    }
    {
        let mut ctx = context(&mut workspace);
        let drawn = registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 60, "h": 40}},
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let inked = |workspace: &mut Workspace| -> usize {
        let (_, _, pixels) = workspace
            .document_mut("doc_new")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 320.0, 240.0))
            .expect("区域渲染应成功");
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    let before = inked(&mut workspace);
    assert!(before > 100, "画完应当有墨（实测 {before}）");
    // ③ **换一个 id ⇒ 一块干净画布** ✓（这正是"新画布"的正解：文档 id 就是持久单元 ✓）
    {
        let mut ctx = context(&mut workspace);
        let fresh = registry().call(
            &mut ctx,
            "new_document",
            &json!({ "doc_id": "doc_new_2", "width": 320, "height": 240 }),
        );
        assert_eq!(fresh["ok"], json!(true), "{fresh}");
    }
    let fresh_ink = {
        let (_, _, pixels) = workspace
            .document_mut("doc_new_2")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 320.0, 240.0))
            .expect("区域渲染应成功");
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    assert_eq!(
        fresh_ink, 0,
        "新 id 的文档应当是干净画布（实测 {fresh_ink} 个墨点）"
    );
    // ④ **同 id 再建 ⇒ 语义是"打开它"** ✓（第三方 MCP 报告 P0-2 ✓：否则多轮续画走不通 ✗）；
    //    **安全约束不变** ✓：打开**绝不清空**已有内容 ✓（见 ⑤）。
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "new_document",
            &json!({ "doc_id": "doc_new", "width": 320, "height": 240 }),
        )
    };
    assert_eq!(refused["ok"], json!(true), "同 id 应当可打开：{refused}");
    assert_eq!(refused["opened"], json!(true), "应当标记为打开：{refused}");
    assert_eq!(
        refused["created"],
        json!(false),
        "不该被当成新建：{refused}"
    );
    let note = refused["note"].as_str().unwrap_or_default();
    assert!(
        note.contains("打开") && note.contains("doc_id"),
        "应当说明这是打开、以及要新画布就换 doc_id：{note}"
    );
    // ⑤ 原来那份文档**没被动过** ✓（"不会清空已存在的文档"这句必须是真的 ✓）。
    let still = inked(&mut workspace);
    assert_eq!(
        still, before,
        "打开之后原文档必须原样（原来 {before}，现在 {still}）"
    );
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_newdoc_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// **"已存在"包括"磁盘上有、此刻没打开"** ✓ —— 工具自己的存在判据 ✓（真实缺陷 ✓ 2026-10-06 ✓）。
///
/// **为什么单列一条** ✗：入口（MCP ✓）现在**不再**替"自建文档"的工具预建目标 ✓
/// ⇒ 工具若只用 `document_mut`（**只在内存里找** ✗）判断"是否存在" ✓，
/// 就会漏掉"磁盘上有、这次进程还没打开" ✓ ⇒ 转去 `create_document` ✓
/// 而它在磁盘那一层**拒绝** ✗ ⇒ 用户拿到"已存在于磁盘"的错误 ✗，
/// 而不是"这次是打开、内容没动" ✓。
///
/// **安全约束** ✓：本工具**绝不清空、绝不改尺寸** ✓ —— 请求里给**不同**的尺寸也必须原样不动 ✓
///（"给我一块干净画布"请换新 doc_id ✓）。这正是判据 `import_project` 那条
/// "只导入、绝不覆盖"的同一件事 ✓。
#[test]
fn an_on_disk_document_is_opened_and_never_resized() {
    let root = temp_dir("disk");
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).expect("落盘工作区");
    workspace
        .create_document(
            NewDocument::new("doc_disk", 320, 240),
            "human:1",
            "session:test",
        )
        .expect("建文档");
    // 画一笔 ⇒ 有像素 ✓（"内容没被清空"要拿像素说话 ✓，不能只看 ok ✓）。
    for (tool, args) in [
        ("create_layer", json!({"layer_id": "L"})),
        (
            "draw_shape",
            json!({"layer_id": "L", "object_id": "s1",
                   "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 60, "h": 40}},
                            "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        ),
    ] {
        let mut ctx = ToolContext::new(&mut workspace, "doc_disk", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let value = registry().call(&mut ctx, tool, &args);
        assert_eq!(value["ok"], json!(true), "{tool}: {value}");
    }
    let inked = |workspace: &mut Workspace| -> usize {
        let (_, _, pixels) = workspace
            .document_mut("doc_disk")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 320.0, 240.0))
            .expect("区域渲染");
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    let before = inked(&mut workspace);
    assert!(before > 100, "画完应当有墨（实测 {before}）");
    // **把它从内存里放下** ✓ —— 磁盘上还在 ✓ ⇒ 正是"有文件、没打开"那种状态 ✓。
    assert!(workspace.close_document("doc_disk"));
    assert!(workspace.document("doc_disk").is_none(), "此刻不该在内存里");

    // **同 id 再建** ✓，而且**故意给不同的尺寸** ✓（安全约束：不许改尺寸 ✓）。
    let opened = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_disk", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_disk", "width": 640, "height": 480}),
        )
    };
    assert_eq!(
        opened["ok"],
        json!(true),
        "磁盘上已存在 ⇒ 应当是打开：{opened}"
    );
    assert_eq!(opened["opened"], json!(true), "应当标记为打开：{opened}");
    assert_eq!(opened["created"], json!(false), "不该被当成新建：{opened}");
    assert_eq!(
        (opened["width"].as_u64(), opened["height"].as_u64()),
        (Some(320), Some(240)),
        "已存在的文档**不许被改成请求里的 640×480**：{opened}"
    );
    // 磁盘上的 meta.json 也必须还是 320×240 ✓（"没动过"要落盘也成立 ✓）。
    let meta: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("docs").join("doc_disk").join("meta.json"))
            .expect("meta.json"),
    )
    .expect("meta.json 是 JSON");
    assert_eq!(
        (meta["width"].as_u64(), meta["height"].as_u64()),
        (Some(320), Some(240))
    );
    // 像素与尺寸**原样** ✓。
    let still = inked(&mut workspace);
    assert_eq!(
        still, before,
        "打开之后原文档必须原样（原来 {before}，现在 {still}）"
    );
    let state_size = {
        let document = workspace.document("doc_disk").expect("已打开");
        (document.state().width, document.state().height)
    };
    assert_eq!(state_size, (320, 240), "尺寸不许变");
    let _ = std::fs::remove_dir_all(&root);
}

/// **缺尺寸时的报错要能照做** ✓（本轮刚给"不存在"加过可用值 ✓，同一个道理 ✓）。
#[test]
fn missing_dimensions_are_refused_with_an_example() {
    let mut workspace = workspace();
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        ("缺宽", json!({ "doc_id": "d1", "height": 100 }), "width"),
        ("缺高", json!({ "doc_id": "d1", "width": 100 }), "height"),
    ];
    for (label, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "new_document", &args)
        };
        assert_eq!(
            refused["ok"],
            json!(false),
            "「{label}」应当被拒：{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "「{label}」的原因应当提到「{expected}」并给例子，实测：{detail}"
        );
    }
}
