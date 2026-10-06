//! **往返判据（新编码 → 新解码）**：压出来的每一个字节都必须**逐字节**解回原样 ✓。
//!
//! 与既有判据的分工 ✓（**不新写第三套 harness** ✓，只补语料的**边界** ✓）：
//! ① `stored_block_round_trip.rs`：stored 兜底跨 65535 块边界 ✓；
//! ② `blob_compression.rs`：可压性 + 拒绝非 zlib + adler32 抓篡改 ✓；
//! ③ 本文件：**长度与内容形态的边界表** ✓ —— 0/1/2/3 字节 ✓、LZ77 最小匹配 3 与最大 258 附近 ✓、
//!    deflate 块边界附近 ✓、全零 ✓、重复 ✓、噪声 ✓、以及"半压缩"（前一半噪声后一半平色 ✓）。
//!
//! **能红** ✓：把 `src/png.rs` 的解码侧改成"忽略错误、返回已解出的前缀" ✗
//! ⇒ 截断/篡改那几条当场失败 ✓（变异验证见提交信息 ✓）。

use yanshi_render::png::{zlib_compress, zlib_compress_best, zlib_decompress};

fn noise(size: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..size)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

/// 半压缩：前半噪声（走 stored 或长距离码 ✓）、后半大片平色 ✓。
fn half_and_half(size: usize) -> Vec<u8> {
    let mut out = noise(size / 2, 0x0BAD_F00D);
    out.resize(size, 200u8);
    out
}

/// 语料的**长度边界** ✓：0 ✓、LZ 最小匹配 3 的两侧 ✓、最长匹配 258 的两侧 ✓、
/// stored 块边界 65535 的两侧 ✓、以及一个 200 000 字节的大输入 ✓。
const LENGTHS: [usize; 16] = [
    0, 1, 2, 3, 4, 257, 258, 259, 4096, 32_767, 32_768, 65_534, 65_535, 65_536, 65_537, 200_000,
];

#[test]
fn every_shape_round_trips_byte_for_byte() {
    for length in LENGTHS {
        for (label, raw) in [
            ("全零", vec![0u8; length]),
            ("重复", vec![0xABu8; length]),
            ("噪声", noise(length, 0x1234_5678 ^ (length as u32))),
            ("半压缩", half_and_half(length)),
            (
                "递增",
                (0..length)
                    .map(|index| (index as u64).wrapping_mul(2_654_435_761) as u8)
                    .collect(),
            ),
        ] {
            let packed = zlib_compress(&raw);
            let restored = zlib_decompress(&packed).unwrap_or_else(|| {
                panic!("compress 的输出必须能解回来（{label} / {length} 字节）")
            });
            assert!(
                restored == raw,
                "compress 往返必须逐字节一致（{label} / {length} 字节）"
            );

            let packed_best = zlib_compress_best(&raw);
            let restored_best = zlib_decompress(&packed_best).unwrap_or_else(|| {
                panic!("compress_best 的输出必须能解回来（{label} / {length} 字节）")
            });
            assert!(
                restored_best == raw,
                "compress_best 往返必须逐字节一致（{label} / {length} 字节）"
            );
        }
    }
}

/// **坏流必须回 `None`** ✓：截断 ✓、adler32 被改 ✓、zlib 头被改 ✓ —— 三条都不得"猜出半个结果" ✗。
#[test]
fn corrupted_streams_are_refused_instead_of_decoded_partially() {
    let raw = half_and_half(20_000);
    let packed = zlib_compress(&raw);
    assert!(zlib_decompress(&packed).is_some(), "干净流必须解得出");

    // ① 截断（丢掉最后 1 / 4 / 全部 adler32 字节 ✓）。
    for cut in [1usize, 4, 8] {
        let truncated = &packed[..packed.len() - cut];
        assert!(
            zlib_decompress(truncated).is_none(),
            "截断 {cut} 字节必须回 None"
        );
    }
    // ② 改 adler32（最后一个字节翻转 ✓）。
    let mut tampered_adler = packed.clone();
    let last = tampered_adler.len() - 1;
    tampered_adler[last] ^= 0xFF;
    assert!(
        zlib_decompress(&tampered_adler).is_none(),
        "adler32 被改必须回 None"
    );
    // ③ 改 zlib 头（CMF 不再是 0x78 ✓）。
    let mut tampered_header = packed.clone();
    tampered_header[0] = 0x79;
    assert!(
        zlib_decompress(&tampered_header).is_none(),
        "zlib 头被改必须回 None"
    );
}
