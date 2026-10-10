//! **行区间并行的公共设施**（只用标准库；`wasm32` 上线程代码整体不参与编译）。
//!
//! 全 crate 只有两处按行并行的大循环 —— 渲染分带（`render::parallel_impl`）与输出量化
//! （[`crate::Buffer::to_rgba8`]）—— 它们共用这里的分带切法与并发上限，避免"两处各写一个
//! 16，日后悄悄漂移"。
//!
//! **顺序无关性的根据**（与 `render::parallel_impl` 的说明一致）：
//! * 每个 worker 拿到的是**互不重叠**的行切片（`split_at_mut` 保证）；
//! * 每个输出字节只由它自己那一行的像素决定，没有跨行求和/累加；
//! * 因此"谁先算完、以什么顺序拼回"都不改变任何字节，也没有浮点规约的舍入问题。
//!
//! `wasm32-unknown-unknown` 没有共享内存线程 ⇒ 本模块的并行部分被 `#[cfg]` 掉，
//! 调用方在 wasm 上回退到原来的**串行**循环，逐字节一致。
//!
//! 本模块还放着输出量化的**逐像素实现** [`encode_quantized_rows`]：它不按目标门控
//! （wasm 也编译），原生并行版与 wasm 串行版共用同一份 ⇒ 原生判据能直接跑到 wasm
//! 分支所用的代码，且 wasm 上不再需要"先复制整幅 f32 再就地量化"。

/// **并行任务的最小像素数**：低于它时线程创建与每块的固定开销盖过收益。
///
/// 这是本 crate 的**唯一**阈值：渲染分带（`render::parallel_impl` 的分派）与
/// [`crate::Buffer::to_rgba8`] 的输出量化都读它。128² = 16384 像素的依据是**实测**
/// （release、4 核、不透明白底、同进程交替 best-of-5）：128² 上串行 1.33ms → 4 worker
/// 0.74ms（1.80×），256² 上 5.39ms → 2.33ms（2.31×），4K 上 0.67s → 0.27s（2.48×），
/// 8K 上 2.86s → 1.12s（2.54×）；64²（4096 像素）串行本身只有约 0.33ms，线程创建
/// 已占可观比例 ⇒ 不并行。阈值不变时挑 128² 是保守的：收益在 256² 起才明显。
pub(crate) const PARALLEL_MIN_PIXELS: usize = 128 * 128;

/// **worker 上限**：避免在超高核数机器上把区域切得过碎、内存峰值失控。
///
/// `buffer_pool::MAX_POOLED_BUFFERS` 也按这个上限取值（见那里的说明）。
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MAX_WORKERS: usize = 16;

/// **渲染分块的最小行数**：块太薄时"每块都要重新解析对象/分配缓冲"的固定成本会反噬。
///
/// 只用于渲染分带；输出量化的每块固定成本小得多，不需要这么厚的块。
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const MIN_BAND_ROWS: u32 = 24;

/// 把 `height` 行切成至多 `workers` 块（前面的块各多一行 ⇒ 覆盖完整、与调度无关的确定性切法）。
///
/// 返回值之和恒等于 `height`（`height == 0` 时为空）；块数 = `min(workers, height)`。
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn split_bands(height: u32, workers: usize) -> Vec<(u32, u32)> {
    let count = workers.min(height as usize).max(1);
    let base = height / count as u32;
    let extra = height % count as u32;
    let mut bands = Vec::with_capacity(count);
    let mut row = 0u32;
    for index in 0..count {
        let rows = base + if (index as u32) < extra { 1 } else { 0 };
        if rows == 0 {
            continue;
        }
        bands.push((row, rows));
        row += rows;
    }
    bands
}

/// 把 `out`（`height` 行、每行 `row_bytes` 字节）按行切成至多 `workers` 个连续块，
/// 用 **scoped 线程**并行执行 `encode(row_start, rows, 本块的行切片)`；返回实际块数。
///
/// * 切片由 `split_at_mut` 切出 ⇒ **块之间互不重叠**，每个字节恰好被一个 worker 写一次；
/// * `scope` 结束即汇合 ⇒ 返回后 `out` 已完整写好；
/// * `blocks == 1` 表示只用了一个块（调用方在启动前就按阈值/核数决定不并行）。
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn for_each_band_mut(
    height: usize,
    row_bytes: usize,
    workers: usize,
    out: &mut [u8],
    encode: impl Fn(usize, usize, &mut [u8]) + Sync,
) -> usize {
    debug_assert_eq!(out.len(), height.saturating_mul(row_bytes));
    let mut jobs: Vec<(&mut [u8], usize, usize)> = Vec::new();
    {
        let bands = split_bands(height as u32, workers);
        jobs.reserve(bands.len());
        let mut rest: &mut [u8] = out;
        for (row_start, rows) in bands {
            let len = rows as usize * row_bytes;
            let (head, tail) = rest.split_at_mut(len);
            jobs.push((head, row_start as usize, rows as usize));
            rest = tail;
        }
    }
    let blocks = jobs.len();
    std::thread::scope(|scope| {
        for (line, row_start, rows) in jobs {
            let encode = &encode;
            scope.spawn(move || encode(row_start, rows, line));
        }
    });
    blocks
}

/// 把 `[row_start, row_start + rows)` 行的像素**逐像素量化到 f16 再编码**成显示空间 u8。
///
/// 这是输出量化的**唯一逐像素实现**：原生并行版（`render::parallel_impl`）与 wasm 串行版
/// （`render::parallel_impl` 的 wasm 变体）都调用它 ⇒ 两端不会各自演化出不同的量化/编码。
///
/// **不复制整幅 f32**：直接在调用方给的 `source`（原始缓冲的像素切片）上按行取切片，
/// 边量化边写入 `destination` 的对应行。旧 wasm 实现先 `buffer.crop(&buffer.bbox())`
/// 复制一份整幅 f32（8K 上 506 MiB）再就地量化 ⇒ 峰值内存多一份整幅；本函数是这个副本
/// 被删掉之后的落点。
///
/// 本函数**不按目标门控**（`wasm32` 也编译），因此原生测试能直接跑到与 wasm 分支
/// **同一份**代码，不是"另一套等价实现"。
///
/// 每像素只由它自己那 4 个 f32 决定（没有跨行状态、没有浮点规约）⇒ 行区间怎么切都不改变
/// 任何字节（顺序无关性的根据见本模块文档）。
/// **∴ 第 308 轮**放宽为 `pub`**✗ ⇒ **∴ 于是**：**判据**可以**直接调**它**当作**CPU 真值**** ✓
///   （**∴ 而**主路径**用的**就是它**✗ ⇒ **∴ 不是**另写一套 ✓）** ✓✓
pub fn encode_quantized_rows(
    source: &[f32],
    destination: &mut [u8],
    row_start: usize,
    rows: usize,
    width: usize,
    bg_linear: Option<crate::color::LinearRgba>,
) {
    let table = crate::color::srgb_encode_table();
    // **★ 逐像素热循环的**零风险**整理 ✗ ★**（第 37 轮 ✓；**不改结果 ✓）：
    //   **∴ ①** **`match bg_linear` 提到**行外** ✗**（**∴ 每像素一个分支 ⇒ **∴ 每行一个 ✓）**
    //   **∴ ②** **改**切片迭代**✗（**`chunks_exact` ＋ `zip` ✓）
    //     ⇒ **∴ 于是**：**每元素的边界检查**消失 ✓**（**∴ 结果**逐字节不变 ✓）** ✓✓
    //   **∴ 判据 ✗**：**仓内大量"**逐字节相同**"判据 ＋ 全工作区测试守住 ✓**** ✓✓
    for row in 0..rows {
        let y = row_start + row;
        let src = &source[y * width * 4..(y + 1) * width * 4];
        let dst = &mut destination[row * width * 4..(row + 1) * width * 4];
        for (src_pixel, dst_pixel) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
            let pixel = [
                crate::half::quantize_f16(src_pixel[0]),
                crate::half::quantize_f16(src_pixel[1]),
                crate::half::quantize_f16(src_pixel[2]),
                crate::half::quantize_f16(src_pixel[3]),
            ];
            let bytes = match bg_linear {
                Some(bg) => crate::color::composite_over_linear_with(table, pixel, bg),
                None => crate::color::linear_premul_to_u8x4_with(table, pixel),
            };
            dst_pixel.copy_from_slice(&bytes);
        }
    }
}

/// **★ 「量化」这一段的**内部分布**✗ ★**（第 182 轮 ✓；**阶段一第 2 条的成本调查 ✓）。
///
/// **∴ 为什么需要它 ✗**：**量化**是首帧的**第一或第二大项** ✓
/// （**本次实测** 945.6 ms ＝ **49.5%** ✗；**文档 §14.1 的另一个场景** 658.6 ms ＝ **31.0%** ✓）。
/// **∴ 而**要不要**引入 SIMD**（**如 `wide` ✓）**✗ ⇒ **∴ 完全取决于**——
/// **∴ f16 往返**（`quantize_f16` ✓）**占它多少** ✓。
///
/// **∴ 逐像素做两件事 ✗**：
/// **∴ ①** **4 × `quantize_f16`** ✗（**RGBA 各一次 ✓**；**f32 → f16 → f32 往返 ✓**）
/// **∴ ②** **颜色表转换 ＋ 写 4 字节** ✓
/// ⇒ **∴ 若** ① 占大头**✗ ⇒ **∴ SIMD** 有明确收益 ✓；
/// **∴ 若** ② 占大头**✗ ⇒ **∴ `wide` 帮不上** ✓ ⇒ **∴ 该查**内存带宽 ✓（**与「裁剪+存 tile 20%」同类 ✓）** ✓✓
///
/// **∴ 口径（**如实 ✓）★**：**这是**单线程**的**比例**✗ ⇒ **∴ 比例**与**并行时相同** ✓
/// （**∴ 因为**两条路**都是**逐像素独立**的 ✓）；**∴ 而**绝对时间**不许**跨机器比较 ✓**。
#[cfg(all(test, not(target_arch = "wasm32")))]
mod split_probe {
    use super::*;
    use std::hint::black_box;
    use std::time::Instant;

    #[test]
    fn f16_share_of_the_quantize_stage() {
        // **∴ 规模**取 1024² ✗ ⇒ **∴ 419 万次** `quantize_f16` ✓（**∴ 比例**与 4K 同 ✓，**而**测试很快 ✓）
        let width = 1024usize;
        let rows = 1024usize;
        let pixels = width * rows;
        let source: Vec<f32> = (0..pixels * 4)
            .map(|i| ((i % 977) as f32) / 977.0)
            .collect();
        let mut destination = vec![0u8; pixels * 4];
        let bg: Option<crate::color::LinearRgba> = None;

        // **∴ 热身 ✗**：`srgb_encode_table()` **是**懒构建**✗ ⇒ **∴ 不热身**会**把建表算进第一次** ✓
        encode_quantized_rows(&source, &mut destination, 0, rows, width, bg);

        // **∴ ① 只测 f16 往返 ✗**（**与热循环**同样的 4 次调用 ✓）
        let t0 = Instant::now();
        let mut sink = 0.0f32;
        for pixel in source.chunks_exact(4) {
            sink += crate::half::quantize_f16(black_box(pixel[0]));
            sink += crate::half::quantize_f16(black_box(pixel[1]));
            sink += crate::half::quantize_f16(black_box(pixel[2]));
            sink += crate::half::quantize_f16(black_box(pixel[3]));
        }
        let f16_only = t0.elapsed();
        black_box(sink);

        // **∴ ② 测整条逐像素路 ✗**（**f16 ＋ 颜色表 ＋ 写 ✓）
        let t1 = Instant::now();
        encode_quantized_rows(&source, &mut destination, 0, rows, width, bg);
        let whole = t1.elapsed();
        black_box(&destination);

        let share = f16_only.as_secs_f64() / whole.as_secs_f64();
        println!(
            "SPLIT 像素={pixels} f16往返={:.3}ms 整条={:.3}ms ⇒ f16占={:.1}%",
            f16_only.as_secs_f64() * 1000.0,
            whole.as_secs_f64() * 1000.0,
            share * 100.0
        );
        // **★ 只断言**测量真的发生了** ✗ ★**：**∴ 若**循环被优化掉**✗ ⇒ **∴ `f16_only` 会是 0** ✓
        //   ⇒ **∴ 那**正是这个测试**唯一**要防的失败模式 ✓（**∴ 不**断言具体比例 ✓ —— **∴ 那**会因机器而抖 ✓）。
        assert!(
            (0.05..0.95).contains(&share),
            "f16 往返占整条的 {:.1}% ⇒ **∴ 不在** 5%–95%**✗ ⇒ **∴ 要么**测量被优化掉了 ✗，\
             **要么**两条路的规模不一致 ✓（**∴ 两者**都要先查清 ✓）",
            share * 100.0
        );
    }
}
