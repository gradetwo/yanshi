//! 文档服务（设计文档 3 章服务端、6.2、12.1）。
//!
//! 一个 [`Document`] 持有：权威原子日志、当前 HEAD 折叠状态、Blob CAS 句柄、
//! 渲染器（L3 tile 缓存）、快照、Job、广播器与标注通道。
//!
//! 提交路径严格按设计文档执行：
//!
//! 1. **提交时校验**（12.2）：blob 先行的引用存在性、引用实体存活、revert 权限；
//! 2. **分配权威 seq** 并追加日志（5.1，客户端 ULID 幂等）；
//! 3. **增量折叠**（6.6）：只重放新增区间，`declare_head` 触发完整求值；
//! 4. **双 dirty 传播** → 失效 tile（6.6）；
//! 5. **控制流广播**全部原子元数据（6.8）；
//! 6. 重型原子创建 **Job**（6.7），渲染异步执行。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use yanshi_core::blob::{plan_gc, run_gc, BlobStore, GcPlan, GcReport};
use yanshi_core::{
    fold::{FoldResult, WarningKind},
    Atom, AtomId, AtomKind, AtomLog, Bbox, BlobHash, ChangesetId, CommitContext, DocumentState,
    ErrorCode, ErrorContext, FoldEngine, HeadBase, IncrementalFolder, Result, Seq, Snapshot,
    SnapshotBase, SnapshotDecision, SnapshotStore, StateAt, YanshiError,
};
use yanshi_render::dirty::{plan_dirty_with_log, DirtyKind, DirtySet};
use yanshi_render::png::encode_png;
use yanshi_render::render::{RegionRender, RenderStats, Renderer};
use yanshi_render::thumb::{render_thumbnail, Thumb, ThumbKind};
use yanshi_render::tile::{TileGrid, TileKey};
use yanshi_render::Buffer;

use crate::annotations::AnnotationStore;
use crate::broadcast::{Broadcaster, PushChannel, SubscriberId};
use crate::job::{JobId, JobManager, JobStatus};
use crate::timings::CommitPhases;
use crate::token::{CapabilityToken, Principal, Role, TransportKind};

/// 新建文档的规格。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewDocument {
    /// 文档 id。
    pub doc_id: String,
    /// 画布宽。
    pub width: u32,
    /// 画布高。
    pub height: u32,
    /// 背景色（`{"r": 0-255, ...}`；`null` 表示透明）。
    pub background: Value,
}

impl NewDocument {
    /// 以 id 与尺寸构造（白色背景）。
    pub fn new(doc_id: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            doc_id: doc_id.into(),
            width,
            height,
            background: json!({"r": 255, "g": 255, "b": 255, "a": 255}),
        }
    }

    /// 指定背景。
    pub fn with_background(mut self, background: Value) -> Self {
        self.background = background;
        self
    }
}

/// 文档级设置。
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentSettings {
    /// tile 边长（必须是 32/64/128/256/512）。
    pub tile_size: u32,
    /// 渲染器字节预算（L3 tile 缓存）。
    pub render_cache_bytes: usize,
    /// 重型原子的渲染等待预算（毫秒）——`wait_for_render` 的超时（6.7）。
    pub wait_for_render_ms: u64,
    /// 快照落后阈值（原子数，默认 1000）。
    pub snapshot_lag_atoms: Seq,
    /// 孤儿 blob TTL（秒，默认 7 天）。
    pub orphan_ttl_seconds: i64,
}

impl Default for DocumentSettings {
    fn default() -> Self {
        Self {
            tile_size: 256,
            render_cache_bytes: 64 * 1024 * 1024,
            wait_for_render_ms: 500,
            snapshot_lag_atoms: yanshi_core::snapshot::SNAPSHOT_LAG_ATOMS,
            orphan_ttl_seconds: yanshi_core::blob::DEFAULT_ORPHAN_TTL_SECONDS,
        }
    }
}

/// 提交结果（工具层据此构造 10.1 响应）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitResult {
    /// 原子 id。
    pub atom_id: AtomId,
    /// 权威序号。
    pub seq: Seq,
    /// 是否幂等命中（重复提交）。
    pub duplicate: bool,
    /// 所属变更集。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset_id: Option<ChangesetId>,
    /// 提交后的 head seq。
    pub head_seq: Seq,
    /// dirty 类型与原因。
    pub dirty_kind: &'static str,
    /// 受影响区域 `[x, y, w, h]`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_bbox: Option<[f64; 4]>,
    /// 失效的 tile。
    pub dirty_tiles: Vec<TileKey>,
    /// 折叠警告（级联失效等），形如 `{"kind": ..., "detail": ...}`。
    pub warnings: Vec<Value>,
    /// 重型原子对应的 job。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<JobId>,
    /// 是否触发了快照。
    pub snapshotted: bool,
}

/// 渲染预览结果（7.5：默认返回 URL，不返回 base64）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderedPreview {
    /// 文档区域 `[x, y, w, h]`。
    pub bbox: [f64; 4],
    /// 宽。
    pub width: u32,
    /// 高。
    pub height: u32,
    /// PNG 在 CAS 中的哈希。
    pub blob_hash: BlobHash,
    /// 取回地址（HTTP 层会替换为真实 URL）。
    pub url: String,
    /// MIME。
    pub mime_type: String,
    /// 字节数。
    pub bytes: usize,
    /// 涉及的 tile 数。
    pub tiles: usize,
    /// 缩略图类型（缩略图预览时填写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_kind: Option<ThumbKind>,
    /// 本次为滤镜扩展的 padding。
    pub filter_padding: u32,
    /// 未实现类型/格式的告警。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// **文档预览自"已知基座"以来累积的脏区** ✓（冷启动复用专题）。
///
/// 为什么要三态、而不是原来那个 `Option<Bbox>` ✗：`None` 原来**同时**表示两件相反的事 ✓——
/// ① "冷启动，不知道哪里变了" ⇒ 必须整幅 ✗；② "刚刚整幅渲染过，没有残留脏区" ✓。
/// 于是**刚渲染完紧接着又被要求渲染**时 ✓，`None` 会让它**再整幅渲染一遍** ✗
///（复用持久化像素时这是致命的 ✓：一次打开就退化成整幅 ✗）。
/// 三态把这两件事分开 ✓：只有 [`PreviewDirty::Unknown`] 才允许退回整幅 ✓。
#[derive(Debug, Clone, Copy, PartialEq)]
enum PreviewDirty {
    /// 冷启动/求值起点跳变 ⇒ 不知道哪里变过 ⇒ 只能整幅渲染。
    Unknown,
    /// 确定没有变化 ⇒ 不需要渲染。
    Clean,
    /// 只有这一块变过 ⇒ 只渲染这一块。
    Region(Bbox),
}

impl PreviewDirty {
    /// **累积**（提交路径与冷启动基座共用同一套语义 ✓）。
    ///
    /// `Unknown` 会**吸收**一切 ✓（保守方向）——这正是"不确定就整幅"的落点 ✓。
    fn merged(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Clean, other) => other,
            (other, Self::Clean) => other,
            (Self::Region(a), Self::Region(b)) => Self::Region(union_bounds(a, b)),
        }
    }
}

/// 把 dirty 规划结果折算成预览脏区（与提交路径此前的口径**逐字一致** ✓）。
/// 预览落后超过这个步数时，[`Document::dirty_since`] 不再逐原子扫描（第 191 轮）。
///
/// 判据：`dirty_since_returns_unknown_when_preview_is_far_behind` ——
/// 删掉用它的那个 `if` ⇒ 该测试红。
pub(crate) const DIRTY_SCAN_LIMIT: u64 = 64;

/// **只在测试里存在的扫描步数计数器**（第 192 轮）。
///
/// **为什么需要它**：判据要能区分"走上限早退"与"走满扫描" ✓。
/// 只看返回值不行 —— 样本在没有上限时也会返回 `Unknown` ✓（第 191 轮实测 ✓）。
#[cfg(test)]
pub(crate) static DIRTY_SCAN_STEPS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// 清零扫描步数（测试用）。
#[cfg(test)]
pub(crate) fn dirty_scan_steps_reset() {
    DIRTY_SCAN_STEPS.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// 读扫描步数（测试用）。
#[cfg(test)]
pub(crate) fn dirty_scan_steps() -> usize {
    DIRTY_SCAN_STEPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// **该原子**确定不会改变任何像素**吗** ✓（第 223 轮 ✓）—— **保守白名单** ✓。
///
/// **为什么要它** ✗：`planned_preview_dirty` 只见 `DirtyKind::Structure` ＋ 无 bbox ✓
/// ⇒ **一律判 `Unknown` ⇒ 整幅重渲** ✗ ⇒ **实测"8K 新建空层"的 `preview_ms` ＝ 11093.968 ms** ✗✓
///（**而建空层只改元数据 ✓，像素一个都没动 ✓**）。
///
/// **为什么用白名单而非新 `DirtyKind` 细分** ✓：细分要**每个原子**显式声明"是否影响像素" ✗
/// ⇒ **漏标即撒谎** ✗；**白名单只列**确定安全**者 ✓ ⇒ **默认仍重渲** ✓
/// ⇒ **∴ 收益是"省掉确定可省的" ✓，而**正确性不依赖它**** ✓✓。
fn atom_cannot_change_pixels(kind: yanshi_core::atom::AtomKind) -> bool {
    use yanshi_core::atom::AtomKind;
    matches!(
        kind,
        AtomKind::CreateLayer | AtomKind::CreateCheckpoint | AtomKind::Comment | AtomKind::Suggest
    )
}

fn planned_preview_dirty(dirty: &DirtySet) -> PreviewDirty {
    if dirty.kind == DirtyKind::Full || (dirty.kind == DirtyKind::Structure && dirty.bbox.is_none())
    {
        return PreviewDirty::Unknown;
    }
    match dirty.bbox {
        Some(bbox) if bbox.w > 0.0 && bbox.h > 0.0 => PreviewDirty::Region(bbox),
        _ => PreviewDirty::Clean,
    }
}

/// 渲染状态（6.7 `get_render_status`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RenderStatus {
    /// 该原子是否已渲染（其 seq 不超过渲染水位）。
    pub rendered: bool,
    /// 当前 head seq。
    pub head_seq: Seq,
    /// 渲染水位。
    pub rendered_seq: Seq,
    /// 对应的原子 seq。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_seq: Option<Seq>,
    /// 可用的缩略图地址。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
}

/// 单个文档的服务端状态。
pub struct Document {
    id: String,
    settings: DocumentSettings,
    authority: crate::token::TokenAuthority,
    log: AtomLog,
    store: Arc<dyn BlobStore>,
    state: DocumentState,
    folder: IncrementalFolder,
    snapshots: SnapshotStore,
    renderer: Renderer,
    jobs: JobManager,
    broadcaster: Broadcaster,
    annotations: AnnotationStore,
    render_watermark: Seq,
    /// 最近一次任意渲染结果（可能只是 dirty 区域，10.1 `preview` 用它）。
    last_render_blob: Option<BlobHash>,
    /// **★ 整幅缓存命中次数 ✗ ★**（第 796 轮 ✓；**目标第 4 条 ✓**）：
    ///   **∴ 为什么要它 ✗**：**`full_frame_render` 命中时**根本不进渲染器 ✗**
    ///   ⇒ **∴ 于是** **tile 级 `below_reused`／`above_reused`** 在那条路上**永远为假 ✓**
    ///   ⇒ **∴ 这个计数**才是**用户实际走的快路径**的指标 ✓**（**∴ 判据**要用它 ✓）**。
    full_frame_hits: u64,
    /// 文档级缩略图（覆盖整幅画布），6.2/7.3「打开即图片」用它。
    document_thumbnail: Option<BlobHash>,
    /// 上述缩略图对应的 seq（用于判定是否已落后于 HEAD）。
    document_thumbnail_seq: Seq,
    /// **缓存文档级缩略图的像素** ✓（性能专题，第 1040 轮）。
    ///
    /// 增量预览必须拿得到"上一版像素" ✓：只重新渲染变过的区域，再把它刷进对应块 ✓；
    /// 若每次都从 `Thumb::new` 开始 ✗，未刷新的块就会是空白 ✗。256² RGBA ＝ 256KB/文档 ✓。
    document_thumb: Option<Thumb>,
    /// **自"已知基座"以来累积的文档脏区** ✓（`Unknown` ＝ 需要整幅重算 ✓）。
    preview_dirty: PreviewDirty,
    /// **持久化的整幅渲染** ✓（磁盘 `render.png` 且尺寸 == 画布 ✓）—— 它可以在
    /// `seq == HEAD` 时**直接充当"整幅区域渲染"的结果** ✓，不必重算 ✗（冷启动复用专题）。
    ///
    /// **必须是整幅** ✗：`render.png` 也可能是 256² 缩略图 ✓ ⇒ 那种只能做增量预览的像素基座 ✓，
    /// 绝不能拿去回答"给我整幅 3840×2160" ✗（那会把 256² 的图当成整幅返回 ✓ —— 分辨率都不对 ✗）。
    full_frame_render: Option<BlobHash>,
    /// 上述整幅渲染对应的 seq（**与 HEAD 比对**，落后就不许当缓存用 ✗）。
    full_frame_render_seq: Seq,
    /// **真正整幅渲染画布的次数** ✓（判据用语义计数 ✓，不看墙钟 ✗）。
    ///
    /// 复用持久化像素的全部价值就是让它**保持 0** ✓ ⇒ 一旦有人把某条路径改回整幅 ✗，
    /// 判据立刻变红 ✓，不需要在慢机器上等 140 秒 ✗。
    full_canvas_renders: usize,
    /// **钉住的 blob** ✓（第 301 轮 ✓，A① 的收尾 ✓）：任何自动回收路径都**不许删**这些 ✓。
    ///
    /// **为什么需要** ✗（用**调用栈**实测定位 ✓）：
    /// 参考图是**用 `render_region` 产出**的 ✓ ⇒ 它天然落在"预览缓存"的管理范围里 ✓
    /// ⇒ `evict_replaced_previews` 在**下一次渲染**时会把"被替换掉的旧预览"删掉 ✗
    /// ⇒ 而那个旧预览**已经被用户设成参考图** ✓ ⇒ **∴ 参考图从磁盘消失** ✓✓
    /// （调用栈：`write_fill_region` → `finish_mutation` → `render_region` →
    ///   `evict_replaced_previews` → `FsBlobStore::remove` ✓）。
    /// ⇒ 症状正是"设了参考图 ✓、过一会儿就没了 ✗" —— **数据丢失**一类 ✓。
    /// **谁填**：服务端在偏好变化时同步 ✓（参考图是**工作区级**的 ✓ ⇒ 每个文档都要记 ✓）。
    pinned_blobs: std::collections::BTreeSet<yanshi_core::BlobHash>,
    /// **文档级预览渲染**（`render_document_preview` 真正渲染了像素）的次数 ✓（判据用语义计数 ✓）。
    ///
    /// **为什么单列一个计数** ✗：`full_canvas_renders` 只在"整幅"时 +1 ✓ ⇒ 暖文档上
    /// `render_document_preview` 只重渲染**缩略图块**（远小于整幅 ✓）⇒ 它**数不到** ✗。
    /// 而外部 4K/8K 报告指认的固定开销正是"提交收尾同步跑一次文档级预览" ✓ ——
    /// 判据需要能看见"这一笔到底有没有跑那一次" ✓，且**与画布面积、与墙钟都无关** ✓
    ///（暖文档上的那一次是块渲染 ✓，冷文档上的那一次是整幅 ✓，两者都该被数到 ✓）。
    document_preview_renders: usize,
    /// **缩略图**渲染次数 ✓（第 66 轮 ✓，**纯观测** ✓）。
    ///
    /// **为什么要它** ✗：保存点**无条件**调 `document.thumbnail(Doc256)` ✓，
    /// 而那次调用会**自己整幅渲染一遍** ✗ ⇒ 实测 **+1241 ms**（4K ✓）／8K 更大 ✓
    /// ⇒ **∴ 它违反本目标第 3 条**："**缩略图按需生成**" ✓。
    /// **∴ 要靠它做判据** ✓：**重复保存时该计数不得增长** ✓（**变异**：去掉跳过去掉 ⇒ 计数增长 ⇒ 判据红 ✓）。
    document_thumbnail_renders: usize,
    /// **本次打开花了多少毫秒** ✓（第 71 轮 ✓）—— **回应真实用户报的"可观测性缺口"** ✗：
    /// 15MB 文档冷加载实测 **2.2 s** ✓，而 `timings.total_ms` 只有 **1.7 ms** ✗
    /// ⇒ **∴ 因为加载发生在**工具开始之前**✗** ⇒ **∴ 物理上装不进 `total_ms`** ✓
    /// ⇒ **∴ 它必须是一个独立读数** ✓（"**上一次打开花了多少**" ✓）。
    open_ms: u64,
    /// **最近一份缩略图缓存槽** ✓（不管哪一级 ✓）—— 每次写缩略图都要淘汰它替换掉的那份 ✓。
    ///
    /// **为什么需要它** ✓：真实工作区实测 **2161 个 blob 里 1912 个是孤儿、共 1.07 GB（约 95%）** ✗，
    /// 主源正是**每次提交都生成一份缩略图、写了新的却没删旧的** ✓ ——
    /// 我第一版只给"文档级缩略图"做了淘汰 ✗ ⇒ 别的级别照旧泄漏 ✓（测试用"渲染多轮后数一数"当场抓住 ✓）。
    last_thumb_blob: Option<BlobHash>,
    /// **区域字节缓存** ✓（设计 §8.4 的 RegionBlock ✓）—— 命中时直接给已量化的 u8 ✓。
    ///
    /// 实测动机 ✓：512² 区域命中时，仅"f16 线性 → u8 显示空间"的重量化就要 **18.9ms／72ns 每像素** ✗，
    /// 而**零层纯管线**已占 30.5ms ✗（约七成 ✓，内容每层只加 1.3ms ✓）。
    /// 设计 §6.1 说"内存 tile 用 f16 线性 ✓，**持久缓存与网络传输用 u8（显示空间）**" ✓
    /// ⇒ 命中就该给字节 ✓，不必重量化 ✓。
    region_cache: yanshi_render::region_block::RegionBlockCache,
    /// **最近一次渲染里"被跳过的东西"** ✓（工程包体积专题 ✓）。
    ///
    /// **为什么必须记** ✗：渲染器现在**不因为缺一个 blob 就整幅失败** ✓（`render.rs` 的
    /// `Primitive::RasterPatch` ✓：跳过 + 记 `RenderStats::unsupported` ✓）——
    /// 但**裸像素那条出口**（`render_region_raw` ✓，`patch` / `export_png` 都走它 ✓）
    /// 的返回类型是个三元组 ✓，**没有地方带告警** ✗ ⇒ 跳过就会变成**静默的不完整画面** ✗
    /// —— 那正是不能接受的那一种 ✓。⇒ 把告警**挂在文档上** ✓，
    /// 裸像素出口的调用方渲染完读一次即可 ✓（[`Document::last_render_warnings`] ✓）。
    last_render_warnings: Vec<String>,
    /// **★ 最近一次渲染的 below tile 账目 ✗ ★**（第 2 轮 ✓；**纯观测 ✓）：
    /// `(想要几格, 缓存里已有几格, 实际用上几格)` ✓。
    ///
    /// **∴ 为什么要它 ✗**：**"**命中 ✓／"未命中 ✓"**说不清**"**差几格 ✓"**✗
    ///   ⇒ **∴ 而**"**部分复用**"要修的**正是那个差 ✓ ⇒ **∴ 先把差**量出来 ✓**** ✓✓
    /// **∴ 不撒谎 ✗**：**每次**渲染都**覆盖**它 ✗**（**不保留旧值 ✓）**
    ///   ⇒ **∴ 于是**读它的人**不会**看到**上一次的数 ✓**** ✓✓
    /// **★ 最近一次 below 账目的**原子快照** ✗ ★**（第 24 轮 ✓）：
    /// `(想要, 已有, 用上, 缺)` —— **∴ 四个数**必须**来自**同一次渲染**✗
    ///   ⇒ **∴ 于是**：**读的人**不会**看到"**三数来自不同渲染 ✓"的混合 ✓**** ✓✓
    /// **∴ 为什么把 `missing` 也放进来 ✗**：**实测**（**第 15／16 轮 ✓）出现
    ///   `available=10 < wanted=16` **而** `missing=0`**✗ ⇒ **∴ 追查发现**
    ///   **`missing`**根本**没被暴露**✗ ⇒ **∴ 脚本**读 `?? 0` ⇒ **∴ 恒 0 ✓
    ///     ⇒ **★ 所以**：**"**矛盾 ✓"**是**我自己造的假象 ✓ ★**** ✓✓
    last_below_tiles: (usize, usize, usize, usize),
    /// **打开这一份文档时重放回来的位图数** ✓（`export_project` 省掉的那些 ✓）。
    ///
    /// **为什么记在文档上** ✓：补的动作发生在 `Workspace::open_document` ✓，
    /// 而 `import_project` 要把"补了几条"写进响应 ✓ ⇒ 记在这里，两边不必各算一遍 ✓。
    replayed_blobs: usize,
    /// **打开这一份文档时"**补不回来**"的位图数 ✗**（第 942 轮 ✓；**修静默丢弃 ✓）：
    ///   **∴ 为什么必须记 ✗**：**补回时**要**比对哈希 ✗**（`service.rs:2179` ✓）——
    ///     **∴ 不符就**不写**✗（**对的：绝不把另一张图塞进那个哈希 ✓）**；
    ///     **∴ 而**那条**只在**导出侧**被计入 `kept_mismatch` ✗**
    ///       ⇒ **∴ 导入侧**原来**一声不响 ✗** ⇒ **∴ 从导入者视角**这**不可见 ✓**
    ///         ⇒ **∴ 于是**：**用户**只会看到"**少了一笔**"**✗，**而**不知道**为什么 ✓**** ✓✓
    ///   **∴ 现在**：**它**在这里计数 ✗** ⇒ **∴ `import_project`** 把它**写进响应 ✓**
    ///     ⇒ **∴ 打开一个包**立刻知道**有几笔没补回来 ✓**** ✓✓
    unreplayable_blobs: usize,
    created_at: i64,
    last_snapshot_seq: Seq,
    last_snapshot_at: i64,
    commits: u64,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("id", &self.id)
            .field("atoms", &self.log.len())
            .field("head_seq", &self.log.head_seq())
            .field("layers", &self.state.layers.len())
            .field("objects", &self.state.objects.len())
            .finish()
    }
}

impl Document {
    /// 新建文档并提交 `create_document` 原子。
    pub fn create(
        store: Arc<dyn BlobStore>,
        spec: NewDocument,
        actor: &str,
        session: &str,
        settings: DocumentSettings,
    ) -> Result<Self> {
        let mut document = Self::empty(store, spec.doc_id.clone(), settings, yanshi_core::now_ms());
        let atom = Atom::new(
            AtomKind::CreateDocument,
            actor.to_owned(),
            session.to_owned(),
            json!({
                "doc_id": spec.doc_id,
                "width": spec.width,
                "height": spec.height,
                "color_space": "srgb",
                "background": spec.background,
            }),
        );
        document.commit(atom)?;
        Ok(document)
    }

    /// 由已有日志打开文档（服务端重启恢复）。
    pub fn open(
        store: Arc<dyn BlobStore>,
        doc_id: impl Into<String>,
        atoms: Vec<Atom>,
        settings: DocumentSettings,
    ) -> Result<Self> {
        let id = doc_id.into();
        let log = AtomLog::with_atoms(atoms)?;
        let mut document = Self::empty(store, id, settings, yanshi_core::now_ms());
        document.log = log;
        document.rebuild()?;
        Ok(document)
    }

    fn empty(
        store: Arc<dyn BlobStore>,
        id: String,
        settings: DocumentSettings,
        created_at: i64,
    ) -> Self {
        let grid = TileGrid::new(settings.tile_size, 1, 1)
            .unwrap_or_else(|| TileGrid::new(256, 1, 1).expect("256 是允许的 tile 边长"));
        Self {
            id,
            authority: crate::token::TokenAuthority::new(),
            log: AtomLog::new(),
            store,
            state: DocumentState::empty(),
            folder: IncrementalFolder::new(),
            snapshots: SnapshotStore::new(8),
            renderer: Renderer::with_budget(grid, settings.render_cache_bytes),
            jobs: JobManager::new(crate::job::DEFAULT_JOB_TTL_SECONDS),
            broadcaster: Broadcaster::new(),
            annotations: AnnotationStore::new(),
            render_watermark: 0,
            last_render_blob: None,
            full_frame_hits: 0,
            document_thumbnail: None,
            document_thumbnail_seq: 0,
            document_thumb: None,
            preview_dirty: PreviewDirty::Unknown,
            full_frame_render: None,
            full_frame_render_seq: 0,
            full_canvas_renders: 0,
            pinned_blobs: std::collections::BTreeSet::new(),
            document_preview_renders: 0,
            document_thumbnail_renders: 0,
            open_ms: 0,
            last_thumb_blob: None,
            // 16 块：够覆盖 1024² 的四个 512² 区域 ✓，又不会让老块赖着不走 ✓。
            region_cache: yanshi_render::region_block::RegionBlockCache::new(16),
            last_render_warnings: Vec::new(),
            last_below_tiles: (0, 0, 0, 0),
            replayed_blobs: 0,
            unreplayable_blobs: 0,
            created_at,
            last_snapshot_seq: 0,
            last_snapshot_at: 0,
            commits: 0,
            settings,
        }
    }

    /// 文档 id。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 设置。
    pub const fn settings(&self) -> &DocumentSettings {
        &self.settings
    }

    /// 权威日志。
    pub const fn log(&self) -> &AtomLog {
        &self.log
    }

    /// 当前 HEAD 折叠状态。
    pub const fn state(&self) -> &DocumentState {
        &self.state
    }

    /// head seq。
    pub fn head_seq(&self) -> Seq {
        self.log.head_seq()
    }

    /// 原子数。
    pub fn atom_count(&self) -> usize {
        self.log.len()
    }

    /// 累计提交次数（含幂等命中）。
    pub const fn commits(&self) -> u64 {
        self.commits
    }

    /// 创建时间。
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    /// Blob CAS。
    pub fn store(&self) -> &dyn BlobStore {
        &*self.store
    }

    /// 渲染水位。
    pub const fn render_watermark(&self) -> Seq {
        self.render_watermark
    }

    /// **★ 整幅缓存命中次数 ✗ ★**（第 796 轮 ✓）：**∴ 它是**用户实际走的快路径**的指标 ✓**
    ///   （**∴ 而** tile 级 `below_reused` 在**那条路上**不会变 ✓）。
    pub const fn full_frame_hits(&self) -> u64 {
        self.full_frame_hits
    }

    /// Job 管理器（只读）。
    pub const fn jobs(&self) -> &JobManager {
        &self.jobs
    }

    /// Job 管理器（可变）。
    pub fn jobs_mut(&mut self) -> &mut JobManager {
        &mut self.jobs
    }

    /// 广播器（只读）。
    pub const fn broadcaster(&self) -> &Broadcaster {
        &self.broadcaster
    }

    /// 广播器（可变）。
    pub fn broadcaster_mut(&mut self) -> &mut Broadcaster {
        &mut self.broadcaster
    }

    /// 标注通道（只读）。
    pub const fn annotations(&self) -> &AnnotationStore {
        &self.annotations
    }

    /// 标注通道（可变）。
    pub fn annotations_mut(&mut self) -> &mut AnnotationStore {
        &mut self.annotations
    }

    /// 令牌管理（可变）。
    pub fn authority_mut(&mut self) -> &mut crate::token::TokenAuthority {
        &mut self.authority
    }

    /// 发放令牌（12.7）。
    pub fn issue_token(&mut self, actor: &str, role: Role) -> CapabilityToken {
        self.authority.issue(actor, role)
    }

    /// 鉴权。
    pub fn authorize(
        &self,
        token: Option<&CapabilityToken>,
        transport: TransportKind,
        fallback_actor: &str,
    ) -> Result<Principal> {
        self.authority.authorize(token, transport, fallback_actor)
    }

    /// 订阅推送（6.8）。
    pub fn subscribe(&mut self, session: &str, channel: PushChannel) -> SubscriberId {
        self.broadcaster.subscribe(session, channel)
    }

    /// 重新折叠整篇日志（打开文档、恢复、调试用）。
    ///
    /// **走 `folder` 而不是一次性 `FoldEngine::fold`** ✓（冷启动专题）：两者算出的状态完全一样 ✓，
    /// 但 `FoldEngine::fold` 用的是**临时**缓存 ✗ ⇒ `self.folder.last` 仍是 `None` ✓
    /// ⇒ **打开后的第一次提交又会全量重放一遍** ✗（实测 494 atom ≈ 30ms ×2 ✓）。
    /// 走 `folder` ⇒ 打开这一次顺带把增量折叠器**预热**好 ✓ ⇒ 首笔提交只折叠新增区间 ✓。
    pub fn rebuild(&mut self) -> Result<()> {
        self.folder = IncrementalFolder::new();
        // **走折叠器而不是另起一次 `FoldEngine::fold`** ✓：`IncrementalFolder` 的完整求值
        // 与 `FoldEngine::fold` 逐字段一致 ✓（见 `seq` 模块的不变量 ✓），但它顺手留下
        // **还原点阶梯** ✓ ⇒ 之后第一次提交不必再全量折一遍 ✓、第一次撤销也能就近续折 ✓
        //（实测 1050 原子工程：完整求值 103ms／续折一次 ~20ms ✓）。
        self.state = self.folder.fold_head(&self.log)?.state;
        self.render_watermark = self.state.head_seq;
        let grid = self.grid_for_state()?;
        let cache_bytes = self.settings.render_cache_bytes;
        self.renderer = Renderer::with_budget(grid, cache_bytes);
        Ok(())
    }

    fn grid_for_state(&self) -> Result<TileGrid> {
        let width = self.state.width.max(1);
        let height = self.state.height.max(1);
        TileGrid::new(self.settings.tile_size, width, height).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("非法 tile 边长 {}", self.settings.tile_size)),
            )
        })
    }

    /// 提交原子（服务端权威路径）。
    pub fn commit(&mut self, atom: Atom) -> Result<CommitResult> {
        self.commit_as(atom, "system", false, None)
    }

    /// 以指定 actor 提交，可选 owner 权限（跨 actor revert，12.5）。
    /// **预览渲染的计数**（只读 ✓）：`(走了渲染的次数, 其中整幅的次数)` ✓。
    ///
    /// **为什么要有出口** ✗（第 274 轮 ✓）：`document_preview_renders` / `full_canvas_renders`
    /// 的注释写着它们是"**这条路真的走了**"的凭证 ✓、判据用它 ✓ ——
    /// 但它们是**私有字段、且不进任何响应** ✗ ⇒ **判据根本读不到** ✓
    /// ⇒ 于是第 273 轮那条"空白文档冷启动不再整幅光栅化"的优化**没有能红的判据** ✗。
    /// ⇒ 这里给它一个只读出口 ✓，判据即可断言"**空文档的冷启动预览不增加这个计数**" ✓
    /// （**结构性**判据 ✓：与区域大小、与机器快慢都无关 ✓）。
    pub fn preview_render_counts(&self) -> (usize, usize) {
        (self.document_preview_renders, self.full_canvas_renders)
    }

    /// 渲染缓存统计（可观测性：设计 1319 行要求包含缓存/生命周期指标）。
    pub fn cache_stats(&self) -> yanshi_render::TileCacheStats {
        self.renderer.cache().stats()
    }

    /// 文档占用的像素缓冲字节数（图层 + 对象的近似占用，用于观测）。
    pub fn pixel_bytes(&self) -> usize {
        let state = self.state();
        let layers = state.alive_layers().len();
        (state.width as usize) * (state.height as usize) * 4 * layers
    }

    /// 提交一个原子（可指定 changeset 归属与跨 actor 权限）。
    /// **整批预校验** ✓（设计中 §12.4 的"离线编辑重连"要用它 ✓）。
    ///
    /// **为什么需要它** ✓：§897 要求"校验失败 ⇒ 原子及其 blob **一起**打包进 Stash" ✓ ——
    /// 也就是**整批要么全进日志、要么一条都不进** ✓。而 `commit_as` 是**逐条**的 ✓
    /// ⇒ 直接循环提交的话 ✓，第三条失败时前两条**已经进日志了** ✗ ⇒ 那就不是"打包"而是"半途而废" ✗。
    ///
    /// **做法** ✓：在**状态的副本**上增量折叠并逐条校验 ✓ ——
    /// 注意必须**增量** ✗：离线批次里的原子会互相引用 ✓（后一条引用前一条建的对象 ✓），
    /// 若把每条都对着"批次之前的状态"校验 ✓ 就会**误判**成引用不存在 ✗。
    /// 用状态副本而不是日志副本 ✓ 是因为折叠结果才是校验的输入 ✓（便宜得多 ✓）。
    pub fn validate_batch(&self, atoms: &[Atom], actor: &str, owner: bool) -> Result<()> {
        let mut state = self.state.clone();
        for atom in atoms {
            let validation = {
                let exists = |hash: &BlobHash| self.store.exists(hash);
                let mut context = CommitContext::new(&state, &exists, actor, &atom.session);
                if owner {
                    context = context.allow_cross_actor_revert(true);
                }
                self.log.validate_commit(atom, &context)
            };
            validation?;
            // 校验通过 ⇒ 把它折进副本 ✓，供下一条校验 ✓。
            let folded = yanshi_core::fold::fold_atoms(state, std::slice::from_ref(atom));
            state = folded.state;
        }
        Ok(())
    }

    /// 以指定 actor 提交一条原子 ✓（`commit` 是它的简写 ✓）。
    ///
    /// **`needs_previous_state` 那一支** ✓：有些原子的校验要"提交前的状态" ✓（`previous` ✓），
    /// 折叠器据此做前后对比 ✓ —— 不需要的原子就不克隆状态 ✓（省一次大拷贝 ✓）。
    pub fn commit_as(
        &mut self,
        atom: Atom,
        actor: &str,
        owner: bool,
        changeset_id: Option<ChangesetId>,
    ) -> Result<CommitResult> {
        self.commit_as_timed(
            atom,
            actor,
            owner,
            changeset_id,
            &mut CommitPhases::default(),
        )
    }

    /// **`commit_as` 的带计时版本** ✓（外部测试报告 P2）：把折叠 ✓ / 脏区 ✓ / 日志 ✓
    /// 三段分别量出来 ✓，填进 `phases` ✓。
    ///
    /// **为什么不改 `commit_as` 的签名** ✗：它被测试与内部大量调用 ✓ ⇒
    /// 改签名会为了**一个观察功能**去动所有调用方 ✗；加一个 `_timed` 兄弟、老的转调它 ✓
    /// 是本项目一贯的做法 ✓（`commit` / `commit_changeset` 同理 ✓）。
    pub fn commit_as_timed(
        &mut self,
        mut atom: Atom,
        actor: &str,
        owner: bool,
        changeset_id: Option<ChangesetId>,
        phases: &mut CommitPhases,
    ) -> Result<CommitResult> {
        // 变更集是原子字段（5.1）；显式写在原子上者优先。
        if atom.changeset_id.is_none() {
            if let Some(id) = &changeset_id {
                atom.changeset_id = Some(id.clone());
            }
        }
        let previous = if needs_previous_state(atom.kind) {
            Some(self.state.clone())
        } else {
            None
        };
        let atom_session = atom.session.clone();
        let atom_id = atom.id.clone();

        // 日志段：校验 ＋ 追加（写内存日志 ✓；落盘由 `Workspace::journal` 量 ✓）。
        let log_started = std::time::Instant::now();
        let validation = {
            let exists = |hash: &BlobHash| self.store.exists(hash);
            let mut context = CommitContext::new(&self.state, &exists, actor, &atom_session);
            if owner {
                context = context.allow_cross_actor_revert(true);
            }
            self.log.append_validated(atom, &context)
        };
        phases.log_us = phases
            .log_us
            .saturating_add(log_started.elapsed().as_micros() as u64);
        let outcome = match validation {
            Ok(outcome) => outcome,
            // 12.3 第 2 步：采样性替换冲突时先确保冲突图层存在，再把错误交回客户端。
            Err(error) if error.code == ErrorCode::Conflict => {
                return Err(self.ensure_conflict_layer(error)?)
            }
            Err(error) => return Err(error),
        };
        self.commits += 1;
        let seq = outcome.seq();
        let duplicate = !outcome.is_appended();

        // 幂等命中：不重复折叠、不重复广播。
        if duplicate {
            return Ok(CommitResult {
                atom_id,
                seq,
                duplicate: true,
                changeset_id: changeset_id.clone(),
                head_seq: self.log.head_seq(),
                dirty_kind: "none",
                dirty_bbox: None,
                dirty_tiles: Vec::new(),
                warnings: Vec::new(),
                job_id: None,
                snapshotted: false,
            });
        }

        let appended = self
            .log
            .by_seq(seq)
            .cloned()
            .ok_or_else(|| internal("刚追加的原子不在日志中"))?;

        // 折叠段（6.6）。
        let fold_started = std::time::Instant::now();
        let folded: StateAt = self.folder.fold(&self.log, self.log.head_seq())?;
        self.state = folded.state;
        if self.state.width > 1 {
            if let Ok(grid) = self.grid_for_state() {
                if grid.width() != self.renderer.grid().width()
                    || grid.height() != self.renderer.grid().height()
                    || grid.tile_size() != self.renderer.grid().tile_size()
                {
                    let cache_bytes = self.settings.render_cache_bytes;
                    self.renderer = Renderer::with_budget(grid, cache_bytes);
                    self.render_watermark = 0;
                }
            }
        }
        phases.fold_us = phases
            .fold_us
            .saturating_add(fold_started.elapsed().as_micros() as u64);

        // 脏区段：双 dirty 传播 → 失效 tile（6.6）。
        let dirty_started = std::time::Instant::now();
        let dirty = plan_dirty_with_log(&self.state, previous.as_ref(), &self.log, &appended);
        let dirty_tiles = self.renderer.apply_dirty(&self.state, &dirty);
        // **累积文档脏区** ✓：给"增量文档预览"用（性能专题，第 1040 轮）。
        // `Full`（求值起点跳变等）或"结构脏但没有 bbox" ⇒ 视为整幅 ✓（保守但正确 ✓）。
        // **元数据类原子 ⇒ 复用既有预览（只重组、不重画 ✓）** —— 第 223 轮 ✓
        // **两道条件同时成立才复用** ✓：① **白名单命中**（确定不改像素 ✓）；
        // ② **渲染器也认为没有 tile 脏**（`dirty_tiles` 为空 ✓）⇒ **双重保守 ✓，绝不撒谎 ✗**。
        let preview_dirty = if dirty_tiles.is_empty() && atom_cannot_change_pixels(appended.kind) {
            PreviewDirty::Clean
        } else {
            planned_preview_dirty(&dirty)
        };
        self.preview_dirty = self.preview_dirty.merged(preview_dirty);
        phases.dirty_us = phases
            .dirty_us
            .saturating_add(dirty_started.elapsed().as_micros() as u64);

        // 控制流广播（6.8）。
        self.broadcaster.publish_atom(&appended);
        if !dirty_tiles.is_empty() {
            self.broadcaster
                .publish_tiles(self.renderer.grid().tile_size(), &dirty_tiles);
        }

        // 重型原子创建 Job（6.7）：渲染异步，客户端轮询。
        let job_id = if appended.is_heavy() {
            let job = self.jobs.submit(
                appended.kind.as_str(),
                appended.session.clone(),
                yanshi_core::now_ms(),
            );
            Some(job)
        } else {
            None
        };

        // 快照策略（6.5）。
        let now = yanshi_core::now_ms();
        let decision: SnapshotDecision = yanshi_core::snapshot::snapshot_decision(
            &self.log,
            self.last_snapshot_seq,
            self.last_snapshot_at,
            now,
        );
        let mut snapshotted = false;
        if decision.needed && decision.lag_atoms >= self.settings.snapshot_lag_atoms {
            if let Ok(id) = self.snapshot_now(now) {
                let _ = id;
                snapshotted = true;
            }
        }

        let warnings = folded
            .warnings
            .iter()
            .map(|warning| {
                let label = match warning.kind {
                    WarningKind::CascadeInvalidation => "cascade_invalidation",
                    WarningKind::PreconditionFailed => "precondition_failed",
                    WarningKind::DeclareHeadOutsideFormula => "declare_head_outside_formula",
                };
                json!({"atom_id": warning.atom_id, "seq": warning.seq, "kind": label, "detail": warning.detail})
            })
            .collect();

        Ok(CommitResult {
            atom_id,
            seq,
            duplicate: false,
            changeset_id: changeset_id.or_else(|| appended.changeset_id.clone()),
            head_seq: self.log.head_seq(),
            dirty_kind: dirty_kind_str(&dirty),
            dirty_bbox: dirty.bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
            dirty_tiles,
            warnings,
            job_id,
            snapshotted,
        })
    }

    /// 确保冲突图层存在（12.3）：不存在则以系统 actor（`system:conflict`）追加 `create_layer`。
    fn ensure_conflict_layer(&mut self, mut error: YanshiError) -> Result<YanshiError> {
        let existing = self
            .state
            .layers
            .values()
            .find(|layer| {
                !layer.is_deleted()
                    && layer
                        .metadata
                        .get("conflict")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
            })
            .map(|layer| layer.id.clone());
        let layer_id = match existing {
            Some(layer_id) => layer_id,
            None => {
                let layer_id = format!("layer_conflict_{}", yanshi_core::Ulid::new().encode());
                let z_index = self
                    .state
                    .alive_layers()
                    .iter()
                    .map(|layer| layer.z_index + 1)
                    .max()
                    .unwrap_or(1);
                let atom = yanshi_core::conflict::conflict_layer_atom(
                    layer_id.clone(),
                    Some(&self.id),
                    z_index,
                );
                self.append_system_atom(atom)?;
                layer_id
            }
        };
        error
            .context
            .extra
            .insert("conflict_layer_id".to_owned(), json!(layer_id));
        Ok(error)
    }

    /// 追加服务端系统原子（不经客户端校验路径），并完成折叠、dirty 与广播。
    fn append_system_atom(&mut self, atom: Atom) -> Result<Seq> {
        let previous = Some(self.state.clone());
        let outcome = self.log.append(atom)?;
        let seq = outcome.seq();
        let appended = self
            .log
            .by_seq(seq)
            .cloned()
            .ok_or_else(|| internal("系统原子未进入日志"))?;
        let folded = self.folder.fold(&self.log, self.log.head_seq())?;
        self.state = folded.state;
        let dirty = plan_dirty_with_log(&self.state, previous.as_ref(), &self.log, &appended);
        let tiles = self.renderer.apply_dirty(&self.state, &dirty);
        self.broadcaster.publish_atom(&appended);
        if !tiles.is_empty() {
            self.broadcaster
                .publish_tiles(self.renderer.grid().tile_size(), &tiles);
        }
        Ok(seq)
    }

    /// `state@seq_n`（5.5 时间旅行）。
    pub fn state_at(&mut self, seq: Seq) -> Result<StateAt> {
        let mut cache = yanshi_core::StateAtCache::new();
        yanshi_core::seq::state_at(&self.log, seq, &mut cache)
    }

    /// `seq > since` 的原子元数据（MCP 轮询 `get_log`，6.7）。
    pub fn log_since(&self, since: Seq) -> Vec<&Atom> {
        self.log.iter().filter(|atom| atom.seq > since).collect()
    }

    /// 渲染区域并输出 PNG 到 CAS（7.5：返回 URL 而非 base64）。
    /// 渲染区域并返回**原始 RGBA8**（sRGB 直通字节），供 `patch` 抓取源像素。
    ///
    /// 与 `render_region` 的区别：不编码 PNG、不写渲染缓存/缩略图状态，
    /// 因为调用方要的是像素而不是可展示的产物。
    pub fn render_region_raw(&mut self, bbox: Bbox) -> Result<(u32, u32, Vec<u8>)> {
        if let Ok(path) = std::env::var("YANSHI_REGION_PROBE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(
                    f,
                    "DOC.render_region_raw bbox=({},{},{},{})",
                    bbox.x, bbox.y, bbox.w, bbox.h
                );
            }
        }
        // **先查区域字节缓存** ✓（设计 §8.4 ✓）。
        //
        // **版本 = HEAD 序号** ✓ —— 这是"绝不给旧像素 ✗"的根据 ✓：
        // 任何提交都会推进 HEAD ✓，于是版本一变、键立刻不命中 ✓，宁可重算 ✓。
        // （本轮的范围与取舍见 `region_block` 的模块说明 ✓：先做区域级 ✓、不做按块渲染 ✗，
        //  因为按块渲染要处理滤镜外扩跨块 ✓，那会碰到"分块与整幅必须一致"这条硬不变量 ✗。）
        // **★ 指纹代替裸 `head_seq` ✓ ★**（第 465 轮 ✓）：**∴ 只改最上层 ⇒ 子区域缓存仍有效 ✓**；
        // **∴ 而内容一变 ⇒ 指纹必变 ⇒ 失效 ✓**（**外部报告：子区域 531 → 426 → 347 ms ✗**）。
        // **★ 诊断开关变量 ✓ ★**（第 554 轮 ✓）：**未设 ⇒ `false` ⇒ 行为不变 ✓**。
        let skip_region_cache = std::env::var("YANSHI_SKIP_REGION_CACHE").is_ok();
        let version = yanshi_render::region_block::region_fingerprint(&self.state);
        let key = yanshi_render::region_block::BlockKey::from_bbox(bbox.x, bbox.y, bbox.w, bbox.h);
        // **★ 探针 ✓**（第 552 轮 ✓，**查明后删 ✓**）：**打印**是否命中外层 ＋ 键 ＋ 版本**✗** ⇒
        // **∴ 一次看出**我的区域是否到了这里 ✗、命中与否 ✗、键是否漂移 ✗****。
        if let Ok(path) = std::env::var("YANSHI_REGION_PROBE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let line = format!(
                    "REGION key=({},{},{},{}) version={} hit={}\n",
                    key.x,
                    key.y,
                    key.w,
                    key.h,
                    version,
                    self.region_cache.get(key, version).is_some()
                );
                let _ = f.write_all(line.as_bytes());
            }
        }
        // **★ 诊断开关 ✓ ★**（第 554 轮 ✓）：**跳过外层区域块缓存 ✓** ——
        // **∴ `YANSHI_SKIP_REGION_CACHE=1` ⇒ get 与 put 都跳 ⇒ **∴ 每次渲染都进 below ✓**；
        // **∴ 未设 ⇒ 行为完全不变 ✓**。**∴ 用途** ✓：**验证 below 的复用 ✓**（**判据需要它 ✗**）。
        if !skip_region_cache {
            if let Some(block) = self.region_cache.get(key, version) {
                // **按缓存块自己的尺寸回** ✓（键里已经带着宽高 ✓ ⇒ 与请求的一致 ✓；
                // 渲染时用了裁剪后的尺寸也没关系 ✓ —— 存与查用的是**同一个请求键** ✓）。
                //
                // **缓存命中 ⇒ 上次没有跳过任何东西** ✓：带告警的渲染**从不进缓存** ✗
                //（见下面 `if rendered.stats.unsupported.is_empty()` ✓）⇒ 命中就等价于"完整" ✓。
                self.last_render_warnings.clear();
                return Ok((block.key.w, block.key.h, block.data.clone()));
            }
        }
        let rendered = self
            .renderer
            .render_region(&self.state, &*self.store, bbox)?;
        // **跳过的东西必须留下痕迹** ✗（否则裸像素出口 = 静默的不完整画面 ✗）。
        self.note_render_stats(&rendered.stats);
        let data = rendered.rgba8.clone();
        // **不完整的画面绝不进缓存** ✗：缓存里没有"告警"这一维 ✓ ⇒ 一旦缓存了 ✓，
        // 之后**命中**就会把同一张缺块的图**当成完整的**发出去 ✓（而且没有任何提示 ✗）。
        // 缺块本身应当是**罕见**的 ✓ ⇒ 放弃这一次缓存**不影响**正常路径的性能 ✓。
        if !skip_region_cache && rendered.stats.unsupported.is_empty() {
            self.region_cache
                .put(yanshi_render::region_block::RegionBlock {
                    // **用"请求的键"存** ✓（不是渲染后的尺寸 ✗）：查的时候用的是请求键 ✓，
                    // 两边不一致就会永远差一点点、永不命中 ✗ —— 我第一版正是这个错 ✗。
                    key,
                    hash: yanshi_render::region_block::content_hash(&data),
                    data,
                    version_atom: version,
                    layers: self.state.alive_layers().len() as u32,
                    objects: self.state.alive_objects().len() as u32,
                });
        }
        Ok((rendered.width, rendered.height, rendered.rgba8))
    }

    /// **只渲染某一层** ✓（单图层导出 ✓）—— **刻意绕过区域字节缓存** ✗。
    ///
    /// **为什么必须绕过** ✗：那张缓存的键是**（区域, 版本）** ✓，**不含图层** ✗
    /// ⇒ 一旦复用 ✓，就可能把**别的图层**的像素当成这一层的返回 ✓ ——
    /// 这正是"**缓存键少了一个维度**"的经典 bug ✓，而且它**只在缓存命中时**发作 ✓（最难查的一类 ✓）。
    /// **代价可以接受** ✓：逐层导出是**低频且刻意**的操作 ✓ ⇒ 直接算 ✓，不冒这个险 ✓。
    /// **★ 渲染区域，并指定"当前层"作为三段分解的切点 ✓ ★**（**目标第 4 条 ✓**；第 521 轮 ✓）。
    ///
    /// **为什么要单独一个方法** ✗：**`render_region_raw` 有 7 处调用**✗** ⇒
    /// **∴ 给它加参数会波及全部 ✗** ⇒ **∴ 用变体 ⇒ 只有需要它的那一处改 ✓**。
    ///
    /// **∴ 必然还原** ✓（**照 [`Self::render_region_raw_layer`] 的写法 ✓**）：
    /// **∴ 否则"切点"会**泄漏**到之后的每一次渲染 ✗ ⇒ **∴ 那就等于**撒谎**✓**。
    pub fn render_region_raw_active(
        &mut self,
        bbox: Bbox,
        active_layer: Option<&str>,
    ) -> Result<(u32, u32, Vec<u8>)> {
        // **★ 临时探针 ✓**（第 527 轮 ✓，**查明后删 ✓**）：**打印**收到的 `active_layer`**✗** ⇒
        // **∴ 分清**"工具层没传到 ✗"与"`set_active_layer` 没生效 ✗"**（**两者症状相同 ✓**）**。
        if let Ok(probe) = std::env::var("YANSHI_BELOW_PROBE") {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&probe)
            {
                let _ = writeln!(f, "active_layer_received={:?}", active_layer);
            }
        }
        let previous = self
            .renderer
            .set_active_layer(active_layer.map(|id| id.to_owned()));
        let rendered = self.render_region_raw(bbox);
        // **无论成败都要还原** ✓（**同 `render_region_raw_layer` ✓**）。
        self.renderer.set_active_layer(previous);
        rendered
    }

    /// **只渲染某一层 ✓**（**单图层导出 ✓**）：**临时把渲染器切成"只画一层"✓，
    /// 渲染后**必然还原**✓**（**否则那个开关会泄漏到下一次渲染 ✗**）。
    /// **⚠️ 返回值**：**`(宽, 高, 原始 RGBA8 ✓)`** —— **∴ 与 [`Self::render_region_raw`] 同形 ✓**。
    pub fn render_region_raw_layer(
        &mut self,
        bbox: Bbox,
        layer_id: &str,
    ) -> Result<(u32, u32, Vec<u8>)> {
        let previous = self.renderer.set_only_layer(Some(layer_id.to_owned()));
        let rendered = self.renderer.render_region(&self.state, &*self.store, bbox);
        // **无论成功失败都要还原** ✓（否则这个"只画一层"的开关会**泄漏到下一次渲染** ✗）。
        self.renderer.set_only_layer(previous);
        let rendered = rendered?;
        if rendered.rgba8.is_empty() {
            return Err(yanshi_core::YanshiError::new(
                yanshi_core::ErrorCode::ReferenceNotFound,
                yanshi_core::ErrorContext::detail(format!(
                    "图层 {layer_id} 渲染出来是空的 ⇒ 它可能不存在，或它里面什么都没有"
                )),
            ));
        }
        // **这条出口也带告警** ✓（它同样返回裸像素 ✓）。
        self.note_render_stats(&rendered.stats);
        Ok((rendered.width, rendered.height, rendered.rgba8))
    }

    /// **最近一次渲染里被跳过的东西** ✓（裸像素出口的告警通道 ✓，见字段说明 ✓）。
    ///
    /// **为什么不是一个 `Result`** ✗：缺一个补丁**不该**让整幅渲染失败 ✓
    ///（一条丢了的笔触不该让整张画都出不来 ✓）⇒ 像素照给 ✓、告警挂在这里 ✓、
    /// 由调用方一起报给用户 ✓（`render_region` 那条路本来就把告警放进 `warnings` ✓）。
    /// **★ 把一次渲染的账目**就地记下** ✗ ★**（第 2 轮 ✓）。
    ///
    /// **∴ 为什么收成一个函数 ✗**：**告警 ＋ tile 账目**必须**同时更新**✗
    ///   ⇒ **∴ 若**各出口**各写各的**✗ ⇒ **∴ 迟早**漏掉一处 ⇒ **∴ 那一处就会**报旧值** ✓**
    ///     ⇒ **★ 那正是"**lazy 变成撒谎 ✓"的经典形态 ✓ ★**** ✓✓
    fn note_render_stats(&mut self, stats: &RenderStats) {
        self.last_render_warnings = stats.unsupported.clone();
        // **★ 只有"**真的做过 below 判定**"的渲染**才能写这三格 ✗ ★**（第 2 轮 ✓）：
        //   **∴ 为什么 ✗**：**同一请求里**可能渲染**多次**✗
        //     ⇒ **∴ 后一次**（**如**只画一层／整幅命中 ✓）**没有账目**✗
        //       ⇒ **∴ 若**它写 `(0,0,0)`**✗ ⇒ **∴ 就把**真账目抹掉 ✓**
        //         ⇒ **★ 实测**：写两次，读到的**永远是 0 ✓ ★**** ✓✓
        //   **∴ 于是**：**"**没测量 ✓"**保留**上一次的**真读数**✗
        //     ⇒ **∴ 而**告警**仍然**每次都更新 ✓**（**∴ 那**是对的 ✓）** ✓✓
        if stats.below_tiles_measured {
            self.last_below_tiles = (
                stats.below_tiles_wanted,
                stats.below_tiles_available,
                stats.below_tiles_reused,
                stats.below_tiles_missing,
            );
        }
    }

    /// **★ 最近一次渲染的 below tile 账目 ✗ ★**：`(想要, 已有, 用上)` ✓。
    pub fn below_tiles(&self) -> (usize, usize, usize, usize) {
        self.last_below_tiles
    }

    /// **最近一次渲染里被跳过的东西** ✓（**裸像素出口的告警通道 ✓，见字段说明 ✓**）。
    pub fn last_render_warnings(&self) -> &[String] {
        &self.last_render_warnings
    }

    /// **打开时重放回来的位图数** ✓（0 ＝ 包里本来就都带着 ✓，或没有可重放的配方 ✓）。
    pub const fn replayed_blobs(&self) -> usize {
        self.replayed_blobs
    }

    /// 记下"打开时重放回来几条" ✓（由 `Workspace::open_document` 在补完之后写一次 ✓）。
    pub(crate) fn set_replayed_blobs(&mut self, count: usize) {
        self.replayed_blobs = count;
    }

    /// **"**补不回来**"的位图数 ✓**（第 942 轮 ✓）—— **∴ 必须**如实报出 ✗**
    /// （**∴ 静默丢弃**是本仓头号病根 ✓）。
    pub const fn unreplayable_blobs(&self) -> usize {
        self.unreplayable_blobs
    }

    pub(crate) fn set_unreplayable_blobs(&mut self, count: usize) {
        self.unreplayable_blobs = count;
    }

    /// 区域字节缓存的统计 ✓（设计要求可观测 ✓）。
    pub fn region_cache_stats(&self) -> yanshi_render::region_block::RegionBlockStats {
        self.region_cache.stats()
    }

    /// **解码位图缓存的统计** ✓（判据据此断言"同一块补丁没有被重复解码" ✓）。
    ///
    /// `misses` 就是真正 `store.get` + 解码的次数 ✓ ⇒
    /// "带背景层的文档上，第 2 笔起不能再出现背景的解码"这句话可以被**直接断言** ✓，
    /// 而不必靠墙钟读数去猜 ✓。
    pub fn bitmap_cache_stats(&self) -> yanshi_render::render::BitmapCacheStats {
        self.renderer.bitmap_cache_stats()
    }

    /// **收集当前仍被缓存指针引用的 blob** ✓（淘汰时的"保留名单" ✓）。
    ///
    /// **`full_frame_render` 必须在这个名单里** ✗ —— 这一条是被实测抓住的 ✓：
    /// 整幅渲染落盘后它是"整幅区域渲染的缓存" ✓，而**下一次增量预览会把
    /// `last_render_blob` 指向新的 256² 图** ✓ ⇒ 若这里不保留它 ✓，它就会被当孤儿删掉 ✗
    /// ⇒ 整幅请求再也命中不了缓存 ✓、退回整幅重渲染（实测 227s ✗）。
    fn cached_preview_hashes(&self) -> std::collections::BTreeSet<BlobHash> {
        let mut keep = std::collections::BTreeSet::new();
        for slot in [
            self.last_render_blob.as_ref(),
            self.document_thumbnail.as_ref(),
            self.last_thumb_blob.as_ref(),
            self.full_frame_render.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            keep.insert(slot.clone());
        }
        keep
    }

    /// **淘汰被替换掉的预览 blob** ✓（缓存语义 ✓，设计 §6.4"分层缓存" ✓）。
    ///
    /// **为什么必须做** ✓：预览/缩略图是**缓存** ✓（不是被原子引用的内容 ✓），
    /// 但它们**写进了 CAS** ✓ ⇒ 每一份都是**孤儿** ✓ ⇒ 实测真实工作区里
    /// **2161 个 blob 中有 1912 个是孤儿、共 1.07 GB（约 95%）** ✗，
    /// 而单个体积中位 55 KB ✓、每次提交产生约一份 ✓ ⇒ 每天上百 MB ✗。
    /// 一处小疏忽（"写新的"却没"删旧的" ✓）在**每笔提交**上复利 ✓ ⇒ 就成了最大的一块占用 ✗。
    ///
    /// **判据** ✓：只有当旧 blob **不再被任何"最新"指针引用**时才删 ✓ ——
    /// 缩略图与"最后一次渲染"可能指向**同一份** ✓（整幅渲染同时更新两者 ✓）⇒ 那就不能删 ✗。
    ///
    /// **判据用"集合"，不用逐个特判** ✓ —— 这一条是被测试逼出来的 ✗：
    /// 我第一版逐个检查"旧的还是不是某个指针" ✓，而**整幅渲染会同时更新区域预览与文档缩略图** ✓
    /// ⇒ 下一个渲染来淘汰时 ✓，看到"旧 blob 还是文档缩略图"就**放过了** ✗——
    /// 可那份缩略图**刚刚已经被换掉** ✓。表现为：**每次渲染都多留一份** ✗
    ///（测试实测 301B → 502B → 651B → 806B 一路累积 ✓）。
    /// 正确的一般形式是 ✓：**先更新全部指针** ✓，再淘汰"旧集合里、已不在保留集合中"的那些 ✓。
    fn evict_replaced_previews(&self, previous: &std::collections::BTreeSet<BlobHash>) {
        let keep = self.cached_preview_hashes();
        for old in previous {
            if keep.contains(old) {
                continue;
            }
            // **钉住的 blob 不许删** ✓（第 301 轮 ✓）：参考图正是"曾被当成预览、后来被用户钉住"的 blob ✓
            // ⇒ 少了这一条，用户设完参考图，**下一次落笔**就把它删掉 ✓（实测调用栈已证 ✓）。
            if self.pinned_blobs.contains(old) {
                continue;
            }
            // 删不掉也不影响正确性 ✓（它只是缓存 ✓）⇒ 不向上报错 ✓。
            let _ = self.store.remove(old);
        }
    }

    /// 渲染区域并编码为 PNG 写入 CAS（7.2）；整幅覆盖时同时更新文档级缩略图。
    ///
    /// **冷启动复用** ✓（14.5 / 本专题）：若请求覆盖整幅画布 ✓、且磁盘上那份**整幅**渲染
    /// 对应的 seq **正好等于 HEAD** ✓，直接把它当作本次结果返回 ✓ —— 不重渲染 ✗。
    /// 这是**纯缓存命中** ✓：同一份像素、同一个 blob ✓ ⇒ 输出逐字节相同 ✓。
    /// 落后一个 seq 都不许命中 ✗（那是"拿旧图冒充 HEAD" ✗）——见 `full_frame_render_seq` 比对。
    /// **渲染该区域并完成 job，但不编码、不落盘** ✓（第 5 轮 ✓，P0 预览解耦 ✓）。
    ///
    /// **为什么要有它** ✗：连续作画时**每一笔**都要编一张预览 PNG ✓ ⇒ 单笔里 **~130 ms** 花在
    /// `encode_png` 上（4K、`Clouds.myb`、`size 180`、5 点长笔触；实测 `preview_ms` 116~148 ms ✓）。
    /// 而 `finish_mutation` 里"要不要渲染"与"要不要给调用方 PNG"是**两件事** ✓：
    /// job 的完成（渲染水位 ✓）**必须**照旧 ✓，但那**不需要**一张 PNG ✓。
    ///
    /// **为什么不去掉 `finish_mutation` 的渲染** ✗（我试过，实测更差 ✓）：那样长笔触会掉进 job 分支
    /// ⇒ 整幅 4K 预览 ⇒ **962 ms/笔** ✗（比原来的 264 ms 更差 ✓）。
    /// **∴ 这条路复用同一条便宜的脏区渲染** ✓，**只省掉最后的编码与落盘** ✓。
    ///
    /// **代价两面** ✗：收益＝每笔省下编码（实测 88 ns/px ✓）与一次 CAS 写入 ✓；
    /// 代价＝这一笔**不产出预览 blob** ⇒ `last_render_blob` / 文档缩略图**不更新** ✗
    /// ⇒ 查看器要看到画面得等下一次"要预览"的调用（或 `export_png` / `get_document` ✓）。
    /// **∴ 只在调用方明确不要图时才走这里** ✓（默认行为一个字节不变 ✓）。
    pub fn render_region_complete_jobs(&mut self, bbox: Bbox) -> Result<()> {
        let region = bbox;
        // 整幅 + 缓存命中：与 `render_region` 一样**必须走完渲染收尾** ✓（否则漏掉 job 完成 ✗），
        // 但这里**不需要**那份 PNG ✓ ⇒ 直接返回 ✓。
        if covers_canvas(region, self.state.width, self.state.height)
            && self.current_full_frame_preview()?.is_some()
        {
            self.complete_render_jobs()?;
            return Ok(());
        }
        let full_canvas = covers_canvas(region, self.state.width, self.state.height);
        // **同一条渲染** ✓（这才是"不换路"的意思 ✓）。
        let rendered = self
            .renderer
            .render_region(&self.state, &*self.store, region)?;
        if full_canvas {
            self.full_canvas_renders += 1;
        }
        // **告警照旧留档** ✓（不因为"不要图"就静默 ✓）。
        self.note_render_stats(&rendered.stats);
        // **推进水位 + 完成 job** ✓（这两件事与"要不要 PNG"无关 ✓）。
        self.render_watermark = self.log.head_seq();
        self.complete_render_jobs()?;
        // **不编码、不落盘、不淘汰** ✗：没有新 blob ⇒ 也就没有"被替换的旧预览" ✓。
        Ok(())
    }

    /// 渲染区域、写入渲染缓存并返回可展示的预览（含 PNG blob 与取回地址）。
    ///
    /// **与 [`Self::render_region_complete_jobs`] 的区别** ✓：这条会**编码 PNG 并落盘** ✓，
    /// 且会更新 `last_render_blob` / 文档缩略图 / 整幅缓存 ✓；那条只渲染并完成 job ✓。
    pub fn render_region(&mut self, bbox: Bbox) -> Result<RenderedPreview> {
        // **★ 每次渲染**开头**重置后端记录 ✗ ★**（**第 465 轮 ✓；**目标第 7 条 ✓）：
        //   **∴ 为什么 ✗**：**第 456 轮**把 `Gpu` 粘住**✗
        //     ⇒ **∴ 于是** `render_backend` **变成**「**曾经用过**」** ✓
        //       ⇒ **∴ 症状 ✗**：**`--gpu off` 那条**也显示 `gpu`** ✓
        //     ⇒ **∴ 现在**：**本函数是**渲染入口**✗
        //       ⇒ **∴ 在这里**重置**✗
        //         ⇒ **∴ 于是**：**`render_backend` ＝ **最近一次渲染**的后端** ✓ ★**** ✓✓
        yanshi_render::begin_render();
        let region = bbox;
        if covers_canvas(region, self.state.width, self.state.height) {
            if let Some(preview) = self.current_full_frame_preview()? {
                // **缓存命中也要走完"渲染收尾"** ✓：本地渲染路径靠 `complete_render_jobs()`
                // 推进 job 状态 ✓（`heavy_atoms_create_jobs_and_render_completes_them` 守着 ✓）
                // ⇒ 快路径少这一步就会**漏掉 job 完成** ✗。
                self.complete_render_jobs()?;
                return Ok(preview);
            }
        }
        let full_canvas = covers_canvas(region, self.state.width, self.state.height);
        let rendered = self
            .renderer
            .render_region(&self.state, &*self.store, region)?;
        if full_canvas {
            self.full_canvas_renders += 1;
        }
        let png = encode_png(rendered.width, rendered.height, &rendered.rgba8)
            .ok_or_else(|| internal("PNG 编码失败（尺寸与像素数不匹配）"))?;
        // **与裸像素出口共用同一个告警槽** ✓（两条出口对同一件事给同一个答案 ✓）。
        self.note_render_stats(&rendered.stats);
        let previous = self.cached_preview_hashes();
        let blob_hash = self.store.put(&png)?;
        self.render_watermark = self.log.head_seq();
        self.last_render_blob = Some(blob_hash.clone());
        // 只有覆盖整幅画布的渲染才是「文档级」预览（7.3），局部 dirty 渲染不算。
        if covers_canvas(region, self.state.width, self.state.height) {
            self.document_thumbnail = Some(blob_hash.clone());
            self.document_thumbnail_seq = self.render_watermark;
            // **刚渲染出来的整幅结果本身就是最好的缓存** ✓：把它记成 `full_frame_render` ✓
            // ⇒ 同一进程内的下一次整幅请求（以及落盘后的下一次打开 ✓）都能直接命中 ✓。
            self.full_frame_render = Some(blob_hash.clone());
            self.full_frame_render_seq = self.render_watermark;
        }
        self.complete_render_jobs()?;
        // **淘汰必须放在"指针都更新完"之后** ✓ —— 这条是测试逼出来的 ✗：
        // 我第一版把它放在更新 `last_render_blob` 的紧后面 ✓，而**整幅渲染还会更新文档缩略图** ✓
        // （就在下面几行 ✓）⇒ 淘汰时缩略图**仍指向旧 blob** ✗ ⇒ 守卫判成"还被引用" ✓ ⇒ 永远不淘汰 ✗
        //（测试实测：两次渲染后 CAS 里 1 → 2 份 ✗）。放到这里 ✓，旧 blob 才真的"无人引用" ✓。
        self.evict_replaced_previews(&previous);
        Ok(RenderedPreview {
            bbox: [
                rendered.bbox.x,
                rendered.bbox.y,
                rendered.bbox.w,
                rendered.bbox.h,
            ],
            width: rendered.width,
            height: rendered.height,
            blob_hash: blob_hash.clone(),
            url: preview_url(&blob_hash),
            mime_type: "image/png".to_owned(),
            bytes: png.len(),
            tiles: rendered.tiles.len(),
            thumb_kind: None,
            filter_padding: rendered.stats.filter_padding,
            warnings: rendered.stats.unsupported,
        })
    }

    /// **★ 单层缩略图预览 ✗ ★**（第 185 轮 ✓；**PWA 实测 P1-2 的后端一半 ✓**）。
    ///
    /// **∴ 为什么不复用 [`Self::render_region`] ✗ ★**：**那条路会**更新文档级状态**✗**
    /// （`last_render_blob` ✓／`document_thumbnail` ✓／`full_frame_render` ✓）
    /// ⇒ **∴ 于是**一张「**只有一层**」的图会被**当成**文档缩略图** ✗（**污染** ✓）
    ///   ⇒ **★ 所以**：**它**不是**「**一行级**」**的改动**✓（**∴ 与**上游补丁 README **的说法不同 ✓）**。
    ///
    /// **∴ 本方法只做四件事 ✗**：**隔离一层 ⇒ 渲染 ⇒ 编码 PNG ⇒ 存 CAS** ✓
    ///   ⇒ **∴ 而****不碰**任何**文档级指针** ✓（**∴ 因为**它是**附属信息** ✓）。
    ///
    /// **∴ 且**：**不调 `note_render_stats`** ✗ —— **∴ 因为**那个槽**承载的是**"**这一次请求**"**的账目**✗
    ///   （**`below_tiles_*` 等 ✓）⇒ **∴ 若**每张缩略图都去记一笔 ✗ ⇒
    ///     **∴ 就会**污染**那些**账目**✗ ⇒ **∴ 而**好几条判据**正**读它们 ✓**** ✓✓
    ///   ⇒ **∴ 替代**：**告警**直接**从 `stats.unsupported` **取 ✓（**∴ 不**经共享槽 ✓）** ✓✓
    pub fn render_region_layer_preview(
        &mut self,
        bbox: Bbox,
        layer_id: &str,
    ) -> Result<RenderedPreview> {
        let previous = self.renderer.set_only_layer(Some(layer_id.to_owned()));
        let rendered = self.renderer.render_region(&self.state, &*self.store, bbox);
        // **无论成功失败都要还原** ✓（否则这个"只画一层"的开关会**泄漏到下一次渲染** ✗）。
        self.renderer.set_only_layer(previous);
        let rendered = rendered?;
        let png = encode_png(rendered.width, rendered.height, &rendered.rgba8)
            .ok_or_else(|| internal("PNG 编码失败（尺寸与像素数不匹配）"))?;
        let blob_hash = self.store.put(&png)?;
        Ok(RenderedPreview {
            bbox: [
                rendered.bbox.x,
                rendered.bbox.y,
                rendered.bbox.w,
                rendered.bbox.h,
            ],
            width: rendered.width,
            height: rendered.height,
            blob_hash: blob_hash.clone(),
            url: preview_url(&blob_hash),
            mime_type: "image/png".to_owned(),
            bytes: png.len(),
            tiles: rendered.tiles.len(),
            thumb_kind: None,
            filter_padding: rendered.stats.filter_padding,
            warnings: rendered.stats.unsupported,
        })
    }

    /// 提交收尾用的**文档级预览**：生成 256² 缩略图（小 PNG），并推进渲染水位。
    ///
    /// 与 [`Self::render_region`] 的区别：不落盘整幅 PNG。显式导出仍走 `render_region`。
    ///
    /// **冷启动复用** ✓（本专题）：`document_thumb` 可以由
    /// [`Self::restore_persisted_render`] **从磁盘 `render.png` 恢复** ✓ ⇒
    /// 打开后的第一次预览只需重渲染 `(持久化 seq, HEAD]` 的脏区 ✓，
    /// 而不是整幅 4K ✗（实测整幅 122s vs 增量区域 ≪ 1s ✓）。
    pub fn render_document_preview(&mut self) -> Result<RenderedPreview> {
        // **增量** ✓：只重渲染"自上次预览以来变过的文档区域"，并只刷新对应的缩略图块 ✓
        //（性能专题，第 1040 轮）。
        //
        // 旧实现每次都 `render_thumbnail(…, None)` ✗ —— 它为了得到一张 **256²** 的小图，
        // **先按整幅文档做全分辨率渲染**（4K ⇒ 830 万像素缓冲区 ＋ 逐像素拷贝 ＋ 降采样）✗
        // ⇒ 实测单笔 1.5s~4.0s、单核 100%、且**随文档对象数线性增长** ✓。
        let doc_size = Bbox::new(0.0, 0.0, self.state.width as f64, self.state.height as f64);
        let dirty = self.preview_dirty;
        let region = match (self.document_thumb.is_some(), dirty) {
            // **已知没有变化** ✓ ⇒ 一个像素都不用重渲染 ✓（旧写法会在这里退回整幅 ✗）。
            (true, PreviewDirty::Clean) => None,
            // 有缓存像素、且知道变过哪里 ⇒ **只渲染那一块** ✓。
            // **外扩到整块** ✗：块只被脏区**部分**覆盖时，`update_blocks_from_region` 会把
            // 块内"脏区之外"的像素按边界 clamp 采样 ✗ ⇒ 缩略图与整幅重算**不一致** ✓。
            // 外扩后每个被刷新的块都被完整渲染覆盖 ✓ ⇒ **逐像素一致** ✓（判据有逐字节比对 ✓）。
            (true, PreviewDirty::Region(bbox)) => {
                let dirty = clamp_to_document(bbox, doc_size);
                Some(expand_to_thumb_blocks(
                    self.document_thumb.as_ref().expect("上面判过 Some"),
                    dirty,
                    doc_size,
                ))
            }
            // 冷启动，或"不知道变过哪里" ⇒ 整幅 ✓（**与旧行为一致** ✓，不会更差 ✓）。
            _ => Some(doc_size),
        };
        let render = match region {
            Some(region) => {
                // **冷启动的空白文档不必真的光栅化** ✓（第 273 轮，方案 d ✓）。
                //
                // **为什么** ✗：冷启动那一条会渲染**整幅**（`Some(doc_size)` ✓）。
                // 而**空文档**（`state.objects` 为空 ✓）＋**不透明背景**时，
                // 整幅渲染的结果**就是纯背景填色** ✓ —— `render.rs:542`：
                //   `let background = self.options.background.or_else(|| parse_background(&state.background));`
                // ⇒ 背景是**一个 RGBA** ✓ ⇒ 830 万像素的光栅化只为了得到一种颜色 ✗。
                // **实测**：空白 4K 新文档第一次 `get_document` 要 **808 ms** ✓（`preview_ms=787.83` ✓）。
                //
                // **为什么这样改** ✓：**不去碰**缩略图的块平均逻辑 ✗ ——
                // 而是给它**同一份输入**：构造一份**均匀**的 f32 缓冲（就是空白文档渲染后
                // `linear_premul_from_u8x4(&render.rgba8)` 会得到的东西 ✓）
                // ⇒ 平均那一步**逐位相同** ✓ ⇒ 输出**逐字节相同** ✓。
                // 背景**不透明**（alpha=255 ✓）时往返精确 ✓：仓库已有测试
                // `color.rs:283 u8_round_trip_is_exact_for_opaque_pixels` ✓。
                //
                // **代价两面** ✓：收益是省掉整幅光栅化 ✓（仍要分配并填一份区域大小的 f32 缓冲 ✗，
                // 4K 约 133 MB ✓ ⇒ 比光栅化便宜一个量级 ✓）；代价是**只覆盖"空文档 ＋ 不透明背景"** ✓
                // ⇒ 有内容的路径**行为不变** ✓。
                //
                // **计数器不动** ✓：`document_preview_renders` / `full_canvas_renders` 是
                // "这条路真的走了"的凭证 ✓ ⇒ 这里**没有**走 `render_region` ⇒ 都不 +1 ✓
                //（实测 `scripts/*.mjs` 里没有判据读它们 ✓ ⇒ 不会假红 ✓）。
                let blank_fill = if self.state.objects.is_empty() {
                    yanshi_render::render::parse_background(&self.state.background)
                        .filter(|rgba| rgba[3] == 255)
                } else {
                    None
                };
                match blank_fill {
                    Some(rgba) => {
                        let width = region.w.max(0.0).ceil() as u32;
                        let height = region.h.max(0.0).ceil() as u32;
                        // **均匀的 u8 RGBA**（不是 f32 ✓）：交给**下面原有的**
                        // `linear_premul_from_u8x4(&render.rgba8)` 去转换 ✓
                        // ⇒ 与"真的渲染一遍空白文档"得到的输入**逐字节相同** ✓✓。
                        let mut rgba8: Vec<u8> = Vec::with_capacity((width * height) as usize * 4);
                        for _ in 0..(width * height) as usize {
                            rgba8.extend_from_slice(&rgba);
                        }
                        // **告警照旧留空** ✓：没渲染 ⇒ 没有"不支持的特性"要报 ✓。
                        self.last_render_warnings.clear();
                        Some(RegionRender {
                            bbox: region,
                            width,
                            height,
                            rgba8,
                            tiles: Vec::new(),
                            stats: RenderStats::default(),
                        })
                    }
                    None => {
                        let rendered =
                            self.renderer
                                .render_region(&self.state, &*self.store, region)?;
                        // **"这条路真的走了"** ✓（判据用它 ✓，与区域大小、与墙钟都无关 ✓）。
                        self.document_preview_renders += 1;
                        if covers_canvas(region, self.state.width, self.state.height) {
                            self.full_canvas_renders += 1;
                        }
                        // **这条出口也必须留下告警** ✗（真实事故复盘暴露的缺口 ✓）：预览是
                        // "打开文档 / 提交一笔之后用户看到的那张图" ✓ —— 缺一个补丁时它以前
                        // **既不进 `last_render_warnings`、也不进返回的 `warnings`** ✗
                        // ⇒ 用户看到的是一张**静默的不完整画面** ✓，与"数据丢了"长得一模一样 ✗
                        //（`render_region` 与 `export_png` 两条出口早已带告警 ✓，只有这条路漏了 ✓）。
                        self.note_render_stats(&rendered.stats);
                        Some(rendered)
                    }
                }
            }
            None => None,
        };
        // 只刷新与 `region` 相交的块 ✓；其余块**保留上一版像素** ✓。
        let (thumb_size, thumb_rgba) = {
            let thumb = self
                .document_thumb
                .get_or_insert_with(|| Thumb::new(ThumbKind::Doc256));
            if let (Some(region), Some(render)) = (region, render.as_ref()) {
                let buffer = Buffer::from_f32(
                    0,
                    0,
                    render.width,
                    render.height,
                    &linear_premul_from_u8x4(&render.rgba8),
                )
                .ok_or_else(|| internal("预览缓冲区尺寸与像素数不匹配"))?;
                let blocks = thumb.dirty_blocks_for(&region, doc_size);
                thumb.update_blocks_from_region(&buffer, region, doc_size, &blocks);
            }
            (thumb.size, thumb.rgba8.clone())
        };
        // **渲染完必清污** ✓：不清的话下一次预览会把同一块再渲染一遍 ✗；
        // 而写回 `Unknown`/`None` 又会**误判成冷启动** ✗ ⇒ 下一笔直接整幅 ✗（旧实现的坑 ✓）。
        self.preview_dirty = PreviewDirty::Clean;
        let png = encode_png(thumb_size, thumb_size, &thumb_rgba)
            .ok_or_else(|| internal("PNG 编码失败（缩略图尺寸不匹配）"))?;
        let previous = self.cached_preview_hashes();
        // **缩略图是可重建的缓存** ✓（设计 `:572`✗ ⇒ **∴ 改为**按需产生 ✓）
        // ⇒ **∴ 所以用 `put_cache`✗（**∴ 跳过 `fsync` ✓）⇒ **∴ 而**权威数据（**∴ 原子日志 ✗／位图 ✓**）
        // 仍走 `put`✗ ⇒ **∴ `/health`✗ 的 `blob_fsync`✗ 语义不变 ✓**。
        let blob_hash = self.store.put_cache(&png)?;
        self.last_thumb_blob = Some(blob_hash.clone());
        self.document_thumbnail = Some(blob_hash.clone());
        self.render_watermark = self.log.head_seq();
        self.document_thumbnail_seq = self.render_watermark;
        self.last_render_blob = Some(blob_hash.clone());
        self.evict_replaced_previews(&previous);
        self.broadcaster.publish_thumbnail(ThumbKind::Doc256);
        self.complete_render_jobs()?;
        // 本次预览渲染跳过了什么 ✓（没有渲染（`region == None`）⇒ 空 ✓：
        // 那时没有新信息，`last_render_warnings` 保留上一次的值 ✓）。
        let render_warnings = render
            .as_ref()
            .map(|rendered| rendered.stats.unsupported.clone())
            .unwrap_or_default();
        Ok(RenderedPreview {
            bbox: [0.0, 0.0, self.state.width as f64, self.state.height as f64],
            width: thumb_size,
            height: thumb_size,
            blob_hash: blob_hash.clone(),
            url: preview_url(&blob_hash),
            mime_type: "image/png".to_owned(),
            bytes: png.len(),
            tiles: 0,
            thumb_kind: Some(ThumbKind::Doc256),
            filter_padding: 0,
            warnings: render_warnings,
        })
    }

    /// **与 HEAD 一致的持久化整幅渲染** ✓（若存在）—— 可直接当成一次整幅区域渲染的结果 ✓。
    ///
    /// 三条准入条件缺一不可 ✓（任何一条不满足都必须退回真渲染 ✗）：
    /// 1. 有整幅缓存 ✓（`render.png` 的尺寸必须**恰好等于画布** ✗ —— 256² 缩略图不算 ✓）；
    /// 2. `full_frame_render_seq == HEAD` ✓（**落后一个 seq 都不许用** ✗ —— 那是拿旧图冒充新图 ✗）；
    /// 3. blob 仍在 CAS 里 ✓（预览是缓存 ✓，可能已被淘汰 ✓）。
    fn current_full_frame_preview(&mut self) -> Result<Option<RenderedPreview>> {
        let head = self.log.head_seq();
        let Some(blob) = self.full_frame_render.clone() else {
            return Ok(None);
        };
        if self.full_frame_render_seq != head {
            return Ok(None);
        }
        let Ok(png) = self.store.get(&blob) else {
            return Ok(None);
        };
        self.render_watermark = head;
        self.last_render_blob = Some(blob.clone());
        // **★ 记一次整幅缓存命中 ✗ ★**（∴ 这是**用户实际走的快路径**✓）
        self.full_frame_hits += 1;
        self.document_thumbnail = Some(blob.clone());
        self.document_thumbnail_seq = head;
        Ok(Some(RenderedPreview {
            bbox: [0.0, 0.0, self.state.width as f64, self.state.height as f64],
            width: self.state.width,
            height: self.state.height,
            blob_hash: blob.clone(),
            url: preview_url(&blob),
            mime_type: "image/png".to_owned(),
            bytes: png.len(),
            tiles: 0,
            thumb_kind: None,
            filter_padding: 0,
            warnings: Vec::new(),
        }))
    }

    /// **恢复"持久化的整幅渲染"** ✓（服务端打开文档时调用；14.5「打开即图片」✓）。
    ///
    /// 旧实现（`mark_rendered`）只恢复了**一个 blob 指针** ✗ ⇒ 缩略图序列号落在旧 seq 上 ✓
    /// ⇒ `document_thumbnail_is_current()` 为假 ✓ ⇒ 新连接**又把整幅渲染一遍** ✗。
    ///
    /// 这里多记一件事 ✓：**如果那张图就是整幅画布** ✓（从 PNG 头读尺寸 ✓，**不解码像素** ✗），
    /// 就把它记成"整幅区域渲染的缓存" ✓ ⇒ 与 HEAD 一致时可直接命中 ✓（见
    /// [`Self::current_full_frame_preview`] ✓）。
    ///
    /// **为什么不顺手解码成预览基座** ✗：实测这份 4K 图 `decode_png` **153.7s**
    /// ＋ u8→线性 26.4s ＝ **183.6s**（debug ✓），比整幅重渲染（122s）**还贵** ✗
    /// ⇒ 拿它当预览基座是**负优化** ✗。预览基座改由**小的** `preview.png`（256² ✓）负责 ✓，
    /// 那张解码只要 ~1s ✓ —— 见 [`Self::restore_persisted_preview`] ✓。
    ///
    /// 返回是否把它认成了"整幅渲染缓存" ✓。
    pub fn restore_persisted_render(&mut self, seq: Seq, blob: BlobHash, png: &[u8]) -> bool {
        // **水位不许越过 HEAD** ✗（文件可能比日志新：先渲染后回滚 ✓）。
        let seq = seq.min(self.log.head_seq());
        self.render_watermark = seq;
        self.document_thumbnail = Some(blob.clone());
        self.last_render_blob = Some(blob.clone());
        self.document_thumbnail_seq = seq;
        self.full_frame_render = None;
        self.full_frame_render_seq = 0;
        let Some((width, height)) = png_dimensions(png) else {
            return false;
        };
        if (width, height) != (self.state.width, self.state.height) {
            // 256²/64²/图层缩略图…**不是**整幅 ✗ ⇒ 绝不能拿去回答"给我整幅" ✗。
            return false;
        }
        self.full_frame_render = Some(blob);
        self.full_frame_render_seq = seq;
        true
    }

    /// **恢复"持久化的文档预览"** ✓（小的 256² 图；预览像素基座的**唯一**来源 ✓）。
    ///
    /// 它把"上次预览时的像素"接回来 ✓，并算出 `(seq, HEAD]` 的脏区 ✓ ⇒
    /// 这次预览只重渲染**那一块** ✓，而不是整幅 ✗。
    ///
    /// **只在"基座 + 脏区 = HEAD"可证时接受** ✗：尺寸不是 256²、解不出像素、
    /// 或脏区不确定 ⇒ 返回 `false` ✓ 并保持 `document_thumb = None` ✓（退回整幅 ✓，正确性优先 ✓）。
    ///
    /// **为什么必须是小图** ✗：整幅 4K 的解码实测 153.7s（见上 ✓）⇒ 复用它是负优化 ✓；
    /// 256² 的解码 ~1s ✓ ⇒ 这一条是"复用"能否成立的分水岭 ✓。
    pub fn restore_persisted_preview(&mut self, seq: Seq, blob: BlobHash, png: &[u8]) -> bool {
        let seq = seq.min(self.log.head_seq());
        let Some((width, height)) = png_dimensions(png) else {
            return false;
        };
        if width != height || width != ThumbKind::Doc256.size() {
            return false;
        }
        let Some((_, _, rgba8)) = yanshi_render::png::decode_png(png) else {
            return false;
        };
        if rgba8.len() != (width * width * 4) as usize {
            return false;
        }
        let mut thumb = Thumb::new(ThumbKind::Doc256);
        thumb.rgba8.copy_from_slice(&rgba8);
        self.document_thumb = Some(thumb);
        // 预览基座是**更新的**那一份 ⇒ 它才是"当前文档预览" ✓。
        if seq >= self.document_thumbnail_seq {
            self.document_thumbnail = Some(blob.clone());
            self.last_render_blob = Some(blob);
            self.document_thumbnail_seq = seq;
            self.render_watermark = seq;
        }
        // **脏区必须可证** ✗：算不出来就整幅（`Unknown`）✓。
        self.preview_dirty = self.dirty_since(seq);
        true
    }

    /// `(from, HEAD]` 的累积脏区 ✓（冷启动复用：持久化基座 seq → HEAD ✓）。
    ///
    /// **逐原子**调用 `plan_dirty_with_log` ✓（与提交路径**同一套口径** ✓）——
    /// 它对 `revert`/`reapply` 会归约到目标原子的 dirty ✓，对"无几何线索"会保守返回整幅 ✓。
    /// 因此这里的结论与"逐笔提交累积出来的脏区"**逐字一致** ✓。
    fn dirty_since(&self, from: Seq) -> PreviewDirty {
        let head = self.log.head_seq();
        if from >= head {
            return PreviewDirty::Clean;
        }
        // **落后太多就直接"不确定"** ✓（第 1512 轮 ✓）—— `PreviewDirty::Unknown` 会吸收一切 ✓
        // ⇒ 语义是"**不确定就整幅**" ✓ ⇒ 这是作者**已经设计好**的保守出口 ✗，我没有新编语义 ✓。
        //
        // **为什么必须加这个上限** ✗：下面那个循环**逐原子折一次** ✗，而每次折叠都要
        // `remember` **深拷贝一份完整状态** ✗（状态随对象增长 ✓）⇒ 实测真实 4K 文档
        //（15.1 MB／9742 原子）**187 秒** ✓，采样为每轮 5→33 ms（越往后越慢 ✓）。
        //
        // **上限怎么定** ✗：单步实测约 19 ms ✓；一张 256² 预览的全幅渲染按实现注释约 1 s ✓
        // ⇒ 扫描成本 ≲ 一次全幅渲染 ⇒ 取 64 步（约 1.2 s ✓）。
        // **代价** ✗：落后超过 64 原子时预览整幅重画 ✗ —— 但预览是 256² ✓，
        // 且 `Unknown` 本来就是"整幅"的合法结果 ✓ ⇒ **不降精度** ✓，只多画一张小图 ✓。
        if head.saturating_sub(from) > DIRTY_SCAN_LIMIT {
            return PreviewDirty::Unknown;
        }
        let mut folder = IncrementalFolder::new();
        let Ok(mut previous) = folder.fold(&self.log, from) else {
            return PreviewDirty::Unknown;
        };
        let mut dirty = PreviewDirty::Clean;
        // 采样：每 1/4 打一次绝对时间戳（第 1510 轮）。它区分"每轮都均匀地慢"与"某几轮很慢"。
        let trace = std::env::var_os("YANSHI_OPEN_TIMING").is_some();
        let total = head.saturating_sub(from) as usize;
        let mut seen = 0usize;
        for atom in self.log.range_exclusive_inclusive(from, head) {
            #[cfg(test)]
            DIRTY_SCAN_STEPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if trace {
                if total > 0
                    && (seen == 0
                        || seen == total / 4
                        || seen == total / 2
                        || seen == (total * 3) / 4
                        || seen + 1 == total)
                {
                    let epoch_ms = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0);
                    eprintln!("dirty_since epoch_ms={epoch_ms} i={seen} of={total}");
                }
                seen += 1;
            }
            let Ok(current) = folder.fold(&self.log, atom.seq) else {
                return PreviewDirty::Unknown;
            };
            dirty = dirty.merged(planned_preview_dirty(&plan_dirty_with_log(
                &current.state,
                Some(&previous.state),
                &self.log,
                atom,
            )));
            previous = current;
        }
        dirty
    }

    /// 真正**整幅**渲染画布的次数 ✓（判据用语义计数 ✓；复用持久化像素就该让它保持 0 ✓）。
    pub const fn full_canvas_render_count(&self) -> usize {
        self.full_canvas_renders
    }

    /// **缩略图渲染**的次数 ✓（第 69 轮 ✓）—— 结构性证据 ✓，与墙钟无关 ✓。
    ///
    /// **判据为什么需要它** ✗：保存点**无条件**调 `document.thumbnail(Doc256)` ✓，
    /// 而它会**自己整幅渲染一遍** ✗（实测 **+1241 ms**／4K ✓，8K 更大 ✓）
    /// ⇒ **∴ 它违反第 3 条**"缩略图按需生成" ✓。
    /// **∴ 回归判据要问的是**："**重复保存时，这一次渲染有没有再跑一遍**" ✓
    /// —— 而不是"花了多少毫秒" ✗。
    pub fn thumbnail_render_count(&self) -> usize {
        self.document_thumbnail_renders
    }

    /// **本次打开耗时（毫秒 ✓）** —— 见 `open_ms` 字段的说明 ✓。
    pub const fn open_ms(&self) -> u64 {
        self.open_ms
    }

    /// 由 `Workspace::open_document` 写入 ✓（**唯一写入点** ✓）。
    pub(crate) fn set_open_ms(&mut self, ms: u64) {
        self.open_ms = ms;
    }

    /// **文档级缩略图是否已经是最新** ✓（第 75 轮 ✓）—— 保存点据此**跳过**重复生成 ✓。
    ///
    /// **为什么需要它** ✗：保存点**无条件**调 `thumbnail(Doc256)` ✓，而它会**整幅渲染**
    /// 一遍 ✗ ⇒ 实测（真实 4K ✓）**`thumbnail_renders` 每次导出 +1** ✓（1 → 2 → 3 ✓，
    /// 而画面**零变化** ✗）⇒ 每次白付 **~1.07 s** ✗（8K ~9 s ✗）。
    /// **∴ 判据** ✓："**无变化时重复导出 ⇒ `thumbnail_renders` 不得增长**" ✓（**今天红 ✓**）；
    /// 配"**有变化之后必须更新**" ✓（否则可以用"永不更新"骗过前一条 ✓）。
    /// **仅供探针** ✓（第 86 轮 ✓）：读出当前 `document_thumbnail` 的哈希 ⇒ 量它到底是整幅还是 256² ✓。
    /// **位图缓存的命中／未命中** ✓（第 93 轮 ✓，**纯观测** ✓ —— 不参与任何渲染决策 ✓）。
    ///
    /// **为什么需要它** ✗：整幅渲染的 **600 ms** 花在 720 个对象的 `raster` 上 ✓
    /// ⇒ **∴ 要判断"缓存是否已足够有效"** ⇒ 必须先看到**命中率** ✓
    ///（**∴ 不许先改** ✗：若命中率已高 ⇒ 那 600 ms 是**对象光栅化的固有成本** ✓，改缓存无益 ✓）。
    pub fn bitmap_cache_hits_misses(&self) -> (usize, usize) {
        let stats = self.renderer.bitmap_cache_stats();
        (stats.hits, stats.misses)
    }

    /// **位图缓存未命中时解出的明文字节之和** ✓（第 133 轮 ✓，**纯观测** ✓）。
    /// **为什么它比"次数"有用** ✓：**一次整幅 4K 背景 ＝ 33.2 MiB** ✓，**一枚小补丁几十 KiB** ✓
    /// ⇒ **∴ 判"小区域有没有整幅解码"要看**字节**，不是次数** ✓✓。
    pub fn bitmap_cache_missed_bytes(&self) -> u64 {
        self.renderer.bitmap_cache_stats().missed_bytes
    }

    /// **仅供探针** ✓（第 86 轮 ✓）：读出当前 `document_thumbnail` 的哈希 ⇒ 量它到底是整幅还是 256² ✓。
    /// （修复记录：插入位图统计时把本函数的注释"抢"走了 ✗ ⇒ `missing-docs` 报错 ✓ ⇒ 已补 ✓。）
    /// **below 复用次数** ✓（第 95 轮 ✓，纯观测 ✓）：判"这一笔有没有复用下方的合成" ✓。
    pub fn below_reuse_count(&self) -> usize {
        self.renderer.below_reuse_count()
    }

    /// **below 缓存累计缺了几格**（第 342 轮，纯观测）：只读转发。
    pub fn below_missing_count(&self) -> usize {
        self.renderer.below_missing_count()
    }

    /// **below 缓存累计想要几格**（第 342 轮，纯观测）：只读转发。
    pub fn below_wanted_count(&self) -> usize {
        self.renderer.below_wanted_count()
    }

    /// **★ `above` 复用次数 ✓ ★**（第 702 轮 ✓，**纯观测 ✓**）：**∴ 与 `below` 同一条链 ✓**
    /// ⇒ **∴ 判据据此断言"半透明层的上方合成被复用了"✗**（**目标第 4 条 ✓**）。
    pub fn above_reuse_count(&self) -> usize {
        self.renderer.above_reuse_count()
    }

    /// **（第 95 轮补）** 本项在插入 below 计数时被"抢走"了注释 ✗ ⇒ 按其作用补回 ✓。
    /// ⚠️ 规律（本会话第 7 次 ✗）：**在某一项之前插入 ⇒ 会挪走它的文档注释** ✓（`-D missing-docs` 每次都能抓住 ✓）。
    pub fn doc_thumbnail_hash_for_probe(&self) -> Option<yanshi_core::BlobHash> {
        self.document_thumbnail.clone()
    }

    /// **文档级缩略图是否已经是最新** ✓（保存点据此**跳过**重复生成 ✓）。
    /// （插入探针时这行注释被我"抢"给了新函数 ✗ ⇒ clippy 的 `missing-docs` 当场报错 ✓ —— 已补 ✓。）
    pub fn doc_thumbnail_is_current(&self) -> bool {
        self.document_thumbnail.is_some() && self.document_thumbnail_seq == self.render_watermark
    }

    /// **文档级预览渲染**的次数 ✓（`render_document_preview` 真正渲染像素的次数 ✓；
    /// 判据用语义计数 ✓，不看墙钟 ✗）。
    ///
    /// **判据为什么需要它** ✗：外部 4K/8K 报告的根因陈述是"提交收尾同步重算了一遍
    /// 文档级预览（只为一张 256² 缩略图）" ✓ ⇒ 回归判据要问的是
    /// **"这一笔有没有跑那次渲染"** ✓ —— 而不是"花了多少毫秒" ✗
    ///（debug 下同一段能差 3-5 倍 ✓，见 `tests/background_stroke_cost.rs` 的记录 ✓）。
    ///
    /// 与 [`Self::full_canvas_render_count`] 的分工 ✓：那一个只数**整幅** ✓
    ///（冷启动 / 复用持久化像素专题用它 ✓），暖文档上的块渲染它数不到 ✗；
    /// 这一个数**这条路走没走** ✓，与区域大小无关 ✓。
    pub const fn document_preview_render_count(&self) -> usize {
        self.document_preview_renders
    }

    /// 日志被**全量重放**的次数 ✓（打开一次 + 每次"增量前推被拒"的提交 ✓）。
    pub fn full_fold_count(&self) -> usize {
        self.folder.full_steps()
    }

    /// 持久化整幅渲染是否与 HEAD 一致 ✓（判据用）。
    pub fn full_frame_render_is_current(&self) -> bool {
        self.full_frame_render.is_some() && self.full_frame_render_seq == self.log.head_seq()
    }

    /// 生成缩略图并输出 PNG 到 CAS（7 章）。
    /// **快路径** ✓（第 87 轮 ✓）：若槽里那份图**远大于**目标尺寸（实测 4K 整幅 PNG ＝ **800 KiB** ✓），
    /// 直接把它**降采样**成目标缩略图 ✓ ⇒ **省掉整幅重渲** ✗。
    ///
    /// **为什么划算** ✗（实测为据 ✓）：整幅渲染 4K ＝ **1 821 ms** ✗（图层 600 ＋ 合成 531 ＋ 量化 414 ＋ 裁剪 182 ✓）；
    /// 而本路径 ＝ **解码 PN​G ＋ 830 万次逐像素转换（＋81 ms ✓）＋ `update_full` 缩放（＋34 ms ✓）** ✓
    /// （与 `render_thumbnail` 走**同一个** `update_full` ✓ ⇒ **∴ 像素来源同、缩放同 ⇒ 无画质取舍** ✓）。
    ///
    /// **∴ 只在"真的更大"时才走** ✓（否则原路更省 ✓）；**槽为空 ⇒ 返回 `None`** ✓
    ///（第 1 次导出就是这种 ✓ ⇒ **仍走原路渲染 ✓，绝不跳过** ✗ —— **否则缩略图缺失 ✓**）。
    fn fast_document_thumb(&self, kind: ThumbKind, target: Option<Bbox>) -> Option<Thumb> {
        if target.is_some() || !kind.is_document_level() {
            return None;
        }
        // **三段探针** ✓（第 89 轮 ✓，宿主侧 ⇒ 允许 `Instant` ✓）：
        // 快路径 ~310 ms 到底是 **decode** ✗、**830 万次转换** ✗，还是 **缩放** ✗
        // —— **∴ 若 decode 占大头 ⇒ "抽稀"无用** ✓（必须先量 ✓）。
        let _ft0 = std::time::Instant::now();
        let _ftmark = |tag: &str| {
            if std::env::var_os("YANSHI_TRACE_FASTTHUMB").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/tmp/yanshi-fastthumb-trace.log")
                    .and_then(|mut f| {
                        use std::io::Write;
                        writeln!(f, "FASTTHUMB {tag} {} ms", _ft0.elapsed().as_millis())
                    });
            }
        };
        let hash = self.document_thumbnail.clone()?;
        let bytes = self.store.get(&hash).ok()?;
        _ftmark("store_get");
        let (w, h, rgba8) = yanshi_render::png::decode_png(&bytes)?;
        _ftmark("decode_png");
        let side = kind.size();
        // **小于"目标的两倍"就不值得** ✓（原路更省 ✓，且避免无意义的重编码 ✓）。
        if w <= side * 2 || h <= side * 2 {
            return None;
        }
        let mut buffer = Buffer::new(0, 0, w, h);
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                if i + 3 >= rgba8.len() {
                    return None;
                }
                buffer.set_pixel(
                    x,
                    y,
                    yanshi_render::color::u8x4_to_linear_premul([
                        rgba8[i],
                        rgba8[i + 1],
                        rgba8[i + 2],
                        rgba8[i + 3],
                    ]),
                );
            }
        }
        _ftmark("pixel_convert");
        let mut thumb = Thumb::new(kind);
        thumb.update_full(&buffer, Bbox::new(0.0, 0.0, w as f64, h as f64));
        _ftmark("update_full");
        Some(thumb)
    }

    /// **生成（或刷新）指定种类的缩略图** ✓，并把结果登记进文档 ✓。
    ///
    /// **第 87 轮起** ✓：若是**文档级**且**没有指定 target** ✓，会先试"**由已有整幅图降采样**"的快路径 ✓
    /// （省掉整幅重渲 ✗）；**快路径不适用时走原路渲染** ✓（**绝不跳过** ✗）。
    /// （修复记录：插入助手时把本函数的注释"抢"走了 ✗ ⇒ `missing-docs` 报错 ✓ ⇒ 已补 ✓。）
    pub fn thumbnail(&mut self, kind: ThumbKind, target: Option<Bbox>) -> Result<RenderedPreview> {
        // **按需生成的计数** ✓（第 66 轮 ✓）：走到这里就说明**真的做了一次渲染** ✓。
        self.document_thumbnail_renders += 1;
        // **先试快路径** ✓（复用已有整幅图 ✓），**失败就走原路** ✓ —— 尾部逻辑**完全不变** ✓。
        let thumb: Thumb = match self.fast_document_thumb(kind, target) {
            Some(thumb) => thumb,
            None => render_thumbnail(&mut self.renderer, &self.state, &*self.store, kind, target)?,
        };
        // 整幅重建 ⇒ 增量用的缓存像素已经过期 ✓（第 1040 轮）。
        if target.is_none() && kind.is_document_level() {
            self.document_thumb = None;
        }
        let png = encode_png(thumb.size, thumb.size, &thumb.rgba8)
            .ok_or_else(|| internal("PNG 编码失败（缩略图尺寸不匹配）"))?;
        let previous = self.cached_preview_hashes();
        let blob_hash = self.store.put(&png)?;
        // **先更新所有指针、再淘汰** ✓（顺序错了就会删到还在用的那份 ✗ —— 见 `render_region` 的注释 ✓）。
        self.last_thumb_blob = Some(blob_hash.clone());
        if target.is_none() && kind.is_document_level() {
            self.document_thumbnail = Some(blob_hash.clone());
            self.document_thumbnail_seq = self.render_watermark;
        }
        self.evict_replaced_previews(&previous);
        self.broadcaster.publish_thumbnail(kind);
        Ok(RenderedPreview {
            bbox: target
                .map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h])
                .unwrap_or([0.0, 0.0, self.state.width as f64, self.state.height as f64]),
            width: thumb.size,
            height: thumb.size,
            blob_hash: blob_hash.clone(),
            url: preview_url(&blob_hash),
            mime_type: "image/png".to_owned(),
            bytes: png.len(),
            tiles: 0,
            thumb_kind: Some(kind),
            filter_padding: 0,
            warnings: Vec::new(),
        })
    }

    /// 渲染状态查询（6.7 `get_render_status`）。
    pub fn render_status(&self, atom_id: &str) -> Result<RenderStatus> {
        let atom = self.log.get(atom_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("原子 {atom_id} 不在日志中")),
            )
            .with_atom(atom_id.to_owned())
        })?;
        Ok(RenderStatus {
            rendered: atom.seq <= self.render_watermark,
            head_seq: self.log.head_seq(),
            rendered_seq: self.render_watermark,
            atom_seq: Some(atom.seq),
            thumb_url: self.document_thumbnail_url(),
        })
    }

    /// 最近一次渲染结果的地址（可能是局部 dirty 区域）。
    pub fn latest_preview_url(&self) -> Option<String> {
        self.last_render_blob.as_ref().map(preview_url)
    }

    /// 文档级缩略图地址（覆盖整幅画布，6.2「打开即图片」）。
    pub fn document_thumbnail_url(&self) -> Option<String> {
        self.document_thumbnail.as_ref().map(preview_url)
    }

    /// 文档级缩略图对应的 blob ✓（落盘预览缓存时用 ✓）。
    pub fn document_thumbnail_blob(&self) -> Option<&BlobHash> {
        self.document_thumbnail.as_ref()
    }

    /// 文档级缩略图是否与当前 HEAD 一致（落后就需要重新生成）。
    pub fn document_thumbnail_is_current(&self) -> bool {
        self.document_thumbnail.is_some() && self.document_thumbnail_seq == self.log.head_seq()
    }

    /// 文档级缩略图对应的 seq。
    pub const fn document_thumbnail_seq(&self) -> Seq {
        self.document_thumbnail_seq
    }

    /// 记录文档级缩略图（服务端从持久化渲染缓存恢复时使用）。
    pub fn set_document_thumbnail(&mut self, blob: BlobHash, seq: Seq) {
        self.document_thumbnail = Some(blob.clone());
        self.last_render_blob = Some(blob);
        self.document_thumbnail_seq = seq.min(self.log.head_seq());
    }

    /// **★ 标记「该文档需要生成预览」 ✓ ★**（第 586 轮 ✓；**异步预览设计第 ② 步 ✓**）。
    ///
    /// **做法** ✓：**在既有作业队列里排一个 `kind == "preview"` 的作业 ✗** ⇒
    /// **∴ 于是**下一次**提交收尾**的 [`Self::run_pending_jobs`]（**它只要 `pending()` 非空
    /// 就会调 `render_document_preview()` ✓**）**就会把预览渲出来 ✓** —— **∴ 完全复用现有机质 ✓，
    /// 不新起一套 ✓**，**且**两条既有契约（**`completed == [job_id]` ✓；渲染水位随提交推进 ✓**）
    /// **不受影响 ✓**（**∴ 因为收尾那行渲染**原样保留 ✓**）。
    ///
    /// **∴ 去重 ✓**：**已有未完成的 `preview` 作业 ⇒ 直接返回 `false` ✓，不重复入队 ✓**
    /// （**∴ 避免同一文档排一堆作业 ✗**）。
    ///
    /// **⚠️ 何时调用** ✓：**只在**读路径**发现"首次需要缩略图"且**愿意等下一次提交**时 ✓**；
    /// **∴ 而**若调用方**只读不编辑**✗ ⇒ **∴ 提交收尾**永不到来 ✗** ⇒ **∴ 调用方必须**自带兜底 ✗
    /// （**如超过一小段时间后**就地同步生成 ✓**）** ⇒ **∴ 见设计文档第八节 ✓**。
    pub fn mark_preview_pending(&mut self, session: yanshi_core::ids::SessionId) -> bool {
        if self.jobs.pending().iter().any(|job| job.kind == "preview") {
            return false;
        }
        self.jobs.submit("preview", session, yanshi_core::now_ms());
        true
    }

    /// 执行待处理的重型渲染 job，返回完成的 job。
    pub fn run_pending_jobs(&mut self) -> Result<Vec<JobId>> {
        let pending = self.jobs.pending();
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        // 一次提交只生成**文档级预览（256²）**，不再写整幅 PNG。
        //
        // 设计决策 A（设计方确认）：提交只写 256² 文档预览，整幅 PNG 仅在**显式导出**
        // （客户端调用 `render_region`/导出工具）时生成。原因：此前每个提交都写一张整幅 PNG，
        // 实测一个工作区累积到 1.3GB / 245 个 PNG（平均约 5MB）。
        // 体验不受影响：画布像素由内核或客户端的显式 `render_region` 提供，
        // 缩略图本来就是 256² —— 客户端拿到的 `preview` 因此更小且足够。
        // **这里不再无条件重渲染整幅文档预览** ✗（性能专题，第 1037 轮）：
        //
        // 原先每次"有 pending job"（`brush_stroke` 等重型原子每次提交都会排一个 ✓）就
        // `render_document_preview()` ⇒ **在本次调用内把整幅文档重渲染一遍** ✗ ⇒
        // 实测 4K 下单笔 1.5s~4.0s、单核 100%，**且随文档对象数线性增长** ✓。
        //
        // 而"按需预览"这条路**本来就已经完备** ✓：`ensure_document_thumbnail`
        // （`service.rs:1895` ✓）会用 `document_thumbnail_is_current()`（比 `log.head_seq()` ✓）
        // 判断缓存是否落后，落后才重建 ✓；`tools.rs:2843/2894` 在需要预览时会调它 ✓。
        // ⇒ **∴ 于是"提交时预先渲染"是**多余的**✗ ⇒ 去掉它，预览改为**首次被请求时**生成 ✓**。
        //
        // 注意 ✓：`render_document_preview()` 仍然由**显式导出/预览请求**使用 ✓（那条路不变 ✓）；
        // 本函数下面的 job 状态推进（`for job in pending` ✓）保持不变 ✓。
        // **回退（第 1038 轮）**：把"job 完成"与"整幅渲染"解耦会破坏两条既有契约 ✗ ——
        // ① `heavy_atoms_create_jobs_and_render_completes_them`（`completed == [job_id]` ✓）
        //    之所以能通过，正是因为**渲染顺便调了 `complete_render_jobs()`** ✓；
        // ② `jobs_ttl_cancel_and_render_watermark` 还要求**渲染水位随提交推进** ✓。
        // ⇒ **∴ 正确方向不是"删掉渲染"✗，而是"让这次渲染**只做变更区域**"✓** ——
        //    见 `thumb.rs` 已有的**分块增量**机制（`incremental.dirty_blocks_for(…)` ✓）。
        // ⇒ 本轮先**恢复原行为**（不留红树 ✓）；增量预览作为下一轮的判据项 ✓。
        let _ = self.render_document_preview()?;
        let now = yanshi_core::now_ms();
        let mut completed = Vec::new();
        for job in pending {
            if let Ok(current) = self.jobs.get(&job.id, now) {
                if current.status == JobStatus::Committed {
                    completed.push(job.id.clone());
                }
            }
        }
        Ok(completed)
    }

    /// 标记「已渲染到某个 seq」（服务端重启后从持久化渲染缓存恢复时使用，14.5）。
    pub fn mark_rendered(&mut self, seq: Seq, preview_blob: Option<BlobHash>) {
        self.render_watermark = seq.min(self.log.head_seq());
        if let Some(blob) = preview_blob {
            // 持久化的渲染缓存是文档级预览（`Workspace` 只在大范围渲染时落盘）。
            self.document_thumbnail = Some(blob.clone());
            self.last_render_blob = Some(blob);
            self.document_thumbnail_seq = self.render_watermark;
        }
    }

    fn complete_render_jobs(&mut self) -> Result<()> {
        let pending = self.jobs.pending();
        for job in pending {
            let now = yanshi_core::now_ms();
            if let Ok(status) = self.jobs.complete(&job.id, "render", now) {
                self.broadcaster
                    .publish_job_finished(job.id.clone(), status.as_str());
            }
        }
        Ok(())
    }

    /// 立即生成快照（6.5）。
    pub fn snapshot_now(&mut self, now: i64) -> Result<String> {
        let head = self.log.head_seq();
        let base_ref = match self.log.last_declare_head_upto(head) {
            Some(declare_head) => SnapshotBase::DeclareHead(declare_head.id.clone()),
            None => SnapshotBase::Snapshot("root".to_owned()),
        };
        let mut cache = yanshi_core::StateAtCache::new();
        let snapshot: Snapshot = Snapshot::create(&self.log, head, base_ref, &mut cache)?;
        let id = snapshot.id.clone();
        self.snapshots.insert(snapshot);
        self.snapshots.pin_recent_declare_head_bases(&self.log, 2);
        self.last_snapshot_seq = head;
        self.last_snapshot_at = now;
        Ok(id)
    }

    /// 最近一次快照结论（可观测性 / 测试）。
    pub fn snapshot_decision(&self, now: i64) -> SnapshotDecision {
        yanshi_core::snapshot::snapshot_decision(
            &self.log,
            self.last_snapshot_seq,
            self.last_snapshot_at,
            now,
        )
    }

    /// 快照数量。
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// 发起 `declare_head`（时间旅行 / revert_to / restore_checkpoint，5.5）。
    pub fn declare_head_atom(
        &self,
        actor: &str,
        session: &str,
        base: HeadBase,
        reason: Option<&str>,
    ) -> Atom {
        FoldEngine::new().declare_head_atom(actor, session, base, reason)
    }

    /// 该文档的 GC 根集：**全日志引用闭包 ∪ 活跃 Manifest**（设计 6.3）。
    ///
    /// 工作区级 GC 需要把**所有文档**的根集并起来，否则会删掉别的文档引用的 blob。
    pub fn gc_roots(
        &self,
        now: i64,
    ) -> Result<std::collections::BTreeSet<yanshi_core::atom::BlobHash>> {
        Ok(self.plan_garbage(now)?.roots)
    }

    /// Blob GC 的**计划**（不删除任何东西）：用于干跑与容量报告。
    ///
    /// 设计 6.3：根集 = 全日志引用闭包 ∪ 活跃 Manifest；三级生命周期（活跃/历史/孤儿）；
    /// **只清理孤儿**（且需超过 TTL）。revert、时间旅行、Stash 引用的 blob 永不回收。
    pub fn plan_garbage(&self, now: i64) -> Result<GcPlan> {
        // 必须是**纯计划**：`run_gc` 会真的删除过期孤儿 ✗ —— 曾因此让「干跑」删了数据。
        let manifest = self.state.active_blob_manifest();
        plan_gc(
            &*self.store,
            &self.log,
            &manifest,
            // **钉住的 blob 也是 GC 的根** ✓（第 301 轮 ✓）—— 与上面的 evict 是**两条**独立路径 ✓，
            // 哪一条都不能删用户钉住的东西 ✓。
            &self.pinned_blobs,
            now,
            self.settings.orphan_ttl_seconds,
        )
    }

    /// Blob GC（6.3）：根集 = 全日志引用闭包 ∪ 活跃 Manifest。
    ///
    /// 返回 `(报告, 计划)` —— 计划里含活跃/历史/孤儿/可清理四类的**同一次扫描**结果，
    /// 调用方不必再扫一遍（此前工具层先 `plan_garbage` 再 `collect_garbage`，
    /// 两次扫描之间状态可能变化，导致报告与实际情况不一致）。
    pub fn collect_garbage(&self, now: i64) -> Result<(GcReport, GcPlan)> {
        let manifest = self.state.active_blob_manifest();
        let (report, plan) = run_gc(
            &*self.store,
            &self.log,
            &manifest,
            // **钉住的 blob 也是 GC 的根** ✓（第 301 轮 ✓）—— 与上面的 evict 是**两条**独立路径 ✓，
            // 哪一条都不能删用户钉住的东西 ✓。
            &self.pinned_blobs,
            now,
            self.settings.orphan_ttl_seconds,
        )?;
        Ok((report, plan))
    }

    /// **设置钉住的 blob** ✓（第 301 轮 ✓）：服务端在偏好变化时调用 ✓。
    ///
    /// 参考图是**工作区级**的 ✓ ⇒ 每个文档都要记 ✓（否则"另一个文档触发回收"也会删掉它 ✗）。
    pub fn set_pinned_blobs(&mut self, pinned: std::collections::BTreeSet<yanshi_core::BlobHash>) {
        self.pinned_blobs = pinned;
    }

    /// 提交时的 dirty 规划（供测试与工具层查询）。
    pub fn plan_dirty_for(&self, previous: Option<&DocumentState>, atom: &Atom) -> DirtySet {
        plan_dirty_with_log(&self.state, previous, &self.log, atom)
    }

    /// 折叠结果（HEAD）。
    pub fn fold_head(&self) -> Result<FoldResult> {
        FoldEngine::new()
            .fold(&self.log)
            .map(|state_at| FoldResult {
                state: state_at.state,
                warnings: state_at.warnings,
                suppressed: state_at.suppressed,
                applied: Vec::new(),
            })
    }
}

/// 渲染区域是否覆盖整幅画布（用于判定「文档级」预览）。
fn covers_canvas(region: Bbox, width: u32, height: u32) -> bool {
    region.x <= 0.0 && region.y <= 0.0 && region.w >= width as f64 && region.h >= height as f64
}

fn needs_previous_state(kind: AtomKind) -> bool {
    matches!(
        kind,
        AtomKind::Tombstone
            | AtomKind::Supersede
            | AtomKind::Move
            | AtomKind::Transform
            | AtomKind::SetProperty
            | AtomKind::Revert
            | AtomKind::Reapply
            | AtomKind::ReorderLayers
    )
}

fn dirty_kind_str(dirty: &DirtySet) -> &'static str {
    dirty.kind.as_str()
}

/// 两个文档区域的**并集** ✓（累积脏区用，第 1040 轮）。
fn union_bounds(a: Bbox, b: Bbox) -> Bbox {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = (a.x + a.w).max(b.x + b.w);
    let y1 = (a.y + a.h).max(b.y + b.h);
    Bbox::new(x0, y0, x1 - x0, y1 - y0)
}

/// 把 `bbox` 裁剪到文档范围内 ✓；裁剪后为空 ⇒ 退回整幅 ✓（第 1040 轮）。
fn clamp_to_document(bbox: Bbox, doc: Bbox) -> Bbox {
    let x0 = bbox.x.max(doc.x);
    let y0 = bbox.y.max(doc.y);
    let x1 = (bbox.x + bbox.w).min(doc.x + doc.w);
    let y1 = (bbox.y + bbox.h).min(doc.y + doc.h);
    if x1 <= x0 || y1 <= y0 {
        doc
    } else {
        Bbox::new(x0, y0, x1 - x0, y1 - y0)
    }
}

/// **把脏区外扩到它触及的那些缩略图块** ✓（冷启动复用专题）。
///
/// **为什么必须外扩** ✗：`Thumb::update_blocks_from_region` 会把块内的**每一个缩略图像素**
/// 映射回文档坐标再采样 ✓ ⇒ 若 `source_doc` 只是**部分的**脏区 ✓，块里"脏区之外"的采样点
/// 会被 `clamp` 到边界像素 ✗ ⇒ 刷出来的缩略图与"整幅重算"**不一致** ✓（边界上差几个像素 ✓）。
/// 外扩之后 ✓，每个被刷新的块的文档范围**完整落在**渲染区域内 ✓ ⇒ 采样点全部落在缓冲区内 ✓
/// ⇒ 与整幅重算**逐像素一致** ✓（判据里有逐字节比对 ✓）。
fn expand_to_thumb_blocks(thumb: &Thumb, dirty: Bbox, doc: Bbox) -> Bbox {
    let content = thumb.content_box(doc);
    if content.w <= 0.0 || content.h <= 0.0 {
        return doc;
    }
    let blocks = thumb.dirty_blocks_for(&dirty, doc);
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (block_x, block_y) in blocks {
        let bounds = thumb.block_bounds(block_x, block_y);
        let u0 = (bounds.x - content.x) / content.w;
        let u1 = (bounds.x + bounds.w - content.x) / content.w;
        let v0 = (bounds.y - content.y) / content.h;
        let v1 = (bounds.y + bounds.h - content.y) / content.h;
        x0 = x0.min(doc.x + u0 * doc.w);
        y0 = y0.min(doc.y + v0 * doc.h);
        x1 = x1.max(doc.x + u1 * doc.w);
        y1 = y1.max(doc.y + v1 * doc.h);
    }
    if !x0.is_finite() || !y0.is_finite() || !x1.is_finite() || !y1.is_finite() {
        return doc;
    }
    let (x0, y0) = (x0.floor(), y0.floor());
    let expanded = Bbox::new(x0, y0, (x1.ceil() - x0).max(1.0), (y1.ceil() - y0).max(1.0));
    clamp_to_document(expanded, doc)
}

/// u8（显示空间、直通 alpha）→ f32（线性、预乘）✓ —— 与渲染路径 `u8x4_to_linear_premul` **同一口径** ✓。
///
/// 单独抽出来是为了**一次转换整块像素** ✓：旧的逐像素 `set_pixel` 在 4K 上是 830 万次带边界
/// 检查的调用 ✗（实测 u8→线性这一步在 4K 上 **26.4s**，debug ✓）。
fn linear_premul_from_u8x4(rgba8: &[u8]) -> Vec<f32> {
    let mut pixels = Vec::with_capacity(rgba8.len());
    for pixel in rgba8.chunks_exact(4) {
        pixels.extend_from_slice(&yanshi_render::color::u8x4_to_linear_premul([
            pixel[0], pixel[1], pixel[2], pixel[3],
        ]));
    }
    pixels
}

/// 只读 PNG 头里的宽高 ✓（**不解码像素** ✗）。
///
/// 为什么要这一步 ✗：磁盘上的 `render.png` **可能是 256² 缩略图，也可能是整幅 4K** ✓，
/// 而"能不能拿它回答整幅请求"完全取决于这个尺寸 ✓。整幅解码要 **153.7s**（debug ✓）
/// ⇒ 判断尺寸**绝不能**顺带解码 ✗（PNG 的 IHDR 就在偏移 16 的 8 个字节 ✓）。
pub(crate) fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width == 0 || height == 0 {
        return None;
    }
    Some((width, height))
}

fn preview_url(hash: &BlobHash) -> String {
    format!("yanshi://blob/{}", hash.as_str())
}

fn internal(detail: &str) -> YanshiError {
    YanshiError::new(ErrorCode::ResourceExhausted, ErrorContext::detail(detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yanshi_core::blob::MemoryBlobStore;

    fn document() -> Document {
        let store = Arc::new(MemoryBlobStore::new());
        Document::create(
            store,
            NewDocument::new("doc_1", 64, 64),
            "human:1",
            "session:a",
            DocumentSettings::default(),
        )
        .unwrap()
    }

    fn drawing_atoms() -> Vec<Atom> {
        vec![
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "session:a",
                json!({"layer_id": "layer_1"}),
            ),
            Atom::new(
                AtomKind::DrawStroke,
                "human:1",
                "session:a",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "data": {"points": [[8.0, 8.0], [40.0, 24.0]], "size": 5.0, "color": [0.0, 0.0, 0.0, 1.0]},
                }),
            ),
        ]
    }

    /// 预览落后超过上限 ⇒ `dirty_since` 必须直接返回 `Unknown`（第 191 轮）。
    ///
    /// 判据打在被判条件本身：删掉 `dirty_since` 里用 `DIRTY_SCAN_LIMIT` 的那个 `if`
    /// ⇒ 这个测试红（它会走满扫描，返回的不是 `Unknown`）。
    /// 后半段是反例方向：落后为 0 ⇒ 必须是 `Clean`，防止判据退化成"永远 Unknown"。
    #[test]
    fn dirty_since_returns_unknown_when_preview_is_far_behind() {
        let mut document = document();
        // 先提交辅助函数给的两颗原子。它们里面有 CreateLayer，图层必须先存在。
        for atom in drawing_atoms() {
            document.commit(atom).expect("建图层与首笔应当成功");
        }
        // 再提交足够多的唯一笔画，使头超出上限。每颗 payload 都不同，避免被去重。
        for i in 0..(DIRTY_SCAN_LIMIT + 8) {
            let atom = Atom::new(
                AtomKind::DrawStroke,
                "human:1",
                "session:a",
                json!({
                    "object_id": format!("obj_extra_{i}"),
                    "layer_id": "layer_1",
                    "data": {"points": [[8.0, 8.0], [40.0, 24.0]], "size": 5.0,
                             "color": [0.0, 0.0, 0.0, 1.0]}
                }),
            );
            document.commit(atom).expect("后续笔画应当成功");
        }
        let head = document.log.head_seq();
        assert!(head > DIRTY_SCAN_LIMIT, "样本要够大：head={head}");

        // **主判据**：落后超过上限 ⇒ 必须走上限早退 ⇒ 扫描步数必须是 0。
        dirty_scan_steps_reset();
        let far = document.dirty_since(0);
        let far_steps = dirty_scan_steps();
        assert_eq!(
            far_steps, 0,
            "落后 {head} 步（上限 {DIRTY_SCAN_LIMIT}）⇒ 不许扫描任何一步"
        );
        assert!(
            matches!(far, PreviewDirty::Unknown),
            "早退必须给出 Unknown（整幅）"
        );

        // **对照**：落后在上限之内 ⇒ 必须真的扫（否则"永远早退"也能过）。
        dirty_scan_steps_reset();
        let near_from = head - (DIRTY_SCAN_LIMIT - 1);
        let _ = document.dirty_since(near_from);
        let near_steps = dirty_scan_steps();
        assert!(
            near_steps > 0 && near_steps <= DIRTY_SCAN_LIMIT as usize,
            "落后 {} 步（上限 {DIRTY_SCAN_LIMIT}）⇒ 应当扫 {near_steps} 步，且不超过上限",
            head - near_from
        );

        // 落后为 0 ⇒ 既不扫，也不是 Unknown。
        dirty_scan_steps_reset();
        assert!(
            matches!(document.dirty_since(head), PreviewDirty::Clean),
            "落后 0 ⇒ Clean"
        );
        assert_eq!(dirty_scan_steps(), 0, "落后 0 ⇒ 不扫");
    }

    #[test]
    fn create_commits_document_atom_and_sets_state() {
        let document = document();
        assert_eq!(document.head_seq(), 1);
        assert_eq!(document.state().width, 64);
        assert_eq!(document.atom_count(), 1);
        // 渲染是异步的（6.7）：提交后水位仍在 0，首次渲染才推进。
        assert_eq!(document.render_watermark(), 0);
    }

    #[test]
    fn commit_reports_dirty_tiles_and_attributes() {
        let mut document = document();
        for atom in drawing_atoms() {
            let result = document.commit(atom).unwrap();
            assert!(!result.duplicate);
            assert!(result.head_seq >= 2);
        }
        let result = document
            .commit(Atom::new(
                AtomKind::Supersede,
                "human:1",
                "session:a",
                json!({"object_id": "obj_1", "data": {"points": [[8.0, 8.0]], "size": 3.0}}),
            ))
            .unwrap();
        assert_eq!(result.dirty_kind, "geometry");
        assert!(result.dirty_bbox.is_some());
        assert!(!result.dirty_tiles.is_empty(), "修改必须失效 tile");
        assert!(result.job_id.is_none(), "普通原子不产生 job");
    }

    #[test]
    fn duplicate_atom_is_idempotent_and_not_rebroadcast() {
        let mut document = document();
        let atom = drawing_atoms().remove(0);
        let first = document.commit(atom.clone()).unwrap();
        let web = document.subscribe("session:web", PushChannel::WebSocket);
        let second = document.commit(atom).unwrap();
        assert!(second.duplicate);
        assert_eq!(second.seq, first.seq);
        assert_eq!(document.broadcaster().queued(web), 0, "重复提交不广播");
    }

    #[test]
    fn heavy_atoms_create_jobs_and_render_completes_them() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        // liquify 是重型原子（6.5）。
        let result = document
            .commit(Atom::new(
                AtomKind::Liquify,
                "human:1",
                "session:a",
                json!({"object_id": "obj_1", "region": {"x": 0, "y": 0, "w": 8, "h": 8}}),
            ))
            .unwrap();
        let job_id = result.job_id.expect("重型原子应产生 job");
        let job = document
            .jobs_mut()
            .get(&job_id, yanshi_core::now_ms())
            .unwrap();
        assert_eq!(job.status, JobStatus::Submitted);
        assert_eq!(job.kind, "liquify");

        let completed = document.run_pending_jobs().unwrap();
        assert_eq!(completed, vec![job_id.clone()]);
        let job = document
            .jobs_mut()
            .get(&job_id, yanshi_core::now_ms())
            .unwrap();
        assert_eq!(job.status, JobStatus::Committed);
    }

    #[test]
    fn render_region_stages_png_into_cas_and_updates_watermark() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let preview = document
            .render_region(Bbox::new(0.0, 0.0, 16.0, 16.0))
            .unwrap();
        assert_eq!((preview.width, preview.height), (16, 16));
        assert_eq!(preview.mime_type, "image/png");
        assert!(preview.bytes > 0);
        assert!(preview.url.starts_with("yanshi://blob/sha256:"));
        assert!(document.store().exists(&preview.blob_hash));
        let png = document.store().get(&preview.blob_hash).unwrap();
        assert_eq!(
            &png[0..8],
            &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
        );
        assert_eq!(document.render_watermark(), document.head_seq());

        let status = document
            .render_status(&preview_status_atom(&document))
            .unwrap();
        assert!(status.rendered);
    }

    fn preview_status_atom(document: &Document) -> String {
        document.log().head_atom().unwrap().id.clone()
    }

    #[test]
    fn render_status_reports_unrendered_atoms() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let head = document.log().head_atom().unwrap().id.clone();
        let status = document.render_status(&head).unwrap();
        assert!(!status.rendered, "提交后未渲染，水位落后");
        assert_eq!(status.rendered_seq, 0);

        document
            .render_region(Bbox::new(0.0, 0.0, 8.0, 8.0))
            .unwrap();
        let status = document.render_status(&head).unwrap();
        assert!(status.rendered);
        assert_eq!(status.rendered_seq, document.head_seq());
        assert!(document.render_status("atom_missing").is_err());
    }

    #[test]
    fn thumbnails_are_staged_and_broadcast() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let web = document.subscribe("session:web", PushChannel::WebSocket);
        let thumb = document.thumbnail(ThumbKind::Doc64, None).unwrap();
        assert_eq!((thumb.width, thumb.height), (64, 64));
        assert_eq!(thumb.thumb_kind, Some(ThumbKind::Doc64));
        assert!(document.store().exists(&thumb.blob_hash));
        assert_eq!(
            document.broadcaster().queued(web),
            1,
            "缩略图更新是数据流事件"
        );
    }

    #[test]
    fn declare_head_jumps_evaluation_origin_and_full_invalidates() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let layer_atom = document
            .log()
            .atoms()
            .iter()
            .find(|atom| atom.kind == AtomKind::CreateLayer)
            .unwrap()
            .id
            .clone();
        let declare = document.declare_head_atom(
            "human:1",
            "session:a",
            HeadBase::Atom(layer_atom),
            Some("revert_to"),
        );
        let result = document.commit(declare).unwrap();
        assert_eq!(result.dirty_kind, "full");
        assert_eq!(result.dirty_tiles.len(), 1, "64×64 文档只有一个 tile");
        // declare_head 自身的 seq 就是新的求值起点（5.5）。
        assert_eq!(document.state().eval_origin_seq(), result.seq);
    }

    #[test]
    fn reopen_from_log_reproduces_state() {
        let store = Arc::new(MemoryBlobStore::new());
        let mut document = Document::create(
            store.clone(),
            NewDocument::new("doc_1", 32, 32)
                .with_background(json!({"r": 0, "g": 0, "b": 0, "a": 255})),
            "human:1",
            "session:a",
            DocumentSettings::default(),
        )
        .unwrap();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let atoms = document.log().atoms().to_vec();
        let mut reopened =
            Document::open(store, "doc_1", atoms, DocumentSettings::default()).unwrap();
        assert_eq!(reopened.head_seq(), document.head_seq());
        assert_eq!(reopened.state(), document.state());
        // 打开文档不重放历史：状态来自日志的折叠结果，渲染水位由持久化缓存决定。
        // （`rebuild` 会把水位对齐到 head，表示服务端已有 HEAD 渲染缓存，14.5。）
        assert_eq!(reopened.render_watermark(), reopened.head_seq());
        reopened.mark_rendered(reopened.head_seq(), None);
        assert_eq!(reopened.render_watermark(), reopened.head_seq());
    }

    #[test]
    fn tokens_gate_remote_access_but_not_stdio() {
        let mut document = document();
        let token = document.issue_token("human:2", Role::Viewer);
        let principal = document
            .authorize(None, TransportKind::Stdio, "local:stdio")
            .unwrap();
        assert_eq!(principal.role, Role::Owner);
        assert!(document.authorize(None, TransportKind::Http, "x").is_err());
        let principal = document
            .authorize(Some(&token), TransportKind::Http, "x")
            .unwrap();
        assert_eq!(principal.actor, "human:2");
        assert!(!principal.role.can_edit());
    }

    #[test]
    fn blob_gc_keeps_history_after_revert() {
        let mut document = document();
        for atom in drawing_atoms() {
            document.commit(atom).unwrap();
        }
        let head_before = document.head_seq();
        let (report, plan) = document
            .collect_garbage(yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000)
            .unwrap();
        assert_eq!(report.deleted.len(), 0, "没有孤儿可清理");
        assert_eq!(plan.historical.len(), 0, "当前状态没有历史级 blob");
        assert_eq!(head_before, document.head_seq());
    }
}
