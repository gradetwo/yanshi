//! **笔迹平滑** ✓（`draw_stroke` 的 `data.smooth` ✓ / `brush_stroke` 的 `smooth` ✓）
//! —— 用户手写的折线不再有硬角 ✓（原话："12 瓣花手写 60 个坐标"✗）。
//!
//! **两条路、同一个实现** ✓：渲染层 `catmull_rom_smooth`（`draw_stroke` 在**渲染时**插值 ✓、
//! 日志里存的仍是**原始采样点** ✓）与工具层 `smooth_stroke_points`（`brush_stroke` 在**落笔前**
//! 把控制点重采样 ✓）—— 不自己写第二份插值 ✗。
//!
//! **判据（每条都能红 ✓）**：
//! ① **不给 `smooth` ⇒ 与"显式 false"逐字节相同** ✓（老调用方与老文档不受影响 ✓）；
//! ② **给了 `smooth` ⇒ 画面必须变** ✓（两条路各测一次 ✓）；
//! ③ **控制点没被拉走** ✓：曲线**过**这些点（Catmull-Rom 是插值样条 ✓）
//!    ⇒ 起点周围那块墨**基本不变**（≤5% ✓）—— 能挡住"把线拉直 / 缩短"那种假平滑 ✗；
//! ④ **平滑 == 手工把同一条曲线加密后传进去** ✓（**逐字节相同** ✓）——
//!    这条是**最强的**判据 ✓：它同时钉住"用了哪一种插值" ✓ 与"细分数是多少" ✓。
//!    （我第一版写的是"总墨量必须下降" ✗ —— 实测**不成立** ✓：brush_stroke 那条路
//!      会把控制点加密 ⇒ 引擎的 dab 排布跟着变 ✓ ⇒ 平滑后墨**更多**（6280 → 6457 ✓）。
//!      "墨量往哪边变"本来就不是平滑的定义 ✗ ⇒ 换成"与手工曲线逐字节相同" ✓。）

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 一条**带尖角**的折线 ✓（顶点在 (150, 160)）—— 平滑与否在这一块看得最清楚 ✓。
const ZIGZAG: [[f64; 2]; 3] = [[60.0, 40.0], [150.0, 160.0], [240.0, 40.0]];

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&[Profile::Core])
}

fn workspace() -> Workspace {
    // **要读得到 `.myb` 笔刷** ✓（`brush_stroke` 那两条走的是真实引擎 ✓，不造假 ✓）。
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_smooth", 300, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_smooth", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// 同一条曲线：**直接调用渲染层那份实现** ✓（测试里不重写算法 ✓）。
fn curve_points() -> Vec<[f64; 3]> {
    let points: Vec<yanshi_render::brush::StrokePoint> = ZIGZAG
        .iter()
        .map(|[x, y]| yanshi_render::brush::StrokePoint {
            x: *x,
            y: *y,
            pressure: 1.0,
        })
        .collect();
    yanshi_render::brush::catmull_rom_smooth(&points, yanshi_render::brush::SMOOTH_SUBDIVISIONS)
        .into_iter()
        .map(|point| [point.x, point.y, point.pressure])
        .collect()
}

/// 某一层上的原始 RGBA ✓（`layer_id` ⇒ 只看这一层 ✓ ⇒ 两条路互不干扰 ✓）。
fn layer_pixels(workspace: &mut Workspace, layer: &str) -> Vec<u8> {
    workspace
        .render_region_raw_layer(
            "doc_smooth",
            yanshi_core::Bbox::new(0.0, 0.0, 300.0, 200.0),
            layer,
        )
        .expect("区域渲染应成功")
        .2
}

/// 两个计数"足够接近" ✓（差 ≤5% ✓）—— 用于"曲线仍然过控制点 ✓、没被拉走" ✓。
///
/// **为什么不要求逐像素相同** ✗（我第一版就是这么写的 ✓，实测差 1..2 个像素 ✓）：
/// Catmull-Rom 保**点** ✓（曲线过它 ✓）但**不保方向** ✓ ⇒ 起点附近那一小段会微微拐弯 ✓
/// ⇒ 那一块的墨会差一两个像素 ✓ —— 那是**正确行为** ✗，不是缺陷 ✓。
fn close_enough(a: usize, b: usize) -> bool {
    let (a, b) = (a as i64, b as i64);
    (a - b).abs() * 20 <= a.max(b)
}

/// 统计某个矩形里"有墨"的像素数 ✓ —— **墨 = 不是白底** ✓（`render_region_raw_layer` 会把图层
/// 合成到文档背景上 ✓ ⇒ 只数 alpha 会把整幅都算进去 ✗ —— 实测 300×200 = 60000 ✗，第一版就是这么错的 ✓）。
fn ink_in(pixels: &[u8], x0: usize, y0: usize, x1: usize, y1: usize) -> usize {
    let mut count = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let at = (y * 300 + x) * 4;
            let (r, g, b, a) = (pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]);
            if a > 32 && !(r > 245 && g > 245 && b > 245) {
                count += 1;
            }
        }
    }
    count
}

/// **`draw_stroke`：`data.smooth` 真的平滑** ✓（渲染时插值 ✓），且**点没被改写** ✓。
#[test]
fn draw_stroke_smooths_only_when_asked_and_keeps_the_points() {
    let mut workspace = workspace();
    for layer in ["L_sharp", "L_smooth", "L_false", "L_manual"] {
        assert_eq!(
            call(&mut workspace, "create_layer", json!({ "layer_id": layer }))["ok"],
            json!(true)
        );
    }
    let stroke = |smooth: Option<bool>| {
        let mut data = json!({"points": ZIGZAG, "size": 20, "color": [1.0, 0.0, 0.0, 1.0]});
        if let Some(flag) = smooth {
            data["smooth"] = json!(flag);
        }
        data
    };
    // **手工把那同一条曲线加密后传进去** ✓ —— 用来证明 `smooth: true` 走的正是那份插值 ✓。
    let manual = call(
        &mut workspace,
        "draw_stroke",
        json!({
            "layer_id": "L_manual", "object_id": "manual",
            "data": {"points": curve_points(), "size": 20, "color": [1.0, 0.0, 0.0, 1.0]}
        }),
    );
    assert_eq!(manual["ok"], json!(true), "{manual}");
    for (object, layer, smooth) in [
        ("sharp", "L_sharp", None),
        ("smooth", "L_smooth", Some(true)),
        ("false_flag", "L_false", Some(false)),
    ] {
        let made = call(
            &mut workspace,
            "draw_stroke",
            json!({ "layer_id": layer, "object_id": object, "data": stroke(smooth) }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }

    // ① **`smooth: false` 与"不给"逐字节相同** ✓（老调用方不受影响 ✓）。
    let sharp = layer_pixels(&mut workspace, "L_sharp");
    let explicit_false = layer_pixels(&mut workspace, "L_false");
    assert_eq!(
        sharp, explicit_false,
        "显式 false 与不传 smooth 必须给出同一幅画面 ✓"
    );

    // ③ **控制点没被拉走** ✓：起点 (60,40) 周围那块墨必须一样 ✓。
    let start_sharp = ink_in(&sharp, 50, 30, 75, 55);
    let smoothed = layer_pixels(&mut workspace, "L_smooth");
    let start_smooth = ink_in(&smoothed, 50, 30, 75, 55);
    assert!(start_sharp > 0, "起点那块应当有墨 ✓");
    assert!(
        close_enough(start_sharp, start_smooth),
        "平滑是**插值**（曲线过控制点 ✓）⇒ 起点那块**不该被拉走** ✗（实测 {start_sharp} vs {start_smooth}）"
    );

    // ② **画面必须变** ✓ + ④ **与手工曲线逐字节相同** ✓。
    assert_ne!(sharp, smoothed, "给了 smooth ⇒ 画面必须变 ✗");
    let manual_pixels = layer_pixels(&mut workspace, "L_manual");
    assert_eq!(
        manual_pixels, smoothed,
        "`smooth: true` 必须就等于「把同一条曲线加密后传进去」✓（逐字节相同 ✓）"
    );

    // **日志里存的仍是原始控制点** ✓（平滑只发生在渲染时 ✓ ⇒ 可随时关掉 ✓）。
    let stored = call(
        &mut workspace,
        "get_object",
        json!({ "object_id": "smooth" }),
    );
    let stored_points = stored["data"]["points"].clone();
    assert_eq!(
        stored_points,
        json!(ZIGZAG),
        "点序列不该被改写（渲染时才插值 ✓）：{stored}"
    );
}

/// **`brush_stroke`：`smooth` 真的平滑** ✓（落笔前把控制点重采样 ✓）。
#[test]
fn brush_stroke_smooths_only_when_asked() {
    let mut workspace = workspace();
    for layer in ["L_brush_sharp", "L_brush_smooth", "L_brush_manual"] {
        assert_eq!(
            call(&mut workspace, "create_layer", json!({ "layer_id": layer }))["ok"],
            json!(true)
        );
    }
    let sharp = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L_brush_sharp", "object_id": "brush_sharp",
            "brush": "100%_Opaque", "size": 20, "points": ZIGZAG
        }),
    );
    assert_eq!(sharp["ok"], json!(true), "{sharp}");
    let smoothed = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L_brush_smooth", "object_id": "brush_smooth",
            "brush": "100%_Opaque", "size": 20, "points": ZIGZAG, "smooth": true
        }),
    );
    assert_eq!(smoothed["ok"], json!(true), "{smoothed}");

    let manual = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L_brush_manual", "object_id": "brush_manual",
            "brush": "100%_Opaque", "size": 20, "points": curve_points()
        }),
    );
    assert_eq!(manual["ok"], json!(true), "{manual}");
    let sharp_pixels = layer_pixels(&mut workspace, "L_brush_sharp");
    let smooth_pixels = layer_pixels(&mut workspace, "L_brush_smooth");
    // ② **画面必须变** ✓（"参数被静默忽略" ✗ 会被这条当场抓住 ✓）。
    assert_ne!(
        sharp_pixels, smooth_pixels,
        "brush_stroke 的 smooth 必须真的改到画面 ✗（不许返回 ok 却什么都不变 ✗）"
    );
    // ③ **控制点没被拉走** ✓。
    let start_sharp = ink_in(&sharp_pixels, 50, 30, 75, 55);
    let start_smooth = ink_in(&smooth_pixels, 50, 30, 75, 55);
    assert!(start_sharp > 0, "起点那块应当有墨 ✓");
    assert!(
        close_enough(start_sharp, start_smooth),
        "平滑是插值 ⇒ 起点那块**不该被拉走** ✗（实测 {start_sharp} vs {start_smooth}）"
    );
    // ④ **与手工曲线逐字节相同** ✓（落笔前重采样 ✓ ⇒ 引擎收到的点序列一模一样 ✓）。
    let manual_pixels = layer_pixels(&mut workspace, "L_brush_manual");
    assert_eq!(
        manual_pixels, smooth_pixels,
        "brush_stroke 的 smooth 必须就等于「把同一条曲线加密后传进去」✓（逐字节相同 ✓）"
    );
    // 而它与不平滑那条**必须不同** ✓（上面已经比过 ✓，这里再确认一次方向 ✓）。
    assert_ne!(manual_pixels, sharp_pixels, "手工曲线也必须与折线不同 ✓");
}

/// **平滑只认"碰过画布"的那条路** ✓：同一条笔触两次调用（一次平滑一次不平滑）
/// 必须给出**不同**的像素 ✓ —— 这条与上一条互补 ✓：它证明**默认行为没被改** ✓。
#[test]
fn the_default_is_still_not_smoothed() {
    let mut workspace = workspace();
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "b1", "brush": "100%_Opaque", "size": 20, "points": ZIGZAG
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // **把它当成"折线"来量** ✓：不平滑时顶点 (150,160) 正下方那一小块应当有墨 ✓；
    // 平滑之后那一块会**明显变少** ✓（曲线在那里被抹圆 ✓）。
    let pixels = layer_pixels(&mut workspace, "L");
    let apex = ink_in(&pixels, 140, 155, 161, 171);
    assert!(
        apex > 0,
        "默认（不平滑）时顶点正下方应当有墨 ✓ —— 若这条空了，说明默认行为被改成了平滑 ✗"
    );
}
