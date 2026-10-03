//! **stored 兜底跨 block 的往返** ✓ —— 用户问的 blob 压缩要靠它 ✓。
//!
//! 这条判据是**先红后绿**的 ✓：deflate 的块头只占 **3 位**（BFINAL 1 + BTYPE 2 ✓）
//! ⇒ 一个字节里还剩 **5 位补齐** ✓。解码端若不当场丢掉 ✗，
//! 就会在**第二个块**上读到旧的补齐位 ⇒ 解出垃圾 ✓
//!（≤65535 字节的**单块**看不出来 ✓ ⇒ 所以这个 bug 一直没暴露 ✓）。
use yanshi_render::png::{zlib_compress_best, zlib_decompress};

/// 不可压的伪随机流（LCG 高位 ✓）⇒ 一定会走 stored 兜底 ✓。
fn noise(size: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..size)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn stored_fallback_round_trips_across_block_boundaries_without_expanding() {
    for size in [0usize, 1, 100, 65_535, 65_536, 65_537, 200_000] {
        let raw = noise(size, 0x1234_5678 ^ (size as u32));
        let packed = zlib_compress_best(&raw);
        let back = zlib_decompress(&packed)
            .unwrap_or_else(|| panic!("compress_best 的输出必须能解回来（size={size}）"));
        assert!(back == raw, "往返必须逐字节一致（size={size}）");
        assert!(
            packed.len() <= raw.len() + 64,
            "不可压数据必须**不膨胀**（size={size}：明文 {} ⇒ {}）",
            raw.len(),
            packed.len()
        );
    }
}
