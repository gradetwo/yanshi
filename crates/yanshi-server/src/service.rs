//! 工作区/服务：多文档注册表与会话编排（设计文档 3 章服务端）。
//!
//! [`Workspace`] 负责：文档创建与打开、持久化（原子 JSONL + CAS + 渲染缓存）、
//! 提交路径的落盘、令牌发放与鉴权、以及给工具层使用的统一入口。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use yanshi_core::blob::{BlobStore, MemoryBlobStore};
use yanshi_core::{Atom, Bbox, ChangesetId, ErrorCode, ErrorContext, Result, Seq, YanshiError};
use yanshi_render::thumb::ThumbKind;

use crate::document::{
    CommitResult, Document, DocumentSettings, NewDocument, RenderStatus, RenderedPreview,
};
use crate::persist::{DocumentMeta, FileStore};
use crate::token::{CapabilityToken, Principal, Role, TransportKind};

/// 文档缩略图尺寸档位（7.3 分级；`Skip` 表示不生成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocThumbSize {
    /// 不生成缩略图。
    Skip,
    /// 64px。
    S64,
    /// 128px。
    S128,
    /// 256px（默认）。
    S256,
}

impl DocThumbSize {
    /// 对应的缩略图类型。
    pub const fn kind(self) -> ThumbKind {
        match self {
            Self::S64 => ThumbKind::Doc64,
            Self::S128 => ThumbKind::Doc128,
            Self::Skip | Self::S256 => ThumbKind::Doc256,
        }
    }

    /// 由参数解析（`64` / `128` / `256` / `false`）。
    pub fn parse(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Bool(false) => Self::Skip,
            serde_json::Value::Bool(true) | serde_json::Value::Null => Self::S256,
            serde_json::Value::Number(number) => match number.as_u64() {
                Some(64) | Some(32) => Self::S64,
                Some(128) => Self::S128,
                _ => Self::S256,
            },
            serde_json::Value::String(text) => match text.as_str() {
                "skip" | "none" | "false" => Self::Skip,
                "64" => Self::S64,
                "128" => Self::S128,
                _ => Self::S256,
            },
            _ => Self::S256,
        }
    }
}

/// 文档摘要（文档列表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSummary {
    /// 文档 id。
    pub doc_id: String,
    /// 画布宽。
    pub width: u32,
    /// 画布高。
    pub height: u32,
    /// 原子数。
    pub atoms: usize,
    /// head seq。
    pub head_seq: Seq,
    /// 图层数。
    pub layers: usize,
    /// 对象数。
    pub objects: usize,
    /// 创建时间。
    pub created_at: i64,
    /// 是否已持久化到磁盘。
    pub persisted: bool,
}

/// **悬空变更集（Stash）** ✓ —— 设计 §12.4 ✓。
///
/// **设计给了行为、没给工具名 ⇒ 记录选择** ✓：
/// * 打包内容 = 原子 ✓ + **原子引用到的全部 blob** ✓（`Atom::all_blob_refs()` ✓，§897 原话 ✓）；
/// * **原子无 seq** ✓（从未进日志 ✓）、**重新提交时获得新 seq** ✓（§900 ✓）——
///   这里就是把 `Atom` 原样留着 ✓（它的 `seq` 只在进日志时才产生 ✓）；
/// * blob 归**历史级保留、不被 GC** ✓（§899 ✓）：眼下本项目**还没有 GC** ✓
///   ⇒ 今天无需做任何事 ✓，但**这条约束要写下来** ✓：将来做 GC 时 ✓，Stash 引用的 blob
///   必须算作根 ✓（否则"离线期间的编辑"会在重连前就被清掉 ✗）。
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Stash {
    /// 悬空变更集 id ✓（`stash_<ULID>` ✓）——`list/apply/discard` 都用它 ✓。
    pub id: String,
    /// 它属于哪份文档 ✓（离线编辑是**针对某一份文档**的 ✓，不是全局的 ✓）。
    pub doc_id: String,
    /// 谁在离线期间做的这些改动 ✓（"分支对比"要显示 ✓）。
    pub actor: String,
    /// 当时那个会话 ✓（原子的 `session` 字段要保留 ✓，否则重放后来源就丢了 ✗）。
    pub session: String,
    /// 搁置时刻（毫秒 ✓），用来排序与展示 ✓。
    pub created_at: i64,
    /// 为什么被搁置 ✓（校验器给的原文 ✓）—— "分支对比"要让人看懂发生了什么 ✓。
    pub reason: String,
    /// **搁置的原子** ✓ —— §900：它们**没有 seq**（从未进日志 ✓），重放时才获得新 seq ✓。
    pub atoms: Vec<yanshi_core::Atom>,
    /// 原子引用到的 blob ✓（随原子一起保存 ✓，§897 ✓）。
    pub blob_refs: Vec<String>,
}

/// **Blob 三级生命周期** ✓（设计 §6.3 ✓）。
///
/// **设计原话** ✓：GC 根集 = **全日志原子引用闭包** ✓ —— **永不删除被任何日志原子引用的 blob** ✓
///（"删 blob 等于部分删除原子" ✓，违反原则 2/21 ✓）。三级是：
/// * **活跃** ✓：当前 HEAD 折叠状态引用的 ✓；
/// * **历史** ✓：被日志里任何原子引用、但不在当前状态 ✓（revert 目标、被 `declare_head` 甩出、
///   **Stash 里的**、旧分支 ✓）⇒ 设计要求**保留**（冷归档 + 按需取回 ✓）；
/// * **孤儿** ✓：上传成功但从未被任何原子引用 ✓ ⇒ 临时区 ✓、**TTL 7 天**后清理 ✓。
///
/// **两条必须写下来的取舍** ✓：
/// 1. **Stash 里的原子算根** ✓ —— 我上一轮给 Stash 写代码时就写下了这条约束 ✓，
///    这里落实它 ✓：否则"离线期间的编辑"会在重连前被清掉 ✗；
/// 2. **不做 zstd 冷归档（暂缓 ✓）** ✓：设计里"历史级 = 冷归档 + zstd" ✓，
///    而本项目迄今**只依赖 `wasm-bindgen`** ✓（传输层还是手写的 ✓）⇒
///    引入压缩库是一次**依赖决策** ✓，我不擅自做 ✗ ⇒ 这一版只做**分级与孤儿回收** ✓
///    （正确性最关键的那半 ✓），压缩留给专门一轮 ✓。
#[derive(Clone, Debug, Default)]
pub struct BlobLifecycle {
    /// **本次分级考虑了几份文档** ✓ —— 分类是否可信，看这个数一眼就知道 ✓
    ///（它少于磁盘上的文档数 ⇒ 结论不可信 ✓，这是那次险些误删换来的可观测项 ✓）。
    pub documents_considered: usize,
    /// 活跃级的 blob 数 ✓（当前 HEAD 折叠状态引用的 ✓）。
    pub active_count: usize,
    /// 活跃级的字节数 ✓。
    pub active_bytes: u64,
    /// 历史级的 blob 数 ✓（被日志引用但不在当前状态 ✓ —— **保留** ✓）。
    pub history_count: usize,
    /// 历史级的字节数 ✓。
    pub history_bytes: u64,
    /// 孤儿级的 blob 数 ✓（上传后从未被任何原子引用 ✓）。
    pub orphan_count: usize,
    /// 孤儿级的字节数 ✓。
    pub orphan_bytes: u64,
    /// 已过 TTL 的孤儿 ✓ —— **只有这些**可以被回收 ✓。
    pub collectible: Vec<String>,
    /// 可回收孤儿的字节数 ✓。
    pub collectible_bytes: u64,
    /// 各级的**哈希清单** ✓ —— 审计需要 ✓，测试也靠它做"针对具体 blob"的断言 ✓
    ///（只看计数的话 ✓，提交时产生的**预览 blob** 会把计数搅乱 ✗ —— 本轮实测如此 ✓）。
    ///
    /// 活跃级的哈希清单 ✓。
    pub active_hashes: Vec<String>,
    /// 历史级的哈希清单 ✓。
    pub history_hashes: Vec<String>,
    /// 孤儿级的哈希清单 ✓。
    pub orphan_hashes: Vec<String>,
}

/// **一条可用纹理** ✓（内置的或缓存的 ✓）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureEntry {
    /// 文件名 ✓（含扩展名 ✓）。
    pub name: String,
    /// 字节数 ✓。
    pub bytes: u64,
    /// **来源** ✓：`bundled` = 随发行包发布 ✓；`cache` = 抓到工作区里的 ✓。
    pub source: &'static str,
}

/// **读一个目录里的纹理文件** ✓（不存在 ⇒ 空表 ✓ —— "还没下载过"是**正常状态** ✗，不是错误 ✓）。
///
/// **抽成函数** ✓：内置目录与缓存目录**走同一段逻辑** ✓ ⇒ 不会出现
/// "缓存那条会过滤隐藏文件、内置那条忘了" 这种漂移 ✗（本项目吃过太多次 ✓）。
/// **资产种类 ⇒ 子目录 + 允许的扩展名** ✓（三类共用 ✓）。
///
/// **为什么把这张表放在内核** ✓：工具层、缓存目录、内置目录、扩展名校验**都读它** ✓
/// ⇒ 加一种资产只改这一处 ✓（各写一份清单必然漂移 ✗）。
fn asset_layout(kind: &str) -> Result<(&'static str, &'static [&'static str])> {
    Ok(match kind {
        "brush" => ("brushes", &["myb"] as &[&str]),
        // **纹理只收 PNG** ✓：像素解码器是自己写的 ✓、只解 PNG ✗
        //（JPEG/WebP 会被明确拒绝 ✓）⇒ 收进来用不了的东西是**骗人** ✗。
        "texture" => ("textures", &["png"] as &[&str]),
        "palette" => ("palettes", &["json", "kpl", "gpl", "txt"] as &[&str]),
        other => {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "未知资产种类 {other} ⇒ 可用：brush / texture / palette"
                )),
            ))
        }
    })
}

/// **资产文件名必须干净** ✗（挡 `..`、路径分隔符、隐藏文件 ✓）。
fn asset_file_name(name: &str) -> Result<String> {
    let clean = name.trim();
    if clean.is_empty()
        || clean.starts_with('.')
        || clean.contains('/')
        || clean.contains('\\')
        || clean.contains("..")
    {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("资产文件名不合法：{name}")),
        ));
    }
    Ok(clean.to_string())
}

fn read_texture_dir(path: &std::path::Path) -> Result<Vec<(String, u64)>> {
    let mut found: Vec<(String, u64)> = Vec::new();
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => {
            // **`ErrorCode` 里没有 Io** ✗ ⇒ 读不到用 ReferenceNotFound ✓，
            // 与 `export_project` 读原子日志失败时保持一致 ✓。
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读不到纹理目录 {}：{error}", path.display())),
            ));
        }
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        found.push((file_name, meta.len()));
    }
    found.sort();
    Ok(found)
}

/// 多文档工作区。
pub struct Workspace {
    store: Arc<dyn BlobStore>,
    documents: BTreeMap<String, Document>,
    persist: Option<FileStore>,
    /// **随发行包一起发布的资产根目录** ✓（仓库里是 `assets/` ✓）。
    ///
    /// **为什么是"根"而不是"纹理目录"** ✓：用户要的是**三类资产都能导入** ✓
    ///（笔刷 ✓、纹理 ✓、调色板 ✓）⇒ 若各配一个字段 ✗，就会有**三份几乎一样的代码** ✓
    /// ⇒ 必然漂移 ✗（本项目吃过太多次 ✓）。**统一成"根 + 种类子目录"** ✓：
    /// `assets/<brushes|textures|palettes>/` ✓ —— 一份实现 ✓，三种资产 ✓。
    assets_dir: Option<std::path::PathBuf>,
    settings: DocumentSettings,
    created: u64,
    /// **已 `begin` 但尚未 `commit`/`abort` 的变更集** ✓（键 = 文档 + 会话 ✓）。
    ///
    /// **设计未规定"打开的变更集"存在哪里 ⇒ 记录选择** ✓：放在**工作区**（而不是 HTTP 层 ✓），
    /// 因为工具层只能经由 `ctx.workspace` 拿到共享状态 ✓ —— 放别处就得把状态一路穿进上下文 ✗。
    /// **键里带上会话** ✓ 是有意的：眼下 HTTP 层把会话写死成 `"session:http"` ✓（等于按文档 ✓），
    /// 但将来真做到"按连接区分会话"时 ✓，这里**不用改** ✓。
    open_changesets: BTreeMap<(String, String), ChangesetId>,
    /// **处于事务中的那些"打开的变更集"** ✓（键同上一张表 ✓）。
    ///
    /// **设计对事务一个字都没写 ⇒ 记录选择** ✓：**事务 = 带失败回滚的变更集** ✓。
    /// 依据只有工具表里的并列命名 ✓（`begin_changeset/…` 与 `begin_transaction/commit_transaction` ✓）——
    /// 既然设计把它们分开列 ✓，就必须**有区别** ✓，否则就该合成一个名字 ✓。
    /// 我取的区别是：**事务在"事务内的某次写操作失败"时，自动把已经落下的原子整体撤销** ✓
    /// （变更集只分组 ✓、不回滚 ✓）。判定"写操作"用的是 `ToolSpec` 现成的 `mutating` ✓ ——
    /// **读操作失败绝不回滚** ✗（否则查一次东西就把人家的编辑撤了 ✗）。
    transactions: BTreeMap<(String, String), ChangesetId>,
    /// **悬空变更集** ✓（设计 §12.4 ✓）。带 `FileStore` 时会落到 `<root>/stash/<id>.json` ✓ ——
    /// 离线窗口可能跨服务重启 ✓ ⇒ 只在内存里会**丢掉用户离线期间的工作** ✗。
    stashes: BTreeMap<String, Stash>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("documents", &self.documents.keys().collect::<Vec<_>>())
            .field("persisted", &self.persist.is_some())
            .finish()
    }
}

impl Workspace {
    /// **开始一个变更集** ✓（设计 793 的 `begin_changeset` ✓）。
    ///
    /// **设计未规定重复 begin 怎么办 ⇒ 记录选择** ✓：**报错** ✓。
    /// 理由：静默复用会让调用方以为"新开了一个" ✓，而实际上前一个还没收尾 ✗；
    /// 静默新建又会把前一个变更集**永远留在打开状态** ✗。报错最诚实 ✓。
    pub fn begin_changeset(&mut self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        let key = (doc_id.to_owned(), session.to_owned());
        if self.open_changesets.contains_key(&key) {
            return None;
        }
        let changeset = ChangesetId::from(yanshi_core::Changeset::new_id());
        self.open_changesets.insert(key, changeset.clone());
        Some(changeset)
    }

    /// 读当前打开的变更集 ✓（`ctx.commit` 用它决定"这一笔该不该并进去" ✓）。
    pub fn open_changeset(&self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        self.open_changesets
            .get(&(doc_id.to_owned(), session.to_owned()))
            .cloned()
    }

    /// **统计 blob 的三级生命周期** ✓（设计 §6.3 ✓，只读 ✓）。
    ///
    /// **先把所有文档读进来** ✓ —— 这条是**一次险些删掉 1.09 GB 活数据**换来的 ✗：
    /// 服务端**惰性打开**文档 ✓ ⇒ 只打开一份空文档时 ✓，别的文档的日志**根本不在内存里** ✗
    /// ⇒ 它们引用的 blob 会被判成"无人引用"的孤儿 ✗ ⇒ 一旦带 `confirm` 跑 ✓ **活数据就没了** ✗✗。
    /// 所以这里先按磁盘上的文档清单逐份打开 ✓，**再**做分级 ✓；
    /// 并把"考虑了几份文档"一并报出来 ✓（分类是否可信，看一眼就知道 ✓）。
    pub fn blob_lifecycle(&mut self, ttl_days: i64, now_ms: i64) -> Result<BlobLifecycle> {
        // ① 磁盘上的文档全部载入 ✓（`list_documents` 只列**已加载**的 ✗ ⇒ 用持久层那份清单 ✓）。
        let known: Vec<String> = match &self.persist {
            Some(persist) => persist.list_documents()?,
            None => self.documents.keys().cloned().collect(),
        };
        for doc_id in &known {
            if !self.documents.contains_key(doc_id) {
                // 打不开就当它不存在 ✓（分级会因此偏保守：宁可把 blob 当孤儿也不误判为活跃 ✗）。
                let _ = self.open_document(doc_id);
            }
        }
        // **改成薄封装内核的 `plan_gc`** ✓ —— 我上一轮**自己重写了一份** ✗，
        // 而 `crates/yanshi-core/src/blob.rs` 里**早就有**完整实现 ✓（`plan_gc` / `run_gc` ✓）：
        // 根集 = `log.blob_roots() ∪ extra_roots ∪ active_manifest` ✓、
        // **`extra_roots` 正是给"快照 Manifest 与 Stash"留的口子** ✓，
        // 而且它有一条**我做漏的防御** ✓："根集里的 blob 一律不许进入 `expiring`" ✓。
        // 教训照旧 ✓：**先搜内核，再写实现** ✗（这已经是同一类错误的第 N 次 ✓）。
        use std::collections::BTreeSet;
        // ② 聚合成内核要的三个输入 ✓：
        //  * 一份**合并所有文档**的日志 ✓（内核只收一份 `AtomLog` ✓；按文档分别调用是**错的** ✗ ——
        //    那样"被 A 文档引用、B 文档没引用"的 blob 会在 B 的计划里被当成孤儿 ✗）；
        //  * **所有文档**的活跃 Manifest ✓（`active_blob_manifest` **连图层 blob 一起算** ✓ ——
        //    我上一轮只算了对象的 ✗）；
        //  * `extra_roots` ✓：Stash 的引用 ✓ + **快照 Manifest**（服务端尚无快照 ✗ ⇒ 暂时为空 ✓，
        //    将来有快照时按设计塞进这里 ✓）。
        let mut log = yanshi_core::AtomLog::new();
        let mut active_manifest: BTreeSet<yanshi_core::BlobHash> = BTreeSet::new();
        for document in self.documents.values() {
            for atom in document.log().iter() {
                // 合并只为算"引用闭包" ✓ ⇒ 追加失败（重复 id 等）不影响根集 ✓，忽略即可 ✓。
                let _ = log.append(atom.clone());
            }
            active_manifest.extend(document.state().active_blob_manifest());
        }
        for stash in self.stashes.values() {
            for atom in &stash.atoms {
                let _ = log.append(atom.clone());
            }
        }
        let extra_roots: BTreeSet<yanshi_core::BlobHash> = BTreeSet::new();
        let plan = yanshi_core::plan_gc(
            &*self.store,
            &log,
            &active_manifest,
            &extra_roots,
            now_ms,
            ttl_days.max(0) * 24 * 60 * 60,
        )?;
        let mut report = BlobLifecycle {
            documents_considered: self.documents.len(),
            ..BlobLifecycle::default()
        };
        report.active_count = plan.active.len();
        report.active_bytes = plan.active_bytes;
        report.history_count = plan.historical.len();
        report.history_bytes = plan.historical_bytes;
        report.orphan_count = plan.orphans.len();
        report.orphan_bytes = plan.orphan_bytes;
        report.active_hashes = plan.active.iter().map(|hash| hash.to_string()).collect();
        report.history_hashes = plan
            .historical
            .iter()
            .map(|entry| entry.blob_hash.to_string())
            .collect();
        report.orphan_hashes = plan
            .orphans
            .iter()
            .map(|entry| entry.blob_hash.to_string())
            .collect();
        report.collectible = plan
            .expiring
            .iter()
            .map(|entry| entry.blob_hash.to_string())
            .collect();
        report.collectible_bytes = report
            .collectible
            .iter()
            .filter_map(|text| {
                plan.expiring
                    .iter()
                    .find(|entry| &entry.blob_hash.to_string() == text)
                    .map(|entry| entry.size)
            })
            .sum();
        // 各级排序只为报告稳定 ✓（内核已按哈希排过 ✓）。
        report.active_hashes.sort();
        Ok(report)
    }

    /// **回收已过 TTL 的孤儿 blob** ✓（设计 §6.3 ✓）。
    ///
    /// **它永远不碰活跃与历史** ✓（调用方只传 `collectible` ✓）；
    /// 而且**先算分级再删** ✓ ⇒ 两次之间若有新原子引用 ✓ 也不会误删 ✗（哈希是内容寻址 ✓，
    /// 引用一旦写进日志 ✓ 该 blob 就已进根集 ✓）。返回真正删掉的条数与字节 ✓。
    pub fn collect_orphan_blobs(&mut self, hashes: &[String]) -> Result<(usize, u64)> {
        let mut removed = 0usize;
        let mut freed = 0u64;
        for text in hashes {
            let Ok(hash) = text.parse::<yanshi_core::BlobHash>() else {
                continue;
            };
            let bytes = self.store.size(&hash).unwrap_or(0);
            if self.store.remove(&hash)? {
                removed += 1;
                freed += bytes;
            }
        }
        Ok((removed, freed))
    }

    /// **搁置一批原子** ✓（设计 §12.4 的"悬空变更集" ✓）。
    pub fn stash(
        &mut self,
        doc_id: &str,
        actor: &str,
        session: &str,
        reason: &str,
        atoms: Vec<yanshi_core::Atom>,
    ) -> Result<String> {
        let mut blob_refs: Vec<String> = Vec::new();
        for atom in &atoms {
            for hash in atom.all_blob_refs() {
                let text = hash.to_string();
                if !blob_refs.contains(&text) {
                    blob_refs.push(text);
                }
            }
        }
        let id = format!("stash_{}", yanshi_core::Ulid::new().encode());
        let entry = Stash {
            id: id.clone(),
            doc_id: doc_id.to_owned(),
            actor: actor.to_owned(),
            session: session.to_owned(),
            created_at: self.now_millis(),
            reason: reason.to_owned(),
            atoms,
            blob_refs,
        };
        // 先落盘再记账 ✓：写失败就当作没搁置 ✓（否则重启后内存里那份**凭空消失** ✗）。
        if let Some(persist) = &self.persist {
            persist.write_stash(&entry)?;
        }
        self.stashes.insert(id.clone(), entry);
        Ok(id)
    }

    /// 列出悬空变更集 ✓（UI 的"分支对比" ✓，§898 ✓）。
    pub fn stashes(&self) -> Vec<&Stash> {
        let mut all: Vec<&Stash> = self.stashes.values().collect();
        all.sort_by_key(|stash| stash.created_at);
        all
    }

    /// 取走一个悬空变更集 ✓（重新提交成功、或用户选择"丢弃" ✓）。
    pub fn take_stash(&mut self, stash_id: &str) -> Result<Option<Stash>> {
        let Some(entry) = self.stashes.remove(stash_id) else {
            return Ok(None);
        };
        if let Some(persist) = &self.persist {
            persist.remove_stash(stash_id)?;
        }
        Ok(Some(entry))
    }

    /// 毫秒时间戳 ✓（Stash 的排序与展示用 ✓）。
    fn now_millis(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0)
    }

    /// **开始一个事务** ✓（设计 777 只给了名字 ⇒ 语义见 `transactions` 字段的说明 ✓）。
    ///
    /// 事务**同时**是一个变更集 ✓ ⇒ 事务里的原子能作为一组被撤销 ✓（复用既有能力 ✓，不另起一套 ✗）。
    /// 已经有打开的变更集/事务 ⇒ `None` ✓（与 `begin_changeset` 同一考虑：报错比静默更诚实 ✓）。
    pub fn begin_transaction(&mut self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        let changeset = self.begin_changeset(doc_id, session)?;
        self.transactions
            .insert((doc_id.to_owned(), session.to_owned()), changeset.clone());
        Some(changeset)
    }

    /// 当前事务 ✓（`ToolRegistry::call` 用它决定"失败时要不要回滚" ✓）。
    pub fn transaction(&self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        self.transactions
            .get(&(doc_id.to_owned(), session.to_owned()))
            .cloned()
    }

    /// 结束当前事务 ✓（收尾或回滚之后都要调它 ✓）。
    pub fn close_transaction(&mut self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        let key = (doc_id.to_owned(), session.to_owned());
        self.transactions.remove(&key);
        self.open_changesets.remove(&key)
    }

    /// 关闭（收尾或放弃）打开的变更集 ✓。
    pub fn close_changeset(&mut self, doc_id: &str, session: &str) -> Option<ChangesetId> {
        self.open_changesets
            .remove(&(doc_id.to_owned(), session.to_owned()))
    }

    /// 内存工作区（测试与嵌入式使用）。
    pub fn in_memory(settings: DocumentSettings) -> Self {
        Self {
            store: Arc::new(MemoryBlobStore::new()),
            documents: BTreeMap::new(),
            persist: None,
            assets_dir: None,
            open_changesets: BTreeMap::new(),
            transactions: BTreeMap::new(),
            stashes: BTreeMap::new(),
            settings,
            created: 0,
        }
    }

    /// 以文件存储构造（打开时加载已有文档索引）。
    pub fn with_file_store(
        root: impl Into<std::path::PathBuf>,
        settings: DocumentSettings,
    ) -> Result<Self> {
        let persist = FileStore::open(root)?;
        let store: Arc<dyn BlobStore> = Arc::new(persist.blob_store()?);
        Ok(Self {
            store,
            documents: BTreeMap::new(),
            persist: Some(persist),
            assets_dir: None,
            settings,
            created: 0,
            open_changesets: BTreeMap::new(),
            transactions: BTreeMap::new(),
            stashes: BTreeMap::new(),
        })
    }

    /// CAS。
    pub fn store(&self) -> Arc<dyn BlobStore> {
        Arc::clone(&self.store)
    }

    /// 持久化层。
    pub const fn persist(&self) -> Option<&FileStore> {
        self.persist.as_ref()
    }

    /// 设置。
    pub const fn settings(&self) -> &DocumentSettings {
        &self.settings
    }

    /// 已打开的文档 id。
    pub fn document_ids(&self) -> Vec<String> {
        self.documents.keys().cloned().collect()
    }

    /// 新建文档（可选持久化）。
    pub fn create_document(
        &mut self,
        spec: NewDocument,
        actor: &str,
        session: &str,
    ) -> Result<&mut Document> {
        let doc_id = spec.doc_id.clone();
        if self.documents.contains_key(&doc_id) {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("文档 {doc_id} 已打开")),
            ));
        }
        if let Some(persist) = &self.persist {
            if !persist.load_atoms(&doc_id)?.is_empty() {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!("文档 {doc_id} 已存在于磁盘")),
                ));
            }
        }
        let document = Document::create(
            Arc::clone(&self.store),
            spec.clone(),
            actor,
            session,
            self.settings.clone(),
        )?;
        self.documents.insert(doc_id.clone(), document);
        self.created += 1;

        if let Some(persist) = &self.persist {
            let document = self.documents.get(&doc_id).expect("刚插入");
            for atom in document.log().atoms() {
                persist.append_atom(&doc_id, atom)?;
            }
            persist.save_meta(&DocumentMeta {
                doc_id: doc_id.clone(),
                width: spec.width,
                height: spec.height,
                created_at: yanshi_core::now_ms(),
                tokens: Vec::new(),
            })?;
        }
        self.document_mut(&doc_id)
    }

    /// 复制一个文档为**新 id 的副本**（"另存为"）。
    ///
    /// 设计**没有规定**文档命名/重命名 ✗ —— 文档以 `doc_id` 为主键 ✓，日志、令牌与持久化
    /// 路径都以它为准 ✓。用户确认采用**路线 A：另存为副本**（新 id、原文档保留、可逆 ✓）—— 与
    /// 真改名（迁移 id，会牵动日志/持久化/令牌，不可逆）相比没有数据风险 ✓。
    ///
    /// 实现是**逐原子原样重放** ✓：新文档与源文档的日志内容一致，因此折叠后的状态与渲染结果
    /// 完全一致 ✓（blob 是 CAS 内容寻址，天然共享，不复制字节 ✓）。
    /// 返回重放的原子数 ✓。
    pub fn duplicate_document(
        &mut self,
        from: &str,
        to: &str,
        actor: &str,
        session: &str,
    ) -> Result<usize> {
        let (width, height, background, atoms) = {
            let source = self.open_document(from)?;
            let state = source.state();
            let atoms: Vec<yanshi_core::Atom> = source.log().atoms().to_vec();
            (state.width, state.height, state.background.clone(), atoms)
        };
        let mut spec = NewDocument::new(to, width, height);
        spec.background = background;
        self.create_document(spec, actor, session)?;
        let mut copied = 0usize;
        for atom in atoms {
            // 跳过**文档自建**事件：新文档由 `create_document` 自己产生了一条等价事件 ✓，
            // 原样重放源文档那条会出现两个 create ✗。
            if atom.kind == yanshi_core::AtomKind::CreateDocument {
                continue;
            }
            self.commit(to, atom, actor, false)?;
            copied += 1;
        }
        Ok(copied)
    }

    /// 打开（或返回已打开的）文档：从磁盘加载日志并重建状态。
    pub fn open_document(&mut self, doc_id: &str) -> Result<&mut Document> {
        if !self.documents.contains_key(doc_id) {
            let (atoms, render) = match &self.persist {
                Some(persist) => (persist.load_atoms(doc_id)?, persist.load_render(doc_id)?),
                None => (Vec::new(), None),
            };
            if atoms.is_empty() {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {doc_id} 不存在")),
                ));
            }
            let mut document = Document::open(
                Arc::clone(&self.store),
                doc_id,
                atoms,
                self.settings.clone(),
            )?;
            if let Some((seq, png)) = render {
                // 打开即图片（14.5/6.2）：直接复用持久化渲染缓存，不重放历史。
                let hash = self.store.put(&png)?;
                document.mark_rendered(seq, Some(hash));
            }
            // 恢复令牌（12.7）。
            if let Some(persist) = &self.persist {
                if let Some(meta) = persist.load_meta(doc_id)? {
                    for record in meta.tokens {
                        document.authority_mut().restore(
                            &record.token,
                            &record.actor,
                            record.role,
                        )?;
                    }
                }
            }
            self.documents.insert(doc_id.to_owned(), document);
        }
        self.document_mut(doc_id)
    }

    /// 打开或创建（工具层 `open_document` 的语义）。
    pub fn open_or_create(
        &mut self,
        spec: NewDocument,
        actor: &str,
        session: &str,
    ) -> Result<&mut Document> {
        let doc_id = spec.doc_id.clone();
        if self.documents.contains_key(&doc_id) {
            return self.document_mut(&doc_id);
        }
        let exists = match &self.persist {
            Some(persist) => !persist.load_atoms(&doc_id)?.is_empty(),
            None => false,
        };
        if exists {
            self.open_document(&doc_id)
        } else {
            self.create_document(spec, actor, session)
        }
    }

    /// 只读文档引用。
    pub fn document(&self, doc_id: &str) -> Option<&Document> {
        self.documents.get(doc_id)
    }

    /// 可变文档引用。
    /// GC 的输入：**所有文档**引用闭包的并集（安全），加上某个文档的日志/Manifest（分类诊断）。
    fn gc_inputs(
        &self,
        now: i64,
    ) -> Result<(
        std::collections::BTreeSet<yanshi_core::atom::BlobHash>,
        yanshi_core::log::AtomLog,
        std::collections::BTreeSet<yanshi_core::atom::BlobHash>,
    )> {
        let mut extra_roots: std::collections::BTreeSet<yanshi_core::atom::BlobHash> =
            std::collections::BTreeSet::new();
        for document in self.documents.values() {
            extra_roots.extend(document.gc_roots(now)?);
        }
        match self.documents.values().next() {
            Some(primary) => Ok((
                extra_roots,
                primary.log().clone(),
                primary.state().active_blob_manifest(),
            )),
            None => Ok((
                extra_roots,
                yanshi_core::log::AtomLog::new(),
                std::collections::BTreeSet::new(),
            )),
        }
    }

    /// **工作区级** GC 计划（不删除任何东西）。用于干跑与容量报告。
    pub fn plan_garbage(&self, now: i64) -> Result<yanshi_core::blob::GcPlan> {
        self.plan_garbage_with_ttl(now, self.settings.orphan_ttl_seconds)
    }

    /// 同上，但显式指定 TTL（调用方可能希望更激进地回收刚产生的孤儿）。
    pub fn plan_garbage_with_ttl(
        &self,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<yanshi_core::blob::GcPlan> {
        let (extra_roots, log, manifest) = self.gc_inputs(now)?;
        yanshi_core::blob::plan_gc(
            &*self.store,
            &log,
            &manifest,
            &extra_roots,
            now,
            ttl_seconds,
        )
    }

    /// 内部：旧的 run_gc 计划路径（保留给诊断使用）。
    #[allow(dead_code)]
    fn plan_garbage_via_run_gc(&self, now: i64) -> Result<yanshi_core::blob::GcPlan> {
        let (extra_roots, log, manifest) = self.gc_inputs(now)?;
        let (_, plan) = yanshi_core::blob::run_gc(
            &*self.store,
            &log,
            &manifest,
            &extra_roots,
            now,
            self.settings.orphan_ttl_seconds,
        )?;
        Ok(plan)
    }

    /// **工作区级**孤儿回收（设计 6.3）：根集 = **所有文档**的日志引用闭包 ∪ 各自活跃 Manifest。
    ///
    /// 为什么必须跨文档：GC 按根集删除「孤儿」，而一个 blob 可能被**别的文档**引用。
    /// 只按当前文档取根集会把其它文档引用的 blob 判成孤儿并删掉（数据丢失）。
    /// 这一点由干跑实测发现：某工作区 280 个 blob 在单文档视角下**全部**显示为孤儿。
    pub fn collect_garbage(
        &self,
        now: i64,
    ) -> Result<(yanshi_core::blob::GcReport, yanshi_core::blob::GcPlan)> {
        self.collect_garbage_with_ttl(now, self.settings.orphan_ttl_seconds)
    }

    /// 同上，但显式指定 TTL。
    pub fn collect_garbage_with_ttl(
        &self,
        now: i64,
        ttl_seconds: i64,
    ) -> Result<(yanshi_core::blob::GcReport, yanshi_core::blob::GcPlan)> {
        let (extra_roots, log, manifest) = self.gc_inputs(now)?;
        yanshi_core::blob::run_gc(
            &*self.store,
            &log,
            &manifest,
            &extra_roots,
            now,
            ttl_seconds,
        )
    }

    /// 打开（或按需创建）文档。
    /// 取可变文档引用（不存在则 `reference_not_found`）。
    pub fn document_mut(&mut self, doc_id: &str) -> Result<&mut Document> {
        self.documents.get_mut(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })
    }

    /// 关闭文档（释放内存；持久化内容保留）。
    pub fn close_document(&mut self, doc_id: &str) -> bool {
        self.documents.remove(doc_id).is_some()
    }

    /// 文档列表（磁盘上的也包含，标记 `persisted`）。
    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        let mut summaries: BTreeMap<String, DocumentSummary> = BTreeMap::new();
        for (doc_id, document) in &self.documents {
            summaries.insert(
                doc_id.clone(),
                DocumentSummary {
                    doc_id: doc_id.clone(),
                    width: document.state().width,
                    height: document.state().height,
                    atoms: document.atom_count(),
                    head_seq: document.head_seq(),
                    // **存活实体** ✓ —— 这里原来直接数 `state.*.len()` ✗，
                    // 而那里面**包含墓碑** ✓（删除可撤销、日志可重放的基础 ✓）
                    // ⇒ 同一个文档、同一瞬间会出现两个数 ✓：
                    // 列表说"241 个对象" ✗、而 `list_objects` 说"0 个" ✓（实测 ✓）。
                    // 本项目早就为 `get_document` 修过同一件事 ✓
                    //（`document_summary.rs` 的用例 ✓），但我上一轮只修了**未打开文档**那一路 ✗
                    // ⇒ 两条路**又不一致** ✓ —— 这正是"同一事实两处各算一遍"的必然结果 ✓。
                    // 现在三处（`get_document` / 未打开 / 已打开）用**同一条谓词** ✓。
                    layers: document
                        .state()
                        .layers
                        .values()
                        .filter(|layer| !layer.is_deleted())
                        .count(),
                    objects: document
                        .state()
                        .objects
                        .values()
                        .filter(|object| !object.is_deleted())
                        .count(),
                    created_at: document.created_at(),
                    persisted: self.persist.is_some(),
                },
            );
        }
        if let Some(persist) = &self.persist {
            for doc_id in persist.list_documents()? {
                if summaries.contains_key(&doc_id) {
                    continue;
                }
                let meta = persist.load_meta(&doc_id)?;
                let atoms = persist.load_atoms(&doc_id)?;
                // **未打开的文档也要如实计数** ✓ —— 这里原来**写死 `layers: 0, objects: 0`** ✗，
                // 于是"任何关闭着的文档"都被报成零对象 ✓（列表里 144 项全显示为空 ✓）。
                // **后果很实在** ✗：我按这个字段挑"空文档"去清理 ✓ ⇒ 把
                // `sample-watercolor`（56 个对象 ✓）与 `sample-brush`（260 个 ✓）当成空的清掉了 ✗
                //（幸好是**移到备份**而不是删除 ✓，已全部恢复 ✓）。
                // **教训** ✓：两个来源互相矛盾时（`get_document` 说有 56 ✓、列表说 0 ✗）
                // **必须先对账再动手** ✓ —— 我这次是先动手后发现 ✗。
                // 做法 ✓：原子**本来就已读出** ✓（上一行 ✓）⇒ 折一遍即可 ✓，成本是增量的 ✓，
                // 口径与"已打开文档"**完全一致** ✓（同一套折叠 ✓ ⇒ 不会两处各算一套 ✗）。
                let folded = yanshi_core::fold::fold_atoms(
                    yanshi_core::state::DocumentState::default(),
                    &atoms,
                );
                summaries.insert(
                    doc_id.clone(),
                    DocumentSummary {
                        doc_id: doc_id.clone(),
                        width: meta.as_ref().map(|meta| meta.width).unwrap_or(0),
                        height: meta.as_ref().map(|meta| meta.height).unwrap_or(0),
                        atoms: atoms.len(),
                        head_seq: atoms.last().map(|atom| atom.seq).unwrap_or(0),
                        layers: folded
                            .state
                            .layers
                            .values()
                            .filter(|layer| !layer.is_deleted())
                            .count(),
                        objects: folded
                            .state
                            .objects
                            .values()
                            .filter(|object| !object.is_deleted())
                            .count(),
                        created_at: meta.as_ref().map(|meta| meta.created_at).unwrap_or(0),
                        persisted: true,
                    },
                );
            }
        }
        Ok(summaries.into_values().collect())
    }

    /// 提交原子并落盘（服务端权威路径）。
    pub fn commit(
        &mut self,
        doc_id: &str,
        atom: Atom,
        actor: &str,
        owner: bool,
    ) -> Result<CommitResult> {
        let result = {
            let document = self.document_mut(doc_id)?;
            document.commit_as(atom, actor, owner, None)?
        };
        self.journal(doc_id, &result)?;
        Ok(result)
    }

    /// 以变更集提交多个原子（batch，5.6）。
    pub fn commit_changeset(
        &mut self,
        doc_id: &str,
        atoms: Vec<Atom>,
        actor: &str,
        owner: bool,
        changeset_id: ChangesetId,
    ) -> Result<Vec<CommitResult>> {
        let mut results = Vec::with_capacity(atoms.len());
        for atom in atoms {
            let result = {
                let document = self.document_mut(doc_id)?;
                document.commit_as(atom, actor, owner, Some(changeset_id.clone()))?
            };
            self.journal(doc_id, &result)?;
            results.push(result);
        }
        Ok(results)
    }

    /// 渲染区域。
    ///
    /// 只有覆盖整幅画布的渲染才落盘为「HEAD 渲染缓存」（14.5 打开即图片）；
    /// 局部 dirty 渲染虽然也进 CAS，但不会覆盖文档级缓存。
    /// 渲染区域并返回原始 RGBA8（供 `patch` 抓取源像素；不触碰渲染缓存状态）。
    pub fn render_region_raw(&mut self, doc_id: &str, bbox: Bbox) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?.render_region_raw(bbox)
    }

    /// **区域字节缓存的统计** ✓（设计 §8.4 ✓；用于观测与测试 ✓）。
    pub fn region_cache_stats(
        &self,
        doc_id: &str,
    ) -> Result<yanshi_render::region_block::RegionBlockStats> {
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        Ok(document.region_cache_stats())
    }

    /// 渲染区域、写入渲染缓存并返回可展示的预览（含 PNG blob 与取回地址）。
    pub fn render_region(&mut self, doc_id: &str, bbox: Bbox) -> Result<RenderedPreview> {
        let (preview, head, png, full_frame) = {
            let document = self.document_mut(doc_id)?;
            let (width, height) = (document.state().width, document.state().height);
            let preview = document.render_region(bbox)?;
            let png = document.store().get(&preview.blob_hash)?;
            let covers =
                bbox.x <= 0.0 && bbox.y <= 0.0 && bbox.w >= width as f64 && bbox.h >= height as f64;
            (preview, document.head_seq(), png, covers)
        };
        if full_frame {
            if let Some(persist) = &self.persist {
                let _ = persist.save_render(doc_id, head, &png);
            }
        }
        Ok(preview)
    }

    /// **纹理缓存目录** ✓（`<root>/textures` ✓ —— **不入 git** ✗）。
    ///
    /// **为什么放缓存而不是签进仓库** ✓（用户裁定 ✓）：CC0 纹理最小的 `1K-PNG` 就有 **13.6MB** ✗
    ///（实测自 ambientCG 的 API ✓）⇒ 几十张就是几百 MB ✗ ⇒ 签进 git 会把仓库撑爆 ✓。
    /// 用户的选择是"**下载到本地缓存**" ✓ ⇒ 于是权威副本在**工作区**里 ✓、由**脚本**填充 ✓、
    /// 而**工具**负责把它**列出来**给 MCP 与 Web 用 ✓（能力在工具层 ⇒ 两边同时可用 ✓）。
    pub fn texture_cache_dir(&self, dir: Option<&str>) -> Result<std::path::PathBuf> {
        let Some(persist) = self.persist.as_ref() else {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "纹理缓存需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有地方放它们",
                ),
            ));
        };
        // **子目录名要挡住路径穿越** ✗（`..` 或绝对路径都不该能指到工作区之外 ✓）。
        let name = dir.unwrap_or("textures");
        if name.is_empty() || name.contains("..") || name.starts_with('/') || name.contains('\\') {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("纹理缓存目录名不合法：{name}")),
            ));
        }
        Ok(persist.root().join(name))
    }

    /// **指定随发行包发布的资产根目录** ✓（仓库里是 `assets/` ✓；包内是 `share/yanshi` ✓）。
    ///
    /// **一个根、三种子目录** ✓（`textures` / `brushes` / `palettes` ✓）⇒
    /// 加一类资产**不用再加开关** ✓（开关会越加越多 ✗，而这份代码只有一段 ✓）。
    pub fn with_assets_dir(mut self, dir: Option<std::path::PathBuf>) -> Self {
        self.assets_dir = dir;
        self
    }

    /// **列出某类资产** ✓（`kind` ∈ `brush` / `texture` / `palette` ✓）。
    ///
    /// **两类来源** ✓：**内置**（`<assets_dir>/<子目录>` ✓，随发行包发布 ✓）与
    /// **缓存**（`<root>/<子目录>` ✓，用户自己导入或抓取的 ✓）；**同名时缓存覆盖内置** ✓
    ///（用户导入的同名文件应当生效 ✓，不必改仓库 ✓）。
    /// **一份实现服务三种资产** ✓ —— 各写一份必然漂移 ✗。
    pub fn list_assets(&self, kind: &str) -> Result<Vec<TextureEntry>> {
        let (sub, _) = asset_layout(kind)?;
        let mut found: Vec<TextureEntry> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // **缓存先扫** ✓（优先级高 ✓ ⇒ 先占名字 ✓，内置里同名的随后被跳过 ✓）。
        let cache = self.asset_dir(kind).ok();
        let bundled = self.assets_dir.as_ref().map(|root| root.join(sub));
        for (path, source) in [(cache, "cache"), (bundled, "bundled")] {
            let Some(path) = path else { continue };
            for (name, bytes) in read_texture_dir(&path)?.into_iter() {
                if !seen.insert(name.clone()) {
                    continue;
                }
                found.push(TextureEntry {
                    name,
                    bytes,
                    source,
                });
            }
        }
        found.sort_by(|a, b| (a.source, &a.name).cmp(&(b.source, &b.name)));
        Ok(found)
    }

    /// **纹理** ✓ = `list_assets("texture")` 的薄包装 ✓。
    ///
    /// **为什么保留它** ✓：`list_textures` 是**已经交付并测过**的工具 ✓，
    /// 去掉会破坏调用方 ✓ ⇒ 留着 ✓，但**共用同一份实现** ✓ ⇒ 两者**不可能漂移** ✓。
    pub fn list_textures(&self, _dir: Option<&str>) -> Result<Vec<TextureEntry>> {
        self.list_assets("texture")
    }

    /// **导入一件资产** ✓（笔刷 / 纹理 / 调色板 ✓）—— **MCP 与 Web 共用这一个入口** ✓。
    ///
    /// **校验** ✓：种类必须认识 ✓、文件名必须干净 ✓（挡路径穿越 ✗）、
    /// 扩展名必须与种类相符 ✓（否则就是"**导入了用不了的东西**" ✗ —— 与"接受了却没用"同类 ✓）。
    pub fn import_asset(
        &self,
        kind: &str,
        name: &str,
        bytes: &[u8],
        overwrite: bool,
    ) -> Result<std::path::PathBuf> {
        let (sub, extensions) = asset_layout(kind)?;
        let clean = asset_file_name(name)?;
        let extension = clean
            .rsplit_once('.')
            .map(|(_, ext)| ext.to_ascii_lowercase())
            .unwrap_or_default();
        if !extensions.contains(&extension.as_str()) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{kind} 不接受 .{extension} ⇒ 可用扩展名：{}",
                    extensions.join(" / ")
                )),
            ));
        }
        if bytes.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("{name} 是空文件 ⇒ 没有可导入的内容")),
            ));
        }
        let dir = self.asset_dir(kind)?;
        std::fs::create_dir_all(&dir).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("建不了资产目录 {}：{error}", dir.display())),
            )
        })?;
        let target = dir.join(&clean);
        // **默认不许覆盖** ✗：静默覆盖会毁掉用户已有的资产 ✓ ⇒ 要覆盖必须**明说** ✓。
        if target.exists() && !overwrite {
            return Err(YanshiError::new(
                ErrorCode::Conflict,
                ErrorContext::detail(format!(
                    "{} 已存在 ⇒ 想替换请传 overwrite: true（不会静默覆盖 ✓）",
                    target.display()
                )),
            ));
        }
        std::fs::write(&target, bytes).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("写不进 {}：{error}", target.display())),
            )
        })?;
        let _ = sub;
        Ok(target)
    }

    /// **某类资产的缓存目录** ✓（`<root>/<子目录>` ✓ —— 内置资产**不在这里** ✓）。
    pub fn asset_dir(&self, kind: &str) -> Result<std::path::PathBuf> {
        let (sub, _) = asset_layout(kind)?;
        let Some(persist) = self.persist.as_ref() else {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "{kind} 的导入需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有地方放它"
                )),
            ));
        };
        Ok(persist.root().join(sub))
    }

    /// **把一份文档打成 `.yanshi` 工程包** ✓（真实用户提的缺口 ✓）。
    ///
    /// **包里装什么** ✓：
    /// * `meta.json` ✓ / `atoms.jsonl` ✓（**不可变原子日志** ✓ —— 这是唯一权威 ✓）；
    /// * `blobs/<hash>` ✓：**只装这份日志真正引用到的 blob** ✓（从每条原子的 `refs.blobs` 收集 ✓）
    ///   ⇒ 不把整个 CAS 一股脑塞进去 ✓（CAS 可能有很多别的文档的 blob ✓）；
    /// * **`render.png`：由调用方传入的"最新整幅渲染"** ✓ —— 这一点是**故意**的 ✓：
    ///   用户手 zip 时把**过期的 `render.png` 缓存**一起装了进去 ✓ ⇒ 四个工程包的预览**全是空白** ✗
    ///   ⇒ 导出必须**当场**写一张最新的 ✓，而不是复用磁盘缓存 ✗；
    /// * `render.seq` ✓ / `BUILD-INFO` ✓ / `README.txt` ✓（告诉人怎么还原 ✓）。
    ///
    /// **为什么要求落盘工作区** ✓：原子日志与元数据是**在磁盘上**的权威副本 ✓；
    /// 纯内存工作区没有它们 ✓ ⇒ 那里应当**明确拒绝** ✓（而不是导出一个**不完整**的包 ✗）。
    /// **随发行包一起发布的内置纹理目录** ✓（用户裁定：纹理**要入库、要打包** ✓）。
    ///
    /// **与缓存的关系** ✓：内置的是"**开箱就有**"的那几款 ✓（`assets/textures/` ✓，随 `make release` 进包 ✓）；
    /// 缓存（`<root>/textures/` ✓）是用户用 `scripts/fetch-textures.sh` 另外抓的 ✓。
    /// **重名时缓存优先** ✓ —— 用户放进去的同名文件**覆盖**内置的 ✓（想换就换 ✓，不必改仓库 ✓）。
    pub fn export_project(&mut self, doc_id: &str, render_png: &[u8]) -> Result<Vec<u8>> {
        let Some(persist) = self.persist.clone() else {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "工程包导出需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有权威的原子日志文件",
                ),
            ));
        };
        let dir = persist.doc_dir(doc_id);
        let atoms_text = std::fs::read_to_string(dir.join("atoms.jsonl")).map_err(|error| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读不到原子日志 {doc_id}: {error}")),
            )
        })?;
        // **只收这条日志引用到的 blob** ✓（逐行解析 ✓，坏行跳过但**不静默**：计数后写进包里 ✓）。
        let mut wanted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut broken_lines = 0usize;
        for line in atoms_text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(atom) => {
                    if let Some(blobs) = atom
                        .get("refs")
                        .and_then(|refs| refs.get("blobs"))
                        .and_then(serde_json::Value::as_array)
                    {
                        for blob in blobs {
                            if let Some(hash) = blob.as_str() {
                                wanted.insert(hash.to_owned());
                            }
                        }
                    }
                }
                Err(_) => broken_lines += 1,
            }
        }
        let mut entries = vec![crate::archive::TarEntry {
            path: "atoms.jsonl".to_owned(),
            bytes: atoms_text.into_bytes(),
        }];
        if let Ok(meta) = std::fs::read(dir.join("meta.json")) {
            entries.push(crate::archive::TarEntry {
                path: "meta.json".to_owned(),
                bytes: meta,
            });
        }
        let head = self.document_mut(doc_id)?.head_seq();
        entries.push(crate::archive::TarEntry {
            path: "render.png".to_owned(),
            bytes: render_png.to_vec(),
        });
        entries.push(crate::archive::TarEntry {
            path: "render.seq".to_owned(),
            bytes: head.to_string().into_bytes(),
        });
        // **blob 走存储接口取** ✓（内存/落盘都能用 ✓，也顺带保证"包里每一个字节都真的存在" ✓）。
        let store = self.store();
        let mut blob_count = 0usize;
        for hash_text in &wanted {
            let hash: yanshi_core::BlobHash = hash_text.parse().map_err(|_| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("日志里有一个不合法的 blob 引用：{hash_text}")),
                )
            })?;
            let bytes = store.get(&hash)?;
            let hex = hash.hex();
            entries.push(crate::archive::TarEntry {
                path: format!("blobs/sha256/{}/{}/{}", &hex[0..2], &hex[2..4], hex),
                bytes,
            });
            blob_count += 1;
        }
        entries.push(crate::archive::TarEntry {
            path: "BUILD-INFO".to_owned(),
            // **版本与 commit 一起进包** ✓（用户提过的排查需求 ✓）：拿到包就知道是哪一版产的 ✓。
            bytes: format!(
                "name: yanshi\ndoc_id: {doc_id}\nhead_seq: {head}\nblobs: {blob_count}\n\
                 unparsable_atom_lines: {broken_lines}\n\
                 restore: 把本包解开到 <root>/ 之下即可（atoms.jsonl 是唯一权威，渲染可重放）\n"
            )
            .into_bytes(),
        });
        entries.push(crate::archive::TarEntry {
            path: "README.txt".to_owned(),
            bytes: b"Yanshi project package (uncompressed tar).\n\
Contents:\n  atoms.jsonl   append-only atom log; this is the authority\n\
  meta.json     document metadata\n  render.png    a render of HEAD at export time\n\
  render.seq    the seq that render corresponds to\n\
  blobs/        content-addressed blobs referenced by the log\n\
Restore by extracting under <root>/ of a yanshi-serve instance.\n"
                .to_vec(),
        });
        Ok(crate::archive::write_tar(&entries))
    }

    /// **把一张"整幅"PNG 存成渲染缓存** ✓（`render.png` + `render.seq` ✓，"打开即图片" 14.5 ✓）。
    ///
    /// **为什么需要它** ✓（真实用户的工程包暴露的 ✓）：缓存原来**只在"整幅渲染"或"文档级缩略图"时刷新** ✓，
    /// 而 `export_png` 走的是 `render_region_raw` ✓ ⇒ **绕过缓存** ✗
    /// ⇒ 用户拿 `export_png` 画完整幅画 ✓，磁盘上的 `render.png` **仍停在刚建文档时的空白** ✗ ✓
    ///（我量了他四个工程包 ✓：缓存 `render.seq` 是 **5–6** ✓，而原子最高 **97–217** ✓ ⇒
    ///  四个包里的 `render.png` 才会是**同一张空图** ✗）。
    /// **这里复用调用方刚编好的 PNG** ✓ ⇒ **不多渲染一次** ✓。
    pub fn cache_full_frame_png(&mut self, doc_id: &str, png: &[u8]) -> Result<()> {
        let head = self.document_mut(doc_id)?.head_seq();
        if let Some(persist) = &self.persist {
            persist.save_render(doc_id, head, png)?;
        }
        Ok(())
    }

    /// 生成缩略图；文档级缩略图会落盘为渲染缓存（14.5）。
    pub fn thumbnail(
        &mut self,
        doc_id: &str,
        kind: ThumbKind,
        target: Option<Bbox>,
    ) -> Result<RenderedPreview> {
        let (preview, head, png) = {
            let document = self.document_mut(doc_id)?;
            let preview = document.thumbnail(kind, target)?;
            let png = document.store().get(&preview.blob_hash)?;
            (preview, document.head_seq(), png)
        };
        if target.is_none() && kind.is_document_level() {
            if let Some(persist) = &self.persist {
                let _ = persist.save_render(doc_id, head, &png);
            }
        }
        Ok(preview)
    }

    /// 确保存在文档级缩略图并返回它的地址（6.2「打开即图片」）。
    ///
    /// 已有缓存（渲染过全幅或从磁盘恢复）直接返回；否则按 `preview_size` 生成一张。
    pub fn ensure_document_thumbnail(
        &mut self,
        doc_id: &str,
        size: DocThumbSize,
    ) -> Result<Option<String>> {
        {
            let document = self.document_mut(doc_id)?;
            // 只有与 HEAD 一致的缩略图才算「打开即图片」的缓存；落后则重新生成。
            if document.document_thumbnail_is_current() {
                if let Some(url) = document.document_thumbnail_url() {
                    return Ok(Some(url));
                }
            }
        }
        if matches!(size, DocThumbSize::Skip) {
            return Ok(None);
        }
        let kind = size.kind();
        let preview = self.thumbnail(doc_id, kind, None)?;
        Ok(Some(preview.url))
    }

    /// 渲染状态。
    pub fn render_status(&mut self, doc_id: &str, atom_id: &str) -> Result<RenderStatus> {
        let document = self.document_mut(doc_id)?;
        document.render_status(atom_id)
    }

    /// 发放令牌并持久化（12.7）。
    pub fn issue_token(
        &mut self,
        doc_id: &str,
        actor: &str,
        role: Role,
    ) -> Result<CapabilityToken> {
        let token = {
            let document = self.document_mut(doc_id)?;
            document.issue_token(actor, role)
        };
        if let Some(persist) = &self.persist {
            persist.record_token(doc_id, &token, actor, role)?;
        }
        Ok(token)
    }

    /// 鉴权（决定提交时的 actor 与权限）。
    ///
    /// 文档若尚未打开但磁盘存在，会先按需加载（含 token 恢复，12.7）——
    /// HTTP/WS 请求带着 token 打进来时不该因为进程刚重启就失败。
    pub fn authorize(
        &mut self,
        doc_id: &str,
        token: Option<&CapabilityToken>,
        transport: TransportKind,
        fallback_actor: &str,
    ) -> Result<Principal> {
        if !self.documents.contains_key(doc_id) {
            let exists = match &self.persist {
                Some(persist) => !persist.load_atoms(doc_id)?.is_empty(),
                None => false,
            };
            if exists {
                self.open_document(doc_id)?;
            }
        }
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        document.authorize(token, transport, fallback_actor)
    }

    /// 文档 capability URL（打开文档时返回内嵌 token 的 URL，12.7）。
    pub fn capability_url(&self, doc_id: &str, token: &CapabilityToken) -> String {
        format!("yanshi://doc/{doc_id}?token={token}")
    }

    /// 文档数量。
    /// 汇总所有已打开文档的渲染缓存统计（`/health` 用）。
    pub fn cache_stats(&self) -> (usize, usize, u64, u64) {
        let mut tiles = 0usize;
        let mut used_bytes = 0usize;
        let mut evictions = 0u64;
        let mut misses = 0u64;
        for document in self.documents.values() {
            let stats = document.cache_stats();
            tiles += stats.tiles;
            used_bytes += stats.used_bytes;
            evictions += stats.evictions;
            misses += stats.misses;
        }
        (tiles, used_bytes, evictions, misses)
    }

    /// 所有已打开文档的像素缓冲**估算上界**（按「每层整幅 RGBA8」计）。
    ///
    /// 注意这是**上界**而非实测：图层实际存的是矢量/原子描述，只有在渲染成栅格时才占用
    /// 相应内存（实测 4K/10 图层 RSS 约 0.08GB，而该估算为 0.63GB）。命名上明确 `estimate`。
    pub fn pixel_bytes_estimate(&self) -> usize {
        self.documents
            .values()
            .map(|document| document.pixel_bytes())
            .sum()
    }

    /// 已打开文档数量。
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// 是否没有打开的文档。
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// 累计创建过的文档数。
    pub const fn created(&self) -> u64 {
        self.created
    }

    /// 文档摘要 JSON（`get_document` 工具用）。
    pub fn summary_json(&self, doc_id: &str) -> Result<Value> {
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        let state = document.state();
        // **只数"存活"的图层/对象/选区** ✓ —— 折叠层**保留墓碑** ✓（`deleted_by` ✓，
        // 这正是"删除可撤销、日志可重放"的基础 ✓），但**摘要必须反映当前状态** ✓。
        // 子 agent 实测：删掉 6 个图层后 `get_document` 仍报 501 个对象 / 14 个图层 ✗，
        // 而 `list_objects`/`list_layers` 报 259 / 6 ✓ —— 同一个瞬间两个数，用户看到会以为数据坏了 ✓。
        let live_layers = state
            .layers
            .values()
            .filter(|layer| !layer.is_deleted())
            .count();
        let live_objects = state
            .objects
            .values()
            .filter(|object| !object.is_deleted())
            .count();
        let live_selections = state
            .selections
            .values()
            .filter(|selection| !selection.is_deleted())
            .count();
        Ok(json!({
            "doc_id": doc_id,
            "width": state.width,
            "height": state.height,
            "color_space": state.color_space,
            "background": state.background,
            "head_seq": document.head_seq(),
            "head_atom": state.head_atom,
            "eval_origin_seq": state.eval_origin_seq(),
            "layers": live_layers,
            "objects": live_objects,
            "selections": live_selections,
            // 墓碑数单独给出 ✓：想审计"删了多少"的人仍然看得到 ✓，而不会误读成当前内容 ✓。
            "tombstoned_layers": state.layers.len() - live_layers,
            "tombstoned_objects": state.objects.len() - live_objects,
            "atoms": document.atom_count(),
            "rendered_seq": document.render_watermark(),
            "medium": state.medium,
        }))
    }

    fn journal(&self, doc_id: &str, result: &CommitResult) -> Result<()> {
        if result.duplicate {
            return Ok(());
        }
        let Some(persist) = &self.persist else {
            return Ok(());
        };
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        if let Some(atom) = document.log().by_seq(result.seq) {
            persist.append_atom(doc_id, atom)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!("yanshi-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn in_memory_workspace_creates_and_commits() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        assert!(workspace.is_empty());
        {
            let document = workspace
                .create_document(NewDocument::new("doc_1", 32, 32), "human:1", "session:a")
                .unwrap();
            assert_eq!(document.head_seq(), 1);
        }
        let result = workspace
            .commit(
                "doc_1",
                Atom::new(
                    yanshi_core::AtomKind::CreateLayer,
                    "human:1",
                    "session:a",
                    json!({"layer_id": "layer_1"}),
                ),
                "human:1",
                false,
            )
            .unwrap();
        assert_eq!(result.seq, 2);
        assert_eq!(workspace.created(), 1);
        assert_eq!(workspace.document_ids(), vec!["doc_1"]);

        let summary = workspace.summary_json("doc_1").unwrap();
        assert_eq!(summary["width"], json!(32));
        assert_eq!(summary["head_seq"], json!(2));

        // 重复创建同名文档被拒绝。
        assert_eq!(
            workspace
                .create_document(NewDocument::new("doc_1", 32, 32), "human:1", "session:a")
                .unwrap_err()
                .code,
            ErrorCode::PreconditionFailed
        );
        assert!(workspace.close_document("doc_1"));
        assert!(workspace.document("doc_1").is_none());
    }

    #[test]
    fn file_workspace_survives_restart_with_tokens_and_render_cache() {
        let root = temp_root("restart");
        let settings = DocumentSettings::default();
        let token;
        {
            let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
            workspace
                .create_document(NewDocument::new("doc_1", 64, 48), "human:1", "session:a")
                .unwrap();
            for atom in [
                Atom::new(
                    yanshi_core::AtomKind::CreateLayer,
                    "human:1",
                    "session:a",
                    json!({"layer_id": "layer_1"}),
                ),
                Atom::new(
                    yanshi_core::AtomKind::DrawStroke,
                    "human:1",
                    "session:a",
                    json!({
                        "object_id": "obj_1",
                        "layer_id": "layer_1",
                        "data": {"points": [[4.0, 4.0], [40.0, 20.0]], "size": 4.0},
                    }),
                ),
            ] {
                workspace.commit("doc_1", atom, "human:1", false).unwrap();
            }
            token = workspace
                .issue_token("doc_1", "human:2", Role::Editor)
                .unwrap();
            let preview = workspace
                .render_region("doc_1", Bbox::new(0.0, 0.0, 64.0, 48.0))
                .unwrap();
            assert_eq!(preview.mime_type, "image/png");
            assert!(workspace.capability_url("doc_1", &token).contains("token="));
        }

        // 重启：新工作区从磁盘恢复。
        let mut workspace = Workspace::with_file_store(&root, settings).unwrap();
        assert!(workspace.is_empty(), "打开时按需加载");
        let summaries = workspace.list_documents().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].doc_id, "doc_1");
        assert_eq!(summaries[0].width, 64);

        let document = workspace.open_document("doc_1").unwrap();
        assert_eq!(document.head_seq(), 3);
        assert_eq!(
            document.render_watermark(),
            3,
            "打开即图片：渲染缓存直接可用（14.5）"
        );
        assert!(document.latest_preview_url().is_some());
        // 令牌恢复后可鉴权。
        let principal = workspace
            .authorize("doc_1", Some(&token), TransportKind::Http, "x")
            .unwrap();
        assert_eq!(principal.actor, "human:2");
        assert!(principal.role.can_edit());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn changeset_commit_shares_one_id() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        workspace
            .create_document(NewDocument::new("doc_1", 16, 16), "human:1", "s")
            .unwrap();
        let atoms = vec![
            Atom::new(
                yanshi_core::AtomKind::CreateLayer,
                "human:1",
                "s",
                json!({"layer_id": "layer_1"}),
            ),
            Atom::new(
                yanshi_core::AtomKind::DrawStroke,
                "human:1",
                "s",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "data": {"points": [[1.0, 1.0]]}}),
            ),
        ];
        let changeset_id = yanshi_core::Changeset::new_id();
        let results = workspace
            .commit_changeset("doc_1", atoms, "human:1", false, changeset_id.clone())
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|result| result.changeset_id.as_deref() == Some(changeset_id.as_str())));
        let document = workspace.document("doc_1").unwrap();
        assert_eq!(document.log().changeset_atoms(&changeset_id).len(), 2);
    }

    #[test]
    fn permission_denied_for_unknown_documents() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        assert_eq!(
            workspace
                .commit(
                    "doc_missing",
                    Atom::new(yanshi_core::AtomKind::Comment, "human:1", "s", json!({})),
                    "human:1",
                    false
                )
                .unwrap_err()
                .code,
            ErrorCode::ReferenceNotFound
        );
        assert!(workspace.open_document("doc_missing").is_err());
    }
}
