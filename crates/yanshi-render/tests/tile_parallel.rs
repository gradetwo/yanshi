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
//! 每个判据都做过**变异演示**（改坏被测条件 ⇒ 判红 ⇒ 逐字改回），见专题报告。

use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use yanshi_core::atom::BlobHash;
use yanshi_core::blob::{BlobEntry, BlobStore, MemoryBlobStore};
use yanshi_core::{
    DocumentState, Layer, LayerType, Object, ObjectType, Result, Selection, Transform,
};
use yanshi_render::render::Renderer;
use yanshi_render::tile::{TileGrid, TileKey};

/// 数 `get` 次数的存储包装 —— 用来证明"分块并行时同一份位图只读一次"（缓存真的生效）。
struct CountingStore {
    inner: MemoryBlobStore,
    gets: AtomicUsize,
}

impl CountingStore {
    fn new() -> Self {
        Self {
            inner: MemoryBlobStore::new(),
            gets: AtomicUsize::new(0),
        }
    }
    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
    fn reset(&self) {
        self.gets.store(0, Ordering::SeqCst);
    }
}

impl BlobStore for CountingStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.inner.put(bytes)
    }
    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        self.gets.fetch_add(1, Ordering::SeqCst);
        self.inner.get(hash)
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
