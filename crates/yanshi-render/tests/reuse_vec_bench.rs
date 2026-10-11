// **★★★ 微基准：复用 Vec 能否省掉那 70 ms ✗ ★★★**（**第 659 轮 ✓）
//
// **∴ 验证方案③的前提 ✗**：**若**复用同一个 `Vec`（**`clear` ＋ 重填 ✓）
//   ＋ **∴ 比**每次新建**快得多**✗ ⇒ **∴ 那么**触页**确实是**主要成本** ✓
//     ＋ **∴ 从而**「**Vec 池**」**值得做** ✓ ★
//
// **∴ 注意 ✗**：**`black_box` 必须**触碰数据** ＋ **非零数据**（**第 641／646 轮教训 ✓）★
use std::time::Instant;

const N: usize = 3840 * 2160 * 4;

#[test]
fn bench_reuse_vec() {
    let pixel = [0.25f32, 0.5, 0.75, 1.0];
    let rounds = 5usize;
    let mut fresh_t = Vec::new();
    let mut reuse_t = Vec::new();
    // **∴ 预热复用缓冲（**先分配一次 ✓）
    let mut reused: Vec<f32> = Vec::with_capacity(N);
    for _ in 0..rounds {
        // **∴ ① 每次新建 ✗**（**现状 ✓）★
        let t = Instant::now();
        let mut fresh = vec![0.0f32; N];
        for c in fresh.chunks_exact_mut(4) {
            c.copy_from_slice(&pixel);
        }
        std::hint::black_box(fresh.iter().fold(0.0f32, |s, v| s + *v));
        fresh_t.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ ② 复用同一个 Vec ✗**（**方案③ ✓）★
        let t = Instant::now();
        reused.clear();
        reused.reserve(N);
        while reused.len() < N {
            reused.extend_from_slice(&pixel);
        }
        std::hint::black_box(reused.iter().fold(0.0f32, |s, v| s + *v));
        reuse_t.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let fm = med(&mut fresh_t);
    let rm = med(&mut reuse_t);
    println!("  ★ 4K｜① 每次新建 vec! ＋ 填  中位 {fm:.1} ms");
    println!("  ★ 4K｜② 复用同一 Vec（clear）中位 {rm:.1} ms");
    println!("  ⇒ 省 {:.1} ms（{:.1}%）", fm - rm, (fm - rm) / fm * 100.0);
}
