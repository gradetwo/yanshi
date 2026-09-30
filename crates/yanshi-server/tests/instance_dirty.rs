//! 实例的**脏区传播**（设计 544："结构 dirty 的传播闭包包括实例引用 master 的传播"）。
//!
//! **先取证再动手** ✓ —— 本轮开工前我怀疑这里有一个真 bug ✗：
//! 脏区规划用只吃 `&Object` 的 `object_bbox` ✓ ⇒ 实例包围盒**未知** ✓ ⇒
//! 会落进"整层失效"的兜底 ✓。查完发现：**正确性没问题** ✓（兜底保住了 ✓），
//! 只是**粒度很粗** ✓（一个实例动一下、整层重渲染 ✓）。
//! 所以本轮的判据不是"有没有残留像素" ✗，而是"**是否精确**" ✓ ——
//! 而这恰好也要**同时**做传播 ✓：一旦实例有了精确包围盒 ✓，
//! master 改动若**不**传播到实例 ✓，实例的旧像素就会留在屏幕上 ✗（精度与传播是一对改动 ✓）。

use serde_json::json;
use yanshi_core::{Atom, AtomKind, Bbox};
use yanshi_render::dirty::{dependents_of, plan_dirty, DirtyKind};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_idirty", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn state_of(workspace: &mut Workspace) -> yanshi_core::DocumentState {
    workspace
        .document_mut("doc_idirty")
        .expect("文档应存在")
        .state()
        .clone()
}

/// **master 移动 ⇒ 实例所在区域必须被失效** ✓，而且**不是整层兜底** ✓（精度 ✓）。
#[test]
fn moving_a_master_invalidates_the_instances_that_reference_it() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_idirty", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 20.0, "y": 20.0, "w": 24.0, "h": 24.0}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 120, 0], "pivot": [0, 0]}}),
        );
    }
    // 依赖查询 ✓（`get_dependency_graph` 用的就是它 ✓）。
    let before = state_of(&mut workspace);
    assert_eq!(
        dependents_of(&before, "master"),
        vec!["mirror".to_owned()],
        "master 的反向依赖应包含实例"
    );
    // 实例自己也要能查到它的依赖来源 ✓（反向查询只回答"谁依赖我" ✓，这是它的用途 ✓）。

    // 移动 master ✓（实例会**跟着**移动 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let moved = registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "master", "delta": {"dx": 0.0, "dy": 80.0}}),
        );
        assert_eq!(moved["ok"], json!(true), "{moved}");
    }
    let after = state_of(&mut workspace);
    let atom = Atom::new(
        AtomKind::Move,
        "human:1",
        "session:test",
        json!({"object_id": "master", "delta": {"dx": 0.0, "dy": 80.0}}),
    );
    let dirty = plan_dirty(&after, Some(&before), &atom);
    assert_eq!(
        dirty.kind,
        DirtyKind::Geometry,
        "应给出精确几何脏区（而不是整层/整幅兜底）：{dirty:?}"
    );
    let bbox = dirty.bbox.expect("几何脏区应有包围盒");
    // **旧位置**（master 与原实例位置 ✓）与**新位置**（都下移 80 ✓）都必须被覆盖 ✓。
    let covers = |x: f64, y: f64| {
        x >= bbox.x - 1.0
            && y >= bbox.y - 1.0
            && x <= bbox.x + bbox.w + 1.0
            && y <= bbox.y + bbox.h + 1.0
    };
    assert!(covers(20.0, 20.0), "master 旧位置应被覆盖：{bbox:?}");
    assert!(covers(20.0, 100.0), "master 新位置应被覆盖：{bbox:?}");
    assert!(
        covers(140.0, 20.0),
        "**实例旧位置**应被覆盖（这正是传播要解决的问题）：{bbox:?}"
    );
    assert!(covers(140.0, 100.0), "实例新位置应被覆盖：{bbox:?}");
    // 而且**不是整层** ✓：脏区应当明显小于整幅 ✓（实例的精度换来的就是这一点 ✓）。
    let layer = Bbox::new(0.0, 0.0, 256.0, 256.0);
    assert!(
        bbox.w * bbox.h < layer.w * layer.h * 0.5,
        "脏区应明显小于整层（实际 {bbox:?}）"
    );
}

/// **实例自身改动**也要给出精确脏区 ✓（此前会整层失效 ✓）。
#[test]
fn changing_an_instance_itself_is_precise() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_idirty", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 20.0, "y": 20.0, "w": 24.0, "h": 24.0}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 120, 0], "pivot": [0, 0]}}),
        );
    }
    let before = state_of(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let overridden = registry.call(
            &mut ctx,
            "update_override",
            &json!({"instance_id": "mirror",
                    "transform": {"matrix": [1, 0, 0, 1, 0, 60], "pivot": [0, 0]}}),
        );
        assert_eq!(overridden["ok"], json!(true), "{overridden}");
    }
    let after = state_of(&mut workspace);
    let atom = Atom::new(
        AtomKind::SetProperty,
        "human:1",
        "session:test",
        json!({"object_id": "mirror", "key": "override", "value": {"transform": {}}}),
    );
    let dirty = plan_dirty(&after, Some(&before), &atom);
    assert_eq!(
        dirty.kind,
        DirtyKind::Geometry,
        "实例改动也应是几何脏区：{dirty:?}"
    );
    let bbox = dirty.bbox.expect("应有包围盒");
    // 覆盖旧位置（local_transform 处 ✓）与新位置（override 之后 ✓）。
    assert!(bbox.y <= 24.0, "应覆盖实例的旧位置：{bbox:?}");
    assert!(
        bbox.y + bbox.h >= 100.0,
        "应覆盖实例的新位置（override 下移 60 ✓）：{bbox:?}"
    );
}
