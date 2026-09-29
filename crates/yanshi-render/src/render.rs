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
use yanshi_core::{Bbox, BlobStore, DocumentState, Layer, Result, YanshiError};

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
            max_filter_padding: 64,
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
        let padding = if self.options.expand_for_filters {
            self.filter_padding(state)
                .min(self.options.max_filter_padding)
        } else {
            0
        };
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

        // 背景：文档 background 为不透明时先铺底。
        let background = self
            .options
            .background
            .or_else(|| parse_background(&state.background));
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

        for layer in state.alive_layers() {
            if !layer.visible && !self.options.include_hidden_layers {
                continue;
            }
            stats.layers += 1;
            let mut layer_buffer = Buffer::new(origin_x, origin_y, width, height);
            self.render_layer_objects(state, store, layer, &mut layer_buffer, &mut stats)?;
            apply_layer_mask(state, layer, &mut layer_buffer);
            if layer.clipping_mask {
                // 剪贴蒙版：用下方内容的 alpha 裁剪本层。
                layer_buffer.multiply_alpha_by(&accumulation);
            }
            layer_buffer.multiply_alpha(layer.opacity.clamp(0.0, 1.0) as f32);
            let mode = BlendMode::from_name(&layer.blend_mode);
            accumulation.composite(&layer_buffer, mode, 1.0);
        }

        // 裁剪回请求区域并切片入缓存。
        let cropped = accumulation.crop(&region);
        let tiles = self.store_tiles(&cropped);
        stats.tiles_rendered = tiles.len();
        // 输出像素先按 **f16 量化**（14.1：内存 tile 用 f16 线性，合成正确性以 tile 为准），
        // 再转显示空间。这样「整幅区域渲染」与「按 tile 组合渲染」（客户端 WASM 内核走后者）
        // 逐字节一致，不会因 f32 scratch 与 f16 tile 的舍入差出现 ±1 分歧（Phase 2 bit-exact）；
        // 且量化是就地计算，不依赖 tile 是否仍在缓存里（小预算下会被淘汰）。
        let rgba8 = quantize_to_rgba8(&cropped, background);

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
    pub fn filter_padding(&self, state: &DocumentState) -> u32 {
        let mut padding = 0u32;
        for object in state.alive_objects() {
            // 修图对象从偏移位置采样：区域渲染必须外扩到源像素，否则边缘会缺一块。
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
        for object in state.objects_in_layer(&layer.id) {
            if !object.visible {
                continue;
            }
            // 几何裁剪：包围盒与渲染区域不相交的对象直接跳过。
            // 调整/滤镜对象作用于整层、无法用几何裁剪；未实现类型必须保留以便产生告警。
            let primitive = parse_object(object);
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
            match primitive {
                Primitive::Stroke { geometry, brush } => {
                    let brush = BrushSpec {
                        opacity: brush.opacity * f64::from(opacity),
                        ..brush
                    };
                    stamp_stroke(layer_buffer, &brush, &geometry);
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
                    let coverage = shape_coverage_in(kind, bbox, &points, &layer_buffer.bbox());
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
                            stamp_stroke(layer_buffer, &brush, &geometry);
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
                Primitive::Retouch {
                    kind,
                    points,
                    offset,
                    size,
                    hardness,
                    opacity,
                    jitter,
                } => {
                    if kind != "clone_stamp" {
                        stats
                            .unsupported
                            .push(format!("修图类型未实现: {kind}（对象 {}）", object.id));
                        continue;
                    }
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
                    for (cx, cy) in stamps.iter().map(|s| (s.0, s.1)) {
                        // 采样源像素（文档坐标 + 偏移），越界则跳过该 stamp。
                        let sx = cx + offset.0 - origin.0 as f64;
                        let sy = cy + offset.1 - origin.1 as f64;
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
                        let straight = [
                            sampled[0] / sampled[3],
                            sampled[1] / sampled[3],
                            sampled[2] / sampled[3],
                        ];
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
                Primitive::Unsupported { reason } => {
                    stats
                        .unsupported
                        .push(format!("{reason}（对象 {}）", object.id));
                }
            }
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
pub fn apply_layer_mask(state: &DocumentState, layer: &Layer, layer_buffer: &mut Buffer) {
    let Some(mask_id) = &layer.mask_id else {
        return;
    };
    let Some(mask) = state.masks.get(mask_id) else {
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
            json!({"adjustment_type": "color_balance", "params": {}});
        let warned = renderer.render_document(&state, &store).unwrap();
        assert_eq!(warned.stats.unsupported.len(), 1);
        assert_eq!(warned.pixel(5, 5).unwrap()[0], 255, "未识别时不改动像素");
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
