//! `path_edit` 的 `boolean`（设计 792 的最后一个算子 ✓）。
//!
//! **设计只给了名字 ⇒ 记录选择** ✓：模式由参数给出（`union`/`intersect`/`subtract`/`xor` ✓）——
//! 四种运算显然都需要 ✓，与其发明四个工具名 ✗ 不如一个工具加一个模式 ✓；
//! 来源可以是路径 ✓、笔迹 ✓ 或既有多边形形状 ✓；结果落成**形状对象** ✓（多环就多个 ✓）；
//! 新建结果 + tombstone 两个输入 ✓，**全部一个变更集** ✓ ⇒ 一次可撤回 ✓。
//!
//! **退化输入明确报错** ✓（交点在顶点上 ✓、共线重叠 ✓、零面积 ✓）—— 几何层**拒绝** ✓，
//! 错误原文直接回给调用方 ✓。**绝不给出"看起来对、其实错"的多边形** ✗。
//!
//! 断言里的面积**从几何推导** ✓（容斥原理 ✓），不写死"结果多边形应该长什么样" ✗。

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
    ToolContext::new(workspace, "doc_bool", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace) -> usize {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_bool", Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("区域渲染应成功");
    bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

fn polygon(id: &str, points: serde_json::Value) -> serde_json::Value {
    json!({"layer_id": "L", "object_id": id, "data": {
        "geometry": {"kind": "polygon", "points": points},
        "color": {"r": 30, "g": 30, "b": 30, "a": 255}}})
}

/// 建两个 10×10 的方块 ✓（错开 4 ✓ ⇒ 交叠 6×6 = 36 ✓）。
fn two_overlapping_squares(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    for (id, x, y) in [("a", 20.0, 20.0), ("b", 24.0, 24.0)] {
        let made = registry.call(
            &mut ctx,
            "draw_shape",
            &polygon(
                id,
                json!([
                    [x, y],
                    [x + 100.0, y],
                    [x + 100.0, y + 100.0],
                    [x, y + 100.0]
                ]),
            ),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
}

fn boolean(
    workspace: &mut Workspace,
    registry: &ToolRegistry,
    mode: &str,
    result: &str,
) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry.call(
        &mut ctx,
        "path_edit",
        &json!({"op": "boolean", "object_id": "a", "other_id": "b",
                "mode": mode, "result_id": result}),
    )
}

/// **四种模式的面积都符合容斥关系** ✓（交 36 ✓、并 164 ✓、差 64 ✓、异或 128 ✓）。
#[test]
fn four_modes_produce_the_areas_the_arithmetic_predicts() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bool", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    // 期望值**从几何推导** ✓：两个 100×100 的方块错开 4 ⇒ 交叠 96×96 = 9216 ✓。
    let (side, shift) = (100.0_f64, 4.0_f64);
    let overlap = (side - shift) * (side - shift);
    let single = side * side;
    for (mode, expected) in [
        ("intersect", overlap),
        ("union", single * 2.0 - overlap),
        ("subtract", single - overlap),
        ("xor", (single * 2.0 - overlap) - overlap),
    ] {
        // 同理：函数顶部的 `workspace` 已经遮蔽了助手函数 ✓ ⇒ 直接构造 ✓。
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        workspace
            .create_document(
                NewDocument::new("doc_bool", 200, 200),
                "human:1",
                "session:test",
            )
            .unwrap();
        two_overlapping_squares(&mut workspace, &registry);
        let result = boolean(&mut workspace, &registry, mode, "res");
        assert_eq!(result["ok"], json!(true), "{mode}：{result}");
        let area = result["area"].as_f64().unwrap_or(0.0);
        assert!(
            (area - expected).abs() < expected * 0.02,
            "{mode} 的面积应约为 {expected}（容斥推导 ✓），实得 {area}"
        );
        // **环数按模式而定** ✓ —— 我第一版一律断言"一个环" ✗ ⇒ 异或当场失败 ✓：
        // 两个交叠方块的**对称差**天然是**两块不相交的 L 形** ✓ ⇒ **2 个环** ✓
        //（面积 1568 ✓ 正是容斥推导值 ✓，所以行为完全正确 ✓）。
        // 这条断言顺带把"支持多环结果"这件事**钉在测试里** ✓。
        let expected_rings = if mode == "xor" { 2 } else { 1 };
        assert_eq!(
            result["rings"],
            json!(expected_rings),
            "{mode} 的环数：{result}"
        );
        if mode == "xor" {
            assert_eq!(
                result["results"][1],
                json!("res_2"),
                "多环应各自落成对象：{result}"
            );
        }
        let state = workspace.document_mut("doc_bool").unwrap().state().clone();
        let live = |id: &str| {
            state
                .objects
                .get(id)
                .map(|o| !o.is_deleted())
                .unwrap_or(false)
        };
        assert!(live("res"), "{mode}：结果应在");
        assert!(!live("a") && !live("b"), "{mode}：两个输入应退役");
        // **渲染墨量与面积同比例** ✓（墨量 ≈ 面积 × 每单位墨 ✓ ⇒ 比值应与面积比一致 ✓）。
        let ink_a = {
            // **不能写 `workspace()`/`registry()`** ✗：循环里已有同名局部变量 ✓（本会话第 N 次 ✓）
            // ⇒ 直接构造 ✓。
            let mut w2 = Workspace::in_memory(DocumentSettings::default());
            w2.create_document(
                NewDocument::new("doc_bool", 200, 200),
                "human:1",
                "session:test",
            )
            .unwrap();
            let r2 = ToolRegistry::with_profiles(&Profile::ALL);
            let mut ctx = context(&mut w2);
            r2.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
            r2.call(
                &mut ctx,
                "draw_shape",
                &polygon(
                    "only",
                    json!([[20.0, 20.0], [120.0, 20.0], [120.0, 120.0], [20.0, 120.0]]),
                ),
            );
            ink(&mut w2)
        };
        let ink_result = ink(&mut workspace);
        let ratio = ink_result as f64 / ink_a as f64;
        let expected_ratio = expected / single;
        assert!(
            (ratio - expected_ratio).abs() < expected_ratio * 0.05,
            "{mode}：墨量比应约为 {expected_ratio:.3}（单个方块的墨量 {ink_a} ⇒ 结果 {ink_result}）"
        );
    }
}

/// **一步撤销 ⇒ 两个输入回来、结果消失、画面复原** ✓。
#[test]
fn the_boolean_is_withdrawn_in_one_action() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bool", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    two_overlapping_squares(&mut workspace, &registry);
    let before = ink(&mut workspace);
    let result = boolean(&mut workspace, &registry, "union", "res");
    assert_eq!(result["ok"], json!(true), "{result}");
    let changeset = result["changeset_id"]
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
    let state = workspace.document_mut("doc_bool").unwrap().state().clone();
    let live = |id: &str| {
        state
            .objects
            .get(id)
            .map(|o| !o.is_deleted())
            .unwrap_or(false)
    };
    assert!(live("a") && live("b"), "撤销后两个输入应回来");
    assert!(!live("res"), "撤销后结果应消失");
    assert_eq!(ink(&mut workspace), before, "撤销后画面应复原");
}

/// **退化输入与错误模式都必须明确报错** ✓ —— 尤其是**共线重叠** ✓（G–H 的已知失效情形 ✓）。
#[test]
fn degenerate_inputs_and_bad_modes_are_refused_with_reasons() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bool", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // **共享一条边**的两个方块 ✓ ⇒ 共线重叠 ⇒ 必须拒绝 ✓。
        for (id, x) in [("a", 20.0), ("b", 120.0)] {
            registry.call(
                &mut ctx,
                "draw_shape",
                &polygon(
                    id,
                    json!([[x, 20.0], [x + 100.0, 20.0], [x + 100.0, 120.0], [x, 120.0]]),
                ),
            );
        }
        let refused = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "boolean", "object_id": "a", "other_id": "b",
                    "mode": "union", "result_id": "x"}),
        );
        assert_eq!(refused["ok"], json!(false), "共线重叠应被拒绝：{refused}");
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("共线") || detail.contains("退化") || detail.contains("无法进行"),
            "错误应说清原因（共线重叠 / 退化）：{detail}"
        );

        // **未知模式** ⇒ 明确列出可用模式 ✓。
        let bad_mode = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "boolean", "object_id": "a", "other_id": "b", "mode": "wat"}),
        );
        assert_eq!(bad_mode["ok"], json!(false), "{bad_mode}");
        let detail = bad_mode["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        for mode in ["union", "intersect", "subtract", "xor"] {
            assert!(detail.contains(mode), "错误应列出可用模式 {mode}：{detail}");
        }

        // **矩形（非多边形）形状** ⇒ 明确提示先 convert_to_shape ✓。
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "rect", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 10.0, "y": 10.0, "w": 8.0, "h": 8.0}},
                "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
        );
        let not_polygon = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "boolean", "object_id": "rect", "other_id": "a", "mode": "union"}),
        );
        assert_eq!(not_polygon["ok"], json!(false), "{not_polygon}");
        assert!(
            not_polygon["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("convert_to_shape"),
            "错误应提示先转多边形：{not_polygon}"
        );
    }
}

/// **路径与笔迹都能作为来源** ✓（几何走渲染端同一套铺平 ✓）。
#[test]
fn paths_and_strokes_can_be_boolean_operands() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_bool", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 形状 A ✓。
        registry.call(
            &mut ctx,
            "draw_shape",
            &polygon(
                "a",
                json!([[20.0, 20.0], [120.0, 20.0], [120.0, 120.0], [20.0, 120.0]]),
            ),
        );
        // B 是**路径** ✓（三角形 ✓，与 A 有重叠 ✓）。
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "seed",
                    "data": {"points": [[80.0, 40.0], [160.0, 40.0]], "size": 4.0,
                             "color": {"r": 30, "g": 30, "b": 30, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "convert_to_path",
            &json!({"object_id": "seed", "path_id": "b"}),
        );
        registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "b", "key": "nodes", "value": [
                {"x": 80.0, "y": 40.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                {"x": 160.0, "y": 40.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                {"x": 120.0, "y": 140.0, "in": [0.0, 0.0], "out": [0.0, 0.0]}
            ]}),
        );
        let result = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "boolean", "object_id": "a", "other_id": "b",
                    "mode": "intersect", "result_id": "overlap"}),
        );
        assert_eq!(result["ok"], json!(true), "形状 ∩ 路径应可行：{result}");
        let area = result["area"].as_f64().unwrap_or(0.0);
        assert!(area > 0.0, "交集应有面积：{result}");
        assert_eq!(result["results"][0], json!("overlap"), "{result}");
    }
}
