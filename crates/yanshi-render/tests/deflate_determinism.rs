//! **确定性判据**：同一份输入 ⇒ 同一串字节 ✓，任何时候、任何平台 ✓。
//!
//! 为什么它是底线 ✗：PNG 导出要**可复现** ✓，blob 存储要**内容寻址** ✓
//!（对明文算哈希 ✓，见 `crates/yanshi-core/tests/blob_codec.rs` ✓），
//! 服务端与浏览器内核之间还要求**逐位一致** ✓ ——
//! 一旦编码器掺进时间/随机/平台相关的分支 ✗，"同一份输入两种输出"就会悄悄发生 ✓。
//!
//! 三条断言，都能红 ✓：
//! ① **同一输入压两次**（每次都是**新构造的编码器** ✓）必须逐字节相同 ✓；
//! ② **显式构造的编码器**（同一配置 ✓）与 [`zlib_compress`] 必须给出同一串字节 ✓
//!    —— 它同时钉住"预留容量不影响输出" ✓（`Vec::with_capacity` 只是预留 ✓）；
//! ③ **黄金摘要** ✓：一份固定语料的输出序列的 sha256 必须是写死的那个值 ✓
//!    ⇒ 后端升级、级别/策略被顺手改、跨平台出现差异 ✓，这条立刻红 ✓。
//!    **这条故意"脆"** ✓：它保护的正是"字节不许悄悄变" ✓ ——
//!    真要升 zlib-rs 并接受新字节时 ✓，必须**显式**改这个常量 ✓（改之前先读上面两行 ✓）。

use std::io::Write;

use flate2::write::ZlibEncoder;
use flate2::Compression;
use yanshi_render::png::{zlib_compress, zlib_compress_best};

/// 探针（**不属于判据** ✓）：本轮跨目标比对用的同一份绘图型数据 ✓。
fn painting(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let band = (y / 64) % 8;
            let r = ((x / 16) as u8).wrapping_add((band * 20) as u8);
            let g = ((y / 16) as u8).wrapping_add((x % 7) as u8);
            let b = if (x / 128 + y / 128) % 2 == 0 {
                200
            } else {
                40
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    rgba
}

fn noise(size: usize, seed: u32) -> Vec<u8> {
    let mut state = seed | 1;
    (0..size)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 24) as u8
        })
        .collect()
}

/// 固定语料 ✓：空 ✓、短 ✓、长重复 ✓、全字节表 ✓、绘图型像素 ✓、不可压噪声 ✓。
fn canonical_corpus() -> Vec<Vec<u8>> {
    vec![
        Vec::new(),
        b"yanshi".to_vec(),
        vec![b'a'; 1000],
        (0..=255u8).collect(),
        painting(64, 40),
        noise(70_000, 0x1234_5678),
    ]
}

/// 每次调用都**新建**一个编码器 ✓（`zlib_compress` 内部就是这么做的 ✓）。
fn encode_with_a_fresh_encoder(raw: &[u8], level: u32) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
    encoder.write_all(raw).expect("写入内存缓冲不会失败");
    encoder.finish().expect("结束 zlib 流不会失败")
}

#[test]
fn the_same_input_yields_the_same_bytes_twice() {
    for raw in canonical_corpus() {
        let first = zlib_compress(&raw);
        let second = zlib_compress(&raw);
        assert!(
            first == second,
            "同一输入两次压缩必须逐字节相同（原始 {} 字节）",
            raw.len()
        );
        let first_best = zlib_compress_best(&raw);
        let second_best = zlib_compress_best(&raw);
        assert!(
            first_best == second_best,
            "compress_best 两次必须逐字节相同（原始 {} 字节）",
            raw.len()
        );
    }
}

#[test]
fn freshly_constructed_compressors_agree_and_the_reserve_does_not_matter() {
    for raw in canonical_corpus() {
        let first = encode_with_a_fresh_encoder(&raw, 7);
        let second = encode_with_a_fresh_encoder(&raw, 7);
        assert!(
            first == second,
            "两个各自新建的同配置编码器必须给出同一串字节（原始 {} 字节）",
            raw.len()
        );
        // **预留容量不得影响输出** ✓：`zlib_compress` 会先 `Vec::with_capacity(...)` ✓，
        // 而上面那两次编码器用的是 `Vec::new()` ✓ —— 字节必须一样 ✓。
        assert!(
            first == zlib_compress(&raw),
            "预留容量/编码器实例不得改变输出（原始 {} 字节）",
            raw.len()
        );
    }
}

/// **黄金摘要** ✓：固定语料的输出序列（带长度前缀 ✓，避免边界歧义 ✓）的 sha256 ✓。
#[test]
fn the_golden_digest_pins_the_backend_bytes() {
    let mut all = Vec::new();
    for item in canonical_corpus() {
        all.extend_from_slice(&(item.len() as u32).to_le_bytes());
        all.extend_from_slice(&zlib_compress_best(&item));
    }
    let digest = yanshi_core::BlobHash::from_bytes(&all).hex().to_owned();
    assert_eq!(
        digest, "514662484df8252bb53cc0f6ce8890f03fa4130935ac6349fc92ea4f8ff82b5e",
        "deflate 输出的字节变了 ⇒ 这是**有意**的改动吗？\
         （换后端/改级别/改策略都会改它 ✓；确认无误后再更新这个常量 ✓，\
         并检查 blob 与 PNG 的既有判据 ✓）"
    );
}
