//! `fill` 必须真的改变像素。
//!
//! 用户报告 + API 实测发现的真实缺陷：`fill` 返回 `ok: true` 但**一点像素都不变** ✗。
//! 根因：`ObjectType` 里**没有 Fill** ✓，fold 按 `payload.type` 决定对象类型、缺省落到
//! **Stroke** ✗ —— 填充对象于是被当作"没有 points 的笔触"解析，什么都不画 ✓。
//! 修法：工具层把 `{color, region}` 规范化为矩形形状（`bbox`）并显式声明 `type: "shape"` ✓。

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
    ToolContext::new(workspace, "doc_fill", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace, x: f64, y: f64) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_fill", Bbox::new(x, y, 40.0, 40.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

/// 指定区域的填充：区域内出现颜色，区域外不受影响。
#[test]
fn fill_changes_pixels_inside_the_region_only() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_fill", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    assert_eq!(ink(&mut workspace, 40.0, 40.0), 0, "填充前该区域应为空");

    let response = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "fill",
            &json!({
                "layer_id": "L",
                "data": {"color": {"r": 10, "g": 200, "b": 120, "a": 255},
                         "region": {"x": 20, "y": 20, "w": 100, "h": 100}}
            }),
        )
    };
    assert_eq!(response["ok"], json!(true), "{response}");

    assert!(
        ink(&mut workspace, 40.0, 40.0) > 0,
        "填充区域内的像素必须改变（此前 fill 返回 ok 但什么都不画）"
    );
    assert_eq!(ink(&mut workspace, 180.0, 180.0), 0, "填充区域外不应被改动");
}

/// 不给 `region` 时填充整幅画布（用户直觉的「填充图层」）。
#[test]
fn fill_without_region_covers_the_whole_canvas() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_fill", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let response = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L", "data": {"color": {"r": 30, "g": 30, "b": 200, "a": 255}}}),
        )
    };
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(ink(&mut workspace, 10.0, 10.0) > 0, "左上是填充后的颜色");
    assert!(ink(&mut workspace, 90.0, 90.0) > 0, "右下也是填充后的颜色");
}

/// `region` 也接受数组写法 `[x, y, w, h]`（工具层此前两种都接受但都不生效）。
#[test]
fn fill_region_accepts_array_form() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_fill", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let response = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L",
                    "data": {"color": {"r": 200, "g": 20, "b": 20, "a": 255}, "region": [10, 10, 60, 60]}}),
        )
    };
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(
        ink(&mut workspace, 20.0, 20.0) > 0,
        "数组写法的区域也应被填充"
    );
}

/// 撤销填充：`revert` 之后区域应重新变空（浏览器检查里这条断言不可靠，因此在这里定死）。
#[test]
fn reverting_a_fill_restores_the_previous_pixels() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_fill", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let fill = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L",
                    "data": {"color": {"r": 10, "g": 200, "b": 120, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
        )
    };
    assert_eq!(fill["ok"], json!(true), "{fill}");
    let atom_id = fill["atom_id"]
        .as_str()
        .expect("fill 应返回 atom_id")
        .to_owned();
    assert!(ink(&mut workspace, 10.0, 10.0) > 0, "填充后区域应有颜色");

    let reverted = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "revert", &json!({"atom_id": atom_id}))
    };
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    assert_eq!(
        ink(&mut workspace, 10.0, 10.0),
        0,
        "撤销填充后区域应重新变空（API 实测如此；浏览器断言不可靠，故在此定死）"
    );
}
