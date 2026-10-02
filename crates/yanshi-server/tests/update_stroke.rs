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

/// **画笔落下的笔触必须能真换色** ✓ —— 用户报的 P0 的**正面判据** ✓。
///
/// **修的是什么** ✗：光栅补丁的颜色**烘进像素** ✓ ⇒ 以前只改 `data.color` ⇒ 返回 `ok` 而**像素一点没变** ✗
///（我上一轮把这种"假成功"改成**明确拒绝** ✓ —— 那只是不再骗人 ✗，颜色还是改不了 ✗）。
/// 现在落笔时**把来源（笔刷 / 原始控制点 / size / color / smooth）留在对象上** ✓
/// ⇒ `update_stroke` 用新颜色**重放同一条笔迹** ✓ ⇒ 新 blob ✓、一条 `Supersede` 换掉 ✓。
///
/// **判据（三条 ✓）**：① 返回 ok ✓；② **像素真的变成新颜色** ✓（方向检查：红笔 R 主导 ⇒ 换成蓝笔后 B 主导 ✓）；
/// ③ **仍然只有一个对象 / 只多一条原子** ✓（不是"再画一笔叠上去" ✗ —— 那会让撤销变成两步 ✗）。
#[test]
fn update_stroke_recolours_a_brush_stroke() {
    let mut workspace = asset_workspace("recolour");
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "B1", "brush": "100%_Opaque", "size": 24,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "points": [[40.0, 60.0, 0.8], [180.0, 60.0, 0.8]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let before = pixels(&mut workspace, 200.0, 120.0);
    let (red_before, _, _) = dominant(&before);
    assert!(
        red_before > 0,
        "先画的那一笔应当是红的 ✓（实测 {red_before}）"
    );

    let updated = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "B1", "core": { "color": {"r": 0, "g": 0, "b": 255, "a": 255} } }),
    );
    assert_eq!(
        updated["ok"],
        json!(true),
        "画笔笔触换色必须成功：{updated}"
    );
    let after = pixels(&mut workspace, 200.0, 120.0);
    let (red, _, blue) = dominant(&after);
    assert!(
        blue > 0 && red == 0,
        "换色之后必须**真的**变蓝、且不留红 ✗（实测 红 {red} / 蓝 {blue}）"
    );
    // ②b **也能改成一笔多色** ✓（重跑认 `color_to` ✓）：起点蓝、末端绿 ⇒ 画面必须再变 ✓。
    let ramped = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "B1", "core": { "color_to": {"r": 0, "g": 255, "b": 0, "a": 255} } }),
    );
    assert_eq!(ramped["ok"], json!(true), "重跑也要能加/改末端色：{ramped}");
    let ramped_pixels = pixels(&mut workspace, 200.0, 120.0);
    assert_ne!(ramped_pixels, after, "加了末端色之后画面必须变 ✗");
    let (_, green_after, _) = dominant(&ramped_pixels);
    assert!(
        green_after > 0,
        "末端色是绿 ⇒ 应当出现偏绿像素 ✓（实测 {green_after}）"
    );

    // ③ **只多一条原子、对象仍然只有一个** ✓（不是又画了一笔 ✓）。
    let objects = call(&mut workspace, "list_objects", json!({}));
    assert_eq!(
        objects["count"],
        json!(1),
        "换色不该多出一个对象 ✗：{objects}"
    );
    // 对象上记的来源也跟着更新 ✓（下一次还能再改 ✓）。
    let got = call(&mut workspace, "get_object", json!({ "object_id": "B1" }));
    assert_eq!(
        got["data"]["source"]["color"],
        json!({"r": 0, "g": 0, "b": 255, "a": 255}),
        "来源参数应当同步更新（下次还能再改 ✓）：{got}"
    );
}

/// **没留下来源的光栅对象仍然明确拒绝** ✓（导入图 / 介质笔触 / 渐变 ✓ —— 它们反推不出画法 ✓）。
///
/// **判据** ✓：返回 `ok:false` ✓、错误里点名"没有留下来源参数" ✓、数据与像素**一个字节都不动** ✓。
#[test]
fn update_stroke_refuses_a_raster_without_a_source() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_us", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    // **渐变**会落成一个光栅补丁 ✓，而它**没有**"画笔来源" ✓。
    let filled = call(
        &mut workspace,
        "gradient_fill",
        json!({
            "layer_id": "L", "object_id": "G1",
            // **起点/终点色都是 `{r,g,b,a}` 对象** ✓（0..255 ✓）—— 参数名照工具 schema ✓，不猜 ✓。
            "from": {"r": 255, "g": 0, "b": 0, "a": 255},
            "to": {"r": 0, "g": 0, "b": 255, "a": 255}
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
    let before = pixels(&mut workspace, 32.0, 32.0);
    let refused = recolor(&mut workspace, "G1", json!([0.0, 1.0, 0.0, 1.0]));
    assert_eq!(
        refused["ok"],
        json!(false),
        "没有来源的光栅不许报成功：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("来源"),
        "错误必须说清「没有来源」这件事：{detail}"
    );
    let after = pixels(&mut workspace, 32.0, 32.0);
    assert_eq!(before, after, "被拒绝的调用绝不许动画布 ✓");
    let got = call(&mut workspace, "get_object", json!({ "object_id": "G1" }));
    assert_eq!(got["versions"], json!(1), "拒绝之后版本链不该涨：{got}");
}

/// **画笔笔触只认 color / size / opacity** ✓ —— 别的键**明确拒绝** ✗（不许静默忽略 ✓）。
#[test]
fn update_stroke_refuses_unsupported_keys_on_a_brush_stroke() {
    let mut workspace = asset_workspace("unsupported_key");
    call(&mut workspace, "create_layer", json!({ "layer_id": "L" }));
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "B2", "brush": "100%_Opaque", "size": 20,
            "points": [[30.0, 40.0, 0.8], [150.0, 40.0, 0.8]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let refused = call(
        &mut workspace,
        "update_stroke",
        json!({ "object_id": "B2", "core": { "blend_mode": "multiply" } }),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "不支持的键必须拒绝而非静默：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("blend_mode") && detail.contains("color"),
        "错误要点名是哪个键、以及只认哪些键：{detail}"
    );
}
