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
