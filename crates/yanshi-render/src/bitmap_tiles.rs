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

/// **★★★ `BITMAP_TILE` 必须是 **2 的幂**，且移位常量必须与它一致 ✗ ★★★**（**第 740 轮 ✓）
///
/// **∴ 为什么需要它 ✗**（**第 739 轮审查外部补丁时发现的唯一风险 ✓）★**：
///   **∴ 事实 ✗**：`assemble_region` **为提速把**除法／取模**换成**
///     `>> tile_shift` ＋ `& tile_mask`**（**`tile_shift = 8`／`tile_mask = 255` ✓）★
///     ＋ **∴ 而那**只在 `BITMAP_TILE == 256` **时**正确** ✓
///       ＋ **∴ 若**将来有人把常量改成 512**✗ ⇒ **∴ 移位**静默算错**✗
///         ＋ **∴ 而**它**不会**崩溃**✗ 只**会**画出**错位的图** ✗ ★
///           ＋ **∴ 所以**：**必须在**编译期**把它钉住** ✓ ★
/// **∴ 为什么用编译期断言（**而不是运行时判据 ✓）✗**：
///   **∴ ① 零运行时成本**✗ ／ **∴ ② 不可能**被跳过或忘跑** ✗
///     ＋ **∴ ③ 改错常量的那次 `cargo build` **直接失败**✗ ⇒ **∴ 拦在**最早的点** ✓ ★
const _: () = {
    assert!(
        BITMAP_TILE == 256,
        "BITMAP_TILE 必须是 256：assemble_region 用 >> 8 与 & 255 代替除法／取模"
    );
    assert!(
        BITMAP_TILE.is_power_of_two(),
        "BITMAP_TILE 必须是 2 的幂，否则移位等价性不成立"
    );
    // **∴ 移位／掩码与常量**逐位一致** ✗**（**∴ 三处必须同时改 ✓）★
    assert!(BITMAP_TILE.trailing_zeros() == 8, "移位常量必须是 8");
    assert!(BITMAP_TILE - 1 == 255, "掩码常量必须是 255");
};

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
    let (tx, _ty) = BitmapIndex::grid(width, height);
    let tile_shift = 8u32;
    let tile_mask = 255u32;
    for row in 0..out_h {
        let sy = (y0 as u32) + row;
        let tile_cy = sy >> tile_shift;
        let in_tile_y = sy & tile_mask;
        let mut col = 0u32;
        while col < out_w {
            let sx = (x0 as u32) + col;
            let tile_cx = sx >> tile_shift;
            let in_tile_x = sx & tile_mask;
            let ti = (tile_cy as usize) * (tx as usize) + (tile_cx as usize);
            let tile = tiles.get(ti)?;
            let tile_w = (width - (tile_cx << tile_shift)).min(BITMAP_TILE);
            let run = (BITMAP_TILE - in_tile_x).min(out_w - col);
            let src = ((in_tile_y * tile_w + in_tile_x) as usize) * 4;
            let dst = ((row as usize) * (out_w as usize) + (col as usize)) * 4;
            let len = (run as usize) * 4;
            if src + len > tile.len() || dst + len > out.len() {
                return None;
            }
            out[dst..dst + len].copy_from_slice(&tile[src..src + len]);
            col += run;
        }
    }
    Some(out)
}

/// **把整幅 RGBA8 切成块** ✓（第 165 轮 ✓）—— **(b1) 导入侧的数据核心 ✓**（**纯函数 ✓**）。
///
/// **块序** ✓：**行优先**（与 [`BitmapIndex::tiles`] 一致 ✓）；**边缘块按实际宽高** ✓（**不补白边 ✗**）。
///
/// **`None` 的语义** ✓：**输入字节数与 `width×height×4` 不符 ⇒ `None`** ✓
/// （**∴ 宁可失败，也不切出一堆错位的块 ✗**）。
pub fn split_into_tiles(width: u32, height: u32, rgba8: &[u8]) -> Option<Vec<Vec<u8>>> {
    if width == 0 || height == 0 {
        return None;
    }
    if rgba8.len() != (width as usize) * (height as usize) * 4 {
        return None;
    }
    let (tx, ty) = BitmapIndex::grid(width, height);
    let row_bytes = (width as usize) * 4;
    let mut out = Vec::with_capacity((tx as usize) * (ty as usize));
    for cy in 0..ty {
        for cx in 0..tx {
            let tile_w = (width - cx * BITMAP_TILE).min(BITMAP_TILE);
            let tile_h = (height - cy * BITMAP_TILE).min(BITMAP_TILE);
            let mut tile = Vec::with_capacity((tile_w * tile_h * 4) as usize);
            for r in 0..tile_h {
                let sy = (cy * BITMAP_TILE + r) as usize;
                let start = sy * row_bytes + (cx * BITMAP_TILE) as usize * 4;
                tile.extend_from_slice(&rgba8[start..start + (tile_w as usize) * 4]);
            }
            out.push(tile);
        }
    }
    Some(out)
}

/// **稀疏装配** ✓（第 184 轮 ✓）：只给"**实际取到的块**" ⇒ **这是省解码的关键** ✓。
///
/// **输入**：`tile_indices[k]` ＝ `tiles[k]` 在索引里的块号（行优先）；其余同 [`assemble_region`]。
/// **语义（与稠密版逐字节相同）**：用到的块缺失 ⇒ `None`；没用到的块缺失 ⇒ 照常出图。
///
/// **代价**：内部按块数建一张稠密表（未取到的块为空）⇒ 大区域下有一次克隆；
/// 对 64²（只碰 1 块）无影响，大区域走原路整幅，阈值判别归调用方。
pub fn assemble_region_sparse(
    width: u32,
    height: u32,
    tile_indices: &[usize],
    tiles: &[Vec<u8>],
    rect: (i64, i64, u32, u32),
) -> Option<Vec<u8>> {
    let (x, y, w, h) = rect;
    if tile_indices.len() != tiles.len() {
        return None;
    }
    let mut dense: Vec<Vec<u8>> = vec![Vec::new(); BitmapIndex::count(width, height)];
    for (slot, source) in tile_indices.iter().zip(tiles.iter()) {
        let entry = dense.get_mut(*slot)?;
        *entry = source.clone();
    }
    assemble_region(width, height, &dense, x, y, w, h)
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

    /// **判据（CPU 线 ✓）：`raster_patch` 的每对象固定开销**分在哪四段** ✓**
    ///
    /// **∴ 为什么必须有它 ✗**：附二十五 把"**每对象 ~4.5 ms 固定开销**"定为优化机会 ✓
    ///   ⇒ **∴ 但**不知道钱花在哪一段**就**不能改**✓（**附十五 那次就是"改了但没收益"✓**）★
    ///
    /// **∴ 量四段 ✗**（**与 `render.rs` 里新加的 5 段探针同名同义 ✓**）：
    ///   **① 取＋解析索引 JSON** ② 逐块取 ③ `assemble_region_sparse` ④ `blit_rgba8` ✓
    ///
    /// **∴ 断言两件 ✗ ★**：
    ///   1. **四段各自 > 0** ✓ —— **∴ 防的是"循环被优化掉"** ✗（**与 `rows.rs` 的 `split_probe` 同一条 ✓**：
    ///      **∴ 不**断言具体占比 ✗ —— **∴ 那会因机器而抖 ✓**）；
    ///   2. **四段之和 ≈ 整段（±10% ✓）** —— **∴ 这条才有意义 ✗**：**它证明"没有一大块没被计到"** ✓
    ///      （**∴ 即**：**外层的总时间**确实**由这四段构成 ✓**）★
    ///
    /// **∴ 变异 ✗**：**把任一段清零 ⇒ **第 2 条必红** ✓（**∴ 本判据**有牙**✓**）。
    ///
    /// **∴ 口径（**如实 ✓）★**：**这是**合成夹具**上的**段间分布**✗ ⇒ **∴ 不是**真实 4K 画作的绝对数 ✓
    ///   （**∴ 那要另跑真语料 ✓ —— 附八／附九 ✓**）；**∴ 而**分布**与**规模**大体无关 ✓（**∴ 逐块/逐行 ✓**）。
    #[test]
    fn raster_patch_fixed_cost_splits_into_four_segments() {
        use crate::buffer::Buffer;
        use std::time::{Duration, Instant};
        use yanshi_core::blob::{BlobStore, MemoryBlobStore};

        // **∴ 规模 ✗**：1024² 画布 ＋ 256² 的补丁 ✓（**∴ 与 4K 同一形状的固定活 ✓，而测试很快 ✓**）
        let (doc_w, doc_h) = (1024u32, 1024u32);
        let (patch_w, patch_h) = (256u32, 256u32);
        let store = MemoryBlobStore::default();

        // **∴ 真实的块字节 ✗**（**每块 64²×4 ＝ 16 KB ✓**）＋ **索引存进 store ✓**（**与渲染时同一形态 ✓**）
        let tile_len = (BITMAP_TILE * BITMAP_TILE * 4) as usize;
        let tile_bytes = vec![0x7fu8; tile_len];
        let count = BitmapIndex::count(doc_w, doc_h);
        let mut hashes = Vec::with_capacity(count);
        for _ in 0..count {
            hashes.push(
                store
                    .put(&tile_bytes)
                    .expect("内存 store 不会写失败")
                    .to_string(),
            );
        }
        let index = BitmapIndex {
            v: BITMAP_INDEX_VERSION,
            tile: BITMAP_TILE,
            width: doc_w,
            height: doc_h,
            tiles: hashes,
        };
        let index_json = serde_json::to_vec(&index).expect("索引可序列化");
        let index_hash = store.put(&index_json).expect("内存 store 不会写失败");
        let rect = (0i64, 0i64, patch_w, patch_h);

        // **∴ 整段 ✗**：**外层一个计时包住四段全部 ✓**（**∴ 用来做第 2 条断言的**分母**✓**）
        let whole_run = || -> Duration {
            let t = Instant::now();
            let raw = store.get(&index_hash).expect("索引可取");
            let parsed: BitmapIndex = serde_json::from_slice(&raw).expect("索引可解析");
            let slots = parsed.tiles_for_rect(rect.0, rect.1, rect.2, rect.3);
            let mut parts = Vec::with_capacity(slots.len());
            for slot in &slots {
                let text = parsed.tiles.get(*slot).expect("块号在表内");
                let th = text
                    .parse::<yanshi_core::atom::BlobHash>()
                    .expect("块哈希可解析");
                parts.push(store.get(&th).expect("块可取"));
            }
            let stitched =
                assemble_region_sparse(doc_w, doc_h, &slots, &parts, rect).expect("拼接应当成功");
            let mut buffer = Buffer::new(0, 0, doc_w, doc_h);
            buffer.blit_rgba8(0, 0, patch_w, patch_h, &stitched, 1.0);
            t.elapsed()
        };

        // **∴ 分段 ✗**：**同一段活，四段各自计时 ✓**（**∴ 顺序与整段完全一致 ✓**）
        let segmented_run = || -> [Duration; 4] {
            let mut d = [Duration::ZERO; 4];
            let t0 = Instant::now();
            let raw = store.get(&index_hash).expect("索引可取");
            let parsed: BitmapIndex = serde_json::from_slice(&raw).expect("索引可解析");
            d[0] = t0.elapsed();

            let t1 = Instant::now();
            let slots = parsed.tiles_for_rect(rect.0, rect.1, rect.2, rect.3);
            let mut parts = Vec::with_capacity(slots.len());
            for slot in &slots {
                let text = parsed.tiles.get(*slot).expect("块号在表内");
                let th = text
                    .parse::<yanshi_core::atom::BlobHash>()
                    .expect("块哈希可解析");
                parts.push(store.get(&th).expect("块可取"));
            }
            d[1] = t1.elapsed();

            let t2 = Instant::now();
            let stitched =
                assemble_region_sparse(doc_w, doc_h, &slots, &parts, rect).expect("拼接应当成功");
            d[2] = t2.elapsed();

            let t3 = Instant::now();
            let mut buffer = Buffer::new(0, 0, doc_w, doc_h);
            buffer.blit_rgba8(0, 0, patch_w, patch_h, &stitched, 1.0);
            d[3] = t3.elapsed();
            d
        };

        // **∴ 3 轮取**最小**✗**（**∴ 守"单次测量不算数"✓ —— 与附二十／二十一 同一条纪律 ✓**）
        let mut whole = Duration::MAX;
        let mut seg = [Duration::MAX; 4];
        for round in 0..3 {
            let w = whole_run();
            let s = segmented_run();
            if round == 0 {
                // **∴ 第一轮是**热身**✗** ⇒ **∴ 丢掉 ✓**（**与既有工具一致 ✓**）
                continue;
            }
            whole = whole.min(w);
            for i in 0..4 {
                seg[i] = seg[i].min(s[i]);
            }
        }
        let sum: Duration = seg.iter().sum();
        println!(
            "RASTER_PATCH 块数={} 整段={:.3}ms ①索引={:.3}ms ②取块={:.3}ms ③拼接={:.3}ms ④blit={:.3}ms \
             ⇒ 四段合={:.3}ms（占整段 {:.1}%）",
            count,
            whole.as_secs_f64() * 1000.0,
            seg[0].as_secs_f64() * 1000.0,
            seg[1].as_secs_f64() * 1000.0,
            seg[2].as_secs_f64() * 1000.0,
            seg[3].as_secs_f64() * 1000.0,
            sum.as_secs_f64() * 1000.0,
            sum.as_secs_f64() / whole.as_secs_f64() * 100.0
        );

        // **★ 第 1 条 ✗**：**四段各自 > 0** ✓（**∴ 任何一段为 0 ⇒ **要么**那段没干**✗、**要么**测量被优化掉了 ✗**）
        for (i, name) in ["①索引 JSON", "②逐块取", "③拼接", "④blit"]
            .iter()
            .enumerate()
        {
            assert!(
                seg[i] > Duration::ZERO,
                "第 {name} 段量为 0 ⇒ **∴ 要么**这段真的没干活 ✓、**要么**循环被优化掉了 ✗ ⇒ \
                 **∴ 两者都**必须先查清 ✓**（**∴ 不许**当「它很快」放过 ✗）"
            );
        }
        // **★ 第 2 条 ✗**：**四段之和 ≈ 整段（±10% ✓）** —— **∴ 它证明**没有一大块没被计到**✓**
        let share = sum.as_secs_f64() / whole.as_secs_f64();
        assert!(
            (0.90..=1.10).contains(&share),
            "四段之和占整段的 {:.1}% ⇒ **∴ 不在 ±10% ⇒ **∴ 说明**有一段没被计到 ✗ ⇒ \
             **∴ 先找它 ✗，不要**先优化 ✓",
            share * 100.0
        );
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
    /// **判据 7（往返恒等 ✓，最强的一条 ✓）**：**切块后按整幅拼回 ⇒ 必须与原图逐字节一致** ✓✓
    /// （**变异** ✗：丢掉每块最后一行 ⇒ 必红 ✓）。
    #[test]
    fn splitting_then_assembling_round_trips_exactly() {
        let (w, h) = (600u32, 520u32); // **含边缘块** ✓
        let whole: Vec<u8> = (0..(w as usize) * (h as usize))
            .flat_map(|i| {
                let px = (i / w as usize) as u32;
                let py = (i % w as usize) as u32;
                [
                    ((px * 3) % 250) as u8,
                    ((py * 5) % 252) as u8,
                    ((px ^ py) % 255) as u8,
                    255,
                ]
            })
            .collect();
        let tiles = split_into_tiles(w, h, &whole).expect("应能切块");
        assert_eq!(tiles.len(), BitmapIndex::count(w, h));
        let back = assemble_region(w, h, &tiles, 0, 0, w, h).expect("应能拼回");
        assert_eq!(back, whole, "**切块后拼回必须与原图逐字节一致**");
        // **字节数不符 ⇒ `None`** ✓（**不切出错位的块 ✗**）
        assert!(split_into_tiles(w, h, &whole[..whole.len() - 4]).is_none());
    }
    /// **判据 8**：稀疏装配（只给它用到的块）必须与稠密装配逐字节相同。
    #[test]
    fn sparse_assembly_matches_dense_for_the_tiles_it_was_given() {
        let (w, h) = (600u32, 520u32);
        let whole: Vec<u8> = (0..(w as usize) * (h as usize))
            .flat_map(|i| {
                let px = (i / w as usize) as u32;
                let py = (i % w as usize) as u32;
                [
                    ((px * 11) % 249) as u8,
                    ((py * 17) % 247) as u8,
                    ((px * py) % 255) as u8,
                    255,
                ]
            })
            .collect();
        let dense = split_into_tiles(w, h, &whole).expect("应能切块");
        let (x, y, rw, rh) = (250i64, 200i64, 300u32, 260u32);
        let want = assemble_region(w, h, &dense, x, y, rw, rh).expect("稠密应能拼出");
        let idx = BitmapIndex {
            v: BITMAP_INDEX_VERSION,
            tile: BITMAP_TILE,
            width: w,
            height: h,
            tiles: vec![String::new(); dense.len()],
        };
        let needed = idx.tiles_for_rect(x, y, rw, rh);
        assert!(needed.len() < dense.len());
        let sparse: Vec<Vec<u8>> = needed.iter().map(|t| dense[*t].clone()).collect();
        let got =
            assemble_region_sparse(w, h, &needed, &sparse, (x, y, rw, rh)).expect("稀疏应能拼出");
        assert_eq!(got, want, "稀疏装配必须与稠密装配逐字节相同");
        let mut short = sparse.clone();
        short.pop();
        assert!(
            assemble_region_sparse(w, h, &needed[..needed.len() - 1], &short, (x, y, rw, rh))
                .is_none()
        );
        assert!(assemble_region_sparse(w, h, &needed, &short, (x, y, rw, rh)).is_none());
    }
}
