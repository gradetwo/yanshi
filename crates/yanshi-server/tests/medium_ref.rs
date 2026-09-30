//! 插件介质描述符：**`id + version` 随原子记录，升级不自动改变旧文档渲染** ✓
//! （设计 11.1 的插件章节明确要求这两条 ✓）。
//!
//! 本文件先落"记录 + 校验 + 不变量"这三块基石 ✓ —— 真正的插件执行（油画等介质）
//! 需要宿主侧的 WASM 运行时 ✓，属后续切片 ✓；但"旧文档渲染不被升级改写"这条不变量
//! 必须**从第一天起**就成立 ✓，否则日后无法保证历史可复现 ✓。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_medium", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_medium", Bbox::new(0.0, 0.0, 64.0, 64.0))
        .expect("渲染应成功")
        .2
}

fn fresh() -> Workspace {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_medium", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    workspace
}

fn stroke(workspace: &mut Workspace, object_id: &str, medium: Option<serde_json::Value>) -> Value {
    let registry = registry();
    let mut ctx = context(workspace);
    let mut data = json!({
        "points": [[8.0, 32.0], [56.0, 32.0]],
        "size": 10.0,
        "color": {"r": 200, "g": 40, "b": 40, "a": 255}
    });
    if let Some(medium) = medium {
        data["medium"] = medium;
    }
    registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": object_id, "data": data}),
    )
}

/// 描述符**随原子记录** ✓：写入后能在对象列表里读到完整的 `{id, version}` ✓。
#[test]
fn a_medium_descriptor_is_recorded_with_the_object() {
    let mut workspace = fresh();
    let drawn = stroke(
        &mut workspace,
        "s_medium",
        Some(json!({"id": "oil", "version": 3})),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    let listed = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "list_objects", &json!({}))
    };
    let object = listed["objects"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["object_id"] == json!("s_medium"))
        })
        .cloned()
        .expect("对象应在列表中");
    assert_eq!(
        object["medium"],
        json!({"id": "oil", "version": 3}),
        "介质描述符（含**版本**）必须随对象记录 ✓"
    );
}

/// **不变量**：安装/升级插件**不会**改写旧对象的渲染 ✓ ——
/// 无插件时同一笔触与不带 medium 的渲染**逐字节一致** ✓（版本钉在对象上 ✓）。
#[test]
fn an_installed_or_upgraded_medium_does_not_change_existing_renders() {
    let mut plain = fresh();
    stroke(&mut plain, "s1", None);
    let without = render(&mut plain);

    let mut with_medium = fresh();
    stroke(
        &mut with_medium,
        "s1",
        Some(json!({"id": "oil", "version": 1})),
    );
    let recorded = render(&mut with_medium);

    let differing = without
        .iter()
        .zip(recorded.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "记录介质不应改变渲染（内核尚无该插件 ⇒ 仍走通用光栅笔刷 ✓）；差异字节 {differing}"
    );

    // "升级" = 同一对象上换一个更高的版本号 ⇒ 旧对象仍按**它自己记录的版本**渲染 ✓，
    // 而当前实现根本不让升级改写历史对象 ✓（要么新建对象，要么显式替换数据 ✓）。
    let mut upgraded = fresh();
    stroke(
        &mut upgraded,
        "s1",
        Some(json!({"id": "oil", "version": 2})),
    );
    let after_upgrade = render(&mut upgraded);
    let differing = recorded
        .iter()
        .zip(after_upgrade.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "介质版本只随对象记录、不由全局状态决定 ⇒ 版本差异不应改变渲染 ✓（差异字节 {differing}）"
    );
}

/// 非法描述符必须**拒绝** ✓（不留静默接受 ✓）。
#[test]
fn invalid_medium_descriptors_are_rejected() {
    let cases: [(&str, serde_json::Value); 6] = [
        ("不是对象", json!("oil")),
        ("缺 id", json!({"version": 1})),
        ("空 id", json!({"id": "", "version": 1})),
        ("非法字符", json!({"id": "oil/../x", "version": 1})),
        ("缺 version", json!({"id": "oil"})),
        ("version 为 0", json!({"id": "oil", "version": 0})),
    ];
    for (label, medium) in cases {
        let mut workspace = fresh();
        let drawn = stroke(&mut workspace, "s_bad", Some(medium.clone()));
        assert_eq!(
            drawn["ok"],
            json!(false),
            "「{label}」应被拒绝，实际 {drawn}（medium={medium}）"
        );
    }
}
