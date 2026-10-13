//! **★ timelapse 的**增量重放驱动**✗ ★**（**第 249 轮 ✓）
//!
//! **∴ 为什么要有这个 lib ✗**（**而**不是把逻辑写在 `main.rs` 里 ✓）★**：
//!   ＋ **∴ 判据要能**直接调它**✗** ⇒ **∴ 二进制里的函数**测试碰不到** ✓
//!     ＋ **∴ 而**"增量帧必须等于整幅重渲"**是这次优化的**命门**✗
//!       ⇒ **∴ 必须有**能红的测试**把它钉住** ✓ ★★
//!   ＋ **∴ 而**浏览器那侧（PWA／web）将来要用**同一套逻辑**✗ ⇒ **∴ 放 lib 里**
//!     才好被两边共用** ✓ ★
//!
//! **∴ 增量怎么算 ✗**：**内核每吃一个 atom 都会报**它脏了哪些 tile**（`last_dirty_tiles` ✓）
//!   ＋ **∴ 我在**两次出帧之间**把它们**并起来**✗（**∴ 不假设内核会替我累积 ✓）
//!     ⇒ **∴ 出帧时**只渲染那个并集的 bbox ✗ 再 blit 进一份**持久 RGBA 缓冲** ✓
//!       ＋ **∴ 而**第一次（或没拿到脏信息时）**回退成整幅** ✓（**∴ 正确优先 ✗ 快其次 ✓）★★

use serde_json::Value;
use yanshi_core::Bbox;
use yanshi_wasm::{Kernel, KernelError};

/// **一块 tile 的坐标范围 ✗**（**闭区间 ✗ tile 坐标不是像素 ✓）★
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileRange {
    /// 最小列 ✓。
    pub x0: u32,
    /// 最小行 ✓。
    pub y0: u32,
    /// 最大列（**含 ✓）★
    pub x1: u32,
    /// 最大行（**含 ✓）★
    pub y1: u32,
}

impl TileRange {
    fn single(key: &yanshi_render::TileKey) -> Self {
        Self {
            x0: key.x,
            y0: key.y,
            x1: key.x,
            y1: key.y,
        }
    }

    fn union(self, other: Self) -> Self {
        Self {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }

    /// **换成像素 bbox ✗**（**画布尺寸用于裁剪 ✓）★
    #[must_use]
    pub fn to_bbox(self, tile: u32, doc_w: u32, doc_h: u32) -> Bbox {
        let x0 = f64::from(self.x0.saturating_mul(tile));
        let y0 = f64::from(self.y0.saturating_mul(tile));
        let x1 = f64::from(self.x1.saturating_add(1).saturating_mul(tile)).min(f64::from(doc_w));
        let y1 = f64::from(self.y1.saturating_add(1).saturating_mul(tile)).min(f64::from(doc_h));
        Bbox::new(x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
    }

    /// 覆盖的 tile 数 ✓（**观测／判据用 ✓）★
    #[must_use]
    pub fn tiles(self) -> u64 {
        u64::from(self.x1 - self.x0 + 1) * u64::from(self.y1 - self.y0 + 1)
    }
}

/// **★ 一次 timelapse 的重放状态 ✗ ★**（**内核 ＋ 持久帧缓冲 ＋ 脏区并集 ✓）★
pub struct Timeline {
    kernel: Kernel,
    tile: u32,
    doc_w: u32,
    doc_h: u32,
    /// **∴ 持久帧缓冲 ✗**（**RGBA8 ✗ 只 blit 脏区 ✓）★
    frame: Vec<u8>,
    /// **∴ 两次出帧之间累计的脏 tile 并集 ✗**（**`None` ＝ 没有信息 ⇒ 整幅 ✓）★
    dirty: Option<TileRange>,
    /// **∴ 统计：**整幅渲染了几次／脏区渲染了几次／脏区覆盖了多少 tile** ✓（**账用 ✓）★
    full_renders: u64,
    /// 见 [`Timeline::stats`]。
    dirty_renders: u64,
    /// 见 [`Timeline::stats`]。
    dirty_tiles_total: u64,
}

impl Timeline {
    /// 建一个**独立**的重放器（**不碰用户当前画布 ✓）★
    pub fn new(
        doc_id: &str,
        tile: u32,
        doc_w: u32,
        doc_h: u32,
        memory_limit: usize,
    ) -> Result<Self, KernelError> {
        let kernel = Kernel::new(doc_id, tile, doc_w, doc_h, memory_limit)?;
        Ok(Self {
            kernel,
            tile,
            doc_w,
            doc_h,
            frame: vec![0u8; (doc_w as usize) * (doc_h as usize) * 4],
            dirty: None,
            full_renders: 0,
            dirty_renders: 0,
            dirty_tiles_total: 0,
        })
    }

    /// 写一个 blob（**转发内核 ✓）★
    pub fn blob_put(&mut self, bytes: &[u8]) -> Result<String, KernelError> {
        self.kernel.blob_put(bytes)
    }

    /// **吃一个 atom ✗ 并把它的脏 tile 并进待渲集合** ✓ ★
    ///
    /// **∴ 为什么在这里并 ✗**：**内核报的是**这一个 atom**脏了哪些 tile**✗
    ///   ＋ **∴ 而**两次出帧之间可能吃了**很多**个 atom**✗ ⇒ **∴ 只取最后一个**会漏内容 ✓
    ///     ⇒ **∴ 于是**：**自己并 ✗ 不假设内核替我们累积** ✓ ★★
    pub fn apply(&mut self, atom_json: &str) -> Result<Value, KernelError> {
        let out = self.kernel.apply_atom_json(atom_json)?;
        for key in self.kernel.last_dirty_tiles() {
            // **∴ 用 `TileRange::single` ✗**（**∴ 第一版**在图里手搓了一遍 ✗ ⇒ **clippy**指出
            //   "`single` 从没被用过"**✓ ⇒ **∴ 于是**：**改成调它 ✗ 顺手去掉重复 ✓）★
            let one = TileRange::single(key);
            self.dirty = Some(match self.dirty {
                Some(prev) => prev.union(one),
                None => one,
            });
        }
        Ok(out)
    }

    /// **出一帧 ✗ 返回持久缓冲的 RGBA8（**整幅 ✓）★
    ///
    /// **∴ 逻辑 ✗**：**有脏区并集 ⇒ 只渲那一块 ＋ blit ✗** 否则**整幅渲 ＋ 覆盖** ✓
    ///   ＋ **∴ 出完帧 ⇒ **清空脏区并集**（**因为缓冲已经是最新的 ✓）★
    pub fn frame(&mut self) -> Result<&[u8], KernelError> {
        let dirty = self.dirty.take();
        match dirty {
            Some(range) => {
                let region = range.to_bbox(self.tile, self.doc_w, self.doc_h);
                let rendered = self.kernel.render_region(region)?;
                self.blit(&rendered);
                self.dirty_renders += 1;
                self.dirty_tiles_total += range.tiles();
            }
            None => {
                let region = Bbox::new(0.0, 0.0, f64::from(self.doc_w), f64::from(self.doc_h));
                let rendered = self.kernel.render_region(region)?;
                self.full_renders += 1;
                self.blit(&rendered);
            }
        }
        Ok(&self.frame)
    }

    /// **把一次区域渲染的结果贴进持久帧缓冲 ✗**（**∴ 这里错一个偏移 ⇒ **画面就错** ✓）★
    fn blit(&mut self, rendered: &yanshi_wasm::RenderResult) {
        let [rx, ry, ..] = rendered.bbox;
        let (rx, ry) = (rx.round() as i64, ry.round() as i64);
        let (rw, rh) = (rendered.width as i64, rendered.height as i64);
        for row in 0..rh {
            let dy = ry + row;
            if dy < 0 || dy >= i64::from(self.doc_h) {
                continue;
            }
            let src_row = row * rw * 4;
            let dst_row = (dy * i64::from(self.doc_w) + rx) * 4;
            for col in 0..rw {
                let dx = rx + col;
                if dx < 0 || dx >= i64::from(self.doc_w) {
                    continue;
                }
                let s = (src_row + col * 4) as usize;
                let d = (dst_row + col * 4) as usize;
                if s + 4 <= rendered.rgba8.len() && d + 4 <= self.frame.len() {
                    self.frame[d..d + 4].copy_from_slice(&rendered.rgba8[s..s + 4]);
                }
            }
        }
    }

    /// **账 ✗**：`(整幅渲染次数, 脏区渲染次数, 脏区累计 tile 数)` ✓（**纯观测 ✓）★
    #[must_use]
    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.full_renders,
            self.dirty_renders,
            self.dirty_tiles_total,
        )
    }

    /// 画布尺寸 ✓（**`(宽, 高)` ✓）★
    #[must_use]
    pub fn doc_size(&self) -> (u32, u32) {
        (self.doc_w, self.doc_h)
    }
}
