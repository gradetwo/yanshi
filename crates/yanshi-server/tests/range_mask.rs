//! D1 集成测试：范围遮罩（亮度/颜色）+ 渐变遮罩（线性/径向）——走真实工具注册表 + 落盘工作区。
//!
//! 与单元测试的分工：单元测试钉住烘焙公式与渐变 coverage；这里钉住工具落成
//! ——烘焙落库、挂接副本层调整后的渲染门控、非法参数零原子。门控测试用
//! 试点 LZ 范式（照片副本层 + 同层调整 + 层蒙版），因调整对象只作用于
//! 同图层中位于其下方的内容，独立调整层是空操作。
//! 断言一律用关系量（选区内外均值之差、单调性、零原子），不断单点像素数。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **⇒ 临时目录必须在测试结束时删掉** ✓（第 930 轮；`check-tests-clean-temp` 抓到的 ✓）
pub struct TempCleanup(pub std::path::PathBuf);
impl Drop for TempCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_range_{}_{}", name, std::process::id()));
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

fn full_bbox(width: u32, height: u32) -> Bbox {
    Bbox::new(0.0, 0.0, width as f64, height as f64)
}

fn head_seq(workspace: &Workspace, doc: &str) -> u64 {
    workspace.document(doc).unwrap().state().head_seq
}

/// 灰度水平斜坡：x=0 黑 → x=w-1 白（R=G=B），全不透明。
fn gray_ramp(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let v = ((x * 255) / (width as usize - 1)) as u8;
            pixels[base] = v;
            pixels[base + 1] = v;
            pixels[base + 2] = v;
            pixels[base + 3] = 255;
        }
    }
    pixels
}

/// 平灰图：每像素同一灰度。
fn flat_gray(width: u32, height: u32, v: u8) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for px in pixels.chunks_exact_mut(4) {
        px[0] = v;
        px[1] = v;
        px[2] = v;
        px[3] = 255;
    }
    pixels
}

/// 左红右绿：左半 (255,0,0)，右半 (0,255,0)，全不透明。
fn red_green_halves(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            if x < width as usize / 2 {
                pixels[base] = 255;
            } else {
                pixels[base + 1] = 255;
            }
            pixels[base + 3] = 255;
        }
    }
    pixels
}

fn import_pixels(
    workspace: &mut Workspace,
    doc: &str,
    layer: &str,
    pixels: &[u8],
    width: u32,
    height: u32,
) {
    let hash = workspace.put_blob(pixels).expect("放源图 blob");
    let imported = call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": layer,
            "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 0, "y": 0, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
}

fn setup_source(
    workspace: &mut Workspace,
    doc: &str,
    width: u32,
    height: u32,
    pixels: &[u8],
) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    import_pixels(workspace, doc, "src", pixels, width, height);
    "src".to_owned()
}

/// 副本层（试点 LZ 范式）：把同一张源图再导一份到新图层（在源图上方），
/// 在副本层上挂曝光调整对象，蒙版挂副本层。
/// 背景：调整对象只作用于**同图层中位于其下方**的内容（设计文档
/// implementation-notes.md），独立调整层是空缓冲上的空操作；蒙版遮罩
/// 的是整层输出——所以"带蒙版的局部调整"在本引擎里的正确结构是照片
/// 副本层 + 同层调整 + 层蒙版：蒙版外露原图、蒙版内露调整后的副本。
fn setup_duplicate_adjustment_layer(
    workspace: &mut Workspace,
    doc: &str,
    pixels: &[u8],
    width: u32,
    height: u32,
    ev: f64,
) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "dup", "name": "副本"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    import_pixels(workspace, doc, "dup", pixels, width, height);
    let adj = call(
        workspace,
        doc,
        "add_adjustment",
        json!({
            "layer_id": "dup",
            "adjustment_type": "exposure",
            "params": {"ev": ev},
            "name": "曝光",
        }),
    );
    assert_eq!(adj["ok"], json!(true), "{adj}");
    "dup".to_owned()
}

fn attach_mask(workspace: &mut Workspace, doc: &str, layer: &str, mask: &str) {
    let attached = call(
        workspace,
        doc,
        "set_property",
        json!({
            "layer_id": layer, "key": "mask_id", "value": mask,
        }),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
}

/// RGBA 渲染的矩形区平均 R 值（0..255；测试图均为灰度，三通道相等）。
fn rect_mean(rgba: &[u8], width: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> f64 {
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let base = (y as usize * width as usize + x as usize) * 4;
            sum += u64::from(rgba[base]);
            n += 1;
        }
    }
    sum as f64 / n as f64
}

/// 亮度遮罩烘焙：灰度斜坡上暗部 0–0.5 段约一半像素被选中，且回执带有效参数。
#[test]
fn range_mask_luminance_bakes_dark_band() {
    let root = temp_dir("lum");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_lum";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &gray_ramp(width, height),
    );
    let result = call(
        &mut workspace,
        doc,
        "create_range_mask",
        json!({"mask_id": "m1", "layer_id": src, "range": "luminance", "low": 0.0, "high": 0.5}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["baked"]["low"], json!(0.0), "{result}");
    assert_eq!(result["baked"]["high"], json!(0.5), "{result}");
    let ratio = result["coverage_matched_ratio"]
        .as_f64()
        .expect("回执须带 coverage_matched_ratio");
    let mean = result["coverage_mean"]
        .as_f64()
        .expect("回执须带 coverage_mean");
    // 斜坡一半暗一半亮：选中比约 0.55（0.5 带 + 0.05 平滑边），均值约 0.5。
    assert!(
        (0.45..0.65).contains(&ratio),
        "暗部带选中比应在 (0.45, 0.65) 内，实测 {ratio}"
    );
    assert!(
        (0.4..0.6).contains(&mean),
        "覆盖均值应在 (0.4, 0.6) 内，实测 {mean}"
    );
    assert!(
        mean < ratio,
        "平滑边使均值小于选中比（部分像素为中间值）：mean={mean} ratio={ratio}"
    );
}

/// 颜色遮罩烘焙：左红右绿图上 hue=0 选中左半；灰像素不参与色相选。
#[test]
fn range_mask_color_bakes_red_band() {
    let root = temp_dir("color");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_color";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &red_green_halves(width, height),
    );
    let result = call(
        &mut workspace,
        doc,
        "create_range_mask",
        json!({"mask_id": "m1", "layer_id": src, "range": "color", "hue": 0.0, "hue_width": 60.0}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    let ratio = result["coverage_matched_ratio"]
        .as_f64()
        .expect("回执须带 coverage_matched_ratio");
    assert!(
        (0.4..0.6).contains(&ratio),
        "红带选中比应在 (0.4, 0.6) 内（左半），实测 {ratio}"
    );
}

/// 非法参数 ⇒ 拒绝 + 零原子落账（每种错各验一次，错误码与原因都要点名）。
#[test]
fn range_mask_rejects_bad_params_without_atoms() {
    let root = temp_dir("reject");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_reject";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &gray_ramp(width, height),
    );
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"mask_id": "m1", "layer_id": src, "range": "luminance", "low": 0.6, "high": 0.4}),
            "low 必须 ≤ high",
        ),
        (
            json!({"mask_id": "m1", "layer_id": src, "range": "depth"}),
            "range 只收 luminance / color",
        ),
        (
            json!({"mask_id": "m1", "layer_id": src, "range": "color", "hue_width": 60.0}),
            "hue 是必填参数",
        ),
        (
            json!({"mask_id": "m1", "layer_id": src, "range": "color", "hue": 0.0, "hue_width": 0.0}),
            "hue_width 必须大于 0",
        ),
    ];
    for (args, reason) in cases {
        let before = head_seq(&workspace, doc);
        let result = call(&mut workspace, doc, "create_range_mask", args);
        assert_eq!(result["ok"], json!(false), "应当拒绝：{result}");
        assert_eq!(
            result["error_code"],
            json!("invalid_argument"),
            "拒绝码应为 invalid_argument：{result}"
        );
        assert!(
            result.to_string().contains(reason),
            "错误要说清「{reason}」：{result}"
        );
        let after = head_seq(&workspace, doc);
        assert_eq!(
            before, after,
            "拒绝时零原子落账：head_seq {before} → {after}"
        );
    }
    // 不存在的源图层 ⇒ reference_not_found，同样零原子。
    let before = head_seq(&workspace, doc);
    let result = call(
        &mut workspace,
        doc,
        "create_range_mask",
        json!({"mask_id": "m1", "layer_id": "no_such_layer", "range": "luminance"}),
    );
    assert_eq!(result["ok"], json!(false), "不存在的图层必须拒绝：{result}");
    assert_eq!(
        result["error_code"],
        json!("reference_not_found"),
        "拒绝码应为 reference_not_found：{result}"
    );
    assert!(
        result.to_string().contains("图层 no_such_layer 不存在"),
        "错误要说清缺什么：{result}"
    );
    assert_eq!(before, head_seq(&workspace, doc), "拒绝时零原子落账");
    // 空层 ⇒ precondition_failed，同样零原子。
    let empty = call(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "empty", "name": "空层"}),
    );
    assert_eq!(empty["ok"], json!(true), "{empty}");
    let before = head_seq(&workspace, doc);
    let result = call(
        &mut workspace,
        doc,
        "create_range_mask",
        json!({"mask_id": "m1", "layer_id": "empty", "range": "luminance"}),
    );
    assert_eq!(result["ok"], json!(false), "空层必须拒绝：{result}");
    assert_eq!(
        result["error_code"],
        json!("precondition_failed"),
        "空层拒绝码应为 precondition_failed：{result}"
    );
    assert_eq!(before, head_seq(&workspace, doc), "空层拒绝时零原子落账");
}

/// 亮度遮罩门控曝光：暗部提亮、亮部不动（挂接调整层的端到端）。
#[test]
fn range_mask_luminance_gates_exposure() {
    let root = temp_dir("gate_lum");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_gate_lum";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &gray_ramp(width, height),
    );
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("基准渲染");
    let adj = setup_duplicate_adjustment_layer(
        &mut workspace,
        doc,
        &gray_ramp(width, height),
        width,
        height,
        1.0,
    );
    let masked = call(
        &mut workspace,
        doc,
        "create_range_mask",
        json!({"mask_id": "m1", "layer_id": src, "range": "luminance", "low": 0.0, "high": 0.5}),
    );
    assert_eq!(masked["ok"], json!(true), "{masked}");
    attach_mask(&mut workspace, doc, &adj, "m1");
    let (_, _, after) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("挂遮罩后渲染");
    // 暗区（左 1/4）提亮显著；亮区（右 1/4）几乎不动。
    let dark_before = rect_mean(&reference, width, 0, 0, 16, height);
    let dark_after = rect_mean(&after, width, 0, 0, 16, height);
    let bright_before = rect_mean(&reference, width, 48, 0, 64, height);
    let bright_after = rect_mean(&after, width, 48, 0, 64, height);
    assert!(
        dark_after - dark_before > 10.0,
        "暗区应提亮 > 10：{dark_before} → {dark_after}"
    );
    assert!(
        (bright_after - bright_before).abs() < 2.0,
        "亮区应几乎不动（< 2）：{bright_before} → {bright_after}"
    );
    assert!(
        dark_after - dark_before > (bright_after - bright_before).abs() * 5.0,
        "暗区提亮须远大于亮区漂移：暗 +{}、亮 {:+}",
        dark_after - dark_before,
        bright_after - bright_before
    );
}

/// 线性渐变遮罩门控曝光：上亮下不动，中部介于两者之间（渐变不是阶跃）。
#[test]
fn linear_gradient_mask_gates_exposure() {
    let root = temp_dir("gate_lin");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_gate_lin";
    let (width, height) = (64, 64);
    let mut workspace = workspace(&root, doc, width, height);
    setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &flat_gray(width, height, 128),
    );
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("基准渲染");
    let adj = setup_duplicate_adjustment_layer(
        &mut workspace,
        doc,
        &flat_gray(width, height, 128),
        width,
        height,
        1.0,
    );
    let masked = call(
        &mut workspace,
        doc,
        "create_mask",
        json!({"mask_id": "m1", "shape": {"kind": "linear_gradient", "start": [0.0, 0.0], "end": [0.0, 64.0]}}),
    );
    assert_eq!(masked["ok"], json!(true), "{masked}");
    attach_mask(&mut workspace, doc, &adj, "m1");
    let (_, _, after) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("挂遮罩后渲染");
    let top_before = rect_mean(&reference, width, 0, 0, width, 8);
    let top_after = rect_mean(&after, width, 0, 0, width, 8);
    let mid_before = rect_mean(&reference, width, 0, 28, width, 36);
    let mid_after = rect_mean(&after, width, 0, 28, width, 36);
    let bottom_before = rect_mean(&reference, width, 0, 56, width, 64);
    let bottom_after = rect_mean(&after, width, 0, 56, width, 64);
    let top_lift = top_after - top_before;
    let mid_lift = mid_after - mid_before;
    assert!(
        top_lift > 10.0,
        "顶部应提亮 > 10：{top_before} → {top_after}"
    );
    assert!(
        (bottom_after - bottom_before).abs() < 2.0,
        "底部应几乎不动（< 2）：{bottom_before} → {bottom_after}"
    );
    assert!(
        mid_lift > 5.0 && mid_lift < top_lift,
        "中部提亮应介于顶部与底部之间（渐变不是阶跃）：顶 +{top_lift}、中 +{mid_lift}"
    );
}

/// 径向渐变遮罩门控曝光：中心提亮、四角不动。
/// 判据必须能区分"真径向渐变"与"硬矩形回退"（径向形状带 bbox，
/// 旧 `coverage_from_shape_clipped` 会静默把它当矩形用）：bbox 角区
/// （框内、椭圆外）须不动，过渡带（d≈0.75）须为部分提亮（介于 0 与满量之间）。
#[test]
fn radial_gradient_mask_gates_exposure() {
    let root = temp_dir("gate_rad");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_gate_rad";
    let (width, height) = (64, 64);
    let mut workspace = workspace(&root, doc, width, height);
    setup_source(
        &mut workspace,
        doc,
        width,
        height,
        &flat_gray(width, height, 128),
    );
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("基准渲染");
    let adj = setup_duplicate_adjustment_layer(
        &mut workspace,
        doc,
        &flat_gray(width, height, 128),
        width,
        height,
        1.0,
    );
    let masked = call(
        &mut workspace,
        doc,
        "create_mask",
        json!({"mask_id": "m1", "shape": {"kind": "radial_gradient", "bbox": {"x": 8.0, "y": 8.0, "w": 48.0, "h": 48.0}, "falloff": 0.5}}),
    );
    assert_eq!(masked["ok"], json!(true), "{masked}");
    attach_mask(&mut workspace, doc, &adj, "m1");
    let (_, _, after) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("挂遮罩后渲染");
    let center_before = rect_mean(&reference, width, 28, 28, 36, 36);
    let center_after = rect_mean(&after, width, 28, 28, 36, 36);
    let corner_before = rect_mean(&reference, width, 0, 0, 8, 8);
    let corner_after = rect_mean(&after, width, 0, 0, 8, 8);
    assert!(
        center_after - center_before > 10.0,
        "中心应提亮 > 10：{center_before} → {center_after}"
    );
    assert!(
        (corner_after - corner_before).abs() < 2.0,
        "四角应几乎不动（< 2）：{corner_before} → {corner_after}"
    );
    // bbox 角区 (8..16, 8..16)：在框内、椭圆外（d≈1.18 > 1）⇒ 覆盖度 0；
    // 硬矩形回退会把它全提亮（+48），真渐变须不动。
    let box_corner_before = rect_mean(&reference, width, 8, 8, 16, 16);
    let box_corner_after = rect_mean(&after, width, 8, 8, 16, 16);
    assert!(
        (box_corner_after - box_corner_before).abs() < 2.0,
        "bbox 角区（框内椭圆外）应几乎不动（< 2）：{box_corner_before} → {box_corner_after}"
    );
    // 过渡带 (28..36, 48..52)：d≈0.75、falloff 0.5 ⇒ 覆盖度≈0.5 ⇒ 提亮≈+24；
    // 硬矩形回退给满量 +48、零覆盖给 0，两者都被本条拦住。
    let band_before = rect_mean(&reference, width, 28, 48, 36, 52);
    let band_after = rect_mean(&after, width, 28, 48, 36, 52);
    let band_lift = band_after - band_before;
    assert!(
        band_lift > 10.0 && band_lift < 40.0,
        "过渡带应为部分提亮（10 < 升 < 40）：{band_before} → {band_after}"
    );
}

/// `create_mask` 拒绝未知 kind 与坏的渐变参数，零原子落账。
/// 变异：白名单被删（未知 kind 落库）⇒ 本条红；feather 检查被删 ⇒ 本条红。
#[test]
fn create_mask_rejects_unknown_kind_and_bad_gradients() {
    let root = temp_dir("reject_mask");
    let _cleanup = TempCleanup(root.clone());
    let doc = "doc_reject_mask";
    let (width, height) = (64, 64);
    let mut workspace = workspace(&root, doc, width, height);
    let cases: Vec<(Value, &str)> = vec![
        (
            json!({"mask_id": "m1", "shape": {"kind": "nope", "bbox": {"x": 0, "y": 0, "w": 10, "h": 10}}}),
            "未知蒙版形状 kind",
        ),
        (
            json!({"mask_id": "m1", "shape": {"kind": "linear_gradient", "start": [0.0, 0.0], "end": [0.0, 64.0]}, "feather": 5.0}),
            "不收 feather",
        ),
        (
            json!({"mask_id": "m1", "shape": {"kind": "linear_gradient", "start": [3.0, 3.0], "end": [3.0, 3.0]}}),
            "start 与 end 重合",
        ),
        (
            json!({"mask_id": "m1", "shape": {"kind": "linear_gradient", "start": [0.0, 0.0]}}),
            "必须给 start/end",
        ),
        (
            json!({"mask_id": "m1", "shape": {"kind": "radial_gradient", "bbox": {"x": 0.0, "y": 0.0, "w": 48.0, "h": 48.0}, "falloff": 2.0}}),
            "falloff 必须在 [0, 1] 内",
        ),
        (
            json!({"mask_id": "m1", "shape": {"kind": "radial_gradient"}}),
            "必须给 bbox",
        ),
    ];
    for (args, reason) in cases {
        let before = head_seq(&workspace, doc);
        let result = call(&mut workspace, doc, "create_mask", args);
        assert_eq!(result["ok"], json!(false), "应当拒绝：{result}");
        assert_eq!(
            result["error_code"],
            json!("invalid_argument"),
            "拒绝码应为 invalid_argument：{result}"
        );
        assert!(
            result.to_string().contains(reason),
            "错误要说清「{reason}」：{result}"
        );
        let after = head_seq(&workspace, doc);
        assert_eq!(
            before, after,
            "拒绝时零原子落账：head_seq {before} → {after}"
        );
    }
}
