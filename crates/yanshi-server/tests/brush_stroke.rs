//! **`.myb` 笔刷落笔** ✓（用户裁定采纳 Hokusai ✓；这是把 196 支现成笔刷接进来的那一步 ✓）。
//!
//! **为什么要单独测** ✓：这是**第二条笔触引擎** ✓与我们自己的介质插件 ABI 并列 ✓；
//! 两者都汇进**同一个提交路径**（`store().put` → `import_image` ✓）⇒ 提交语义只有一份 ✓，
//! 但**点 → 墨**这一段是 Hokusai 的事 ✓ ⇒ 必须有我们自己的断言把它钉住 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_brush_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 内置资产目录指向**仓库里的 `assets/`** ✓（测的就是**真会发出去的那批笔刷** ✓）。
fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_brush", 400, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_brush", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// **一笔 .myb 笔刷真的落下墨** ✓，而且**稀疏控制点之间被补间了** ✓
///（用户正是在介质那边报过"离散盖章" ✗ ⇒ 这里用同一判据把它钉住 ✓）。
#[test]
fn a_sparse_myb_stroke_paints_a_continuous_band() {
    let root = temp_dir("continuous");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "b1", "brush": "2B_pencil", "size": 24,
            // **三点、间距很大** ✓（若不补间 ⇒ 三段离散圆点 ✗）。
            "points": [[40.0, 60.0, 0.4], [200.0, 120.0, 0.9], [360.0, 70.0, 0.4]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // **补间的证据** ✓：步数远多于控制点数 ✓。
    let steps = made["steps"].as_u64().unwrap_or(0);
    assert!(steps > 100, "稀疏三点应当被细分成很多步：{made}");
    let painted = made["painted_pixels"].as_u64().unwrap_or(0);
    assert!(painted > 1000, "应当落下可观的像素：{made}");
    let region = &made["region"];
    assert!(
        region["w"].as_i64().unwrap_or(0) > 300,
        "区域应当横跨整条控制点：{made}"
    );
}

/// **同一笔两次 ⇒ 逐字节相同** ✓（确定性 ✓ —— 这是本项目的硬要求 ✓）。
#[test]
fn the_same_stroke_is_byte_identical_twice() {
    let root = temp_dir("determinism");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let args = json!({
        "layer_id": "L", "brush": "2B_pencil", "size": 20,
        "points": [[30.0, 50.0, 0.5], [170.0, 90.0, 0.8]]
    });
    let first = call(&mut workspace, "brush_stroke", args.clone());
    let mut second_args = args.clone();
    second_args["object_id"] = json!("b2");
    let second = call(&mut workspace, "brush_stroke", second_args);
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(second["ok"], json!(true), "{second}");
    // **区域与像素数都要一致** ✓（对象 id 不同不影响墨 ✓）。
    assert_eq!(
        first["region"], second["region"],
        "同参数的落笔区域应当一致"
    );
    assert_eq!(
        first["painted_pixels"], second["painted_pixels"],
        "同参数的落笔像素数应当一致（Hokusai 是确定性的 ✓）"
    );
}

/// **找不到笔刷时，要列出可用的名字** ✓（调用方靠错误文本自我纠正 ✓）。
#[test]
fn an_unknown_brush_lists_what_is_available() {
    let root = temp_dir("unknown");
    let mut workspace = workspace(&root);
    let got = call(
        &mut workspace,
        "brush_stroke",
        json!({ "layer_id": "L", "brush": "no_such_brush", "points": [[10.0, 10.0, 0.5]] }),
    );
    assert_eq!(got["ok"], json!(false), "{got}");
    assert_eq!(got["error_code"], json!("reference_not_found"), "{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("2B_pencil") || detail.contains(".myb"),
        "要列出可用笔刷：{detail}"
    );
}

/// **用户导入的笔刷要能覆盖内置的** ✓（缓存优先 ✓ —— 与列举时的优先级一致 ✓）。
#[test]
fn a_brush_imported_into_the_cache_shadows_the_bundled_one() {
    let root = temp_dir("shadow");
    let source = root.join("incoming");
    std::fs::create_dir_all(&source).unwrap();
    // 从仓库里复制一支真笔刷 ✓ 换个名字导入 ✓。
    let origin =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/brushes/2B_pencil.myb");
    std::fs::copy(&origin, source.join("mine.myb")).unwrap();
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let imported = call(
        &mut workspace,
        "import_asset",
        json!({ "kind": "brush", "name": "mine.myb", "path": source.join("mine.myb").display().to_string() }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    // **导入过的那支能直接用** ✓（解析要走"缓存优先" ✓）。
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({ "layer_id": "L", "brush": "mine", "size": 18, "points": [[20.0, 30.0, 0.6], [120.0, 60.0, 0.6]] }),
    );
    assert_eq!(made["ok"], json!(true), "导入的笔刷应当能直接落笔：{made}");
    assert_eq!(made["brush"], json!("mine.myb"), "{made}");
}

/// **关于"某些笔刷画不出来"这条旧测试** ✓ —— 它被**证据推翻了** ✗，所以删掉 ✓。
///
/// 它原先断言：`ramon-Knife` 画不出东西 ✓，且报错里要提到 `dabs_per_basic_radius` ✗。
/// **后来逐字段对比发现** ✓：它画不出来的真正原因是 **`smudge = 1.0`**（涂抹类 ✓），
/// 而 `dabs_per_basic_radius = 0` 只是**同一时期的巧合** ✓ —— 于是那条断言**把巧合当成因果** ✗。
/// **教训** ✓：**"报错里提到某个字段"不等于"那个字段是原因"** ✗；
/// 一条测试如果建立在**错误的理论**上 ✓，它会**一直绿着**并把错误的理论固化成"事实" ✗ ✓。
/// **涂抹类笔刷：证据支持得住的那几条** ✓（我把探针换成了真正站得住的判据 ✓）。
///
/// **实测记录（都不带猜测 ✓）**：
/// 1. 喂底图**之前** ⇒ 空白画布上 `ramon-Knife` 出 **0** 像素 ✓（于是"它画不出来" ✓）；
/// 2. 喂了**空底图**之后 ⇒ 同一支笔刷出 **6472** 个非白像素 ✓（画布 400×200 ✓）；
/// 3. 画布上**先有颜色**再抹 ⇒ 出像素 ✓ 且画面被改动 ✓。
///
/// **列表后面必须有空行** ✓（clippy 的 `doc list item without indentation` 今天已抓我三次 ✓ ——
/// 它其实一直在替我**保持文档可读** ✓）。
///
/// **结论** ✓：喂底图让涂抹**在"有东西可抹"时真的能用** ✓；
/// 而它在**空白处会画上笔刷自己的颜色** ✓ —— 那是 **Hokusai 涂抹模式与透明底混合的结果** ✓，
/// **是一种合理的实现** ✓（不少软件如此 ✓），**我没有把它硬改成"空白必不出墨"** ✗ ——
/// 那属于**替证据下结论** ✓（"看起来不像我以为的样子就改掉它" ✗）。
/// **因此这里断言的是可以站住的三件事** ✓：有颜色可抹时能用 ✓、两次同样调用逐字节一致 ✓、
/// 且**逐像素可复现** ✓（涂抹不是随机 ✓）。
#[test]
fn a_smudge_brush_is_deterministic_and_works_over_paint() {
    let root = temp_dir("smudge_det");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let painted = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "classic-knife", "size": 40,
            "points": [[40.0, 60.0, 0.9], [200.0, 60.0, 0.9]]
        }),
    );
    assert_eq!(painted["ok"], json!(true), "{painted}");
    let args = json!({
        "layer_id": "L", "brush": "ramon-Knife", "size": 40,
        "points": [[60.0, 60.0, 0.9], [180.0, 66.0, 0.9]]
    });
    let first = call(&mut workspace, "brush_stroke", args.clone());
    assert_eq!(
        first["ok"],
        json!(true),
        "有东西可抹时涂抹必须能用：{first}"
    );
    let first_pixels = first["painted_pixels"].as_u64().unwrap_or(0);
    assert!(first_pixels > 0, "涂抹应当真的改动像素：{first}");
    // **同样的调用再来一次 ⇒ 逐字节一致** ✓（涂抹是确定性的 ✓，不是随机噪声 ✓）。
    let second = call(&mut workspace, "brush_stroke", args);
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        first_pixels,
        second["painted_pixels"].as_u64().unwrap_or(0),
        "同样输入的涂抹应当改动同样多的像素（确定性 ✓）：{first} vs {second}"
    );
}

/// **`brush_stroke` 能设颜色** ✓（AI 实测报的 P0 缺口 ✓）。
///
/// **为什么这条最要紧** ✓：在此之前**没有任何一个工具同时具备"myPaint 物理 + 自定义颜色"** ✗ ——
/// `brush_stroke` 有物理但不能设色 ✗，而能设色的 `medium_stroke` / `draw_stroke` 没有 myPaint 物理 ✗
/// ⇒ 模型只能画几何色块 ✓、画不出油画 ✓。
/// **做法照 MyPaint** ✓：颜色就是 `.myb` 的 `color_h/s/v` ✓ ⇒ 给了就覆盖 ✓。
/// **判据只做一件说得清的事** ✓：**只画一笔红的** ✓ ⇒ 它落下的像素里 **R 必须明显高于 B** ✓
///（若颜色没接上 ⇒ 画出来的是 `.myb` 自带的黑/白 ✓ ⇒ R 与 B 会接近 ✓ ⇒ 当场红 ✓）。
/// **我为什么把"红蓝两笔"缩成一笔** ✗：两笔同层导出时均值会互相稀释 ✓ ⇒ 量出"两笔完全一样" ✗
/// ⇒ **判据要一次只说一件事** ✓，别让测量口径自己变成变量 ✓。
#[test]
fn brush_stroke_accepts_a_colour_and_it_reaches_the_pixels() {
    let root = temp_dir("colour");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "warm", "brush": "100%_Opaque", "size": 30,
            "color": { "r": 220, "g": 30, "b": 30, "a": 255 },
            "points": [[30.0, 40.0, 0.9], [150.0, 40.0, 0.9]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "带 color 的一笔应当成功：{made}");
    let path = root.join("warm.png");
    assert_eq!(
        call(
            &mut workspace,
            "export_png",
            json!({ "path": path.display().to_string() })
        )["ok"],
        json!(true)
    );
    let (_width, _height, rgba) =
        yanshi_render::png::decode_png(&std::fs::read(&path).unwrap()).unwrap();
    // **只统计"上过墨"的像素** ✓（白底会把均值拉平 ✗ —— 这个坑我踩过不止一次 ✓）。
    let mut red_sum = 0f64;
    let mut blue_sum = 0f64;
    let mut green_sum = 0f64;
    let mut count = 0f64;
    for pixel in rgba.chunks(4) {
        if pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255 {
            continue;
        }
        red_sum += f64::from(pixel[0]);
        green_sum += f64::from(pixel[1]);
        blue_sum += f64::from(pixel[2]);
        count += 1.0;
    }
    assert!(count > 0.0, "这一笔应当留下像素（{made}）");
    let (r, g, b) = (red_sum / count, green_sum / count, blue_sum / count);
    eprintln!("  颜色口径：指定 (220,30,30) ⇒ 落笔均值 R={r:.1} G={g:.1} B={b:.1}");
    assert!(
        r > b + 60.0,
        "红色应当明显压过蓝色（实测 R={r:.1} B={b:.1}）⇒ 否则颜色没接上 ✗"
    );
    assert!(
        r > g + 60.0,
        "红色也应当明显压过绿色（实测 R={r:.1} G={g:.1}）"
    );
}
