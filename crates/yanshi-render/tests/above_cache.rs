//! **`above` 缓存命中时必须跳过上方那些层**（第 714 轮；外部审计 R-01 第 ① 项）
//!
//! **⇒ 缺陷**：`if above_ready && index_here > split` 被嵌在
//!   `if reused && index_here < split` **里面** ⇒ `index_here` 不可能同时 `<` 与 `>` `split`
//!   ⇒ **内层判断永远不可达** ✗ ⇒ `above` 命中之后**上方那些层照样被重渲** ✓（白算 ✓）。
//!
//! **⇒ 为什么以前没被发现** ✗：`above` 只在 `above_wanted` 成立时启用 ✓ ⇒ 条件是
//!   `split > 0 && !except_seen && 可见不透明度(split 那层) < 1.0` ✓ —— **要求分割层半透明** ✓，
//!   而仓库里**没有一个夹具满足它** ✓（第 705 轮实测：连渲两次 `above_reuse_count` 是 `0 ⇒ 0` ✗）。
//!   ⇒ **所以本文件先造出这个夹具** ✓（3 层、活动层在中间、**那一层 `opacity: 0.5`** ✓）。
//!
//! **⇒ 判据**：第二次渲染（`above` 命中）**渲染过的图层数必须是 0** ✓
//!   （下方由 `below` 命中跳过、上方由 `above` 命中跳过 ✓）。
//!   **变异**：把 ① 那句挪回 `index_here < split` 里面 ⇒ 上方两层会被渲染 ⇒ `layers == 2` ⇒ 红 ✓。
//!
//! **⇒ 同时钉住像素**：第二次渲染必须与**新渲染器**的整幅渲染**逐字节相同** ✓
//!   （这防「跳过过头」把上方层丢掉 ✗ —— 与 R-01 第 ② 项配套 ✓）。
use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_core::Bbox;
use yanshi_render::{Renderer, TileGrid};

fn layer(id: &str, z: i64, opacity: f64) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: z,
        blend_mode: "normal".to_owned(),
        opacity,
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

/// **3 层、活动层在中间、那层 `opacity: 0.5` ⇒ `above` 真的会启用** ✓
fn document() -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("above_cache".to_owned());
    state.width = 64;
    state.height = 64;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    // 期望「先加的对象在下面」⇒ z_index **大的在下面**（本仓库既有夹具一致 ✓）
    state.layers.insert("L0".to_owned(), layer("L0", 20, 1.0));
    state.layers.insert("L1".to_owned(), layer("L1", 10, 0.5)); // 活动层（半透明 ✓）
    state.layers.insert("L2".to_owned(), layer("L2", 0, 1.0));
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

#[test]
fn a_hit_on_the_above_cache_skips_the_layers_it_already_holds() {
    let state = document();
    let store = MemoryBlobStore::new();
    let bbox = Bbox::new(0.0, 0.0, 64.0, 64.0);

    let mut renderer = Renderer::new(TileGrid::new(64, state.width, state.height).unwrap());
    renderer.set_active_layer(Some("L1".to_owned()));

    // ① 第一次：把 below 与 above 都填上
    let first = renderer
        .render_region(&state, &store, bbox)
        .expect("first render");
    let above_before = renderer.above_reuse_count();
    // ② 第二次：本次不许再"走一遍上方层"
    let second = renderer
        .render_region(&state, &store, bbox)
        .expect("second render");
    let above_after = renderer.above_reuse_count();
    assert!(
        above_after > above_before,
        "第二次没有命中 above（{above_before} ⇒ {above_after}）⇒ 这条判据测不到东西（先看夹具）"
    );
    // **活动层（`split` 那层）永远要渲** ✓ ⇒ 正确值是 **1** ✓；
    //   修复前实测是 **2**（上方那层也被重渲 ✗）。
    assert_eq!(
        second.stats.layers, 1,
        "above 命中之后渲染了 {} 层（正确值 1 = 只有活动层）⇒ 第 ① 项那句判断还在死代码里",
        second.stats.layers
    );

    // ③ 像素必须与"从零渲染"逐字节相同（防跳过过头）
    let mut fresh = Renderer::new(TileGrid::new(64, state.width, state.height).unwrap());
    fresh.set_active_layer(Some("L1".to_owned()));
    let truth = fresh
        .render_region(&state, &store, bbox)
        .expect("truth render");
    assert_eq!(first.rgba8, truth.rgba8, "第一次渲染与整幅渲染不一致");
    assert_eq!(
        second.rgba8, truth.rgba8,
        "命中 above 之后画面变了 ⇒ 跳过过头或合成错了"
    );
}

/// **空白 tile 也算"缓存里有"**（第 715 轮；外部审计 R-01 第 ② 项）
///
/// **⇒ 缺陷**：写侧把**整块为空**的 tile 记进 `fresh_empty`（**不进 `c.tiles`** ✓），
///   而读侧 `above_ready` **只查 `c.tiles`** ✗ ⇒ 只要上方层在区域里**有一块是空的**
///   （比如上方层只覆盖左半边 ✓），`above_ready` **恒 `false`** ✗ ⇒ **上方缓存永远不命中** ✓（白算 ✓）。
///
/// **⇒ 夹具**：活动层仍在中间（半透明 ✓），但**上方层只画左半边** ✓
///   ⇒ 区域右半边那块 above tile 是**空的** ✓。
/// **⇒ 判据**：连渲两次 ⇒ `above_reuse_count` **必须增长** ✓。
///   **变异**：把 `above_ready` 里的 `|| c.empty.contains(t)` 去掉 ⇒ 计数不涨 ⇒ 红 ✓。
#[test]
fn an_empty_above_tile_still_counts_as_cached() {
    // **`BELOW_TILE` 是 256** ✓ ⇒ 区域必须**跨 2×2 格**才可能出现"空白格" ✓
    //   （64×64 只落在一格里 ⇒ 那一格有内容 ⇒ 这条判据测不到东西 ✗ —— 我第一次就是这么写的 ✓）
    let mut state = document();
    state.width = 512;
    state.height = 512;
    // 各层内容铺满整幅，但**上方层（L2）只画左上角一格** ✓ ⇒ 其余三格是空的 ✓
    state.objects.insert(
        "o0".to_owned(),
        rect("o0", "L0", 0, (0.0, 0.0, 512.0, 512.0), [200, 30, 30]),
    );
    state.objects.insert(
        "o1".to_owned(),
        rect("o1", "L1", 0, (0.0, 0.0, 512.0, 512.0), [30, 200, 30]),
    );
    state.objects.insert(
        "o2".to_owned(),
        rect("o2", "L2", 0, (0.0, 0.0, 200.0, 200.0), [30, 30, 200]),
    );
    let store = MemoryBlobStore::new();
    let bbox = Bbox::new(0.0, 0.0, 512.0, 512.0);
    let mut renderer = Renderer::new(TileGrid::new(256, state.width, state.height).unwrap());
    renderer.set_active_layer(Some("L1".to_owned()));
    let _ = renderer
        .render_region(&state, &store, bbox)
        .expect("first render");
    let before = renderer.above_reuse_count();
    let _ = renderer
        .render_region(&state, &store, bbox)
        .expect("second render");
    let after = renderer.above_reuse_count();
    assert!(
        after > before,
        "上方层有一块是空的 ⇒ `above` 就再也不命中了（{before} ⇒ {after}）⇒ 空白块没被算作已缓存"
    );
}

/// **`clear_all_caches` 必须连 `above` 一起清**（第 761 轮；外部审计 R-07 的**运行时判据**）
///
/// **⇒ 为什么以前只能做结构检查** ✗：`above` 只在「`split` 那层**半透明**」时才启用 ✓，
///   而当时仓库里**没有**这样的夹具 ⇒ 连渲两次 `above_reuse_count` 是 `0 ⇒ 0` ⇒ 断言恒真 ✗。
/// **⇒ 现在有了**：本文件第 1 条测试造的夹具（3 层、活动层 `opacity: 0.5`）**真会命中 above** ✓
///   ⇒ 于是这一条可以**跑起来**：命中 → 清缓存 → **不许再命中** ✓。
/// **⇒ 判据**：② 之后 `above_reuse_count` **不再增长** ✓。
/// **变异**：去掉 `clear_all_caches` 里清 `self.above` 的三行 ⇒ 计数继续涨 ⇒ 红 ✓。
#[test]
fn clearing_caches_stops_the_above_cache_from_hitting() {
    let state = document();
    let store = MemoryBlobStore::new();
    let bbox = Bbox::new(0.0, 0.0, 64.0, 64.0);
    let mut renderer = Renderer::new(TileGrid::new(32, state.width, state.height).unwrap());
    renderer.set_active_layer(Some("L1".to_owned()));

    // ① 先证明"会命中" ✓（否则下面那半是空断言 ✗）
    let _ = renderer.render_region(&state, &store, bbox).expect("first");
    let before = renderer.above_reuse_count();
    let _ = renderer
        .render_region(&state, &store, bbox)
        .expect("second");
    let primed = renderer.above_reuse_count();
    assert!(
        primed > before,
        "连渲两次都没命中 above（{before} ⇒ {primed}）⇒ 这条判据测不到东西（先看夹具）"
    );

    // ② 清掉全部缓存 ⇒ 再渲一次**不许**再命中 ✓
    renderer.clear_all_caches();
    let after_clear_start = renderer.above_reuse_count();
    let _ = renderer.render_region(&state, &store, bbox).expect("third");
    let after_clear = renderer.above_reuse_count();
    assert_eq!(
        after_clear, after_clear_start,
        "清缓存之后 still 命中了 above（{after_clear_start} ⇒ {after_clear}）⇒ `clear_all_caches` 漏了 above"
    );
}
