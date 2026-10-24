//! C1-a 判据：解码裁剪不许再物化全幅 f32（`decode_crop_does_not_
//! materialize_full_frame`）。计数型全局分配器只在本文件测试线程
//! arm 的区间内记账（线程本地开关，同二进制的其它测试不受污染）。
//!
//! 账目口径（合成 DNG 全幅 240×160、ActiveArea 裁到 120×80）：
//! 整数源（rawler 的 u16 全幅）240×160×2 = 76,800 B（解码器侧，
//! 改不掉）；裁剪 f32 mosaic 120×80×4 = 38,400 B（本项唯一应有的
//! 大额产物）；杂项（TIFF 解析／元数据／小 Vec 与缓冲增长）稳态
//! 实测 ≈98.6 KB（新形态总账 213,835 B，盒上实测）；旧形态还会先
//! 整幅转 f32：240×160×4 = 153,600 B，总账 ≈367 KB。 上限取
//! 源 + 裁剪 + 128 KiB：新形态有 ≈32 KB 余量、旧形态超 ≈121 KB
//! ⇒ 变异（恢复全幅 collect）必红。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use yanshi_raw::decode_bytes;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCATED: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ARMED.try_with(|armed| {
            if armed.get() {
                let _ = ALLOCATED.try_with(|total| {
                    total.set(total.get() + layout.size() as u64);
                });
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn arm() {
    ALLOCATED.with(|total| total.set(0));
    ARMED.with(|armed| armed.set(true));
}

fn disarm() -> u64 {
    ARMED.with(|armed| armed.set(false));
    ALLOCATED.with(|total| total.get())
}

#[test]
fn decode_crop_does_not_materialize_full_frame() {
    let (full_w, full_h) = (240u32, 160u32);
    // ActiveArea (top, left, bottom, right)：裁出 120×80。
    let spec = SyntheticDng {
        width: full_w,
        height: full_h,
        mosaic: (0..full_w * full_h).map(|i| (i % 16000) as u16).collect(),
        active_area: Some([40, 60, 120, 180]),
        ..SyntheticDng::default()
    };
    let bytes = write_synthetic_dng(&spec);

    // 预热：rawler 的相机库等进程级数据是懒加载的，首解会一次性
    // 分配数十 MB——那不是逐帧成本，不许混进本项账目（实测首解
    // ≈54 MB、稳态第二解回到逐帧量级，两个数字都记在期 2 备忘里）。
    let _ = decode_bytes(&bytes).expect("预热解码");

    arm();
    let decoded = decode_bytes(&bytes).expect("合成 DNG 必须可解码");
    let allocated = disarm();

    let frame = &decoded.frame;
    assert_eq!((frame.width, frame.height), (120, 80), "裁剪窗应为 120×80");
    // 产物正确性锚点：裁后首像素 = 全幅 (60, 40) 处的源值。
    assert_eq!(frame.mosaic[0], (40 * 240 + 60) as f32 % 16000.0);
    assert_eq!(frame.mosaic.len(), 120 * 80);

    let source_bytes = 240u64 * 160 * 2;
    let crop_bytes = 120u64 * 80 * 4;
    let budget = source_bytes + crop_bytes + 128 * 1024;
    assert!(
        allocated <= budget,
        "解码分配 {allocated} B 超过上限 {budget} B（源 {source_bytes} + 裁剪 {crop_bytes} + 杂项 128 KiB）：\
         全幅 f32（153,600 B）疑似又被物化"
    );
}
