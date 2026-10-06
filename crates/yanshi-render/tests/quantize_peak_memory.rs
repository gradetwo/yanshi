//! **判据 2：量化路径不再分配整幅 f32 中间副本。**
//!
//! 用**全局分配器的单次最大分配**（不是墙钟：墙钟会被机器负载骗过）来判定。
//! 旧 wasm 实现做的 `buffer.crop(&buffer.bbox())` 会分配一个
//! `Vec<f32>`，长度 = 像素数 × 4 ⇒ 字节数 = 像素数 × 16 = **整幅 f32 大小**；
//! 新实现只分配输出 `Vec<u8>`（像素数 × 4 字节 = 整幅的 1/4）。
//! 因此断言"量化调用期间没有任何单次分配 ≥ 整幅 f32 大小"即可：
//! 把旧副本改回来，这条判据必红（单次最大分配正好 = 整幅 f32 大小）。
//!
//! **本文件只有一个 `#[test]`**：全局分配器是进程级的，测试并行跑会互相污染计数。
//! 想再加分配计数判据，请另开一个集成测试文件（= 另一个进程），不要加到这里。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use yanshi_render::Buffer;

/// 统计"测量窗口内"的单次最大分配与分配总字节数。
struct CountingAllocator;

static MEASURING: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);

fn note_alloc(size: usize) {
    if !MEASURING.load(Ordering::Relaxed) {
        return;
    }
    LARGEST.fetch_max(size, Ordering::Relaxed);
    TOTAL.fetch_add(size, Ordering::Relaxed);
}

// SAFETY: 只是把每个方法转发给 `System`，并在成功分配后记一笔计数；
// 不改变指针、布局或生命周期语义。
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() {
            note_alloc(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() {
            note_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        System.dealloc(pointer, layout);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let out = System.realloc(pointer, layout, new_size);
        if !out.is_null() && new_size > layout.size() {
            note_alloc(new_size - layout.size());
        }
        out
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

struct Measurement {
    largest: usize,
    total: usize,
}

fn measure_start() {
    LARGEST.store(0, Ordering::SeqCst);
    TOTAL.store(0, Ordering::SeqCst);
    MEASURING.store(true, Ordering::SeqCst);
}

fn measure_stop() -> Measurement {
    MEASURING.store(false, Ordering::SeqCst);
    Measurement {
        largest: LARGEST.load(Ordering::SeqCst),
        total: TOTAL.load(Ordering::SeqCst),
    }
}

fn probe_buffer(width: u32, height: u32) -> Buffer {
    let mut buffer = Buffer::new(0, 0, width, height);
    let mut state = 0x9e37_79b9u32;
    for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((state >> 8) & 0xff) as f32 / 255.0;
        let alpha = if index % 3 == 0 { 0.5 } else { 1.0 };
        pixel.copy_from_slice(&[a, ((state >> 16) & 0xff) as f32 / 255.0, 0.25, alpha]);
    }
    buffer
}

#[test]
fn no_full_frame_f32_copy_is_allocated() {
    const WIDTH: u32 = 1024;
    const HEIGHT: u32 = 512;
    let pixels = WIDTH as usize * HEIGHT as usize;
    let out_bytes = pixels * 4; // 输出 Vec<u8>：4 B/px
    let frame_bytes = pixels * 16; // 整幅 f32 RGBA：16 B/px（旧 crop 副本的大小）

    let buffer = probe_buffer(WIDTH, HEIGHT);
    let background = Some([255u8, 255, 255, 255]);
    // 预热：`srgb_encode_table()` 的 OnceLock 首次分配等一次性开销落在测量窗口之外。
    std::hint::black_box(buffer.to_rgba8_quantized(background));

    measure_start();
    let out = buffer.to_rgba8_quantized(background);
    let stats = measure_stop();

    assert_eq!(out.len(), out_bytes, "输出尺寸");
    std::hint::black_box(&out);
    println!(
        "1024×512（{} px）量化：单次最大分配 {} B｜总分配 {} B｜整幅 f32 副本应为 {} B｜输出 {} B",
        pixels, stats.largest, stats.total, frame_bytes, out_bytes
    );
    assert!(
        stats.largest < frame_bytes,
        "量化期间出现了 {} B 的单次分配，达到整幅 f32 副本量级（{} B）⇒ 副本没有删掉",
        stats.largest,
        frame_bytes
    );
}
