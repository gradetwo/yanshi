//! 对象组（设计 9：`create_group` / `add_to_group` / `remove_from_group` / `set_group_transform`）。
//!
//! **本片的目标只有一条 ✓：成员要"一起动"** ✓ —— 这是用户能直接看到、也是组存在的理由 ✓。
//! 设计给出的完整链路是**派生解析**（9.2 `resolve_object` ✓、9.3 缓存共享与依赖图失效 ✓、
//! 循环引用检测 ✓）；本片**只做组** ✓，用"把平移增量作用到成员自身 transform 上"这条
//! 最小且处处自洽的路径 ✓ ⇒ 渲染 ✓、包围盒 ✓、命中测试 ✓、脏区规划 ✓ 都无需改动就正确 ✓
//!（见 `fold.rs` 里那段说明 ✓，其中也写明了**边界**：一般仿射、成员级 override、组嵌套尚未实现 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_group", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn square(offset: f64) -> serde_json::Value {
    json!({
        "geometry": {"kind": "rect", "bbox": {"x": offset, "y": offset, "w": 20.0, "h": 20.0}},
        "color": {"r": 10, "g": 20, "b": 30, "a": 255},
    })
}

fn bbox_of(workspace: &mut Workspace, registry: &ToolRegistry, object_id: &str) -> Vec<f64> {
    let listed = {
        let mut ctx = context(workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    listed["objects"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["object_id"] == json!(object_id))
        })
        .and_then(|object| object["bbox"].as_array())
        .map(|bbox| bbox.iter().filter_map(serde_json::Value::as_f64).collect())
        .unwrap_or_else(|| panic!("对象 {object_id} 应有包围盒：{listed}"))
}

/// **组成员一起移动** ✓ —— 这是组唯一不可替代的性质 ✓（也是本片交付的东西 ✓）。
#[test]
fn group_members_move_together() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_group", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "a", "data": square(20.0)}),
        );
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "b", "data": square(60.0)}),
        );
    }
    let a_before = bbox_of(&mut workspace, &registry, "a");
    let b_before = bbox_of(&mut workspace, &registry, "b");

    // 建组并纳入两个对象 ✓。
    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_group",
            &json!({"group_id": "g1", "layer_id": "L", "members": ["a"]}),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
        let added = registry.call(
            &mut ctx,
            "add_to_group",
            &json!({"group_id": "g1", "object_id": "b"}),
        );
        assert_eq!(added["ok"], json!(true), "{added}");
        assert_eq!(
            added["members"],
            json!(["a", "b"]),
            "成员列表应包含两个对象：{added}"
        );
    }

    // **平移整组** ✓ ⇒ 两个成员必须各自移动同样的距离 ✓。
    {
        let mut ctx = context(&mut workspace);
        let moved = registry.call(
            &mut ctx,
            "set_group_transform",
            &json!({"group_id": "g1", "delta": {"dx": 40.0, "dy": 25.0}}),
        );
        assert_eq!(moved["ok"], json!(true), "{moved}");
    }
    let a_after = bbox_of(&mut workspace, &registry, "a");
    let b_after = bbox_of(&mut workspace, &registry, "b");
    for (label, before, after) in [("a", &a_before, &a_after), ("b", &b_before, &b_after)] {
        let dx = after[0] - before[0];
        let dy = after[1] - before[1];
        assert!(
            (dx - 40.0).abs() < 1.5 && (dy - 25.0).abs() < 1.5,
            "成员 {label} 应随组平移 (40, 25)，实际 ({dx:.1}, {dy:.1})"
        );
    }
    // **两者位移必须一致** ✓（"一起动"的关键 ✓：只各自动一半或各自不同都不算 ✓）。
    let delta_a = (a_after[0] - a_before[0], a_after[1] - a_before[1]);
    let delta_b = (b_after[0] - b_before[0], b_after[1] - b_before[1]);
    assert!(
        (delta_a.0 - delta_b.0).abs() < 1.5 && (delta_a.1 - delta_b.1).abs() < 1.5,
        "两个成员的位移必须一致（a {delta_a:?} vs b {delta_b:?}）"
    );

    // **移出之后就不再跟随** ✓ —— 否则"成员关系"没有任何约束力 ✓。
    {
        let mut ctx = context(&mut workspace);
        let removed = registry.call(
            &mut ctx,
            "remove_from_group",
            &json!({"group_id": "g1", "object_id": "b"}),
        );
        assert_eq!(removed["ok"], json!(true), "{removed}");
        assert_eq!(removed["members"], json!(["a"]), "移除后只剩 a：{removed}");
    }
    let b_before_second = bbox_of(&mut workspace, &registry, "b");
    {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "set_group_transform",
            &json!({"group_id": "g1", "delta": {"dx": 10.0, "dy": 0.0}}),
        );
    }
    let a_third = bbox_of(&mut workspace, &registry, "a");
    let b_third = bbox_of(&mut workspace, &registry, "b");
    assert!(
        (a_third[0] - a_after[0] - 10.0).abs() < 1.5,
        "仍在组内的 a 应继续跟随（{a_after:?} → {a_third:?}）"
    );
    assert!(
        (b_third[0] - b_before_second[0]).abs() < 1.5,
        "已移出的 b 不该再跟随（{b_before_second:?} → {b_third:?}）"
    );
}

/// **本片的边界必须显式报错** ✓ —— 静默接受一个不会生效的姿势比不支持更糟 ✓。
#[test]
fn the_slice_boundaries_are_reported_instead_of_ignored() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_group", 128, 128),
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
            &json!({"layer_id": "L", "object_id": "a", "data": square(10.0)}),
        );
        registry.call(
            &mut ctx,
            "create_group",
            &json!({"group_id": "g1", "layer_id": "L"}),
        );

        // ① **组嵌套**：本片不支持 ✓ ⇒ 必须报错 ✓（需要设计 9.2 的派生解析 ✓）。
        let nested = registry.call(
            &mut ctx,
            "create_group",
            &json!({"group_id": "g2", "layer_id": "L", "members": ["g1"]}),
        );
        assert_eq!(nested["ok"], json!(false), "组嵌套应被拒绝：{nested}");
        assert_eq!(
            nested["error_code"].as_str().unwrap_or_default(),
            "invalid_argument",
            "应给出 invalid_argument：{nested}"
        );

        // ② **一般仿射的组变换**：本片只支持平移 ✓ ⇒ 必须报错 ✓ 而不是静默忽略 ✓。
        let affine = registry.call(
            &mut ctx,
            "set_group_transform",
            &json!({"group_id": "g1", "delta": {"matrix": [2, 0, 0, 2, 0, 0]}}),
        );
        assert_eq!(
            affine["ok"],
            json!(false),
            "非平移的组变换应被拒绝：{affine}"
        );

        // ③ 不存在的组 ✓ ⇒ 明确报"不存在"✓。
        let ghost = registry.call(
            &mut ctx,
            "set_group_transform",
            &json!({"group_id": "nope", "delta": {"dx": 1.0, "dy": 1.0}}),
        );
        assert_eq!(ghost["ok"], json!(false), "不存在的组应被拒绝：{ghost}");
    }
}
