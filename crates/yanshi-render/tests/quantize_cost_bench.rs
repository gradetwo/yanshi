// **★★★ 微基准：量化的真实成本 ✗ ★★★**（**第 662 轮 ✓）
//
// **∴ 为什么量它 ✗**：
//   **∴ 第 520 轮 ✗**：**4K 量化 ＝ **472 ms 级别**✗（**∴ 那时是 **CPU 路** ✓）
//     ＋ **∴ 而现在**有 GPU 路**✗（**`should_use_gpu_on(device_class, count)` ✓）
//       ＋ **∴ 4K（**8 294 400 像素 ✓）**远超**阈值**✗ ⇒ **∴ 所以**必走 GPU** ✓
//         ⇒ **∴ 真实成本**可能已大降**✗ ⇒ **∴ 必须**量** ✓ ★
//
// **∴ 方法学（**第 661 轮的修正 ✓）✗**：
//   **∴ `black_box` **在停表之后****✗（**∴ 既**防优化**又**不污染** ✓）★
//
// **∴ 门控 ✗**：**需要 `gpu` feature**✗（**∴ 否则**没有 GPU 路可测 ✓）★
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test quantize_cost_bench --features gpu -- --nocapture`
#![cfg(feature = "gpu")]
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
fn bench_quantize_cost() {
    for (label, w, h) in [
        ("4K", 3840u32, 2160u32),
        ("2K", 1920, 1080),
        ("1K", 960, 540),
    ] {
        let mut buf = Buffer::new(0, 0, w, h);
        fill(&mut buf, 0x1234_5678);
        let rounds = 5usize;
        let mut t = Vec::new();
        for _ in 0..rounds {
            let start = Instant::now();
            let out = buf.to_rgba8_quantized(Some([255, 255, 255, 255]));
            t.push(start.elapsed().as_secs_f64() * 1000.0);
            // **∴ 停表之后**才触碰数据 ✓（**第 661 轮修正 ✓）★
            std::hint::black_box(out.len());
            std::hint::black_box(out.iter().fold(0u32, |s, v| s.wrapping_add(*v as u32)));
        }
        t.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = t[t.len() / 2];
        let px = (w as f64) * (h as f64);
        println!(
            "  ★ {label}（{w}x{h} ＝ {:.2} M 像素）｜量化中位 {med:.1} ms｜min {:.1}｜max {:.1}｜{:.1} ns/像素",
            px / 1e6,
            t[0],
            t[t.len() - 1],
            med * 1e6 / px
        );
    }
}
