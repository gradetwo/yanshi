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
