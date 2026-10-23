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

use std::collections::BTreeSet;

use serde_json::Value;
use yanshi_core::Bbox;
use yanshi_wasm::{Kernel, KernelError};

/// **RGBA8 → RGB24 ＋ 盒平均缩小，但只算"受影响的那块输出像素"**（第 556 轮）。
///
/// **为什么可以只算一块**：每个输出像素只由**它自己那块源像素**决定 ⇒ 子集与全量
/// **逐字节相同** ✓（判据 `scaled_region_matches_full` 守着）。`frame()` 本来就是增量的
/// （只渲脏 tile），缺的只是"只更新对应的输出像素" ⇒ 每帧成本 ∝ 脏面积，而不是整幅。
///
/// `out_rows` / `out_cols` 是**输出空间**的行、列范围（左闭右开）。
pub fn scale_rgb_region(
    src: &[u8],
    src_size: (u32, u32),
    out: &mut [u8],
    out_size: (u32, u32),
    out_rows: (u32, u32),
    out_cols: (u32, u32),
) {
    let (src_w, src_h) = src_size;
    let (out_w, out_h) = out_size;
    let (sw, sh) = (src_w as usize, src_h as usize);
    let ow = out_w as usize;
    if sw == 0 || sh == 0 || ow == 0 || out_h == 0 {
        return;
    }
    let (row0, row1) = (
        out_rows.0 as usize,
        (out_rows.1 as usize).min(out_h as usize),
    );
    let (col0, col1) = (out_cols.0 as usize, (out_cols.1 as usize).min(ow));
    for oy in row0..row1 {
        let y0 = oy * sh / out_h as usize;
        let y1 = (((oy + 1) * sh) / out_h as usize).max(y0 + 1).min(sh);
        for ox in col0..col1 {
            let x0 = ox * sw / ow;
            let x1 = (((ox + 1) * sw) / ow).max(x0 + 1).min(sw);
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * sw + x) * 4;
                    if i + 3 < src.len() {
                        r += u32::from(src[i]);
                        g += u32::from(src[i + 1]);
                        b += u32::from(src[i + 2]);
                        n += 1;
                    }
                }
            }
            let n = n.max(1);
            let o = (oy * ow + ox) * 3;
            if o + 2 < out.len() {
                out[o] = (r / n) as u8;
                out[o + 1] = (g / n) as u8;
                out[o + 2] = (b / n) as u8;
            }
        }
    }
}

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
    /// **上一帧的脏区（像素，闭开区间）✗**；`None` ⇒ 整幅都脏 ✓（第 556 轮）。
    last_dirty_px: Option<(u32, u32, u32, u32)>,
    /// **降采样后的 RGB24 缓冲 ✗**（**只更新脏区对应的输出像素 ✓）★
    scaled: Vec<u8>,
    /// 上面那个缓冲的尺寸 ✓（变了就整幅重算 ✓）。
    scaled_dims: (u32, u32),
    /// **★★★ 两次出帧之间累计的脏 tile **集合** ✗ ★★★**（**第 249 轮续 ✓）
    ///
    /// **∴ 为什么不是"并集的 bbox" ✗**（**实测换来的 ✓）**：
    ///   ＋ **∴ 前一版**存并集 bbox**✗ ⇒ **∴ 而**散落的 10 笔 ⇒ **它们的 bbox**几乎等于整幅** ✓
    ///     ⇒ **∴ 于是**：**每帧渲了 8.4 个 tile ✗ 而画布只有 12 个** ⇒ **∴ 收益**只剩 18%** ✓
    ///   ＋ **∴ 现在 ✗**：**存集合 ✗ 出帧时按行并成 run**✗ ⇒ **∴ 渲的量**与"**改了多少像素**"
    ///     成正比** ✓ ★★★
    dirty: BTreeSet<(u32, u32)>,
    /// **画布背景色** ✓（从 `create_document` 里记下来 ✓）：还没放过任何对象时，
    /// 出帧就是**一块背景色** ✓ ⇒ 不必为它渲整幅 ✓（第 620 轮；4608×8192 上省 ~27 s/帧 ✓）。
    background: [u8; 4],
    /// **已经出过至少一帧** ✓ —— 没有变化时可复用上一帧，不必整幅重渲 ✓（第 624 轮）。
    have_frame: bool,
    /// **没有变化而复用上一帧的次数** ✓（账用 ✓）。
    reused_frames: u64,
    /// **有没有出现过"非 建文档/建图层"的原子** ✓ —— 没有 ⇒ **帧就是背景色** ✓
    /// （保守：只要来过一个别的 kind 就置 false ✓，之后一律走真渲染 ✓）。
    only_document_and_layers: bool,
    /// **∴ 统计：**整幅渲染了几次／脏区渲染了几次／脏区覆盖了多少 tile** ✓（**账用 ✓）★
    full_renders: u64,
    /// 见 [`Timeline::stats`]。
    dirty_renders: u64,
    /// 见 [`Timeline::stats`]。
    dirty_tiles_total: u64,
    /// **∴ 一共分成了几条 run ✗**（**每条 run ＝ 一次 `render_region` ✓）★
    dirty_runs: u64,
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
            last_dirty_px: None,
            scaled: Vec::new(),
            scaled_dims: (0, 0),
            dirty: BTreeSet::new(),
            background: [255, 255, 255, 255],
            have_frame: false,
            reused_frames: 0,
            only_document_and_layers: true,
            full_renders: 0,
            dirty_renders: 0,
            dirty_tiles_total: 0,
            dirty_runs: 0,
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
        // **记背景色 ＋ 判断"还没放过对象"** ✓（第 620 轮）
        if let Ok(atom) = serde_json::from_str::<Value>(atom_json) {
            match atom.get("kind").and_then(Value::as_str) {
                Some("create_document") => {
                    if let Some(background) = atom.get("payload").and_then(|p| p.get("background"))
                    {
                        let channel = |key: &str| {
                            background
                                .get(key)
                                .and_then(Value::as_u64)
                                .map(|value| value.min(255) as u8)
                                .unwrap_or(255)
                        };
                        self.background = [channel("r"), channel("g"), channel("b"), channel("a")];
                    }
                }
                // 建图层不影响"画面还是背景色"这一点 ✓
                Some("create_layer") => {}
                // 其它任何 kind 都可能改像素 ⇒ 以后一律走真渲染 ✓
                Some(_) | None => self.only_document_and_layers = false,
            }
        }
        let out = self.kernel.apply_atom_json(atom_json)?;
        for key in self.kernel.last_dirty_tiles() {
            // **∴ 只**记下 tile 坐标 ✗ 不在这里算 bbox** ✓（**∴ 出帧时才按行并 run ✓）★
            self.dirty.insert((key.x, key.y));
        }
        Ok(out)
    }

    /// **出一帧 ✗ 返回持久缓冲的 RGBA8（**整幅 ✓）★
    ///
    /// **∴ 逻辑 ✗**：**有脏 tile ⇒ **按行并成 run**✗ 逐条渲 ＋ blit** ✓
    ///   ＋ **∴ 没有脏信息（**比如第一帧 ✓）⇒ **整幅渲 ＋ 覆盖** ✓
    ///     ＋ **∴ 出完帧 ⇒ **清空集合**（**缓冲已经是最新的 ✓）★
    ///
    /// **∴ 为什么不取"并集的 bbox" ✗**（**上一版就是这么写的 ✓）**：**散落的 10 笔
    ///   并集 bbox 几乎等于整幅**✗ ⇒ **∴ 实测**每帧渲 8.4 个 tile ✗ 画布才 12 个** ✓
    ///     ⇒ **∴ 收益**只剩 18%** ✓ ⇒ **∴ 所以**改成**逐条 run** ✓ ★★
    pub fn frame(&mut self) -> Result<&[u8], KernelError> {
        let dirty = std::mem::take(&mut self.dirty);
        // **记录这一帧的脏区**（像素口径）⇒ 缩小那步可以只更新对应输出像素 ✓
        self.last_dirty_px = if dirty.is_empty() {
            None
        } else {
            let mut x0 = u32::MAX;
            let mut y0 = u32::MAX;
            let mut x1 = 0u32;
            let mut y1 = 0u32;
            for (x, y) in &dirty {
                x0 = x0.min(*x);
                y0 = y0.min(*y);
                x1 = x1.max(*x);
                y1 = y1.max(*y);
            }
            Some((
                x0.saturating_mul(self.tile),
                y0.saturating_mul(self.tile),
                (x1 + 1).saturating_mul(self.tile).min(self.doc_w),
                (y1 + 1).saturating_mul(self.tile).min(self.doc_h),
            ))
        };
        // **★ 只建过文档/图层 ⇒ 这一帧就是背景色 ⇒ 不要渲整幅 ★**（第 620 轮）
        //   ⇒ 4608×8192 上**每帧 ~27 s** ⇒ 前两帧直接省掉 ✓（外部第 92 次的"卡死"就是这两帧起步 ✓）
        //   ⇒ 判据 `blank_frames_are_background_not_full_renders`（`incremental_matches_full.rs` ✓）
        if self.only_document_and_layers {
            let pixel = self.background;
            for chunk in self.frame.chunks_exact_mut(4) {
                chunk.copy_from_slice(&pixel);
            }
            self.last_dirty_px = Some((0, 0, self.doc_w, self.doc_h));
            self.have_frame = true;
            return Ok(&self.frame);
        }
        if dirty.is_empty() {
            // **★ 没有任何变化 ⇒ 复用上一帧，别整幅重渲 ★**（第 624 轮）
            //   ⇒ 旧行为：**空脏区也渲整幅** ✗ ⇒ 收尾那一帧白花 **~27 s**（4608×8192）✓
            //   ⇒ 判据 `a_frame_with_no_changes_reuses_the_last_one`（`incremental_matches_full.rs` ✓）
            if self.have_frame {
                self.reused_frames += 1;
                return Ok(&self.frame);
            }
            let region = Bbox::new(0.0, 0.0, f64::from(self.doc_w), f64::from(self.doc_h));
            let rendered = self.kernel.render_region(region)?;
            self.full_renders += 1;
            self.blit(&rendered);
            self.have_frame = true;
            return Ok(&self.frame);
        }
        // **∴ 同一行里**连着的 tile 合成一条**✗ 才能把每次 `render_region` 的固定开销摊薄** ✓ ★
        let rows: BTreeSet<u32> = dirty.iter().map(|(_, y)| *y).collect();
        for y in rows {
            let mut runs: Vec<(u32, u32)> = Vec::new();
            let mut run: Option<(u32, u32)> = None;
            for (x, _) in dirty.iter().filter(|(_, ty)| *ty == y) {
                match run {
                    Some((start, end)) if *x == end + 1 => run = Some((start, *x)),
                    Some((start, end)) => {
                        runs.push((start, end));
                        run = Some((*x, *x));
                    }
                    None => run = Some((*x, *x)),
                }
            }
            if let Some(pair) = run {
                runs.push(pair);
            }
            for (start, end) in runs {
                let range = TileRange {
                    x0: start,
                    y0: y,
                    x1: end,
                    y1: y,
                };
                let region = range.to_bbox(self.tile, self.doc_w, self.doc_h);
                let rendered = self.kernel.render_region(region)?;
                self.blit(&rendered);
                self.dirty_renders += 1;
                self.dirty_tiles_total += range.tiles();
                self.dirty_runs += 1;
            }
        }
        self.have_frame = true;
        Ok(&self.frame)
    }

    /// **出一帧"降采样后的 RGB24"**（第 556 轮）：内部只更新**脏区对应的输出像素** ✓。
    ///
    /// **为什么快**：整幅缩小一次要 ~3.9 s（`4608×8192 → 960×1707` 实测），而每帧真正变的
    /// 只是几个 tile ⇒ 只重算那几个输出像素即可。**子集与全量逐字节相同**（判据
    /// `scaled_region_matches_full` 守着 ✓）。
    pub fn frame_rgb_scaled(&mut self, out_w: u32, out_h: u32) -> Result<&[u8], KernelError> {
        self.frame()?;
        if self.scaled_dims != (out_w, out_h)
            || self.scaled.len() != (out_w as usize) * (out_h as usize) * 3
        {
            self.scaled = vec![0u8; (out_w as usize) * (out_h as usize) * 3];
            self.scaled_dims = (out_w, out_h);
            self.last_dirty_px = None;
        }
        let (rows, cols) = match self.last_dirty_px {
            None => ((0, out_h), (0, out_w)),
            Some((x0, y0, x1, y1)) => {
                // 输出行 oy 覆盖源行 [y0', y1')：与脏区相交才算 ✓
                let mut row_lo = out_h;
                let mut row_hi = 0u32;
                for oy in 0..out_h {
                    let sy0 = (oy as u64 * self.doc_h as u64 / out_h as u64) as u32;
                    let sy1 = (((oy + 1) as u64 * self.doc_h as u64 / out_h as u64) as u32)
                        .max(sy0 + 1)
                        .min(self.doc_h);
                    if sy1 > y0 && sy0 < y1 {
                        row_lo = row_lo.min(oy);
                        row_hi = row_hi.max(oy + 1);
                    }
                }
                let mut col_lo = out_w;
                let mut col_hi = 0u32;
                for ox in 0..out_w {
                    let sx0 = (ox as u64 * self.doc_w as u64 / out_w as u64) as u32;
                    let sx1 = (((ox + 1) as u64 * self.doc_w as u64 / out_w as u64) as u32)
                        .max(sx0 + 1)
                        .min(self.doc_w);
                    if sx1 > x0 && sx0 < x1 {
                        col_lo = col_lo.min(ox);
                        col_hi = col_hi.max(ox + 1);
                    }
                }
                ((row_lo, row_hi), (col_lo, col_hi))
            }
        };
        scale_rgb_region(
            &self.frame,
            (self.doc_w, self.doc_h),
            &mut self.scaled,
            (out_w, out_h),
            rows,
            cols,
        );
        Ok(&self.scaled)
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
                    // **★ 必须按 alpha 合成（`over`），不能直接覆盖 ★**（第 1575 轮；外部实测）
                    //   **⇒ 他们的最小复现**：参考底图（满幅不透明）+ 1 笔 + 抽帧 ⇒
                    //     **实测那一帧是"底图 + 笔 + 该笔补丁的白色矩形"** ✗
                    //   **⇒ ⇒ 根因**：`brush_stroke` 落库的是 `import_image` 的**位图补丁**，
                    //     RGBA 里**笔之外是 `a=0`** ⇒ 而这里原来 `copy_from_slice` **整段覆盖**
                    //     ⇒ **底图被 `a=0` 替换成透明** ⇒ **编码成图/视频时就变白** ✗
                    //   **⇒ ⇒ ⇒ 所以**改成**标准 `over`**（非预乘）：
                    //     `out_a = sa + da·(1-sa)`、`out_c = (sc·sa + dc·da·(1-sa)) / out_a`
                    //     ⇒ **透明区透出底图**，**不透明区盖住底图** ✓
                    //   **⇒ ⇒ ⇒ ⇒ 而帧初值是**全零**（透明）⇒ `over` 在透明底上也正确** ✓
                    let mut src = [0u8; 4];
                    src.copy_from_slice(&rendered.rgba8[s..s + 4]);
                    let mut dst = [0u8; 4];
                    dst.copy_from_slice(&self.frame[d..d + 4]);
                    self.frame[d..d + 4].copy_from_slice(&over_pixel(dst, src));
                }
            }
        }
    }

    /// **账 ✗**：`(整幅渲染次数, 脏区渲染次数, 脏区累计 tile 数)` ✓（**纯观测 ✓）★
    #[must_use]
    pub fn stats(&self) -> (u64, u64, u64, u64) {
        (
            self.full_renders,
            self.dirty_renders,
            self.dirty_tiles_total,
            self.dirty_runs,
        )
    }

    /// **没有变化而复用上一帧的次数** ✓（第 626 轮：让"省下来多少"看得见 ✓）。
    #[must_use]
    pub fn reused_frames(&self) -> u64 {
        self.reused_frames
    }

    /// 画布尺寸 ✓（**`(宽, 高)` ✓）★
    #[must_use]
    pub fn doc_size(&self) -> (u32, u32) {
        (self.doc_w, self.doc_h)
    }
}

/// **一个像素的 `over` 合成**（非预乘 RGBA）✓ —— 第 1575 轮抽成**纯函数**，于是**可单测** ✓
///
/// **⇒ 为什么要它**：外部实测「参考底图 + 1 笔」抽帧后**出现该笔补丁的白色矩形** ✗
///   ⇒ 根因是这里原来**直接覆盖** ⇒ 笔补丁的 `a=0` 区**把底图替换成透明** ✓
/// **⇒ ⇒ 语义**：
///   * `src.a == 0` ⇒ **原样返回 dst**（透明的东西不该改变任何像素）✓
///   * `src.a == 255` ⇒ **原样返回 src**（不透明的东西完全盖住）✓
///   * 其余 ⇒ `out_a = sa + da·(1-sa)`、`out_c = (sc·sa + dc·da·(1-sa)) / out_a` ✓
#[must_use]
pub fn over_pixel(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    let sa = f32::from(src[3]) / 255.0;
    if sa >= 1.0 {
        return src;
    }
    if sa <= 0.0 {
        return dst;
    }
    let da = f32::from(dst[3]) / 255.0;
    let out_a = sa + da * (1.0 - sa);
    if out_a <= 0.0 {
        return [0, 0, 0, 0];
    }
    let blend = |sc: u8, dc: u8| -> u8 {
        let value = (f32::from(sc) * sa + f32::from(dc) * da * (1.0 - sa)) / out_a;
        value.round().clamp(0.0, 255.0) as u8
    };
    [
        blend(src[0], dst[0]),
        blend(src[1], dst[1]),
        blend(src[2], dst[2]),
        (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::over_pixel;

    /// **透明像素不许改变底图** ✓ —— 这正是外部那个"白色矩形"的形状：
    /// 笔补丁的 `a=0` 区若被覆盖，底图就没了 ✗
    #[test]
    fn transparent_source_leaves_destination_untouched() {
        let dst = [40, 90, 160, 255];
        let src = [255, 255, 255, 0];
        assert_eq!(over_pixel(dst, src), dst, "a=0 必须一个字节都不改");
    }

    /// **不透明像素完全盖住底图** ✓
    #[test]
    fn opaque_source_replaces_destination() {
        let dst = [40, 90, 160, 255];
        let src = [255, 0, 0, 255];
        assert_eq!(over_pixel(dst, src), src);
    }

    /// **半透明按比例混合** ✓（`sa≈0.5`、底不透明 ⇒ 中点、alpha 仍 255）
    #[test]
    fn half_transparent_blends_evenly() {
        let out = over_pixel([0, 0, 0, 255], [200, 200, 200, 128]);
        assert_eq!(out[3], 255, "底不透明 ⇒ 结果仍不透明");
        for channel in &out[..3] {
            assert!(
                (i16::from(*channel) - 100).abs() <= 2,
                "中点应在 100 附近，实测 {channel}"
            );
        }
    }

    /// **透明底 + 半透明源 ⇒ alpha 累加、颜色保留** ✓
    #[test]
    fn transparent_destination_accumulates_alpha() {
        let out = over_pixel([0, 0, 0, 0], [100, 100, 100, 128]);
        assert_eq!(out[3], 128);
        assert!(
            out[0] >= 99 && out[0] <= 101,
            "颜色应原样保留，实测 {}",
            out[0]
        );
    }
}

#[cfg(test)]
mod scaled_tests {
    use super::scale_rgb_region;

    fn sample(w: u32, h: u32) -> Vec<u8> {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.push(((x * 7 + y * 13) % 256) as u8);
                data.push(((x * 3 + y * 29) % 256) as u8);
                data.push(((x * 51 + y * 5) % 256) as u8);
                data.push(255);
            }
        }
        data
    }

    /// **判据**：只算"受影响的那块输出像素"必须与**全量缩小**逐字节相同（截取对应子块比较）。
    /// **变异**：把 `out_rows`/`out_cols` 的边界改成开区间（少算一行/一列）⇒ 该子块与全量不同 ⇒ 红。
    #[test]
    fn scaled_region_matches_full() {
        for (sw, sh, ow, oh) in [
            (64u32, 48u32, 16u32, 12u32),
            (33, 17, 9, 5),
            (100, 60, 7, 3),
        ] {
            let src = sample(sw, sh);
            let mut full = vec![0u8; (ow * oh * 3) as usize];
            scale_rgb_region(&src, (sw, sh), &mut full, (ow, oh), (0, oh), (0, ow));
            for (rows, cols) in [
                ((0u32, 3u32), (0u32, 2u32)),
                ((2, 5), (1, 4)),
                ((0, oh), (0, ow)),
            ] {
                let mut part = vec![0u8; (ow * oh * 3) as usize];
                scale_rgb_region(&src, (sw, sh), &mut part, (ow, oh), rows, cols);
                for oy in rows.0 as usize..(rows.1 as usize).min(oh as usize) {
                    for ox in cols.0 as usize..(cols.1 as usize).min(ow as usize) {
                        let o = (oy * ow as usize + ox) * 3;
                        assert_eq!(
                            &part[o..o + 3],
                            &full[o..o + 3],
                            "缩小 {sw}x{sh}→{ow}x{oh} 区域 {rows:?}/{cols:?} 的像素 ({ox},{oy}) 必须与全量一致"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod scaled_bench {
    use super::scale_rgb_region;

    /// **量一下"只算脏区"到底省多少**（`--nocapture` 时打印）。
    /// 4608×8192 → 960×1707 的画布上，一个 256×256 的脏区对应约 53×53 个输出像素。
    #[test]
    fn dirty_region_scaling_is_much_cheaper_than_full() {
        let (sw, sh, ow, oh) = (4608u32, 8192u32, 960u32, 1707u32);
        let mut src = vec![0u8; (sw * sh * 4) as usize];
        for (i, byte) in src.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        let mut out = vec![0u8; (ow * oh * 3) as usize];
        let full = std::time::Instant::now();
        scale_rgb_region(&src, (sw, sh), &mut out, (ow, oh), (0, oh), (0, ow));
        let full_ms = full.elapsed().as_secs_f64() * 1000.0;
        // 一个 256×256 的脏区（源像素）⇒ 输出行列各约 53 格
        let (row0, row1, col0, col1) = (0u32, 53u32, 0u32, 53u32);
        let dirty = std::time::Instant::now();
        scale_rgb_region(
            &src,
            (sw, sh),
            &mut out,
            (ow, oh),
            (row0, row1),
            (col0, col1),
        );
        let dirty_ms = dirty.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "[bench-scale] 整幅 {full_ms:.0} ms｜256×256 脏区 {dirty_ms:.1} ms ⇒ {:.0}×",
            full_ms / dirty_ms.max(0.001)
        );
        assert!(
            dirty_ms * 20.0 < full_ms,
            "只算脏区应当便宜一个数量级以上（整幅 {full_ms:.0} ms，脏区 {dirty_ms:.1} ms）"
        );
    }
}
