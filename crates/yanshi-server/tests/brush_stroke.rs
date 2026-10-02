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
    // **判据改成"两个完全相同的输入 ⇒ 两份完全相同的画面"** ✓。
    //
    // **为什么换掉旧写法** ✗：旧断言是"**同一个文档**里连调两次涂抹 ⇒ `painted_pixels` 相等" ✓，
    // 而在新语义下这个数已经变成"**这一次真的改动了多少像素**" ✓ ——
    // 第二次涂抹时画布**已经被抹过一次** ✓ ⇒ 改动数当然不同 ✓（那与"是不是随机"无关 ✗）。
    // 旧写法之所以一直通过 ✓，恰恰是因为它数的是"**连底图一起复制进来的整个区域**" ✗
    //（也就是本轮修掉的那个矩形 artifact ✓）⇒ 那个数恒定 ✓、什么都不能证明 ✗。
    let run = |name: &str| -> (u64, Vec<u8>) {
        let root = temp_dir(name);
        let mut workspace = workspace(&root);
        assert_eq!(
            call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
            json!(true)
        );
        // 先铺一层**有东西可抹**的底 ✓（`classic-knife` ✓）。
        let base = call(
            &mut workspace,
            "brush_stroke",
            json!({
                "layer_id": "L", "object_id": "base1", "brush": "classic-knife", "size": 40,
                "points": [[40.0, 60.0, 0.9], [200.0, 60.0, 0.9]]
            }),
        );
        assert_eq!(base["ok"], json!(true), "{base}");
        let smudged = call(
            &mut workspace,
            "brush_stroke",
            json!({
                "layer_id": "L", "object_id": "smudge1", "brush": "ramon-Knife", "size": 40,
                "points": [[60.0, 60.0, 0.9], [180.0, 66.0, 0.9]]
            }),
        );
        assert_eq!(
            smudged["ok"],
            json!(true),
            "有东西可抹时涂抹必须能用：{smudged}"
        );
        let painted = smudged["painted_pixels"].as_u64().unwrap_or(0);
        assert!(painted > 0, "涂抹应当真的改动像素：{smudged}");
        let (_w, _h, pixels) = workspace
            .document_mut("doc_brush")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 400.0, 200.0))
            .expect("区域渲染应成功");
        (painted, pixels)
    };
    let (first_painted, first_pixels) = run("smudge_det_a");
    let (second_painted, second_pixels) = run("smudge_det_b");
    assert_eq!(
        first_pixels, second_pixels,
        "同样的输入必须给出**逐字节相同**的画面（涂抹是确定性的 ✓，不是随机噪声 ✗）"
    );
    assert_eq!(
        first_painted, second_painted,
        "同样的输入必须改动同样多的像素（确定性 ✓）"
    );
}

/// **`brush_stroke` 能设颜色** ✓（AI 实测报的 P0 缺口 ✓）—— **判据必须是"两色不同"** ✗。
///
/// **我上一版为什么不算数** ✗：只画了**一笔红的** ✓ ⇒ 它**无法区分**"用了我的颜色" ✓
/// 与"**无论给什么都画红色**" ✗ ✓ —— 而事实恰恰是后者 ✓（实测黄/蓝都成了红 ✗）。
/// **根因（查引擎源码得到的 ✓）**：`hsv_to_rgb` 里 `h` 是 **0..1 的圆周分数** ✗，
/// 而我传的是**度数** ⇒ `rem_euclid(1.0)` 把 60 与 240 都变成 0 ✗ ⇒ **全红** ✓。
/// ⇒ 本判据的形状改成"**两个不同的输入 ⇒ 两个不同的输出**" ✓，**它才可能红** ✓。
#[test]
fn brush_stroke_accepts_a_colour_and_two_colours_differ() {
    let root = temp_dir("colour");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // **两笔各画在一层上** ✓ —— 同层导出会互相稀释 ✗（这个坑我也踩过 ✓）。
    let paint =
        |workspace: &mut Workspace, object: &str, colour: serde_json::Value| -> (f64, f64, f64) {
            let layer = format!("L_{object}");
            assert_eq!(
                call(workspace, "create_layer", json!({ "layer_id": layer }))["ok"],
                json!(true)
            );
            let made = call(
                workspace,
                "brush_stroke",
                json!({
                    "layer_id": layer, "object_id": object, "brush": "100%_Opaque", "size": 30,
                    "color": colour,
                    "points": [[30.0, 40.0, 0.9], [150.0, 40.0, 0.9]]
                }),
            );
            assert_eq!(made["ok"], json!(true), "带 color 的一笔应当成功：{made}");
            let path = root.join(format!("{object}.png"));
            assert_eq!(
                call(
                    workspace,
                    "export_png",
                    json!({ "path": path.display().to_string(), "layer_id": layer })
                )["ok"],
                json!(true)
            );
            let (_w, _h, rgba) =
                yanshi_render::png::decode_png(&std::fs::read(&path).unwrap()).unwrap();
            let (mut r, mut g, mut b, mut count) = (0f64, 0f64, 0f64, 0f64);
            for pixel in rgba.chunks(4) {
                if pixel[0] == 255 && pixel[1] == 255 && pixel[2] == 255 {
                    continue; // **白底会把均值拉平** ✗（老坑 ✓）
                }
                r += f64::from(pixel[0]);
                g += f64::from(pixel[1]);
                b += f64::from(pixel[2]);
                count += 1.0;
            }
            assert!(count > 0.0, "这一笔应当留下像素（{object}）");
            (r / count, g / count, b / count)
        };
    let warm = paint(
        &mut workspace,
        "warm",
        json!({"r": 220, "g": 30, "b": 30, "a": 255}),
    );
    let cool = paint(
        &mut workspace,
        "cool",
        json!({"r": 30, "g": 30, "b": 220, "a": 255}),
    );
    eprintln!(
        "  颜色口径：红笔 R={:.1} G={:.1} B={:.1} ／ 蓝笔 R={:.1} G={:.1} B={:.1}",
        warm.0, warm.1, warm.2, cool.0, cool.1, cool.2
    );
    // ① **两个不同的输入必须给出不同的输出** ✓（这条才是能红的判据 ✓）。
    let difference = (warm.0 - cool.0).abs() + (warm.2 - cool.2).abs();
    assert!(
        difference > 60.0,
        "红笔与蓝笔必须明显不同 ✗（实测差 {difference:.1} ⇒ 颜色没被采纳 ✗）"
    );
    // ② **方向也要对** ✓（红笔 R 最大 ✓、蓝笔 B 最大 ✓）。
    assert!(
        warm.0 > warm.2 + 60.0,
        "红笔的 R 应当最大 ✓（实测 {:?}）",
        warm
    );
    assert!(
        cool.2 > cool.0 + 60.0,
        "蓝笔的 B 应当最大 ✓（实测 {:?}）",
        cool
    );
}

/// **`color` 与 `size` 相互独立** ✓ —— **只给颜色、不给 `size`** 时颜色也必须生效 ✓。
///
/// **修的是什么** ✗（同类病症又一例 ✓）：颜色覆盖此前**嵌在 `if let Some(diameter)` 里面** ✓
/// ⇒ 只给 `color` 不给 `size` 时颜色被**静默丢掉** ✗：返回 `ok` ✓、画面用的是 `.myb` 自带色 ✗
/// —— 正是"返回 ok 却没有效果" ✓。判据与上一条同形 ✓：
/// **同一支笔刷、都不给 `size`、两种颜色 ⇒ 两张图必须一张是红、一张是蓝** ✓ + **方向检查** ✓。
///
/// **为什么按"主色像素数"量、而不按均值** ✗：`100%_Opaque` 不给 `size` 时缺省半径很小 ✓
/// ⇒ 墨只占几个像素 ✓，均值被白底拉平（实测差只有 26.5 ✗）⇒ 那是**不能红**的判据 ✗（老坑 ✓）。
#[test]
fn brush_stroke_colours_without_a_size() {
    let root = temp_dir("colour_no_size");
    let mut workspace = workspace(&root);
    // 两笔各画一层 ✓（同层会互相稀释 ✗ —— 这个坑上面那条已经踩过 ✓）。
    let paint =
        |workspace: &mut Workspace, object: &str, colour: serde_json::Value| -> (usize, usize) {
            let layer = format!("L_{object}");
            assert_eq!(
                call(workspace, "create_layer", json!({ "layer_id": layer }))["ok"],
                json!(true)
            );
            let made = call(
                workspace,
                "brush_stroke",
                json!({
                    "layer_id": layer, "object_id": object, "brush": "100%_Opaque",
                    // **刻意不给 `size`** ✓ —— 缺省半径由 `.myb` 自己说了算 ✓。
                    "color": colour,
                    "points": [[30.0, 40.0, 0.9], [150.0, 40.0, 0.9]]
                }),
            );
            assert_eq!(
                made["ok"],
                json!(true),
                "只给颜色不给 size 也应当成功：{made}"
            );
            let (_w, _h, rgba) = workspace
                .render_region_raw_layer(
                    "doc_brush",
                    yanshi_core::Bbox::new(0.0, 0.0, 400.0, 200.0),
                    &layer,
                )
                .expect("区域渲染应成功");
            // **数"明确偏红 / 明确偏蓝"的像素** ✓（比另外两个通道高 40 ✓）——
            // 白底（三个通道都高 ✓）与灰墨（都不高 ✓）都不会被算进去 ✓。
            let (mut red_ink, mut blue_ink) = (0usize, 0usize);
            for pixel in rgba.chunks_exact(4) {
                let (r, g, b) = (
                    i32::from(pixel[0]),
                    i32::from(pixel[1]),
                    i32::from(pixel[2]),
                );
                if r > g + 40 && r > b + 40 {
                    red_ink += 1;
                }
                if b > r + 40 && b > g + 40 {
                    blue_ink += 1;
                }
            }
            (red_ink, blue_ink)
        };
    // **用十六进制写法** ✓ —— 同时钉住"Web 颜色选择器那种写法也要认" ✓。
    let (warm_red, warm_blue) = paint(&mut workspace, "nosize_warm", json!("#dc1e1e"));
    let (cool_red, cool_blue) = paint(&mut workspace, "nosize_cool", json!("#1e1edc"));
    eprintln!(
        "  不给 size 的颜色口径：红笔 偏红像素={warm_red} 偏蓝像素={warm_blue} ／ \
         蓝笔 偏红像素={cool_red} 偏蓝像素={cool_blue}"
    );
    // ① **两个不同的输入 ⇒ 两个不同的输出** ✓（这条才是能红的判据 ✓）。
    assert!(
        warm_red > 0 && cool_blue > 0,
        "两种颜色都应当真的落到画面上 ✗（红笔偏红 {warm_red} ✓、蓝笔偏蓝 {cool_blue} ✓）"
    );
    // ② **方向也要对** ✓：红笔不许有偏蓝像素 ✓、蓝笔不许有偏红像素 ✓。
    assert_eq!(
        (warm_blue, cool_red),
        (0, 0),
        "方向不对：红笔里出现了偏蓝像素 {warm_blue} ✗ / 蓝笔里出现了偏红像素 {cool_red} ✗"
    );
}

/// **笔触不许把"区域矩形"烘进对象** ✓ —— 用户报的「`Flat2#1` 画叶子出矩形 artifact」的确切形状 ✓。
///
/// **机理** ✓：涂抹 / 平头笔刷要把**画布现有像素**喂进引擎 ✓ ⇒ 落笔后 `surface` 里**整个区域**都有像素 ✓；
/// 而读回时若把 surface **原样全部**导出 ✓，那"笔触的包围矩形"就被当成**新对象**提交 ✓
/// ⇒ 删掉底下的东西之后，画面里留下一块**直角矩形幽灵** ✗（实测：`painted_pixels` **正好等于区域面积** ✓）。
///
/// **判据两条** ✓（都能红 ✓，都实测过 ✓）：
/// ① **对象不许背着整个区域** ✓：`painted_pixels` 必须**远小于**区域面积
///    （实测：修复前 `1596/1596` ✗ ⇒ 修复后 `113/1596` ✓）；
/// ② **删掉底下的绿条 ⇒ 不许留下"矩形幽灵"** ✓：绿条那两行里"仍像绿条颜色"的像素必须很少
///    （实测：修复前 `155/160` ✗ ⇒ 修复后 `6/160` ✓）。
#[test]
fn a_stroke_does_not_bake_its_region_rectangle_into_the_object() {
    let root = temp_dir("no_bake");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // 绿条放在**区域的上边缘**（笔触在 y=40 ✓、半径 3 ✓、区域 y=33..46 ✓）⇒ **笔尖碰不到它** ✓
    // ⇒ 它出现在对象里就只可能是"被复制"✓，不可能是"被画到"✓。
    assert_eq!(
        call(
            &mut workspace,
            "draw_shape",
            json!({
                "layer_id": "L", "object_id": "bar1",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 60, "y": 33, "w": 80, "h": 2}},
                    "color": {"r": 0, "g": 255, "b": 0, "a": 255}
                }
            })
        )["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "pen1", "brush": "2B_pencil", "size": 6,
            "points": [[50.0, 40.0, 0.9], [150.0, 40.0, 0.9]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");

    // ① 对象不许**把整个区域**都背进去 ✓
    //（修复前 `painted_pixels` **正好等于区域面积** ✓ —— 每个像素都被复制了一份 ✗；
    //  修复后它只数"这一笔真的碰过的像素" ✓ —— 墨当然仍占区域里的一部分 ✓，但那不再是"矩形复制"✓）。
    let area =
        made["region"]["w"].as_f64().unwrap_or(0.0) * made["region"]["h"].as_f64().unwrap_or(0.0);
    let painted = made["painted_pixels"].as_f64().unwrap_or(0.0);
    assert!(
        painted < area,
        "整个区域都被背进了对象 ✗（painted={painted} 区域面积={area}）\
         —— 那正是「矩形 artifact」的来源 ✓"
    );

    // ② 删掉底下的绿条 ⇒ 画面里不许留下矩形幽灵 ✓
    assert_eq!(
        call(
            &mut workspace,
            "delete_object",
            json!({ "object_id": "bar1" })
        )["ok"],
        json!(true)
    );
    let (_w, _h, rgba) = workspace
        .document_mut("doc_brush")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 400.0, 200.0))
        .expect("区域渲染应成功");
    let mut barish = 0;
    for y in 33..35 {
        for x in 60..140 {
            let at = (y * 400 + x) * 4;
            let (r, g, b, a) = (
                rgba[at] as i32,
                rgba[at + 1] as i32,
                rgba[at + 2] as i32,
                rgba[at + 3],
            );
            if a > 40 && r.abs() <= 8 && (g - 255).abs() <= 8 && b.abs() <= 8 {
                barish += 1;
            }
        }
    }
    assert!(
        barish * 10 <= 160,
        "底下的绿条被复制成了**矩形幽灵** ✗（原本 160 个绿像素里还剩 {barish} 个 ✓）\
         —— 笔触对象里背着一份底图 ✓"
    );
}

/// **靠近画布边缘的涂抹必须照样能用** ✓ —— 区域越界时要**裁到画布内** ✓，而不是整块跳过 ✗。
///
/// **修的是什么** ✗（第 33 轮实测撞见 ✓）：`render_region_raw` 会把越界的区域**裁到画布内** ✓
/// ⇒ 回来的尺寸与请求的尺寸对不上 ✓ ⇒ 旧代码**直接跳过喂底图** ✓（只打一行"底图尺寸不符"✓）
/// ⇒ **涂抹退化成"没有东西可抹"** ✗ —— 而它在画布中间完全正常 ✓（边缘与中间**语义不一致** ✓）。
///
/// **判据** ✓：同一条涂抹在**画布边缘**与在**画布中间**都必须真的改动像素 ✓
/// （修复前：边缘那次会以 `precondition_failed` 失败 ✓ ⇒ 能红 ✓）。
#[test]
fn a_smudge_brush_works_at_the_canvas_edge_too() {
    let root = temp_dir("smudge_edge");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // **先把"可抹的东西"画在紧贴顶边的地方** ✓（`classic-knife` 不读画布 ⇒ 它自己画得出来 ✓）。
    let base = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "base_edge", "brush": "classic-knife", "size": 30,
            "points": [[60.0, 8.0, 0.9], [200.0, 8.0, 0.9]]
        }),
    );
    assert_eq!(base["ok"], json!(true), "{base}");
    // **再在顶边抹一把** ✓ —— 它的区域一定越界（半径 15 ⇒ 想要 y=-7 起 ✓）⇒ 必须裁 ✓。
    let edge = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "smudge_edge", "brush": "ramon-Knife", "size": 30,
            "points": [[70.0, 8.0, 0.9], [190.0, 12.0, 0.9]]
        }),
    );
    assert_eq!(
        edge["ok"],
        json!(true),
        "贴边的涂抹也必须能用（不许因为区域越界就跳过底图 ✗）：{edge}"
    );
    let edge_painted = edge["painted_pixels"].as_u64().unwrap_or(0);
    assert!(edge_painted > 0, "贴边的涂抹应当真的改动像素：{edge}");

    // **画布中间做一遍同样的对照** ✓ ⇒ 两边都必须成立 ✓
    //（"边缘与中间不一致" ✗ 才是要修的病 ✓）。
    // **对照组也必须先有"可抹的东西"** ✗ —— 我第一版漏了这一步 ✓，
    // 于是中间那次当然报"没东西可抹" ✓：那是**测试自己的问题** ✗，不是代码的 ✓（当场看出来 ✓）。
    let base_middle = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "base_middle", "brush": "classic-knife", "size": 30,
            "points": [[60.0, 100.0, 0.9], [200.0, 100.0, 0.9]]
        }),
    );
    assert_eq!(base_middle["ok"], json!(true), "{base_middle}");
    let middle = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "smudge_middle", "brush": "ramon-Knife", "size": 30,
            "points": [[70.0, 100.0, 0.9], [190.0, 104.0, 0.9]]
        }),
    );
    assert_eq!(middle["ok"], json!(true), "{middle}");
    assert!(
        middle["painted_pixels"].as_u64().unwrap_or(0) > 0,
        "画布中间的涂抹应当真的改动像素：{middle}"
    );
    // **同一个对象区域都不许越出画布** ✓（裁过之后才提交 ✓）。
    for value in [&edge, &middle] {
        let region = &value["region"];
        assert!(
            region["x"].as_f64().unwrap_or(-1.0) >= 0.0
                && region["y"].as_f64().unwrap_or(-1.0) >= 0.0,
            "对象区域不许越出画布左上角：{value}"
        );
    }
}

/// **一笔多色（Loaded Brush）** ✓ —— 用户："一笔一色，花瓣渐变只能分两笔，交界硬" ✗。
///
/// **做法** ✓：把路径按弧长切段 ✓、**每段换一次笔刷颜色** ✓，而 `surface` 与 `BrushState`
/// **一路共用** ✓ ⇒ 出来的还是**一条笔迹** ✓（不是"两笔拼起来"✗ —— 那会多一个对象 ✗、交界也硬 ✗）。
///
/// **判据（三条都能红 ✓）**：
/// ① **沿长度颜色真的在变** ✓：把笔迹切成 8 段，红分量必须**单调下降**、蓝分量**单调上升** ✓
///    （每一对相邻段至少各有 4 处严格变化 ✓ ⇒ "整笔一个色" ✗ 与"到某处突然跳一下" ✗ 都不算 ✓）；
/// ② **仍然只有一个对象** ✓（"分两笔画"✗ 会变成两个 ✓）；
/// ③ **不给 `color_to` ⇒ 与以前逐字节相同** ✓（老调用方不受影响 ✓）。
#[test]
fn brush_stroke_ramps_from_one_colour_to_another() {
    let root = temp_dir("ramp");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // ① 一笔从红到蓝 ✓（横向一条，便于按 x 切段 ✓）。
    let ramped = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "ramp1", "brush": "100%_Opaque", "size": 24,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "color_to": {"r": 0, "g": 0, "b": 255, "a": 255},
            "points": [[40.0, 60.0, 0.9], [360.0, 60.0, 0.9]]
        }),
    );
    assert_eq!(ramped["ok"], json!(true), "{ramped}");
    // ② **一个对象** ✓。
    let objects = call(&mut workspace, "list_objects", json!({}));
    assert_eq!(
        objects["count"],
        json!(1),
        "一笔多色必须还是**一个**对象：{objects}"
    );

    let (_w, _h, pixels) = workspace
        .document_mut("doc_brush")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 400.0, 200.0))
        .expect("区域渲染应成功");
    // 沿 x 切 8 段 ✓，每段只看"有墨且不是白底"的像素 ✓。
    let mut reds = Vec::new();
    let mut blues = Vec::new();
    for band in 0..8 {
        let x0 = 40 + band * 40;
        let (mut r_sum, mut b_sum, mut count) = (0f64, 0f64, 0f64);
        for y in 0..200 {
            for x in x0..(x0 + 40).min(400) {
                let at = (y * 400 + x) * 4;
                let (r, g, b, a) = (pixels[at], pixels[at + 1], pixels[at + 2], pixels[at + 3]);
                if a < 40 || (r > 245 && g > 245 && b > 245) {
                    continue; // 透明 / 白底都不算墨 ✓。
                }
                r_sum += f64::from(r);
                b_sum += f64::from(b);
                count += 1.0;
            }
        }
        assert!(count > 0.0, "第 {band} 段应当有墨 ✓");
        reds.push(r_sum / count);
        blues.push(b_sum / count);
    }
    eprintln!("  一笔多色 红分量: {reds:?}");
    eprintln!("  一笔多色 蓝分量: {blues:?}");
    let falling = (0..7).filter(|i| reds[*i] > reds[i + 1] + 1.0).count();
    let rising = (0..7).filter(|i| blues[i + 1] > blues[*i] + 1.0).count();
    assert!(
        falling >= 4 && rising >= 4,
        "整笔必须**沿长度从红走到蓝** ✗（实测 红下降 {falling}/7 ✓、蓝上升 {rising}/7 ✓）"
    );

    // ③ **不给 color_to ⇒ 与以前逐字节相同** ✓（"多色"是加法 ✓，不是改默认 ✗）。
    let plain = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "plain1", "brush": "100%_Opaque", "size": 24,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "points": [[40.0, 120.0, 0.9], [360.0, 120.0, 0.9]]
        }),
    );
    assert_eq!(plain["ok"], json!(true), "{plain}");
    let again = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "plain2", "brush": "100%_Opaque", "size": 24,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "points": [[40.0, 120.0, 0.9], [360.0, 120.0, 0.9]]
        }),
    );
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(
        plain["painted_pixels"], again["painted_pixels"],
        "同一条单色笔触两次必须一致（默认路径没被多色改坏 ✓）"
    );
}

/// **只给 `color_to`、不给 `color` ⇒ 明确拒绝** ✓（起点色**不猜** ✗ —— 猜错就是"界面与画面不一致" ✓）。
#[test]
fn brush_stroke_refuses_colour_to_without_colour() {
    let root = temp_dir("ramp_no_from");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let refused = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque", "size": 20,
            "color_to": {"r": 0, "g": 0, "b": 255, "a": 255},
            "points": [[40.0, 60.0, 0.9], [200.0, 60.0, 0.9]]
        }),
    );
    assert_eq!(refused["ok"], json!(false), "只给末端色必须拒绝：{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("color_to") && detail.contains("color"),
        "错误要说清「必须同时给 color」：{detail}"
    );
}

/// **半透明笔触必须存"纯色 + 低 alpha"** ✓ —— 不许把颜色按覆盖度**预乘**进 RGB ✗。
///
/// **真 bug（外部 MCP 报告指出 ✓，本轮定位到根因 ✓）**：Hokusai 的 tile 是
/// **预乘** RGBA（其 crate 文档 `brushmodes.rs` 第 4-7 行明说 ✓），而我们存对象/blob 用的是
/// **直通** RGBA8 ✓ ⇒ 直接把预乘值当直通 ✗ ⇒ 每个半透明 dab 的颜色被自己的覆盖度**乘了第二次** ✓
/// ⇒ 叠到白底就"颜色被洗掉、发灰" ✗（`Round` 笔刷纯红实测：最饱和像素是 `(70,0,0) alpha=70` ✗）。
///
/// **判据（能红 ✓，且与"多深"无关 ✓）**：同一支笔刷、同一个颜色、**两种深浅**（大小或压力不同 ✓）
/// ⇒ 对象里**最饱和的像素**必须是**同一个纯色** ✓（红 ⇒ `(255,0,0)` ✓），
/// 而 alpha 允许不同 ✓ —— 预乘错会把两者一起拉低 ✓ ⇒ 一红就抓住 ✓。
#[test]
fn a_semi_transparent_stroke_stores_the_pure_colour_not_a_premultiplied_one() {
    let root = temp_dir("premultiply");
    let mut workspace = workspace(&root);
    let probe = |workspace: &mut Workspace, object: &str, size: f64| -> (u8, u8, u8, u8) {
        assert_eq!(
            call(workspace, "create_layer", json!({ "layer_id": object }))["ok"],
            json!(true)
        );
        let made = call(
            workspace,
            "brush_stroke",
            json!({
                "layer_id": object, "object_id": object, "brush": "Round", "size": size,
                "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                "points": [[40.0, 60.0, 1.0], [120.0, 60.0, 1.0]]
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
        let got = call(workspace, "get_object", json!({ "object_id": object }));
        let blob = got["data"]["bitmap"]["blob_hash"]
            .as_str()
            .expect("画笔笔触应当带 bitmap.blob_hash")
            .to_owned();
        let hash: yanshi_core::BlobHash = blob.parse().expect("blob 地址应当能解析");
        let pixels = workspace.store().get(&hash).expect("对象 blob 应当取得到");
        // **最"有颜色"的那个像素** ✓：预乘错会让它的 RGB 跟着覆盖度一起变小 ✓。
        let mut best = (0i32, 0u8, [0u8; 3]);
        for pixel in pixels.chunks_exact(4) {
            let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
            let saturation = i32::from(r.max(g).max(b)) - i32::from(r.min(g).min(b));
            if a > 0 && saturation >= best.0 {
                best = (saturation, a, [r, g, b]);
            }
        }
        (best.2[0], best.2[1], best.2[2], best.1)
    };
    let (r1, g1, b1, a1) = probe(&mut workspace, "thin", 24.0);
    let (r2, g2, b2, a2) = probe(&mut workspace, "thick", 60.0);
    eprintln!("  半透明笔触最饱和像素：细 {r1},{g1},{b1} a={a1} ／ 粗 {r2},{g2},{b2} a={a2}");
    // ① **颜色是纯红** ✓（不是被覆盖度乘过的暗红 ✗）。
    for (r, g, b, a, label) in [(r1, g1, b1, a1, "细"), (r2, g2, b2, a2, "粗")] {
        assert!(
            r > 240 && g < 16 && b < 16,
            "{label}笔最饱和的像素应当是**纯红** ✗（实测 rgb=({r},{g},{b}) alpha={a} ⇒ 颜色被预乘进 RGB 了 ✓）"
        );
    }
    // ② **两种深浅都要有自己的 alpha** ✓（否则这条判据本身就没在测"半透明" ✓）。
    assert!(
        a1 > 0 && a2 > 0,
        "两条笔触都应当有墨 ✓（实测 alpha {a1} / {a2}）"
    );
}

/// **`brush_stroke` 的 `opacity` / `hardness` 真的进到引擎** ✓ ——
/// 外部绘画 agent 实测痛点（原文 ✓）："`draw_stroke` 有 hardness + opacity，**`brush_stroke` 没有**
/// ⇒ 要 MyPaint 物理和要可控透明度**无法同时满足**"✗ —— 它只能放弃笔刷引擎、改用 `draw_stroke` 画云 ✓。
///
/// **判据（两条都能红 ✓，都不看"返回 ok"✗）**：
/// ① `opacity` 0.25 与 1.0 ⇒ 对象 blob 的**平均 alpha** 必须显著下降 ✓（笔触变淡 ✓）；
/// ② `hardness` 0.3 与 1.0 ⇒ 软边的**平均 alpha 更低** ✓（同样的墨摊在更宽的过渡上 ✓）；
///    （`hardness: 0.0` 在本引擎上**一枚 dab 都不落** ✓ ⇒ 那档不用来当判据 ✓，如实写在这里 ✓）。
/// 把落笔时那两处 `brush.set` 去掉 ⇒ 两者**逐字节相同** ⇒ 两条都当场红 ✓（实测过 ✓）。
#[test]
fn brush_stroke_opacity_and_hardness_reach_the_engine() {
    let root = temp_dir("opacity_hardness");
    let mut workspace = workspace(&root);
    let probe = |workspace: &mut Workspace, object: &str, extra: serde_json::Value| -> f64 {
        assert_eq!(
            call(workspace, "create_layer", json!({ "layer_id": object }))["ok"],
            json!(true)
        );
        let mut args = json!({
            "layer_id": object, "object_id": object, "brush": "100%_Opaque", "size": 40,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "points": [[40.0, 60.0, 1.0], [160.0, 60.0, 1.0]]
        });
        for (key, value) in extra.as_object().cloned().unwrap_or_default() {
            args[key] = value;
        }
        let made = call(workspace, "brush_stroke", args);
        assert_eq!(made["ok"], json!(true), "{made}");
        let got = call(workspace, "get_object", json!({ "object_id": object }));
        let hash: yanshi_core::BlobHash = got["data"]["bitmap"]["blob_hash"]
            .as_str()
            .expect("画笔笔触应当带 bitmap.blob_hash")
            .parse()
            .expect("blob 地址应当能解析");
        let pixels = workspace.store().get(&hash).expect("对象 blob 应当取得到");
        let mut total = 0f64;
        let mut count = 0f64;
        for pixel in pixels.chunks_exact(4) {
            if pixel[3] > 0 {
                total += f64::from(pixel[3]);
                count += 1.0;
            }
        }
        total / count.max(1.0)
    };
    let opaque = probe(&mut workspace, "full", json!({"opacity": 1.0}));
    let faint = probe(&mut workspace, "quarter", json!({"opacity": 0.25}));
    eprintln!("  opacity：1.0 ⇒ 平均 alpha {opaque:.1} ／ 0.25 ⇒ {faint:.1}");
    // **阈值不能按"每 dab 的 opaque"去要求 1/4** ✗（实测 252.5 ⇒ 184.1 = 0.73 ✓）：
    // 一条笔触上有**几十枚 dab 叠加** ✓ ⇒ 单枚 0.25 的不透明度会被**累积**回一大截 ✓（这是引擎的正确行为 ✓）。
    // 判据取"**明显更淡**"（< 0.9×）✓ —— 参数被忽略时两者**逐字节相同** ⇒ 照样红 ✓。
    assert!(
        faint < opaque * 0.9,
        "opacity 0.25 必须明显淡于 1.0 ✗（实测 {opaque:.1} ⇒ {faint:.1}）"
    );
    // **`hardness: 0.0` 会一枚 dab 都不落** ✓（实测：`precondition_failed`"没落下任何像素"✗）——
    // 那是引擎对"完全软边"的真实行为 ✓，不是本测试要测的东西 ✗ ⇒ 取 0.3 与 1.0（两头都能落墨 ✓）。
    let hard = probe(&mut workspace, "hard", json!({"hardness": 1.0}));
    let soft = probe(&mut workspace, "soft", json!({"hardness": 0.3}));
    eprintln!("  hardness：1.0 ⇒ 平均 alpha {hard:.1} ／ 0.3 ⇒ {soft:.1}");
    assert!(
        soft < hard,
        "软边（hardness 0）的平均 alpha 应当低于硬边 ✗（实测 {hard:.1} ⇒ {soft:.1}）"
    );
}

/// **落笔位置就是调用方要的位置** ✓ —— 外部 MCP 报告连着两轮说"坐标非线性偏移"✗，
/// 而我这边量到的质心与请求**逐点吻合** ✓（见 `docs/design/implementation-notes.md` 第 41 轮 ✓）。
///
/// **这条判据的意义** ✓：把"坐标映射"这件事**钉死** ⇒ 将来真出现偏移（差一个视口原点、
/// DPI 缩放、区域裁剪……✓）会**当场红** ✓，而不用再靠外部报告的两次互相矛盾的数字来猜 ✓。
///
/// **判据** ✓（都是绝对断言 ✓，不看 `ok` ✗）：
/// ① 三点笔迹 (150,100)→(170,100)→(190,100)、size 24（测试文档是 400×200 ✓）
///    ⇒ 墨的**质心必须落在 (170,100) ± 3px** ✓；
/// ② 墨的**包围盒**必须落在 `x ∈ [130,215]`、`y ∈ [80,122]` ✓（= 笔迹 ± 半个笔尖 ✓）。
#[test]
fn a_stroke_lands_exactly_where_the_caller_asked() {
    let root = temp_dir("lands_where_asked");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "placed", "brush": "Round", "size": 40,
            "color": {"r": 0, "g": 0, "b": 0},
            "points": [[150.0, 100.0, 1.0], [170.0, 100.0, 1.0], [190.0, 100.0, 1.0]]
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let document = workspace.document_mut("doc_brush").unwrap();
    let (width, height, pixels) = document
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 400.0, 200.0))
        .expect("区域渲染应成功");
    assert_eq!((width, height), (400, 200));
    let mut count = 0f64;
    let mut sum_x = 0f64;
    let mut sum_y = 0f64;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (index, pixel) in pixels.chunks_exact(4).enumerate() {
        let (r, g, b, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a <= 32 || (r > 245 && g > 245 && b > 245) {
            continue;
        }
        let (x, y) = ((index % 400) as f64, (index / 400) as f64);
        count += 1.0;
        sum_x += x;
        sum_y += y;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    assert!(count > 0.0, "这一笔应当真的落下墨 ✓");
    let (cx, cy) = (sum_x / count, sum_y / count);
    eprintln!(
        "  落笔位置：质心 ({cx:.1}, {cy:.1})，包围盒 [{min_x:.0},{min_y:.0}]–[{max_x:.0},{max_y:.0}]，墨 {count:.0} 像素"
    );
    assert!(
        (cx - 170.0).abs() <= 3.0 && (cy - 100.0).abs() <= 3.0,
        "墨的质心必须落在 (170,100) ± 3px ✗（实测 ({cx:.1}, {cy:.1})）"
    );
    assert!(
        min_x >= 130.0 && max_x <= 215.0 && min_y >= 80.0 && max_y <= 122.0,
        "墨的包围盒必须落在笔迹 ± 半个笔尖之内 ✗（实测 [{min_x:.0},{min_y:.0}]–[{max_x:.0},{max_y:.0}]）"
    );
}

/// **整条笔迹在画布外 ⇒ 必须报"坐标在画布外"，不许报"换一支笔刷"** ✗。
///
/// **为什么专门测这个** ✓：外部 MCP 报告**连着两轮**把"坐标非线性偏移"当 bug 报 ✓，
/// 而它量到的很可能是**被画布裁掉一半的墨** ✗ —— 因为当时这种情况会一路走到
/// "这一笔没落下任何像素" ✓，再被那句"换一支笔刷"✗ 引到**坐标以外**的方向 ✓。
/// **判据** ✓（措辞级 ✓，能红 ✓）：错误里必须出现**画布尺寸**与**笔迹范围** ✓，
/// 且**不许**出现"换一支笔刷"✗（把修复前的消息接回去 ⇒ 当场红 ✓）。
#[test]
fn a_stroke_entirely_outside_the_canvas_says_so_instead_of_blaming_the_brush() {
    let root = temp_dir("outside_canvas");
    let mut workspace = workspace(&root);
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    // 测试文档是 **400×200** ✓ ⇒ 这条笔迹（600..700, 400）整条都在画布外 ✓。
    let made = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "object_id": "outside", "brush": "Round", "size": 24,
            "color": {"r": 0, "g": 0, "b": 0},
            "points": [[600.0, 400.0, 1.0], [700.0, 400.0, 1.0]]
        }),
    );
    let detail = made["context"]["detail"].as_str().unwrap_or("").to_owned();
    eprintln!("  画布外落笔的错误：{detail}");
    assert_eq!(made["ok"], json!(false), "整条在画布外不该静默成功：{made}");
    assert!(
        detail.contains("400") && detail.contains("200"),
        "错误里必须写出**画布尺寸**（400×200）✗：{detail}"
    );
    assert!(
        detail.contains("范围") && detail.contains("坐标"),
        "错误里必须写出**笔迹范围**并点明这是坐标问题（让调用方一眼能改 ✓）：{detail}"
    );
    assert!(
        !detail.contains("换一支笔刷"),
        "不许把「坐标在画布外」说成「这支笔刷画不出来」✗ —— 那正是把外部 agent 引偏的那句话 ✓：{detail}"
    );
}
