//! **位图分块（(b1) 的数据层）** ✓（第 162 轮 ✓）—— 目标第 1 条"懒解码"的**最小地基** ✓。
//!
//! **为什么需要它** ✗（实测为据 ✓）：8K 一份位图是**整体式 deflate 流** ✓ ⇒ **取任意一段必须先解整条** ✗
//! ⇒ **∴ 为渲 64²（4096 像素）要解 265 MB ＝ 16200× 区域像素** ✗✓（**判据① 实测 ✓**）。
//! ⇒ **∴ 把位图**存成 256² 的块** ✓ ⇒ **64² 请求只碰 1 块 ＝ 256 KB ⇒ 530× 更少** ✓✓。
//!
//! **为什么 tile ＝ 256²** ✓（推导见实现笔记第 453 轮 ✓）：
//! * **与 `TileGrid::DEFAULT_TILE_SIZE` 一致** ✓ ⇒ 渲染 tile 与位图 tile **同一尺寸** ✓；
//! * **8K ⇒ 30×17 ＝ 510 块** ✓ ＝ **既有渲染缓存的口径** ✓ ⇒ 内存账一致 ✓。
//!
//! **本模块只做数据层** ✓（**网格／索引／区域→块映射** ✓）；**切像素与接线在后续轮次** ✓
//! —— **∴ 这样它能被**纯单元测试**守住 ✓，且**不碰渲染路径** ⇒ 风险低 ✓**。

use serde::{Deserialize, Serialize};

/// **位图 tile 边长** ✓ —— **与 `crate::tile::DEFAULT_TILE_SIZE` 对齐** ✓（256 ✓）。
pub const BITMAP_TILE: u32 = 256;

/// **索引格式版本** ✓ —— 旧读方见**不认识的版本** ⇒ **回退原路（整幅 ✓）** ✓。
pub const BITMAP_INDEX_VERSION: u32 = 1;

/// **位图分块索引** ✓：宽／高／tile 边长 ＋ **行优先**的块哈希表 ✓。
///
/// **兼容性** ✓：它作为**新的 JSON 字段**放进对象净荷 ✓ ⇒ **旧读方忽略未知字段** ✓
/// ⇒ **∴ 不需要迁移** ✓（实现笔记第 451 轮 ✓）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BitmapIndex {
    /// **格式版本** ✓（未来演进用 ✓）。
    pub v: u32,
    /// tile 边长（像素 ✓）。
    pub tile: u32,
    /// 位图宽（像素 ✓）。
    pub width: u32,
    /// 位图高（像素 ✓）。
    pub height: u32,
    /// **行优先**的块哈希（长度 ＝ `tiles_x * tiles_y` ✓）。
    pub tiles: Vec<String>,
}

impl BitmapIndex {
    /// **块网格**（`tiles_x`, `tiles_y` ✓）—— 向上取整 ✓（**边缘块可以不满** ✓）。
    pub fn grid(width: u32, height: u32) -> (u32, u32) {
        (width.div_ceil(BITMAP_TILE), height.div_ceil(BITMAP_TILE))
    }

    /// 该尺寸下的**块数** ✓。
    pub fn count(width: u32, height: u32) -> usize {
        let (tx, ty) = Self::grid(width, height);
        (tx as usize).saturating_mul(ty as usize)
    }

    /// **该索引的块数必须与声明尺寸吻合** ✓（**判据用 ✓**：撒谎会被抓住 ✗）。
    pub fn is_consistent(&self) -> bool {
        self.v == BITMAP_INDEX_VERSION
            && self.tile == BITMAP_TILE
            && self.width > 0
            && self.height > 0
            && self.tiles.len() == Self::count(self.width, self.height)
    }

    /// **某个像素区域覆盖的块索引** ✓（**行优先 ✓，升序 ✓，去重 ✓**）—— **本模块的收益核心** ✓。
    ///
    /// **判据** ✓：**64² 的区域只应得到 **1** 个块** ✓ ⇒ **∴ 只会解码 256 KB，而不是 132.7 MB** ✓✓
    /// （**变异** ✗：让它返回"全部块" ⇒ 判据必红 ✓）。
    ///
    /// 越界／空区域的语义 ✓：**先与 [0,w)×[0,h) 求交** ✓ ⇒ **交集为空 ⇒ 返回空表** ✓
    /// （**不撒谎** ✓：没覆盖任何块就说没有 ✓，而不是"给全部" ✗）。
    pub fn tiles_for_rect(&self, x: i64, y: i64, w: u32, h: u32) -> Vec<usize> {
        if w == 0 || h == 0 {
            return Vec::new();
        }
        let (tx, ty) = Self::grid(self.width, self.height);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + i64::from(w)).min(i64::from(self.width));
        let y1 = (y + i64::from(h)).min(i64::from(self.height));
        if x0 >= x1 || y0 >= y1 {
            return Vec::new();
        }
        let t = i64::from(BITMAP_TILE);
        let cx0 = (x0 / t) as u32;
        let cy0 = (y0 / t) as u32;
        let cx1 = (((x1 - 1) / t) as u32).min(tx.saturating_sub(1));
        let cy1 = (((y1 - 1) / t) as u32).min(ty.saturating_sub(1));
        let mut out = Vec::new();
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                out.push((cy as usize) * (tx as usize) + (cx as usize));
            }
        }
        out
    }
}

/// **按区域从块拼出 RGBA8** ✓（第 164 轮 ✓）—— **(b2) 的数据侧另一半 ✓**（**纯函数 ✓，可判据 ✓**）。
///
/// **输入** ✓：位图宽高 ✓、**行优先的块像素表**（每块 `256×256×4` ✓，**边缘块按实际尺寸** ✓）、目标矩形 ✓；
/// **输出** ✓：该矩形的 RGBA8 ✓（**必须逐字节等于整幅图上的同一区域 ✓**）。
///
/// **`None` 的语义** ✓：**块表长度与尺寸不吻合** ⇒ **宁可 `None`（调用方回退整幅 ✓），也不给半个图 ✗**。
pub fn assemble_region(
    width: u32,
    height: u32,
    tiles: &[Vec<u8>],
    x: i64,
    y: i64,
    w: u32,
    h: u32,
) -> Option<Vec<u8>> {
    let index = BitmapIndex {
        v: BITMAP_INDEX_VERSION,
        tile: BITMAP_TILE,
        width,
        height,
        tiles: vec![String::new(); tiles.len()],
    };
    if !index.is_consistent() {
        return None;
    }
    if w == 0 || h == 0 {
        return Some(Vec::new());
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + i64::from(w)).min(i64::from(width));
    let y1 = (y + i64::from(h)).min(i64::from(height));
    if x0 >= x1 || y0 >= y1 {
        return Some(Vec::new());
    }
    let out_w = (x1 - x0) as u32;
    let out_h = (y1 - y0) as u32;
    let mut out = vec![0u8; (out_w as usize) * (out_h as usize) * 4];
    let t = i64::from(BITMAP_TILE);
    let (tx, _ty) = BitmapIndex::grid(width, height);
    for row in 0..out_h {
        let sy = y0 + i64::from(row);
        let tile_cy = (sy / t) as u32;
        let in_tile_y = (sy % t) as u32;
        for col in 0..out_w {
            let sx = x0 + i64::from(col);
            let tile_cx = (sx / t) as u32;
            let in_tile_x = (sx % t) as u32;
            let ti = (tile_cy as usize) * (tx as usize) + (tile_cx as usize);
            let tile = tiles.get(ti)?;
            let tile_w = (width - tile_cx * BITMAP_TILE).min(BITMAP_TILE);
            let src = ((in_tile_y * tile_w + in_tile_x) as usize) * 4;
            let dst = ((row as usize) * (out_w as usize) + (col as usize)) * 4;
            if src + 4 > tile.len() || dst + 4 > out.len() {
                return None;
            }
            out[dst..dst + 4].copy_from_slice(&tile[src..src + 4]);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(width: u32, height: u32) -> BitmapIndex {
        BitmapIndex {
            v: BITMAP_INDEX_VERSION,
            tile: BITMAP_TILE,
            width,
            height,
            tiles: (0..BitmapIndex::count(width, height))
                .map(|i| format!("sha256:{i:064x}"))
                .collect(),
        }
    }

    /// **判据 1** ✓：**8K 的块数 ＝ 510** ✓（与既有渲染缓存的 510 tile 口径一致 ✓）。
    #[test]
    fn eight_k_yields_five_hundred_ten_tiles() {
        assert_eq!(BitmapIndex::grid(7680, 4320), (30, 17));
        assert_eq!(BitmapIndex::count(7680, 4320), 510);
    }

    /// **判据 2（收益核心 ✓）**：**64² 的区域只应得到 1 个块** ✓。
    #[test]
    fn a_small_rect_touches_one_tile() {
        let idx = index(7680, 4320);
        assert_eq!(idx.tiles_for_rect(600, 600, 64, 64).len(), 1);
        // 跨块边界时变多 ✓（**不撒谎** ✓）：
        assert!(idx.tiles_for_rect(250, 250, 64, 64).len() >= 2);
    }

    /// **判据 3** ✓：**整幅 ⇒ 全部块** ✓（**上界自检** ✓）。
    #[test]
    fn the_whole_frame_touches_every_tile() {
        let idx = index(7680, 4320);
        assert_eq!(idx.tiles_for_rect(0, 0, 7680, 4320).len(), 510);
    }

    /// **判据 4** ✓：**越界／空区域 ⇒ 空表** ✓（**没覆盖就说没有 ✓，不给全部 ✗**）。
    #[test]
    fn out_of_bounds_and_empty_rects_touch_nothing() {
        let idx = index(7680, 4320);
        assert!(idx.tiles_for_rect(7680, 0, 64, 64).is_empty());
        assert!(idx.tiles_for_rect(0, 0, 0, 64).is_empty());
        assert!(idx.tiles_for_rect(-100, -100, 64, 64).is_empty());
    }

    /// **判据 5** ✓：**索引自洽性检查** ✓ ⇒ **块表长度与尺寸不吻合 ⇒ 判为不一致** ✓（**防撒谎 ✓**）。
    #[test]
    fn an_index_whose_tile_list_does_not_match_its_size_is_inconsistent() {
        let mut idx = index(7680, 4320);
        assert!(idx.is_consistent());
        idx.tiles.pop();
        assert!(!idx.is_consistent());
    }
    /// **判据 6（等价核心 ✓）**：**由块拼出的区域必须与整幅图上的同一区域逐字节一致** ✓✓
    /// （**变异** ✗：把 x/y 弄错 ⇒ 必红 ✓）。
    #[test]
    fn assembling_a_region_matches_the_whole_image() {
        let (w, h) = (600u32, 520u32);
        let whole: Vec<u8> = (0..(w as usize) * (h as usize))
            .flat_map(|i| {
                let px = (i / w as usize) as u32;
                let py = (i % w as usize) as u32;
                [
                    ((px * 7) % 251) as u8,
                    ((py * 13) % 253) as u8,
                    ((px + py) % 255) as u8,
                    255,
                ]
            })
            .collect();
        let (tx, ty) = BitmapIndex::grid(w, h);
        let mut tiles: Vec<Vec<u8>> = Vec::new();
        for cy in 0..ty {
            for cx in 0..tx {
                let tw = (w - cx * BITMAP_TILE).min(BITMAP_TILE);
                let th = (h - cy * BITMAP_TILE).min(BITMAP_TILE);
                let mut t = Vec::with_capacity((tw * th * 4) as usize);
                for r in 0..th {
                    let sy = cy * BITMAP_TILE + r;
                    let start = ((sy as usize) * (w as usize) + (cx * BITMAP_TILE) as usize) * 4;
                    t.extend_from_slice(&whole[start..start + (tw as usize) * 4]);
                }
                tiles.push(t);
            }
        }
        let (x, y, rw, rh) = (250i64, 200i64, 300u32, 260u32);
        let got = assemble_region(w, h, &tiles, x, y, rw, rh).expect("应能拼出");
        let mut want = Vec::new();
        for r in 0..rh {
            let start = (((y as u32 + r) as usize) * (w as usize) + x as usize) * 4;
            want.extend_from_slice(&whole[start..start + (rw as usize) * 4]);
        }
        assert_eq!(got, want, "**由块拼出的区域必须与整幅逐字节一致**");
        let mut short = tiles.clone();
        short.pop();
        assert!(assemble_region(w, h, &short, x, y, rw, rh).is_none());
    }
}
