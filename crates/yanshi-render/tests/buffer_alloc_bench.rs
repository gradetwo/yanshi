// **★★★ 微基准：4K 缓冲的分配／触页成本 ✗ ★★★**（**第 646 轮 ✓）
//
// **∴ 为什么量它 ✗**（**第 520 / 636 轮的账 ✓）：
//   **∴ 探针的 5 个分段里 `fill` 表示**铺底/分配**✗
//     ＋ **∴ 而** `render.rs:1257` 的 `Buffer::new`／`Buffer::filled` **在 4K 上是**：
//       **∴ 一次 132.7 MB 的 `Vec<f32>` 分配 ＋ `vec![0.0; n]` 的清零（**＝ **触页 ✓）
//         ＋ **∴ 而**那**是**每一步都要付的**✗
//           ⇒ **∴ 所以**：**先量清楚它占多少** ✓
//
// **∴ 4K 规模 ✗**：**3840×2160×4 ＝ 33.2 M 个 f32 ＝ 132.7 MB** ✓
// **∴ 不是判据 ✗**：**不断言时间** ✓
//
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test buffer_alloc_bench -- --nocapture`
use std::time::Instant;
use yanshi_render::buffer::Buffer;

#[test]
fn bench_buffer_alloc_4k() {
    let (w, h) = (3840u32, 2160u32);
    let rounds = 5usize;
    let mut new_times = Vec::with_capacity(rounds);
    let mut filled_times = Vec::with_capacity(rounds);
    let mut clone_times = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        // **∴ ① `Buffer::new` ✗**：**分配 ＋ 清零 ✓
        let t = Instant::now();
        let mut b = Buffer::new(0, 0, w, h);
        // **★★★ 必须**触碰像素**，不能只碰元数据 ✗ ★★★**（**第 646 轮，同一个坑的第二次 ✓）
        std::hint::black_box(b.pixels_mut().iter().fold(0.0f32, |a, v| a + *v));
        new_times.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ ② `Buffer::filled` ✗**：**分配 ＋ 填一个具体像素 ✓
        let t = Instant::now();
        let mut f = Buffer::filled(0, 0, w, h, [0.25, 0.5, 0.75, 1.0]);
        std::hint::black_box(f.pixels_mut().iter().fold(0.0f32, |a, v| a + *v));
        filled_times.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ ③ `clone` ✗**：**复制已有缓冲（**132.7 MB memcpy ✓）★
        let src = Buffer::new(0, 0, w, h);
        let t = Instant::now();
        let mut c = src.clone();
        std::hint::black_box(c.pixels_mut().iter().fold(0.0f32, |a, v| a + *v));
        clone_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let m_new = med(&mut new_times);
    let m_filled = med(&mut filled_times);
    let m_clone = med(&mut clone_times);
    println!("  ★ 4K｜Buffer::new（**分配 ＋ 清零 ✓）  中位 {m_new:.1} ms");
    println!("  ★ 4K｜Buffer::filled（**分配 ＋ 填 ✓）   中位 {m_filled:.1} ms");
    println!("  ★ 4K｜clone（**132.7 MB memcpy ✓）    中位 {m_clone:.1} ms");
    println!(
        "  ⇒ 132.7 MB 的等效带宽：分配 {:.1} GB/s｜memcpy {:.1} GB/s",
        132.7 / m_new * 1000.0 / 1024.0,
        2.0 * 132.7 / m_clone * 1000.0 / 1024.0
    );
}
