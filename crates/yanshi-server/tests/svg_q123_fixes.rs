//! **SVG 复测三问题修复的端到端判据**（2026-10-21 ✓）。
//!
//! - **Q3**：`draw_stroke` 的 `data.opacity` 曾被消费两次（`BrushSpec::from_value`
//!   读一次、渲染器对象通路再乘一次）⇒ 半透明笔按 opacity² 落墨。
//!   判据：opacity 0.5 的笔在 PNG 里的平台 alpha 必须是
//!   `1 − 0.5⁴ = 0.9375` 对应的颜色，而不是双算的 `1 − 0.75⁴ = 0.6836`。
//! - **Q1**：`flatten_opacity` 曾逐层各自对文档背景压平 ⇒ 多层笔触的顶层
//!   不透明后把下层贡献全遮掉（中心差 20 通道）。修法是**累积压平**
//!   （第 k 层 emit 色 = 前 k 层依次合成后的颜色）＋**真实 backdrop 均质判据**
//!   （异质底色不许用单色糊 ⇒ 回退位图并计数）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录／文件** ✓（第 820 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
/// **⇒ 绑的必须是**子目录或文件**，**绝不**是 `std::env::temp_dir()` 本身 ✓（那会删掉整个 `/tmp` ✗）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_file(&self.0);
    }
}

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: &Value) -> Value {
    let mut ctx = context(workspace, doc);
    registry().call(&mut ctx, tool, args)
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

fn temp_path(name: &str, ext: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "yanshi_q123_{}_{}.{}",
        name,
        std::process::id(),
        ext
    ));
    path
}

/// sRGB 字节色在 linear 空间按 alpha 混到另一个 sRGB 字节色上、再编码回字节
/// （与引擎合成同式 ⇒ 期望值从公式算、不写死快照 ✓）。
fn mix_bytes(fg: [u8; 3], bg: [u8; 3], alpha: f64) -> [i32; 3] {
    let mut out = [0i32; 3];
    for ch in 0..3 {
        let f = yanshi_render::color::srgb_to_linear(f32::from(fg[ch]) / 255.0);
        let b = yanshi_render::color::srgb_to_linear(f32::from(bg[ch]) / 255.0);
        let mixed = f * alpha as f32 + b * (1.0 - alpha as f32);
        out[ch] = (yanshi_render::color::linear_to_srgb(mixed) * 255.0).round() as i32;
    }
    out
}

fn assert_rgb_near(actual: [u8; 3], expected: [i32; 3], tol: i32, what: &str) {
    for ch in 0..3 {
        let diff = (i32::from(actual[ch]) - expected[ch]).abs();
        assert!(
            diff <= tol,
            "{what} 第 {ch} 通道：实测 {} vs 期望 {}（差 {diff} > {tol}）",
            actual[ch],
            expected[ch]
        );
    }
}

/// docE 同款笔：size 40 / hardness 1 / opacity 0.5 / spacing 0.25 / (200,40,40) 直线。
fn draw_e_stroke(workspace: &mut Workspace, doc: &str) {
    let drawn = call(
        workspace,
        doc,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": "e",
                "data": {"size": 40.0, "hardness": 1.0, "opacity": 0.5, "spacing": 0.25,
                         "color": {"r": 200, "g": 40, "b": 40, "a": 255},
                         "points": [[60.0, 180.0, 1.0], [420.0, 180.0, 1.0]]}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
}

fn export_png_pixels(workspace: &mut Workspace, doc: &str, name: &str) -> (u32, u32, Vec<u8>) {
    let path = temp_path(name, "png");
    let _cleanup_1 = Cleanup(path.clone());
    let _ = std::fs::remove_file(&path);
    let exported = call(
        workspace,
        doc,
        "export_png",
        &json!({ "path": path.to_string_lossy() }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let bytes = std::fs::read(&path).expect("PNG 应当落盘");
    let _ = std::fs::remove_file(&path);
    yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG")
}

fn export_svg_text(
    workspace: &mut Workspace,
    doc: &str,
    name: &str,
    flatten: bool,
) -> (Value, String) {
    let path = temp_path(name, "svg");
    let _cleanup_2 = Cleanup(path.clone());
    let _ = std::fs::remove_file(&path);
    let exported = call(
        workspace,
        doc,
        "export_svg",
        &json!({ "path": path.to_string_lossy(), "flatten_opacity": flatten }),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let text = std::fs::read_to_string(&path).expect("SVG 应当落盘");
    let _ = std::fs::remove_file(&path);
    (exported, text)
}

/// 按出现顺序抽出所有 `<path …>` 标签（笔触层与形状 path 都在内 ✓）。
fn path_tags(svg: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<path ") {
        let tail = &rest[start..];
        let end = tail.find("/>").map(|i| i + 2).unwrap_or(tail.len());
        tags.push(tail[..end].to_string());
        rest = &tail[end..];
    }
    tags
}

fn tag_fill(tag: &str) -> Option<[u8; 3]> {
    let start = tag.find("fill=\"#")? + 7;
    let rest = &tag[start..];
    let hex_len = rest.find('"')?;
    let hex = &rest[..hex_len];
    // srgb_hex 会把 #rrggbb 压成 #rgb（每字节两 nibble 相等时 ✓）⇒ 两种都要认。
    if hex.len() == 3 {
        let mut out = [0u8; 3];
        for (i, slot) in out.iter_mut().enumerate() {
            let c = hex.as_bytes()[i] as char;
            *slot = u8::from_str_radix(&format!("{c}{c}"), 16).ok()?;
        }
        return Some(out);
    }
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

/// **Q3**：opacity 只许算一次 —— PNG 中心像素必须是单算的平台色。
#[test]
fn stroke_opacity_is_applied_once_not_twice() {
    let mut workspace = workspace();
    make_doc(&mut workspace, "doc_q3", 480, 360);
    draw_e_stroke(&mut workspace, "doc_q3");
    let (w, _h, rgba) = export_png_pixels(&mut workspace, "doc_q3", "q3");
    let idx = (180 * w as usize + 240) * 4;
    let center = [rgba[idx], rgba[idx + 1], rgba[idx + 2]];
    // 平台 alpha = 1 − (1 − 0.5)^4 = 0.9375（spacing 0.25 ⇒ 覆盖数 K = 4 ✓）。
    // 双算时单 dab alpha 变 0.25、平台 0.6836、中心是 (219, 156, 156)。
    let expected = mix_bytes([200, 40, 40], [255, 255, 255], 0.9375);
    assert_eq!(expected, [204, 81, 81], "公式锚点先自检：{expected:?}");
    assert_rgb_near(center, expected, 2, "Q3 中心像素");
}

/// **Q1**：压平必须累积 —— 顶层 emit 色是全叠层的合成色、不带 fill-opacity。
#[test]
fn flatten_opacity_flattens_stroke_tiers_cumulatively() {
    let mut workspace = workspace();
    make_doc(&mut workspace, "doc_q1", 480, 360);
    draw_e_stroke(&mut workspace, "doc_q1");
    let (exported, svg) = export_svg_text(&mut workspace, "doc_q1", "q1flat", true);
    assert_eq!(exported["strokes_flattened"], json!(1), "{exported}");
    assert_eq!(
        exported["flatten_heterogeneous_raster"],
        json!(0),
        "{exported}"
    );
    let tags = path_tags(&svg);
    assert_eq!(tags.len(), 2, "半透明笔应有环+平台两层 path：{tags:?}");
    for tag in &tags {
        assert!(
            !tag.contains("fill-opacity"),
            "压平后的 path 不许再带 fill-opacity（会被 sRGB 二次混合）：{tag}"
        );
    }
    // 第 1 层（环）：A₁ = 单 dab 0.5；第 2 层（平台）：A₂ = 0.9375（累积 ✓）。
    let ring = tag_fill(&tags[0]).expect("环层应有 fill");
    assert_rgb_near(
        ring,
        mix_bytes([200, 40, 40], [255, 255, 255], 0.5),
        1,
        "环层压平色",
    );
    let plateau = tag_fill(&tags[1]).expect("平台层应有 fill");
    assert_rgb_near(
        plateau,
        mix_bytes([200, 40, 40], [255, 255, 255], 0.9375),
        1,
        "平台层累积压平色",
    );
    // 不压平 ⇒ 保留 fill-opacity 形式（现状不许被压平逻辑顺手改掉 ✓）。
    let (_raw, svg_raw) = export_svg_text(&mut workspace, "doc_q1", "q1raw", false);
    let raw_tags = path_tags(&svg_raw);
    assert_eq!(raw_tags.len(), 2, "{raw_tags:?}");
    assert!(
        raw_tags.iter().all(|t| t.contains("fill-opacity")),
        "不压平时每层应保留 fill-opacity：{raw_tags:?}"
    );
}

/// **Q1**：backdrop 必须是笔触下方的真实内容，不是文档背景字段。
#[test]
fn flatten_opacity_uses_the_real_backdrop() {
    let mut workspace = workspace();
    make_doc(&mut workspace, "doc_q1b", 480, 360);
    // 整幅蓝矩形垫底（文档背景仍是白 ⇒ 若误用背景字段会得到白底压平色 ✗）。
    let rect = call(
        &mut workspace,
        "doc_q1b",
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "bg",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 480, "h": 360}},
                         "color": {"r": 0, "g": 0, "b": 255, "a": 255}}}),
    );
    assert_eq!(rect["ok"], json!(true), "{rect}");
    draw_e_stroke(&mut workspace, "doc_q1b");
    let (exported, svg) = export_svg_text(&mut workspace, "doc_q1b", "q1b", true);
    assert_eq!(exported["strokes_flattened"], json!(1), "{exported}");
    let tags = path_tags(&svg);
    assert_eq!(tags.len(), 2, "{tags:?}");
    let plateau = tag_fill(&tags[1]).expect("平台层应有 fill");
    assert_rgb_near(
        plateau,
        mix_bytes([200, 40, 40], [0, 0, 255], 0.9375),
        1,
        "蓝 backdrop 上的累积压平色",
    );
}

/// **Q1**：异质 backdrop（笔横跨黑/白）不许单色压平 ⇒ 回退位图并计数。
#[test]
fn flatten_opacity_heterogeneous_backdrop_falls_back_to_raster() {
    let mut workspace = workspace();
    make_doc(&mut workspace, "doc_q1c", 480, 360);
    let rect = call(
        &mut workspace,
        "doc_q1c",
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "bg",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 240, "h": 360}},
                         "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
    );
    assert_eq!(rect["ok"], json!(true), "{rect}");
    draw_e_stroke(&mut workspace, "doc_q1c");
    let (exported, svg) = export_svg_text(&mut workspace, "doc_q1c", "q1c", true);
    assert_eq!(
        exported["flatten_heterogeneous_raster"],
        json!(1),
        "{exported}"
    );
    assert_eq!(exported["strokes_flattened"], json!(0), "{exported}");
    assert!(
        exported["strokes_raster"].as_u64().unwrap_or(0) >= 1,
        "异质底色笔应计入 strokes_raster：{exported}"
    );
    assert!(svg.contains("<image"), "回退段应以位图出现：{svg:.200}");
}

/// **Q1**：形状压平不回归 —— 半透明黑矩形 over 白 ⇒ 压平色逐通道对齐、无 fill-opacity。
#[test]
fn flatten_opacity_shape_stays_exact() {
    let mut workspace = workspace();
    make_doc(&mut workspace, "doc_q1s", 200, 200);
    let rect = call(
        &mut workspace,
        "doc_q1s",
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "sq",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 40, "y": 40, "w": 120, "h": 120}},
                         "color": {"r": 0, "g": 0, "b": 0, "a": 128}}}),
    );
    assert_eq!(rect["ok"], json!(true), "{rect}");
    let (exported, svg) = export_svg_text(&mut workspace, "doc_q1s", "q1s", true);
    assert_eq!(exported["shapes_flattened"], json!(1), "{exported}");
    // 背景 rect 也是 <rect> ⇒ 按坐标找形状那个（x="40" ✓）。
    let start = svg.find("<rect x=\"40\"").expect("应有形状 rect 元素");
    let tag = &svg[start..svg[start..].find("/>").map(|i| start + i + 2).unwrap()];
    assert!(
        !tag.contains("fill-opacity"),
        "压平后的 rect 不许带 fill-opacity：{tag}"
    );
    let fill = tag_fill(tag).expect("rect 应有 fill");
    assert_rgb_near(
        fill,
        mix_bytes([0, 0, 0], [255, 255, 255], 128.0 / 255.0),
        1,
        "形状压平色",
    );
}
