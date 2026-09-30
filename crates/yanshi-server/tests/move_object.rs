//! `move_object` 必须真的移动像素（对象变换此前**根本没进内核**）。
//!
//! 隔离实测（本轮）：`move_object` 返回 `ok: true`，但渲染出的红色像素范围与
//! `list_objects` 的 bbox **都完全没变** ✗。根因：`yanshi-render` 里**一处都没有读
//! `object.transform`** ✓ —— `object_bbox` 的注释也写着「对象的仿射 transform 尚未进入内核」✓。
//! 于是设计 13.3 的基础工具「移动/变换」形同虚设 ✓。
//!
//! 修法：`object::transform_primitive` 在解析后统一把变换施加到图元几何上 ✓，
//! 且 `object_bbox` 也基于**变换后**的图元计算 ✓（剔除与命中测试随之正确 ✓）。

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
    ToolContext::new(workspace, "doc_move", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 返回区域渲染里"红色像素"的包围盒 `(min_x, min_y, max_x, max_y)`。
fn red_bounds(workspace: &mut Workspace) -> Option<(usize, usize, usize, usize)> {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_move", Bbox::new(0.0, 0.0, 256.0, 256.0))
        .expect("区域渲染应成功");
    let mut bounds: Option<(usize, usize, usize, usize)> = None;
    for index in 0..pixels.len() / 4 {
        let pixel = &pixels[index * 4..index * 4 + 4];
        if pixel[0] > 200 && pixel[1] < 120 {
            let x = index % 256;
            let y = index / 256;
            bounds = Some(match bounds {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    bounds
}

#[test]
fn move_object_shifts_rendered_pixels_and_bbox() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_move", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        let drawn = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "r1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 20, "y": 20, "w": 40, "h": 40}},
                             "color": {"r": 250, "g": 20, "b": 20, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let before = red_bounds(&mut workspace).expect("移动前应有红色像素");
    assert_eq!(before, (20, 20, 59, 59), "初始位置应为 20..59");

    let moved = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "r1", "delta": {"dx": 60, "dy": 30}}),
        )
    };
    assert_eq!(moved["ok"], json!(true), "{moved}");

    // 像素必须**精确**平移 (60, 30) —— 此前返回 ok 但一个像素都不动。
    let after = red_bounds(&mut workspace).expect("移动后仍应有红色像素");
    assert_eq!(
        after,
        (80, 50, 119, 89),
        "像素应精确平移 (60,30)，实际 {after:?}（此前对象变换根本没进内核）"
    );

    // 命中测试依赖的 bbox 也必须跟着走。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let bbox = listed["objects"]
        .as_array()
        .and_then(|objects| objects.iter().find(|o| o["object_id"] == json!("r1")))
        .map(|object| object["bbox"].clone())
        .expect("应能取到 r1 的 bbox");
    assert_eq!(
        bbox,
        json!([80.0, 50.0, 40.0, 40.0]),
        "bbox 应为变换后的位置（命中测试依赖它）"
    );
}
