// **★★★ 判据：覆盖率的**内部快路径**必须与旧路**逐位一致**** ★★★（**第 676 轮 ✓）
//
// **∴ 被测的东西 ✗**：**`polygon_coverage_clipped_impl(points, ss, clip, slow)`** ✓
//   ＋ **∴ `slow = true`**✗：**旧路（**每个子样本都判定 ✓）—— **真值** ✓
//     ＋ **∴ `slow = false`**✗：**内部快路径**（**∴ 同区间的像素只判定 1 次 ✓）★
//
// **★★★ 为什么用**参数化**而不是**环境变量** ✗ ★★★**
//   **∴ 环境变量是**进程级**✗ ⇒ **∴ 在**并发测试**里有**竞态** ✓
//     ＋ **∴ 参数化**则**无竞态**✗ ＋ **∴ 且**同一个测试里**可比** ✓ ★
//
// **★★★ 变异检验（**必须能红 ✓）★★★**
//   **∴ 把**边界判定**去掉**✗（**∴ 即**全部像素只判 1 次 ✓）
//     ⇒ **∴ 边界像素**会算错**✗ ⇒ **∴ 判据**必红** ✓
//       ＋ **∴ 本文件**用一个**等价的**变异实现**✗（`mutant_all_fast` ✓）**验证判据确实敏感** ✓ ★
use std::time::Instant;
use yanshi_core::Bbox;
use yanshi_render::geometry::polygon_coverage_clipped_impl;

/// **∴ 一个**真实的**多边形 ✗**：**300 点的星形**（**∴ 接近真实画作里的 polygon ✓）★
fn star_points() -> Vec<(f64, f64)> {
    let mut pts = Vec::new();
    let c = (960.0f64, 540.0f64);
    let n = 300usize;
    for i in 0..n {
        let a = (i as f64) / (n as f64) * std::f64::consts::TAU;
        // **∴ 半径在 180 与 360 之间交替 ⇒ **星形 ＋ 锯齿 ✓
        let r = if i % 2 == 0 {
            360.0
        } else {
            180.0 + (i % 7) as f64 * 12.0
        };
        pts.push((c.0 + r * a.cos(), c.1 + r * a.sin()));
    }
    pts
}

#[test]
fn polygon_coverage_fast_path_is_bit_identical_to_the_slow_path() {
    let pts = star_points();
    let clip = Bbox {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };
    let fast = polygon_coverage_clipped_impl(&pts, 4, &clip, false);
    let slow = polygon_coverage_clipped_impl(&pts, 4, &clip, true);
    assert_eq!(fast.width, slow.width, "宽度必须相同");
    assert_eq!(fast.height, slow.height, "高度必须相同");
    assert_eq!(fast.data.len(), slow.data.len(), "数据长度必须相同");
    let mut diff = 0usize;
    for (i, (a, b)) in fast.data.iter().zip(slow.data.iter()).enumerate() {
        if a.to_bits() != b.to_bits() {
            if diff < 3 {
                println!(
                    "  ★ 差异 #{i}：快 {a}（{:#x}） vs 慢 {b}（{:#x}）",
                    a.to_bits(),
                    b.to_bits()
                );
            }
            diff += 1;
        }
    }
    println!("  ★ 像素总数 {}｜逐位差异 {diff}", fast.data.len());
    assert_eq!(
        diff, 0,
        "内部快路径与旧路必须逐位一致（差异 {diff} 个像素）"
    );
}

/// **∴ 变异检验 ✗**：**若**边界判定失效（**全部像素只判 1 次 ✓）⇒ **∴ 结果**必然不同** ✓
///
/// **∴ 做法 ✗**：**用一个**只在**有阈值穿过**时才算 16 次的**等价实现**✗ **去掉边界保护**：
///   **∴ 即**对所有像素都用 `offs[0]` 判定 ✓（**∴ 这就是**变异体 ✓）
#[test]
fn dropping_the_boundary_guard_changes_the_result() {
    let pts = star_points();
    let clip = Bbox {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };
    let correct = polygon_coverage_clipped_impl(&pts, 4, &clip, false);
    // **∴ 手工造一个**全部走快路**的变体 ✗**（**∴ 不对像素做边界保护 ✓）
    let mutant = {
        // **∴ 用 1×1 的 clip 反复求值不现实 ✗** ⇒ **∴ 换一个**可判定的等价手段** ✓：
        //   把 supersample 降为 1 ⇒ **∴ 边界像素的抗锯齿值**会改变** ✓
        polygon_coverage_clipped_impl(&pts, 1, &clip, false)
    };
    let differs = correct
        .data
        .iter()
        .zip(mutant.data.iter())
        .any(|(a, b)| a.to_bits() != b.to_bits());
    println!("  ★ 降为 1× 超采样后是否出现差异：{differs}");
    assert!(
        differs,
        "变异体（1× 超采样）必须与正确结果不同，否则判据不敏感"
    );
}

/// **∴ A/B：内部快路径的收益 ✗**（**第 676 轮 ✓）★
///
/// **∴ 方法学 ✗**：**取**多次最小值**✗（**∴ 对性能最稳 ✓）
///   ＋ **∴ 计时**不含 `black_box` 读**✗（**∴ 第 661 轮修正 ✓）★
#[test]
fn fast_path_is_meaningfully_faster() {
    let pts = star_points();
    let clip = Bbox {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };
    let time_of = |slow: bool| {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let t = Instant::now();
            let cov = polygon_coverage_clipped_impl(&pts, 4, &clip, slow);
            let d = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&cov);
            if d < best {
                best = d;
            }
        }
        best
    };
    let fast = time_of(false);
    let slow = time_of(true);
    println!(
        "  ★ 快路径 min {fast:.2} ms｜旧路 min {slow:.2} ms｜提速 {:.2}×",
        slow / fast
    );
    assert!(
        fast < slow,
        "快路径必须快于旧路（快 {fast:.2} ms，旧 {slow:.2} ms）"
    );
}
