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
