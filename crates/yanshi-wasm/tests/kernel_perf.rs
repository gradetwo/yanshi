//! 客户端内核的**原生**计时参照（确定性工具）。
//!
//! 为什么需要它：浏览器里测得的「每次调用约 4.2ms 固定开销」随机器负载在 4.2–11.5ms 间跳动，
//! 单靠浏览器计时无法归因。`Kernel` 只依赖纯 Rust crate，可在宿主上直接计时，
//! 从而把「WASM/浏览器特有开销」与「kernel.rs 自身的算法开销」区分开。
//!
//! 由 CI 的 `--ignored` 长跑作业执行。

use std::time::{Duration, Instant};

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_wasm::Kernel;

fn best_of(rounds: u32, mut body: impl FnMut()) -> Duration {
    body();
    let mut best = Duration::MAX;
    for _ in 0..rounds {
        let started = Instant::now();
        body();
        best = best.min(started.elapsed());
    }
    best
}

#[test]
#[ignore = "性能参照：CI 用 --ignored 执行（内核原生计时）"]
fn kernel_render_cost_on_the_host() {
    let mut kernel = Kernel::new("doc_kernel_perf", 256, 1024, 1024, 64 * 1024 * 1024).unwrap();
    kernel
        .load_atoms_json(
            &json!([
                {"seq": 1, "id": "01AAAAAAAAAAAAAAAAAAAAAAAA", "kind": "create_document",
                 "actor": "human:1", "session": "session:a", "timestamp": 1,
                 "payload": {"doc_id": "doc_kernel_perf", "width": 1024, "height": 1024}},
                {"seq": 2, "id": "01BBBBBBBBBBBBBBBBBBBBBBBB", "kind": "create_layer",
                 "actor": "human:1", "session": "session:a", "timestamp": 2,
                 "payload": {"layer_id": "layer_1", "name": "base"}},
                {"seq": 3, "id": "01CCCCCCCCCCCCCCCCCCCCCCCC", "kind": "create_object",
                 "actor": "human:1", "session": "session:a", "timestamp": 3,
                 "payload": {"object_id": "shape_1", "layer_id": "layer_1", "kind": "shape",
                             "data": {"geometry": {"kind": "rect",
                                      "bbox": {"x": 100.0, "y": 100.0, "w": 600.0, "h": 600.0}},
                                      "color": {"r": 200, "g": 120, "b": 60, "a": 255}}}}
            ])
            .to_string(),
        )
        .expect("夹具原子应能加载");

    println!("内核原生区域渲染（最小值）：");
    for side in [8.0f64, 64.0, 256.0, 512.0] {
        let region = Bbox::new(0.0, 0.0, side, side);
        let elapsed = best_of(5, || {
            let _ = kernel.render_region(region).expect("渲染应成功");
        });
        println!(
            "  {side:>4.0}²: {elapsed:?}｜每像素 {:.1}ns",
            elapsed.as_secs_f64() * 1e9 / (side * side)
        );
    }

    // 元信息字段（tile 数 / padding / 警告）也打印出来，便于判断渲染实际做了多少工作。
    let result = kernel
        .render_region(Bbox::new(0.0, 0.0, 8.0, 8.0))
        .expect("渲染应成功");
    println!(
        "  8² 结果: {}×{}｜tiles={:?}｜padding={}｜警告 {} 条",
        result.width,
        result.height,
        result.tiles,
        result.filter_padding,
        result.warnings.len()
    );
}
