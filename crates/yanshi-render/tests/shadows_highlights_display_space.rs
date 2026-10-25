//! **回归：`shadows_highlights` 的渲染结果必须是「显示空间单次 `toward()` 数学」**。
//!
//! **缺陷（实测定位）**：`shadows_highlights` 的掩码权重与 `toward()` 幅度是按
//! **显示参照值**（sRGB 编码值，用户看到的 0..1）标定的，但旧实现直接把它们作用在
//! **线性光**缓冲上。线性域里 sRGB 0.5 只对应 0.214、0.75 只对应 0.522 ⇒ 照片的主体
//! 色调全挤在低线性区、暗部权重 `(1−L)²` 对几乎全图 ≈ 1、抬升量被成倍放大，
//! 编码回显示后全图堆成近常数灰：
//!   * 256 级灰渐变 + `shadows=0.6`：基线输出样本 `[149,149,151,154,162,176,197,225,255]`、
//!     std **31.43**（原图 std 73.90）；单次显示空间数学应为 `[76,83,96,115,137,164,193,224,255]`、
//!     std **54.63**。
//!   * 同渐变 + `shadows=0.15`：基线黑端被抬到 **77**（显示值 0.30）；
//!     文档数学只应抬到 `0.15×0.5 = 0.075` ⇒ **19**。
//!   * 排除「重复施加」：基线输出与「线性域**单次**施加」模型逐点误差 0（std 31.44 对
//!     实测 31.43），与「线性域两次」模型差达 21 ⇒ 每像素恰好施加了一次，病在作用空间。
//!
//! 本文件三条判据（都断**数值**，不断「不报错」）：
//! ① 样本像素与独立复算的显示空间单次公式在 ±2 LSB 内一致；
//! ② 输出 std 不塌（`shadows=0.6` 时 ≥ 45，期望 ≈54.6，基线 31.4 为红）；
//! ③ 小参数 `shadows=0.15` 的黑端抬升 ≈19（基线 77 为红）。

use serde_json::json;
use yanshi_core::blob::{BlobStore, MemoryBlobStore};
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::{Renderer, TileGrid};

const W: u32 = 256;
const H: u32 = 64;
const SAMPLE_X: [usize; 9] = [0, 32, 64, 96, 128, 160, 192, 224, 255];

/// 独立复算的期望值：显示空间（0..1）里的一次 `toward()` 数学，
/// 公式抄自 `filter.rs` 的**文档口径**（权重按明度、幅度 = 参数×权重×剩余距离×0.5），
/// 不调用被测实现 ⇒ 实现改坏时这里不会跟着改坏。
fn expected_display(v: f64, shadows: f64, highlights: f64) -> f64 {
    let toward = |amount: f64, weight: f64, value: f64| {
        let room = if amount >= 0.0 { 1.0 - value } else { value };
        amount * weight * room * 0.5
    };
    let shadow_weight = (1.0 - v) * (1.0 - v);
    let highlight_weight = v * v;
    (v + toward(shadows, shadow_weight, v) + toward(highlights, highlight_weight, v))
        .clamp(0.0, 1.0)
}

fn layer(id: &str, z: i64) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: z,
        blend_mode: "normal".to_owned(),
        opacity: 1.0,
        visible: true,
        locked: false,
        alpha_lock: false,
        clipping_mask: false,
        mask_id: None,
        transform: Transform::IDENTITY,
        medium: None,
        style: None,
        metadata: serde_json::Value::Null,
        blobs: Vec::new(),
        created_by: "a1".to_owned(),
        updated_by: None,
        deleted_by: None,
    }
}

fn object(id: &str, ty: ObjectType, z: i64, data: serde_json::Value) -> Object {
    Object {
        seq: 0,
        id: id.to_owned(),
        layer_id: "layer_1".to_owned(),
        object_type: ty,
        z_index: z,
        visible: true,
        enabled: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec!["a1".to_owned()],
        current_version: Some("a1".to_owned()),
        created_by: "a1".to_owned(),
        deleted_by: None,
        data,
        blobs: Vec::new(),
    }
}

/// 256 级灰渐变文档（raw 位图补丁）＋一个 `shadows_highlights` 调整对象，整幅渲染，
/// 返回首行灰值（u8）与全行 std。
fn render_gradient(shadows: f64, highlights: f64) -> (Vec<u8>, f64) {
    let mut rgba = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let base = ((y * W + x) * 4) as usize;
            let v = (x * 255 / (W - 1)) as u8;
            rgba[base] = v;
            rgba[base + 1] = v;
            rgba[base + 2] = v;
            rgba[base + 3] = 255;
        }
    }
    let store = MemoryBlobStore::new();
    let blob = store.put(&rgba).unwrap();
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_shadows_regression".to_owned());
    state.width = W;
    state.height = H;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state
        .layers
        .insert("layer_1".to_owned(), layer("layer_1", 0));
    state.objects.insert(
        "obj_img".to_owned(),
        object(
            "obj_img",
            ObjectType::RasterPatch,
            0,
            json!({
                "bitmap": {"blob_hash": blob.to_string(), "mime_type": "image/x-yanshi-raw"},
                "region": {"x": 0, "y": 0, "w": W, "h": H},
                "width": W,
                "height": H,
            }),
        ),
    );
    state.objects.insert(
        "obj_adj".to_owned(),
        object(
            "obj_adj",
            ObjectType::Adjustment,
            5,
            json!({
                "adjustment_type": "shadows_highlights",
                "params": {"shadows": shadows, "highlights": highlights},
            }),
        ),
    );
    let out = Renderer::new(TileGrid::new(256, W, H).unwrap())
        .render_document(&state, &store)
        .unwrap();
    let row: Vec<u8> = (0..W as usize).map(|x| out.rgba8[x * 4]).collect();
    let mean = row.iter().map(|v| *v as f64).sum::<f64>() / row.len() as f64;
    let var = row
        .iter()
        .map(|v| {
            let d = *v as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / row.len() as f64;
    (row, var.sqrt())
}

fn assert_samples(row: &[u8], shadows: f64, highlights: f64) {
    for x in SAMPLE_X {
        let expected = (expected_display(x as f64 / 255.0, shadows, highlights) * 255.0).round();
        let got = row[x] as f64;
        assert!(
            (got - expected).abs() <= 2.0,
            "x={x} 应为显示空间单次值 {expected:.0}（±2），实得 {got:.0} \
             （shadows={shadows}, highlights={highlights}）"
        );
    }
}

/// ①＋②：`shadows=0.6` 的样本值 = 显示空间单次公式，且 std 不塌。
#[test]
fn shadows_06_matches_single_display_application_and_keeps_range() {
    let (row, std) = render_gradient(0.6, 0.0);
    assert_samples(&row, 0.6, 0.0);
    assert!(
        std >= 45.0,
        "shadows=0.6 后渐变 std 应保持（期望 ≈54.6），实得 {std:.2} ⇒ 动态范围被压塌"
    );
}

/// ③：`shadows=0.15` 只把黑端抬到约 0.075（字节 ≈19），不许漂到 ~79。
#[test]
fn shadows_015_lifts_black_only_slightly() {
    let (row, _std) = render_gradient(0.15, 0.0);
    assert_samples(&row, 0.15, 0.0);
    let black = row[0] as f64;
    assert!(
        (black - 19.0).abs() <= 2.0,
        "shadows=0.15 的黑端应为 ≈19（0.075×255），实得 {black:.0}"
    );
}

/// 高光分支同查：`highlights=-0.6` 的样本值 = 显示空间单次公式
/// （基线在线性域算时白端只压到 ≈218，期望 178 ⇒ 基线为红）。
#[test]
fn highlights_minus_06_matches_single_display_application() {
    let (row, _std) = render_gradient(0.0, -0.6);
    assert_samples(&row, 0.0, -0.6);
}
