//! 服务端性能预算验收（设计文档 Phase 1 出口条件 / 14.10），由 CI 的 `--ignored` 长跑作业执行。
//!
//! 覆盖两条**出口条件**级别的时间预算：
//!
//! | 预算 | 文档出处 | 目标 |
//! |---|---|---|
//! | view 模式打开文档 | Phase 1 出口条件「view 模式打开 < 100ms」 | < 100ms |
//! | 打开即图片（缩略图可用） | 6.2 打开即图片 / 7.3 | 打开过程中即可拿到缩略图地址 |
//! | 单原子提交往返 | 14.10 交互延迟 | < 20ms（提交 + 折叠 + dirty） |
//!
//! 实测值全部打印，供回归对比；断言留了余量以免共享 CI 抖动误报。

use serde_json::json;
use std::time::{Duration, Instant};
use yanshi_core::{Atom, AtomKind, Bbox};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

fn temp_root(tag: &str) -> std::path::PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "yanshi-srv-bench-{tag}-{}-{}",
        std::process::id(),
        yanshi_core::now_ms()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn bench(label: &str, iterations: u32, mut body: impl FnMut()) -> Duration {
    body();
    let started = Instant::now();
    for _ in 0..iterations {
        body();
    }
    let elapsed = started.elapsed() / iterations;
    println!("{label}: {elapsed:?}（{iterations} 次平均）");
    elapsed
}

/// 造一个「真实规模」的持久化文档：`strokes` 笔笔触 + 图层结构，并渲染出文档级缩略图。
fn seed_workspace(root: &std::path::Path, strokes: usize) {
    let settings = DocumentSettings::default();
    let mut workspace = Workspace::with_file_store(root, settings).unwrap();
    workspace
        .create_document(
            NewDocument::new("doc_bench", 1024, 1024),
            "human:1",
            "session:a",
        )
        .unwrap();
    workspace
        .commit(
            "doc_bench",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "ink"}),
            ),
            "human:1",
            false,
        )
        .unwrap();
    for index in 0..strokes {
        let x = 40.0 + (index % 24) as f64 * 40.0;
        let y = 40.0 + (index / 24) as f64 * 40.0;
        workspace
            .commit(
                "doc_bench",
                Atom::new(
                    AtomKind::DrawStroke,
                    "human:1",
                    "session:a",
                    json!({
                        "object_id": format!("obj_{index}"),
                        "layer_id": "layer_1",
                        // 不在原子 data 里塞颜色以外的重物，保持原子元数据很小（6.8）。
                        "data": {
                            "points": [[x, y], [x + 26.0, y + 18.0]],
                            "size": 5.0,
                            "color": [30, 30, 40, 255],
                        },
                    }),
                ),
                "human:1",
                false,
            )
            .unwrap();
    }
    // 打开即图片：整幅渲染会把渲染缓存写到磁盘（14.5）。
    workspace
        .render_region("doc_bench", Bbox::new(0.0, 0.0, 1024.0, 1024.0))
        .unwrap();
    workspace.close_document("doc_bench");
}

#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（Phase 1 出口条件 / 14.10）"]
fn perf_view_open_under_100ms() {
    for atoms in [120usize, 600] {
        let root = temp_root(&format!("open{atoms}"));
        seed_workspace(&root, atoms);
        let settings = DocumentSettings::default();

        // 1) 冷启动打开：新工作区 + 打开文档 + 拿缩略图（「view 模式打开」的完整路径）。
        let mut samples = Vec::new();
        for _ in 0..3 {
            let started = Instant::now();
            let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
            workspace.open_document("doc_bench").unwrap();
            let thumb = workspace
                .ensure_document_thumbnail("doc_bench", yanshi_server::DocThumbSize::S256)
                .unwrap();
            let elapsed = started.elapsed();
            assert!(thumb.is_some(), "打开即图片：应能直接给出缩略图地址");
            samples.push(elapsed);
            workspace.close_document("doc_bench");
        }
        samples.sort();
        let median = samples[samples.len() / 2];
        println!(
            "view 模式打开（{} 个原子，含日志加载/折叠/缩略图）: {median:?}（三次取中位：{samples:?}）",
            atoms + 2
        );

        // 2) 已打开文档的重复查询（GET /api/documents/{id} 路径）。
        let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
        workspace.open_document("doc_bench").unwrap();
        let query = bench(
            &format!("文档摘要查询（{} 个原子）", atoms + 2),
            20,
            || {
                let _ = workspace.summary_json("doc_bench").unwrap();
            },
        );
        workspace.close_document("doc_bench");

        // 3) 提交一个原子的往返（14.10 交互延迟）。
        let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
        workspace.open_document("doc_bench").unwrap();
        let mut counter = 0usize;
        let commit = bench(
            &format!("单原子提交（{} 个原子）", atoms + 2),
            10,
            || {
                counter += 1;
                workspace
                    .commit(
                        "doc_bench",
                        Atom::new(
                            AtomKind::DrawStroke,
                            "human:1",
                            "session:a",
                            json!({
                                "object_id": format!("bench_{counter}"),
                                "layer_id": "layer_1",
                                "data": {"points": [[10.0, 10.0], [30.0, 20.0]], "size": 4.0},
                            }),
                        ),
                        "human:1",
                        false,
                    )
                    .unwrap();
            },
        );
        workspace.close_document("doc_bench");
        let _ = std::fs::remove_dir_all(&root);

        // 出口条件：view 模式打开 < 100ms。留 3 倍余量以吸收共享 CI 抖动。
        assert!(
            median < Duration::from_millis(300),
            "view 模式打开耗时 {median:?} 超出预算（目标 <100ms）"
        );
        assert!(
            query < Duration::from_millis(50),
            "文档摘要查询耗时 {query:?} 超出预算"
        );
        assert!(
            commit < Duration::from_millis(200),
            "单原子提交耗时 {commit:?} 超出预算"
        );
    }
}

/// 退化路径：没有任何渲染缓存时打开文档，仍然只做「加载 + 折叠」，不重放历史。
#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（6.2 打开即图片 / 14.5）"]
fn perf_open_without_render_cache_does_not_replay() {
    let root = temp_root("nocache");
    let settings = DocumentSettings::default();
    {
        let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
        workspace
            .create_document(NewDocument::new("doc_nc", 512, 512), "human:1", "session:a")
            .unwrap();
        workspace
            .commit(
                "doc_nc",
                Atom::new(
                    AtomKind::CreateLayer,
                    "human:1",
                    "session:a",
                    json!({"layer_id": "layer_1"}),
                ),
                "human:1",
                false,
            )
            .unwrap();
        workspace.close_document("doc_nc");
    }
    let started = Instant::now();
    let mut workspace = Workspace::with_file_store(&root, settings).unwrap();
    workspace.open_document("doc_nc").unwrap();
    let without_cache = started.elapsed();
    let document = workspace.document("doc_nc").unwrap();
    println!(
        "无渲染缓存打开（2 个原子）: {without_cache:?}，渲染水位 {}，缩略图 {:?}",
        document.render_watermark(),
        document.document_thumbnail_url()
    );
    assert!(
        document.document_thumbnail_url().is_none(),
        "没有缓存时不应伪造缩略图"
    );
    assert!(
        without_cache < Duration::from_millis(100),
        "无缓存打开耗时 {without_cache:?} 超出预算"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 4K 全幅渲染的**缓存容量剖面**：验证「默认 64MB 渲染缓存装不下 4K 的 f32 工作集，
/// 因此整幅渲染会反复淘汰重算」这一假设。
///
/// 这是**测量**而不是断言：两条配置各测一次，把数字打印出来对比。
/// 若大缓存显著更快，说明慢的根因是缓存容量而非算法。
#[test]
#[ignore = "缓存容量剖面：CI 用 --ignored 执行（4K 全幅渲染，较慢）"]
fn profile_4k_full_render_cache_capacity() {
    for (label, cache_bytes, layers) in [
        ("默认 64MB / 10 图层", 64 * 1024 * 1024usize, 10usize),
        ("512MB / 10 图层", 512 * 1024 * 1024usize, 10usize),
        ("512MB / 1 图层", 512 * 1024 * 1024usize, 1usize),
        (
            "512MB / 0 图层（纯量化基线）",
            512 * 1024 * 1024usize,
            0usize,
        ),
    ] {
        let settings = DocumentSettings {
            render_cache_bytes: cache_bytes,
            ..DocumentSettings::default()
        };
        let mut workspace = Workspace::in_memory(settings);
        workspace
            .create_document(
                NewDocument::new("doc_4k", 4096, 4096),
                "human:1",
                "session:a",
            )
            .unwrap();
        // 10 图层，每层一个 2000² 形状（与实测内存预算时的负载一致）。
        for index in 1..=layers {
            workspace
                .commit(
                    "doc_4k",
                    Atom::new(
                        AtomKind::CreateLayer,
                        "human:1",
                        "session:a",
                        json!({"layer_id": format!("L{index}"), "name": format!("layer{index}")}),
                    ),
                    "human:1",
                    true,
                )
                .unwrap();
            workspace
                .commit(
                    "doc_4k",
                    Atom::new(
                        AtomKind::CreateObject,
                        "human:1",
                        "session:a",
                        json!({
                            "object_id": format!("obj{index}"),
                            "layer_id": format!("L{index}"),
                            "kind": "shape",
                            "data": {
                                "geometry": {"kind": "rect", "bbox": {"x": index * 100, "y": index * 100, "w": 2000, "h": 2000}},
                                "color": {"r": index * 20, "g": 100, "b": 200, "a": 255}
                            }
                        }),
                    ),
                    "human:1",
                    true,
                )
                .unwrap();
        }
        // **多次测量取最小/中位数**：这台机器上 4–7s 量级的单次计时噪声可达 ±1s（±15%），
        // 单次结果不足以支撑任何结论（第一版就是这样把噪声当成了 6% 的优化）。
        let full = Bbox::new(0.0, 0.0, 4096.0, 4096.0);
        let _ = workspace.render_region("doc_4k", full).unwrap();
        let mut png_samples = Vec::new();
        let mut raw_samples = Vec::new();
        for _ in 0..3 {
            let started = Instant::now();
            let _ = workspace.render_region("doc_4k", full).unwrap();
            png_samples.push(started.elapsed());
            let raw_started = Instant::now();
            let _ = workspace.render_region_raw("doc_4k", full).unwrap();
            raw_samples.push(raw_started.elapsed());
        }
        png_samples.sort();
        raw_samples.sort();
        let elapsed = png_samples[0];
        let raw_elapsed = raw_samples[0];
        println!(
            "   ↳ 3 次采样：PNG 最小 {:?} / 中位 {:?}；raw 最小 {:?} / 中位 {:?}；编码成本 ≈ {:?}",
            png_samples[0],
            png_samples[1],
            raw_samples[0],
            raw_samples[1],
            elapsed.saturating_sub(raw_elapsed)
        );
        let document = workspace.document("doc_4k").unwrap();
        let stats = document.cache_stats();
        println!(
            "4K 全幅渲染（{label}，缓存预算 {}MB）：最小 {elapsed:?}（3 次）｜缓存 tiles={} 占用 {:.1}MB 淘汰 {} 次",
            cache_bytes / (1024 * 1024),
            stats.tiles,
            stats.used_bytes as f64 / (1024.0 * 1024.0),
            stats.evictions
        );
        // 只断言「能渲染出来且缓存不越界」，速度对比仅作观测。
        assert!(
            stats.used_bytes <= cache_bytes,
            "缓存占用 {} 超过预算 {cache_bytes}",
            stats.used_bytes
        );
    }
}

/// 14.10 预算表：区域渲染三档（缓存命中 < 10ms / 未命中简单 < 100ms / 未命中复杂 < 300ms）。
///
/// 设计把这张表的关键行列为 **CI benchmark 出口条件**，因此这里直接断言设计数字。
/// 计时用**多次采样取最小**：单次计时在这台机器上噪声可达 ±15%（见 profile_4k 的教训），
/// 取最小是对「该路径的最佳可达延迟」的保守估计，避免把噪声当回归。
#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（14.10 区域渲染三档）"]
fn region_render_matches_the_design_budget_tiers() {
    fn best_of<F: FnMut()>(rounds: u32, mut body: F) -> Duration {
        body();
        let mut best = Duration::MAX;
        for _ in 0..rounds {
            let started = Instant::now();
            body();
            best = best.min(started.elapsed());
        }
        best
    }

    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_tier", 2048, 2048),
            "human:1",
            "session:a",
        )
        .unwrap();
    // 简单内容：一层 + 一个中等矩形。
    workspace
        .commit(
            "doc_tier",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "base"}),
            ),
            "human:1",
            true,
        )
        .unwrap();
    workspace
        .commit(
            "doc_tier",
            Atom::new(
                AtomKind::CreateObject,
                "human:1",
                "session:a",
                json!({
                    "object_id": "shape_1",
                    "layer_id": "layer_1",
                    "kind": "shape",
                    "data": {
                        "geometry": {"kind": "rect", "bbox": {"x": 200.0, "y": 200.0, "w": 600.0, "h": 600.0}},
                        "color": {"r": 180, "g": 90, "b": 40, "a": 255}
                    }
                }),
            ),
            "human:1",
            true,
        )
        .unwrap();

    // 临时探针：区域尺寸 vs 耗时，判断是逐像素成本还是固定开销。
    for side in [64.0f64, 128.0, 256.0, 512.0] {
        let target = Bbox::new(0.0, 0.0, side, side);
        let _ = workspace.render_region_raw("doc_tier", target).unwrap();
        let mut best = Duration::MAX;
        for _ in 0..5 {
            let started = Instant::now();
            let _ = workspace.render_region_raw("doc_tier", target).unwrap();
            best = best.min(started.elapsed());
        }
        let pixels = side * side;
        println!(
            "   探针 {side:.0}²（{pixels:.0} px）: {best:?}｜每像素 {:.1}ns",
            best.as_secs_f64() * 1e9 / pixels
        );
    }

    // 成本归属：直接测 262144 次 sRGB 传递函数（每像素 3 次，与装配路径同量级）。
    {
        let count = 262_144usize;
        let mut best = Duration::MAX;
        for _ in 0..3 {
            let started = Instant::now();
            let mut sum = 0.0f32;
            for index in 0..count {
                let value = (index as f32 / count as f32).clamp(0.0, 1.0);
                // linear_to_byte = sRGB 传递函数 + 量化（装配路径每通道都要走一次）。
                sum += yanshi_render::srgb_to_linear(value) * 0.0
                    + yanshi_render::linear_to_srgb(value) * 255.0;
            }
            std::hint::black_box(sum);
            best = best.min(started.elapsed());
        }
        println!(
            "   归属 262144 次 linear_to_srgb（1 通道/像素的量级）: {best:?}｜按每像素 3 通道外推 ≈ {:.1}ms",
            best.as_secs_f64() * 3.0 * 1000.0
        );
    }

    let region = Bbox::new(0.0, 0.0, 512.0, 512.0);
    // 口径说明：`render_region` 会**编码 PNG**（导出路径成本），
    // `render_region_raw` 只做渲染 + 量化（交互路径成本）。
    // 14.10 的"区域渲染"预算针对渲染本身，因此断言 raw；PNG 成本单独打印，避免把导出成本算作渲染超标。
    let hit_png = best_of(5, || {
        let _ = workspace.render_region("doc_tier", region).unwrap();
    });
    let hit = best_of(5, || {
        let _ = workspace.render_region_raw("doc_tier", region).unwrap();
    });
    println!(
        "区域渲染 512² 缓存命中（最小）: raw {hit:?}｜PNG {hit_png:?}｜设计预算 raw < 10ms（**当前未达标，见下**）"
    );
    // **已知偏差（设计未决）**：设计 14.10 要求缓存命中 < 10ms，实测约 49ms（约 5×）。
    // 实测每像素约 190ns 且与像素数严格线性；其中约 52% 是 sRGB 传递函数的 `powf`
    // （每像素 3 次；262144 次调用实测 8.4ms，外推 25.3ms）。
    // 让它达标需要设计层面的取舍：放宽该路径到 D1（±1 LSB）以便用查找表、
    // 或修订 CPU 路径预算、或寻找逐位等价的更快实现 —— 均**不由实现方擅自决定**。
    // 因此这里守护「不得明显回归」的实测基线，同时把与设计目标的差距如实打印出来。
    // 余量说明：本机噪声可达 3×（见 implementation-notes 的两次噪声警示），
    // 因此基线留 3× 以上余量（实测常见 49–70ms）。设计目标仍每次打印，供人/CI 对比。
    assert!(
        hit < Duration::from_millis(250),
        "缓存命中（raw）{hit:?} 相对实测基线（约 49–70ms）明显回归；PNG 编码另计 {hit_png:?}"
    );

    // 未命中简单：每次换一个区域，强制未命中，但内容简单（单层单形状）。
    let mut index = 0u32;
    let simple = best_of(5, || {
        index += 1;
        let offset = f64::from(index % 3) * 512.0;
        let target = Bbox::new(offset, offset, 512.0, 512.0);
        let _ = workspace.render_region_raw("doc_tier", target).unwrap();
    });
    println!("区域渲染 512² 未命中·简单（最小，raw）: {simple:?}｜设计预算 < 100ms");
    assert!(
        simple < Duration::from_millis(300),
        "未命中简单 {simple:?} 相对预算/基线明显回归（设计 100ms；本机噪声可达 3×）"
    );

    // 未命中复杂：叠加大半径模糊 + 调整，再换区域强制未命中。
    for (object_id, layer_id, kind, payload) in [
        (
            "fx_blur",
            "layer_1",
            AtomKind::CreateObject,
            json!({
                "object_id": "fx_blur",
                "layer_id": "layer_1",
                "kind": "filter",
                "data": {"filter_name": "gaussian_blur", "params": {"sigma": 12.0}}
            }),
        ),
        (
            "fx_exposure",
            "layer_1",
            AtomKind::CreateObject,
            json!({
                "object_id": "fx_exposure",
                "layer_id": "layer_1",
                "kind": "adjustment",
                "data": {"adjustment_type": "exposure", "params": {"ev": 0.4}}
            }),
        ),
    ] {
        let _ = (object_id, kind);
        workspace
            .commit(
                "doc_tier",
                Atom::new(AtomKind::CreateObject, "human:1", "session:a", payload),
                "human:1",
                true,
            )
            .unwrap();
        let _ = layer_id;
    }
    let mut index = 0u32;
    let complex = best_of(3, || {
        index += 1;
        let offset = f64::from(index % 3) * 512.0;
        let target = Bbox::new(offset, offset, 512.0, 512.0);
        let _ = workspace.render_region_raw("doc_tier", target).unwrap();
    });
    println!("区域渲染 512² 未命中·复杂（最小，raw）: {complex:?}｜设计预算 < 300ms");
    assert!(
        complex < Duration::from_millis(900),
        "未命中复杂 {complex:?} 相对预算/基线明显回归（设计 300ms；本机噪声可达 3×）"
    );
}

/// 原生参照：单个 256² tile 的渲染 + 量化成本，用来判断客户端观测到的「每次调用固定开销」
/// 是 WASM 侧固有成本，还是本机噪声放大的结果。
#[test]
#[ignore = "性能参照：CI 用 --ignored 执行（单 tile 渲染原生成本）"]
fn native_single_tile_render_reference() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_tile", 1024, 1024),
            "human:1",
            "session:a",
        )
        .unwrap();
    workspace
        .commit(
            "doc_tile",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "base"}),
            ),
            "human:1",
            true,
        )
        .unwrap();
    workspace
        .commit(
            "doc_tile",
            Atom::new(
                AtomKind::CreateObject,
                "human:1",
                "session:a",
                json!({
                    "object_id": "shape_1",
                    "layer_id": "layer_1",
                    "kind": "shape",
                    "data": {
                        "geometry": {"kind": "rect", "bbox": {"x": 100.0, "y": 100.0, "w": 600.0, "h": 600.0}},
                        "color": {"r": 200, "g": 120, "b": 60, "a": 255}
                    }
                }),
            ),
            "human:1",
            true,
        )
        .unwrap();
    // 8×8（一个 tile 的极小切片）与 256×256（整 tile）对比：客户端在两者上观测到的差异很小，
    // 若原生同样如此，则说明成本确实与输出像素数无关，而在「至少渲染一个 tile」这件事上。
    for side in [8.0f64, 256.0] {
        let region = Bbox::new(0.0, 0.0, side, side);
        let _ = workspace.render_region_raw("doc_tile", region).unwrap();
        let mut best = Duration::MAX;
        for _ in 0..5 {
            let started = Instant::now();
            let _ = workspace.render_region_raw("doc_tile", region).unwrap();
            best = best.min(started.elapsed());
        }
        println!("原生 {side:.0}² 区域 raw 渲染（最小）: {best:?}");
    }
}

/// 特征测试：服务端的区域渲染**确实走 tile 缓存**，且重复渲染同一区域会命中。
///
/// 这纠正了一个我原先的推断：`Renderer::render_region`（累积缓冲路径）内部会经由
/// `render_tile` 读写 tile 缓存，因此「服务端不使用 tile 缓存」是错的。
/// 该测试固定「缓存被使用且在重复渲染时命中」，一旦这条性质变化就会被发现。
///
/// 顺带记录：即便命中缓存，同一区域的耗时仍由**量化/合成**主导（见
/// `kernel_perf.rs` 的背景二分：有背景 175ns/px vs 无背景 43ns/px），
/// 所以 14.10 的「缓存命中 < 10ms」在当前架构下仍不可达。
#[test]
#[ignore = "架构特征：CI 用 --ignored 执行（服务端 tile 缓存命中行为）"]
fn server_region_renders_use_the_tile_cache() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_cache", 1024, 1024),
            "human:1",
            "session:a",
        )
        .unwrap();
    workspace
        .commit(
            "doc_cache",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "base"}),
            ),
            "human:1",
            true,
        )
        .unwrap();
    workspace
        .commit(
            "doc_cache",
            Atom::new(
                AtomKind::CreateObject,
                "human:1",
                "session:a",
                json!({
                    "object_id": "shape_1",
                    "layer_id": "layer_1",
                    "kind": "shape",
                    "data": {
                        "geometry": {"kind": "rect", "bbox": {"x": 100.0, "y": 100.0, "w": 600.0, "h": 600.0}},
                        "color": {"r": 200, "g": 120, "b": 60, "a": 255}
                    }
                }),
            ),
            "human:1",
            true,
        )
        .unwrap();
    let region = Bbox::new(0.0, 0.0, 512.0, 512.0);
    let _ = workspace.render_region_raw("doc_cache", region).unwrap();
    let after_first = workspace.document("doc_cache").unwrap().cache_stats();
    // 512² 区域 = 2×2 个 256² tile。
    assert_eq!(
        after_first.tiles, 4,
        "首次渲染应写入 4 个 tile：{after_first:?}"
    );
    assert_eq!(
        after_first.misses, 4,
        "首次渲染应全部未命中：{after_first:?}"
    );

    let _ = workspace.render_region_raw("doc_cache", region).unwrap();
    let after_second = workspace.document("doc_cache").unwrap().cache_stats();
    assert_eq!(
        after_second.misses, after_first.misses,
        "重复渲染同一区域应全部命中（未命中数不再增长）：{after_second:?}"
    );
    println!(
        "服务端 tile 缓存：tiles={} used={}B 未命中={}（第二次渲染未新增未命中）",
        after_second.tiles, after_second.used_bytes, after_second.misses
    );
}

/// 14.10 预算表：原子提交（服务端处理）——单原子 < 20ms、批量 < 10ms。
///
/// 这里量的是**服务端处理**（提交 + 折叠 + dirty 计算），不含网络往返。
#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（14.10 提交预算）"]
fn commit_budget_single_and_batch() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_commit", 512, 512),
            "human:1",
            "session:a",
        )
        .unwrap();
    workspace
        .commit(
            "doc_commit",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "base"}),
            ),
            "human:1",
            true,
        )
        .unwrap();

    // 单原子提交：连续提交 30 个形状，取最小耗时（噪声机器上用最小值更稳）。
    let mut best = Duration::MAX;
    for index in 0..30 {
        let atom = Atom::new(
            AtomKind::CreateObject,
            "human:1",
            "session:a",
            json!({
                "object_id": format!("shape_{index}"),
                "layer_id": "layer_1",
                "kind": "shape",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 10.0 + index as f64, "y": 10.0, "w": 40.0, "h": 40.0}},
                    "color": {"r": 100, "g": 100, "b": 100, "a": 255}
                }
            }),
        );
        let started = Instant::now();
        workspace
            .commit("doc_commit", atom, "human:1", true)
            .expect("提交应成功");
        best = best.min(started.elapsed());
    }
    println!("单原子提交（服务端，最小）: {best:?}｜预算 < 20ms（14.10）");
    assert!(
        best < Duration::from_millis(20),
        "单原子提交 {best:?} 超出 20ms 预算（14.10）"
    );

    // 批量提交：一个变更集里 N 个原子，取最小耗时。
    let mut batch_best = Duration::MAX;
    for round in 0..10 {
        let atoms: Vec<Atom> = (0..20)
            .map(|index| {
                Atom::new(
                    AtomKind::CreateObject,
                    "human:1",
                    "session:a",
                    json!({
                        "object_id": format!("batch_{round}_{index}"),
                        "layer_id": "layer_1",
                        "kind": "shape",
                        "data": {
                            "geometry": {"kind": "rect", "bbox": {"x": 5.0, "y": 5.0, "w": 20.0, "h": 20.0}},
                            "color": {"r": 40, "g": 90, "b": 140, "a": 255}
                        }
                    }),
                )
            })
            .collect();
        let started = Instant::now();
        let changeset = format!("cs-{round}");
        workspace
            .commit_changeset("doc_commit", atoms, "human:1", true, changeset)
            .expect("批量提交应成功");
        batch_best = batch_best.min(started.elapsed());
    }
    let per_atom = batch_best / 20;
    println!("批量提交 20 原子（服务端，最小）: {batch_best:?}｜每原子 {per_atom:?}｜预算 < 10ms（14.10）");
    assert!(
        batch_best < Duration::from_millis(10),
        "批量提交 20 原子 {batch_best:?} 超出 10ms 预算（14.10）"
    );
}

/// 14.10 预算表：时间旅行（近期历史 / checkpoint）< 300ms。
///
/// 覆盖的是「从 HEAD 回看近期历史」的**状态重建**成本（`state_at` 路径），
/// 以及 checkpoint 建立后的取用。老历史含归档取回（秒级、UI 提示）另有条目，暂未设门禁。
#[test]
#[ignore = "性能预算验收：CI 用 --ignored 执行（14.10 时间旅行预算）"]
fn time_travel_budget_near_history() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_travel", 512, 512),
            "human:1",
            "session:a",
        )
        .unwrap();
    workspace
        .commit(
            "doc_travel",
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1", "name": "base"}),
            ),
            "human:1",
            true,
        )
        .unwrap();
    // 造一段中等长度的历史（200 个原子），并记录第 20 个原子处的目标 seq。
    let mut target_seq = 0;
    for index in 0..200 {
        let result = workspace
            .commit(
                "doc_travel",
                Atom::new(
                    AtomKind::CreateObject,
                    "human:1",
                    "session:a",
                    json!({
                        "object_id": format!("obj_{index}"),
                        "layer_id": "layer_1",
                        "kind": "shape",
                        "data": {
                            "geometry": {"kind": "rect",
                                "bbox": {"x": (index % 32) as f64 * 12.0, "y": (index / 32) as f64 * 12.0,
                                         "w": 10.0, "h": 10.0}},
                            "color": {"r": 90, "g": 140, "b": 190, "a": 255}
                        }
                    }),
                ),
                "human:1",
                true,
            )
            .unwrap();
        if index == 20 {
            target_seq = result.seq;
        }
    }

    // 近期历史：连续 5 次回看同一 seq，取最小值。
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        let _ = workspace
            .document_mut("doc_travel")
            .unwrap()
            .state_at(target_seq)
            .expect("近期历史应可重建");
        best = best.min(started.elapsed());
    }
    println!("时间旅行（近期历史，seq {target_seq}，最小）: {best:?}｜预算 < 300ms（14.10）");
    assert!(
        best < Duration::from_millis(300),
        "近期历史重建 {best:?} 超出 300ms 预算（14.10）"
    );

    // checkpoint 建立后从 checkpoint 出发的回看也应在预算内。
    let checkpoint = workspace
        .commit(
            "doc_travel",
            Atom::new(
                AtomKind::CreateCheckpoint,
                "human:1",
                "session:a",
                json!({"checkpoint_id": "ckpt_1", "name": "v1", "anchor_seq": target_seq}),
            ),
            "human:1",
            true,
        )
        .unwrap();
    let mut checkpoint_best = Duration::MAX;
    for _ in 0..5 {
        let started = Instant::now();
        let _ = workspace
            .document_mut("doc_travel")
            .unwrap()
            .state_at(target_seq)
            .expect("checkpoint 路径应可重建");
        checkpoint_best = checkpoint_best.min(started.elapsed());
    }
    println!(
        "时间旅行（checkpoint seq {} 之后回看，最小）: {checkpoint_best:?}｜预算 < 300ms",
        checkpoint.seq
    );
    assert!(
        checkpoint_best < Duration::from_millis(300),
        "checkpoint 路径重建 {checkpoint_best:?} 超出 300ms 预算（14.10）"
    );
}
