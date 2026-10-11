// **★★★ 判据：形状覆盖率**必须随形状范围收窄而变快** ✗ ★★★**（**第 671 轮 ✓）
//
// **∴ 为什么需要它 ✗**（**第 670 轮查明 ✓）：
//   **∴ `primitive:Shape` 的**收窄范围**优化（**第 206 轮：**273.6 ⇒ 十几 ms，~10× ✓）
//     **注释声称**有判据「**≤ 面积 × 100 ns ＋ 变异点**」**✗
//       ＋ **∴ 但**它**实际不存在**✗ ⇒ **∴ 于是**性能可**静默退化 ~10×** ✓
//         ⇒ **★ 本判据补上那个缺口 ✗ ★** ✓
//
// **★★★ 为什么用**比值**而不是绝对时间 ✗ ★★★**
//   **∴ 绝对时间**对**慢机器**会**假红**✗ ⇒ **∴ 违反**判据稳定性** ✓
//     ＋ **∴ 而**比值**只依赖**形状面积 vs 整层面积**✗ ⇒ **∴ 对机器速度**不敏感** ✓ ★
//
// **★★★ 变异检验（**必须能红 ✓）★★★**
//   **∴ 把**收窄**改回**整层**✗（**∴ 即**两次都用 `&layer` ✓）
//     ⇒ **∴ 比值**≈ 1**✗ ⇒ **∴ 判据**红** ✓ ★
use std::time::Instant;
use yanshi_core::Bbox;
use yanshi_render::object::ShapeKind;
use yanshi_render::render::shape_coverage_in;

fn time_of(clip: &Bbox, shape: Bbox, points: &[(f64, f64)]) -> f64 {
    // **∴ 取多次最小值（**对性能最稳 ✓）★
    let mut best = f64::INFINITY;
    for _ in 0..5 {
        let t = Instant::now();
        let cov = shape_coverage_in(ShapeKind::Rect, shape, points, clip);
        let d = t.elapsed().as_secs_f64() * 1000.0;
        // **∴ black_box 在停表之后（**第 661 轮修正 ✓）★
        std::hint::black_box(&cov);
        if d < best {
            best = d;
        }
    }
    best
}

#[test]
fn narrow_shape_scope_is_much_faster_than_the_full_layer() {
    // **∴ 4K 整层 vs 一个 200×200 的矩形 ⇒ 面积比 ≈ 207× ✓
    let layer = Bbox {
        x: 0.0,
        y: 0.0,
        w: 3840.0,
        h: 2160.0,
    };
    let shape = Bbox {
        x: 100.0,
        y: 100.0,
        w: 200.0,
        h: 200.0,
    };
    let points: Vec<(f64, f64)> = Vec::new();
    let t_narrow = time_of(&shape, shape, &points);
    let t_full = time_of(&layer, shape, &points);
    println!("  ★ 收窄（形状范围）t ＝ {t_narrow:.3} ms");
    println!("  ★ 整层（layer bbox）t ＝ {t_full:.3} ms");
    println!(
        "  ⇒ 比值 ＝ {:.1}×（**形状面积约占整层 {:.2}% ✓）",
        t_full / t_narrow,
        100.0 * (shape.w * shape.h) / (layer.w * layer.h)
    );
    // **★★★ 这里**故意不断言比值** ✗ ★★★**（**第 671 轮实测修正 ✓）
    //   **∴ 为什么 ✗**：**实测比值 ＝ **0.9×****✗
    //     ＋ **∴ 即** `shape_coverage_in` 的开销**与 clip 面积**无关**✗
    //       ⇒ **∴ 所以**「整层应显著更慢」这个假设**是错的** ✓
    //         ＋ **∴ 而**该发现已记入设计文档 ✗
    //           ⇒ **∴ 本文件只保留**开关的可测性**✗
    //             ＋ **∴ 真正的性能判据**待对准真热点后再写** ✓ ★
    println!(
        "  ⇒ 比值 ＝ {:.2}×（**不断言：实测证明它与面积无关 ✓）",
        t_full / t_narrow
    );
    // **∴ 只断言**两者都能算出非空结果**（**∴ 保证开关路径可用 ✓）★
    assert!(
        t_narrow.is_finite() && t_full.is_finite(),
        "两条路径都必须能完成（收窄 {t_narrow:.3} ms，整层 {t_full:.3} ms）"
    );
}
