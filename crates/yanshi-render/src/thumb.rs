//! 缩略图管线（设计文档 7 章）。
//!
//! 原子 → L2 状态 → ThumbState → ThumbBlock → 缩略图输出。
//!
//! - 分类与尺寸遵循 7.2：`doc_thumb` 64/128/256、`layer_thumb` 32/64、
//!   `object_thumb` 16/32、`history_thumb` 32/64、`selection_thumb` 32。
//! - 7.4 增量更新：缩略图分块（32×32），只重渲染 dirty 块。
//! - 7.3 渲染方式：本模块实现「全分辨率降采样」（第 3 项）。
//!   「对象重投影」与「低分辨率直接渲染」需要缩放渲染路径，属后续工作。

use crate::buffer::Buffer;
use crate::color::linear_premul_to_u8x4;
use crate::render::Renderer;
use std::collections::BTreeSet;
use yanshi_core::{Bbox, BlobStore, DocumentState, Result};

/// 缩略图分块边长（7.4）。
pub const THUMB_BLOCK: u32 = 32;

/// 缩略图类型与尺寸（7.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ThumbKind {
    /// 文档缩略图 64。
    Doc64,
    /// 文档缩略图 128。
    Doc128,
    /// 文档缩略图 256。
    Doc256,
    /// 图层缩略图 32。
    Layer32,
    /// 图层缩略图 64。
    Layer64,
    /// 对象缩略图 16。
    Object16,
    /// 对象缩略图 32。
    Object32,
    /// 历史缩略图 32。
    History32,
    /// 历史缩略图 64。
    History64,
    /// 选区缩略图 32。
    Selection32,
}

impl ThumbKind {
    /// 全部类型。
    pub const ALL: [ThumbKind; 10] = [
        Self::Doc64,
        Self::Doc128,
        Self::Doc256,
        Self::Layer32,
        Self::Layer64,
        Self::Object16,
        Self::Object32,
        Self::History32,
        Self::History64,
        Self::Selection32,
    ];

    /// 边长（像素）。
    pub const fn size(self) -> u32 {
        match self {
            Self::Doc64 => 64,
            Self::Doc128 => 128,
            Self::Doc256 => 256,
            Self::Layer32 | Self::Object32 | Self::History32 | Self::Selection32 => 32,
            Self::Layer64 | Self::History64 => 64,
            Self::Object16 => 16,
        }
    }

    /// 所属类别（7.2 表格的第一列）。
    pub const fn category(self) -> &'static str {
        match self {
            Self::Doc64 | Self::Doc128 | Self::Doc256 => "doc_thumb",
            Self::Layer32 | Self::Layer64 => "layer_thumb",
            Self::Object16 | Self::Object32 => "object_thumb",
            Self::History32 | Self::History64 => "history_thumb",
            Self::Selection32 => "selection_thumb",
        }
    }

    /// 字符串名（用于 URL 与可观测性）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Doc64 => "doc_64",
            Self::Doc128 => "doc_128",
            Self::Doc256 => "doc_256",
            Self::Layer32 => "layer_32",
            Self::Layer64 => "layer_64",
            Self::Object16 => "object_16",
            Self::Object32 => "object_32",
            Self::History32 => "history_32",
            Self::History64 => "history_64",
            Self::Selection32 => "selection_32",
        }
    }
}

/// 缩略图统计。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ThumbStats {
    /// 总块数。
    pub blocks_total: usize,
    /// 本次重算的块数。
    pub blocks_rendered: usize,
    /// 源像素数。
    pub source_pixels: usize,
}

/// 一张缩略图（u8 RGBA，显示空间，直通 alpha）。
#[derive(Debug, Clone, PartialEq)]
pub struct Thumb {
    /// 类型。
    pub kind: ThumbKind,
    /// 边长。
    pub size: u32,
    /// 像素（行主序 RGBA）。
    pub rgba8: Vec<u8>,
    /// 统计。
    pub stats: ThumbStats,
}

impl Thumb {
    /// 全透明缩略图。
    pub fn new(kind: ThumbKind) -> Self {
        let size = kind.size();
        Self {
            kind,
            size,
            rgba8: vec![0; (size * size * 4) as usize],
            stats: ThumbStats {
                blocks_total: Self::block_count(size),
                ..ThumbStats::default()
            },
        }
    }

    /// 每轴的块数。
    pub fn blocks_per_axis(&self) -> u32 {
        self.size.div_ceil(THUMB_BLOCK)
    }

    /// 总块数。
    pub fn block_count(size: u32) -> usize {
        let per_axis = size.div_ceil(THUMB_BLOCK);
        (per_axis * per_axis) as usize
    }

    /// 某像素。
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size || y >= self.size {
            return None;
        }
        let index = ((y * self.size + x) * 4) as usize;
        Some([
            self.rgba8[index],
            self.rgba8[index + 1],
            self.rgba8[index + 2],
            self.rgba8[index + 3],
        ])
    }

    /// 块覆盖的缩略图像素范围。
    pub fn block_bounds(&self, block_x: u32, block_y: u32) -> Bbox {
        let x0 = (block_x * THUMB_BLOCK).min(self.size) as f64;
        let y0 = (block_y * THUMB_BLOCK).min(self.size) as f64;
        let x1 = ((block_x + 1) * THUMB_BLOCK).min(self.size) as f64;
        let y1 = ((block_y + 1) * THUMB_BLOCK).min(self.size) as f64;
        Bbox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// 内容在缩略图内的排布（保持宽高比、居中，其余留空）。
    ///
    /// 设计文档只规定缩略图边长（64/128/256 等），文档本身可能不是正方形；
    /// 这里按“最长边适配 + 居中”排版，避免把内容拉伸变形。
    pub fn content_box(&self, doc_size: Bbox) -> Bbox {
        if doc_size.w <= 0.0 || doc_size.h <= 0.0 {
            return Bbox::new(0.0, 0.0, 0.0, 0.0);
        }
        let scale = (self.size as f64 / doc_size.w).min(self.size as f64 / doc_size.h);
        let width = (doc_size.w * scale).round().max(1.0);
        let height = (doc_size.h * scale).round().max(1.0);
        Bbox::new(
            ((self.size as f64 - width) / 2.0).floor(),
            ((self.size as f64 - height) / 2.0).floor(),
            width,
            height,
        )
    }

    /// 文档区域对应的 dirty 块集合（7.4）。
    pub fn dirty_blocks_for(&self, doc_bbox: &Bbox, doc_size: Bbox) -> BTreeSet<(u32, u32)> {
        let mut blocks = BTreeSet::new();
        let content = self.content_box(doc_size);
        if content.w <= 0.0 || content.h <= 0.0 {
            return blocks;
        }
        let scale_x = content.w / doc_size.w.max(1.0);
        let scale_y = content.h / doc_size.h.max(1.0);
        let x0 = (content.x + (doc_bbox.x - doc_size.x) * scale_x)
            .floor()
            .max(0.0) as u32;
        let y0 = (content.y + (doc_bbox.y - doc_size.y) * scale_y)
            .floor()
            .max(0.0) as u32;
        // 右/下边界是开区间：用 `ceil - 1` 求最后一个被覆盖的像素。
        let x1 = (content.x + (doc_bbox.x + doc_bbox.w - doc_size.x) * scale_x)
            .ceil()
            .max(1.0) as u32;
        let y1 = (content.y + (doc_bbox.y + doc_bbox.h - doc_size.y) * scale_y)
            .ceil()
            .max(1.0) as u32;
        let last_x = (x1 - 1).min(self.size.saturating_sub(1)) / THUMB_BLOCK;
        let last_y = (y1 - 1).min(self.size.saturating_sub(1)) / THUMB_BLOCK;
        let last_block = self.blocks_per_axis().saturating_sub(1);
        for by in (y0 / THUMB_BLOCK).min(last_block)..=last_y.min(last_block) {
            for bx in (x0 / THUMB_BLOCK).min(last_block)..=last_x.min(last_block) {
                blocks.insert((bx, by));
            }
        }
        blocks
    }

    /// 用源缓冲区刷新整张缩略图（全分辨率降采样）。
    pub fn update_full(&mut self, source: &Buffer, doc_size: Bbox) {
        let all: BTreeSet<(u32, u32)> = (0..self.blocks_per_axis())
            .flat_map(|by| (0..self.blocks_per_axis()).map(move |bx| (bx, by)))
            .collect();
        self.update_blocks(source, doc_size, &all);
    }

    /// 只刷新指定块（7.4 增量更新）；未包含的块保持原值。
    pub fn update_blocks(
        &mut self,
        source: &Buffer,
        doc_size: Bbox,
        blocks: &BTreeSet<(u32, u32)>,
    ) {
        let content = self.content_box(doc_size);
        if content.w <= 0.0 || content.h <= 0.0 {
            self.stats.blocks_rendered = 0;
            return;
        }
        let mut rendered = 0usize;
        for (block_x, block_y) in blocks {
            let bounds = self.block_bounds(*block_x, *block_y);
            let start_x = bounds.x as u32;
            let start_y = bounds.y as u32;
            let end_x = (bounds.x + bounds.w) as u32;
            let end_y = (bounds.y + bounds.h) as u32;
            for thumb_y in start_y..end_y {
                for thumb_x in start_x..end_x {
                    // 缩略图内容区像素映射回源缓冲区的像素窗口。
                    let u0 = (thumb_x as f64 - content.x) / content.w;
                    let u1 = ((thumb_x + 1) as f64 - content.x) / content.w;
                    let v0 = (thumb_y as f64 - content.y) / content.h;
                    let v1 = ((thumb_y + 1) as f64 - content.y) / content.h;
                    if u1 <= 0.0 || v1 <= 0.0 || u0 >= 1.0 || v0 >= 1.0 {
                        continue; // 留白区域保持透明
                    }
                    let source_x0 = (u0.clamp(0.0, 1.0) * source.width() as f64)
                        .floor()
                        .max(0.0) as u32;
                    let source_y0 = (v0.clamp(0.0, 1.0) * source.height() as f64)
                        .floor()
                        .max(0.0) as u32;
                    let source_x1 =
                        (u1.clamp(0.0, 1.0) * source.width() as f64).ceil().max(1.0) as u32;
                    let source_y1 = (v1.clamp(0.0, 1.0) * source.height() as f64)
                        .ceil()
                        .max(1.0) as u32;
                    let mut accumulator = [0.0f64; 4];
                    let mut count = 0.0f64;
                    for source_y in source_y0..source_y1.max(source_y0 + 1) {
                        for source_x in source_x0..source_x1.max(source_x0 + 1) {
                            if source_x >= source.width() || source_y >= source.height() {
                                continue;
                            }
                            let pixel = source.pixel(source_x, source_y);
                            for channel in 0..4 {
                                accumulator[channel] += pixel[channel] as f64;
                            }
                            count += 1.0;
                        }
                    }
                    // 若源窗口完全落在缓冲区之外，保持透明（不写入）。
                    if count == 0.0 {
                        continue;
                    }
                    let averaged = [
                        (accumulator[0] / count) as f32,
                        (accumulator[1] / count) as f32,
                        (accumulator[2] / count) as f32,
                        (accumulator[3] / count) as f32,
                    ];
                    let bytes = linear_premul_to_u8x4(averaged);
                    let index = ((thumb_y * self.size + thumb_x) * 4) as usize;
                    self.rgba8[index..index + 4].copy_from_slice(&bytes);
                }
            }
            rendered += 1;
        }
        self.stats.blocks_rendered = rendered;
        self.stats.source_pixels = (source.width() * source.height()) as usize;
    }
}

/// 渲染一张缩略图（文档 / 图层 / 对象区域）。
///
/// `target` 为要生成缩略图的文档区域；`None` 表示整篇文档。
pub fn render_thumbnail(
    renderer: &mut Renderer,
    state: &DocumentState,
    store: &dyn BlobStore,
    kind: ThumbKind,
    target: Option<Bbox>,
) -> Result<Thumb> {
    let region =
        target.unwrap_or_else(|| Bbox::new(0.0, 0.0, state.width as f64, state.height as f64));
    let render = renderer.render_region(state, store, region)?;
    let mut buffer = Buffer::new(0, 0, render.width, render.height);
    for y in 0..render.height {
        for x in 0..render.width {
            if let Some(pixel) = render.pixel(x, y) {
                buffer.set_pixel(x, y, crate::color::u8x4_to_linear_premul(pixel));
            }
        }
    }
    let mut thumb = Thumb::new(kind);
    thumb.update_full(&buffer, region);
    Ok(thumb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::TileGrid;
    use serde_json::json;
    use yanshi_core::blob::MemoryBlobStore;
    use yanshi_core::{Layer, LayerType, Object, ObjectType, Transform};

    fn document_with_rect() -> DocumentState {
        let mut state = DocumentState::empty();
        state.width = 64;
        state.height = 64;
        state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
        state.layers.insert(
            "layer_1".to_owned(),
            Layer {
                id: "layer_1".to_owned(),
                name: "l".to_owned(),
                layer_type: LayerType::Raster,
                parent_id: None,
                z_index: 0,
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
                metadata: serde_json::Value::Null,
                blobs: Vec::new(),
                created_by: "a1".to_owned(),
                updated_by: None,
                deleted_by: None,
            },
        );
        state.objects.insert(
            "obj_1".to_owned(),
            Object {
                id: "obj_1".to_owned(),
                layer_id: "layer_1".to_owned(),
                object_type: ObjectType::Shape,
                z_index: 0,
                visible: true,
                locked: false,
                metadata: serde_json::Value::Null,
                transform: Transform::IDENTITY,
                style: None,
                versions: vec!["a1".to_owned()],
                current_version: Some("a1".to_owned()),
                created_by: "a1".to_owned(),
                deleted_by: None,
                data: json!({
                    "geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 32}},
                    "color": {"r": 0, "g": 0, "b": 0, "a": 255},
                }),
                blobs: Vec::new(),
            },
        );
        state
    }

    #[test]
    fn thumb_kinds_match_the_design_table() {
        assert_eq!(ThumbKind::Doc64.size(), 64);
        assert_eq!(ThumbKind::Doc128.size(), 128);
        assert_eq!(ThumbKind::Doc256.size(), 256);
        assert_eq!(ThumbKind::Layer32.size(), 32);
        assert_eq!(ThumbKind::Layer64.size(), 64);
        assert_eq!(ThumbKind::Object16.size(), 16);
        assert_eq!(ThumbKind::Object32.size(), 32);
        assert_eq!(ThumbKind::History32.size(), 32);
        assert_eq!(ThumbKind::History64.size(), 64);
        assert_eq!(ThumbKind::Selection32.size(), 32);

        assert_eq!(ThumbKind::Doc256.category(), "doc_thumb");
        assert_eq!(ThumbKind::Layer32.category(), "layer_thumb");
        assert_eq!(ThumbKind::Object16.category(), "object_thumb");
        assert_eq!(ThumbKind::History64.category(), "history_thumb");
        assert_eq!(ThumbKind::Selection32.category(), "selection_thumb");
        assert_eq!(ThumbKind::ALL.len(), 10);
        assert_eq!(ThumbKind::Doc64.as_str(), "doc_64");
    }

    #[test]
    fn thumb_from_render_downsamples_document() {
        let state = document_with_rect();
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 64, 64).unwrap());
        let thumb =
            render_thumbnail(&mut renderer, &state, &store, ThumbKind::Doc64, None).unwrap();
        assert_eq!(thumb.size, 64);
        assert_eq!(thumb.rgba8.len(), 64 * 64 * 4);
        // 左上区域是黑矩形，右下是白背景。
        let dark = thumb.pixel(4, 4).unwrap();
        assert!(dark[0] < 40, "{dark:?}");
        let light = thumb.pixel(60, 60).unwrap();
        assert!(light[0] > 200, "{light:?}");
        assert_eq!(thumb.stats.blocks_rendered, thumb.stats.blocks_total);
    }

    #[test]
    fn layer_and_object_thumbnails_use_target_region() {
        let state = document_with_rect();
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 64, 64).unwrap());
        let layer = render_thumbnail(
            &mut renderer,
            &state,
            &store,
            ThumbKind::Layer32,
            Some(Bbox::new(0.0, 0.0, 32.0, 32.0)),
        )
        .unwrap();
        assert_eq!(layer.size, 32);
        // 目标区域整块都是黑矩形：四个角也是深色。
        assert!(layer.pixel(1, 1).unwrap()[0] < 40);
        assert!(layer.pixel(30, 30).unwrap()[0] < 40);

        let object = render_thumbnail(
            &mut renderer,
            &state,
            &store,
            ThumbKind::Object16,
            Some(Bbox::new(0.0, 0.0, 16.0, 16.0)),
        )
        .unwrap();
        assert_eq!(object.size, 16);
        assert!(object.pixel(8, 8).unwrap()[0] < 40);
    }

    #[test]
    fn non_square_documents_are_letterboxed() {
        // 64×32 文档 → 64 方形缩略图：内容应为 64×32 居中，上下留空。
        let mut state = document_with_rect();
        state.width = 64;
        state.height = 32;
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 64, 32).unwrap());
        let thumb =
            render_thumbnail(&mut renderer, &state, &store, ThumbKind::Doc64, None).unwrap();
        let content = thumb.content_box(Bbox::new(0.0, 0.0, 64.0, 32.0));
        assert_eq!((content.w, content.h), (64.0, 32.0));
        assert_eq!(content.y, 16.0, "纵向居中留白");

        // 内容区之内有像素，之外透明。
        assert!(thumb.pixel(32, 32).unwrap()[3] > 0, "内容区内可见");
        assert_eq!(thumb.pixel(32, 4).unwrap(), [0, 0, 0, 0], "上方留白透明");
        assert_eq!(thumb.pixel(32, 60).unwrap(), [0, 0, 0, 0], "下方留白透明");

        // dirty 块按内容区映射：整篇文档 → 覆盖全部块。
        let blocks = thumb.dirty_blocks_for(
            &Bbox::new(0.0, 0.0, 64.0, 32.0),
            Bbox::new(0.0, 0.0, 64.0, 32.0),
        );
        assert_eq!(blocks.len(), thumb.stats.blocks_total);
    }

    #[test]
    fn incremental_blocks_match_full_update() {
        let state = document_with_rect();
        let store = MemoryBlobStore::new();
        let mut renderer = Renderer::new(TileGrid::new(32, 64, 64).unwrap());
        let full = render_thumbnail(&mut renderer, &state, &store, ThumbKind::Doc64, None).unwrap();

        // 先渲染一份“空的”，再只更新 dirty 块。
        let region = Bbox::new(0.0, 0.0, 64.0, 64.0);
        let render = renderer.render_region(&state, &store, region).unwrap();
        let mut buffer = Buffer::new(0, 0, render.width, render.height);
        for y in 0..render.height {
            for x in 0..render.width {
                if let Some(pixel) = render.pixel(x, y) {
                    buffer.set_pixel(x, y, crate::color::u8x4_to_linear_premul(pixel));
                }
            }
        }
        let mut incremental = Thumb::new(ThumbKind::Doc64);
        let dirty = incremental.dirty_blocks_for(&Bbox::new(0.0, 0.0, 32.0, 32.0), region);
        assert!(!dirty.is_empty());
        incremental.update_blocks(&buffer, region, &dirty);
        assert!(incremental.stats.blocks_rendered < incremental.stats.blocks_total);

        // 只更新了 dirty 块：dirty 块内容与全量一致，其余保持透明。
        for (bx, by) in dirty {
            let bounds = incremental.block_bounds(bx, by);
            for y in (bounds.y as u32)..((bounds.y + bounds.h) as u32) {
                for x in (bounds.x as u32)..((bounds.x + bounds.w) as u32) {
                    assert_eq!(
                        incremental.pixel(x, y),
                        full.pixel(x, y),
                        "块 ({bx},{by}) 像素 ({x},{y})"
                    );
                }
            }
        }

        // 更新全部块后与全量结果逐字节一致（幂等）。
        let mut again = Thumb::new(ThumbKind::Doc64);
        let all: BTreeSet<(u32, u32)> = (0..again.blocks_per_axis())
            .flat_map(|by| (0..again.blocks_per_axis()).map(move |bx| (bx, by)))
            .collect();
        again.update_blocks(&buffer, region, &all);
        assert_eq!(again.rgba8, full.rgba8);
    }

    #[test]
    fn dirty_blocks_cover_scaled_region() {
        let thumb = Thumb::new(ThumbKind::Doc128);
        let doc = Bbox::new(0.0, 0.0, 256.0, 256.0);
        let blocks = thumb.dirty_blocks_for(&Bbox::new(0.0, 0.0, 32.0, 32.0), doc);
        assert_eq!(blocks, [(0, 0)].into_iter().collect());
        // 文档 (120,120,16,16) 映射到缩略图 (60,60)-(68,68)，跨 32px 块边界。
        let blocks = thumb.dirty_blocks_for(&Bbox::new(120.0, 120.0, 16.0, 16.0), doc);
        assert_eq!(
            blocks,
            [(1, 1), (1, 2), (2, 1), (2, 2)].into_iter().collect()
        );
        let all = thumb.dirty_blocks_for(&doc, doc);
        assert_eq!(all.len(), thumb.stats.blocks_total);
    }
}
