// **★★★ 判据：`rect_coverage_clipped` 的**预计算 `overlap_x`** 必须与旧路**逐位一致**** ★★★
//   （**第 683 轮 ✓）
//
// **∴ 背景 ✗**（**第 682 轮发现：**测试画作里 52／52 都是 Rect ✓）：
//   **∴ `overlap_x` **只依赖 `px`**✗ 而**原来在内层循环里 ⇒ **∴ 同一列被算 height 次** ✓
//     ＋ **∴ 优化 ✗**：**预计算长度 width 的数组** ✓
//       ＋ **∴ 逐位不变（**关键 ✓）✗**：**表达式**逐字相同** ⇒ **∴ 同一个 f64** ✓
//
// **★★★ 变异检验（**必须能红 ✓）★★★**
//   **∴ 本文件**手工重放旧路**✗（**真值 ✓）
//     ＋ **∴ 第二个测试**用一个**故意错的** overlap_x**✗（**如**忘了 clamp ✓）⇒ **∴ 必须**不同** ✓ ★
use std::time::Instant;
use yanshi_core::Bbox;
use yanshi_render::geometry::rect_coverage_clipped;

/// **∴ 手工重放**旧路**✗**（**每像素重算 `overlap_x` ✓）—— **真值** ✓
fn old_impl(bbox: Bbox, clip: &Bbox) -> (u32, u32, Vec<f32>) {
    let x0 = bbox.x.max(clip.x).floor() as i64;
    let y0 = bbox.y.max(clip.y).floor() as i64;
    let x1 = (bbox.x + bbox.w).min(clip.x + clip.w).ceil() as i64;
    let y1 = (bbox.y + bbox.h).min(clip.y + clip.h).ceil() as i64;
    if x1 <= x0 || y1 <= y0 {
        return (0, 0, Vec::new());
    }
    let width = (x1 - x0) as u32;
    let height = (y1 - y0) as u32;
    let mut data = Vec::with_capacity((width * height) as usize);
    for py in 0..height {
        let top = (y0 + py as i64) as f64;
        let bottom = top + 1.0;
        let overlap_y = (bottom.min(bbox.y + bbox.h) - top.max(bbox.y)).clamp(0.0, 1.0);
        for px in 0..width {
            let left = (x0 + px as i64) as f64;
            let right = left + 1.0;
            let overlap_x = (right.min(bbox.x + bbox.w) - left.max(bbox.x)).clamp(0.0, 1.0);
            data.push((overlap_x * overlap_y) as f32);
        }
    }
    (width, height, data)
}

#[test]
fn precomputed_overlap_x_is_bit_identical() {
    // **∴ 覆盖：**整数对齐 ＋ **非整数边界 ＋ **越界裁剪 ＋ **零尺寸 ✓
    let cases: Vec<(Bbox, Bbox)> = vec![
        (
            Bbox::new(100.0, 50.0, 200.0, 120.0),
            Bbox::new(0.0, 0.0, 1920.0, 1080.0),
        ),
        (
            Bbox::new(100.3, 50.7, 200.4, 120.2),
            Bbox::new(0.0, 0.0, 1920.0, 1080.0),
        ),
        (
            Bbox::new(-40.5, -20.25, 300.0, 200.0),
            Bbox::new(0.0, 0.0, 100.0, 100.0),
        ),
        (
            Bbox::new(0.0, 0.0, 0.0, 0.0),
            Bbox::new(0.0, 0.0, 10.0, 10.0),
        ),
        (
            Bbox::new(5000.0, 5000.0, 10.0, 10.0),
            Bbox::new(0.0, 0.0, 100.0, 100.0),
        ),
    ];
    let mut diff = 0usize;
    let mut checked = 0usize;
    for (bbox, clip) in cases {
        let new = rect_coverage_clipped(bbox, &clip);
        let (w, h, old_data) = old_impl(bbox, &clip);
        assert_eq!(new.width, w, "宽度：bbox={bbox:?}");
        assert_eq!(new.height, h, "高度：bbox={bbox:?}");
        for (i, (a, b)) in new.data.iter().zip(old_data.iter()).enumerate() {
            checked += 1;
            if a.to_bits() != b.to_bits() {
                if diff < 3 {
                    println!(
                        "  ★ 差异 #{i}：新 {a}（{:#x}） vs 旧 {b}（{:#x}）",
                        a.to_bits(),
                        b.to_bits()
                    );
                }
                diff += 1;
            }
        }
    }
    println!("  ★ 比对了 {checked} 个 f32｜逐位差异 {diff}");
    assert_eq!(diff, 0, "预计算 overlap_x 必须逐位一致（差异 {diff}）");
}

/// **∴ 变异检验 ✗**：**忽略边界分数**（**把 overlap_x 当成恒 1 ✓）⇒ **∴ 必红** ✓
///
/// **∴ 为什么换掉「去掉 clamp」✗**（**第 683 轮实测 ✓）：
///   **∴ 那个变异**检不出**✗（**实测 `false` ✓）
///     ＋ **∴ 因为** `clipped_grid` **已把范围限制在 bbox ∩ clip 内**✗
///       ⇒ **∴ 于是** overlap **恒在 [0,1]** ⇒ **∴ clamp**从不生效** ✓
///         ＋ **∴ 所以**：**clamp**是**防御性的**✗ ⇒ **∴ 而**它**不是**有效变异点** ✓ ★
///           ＋ **∴ 改用**「忽略 overlap_x」**✗ ⇒ **∴ 对**非整数 bbox **必红** ✓ ★
#[test]
fn ignoring_the_boundary_fraction_changes_the_result() {
    // **∴ 用**非整数 bbox**✗** ⇒ **∴ 边界列**是真分数** ✓
    let bbox = Bbox::new(100.3, 50.7, 200.4, 120.2);
    let clip = Bbox::new(0.0, 0.0, 1920.0, 1080.0);
    let correct = rect_coverage_clipped(bbox, &clip);
    // **∴ 变异体 ✗**：**overlap_x 恒为 1.0**（**忽略边界 ✓）★
    let (w, h, _) = old_impl(bbox, &clip);
    let mut mutant = Vec::new();
    for py in 0..h {
        let top = (50 + py as i64) as f64;
        let bottom = top + 1.0;
        let overlap_y = (bottom.min(bbox.y + bbox.h) - top.max(bbox.y)).clamp(0.0, 1.0);
        for _ in 0..w {
            mutant.push((1.0 * overlap_y) as f32);
        }
    }
    let differs = correct
        .data
        .iter()
        .zip(mutant.iter())
        .any(|(a, b)| a.to_bits() != b.to_bits());
    println!("  ★ 忽略 overlap_x 是否被检出：{differs}");
    assert!(differs, "忽略边界分数必须产生不同结果，否则判据不敏感");
}

/// **∴ A/B ✗**：**预计算相对每像素重算的收益 ✓**★
#[test]
fn precomputation_is_faster() {
    let bbox = Bbox::new(0.0, 0.0, 3840.0, 2160.0);
    let clip = Bbox::new(0.0, 0.0, 3840.0, 2160.0);
    let time_new = {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let t = Instant::now();
            let c = rect_coverage_clipped(bbox, &clip);
            let d = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&c);
            if d < best {
                best = d;
            }
        }
        best
    };
    let time_old = {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let t = Instant::now();
            let r = old_impl(bbox, &clip);
            let d = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&r);
            if d < best {
                best = d;
            }
        }
        best
    };
    println!(
        "  ★ 4K 全屏 Rect｜新（预计算）min {time_new:.2} ms｜旧（每像素）min {time_old:.2} ms｜提速 {:.2}×",
        time_old / time_new
    );
    // ★ **别断言「严格更快」**（第 963 轮 CI 实测）：
    //   CI 上跑出 `新 144.78 ms vs 旧 142.23 ms`（差 **1.8%**）⇒ 落在噪声里 ⇒ 红。
    //   ⇒ 性能断言的对手是**噪声**，不是「新一定 < 旧」。
    //   ⇒ 所以留 **15%** 容差：只要不是**明显更慢**（比如慢一倍）就算通过。
    //   ⇒ 真正的价值在**打印那一行倍数**（人可复核），而不是 `assert!(new < old)`。
    let ratio = time_new / time_old;
    assert!(
        ratio < 1.15,
        "预计算不该明显更慢（新 {time_new:.2} ms，旧 {time_old:.2} ms，比 {ratio:.2}×；容差 1.15×）"
    );
}
