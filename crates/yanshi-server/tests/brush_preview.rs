//! **笔刷预览** ✓（用户：「201 支笔刷只有一个名字 ⇒ 选笔全凭猜，很不友好，**web 上也是**」✗）。
//!
//! **判据四条，每条都能红** ✓（"能红"= 出事时它必须失败 ✗）：
//! ① **两支不同的笔刷 ⇒ 两张预览必须不同** ✓
//!    —— "预览画成固定样子 / 忽略笔刷参数" ✗ 会被这条当场抓住 ✓；
//! ② **同一支两次 ⇒ 逐字节相同** ✓（确定性 ✓ —— 本项目的硬要求 ✓）；
//! ③ **它不碰文档** ✓（`head_seq` 与对象数不变 ✓）—— 预览绝不能顺手留下原子 ✗；
//! ④ **颜色真的进了预览** ✓（红/蓝两张，方向检查 ✓：红 ⇒ R 最大 ✓、蓝 ⇒ B 最大 ✓）
//!    —— 顺带钉住 `color` 的三种写法（这里用 `"#RRGGBB"` ✓，正是 Web 颜色选择器给的那种 ✓）。
//!
//! **为什么必须与 `brush_stroke` 同源** ✓：预览若另画一份"示意图" ✗，
//! 迟早与真笔触漂移 ✓ ⇒ 判据①就是**同一支笔刷经两条路**必须给出同一支笔的样子 ✓
//! （实现上两者共用 `paint_brush` ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 内存工作区 + **仓库里的 `assets/`** ✓（预览要真读 `.myb` ✓，不造假 ✓）。
fn workspace() -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_preview", 120, 80),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_preview", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

/// 把预览 PNG 从 blob CAS 里取回并解码 ✓（判据④要看**像素** ✓，不是看 `ok` ✗）。
fn preview_pixels(workspace: &mut Workspace, thumbnail_url: &str) -> (u32, u32, Vec<u8>) {
    let hash = thumbnail_url
        .strip_prefix("yanshi://blob/")
        .expect("thumb_url 应当是 blob 地址");
    let hash: yanshi_core::BlobHash = hash.parse().expect("blob 地址应当能解析");
    let png = workspace.store().get(&hash).expect("预览 PNG 应当取得到");
    yanshi_render::png::decode_png(&png).expect("预览必须是能解码的 PNG")
}

/// 各通道"占主导"的像素数 ✓（≥200 且比另外两个通道高 60 ✓）。
fn dominant(pixels: &[u8]) -> (usize, usize, usize) {
    let (mut red, mut green, mut blue) = (0, 0, 0);
    for pixel in pixels.chunks_exact(4) {
        // **只看有墨的像素** ✓（预览的其余部分是透明的 ✓，会把均值拉平 ✗ —— 老坑 ✓）。
        if pixel[3] < 200 {
            continue;
        }
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

/// ① **两支不同的笔刷 ⇒ 两张预览必须不同** ✓（"固定画成一个样子" ✗ 会被抓住 ✓）。
#[test]
fn two_brushes_give_two_different_previews() {
    let mut workspace = workspace();
    let pencil = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "2B_pencil", "size": 24 }),
    );
    let opaque = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "100%_Opaque", "size": 24 }),
    );
    assert_eq!(pencil["ok"], json!(true), "{pencil}");
    assert_eq!(opaque["ok"], json!(true), "{opaque}");
    for (name, value) in [("2B_pencil", &pencil), ("100%_Opaque", &opaque)] {
        assert!(
            value["painted_pixels"].as_u64().unwrap_or(0) > 0,
            "{name} 的预览应当真的落了墨：{value}"
        );
        assert!(
            value["thumb_url"]
                .as_str()
                .unwrap_or_default()
                .starts_with("yanshi://blob/"),
            "{name} 的预览应当给 blob 地址（Web 直接 <img> ✓）：{value}"
        );
    }
    // **两个不同的输入 ⇒ 两个不同的输出** ✓（这条才是能红的判据 ✓）。
    assert_ne!(
        pencil["blob_hash"], opaque["blob_hash"],
        "两支不同的笔刷必须给出不同的预览 ✗（预览没跟着笔刷走？）"
    );
}

/// ② **同一支笔刷两次 ⇒ 逐字节相同** ✓（预览是可复现的 ✓，不是随机噪声 ✓）。
#[test]
fn the_same_brush_is_byte_identical_twice() {
    let mut workspace = workspace();
    let first = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "2B_pencil" }),
    );
    let second = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "2B_pencil" }),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        first["blob_hash"], second["blob_hash"],
        "同一支笔刷两次必须给出同一张图（确定性 ✓）：{first} vs {second}"
    );
}

/// ③ **它不碰文档** ✓ —— 预览不许顺手在文档里留下原子 ✗。
#[test]
fn a_preview_does_not_touch_the_document() {
    let mut workspace = workspace();
    let before = call(&mut workspace, "get_state", json!({}));
    let head_before = before["head_seq"].clone();
    let preview = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "100%_Opaque" }),
    );
    assert_eq!(preview["ok"], json!(true), "{preview}");
    let after = call(&mut workspace, "get_state", json!({}));
    assert_eq!(
        after["head_seq"], head_before,
        "预览是只读的 ⇒ head_seq 不许变 ✗：{before} vs {after}"
    );
    let objects = call(&mut workspace, "list_objects", json!({}));
    assert_eq!(objects["count"], json!(0), "预览不许创建对象 ✗：{objects}");
}

/// ④ **颜色真的进了预览** ✓ + **`"#RRGGBB"` 这种写法必须被接受** ✓（Web 的颜色选择器给的就是它 ✓）。
#[test]
fn a_hex_colour_reaches_the_preview_with_the_right_direction() {
    let mut workspace = workspace();
    let red = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "100%_Opaque", "size": 30, "color": "#ff0000" }),
    );
    assert_eq!(red["ok"], json!(true), "十六进制颜色必须被接受：{red}");
    let blue = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "100%_Opaque", "size": 30, "color": "#0000ff" }),
    );
    assert_eq!(blue["ok"], json!(true), "{blue}");
    assert_ne!(
        red["blob_hash"], blue["blob_hash"],
        "两种颜色必须给出两张不同的预览 ✗"
    );
    let (_, _, red_pixels) = preview_pixels(&mut workspace, red["thumb_url"].as_str().unwrap());
    let (_, _, blue_pixels) = preview_pixels(&mut workspace, blue["thumb_url"].as_str().unwrap());
    let (r, g, b) = dominant(&red_pixels);
    assert!(
        r > 0 && g == 0 && b == 0,
        "红笔的预览应当是红占主导 ✓（实测 红 {r} / 绿 {g} / 蓝 {b}）"
    );
    let (r, g, b) = dominant(&blue_pixels);
    assert!(
        b > 0 && r == 0 && g == 0,
        "蓝笔的预览应当是蓝占主导 ✓（实测 红 {r} / 绿 {g} / 蓝 {b}）"
    );
}

/// ⑤ **`include_image` 给 MCP 的是一张真 PNG** ✓（≤512px ✓，base64 前缀就是 PNG 魔数 ✓）。
#[test]
fn include_image_returns_a_real_png_for_mcp() {
    let mut workspace = workspace();
    let got = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "100%_Opaque", "include_image": true }),
    );
    assert_eq!(got["ok"], json!(true), "{got}");
    assert_eq!(got["image"]["mime_type"], json!("image/png"), "{got}");
    let data = got["image"]["data"].as_str().unwrap_or_default();
    // **PNG 的 base64 前缀** ✓（`\x89PNG\r\n\x1a\n` ✓）—— 不解码也能证明"这是一张 PNG"✓。
    assert!(
        data.starts_with("iVBORw0KGgo"),
        "内嵌的应当是 PNG 的 base64：{}",
        &data[..data.len().min(32)]
    );
    assert!(
        got["width"].as_u64().unwrap_or(0) <= 512 && got["height"].as_u64().unwrap_or(0) <= 512,
        "内嵌图必须 ≤512px ✓：{got}"
    );
}

/// ⑥ **涂抹类笔刷的预览必须说清"为什么是空的"** ✓（而不是给一张空白图 ✗ 或只说 ok ✓）。
#[test]
fn a_smudge_brush_explains_why_its_preview_is_empty() {
    let mut workspace = workspace();
    // `ramon-Knife` 是涂抹类（`smudge = 1.0` ✓）—— 空画布上它**本来就什么都不出** ✓。
    let got = call(
        &mut workspace,
        "brush_preview",
        json!({ "brush": "ramon-Knife" }),
    );
    assert_eq!(got["ok"], json!(false), "没有墨的预览不许报成功：{got}");
    let detail = got["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("涂抹") || detail.contains("smudge"),
        "必须说清原因（涂抹类 / 空画布）✓：{detail}"
    );
}

/// **`list_assets` 的笔刷必须带分类** ✓（用户："只返回名字，无缩略图/分类" ✗）。
///
/// **为什么分类放在工具层** ✓：查看器下拉里的分组与 MCP 拿到的是**同一份** ✗
/// （两边各算一套前缀必然漂移 ✓ —— 本项目在"两份实现"上栽过多次 ✓）。
/// **缩略图不在这里发** ✓：201 支一次全画是肉眼可见的浪费 ✗ ⇒ 按需 `brush_preview` ✓。
///
/// **判据（三条 ✓）**：① 每一支都有非空 `category` ✓；② **已知前缀落对应分类** ✓；
/// ③ **没有前缀的落「其他」** ✓（不许静默变成空串 ✗）。
#[test]
fn list_assets_labels_every_brush_with_a_category() {
    let mut workspace = workspace();
    let listed = call(&mut workspace, "list_assets", json!({ "kind": "brush" }));
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let assets = listed["assets"].as_array().cloned().unwrap_or_default();
    assert!(assets.len() > 100, "应当列出上百支笔刷：{}", assets.len());
    let mut missing = 0;
    for asset in &assets {
        match asset["category"].as_str() {
            Some(text) if !text.is_empty() => {}
            _ => missing += 1,
        }
    }
    assert_eq!(missing, 0, "每一支笔刷都必须带分类（实测缺 {missing} 支）");
    let category_of = |prefix: &str| -> Option<String> {
        assets
            .iter()
            .find(|asset| asset["name"].as_str().unwrap_or("").starts_with(prefix))
            .and_then(|asset| asset["category"].as_str().map(str::to_owned))
    };
    // **已知前缀** ✓（`assets/brushes` 里这四类都在 ✓）。
    for (prefix, expected) in [
        ("classic-", "classic"),
        ("deevad-", "deevad"),
        ("ramon-", "ramon"),
        ("brushkit-", "brushkit"),
    ] {
        assert_eq!(
            category_of(prefix).as_deref(),
            Some(expected),
            "{prefix} 应当落在分类 {expected}"
        );
    }
    // **没有前缀的** ✓（例如 `Flat2#1` / `2B_pencil` ✓）⇒ 「其他」✓。
    let without_prefix = assets
        .iter()
        .find(|asset| {
            let name = asset["name"].as_str().unwrap_or("");
            !["classic", "deevad", "ramon", "brushkit"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .expect("应当有没前缀的笔刷");
    assert_eq!(
        without_prefix["category"],
        json!("其他"),
        "没前缀的笔刷应当落「其他」：{without_prefix}"
    );
}
