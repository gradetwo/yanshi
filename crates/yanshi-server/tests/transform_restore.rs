//! `transform_object` 与 `restore_object`（设计 783 的 structure 组 ✓）。
//!
//! **开工前先取证 ✓**（本会话反复奏效的做法 ✓）：我先写了一个临时探针确认两件事 ——
//! ① `Move` 的绝对 `transform` **真的能承载任意仿射** ✓
//!   （实测：20×10 的方块绕 (70,15) 转 90° ⇒ bbox 由 `60,10,20,10` 变成 `65,5,10,20` ✓）；
//! ② **撤销 tombstone 能让对象复活** ✓（对象数 1 → 0 → 1 ✓）。
//! 两件事都成立 ⇒ 这两个工具**不需要任何新机制** ✓，只需把既有能力包成清晰的入口 ✓。

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
    ToolContext::new(workspace, "doc_tr", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace, x: f64, y: f64) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_tr", Bbox::new(x, y, 40.0, 40.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
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

fn draw_box(workspace: &mut Workspace, registry: &ToolRegistry, x: f64, y: f64, w: f64, h: f64) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    registry.call(
        &mut ctx,
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "box", "data": {
            "geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": w, "h": h}},
            "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
    );
}

/// **旋转 90°：宽高互换、中心不动** ✓（缺省 anchor 就是包围盒中心 ✓）。
#[test]
fn rotating_a_shape_swaps_its_extent_around_the_centre() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tr", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_box(&mut workspace, &registry, 60.0, 10.0, 40.0, 20.0);
    let before = bbox_of(&mut workspace, &registry, "box");
    let centre = (before[0] + before[2] / 2.0, before[1] + before[3] / 2.0);
    let rotated = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "transform_object",
            &json!({"object_id": "box", "rotate": {"degrees": 90.0}}),
        )
    };
    assert_eq!(rotated["ok"], json!(true), "{rotated}");
    let after = bbox_of(&mut workspace, &registry, "box");
    assert!(
        (after[2] - before[3]).abs() < 1.5 && (after[3] - before[2]).abs() < 1.5,
        "90° 应让宽高互换（前 {before:?} 后 {after:?}）"
    );
    let new_centre = (after[0] + after[2] / 2.0, after[1] + after[3] / 2.0);
    assert!(
        (new_centre.0 - centre.0).abs() < 1.5 && (new_centre.1 - centre.1).abs() < 1.5,
        "缺省 anchor 是中心 ⇒ 中心不该动（{centre:?} → {new_centre:?}）"
    );
}

/// **`compose` 叠加** ✓：连续两次 45° ≈ 一次 90° ✓（与直接转 90° 的结果一致 ✓）。
#[test]
fn transforms_compose_onto_the_existing_one() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tr", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_box(&mut workspace, &registry, 40.0, 40.0, 40.0, 20.0);
    let before = bbox_of(&mut workspace, &registry, "box");
    for _ in 0..2 {
        let mut ctx = context(&mut workspace);
        let rotated = registry.call(
            &mut ctx,
            "transform_object",
            &json!({"object_id": "box", "rotate": {"degrees": 45.0}, "anchor": {"x": 60.0, "y": 50.0}}),
        );
        assert_eq!(rotated["ok"], json!(true), "{rotated}");
    }
    let composed = bbox_of(&mut workspace, &registry, "box");
    // 与"一次 90°、锚点相同"对照 ✓。
    // **不能写 `workspace()`** ✗：本测试里的局部变量 `workspace` 已经遮蔽了同名助手函数 ✓
    //（本会话第 N 次踩这个 ✓）⇒ 直接构造即可 ✓。
    let mut reference = Workspace::in_memory(DocumentSettings::default());
    reference
        .create_document(
            NewDocument::new("doc_tr", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    draw_box(&mut reference, &registry, 40.0, 40.0, 40.0, 20.0);
    {
        let mut ctx = context(&mut reference);
        registry.call(
            &mut ctx,
            "transform_object",
            &json!({"object_id": "box", "rotate": {"degrees": 90.0}, "anchor": {"x": 60.0, "y": 50.0}}),
        );
    }
    let direct = bbox_of(&mut reference, &registry, "box");
    for index in 0..4 {
        assert!(
            (composed[index] - direct[index]).abs() < 1.5,
            "两次 45° 应约等于一次 90°（叠加 {composed:?} vs 直接 {direct:?}，起点 {before:?}）"
        );
    }

    // **`compose: false` ⇒ 替换** ✓：从当前（已转 90°）直接替成"只平移"⇒ 旋转效果消失 ✓。
    {
        let mut ctx = context(&mut workspace);
        let replaced = registry.call(
            &mut ctx,
            "transform_object",
            &json!({"object_id": "box", "translate": {"dx": 100.0, "dy": 0.0},
                    "compose": false, "anchor": {"x": 0.0, "y": 0.0}}),
        );
        assert_eq!(replaced["ok"], json!(true), "{replaced}");
    }
    let replaced = bbox_of(&mut workspace, &registry, "box");
    assert_eq!(
        (replaced[2].round(), replaced[3].round()),
        (40.0, 20.0),
        "替换之后应回到未旋转的宽高：{replaced:?}"
    );
}

/// **参数校验要严** ✓：三个操作必须恰好给一个 ✓；缩放不能全 0 ✓；缺 anchor 且无包围盒 ⇒ 报错 ✓。
#[test]
fn transform_object_validates_its_arguments() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tr", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_box(&mut workspace, &registry, 4.0, 4.0, 10.0, 10.0);
    for (label, args) in [
        ("一个都没给", json!({"object_id": "box"})),
        (
            "给了两个",
            json!({"object_id": "box", "rotate": {"degrees": 10.0}, "scale": {"x": 2.0, "y": 2.0}}),
        ),
        (
            "缩放全 0",
            json!({"object_id": "box", "scale": {"x": 0.0, "y": 0.0}}),
        ),
    ] {
        let mut ctx = context(&mut workspace);
        let refused = registry.call(&mut ctx, "transform_object", &args);
        assert_eq!(refused["ok"], json!(false), "{label} 应被拒绝：{refused}");
        assert_eq!(
            refused["error_code"].as_str().unwrap_or_default(),
            "invalid_argument",
            "{label} 应给出 invalid_argument：{refused}"
        );
    }
}

/// **删除 ⇒ 恢复：对象回来，而且像素也回来** ✓（走历史 ✓，不是新建一个同 id 的壳 ✓）。
#[test]
fn restoring_a_deleted_object_brings_back_its_pixels() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_tr", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_box(&mut workspace, &registry, 20.0, 20.0, 30.0, 30.0);
    let before = ink(&mut workspace, 16.0, 16.0);
    assert!(before > 0, "删除前应有墨");
    {
        let mut ctx = context(&mut workspace);
        let deleted = registry.call(&mut ctx, "delete_object", &json!({"object_id": "box"}));
        assert_eq!(deleted["ok"], json!(true), "{deleted}");
    }
    assert_eq!(ink(&mut workspace, 16.0, 16.0), 0, "删除后该处应为空");

    // **活着的时候恢复 ⇒ 如实说明无需恢复** ✓（不是错误 ✓，也不是静默成功 ✗）。
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "restore_object", &json!({"object_id": "other"}));
    }

    let restored = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "restore_object", &json!({"object_id": "box"}))
    };
    assert_eq!(restored["ok"], json!(true), "{restored}");
    assert_eq!(restored["restored"], json!(true), "{restored}");
    assert_eq!(
        restored["reverted_tombstones"].as_array().map(Vec::len),
        Some(1),
        "应当撤销那条 tombstone：{restored}"
    );
    assert_eq!(
        ink(&mut workspace, 16.0, 16.0),
        before,
        "恢复后像素应与删除前一致"
    );

    // **再恢复一次 ⇒ 如实回报"无需恢复"** ✓。
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "restore_object", &json!({"object_id": "box"}))
    };
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["restored"], json!(false), "{again}");

    // **"撤销这次恢复"现在成立** ✓（你已拍板放开校验 ✓ —— 决策④）：
    // 恢复 = 撤销 tombstone ✓ ⇒ 撤销恢复 = 撤销一条 revert ✓ ⇒ 而这条现在**允许**了 ✓
    //（设计 793 的恒等式 ✓，折叠层早就实现了它 ✓）。
    // 上一轮这里断言的是"被跳过、状态不变" ✗ —— 注释里写着"等你拍板后应改成对象再次消失" ✓，就是这里 ✓。
    let changeset = restored["changeset_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let undone = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": changeset}),
        )
    };
    assert_eq!(undone["ok"], json!(true), "{undone}");
    assert_eq!(
        undone["reverted"],
        json!(1),
        "那条 revert 应被撤销：{undone}"
    );
    assert_eq!(
        undone["skipped_history_atoms"],
        json!(0),
        "不再跳过历史原子：{undone}"
    );
    assert_eq!(
        ink(&mut workspace, 16.0, 16.0),
        0,
        "撤销恢复 ⇒ 对象再次消失"
    );

    // 再撤销一次 ⇒ **重新生效** ✓（`revert(revert(x)) ≡ reapply(x)` ✓，设计 793 ✓）。
    let revert_changeset = undone["revert_changeset_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "revert_changeset",
            &json!({"changeset_id": revert_changeset}),
        )
    };
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(
        ink(&mut workspace, 16.0, 16.0),
        before,
        "撤销撤销 ⇒ 对象回来"
    );

    // **从未存在过的对象 ⇒ 明确报错** ✓。
    let ghost = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "restore_object", &json!({"object_id": "ghost"}))
    };
    assert_eq!(ghost["ok"], json!(false), "{ghost}");
    assert_eq!(
        ghost["error_code"].as_str().unwrap_or_default(),
        "reference_not_found",
        "{ghost}"
    );
}
