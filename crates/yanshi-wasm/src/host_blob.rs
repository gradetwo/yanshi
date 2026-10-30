//! WASM 端 BlobStore 外部化到宿主（bake 二期第 4 项）。
//!
//! **问题**：一期以前，WASM 内核在自己的线性内存里持有一份完整的
//! [`yanshi_core::blob::MemoryBlobStore`]（`Kernel.store` 字段）：压平基底 PNG
//! 这类大字节块与 tile 缓存抢同一片 WASM 线性内存，且宿主（页面）无法接管
//! 它们的生命周期与持久化（页面侧已有的 OPFS + IndexedDB 缓存是**异步**的，
//! 而渲染器的 `BlobStore` trait 是**同步**的，无法直接当后端）。
//!
//! **本模块的边界**：
//!
//! - `Kernel` 不再直接拥有字节：它只持有一个 [`HostBlobStore`]，后者实现
//!   [`yanshi_core::blob::BlobStore`]（渲染器看到的仍是同一 trait，
//!   `yanshi-render` 与 `yanshi-core` 一字不动，Native 的 `FsBlobStore` 路径不动）。
//! - [`HostBlobStore`] 只持有一个 [`HostBlobBackend`]（宿主侧实现）：
//!   `wasm32` 目标下是 JS 宿主函数（见本文件底部的 `inline_js`）；
//!   非 `wasm32` 目标下是进程内 `HashMap` 替身（仅供 Native 测试与宿主占位，
//!   该后端在 `wasm32` 编译时不存在）。
//! - **同步约束**：宿主接口必须是同步的（渲染路径不能 `await`，否则单线程
//!   事件循环死锁，见 `kernel.rs` 的 GPU 异步路注释）。页面要接 OPFS /
//!   IndexedDB 时，做法是**异步取回后经 `WasmKernel::blob_put` 推入同步宿主
//!   store**（或把一个同步缓存实现装到 `globalThis.__yanshiHostBlobStore` 上），
//!   而不是让同步路径去等异步存储。
//!
//! **接口（put / get / has / size / list / remove / count）**：
//!
//! | Rust 侧（[`HostBlobBackend`]） | JS 宿主函数 | 说明 |
//! |---|---|---|
//! | `put_bytes(hash, bytes)` | `__yanshi_host_blob_put(hash, bytes)` | 键由 Rust 侧算出，宿主不自行算哈希 |
//! | `get_bytes(hash)` | `__yanshi_host_blob_get(hash)` | 仅 `has` 为真时调；缺失返回空数组作防御 |
//! | `has(hash)` | `__yanshi_host_blob_has(hash)` | 缺失判据，渲染严格报错的前置 |
//! | `size(hash)` | `__yanshi_host_blob_size(hash)` | 缺失返回 `-1`；不取全量字节 |
//! | `list_hashes()` | `__yanshi_host_blob_list()` | JSON 字符串数组 |
//! | `remove(hash)` | `__yanshi_host_blob_remove(hash)` | 返回是否真的删掉了 |
//! | `count()` | `__yanshi_host_blob_count()` | 条数（`KernelStats.blobs` 的来源） |
//!
//! **哈希校验落点（只在 Rust 侧，不在宿主侧）**：
//!
//! - `put`：Rust 侧用 [`yanshi_core::BlobHash::from_bytes`] 算哈希，
//!   以该哈希为键交给宿主；宿主按给定键原样存字节。宿主不算哈希，
//!   避免第二份哈希实现漂移；幂等去重语义与 `MemoryBlobStore` / `FsBlobStore` 一致。
//! - `get`：先 `has` 判缺（缺失报 `reference_not_found`，与
//!   `MemoryBlobStore` / `FsBlobStore` 的缺失口径一致）；取回后 Rust 侧
//!   重算 SHA-256 并与请求哈希比对，不一致报 `precondition_failed`
//!   （与 bake 重放哈希不一致 `hash_mismatch_error` 同族），**不按缺失处理**。
//!   注意：渲染器的 `BakedBase` 分支会把一切 `store.get` 失败都折成
//!   `missing_baked_blob`（一期现成行为，服务端同款折叠，本项不动渲染器），
//!   因此校验失败在渲染信封里表现为 `missing_baked_blob`，而在
//!   `Kernel::blob_get` 直读路径里表现为 `precondition_failed`（可区分）。
//! - `put` 不碰时钟：宿主侧拥有条目的生命周期（`created_at` 一律记 `0`，
//!   与一期 `blob_put` 走 `put_at(bytes, 0)` 的口径一致；内核不做 GC）。
//!   这同时去掉了浏览器里 `blob_put` 曾因宿主时钟而 `unreachable` 的隐患。
//!
//! **缺 Blob 的错误形态（与 Native 口径对齐）**：
//!
//! - 渲染缺压平基底：`yanshi_render` 的 `BakedBase` 分支报
//!   `yanshi_core::bake::missing_baked_blob_error`（`error_code =
//!   missing_baked_blob`，含 `layer` / `blob` / `bake_seq` /
//!   `replay_recoverable=true`）；`Kernel` 的渲染路把该码原样带进信封
//!   （见 `kernel.rs` 的 `render_error`），不再像一期那样一律折成
//!   `invalid_argument`（JS 侧此前只能从 `detail` 字符串里抠码）。
//! - 服务端独有的 Undo 拦截码 `MISSING_HISTORICAL_BLOB_ON_UNDO` 是
//!   服务端 Undo 路径的 `extra`，WASM 渲染路径不产生它（WASM 没有 Undo
//!   工具入口），本项不对齐也不伪造它。
//!
//! **内存与行为变化（WASM 目标下）**：
//!
//! - 字节不再进 WASM 线性内存：默认宿主实现是一个 JS 堆内的 `Map`
//!   （`inline_js` 内定义），WASM 模块不为 blob 字节增长内存；
//!   页面总内存不变（字节只是换了个堆），收益是宿主拥有生命周期，
//!   且后续可把后端换成同步缓存而不重编 WASM。
//! - 默认宿主 `Map` 是页面全局的（内容寻址，跨文档共享，与服务端
//!   CAS 跨文档共享同一口径）；`KernelStats.blobs` 在 WASM 下因此是
//!   宿主 store 的总条数，不再是单个内核的私有条数（Native 替身后端
//!   仍是每 `HostBlobStore` 一份，行为与一期一致）。
//! - 宿主可替换：页面把完整实现（`put` / `has` / `get` / `size` /
//!   `list` / `remove` / `count` 七个方法齐全）赋给
//!   `globalThis.__yanshiHostBlobStore`，每次调用都会先查它；
//!   方法不齐的对象会被忽略并只告警一次（绝不半委托，避免“写进一处、
//!   读在另一处”的静默分裂）。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use yanshi_core::blob::{BlobEntry, BlobStore};
use yanshi_core::error::{ErrorCode, ErrorContext, Result, YanshiError};
use yanshi_core::BlobHash;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// 宿主侧 blob 后端：`HostBlobStore` 与宿主之间的唯一边界。
///
/// 方法全部是同步的（见模块文档的同步约束）。`hash` 形如
/// `"sha256:<hex>"`（[`BlobHash`] 的字符串形），宿主按该字符串原样作键。
pub trait HostBlobBackend: Send + Sync {
    /// 按给定哈希键存字节（幂等：同键重复存入不产生第二份）。
    fn put_bytes(&self, hash: &str, bytes: &[u8]);
    /// 按哈希取字节；缺失返回 `None`（不报错，缺失由调用方定性）。
    fn get_bytes(&self, hash: &str) -> Option<Vec<u8>>;
    /// 是否存在该哈希。
    fn has(&self, hash: &str) -> bool;
    /// 字节数；缺失返回 `None`（不取全量字节）。
    fn size(&self, hash: &str) -> Option<u64>;
    /// 全部哈希键（排序不作保证，调用方如需有序自行排序）。
    fn list_hashes(&self) -> Vec<String>;
    /// 删除；返回是否真的删掉了一份。
    fn remove(&self, hash: &str) -> bool;
    /// 条数。
    fn count(&self) -> usize;
}

/// 宿主侧 blob 存储：实现 [`BlobStore`]，字节只活在宿主后端里。
///
/// `wasm32` 下后端是 JS 宿主函数；其余目标下后端是
/// [`MemoryHostBackend`]（Native 测试替身，见 [`HostBlobStore::new`]）。
pub struct HostBlobStore {
    backend: Arc<dyn HostBlobBackend>,
}

impl HostBlobStore {
    /// 新建：`wasm32` 用 JS 宿主后端，其余目标用进程内替身后端。
    pub fn new() -> Self {
        #[cfg(target_arch = "wasm32")]
        let backend: Arc<dyn HostBlobBackend> = Arc::new(JsHostBackend);
        #[cfg(not(target_arch = "wasm32"))]
        let backend: Arc<dyn HostBlobBackend> = Arc::new(MemoryHostBackend::default());
        Self { backend }
    }

    /// 注入指定后端（测试故障注入与嵌入宿主用；`Kernel::new` 仍走 [`HostBlobStore::new`]）。
    pub fn with_backend(backend: Arc<dyn HostBlobBackend>) -> Self {
        Self { backend }
    }

    /// 条数（宿主后端 `count` 的直通；`KernelStats.blobs` 的来源）。
    pub fn len(&self) -> usize {
        self.backend.count()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 后端名（可观测性用：`wasm32` 为 `"host_js"`，其余目标为
    /// `"host_memory_backend"`，即 Native 测试替身，不是 JS 宿主）。
    pub fn backend_name(&self) -> &'static str {
        #[cfg(target_arch = "wasm32")]
        return "host_js";
        #[cfg(not(target_arch = "wasm32"))]
        return "host_memory_backend";
    }
}

impl Default for HostBlobStore {
    fn default() -> Self {
        Self::new()
    }
}

fn missing_blob(hash: &BlobHash) -> YanshiError {
    YanshiError::new(
        ErrorCode::ReferenceNotFound,
        ErrorContext::detail(format!("blob {hash} 不存在")),
    )
    .with_blob(hash.to_string())
}

impl BlobStore for HostBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        // 哈希只在 Rust 侧算（见模块文档的校验落点）；宿主不算哈希。
        let hash = BlobHash::from_bytes(bytes);
        self.backend.put_bytes(hash.as_str(), bytes);
        Ok(hash)
    }

    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        if !self.backend.has(hash.as_str()) {
            return Err(missing_blob(hash));
        }
        let bytes = self
            .backend
            .get_bytes(hash.as_str())
            .ok_or_else(|| missing_blob(hash))?;
        // 宿主返回内容必须与请求哈希一致；不一致是损坏，不是缺失。
        let actual = BlobHash::from_bytes(&bytes);
        if actual != *hash {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "宿主返回内容与哈希不符：请求 {hash}，实得 {actual}（不按缺失处理）"
                )),
            )
            .with_blob(hash.to_string()));
        }
        Ok(bytes)
    }

    fn exists(&self, hash: &BlobHash) -> bool {
        self.backend.has(hash.as_str())
    }

    fn size(&self, hash: &BlobHash) -> Option<u64> {
        self.backend.size(hash.as_str())
    }

    fn list(&self) -> Result<Vec<BlobEntry>> {
        let mut entries = Vec::new();
        for hash_str in self.backend.list_hashes() {
            let Ok(hash) = hash_str.parse::<BlobHash>() else {
                continue;
            };
            entries.push(BlobEntry {
                size: self.backend.size(&hash_str).unwrap_or(0),
                created_at: 0,
                blob_hash: hash,
            });
        }
        entries.sort_by(|left, right| left.blob_hash.cmp(&right.blob_hash));
        Ok(entries)
    }

    fn remove(&self, hash: &BlobHash) -> Result<bool> {
        Ok(self.backend.remove(hash.as_str()))
    }
}

/// 非 `wasm32` 目标的进程内替身后端：仅供 Native 测试与宿主占位。
///
/// 它在 `wasm32` 编译时不存在（`HostBlobStore::new` 在该目标下只用
/// [`JsHostBackend`]），因此不构成“WASM 侧仍内含完整 store”的回归。
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
pub struct MemoryHostBackend {
    entries: Mutex<BTreeMap<String, Vec<u8>>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl HostBlobBackend for MemoryHostBackend {
    fn put_bytes(&self, hash: &str, bytes: &[u8]) {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .insert(hash.to_owned(), bytes.to_vec());
    }

    fn get_bytes(&self, hash: &str) -> Option<Vec<u8>> {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .get(hash)
            .cloned()
    }

    fn has(&self, hash: &str) -> bool {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .contains_key(hash)
    }

    fn size(&self, hash: &str) -> Option<u64> {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .get(hash)
            .map(|bytes| bytes.len() as u64)
    }

    fn list_hashes(&self) -> Vec<String> {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .keys()
            .cloned()
            .collect()
    }

    fn remove(&self, hash: &str) -> bool {
        self.entries
            .lock()
            .expect("宿主替身后端锁未中毒")
            .remove(hash)
            .is_some()
    }

    fn count(&self) -> usize {
        self.entries.lock().expect("宿主替身后端锁未中毒").len()
    }
}

/// `wasm32` 目标的 JS 宿主后端：字节活在 JS 堆里，不进 WASM 线性内存。
///
/// JS 侧实现见下面的 `inline_js`（默认 `Map` + `globalThis.__yanshiHostBlobStore`
/// 完整替换 hook；沿用 `lib.rs` 里 `navigator.gpu` 探测的零新依赖先例，
/// 不引入 `js-sys` / `web-sys`）。
#[cfg(target_arch = "wasm32")]
pub struct JsHostBackend;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(inline_js = r#"
// Host blob store (see host_blob.rs): bytes live in the JS heap, never in
// WASM linear memory. Default backend is one Map keyed by content hash.
// A page may install a full replacement by assigning
// globalThis.__yanshiHostBlobStore = { put, has, get, size, list, remove, count };
// an incomplete object is ignored with a single console warning (never
// half-delegated: a put into one store must never be read from another).
let __yanshi_host_blob_map = null;
let __yanshi_host_blob_warned = false;
function __yanshi_host_blob_default_map() {
  if (__yanshi_host_blob_map === null) { __yanshi_host_blob_map = new Map(); }
  return __yanshi_host_blob_map;
}
function __yanshi_host_blob_custom() {
  try {
    const s = globalThis.__yanshiHostBlobStore;
    if (!s) return null;
    const methods = ['put', 'has', 'get', 'size', 'list', 'remove', 'count'];
    for (const m of methods) {
      if (typeof s[m] !== 'function') {
        if (!__yanshi_host_blob_warned) {
          __yanshi_host_blob_warned = true;
          try { console.warn('[yanshi] __yanshiHostBlobStore incomplete (missing ' + m + '); using default Map store'); } catch (_) {}
        }
        return null;
      }
    }
    return s;
  } catch (_) { return null; }
}
export function __yanshi_host_blob_put(hash, bytes) {
  // Copy: the incoming view aliases WASM linear memory, which may grow.
  const copy = bytes.slice();
  const s = __yanshi_host_blob_custom();
  if (s) { s.put(hash, copy); return; }
  __yanshi_host_blob_default_map().set(hash, copy);
}
export function __yanshi_host_blob_has(hash) {
  const s = __yanshi_host_blob_custom();
  if (s) return !!s.has(hash);
  return __yanshi_host_blob_default_map().has(hash);
}
export function __yanshi_host_blob_get(hash) {
  // Only called when has() is true (see HostBlobStore::get); an empty
  // array here is defensive and fails Rust-side hash verification unless
  // the requested hash really is the empty blob's hash.
  const s = __yanshi_host_blob_custom();
  if (s) { const v = s.get(hash); return (v instanceof Uint8Array) ? v : new Uint8Array(0); }
  const v = __yanshi_host_blob_default_map().get(hash);
  return v ? v : new Uint8Array(0);
}
export function __yanshi_host_blob_size(hash) {
  const s = __yanshi_host_blob_custom();
  if (s) { const n = Number(s.size(hash)); return Number.isFinite(n) ? n : -1; }
  const v = __yanshi_host_blob_default_map().get(hash);
  return v ? v.byteLength : -1;
}
export function __yanshi_host_blob_list() {
  const s = __yanshi_host_blob_custom();
  if (s) { const v = s.list(); return JSON.stringify(Array.isArray(v) ? v : []); }
  return JSON.stringify(Array.from(__yanshi_host_blob_default_map().keys()));
}
export function __yanshi_host_blob_remove(hash) {
  const s = __yanshi_host_blob_custom();
  if (s) return !!s.remove(hash);
  return __yanshi_host_blob_default_map().delete(hash);
}
export function __yanshi_host_blob_count() {
  const s = __yanshi_host_blob_custom();
  if (s) { const n = Number(s.count()); return Number.isFinite(n) ? n : 0; }
  return __yanshi_host_blob_default_map().size;
}
"#)]
extern "C" {
    /// 宿主存入（JS 侧必须拷贝入参视图，见 `inline_js` 注释）。
    #[wasm_bindgen(js_name = __yanshi_host_blob_put)]
    fn js_host_blob_put(hash: &str, bytes: &[u8]);
    /// 宿主是否存在该哈希。
    #[wasm_bindgen(js_name = __yanshi_host_blob_has)]
    fn js_host_blob_has(hash: &str) -> bool;
    /// 宿主取字节（仅 `has` 为真时调用）。
    #[wasm_bindgen(js_name = __yanshi_host_blob_get)]
    fn js_host_blob_get(hash: &str) -> Vec<u8>;
    /// 宿主字节数，缺失为 `-1`。
    #[wasm_bindgen(js_name = __yanshi_host_blob_size)]
    fn js_host_blob_size(hash: &str) -> f64;
    /// 宿主全部哈希键的 JSON 字符串数组。
    #[wasm_bindgen(js_name = __yanshi_host_blob_list)]
    fn js_host_blob_list() -> String;
    /// 宿主删除，返回是否真的删掉了一份。
    #[wasm_bindgen(js_name = __yanshi_host_blob_remove)]
    fn js_host_blob_remove(hash: &str) -> bool;
    /// 宿主条数。
    #[wasm_bindgen(js_name = __yanshi_host_blob_count)]
    fn js_host_blob_count() -> f64;
}

#[cfg(target_arch = "wasm32")]
impl HostBlobBackend for JsHostBackend {
    fn put_bytes(&self, hash: &str, bytes: &[u8]) {
        js_host_blob_put(hash, bytes);
    }

    fn get_bytes(&self, hash: &str) -> Option<Vec<u8>> {
        if !js_host_blob_has(hash) {
            return None;
        }
        Some(js_host_blob_get(hash))
    }

    fn has(&self, hash: &str) -> bool {
        js_host_blob_has(hash)
    }

    fn size(&self, hash: &str) -> Option<u64> {
        let size = js_host_blob_size(hash);
        if size < 0.0 {
            None
        } else {
            Some(size as u64)
        }
    }

    fn list_hashes(&self) -> Vec<String> {
        serde_json::from_str(&js_host_blob_list()).unwrap_or_default()
    }

    fn remove(&self, hash: &str) -> bool {
        js_host_blob_remove(hash)
    }

    fn count(&self) -> usize {
        js_host_blob_count().max(0.0) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap as Map;
    use std::sync::Mutex as Lock;

    /// 测试用宿主后端：进程内 `HashMap`，与生产 JS 后端同一 trait 边界。
    #[derive(Default)]
    struct TestBackend {
        entries: Lock<Map<String, Vec<u8>>>,
    }

    impl HostBlobBackend for TestBackend {
        fn put_bytes(&self, hash: &str, bytes: &[u8]) {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .insert(hash.to_owned(), bytes.to_vec());
        }

        fn get_bytes(&self, hash: &str) -> Option<Vec<u8>> {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .get(hash)
                .cloned()
        }

        fn has(&self, hash: &str) -> bool {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .contains_key(hash)
        }

        fn size(&self, hash: &str) -> Option<u64> {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .get(hash)
                .map(|bytes| bytes.len() as u64)
        }

        fn list_hashes(&self) -> Vec<String> {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .keys()
                .cloned()
                .collect()
        }

        fn remove(&self, hash: &str) -> bool {
            self.entries
                .lock()
                .expect("测试后端锁未中毒")
                .remove(hash)
                .is_some()
        }

        fn count(&self) -> usize {
            self.entries.lock().expect("测试后端锁未中毒").len()
        }
    }

    /// 撒谎的宿主后端：`has` 为真但 `get` 返回另一份字节。
    /// 用于钉住“校验失败不是缺失”这一区分。
    struct LyingBackend;

    impl HostBlobBackend for LyingBackend {
        fn put_bytes(&self, _hash: &str, _bytes: &[u8]) {}

        fn get_bytes(&self, _hash: &str) -> Option<Vec<u8>> {
            Some(b"not the bytes you asked for".to_vec())
        }

        fn has(&self, _hash: &str) -> bool {
            true
        }

        fn size(&self, _hash: &str) -> Option<u64> {
            Some(27)
        }

        fn list_hashes(&self) -> Vec<String> {
            Vec::new()
        }

        fn remove(&self, _hash: &str) -> bool {
            false
        }

        fn count(&self) -> usize {
            1
        }
    }

    #[test]
    fn host_store_roundtrip_is_content_addressed() {
        let store = HostBlobStore::with_backend(Arc::new(TestBackend::default()));
        assert!(store.is_empty());
        let first = store.put(b"same").unwrap();
        let again = store.put(b"same").unwrap();
        let other = store.put(b"other").unwrap();
        assert_eq!(first, again, "同内容重复 put 必须返回同一哈希");
        assert_ne!(first, other);
        assert_eq!(store.get(&first).unwrap(), b"same");
        assert!(store.exists(&first));
        assert_eq!(store.size(&first), Some(4));
        assert_eq!(store.len(), 2);
        assert_eq!(store.list().unwrap().len(), 2);
        // put_cache 与 put 同语义（宿主的持久性由宿主负责，不由入口区分）。
        assert_eq!(store.put_cache(b"same").unwrap(), first);
        assert!(store.remove(&other).unwrap());
        assert!(!store.exists(&other));
        assert!(!store.remove(&other).unwrap());
    }

    #[test]
    fn host_store_missing_is_reference_not_found() {
        let store = HostBlobStore::with_backend(Arc::new(TestBackend::default()));
        let missing = BlobHash::from_bytes(b"never stored");
        assert!(!store.exists(&missing));
        assert_eq!(store.size(&missing), None);
        assert_eq!(
            store.get(&missing).unwrap_err().code,
            ErrorCode::ReferenceNotFound
        );
    }

    #[test]
    fn host_store_hash_mismatch_is_not_missing() {
        let store = HostBlobStore::with_backend(Arc::new(LyingBackend));
        let requested = BlobHash::from_bytes(b"whatever the caller wanted");
        let error = store.get(&requested).unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::PreconditionFailed,
            "宿主返回内容与哈希不符时，必须报 precondition_failed，而不是 reference_not_found"
        );
        assert!(
            error.to_string().contains(requested.as_str()),
            "错误里必须同时写明请求哈希：{error}"
        );
    }
}
