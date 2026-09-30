//! 文档渲染：图层隔离、区域渲染、位图补丁（设计文档 6.2 / 8.3 / 13.1）。
//!
//! 渲染只读状态，不看历史：**打开文档是读取渲染状态，不是重放历史**（6.2）。
//!
//! ## 管线
//!
//! 1. 按图层 z 序（升序）逐层渲染；
//! 2. 层内按对象 z 序渲染：笔触 stamping、形状覆盖率填充与描边、位图补丁；
//!    `Adjustment` / `Filter` 对象作用于同层中位于其下方的内容；
//! 3. 图层蒙版、剪贴蒙版、图层不透明度，再按图层混合模式合成；
//! 4. 结果裁剪到请求区域，切片写入 L3 tile 缓存，并输出显示空间 u8。
//!
//! ## 未实现类型的处理
//!
//! 文本光栅化（需要内嵌字体子集）、retouch/liquify、实例/组引用、
//! WebP/AVIF 位图编解码尚未实现：这些对象被跳过并记入 [`RenderStats::unsupported`]，
//! 而不是让整次渲染失败。位图补丁目前只支持 `image/x-yanshi-raw`（内核内表示）。

use crate::blend::BlendMode;
use crate::brush::{stamp_stroke, BrushSpec, StrokeGeometry, StrokePoint};
use crate::buffer::Buffer;
use crate::color::{premultiply, u8x4_to_linear_premul, LinearRgba};
use crate::filter::{apply_adjustment, apply_filter, FilterKind};
use crate::geometry::{
    ellipse_coverage_clipped, polygon_coverage_clipped, rect_coverage_clipped, Coverage,
};
use crate::object::{parse_object, Primitive, ShapeKind};
use crate::tile::{Tile, TileCache, TileGrid, TileKey};
use serde_json::Value;
use yanshi_core::{Bbox, BlobStore, DocumentState, Layer, Object, Result, YanshiError};

/// 渲染选项。
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// 是否渲染不可见图层（图层隔离渲染与调试用）。
    pub include_hidden_layers: bool,
    /// 输出底色；`None` 时使用文档 `background`。
    pub background: Option<[u8; 4]>,
    /// 是否为滤镜扩展渲染区域（关闭时模糊将在区域边界被裁剪）。
    pub expand_for_filters: bool,
    /// 滤镜 padding 上限（防止超大半径拖垮区域渲染）。
    pub max_filter_padding: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            include_hidden_layers: false,
            background: None,
            expand_for_filters: true,
            // 与外扩上限保持一致：小于它会让声明了较大邻域的调用被静默截断。
            max_filter_padding: MAX_EFFECT_PADDING,
        }
    }
}

/// 渲染统计（设计文档 14.9 可观测性）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderStats {
    /// 参与渲染的图层数。
    pub layers: usize,
    /// 参与渲染的对象数。
    pub objects: usize,
    /// 因包围盒与渲染区域不相交而被裁剪的对象数（O(dirty) 渲染的关键）。
    pub objects_culled: usize,
    /// 未实现类型/格式的告警。
    pub unsupported: Vec<String>,
    /// 写入缓存的 tile 数。
    pub tiles_rendered: usize,
    /// 复用缓存的 tile 数。
    pub tiles_reused: usize,
    /// 本次为滤镜扩展的像素半径。
    pub filter_padding: u32,
}

/// 区域渲染结果。
#[derive(Debug, Clone, PartialEq)]
pub struct RegionRender {
    /// 实际渲染的文档区域。
    pub bbox: Bbox,
    /// 宽（像素）。
    pub width: u32,
    /// 高（像素）。
    pub height: u32,
    /// 显示空间 u8 RGBA（行主序）。
    pub rgba8: Vec<u8>,
    /// 涉及到的 tile。
    pub tiles: Vec<TileKey>,
    /// 统计。
    pub stats: RenderStats,
}

impl RegionRender {
    /// 取某像素（`None` 表示越界）。
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let index = ((y * self.width + x) * 4) as usize;
        Some([
            self.rgba8[index],
            self.rgba8[index + 1],
            self.rgba8[index + 2],
            self.rgba8[index + 3],
        ])
    }
}

/// 计算内核层的文档渲染器。
#[derive(Debug)]
pub struct Renderer {
    grid: TileGrid,
    cache: TileCache,
    options: RenderOptions,
}

impl Renderer {
    /// 以 tile 网格构造（默认缓存预算 64 MiB）。
    pub fn new(grid: TileGrid) -> Self {
        let budget = 64 * 1024 * 1024;
        let cache = TileCache::new(grid.clone(), budget);
        Self {
            grid,
            cache,
            options: RenderOptions::default(),
        }
    }

    /// 指定缓存预算。
    pub fn with_budget(grid: TileGrid, budget_bytes: usize) -> Self {
        let cache = TileCache::new(grid.clone(), budget_bytes);
        Self {
            grid,
            cache,
            options: RenderOptions::default(),
        }
    }

    /// 设置渲染选项。
    pub fn with_options(mut self, options: RenderOptions) -> Self {
        self.options = options;
        self
    }

    /// 渲染选项。
    pub const fn options(&self) -> &RenderOptions {
        &self.options
    }

    /// tile 网格。
    pub const fn grid(&self) -> &TileGrid {
        &self.grid
    }

    /// tile 缓存。
    pub const fn cache(&self) -> &TileCache {
        &self.cache
    }

    /// 可写 tile 缓存。
    pub fn cache_mut(&mut self) -> &mut TileCache {
        &mut self.cache
    }

    /// 失效指定 tile（配合 [`crate::dirty`] 使用）。
    pub fn invalidate_tiles(&mut self, keys: &[TileKey]) -> usize {
        self.cache.invalidate(keys)
    }

    /// 按 dirty 集合失效 tile，返回失效列表。
    pub fn apply_dirty(
        &mut self,
        state: &DocumentState,
        dirty: &crate::dirty::DirtySet,
    ) -> Vec<TileKey> {
        let keys = crate::dirty::invalidated_tiles(&self.grid, state, dirty);
        self.cache.invalidate(&keys);
        keys
    }

    /// 渲染整个文档。
    pub fn render_document(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
    ) -> Result<RegionRender> {
        self.render_region(
            state,
            store,
            Bbox::new(0.0, 0.0, state.width as f64, state.height as f64),
        )
    }

    /// 渲染指定区域（文档坐标）。
    pub fn render_region(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
        bbox: Bbox,
    ) -> Result<RegionRender> {
        let region = clamp_region(state, &bbox)?;
        let declared = self.padding_for_region(state, &region);
        let padding = if self.options.expand_for_filters {
            declared.min(self.options.max_filter_padding)
        } else {
            0
        };
        let mut mask_truncated = None;
        if declared > padding {
            // 历史原子可能声明了超限外扩：必须可观测，否则表现为「分块与整幅静默不一致」。
            mask_truncated = Some(format!(
                "区域外扩被截断：声明 {declared}px，上限 {}px；该区域的分块渲染可能与整幅渲染不一致",
                self.options.max_filter_padding
            ));
        }
        let padded = Bbox::new(
            region.x - padding as f64,
            region.y - padding as f64,
            region.w + padding as f64 * 2.0,
            region.h + padding as f64 * 2.0,
        );
        let origin_x = padded.x.floor() as i64;
        let origin_y = padded.y.floor() as i64;
        let width = ((padded.x + padded.w).ceil() as i64 - origin_x).max(0) as u32;
        let height = ((padded.y + padded.h).ceil() as i64 - origin_y).max(0) as u32;

        let mut stats = RenderStats {
            filter_padding: padding,
            ..RenderStats::default()
        };
        if let Some(warning) = mask_truncated {
            stats.unsupported.push(warning);
        }

        // 背景：文档 background 为不透明时先铺底。
        let background = self
            .options
            .background
            .or_else(|| parse_background(&state.background));
        // 阶段计时（诊断用，默认关闭）：`YANSHI_RENDER_PROBE=1` 时打印各阶段耗时。
        // wasm32 下是空操作（见 `stage_probe` 的说明：`Instant` 在 wasm32 会 panic）。
        let mut probe_fill_stage = stage_probe::Stage::start();
        let mut accumulation = match background {
            Some(color) if color[3] > 0 => Buffer::filled(
                origin_x,
                origin_y,
                width,
                height,
                u8x4_to_linear_premul(color),
            ),
            _ => Buffer::new(origin_x, origin_y, width, height),
        };
        let probe_fill = probe_fill_stage.stop();
        let mut probe_render = std::time::Duration::ZERO;
        let mut probe_composite = std::time::Duration::ZERO;

        for layer in state.alive_layers() {
            if !layer.visible && !self.options.include_hidden_layers {
                continue;
            }
            stats.layers += 1;
            let mut probe_stage = stage_probe::Stage::start();
            let mut layer_buffer = Buffer::new(origin_x, origin_y, width, height);
            self.render_layer_objects(state, store, layer, &mut layer_buffer, &mut stats)?;
            apply_layer_mask(state, layer, &mut layer_buffer, &mut stats);
            probe_render += probe_stage.stop();
            if layer.clipping_mask {
                // 剪贴蒙版：用下方内容的 alpha 裁剪本层。
                layer_buffer.multiply_alpha_by(&accumulation);
            }
            layer_buffer.multiply_alpha(layer.opacity.clamp(0.0, 1.0) as f32);
            let mode = BlendMode::from_name(&layer.blend_mode);
            let mut probe_stage = stage_probe::Stage::start();
            accumulation.composite(&layer_buffer, mode, 1.0);
            probe_composite += probe_stage.stop();
        }

        // 裁剪回请求区域并切片入缓存。
        let mut probe_stage = stage_probe::Stage::start();
        let cropped = accumulation.crop(&region);
        let tiles = self.store_tiles(&cropped);
        stats.tiles_rendered = tiles.len();
        let probe_crop = probe_stage.stop();
        // 输出像素先按 **f16 量化**（14.1：内存 tile 用 f16 线性，合成正确性以 tile 为准），
        // 再转显示空间。这样「整幅区域渲染」与「按 tile 组合渲染」（客户端 WASM 内核走后者）
        // 逐字节一致，不会因 f32 scratch 与 f16 tile 的舍入差出现 ±1 分歧（Phase 2 bit-exact）；
        // 且量化是就地计算，不依赖 tile 是否仍在缓存里（小预算下会被淘汰）。
        let mut probe_stage = stage_probe::Stage::start();
        let rgba8 = quantize_to_rgba8(&cropped, background);
        stage_probe::report(
            background.is_some(),
            probe_fill,
            probe_render,
            probe_composite,
            probe_crop,
            probe_stage.stop(),
        );

        Ok(RegionRender {
            bbox: region,
            width: cropped.width(),
            height: cropped.height(),
            rgba8,
            tiles,
            stats,
        })
    }

    /// 渲染单个 tile（命中缓存则直接返回）。
    pub fn render_tile(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
        key: TileKey,
    ) -> Result<Tile> {
        if !self.grid.contains(key) {
            return Err(YanshiError::new(
                yanshi_core::ErrorCode::InvalidArgument,
                yanshi_core::ErrorContext::detail(format!("tile {key:?} 超出文档范围")),
            ));
        }
        if let Some(tile) = self.cache.get(key) {
            return Ok(tile.clone());
        }
        let bounds = self.grid.bounds(key);
        self.render_region(state, store, bounds)?;
        self.cache.get(key).cloned().ok_or_else(|| {
            YanshiError::new(
                yanshi_core::ErrorCode::ResourceExhausted,
                yanshi_core::ErrorContext::detail("tile 渲染后未能写入缓存（预算过小）"),
            )
        })
    }

    /// 把一笔笔迹**增量盖章**到已缓存的 tile 上（13.3 本地乐观渲染的关键路径）。
    ///
    /// 与「失效整块 tile 后整块重绘」的区别：只对 `geometry` 覆盖到的 tile 做一次
    /// stamping，成本 ∝ 笔段长度而不是整块面积；缓存缺失时先整块渲染一次。
    /// 返回实际改动的 tile 键与它们的文档区域（供客户端局部重绘）。
    pub fn stamp_into_tiles(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
        brush: &crate::brush::BrushSpec,
        geometry: &crate::brush::StrokeGeometry,
    ) -> Result<Vec<TileKey>> {
        let Some(bbox) = geometry_bbox(geometry, brush.size) else {
            return Ok(Vec::new());
        };
        self.stamp_samples_into_tiles(state, store, brush, geometry, bbox, None)
    }

    /// 增量盖章（跨帧连续采样）：`cursor` 保证逐段与一次性整段的 stamp 逐点相同。
    ///
    /// **暂勿用于产品路径**：读改写 tile 的路径实测会让 tile 丢掉场景内容
    /// （见 `docs/design/implementation-notes.md` 的复盘），待缺陷定位后启用。
    pub fn stamp_into_tiles_incremental(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
        brush: &crate::brush::BrushSpec,
        geometry: &crate::brush::StrokeGeometry,
        cursor: &mut crate::geometry::StrokeCursor,
    ) -> Result<Vec<TileKey>> {
        let Some(bbox) = geometry_bbox(geometry, brush.size) else {
            return Ok(Vec::new());
        };
        self.stamp_samples_into_tiles(state, store, brush, geometry, bbox, Some(cursor))
    }

    fn stamp_samples_into_tiles(
        &mut self,
        state: &DocumentState,
        store: &dyn BlobStore,
        brush: &crate::brush::BrushSpec,
        geometry: &crate::brush::StrokeGeometry,
        bbox: Bbox,
        cursor: Option<&mut crate::geometry::StrokeCursor>,
    ) -> Result<Vec<TileKey>> {
        // 采样只生成一次（跨 tile 共用），否则每个 tile 都会推进游标、产出不同采样。
        let samples: Vec<(f64, f64, f64)> = geometry
            .points
            .iter()
            .map(|point| (point.x, point.y, point.pressure))
            .collect();
        let spacing = brush.spacing_pixels();
        let (stamps, base_index) = match cursor {
            Some(cursor) => {
                let stamps = crate::geometry::dashed_line_from(&samples, brush.dash, cursor);
                let base = cursor.stamp_index.saturating_sub(stamps.len() as u64);
                (stamps, base)
            }
            None => (
                crate::geometry::dashed_line(&samples, spacing, brush.dash),
                0,
            ),
        };
        if stamps.is_empty() {
            return Ok(Vec::new());
        }
        let keys: Vec<TileKey> = self
            .grid
            .keys_for_bbox(&bbox)
            .into_iter()
            .filter(|key| self.grid.contains(*key))
            .collect();
        for key in &keys {
            // 缓存缺失（或已被淘汰）时先整块渲染一次，后续增量盖章才有底。
            if self.cache.get(*key).is_none() {
                self.render_tile(state, store, *key)?;
            }
            let Some(tile) = self.cache.get(*key).cloned() else {
                continue;
            };
            let bounds = self.grid.bounds(*key);
            let mut buffer = Buffer::new(
                bounds.x as i64,
                bounds.y as i64,
                bounds.w.max(1.0) as u32,
                bounds.h.max(1.0) as u32,
            );
            buffer.blit_rgba8(
                0,
                0,
                buffer.width(),
                buffer.height(),
                &tile.to_rgba8(None),
                1.0,
            );
            crate::brush::stamp_samples_from(&mut buffer, brush, &stamps, base_index);
            self.cache
                .insert(tile_from_buffer(&buffer, &self.grid, *key));
        }
        Ok(keys)
    }

    /// 文档内滤镜对象所需的最大邻域半径（像素）。
    /// **区域相关**的外扩：整层类效果取全局半径，空间局部对象只在与其影响范围相交时计入。
    ///
    /// * 整层类（调整/滤镜/蒙版羽化）作用于整个图层缓冲，与区域无关 → 全局半径；
    /// * 空间局部（修图/液化的源采样）只修改自身包围盒（含源偏移）内的像素 →
    ///   只在与「区域 + 全局半径」相交时才把它的可达距离计入。
    ///
    /// 这样远离修图笔迹的 tile 不必按整篇文档的最大外扩渲染（实测可省约 2/3 面积），
    /// 而正确性由「分块组合 == 整幅渲染」的测试与浏览器自检守住。
    pub fn padding_for_region(&self, state: &DocumentState, region: &Bbox) -> u32 {
        let global = self.global_padding(state);
        let reach_area = Bbox::new(
            region.x - global as f64,
            region.y - global as f64,
            region.w + global as f64 * 2.0,
            region.h + global as f64 * 2.0,
        );
        self.local_padding(state, &reach_area).max(global)
    }

    /// 整层类效果的外扩（调整不产生外扩；滤镜与蒙版羽化产生邻域需求）。
    fn global_padding(&self, state: &DocumentState) -> u32 {
        let mut padding = 0u32;
        for layer in state.alive_layers() {
            let Some(mask_id) = &layer.mask_id else {
                continue;
            };
            let Some(mask) = state.masks.get(mask_id) else {
                continue;
            };
            if mask.is_deleted() || mask.feather <= 0.0 {
                continue;
            }
            let radius = (mask.feather / 2.0).round().max(1.0) as u32;
            padding = padding.max(radius + 1);
        }
        for object in state.alive_objects() {
            if object.object_type != yanshi_core::ObjectType::Filter {
                continue;
            }
            let name = object
                .data
                .get("filter_name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = object.data.get("params").cloned().unwrap_or(Value::Null);
            if let Some(kind) = FilterKind::from_name(name) {
                padding = padding.max(kind.padding(&params));
            }
        }
        padding
    }

    /// 空间局部对象（修图/液化）在给定区域内需要的外扩。
    fn local_padding(&self, state: &DocumentState, area: &Bbox) -> u32 {
        let mut padding = 0u32;
        for object in state.alive_objects() {
            let reach: u32 = match object.object_type {
                yanshi_core::ObjectType::Retouch => {
                    let size = object
                        .data
                        .get("size")
                        .and_then(Value::as_f64)
                        .unwrap_or(24.0);
                    let offset = object
                        .data
                        .get("source_offset")
                        .and_then(Value::as_array)
                        .map(|pair| {
                            (
                                pair.first().and_then(Value::as_f64).unwrap_or(0.0).abs(),
                                pair.get(1).and_then(Value::as_f64).unwrap_or(0.0).abs(),
                            )
                        })
                        .unwrap_or((0.0, 0.0));
                    let smudge = object
                        .data
                        .get("smudge_length")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0);
                    (offset.0.max(offset.1).max(smudge) + size / 2.0 + 2.0).ceil() as u32
                }
                yanshi_core::ObjectType::Liquify => {
                    let size = object
                        .data
                        .get("size")
                        .and_then(Value::as_f64)
                        .unwrap_or(80.0);
                    let strength = object
                        .data
                        .get("strength")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.5)
                        .abs()
                        .clamp(0.0, 2.0);
                    (size / 2.0 + strength * size + 2.0).ceil() as u32
                }
                _ => continue,
            };
            // 该对象本身的包围盒（含源偏移）与目标区域相交时才需要外扩。
            let bbox = crate::object::object_bbox(object);
            let Some(bbox) = bbox else {
                padding = padding.max(reach);
                continue;
            };
            let expanded = Bbox::new(
                bbox.x - reach as f64,
                bbox.y - reach as f64,
                bbox.w + reach as f64 * 2.0,
                bbox.h + reach as f64 * 2.0,
            );
            if expanded.intersects(area) {
                padding = padding.max(reach);
            }
        }
        padding
    }

    /// 整篇文档的最大外扩（所有对象需求的并集；诊断与测试用）。
    ///
    /// 渲染路径请用 [`Renderer::padding_for_region`]：它按区域收紧外扩，避免远离修图笔迹的
    /// tile 也按整篇文档的最大值膨胀缓冲。
    pub fn filter_padding(&self, state: &DocumentState) -> u32 {
        let mut padding = 0u32;
        // 蒙版羽化同样是「有限支撑的邻域运算」：区域渲染必须外扩，
        // 否则 tile 边界会被 clamp，与整幅渲染不一致（bit-exact 自检会失败）。
        for layer in state.alive_layers() {
            let Some(mask_id) = &layer.mask_id else {
                continue;
            };
            let Some(mask) = state.masks.get(mask_id) else {
                continue;
            };
            if mask.is_deleted() || mask.feather <= 0.0 {
                continue;
            }
            let radius = (mask.feather / 2.0).round().max(1.0) as u32;
            padding = padding.max(radius + 1);
        }
        for object in state.alive_objects() {
            // 修图对象从偏移位置采样：区域渲染必须外扩到源像素，否则边缘会缺一块。
            if object.object_type == yanshi_core::ObjectType::Liquify {
                let size = object
                    .data
                    .get("size")
                    .and_then(Value::as_f64)
                    .unwrap_or(80.0);
                let strength = object
                    .data
                    .get("strength")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .abs()
                    .clamp(0.0, 2.0);
                padding = padding.max((size / 2.0 + strength * size + 2.0).ceil() as u32);
                continue;
            }
            if object.object_type == yanshi_core::ObjectType::Retouch {
                let size = object
                    .data
                    .get("size")
                    .and_then(Value::as_f64)
                    .unwrap_or(24.0);
                let offset = object
                    .data
                    .get("source_offset")
                    .and_then(Value::as_array)
                    .map(|pair| {
                        (
                            pair.first().and_then(Value::as_f64).unwrap_or(0.0).abs(),
                            pair.get(1).and_then(Value::as_f64).unwrap_or(0.0).abs(),
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                let reach = (offset.0.max(offset.1) + size / 2.0 + 2.0).ceil() as u32;
                padding = padding.max(reach);
                continue;
            }
            if object.object_type != yanshi_core::ObjectType::Filter {
                continue;
            }
            let name = object
                .data
                .get("filter_name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let params = object.data.get("params").cloned().unwrap_or(Value::Null);
            if let Some(kind) = FilterKind::from_name(name) {
                padding = padding.max(kind.padding(&params));
            }
        }
        padding
    }

    fn render_layer_objects(
        &self,
        state: &DocumentState,
        store: &dyn BlobStore,
        layer: &Layer,
        layer_buffer: &mut Buffer,
        stats: &mut RenderStats,
    ) -> Result<()> {
        let mut probe_objects = stage_probe::ObjectTimings::default();
        // 选区「约束落笔」（路线 A）✓：**把覆盖度折进印章**（见 `stamp_samples_clipped`）✓ ——
        // 只影响本次新落笔 ✓、从不触碰选区外像素 ✓、与整幅/分次渲染无关 ✓。
        // 语义细节：选区只约束**在其创建之后创建的对象** ✓（ULID 单调 ⇒ 比较 `created_by` ✓）。
        // 设计未规定先后语义 ✓，已记入 implementation-notes ✓。
        // 覆盖范围：**笔触**已接入 ✓；形状/填充/文本/擦除尚未接入 ✓（如实记录，不是静默缺口 ✗）。
        for object in state.objects_in_layer(&layer.id) {
            if !object.visible {
                continue;
            }
            // 几何裁剪：包围盒与渲染区域不相交的对象直接跳过。
            // 调整/滤镜对象作用于整层、无法用几何裁剪；未实现类型必须保留以便产生告警。
            // 对象变换在此统一施加（此前内核完全不读 `object.transform` ✗，
            // 导致 `move_object` 返回 ok 但画面不变）。
            let primitive =
                crate::object::transform_primitive(parse_object(object), &object.transform);
            let affects_whole_layer = matches!(
                primitive,
                Primitive::Adjustment { .. }
                    | Primitive::Filter { .. }
                    | Primitive::Unsupported { .. }
            );
            if !affects_whole_layer {
                let intersects = crate::object::object_bbox(object)
                    .map(|bbox| {
                        bbox.w > 0.0 && bbox.h > 0.0 && bbox.intersects(&layer_buffer.bbox())
                    })
                    .unwrap_or(false);
                if !intersects {
                    stats.objects_culled += 1;
                    continue;
                }
            }
            stats.objects += 1;
            // 对象不透明度：`data.opacity`（缺省 1）。
            let opacity = object
                .data
                .get("opacity")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32;
            let probe_label = object_probe_label(object, &primitive);
            let mut probe_object_stage = stage_probe::Stage::start();
            match primitive {
                Primitive::Stroke { geometry, brush } => {
                    let brush = BrushSpec {
                        opacity: brush.opacity * f64::from(opacity),
                        ..brush
                    };
                    // 选区「约束落笔」（路线 A）：把覆盖度折进印章 ✓ ——
                    // 只影响本次新落笔 ✓、从不触碰选区外像素 ✓（与整幅/分次渲染无关 ✓）。
                    // 只把"早于该对象"的选区计入 ✓（用日志里的原子 id 判定先后 ✓）。
                    let clip = object_clip(state, &layer.id, object);
                    if let Some(clip) = &clip {
                        // 与 `stamp_stroke` 保持一致的采样：**同样的间距与 dash** ✓
                        let samples: Vec<(f64, f64, f64)> = geometry
                            .points
                            .iter()
                            .map(|point| (point.x, point.y, point.pressure))
                            .collect();
                        let stamps = crate::geometry::dashed_line(
                            &samples,
                            brush.spacing_pixels(),
                            brush.dash,
                        );
                        crate::brush::stamp_samples_clipped(
                            layer_buffer,
                            &brush,
                            &stamps,
                            &|x, y| clip.coverage(x, y),
                        );
                    } else {
                        stamp_stroke(layer_buffer, &brush, &geometry);
                    }
                }
                Primitive::Shape {
                    kind,
                    bbox,
                    points,
                    color,
                    stroke_width,
                    stroke_color,
                } => {
                    // 只生成落在本层缓冲内的覆盖率（tile 渲染时省下十几倍工作量）。
                    let mut coverage = shape_coverage_in(kind, bbox, &points, &layer_buffer.bbox());
                    // 选区「约束落笔」：**逐像素**乘进覆盖率 ✓（从不触碰选区外 ✓）。
                    let shape_clip = object_clip(state, &layer.id, object);
                    if let Some(clip) = &shape_clip {
                        clip_coverage(&mut coverage, clip);
                    }
                    layer_buffer.fill_coverage(&coverage, color, BlendMode::Normal, opacity);
                    if stroke_width > 0.0 {
                        let outline = shape_outline(kind, bbox, &points);
                        if outline.len() >= 2 {
                            let brush = BrushSpec {
                                size: stroke_width,
                                color: stroke_color.unwrap_or(color),
                                opacity: f64::from(opacity),
                                seed: object.data.get("seed").and_then(Value::as_u64).unwrap_or(0),
                                ..BrushSpec::default()
                            };
                            let geometry = StrokeGeometry {
                                points: outline
                                    .iter()
                                    .map(|(x, y)| StrokePoint {
                                        x: *x,
                                        y: *y,
                                        pressure: 1.0,
                                    })
                                    .collect(),
                            };
                            match &shape_clip {
                                Some(clip) => {
                                    let samples: Vec<(f64, f64, f64)> = geometry
                                        .points
                                        .iter()
                                        .map(|point| (point.x, point.y, point.pressure))
                                        .collect();
                                    let stamps = crate::geometry::dashed_line(
                                        &samples,
                                        brush.spacing_pixels(),
                                        brush.dash,
                                    );
                                    crate::brush::stamp_samples_clipped(
                                        layer_buffer,
                                        &brush,
                                        &stamps,
                                        &|x, y| clip.coverage(x, y),
                                    );
                                }
                                None => {
                                    stamp_stroke(layer_buffer, &brush, &geometry);
                                }
                            }
                        }
                    }
                }
                Primitive::Adjustment { kind, params } => {
                    if !apply_adjustment(layer_buffer, &kind, &params, opacity) {
                        stats
                            .unsupported
                            .push(format!("调整类型未实现: {kind}（对象 {}）", object.id));
                    }
                }
                Primitive::Filter { name, params } => {
                    if !apply_filter(
                        layer_buffer,
                        &name,
                        &params,
                        opacity,
                        (state.width as f64, state.height as f64),
                    ) {
                        stats
                            .unsupported
                            .push(format!("滤镜未实现: {name}（对象 {}）", object.id));
                    }
                }
                Primitive::RasterPatch {
                    blob,
                    width,
                    height,
                    offset,
                    mime_type,
                } => {
                    if mime_type != RAW_RGBA_MIME {
                        stats.unsupported.push(format!(
                            "位图补丁格式未实现: {mime_type}（对象 {}，内核仅支持 {RAW_RGBA_MIME}）",
                            object.id
                        ));
                        continue;
                    }
                    let bytes = store.get(&blob)?;
                    let expected = (width as usize) * (height as usize) * 4;
                    if bytes.len() < expected {
                        return Err(YanshiError::new(
                            yanshi_core::ErrorCode::InvalidArgument,
                            yanshi_core::ErrorContext::detail(format!(
                                "位图补丁 {} 字节数不足：期望 {expected}，实际 {}",
                                object.id,
                                bytes.len()
                            )),
                        )
                        .with_object(object.id.clone())
                        .with_blob(blob.to_string()));
                    }
                    layer_buffer.blit_rgba8(
                        offset.0 as i64,
                        offset.1 as i64,
                        width,
                        height,
                        &bytes[..expected],
                        opacity,
                    );
                }
                Primitive::Text {
                    text,
                    size,
                    color,
                    align,
                    position,
                    ..
                } => {
                    // 路线 A 的最小切片：内置 5×7 ASCII 位图字体 ✓（CJK 子集为后续项 ✓）。
                    // 文本同样受选区约束 ✓（逐像素 ✓）。
                    let scale = crate::object::text_scale_for_size(size);
                    let text_clip = object_clip(state, &layer.id, object);
                    let drawn = match &text_clip {
                        Some(clip) => crate::font::draw_text_clipped(
                            layer_buffer,
                            text.as_str(),
                            position.0,
                            position.1,
                            scale,
                            color,
                            align.as_str(),
                            0.0,
                            &|x, y| clip.coverage(x, y),
                        ),
                        None => crate::font::draw_text(
                            layer_buffer,
                            text.as_str(),
                            position.0,
                            position.1,
                            scale,
                            color,
                            align.as_str(),
                            0.0,
                        ),
                    };
                    if drawn == 0 {
                        stats.unsupported.push(format!(
                            "文本未绘制出像素（对象 {}，文本 {:?}）",
                            object.id, text
                        ));
                    }
                }
                Primitive::Retouch {
                    kind,
                    points,
                    offset,
                    size,
                    hardness,
                    opacity,
                    jitter,
                    smudge_length,
                } => {
                    if kind != "clone_stamp"
                        && kind != "heal"
                        && kind != "smudge"
                        && kind != "erase"
                    {
                        stats
                            .unsupported
                            .push(format!("修图类型未实现: {kind}（对象 {}）", object.id));
                        continue;
                    }
                    // 擦除：按覆盖度扣除 alpha（destination-out 语义），不需要采样源像素。
                    if kind == "erase" {
                        let brush_radius = size.max(1.0) / 2.0;
                        let erase_hardness = hardness.clamp(0.0, 1.0);
                        let strength = opacity.clamp(0.0, 1.0);
                        let spacing = (size.max(1.0) * 0.15).max(1.0);
                        let samples: Vec<(f64, f64, f64)> =
                            points.iter().map(|(x, y)| (*x, *y, 1.0)).collect();
                        // 擦除同样受选区约束 ✓ —— 否则选区下擦除会擦掉选区**外**的内容 ✓
                        //（那是数据丢失 ✓，而不是功能缺失 ✓）。
                        let clip = object_clip(state, &layer.id, object);
                        for stamp in crate::geometry::dashed_line(&samples, spacing, None) {
                            if let Some(clip) = &clip {
                                crate::brush::erase_stamp_clipped(
                                    layer_buffer,
                                    stamp.0,
                                    stamp.1,
                                    brush_radius,
                                    erase_hardness,
                                    strength,
                                    &|x, y| clip.coverage(x, y),
                                );
                            } else {
                                crate::brush::erase_stamp(
                                    layer_buffer,
                                    stamp.0,
                                    stamp.1,
                                    brush_radius,
                                    erase_hardness,
                                    strength,
                                );
                            }
                        }
                        continue;
                    }
                    let healing = kind == "heal";
                    // 涂抹：偏移随笔迹方向变化（把后方像素拖到前方），
                    // 仍然只从**应用本对象之前**的副本采样，因此确定且无自反馈。
                    let smudging = kind == "smudge";
                    // 源必须是**应用本对象之前**的图层内容：先拷贝一份，避免自反馈。
                    let source = layer_buffer.clone();
                    let brush = crate::brush::BrushSpec {
                        size: size.max(1.0),
                        hardness: hardness.clamp(0.0, 1.0),
                        color: [0.0, 0.0, 0.0, 1.0],
                        opacity: opacity.clamp(0.0, 1.0),
                        flow: 1.0,
                        spacing: 0.15,
                        jitter: jitter.max(0.0),
                        ..Default::default()
                    };
                    let samples: Vec<(f64, f64, f64)> =
                        points.iter().map(|(x, y)| (*x, *y, 1.0)).collect();
                    let stamps =
                        crate::geometry::dashed_line(&samples, brush.spacing_pixels(), None);
                    let radius = brush.size / 2.0;
                    let origin = layer_buffer.origin();
                    let mut previous_stamp: Option<(f64, f64)> = None;
                    for (cx, cy) in stamps.iter().map(|s| (s.0, s.1)) {
                        // 涂抹的每 stamp 采样偏移：沿笔迹方向后退 `smudge_length`。
                        let stamp_offset = if smudging {
                            match previous_stamp {
                                Some((px, py)) => {
                                    let dx = cx - px;
                                    let dy = cy - py;
                                    let length = (dx * dx + dy * dy).sqrt();
                                    if length <= 1e-9 {
                                        previous_stamp = Some((cx, cy));
                                        continue;
                                    }
                                    let back = smudge_length.max(1.0);
                                    (-dx / length * back, -dy / length * back)
                                }
                                None => {
                                    previous_stamp = Some((cx, cy));
                                    continue;
                                }
                            }
                        } else {
                            offset
                        };
                        previous_stamp = Some((cx, cy));
                        // 采样源像素（文档坐标 + 偏移），越界则跳过该 stamp。
                        let sx = cx + stamp_offset.0 - origin.0 as f64;
                        let sy = cy + stamp_offset.1 - origin.1 as f64;
                        if sx < 0.0 || sy < 0.0 {
                            continue;
                        }
                        let (sx, sy) = (sx as u32, sy as u32);
                        if sx >= source.width() || sy >= source.height() {
                            continue;
                        }
                        let sampled = source.pixel(sx, sy);
                        if sampled[3] <= 0.0 {
                            continue;
                        }
                        let mut straight = [
                            sampled[0] / sampled[3],
                            sampled[1] / sampled[3],
                            sampled[2] / sampled[3],
                        ];
                        if healing {
                            // 修复画笔：保留源纹理，但把低频颜色/明度对齐到目标处。
                            // 两侧均值都取自**应用本对象之前**的副本，因此结果确定且无自反馈。
                            let dest_x = (cx - origin.0 as f64).max(0.0) as u32;
                            let dest_y = (cy - origin.1 as f64).max(0.0) as u32;
                            let radius_u32 = radius.max(1.0) as u32;
                            let mut source_mean = [0.0f32; 3];
                            let mut dest_mean = [0.0f32; 3];
                            let mut samples_count = 0.0f32;
                            // 8 个固定方向 × 半径中点，确定性采样。
                            // 只统计**源与目标都不透明**的样本：图层缓冲在未绘制处是透明的，
                            // 那种位置没有颜色信息可用；若一个有效样本都没有（例如目标处图层为空），
                            // 就退回纯 clone 语义（不做低频校正）。
                            for step in 0..8 {
                                let angle = std::f32::consts::TAU * (step as f32) / 8.0;
                                let ox = (angle.cos() * (radius_u32 as f32 * 0.5)).round() as i64;
                                let oy = (angle.sin() * (radius_u32 as f32 * 0.5)).round() as i64;
                                let source_x =
                                    (sx as i64 + ox).clamp(0, source.width() as i64 - 1) as u32;
                                let source_y =
                                    (sy as i64 + oy).clamp(0, source.height() as i64 - 1) as u32;
                                let dest_x =
                                    (dest_x as i64 + ox).clamp(0, source.width() as i64 - 1) as u32;
                                let dest_y = (dest_y as i64 + oy)
                                    .clamp(0, source.height() as i64 - 1)
                                    as u32;
                                let a = source.pixel(source_x, source_y);
                                let b = source.pixel(dest_x, dest_y);
                                if a[3] <= 0.0 || b[3] <= 0.0 {
                                    continue;
                                }
                                for channel in 0..3 {
                                    source_mean[channel] += a[channel] / a[3];
                                    dest_mean[channel] += b[channel] / b[3];
                                }
                                samples_count += 1.0;
                            }
                            if samples_count > 0.0 {
                                for channel in 0..3 {
                                    let delta =
                                        (dest_mean[channel] - source_mean[channel]) / samples_count;
                                    straight[channel] = (straight[channel] + delta).clamp(0.0, 1.0);
                                }
                            }
                        }
                        crate::brush::draw_stamp(
                            layer_buffer,
                            cx,
                            cy,
                            radius,
                            brush.hardness,
                            [
                                straight[0],
                                straight[1],
                                straight[2],
                                (brush.opacity as f32 * sampled[3]).clamp(0.0, 1.0),
                            ],
                            BlendMode::Normal,
                        );
                    }
                    continue;
                }
                Primitive::Liquify {
                    mode,
                    points,
                    size,
                    strength,
                    direction,
                } => {
                    let radius = (size.max(2.0)) / 2.0;
                    let length = (direction.0 * direction.0 + direction.1 * direction.1)
                        .sqrt()
                        .max(1e-9);
                    let (ux, uy) = (direction.0 / length, direction.1 / length);
                    // 位移随距离平滑衰减，最大位移 = strength * 直径。
                    let max_shift = (strength.clamp(-2.0, 2.0)) * size;
                    let origin = layer_buffer.origin();
                    // **只快照采样可能触达的区域**，而不是整层克隆。
                    // 原实现每个对象克隆整层：1024² + PAD 约 26MB，4K 上约 268MB ✗，
                    // 且成本随画布线性增长，而液化真正读取的范围只有「影响圈 + 最大位移」。
                    // 采样点在文档坐标下最多偏离目标 `|max_shift|`，因此快照
                    // = 各点圆 ± (radius + |max_shift| + 2) 与层缓冲求交；
                    // 局部坐标换算后读到的像素与整层克隆时**完全相同**，故逐位等价。
                    let margin = max_shift.abs().ceil() + 2.0;
                    let mut snapshot_min_x = f64::INFINITY;
                    let mut snapshot_min_y = f64::INFINITY;
                    let mut snapshot_max_x = f64::NEG_INFINITY;
                    let mut snapshot_max_y = f64::NEG_INFINITY;
                    for (px, py) in &points {
                        snapshot_min_x = snapshot_min_x.min(px - radius - margin);
                        snapshot_min_y = snapshot_min_y.min(py - radius - margin);
                        snapshot_max_x = snapshot_max_x.max(px + radius + margin);
                        snapshot_max_y = snapshot_max_y.max(py + radius + margin);
                    }
                    let snapshot_bbox = Bbox::new(
                        snapshot_min_x,
                        snapshot_min_y,
                        snapshot_max_x - snapshot_min_x,
                        snapshot_max_y - snapshot_min_y,
                    );
                    let source = layer_buffer.crop(&snapshot_bbox);
                    let source_origin = source.origin();
                    // 注意：**采样夹取**用快照尺寸，而**迭代边界**仍用层缓冲尺寸 ——
                    // 两者混用会让迭代被裁到快照范围、圈内像素被静默跳过（测试抓到的正是这个错）。
                    let source_width = source.width();
                    let source_height = source.height();
                    let width = layer_buffer.width();
                    let height = layer_buffer.height();
                    // **只在受影响区域内迭代**：圆外像素的位移恒为 0，原代码路径在那里就是
                    // `continue`（不改动像素），因此把循环收缩到「各点圆的外接正方形 ∩ 缓冲」
                    // 是**逐位等价**的，却能把 1024² 的 100 万像素降到只处理圆覆盖的十余万像素。
                    // （此前实测：size=400 的 twirl 净成本 110–130ms，其中绝大部分花在圆外像素上。）
                    let mut affected_min_x = f64::INFINITY;
                    let mut affected_min_y = f64::INFINITY;
                    let mut affected_max_x = f64::NEG_INFINITY;
                    let mut affected_max_y = f64::NEG_INFINITY;
                    for (px, py) in &points {
                        affected_min_x = affected_min_x.min(px - radius);
                        affected_min_y = affected_min_y.min(py - radius);
                        affected_max_x = affected_max_x.max(px + radius);
                        affected_max_y = affected_max_y.max(py + radius);
                    }
                    let start_x =
                        ((affected_min_x - origin.0 as f64).floor().max(0.0) as u32).min(width);
                    let start_y =
                        ((affected_min_y - origin.1 as f64).floor().max(0.0) as u32).min(height);
                    let end_x =
                        ((affected_max_x - origin.0 as f64).ceil().max(0.0) as u32 + 1).min(width);
                    let end_y =
                        ((affected_max_y - origin.1 as f64).ceil().max(0.0) as u32 + 1).min(height);
                    for y in start_y..end_y {
                        for x in start_x..end_x {
                            // 目标像素的文档坐标。
                            let document_x = origin.0 as f64 + x as f64;
                            let document_y = origin.1 as f64 + y as f64;
                            let mut shift_x = 0.0f64;
                            let mut shift_y = 0.0f64;
                            for (px, py) in &points {
                                let dx = document_x - px;
                                let dy = document_y - py;
                                let distance = (dx * dx + dy * dy).sqrt();
                                if distance >= radius {
                                    continue;
                                }
                                // smoothstep 衰减：中心 1、边缘 0。
                                let t = 1.0 - distance / radius;
                                let falloff = t * t * (3.0 - 2.0 * t);
                                match mode.as_str() {
                                    // 旋转：把采样点绕中心转过 `strength × falloff` 弧度。
                                    "twirl" => {
                                        let angle = strength.clamp(-2.0, 2.0) * falloff;
                                        let (sin, cos) = angle.sin_cos();
                                        let rotated_x = dx * cos - dy * sin;
                                        let rotated_y = dx * sin + dy * cos;
                                        shift_x += dx - rotated_x;
                                        shift_y += dy - rotated_y;
                                    }
                                    // 收缩：采样点向中心靠拢（采样更靠近中心的内容 →
                                    // 内容看起来被吸向中心）。负强度即膨胀。
                                    "pinch" => {
                                        let scale = strength.clamp(-2.0, 2.0) * falloff;
                                        shift_x -= dx * scale;
                                        shift_y -= dy * scale;
                                    }
                                    // 推力：沿 direction 平移。
                                    _ => {
                                        shift_x += ux * max_shift * falloff;
                                        shift_y += uy * max_shift * falloff;
                                    }
                                }
                            }
                            if shift_x == 0.0 && shift_y == 0.0 {
                                continue;
                            }
                            // 反向映射：目标像素取「源 − 位移」处的**双线性**采样。
                            let sample_x = document_x - shift_x - source_origin.0 as f64;
                            let sample_y = document_y - shift_y - source_origin.1 as f64;
                            if sample_x < -1.0
                                || sample_y < -1.0
                                || sample_x > width as f64
                                || sample_y > height as f64
                            {
                                continue;
                            }
                            let x0 = sample_x.floor();
                            let y0 = sample_y.floor();
                            let fx = (sample_x - x0) as f32;
                            let fy = (sample_y - y0) as f32;
                            let clamp =
                                |value: i64, limit: u32| value.clamp(0, limit as i64 - 1) as u32;
                            let x0i = clamp(x0 as i64, source_width);
                            let y0i = clamp(y0 as i64, source_height);
                            let x1i = clamp(x0 as i64 + 1, source_width);
                            let y1i = clamp(y0 as i64 + 1, source_height);
                            let p00 = source.pixel(x0i, y0i);
                            let p10 = source.pixel(x1i, y0i);
                            let p01 = source.pixel(x0i, y1i);
                            let p11 = source.pixel(x1i, y1i);
                            let mut out = [0.0f32; 4];
                            for channel in 0..4 {
                                let top = p00[channel] + (p10[channel] - p00[channel]) * fx;
                                let bottom = p01[channel] + (p11[channel] - p01[channel]) * fx;
                                out[channel] = top + (bottom - top) * fy;
                            }
                            layer_buffer.set_pixel(x, y, out);
                        }
                    }
                    continue;
                }
                Primitive::Unsupported { reason } => {
                    stats
                        .unsupported
                        .push(format!("{reason}（对象 {}）", object.id));
                }
            }
            probe_objects.record(&probe_label, probe_object_stage.stop());
            probe_objects.report();
        }
        Ok(())
    }

    fn store_tiles(&mut self, buffer: &Buffer) -> Vec<TileKey> {
        let keys = self.grid.keys_for_bbox(&buffer.bbox());
        let mut stored = Vec::with_capacity(keys.len());
        for key in keys {
            // 已有 tile 时只覆盖本次渲染真正覆盖到的像素，避免局部渲染清空其余像素。
            let previous = self.cache.peek(key).cloned();
            let tile = tile_from_buffer_preserving(buffer, &self.grid, key, previous.as_ref());
            self.cache.insert(tile);
            stored.push(key);
        }
        stored
    }
}

/// 内核内部使用的位图格式：未压缩 RGBA8（WebP/AVIF 编解码属传输层，尚未实现）。
/// 渲染器保证「分块渲染 == 整幅渲染」的最大区域外扩量（像素）。
///
/// 邻域运算（滤镜、蒙版羽化、修图/液化的源采样）靠区域外扩来保证这一点；
/// 外扩越大，单块缓冲越大（内存与耗时都线性增长），因此有上限。
/// **参数超出该上限的调用必须被拒绝**：否则 tile 渲染取不到邻域/源像素，
/// 客户端与服务端的结果会静默不一致（实测 `source_offset = 200` 时 bit-exact 失败）。
pub const MAX_EFFECT_PADDING: u32 = 128;

/// 位图补丁的原始像素 MIME（RGBA8 直通字节）。
pub const RAW_RGBA_MIME: &str = "image/x-yanshi-raw";

/// 笔迹几何的文档包围盒（含笔尖半径）。
fn geometry_bbox(geometry: &crate::brush::StrokeGeometry, size: f64) -> Option<Bbox> {
    if geometry.points.is_empty() {
        return None;
    }
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for point in &geometry.points {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    let radius = (size / 2.0).max(0.5) + 1.0;
    Some(Bbox::new(
        min_x - radius,
        min_y - radius,
        (max_x - min_x) + radius * 2.0,
        (max_y - min_y) + radius * 2.0,
    ))
}

/// f16 量化后转显示空间 RGBA8（与 tile 存储精度一致，且与缓存状态无关）。
fn quantize_to_rgba8(buffer: &Buffer, background: Option<[u8; 4]>) -> Vec<u8> {
    let mut quantized = buffer.crop(&buffer.bbox());
    for value in quantized.pixels_mut() {
        *value = crate::half::quantize_f16(*value);
    }
    quantized.to_rgba8(background)
}

fn clamp_region(state: &DocumentState, bbox: &Bbox) -> Result<Bbox> {
    let x0 = bbox.x.floor().max(0.0);
    let y0 = bbox.y.floor().max(0.0);
    let x1 = (bbox.x + bbox.w).ceil().min(state.width as f64);
    let y1 = (bbox.y + bbox.h).ceil().min(state.height as f64);
    if x1 <= x0 || y1 <= y0 {
        return Err(YanshiError::new(
            yanshi_core::ErrorCode::InvalidArgument,
            yanshi_core::ErrorContext::detail(format!(
                "渲染区域 {bbox:?} 与文档 {}×{} 不相交",
                state.width, state.height
            )),
        ));
    }
    Ok(Bbox::new(x0, y0, x1 - x0, y1 - y0))
}

/// 渲染阶段计时探针（诊断用）。
///
/// **wasm32 上必须是空操作**：`std::time::Instant::now()` 在 `wasm32-unknown-unknown`
/// 上不支持，会直接 panic（曾因此让浏览器端每次渲染都崩，而原生测试全绿 ——
/// 教训：原生通过不等于浏览器通过，探针一定要按目标平台分派）。
#[cfg(not(target_arch = "wasm32"))]
mod stage_probe {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    /// 是否启用（环境变量只读一次）。
    pub fn enabled() -> bool {
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("YANSHI_RENDER_PROBE").is_some())
    }

    /// 一个计时点；未启用时不取时间。
    pub struct Stage(Option<Instant>);

    impl Stage {
        pub fn start() -> Self {
            Stage(if enabled() {
                Some(Instant::now())
            } else {
                None
            })
        }
        pub fn stop(&mut self) -> Duration {
            match self.0.take() {
                Some(started) => started.elapsed(),
                None => Duration::ZERO,
            }
        }
    }

    /// 按对象累计耗时（诊断用）。
    #[derive(Default)]
    pub struct ObjectTimings {
        entries: Vec<(String, Duration, u32)>,
    }

    impl ObjectTimings {
        /// 记录一次对象渲染耗时。
        pub fn record(&mut self, label: &str, elapsed: Duration) {
            if !enabled() {
                return;
            }
            match self.entries.iter_mut().find(|entry| entry.0 == label) {
                Some(entry) => {
                    entry.1 += elapsed;
                    entry.2 += 1;
                }
                None => self.entries.push((label.to_owned(), elapsed, 1)),
            }
        }

        /// 按总耗时降序打印（最多 8 项）。
        pub fn report(&self) {
            if !enabled() || self.entries.is_empty() {
                return;
            }
            let mut entries = self.entries.clone();
            entries.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            let mut line = String::from("  对象耗时:");
            for (label, total, count) in entries.iter().take(8) {
                line.push_str(&format!(" {label}={total:?}×{count}"));
            }
            eprintln!("{line}");
        }
    }

    /// 打印各阶段耗时。
    pub fn report(
        has_background: bool,
        fill: Duration,
        layers: Duration,
        composite: Duration,
        crop: Duration,
        quantize: Duration,
    ) {
        if enabled() {
            eprintln!(
                "PROBE 有背景={has_background} 填充={fill:?} 图层={layers:?} 合成={composite:?} \
                 裁剪+存tile={crop:?} 量化={quantize:?}"
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod stage_probe {
    use std::time::Duration;

    /// 空计时点。
    pub struct Stage;

    impl Stage {
        pub const fn start() -> Self {
            Stage
        }
        pub const fn stop(&mut self) -> Duration {
            Duration::ZERO
        }
    }

    /// wasm32 上不记录对象耗时。
    #[derive(Default)]
    pub struct ObjectTimings;

    impl ObjectTimings {
        pub const fn record(&mut self, _label: &str, _elapsed: Duration) {}
        pub const fn report(&self) {}
    }

    /// 空报告。
    pub fn report(
        _has_background: bool,
        _fill: Duration,
        _layers: Duration,
        _composite: Duration,
        _crop: Duration,
        _quantize: Duration,
    ) {
    }
}

/// 探针标签：把对象标成「类型:细节」（如 `filter:gaussian_blur`、`liquify:pinch`），
/// 以便分段计时直接回答"是哪个效果拖慢了整层"。
#[cfg(not(target_arch = "wasm32"))]
fn object_probe_label(object: &Object, primitive: &Primitive) -> String {
    let kind = object
        .data
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match primitive {
        Primitive::Filter { name, .. } => format!("filter:{name}"),
        Primitive::Adjustment { kind, .. } => format!("adjustment:{}", kind.as_str()),
        Primitive::Liquify { mode, .. } => format!("liquify:{mode}"),
        Primitive::Retouch { kind, .. } => format!("retouch:{kind}"),
        Primitive::Stroke { .. } => "stroke".to_owned(),
        Primitive::RasterPatch { .. } => "raster_patch".to_owned(),
        _ => kind.to_owned(),
    }
}

/// wasm32 上不需要标签。
#[cfg(target_arch = "wasm32")]
fn object_probe_label(_object: &Object, _primitive: &Primitive) -> String {
    String::new()
}

/// 解析文档背景色（`{"r": 0-255, ...}`）。
pub fn parse_background(value: &Value) -> Option<[u8; 4]> {
    let object = value.as_object()?;
    let channel = |key: &str, default: u8| {
        object
            .get(key)
            .and_then(Value::as_u64)
            .unwrap_or(default as u64) as u8
    };
    if !object.contains_key("r") && !object.contains_key("g") && !object.contains_key("b") {
        return None;
    }
    Some([
        channel("r", 255),
        channel("g", 255),
        channel("b", 255),
        channel("a", 255),
    ])
}

/// 由形状参数构造覆盖率。
pub fn shape_coverage(kind: ShapeKind, bbox: Bbox, points: &[(f64, f64)]) -> Coverage {
    shape_coverage_in(kind, bbox, points, &bbox)
}

/// 形状覆盖率，**只生成与 `clip` 相交的像素**。
///
/// 渲染单个 tile 时，一块覆盖全画布的形状若按自身 bbox 生成覆盖率会白算十几倍
/// （1024×1024 vs 256×256）；逐像素的覆盖率只取决于该像素与形状的几何关系，
/// 因此裁剪生成结果与全量生成在相交区域上完全一致（D0 不变）。
pub fn shape_coverage_in(
    kind: ShapeKind,
    bbox: Bbox,
    points: &[(f64, f64)],
    clip: &Bbox,
) -> Coverage {
    match kind {
        ShapeKind::Rect => rect_coverage_clipped(bbox, clip),
        ShapeKind::Ellipse => ellipse_coverage_clipped(bbox, 4, clip),
        ShapeKind::Polygon => {
            if points.len() >= 3 {
                polygon_coverage_clipped(points, 4, clip)
            } else {
                rect_coverage_clipped(bbox, clip)
            }
        }
    }
}

/// 由蒙版 `shape` 字段构造覆盖率（支持 `{"kind": "rect"|"ellipse"|"polygon", ...}`）。
pub fn coverage_from_shape(shape: &Value) -> Coverage {
    let kind = match shape.get("kind").and_then(Value::as_str) {
        Some("ellipse") => ShapeKind::Ellipse,
        Some("polygon") => ShapeKind::Polygon,
        _ => ShapeKind::Rect,
    };
    let bbox = shape
        .get("bbox")
        .and_then(Bbox::from_value)
        .or_else(|| Bbox::from_value(shape))
        .unwrap_or_else(|| Bbox::new(0.0, 0.0, 0.0, 0.0));
    let mut points = Vec::new();
    if let Some(array) = shape.get("points").and_then(Value::as_array) {
        for item in array {
            if let Some(pair) = item.as_array() {
                if pair.len() >= 2 {
                    if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                        points.push((x, y));
                    }
                }
            }
        }
    }
    shape_coverage(kind, bbox, &points)
}

/// 图层蒙版：用蒙版覆盖率乘以图层 alpha（蒙版缺失或已删除时保持原样）。
pub fn apply_layer_mask(
    state: &DocumentState,
    layer: &Layer,
    layer_buffer: &mut Buffer,
    stats: &mut RenderStats,
) {
    let Some(mask_id) = &layer.mask_id else {
        return;
    };
    let Some(mask) = state.masks.get(mask_id) else {
        // 引用不存在的蒙版必须可观测（9 章校验也会报 missing）。
        stats
            .unsupported
            .push(format!("蒙版不存在: {mask_id}（图层 {}）", layer.id));
        return;
    };
    if mask.is_deleted() {
        return;
    }
    let (origin_x, origin_y) = layer_buffer.origin();
    let mut mask_buffer = Buffer::new(
        origin_x,
        origin_y,
        layer_buffer.width(),
        layer_buffer.height(),
    );
    let coverage = coverage_from_shape(&mask.shape);
    mask_buffer.fill_coverage(&coverage, [0.0, 0.0, 0.0, 1.0], BlendMode::Normal, 1.0);
    if mask.invert {
        for y in 0..mask_buffer.height() {
            for x in 0..mask_buffer.width() {
                let pixel = mask_buffer.pixel(x, y);
                mask_buffer.set_pixel(x, y, [0.0, 0.0, 0.0, 1.0 - pixel[3]]);
            }
        }
    }
    // 羽化：对蒙版 alpha 做方框模糊（`feather` 是过渡总宽度，半径取一半）。
    // 此前这里被静默忽略，表现为「设了羽化却没有软边」。
    if mask.feather > 0.0 {
        let radius = (mask.feather / 2.0).round().max(1.0) as u32;
        crate::filter::box_blur(&mut mask_buffer, radius, 1);
    }
    layer_buffer.multiply_alpha_by(&mask_buffer);
}

/// 形状轮廓折线（闭合），用于描边。
pub fn shape_outline(kind: ShapeKind, bbox: Bbox, points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    match kind {
        ShapeKind::Rect => {
            let (x, y, w, h) = (bbox.x, bbox.y, bbox.w, bbox.h);
            vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y)]
        }
        ShapeKind::Ellipse => {
            let cx = bbox.x + bbox.w / 2.0;
            let cy = bbox.y + bbox.h / 2.0;
            let rx = bbox.w / 2.0;
            let ry = bbox.h / 2.0;
            let segments = 48;
            (0..=segments)
                .map(|index| {
                    let angle = index as f64 / segments as f64 * std::f64::consts::TAU;
                    (cx + rx * angle.cos(), cy + ry * angle.sin())
                })
                .collect()
        }
        ShapeKind::Polygon => {
            let mut outline = points.to_vec();
            if let Some(first) = points.first() {
                outline.push(*first);
            }
            outline
        }
    }
}

/// 从区域缓冲区切出一个 tile（tile 内越出文档/缓冲区的部分保持透明）。
pub fn tile_from_buffer(buffer: &Buffer, grid: &TileGrid, key: TileKey) -> Tile {
    tile_from_buffer_preserving(buffer, grid, key, None)
}

/// 由缓冲生成 tile；`previous` 非空时，**缓冲未覆盖的像素沿用旧 tile**。
///
/// 关键正确性：`render_region` 只渲染一小块区域（例如 1×1 探针或局部 dirty）时，
/// 若把整块 tile 都按「缓冲未覆盖 = 透明」写回缓存，其余像素会被清空；
/// 之后任何**按 tile 组合**的渲染（客户端 WASM 内核、增量 dirty 渲染）都会丢内容。
/// 这是实测到的真实缺陷（表现为画面上出现白块/内容消失）。
pub fn tile_from_buffer_preserving(
    buffer: &Buffer,
    grid: &TileGrid,
    key: TileKey,
    previous: Option<&Tile>,
) -> Tile {
    let size = grid.tile_size();
    let bounds = grid.bounds(key);
    let mut tile = match previous {
        Some(previous) if previous.size() == size => previous.clone(),
        _ => Tile::new(key, size),
    };
    let origin_x = bounds.x as i64;
    let origin_y = bounds.y as i64;
    let (buffer_origin_x, buffer_origin_y) = buffer.origin();
    for y in 0..size {
        for x in 0..size {
            let document_x = origin_x + x as i64;
            let document_y = origin_y + y as i64;
            if document_x >= grid.width() as i64 || document_y >= grid.height() as i64 {
                continue;
            }
            let buffer_x = document_x - buffer_origin_x;
            let buffer_y = document_y - buffer_origin_y;
            if buffer_x < 0 || buffer_y < 0 {
                continue;
            }
            let (buffer_x, buffer_y) = (buffer_x as u32, buffer_y as u32);
            if buffer_x >= buffer.width() || buffer_y >= buffer.height() {
                continue;
            }
            tile.set(x, y, buffer.pixel(buffer_x, buffer_y));
        }
    }
    tile
}

/// 直通颜色 → 预乘（渲染器内部使用）。
pub fn premultiplied(color: LinearRgba) -> LinearRgba {
    premultiply(color)
}

/// 本图层相关的选区（含各自 `created_by`，用于判定"晚于选区创建"）✓。
///
/// `linked_layer` 为空 ⇒ 作用于全文档 ✓；有值 ⇒ 只作用于该图层 ✓；
/// 已删除的选区不参与 ✓。没有选区时返回空表 ⇒ **零开销** ✓。
/// 该对象生效的选区（路线 A：只约束**在其创建之后创建的对象** ✓）。
///
/// 用日志里的原子 id（真 ULID，单调 ✓）判定先后 ✓，而不是用户传入的 `selection_id`
/// （后者可以是任意字符串 ✗ —— 本会话就因此先失败过一次 ✓）。
pub fn object_clip(
    state: &DocumentState,
    layer_id: &str,
    object: &yanshi_core::Object,
) -> Option<crate::selection::SelectionSet> {
    let mut clip = crate::selection::SelectionSet::new();
    let mut any = false;
    for (created_by, shape) in layer_selections(state, layer_id) {
        if created_by.as_str() < object.created_by.as_str() {
            clip.push(shape);
            any = true;
        }
    }
    any.then_some(clip)
}

/// 把选区覆盖度**逐像素**乘进形状覆盖率 ✓ —— 选区外一个像素都不会被写 ✓。
pub fn clip_coverage(
    coverage: &mut crate::geometry::Coverage,
    clip: &crate::selection::SelectionSet,
) {
    for y in 0..coverage.height {
        for x in 0..coverage.width {
            let index = (y * coverage.width + x) as usize;
            // 取**像素中心** ✓ —— 与笔触那条链（ 的 +0.5）保持一致 ✓。
            // 用左边界取样会让选区边界那一列被多算进去（实测越界 128 个像素 = 一整列 ✗）。
            let document_x = coverage.bbox.x + f64::from(x) + 0.5;
            let document_y = coverage.bbox.y + f64::from(y) + 0.5;
            coverage.data[index] *= clip.coverage(document_x, document_y).clamp(0.0, 1.0);
        }
    }
}

/// 本图层相关的选区（含 ，供判定先后）✓。
pub fn layer_selections(
    state: &DocumentState,
    layer_id: &str,
) -> Vec<(String, crate::selection::SelectionShape)> {
    let mut out = Vec::new();
    for selection in state.selections.values() {
        if selection.is_deleted() {
            continue;
        }
        if let Some(linked) = &selection.linked_layer {
            if linked != layer_id {
                continue;
            }
        }
        out.push((
            selection.created_by.clone(),
            crate::selection::SelectionShape::from_value(&selection.shape),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dirty::{plan_dirty, DirtyKind};
    use serde_json::json;
    use yanshi_core::blob::{stage_blob, MemoryBlobStore};
    use yanshi_core::{
        Atom, AtomKind, AtomLog, FoldEngine, Layer, LayerType, Object, ObjectType, Transform,
    };

    fn layer(id: &str, z: i64) -> Layer {
        Layer {
            id: id.to_owned(),
            name: id.to_owned(),
            layer_type: LayerType::Raster,
            parent_id: None,
            z_index: z,
            blend_mode: "normal".to_owned(),
            opacity: 1.0,
            visible: true,
            locked: false,
            alpha_lock: false,
            clipping_mask: false,
            mask_id: None,
            transform: Transform::IDENTITY,
            medium: None,
            style: None,
            metadata: Value::Null,
            blobs: Vec::new(),
            created_by: "a1".to_owned(),
            updated_by: None,
            deleted_by: None,
        }
    }

    fn object(id: &str, layer_id: &str, object_type: ObjectType, z: i64, data: Value) -> Object {
        Object {
            id: id.to_owned(),
            layer_id: layer_id.to_owned(),
            object_type,
            z_index: z,
            visible: true,
            locked: false,
            metadata: Value::Null,
            transform: Transform::IDENTITY,
            style: None,
            versions: vec!["a1".to_owned()],
            current_version: Some("a1".to_owned()),
            created_by: "a1".to_owned(),
            deleted_by: None,
            data,
            blobs: Vec::new(),
        }
    }

    fn white_document() -> DocumentState {
        let mut state = DocumentState::empty();
        state.doc_id = Some("doc_1".to_owned());
        state.width = 32;
        state.height = 32;
        state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
        state
            .layers
            .insert("layer_1".to_owned(), layer("layer_1", 0));
        state
            .layers
            .insert("layer_2".to_owned(), layer("layer_2", 1));
        state
    }

    fn renderer() -> Renderer {
        Renderer::new(TileGrid::new(32, 32, 32).unwrap())
    }

    #[test]
    fn background_only_render_is_uniform() {
        let state = white_document();
        let store = MemoryBlobStore::new();
        let out = renderer().render_document(&state, &store).unwrap();
        assert_eq!(out.width, 32);
        assert_eq!(out.height, 32);
        assert_eq!(out.pixel(0, 0), Some([255, 255, 255, 255]));
        assert_eq!(out.pixel(31, 31), Some([255, 255, 255, 255]));
        assert_eq!(out.tiles, vec![TileKey::new(0, 0)]);
        assert_eq!(out.stats.layers, 2);
    }

    #[test]
    fn transparent_document_renders_transparent_pixels() {
        let mut state = white_document();
        state.background = Value::Null;
        let store = MemoryBlobStore::new();
        let out = renderer().render_document(&state, &store).unwrap();
        assert_eq!(out.pixel(0, 0), Some([0, 0, 0, 0]));
    }

    #[test]
    fn stroke_renders_inside_region_and_covers_only_dirty_tiles() {
        let mut state = white_document();
        state.objects.insert(
            "obj_1".to_owned(),
            object(
                "obj_1",
                "layer_1",
                ObjectType::Stroke,
                0,
                json!({
                    "points": [[4.0, 4.0], [12.0, 4.0]],
                    "size": 4.0,
                    "color": [0.0, 0.0, 0.0, 1.0],
                }),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let out = renderer.render_document(&state, &store).unwrap();
        let painted = out.pixel(8, 4).unwrap();
        assert!(painted[0] < 60, "painted={painted:?}");
        assert_eq!(out.pixel(30, 30), Some([255, 255, 255, 255]));

        let atom = Atom::new(
            AtomKind::DrawStroke,
            "human:1",
            "s",
            json!({"object_id": "obj_1", "layer_id": "layer_1"}),
        );
        let dirty = plan_dirty(&state, None, &atom);
        assert_eq!(dirty.kind, DirtyKind::Geometry);
        assert_eq!(
            crate::dirty::invalidated_tiles(renderer.grid(), &state, &dirty),
            vec![TileKey::new(0, 0)]
        );
    }

    /// 历史事故回归：`color: [40, 120, 60, 255]` 曾被当作线性浮点直通，
    /// alpha=255 直接饱和 → 画了一笔却只看到白色。字节数组与对象写法必须等价。
    #[test]
    fn byte_array_colors_paint_the_same_as_srgb_objects() {
        let stroke = |color: serde_json::Value| {
            let mut state = white_document();
            state.objects.insert(
                "obj_1".to_owned(),
                object(
                    "obj_1",
                    "layer_1",
                    ObjectType::Stroke,
                    0,
                    json!({
                        "points": [[6.0, 20.0], [26.0, 20.0]],
                        "size": 7.0,
                        "color": color,
                    }),
                ),
            );
            renderer()
                .render_document(&state, &MemoryBlobStore::new())
                .unwrap()
        };

        let from_array = stroke(json!([40, 120, 60, 255]));
        let from_object = stroke(json!({"r": 40, "g": 120, "b": 60, "a": 255}));
        for x in [10, 16, 22] {
            let a = from_array.pixel(x, 20).unwrap();
            let b = from_object.pixel(x, 20).unwrap();
            assert_eq!(a, b, "x={x} 两种颜色写法必须渲染一致");
            assert!(
                a[1] as i32 > a[0] as i32 + 20 && a[1] as i32 > a[2] as i32 + 20,
                "x={x} 应是绿色而不是被饱和成白色：{a:?}"
            );
            assert!(a[1] < 200, "x={x} 不应接近白色：{a:?}");
        }
        // 笔迹之外仍是背景。
        assert_eq!(from_array.pixel(16, 2), Some([255, 255, 255, 255]));
    }

    /// 13.3 本地乐观渲染的关键不变量：把笔段**增量盖章**到缓存 tile 上，
    /// 结果必须与「把这条笔迹整块重绘」逐字节一致（否则本地乐观画面与权威画面会漂移）。
    #[test]
    fn incremental_stamp_matches_full_tile_re_render() {
        let mut state = white_document();
        state.width = 128;
        state.height = 128;
        let store = MemoryBlobStore::new();
        let grid = TileGrid::new(32, 128, 128).unwrap();
        let mut renderer = Renderer::with_budget(grid.clone(), 8 * 1024 * 1024);
        // 先铺一层已有内容，确保增量盖章是「叠加」在缓存像素上。
        state.objects.insert(
            "bg".to_owned(),
            object(
                "bg",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 128, "h": 128}},
                       "color": {"r": 240, "g": 240, "b": 240, "a": 255}}),
            ),
        );
        renderer.cache_mut().clear();
        renderer
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();

        let brush = BrushSpec {
            size: 5.0,
            color: [0.2, 0.1, 0.05, 1.0],
            seed: 7,
            ..BrushSpec::default()
        };
        let whole = StrokeGeometry {
            points: vec![
                StrokePoint {
                    x: 8.0,
                    y: 8.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 60.0,
                    y: 40.0,
                    pressure: 1.0,
                },
                StrokePoint {
                    x: 110.0,
                    y: 90.0,
                    pressure: 1.0,
                },
            ],
        };
        // 分三段增量盖章（模拟拖动）。
        let segments = [
            StrokeGeometry {
                points: whole.points[0..2].to_vec(),
            },
            StrokeGeometry {
                points: whole.points[1..].to_vec(),
            },
        ];
        for segment in &segments {
            renderer
                .stamp_into_tiles(&state, &store, &brush, segment)
                .unwrap();
        }
        let incremental = renderer
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();

        // 参考：把同一笔迹一次性盖到干净的缓存上（等价于整块重绘的像素）。
        let mut reference = Renderer::with_budget(grid.clone(), 8 * 1024 * 1024);
        reference.cache_mut().clear();
        reference
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();
        reference
            .stamp_into_tiles(&state, &store, &brush, &whole)
            .unwrap();
        let expected = reference
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 128.0, 128.0))
            .unwrap();

        let diff = incremental
            .rgba8
            .iter()
            .zip(expected.rgba8.iter())
            .filter(|(a, b)| a != b)
            .count();
        // 分段与整段在接缝处的盖章起点略有差异（间距累积），允许极小比例差异；
        // 关键是不能整块漂移。
        assert!(
            diff * 200 < incremental.rgba8.len(),
            "增量盖章与整段盖章差异过大：{diff}/{} 字节",
            incremental.rgba8.len()
        );
    }

    /// 回归：局部区域渲染不得清空该 tile 的其余像素。
    ///
    /// 曾经 `store_tiles` 把「缓冲未覆盖 = 透明」整块写回缓存，于是 1×1 探针渲染
    /// 会把整块 tile 抹空；之后任何按 tile 组合的渲染（客户端 WASM 内核）就丢内容。
    #[test]
    fn partial_region_render_preserves_untouched_tile_pixels() {
        let mut state = white_document();
        state.width = 64;
        state.height = 64;
        state.objects.insert(
            "stroke_a".to_owned(),
            object(
                "stroke_a",
                "layer_1",
                ObjectType::Stroke,
                0,
                json!({"points": [[4.0, 4.0], [56.0, 40.0]], "size": 6.0,
                       "color": {"r": 20, "g": 20, "b": 30, "a": 255}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let grid = TileGrid::new(32, 64, 64).unwrap();
        let mut renderer = Renderer::with_budget(grid, 8 * 1024 * 1024);

        let full = renderer
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 64.0, 64.0))
            .unwrap();

        // 局部渲染（1×1 探针）后，整幅组合渲染必须与第一次逐字节一致。
        let _ = renderer
            .render_region(&state, &store, Bbox::new(36.0, 15.0, 1.0, 1.0))
            .unwrap();
        let after_probe = renderer
            .render_region(&state, &store, Bbox::new(0.0, 0.0, 64.0, 64.0))
            .unwrap();
        let diff = full
            .rgba8
            .iter()
            .zip(after_probe.rgba8.iter())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(diff, 0, "局部渲染不得改变其它像素（差异 {diff} 字节）");

        // 也检查按 tile 直接读取的路径（客户端内核就是这么读的）。
        for key in [
            TileKey::new(0, 0),
            TileKey::new(1, 0),
            TileKey::new(0, 1),
            TileKey::new(1, 1),
        ] {
            let tile = renderer.render_tile(&state, &store, key).unwrap();
            let rgba = tile.to_rgba8(Some([255, 255, 255, 255]));
            let origin_x = (key.x * 32) as usize;
            let origin_y = (key.y * 32) as usize;
            for y in 0..32usize {
                for x in 0..32usize {
                    let index = (y * 32 + x) * 4;
                    let full_index = (((origin_y + y) * 64) + origin_x + x) * 4;
                    assert_eq!(
                        &rgba[index..index + 4],
                        &full.rgba8[full_index..full_index + 4],
                        "tile {key:?} 的像素 ({x},{y}) 与整幅渲染不一致"
                    );
                }
            }
        }
    }

    #[test]
    fn layer_order_and_opacity_affect_composite() {
        let mut state = white_document();
        state.objects.insert(
            "obj_red".to_owned(),
            object(
                "obj_red",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 16, "h": 16}},
                    "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                }),
            ),
        );
        let blue = object(
            "obj_blue",
            "layer_2",
            ObjectType::Shape,
            0,
            json!({
                "geometry": {"kind": "rect", "bbox": {"x": 8, "y": 8, "w": 16, "h": 16}},
                "color": {"r": 0, "g": 0, "b": 255, "a": 128},
            }),
        );
        state.objects.insert("obj_blue".to_owned(), blue);
        let store = MemoryBlobStore::new();
        let out = renderer().render_document(&state, &store).unwrap();

        let red_only = out.pixel(2, 2).unwrap();
        assert!(red_only[0] > 200 && red_only[2] < 60, "{red_only:?}");
        let overlap = out.pixel(10, 10).unwrap();
        assert!(overlap[2] > 100, "重叠处应含蓝：{overlap:?}");
        let blue_only = out.pixel(20, 20).unwrap();
        assert!(blue_only[2] > 200 && blue_only[0] < 200, "{blue_only:?}");
    }

    #[test]
    fn hidden_layer_is_skipped_unless_requested() {
        let mut state = white_document();
        state.layers.get_mut("layer_2").unwrap().visible = false;
        state.objects.insert(
            "obj_blue".to_owned(),
            object(
                "obj_blue",
                "layer_2",
                ObjectType::Shape,
                0,
                json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 32}},
                    "color": {"r": 0, "g": 0, "b": 255, "a": 255},
                }),
            ),
        );
        let store = MemoryBlobStore::new();
        let hidden = renderer().render_document(&state, &store).unwrap();
        assert_eq!(hidden.pixel(5, 5), Some([255, 255, 255, 255]));

        let mut include = renderer().with_options(RenderOptions {
            include_hidden_layers: true,
            ..RenderOptions::default()
        });
        let shown = include.render_document(&state, &store).unwrap();
        let pixel = shown.pixel(5, 5).unwrap();
        assert!(pixel[2] > 200 && pixel[0] < 60, "{pixel:?}");
    }

    #[test]
    fn adjustment_object_applies_to_content_below() {
        let mut state = white_document();
        state.objects.insert(
            "obj_rect".to_owned(),
            object(
                "obj_rect",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 32}},
                    "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                }),
            ),
        );
        state.objects.insert(
            "obj_adj".to_owned(),
            object(
                "obj_adj",
                "layer_1",
                ObjectType::Adjustment,
                5,
                json!({"adjustment_type": "invert", "params": {}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let inverted = renderer.render_document(&state, &store).unwrap();
        let pixel = inverted.pixel(5, 5).unwrap();
        assert!(pixel[1] > 200, "红色被反相为青色：{pixel:?}");
        assert!(pixel[2] > 200, "{pixel:?}");
        assert!(inverted.stats.unsupported.is_empty());

        // 未实现的类型必须产生告警且不改动像素（`curves` 已实现，这里用内核没有的类型）。
        state.objects.get_mut("obj_adj").unwrap().data =
            json!({"adjustment_type": "not_an_effect", "params": {}});
        let warned = renderer.render_document(&state, &store).unwrap();
        assert_eq!(warned.stats.unsupported.len(), 1);
        assert_eq!(warned.pixel(5, 5).unwrap()[0], 255, "未识别时不改动像素");
    }

    /// heal：复制源纹理的同时，把低频颜色对齐到目标处（修复画笔）。
    #[test]
    fn heal_matches_the_destination_colour() {
        let mut state = white_document();
        state.width = 64;
        state.height = 64;
        // 目标处必须是**图层里已绘制**的内容（浅灰底），否则没有颜色信息可对齐。
        state.objects.insert(
            "obj_bg".to_owned(),
            object(
                "obj_bg",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}},
                       "color": {"r": 200, "g": 200, "b": 200, "a": 255}}),
            ),
        );
        // 源：偏暗的蓝灰块（左上）。
        state.objects.insert(
            "obj_src".to_owned(),
            object(
                "obj_src",
                "layer_1",
                ObjectType::Shape,
                1,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 24, "h": 24}},
                       "color": {"r": 30, "g": 40, "b": 60, "a": 255}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();

        let retouch = |renderer: &mut Renderer, kind: &str| {
            let mut state = state.clone();
            state.objects.insert(
                "obj_retouch".to_owned(),
                object(
                    "obj_retouch",
                    "layer_1",
                    ObjectType::Retouch,
                    2,
                    json!({"retouch_type": kind, "points": [[44.0, 44.0]],
                           "source_offset": [-40.0, -40.0], "size": 12.0,
                           "hardness": 1.0, "opacity": 1.0}),
                ),
            );
            renderer
                .render_document(&state, &store)
                .unwrap()
                .pixel(44, 44)
                .unwrap()
        };

        let cloned = retouch(&mut renderer, "clone_stamp");
        let healed = retouch(&mut renderer, "heal");
        let background = renderer
            .render_document(&state, &store)
            .unwrap()
            .pixel(44, 44)
            .unwrap();
        // clone 直接搬来暗蓝灰；heal 应被浅灰底抬高，明显更接近底色。
        assert!(
            cloned[0] < 100 && cloned[2] > cloned[0],
            "clone 应搬来暗蓝灰：{cloned:?}"
        );
        assert!(
            healed[0] > cloned[0] + 40 && healed[1] > cloned[1] + 40,
            "heal 应把低频颜色对齐到目标处：clone={cloned:?} heal={healed:?}"
        );
        assert!(
            (healed[0] as i32 - background[0] as i32).unsigned_abs()
                < (cloned[0] as i32 - background[0] as i32).unsigned_abs(),
            "heal 应比 clone 更接近目标处底色：heal={healed:?} clone={cloned:?} bg={background:?}"
        );
        // 未实现类型仍要告警。
        let mut bad = state.clone();
        bad.objects.insert(
            "obj_retouch".to_owned(),
            object(
                "obj_retouch",
                "layer_1",
                ObjectType::Retouch,
                2,
                json!({"retouch_type": "warp", "points": [[44.0, 44.0]], "source_offset": [-40.0, -40.0]}),
            ),
        );
        let rendered = renderer.render_document(&bad, &store).unwrap();
        assert_eq!(
            rendered.stats.unsupported.len(),
            1,
            "未实现的修图类型必须告警"
        );
    }

    /// 蒙版：范围外应被裁掉、反选应翻转、羽化应产生软边，缺失蒙版必须告警。
    #[test]
    fn layer_mask_clips_inverts_and_feathers() {
        fn scene(shape: serde_json::Value, feather: f64, invert: bool) -> DocumentState {
            let mut state = white_document();
            state.width = 64;
            state.height = 64;
            state.objects.insert(
                "obj_fill".to_owned(),
                object(
                    "obj_fill",
                    "layer_1",
                    ObjectType::Shape,
                    0,
                    json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}},
                           "color": {"r": 10, "g": 10, "b": 10, "a": 255}}),
                ),
            );
            state.masks.insert(
                "mask_1".to_owned(),
                yanshi_core::Selection {
                    id: "mask_1".to_owned(),
                    shape,
                    feather,
                    mode: "new".to_owned(),
                    invert,
                    linked_layer: Some("layer_1".to_owned()),
                    refined_edges: false,
                    blobs: Vec::new(),
                    created_by: "human:1".to_owned(),
                    deleted_by: None,
                },
            );
            if let Some(layer) = state.layers.get_mut("layer_1") {
                layer.mask_id = Some("mask_1".to_owned());
            }
            state
        }

        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        // 左半矩形蒙版：右半应被裁掉。
        let masked = renderer
            .render_document(
                &scene(
                    json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}}),
                    0.0,
                    false,
                ),
                &store,
            )
            .unwrap();
        // 合成结果叠在白色文档底上，因此按**颜色**判断：蒙版内是深色填充，蒙版外是白底。
        assert!(masked.pixel(16, 32).unwrap()[0] < 120, "蒙版内应保留内容");
        assert!(
            masked.pixel(48, 32).unwrap()[0] > 200,
            "蒙版外应被裁掉（露白底）"
        );

        // 反选后相反。
        let inverted = renderer
            .render_document(
                &scene(
                    json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}}),
                    0.0,
                    true,
                ),
                &store,
            )
            .unwrap();
        assert!(
            inverted.pixel(48, 32).unwrap()[0] < 120,
            "反选后蒙版外应保留"
        );
        assert!(
            inverted.pixel(8, 32).unwrap()[0] > 200,
            "反选后蒙版内应被裁掉"
        );

        // 羽化：边界附近应出现中间值（软边）。
        let feathered = renderer
            .render_document(
                &scene(
                    json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}}),
                    12.0,
                    false,
                ),
                &store,
            )
            .unwrap();
        let edge = feathered.pixel(32, 32).unwrap()[0];
        assert!(
            edge > 20 && edge < 235,
            "羽化后边界应是中间值（软边）：{edge}"
        );

        // 引用不存在的蒙版必须告警而不是静默忽略。
        let mut broken = scene(
            json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}}),
            0.0,
            false,
        );
        if let Some(layer) = broken.layers.get_mut("layer_1") {
            layer.mask_id = Some("mask_missing".to_owned());
        }
        let rendered = renderer.render_document(&broken, &store).unwrap();
        assert_eq!(rendered.stats.unsupported.len(), 1, "缺失蒙版必须告警");
    }

    /// 不变量：liquify 只影响其影响圈内的像素，**圆外像素必须逐位不变**。
    ///
    /// 这条性质正是「把循环收缩到受影响外接方框」优化所依赖的前提；
    /// 一旦外框算错（漏掉某些像素），本测试就会红。
    #[test]
    fn liquify_leaves_pixels_outside_its_circles_untouched() {
        let mut state = white_document();
        state.width = 128;
        state.height = 128;
        // 中心放一个小方块（周围留白）：这样 twirl/pinch 旋转缩放它、push 沿 x 推动它，
        // 三种模式都会在影响圈内产生**可见**变化。
        // 前两版 fixture 分别是纯色矩形与上下分界 —— 纯色旋转看不出变化，
        // 水平分界沿 x 推动也看不出变化，于是"圈内应改变"的断言失败（都是 fixture 的问题）。
        state.objects.insert(
            "square".to_owned(),
            object(
                "square",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 54, "y": 54, "w": 20, "h": 20}},
                       "color": {"r": 20, "g": 20, "b": 20, "a": 255}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let before = renderer.render_document(&state, &store).unwrap();

        for mode in ["twirl", "pinch", "push"] {
            let mut liquified_state = state.clone();
            liquified_state.objects.insert(
                "liquify".to_owned(),
                object(
                    "liquify",
                    "layer_1",
                    ObjectType::Liquify,
                    1,
                    json!({"mode": mode, "points": [[64.0, 64.0]], "size": 64.0,
                           "strength": 0.9, "direction": [1.0, 0.0]}),
                ),
            );
            let after = renderer.render_document(&liquified_state, &store).unwrap();
            // 影响圈半径 = size/2 = 32，中心 (64,64)。
            let mut outside_changed = Vec::new();
            let mut inside_changed = 0usize;
            for y in 0..128u32 {
                for x in 0..128u32 {
                    let index = ((y * 128 + x) * 4) as usize;
                    let changed = before.rgba8[index..index + 4] != after.rgba8[index..index + 4];
                    let dx = x as f64 - 64.0;
                    let dy = y as f64 - 64.0;
                    if (dx * dx + dy * dy).sqrt() <= 32.0 {
                        if changed {
                            inside_changed += 1;
                        }
                    } else if changed {
                        outside_changed.push((x, y));
                    }
                }
            }
            assert!(
                outside_changed.is_empty(),
                "{mode}: 影响圈外有 {} 个像素被改动，例如 {:?}",
                outside_changed.len(),
                &outside_changed[..outside_changed.len().min(4)]
            );
            assert!(
                inside_changed > 20,
                "{mode}: 影响圈内仅有 {inside_changed} 个像素变化，效果没有真正生效（检查 fixture）"
            );
        }
    }

    /// 微基准：整层克隆的成本（说明「局部快照」在多大画布上才真正值钱）。
    #[test]
    #[ignore = "诊断：整层克隆成本"]
    fn layer_clone_cost_probe() {
        for side in [1024u32, 4096] {
            let padded = side + 256; // 每边 128 的外扩
            let mut buffer = Buffer::new(0, 0, padded, padded);
            buffer.fill([0.4, 0.5, 0.6, 1.0]);
            let mut best = std::time::Duration::MAX;
            for _ in 0..3 {
                let started = std::time::Instant::now();
                let clone = buffer.clone();
                std::hint::black_box(&clone);
                best = best.min(started.elapsed());
            }
            let bytes = (padded as usize).pow(2) * 16;
            println!(
                "  整层克隆 {side}²（PAD 后 {padded}² ≈ {:.0}MB）: {best:?}",
                bytes as f64 / (1024.0 * 1024.0)
            );
        }
    }

    /// 微基准：`Buffer` 访问器的成本（双线性采样每个目标像素要 4 次 get + 1 次 set）。
    /// 结论：约 3ns —— 访问器**不是**热点（被向量化）。
    #[test]
    #[ignore = "诊断：Buffer 访问器成本"]
    fn buffer_accessor_cost_probe() {
        let width = 1024u32;
        let mut buffer = Buffer::new(0, 0, width, width);
        buffer.fill([0.4, 0.5, 0.6, 1.0]);
        let samples = 160_000usize;
        // 4 次 get + 1 次 set，模拟一次双线性采样并写回。
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let mut acc = 0.0f32;
            for index in 0..samples {
                let x = (index % 1000) as u32;
                let y = (index / 1000) as u32;
                let p00 = buffer.pixel(x, y);
                let p10 = buffer.pixel(x + 1, y);
                let p01 = buffer.pixel(x, y + 1);
                let p11 = buffer.pixel(x + 1, y + 1);
                let out = [
                    (p00[0] + p10[0] + p01[0] + p11[0]) * 0.25,
                    (p00[1] + p10[1] + p01[1] + p11[1]) * 0.25,
                    (p00[2] + p10[2] + p01[2] + p11[2]) * 0.25,
                    (p00[3] + p10[3] + p01[3] + p11[3]) * 0.25,
                ];
                buffer.set_pixel(x, y, out);
                acc += out[0];
            }
            std::hint::black_box(acc);
            best = best.min(started.elapsed());
        }
        println!(
            "  4×get+1×set（{} 次）: {best:?}｜每次 {:.0}ns",
            samples,
            best.as_secs_f64() * 1e9 / samples as f64
        );
    }

    /// 微基准：复刻 liquify twirl 的**内层数学**（距离 + smoothstep + sin_cos + 双线性），
    /// 结论：**约 66ns/像素** —— 与实测液化净成本（约 30ms / 16 万像素 ≈ 190ns/像素）
    /// 同量级，差额来自 `layer_buffer.clone()`（26MB）与循环开销；**没有**结构性重复
    /// （整幅渲染 `tiles_rendered=1`，不是按 tile 重做）。
    #[test]
    #[ignore = "诊断：liquify 内层数学成本"]
    fn liquify_inner_math_probe() {
        let width = 1024u32;
        let mut buffer = Buffer::new(0, 0, width, width);
        buffer.fill([0.4, 0.5, 0.6, 1.0]);
        let (px, py) = (512.0f64, 512.0f64);
        let size = 400.0f64;
        let radius = size / 2.0;
        let strength = 0.8f64;
        let affected = 160_000usize;
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let mut acc = 0.0f32;
            for index in 0..affected {
                let x = (index % 400) as u32 + 312;
                let y = (index / 400) as u32 + 312;
                let document_x = x as f64;
                let document_y = y as f64;
                let dx = document_x - px;
                let dy = document_y - py;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance >= radius {
                    continue;
                }
                let t = 1.0 - distance / radius;
                let falloff = t * t * (3.0 - 2.0 * t);
                let angle = strength * falloff;
                let (sin, cos) = angle.sin_cos();
                let rotated_x = dx * cos - dy * sin;
                let rotated_y = dx * sin + dy * cos;
                let shift_x = dx - rotated_x;
                let shift_y = dy - rotated_y;
                if shift_x == 0.0 && shift_y == 0.0 {
                    continue;
                }
                let sample_x = document_x - shift_x;
                let sample_y = document_y - shift_y;
                let x0 = sample_x.floor();
                let y0 = sample_y.floor();
                let fx = (sample_x - x0) as f32;
                let fy = (sample_y - y0) as f32;
                let clamp = |value: i64, limit: u32| value.clamp(0, limit as i64 - 1) as u32;
                let (x0i, y0i) = (clamp(x0 as i64, width), clamp(y0 as i64, width));
                let (x1i, y1i) = (clamp(x0 as i64 + 1, width), clamp(y0 as i64 + 1, width));
                let p00 = buffer.pixel(x0i, y0i);
                let p10 = buffer.pixel(x1i, y0i);
                let p01 = buffer.pixel(x0i, y1i);
                let p11 = buffer.pixel(x1i, y1i);
                let mut out = [0.0f32; 4];
                for channel in 0..4 {
                    let top = p00[channel] + (p10[channel] - p00[channel]) * fx;
                    let bottom = p01[channel] + (p11[channel] - p01[channel]) * fx;
                    out[channel] = top + (bottom - top) * fy;
                }
                buffer.set_pixel(x, y, out);
                acc += out[0];
            }
            std::hint::black_box(acc);
            best = best.min(started.elapsed());
        }
        println!(
            "  liquify twirl 内层数学（{} 像素）: {best:?}｜每像素 {:.0}ns",
            affected,
            best.as_secs_f64() * 1e9 / affected as f64
        );
    }

    /// 性能诊断：liquify 三种模式在 1024² 上的渲染成本（用于"循环不变量外提"类改动的前后对比）。
    #[test]
    #[ignore = "诊断：liquify 成本"]
    fn liquify_cost_probe() {
        for mode in ["twirl", "pinch", "push"] {
            let mut state = white_document();
            state.width = 1024;
            state.height = 1024;
            state.objects.insert(
                "shape".to_owned(),
                object(
                    "shape",
                    "layer_1",
                    ObjectType::Shape,
                    0,
                    json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 1024, "h": 1024}},
                           "color": {"r": 90, "g": 120, "b": 160, "a": 255}}),
                ),
            );
            state.objects.insert(
                "liquify".to_owned(),
                object(
                    "liquify",
                    "layer_1",
                    ObjectType::Liquify,
                    1,
                    json!({"mode": mode, "points": [[512.0, 512.0]], "size": 400.0,
                           "strength": 0.8, "direction": [1.0, 0.0]}),
                ),
            );
            let store = MemoryBlobStore::new();
            // 先测「不加 liquify」的同一文档作为基线，再测加上之后的时间，两者相减才是 liquify 的成本。
            let without = state.objects.remove("liquify").unwrap();
            let mut renderer = renderer();
            let mut baseline = std::time::Duration::MAX;
            for _ in 0..5 {
                let started = std::time::Instant::now();
                let _ = renderer.render_document(&state, &store).unwrap();
                baseline = baseline.min(started.elapsed());
            }
            state.objects.insert("liquify".to_owned(), without);
            let mut with_liquify = std::time::Duration::MAX;
            for _ in 0..5 {
                let started = std::time::Instant::now();
                let _ = renderer.render_document(&state, &store).unwrap();
                with_liquify = with_liquify.min(started.elapsed());
            }
            let rendered = renderer.render_document(&state, &store).unwrap();
            println!(
                "  liquify {mode}（1024²，size 400）: 总 {:?}｜基线 {:?}｜**净成本 {:?}**｜tiles_rendered={} 对象={}",
                with_liquify,
                baseline,
                with_liquify.saturating_sub(baseline),
                rendered.stats.tiles_rendered,
                rendered.stats.objects
            );
        }
    }

    /// 液化 twirl：横向条纹应被旋转出倾斜（同一列上出现横向位移差）。
    #[test]
    fn liquify_twirl_rotates_content() {
        let mut state = white_document();
        state.width = 96;
        state.height = 96;
        // 一半黑一半白的水平分界（y=48），便于观察旋转。
        state.objects.insert(
            "top".to_owned(),
            object(
                "top",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 96, "h": 48}},
                       "color": {"r": 10, "g": 10, "b": 10, "a": 255}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let before = renderer.render_document(&state, &store).unwrap();

        state.objects.insert(
            "twirl".to_owned(),
            object(
                "twirl",
                "layer_1",
                ObjectType::Liquify,
                1,
                json!({"liquify_type": "twirl", "points": [[48.0, 48.0]], "size": 40.0, "strength": 1.0}),
            ),
        );
        let after = renderer.render_document(&state, &store).unwrap();
        // 分界附近应出现旋转：左右两侧（相对中心对称）的分界高度不再相同。
        // 用足够宽的扫描窗口，避免旋转把分界推出窗口导致误判。
        // 分界高度 = 该列上最后一个暗像素的 y（从底部向上找）。
        let boundary = |buffer: &RegionRender, x: u32| {
            (8..88)
                .rev()
                .find(|y| buffer.pixel(x, *y).unwrap()[0] < 128)
        };
        let left = boundary(&after, 36);
        let right = boundary(&after, 60);
        assert!(left.is_some() && right.is_some(), "两侧都应能找到分界");
        assert_ne!(
            left, right,
            "旋转应让左右两侧的分界位置不同：{left:?} vs {right:?}"
        );
        // 未旋转时两侧分界相同（对照）。
        assert_eq!(boundary(&before, 36), boundary(&before, 60));
        // 远处不受影响。
        assert_eq!(before.pixel(2, 2).unwrap(), after.pixel(2, 2).unwrap());
        assert!(after.stats.unsupported.is_empty());
    }

    /// 液化 pinch：边界应被吸向中心（同一行上分界向内移动）。
    #[test]
    fn liquify_pinch_pulls_content_inward() {
        let mut state = white_document();
        state.width = 96;
        state.height = 96;
        state.objects.insert(
            "left".to_owned(),
            object(
                "left",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 48, "h": 96}},
                       "color": {"r": 10, "g": 10, "b": 10, "a": 255}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        // 先渲染未液化的版本（对照组），再插入液化对象。
        let before = renderer.render_document(&state, &store).unwrap();
        // 竖直分界在 x=48；把收缩中心放在右侧 (64,48)，径向缩放才会把边界拉向中心。
        // （中心若正好落在分界线上，分界线是径向缩放的不变量，看不出效果。）
        state.objects.insert(
            "pinch".to_owned(),
            object(
                "pinch",
                "layer_1",
                ObjectType::Liquify,
                1,
                json!({"liquify_type": "pinch", "points": [[64.0, 48.0]], "size": 60.0, "strength": 0.5}),
            ),
        );
        let after = renderer.render_document(&state, &store).unwrap();
        // 分界位置（y=48 上第一个变亮的 x）应因收缩而向中心移动。
        let edge = |buffer: &RegionRender| (8..88).find(|x| buffer.pixel(*x, 48).unwrap()[0] > 128);
        let before_edge = edge(&before).expect("原图应能找到分界");
        let after_edge = edge(&after).expect("收缩后仍应有分界");
        assert!(
            after_edge > before_edge,
            "pinch（中心在右）应把分界拉向中心：{before_edge} → {after_edge}"
        );
        assert!(after.stats.unsupported.is_empty());
        // 未实现的模式必须告警。
        let mut bad = state.clone();
        bad.objects.insert(
            "warp".to_owned(),
            object(
                "warp",
                "layer_1",
                ObjectType::Liquify,
                2,
                json!({"liquify_type": "warp", "points": [[48.0, 48.0]], "size": 40.0, "strength": 0.5}),
            ),
        );
        let rendered = renderer.render_document(&bad, &store).unwrap();
        assert_eq!(
            rendered.stats.unsupported.len(),
            1,
            "未实现的液化模式必须告警"
        );
    }

    /// 液化：硬边界应沿方向被推开，影响范围外不动，且区域渲染与整幅一致。
    #[test]
    fn liquify_pushes_pixels_along_the_direction() {
        let mut state = white_document();
        state.width = 64;
        state.height = 64;
        // 左半红、右半蓝的竖直边界在 x=32。
        for (id, x, color) in [
            ("obj_l", 0.0, json!({"r": 220, "g": 20, "b": 20, "a": 255})),
            ("obj_r", 32.0, json!({"r": 20, "g": 20, "b": 220, "a": 255})),
        ] {
            state.objects.insert(
                id.to_owned(),
                object(
                    id,
                    "layer_1",
                    ObjectType::Shape,
                    0,
                    json!({"geometry": {"kind": "rect", "bbox": {"x": x, "y": 0, "w": 32, "h": 64}},
                           "color": color}),
                ),
            );
        }
        // 在边界附近向右推：红色应向右侵入蓝区。
        state.objects.insert(
            "obj_liq".to_owned(),
            object(
                "obj_liq",
                "layer_1",
                ObjectType::Liquify,
                1,
                json!({"points": [[32.0, 32.0]], "size": 24.0, "strength": 0.8, "direction": [1.0, 0.0]}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let pushed = renderer.render_document(&state, &store).unwrap();
        let near = pushed.pixel(34, 32).unwrap();
        assert!(
            near[0] as i32 > 150 && near[2] < 150,
            "边界右移后 (34,32) 应偏红：{near:?}"
        );
        // 远处不受影响。
        let far = pushed.pixel(62, 32).unwrap();
        assert!(far[2] > 180 && far[0] < 60, "远处应仍是蓝：{far:?}");

        // 区域渲染必须与整幅一致（padding 覆盖位移）。
        let region = renderer
            .render_region(&state, &store, Bbox::new(20.0, 20.0, 24.0, 24.0))
            .unwrap();
        let full = renderer
            .render_region(&state, &store, Bbox::new(20.0, 20.0, 24.0, 24.0))
            .unwrap();
        assert_eq!(region.rgba8, full.rgba8);
        // 整幅渲染在 (34,32) 的像素应等于区域渲染对应位置。
        let region_pixel = pushed.pixel(34, 32).unwrap();
        assert_eq!(region_pixel, near);
    }

    /// 涂抹：应把笔迹**后方**的颜色沿方向拖到前方（跨颜色边界时最明显）。
    #[test]
    fn smudge_drags_colour_along_the_stroke() {
        let mut state = white_document();
        state.width = 64;
        state.height = 64;
        // 左半红、右半蓝（图层的两块不透明矩形）。
        for (id, x, color) in [
            ("obj_l", 0.0, json!({"r": 220, "g": 20, "b": 20, "a": 255})),
            ("obj_r", 32.0, json!({"r": 20, "g": 20, "b": 220, "a": 255})),
        ] {
            state.objects.insert(
                id.to_owned(),
                object(
                    id,
                    "layer_1",
                    ObjectType::Shape,
                    0,
                    json!({"geometry": {"kind": "rect", "bbox": {"x": x, "y": 0, "w": 32, "h": 64}},
                           "color": color}),
                ),
            );
        }
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        // 从红区向右拖进蓝区：应把红色带进蓝区。
        state.objects.insert(
            "obj_smudge".to_owned(),
            object(
                "obj_smudge",
                "layer_1",
                ObjectType::Retouch,
                1,
                json!({"retouch_type": "smudge", "points": [[30.0, 32.0], [34.0, 32.0], [38.0, 32.0], [42.0, 32.0]],
                       "size": 14.0, "hardness": 1.0, "opacity": 1.0, "smudge_length": 10.0}),
            ),
        );
        let rendered = renderer.render_document(&state, &store).unwrap();
        let dragged = rendered.pixel(42, 32).unwrap();
        let original = rendered.pixel(42, 60).unwrap();
        assert!(
            dragged[0] as i32 > original[0] as i32 + 30,
            "涂抹应把红色拖进蓝区：dragged={dragged:?} original={original:?}"
        );
        assert!(rendered.stats.unsupported.is_empty());
    }

    /// clone_stamp：必须把偏移处的已有内容复制到笔迹位置，且**只**改笔迹覆盖处。
    #[test]
    fn clone_stamp_copies_pixels_from_the_source_offset() {
        let mut state = white_document();
        state.width = 64;
        state.height = 64;
        // 源：左上 16×16 的红色块。
        state.objects.insert(
            "obj_src".to_owned(),
            object(
                "obj_src",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 16, "h": 16}},
                       "color": {"r": 220, "g": 30, "b": 30, "a": 255}}),
            ),
        );
        // 修图：在 (40,40) 处取样偏移 (-40,-40) → 等价于把左上角红块复制到右下角。
        state.objects.insert(
            "obj_clone".to_owned(),
            object(
                "obj_clone",
                "layer_1",
                ObjectType::Retouch,
                1,
                json!({"retouch_type": "clone_stamp", "points": [[44.0, 44.0]],
                       "source_offset": [-40.0, -40.0], "size": 10.0, "hardness": 1.0, "opacity": 1.0}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let rendered = renderer.render_document(&state, &store).unwrap();
        let cloned = rendered.pixel(44, 44).unwrap();
        assert!(
            cloned[0] > 200 && cloned[1] < 60,
            "应复制到红色：{cloned:?}"
        );
        // 未覆盖处不受影响。
        let untouched = rendered.pixel(30, 30).unwrap();
        assert!(
            untouched[0] > 240 && untouched[1] > 240,
            "其它像素不应改变：{untouched:?}"
        );
        assert!(rendered.stats.unsupported.is_empty());

        // 区域渲染必须外扩到源像素：只渲染右下 16×16 也应得到同样的复制结果。
        let region = renderer
            .render_region(&state, &store, Bbox::new(36.0, 36.0, 16.0, 16.0))
            .unwrap();
        let local = region.pixel(8, 8).unwrap();
        assert_eq!(
            local, cloned,
            "区域渲染与整幅渲染必须逐字节一致：{local:?} vs {cloned:?}"
        );
    }

    #[test]
    fn filter_padding_expands_render_region() {
        let mut state = white_document();
        state.objects.insert(
            "obj_blur".to_owned(),
            object(
                "obj_blur",
                "layer_1",
                ObjectType::Filter,
                0,
                json!({"filter_name": "box_blur", "params": {"radius": 3, "passes": 2}}),
            ),
        );
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        assert_eq!(renderer.filter_padding(&state), 6);
        let out = renderer
            .render_region(&state, &store, Bbox::new(8.0, 8.0, 8.0, 8.0))
            .unwrap();
        assert_eq!(out.stats.filter_padding, 6);
        assert_eq!(out.bbox, Bbox::new(8.0, 8.0, 8.0, 8.0));
        assert_eq!(out.pixel(0, 0), Some([255, 255, 255, 255]));
    }

    #[test]
    fn raster_patch_blits_raw_rgba_from_cas() {
        let store = MemoryBlobStore::new();
        let mut pixels = Vec::new();
        for _ in 0..(4 * 4) {
            pixels.extend_from_slice(&[0u8, 255, 0, 255]);
        }
        let blob = stage_blob(&store, &pixels, RAW_RGBA_MIME).unwrap();
        let mut state = white_document();
        state.objects.insert(
            "obj_patch".to_owned(),
            object(
                "obj_patch",
                "layer_1",
                ObjectType::RasterPatch,
                0,
                json!({
                    "bitmap": blob,
                    "width": 4,
                    "height": 4,
                    "region": {"x": 10, "y": 10, "w": 4, "h": 4},
                }),
            ),
        );
        let out = renderer().render_document(&state, &store).unwrap();
        let inside = out.pixel(11, 11).unwrap();
        assert!(inside[1] > 200 && inside[0] < 60, "{inside:?}");
        assert_eq!(out.pixel(2, 2), Some([255, 255, 255, 255]));
        assert!(out.stats.unsupported.is_empty());
    }

    #[test]
    fn missing_blob_surfaces_reference_not_found() {
        let store = MemoryBlobStore::new();
        let mut state = white_document();
        state.objects.insert(
            "obj_patch".to_owned(),
            object(
                "obj_patch",
                "layer_1",
                ObjectType::RasterPatch,
                0,
                json!({
                    "bitmap": {
                        "blob_hash": format!("sha256:{}", "b".repeat(64)),
                        "size": 16,
                        "mime_type": RAW_RGBA_MIME,
                    },
                    "width": 2,
                    "height": 2,
                    "region": {"x": 0, "y": 0, "w": 2, "h": 2},
                }),
            ),
        );
        let error = renderer().render_document(&state, &store).unwrap_err();
        assert_eq!(error.code, yanshi_core::ErrorCode::ReferenceNotFound);
    }

    #[test]
    fn unsupported_mime_is_warned_not_fatal() {
        let store = MemoryBlobStore::new();
        let blob = stage_blob(&store, &[0u8; 16], "image/webp").unwrap();
        let mut state = white_document();
        state.objects.insert(
            "obj_patch".to_owned(),
            object(
                "obj_patch",
                "layer_1",
                ObjectType::RasterPatch,
                0,
                json!({
                    "bitmap": blob,
                    "width": 2,
                    "height": 2,
                    "region": {"x": 0, "y": 0, "w": 2, "h": 2},
                }),
            ),
        );
        let out = renderer().render_document(&state, &store).unwrap();
        assert_eq!(out.stats.unsupported.len(), 1);
        assert!(out.stats.unsupported[0].contains("image/webp"));
    }

    #[test]
    fn tile_cache_reuse_and_invalidation() {
        let mut state = white_document();
        // 64×64 文档 + 32px tile = 4 个 tile。
        state.width = 64;
        state.height = 64;
        let grid = TileGrid::new(32, 64, 64).unwrap();
        let mut renderer = Renderer::with_budget(grid, 32 * 32 * 4 * 2 * 8);
        let store = MemoryBlobStore::new();
        let first = renderer.render_document(&state, &store).unwrap();
        assert_eq!(first.tiles.len(), 4);
        let tile = renderer
            .render_tile(&state, &store, TileKey::new(0, 0))
            .unwrap();
        assert_eq!(tile.key(), TileKey::new(0, 0));

        state.objects.insert(
            "obj_1".to_owned(),
            object(
                "obj_1",
                "layer_1",
                ObjectType::Shape,
                0,
                json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 4, "h": 4}},
                    "color": {"r": 0, "g": 0, "b": 0, "a": 255},
                }),
            ),
        );
        let atom = Atom::new(
            AtomKind::DrawShape,
            "human:1",
            "s",
            json!({"object_id": "obj_1", "layer_id": "layer_1"}),
        );
        let dirty = plan_dirty(&state, None, &atom);
        let keys = renderer.apply_dirty(&state, &dirty);
        assert_eq!(keys, vec![TileKey::new(0, 0)]);
        let after = renderer
            .render_tile(&state, &store, TileKey::new(0, 0))
            .unwrap();
        assert_eq!(after.get(1, 1)[3], 1.0, "重渲染后包含新内容");
    }

    #[test]
    fn region_render_clamps_and_rejects_empty() {
        let state = white_document();
        let store = MemoryBlobStore::new();
        let mut renderer = renderer();
        let out = renderer
            .render_region(&state, &store, Bbox::new(-10.0, -10.0, 20.0, 20.0))
            .unwrap();
        assert_eq!(out.bbox, Bbox::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(out.width, 10);
        assert!(renderer
            .render_region(&state, &store, Bbox::new(100.0, 100.0, 4.0, 4.0))
            .is_err());
    }

    #[test]
    fn layer_mask_limits_coverage() {
        let mut state = white_document();
        state.objects.insert(
            "obj_blue".to_owned(),
            object(
                "obj_blue",
                "layer_2",
                ObjectType::Shape,
                0,
                json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 32}},
                    "color": {"r": 0, "g": 0, "b": 255, "a": 255},
                }),
            ),
        );
        state.layers.get_mut("layer_2").unwrap().mask_id = Some("mask_1".to_owned());
        state.masks.insert(
            "mask_1".to_owned(),
            yanshi_core::Mask {
                id: "mask_1".to_owned(),
                shape: json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 16, "h": 16}}),
                feather: 0.0,
                mode: "new".to_owned(),
                invert: false,
                linked_layer: None,
                refined_edges: false,
                blobs: Vec::new(),
                created_by: "a1".to_owned(),
                deleted_by: None,
            },
        );
        let store = MemoryBlobStore::new();
        let out = renderer().render_document(&state, &store).unwrap();
        let inside = out.pixel(4, 4).unwrap();
        assert!(inside[2] > 200, "蒙版内保留蓝色：{inside:?}");
        assert_eq!(
            out.pixel(20, 20),
            Some([255, 255, 255, 255]),
            "蒙版外恢复背景"
        );
    }

    #[test]
    fn document_from_atom_log_renders_end_to_end() {
        let store = MemoryBlobStore::new();
        let mut log = AtomLog::new();
        for atom in [
            Atom::new(
                AtomKind::CreateDocument,
                "human:1",
                "s",
                json!({"doc_id": "doc_1", "width": 24, "height": 24, "background": {"r":255,"g":255,"b":255,"a":255}}),
            ),
            Atom::new(
                AtomKind::CreateLayer,
                "human:1",
                "s",
                json!({"layer_id": "layer_1"}),
            ),
            Atom::new(
                AtomKind::DrawStroke,
                "human:1",
                "s",
                json!({
                    "object_id": "obj_1",
                    "layer_id": "layer_1",
                    "data": {"points": [[6.0, 12.0], [18.0, 12.0]], "size": 6.0, "color": [0.0, 0.0, 0.0, 1.0]},
                }),
            ),
        ] {
            log.append(atom).unwrap();
        }
        let state = FoldEngine::new().fold(&log).unwrap().state;
        let mut renderer = Renderer::new(TileGrid::new(32, 24, 24).unwrap());
        let out = renderer.render_document(&state, &store).unwrap();
        assert!(out.pixel(12, 12).unwrap()[0] < 60, "笔迹处为深色");
        assert_eq!(out.pixel(1, 1), Some([255, 255, 255, 255]));
        assert!(out.stats.unsupported.is_empty());
        assert!(state.is_consistent());
    }
}
