//! draw_text 真字体溯源：`export_svg` 出真 `<text>`（设计文档 §4）。
//!
//! 每条断言都对着一个「静默降级」形态：有溯源的对象必须出 `<text>`（不是
//! 悄悄回退位图）、无子集时必须计数 + 警告（不是悄悄出图）、builtin 文本
//! 必须单列计数（不是混进 vector 数）、旧文档（无溯源）必须行为不变 ✓。
//!
//! **∴ 种子方式** ✓：**不经过 `draw_text` 工具**（字体解析是 `fonts` 单测与
//! 盒上接受探针的事 ✓）⇒ **直接用 `import_image` 工具种一个带溯源的位图对象**
//! （与 `write_draw_text_with_font` 产出的净荷同形 ✓）⇒ **只验导出侧** ✓。
//! **∴ 字体** ✓：**仓库预置 `assets/fonts/Inter.ttf`**（`CARGO_MANIFEST_DIR` 相对路径 ✓，
//! CI 与本机都有 ✓，不依赖系统字体 ✓）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 临时目录守卫（本仓规矩：用了 `temp_dir()` 就必须有 Drop 守卫 ✓，
/// `check-tests-clean-temp` 会抓——声明 + 调用点绑定缺一不可 ✓）。
pub struct TempCleanup(pub std::path::PathBuf);
impl Drop for TempCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace(doc: &str) -> (Workspace, TempCleanup) {
    // 落盘工作区：`put_blob` 需要持久化存储，`in_memory` 工作区写 blob 会
    // `PreconditionFailed`（"用 --root 启动才有"）⇒ 种子用的位图/子集入库
    // 必须走 file store ✓（与 auto_retouch / auto_backtest 同例 ✓）。
    let root =
        std::env::temp_dir().join(format!("yanshi_text_prov_{}_{}", doc, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let guard = TempCleanup(root.clone());
    let mut workspace =
        Workspace::with_file_store(root, DocumentSettings::default()).expect("落盘工作区应当能建");
    workspace
        .create_document(NewDocument::new(doc, 256, 256), "human:1", "session:test")
        .unwrap();
    (workspace, guard)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn export_svg(
    ws: &mut Workspace,
    doc: &str,
    root: &std::path::Path,
    name: &str,
) -> (Value, String) {
    let path = root.join(name);
    let receipt = call(
        ws,
        doc,
        "export_svg",
        json!({"path": path.to_string_lossy()}),
    );
    assert_eq!(receipt["ok"], json!(true), "{receipt}");
    let text = std::fs::read_to_string(&path).expect("导出文件应能读回");
    (receipt, text)
}

fn repo_fonts_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets")
        .join("fonts")
}

/// 用仓库预置字体 Inter 种一个带溯源的位图对象（与 `draw_text{font=Inter}` 的
/// 净荷同形：位图 blob + region + `text_provenance` ✓）。
///
/// `with_subset=true` ⇒ 子集入库、溯源带 `subset_blob`；否则 `subset_blob` 为
/// null、`fallback_reason` 按传入的值记（`None` ⇒ null）。
#[allow(clippy::too_many_arguments)]
fn seed_text(
    ws: &mut Workspace,
    doc: &str,
    object_id: &str,
    text: &str,
    size: f32,
    color: [u8; 4],
    position: (i64, i64),
    align: &str,
    with_subset: bool,
    fallback_reason: Option<&str>,
) -> Value {
    let mut db = fontdb::Database::new();
    db.load_fonts_dir(repo_fonts_dir());
    let layout = yanshi_server::fonts::layout_text(&db, text, "Inter", size, align)
        .expect("仓库预置字体 Inter 应能排字");
    let (width, height, rgba) = yanshi_server::fonts::rasterize_layout(&layout, color);
    let bitmap_hash = ws.put_blob(&rgba).expect("位图入库");
    let subset_hash = if with_subset {
        let subset = yanshi_server::fonts::subset_layout(&layout).expect("子集应能生成");
        Some(ws.put_blob(&subset).expect("子集入库"))
    } else {
        None
    };
    let (anchor_x, anchor_y, anchor) =
        yanshi_server::fonts::text_anchor(&layout, (position.0 as f64, position.1 as f64), align);
    let provenance = json!({
        "text": text,
        "font": "Inter",
        "size": size,
        "font_size_svg": yanshi_server::fonts::effective_font_size(&layout),
        "color_rgba": [color[0], color[1], color[2], color[3]],
        "align": align,
        "position": [position.0, position.1],
        "baseline": {"x": anchor_x, "y": anchor_y},
        "anchor": anchor,
        "units_per_em": layout.units_per_em,
        "line_height_units": layout.line_height_units,
        "subset_blob": subset_hash,
        "subset_format": if with_subset { json!("ttf") } else { Value::Null },
        "fallback_reason": fallback_reason,
    });
    call(
        ws,
        doc,
        "import_image",
        json!({
            "layer_id": "L",
            "object_id": object_id,
            "bitmap": {"blob_hash": bitmap_hash, "size": rgba.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": position.0, "y": position.1, "w": width, "h": height},
            "text_provenance": provenance,
        }),
    )
}

fn create_vector_layer(ws: &mut Workspace, doc: &str) {
    let created = call(
        ws,
        doc,
        "create_layer",
        json!({"layer_id": "L", "kind": "vector"}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
}

fn create_raster_layer(ws: &mut Workspace, doc: &str) {
    // 种子走 `import_image`（光栅通路，矢量图层守卫会拦）⇒ 种子层必须是光栅层 ✓；
    // 生产 `draw_text` 写在矢量层，但导出侧的 Text 臂按对象遍历、与图层类型无关
    //（正常混合的图层都走逐对象路）⇒ 种子层类型不影响导出断言 ✓。
    let created = call(
        ws,
        doc,
        "create_layer",
        json!({"layer_id": "L", "kind": "raster"}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
}

#[test]
fn text_exports_as_real_text_element() {
    let doc = "doc_text_prov_1";
    let (mut ws, guard) = workspace(doc);
    create_raster_layer(&mut ws, doc);
    let drawn = seed_text(
        &mut ws,
        doc,
        "t1",
        "Hello",
        40.0,
        [200, 60, 40, 255],
        (20, 20),
        "left",
        true,
        None,
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    // 溯源真的落进对象数据（`get_object` 读回 ⇒ 字段全 ✓）。
    let got = call(&mut ws, doc, "get_object", json!({"object_id": "t1"}));
    assert_eq!(got["ok"], json!(true), "{got}");
    let prov = &got["data"]["text_provenance"];
    assert_eq!(prov["text"], json!("Hello"), "{prov}");
    assert_eq!(prov["font"], json!("Inter"), "{prov}");
    assert_eq!(prov["anchor"], json!("start"), "{prov}");
    assert!(
        prov["subset_blob"]
            .as_str()
            .is_some_and(|s| s.starts_with("sha256:")),
        "子集 blob 哈希应为 sha256:<hex>：{prov}"
    );
    assert!(
        prov["fallback_reason"].is_null(),
        "有子集时不应有回退原因：{prov}"
    );

    let (receipt, text) = export_svg(&mut ws, doc, &guard.0, "prov1.svg");
    assert_eq!(receipt["texts_vector"], json!(1), "{receipt}");
    assert_eq!(receipt["texts_raster_fallback"], json!(0), "{receipt}");
    assert_eq!(receipt["texts_builtin_raster"], json!(0), "{receipt}");
    assert_eq!(receipt["images_embedded"], json!(0), "{receipt}");
    assert!(text.contains("<text "), "应有 <text> 元素");
    assert!(text.contains("@font-face"), "应内嵌 @font-face");
    assert!(text.contains("Yanshi-"), "族名应为 Yanshi-<sha8>");
    assert!(text.contains(">Hello</text>"), "文本内容应在 <text> 里");
    assert!(
        text.contains("text-anchor=\"start\""),
        "左对齐应为 start 锚点"
    );
    assert!(!text.contains("<image"), "真文本对象不应再嵌位图");
    // **变异**：把 Text 臂改成永远回退 ⇒ `texts_vector` 变 0、`images_embedded` 变 1 ⇒ 本单测红 ✓。

    // 两次导出逐字节一致（确定性 ✓）。
    let path2 = guard.0.join("prov1b.svg");
    let receipt2 = call(
        &mut ws,
        doc,
        "export_svg",
        json!({"path": path2.to_string_lossy()}),
    );
    assert_eq!(receipt2["ok"], json!(true), "{receipt2}");
    assert_eq!(
        std::fs::read(guard.0.join("prov1.svg")).unwrap(),
        std::fs::read(&path2).unwrap(),
        "两次导出的 SVG 必须逐字节一致"
    );
}

#[test]
fn same_subset_is_embedded_only_once() {
    let doc = "doc_text_prov_2";
    let (mut ws, guard) = workspace(doc);
    create_raster_layer(&mut ws, doc);
    for (id, pos) in [("t1", (20, 20)), ("t2", (20, 80))] {
        let drawn = seed_text(
            &mut ws,
            doc,
            id,
            "Hello",
            40.0,
            [200, 60, 40, 255],
            pos,
            "left",
            true,
            None,
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let (receipt, text) = export_svg(&mut ws, doc, &guard.0, "prov2.svg");
    assert_eq!(receipt["texts_vector"], json!(2), "{receipt}");
    assert_eq!(
        text.matches("@font-face").count(),
        1,
        "同一子集只应嵌一次 @font-face"
    );
    assert_eq!(text.matches("<text ").count(), 2, "两个对象各出一行 <text>");
    // **变异**：去重表改成每次都插 ⇒ `@font-face` 变 2 ⇒ 本单测红 ✓。
}

#[test]
fn text_with_xml_special_chars_is_escaped() {
    let doc = "doc_text_prov_3";
    let (mut ws, guard) = workspace(doc);
    create_raster_layer(&mut ws, doc);
    let drawn = seed_text(
        &mut ws,
        doc,
        "t1",
        "A&B <C>",
        40.0,
        [200, 60, 40, 255],
        (20, 20),
        "left",
        true,
        None,
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let (_, text) = export_svg(&mut ws, doc, &guard.0, "prov3.svg");
    assert!(
        text.contains("A&amp;B &lt;C&gt;"),
        "特殊字符必须转义：{text}"
    );
    assert!(!text.contains("A&B"), "裸 & 不应出现在 SVG 里");
    assert!(!text.contains("<C>"), "裸 <C> 不应出现在 SVG 里");
    // **变异**：把转义 helper 换成原样输出 ⇒ 裸字符断言红 ✓。
}

#[test]
fn fallback_without_subset_renders_bitmap_with_reason() {
    let doc = "doc_text_prov_4";
    let (mut ws, guard) = workspace(doc);
    create_raster_layer(&mut ws, doc);
    let drawn = seed_text(
        &mut ws,
        doc,
        "t1",
        "Hello",
        40.0,
        [200, 60, 40, 255],
        (20, 20),
        "left",
        false,
        Some("font_license_unknown"),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let (receipt, text) = export_svg(&mut ws, doc, &guard.0, "prov4.svg");
    assert_eq!(receipt["texts_vector"], json!(0), "{receipt}");
    assert_eq!(receipt["texts_raster_fallback"], json!(1), "{receipt}");
    assert_eq!(receipt["images_embedded"], json!(1), "{receipt}");
    assert_eq!(
        receipt["raster_reasons"]["font_license_unknown"],
        json!(1),
        "{receipt}"
    );
    assert!(text.contains("<image"), "回退时应嵌位图");
    assert!(!text.contains("<text "), "回退时不应出 <text>");
    let warnings = receipt["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .is_some_and(|s| s.contains("t1") && s.contains("font_license_unknown"))),
        "警告应点名对象 + 原因：{warnings:?}"
    );
    // **变异**：把回退分支改成静默 pending（不计数）⇒ `texts_raster_fallback` 变 0 ⇒ 红 ✓。
}

#[test]
fn builtin_text_stays_raster_with_its_own_count() {
    let doc = "doc_text_prov_5";
    let (mut ws, guard) = workspace(doc);
    create_vector_layer(&mut ws, doc);
    let drawn = call(
        &mut ws,
        doc,
        "draw_text",
        json!({"layer_id": "L", "object_id": "t1",
               "data": {"text": "AB", "font": "builtin", "size": 21.0,
                        "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                        "position": [8.0, 8.0], "align": "left"}}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let (receipt, text) = export_svg(&mut ws, doc, &guard.0, "prov5.svg");
    assert_eq!(receipt["texts_builtin_raster"], json!(1), "{receipt}");
    assert_eq!(receipt["texts_vector"], json!(0), "{receipt}");
    assert_eq!(receipt["texts_raster_fallback"], json!(0), "{receipt}");
    assert_eq!(receipt["images_embedded"], json!(1), "{receipt}");
    assert!(text.contains("<image"), "builtin 文本应继续走位图");
    assert!(!text.contains("<text "), "builtin 文本本轮不应出 <text>");
}

#[test]
fn plain_bitmap_without_provenance_is_untouched() {
    let doc = "doc_text_prov_6";
    let (mut ws, guard) = workspace(doc);
    create_raster_layer(&mut ws, doc);
    // 不带溯源的普通位图导入（旧文档同形 ✓）。
    let mut db = fontdb::Database::new();
    db.load_fonts_dir(repo_fonts_dir());
    let layout = yanshi_server::fonts::layout_text(&db, "Hello", "Inter", 40.0, "left")
        .expect("Inter 应能排字");
    let (width, height, rgba) = yanshi_server::fonts::rasterize_layout(&layout, [200, 60, 40, 255]);
    let bitmap_hash = ws.put_blob(&rgba).expect("位图入库");
    let imported = call(
        &mut ws,
        doc,
        "import_image",
        json!({
            "layer_id": "L",
            "object_id": "t1",
            "bitmap": {"blob_hash": bitmap_hash, "size": rgba.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 20, "y": 20, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let (receipt, text) = export_svg(&mut ws, doc, &guard.0, "prov6.svg");
    assert_eq!(receipt["texts_vector"], json!(0), "{receipt}");
    assert_eq!(receipt["texts_raster_fallback"], json!(0), "{receipt}");
    assert_eq!(receipt["texts_builtin_raster"], json!(0), "{receipt}");
    assert_eq!(receipt["images_embedded"], json!(1), "{receipt}");
    assert!(text.contains("<image"), "无溯源位图应继续走位图");
    assert!(!text.contains("<text "), "无溯源位图不应出 <text>");
}
