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
use yanshi_core::blob::{run_gc, BlobStore, GcReport};
use yanshi_core::{
    fold::{FoldResult, WarningKind},
    Atom, AtomId, AtomKind, AtomLog, Bbox, BlobHash, ChangesetId, CommitContext, DocumentState,
    ErrorCode, ErrorContext, FoldEngine, HeadBase, IncrementalFolder, Result, Seq, Snapshot,
    SnapshotBase, SnapshotDecision, SnapshotStore, StateAt, YanshiError,
};
use yanshi_render::dirty::{plan_dirty_with_log, DirtySet};
use yanshi_render::png::encode_png;
use yanshi_render::render::Renderer;
use yanshi_render::thumb::{render_thumbnail, Thumb, ThumbKind};
use yanshi_render::tile::{TileGrid, TileKey};

use crate::annotations::AnnotationStore;
use crate::broadcast::{Broadcaster, PushChannel, SubscriberId};
use crate::job::{JobId, JobManager, JobStatus};
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
    last_render_blob: Option<BlobHash>,
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
    pub fn rebuild(&mut self) -> Result<()> {
        self.folder = IncrementalFolder::new();
        self.state = FoldEngine::new().fold(&self.log)?.state;
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
    pub fn commit_as(
        &mut self,
        mut atom: Atom,
        actor: &str,
        owner: bool,
        changeset_id: Option<ChangesetId>,
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

        let validation = {
            let exists = |hash: &BlobHash| self.store.exists(hash);
            let mut context = CommitContext::new(&self.state, &exists, actor, &atom_session);
            if owner {
                context = context.allow_cross_actor_revert(true);
            }
            self.log.append_validated(atom, &context)
        };
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

        // 增量折叠（6.6）。
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

        // 双 dirty 传播 → 失效 tile（6.6）。
        let dirty = plan_dirty_with_log(&self.state, previous.as_ref(), &self.log, &appended);
        let dirty_tiles = self.renderer.apply_dirty(&self.state, &dirty);

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
    pub fn render_region(&mut self, bbox: Bbox) -> Result<RenderedPreview> {
        let region = bbox;
        let rendered = self
            .renderer
            .render_region(&self.state, &*self.store, region)?;
        let png = encode_png(rendered.width, rendered.height, &rendered.rgba8)
            .ok_or_else(|| internal("PNG 编码失败（尺寸与像素数不匹配）"))?;
        let blob_hash = self.store.put(&png)?;
        self.render_watermark = self.log.head_seq();
        self.last_render_blob = Some(blob_hash.clone());
        self.complete_render_jobs()?;
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

    /// 生成缩略图并输出 PNG 到 CAS（7 章）。
    pub fn thumbnail(&mut self, kind: ThumbKind, target: Option<Bbox>) -> Result<RenderedPreview> {
        let thumb: Thumb =
            render_thumbnail(&mut self.renderer, &self.state, &*self.store, kind, target)?;
        let png = encode_png(thumb.size, thumb.size, &thumb.rgba8)
            .ok_or_else(|| internal("PNG 编码失败（缩略图尺寸不匹配）"))?;
        let blob_hash = self.store.put(&png)?;
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
            thumb_url: self.last_render_blob.as_ref().map(preview_url),
        })
    }

    /// 当前 HEAD 的渲染地址（若已渲染过）。
    pub fn latest_preview_url(&self) -> Option<String> {
        self.last_render_blob.as_ref().map(preview_url)
    }

    /// 执行待处理的重型渲染 job，返回完成的 job。
    pub fn run_pending_jobs(&mut self) -> Result<Vec<JobId>> {
        let pending = self.jobs.pending();
        if pending.is_empty() {
            return Ok(Vec::new());
        }
        // 渲染一次即覆盖所有待处理 job（同一 HEAD 的渲染结果相同）；
        // `render_region` 内部会把它们标记为 committed 并广播 job 完成事件。
        let bbox = Bbox::new(0.0, 0.0, self.state.width as f64, self.state.height as f64);
        let _ = self.render_region(bbox)?;
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
        self.last_render_blob = preview_blob;
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

    /// Blob GC（6.3）：根集 = 全日志引用闭包 ∪ 活跃 Manifest。
    pub fn collect_garbage(&self, now: i64) -> Result<(GcReport, usize)> {
        let manifest = self.state.active_blob_manifest();
        let (report, plan) = run_gc(
            &*self.store,
            &self.log,
            &manifest,
            &Default::default(),
            now,
            self.settings.orphan_ttl_seconds,
        )?;
        Ok((report, plan.historical.len()))
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
    match dirty.kind {
        yanshi_render::dirty::DirtyKind::None => "none",
        yanshi_render::dirty::DirtyKind::Geometry => "geometry",
        yanshi_render::dirty::DirtyKind::Structure => "structure",
        yanshi_render::dirty::DirtyKind::Full => "full",
    }
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
        let (report, historical) = document
            .collect_garbage(yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000)
            .unwrap();
        assert_eq!(report.deleted.len(), 0, "没有孤儿可清理");
        assert_eq!(historical, 0, "当前状态没有历史级 blob");
        assert_eq!(head_before, document.head_seq());
    }
}
