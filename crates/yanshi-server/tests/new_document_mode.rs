//! **`new_document` 的 `mode` 参数** ✓（反馈报告 2 §4.6 期 2）。
//!
//! **钉住** ✓：① 缺省 open = 今天的行为（同 id ⇒ 打开、内容不动、传入的宽高与背景
//! 原样列在 ignored 里）；② mode=create 在已存在时报错（不打开、不清空）；
//! ③ mode=reset 在已存在时删后重建空白（新尺寸生效、旧墨归零）；
//! ④ mode=reset 在不存在时就是新建（不谎报 reset）；⑤ 非法 mode 报错且报出合法取值；
//! ⑥ reset 遵循与 delete_document 相同的权限闸（editor 不能跨文档删）。

use serde_json::json;
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, session_doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, session_doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(
    workspace: &mut Workspace,
    session_doc: &str,
    tool: &str,
    args: &serde_json::Value,
) -> serde_json::Value {
    let mut ctx = context(workspace, session_doc);
    registry().call(&mut ctx, tool, args)
}

/// **建一个带墨的文档** ✓：320×240，画一个 60×40 深色矩形 ⇒ 墨点 > 100。
fn make_inked(workspace: &mut Workspace, doc_id: &str) {
    let made = call(
        workspace,
        doc_id,
        "new_document",
        &json!({ "doc_id": doc_id, "width": 320, "height": 240 }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let layer = call(
        workspace,
        doc_id,
        "create_layer",
        &json!({ "layer_id": "L" }),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let drawn = call(
        workspace,
        doc_id,
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "s1",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 60, "h": 40}},
                         "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
}

fn ink_count(workspace: &mut Workspace, doc_id: &str, width: f64, height: f64) -> usize {
    let (_, _, pixels) = workspace
        .document_mut(doc_id)
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, width, height))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| {
            (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                / 1000
                < 200
        })
        .count()
}

/// **缺省 open 不动** ✓：同 id 再调 ⇒ 打开、内容不动；传入的宽高与背景原样列在 ignored 里。
#[test]
fn default_open_is_unchanged_and_reports_ignored_values() {
    let mut workspace = workspace();
    make_inked(&mut workspace, "doc_mode");
    let before = ink_count(&mut workspace, "doc_mode", 320.0, 240.0);
    assert!(before > 100, "画完应当有墨（实测 {before}）");
    let opened = call(
        &mut workspace,
        "doc_mode",
        "new_document",
        &json!({"doc_id": "doc_mode", "width": 640, "height": 480,
                 "background": {"r": 1, "g": 2, "b": 3, "a": 255}}),
    );
    assert_eq!(opened["ok"], json!(true), "{opened}");
    assert_eq!(opened["opened"], json!(true), "{opened}");
    assert_eq!(opened["created"], json!(false), "{opened}");
    assert_eq!(opened["mode"], json!("open"), "{opened}");
    assert_eq!(
        (opened["width"].as_u64(), opened["height"].as_u64()),
        (Some(320), Some(240)),
        "打开不许改尺寸：{opened}"
    );
    assert_eq!(opened["ignored"]["width"], json!(640), "{opened}");
    assert_eq!(opened["ignored"]["height"], json!(480), "{opened}");
    assert_eq!(opened["ignored"]["background"]["r"], json!(1), "{opened}");
    let note = opened["note"].as_str().unwrap_or_default();
    assert!(
        note.contains("打开") && note.contains("doc_id"),
        "应当说明这是打开：{note}"
    );
    let still = ink_count(&mut workspace, "doc_mode", 320.0, 240.0);
    assert_eq!(
        still, before,
        "打开之后原文档必须原样（原来 {before}，现在 {still}）"
    );
}

/// **缺省 open 不传宽高时没有 ignored** ✓：没传的键不编 ✓。
#[test]
fn default_open_without_passed_values_has_no_ignored() {
    let mut workspace = workspace();
    make_inked(&mut workspace, "doc_mode_plain");
    let opened = call(
        &mut workspace,
        "doc_mode_plain",
        "new_document",
        &json!({ "doc_id": "doc_mode_plain" }),
    );
    assert_eq!(opened["ok"], json!(true), "{opened}");
    assert_eq!(opened["opened"], json!(true), "{opened}");
    assert!(
        opened.get("ignored").is_none(),
        "没传宽高与背景 ⇒ 不该有 ignored：{opened}"
    );
}

/// **mode=create 在已存在时报错** ✓：不打开、不清空，报错里同时给两条出路。
#[test]
fn create_mode_refuses_existing_without_opening_or_clearing() {
    let mut workspace = workspace();
    make_inked(&mut workspace, "doc_mode_create");
    let before = ink_count(&mut workspace, "doc_mode_create", 320.0, 240.0);
    assert!(before > 100, "画完应当有墨（实测 {before}）");
    let refused = call(
        &mut workspace,
        "doc_mode_create",
        "new_document",
        &json!({"doc_id": "doc_mode_create", "mode": "create", "width": 320, "height": 240}),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "已存在时 create 应当报错：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("已存在") && detail.contains("mode=reset") && detail.contains("mode=open"),
        "报错应当说明已存在并给出两条出路：{detail}"
    );
    let still = ink_count(&mut workspace, "doc_mode_create", 320.0, 240.0);
    assert_eq!(
        still, before,
        "报错之后原文档必须原样（原来 {before}，现在 {still}）"
    );
    // **报错之后仍能正常打开** ✓（create 报错不是破坏性操作 ✓）。
    let opened = call(
        &mut workspace,
        "doc_mode_create",
        "new_document",
        &json!({ "doc_id": "doc_mode_create" }),
    );
    assert_eq!(opened["opened"], json!(true), "{opened}");
}

/// **mode=create 在不存在时就是新建** ✓。
#[test]
fn create_mode_on_fresh_id_creates() {
    let mut workspace = workspace();
    let made = call(
        &mut workspace,
        "doc_mode_fresh",
        "new_document",
        &json!({"doc_id": "doc_mode_fresh", "mode": "create", "width": 320, "height": 240}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["created"], json!(true), "{made}");
    assert_eq!(made["mode"], json!("create"), "{made}");
    assert!(made.get("reset").is_none(), "新建不该带 reset 标记：{made}");
}

/// **mode=reset 在已存在时删后重建空白** ✓：新尺寸生效、旧墨归零、reset:true。
#[test]
fn reset_mode_wipes_and_recreates_blank() {
    let mut workspace = workspace();
    make_inked(&mut workspace, "doc_mode_reset");
    let before = ink_count(&mut workspace, "doc_mode_reset", 320.0, 240.0);
    assert!(before > 100, "画完应当有墨（实测 {before}）");
    let remade = call(
        &mut workspace,
        "doc_mode_reset",
        "new_document",
        &json!({"doc_id": "doc_mode_reset", "mode": "reset", "width": 640, "height": 480}),
    );
    assert_eq!(remade["ok"], json!(true), "{remade}");
    assert_eq!(remade["created"], json!(true), "{remade}");
    assert_eq!(remade["opened"], json!(false), "{remade}");
    assert_eq!(remade["mode"], json!("reset"), "{remade}");
    assert_eq!(remade["reset"], json!(true), "{remade}");
    assert_eq!(
        (remade["width"].as_u64(), remade["height"].as_u64()),
        (Some(640), Some(480)),
        "重建尺寸按传入值：{remade}"
    );
    let after = ink_count(&mut workspace, "doc_mode_reset", 640.0, 480.0);
    assert_eq!(after, 0, "重建后应当是空白画布（实测 {after} 个墨点）");
    let state_size = {
        let document = workspace.document("doc_mode_reset").expect("已重建");
        (document.state().width, document.state().height)
    };
    assert_eq!(state_size, (640, 480), "重建后尺寸应当是 640×480");
}

/// **mode=reset 在不存在时就是新建** ✓：不谎报 reset。
#[test]
fn reset_mode_on_fresh_id_just_creates() {
    let mut workspace = workspace();
    let made = call(
        &mut workspace,
        "doc_mode_reset_fresh",
        "new_document",
        &json!({"doc_id": "doc_mode_reset_fresh", "mode": "reset", "width": 320, "height": 240}),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["created"], json!(true), "{made}");
    assert_eq!(made["mode"], json!("reset"), "{made}");
    assert!(
        made.get("reset").is_none(),
        "没删过 ⇒ 不该带 reset 标记：{made}"
    );
}

/// **非法 mode 报错且报出合法取值** ✓（不静默回落到 open ✓）。
#[test]
fn invalid_mode_is_refused_loudly() {
    let mut workspace = workspace();
    // **未知字符串** ✓：进到 handler 的取值校验 ⇒ 报错里报出合法取值 ✓。
    let refused = call(
        &mut workspace,
        "doc_mode_bad",
        "new_document",
        &json!({"doc_id": "doc_mode_bad", "mode": "wipe", "width": 100, "height": 100}),
    );
    assert_eq!(refused["ok"], json!(false), "未知字符串应当被拒：{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("open") && detail.contains("create") && detail.contains("reset"),
        "未知字符串的原因应当报出合法取值：{detail}"
    );
    // **非字符串** ✓：spec 已声明 mode 为 string ⇒ 框架的类型校验先拦下，
    // 报错说清"类型应为 string" ✓（handler 的非字符串分支是纵深防御，走不到这里 ✓）。
    let refused_type = call(
        &mut workspace,
        "doc_mode_bad",
        "new_document",
        &json!({"doc_id": "doc_mode_bad", "mode": 42, "width": 100, "height": 100}),
    );
    assert_eq!(
        refused_type["ok"],
        json!(false),
        "非字符串 mode 应当被拒：{refused_type}"
    );
    let detail_type = refused_type["context"]["detail"]
        .as_str()
        .unwrap_or_default();
    assert!(
        detail_type.contains("string"),
        "非字符串的原因应当说明类型应为 string：{detail_type}"
    );
}

/// **reset 遵循与 delete_document 相同的权限闸** ✓：editor 不能跨文档删，
/// 但删自己的会话文档可以（doc_id 相等即放行 ✓）。
#[test]
fn reset_mode_honours_the_delete_scope_gate() {
    let mut workspace = workspace();
    make_inked(&mut workspace, "doc_mode_other");
    // **自己的会话文档先建一份带墨的** ✓：否则 reset 是"不存在⇒新建"，不带 reset 标记 ✓。
    make_inked(&mut workspace, "doc_mode_self");
    let self_before = ink_count(&mut workspace, "doc_mode_self", 320.0, 240.0);
    assert!(
        self_before > 100,
        "自己的文档应当先有墨（实测 {self_before}）"
    );
    let denied = {
        let mut editor =
            ToolContext::new(&mut workspace, "doc_mode_self", "human:1", "session:test")
                .with_role(yanshi_server::token::Role::Editor)
                .with_wait_for_render(true, 4_000);
        registry().call(
            &mut editor,
            "new_document",
            &json!({"doc_id": "doc_mode_other", "mode": "reset", "width": 100, "height": 100}),
        )
    };
    assert_eq!(
        denied["ok"],
        json!(false),
        "editor 跨文档 reset 应当被拒：{denied}"
    );
    assert_eq!(
        denied["error_code"],
        json!("permission_denied"),
        "应当是权限错误：{denied}"
    );
    let still = ink_count(&mut workspace, "doc_mode_other", 320.0, 240.0);
    assert!(still > 100, "被拒之后对方文档必须原样（实测 {still}）");
    // **自己的会话文档可以 reset** ✓（doc_id 相等即放行 ✓）。
    let own = {
        let mut editor =
            ToolContext::new(&mut workspace, "doc_mode_self", "human:1", "session:test")
                .with_role(yanshi_server::token::Role::Editor)
                .with_wait_for_render(true, 4_000);
        registry().call(
            &mut editor,
            "new_document",
            &json!({"doc_id": "doc_mode_self", "mode": "reset", "width": 100, "height": 100}),
        )
    };
    assert_eq!(
        own["ok"],
        json!(true),
        "自己的会话文档 reset 应当放行：{own}"
    );
    assert_eq!(own["reset"], json!(true), "{own}");
    let self_after = ink_count(&mut workspace, "doc_mode_self", 100.0, 100.0);
    assert_eq!(
        self_after, 0,
        "自己的文档 reset 后应当是空白（实测 {self_after} 个墨点）"
    );
}
