//! **向后兼容判据**：`flate2`/`zlib-rs` 必须能读**旧手写编码器**已经写下的每一个字节 ✓。
//!
//! 为什么必须有它 ✗：本轮把**编码器与解码器一起**换成了第三方 crate ✓。
//! 已经落盘的 blob（本地 CAS ✓、用户导出的工程包 `blobs.encoding: "zlib"` ✓）
//! 与旧版本导出的 PNG ✓ 都是**手写编码器**的产物 ✓ ——
//! 若新解码器读不了它们 ✓，"打开旧工程"就会变成"文件坏了" ✗，
//! 而且**很难回头**（用户的字节已经在盘上 ✓）。
//!
//! 夹具是**旧实现**当场生成的 ✓（`crates/yanshi-render/examples/deflate_probe.rs`
//! 的 `fixtures` 模式 ✓，在换实现**之前**跑 ✓）：
//! ① 固定 Huffman 流（`zlib_compress` 的产物 ✓）；
//! ② stored 兜底流、且**跨 65535 字节块边界** ✓（`zlib_compress_best` 对噪声的产物 ✓）；
//! ③ 一张旧编码器产出的 PNG ✓。
//! **判据能红** ✓：把 `zlib_decompress` 换回"只看头就返回输入"之类的假解码 ✓，
//! 或把解码器换成只认某一种块类型的实现 ✗ ⇒ 三条断言当场失败 ✓（变异验证见提交信息 ✓）。

use yanshi_render::png::{decode_png, zlib_decompress};

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("读取夹具 {path:?} 失败：{error}"))
}

/// ① **旧编码器的固定 Huffman 流** ✓（8 KiB 绘画型数据 ⇒ 195 字节 ✓，几乎必然是固定 Huffman 块 ✓）。
#[test]
fn legacy_fixed_huffman_blobs_still_decode() {
    let packed = fixture("legacy_fixed_painting.zlib");
    let restored = zlib_decompress(&packed).expect("旧手写编码器的 zlib 流必须仍能解开");
    assert!(
        restored == fixture("legacy_fixed_painting.raw"),
        "解出来必须与旧编码器的输入逐字节一致"
    );
}

/// ② **旧编码器的 stored 兜底、跨块边界** ✓（70 016 字节 ⇒ 两个 stored 块 ✓）。
#[test]
fn legacy_stored_fallback_blobs_still_decode() {
    let packed = fixture("legacy_stored_multi_block.zlib");
    let restored = zlib_decompress(&packed).expect("旧的 stored 流必须仍能解开");
    assert!(
        restored == fixture("legacy_stored_multi_block.raw"),
        "跨 65535 块边界的旧 blob 必须逐字节解回"
    );
}

/// ③ **旧编码器产出的 PNG** ✓（IDAT 是旧的固定 Huffman 流 ✓）。
#[test]
fn legacy_encoded_pngs_still_decode() {
    let bytes = fixture("legacy_encoded_64x40.png");
    let (width, height, rgba) = decode_png(&bytes).expect("旧编码器产出的 PNG 必须仍能解开");
    assert_eq!((width, height), (64, 40));
    assert!(
        rgba == fixture("legacy_encoded_64x40.raw"),
        "旧 PNG 的像素必须逐字节一致"
    );
}
