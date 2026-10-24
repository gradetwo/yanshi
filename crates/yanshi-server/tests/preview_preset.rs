//! 期 4 D2-1 集成测试：`preview_preset` 的渲染路径（走真实工具注册表 +
//! 落盘工作区）。
//!
//! 与单元测试的分工：单元测试钉住盒式降采样、render 参数解析、尺度标注
//! 的纯逻辑；这里钉住工具行为——无 render 时响应不变、有 render 时出图
//! 且临时文档消失、原文档原子数不变、代理尺度下预览与真 apply 逐像素
//! 相等、尺度标注随预设变化、非法 render 参数响亮拒绝。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **⇒ 临时目录必须在测试结束时删掉** ✓（第 930 轮；`check-tests-clean-temp` 抓到的 ✓）
/// **⇒ 为什么** ✗：`/tmp` 是 **1.9 GB tmpfs** ✓ ⇒ 漏删会把 `bash` 都撑死 ✗
pub struct TempCleanup(pub std::path::PathBuf);
impl Drop for TempCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_preview_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 合成源图（64×48，全不透明）：水平渐变 + 纯色块。
fn synthetic(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let gradient = (x * 255 / width as usize) as u8;
            let (r, g, b) = if x >= 48 && y >= 32 {
                (200, 30, 30)
            } else {
                (gradient, 255 - gradient, 90)
            };
            pixels[base] = r;
            pixels[base + 1] = g;
            pixels[base + 2] = b;
            pixels[base + 3] = 255;
        }
    }
    pixels
}

/// 建源图层并导入合成图，返回图层 id。
fn setup_source(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let pixels = synthetic(width, height);
    let hash = workspace.put_blob(&pixels).expect("放源图 blob");
    let imported = call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": "src",
            "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 0, "y": 0, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    "src".to_owned()
}

fn full_bbox(width: u32, height: u32) -> Bbox {
    Bbox::new(0.0, 0.0, width as f64, height as f64)
}

fn decode_preview_png(result: &Value) -> (u32, u32, Vec<u8>) {
    let base64 = result["image_base64"].as_str().expect("应回 image_base64");
    let bytes = yanshi_server::base64::decode(base64).expect("base64 应能解码");
    yanshi_render::png::decode_png(&bytes).expect("预览 PNG 应能解码")
}

/// 不给 render ⇒ 响应与此前一致：无 image 字段、notes 说“不渲染”、
/// ops/op_count 齐全。
#[test]
fn preview_preset_without_render_is_unchanged() {
    let root = temp_dir("no_render");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_plain";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let result = call(
        &mut workspace,
        doc,
        "preview_preset",
        json!({"preset_id": "ccd"}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert!(
        result["image_base64"].is_null(),
        "不给 render 时响应不得带 image_base64：{result}"
    );
    assert!(
        result["notes"]
            .as_str()
            .unwrap_or_default()
            .contains("不渲染"),
        "不给 render 时 notes 应说不渲染：{result}"
    );
    assert_eq!(result["preset_id"], json!("ccd"));
    assert!(
        result["op_count"].as_u64().unwrap_or(0) > 0,
        "展开笔数应大于 0：{result}"
    );
    assert_eq!(
        result["ops"].as_array().map(Vec::len).unwrap_or(0) as u64,
        result["op_count"].as_u64().unwrap_or(0),
        "ops 长度应等于 op_count"
    );
    let _ = src;
}

/// 有 render ⇒ 出图（原生 ≤ max_edge 故尺寸不变）、临时文档在
/// list_documents 中消失、原文档 head_seq 不变。
#[test]
fn preview_preset_render_returns_image_and_cleans_up() {
    let root = temp_dir("render");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_render";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let before = workspace.document(doc).unwrap().state().head_seq;
    let result = call(
        &mut workspace,
        doc,
        "preview_preset",
        json!({"preset_id": "ccd", "layer_id": src, "render": {"max_edge": 512, "format": "png"}}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["image_width"], json!(width));
    assert_eq!(result["image_height"], json!(height));
    assert_eq!(result["image_format"], json!("png"));
    assert!(
        result["ops_applied"].as_u64().unwrap_or(0) > 0,
        "应落成至少一笔：{result}"
    );
    let (w, h, pixels) = decode_preview_png(&result);
    assert_eq!((w, h), (width, height));
    assert_eq!(pixels.len(), (width as usize) * (height as usize) * 4);
    let listed = call(&mut workspace, doc, "list_documents", json!({}));
    let docs = listed["documents"].as_array().expect("应回 documents 数组");
    assert!(
        !docs.iter().any(|d| d["doc_id"]
            .as_str()
            .unwrap_or_default()
            .starts_with("preview_")),
        "临时文档必须已删除：{listed}"
    );
    let after = workspace.document(doc).unwrap().state().head_seq;
    assert_eq!(
        before, after,
        "预览渲染不得动原文档原子：{before} → {after}"
    );
}

/// 代理尺度一致性：原生即 ≤ max_edge 的小图，preview 渲染（png）与
/// 真 apply 后整幅渲染逐像素相等——守“同一份展开 + 同一份落成”；
/// 展开器或落成循环被复制改坏一处 ⇒ 本条红。
#[test]
fn preview_preset_render_matches_apply_at_proxy_scale() {
    let root = temp_dir("match");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_match";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let preview = call(
        &mut workspace,
        doc,
        "preview_preset",
        json!({"preset_id": "ccd", "layer_id": src, "render": {"max_edge": 512, "format": "png"}}),
    );
    assert_eq!(preview["ok"], json!(true), "{preview}");
    let (_, _, preview_pixels) = decode_preview_png(&preview);
    let applied = call(
        &mut workspace,
        doc,
        "apply_preset",
        json!({"preset_id": "ccd", "layer_id": src}),
    );
    assert_eq!(applied["ok"], json!(true), "{applied}");
    let (_, _, applied_pixels) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("apply 后整幅渲染");
    assert_eq!(
        preview_pixels, applied_pixels,
        "同图同预设同强度：预览渲染必须与真 apply 渲染逐像素相等"
    );
}

/// 尺度标注：含 clarity 的预设 ⇒ 非空且点名（空分支见单元测试的
/// 合成操作串：22 套库预设全含尺度敏感操作，无库预设可测空分支）。
#[test]
fn preview_preset_render_notes_scale_sensitive_ops() {
    let root = temp_dir("notes");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_notes";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let with_clarity = call(
        &mut workspace,
        doc,
        "preview_preset",
        json!({"preset_id": "ccd", "layer_id": src, "render": {"format": "png"}}),
    );
    assert_eq!(with_clarity["ok"], json!(true), "{with_clarity}");
    assert!(
        with_clarity["scale_notes"]
            .as_str()
            .unwrap_or_default()
            .contains("clarity"),
        "含 clarity 时 scale_notes 必须点名：{with_clarity}"
    );
}

/// 缺省渲染格式是 jpeg：不给 format ⇒ image_format 为 jpeg，且字节
/// 以 JPEG SOI（FF D8 FF）开头（本仓无 JPEG 解码依赖进测试，故以
/// 魔数断“确实是 JPEG”，不只断字段）。
#[test]
fn preview_preset_render_defaults_to_jpeg() {
    let root = temp_dir("jpeg");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_jpeg";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let result = call(
        &mut workspace,
        doc,
        "preview_preset",
        json!({"preset_id": "ccd", "layer_id": src, "render": {}}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["image_format"], json!("jpeg"));
    let base64 = result["image_base64"].as_str().expect("应回 image_base64");
    let bytes = yanshi_server::base64::decode(base64).expect("base64 应能解码");
    assert!(
        bytes.len() > 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF,
        "缺省渲染字节应以 JPEG SOI 开头，得到前 3 字节 {:02X?}",
        &bytes[..bytes.len().min(3)]
    );
}

/// 非法 render 参数一律响亮拒绝（invalid_argument），不静默取缺省。
#[test]
fn preview_preset_render_rejects_bad_args() {
    let root = temp_dir("bad_args");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_bad";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    for (label, args) in [
        ("缺 layer_id", json!({"preset_id": "ccd", "render": {}})),
        (
            "max_edge 越下界",
            json!({"preset_id": "ccd", "layer_id": src, "render": {"max_edge": 64}}),
        ),
        (
            "format 非法",
            json!({"preset_id": "ccd", "layer_id": src, "render": {"format": "webp"}}),
        ),
        (
            "render 未知键",
            json!({"preset_id": "ccd", "layer_id": src, "render": {"quality": 90}}),
        ),
    ] {
        let result = call(&mut workspace, doc, "preview_preset", args);
        assert_eq!(result["ok"], json!(false), "{label} 应拒绝：{result}");
        assert_eq!(
            result["error_code"],
            json!("invalid_argument"),
            "{label} 应为 invalid_argument：{result}"
        );
    }
}
