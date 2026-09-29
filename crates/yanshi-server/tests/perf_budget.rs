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
