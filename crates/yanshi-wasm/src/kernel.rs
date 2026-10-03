//! 平台无关的 WASM 计算内核（设计文档 13.3 / 6.1「计算内核层单一来源」）。
//!
//! 本模块不依赖 wasm 运行时，可在宿主上直接跑单测；[`crate::WasmKernel`] 只是它的薄包装。
//!
//! 职责：
//!
//! - 本地维护与服务端**同构**的原子日志与折叠状态（[`AtomLog`] + [`IncrementalFolder`]）；
//! - 增量折叠 + 双 dirty 传播 + tile 失效（与服务端同一套 `yanshi-render` 代码，D0 bit-exact）；
//! - 乐观渲染：拿到原子立刻出像素，不等服务端确认；
//! - **LRU tile 内存池与水位兜底**（13.3）：硬上限 + 90% 水位主动淘汰，
//!   并暴露 [`Kernel::evict_outside_viewport`] 给 JS 侧按视口联动。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use yanshi_core::blob::{BlobStore, MemoryBlobStore};
use yanshi_core::fold::FoldWarning;
use yanshi_core::{
    Atom, AtomLog, Bbox, BlobHash, DocumentState, IncrementalFolder, ObjectType, Seq,
};
use yanshi_render::dirty::{plan_dirty_with_log, DirtyKind, DirtySet};
use yanshi_render::png::encode_png;
use yanshi_render::render::Renderer;
use yanshi_render::tile::{TileGrid, TileKey};

/// 默认硬内存上限（13.3 举例 500MB；客户端保守取 128MB，浏览器 tab 更安全）。
pub const DEFAULT_MEMORY_LIMIT: usize = 128 * 1024 * 1024;
/// 水位兜底触发比例（13.3：硬上限 90%）。
pub const WATERMARK_RATIO: f64 = 0.9;
/// 水位兜底的目标比例（淘汰到硬上限的 80%）。
pub const WATERMARK_TARGET_RATIO: f64 = 0.8;

/// 内核错误（与 5.7 错误码同名，便于 JS 侧统一处理）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KernelError {
    /// 错误码（复用 5.7 词表）。
    pub code: String,
    /// 说明。
    pub detail: String,
}

impl KernelError {
    /// 构造。
    pub fn new(code: &str, detail: impl Into<String>) -> Self {
        Self {
            code: code.to_owned(),
            detail: detail.into(),
        }
    }

    /// JSON 形状（`{"ok":false,"error_code":...,"context":{...}}`）。
    pub fn to_json(&self) -> Value {
        json!({
            "ok": false,
            "error_code": self.code,
            "retryable": self.code == "out_of_order",
            "context": {"detail": self.detail},
        })
    }
}

/// 落笔提交的结果：与 [`ApplyReport`] 的脏区部分同形，供调用方只重绘脏区。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommitReport {
    /// 归属的日志序号。
    pub seq: u64,
    /// 需要重绘的区域。
    pub dirty_bbox: Option<[f64; 4]>,
    /// 被失效并重算的 tile。
    pub dirty_tiles: Vec<TileKey>,
    /// 本地日志 HEAD。
    pub head: u64,
}

/// 单次原子应用的报告（对应服务端 10.1 的 dirty 字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyReport {
    /// 原子 id。
    pub atom_id: String,
    /// 权威 seq。
    pub seq: Seq,
    /// 是否幂等命中（已在本地日志里）。
    pub duplicate: bool,
    /// dirty 类别。
    pub dirty_kind: String,
    /// dirty 包围盒 `[x, y, w, h]`。
    pub dirty_bbox: Option<[f64; 4]>,
    /// 失效的 tile。
    pub dirty_tiles: Vec<TileKey>,
    /// 折叠警告（级联失效等）。
    pub warnings: Vec<FoldWarning>,
    /// 折叠后的 HEAD。
    pub head: Seq,
}

/// 内核统计（14.9 可观测性）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KernelStats {
    /// 已应用原子数。
    pub atoms: u64,
    /// 幂等命中次数。
    pub duplicates: u64,
    /// HEAD seq。
    pub head_seq: Seq,
    /// tile 缓存命中。
    pub cache_hits: u64,
    /// tile 缓存未命中。
    pub cache_misses: u64,
    /// tile 缓存淘汰（含 dirty 失效以外的 LRU/视口淘汰）。
    pub cache_evictions: u64,
    /// 水位兜底自动淘汰的 tile 数（13.3，不依赖 JS 调用）。
    pub auto_evictions: u64,
    /// 当前 tile 占用的字节。
    pub used_bytes: usize,
    /// 硬上限字节。
    pub memory_limit: usize,
    /// 水位比例。
    pub watermark: f64,
    /// 当前缓存 tile 数。
    pub tiles: usize,
    /// 本地 blob 数。
    pub blobs: usize,
}

/// 原生计算结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderResult {
    /// 实际渲染区域 `[x, y, w, h]`。
    pub bbox: [f64; 4],
    /// 宽。
    pub width: u32,
    /// 高。
    pub height: u32,
    /// 直通 RGBA8（与 PNG 同源，供 canvas `ImageData` 零拷贝使用）。
    pub rgba8: Vec<u8>,
    /// PNG 字节（与服务端同一编码器 → 可直接比对哈希验证 bit-exact）。
    pub png: Vec<u8>,
    /// 涉及的 tile。
    pub tiles: Vec<TileKey>,
    /// 滤镜邻域扩展。
    pub filter_padding: u32,
    /// 不支持类型的警告。
    pub warnings: Vec<String>,
}

#[cfg(test)]
/// 点到线段的距离（测试用：判断差异是否落在笔迹附近）。
fn point_segment_distance(px: f64, py: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return ((px - x0).powi(2) + (py - y0).powi(2)).sqrt();
    }
    let t = (((px - x0) * dx + (py - y0) * dy) / length_squared).clamp(0.0, 1.0);
    let (cx, cy) = (x0 + t * dx, y0 + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

/// 两个区域（任一可空）的并集。
fn union_bbox(a: Option<Bbox>, b: Option<Bbox>) -> Option<Bbox> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let x = a.x.min(b.x);
            let y = a.y.min(b.y);
            let right = (a.x + a.w).max(b.x + b.w);
            let bottom = (a.y + a.h).max(b.y + b.h);
            Some(Bbox::new(x, y, right - x, bottom - y))
        }
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// 把请求区域夹到画布内（至少 1×1 像素）。
fn clamp_to_canvas(region: Bbox, width: u32, height: u32) -> Bbox {
    let x = region.x.max(0.0).min((width as f64 - 1.0).max(0.0));
    let y = region.y.max(0.0).min((height as f64 - 1.0).max(0.0));
    let w = region.w.max(1.0).min(width as f64 - x).max(1.0);
    let h = region.h.max(1.0).min(height as f64 - y).max(1.0);
    Bbox::new(x, y, w, h)
}

/// 本地待提交覆盖层的保留对象 id（不进原子日志，13.3 乐观渲染）。
pub const PREVIEW_OBJECT_ID: &str = "__yanshi_preview__";

/// 计算内核实例。
pub struct Kernel {
    doc_id: String,
    log: AtomLog,
    folder: IncrementalFolder,
    state: DocumentState,
    renderer: Renderer,
    store: MemoryBlobStore,
    viewport: Bbox,
    memory_limit: usize,
    atoms_applied: u64,
    duplicates: u64,
    auto_evictions: u64,
    last_tiles: Vec<TileKey>,
    /// 上一次盖过章的点列（增量盖章的锚点）。
    preview_points: Option<Vec<yanshi_render::brush::StrokePoint>>,
    /// 跨帧采样游标（保证逐段盖章与一次性整段逐点一致）。
    preview_cursor: Option<yanshi_render::geometry::StrokeCursor>,
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kernel")
            .field("doc_id", &self.doc_id)
            .field("head_seq", &self.log.head_seq())
            .field("atoms", &self.log.len())
            .field("memory_limit", &self.memory_limit)
            .finish()
    }
}

impl Kernel {
    /// 新建内核。
    ///
    /// `width`/`height` 来自服务端的文档摘要（13.1 edit 模式先行数据）；
    /// `tile_size` 必须是渲染内核允许的分块（32/64/128/256/512）。
    pub fn new(
        doc_id: impl Into<String>,
        tile_size: u32,
        width: u32,
        height: u32,
        memory_limit: usize,
    ) -> Result<Self, KernelError> {
        let grid = TileGrid::new(tile_size, width, height).ok_or_else(|| {
            KernelError::new(
                "invalid_argument",
                format!("tile_size/画布尺寸非法：{tile_size} {width}×{height}"),
            )
        })?;
        let memory_limit = memory_limit.max(tile_size as usize * tile_size as usize * 8);
        Ok(Self {
            doc_id: doc_id.into(),
            log: AtomLog::new(),
            folder: IncrementalFolder::new(),
            state: DocumentState::empty(),
            renderer: Renderer::with_budget(grid, memory_limit),
            store: MemoryBlobStore::new(),
            viewport: Bbox::new(0.0, 0.0, width as f64, height as f64),
            memory_limit,
            atoms_applied: 0,
            duplicates: 0,
            auto_evictions: 0,
            last_tiles: Vec::new(),
            preview_points: None,
            preview_cursor: None,
        })
    }

    /// 文档 id。
    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }

    /// HEAD seq。
    pub fn head_seq(&self) -> Seq {
        self.log.head_seq()
    }

    /// 本地状态。
    pub const fn state(&self) -> &DocumentState {
        &self.state
    }

    /// 本地日志。
    pub const fn log(&self) -> &AtomLog {
        &self.log
    }

    /// 硬内存上限。
    pub const fn memory_limit(&self) -> usize {
        self.memory_limit
    }

    /// 设置硬上限；若已超水位会立刻兜底淘汰。
    pub fn set_memory_limit(&mut self, bytes: usize) {
        let one_tile = {
            let grid = self.renderer.grid();
            (grid.tile_size() * grid.tile_size() * 8) as usize
        };
        self.memory_limit = bytes.max(one_tile);
        self.renderer.cache_mut().set_budget(self.memory_limit);
        self.enforce_watermark();
    }

    /// 视口（用于数据流过滤与视口淘汰）。
    pub const fn viewport(&self) -> Bbox {
        self.viewport
    }

    /// 设置视口（JS 侧 `IntersectionObserver` / 视口矩阵驱动）。
    pub fn set_viewport(&mut self, viewport: Bbox) {
        self.viewport = viewport;
    }

    /// 13.3：JS 主动按视口淘汰，返回淘汰的 tile 数。
    pub fn evict_outside_viewport(&mut self, viewport: Bbox) -> usize {
        self.viewport = viewport;
        self.renderer.cache_mut().evict_outside_viewport(&viewport)
    }

    /// 设置/更新**本地待提交覆盖层**（13.3 乐观渲染）。
    ///
    /// 覆盖层只改本地 `state` 的像素，`log` 完全不动——这样「拖动中的笔迹」不会污染
    /// 原子日志，也不会因为反复 apply 同一个 id 而走幂等分支（历史缺陷：笔迹只画开头一段）。
    /// 返回需要重绘的区域（旧覆盖层 ∪ 新覆盖层，保证上一帧的笔迹被擦掉）。
    pub fn set_preview_object(&mut self, json: &str) -> Result<Option<Bbox>, KernelError> {
        self.upsert_preview_object(json, true)
    }

    /// 更新覆盖层状态但**不失效 tile**：像素由 [`Kernel::extend_preview_stroke`] 的增量盖章维护，
    /// 若每帧都失效整条笔迹的 tile，增量盖章会被打回整块重绘（实测浪费 ~130ms/帧）。
    fn upsert_preview_object(
        &mut self,
        json: &str,
        invalidate: bool,
    ) -> Result<Option<Bbox>, KernelError> {
        // 覆盖层用**最小字段**构造，不要求调用方提供 versions/created_by 等日志语义字段。
        let value: Value = serde_json::from_str(json).map_err(|error| {
            KernelError::new("invalid_argument", format!("待提交对象解析失败：{error}"))
        })?;
        let layer_id = value
            .get("layer_id")
            .and_then(Value::as_str)
            .ok_or_else(|| KernelError::new("invalid_argument", "待提交对象缺少 layer_id"))?
            .to_owned();
        let object_type: ObjectType = serde_json::from_value(
            value.get("type").cloned().unwrap_or(Value::Null),
        )
        .map_err(|error| {
            KernelError::new("invalid_argument", format!("待提交对象类型非法：{error}"))
        })?;
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let z_index = value
            .get("z_index")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);

        // 目标图层不存在（或已删除）时退到第一个存活图层，避免覆盖层静默不可见。
        let layer_ok = self
            .state
            .layers
            .get(&layer_id)
            .map(|layer| !layer.is_deleted())
            .unwrap_or(false);
        let layer_id = if layer_ok {
            layer_id
        } else {
            self.state
                .alive_layers()
                .first()
                .map(|layer| layer.id.clone())
                .ok_or_else(|| KernelError::new("precondition_failed", "文档没有存活图层"))?
        };

        let object = yanshi_core::Object {
            id: PREVIEW_OBJECT_ID.to_owned(),
            layer_id,
            object_type,
            z_index,
            visible: value
                .get("visible")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            locked: false,
            metadata: Value::Null,
            transform: yanshi_core::Transform::IDENTITY,
            style: None,
            versions: Vec::new(),
            current_version: None,
            created_by: "preview".to_owned(),
            deleted_by: None,
            data,
            blobs: Vec::new(),
        };

        let previous = self.remove_preview_bbox();
        let current = yanshi_render::object_bbox(&object);
        self.state
            .objects
            .insert(PREVIEW_OBJECT_ID.to_owned(), object);
        let dirty = union_bbox(previous, current);
        if invalidate {
            self.invalidate_bbox(dirty);
        }
        Ok(dirty)
    }

    /// 更新待提交笔迹：失效覆盖层的 tile 并返回需要重绘的区域。
    ///
    /// **为什么不是增量盖章**：`Renderer::stamp_into_tiles_incremental` 的读改写路径实测会
    /// 让 tile 丢掉场景内容（表现为画面上出现白块/内容消失，见 implementation-notes 的复盘），
    /// 在定位清楚之前这里走「失效 + 客户端重绘」这条**已证正确**的路径。
    /// 采样相位连续的增量能力（`StrokeCursor`）已在渲染内核里实现并单测通过，待缺陷修好后启用。
    pub fn extend_preview_stroke(&mut self, json: &str) -> Result<Option<Bbox>, KernelError> {
        let value: Value = serde_json::from_str(json).map_err(|error| {
            KernelError::new("invalid_argument", format!("待提交笔迹解析失败：{error}"))
        })?;
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let geometry = yanshi_render::brush::StrokeGeometry::from_value(&data);
        let brush = yanshi_render::brush::BrushSpec::from_value(&data);
        let incoming: Option<Vec<yanshi_render::brush::StrokePoint>> =
            geometry.as_ref().map(|geometry| geometry.points.clone());

        // 笔迹是**追加式增长**的：本次只需失效「新增笔段」的 tile。
        // 旧像素不会被擦除（点数只增不减），而失效的 tile 仍由权威状态重渲染，
        // 因此正确性不依赖任何增量盖章，成本却从 O(整条笔迹) 降到 O(新增笔段)。
        // 点列被替换（换工具/重开一笔）时才退回整段并集。
        let previous = self.preview_points.take();
        let mut bbox = self.upsert_preview_object(&value.to_string(), true)?;
        let is_extension = match (&previous, &incoming) {
            (Some(previous), Some(incoming)) => {
                incoming.len() > previous.len()
                    && previous.iter().zip(incoming.iter()).all(|(a, b)| {
                        (a.x - b.x).abs() < 1e-9
                            && (a.y - b.y).abs() < 1e-9
                            && (a.pressure - b.pressure).abs() < 1e-9
                    })
            }
            _ => false,
        };
        if is_extension {
            if let (Some(previous), Some(incoming)) = (&previous, &incoming) {
                let tail = &incoming[previous.len().saturating_sub(1)..];
                let margin = brush.size / 2.0 + brush.jitter + 1.0;
                let mut min_x = f64::INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for point in tail {
                    min_x = min_x.min(point.x);
                    min_y = min_y.min(point.y);
                    max_x = max_x.max(point.x);
                    max_y = max_y.max(point.y);
                }
                if min_x.is_finite() {
                    bbox = Some(Bbox::new(
                        min_x - margin,
                        min_y - margin,
                        (max_x - min_x) + margin * 2.0,
                        (max_y - min_y) + margin * 2.0,
                    ));
                }
            }
        }
        self.preview_points = incoming;
        Ok(bbox)
    }

    /// 落笔提交：把覆盖层对应的原子并入本地日志，**并让受影响 tile 失效重算**。
    ///
    /// 返回 `CommitReport`（与 `apply_atom` 的 `dirty_bbox` 同形），调用方据此只重绘脏区。
    ///
    /// 历史缺陷：早期实现假定了"覆盖层的像素已经盖章进 tile"因而不重绘 ✗。这在覆盖层
    /// 与提交原子落在**不同图层**时不成立（新建图层后落笔），于是落笔后按 tile 渲染拿到的是
    /// 旧内容，表现为"每次操作后画布空白、刷新或重做才可见"。现在与 `apply_atom` 走同一条
    /// 失效路径（`plan_dirty_with_log` + `apply_dirty`），不再依赖该假设。
    ///
    /// 服务端权威 seq 与本地预测不一致时，调用方应改用 `resync()` 全量重建。
    pub fn commit_preview(&mut self, atom_json: &str) -> Result<CommitReport, KernelError> {
        let mut atom: Atom = serde_json::from_str(atom_json).map_err(|error| {
            KernelError::new("invalid_argument", format!("原子解析失败：{error}"))
        })?;
        // 覆盖层对象从本地 state 摘掉：它的像素已经画进 tile，状态由新原子接管。
        self.preview_points = None;
        self.preview_cursor = None;
        self.state.objects.remove(PREVIEW_OBJECT_ID);
        let expected = self.log.head_seq() + 1;
        if atom.is_submitted() && atom.seq != expected {
            return Err(KernelError::new(
                "out_of_order",
                format!(
                    "原子 seq={} 与本地 HEAD={expected} 不连续，需要重新同步",
                    atom.seq
                ),
            ));
        }
        let applied_id = atom.id.clone();
        atom.seq = 0;
        // 折叠前后都要有 state：脏区规划需要"前一份状态"。
        let previous = self.state.clone();
        let outcome = self
            .log
            .append(atom)
            .map_err(|error| KernelError::new("precondition_failed", error.to_string()))?;
        let seq = outcome.seq();
        let folded = self
            .folder
            .fold(&self.log, self.log.head_seq())
            .map_err(|error| KernelError::new("precondition_failed", error.to_string()))?;
        self.state = folded.state;
        self.atoms_applied += 1;

        // 与 apply_atom 相同的失效路径：落笔后必须重算受影响 tile，否则渲染读到旧内容。
        let applied = self.log.get(&applied_id).cloned();
        let dirty = match applied.as_ref() {
            Some(applied) => plan_dirty_with_log(&self.state, Some(&previous), &self.log, applied),
            None => DirtySet::none(),
        };
        let tiles = self.renderer.apply_dirty(&self.state, &dirty);
        self.last_tiles = tiles.clone();
        self.enforce_watermark();
        Ok(CommitReport {
            seq,
            dirty_bbox: dirty.bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
            dirty_tiles: tiles,
            head: self.log.head_seq(),
        })
    }

    /// 清除本地待提交覆盖层（落笔提交后调用），返回需要重绘的区域。
    pub fn clear_preview(&mut self) -> Option<Bbox> {
        self.preview_points = None;
        self.preview_cursor = None;
        let bbox = self.remove_preview_bbox();
        self.invalidate_bbox(bbox);
        bbox
    }

    /// 当前是否存在待提交覆盖层。
    pub fn has_preview(&self) -> bool {
        self.state.objects.contains_key(PREVIEW_OBJECT_ID)
    }

    fn remove_preview_bbox(&mut self) -> Option<Bbox> {
        self.state
            .objects
            .remove(PREVIEW_OBJECT_ID)
            .and_then(|object| yanshi_render::object_bbox(&object))
    }

    fn invalidate_bbox(&mut self, bbox: Option<Bbox>) {
        let Some(bbox) = bbox else { return };
        let keys = self.renderer.grid().keys_for_bbox(&bbox);
        self.renderer.cache_mut().invalidate(&keys);
    }

    /// 写入本地 blob（设计 6.3：blob 先行），返回 CAS 哈希。
    pub fn blob_put(&mut self, bytes: &[u8]) -> Result<String, KernelError> {
        self.store
            .put(bytes)
            .map(|hash| hash.to_string())
            .map_err(|error| KernelError::new("resource_exhausted", error.to_string()))
    }

    /// 读取本地 blob。
    pub fn blob_get(&mut self, hash: &str) -> Result<Vec<u8>, KernelError> {
        let hash: BlobHash = hash
            .parse()
            .map_err(|_| KernelError::new("invalid_argument", format!("非法 blob 哈希 {hash}")))?;
        if !self.store.exists(&hash) {
            return Err(KernelError::new(
                "reference_not_found",
                format!("本地缺少 blob {hash}（需先 blob_put）"),
            ));
        }
        self.store
            .get(&hash)
            .map_err(|error| KernelError::new("resource_exhausted", error.to_string()))
    }

    /// view 模式批量装载（13.1：从日志一次性折叠）。
    pub fn load_atoms_json(&mut self, json_array: &str) -> Result<Value, KernelError> {
        let atoms: Vec<Atom> = serde_json::from_str(json_array).map_err(|error| {
            KernelError::new("invalid_argument", format!("原子数组解析失败：{error}"))
        })?;
        self.log = AtomLog::with_atoms(atoms)
            .map_err(|error| KernelError::new("invalid_argument", error.to_string()))?;
        let folded = self
            .folder
            .fold(&self.log, self.log.head_seq())
            .map_err(|error| KernelError::new("precondition_failed", error.to_string()))?;
        self.state = folded.state;
        // 全量装载后 tile 缓存全部失效（相当于 declare_head 跳变）。
        self.renderer.cache_mut().clear();
        self.atoms_applied = self.log.len() as u64;
        Ok(json!({
            "ok": true,
            "head_seq": self.log.head_seq(),
            "atoms": self.log.len(),
            "eval_origin_seq": self.state.eval_origin_seq(),
            "warnings": folded.warnings,
        }))
    }

    /// 应用一个原子（本地乐观渲染路径：拿到就画，不等服务端）。
    ///
    /// 原子必须带服务端权威 seq，且与本地 HEAD 连续；否则返回 `out_of_order`，
    /// 调用方应通过 `get_log` 重新同步（12.4 断线重连）。
    pub fn apply_atom_json(&mut self, json: &str) -> Result<Value, KernelError> {
        let atom: Atom = serde_json::from_str(json).map_err(|error| {
            KernelError::new("invalid_argument", format!("原子解析失败：{error}"))
        })?;
        let report = self.apply_atom(atom)?;
        Ok(json!({"ok": true, "report": report}))
    }

    /// 见 [`Kernel::apply_atom_json`]。
    pub fn apply_atom(&mut self, atom: Atom) -> Result<ApplyReport, KernelError> {
        let expected = self.log.head_seq() + 1;
        if atom.is_submitted() && atom.seq != expected {
            // 幂等命中：同一 id 重复投递是正常的（重连补发）。
            if self.log.get(&atom.id).is_some() {
                self.duplicates += 1;
                let existing = self.log.get(&atom.id).expect("刚判定存在");
                return Ok(ApplyReport {
                    atom_id: atom.id.clone(),
                    seq: existing.seq,
                    duplicate: true,
                    dirty_kind: DirtyKind::None.as_str().to_owned(),
                    dirty_bbox: None,
                    dirty_tiles: Vec::new(),
                    warnings: Vec::new(),
                    head: self.log.head_seq(),
                });
            }
            return Err(KernelError::new(
                "out_of_order",
                format!(
                    "原子 seq={} 与本地 HEAD={expected} 不连续，需要重新同步",
                    atom.seq
                ),
            ));
        }

        let previous = self.state.clone();
        let outcome = self
            .log
            .append(atom)
            .map_err(|error| KernelError::new("precondition_failed", error.to_string()))?;
        let seq = outcome.seq();
        let duplicate = !matches!(outcome, yanshi_core::AppendOutcome::Appended { .. });
        let applied = self
            .log
            .by_seq(seq)
            .cloned()
            .ok_or_else(|| KernelError::new("internal", "原子未进入本地日志"))?;

        let folded = self
            .folder
            .fold(&self.log, self.log.head_seq())
            .map_err(|error| KernelError::new("precondition_failed", error.to_string()))?;
        self.state = folded.state;

        let dirty = plan_dirty_with_log(&self.state, Some(&previous), &self.log, &applied);
        let tiles = self.renderer.apply_dirty(&self.state, &dirty);
        self.last_tiles = tiles.clone();
        self.atoms_applied += 1;
        if duplicate {
            self.duplicates += 1;
        }

        // 13.3 水位兜底：达到 90% 就地淘汰，不等 JS 调用。
        self.enforce_watermark();

        Ok(ApplyReport {
            atom_id: applied.id.clone(),
            seq,
            duplicate,
            dirty_kind: dirty.kind.as_str().to_owned(),
            dirty_bbox: dirty.bbox.map(|bbox| [bbox.x, bbox.y, bbox.w, bbox.h]),
            dirty_tiles: tiles,
            warnings: folded.warnings,
            head: self.log.head_seq(),
        })
    }

    /// 渲染区域（乐观渲染 / 校正渲染都用它）。
    ///
    /// **直绘**：用 `Renderer::render_region` 的 scratch 路径渲染一个小区域（不做 tile 组合）。
    ///
    /// 拖动中的笔迹区域通常只有几十像素见方，走 tile 组合哪怕 1px 变化也要重算整块 256² tile；
    /// scratch 直绘只算该区域，代价与面积成正比。两者数值逐位一致（同一套渲染实现）。
    ///
    /// 历史缺陷：查看器一直在调用本方法，但**它此前并不存在** —— wasm 侧抛
    /// `TypeError: ... is not a function`，异常被事件处理器吞掉，于是拖动中没有任何
    /// `putImageData`，表现为「拖动无反馈、落笔后画布空白」。现在补上实现并由测试固定。
    pub fn render_region_direct_rgba(
        &mut self,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<Vec<u8>, KernelError> {
        let bbox = Bbox::new(x as f64, y as f64, width as f64, height as f64);
        let rendered = self
            .renderer
            .render_region(&self.state, &self.store, bbox)
            .map_err(|error| KernelError::new("invalid_argument", error.to_string()))?;
        Ok(rendered.rgba8)
    }

    /// 实现按 **tile 组合**而不是 `Renderer::render_region` 的 scratch 直绘：
    /// 前者复用 LRU tile 缓存（首笔/缩放时只重算失效 tile），后者每次都会重绘区域内所有对象。
    /// 单块 tile 仍由 `render_tile` 渲染，因此滤镜的邻域 padding 依旧正确（见 `render_tile` 文档）。
    pub fn render_region(&mut self, region: Bbox) -> Result<RenderResult, KernelError> {
        let grid = self.renderer.grid().clone();
        let clamped = clamp_to_canvas(region, grid.width(), grid.height());
        let background = self
            .renderer
            .options()
            .background
            .or_else(|| yanshi_render::render::parse_background(&self.state.background));

        let origin_x = clamped.x.floor() as i64;
        let origin_y = clamped.y.floor() as i64;
        let width = ((clamped.x + clamped.w).ceil() as i64 - origin_x).max(1) as u32;
        let height = ((clamped.y + clamped.h).ceil() as i64 - origin_y).max(1) as u32;
        let mut rgba8 = vec![0u8; width as usize * height as usize * 4];
        let mut tiles = Vec::new();

        for key in grid.keys_for_bbox(&clamped) {
            let tile = self
                .renderer
                .render_tile(&self.state, &self.store, key)
                .map_err(|error| KernelError::new("invalid_argument", error.to_string()))?;
            tiles.push(key);
            let bounds = grid.bounds(key);
            let tile_rgba = tile.to_rgba8(background);
            let tile_size = tile.size() as i64;
            let dx = bounds.x as i64 - origin_x;
            let dy = bounds.y as i64 - origin_y;
            // 按行切片拷贝（逐像素拷贝在 1024×1024 上要几十毫秒）。
            let first_column = dx.max(0);
            let last_column = (dx + tile_size).min(width as i64);
            if last_column <= first_column {
                continue;
            }
            let span = (last_column - first_column) as usize * 4;
            for row in 0..tile_size {
                let out_y = dy + row;
                if out_y < 0 || out_y >= height as i64 {
                    continue;
                }
                let src = ((row * tile_size + (first_column - dx)) * 4) as usize;
                let dst = ((out_y * width as i64 + first_column) * 4) as usize;
                rgba8[dst..dst + span].copy_from_slice(&tile_rgba[src..src + span]);
            }
        }

        let png = encode_png(width, height, &rgba8)
            .ok_or_else(|| KernelError::new("internal", "PNG 编码失败（尺寸与像素数不匹配）"))?;
        let result = RenderResult {
            bbox: [clamped.x, clamped.y, width as f64, height as f64],
            width,
            height,
            rgba8,
            png,
            tiles,
            filter_padding: self.renderer.filter_padding(&self.state),
            // tile 路径不收集对象解析告警；类型级告警以服务端为权威（客户端只负责像素）。
            warnings: Vec::new(),
        };
        self.enforce_watermark();
        Ok(result)
    }

    /// 状态摘要（与服务端 `get_state` 对齐的裁剪版）。
    pub fn state_json(&self) -> Value {
        json!({
            "doc_id": self.doc_id,
            "head_seq": self.log.head_seq(),
            "eval_origin_seq": self.state.eval_origin_seq(),
            "width": self.state.width,
            "height": self.state.height,
            "layers": self.state.alive_layers().len(),
            "objects": self.state.alive_objects().len(),
            "atoms": self.log.len(),
            "suppressed": self.suppressed().len(),
        })
    }

    /// 本地被有效 revert 撤销的原子（有效集抑制结果）。
    pub fn suppressed(&self) -> BTreeSet<String> {
        yanshi_core::fold::compute_suppressed(self.log.atoms())
            .into_iter()
            .collect()
    }

    /// 统计。
    pub fn stats(&self) -> KernelStats {
        let cache = self.renderer.cache().stats();
        KernelStats {
            atoms: self.atoms_applied,
            duplicates: self.duplicates,
            head_seq: self.log.head_seq(),
            cache_hits: cache.hits,
            cache_misses: cache.misses,
            cache_evictions: cache.evictions,
            auto_evictions: self.auto_evictions,
            used_bytes: cache.used_bytes,
            memory_limit: self.memory_limit,
            watermark: self.renderer.cache().watermark(),
            tiles: cache.tiles,
            blobs: self.store.len(),
        }
    }

    /// 最近一次 dirty 的 tile。
    pub fn last_dirty_tiles(&self) -> &[TileKey] {
        &self.last_tiles
    }

    /// 13.3 水位兜底：先淘汰视口外，再按 LRU 淘汰到 80%，都不依赖 JS 调用。
    fn enforce_watermark(&mut self) {
        if self.memory_limit == 0 {
            return;
        }
        let threshold = self.memory_limit as f64 * WATERMARK_RATIO;
        if (self.renderer.cache().stats().used_bytes as f64) < threshold {
            return;
        }
        let evicted = self
            .renderer
            .cache_mut()
            .evict_outside_viewport(&self.viewport);
        self.auto_evictions += evicted as u64;
        if (self.renderer.cache().stats().used_bytes as f64) >= threshold {
            let target = (self.memory_limit as f64 * WATERMARK_TARGET_RATIO) as usize;
            let evicted = self.renderer.cache_mut().evict_to_bytes(target);
            self.auto_evictions += evicted as u64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scene_atoms() -> Vec<Value> {
        vec![
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_1", "width": 128, "height": 128, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1", "name": "ink"}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA03", "seq": 3, "kind": "draw_stroke",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"object_id": "obj_1", "layer_id": "layer_1",
                               "data": {"points": [[10.0, 10.0], [110.0, 40.0]], "size": 6.0,
                                        "color": [30, 30, 40, 255]}}}),
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA04", "seq": 4, "kind": "draw_shape",
                   "actor": "human:1", "session": "s", "timestamp": 4,
                   "payload": {"object_id": "obj_2", "layer_id": "layer_1",
                               "data": {"geometry": {"kind": "rect", "bbox": {"x": 20, "y": 60, "w": 40, "h": 30}},
                                        "color": "#c0392b"}}}),
        ]
    }

    fn kernel_with(atoms: &[Value], memory_limit: usize) -> Kernel {
        let mut kernel = Kernel::new("doc_1", 32, 128, 128, memory_limit).unwrap();
        for atom in atoms {
            kernel
                .apply_atom_json(&atom.to_string())
                .unwrap_or_else(|error| panic!("应用原子失败：{error:?}"));
        }
        kernel
    }

    #[test]
    fn applies_atoms_incrementally_and_matches_full_fold() {
        let atoms = scene_atoms();
        let kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        assert_eq!(kernel.head_seq(), 4);
        assert_eq!(kernel.state().objects.len(), 2);
        assert_eq!(kernel.state().layers.len(), 2);

        // 增量折叠结果必须与一次性全量折叠一致（客户端/服务端同构的前提）。
        let mut full = Kernel::new("doc_1", 32, 128, 128, DEFAULT_MEMORY_LIMIT).unwrap();
        let array = serde_json::to_string(&atoms).unwrap();
        full.load_atoms_json(&array).unwrap();
        assert_eq!(
            serde_json::to_value(full.state()).unwrap(),
            serde_json::to_value(kernel.state()).unwrap()
        );
        assert_eq!(full.head_seq(), kernel.head_seq());
    }

    #[test]
    fn duplicate_and_out_of_order_atoms_are_handled() {
        let atoms = scene_atoms();
        let mut kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);

        // 幂等：重复投递同一 id 不改变状态。
        let report = kernel.apply_atom_json(&atoms[2].to_string()).unwrap();
        assert_eq!(report["report"]["duplicate"], json!(true));
        assert_eq!(kernel.head_seq(), 4);
        assert_eq!(kernel.stats().duplicates, 1);

        // 跳号：必须报 out_of_order 让 JS 重新同步，而不是静默错位。
        let mut skipped = atoms[3].clone();
        skipped["id"] = json!("01AAAAAAAAAAAAAAAAAAAAAAAA09");
        skipped["seq"] = json!(9);
        let error = kernel.apply_atom_json(&skipped.to_string()).unwrap_err();
        assert_eq!(error.code, "out_of_order");
        assert!(error.to_json()["retryable"].as_bool().unwrap());
        assert_eq!(kernel.head_seq(), 4, "拒绝的原子不应进入本地日志");
    }

    #[test]
    fn revert_suppresses_and_reapply_restores_locally() {
        let mut atoms = scene_atoms();
        atoms.push(
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA05", "seq": 5, "kind": "revert",
                          "actor": "human:1", "session": "s", "timestamp": 5,
                          "payload": {"target": "01AAAAAAAAAAAAAAAAAAAAAAAA03"}}),
        );
        let mut kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        assert!(kernel.suppressed().contains("01AAAAAAAAAAAAAAAAAAAAAAAA03"));

        atoms.push(
            json!({"id": "01AAAAAAAAAAAAAAAAAAAAAAAA06", "seq": 6, "kind": "reapply",
                          "actor": "human:1", "session": "s", "timestamp": 6,
                          "payload": {"target": "01AAAAAAAAAAAAAAAAAAAAAAAA03"}}),
        );
        let reapply = atoms.last().unwrap().clone();
        kernel.apply_atom_json(&reapply.to_string()).unwrap();
        assert!(kernel.suppressed().is_empty(), "reapply 后恢复");
    }

    #[test]
    fn watermark_auto_evicts_without_js_and_keeps_pixels_identical() {
        let atoms = scene_atoms();
        // 只给 2 个 32×32 tile 的预算（每个 32*32*8 = 8KB），强制触发水位兜底。
        let tiny = 2 * 32 * 32 * 8;
        let mut kernel = kernel_with(&atoms, tiny);
        kernel.set_viewport(Bbox::new(0.0, 0.0, 64.0, 64.0));

        let first = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        assert!(
            kernel.stats().auto_evictions > 0,
            "达到 90% 水位应自动淘汰：{:?}",
            kernel.stats()
        );
        assert!(kernel.stats().used_bytes <= kernel.memory_limit());

        // 淘汰不得改变像素：与宽预算内核逐字节比对（D0 确定性 + 缓存无关性）。
        let generous = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let mut generous = generous;
        let second = generous
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        assert_eq!(
            first.png, second.png,
            "水位淘汰后的渲染必须与宽预算渲染 bit-exact"
        );
        assert_eq!(first.rgba8, second.rgba8);
        assert!(kernel.stats().watermark <= 0.9);
    }

    #[test]
    fn evict_outside_viewport_is_driven_by_js_and_reports_counts() {
        let atoms = scene_atoms();
        let mut kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        let before = kernel.stats().tiles;
        assert!(before > 1, "128×128 至少两块 tile：{before}");

        let evicted = kernel.evict_outside_viewport(Bbox::new(0.0, 0.0, 32.0, 32.0));
        assert!(evicted > 0);
        assert_eq!(kernel.stats().tiles, before - evicted);
        assert!(kernel.stats().cache_evictions >= evicted as u64);

        // 视口内仍可渲染，且结果与全量渲染的对应区域一致。
        let after = kernel
            .render_region(Bbox::new(0.0, 0.0, 32.0, 32.0))
            .unwrap();
        let mut reference = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let full = reference
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        let expected: Vec<u8> = (0..32)
            .flat_map(|y| {
                let start = ((y * 128) * 4) as usize;
                full.rgba8[start..start + 32 * 4].to_vec()
            })
            .collect();
        assert_eq!(after.rgba8, expected, "视口内像素应与整幅渲染一致");
    }

    /// Phase 2 出口条件：客户端（按 tile 组合）与服务端（整幅区域渲染）必须 bit-exact。
    ///
    /// 这条测试曾经抓到真缺陷：服务端从 f32 scratch 直接转 u8、客户端经 f16 tile 转 u8，
    /// 在舍入边界上差 149/4.2M 字节（如 32 vs 31）。现在两条路径都以 f16 为准。
    #[test]
    fn tile_composed_render_is_bit_exact_with_whole_region_render() {
        use yanshi_render::render::Renderer;
        use yanshi_render::tile::TileGrid;

        let atoms = scene_atoms();
        let kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let state = kernel.state();

        for tile_size in [32u32, 64] {
            let grid = TileGrid::new(tile_size, state.width, state.height).unwrap();
            let mut server_side = Renderer::with_budget(grid, DEFAULT_MEMORY_LIMIT);
            let whole = server_side
                .render_region(
                    state,
                    &MemoryBlobStore::new(),
                    Bbox::new(0.0, 0.0, state.width as f64, state.height as f64),
                )
                .unwrap();

            let mut client_side = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
            let composed = client_side
                .render_region(Bbox::new(0.0, 0.0, state.width as f64, state.height as f64))
                .unwrap();

            assert_eq!(whole.width, composed.width);
            assert_eq!(whole.height, composed.height);
            assert_eq!(
                whole.rgba8, composed.rgba8,
                "tile_size={tile_size}：服务端整幅渲染与客户端按 tile 组合必须逐字节一致"
            );
        }
    }

    /// 待提交覆盖层：笔迹随指针增长可见，且**完全不动原子日志**。
    #[test]
    fn preview_overlay_grows_without_touching_the_atom_log() {
        let atoms = scene_atoms();
        let mut kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let log_before = kernel.log().len();
        let head_before = kernel.head_seq();

        let stroke = |points: serde_json::Value| {
            json!({
                "id": "preview",
                "layer_id": "layer_1",
                "type": "stroke",
                "visible": true,
                "data": {"points": points, "size": 6.0, "color": [20, 20, 30, 255]},
            })
            .to_string()
        };

        // 第一段。
        let first = kernel
            .set_preview_object(&stroke(json!([[10.0, 10.0], [40.0, 20.0]])))
            .unwrap()
            .expect("应返回重绘区域");
        assert!(kernel.has_preview());
        let painted = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        // 笔迹中段被画上（背景是白色，取整行最小值判断）。
        let sample = |rgba: &[u8], x: u32, y: u32| {
            let index = ((y * 128 + x) * 4) as usize;
            rgba[index]
        };
        let dark = |rgba: &[u8], y: u32| (0..128).filter(|x| sample(rgba, *x, y) < 200).count();
        let dark_before = dark(&painted.rgba8, 15);
        assert!(dark_before > 0, "第一段应画出来");

        // 第二段（笔迹变长）：返回的区域必须覆盖新旧两段，且像素确实增长。
        let second = kernel
            .set_preview_object(&stroke(json!([[10.0, 10.0], [110.0, 90.0]])))
            .unwrap()
            .expect("应返回重绘区域");
        assert!(
            second.w > first.w || second.h > first.h,
            "并集区域应覆盖增长后的笔迹：{first:?} → {second:?}"
        );
        let painted = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        let dark_after = dark(&painted.rgba8, 15) + dark(&painted.rgba8, 40);
        assert!(
            dark_after > dark_before,
            "笔迹应随指针增长：{dark_before} → {dark_after}"
        );

        // 覆盖层不写日志：原子数与 HEAD 都不变。
        assert_eq!(kernel.log().len(), log_before, "覆盖层不得写入原子日志");
        assert_eq!(kernel.head_seq(), head_before);

        // 清除覆盖层后像素回到提交状态。
        let cleared = kernel.clear_preview().expect("应返回重绘区域");
        assert!(!kernel.has_preview());
        assert!(cleared.w > 0.0);
        let after = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        // 与「从未设置覆盖层」的基线比较：场景本身的笔迹应原样回来，且不残留覆盖层像素。
        let mut baseline = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let baseline = baseline
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        assert_eq!(after.rgba8, baseline.rgba8, "清除覆盖层后应回到提交状态");
        assert_eq!(kernel.log().len(), log_before);
    }

    /// 增量盖章路径：逐帧延长笔迹后，像素必须与「一次性整段覆盖层」一致（允许接缝处极小差异）。
    #[test]
    fn incremental_preview_stroke_matches_single_shot_overlay() {
        let atoms = scene_atoms();
        let stroke = |points: Vec<[f64; 2]>| {
            json!({
                "layer_id": "layer_1",
                "type": "stroke",
                "data": {"points": points, "size": 6.0, "color": [20, 20, 30, 255], "hardness": 0.7},
            })
            .to_string()
        };
        let full_points: Vec<[f64; 2]> = (0..9)
            .map(|i| [10.0 + i as f64 * 11.0, 20.0 + i as f64 * 9.0])
            .collect();

        // 增量：每帧多一个点。
        let mut incremental = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        for count in 1..=full_points.len() {
            incremental
                .extend_preview_stroke(&stroke(full_points[..count].to_vec()))
                .unwrap();
            // 逐帧 1×1 探针会把局部渲染混进被测路径；仅在调试时启用。
            if std::env::var("YANSHI_DEBUG_PROBE").is_ok() {
                let probe = incremental
                    .render_region(Bbox::new(36.0, 15.0, 1.0, 1.0))
                    .unwrap();
                println!(
                    "  帧 {count}: 场景(36,15) = {:?}",
                    [
                        probe.rgba8[0],
                        probe.rgba8[1],
                        probe.rgba8[2],
                        probe.rgba8[3]
                    ]
                );
            }
        }
        let incremental_pixels = incremental
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();

        // 一次性整段覆盖层。
        let mut single = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        single
            .set_preview_object(&stroke(full_points[..full_points.len()].to_vec()))
            .unwrap();
        let single_pixels = single
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();

        // 现状覆盖层路径（失效 + 客户端重绘）必须与一次性整段渲染**逐字节一致**：
        // 像素始终由权威状态重建，不存在接缝叠加或内容丢失。
        let mut diff = 0usize;
        let mut far_from_stroke = 0usize;
        for (index, (a, b)) in incremental_pixels
            .rgba8
            .iter()
            .zip(single_pixels.rgba8.iter())
            .enumerate()
        {
            if a == b {
                continue;
            }
            diff += 1;
            let pixel = index / 4;
            let x = (pixel % 128) as f64;
            let y = (pixel / 128) as f64;
            // 笔迹是从 (10,20) 到 (98,92) 的一条带；离它 20px 之外不应有差异。
            let distance = point_segment_distance(x, y, 10.0, 20.0, 98.0, 92.0);
            if distance > 20.0 {
                far_from_stroke += 1;
            }
        }
        assert_eq!(
            diff,
            0,
            "覆盖层路径必须与一次性整段渲染逐字节一致（差异 {diff}/{}）",
            incremental_pixels.rgba8.len()
        );
        assert_eq!(far_from_stroke, 0);
        assert_eq!(incremental.head_seq(), single.head_seq(), "覆盖层不改 HEAD");
        assert_eq!(
            incremental.log().len(),
            single.log().len(),
            "覆盖层不写日志"
        );
    }

    #[test]
    fn blobs_are_stored_locally_and_required_before_use() {
        let mut kernel = Kernel::new("doc_1", 32, 64, 64, DEFAULT_MEMORY_LIMIT).unwrap();
        let hash = kernel.blob_put(&[1u8, 2, 3, 4]).unwrap();
        assert!(hash.starts_with("sha256:"));
        assert_eq!(kernel.blob_get(&hash).unwrap(), vec![1, 2, 3, 4]);
        let missing = format!("sha256:{}", "0".repeat(64));
        assert_eq!(
            kernel.blob_get(&missing).unwrap_err().code,
            "reference_not_found"
        );
        assert_eq!(kernel.stats().blobs, 1);
    }

    #[test]
    fn state_json_exposes_the_summary_used_by_the_editor() {
        let atoms = scene_atoms();
        let kernel = kernel_with(&atoms, DEFAULT_MEMORY_LIMIT);
        let summary = kernel.state_json();
        assert_eq!(summary["doc_id"], json!("doc_1"));
        assert_eq!(summary["head_seq"], json!(4));
        assert_eq!(summary["layers"], json!(2));
        assert_eq!(summary["objects"], json!(2));
        assert_eq!(summary["width"], json!(128));
    }
}

#[cfg(test)]
mod second_layer_tests {
    use super::*;
    use serde_json::json;

    fn atom(id_suffix: u8, seq: u64, kind: &str, payload: Value) -> Value {
        json!({
            "id": format!("01BBBBBBBBBBBBBBBBBBBBBBBB{id_suffix:02}"),
            "seq": seq,
            "kind": kind,
            "actor": "human:1",
            "session": "s",
            "timestamp": seq,
            "payload": payload,
        })
    }

    /// 回归：**第二个图层上的内容必须被内核渲染出来**。
    ///
    /// 实际使用中发现的缺陷：新建图层后画的内容服务端看得到、浏览器内核渲染为空白。
    /// 症状是「每次操作后画布空白，刷新/重做才可见」——因为落笔总是发生在查看器自动创建
    /// 的图层之上。这里用最小场景固定住：两个图层各画一笔，第二笔必须出现在渲染结果里。
    #[test]
    fn content_on_the_second_layer_is_rendered() {
        let mut kernel = Kernel::new("doc_1", 32, 128, 128, 8 * 1024 * 1024).unwrap();
        let atoms = [
            atom(
                1,
                1,
                "create_document",
                json!({"doc_id": "doc_1", "width": 128, "height": 128, "color_space": "srgb",
                       "background": {"r": 255, "g": 255, "b": 255, "a": 255}}),
            ),
            atom(
                2,
                2,
                "create_layer",
                json!({"layer_id": "layer_1", "name": "first"}),
            ),
            atom(
                3,
                3,
                "create_layer",
                json!({"layer_id": "layer_2", "name": "second"}),
            ),
            atom(
                4,
                4,
                "draw_stroke",
                json!({"object_id": "obj_first", "layer_id": "layer_1",
                       "data": {"points": [[10.0, 10.0], [40.0, 10.0]], "size": 6.0,
                                "color": [20, 20, 20, 255]}}),
            ),
            atom(
                5,
                5,
                "draw_stroke",
                json!({"object_id": "obj_second", "layer_id": "layer_2",
                       "data": {"points": [[70.0, 90.0], [110.0, 90.0]], "size": 8.0,
                                "color": [200, 20, 20, 255]}}),
            ),
        ];
        for value in &atoms {
            kernel
                .apply_atom_json(&value.to_string())
                .unwrap_or_else(|error| panic!("应用原子失败：{error:?}"));
        }

        let result = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .expect("渲染应成功");

        // 第二笔是红色，落在 y≈90、x∈[66,114]；统计该区域内的红色像素。
        let mut red_on_second = 0;
        for y in 80..100usize {
            for x in 60..120usize {
                let index = (y * 128 + x) * 4;
                let (r, g, b) = (
                    result.rgba8[index],
                    result.rgba8[index + 1],
                    result.rgba8[index + 2],
                );
                if r > 120 && g < 120 && b < 120 {
                    red_on_second += 1;
                }
            }
        }
        // 第一笔在 y≈10，按同样方式统计。
        let mut dark_on_first = 0;
        for y in 0..25usize {
            for x in 0..50usize {
                let index = (y * 128 + x) * 4;
                if result.rgba8[index] < 120 {
                    dark_on_first += 1;
                }
            }
        }
        assert!(
            dark_on_first > 0,
            "第一个图层上的笔迹也没渲染出来（基线不成立）"
        );
        assert!(
            red_on_second > 0,
            "第二个图层上的内容没有被渲染（第二个图层被跳过）——这是实际使用中报告的空白画布缺陷"
        );
    }
}

#[cfg(test)]
mod preview_commit_tests {
    use super::*;
    use serde_json::json;

    fn doc_and_layers() -> Vec<Value> {
        vec![
            json!({"id": "01CCCCCCCCCCCCCCCCCCCCCCCC01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_1", "width": 128, "height": 128, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01CCCCCCCCCCCCCCCCCCCCCCCC02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1", "name": "first"}}),
            json!({"id": "01CCCCCCCCCCCCCCCCCCCCCCCC03", "seq": 3, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"layer_id": "layer_2", "name": "second"}}),
        ]
    }

    fn red_pixels(result: &RenderResult, region: (usize, usize, usize, usize)) -> usize {
        let (x0, y0, x1, y1) = region;
        let width = result.width as usize;
        let mut count = 0;
        for y in y0..y1 {
            for x in x0..x1 {
                let index = (y * width + x) * 4;
                if result.rgba8[index] > 120
                    && result.rgba8[index + 1] < 120
                    && result.rgba8[index + 2] < 120
                {
                    count += 1;
                }
            }
        }
        count
    }

    /// 回归（用户实际报告）：**在第二个图层上落笔后，画布不能变空白**。
    ///
    /// 走的是浏览器真实路径：`extend_preview_stroke` 增量盖章 → `commit_preview` 落笔。
    /// 早期 `commit_preview` 假定"覆盖层像素已经盖章进 tile"因而不失效 tile ✗，
    /// 当覆盖层与提交原子不在同一图层时不成立 —— 落笔后按 tile 渲染读到旧内容。
    #[test]
    fn stroke_committed_on_the_second_layer_stays_visible() {
        let mut kernel = Kernel::new("doc_1", 32, 128, 128, 8 * 1024 * 1024).unwrap();
        for value in doc_and_layers() {
            kernel.apply_atom_json(&value.to_string()).unwrap();
        }

        // 覆盖层：在第二个图层上画一笔。
        let preview = json!({"object_id": PREVIEW_OBJECT_ID, "layer_id": "layer_2",
                             "type": "stroke",
                             "data": {"points": [[20.0, 100.0], [100.0, 100.0]], "size": 10.0,
                                      "color": {"r": 220, "g": 20, "b": 20, "a": 255}}});
        kernel
            .extend_preview_stroke(&preview.to_string())
            .expect("覆盖层盖章应成功");

        // 落笔提交（seq 由内核补 0，此处不带 seq 走本地预测路径）。
        let atom = json!({"id": "01CCCCCCCCCCCCCCCCCCCCCCCC04", "kind": "draw_stroke",
                          "actor": "human:web", "session": "session:wasm", "timestamp": 4,
                          "payload": {"object_id": "obj_second", "layer_id": "layer_2",
                                      "data": {"points": [[20.0, 100.0], [100.0, 100.0]], "size": 10.0,
                                               "color": [220, 20, 20, 255]}}});
        let report = kernel
            .commit_preview(&atom.to_string())
            .expect("落笔提交应成功");
        assert!(
            report.dirty_bbox.is_some(),
            "落笔提交必须给出脏区，否则调用方无从重绘（这正是画布空白的原因）"
        );

        let result = kernel
            .render_region(Bbox::new(0.0, 0.0, 128.0, 128.0))
            .expect("渲染应成功");
        let red = red_pixels(&result, (0, 85, 128, 115));
        assert!(
            red > 0,
            "落笔后第二个图层上的笔迹没有出现在渲染结果里（画布空白缺陷复发）"
        );
    }
}

#[cfg(test)]
mod preview_bbox_tests {
    use super::*;
    use serde_json::json;

    fn setup() -> Kernel {
        let mut kernel = Kernel::new("doc_1", 32, 128, 128, 8 * 1024 * 1024).unwrap();
        for value in [
            json!({"id": "01DDDDDDDDDDDDDDDDDDDDDDDD01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_1", "width": 128, "height": 128, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01DDDDDDDDDDDDDDDDDDDDDDDD02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1", "name": "first"}}),
            json!({"id": "01DDDDDDDDDDDDDDDDDDDDDDDD03", "seq": 3, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"layer_id": "layer_2", "name": "second"}}),
        ] {
            kernel.apply_atom_json(&value.to_string()).unwrap();
        }
        kernel
    }

    fn preview(points: Value) -> String {
        json!({"object_id": PREVIEW_OBJECT_ID, "layer_id": "layer_2", "type": "stroke",
               "data": {"points": points, "size": 10.0,
                        "color": {"r": 220, "g": 20, "b": 20, "a": 255}}})
        .to_string()
    }

    /// 查看器每次 pointermove 都会增量盖章，**每一次都必须返回可重绘的区域**。
    /// 若某次返回 None，查看器就什么都画不出来（表现为拖动中没有反馈、落笔后画布空白）。
    #[test]
    fn every_preview_stamp_reports_a_repaint_bbox() {
        let mut kernel = setup();
        let first = kernel
            .extend_preview_stroke(&preview(json!([[20.0, 100.0], [60.0, 100.0]])))
            .expect("第一次盖章");
        assert!(first.is_some(), "第一次盖章必须给出脏区");

        let second = kernel
            .extend_preview_stroke(&preview(json!([
                [20.0, 100.0],
                [60.0, 100.0],
                [100.0, 100.0]
            ])))
            .expect("第二次盖章（增量）");
        assert!(
            second.is_some(),
            "增量盖章必须给出新增笔段的脏区，否则查看器无法重绘（画布空白缺陷）"
        );

        // 第三次：点列不变（例如合成事件重复），仍应给出可重绘区域或明确无变化，但不能 panic。
        let third = kernel.extend_preview_stroke(&preview(json!([
            [20.0, 100.0],
            [60.0, 100.0],
            [100.0, 100.0]
        ])));
        assert!(third.is_ok(), "重复盖章不应失败：{third:?}");
    }
}

#[cfg(test)]
mod direct_render_tests {
    use super::*;
    use serde_json::json;

    /// 回归：**预览盖章后立刻直接渲染**必须能工作（查看器每次 pointermove 都这么做）。
    ///
    /// 实际使用时表现为「拖动中无反馈、落笔后画布空白」：`render_region_direct_rgba`
    /// 在 wasm 里 panic（JS 异常被事件处理器吞掉），于是没有任何 putImageData。
    #[test]
    fn direct_render_after_preview_on_second_layer() {
        let mut kernel = Kernel::new("doc_1", 32, 128, 128, 8 * 1024 * 1024).unwrap();
        for value in [
            json!({"id": "01EEEEEEEEEEEEEEEEEEEEEEEE01", "seq": 1, "kind": "create_document",
                   "actor": "human:1", "session": "s", "timestamp": 1,
                   "payload": {"doc_id": "doc_1", "width": 128, "height": 128, "color_space": "srgb",
                               "background": {"r": 255, "g": 255, "b": 255, "a": 255}}}),
            json!({"id": "01EEEEEEEEEEEEEEEEEEEEEEEE02", "seq": 2, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 2,
                   "payload": {"layer_id": "layer_1", "name": "first"}}),
            json!({"id": "01EEEEEEEEEEEEEEEEEEEEEEEE03", "seq": 3, "kind": "create_layer",
                   "actor": "human:1", "session": "s", "timestamp": 3,
                   "payload": {"layer_id": "layer_2", "name": "second"}}),
        ] {
            kernel.apply_atom_json(&value.to_string()).unwrap();
        }

        let preview = json!({"object_id": PREVIEW_OBJECT_ID, "layer_id": "layer_2", "type": "stroke",
                             "data": {"points": [[20.0, 100.0]], "size": 10.0,
                                      "color": {"r": 220, "g": 20, "b": 20, "a": 255}}});
        let bbox = kernel
            .extend_preview_stroke(&preview.to_string())
            .expect("盖章应成功")
            .expect("应给出脏区");

        let x = bbox.x.floor().max(0.0) as u32;
        let y = bbox.y.floor().max(0.0) as u32;
        let w = bbox.w.ceil().max(1.0) as u32;
        let h = bbox.h.ceil().max(1.0) as u32;
        let rgba = kernel
            .render_region_direct_rgba(x, y, w, h)
            .expect("直接渲染不应失败（查看器依赖它做拖动反馈）");
        assert_eq!(rgba.len(), (w * h * 4) as usize, "像素数应与区域匹配");
        let mut dark = 0;
        for index in (0..rgba.len()).step_by(4) {
            if rgba[index] < 200 || rgba[index + 1] < 200 || rgba[index + 2] < 200 {
                dark += 1;
            }
        }
        assert!(dark > 0, "直接渲染应包含刚盖章的笔迹");
    }
}
