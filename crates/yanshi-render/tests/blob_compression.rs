//! **blob 压缩原语**的判据（用户问"blobs 存储时压缩是不是最好" ⇒ 先把这个原语锁住 ✓）。
//!
//! 三条断言都**能红** ✓：
//! ① 可压的数据必须**真的压得动**（≤ 明文 25% ✓）—— 若哪天退回"stored（未压缩）块"就立刻红 ✓；
//! ② **往返逐字节一致** ✓ —— 存储层的底线 ✓；
//! ③ 不是 zlib 流的输入必须被**拒绝**（回 `None` ✓）—— 免得把垃圾当真数据解出来 ✗。
use yanshi_render::png::{zlib_compress, zlib_decompress};

/// 造一段"像原始 RGBA 那样高度重复"的数据（用户包里 91% 就是这种东西 ✓）。
fn raw_pixels(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut step = 0u32;
    while out.len() < len {
        // 每 4 字节一个像素，且**大块重复**（真实笔触像素就是这样 ✓）
        let value = (step / 4096) as u8;
        out.extend_from_slice(&[value, value.wrapping_add(15), value.wrapping_add(70), 255]);
        step += 1;
    }
    out
}

#[test]
fn compressible_bytes_shrink_a_lot_and_round_trip_exactly() {
    let raw = raw_pixels(1 << 20); // 1 MiB
    let packed = zlib_compress(&raw);
    assert!(
        packed.len() * 4 <= raw.len(),
        "可压数据应当至少压到 1/4：明文 {} 字节 ⇒ 压缩后 {} 字节",
        raw.len(),
        packed.len()
    );
    let back = zlib_decompress(&packed).expect("自己压的必须能解回来");
    assert_eq!(back.len(), raw.len(), "解压后长度必须一致");
    assert!(back == raw, "解压后必须**逐字节一致**");
}

#[test]
fn a_stream_that_is_not_zlib_is_rejected_instead_of_guessed() {
    // 任意二进制不该被当成 zlib 解出来 ✗
    assert!(
        zlib_decompress(&[0u8; 64]).is_none(),
        "全零不是 zlib 流，必须回 None"
    );
    assert!(
        zlib_decompress(b"not compressed at all").is_none(),
        "文本不是 zlib 流"
    );
    assert!(zlib_decompress(&[]).is_none(), "空输入必须回 None");
    // 头部对、但载荷被改坏 ⇒ adler32 校验应当抓住 ✓
    let mut tampered = zlib_compress(&raw_pixels(4096));
    let last = tampered.len() - 1;
    tampered[last] ^= 0xff;
    assert!(
        zlib_decompress(&tampered).is_none(),
        "载荷被改坏必须被 adler32 抓住"
    );
}
