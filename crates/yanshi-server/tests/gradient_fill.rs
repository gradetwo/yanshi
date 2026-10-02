//! **渐变填充** ✓（目标 (d) ✓ —— 针对用户报过的"大面积背景难处理" ✓）。
//!
//! **为什么要它** ✓：他实测到的背景毛病是笔刷铺出来的**笔触边缘**（"横向条带" ✓）与
//! **采样噪声**（"盖章圆点" ✓）；而渐变是**纯函数** ✓ —— 没有随机、没有重叠 ✗。
//! **判据用真实像素** ✓：导出 PNG ⇒ 用本仓库自己的解码器读回来 ⇒ 比对角落与中心 ✓
//!（**不**只看工具回没回 `ok` ✓ —— 那是自证 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_grad_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, width: u32, height: u32) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_g", width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_g", "human:1", "session:test");
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

/// **宽容比较** ✓：用在**采样点**而不是理想端点的地方（径向的中心正好落在采样点上 ✓）。
fn close(a: [u8; 4], b: [u8; 4], tolerance: u8, what: &str) {
    let near = a
        .iter()
        .zip(b.iter())
        .all(|(left, right)| left.abs_diff(*right) <= tolerance);
    assert!(near, "{what}：期望 ≈{b:?}，实测 {a:?}（容差 {tolerance}）");
}

/// **0° 线性渐变：左端是起点色、右端是终点色、中间在两者之间** ✓。
#[test]
fn a_zero_degree_linear_gradient_runs_left_to_right() {
    let root = temp_dir("linear0");
    let mut workspace = workspace(&root, 100, 20);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "gradient_fill",
        json!({
            "layer_id": "L", "kind": "linear", "angle": 0,
            "from": { "r": 255, "g": 0, "b": 0, "a": 255 },
            "to": { "r": 0, "g": 0, "b": 255, "a": 255 }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let rgba = pixels(&mut workspace, &root.join("g.png"), 100, 20);
    // **不要咬死精确值** ✗（我第一版就是这么写的 ✓ ⇒ 抓到的是 3 而不是 0 ✓）：
    // 渐变的**最后一个像素**落在 `t = 99/100` ✓（跨度是整个区域 ✓）⇒ 本来就到不了纯终点色 ✓。
    // **这不是 bug，是采样** ✓ ⇒ 判据应当说"**接近端点 + 中间单调**" ✓
    //（这也正是"大面积背景"真正需要保证的性质 ✓：没有条带、没有噪声 ✓）。
    let left = pixel(&rgba, 100, 0, 10);
    let right = pixel(&rgba, 100, 99, 10);
    assert!(
        left[0] > 245 && left[2] < 10,
        "左端应当接近起点色（实测 {left:?}）"
    );
    assert!(
        right[2] > 245 && right[0] < 10,
        "右端应当接近终点色（实测 {right:?}）"
    );
    let middle = pixel(&rgba, 100, 50, 10);
    assert!(
        middle[0] > 100 && middle[0] < 155 && middle[2] > 100 && middle[2] < 155,
        "中点应当是两者之间（实测 {middle:?}）"
    );
    // **同一行左右应当单调** ✓（"渐变"的定义 ✓；若有重叠或噪声就会破坏它 ✓）。
    let left = pixel(&rgba, 100, 10, 10)[0];
    let right = pixel(&rgba, 100, 90, 10)[0];
    assert!(left > right, "红色分量应当自左向右递减（{left} → {right}）");
}

/// **90° 就是"从上到下"** ✓（按屏幕坐标量角度 ✓）。
#[test]
fn ninety_degrees_runs_top_to_bottom() {
    let root = temp_dir("linear90");
    let mut workspace = workspace(&root, 20, 100);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    assert_eq!(
        call(
            &mut workspace,
            "gradient_fill",
            json!({
                "layer_id": "L", "kind": "linear", "angle": 90,
                "from": { "r": 255, "g": 255, "b": 255, "a": 255 },
                "to": { "r": 0, "g": 0, "b": 0, "a": 255 }
            }),
        )["ok"],
        json!(true)
    );
    let rgba = pixels(&mut workspace, &root.join("g.png"), 20, 100);
    // 同上：端点附近就已经够 ✓（`t = 99/100` ⇒ 3 而不是 0 ✓）。
    let top = pixel(&rgba, 20, 10, 0);
    let bottom = pixel(&rgba, 20, 10, 99);
    assert!(top[0] > 245, "顶端应当接近起点色（实测 {top:?}）");
    assert!(bottom[0] < 10, "底端应当接近终点色（实测 {bottom:?}）");
}

/// **径向渐变：中心是起点色、远处趋向终点色** ✓。
#[test]
fn a_radial_gradient_starts_at_its_centre() {
    let root = temp_dir("radial");
    let mut workspace = workspace(&root, 80, 80);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    assert_eq!(
        call(
            &mut workspace,
            "gradient_fill",
            json!({
                "layer_id": "L", "kind": "radial", "radius": 40,
                "from": { "r": 255, "g": 255, "b": 0, "a": 255 },
                "to": { "r": 0, "g": 0, "b": 0, "a": 255 }
            }),
        )["ok"],
        json!(true)
    );
    let rgba = pixels(&mut workspace, &root.join("g.png"), 80, 80);
    close(
        pixel(&rgba, 80, 40, 40),
        [255, 255, 0, 255],
        3,
        "中心应当是起点色",
    );
    // **半径之外夹到终点色** ✓（角落离中心 ≈56 > 40 ✓）。
    close(
        pixel(&rgba, 80, 0, 0),
        [0, 0, 0, 255],
        3,
        "半径之外应当是终点色",
    );
}

/// **同一件事两次 ⇒ 逐像素相同** ✓（确定性 ✓ —— 这正是渐变比"笔刷铺底"强的地方 ✓）。
#[test]
fn the_same_gradient_is_identical_twice() {
    let root = temp_dir("determinism");
    let mut workspace = workspace(&root, 60, 60);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "a" }))["ok"],
        json!(true)
    );
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "b" }))["ok"],
        json!(true)
    );
    let args = |layer: &str| {
        json!({
            "layer_id": layer, "kind": "linear", "angle": 33,
            "from": { "r": 12, "g": 200, "b": 90, "a": 255 },
            "to": { "r": 240, "g": 30, "b": 170, "a": 255 }
        })
    };
    let first = call(&mut workspace, "gradient_fill", args("a"));
    let second = call(&mut workspace, "gradient_fill", args("b"));
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(second["ok"], json!(true), "{second}");
    let left = first["preview"]["blob_hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let right = second["preview"]["blob_hash"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    // **同一份输入 ⇒ 同一个 blob 哈希** ✓（内容寻址存储 ✓ ⇒ 哈希相同就是逐字节相同 ✓）。
    assert!(!left.is_empty(), "{first}");
    assert_eq!(left, right, "同样的渐变两次应当产出完全相同的像素");
}

/// **区域版** ✓：只填那一块 ✓，而且区域**超出画布要夹住** ✓。
#[test]
fn a_region_fills_only_that_part() {
    let root = temp_dir("region");
    let mut workspace = workspace(&root, 100, 60);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "gradient_fill",
        json!({
            "layer_id": "L", "kind": "linear", "angle": 0,
            "region": { "x": 20, "y": 10, "w": 40, "h": 20 },
            "from": { "r": 255, "g": 0, "b": 0, "a": 255 },
            "to": { "r": 0, "g": 255, "b": 0, "a": 255 }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    assert_eq!(
        made["region"],
        json!({ "x": 20, "y": 10, "w": 40, "h": 20 }),
        "{made}"
    );
    let rgba = pixels(&mut workspace, &root.join("g.png"), 100, 60);
    // **区域内**有颜色 ✓；**区域外**保持透明 ✓（只有那一块被填 ✓）。
    let inside = pixel(&rgba, 100, 20, 10);
    assert!(inside[0] > 200, "区域左端应当是起点色（实测 {inside:?}）");
    // **区域外是"没被这一笔碰过"** ✓ —— 而画布的默认底色是**白** ✓，
    // 所以我第一版断言"透明"是错的 ✗（实测 `[255,255,255,255]` ✓ ⇒ 那正是没被碰过的样子 ✓）。
    // **判据应当是"与区域内的颜色不同"** ✓：只要没被渐变染上就对了 ✓。
    let outside = pixel(&rgba, 100, 90, 50);
    let inside_right = pixel(&rgba, 100, 59, 10);
    assert_ne!(
        [outside[0], outside[1], outside[2]],
        [inside_right[0], inside_right[1], inside_right[2]],
        "区域之外不该被渐变染上（外 {outside:?} 内 {inside_right:?}）"
    );
    assert!(
        outside[0] > 240 && outside[1] > 240 && outside[2] > 240,
        "区域外应当是没被碰过的画布底色（实测 {outside:?}）"
    );
}

/// **未知类型要列出可用值** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn an_unknown_kind_lists_the_supported_ones() {
    let root = temp_dir("badkind");
    let mut workspace = workspace(&root, 20, 20);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let got = call(
        &mut workspace,
        "gradient_fill",
        json!({
            "layer_id": "L", "kind": "conic",
            "from": { "r": 0, "g": 0, "b": 0, "a": 255 },
            "to": { "r": 255, "g": 255, "b": 255, "a": 255 }
        }),
    );
    assert_eq!(got["error_code"], json!("invalid_argument"), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("linear") && detail.contains("radial"),
        "要列出可用类型：{detail}"
    );
}

/// **单图层导出：只导出指定那一层** ✓（用户报过的缺口 ✓）。
///
/// **判据不是"导出成功了"** ✗（那太弱 ✓）⇒ 而是：**另一层的颜色绝不出现在结果里** ✓。
/// **为什么值得专门测** ✓：我为此在渲染器上加了"只画这一层"的开关 ✓，
/// 而**区域字节缓存的键里没有图层** ✗ ⇒ 一旦误用缓存 ✓，
/// 就会把**别的图层**的像素当成这一层的返回 ✓ —— 这类 bug **只在缓存命中时**发作 ✓（最难查 ✓）。
#[test]
fn exporting_one_layer_leaves_the_other_layer_out() {
    let root = temp_dir("one_layer");
    let mut workspace = workspace(&root, 60, 40);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "red" }))["ok"],
        json!(true)
    );
    assert_eq!(
        call(
            &mut workspace,
            "create_layer",
            json!({ "layer_id": "green" })
        )["ok"],
        json!(true)
    );
    // 两层各画一条**实心**矩形 ✓（颜色差别要大 ✓，好判 ✓）。
    assert_eq!(
        call(
            &mut workspace,
            "gradient_fill",
            json!({
                "layer_id": "red", "kind": "linear", "angle": 0,
                "from": { "r": 255, "g": 0, "b": 0, "a": 255 },
                "to": { "r": 255, "g": 0, "b": 0, "a": 255 }
            }),
        )["ok"],
        json!(true)
    );
    assert_eq!(
        call(
            &mut workspace,
            "gradient_fill",
            json!({
                "layer_id": "green", "kind": "linear", "angle": 0,
                "from": { "r": 0, "g": 255, "b": 0, "a": 255 },
                "to": { "r": 0, "g": 255, "b": 0, "a": 255 }
            }),
        )["ok"],
        json!(true)
    );
    // **先整幅导出一次** ✓ ⇒ 让区域缓存**先填满** ✓（不这么做就测不到"缓存串层" ✗）。
    let both = root.join("both.png");
    assert_eq!(
        call(
            &mut workspace,
            "export_png",
            json!({ "path": both.display().to_string() })
        )["ok"],
        json!(true)
    );
    // **再只导红色那层** ✓。
    let only_red = root.join("red.png");
    let got = call(
        &mut workspace,
        "export_png",
        json!({ "path": only_red.display().to_string(), "layer_id": "red" }),
    );
    assert_eq!(got["ok"], json!(true), "{got}");
    let (_, _, rgba) = yanshi_render::png::decode_png(&std::fs::read(&only_red).unwrap()).unwrap();
    let mut red = 0usize;
    let mut green = 0usize;
    for index in (0..rgba.len()).step_by(4) {
        let (r, g, b) = (rgba[index], rgba[index + 1], rgba[index + 2]);
        if r > 200 && g < 60 && b < 60 {
            red += 1;
        }
        if g > 200 && r < 60 && b < 60 {
            green += 1;
        }
    }
    assert!(red > 0, "导出的那层应当在里面（红色像素 {red} ✓）");
    assert_eq!(
        green, 0,
        "**另一层的颜色绝不能出现** ✗（绿色像素 {green} ⇒ 说明渲染或缓存把别的图层也带进来了 ✓）"
    );
}
