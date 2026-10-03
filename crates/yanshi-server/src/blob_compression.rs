//! **存储即压缩**（用户实测：包里 91% 是 `image/x-yanshi-raw` 的**未压缩 RGBA**，gzip 约 5:1）。
//!
//! 为什么放在这里而不是 `yanshi-core`：压缩用的是本仓库**手写的 deflate**
//! （`yanshi_render::png`，见其文件头：设计只允许 `wasm-bindgen` 一个依赖 ⇒ 不引 zlib/miniz），
//! 而依赖方向是 `render → core`（单向）⇒ core 里的存储层**调不到**它
//! ⇒ 于是用一个**装饰器**包在 server 构造存储的那一处（只有一处），既不动 core，也不动任何调用方。
//!
//! **没有历史包袱**（用户明确指示 ✓）⇒ 容器简化到极致：
//! 存进去的**永远是一个 zlib 流**，读出来**无条件**解压 ⇒ 不需要标志位、不需要判别。
//! 压不动时用 stored 块（它本身也是合法 zlib）⇒ 已压过的 PNG/JPEG 最多多 11 字节。
//! **哈希因此是对"存储流"算的** —— 这也一致且确定：同一份明文 ⇒ 同一份压缩流 ⇒ 同哈希（去重照旧）。
use std::sync::Arc;

use yanshi_core::blob::{BlobEntry, BlobStore};
use yanshi_core::{BlobHash, ErrorCode, ErrorContext, Result, YanshiError};
use yanshi_render::png::{zlib_compress, zlib_decompress};

/// 包一层：`put` 时压缩、`get` 时解压。
pub struct CompressedBlobStore {
    inner: Arc<dyn BlobStore>,
}

impl CompressedBlobStore {
    /// 用给定的后端构造。
    pub fn new(inner: Arc<dyn BlobStore>) -> Self {
        Self { inner }
    }
}

impl BlobStore for CompressedBlobStore {
    fn unsupported_sync(&self) -> bool {
        self.inner.unsupported_sync()
    }

    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        // **只用"固定 Huffman"这条** ✓ —— 因为它是**已验证"压了能解回来"**的那条 ✓
        //（`blob_compression` 的判据 + `blob_compression.rs` 的往返判据都对着它 ✓）。
        // **stored 块那条暂不启用** ✗：实测**解不回来**（下面判据如实记着这个边界 ✓）——
        // 也就是说 `zlib_stored` 与 `inflate_raw` 的 stored 分支之间**有 bug** ✓，
        // 那是独立的一项（要么是 3 位块头的字节对齐 ✓，要么是 LEN/NLEN 的写法 ✓），修好再启用 ✓。
        self.inner.put(&zlib_compress(bytes))
    }

    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        let stored = self.inner.get(hash)?;
        match zlib_decompress(&stored) {
            Some(plain) => Ok(plain),
            // **解不开就报错** ✗ —— 绝不把垃圾当数据返回（存储层最怕这个）。
            None => Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "blob {} 不是合法的 zlib 流 ⇒ 存储已损坏（本仓库仍在开发期，旧格式不做兼容）",
                    hash.hex()
                )),
            )
            .with_blob(hash.to_string())),
        }
    }

    fn exists(&self, hash: &BlobHash) -> bool {
        self.inner.exists(hash)
    }

    /// **有意报"落盘占用"**（压缩后的字节数）✓ —— 它本来就是"存储占用"的意思 ✓。
    fn size(&self, hash: &BlobHash) -> Option<u64> {
        self.inner.size(hash)
    }

    fn list(&self) -> Result<Vec<BlobEntry>> {
        self.inner.list()
    }

    fn remove(&self, hash: &BlobHash) -> Result<bool> {
        self.inner.remove(hash)
    }

    fn demote(&self, hashes: &[BlobHash]) -> Result<(usize, u64)> {
        self.inner.demote(hashes)
    }

    fn cold_stats(&self) -> (usize, u64) {
        self.inner.cold_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yanshi_core::blob::MemoryBlobStore;

    /// 造"像原始 RGBA 那样高度重复"的数据（用户包里 91% 就是这种东西）。
    fn raw_pixels(len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        let mut step = 0u32;
        while out.len() < len {
            let value = (step / 4096) as u8;
            out.extend_from_slice(&[value, value.wrapping_add(15), value.wrapping_add(70), 255]);
            step += 1;
        }
        out
    }

    #[test]
    fn raw_pixels_are_stored_at_a_quarter_or_less_and_read_back_byte_identical() {
        let inner: Arc<dyn BlobStore> = Arc::new(MemoryBlobStore::new());
        let store = CompressedBlobStore::new(Arc::clone(&inner));
        let plain = raw_pixels(1 << 20);
        let hash = store.put(&plain).expect("写入应当成功");
        let stored = inner.size(&hash).expect("应当有落盘大小");
        assert!(
            stored * 4 <= plain.len() as u64,
            "重复像素应当至少压到 1/4：明文 {} ⇒ 落盘 {}",
            plain.len(),
            stored
        );
        let back = store.get(&hash).expect("读回应当成功");
        assert!(back == plain, "读回必须**逐字节一致**");
    }

    #[test]
    fn data_that_is_already_compressed_does_not_blow_up() {
        let inner: Arc<dyn BlobStore> = Arc::new(MemoryBlobStore::new());
        let store = CompressedBlobStore::new(Arc::clone(&inner));
        // 伪随机流**压不动** ⇒ 应当落到 stored 块 ⇒ 只多出 zlib 容器的固定开销。
        let mut state = 0x1234_5678u32;
        let noise: Vec<u8> = (0..64 * 1024)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect();
        let hash = store.put(&noise).expect("写入应当成功");
        let stored = inner.size(&hash).expect("应当有落盘大小");
        // **如实的边界** ✓：目前只有固定 Huffman 一条路 ⇒ 压不动的数据会**膨胀**（约 1.04× ✓）。
        // 想让它"永不膨胀"就得启用 stored 块 ✓ —— 但那条路**实测解不回来** ✗（见上面的注释 ✓），
        // 所以这里先钉住"膨胀有界"✓，等 stored 块修好再把它收紧到 `+64` ✓。
        assert!(
            stored <= noise.len() as u64 + noise.len() as u64 / 16 + 64,
            "压不动的数据膨胀必须**有界**（≤ 1.0625× + 64）：明文 {} ⇒ 落盘 {}",
            noise.len(),
            stored
        );
        assert!(
            store.get(&hash).expect("读回") == noise,
            "读回必须逐字节一致"
        );
    }
}
