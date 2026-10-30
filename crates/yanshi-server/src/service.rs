//! 工作区/服务：多文档注册表与会话编排（设计文档 3 章服务端）。
//!
//! [`Workspace`] 负责：文档创建与打开、持久化（原子 JSONL + CAS + 渲染缓存）、
//! 提交路径的落盘、令牌发放与鉴权、以及给工具层使用的统一入口。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use yanshi_core::blob::{BlobStore, MemoryBlobStore, DELETE_DOCUMENT_BLOB_GRACE_SECONDS};
use yanshi_core::{Atom, Bbox, ChangesetId, ErrorCode, ErrorContext, Result, Seq, YanshiError};
use yanshi_render::thumb::ThumbKind;

use crate::document::{
    CommitResult, Document, DocumentSettings, NewDocument, RenderStatus, RenderedPreview,
};
use crate::inflight::InflightRegistry;
use crate::persist::{DocumentMeta, FileStore};
use crate::timings::CommitPhases;
use crate::token::{CapabilityToken, Principal, Role, TransportKind};

/// **★ 等待预览的宽限期 ✓ ★**（毫秒 ✓）：**超过它仍未就绪 ⇒ **就地同步生成**✗**
///（**∴ 兜底 ✓；∴ 值小 ⇒ 只读不编辑的调用方不会等太久 ✓**）。
pub const PREVIEW_PENDING_GRACE_MS: i64 = 200;

/// 确保存在文档级缩略图并返回它的地址（6.2「打开即图片」）。
///
/// 已有缓存（渲染过全幅或从磁盘恢复）直接返回；否则按 `preview_size` 生成一张。
///
/// **默认 256² 走增量预览** ✓（冷启动复用专题）：`render_document_preview` 只需要重渲染
/// `(持久化 seq, HEAD]` 的脏区 ✓（像素基座由打开时的 `restore_persisted_render` 恢复 ✓）。
/// 旧路径走 `thumbnail()` ✗ ⇒ 每次都 `render_thumbnail(…, None)` ⇒ **整幅全分辨率渲染** ✗
/// ⇒ 实测 4K/318 对象 **122s**，而**每个新连接**都会走到这里 ✗（磁盘缓存的 seq 一落后就重来 ✓）。
/// 其它尺寸（64/128）仍走原来的整幅缩略图路径 ✓（请求少见，行为保持不变 ✓）。
/// **★ 文档缩略图是否已是最新 ✓ ★**（第 514 轮 ✓）：**只读转发 ✓** ——
/// **∴ 不动 `ensure_document_thumbnail` 的返回类型 ✓**（**改它会波及两个调用点 ✗**）。
/// **∴ 用途** ✓：**`get_document` 据此标 `first_preview` ✓** ⇒ **∴ 让调用方能区分
/// "**慢一次**（**一次性生成缩略图 ＋ 预览 ✓**）"与"**慢每次**（**真回归 ✗**）"** ✓✓
/// **★ 缩略图／预览的三态 ✓ ★**（第 589 轮 ✓；**异步预览设计第 ③ 步 ✓**）。
///
/// **为什么需要它** ✗：**`ensure_document_thumbnail` 的 `Result<Option<String>>` 里
/// `None` 已表示 `Skip`** ✗ ⇒ **∴ 无法表达"**还在生成**✗"** ⇒ **∴ 于是**另起一个类型 ✓，
/// **∴ 而 `ensure_document_thumbnail` 保持不动 ✓**（**改它会波及 2 个调用点 ✗**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    /// **已就绪 ✓**：**URL 可直接取图 ✓**。
    Ready(String),
    /// **正在生成 ✗**：**调用方应稍后再取 ✓**（**∴ 不许当成"没有图"✗**）。
    Pending,
    /// **明确不提供 ✓**：**如调用方要求 `Skip` ✓**。
    Unavailable,
}

/// **工程包里 `blobs/` 的编码** ✓ —— 写进 `blobs.encoding` ✓，导入端据此解码 ✓。
///
/// 目前只有一种：`zlib` ✓（与本地 CAS 同一个编解码器 ✓，**逐字节可逆** ✓）。
/// 它是**显式**的 ✓ —— 导入端不需要"试着解一下，失败就当明文" ✗
/// （那种猜法会把"解出来是垃圾"变成"悄悄导入了坏字节" ✗）。
const PACKAGE_BLOB_ENCODING: &str = "zlib";

/// **每原子的内存估算口径**（P-8/X-1：按冷重开实测校准）。
/// 实测：4,002 原子冷重开后 RSS 275 MB ⇒ **≈0.069 MB/原子**
/// （P21-DESIGN.md §1.3）。旧值 300 KB/原子来自一份已渲染文档的
/// RSS（3,600 原子 ⇒ 1,088 MB），那份 RSS 里含渲染缓存；渲染缓存
/// 现由 `renderer_retained_bytes()` 另行实测叠加，再按 300 KB 算
/// 等于 double-count ⇒ 估算约为实测的 4.3 倍，10,000 原子文档在
/// 缺省 2,048 MB 预算下被误拒。
/// 新值 **100 KB/原子**：在冷重开实测（0.069 MB/原子）上留约 45%
/// 余量；预算闸本身不动（缺省仍 2,048 MB，超限仍拒绝），只改
/// 估算口径。两处共用同一份：blob GC 闸（估「全打开」的代价）与
/// C2 文档预算闸（估「已载入」的驻留）——口径只许有一个出处，
/// 两处各写一份必然漂移。
/// **对外可见**（P8 后效 ✓）：`tests/blob_gc_budget.rs` 要按**同一把尺子**算
/// 「造几个原子能越过 1 MB 的测试预算」⇒ 写死个数会随口径校准第二次漂移
/// （300 KB→100 KB 时它就从绿变红 ✓ CI 抓的 ✓）。
pub const BYTES_PER_ATOM: u64 = 100 * 1_024;

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

/// **已载入文档的内存观测**（C2，期 2）：`Workspace::loaded_documents`
/// 的一行。`estimated_bytes` = 原子估算（BYTES_PER_ATOM 口径）+
/// 渲染侧常驻缓存实测——与预算闸的判定同源，观测与处置不许两套数。
/// （RSS 修复 paint-rss：实测口径从"只看位图"扩到"全部渲染缓存"，见
/// `estimate_loaded_doc_bytes`；`retained_bytes` 把其中渲染侧那份单列，
/// 判据据此算"待载入文档的原子口径"而不必反推常量。）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedDocumentInfo {
    /// 文档 id。
    pub doc_id: String,
    /// 原子数。
    pub atoms: usize,
    /// 估算驻留字节（原子估算 + 渲染侧常驻缓存实测）。
    pub estimated_bytes: u64,
    /// 位图缓存实测字节。
    pub cache_bytes: u64,
    /// 渲染侧常驻缓存实测字节（池保留 + tile + 位图 + 区域块 +
    /// below/above + 蒙版 + 矢量光栅；`estimated_bytes` 里渲染侧那份）。
    pub retained_bytes: u64,
    /// 最近使用序号（工作区单调递增；越大越新，LRU 卸载按它升序）。
    pub last_used_seq: u64,
}

/// **文档卸载观测**（C2，期 2）：预算卸载不许静默——计数与最近
/// 一次留痕经 `Workspace::doc_eviction_stats` 与诊断响应可见。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocEvictionStats {
    /// 累计卸载次数。
    pub evictions: u64,
    /// 选候选时因 busy（inflight / 打开的 changeset / transaction）
    /// 被跳过的累计次数。
    pub skipped_busy: u64,
    /// 最近一次被卸载的文档。
    pub last_evicted_doc: Option<String>,
    /// 最近一次卸载的原因。
    pub last_reason: Option<String>,
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

/// **把资产目录解析成一个真实存在的路径** ✓（真实用户报告 ✓）。
///
/// **问题** ✓：`--assets-dir` 的缺省是**相对路径** `assets` ✓ ⇒ 只有**在仓库根目录**启动才找得到 ✓
/// ⇒ 换个工作目录启动（或包装脚本没 `cd` ✓）⇒ **三类内置资产全丢** ✗，
/// 界面表现为"笔刷下拉里只有'内置画笔' ✓、调色板与纹理一片空白" ✗ ——
/// 用户实测到的正是这个 ✓（我在别的目录启动，**原样复现**：三类的 `count` 全是 0 ✓）。
///
/// **修法** ✓：按**候选顺序**取第一个**真的存在**的 ✓：
/// 1. 调用方给的那个（命令行 / 代码里设的 ✓）；
/// 2. **可执行文件旁边的包内布局** ✓：`<exe>/../share/yanshi` ✓ ——
///    这正是 `make release` 打出来的包的结构 ✓（`bin/yanshi-serve` + `share/yanshi/*` ✓）；
/// 3. `<exe>/assets` ✓ —— 覆盖"可执行文件与资产目录并列"的布局 ✓；
/// 4. **可执行文件的祖父目录**下的 `assets` ✓：`<exe>/../../assets` ✓ ——
///    覆盖"从 `target/release/` 直接跑"这个**开发时最常见**的情形 ✓；
/// 5. `<exe>/../../../assets` ✓ —— 覆盖测试二进制与 `cargo` 中间产物
///    再深一级的布局（`target/debug/deps/yanshi_mcp-<hash>` ✓）。
///
/// **一个都找不到就返回 `(None, 注记)`** ✓（调用方据此启动失败 ✓，
/// 不再悄悄拿一个不存在的目录继续跑 ✓），**而且不管找到没找到都要有一段说明** ✓
/// ⇒ 下一次这类问题**一眼就能查** ✓，而不是再花一小时 ✗。
pub fn resolve_assets_dir(
    configured: Option<std::path::PathBuf>,
) -> (Option<std::path::PathBuf>, String) {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Some(dir) = configured.clone() {
        candidates.push(dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bin_dir) = exe.parent() {
            // 包内布局：`bin/yanshi-serve` 与 `share/yanshi/` 并列 ✓。
            candidates.push(bin_dir.join("../share/yanshi"));
            candidates.push(bin_dir.join("assets"));
            // 开发时：`target/release/yanshi-serve` ⇒ `<repo>/assets` ✓。
            candidates.push(bin_dir.join("../../assets"));
            // 测试二进制与 `cargo` 的中间产物比可执行文件**再深一级** ✗：
            // `target/debug/deps/yanshi_mcp-<hash>` ⇒ 需要 `../../../assets` ✓。
            // **∴ 不补这一条 ✗**：`yanshi-mcp` 的端到端测试**找不到资产** ✗
            // ⇒ **∴ `create_layer` 的预览没有缩略图** ⇒ **∴ `thumb_url` 是 `None` ✗**
            // ⇒ **∴ 测试在 `unwrap()` 上崩 ✓**（真实门禁红灯 ✓，实测 ✓）。
            // **∴ 放在最后 ✗**：**不遮蔽**上面四条已文档化的布局 ✓。
            candidates.push(bin_dir.join("../../../assets"));
        }
    }
    for candidate in &candidates {
        if candidate.join("textures").is_dir() || candidate.join("brushes").is_dir() {
            let note = format!("资产目录：{}", candidate.display());
            return (Some(candidate.clone()), note);
        }
    }
    // **一个都没有** ✓ ⇒ 把找过的都写进制表里 ✓（比"什么都没有"有用得多 ✓）。
    let tried = candidates
        .iter()
        .map(|candidate| candidate.display().to_string())
        .collect::<Vec<_>>()
        .join(" / ");
    (None, format!("资产目录：未找到（找过：{tried}）"))
}

/// **调色板里的一个颜色** ✓。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteColor {
    /// 红 ✓。
    pub r: u8,
    /// 绿 ✓。
    pub g: u8,
    /// 蓝 ✓。
    pub b: u8,
    /// 名字 ✓（`.gpl` 的第四列 ✓ / `open-color.json` 的键 ✓）—— 取不到就 `None` ✓。
    pub name: Option<String>,
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
    /// **这个文件到底能不能用** ✓ —— **由内核判定** ✗，不是调用方看扩展名猜的 ✓。
    ///
    /// **为什么放进内核** ✓（真实事故 ✓）：我第一版让工具层"看扩展名"算可用性 ✗
    /// ⇒ `assets/textures/Paper003.png` 被报成 `usable: true` ✓，
    /// 而它其实是**灰度 PNG** ✗ ⇒ 上传时被解码器拒绝 ✓：
    /// "PNG 解码失败（本仓库只支持 8 位、非隔行的 RGB/RGBA PNG）" ✗ ✗
    /// ⇒ **"说能用、其实不能用"** ✓ —— 与"接受了却没用"是同一类病 ✓。
    /// ⇒ 现在**真读 PNG 头**（位深 / 颜色类型 / 隔行 ✓）✓ ⇒ 报出来的就是事实 ✓。
    pub usable: bool,
}

/// **读一个目录里的纹理文件** ✓（不存在 ⇒ 空表 ✓ —— "还没下载过"是**正常状态** ✗，不是错误 ✓）。
///
/// **抽成函数** ✓：内置目录与缓存目录**走同一段逻辑** ✓ ⇒ 不会出现
/// "缓存那条会过滤隐藏文件、内置那条忘了" 这种漂移 ✗（本项目吃过太多次 ✓）。
/// **解析 GIMP 调色板文本** ✓（`.gpl` / `.kpl` ✓）。
///
/// **格式** ✓：第一行 `GIMP Palette` ✓、`Name:` 元信息 ✓、`#` 注释 ✓，
/// 其余每行 `R G B [名字]` ✓（用空格或制表符分隔 ✓）。**解析要宽容** ✗ ——
/// 这些文件来自 20 多个不同软件 ✓，缩进与分隔符并不统一 ✓
/// ⇒ 跳过认不出的行 ✓（但**不静默**：全都没认出来时上层会报错 ✓）。
fn parse_gimp_palette(text: &str) -> Vec<PaletteColor> {
    let mut colors = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("GIMP Palette") {
            continue;
        }
        if trimmed.starts_with("Name:") || trimmed.starts_with("Columns:") {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let (Some(r), Some(g), Some(b)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(r), Ok(g), Ok(b)) = (r.parse::<u16>(), g.parse::<u16>(), b.parse::<u16>()) else {
            continue;
        };
        if r > 255 || g > 255 || b > 255 {
            continue;
        }
        let name = parts.collect::<Vec<_>>().join(" ");
        colors.push(PaletteColor {
            r: r as u8,
            g: g as u8,
            b: b as u8,
            name: if name.is_empty() { None } else { Some(name) },
        });
    }
    colors
}

/// **解析 Open Colors 那种 JSON** ✓（键 ⇒ 十六进制串 ✓ 或**串数组** ✓）—— 与 `assets/palettes/open-color.json` 一致 ✓。
fn parse_open_color_json(text: &str) -> Result<Vec<PaletteColor>> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("不是合法 JSON：{error}")),
        )
    })?;
    let object = value.as_object().ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "JSON 调色板的最外层应当是对象：键是颜色名，值是十六进制色号，或是一串色号",
            ),
        )
    })?;
    let mut colors = Vec::new();
    for (key, entry) in object {
        match entry {
            serde_json::Value::String(hex) => {
                if let Some((r, g, b)) = parse_hex_color(hex) {
                    colors.push(PaletteColor {
                        r,
                        g,
                        b,
                        name: Some(key.clone()),
                    });
                }
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    let Some(hex) = item.as_str() else { continue };
                    if let Some((r, g, b)) = parse_hex_color(hex) {
                        colors.push(PaletteColor {
                            r,
                            g,
                            b,
                            // **名字带上序号** ✓：同一色系十阶 ✓ ⇒ 只给键名会十条同名 ✗。
                            name: Some(format!("{key} {}", index + 1)),
                        });
                    }
                }
            }
            _ => continue,
        }
    }
    Ok(colors)
}

/// **解析 `#rgb` / `#rrggbb`** ✓（大小写都收 ✓；“`#`”可省 ✓）。
fn parse_hex_color(text: &str) -> Option<(u8, u8, u8)> {
    let digits = text.trim().trim_start_matches('#');
    let parse = |slice: &str| u8::from_str_radix(slice, 16).ok();
    match digits.len() {
        3 => {
            let mut chars = digits.chars();
            let (r, g, b) = (chars.next()?, chars.next()?, chars.next()?);
            let expand = |c: char| u8::from_str_radix(&format!("{c}{c}"), 16).ok();
            Some((expand(r)?, expand(g)?, expand(b)?))
        }
        6 => Some((
            parse(&digits[0..2])?,
            parse(&digits[2..4])?,
            parse(&digits[4..6])?,
        )),
        _ => None,
    }
}

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

/// **这个资产文件能不能真的被我们用** ✓。
///
/// **纹理要真读 PNG 头** ✗（不是看扩展名 ✓）：我们的解码器是自己写的 ✓、只吃
/// **8 位、非隔行、RGB 或 RGBA** ✓ ⇒ 灰度 / 16 位 / 隔行 都会被拒 ✓。
/// **判据来自解码器本身** ✓ ⇒ 列表里说"能用"的，导入时**真的能用** ✓。
fn asset_file_usable(kind: &str, path: &std::path::Path, extension: &str) -> bool {
    match kind {
        "texture" => extension == "png" && png_is_decodable(path),
        "brush" => extension == "myb",
        // **调色板同样要真读文件头** ✗（只看扩展名就会重犯灰度 PNG 那个错 ✓）：
        // `.gpl` 与 `.kpl` 都以 `GIMP Palette` 开头 ✓；`.json` 应当是对象或数组 ✓。
        "palette" => match extension {
            "gpl" | "kpl" => file_starts_with(path, b"GIMP Palette"),
            "json" => {
                let head = read_head(path, 8);
                let trimmed = head
                    .iter()
                    .copied()
                    .skip_while(|byte| byte.is_ascii_whitespace())
                    .collect::<Vec<u8>>();
                matches!(trimmed.first(), Some(b'{') | Some(b'['))
            }
            _ => false,
        },
        _ => false,
    }
}

/// **读文件开头若干字节** ✓（读不到就返回空 ✓ ⇒ 上层自然判为不可用 ✓）。
fn read_head(path: &std::path::Path, limit: usize) -> Vec<u8> {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let mut buffer = vec![0u8; limit];
    match file.read(&mut buffer) {
        Ok(read) => {
            buffer.truncate(read);
            buffer
        }
        Err(_) => Vec::new(),
    }
}

/// **文件是否以某串字节开头** ✓（大小写敏感 ✓ —— 这几个格式头都是固定写法 ✓）。
fn file_starts_with(path: &std::path::Path, prefix: &[u8]) -> bool {
    let head = read_head(path, prefix.len().max(16));
    head.starts_with(prefix)
}

/// **读 PNG 头判断解码器能不能吃** ✓（只读前 33 字节 ✓，不解整幅 ✓ ⇒ 列表也很快 ✓）。
///
/// 判据与解码器一致 ✓：**位深 8** ✓、**非隔行** ✓、**颜色类型 2（RGB）或 6（RGBA）** ✓。
fn png_is_decodable(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut header = [0u8; 33];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    if &header[0..8] != b"\x89PNG\r\n\x1a\n" || &header[12..16] != b"IHDR" {
        return false;
    }
    let depth = header[24];
    let color = header[25];
    let interlace = header[28];
    depth == 8 && interlace == 0 && (color == 2 || color == 6)
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
    /// **在飞变更操作登记表** ✓（外部测试报告 P1 ✓）。
    ///
    /// **为什么用 `Arc`** ✓：登记表必须在**不取这把 `Workspace` 锁**的前提下被读到 ✓
    ///（长操作正持着锁 ✓）⇒ HTTP 层另存一份句柄 ✓，两边指向**同一张表** ✓。
    /// **为什么放在这里而不是 HTTP 层** ✗：MCP／嵌入式／测试也走工具层 ✓，
    /// 状态只有放在**所有入口共用**的地方才不会分叉 ✓（见 `crate::inflight` ✓）。
    inflight: Arc<InflightRegistry>,
    documents: BTreeMap<String, Document>,
    persist: Option<FileStore>,
    /// **每个文档上次看到的 `atoms.jsonl` 长度** ✓（第 762 轮，性能）
    ///
    /// **⇒ 为什么要缓存** ✗：`sync_next_seq_from_disk` 每次提交都要读盘尾 8 KiB ✓
    /// ⇒ 实测 **35.7 ms/commit vs 关掉时 33.9** ✓ ⇒ **多花 ~1.8 ms/commit（5.4%）** ✗。
    /// **⇒ 做法**：先用 `metadata().len()` 比一下 ✓（一次 `stat`，微秒级 ✓）
    /// ⇒ 长度没变 ⇒ **盘上没新东西** ✓ ⇒ **不读那 8 KiB** ✓。
    disk_len_seen: std::collections::HashMap<String, u64>,
    /// **工作区级偏好** ✓（收藏的笔刷、最近使用…… ✓）。
    ///
    /// **为什么在工作区、而不是文档里** ✓：这些是**用的人**的偏好 ✓，不是这张画的一部分 ✓
    /// ⇒ 放进文档会把"谁在用"混进"画了什么" ✗（导出工程包时更不该把它一起带走 ✗）。
    /// **持久化位置** ✓：`<root>/preferences.json` ✓（有落盘工作区就存 ✓；纯内存模式只存在内存里 ✓，
    /// 并在工具返回里**说清这一点** ✓ —— 不让人以为重启后还在 ✓）。
    preferences: BTreeMap<String, Value>,
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
    /// **"不按时间出 dab"警告已报过的（会话，笔刷）** ✓（P-16）。
    ///
    /// **为什么要它** ✗：`oil-06-paint` 这类 `dabs_per_second = 0` 的笔刷 ✓，每笔带
    /// `time_ms` 都要写一条同样的警告 ✓ ⇒ 20,000 笔就是 20,000 条 ✓。
    /// **只在内存里** ✓：重启后新会话理应再报一次 ✓（新调用方可能没见过首次警告 ✓）；
    /// 键里带会话 ⇒ 换会话重新报 ✓，同一会话同一笔刷只报一次 ✓。
    warned_time_insensitive_brushes: std::collections::HashSet<(String, String)>,
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
    /// **★ 预览"已入队等待"的起始时刻 ✓ ★**（第 594 轮 ✓；**异步预览设计 ④⑤ ✓**）：
    /// **∴ 兜底依据 ✗**：**超过 `PREVIEW_PENDING_GRACE_MS` 仍未就绪 ⇒ 就地同步生成 ✓**
    ///（**∴ 否则只读不编辑的调用方会**永远 pending ✗**）。
    preview_pending_at: std::collections::BTreeMap<String, i64>,
    /// **C2 文档预算（期 2）：每份已载入文档的最近使用序号**。
    /// `open_document` / `create_document` 每次成功都刷新；预算闸按
    /// 它升序选 LRU 卸载候选。序号只在内存里——它排序的是「本进程
    /// 的使用热度」，重启后从空开始是对的。
    doc_last_used: BTreeMap<String, u64>,
    /// 最近使用序号的单调分配器（与 `doc_last_used` 同源）。
    doc_use_seq: u64,
    /// **C2 卸载留痕**：累计卸载数（不许静默卸载，诊断可见）。
    doc_evictions: u64,
    /// **C2 卸载留痕**：选候选时 busy 文档被跳过的累计数。
    doc_evictions_skipped_busy: u64,
    /// **C2 卸载留痕**：最近一次卸载的 (doc_id, 原因)。
    doc_last_eviction: Option<(String, String)>,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("documents", &self.documents.keys().collect::<Vec<_>>())
            .field("persisted", &self.persist.is_some())
            .finish()
    }
}

/// **一次工程包导出的统计** ✓（第 1331 轮的定案暴露了需求 ✓）：
/// `export_project` 原先只回 `Vec<u8>` ✗ ⇒ 调用方**只能看到整包体积** ✗
/// ⇒ 而"被替代的位图有没有被回收"是**位图部分**的问题 ✓（**∴ 日志必然增长 ✗，位图不该 ✓**）
/// ⇒ 所以把这三项**显式回出来** ✓ ⇒ 判据就能**断言对的东西** ✓（而不是断言整包体积 ✗）。
// **去掉了 `Copy`** ✓（第 70 轮 ✓）：加上"缺件清单"（`Vec<String>` ✓）之后它不再能 `Copy` ✗
// —— 而 `Vec` 正是**必须报出来**的东西 ✓（静默跳过＝撒谎 ✓）⇒ **∴ 保留 `Clone`／`Default`，去掉 `Copy`** ✓。
#[derive(Debug, Clone, Default)]
pub struct ExportStats {
    /// 装进包里的位图个数 ✓（**∴ `include_bitmaps: false`✗ 时只装"无法证明可重放"✗ 的 ✓**）。
    pub blob_count: usize,
    /// 位图**裸字节**之和 ✓。
    pub blob_bytes_plain: usize,
    /// 位图**压缩后**字节之和 ✓（**∴ 它决定包体积 ✓**）。
    pub blob_bytes_packed: usize,
    /// **被"可重放证明"省掉**的位图个数 ✓（= 重放后哈希一致 ⇒ 不装 ✓）。
    ///
    /// **为什么暴露它** ✗（第 35 轮 ✓）：`blob_count` 只说"装了几个"✗ ⇒ 看不出
    /// **重放验证跑了多少次** ✓ —— 而那次重放**正是导出慢的主因** ✓
    ///（实测：4K **2.65 s** ✗／8K **12 s** ✗）。有了这三个数 ✓，才能量化
    /// "**把证明记忆化**能省多少" ✓（第 346 轮定的真杠杆 ✓）。
    pub omitted: usize,
    /// **没有可重放配方** ⇒ 照装 ✓ 的个数（笔刷读画布／非笔刷来源 ✓）。
    pub kept_no_recipe: usize,
    /// **`kept_no_recipe` 为什么成立** ✓，按位图分类计数（第 230 轮 ✓，为 CI 独有红定位 ✓）。
    ///
    /// 键是原因名（`no_source`／`kind_not_brush`／`brush_absent`／`brush_unreadable`／
    /// `reads_canvas`／`unreferenced`／`all_replayable_but_kept` ✓），值是条数 ✓；
    /// **恒等式 `values 之和 == kept_no_recipe`** 由判据 `tool-export-idempotent` 守着 ✓
    ///（分类与 `brush_source_is_replayable` 共用同一判定 ✓ ⇒ 两份实现漂移不了 ✗）。
    pub kept_no_recipe_reasons: std::collections::BTreeMap<&'static str, usize>,
    /// **有配方但重放对不上** ⇒ 照装 ✓ 的个数（宁可包大，也要能打开 ✓）。
    pub kept_mismatch: usize,
    /// **引用了但存储里没有** ✗ 的**本体**位图个数（第 70 轮 ✓，真实用户报告 ✓）。
    ///
    /// **B2 改了第 70 轮的结论** ✓：那时"降级而非整单失败"是怕 15MB 真实文档
    /// 整个导出报错退出 ✗ —— 但降级产出的缺本体包**看起来正常**，调用方
    /// 拿去重放才发现是空的 ✗ ⇒ 所以现在**本体缺 ⇒ 整个导出失败**
    /// （`ReferenceNotFound` ✓，不产包 ✓）；只缺派生 tile 时仍照常导出 ✓。
    /// 成功返回的统计里它恒为 0 ✓；失败时个数进报错文案 ✓。
    pub kept_missing: usize,
    /// **缺本体 blob 的具体清单** ✓（哪些本体位图没装进包 ✓）—— **必须报出来** ✗，静默就是撒谎 ✓。
    pub missing: Vec<String>,
    /// **缺本体 blob 的个数** ✓（B2 ✓）：引用了但存储里没有的本体位图数 ✓。
    ///
    /// 与 `missing` 清单同数 ✓（恒等式 `missing_body == missing.len()` ✓）；
    /// `> 0` ⇒ 导出直接失败（`ReferenceNotFound` ✓），不产包 ✓。
    pub missing_body: usize,
    /// **缺派生 tile 的个数** ✓（B2 ✓）：引用了但存储里没有的派生 tile 索引数 ✓。
    ///
    /// 口径与 `check_package.py` 一致：`payload.tiles` 里藏的引用 ⇒ 派生 ✓；
    /// tile 索引由本体位图重建 ✓，缺了它渲染落回整幅路 ✓ ⇒ 只缺派生时
    /// 仍可导出 ✓，但数字必须报出来 ✓。
    ///
    /// **B 期项 7/8（出口侧口径）**：本栏只数**存储里缺失**的派生索引；
    /// 按设计未装包的派生总数另见 `derived_not_packaged`，不计入本栏。
    pub missing_derived: usize,
    /// **按设计未装包的派生 tile 数** ✓（B 期项 7/8，出口侧口径）：
    /// `payload.tiles` 里引用、且不是本体引用的 tile 索引总数——
    /// 无论存储里在不在，导出时**都不装包**（tile 由本体位图重建 ✓）。
    /// 与 `missing_derived` 的区别：`missing_derived` 只数其中存储里
    /// 不存在的那部分（存储缺失）；本栏数的是全部按设计未装包的。
    pub derived_not_packaged: usize,
    /// **bake frontier 口径**（第二轮第 9 条）：`blob_bundle_mode="frontier"`
    /// 时被剔除的历史 Bake Blob 数（日志中有 BakeLayer 引用但不在有效集内，
    /// 且未被任何存活对象引用）；`all_history` 时恒为 0。
    pub frontier_omitted_historical_blobs: usize,
}

/// **导出"可重放证明"的进程内缓存** ✓（第 39 轮 ✓）。
///
/// **为什么进程内就够** ✓：缓存的键已**完整**涵盖重放的四个输入 ✓ ⇒ 只要进程还活着，
/// 同一个键的结论**必然**仍然成立 ✓；进程退出 ⇒ 缓存消失 ⇒ **不需要任何磁盘失效逻辑** ✓
///（**∴ 零"过期"风险** ✓ —— 与"渲染快照"那种要写盘的东西**不同** ✓）。
/// 键的四个分量 ✓（第 39 轮 ✓）：`(crate 版本, 配方 canonical 串, 笔刷内容哈希, 目标哈希)` ✓。
type ExportProofKey = (String, String, String, String);
/// 缓存的类型别名 ✓ —— **必须单独起名** ✗：直接把这一长串写进 `static`／返回值会被
/// `clippy::type_complexity` 判为"类型太复杂" ✗（第 39 轮实测：CI 的 clippy **因此变红** ✓）。
type ExportProofCache = std::sync::Mutex<std::collections::HashSet<ExportProofKey>>;
static EXPORT_PROOF_CACHE: std::sync::OnceLock<ExportProofCache> = std::sync::OnceLock::new();

fn export_proof_cache() -> &'static ExportProofCache {
    EXPORT_PROOF_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
}

/// **键 ＝ 重放的四个输入** ✓（缺一不可 ✗）：`(crate 版本, 配方 canonical 串, 笔刷内容哈希, 目标哈希)` ✓。
///
/// **解析不出笔刷 ⇒ 返回 `None`** ✓（**保守回退** ⇒ 不缓存 ✓ ⇒ 照旧重放 ✓）。
fn export_proof_key(
    workspace: &Workspace,
    source: &serde_json::Value,
    hash: &yanshi_core::BlobHash,
) -> Option<ExportProofKey> {
    let brush_name = source.get("brush").and_then(serde_json::Value::as_str)?;
    let file = if brush_name.ends_with(".myb") {
        brush_name.to_owned()
    } else {
        format!("{brush_name}.myb")
    };
    let path = workspace.resolve_asset("brush", &file).ok()?;
    let bytes = std::fs::read(path).ok()?;
    let brush_hash = yanshi_core::BlobHash::from_bytes(&bytes);
    Some((
        env!("CARGO_PKG_VERSION").to_owned(),
        serde_json::Value::to_string(source),
        brush_hash.to_string(),
        hash.to_string(),
    ))
}

/// 查缓存 ✓（**只读**：命中即表示"该配方确实重放出该哈希" ✓）。
fn export_proof_lookup(key: &ExportProofKey) -> bool {
    export_proof_cache()
        .lock()
        .map(|guard| guard.contains(key))
        .unwrap_or(false)
}

/// 证明缓存的条目上限 ✓：键含**笔刷内容哈希** ⇒ 不同笔刷可持续撑大它 ✗。
///
/// 满了就**整体清空**：缓存只省重放时间 ✓，清空不影响任何结果 ✓
/// （代价只是下一批重放要重新验一遍 ✓）。
const MAX_EXPORT_PROOF_ENTRIES: usize = 4096;

/// 记入缓存 ✓。
fn export_proof_remember(key: &ExportProofKey) {
    if let Ok(mut guard) = export_proof_cache().lock() {
        if guard.len() >= MAX_EXPORT_PROOF_ENTRIES && !guard.contains(key) {
            guard.clear();
        }
        guard.insert(key.clone());
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

    /// **"不按时间出 dab"警告是否已对这个（会话，笔刷）报过** ✓（P-16）。
    ///
    /// 未报过 ⇒ 登记并返回 `false`（调用方这次要报 ✓）；报过 ⇒ 返回 `true`
    /// （调用方这次跳过 ✓）。同一会话同一笔刷只报一次；换会话重新报 ✓。
    pub fn mark_time_warning_shown(&mut self, session: &str, brush: &str) -> bool {
        !self
            .warned_time_insensitive_brushes
            .insert((session.to_owned(), brush.to_owned()))
    }

    /// **统计 blob 的三级生命周期** ✓（设计 §6.3 ✓，只读 ✓）。
    ///
    /// **先把所有文档读进来** ✓ —— 这条是**一次险些删掉 1.09 GB 活数据**换来的 ✗：
    /// 服务端**惰性打开**文档 ✓ ⇒ 只打开一份空文档时 ✓，别的文档的日志**根本不在内存里** ✗
    /// ⇒ 它们引用的 blob 会被判成"无人引用"的孤儿 ✗ ⇒ 一旦带 `confirm` 跑 ✓ **活数据就没了** ✗✗。
    /// 所以这里先按磁盘上的文档清单逐份打开 ✓，**再**做分级 ✓；
    /// 并把"考虑了几份文档"一并报出来 ✓（分类是否可信，看一眼就知道 ✓）。
    /// **某个 blob 能不能解码**（第 1236 轮；`import_image` 用它给调用方一个**即时信号**）
    ///
    /// **为什么需要** ✗：外部用户 5 次失败，而**唯一能看见原因的地方**是
    /// `export_png` 的 `render_warnings`（「位图补丁缺少 blob ⇒ 这一块没有画」）
    /// ⇒ 而调用方通常只看 `import_image` 的返回 ⇒ **把「原子提交成功」误读成「位图导入成功」** ✗
    ///
    /// **为什么现在敢用它**：第 1235 轮的探针（`blob_codec.rs` 的 `probe_bare_png`）
    /// **直接对 `RenderCodec::decode` 写了单测** ⇒ 结论：
    ///   裸 PNG（首字节 0x89）⇒ `None`；包头（`\x00\x00` + PNG）⇒ `Some(原 PNG)` ✓
    /// ⇒ 所以 `get(...).is_ok()` **就是**「能不能解码」的准确判据 ✓
    /// （第 1234 轮我说它「反了」是**我的测试脚本错**，不是实现错）
    pub fn blob_decodable(&self, hash: &str) -> bool {
        let Ok(parsed) = hash.parse::<yanshi_core::BlobHash>() else {
            return false;
        };
        match self.persist.as_ref().and_then(|p| p.blob_store().ok()) {
            Some(store) => yanshi_core::blob::BlobStore::get(&store, &parsed).is_ok(),
            None => false,
        }
    }

    /// **工作区根目录**（用于把 `put_blob` 的路径限制在工作区内）✓
    pub fn root_path(&self) -> Option<&std::path::Path> {
        self.persist.as_ref().map(|p| p.root())
    }

    /// **把一个字节块放进内容寻址存储，返回 `blob_hash`** ✓（第 1228 轮；外部 §176/177 要的 `put_blob`）
    ///
    /// **为什么需要**：外部用户手里**只有本地 PNG 文件** ⇒ 而 `import_image` 要求先有 `blob_hash`。
    /// 手写文件**不够**（服务端用的是 `RenderCodec`：落盘格式 `[0x00][TAG][body]`，
    /// 裸 PNG 的第一个字节 `0x89` 会让 `decode` 返回 `None`）⇒ 所以必须由服务端自己写。
    ///
    /// **它内部会过 `RenderCodec::encode`** ⇒ 所以调用方**不需要**知道编码格式 ✓
    pub fn put_blob(&mut self, bytes: &[u8]) -> Result<String> {
        let persist = self.persist.as_ref().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail("工作区没有持久化存储 ⇒ 不能写 blob（用 --root 启动才有）"),
            )
        })?;
        // **★ 不要在这里 encode**（第 1236 轮纠正第 1228 轮的错）
        //   **第 1228 轮我写的**：`let stored = encode_for_storage(bytes); put(&stored)`
        //     理由是我当时读码认为「`put` 本身不编码」✗ —— **那是错的** ✗
        //   **实测（第 1236 轮）**：`put_blob` 之后 `import_image` 报
        //     「位图补丁 PNG 解码失败」⇒ 因为 `put_inner` **已经 encode 过一次**
        //     ⇒ 而我又 encode 一次 ⇒ 落盘是 `[0x00][0x00][0x00][0x00] + png`
        //     ⇒ `decode` 只剥一层 ⇒ 得到 `[0x00][0x00] + png` ⇒ **不是 PNG** ⇒ 解码失败 ✗
        //   ⇒ 所以这里**直接** `put(bytes)`：`put_inner` 会做唯一的一次 encode ✓
        Ok(persist.blob_store()?.put(bytes)?.to_string())
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
        // **★ 先估算内存，超了就**明确拒绝**，而不是让 OOM killer 杀进程 ★**（第 1551 轮）
        //   **⇒ 外部实测（`B4`）**：25 499 原子的 4K 文档、磁盘 469 MB ⇒ `blob_gc` **即使
        //     `dry_run:true`** 也被 **SIGKILL**（`rc=-9`）⇒ **进程死掉，调用方只看到 "server closed"** ✗✗
        //   **⇒ ⇒ 为什么 dry_run 也死**：本函数在**分档之前**就把
        //     ① **所有文档**（`open_document`）② **所有原子 clone 进一份合并日志**
        //     全载入内存 ⇒ **25k 原子的 4K 文档约 1.8 GB/份** ⇒ 几份就爆 ✗
        //   **⇒ ⇒ ⇒ 所以加一道闸**：估算"把未加载文档全打开 + 合并原子"的内存，
        //     超过上限就**返回可重试的错误**，并在 `context.detail` 里给出**两个数字**
        //     （估算值与上限）⇒ 调用方据此**分阶段跑**（外部本来就是这么绕的）✓
        //   **⇒ ⇒ ⇒ ⇒ 为什么不直接改成流式**：`plan_gc` 收的是**一份** `AtomLog`（内核签名），
        //     改它要动内核；而**先给闸**能立刻把"进程被杀"变成"可读的拒绝" ✓（**下一步再做流式**）
        // **上限可被环境变量覆盖** ✓（`YANSHI_GC_MEMORY_BUDGET_MB`，缺省 **1536 MB**）：
        //   **⇒ 为什么要这个口子** ✗：这道闸挡的是"进程被 OOM 杀"✓，而**不可调的常量没法被测** ✗
        //   ⇒ 判据只能在 3.8 GiB 的机器上真造 1.5 GiB 的文档才碰得到 ✗（谁都不会那么测 ✓）
        //   ⇒ 于是它就成了"写了但永远不验"的那类断言 ✗（本轮补的正是这一条 ✓）。
        //   顺带对运维有用 ✓：小机器可以把它调小（宁可拒绝，也别被 SIGKILL ✓）。
        const GC_MEMORY_BUDGET_DEFAULT_BYTES: u64 = 1_536 * 1_024 * 1_024; // 1.5 GiB
        let budget_bytes: u64 = std::env::var("YANSHI_GC_MEMORY_BUDGET_MB")
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
            .map(|mb| mb.saturating_mul(1_024 * 1_024))
            .unwrap_or(GC_MEMORY_BUDGET_DEFAULT_BYTES);
        let mut unloaded_atoms = 0u64;
        for doc_id in &known {
            if !self.documents.contains_key(doc_id) {
                // 用持久层的原子数**估算**（不必真的打开）✓
                if let Some(persist) = &self.persist {
                    if let Ok(atoms) = persist.load_atoms(doc_id) {
                        unloaded_atoms += atoms.len() as u64;
                    }
                }
            }
        }
        let loaded_atoms: u64 = self
            .documents
            .values()
            .map(|document| document.log().len() as u64)
            .sum();
        // 估算口径 = 模块级 BYTES_PER_ATOM（与 C2 文档预算闸共用一份）。
        let estimate = (unloaded_atoms + loaded_atoms).saturating_mul(BYTES_PER_ATOM);
        if estimate > budget_bytes {
            return Err(YanshiError::new(
                ErrorCode::ResourceExhausted,
                ErrorContext::detail(format!(
                    "blob_gc 需要把所有文档载入内存来算引用闭包 ⇒ 估算 **{} MB**（{} 个原子 × ≈100 KB/原子）\
                     超过本服务的上限 **{} MB** ⇒ **已拒绝，进程不会被 OOM 杀掉**。\
                     出路：① 先 `close_document` 掉不用的文档再跑；② 分阶段跑（关掉一批、跑一次）；\
                     ③ 真的只需要**计划**时，给 `dry_run:true` 也要走同一条闸（本函数就是它）",
                    estimate / (1_024 * 1_024),
                    unloaded_atoms + loaded_atoms,
                    budget_bytes / (1_024 * 1_024),
                )),
            ));
        }
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

    /// **这个工作区会不会把偏好落盘** ✓（纯内存模式 ⇒ 否 ✓）。
    ///
    /// **为什么要有它** ✓：`set_preferences` 在内存模式下**也能成功** ✓（改内存 ✓）
    /// ⇒ 不把这件事报出来 ✓，用户会以为"重启后还在" ✗ —— 又是"看起来成功、其实没留下" ✓。
    pub fn is_file_backed(&self) -> bool {
        self.persist.is_some()
    }

    /// **读偏好** ✓（给了 `keys` 就只回这几个 ✓ —— 调用方不必把整份偏好都拿走 ✓）。
    pub fn preferences(&self, keys: Option<&[String]>) -> Value {
        let picked: serde_json::Map<String, Value> = match keys {
            Some(keys) => keys
                .iter()
                .filter_map(|key| {
                    self.preferences
                        .get(key)
                        .map(|value| (key.clone(), value.clone()))
                })
                .collect(),
            None => self
                .preferences
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        };
        Value::Object(picked)
    }

    /// **合并写入偏好** ✓（`null` 表示**删掉这个键** ✓ —— 否则"清空收藏"只能靠写空数组 ✓，
    /// 而那会留下一堆空壳 ✓）。
    ///
    /// **落盘** ✓：有工作区根目录就写到 `<root>/preferences.json` ✓；
    /// 纯内存模式**只改内存** ✓，但调用方能从返回里看出这一点 ✓（见工具层的 `persisted` ✓）。
    pub fn set_preferences(&mut self, values: &serde_json::Map<String, Value>) -> Result<Value> {
        for (key, value) in values {
            if value.is_null() {
                self.preferences.remove(key);
            } else {
                self.preferences.insert(key.clone(), value.clone());
            }
        }
        if let Some(persist) = &self.persist {
            let path = persist.root().join("preferences.json");
            let text = serde_json::to_string_pretty(&self.preferences(None))
                .unwrap_or_else(|_| "{}".to_owned());
            std::fs::write(&path, text).map_err(|error| {
                YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!("写不进偏好文件 {}：{error}", path.display())),
                )
            })?;
        }
        // **同步"钉住的 blob"** ✓（第 301 轮 ✓，A① 的收尾 ✓）：
        // 参考图是**用 `render_region` 产出**的 ✓ ⇒ 它天然落在"预览缓存"的管理范围里 ✓
        // ⇒ `evict_replaced_previews` 会在**下一次渲染**时把"被替换的旧预览"删掉 ✗
        // ⇒ 而它**正是用户设的参考图** ✓（调用栈实测：`write_fill_region` →
        //   `finish_mutation` → `render_region` → `evict_replaced_previews` → `remove` ✓）
        // ⇒ 症状：`set_reference` 当场成功 ✓、**下一次落笔**就从磁盘消失 ✗
        //   ⇒ `analyze_region` 报 `reference_not_found` ✓ —— **数据丢失**一类 ✓。
        // **∴ 每次偏好变化后**（设 / 清参考图都走这里 ✓）把它发给**每个文档** ✓
        //（参考图是**工作区级**的 ✓ ⇒ 哪个文档渲染都不许删它 ✓）。
        let mut pinned = std::collections::BTreeSet::new();
        if let Some(text) = self
            .preferences
            .get("reference.blob_hash")
            .and_then(Value::as_str)
        {
            if let Ok(hash) = text.parse::<yanshi_core::BlobHash>() {
                pinned.insert(hash);
            }
        }
        for document in self.documents.values_mut() {
            document.set_pinned_blobs(pinned.clone());
        }
        Ok(self.preferences(None))
    }

    /// 内存工作区（测试与嵌入式使用）。
    pub fn in_memory(settings: DocumentSettings) -> Self {
        Self::with_store(Arc::new(MemoryBlobStore::new()), settings)
    }

    /// 以指定 Blob 存储构造（测试用：注入失败存储，验 bake 失败路径）。
    pub fn with_store(store: Arc<dyn BlobStore>, settings: DocumentSettings) -> Self {
        Self {
            preview_pending_at: std::collections::BTreeMap::new(),
            doc_last_used: BTreeMap::new(),
            doc_use_seq: 0,
            doc_evictions: 0,
            doc_evictions_skipped_busy: 0,
            doc_last_eviction: None,
            store,
            inflight: Arc::new(InflightRegistry::new()),
            documents: BTreeMap::new(),
            persist: None,
            disk_len_seen: std::collections::HashMap::new(),
            assets_dir: None,
            preferences: BTreeMap::new(),
            open_changesets: BTreeMap::new(),
            warned_time_insensitive_brushes: std::collections::HashSet::new(),
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
        // **注入存储编码** ✓（用户实测：包里 91% 是未压缩 RGBA ⇒ 写入即压 ✓）。
        // **哈希仍对明文算** ✓（core 的 `put` 做的 ✓）⇒ 内容寻址/去重/导入对账都不受影响 ✓
        // —— 这条不变量由 `crates/yanshi-core/tests/blob_codec.rs` 守着 ✓。
        let store: Arc<dyn BlobStore> = Arc::new(
            persist
                .blob_store()?
                .with_codec(Arc::new(crate::blob_codec::RenderCodec)),
        );
        Ok(Self {
            preview_pending_at: std::collections::BTreeMap::new(),
            doc_last_used: BTreeMap::new(),
            doc_use_seq: 0,
            doc_evictions: 0,
            doc_evictions_skipped_busy: 0,
            doc_last_eviction: None,
            store,
            inflight: Arc::new(InflightRegistry::new()),
            documents: BTreeMap::new(),
            preferences: read_preferences(&persist),
            persist: Some(persist),
            disk_len_seen: std::collections::HashMap::new(),
            assets_dir: None,
            settings,
            created: 0,
            open_changesets: BTreeMap::new(),
            warned_time_insensitive_brushes: std::collections::HashSet::new(),
            transactions: BTreeMap::new(),
            stashes: BTreeMap::new(),
        })
    }

    /// CAS。
    pub fn store(&self) -> Arc<dyn BlobStore> {
        Arc::clone(&self.store)
    }

    /// **在飞操作登记表的共享句柄** ✓（外部测试报告 P1）。
    ///
    /// 克隆出来的是**同一张表** ✓ —— HTTP 层在取工作区锁**之前**用它做
    /// "忙不忙 / 取消 / 观察"三件事 ✓（见 `crate::inflight` 的模块说明 ✓）。
    pub fn inflight(&self) -> &Arc<InflightRegistry> {
        &self.inflight
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
        // **★ 画布尺寸上限** ✓（第 1418 轮；外部 D-12）
        //   **⇒ 原缺陷** ✗：`POST /api/documents` 接受 **100000×100000**（= **10^10 像素**）⇒
        //     实测**真的建成**了（惰性分配，所以当场不炸）⇒ 但**任何一次全幅渲染/导出**
        //     都要 ≈ **40 GB** 内存 ⇒ 变成一次**必然的 OOM** ✗
        //   **⇒ 注意**：`new_document` **工具**那条路早有 `clamp(1, 65536)` ✓
        //     ⇒ 而 **HTTP 这条路没有** ✗ ⇒ 所以同一个参数两条路行为不同 ✓
        //   **⇒ ⇒ 所以**在这里也拒绝**，并说清上限 ✓
        //   **⇒ 上限**：单边 `16384`（GPU 常见最大纹理边）＋ 总像素 `2^28`（≈ 2.68 亿 ⇒ 1 GiB RGBA）
        const MAX_CANVAS_EDGE: u32 = 16_384;
        const MAX_CANVAS_PIXELS: u64 = 1 << 28;
        if spec.width > MAX_CANVAS_EDGE || spec.height > MAX_CANVAS_EDGE {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "画布单边上限 {MAX_CANVAS_EDGE}（请求 {}×{}）⇒ 超过就必然 OOM，所以在这里拒绝",
                    spec.width, spec.height
                )),
            ));
        }
        let pixels = u64::from(spec.width).saturating_mul(u64::from(spec.height));
        if pixels > MAX_CANVAS_PIXELS {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "画布总像素上限 {MAX_CANVAS_PIXELS}（请求 {}×{} = {pixels}）⇒ 超过就必然 OOM，所以在这里拒绝",
                    spec.width, spec.height
                )),
            ));
        }
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
        // **★ C2 预算闸（RSS 修复 paint-rss）：新建也是"装入" ★**：
        // 修前闸只接在 `open_document_inner` 里，新建走 `create_document` 全程
        // 绕闸 ⇒ 连续新建 30 个文档、预算 2GB 也一次不触发卸载，直接顶到 OOM
        // （定位报告 §5 第 3 条）。新建文档磁盘原子数为 0、入量估算为 0，
        // 闸按已载入总量腾位（LRU 卸载空闲文档）；全 busy 且仍超 ⇒ 拒绝新建。
        // 注意顺序：先过闸、再创建——闸拒绝时新文档尚未创建，无半截状态；
        // 闸里卸载的是**别的**文档，不含本 id。
        self.enforce_doc_budget_for_open(&doc_id)?;
        let document = Document::create(
            Arc::clone(&self.store),
            spec.clone(),
            actor,
            session,
            self.settings.clone(),
        )?;
        self.documents.insert(doc_id.clone(), document);
        // C2：新建即载入、即「刚使用过」——记 LRU 序号。
        self.touch_doc(&doc_id);
        self.created += 1;
        // **初始原子走提交路径的同一条咽喉** ✓（`flush_journal` ✓）⇒ 一份实现 ✓、
        // 水位与落盘同步推进 ✓（旧写法自己循环 `append_atom` ✗ ⇒ 水位会停在 0 ⇒ 下次提交**重复追加** ✓）。
        // **初始原子走提交路径的同一条咽喉** ✓（`flush_journal` ✓）⇒ 一份实现 ✓、
        // 水位与落盘同步推进 ✓（旧写法自己循环 `append_atom` ✗ ⇒ 水位会停在 0 ⇒ 下次提交**重复追加** ✓）。
        self.flush_journal(&doc_id)?;
        if let Some(persist) = &self.persist {
            persist.save_meta(&DocumentMeta {
                doc_id: doc_id.clone(),
                width: spec.width,
                height: spec.height,
                created_at: yanshi_core::now_ms(),
                tokens: Vec::new(),
                // **新文档从第一枚令牌起就写摘要** ✓（审计 H-2③）⇒ 不存在明文窗口 ✓。
                tokens_hashed: true,
            })?;
            // **★ 顺手把"最新可用快照"落盘（待办 ① ✓，2026-10-15 ✓）★**
            //   **调用处不写"存不存在"的判断** ✓：`save_snapshot` **自带幂等** ✓（已存在 ⇒ `Ok(false)` ✓）
            //     —— 引擎已有的不做第二遍 ✓。
            //   **失败只记 stderr** ✗✓：快照只是**加速**用的 ✓ ⇒ **绝不让提交失败** ✓。
            let document = self.documents.get(&doc_id).expect("刚插入");
            if let Some(snapshot) = document.latest_usable_snapshot() {
                if let Err(error) = persist.save_snapshot(&doc_id, snapshot) {
                    eprintln!("快照落盘失败（不影响提交 ✓）：{error}");
                }
            }
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
        // **副本目标 id 也要过白名单** ✓（审计 P3 #14）：`create_document` 内部有校验 ✓
        // 但在它之前 `open_document(from)` 已跑完 ✓ —— 校验提前到入口 ✓
        // 让 400 在门口就发生，而不是静默落 `docs/_invalid` ✓。
        crate::persist::FileStore::validate_doc_id(to)?;
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

    /// **C2 预算读取**（期 2）：`YANSHI_DOC_MEMORY_BUDGET_MB`
    /// （缺省 2048 MB）——沿 GC 闸形态：调用时现读、解析失败回退
    /// 缺省（可调才可测；小内存部署可调低，宁可拒绝不被 OOM 杀）。
    pub(crate) fn memory_budget_bytes() -> u64 {
        std::env::var("YANSHI_DOC_MEMORY_BUDGET_MB")
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
            .map(|mb| mb.saturating_mul(1_024 * 1_024))
            .unwrap_or(2_048 * 1_024 * 1_024)
    }

    /// **C2 数量护栏读取**（期 2）：`YANSHI_DOC_MAX_LOADED`
    /// （缺省 64）——预算闸之外的第二道廉价护栏，防句柄泄漏型
    /// 累积；软闸：无空闲文档可卸时放行，不把服务卡死。
    fn doc_max_loaded() -> usize {
        std::env::var("YANSHI_DOC_MAX_LOADED")
            .ok()
            .and_then(|text| text.trim().parse::<usize>().ok())
            .filter(|count| *count >= 1)
            .unwrap_or(64)
    }

    /// 一份已载入文档的估算驻留：原子口径 + 渲染侧常驻缓存**实测**
    /// （原子口径只计日志加状态加折叠；位图-heavy 文档的渲染缓存
    /// 另行叠实测——设计 C2 的硬要求）。
    ///
    /// **★ RSS 修复 paint-rss：实测口径从"只看位图"扩到"全部渲染缓存" ★**：
    /// 修前这里只加 `bitmap_cache_stats().bytes`（位图约 5–15MB），4K 全配方
    /// 文档估 ~17MB、实测 ~260MB ⇒ 2GB 预算下 30 个文档全程不触发卸载
    /// （定位报告 §5 第 3 条）。现加 `renderer_retained_bytes()`（池保留 +
    /// tile + 位图 + 区域块 + below/above + 蒙版 + 矢量光栅），估算与实测同量级，
    /// 预算闸才能接住。`LoadedDocumentInfo.cache_bytes` 仍只报位图（字段语义不变）。
    fn estimate_loaded_doc_bytes(document: &Document) -> u64 {
        (document.atom_count() as u64)
            .saturating_mul(BYTES_PER_ATOM)
            .saturating_add(document.renderer_retained_bytes() as u64)
    }

    /// 文档是否「使用中」：有 inflight 操作、或有打开的
    /// changeset / transaction ⇒ 预算卸载必须跳过（卸掉正在
    /// 编辑的文档等于把用户手里的工作抽走）。
    fn document_is_busy(&self, doc_id: &str) -> bool {
        self.inflight.op(doc_id).is_some()
            || self
                .open_changesets
                .keys()
                .any(|(doc, _session)| doc == doc_id)
            || self
                .transactions
                .keys()
                .any(|(doc, _session)| doc == doc_id)
    }

    /// 刷新某文档的最近使用序号（LRU 的时钟）。
    fn touch_doc(&mut self, doc_id: &str) {
        self.doc_use_seq += 1;
        let seq = self.doc_use_seq;
        self.doc_last_used.insert(doc_id.to_owned(), seq);
    }

    /// 选卸载候选：已载入文档里**最久未用且空闲**的一份（不含
    /// `exclude`）；途经的 busy 文档逐个计数后跳过（跳过并计数，
    /// 卸载行为全程可观测）。无候选 ⇒ `None`。
    fn lru_idle_candidate(&mut self, exclude: &str) -> Option<String> {
        let mut by_age: Vec<(u64, String)> = self
            .documents
            .keys()
            .filter(|id| id.as_str() != exclude)
            .map(|id| (self.doc_last_used.get(id).copied().unwrap_or(0), id.clone()))
            .collect();
        by_age.sort();
        for (_seq, id) in by_age {
            if self.document_is_busy(&id) {
                self.doc_evictions_skipped_busy += 1;
                continue;
            }
            return Some(id);
        }
        None
    }

    /// 执行一次预算卸载（纯内存动作：持久化在盘，下次打开从
    /// 原子日志重载）并留痕。
    fn evict_document_for_budget(&mut self, doc_id: &str, reason: &str) {
        self.documents.remove(doc_id);
        self.doc_last_used.remove(doc_id);
        self.doc_evictions += 1;
        self.doc_last_eviction = Some((doc_id.to_owned(), reason.to_owned()));
        // RSS 修复 paint-rss：只丢文档不归还堆，卸载的内存仍有 ~60% 滞留 arena
        // （与 close 同一分配器层，定位报告 §3.2）⇒ 卸完即归还一次。
        yanshi_heap::release_heap_to_os();
    }

    /// **C2 预算闸**（期 2）：在真正装入一份**尚未载入**的文档
    /// 之前调用。先过数量护栏（软），再过内存预算（硬）：估算
    /// 总量超预算 ⇒ 按 LRU 卸载空闲文档腾位；全部不可卸载且仍
    /// 超 ⇒ `ResourceExhausted` 并给出估算与上限两个数字
    /// （不许硬开到 OOM，与 GC 闸同一处置哲学）。
    fn enforce_doc_budget_for_open(&mut self, incoming_doc_id: &str) -> Result<()> {
        let max_loaded = Self::doc_max_loaded();
        while self.documents.len() >= max_loaded {
            match self.lru_idle_candidate(incoming_doc_id) {
                Some(victim) => self.evict_document_for_budget(
                    &victim,
                    "数量护栏：已载入文档数达到 YANSHI_DOC_MAX_LOADED 上限",
                ),
                None => break,
            }
        }
        let budget_bytes = Self::memory_budget_bytes();
        // 待载入文档按其磁盘原子数估算（位图缓存尚未产生，按 0）；
        // 与 GC 闸一样先读一遍原子计数，不为估算另开近似通道。
        let incoming_atoms = match &self.persist {
            Some(persist) => persist
                .load_atoms(incoming_doc_id)
                .map(|atoms| atoms.len() as u64)
                .unwrap_or(0),
            None => 0,
        };
        let incoming_estimate = incoming_atoms.saturating_mul(BYTES_PER_ATOM);
        loop {
            let loaded_estimate: u64 = self
                .documents
                .values()
                .map(Self::estimate_loaded_doc_bytes)
                .sum();
            if loaded_estimate.saturating_add(incoming_estimate) <= budget_bytes {
                return Ok(());
            }
            match self.lru_idle_candidate(incoming_doc_id) {
                Some(victim) => self.evict_document_for_budget(
                    &victim,
                    "内存预算：估算总量超过 YANSHI_DOC_MEMORY_BUDGET_MB 上限",
                ),
                None => {
                    return Err(YanshiError::new(
                        ErrorCode::ResourceExhausted,
                        ErrorContext::detail(format!(
                            "打开文档 {incoming_doc_id} 后已载入文档估算内存将达 {} MB\
                             （已载入 {} MB + 待载入约 {} MB），超过上限 {} MB\
                             （YANSHI_DOC_MEMORY_BUDGET_MB），且其余已载入文档都在使用中\
                             （inflight / 打开的 changeset / transaction）无法卸载\
                             ⇒ 已拒绝，避免进程被 OOM 杀掉。\
                             出路：先 close_document 掉不用的文档，或调高预算。",
                            (loaded_estimate + incoming_estimate) / (1_024 * 1_024),
                            loaded_estimate / (1_024 * 1_024),
                            incoming_estimate / (1_024 * 1_024),
                            budget_bytes / (1_024 * 1_024),
                        )),
                    ));
                }
            }
        }
    }

    /// **已载入文档的内存观测**（C2 只读出口）：每文档的原子数、
    /// 估算驻留（与预算闸同源）、位图缓存实测、最近使用序号。
    pub fn loaded_documents(&self) -> Vec<LoadedDocumentInfo> {
        self.documents
            .iter()
            .map(|(doc_id, document)| LoadedDocumentInfo {
                doc_id: doc_id.clone(),
                atoms: document.atom_count(),
                estimated_bytes: Self::estimate_loaded_doc_bytes(document),
                cache_bytes: document.bitmap_cache_stats().bytes as u64,
                retained_bytes: document.renderer_retained_bytes() as u64,
                last_used_seq: self.doc_last_used.get(doc_id).copied().unwrap_or(0),
            })
            .collect()
    }

    /// **卸载观测**（C2）：累计卸载数、busy 跳过数、最近一次留痕。
    pub fn doc_eviction_stats(&self) -> DocEvictionStats {
        DocEvictionStats {
            evictions: self.doc_evictions,
            skipped_busy: self.doc_evictions_skipped_busy,
            last_evicted_doc: self
                .doc_last_eviction
                .as_ref()
                .map(|(doc_id, _reason)| doc_id.clone()),
            last_reason: self
                .doc_last_eviction
                .as_ref()
                .map(|(_doc_id, reason)| reason.clone()),
        }
    }

    /// 打开（或返回已打开的）文档：从磁盘加载日志并重建状态。
    /// **打开文档（计时包装 ✓）** —— 第 71 轮 ✓：把整段耗时记进文档 ✓，
    /// 供 `get_document` 以 `open_ms` 报出 ✓（**加载在工具开始之前 ⇒ 装不进 `total_ms`** ✗）。
    pub fn open_document(&mut self, doc_id: &str) -> Result<&mut Document> {
        // **C-02（第三轮复审）**：先校验 doc_id ⇒ 否则 ../ 可穿越写任意目录
        crate::persist::FileStore::validate_doc_id(doc_id)?;
        let started = std::time::Instant::now();
        // 第一次调用**真正做加载** ✓（第二次只是 map 查找 ✓ ⇒ 极快 ✓）。
        self.open_document_inner(doc_id)?;
        // C2：打开即「使用」——刷新 LRU 序号（冷开、热开都算）。
        self.touch_doc(doc_id);
        let ms = started.elapsed().as_millis() as u64;
        let document = self.open_document_inner(doc_id)?;
        // **只在"冷开"时写入** ✗：已打开的文档走的是"取出来"那条 ✓ ⇒ 不该覆盖真实读数 ✓。
        if document.open_ms() == 0 {
            document.set_open_ms(ms);
        }
        Ok(document)
    }

    fn open_document_inner(&mut self, doc_id: &str) -> Result<&mut Document> {
        if !self.documents.contains_key(doc_id) {
            // C2 预算闸（期 2）：装入前先腾位/拒绝——闸只对「新
            // 装入」触发，已载入文档的热开不重复过闸。
            self.enforce_doc_budget_for_open(doc_id)?;
            // **打开路径的分阶段计时** ✓（第 1493 轮 ✓）—— 外部报告实测 15.1 MB 文档
            // `get_document` 要 **294.6 s** ✗，但报告自己说**没做 profiling** ✗。
            // 读代码只能把候选排到 `load_atoms`／`load_render`／`load_preview`／`load_meta`
            // 与令牌恢复 ✗（`put`／`restore_persisted_*`／`with_atoms`／`full_fold` 都已按量级排除 ✓）。
            // 用 `YANSHI_OPEN_TIMING=1` 打开 ✓ ⇒ 把六段耗时打到 stderr ✓，报告方可用**同一份文档**
            // 复跑取得分层数据 ✓（**∴ 我构造的输入 ≠ 产品真实输入” ✓）。
            let timing = std::env::var_os("YANSHI_OPEN_TIMING").is_some();
            let mut marks: Vec<(&str, std::time::Instant)> = Vec::new();
            let mut mark = |name: &'static str| {
                if timing {
                    let now = std::time::Instant::now();
                    let epoch_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    eprintln!("open_mark epoch_ms={epoch_ms} name={name}");
                    marks.push((name, now));
                }
            };
            mark("open");
            let (atoms, base, render, preview) = match &self.persist {
                Some(persist) => {
                    mark("load_atoms");
                    // **★★★ 「快照 ＋ 尾巴」打开**暂时停用** ✗ ★★★**（2026-10-16 ✓；**本机独立复现 ＋ 第三方同报** ✓）
                    //
                    // **∴ 症状 ✗**（**本机实测 ✓**）：文档原子数 ≥ `SNAPSHOT_LAG_ATOMS`（**1000** ✓）时
                    //   磁盘上会出现 `snapshot-<seq>.json` ✓ ⇒ 之后**换一个进程重开** ✓ 就走下面那条
                    //   「快照 ＋ 尾巴」✓ ⇒ **开出来 `head_seq` 是 0** ✗、**再提交一笔直接失败** ✗：
                    //   `渲染区域 Bbox {0,0,1,1} 与文档 0×0 不相交` ✓，此后 `get_document` 取不到 ✗。
                    //   （实测脚本：造 1000 原子 ⇒ 落快照 ⇒ 新进程重开 ⇒ `layers=3/objects=997` 但
                    //    `head_seq=0` ✓ ⇒ 再画一笔 `ok=false` ✓。）
                    //
                    // **∴ 根因 ✗**（与第三方报告的诊断一致 ✓）：快照只喂给 `open_from_snapshot`（**状态** ✓），
                    //   而**日志**只装了快照之后的 tail ✓ ⇒ 后续提交走 `IncrementalFolder` 时
                    //   **从 tail 的零基础开始折叠** ✗ ⇒ **快照里那些图层在第一次提交后就没了** ✗
                    //   ⇒ **∴ 这不是"慢"** ✗ 而**是"可编辑文档被开坏"** ✓ ★
                    //   **∴ 原子日志本身没丢 ✓**（磁盘上 `atoms.jsonl` 是完整的 ✓）—— 丢的是"这一进程理解的状态" ✓。
                    //
                    // **∴ 处置 ✗**：**可编辑的文档一律从完整权威日志打开** ✓（`load_atoms` 全量 ＋ `base=None` ✓），
                    //   直到**折叠与历史操作都支持"快照基座"**为止 ✓ —— 也就是第三方验证过的那条路 ✓
                    //   （它实测：冷开续画、撤销、再次冷开都保留原内容 ✓、PNG 逐字节一致 ✓）。
                    //   **∴ 代价如实说 ✗**：**冷开要多读多折历史** ✓（大文档上这是实打实的 I/O ✗）——
                    //   **∴ 但"打开就能用"优先于"打开快"** ✓。
                    //   **∴ 什么时候能把它放回来 ✗**：当 `IncrementalFolder` 能**以快照为基座**折叠 ✓
                    //   （或提交路径能区分"快照基座 ＋ tail" ✓）⇒ 那时**带着判据**（冷开 ⇒ 续写 ⇒ 层/对象/锚点
                    //   一个不少 ＋ 两次冷开出图逐字节一致 ✓）再打开 ✓。
                    //   **∴ `open_from_snapshot` 与 `load_atoms_since` 先留着** ✓（都是 `pub` ✓，
                    //   将来恢复这条路要用 ✓；现在没有调用点 ✓）。
                    let (atoms, base) = (persist.load_atoms(doc_id)?, None);
                    mark("load_render");
                    let render = persist.load_render(doc_id)?;
                    mark("load_preview");
                    let preview = persist.load_preview(doc_id)?;
                    mark("loads_done");
                    (atoms, base, render, preview)
                }
                None => (Vec::new(), None, None, None),
            };
            let atoms_len = atoms.len();
            // **★ 有快照时尾巴可以是空的** ✗✓：只按 `atoms.is_empty()` 判会把**好文档误报成不存在** ✗
            //   ⇒ 判据是"**既没尾巴、也没快照**" ✓（`§49 之补十六` ✓）。
            if atoms.is_empty() && base.is_none() {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {doc_id} 不存在")),
                ));
            }
            if timing {
                mark("before_doc_open");
            }
            // **`base = None` 时与老 `open` 等价** ✓ ⇒ 没持久化存储那条路**行为不变** ✓
            //   （`Document::open` 本身**没动** ✗ ✓）。
            let mut document = Document::open_from_snapshot(
                Arc::clone(&self.store),
                doc_id,
                base,
                atoms,
                self.settings.clone(),
            )?;
            if timing {
                mark("after_doc_open");
            }
            // **★ 记下"打开时读了多少原子"（2026-10-15 ✓；待办 ①）★**
            //   现在它**等于全量** ✓（因为还没有快照 ✓）⇒ ⇒ 将来加了"快照 ＋ 尾巴"后
            //   它应当**明显变小** ✓ —— 判据就断言这一点 ✓（变异：退化成全量读 ⇒ 变大 ⇒ 红 ✓）。
            document.set_atoms_read_on_open(atoms_len as u64);
            if let Some((seq, png)) = render {
                // 打开即图片（14.5/6.2）：**复用持久化渲染**，不重放历史。
                //
                // `restore_persisted_render` **不解码像素** ✗（4K 解码实测 153.7s ✓）——
                // 它只从 PNG 头读尺寸 ✓，把"整幅、且与 HEAD 一致"的那份记成**整幅区域渲染的缓存** ✓
                //（下一次整幅请求直接命中 ✓，省掉实测 142s 的重渲染 ✓）。
                let hash = self.store.put(&png)?;
                if timing {
                    mark("after_render_put");
                }
                document.restore_persisted_render(seq, hash, &png);
            }
            if timing {
                mark("after_render_restore");
            }
            if let Some((seq, png)) = preview {
                // **小图才是增量预览的基座** ✓（256² 解码 ~1s ✓）：接回像素 ✓ + 算出脏区 ✓
                // ⇒ 第一个新连接的预览只重渲染 `(seq, HEAD]` ✓，而不是整幅 ✗
                //（实测落后 9 个 atom 时整幅 122s ✓）。
                let hash = self.store.put(&png)?;
                if timing {
                    mark("after_preview_put");
                }
                document.restore_persisted_preview(seq, hash, &png);
            }
            // 恢复令牌（12.7）。
            if let Some(persist) = &self.persist {
                if timing {
                    mark("before_meta");
                }
                if let Some(meta) = persist.load_meta(doc_id)? {
                    // **旧格式（明文）按原文恢复，新格式（摘要）直接入表** ✓（审计 H-2③）：
                    // 迁移只发生在**下一次写入** ✓ ⇒ 读路径必须认两种格式 ✓，不能只认一种 ✗。
                    let hashed = meta.tokens_hashed;
                    for record in meta.tokens {
                        if hashed {
                            document.authority_mut().restore_digest(
                                &record.token,
                                &record.actor,
                                record.role,
                            )?;
                        } else {
                            document.authority_mut().restore(
                                &record.token,
                                &record.actor,
                                record.role,
                            )?;
                        }
                    }
                }
            }
            // 末尾也要打一个点 ✓：差值按相邻两点算 ⇒ 没有后继点的那一段**永远打不出来** ✗
            //（上一版就漏了 `restore_meta` ✓）。
            if timing {
                mark("after_meta");
            }
            if timing {
                mark("restore_meta");
                mark("done");
                let mut report = format!("open_timing doc={doc_id} atoms={atoms_len}");
                for pair in marks.windows(2) {
                    report.push_str(&format!(
                        " {}={}ms",
                        pair[0].0,
                        pair[1].1.duration_since(pair[0].1).as_millis()
                    ));
                }
                eprintln!("{report}");
            }
            // **日志是从磁盘读来的 ⇒ 水位直接推到 HEAD** ✓：这些原子本来就在 `atoms.jsonl` 里 ✓，
            // 下一次提交只该追加**新**的那些 ✓（漏了这一行的后果是**重复追加** ✗ —— 可见 ✓，
            // 比**漏写** ✓ 好发现，这正是水位方向选"保守重放"的理由 ✓）。
            let head = document.log().head_seq();
            document.set_journal_watermark(head);
            self.documents.insert(doc_id.to_owned(), document);
            // **"只解压到 `--root`"那条路也要能把省掉的位图补回来** ✓。
            //
            // **为什么必须在这里做** ✗：README 的还原方式是"把包解开到 `<root>/`" ✓ ——
            // 那条路**根本不经过 `import_project`** ✓（没有工具调用 ✓）⇒ 只有渲染器直接读 CAS ✓。
            // 若不在这里补 ✓，`export_project` 省掉的那些位图就**永远缺着** ✗
            // ⇒ 打开是**静默不完整的画** ✓（有告警 ✓，但谁也不该被迫看这种画 ✓）。
            //
            // **正常文档零成本** ✓：每个引用先问一次 `store.exists` ✓（几百次文件存在性检查 ✓），
            // 只有**真的缺**且**配方可重放**才落笔 ✓ ⇒ 平时的打开**一次重放都不做** ✓；
            // 补回来的会**写进本地 CAS** ✓ ⇒ 第二次打开就是普通的读 ✓（本地缓存 ✓）。
            let (replayed, unreplayable) = self.materialize_pending_replayable_blobs(doc_id)?;
            // **"全部重放失败"也必须记账** ✓（第 725 轮；外部审计 S-03）
            //   **⇒ 缺陷**：条件原来只有 `replayed > 0` ✗ ⇒ 当 `replayed == 0` 且
            //     `unreplayable > 0`（**一个都没补回来**）时，`set_unreplayable_blobs` **不执行** ✗
            //     ⇒ 文档状态把"全失败"报成 `blobs_unreplayable = 0` ✓ ⇒ **最坏的一种静默** ✓
            //     （调用方看到"没有不可重放的" ⇒ 以为画是完整的 ✓）。
            //   **⇒ 判据**：`blobs_replayed == 0` 且有不可重放的 ⇒ 那个计数必须 > 0 ✓。
            if replayed > 0 || unreplayable > 0 {
                if let Some(document) = self.documents.get_mut(doc_id) {
                    document.set_replayed_blobs(replayed);
                    document.set_unreplayable_blobs(unreplayable);
                }
            }
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
        // **C-02（第三轮复审）**：先校验 doc_id ⇒ 否则 ../ 可穿越写任意目录
        crate::persist::FileStore::validate_doc_id(&doc_id)?;
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

    /// **只读判断："这份文档已经存在"** ✓ —— **内存里已打开 ✓ 或磁盘上有它的目录 ✓**。
    ///
    /// **判据与 [`Workspace::import_project`] 的"只导入、绝不覆盖"逐字一致** ✓
    /// （`persist.doc_dir(..).exists() || documents.contains_key(..)` ✓）——
    /// **一处定义、多处复用** ✓，免得两个"存在"慢慢漂移 ✗。
    ///
    /// **不创建、不加载** ✓（调用方要打开就自己 [`Workspace::open_document`] ✓）。
    /// **为什么不查 `document_mut`** ✗：那只在**内存**里找 ✓ ⇒ 会漏掉
    /// "磁盘上有、这次进程还没打开" ✓ —— 真实缺陷（2026-10-06 ✓）里
    /// `new_document` 正是这样把"已存在"错判成"该新建" ✓，再被底层"已存在于磁盘"拒绝 ✗。
    pub fn document_exists(&self, doc_id: &str) -> bool {
        let in_memory = self.documents.contains_key(doc_id);
        match &self.persist {
            Some(persist) => persist.doc_dir(doc_id).exists() || in_memory,
            None => in_memory,
        }
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
        // 磁盘文档：本进程未加载的已落盘文档，其日志引用同样是 GC 根。
        // 只看内存会把它们引用的旧 blob 判成孤儿删掉（另一进程的文档再打开
        // 就缺像素）。已在内存的跳过（上面已并入）；读日志失败时直接报错、
        // 不回收——删数据不可逆，根集不明时宁可不删（fail-closed）。
        // 日志引用是活跃 Manifest 的超集（Manifest 里的 blob 都来自某原子），
        // 所以只并日志根就够了；钉住 blob 是纯内存状态、无落盘，不在本修复内。
        if let Some(persist) = &self.persist {
            for doc_id in persist.list_documents()? {
                if self.documents.contains_key(&doc_id) {
                    continue;
                }
                for atom in persist.load_atoms(&doc_id)? {
                    extra_roots.extend(atom.all_blob_refs());
                }
            }
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

    /// **取可变文档引用** ✓（**只在内存里找** ✗ —— 不加载 ✓、不创建 ✓）。
    ///
    /// **不存在 ⇒ `reference_not_found`** ✓。要"打开磁盘上已有的"用
    /// [`Workspace::open_document`] ✓，要"没有就建"用 [`Workspace::open_or_create`] ✓，
    /// 要**判"是否已存在"**用 [`Workspace::document_exists`] ✓（**内存 ∪ 磁盘** ✓）。
    ///
    /// **别用"本函数能不能取到"来判存在** ✗ —— 那**只看内存** ✓。这条旧注释原先写着
    /// "打开（**或按需创建**）文档" ✗，而实现从来没有按需创建 ✓ ⇒ 2026-10-06 的
    /// `new_document` 缺陷排查一开始就被它误导 ✓（描述与实现相反，正是本项目最忌讳的一类 ✗）。
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
        // C2：LRU 记账随文档一并摘除（序号本身单调，不复用）。
        self.doc_last_used.remove(doc_id);
        let removed = self.documents.remove(doc_id).is_some();
        if removed {
            // RSS 修复 paint-rss：只丢文档不归还堆，关完仍有 ~60% 滞留 arena
            // （定位报告 §3.2：关完 12 文档静置后仍滞留 2,073MB）⇒ 关完即归还一次。
            yanshi_heap::release_heap_to_os();
        }
        removed
    }

    /// **★ 重新从磁盘加载某文档（`--watch` 用 ✓，2026-10-15 ✓）★**
    ///
    /// **语义** ✓：把内存里那份**丢掉**再重新打开 ⇒ **磁盘当真相** ✓。
    /// **为什么不是"把外部原子折叠进内存"** ✗：那会让磁盘日志里出现内存 `AtomLog` **不知道**的行 ✓
    /// ⇒ 之后工具侧再提交时按**自己的记账**发 `seq` ⇒ **可能撞号** ✗（两条不同原子共用一个 `seq` ✗）。
    /// **代价** ✓：这是**全量**加载 ✗ ⇒ 大文档下不便宜 ⇒ 与"快照落盘 ＋ 加载走快照+尾巴"那条待办**交汇** ✓。
    /// **没打开过也不报错** ✓（`close_document` 丢掉一个不存在的东西只是返回 `false` ✓）。
    pub fn reload_document(&mut self, doc_id: &str) -> Result<&mut Document> {
        self.close_document(doc_id);
        self.open_document(doc_id)
    }
    /// **诊断出口**：`disk_len_seen` 里还有多少条 ✓（第 767 轮，给判据用）
    ///
    /// **⇒ 为什么要有出口** ✗：缓存条目是**私有字段** ✓ ⇒ 判据读不到 ✗
    /// ⇒ 于是"删文档时有没有清掉它"**没法断言** ✗ ⇒ 只能靠读代码 ✓（而读代码不是判据 ✓）。
    pub fn disk_len_cache_entries(&self) -> usize {
        self.disk_len_seen.len()
    }

    /// **真正删掉一份文档** ✓（原子日志与元数据从磁盘上一并移除 ✓）。
    ///
    /// **与 `close_document` 的区别必须保留** ✗：关闭只把它从**内存**里放下 ✓
    ///（磁盘上还在 ✓，下次打开就回来 ✓）；删除是**不可逆**的 ✓ —— 产品负责人要的正是后者 ✓
    /// ⇒ 两者**各自命名、各自成路** ✓，绝不让一个悄悄变成另一个 ✗。
    ///
    /// **三条硬规矩** ✓：
    ///
    /// 1. **不存在就说"不存在"** ✗：内存里没有、磁盘上也没有 ⇒ `reference_not_found`
    ///    （"删成功了"而其实什么也没删 ✓，是这里最坏的一种回答 ✗）；
    /// 2. **正在使用中 ⇒ 拒绝** ✗：文档还有**实时订阅者**（WebSocket 客户端 ✓）时删它 ✓
    ///    等于把人脚下的地板抽掉 ✓ ⇒ `conflict`，并说清有几个连接 ✓。
    ///    "打开着"与"只是加载在内存里"是**两件事** ✓：后者没有人在看 ✓ ⇒ 可以删 ✓；
    /// 3. **blob 按引用计数删** ✓（产品负责人的裁定 ✓）：
    ///    只有**这份文档引用**的 blob 才删 ✓，**被别的文档或 Stash 引用的一律保留** ✗
    ///    —— 详见 `document_blob_roots` 与 `referenced_blobs_except` 的说明 ✓。
    ///    **别的来源的孤儿**（历史遗留、没有任何文档引用 ✓）**不在本次范围内** ✗：
    ///    那是孤儿回收（`collect_garbage` ✓）的事 ✓，这里**不冒领** ✓。
    pub fn delete_document(&mut self, doc_id: &str) -> Result<Value> {
        // **路径安全** ✗：`doc_id` 会被拼进目录名 ✓ ⇒ 形状不对就当场拒绝 ✓
        //（删除是不可逆动作 ✓，宁可多一条检查 ✗）。
        if doc_id.is_empty()
            || doc_id.contains('/')
            || doc_id.contains('\\')
            || doc_id.contains("..")
        {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("文档 id 不合法：{doc_id:?}")),
            ));
        }
        let subscribers = self
            .documents
            .get(doc_id)
            .map(|document| document.broadcaster().len())
            .unwrap_or(0);
        if subscribers > 0 {
            return Err(YanshiError::new(
                ErrorCode::Conflict,
                ErrorContext::detail(format!(
                    "文档 {doc_id} 正在使用中（{subscribers} 个实时连接）⇒ 先关掉正在打开它的标签页 / 客户端，再删除"
                )),
            ));
        }
        let dir = self.persist.as_ref().map(|persist| persist.doc_dir(doc_id));
        let persisted = dir.as_ref().map(|path| path.exists()).unwrap_or(false);
        let in_memory = self.documents.contains_key(doc_id);
        if !in_memory && !persisted {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 不存在 ⇒ 没有可删除的东西")),
            ));
        }
        // **引用计数必须在删之前算** ✗：文档目录一删，它的日志就读不出来了 ✓
        //（`document_blob_roots` 要读它 ✓）。
        let referenced_by_target = self.document_blob_roots(doc_id)?;
        let referenced_elsewhere = self.referenced_blobs_except(Some(doc_id))?;
        let only_here: Vec<String> = referenced_by_target
            .difference(&referenced_elsewhere)
            .map(|hash| hash.to_string())
            .collect();
        // **跨服务误删防护**（GC 隔离分期）：`only_here` 是"只有这份文档引用"，
        // 但"引用"只看**本进程**——别的服务刚 `put`、还没 `import` 的 blob
        // 在这里也是"只有它引用"（甚或零引用）⇒ 不能跟着删。
        // 只删年龄 ≥ 门槛的；新鲜的留下来当孤儿，由 TTL GC 回收。
        // `list()` 失败或 blob 不在清单里时宁可全留（fail-safe），不可误删。
        let now = yanshi_core::now_ms();
        let ages: std::collections::BTreeMap<String, i64> = self
            .store()
            .list()
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| (entry.blob_hash.to_string(), entry.age_seconds(now)))
                    .collect()
            })
            .unwrap_or_default();
        let mut deletable: Vec<String> = Vec::new();
        let mut deferred_fresh = 0usize;
        for hash in &only_here {
            let old_enough = ages
                .get(hash)
                .map(|age| *age >= DELETE_DOCUMENT_BLOB_GRACE_SECONDS)
                .unwrap_or(false);
            if old_enough {
                deletable.push(hash.clone());
            } else {
                deferred_fresh += 1;
            }
        }
        let mut freed_bytes = 0u64;
        if let Some(dir) = &dir {
            if persisted {
                freed_bytes = directory_bytes(dir);
                // **先删磁盘、再摘内存** ✓：磁盘删失败就**什么都没变** ✓
                //（反过来的话会留下"内存里没了、磁盘上还在"的半截状态 ✗）。
                std::fs::remove_dir_all(dir).map_err(|error| {
                    YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!(
                            "删不掉文档目录 {}：{error} ⇒ 文档保持原样（没有半删状态）",
                            dir.display()
                        )),
                    )
                })?;
            }
        }
        let removed_from_memory = self.documents.remove(doc_id).is_some();
        if removed_from_memory {
            // RSS 修复 paint-rss：与 `close_document` 同理——删完即归还一次堆
            // （定位报告 §3.2：DELETE 删完仍滞留 59.5%，与 close 同形）。
            yanshi_heap::release_heap_to_os();
        }
        // **删除时也要清掉"上次看到的盘上长度"** ✓（第 767 轮）
        //   **⇒ 缺陷（我自己第 762 轮引入的）**：`disk_len_seen` 按 `doc_id` 记 ✓
        //     ⇒ 文档删掉后**条目留着** ✗ ⇒ ①**小内存泄漏**（每个删过的 id 一条 ✓）；
        //     ②**更麻烦的一条**：同 id 重建后，若新文件长度**恰好等于**旧长度 ✓
        //     ⇒ 缓存命中 ⇒ **跳过读盘** ⇒ **漏掉别的写者的原子** ✗（概率低但会静默错 ✓）。
        self.disk_len_seen.remove(doc_id);
        // **真正删存储这一件事只有一处实现** ✓（`collect_orphan_blobs` ✓）——
        // 删除这层只负责**算出"哪些只有它引用"** ✓。
        //（上面已按年龄筛过：`deletable` 才进删除，新鲜的留在 `deferred_fresh` 里。）
        let (blobs_deleted, blob_bytes_freed) = self.collect_orphan_blobs(&deletable)?;
        Ok(json!({
            // **`ok` 必须有** ✗：本服务**每一个**回执都带它 ✓，而第一版这里漏了 ✓
            // ⇒ 查看器按 `value.ok` 判成败 ✓ ⇒ **删除其实成功了，界面却报 "unknown"** ✗
            //（真浏览器判据当场抓到 ✓ —— 这正是"同一份回执要两处都读得懂"的那类错 ✓）。
            "ok": true,
            "deleted": true,
            "doc_id": doc_id,
            "freed_bytes": freed_bytes,
            "removed_from_memory": removed_from_memory,
            "blobs_referenced": referenced_by_target.len(),
            "blobs_deleted": blobs_deleted,
            "blob_bytes_freed": blob_bytes_freed,
            "blobs_shared_kept": referenced_by_target
                .intersection(&referenced_elsewhere)
                .count(),
            "blobs_deferred_fresh": deferred_fresh,
            "note": format!("文档目录已删；只删了**只有这份文档引用**的 blob，被别的文档或 Stash 引用的一律保留；**别的来源的孤儿不动**（孤儿回收是 collect_garbage 的事，不是删除的事）；刚写入不久（{} 秒内）的 blob 即使只有这份文档引用也暂不删（别的服务可能正要 import），留给 TTL GC 当孤儿回收，个数见 blobs_deferred_fresh", DELETE_DOCUMENT_BLOB_GRACE_SECONDS),
        }))
    }

    /// **某一份文档引用到的 blob 集合** ✓（日志的引用闭包 ＋ 活跃 Manifest ✓）。
    ///
    /// **没打开的文档要从磁盘读** ✗：`self.documents` 里只有**打开着**的文档 ✓，
    /// 而磁盘上还躺着一堆没打开的 ✓ —— 只看内存那几份 ✓，就会把"别的作品的 blob"
    /// 当成无人引用而删掉 ✗（那正是"删一份、坏另一份" ✓）。
    fn document_blob_roots(
        &self,
        doc_id: &str,
    ) -> Result<std::collections::BTreeSet<yanshi_core::BlobHash>> {
        let mut roots = std::collections::BTreeSet::new();
        if let Some(document) = self.documents.get(doc_id) {
            roots.extend(document.log().blob_roots());
            // **活跃 Manifest 也要算** ✓：它连**图层**的 blob 一起算 ✓（只数对象的会漏 ✗）。
            roots.extend(document.state().active_blob_manifest());
            return Ok(roots);
        }
        if let Some(persist) = &self.persist {
            for atom in persist.load_atoms(doc_id)? {
                roots.extend(atom.all_blob_refs());
            }
        }
        Ok(roots)
    }

    /// **除 `skip` 之外，整个工作区引用到的 blob** ✓
    ///（内存里的文档 ＋ **磁盘上没打开的文档** ＋ **Stash** ✓）。
    ///
    /// **Stash 必须算** ✗：设计 §899 明说「Stash 引用的 blob 归**历史级保留、不被 GC**」✓ ——
    /// 漏了它 ⇒ 删一份文档会顺手删掉"离线期间的编辑"里那个 blob ✗，
    /// 而那些编辑**还没有重放** ✓ ⇒ 用户的离线成果凭空消失 ✗。
    ///
    /// **落盘但没载入的 Stash 也要算** ✗：`with_file_store` 至今**不载入**它们 ✓
    ///（那是另一个缺口 ✗，不是本轮的范围 ✗）—— 但"删 blob"必须对它**保守** ✓：
    /// 宁可少删几个 ✓，也不能删掉离线编辑里的像素 ✗。
    fn referenced_blobs_except(
        &self,
        skip: Option<&str>,
    ) -> Result<std::collections::BTreeSet<yanshi_core::BlobHash>> {
        let mut roots = std::collections::BTreeSet::new();
        for (doc_id, document) in &self.documents {
            if Some(doc_id.as_str()) == skip {
                continue;
            }
            roots.extend(document.log().blob_roots());
            roots.extend(document.state().active_blob_manifest());
        }
        if let Some(persist) = &self.persist {
            for doc_id in persist.list_documents()? {
                if Some(doc_id.as_str()) == skip || self.documents.contains_key(&doc_id) {
                    continue;
                }
                for atom in persist.load_atoms(&doc_id)? {
                    roots.extend(atom.all_blob_refs());
                }
            }
            roots.extend(persisted_stash_roots(persist));
        }
        for stash in self.stashes.values() {
            for atom in &stash.atoms {
                roots.extend(atom.all_blob_refs());
            }
            for text in &stash.blob_refs {
                if let Ok(hash) = text.parse() {
                    roots.insert(hash);
                }
            }
        }
        Ok(roots)
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
                        // **★ 尺寸也必须从**折叠结果**取 ✗ ★**（**真实用户报告 ✓，2026-10-16 ✓）：
                        //   **∴ 症状 ✗**：`yanshi-mcp` 对**已存在**的文档报 **0×0** ✗
                        //     而 `yanshi-serve` 报对 ✓（**同一份磁盘** ✓）⇒ 后续 `brush_stroke` 无法继续 ✗。
                        //   **∴ 根因 ✗**：这里原先**只读 `meta.json`** ✓ 且缺失时 `unwrap_or(0)` ✗
                        //     ＋ **∴ 而**上面几行**已经为 `layers`/`objects` 折过原子了** ✓
                        //       ⇒ **∴ 于是**同一个响应里出现「`width:0,height:0` ✗＋`layers:2,objects:1` ✓」
                        //         这种**自相矛盾** ✓（本机实测 ✓）。
                        //   **∴ 为什么 serve 正常 ✗**：**serve 每个请求都按需打开文档** ✓
                        //     ⇒ 走"已打开"分支 ⇒ 尺寸来自**状态** ✓；
                        //     **MCP 只**预打开自己的 `--doc`**✗ ⇒ 别的文档走"未打开"这一支 ✗
                        //       ⇒ **∴ 会话挂在 A 上时 B 就报 0×0** ✗（实测 ✓）。
                        //   **∴ 修法 ✗**：与 `layers`/`objects` **同一个来源** ✓ ——
                        //     **原子里的 `new_document` 本来就带着真实尺寸** ✓
                        //     ⇒ **引擎已有的不做第二遍** ✓；折叠给不出（空日志 ✓）才回落到 `meta.json` ✓。
                        width: if folded.state.width > 0 {
                            folded.state.width
                        } else {
                            meta.as_ref().map_or(0, |meta| meta.width)
                        },
                        height: if folded.state.height > 0 {
                            folded.state.height
                        } else {
                            meta.as_ref().map_or(0, |meta| meta.height)
                        },
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
        self.commit_timed(doc_id, atom, actor, owner, &mut CommitPhases::default())
    }

    /// **`commit` 的带计时版本** ✓（外部测试报告 P2）：分项耗时写进 `phases` ✓（微秒 ✓）。
    ///
    /// **为什么不改 `commit` 的签名** ✗：它是公开 API ✓ 且有大量调用方与测试 ✓ ⇒
    /// 加一个 `_timed` 兄弟、老的转调它 ✓（与 `Document::commit_as_timed` 同一规矩 ✓）。
    pub fn commit_timed(
        &mut self,
        doc_id: &str,
        atom: Atom,
        actor: &str,
        owner: bool,
        phases: &mut CommitPhases,
    ) -> Result<CommitResult> {
        // **提交与落盘解耦** ✓：先拿到提交结果（可能是 Err ✓），**再按水位落盘** ✓ ——
        // 冲突拒绝在返回 Err 之前已把冲突图层原子追加进日志 ✓，
        // 旧写法 `commit_as_timed(...)?` 会**短路掉落盘** ⇒ 那条原子只在内存 ✗ ⇒ 重开就没了 ✓。
        let attempt = {
            let document = self.document_mut(doc_id)?;
            document.commit_as_timed(atom, actor, owner, None, phases)
        };
        let journal_started = std::time::Instant::now();
        self.journal(doc_id)?;
        let result = attempt?;
        phases.log_us = phases
            .log_us
            .saturating_add(journal_started.elapsed().as_micros() as u64);
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
        self.commit_changeset_timed(
            doc_id,
            atoms,
            actor,
            owner,
            changeset_id,
            &mut CommitPhases::default(),
        )
    }

    /// **`commit_changeset` 的带计时版本** ✓：各原子分项耗时**累加**进 `phases` ✓。
    pub fn commit_changeset_timed(
        &mut self,
        doc_id: &str,
        atoms: Vec<Atom>,
        actor: &str,
        owner: bool,
        changeset_id: ChangesetId,
        phases: &mut CommitPhases,
    ) -> Result<Vec<CommitResult>> {
        let mut results = Vec::with_capacity(atoms.len());
        for atom in atoms {
            // **同 `commit_timed` 的规矩** ✓：先落盘（按水位 ✓）再传播提交结果 ⇒
            // 失败路径追加的原子（冲突图层 ✓）也进 `atoms.jsonl` ✓。
            let attempt = {
                let document = self.document_mut(doc_id)?;
                document.commit_as_timed(atom, actor, owner, Some(changeset_id.clone()), phases)
            };
            let journal_started = std::time::Instant::now();
            self.journal(doc_id)?;
            let result = attempt?;
            phases.log_us = phases
                .log_us
                .saturating_add(journal_started.elapsed().as_micros() as u64);
            results.push(result);
        }
        Ok(results)
    }

    /// **只渲染某一层** ✓（单图层导出 ✓）—— 缓存那条注意事项见 `Document::render_region_raw_layer` ✓。
    pub fn render_region_raw_layer(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        layer_id: &str,
    ) -> Result<(u32, u32, Vec<u8>)> {
        self.render_region_raw_layer_with_lod(doc_id, bbox, layer_id, 0)
    }

    /// 与 [`Self::render_region_raw_layer`] 相同，但取样步长为 `1 << lod`（期 4 `export_png` 的 `lod`）。
    pub fn render_region_raw_layer_with_lod(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        layer_id: &str,
        lod: u32,
    ) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?
            .render_region_raw_layer_with_lod(bbox, layer_id, lod)
    }

    /// **只渲染某一层里的一个对象子集** ✓（一行转调，语义见 `Document` 侧 ✓）。
    pub fn render_layer_objects_subset(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        layer_id: &str,
        keep: &std::collections::BTreeSet<yanshi_core::ObjectId>,
    ) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?
            .render_layer_objects_subset(bbox, layer_id, keep)
    }

    /// 渲染区域。
    ///
    /// 只有覆盖整幅画布的渲染才落盘为「HEAD 渲染缓存」（14.5 打开即图片）；
    /// 局部 dirty 渲染虽然也进 CAS，但不会覆盖文档级缓存。
    /// 渲染区域并返回原始 RGBA8（供 `patch` 抓取源像素；不触碰渲染缓存状态）。
    ///
    /// **⚠️ 我先前把新方法插在了这段注释与它之间** ✗ ⇒ 文档被新方法"吃掉" ✓、这个函数成了无文档 ✓
    /// ⇒ `missing_docs` 抓到了 ✓。**这个错在本项目已经犯过好几次** ✗
    ///（`Workspace` ✓、`export_project` 两次 ✓、`with_assets_dir` ✓ ⇒ 这次是第四次 ✓）
    /// ⇒ **插代码前先看它上面是不是文档注释** ✓ 这条纪律要当真 ✓。
    pub fn render_region_raw(&mut self, doc_id: &str, bbox: Bbox) -> Result<(u32, u32, Vec<u8>)> {
        self.render_region_raw_with_lod(doc_id, bbox, 0)
    }

    /// 与 [`Self::render_region_raw`] 相同，但取样步长为 `1 << lod`（期 4 `export_png` 的 `lod`）。
    pub fn render_region_raw_with_lod(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        lod: u32,
    ) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?
            .render_region_raw_with_lod(bbox, lod)
    }

    /// **★ 同上，但指定"当前层"作切点 ✓ ★**（第 521 轮 ✓）：**一行转调 ✓**。
    pub fn render_region_raw_active(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        active_layer: Option<&str>,
    ) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?
            .render_region_raw_active(bbox, active_layer)
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

    /// **渲染区域、完成 job，但既不编码也不落盘** ✓（第 5 轮 ✓，P0 预览解耦 ✓）。
    ///
    /// 给"调用方明确不要这一笔的预览图"的路用 ✓：**job 照旧完成** ✓（渲染水位不受影响 ✓），
    /// 只是**不编那张 PNG** ✗（实测编码 88 ns/px ✓、4K 长笔触约 130 ms/笔 ✓）。
    /// **不落盘 ⇒ 这条比 `render_region` 少两个副作用** ✓（也不更新 `last_render_blob`
    /// 与文档缩略图 ✗）—— 那正是"不要图"的含义 ✓。
    pub fn render_region_complete_jobs(&mut self, doc_id: &str, bbox: Bbox) -> Result<()> {
        let document = self.document_mut(doc_id)?;
        document.render_region_complete_jobs(bbox)
    }

    /// 变更后预览：降采样渲染并返回可展示的预览（P-01 M1 ✓）。
    ///
    /// 与 [`Self::render_region`] 的区别：**不写磁盘 `render.png`** ✓（降采样图
    /// 不许进整幅缓存）；**也不写磁盘 `preview.png`** ✓ —— 旧 `render_region`
    /// 路径本来就不写它（只由显式缩略图刷新／冷启动兑现路径写 ✓），每笔变更
    /// 都写会把"落后基座"提前刷成 HEAD、破掉冷启动复用的增量前提
    ///（`coldstart_reuse` 判据 1 ✓）。落盘副作用与旧路一致：无。
    pub fn render_region_preview(&mut self, doc_id: &str, bbox: Bbox) -> Result<RenderedPreview> {
        let document = self.document_mut(doc_id)?;
        document.render_region_preview(bbox)
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

    /// **★ 单层缩略图预览（不污染文档级缓存）✗ ★**（第 185 轮 ✓）。
    ///
    /// **∴ 与 [`Self::render_region`] 的差别 ✗**：**它**只**渲染**被隔离的**那一层**✗
    ///   ⇒ **∴ 而**不更新 `document_thumbnail` / `full_frame_render` / `last_render_blob` ✓。
    ///   ⇒ **★ 所以**：**一张单层小图**不会**冒充**文档缩略图** ✓ ★**** ✓✓
    pub fn render_region_layer(
        &mut self,
        doc_id: &str,
        bbox: Bbox,
        layer_id: &str,
    ) -> Result<RenderedPreview> {
        let document = self.document_mut(doc_id)?;
        document.render_region_layer_preview(bbox, layer_id)
    }

    /// **最近一次渲染里被跳过的东西** ✓（裸像素出口的告警通道 ✓，见
    /// `Document::last_render_warnings` 的说明 ✓）。
    ///
    /// **为什么需要它** ✗：`render_region_raw` / `render_region_raw_layer` 的返回类型是
    /// `(宽, 高, RGBA)` ✓，**没有地方带告警** ✗ ⇒ 缺一个补丁时它们会给出
    /// **静默的不完整画面** ✗（`export_png` / `patch` 正是走这条路 ✓）。
    /// ⇒ 渲染完读一次这个 ✓，把告警一起报给调用方 ✓。
    pub fn last_render_warnings(&self, doc_id: &str) -> Vec<String> {
        self.document(doc_id)
            .map(|document| document.last_render_warnings().to_vec())
            .unwrap_or_default()
    }

    /// **最近一次渲染里缺失的本体 blob 哈希**（期 2 块 3，P2-11）。
    ///
    /// 与 `last_render_warnings` **同一条 funnel**（`Document::note_render_stats`
    /// 里一起记）：告警是人读的句子，这里是机读的哈希 —— `export_png`
    /// 靠它判定「缺的是本体还是派生」，**不解析告警文本**（人话会变，哈希不会）。
    /// 渲染没缺东西 ⇒ 空；区域缓存命中 ⇒ 与告警一起被清空（那次没渲染）。
    pub fn last_missing_blobs(&self, doc_id: &str) -> Vec<String> {
        self.document(doc_id)
            .map(|document| document.last_missing_blobs().to_vec())
            .unwrap_or_default()
    }

    /// **导入一个 `.yanshi` 工程包** ✓（真实用户报过的另一半 ✓：导出有了 ✓，导入一直没有 ✗）。
    ///
    /// **和 `export_project` 是配对的两个方向** ✓ —— 于是"备份 / 搬到另一台机器 / 给人复现问题"
    /// 这几种事**都能闭环** ✓（用户当初报这个缺口时正是为了这些 ✓）。
    ///
    /// **四条硬规矩** ✓（都来自既有教训 ✓）：
    /// 1. **只导入、绝不覆盖** ✗：目标 `doc_id` 已存在就**拒绝** ✓
    ///    （"导入把现有文档冲掉"是**不可逆**的 ✓ —— 与"不做不可逆动作"一致 ✓）；
    /// 2. **blob 按内容寻址核对** ✓：包里的路径写着 sha256 ✓ ⇒ 写进存储后再比对算出来的哈希 ✓
    ///    ⇒ 不符就**拒绝** ✗（"看起来导进去了、其实字节是坏的"最糟糕 ✓）；
    /// 3. **缺东西要说清缺什么** ✓（包里只有 `BUILD-INFO` 之类的说明文件时 ✓，报错会**列出包里有什么** ✓）；
    /// 4. **编码显式声明、缺了就拒绝** ✗（`blobs.encoding` ✓，本版本只认 `zlib` ✓）——
    ///    **不给"没有清单就按明文读"的兜底** ✗：本地 CAS（`FsBlobStore` ＋ `RenderCodec` ✓）
    ///    也是按编解码器读的 ✓ ⇒ 一个兜底会把"导出与读回不对称"这种缺陷**藏起来** ✓
    ///    （"字节解释错了"会伪装成"导入成功" ✗）。
    ///
    /// **列表后面必须空一行** ✓（clippy 的 `doc list item without indentation` 又抓了我一次 ✓ ——
    /// 这条 lint 其实一直在帮我保持文档可读 ✓）。
    /// 包内 atoms.jsonl 的有效 Bake 集合（导入策略检查用）：按全日志
    /// compute_suppressed 算有效集，每层取 seq 最大的有效 BakeLayer；原子 JSON
    /// 损坏行按 Value 保守识别为有效（不因解析失败放过一个历史 Blob）。
    /// 返回 `(layer_id, bake_seq, up_to_seq, blob_hash, flatten_mask)`。
    fn effective_bake_blobs_in_package(
        atoms_text: &str,
    ) -> (Vec<yanshi_core::Atom>, Vec<yanshi_core::bake::BakeBlobRef>) {
        let mut atoms: Vec<yanshi_core::Atom> = Vec::new();
        let mut fallback: Vec<yanshi_core::bake::BakeBlobRef> = Vec::new();
        for line in atoms_text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<yanshi_core::Atom>(line) {
                Ok(atom) => atoms.push(atom),
                Err(_) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                        let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                        if kind == "bake_layer" {
                            let payload = value.get("payload");
                            let layer_id = payload
                                .and_then(|payload| payload.get("layer_id"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned();
                            let blob_hash = payload
                                .and_then(|payload| payload.get("blob_hash"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_owned();
                            let up_to_seq = payload
                                .and_then(|payload| payload.get("up_to_seq"))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            let flatten_mask = payload
                                .and_then(|payload| payload.get("flatten_mask"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let seq = value.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
                            if !layer_id.is_empty() && !blob_hash.is_empty() {
                                fallback.push(yanshi_core::bake::BakeBlobRef {
                                    layer_id,
                                    bake_seq: seq,
                                    up_to_seq,
                                    blob_hash,
                                    flatten_mask,
                                });
                            }
                        }
                    }
                }
            }
        }
        let suppressed = yanshi_core::fold::compute_suppressed(&atoms);
        let mut latest: std::collections::BTreeMap<String, (u64, u64, String, bool)> =
            std::collections::BTreeMap::new();
        for atom in &atoms {
            if atom.kind != yanshi_core::AtomKind::BakeLayer || suppressed.contains(&atom.id) {
                continue;
            }
            let Some(layer_id) = atom.payload.get("layer_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(blob_hash) = atom.payload.get("blob_hash").and_then(|v| v.as_str()) else {
                continue;
            };
            let up_to_seq = atom
                .payload
                .get("up_to_seq")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let flatten_mask = atom
                .payload
                .get("flatten_mask")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let entry = latest
                .entry(layer_id.to_owned())
                .or_insert((0, 0, String::new(), false));
            if atom.seq >= entry.0 {
                *entry = (atom.seq, up_to_seq, blob_hash.to_owned(), flatten_mask);
            }
        }
        let mut out: Vec<yanshi_core::bake::BakeBlobRef> = latest
            .into_iter()
            .map(
                |(layer_id, (bake_seq, up_to_seq, blob_hash, flatten_mask))| {
                    yanshi_core::bake::BakeBlobRef {
                        layer_id,
                        bake_seq,
                        up_to_seq,
                        blob_hash,
                        flatten_mask,
                    }
                },
            )
            .collect();
        out.extend(fallback);
        (atoms, out)
    }

    /// 导入工程包（默认口径：缺有效压平基底 Blob 时直接拒绝导入；
    /// 要重放自愈请用 [`Self::import_project_with_policy`] 并传
    /// `missing_blob_policy="replay_and_heal"`）。
    pub fn import_project(&mut self, bytes: &[u8], requested: Option<&str>) -> Result<Value> {
        self.import_project_with_policy(bytes, requested, "error")
    }

    /// 带缺失 Blob 策略的导入（第二轮第 4 条）：`missing_blob_policy` 缺省
    /// `"error"`；显式 `"replay_and_heal"` 时在打开文档后对缺失的有效 Bake
    /// 基底做一次日志重放自愈（严格 SHA 校验，不一致直接报错，不顶替）。
    pub fn import_project_with_policy(
        &mut self,
        bytes: &[u8],
        requested: Option<&str>,
        missing_blob_policy: &str,
    ) -> Result<Value> {
        if !matches!(missing_blob_policy, "error" | "replay_and_heal") {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("missing_blob_policy 只接受 error（默认）或 replay_and_heal"),
            ));
        }
        let Some(persist) = self.persist.clone() else {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "工程包导入需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有地方放它",
                ),
            ));
        };
        let entries = crate::archive::read_tar(bytes)?;
        let mut atoms_text: Option<String> = None;
        let mut meta_text: Option<String> = None;
        let mut render: Option<(u64, Vec<u8>)> = None;
        let mut blobs: Vec<(String, Vec<u8>)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        // `blobs.encoding` 声明的编码 ✓（缺省 ⇒ 明文 ✓，兼容手写的包/测试夹具 ✓）。
        let mut blob_encoding: Option<String> = None;
        for entry in &entries {
            names.push(entry.path.clone());
            let path = entry.path.as_str();
            if path == "atoms.jsonl" {
                atoms_text = Some(String::from_utf8_lossy(&entry.bytes).into_owned());
            } else if path == "meta.json" {
                meta_text = Some(String::from_utf8_lossy(&entry.bytes).into_owned());
            } else if path == "render.png" {
                // **seq 缺省 0** ✓，随后若包里有 `render.seq` 就覆盖它 ✓
                //（我第一版这里去"探测"了一个根本不存在的文档 ✓ —— 那是**写歪了的胡话** ✗，
                //  编译器没拦住它，是我自己回读时发现的 ✓ ⇒ 回读代码这一步不能省 ✓）。
                void_seq(&mut render, 0, entry.bytes.clone());
            } else if path == "render.seq" {
                let text = String::from_utf8_lossy(&entry.bytes);
                if let Ok(seq) = text.trim().parse::<u64>() {
                    if let Some((_, png)) = render.take() {
                        render = Some((seq, png));
                    } else {
                        render = Some((seq, Vec::new()));
                    }
                }
            } else if path == "blobs.encoding" {
                // **显式声明** ✓（不是"试着解一下" ✗）：值不认识就明确拒绝 ✓。
                blob_encoding = Some(String::from_utf8_lossy(&entry.bytes).trim().to_owned());
            } else if let Some(rest) = path.strip_prefix("blobs/sha256/") {
                // 形如 `<ab>/<cd>/<64 位十六进制>` ✓
                if let Some(hex) = rest.rsplit('/').next() {
                    blobs.push((hex.to_ascii_lowercase(), entry.bytes.clone()));
                }
            }
            // **其余（`BUILD-INFO` / `README.txt`）忽略** ✓ —— 它们是给人看的 ✓，不影响还原 ✓。
        }
        let Some(atoms_text) = atoms_text else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "这不是一个工程包：里面没有 atoms.jsonl ⇒ 实际内容：{}",
                    names.join(" / ")
                )),
            ));
        };
        // **目标 id** ✓：调用方给的优先 ✓，否则用 meta.json 里记的 ✓。
        let meta: Option<DocumentMeta> = match &meta_text {
            Some(text) => Some(serde_json::from_str(text).map_err(|error| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("包里的 meta.json 解析不了：{error}")),
                )
            })?),
            None => None,
        };
        let doc_id = requested
            .map(str::to_owned)
            .or_else(|| meta.as_ref().map(|m| m.doc_id.clone()))
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(
                        "既没给 doc_id，包里也没有 meta.json ⇒ 无法确定要导入成哪个文档",
                    ),
                )
            })?;
        // **只导入、绝不覆盖** ✗（不可逆的事不做 ✓）。
        // 判据与 `new_document` 的"已存在 ⇒ 打开"**共用** [`Workspace::document_exists`] ✓
        //（内存 ✓ 或磁盘 ✓ 都算存在 ✓）—— 两处各写一份必然漂移 ✗。
        if self.document_exists(&doc_id) {
            return Err(YanshiError::new(
                ErrorCode::Conflict,
                ErrorContext::detail(format!(
                    "文档 {doc_id} 已经存在 ⇒ **不会覆盖** ✗ ⇒ 请换一个 doc_id（或先把它删掉 / 移走 ✓）"
                )),
            ));
        }
        // **先把 blob 落进内容寻址存储并逐个核对** ✓（不符就整体拒绝 ✓，不留半成品 ✓）。
        //
        // **顺序很重要** ✗：**先按 `blobs.encoding` 解码** ✓、**再对"解出来的明文"核哈希** ✓
        // —— 哈希的定义就是**对明文**算的 ✓（`blob_codec.rs` 的注释 ✓：
        // "哈希仍对明文算 ⇒ 内容寻址/去重/导入对账都不受影响" ✓）⇒ 反过来就会把好包判成坏包 ✗。
        //
        // **只有一种编码，而且是显式声明的** ✗（用户裁定：不做兼容 ✓）：
        // * 有 blob 却**没有** `blobs.encoding` ⇒ **拒绝** ✓ —— 这正是"导出写明文、
        //   而读端只认编解码器"那个**不对称缺陷**的入口 ✓；给一个**默认按明文读**的兜底 ✗
        //   只会把这一类 bug **藏起来** ✓（"看起来导入了、其实字节解释错了" ✗）；
        // * 声明了别的编码 ⇒ **拒绝** ✓（不认识就是不认识 ✓）。
        // **没有 blob 的包**（纯矢量文档 ✓）不要求清单 ✓ —— 没有东西要用它解释 ✓。
        if !blobs.is_empty() {
            match blob_encoding.as_deref() {
                Some(PACKAGE_BLOB_ENCODING) => {}
                Some(other) => {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "包里的 blobs.encoding 是「{other}」⇒ 这个版本只认「{PACKAGE_BLOB_ENCODING}」\
                             ⇒ 拒绝导入（硬猜会把坏字节悄悄导入 ✗）"
                        )),
                    ));
                }
                None => {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "包里有 {} 个 blob，却没有 blobs.encoding ⇒ 拒绝导入 ✗：\
                             不猜编码（默认按明文读会把「字节解释错了」变成「看起来导入成功」✗）。\
                             本版本导出的包一律带 `blobs.encoding: {PACKAGE_BLOB_ENCODING}` ✓",
                            blobs.len()
                        )),
                    ));
                }
            }
        }
        let mut restored = 0usize;
        for (hex, blob_bytes) in &blobs {
            let plain: Vec<u8> = if blob_encoding.as_deref() == Some(PACKAGE_BLOB_ENCODING) {
                yanshi_render::png::zlib_decompress(blob_bytes).ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "工程包里的 blob {hex} 声明是 zlib，但解不开（流坏了或被截断）⇒ 拒绝导入"
                        )),
                    )
                })?
            } else {
                blob_bytes.clone()
            };
            let hash = self.store.put(&plain)?;
            if hash.hex().to_ascii_lowercase() != *hex {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "工程包里的 blob 坏了：路径写着 {hex}，但内容的哈希是 {} ⇒ 拒绝导入",
                        hash.hex()
                    )),
                ));
            }
            restored += 1;
        }
        // **bake 缺失 Blob 策略**（第二轮第 4 条）：在写 atoms.jsonl 与
        // open_document **之前**检查包内有效 BakeLayer 集合；此时 CAS 已 put
        // 了包内 blob（无害），但原子日志尚未落盘、文档尚未打开 ⇒ 拒绝导入
        // 不留半成品文档。`replay_and_heal` 在此先行重放验哈希，失败同样在
        // 落盘前报错；成功后才继续落盘与打开。
        let (baked_blobs_healed, replayed_ms, replayed_atom_count) = {
            let store = self.store();
            let (all_atoms, effective_bakes) = Self::effective_bake_blobs_in_package(&atoms_text);
            let mut missing: Vec<yanshi_core::bake::BakeBlobRef> = Vec::new();
            for bake in &effective_bakes {
                let hash: yanshi_core::BlobHash = match bake.blob_hash.parse() {
                    Ok(hash) => hash,
                    Err(_) => continue,
                };
                if !store.exists(&hash) {
                    missing.push(bake.clone());
                }
            }
            if !missing.is_empty() && missing_blob_policy == "error" {
                let list = missing
                    .iter()
                    .map(|bake| {
                        format!(
                            "图层 {} bake_seq {} blob {}",
                            bake.layer_id, bake.bake_seq, bake.blob_hash
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("；");
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "工程包缺有效压平基底 Blob ⇒ 拒绝导入（missing_blob_policy=error）：{list} ⇒ 用 missing_blob_policy=replay_and_heal 从原子日志重放自愈，或经 put_blob 上传 Blob 后重试"
                    )),
                ));
            }
            let mut healed: Vec<serde_json::Value> = Vec::new();
            let mut replayed_atom_count = 0usize;
            let started = std::time::Instant::now();
            if missing_blob_policy == "replay_and_heal" {
                for bake in &missing {
                    // 重放子集含更早 Bake（只排除目标 Bake 本身）：压平后变换
                    // 瞄准基底对象，排除全部 Bake 会使变换失 target、哈希恒不一致
                    //（与 tools_bake::replay_baked_blob 同一修法，见该处注释）。
                    let subset: Vec<yanshi_core::Atom> = all_atoms
                        .iter()
                        .filter(|atom| {
                            atom.seq <= bake.up_to_seq
                                && !(atom.kind == yanshi_core::AtomKind::BakeLayer
                                    && atom.seq == bake.bake_seq)
                        })
                        .cloned()
                        .collect();
                    replayed_atom_count += subset.len();
                    let mut temp = crate::document::Document::open(
                        std::sync::Arc::clone(&store),
                        format!("__import_heal_{}", bake.layer_id),
                        subset,
                        crate::document::DocumentSettings::default(),
                    )?;
                    let (w, h, rgba) =
                        temp.render_layer_bake_base(&bake.layer_id, bake.flatten_mask)?;
                    let png = yanshi_render::png::encode_png(w, h, &rgba).ok_or_else(|| {
                        YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("导入自愈 PNG 编码失败 ⇒ 拒绝导入"),
                        )
                    })?;
                    let actual_hash = yanshi_core::BlobHash::from_bytes(&png);
                    if actual_hash.to_string() != bake.blob_hash {
                        return Err(YanshiError::new(
                            ErrorCode::PreconditionFailed,
                            ErrorContext::detail(format!(
                                "导入自愈哈希不一致（strict）：图层 {} bake_seq {} 期望 {}，重建 {} ⇒ 拒绝写入与导入",
                                bake.layer_id, bake.bake_seq, bake.blob_hash, actual_hash
                            )),
                        ));
                    }
                    store.put(&png)?;
                    healed.push(serde_json::json!({
                        "layer_id": bake.layer_id,
                        "bake_seq": bake.bake_seq,
                        "blob_hash": bake.blob_hash,
                    }));
                }
            }
            (healed, started.elapsed().as_millis(), replayed_atom_count)
        };
        // **落盘** ✓：原子日志 + 元数据（+ 渲染缓存 ✓）。
        let dir = persist.doc_dir(&doc_id);
        std::fs::create_dir_all(&dir).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("建不了文档目录 {}：{error}", dir.display())),
            )
        })?;
        std::fs::write(dir.join("atoms.jsonl"), atoms_text.as_bytes()).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("写不进原子日志：{error}")),
            )
        })?;
        if let Some(meta) = &meta {
            // **元数据要写成"这次导入的那个 id"** ✓（第 728 轮；外部审计 S-04）
            //   **⇒ 缺陷**：`meta` 来自**包里的** `meta.json` ✓ ⇒ 它的 `doc_id` 是**原文档**的 ✗
            //     ⇒ `save_meta` 按 `meta.doc_id` 建目录 ⇒ **新文档的元数据写进原文档的目录** ✓
            //     ⇒ 两个文档的 `meta.json` 互相覆盖 ✓（跨文档污染，且**没有任何信号** ✓）。
            //   **⇒ 判据**：`import_project(doc_id=doc_copy)` 之后，`docs/doc_copy/meta.json`
            //     必须存在且里面是 `doc_copy` ✓（`import_project::importing_under_a_new_id_…` ✓）。
            let mut meta = meta.clone();
            meta.doc_id = doc_id.clone();
            persist.save_meta(&meta)?;
        }
        if let Some((seq, png)) = &render {
            if !png.is_empty() {
                // **`Seq` 是 `u64` 的别名** ✗（不是新类型 ✓）⇒ 直接传值 ✓。
                let _ = persist.save_render(&doc_id, *seq, png);
            }
        }
        // **载进工作区** ✓ —— 到这一步文档才算真的可用 ✓（能画、能导、能被 MCP 看到 ✓）。
        self.open_document(&doc_id)?;
        // **包省掉的位图由 `open_document` 内部补回来** ✓（导入与"解压到 `--root`"两条路共用 ✓）。
        let replayed = self
            .document(&doc_id)
            .map(|document| document.replayed_blobs())
            .unwrap_or(0);
        // **★ "补不回来"的条数必须**写进响应 ✗ ★**（第 942 轮 ✓）：
        //   **∴ 为什么 ✗**：**静默丢弃**是**本仓头号病根 ✗** ⇒
        //     **∴ 导入者**必须**一眼看到"**有几笔没补回来 ✓" ✓**** ✓✓
        let unreplayable = self
            .document(&doc_id)
            .map(|document| document.unreplayable_blobs())
            .unwrap_or(0);
        let atom_count = atoms_text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count();
        // **★ 把「实际恢复了什么」放进返回值** ✗（外部 §84 ✓）★
        //   ⇒ **∴ 他们实测**✗**：`import_project` 返回 `ok=true` ✗ 而 `list_layers` 返回 `[]`**✗**
        //   ⇒ **∴ 于是**每一笔都报「图层 paint 不存在」✗ 而**笔数 0**✗**
        //   ⇒ **∴ 所以**：**∴ `ok=true`**✗ **只表示**调用成功**✗** ⇒ **∴ 不表示**图层可用**✗** ✓✓
        //   ⇒ **∴ 而**这与 §64（`read_canvas_effective`）／§70（`target_layer_supported`）**同一个模式**✗** ✓
        let restored_layers: Vec<String> = self
            .document(&doc_id)
            .map(|document| {
                document
                    .state()
                    .layers
                    .keys()
                    .map(|id| id.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        Ok(json!({
            "doc_id": doc_id,
            "atoms": atom_count,
            "blobs": restored,
            "blobs_replayed": replayed,
            "blobs_unreplayable": unreplayable,
            "missing_blob_policy": missing_blob_policy,
            "baked_blobs_healed": baked_blobs_healed,
            "replayed_ms": replayed_ms,
            "replayed_atom_count": replayed_atom_count,
            "entries": entries.len(),
            "had_render": render.is_some(),
            "layers": restored_layers,
            "layers_count": restored_layers.len(),
            "note": "`layers` 是**实际恢复**的图层名（不是包里有几个）⇒ 若它为空或少于预期，说明结构没恢复全 ✗",
            "hint": "已导入并打开 ⇒ 可用 get_document / render_region 核对；原文档未被触碰 ✓",
        }))
    }

    /// **把省掉的"可重放位图"按日志重跑回来** ✓（`export_project` 的另一半 ✓）。
    ///
    /// **两条路都要它** ✓：① `import_project` 走工具导入 ✓；② **把包解开到 `--root/`**
    /// （README 的还原方式 ✓）**完全不经过工具层** ✗ ⇒ 只有 `open_document` 这一处能兜住 ✓。
    ///
    /// **判据与导出侧逐条对称** ✓（两边都调 [`crate::tools::brush_source_is_replayable`] ✓）：
    /// 只补"配方可重放 **且** 重跑出来的哈希与日志里记的**逐字节相同**"的那些 ✓；
    /// 有一条对不上就**不补** ✗ —— 宁可让这一次渲染报缺 blob ✓（可见 ✓），
    /// 也绝不把**另一张图**塞进那个哈希 ✓（那是**静默的错误画面** ✗）。
    ///
    /// **正常文档零成本** ✓：先 `store.exists` 逐个问 ✓（几百次文件存在性检查 ✓）；
    /// 只有真的缺才落笔 ✓ ⇒ 平时打开**不做一次重放** ✓。补回来的写进本地 CAS ✓
    /// ⇒ 第二次打开就是普通读 ✓。
    ///
    /// **判据** ✓：`crates/yanshi-server/tests/export_small.rs` 的
    /// `a_package_restores_through_root_and_renders_identically`（变异：去掉这一步 ⇒ 红 ✓）。
    /// **返回值**：`(补回来的条数, 补不回来的条数)` ✓（第 942 轮 ✓）。
    fn materialize_pending_replayable_blobs(&mut self, doc_id: &str) -> Result<(usize, usize)> {
        let store = self.store();
        // **先收齐再落笔** ✗：下面要 `&mut self` 去重放 ✓，不能在遍历日志的同时持有它的借用 ✓。
        let (pending, mut unreplayable): (Vec<(yanshi_core::BlobHash, serde_json::Value)>, usize) = {
            let Some(document) = self.document(doc_id) else {
                return Ok((0, 0));
            };
            let mut pending = Vec::new();
            // **无配方缺失也必须计数**（第 4 轮第 3 期口径簇 · 水彩 P2 反证边界）：
            // 照片 / 媒体原子根本没有 `source`，读画布笔刷的配方也不可重放 ——
            // 从前在这里直接 `continue` ⇒ 包里缺了它们的本体，导入回执仍报
            // `blobs_unreplayable = 0`，丢失是静默的。第 942 轮「补不回来必须计数」
            // 只堵了有配方那条路；无配方本体缺失同样是真实丢失，必须进
            // `unreplayable`，不能跳过。计数按引用原子数（与 `replayed` 同口径，
            // 不是去重后的哈希数 —— `verify_package` 那边才是去重计数）。
            let mut unreplayable = 0usize;
            for atom in document.log().iter() {
                let payload = &atom.payload;
                let Some(hash_text) = payload
                    .get("bitmap")
                    .and_then(|bitmap| bitmap.get("blob_hash"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                let Ok(hash) = hash_text.parse::<yanshi_core::BlobHash>() else {
                    continue;
                };
                // **已经在包里的（或者上一轮补过的）跳过** ✓ —— 绝不覆盖已有字节 ✓。
                if store.exists(&hash) {
                    continue;
                }
                match payload.get("source") {
                    Some(source) if crate::tools::brush_source_is_replayable(self, source) => {
                        pending.push((hash, source.clone()));
                    }
                    // 没有重跑路径 ⇒ 直接记补不回来，不静默跳过。
                    _ => unreplayable += 1,
                }
            }
            (pending, unreplayable)
        };
        let mut replayed = 0usize;
        // **★ "补不回来"必须**计数 ✗ ★**（第 942 轮 ✓；**修静默丢弃 ✓）：
        //   **∴ 原来 ✗**：**哈希不符 ⇒ **既不写、也不报、也不计 ✗**
        //     ⇒ **∴ 导入者**只看到"**少了一笔**"✗，**而**不知道**为什么 ✓**** ✓✓
        //   **∴ 而**导出侧**对同一件事**有 `kept_mismatch` 计数 ✗**
        //     ⇒ **∴ 同一条断言**应当**在两处都留下痕迹 ✓**（**第 941 轮的教训 ✓）** ✓✓
        //   **∴ 现在**：**它**计入返回值 ✗** ⇒ **∴ 由 `import_project`** 写进响应 ✓**
        //     ⇒ **∴ 打开一个包**立刻知道**有几笔没补回来 ✓**** ✓✓
        //   **∴ 且**"**不写**"这个行为**不变 ✗**（**对的 ✓：**绝不把**另一张图
        //     塞进那个哈希 ✓）—— **∴ 变的只是"**说出来**" ✓**** ✓✓
        // （`unreplayable` 已在上面的收集段声明：有配方重放失败与无配方缺失
        // 都进同一个计数，见收集段注释。）
        for (hash, source) in pending {
            match crate::tools::replay_brush_bitmap(self, doc_id, &source) {
                Ok(bytes) if yanshi_core::BlobHash::from_bytes(&bytes) == hash => {
                    store.put(&bytes)?;
                    replayed += 1;
                }
                // **∴ 两种情况都要数 ✗**：**重放失败**（`Err` ✓）**与**哈希不符 ✓**
                //   ⇒ **∴ 它们**对调用方的含义**相同**：**这一笔**没补回来 ✓**** ✓✓
                _ => unreplayable += 1,
            }
        }
        Ok((replayed, unreplayable))
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
        // **P-18：按 `asset_layout` 的扩展名过滤** ✓ —— 原来枚举目录时不看扩展名 ✓ ⇒
        // `assets/brushes` 里的 `CASE-NOTES.md` 与 `LICENSE-CC0.txt` 也被当成笔刷列出来 ✓
        // （下游 `read_list_brushes` 只能给它们挂一条"读不到或解析失败" ✓）。
        // **过滤点在这里** ✓（一处改动，三类资产同时受益 ✓）：`asset_file_usable` 只管
        // "这个文件能不能用"（`usable: false` 照样列出来 ✓），而"该不该列出来"由扩展名定 ✓。
        let (sub, extensions) = asset_layout(kind)?;
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
                let extension = name
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase())
                    .unwrap_or_default();
                if !extensions.contains(&extension.as_str()) {
                    continue;
                }
                let usable = asset_file_usable(kind, &path.join(&name), &extension);
                found.push(TextureEntry {
                    name,
                    bytes,
                    source,
                    usable,
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

    /// **把资产名解析成实际文件路径** ✓（**缓存优先、内置其次** ✓ —— 与列举时的优先级一致 ✓）。
    ///
    /// **为什么要它** ✓：用户导入的笔刷放在缓存 ✓、随包发布的内置笔刷在 `assets/` ✓
    /// ⇒ 调用方只给一个**名字** ✓，由这里决定用哪一个 ✓ ⇒ **覆盖语义在"读"和"列"两处一致** ✓
    ///（各判一次必然漂移 ✗）。找不到时报错并**列出可用的名字** ✓（调用方靠错误文本自我纠正 ✓）。
    pub fn resolve_asset(&self, kind: &str, name: &str) -> Result<std::path::PathBuf> {
        let (sub, _) = asset_layout(kind)?;
        let clean = asset_file_name(name)?;
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        if let Ok(dir) = self.asset_dir(kind) {
            candidates.push(dir.join(&clean));
        }
        if let Some(root) = self.assets_dir.as_ref() {
            candidates.push(root.join(sub).join(&clean));
        }
        for candidate in &candidates {
            if candidate.is_file() {
                return Ok(candidate.clone());
            }
        }
        // **把可用的名字报出来** ✓（只说"找不到"会让调用方猜 ✓）。
        let available: Vec<String> = self
            .list_assets(kind)?
            .into_iter()
            .filter(|entry| entry.usable)
            .map(|entry| entry.name)
            .collect();
        let shown = if available.is_empty() {
            "（当前一个都没有）".to_string()
        } else {
            let mut head: Vec<String> = available.iter().take(8).cloned().collect();
            if available.len() > 8 {
                head.push(format!("…（共 {} 个）", available.len()));
            }
            head.join(" / ")
        };
        Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("找不到 {kind} 「{clean}」⇒ 可用的有：{shown}")),
        ))
    }

    /// **读一个调色板的颜色** ✓（`.gpl` / `.kpl` / `.json` ✓）—— **MCP 与 Web 共用这一个入口** ✓。
    ///
    /// **支持哪三种** ✓（按**真实文件**决定 ✓，不是按扩展名猜 ✓ —— 内容不对就**明确报错** ✗）：
    /// * `.gpl` / `.kpl` ✓：GIMP 调色板文本 ✓（`R G B [名字]` 一行一色 ✓，`#` 是注释 ✓）；
    /// * `.json` ✓：Open Colors 那种形状 ✓（键 ⇒ 十六进制串 ✓ 或**串数组** ✓）。
    ///
    /// **这里返回全部** ✓ —— 截断与"截断了没有"归**工具层**报 ✓
    ///（内核只回答"这个文件里有什么" ✓；职责分开 ⇒ 不会出现"截了却没说" ✗）。
    /// 另外**列表后面必须空一行** ✓ —— 我第一版没空 ✓ ⇒ clippy 的
    /// "doc list item without indentation" 当场指出 ✓（这条 lint 其实是在帮我保持文档可读 ✓）。
    pub fn palette_colors(&self, name: &str) -> Result<Vec<PaletteColor>> {
        let path = self.resolve_asset("palette", name)?;
        let text = std::fs::read_to_string(&path).map_err(|error| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读不到调色板 {}：{error}", path.display())),
            )
        })?;
        let extension = path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let colors = match extension.as_str() {
            "gpl" | "kpl" => parse_gimp_palette(&text),
            "json" => parse_open_color_json(&text)?,
            other => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "还不支持解析 .{other} ⇒ 目前支持 gpl / kpl / json"
                    )),
                ))
            }
        };
        if colors.is_empty() {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "{} 里没解析出任何颜色 ⇒ 多半不是这两种格式",
                    path.display()
                )),
            ));
        }
        // **不在内核里截断** ✓：截断与"截断了没有"都归**工具层**报 ✓
        // ⇒ 内核只负责"这个文件里有什么" ✓（职责分开 ⇒ 不会出现"截了却没说" ✗）。
        Ok(colors)
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
    /// * `blobs.encoding` ✓：`blobs/` 的编码 ✓（`zlib` ✓，与本地 CAS 同一个编解码器 ✓，逐字节可逆 ✓）；
    /// * `render.seq` ✓ / `BUILD-INFO` ✓ / `README.txt` ✓（告诉人怎么还原 ✓）。
    ///
    /// **`include_bitmaps`** ✓：`true` ⇒ **每个被引用的位图都装** ✓（老行为 ✓，排查用 ✓）；
    /// `false`（缺省 ✓）⇒ **只省掉"能证明重放得出来"的那些** ✗ —— 判据是
    /// **真的重跑一遍**（`crate::tools::replay_brush_bitmap` ✓）且**哈希逐字节相同** ✓，
    /// 外加**引用它的每条原子都带配方** ✓、**笔刷不读画布** ✓。
    /// 一条对不上就**照装** ✓ —— **包的体积可以谈，打开的画面不能错** ✗。
    ///
    /// **为什么不能省读画布的笔刷** ✗（实测 ✓）：`oil-03-paint.myb` 的 `smudge = 0.9` ✓
    /// ⇒ 烘出来的像素取决于**落笔时底下是什么** ✓ ⇒ `source` 不是重放配方 ✗
    /// （同参数、空画布 vs 绿底 ⇒ 6.2% 字节不同 ✓；真实 4K 工程 369/376 条 `source` 正是这种 ✓）
    /// ⇒ 省了它 ⇒ **打开就是错的画** ✓（静默错误 ✓，比包大糟糕得多 ✗）。
    /// **19 条介质笔触（142.1 MiB）连 `source` 都没有** ✗ ⇒ 永远得装 ✓。
    ///
    /// **为什么要求落盘工作区** ✓：原子日志与元数据是**在磁盘上**的权威副本 ✓；
    /// 纯内存工作区没有它们 ✓ ⇒ 那里应当**明确拒绝** ✓（而不是导出一个**不完整**的包 ✗）。
    /// **随发行包一起发布的内置纹理目录** ✓（用户裁定：纹理**要入库、要打包** ✓）。
    ///
    /// **与缓存的关系** ✓：内置的是"**开箱就有**"的那几款 ✓（`assets/textures/` ✓，随 `make release` 进包 ✓）；
    /// 缓存（`<root>/textures/` ✓）是用户用 `scripts/fetch-textures.sh` 另外抓的 ✓。
    /// **重名时缓存优先** ✓ —— 用户放进去的同名文件**覆盖**内置的 ✓（想换就换 ✓，不必改仓库 ✓）。
    pub fn export_project(
        &mut self,
        doc_id: &str,
        include_bitmaps: bool,
        blob_bundle_mode: &str,
    ) -> Result<(Vec<u8>, ExportStats)> {
        if !matches!(blob_bundle_mode, "frontier" | "all_history") {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(
                    "blob_bundle_mode 只接受 frontier（默认：完整原子日志 + 当前折叠必需的活跃 Blob）或 all_history（连同被新 Bake 或 Undo 遮蔽的历史 Blob 全量打包）",
                ),
            ));
        }
        let Some(persist) = self.persist.clone() else {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "工程包导出需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有权威的原子日志文件",
                ),
            ));
        };
        let dir = persist.doc_dir(doc_id);
        let _e3_t0 = std::time::Instant::now();
        let _e3 = |tag: &str| {
            if std::env::var_os("YANSHI_TRACE_EXPORT4").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/yanshi-export4-trace.log")
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "EXPORT4 {tag} {} ms", _e3_t0.elapsed().as_millis())
                    });
            }
        };
        _e3("start");
        let atoms_text = std::fs::read_to_string(dir.join("atoms.jsonl")).map_err(|error| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("读不到原子日志 {doc_id}: {error}")),
            )
        })?;
        // **只收这条日志引用到的 blob** ✓（逐行解析 ✓，坏行跳过但**不静默**：计数后写进包里 ✓）。
        //
        // **同时记下"每个 blob 是被哪些净荷引用的"** ✓ —— 省略位图前必须问"引用它的每一条原子
        // 是不是都带着可重放的配方" ✓（只被一条没有配方的原子引用 ⇒ **必须装** ✗）。
        _e3("read_atoms");
        let mut wanted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        // **派生 tile 索引的引用** ✓（B2 ✓）：净荷 `tiles` 里藏的那个哈希 ✓
        // —— 与 `check_package.py` 的结构口径一致：`payload.tiles` ⇒ 派生 ✓，
        // `bitmap.blob_hash`／`refs.blobs` ⇒ 本体 ✓。tile 索引**只计数、不装包**
        // （由本体位图重建 ✓，见下面的 tile 循环 ✓）。
        let mut tile_wanted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut refs_by_blob: std::collections::BTreeMap<String, Vec<serde_json::Value>> =
            std::collections::BTreeMap::new();
        let mut broken_lines = 0usize;
        for line in atoms_text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(line) {
                Ok(atom) => {
                    // **★ `refs.blobs` **两处都看** ✗ ★**（第 228 轮 ✓；**∴ 导出丢位图的修复 ✓）。
                    //
                    // **∴ 为什么 ✗**：**实测**（**第 226／227 轮 ✓）**✗**：
                    //   **∴ 原来只读**原子顶层**的 `refs`**✗（`atom.get("refs")` ✓）
                    //     ⇒ **∴ 而** `import_image` **写的是**净荷里**的 `refs`** ✓
                    //       ⇒ **∴ 位置**不匹配** ⇒ **∴ `wanted` **空**** ✓
                    //         ⇒ **∴ 导出时**一个 blob**都不装** ✓
                    //           ⇒ **∴ 包**14 KiB、`blobs/` ＝ **0 条** ✓
                    //             ⇒ **★ 那**就是 `tool-export-bitmaps.mjs` 的红灯 ✓ ★**** ✓✓
                    //
                    // **∴ 改法 ✗**：**顶层**没有**✗ ⇒ **∴ 再**看净荷里** ✓
                    //   ⇒ **∴ 于是**：**两种写法**都能被收到** ✓（**向后兼容 ✓）** ✓✓
                    // **∴ 代价**：**多一次 `get`**（**∴ 可忽略 ✓）** ✓✓
                    // **∴ 判据 ✗**：`scripts/tool-export-bitmaps.mjs` ✓
                    // **∴ 变异点**：**把净荷那一路去掉** ⇒ **∴ 判据必红** ✓**** ✓✓
                    if let Some(blobs) = atom
                        .get("refs")
                        .or_else(|| atom.get("payload").and_then(|payload| payload.get("refs")))
                        .and_then(|refs| refs.get("blobs"))
                        .and_then(serde_json::Value::as_array)
                    {
                        let payload = atom
                            .get("payload")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null);
                        for blob in blobs {
                            if let Some(hash) = blob.as_str() {
                                wanted.insert(hash.to_owned());
                                refs_by_blob
                                    .entry(hash.to_owned())
                                    .or_default()
                                    .push(payload.clone());
                            }
                        }
                    }
                    // **派生 tile 索引也收进来** ✓（B2 ✓）：只认净荷 `tiles` 里的
                    // 字符串哈希 ✓ —— 索引 blob 的内容（各分块哈希）不在原子日志里 ✓，
                    // 也不在这里展开 ✓（`check_package.py` 同款：只扫 `atoms.jsonl` ✓）。
                    if let Some(tiles) = atom
                        .get("payload")
                        .and_then(|payload| payload.get("tiles"))
                        .and_then(serde_json::Value::as_str)
                    {
                        tile_wanted.insert(tiles.to_owned());
                    }
                }
                Err(_) => broken_lines += 1,
            }
        }
        _e3("parse_and_collect");
        // **bake frontier 分流**（第二轮第 9 条）：只剔除“历史 Bake Blob”——
        // 日志中有 BakeLayer 引用、但不在有效集内、且未被任何存活对象引用
        // 的那部分；其余历史 blob（旧笔触位图等）不受本参数影响，仍走
        // 下面的可重放判定。有效集按全日志 compute_suppressed 算（与折叠同源）。
        let mut frontier_omitted_historical_blobs = 0usize;
        if blob_bundle_mode == "frontier" {
            let mut all_bake_hashes: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            let mut bake_atoms: Vec<yanshi_core::Atom> = Vec::new();
            for line in atoms_text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(atom) = serde_json::from_str::<yanshi_core::Atom>(line) {
                    if atom.kind == yanshi_core::AtomKind::BakeLayer {
                        if let Some(hash) = atom.payload.get("blob_hash").and_then(|v| v.as_str()) {
                            all_bake_hashes.insert(hash.to_owned());
                        }
                        bake_atoms.push(atom);
                    }
                } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                    // 原子 JSON 损坏时仍按 Value 保守识别（当作有效 Bake 参与检查，
                    // 不因解析失败就悄悄放过一个历史 Blob）。
                    let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                    if kind == "bake_layer" {
                        if let Some(hash) = value
                            .get("payload")
                            .and_then(|payload| payload.get("blob_hash"))
                            .and_then(|v| v.as_str())
                        {
                            all_bake_hashes.insert(hash.to_owned());
                        }
                    }
                }
            }
            let suppressed = yanshi_core::fold::compute_suppressed(&bake_atoms);
            let mut effective_bake_hashes: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            {
                let mut latest: std::collections::BTreeMap<String, (u64, String)> =
                    std::collections::BTreeMap::new();
                for atom in &bake_atoms {
                    if suppressed.contains(&atom.id) {
                        continue;
                    }
                    let Some(layer_id) = atom.payload.get("layer_id").and_then(|v| v.as_str())
                    else {
                        continue;
                    };
                    let Some(hash) = atom.payload.get("blob_hash").and_then(|v| v.as_str()) else {
                        continue;
                    };
                    let entry = latest
                        .entry(layer_id.to_owned())
                        .or_insert((0, String::new()));
                    if atom.seq >= entry.0 {
                        *entry = (atom.seq, hash.to_owned());
                    }
                }
                for (_, (_, hash)) in latest {
                    effective_bake_hashes.insert(hash);
                }
            }
            let mut live_hashes: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            if let Some(document) = self.document(doc_id) {
                for object in document.state().objects.values() {
                    for blob in &object.blobs {
                        live_hashes.insert(blob.to_string());
                    }
                    if let Some(hash) = object.data.get("blob_hash").and_then(|v| v.as_str()) {
                        live_hashes.insert(hash.to_owned());
                    }
                }
            }
            let mut omitted: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
            for hash in &all_bake_hashes {
                if !effective_bake_hashes.contains(hash) && !live_hashes.contains(hash) {
                    omitted.insert(hash.clone());
                }
            }
            for hash in &omitted {
                if wanted.remove(hash) {
                    frontier_omitted_historical_blobs += 1;
                }
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
        // **不带"当场渲染的预览"** ✓（用户实测：它占整包 96%～97%，而**导入后可随时由原子日志重建** ✓；
        // 导入端本来就把 `render.png` 当**可选** ✓（`Option` ✓，缺省空 ✓）⇒ 去掉它**向后兼容** ✓。
        entries.push(crate::archive::TarEntry {
            path: "render.seq".to_owned(),
            bytes: head.to_string().into_bytes(),
        });
        // **包里的 blob 用与本地 CAS 同一个编解码器压一遍** ✓（`crate::blob_codec` ✓
        // ＝ `yanshi_render::png::zlib_compress_best` / `zlib_decompress` ✓）。
        //
        // **为什么这样是安全的** ✓（这是上一轮 PNG 方案被否之后必须说清的一条 ✓）：
        // 这两步是**逐字节可逆**的 ✓（`zlib_decompress(zlib_compress_best(x)) == x` ✓，
        // 由 `crates/yanshi-core/tests/blob_codec.rs` 与 `crates/yanshi-render/src/png.rs` 的
        // 往返判据守着 ✓）⇒ **它只是把同样的字节换个容器** ✓，**不重新解释像素** ✓
        // ⇒ **预乘 alpha / 半透明颜色 / 不透明度全部原样通过** ✓
        // —— 而 PNG 那次红的根因正是"把预乘字节当普通 RGBA 重新编码" ✗（见 `ce17665` ✓）。
        //
        // **为什么不会让打开变慢** ✓：本地 CAS 的**每一次** `store.get()` 本来就要 inflate ✓
        //（`blob_codec.rs` 的 `decode` ✓）⇒ 打开/渲染那条路**一个字节的解码成本都没多** ✓；
        // 多出来的只是**导入时解一次压** ✓（一次性 ✓）。
        //
        // **为什么是 manifest 而不是改路径** ✓：`blobs.encoding` 是显式的 ✓ ——
        // 读包的人**不需要猜**这些字节是明文还是 zlib ✓（猜错就是"看起来导入了、其实是垃圾" ✗）。
        entries.push(crate::archive::TarEntry {
            path: "blobs.encoding".to_owned(),
            bytes: PACKAGE_BLOB_ENCODING.as_bytes().to_vec(),
        });
        // **blob 走存储接口取** ✓（内存/落盘都能用 ✓，也顺带保证"包里每一个字节都真的存在" ✓）。
        let store = self.store();
        let mut blob_count = 0usize;
        let mut plain_bytes = 0usize;
        let mut packed_bytes = 0usize;
        let mut omitted = 0usize;
        // **"为什么这一条必须装"要可观测** ✓（否则判据只能间接地测它 ✓ —— 而"间接"正是
        // 实测里被漏掉的那一类 ✗：我第一版去掉 `brush_reads_the_canvas` 那条判据时，
        // 读画布的那一笔**仍然**因为"重跑哈希对不上"而照装 ✓ ⇒ 判据**照样绿** ✗。
        // ⇒ 把两个理由**分别**记进 `BUILD-INFO` ✓：判据才能钉住**判据本身** ✓）。
        let mut kept_no_recipe = 0usize;
        // **`kept_no_recipe` 的原因分类** ✓（第 230 轮 ✓）：只有计数时 CI 独有红
        // （`kept_no_recipe=3`／本机 `omitted=2`）**读不出方向** ✗ ⇒ 每类原因分别记数 ✓。
        let mut kept_no_recipe_reasons: std::collections::BTreeMap<&'static str, usize> =
            Default::default();
        let mut kept_mismatch = 0usize;
        let mut kept_missing = 0usize;
        let mut missing: Vec<String> = Vec::new();
        // **B2 的两栏计数** ✓：本体缺（`missing_body`）与派生缺（`missing_derived`）分开数 ✓。
        let mut missing_body = 0usize;
        let mut missing_derived = 0usize;
        // **B 期项 7/8**：按设计未装包的派生总数（见 `ExportStats::derived_not_packaged`）。
        let mut derived_not_packaged = 0usize;
        // **缺席本体的裸哈希**（与上面 `missing` 里的人话清单并存）：
        // `verify_package` 靠它区分「存储里就没有」（真缺）与「导出时有意省掉」
        // （`omitted`）—— 只看计数分不出来，必须有名有姓的清单。
        // 写进包里 `BUILD-INFO` 的 `missing_blobs:` 行（期 2 块 3，D2）。
        let mut missing_hashes: Vec<String> = Vec::new();
        _e3("pack_headers");
        // （第 229 轮的临时 `EXPORT_WANTED` 探针已删 ✓ —— 第 230 轮把它**规范化成常驻观测**：
        //  回执与 `BUILD-INFO` 里的 `kept_no_recipe_reasons` 分类计数 ✓，不再需要环境变量 ✗。）
        for hash_text in &wanted {
            let hash: yanshi_core::BlobHash = hash_text.parse().map_err(|_| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("日志里有一个不合法的 blob 引用：{hash_text}")),
                )
            })?;
            // **先判定，再读字节** ✓（第 20 轮 ✓）：这一行原先在**判定之前** ✗
            // ⇒ 即使这张位图随后被**省掉**（`omitted` ✓），也先把**整张位图的字节**
            // 读进内存 ✗（4K 画布上可能是**数百 MB** ✗）。
            // 而**判定与重放都不需要这些字节** ✓：`brush_source_is_replayable` 只看日志 ✓、
            // `replay_brush_bitmap` 也是从**日志里的配方**重跑 ✓ ⇒ **∴ 挪到真要打包时读** ✓。
            // **代价两面** ✗：收益＝省掉"每张被省掉的位图一次全量读" ✓（内存与 I/O ✓）；
            // 代价≈0 ✓ —— **输出字节一个都不变** ✓（读的时机变了，读的内容与用法没变 ✓）。
            // **能不能不装** ✗ ⇒ **真的重跑一遍再说** ✓（不是看名字猜 ✓）。
            //
            // 条件：引用它的**每一条**净荷都带可重放配方 ✓（`brush_source_is_replayable` ✓，
            // 即 `kind == "brush"` 且笔刷**不读画布** ✓），**并且**按配方重跑出来的字节
            // **哈希与记录的一致** ✓。两条都过才省 ✗ —— 只要有一条对不上 ✓
            // 就**照装** ✓（包大一点，但**打开一定是对的** ✓）。
            if !include_bitmaps {
                let all_replayable = refs_by_blob.get(hash_text).is_some_and(|payloads| {
                    !payloads.is_empty()
                        && payloads.iter().all(|payload| {
                            crate::tools::payload_brush_source(payload).is_some_and(|source| {
                                crate::tools::brush_source_is_replayable(self, source)
                            })
                        })
                });
                if all_replayable {
                    // **配方从引用它的第一条净荷取** ✓（`supersede` 原子的配方在
                    // `data` 下 ✓，见 `payload_brush_source` ✓）—— `all_replayable`
                    // 已保证每条净荷都有可重放配方 ⇒ 这里一定找得到 ✓。
                    let source = refs_by_blob[hash_text]
                        .iter()
                        .find_map(crate::tools::payload_brush_source)
                        .cloned()
                        .unwrap_or(Value::Null);
                    // **证明记忆化** ✓（第 39 轮 ✓）：
                    // 重放的输入只有四项 ✓（第 365 轮查明 ✓）：配方 ✓、**那个 `.myb` 的字节** ✓、
                    // 引擎数学（crate 版本 ✓）、目标哈希 ✓ ⇒ **∴ 键就是这四项** ✓，缺一不可 ✗。
                    //
                    // ⚠️ **为什么键里必须有 `.myb` 的内容哈希** ✗：只含**名字**的话 ✓，
                    // 磁盘上的 `.myb` 被替换（用户或构建 ✓）⇒ **旧证明会被错误复用** ✗
                    // ⇒ 包**少装本该装的位图** ✗ ⇒ 打开缺图形 ✗ = **本仓头号病根** ✓。
                    // **∴ 解析不出笔刷就"不缓存"** ✓（保守回退 ✓ ⇒ **绝不用不完整的键** ✗）。
                    //
                    // **代价两面** ✗：收益＝同一进程内重复保存时，720 次重放（实测 **2.62 s** ✗）
                    // 直接命中 ⇒ **几十 ms** ✓；代价＝每个键几十字节的内存 ✓
                    // ＋ **为每支笔刷读一次 `.myb` 并哈希**（~6 KB ✓，一次导出内按名去重 ✓）。
                    // **边界** ✗：进程内缓存 ⇒ **服务端重启后第一次保存仍慢** ✓（磁盘持久化是下一步 ✓）。
                    let proof_key = export_proof_key(self, &source, &hash);
                    if let Some(key) = &proof_key {
                        if export_proof_lookup(key) {
                            // **命中 ⇒ 与"重放成功"完全同一条路** ✓（输出不受影响 ✓）。
                            omitted += 1;
                            continue;
                        }
                    }
                    let reproved = crate::tools::replay_brush_bitmap(self, doc_id, &source);
                    if let Ok(replayed) = reproved {
                        if yanshi_core::BlobHash::from_bytes(&replayed) == hash {
                            if let Some(key) = &proof_key {
                                export_proof_remember(key);
                            }
                            omitted += 1;
                            continue;
                        }
                    }
                    kept_mismatch += 1;
                } else {
                    kept_no_recipe += 1;
                    *kept_no_recipe_reasons
                        .entry(crate::tools::payloads_replay_rejection(
                            self,
                            refs_by_blob.get(hash_text),
                        ))
                        .or_insert(0) += 1;
                }
            }
            // 到这里才真的需要字节 ✓（下面要把它按十六进制路径放进 tar ✓）。
            // **B2：本体缺不再降级** ✓ —— 第 70 轮的"跳过这一条 ＋ 记数 ＋ 告警"
            // 已被推翻 ✓：缺本体时整个导出在循环后直接失败（`ReferenceNotFound` ✓），
            // 这里只记数、记清单 ✓，不产包 ✓（见循环后的检查 ✓）。
            let bytes = match store.get(&hash) {
                Ok(bytes) => bytes,
                Err(error) => {
                    // **原先这里把 `kept_missing` 连加两次** ✗（`00220ddd` 与 `e7a4402d`
                    // 各加了一行 ✓）⇒ 计数会翻倍。第 230 轮查明它是**死写**：本路径随后
                    // `missing_body > 0` ⇒ 整个导出 `Err` ⇒ 统计根本不回执 ✓，所以从未见红 ✗。
                    // 修法＝删重复行 ✓（行为不变 ⇒ 不需要变异；已在台账诚实记录）。
                    kept_missing += 1;
                    missing_body += 1;
                    missing_hashes.push(hash_text.clone());
                    missing.push(format!(
                        "位图 {hash} 在存储里不存在 ⇒ 这一条**没有**装进包: {}",
                        error
                            .context
                            .detail
                            .clone()
                            .unwrap_or_else(|| "（无细节）".to_owned())
                    ));
                    continue;
                }
            };
            let hex = hash.hex();
            let packed = yanshi_render::png::zlib_compress_best(&bytes);
            plain_bytes += bytes.len();
            packed_bytes += packed.len();
            entries.push(crate::archive::TarEntry {
                path: format!("blobs/sha256/{}/{}/{}", &hex[0..2], &hex[2..4], hex),
                bytes: packed,
            });
            blob_count += 1;
        }
        // **派生 tile 只计数、不装包** ✓（B2 ✓）：tile 索引由本体位图重建 ✓
        // —— 渲染时索引取不到就落回整幅路（`render.rs` 的分块路取不到索引 ⇒
        // `None` ⇒ 整幅重算 ✓）⇒ 缺派生不致命 ✓，但数字必须报出来 ✓
        //（与 `check_package.py` 的两栏口径对齐 ✓）。
        // **本体优先** ✓：同一个哈希既是本体引用又是 tile 引用 ⇒ 按本体算 ✓
        //（`check_package.py` 同款聚合：本体胜出 ✓）—— 只认 `exists` ✓，
        // 不读字节 ✓（索引只是小 JSON，但"在不在"不需要内容 ✓）。
        for hash_text in &tile_wanted {
            if wanted.contains(hash_text) {
                continue;
            }
            // **B 期项 7/8**：只要是 tile 引用而非本体引用，导出时就按设计不装包——
            // 无论存储里在不在，都先计入本栏；存储缺失的那部分另由 `missing_derived` 数。
            derived_not_packaged += 1;
            let hash: yanshi_core::BlobHash = hash_text.parse().map_err(|_| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("日志里有一个不合法的 tile 引用：{hash_text}")),
                )
            })?;
            if !store.exists(&hash) {
                missing_derived += 1;
            }
        }
        // **B2：本体缺 ⇒ 整个导出失败** ✓（`ReferenceNotFound` ✓）——
        // tar 字节此刻还没组装（`write_tar` 在后面的 `Ok` 里 ✓）、文件还没写 ✓、
        // CAS 也还没入 ✓ ⇒ 不留半成品包 ✓。只缺派生时照常往下走 ✓。
        if missing_body > 0 {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "工程包缺本体 blob ⇒ 无法导出（missing_body={missing_body}，missing_derived={missing_derived}）⇒ 补齐被引用的本体 blob 再导出；只缺派生 tile 时仍可导出"
                )),
            ));
        }
        // **机读行（期 2 块 3，D2）**：`verify_package` 的 `omitted≠missing`
        // 判定要读它们 —— `include_bitmaps` 决定「省掉」合不合法，
        // `missing_blobs:` 逐行记导出时存储里就没有的本体（老包没有这两行
        // 时，`verify_package` 一律按「真缺」保守处理）。
        let missing_lines = missing_hashes
            .iter()
            .map(|hash| format!("missing_blobs: {hash}\n"))
            .collect::<String>();
        // **原因分类也进包** ✓（第 230 轮 ✓）：拿到包就能看出"照装的是哪一类配方问题" ✓，
        // 不必再靠临时 `eprintln` 探针（下面原来那段已删 ✓）。
        let reasons_line = kept_no_recipe_reasons
            .iter()
            .map(|(reason, n)| format!("{reason}={n}"))
            .collect::<Vec<_>>()
            .join("|");
        entries.push(crate::archive::TarEntry {
            path: "BUILD-INFO".to_owned(),
            // **版本与 commit 一起进包** ✓（用户提过的排查需求 ✓）：拿到包就知道是哪一版产的 ✓。
            bytes: format!(
                "name: yanshi\ndoc_id: {doc_id}\nhead_seq: {head}\nblobs: {blob_count}\n\
                 blobs_omitted_replayable: {omitted}\n\
                 include_bitmaps: {include_bitmaps}\n\
                 blob_bundle_mode: {blob_bundle_mode}\n\
                 frontier_omitted_historical_blobs: {frontier_omitted_historical_blobs}\n\
                 {missing_lines}\
                 blobs_kept_no_replayable_recipe: {kept_no_recipe}\n\
                 blobs_kept_no_replayable_recipe_reasons: {reasons_line}\n\
                 blobs_kept_replay_mismatch: {kept_mismatch}\n\
                 blob_encoding: {PACKAGE_BLOB_ENCODING}\n\
                 blob_bytes_plain: {plain_bytes}\nblob_bytes_packed: {packed_bytes}\n\
                 unparsable_atom_lines: {broken_lines}\n\
                 restore: 把本包解开到 <root>/ 之下即可（atoms.jsonl 是唯一权威，仅 import 重放可恢复省掉的位图，直接读 blob 的消费方仍见缺失）\n"
            )
            .into_bytes(),
        });
        entries.push(crate::archive::TarEntry {
            path: "README.txt".to_owned(),
            bytes: b"Yanshi project package.\n\
Contents:\n  atoms.jsonl      append-only atom log; this is the authority\n\
  meta.json        document metadata\n  render.seq       the seq that render corresponds to\n\
  blobs.encoding   how blobs/ is encoded (zlib = byte-exact deflate of the raw blob)\n\
  blobs/           content-addressed blobs referenced by the log\n\
Not every blob is here: a patch whose brush recipe provably replays byte-exact is omitted\n\
(BUILD-INFO blobs_omitted_replayable). Importing replays those from the log; blobs whose\n\
pixels cannot be proven reproducible (canvas-reading brushes, media strokes) are always kept.\n\
Boundary (default packages are NOT byte-self-contained): only the import path restores\n\
an omitted bitmap. A consumer that unpacks blobs/ directly without import replay still\n\
sees the omitted bitmaps as missing. Judge a package with verify_package (complete=true\n\
means import-replayable); do not judge self-containment from the export receipt's\n\
missing_body, which is a storage-side count (0 on every successful export). Counts in\n\
omitted/missing_body/missing_derived are deduplicated blob hashes, not stroke counts.\n\
Restore under <root>/ of a yanshi-serve instance (the doc lives under docs/<doc_id>/):\n\
  mkdir -p <root>/docs/<doc_id>\n\
  mv atoms.jsonl meta.json <root>/docs/<doc_id>/\n\
  mv blobs <root>/            # merge into the workspace CAS; same encoding it already uses\n\
The first open replays any omitted bitmap into the local CAS; later opens just read it.\n"
                .to_vec(),
        });
        Ok((
            crate::archive::write_tar(&entries),
            ExportStats {
                blob_count,
                blob_bytes_plain: plain_bytes,
                blob_bytes_packed: packed_bytes,
                // **重放验证的三个读数** ✓（第 35 轮 ✓，纯观测 ✓）。
                omitted,
                kept_no_recipe,
                kept_no_recipe_reasons,
                kept_mismatch,
                kept_missing,
                missing,
                // **B2 的两栏** ✓：到这里 `missing_body` 恒为 0 ✓（`> 0` 已在上面
                // 直接失败 ✓）—— 字段仍要填 ✓（成功回执的两栏数字是契约 ✓）。
                missing_body,
                missing_derived,
                derived_not_packaged,
                frontier_omitted_historical_blobs,
            },
        ))
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

    /// **★ 查询某文档的预览状态 ✓ ★**（第 589 轮 ✓）。
    ///
    /// **⚠️ 本步**不改行为**✗**：**它等价于现有逻辑 ✓，只是把"已就绪／不可用"**显式化 ✓**
    /// ⇒ **∴ 下一轮**把"未就绪"那一段换成**入队 ＋ 返回 `Pending`**✗（**设计文档第九节 ✓**）。
    pub fn preview_state(&mut self, doc_id: &str, size: DocThumbSize) -> Result<PreviewState> {
        if self.document_thumbnail_is_current(doc_id) {
            if let Some(url) = self
                .document(doc_id)
                .and_then(|d| d.document_thumbnail_url())
            {
                return Ok(PreviewState::Ready(url));
            }
        }
        if matches!(size, DocThumbSize::Skip) {
            return Ok(PreviewState::Unavailable);
        }
        // **★ ④ 首次：入队 ＋ 返回 `Pending` ✓（**不渲染 ✗**）★**（第 594 轮 ✓）——
        // **∴ 响应立刻返回 ✓**；**而**下一次提交收尾**的 `run_pending_jobs`（**只要 `pending()` 非空
        // 就渲预览 ✓**）会把它渲出来 ✓（**∴ 契约不动 ✓**）。
        let now = yanshi_core::now_ms();
        match self.preview_pending_at.get(doc_id).copied() {
            // **★ ⑤ 兜底 ✓**：**仍在宽限期内 ⇒ 继续等 ✓**（**不许当成"没有图" ✗**）。
            Some(started) if now - started < PREVIEW_PENDING_GRACE_MS => Ok(PreviewState::Pending),
            // **超时 ⇒ 就地同步生成 ✓**（**∴ 只读调用方也能拿到图 ✓；亦满足"慢的仍然要算"✓**）。
            Some(_) => {
                let url = self.ensure_document_thumbnail(doc_id, size)?;
                self.preview_pending_at.remove(doc_id);
                Ok(url
                    .map(PreviewState::Ready)
                    .unwrap_or(PreviewState::Unavailable))
            }
            // **首次 ⇒ 入队 ＋ 记时 ＋ 返回 `Pending` ✓**。
            None => {
                if let Some(doc) = self.documents.get_mut(doc_id) {
                    doc.mark_preview_pending("server:preview".to_owned());
                }
                self.preview_pending_at.insert(doc_id.to_owned(), now);
                Ok(PreviewState::Pending)
            }
        }
    }

    /// **文档缩略图是否已是最新 ✓**（**只读转发 ✓**）：**已是最新 ⇒ 可直接返回已存 URL ✓**；
    /// **∴ 否则**说明需要生成 ✓**（**调用方据此判断"要不要等" ✓**）。
    /// **⚠️ 它**不生成**任何东西 ✗** —— **∴ 想"确保有图"请用 [`Self::ensure_document_thumbnail`] ✓**。
    pub fn document_thumbnail_is_current(&self, doc_id: &str) -> bool {
        self.document(doc_id)
            .map(|d| d.document_thumbnail_is_current())
            .unwrap_or(false)
    }

    /// **★ 标记「预览已推迟过一次」✗ 并返回**之前**是否已推迟**✗ ★**（**第 247 轮 ✓）
    ///
    /// **∴ 语义 ✗**：**`false` ＝ **这一次是首次**（**∴ 调用方**只报 `pending` ✗ 不阻塞 ✓）**｜
    /// **`true` ＝ **之前已经推迟过**（**∴ 调用方**这次**就地材料化** ✓）★
    ///   ⇒ **∴ 于是**：**首次不阻塞 ＋ 之后仍拿得到图**两件事**同时成立** ✓ ★★
    ///
    /// **∴ 文档不在时 ✗**：**按「已经推迟过」返回**✗（**∴ 于是**照旧走就地路 ✓）
    ///   ⇒ **∴ 既有行为**不被这次改动**改变** ✓（**∴ 缺文档 ⇒ **后面的 `ensure` **本来就会报错 ✓）★
    pub fn mark_preview_deferred(&mut self, doc_id: &str) -> bool {
        self.document_mut(doc_id)
            .map(|d| d.mark_preview_deferred())
            .unwrap_or(true)
    }

    /// **确保文档缩略图存在 ✓**（**按需生成并落盘 ✓**；**已是最新 ⇒ 直接返回已存 URL ✓**）。
    /// **⚠️ 返回值**：**`Option<String>` ＝ URL ✓｜`None` ＝ `Skip`（**调用方明确不要图 ✓**）** ——
    /// **∴ 想知道"**本次是否真的生成了**✗"⇒ 请用 [`Self::document_thumbnail_is_current`] ✓**。
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
        if kind == ThumbKind::Doc256 {
            let (preview, head, png) = {
                let document = self.document_mut(doc_id)?;
                let preview = document.render_document_preview()?;
                let png = document.store().get(&preview.blob_hash)?;
                (preview, document.head_seq(), png)
            };
            // 落盘**小图**预览缓存 ⇒ 下一次打开（或新进程）有基座可用 ✓，不再整幅重渲染 ✗。
            if let Some(persist) = &self.persist {
                let _ = persist.save_preview(doc_id, head, &png);
            }
            return Ok(Some(preview.url));
        }
        let preview = self.thumbnail(doc_id, kind, None)?;
        Ok(Some(preview.url))
    }

    /// **把当前文档预览落盘** ✓（若它已与 HEAD 一致 ✓）——「打开即图片」的写入端 ✓。
    ///
    /// **为什么每次提交后都要写** ✗：文档预览是**每个新连接**都要的东西 ✓，
    /// 而它一旦落后一个 seq ✓，旧路径就会**整幅重渲染**（实测 4K / 318 对象 **122s** ✗）✓。
    /// 一份 256² PNG 只有几十 KB ✓ ⇒ 用它换掉"每个连接一次整幅渲染" ✓ 是本专题性价比最高的一步 ✓。
    ///
    /// **只写 256²** ✗：`document_thumbnail` 也可能是**整幅** blob ✓（整幅 `render_region` 会更新它 ✓）
    /// ⇒ 用 PNG 头判尺寸 ✓，不是 256² 就不写 ✓（那份由 `render.png` 负责 ✓）。
    pub fn cache_document_preview(&mut self, doc_id: &str) -> Result<()> {
        let Some(persist) = self.persist.clone() else {
            return Ok(());
        };
        let (head, png) = {
            let document = self.document_mut(doc_id)?;
            if !document.document_thumbnail_is_current() {
                return Ok(());
            }
            let Some(blob) = document.document_thumbnail_blob() else {
                return Ok(());
            };
            let png = document.store().get(blob)?;
            (document.head_seq(), png)
        };
        let Some((width, height)) = crate::document::png_dimensions(&png) else {
            return Ok(());
        };
        if width != height || width != ThumbKind::Doc256.size() {
            return Ok(());
        }
        let _ = persist.save_preview(doc_id, head, &png);
        Ok(())
    }

    /// 渲染状态。
    pub fn render_status(&mut self, doc_id: &str, atom_id: &str) -> Result<RenderStatus> {
        let document = self.document_mut(doc_id)?;
        document.render_status(atom_id)
    }

    /// 发放令牌并持久化（12.7）；返回 `(令牌, 被淘汰的旧令牌数)` ✓（审计 P2-1）。
    ///
    /// **被淘汰数必须回显** ✓：链接被淘汰后用会得到 403（显式失败 ✓），而**签发方**
    /// 要从响应里的 `tokens_evicted` 当场知道「刚刚丢了几枚」✗⇒ 不许静默 ✓。
    pub fn issue_token(
        &mut self,
        doc_id: &str,
        actor: &str,
        role: Role,
    ) -> Result<(CapabilityToken, usize)> {
        // **尺寸在这里一并取出** ✓（`meta.json` 缺失时 `record_token` **不许**凭空写 0×0 ✗，
        // 见 `persist.rs::record_token` 的说明 ✓）：`document` 就在手里 ✓ ⇒ 尺寸是已知的 ✓。
        let (token, evicted, width, height) = {
            let document = self.document_mut(doc_id)?;
            let width = document.state().width;
            let height = document.state().height;
            let (token, evicted) = document.issue_token(actor, role);
            (token, evicted, width, height)
        };
        if let Some(persist) = &self.persist {
            persist.record_token(doc_id, width, height, &token, actor, role)?;
        }
        Ok((token, evicted))
    }

    /// 吊销令牌（审计 H-2）：**内存与 `meta.json` 同步删除** ✓ ⇒ 重启后不会复活 ✓。
    ///
    /// 返回 `(是否真的删掉了, 剩余令牌数)`：前者让调用方区分「吊销成功」与「本来就没有」✗。
    pub fn revoke_token(&mut self, doc_id: &str, token: &CapabilityToken) -> Result<(bool, usize)> {
        let (removed, remaining) = {
            let document = self.document_mut(doc_id)?;
            let removed = document.authority_mut().revoke(token);
            (removed, document.authority_mut().len())
        };
        // **无论内存里有没有都清盘** ✓：磁盘上残留的记录会在下次打开时 `restore` ⇒ 复活 ✗。
        if let Some(persist) = &self.persist {
            persist.forget_token(doc_id, token.as_str())?;
        }
        Ok((removed, remaining))
    }

    /// 鉴权（决定提交时的 actor 与权限）。
    ///
    /// 文档若尚未打开但磁盘存在，会先按需加载（含 token 恢复，12.7）——
    /// HTTP/WS 请求带着 token 打进来时不该因为进程刚重启就失败。
    ///
    /// **★ 05 复审 NEW-C1：未认证的请求不许触发重活 ★**
    /// 旧实现无条件先 `load_atoms`（读盘解析整份日志）＋ `open_document`（全量折叠 ＋
    /// 渲染缓存分配），**然后**才查令牌 ⇒ 拿着无效 token 的匿名请求也能逼服务端做全量 IO，
    /// 与已修的 P0-1 同类（副作用跑在闸前），只是漏在**读**路径上。
    /// 现在远端请求先用 **meta.json 浅探针**查令牌表（令牌本来就在 meta 里，不需要折叠），
    /// 通过后才付打开的代价；本地传输（stdio／进程内，受信）维持旧顺序。
    pub fn authorize(
        &mut self,
        doc_id: &str,
        token: Option<&CapabilityToken>,
        transport: TransportKind,
        fallback_actor: &str,
    ) -> Result<Principal> {
        if !self.documents.contains_key(doc_id) {
            if transport.is_local() {
                let exists = match &self.persist {
                    Some(persist) => !persist.load_atoms(doc_id)?.is_empty(),
                    None => false,
                };
                if exists {
                    self.open_document(doc_id)?;
                }
            } else {
                // 浅探针：只读 meta.json，不读原子、不折叠 ⇒ 无效/缺失令牌在这里就被拒，
                // 后面的全量打开只有已认证的请求才能触发。
                let Some(token) = token else {
                    return Err(YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail("缺少 capability token"),
                    ));
                };
                if !self.token_known_on_disk(doc_id, token)? {
                    return Err(YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail("capability token 无效或已吊销"),
                    ));
                }
                // 令牌确实落过盘 ⇒ 这时才付全量打开的代价。
                // **如实说边界**：浅探针防的是「**无效令牌免费触发重活**」；
                // 持**有效令牌**的请求仍要付一次冷开——那是它本来就该付的。
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

    /// 令牌是否在该文档 `meta.json` 的令牌表里（[`Workspace::authorize`] 的浅探针）。
    ///
    /// `tokens_hashed` 为假时盘上是**旧版明文** ✓ ⇒ 按明文比对；为真时按 SHA-256 摘要比对。
    fn token_known_on_disk(&self, doc_id: &str, token: &CapabilityToken) -> Result<bool> {
        let Some(persist) = &self.persist else {
            return Ok(false);
        };
        let Some(meta) = persist.load_meta(doc_id)? else {
            return Ok(false);
        };
        Ok(meta.tokens.iter().any(|record| {
            if meta.tokens_hashed {
                record.token == crate::token::token_digest(token.as_str())
            } else {
                record.token == token.as_str()
            }
        }))
    }

    /// 文档 capability URL（打开文档时返回内嵌 token 的 URL，12.7）。
    pub fn capability_url(&self, doc_id: &str, token: &CapabilityToken) -> String {
        format!("yanshi://doc/{doc_id}?token={token}")
    }

    /// 文档数量。
    /// 汇总所有已打开文档的渲染缓存统计（`/health` 用）。
    /// **缓存读数** ✓：`(tiles, used_bytes, evictions, misses, hits)` ✓。
    ///
    /// **为什么加上 `hits`** ✗（第 25 轮 ✓）：底层的 `BitmapCacheStats` **本来就有** `hits` ✓
    ///（`render.rs:155` ✓，命中时 `hits += 1` ✓），但这里**没往上传** ✗ ⇒ `/health` 只有 `misses`
    /// ⇒ **∴ 命中率算不出来** ✓ ⇒ **∴ 无法为"缓存是否有效"立判据** ✗
    ///（"查表全变未命中"这种退化会无人察觉 ✓）。
    /// **∴ 这是纯观测补充** ✓：**不参与任何渲染决策** ✗ ⇒ 输出像素一个都不变 ✓。
    /// **位图缓存的命中／未命中（全文档聚合）** ✓（第 93 轮 ✓，纯观测 ✓）。
    /// **below 复用次数（全文档求和）** ✓（第 95 轮 ✓，纯观测 ✓）。
    /// **`by_sig` 分组数（进程级观测；第 254 轮）** —— 用来判断"某个场景有没有把
    /// 分组数顶到 `BELOW_SIG_GROUPS` 上限"，从而判断淘汰策略有没有生效的机会。
    pub fn below_groups_count(&self) -> usize {
        yanshi_render::render::below_groups_count()
    }

    /// **`by_sig` 分组数的峰值（进程级观测；第 254 轮）**。
    pub fn below_groups_peak(&self) -> usize {
        yanshi_render::render::below_groups_peak()
    }

    /// **below 复用的累计次数**（所有文档求和）—— 与 `below_missing_count`/`below_wanted_count`
    /// 配对，判据据此算自洽性。
    ///
    /// **注**（第 254 轮）：这条文档被"新增方法插进它和函数之间"坑掉过一次（本会话**第六次** ✗）
    /// => 规矩见 `AGENTS.md`：新增方法要插在**整段文档注释之前**。
    pub fn below_reuse_count(&self) -> usize {
        self.documents.values().map(|d| d.below_reuse_count()).sum()
    }

    /// **below 缓存累计缺了几格（全文档求和）**（第 342 轮，纯观测）。
    pub fn below_missing_count(&self) -> usize {
        self.documents
            .values()
            .map(|d| d.below_missing_count())
            .sum()
    }

    /// **below 缓存累计想要几格（全文档求和）**（第 342 轮，纯观测）。
    pub fn below_wanted_count(&self) -> usize {
        self.documents
            .values()
            .map(|d| d.below_wanted_count())
            .sum()
    }

    /// **★ 某一个文档的 below 复用计数 ✓ ★**（第 535 轮 ✓）：**只读转发 ✓**。
    ///
    /// **为什么不能只用上面那个** ✗：**`below_reuse_count()` 把**所有文档**加起来 ✗**，
    /// **∴ 那样就算不出"**这次渲染**是否复用 ✗**（**其它文档的计数会混进来 ✓**）。
    /// **∴ 用途** ✓：**调用方紧贴一次渲染读两次 ⇒ **差值 > 0 ⇔ 该次复用 ✓****。
    /// **★ 某一个文档**最近一次渲染的 below tile 账目 ✗ ★**（第 2 轮 ✓）：
    /// `(想要几格, 缓存里已有几格, 实际用上几格)` ✓；**文档不存在时归零 ✓**。
    ///
    /// **∴ 为什么要它 ✗**：**"**命中 ✓／"未命中 ✓"**说不清**"**差几格 ✓"**✗
    ///   ⇒ **∴ 而**部分复用要修的**正是那个差 ✓ ⇒ **∴ 判据**需要这三个数 ✓**** ✓✓
    pub fn document_below_tiles(&self, doc_id: &str) -> (usize, usize, usize, usize) {
        self.document(doc_id)
            .map(|d| d.below_tiles())
            .unwrap_or((0, 0, 0, 0))
    }

    /// **某一个文档的**下方合成复用计数** ✗**（**语义计数 ✓，见 `below_tiles` 的三元账目 ✓）**。
    pub fn document_below_reuse_count(&self, doc_id: &str) -> usize {
        self.document(doc_id)
            .map(|d| d.below_reuse_count())
            .unwrap_or(0)
    }

    /// **★ 某一个文档的**整幅缓存命中计数** ✗ ★**（第 804 轮 ✓；**目标第 4 条 ✓**）：
    ///   **∴ 为什么需要它 ✗**：**整幅请求**命中 `full_frame_render` 时**根本不进渲染器 ✗**
    ///   ⇒ **∴ 于是** **tile 级 `below_reused`／`above_reused`** 在那条路上**永远为假 ✓**
    ///   ⇒ **∴ 只有这个计数**能证明"**用户实际走的快路径**"命中了 ✓**（**∴ 判据要用它 ✓）**。
    pub fn document_full_frame_hits(&self, doc_id: &str) -> u64 {
        self.document(doc_id)
            .map(|d| d.full_frame_hits())
            .unwrap_or(0)
    }

    /// **★ 某一个文档的 `above` 复用计数 ✓ ★**（第 704 轮 ✓）：**只读转发 ✓**。
    ///
    /// **∴ 为什么需要它 ✗**：**全文档求和**算不出"**这次渲染**是否复用 ✗**
    /// ⇒ **∴ 调用方**紧贴一次渲染读两次 ⇒ **差值 > 0 ⇔ 该次复用了上方合成 ✓****。
    pub fn document_above_reuse_count(&self, doc_id: &str) -> usize {
        self.document(doc_id)
            .map(|d| d.above_reuse_count())
            .unwrap_or(0)
    }

    /// **（第 95 轮补）** 本项在插入 below 计数时被"抢走"了注释 ✗ ⇒ 按其作用补回 ✓。
    /// ⚠️ 规律（本会话第 7 次 ✗）：**在某一项之前插入 ⇒ 会挪走它的文档注释** ✓（`-D missing-docs` 每次都能抓住 ✓）。
    pub fn bitmap_cache_hits_misses(&self) -> (usize, usize) {
        let mut hits = 0usize;
        let mut misses = 0usize;
        for document in self.documents.values() {
            let (h, m) = document.bitmap_cache_hits_misses();
            hits += h;
            misses += m;
        }
        (hits, misses)
    }

    /// **★ 区域字节缓存的命中／未命中（**全文档求和 ✓）✗ ★**（**第 247 轮 ✓）
    ///
    /// **∴ 为什么需要它 ✗**（**∴ 判据的形式问题换来的 ✓）**：
    ///   **∴ `tool-region-cache-reuse.mjs` 第 2 段原来拿**绝对耗时**判"有没有复用"**✗
    ///     ＋ **∴ 而**绝对值随机器与构建档位变化**✗（**∴ 实测三档 ✗ 本机 debug 93 ms ⇒ 红 ✓
    ///       CI 31 ms ⇒ 红 ✗ L4 release 10 ms ⇒ 判"不适用"⇒ 绿 ✓）★★
    ///       ＋ **∴ 而**那 93／31 ms 里的**大头是**哈希 1 MB 裸像素**✗ 与复用**无关** ✓
    ///         ⇒ **∴ 所以**：**要一个**与机器无关**的读数 ✗ ⇒ **∴ 它**就是这个 ✓ ★★
    ///   ＋ **∴ 而**它**纯观测 ✗ 不参与任何渲染决策** ✓（**同 `bitmap_cache_hits_misses` ✓）★
    pub fn region_cache_hits_misses(&self) -> (u64, u64, u64, u64) {
        let mut hits = 0u64;
        let mut misses = 0u64;
        let mut stale = 0u64;
        let mut evicted = 0u64;
        for document in self.documents.values() {
            let stats = document.region_cache_stats();
            hits += stats.hits;
            misses += stats.misses;
            stale += stats.stale;
            evicted += stats.evicted;
        }
        (hits, misses, stale, evicted)
    }

    /// **位图缓存未命中字节之和（全文档）** ✓（第 133 轮 ✓，纯观测 ✓）。
    pub fn bitmap_cache_missed_bytes(&self) -> u64 {
        self.documents
            .values()
            .map(|d| d.bitmap_cache_missed_bytes())
            .sum()
    }

    /// **tile 缓存的统计（全文档聚合）** ✓：`(tiles, used_bytes, evictions, misses, hits)` ✓。
    /// （修复记录：插入 `bitmap_cache_hits_misses` 时把本函数的注释"抢"走了 ✗ ⇒ 已补 ✓。）
    pub fn cache_stats(&self) -> (usize, usize, u64, u64, u64) {
        let mut tiles = 0usize;
        let mut used_bytes = 0usize;
        let mut evictions = 0u64;
        let mut misses = 0u64;
        let mut hits = 0u64;
        for document in self.documents.values() {
            let stats = document.cache_stats();
            tiles += stats.tiles;
            used_bytes += stats.used_bytes;
            evictions += stats.evictions;
            misses += stats.misses;
            hits += stats.hits;
        }
        (tiles, used_bytes, evictions, misses, hits)
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

    /// **落盘这份文档日志里"还没在盘上"的全部原子，再存快照** ✓。
    ///
    /// **★ 为什么按水位刷、不按"提交回了哪条 seq"刷 ✗**（第 1440 轮审计 ✓）：
    /// 上一版只在提交**成功**时写 `result.seq` 那一条 ✗ ⇒
    /// 而**冲突拒绝**这条路径在返回 Err **之前**已经把冲突图层原子追加进了日志 ✓
    /// （`Document::ensure_conflict_layer` → `append_system_atom` ✓）⇒
    /// 那条原子**只活在内存** ✗ ⇒ 重开文档后冲突图层凭空消失 ✓（数据丢失一类 ✓）。
    /// 改成"日志里多了什么就落什么" ✓ ⇒ **任何追加路径都走同一条咽喉** ✓（内核 ①：一处实现 ✓）。
    fn journal(&mut self, doc_id: &str) -> Result<()> {
        self.flush_journal(doc_id)?;
        let Some(persist) = &self.persist else {
            return Ok(());
        };
        // **★ 普通提交路径也要把"最新可用快照"落盘（待办 ① ✓，2026-10-15 ✓）★**
        //   **为什么这一处也要** ✗：建文档处只覆盖"建文档"那一刻 ✓；
        //     普通提交走的是**这里** ✓ ⇒ 少挂一处 ⇒ 快照永远只覆盖开头 ✗ ⇒ 尾巴依旧很长 ✗。
        //   **失败只记 stderr** ✗✓：快照只是**加速**用的 ✓ ⇒ **绝不让提交失败** ✓。
        if let Some(snapshot) = self
            .documents
            .get(doc_id)
            .and_then(|document| document.latest_usable_snapshot())
        {
            if let Err(error) = persist.save_snapshot(doc_id, snapshot) {
                eprintln!("快照落盘失败（不影响提交 ✓）：{error}");
            }
        }
        Ok(())
    }

    /// **把 `journal_watermark + 1 ..= head_seq` 的原子逐条落盘** ✓（`journal` 的落笔部分 ✓）。
    ///
    /// **逐条推进水位** ✓：每写完一条立刻记一条 ⇒ 中途失败（磁盘满 ✓）后
    /// **下一次从断点续写** ✓ —— 不重复写 ✗、不漏写 ✓（失败本身由 `?` 抛出 ✓，不静默 ✓）。
    /// **没有持久层** ⇒ 只把水位推到 HEAD ✓（没东西可写 ✓，语义保持一致 ✓）。
    /// **提交前把 `next_seq` 与盘上对齐** ✓（第 752 轮；台账三百三十）
    ///
    /// **⇒ 修的是哪件事**：另一个进程（自己的计数器 ✓）可能已经把日志写到更后面 ✓
    /// ⇒ 本进程照自己的 `next_seq` 分配 ⇒ **撞号** ✓ ⇒ 盘上重复 `seq` ✓。
    /// **⇒ 修法**：分配**之前**把 `next_seq` 至少推到"盘上最后一条 + 1" ✓
    /// ⇒ **两个写者都能继续写** ✓、**号不撞** ✓（比第 749 轮"拒绝第二个写者"强 ✓）。
    /// **⇒ 代价（如实说）**：另一个写者的原子**不会**进入本进程的内存日志 ✓
    /// ⇒ 要看到它们仍需重新打开文档 ✓（真正的多写者一致性还得靠文件锁 ✓）。
    pub fn sync_next_seq_from_disk(&mut self, doc_id: &str) -> Result<()> {
        let Some(persist) = &self.persist else {
            return Ok(());
        };
        // **先比长度，再决定要不要读盘尾** ✓（第 762 轮，性能）
        let len = persist.journal_len(doc_id)?;
        if self.disk_len_seen.get(doc_id).copied() == Some(len) {
            return Ok(());
        }
        let Some(last) = persist.last_seq_on_disk(doc_id)? else {
            self.disk_len_seen.insert(doc_id.to_owned(), len);
            return Ok(());
        };
        self.disk_len_seen.insert(doc_id.to_owned(), len);
        let head = self
            .documents
            .get(doc_id)
            .map_or(0, |document| document.log().head_seq());
        if last > head {
            // **把它们读进来，而不只是把号推上去** ✓（第 754 轮）：
            //   否则本进程的 `get_log` **永远看不到**别的写者那一笔（要重开才行 ✗）。
            let extra: Vec<yanshi_core::Atom> = persist
                .load_atoms(doc_id)?
                .into_iter()
                .filter(|atom| atom.seq > head)
                .collect();
            let adopted_head = {
                let document = self.documents.get_mut(doc_id);
                match document {
                    Some(document) => {
                        let before = document.log().head_seq();
                        document.adopt_atoms(extra);
                        document.log().head_seq().max(before)
                    }
                    None => head,
                }
            };
            // **水位也要越过它们** ✓：否则 `flush_journal` 会把刚并进来的原子**再写一遍** ✗
            //   （第 754 轮实测：不推水位 ⇒ 盘上出现"我自己重写了一遍 seq 3" ⇒ 打开时判重复 ✓）。
            if let Some(document) = self.documents.get_mut(doc_id) {
                if adopted_head > document.journal_watermark() {
                    document.set_journal_watermark(adopted_head);
                }
            }
        }
        if let Some(document) = self.documents.get_mut(doc_id) {
            document.bump_next_seq_at_least(last);
        }
        Ok(())
    }

    pub(crate) fn flush_journal(&mut self, doc_id: &str) -> Result<()> {
        let not_open = || {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        };
        let head = self
            .documents
            .get(doc_id)
            .map_or_else(|| Err(not_open()), |document| Ok(document.log().head_seq()))?;
        let watermark = self.documents.get(doc_id).map_or_else(
            || Err(not_open()),
            |document| Ok(document.journal_watermark()),
        )?;
        let Some(persist) = &self.persist else {
            if let Some(document) = self.documents.get_mut(doc_id) {
                document.set_journal_watermark(head);
            }
            return Ok(());
        };
        for seq in watermark + 1..=head {
            match self
                .documents
                .get(doc_id)
                .and_then(|document| document.log().by_seq(seq))
            {
                Some(atom) => persist.append_atom(doc_id, atom)?,
                // **空洞不静默** ✓：日志的 seq 应当连续 ✓ ⇒ 真跳号就是异常，
                // 大声说出来 ✓（水位照常推进 ✗ 否则会永远卡在这条 ✗）。
                None => {
                    eprintln!("⚠ 日志落盘：文档 {doc_id} 的日志里没有 seq {seq} ⇒ 跳过这一条")
                }
            }
            let document = self.documents.get_mut(doc_id).ok_or_else(not_open)?;
            document.set_journal_watermark(seq);
        }
        Ok(())
    }
}

/// **把 `render.png` 与 `render.seq` 拼到一起** ✓（tar 里两者顺序不保证 ✓ ⇒ 两种顺序都要能对 ✓）。
fn void_seq(slot: &mut Option<(u64, Vec<u8>)>, seq: u64, png: Vec<u8>) {
    match slot.take() {
        Some((existing_seq, existing_png)) => {
            let use_seq = if existing_seq != 0 { existing_seq } else { seq };
            let use_png = if existing_png.is_empty() {
                png
            } else {
                existing_png
            };
            *slot = Some((use_seq, use_png));
        }
        None => *slot = Some((seq, png)),
    }
}

/// **读工作区的偏好文件** ✓（不存在 / 坏了 ⇒ **空表** ✓，并且**不报错** ✓）。
///
/// **为什么不报错** ✓：偏好**不是文档** ✗ —— 它坏了只该"回到默认" ✓，
/// 不该让整个工作区打不开 ✗（那是把"方便"当成"必需" ✓，代价太大 ✓）。
fn read_preferences(persist: &FileStore) -> BTreeMap<String, Value> {
    let path = persist.root().join("preferences.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

/// **一个目录下所有文件的字节数** ✓（只用于删除回执里那句"释放了多少" ✓）。
///
/// **读不到就算 0** ✓：这只是**观测**，不是删除的前提 ✗ ——
/// 因为一个统计数字读不出来就拒绝删除 ✓，那才是本末倒置 ✗。
fn directory_bytes(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            total = total.saturating_add(directory_bytes(&entry.path()));
        } else if let Ok(metadata) = entry.metadata() {
            total = total.saturating_add(metadata.len());
        }
    }
    total
}

/// **落盘但没载入的 Stash 引用到的 blob** ✓（`<root>/stash/*.json` ✓）。
///
/// **为什么要单独读一遍** ✗：`with_file_store` **不载入**已落盘的 Stash ✓
///（那是另一个缺口 ✗，不在本轮范围 ✗）⇒ 只看 `self.stashes` 会把"离线期间的编辑"
/// 引用的 blob 当成无人引用 ✓ ⇒ 删除时顺手删掉 ✗。
/// **读不出来的文件就跳过** ✓：这是**保守**方向 ✓（少删几个 ✓，绝不误删 ✗）。
fn persisted_stash_roots(persist: &FileStore) -> std::collections::BTreeSet<yanshi_core::BlobHash> {
    let mut roots = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(persist.root().join("stash")) else {
        return roots;
    };
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(stash) = serde_json::from_str::<Stash>(&text) else {
            continue;
        };
        for atom in &stash.atoms {
            roots.extend(atom.all_blob_refs());
        }
        for text in &stash.blob_refs {
            if let Ok(hash) = text.parse() {
                roots.insert(hash);
            }
        }
    }
    roots
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
                .unwrap()
                .0;
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
    fn revoked_tokens_stop_authorizing_and_do_not_resurrect_after_restart() {
        let root = temp_root("revoke");
        let settings = DocumentSettings::default();
        let token;
        {
            let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
            workspace
                .create_document(NewDocument::new("doc_r", 16, 16), "human:1", "s")
                .unwrap();
            token = workspace
                .issue_token("doc_r", "human:2", Role::Editor)
                .unwrap()
                .0;
            let (removed, remaining) = workspace.revoke_token("doc_r", &token).unwrap();
            assert!(removed, "第一次吊销必须真的删掉");
            assert_eq!(remaining, 0);
            assert!(
                workspace
                    .authorize("doc_r", Some(&token), TransportKind::Http, "x")
                    .is_err(),
                "吊销后同一进程内必须立刻失效"
            );
            let (removed_again, _) = workspace.revoke_token("doc_r", &token).unwrap();
            assert!(!removed_again, "重复吊销如实报 false，不假装成功");
        }
        // 重启：`meta.json` 里已无该令牌 ⇒ 不许复活。
        let mut workspace = Workspace::with_file_store(&root, settings).unwrap();
        assert!(
            workspace
                .authorize("doc_r", Some(&token), TransportKind::Http, "x")
                .is_err(),
            "吊销必须跨重启生效（否则泄露的令牌永久有效 ✗）"
        );
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

    #[test]
    fn export_proof_cache_is_bounded() {
        // 键含笔刷内容哈希 ⇒ 不同笔刷能无限撑大它 ✓；上限满了整体清空（只损失时间）。
        for n in 0..MAX_EXPORT_PROOF_ENTRIES + 10 {
            export_proof_remember(&(
                "crate".to_owned(),
                format!("recipe-{n}"),
                "brush".to_owned(),
                "target".to_owned(),
            ));
        }
        let len = export_proof_cache().lock().unwrap().len();
        assert!(
            len <= MAX_EXPORT_PROOF_ENTRIES,
            "证明缓存必须有界：实际 {len} 条，上限 {MAX_EXPORT_PROOF_ENTRIES}"
        );
    }
}
