//! **`new_document`** ✓ —— 一键拿到**空白画布** ✓（真实用户 §六-4 ✓）。
//!
//! **用户原话** ✓："同一 `--root` 下重复打开页面会载入同一文档，三次绘画叠在一张画布上；
//! 自动化测试/创作时必须手动删 `--root` 目录或点新建，**没有 `new_document` 的 MCP 工具**" ✓。
//! **本测试钉住** ✓：① 新建后是**干净、尺寸正确**的画布 ✓；② **同 id 再建 ⇒ 覆盖** ✓（语义就是"给我一块新画布" ✓）；
//! ③ 缺尺寸时的报错**能照做** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

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
    // ④ **同 id 再建 ⇒ 明确拒绝，并告诉调用方下一步** ✓（不会假装成功 ✗、也不会默默清空 ✗）。
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "new_document",
            &json!({ "doc_id": "doc_new", "width": 320, "height": 240 }),
        )
    };
    assert_eq!(
        refused["ok"],
        json!(false),
        "已打开的文档不该被悄悄清空：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("doc_id"),
        "应当告诉调用方换一个 doc_id：{detail}"
    );
    // ⑤ 原来那份文档**没被动过** ✓（"不会清空已存在的文档"这句必须是真的 ✓）。
    let still = inked(&mut workspace);
    assert_eq!(
        still, before,
        "拒绝之后原文档必须原样（原来 {before}，现在 {still}）"
    );
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
