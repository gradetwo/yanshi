//! **存储编码与内容寻址的不变量** ✓ —— 这三条判据锁住我踩过的那个坑 ✓。
//!
//! 背景（实测 ✓）：包里 91% 是未压缩的原始 RGBA ⇒ 存储时该压 ✓；
//! 但**哈希必须对明文算** ✓。曾经在 server 侧用一个装饰器"先压再交给内层" ✗
//! ⇒ 内层对**压缩流**算哈希 ✗ ⇒ 导入端按明文对账时**对不上** ✗（用户的真实包被拒 ✓）。
//! 所以这里钉住：**哈希来自明文** ✓、**落盘来自编码器** ✓、**解不开必须报错** ✗。
use std::sync::Arc;

use yanshi_core::blob::{BlobCodec, BlobStore, FsBlobStore};
use yanshi_core::BlobHash;

/// 给每个 blob 前面加一个标记字节 ⇒ "落盘 ≠ 明文"**可验证** ✓（长度会 +1 ✓）。
#[derive(Debug)]
struct Marked;

impl BlobCodec for Marked {
    fn encode(&self, plain: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(plain.len() + 1);
        out.push(0xAB);
        out.extend_from_slice(plain);
        out
    }

    fn decode(&self, stored: &[u8]) -> Option<Vec<u8>> {
        match stored.split_first() {
            Some((0xAB, rest)) => Some(rest.to_vec()),
            _ => None,
        }
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("yanshi-blob-codec-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_hash_comes_from_the_plaintext_while_the_file_holds_the_encoded_form() {
    let dir = scratch("hash");
    let store = FsBlobStore::open(&dir)
        .expect("打开")
        .with_codec(Arc::new(Marked));
    let plain = b"the quick brown fox jumps over the lazy dog".repeat(64);

    let hash = store.put(&plain).expect("写入");
    // ① **哈希来自明文** ✓ —— 这正是当初被装饰器破坏的那条 ✓
    assert_eq!(hash, BlobHash::from_bytes(&plain), "哈希必须对**明文**算");
    // ② **落盘确实是编码后的** ✓（标记字节使长度 +1 ✓）
    let entries = store.list().expect("列出");
    let entry = entries
        .iter()
        .find(|item| item.blob_hash == hash)
        .expect("应当有条目");
    assert_eq!(
        entry.size,
        plain.len() as u64 + 1,
        "落盘应当比明文多一个标记字节 ⇒ 编码**确实生效**"
    );
    // ③ **读回来逐字节一致** ✓
    assert!(
        store.get(&hash).expect("读回") == plain,
        "读回必须逐字节一致"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stored_form_the_codec_cannot_read_is_an_error_rather_than_garbage() {
    let dir = scratch("garbage");
    // 先用"原样"写入（文件里就是明文 ✓），再用"标记"解码器去读 ⇒ 必须**报错** ✗
    let hash = {
        let plain_store = FsBlobStore::open(&dir).expect("打开");
        plain_store
            .put(b"written by the plain codec")
            .expect("写入")
    };
    let marked = FsBlobStore::open(&dir)
        .expect("重开")
        .with_codec(Arc::new(Marked));
    assert!(
        marked.get(&hash).is_err(),
        "解码器不认识的内容必须**报错**，绝不返回垃圾"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
