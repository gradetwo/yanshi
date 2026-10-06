//! **内存判据**：压缩期新增的堆内存**不得随输入长度增长** ✓ —— 这是本轮最核心的收益 ✓。
//!
//! 背景（外部报告对真实 8K 工作负载的实测 ✓，我在本机复现 ✓）：
//! 旧手写 LZ77 为每个输入字节建一个 `u32` 前驱表 ✓（`vec![u32::MAX; raw.len()]` ✓）
//! ⇒ 8K 的 132 MB 原始数据要**单块约 530 MB** ✗。
//! 本机旧实现实测（release ✓）：4K 扫线输入 33.18 MB ⇒ 压缩期新增峰值 **130 420 KiB** ✓（≈ 4× ✓），
//! 8K 132.7 MB ⇒ **520 772 KiB** ✓（≈ 4× ✓，进程峰值 638 MiB ✓）。
//!
//! **为什么用断言而不是计时/目测** ✓：时间在本机噪声大 ✓，而"内存与输入长度无关"是**结构性**的 ✓
//! —— 换成 `flate2`/`zlib-rs` 后压缩状态只有固定窗口 + 哈希表 + 32 KiB 输出缓冲 ✓。
//! 这条判据把结构钉住 ✓：谁把"每个字节一份元数据"加回来 ✓，它立刻红 ✓（变异验证见提交信息 ✓）。
//!
//! **它怎么量** ✓：这个测试**独占一个测试二进制** ✓（`tests/deflate_memory.rs` ✓，
//! 里面只有一条 `#[test]` ✓）⇒ 用记账式全局分配器统计"当前存活字节"的峰值 ✓，
//! 并在调用压缩**之前**先把输入缓冲分配好 ✓ ⇒ 量到的增长只属于**压缩本身** ✓。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use yanshi_render::png::zlib_compress_best;

/// 当前存活字节数与历史峰值 ✓。
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn record(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

struct Counting;

// SAFETY: 只是把调用转发给 `System` ✓，并在旁边记账 ✓ ——
// 记账用的是无锁原子量 ✓，不会重入分配器 ✓。
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() {
            record(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        System.dealloc(pointer, layout);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = System.realloc(pointer, layout, new_size);
        if !new_pointer.is_null() {
            if new_size >= layout.size() {
                record(new_size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// 在 `f` 运行期间测"相对于开始时刻的存活字节峰值" ✓。
fn peak_growth<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let output = f();
    let peak = PEAK.load(Ordering::Relaxed);
    (output, peak.saturating_sub(baseline))
}

/// 绘图型（**可压** ✓）数据：大片平色 + 渐变 ✓ —— 用户包里 91% 就是这种东西 ✓。
fn painting(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| {
            let pixel = index / 4;
            match index % 4 {
                0 => (pixel / 4096) as u8,
                1 => ((pixel / 64) % 251) as u8,
                2 => {
                    if (pixel / 16384) % 2 == 0 {
                        200
                    } else {
                        40
                    }
                }
                _ => 255,
            }
        })
        .collect()
}

const MIB: usize = 1 << 20;

#[test]
fn compression_scratch_does_not_grow_with_the_input_size() {
    // **输入在测量之前就分配好** ✓ ⇒ 量到的是压缩本身的开销 ✓。
    let small_input = painting(MIB);
    let large_input = painting(16 * MIB);

    let (small_packed, small_peak) = peak_growth(|| zlib_compress_best(&small_input));
    let (large_packed, large_peak) = peak_growth(|| zlib_compress_best(&large_input));
    // **把实测值打出来** ✓：`cargo test -- --nocapture` 时就是本判据的证据 ✓
    //（数字随 crate 版本变 ✓，判据只认结构 ✓）。
    println!(
        "压缩期新增堆峰值：1 MiB 输入 ⇒ {small_peak} 字节（{:.0} KiB）；16 MiB 输入 ⇒ {large_peak} 字节（{:.0} KiB）",
        small_peak as f64 / 1024.0,
        large_peak as f64 / 1024.0
    );

    // 自检：确实压得动 ✓（否则这条判据测的就不是"可压数据"这条路了 ✓）。
    assert!(
        small_packed.len() * 8 < small_input.len(),
        "语料必须可压：{} ⇒ {}",
        small_input.len(),
        small_packed.len()
    );

    // ① **绝对上界** ✓：压缩 16 MiB 可压数据的新增堆峰值必须远小于"每字节一份 u32"✗
    //    （后者是 64 MiB ✗）。这里给到 6 MiB 的余量 ✓（实测约 0.5 MiB ✓）。
    assert!(
        large_peak < 6 * MIB,
        "压缩期新增堆峰值过大：{large_peak} 字节（16 MiB 输入）⇒ \
         是不是又把“每个输入字节一份元数据”加回来了？"
    );

    // ② **与输入长度无关** ✓：输入大 16 倍 ✓，压缩期峰值不得跟着涨 ✗
    //    （旧实现会涨 (16-1)×4 = 60 MiB ✗）。
    assert!(
        large_peak < small_peak + 2 * MIB,
        "压缩期峰值必须与输入长度无关：1 MiB 输入 {small_peak} 字节 ⇒ 16 MiB 输入 {large_peak} 字节"
    );

    // ③ 顺带钉住输出确实解得出 ✓（这条是"内存判据"的护栏 ✓，真正的往返判据在别处 ✓）。
    let restored = yanshi_render::png::zlib_decompress(&large_packed).expect("必须能解回来");
    assert!(restored == large_input, "往返必须逐字节一致");
}
