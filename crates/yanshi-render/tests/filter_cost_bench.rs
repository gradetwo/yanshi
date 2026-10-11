// **★★★ 函数级基准：滤镜的真实成本 ✗ ★★★**（**第 696 轮 ✓）
//
// **∴ 为什么量它 ✗**（**用户第 82 轮的方向 ✓）：
//   **∴ 用户指出 ✗**：**GPU 确定能收益的地方是「**计算高度并行 ＋ **不与 CPU 频繁交换**」** ✓
//     ＋ **∴ 而**滤镜（**模糊／锐化 ✓）正是这一类**✗
//       ＋ **∴ 所以**：**先量**它在**真实图层尺寸**上的成本** ✓ ★
//
// **∴ 结构 ✗**：
//   **∴ `box_blur`／`gaussian_blur` **可分离****✗ ⇒ **∴ 每像素 2×(2r+1) 次乘加 ＋ **无跨像素依赖** ✓
//     ＋ **∴ `sharpen` **先 `buffer.clone()`**✗ ⇒ **∴ 一次**整层拷贝**（**4K ⇒ 132.7 MB ✓）★
//
// **∴ 方法学 ✗**：**取多次最小值**✗ ＋ **∴ `black_box` 在**停表之后****✗（**∴ 第 661 轮修正 ✓）★
use std::time::Instant;
use yanshi_render::buffer::Buffer;
use yanshi_render::filter::{box_blur, gaussian_blur, sharpen};

fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

fn bench<F: FnMut(&mut Buffer)>(label: &str, w: u32, h: u32, rounds: usize, mut f: F) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..rounds {
        let mut buf = Buffer::new(0, 0, w, h);
        fill(&mut buf, 0xF11A_E12);
        let t = Instant::now();
        f(&mut buf);
        let d = t.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(&buf);
        if d < best {
            best = d;
        }
    }
    println!("  ★ {label:38} min {best:8.1} ms");
    best
}

#[test]
fn filter_costs() {
    for (size_label, w, h) in [("1080p", 1920u32, 1080u32), ("4K", 3840, 2160)] {
        println!(
            "  ── {size_label}（{w}x{h} ＝ {:.1} M 像素）──",
            (w as f64) * (h as f64) / 1e6
        );
        bench("gaussian_blur sigma=2", w, h, 3, |b| gaussian_blur(b, 2.0));
        bench("gaussian_blur sigma=8", w, h, 3, |b| gaussian_blur(b, 8.0));
        bench("gaussian_blur sigma=16", w, h, 3, |b| {
            gaussian_blur(b, 16.0)
        });
        bench("box_blur radius=4 passes=1", w, h, 3, |b| box_blur(b, 4, 1));
        bench("box_blur radius=16 passes=3", w, h, 3, |b| {
            box_blur(b, 16, 3)
        });
        bench("sharpen amount=1 radius=2", w, h, 3, |b| sharpen(b, 1.0, 2));
    }
}
