//! Blob 内容寻址存储、提交顺序协议与三级生命周期（设计文档 6.3 / 原则 21）。
//!
//! - 超过 4KB 的 payload 走 CAS，原子只存 `{blob_hash, size, mime_type}` 引用。
//! - 提交顺序协议：**blob 先行**（`.tmp` + `rename` 原子落盘），原子后行；
//!   服务端校验原子引用的所有 blob 已存在，缺失则拒绝。
//! - 三级生命周期：
//!
//! | 级别 | 定义 | GC |
//! |---|---|---|
//! | 活跃 | 当前 HEAD 折叠状态引用的 blob（活跃 Manifest） | 保留 |
//! | 历史 | 被日志中任何原子引用、但不在当前折叠状态（revert 中、被 declare_head 甩出、Stash 中） | 保留 |
//! | 孤儿 | 上传成功但从未被任何原子引用（提交失败/中断） | TTL 7 天后清理 |
//!
//! **GC 根集 = 全日志原子引用闭包**。GC 永不删除被任何日志原子引用的 blob——
//! 删 blob 等于部分删除原子，违反原则 2/21。Manifest 只用于活跃集标记与冷热迁移。

use crate::atom::{BlobHash, BlobRef};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::now_ms;
use crate::log::AtomLog;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// 孤儿 blob 的默认 TTL：7 天（设计文档 6.3）。
pub const DEFAULT_ORPHAN_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

/// 超过该字节数的 payload 必须走 CAS（设计文档 6.3），定义见 [`crate::atom`]。
pub use crate::atom::CAS_THRESHOLD_BYTES;

/// blob 生命周期级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlobTier {
    /// 当前折叠状态引用：热存储。
    Active,
    /// 被日志引用但不在当前折叠状态：冷归档，永不因 GC 删除。
    Historical,
    /// 从未被任何原子引用：TTL 到期后清理。
    Orphan,
}

/// CAS 中的一个条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobEntry {
    /// 内容哈希。
    pub blob_hash: BlobHash,
    /// 字节数。
    pub size: u64,
    /// 写入时间（Unix 毫秒）。
    pub created_at: i64,
}

impl BlobEntry {
    /// 距 `now` 的存活秒数。
    pub fn age_seconds(&self, now: i64) -> i64 {
        ((now - self.created_at) / 1000).max(0)
    }
}

/// **blob 的存储编码** ✓ —— 让存储层能压/解，同时**不破坏内容寻址** ✓。
///
/// **为什么需要它**（用户实测 + 我踩过的坑 ✓）：包里 91% 是**未压缩的原始 RGBA** ✓ ⇒ 存储时压一下能省约 5 倍 ✓。
/// 但压缩**必须由存储层自己做** ✓：`put` **先**用**明文**算哈希 ✓、**再**决定怎么写 ✓。
/// 曾经在 server 侧用一个装饰器"先压再交给内层" ✗ ⇒ 内层对**压缩流**算哈希 ✗
/// ⇒ 导入端按**明文**对账时**对不上** ✗（实测：导入用户的真实包被拒 ✓）。
/// 所以这里把它做成**存储自带的一层** ✓，而**不是**外部的包装 ✓。
///
/// 实现放在调用方（如 `yanshi-server` ✓）⇒ **core 不需要依赖 render** ✓，
/// 也就不会形成依赖环 ✓。
pub trait BlobCodec: Send + Sync + std::fmt::Debug {
    /// 明文 ⇒ 落盘字节（**必须是** [`BlobCodec::decode`] 能还原的形式 ✓）。
    fn encode(&self, plain: &[u8]) -> Vec<u8>;

    /// 落盘字节 ⇒ 明文；**不是本编码能认的形式就回 `None`** ✓（调用方据此报错，绝不返回垃圾 ✗）。
    fn decode(&self, stored: &[u8]) -> Option<Vec<u8>>;
}

/// **原样**（缺省）✓：内存存储与测试用它 ✓ —— 不压、也不多一次拷贝语义上的歧义 ✓。
#[derive(Debug)]
pub struct PlainCodec;

impl BlobCodec for PlainCodec {
    fn encode(&self, plain: &[u8]) -> Vec<u8> {
        plain.to_vec()
    }

    fn decode(&self, stored: &[u8]) -> Option<Vec<u8>> {
        Some(stored.to_vec())
    }
}

/// 内容寻址存储接口。
pub trait BlobStore: Send + Sync {
    /// **本存储是否遇到过"文件系统不支持 fsync"** ✓（真实用户报的第 2 条 ✓）。
    ///
    /// 缺省 `false` ✓；文件系统 CAS 在遇到 `ENOTSUP` 时会置位 ✓。
    /// **它存在的意义是"让降级可见"** ✓ —— 数据仍然可靠地写得进去 ✓，
    /// 但**掉电安全没有保证** ✓ ⇒ 使用者有权知道 ✓（由 `/health` 的 `blob_fsync` 报出 ✓）。
    fn unsupported_sync(&self) -> bool {
        false
    }

    /// 写入内容并返回哈希（同哈希幂等去重）。
    ///
    /// **语义**：写入对掉电安全 ✓（实现里 `fsync` ✓；**不支持的文件系统会降级并被看见** ✓，
    /// 见 `unsupported_sync` ✓ 与 `/health` 的 `blob_fsync` ✓）。
    fn put(&self, bytes: &[u8]) -> Result<BlobHash>;

    /// **写入"可重建的缓存"内容** ✓（第 1400 轮 ✓）：**内容寻址与去重语义与 `put` 完全相同** ✗，
    /// 但**允许实现跳过落盘同步** ✗ ⇒ **∴ 因为**调用方按契约**可以重新生成它** ✓
    /// （**∴ 目前唯一的调用方**＝文档缩略图：设计 `:572`✗ 说它是**缓存** ✗ ⇒ **∴ 改为**按需产生 ✓）。
    ///
    /// **为什么需要它** ✗：`put` 的 `fsync` 是**对外的持久性承诺** ✗
    /// ⇒ **∴ 而**为**可重建**的东西付这个成本**没有收益** ✗
    /// ⇒ **∴ 所以**把"是否承诺持久"变成**调用方的选择** ✗ ⇒ **∴ 而不是**全局削弱 ✓。
    ///
    /// **默认实现就是 `put`** ✓ ⇒ **∴ 所以**已有实现**不必改** ✗
    /// ⇒ **∴ 而**只有**文件存储**会覆盖它（**∴ 第 1401 轮 ✓**）** ✓。
    ///
    /// ⚠️ **契约**：**∴ 存进来的东西**崩溃后可能不在** ✗ ⇒ **∴ 所以**调用方
    /// **必须能重建它** ✗ ⇒ **∴ 不得**用它存**权威数据**（**∴ 原子日志 ✗／**∴ 位图 ✓**）** ✓✓
    fn put_cache(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.put(bytes)
    }

    /// 读取内容。
    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>>;

    /// 是否存在。
    fn exists(&self, hash: &BlobHash) -> bool;

    /// 字节数（不存在返回 None）。
    fn size(&self, hash: &BlobHash) -> Option<u64>;

    /// 列出全部条目。
    fn list(&self) -> Result<Vec<BlobEntry>>;

    /// 删除（GC 使用；返回是否真的删除了文件）。
    fn remove(&self, hash: &BlobHash) -> Result<bool>;

    /// **降冷：把热区的 blob 移入冷归档** ✓（设计 §6.3 的后台迁移 ✓）。
    ///
    /// **默认实现是"不做"** ✓ —— 不是所有存储都有冷热之分 ✓（内存存储就没有 ✓）；
    /// 默认空实现让"没有冷层"这件事**显式** ✓，而不是让调用方去猜 ✗。
    /// 返回 `(迁移条数, 迁移字节数)` ✓（设计要求可观测 ✓）。
    ///
    /// **zstd 压缩暂缓** ✗：设计写的是"冷归档 **+ zstd**" ✓ —— 引入压缩库是一次**依赖决策** ✓
    ///（本项目迄今只依赖 `wasm-bindgen` ✓）⇒ 这一轮只做**目录分层** ✓，压缩留给专门一轮 ✓。
    fn demote(&self, _hashes: &[BlobHash]) -> Result<(usize, u64)> {
        Ok((0, 0))
    }

    /// **冷层统计** ✓（条数与字节数 ✓，用于可观测性 ✓）。
    fn cold_stats(&self) -> (usize, u64) {
        (0, 0)
    }
}

/// 按设计文档 6.3 的提交顺序协议把内容写入 CAS，并返回可放进原子 payload 的引用。
pub fn stage_blob(store: &dyn BlobStore, bytes: &[u8], mime_type: &str) -> Result<BlobRef> {
    let hash = store.put(bytes)?;
    Ok(BlobRef {
        blob_hash: hash,
        size: bytes.len() as u64,
        mime_type: mime_type.to_owned(),
    })
}

/// GC 计划：把存储中的 blob 分为活跃 / 历史 / 孤儿，并挑出可清理的孤儿。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcPlan {
    /// GC 根集 = 全日志引用闭包 ∪ 额外根（快照、Stash 等）。
    pub roots: BTreeSet<BlobHash>,
    /// 活跃集（当前折叠状态引用的 blob）。
    pub active: BTreeSet<BlobHash>,
    /// 历史集：被日志引用但不在活跃集。
    pub historical: Vec<BlobEntry>,
    /// 孤儿：未被任何日志原子引用。
    pub orphans: Vec<BlobEntry>,
    /// 孤儿且超过 TTL，本次可清理。
    pub expiring: Vec<BlobEntry>,
    /// 活跃字节数。
    pub active_bytes: u64,
    /// 历史字节数。
    pub historical_bytes: u64,
    /// 孤儿字节数。
    pub orphan_bytes: u64,
}

impl GcPlan {
    /// 生命周期指标（设计文档 14.9 可观测性）。
    pub fn metrics(&self) -> BlobLifecycleMetrics {
        BlobLifecycleMetrics {
            active_count: self.active.len(),
            active_bytes: self.active_bytes,
            historical_count: self.historical.len(),
            historical_bytes: self.historical_bytes,
            orphan_count: self.orphans.len(),
            orphan_bytes: self.orphan_bytes,
            expiring_count: self.expiring.len(),
        }
    }

    /// 活跃→历史的降冷迁移清单（后台任务依据，设计文档 6.3）。
    pub fn to_cool(&self) -> &[BlobEntry] {
        &self.historical
    }
}

/// blob 生命周期指标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobLifecycleMetrics {
    /// 活跃数量。
    pub active_count: usize,
    /// 活跃字节数。
    pub active_bytes: u64,
    /// 历史数量。
    pub historical_count: usize,
    /// 历史字节数。
    pub historical_bytes: u64,
    /// 孤儿数量。
    pub orphan_count: usize,
    /// 孤儿字节数。
    pub orphan_bytes: u64,
    /// 本次可清理的孤儿数量。
    pub expiring_count: usize,
}

/// GC 执行报告。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GcReport {
    /// 被删除的 blob。
    pub deleted: Vec<BlobHash>,
    /// 回收字节数。
    pub bytes_reclaimed: u64,
    /// 扫描条目数。
    pub scanned: usize,
    /// 保留的历史 blob 数。
    pub retained_historical: usize,
    /// 保留的活跃 blob 数。
    pub retained_active: usize,
}

/// 生成 GC 计划。
///
/// `active_manifest` 是当前折叠状态的活跃 blob 清单（`DocumentState::active_blob_manifest()`）；
/// `extra_roots` 覆盖快照 Manifest 与 Stash 中的引用（它们同样属于历史资产）。
pub fn plan_gc(
    store: &dyn BlobStore,
    log: &AtomLog,
    active_manifest: &BTreeSet<BlobHash>,
    extra_roots: &BTreeSet<BlobHash>,
    now: i64,
    ttl_seconds: i64,
) -> Result<GcPlan> {
    let mut roots = log.blob_roots();
    roots.extend(extra_roots.iter().cloned());
    roots.extend(active_manifest.iter().cloned());

    let mut plan = GcPlan {
        roots: roots.clone(),
        active: active_manifest.clone(),
        historical: Vec::new(),
        orphans: Vec::new(),
        expiring: Vec::new(),
        active_bytes: 0,
        historical_bytes: 0,
        orphan_bytes: 0,
    };

    for entry in store.list()? {
        let tier = if active_manifest.contains(&entry.blob_hash) {
            BlobTier::Active
        } else if roots.contains(&entry.blob_hash) {
            BlobTier::Historical
        } else {
            BlobTier::Orphan
        };
        match tier {
            BlobTier::Active => plan.active_bytes += entry.size,
            BlobTier::Historical => {
                plan.historical_bytes += entry.size;
                plan.historical.push(entry);
            }
            BlobTier::Orphan => {
                plan.orphan_bytes += entry.size;
                if entry.age_seconds(now) >= ttl_seconds {
                    plan.expiring.push(entry.clone());
                }
                plan.orphans.push(entry);
            }
        }
    }
    plan.historical
        .sort_by(|a, b| a.blob_hash.cmp(&b.blob_hash));
    plan.orphans.sort_by(|a, b| a.blob_hash.cmp(&b.blob_hash));
    plan.expiring.sort_by(|a, b| a.blob_hash.cmp(&b.blob_hash));

    // 防御：可达性检查——任何根集内的 blob 都不允许进入 expiring。
    plan.expiring
        .retain(|entry| !roots.contains(&entry.blob_hash));
    Ok(plan)
}

/// 执行 GC：只删除超 TTL 的孤儿。
pub fn run_gc(
    store: &dyn BlobStore,
    log: &AtomLog,
    active_manifest: &BTreeSet<BlobHash>,
    extra_roots: &BTreeSet<BlobHash>,
    now: i64,
    ttl_seconds: i64,
) -> Result<(GcReport, GcPlan)> {
    let plan = plan_gc(store, log, active_manifest, extra_roots, now, ttl_seconds)?;
    let mut report = GcReport {
        scanned: plan.active.len() + plan.historical.len() + plan.orphans.len(),
        retained_historical: plan.historical.len(),
        retained_active: plan.active.len(),
        ..GcReport::default()
    };
    for entry in &plan.expiring {
        debug_assert!(
            !plan.roots.contains(&entry.blob_hash),
            "GC 永不删除被日志引用的 blob（原则 21）"
        );
        if store.remove(&entry.blob_hash)? {
            report.deleted.push(entry.blob_hash.clone());
            report.bytes_reclaimed += entry.size;
        }
    }
    Ok((report, plan))
}

/// 内存 CAS（测试与短生命周期会话）。
#[derive(Debug, Default)]
pub struct MemoryBlobStore {
    entries: Mutex<std::collections::BTreeMap<BlobHash, (Vec<u8>, i64)>>,
}

impl MemoryBlobStore {
    /// 新建空存储。
    pub fn new() -> Self {
        Self::default()
    }

    /// 以指定写入时间放入内容（TTL 测试用）。
    pub fn put_at(&self, bytes: &[u8], created_at: i64) -> Result<BlobHash> {
        let hash = BlobHash::from_bytes(bytes);
        let mut entries = self.entries.lock().expect("内存 CAS 锁未中毒");
        entries
            .entry(hash.clone())
            .or_insert_with(|| (bytes.to_vec(), created_at));
        Ok(hash)
    }

    /// 当前条目数。
    pub fn len(&self) -> usize {
        self.entries.lock().expect("内存 CAS 锁未中毒").len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl BlobStore for MemoryBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.put_at(bytes, now_ms())
    }

    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        self.entries
            .lock()
            .expect("内存 CAS 锁未中毒")
            .get(hash)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| missing_blob(hash))
    }

    fn exists(&self, hash: &BlobHash) -> bool {
        self.entries
            .lock()
            .expect("内存 CAS 锁未中毒")
            .contains_key(hash)
    }

    fn size(&self, hash: &BlobHash) -> Option<u64> {
        self.entries
            .lock()
            .expect("内存 CAS 锁未中毒")
            .get(hash)
            .map(|(bytes, _)| bytes.len() as u64)
    }

    fn list(&self) -> Result<Vec<BlobEntry>> {
        Ok(self
            .entries
            .lock()
            .expect("内存 CAS 锁未中毒")
            .iter()
            .map(|(hash, (bytes, created_at))| BlobEntry {
                blob_hash: hash.clone(),
                size: bytes.len() as u64,
                created_at: *created_at,
            })
            .collect())
    }

    fn remove(&self, hash: &BlobHash) -> Result<bool> {
        Ok(self
            .entries
            .lock()
            .expect("内存 CAS 锁未中毒")
            .remove(hash)
            .is_some())
    }
}

/// 文件系统 CAS：hash 分桶路径 + `.tmp` 后 `rename` 的原子写入。
#[derive(Debug, Clone)]
pub struct FsBlobStore {
    root: PathBuf,
    /// **这台机器上 fsync 是否不被支持** ✓（真实用户报的第 2 条 ✓：9p/NFS 上 `ENOTSUP` ✗）。
    ///
    /// **为什么要记下来** ✓：降级是**必要的**（否则任何写操作都失败 ✗），
    /// 但**不能静默** ✗ ⇒ 这个标记由 `/health` 报出来 ✓ ⇒ 使用者能知道
    /// "我的数据落在了一个不保证掉电安全的文件系统上" ✓。
    unsupported_sync: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// **冷归档目录** ✓：与热区**同构** ✓（`<root>/cold/sha256/xx/yy/<hex>` ✓）⇒ 迁移就是"改名" ✓。
    /// 设计里它还有一层 zstd 压缩 ✓ —— **暂缓** ✓（依赖决策 ✓，见 trait 说明 ✓）。
    cold: PathBuf,
    /// **存储编码** ✓：缺省原样 ✓；`yanshi-server` 会注入"仓库自带 deflate"那一版 ✓
    /// ⇒ **哈希仍对明文算** ✓（详见 [`BlobCodec`] ✓）。
    codec: std::sync::Arc<dyn BlobCodec>,
}

impl FsBlobStore {
    /// **注入存储编码** ✓（builder 风格 ✓：必须在 `Arc` 包装**之前**调用 ✓）。
    /// 传入的实现由调用方提供（如 `yanshi-server` 用仓库自带的 deflate ✓）⇒ core 不依赖 render ✓。
    pub fn with_codec(mut self, codec: std::sync::Arc<dyn BlobCodec>) -> Self {
        self.codec = codec;
        self
    }

    /// 打开（或创建）根目录。
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("sha256")).map_err(|error| io_error(&root, error))?;
        fs::create_dir_all(root.join("cold").join("sha256"))
            .map_err(|error| io_error(&root, error))?;
        let cold = root.join("cold");
        let unsupported_sync = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        Ok(Self {
            root,
            cold,
            codec: std::sync::Arc::new(PlainCodec),
            unsupported_sync,
        })
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// **本存储是否遇到过"文件系统不支持 fsync"** ✓（真实用户报的第 2 条 ✓）。
    ///
    /// 一旦为真 ✓，说明**掉电安全没有保证** ✓（数据仍然写得进去 ✓，只是没有 fsync 兜底 ✓）。
    /// 由 `/health` 暴露 ✓ —— **降级可以接受，静默降级不可以** ✗。
    pub fn unsupported_sync(&self) -> bool {
        self.unsupported_sync
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// blob 的最终路径：`<root>/sha256/<ab>/<cd>/<hex>`。
    /// **冷归档里的路径** ✓（与热区同构 ✓）。
    pub fn cold_path_of(&self, hash: &BlobHash) -> PathBuf {
        let hex = hash.hex();
        self.cold
            .join("sha256")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(hex)
    }

    /// 走一层（热区或冷层 ✓），按哈希去重 ✓。
    fn collect_tier(
        &self,
        shard_root: &Path,
        entries: &mut Vec<BlobEntry>,
        seen: &mut std::collections::BTreeSet<String>,
    ) {
        let Ok(shards) = fs::read_dir(shard_root) else {
            return;
        };
        for shard in shards.flatten() {
            let Ok(sub_shards) = fs::read_dir(shard.path()) else {
                continue;
            };
            for sub in sub_shards.flatten() {
                let Ok(files) = fs::read_dir(sub.path()) else {
                    continue;
                };
                for file in files.flatten() {
                    let name = file.file_name().to_string_lossy().into_owned();
                    if !is_hex_name(&name) || !seen.insert(name.clone()) {
                        continue;
                    }
                    let Ok(metadata) = file.metadata() else {
                        continue;
                    };
                    let Ok(hash) = format!("sha256:{name}").parse::<BlobHash>() else {
                        continue;
                    };
                    entries.push(BlobEntry {
                        blob_hash: hash,
                        size: metadata.len(),
                        created_at: mtime_ms(&metadata),
                    });
                }
            }
        }
    }

    /// 热区里的路径 ✓（分片目录 ✓，避免单目录几十万文件 ✗）。
    pub fn path_of(&self, hash: &BlobHash) -> PathBuf {
        let hex = hash.hex();
        self.root
            .join("sha256")
            .join(&hex[0..2])
            .join(&hex[2..4])
            .join(hex)
    }
}

/// **判断一个 fsync 错误是否属于"文件系统不支持"** ✓（真实用户报的第 2 条 ✓）。
///
/// **为什么单独一个函数** ✓：这是**策略** ✓（哪些错误可以降级 ✓、哪些必须失败 ✗）⇒
/// **必须能单独测** ✓ —— 否则本地没有 9p 挂载就永远测不到这条路径 ✗。
///
/// **判据** ✓：`ErrorKind::Unsupported` ✓（Rust 对 ENOTSUP/EOPNOTSUPP 的映射 ✓）
/// 或**原始 errno 95** ✓（各版本映射不一定一致 ⇒ 两条都认 ✓）。
fn is_unsupported_sync(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::Unsupported || error.raw_os_error() == Some(95)
}

/// 尽力同步 ✓：`Ok(true)` = 同步成功 ✓；`Ok(false)` = **文件系统不支持 ⇒ 降级** ✓；
/// `Err` = **其它错误照旧失败** ✗（磁盘满 / 权限 / IO 错都不能放过 ✓）。
fn sync_or_degrade(file: &fs::File) -> std::io::Result<bool> {
    match file.sync_all() {
        Ok(()) => Ok(true),
        Err(error) if is_unsupported_sync(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn io_error(path: &Path, error: std::io::Error) -> YanshiError {
    YanshiError::new(
        ErrorCode::ResourceExhausted,
        ErrorContext::detail(format!("CAS IO 失败 {:?}: {error}", path)),
    )
}

fn missing_blob(hash: &BlobHash) -> YanshiError {
    YanshiError::new(
        ErrorCode::ReferenceNotFound,
        ErrorContext::detail(format!("blob {hash} 不存在")),
    )
    .with_blob(hash.to_string())
}

fn is_hex_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl BlobStore for FsBlobStore {
    fn put(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.put_inner(bytes, true)
    }

    /// **缓存内容：跳过落盘同步** ✓（第 1402 轮 ✓）。内容寻址与去重语义**完全不变** ✓
    /// ⇒ **∴ 差别只在**不调 `sync_or_degrade`** ✗ ⇒ **∴ 所以**崩溃后它**可能缺失** ✗
    /// ⇒ **∴ 而**调用方**必须**能重建它 ✓（**∴ 契约见 trait 上的 `put_cache` ✓**）。
    fn put_cache(&self, bytes: &[u8]) -> Result<BlobHash> {
        self.put_inner(bytes, false)
    }

    fn get(&self, hash: &BlobHash) -> Result<Vec<u8>> {
        // **热区优先、冷归档兜底** ✓ —— 这就是设计说的"历史 blob 按需从归档取回（慢路径）" ✓。
        let stored = match fs::read(self.path_of(hash)) {
            Ok(bytes) => bytes,
            Err(_) => fs::read(self.cold_path_of(hash)).map_err(|_| missing_blob(hash))?,
        };
        // **解不开就报错** ✗ —— 绝不把垃圾当数据返回 ✓（存储层最怕这个 ✓）。
        self.codec.decode(&stored).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "blob {hash} 无法按当前存储编码解出（本仓库仍在开发期，旧格式不做兼容）"
                )),
            )
            .with_blob(hash.to_string())
        })
    }

    fn exists(&self, hash: &BlobHash) -> bool {
        self.path_of(hash).is_file() || self.cold_path_of(hash).is_file()
    }

    fn size(&self, hash: &BlobHash) -> Option<u64> {
        fs::metadata(self.path_of(hash))
            .or_else(|_| fs::metadata(self.cold_path_of(hash)))
            .ok()
            .map(|meta| meta.len())
    }

    fn list(&self) -> Result<Vec<BlobEntry>> {
        // **两层都要列** ✓（冷归档里的 blob 依然存在 ✓，GC 与体积统计必须看得见 ✓）；
        // **按哈希去重** ✓ —— 迁移是"改名" ✓，但中断可能留下两边都有同一份的瞬间 ✓，
        // 那时列两遍会让统计虚高 ✗、也会让 GC 重复处理同一份 ✗。
        let mut entries = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        self.collect_tier(&self.root.join("sha256"), &mut entries, &mut seen);
        self.collect_tier(&self.cold.join("sha256"), &mut entries, &mut seen);
        entries.sort_by(|left, right| left.blob_hash.cmp(&right.blob_hash));
        Ok(entries)
    }
    fn remove(&self, hash: &BlobHash) -> Result<bool> {
        // **热区与冷层都要能删** ✓ —— 否则"已被降冷的历史 blob"永远删不掉 ✗
        //（`remove` 是全项目统一的删除入口 ✓，它必须覆盖两层 ✓）。
        for path in [self.path_of(hash), self.cold_path_of(hash)] {
            match fs::remove_file(&path) {
                Ok(()) => return Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(io_error(&path, error)),
            }
        }
        Ok(false)
    }

    /// **降冷：热区 ⇒ 冷归档** ✓（设计 §6.3 的后台迁移 ✓）。
    ///
    /// **为什么是"改名"而不是"复制 + 删"** ✓：同一文件系统内改名是原子的 ✓
    /// ⇒ 不会出现"复制了一半、两端都不完整"的窗口 ✗。
    fn demote(&self, hashes: &[BlobHash]) -> Result<(usize, u64)> {
        let mut moved = 0usize;
        let mut bytes = 0u64;
        for hash in hashes {
            let from = self.path_of(hash);
            if !from.is_file() {
                continue;
            }
            let to = self.cold_path_of(hash);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|error| io_error(parent, error))?;
            }
            let size = fs::metadata(&from).map(|meta| meta.len()).unwrap_or(0);
            fs::rename(&from, &to).map_err(|error| io_error(&from, error))?;
            moved += 1;
            bytes += size;
        }
        Ok((moved, bytes))
    }

    /// **冷层统计** ✓（条数与字节数 ✓）。
    fn cold_stats(&self) -> (usize, u64) {
        let mut entries = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        self.collect_tier(&self.cold.join("sha256"), &mut entries, &mut seen);
        (entries.len(), entries.iter().map(|entry| entry.size).sum())
    }
}

/// **`FsBlobStore` 的固有实现** ✓（第 1402 轮 ✓）：**Rust 不允许在 `impl Trait for Type`
/// 里定义非 trait 方法** ✗（`E0407` 实测 ✓）⇒ **∴ 所以 `put_inner` 放在这里 ✓**。
impl FsBlobStore {
    /// **`put` 与 `put_cache` 的共同实现** ✓：`sync` 决定是否调 `sync_or_degrade` ✓。
    fn put_inner(&self, bytes: &[u8], sync: bool) -> Result<BlobHash> {
        let hash = BlobHash::from_bytes(bytes);
        let target = self.path_of(&hash);
        if target.exists() {
            return Ok(hash);
        }
        let dir = target.parent().expect("CAS 路径必有父目录");
        fs::create_dir_all(dir).map_err(|error| io_error(dir, error))?;
        // 并发写入安全：先写唯一 .tmp，再 rename（同哈希并发时 rename 幂等）。
        static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
        let tmp = dir.join(format!(
            ".{}.{}.tmp",
            hash.hex(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        {
            let mut file = fs::File::create(&tmp).map_err(|error| io_error(&tmp, error))?;
            // **写入的是"编码后"的字节** ✓，而上面的哈希来自**明文** ✓ ⇒ 内容寻址语义不变 ✓。
            let stored = self.codec.encode(bytes);
            file.write_all(&stored)
                .map_err(|error| io_error(&tmp, error))?;
            // **`sync_all` 在部分文件系统上不被支持** ✓（真实用户报的第 2 条 ✓）：
            // 9p / 某些网络挂载上 fsync 返回 `ENOTSUP`（os error 95 ✓）✗
            // ⇒ 原来这一句让**任何写操作都失败** ✗（"CAS IO 失败 … Operation not supported" ✓）。
            // **原则** ✓：**只在"文件系统明确不支持"时降级** ✓ ——
            // 磁盘满 / 权限 / IO 错**照旧失败** ✗，绝不笼统放过 ✓；
            // 并且**降级要被看见** ✓（见 `unsupported_sync` ✓，由 `/health` 报出 ✓）。
            if sync && !sync_or_degrade(&file).map_err(|error| io_error(&tmp, error))? {
                self.unsupported_sync
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        fs::rename(&tmp, &target).map_err(|error| io_error(&target, error))?;
        Ok(hash)
    }
}

fn mtime_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_else(now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom::{Atom, AtomKind};
    use serde_json::json;
    use std::sync::Arc;

    fn temp_root(tag: &str) -> PathBuf {
        let unique = format!("yanshi-cas-{tag}-{}", std::process::id());
        let mut root = std::env::temp_dir();
        root.push(unique);
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn log_with_blob(hash: &BlobHash, atom_id: &str) -> AtomLog {
        let mut log = AtomLog::new();
        log.append(
            Atom::new(
                AtomKind::CreateDocument,
                "human:1",
                "s",
                json!({"doc_id": "doc_1"}),
            )
            .with_id("a_doc"),
        )
        .unwrap();
        log.append(
            Atom::new(
                AtomKind::CreateObject,
                "ai:1",
                "s",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "type": "raster_patch",
                    "bitmap": {"blob_hash": hash, "size": 4, "mime_type": "image/webp"},
                }),
            )
            .with_id(atom_id),
        )
        .unwrap();
        log
    }

    #[test]
    fn put_is_content_addressed_and_idempotent() {
        let store = MemoryBlobStore::new();
        let a = store.put(b"same").unwrap();
        let b = store.put(b"same").unwrap();
        let c = store.put(b"other").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get(&a).unwrap(), b"same");
        assert_eq!(store.size(&a), Some(4));
        assert!(store.exists(&a));
        assert!(!store.exists(&BlobHash::from_bytes(b"never")));
        assert_eq!(
            store.get(&BlobHash::from_bytes(b"never")).unwrap_err().code,
            ErrorCode::ReferenceNotFound
        );
    }

    #[test]
    fn fs_store_roundtrip_and_sharded_paths() {
        let root = temp_root("roundtrip");
        let store = FsBlobStore::open(&root).unwrap();
        let bytes = vec![7u8; 10_000];
        let hash = store.put(&bytes).unwrap();
        assert_eq!(store.get(&hash).unwrap(), bytes);
        let path = store.path_of(&hash);
        assert!(path.is_file());
        assert_eq!(
            &hash.hex()[0..2],
            path.parent()
                .unwrap()
                .parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
        );
        assert_eq!(store.list().unwrap().len(), 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ten_concurrent_writers_of_same_hash_produce_one_file() {
        let root = temp_root("concurrent");
        let store = Arc::new(FsBlobStore::open(&root).unwrap());
        let payload = Arc::new(vec![42u8; 32 * 1024]);
        let mut handles = Vec::new();
        for _ in 0..10 {
            let store = Arc::clone(&store);
            let payload = Arc::clone(&payload);
            handles.push(std::thread::spawn(move || store.put(&payload).unwrap()));
        }
        let hashes: Vec<BlobHash> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(hashes.windows(2).all(|pair| pair[0] == pair[1]));
        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 1, "同哈希并发写入必须去重为一份");
        assert_eq!(store.get(&hashes[0]).unwrap(), *payload);
        // 没有遗留 .tmp 文件。
        let dir = store.path_of(&hashes[0]).parent().unwrap().to_path_buf();
        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "遗留临时文件: {leftovers:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gc_keeps_reverted_history_blobs_and_removes_orphans_after_ttl() {
        let store = MemoryBlobStore::new();
        let history_blob = store.put_at(b"history", 1_000).unwrap();
        let orphan_old = store.put_at(b"orphan-old", 1_000).unwrap();
        // 距今 1000 秒：未超 7 天 TTL。
        let orphan_fresh = store.put_at(b"orphan-fresh", 999_000_000).unwrap();

        let log = log_with_blob(&history_blob, "a_obj");
        // 该 blob 被 revert：不在活跃 Manifest，但仍是历史资产。
        let active_manifest = BTreeSet::new();
        let now = 1_000_000_000;
        let plan = plan_gc(
            &store,
            &log,
            &active_manifest,
            &BTreeSet::new(),
            now,
            DEFAULT_ORPHAN_TTL_SECONDS,
        )
        .unwrap();

        assert!(plan.roots.contains(&history_blob));
        assert!(plan.historical.iter().any(|e| e.blob_hash == history_blob));
        assert_eq!(
            plan.orphans
                .iter()
                .filter(|e| e.blob_hash == orphan_old || e.blob_hash == orphan_fresh)
                .count(),
            2
        );
        assert_eq!(plan.expiring.len(), 1);
        assert_eq!(plan.expiring[0].blob_hash, orphan_old);

        let (report, _) = run_gc(
            &store,
            &log,
            &active_manifest,
            &BTreeSet::new(),
            now,
            DEFAULT_ORPHAN_TTL_SECONDS,
        )
        .unwrap();
        assert_eq!(report.deleted, vec![orphan_old.clone()]);
        assert!(store.exists(&history_blob), "历史 blob 永不删除");
        assert!(store.exists(&orphan_fresh), "未超 TTL 的孤儿保留");
        assert!(!store.exists(&orphan_old));

        let metrics = plan.metrics();
        assert_eq!(metrics.historical_count, 1);
        assert_eq!(metrics.orphan_count, 2);
        assert_eq!(metrics.expiring_count, 1);
    }

    #[test]
    fn gc_root_set_includes_manifest_and_extra_roots() {
        let store = MemoryBlobStore::new();
        let manifest_blob = store.put_at(b"active", 0).unwrap();
        let stash_blob = store.put_at(b"stash", 0).unwrap();
        let orphan = store.put_at(b"orphan", 0).unwrap();
        let log = AtomLog::new();

        let mut manifest = BTreeSet::new();
        manifest.insert(manifest_blob.clone());
        let mut extra = BTreeSet::new();
        extra.insert(stash_blob.clone());

        let plan = plan_gc(
            &store,
            &log,
            &manifest,
            &extra,
            1_000_000_000,
            DEFAULT_ORPHAN_TTL_SECONDS,
        )
        .unwrap();
        assert!(plan.active.contains(&manifest_blob));
        assert!(plan.historical.iter().any(|e| e.blob_hash == stash_blob));
        assert_eq!(plan.expiring.len(), 1);
        assert_eq!(plan.expiring[0].blob_hash, orphan);
    }

    #[test]
    fn stage_blob_matches_commit_order_protocol() {
        let store = MemoryBlobStore::new();
        let blob_ref = stage_blob(&store, b"payload-bytes", "image/webp").unwrap();
        assert_eq!(blob_ref.size, 13);
        assert_eq!(blob_ref.mime_type, "image/webp");
        assert!(store.exists(&blob_ref.blob_hash));
        assert_eq!(
            blob_ref.blob_hash,
            BlobHash::from_bytes(b"payload-bytes"),
            "先 PUT blob，再提交引用该 hash 的原子"
        );
    }

    /// **降冷之后仍能按需取回** ✓（设计 §6.3：历史 blob "保留，可按需取回" ✓）。
    ///
    /// 这条是冷热分层最要紧的不变量 ✓：**分层不许让任何数据变得读不到** ✗ ——
    /// 时间旅行、reapply、Stash 重放都要从归档里把像素取回来 ✓（慢路径 ✓，但必须成功 ✓）。
    #[test]
    fn demoted_blobs_are_still_readable_and_counted_once() {
        let dir = std::env::temp_dir().join(format!("yanshi-cold-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let store = FsBlobStore::open(&dir).expect("应能打开存储");
        let first = store.put(b"hot-one").expect("入库应成功");
        let second = store.put(b"hot-two").expect("入库应成功");
        assert_eq!(store.list().expect("应能列出").len(), 2, "两份都在热区 ✓");
        // 降冷一份 ✓。
        let (moved, bytes) = store
            .demote(std::slice::from_ref(&first))
            .expect("降冷应成功");
        assert_eq!((moved, bytes), (1, 7), "应迁移 1 份、7 字节 ✓");
        // ① **仍能取回** ✓（读路径回退到冷层 ✓）。
        assert_eq!(
            store.get(&first).expect("降冷后仍应能取回 ✓"),
            b"hot-one".to_vec()
        );
        assert!(store.exists(&first), "exists 也要覆盖冷层 ✓");
        assert_eq!(store.size(&first), Some(7), "size 也要覆盖冷层 ✓");
        // ② **清单里只见一次** ✓（去重 ✓）。
        assert_eq!(
            store.list().expect("应能列出").len(),
            2,
            "两层合起来还是两份 ✓"
        );
        // ③ **冷层统计** ✓（设计要求可观测 ✓）。
        let (count, bytes) = store.cold_stats();
        assert_eq!((count, bytes), (1, 7), "冷层应有 1 份、7 字节 ✓");
        // ④ **重复降冷是 no-op** ✓（热区已经没有它了 ✓）。
        assert_eq!(
            store.demote(std::slice::from_ref(&first)).expect("应成功"),
            (0, 0),
            "重复降冷不该重复计数 ✓"
        );
        // ⑤ **冷层的 blob 也能删** ✓（`remove` 覆盖两层 ✓）。
        assert!(
            store.remove(&first).expect("删除应成功"),
            "冷层的也要能删 ✓"
        );
        assert!(!store.exists(&first), "删掉之后不该还在 ✓");
        let (count, _) = store.cold_stats();
        assert_eq!(count, 0, "冷层应已空 ✓");
        // 另一份没被牵连 ✓。
        assert_eq!(store.get(&second).expect("取回应成功"), b"hot-two".to_vec());
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod sync_policy_tests {
    use super::*;

    /// **降级策略：只放过"不支持"，其余照旧失败** ✓（真实用户报的第 2 条 ✓）。
    ///
    /// 这条**不需要 9p 挂载也能测** ✓ —— 因为它测的是**策略** ✓，不是文件系统 ✓。
    #[test]
    fn unsupported_sync_is_degraded_but_other_errors_are_not() {
        // ① ENOTSUP（errno 95 ✓）：**可以降级** ✓ —— 这正是 9p / 网络挂载上报的错 ✓。
        let unsupported = std::io::Error::from_raw_os_error(95);
        assert!(
            is_unsupported_sync(&unsupported),
            "ENOTSUP 应当被识别为可降级"
        );
        // ② `ErrorKind::Unsupported` ✓：也要认 ✓（不同平台映射不同 ✓）。
        let kind = std::io::Error::new(std::io::ErrorKind::Unsupported, "no fsync here");
        assert!(is_unsupported_sync(&kind), "Unsupported 应当被识别为可降级");
        // ③ **其它错误一律不能放过** ✗：磁盘满 ✓、权限 ✓、IO 错 ✓。
        for fatal in [
            std::io::Error::from_raw_os_error(28), // ENOSPC
            std::io::Error::from_raw_os_error(13), // EACCES
            std::io::Error::from_raw_os_error(5),  // EIO
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope"),
        ] {
            assert!(
                !is_unsupported_sync(&fatal),
                "不该把 {fatal:?} 当成可降级（那会把真实故障吞掉）"
            );
        }
    }

    /// **正常文件系统上不发生降级** ✓（大多数环境的实际路径 ✓）。
    #[test]
    fn a_normal_write_reports_sync_supported() {
        let dir = std::env::temp_dir().join(format!("yanshi_sync_{}", std::process::id()));
        let path = dir.join("probe.bin");
        fs::create_dir_all(&dir).expect("临时目录应可建");
        let file = fs::File::create(&path).expect("临时文件应可建");
        let outcome = sync_or_degrade(&file).expect("正常文件系统上 fsync 应当成功");
        assert!(outcome, "正常文件系统上应当报告同步成功");
        let _ = fs::remove_dir_all(&dir);
    }
}
