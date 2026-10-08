//! **并行分块渲染的判据**（本专题的核心不变量）。
//!
//! 判据 1：同一份文档，**串行**（`max_workers = 1`）与**并行**（`max_workers = 4`）
//! 的渲染结果**逐字节相同**（`rgba8` 与每个 tile 都比）。这条判据用的文档刻意**不含
//! 滑动窗口方框模糊**（蒙版羽化 / glow / clarity / sharpen）—— 那是项目**已批准**的
//! D1 例外（见 `filter.rs::blur_horizontal` 的说明：分块与整幅之间可能有 ±1 LSB）。
//! 含该例外的文档另有一条 D1 判据（`parallel_matches_serial_within_documented_d1`）。
//!
//! 判据 2：并发**真的发生了** —— 由统计里的 `parallel_chunks` / `parallel_workers`
//! 计数器证明，而不是墙钟快慢（墙钟会被机器负载骗过）。
//!
//! 判据 3：wasm 目标**不编译线程路径**（见 `thread_code_is_cfg_gated_out_of_wasm` 与
//! `scripts/wasm-smoke.sh`）。
//!
//! 判据 4：`Buffer::to_rgba8` 的按行并行与串行（`workers = Some(1)`）逐字节相同；
//! 判据 5：它的**阈值真的在选边**（小图串行、大图并行，用 `blocks` 语义计数断言）。
//! 这两条盯的是同一条不变量用在"输出量化"这条公开 API 上的样子。
//!
//! 每个判据都做过**变异演示**（改坏被测条件 ⇒ 判红 ⇒ 逐字改回），见专题报告。

use serde_json::json;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use yanshi_core::atom::BlobHash;
use yanshi_core::blob::{BlobEntry, BlobStore, MemoryBlobStore};
use yanshi_core::{
    Bbox, DocumentState, Layer, LayerType, Object, ObjectType, Result, Selection, Transform,
};
use yanshi_render::buffer_pool::{BufferPool, MAX_POOLED_BUFFERS, MIN_POOLED_BYTES};
use yanshi_render::render::Renderer;
use yanshi_render::tile::{TileGrid, TileKey};

/// 数 `get` 次数的存储包装 —— 用来证明"分块并行时同一份位图只读一次"（缓存真的生效）。
struct CountingStore {
    inner: MemoryBlobStore,
    /// **累计取用的字节数** ✓（第 203 轮 ✓）：判"只取覆盖区域的块"用 ✓。
    bytes: AtomicU64,
    gets: AtomicUsize,
}

impl CountingStore {
    fn new() -> Self {
        Self {
            inner: MemoryBlobStore::new(),
            gets: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
        }
    }
    /// **累计取用字节** ✓（第 203 轮 ✓）—— **"只取覆盖区域的块"的度量** ✓。
    fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::SeqCst)
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
    fn reset(&self) {
        self.gets.store(0, Ordering::SeqCst);
        self.bytes.store(0, Ordering::SeqCst);
    }
}

impl BlobStore for CountingStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.inner.put(bytes)
    }
    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        let bytes = self.inner.get(hash)?;
        self.bytes.fetch_add(bytes.len() as u64, Ordering::SeqCst);
        self.gets.fetch_add(1, Ordering::SeqCst);
        Ok(bytes)
    }
    fn exists(&self, hash: &BlobHash) -> bool {
        self.inner.exists(hash)
    }
    fn size(&self, hash: &BlobHash) -> Option<u64> {
        self.inner.size(hash)
    }
    fn list(&self) -> Result<Vec<BlobEntry>> {
        self.inner.list()
    }
    fn remove(&self, hash: &BlobHash) -> Result<bool> {
        self.inner.remove(hash)
    }
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

fn object(id: &str, layer_id: &str, kind: ObjectType, z: i64, data: serde_json::Value) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: kind,
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

/// **专为"分块"造的文档**：内容与效果都**跨块边界**，任何"块之间可以各自为政"的假设都会露馅。
///
/// 关键构造：
/// * 三层、其中两层带**非 normal 混合模式**与不透明度（顺序与逐像素合成必须保持）；
/// * 一层是**剪贴蒙版**，一层带蒙版（`feathered_mask` 决定是否羽化）；
/// * 一条**高斯模糊**（可分离卷积、固定求和顺序 ⇒ 分块无关）⇒ 块边界必须靠外扩拿到真邻居；
/// * 一个**修图克隆**（`source_offset = [-64,-48]`）⇒ 块内像素要读**别处**的源像素；
/// * 一个**未知滤镜**（整层对象）⇒ 每块都会产生同一条告警，合并必须去重（与串行一致）。
///
/// `feathered_mask = true` 会引入蒙版羽化（滑动窗口方框模糊 ⇒ **项目已批准的 D1**），
/// 只用于 D1 判据；判据 1 用 `false`。
fn stress_document(size: u32, feathered_mask: bool) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_tile_parallel".to_owned());
    state.width = size;
    state.height = size;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});

    let mut bottom = layer("layer_bottom", 0);
    bottom.blend_mode = "normal".to_owned();
    state.layers.insert(bottom.id.clone(), bottom);

    let mut multiply = layer("layer_multiply", 1);
    multiply.blend_mode = "multiply".to_owned();
    multiply.opacity = 0.85;
    state.layers.insert(multiply.id.clone(), multiply);

    let mut screen = layer("layer_screen", 2);
    screen.blend_mode = "screen".to_owned();
    screen.opacity = 0.9;
    screen.clipping_mask = true;
    screen.mask_id = Some("mask_1".to_owned());
    state.layers.insert(screen.id.clone(), screen);

    let s = size as f64;

    // 底层：半透明满幅矩形 + 一条贯穿全高的对角笔迹（跨所有块边界）。
    state.objects.insert(
        "bg".to_owned(),
        object(
            "bg",
            "layer_bottom",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": s, "h": s}},
                   "color": {"r": 40, "g": 70, "b": 110, "a": 200}}),
        ),
    );
    state.objects.insert(
        "ink".to_owned(),
        object(
            "ink",
            "layer_bottom",
            ObjectType::Stroke,
            1,
            json!({"points": [[8.0, 8.0], [s - 8.0, s - 8.0]], "size": 9.0,
                   "color": {"r": 240, "g": 200, "b": 60, "a": 220}}),
        ),
    );

    // multiply 层：两个圆 + 高斯模糊（作用于其下方内容 ⇒ 需要真实邻域）。
    for (index, (cx, cy)) in [(s * 0.30, s * 0.25), (s * 0.70, s * 0.75)]
        .iter()
        .enumerate()
    {
        let radius = s * 0.16;
        state.objects.insert(
            format!("blob_{index}"),
            object(
                &format!("blob_{index}"),
                "layer_multiply",
                ObjectType::Shape,
                index as i64,
                json!({"geometry": {"kind": "ellipse",
                                    "bbox": {"x": cx - radius, "y": cy - radius,
                                             "w": radius * 2.0, "h": radius * 2.0}},
                       "color": {"r": 200, "g": 60, "b": 90, "a": 210}}),
            ),
        );
    }
    state.objects.insert(
        "blur".to_owned(),
        object(
            "blur",
            "layer_multiply",
            ObjectType::Filter,
            10,
            json!({"filter_name": "gaussian_blur", "params": {"sigma": 4.0}}),
        ),
    );

    // screen 层：一个矩形 + 跨块读源的修图克隆 + 未知滤镜（去重告警）。
    state.objects.insert(
        "wash".to_owned(),
        object(
            "wash",
            "layer_screen",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect",
                                "bbox": {"x": s * 0.1, "y": s * 0.4, "w": s * 0.8, "h": s * 0.2}},
                   "color": {"r": 60, "g": 220, "b": 180, "a": 160}}),
        ),
    );
    state.objects.insert(
        "clone".to_owned(),
        object(
            "clone",
            "layer_screen",
            ObjectType::Retouch,
            1,
            json!({"retouch_type": "clone_stamp",
                   "points": [[s * 0.6, s * 0.55], [s * 0.7, s * 0.55]],
                   "source_offset": [-64.0, -48.0], "size": 26.0, "hardness": 0.7, "opacity": 1.0}),
        ),
    );
    state.objects.insert(
        "unsupported_filter".to_owned(),
        object(
            "unsupported_filter",
            "layer_screen",
            ObjectType::Filter,
            2,
            json!({"filter_name": "definitely_not_a_filter", "params": {}}),
        ),
    );

    // 蒙版：`feathered_mask = true` 时带羽化（滑动窗口方框模糊 ⇒ D1）。
    state.masks.insert(
        "mask_1".to_owned(),
        Selection {
            id: "mask_1".to_owned(),
            shape: json!({"kind": "ellipse",
                          "bbox": {"x": s * 0.05, "y": s * 0.05, "w": s * 0.9, "h": s * 0.9}}),
            feather: if feathered_mask { 12.0 } else { 0.0 },
            mode: "new".to_owned(),
            invert: false,
            linked_layer: Some("layer_screen".to_owned()),
            refined_edges: false,
            blobs: Vec::new(),
            created_by: "human:1".to_owned(),
            deleted_by: None,
        },
    );
    state
}

fn renderers(size: u32, budget: usize) -> (Renderer, Renderer) {
    let grid = TileGrid::new(64, size, size).unwrap();
    (
        Renderer::with_budget(grid.clone(), budget).with_max_workers(1),
        Renderer::with_budget(grid, budget).with_max_workers(4),
    )
}

/// 判决 1：并行与串行**逐字节相同**（整幅 + 每个 tile）。
#[test]
fn parallel_and_serial_render_byte_identical() {
    // 不含滑动窗口方框模糊 ⇒ 渲染是 D0（逐字节），判据可以要求"完全相同"。
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let (mut serial, mut parallel) = renderers(320, 32 * 1024 * 1024);

    let a = serial.render_document(&state, &store).unwrap();
    let b = parallel.render_document(&state, &store).unwrap();

    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    assert_eq!(a.rgba8.len(), b.rgba8.len());
    let diff = a
        .rgba8
        .iter()
        .zip(b.rgba8.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(
        diff,
        0,
        "并行与串行必须逐字节相同：{diff}/{} 字节不同",
        a.rgba8.len()
    );

    // 缓存里的 tile（客户端真正组合的那条路径）也必须逐字节相同。
    let grid = TileGrid::new(64, 320, 320).unwrap();
    let mut checked = 0usize;
    for key in grid.all_keys() {
        let left = serial.render_tile(&state, &store, key).unwrap();
        let right = parallel.render_tile(&state, &store, key).unwrap();
        assert_eq!(left.key(), right.key());
        assert_eq!(left.size(), right.size());
        assert_eq!(
            left.to_f32(),
            right.to_f32(),
            "tile {key:?} 的 f16 像素在两种模式下必须相同"
        );
        checked += 1;
    }
    assert!(checked > 1, "应检查多个 tile（实际 {checked}）");
}

/// 判决 1 的**偏移区域**版本：脏区不是从 `(0,0)` 开始时也必须逐字节相同、且尺寸正确。
///
/// **这条判据是补写的**：最初的并行执行器把区域宽高算成 `w − x`（`x` 是绝对文档坐标）
/// ⇒ 偏移区域被算成 0 宽 / 截断（服务端 `coldstart_reuse` 的脏区渲染返回 0×0 才暴露）。
/// 只测全幅（`x = y = 0`）的判据**抓不到它** —— 判据必须覆盖"非零原点"。
#[test]
fn parallel_offset_region_byte_identical() {
    let state = stress_document(512, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 512, 512).unwrap();
    // 300×240 = 72000 像素 ≥ 并行阈值；x/y 都非零。
    let region = yanshi_core::Bbox::new(101.0, 137.0, 300.0, 240.0);
    let mut serial = Renderer::with_budget(grid.clone(), 64 * 1024 * 1024).with_max_workers(1);
    let mut parallel = Renderer::with_budget(grid, 64 * 1024 * 1024).with_max_workers(4);

    let a = serial.render_region(&state, &store, region).unwrap();
    let b = parallel.render_region(&state, &store, region).unwrap();
    assert_eq!(a.width, 300, "请求 300 宽的区域必须返回 300 宽");
    assert_eq!(a.height, 240, "请求 240 高的区域必须返回 240 高");
    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    assert_eq!(a.bbox, b.bbox);
    let diff = a
        .rgba8
        .iter()
        .zip(b.rgba8.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(
        diff,
        0,
        "偏移区域并行与串行必须逐字节相同：{diff}/{} 字节不同",
        a.rgba8.len()
    );
    assert_eq!(a.stats.parallel_chunks, 0, "串行区域不应分块");
    assert!(
        b.stats.parallel_chunks >= 2,
        "偏移区域也必须真的分块：{}",
        b.stats.parallel_chunks
    );
}

/// 项目**已批准**的 D1 例外：滑动窗口方框模糊（蒙版羽化 / glow / clarity / sharpen）
/// 的求和顺序与窗口起点有关 ⇒ 分块与整幅之间可能有 ±1 LSB。
///
/// 本条判据不是"要求逐字节相同"（那与 `filter.rs` 的既定决策冲突），而是：
/// 差异必须**小且稀疏**，并且**只可能来自那个已登记的例外**。
#[test]
fn parallel_matches_serial_within_documented_d1() {
    let state = stress_document(320, true);
    let store = MemoryBlobStore::new();
    let (mut serial, mut parallel) = renderers(320, 32 * 1024 * 1024);
    let a = serial.render_document(&state, &store).unwrap();
    let b = parallel.render_document(&state, &store).unwrap();

    let diffs: Vec<usize> = a
        .rgba8
        .iter()
        .zip(b.rgba8.iter())
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .map(|(index, _)| index)
        .collect();
    let max_delta = diffs
        .iter()
        .map(|index| (a.rgba8[*index] as i32 - b.rgba8[*index] as i32).abs())
        .max()
        .unwrap_or(0);
    println!(
        "含羽化蒙版（D1 例外）：{} 字节不同 / {}，最大差 {max_delta} LSB",
        diffs.len(),
        a.rgba8.len()
    );
    assert!(
        max_delta <= 1,
        "D1 例外只允许 ±1 LSB，实测最大 {max_delta} LSB（差异字节 {}）",
        diffs.len()
    );
    // 稀疏性：±1 LSB 只应出现在极少数像素上（不是"整幅都变了"）。
    assert!(
        diffs.len() * 100 <= a.rgba8.len(),
        "D1 差异应稀疏（<1% 字节），实测 {}/{}",
        diffs.len(),
        a.rgba8.len()
    );
}

/// 判决 2：**计数器证明并行真的发生**（不是靠墙钟）。
#[test]
fn parallel_workers_counter_proves_parallelism() {
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 320, 320).unwrap();

    let serial = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024)
        .with_max_workers(1)
        .render_document(&state, &store)
        .unwrap();
    assert_eq!(serial.stats.parallel_workers, 1, "串行路径 worker 数应为 1");
    assert_eq!(serial.stats.parallel_chunks, 0, "串行路径不应分块");

    let parallel = Renderer::with_budget(grid, 32 * 1024 * 1024)
        .with_max_workers(4)
        .render_document(&state, &store)
        .unwrap();
    assert!(
        parallel.stats.parallel_chunks >= 2,
        "强制 4 worker 时应至少分 2 块：{}",
        parallel.stats.parallel_chunks
    );
    assert!(
        parallel.stats.parallel_workers >= 2,
        "应至少用到 2 个 worker：{}",
        parallel.stats.parallel_workers
    );
}

/// 判决 2 的补强：并行分块的**统计口径与串行一致**（不会因分块把对象重复计数、
/// 或把同一条告警重复若干次）。
#[test]
fn parallel_stats_match_serial() {
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 320, 320).unwrap();

    let serial = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024)
        .with_max_workers(1)
        .render_document(&state, &store)
        .unwrap();
    let parallel = Renderer::with_budget(grid, 32 * 1024 * 1024)
        .with_max_workers(4)
        .render_document(&state, &store)
        .unwrap();

    assert_eq!(serial.stats.layers, parallel.stats.layers, "图层数");
    assert_eq!(serial.stats.objects, parallel.stats.objects, "对象数");
    assert_eq!(
        serial.stats.objects_culled, parallel.stats.objects_culled,
        "被裁对象数"
    );
    // 告警集合相同（顺序可能不同 ⇒ 比集合）。
    let mut left = serial.stats.unsupported.clone();
    let mut right = parallel.stats.unsupported.clone();
    left.sort();
    right.sort();
    assert_eq!(left, right, "告警集合");
    assert!(
        !left.is_empty(),
        "文档里有未知滤镜 ⇒ 必须产生告警（否则本条判据是空转）"
    );
}

/// 判决 1 的边界条件：**小于阈值的区域必须走串行**（线程开销不该被付在小渲染上）。
#[test]
fn small_region_stays_serial() {
    let state = stress_document(256, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 256, 256).unwrap();
    let mut renderer = Renderer::with_budget(grid, 32 * 1024 * 1024).with_max_workers(8);
    // 64×64 = 4096 像素 < 128×128 阈值。
    let out = renderer
        .render_region(
            &state,
            &store,
            yanshi_core::Bbox::new(64.0, 64.0, 64.0, 64.0),
        )
        .unwrap();
    assert_eq!(out.stats.parallel_chunks, 0, "小区域应走串行");
    assert_eq!(out.stats.parallel_workers, 1);
}

/// 同一份文档、同一次运行内的**相对**加速比（不用绝对秒数 ⇒ 不被机器快慢骗过）。
///
/// `#[ignore]`：这是性能验收，不进默认测试（用 `--ignored` 或 release 跑）。
#[test]
#[ignore = "性能验收：同一次运行内比较并行/串行（用 release 跑）"]
fn perf_parallel_speedup_same_run() {
    let cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    if cores < 2 {
        eprintln!("只有 {cores} 个核 ⇒ 跳过（并行无从谈起）");
        return;
    }
    // 2048²：在 4 核机器上 release 约数秒量级；debug 会慢约 10×。
    let state = stress_document(2048, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(256, 2048, 2048).unwrap();
    let budget = 512 * 1024 * 1024;

    let run = |workers: usize| -> (Duration, usize) {
        let mut renderer = Renderer::with_budget(grid.clone(), budget).with_max_workers(workers);
        let started = Instant::now();
        let out = renderer.render_document(&state, &store).unwrap();
        (started.elapsed(), out.stats.parallel_chunks)
    };

    // 各自预热一次（分配/页错误不参与比值），再各取 3 次的**最好一次** —— 这台机器上
    // 常有别的构建在跑，单次采样会被外部负载随机拉长；取最好一次能比较公平地反映
    // "这条路径本身跑多快"，同时仍是**同一次运行内**的比值（不是绝对时间预算）。
    let _ = run(1);
    let _ = run(4);
    let mut serial = Duration::MAX;
    let mut parallel = Duration::MAX;
    let mut serial_chunks = 0;
    let mut parallel_chunks = 0;
    for _ in 0..3 {
        let (elapsed, chunks) = run(1);
        serial = serial.min(elapsed);
        serial_chunks = chunks;
        let (elapsed, chunks) = run(4);
        parallel = parallel.min(elapsed);
        parallel_chunks = chunks;
    }
    assert_eq!(serial_chunks, 0);
    assert!(parallel_chunks >= 2, "应走并行：{parallel_chunks}");
    let ratio = serial.as_secs_f64() / parallel.as_secs_f64().max(1e-9);
    println!(
        "2048² 分块压力文档：串行 {serial:?}｜并行 {parallel:?}｜加速 {ratio:.2}×（{cores} 核，{parallel_chunks} 块）"
    );
    // 阈值刻意保守：分块有"每块都要外扩 padding"的固有重复行，且这一步是**内存带宽**敏感
    // （量化也是），核越多越受带宽限制 ⇒ 4 核上远不到 4×。这条断言只抓"并行基本没生效"。
    assert!(
        ratio >= 1.3,
        "并行应至少快 1.3×（同一次运行的比值）：{ratio:.2}×（串行 {serial:?} / 并行 {parallel:?}）"
    );
}

/// 判据 3 的**静态**部分：`std::thread` 只允许出现在 `#[cfg(not(target_arch = "wasm32"))]`
/// 门控的 `parallel_impl` 模块里；wasm 变体必须存在且**不含**线程代码。
///
/// 变异：去掉 `#[cfg(not(target_arch = "wasm32"))]`（或在别处加一处 `std::thread`）⇒ 判红。
/// 真正的运行时兜底由 `scripts/wasm-smoke.sh`（在 node 里真跑一次 wasm 渲染）提供。
#[test]
fn thread_code_is_cfg_gated_out_of_wasm() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/render.rs");
    let text = std::fs::read_to_string(&path).expect("应能读到 render.rs");

    // 定位 `mod parallel_impl {` 的两个变体：紧邻其上的 cfg 行决定它属于哪个目标。
    let mut native_range = None;
    let mut wasm_ranges = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "mod parallel_impl {" {
            continue;
        }
        // 向上找最近的属性行。
        let attribute = lines[..index]
            .iter()
            .rev()
            .find(|candidate| !candidate.trim().is_empty())
            .map(|candidate| candidate.trim().to_owned())
            .unwrap_or_default();
        // 花括号配对，确定模块范围。
        let mut depth = 0i32;
        let mut end = index;
        for (offset, candidate) in lines[index..].iter().enumerate() {
            depth += candidate.matches('{').count() as i32;
            depth -= candidate.matches('}').count() as i32;
            if depth == 0 {
                end = index + offset;
                break;
            }
        }
        if attribute.contains("cfg(not(target_arch = \"wasm32\"))") {
            native_range = Some((index, end));
        } else if attribute.contains("cfg(target_arch = \"wasm32\")") {
            wasm_ranges.push((index, end));
        }
    }
    let (native_start, native_end) =
        native_range.expect("应有 #[cfg(not(target_arch = \"wasm32\"))] mod parallel_impl");
    assert!(
        !wasm_ranges.is_empty(),
        "应有 #[cfg(target_arch = \"wasm32\")] 的串行回退 parallel_impl"
    );

    let mut thread_lines = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        // 只看**代码**：文档注释里可以出现 `std::thread` 这个词（例如"这里不引用它"）。
        if line.trim_start().starts_with("//") {
            continue;
        }
        if line.contains("std::thread") {
            thread_lines.push(index);
        }
    }
    assert!(
        !thread_lines.is_empty(),
        "本专题应真的用了线程（否则判据是空转）"
    );
    for index in thread_lines {
        assert!(
            index >= native_start && index <= native_end,
            "render.rs:{} 的 std::thread 不在 cfg(not(wasm32)) 门控的 parallel_impl 内：{}",
            index + 1,
            lines[index].trim()
        );
    }
    for (start, end) in wasm_ranges {
        for line in &lines[start..=end] {
            assert!(
                !line.contains("std::thread"),
                "wasm 变体里不应出现线程代码：{}",
                line.trim()
            );
        }
    }

    // **共享的行并行设施（`src/rows.rs`）适用同一约定**：所有 `std::thread` 必须落在
    // `#[cfg(not(target_arch = "wasm32"))]` 门控的执行器函数体里，别处一处都不许有。
    // 变异：把执行器的 `#[cfg]` 去掉，或在 rows.rs 别处新增一处 `std::thread` ⇒ 判红。
    let rows_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rows.rs");
    let rows_text = std::fs::read_to_string(&rows_path).expect("应能读到 rows.rs");
    let rows_lines: Vec<&str> = rows_text.lines().collect();
    let executor = rows_lines
        .iter()
        .position(|line| {
            line.trim_start()
                .starts_with("pub(crate) fn for_each_band_mut(")
        })
        .expect("rows.rs 应有按行并行的执行器 for_each_band_mut");
    let gate = rows_lines[..executor]
        .iter()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .unwrap_or_default();
    assert_eq!(
        gate, "#[cfg(not(target_arch = \"wasm32\"))]",
        "行并行执行器必须整体 cfg 掉 wasm"
    );
    // 执行器函数体的行范围（花括号配对；签名跨行 ⇒ 从第一处 `{` 起算）。
    let mut depth = 0i32;
    let mut started = false;
    let mut executor_end = executor;
    for (offset, candidate) in rows_lines[executor..].iter().enumerate() {
        let opens = candidate.matches('{').count() as i32;
        let closes = candidate.matches('}').count() as i32;
        if !started {
            if opens == 0 {
                continue;
            }
            started = true;
        }
        depth += opens - closes;
        if depth == 0 {
            executor_end = executor + offset;
            break;
        }
    }
    assert!(started, "应能找到执行器 for_each_band_mut 的函数体");
    let mut rows_threads = 0usize;
    for (index, line) in rows_lines.iter().enumerate() {
        // 只看代码：文档注释里可以出现 `std::thread` 这个词。
        if line.trim_start().starts_with("//") {
            continue;
        }
        if line.contains("std::thread") {
            rows_threads += 1;
            assert!(
                index >= executor && index <= executor_end,
                "rows.rs:{} 的 std::thread 不在 cfg(not(wasm32)) 门控的执行器内：{}",
                index + 1,
                line.trim()
            );
        }
    }
    assert!(
        rows_threads > 0,
        "rows.rs 应真的用了线程（否则这条静态判据是空转）"
    );
}

/// 判据 3 的**语义**部分：wasm 上并行度恒为 1（该目标没有共享内存线程）。
#[test]
fn wasm_target_is_always_serial() {
    // 真正的 wasm 行为由 `scripts/wasm-smoke.sh`（构建 + node 里真跑）验证；
    // 这里守住的是"没有任何调用点能在 wasm 上绕过串行"这一约定：worker 数只在
    // `parallel_impl::workers` 里取值，而 wasm 变体返回 1。
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/render.rs");
    let text = std::fs::read_to_string(&path).expect("应能读到 render.rs");
    assert!(
        text.contains(
            "pub const fn workers(_explicit: Option<usize>) -> usize {\n        1\n    }"
        ),
        "wasm 变体的 workers() 必须恒为 1"
    );
}

/// 缓存命中路径不受并行度影响：同一 tile 第二次读取必须命中且字节不变。
#[test]
fn second_read_is_a_cache_hit_under_parallel_render() {
    let state = stress_document(256, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 256, 256).unwrap();
    let mut renderer = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024).with_max_workers(4);
    renderer.render_document(&state, &store).unwrap();
    let key = TileKey::new(0, 0);
    let first = renderer.render_tile(&state, &store, key).unwrap();
    let hits = renderer.cache().stats().hits;
    let second = renderer.render_tile(&state, &store, key).unwrap();
    assert!(renderer.cache().stats().hits > hits, "第二次应命中缓存");
    assert_eq!(first.to_f32(), second.to_f32());
}

/// **位图补丁在分块并行下只读/解码一次**（缓存判据）。
///
/// 一块覆盖全画布的补丁会与每个分块相交；没有缓存时 `store.get`（文件存储还要读盘）
/// 会被调用**块数次**。这里用计数存储把这件事变成**语义计数**（不是计时）。
///
/// 变异：让并行分块不共享缓存（每块各建一个 / 传 `None`）⇒ 本判据红（4 次 ≠ 1 次）。
#[test]
fn bitmap_patch_is_read_once_per_render_even_when_banded() {
    let store = CountingStore::new();
    let side = 256u32;
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for index in 0..(side * side) {
        pixels.extend_from_slice(&[(index % 251) as u8, 40, 200, 255]);
    }
    let blob = yanshi_core::blob::stage_blob(&store, &pixels, "image/x-yanshi-raw").unwrap();
    let mut state = stress_document(side, false);
    state.objects.insert(
        "patch".to_owned(),
        object(
            "patch",
            "layer_bottom",
            ObjectType::RasterPatch,
            3,
            json!({"bitmap": blob, "width": side, "height": side,
                   "region": {"x": 0, "y": 0, "w": side, "h": side}}),
        ),
    );
    let grid = TileGrid::new(64, side, side).unwrap();
    let mut serial = Renderer::with_budget(grid.clone(), 64 * 1024 * 1024).with_max_workers(1);
    let mut parallel = Renderer::with_budget(grid, 64 * 1024 * 1024).with_max_workers(4);

    store.reset();
    let a = serial.render_document(&state, &store).unwrap();
    let serial_gets = store.gets();
    store.reset();
    let b = parallel.render_document(&state, &store).unwrap();
    let parallel_gets = store.gets();

    assert_eq!(serial_gets, 1, "串行渲染同一份位图应只读一次");
    assert!(b.stats.parallel_chunks >= 2, "应走并行");
    assert_eq!(
        parallel_gets, 1,
        "并行分块也应对同一份位图只读一次（共享缓存）；实测 {parallel_gets} 次"
    );
    assert_eq!(a.rgba8, b.rgba8, "含位图补丁时并行与串行仍须逐字节相同");
}

// ---------------------------------------------------------------------------
// 图层缓冲池的判据（性能改动：复用图层缓冲的底层分配；**不改像素**）。
//
// 复用点：`render.rs::render_accumulation` 的图层循环。改造前每层 `Buffer::new`
// （4K 单层约 132.7 MB，5 层每帧约 663 MB 的分配 + 清零）；改造后从
// `crate::buffer_pool::BufferPool` 取用，层间归还复用。
// ---------------------------------------------------------------------------

/// **缓冲池判据 1（正确性）**：同一份文档，池化（默认）与**强制每层新建分配**两条
/// 路径必须逐字节相同（整幅 `rgba8` + 每个 tile 的 f16 像素）。
///
/// 强制新建走 `BufferPool::set_enabled(false)` ⇒ 取用点每次 `Buffer::new`，与改造前一致。
/// 断言"池化确实发生了复用"是为了避免本条空转（两条都新建就没有证明力）。
///
/// 变异：`Buffer::reset` 不再清零（去掉 `clear`，只留同长度下 no-op 的 `resize`）
/// ⇒ 复用缓冲残留上一层像素 ⇒ 本判据红。
#[test]
fn buffer_pool_matches_fresh_allocation_byte_identical() {
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(64, 320, 320).unwrap();

    let mut pooled = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024).with_max_workers(1);
    let mut fresh = Renderer::with_budget(grid.clone(), 32 * 1024 * 1024).with_max_workers(1);
    fresh.buffer_pool().set_enabled(false);

    let a = pooled.render_document(&state, &store).unwrap();
    let b = fresh.render_document(&state, &store).unwrap();

    assert!(
        a.stats.layer_buffers_reused > 0,
        "池化路径必须真的发生复用，否则本条是空转：{:?}",
        a.stats
    );
    assert_eq!(b.stats.layer_buffers_reused, 0, "强制新建路径不应复用");
    assert_eq!(a.rgba8.len(), b.rgba8.len());
    assert_eq!(a.rgba8, b.rgba8, "池化与强制新建必须逐字节相同");

    // tile 缓存（客户端真正组合的那条路径）也必须逐字节相同。
    for key in grid.all_keys() {
        let left = pooled.render_tile(&state, &store, key).unwrap();
        let right = fresh.render_tile(&state, &store, key).unwrap();
        assert_eq!(
            left.to_f32(),
            right.to_f32(),
            "tile {key:?} 的 f16 像素在池化/新建下必须相同"
        );
    }
}

/// **缓冲池判据 2（资源，不是墙钟）**：一次全幅渲染里"分配 vs 复用"的计数必须是
/// `1 / (层数 − 1)`——首层建池，其余每层复用；第二次渲染（热池）应当 **0 次新建**。
///
/// 这正是"每层都新分配"会红的判据：变异为 `acquire` 跳过空闲表 ⇒
/// `allocated == 层数`、`reused == 0` ⇒ 红。
#[test]
fn buffer_pool_reuses_one_allocation_across_all_layers() {
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let mut renderer =
        Renderer::with_budget(TileGrid::new(64, 320, 320).unwrap(), 32 * 1024 * 1024)
            .with_max_workers(1);

    let first = renderer.render_document(&state, &store).unwrap();
    let layers = first.stats.layers;
    assert_eq!(layers, 3, "本判据按 3 层文档断言（口径变化时必须同步改）");
    assert_eq!(
        first.stats.layer_buffers_allocated + first.stats.layer_buffers_reused,
        layers,
        "每层恰好取用一次图层缓冲"
    );
    assert_eq!(
        first.stats.layer_buffers_allocated, 1,
        "只应新建 1 张（首层）"
    );
    assert_eq!(
        first.stats.layer_buffers_reused,
        layers - 1,
        "其余层必须复用"
    );

    // 池自身的计数与渲染计数一致（串行路径没有别的取用者）。
    let stats = renderer.buffer_pool().stats();
    assert_eq!(stats.allocated, 1);
    assert_eq!(stats.reused, layers - 1);
    assert_eq!(stats.alias_violations, 0);

    // 热池：第二次渲染一次都不该新建。
    let second = renderer.render_document(&state, &store).unwrap();
    assert_eq!(
        second.stats.layer_buffers_allocated, 0,
        "热池再渲染不应新建"
    );
    assert_eq!(second.stats.layer_buffers_reused, layers);
    assert_eq!(second.rgba8, first.rgba8, "热池渲染也必须逐字节相同");
}

/// 造一份"每个 worker 的工作集很大"的文档：`width × height` 画布、`layers` 层，
/// 每层只有一个 32×32 的矩形（内容成本与画布面积无关 ⇒ 差值只来自图层缓冲的分配）。
fn banded_document(width: u32, height: u32, layers: usize) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_pool_budget".to_owned());
    state.width = width;
    state.height = height;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    for index in 0..layers {
        let id = format!("layer_{index}");
        state.layers.insert(id.clone(), layer(&id, index as i64));
        state.objects.insert(
            format!("rect_{index}"),
            object(
                &format!("rect_{index}"),
                &id,
                ObjectType::Shape,
                index as i64,
                json!({"geometry": {"kind": "rect",
                                    "bbox": {"x": 16.0, "y": 16.0, "w": 32.0, "h": 32.0}},
                       "color": {"r": 20, "g": 40, "b": 80, "a": 200}}),
            ),
        );
    }
    state
}

/// **缓冲池判据 4（并发工作集 > 固定预算）——本条是"预算必须由真实并发需求推导"的核心判据**。
///
/// 场景：`WORKERS = 4` 个并行 worker，画布 `4096×3076` ⇒ 每块 769 行。每个 worker 的
/// 图层缓冲是 `4096 × 769 × 4 通道 × 4 B = 50_397_184 B ≈ 48.06 MiB`；4 块合计
/// `201_588_736 B`，**比旧的固定预算 192 MiB = 201_326_592 B 多 256 KiB**。
///
/// **为什么这个组合是忠实的、不是凑数**：并行渲染把 `rh` 行均分给 worker，每个 worker
/// 的工作集是 `rw × (rh / W)`，而 `W` 块之和**恒等于整幅** `rw × rh`。所以
/// "N 块之和 > 192 MiB" ⟺ "画布 > 192 MiB / 16 B = 12.58 M 像素"，与外部报告里
/// 8K（7680×4320、4 worker、每块 126.56 MiB、合计 506 MiB）是**同一个机制**；这里只是
/// 把画布按比例缩到本机能承载的最小值（12.6 M 像素，刚越过阈值 0.25 MiB），worker 数
/// 与报告一致（4）。阈值不可能靠"更小的画布"跨过——每块再小，4 块之和仍是整幅大小。
///
/// 断言的**被判条件**：冷池时首层每个 worker 各新建一块（4 次），此后每一层都必须复用；
/// 热池（同一渲染器的第二轮）**一次新建都不该有**。旧实现里空闲表最多留下
/// `floor(192 MiB / 50.4 MiB) = 3` 块 ⇒ 第 4 块每层都被丢弃 ⇒ 每层重新分配一块 48 MiB，
/// 即 5 层导出白白 churn 约 1.5 GB 的那个缺陷。
///
/// 变异：把归还规则改回"只按固定 192 MiB 预算"（或让 `acquire` 跳过空闲表）⇒ 红。
#[test]
fn buffer_pool_budget_covers_every_concurrent_worker() {
    const W: u32 = 4096;
    const H: u32 = 3076;
    const WORKERS: usize = 4;
    const LAYERS: usize = 2;
    const OLD_BUDGET: usize = 192 * 1024 * 1024;

    assert_eq!(
        H as usize % WORKERS,
        0,
        "本判据按均分带高断言（切块口径变化时必须同步改）"
    );
    let band_bytes = W as usize * (H as usize / WORKERS) * 16;
    assert!(
        band_bytes * WORKERS > OLD_BUDGET,
        "场景必须让 {WORKERS} 块并发缓冲（{} B）超过旧预算 {OLD_BUDGET} B，否则本条空转",
        band_bytes * WORKERS
    );

    let state = banded_document(W, H, LAYERS);
    let store = MemoryBlobStore::new();
    let grid = TileGrid::new(256, W, H).unwrap();
    let mut renderer = Renderer::with_budget(grid, 32 * 1024 * 1024).with_max_workers(WORKERS);

    let first = renderer.render_document(&state, &store).unwrap();
    assert_eq!(
        first.stats.parallel_workers, WORKERS,
        "判据要求真的并行到 {WORKERS} 个 worker（否则工作集只有一份，机制不成立）"
    );
    assert_eq!(
        first.stats.layer_buffers_allocated, WORKERS,
        "冷池首层只应为每个 worker 各新建一块；其余层必须复用。实测 分配/复用 = {}/{}",
        first.stats.layer_buffers_allocated, first.stats.layer_buffers_reused
    );
    assert_eq!(first.stats.layer_buffers_reused, (LAYERS - 1) * WORKERS);
    drop(first);

    // **热池**：池里已经躺着上一轮归还的全部并发工作集 ⇒ 本轮必须零新建。
    let second = renderer.render_document(&state, &store).unwrap();
    assert_eq!(
        second.stats.layer_buffers_allocated,
        0,
        "热池第二轮不得新建：{WORKERS} 个 worker 的工作集合计 {} B > 旧预算 {OLD_BUDGET} B，\
         旧实现每层都会丢掉一块、于是每层重新分配一块。实测 分配/复用 = {}/{}",
        band_bytes * WORKERS,
        second.stats.layer_buffers_allocated,
        second.stats.layer_buffers_reused
    );
    assert_eq!(second.stats.layer_buffers_reused, LAYERS * WORKERS);

    // **有界**（判据 2 的大画布版本）：预算由并发推导，但它仍是一个上界——
    // N 块并发缓冲之和就是整幅 ⇒ 池留不下超过"一次渲染的工作集"。
    let pool = renderer.buffer_pool().stats();
    assert_eq!(pool.alias_violations, 0, "借出/归还的 id 记账不得冲突");
    let frame_bytes = W as usize * H as usize * 16;
    let budget = renderer.buffer_pool().retained_byte_budget();
    assert!(
        pool.retained_bytes <= budget,
        "保留字节必须 ≤ 公布的预算：{} > {budget}",
        pool.retained_bytes
    );
    assert!(
        pool.retained_bytes <= frame_bytes,
        "保留字节不得超过一次渲染的工作集（N 块之和 = 整幅）：{} > {frame_bytes}",
        pool.retained_bytes
    );
    assert!(
        pool.retained_buffers <= MAX_POOLED_BUFFERS,
        "保留个数必须 ≤ {MAX_POOLED_BUFFERS}：{}",
        pool.retained_buffers
    );
}

/// **缓冲池判据 3（有界 + 不别名）**。
///
/// 有界：给定小上限（2 张 / 256 KiB），即使同时借出 10 张，归还后**保留**的也不超过上限
/// （超出上限的按需新建、归还即丢弃，而不是把池撑大）。另测"至少保留一张"的例外：
/// 超过字节预算的单张整幅缓冲仍保留一张（否则超大画布池化失效），但不会留下第二张。
///
/// 不别名（怎么断言）：把两个同时持有的租约的底层 `f32` 切片换算成内存区间，断言**不相交**；
/// 给其中一个写标记、另一个读不到；释放一个后重新借出的租约也不得与仍在借的那个重叠；
/// 最后断言池的 `alias_violations == 0`（借出/归还的 id 记账冲突计数）。
///
/// 变异：① 归还时去掉上限判断 ⇒ 保留数超过上限 ⇒ 红；
/// ② 归还时不从在借集合移除 id ⇒ 下一次借出命中冲突 ⇒ `alias_violations != 0` ⇒ 红。
#[test]
fn buffer_pool_is_bounded_and_never_aliases_a_live_lease() {
    fn range(slice: &[f32]) -> (usize, usize) {
        let start = slice.as_ptr() as usize;
        (start, start + std::mem::size_of_val(slice))
    }
    fn overlaps(a: (usize, usize), b: (usize, usize)) -> bool {
        a.0 < b.1 && b.0 < a.1
    }

    // 64×64×4 通道×4 B = 64 KiB/张。
    let pool = BufferPool::with_limits(2, 256 * 1024);
    let mut first = pool.acquire(0, 0, 64, 64);
    let second = pool.acquire(0, 0, 64, 64);
    let first_range = range(first.as_f32());
    assert!(
        !overlaps(first_range, range(second.as_f32())),
        "同时在借的两块缓冲不得共享内存"
    );
    first.set_pixel(1, 1, [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(
        second.pixel(1, 1),
        [0.0; 4],
        "另一块在借缓冲看不到本块的写入"
    );

    drop(second);
    let third = pool.acquire(0, 0, 64, 64);
    assert!(
        !overlaps(first_range, range(third.as_f32())),
        "归还后重借的缓冲不得与仍在借的缓冲重叠"
    );
    assert_eq!(third.pixel(1, 1), [0.0; 4], "复用的缓冲必须以全零开始");
    assert_eq!(
        pool.stats().alias_violations,
        0,
        "借出/归还的 id 记账不得冲突"
    );
    assert!(pool.stats().max_in_use >= 2, "应观测到 2 块同时在借");

    // 并发需要可以超过保留上限（按需新建），但归还后保留量不得超限。
    let others: Vec<_> = (0..8).map(|_| pool.acquire(0, 0, 64, 64)).collect();
    assert!(pool.stats().max_in_use >= 10, "10 块同时借出应被满足");
    drop(others);
    drop(third);
    drop(first);
    let stats = pool.stats();
    assert!(
        stats.retained_buffers <= 2,
        "保留个数必须 ≤ 上限：{}",
        stats.retained_buffers
    );
    assert!(
        stats.retained_bytes <= 256 * 1024,
        "保留字节必须 ≤ 上限：{}",
        stats.retained_bytes
    );

    // **至少保留一张**的例外：超过字节预算的单张缓冲仍要能留下（否则超大画布的池化
    // 完全失效），但个数上限照样成立、且不会留下第二张超预算的。
    let oversized = BufferPool::with_limits(2, 256 * 1024);
    let big_a = oversized.acquire(0, 0, 1024, 1024); // 1024² × 16 B = 16 MiB > 256 KiB
    let big_b = oversized.acquire(0, 0, 1024, 1024);
    drop(big_a);
    drop(big_b);
    let stats = oversized.stats();
    assert_eq!(
        stats.retained_buffers, 1,
        "超预算时也只保留一张（个数上限仍然生效）"
    );
    assert!(
        stats.retained_bytes > 256 * 1024,
        "这一张超预算的整幅缓冲必须被留下，否则超大画布无法复用：{}",
        stats.retained_bytes
    );
}

/// 判据 3 的**默认上限**版本：真实渲染器的池也不得超过**公布的规则**——防止默认值被
/// 改大、或预算推导写错之后悄悄失去上界。
///
/// 公布的规则（[`BufferPool::retained_byte_budget`]）是
/// `max(MIN_POOLED_BYTES, 并发 worker 数 × 本轮最大单块工作集)`；`MIN_POOLED_BYTES`
/// 只是**下界**，不再是硬上限（旧实现把它当硬上限，8K/4 worker 时每层丢 3 块）。
#[test]
fn default_buffer_pool_stays_within_its_published_limits() {
    let state = stress_document(320, false);
    let store = MemoryBlobStore::new();
    let mut renderer =
        Renderer::with_budget(TileGrid::new(64, 320, 320).unwrap(), 32 * 1024 * 1024)
            .with_max_workers(1);
    renderer.render_document(&state, &store).unwrap();
    let pool = renderer.buffer_pool();
    let stats = pool.stats();
    assert!(
        stats.retained_buffers <= MAX_POOLED_BUFFERS,
        "保留个数超过默认上限：{} > {MAX_POOLED_BUFFERS}",
        stats.retained_buffers
    );
    let budget = pool.retained_byte_budget();
    assert!(
        stats.retained_bytes <= budget,
        "保留字节超过公布预算：{} > {budget}",
        stats.retained_bytes
    );
    // 串行小画布（1.6 MiB/块）的预算就等于固定下界：并发 1 × 工作集 ≪ 192 MiB。
    assert_eq!(
        budget, MIN_POOLED_BYTES,
        "串行小画布的公布预算应等于下界 MIN_POOLED_BYTES"
    );
}

/// 造一张**确定性**、内容多样的缓冲区（alpha 覆盖 0 / 1 / 分数；颜色略越界以走 clamp）。
fn rgba8_probe_buffer(width: u32, height: u32, seed: u32) -> yanshi_render::Buffer {
    let mut buffer = yanshi_render::Buffer::new(0, 0, width, height);
    let mut state = seed | 1;
    for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((state >> 8) & 0xff) as f32 / 255.0;
        let b = ((state >> 16) & 0xff) as f32 / 255.0;
        let c = ((state >> 24) & 0xff) as f32 / 255.0;
        let alpha = match index % 5 {
            0 => 0.0,
            1 => 1.0,
            2 => 0.5,
            3 => a,
            _ => 1.0 - a,
        };
        pixel.copy_from_slice(&[b * 1.2 - 0.1, c, a * 0.5, alpha]);
    }
    buffer
}

/// 逐字节比较，失败时只报**第一处不同**与两边的字节数。
///
/// 不能用 `assert_eq!`：输出是 MB 级，断言失败会把整块向量打进日志，
/// 把真正的失败行淹掉（本专题的红/绿证据要能一眼读到是哪一行、哪个字节）。
fn assert_bytes_identical(label: &str, left: &[u8], right: &[u8]) {
    assert_eq!(left.len(), right.len(), "{label}：字节数不同");
    if let Some(index) = left.iter().zip(right).position(|(a, b)| a != b) {
        panic!(
            "{label}：第 {index} 字节不同（{} ≠ {}）",
            left[index], right[index]
        );
    }
}

/// **判据 4：`Buffer::to_rgba8` 的按行并行与串行逐字节相同**。
///
/// 与判据 1 是同一条不变量（"块与块没有共享像素 ⇒ 怎么切都不改变字节"），只是对象从
/// "整幅渲染"换成"输出量化"这条公开 API。强制串行的办法是 `workers = Some(1)`；
/// `Some(4)` 与 `None`（可用核数）都必须与它逐字节相同。
///
/// 变异演示：让并行分带**漏写一块**（或重叠写两次）⇒ `Some(4)` 与 `Some(1)` 的字节不同 ⇒
/// 本判据红；逐字改回 ⇒ 绿（专题报告有 red/green 原文与 `cmp` 复原证明）。
#[test]
fn to_rgba8_is_byte_identical_across_worker_counts() {
    // 大（确定走并行）、奇数高（分带不整除）、小（确定走串行）、宽而一行（只能切 1 块）
    // 四种形状都要覆盖。
    for (width, height) in [(512u32, 512u32), (1024, 257), (64, 64), (16_384, 1)] {
        let buffer = rgba8_probe_buffer(width, height, width * 31 + height);
        for background in [None, Some([255u8, 255, 255, 255]), Some([12, 200, 90, 255])] {
            let (serial, serial_blocks) = buffer.to_rgba8_blocks(background, Some(1));
            let (four, four_blocks) = buffer.to_rgba8_blocks(background, Some(4));
            let (auto, auto_blocks) = buffer.to_rgba8_blocks(background, None);
            assert_bytes_identical(
                &format!("{width}×{height} bg={background:?}：4 worker 与串行"),
                &serial,
                &four,
            );
            assert_bytes_identical(
                &format!("{width}×{height} bg={background:?}：自动 worker 与串行"),
                &serial,
                &auto,
            );
            assert_eq!(serial.len(), width as usize * height as usize * 4);
            assert_eq!(serial_blocks, 0, "Some(1) 必须走串行（0 块）");
            // 走并行的条件是「像素数 ≥ 128² **且** 行数 ≥ 2」（一行切不出两块）。
            let parallel_expected = width * height >= 128 * 128 && height >= 2;
            assert_eq!(
                four_blocks > 0,
                parallel_expected,
                "{width}×{height} 的并行/串行选边与阈值不符：four={four_blocks}"
            );
            assert!(
                four_blocks != 1,
                "{width}×{height}：分块数 1 与「串行」无法区分，按构造不应出现"
            );
            assert_eq!(
                auto_blocks > 0,
                parallel_expected,
                "{width}×{height} 的自动选边与阈值不符：auto={auto_blocks}"
            );
        }
    }
}

/// **判据 5：阈值是真的在选边** —— 小图必须走串行、阈值上与大图必须走并行，
/// 断言的是第二个返回值这个**语义计数**（`0` = 串行，`≥2` = 并行块数，沿用
/// `RenderStats::parallel_chunks` 的口径），不是墙钟（墙钟会被机器负载骗过）。
///
/// 变异演示：阈值改 `0` ⇒ 64² 也切成 4 块 ⇒ 红；阈值改 `usize::MAX` ⇒ 128²/512² 都串行 ⇒
/// 红；去掉"行数够切"那一项 ⇒ 16384×1 也起 1 个 worker ⇒ 红；逐字改回 ⇒ 绿。
#[test]
fn to_rgba8_threshold_selects_the_serial_and_parallel_paths() {
    // 阈值以下：64² = 4096 px < 128² = 16384 px ⇒ 即使给 4 个 worker 也不启线程。
    let small = rgba8_probe_buffer(64, 64, 1);
    let (_, small_blocks) = small.to_rgba8_blocks(None, Some(4));
    assert_eq!(small_blocks, 0, "小图必须走串行路径");

    // 恰好在阈值上：128² = 16384 px ⇒ 走并行（判据是 `<` 阈值，不是 `<=`）。
    let edge = rgba8_probe_buffer(128, 128, 2);
    let (_, edge_blocks) = edge.to_rgba8_blocks(None, Some(4));
    assert_eq!(edge_blocks, 4, "阈值上的图应切成 4 块");

    // 明显大于阈值 ⇒ 走并行。
    let large = rgba8_probe_buffer(512, 300, 3);
    let (_, large_blocks) = large.to_rgba8_blocks(None, Some(4));
    assert_eq!(large_blocks, 4, "大图应切成 4 块");

    // 单 worker：大图也不启线程（判据 4 的"串行参照"就建立在这一点上）。
    let (_, forced_serial) = large.to_rgba8_blocks(None, Some(1));
    assert_eq!(forced_serial, 0, "workers=Some(1) 必须走串行");

    // **宽而只有一行**：像素数够了（16384 = 128²），但按行只能切出 1 块 ⇒ 没有并行可言，
    // 起线程是纯开销 ⇒ 必须走串行（阈值不只按像素数，还要求行数够切）。
    let single_row = rgba8_probe_buffer(16_384, 1, 4);
    let (_, single_row_blocks) = single_row.to_rgba8_blocks(None, Some(4));
    assert_eq!(single_row_blocks, 0, "只有一行时不应起线程");
    // 两行就能切成 2 块 ⇒ 走并行（同一像素量级，区别只在行数）。
    let two_rows = rgba8_probe_buffer(16_384, 2, 5);
    let (_, two_rows_blocks) = two_rows.to_rgba8_blocks(None, Some(4));
    assert_eq!(two_rows_blocks, 2, "两行应切成 2 块");

    // 空缓冲：没有像素可算 ⇒ 0 块，输出也是空的。
    let empty = yanshi_render::Buffer::new(0, 0, 0, 0);
    let (bytes, empty_blocks) = empty.to_rgba8_blocks(None, Some(4));
    assert!(bytes.is_empty());
    assert_eq!(empty_blocks, 0, "空缓冲不应分块");
}

/// **判据（第 206 轮 ✓）**：**小区域渲染只应取用"覆盖它的那些块"** ✓ ——
/// **整幅路**：为渲 64² 要读**整幅**（`side²×4` ＝ 1 MiB ✓）✗；
/// **分块路**：只读**覆盖的那 1 块**（256 KiB ✓）✓ ⇒ **∴ 上限取 512 KiB ⇒ 可红 ✓**。
/// **变异** ✗：让分块路取全部块 ⇒ 必红 ✓。
#[test]
fn a_small_region_reads_only_the_tiles_it_needs() {
    let side = 512u32;
    let store = CountingStore::new();
    let pixels: Vec<u8> = (0..(side as usize) * (side as usize) * 4)
        .map(|i| (i % 251) as u8)
        .collect();
    let blob = yanshi_core::blob::stage_blob(&store, &pixels, "image/x-yanshi-raw").unwrap();
    // **建分块索引** ✓（切块 ⇒ 逐块存 ⇒ 写索引 blob ✓）
    let parts = yanshi_render::bitmap_tiles::split_into_tiles(side, side, &pixels).unwrap();
    let mut hashes = Vec::with_capacity(parts.len());
    for part in &parts {
        hashes.push(
            yanshi_core::blob::stage_blob(&store, part, "image/x-yanshi-raw")
                .unwrap()
                .blob_hash
                .to_string(),
        );
    }
    let index = yanshi_render::bitmap_tiles::BitmapIndex {
        v: yanshi_render::bitmap_tiles::BITMAP_INDEX_VERSION,
        tile: yanshi_render::bitmap_tiles::BITMAP_TILE,
        width: side,
        height: side,
        tiles: hashes,
    };
    let index_blob = yanshi_core::blob::stage_blob(
        &store,
        &serde_json::to_vec(&index).unwrap(),
        "application/json",
    )
    .unwrap();
    let mut state = stress_document(side, false);
    state.objects.insert(
        "patch_tiles".to_owned(),
        object(
            "patch_tiles",
            "layer_bottom",
            ObjectType::RasterPatch,
            4,
            json!({"bitmap": blob, "width": side, "height": side,
                   "region": {"x": 0, "y": 0, "w": side, "h": side},
                   "tiles": index_blob.blob_hash.to_string()}),
        ),
    );
    let grid = TileGrid::new(64, side, side).unwrap();
    let mut renderer = Renderer::with_budget(grid, 64 * 1024 * 1024).with_max_workers(1);
    store.reset();
    let out = renderer
        .render_region(&state, &store, Bbox::new(0.0, 0.0, 64.0, 64.0))
        .unwrap();
    let read = store.bytes();
    assert!(!out.rgba8.is_empty(), "小区域必须渲出像素 ✓");
    assert!(
        read <= 512 * 1024,
        "**小区域只应取用覆盖它的块**（1 块 ＝ 256 KiB ✓；实测取用 {read} 字节 —— \
         若 ≥ 1 MiB 说明仍在读整幅 ✗）"
    );
}

/// **判据（第 208 轮 ✓）**：**同一批像素，存成分块（带索引 ✓）与存成整幅 ⇒ 渲染结果必须逐字节相同** ✓✓
/// —— **∴ 这是"分块不许改画面"的硬守卫 ✓**（**变异** ✗：把区域坐标／块号弄错 ⇒ 必红 ✓）。
#[test]
fn tiled_and_plain_bitmaps_render_identically() {
    let side = 512u32;
    let pixels: Vec<u8> = (0..(side as usize) * (side as usize) * 4)
        .map(|i| ((i * 7 + 13) % 251) as u8)
        .collect();
    let store = CountingStore::new();
    // ① **整幅存的版本** ✓
    let plain = yanshi_core::blob::stage_blob(&store, &pixels, "image/x-yanshi-raw").unwrap();
    // ② **分块 ＋ 索引的版本** ✓（**同一批像素 ✓**）
    let parts = yanshi_render::bitmap_tiles::split_into_tiles(side, side, &pixels).unwrap();
    let mut hashes = Vec::with_capacity(parts.len());
    for part in &parts {
        hashes.push(
            yanshi_core::blob::stage_blob(&store, part, "image/x-yanshi-raw")
                .unwrap()
                .blob_hash
                .to_string(),
        );
    }
    let index = yanshi_render::bitmap_tiles::BitmapIndex {
        v: yanshi_render::bitmap_tiles::BITMAP_INDEX_VERSION,
        tile: yanshi_render::bitmap_tiles::BITMAP_TILE,
        width: side,
        height: side,
        tiles: hashes,
    };
    let index_blob = yanshi_core::blob::stage_blob(
        &store,
        &serde_json::to_vec(&index).unwrap(),
        "application/json",
    )
    .unwrap();
    // ③ **两份文档：除 bitmap 之外完全相同** ✓
    let build = |data: serde_json::Value| {
        let mut state = stress_document(side, false);
        state.objects.insert(
            "patch".to_owned(),
            object("patch", "layer_bottom", ObjectType::RasterPatch, 4, data),
        );
        state
    };
    let plain_state = build(json!({"bitmap": plain, "width": side, "height": side,
                                   "region": {"x": 0, "y": 0, "w": side, "h": side}}));
    let tiled_state = build(
        json!({"bitmap": plain.clone(), "width": side, "height": side,
                                   "region": {"x": 0, "y": 0, "w": side, "h": side},
                                   "tiles": index_blob.blob_hash.to_string()}),
    );
    let grid = || TileGrid::new(64, side, side).unwrap();
    let mut r1 = Renderer::with_budget(grid(), 64 * 1024 * 1024).with_max_workers(1);
    let mut r2 = Renderer::with_budget(grid(), 64 * 1024 * 1024).with_max_workers(1);
    // ④ **跨块边界的区域** ✓（250,250,300,260 ⇒ 覆盖 2×2 块 ✓）
    let region = Bbox::new(250.0, 250.0, 300.0, 260.0);
    let a = r1.render_region(&plain_state, &store, region).unwrap();
    let b = r2.render_region(&tiled_state, &store, region).unwrap();
    assert_eq!(a.rgba8.len(), b.rgba8.len(), "尺寸必须相同");
    assert!(a.rgba8 == b.rgba8, "**分块路径必须与整幅路径逐字节相同** ✗");
}
