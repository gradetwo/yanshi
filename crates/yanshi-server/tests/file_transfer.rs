//! **文件传输双通道**（设计文档 `yanshi-RAW导入-需求与详细设计-20261021.md` 第 4 章）。
//!
//! 判据的形状（按仓库 AGENTS.md 的内核）：**恒等式优先于单点断言**——
//!   · 入参侧：同一份字节走 本地 path／http URL／`data_base64` 三条通道入库
//!     ⇒ `blob_hash` **必须相等**（CAS 的 hash 就是字节身份，不相等即通道改了字节）；
//!   · 出参侧：同一预览走 内嵌 base64 与 `output_path` 落盘 ⇒ 文件字节与解码后的
//!     base64 **逐字节相等**；
//!   · 错误侧：互斥／缺参／类型不符都必须**带错误码且文案点名缺什么**，不许静默。
//!
//! HTTP 一律走本文件内的 127.0.0.1 回环服务器（零外网依赖）。

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 1×1 透明 PNG（标准测试图，正文带 `=` 填充，顺带覆盖填充解码路径）。
/// **跑完即删临时目录** ✓（第 821 轮；`/tmp` 是 1.9 GB tmpfs ✓）
/// **⇒ 只绑每个测试自己的 `root`** ✓ —— `exports_root()` 是**进程级共享**目录（`Once` ✓）
///   ⇒ 给它绑守卫会在某个测试结束时删掉别的测试还要用的东西 ✗
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        let _ = std::fs::remove_file(&self.0);
    }
}

const PNG_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn png_bytes() -> Vec<u8> {
    yanshi_server::base64::decode(PNG_B64).expect("测试图 base64 必须可解")
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("yanshi_filexfer_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn workspace(root: &Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(NewDocument::new("doc_a", 32, 32), "human:1", "session:test")
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, "doc_a", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// 笔刷类工具要能解析笔刷资产：与 tests/brush_preview.rs 同款的
/// in-memory 工作区 + 仓库 assets 目录（落盘工作区在 /tmp 下解析不到笔刷）。
fn brush_workspace() -> Workspace {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_a", 120, 80),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// 出参落盘守卫读 `YANSHI_EXPORT_DIR`——本测试二进制内统一指到临时目录
/// （进程级 env，同二进制内的用例共享同一导出根，互不干扰因为文件名各异）。
fn exports_root() -> PathBuf {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let dir = std::env::temp_dir().join(format!("yanshi_filexfer_exports_{}", std::process::id()));
    ONCE.call_once(|| {
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("YANSHI_EXPORT_DIR", &dir);
    });
    dir
}

fn detail(response: &Value) -> String {
    response["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ───────────────────────── 回环 HTTP 服务器 ─────────────────────────

/// 起一个只服务本测试的回环服务器；路由按 path 分发，线程随进程退出而亡。
fn serve(routes: Vec<(&'static str, u16, &'static str, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let Ok(n) = stream.read(&mut buf) else {
                continue;
            };
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = request
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .split('?')
                .next()
                .unwrap_or("/")
                .to_owned();
            if path == "/blackhole" {
                // 接了连接但永不回话 ⇒ 客户端必须在总时限内报超时，而不是挂死。
                std::thread::sleep(std::time::Duration::from_secs(5));
                continue;
            }
            let (status, content_type, body) = match routes.iter().find(|(p, ..)| *p == path) {
                Some((_, status, content_type, body)) => (*status, *content_type, body.clone()),
                None => (404, "text/plain", b"not found".to_vec()),
            };
            let reason = match status {
                200 => "OK",
                302 => "Found",
                404 => "Not Found",
                _ => "X",
            };
            let location = if status == 302 {
                "Location: /img.png\r\n"
            } else {
                ""
            };
            let head = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{location}Connection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    format!("http://127.0.0.1:{port}")
}

// ───────────────────────── put_blob：三通道恒等 ─────────────────────────

#[test]
fn put_blob_three_channels_land_on_the_same_hash() {
    let root = temp_dir("putblob3");
    let _cleanup_1 = Cleanup(root.clone());
    let mut ws = workspace(&root);
    std::fs::write(root.join("in.png"), png_bytes()).unwrap();

    let by_path = call(
        &mut ws,
        "put_blob",
        json!({ "path": root.join("in.png").display().to_string() }),
    );
    let by_b64 = call(&mut ws, "put_blob", json!({ "data_base64": PNG_B64 }));

    let base = serve(vec![("/img.png", 200, "image/png", png_bytes())]);
    let by_url = call(
        &mut ws,
        "put_blob",
        json!({ "path": format!("{base}/img.png") }),
    );

    for (label, resp) in [("path", &by_path), ("base64", &by_b64), ("url", &by_url)] {
        assert_eq!(resp["ok"], json!(true), "{label} 通道应成功：{resp}");
    }
    // **恒等式**：三条通道的 blob_hash 必须相等——hash 即字节身份。
    assert_eq!(
        by_path["blob_hash"], by_b64["blob_hash"],
        "path 与 base64 通道字节不一致"
    );
    assert_eq!(
        by_path["blob_hash"], by_url["blob_hash"],
        "path 与 URL 通道字节不一致"
    );
    assert_eq!(by_b64["source"], json!("base64"));
    assert_eq!(by_url["source"], json!("url"));
    assert_eq!(by_url["detected_format"], json!("png"));
    // 本地 path 通道的响应**不许长新字段**（老客户端逐字兼容）。
    assert!(
        by_path.get("source").is_none(),
        "本地 path 响应形状变了：{by_path}"
    );
}

#[test]
fn put_blob_rejects_conflicts_and_bad_base64() {
    let root = temp_dir("putblob_bad");
    let _cleanup_2 = Cleanup(root.clone());
    let mut ws = workspace(&root);
    std::fs::write(root.join("in.png"), png_bytes()).unwrap();

    let both = call(
        &mut ws,
        "put_blob",
        json!({ "path": root.join("in.png").display().to_string(), "data_base64": PNG_B64 }),
    );
    assert_eq!(both["ok"], json!(false), "同给两来源必须失败：{both}");
    assert!(
        detail(&both).contains("互斥"),
        "报错要说清是互斥：{}",
        detail(&both)
    );

    let prefixed = call(
        &mut ws,
        "put_blob",
        json!({ "data_base64": format!("data:image/png;base64,{PNG_B64}") }),
    );
    assert_eq!(
        prefixed["ok"],
        json!(false),
        "带 data: 前缀必须拒：{prefixed}"
    );
    assert!(
        detail(&prefixed).contains("data:"),
        "报错要点名前缀问题：{}",
        detail(&prefixed)
    );

    let none = call(&mut ws, "put_blob", json!({}));
    assert_eq!(none["ok"], json!(false), "一个来源都不给必须失败：{none}");

    let bad_scheme = call(&mut ws, "put_blob", json!({ "path": "file:///etc/passwd" }));
    assert_eq!(
        bad_scheme["ok"],
        json!(false),
        "file:// 必须拒：{bad_scheme}"
    );
    assert!(
        detail(&bad_scheme).contains("scheme"),
        "报错要点名 scheme：{}",
        detail(&bad_scheme)
    );
}

// ───────────────────────── URL 拉取细则（fetch 层） ─────────────────────────

#[test]
fn fetch_follows_redirects_and_classifies_failures() {
    use yanshi_server::file_source::{fetch_url, fetch_url_with};
    let base = serve(vec![
        ("/img.png", 200, "image/png", png_bytes()),
        ("/redir", 302, "text/plain", Vec::new()),
        ("/wrongtype", 200, "text/html", png_bytes()),
        ("/lying", 200, "image/png", b"this is not an image".to_vec()),
        ("/big", 200, "application/octet-stream", vec![7u8; 4096]),
    ]);

    let direct = fetch_url(&format!("{base}/img.png")).expect("直取应成功");
    assert_eq!(direct.bytes, png_bytes());
    assert_eq!(direct.detected_format, Some("png"));

    let redirected = fetch_url(&format!("{base}/redir")).expect("一跳重定向应跟随");
    assert_eq!(redirected.bytes, png_bytes());
    assert!(
        redirected.final_url.ends_with("/img.png"),
        "应报最终 URL：{}",
        redirected.final_url
    );

    let wrong_type = fetch_url(&format!("{base}/wrongtype")).unwrap_err();
    assert!(
        format!("{wrong_type}").contains("（类型）"),
        "Content-Type 错应归类型类：{wrong_type}"
    );
    assert!(
        format!("{wrong_type}").contains("text/html"),
        "报错应点名实际类型：{wrong_type}"
    );

    let lying = fetch_url(&format!("{base}/lying")).unwrap_err();
    assert!(
        format!("{lying}").contains("magic"),
        "自称图片实为文本应被 magic 拦：{lying}"
    );

    let missing = fetch_url(&format!("{base}/nope")).unwrap_err();
    assert!(
        format!("{missing}").contains("404"),
        "非 2xx 要带状态码：{missing}"
    );

    let oversized = fetch_url_with(
        &format!("{base}/big"),
        1024,
        std::time::Duration::from_secs(2),
        std::time::Duration::from_secs(5),
    )
    .unwrap_err();
    assert_eq!(oversized.code, yanshi_core::ErrorCode::ResourceExhausted);
    assert!(
        format!("{oversized}").contains("超过上限"),
        "超限要说清：{oversized}"
    );

    let timeout = fetch_url_with(
        &format!("{base}/blackhole"),
        1024,
        std::time::Duration::from_millis(200),
        std::time::Duration::from_millis(800),
    )
    .unwrap_err();
    assert!(
        format!("{timeout}").contains("超时"),
        "黑洞服务器应归超时类：{timeout}"
    );
}

// ───────────────────────── import_asset：第三条通道 ─────────────────────────

#[test]
fn import_asset_accepts_base64_and_rejects_mixed_sources() {
    let root = temp_dir("asset_b64");
    let _cleanup_3 = Cleanup(root.clone());
    let mut ws = workspace(&root);

    let by_b64 = call(
        &mut ws,
        "import_asset",
        json!({ "kind": "texture", "name": "tex_b64.png", "data_base64": PNG_B64 }),
    );
    assert_eq!(by_b64["ok"], json!(true), "base64 通道应可导入：{by_b64}");
    assert_eq!(
        by_b64["bytes"],
        json!(png_bytes().len()),
        "导入字节数应与源一致：{by_b64}"
    );

    // 同一份字节再走 blob 通道 ⇒ 两条记录的字节数必须相等（恒等式）。
    let put = call(&mut ws, "put_blob", json!({ "data_base64": PNG_B64 }));
    let by_blob = call(
        &mut ws,
        "import_asset",
        json!({
            "kind": "texture",
            "name": "tex_blob.png",
            "blob": { "blob_hash": put["blob_hash"], "size": png_bytes().len(), "mime_type": "image/png" }
        }),
    );
    assert_eq!(by_blob["ok"], json!(true), "blob 通道应可导入：{by_blob}");
    assert_eq!(
        by_blob["bytes"], by_b64["bytes"],
        "两通道导入的字节数不一致"
    );

    let mixed = call(
        &mut ws,
        "import_asset",
        json!({ "kind": "texture", "name": "tex_mixed.png", "path": "x.png", "data_base64": PNG_B64 }),
    );
    assert_eq!(
        mixed["ok"],
        json!(false),
        "path 与 data_base64 同给必须互斥报错：{mixed}"
    );
    assert!(
        detail(&mixed).contains("互斥"),
        "报错要说清是互斥：{}",
        detail(&mixed)
    );
}

// ───────────────────────── 出参侧：落盘与内嵌逐字节一致 ─────────────────────────

#[test]
fn render_region_output_path_matches_the_inline_bytes() {
    let root = temp_dir("region_out");
    let _cleanup_4 = Cleanup(root.clone());
    let mut ws = workspace(&root);
    let exports = exports_root();

    let inline = call(
        &mut ws,
        "render_region",
        json!({ "region": { "x": 0, "y": 0, "w": 16, "h": 16 }, "include_image": true, "max_px": 512 }),
    );
    assert_eq!(inline["ok"], json!(true), "内嵌形态应成功：{inline}");
    let inline_png = yanshi_server::base64::decode(inline["image"]["data"].as_str().unwrap())
        .expect("内嵌 base64 必须可解");

    let filed = call(
        &mut ws,
        "render_region",
        json!({ "region": { "x": 0, "y": 0, "w": 16, "h": 16 }, "output_path": "region.png" }),
    );
    assert_eq!(filed["ok"], json!(true), "落盘形态应成功：{filed}");
    assert!(
        filed.get("image").is_none(),
        "落盘形态不该再内嵌 base64：{filed}"
    );
    let meta = &filed["image_file"];
    assert_eq!(
        meta["bytes"],
        json!(inline_png.len()),
        "元数据字节数应与内嵌一致：{meta}"
    );
    let on_disk = std::fs::read(exports.join("region.png")).expect("文件应已落盘");
    // **恒等式**：落盘字节 == 内嵌字节（同一预览两种去向，不许各编一份）。
    assert_eq!(on_disk, inline_png, "落盘 PNG 与内嵌 PNG 逐字节不一致");

    let raw_conflict = call(
        &mut ws,
        "render_region",
        json!({ "region": { "x": 0, "y": 0, "w": 16, "h": 16 }, "raw": true, "output_path": "region2.png" }),
    );
    assert_eq!(
        raw_conflict["ok"],
        json!(false),
        "raw 与 output_path 同给必须报错（不许静默忽略）：{raw_conflict}"
    );
    assert!(
        detail(&raw_conflict).contains("raw"),
        "报错要点名 raw：{}",
        detail(&raw_conflict)
    );
}

#[test]
fn brush_preview_output_path_writes_the_same_png() {
    let mut ws = brush_workspace();
    let exports = exports_root();

    let inline = call(
        &mut ws,
        "brush_preview",
        json!({ "brush": "2B_pencil", "size": 24, "include_image": true }),
    );
    assert_eq!(inline["ok"], json!(true), "内嵌形态应成功：{inline}");
    let inline_png = yanshi_server::base64::decode(inline["image"]["data"].as_str().unwrap())
        .expect("内嵌 base64 必须可解");

    let filed = call(
        &mut ws,
        "brush_preview",
        json!({ "brush": "2B_pencil", "size": 24, "output_path": "preview.png" }),
    );
    assert_eq!(filed["ok"], json!(true), "落盘形态应成功：{filed}");
    assert!(filed.get("image").is_none(), "落盘形态不该再内嵌：{filed}");
    let on_disk = std::fs::read(exports.join("preview.png")).expect("文件应已落盘");
    assert_eq!(on_disk, inline_png, "落盘 PNG 与内嵌 PNG 逐字节不一致");
    assert_eq!(filed["image_file"]["width"], filed["width"]);
    assert_eq!(filed["image_file"]["height"], filed["height"]);
}

#[test]
fn try_stroke_output_path_writes_a_png() {
    let mut ws = brush_workspace();
    let exports = exports_root();

    let layers = call(&mut ws, "list_layers", json!({}));
    let layer_id = layers["layers"][0]["layer_id"]
        .as_str()
        .expect("新文档应有图层")
        .to_owned();
    let filed = call(
        &mut ws,
        "try_stroke",
        json!({
            "layer_id": layer_id,
            "brush": "2B_pencil",
            "points": [[4.0, 4.0, 0.8], [20.0, 20.0, 0.8]],
            "output_path": "try.png"
        }),
    );
    assert_eq!(filed["ok"], json!(true), "落盘形态应成功：{filed}");
    assert!(filed.get("image").is_none(), "落盘形态不该再内嵌：{filed}");
    let on_disk = std::fs::read(exports.join("try.png")).expect("文件应已落盘");
    assert_eq!(&on_disk[..8], b"\x89PNG\r\n\x1a\n", "落盘的应是真 PNG");
    assert_eq!(filed["image_file"]["bytes"], json!(on_disk.len()));
}

#[test]
fn swatch_output_path_writes_one_file_per_swatch() {
    let mut ws = brush_workspace();
    let exports = exports_root();

    let filed = call(
        &mut ws,
        "swatch_brush_strokes",
        json!({ "brushes": ["2B_pencil"], "output_path": "swatches" }),
    );
    assert_eq!(filed["ok"], json!(true), "落盘形态应成功：{filed}");
    let first = &filed["swatches"][0];
    assert_eq!(first["ok"], json!(true), "第一支试笔应成功：{first}");
    assert!(first["image"].is_null(), "文件形态不该内嵌 base64：{first}");
    let meta = &first["image_file"];
    assert!(
        meta["path"].as_str().unwrap().ends_with("swatch_0.png"),
        "文件名应带序号：{meta}"
    );
    let on_disk =
        std::fs::read(exports.join("swatches").join("swatch_0.png")).expect("文件应已落盘");
    assert_eq!(&on_disk[..8], b"\x89PNG\r\n\x1a\n", "落盘的应是真 PNG");
    assert_eq!(meta["bytes"], json!(on_disk.len()));
}

#[test]
fn export_psd_layers_output_path_writes_the_same_bytes_as_the_blob() {
    let root = temp_dir("psd_out");
    let _cleanup_5 = Cleanup(root.clone());
    let mut ws = workspace(&root);
    let exports = exports_root();

    let legacy = call(&mut ws, "export_psd_layers", json!({}));
    assert_eq!(legacy["ok"], json!(true), "旧形态应成功：{legacy}");
    assert!(
        legacy.get("path").is_none(),
        "不给 output_path 时不许长 path 字段：{legacy}"
    );

    let filed = call(
        &mut ws,
        "export_psd_layers",
        json!({ "output_path": "layers.psd" }),
    );
    assert_eq!(filed["ok"], json!(true), "落盘形态应成功：{filed}");
    assert_eq!(
        filed["blob_hash"], legacy["blob_hash"],
        "同一文档两次导出的 blob 应一致"
    );
    assert_eq!(filed["mime_type"], json!("image/vnd.adobe.photoshop"));
    let on_disk = std::fs::read(exports.join("layers.psd")).expect("文件应已落盘");
    assert_eq!(
        on_disk.len() as u64,
        filed["bytes"].as_u64().unwrap(),
        "文件大小应与元数据一致"
    );
    assert_eq!(&on_disk[..4], b"8BPS", "落盘的应是真 PSD");
}
