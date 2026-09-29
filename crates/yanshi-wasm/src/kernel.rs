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
use yanshi_core::{Atom, AtomLog, Bbox, BlobHash, DocumentState, IncrementalFolder, Seq};
use yanshi_render::dirty::{plan_dirty_with_log, DirtyKind};
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

/// 把请求区域夹到画布内（至少 1×1 像素）。
fn clamp_to_canvas(region: Bbox, width: u32, height: u32) -> Bbox {
    let x = region.x.max(0.0).min((width as f64 - 1.0).max(0.0));
    let y = region.y.max(0.0).min((height as f64 - 1.0).max(0.0));
    let w = region.w.max(1.0).min(width as f64 - x).max(1.0);
    let h = region.h.max(1.0).min(height as f64 - y).max(1.0);
    Bbox::new(x, y, w, h)
}

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
            for row in 0..tile_size {
                let out_y = dy + row;
                if out_y < 0 || out_y >= height as i64 {
                    continue;
                }
                for column in 0..tile_size {
                    let out_x = dx + column;
                    if out_x < 0 || out_x >= width as i64 {
                        continue;
                    }
                    let src = ((row * tile_size + column) * 4) as usize;
                    let dst = ((out_y * width as i64 + out_x) * 4) as usize;
                    rgba8[dst..dst + 4].copy_from_slice(&tile_rgba[src..src + 4]);
                }
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
        assert_eq!(kernel.state().layers.len(), 1);

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
        assert_eq!(summary["layers"], json!(1));
        assert_eq!(summary["objects"], json!(2));
        assert_eq!(summary["width"], json!(128));
    }
}
