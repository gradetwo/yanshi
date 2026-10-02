//! **"不存在"要说清"有哪些"** ✓（真实用户 §五-18 的原话 ✓）。
//!
//! **原症状** ✓：图层不存在时只回 `reference_not_found` ✓ ⇒ **不说可用图层有哪些** ✗
//! ⇒ 调用方（尤其 agent ✓）只能再调一次列表接口 ✓、或者乱试几个 id ✗。
//! **本测试钉死两件事** ✓：① 拒绝时**列出可用 id** ✓；② **空的时候单独说** ✓ ——
//! 后者比"列一串空值"有用得多 ✓（"当前没有任何图层" 一眼就知道下一步该建图层 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_missing", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_missing", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
}

fn detail_of(value: &serde_json::Value) -> String {
    value["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// **引用一个不存在的图层 ⇒ 报错必须列出已有的图层** ✓。
#[test]
fn a_missing_layer_lists_the_layers_that_do_exist() {
    let mut workspace = workspace();
    setup(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L_alpha" }));
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L_beta" }));
    }
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L_typo", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 1, "y": 2, "w": 3, "h": 4}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "应当被拒：{refused}");
    assert_eq!(
        refused["error_code"],
        json!("reference_not_found"),
        "{refused}"
    );
    let detail = detail_of(&refused);
    // ① 点名那个错的 id ✓ ② **列出可用的** ✓（这条就是用户要的 ✓）
    assert!(detail.contains("L_typo"), "应当点名写错的那个：{detail}");
    assert!(
        detail.contains("L_alpha"),
        "应当列出可用图层 L_alpha：{detail}"
    );
    assert!(
        detail.contains("L_beta"),
        "应当列出可用图层 L_beta：{detail}"
    );
}

/// **一个图层都没有时 ⇒ 单独说清楚** ✓（而不是给一句空的"现有："✓）。
#[test]
fn a_missing_layer_with_no_layers_says_so_plainly() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L_nope", "object_id": "s1",
                    "data": {"points": [[1.0, 2.0], [3.0, 4.0]], "size": 4.0,
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = detail_of(&refused);
    assert!(detail.contains("没有任何"), "空集合要单独说：{detail}");
}

/// **可用值不能把报错变成一屏** ✓（长文档里可能有几百个图层 ✓）。
///
/// **顺带记下一个既有语义** ✓：**父图层不存在不是"拒绝"** ✓ 而是 `ok: true` +
/// 一条 **`cascade_invalidation` 警告** ✓（所以本测试断言的是**警告文本** ✓，不是错误码 ✓ ——
/// 我第一版按"拒绝"写 ✓ ⇒ 断言全错 ✗ ⇒ **读代码不如先看真实返回** ✓）。
#[test]
fn the_list_of_available_ids_is_capped() {
    let mut workspace = workspace();
    setup(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        for index in 0..12 {
            registry().call(
                &mut ctx,
                "create_layer",
                &json!({ "layer_id": format!("L_{index:02}") }),
            );
        }
    }
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "create_layer",
            &json!({ "layer_id": "L_new", "parent_id": "L_missing" }),
        )
    };
    assert_eq!(made["ok"], json!(true), "父图层缺失是警告而非拒绝：{made}");
    let warnings = made["warnings"].as_array().cloned().unwrap_or_default();
    let detail = warnings
        .iter()
        .filter_map(|warning| warning["detail"].as_str())
        .find(|detail| detail.contains("L_missing"))
        .unwrap_or_default()
        .to_owned();
    assert!(
        !detail.is_empty(),
        "应当有一条提到 L_missing 的警告：{made}"
    );
    // ① 列出了一些可用值 ✓ ② 超出上限以省略号收尾 ✓ ③ 整体别太长 ✓
    assert!(detail.contains("L_00"), "应当列出可用图层：{detail}");
    assert!(detail.contains("…"), "超出上限时应当省略：{detail}");
    assert!(
        detail.chars().count() < 200,
        "报错不该变成一屏（实测 {} 字）：{detail}",
        detail.chars().count()
    );
}
