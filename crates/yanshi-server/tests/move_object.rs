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

/// **移动后旧位置必须被失效并重绘** ✓ —— 用户实测：移动对象后画布**旧位置不刷新** ✗
/// （留下残影 ✓），而缩略图（整幅重绘 ✓）正常 ✓。
///
/// 根因：脏区规划只用**新状态**的包围盒 ✗（`dirty_for_object` ✓），
/// 移动时旧位置从未进入失效集合 ✓。本用例直接断言**旧位置的像素变回背景** ✓，
/// 这类"只看对象属性、不看旧像素"的断言缺口 ✓ 正是 bug 溜过去的原因 ✓。
#[test]
fn moving_an_object_clears_the_pixels_it_left_behind() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_move", 128, 64),
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
            &json!({"layer_id": "L", "object_id": "a_square",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 8.0, "y": 8.0, "w": 24.0, "h": 24.0}},
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }

    let red_at = |workspace: &mut Workspace, x: f64, y: f64| -> [u8; 4] {
        let raw = workspace
            .render_region_raw("doc_move", Bbox::new(x, y, 1.0, 1.0))
            .expect("渲染应成功")
            .2;
        [raw[0], raw[1], raw[2], raw[3]]
    };

    let before_old = red_at(&mut workspace, 20.0, 20.0);
    assert!(
        before_old[0] > 200 && before_old[1] < 60,
        "移动前旧位置应是红色 ✓，实际 {before_old:?}"
    );

    {
        let mut ctx = context(&mut workspace);
        let moved = registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "a_square", "delta": {"dx": 72.0, "dy": 0.0}}),
        );
        assert_eq!(moved["ok"], json!(true), "{moved}");
    }

    let after_old = red_at(&mut workspace, 20.0, 20.0);
    let after_new = red_at(&mut workspace, 92.0, 20.0);
    assert!(
        after_old[0] < 60 || after_old[1] > 200,
        "**移动后旧位置必须变回背景** ✓（否则画布会留残影 ✗），实际 {after_old:?}"
    );
    assert!(
        after_new[0] > 200 && after_new[1] < 60,
        "移动后新位置应是红色 ✓，实际 {after_new:?}"
    );
}

/// **`delta` 是增量、`transform` 是绝对值** ✓ —— 子 agent 报的 #6：
/// `delta{dx:30}` → 130 ✓，再 `delta{dx:10}` → **110** ✗（期望 **140** ✓，前面那一步被丢掉了 ✓）。
///
/// 两者的名字本就说明语义 ✓：`delta` = 增移 ✓ ⇒ 与对象**当前**变换复合 ✓（这只能在折叠层做 ✓，
/// 因为只有它知道当前状态 ✓）；`transform` = 绝对 ✓ ⇒ 直接赋值 ✓。
#[test]
fn successive_deltas_compose_instead_of_replacing() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_move", 256, 128),
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
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "a_box",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 20.0, "y": 20.0, "w": 30.0, "h": 30.0}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        );
    }
    let bbox_of = |workspace: &mut Workspace| -> Vec<f64> {
        let listed = {
            let mut ctx = context(workspace);
            registry.call(&mut ctx, "list_objects", &json!({}))
        };
        listed["objects"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["object_id"] == json!("a_box"))
            })
            .and_then(|object| object["bbox"].as_array())
            .map(|bbox| bbox.iter().filter_map(serde_json::Value::as_f64).collect())
            .expect("对象应有包围盒")
    };

    let before = bbox_of(&mut workspace);
    for _ in 0..2 {
        let mut ctx = context(&mut workspace);
        let moved = registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "a_box", "delta": {"dx": 30.0, "dy": 0.0}}),
        );
        assert_eq!(moved["ok"], json!(true), "{moved}");
    }
    let after = bbox_of(&mut workspace);
    assert_eq!(
        (after[0] - before[0]).round(),
        60.0,
        "连续两次 dx=30 应累积 60（而不是只生效最后一次 ✓）：{before:?} → {after:?}"
    );

    // **绝对值入口仍然可用** ✓：`transform` 直接赋值 ✓。
    let mut ctx = context(&mut workspace);
    let placed = registry.call(
        &mut ctx,
        "move_object",
        &json!({"object_id": "a_box", "transform": {"matrix": [1, 0, 0, 1, 5, 5], "pivot": [0, 0]}}),
    );
    assert_eq!(placed["ok"], json!(true), "{placed}");
    let absolute = bbox_of(&mut workspace);
    // **注意语义** ✓：`transform` 是**施加在对象自身几何上**的变换 ✓，不是"把包围盒设成某个坐标" ✗。
    // 对象的基准几何是 (20,20) ✓ ⇒ 平移 (5,5) 之后落在 **(25,25)** ✓
    //（我第一版期望 (5,5) ✗ —— 那是我把两种语义搞混了 ✓，实现本身是对的 ✓）。
    assert_eq!(
        (absolute[0].round(), absolute[1].round()),
        (25.0, 25.0),
        "`transform` 应把对象的基准几何 (20,20) 平移到 (25,25)：{absolute:?}"
    );
}

/// **对象必须能回到原点** ✓ —— 子 agent 报的 #7：
/// `move_object {delta:{0,0}}` / 单位矩阵都返回 ok 却**不改包围盒** ✗
/// （折叠层用 `if !transform.is_identity()` 把单位变换丢掉了 ✓）。
#[test]
fn an_identity_transform_returns_the_object_to_the_origin() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_move", 256, 128),
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
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "b_box",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 40.0, "y": 40.0, "w": 20.0, "h": 20.0}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "b_box", "delta": {"dx": 70.0, "dy": 30.0}}),
        );
    }
    let bbox_now = |workspace: &mut Workspace| -> Vec<f64> {
        let listed = {
            let mut ctx = context(workspace);
            registry.call(&mut ctx, "list_objects", &json!({}))
        };
        listed["objects"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["object_id"] == json!("b_box"))
            })
            .and_then(|object| object["bbox"].as_array())
            .map(|bbox| bbox.iter().filter_map(serde_json::Value::as_f64).collect())
            .expect("对象应有包围盒")
    };
    let moved = bbox_now(&mut workspace);
    assert!((moved[0] - 110.0).abs() < 1.0, "先移开：{moved:?}");

    // 单位矩阵 ⇒ 回到文档原点 ✓（这里"原点"指**未施加变换**的位置 ✓）。
    let mut ctx = context(&mut workspace);
    let reset = registry.call(
        &mut ctx,
        "move_object",
        &json!({"object_id": "b_box", "transform": {"matrix": [1, 0, 0, 1, 0, 0], "pivot": [0, 0]}}),
    );
    assert_eq!(reset["ok"], json!(true), "{reset}");
    let home = bbox_now(&mut workspace);
    assert!(
        (home[0] - 40.0).abs() < 1.0 && (home[1] - 40.0).abs() < 1.0,
        "单位变换必须真的把对象放回原位（40,40），实际 {home:?}"
    );
}
