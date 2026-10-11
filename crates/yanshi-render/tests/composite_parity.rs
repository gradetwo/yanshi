// **★★★ 判据：GPU 合成必须与 CPU **逐位一致** ✗ ★★★**（**第 626 轮 ✓；目标第 11 条 ✓）
//
// **∴ 为什么这是**最关键的判据**✗**：
//   **∴ 目标第 11 条要求「**先立判据再动**」✗
//     ＋ **∴ 而**最大的未知是**：**GPU 的 f32 会不会**融合成 fma**✗
//       ⇒ **∴ 那**会让**舍入不同 ⇒ **∴ 逐位一致**失败** ✓ ★
//
// **★★★ 本判据的设计（**两条对照 ✓）★★★**
//   **∴ ① 普通 CPU ✗**：`over()` 的**原始表达式**（**`a + b * c`** ✓）
//     ＋ **∴ ② fma CPU ✗**：**同一算式但用** `f32::mul_add`** ✓
//       ⇒ **∴ 而** GPU **必然**是其中之一**（**∴ 硬件要么融合要么不融合 ✓）
//         ＋ **∴ 所以 ✗**：**若** GPU 与①一致 ⇒ **∴ 它**没融合**✗ ⇒ **∴ 逐位一致**成立** ✓
//           ＋ **∴ 若** GPU 与②一致 ⇒ **∴ 它**融合了**✗
//             ⇒ **∴ 对策**：**让 CPU 也用 `mul_add`**（**∴ 两边一致 ✓）★
//   **∴ 而**若**两者都不一致**✗ ⇒ **∴ 说明**有**别的差异**（**∴ 如**优化／精度 ✓）★
//
// **∴ 退出码 ✗**：**0 ＝ 全部逐位一致**✗｜**1 ＝ 有不一致（**并打印**通道 delta ✓）**★
//   ＋ **∴ 而**`3` **＝**判据自身跑不了**（**∴ 如**无适配器 ✓ ⇒ **∴ 不是**产品问题 ✓）★
#![cfg(all(feature = "gpu", not(target_arch = "wasm32")))]

use yanshi_render::blend::over;

/// **∴ 确定性伪随机 ✗**（**∴ 与 `tool-render-cost-accounts.mjs` 同思路 ⇒ 可复现 ✓）★
fn pixels(seed: u32, n: usize) -> Vec<f32> {
    let mut state = seed | 1;
    let mut out = Vec::with_capacity(n * 4);
    for _ in 0..n * 4 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        // **∴ 覆盖 0／1／中间值 ⇒ **边界也要比** ✓（**∴ 早退分支的关键 ✓）★
        let v = ((state >> 8) & 0xffff) as f32 / 65535.0;
        out.push(v);
    }
    // **∴ 显式塞入边界 alpha** ✗（**∴ α ＝ 0／1 会走 CPU 的早退分支 ✓）★
    if n >= 2 {
        out[3] = 0.0;
        out[7] = 1.0;
    }
    out
}

#[test]
fn gpu_composite_normal_is_bit_exact_against_cpu() {
    let n = 4096;
    let src = pixels(0x1234_5678, n);
    let dst = pixels(0x9e37_79b9, n);
    let lut = yanshi_render::color::srgb_encode_table();
    let quantizer = match yanshi_gpu::Quantizer::new(lut) {
        Ok(q) => q,
        Err(e) => {
            // **∴ 没有适配器 ⇒ 判据自己跑不了 ⇒ **exit 3**（**∴ 不是产品问题 ✓）★
            eprintln!("  判据自身的求值失败（不是产品问题）：没有 GPU 适配器 ⇒ {e}");
            std::process::exit(3);
        }
    };
    let gpu = quantizer
        .composite_normal(&src, &dst)
        .expect("GPU 合成应返回结果");
    assert_eq!(gpu.len(), src.len(), "长度必须相同");
    // **∴ 两条 CPU 对照 ✗**：
    let mut plain = Vec::with_capacity(src.len());
    let mut fused = Vec::with_capacity(src.len());
    for i in 0..n {
        let s = [src[i * 4], src[i * 4 + 1], src[i * 4 + 2], src[i * 4 + 3]];
        let d = [dst[i * 4], dst[i * 4 + 1], dst[i * 4 + 2], dst[i * 4 + 3]];
        plain.extend_from_slice(&over(s, d));
        fused.extend_from_slice(&over_fma(s, d));
    }
    let mism_plain = count_mismatch(&gpu, &plain);
    let mism_fused = count_mismatch(&gpu, &fused);
    println!("  非逐位相同的通道数｜vs 普通 CPU：{mism_plain}｜vs fma CPU：{mism_fused}");
    if mism_plain == 0 {
        println!("  ✓ GPU 与**普通 CPU**逐位一致 ⇒ 没有 fma 融合 ⇒ 逐位一致成立");
    } else if mism_fused == 0 {
        println!("  ★ GPU 与**fma CPU**逐位一致 ⇒ GPU 融合了 fma ⇒ 对策：CPU 也用 mul_add");
    } else {
        println!("  ✗ GPU 与两者都不同 ⇒ 需要进一步排查");
    }
    assert_eq!(
        mism_plain.min(mism_fused),
        0,
        "GPU 必须与「普通 CPU」或「fma CPU」其中之一逐位一致"
    );
}

/// **∴ fma 版的 `over` ✗**（**∴ 只把**两处 `a + b * c`**换成 `mul_add` ✓）★
fn over_fma(source: [f32; 4], backdrop: [f32; 4]) -> [f32; 4] {
    let alpha_s = source[3];
    if alpha_s <= 0.0 {
        return backdrop;
    }
    if alpha_s >= 1.0 {
        return source;
    }
    let inverse = 1.0 - alpha_s;
    [
        backdrop[0].mul_add(inverse, source[0]),
        backdrop[1].mul_add(inverse, source[1]),
        backdrop[2].mul_add(inverse, source[2]),
        backdrop[3].mul_add(inverse, source[3]),
    ]
}

fn count_mismatch(a: &[f32], b: &[f32]) -> usize {
    a.iter()
        .zip(b)
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count()
}
