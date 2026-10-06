//! **判据 1：wasm 量化路径去掉整幅 f32 副本后，输出逐字节不变。**
//!
//! 被judged 的改动在 `crates/yanshi-render/src/render.rs` 的
//! `#[cfg(target_arch = "wasm32")] mod parallel_impl` 里：旧实现先
//! `buffer.crop(&buffer.bbox())` 复制一份整幅 f32，再就地量化、再编码。
//! 新实现（`Buffer::to_rgba8_quantized`）逐行读原始像素、边量化边编码进输出。
//!
//! 那一段在原生上被 `#[cfg]` 掉，**测试二进制跑不到它的函数体**。能跑到的是它现在
//! 唯一调用的**目标无关**实现 `yanshi_render::Buffer::to_rgba8_quantized`
//! （→ `yanshi_render::rows::encode_quantized_rows`），原生并行分支也调用同一份。
//! 所以本判据比较的是 **wasm 分支实际执行的同一份代码**，而不是"另写一套等价实现"。
//!
//! 参照端是旧实现**逐字保留**的测试专用函数 [`copy_reference`]（见其文档）。

use yanshi_render::{quantize_f16, Buffer};

/// 旧 wasm 实现**逐字保留**的测试参照（`#[cfg(test)]` 等价物：只在本测试文件里）。
///
/// 这就是改动前 `render::parallel_impl`（wasm 变体）的 `quantize_to_rgba8` 函数体：
/// 整幅 `crop` 出 f32 副本 → 就地量化到 f16 → 交给 `Buffer::to_rgba8` 编码。
/// **不要**把它当生产实现；它存在的唯一目的是给新路径当字节金标准。
fn copy_reference(buffer: &Buffer, background: Option<[u8; 4]>) -> Vec<u8> {
    let mut quantized = buffer.crop(&buffer.bbox());
    for value in quantized.pixels_mut() {
        *value = quantize_f16(*value);
    }
    quantized.to_rgba8(background)
}

/// 确定性探针缓冲：LCG 填充，覆盖 0 / 1 / 0.5 / 透明 / 半透明与负值、>1 的高光。
fn probe_buffer(width: u32, height: u32, seed: u32) -> Buffer {
    let mut buffer = Buffer::new(0, 0, width, height);
    let mut state = seed | 1;
    for (index, pixel) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((state >> 8) & 0xff) as f32 / 255.0;
        let b = ((state >> 16) & 0xff) as f32 / 255.0;
        let c = ((state >> 24) & 0xff) as f32 / 255.0;
        let alpha = match index % 5 {
            0 => 0.0,
            1 => 1.0,
            2 => 0.5,
            3 => a,
            _ => 1.0 - a,
        };
        pixel.copy_from_slice(&[b * 1.2 - 0.1, c, a * 0.5, alpha]);
    }
    buffer
}

/// f16 量化会真正改变数值的极端输入：极小/极大/次正规/负零/精确可表示值。
fn edge_value_buffer() -> Buffer {
    const VALUES: [f32; 12] = [
        0.0,
        -0.0,
        1.0,
        0.5,
        1.0 / 3.0,
        f32::MIN_POSITIVE,
        1.0e-8,
        6.103_515_6e-5,
        65504.0,
        1.0e9,
        -0.25,
        2.5,
    ];
    let mut buffer = Buffer::new(0, 0, 12, 4);
    let pixels = buffer.pixels_mut();
    for (index, value) in pixels.iter_mut().enumerate() {
        *value = VALUES[(index + index / 4) % VALUES.len()];
    }
    buffer
}

/// 逐字节比较；失败只报**第一处不同**（MB 级输出不能整块打进日志）。
fn assert_bytes_identical(label: &str, left: &[u8], right: &[u8]) {
    assert_eq!(left.len(), right.len(), "{label}：字节数不同");
    if let Some(index) = left.iter().zip(right).position(|(a, b)| a != b) {
        panic!(
            "{label}：第 {index} 字节不同（新 {} ≠ 旧 {}）",
            left[index], right[index]
        );
    }
}

/// 形状覆盖：空、单像素、只有一行、只有一列、含前导空行（行长不整除 4）、
/// 多行非方形、稍大的图（几百 KB，足以跨多行/多缓存行）。
#[test]
fn no_copy_path_is_byte_identical_to_the_copy_reference() {
    let shapes = [
        (0u32, 0u32),
        (1, 1),
        (1, 7),
        (7, 1),
        (3, 2),
        (5, 4),
        (64, 64),
        (257, 129),
        (300, 200),
    ];
    for (width, height) in shapes {
        let buffer = probe_buffer(width, height, width.wrapping_mul(31).wrapping_add(height));
        for background in [
            None,
            Some([255u8, 255, 255, 255]),
            Some([12, 200, 90, 255]),
            Some([0, 0, 0, 0]),
        ] {
            let new = buffer.to_rgba8_quantized(background);
            let old = copy_reference(&buffer, background);
            assert_eq!(new.len(), width as usize * height as usize * 4);
            assert_bytes_identical(
                &format!("{width}×{height} bg={background:?}：新路径 vs 旧复制参照"),
                &new,
                &old,
            );
        }
    }
}

/// 极端数值也必须逐字节相同（量化的舍入/溢出路径都被走到）。
#[test]
fn edge_values_are_byte_identical_to_the_copy_reference() {
    let buffer = edge_value_buffer();
    for background in [None, Some([255u8, 255, 255, 255]), Some([7, 8, 9, 255])] {
        let new = buffer.to_rgba8_quantized(background);
        let old = copy_reference(&buffer, background);
        assert_bytes_identical(
            &format!("极端值 bg={background:?}：新路径 vs 旧复制参照"),
            &new,
            &old,
        );
    }
}
