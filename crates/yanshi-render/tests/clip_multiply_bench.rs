// **★★★ 微基准：`multiply_alpha_by`（**clipping 那一遍 ✓）✗ ★★★**（**第 641 轮 ✓）
//
// **∴ 为什么量它 ✗**（**第 636 轮的账 ✓）：
//   **∴ 渲染路径每层原来有**两遍整层乘法**✗**：
//     **∴ ① `multiply_alpha_by(&accumulation)`** —— **clipping 时**（**本基准的对象 ✓）
//       ＋ **∴ ② `multiply_alpha(opacity)`** —— **每层都有**（**∴ 第 639／640 轮**已省掉 ✓）
//         ⇒ **∴ 所以**：**① 是**仅剩的一遍**✗
//           ＋ **∴ 而**它**能否也省掉**取决于**两个数字**✗：
//             **∴ a.** 它**本身多贵**（**本基准量 ✓）
//               ＋ **∴ b.** 融合它的**替代方案**（**先取 alpha 到紧凑数组 ⇒ 再融合 ✓）**多贵** ✓
//
// **∴ 4K 规模 ✗**：**3840×2160×4 ＝ 33.2 M 个 f32（132.7 MB ✓）★
// **∴ 不是判据 ✗**：**不断言时间** ✓ ★
//
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test clip_multiply_bench -- --nocapture`
use std::time::Instant;
use yanshi_render::buffer::Buffer;

fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

#[test]
fn bench_clip_multiply() {
    let (w, h) = (3840u32, 2160u32);
    let rounds = 5usize;
    let mut by_times = Vec::with_capacity(rounds);
    let mut plain_times = Vec::with_capacity(rounds);
    let mut alpha_only_times = Vec::with_capacity(rounds);
    for r in 0..rounds {
        let mut layer = Buffer::new(0, 0, w, h);
        fill(&mut layer, 0x1234_5678 ^ (r as u32));
        let mut mask = Buffer::new(0, 0, w, h);
        fill(&mut mask, 0x0bad_f00d);
        // **∴ ① `multiply_alpha_by`（**clipping ✓）★
        let mut a = layer.clone();
        let t = Instant::now();
        a.multiply_alpha_by(&mask);
        std::hint::black_box(a.pixels_mut()[0]);
        by_times.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ ② `multiply_alpha`（**普通，**无 mask 读取 ✓）★
        let mut b = layer.clone();
        let t = Instant::now();
        b.multiply_alpha(0.5);
        std::hint::black_box(b.pixels_mut()[0]);
        plain_times.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ ③ 只取 alpha 到紧凑数组（**替代方案的上半 ✓）★
        let t = Instant::now();
        let alphas: Vec<f32> = mask.pixels_mut().chunks_exact(4).map(|p| p[3]).collect();
        // **★★★ 必须**强制物化** ✗ ★★★**（**第 641 轮自我修正 ✓）
        //   **∴ 原来 ✗**：`black_box(alphas.len())` **不需要真的构建 Vec**✗
        //     ＋ **∴ 于是** LLVM **把整个 `collect` 消除了**✗ ⇒ **∴ 报出 0.0 ms 的假数字** ✓
        //       ＋ **∴ 现在**：**对**内容求和**✗（**∴ 必须真的读一遍 ✓）★
        std::hint::black_box(alphas.iter().fold(0.0f32, |acc, v| acc + *v));
        alpha_only_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let m_by = med(&mut by_times);
    let m_plain = med(&mut plain_times);
    let m_alpha = med(&mut alpha_only_times);
    println!("  ★ 4K｜multiply_alpha_by（**clipping ✓）中位 {m_by:.1} ms");
    println!("  ★ 4K｜multiply_alpha（**无 mask ✓）    中位 {m_plain:.1} ms");
    println!("  ★ 4K｜只取 alpha 到紧凑数组           中位 {m_alpha:.1} ms");
    println!(
        "  ⇒ mask 读取的额外代价 ≈ {:.1} ms｜若 5 层都 clip ⇒ 约 {:.0} ms",
        m_by - m_plain,
        (m_by - m_plain) * 5.0
    );
}
