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
