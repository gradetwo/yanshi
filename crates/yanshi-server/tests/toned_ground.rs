//! **一键有机底色 `toned_ground`**（反馈报告 §5.3）。
//!
//! **判据用真实像素** ✓：导出 PNG ⇒ 用本仓库自己的解码器读回来 ⇒ 算均色与标准差 ✓
//! （**不**只看工具回没回 `ok` ✓ —— 那是自证 ✓）。
//! 每个 case 用**独立文档** ✓（AGENTS.md 第 916 轮教训：共用文档会让前一段的墨污染后一段 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_toned_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_t", width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_t", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **导出整幅并解码回来** ✓（用本仓库的解码器 ✓ ⇒ 测的是真像素 ✓）。
fn pixels(workspace: &mut Workspace, path: &std::path::Path, width: u32, height: u32) -> Vec<u8> {
    let exported = call(
        workspace,
        "export_png",
        json!({ "path": path.display().to_string() }),
    );
    assert_eq!(exported["ok"], json!(true), "导出应当成功：{exported}");
    let bytes = std::fs::read(path).expect("导出的 PNG 应当存在");
    let (got_width, got_height, rgba) =
        yanshi_render::png::decode_png(&bytes).expect("自己导出的 PNG 应当自己解得了");
    assert_eq!(
        (got_width, got_height),
        (width, height),
        "导出尺寸应当是整幅"
    );
    rgba
}

fn pixel(rgba: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let at = (y * width + x) * 4;
    [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
}

/// **均色与标准差** ✓（按通道算 ⇒ 纹理判据不断单点像素 ✓）。
fn mean_and_stddev(rgba: &[u8]) -> ([f64; 3], [f64; 3]) {
    let count = (rgba.len() / 4) as f64;
    let mut mean = [0.0; 3];
    for chunk in rgba.chunks_exact(4) {
        for channel in 0..3 {
            mean[channel] += f64::from(chunk[channel]);
        }
    }
    for channel in 0..3 {
        mean[channel] /= count;
    }
    let mut variance = [0.0; 3];
    for chunk in rgba.chunks_exact(4) {
        for channel in 0..3 {
            let diff = f64::from(chunk[channel]) - mean[channel];
            variance[channel] += diff * diff;
        }
    }
    let mut stddev = [0.0; 3];
    for channel in 0..3 {
        stddev[channel] = (variance[channel] / count).sqrt();
    }
    (mean, stddev)
}

/// **底色层在最底、盖满画布、均色贴住请求色、标准差显著高于平涂** ✓。
#[test]
fn ground_goes_to_the_bottom_and_covers_the_canvas_with_texture() {
    let root = temp_dir("bottom");
    let _cleanup_1 = Cleanup(root.clone());
    let mut workspace = workspace(&root, 160, 100);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "art" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "toned_ground",
        json!({ "color": { "r": 210, "g": 190, "b": 160, "a": 255 } }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(made["brush"], json!("Classic_Paint"), "{made}");
    assert!(
        made["strokes"].as_u64().unwrap_or(0) > 0,
        "至少要落一笔：{made}"
    );
    let created = made["created_layer"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!created.is_empty(), "缺省应当新建一个底色图层：{made}");
    let listed = call(&mut workspace, "list_layers", json!({}));
    let order: Vec<String> = listed["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|layer| layer["layer_id"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert_eq!(
        order.first().map(String::as_str),
        Some(created.as_str()),
        "底色层必须在最底：{order:?}"
    );
    assert!(
        order.contains(&"art".to_string()),
        "原来的图层还得在：{order:?}"
    );
    let rgba = pixels(&mut workspace, &root.join("g.png"), 160, 100);
    let (mean, stddev) = mean_and_stddev(&rgba);
    let request = [210.0, 190.0, 160.0];
    for channel in 0..3 {
        assert!(
            (mean[channel] - request[channel]).abs() <= 15.0,
            "均色应当贴住请求色（通道 {channel}：期望 {request:?}，实测 {mean:?}）"
        );
        assert!(
            stddev[channel] > 4.0,
            "标准差应当显著高于平涂（通道 {channel}：实测 {stddev:?}，平涂 ≈ 0）"
        );
    }
    // **盖满** ✓：纯白（文档缺省底）残留不得超过 1% ✓（底色不透明 ⇒ 露白即没盖满 ✗）。
    let white = rgba
        .chunks_exact(4)
        .filter(|chunk| chunk[0] == 255 && chunk[1] == 255 && chunk[2] == 255)
        .count();
    assert!(
        white * 100 < rgba.len() / 4,
        "露白像素太多（{white}/{}）⇒ 没盖满画布",
        rgba.len() / 4
    );
}

/// **重复调用幂等** ✓：第二次再生本层 ⇒ 导出逐位一致、均色不更暗、底色层仍只有一个 ✓。
#[test]
fn second_call_regenerates_without_darkening() {
    let root = temp_dir("idem");
    let _cleanup_2 = Cleanup(root.clone());
    let mut workspace = workspace(&root, 160, 100);
    let first = call(
        &mut workspace,
        "toned_ground",
        json!({ "color": { "r": 210, "g": 190, "b": 160, "a": 255 } }),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(
        first["replaced"],
        json!(false),
        "第一次没有旧层可替：{first}"
    );
    let first_pixels = pixels(&mut workspace, &root.join("first.png"), 160, 100);
    let (first_mean, _) = mean_and_stddev(&first_pixels);
    let second = call(
        &mut workspace,
        "toned_ground",
        json!({ "color": { "r": 210, "g": 190, "b": 160, "a": 255 } }),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        second["replaced"],
        json!(true),
        "第二次应当替掉旧层：{second}"
    );
    let second_pixels = pixels(&mut workspace, &root.join("second.png"), 160, 100);
    assert_eq!(
        first_pixels, second_pixels,
        "同参数调两次 ⇒ 导出应当逐位一致（否则就是叠加或随机漂移）"
    );
    let (second_mean, _) = mean_and_stddev(&second_pixels);
    for channel in 0..3 {
        assert!(
            second_mean[channel] >= first_mean[channel] - 2.0,
            "第二次均色不得更暗（通道 {channel}：第一次 {first_mean:?}，第二次 {second_mean:?}）"
        );
    }
    let listed = call(&mut workspace, "list_layers", json!({}));
    let grounds = listed["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|layer| layer["name"].as_str() == Some("底色（笔触）"))
        .count();
    assert_eq!(
        grounds, 1,
        "底色层应当只有一个（重复调用不得再起一层）：{listed}"
    );
}

/// **已有画作在上不受影响** ✓：先画红笔，再铺底色 ⇒ 红笔仍在上、底色不盖它 ✓。
#[test]
fn artwork_above_the_ground_is_untouched() {
    let root = temp_dir("artwork");
    let _cleanup_3 = Cleanup(root.clone());
    let mut workspace = workspace(&root, 160, 100);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "art" }))["ok"],
        json!(true)
    );
    let painted = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "art", "brush": "100%_Opaque", "size": 24,
            "points": [[40.0, 50.0, 0.9], [120.0, 50.0, 0.9]],
            "color": { "r": 255, "g": 0, "b": 0, "a": 255 },
        }),
    );
    assert_eq!(painted["ok"], json!(true), "{painted}");
    let made = call(
        &mut workspace,
        "toned_ground",
        json!({ "color": { "r": 210, "g": 190, "b": 160, "a": 255 } }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let listed = call(&mut workspace, "list_layers", json!({}));
    let order: Vec<String> = listed["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|layer| layer["layer_id"].as_str().unwrap_or_default().to_owned())
        .collect();
    let ground_pos = order
        .iter()
        .position(|id| id == made["created_layer"].as_str().unwrap_or_default())
        .unwrap_or(usize::MAX);
    let art_pos = order
        .iter()
        .position(|id| id == "art")
        .unwrap_or(usize::MAX);
    assert!(ground_pos < art_pos, "底色层必须在画作层之下：{order:?}");
    let rgba = pixels(&mut workspace, &root.join("a.png"), 160, 100);
    let on_art = pixel(&rgba, 160, 80, 50);
    assert!(
        on_art[0] > 200 && on_art[1] < 80 && on_art[2] < 80,
        "画作处的红笔不得被底色盖掉（实测 {on_art:?}）"
    );
    let on_ground = pixel(&rgba, 160, 10, 90);
    assert!(
        (f64::from(on_ground[0]) - 210.0).abs() <= 30.0
            && (f64::from(on_ground[1]) - 190.0).abs() <= 30.0
            && (f64::from(on_ground[2]) - 160.0).abs() <= 30.0,
        "远离画作处应当是底色（实测 {on_ground:?}）"
    );
}

/// **非法颜色大声报错、不静默铺底** ✓：不传、缺通道、越界、非整数、非对象 ⇒
/// 全部 `ok:false` + `invalid_argument`，且事后没有底色层 ✓。
#[test]
fn missing_or_invalid_color_is_a_loud_error() {
    let root = temp_dir("badcolor");
    let _cleanup_4 = Cleanup(root.clone());
    let mut workspace = workspace(&root, 160, 100);
    let bad_calls = [
        json!({}),
        json!({ "color": { "g": 190, "b": 160, "a": 255 } }),
        json!({ "color": { "r": 300, "g": 190, "b": 160, "a": 255 } }),
        json!({ "color": { "r": -5, "g": 190, "b": 160, "a": 255 } }),
        json!({ "color": { "r": 200.5, "g": 190, "b": 160, "a": 255 } }),
        json!({ "color": "#zzzzzz" }),
        json!({ "color": { "r": 210, "g": 190, "b": 160, "a": 255 }, "strength": 2.0 }),
    ];
    for args in bad_calls {
        let got = call(&mut workspace, "toned_ground", args.clone());
        assert_eq!(got["ok"], json!(false), "非法调用应当报错：{args} ⇒ {got}");
        assert_eq!(
            got["error_code"],
            json!("invalid_argument"),
            "非法调用应当是 invalid_argument：{args} ⇒ {got}"
        );
        let detail = got["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("color") || detail.contains("strength"),
            "错误信息应当说清哪个参数非法：{args} ⇒ {detail}"
        );
    }
    let listed = call(&mut workspace, "list_layers", json!({}));
    let grounds = listed["layers"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|layer| layer["name"].as_str() == Some("底色（笔触）"))
        .count();
    assert_eq!(grounds, 0, "非法调用之后不得留下底色层：{listed}");
}
