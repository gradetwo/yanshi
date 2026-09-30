//! **复制图层** ✓（用户点名的图层功能之一 ✓，后端此前没有 ✗）。
//!
//! 断言集中在三件**容易做错**的事上 ✓：
//! ① 副本**连对象一起**复制（不是空壳 ✓）；
//! ② 副本落在原图层**正上方** ✓ —— 这需要同时提交完整 z 序 ✓，只给 `z_index + 1` 会**撞号** ✓，
//!    撞号后由 id 决定先后 ✓ ⇒ "有时在上面、有时在下面" ✗（最难察觉的一类错 ✓）；
//! ③ 一次变更集 ⇒ **一次撤销**干净 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_dup", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn layer_order(workspace: &mut Workspace, registry: &ToolRegistry) -> Vec<String> {
    let listed = {
        let mut ctx = context(workspace);
        registry.call(&mut ctx, "list_layers", &json!({}))
    };
    listed["layers"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["layer_id"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// **副本连对象一起复制，且正好在原图层上方** ✓。
#[test]
fn duplicating_a_layer_copies_its_objects_and_sits_directly_above() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_dup", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        // 底层 L1（两个对象 ✓）、上层 L2 ✓。
        for layer in ["L1", "L2"] {
            let created = registry.call(
                &mut ctx,
                "create_layer",
                &json!({"layer_id": layer, "name": layer}),
            );
            assert_eq!(created["ok"], json!(true), "{created}");
        }
        for (index, y) in [(0, 10.0), (1, 40.0)] {
            let drawn = registry.call(
                &mut ctx,
                "draw_shape",
                &json!({"layer_id": "L1", "object_id": format!("a{index}"), "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 10.0, "y": y, "w": 20.0, "h": 20.0}},
                    "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
            );
            assert_eq!(drawn["ok"], json!(true), "{drawn}");
        }
    }
    let before = layer_order(&mut workspace, &registry);
    assert_eq!(
        before,
        vec!["L1".to_owned(), "L2".to_owned()],
        "初始自下而上：L1, L2"
    );

    let duplicated = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "duplicate_layer",
            &json!({"layer_id": "L1", "new_layer_id": "L1copy"}),
        )
    };
    assert_eq!(duplicated["ok"], json!(true), "{duplicated}");
    assert_eq!(
        duplicated["objects"],
        json!(2),
        "两个对象都该被复制：{duplicated}"
    );
    assert_eq!(duplicated["name"], json!("L1 副本"), "{duplicated}");

    // ① **连对象一起** ✓：新图层上的对象数 = 2 ✓，且数据与原对象一致 ✓。
    let state = workspace.document_mut("doc_dup").unwrap().state().clone();
    let copied: Vec<&yanshi_core::state::Object> = state
        .objects
        .values()
        .filter(|object| !object.is_deleted() && object.layer_id == "L1copy")
        .collect();
    assert_eq!(copied.len(), 2, "副本图层上应有两个对象");
    for object in &copied {
        let source = state
            .objects
            .values()
            .find(|other| {
                !other.is_deleted()
                    && other.layer_id == "L1"
                    && other.data.get("geometry") == object.data.get("geometry")
            })
            .expect("副本对象应在原图层上找到对应者");
        assert_eq!(
            object.data, source.data,
            "复制的对象应与原对象同数据 ✓（共享同一批 blob ✓）"
        );
    }

    // ② **正好在原图层上方** ✓（自下而上：L1, L1copy, L2 ✓）。
    let after = layer_order(&mut workspace, &registry);
    assert_eq!(
        after,
        vec!["L1".to_owned(), "L1copy".to_owned(), "L2".to_owned()],
        "副本应插在原图层**正上方**（而不是最顶或最底，也不该与 L2 撞号）"
    );

    // ③ **一次撤销** ✓：副本图层与它的对象都消失 ✓，顺序复原 ✓。
    let changeset = duplicated["changeset_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    {
        let mut ctx = context(&mut workspace);
        let undone = registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": changeset}),
        );
        assert_eq!(undone["ok"], json!(true), "{undone}");
    }
    assert_eq!(
        layer_order(&mut workspace, &registry),
        before,
        "撤销后顺序应复原"
    );
    let state = workspace.document_mut("doc_dup").unwrap().state().clone();
    let alive_copies = state
        .objects
        .values()
        .filter(|object| !object.is_deleted() && object.layer_id == "L1copy")
        .count();
    assert_eq!(alive_copies, 0, "撤销后副本的对象应消失");
}

/// **复制是读操作** ✓：锁定不该挡住它 ✓；而副本**忠实保留**锁定状态 ✓（记录的选择 ✓）。
#[test]
fn duplicating_a_locked_layer_works_and_keeps_the_lock() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_dup", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": "L1", "name": "底稿"}),
        );
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L1", "object_id": "a0", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}},
                "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
        );
        let locked = registry.call(&mut ctx, "lock_layer", &json!({"layer_id": "L1"}));
        assert_eq!(locked["ok"], json!(true), "{locked}");
        // **复制被锁的图层 → 应当成功** ✓（读操作 ✓，与"改内容"不同 ✓）。
        let duplicated = registry.call(
            &mut ctx,
            "duplicate_layer",
            &json!({"layer_id": "L1", "new_layer_id": "L1b"}),
        );
        assert_eq!(
            duplicated["ok"],
            json!(true),
            "复制不该被锁挡住：{duplicated}"
        );
        // 副本**保留**锁定 ✓（忠实复制 ✓）。
        let listed = registry.call(&mut ctx, "list_layers", &json!({}));
        let copy = listed["layers"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["layer_id"] == json!("L1b")))
            .cloned()
            .expect("副本应出现在图层列表里");
        assert_eq!(
            copy["locked"],
            json!(true),
            "副本应忠实保留锁定状态：{copy}"
        );
        // 而**改副本内容**仍然要被锁挡住 ✓（说明副本的锁是真锁 ✓）。
        let refused = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L1b", "object_id": "nope", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 1.0, "y": 1.0, "w": 4.0, "h": 4.0}},
                "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
        );
        assert_eq!(
            refused["ok"],
            json!(false),
            "副本继承了锁 ⇒ 改内容应被拒：{refused}"
        );
    }
}
