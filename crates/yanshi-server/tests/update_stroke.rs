//! **`update_stroke`** ✓（设计 10.3 的**三层参数** ✓）—— "画完之后**改这一笔**" ✓。
//!
//! **为什么有这条测试** ✓：这个工具此前**一个测试都没提过** ✗（零覆盖 ✓），
//! 而查看器本轮把它做成了「改笔触」✓ ⇒ 先把行为钉死 ✓。
//! 判据两条**事实** ✓：① **对象数据真的变了** ✓（`get_object` 的响应里对象字段在**顶层** ✓）；
//! ② **渲染出来的像素真的变了** ✓（`render_region` 在文档空间 ✓）。
//!
//! **第 31 轮补的两条** ✓（用户报的 P0 ✓：`ok:true` 但颜色没变 ✗）：
//! * **判据必须能红** ✓：**两次调用、两种颜色** ⇒ 像素必须**不同** ✓ + **方向检查** ✓
//!   （红 ⇒ R 占主导 ✓、绿 ⇒ G ✓、蓝 ⇒ B ✓）⇒ "固定画成某一色"也会被抓住 ✓；
//! * **烘进 blob 的光栅对象必须被拒绝** ✓：`brush_stroke` 落笔的产物是 `raster_patch` ✓，
//!   颜色在落笔时就烘进 blob ✓ ⇒ `data.color` **不参与渲染** ✗。
//!   拒绝之前它返回 `ok` 却**一个像素都不动** ✗（实测 96000 字节逐字节相同 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `brush_stroke` 要从那儿读 `.myb` 笔刷 ✓。
fn asset_workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!(
        "yanshi_update_stroke_{}_{}",
        name,
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_us", 200, 120),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_us", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(&mut ctx, tool, &args)
}

/// 在图层上画一条短笔触 ✓，返回它的 id ✓。
fn draw(workspace: &mut Workspace, object_id: &str, color: serde_json::Value) {
    let made = call(
        workspace,
        "draw_stroke",
        json!({
            "layer_id": "L",
            "object_id": object_id,
            "data": { "points": [[6.0, 8.0], [26.0, 8.0]], "size": 6, "color": color, "opacity": 1.0 },
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// **原始 RGBA** ✓（文档空间 ✓，不经白底合成 ✗ —— 量的是引擎真正写下的像素 ✓）。
fn pixels(workspace: &mut Workspace, width: f64, height: f64) -> Vec<u8> {
    workspace
        .document_mut("doc_us")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, width, height))
        .expect("区域渲染应成功")
        .2
}

/// **各通道占主导的像素数** ✓（通道 ≥200 且比另外两个通道高 60 ✓ ⇒ 只看**明确的主色** ✓）。
///
/// 这是"能红"的形状 ✓：颜色写死成某一色 ⇒ 另外两个计数必然为 0 ✓ ⇒ 判据当场抓住 ✓。
fn dominant(pixels: &[u8]) -> (usize, usize, usize) {
    let (mut red, mut green, mut blue) = (0, 0, 0);
    for pixel in pixels.chunks_exact(4) {
        let (r, g, b) = (
            i32::from(pixel[0]),
            i32::from(pixel[1]),
            i32::from(pixel[2]),
        );
        if r >= 200 && r > g + 60 && r > b + 60 {
            red += 1;
        }
        if g >= 200 && g > r + 60 && g > b + 60 {
            green += 1;
        }
        if b >= 200 && b > r + 60 && b > g + 60 {
            blue += 1;
        }
    }
    (red, green, blue)
}

/// 只改颜色 ✓（`core` 的三层参数里最常被误用的那一项 ✓）。
fn recolor(
    workspace: &mut Workspace,
    object_id: &str,
    color: serde_json::Value,
) -> serde_json::Value {
    call(
        workspace,
        "update_stroke",
        json!({ "object_id": object_id, "core": { "color": color } }),
    )
}

/// **改核心参数之后，对象数据与渲染像素都会变** ✓ —— 两次调用、两种颜色 ✓ + 方向检查 ✓。
#[test]
fn updating_a_strokes_core_changes_its_data_and_its_pixels() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_us", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    // 先用**纯红**画 ✓（方便断言通道 ✓）。
    draw(&mut workspace, "S1", json!([1.0, 0.0, 0.0, 1.0]));
    let got = call(&mut workspace, "get_object", json!({ "object_id": "S1" }));
    let red_before = got["data"]["color"].clone();
    let red_pixels = pixels(&mut workspace, 32.0, 32.0);
    let (red, green, blue) = dominant(&red_pixels);
    assert!(
        red > 0 && green == 0 && blue == 0,
        "画完就该是红占主导 ✓（实测 红 {red} / 绿 {green} / 蓝 {blue}）"
    );

    // ① 改成**绿色 + 更粗** ✓
    let updated = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "S1", "core": { "color": [0.0, 1.0, 0.0, 1.0], "size": 14.0 } }),
    );
    assert_eq!(updated["ok"], json!(true), "{updated}");
    // ② 事实一：对象数据变了 ✓
    let after = call(&mut workspace, "get_object", json!({ "object_id": "S1" }));
    let green_after = after["data"]["color"].clone();
    assert_ne!(green_after, red_before, "颜色数据应当变了：{after}");
    // ③ 事实二：**像素真的变了** ✓ + 方向检查 ✓
    let green_pixels = pixels(&mut workspace, 32.0, 32.0);
    assert_ne!(
        green_pixels, red_pixels,
        "改了颜色 ⇒ 像素必须不同 ✓（绝不能拿返回 ok 当判据 ✗）"
    );
    let (red, green, blue) = dominant(&green_pixels);
    assert!(
        green > 0 && red == 0 && blue == 0,
        "改成绿色之后应当是绿占主导 ✓（实测 红 {red} / 绿 {green} / 蓝 {blue}）"
    );

    // ④ **第二次调用、第三种颜色** ✓ —— 只测一次会漏掉"颜色被写死成某一次的值"✓。
    let again = recolor(&mut workspace, "S1", json!([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(again["ok"], json!(true), "{again}");
    let blue_pixels = pixels(&mut workspace, 32.0, 32.0);
    assert_ne!(blue_pixels, green_pixels, "第二次换色也必须改到像素 ✓");
    let (red, green, blue) = dominant(&blue_pixels);
    assert!(
        blue > 0 && red == 0 && green == 0,
        "改成蓝色之后应当是蓝占主导 ✓（实测 红 {red} / 绿 {green} / 蓝 {blue}）"
    );
}

/// **只给核心参数时，没提到的项保持不动** ✓（`core` 是**合并**，不是整体替换 ✓）。
#[test]
fn updating_one_core_field_leaves_the_others_alone() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_us", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    draw(&mut workspace, "S2", json!([1.0, 0.0, 0.0, 1.0]));
    let size_before =
        call(&mut workspace, "get_object", json!({ "object_id": "S2" }))["data"]["size"].clone();
    let done = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "S2", "core": { "opacity": 0.25 } }),
    );
    assert_eq!(done["ok"], json!(true), "{done}");
    let got = call(&mut workspace, "get_object", json!({ "object_id": "S2" }));
    assert_eq!(
        got["data"]["size"], size_before,
        "只改了 opacity ⇒ size 不应变：{got}"
    );
    assert_eq!(got["data"]["color"], json!([1.0, 0.0, 0.0, 1.0]), "{got}");
    assert_eq!(got["data"]["opacity"], json!(0.25), "{got}");
}

/// **烘进 blob 的光栅对象必须被拒绝** ✓ —— 不是"返回 ok、像素不动" ✗（用户报的 P0 ✓）。
///
/// **真实形状** ✓：`brush_stroke`（以及 `medium_stroke` / `import_image` ✓）落笔的产物是
/// **`raster_patch`** ✓：颜色在落笔那一刻就**烘进了 blob** ✓，渲染只读 `bitmap` / `region` ✓
/// ⇒ `data.color` **永远不参与渲染** ✗ ⇒ 此前 `update_stroke` 返回 `ok:true` 却**一个像素都不动** ✓。
#[test]
fn a_baked_raster_is_refused_instead_of_reporting_success() {
    let mut workspace = asset_workspace("baked");
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "B1", "brush": "2B_pencil", "size": 24,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "points": [[40.0, 60.0, 0.8], [160.0, 60.0, 0.8]],
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let got = call(&mut workspace, "get_object", json!({ "object_id": "B1" }));
    assert_eq!(
        got["type"],
        json!("raster_patch"),
        "落笔产物是光栅补丁：{got}"
    );
    assert!(
        got["data"].get("color").is_none(),
        "光栅对象的数据里**没有**颜色语义（只有 bitmap/region）：{got}"
    );
    let before = pixels(&mut workspace, 200.0, 120.0);

    // **换色必须被明确拒绝** ✓ —— 不许回 `ok` ✗（这正是用户报的病 ✓）。
    let refused = recolor(&mut workspace, "B1", json!([0.0, 1.0, 0.0, 1.0]));
    assert_eq!(
        refused["ok"],
        json!(false),
        "光栅对象换色不许报成功：{refused}"
    );
    assert_eq!(
        refused["error_code"],
        json!("invalid_argument"),
        "{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("raster_patch"),
        "错误必须点名真实类型，调用方才能自我纠正：{detail}"
    );
    assert!(
        detail.contains("stroke"),
        "错误必须说清它作用于什么类型：{detail}"
    );

    // **被拒绝的调用一个字节都不许动** ✓（没有原子被提交 ✓）。
    let after = pixels(&mut workspace, 200.0, 120.0);
    assert_eq!(before, after, "被拒绝的调用绝不许动画面 ✓");
    let got = call(&mut workspace, "get_object", json!({ "object_id": "B1" }));
    assert_eq!(got["versions"], json!(1), "拒绝之后版本链不该增长：{got}");
    assert!(
        got["data"].get("color").is_none(),
        "拒绝之后数据里不该冒出 color：{got}"
    );
}
