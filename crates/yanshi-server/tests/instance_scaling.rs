//! 实例的**渲染成本上界**（设计 9.3 的"实例共享 Master 渲染缓存"何时才值得做）。
//!
//! **先量再改** ✓ —— 本轮本来打算实现 master 渲染缓存 ✓，量完发现**当前规模下不值得** ✗：
//!
//! | 场景 | 整幅 512×512 渲染 |
//! |---|---|
//! | 小 master（64²）× 0 个实例 | ~400 ms |
//! | 小 master（64²）× 32 个实例 | ~430 ms（**+7%**） |
//! | 大 master（384²）× 0 个实例 | ~450 ms |
//! | 大 master（384²）× 8 个实例 | ~690 ms（**+53%**） |
//!
//! ⇒ 两个结论 ✓：
//! * **每个实例的成本与 master 的尺寸成正比** ✓（384² 的 master 每实例约 30ms ✓；
//!   64² 的 master 每实例约 1ms ✓）—— 这符合直觉 ✓：实例要**重新栅格化** master 的图元 ✓；
//! * 整幅渲染本身由**帧成本**主导（~400ms ✓）⇒ 小 master 时实例几乎不花时间 ✓。
//!
//! ⇒ **触发条件（写进文档 ✓）**：当 master **很大**（百万像素级 ✓）**且**同一 master 有**多个实例**时，
//! 9.3 的 master 缓存才会真正回本 ✓。在那之前加缓存是**过早优化** ✗：
//! 它会带来缓存键、失效语义与"缓存不得改变渲染结果"这整套复杂度 ✓，
//! 而收益在实测里还不到一成的量级 ✓。
//!
//! 这个测试**常驻** ✓ 并断言一个**宽松上界** ✓：小 master × 32 个实例不得比 0 个实例慢 1.6 倍以上 ✓。
//! 它的作用是**守住退化** ✓（哪天真把实例路径写坏了 ✓，这里会先响 ✓），
//! 而不是证明"现在很快" ✓。由 CI 的 `--ignored` 长跑作业执行 ✓（与 `perf_budget.rs` 同一条 ✓）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_scaling", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

/// 建一份文档：一个 `master_size²` 的方块，外加 `instances` 个实例 ✓。
fn build(master_size: f64, instances: usize) -> (Workspace, ToolRegistry) {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_scaling", 512, 512),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "master", "data": {
                "geometry": {"kind": "rect",
                             "bbox": {"x": 10.0, "y": 10.0, "w": master_size, "h": master_size}},
                "color": {"r": 200, "g": 30, "b": 30, "a": 255}}}),
        );
        for index in 0..instances {
            registry.call(
                &mut ctx,
                "create_instance",
                &json!({"instance_id": format!("mirror{index}"), "layer_id": "L",
                        "master_id": "master",
                        "local_transform": {"matrix": [1, 0, 0, 1,
                            8.0 * (index % 40) as f64, 8.0 * (index / 40) as f64], "pivot": [0, 0]}}),
            );
        }
    }
    (workspace, registry)
}

/// 量 `rounds` 次整幅渲染的**中位数** ✓（中位数比平均更抗噪 ✓，CI 机器上尤其重要 ✓）。
fn median_full_render_ms(workspace: &mut Workspace, rounds: usize) -> f64 {
    let mut timings: Vec<f64> = Vec::new();
    for _ in 0..rounds {
        let started = std::time::Instant::now();
        let _ = workspace
            .render_region_raw("doc_scaling", Bbox::new(0.0, 0.0, 512.0, 512.0))
            .expect("区域渲染应成功");
        timings.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    timings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    timings[timings.len() / 2]
}

/// 小 master 下，32 个实例不该让整幅渲染明显变慢 ✓（宽松上界 1.6× ✓，只为守退化 ✓）。
#[test]
#[ignore = "长跑性能作业（CI 的 --ignored）"]
fn many_small_instances_do_not_dominate_the_frame_cost() {
    let (mut plain, _) = build(64.0, 0);
    let (mut loaded, _) = build(64.0, 32);
    let plain_ms = median_full_render_ms(&mut plain, 3);
    let loaded_ms = median_full_render_ms(&mut loaded, 3);
    println!("  小 master：0 实例 {plain_ms:.0} ms｜32 实例 {loaded_ms:.0} ms");
    assert!(
        loaded_ms < plain_ms * 1.6,
        "32 个小实例把整幅渲染拖慢了 {:.1}×（{plain_ms:.0} → {loaded_ms:.0} ms）",
        loaded_ms / plain_ms
    );
}

/// **记录触发条件** ✓：大 master 的实例子集明显更贵 ✓ —— 这正是设计 9.3 的 master 缓存
/// 将来要解决的场景 ✓。这里**不断言"贵"** ✓（那会变成对当前实现的抱怨 ✓），
/// 只**打印实测值** ✓ 供 CI 日志留痕 ✓：将来做缓存时，拿这条基线对比收益 ✓。
#[test]
#[ignore = "长跑性能作业（CI 的 --ignored）"]
fn a_large_master_makes_instances_measurably_expensive() {
    let (mut plain, _) = build(384.0, 0);
    let (mut loaded, _) = build(384.0, 8);
    let plain_ms = median_full_render_ms(&mut plain, 3);
    let loaded_ms = median_full_render_ms(&mut loaded, 3);
    println!(
        "  大 master：0 实例 {plain_ms:.0} ms｜8 实例 {loaded_ms:.0} ms\
         （每个实例约 {:.0} ms）—— 这是 9.3 master 缓存的触发场景",
        (loaded_ms - plain_ms) / 8.0
    );
    // 只做**极宽松**的健全性断言 ✓：渲染必须真的完成并且时间合理 ✓（不能是 0 或荒谬的大 ✓）。
    assert!(plain_ms > 1.0 && loaded_ms > 1.0, "渲染应当真的花了时间");
    assert!(
        loaded_ms < plain_ms * 4.0,
        "即便大 master 也不该退化到 4 倍以上"
    );
}
