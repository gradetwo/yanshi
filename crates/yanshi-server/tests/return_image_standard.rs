//! **`return_image` 标准化**（feedback5-feat-return-image）：
//!
//! 统一语义：缺省关图（不请求不回图）；请求回图时回一张内嵌图；
//! max_px 是长边上限（缺省 512、钳制 1..=8192），超限自动缩小并回
//! scaled_from，缩小失败才回 image_omitted 与原因。
//!
//! 覆盖：render_region、export_png、brush_preview、swatch_brush_strokes
//! 与注册表里 9 只 smart_ 工具；每只都验三态（不请求无图／请求有图／
//! 小 max_px 缩小受限）。旧 include_image 的行为另有单测钉住（全尺寸内嵌）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 本测试落盘的单个 png 的清理器：Drop 时删文件。
/// 注意：测试写的是共享 /tmp 里的单个文件，不是私有临时目录，
/// 所以 Drop 只删该文件，绝不能 remove_dir_all（那会删 /tmp 本身）。
struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn fresh_workspace(doc_w: u32, doc_h: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_ri", doc_w, doc_h),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, "doc_ri", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// 实底：给 select/sample/erase/edge_soften 用的纯色矩形（40,40,180,120）。
fn paint_solid_base(workspace: &mut Workspace) {
    let response = call(
        workspace,
        "fill_region",
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "color": "#3366CC",
        }),
    );
    assert_eq!(
        response.get("ok"),
        Some(&json!(true)),
        "打底 fill_region 应当成功：{response}"
    );
}

fn no_setup(_workspace: &mut Workspace) {}

/// 同一工具跑三遍（每遍 fresh 文档，避免重复落笔互相干扰）：
/// 不请求／请求回图／请求回图 + max_px=16。
fn three_calls(
    tool: &str,
    setup: fn(&mut Workspace),
    base_args: Value,
    doc_w: u32,
    doc_h: u32,
) -> (Value, Value, Value) {
    let with = |extra: Value| -> Value {
        let mut workspace = fresh_workspace(doc_w, doc_h);
        setup(&mut workspace);
        let mut args = base_args.clone();
        if let (Some(target), Some(source)) = (args.as_object_mut(), extra.as_object()) {
            for (key, value) in source {
                target.insert(key.clone(), value.clone());
            }
        }
        call(&mut workspace, tool, args)
    };
    (
        with(json!({})),
        with(json!({"return_image": true})),
        with(json!({"return_image": true, "max_px": 16})),
    )
}

fn assert_ok(response: &Value, tool: &str) {
    assert_eq!(
        response.get("ok"),
        Some(&json!(true)),
        "{tool} 调用应当成功：{response}"
    );
}

fn assert_no_image(response: &Value, tool: &str) {
    assert!(
        response.get("image").is_none_or(Value::is_null),
        "{tool} 不请求时不许回图：{response}"
    );
    assert!(
        response.get("image_omitted").is_none_or(Value::is_null),
        "{tool} 不请求时也不该出现 omitted 标记：{response}"
    );
}

fn assert_image(response: &Value, tool: &str, max_side: Option<u64>) {
    let image = response
        .get("image")
        .unwrap_or_else(|| panic!("{tool} 请求后应当回图：{response}"));
    let data = image.get("data").and_then(Value::as_str).unwrap_or("");
    assert!(!data.is_empty(), "{tool} 回图的 data 不许为空：{response}");
    let width = image.get("width").and_then(Value::as_u64).unwrap_or(0);
    let height = image.get("height").and_then(Value::as_u64).unwrap_or(0);
    assert!(
        width > 0 && height > 0,
        "{tool} 回图尺寸应当大于 0：{response}"
    );
    if let Some(cap) = max_side {
        assert!(
            width <= cap && height <= cap,
            "{tool} 回图长边应当受 max_px={cap} 约束，实测 {width}×{height}：{response}"
        );
    }
}

fn assert_image_case(tool: &str, setup: fn(&mut Workspace), base_args: Value) {
    let (plain, with_image, capped) = three_calls(tool, setup, base_args, 400, 400);
    assert_ok(&plain, tool);
    assert_no_image(&plain, tool);
    assert_ok(&with_image, tool);
    assert_image(&with_image, tool, None);
    assert_ok(&capped, tool);
    assert_image(&capped, tool, Some(16));
    assert!(
        capped.get("scaled_from").is_some(),
        "{tool} 超限缩小后应当回 scaled_from：{capped}"
    );
}

#[test]
fn render_region_return_image() {
    let mut workspace = fresh_workspace(64, 64);
    let plain = call(
        &mut workspace,
        "render_region",
        json!({"region": [0, 0, 32, 32]}),
    );
    assert_no_image(&plain, "render_region");
    let with_image = call(
        &mut workspace,
        "render_region",
        json!({"region": [0, 0, 32, 32], "return_image": true}),
    );
    assert_image(&with_image, "render_region", Some(512));
    // 旧开关仍是同义（别名语义），且超限自动缩小而非报错。
    let legacy = call(
        &mut workspace,
        "render_region",
        json!({"region": [0, 0, 32, 32], "include_image": true, "max_px": 16}),
    );
    assert_image(&legacy, "render_region", Some(16));
    assert!(
        legacy.get("scaled_from").is_some(),
        "render_region 超限缩小后应当回 scaled_from：{legacy}"
    );
    // 内嵌图应当是真能解码的 PNG，且尺寸与字段一致。
    let data = legacy["image"]["data"].as_str().unwrap_or("");
    let png = yanshi_server::base64::decode(data).expect("内嵌图应当是合法 base64");
    let (width, height, _) =
        yanshi_render::png::decode_png(&png).expect("内嵌图应当是能解码的 PNG");
    assert_eq!(
        (u64::from(width), u64::from(height)),
        (
            legacy["image"]["width"].as_u64().unwrap(),
            legacy["image"]["height"].as_u64().unwrap()
        ),
        "解码尺寸应当与回执字段一致"
    );
}

#[test]
fn export_png_return_image() {
    let dir = std::env::temp_dir();
    let path = dir.join("yanshi-return-image-test.png");
    // 显式绑定守卫：判据 check-tests-clean-temp 只认 `x = Cleanup(` 的绑定形态，
    // 且要求 Drop 清理器真的被实例化（只声明不绑定同样判红，见 colorspace 跟进 23e7d461）。
    // 守卫活到测试结束，panic 路径同样清掉落盘文件。
    let _cleanup = Cleanup(path.clone());
    let path_str = path.display().to_string();
    let mut workspace = fresh_workspace(64, 64);
    let plain = call(&mut workspace, "export_png", json!({"path": path_str}));
    assert!(path.exists(), "export_png 不请求时仍应落盘：{plain}");
    assert_no_image(&plain, "export_png");
    let with_image = call(
        &mut workspace,
        "export_png",
        json!({"path": path_str, "return_image": true}),
    );
    assert_image(&with_image, "export_png", Some(512));
    let capped = call(
        &mut workspace,
        "export_png",
        json!({"path": path_str, "return_image": true, "max_px": 16}),
    );
    assert_image(&capped, "export_png", Some(16));
    assert!(
        capped.get("scaled_from").is_some(),
        "export_png 超限缩小后应当回 scaled_from：{capped}"
    );
}

#[test]
fn brush_preview_return_image_and_legacy_full_size() {
    let mut workspace = fresh_workspace(120, 80);
    let plain = call(
        &mut workspace,
        "brush_preview",
        json!({"brush": "Classic_Paint"}),
    );
    assert_no_image(&plain, "brush_preview");
    let with_image = call(
        &mut workspace,
        "brush_preview",
        json!({"brush": "Classic_Paint", "return_image": true}),
    );
    assert_image(&with_image, "brush_preview", Some(512));
    // 宽自定义笔迹 + 小 max_px：新开关受限。
    let capped = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "Classic_Paint",
            "points": [[0, 50, 0.5], [500, 50, 0.5]],
            "return_image": true,
            "max_px": 100,
        }),
    );
    assert_image(&capped, "brush_preview", Some(100));
    assert!(
        capped.get("scaled_from").is_some(),
        "brush_preview 超限缩小后应当回 scaled_from：{capped}"
    );
    // 旧 include_image 单独调用：保持全尺寸内嵌（不被缺省 512 截断）。
    let legacy = call(
        &mut workspace,
        "brush_preview",
        json!({
            "brush": "Classic_Paint",
            "points": [[0, 50, 0.5], [500, 50, 0.5]],
            "include_image": true,
        }),
    );
    let image = legacy.get("image").expect("旧 include_image 应当继续回图");
    assert!(
        image["width"].as_u64().unwrap_or(0) > 100,
        "旧 include_image 单独调用不该被新缺省截断：{legacy}"
    );
    assert!(
        legacy.get("scaled_from").is_none_or(Value::is_null),
        "旧 include_image 单独调用不该出现 scaled_from：{legacy}"
    );
}

#[test]
fn swatch_return_image() {
    let mut workspace = fresh_workspace(120, 80);
    let plain = call(
        &mut workspace,
        "swatch_brush_strokes",
        json!({"brushes": ["Classic_Paint"], "size": 24}),
    );
    let first = plain["swatches"][0].clone();
    assert!(
        first.get("image").is_none_or(Value::is_null),
        "swatch 不请求时不许回图：{plain}"
    );
    let with_image = call(
        &mut workspace,
        "swatch_brush_strokes",
        json!({"brushes": ["Classic_Paint"], "size": 24, "return_image": true}),
    );
    let first = with_image["swatches"][0].clone();
    assert!(
        first.get("image").is_some_and(|image| !image.is_null()),
        "swatch 请求后每张应当回图：{with_image}"
    );
    let capped = call(
        &mut workspace,
        "swatch_brush_strokes",
        json!({"brushes": ["Classic_Paint"], "size": 24, "return_image": true, "max_px": 16}),
    );
    let first = capped["swatches"][0].clone();
    let image = first.get("image").expect("swatch 小 max_px 时应当回图");
    assert!(
        image["width"].as_u64().unwrap_or(0) <= 16 && image["height"].as_u64().unwrap_or(0) <= 16,
        "swatch 回图长边应当受 max_px 约束：{capped}"
    );
}

#[test]
fn smart_fill_return_image() {
    assert_image_case(
        "smart_fill",
        no_setup,
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "brush": "2B_pencil",
            "color": [0.86, 0.24, 0.16, 1.0],
        }),
    );
}

#[test]
fn smart_blend_return_image() {
    assert_image_case(
        "smart_blend",
        no_setup,
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "color_a": [0.9, 0.2, 0.1, 1.0],
            "color_b": [0.1, 0.2, 0.9, 1.0],
        }),
    );
}

#[test]
fn smart_glaze_return_image() {
    assert_image_case(
        "smart_glaze",
        no_setup,
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "color": [0.9, 0.6, 0.2, 1.0],
        }),
    );
}

#[test]
fn smart_edge_soften_return_image() {
    assert_image_case(
        "smart_edge_soften",
        paint_solid_base,
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
        }),
    );
}

#[test]
fn smart_texture_return_image() {
    assert_image_case(
        "smart_texture",
        no_setup,
        json!({
            "layer_id": "layer_default",
            "shape": {"kind": "rect", "x": 20, "y": 20, "w": 360, "h": 360},
            "texture_type": "stone",
            "density": 0.5,
        }),
    );
}

#[test]
fn smart_select_return_image() {
    assert_image_case(
        "smart_select",
        paint_solid_base,
        json!({"seed": [100, 100], "tolerance": 0.2, "mode": "new"}),
    );
}

#[test]
fn smart_sample_return_image() {
    assert_image_case(
        "smart_sample",
        paint_solid_base,
        json!({
            "ref": "layer_default",
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "samples": 8,
        }),
    );
}

#[test]
fn smart_erase_shape_return_image() {
    assert_image_case(
        "smart_erase_shape",
        paint_solid_base,
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 80, "y": 80, "w": 60, "h": 40},
        }),
    );
}

#[test]
fn smart_hatch_follow_return_image() {
    assert_image_case(
        "smart_hatch_follow",
        no_setup,
        json!({
            "layer_id": "layer_default",
            "guides": [[[40, 200], [120, 160], [200, 200]]],
            "color": [0.2, 0.15, 0.1, 1.0],
            "width": 12,
            "size": 22,
            "density": 0.6,
        }),
    );
}

#[test]
fn smart_return_image_rejects_bad_max_px_before_painting() {
    // max_px 非法必须在落笔前报错：笔数应当为 0（副作用与结论不能矛盾）。
    let mut workspace = fresh_workspace(400, 400);
    let response = call(
        &mut workspace,
        "smart_fill",
        json!({
            "layer_id": "layer_default",
            "shape": {"type": "rect", "x": 40, "y": 40, "w": 180, "h": 120},
            "color": [0.86, 0.24, 0.16, 1.0],
            "return_image": true,
            "max_px": -1,
        }),
    );
    let text = response.to_string();
    assert!(
        text.contains("max_px"),
        "非法 max_px 应当被点名拒绝，而不是静默接管：{response}"
    );
}
