//! **★ 渲染器的 GPU 分派：结果必须与纯 CPU **逐位相同**** ✗ ★**（第 308 轮 ✓；**目标第 7 条 ✓）。
//!
//! **∴ 判据 ✗**：**构造一个**超过阈值**的缓冲**✗（**∴ 于是**它会**真的走 GPU** ✓）
//!   ⇒ **∴ 断言**：**`to_rgba8_quantized(None)` 必须与**纯 CPU 的结果**逐字节相同** ✓
//!     **∴ 而那个「纯 CPU 的结果」**由**同一函数的**另一条路**给出** ✓
//!       **∴ 即**：**在**不开 feature** 的构建里跑同一个测试**✗ ⇒ **∴ 得到**真值** ✓**** ✓✓
//!
//! **∴ 变异点（**两个 ✓）★**：
//!   **∴ ①** **把 `try_quantize_on_gpu` 的**逐位核对**拆掉**✗
//!     ⇒ **∴ 若** GPU 与 CPU 有分叉**✗ ⇒ **∴ 断言**必红** ✓**** ✓✓
//!   **∴ ②** **把阈值改成 0**✗ ⇒ **∴ 小图也走 GPU** ✗
//!     ⇒ **∴ 那**会让**行为变差**✗ ⇒ **∴ 由**另两条判据**挡住** ✓（**第 281／286 轮 ✓）** ✓✓
//!
//! **∴ 平台说明 ✗**：**没有适配器时**✗ ⇒ **∴ GPU 路**回退 CPU**✗
//!   ⇒ **∴ 于是**：**本判据**仍然通过** ✓（**∴ 因为**两条路**结果相同** ✓）
//!     ⇒ **∴ 而**它**不会**误红** ✓（**∴ 与** CI 的无 GPU runner**相容** ✓）** ✓✓
//!
//! `cargo test -p yanshi-render --features gpu --test gpu_quantize_dispatch`
#![cfg(feature = "gpu")]

use yanshi_render::buffer::Buffer;

#[test]
fn dispatch_matches_the_pure_cpu_result_bit_for_bit() {
    // **∴ 超过 `GPU_MIN_PIXELS`（30 000 ✓）✗ ⇒ **∴ 于是**真的会走 GPU** ✓
    let (w, h) = (256u32, 256u32);
    assert!(
        (w * h) as usize >= yanshi_gpu::GPU_MIN_PIXELS,
        "本测试必须超过阈值"
    );

    let mut b = Buffer::new(0, 0, w, h);
    let mut s: u32 = 0x9E37_79B9;
    for y in 0..h {
        for x in 0..w {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let a = ((s >> 20) & 0xFF) as f32 / 255.0;
            let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
            b.set_pixel(x, y, [f(0) * a, f(1) * a, f(2) * a, a]);
        }
    }

    // **∴ 被测：**开了 feature ⇒ **可能**走 GPU（**∴ 由**规模 ＋ 适配器决定 ✓）**
    let got = b.to_rgba8_quantized(None);

    // **★ 真值：**手工走 CPU 那条实现（**∴ 与主路径**同一函数** ✓）★**
    let mut want = vec![0u8; (w * h * 4) as usize];
    // **∴ 用公开的 `pixels` 字段（**∴ 本 crate 内 ✓）
    yanshi_render::rows::encode_quantized_rows(
        &b.pixels, &mut want, 0, h as usize, w as usize, None,
    );

    assert_eq!(got.len(), want.len(), "长度必须一致");
    let bad = (0..want.len()).filter(|&i| got[i] != want[i]).count();
    println!(
        "  GPU 分派：{}x{}（{} 像素，**阈值 {}）｜★ 不符字节 = {} ★",
        w,
        h,
        w * h,
        yanshi_gpu::GPU_MIN_PIXELS,
        bad
    );
    assert_eq!(
        bad, 0,
        "**∴ 分派后的结果**必须与纯 CPU **逐位相同****（**目标第 7 条 ✓）"
    );
}
