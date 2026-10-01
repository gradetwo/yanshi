//! `convert_to_shape`（设计 792 ✓）—— 路径/笔迹 ⇒ **多边形形状** ✓。
//!
//! **设计未规定语义 ⇒ 记录选择** ✓：
//! * 形状取 **`polygon`** ✓（形状本来就有这一种 ✓，而多边形**天然闭合** ✓ ⇒ 最直白的读法 ✓）；
//! * **顶点复用渲染端的铺平函数** ✓（`yanshi_render::object::flatten_path` ✓）——
//!   本项目吃过"包围盒一处一套、渲染一处一套"的亏 ✓ ⇒ 形状的顶点与渲染所见**必然一致** ✓；
//! * 路径与笔迹**都接受** ✓；样式原样保留 ✓（填充色取 `color` ✓）。
//!
//! 断言落在**用户看得见**的量上：形状会把路径"填实" ⇒ 墨量**大幅增加** ✓ 且颜色一致 ✓。

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
    ToolContext::new(workspace, "doc_c2s", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn pixels(workspace: &mut Workspace) -> Vec<u8> {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_c2s", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    bytes
}

fn ink(workspace: &mut Workspace) -> usize {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_c2s", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

fn colors(workspace: &mut Workspace) -> std::collections::BTreeSet<(u8, u8, u8)> {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_c2s", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("区域渲染应成功");
    bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .map(|pixel| (pixel[0], pixel[1], pixel[2]))
        .collect()
}

/// "这个对象还在吗" ✓ —— **两种"没了"都算** ✓（见下面的说明 ✓）。
fn is_gone(workspace: &mut Workspace, object_id: &str) -> bool {
    let kind = object_type(workspace, object_id);
    kind == "missing" || kind == "deleted"
}

fn object_type(workspace: &mut Workspace, object_id: &str) -> String {
    let state = workspace
        .document_mut("doc_c2s")
        .expect("文档应存在")
        .state()
        .clone();
    // **"没了"有两种表现** ✓：不在状态里（`missing` ✓）或仍在但已 tombstone（`deleted` ✓）——
    // 后者取决于该对象是否还有别的原子引用它 ✓。断言"消失"时两者都算 ✓
    //（我第一版只认 `missing` ✗ ⇒ 撤销用例失败 ✓，而行为其实是对的 ✓）。
    state
        .objects
        .get(object_id)
        .map(|object| {
            if object.is_deleted() {
                "deleted".to_owned()
            } else {
                format!("{:?}", object.object_type).to_lowercase()
            }
        })
        .unwrap_or_else(|| "missing".to_owned())
}

/// 画一个**三角形**路径 ✓（经"落笔 ⇒ 转路径 ⇒ 改 nodes" ✓）。
fn draw_triangle(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "seed",
                "data": {"points": [[20.0, 20.0], [140.0, 20.0]], "size": 4.0, "hardness": 1.0,
                         "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let converted = registry.call(
        &mut ctx,
        "convert_to_path",
        &json!({"object_id": "seed", "path_id": "p1"}),
    );
    assert_eq!(converted["ok"], json!(true), "{converted}");
    let set = registry.call(
        &mut ctx,
        "set_property",
        &json!({"object_id": "p1", "key": "nodes", "value": [
            {"x": 20.0, "y": 20.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
            {"x": 140.0, "y": 20.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
            {"x": 80.0, "y": 100.0, "in": [0.0, 0.0], "out": [0.0, 0.0]}
        ]}),
    );
    assert_eq!(set["ok"], json!(true), "{set}");
}

/// **路径 ⇒ 形状会把轮廓"填实"** ✓：墨量大增 ✓、颜色不变 ✓、类型变成 shape ✓、包围盒一致 ✓。
#[test]
fn converting_a_path_to_a_shape_fills_the_outline() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_c2s", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_triangle(&mut workspace, &registry);
    let outline_ink = ink(&mut workspace);
    let outline_colors = colors(&mut workspace);
    assert!(outline_ink > 0, "三角形轮廓应有墨");
    assert_eq!(
        object_type(&mut workspace, "p1"),
        "path",
        "转换前应当是路径"
    );

    let converted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "convert_to_shape",
            &json!({"object_id": "p1", "shape_id": "tri"}),
        )
    };
    assert_eq!(converted["ok"], json!(true), "{converted}");
    assert!(
        converted["vertices"].as_u64().unwrap_or(0) >= 3,
        "顶点数应至少三个：{converted}"
    );
    assert_eq!(
        object_type(&mut workspace, "tri"),
        "shape",
        "转换后应当是形状"
    );
    assert!(
        is_gone(&mut workspace, "p1"),
        "原路径应当退役（missing 或 deleted ✓）"
    );

    let filled_ink = ink(&mut workspace);
    assert!(
        filled_ink > outline_ink * 3,
        "填实之后墨量应大幅增加（轮廓 {outline_ink} ⇒ 填充 {filled_ink}）"
    );
    // **颜色一致** ✓：形状沿用原来的 `color` ✓。
    //
    // **注意** ✓：多边形边缘是**抗锯齿**的 ✓ ⇒ 填充里必然出现"基色与背景混合"的中间色 ✓
    //（实测新增了 (204,77,77)、(212,123,123)、(223,166,166) ✓）。
    // 我第一版断言"填充色是轮廓色的**子集**" ✗ ⇒ 与抗锯齿的事实不符 ✓。
    // 正确的断言是：**基色仍在** ✓ + **颜色种类很少** ✓（只有抗锯齿的过渡 ✓，不是换了颜色 ✓）。
    let filled_colors = colors(&mut workspace);
    assert!(
        filled_colors.contains(&(200, 30, 30)),
        "填充应保留原来的基色 (200,30,30)：{filled_colors:?}"
    );
    assert!(
        filled_colors.len() <= outline_colors.len() + 8,
        "填充只应新增抗锯齿过渡色（原 {} 种 ⇒ 现 {} 种）",
        outline_colors.len(),
        filled_colors.len()
    );
    // **顶点与渲染口径一致** ✓：形状的包围盒应等于路径的包围盒 ✓。
    let (_, _, bytes) = workspace
        .render_region_raw("doc_c2s", Bbox::new(0.0, 0.0, 160.0, 120.0))
        .expect("渲染");
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for (index, pixel) in bytes.chunks_exact(4).enumerate() {
        if pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200) {
            let x = (index % 160) as u32;
            let y = (index / 160) as u32;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    let bbox = converted["bbox"].as_array().cloned().unwrap_or_default();
    let value = |index: usize| {
        bbox.get(index)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    assert!(
        (value(0) - min_x as f64).abs() <= 2.0 && (value(1) - min_y as f64).abs() <= 2.0,
        "形状包围盒起点应贴合实际渲染（{bbox:?} vs 实际 ({min_x},{min_y})）"
    );
    assert!(
        (value(2) - (max_x - min_x) as f64).abs() <= 3.0
            && (value(3) - (max_y - min_y) as f64).abs() <= 3.0,
        "形状包围盒尺寸应贴合实际渲染（{bbox:?} vs 实际 {}×{}）",
        max_x - min_x,
        max_y - min_y
    );
}

/// **整体撤销 ⇒ 路径回来、画面回到轮廓** ✓（两步原子一个变更集 ✓）。
#[test]
fn the_conversion_is_withdrawn_in_one_action() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_c2s", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_triangle(&mut workspace, &registry);
    let before = pixels(&mut workspace);
    let converted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "convert_to_shape",
            &json!({"object_id": "p1", "shape_id": "tri"}),
        )
    };
    assert_eq!(converted["ok"], json!(true), "{converted}");
    let changeset = converted["changeset_id"]
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
        object_type(&mut workspace, "p1"),
        "path",
        "撤销后路径应回来"
    );
    assert!(
        is_gone(&mut workspace, "tri"),
        "撤销后形状应消失（missing 或 deleted ✓）"
    );
    if let Some((first, count)) = {
        let after = pixels(&mut workspace);
        if after == before {
            None
        } else {
            Some((
                0usize,
                before.iter().zip(&after).filter(|(a, b)| a != b).count(),
            ))
        }
    } {
        assert!(
            count <= 8,
            "撤销之后画面应回到原样（差 {count} 字节，首个 #{first}）"
        );
    }
}

/// **笔迹也能直接转形状** ✓；**少于三个点**、**已经是形状**都要明确报错 ✓。
#[test]
fn convert_to_shape_validates_its_inputs() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_c2s", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_triangle(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        // 少于三个点 ⇒ 拒绝 ✓（"没有面积可言" ✓）。
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "short",
                    "data": {"points": [[10.0, 10.0], [40.0, 40.0]], "size": 4.0,
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        let refused = registry.call(
            &mut ctx,
            "convert_to_shape",
            &json!({"object_id": "short", "shape_id": "no"}),
        );
        assert_eq!(refused["ok"], json!(false), "{refused}");
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("三个顶点"),
            "错误应说清顶点数要求：{detail}"
        );

        // 笔迹（≥3 点）也能转 ✓。
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "poly",
                    "data": {"points": [[10.0, 10.0], [60.0, 20.0], [40.0, 70.0]], "size": 4.0,
                             "color": {"r": 20, "g": 120, "b": 20, "a": 255}}}),
        );
        let made = registry.call(
            &mut ctx,
            "convert_to_shape",
            &json!({"object_id": "poly", "shape_id": "from_stroke"}),
        );
        assert_eq!(made["ok"], json!(true), "笔迹应能直接转形状：{made}");
        assert_eq!(made["from"], json!("poly"), "{made}");

        // 形状再转形状 ⇒ 明确拒绝 ✓（没有意义 ✓）。
        let again = registry.call(
            &mut ctx,
            "convert_to_shape",
            &json!({"object_id": "from_stroke", "shape_id": "no2"}),
        );
        assert_eq!(again["ok"], json!(false), "{again}");
        assert!(
            again["context"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("形状无需再转"),
            "{again}"
        );
    }
}

/// **`path_edit` 的 `convert_to_shape` 算子与独立工具走同一条路** ✓（两入口不分叉 ✓）。
#[test]
fn the_path_edit_operator_delegates_to_the_same_implementation() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_c2s", 160, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_triangle(&mut workspace, &registry);
    let by_operator = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "convert_to_shape", "object_id": "p1", "shape_id": "via_op"}),
        )
    };
    assert_eq!(by_operator["ok"], json!(true), "{by_operator}");
    assert_eq!(by_operator["shape_id"], json!("via_op"), "{by_operator}");
    assert_eq!(
        object_type(&mut workspace, "via_op"),
        "shape",
        "算子入口也应产出形状"
    );
    // **这一段是 CI 抓出来的** ✓（本地我只跑了"涉及包"的测试 ✗ ⇒ 漏了这个文件 ✓）：
    // 它原本断言 `path_edit {op:"convert_to_path"}` **因设计缺口被拒** ✗ ——
    // 而第 52 轮我已把该算子**委托**给独立工具 ✓ ⇒ 契约变了 ✓。
    // 现在的正确契约 ✓：**形状**（既非笔迹也非路径 ✓）被拒 ✓，且错误要说明它只从笔迹转换 ✓。
    let refused = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "convert_to_path", "object_id": "via_op"}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "{refused}");
    assert!(
        refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("不是笔迹"),
        "拒绝理由应指向设计缺口：{refused}"
    );
}
