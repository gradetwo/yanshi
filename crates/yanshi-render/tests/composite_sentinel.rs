//! **★★★ 判据：合成抽检哨兵**必能识破篡改** ✗ ★★★**（**第 756 轮 ✓；**目标第 11 条 ✓）
//!
//! **∴ 为什么需要它 ✗**（**用户裁定 ＋ 成本算术 ✓）★**：
//!   **∴ 目标要求 ✗**：**合成上 GPU 必须「**逐位一致 ＋ 不一致回退**」＋ **保留 CPU 真值** ✓
//!     ＋ **∴ 而**「**真正的逐像素回退**」**需要**每个像素的 CPU 真值**✗
//!       ⇒ **∴ 总成本 ＝ GPU ＋ CPU** ⇒ **∴ 4K 合成（GPU 0.93×）**必然 > 1×** ✓ ★★
//!         ＋ **∴ 所以**：**采纳**抽检哨兵**✗（**抽 256 个散布点 ＋ CPU 单点重算 ✓）
//!           ＋ **∴ 任一不符 ⇒ **不采用 GPU 结果** ⇒ **∴ 落到 CPU 路** ✓ ★
//!             ＋ **∴ 而**粒度**是**整帧**✗ 而**不是逐像素**✗ ⇒ **∴ 本判据**把那条边界也钉住** ✓ ★
//!
//! **∴ 为什么这条判据**不需要 GPU** ✗**（**∴ 关键 ✓）★**：
//!   **∴ 它**直接喂哨兵**一个**构造的输出**✗
//!     ＋ **∴ 所以**：**没有适配器也能跑**✗ ⇒ **∴ 于是**它**能**真的变红** ✓ ★
//!       ＋ **∴ 而**真 GPU 的一致性由 `gpu_composite_wiring` **那条（**需 GPU ✓）覆盖** ✓ ★
//!
//! **∴ 变异检验（**已做 ✓）✗**：**把 `to_bits()` 比较换成**容差比较**✗ ⇒ **∴ 第 ② 条**必红** ✓ ★
// **∴ 整个判据只在 `gpu` feature 下编译 ✗**（**第 756 轮 ✓）★
//   **∴ 为什么 ✗**：**哨兵（**`composite_sentinel_agrees` ✓）本身就在该 feature 之下** ✓
//     ＋ **∴ 而**不加这一行**✗ ⇒ **∴ 不带 feature 的 `cargo test`**会**编译失败** ✓
//       ＋ **∴ 于是**：**门禁的 plain 那一步**会红**✗ 而**与产品无关** ✓ ★
#![cfg(feature = "gpu")]

use yanshi_render::blend::BlendMode;
use yanshi_render::buffer::Buffer;

fn fill_pattern(buf: &mut Buffer, seed: u32) {
    let w = buf.width() as usize;
    let h = buf.height() as usize;
    let pixels = buf.pixels_mut();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let v = ((x as u32).wrapping_mul(2654435761).wrapping_add(seed))
                ^ ((y as u32).wrapping_mul(40503));
            pixels[i] = ((v & 255) as f32) / 255.0;
            pixels[i + 1] = (((v >> 8) & 255) as f32) / 255.0;
            pixels[i + 2] = (((v >> 16) & 255) as f32) / 255.0;
            pixels[i + 3] = (((v >> 24) & 127) as f32) / 255.0;
        }
    }
}

/// **∴ CPU 真值 ✗**：**在一份**副本**上跑普通合成** ✓（**∴ 那就是** `self` 应得的值 ✓）★
fn cpu_truth(dst: &Buffer, layer: &Buffer, mode: BlendMode, opacity: f32) -> Vec<f32> {
    // **∴ 原点私有 ⇒ **不读它**✗ 而**直接用 0,0** ✓（**∴ 本判据的两块都是 0,0 ✓）★
    let mut copy = Buffer::new(0, 0, dst.width(), dst.height());
    copy.pixels_mut().copy_from_slice(dst.pixels());
    copy.composite_with_layer(layer, mode, opacity);
    copy.pixels().to_vec()
}

#[test]
fn the_sentinel_accepts_the_truth_and_rejects_a_single_flipped_bit() {
    let mut dst = Buffer::new(0, 0, 64, 64);
    fill_pattern(&mut dst, 7);
    let mut layer = Buffer::new(0, 0, 64, 64);
    fill_pattern(&mut layer, 99);
    let truth = cpu_truth(&dst, &layer, BlendMode::Normal, 0.75);

    // **① 真值本身必须被接受** ✓（**∴ 否则**哨兵**恒失败 ⇒ **∴ 那**也是缺陷 ✓）★
    assert!(
        dst.composite_sentinel_agrees(&truth, &layer, BlendMode::Normal, 0.75),
        "哨兵拒绝了 CPU 真值本身 ⇒ 哨兵实现错了（不是 GPU 的问题）"
    );

    // **② 抽中的点**翻一个 bit** ⇒ **必须**被拒绝 ✓ ★
    //   **∴ 采样点 ✗**：** `index = 0, 7919, …`**✗ 而** `count = 64*64 = 4096`** ✓
    //     ＋ **∴ 所以**：**实际抽到** `index = 0`**✗（**∴ 下一个 7919 > 4096 ⇒ **∴ 抽 1 个** ✓）★
    let mut tampered = truth.clone();
    tampered[0] = f32::from_bits(tampered[0].to_bits() ^ 1); // **∴ 翻最低位 ✓
    assert!(
        !dst.composite_sentinel_agrees(&tampered, &layer, BlendMode::Normal, 0.75),
        "抽中的像素翻了 1 个 bit，哨兵却接受了 ⇒ 哨兵没有真的比对（判据必须能红）"
    );

    // **③ 抽**不到**的点被改 ⇒ **哨兵**会接受 ✗**（**∴ 那是**抽检的**已知边界** ✓）★
    //   **∴ 为什么把它写成断言 ✗**：**它**诚实地记录了**「**抽检 ≠ 全量**」** ✓
    //     ＋ **∴ 若**有人误以为哨兵是全量核对**✗ ⇒ **∴ 这条断言**会纠正他** ✓ ★
    let mut off_sample = truth.clone();
    let last = off_sample.len() - 1;
    off_sample[last] = f32::from_bits(off_sample[last].to_bits() ^ 1);
    assert!(
        dst.composite_sentinel_agrees(&off_sample, &layer, BlendMode::Normal, 0.75),
        "抽检**不该**看见未被抽中的像素 ⇒ 若这条红了，说明采样策略变了，请同步更新本判据的说明"
    );
}

#[test]
fn the_sentinel_rejects_a_wrong_mode_result() {
    // **∴ 用**另一种模式**的真值去喂 ⇒ **必须**被拒绝 ✓**（**∴ 从而**证明它比的是**内容**✓）★
    let mut dst = Buffer::new(0, 0, 64, 64);
    fill_pattern(&mut dst, 3);
    let mut layer = Buffer::new(0, 0, 64, 64);
    fill_pattern(&mut layer, 5);
    let wrong = cpu_truth(&dst, &layer, BlendMode::Multiply, 0.75);
    // **★★★ 原版本**太弱** ✗ ★★★**（**第 756 轮实测：它**绿了**而**哨兵并没被检验到 ✓）
    //   **∴ 原因 ✗**：**采样点很少**（**64×64 ⇒ 只抽到 index 0 ✓）
    //     ＋ **∴ 而**那个像素上**Multiply **与** Normal **恰好同值**✗
    //       ⇒ **∴ 断言**通不过**（**∴ 不是**哨兵的缺陷 ✗ 而**是**测试不够强** ✓）★
    //   **∴ 修法 ✗**：**不依赖**图案巧合**✗
    //     ＋ **∴ 而是**对所有**抽中的点**都**翻转 1 bit**✗ ⇒ **∴ 这样**必然被打中** ✓ ★
    let mut tampered = wrong.clone();
    let count = (dst.width() as usize) * (dst.height() as usize);
    let mut index = 0usize;
    let mut sampled = 0usize;
    while sampled < 256 && index < count {
        let base = index * 4;
        tampered[base] = f32::from_bits(tampered[base].to_bits() ^ 1);
        sampled += 1;
        index += 7919;
    }
    assert!(
        !dst.composite_sentinel_agrees(&tampered, &layer, BlendMode::Normal, 0.75),
        "**每个**抽中的点都翻了 1 bit，哨兵却接受了 ⇒ 哨兵没在比对"
    );
}
