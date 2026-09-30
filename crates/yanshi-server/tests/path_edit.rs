//! `path_edit`（设计 792 行）——**本片只做三个算子** ✓，其余**显式拒绝** ✓。
//!
//! 设计把 `path_edit` 列进工具表 ✓，但**没有规定「路径对象」在内核里怎么表达** ✗
//!（`ObjectType` 里没有 Path ✓；`Region` 里的 `path` 是另一回事 ✓）。
//! `split`/`merge`/`boolean`/`convert_to_*` 都需要那套模型 ✓ ⇒ 是**设计决策** ✓，
//! 本片不擅自发明 ✓，而是**明确报错并说明** ✓。
//!
//! 能做且语义明确的是 `reverse` / `close` / `join` ✓ —— 它们都落在**既有**的笔迹几何上 ✓。
//! 其中 `reverse` 有一条**可验证**的性质 ✓：配合 `appearance.paint_load`（墨沿笔迹耗尽 ✓），
//! 反向之后**渲染结果会变** ✓，而且"深淡两端会互换" ✓ —— 这条断言比"点序反了"强得多 ✓
//!（点序反了是**实现** ✓，深浅互换才是**用户看得见的效果** ✓）。

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
    ToolContext::new(workspace, "doc_path", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 左右两半的墨量 ✓（用来量"深浅两端是否互换" ✓）。
fn ink_halves(workspace: &mut Workspace) -> (usize, usize) {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_path", Bbox::new(0.0, 0.0, 128.0, 40.0))
        .expect("区域渲染应成功");
    let mut left = 0usize;
    let mut right = 0usize;
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        // **按颜色判墨** ✓ —— 区域渲染的背景是**不透明白** ✓，
        // 只看 alpha 会把整块背景算成墨 ✗（我第一版就是这么得到"左右完全相同"的假结论 ✓）。
        if pixel[3] <= 8 || (pixel[0] > 200 && pixel[1] > 200 && pixel[2] > 200) {
            continue;
        }
        let x = index % 128;
        if x < 64 {
            left += usize::from(pixel[3]);
        } else {
            right += usize::from(pixel[3]);
        }
    }
    (left, right)
}

fn draw_long_stroke(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    // **带 `paint_load`** ✓：墨沿笔迹耗尽。实测（本轮探针 ✓，判墨口径修正后 ✓）：
    // `paint_load: 0.55` 时墨在**半途就耗尽** ✓（左半边 24480、右半边 **0** ✓）；
    // 而不带 appearance 时是 47430 / 41310 ✓ —— 差别极大 ✓，因此这条断言很稳 ✓。
    // 笔尖取**细而不重叠** ✓（size 4、spacing 1.0 ✓）：重叠的粗笔会让每像素饱和到 255 ✓，
    // 耗墨就被**掩盖**了 ✓（这正是我第二次度量的假象 ✓）。
    let points: Vec<serde_json::Value> = (0..=28)
        .map(|index| json!([4.0 + index as f64 * 4.0, 20.0]))
        .collect();
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "s1",
                "data": {"points": points, "size": 4.0, "hardness": 1.0, "spacing": 1.0,
                         "color": {"r": 20, "g": 20, "b": 20, "a": 255},
                         "appearance": {"paint_load": 0.55}}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
}

/// **`reverse` 必须真的改变渲染** ✓（用户看得见的效果 ✓），而且**深浅两端互换** ✓。
#[test]
fn reversing_a_stroke_swaps_which_end_is_heavier() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_path", 128, 40),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_long_stroke(&mut workspace, &registry);

    let (left_before, right_before) = ink_halves(&mut workspace);
    assert!(
        left_before > right_before,
        "带载墨的笔迹应当起点更浓（左 {left_before} vs 右 {right_before}）"
    );

    let reversed = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "reverse", "object_id": "s1"}),
        )
    };
    assert_eq!(reversed["ok"], json!(true), "{reversed}");
    assert_eq!(reversed["op"], json!("reverse"), "{reversed}");

    let (left_after, right_after) = ink_halves(&mut workspace);
    // **这条断言是本用例的重点** ✓：深浅两端互换 ✓ ⇒ 右侧变得比左侧浓 ✓。
    //（只说"点序反了"是**实现** ✓；"墨浓的一端换了边"才是**效果** ✓。）
    assert!(
        right_after > left_after,
        "反向之后应变成终点更浓（左 {left_after} vs 右 {right_after}）\
         ｜反向前后：左 {left_before}→{left_after}，右 {right_before}→{right_after}"
    );
    assert_eq!(
        (left_before + right_before),
        (left_after + right_after),
        "反向不该改变总墨量（只是换了方向）"
    );
}

/// **`close` 把首点接到末尾，并且幂等** ✓ —— 第二次调用不该再追加一个点 ✓。
#[test]
fn closing_a_stroke_is_idempotent() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_path", 128, 40),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_long_stroke(&mut workspace, &registry);
    // **从状态读点** ✓ —— `list_objects` 返回的是元数据 ✓，**不含 `data.points`** ✗
    //（我第一版按它读，得到"点数 0"的假失败 ✓）。
    let points_of = |workspace: &mut Workspace| -> Vec<serde_json::Value> {
        let state = workspace
            .document_mut("doc_path")
            .expect("文档应存在")
            .state()
            .clone();
        state
            .objects
            .get("s1")
            .and_then(|object| object.data.get("points"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let before = points_of(&mut workspace);
    let closed = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "close", "object_id": "s1"}),
        )
    };
    assert_eq!(closed["ok"], json!(true), "{closed}");
    assert_eq!(
        closed["changed"],
        json!(true),
        "首次闭合应当有改动：{closed}"
    );
    let after = points_of(&mut workspace);
    assert_eq!(after.len(), before.len() + 1, "闭合应追加一个点");
    assert_eq!(after.last(), after.first(), "闭合后末点应等于首点");
    assert_ne!(before.last(), before.first(), "（前提：原来并未闭合）");

    // **幂等** ✓：再闭合一次 ⇒ 不再追加 ✓，并如实报告 `changed: false` ✓。
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "close", "object_id": "s1"}),
        )
    };
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(
        again["changed"],
        json!(false),
        "第二次闭合不该再改动：{again}"
    );
    assert_eq!(points_of(&mut workspace).len(), after.len(), "点数不该再变");
}

/// **`join` 接上第二条并把它 tombstone** ✓，两步同属**一个变更集** ✓。
#[test]
fn joining_two_strokes_concatenates_and_retires_the_second() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_path", 128, 40),
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
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "a",
                    "data": {"points": [[4.0, 10.0], [40.0, 10.0]], "size": 6.0,
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "b",
                    "data": {"points": [[60.0, 30.0], [120.0, 30.0]], "size": 6.0,
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        );
    }
    let joined = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "join", "object_id": "a", "other_id": "b"}),
        )
    };
    assert_eq!(joined["ok"], json!(true), "{joined}");
    assert_eq!(
        joined["points"],
        json!(4),
        "两条各两点 ⇒ 接起来是四点：{joined}"
    );
    assert_eq!(joined["joined_from"], json!("b"), "{joined}");
    let changeset = joined["changeset_id"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!changeset.is_empty(), "应给出 changeset_id：{joined}");

    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let ids: Vec<String> = listed["objects"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["object_id"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    assert!(ids.contains(&"a".to_owned()), "第一条应留下：{listed}");
    assert!(!ids.contains(&"b".to_owned()), "第二条应已消失：{listed}");
    // 点序 ✓：先是 a 的两点、再是 b 的两点 ✓（同样从**状态**读 ✓）。
    let points: Vec<serde_json::Value> = {
        let state = workspace
            .document_mut("doc_path")
            .expect("文档应存在")
            .state()
            .clone();
        state
            .objects
            .get("a")
            .and_then(|object| object.data.get("points"))
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    assert_eq!(points.len(), 4, "接起来应是四点：{points:?}");
    assert_eq!(
        points[2],
        json!([60.0, 30.0]),
        "第三点应来自第二条：{points:?}"
    );
}

/// **需要「路径对象」模型的算子必须显式拒绝** ✓ —— 静默接受一个不生效的姿势比不支持更糟 ✗。
#[test]
fn operations_that_need_an_unspecified_path_model_are_refused() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_path", 128, 40),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_long_stroke(&mut workspace, &registry);
    // **本用例在路径片全部落地之后被改写过** ✓ —— 这是它现在的契约 ✓：
    // 设计 792 的八个算子里，除了 `convert_to_path`（它其实是**笔迹 ⇒ 路径** ✓，
    // 已作为独立工具 ✓，作为 `path_edit` 算子没有意义 ✓）之外，**全部已实现** ✓。
    //
    // **为什么必须改** ✗：我第 45/46/47 轮陆续实现了 `split`/`merge`/`convert_to_shape`/`boolean` ✓，
    // 而这条测试还在断言它们"因缺少路径对象模型被拒" ✗ ⇒ 早该跟着改 ✓。
    // **它当时为什么没被发现** ✗：我说"本地只跑改动涉及的包与测试" ✓，
    // 但**是按测试文件名挑的** ✓ —— 跑了新的 `path_split_merge.rs` ✓，
    // 却漏了更早的 `path_edit.rs` ✗，而后者测的正是**同一个工具** ✓。
    // **教训** ✓：选测试的判据应当是"**这个工具/模块的测试文件有哪些**" ✓，
    // 而不是"这轮新建的测试文件叫什么" ✗ —— 否则最容易漏掉最老的回归测试 ✓。
    {
        // **只剩一个算子在拒绝** ✓ ⇒ 不再写单元素 `for` ✓（clippy 当场指出 ✓）。
        let op = "convert_to_path";
        let mut ctx = context(&mut workspace);
        let refused = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": op, "object_id": "s1", "other_id": "s1"}),
        );
        assert_eq!(
            refused["ok"],
            json!(false),
            "{op} 作为算子应被拒绝：{refused}"
        );
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("路径对象") || detail.contains("convert_to_path"),
            "{op} 的错误应说清它是独立工具、不是算子：{detail}"
        );
    }
    // **已实现的算子不该再被当成"不支持"** ✓（一条正例 ✓，防止将来把实现改回去 ✗）。
    {
        let mut ctx = context(&mut workspace);
        let reversed = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "reverse", "object_id": "s1"}),
        );
        assert_eq!(reversed["ok"], json!(true), "reverse 应当可用：{reversed}");
    }
    // 未知算子 ✓ 与**非笔迹对象** ✓ 也要被拒 ✓。
    {
        let mut ctx = context(&mut workspace);
        let unknown = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "wat", "object_id": "s1"}),
        );
        assert_eq!(unknown["ok"], json!(false), "{unknown}");
        let shape = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "box", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 1.0, "y": 1.0, "w": 4.0, "h": 4.0}},
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        );
        assert_eq!(shape["ok"], json!(true), "{shape}");
        let not_a_stroke = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "reverse", "object_id": "box"}),
        );
        assert_eq!(
            not_a_stroke["ok"],
            json!(false),
            "形状没有点序可言：{not_a_stroke}"
        );
    }
}
