//! `path_edit` 的 `split` 与 `merge`（设计 792 ✓，模型已就位 ✓，本轮只补算子 ✓）。
//!
//! **设计只给了名字 ⇒ 记录选择** ✓：
//! * `split` 只在**开放路径**上工作 ✓ —— 闭合环在**一个**节点处切不开 ✓（数学上需要两刀 ✓），
//!   设计没说第二刀怎么给 ✓ ⇒ **明确拒绝** ✓；切口节点的控制柄**按归属分** ✓
//!   （左半末节点留 `in` ✓、右半首节点留 `out` ✓）⇒ **两半合起来与原曲线一致** ✓；
//! * `merge` = `join` **加上接缝切线对齐** ✓（弦长 1/3 ✓）⇒ 接缝平滑而不是折角 ✓。
//!   若两者语义相同就应当只留一个名字 ✓；这里给出可验证的差别 ✓。
//!
//! 断言尽量落在**用户看得见**的量上（墨量 ✓、渲染差异 ✓），而不是内部字段 ✓。

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
    ToolContext::new(workspace, "doc_sm", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn pixels(workspace: &mut Workspace) -> Vec<u8> {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_sm", Bbox::new(0.0, 0.0, 160.0, 100.0))
        .expect("区域渲染应成功");
    bytes
}

fn ink(workspace: &mut Workspace) -> usize {
    let (_, _, bytes) = workspace
        .render_region_raw("doc_sm", Bbox::new(0.0, 0.0, 160.0, 100.0))
        .expect("区域渲染应成功");
    bytes
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
        .count()
}

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

fn nodes_of(workspace: &mut Workspace, object_id: &str) -> Vec<serde_json::Value> {
    let state = workspace
        .document_mut("doc_sm")
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

/// 造一条**带控制柄的开放路径** ✓（经"落笔 ⇒ 转路径 ⇒ 改 nodes" ✓ —— 路径对象只能由工具产生 ✓）。
fn draw_path(workspace: &mut Workspace, registry: &ToolRegistry, object_id: &str) {
    let mut ctx = context(workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "seed",
                "data": {"points": [[10.0, 50.0], [150.0, 50.0]], "size": 4.0, "hardness": 1.0,
                         "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let converted = registry.call(
        &mut ctx,
        "convert_to_path",
        &json!({"object_id": "seed", "path_id": object_id}),
    );
    assert_eq!(converted["ok"], json!(true), "{converted}");
    // 四个节点、两端带**向外**的控制柄 ✓ ⇒ 切开后两半各自仍有曲率 ✓。
    let set = registry.call(
        &mut ctx,
        "set_property",
        &json!({"object_id": object_id, "key": "nodes", "value": [
            {"x": 10.0, "y": 50.0, "in": [0.0, 0.0], "out": [20.0, -30.0]},
            {"x": 60.0, "y": 20.0, "in": [-20.0, 30.0], "out": [20.0, 30.0]},
            {"x": 110.0, "y": 80.0, "in": [-20.0, -30.0], "out": [20.0, -30.0]},
            {"x": 150.0, "y": 50.0, "in": [-20.0, 30.0], "out": [0.0, 0.0]}
        ]}),
    );
    assert_eq!(set["ok"], json!(true), "{set}");
}

/// **切开之后两半的墨量之和 = 原来的墨量** ✓ —— 这条守住"控制柄按归属分" ✓。
///
/// 只反转节点、不做归属分配（或把控制柄一股脑留给一侧 ✓）都会让曲线变形 ✓ ⇒ 墨量不再相等 ✓。
#[test]
fn splitting_a_path_keeps_the_total_ink() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sm", 160, 100),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_path(&mut workspace, &registry, "p1");
    let before = ink(&mut workspace);
    assert!(before > 0, "原路径应有墨");

    let split = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "split", "object_id": "p1", "at": 2, "right_id": "p2"}),
        )
    };
    assert_eq!(split["ok"], json!(true), "{split}");
    assert_eq!(split["left_nodes"], json!(3), "{split}");
    assert_eq!(
        split["right_nodes"],
        json!(2),
        "切口节点两半都有 ⇒ 3 + 2 = 5：{split}"
    );

    let left = nodes_of(&mut workspace, "p1");
    let right = nodes_of(&mut workspace, "p2");
    assert_eq!(left.len(), 3, "{left:?}");
    assert_eq!(right.len(), 2, "{right:?}");
    // 切口节点的归属 ✓：左半末节点的 `out` 归零 ✓、右半首节点的 `in` 归零 ✓（`in`/`out` 各留一半 ✓）。
    assert_eq!(
        left[2]["out"],
        json!([0.0, 0.0]),
        "左半末节点的出柄应归零：{left:?}"
    );
    assert_eq!(
        right[0]["in"],
        json!([0.0, 0.0]),
        "右半首节点的入柄应归零：{right:?}"
    );
    assert_eq!(
        left[2]["in"],
        json!([-20.0, -30.0]),
        "左半末节点的入柄应保留：{left:?}"
    );
    assert_eq!(
        right[0]["out"],
        json!([20.0, -30.0]),
        "右半首节点的出柄应保留：{right:?}"
    );
    assert_eq!(
        left[2]["x"], right[0]["x"],
        "切口处两半共享同一个节点位置 ✓"
    );

    // **墨量之和** ✓（容许几个像素的落笔顺序差异 ✓，实测见断言信息 ✓）。
    let after = ink(&mut workspace);
    let delta = (after as i64 - before as i64).abs();
    assert!(
        delta <= 8,
        "两半墨量之和应等于原墨量（原 {before}、后 {after}、差 {delta}）"
    );

    // 一个变更集 ✓ ⇒ 一次可整体撤销 ✓。
    let changeset = split["changeset_id"]
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
    let state = workspace.document_mut("doc_sm").unwrap().state().clone();
    assert!(
        state
            .objects
            .get("p2")
            .map(|object| object.is_deleted())
            .unwrap_or(true),
        "撤销之后右半应当消失"
    );
    assert_eq!(ink(&mut workspace), before, "撤销之后墨量应回到原样");
}

/// **闭合路径不能切** ✓（一个切口切不开环 ✓）、**边界下标**不能切 ✓、**非路径**不能切 ✓。
#[test]
fn split_refuses_the_cases_the_design_does_not_define() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sm", 160, 100),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    draw_path(&mut workspace, &registry, "p1");
    for (label, at) in [("首节点", 0u64), ("末节点", 3)] {
        let mut ctx = context(&mut workspace);
        let refused = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "split", "object_id": "p1", "at": at}),
        );
        assert_eq!(refused["ok"], json!(false), "{label} 不该能切：{refused}");
    }
    // 闭合之后拒绝 ✓，并且错误里说清**为什么**（需要两刀 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let closed = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "close", "object_id": "p1"}),
        );
        assert_eq!(closed["ok"], json!(true), "{closed}");
        let refused = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "split", "object_id": "p1", "at": 2}),
        );
        assert_eq!(refused["ok"], json!(false), "{refused}");
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("闭合"),
            "错误应说明闭合路径的原因：{detail}"
        );
        assert!(
            detail.contains("两个切口"),
            "错误应说明「需要两个切口」：{detail}"
        );
    }
}

/// **`merge` 的接缝被平滑** ✓：控制柄按弦长 1/3 对齐 ✓，且**渲染与 `join` 不同** ✓。
#[test]
fn merging_paths_smooths_the_seam_unlike_joining() {
    // 两份完全相同的文档 ✓：一份 `join` ✓、一份 `merge` ✓ ⇒ 只差接缝处理 ✓。
    let build = |op: &str| -> (usize, Vec<u8>, Vec<serde_json::Value>) {
        let mut workspace = workspace();
        workspace
            .create_document(
                NewDocument::new("doc_sm", 160, 100),
                "human:1",
                "session:test",
            )
            .unwrap();
        let registry = registry();
        {
            let mut ctx = context(&mut workspace);
            registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
            // 两条各两个节点的**折线**路径 ✓（接缝处原本是折角 ✓）。
            for (id, nodes) in [
                // **两端都要留出"相邻节点"** ✓ —— 切线对齐要取**相邻段的方向** ✓
                // ⇒ 接缝两侧各需至少一个邻居 ✓（我第一版每边只有两个节点 ✗ ⇒
                // 去重后接缝之后没有邻居 ✓ ⇒ 只能退化用弦方向 ✓、测不到切线对齐 ✓）。
                (
                    "a",
                    json!([
                    {"x": 10.0, "y": 20.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                    {"x": 35.0, "y": 45.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                    {"x": 60.0, "y": 50.0, "in": [0.0, 0.0], "out": [0.0, 0.0]}]),
                ),
                // **B 必须与 A 不共线** ✓ —— 我第一版让两条线段斜率相同（都 30/50 ✓）
                // ⇒ 接缝处**本来就没有折角** ⇒ 平滑之后曲线不变 ⇒ 渲染相同 ✓，
                // "merge 与 join 应当不同"的断言自然失败 ✗。
                // **教训** ✓：测"平滑/过渡"这类效果时，测试数据必须**真的有折角** ✓，
                // 否则测的是一个恒等式 ✓。
                (
                    "b",
                    json!([
                    {"x": 60.0, "y": 50.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                    {"x": 72.0, "y": 70.0, "in": [0.0, 0.0], "out": [0.0, 0.0]},
                    {"x": 80.0, "y": 92.0, "in": [0.0, 0.0], "out": [0.0, 0.0]}]),
                ),
            ] {
                let drawn = registry.call(
                    &mut ctx,
                    "draw_stroke",
                    &json!({"layer_id": "L", "object_id": format!("seed_{id}"),
                            "data": {"points": [[0.0, 0.0], [1.0, 1.0]], "size": 4.0,
                                     "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
                );
                assert_eq!(drawn["ok"], json!(true), "{drawn}");
                let converted = registry.call(
                    &mut ctx,
                    "convert_to_path",
                    &json!({"object_id": format!("seed_{id}"), "path_id": id}),
                );
                assert_eq!(converted["ok"], json!(true), "{converted}");
                let set = registry.call(
                    &mut ctx,
                    "set_property",
                    &json!({"object_id": id, "key": "nodes", "value": nodes}),
                );
                assert_eq!(set["ok"], json!(true), "{set}");
            }
            let merged = registry.call(
                &mut ctx,
                "path_edit",
                &json!({"op": op, "object_id": "a", "other_id": "b"}),
            );
            assert_eq!(merged["ok"], json!(true), "{op}: {merged}");
        }
        (
            ink(&mut workspace),
            pixels(&mut workspace),
            nodes_of(&mut workspace, "a"),
        )
    };
    let (join_ink, join_pixels, join_nodes) = build("join");
    let (merge_ink, merge_pixels, merge_nodes) = build("merge");
    // **3 个节点** ✓：两条路径的端点在 (60,50) **重合** ✓ ⇒ 拼接时**去重** ✓
    //（我第一版断言 4 ✓ ⇒ 但那是"留下一个零长度段"的旧行为 ✓，测试当场指出预期要跟着改 ✓）。
    assert_eq!(join_nodes.len(), 5, "3 + 3 − 1（去重）= 5：{join_nodes:?}");
    assert_eq!(merge_nodes.len(), 5, "{merge_nodes:?}");

    // **接缝控制柄** ✓ —— 期望值**从数据推导** ✓（不写死数字 ✓）。
    //
    // `seam` = **B 的首节点下标** ✓：A 有两个节点 ✓ ⇒ 去重后 B 的首节点在 **index 2** ✓
    //（我第一版把 seam 当成 1 ✗ ⇒ 断言查到了 A 的首节点上 ✓、自然不等 ✓ ——
    //  这类"下标差一"的错误正是**断言要打印上下文**才好看出来的 ✓）。
    let seam = 3usize;
    let last = &merge_nodes[seam - 1];
    let first = &merge_nodes[seam];
    let xy = |node: &serde_json::Value| {
        (
            node["x"].as_f64().unwrap_or(0.0),
            node["y"].as_f64().unwrap_or(0.0),
        )
    };
    let (ax, ay) = xy(last);
    let (bx, by) = xy(first);
    let chord = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
    assert!(chord > 1e-9, "接缝弦长不该为零：{merge_nodes:?}");
    // **切线对齐（G1 连续）** ✓：方向取自**相邻段** ✓（进入接缝 = 前一节点 → 接缝 ✓、
    // 离开接缝 = 接缝 → 后一节点 ✓），长度取弦长 1/3 ✓。期望值**从数据推导** ✓，不写死数字 ✓。
    //
    // 我第一版把期望写成"沿弦的 1/3" ✗ —— 而工具当时也正是那么做的 ✓ ⇒ 断言通过 ✓，
    // 但渲染与 `join` **逐像素相同** ✓（弦上的控制点只能表达直线 ✓）⇒ 下一条断言当场揭穿 ✓。
    let (px, py) = xy(&merge_nodes[seam - 2]);
    let (qx, qy) = xy(&merge_nodes[seam + 1]);
    let dir = |dx: f64, dy: f64| {
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 1e-12 {
            (0.0, 0.0)
        } else {
            (dx / length, dy / length)
        }
    };
    let (ix, iy) = dir(ax - px, ay - py);
    let (ox, oy) = dir(qx - bx, qy - by);
    let scale = chord / 3.0;
    let (dx, dy) = (ix * scale, iy * scale);
    let (ex, ey) = (-ox * scale, -oy * scale);
    assert_eq!(
        join_nodes[seam - 1]["out"],
        json!([0.0, 0.0]),
        "join 不动控制柄：{join_nodes:?}"
    );
    assert_eq!(
        merge_nodes[seam - 1]["out"],
        json!([dx, dy]),
        "merge 应设接缝出柄：{merge_nodes:?}"
    );
    assert_eq!(
        merge_nodes[seam]["in"],
        json!([ex, ey]),
        "merge 应设接缝入柄：{merge_nodes:?}"
    );

    // **渲染确实不同** ✓（这是"两个名字给了两种语义"的证据 ✓）。
    let (first, count) = diff(&join_pixels, &merge_pixels)
        .unwrap_or_else(|| panic!("merge 与 join 的渲染应当不同（否则两个名字就是多余的）"));
    assert!(count > 0, "差 {count} 字节（首个 #{first}）");
    // **墨量应几乎不变** ✓：接缝被平滑只是把折角变成弧线 ✓（弧线略短于折线 ✓）。
    let delta = (merge_ink as i64 - join_ink as i64).abs();
    assert!(
        delta <= 40,
        "接缝平滑不该显著改变墨量（join {join_ink} vs merge {merge_ink}）"
    );
}

/// **`merge` 只作用于路径** ✓ —— 笔迹没有控制柄 ✓ ⇒ 明确报错 ✓，不静默降级成 `join` ✗。
#[test]
fn merge_refuses_strokes_instead_of_silently_joining() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_sm", 160, 100),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        for id in ["s1", "s2"] {
            registry.call(
                &mut ctx,
                "draw_stroke",
                &json!({"layer_id": "L", "object_id": id,
                        "data": {"points": [[10.0, 20.0], [60.0, 50.0]], "size": 4.0,
                                 "color": {"r": 20, "g": 20, "b": 20, "a": 255}}}),
            );
        }
        let refused = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "merge", "object_id": "s1", "other_id": "s2"}),
        );
        assert_eq!(refused["ok"], json!(false), "{refused}");
        let detail = refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            detail.contains("merge 只作用于路径"),
            "错误应说清原因：{detail}"
        );
        // `join` 对笔迹仍然可用 ✓（现有行为不变 ✓）。
        let joined = registry.call(
            &mut ctx,
            "path_edit",
            &json!({"op": "join", "object_id": "s1", "other_id": "s2"}),
        );
        assert_eq!(joined["ok"], json!(true), "{joined}");
    }
}
