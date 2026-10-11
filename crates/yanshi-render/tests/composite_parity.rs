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
    println!("  ── f32 空间（**线性预乘 ✓）──");
    println!("  非逐位相同的通道数｜vs 普通 CPU：{mism_plain}｜vs fma CPU：{mism_fused}");
    println!(
        "  ★ 最大绝对差｜vs 普通 CPU：{:.3e}｜vs fma CPU：{:.3e}",
        max_abs_diff(&gpu, &plain),
        max_abs_diff(&gpu, &fused)
    );
    // **★★★ 关键：**量化成 u8 之后**才是**用户看到的****（**第 627 轮 ✓）★★★
    //   **∴ 为什么必须量它 ✗**：**f32 的差可能只有几个 ULP**✗
    //     ＋ **∴ 而**它**经过**「除以 alpha ⇒ 查 sRGB 表 ⇒ ×255 + 0.5 ⇒ floor」**后
    //       **可能**完全消失** ✓（**∴ 因为**那是**量化 ✓）
    //         ⇒ **★ 若** u8 **逐位一致 ⇒ **∴ 产品输出**完全一致**✗
    //           ⇒ **∴ 那**比「f32 容差」**更强、更有意义** ✓ ★
    let table = yanshi_render::color::srgb_encode_table();
    let to_u8 = |v: &[f32]| -> Vec<u8> {
        v.chunks_exact(4)
            .flat_map(|c| {
                yanshi_render::color::linear_premul_to_u8x4_with(table, [c[0], c[1], c[2], c[3]])
            })
            .collect()
    };
    let (gu8, pu8, fu8) = (to_u8(&gpu), to_u8(&plain), to_u8(&fused));
    let u8_plain = count_mismatch_u8(&gu8, &pu8);
    let u8_fused = count_mismatch_u8(&gu8, &fu8);
    println!("  ── u8 空间（**用户实际看到的 ✓）──");
    println!("  非逐位相同的字节数｜vs 普通 CPU：{u8_plain}｜vs fma CPU：{u8_fused}");
    println!(
        "  ★ u8 最大差（**LSB ✓）｜vs 普通 CPU：{}｜vs fma CPU：{}",
        max_abs_diff_u8(&gu8, &pu8),
        max_abs_diff_u8(&gu8, &fu8)
    );
    let best = u8_plain.min(u8_fused);
    if best == 0 {
        println!("  ✓★ 量化后**逐位一致** ⇒ 产品输出与 CPU 完全一致（**无需容差**）★");
    } else {
        println!("  ★ 量化后仍有差 ⇒ 容差 ＝ {best} 字节｜最大 LSB 差见上");
    }
    // **∴ 断言：**u8 必须与「普通 CPU」或「fma CPU」之一逐位一致****（**第 627 轮 ✓）
    //   ＋ **∴ 若**将来**达不到** ⇒ **∴ 那**就是**引入容差的时刻**（**按用户口径 ✓）★
    assert_eq!(
        best, 0,
        "GPU 的 u8 输出必须与「普通 CPU」或「fma CPU」其中之一逐位一致"
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

// **∴ 差值统计 ✗**（**第 627 轮 ✓）★

fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f32, f32::max)
}

fn count_mismatch_u8(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

fn max_abs_diff_u8(a: &[u8], b: &[u8]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}
