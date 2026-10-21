//! 选区管线认 `path` 形状（曲线线第三包）。
//!
//! 本包之前的现状：`create_selection` 只透传 shape JSON 不校验几何，而渲染期
//! `SelectionShape` 不认 `path` ⇒ 落进未知 kind 的「覆盖全部」分支 ⇒
//! **path 选区会静默变成全选**（静默降级）。本文件逐条锁住修复：
//! 曲线选区约束落笔、evenodd 挖孔、位图布尔合并（subtract)、
//! `transform_selection` 对 path 的精确仿射。
//!
//! 判据刻意不依赖具体颜色：只比较「绘制前后哪些像素被改动」及其位置
//! （与 `selection_clip.rs` 同一口径）。

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
    ToolContext::new(workspace, "doc_selpath", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_selpath", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("渲染应成功")
        .2
}

fn pixel(img: &[u8], x: usize, y: usize) -> &[u8] {
    let index = (y * 128 + x) * 4;
    &img[index..index + 4]
}

/// 圆弧近似的圆子路径（4 段三次 Bézier）：圆心 (cx, cy)、半径 r。
fn circle_subpath(cx: f64, cy: f64, r: f64) -> serde_json::Value {
    const KAPPA: f64 = 0.552_284_749_830_79;
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

/// 建文档 + 图层 + 底色，返回底色渲染（后续比较的基准）。
fn setup(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .create_document(
            NewDocument::new("doc_selpath", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    }
    let filled = {
        let mut ctx = context(workspace);
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L", "object_id": "a_background",
                    "data": {"color": {"r": 40, "g": 120, "b": 200, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
        )
    };
    assert_eq!(filled["ok"], json!(true), "{filled}");
    render(workspace)
}

fn draw_line(
    workspace: &mut Workspace,
    object_id: &str,
    points: serde_json::Value,
    size: f64,
) -> serde_json::Value {
    let registry = registry();
    let mut ctx = context(workspace);
    registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": object_id,
                "data": {"points": points, "size": size,
                         "hardness": 1.0,
                         "color": {"r": 250, "g": 240, "b": 20, "a": 255}}}),
    )
}

fn create_selection(
    workspace: &mut Workspace,
    id: &str,
    shape: serde_json::Value,
    mode: &str,
) -> serde_json::Value {
    let registry = registry();
    let mut ctx = context(workspace);
    registry.call(
        &mut ctx,
        "create_selection",
        &json!({"selection_id": id, "shape": shape,
                "feather": 0.0, "invert": false, "mode": mode}),
    )
}

#[test]
fn path_selection_clips_stroke_to_curve_area() {
    let mut workspace = workspace();
    let baseline = setup(&mut workspace);

    // 圆选区：圆心 (64,64)、r=30。
    let created = create_selection(
        &mut workspace,
        "sel1",
        json!({"kind": "path", "subpaths": [circle_subpath(64.0, 64.0, 30.0)]}),
        "new",
    );
    assert_eq!(created["ok"], json!(true), "{created}");

    // 横贯整幅的粗线：只应改动圆内的像素。
    let drawn = draw_line(
        &mut workspace,
        "z_line",
        json!([[4.0, 64.0], [124.0, 64.0]]),
        20.0,
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let after = render(&mut workspace);

    assert_ne!(
        pixel(&after, 64, 64),
        pixel(&baseline, 64, 64),
        "圆心应被落笔改动（选区内）"
    );
    assert_eq!(
        pixel(&after, 10, 64),
        pixel(&baseline, 10, 64),
        "圆外 (10,64) 不应被改动（修复前 path 落进全选分支，这里会变色）"
    );
    // 全部改动像素都必须落在圆内（带 1.5px 抗锯齿余量）。
    for y in 0..128usize {
        for x in 0..128usize {
            if pixel(&after, x, y) != pixel(&baseline, x, y) {
                let dx = x as f64 - 64.0;
                let dy = y as f64 - 64.0;
                assert!(
                    (dx * dx + dy * dy).sqrt() <= 31.5,
                    "改动像素 ({x},{y}) 距圆心超过 r+1.5"
                );
            }
        }
    }
}

#[test]
fn path_selection_evenodd_hole_stays_unpainted() {
    let mut workspace = workspace();
    let baseline = setup(&mut workspace);

    // 同心双环 evenodd：外 r=34、内 r=12，中心是洞。
    let created = create_selection(
        &mut workspace,
        "sel1",
        json!({"kind": "path", "fill_rule": "evenodd",
               "subpaths": [circle_subpath(64.0, 64.0, 34.0),
                            circle_subpath(64.0, 64.0, 12.0)]}),
        "new",
    );
    assert_eq!(created["ok"], json!(true), "{created}");

    // 大胶囊笔触盖住整片区域（无选区时中心、环带、外侧都会被改）。
    let drawn = draw_line(
        &mut workspace,
        "z_blob",
        json!([[40.0, 64.0], [88.0, 64.0]]),
        76.0,
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let after = render(&mut workspace);

    assert_eq!(
        pixel(&after, 64, 64),
        pixel(&baseline, 64, 64),
        "evenodd 洞（中心）不应被落笔改动"
    );
    assert_ne!(
        pixel(&after, 87, 64),
        pixel(&baseline, 87, 64),
        "环带（距中心 23，介于 12 与 34 之间）应被改动"
    );
    assert_eq!(
        pixel(&after, 104, 64),
        pixel(&baseline, 104, 64),
        "外环之外（距中心 40）不应被改动"
    );
}

#[test]
fn path_selection_subtract_merge_uses_bitmap_boolean() {
    let mut workspace = workspace();
    let _baseline = setup(&mut workspace);

    // 旧形状用扁平 x/y/w/h 形：`shape_json_bbox`（布尔合并的 bbox 入口）的
    // rect 臂只认这一形（既存契约，本包未动）；`bbox` 对象形它取不到盒。
    let created = create_selection(
        &mut workspace,
        "s",
        json!({"kind": "rect", "x": 16.0, "y": 16.0, "w": 96.0, "h": 96.0}),
        "new",
    );
    assert_eq!(created["ok"], json!(true), "{created}");

    // subtract 一个 path 圆：走既有位图布尔合并（修复前 shape_json_bbox 不认
    // path ⇒ 合并返回空 ⇒ 这里会报「空选区」错）。
    let merged = create_selection(
        &mut workspace,
        "s",
        json!({"kind": "path", "subpaths": [circle_subpath(64.0, 64.0, 18.0)]}),
        "subtract",
    );
    assert_eq!(
        merged["ok"],
        json!(true),
        "path subtract 合并应成功：{merged}"
    );

    // 合并产物是位图选区（结构断言）。
    let listed = {
        let registry = registry();
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "list_selections", &json!({}))
    };
    assert_eq!(
        listed["selections"][0]["shape"]["kind"],
        json!("bitmap"),
        "合并后应为位图选区：{listed}"
    );
    // 合并内容断言（走 list 的面积：bitmap 面积是解 blob 数非零像素，精确口径）：
    // 期望 ≈ 矩形 96×96 − 圆 π·18² ≈ 8198。若 path 覆盖率仍落进全选分支，
    // 合并结果为空会先在上面报错；若 path 覆盖率恒 0，面积会是 9216 ⇒ 这里红。
    // 注：合并后位图选区的**渲染裁剪**不在本用例范围——已用对照实验确认：
    // 素的 rect−rect subtract 合并在本测试 harness 下落笔同样被全面抑制
    // （changed=0），是与本包无关的既存行为（见包内 README）。
    let area = listed["selections"][0]["area"].as_f64().unwrap();
    assert_eq!(
        listed["selections"][0]["area_exact"],
        json!(true),
        "{listed}"
    );
    let expected = 96.0 * 96.0 - std::f64::consts::PI * 18.0 * 18.0;
    assert!(
        (area - expected).abs() <= 250.0,
        "合并面积 {area} 与期望 {expected:.0}（矩形 − 圆）偏差过大"
    );
}

#[test]
fn transform_selection_moves_and_rotates_path_exactly() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_selpath", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();

    let original = json!({
        "kind": "path", "fill_rule": "nonzero",
        "subpaths": [{
            "start": [20.0, 20.0], "closed": true,
            "segments": [
                {"op": "C", "c1": [50.0, 10.0], "c2": [90.0, 30.0], "to": [100.0, 60.0]},
                {"op": "L", "to": [100.0, 100.0]},
                {"op": "L", "to": [20.0, 100.0]}
            ]
        }]
    });
    let created = create_selection(&mut workspace, "tp", original.clone(), "new");
    assert_eq!(created["ok"], json!(true), "{created}");

    let listed_shape = |workspace: &mut Workspace| -> serde_json::Value {
        let registry = registry();
        let mut ctx = context(workspace);
        let listed = registry.call(&mut ctx, "list_selections", &json!({}));
        listed["selections"][0]["shape"].clone()
    };

    // 平移 (10, 5)：全部坐标点精确平移（修复前 selection_shape_bbox 不认
    // path ⇒ transform 直接报「无法解析包围盒」）。
    let moved = {
        let reg = registry();
        let mut ctx = context(&mut workspace);
        reg.call(
            &mut ctx,
            "transform_selection",
            &json!({"selection_id": "tp", "translate": [10.0, 5.0]}),
        )
    };
    assert_eq!(moved["ok"], json!(true), "{moved}");
    let shape = listed_shape(&mut workspace);
    assert_eq!(shape["kind"], json!("path"), "平移后应仍是 path：{shape}");
    assert_eq!(shape["subpaths"][0]["start"], json!([30.0, 25.0]));
    assert_eq!(
        shape["subpaths"][0]["segments"][0]["c1"],
        json!([60.0, 15.0])
    );
    assert_eq!(
        shape["subpaths"][0]["segments"][0]["to"],
        json!([110.0, 65.0])
    );
    assert_eq!(
        shape["subpaths"][0]["segments"][2]["to"],
        json!([30.0, 105.0])
    );

    // 旋转 90°×4 = 恒等（关系断言）：kind 保持 path、坐标回到原值（浮点容差）。
    for i in 0..4 {
        let rotated = {
            let reg = registry();
            let mut ctx = context(&mut workspace);
            reg.call(
                &mut ctx,
                "transform_selection",
                &json!({"selection_id": "tp", "rotate_deg": 90.0, "pivot": [64.0, 64.0]}),
            )
        };
        assert_eq!(
            rotated["ok"],
            json!(true),
            "第 {} 次旋转应成功：{rotated}",
            i + 1
        );
        if i == 0 {
            let once = listed_shape(&mut workspace);
            assert_eq!(
                once["kind"],
                json!("path"),
                "旋转后应仍是 path（不转 polygon）：{once}"
            );
            // 旋转保距：start 到 pivot 的距离不变。
            let start = &once["subpaths"][0]["start"];
            let (sx, sy) = (start[0].as_f64().unwrap(), start[1].as_f64().unwrap());
            let dist = ((sx - 64.0).powi(2) + (sy - 64.0).powi(2)).sqrt();
            let orig = ((30.0f64 - 64.0).powi(2) + (25.0f64 - 64.0).powi(2)).sqrt();
            assert!((dist - orig).abs() < 1e-6, "旋转应保距：{dist} vs {orig}");
        }
    }
    let back = listed_shape(&mut workspace);
    let start = &back["subpaths"][0]["start"];
    assert!(
        (start[0].as_f64().unwrap() - 30.0).abs() < 1e-6
            && (start[1].as_f64().unwrap() - 25.0).abs() < 1e-6,
        "旋转四次应回到平移后的 start (30,25)，实际 {start}"
    );
    assert_eq!(back["fill_rule"], json!("nonzero"), "fill_rule 应保持");
}

// ---------------------------------------------------------------------------
// F2：create_selection 的 path 选区直收 `d`（此前只认 subpaths，带 d 的
// 形状在渲染期被 parse_path_subpaths 判非法 ⇒ 静默退化成空选；v8 实测
// 只能先建蒙版中转）。入口转 subpaths（与 create_mask 同口径，只用既有
// parse_svg_d 一份解析器），落库只存结构化形态。
// ---------------------------------------------------------------------------

/// 与 `circle_subpath(64, 64, 30)` 等价的 d 字符串（同一 kappa 常数、同一批
/// 坐标，f64 最短往返格式 ⇒ 解析回的 f64 与 JSON 数字逐位相同）。
fn circle_d_64_30() -> String {
    const KAPPA: f64 = 0.552_284_749_830_79;
    let (cx, cy, r) = (64.0_f64, 64.0_f64, 30.0_f64);
    let d = r * KAPPA;
    format!(
        "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} Z",
        cx + r,
        cy,
        cx + r,
        cy + d,
        cx + d,
        cy + r,
        cx,
        cy + r,
        cx - d,
        cy + r,
        cx - r,
        cy + d,
        cx - r,
        cy,
        cx - r,
        cy - d,
        cx - d,
        cy - r,
        cx,
        cy - r,
        cx + d,
        cy - r,
        cx + r,
        cy - d,
        cx + r,
        cy
    )
}

fn call_tool(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let registry = registry();
    let mut ctx = context(workspace);
    registry.call(&mut ctx, tool, &args)
}

/// 一条横贯线在选区约束下的改动像素掩码（与基准逐像素比）。
fn changed_mask(workspace: &mut Workspace, baseline: &[u8], line_id: &str) -> Vec<bool> {
    let drawn = draw_line(
        workspace,
        line_id,
        json!([[4.0, 64.0], [124.0, 64.0]]),
        20.0,
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let after = render(workspace);
    (0..128 * 128)
        .map(|i| after[i * 4..i * 4 + 4] != baseline[i * 4..i * 4 + 4])
        .collect()
}

#[test]
fn path_selection_accepts_d_and_matches_subpaths_form() {
    let mut workspace = workspace();
    let baseline = setup(&mut workspace);

    // 基准相：subpaths 写法。
    let created = create_selection(
        &mut workspace,
        "sel_sub",
        json!({"kind": "path", "subpaths": [circle_subpath(64.0, 64.0, 30.0)]}),
        "new",
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let changed_sub = changed_mask(&mut workspace, &baseline, "line_sub");
    let removed = call_tool(
        &mut workspace,
        "delete_object",
        json!({"object_id": "line_sub"}),
    );
    assert_eq!(removed["ok"], json!(true), "{removed}");
    let removed = call_tool(
        &mut workspace,
        "delete_selection",
        json!({"selection_id": "sel_sub"}),
    );
    assert_eq!(removed["ok"], json!(true), "{removed}");

    // d 相：同一圆改用 d 字符串。
    let created = create_selection(
        &mut workspace,
        "sel_d",
        json!({"kind": "path", "d": circle_d_64_30()}),
        "new",
    );
    assert_eq!(created["ok"], json!(true), "d 写法应被接受：{created}");

    // 落库只存结构化形态：读回的 shape 应有 subpaths、无 d。
    let listed = call_tool(&mut workspace, "list_selections", json!({}));
    let shape = &listed["selections"][0]["shape"];
    assert_eq!(shape["kind"], json!("path"), "{listed}");
    assert!(
        shape.get("subpaths").is_some_and(|s| s.is_array()),
        "落库 shape 应含 subpaths：{shape}"
    );
    assert!(shape.get("d").is_none(), "落库 shape 不应残留 d：{shape}");

    let changed_d = changed_mask(&mut workspace, &baseline, "line_d");
    let count = changed_d.iter().filter(|&&c| c).count();
    assert!(count > 500, "圆内横线应改动数百像素，实测 {count}");
    assert_eq!(
        changed_sub, changed_d,
        "d 写法与 subpaths 写法的选区约束应逐像素一致"
    );
}

#[test]
fn path_selection_d_argument_errors() {
    let mut workspace = workspace();
    let _baseline = setup(&mut workspace);

    // d 与 subpaths 同给 ⇒ 二选一报错。
    let both = create_selection(
        &mut workspace,
        "sel_both",
        json!({"kind": "path", "d": circle_d_64_30(),
               "subpaths": [circle_subpath(64.0, 64.0, 30.0)]}),
        "new",
    );
    assert_ne!(both["ok"], json!(true), "同给应失败：{both}");
    assert!(
        both.to_string().contains("二选一"),
        "错误应说明二选一，实测：{both}"
    );

    // d 语法坏 ⇒ 入口报错（带字符位置），不静默落库成空选。
    let bad = create_selection(
        &mut workspace,
        "sel_bad",
        json!({"kind": "path", "d": "M 10 10 C 1 2 3"}),
        "new",
    );
    assert_ne!(bad["ok"], json!(true), "坏 d 应失败：{bad}");
    assert!(
        bad.to_string().contains("d 非法"),
        "错误应指明 d 非法，实测：{bad}"
    );

    // kind=path 既无 d 也无 subpaths ⇒ 入口报错，不再静默退化成空选。
    let empty = create_selection(&mut workspace, "sel_none", json!({"kind": "path"}), "new");
    assert_ne!(empty["ok"], json!(true), "无载荷应失败：{empty}");
}
