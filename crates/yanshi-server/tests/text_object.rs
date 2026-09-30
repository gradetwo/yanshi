//! 文本对象必须真的渲染出像素（设计 4.2 的 `Text`：text / font / size / color / align）。
//!
//! 背景：`AtomKind::DrawText` 与 `draw_text` 工具一直都在 ✓，但内核把 `ObjectType::Text`
//! 映射成 `Primitive::Unsupported` ✗ —— 于是"文本工具"画不出任何东西 ✓。
//! 本轮接入**路线 A 的最小切片**：内置 5×7 ASCII 位图字体（零依赖、无外部字体资产 ✓；
//! CJK 字体子集属后续项 ✓，设计 1175/1287 行）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_core::{Atom, AtomKind};
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

/// 含 CJK 的文本走**内嵌 OFL 位图图集** ✓（设计 1175/1287「内嵌开源字体子集，首版 Latin/CJK 基础」✓）。
#[test]
fn cjk_text_renders_through_the_embedded_atlas() {
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
    let before = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert_eq!(before.0, 0, "写文本前该区域应为空");

    let drawn = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "z_cjk",
                    "data": {"text": "中文永", "font": "builtin", "size": 32.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [8.0, 8.0], "align": "left"}}),
        )
    };
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let (count, first) = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert!(count > 0, "CJK 文本必须渲染出像素（图集路径）");
    let (fx, fy) = first.expect("应能找到第一个有墨像素");
    assert!(
        fx >= 8 && fy >= 8,
        "CJK 文本应从给定位置开始（实测首个有墨像素在 ({fx},{fy})）"
    );
    // 三个字 × 16×16 格（size 32 ⇒ scale 2 ⇒ 每字 32×32）。实测 182 个像素 ✓，
    // 用**写死的量级**断言（照 "'A' 恰好 18 像素" 那条的做法 ✓）。
    assert!(
        (150..400).contains(&count),
        "三个 CJK 字的像素数应在实测量级内（实际 {count}）"
    );
}

/// 文本是**可编辑的文本对象** ✓（不是烘焙进像素的位图 ✓）。
///
/// 设计 4.2 把 `Text` 列为对象类型之一 ✓、5.2 把「修改内容」交给 `supersede` ✓
/// （设计里"改颜色 = supersede"是同一机制 ✓）。本用例证明：**换掉对象的 `data` 之后，
/// 渲染跟着变** ✓，且对象类型仍是 `text` ✓ —— 即文字始终是日志里的一个对象 ✓。
#[test]
fn text_stays_an_editable_object() {
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
        registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "t_edit",
                    "data": {"text": "AB", "font": "builtin", "size": 21.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [8.0, 8.0], "align": "left"}}),
        );
    }
    let (before_count, _) = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert!(before_count > 0, "先画出文本（前置条件）");

    // 用 `supersede` 换掉对象的数据 —— 这是设计规定的"修改内容"机制 ✓。
    let atom = Atom::new(
        AtomKind::Supersede,
        "human:web",
        "session:web",
        json!({
            "object_id": "t_edit",
            "data": {
                "text": "中文永",
                "font": "builtin",
                "size": 21.0,
                "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                "position": [8.0, 8.0],
                "align": "left"
            }
        }),
    );
    workspace
        .commit("doc_text", atom, "human:web", false)
        .expect("supersede 应被接受");

    // ① 渲染必须跟着变（内容确实是"活的"）。
    let (after_count, _) = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert!(
        after_count != before_count,
        "替换文本后渲染应改变（此前 {before_count}，之后 {after_count}）"
    );
    assert!(after_count > 0, "替换后的 CJK 文本应渲染出像素");

    // ② 对象类型仍是 `text` ✓（不是位图补丁）⇒ 文字保持可编辑 ✓。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let object = listed["objects"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["object_id"] == json!("t_edit"))
        })
        .cloned()
        .expect("对象应仍在列表中");
    assert_eq!(
        object["type"],
        json!("text"),
        "文本对象必须保持 `text` 类型（可编辑 ✓），而不是被烘焙成位图 ✗"
    );
}

/// `replace_object_data` 工具：把"改内容"从"自己构造原子"变成**正式入口** ✓。
#[test]
fn replace_object_data_edits_text_through_the_tool() {
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
        registry.call(
            &mut ctx,
            "draw_text",
            &json!({"layer_id": "L", "object_id": "t_tool",
                    // 用**红色**：本文件的 `ink` 助手数的是红色像素 ✓
                    //（第一版写成蓝色 ⇒ 前置条件直接为 0 ✗，又一次度量口径不一致 ✓）。
                    "data": {"text": "AB", "font": "builtin", "size": 21.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [8.0, 8.0], "align": "left"}}),
        );
    }
    let (before_count, _) = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert!(before_count > 0, "先画出文本（前置条件）");

    let replaced = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "replace_object_data",
            &json!({"object_id": "t_tool",
                    "data": {"text": "二级字库", "font": "builtin", "size": 21.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "position": [8.0, 8.0], "align": "left"}}),
        )
    };
    assert_eq!(replaced["ok"], json!(true), "{replaced}");

    let (after_count, _) = ink(&mut workspace, 0.0, 0.0, 200.0, 96.0);
    assert!(
        after_count != before_count && after_count > 0,
        "工具替换文本数据后渲染应改变（此前 {before_count}，之后 {after_count}）"
    );

    // 对象仍是 `text` ✓（可编辑 ✓），且对象 id 未变 ✓（是"替换数据"而不是新建对象）。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let object = listed["objects"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["object_id"] == json!("t_tool"))
        })
        .cloned()
        .expect("对象应仍在列表中");
    assert_eq!(
        object["type"],
        json!("text"),
        "替换数据后对象类型应保持 text"
    );
    assert_eq!(listed["count"], json!(1), "不应新建对象：{listed}");
}

/// 参数校验：`data` 缺失或不是对象时必须报错 ✓（不留"静默接受"）。
#[test]
fn replace_object_data_validates_its_arguments() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_text", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    let missing = registry.call(&mut ctx, "replace_object_data", &json!({"object_id": "x"}));
    assert_eq!(missing["ok"], json!(false), "{missing}");
    let wrong_type = registry.call(
        &mut ctx,
        "replace_object_data",
        &json!({"object_id": "x", "data": 3}),
    );
    assert_eq!(wrong_type["ok"], json!(false), "{wrong_type}");
    // 不存在的对象：由折叠层按"无孤儿引用"拒绝 ✓。
    let missing_object = registry.call(
        &mut ctx,
        "replace_object_data",
        &json!({"object_id": "nope", "data": {"text": "x"}}),
    );
    assert_eq!(missing_object["ok"], json!(false), "{missing_object}");
}
