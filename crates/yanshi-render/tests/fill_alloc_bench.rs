// **★★★ 微基准：`Buffer::filled` 的两种构造 ✗ ★★★**（**第 659 轮 ✓）
//
// **∴ 对比什么 ✗**：
//   **∴ A（**现状 ✓）✗**：`vec![0.0; n]`（**清零 ✓）＋ 按块覆盖（**`fill` ✓）
//     ⇒ **∴ 那**一遍清零**是白做的**✗（**∴ 因为**紧接着全部覆盖 ✓）
//   **∴ B（**候选 ✓）✗**：**预分配容量 ＋ 直接**按块扩展**✗（**∴ 不先清零 ✓）
//
// **∴ 注意 ✗**：**`black_box` 必须**触碰数据**✗（**∴ 第 641／646 轮的教训 ✓）
//   ＋ **∴ 且**必须用**非零数据**✗（**∴ 否则** LLVM 可静态推断 ✓）★
//
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test fill_alloc_bench -- --nocapture`
use std::time::Instant;

fn fill_current(w: u32, h: u32, pixel: [f32; 4]) -> Vec<f32> {
    // **∴ A：先清零，再按块覆盖 ✓**（**照抄 `Buffer::new` ＋ `fill` ✓）
    let mut pixels = vec![0.0f32; (w * h * 4) as usize];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&pixel);
    }
    pixels
}

fn fill_no_clear(w: u32, h: u32, pixel: [f32; 4]) -> Vec<f32> {
    // **∴ B：预分配容量，直接按块扩展（**不清零 ✓）★
    let n = (w * h) as usize;
    let mut pixels: Vec<f32> = Vec::with_capacity(n * 4);
    let mut remaining = n;
    while remaining > 0 {
        pixels.extend_from_slice(&pixel);
        remaining -= 1;
    }
    pixels
}

#[test]
fn bench_fill_alloc() {
    let (w, h) = (3840u32, 2160u32);
    let pixel = [0.25f32, 0.5, 0.75, 1.0];
    let rounds = 5usize;
    let mut a_t = Vec::new();
    let mut b_t = Vec::new();
    for _ in 0..rounds {
        let t = Instant::now();
        let a = fill_current(w, h, pixel);
        std::hint::black_box(a.iter().fold(0.0f32, |s, v| s + *v));
        a_t.push(t.elapsed().as_secs_f64() * 1000.0);
        let t = Instant::now();
        let b = fill_no_clear(w, h, pixel);
        std::hint::black_box(b.iter().fold(0.0f32, |s, v| s + *v));
        b_t.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        v[v.len() / 2]
    };
    let am = med(&mut a_t);
    let bm = med(&mut b_t);
    println!("  ★ 4K｜A（**清零 ＋ 覆盖 ✓）中位 {am:.1} ms");
    println!("  ★ 4K｜B（**不清零直接填 ✓）中位 {bm:.1} ms");
    println!("  ⇒ 省 {:.1} ms（{:.1}%）", am - bm, (am - bm) / am * 100.0);
}
