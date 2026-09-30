//! 变更集的**读取与整体撤销**（设计 793：`revert_changeset` 整体撤销一个变更集 ✓）。
//!
//! **这一轮补的是我自己挖的坑** ✓：第 36 轮给 `detach_instance` 加了变更集 ✓
//!（两步原子归一个变更集 ✓），说明里写着"半成品能**一次整体撤销**" ✗ ——
//! 而**当时并没有撤销它的工具** ✗✓。`path_edit join` 第 39 轮又加了一处同样的承诺 ✓。
//! 承诺兑现不了，就等于把缺口写成了特性 ✓ ⇒ 本轮补上 ✓：
//! `get_changesets`（看得到 ✓）+ `revert_changeset`（撤得掉 ✓）。

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
    ToolContext::new(workspace, "doc_cs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace, x: f64, y: f64) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_cs", Bbox::new(x, y, 40.0, 40.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

fn object_ids(workspace: &mut Workspace, registry: &ToolRegistry) -> Vec<String> {
    let listed = {
        let mut ctx = context(workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    listed["objects"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["object_id"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn shape() -> serde_json::Value {
    json!({"geometry": {"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 20.0, "h": 20.0}},
           "color": {"r": 200, "g": 30, "b": 30, "a": 255}})
}

/// **脱离 ⇒ 整体撤销 ⇒ 实例回来、副本消失、画面复原** ✓ —— 这就是那句承诺的兑现 ✓。
#[test]
fn reverting_the_detach_changeset_restores_the_instance() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cs", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": shape()}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 120, 0], "pivot": [0, 0]}}),
        );
    }
    let ink_before = ink(&mut workspace, 116.0, 6.0);
    assert!(ink_before > 0, "脱离前实例处应有墨");
    let ids_before = object_ids(&mut workspace, &registry);
    assert!(ids_before.contains(&"mirror".to_owned()), "{ids_before:?}");

    let changeset = {
        let mut ctx = context(&mut workspace);
        let detached = registry.call(
            &mut ctx,
            "detach_instance",
            &json!({"instance_id": "mirror"}),
        );
        assert_eq!(detached["ok"], json!(true), "{detached}");
        detached["changeset_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    // ① **看得到** ✓：`get_changesets` 里应当有这个变更集 ✓，且含两条原子 ✓（建副本 ✓ + 删实例 ✓）。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_changesets", &json!({"limit": 5}))
    };
    let entry = listed["changesets"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["changeset_id"] == json!(changeset))
        })
        .cloned();
    let entry = entry.unwrap_or_else(|| panic!("变更集应出现在列表里：{listed}"));
    assert_eq!(entry["atoms"], json!(2), "脱离应当是两条原子：{entry}");

    // ② **撤得掉** ✓：整体撤销 ⇒ 副本消失 ✓、实例回来 ✓、**画面复原** ✓。
    let reverted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": changeset}),
        )
    };
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    assert_eq!(
        reverted["reverted"],
        json!(2),
        "两条原子都该被撤销：{reverted}"
    );
    let ids_after = object_ids(&mut workspace, &registry);
    assert!(
        ids_after.contains(&"mirror".to_owned()),
        "实例应当回来：{ids_after:?}"
    );
    assert!(
        !ids_after.contains(&"detached".to_owned())
            && !ids_after.iter().any(|id| id.starts_with("detached_")),
        "副本应当消失：{ids_after:?}"
    );
    assert_eq!(
        ink(&mut workspace, 116.0, 6.0),
        ink_before,
        "画面应当与脱离前完全一致（这才是「整体撤销」的意义）"
    );

    // ③ **"撤销一次撤销"目前走不通，而且必须如实报告** ✓ —— 这是本轮实测出来的**设计缺口** ✓：
    // 设计 793 写着 `revert(revert(x)) ≡ reapply(x)` ✓，折叠层**也预留了**这条语义 ✓，
    // 但**提交校验**拒绝"revert/reapply 一个 revert 原子" ✗（两句错误都实测到了 ✓）。
    // ⇒ 本工具**跳过历史原子并报告条数** ✓，不擅自决定放开哪一层 ✓。
    let revert_changeset = reverted["revert_changeset_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!revert_changeset.is_empty(), "{reverted}");
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": revert_changeset}),
        )
    };
    assert_eq!(again["ok"], json!(true), "应当成功返回并如实报告：{again}");
    assert_eq!(
        again["skipped_history_atoms"],
        json!(2),
        "两条 revert 原子都应被**跳过并计数**（而不是假装撤销成功）：{again}"
    );
    assert_eq!(again["reverted"], json!(0), "没有内容原子可撤：{again}");
    // 结果：实例**保持已恢复** ✓（没有把那次撤销悄悄回滚 ✗）。
    let ids_still = object_ids(&mut workspace, &registry);
    assert!(
        ids_still.contains(&"mirror".to_owned()),
        "跳过历史原子之后，状态不该被改变：{ids_still:?}"
    );
}

/// **`join` 的变更集同样可整体撤销** ✓ —— 两条笔迹都回来 ✓。
#[test]
fn reverting_the_join_changeset_brings_both_strokes_back() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cs", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        for (id, points) in [
            ("a", json!([[4.0, 10.0], [40.0, 10.0]])),
            ("b", json!([[60.0, 40.0], [120.0, 40.0]])),
        ] {
            registry.call(
                &mut ctx,
                "draw_stroke",
                &json!({"layer_id": "L", "object_id": id,
                        "data": {"points": points, "size": 6.0,
                                 "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
            );
        }
    }
    let changeset = {
        let mut ctx = context(&mut workspace);
        let joined = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "join", "object_id": "a", "other_id": "b"}),
        );
        assert_eq!(joined["ok"], json!(true), "{joined}");
        joined["changeset_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    assert!(
        !object_ids(&mut workspace, &registry).contains(&"b".to_owned()),
        "join 之后 b 应消失"
    );
    {
        let mut ctx = context(&mut workspace);
        let reverted = registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": changeset}),
        );
        assert_eq!(reverted["ok"], json!(true), "{reverted}");
    }
    let ids = object_ids(&mut workspace, &registry);
    assert_eq!(
        ids.iter().filter(|id| *id == "a" || *id == "b").count(),
        2,
        "撤销 join 之后两条笔迹都该在：{ids:?}"
    );
}

/// 不存在的变更集 ⇒ **明确报错** ✓，而不是静默成功 ✓。
#[test]
fn reverting_an_unknown_changeset_is_an_error() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_cs", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        let missing = registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": "01NOTAREALCHANGESET00000000"}),
        );
        assert_eq!(missing["ok"], json!(false), "{missing}");
        assert_eq!(
            missing["error_code"].as_str().unwrap_or_default(),
            "reference_not_found",
            "应给出 reference_not_found：{missing}"
        );
    }
}
