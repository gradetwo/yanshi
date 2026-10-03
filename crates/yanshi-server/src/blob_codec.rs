//! **存储编码的注入实现** ✓：用**仓库自带的 deflate** ✓（`yanshi_render::png` ✓，零新依赖 ✓）。
//!
//! 为什么放在这里而不是 core ✓：core 定义接口（`BlobCodec` ✓）、**不依赖 render** ✓；
//! server 两边都依赖得上 ✓ ⇒ 由它注入最自然 ✓（见 `blob.rs` 的 `BlobCodec` 说明 ✓）。
//! **哈希仍对明文算** ✓（那是 core 的 `put` 做的 ✓）⇒ 内容寻址/去重/导入对账都不受影响 ✓
//! —— 这条不变量由 `crates/yanshi-core/tests/blob_codec.rs` 守着 ✓。
use yanshi_core::blob::BlobCodec;
use yanshi_render::png::{zlib_compress_best, zlib_decompress};

/// 压/解各一次 ✓：**不划算就用 stored 块** ✓（已压过的 PNG/JPEG、噪声都**不会膨胀** ✓）。
#[derive(Debug)]
pub struct RenderCodec;

impl BlobCodec for RenderCodec {
    fn encode(&self, plain: &[u8]) -> Vec<u8> {
        zlib_compress_best(plain)
    }

    fn decode(&self, stored: &[u8]) -> Option<Vec<u8>> {
        zlib_decompress(stored)
    }
}
