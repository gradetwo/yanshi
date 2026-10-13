// **★★★ 判据：**图层合成默认不许走 GPU**（用户裁定：**放弃纯 GPU 路线****）★★★**
//   （**第 246 轮 ✓；**第 691 轮实测 ＋ 第 246 轮 L4 复测 ✓）
//
// **∴ 用户裁定（**原文 ✓）✗**：
//   「**GPU 就做 GPU 确实有收益的 ✗ 例如滤镜之类 ✗ 放弃纯 gpu 路线**」 ✓
//
// **∴ 支撑这条裁定的**两笔实测**（**都可复核 ✓）★**：
//   **∴ ① 合成走 GPU ✗ 是**负收益****（**快 0.54–0.56× ✗ 即**慢约 1.8 倍 ✓）：
//     ```
//     ★ 1024x1024：GPU min  792.26 ms｜CPU 三步 min  435.51 ms｜提速 0.55×
//     ★ 1920x1080：GPU min 1620.15 ms｜CPU 三步 min  870.64 ms｜提速 0.54×
//     ★ 3840x2160：GPU min 6228.76 ms｜CPU 三步 min 3475.27 ms｜提速 0.56×
//     ```
//     （**L4 ✓ `deviceName = NVIDIA L4`／`580.82.07`／Vulkan DiscreteGpu ✓）
//     ＋ **∴ 机制 ✗**：**每次调用**上传 src ＋ 上传 dst ＋ 读回 out**（**4K 约 100 MB ✓）
//       ⇒ **∴ 而**本机搬运只有 **0.4–1 GB/s** ⇒ **∴ 传输**把 GPU 的算**吃掉** ✓ ★★
//   **∴ ② 量化走 GPU ✗ 是**正收益****（**4K 端到端：**CPU 1541 ms ⇒ GPU 1358 ms ✓）
//     ⇒ **∴ 所以**：**GPU 留下 ✗ 但**只留在**它真的赢的地方** ✓ ★
//
// **∴ 本判据守什么 ✗**（**两个方向都守 ✓）★**：
//   **∴ ① 默认（**不设开关 ✓）⇒ **`gpu_composite_calls() == 0`** ✗
//     ⇒ **∴ 于是**：**谁把默认打开**✗ 或**移除开关直接走 GPU**✗ ⇒ **∴ 本判据**必红** ✓ ★★
//   **∴ ② 显式设 `YANSHI_GPU_COMPOSITE=1` ⇒ **必须真的走 GPU**** ✗
//     ⇒ **∴ 于是**：**若**开关变成空操作**✗ ⇒ **∴ 本判据**也红** ✓
//       ⇒ **∴ 而**那**正是"有牙"的一半**：**∴ 否则①的"0 次"可能是因为**整条路**坏了** ✓ ★★
//
// **∴ 为什么**单独一个测试文件**✗**（**∴ 而不是并进 `gpu_composite_wiring.rs` ✓）★**：
//   **∴ 因为**那个文件里的测试会 `std::env::set_var("YANSHI_GPU_COMPOSITE", "1")`**✗
//     ＋ **∴ 而** cargo 的测试**在**同一进程的多线程里跑**✗
//       ⇒ **∴ 于是**：**同一个二进制里的测试会**互相看见对方设的环境变量** ✓ ★
//         ＋ **∴ 若**把本判据放进去**✗ ⇒ **∴ 它**可能**因为别的测试设了开关而假红／假绿** ✓
//           ⇒ **★ 所以：**独立文件 ＝ 独立进程**✗ ⇒ **∴ 环境变量**不会被别的测试污染** ✓ ★★
//
// **∴ 跑法 ✗**：`cargo test -p yanshi-render --features gpu --test gpu_composite_default_off`
// **∴ 没有 GPU 时 ✗**：**如实跳过并打印 ✗**（**∴ 不是**假装通过 ✓ —— **∴ 本仓的纪律 ✓）
#![cfg(feature = "gpu")]

use yanshi_render::blend::BlendMode;
use yanshi_render::buffer::{
    gpu_composite_calls, init_gpu_quantizer, reset_gpu_composite_calls, Buffer,
};
use yanshi_render::color::srgb_encode_table;

/// **∴ 确定性填充（**预乘不变式：rgb ≤ alpha ✓）★
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
fn the_layer_composite_stays_off_the_gpu_by_default() {
    // **∴ 尺寸必须**超过阈值**✗**（**∴ 否则**两边都不走 GPU ⇒ **∴ 判据**什么都没测** ✓）★★
    //   ＋ **∴ 独显阈值 ＝ 1 048 576**（**`threshold_for(Discrete)` ✓）
    //     ⇒ **∴ 取 1024² ＝ 1 048 576**✗ ⇒ **∴ 恰好**在阈值上** ✓ ★
    let (w, h) = (1024u32, 1024u32);
    let lut = srgb_encode_table();
    if init_gpu_quantizer(lut.to_vec()).is_err() {
        // **∴ 没有适配器 ⇒ **如实跳过**✗ 而**不是**假装通过** ✓（**∴ 本仓的纪律 ✓）★
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）");
        return;
    }

    // ── 方向 ①：默认（**不设开关 ✓）⇒ **一次都不许走 GPU** ──
    std::env::remove_var("YANSHI_GPU_COMPOSITE");
    let mut layer = Buffer::new(0, 0, w, h);
    fill(&mut layer, 0xC0FF_EE01);
    reset_gpu_composite_calls();
    let mut acc = Buffer::new(0, 0, w, h);
    fill(&mut acc, 0x5EED_0001);
    acc.composite_with_layer(&layer, BlendMode::Normal, 1.0);
    let default_calls = gpu_composite_calls();
    println!("  ★ 默认：GPU 合成采用 {default_calls} 次（**应为 0 ✓）");
    assert_eq!(
        default_calls, 0,
        "默认必须走 CPU：用户裁定「放弃纯 GPU 路线」＋ 合成走 GPU 实测 0.54–0.56×（负收益）⇒ \
         谁把默认打开或去掉开关，都会让这条红"
    );

    // ── 方向 ②：显式打开 ⇒ **必须真的走 GPU**（**∴ 证明方向①的"0"不是因为整条路坏了 ✓）──
    std::env::set_var("YANSHI_GPU_COMPOSITE", "1");
    reset_gpu_composite_calls();
    let mut acc = Buffer::new(0, 0, w, h);
    fill(&mut acc, 0x5EED_0001);
    acc.composite_with_layer(&layer, BlendMode::Normal, 1.0);
    let forced_calls = gpu_composite_calls();
    std::env::remove_var("YANSHI_GPU_COMPOSITE");
    println!("  ★ 显式打开：GPU 合成采用 {forced_calls} 次（**应 > 0 ✓）");
    assert!(
        forced_calls > 0,
        "显式设 YANSHI_GPU_COMPOSITE=1 之后仍然没走 GPU ⇒ 这个开关成了空操作 ⇒ \
         **∴ 方向①的 0 次就不能说明「默认关闭」✗ 而只能说明「整条路坏了」**"
    );
}
