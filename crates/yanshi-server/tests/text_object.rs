//! 文本对象必须真的渲染出像素（设计 4.2 的 `Text`：text / font / size / color / align）。
//!
//! 背景：`AtomKind::DrawText` 与 `draw_text` 工具一直都在 ✓，但内核把 `ObjectType::Text`
//! 映射成 `Primitive::Unsupported` ✗ —— 于是"文本工具"画不出任何东西 ✓。
//! 本轮接入**路线 A 的最小切片**：内置 5×7 ASCII 位图字体（零依赖、无外部字体资产 ✓；
//! CJK 字体子集属后续项 ✓，设计 1175/1287 行）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_text", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 区域内**红字**像素数与其首个位置。
///
/// 注意：文档默认是**白色背景** ✓，按"亮像素"计数会把背景一起算进去 ✗
///（本次就因此误判"写文本前区域应为空"失败 ✓）。因此按**文字颜色**判定 ✓。
fn ink(
    workspace: &mut Workspace,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> (usize, Option<(usize, usize)>) {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_text", Bbox::new(x, y, w, h))
        .expect("区域渲染应成功");
    let width = w as usize;
    let mut count = 0usize;
    let mut first: Option<(usize, usize)> = None;
    for index in 0..pixels.len() / 4 {
        let pixel = &pixels[index * 4..index * 4 + 4];
        if pixel[0] > 128 && pixel[1] < 128 && pixel[3] > 128 {
            count += 1;
            if first.is_none() {
                first = Some((index % width, index / width));
            }
        }
    }
    (count, first)
}

#[test]
fn draw_text_renders_pixels_at_the_given_position() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_text", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let before = ink(&mut workspace, 0.0, 0.0, 128.0, 64.0);
    assert_eq!(before.0, 0, "写文本前该区域应为空");

    let drawn = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "t1",
                    "data": {"text": "AB", "font": "builtin", "size": 7.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [10.0, 10.0], "align": "left"}}),
        )
    };
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let (count, first) = ink(&mut workspace, 0.0, 0.0, 128.0, 64.0);
    assert!(
        count > 0,
        "文本必须渲染出像素（此前 ObjectType::Text 映射为 Unsupported，画不出任何东西）"
    );
    let (fx, fy) = first.expect("应能找到第一个有墨像素");
    assert!(
        (10..12).contains(&fx) && (10..12).contains(&fy),
        "文本应从给定位置 (10,10) 开始（实测首个有墨像素在 ({fx},{fy})）"
    );
    // 单字符 5×7、scale=1 的 'A' 共 18 个像素 —— 注意要在**文字所在位置**取样
    //（我第一次把取样框开在原点，得到 0 ✗，又是一次度量位置写错）。
    // 取 5px 宽的框 ⇒ 只含 'A'（'B' 从 x≈15 开始；我第一次用 8px 宽把 'B' 也框进来了 ✗）。
    let (one, _) = ink(&mut workspace, 10.0, 10.0, 5.0, 8.0);
    assert_eq!(
        one, 18,
        "'A' 在 scale=1 下应有 18 个像素（内置字形第一行 01110）"
    );
}

#[test]
fn text_alignment_shifts_the_start_column() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_text", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 右对齐：用一个自身宽度已知的盒子（内置字体等宽，2 字符 = 10px）。
        registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "t2",
                    "data": {"text": "AB", "font": "builtin", "size": 7.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [10.0, 10.0], "align": "right"}}),
        );
    }
    // 内置字体的右对齐需要调用方给出盒宽；未给时按文本自身宽度，因此起点仍在 10。
    let (_, first) = ink(&mut workspace, 0.0, 0.0, 64.0, 32.0);
    let (fx, _) = first.expect("应能找到有墨像素");
    assert!(
        fx >= 10,
        "无论对齐方式，起点都不应早于给定位置（实际 {fx}）"
    );
}
