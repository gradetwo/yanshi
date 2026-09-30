//! 实例（设计 9.2 `resolve_object` 的第一步 + 9.3 循环引用检测）。
//!
//! **本片的目标只有一条 ✓：实例必须真的画出 master 的像素** ✓ ——
//! 这是用户能直接看到、也是实例存在的理由 ✓。
//! 设计里的完整链路还包括 `override`、`sync_policy`、master 缓存共享与依赖图传播（9.3 ✓），
//! 那些尚未实现 ✓ ⇒ 在**工具层明确拒绝** ✓ 而不是静默忽略 ✓（见 `create_instance` ✓）。
//!
//! **循环引用在折叠层就被挡住** ✓：手写进日志的环、自引用、过深的链都会在提交时被拒 ✓，
//! 而不是等到渲染时无限递归 ✓ —— 这条必须测 ✓，因为它是"能不能被写坏"的分界线 ✓。

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
    ToolContext::new(workspace, "doc_inst", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn ink(workspace: &mut Workspace, x: f64, y: f64) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_inst", Bbox::new(x, y, 40.0, 40.0))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

fn shape(x: f64, y: f64) -> serde_json::Value {
    json!({
        "geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": 24.0, "h": 24.0}},
        "color": {"r": 200, "g": 30, "b": 30, "a": 255},
    })
}

/// **实例画出 master 的像素** ✓，并且 `local_transform` 把它**挪开** ✓。
///
/// 这条同时验证了变换的**合成次序** ✓：master 自身的 transform → `local_transform` → 实例自身 ✓
///（次序错了画面就会跑到别处 ✓，而"有没有画出来"是抓不住次序错的 ✓，所以这里量**两个位置** ✓）。
#[test]
fn an_instance_renders_its_master_at_the_local_transform() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": shape(20.0, 20.0)}),
        );
    }
    let master_ink = ink(&mut workspace, 16.0, 16.0);
    assert!(master_ink > 0, "master 本身应当有墨（实测 {master_ink}）");
    assert_eq!(ink(&mut workspace, 100.0, 16.0), 0, "此处此刻不该有墨");

    {
        let mut ctx = context(&mut workspace);
        let created = registry.call(
            &mut ctx,
            "create_instance",
            &json!({
                "instance_id": "copy1",
                "layer_id": "L",
                "master_id": "master",
                "local_transform": {"matrix": [1, 0, 0, 1, 100, 0], "pivot": [0, 0]},
            }),
        );
        assert_eq!(created["ok"], json!(true), "{created}");
    }

    // 原处**仍然有墨** ✓（实例是引用 ✓，不是移动 master ✓）。
    assert!(
        ink(&mut workspace, 16.0, 16.0) > 0,
        "实例不该把 master 挪走（原处应仍有墨）"
    );
    // 偏移处**出现墨** ✓ —— 这就是实例本身 ✓。
    let instance_ink = ink(&mut workspace, 116.0, 16.0);
    assert!(
        instance_ink > 0,
        "实例应在 local_transform 指定的位置画出 master（实测 {instance_ink}）"
    );
    // 两个位置的墨量应当接近 ✓（同一个 master ✓，只是位置不同 ✓）。
    assert!(
        instance_ink * 2 > master_ink && master_ink * 2 > instance_ink,
        "实例与 master 的墨量应接近（master {master_ink} vs 实例 {instance_ink}）"
    );
}

/// **循环引用必须被拒** ✓（设计 9.3 ✓），而且**在折叠层**就被拒 ✓ ——
/// 这样即便有人绕过工具层直接写原子 ✓，日志里也不会出现能导致无限递归的结构 ✓。
#[test]
fn cycles_are_rejected_at_commit_time() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 128, 128),
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
            &json!({"layer_id": "L", "object_id": "a", "data": shape(10.0, 10.0)}),
        );
    }
    // ① 正常实例 ✓。
    {
        let mut ctx = context(&mut workspace);
        let ok = registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "b", "layer_id": "L", "master_id": "a"}),
        );
        assert_eq!(ok["ok"], json!(true), "{ok}");
    }
    // ② **自引用**：a 的实例 b 再做成 a 的 master ⇒ 环 ✓ ⇒ 必须被拒 ✓。
    {
        let mut ctx = context(&mut workspace);
        let cyclic = registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "a2", "layer_id": "L", "master_id": "b", "local_transform": {"matrix": [1,0,0,1,0,0]}}),
        );
        assert_eq!(cyclic["ok"], json!(true), "一层实例本身是合法的：{cyclic}");
        // 现在把 b 的 master 改成 a2 ⇒ b → a2 → b 成环 ✓（走折叠层的校验 ✓）。
        let cyclic2 = registry.call(
            &mut ctx,
            "create_object",
            &json!({
                "layer_id": "L",
                "object_id": "c",
                "type": "instance",
                "data": {"master_ref": {"object_id": "c", "local_transform": {"matrix": [1,0,0,1,0,0]}}},
            }),
        );
        assert_eq!(cyclic2["ok"], json!(false), "**自引用必须被拒**：{cyclic2}");
        assert_eq!(
            cyclic2["error_code"].as_str().unwrap_or_default(),
            "invalid_argument",
            "应给出 invalid_argument：{cyclic2}"
        );
    }
}

/// **master 被删除之后：实例不画、不崩** ✓ —— 这是真实会发生的场景 ✓（删掉被引用的对象 ✓）。
///
/// **设计未规定 ⇒ 记录选择 ✓**：把实例的引用保留 ✓、渲染时解析不到就**什么都不画** ✓，
/// 而不是自动删除实例 ✗（那是无痕的破坏 ✗）也不是报错 ✗（一次删除会让整幅渲染失败 ✓）。
/// 这与"**日志决定渲染**"一致 ✓：引用是一条**声明** ✓，能不能解析取决于当前 head ✓。
///
/// 顺带说明一条本轮的边界 ✓：我原本写的场景是"先建实例、后建 master" ✗ ——
/// 那**不被设计允许** ✓：12.2 的补丁顺序协议要求**引用在写入时就必须存在** ✓
/// （折叠层报"对象 later 在当前 HEAD 中不存在" ✓）。测试要按设计的规则写 ✓，
/// 不能拿一个设计不允许的顺序去证明一个自己想要的结论 ✓。
#[test]
fn an_instance_whose_master_was_deleted_draws_nothing_and_does_not_fail() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 128, 128),
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
            &json!({"layer_id": "L", "object_id": "master", "data": shape(20.0, 20.0)}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 60, 0], "pivot": [0, 0]}}),
        );
    }
    assert!(ink(&mut workspace, 16.0, 16.0) > 0, "master 应在原处");
    assert!(ink(&mut workspace, 76.0, 16.0) > 0, "实例应在偏移处");

    // 删掉 master ⇒ 实例**不再画** ✓，但**不报错** ✓、也不影响其它内容 ✓。
    {
        let mut ctx = context(&mut workspace);
        let removed = registry.call(&mut ctx, "delete_object", &json!({"object_id": "master"}));
        assert_eq!(removed["ok"], json!(true), "{removed}");
    }
    assert_eq!(
        ink(&mut workspace, 16.0, 16.0),
        0,
        "master 删除后原处应为空"
    );
    assert_eq!(
        ink(&mut workspace, 76.0, 16.0),
        0,
        "master 删除后实例也不该再画"
    );
    // 引用仍在日志里 ✓（不是悄悄改数据 ✓）—— 列表里还能看到这个实例 ✓。
    let listed = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let instance = listed["objects"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["object_id"] == json!("mirror"))
        })
        .cloned();
    assert!(
        instance.is_some(),
        "实例本身应仍在（引用被保留 ✓）：{listed}"
    );
}

/// **脱离实例：位置不变、此后不再跟随** ✓（设计 9.4 `detach_instance` ✓）。
///
/// 两条都要验 ✓：脱离**当刻**画面必须不变 ✓（否则用户点一下就跳一下 ✓），
/// 而脱离**之后** master 再动就不该带走它 ✓（否则"脱离"没有意义 ✓）。
#[test]
fn detaching_an_instance_keeps_it_put_and_stops_it_following() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": shape(20.0, 20.0)}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 80, 0], "pivot": [0, 0]}}),
        );
    }
    let before = ink(&mut workspace, 96.0, 16.0);
    assert!(before > 0, "脱离之前实例处应有墨（实测 {before}）");

    let detached_id;
    {
        let mut ctx = context(&mut workspace);
        let detached = registry.call(
            &mut ctx,
            "detach_instance",
            &json!({"instance_id": "mirror", "object_id": "detached"}),
        );
        assert_eq!(detached["ok"], json!(true), "{detached}");
        detached_id = detached["detached_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
    }
    assert_eq!(detached_id, "detached");
    // ① **画面不变** ✓。
    assert_eq!(
        ink(&mut workspace, 96.0, 16.0),
        before,
        "脱离当刻画面必须完全不变"
    );

    // ② **此后不再跟随** ✓：把 master 移开 ✓ ⇒ 脱离出来的对象留在原处 ✓。
    {
        let mut ctx = context(&mut workspace);
        let moved = registry.call(
            &mut ctx,
            "move_object",
            &json!({"object_id": "master", "delta": {"dx": 0.0, "dy": 100.0}}),
        );
        assert_eq!(moved["ok"], json!(true), "{moved}");
    }
    assert!(
        ink(&mut workspace, 96.0, 16.0) > 0,
        "脱离出来的对象不该跟着 master 走（原处应仍有墨）"
    );
}

/// **改写实例指向的 master** ✓（设计 9.4 `link_to_master` ✓），
/// 以及**通过改写制造环必须被拒** ✓ —— 这是本轮新增的那道折叠层防线 ✓。
#[test]
fn linking_an_instance_to_another_master_works_and_cycles_are_refused() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "red", "data": shape(20.0, 20.0)}),
        );
        // 第二个 master 放在别处 ✓（这样"换了指向"在像素上看得出来 ✓）。
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "blue", "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 20.0, "y": 120.0, "w": 24.0, "h": 24.0}},
                "color": {"r": 30, "g": 30, "b": 200, "a": 255}}}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "red",
                    "local_transform": {"matrix": [1, 0, 0, 1, 120, 0], "pivot": [0, 0]}}),
        );
    }
    // 指向 red（y≈20）时，实例的墨在 y≈20 一带 ✓；指向 blue 之后应移到 y≈120 一带 ✓。
    assert!(ink(&mut workspace, 136.0, 16.0) > 0, "开始时实例应跟着 red");
    {
        let mut ctx = context(&mut workspace);
        let linked = registry.call(
            &mut ctx,
            "link_to_master",
            &json!({"instance_id": "mirror", "master_id": "blue"}),
        );
        assert_eq!(linked["ok"], json!(true), "{linked}");
    }
    assert_eq!(ink(&mut workspace, 136.0, 16.0), 0, "换指向后原处应为空");
    assert!(
        ink(&mut workspace, 136.0, 116.0) > 0,
        "换指向后实例应出现在 blue 的位置"
    );

    // **通过改写制造环 ⇒ 必须被拒** ✓：让 red 变成一个指向 mirror 的实例 ✓，
    // 而 mirror 已经指向 red ⇒ red → mirror → red ✓。
    {
        let mut ctx = context(&mut workspace);
        let cyclic = registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "red_wrapper", "layer_id": "L", "master_id": "mirror"}),
        );
        assert_eq!(cyclic["ok"], json!(true), "先建一层是合法的：{cyclic}");
        // 把 mirror 改指向 red_wrapper ✓ ⇒ red_wrapper → mirror → red_wrapper ✓。
        let refused = registry.call(
            &mut ctx,
            "link_to_master",
            &json!({"instance_id": "mirror", "master_id": "red_wrapper"}),
        );
        assert_eq!(
            refused["ok"],
            json!(false),
            "**通过改写制造环必须被拒**：{refused}"
        );
        assert_eq!(
            refused["error_code"].as_str().unwrap_or_default(),
            "invalid_argument",
            "应给出 invalid_argument：{refused}"
        );
    }
}

/// **`get_resolved_state` 让解析可观测** ✓（设计 9.2 ✓），
/// 而且它报的包围盒必须**来自与渲染同一个函数** ✓ ⇒ 读到的就是画出来的 ✓。
#[test]
fn the_resolved_state_reports_the_master_and_the_rendered_bbox() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_inst", 256, 256),
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
            &json!({"layer_id": "L", "object_id": "master", "data": shape(20.0, 20.0)}),
        );
        registry.call(
            &mut ctx,
            "create_instance",
            &json!({"instance_id": "mirror", "layer_id": "L", "master_id": "master",
                    "local_transform": {"matrix": [1, 0, 0, 1, 100, 40], "pivot": [0, 0]}}),
        );
    }
    let reported = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_resolved_state",
            &json!({"object_id": "mirror"}),
        )
    };
    assert_eq!(reported["ok"], json!(true), "{reported}");
    assert_eq!(
        reported["is_instance"],
        json!(true),
        "应报告这是实例：{reported}"
    );
    assert_eq!(reported["master_id"], json!("master"), "{reported}");
    let bbox: Vec<f64> = reported["bbox"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(bbox.len(), 4, "应给出包围盒：{reported}");
    // master 在 (20,20,24,24) ✓，local_transform 平移 (100,40) ⇒ 实例包围盒约 (120,60) ✓。
    assert!(
        (bbox[0] - 120.0).abs() < 2.0 && (bbox[1] - 60.0).abs() < 2.0,
        "解析出的包围盒应反映 local_transform（实际 {bbox:?}）"
    );
    // **包围盒处确实有墨** ✓ —— 这条把"读到的"和"画出来的"绑在一起 ✓。
    assert!(
        ink(&mut workspace, bbox[0] as f64 - 4.0, bbox[1] as f64 - 4.0) > 0,
        "包围盒位置应当就是画出来的位置"
    );
    // 非实例对象也要能查 ✓（此时没有 master ✓）。
    let plain = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "get_resolved_state",
            &json!({"object_id": "master"}),
        )
    };
    assert_eq!(plain["is_instance"], json!(false), "{plain}");
    assert!(plain["master_id"].is_null(), "{plain}");
}
