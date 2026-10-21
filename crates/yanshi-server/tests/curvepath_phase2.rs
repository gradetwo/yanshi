//! **曲线/路径二期端到端判据**（详细设计 §9 二期预留四项）。
//!
//! - mask 源 = path：内联 subpaths 与 object_id 引用两种来源都真生效
//!   （像素关系与无蒙版文档对照）；引用删掉后蒙版失效是**可观测**的
//!   （内容恢复显示），入口校验二选一/引用不存在都明确报错。
//! - path_guide 曲线尺吸附：远处 guide ⇒ 与不传逐字节一致（回归锁）；
//!   近处 guide ⇒ 墨迹质心被拉向 guide（结构量）；参数错误明确报。
//! - SVG d 语法糖：draw_shape 用 d 与用等价 subpaths 渲染逐字节一致、
//!   落库后只剩 subpaths（d 已摘掉）；stroke_path 同理；同给报错、
//!   语法错误带字符位置。
//! - combine_shapes：union 面积对解析值（相对断言）、subtract 出带孔
//!   path、intersect 空集报错、输入不被 tombstone。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: &Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, args)
}

/// brush_stroke 要读真 .myb ⇒ 落盘工作区 + 仓库 assets（与一期同款脚手架）。
fn assets_workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!(
        "yanshi_curvepath2_ws_{}_{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets))
}

fn make_doc(workspace: &mut Workspace, doc: &str, width: u32, height: u32) {
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    let made = call(workspace, doc, "create_layer", &json!({ "layer_id": "L" }));
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn render(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> Vec<u8> {
    workspace
        .render_region_raw(doc, Bbox::new(0.0, 0.0, width as f64, height as f64))
        .expect("渲染应成功")
        .2
}

fn px(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

fn is_red(pixel: [u8; 4]) -> bool {
    pixel[0] > 150 && pixel[1] < 100 && pixel[2] < 100
}

/// 铺满画布的红色内容（蒙版/布尔的被作用对象）。
fn draw_red_canvas(workspace: &mut Workspace, doc: &str, width: u32, height: u32) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "content",
            "data": {
                "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": width, "h": height}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// 圆角 blob 子路径（中心 (60,60)、半径约 30，4 段 C 闭合）。
fn blob_subpaths() -> Value {
    json!([{
        "start": [30, 60],
        "closed": true,
        "segments": [
            {"op": "C", "c1": [30, 40], "c2": [45, 30], "to": [60, 30]},
            {"op": "C", "c1": [75, 30], "c2": [90, 40], "to": [90, 60]},
            {"op": "C", "c1": [90, 80], "c2": [75, 90], "to": [60, 90]},
            {"op": "C", "c1": [45, 90], "c2": [30, 80], "to": [30, 60]}
        ]
    }])
}

/// 外环 + 内环（evenodd 时内环是孔）。
fn ring_with_hole_subpaths() -> Value {
    let mut subs = blob_subpaths();
    subs.as_array_mut().unwrap().push(json!({
        "start": [50, 60],
        "closed": true,
        "segments": [
            {"op": "C", "c1": [50, 52], "c2": [54, 50], "to": [60, 50]},
            {"op": "C", "c1": [66, 50], "c2": [70, 52], "to": [70, 60]},
            {"op": "C", "c1": [70, 68], "c2": [66, 70], "to": [60, 70]},
            {"op": "C", "c1": [54, 70], "c2": [50, 68], "to": [50, 60]}
        ]
    }));
    subs
}

// ---------------------------------------------------------------------------
// 1. mask 源 = path
// ---------------------------------------------------------------------------

#[test]
fn mask_accepts_inline_path_and_hides_outside() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    // A：带内联 path 蒙版；B：同内容无蒙版（对照）。
    make_doc(&mut ws, "mA", 160, 120);
    draw_red_canvas(&mut ws, "mA", 160, 120);
    let created = call(
        &mut ws,
        "mA",
        "create_mask",
        &json!({"mask_id": "mk", "shape": {"kind": "path", "subpaths": blob_subpaths()}}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let attached = call(
        &mut ws,
        "mA",
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "mk"}),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
    make_doc(&mut ws, "mB", 160, 120);
    draw_red_canvas(&mut ws, "mB", 160, 120);

    let a = render(&mut ws, "mA", 160, 120);
    let b = render(&mut ws, "mB", 160, 120);
    // 环内：两边都红（蒙版放行）。
    assert!(
        is_red(px(&a, 160, 60, 60)),
        "环内应放行：{:?}",
        px(&a, 160, 60, 60)
    );
    assert_eq!(px(&a, 160, 60, 60), px(&b, 160, 60, 60));
    // 环外：B 红、A 被蒙版挡住（不红）。
    assert!(
        is_red(px(&b, 160, 10, 10)),
        "对照应全红：{:?}",
        px(&b, 160, 10, 10)
    );
    assert!(
        !is_red(px(&a, 160, 10, 10)),
        "环外应被挡住：{:?}",
        px(&a, 160, 10, 10)
    );
}

#[test]
fn mask_inline_path_evenodd_hole_stays_hidden() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "mH", 160, 120);
    draw_red_canvas(&mut ws, "mH", 160, 120);
    let created = call(
        &mut ws,
        "mH",
        "create_mask",
        &json!({"mask_id": "mk", "shape": {
            "kind": "path", "fill_rule": "evenodd", "subpaths": ring_with_hole_subpaths()
        }}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    call(
        &mut ws,
        "mH",
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "mk"}),
    );
    let img = render(&mut ws, "mH", 160, 120);
    // 环带上（外环内、内环外）：放行 ⇒ 红。
    assert!(
        is_red(px(&img, 160, 60, 38)),
        "环带应放行：{:?}",
        px(&img, 160, 60, 38)
    );
    // 孔中心：被 evenodd 挖掉 ⇒ 不红（证明蒙版走的是 path 填充 + fill_rule
    // 同一条路，而不是 bbox/外环近似）。
    assert!(
        !is_red(px(&img, 160, 60, 60)),
        "孔应被挖掉：{:?}",
        px(&img, 160, 60, 60)
    );
}

#[test]
fn mask_accepts_path_object_reference_and_follows_deletion() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "mR", 160, 120);
    draw_red_canvas(&mut ws, "mR", 160, 120);
    // 被引用的 path 对象：1px 红描边（采样点都离描边足够远）。
    let drawn = call(
        &mut ws,
        "mR",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "ring1",
            "data": {
                "geometry": {"kind": "path", "subpaths": blob_subpaths()},
                "stroke_width": 1,
                "stroke_color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let created = call(
        &mut ws,
        "mR",
        "create_mask",
        &json!({"mask_id": "mk", "shape": {"kind": "path", "object_id": "ring1"}}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    call(
        &mut ws,
        "mR",
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "mk"}),
    );
    let img = render(&mut ws, "mR", 160, 120);
    assert!(
        is_red(px(&img, 160, 60, 60)),
        "引用环内应放行：{:?}",
        px(&img, 160, 60, 60)
    );
    assert!(
        !is_red(px(&img, 160, 10, 10)),
        "引用环外应挡住：{:?}",
        px(&img, 160, 10, 10)
    );

    // 删掉被引用对象 ⇒ 引用失效、蒙版不施加（告警路径），内容恢复全显。
    let deleted = call(
        &mut ws,
        "mR",
        "delete_object",
        &json!({"object_id": "ring1"}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let img = render(&mut ws, "mR", 160, 120);
    assert!(
        is_red(px(&img, 160, 10, 10)),
        "引用失效后内容应恢复：{:?}",
        px(&img, 160, 10, 10)
    );
}

#[test]
fn mask_path_shape_validation_errors() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "mE", 160, 120);
    let cases: Vec<(Value, &str)> = vec![
        // 两种来源同给。
        (
            json!({"mask_id": "x1", "shape": {"kind": "path", "subpaths": blob_subpaths(), "object_id": "ring1"}}),
            "只能给一个",
        ),
        // 两种来源都不给。
        (
            json!({"mask_id": "x2", "shape": {"kind": "path"}}),
            "必须给",
        ),
        // 引用不存在。
        (
            json!({"mask_id": "x3", "shape": {"kind": "path", "object_id": "nope"}}),
            "不存在",
        ),
        // 内联 subpaths 非法（段 op 未知）⇒ 同一份校验的原话。
        (
            json!({"mask_id": "x4", "shape": {"kind": "path", "subpaths": [
                {"start": [0, 0], "segments": [{"op": "X", "to": [1, 1]}]}
            ]}}),
            "shape 非法",
        ),
    ];
    for (args, needle) in cases {
        let failed = call(&mut ws, "mE", "create_mask", &args);
        assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
        let text = failed.to_string();
        assert!(text.contains(needle), "错误应含「{needle}」，实测：{text}");
    }
    // 引用非 path 对象（rect）⇒ 明确报错。
    draw_red_canvas(&mut ws, "mE", 160, 120);
    let failed = call(
        &mut ws,
        "mE",
        "create_mask",
        &json!({"mask_id": "x5", "shape": {"kind": "path", "object_id": "content"}}),
    );
    assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
    assert!(
        failed.to_string().contains("不是 path 形状"),
        "实测：{failed}"
    );
}

// ---------------------------------------------------------------------------
// 2. path_guide 曲线尺吸附
// ---------------------------------------------------------------------------

fn guide_line_subpaths() -> Value {
    json!([{"start": [10, 40], "segments": [{"op": "L", "to": [190, 40]}]}])
}

fn stroke_args(extra: Value) -> Value {
    let mut args = json!({
        "layer_id": "L",
        "brush": "2B_pencil",
        "size": 12,
        "color": {"r": 20, "g": 20, "b": 20, "a": 255},
        "humanize": false,
        "points": [[20.0, 46.0, 1.0], [100.0, 46.0, 1.0], [180.0, 46.0, 1.0]]
    });
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    args
}

/// 墨迹（暗像素）的纵向质心。
fn ink_centroid_y(data: &[u8], width: u32, height: u32) -> f64 {
    let mut sum = 0.0f64;
    let mut count = 0.0f64;
    for y in 0..height {
        for x in 0..width {
            let p = px(data, width, x, y);
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) < 300 {
                sum += y as f64;
                count += 1.0;
            }
        }
    }
    assert!(count > 50.0, "应当有可观墨迹，实测 {count} px");
    sum / count
}

#[test]
fn path_guide_far_away_is_bit_identical() {
    let mut ws = assets_workspace("guide_far");
    make_doc(&mut ws, "gA", 200, 120);
    let plain = call(&mut ws, "gA", "brush_stroke", &stroke_args(json!({})));
    assert_eq!(plain["ok"], json!(true), "{plain}");

    make_doc(&mut ws, "gB", 200, 120);
    // guide 在画布外远处（y = −500），R=10 内够不着任何落点 ⇒ 逐位不变。
    let guided = call(
        &mut ws,
        "gB",
        "brush_stroke",
        &stroke_args(json!({"path_guide": {
            "subpaths": [{"start": [10, -500], "segments": [{"op": "L", "to": [190, -500]}]}],
            "snap_radius": 10
        }})),
    );
    assert_eq!(guided["ok"], json!(true), "{guided}");
    let a = render(&mut ws, "gA", 200, 120);
    let b = render(&mut ws, "gB", 200, 120);
    assert_eq!(a, b, "远处 guide 不应改动任何像素（吸附关回归锁）");
}

#[test]
fn path_guide_pulls_stroke_toward_the_curve() {
    let mut ws = assets_workspace("guide_pull");
    make_doc(&mut ws, "gC", 200, 120);
    let plain = call(&mut ws, "gC", "brush_stroke", &stroke_args(json!({})));
    assert_eq!(plain["ok"], json!(true), "{plain}");
    let plain_y = ink_centroid_y(&render(&mut ws, "gC", 200, 120), 200, 120);

    make_doc(&mut ws, "gD", 200, 120);
    // guide 是 y=40 直线；落点恒在 y=46（d=6，R=10 ⇒ w=(1−0.6)²=0.16，
    // 每点上移 0.96px）⇒ 质心应明显向 40 靠、但不到 40。
    let guided = call(
        &mut ws,
        "gD",
        "brush_stroke",
        &stroke_args(json!({"path_guide": {
            "subpaths": guide_line_subpaths(),
            "snap_radius": 10
        }})),
    );
    assert_eq!(guided["ok"], json!(true), "{guided}");
    let guided_y = ink_centroid_y(&render(&mut ws, "gD", 200, 120), 200, 120);
    assert!(
        guided_y < plain_y - 0.3,
        "质心应被拉向 guide：plain={plain_y:.2} guided={guided_y:.2}"
    );
    assert!(
        guided_y > 40.0,
        "质心不应越过 guide 线：guided={guided_y:.2}"
    );
}

#[test]
fn path_guide_object_reference_and_errors() {
    let mut ws = assets_workspace("guide_err");
    make_doc(&mut ws, "gE", 200, 120);
    // guide 走对象引用同样生效。
    let drawn = call(
        &mut ws,
        "gE",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "guide1",
            "data": {
                "geometry": {"kind": "path", "subpaths": guide_line_subpaths()},
                "stroke_width": 1,
                "stroke_color": {"r": 20, "g": 20, "b": 20, "a": 255}
            }
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let ok = call(
        &mut ws,
        "gE",
        "brush_stroke",
        &stroke_args(json!({"path_guide": {"object_id": "guide1", "snap_radius": 10}})),
    );
    assert_eq!(ok["ok"], json!(true), "{ok}");

    let cases: Vec<(Value, &str)> = vec![
        // 两种来源同给。
        (
            json!({"path_guide": {"object_id": "guide1", "subpaths": guide_line_subpaths(), "snap_radius": 10}}),
            "只能给一个",
        ),
        // 缺 snap_radius。
        (
            json!({"path_guide": {"object_id": "guide1"}}),
            "snap_radius 必填",
        ),
        // 半径非法。
        (
            json!({"path_guide": {"object_id": "guide1", "snap_radius": 0}}),
            "snap_radius 必须 > 0",
        ),
        // 引用不存在。
        (
            json!({"path_guide": {"object_id": "nope", "snap_radius": 10}}),
            "不存在",
        ),
    ];
    for (extra, needle) in cases {
        let failed = call(&mut ws, "gE", "brush_stroke", &stroke_args(extra));
        assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
        let text = failed.to_string();
        assert!(text.contains(needle), "错误应含「{needle}」，实测：{text}");
    }
}

#[test]
fn smart_edge_soften_path_guide_far_is_bit_identical() {
    // smart_edge_soften 落笔要用真笔刷（缺省 calligraphy）⇒ 落盘资产工作区。
    let mut ws = assets_workspace("smart_guide");
    let setup = |ws: &mut Workspace, doc: &str| {
        make_doc(ws, doc, 160, 120);
        let made = call(
            ws,
            doc,
            "draw_shape",
            &json!({
                "layer_id": "L",
                "object_id": "block",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 30, "y": 30, "w": 100, "h": 60}},
                    "color": {"r": 200, "g": 30, "b": 30, "a": 255}
                }
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    };
    setup(&mut ws, "sA");
    let plain = call(
        &mut ws,
        "sA",
        "smart_edge_soften",
        &json!({"layer_id": "L", "edge_path": [[30, 30], [130, 30], [130, 90], [30, 90]]}),
    );
    assert_eq!(plain["ok"], json!(true), "{plain}");

    setup(&mut ws, "sB");
    let guided = call(
        &mut ws,
        "sB",
        "smart_edge_soften",
        &json!({"layer_id": "L",
        "edge_path": [[30, 30], [130, 30], [130, 90], [30, 90]],
        "path_guide": {
            "subpaths": [{"start": [0, -400], "segments": [{"op": "L", "to": [160, -400]}]}],
            "snap_radius": 8
        }}),
    );
    assert_eq!(guided["ok"], json!(true), "{guided}");
    assert_eq!(
        render(&mut ws, "sA", 160, 120),
        render(&mut ws, "sB", 160, 120),
        "远处 guide 不应改动 smart_edge_soften 的任何像素"
    );
    // path_guide 坏参数在 smart 侧同样明确报错（共用解析）。
    let failed = call(
        &mut ws,
        "sB",
        "smart_edge_soften",
        &json!({"layer_id": "L",
                "edge_path": [[30, 30], [130, 30]],
                "path_guide": {"subpaths": guide_line_subpaths()}}),
    );
    assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
    assert!(
        failed.to_string().contains("snap_radius 必填"),
        "实测：{failed}"
    );
}

// ---------------------------------------------------------------------------
// 3. SVG d 语法糖
// ---------------------------------------------------------------------------

const BLOB_D: &str =
    "M 30 60 C 30 40 45 30 60 30 C 75 30 90 40 90 60 C 90 80 75 90 60 90 C 45 90 30 80 30 60 Z";

#[test]
fn draw_shape_d_matches_subpaths_and_is_stripped_on_store() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "dA", 160, 120);
    let via_d = call(
        &mut ws,
        "dA",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "blob",
            "data": {
                "geometry": {"kind": "path", "d": BLOB_D},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(via_d["ok"], json!(true), "{via_d}");

    make_doc(&mut ws, "dB", 160, 120);
    let via_subs = call(
        &mut ws,
        "dB",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "blob",
            "data": {
                "geometry": {"kind": "path", "subpaths": blob_subpaths()},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(via_subs["ok"], json!(true), "{via_subs}");
    assert_eq!(
        render(&mut ws, "dA", 160, 120),
        render(&mut ws, "dB", 160, 120),
        "d 与等价 subpaths 的渲染应逐字节一致"
    );
    // 落库形态：d 已被摘掉、subpaths 与结构化输入相同（只剩一种形态）。
    let stored = call(&mut ws, "dA", "get_object", &json!({"object_id": "blob"}));
    assert_eq!(stored["ok"], json!(true), "{stored}");
    let text = stored.to_string();
    assert!(!text.contains("\"d\":"), "落库数据不应再带 d：{text}");
    assert!(
        text.contains("\"subpaths\""),
        "落库数据应带 subpaths：{text}"
    );
}

#[test]
fn draw_shape_d_errors() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "dE", 160, 120);
    // d 与 subpaths 同给 ⇒ 二选一报错。
    let both = call(
        &mut ws,
        "dE",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "data": {
                "geometry": {"kind": "path", "d": BLOB_D, "subpaths": blob_subpaths()},
                "color": {"r": 1, "g": 2, "b": 3, "a": 255}
            }
        }),
    );
    assert_ne!(both["ok"], json!(true), "应当失败：{both}");
    assert!(both.to_string().contains("只能给一个"), "实测：{both}");
    // 语法错误（不支持的命令 A）⇒ 带字符位置。
    let bad = call(
        &mut ws,
        "dE",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "data": {
                "geometry": {"kind": "path", "d": "M 0 0 A 5 5"},
                "color": {"r": 1, "g": 2, "b": 3, "a": 255}
            }
        }),
    );
    assert_ne!(bad["ok"], json!(true), "应当失败：{bad}");
    assert!(bad.to_string().contains("@字符"), "实测：{bad}");
}

#[test]
fn stroke_path_d_matches_subpaths() {
    let mut ws = assets_workspace("d_stroke");
    make_doc(&mut ws, "dS1", 200, 120);
    let via_d = call(
        &mut ws,
        "dS1",
        "stroke_path",
        &json!({
            "layer_id": "L",
            "brush": "2B_pencil",
            "size": 12,
            "color": {"r": 30, "g": 30, "b": 30, "a": 255},
            "d": "M 20 60 C 60 20 140 20 180 60"
        }),
    );
    assert_eq!(via_d["ok"], json!(true), "{via_d}");

    make_doc(&mut ws, "dS2", 200, 120);
    let via_subs = call(
        &mut ws,
        "dS2",
        "stroke_path",
        &json!({
            "layer_id": "L",
            "brush": "2B_pencil",
            "size": 12,
            "color": {"r": 30, "g": 30, "b": 30, "a": 255},
            "subpaths": [{"start": [20, 60], "segments": [
                {"op": "C", "c1": [60, 20], "c2": [140, 20], "to": [180, 60]}
            ]}]
        }),
    );
    assert_eq!(via_subs["ok"], json!(true), "{via_subs}");
    assert_eq!(via_d["points_total"], via_subs["points_total"]);
    assert_eq!(
        render(&mut ws, "dS1", 200, 120),
        render(&mut ws, "dS2", 200, 120),
        "stroke_path 用 d 与用 subpaths 应逐字节一致"
    );
    // d 语法错误带字符位置；三来源同给报错。
    let bad = call(
        &mut ws,
        "dS1",
        "stroke_path",
        &json!({"layer_id": "L", "brush": "2B_pencil", "d": "M 0 0 S 1 1"}),
    );
    assert_ne!(bad["ok"], json!(true), "应当失败：{bad}");
    assert!(bad.to_string().contains("@字符"), "实测：{bad}");
    let both = call(
        &mut ws,
        "dS1",
        "stroke_path",
        &json!({"layer_id": "L", "brush": "2B_pencil", "d": "M 0 0 L 1 1",
                "subpaths": [{"start": [0, 0], "segments": [{"op": "L", "to": [1, 1]}]}]}),
    );
    assert_ne!(both["ok"], json!(true), "应当失败：{both}");
    assert!(both.to_string().contains("只能给一个"), "实测：{both}");
}

// ---------------------------------------------------------------------------
// 4. combine_shapes 简易布尔
// ---------------------------------------------------------------------------

fn draw_rect(workspace: &mut Workspace, doc: &str, id: &str, x: f64, y: f64, w: f64, h: f64) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": id,
            "data": {
                "geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": w, "h": h}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn count_red(data: &[u8]) -> usize {
    data.chunks_exact(4)
        .filter(|p| p[0] > 150 && p[1] < 100 && p[2] < 100)
        .count()
}

/// 隐藏一个对象（update_object 的 visible 补丁），好看结果对象自己的面积。
fn hide_object(workspace: &mut Workspace, doc: &str, id: &str) {
    let updated = call(
        workspace,
        doc,
        "update_object",
        &json!({"object_id": id, "patch": {"visible": false}}),
    );
    assert_eq!(updated["ok"], json!(true), "{updated}");
}

#[test]
fn combine_union_area_matches_analytic_union() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cU", 120, 120);
    draw_rect(&mut ws, "cU", "r1", 10.0, 10.0, 40.0, 40.0);
    draw_rect(&mut ws, "cU", "r2", 30.0, 30.0, 40.0, 40.0);
    let combined = call(
        &mut ws,
        "cU",
        "combine_shapes",
        &json!({"object_ids": ["r1", "r2"], "result_id": "u1", "simplify": 0}),
    );
    assert_eq!(combined["ok"], json!(true), "{combined}");
    assert_eq!(combined["kind"], json!("polygon"), "{combined}");
    assert_eq!(combined["loops"], json!(1), "{combined}");
    assert_eq!(combined["inputs_kept"], json!(true), "{combined}");
    // 输入仍存活（不 tombstone，与 path_boolean 的关键区别）。
    let alive = call(&mut ws, "cU", "get_object", &json!({"object_id": "r1"}));
    assert_eq!(alive["ok"], json!(true), "{alive}");
    // 只看结果对象：面积应 ≈ 解析并集 1600 + 1600 − 400 = 2800（±4%，
    // 光栅等值线在边界有亚像素误差；若实现误成求和/求交会远超这个带）。
    hide_object(&mut ws, "cU", "r1");
    hide_object(&mut ws, "cU", "r2");
    let area = count_red(&render(&mut ws, "cU", 120, 120)) as f64;
    assert!(
        (area - 2800.0).abs() <= 2800.0 * 0.04,
        "并集面积应 ≈2800，实测 {area}"
    );
}

#[test]
fn combine_subtract_rect_minus_ellipse_leaves_a_hole() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cS", 120, 120);
    draw_rect(&mut ws, "cS", "base", 20.0, 20.0, 80.0, 80.0);
    let ellipse = call(
        &mut ws,
        "cS",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "hole",
            "data": {
                "geometry": {"kind": "ellipse", "bbox": {"x": 45, "y": 45, "w": 30, "h": 30}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(ellipse["ok"], json!(true), "{ellipse}");
    let combined = call(
        &mut ws,
        "cS",
        "combine_shapes",
        &json!({"object_ids": ["base", "hole"], "mode": "subtract", "result_id": "donut"}),
    );
    assert_eq!(combined["ok"], json!(true), "{combined}");
    // 带孔 ⇒ 多环 ⇒ 落成 path（evenodd）。
    assert_eq!(combined["kind"], json!("path"), "{combined}");
    assert_eq!(combined["loops"], json!(2), "{combined}");
    hide_object(&mut ws, "cS", "base");
    hide_object(&mut ws, "cS", "hole");
    let img = render(&mut ws, "cS", 120, 120);
    assert!(
        is_red(px(&img, 120, 28, 28)),
        "角上应是红：{:?}",
        px(&img, 120, 28, 28)
    );
    assert!(
        !is_red(px(&img, 120, 60, 60)),
        "孔中心应透空：{:?}",
        px(&img, 120, 60, 60)
    );
}

#[test]
fn combine_intersect_of_disjoint_shapes_is_an_empty_set_error() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cI", 120, 120);
    draw_rect(&mut ws, "cI", "a", 5.0, 5.0, 20.0, 20.0);
    draw_rect(&mut ws, "cI", "b", 80.0, 80.0, 20.0, 20.0);
    let failed = call(
        &mut ws,
        "cI",
        "combine_shapes",
        &json!({"object_ids": ["a", "b"], "mode": "intersect"}),
    );
    assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
    assert!(failed.to_string().contains("空集"), "实测：{failed}");
}

#[test]
fn combine_accepts_bezier_path_inputs() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cP", 160, 120);
    let blob = call(
        &mut ws,
        "cP",
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": "blob",
            "data": {
                "geometry": {"kind": "path", "subpaths": blob_subpaths()},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(blob["ok"], json!(true), "{blob}");
    draw_rect(&mut ws, "cP", "cover", 50.0, 40.0, 60.0, 40.0);
    let combined = call(
        &mut ws,
        "cP",
        "combine_shapes",
        &json!({"object_ids": ["blob", "cover"], "result_id": "merged"}),
    );
    assert_eq!(combined["ok"], json!(true), "{combined}");
    hide_object(&mut ws, "cP", "blob");
    hide_object(&mut ws, "cP", "cover");
    let area = count_red(&render(&mut ws, "cP", 160, 120)) as f64;
    // 并集应严格大于两个输入中较大的那个（rect 2400），且小于两者之和的上界。
    assert!(area > 2500.0, "Bézier ∪ rect 面积应 > 2500，实测 {area}");
}

#[test]
fn combine_shapes_argument_errors() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cE", 120, 120);
    draw_rect(&mut ws, "cE", "only", 10.0, 10.0, 30.0, 30.0);
    let cases: Vec<(Value, &str)> = vec![
        // 只给一个 id。
        (json!({"object_ids": ["only"]}), "至少给 2 个"),
        // 未知 mode。
        (
            json!({"object_ids": ["only", "only"], "mode": "xor"}),
            "未知的 mode",
        ),
        // id 不存在。
        (json!({"object_ids": ["only", "ghost"]}), "不存在"),
        // simplify 非法。
        (
            json!({"object_ids": ["only", "only"], "simplify": -1}),
            "simplify 必须是",
        ),
    ];
    for (args, needle) in cases {
        let failed = call(&mut ws, "cE", "combine_shapes", &args);
        assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
        let text = failed.to_string();
        assert!(text.contains(needle), "错误应含「{needle}」，实测：{text}");
    }
}

// ---------------------------------------------------------------------------
// 4b. combine_shapes 的 path 输入（F1 回归：外延口径错位）
//
// 二期自测的 path 输入恰好被另一个 rect 输入的外延罩住、断言又只要求
// 「面积 > 较大输入」，path 臂的真实覆盖从未被验过。根因：输入外延累积时
// path 臂直接透传 `path_control_bbox` 的 (x, y, w, h)，而合成口径是角点
// (x0, y0, x1, y1) ⇒ min 坐标 > 0 时场窗口被压缩：path 独占的外延被切、
// 两个 path 相遇时场退化为空。下面四条让 path 自己定义外延，面积对解析值。
// ---------------------------------------------------------------------------

/// 圆子路径（4 段三次 Bézier，kappa 常数）：圆心 (cx, cy)、半径 r。
fn circle_subpaths(cx: f64, cy: f64, r: f64) -> Value {
    const KAPPA: f64 = 0.552_284_749_830_79;
    let d = r * KAPPA;
    json!([{
        "start": [cx + r, cy], "closed": true,
        "segments": [
            {"op": "C", "c1": [cx + r, cy + d], "c2": [cx + d, cy + r], "to": [cx, cy + r]},
            {"op": "C", "c1": [cx - d, cy + r], "c2": [cx - r, cy + d], "to": [cx - r, cy]},
            {"op": "C", "c1": [cx - r, cy - d], "c2": [cx - d, cy - r], "to": [cx, cy - r]},
            {"op": "C", "c1": [cx + d, cy - r], "c2": [cx + r, cy - d], "to": [cx + r, cy]}
        ]
    }])
}

/// L 段矩形环子路径：(x0, y0) → (x1, y1) 的轴对齐矩形（隐式闭合回起点）。
fn l_rect_subpaths(x0: f64, y0: f64, x1: f64, y1: f64) -> Value {
    json!([{
        "start": [x0, y0], "closed": true,
        "segments": [
            {"op": "L", "to": [x1, y0]},
            {"op": "L", "to": [x1, y1]},
            {"op": "L", "to": [x0, y1]}
        ]
    }])
}

fn draw_path(workspace: &mut Workspace, doc: &str, id: &str, subpaths: Value) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": id,
            "data": {
                "geometry": {"kind": "path", "subpaths": subpaths},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// 隔离渲染结果对象（隐藏全部输入）后的红色像素数。
fn isolated_area(
    workspace: &mut Workspace,
    doc: &str,
    width: u32,
    height: u32,
    inputs: &[&str],
) -> f64 {
    for id in inputs {
        hide_object(workspace, doc, id);
    }
    count_red(&render(workspace, doc, width, height)) as f64
}

#[test]
fn combine_union_of_two_bezier_circles_matches_analytic() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cPC", 320, 220);
    draw_path(&mut ws, "cPC", "c1", circle_subpaths(120.0, 110.0, 50.0));
    draw_path(&mut ws, "cPC", "c2", circle_subpaths(190.0, 110.0, 50.0));
    let combined = call(
        &mut ws,
        "cPC",
        "combine_shapes",
        &json!({"object_ids": ["c1", "c2"], "result_id": "u", "simplify": 0}),
    );
    assert_eq!(
        combined["ok"],
        json!(true),
        "两圆 path 并集不应失败：{combined}"
    );
    assert_eq!(combined["loops"], json!(1), "{combined}");
    let area = isolated_area(&mut ws, "cPC", 320, 220, &["c1", "c2"]);
    // 解析并集：2·π·50² − 交叠（d=70：2r²·acos(d/2r) − (d/2)·√(4r²−d²) ≈ 1477.5）
    // ≈ 14230。修复前两 path 的外延互相压缩 ⇒ 场退化为空、直接报「空集」。
    assert!(
        (area - 14230.0).abs() <= 14230.0 * 0.05,
        "两圆 path 并集面积应 ≈14230，实测 {area}"
    );
}

#[test]
fn combine_intersect_of_two_bezier_circles_matches_analytic() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cPI", 320, 220);
    draw_path(&mut ws, "cPI", "c1", circle_subpaths(120.0, 110.0, 50.0));
    draw_path(&mut ws, "cPI", "c2", circle_subpaths(190.0, 110.0, 50.0));
    let combined = call(
        &mut ws,
        "cPI",
        "combine_shapes",
        &json!({"object_ids": ["c1", "c2"], "mode": "intersect", "result_id": "i", "simplify": 0}),
    );
    assert_eq!(
        combined["ok"],
        json!(true),
        "两圆 path 交集不应失败：{combined}"
    );
    let area = isolated_area(&mut ws, "cPI", 320, 220, &["c1", "c2"]);
    // 解析交叠（透镜）≈ 1477.5（推导见并集用例）。修复前场退化 ⇒ 报空集。
    assert!(
        (area - 1477.5).abs() <= 1477.5 * 0.08,
        "两圆 path 交集面积应 ≈1477.5，实测 {area}"
    );
}

#[test]
fn combine_union_path_and_rect_where_path_defines_extent() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cPE", 400, 240);
    // L 多边形 path（200×100 矩形环）向右/向下独占外延；rect 只占左上一角。
    draw_path(
        &mut ws,
        "cPE",
        "p",
        l_rect_subpaths(150.0, 50.0, 350.0, 150.0),
    );
    draw_rect(&mut ws, "cPE", "r", 100.0, 75.0, 100.0, 50.0);
    let combined = call(
        &mut ws,
        "cPE",
        "combine_shapes",
        &json!({"object_ids": ["p", "r"], "result_id": "u", "simplify": 0}),
    );
    assert_eq!(combined["ok"], json!(true), "{combined}");
    let area = isolated_area(&mut ws, "cPE", 400, 240, &["p", "r"]);
    // 解析并集：20000 + 5000 − 重叠 (50×50) = 22500。
    // 修复前 path 外延被当成 (150,50,200,100) 角点 ⇒ 场右界止于 200，
    // path 在 x>200 的一半被整段切掉，实测约 6250。
    assert!(
        (area - 22500.0).abs() <= 22500.0 * 0.03,
        "path∪rect 并集面积应 ≈22500，实测 {area}"
    );
}

#[test]
fn combine_subtract_rect_from_path_base() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "cPS", 480, 240);
    // path 作被减数（240×120 矩形环），rect 刀完全落在 path 右半内部。
    draw_path(
        &mut ws,
        "cPS",
        "base",
        l_rect_subpaths(200.0, 50.0, 440.0, 170.0),
    );
    draw_rect(&mut ws, "cPS", "knife", 380.0, 80.0, 40.0, 60.0);
    let combined = call(
        &mut ws,
        "cPS",
        "combine_shapes",
        &json!({"object_ids": ["base", "knife"], "mode": "subtract", "result_id": "cut", "simplify": 0}),
    );
    assert_eq!(combined["ok"], json!(true), "{combined}");
    assert_eq!(
        combined["kind"],
        json!("path"),
        "带孔应落成 path：{combined}"
    );
    assert_eq!(combined["loops"], json!(2), "{combined}");
    let area = isolated_area(&mut ws, "cPS", 480, 240, &["base", "knife"]);
    // 解析差：28800 − 2400 = 26400。修复前场被压到 (200,50)-(420,140)，
    // path 的右条与下条被切，实测约 17400。
    assert!(
        (area - 26400.0).abs() <= 26400.0 * 0.03,
        "path−rect 差集面积应 ≈26400，实测 {area}"
    );
}
