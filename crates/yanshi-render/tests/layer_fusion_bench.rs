// **★★★ 微基准：图层「不透明度 ＋ 合成」的 A／B ✗ ★★★**（**第 640 轮 ✓）
//
// **∴ 对比什么 ✗**：
//   **∴ A（**旧路径 ✓）✗**：`layer.multiply_alpha(opacity)` ＋ `acc.composite(&layer, mode, 1.0)`
//     ＋ **∴ 但** A **会**原地改 layer**✗ ⇒ **∴ 每轮要**克隆一份**（**∴ 那**是旧路径的**真实代价 ✓）
//   **∴ B（**新路径 ✓）✗**：`acc.composite_with_layer(&layer, mode, opacity)`（**不修改 layer ✓）
//     ⇒ **∴ 节省 **一遍 132.7 MB 的整层读写** ✓ ★
//
// **∴ 为什么同进程交替测 ✗**：**消除**版本间／机器间的差异**✗ ⇒ **∴ 噪声最小 ✓ ★
// **∴ 它不是判据 ✗**：**不断言时间**（**∴ 只打印中位 ＋ min／max ✓）★
//
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test layer_fusion_bench -- --nocapture`
use std::time::Instant;
use yanshi_render::blend::BlendMode;
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
fn bench_layer_opacity_fusion() {
    // **∴ 4K 文档 ✓**（**∴ 与第 630／636 轮同一规模 ✓）★
    let (w, h) = (3840u32, 2160u32);
    let rounds = 5usize;
    let mut a_times = Vec::with_capacity(rounds);
    let mut b_times = Vec::with_capacity(rounds);
    for r in 0..rounds {
        let mut layer = Buffer::new(0, 0, w, h);
        fill(&mut layer, 0x1234_5678 ^ (r as u32));
        let mut base = Buffer::new(0, 0, w, h);
        fill(&mut base, 0x9e37_79b9);
        // **★★★ 关键 ✗ ★★★**：**准备动作一律**放在计时之外**（**第 640 轮修正 ✓）
        //   **∴ 为什么 ✗**：**第一版把 `base.clone()`／`layer.clone()` 计进了 A**✗
        //     ＋ **∴ 而**真实渲染路径**里两者都来自**池／原地复用**（**不克隆 ✓）
        //       ⇒ **∴ 于是**第一版的 36.7% **高估了**真实收益** ✓
        //         ＋ **∴ 现在**：**A ＝ 乘 ＋ 合成**✗ ｜ **B ＝ 融合** ✓
        //           ⇒ **★ 两者的差**正好是**那一遍整层遍历** ✓ ★
        let mut layer_a = layer.clone();
        let mut acc_a = base.clone();
        let mut acc_b = base.clone();
        // **∴ A：**整层乘 ＋ 合成**（**旧路径 ✓）★
        let t = Instant::now();
        layer_a.multiply_alpha(0.5);
        acc_a.composite(&layer_a, BlendMode::Normal, 1.0);
        std::hint::black_box(acc_a.pixels_mut()[0]);
        a_times.push(t.elapsed().as_secs_f64() * 1000.0);
        // **∴ B：**融合（**不修改 layer ✓）★
        let t = Instant::now();
        acc_b.composite_with_layer(&layer, BlendMode::Normal, 0.5);
        std::hint::black_box(acc_b.pixels_mut()[0]);
        b_times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    a_times.sort_by(|x, y| x.partial_cmp(y).unwrap());
    b_times.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let am = a_times[rounds / 2];
    let bm = b_times[rounds / 2];
    println!(
        "  ★ 4K 单层｜A（整层乘＋合成）中位 {am:.1} ms（min {:.1}｜max {:.1}）",
        a_times[0],
        a_times[rounds - 1]
    );
    println!(
        "  ★ 4K 单层｜B（融合）        中位 {bm:.1} ms（min {:.1}｜max {:.1}）",
        b_times[0],
        b_times[rounds - 1]
    );
    println!(
        "  ⇒ ★ 真实路径对比（**无克隆**）：省 {:.1} ms／层（{:.1}%）｜若 5 层 ⇒ 约 {:.0} ms",
        am - bm,
        (am - bm) / am * 100.0,
        (am - bm) * 5.0
    );
}
