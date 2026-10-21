//! A1 路径数据面端到端判据：`get_path` 读回往返恒等、`edit_path` 节点级
//! 编辑（局部性 + 读回核对）、`create_selection` 的 `shape:{object_id}`
//! 引用形态与内联形态逐像素等价。
//!
//! 往返恒等的口径是**渲染逐像素一致**（不是「不报错」）：读回的
//! `geometry` 与 `d` 分别重建同一形状，三次渲染两两逐字节相等。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_a1", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let registry = registry();
    let mut ctx = context(workspace);
    registry.call(&mut ctx, tool, &args)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_a1", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("渲染应成功")
        .2
}

/// 建文档 + 图层 + 白底（后续形状画在白底上，比较不依赖缺省背景）。
fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_a1", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let made = call(workspace, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(made["ok"], json!(true), "{made}");
    let filled = call(
        workspace,
        "fill",
        json!({"layer_id": "L", "object_id": "bg",
                "data": {"color": {"r": 255, "g": 255, "b": 255, "a": 255},
                         "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
}

/// 双环方框（外 48×48、内 16×16 洞，全直段）：bbox / 总长都可手算。
fn donut_subpaths() -> Value {
    json!([
        {"start": [16.0, 16.0], "closed": true, "segments": [
            {"op": "L", "to": [64.0, 16.0]},
            {"op": "L", "to": [64.0, 64.0]},
            {"op": "L", "to": [16.0, 64.0]}
        ]},
        {"start": [24.0, 24.0], "closed": true, "segments": [
            {"op": "L", "to": [40.0, 24.0]},
            {"op": "L", "to": [40.0, 40.0]},
            {"op": "L", "to": [24.0, 40.0]}
        ]}
    ])
}

fn draw_path(workspace: &mut Workspace, object_id: &str, geometry: Value) -> Value {
    call(
        workspace,
        "draw_shape",
        json!({"layer_id": "L", "object_id": object_id,
                "data": {"geometry": geometry,
                         "color": {"r": 220, "g": 40, "b": 40, "a": 255}}}),
    )
}

#[test]
fn get_path_readback_and_roundtrip_identity() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let drawn = draw_path(
        &mut workspace,
        "p1",
        json!({"kind": "path", "fill_rule": "evenodd", "subpaths": donut_subpaths()}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let img1 = render(&mut workspace);

    let got = call(&mut workspace, "get_path", json!({"object_id": "p1"}));
    assert_eq!(got["ok"], json!(true), "{got}");
    // 结构化读回与写入逐值一致（数字在测试里全用 f64 字面量）。
    assert_eq!(got["subpaths"], donut_subpaths(), "{got}");
    assert_eq!(got["fill_rule"], json!("evenodd"));
    // 等价 d：绝对命令、最短往返数字、Z 收尾。
    assert_eq!(
        got["d"],
        json!("M 16 16 L 64 16 L 64 64 L 16 64 Z M 24 24 L 40 24 L 40 40 L 24 40 Z"),
        "{got}"
    );
    // 控制点包围盒 = 外环范围；总长 = 外环 192 + 内环 64 = 256（直段精确）。
    assert_eq!(
        got["bbox"],
        json!({"x": 16.0, "y": 16.0, "w": 48.0, "h": 48.0})
    );
    assert_eq!(got["length"], json!(256.0), "{got}");
    assert_eq!(got["node_counts"], json!([4, 4]));

    // 往返一：读回的 geometry 原样喂回 draw_shape ⇒ 逐像素一致。
    let removed = call(&mut workspace, "delete_object", json!({"object_id": "p1"}));
    assert_eq!(removed["ok"], json!(true), "{removed}");
    let redrawn = draw_path(&mut workspace, "p2", got["geometry"].clone());
    assert_eq!(redrawn["ok"], json!(true), "{redrawn}");
    let img2 = render(&mut workspace);
    assert_eq!(img1, img2, "geometry 往返重建应逐像素一致");

    // 往返二：读回的 d 字符串重建 ⇒ 逐像素一致。
    let removed = call(&mut workspace, "delete_object", json!({"object_id": "p2"}));
    assert_eq!(removed["ok"], json!(true), "{removed}");
    let redrawn = draw_path(
        &mut workspace,
        "p3",
        json!({"kind": "path", "fill_rule": "evenodd", "d": got["d"]}),
    );
    assert_eq!(redrawn["ok"], json!(true), "{redrawn}");
    let img3 = render(&mut workspace);
    assert_eq!(img1, img3, "d 往返重建应逐像素一致");
}

#[test]
fn get_path_errors_are_explicit() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // 不存在 ⇒ 明确报错、点名 id。
    let missing = call(&mut workspace, "get_path", json!({"object_id": "ghost"}));
    assert_ne!(missing["ok"], json!(true), "{missing}");
    assert!(
        missing.to_string().contains("ghost") && missing.to_string().contains("不存在"),
        "错误应点名 id 并说明不存在，实测：{missing}"
    );
    // 不是 path 形状（白底 fill 是 shape/rect）⇒ 明确报错。
    let rect = call(&mut workspace, "get_path", json!({"object_id": "bg"}));
    assert_ne!(rect["ok"], json!(true), "{rect}");
    assert!(
        rect.to_string().contains("不是 path 形状"),
        "错误应说明不是 path 形状，实测：{rect}"
    );
    // 已删除 ⇒ 明确报错。
    let drawn = draw_path(
        &mut workspace,
        "pd",
        json!({"kind": "path", "subpaths": donut_subpaths()}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let removed = call(&mut workspace, "delete_object", json!({"object_id": "pd"}));
    assert_eq!(removed["ok"], json!(true), "{removed}");
    let deleted = call(&mut workspace, "get_path", json!({"object_id": "pd"}));
    assert_ne!(deleted["ok"], json!(true), "{deleted}");
    assert!(
        deleted.to_string().contains("已被删除"),
        "错误应说明已被删除，实测：{deleted}"
    );
}

/// 两个分离方框同一对象（局部性判据的素材）：A 在左上、B 在右下。
fn two_squares() -> Value {
    json!([
        {"start": [8.0, 8.0], "closed": true, "segments": [
            {"op": "L", "to": [40.0, 8.0]},
            {"op": "L", "to": [40.0, 40.0]},
            {"op": "L", "to": [8.0, 40.0]}
        ]},
        {"start": [88.0, 88.0], "closed": true, "segments": [
            {"op": "L", "to": [120.0, 88.0]},
            {"op": "L", "to": [120.0, 120.0]},
            {"op": "L", "to": [88.0, 120.0]}
        ]}
    ])
}

#[test]
fn edit_path_move_node_is_local() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let drawn = draw_path(
        &mut workspace,
        "e1",
        json!({"kind": "path", "fill_rule": "evenodd", "subpaths": two_squares()}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let before = render(&mut workspace);

    // 把 A 的节点 1（(40,8)）移到 (48,8)。
    let edited = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e1", "op": "move_node", "subpath": 0, "node": 1, "to": [48.0, 8.0]}),
    );
    assert_eq!(edited["ok"], json!(true), "{edited}");
    assert_eq!(
        edited["subpaths"][0]["segments"][0]["to"],
        json!([48.0, 8.0]),
        "{edited}"
    );
    let after = render(&mut workspace);

    // 局部性强断言：B 所在区域（x,y ≥ 80）逐像素零变化；全部变化都落在
    // A 的新旧范围并集（x ≤ 56、y ≤ 48，含抗锯齿余量）之内；且确实有变化。
    let mut changed = 0usize;
    for y in 0..128usize {
        for x in 0..128usize {
            let i = (y * 128 + x) * 4;
            if before[i..i + 4] != after[i..i + 4] {
                changed += 1;
                assert!(
                    x <= 56 && y <= 48,
                    "编辑点之外的像素 ({x},{y}) 变了（局部性被破坏）"
                );
            }
        }
    }
    assert!(changed > 50, "移动节点应产生可见变化，实测 {changed} 像素");
    for y in 80..128usize {
        for x in 80..128usize {
            let i = (y * 128 + x) * 4;
            assert_eq!(
                before[i..i + 4],
                after[i..i + 4],
                "方框 B 的像素 ({x},{y}) 不应变化"
            );
        }
    }
    // 读回核对：bbox 仍含 B（总盒不变），节点数不变。
    let got = call(&mut workspace, "get_path", json!({"object_id": "e1"}));
    assert_eq!(got["node_counts"], json!([4, 4]), "{got}");
    assert_eq!(
        got["bbox"],
        json!({"x": 8.0, "y": 8.0, "w": 112.0, "h": 112.0})
    );
}

#[test]
fn edit_path_insert_delete_set_handle_via_readback() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let drawn = draw_path(
        &mut workspace,
        "e2",
        json!({"kind": "path", "subpaths": [
            {"start": [16.0, 16.0], "closed": false, "segments": [
                {"op": "L", "to": [64.0, 16.0]},
                {"op": "L", "to": [64.0, 64.0]}
            ]}
        ]}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let before = render(&mut workspace);

    // insert_node（直段中点）：几何逐位不变 ⇒ 渲染逐像素不变（强恒等）。
    let inserted = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "insert_node", "subpath": 0, "segment": 0, "t": 0.5}),
    );
    assert_eq!(inserted["ok"], json!(true), "{inserted}");
    assert_eq!(inserted["new_node"], json!(1));
    assert_eq!(inserted["node_counts"], json!([4]));
    let after_insert = render(&mut workspace);
    assert_eq!(before, after_insert, "直段插点不应改变任何像素");

    // set_handle（节点 1 的 out，直段升格）：c1 = 给定点、c2 = 段上 2/3 点。
    // 段是 (40,16)→(64,16)（插点后），2/3 点 = (56,16)。
    let handled = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "set_handle", "subpath": 0, "node": 1,
                "side": "out", "to": [40.0, 40.0]}),
    );
    assert_eq!(handled["ok"], json!(true), "{handled}");
    assert_eq!(
        handled["subpaths"][0]["segments"][1],
        json!({"op": "C", "c1": [40.0, 40.0], "c2": [56.0, 16.0], "to": [64.0, 16.0]}),
        "{handled}"
    );

    // delete_node（删刚插的节点 1）：相邻两段合并——入段是直段、出段是
    // 三次段 ⇒ 三次段，c1 取被删锚点 (40,16)、c2 取出段 c2 (56,16)。
    let deleted = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "delete_node", "subpath": 0, "node": 1}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    assert_eq!(deleted["node_counts"], json!([3]));
    assert_eq!(
        deleted["subpaths"][0]["segments"][0],
        json!({"op": "C", "c1": [40.0, 16.0], "c2": [56.0, 16.0], "to": [64.0, 16.0]}),
        "{deleted}"
    );

    // 错误面：未知算子 / 越界 subpath / 越界 node / 开放末节点 set_handle out。
    let bad_op = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "explode", "subpath": 0}),
    );
    assert_ne!(bad_op["ok"], json!(true), "{bad_op}");
    let bad_sub = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "move_node", "subpath": 7, "node": 0, "to": [0.0, 0.0]}),
    );
    assert_ne!(bad_sub["ok"], json!(true), "{bad_sub}");
    assert!(bad_sub.to_string().contains("越界"), "{bad_sub}");
    let bad_node = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "move_node", "subpath": 0, "node": 42, "to": [0.0, 0.0]}),
    );
    assert_ne!(bad_node["ok"], json!(true), "{bad_node}");
    let bad_handle = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "e2", "op": "set_handle", "subpath": 0, "node": 2,
                "side": "out", "to": [0.0, 0.0]}),
    );
    assert_ne!(bad_handle["ok"], json!(true), "{bad_handle}");
    assert!(
        bad_handle.to_string().contains("没有 out 控制柄"),
        "{bad_handle}"
    );
    // 对非 path 对象编辑 ⇒ 明确报错。
    let not_path = call(
        &mut workspace,
        "edit_path",
        json!({"object_id": "bg", "op": "move_node", "subpath": 0, "node": 0, "to": [0.0, 0.0]}),
    );
    assert_ne!(not_path["ok"], json!(true), "{not_path}");
}

/// 圆弧近似圆（4 段三次）：选区等价判据的曲线素材（圆心 (64,64)、r=30）。
fn circle_subpath() -> Value {
    const KAPPA: f64 = 0.552_284_749_830_79;
    let (cx, cy, r) = (64.0_f64, 64.0_f64, 30.0_f64);
    let d = r * KAPPA;
    json!({
        "start": [cx + r, cy], "closed": true,
        "segments": [
            {"op": "C", "c1": [cx + r, cy + d], "c2": [cx + d, cy + r], "to": [cx, cy + r]},
            {"op": "C", "c1": [cx - d, cy + r], "c2": [cx - r, cy + d], "to": [cx - r, cy]},
            {"op": "C", "c1": [cx - r, cy - d], "c2": [cx - d, cy - r], "to": [cx, cy - r]},
            {"op": "C", "c1": [cx + d, cy - r], "c2": [cx + r, cy - d], "to": [cx + r, cy]}
        ]
    })
}

fn draw_line(workspace: &mut Workspace, object_id: &str) -> Value {
    call(
        workspace,
        "draw_stroke",
        json!({"layer_id": "L", "object_id": object_id,
                "data": {"points": [[4.0, 64.0], [124.0, 64.0]], "size": 20.0,
                         "hardness": 1.0,
                         "color": {"r": 250, "g": 240, "b": 20, "a": 255}}}),
    )
}

fn changed_mask(workspace: &mut Workspace, baseline: &[u8], line_id: &str) -> Vec<bool> {
    let drawn = draw_line(workspace, line_id);
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let after = render(workspace);
    (0..128 * 128)
        .map(|i| after[i * 4..i * 4 + 4] != baseline[i * 4..i * 4 + 4])
        .collect()
}

#[test]
fn selection_by_object_id_matches_inline_and_errors_explicitly() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // 被引用的 path 对象（画出来只为让它存在；等价比较用「落笔改动掩码」，
    // 与 selection_path.rs 同口径，基准渲染已包含它）。
    let drawn = draw_path(
        &mut workspace,
        "src",
        json!({"kind": "path", "fill_rule": "evenodd", "subpaths": [circle_subpath()]}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let baseline = render(&mut workspace);

    // 引用形态建选区。
    let by_ref = call(
        &mut workspace,
        "create_selection",
        json!({"selection_id": "sel_ref",
                "shape": {"kind": "path", "object_id": "src"},
                "feather": 0.0, "invert": false, "mode": "new"}),
    );
    assert_eq!(by_ref["ok"], json!(true), "{by_ref}");
    // 落库是展开后的内联快照：有 subpaths、有 resolved_from、无 object_id。
    let listed = call(&mut workspace, "list_selections", json!({}));
    let shape = &listed["selections"][0]["shape"];
    assert_eq!(shape["kind"], json!("path"), "{listed}");
    assert_eq!(shape["resolved_from"], json!("src"), "{listed}");
    assert!(
        shape.get("object_id").is_none(),
        "落库不应残留 object_id：{shape}"
    );
    assert_eq!(
        shape["subpaths"],
        json!([circle_subpath()]),
        "展开的 subpaths 应与源对象逐值一致：{shape}"
    );
    let changed_ref = changed_mask(&mut workspace, &baseline, "line_ref");
    call(
        &mut workspace,
        "delete_object",
        json!({"object_id": "line_ref"}),
    );
    call(
        &mut workspace,
        "delete_selection",
        json!({"selection_id": "sel_ref"}),
    );

    // 内联形态建同一选区 ⇒ 约束落笔逐像素一致。
    let inline = call(
        &mut workspace,
        "create_selection",
        json!({"selection_id": "sel_inline",
                "shape": {"kind": "path", "fill_rule": "evenodd",
                          "subpaths": [circle_subpath()]},
                "feather": 0.0, "invert": false, "mode": "new"}),
    );
    assert_eq!(inline["ok"], json!(true), "{inline}");
    let changed_inline = changed_mask(&mut workspace, &baseline, "line_inline");
    let count = changed_ref.iter().filter(|&&c| c).count();
    assert!(count > 500, "圆内横线应改动数百像素，实测 {count}");
    assert_eq!(
        changed_ref, changed_inline,
        "引用建的选区与内联同几何建的选区应逐像素一致"
    );

    // 坏 id ⇒ 明确报错、不静默回退（不存在 / 不是 path / 二选一）。
    let ghost = call(
        &mut workspace,
        "create_selection",
        json!({"selection_id": "sel_ghost",
                "shape": {"kind": "path", "object_id": "nope"}}),
    );
    assert_ne!(ghost["ok"], json!(true), "{ghost}");
    assert!(ghost.to_string().contains("不存在"), "{ghost}");
    let not_path = call(
        &mut workspace,
        "create_selection",
        json!({"selection_id": "sel_rect",
                "shape": {"kind": "path", "object_id": "bg"}}),
    );
    assert_ne!(not_path["ok"], json!(true), "{not_path}");
    assert!(
        not_path.to_string().contains("不是 path 形状"),
        "{not_path}"
    );
    let both = call(
        &mut workspace,
        "create_selection",
        json!({"selection_id": "sel_both",
                "shape": {"kind": "path", "object_id": "src",
                          "subpaths": [circle_subpath()]}}),
    );
    assert_ne!(both["ok"], json!(true), "{both}");
    assert!(both.to_string().contains("二选一"), "{both}");
}
