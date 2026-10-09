//! **★ 默认带数要**夹住**，**而**显式设置必须被尊重 ✗ ★**（第 23 轮 ✓；**阶段一第 5 条的落地判据 ✓**）。
//!
//! **∴ 为什么这条判据**不看墙钟** ✗ ★**：**墙钟**在**共享机器**上**会漂**✗
//!   ⇒ **∴ 而**"**实际用了几条带**"**是**确定的整数 ✓**（`RenderStats::parallel_chunks` ✓）**
//!     ⇒ **★ 所以**：**本判据**只断言带数**✗ ⇒ **∴ 它**永不 flaky ✓ ★**** ✓✓
//!
//! **∴ 判据背后的实测（**release ／ 4K ／ 4 层 ／ 本机 4 核 ✓）★**：
//! ```text
//! 1 带：墙钟 2818.2｜CPU 2770｜小页错误 341239｜VmHWM 1.09 GB
//! 2 带：墙钟 2159.8／2169.4／2175.6｜CPU 3670／3620／3630｜VmHWM 1.42 GB
//! 3 带：墙钟 2217.1｜CPU 4780
//! 4 带：墙钟 2395.5／2204.3｜CPU 5070／4770｜VmHWM 1.48 GB
//! ```
//! ⇒ **∴ 2 带在**墙钟／CPU／驻留**三项上都最好 ✗ ⇒ **∴ 而 3／4 带**CPU 高约 31% ✓**
//!   ⇒ **∴ 且**已排除「**池预算少算 ✓」这个嫌疑（**§21.2 ✓）⇒ **∴ 主因是**内存带宽／L3 争用 ✓**** ✓✓
//!
//! **∴ 变异 ✗ ★**：**把 `workers()` 里的 `.min(DEFAULT_BAND_CAP)` 去掉** ✗
//!   ⇒ **∴ 第一条断言**必红 ✓**（**本机 ≥ 3 核时 `parallel_chunks > 2` ✓）** ✓✓
//!
//! **∴ 借鉴对应 ✗**：**Krita 的**LoD／分块**与 **GIMP 的**tile 化**都表明 ✗**
//!   **并发粒度**不是**越细越好**✗（**∴ 内存带宽**是**共同约束 ✓）** ✓✓

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::render::DEFAULT_BAND_CAP;
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

fn object(id: &str, layer_id: &str, z: i64, data: serde_json::Value) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: ObjectType::Shape,
        z_index: z,
        visible: true,
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

/// **∴ 画布要**足够大** ✗**：**并行门槛是 `128 × 128` 像素 ✗**（`PARALLEL_MIN_PIXELS` ✓）
///   ⇒ **∴ 用 1024×512 ＝ 524288 像素**✗ ⇒ **∴ 远超门槛 ⇒ **∴ 会走并行路径 ✓**** ✓✓
fn document() -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_bands".to_owned());
    state.width = 1024;
    state.height = 512;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    let content = json!({"geometry": {"kind": "rect",
                                      "bbox": {"x": 8.0, "y": 8.0, "w": 900.0, "h": 400.0}},
                         "color": {"r": 20, "g": 40, "b": 80, "a": 200}});
    for (id, z) in [("layer_a", 0_i64), ("layer_b", 1)] {
        state.layers.insert(id.to_owned(), layer(id, z));
        state.objects.insert(
            format!("rect_{id}"),
            object(&format!("rect_{id}"), id, 0, content.clone()),
        );
    }
    state
}

/// **★ 判据 ✗ ★**：
///   **① 默认 ⇒ 带数 ≤ `DEFAULT_BAND_CAP`** ✗**（**实测最优 ✓）**
///   **② 显式 4 ⇒ 必须**真的用 4** ✗**（**用户的明确选择**不许**被默认值吃掉 ✓）** ✓✓
#[test]
fn default_band_count_is_capped_and_explicit_is_honoured() {
    let state = document();
    let store = MemoryBlobStore::new();
    let region = yanshi_core::Bbox::new(0.0, 0.0, 1024.0, 512.0);

    // **① 默认。**
    let mut default_renderer =
        Renderer::new(TileGrid::new(256, state.width, state.height).unwrap());
    let default_out = default_renderer
        .render_region(&state, &store, region)
        .expect("默认渲染");
    eprintln!(
        "默认：parallel_chunks={}（上限 {DEFAULT_BAND_CAP}）",
        default_out.stats.parallel_chunks
    );
    assert!(
        default_out.stats.parallel_chunks >= 1,
        "**默认**至少要**一条带** ✗；实测 0 ⇒ **∴ 这次渲染**没走并行路径 ✓（**∴ 要么**区域太小 ✗，\
         **要么**判据构造不对 ✓）"
    );
    assert!(
        default_out.stats.parallel_chunks <= DEFAULT_BAND_CAP,
        "**默认带数**必须 ≤ {DEFAULT_BAND_CAP} ✗（**实测 2 带在**墙钟／CPU／驻留**三项上都最好 ✓）；\
         实测 {} ⇒ **∴ 要么**夹取被去掉了 ✗，**要么**机器只有更少的核 ✓",
        default_out.stats.parallel_chunks
    );

    // **② 显式 4 ⇒ 必须被尊重。**
    let mut explicit_renderer =
        Renderer::new(TileGrid::new(256, state.width, state.height).unwrap()).with_max_workers(4);
    let explicit_out = explicit_renderer
        .render_region(&state, &store, region)
        .expect("显式 4 渲染");
    eprintln!(
        "显式 4：parallel_chunks={}",
        explicit_out.stats.parallel_chunks
    );
    assert_eq!(
        explicit_out.stats.parallel_chunks, 4,
        "**调用方显式设的带数**必须被尊重 ✗；实测 {} ⇒ **∴ 说明**夹取**误伤了**显式设置 ✓",
        explicit_out.stats.parallel_chunks
    );
    // **∴ 且**：**显式路径**的像素必须与默认**逐字节相同** ✗（**∴ 带数**只影响**怎么算** ✓）** ✓✓
    assert_eq!(
        default_out.rgba8.len(),
        explicit_out.rgba8.len(),
        "两次渲染的像素数必须相同 ✓"
    );
    assert_eq!(
        default_out.rgba8, explicit_out.rgba8,
        "**带数**只能影响**怎么算** ✗，**绝不能**影响**算出什么** ✓\
         （**∴ 与 CPU 真值逐字节一致 ✓）"
    );
}
