//! **图层/对象锁定必须被强制** ✓（本轮补齐 ✓）。
//!
//! **开工前的实测** ✓：`locked` 只被**记录与回报** ✓，内核对它**没有任何检查** ✗ ——
//! 也就是说"锁定图层"此前是个**点了没用的标签** ✗：锁了照样能画、能删 ✓。
//! 这正是本项目一路在消灭的"**静默失效**" ✓。
//!
//! 校验放在**提交层**（`log::validate_commit` ✓）而不是 `fold::precondition` ✓：
//! 后者返回 `Err` 只**警告并跳过** ✓，只有前者才**拒绝提交** ✓（这个亏本项目吃过多次 ✓）。
//!
//! 规则（少而清楚 ✓）：对象自身或其图层被锁 ⇒ 拒绝修改 ✓；
//! 例外是 `locked` / `visible` 这两个**管理性属性** ✓（否则永远解不开锁 ✓）；
//! **图层自己的**改名/可见性/不透明度/顺序**不受**图层锁影响 ✓（锁的是内容 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_lock", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn draw(workspace: &mut Workspace, registry: &ToolRegistry, id: &str) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": id,
                "data": {"points": [[10.0, 10.0], [40.0, 40.0]], "size": 5.0,
                         "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    )
}

fn setup(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    drop(ctx);
    let first = draw(workspace, registry, "s1");
    assert_eq!(first["ok"], json!(true), "{first}");
}

/// **锁住图层之后必须拒绝修改** ✓，而且**错误要说清为什么** ✓。
#[test]
fn a_locked_layer_refuses_edits_and_says_why() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_lock", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        let locked = registry.call(&mut ctx, "lock_layer", &json!({"layer_id": "L"}));
        assert_eq!(locked["ok"], json!(true), "{locked}");
    }
    // 各种"改内容"的操作都应被拒绝 ✓。
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "落笔",
            json!({"layer_id": "L", "object_id": "s2",
                   "data": {"points": [[5.0, 5.0], [20.0, 20.0]], "size": 4.0,
                            "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        ),
        (
            "改已有对象",
            json!({"object_id": "s1", "key": "z_index", "value": 3}),
        ),
        ("删除对象", json!({"object_id": "s1"})),
        (
            "移动对象",
            json!({"object_id": "s1", "delta": {"dx": 5.0, "dy": 5.0}}),
        ),
    ];
    for (label, args) in cases {
        let mut ctx = context(&mut workspace);
        let result = match label {
            "落笔" => registry.call(&mut ctx, "draw_stroke", &args),
            "改已有对象" => registry.call(&mut ctx, "set_property", &args),
            "删除对象" => registry.call(&mut ctx, "delete_object", &args),
            _ => registry.call(&mut ctx, "move_object", &args),
        };
        assert_eq!(
            result["ok"],
            json!(false),
            "{label} 在锁定图层上应被拒绝：{result}"
        );
        assert_eq!(
            result["error_code"].as_str().unwrap_or_default(),
            "permission_denied",
            "{label} 应给出 permission_denied：{result}"
        );
        let detail = result["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("锁定"),
            "{label} 的错误应提到锁定：{detail}"
        );
    }
    // **图层自己的管理属性仍然可以改** ✓（锁的是内容 ✓，不是图层的管理属性 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let renamed = registry.call(
            &mut ctx,
            "update_layer",
            &json!({"layer_id": "L", "patch": {"name": "底稿", "visible": false, "opacity": 0.5}}),
        );
        assert_eq!(
            renamed["ok"],
            json!(true),
            "锁定不该妨碍改名/可见性/不透明度：{renamed}"
        );
        // **解锁必须可行** ✓（否则这个功能就是单向陷阱 ✓）。
        let unlocked = registry.call(&mut ctx, "unlock_layer", &json!({"layer_id": "L"}));
        assert_eq!(unlocked["ok"], json!(true), "{unlocked}");
        let visible = registry.call(
            &mut ctx,
            "update_layer",
            &json!({"layer_id": "L", "patch": {"visible": true}}),
        );
        assert_eq!(visible["ok"], json!(true), "{visible}");
    }
    // 解锁之后又能画了 ✓。
    let after = draw(&mut workspace, &registry, "s3");
    assert_eq!(after["ok"], json!(true), "解锁后应能继续编辑：{after}");
}

/// **对象自身的锁**同样生效 ✓，而且**可以通过改 `locked` 解开** ✓（唯一的出口 ✓）。
#[test]
fn a_locked_object_refuses_edits_but_can_be_unlocked() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_lock", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        let locked = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "s1", "key": "locked", "value": true}),
        );
        assert_eq!(locked["ok"], json!(true), "{locked}");
        // 改内容 ⇒ 拒绝 ✓。
        let refused = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "s1", "key": "z_index", "value": 9}),
        );
        assert_eq!(refused["ok"], json!(false), "{refused}");
        assert!(
            refused["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("对象自身已锁定"),
            "{refused}"
        );
        // **改可见性仍然允许** ✓（管理性属性 ✓）。
        let visible = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "s1", "key": "visible", "value": false}),
        );
        assert_eq!(visible["ok"], json!(true), "可见性不该被锁挡住：{visible}");
        // **解锁** ✓。
        let unlocked = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "s1", "key": "locked", "value": false}),
        );
        assert_eq!(unlocked["ok"], json!(true), "{unlocked}");
        let after = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "s1", "key": "z_index", "value": 2}),
        );
        assert_eq!(after["ok"], json!(true), "解锁后应能改：{after}");
    }
}

/// **在被锁定的图层上新建对象**也要被拒 ✓（锁住的图层不该长出新东西 ✓）。
#[test]
fn a_locked_layer_refuses_new_objects() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_lock", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    setup(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "lock_layer", &json!({"layer_id": "L"}));
        let refused = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "box", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 4.0, "y": 4.0, "w": 8.0, "h": 8.0}},
                "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
        );
        assert_eq!(refused["ok"], json!(false), "{refused}");
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("不能在锁定"),
            "错误应说明是「不能在锁定图层上新建」：{detail}"
        );
    }
}
