//! Tile、Tile 网格与 Tile 缓存（设计文档 6.4 / 8.4 / 13.3 / 14.1）。
//!
//! - L3 缓存：**Tile 是唯一物化形式**，图层级操作从 tiles 拼装，不存在独立的图层全量位图缓存。
//! - 内存 tile 用 f16 线性预乘 RGBA（[`crate::half`]），`256×256` 或 `512×512`。
//! - 缓存按字节预算做 LRU；客户端 WASM 侧另有「硬上限 90% 自动 evict」与
//!   `evict_outside_viewport(bbox)` 的视口淘汰（设计文档 13.3）。

use crate::color::{linear_premul_to_u8x4, u8x4_to_linear_premul, LinearRgba};
use crate::half::{f16_bits_to_f32, f32_to_f16_bits};
use std::collections::{HashMap, VecDeque};
use yanshi_core::Bbox;

/// 默认 tile 边长（像素）。
pub const DEFAULT_TILE_SIZE: u32 = 256;
/// 允许的 tile 边长。
pub const ALLOWED_TILE_SIZES: [u32; 5] = [32, 64, 128, 256, 512];

/// Tile 在网格中的坐标。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    /// 列。
    pub x: u32,
    /// 行。
    pub y: u32,
}

impl TileKey {
    /// 构造。
    pub const fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// 一个 tile：f16 线性预乘 RGBA 像素数组。
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    key: TileKey,
    size: u32,
    pixels: Vec<u16>,
}

impl Tile {
    /// 全透明 tile。
    pub fn new(key: TileKey, size: u32) -> Self {
        Self {
            key,
            size,
            pixels: vec![0u16; (size * size * 4) as usize],
        }
    }

    /// 由 f32 线性预乘像素构造（长度必须是 `size*size*4`）。
    pub fn from_f32(key: TileKey, size: u32, data: &[f32]) -> Option<Self> {
        if data.len() != (size * size * 4) as usize {
            return None;
        }
        let pixels = data.iter().map(|value| f32_to_f16_bits(*value)).collect();
        Some(Self { key, size, pixels })
    }

    /// tile 坐标。
    pub const fn key(&self) -> TileKey {
        self.key
    }

    /// tile 边长。
    pub const fn size(&self) -> u32 {
        self.size
    }

    /// 是否全透明（渲染占位判断与空 tile 优化）。
    pub fn is_transparent(&self) -> bool {
        self.pixels.chunks_exact(4).all(|pixel| pixel[3] == 0)
    }

    /// 读取像素（越界返回全透明）。
    pub fn get(&self, x: u32, y: u32) -> LinearRgba {
        if x >= self.size || y >= self.size {
            return [0.0; 4];
        }
        let base = ((y * self.size + x) * 4) as usize;
        [
            f16_bits_to_f32(self.pixels[base]),
            f16_bits_to_f32(self.pixels[base + 1]),
            f16_bits_to_f32(self.pixels[base + 2]),
            f16_bits_to_f32(self.pixels[base + 3]),
        ]
    }

    /// 写入像素（越界忽略；写入即量化到 f16）。
    pub fn set(&mut self, x: u32, y: u32, pixel: LinearRgba) {
        if x >= self.size || y >= self.size {
            return;
        }
        let base = ((y * self.size + x) * 4) as usize;
        for (offset, bits) in self.pixels[base..base + 4].iter_mut().zip(pixel) {
            *offset = f32_to_f16_bits(bits);
        }
    }

    /// 以 source-over 叠加像素。
    pub fn blend_over(&mut self, x: u32, y: u32, source: LinearRgba) {
        let backdrop = self.get(x, y);
        self.set(x, y, crate::blend::over(source, backdrop));
    }

    /// 填充整块 tile。
    pub fn fill(&mut self, pixel: LinearRgba) {
        for y in 0..self.size {
            for x in 0..self.size {
                self.set(x, y, pixel);
            }
        }
    }

    /// 导出为 f32 线性预乘像素。
    pub fn to_f32(&self) -> Vec<f32> {
        self.pixels
            .iter()
            .map(|bits| f16_bits_to_f32(*bits))
            .collect()
    }

    /// 导出为 u8 RGBA（显示空间）；`background` 为不透明底色。
    pub fn to_rgba8(&self, background: Option<[u8; 4]>) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.size * self.size * 4) as usize);
        for y in 0..self.size {
            for x in 0..self.size {
                let pixel = self.get(x, y);
                let bytes = match background {
                    Some(bg) => crate::color::composite_over_background(pixel, bg),
                    None => linear_premul_to_u8x4(pixel),
                };
                out.extend_from_slice(&bytes);
            }
        }
        out
    }

    /// 该 tile 的字节数（f16 RGBA）。
    pub fn byte_len(&self) -> usize {
        self.pixels.len() * 2
    }

    /// 用 u8 RGBA（显示空间，直通 alpha）覆盖写入，用于 RasterPatch / import_image。
    pub fn blit_rgba8(&mut self, x0: i64, y0: i64, width: u32, height: u32, rgba8: &[u8]) {
        for row in 0..height {
            for col in 0..width {
                let index = ((row * width + col) * 4) as usize;
                if index + 3 >= rgba8.len() {
                    return;
                }
                let pixel = [
                    rgba8[index],
                    rgba8[index + 1],
                    rgba8[index + 2],
                    rgba8[index + 3],
                ];
                let x = x0 + col as i64;
                let y = y0 + row as i64;
                if x < 0 || y < 0 {
                    continue;
                }
                let (x, y) = (x as u32, y as u32);
                if x >= self.size || y >= self.size {
                    continue;
                }
                // 贴图按 source-over 覆盖（patch 自带 alpha）。
                self.blend_over(x, y, u8x4_to_linear_premul(pixel));
            }
        }
    }
}

/// 文档的 tile 网格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGrid {
    tile_size: u32,
    width: u32,
    height: u32,
}

impl TileGrid {
    /// 构造（tile 边长必须是 [`ALLOWED_TILE_SIZES`] 之一）。
    pub fn new(tile_size: u32, width: u32, height: u32) -> Option<Self> {
        if !ALLOWED_TILE_SIZES.contains(&tile_size) {
            return None;
        }
        Some(Self {
            tile_size,
            width,
            height,
        })
    }

    /// tile 边长。
    pub const fn tile_size(&self) -> u32 {
        self.tile_size
    }

    /// 文档宽。
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 文档高。
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// 横向 tile 数。
    pub fn tiles_x(&self) -> u32 {
        self.width.div_ceil(self.tile_size)
    }

    /// 纵向 tile 数。
    pub fn tiles_y(&self) -> u32 {
        self.height.div_ceil(self.tile_size)
    }

    /// tile 总数。
    pub fn tile_count(&self) -> usize {
        (self.tiles_x() * self.tiles_y()) as usize
    }

    /// 点所在 tile。
    pub fn key_of(&self, x: u32, y: u32) -> TileKey {
        TileKey::new(x / self.tile_size, y / self.tile_size)
    }

    /// 点（文档坐标）所在 tile，越界返回 None。
    pub fn key_at_point(&self, x: f64, y: f64) -> Option<TileKey> {
        if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
            return None;
        }
        Some(self.key_of(x as u32, y as u32))
    }

    /// tile 是否在文档内。
    pub fn contains(&self, key: TileKey) -> bool {
        key.x < self.tiles_x() && key.y < self.tiles_y()
    }

    /// tile 在文档坐标下的边界（裁剪到文档范围）。
    pub fn bounds(&self, key: TileKey) -> Bbox {
        let x0 = (key.x * self.tile_size).min(self.width) as f64;
        let y0 = (key.y * self.tile_size).min(self.height) as f64;
        let x1 = ((key.x + 1) * self.tile_size).min(self.width) as f64;
        let y1 = ((key.y + 1) * self.tile_size).min(self.height) as f64;
        Bbox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// 与包围盒相交的 tile 集合（升序，确定性）。
    pub fn keys_for_bbox(&self, bbox: &Bbox) -> Vec<TileKey> {
        if bbox.w <= 0.0 || bbox.h <= 0.0 {
            return Vec::new();
        }
        let x0 = bbox.x.max(0.0);
        let y0 = bbox.y.max(0.0);
        let x1 = (bbox.x + bbox.w).min(self.width as f64);
        let y1 = (bbox.y + bbox.h).min(self.height as f64);
        if x1 <= x0 || y1 <= y0 {
            return Vec::new();
        }
        let start = self.key_of(x0.floor() as u32, y0.floor() as u32);
        let end = self.key_of(
            ((x1 - 1e-9).floor().max(0.0) as u32).min(self.width.saturating_sub(1)),
            ((y1 - 1e-9).floor().max(0.0) as u32).min(self.height.saturating_sub(1)),
        );
        let mut keys = Vec::new();
        for ty in start.y..=end.y {
            for tx in start.x..=end.x {
                keys.push(TileKey::new(tx, ty));
            }
        }
        keys
    }

    /// 全部 tile（升序）。
    pub fn all_keys(&self) -> Vec<TileKey> {
        let mut keys = Vec::new();
        for ty in 0..self.tiles_y() {
            for tx in 0..self.tiles_x() {
                keys.push(TileKey::new(tx, ty));
            }
        }
        keys
    }
}

/// Tile 缓存统计（设计文档 14.9 可观测性）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TileCacheStats {
    /// 命中次数。
    pub hits: u64,
    /// 未命中次数。
    pub misses: u64,
    /// 淘汰次数。
    pub evictions: u64,
    /// 当前占用字节。
    pub used_bytes: usize,
    /// 当前 tile 数。
    pub tiles: usize,
}

/// Tile 缓存：按字节预算做 LRU 淘汰，并支持视口淘汰（13.3）。
#[derive(Debug)]
pub struct TileCache {
    grid: TileGrid,
    tiles: HashMap<TileKey, Tile>,
    order: VecDeque<TileKey>,
    budget_bytes: usize,
    used_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl TileCache {
    /// 以字节预算构造（预算至少容纳一个 tile）。
    pub fn new(grid: TileGrid, budget_bytes: usize) -> Self {
        let one_tile = (grid.tile_size() * grid.tile_size() * 4 * 2) as usize;
        Self {
            grid,
            tiles: HashMap::new(),
            order: VecDeque::new(),
            budget_bytes: budget_bytes.max(one_tile),
            used_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// 网格。
    pub const fn grid(&self) -> &TileGrid {
        &self.grid
    }

    /// 当前 tile 数。
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// 字节预算。
    pub const fn budget_bytes(&self) -> usize {
        self.budget_bytes
    }

    /// 统计。
    pub fn stats(&self) -> TileCacheStats {
        TileCacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            used_bytes: self.used_bytes,
            tiles: self.tiles.len(),
        }
    }

    /// 只读查询（不计入 LRU 顺序，命中计数 +1）。
    pub fn peek(&mut self, key: TileKey) -> Option<&Tile> {
        if self.tiles.contains_key(&key) {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        self.tiles.get(&key)
    }

    /// 查询并刷新 LRU 位置。
    pub fn get(&mut self, key: TileKey) -> Option<&Tile> {
        if self.tiles.contains_key(&key) {
            self.hits += 1;
            self.touch(key);
        } else {
            self.misses += 1;
        }
        self.tiles.get(&key)
    }

    /// 可写查询并刷新 LRU 位置。
    pub fn get_mut(&mut self, key: TileKey) -> Option<&mut Tile> {
        if self.tiles.contains_key(&key) {
            self.hits += 1;
            self.touch(key);
        } else {
            self.misses += 1;
        }
        self.tiles.get_mut(&key)
    }

    /// 插入或替换 tile，并按需淘汰 LRU。
    pub fn insert(&mut self, tile: Tile) {
        let key = tile.key();
        let bytes = tile.byte_len();
        if let Some(previous) = self.tiles.insert(key, tile) {
            self.used_bytes = self.used_bytes.saturating_sub(previous.byte_len());
        }
        self.used_bytes += bytes;
        self.touch(key);
        self.evict_to_fit();
    }

    /// 取出 tile。
    pub fn remove(&mut self, key: TileKey) -> Option<Tile> {
        let removed = self.tiles.remove(&key);
        if let Some(tile) = &removed {
            self.used_bytes = self.used_bytes.saturating_sub(tile.byte_len());
        }
        self.order.retain(|candidate| *candidate != key);
        removed
    }

    /// 失效若干 tile，返回实际移除数量。
    pub fn invalidate(&mut self, keys: &[TileKey]) -> usize {
        let mut removed = 0;
        for key in keys {
            if self.remove(*key).is_some() {
                removed += 1;
            }
        }
        removed
    }

    /// 视口淘汰：丢掉与视口不相交的 tile（13.3 `evict_outside_viewport`）。
    pub fn evict_outside_viewport(&mut self, viewport: &Bbox) -> usize {
        let keep: std::collections::HashSet<TileKey> =
            self.grid.keys_for_bbox(viewport).into_iter().collect();
        let doomed: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|key| !keep.contains(key))
            .copied()
            .collect();
        self.invalidate(&doomed)
    }

    /// 清空缓存。
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.order.clear();
        self.used_bytes = 0;
    }

    /// 已缓存 tile 的键（升序）。
    pub fn keys(&self) -> Vec<TileKey> {
        let mut keys: Vec<TileKey> = self.tiles.keys().copied().collect();
        keys.sort();
        keys
    }

    fn touch(&mut self, key: TileKey) {
        self.order.retain(|candidate| *candidate != key);
        self.order.push_back(key);
    }

    fn evict_to_fit(&mut self) {
        while self.used_bytes > self.budget_bytes && !self.order.is_empty() {
            let Some(victim) = self.order.pop_front() else {
                break;
            };
            if let Some(tile) = self.tiles.remove(&victim) {
                self.used_bytes = self.used_bytes.saturating_sub(tile.byte_len());
                self.evictions += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> TileGrid {
        TileGrid::new(32, 100, 70).unwrap()
    }

    #[test]
    fn grid_geometry_is_consistent() {
        let grid = grid();
        assert_eq!(grid.tiles_x(), 4);
        assert_eq!(grid.tiles_y(), 3);
        assert_eq!(grid.tile_count(), 12);
        assert_eq!(grid.key_of(0, 0), TileKey::new(0, 0));
        assert_eq!(grid.key_of(99, 69), TileKey::new(3, 2));
        assert_eq!(grid.key_at_point(100.0, 0.0), None);
        assert_eq!(grid.key_at_point(0.0, 70.0), None);
        assert!(grid.contains(TileKey::new(3, 2)));
        assert!(!grid.contains(TileKey::new(4, 2)));

        // 边界裁剪：最后一个 tile 不足整块。
        let last = grid.bounds(TileKey::new(3, 2));
        assert_eq!(last, Bbox::new(96.0, 64.0, 4.0, 6.0));
    }

    #[test]
    fn keys_for_bbox_covers_intersections_only() {
        let grid = grid();
        assert_eq!(
            grid.keys_for_bbox(&Bbox::new(0.0, 0.0, 1.0, 1.0)),
            vec![TileKey::new(0, 0)]
        );
        assert_eq!(
            grid.keys_for_bbox(&Bbox::new(31.0, 31.0, 2.0, 2.0)),
            vec![
                TileKey::new(0, 0),
                TileKey::new(1, 0),
                TileKey::new(0, 1),
                TileKey::new(1, 1)
            ]
        );
        assert!(grid
            .keys_for_bbox(&Bbox::new(-50.0, -50.0, 10.0, 10.0))
            .is_empty());
        assert!(grid
            .keys_for_bbox(&Bbox::new(0.0, 0.0, 0.0, 10.0))
            .is_empty());
        assert_eq!(
            grid.keys_for_bbox(&Bbox::new(0.0, 0.0, 1000.0, 1000.0))
                .len(),
            12
        );
    }

    #[test]
    fn tile_pixels_round_trip_through_f16() {
        let mut tile = Tile::new(TileKey::new(0, 0), 4);
        assert!(tile.is_transparent());
        tile.set(1, 1, [0.25, 0.5, 0.75, 1.0]);
        let pixel = tile.get(1, 1);
        assert!((pixel[0] - 0.25).abs() < 1e-4);
        assert!((pixel[3] - 1.0).abs() < 1e-6);
        assert!(!tile.is_transparent());
        // 越界读写是安全的空操作。
        tile.set(9, 9, [1.0; 4]);
        assert_eq!(tile.get(9, 9), [0.0; 4]);
    }

    #[test]
    fn tile_blend_and_export() {
        let mut tile = Tile::new(TileKey::new(0, 0), 2);
        tile.fill([0.0, 0.0, 0.0, 1.0]);
        tile.blend_over(0, 0, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(tile.get(0, 0)[0], 1.0);
        assert_eq!(tile.get(1, 1)[0], 0.0);

        let rgba = tile.to_rgba8(None);
        assert_eq!(rgba.len(), 2 * 2 * 4);
        assert_eq!(&rgba[0..4], &[255, 255, 255, 255]);
        assert_eq!(&rgba[4..8], &[0, 0, 0, 255]);
    }

    #[test]
    fn cache_evicts_lru_within_budget() {
        let grid = grid();
        let tile_bytes = 32 * 32 * 4 * 2;
        let mut cache = TileCache::new(grid.clone(), tile_bytes * 2);
        for key in [TileKey::new(0, 0), TileKey::new(1, 0)] {
            cache.insert(Tile::new(key, 32));
        }
        // 访问 (0,0) 使其成为最近使用。
        assert!(cache.get(TileKey::new(0, 0)).is_some());
        cache.insert(Tile::new(TileKey::new(2, 0), 32));
        assert_eq!(cache.len(), 2);
        assert!(cache.peek(TileKey::new(0, 0)).is_some(), "最近使用者保留");
        assert!(
            cache.peek(TileKey::new(1, 0)).is_none(),
            "最久未使用者被淘汰"
        );
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
        assert!(stats.used_bytes <= cache.budget_bytes());
    }

    #[test]
    fn cache_invalidate_and_viewport_eviction() {
        let grid = grid();
        let mut cache = TileCache::new(grid.clone(), 32 * 32 * 4 * 2 * 20);
        for key in grid.all_keys() {
            cache.insert(Tile::new(key, 32));
        }
        assert_eq!(cache.len(), 12);
        assert_eq!(
            cache.evict_outside_viewport(&Bbox::new(0.0, 0.0, 32.0, 32.0)),
            11
        );
        assert_eq!(cache.keys(), vec![TileKey::new(0, 0)]);
        assert_eq!(cache.invalidate(&[TileKey::new(0, 0)]), 1);
        assert!(cache.is_empty());
        assert_eq!(cache.stats().used_bytes, 0);
    }

    #[test]
    fn blit_rgba8_clips_to_tile() {
        let mut tile = Tile::new(TileKey::new(0, 0), 4);
        let patch = vec![255u8; 2 * 2 * 4];
        tile.blit_rgba8(-1, -1, 2, 2, &patch);
        assert_eq!(tile.get(0, 0)[3], 1.0);
        assert!(!tile.is_transparent());
        let mut tile = Tile::new(TileKey::new(0, 0), 2);
        tile.blit_rgba8(1, 1, 4, 4, &patch);
        assert_eq!(tile.get(1, 1)[3], 1.0);
        assert_eq!(tile.get(0, 0)[3], 0.0);
    }
}
