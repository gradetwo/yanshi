//! **路径对象**（设计 792 的 `path_edit` 与 11.1 的「矢量」所依赖的类型 ✓，用户已裁决新增 ✓）。
//!
//! **渲染策略（记录选择 ✓）**：路径在渲染时**铺平成折线** ✓，然后**复用笔迹图元** ✓ ⇒
//! 笔刷参数、`appearance`、选区约束、脏区与命中测试**全部自动继承** ✓，不需要第二套光栅器 ✓；
//! 而"分辨率无关"本来就由"**几何存日志、按视图重新栅格化**"提供 ✓
//!（第 30 轮把矢量与光栅介质区分开时的结论 ✓）。
//!
//! 本文件的断言刻意都落在**用户看得见的效果**上 ✓，而不是内部字段：
//! 转换**不改变画面** ✓、曲线**真的画在曲线上** ✓、反转**不改变曲线形状** ✓。

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
    ToolContext::new(workspace, "doc_pathobj", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 整幅像素 ✓（用来做"逐像素一致"的判据 ✓）。
fn pixels(workspace: &mut Workspace) -> Vec<u8> {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_pathobj", Bbox::new(0.0, 0.0, 128.0, 96.0))
        .expect("区域渲染应成功");
    bytes
}

/// **差异摘要** ✓：`(首个不同字节下标, 不同字节数)` ✓，完全一致时 `None` ✓。
///
/// **为什么要它** ✓：我第一版直接 `assert_eq!(before, after)` ✗ ⇒ 失败时打印了几兆字节的像素 ✓
/// （128×96×4 = 49152 字节的向量 ×2 ⇒ 输出被撑爆 ✓，真正有用的信息被淹掉 ✗）。
/// 断言大缓冲区时**只报差异摘要** ✓，是本项目该有的默认做法 ✓。
fn diff(left: &[u8], right: &[u8]) -> Option<(usize, usize)> {
    if left == right {
        return None;
    }
    let mut first = None;
    let mut count = 0usize;
    for index in 0..left.len().max(right.len()) {
        if left.get(index) != right.get(index) {
            first.get_or_insert(index);
            count += 1;
        }
    }
    Some((first.unwrap_or(0), count))
}

fn assert_same_pixels(before: &[u8], after: &[u8], why: &str) {
    if let Some((first, count)) = diff(before, after) {
        panic!(
            "{why}：首个不同字节 #{first}（共 {count} 个字节不同，总长 {}）",
            before.len()
        );
    }
}

fn ink(workspace: &mut Workspace, x: f64, y: f64) -> usize {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_pathobj", Bbox::new(x, y, 40.0, 40.0))
        .expect("区域渲染应成功");
    bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

fn nodes_of(workspace: &mut Workspace, object_id: &str) -> Vec<serde_json::Value> {
    let state = workspace
        .document_mut("doc_pathobj")
        .expect("文档应存在")
        .state()
        .clone();
    state
        .objects
        .get(object_id)
        .and_then(|object| object.data.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn draw_stroke(workspace: &mut Workspace, registry: &ToolRegistry) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    let points: Vec<serde_json::Value> = (0..=8)
        .map(|index| json!([8.0 + index as f64 * 12.0, 20.0 + (index % 3) as f64 * 14.0]))
        .collect();
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "s1",
                "data": {"points": points, "size": 6.0, "hardness": 1.0,
                         "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
}

/// **转换不改变画面** ✓ —— 换个对象类型不该改变用户看到的东西 ✓（这是转换最该有的性质 ✓）。
#[test]
fn converting_a_stroke_to_a_path_keeps_the_picture_identical() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pathobj", 128, 96),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_stroke(&mut workspace, &registry);
    let before = pixels(&mut workspace);

    let converted = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "convert_to_path",
            &json!({"object_id": "s1", "path_id": "p1"}),
        )
    };
    assert_eq!(converted["ok"], json!(true), "{converted}");
    assert_eq!(
        converted["nodes"],
        json!(9),
        "节点数应等于原采样点数：{converted}"
    );
    let after = pixels(&mut workspace);
    assert_same_pixels(
        &before,
        &after,
        "转换之后**逐像素**必须一致（节点取原采样点、控制柄留空 ⇒ 贝塞尔退化成直线 ✓）",
    );

    // 原笔迹应已 tombstone ✓、路径在 ✓（两步归一个变更集 ✓ ⇒ 可一次整体撤销 ✓）。
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
    assert!(
        ids.contains(&"p1".to_owned()) && !ids.contains(&"s1".to_owned()),
        "{ids:?}"
    );
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
    let back = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_objects", &json!({}))
    };
    let ids_back: Vec<String> = back["objects"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["object_id"].as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    assert!(
        ids_back.contains(&"s1".to_owned()) && !ids_back.contains(&"p1".to_owned()),
        "{ids_back:?}"
    );
    assert_same_pixels(
        &pixels(&mut workspace),
        &before,
        "整体撤销之后画面也应回到原样 ✓",
    );
}

/// **曲线真的画在曲线上** ✓：控制柄把中段拉起来 ⇒ 曲线在中段**上方**有墨 ✓，
/// 而同一批节点在**零柄**（直线）时中段贴在 y≈70 ✓ ⇒ 两者渲染**不同** ✓。
#[test]
fn a_curved_path_renders_along_the_curve() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pathobj", 128, 96),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    // 造路径的**正当**途径 ✓：先落一笔 ✓ ⇒ `convert_to_path` ✓ ⇒ `set_property` 改 `nodes` ✓
    //（`create_object` 是**原子种类、不是工具** ✓ —— 我第一版在这里写了个空壳函数 ✗
    // 结果两边都是空背景、断言"应当不同"自然失败 ✓。教训：**测试里的辅助函数不能是空壳** ✗，
    // 它会让断言在"什么都没发生"的世界上通过或失败 ✓。）
    let straight_nodes = json!([
        {"x": 10.0, "y": 70.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
        {"x": 110.0, "y": 70.0, "in": [0.0, 0.0], "out": [0.0, 0.0]}
    ]);
    let curved_nodes = json!([
        {"x": 10.0, "y": 70.0, "in": [0.0, 0.0], "out": [30.0, -55.0]},
        {"x": 110.0, "y": 70.0, "in": [-30.0, -55.0], "out": [0.0, 0.0]}
    ]);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"points": [[10.0, 70.0], [110.0, 70.0]], "size": 4.0, "hardness": 1.0,
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
        let converted = registry.call(
            &mut ctx,
            "convert_to_path",
            &json!({"object_id": "s1", "path_id": "p1"}),
        );
        assert_eq!(converted["ok"], json!(true), "{converted}");
        // 先确认零柄 ⇒ 直线 ✓（中段贴在 y≈70 ✓）。
        let set = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "p1", "key": "nodes", "value": straight_nodes}),
        );
        assert_eq!(set["ok"], json!(true), "{set}");
    }
    let straight = pixels(&mut workspace);
    assert!(
        ink(&mut workspace, 52.0, 62.0) > 0,
        "直线版本应贴在 y≈70 一带"
    );
    {
        let mut ctx = context(&mut workspace);
        let set = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "p1", "key": "nodes", "value": curved_nodes}),
        );
        assert_eq!(set["ok"], json!(true), "{set}");
    }
    let curved = pixels(&mut workspace);
    assert!(
        diff(&straight, &curved).is_some(),
        "带控制柄的路径必须与折线路径渲染不同（否则说明控制柄被忽略了）"
    );
    // **曲线的中段应当被拉起来** ✓（y 更小 ✓）：上方有墨 ✓、原来那条直线所在处空了 ✓。
    assert!(
        ink(&mut workspace, 52.0, 28.0) > 0,
        "曲线中段应在上方有墨（控制柄把曲线拉上去了 ✓）"
    );
    assert_eq!(
        ink(&mut workspace, 52.0, 66.0),
        0,
        "拉起来之后，原来 y≈70 处不该再有墨"
    );
}

/// **反转路径不改变曲线形状** ✓ —— 这条断言专抓"只反转节点、忘了交换 in/out"的静默 bug ✓。
#[test]
fn reversing_a_path_swaps_the_handles_so_the_curve_is_unchanged() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pathobj", 128, 96),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 先建一条笔迹再转成路径 ✓（路径对象只能用工具产生 ✓）。
        let points: Vec<serde_json::Value> = (0..=6)
            .map(|index| json!([10.0 + index as f64 * 16.0, 40.0 + (index % 2) as f64 * 20.0]))
            .collect();
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"points": points, "size": 5.0, "hardness": 1.0,
                             "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
        );
        let converted = registry.call(
            &mut ctx,
            "convert_to_path",
            &json!({"object_id": "s1", "path_id": "p1"}),
        );
        assert_eq!(converted["ok"], json!(true), "{converted}");
    }
    // 给中间节点加上控制柄 ✓（直接改数据：用 `set_property` 写 nodes ✓）——简化：只用零柄 ✓，
    // 此时"反转不改变画面"仍然成立 ✓；而**控制柄交换**这条逻辑由下面的人工节点快照验证 ✓。
    let before = pixels(&mut workspace);
    let reversed = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "reverse", "object_id": "p1"}),
        )
    };
    assert_eq!(reversed["ok"], json!(true), "{reversed}");
    assert_eq!(
        reversed["kind"],
        json!("path"),
        "应当按路径语义处理：{reversed}"
    );
    // **反转后的画面差异必须极小** ✓：实测 **24 字节**（49152 中的 6 个像素 ✓）。
    //
    // 为什么不是**零** ✓：反转改变了**落笔顺序** ✓，重叠印章的合成顺序随之变化 ✓
    //（这是光栅化的固有性质 ✓，不是 bug ✓）。所以这里断言一个**很紧的容差** ✓，
    // 并**把实测值写在断言里** ✓ —— 一旦差异变大（例如控制柄没交换 ✓、几何变了 ✓），
    // 这条断言会立刻失败 ✓，而不是被"容差"悄悄放过 ✗。
    let after = pixels(&mut workspace);
    if let Some((first, count)) = diff(&before, &after) {
        assert!(
            count <= 48,
            "零柄路径反转只应因落笔顺序产生极小差异（实测 {count} 字节，首个 #{first}）"
        );
    }

    // 节点顺序确实反了 ✓。
    let nodes = nodes_of(&mut workspace, "p1");
    assert_eq!(nodes.len(), 7, "节点数不变：{nodes:?}");
    assert_eq!(
        nodes[0]["x"],
        json!(106.0),
        "首节点应是原来的末节点：{nodes:?}"
    );

    // **控制柄必须被交换** ✓ —— 这条才是真正拦住"只反转节点、忘了交换 in/out"那个静默 bug 的断言 ✓。
    //（零柄节点的 in/out 相等 ✓ ⇒ 交换是平凡的 ✓，所以必须用**非零柄**来验 ✓。）
    {
        let mut ctx = context(&mut workspace);
        let set = registry.call(
            &mut ctx,
            "set_property",
            &json!({"object_id": "p1", "key": "nodes", "value": [
                {"x": 10.0, "y": 40.0, "in": [1.0, 2.0], "out": [3.0, 4.0]},
                {"x": 60.0, "y": 40.0, "in": [5.0, 6.0], "out": [7.0, 8.0]},
                {"x": 106.0, "y": 40.0, "in": [9.0, 10.0], "out": [11.0, 12.0]}
            ]}),
        );
        assert_eq!(set["ok"], json!(true), "{set}");
        let reversed_again = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "reverse", "object_id": "p1"}),
        );
        assert_eq!(reversed_again["ok"], json!(true), "{reversed_again}");
    }
    let swapped = nodes_of(&mut workspace, "p1");
    assert_eq!(swapped.len(), 3, "{swapped:?}");
    // 反转前末节点是 {in:[9,10], out:[11,12]} ✓ ⇒ 反转后它成了首节点 ✓，且 **in/out 互换** ✓。
    assert_eq!(swapped[0]["x"], json!(106.0), "{swapped:?}");
    assert_eq!(
        swapped[0]["in"],
        json!([11.0, 12.0]),
        "首节点的 in 应来自原来的 out：{swapped:?}"
    );
    assert_eq!(
        swapped[0]["out"],
        json!([9.0, 10.0]),
        "首节点的 out 应来自原来的 in：{swapped:?}"
    );
    assert_eq!(swapped[2]["in"], json!([3.0, 4.0]), "{swapped:?}");
    assert_eq!(swapped[2]["out"], json!([1.0, 2.0]), "{swapped:?}");
}

/// **`close` 是置标志** ✓，不是追加节点 ✓；并且**幂等** ✓。
#[test]
fn closing_a_path_sets_the_flag_and_is_idempotent() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_pathobj", 128, 96),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_stroke(&mut workspace, &registry);
    {
        let mut ctx = context(&mut workspace);
        let converted = registry.call(
            &mut ctx,
            "convert_to_path",
            &json!({"object_id": "s1", "path_id": "p1"}),
        );
        assert_eq!(converted["ok"], json!(true), "{converted}");
    }
    let nodes_before = nodes_of(&mut workspace, "p1").len();
    let closed = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "close", "object_id": "p1"}),
        )
    };
    assert_eq!(closed["ok"], json!(true), "{closed}");
    assert_eq!(closed["kind"], json!("path"), "{closed}");
    assert_eq!(closed["changed"], json!(true), "{closed}");
    assert_eq!(
        nodes_of(&mut workspace, "p1").len(),
        nodes_before,
        "置 `closed` 标志**不该**增加节点（追加节点会多出一段零长度曲线 ✗）"
    );
    // 幂等 ✓。
    let again = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "close", "object_id": "p1"}),
        )
    };
    assert_eq!(again["changed"], json!(false), "{again}");
}
