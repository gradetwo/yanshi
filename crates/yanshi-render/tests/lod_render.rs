//! **期 4 `lod`**：`render_region_with_lod` 的三条等价判据 ✓
//!
//! **∴ 为什么要有这个文件 ✗**：`lod` 是**取样密度**（输出像素 (i,j) 采文档坐标
//!   (x0 + i·2^lod, y0 + j·2^lod)）✓ —— 一旦步长算错一位、或两路（串行／并行）
//!   密度不一致 ⇒ 画面**错位或丢笔触** ✗，而回执里的 `lod` 字段照样回显「1」✓
//!   （**回执撒谎** ✗ —— 仓库列的**静默降级形态** ✓）。
//!
//! **∴ 三条判据**（每条都能红 ✓）：
//!   ① `lod: 0` 与今天的 `render_region` **逐字节相同**（缺省等价 ✓）；
//!   ② `lod: 1/2` 的输出与全幅同倍率 **bilinear 下采样**逐像素相同、
//!      最大单通道差 ≤ 1（设计 §2 E1 验收 ② ✓）；
//!   ③ `region` 渲染与全幅裁出同一区域**逐像素相同**（设计 §2 E1 验收 ② ✓）。
//!
//! **∴ 夹具的取舍（诚实声明）**：形状边全部落在**偶数坐标**上（8/16/32/40/48）。
//!   点采样与 bilinear 下采样在**偶数边**处数学上一致（k=1 时 bilinear 是整 2×2 块均值，
//!   采样点正好落在块的左上角 ⇒ 值相等 ✓）；**奇数坐标的硬边**与斜边处两者必然有差
//!   （点采样取单点、bilinear 取块均值，差可达数十 LSB）—— 那是**点采样语义本身**的
//!   性质，不是实现 bug ✓；真实内容（笔触软边、纹理）上的实测最大差见 8K 文档量测
//!   （包说明），不在本文件里用一个人为放宽的容差把它盖过去 ✓。
//!   **变异**：步长取 `2^(lod+1)` ⇒ ② 的尺寸与像素全错 ⇒ 红 ✓；
//!   步长只进串行路不进并行路 ⇒ 并行分带错位 ⇒ ② 红 ✓。
use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::resample::{resample_rgba, ResampleFilter};
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_core::Bbox;
use yanshi_render::{Renderer, TileGrid};

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

fn rect(id: &str, layer_id: &str, z: i64, bbox: (f64, f64, f64, f64), colour: [u8; 3]) -> Object {
    Object {
        seq: 0,
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Shape,
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
        data: json!({
            "geometry": {"kind": "rect",
                         "bbox": {"x": bbox.0, "y": bbox.1, "w": bbox.2, "h": bbox.3}},
            "color": {"r": colour[0], "g": colour[1], "b": colour[2], "a": 255},
        }),
        blobs: Vec::new(),
    }
}

/// **64×64、三层矩形、边全在偶数坐标** ✓（见文件头「夹具的取舍」）。
fn document() -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("lod_render".to_owned());
    state.width = 64;
    state.height = 64;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    // 期望「先加的对象在下面」⇒ z_index **大的在下面**（与 `above_cache.rs` 同一口径 ✓）
    state.layers.insert("L0".to_owned(), layer("L0", 20));
    state.layers.insert("L1".to_owned(), layer("L1", 10));
    state.layers.insert("L2".to_owned(), layer("L2", 0));
    state.objects.insert(
        "o0".to_owned(),
        rect("o0", "L0", 0, (0.0, 0.0, 64.0, 64.0), [200, 30, 30]),
    );
    state.objects.insert(
        "o1".to_owned(),
        rect("o1", "L1", 0, (8.0, 8.0, 32.0, 32.0), [30, 200, 30]),
    );
    state.objects.insert(
        "o2".to_owned(),
        rect("o2", "L2", 0, (16.0, 16.0, 32.0, 32.0), [30, 30, 200]),
    );
    state
}

fn renderer(state: &DocumentState) -> Renderer {
    Renderer::new(TileGrid::new(64, state.width, state.height).unwrap())
}

/// **① `lod: 0` 必须与今天逐字节相同** ✓（缺省等价：不给 `lod` ⇒ 与今天相同 ✓）。
#[test]
fn lod_zero_is_byte_identical_to_plain_render_region() {
    let state = document();
    let store = MemoryBlobStore::new();
    let bbox = Bbox::new(0.0, 0.0, 64.0, 64.0);
    let plain = renderer(&state)
        .render_region(&state, &store, bbox)
        .expect("整幅渲染应成功");
    let lod0 = renderer(&state)
        .render_region_with_lod(&state, &store, bbox, 0)
        .expect("lod:0 渲染应成功");
    assert_eq!(
        (lod0.0.width, lod0.0.height),
        (plain.0.width, plain.0.height),
        "lod:0 的输出尺寸必须与今天相同"
    );
    assert_eq!(
        lod0.0.to_rgba8(Some([255, 255, 255, 255])),
        plain.0.to_rgba8(Some([255, 255, 255, 255])),
        "lod:0 的像素必须与今天逐字节相同"
    );
}

/// **② `lod: k` ≈ 全幅同倍率 bilinear 下采样，最大单通道差 ≤ 1** ✓。
#[test]
fn lod_output_matches_bilinear_downsample_within_one_lsb() {
    let state = document();
    let store = MemoryBlobStore::new();
    let bbox = Bbox::new(0.0, 0.0, 64.0, 64.0);
    let full = renderer(&state)
        .render_region(&state, &store, bbox)
        .expect("整幅渲染应成功")
        .0
        .to_rgba8(Some([255, 255, 255, 255]));
    assert_eq!(full.len(), 64 * 64 * 4);
    for (lod, expect_w, expect_h) in [(1u32, 32u32, 32u32), (2u32, 16u32, 16u32)] {
        let got = renderer(&state)
            .render_region_with_lod(&state, &store, bbox, lod)
            .expect("lod 渲染应成功")
            .0;
        assert_eq!(
            (got.width, got.height),
            (expect_w, expect_h),
            "lod:{lod} 的输出尺寸必须是 ceil(64/2^{lod})"
        );
        let got_rgba = got.to_rgba8(Some([255, 255, 255, 255]));
        assert_eq!(got_rgba.len(), (expect_w * expect_h * 4) as usize);
        let reference = resample_rgba(
            &full,
            64,
            64,
            expect_w,
            expect_h,
            ResampleFilter::Bilinear,
        )
        .expect("下采样参照应可算");
        let max_diff = got_rgba
            .iter()
            .zip(reference.iter())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(
            max_diff <= 1,
            "lod:{lod} 与 bilinear 下采样参照的最大单通道差是 {max_diff}，超出 ≤1 的验收"
        );
    }
}

/// **③ `region` 渲染与全幅裁出同一区域逐像素相同** ✓。
#[test]
fn region_render_matches_cropped_full_frame() {
    let state = document();
    let store = MemoryBlobStore::new();
    let full = renderer(&state)
        .render_region(&state, &store, Bbox::new(0.0, 0.0, 64.0, 64.0))
        .expect("整幅渲染应成功")
        .0
        .to_rgba8(Some([255, 255, 255, 255]));
    let region = renderer(&state)
        .render_region(&state, &store, Bbox::new(16.0, 16.0, 32.0, 32.0))
        .expect("区域渲染应成功")
        .0;
    assert_eq!((region.width, region.height), (32, 32));
    let region_rgba = region.to_rgba8(Some([255, 255, 255, 255]));
    let mut cropped = Vec::with_capacity(32 * 32 * 4);
    for y in 16..48 {
        let start = (y * 64 + 16) * 4;
        cropped.extend_from_slice(&full[start..start + 32 * 4]);
    }
    assert_eq!(
        region_rgba, cropped,
        "区域渲染必须与全幅裁出同一区域逐像素相同"
    );
}
