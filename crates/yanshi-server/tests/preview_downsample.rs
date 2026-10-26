//! 变更后预览降采样（P-01 M1）：大画布上变更的响应预览按档位渲染，
//! 导出与像素读数不受影响，整幅缓存不被降采样图污染。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

const W: u32 = 2048;
const H: u32 = 1536;

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 2_000)
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG"), "应为 PNG");
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

/// 取响应里的 blob（提交响应放在 `preview` 里）。
fn blob_for(response: &Value, workspace: &Workspace, key: &str) -> Vec<u8> {
    let url = response["preview"][key]
        .as_str()
        .or_else(|| response[key].as_str())
        .unwrap_or_else(|| panic!("响应缺少 {key}：{response}"));
    let raw = url.trim_start_matches("yanshi://blob/");
    let hash: yanshi_core::atom::BlobHash = raw
        .parse()
        .unwrap_or_else(|error| panic!("blob 地址 {raw} 非法：{error}"));
    workspace
        .store()
        .get(&hash)
        .unwrap_or_else(|error| panic!("取 blob 失败：{error}"))
}

/// 建 2048×1536 文档 + 一层（调用方再落笔）。
fn setup(workspace: &mut Workspace, doc: &str) {
    workspace
        .create_document(NewDocument::new(doc, W, H), "human:1", "session:test")
        .unwrap();
    let mut ctx = context(workspace, doc);
    registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
}

/// 横贯画布的一笔（脏区长边 > 1600，档位必定生效）。
fn wide_stroke(workspace: &mut Workspace, doc: &str) -> Value {
    let mut ctx = context(workspace, doc);
    registry().call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "L",
            "object_id": "wide",
            "data": {"points": [[40.0, 768.0], [2008.0, 768.0]], "size": 40.0,
                     "color": {"r": 200, "g": 40, "b": 40, "a": 255}}
        }),
    )
}

/// 大画布上变更的响应预览必须降采样（长边 ≤1600），且仍是区域预览而非 256 缩略图。
#[test]
fn mutation_preview_is_downsampled_on_large_canvas() {
    let mut workspace = workspace();
    setup(&mut workspace, "doc_down");
    let response = wide_stroke(&mut workspace, "doc_down");
    assert_eq!(response["ok"], json!(true), "{response}");
    // 前提：脏区长边确实 > 1600，否则档位没生效、测试本身无意义。
    let dirty_w = response["dirty_bbox"][2].as_f64().unwrap_or(0.0);
    assert!(
        dirty_w > 1600.0,
        "脏区应当横贯画布（实测宽 {dirty_w}），否则测不到降采样"
    );
    let width = response["preview"]["width"].as_u64().unwrap_or(0);
    let height = response["preview"]["height"].as_u64().unwrap_or(0);
    assert!(
        width <= 1600,
        "响应预览长边必须 ≤1600（实测 {width}×{height}，脏区宽 {dirty_w}）"
    );
    assert!(
        width > 256,
        "响应预览应当是区域预览（实测 {width}×{height}），不是 256 缩略图"
    );
    assert!(
        (width as f64) < dirty_w,
        "响应预览必须比全幅脏区小（实测 {width}，脏区宽 {dirty_w}）"
    );
    let bytes = blob_for(&response, &workspace, "blob_hash");
    assert_eq!(
        png_size(&bytes),
        (width as u32, height as u32),
        "预览 blob 的 PNG 尺寸应当与回执一致"
    );
    // 文档级缩略图仍新鲜，且是 256²。
    assert!(
        workspace.document_thumbnail_is_current("doc_down"),
        "变更后文档缩略图应当与 HEAD 一致"
    );
    let document = {
        let mut ctx = context(&mut workspace, "doc_down");
        registry().call(&mut ctx, "get_document", &json!({ "preview_size": 256 }))
    };
    let thumb = blob_for(&document, &workspace, "thumb_url");
    assert_eq!(png_size(&thumb), (256, 256), "文档级预览应为 256²");
}

/// 降采样预览不改变文档像素：同一笔，有预览与无预览的两份文档，全幅导出逐字节相同；
/// 且降采样预览不污染整幅缓存（随后一次显式全幅渲染必须真渲染、不命中，第二次才命中）。
#[test]
fn mutation_preview_does_not_change_full_export() {
    let mut with_preview = workspace();
    let mut without_preview = workspace();
    setup(&mut with_preview, "doc_a");
    setup(&mut without_preview, "doc_b");
    wide_stroke(&mut with_preview, "doc_a");
    {
        let mut ctx = context(&mut without_preview, "doc_b");
        ctx.preview = false;
        registry().call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "L",
                "object_id": "wide",
                "data": {"points": [[40.0, 768.0], [2008.0, 768.0]], "size": 40.0,
                         "color": {"r": 200, "g": 40, "b": 40, "a": 255}}
            }),
        );
    }
    let export = |workspace: &mut Workspace, doc: &str, name: &str| -> Vec<u8> {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "yanshi_preview_down_{name}_{}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut ctx = context(workspace, doc);
        let exported = registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        );
        assert_eq!(exported["ok"], json!(true), "{exported}");
        assert_eq!(exported["width"], json!(W), "{exported}");
        let bytes = std::fs::read(&path).expect("导出文件应当存在");
        let _ = std::fs::remove_file(&path);
        bytes
    };
    let a = export(&mut with_preview, "doc_a", "a");
    let b = export(&mut without_preview, "doc_b", "b");
    assert_eq!(a, b, "有预览与无预览的全幅导出必须逐字节相同");

    // 整幅缓存未被污染：第一次显式全幅渲染是真渲染（不命中），且给出全幅像素。
    assert_eq!(with_preview.document_full_frame_hits("doc_a"), 0);
    let first = {
        let mut ctx = context(&mut with_preview, "doc_a");
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": W, "h": H}}),
        )
    };
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(first["width"], json!(W), "{first}");
    assert_eq!(
        with_preview.document_full_frame_hits("doc_a"),
        0,
        "降采样预览后第一次全幅请求不得命中整幅缓存"
    );
    // 第二次同一请求命中（整幅缓存机制本身仍工作）。
    let _second = {
        let mut ctx = context(&mut with_preview, "doc_a");
        registry().call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": W, "h": H}}),
        )
    };
    assert_eq!(
        with_preview.document_full_frame_hits("doc_a"),
        1,
        "真全幅渲染一次后，整幅缓存应当能命中"
    );
}
