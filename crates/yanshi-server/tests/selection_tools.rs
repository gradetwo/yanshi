//! 选区工具（设计 4.4；语义由用户确认为**路线 A：约束落笔**）。
//!
//! 设计只给了选区的**数据模型**与类型清单 ✓，未规定它对落笔的作用 ✓；
//! 用户拍板：选区只约束**之后新落笔**的像素 ✓、不改写已有内容 ✓、删掉选区后
//! 已画内容保持不变 ✓（约束在渲染期按日志重算，因此可撤销 ✓）。
//!
//! 本测试覆盖工具层的创建 / 列出 / 删除与参数校验；**渲染期的裁剪**由渲染侧测试覆盖。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_sel", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn selections_can_be_created_listed_and_deleted() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sel", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();

    // 创建：矩形选区 + 羽化 + 反选字段。
    let created = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel1",
                    "shape": {"kind": "rect", "bbox": {"x": 20.0, "y": 20.0, "w": 40.0, "h": 40.0}},
                    "feather": 4.0, "invert": false, "mode": "new"}),
        )
    };
    assert_eq!(created["ok"], json!(true), "创建选区应成功：{created}");

    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_selections", &json!({}))
    };
    assert_eq!(listed["count"], json!(1), "{listed}");
    assert_eq!(listed["selections"][0]["selection_id"], json!("sel1"));
    assert_eq!(listed["selections"][0]["feather"], json!(4.0));

    // 删除（tombstone）：选区消失。
    let deleted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel1"}),
        )
    };
    assert_eq!(deleted["ok"], json!(true), "删除选区应成功：{deleted}");
    let after = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_selections", &json!({}))
    };
    assert_eq!(after["count"], json!(0), "删除后不应再列出：{after}");
}

#[test]
fn selection_arguments_are_validated() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sel", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    // 缺 kind
    let bad_shape = registry.call(
        &mut ctx,
        "create_selection",
        &json!({"selection_id": "s", "shape": {"bbox": {"x": 0, "y": 0, "w": 1, "h": 1}}}),
    );
    assert_eq!(bad_shape["ok"], json!(false), "{bad_shape}");
    // 非法 mode
    let bad_mode = registry.call(
        &mut ctx,
        "create_selection",
        &json!({"selection_id": "s",
                "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 1, "h": 1}},
                "mode": "union"}),
    );
    assert_eq!(bad_mode["ok"], json!(false), "{bad_mode}");
    // 羽化越界
    let bad_feather = registry.call(
        &mut ctx,
        "create_selection",
        &json!({"selection_id": "s",
                "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 1, "h": 1}},
                "feather": 9999.0}),
    );
    assert_eq!(bad_feather["ok"], json!(false), "{bad_feather}");
}

/// **删掉的选区，id 必须能再用** ✓（真机复现的 bug ✓）。
///
/// **症状 ✗**：`delete_selection` 之后 ✓
/// `list_selections` 报 0 条 ✓、`get_document` 报 `selections: 0` ✓ ——
/// 但用**同一个 id** 再 `create_selection` 却回「选区 Q1 已存在」✗
/// ⇒ 两句互相矛盾 ✓、而且**没有任何出路** ✓（换 id 只是绕开 ✓、
/// 对一个固定工作流（比如总用 "sel_main"）就是永久报废这个名字 ✓）。
///
/// **根因 ✗**：`CreateSelection` 的"不许重名"检查写的是 `contains_key` ✓ ⇒
/// **把墓碑也算成占用** ✗；而 `CreateLayer` 早就改成"只与存活的冲突" ✓
///（`fold.rs` 的 `CreateLayer` 注释里记着一模一样的教训 ✓）。
/// **蒙版／风格同形** ✓（`create_mask` 撞墓碑也会永久报废 id ✓）。
#[test]
fn a_deleted_selection_id_can_be_reused() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sel", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let shape = |x: f64| json!({"kind": "rect", "bbox": {"x": x, "y": x, "w": 20.0, "h": 20.0}});

    let first = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_main", "shape": shape(10.0), "mode": "new"}),
        )
    };
    assert_eq!(first["ok"], json!(true), "首次创建应成功：{first}");

    let deleted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "delete_selection",
            &json!({"selection_id": "sel_main"}),
        )
    };
    assert_eq!(deleted["ok"], json!(true), "删除应成功：{deleted}");

    // **删除必须是"列表里没有"** ✓（否则下面的重建就没有自相矛盾的参照物 ✓）。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_selections", &json!({}))
    };
    assert_eq!(
        listed["count"],
        json!(0),
        "删除后不该再列出该选区：{listed}"
    );

    // 同一个 id **重建必须成功** ✓（本条就是回归点 ✗：修复前这里 ok:false）。
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_main", "shape": shape(40.0), "mode": "new"}),
        )
    };
    assert_eq!(
        again["ok"],
        json!(true),
        "墓碑不该占用选区 id（与图层同规则）：{again}"
    );

    let relisted = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_selections", &json!({}))
    };
    assert_eq!(relisted["count"], json!(1), "重建后应重新列出：{relisted}");

    // 但**存活**的那一个仍须拒绝重名 ✗ —— 否则就是无声覆盖用户正在用的选区 ✓。
    let duplicate = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_selection",
            &json!({"selection_id": "sel_main", "shape": shape(70.0), "mode": "new"}),
        )
    };
    assert_eq!(
        duplicate["ok"],
        json!(false),
        "存活选区必须拒绝同名：{duplicate}"
    );
    assert_eq!(
        duplicate["error_code"],
        json!("precondition_failed"),
        "应报前置条件失败：{duplicate}"
    );
}

/// **删掉的蒙版，id 必须能再用** ✓（与选区同一类 ✓）。
#[test]
fn a_deleted_mask_id_can_be_reused() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sel", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mask_args = json!({"mask_id": "mask_main",
                           "shape": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 16.0, "h": 16.0}}});
    let first = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_mask", &mask_args)
    };
    assert_eq!(first["ok"], json!(true), "首次创建蒙版应成功：{first}");
    let deleted = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "delete_mask", &json!({"mask_id": "mask_main"}))
    };
    assert_eq!(deleted["ok"], json!(true), "删除蒙版应成功：{deleted}");
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_mask", &mask_args)
    };
    assert_eq!(again["ok"], json!(true), "墓碑不该占用蒙版 id：{again}");
}
