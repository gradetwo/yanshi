//! **★ 部分复用：缓存里**有几格**就必须**用上几格** ✗ ★**（第 5 轮 ✓；**阶段一第 1 项的红判据 ✓**）。
//!
//! **∴ 借鉴来源 ✗ ★**：**GIMP `GimpProjection`** ✗**（[`app/core/gimpprojection.c`](https://gitlab.gnome.org/GNOME/gimp/-/blob/master/app/core/gimpprojection.c) ✓）：
//!   **它的有效性**按 tile 记**✗（**invalid region 是**逐块**的 ✓）**
//!   ⇒ **∴ 换到我们这里 ✗**：**"**缓存里有一格 ✓"**就**该用那一格**✗
//!     ⇒ **∴ 而**不是**"**缺一格 ⇒ **整块作废** ✓" ✓**** ✓✓
//!   **∴ 与它的差别 ✗**：**我们的**真值**是 **CPU 逐位一致** ✗**（**GIMP 不承诺 ✓）
//!     ⇒ **∴ 所以**：**这条判据**同时**要求**像素与"**完整重算**"**逐字节相同 ✓**** ✓✓
//!
//! **∴ 为什么先立判据 ✗**：**目标**明文**"**先立刻观测 ＋ 红判据 ✗，**再改实现 ✓"
//!   ⇒ **∴ 于是**：**本条**在**当前实现**下**必然红** ✗**（**`ready` 是全有或全无 ✓）
//!     ⇒ **∴ 而**它**红了**才证明**判据有牙 ✓**** ✓✓
//!
//! **∴ 变异 ✗**：**把 `ready` 改回"**必须全有 ✓"**✗ ⇒ **∴ 本条**必红 ✓**** ✓✓

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::state::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::{Renderer, TileGrid};

/// 最小图层（**照 `below_exceptions.rs` 的既有写法 ✓**）。
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

/// 最小对象（**同上 ✓**）。
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

/// **★ 三层文档 ✗ ★**：**下面两层有内容** ✗**，**最上层是活动层 ✓**。
///
/// **∴ 为什么三层 ✗**：**below 必须**非空**✗ ⇒ **∴ 否则**"**有没有缓存 ✓"**都无从谈起 ✓**** ✓✓
fn document() -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_partial".to_owned());
    state.width = 512;
    state.height = 512;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    // **∴ 内容要**横跨两格 ✗**（**x 到 100 ✓）⇒ **∴ 于是**"**缺的那一格**"**也有内容 ✓**** ✓✓
    let content = json!({"geometry": {"kind": "rect",
                                      "bbox": {"x": 8.0, "y": 8.0, "w": 292.0, "h": 120.0}},
                         "color": {"r": 20, "g": 40, "b": 80, "a": 200}});
    for (id, z) in [("layer_bottom", 0_i64), ("layer_middle", 1)] {
        state.layers.insert(id.to_owned(), layer(id, z));
        state.objects.insert(
            format!("rect_{id}"),
            object(&format!("rect_{id}"), id, 0, content.clone()),
        );
    }
    state
        .layers
        .insert("layer_top".to_owned(), layer("layer_top", 2));
    state.objects.insert(
        "rect_top".to_owned(),
        object("rect_top", "layer_top", 0, content),
    );
    state
}

/// **★ 判据 ✗ ★**：**画布 512×512 ✗；below 的 tile 由 `BELOW_TILE` 决定** ✗（**与 `TileGrid` 的 64 无关 ✓）**
///   **∴ 判据**不写死格数** ✗** ⇒ **∴ 实测**它**只断言"**有但有缺 ✓" ✓**** ✓✓
///
/// **★ 第 7 轮实测（**为什么不能写死格数 ✓）★**：
///   **∴ 并行分带**✗（**本机 4 条带 ✓）⇒ **∴ 每带**各自算**自己那条带**的 `want_tiles`** ✓
///     ⇒ **∴ 归并**求和 ✗ ⇒ **∴ 于是** `wanted = 2 × 4 = 8` ✓**（**而**单带只需 2 格 ✓）** ✓✓
///     ⇒ **∴ 且**：**四条带里**只有**一条带**的格命中 ✗（`available = 4` ✓）
///       ⇒ **∴ 而**「**全有或全无**」**在**带内**就作废 ⇒ `reused = 0` ✓**** ✓✓
///   ⇒ **∴ 所以**：**分带**会**放大**"**缺 1 格 ⇒ 全废 ✓"的损失 ✓**** ✓✓。
///
/// **∴ 步骤 ✗**：
///   **①** 先**只渲左上 256×256 ⇒ below 缓存 1 格** ✗**（**缓存里因此只有 1 格 ✓）**
///   **②** 再**渲左上 512×256 ⇒ 想要 2 格** ✗**（**含 1 格缓存里没有的 ✓）
///     ⇒ **∴ 于是**：`available = 1` **而** `wanted = 2` ✓**
///       ⇒ **★ 判据：`reused` 必须 > 0 ✗ ★**
///         （**∴ 当前实现**是全有或全无 ⇒ **∴ 这里**是 0 ⇒ **∴ 本条红 ✓）** ✓✓
///   **③** 同时**要求**像素与"**全新渲染器**"**逐字节相同** ✗
///     ⇒ **∴ 那**是**部分复用**的**安全网 ✓**** ✓✓
#[test]
fn partial_below_cache_reuses_the_tiles_it_has() {
    let state = document();
    let store = MemoryBlobStore::new();
    let mut renderer = Renderer::new(TileGrid::new(64, state.width, state.height).unwrap());
    // **∴ 活动层 ＝ 最上层 ⇒ **below ＝ 下面两层 ✓**（**非空 ✓）** ✓✓
    renderer.set_active_layer(Some("layer_top".to_owned()));

    // **① 只渲左上 1 格。**
    let first = renderer
        .render_region(
            &state,
            &store,
            yanshi_core::Bbox::new(0.0, 0.0, 256.0, 256.0),
        )
        .expect("第一块渲染");
    eprintln!(
        "第一次：bbox={:?} {}x{} tiles={} wanted={} measured={}",
        first.bbox,
        first.width,
        first.height,
        first.tiles.len(),
        first.stats.below_tiles_wanted,
        first.stats.below_tiles_measured
    );

    // **② 再渲左上 2 格：缓存里有 1 格，想要 2 格。**
    let r = renderer
        .render_region(
            &state,
            &store,
            yanshi_core::Bbox::new(0.0, 0.0, 512.0, 256.0),
        )
        .expect("第二块渲染");

    eprintln!(
        "第二次：bbox={:?} {}x{} tiles={}｜want_bbox={:?}｜missing={} recompute={:?}",
        r.bbox,
        r.width,
        r.height,
        r.tiles.len(),
        r.stats.below_want_bbox,
        r.stats.below_tiles_missing,
        r.stats.below_recompute_bbox
    );
    let s = &r.stats;
    eprintln!(
        "部分复用：measured={} wanted={} available={} reused={}",
        s.below_tiles_measured, s.below_tiles_wanted, s.below_tiles_available, s.below_tiles_reused
    );

    // **★ 三数**自洽性**：恒等式 `available + missing == wanted` ✗ ★**（第 180 轮 ✓）：
    //   **∴ 为什么它必然成立 ✗**：**实现**对**每一个想要的格**✗
    //     ⇒ **∴ 恰好**给 `available`**✗、**或**给 `missing`**✗ **加一** ✓**** ✓✓
    //       （**∴ 在 `tiles` 里 ⇒ available ✓；**在 `empty` 里 ⇒ available ✓；
    //        **两者都不在 ⇒ missing ✓）** ✓✓
    //     ⇒ **★ 所以**：**它是**实现的**不变量**✗，**不是**某个场景的巧合 ✓ ★**** ✓✓
    //   **∴ 为什么必须有它 ✗**：**目标里记着一个**旧读数**✗
    //     ⇒ `available=20 < wanted=32` **而** `missing=0`**✗
    //       ⇒ **∴ 20+0 ≠ 32** ✗ ⇒ **★ 那个读数**违反本恒等式** ✓ ★**** ✓✓
    //         ⇒ **∴ 所以**：**它**只能来自**两次取锁**的旧实现 ✓（**第 16 轮已修 ✓）** ✓✓
    //   **∴ 它**不写死格数**✗ ⇒ **∴ tile 常量一变**它**照样**有意义 ✓ ★**** ✓✓
    assert_eq!(
        s.below_tiles_available + s.below_tiles_missing,
        s.below_tiles_wanted,
        "三数**必须**自洽 ✗：available({}) + missing({}) **必须**等于 wanted({}) —— \
         **∴ 因为**实现**对每个想要的格**恰好**给 available 或 missing 加一 ✓；\
         **∴ 不等**说明**三数来自**不同的快照** ✗（**第 16 轮的错 ✓）",
        s.below_tiles_available,
        s.below_tiles_missing,
        s.below_tiles_wanted
    );

    assert!(
        s.below_tiles_measured,
        "这次渲染**必须**做过 below 判定 ✗；实测没做过 ⇒ **∴ 要么**这条判据构造不对 ✗，\
         **要么**这个区域走的是「不可缓存」的路 ✓（**∴ 那样后面全是空转 ✓）"
    );
    // **∴ 这里**不写死格数 ✗**（**第 6 轮教训 ✓）：**tile 尺寸是**实现的常量**✗
    //   ⇒ **∴ 判据**一旦写死 ✗**，**常量一变**它就**测我的假设**✗，**而不是**测实现 ✓**** ✓✓
    //   **∴ 只断言**场景前提 ✗**：**"**缓存里有格 ✓、**而**不够 ✓"** ✓**** ✓✓
    assert!(
        s.below_tiles_wanted > 0,
        "这次渲染**应当**想要至少 1 格 ✗；实测 0 ⇒ **∴ 它**根本没做 below 判定 ✓"
    );
    assert!(
        s.below_tiles_available > 0,
        "先渲过一块 ⇒ 缓存里**应当**有可用格 ✗；实测 0 ⇒ **∴ 要么**缓存没写进去 ✗，\
         **要么**指纹变了 ✓（**∴ 两种都要先查清 ✓）"
    );
    assert!(
        s.below_tiles_available < s.below_tiles_wanted,
        "这条判据要的场景是**有但有缺** ✓；实测 available={} wanted={} ⇒ \
         **∴ 要么**两次区域没跨格 ✗，**要么**缓存已覆盖全部 ✓（**∴ 那样这条就退化了 ✓）",
        s.below_tiles_available,
        s.below_tiles_wanted
    );
    // **★ 这一条就是"**部分复用**"本身 ✗ ★**：**有一格**就必须用那一格 ✓**** ✓✓
    assert!(
        s.below_tiles_reused > 0,
        "缓存里有 {} 格能用（想要 {} 格）⇒ **∴ 必须**用上它们 ✗ —— \
         借用 GIMP `GimpProjection` 的口径：**有效性属于 tile ✗**，\
         **不是**整幅的属性 ✓；实测复用 {} 格 ⇒ **∴ 当前实现**是「全有或全无」 ✗，\
         于是**缺 1 格**就**把整块作废 ✓（**宁慢勿错是对的 ✗，**但这里**慢得没必要 ✓）",
        s.below_tiles_available,
        s.below_tiles_wanted,
        s.below_tiles_reused
    );

    // **③ 安全网：部分复用**不许**改变像素 ✗**（**∴ 与全新渲染器逐字节比 ✓）** ✓✓
    let mut fresh = Renderer::new(TileGrid::new(64, state.width, state.height).unwrap());
    fresh.set_active_layer(Some("layer_top".to_owned()));
    let f = fresh
        .render_region(
            &state,
            &store,
            yanshi_core::Bbox::new(0.0, 0.0, 512.0, 256.0),
        )
        .expect("对照渲染");
    assert_eq!(
        r.rgba8.len(),
        f.rgba8.len(),
        "两次渲染的像素数**必须**相同 ✓"
    );
    // **★ 差异的形状（**先报出来再断言 ✓）★**（第 11 轮 ✓）：
    //   **∴ 为什么先报 ✗**：**"**不一致 ✓"**这三个字**定位不了任何东西 ✓**
    //     ⇒ **∴ 至少要**差异数 ＋ 首像素 ＋ **按 tile 的分布 ✓**** ✓✓
    // **∴ 从渲染结果里拿不到缓存内部 ✗ ⇒ **∴ 用公开的 want_bbox 反推覆盖 ✓**（**第 11 轮 ✓）** ✓✓
    let acc_debug = r.stats.below_want_bbox;
    if r.rgba8 != f.rgba8 {
        let w = r.width as usize;
        let diff_bytes = r
            .rgba8
            .iter()
            .zip(f.rgba8.iter())
            .filter(|(a, b)| a != b)
            .count();
        let first = r
            .rgba8
            .iter()
            .zip(f.rgba8.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        let (px, py) = ((first / 4) % w, (first / 4) / w);
        eprintln!(
            "像素差异：{} 字节｜首差异在像素 ({px},{py})｜图 {}x{}",
            diff_bytes, r.width, r.height
        );
        // **∴ 按 tile 统计 ✗**（**tile = 256 ⇒ 看**哪些块错了 ✓）** ✓✓
        let mut by_tile: std::collections::BTreeMap<(usize, usize), usize> =
            std::collections::BTreeMap::new();
        for i in 0..(r.rgba8.len() / 4) {
            if r.rgba8[i * 4..i * 4 + 4] != f.rgba8[i * 4..i * 4 + 4] {
                let (x, y) = (i % w, i / w);
                *by_tile.entry((x / 256, y / 256)).or_insert(0) += 1;
            }
        }
        // **∴ 缓存 tile 的**实际覆盖**也要看 ✗**（**∴ 若**它比 256 小 ⇒ **∴ 预填会**留下没人算的格 ✓）** ✓✓
        eprintln!("  accumulation 盒 = {:?}", acc_debug);
        for (k, n) in &by_tile {
            eprintln!("  错块 tile({},{}) ⇒ {} 像素", k.0, k.1, n);
        }
    }
    assert_eq!(
        r.rgba8, f.rgba8,
        "**部分复用**只能省计算 ✗，**绝不许**改变像素 ✓（**∴ 与 CPU 真值逐字节一致 ✓）"
    );
}
