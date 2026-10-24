//! 客户端内核的**原生**计时参照（确定性工具）。
//!
//! 为什么需要它：浏览器里测得的「每次调用约 4.2ms 固定开销」随机器负载在 4.2–11.5ms 间跳动，
//! 单靠浏览器计时无法归因。`Kernel` 只依赖纯 Rust crate，可在宿主上直接计时，
//! 从而把「WASM/浏览器特有开销」与「kernel.rs 自身的算法开销」区分开。
//!
//! 由 CI 的 `--ignored` 长跑作业执行。

use std::time::{Duration, Instant};

use serde_json::{json, Value};
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

    // 2×2 二分：有/无背景 × 有/无对象，定位「带背景慢 4×」到底来自背景填充还是对象混合。
    for (label, has_background, has_shape) in [
        ("无背景·无对象", false, false),
        ("无背景·有对象", false, true),
        ("有背景·无对象", true, false),
        ("有背景·有对象", true, true),
    ] {
        let mut atoms = vec![
            json!({"seq": 1, "id": "01AAAAAAAAAAAAAAAAAAAAAAAA", "kind": "create_document",
                   "actor": "human:1", "session": "session:a", "timestamp": 1,
                   "payload": {"doc_id": "doc_bisect", "width": 1024, "height": 1024}}),
            json!({"seq": 2, "id": "01BBBBBBBBBBBBBBBBBBBBBBBB", "kind": "create_layer",
                   "actor": "human:1", "session": "session:a", "timestamp": 2,
                   "payload": {"layer_id": "layer_1", "name": "base"}}),
        ];
        if has_background {
            atoms[0]["payload"]["background"] = json!({"r": 255, "g": 255, "b": 255, "a": 255});
        }
        if has_shape {
            atoms.push(json!({"seq": 3, "id": "01CCCCCCCCCCCCCCCCCCCCCCCC",
                "kind": "create_object", "actor": "human:1", "session": "session:a",
                "timestamp": 3,
                "payload": {"object_id": "shape_1", "layer_id": "layer_1", "kind": "shape",
                            "data": {"geometry": {"kind": "rect",
                                     "bbox": {"x": 100.0, "y": 100.0, "w": 600.0, "h": 600.0}},
                                     "color": {"r": 200, "g": 120, "b": 60, "a": 255}}}}));
        }
        let mut kernel = Kernel::new("doc_bisect", 256, 1024, 1024, 64 * 1024 * 1024).unwrap();
        kernel
            .load_atoms_json(&Value::Array(atoms).to_string())
            .expect("夹具应能加载");
        let region = Bbox::new(0.0, 0.0, 512.0, 512.0);
        let elapsed = best_of(5, || {
            let _ = kernel.render_region(region).expect("渲染应成功");
        });
        println!(
            "  512² {label}: {elapsed:?}｜每像素 {:.1}ns",
            elapsed.as_secs_f64() * 1e9 / (512.0 * 512.0)
        );
    }

    // 量化器本身的两条分支对比（决定「有背景慢 4×」是否发生在量化阶段）。
    {
        let mut buffer = yanshi_render::Buffer::new(0, 0, 512, 512);
        for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
            let value = (index % 97) as f32 / 97.0;
            pixel.copy_from_slice(&[value, value * 0.5, value * 0.25, 0.75]);
        }
        let background = [255u8, 255, 255, 255];
        let none_ms = best_of(5, || {
            std::hint::black_box(buffer.to_rgba8(None));
        });
        let some_ms = best_of(5, || {
            std::hint::black_box(buffer.to_rgba8(Some(background)));
        });
        println!(
            "  量化器 512²：无背景 {:?}（{:.1}ns/px）｜有背景 {:?}（{:.1}ns/px）｜倍数 {:.2}×",
            none_ms,
            none_ms.as_secs_f64() * 1e9 / 262144.0,
            some_ms,
            some_ms.as_secs_f64() * 1e9 / 262144.0,
            some_ms.as_secs_f64() / none_ms.as_secs_f64()
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

/// **`apply` 的每 atom 耗时**必须与日志长度**近似无关**（第 792 轮；外部 `art` 第 120 次）
///
/// **⇒ 他们的实测**（`yanshi-timelapse` 侧）：每 atom 从 **33 ms → 209 → 299 ms** ✓，
///   `test3` 直接**卡死**（3 轮停在 `12200/23472` ✓）。
///
/// **⇒ 我在代码里定到的原因**（`kernel.rs` 的 `apply_atom_json`）：
///   每吃一个 atom 都 `self.folder.fold(&self.log, self.log.head_seq())` ✓
///   而 `fold` 的第一步是 `compute_suppressed(atoms)` ＝ **扫全部原子** ✓
///   ⇒ `apply` × 全日志扫描 ＝ **O(n²)** ✓。
///
/// **⇒ 本判据要钉的性质**：**`apply` 的单次耗时不随 n 线性增长** ✓。
///   做法：在同一台机器上量 n≈100 与 n≈800 的**每 atom 中位耗时** ✓，断言比值 **< 4** ✓
///   （线性会给出 ~8 ✓，二次会给出 ~60+ ✓；留足机器抖动余量 ✓）。
///   **⇒ 为什么是比值不是毫秒**：绝对毫秒跨机器不可比 ✓，而**比值**是算法性质 ✓
///   （与"别断像素数、要断相对关系"同一条规矩 ✓）。
///
/// **⇒ 变异**：把 `fold` 换回"每 atom 折全日志"（现状即如此 ⇒ 本判据**今天应当红** ✓）
///   ⇒ 修好之后转绿 ✓ —— 这就是"先红后绿"的顺序 ✓。
#[test]
#[ignore = "性能参照：CI 用 --ignored 执行（内核原生计时）"]
fn apply_cost_does_not_grow_with_log_length() {
    fn per_atom_median(count: usize) -> Duration {
        let mut kernel = Kernel::new("doc_apply_scale", 64, 256, 256, 16 * 1024 * 1024).unwrap();
        let mut samples = Vec::with_capacity(count);
        for index in 0..count {
            let atom = json!({
                "id": format!("a{index:06}"),
                "seq": index as u64 + 2,
                "kind": "draw_stroke",
                "actor": "human:bench",
                "session": "bench",
                "timestamp": 0,
                "schema_version": 1,
                "payload": {
                    "layer_id": "layer_default",
                    "object_id": format!("o{index:06}"),
                    "data": {
                        "points": [[4.0 + (index % 32) as f64, 4.0 + (index % 16) as f64],
                                   [20.0, 20.0]],
                        "size": 2.0, "hardness": 1.0,
                        "color": {"r": 10, "g": 20, "b": 30, "a": 255}
                    }
                }
            });
            let text = atom.to_string();
            let started = Instant::now();
            let _ = kernel.apply_atom_json(&text);
            samples.push(started.elapsed());
        }
        samples.sort_unstable();
        samples[samples.len() / 2]
    }

    let small = per_atom_median(100);
    let large = per_atom_median(800);
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(f64::EPSILON);
    eprintln!(
        "apply 每 atom 中位：n=100 ⇒ {small:?}；n=800 ⇒ {large:?}；比值 {ratio:.2}（线性会 ~8，二次会 ~60）"
    );
    assert!(
        ratio < 4.0,
        "`apply` 的单次耗时随日志长度增长（n=100 {small:?} ⇒ n=800 {large:?}，比值 {ratio:.2}）\
         ⇒ 这是 O(n²)：`apply_atom_json` 每 atom 都 `fold` 整份日志，而 `fold` 会 `compute_suppressed` 扫全部原子"
    );
}
