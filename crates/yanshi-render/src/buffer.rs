//! 区域像素缓冲（设计文档 6.4 / 8.3）。
//!
//! Tile 是缓存与持久化的唯一物化形式；**渲染计算在区域缓冲区上进行**，
//! 这样滤镜、图层混合等需要邻域或整层的运算可以正确工作，最后再切片进 tile 缓存。

use crate::blend::{blend_pixel, over, BlendMode};
use crate::color::{premultiply, LinearRgba};
use crate::geometry::Coverage;
use yanshi_core::Bbox;

/// **★ `Quantizer` 的**进程级缓存**✗ ★**（**第 512 轮 ✓）
///
/// **∴ 为什么必须有它 ✗**：**建 `Quantizer` ＝ 建 `wgpu::Instance` ＋ 枚举 adapter** ✓
///   ⇒ **∴ 实测 ≈ **32 ms****（**核显／Vulkan ✓）★
///     ＋ **∴ 而**规模阈值**必须先知道 `device_class`**✗
///       ⇒ **∴ 而**那**又必须先建 quantizer** ✓
///         ⇒ **∴ 于是**：**小图（**最终走 CPU ✓）**也白付 32 ms** ✓ ★**** ✓✓
///
/// **∴ 安全性 ✗**：**LUT 是全局常量 ⇒ 同一个 quantizer 可以复用** ✓
///   ＋ **∴ `--gpu off` 在调用它**之前**就返回**✗ ⇒ **∴ 那个模式**不碰缓存** ✓ ★**** ✓✓
// **★ 必须加 cfg 门 ✗ ★**（**第 512 轮实测 ✓）
//   **∴ 为什么 ✗**：**没加它 ⇒ `cargo test --workspace`（**无 feature ✓）
//     报 `cannot find module or crate yanshi_gpu`** ✓
//     ＋ **∴ 那**正是那条门禁存在的意义**（**抓非 feature 构建 ✓）★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(crate) fn quantizer_cache() -> &'static std::sync::Mutex<Option<yanshi_gpu::Quantizer>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<yanshi_gpu::Quantizer>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

/// **★★★ 取（必要时**创建**）进程级 GPU 量化器 ✗ ★★★**（**第 246 轮 ✓）
///
/// **∴ 用户裁定（**原文 ✓）✗**：
///   「**如果有 gpu 用 ✗ 那么就**打开 gpu 能加速且确实有收益的功能**」 ✓
///
/// **∴ 它解决什么 ✗**（**∴ 一个**真的缺口 ✗ 有测试为证 ✓）★**：
///   **∴ 在此之前 ✗**：**只有**量化路（`try_quantize_on_gpu_keyed` ✓）会创建它** ✓
///     ＋ **∴ 而**滤镜路（**`filter::try_gpu_gaussian_blur` ✓）**只**读缓存**✗
///       ⇒ **∴ 于是**：**若**某一帧**先做大滤镜、后做输出量化**✗**（**∴ 那**正是
///       渲染的**顺序**：先画对象／滤镜 ✗ 最后才量化输出 ✓）**
///         ⇒ **∴ 滤镜**看到的是**空缓存** ⇒ **∴ 回退 CPU** ✗
///           ⇒ **★ 而**明明有 GPU ✗ ★**（**∴ 只有****第二帧起**才用得上** ✓）★★
///   **∴ 现在 ✗**：**创建逻辑**只有一处**✗ ⇒ **∴ 谁先要用**谁创建** ✓ ★
///
/// **∴ 门控 ✗**（**两条都必须 ✓）★**：
///   **∴ ①** 与 `quantizer_cache` 同一个 `cfg`**✗（**∴ 否则**非 gpu 构建会编译不过 ✓）**；
///   **∴ ②** `crate::gpu_disabled()`**✗ —— **∴ `--gpu off` **必须**连设备都不创建** ✓
///     （**∴ 否则**那个参数**说了假话**✗ 而**判据 ② 正是守它的 ✓）★
///
/// **∴ 返回值 ✗**：**`true` ＝ 可用**｜**`false` ＝ 不可用（**模式／适配器／创建失败 ✓）** ✓
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub(crate) fn ensure_quantizer(lut: &[f32]) -> bool {
    if crate::gpu_disabled() {
        return false;
    }
    let cache = quantizer_cache();
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        match yanshi_gpu::Quantizer::new(lut) {
            Ok(q) => *guard = Some(q),
            Err(e) => {
                eprintln!("GPU 不可用 ⇒ 回退 CPU：{e}");
                return false;
            }
        }
    }
    true
}

/// **∴ 看缓存里**已经有**的设备等级 ✗**（**第 246 轮 ✓；**滤镜门槛分档要用 ✓）
///
/// **∴ 语义 ✗**：**`Some(class)` ＝ 缓存里**已经有**设备**✗ **`None` ＝ 还没有** ✓
///   ＋ **∴ 它**只读 ✗ **不创建** ✓（**∴ 于是**：**调用方**可以先按保守档判规模**✗
///     而**不必为了一个小图付 ~32 ms 的设备创建** ✓）★★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
#[must_use]
pub(crate) fn quantizer_device_class() -> Option<yanshi_gpu::DeviceClass> {
    let cache = quantizer_cache();
    let guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.as_ref().map(yanshi_gpu::Quantizer::device_class)
}

/// **∴ 测试用：清空进程级量化器缓存 ✗**（**第 246 轮 ✓）
///
/// **∴ 为什么需要它 ✗**：**"第一帧"的语义就是**缓存为空**✗
///   ＋ **∴ 而**同一进程里**别的判据**可能已经填过它** ✓
///     ⇒ **∴ 于是**：**判据要先**清一次**✗ 并**报告它原来是不是空的** ✓ ★
///
/// **∴ 为什么不用 `#[cfg(test)]` ✗**：**集成测试**把本 crate 当**普通依赖**链接**✗
///   ⇒ **∴ `cfg(test)` 对它们**不生效** ✓
///     ⇒ **∴ 所以**：**用 `#[doc(hidden)]` ＋ 名字带 `_for_test`** ✓（**与
///       `gpu_composite_calls`／`reset_blur_degrade` 同一约定 ✓）★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
#[doc(hidden)]
pub fn clear_quantizer_cache_for_test() -> bool {
    let cache = quantizer_cache();
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let was_empty = guard.is_none();
    *guard = None;
    was_empty
}

/// 有符号整数的上取整除法（`a` 可为负，`b` 必须 > 0；取样步长恒 > 0）。
///
/// 标准库的 `i64::div_ceil` 仍是不稳定特性（`int_roundings`，Rust 1.99 未稳定），
/// 这里用稳定的 [`i64::div_euclid`] 手写（`b > 0` 时：整除取商，否则 floor 商 + 1）。
pub(crate) fn div_ceil_i64(a: i64, b: i64) -> i64 {
    debug_assert!(b > 0, "div_ceil_i64: 除数必须 > 0");
    let q = a.div_euclid(b);
    if a % b == 0 {
        q
    } else {
        q + 1
    }
}

/// 线性光、预乘 alpha 的区域缓冲区（行主序）。
///
/// **取样步长 `step`**（期 4，`export_png` 的 `lod`）：缓冲像素 `(lx, ly)` 对应文档坐标
/// `(origin_x + lx * step, origin_y + ly * step)`。`step == 1` 即今天的行为（逐像素）；
/// `step == 2^k` 时缓冲只有 `ceil(文档宽 / step) × ceil(文档高 / step)` 个像素，渲染取样
/// 密度即 `1 / step`——输出像素 `(i, j)` 采样文档坐标 `(x0 + i * step, y0 + j * step)` 的
/// 那个文档像素（点采样），不经过全分辨率中间缓冲。`bbox()` 恒返回**文档范围**
/// （`width * step`），局部索引方法（`pixel`／`set_pixel`／`blend`／`fill`／`to_rgba8`）
/// 只认局部坐标，与步长无关；文档坐标与局部坐标的换算只发生在 `blit_*`／`fill_coverage*`／
/// `crop`／`copy_region_into`／`composite*` 里。
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer {
    origin_x: i64,
    origin_y: i64,
    width: u32,
    height: u32,
    /// 取样步长（文档像素／缓冲像素），缺省 1。
    step: u32,
    /// **∴ 第 308 轮**放宽为 `pub`**✗（**判据要用它当输入 ✓）
    pub pixels: Vec<f32>,
}

impl Buffer {
    /// 全透明缓冲区，原点为文档坐标 `(origin_x, origin_y)`。
    #[track_caller]
    pub fn new(origin_x: i64, origin_y: i64, width: u32, height: u32) -> Self {
        Self::new_stepped(origin_x, origin_y, width, height, 1)
    }

    /// 与 [`Buffer::new`] 相同，但取样步长为 `step`（期 4 `lod` 用；`step` 必须 ≥ 1）。
    #[track_caller]
    pub fn new_stepped(origin_x: i64, origin_y: i64, width: u32, height: u32, step: u32) -> Self {
        Self {
            origin_x,
            origin_y,
            width,
            height,
            step: step.max(1),
            pixels: {
                // **分配探针**：`Buffer::new` 绕过池的那些路径（直接 `vec!`）。
                let len = (width * height * 4) as usize;
                let location = std::panic::Location::caller();
                crate::alloc_probe::record_site(
                    crate::alloc_probe::AllocPoint::BufferFresh,
                    len * std::mem::size_of::<f32>(),
                    location.file(),
                    location.line(),
                );
                vec![0.0; len]
            },
        }
    }

    /// 取样步长（文档像素／缓冲像素）：1 = 逐像素，`2^k` = LOD 档位。
    pub const fn step(&self) -> u32 {
        self.step
    }

    /// 缓冲覆盖的文档宽（像素）=`width * step`。
    pub const fn doc_width(&self) -> u32 {
        self.width * self.step
    }

    /// 缓冲覆盖的文档高（像素）=`height * step`。
    pub const fn doc_height(&self) -> u32 {
        self.height * self.step
    }

    /// 单色填充缓冲区。
    pub fn filled(
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        pixel: LinearRgba,
    ) -> Self {
        Self::filled_stepped(origin_x, origin_y, width, height, pixel, 1)
    }

    /// 与 [`Buffer::filled`] 相同，但取样步长为 `step`（期 4 `lod` 用）。
    pub fn filled_stepped(
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        pixel: LinearRgba,
        step: u32,
    ) -> Self {
        let mut buffer = Self::new_stepped(origin_x, origin_y, width, height, step);
        buffer.fill(pixel);
        buffer
    }

    /// 宽。
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 高。
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// 原点（文档坐标）。
    pub const fn origin(&self) -> (i64, i64) {
        (self.origin_x, self.origin_y)
    }

    /// 缓冲区覆盖的文档范围（`step > 1` 时宽高是文档像素数 `width * step`，
    /// 不是缓冲像素数——凡按文档坐标裁剪／求交的代码都读它）。
    pub fn bbox(&self) -> Bbox {
        Bbox::new(
            self.origin_x as f64,
            self.origin_y as f64,
            self.doc_width() as f64,
            self.doc_height() as f64,
        )
    }

    /// 像素总数。
    pub fn len(&self) -> usize {
        (self.width * self.height) as usize
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 原始 f32 切片（线性预乘）。
    /// 可变访问底层 f32 像素（用于就地量化；不改尺寸与原点）。
    /// 只读访问底层 f32 像素（行优先，RGBA 预乘线性）。
    ///
    /// **∴ 只读像素视图 ✗**（**第 698 轮 ✓）★
    ///
    /// **∴ 为什么需要 ✗**：**`pixels_mut` **需要 `&mut`**✗
    ///   ＋ **∴ 而**很多调用点**只有 `&self`**✗（**∴ 如** GPU 上传前的取源 ✓）
    ///     ＋ **∴ 于是**：**它们**只能**逐点** `pixel(x,y)`**✗ ⇒ **∴ 又慢又啰嗦** ✓
    ///       ＋ **∴ 而**本会话**已为此**踩了三次**✗ ⇒ **∴ 补上 ✓ ★
    #[must_use]
    pub fn pixels(&self) -> &[f32] {
        &self.pixels
    }

    /// **∴ 可变像素视图 ✗**（**写入用 ✓）★
    ///
    /// **∴ 与 `pixels()` 的分工 ✗**：**只读用 `pixels()`**✗（**∴ 第 698 轮补 ✓）
    ///   ＋ **∴ 需要**就地修改**✗（**∴ 如**滤镜、量化 ✓）**才用本方法 ✓ ★
    pub fn pixels_mut(&mut self) -> &mut [f32] {
        &mut self.pixels
    }

    /// 由 f32 线性预乘像素构造（长度必须为 `width * height * 4`）。
    ///
    /// 与 `blit_rgba8` 的区别是**不经过 8 位量化**——tile 增量盖章每帧都要读改写，
    /// 走 u8 会在多帧后累积可见漂移。
    pub fn from_f32(
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        pixels: &[f32],
    ) -> Option<Self> {
        if pixels.len() != (width * height * 4) as usize {
            return None;
        }
        Some(Self {
            origin_x,
            origin_y,
            width,
            height,
            step: 1,
            pixels: pixels.to_vec(),
        })
    }

    /// 由 f32 线性预乘像素构造（**move 语义，不拷**）✓ —— 与 `from_f32` 同一校验 ✓。
    ///
    /// **P2-7**：`render_document_preview` 分块转换时用它 ✓ —— 整幅 `Vec<f32>` 再拷一份
    /// 是首层峰值的一半（4K 上约 204.8MB）⇒ 小带直接 move 进来，不再多占一份 ✓。
    pub fn from_f32_vec(
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        pixels: Vec<f32>,
    ) -> Option<Self> {
        if pixels.len() != (width * height * 4) as usize {
            return None;
        }
        Some(Self {
            origin_x,
            origin_y,
            width,
            height,
            pixels,
        })
    }

    /// 只读访问底层 f32 像素（行优先，RGBA 预乘线性）。
    pub fn as_f32(&self) -> &[f32] {
        &self.pixels
    }

    /// 像素（越界返回全透明）。
    pub fn pixel(&self, x: u32, y: u32) -> LinearRgba {
        if x >= self.width || y >= self.height {
            return [0.0; 4];
        }
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ]
    }

    /// 写入像素（越界忽略）。
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: LinearRgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = ((y * self.width + x) * 4) as usize;
        self.pixels[index..index + 4].copy_from_slice(&pixel);
    }

    /// 以 source-over 叠加。
    pub fn blend(&mut self, x: u32, y: u32, source: LinearRgba) {
        let backdrop = self.pixel(x, y);
        self.set_pixel(x, y, over(source, backdrop));
    }

    /// 以指定混合模式叠加。
    pub fn blend_mode(&mut self, x: u32, y: u32, source: LinearRgba, mode: BlendMode) {
        if mode == BlendMode::Normal {
            // 热路径：预乘表示的 source-over 不需要反预乘。
            self.blend(x, y, source);
            return;
        }
        let backdrop = self.pixel(x, y);
        self.set_pixel(x, y, blend_pixel(mode, source, backdrop));
    }

    /// 直接索引的 source-over（调用方保证坐标已裁剪在缓冲区内）。
    ///
    /// 用于笔刷 stamping 等内层热路径：省去逐像素的边界判断与局部坐标换算。
    pub fn blend_at(&mut self, x: u32, y: u32, source: LinearRgba) {
        debug_assert!(x < self.width && y < self.height, "blend_at 越界");
        let index = ((y * self.width + x) * 4) as usize;
        let inverse = 1.0 - source[3];
        self.pixels[index] = source[0] + self.pixels[index] * inverse;
        self.pixels[index + 1] = source[1] + self.pixels[index + 1] * inverse;
        self.pixels[index + 2] = source[2] + self.pixels[index + 2] * inverse;
        self.pixels[index + 3] = source[3] + self.pixels[index + 3] * inverse;
    }

    /// 整体填充。
    pub fn fill(&mut self, pixel: LinearRgba) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&pixel);
        }
    }

    /// 用直通颜色 + 覆盖率合成（`opacity` 为整体不透明度）。
    pub fn fill_coverage(
        &mut self,
        coverage: &Coverage,
        color: LinearRgba,
        mode: BlendMode,
        opacity: f32,
    ) {
        let base = [
            color[0].clamp(0.0, 1.0),
            color[1].clamp(0.0, 1.0),
            color[2].clamp(0.0, 1.0),
            color[3].clamp(0.0, 1.0),
        ];
        let opacity = opacity.clamp(0.0, 1.0);
        // **期 4 `lod`**：`step > 1` 时只取落在取样格点上的文档像素（点采样）——
        // 输出像素 `(lx, ly)` 采样文档坐标 `(origin_x + lx * step, origin_y + ly * step)`；
        // 覆盖率网格本身仍是全密度算的（`shape_coverage_in` 那一步不省），这里只省
        // 「填」的遍历。`step == 1` 走下面原路（逐字节与 686 轮的切片优化一致）。
        if self.step > 1 {
            self.fill_coverage_strided(coverage, base, mode, opacity);
            return;
        }
        // 只遍历「覆盖率网格 ∩ 本缓冲」——未裁剪的调用方（例如蒙版）也不会白跑。
        let doc_x0 = coverage.bbox.x as i64;
        let doc_y0 = coverage.bbox.y as i64;
        let start_x = (self.origin_x - doc_x0).max(0) as u32;
        let start_y = (self.origin_y - doc_y0).max(0) as u32;
        let end_x = (self.origin_x + self.width as i64 - doc_x0).min(coverage.width as i64);
        let end_y = (self.origin_y + self.height as i64 - doc_y0).min(coverage.height as i64);
        if end_x <= start_x as i64 || end_y <= start_y as i64 {
            return;
        }
        for y in start_y..end_y as u32 {
            let local_y = (doc_y0 + y as i64 - self.origin_y) as u32;
            let row = (y * coverage.width) as usize;
            // **★★★ 索引递增 ＋ 切片迭代 ✗ ★★★**（**第 686 轮 ✓；**目标第 11 条 ✓）
            //   **∴ 为什么 ✗**：**原来每像素都要**✗：
            //     **∴ ① `coverage.data[row + x as usize]`**✗（**∴ 一次加 ＋ 一次边界检查 ✓）
            //       ＋ **∴ ② `local_x = (doc_x0 + x as i64 - self.origin_x) as u32`**✗
            //         （**∴ 一次减法 ＋ 一次转 u32 ✓）
            //           ⇒ **∴ 而**两者**都只随 `x` **线性变化**✗ ⇒ **∴ 可以**递增／迭代** ✓
            //   **∴ 改法 ✗**：
            //     **∴ ① 用**切片迭代**✗（**∴ 省掉 `row + x` ＋ 边界检查 ✓）
            //       ＋ **∴ ② `local_x` **按 `dx` 递增**✗（**∴ 省掉减法 ＋ 转 u32 ✓）
            //   **∴ 逐位一致（**关键 ✓）✗**：**算术表达式**逐字不变**✗
            //     ＋ **∴ 只**改了**取覆盖率与算 `local_x` 的方式** ✓
            //       ＋ **∴ `local_x` 的初值与步长**与原来**完全等价** ✓
            //         （**∴ `doc_x0 + x - origin_x` ⇒ `dx` 从 0 起，**初值 ＝ start_x 处** ✓）★
            let row_start = row + start_x as usize;
            let row_end = row + end_x as usize;
            let local_x0 = (doc_x0 + start_x as i64 - self.origin_x) as u32;
            for (dx, &coverage_value) in coverage.data[row_start..row_end].iter().enumerate() {
                if coverage_value <= 0.0 {
                    continue;
                }
                let local_x = local_x0 + dx as u32;
                let alpha = base[3] * coverage_value * opacity;
                let source = premultiply([base[0], base[1], base[2], alpha]);
                // **★★★ 显式复现 `over` 的两个边界分支 ✗ ★★★**（**第 678 轮判据实测 ✓）
                if alpha <= 0.0 {
                    // **∴ `over` 返回 backdrop ⇒ 不改动 ✓
                } else if alpha >= 1.0 {
                    self.set_pixel(local_x, local_y, source);
                } else if mode == BlendMode::Normal {
                    self.blend_at(local_x, local_y, source);
                } else {
                    self.blend_mode(local_x, local_y, source, mode);
                }
            }
        }
    }

    /// `fill_coverage` 的步长分支（`step > 1` 才调用）：只访问取样格点上的覆盖率。
    fn fill_coverage_strided(
        &mut self,
        coverage: &Coverage,
        base: [f32; 4],
        mode: BlendMode,
        opacity: f32,
    ) {
        let step = self.step as i64;
        let doc_x0 = coverage.bbox.x as i64;
        let doc_y0 = coverage.bbox.y as i64;
        // 相交的文档范围（半开）。
        let ix0 = self.origin_x.max(doc_x0);
        let iy0 = self.origin_y.max(doc_y0);
        let ix1 = (self.origin_x + self.width as i64 * step).min(doc_x0 + coverage.width as i64);
        let iy1 = (self.origin_y + self.height as i64 * step).min(doc_y0 + coverage.height as i64);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        // 取样格点锚在本缓冲原点：第一个格点 ≥ 相交左／上界。
        let first_lx = div_ceil_i64(ix0 - self.origin_x, step);
        let first_ly = div_ceil_i64(iy0 - self.origin_y, step);
        let last_lx = (ix1 - 1 - self.origin_x) / step;
        let last_ly = (iy1 - 1 - self.origin_y) / step;
        for ly in first_ly..=last_ly {
            let doc_y = self.origin_y + ly * step;
            let row = ((doc_y - doc_y0) as usize) * coverage.width as usize;
            for lx in first_lx..=last_lx {
                let doc_x = self.origin_x + lx * step;
                let coverage_value = coverage.data[row + (doc_x - doc_x0) as usize];
                if coverage_value <= 0.0 {
                    continue;
                }
                let (local_x, local_y) = (lx as u32, ly as u32);
                let alpha = base[3] * coverage_value * opacity;
                let source = premultiply([base[0], base[1], base[2], alpha]);
                if alpha <= 0.0 {
                    // **∴ `over` 返回 backdrop ⇒ 不改动 ✓
                } else if alpha >= 1.0 {
                    self.set_pixel(local_x, local_y, source);
                } else if mode == BlendMode::Normal {
                    self.blend_at(local_x, local_y, source);
                } else {
                    self.blend_mode(local_x, local_y, source, mode);
                }
            }
        }
    }

    /// **按像素取色填充**（第 1621 轮；外部 `fill_region(texture=)` 的渲染腿）
    ///
    /// **⇒ 与 [`Self::fill_coverage`] 的唯一差别**：颜色不是常数，而是
    /// **每像素问一次 `sample(文档 x, 文档 y)`** ⇒ 于是可以做"从源区域取纹理"。
    ///
    /// **⇒ ⇒ 三条必须守住的语义**（外部给的验收规格）：
    /// 1. **每处颜色都来自某个真实源像素** ⇒ 由调用方的 `sample` 保证（**平铺 = 最近邻**），
    ///    这里**不做插值、不做平均**（插值会造出源里不存在的颜色）✓
    /// 2. **羽化只作用在合成量上** ⇒ `alpha = sample.a × coverage × opacity`，
    ///    **绝不把背景色混进 `sample`** ⇒ 于是边界上不会出现"鬼影" ✓
    /// 3. **`coverage ≤ 0` 的像素一个字节都不碰** ⇒ 与 `fill_coverage` 相同 ✓
    ///
    /// **⇒ ⇒ ⇒ 逐位一致**：`coverage ≤ 0` 跳过、`alpha` 的三个边界分支
    /// （`≤0` 不动 / `≥1` 直写 / 其余 `over`）**与 `fill_coverage` 逐字相同** ✓
    pub fn fill_coverage_sampled(
        &mut self,
        coverage: &Coverage,
        sample: impl Fn(i32, i32) -> LinearRgba,
        mode: BlendMode,
        opacity: f32,
    ) {
        // **期 4 `lod`**：`step > 1` 时只在取样格点上问 `sample`（点采样，文档坐标仍是
        // 被采的那个文档像素的坐标）；`step == 1` 走下面原路。
        if self.step > 1 {
            self.fill_coverage_sampled_strided(coverage, sample, mode, opacity);
            return;
        }
        let doc_x0 = coverage.bbox.x as i64;
        let doc_y0 = coverage.bbox.y as i64;
        let start_x = (self.origin_x - doc_x0).max(0) as u32;
        let start_y = (self.origin_y - doc_y0).max(0) as u32;
        let end_x = (self.origin_x + self.width as i64 - doc_x0).min(coverage.width as i64);
        let end_y = (self.origin_y + self.height as i64 - doc_y0).min(coverage.height as i64);
        if end_x <= start_x as i64 || end_y <= start_y as i64 {
            return;
        }
        let opacity = opacity.clamp(0.0, 1.0);
        for y in start_y..end_y as u32 {
            let local_y = (doc_y0 + y as i64 - self.origin_y) as u32;
            let row = (y * coverage.width) as usize;
            let row_start = row + start_x as usize;
            let row_end = row + end_x as usize;
            let local_x0 = (doc_x0 + start_x as i64 - self.origin_x) as u32;
            let doc_y = doc_y0 + y as i64;
            for (dx, &coverage_value) in coverage.data[row_start..row_end].iter().enumerate() {
                if coverage_value <= 0.0 {
                    continue;
                }
                let local_x = local_x0 + dx as u32;
                let doc_x = doc_x0 + start_x as i64 + dx as i64;
                let texel = sample(doc_x as i32, doc_y as i32);
                let rgb = [
                    texel[0].clamp(0.0, 1.0),
                    texel[1].clamp(0.0, 1.0),
                    texel[2].clamp(0.0, 1.0),
                ];
                // **羽化只乘 alpha** ⇒ 颜色本身不被任何背景污染 ✓
                let alpha = texel[3].clamp(0.0, 1.0) * coverage_value * opacity;
                let source = premultiply([rgb[0], rgb[1], rgb[2], alpha]);
                if alpha <= 0.0 {
                    // `over` 返回 backdrop ⇒ 不改动 ✓
                } else if alpha >= 1.0 {
                    self.set_pixel(local_x, local_y, source);
                } else if mode == BlendMode::Normal {
                    self.blend_at(local_x, local_y, source);
                } else {
                    self.blend_mode(local_x, local_y, source, mode);
                }
            }
        }
    }

    /// `fill_coverage_sampled` 的步长分支（`step > 1` 才调用）。
    fn fill_coverage_sampled_strided(
        &mut self,
        coverage: &Coverage,
        sample: impl Fn(i32, i32) -> LinearRgba,
        mode: BlendMode,
        opacity: f32,
    ) {
        let step = self.step as i64;
        let doc_x0 = coverage.bbox.x as i64;
        let doc_y0 = coverage.bbox.y as i64;
        let ix0 = self.origin_x.max(doc_x0);
        let iy0 = self.origin_y.max(doc_y0);
        let ix1 = (self.origin_x + self.width as i64 * step).min(doc_x0 + coverage.width as i64);
        let iy1 = (self.origin_y + self.height as i64 * step).min(doc_y0 + coverage.height as i64);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        let opacity = opacity.clamp(0.0, 1.0);
        let first_lx = div_ceil_i64(ix0 - self.origin_x, step);
        let first_ly = div_ceil_i64(iy0 - self.origin_y, step);
        let last_lx = (ix1 - 1 - self.origin_x) / step;
        let last_ly = (iy1 - 1 - self.origin_y) / step;
        for ly in first_ly..=last_ly {
            let doc_y = self.origin_y + ly * step;
            let row = ((doc_y - doc_y0) as usize) * coverage.width as usize;
            for lx in first_lx..=last_lx {
                let doc_x = self.origin_x + lx * step;
                let coverage_value = coverage.data[row + (doc_x - doc_x0) as usize];
                if coverage_value <= 0.0 {
                    continue;
                }
                let (local_x, local_y) = (lx as u32, ly as u32);
                let texel = sample(doc_x as i32, doc_y as i32);
                let rgb = [
                    texel[0].clamp(0.0, 1.0),
                    texel[1].clamp(0.0, 1.0),
                    texel[2].clamp(0.0, 1.0),
                ];
                let alpha = texel[3].clamp(0.0, 1.0) * coverage_value * opacity;
                let source = premultiply([rgb[0], rgb[1], rgb[2], alpha]);
                if alpha <= 0.0 {
                    // `over` 返回 backdrop ⇒ 不改动 ✓
                } else if alpha >= 1.0 {
                    self.set_pixel(local_x, local_y, source);
                } else if mode == BlendMode::Normal {
                    self.blend_at(local_x, local_y, source);
                } else {
                    self.blend_mode(local_x, local_y, source, mode);
                }
            }
        }
    }

    /// 以 `mode` 把整块 `source` 合成到自身对应位置（按文档坐标对齐）。
    pub fn composite(&mut self, source: &Buffer, mode: BlendMode, opacity: f32) {
        let factor = opacity.clamp(0.0, 1.0);
        // **期 4 `lod`**：源像素的文档坐标是 `origin + local * source.step`；目标局部坐标
        // 是 `(doc - self.origin) / self.step`，整除才落点（两路同原点同步长时恒成立，
        // `step == 1` 时退化为今天的 1:1 映射）。
        for y in 0..source.height {
            for x in 0..source.width {
                let pixel = source.pixel(x, y);
                let scaled = [
                    pixel[0] * factor,
                    pixel[1] * factor,
                    pixel[2] * factor,
                    pixel[3] * factor,
                ];
                if scaled[3] <= 0.0 && mode == BlendMode::Normal {
                    continue;
                }
                let document_x = source.origin_x + x as i64 * source.step as i64;
                let document_y = source.origin_y + y as i64 * source.step as i64;
                let local_x = document_x - self.origin_x;
                let local_y = document_y - self.origin_y;
                if local_x < 0 || local_y < 0 {
                    continue;
                }
                let self_step = self.step as i64;
                if local_x % self_step != 0 || local_y % self_step != 0 {
                    continue;
                }
                let (local_x, local_y) = ((local_x / self_step) as u32, (local_y / self_step) as u32);
                if local_x >= self.width || local_y >= self.height {
                    continue;
                }
                self.blend_mode(local_x, local_y, scaled, mode);
            }
        }
    }

    /// 以 u8 RGBA（显示空间，直通 alpha）覆盖写入，用于 RasterPatch / import_image。
    ///
    /// `x`/`y` 为文档坐标左上角；`opacity` 为额外不透明度。
    pub fn blit_rgba8(
        &mut self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        rgba8: &[u8],
        opacity: f32,
    ) {
        let factor = opacity.clamp(0.0, 1.0);
        // **期 4 `lod`**：`step > 1` 时只取落在取样格点上的源像素（点采样）。
        if self.step > 1 {
            self.blit_rgba8_strided(x, y, width, height, rgba8, factor);
            return;
        }
        // **只遍历与自身相交的行列**：过去整块补丁（例如覆盖全画布的 import_image）在每个
        // 分块里都被完整扫一遍，逐像素才发现越界 ⇒ 分块渲染时成本 `∝ 补丁面积`（不是 `∝ 相交面积`）。
        // 被跳过的迭代在旧实现里本来就因越界而不写任何像素 ⇒ 结果逐字节相同。
        let col_start = (self.origin_x - x).max(0);
        let row_start = (self.origin_y - y).max(0);
        let col_end = (self.origin_x + self.width as i64 - x).min(width as i64);
        let row_end = (self.origin_y + self.height as i64 - y).min(height as i64);
        if col_end <= col_start || row_end <= row_start {
            return;
        }
        for row in row_start..row_end {
            for col in col_start..col_end {
                let index = ((row * width as i64 + col) * 4) as usize;
                if index + 3 >= rgba8.len() {
                    return;
                }
                let local_x = (x + col - self.origin_x) as u32;
                let local_y = (y + row - self.origin_y) as u32;
                let bytes = [
                    rgba8[index],
                    rgba8[index + 1],
                    rgba8[index + 2],
                    rgba8[index + 3],
                ];
                let mut pixel = crate::color::u8x4_to_linear_premul(bytes);
                for value in pixel.iter_mut() {
                    *value *= factor;
                }
                self.blend(local_x, local_y, pixel);
            }
        }
    }

    /// 以 f32 线性 RGB 三元组（不透明）写入，与 [`Buffer::blit_rgba8`] 并列的
    /// 线性直通路：源值已是线性预乘口径（alpha 恒 1 ⇒ 预乘即自身），**不经
    /// 显示空间量化**——值 >1（HDR）与负值都不在此处截断，截断归出口
    /// （`to_rgba8` 的显示编码）。语义与 blit_rgba8 一致：`opacity` 额外缩放、
    /// 走同一 blend（Normal 即覆盖写入）。
    ///
    /// 通用能力：任何「已经在线性空间里算好的像素」都走这里（相机 RAW 展开
    /// 是第一个用户），本函数不认识任何来源格式。
    pub fn blit_linear(
        &mut self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        rgb_linear: &[f32],
        opacity: f32,
    ) {
        let factor = opacity.clamp(0.0, 1.0);
        // **期 4 `lod`**：`step > 1` 时只取落在取样格点上的源像素（点采样）。
        if self.step > 1 {
            self.blit_linear_strided(x, y, width, height, rgb_linear, factor);
            return;
        }
        // 相交裁剪与 blit_rgba8 同口径（只遍历相交行列，跳过的迭代本就不写）。
        let col_start = (self.origin_x - x).max(0);
        let row_start = (self.origin_y - y).max(0);
        let col_end = (self.origin_x + self.width as i64 - x).min(width as i64);
        let row_end = (self.origin_y + self.height as i64 - y).min(height as i64);
        if col_end <= col_start || row_end <= row_start {
            return;
        }
        for row in row_start..row_end {
            for col in col_start..col_end {
                let index = ((row * width as i64 + col) * 3) as usize;
                if index + 2 >= rgb_linear.len() {
                    return;
                }
                let local_x = (x + col - self.origin_x) as u32;
                let local_y = (y + row - self.origin_y) as u32;
                let pixel = [
                    rgb_linear[index] * factor,
                    rgb_linear[index + 1] * factor,
                    rgb_linear[index + 2] * factor,
                    factor,
                ];
                self.blend(local_x, local_y, pixel);
            }
        }
    }

    /// `blit_rgba8` 的步长分支（`step > 1` 才调用）：源补丁是全密度的，只采格点。
    fn blit_rgba8_strided(
        &mut self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        rgba8: &[u8],
        factor: f32,
    ) {
        let step = self.step as i64;
        let ix0 = self.origin_x.max(x);
        let iy0 = self.origin_y.max(y);
        let ix1 = (self.origin_x + self.width as i64 * step).min(x + width as i64);
        let iy1 = (self.origin_y + self.height as i64 * step).min(y + height as i64);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        let first_lx = div_ceil_i64(ix0 - self.origin_x, step);
        let first_ly = div_ceil_i64(iy0 - self.origin_y, step);
        let last_lx = (ix1 - 1 - self.origin_x) / step;
        let last_ly = (iy1 - 1 - self.origin_y) / step;
        for ly in first_ly..=last_ly {
            let doc_y = self.origin_y + ly * step;
            let row = doc_y - y;
            for lx in first_lx..=last_lx {
                let doc_x = self.origin_x + lx * step;
                let col = doc_x - x;
                let index = ((row * width as i64 + col) * 4) as usize;
                if index + 3 >= rgba8.len() {
                    return;
                }
                let bytes = [
                    rgba8[index],
                    rgba8[index + 1],
                    rgba8[index + 2],
                    rgba8[index + 3],
                ];
                let mut pixel = crate::color::u8x4_to_linear_premul(bytes);
                for value in pixel.iter_mut() {
                    *value *= factor;
                }
                self.blend(lx as u32, ly as u32, pixel);
            }
        }
    }

    /// `blit_linear` 的步长分支（`step > 1` 才调用）：源补丁是全密度的，只采格点。
    fn blit_linear_strided(
        &mut self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        rgb_linear: &[f32],
        factor: f32,
    ) {
        let step = self.step as i64;
        let ix0 = self.origin_x.max(x);
        let iy0 = self.origin_y.max(y);
        let ix1 = (self.origin_x + self.width as i64 * step).min(x + width as i64);
        let iy1 = (self.origin_y + self.height as i64 * step).min(y + height as i64);
        if ix1 <= ix0 || iy1 <= iy0 {
            return;
        }
        let first_lx = div_ceil_i64(ix0 - self.origin_x, step);
        let first_ly = div_ceil_i64(iy0 - self.origin_y, step);
        let last_lx = (ix1 - 1 - self.origin_x) / step;
        let last_ly = (iy1 - 1 - self.origin_y) / step;
        for ly in first_ly..=last_ly {
            let doc_y = self.origin_y + ly * step;
            let row = doc_y - y;
            for lx in first_lx..=last_lx {
                let doc_x = self.origin_x + lx * step;
                let col = doc_x - x;
                let index = ((row * width as i64 + col) * 3) as usize;
                if index + 2 >= rgb_linear.len() {
                    return;
                }
                let pixel = [
                    rgb_linear[index] * factor,
                    rgb_linear[index + 1] * factor,
                    rgb_linear[index + 2] * factor,
                    factor,
                ];
                self.blend(lx as u32, ly as u32, pixel);
            }
        }
    }

    /// 裁剪到文档坐标范围（不相交则返回空缓冲区）。
    pub fn crop(&self, bbox: &Bbox) -> Buffer {
        // **期 4 `lod`**：输出像素是落在相交文档范围内的取样格点（格点锚在本缓冲原点），
        // 像素数 = 格点数；`step == 1` 时退化为今天的 `x1 - x0`（原点与像素数都一致）。
        let step = self.step as i64;
        let x0 = (bbox.x.floor() as i64).max(self.origin_x);
        let y0 = (bbox.y.floor() as i64).max(self.origin_y);
        let x1 = ((bbox.x + bbox.w).ceil() as i64).min(self.origin_x + self.doc_width() as i64);
        let y1 = ((bbox.y + bbox.h).ceil() as i64).min(self.origin_y + self.doc_height() as i64);
        if x1 <= x0 || y1 <= y0 {
            return Buffer::new_stepped(x0, y0, 0, 0, self.step);
        }
        let first_x = div_ceil_i64(x0 - self.origin_x, step);
        let first_y = div_ceil_i64(y0 - self.origin_y, step);
        let width = ((x1 - 1 - self.origin_x) / step - first_x + 1) as u32;
        let height = ((y1 - 1 - self.origin_y) / step - first_y + 1) as u32;
        let mut out = Buffer::new_stepped(
            self.origin_x + first_x * step,
            self.origin_y + first_y * step,
            width,
            height,
            self.step,
        );
        for y in 0..height {
            for x in 0..width {
                let source_x = (first_x + x as i64) as u32;
                let source_y = (first_y + y as i64) as u32;
                out.set_pixel(x, y, self.pixel(source_x, source_y));
            }
        }
        out
    }

    /// 导出为 u8 RGBA（显示空间）；`background` 为不透明底色。
    ///
    /// 本机（非 `wasm32`）上**按行并行**，细节见 [`Buffer::to_rgba8_blocks`]；
    /// `wasm32` 没有共享内存线程 ⇒ 那条路径整体不参与编译，这里走串行循环。
    /// **★ `copy_region_into`：把本缓冲的一块**拷进已有缓冲**（内存修的接口）★**（第 254 轮）
    ///
    /// **为什么需要它**：`crop` 每次调用都 `Buffer::new`（**绕过缓冲池**）—— 实测一帧 2048² 渲染里
    /// 有 **66 次 `Buffer::new`、合计 128 MB 全部来自 `crop`**（`docs/design/render-memory-account.md`
    /// 第十节）。要池化，就得让调用方先 `pool.acquire(x0, y0, w, h)` 借一块**正好尺寸**的缓冲，
    /// 再把像素**拷进去** => 于是有了本函数。
    ///
    /// **与 `crop` 的语义必须逐字节一致**：裁剪框先按本缓冲的 `origin/size` **同一套** `floor/ceil/min/max`
    /// 夹紧，然后把 `out` 的 `(0,0)..(w,h)` 填成对应像素。
    /// **目标尺寸必须正好等于夹紧后的尺寸**，否则返回 `false`（不猜、不越界）——
    /// 那正是"从池里按这个尺寸借"要保证的事。
    /// **本函数自己不分配** ✓（这就是重点）。
    pub fn copy_region_into(&self, bbox: &Bbox, out: &mut Buffer) -> bool {
        // **期 4 `lod`**：与 `crop` 同口径——输出像素是落在相交文档范围内的取样格点；
        // `step == 1` 时退化为今天的行为。
        let step = self.step as i64;
        let x0 = (bbox.x.floor() as i64).max(self.origin_x);
        let y0 = (bbox.y.floor() as i64).max(self.origin_y);
        let x1 = ((bbox.x + bbox.w).ceil() as i64).min(self.origin_x + self.doc_width() as i64);
        let y1 = ((bbox.y + bbox.h).ceil() as i64).min(self.origin_y + self.doc_height() as i64);
        let (width, height, first_x, first_y) = if x1 <= x0 || y1 <= y0 {
            (0, 0, 0, 0)
        } else {
            let first_x = div_ceil_i64(x0 - self.origin_x, step);
            let first_y = div_ceil_i64(y0 - self.origin_y, step);
            let width = ((x1 - 1 - self.origin_x) / step - first_x + 1) as u32;
            let height = ((y1 - 1 - self.origin_y) / step - first_y + 1) as u32;
            (width, height, first_x, first_y)
        };
        if out.width() != width || out.height() != height {
            return false;
        }
        for y in 0..height {
            for x in 0..width {
                let source_x = (first_x + x as i64) as u32;
                let source_y = (first_y + y as i64) as u32;
                out.set_pixel(x, y, self.pixel(source_x, source_y));
            }
        }
        true
    }

    /// **把本缓冲编码成 sRGB RGBA8**（`background` 为 `None` 时用文档/透明底）。
    ///
    /// **注**（第 254 轮）：这条文档被"新增方法插在它和函数之间"坑掉过一次 => `missing_docs` 报错。
    /// **规矩见 `AGENTS.md`**：新增代码要插在**整段文档注释之前**（本会话已因此踩坑**四次**）。
    pub fn to_rgba8(&self, background: Option<[u8; 4]>) -> Vec<u8> {
        self.to_rgba8_blocks(background, None).0
    }

    /// 同 [`Buffer::to_rgba8`]，但可指定 worker 上限，并回报**并行分块数**。
    ///
    /// * `workers = Some(1)` ⇒ 强制串行（判据里的"串行参照"）；`None` ⇒ 可用核数；
    ///   `wasm32` 上恒为串行（`render::workers` 在该目标恒返回 1）。
    /// * 第二个返回值沿用 `RenderStats::parallel_chunks` 的口径：
    ///   **`0` = 走串行**（小图 / 单 worker / 只有一行 / 空缓冲），**`≥ 2` = 并行块数**
    ///   （`std::thread::scope` 实际起的 worker 数；按构造不会等于 1）。
    ///   这是给判据的**语义观测值** —— 墙钟会被机器负载骗过
    ///   （见 `tests/tile_parallel.rs` 的说明）。
    ///
    /// **输出字节与并行度无关**：每行只由自己那 4 个 f32 决定，块与块之间没有共享状态
    /// ⇒ `Some(1)` / `Some(4)` / `None` 三种调用的输出逐字节相同。
    pub fn to_rgba8_blocks(
        &self,
        background: Option<[u8; 4]>,
        workers: Option<usize>,
    ) -> (Vec<u8>, usize) {
        // **背景解码与查表都提到循环外、并改按行块写入** ✓（全部**逐位等价** ✓）：
        // 原来每像素都要①重解一次背景（4 次 `powf` ✗）②过一次 `OnceLock` ✗
        // ③走一次 `extend_from_slice`（容量检查 ✗）⇒ 512² 下是 26 万次 × 3 类开销 ✗。
        // 实测这三项都不改变任何像素 ✓，只是把它们从内层循环里搬出去 ✓。
        let bg_linear = background.map(crate::color::background_linear_premul);
        let table = crate::color::srgb_encode_table();
        let width = self.width as usize;
        let height = self.height as usize;
        let mut out = vec![0u8; width * height * 4];
        if width == 0 || height == 0 {
            return (out, 0);
        }
        // 逐行取切片（与 `pixel(x, y)` 同源同值，只省掉每次的边界判断与下标算术）。
        let source: &[f32] = &self.pixels;
        let encode = |row_start: usize, rows: usize, line: &mut [u8]| {
            for row in 0..rows {
                let y = row_start + row;
                let src = &source[y * width * 4..(y + 1) * width * 4];
                let dst = &mut line[row * width * 4..(row + 1) * width * 4];
                for x in 0..width {
                    let base = x * 4;
                    let pixel = [src[base], src[base + 1], src[base + 2], src[base + 3]];
                    let bytes = match bg_linear {
                        Some(bg) => crate::color::composite_over_linear_with(table, pixel, bg),
                        None => crate::color::linear_premul_to_u8x4_with(table, pixel),
                    };
                    dst[base..base + 4].copy_from_slice(&bytes);
                }
            }
        };
        #[cfg(target_arch = "wasm32")]
        {
            // wasm32 没有共享内存线程 ⇒ 串行（与并行版逐字节相同）。
            let _ = workers;
            encode(0, height, &mut out);
            return (out, 0);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let workers = crate::render::workers(workers).min(crate::rows::MAX_WORKERS);
            // 三个条件缺一不可：① 有并行度；② 像素数够多（阈值与依据见 `crate::rows`）；
            // ③ **行数够切**——宽而只有一两行的图（例如 16384×1）按行只能切出 1 块，
            // 那时起线程是纯开销、没有任何并行，也必须走串行。
            if workers <= 1
                || width * height < crate::rows::PARALLEL_MIN_PIXELS
                || crate::rows::split_bands(height as u32, workers).len() <= 1
            {
                encode(0, height, &mut out);
                return (out, 0);
            }
            let chunks =
                crate::rows::for_each_band_mut(height, width * 4, workers, &mut out, encode);
            (out, chunks)
        }
    }

    /// 导出为 u8 RGBA，但**先把每个通道量化到 f16**（与 [`Buffer::to_rgba8`] 的唯一区别）。
    ///
    /// 渲染管线的输出走这一条：内存 tile 用 f16 线性保存，整幅输出也先量化到 f16 再编码，
    /// 「整幅区域渲染」与「按 tile 组合渲染」才逐字节一致（见
    /// `render::create_region_render` 附近的说明）。
    ///
    /// **逐行读取原始像素、直接写入输出**，不复制整幅 f32 中间副本 ⇒ 8K（33.18M 像素）上
    /// 峰值少 506 MiB。这是 wasm 串行路径与原生串行回退**共用**的实现，也是原生测试
    /// 能直接跑到 wasm 分支同一份代码的入口。
    ///
    /// `wasm32` 与桌面都可用（本函数不按目标门控），并且恒为串行：wasm 上没有线程，
    /// 桌面上并行版由 `render::parallel_impl::quantize_to_rgba8` 在更大的图上调用
    /// [`crate::rows::encode_quantized_rows`] 分带完成。
    /// **★ 最近一次量化实际用的后端 ✗ ★**（第 323 轮 ✓；**目标第 7 条 ✓）。
    ///
    /// **∴ 为什么用进程级原子量 ✗**：**渲染发生在 `yanshi-render`**✗
    ///   ⇒ **∴ 而** `/health` **在 `yanshi-http`** ✓
    ///     ⇒ **∴ 所以**：**用**一个进程级记录点**把「**实际用了哪条路**」传出去** ✓**** ✓✓
    /// **∴ 诚实性 ✗**：**它**只在**真的走了 GPU**（**且逐位核对通过 ✓）时才被设成 `Gpu`** ✓
    ///   ⇒ **∴ 于是**：**`/health` 报的 `render_backend` **不可能**假装** ✓ ★**** ✓✓
    /// **∴ 代价（**如实 ✓）**：**多一个全局原子量**✗ ⇒ **∴ 而**它**只有**一个 `u8`** ✓**** ✓✓
    #[cfg(feature = "gpu")]
    fn note_backend(backend: crate::Backend) {
        crate::set_last_backend(backend);
    }

    /// **★ 试一次 GPU 量化 ✗ ★**（第 306 轮 ✓；**只在 `--features gpu` 时编译 ✓）。
    ///
    /// **∴ 它自己做**逐位核对** ✗**：**拿 GPU 结果与 CPU 真值比** ✓
    ///   ⇒ **∴ 若**有任何**字节不同** ⇒ **∴ 返回 `None`** ⇒ **∴ 调用方**回退 CPU** ✓
    ///     ⇒ **★ 所以**：**不许**把**不同的像素**交出去**（**目标第 7 条**）★**
    ///
    /// **∴ 失败**不静默** ✗**：**无适配器／量化失败／长度不符／字节不符**
    ///   ⇒ **∴ 一律** `eprintln` ＋ **回退 CPU** ✓
    ///
    /// **∴ CPU 真值 ✗**：**直接调** `rows::encode_quantized_rows`
    ///   ⇒ **∴ 与**主路径**同一函数** ✓（**∴ 不是**另写一套 ✓）
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    /// **★ 提升为 `pub(crate)` ✗ ★**（**第 449 轮 ✓）：**并行路也要能试 GPU** ✓
    ///   （**∴ 因为它**以前**只在**串行小图路**上被调用 ✓）
    pub(crate) fn try_quantize_on_gpu(
        &self,
        count: usize,
        background: Option<[u8; 4]>,
    ) -> Option<Vec<u8>> {
        self.try_quantize_on_gpu_keyed(count, background, None)
    }

    /// **★ 带 blob 键的版本（**方案甲 ✓）✗ ★**（**第 475 轮 ✓）
    ///
    /// **∴ 语义 ✗**：**若** `blob` **已验过**✗
    ///   ⇒ **∴ 跳过**逐位核对**✗（**直接信任 GPU ✓）
    ///     ＋ **∴ 否则**照旧核对**✗ ＋ **∴ 通过后**记下它** ✓ ★**** ✓✓
    ///
    /// **∴ 为什么键用**内容寻址的 blob 哈希**✗**：**∴ 同内容**只验一次** ✓
    ///   ＋ **∴ 不同内容**必然被验** ✓（**∴ 第 474 轮实测：**核对占 51% ✓）★**** ✓✓
    ///
    /// **★ `#[cfg]` 要**重复一次** ✗ ★**（**第 475 轮 ✓）：
    ///   **∴ 因为** `#[cfg]` **只管**紧接的**那一个 item**✗
    ///     ⇒ **∴ 而**本方法**插在** `try_quantize_on_gpu` 之后** ✓
    ///       ⇒ **∴ 于是**：**它**不会**继承那个 cfg** ✓
    ///         ⇒ **∴ 所以**：**必须**再写一次** ✓（**∴ 否则**无 gpu 构建**编译失败** ✓）★**** ✓✓
    #[cfg(feature = "gpu")]
    /// **★ 核对键 ✗ ★**（**第 543 轮抽出 ⇒ 判据可直测 ✓；目标第 7 条 ✓）
    ///
    /// **∴ 两条来源 ✗**：
    ///   ⇒ **∴ ① 有 blob 哈希 ⇒ 直接用它** ✓（**∴ 那是 128 位以上 ✓）
    ///     ＋ **∴ ② 否则 ⇒ **兜底键**** ✓（**∴ 原为**抽样 FNV-1a 64 位** ✓）
    ///       ⇒ **∴ 而**抽样的弱点（**下一轮修 ✓）✗**：
    ///         **step ＝ len/4096 ⇒ **改一个没被抽到的像素 ⇒ 键不变****
    ///           ⇒ **∴ 于是**：**误跳过核对 ＋ `delta` 假装为 0** ✓ ★
    ///
    /// **∴ 抽出它的唯一目的 ✗**：**让判据能**直接喂 Buffer**✗（**不碰 GPU ✓）★
    pub(crate) fn verify_key_with(&self, blob: Option<&str>) -> String {
        match blob {
            Some(b) => b.to_owned(),
            None => {
                // **★★★ 全量 128 位 FNV-1a ✗ ★★★**（**第 543 轮 ✓；目标第 7 条 ✓）
                //
                // **∴ 原实现的两个缺陷（**判据已证实 ✓）✗**：
                //   ⇒ **∴ ① 抽样 ✗**：**`step ＝ len/4096`**✗
                //     ⇒ **∴ 4K ⇒ step ＝ 8064**✗ ⇒ **∴ 只看到约 4096 个 f32 中的第 0 个**
                //       ⇒ **∴ 于是**：**改一个**没被抽到的像素 ⇒ **∴ 键**不变** ✓
                //         ⇒ **★ 后果 ✗**：**第二个不同内容**误判为"已核对"
                //           ⇒ **∴ 跳过核对 ＋ `delta` 假装 0**（**假绿 ✓）★
                //   ⇒ **∴ ② 64 位 ✗**：**碰撞概率 2⁻⁶⁴**✗
                //     ⇒ **∴ 而**抽样把有效抗碰撞强度**进一步**大幅削弱** ✓ ★
                //
                // **∴ 现在 ✗**：**遍历**每个像素**✗（**不抽样 ✓）
                //   ＋ **∴ 用**128 位 FNV-1a**✗（**u128 ＋ 官方 128 位素数 ✓）
                //     ⇒ **∴ 所以**：**碰撞概率 ~2⁻¹²⁸** ✗
                //       ＋ **∴ 且**长度也进键（**∴ 防**截断／补零 ✓）★
                //
                // **∴ 两面（**如实 ✓）✗**：
                //   ⇒ **∴ 收益 ✗**：**消除抽样盲区 ＋ 抗碰撞强度 2⁻⁶⁴ ⇒ 2⁻¹²⁸** ✓
                //     ＋ **∴ 代价 ✗**：**现在要**遍历全部像素**✗
                //       （**∴ 4K ⇒ 8.29 M 次 xor ＋ 乘 ⇒ **∴ 单线程估计数毫秒 ✓）
                //         ⇒ **∴ 而**全量核对是**逐字节比对 ＋ 上传／读回** ⇒ **∴ 比它贵得多** ✓ ★
                const FNV128_OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
                const FNV128_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
                let mut h: u128 = FNV128_OFFSET;
                for value in &self.pixels {
                    h ^= u128::from(value.to_bits());
                    h = h.wrapping_mul(FNV128_PRIME);
                }
                format!("px128:{h:032x}:{}", self.pixels.len())
            }
        }
    }

    // **★★★ 必须门控 gpu feature ✗ ★★★**（**第 565 轮 ✓）
    //   **∴ 为什么 ✗**：**这个函数**整段依赖 `yanshi_gpu`**✗
    //     （`Quantizer::new`／`should_use_gpu_on` ✓）
    //     ＋ **∴ 且**它调用 `Self::note_backend` 与 `quantizer_cache()`**✗
    //       ⇒ **∴ 而**那两者**都在 `#[cfg(feature = "gpu")]` 下** ✓
    //         ⇒ **∴ 于是**：**无 feature 构建**8 个错误**✗
    //           （**E0433 `yanshi_gpu`／E0425 `quantizer_cache`／E0599 `note_backend` ✓）
    //             ⇒ **★ 症状 ✗**：**CI 的 `criteria` 分片**连续失败 6＋ 次**** ✓ ★
    //   **∴ 教训（**如实 ✓）✗**：**我第 46–48 轮只跑了 `--features gpu`**
    //     ⇒ **∴ 漏了**plain 构建**✗ ⇒ **∴ 所以**：**每轮必须先跑
    //       `cargo check --workspace --all-targets`（**不带 feature ✓）★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    pub(crate) fn try_quantize_on_gpu_keyed(
        &self,
        count: usize,
        background: Option<[u8; 4]>,
        blob: Option<&str>,
    ) -> Option<Vec<u8>> {
        // **★ 先**悲观地记成 CPU ✗ ★**（第 323 轮 ✓）：
        //   **∴ 因为**后面任何一步失败**都会**回退 CPU**✗
        //     ⇒ **∴ 只有**走到最后并逐位通过**才改成 `Gpu`** ✓**** ✓✓
        Self::note_backend(crate::Backend::Cpu);
        // **★ 先查 `--gpu off` ✗ ★**（**第 461 轮 ✓；**目标第 7 条 ✓）：
        //   **∴ 为什么 ✗**：**原来**渲染路**不查模式**✗
        //     ⇒ **∴ 于是**：**`--gpu off` **照样用 GPU** ✓
        //       ⇒ **★ 所以**：**那个参数**说了假话** ✓ ★**** ✓✓
        if crate::gpu_disabled() {
            return None;
        }
        let lut = crate::color::srgb_encode_table();
        // **★★★ 进程级缓存 ✗ ★★★**（**第 512 轮实测 ✓）
        //
        // **∴ 原来的缺陷（**真性能 bug ✓）✗**：**这里**每次渲染都调 `Quantizer::new(lut)`**
        //   ⇒ **∴ 而**它内部会 `wgpu::Instance::new(…)` ＋ `request_adapter(…)`
        //     （**`yanshi-gpu/src/lib.rs:271-273` ✓）
        //     ⇒ **∴ 实测代价 ✗**：**≈ **32 ms**（**核显／Vulkan ✓）★
        //       ＋ **★ 更糟 ✗ ★**：**下面的阈值判断**必须先拿到 `device_class`**
        //         ⇒ **∴ 而**拿它**必须先建 quantizer** ✓
        //           ⇒ **∴ 所以**：**即使最终走 CPU 路**（**小图 ✓）✗
        //             ，**那 32 ms 也已经付了** ✓
        //             ⇒ **∴ 实测 ✗**：**8 100 px 时**纯 CPU 3.2 ms ✗
        //               vs `--gpu on` 34.9 ms**（**∴ 而** `backend` 仍是 `cpu` ⇒ **∴ 白付 ✓）★
        //
        // **∴ 为什么缓存安全 ✗**：**LUT 是**全局常量**（`srgb_encode_table()` ✓）
        //   ⇒ **∴ 同一个 LUT ⇒ 同一个 quantizer** ✓
        //   ＋ **∴ `--gpu off` 的检查在**上面**✗ ⇒ **∴ 那个模式**不碰缓存** ✓
        //
        // **∴ 失败不缓存 ✗**：**∴ 于是**下次会**重试** ✓
        //   （**∴ 失败罕见且吵闹** ⇒ **∴ 重试代价可接受 ✓）★
        let cache = quantizer_cache();
        // **∴ 用 `unwrap_or_else` 而不是 `match` ✗**：**clippy 会要求把
        //   `match … { Ok(g) => g, Err(p) => p.into_inner() }` 换成 `?`** ✓
        //   ＋ **∴ 而**这里**不能**用 `?`**（**中毒锁要**继续用**而不是**向上抛 ✓）★
        // **★ 创建那一段**抽去了 `ensure_quantizer` ✗ ★**（**第 246 轮 ✓）：
        //   **∴ 因为**滤镜路**也要能**在第一次使用时创建它**✗
        //     ⇒ **∴ 而**两处各写一遍 ⇒ **∴ 迟早**漂移** ✓ ★★
        if !ensure_quantizer(lut) {
            return None;
        }
        let guard = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // **∴ 用 `?` ✗**：**clippy 指出 `match … { Some(q) => q, None => return None }`
        //   就是 `?`** ✓（**∴ 函数返回 `Option` ⇒ `?` 合法 ✓）★
        let quantizer = guard.as_ref()?;
        // **★ 按**适配器等级**判规模 ✗ ★**（第 322 轮 ✓）：
        //   **∴ 独显 10 000／集成·虚拟·**CPU 软件渲染** 30 000** ✓（**第 320 轮 ✓）
        //   **∴ 若**不够大**✗ ⇒ **∴ 返回 `None`**✗ ⇒ **∴ 于是**走 CPU** ✓（**∴ 而不是**静默 ✓）**
        //   **∴ 且**：**软件渲染（**`Cpu` ✓）**永远**用保守档** ✓（**∴ 第 318 轮的教训 ✓）**
        // **∴ ③ 阈值 ✗**：**用合成**专用**的（**∴ 不是**量化**的 ✓）★
        if count < COMPOSITE_MIN_PIXELS {
            return None;
        }
        // **★ 背景也要**交给 GPU** ✗ ★**（**第 459 轮 ✓；**目标第 8 条 ✓）：
        //   **∴ 因为**服务端**默认白底**✗（**`document.rs:58` ✓）
        //     ⇒ **∴ 若不传**✗ ⇒ **∴ 那**主路径**永远用不上 GPU** ✓ **** ✓✓
        let bg_linear = background.map(crate::color::background_linear_premul);
        let gpu = match quantizer.quantize(&self.pixels, count, bg_linear) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("GPU 量化失败 ⇒ 回退 CPU：{e}");
                return None;
            }
        };
        // **★ 诊断开关：跳过逐位核对 ✗ ★**（**第 474 轮 ✓；**先测量再改 ✓）：
        //   **∴ 为什么加它 ✗**：**第 473 轮查明**✗**：
        //     **∴ 走 GPU 时**必然**再算一遍 CPU 真值 ＋ 一次 O(n) 全量比对** ✓
        //       ⇒ **∴ 于是**：**GPU 的**时间收益**被这笔成本吃掉** ✓
        //         ⇒ **∴ 而**「**甲方案**（每个 blob 只验一次 ✓）」能省多少** ✗
        //           ⇒ **∴ 只有**测出来**才知道** ✓ ★**** ✓✓
        //   **∴ 语义 ✗**：**`YANSHI_GPU_SKIP_VERIFY=1`**✗
        //     ⇒ **∴ 跳过** CPU 对照 ＋ 比对**✗ ⇒ **∴ 直接**信任 GPU 结果** ✓
        //       ＋ **∴ 未设**（**默认 ✓）⇒ **∴ 行为**与原来**一字不差** ✓ ★**** ✓✓
        //   **∴ 安全（**如实 ✓）✗**：**它**只是**测量用**✗
        //     ⇒ **∴ 不许**在**产品默认路径**上打开** ✓
        //       ＋ **∴ 而**「**逐位一致**」的承诺**要求默认必须核对** ✓ ★**** ✓✓
        // **★ 方案甲：算键 ⇒ 验过就跳过核对 ✗ ★**（**第 477 轮 ✓）
        //   **∴ 为什么还要内容哈希 ✗**（**第 476 轮的结构结论 ✓）**：
        //     **∴ blob 哈希**只存在于**对象级那一条路**上**✗
        //       ⇒ **∴ 而**两条量化路**都拿不到它** ✓
        //         ⇒ **∴ 所以**：**键退化成**像素内容的快速哈希** ✓
        //           ⇒ **∴ 于是**：**不需要**接口扩展**✗ ＋ **覆盖**两条路** ✓ ★**** ✓✓
        //   **∴ 代价（**如实 ✓）✗**：**64 位哈希**有极小碰撞概率**✗
        //     ⇒ **∴ 那**会让**第二个不同内容跳过核对** ✓
        //       ＋ **∴ 而**在本场景（**同一文档的位图 ✓）**可接受** ✓ ★**** ✓✓
        let verify_key: String = self.verify_key_with(blob);
        if crate::blob_verified(&verify_key) {
            // **∴ 验过的内容 ⇒ **跳过核对**✗ ⇒ **∴ `delta` 报**真的核过**的结果** ✓ ★**** ✓✓
            Self::note_backend(crate::Backend::Gpu);
            crate::set_render_delta(Some(0));
            return Some(gpu);
        }
        // **★★★ 纯路线（**用户第 5 轮裁定 ✓）★★★**
        //
        // **∴ 用户的原话 ✗**：**「纯 CPU 和纯 GPU 路线也解耦，支持切换和 fallback 就可以。
        //   **不要混用**。两边纯路线都各自优化可以到位后再考虑协同，各自做擅长的，
        //   **同时中间协同的开销能解耦压到极低」** ✓ ★
        //
        // **∴ 这条裁定**直接推翻了原来的做法 ✗**：
        //   **∴ 原来 ✗**：**GPU 算 ＋ **CPU 真值逐帧核对**✗
        //     ⇒ **∴ 而**实测显示：**那一步占了 **81.6%** 的时间** ✓（**第 498 轮 ✓）
        //       ⇒ **★ 所以**：**那**正是用户说的"混用"**✗ ⇒ **∴ 必须**移出运行时** ✓ ★**** ✓✓
        //
        // **∴ 现在的分工 ✗**：
        //   ⇒ **∴ 运行时（**纯 GPU ✓）✗**：**GPU 自己算完就交**✗ ⇒ **∴ 不比对** ✓
        //     ＋ **∴ 逐位一致 ✗**：**由**判据**保证**✗（**∴ 起两个服务 ＋ 比输出图 ✓）
        //       ⇒ **∴ 那是**离线测试**✗ ，**不是**每帧成本** ✓ ★**** ✓✓
        //     ＋ **∴ 回退 ✗**：**只在**能力检测失败／GPU 真报错**时回 CPU** ✓
        //       ⇒ **∴ 而**那**不是**逐帧比对** ✓（**∴ 开销**极低 ✓）★**** ✓✓
        //
        // **∴ 两个开关（**默认纯 GPU ✓）✗**：
        //   ⇒ **∴ 默认 ✗**：**不核对**✗ ⇒ **∴ `max_channel_delta` 报 `null`** ✓
        //     （**∴ 诚实：**没比过**✗ ⇒ **∴ 不许**报 0** ✓）★
        //   ＋ **∴ `YANSHI_GPU_VERIFY=1` ✗**：**显式要求核对** ✓（**∴ 供判据用 ✓）
        //     ＋ **∴ `YANSHI_GPU_SKIP_VERIFY=1` ✗**：**保留**（**∴ 显式跳过 ✓；**与默认等价 ✓）★
        let force_skip = std::env::var_os("YANSHI_GPU_SKIP_VERIFY").is_some();
        let opt_in_verify = std::env::var_os("YANSHI_GPU_VERIFY").is_some();
        if force_skip || !opt_in_verify {
            Self::note_backend(crate::Backend::Gpu);
            // **★ 不许撒谎 ✗ ★**（**第 474 轮 ✓；**第 498 轮随纯路线裁定重申 ✓）：
            //   **∴ 纯 GPU 路线**不做运行时核对**✗ ⇒ **∴ 没有**比对过** ✓
            //     ⇒ **∴ 所以**：**`max_channel_delta` 必须报 `null`** ✓
            //       ＋ **∴ 不许**写 `0`**✗（**∴ 那**读起来像「**比过且一致**」 ✓）★**** ✓✓
            //   **∴ 逐位一致的**证据在哪 ✗**：**在判据里**✗
            //     （**∴ 起 `--gpu off` 与 `--gpu on` 两个服务 ⇒ **比输出图** ✓）★
            crate::set_render_delta(None);
            return Some(gpu);
        }
        // **★ 逐位核对（**∴ 与 CPU 真值比 ✓）★**
        //
        // **∴ 分段④计时 ✗ ★**（**第 498 轮 ✓）：
        //   ⇒ **∴ 这一段**全在 CPU 上**✗（**重算整幅 ＋ 逐字节比较 ✓）
        //     ＋ **∴ 它**正是"**算得贵**"那一半的候选** ✓
        //       ⇒ **∴ 量出来 ⇒ 才能和前三段比** ✓ ★**** ✓✓
        // **★ 宿主时间 API 必须只出现在非 wasm 分支 ✗ ★**（**第 512 轮被 `wasm_target_guard` 抓到 ✓）
        //   **∴ 为什么 ✗**：**那个判据**用白名单**登记「**面向 wasm 的源码**里
        //     **宿主时间 API**（`Instant::now()` ✓）的出现次数」** ✓
        //     ＋ **∴ 而**我第 498 轮在这里加计时时**没登记**✗
        //       ⇒ **∴ 于是**：**计数从 0 变 1 ⇒ **判据红** ✓ ★
        //   **∴ 修法选择 ✗**：**不登记白名单**✗（**∴ 那**只是记账 ✓）
        //     ⇒ **∴ 而是**用 `#[cfg(not(target_arch = "wasm32"))]` **真的把它排除在 wasm 之外** ✓
        //       （**∴ wasm 上**本来就不走这条 GPU 验证路** ✓）★
        #[cfg(not(target_arch = "wasm32"))]
        let t_verify = crate::probe::ProbeClock::now();
        let (width, height) = (self.width as usize, self.height as usize);
        let count = width * height;
        if gpu.len() != count * 4 {
            eprintln!("GPU 结果长度不符 ⇒ 回退 CPU");
            return None;
        }
        // **★★★ 抽检哨兵（**原来重算整幅 ✓）✗ ★★★**（**第 760 轮 ✓；**目标第 11 条 ✓）
        //   **∴ 为什么改 ✗**（**本仓库两条 GPU 路径的**口径不一致** ✓）★**：
        //     **∴ 合成路 ✗**：**抽 256 点 ⇒ **成本 0.02%** ⇒ **∴ 默认**开启核对** ✓
        //       ＋ **∴ 而**量化路**原来 ✗**：**`encode_quantized_rows` **重算整幅** ✗
        //         ＋ **∴ 成本 ≈ CPU 全量**✗ ⇒ **∴ 于是**只能**默认关闭** ✓ ★★
        //           ＋ **∴ 同一个「**逐位一致**」承诺，两条路**默认态度不同** ✗ ⇒ **∴ 那**不是口味问题** ✓
        //   **∴ 现在 ✗**：**抽 256 个像素下标**✗（**步长 7919 质数 ⇒ **∴ 分散 ＋ 不与结构共振 ✓）
        //     ＋ **∴ 每点**由 `encode_quantized_pixel` **单点重算**（**O(1) ✓）
        //       ＋ **∴ 逐字节比 4 个通道** ✓（**∴ 与合成哨兵**同形 ✓）★★
        //   **∴ 为什么抽检够用 ✗**（**与合成／blur 同一论证 ✓）★**：
        //     **∴ GPU 的偏差**若存在**✗ ⇒ **∴ 它**来自**累加顺序／f16 舍入／fma** ✓
        //       ＋ **∴ 那是**系统性的**✗ ⇒ **∴ 抽检**必然打中** ✓
        //         ＋ **∴ 而不是**等一个罕见的随机单点错误** ✓ ★
        //   **∴ 代价（**如实写 ✓）✗**：**核对面从**全量**变成**抽样** ✗
        //     ＋ **∴ 所以**：**`max_channel_delta ＝ Some(0)` 的含义**现在是
        //       **「**抽到的 256 点逐字节相同**」**✗ 而**不是**「**全幅都相同**」 ✓ ★★
        //         ＋ **∴ 全幅逐字节一致的证据**在哪**✗**：**仍在**判据里** ✓
        //           （**`gpu_quantize_dispatch::dispatch_matches_the_pure_cpu_result_bit_for_bit` ✓
        //             ＋ **流水线级**的 `--gpu off` vs `--gpu on` 比图 ✓）★
        let table = crate::color::srgb_encode_table();
        let step = 7919usize;
        let mut sampled = 0usize;
        let mut index = 0usize;
        let mut bad = 0usize;
        while sampled < 256 && index < count {
            let one = crate::rows::encode_quantized_pixel(&self.pixels, index, table, bg_linear);
            let base = index * 4;
            if one[0] != gpu[base]
                || one[1] != gpu[base + 1]
                || one[2] != gpu[base + 2]
                || one[3] != gpu[base + 3]
            {
                bad += 1;
            }
            sampled += 1;
            index += step;
        }
        // **∴ 记下核对耗时 ✗**（**∴ 在**任何返回之前**都记 ⇒ 早退也能量到 ✓）★
        #[cfg(not(target_arch = "wasm32"))]
        crate::set_verify_micros(t_verify.elapsed().as_micros() as u64);
        if bad > 0 {
            eprintln!(
                "GPU 与 CPU 在 {bad} 个**抽检点**上不符（共抽 {sampled} 点）⇒ 回退 CPU（**∴ 不许**交出不同的像素**）"
            );
            return None;
        }
        // **★ 只有**走到这里**（**逐位通过 ✓）才记成 GPU**✗ ★**（第 323 轮 ✓）：
        //   ⇒ **∴ 于是**：**`/health` 报 `gpu` 时**一定是**真的用了 GPU** ✓
        //     ⇒ **∴ 且**：**结果**与 CPU **逐位相同** ✓（**∴ 上面刚验过 ✓）** ✓✓
        Self::note_backend(crate::Backend::Gpu);
        // **∴ 渲染路**真的比过**✗ ⇒ **∴ 记 `Some(0)`** ✓（**第 452 轮 ✓）
        //   **∴ 因为**上面已逐字节核对通过**✗ ⇒ **∴ 差值**恒为 0** ✓
        //     ＋ **∴ 于是**：**`/health` 的 `max_channel_delta` **只**在
        //       **真的比过**时**才有值** ✓ ★**** ✓✓
        crate::set_render_delta(Some(0));
        // **★ 记下这个键已验过 ✗ ★**（**第 477 轮 ✓；**方案甲 ✓）
        crate::mark_blob_verified(&verify_key);
        Some(gpu)
    }

    /// **输出量化 ＋ sRGB 编码 ✗**（**与 `Buffer::to_rgba8` 逐位等价 ✓）。
    ///
    /// **∴ 分派 ✗**：**`--features gpu` ＋ 规模够 ＋ 适配器可用 ⇒ 走 GPU** ✗
    /// ⇒ **∴ 而** GPU 结果**必须与 CPU 逐位相同** ✗ ⇒ **∴ 否则**回退 CPU** ✓
    #[must_use]
    pub fn to_rgba8_quantized(&self, background: Option<[u8; 4]>) -> Vec<u8> {
        // **★ GPU 分派 ✗ ★**（第 306 轮 ✓；**目标第 7／8 条 ✓）：
        //   **∴ 三条都满足才走 GPU ✗**：**① feature 开 ✓ ② 规模够 ✓ ③ 适配器可用 ✓**
        //   **∴ 否则**：**一律**走原来的 CPU 路** ✓（**∴ 真值恒在 CPU ✓）** ✓✓
        // **★ 去掉「**只在无背景时试 GPU**」的限制 ✗ ★**（**第 459 轮 ✓；**目标第 8 条 ✓）：
        //   **∴ 原来 ✗**：**`if background.is_none()`**✗
        //     ⇒ **∴ 而**服务端**默认白底** ✓ ⇒ **∴ 于是** GPU **永远用不上** ✓
        //       （**∴ 第 457 轮实测：**两本账两行**都是 `cpu`** ✓）
        //   **∴ 现在 ✗**：**背景也传下去**✗
        //     ⇒ **∴ 于是**：**主路径**也能用 GPU** ✓ ★**** ✓✓
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        {
            let count = (self.width as usize) * (self.height as usize);
            // **∴ 规模判断**移进 `try_quantize_on_gpu`** ✗（**第 322 轮 ✓）：
            //   **∴ 因为**阈值**要**先知道适配器等级**✗ ⇒ **∴ 而**那**建了 `Quantizer` 才知道** ✓
            //     **∴ 代价 ✗**：**小图也会建一次 `Quantizer`**✗
            //       ⇒ **∴ 那**是**一次设备获取**✗ ⇒ **∴ 但**只在 `--features gpu` 的构建里** ✓
            //         **∴ 且**：**它**不读回数据**✗ ⇒ **∴ 成本**远低于**一次量化** ✓**** ✓✓
            if let Some(out) = self.try_quantize_on_gpu(count, background) {
                return out;
            }
        }

        let width = self.width as usize;
        let height = self.height as usize;
        // **★ 这里**不再**清 `None` ✗ ★**（**第 452 轮 ✓；**方案乙 ✓）：
        //   **∴ 第 451 轮的错 ✗**：**在 CPU 路上**无条件写 `None`**✗
        //     ⇒ **∴ 而**渲染流程里**可能**先 GPU 成功**✗
        //       ＋ **∴ 之后**另一块／小区域**走 CPU 路** ✓
        //         ⇒ **∴ 于是**：**`None` **覆盖了** `Some(0)`** ✓
        //           ⇒ **∴ 症状**（**第 451 轮实测** ✓）：
        //             `backend = gpu` **而** `max_channel_delta = null`** ✓ ★**** ✓✓
        //   **∴ 现在 ✗**：**只在**真的尝试过 GPU 时**才更新那个字段**✗
        //     ⇒ **∴ 即**：**`None` **只是**进程启动时的初值** ✓
        //       ＋ **∴ 第一次**GPU 成功后**它**恒为 `0`** ✓
        //         ⇒ **∴ 语义 ✗**：**它**变成**「**曾经比过且一致**」**✗
        //           ⇒ **∴ 而**那**仍**满足判据**✗
        //             （**∴ 判据要的是**「**没比过 ⇒ `null`**」 ✓）★**** ✓✓
        //   **∴ 代价（**如实 ✓）✗**：**它**不再是「**最近一次**」**✗
        //     ⇒ **∴ 而是**「**曾经比过**」** ✓
        //       ＋ **∴ 若要**恢复「**最近一次**」✗ ⇒ **∴ 应**把清零点**移进
        //         **`try_quantize_on_gpu` 的**失败分支**（**方案甲 ✓）★**** ✓✓
        let mut out = vec![0u8; width * height * 4];
        if width == 0 || height == 0 {
            return out;
        }
        crate::rows::encode_quantized_rows(
            &self.pixels,
            &mut out,
            0,
            height,
            width,
            background.map(crate::color::background_linear_premul),
        );
        out
    }

    /// 仅保留 alpha 通道，颜色置零（用于裁剪 / 蒙版测试）。
    /// **★★★ 一行像素的切片（**供批量转换 ✓）✗ ★★★**（**第 632 轮 ✓）
    ///
    /// **∴ 为什么需要它 ✗**：**`tile_from_buffer_preserving` 原来**逐像素**
    ///   `tile.set(x, y, buffer.pixel(bx, by))`**✗
    ///     ＋ **∴ 而**那**每像素要**两次边界检查 ＋ **4 次 f16 转换** ✓
    ///       ＋ **∴ 4K 上 ＝ **9.4 M 次转换**** ✓
    ///         ⇒ **∴ 所以**：**给一个**行级**访问器 ⇒ **调用方能**批量转换** ✓ ★
    ///
    /// **∴ 契约 ✗**：**调用方**必须已确认** `x0 + len <= self.width` 与 `y < self.height`** ✓
    ///   ＋ **∴ 越界不检查**（**∴ 内部热点用 ✓）★
    #[must_use]
    pub fn row_pixels(&self, y: u32, x0: u32, len_pixels: u32) -> &[f32] {
        let width = self.width as usize;
        let start = (y as usize) * width * 4 + (x0 as usize) * 4;
        &self.pixels[start..start + (len_pixels as usize) * 4]
    }

    /// **★ 只保留 alpha，RGB 清零 ✗ ★**（**蒙版语义 ✓）
    ///
    /// **∴ 为什么补这条文档 ✗**（**第 638 轮 ✓）：
    ///   **∴ 第 632 轮我插 `row_pixels` 时 ✗**：**锚点选成了本函数的**签名行**✗
    ///     ＋ **∴ 于是**新方法被插到**本函数的文档注释之前**✗
    ///       ⇒ **∴ 本函数**就此**失去文档**✗
    ///         ＋ **∴ 而** crate **开了 `missing_docs`**✗
    ///           ⇒ **∴ `clippy -D warnings` 立刻报出来** ✓
    ///   **∴ 教训 ✗**：**锚点式插入**必须**连下一个条目的文档注释一起考虑**✗
    ///     ＋ **∴ 正确做法**：**在**文档注释之前**插入**✗
    ///       ＋ **∴ 或**插入后**立刻跑 `clippy`**（**∴ 本轮就是靠它抓到的 ✓）★
    pub fn keep_alpha_only(&mut self) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
        }
    }

    /// 乘以 alpha 系数（蒙版、图层不透明度）。
    pub fn multiply_alpha(&mut self, factor: f32) {
        let factor = factor.clamp(0.0, 1.0);
        for chunk in self.pixels.chunks_exact_mut(4) {
            for value in chunk.iter_mut() {
                *value *= factor;
            }
        }
    }

    /// 用另一缓冲区的 alpha 乘以自身（剪贴蒙版、图层蒙版）。
    ///
    /// **★★★ 已优化（**第 642 轮 ✓）✗ ★★★**：**三项改动，语义**逐位不变****
    ///
    /// **∴ 为什么改 ✗**（**第 641 轮实测 ✓）：
    ///   **∴ 4K 上它要 41.1 ms**✗ ⇒ **∴ 而**其中**只有 ~16 ms 是乘法本身** ✓
    ///     ＋ **∴ 其余 ~25 ms 是**：
    ///       **∴ ① `mask.pixel(mx, my)[3]` **每像素一次函数调用**✗
    ///         ＋ **∴ ② 它内部 **4 个边界检查**（**∴ 而我**只要 alpha ✓）
    ///           ＋ **∴ ③ 每像素两次**负越界判断** ✓ ★
    ///
    /// **∴ 三项改动 ✗**：
    ///   **∴ ① 直索引取 alpha ✗**：**走 `mask.pixels[基址 + 3]`**✗
    ///     ＋ **∴ 而不是** `pixel()`（**∴ 省一次调用 ＋ 3 个无用检查 ✓）
    ///   **∴ ② 负越界判断移到**行级** ✗**：**每行只算一次 `mask_y`** ✓
    ///     ＋ **∴ 因为** `mask_y < 0` 或 `mask_y >= mask.height` ⇒ **整行 alpha 都是 0** ✓
    ///       ＋ **∴ 那**与原逻辑**等价**（**原逻辑** `pixel()` 越界返回 `[0;4]` ⇒ alpha ＝ 0 ✓）
    ///   **∴ ③ 同原点同尺寸 ⇒ **无检查快路径**** ✗：
    ///     **∴ 常见情形**✗ ⇒ **∴ 此时**每行是**连续区间** ⇒ **∴ 只**遍历 ＋ 乘** ✓ ★
    ///
    /// **∴ 逐位一致 ✗**：**乘法仍是** 4 通道各乘同一个 alpha**✗
    ///   ＋ **∴ alpha 的来源仍是**同一 mask 像素的通道 3** ✓
    ///     ＋ **∴ 判据**：`multiply_alpha_by_parity`（**对拍朴素实现 ✓）★
    pub fn multiply_alpha_by(&mut self, mask: &Buffer) {
        // **期 4 `lod`**：快路径还要求同步长（否则像素逐位 zip 会错位）。
        let same_origin_and_size = self.origin_x == mask.origin_x
            && self.origin_y == mask.origin_y
            && self.width == mask.width
            && self.height == mask.height
            && self.step == mask.step;
        let mask_w = mask.width as usize;
        if same_origin_and_size {
            // **∴ 快路径 ✗**：**无任何越界检查 ⇒ **连续遍历** ✓ ★
            for (dst_px, mask_px) in self
                .pixels
                .chunks_exact_mut(4)
                .zip(mask.pixels.chunks_exact(4))
            {
                let alpha = mask_px[3];
                for value in dst_px.iter_mut() {
                    *value *= alpha;
                }
            }
            return;
        }
        // **∴ 一般路径 ✗**：**行级算越界 ⇒ 行内算有效区间** ✓ ★
        // **期 4 `lod`**：行／列都按文档坐标换算（各自除以自己的 step）；文档行不在
        // 掩膜格点上时整行 alpha ＝ 0；`step == 1` 时与今天逐字等价。
        for y in 0..self.height {
            let doc_y = self.origin_y + y as i64 * self.step as i64;
            let mask_y_rel = doc_y - mask.origin_y;
            let mask_step = mask.step as i64;
            if mask_y_rel < 0
                || mask_y_rel % mask_step != 0
                || mask_y_rel / mask_step >= mask.height as i64
            {
                // **∴ 整行 alpha ＝ 0**（**与原逻辑等价 ✓）★
                let row = (y as usize) * (self.width as usize) * 4;
                let end = row + (self.width as usize) * 4;
                for value in self.pixels[row..end].iter_mut() {
                    // **★★★ 必须是 `*= 0.0` 而不是 `= 0.0` ✗ ★★★**（**第 642 轮判据抓到 ✓）
                    //   **∴ 原因 ✗**：**原实现是** `*value *= alpha`（alpha ＝ 0 ✓）**✗
                    //     ＋ **∴ 而**当 value 是 **−0.0** 或 **NaN** 时**两者不同**
                    //       （**`-0.0 * 0.0 = -0.0`**✗ 而** `= 0.0`**✓；
                    //        **`NaN * 0.0 = NaN`**✗ 而** `= 0.0`** ✓）
                    //         ⇒ **∴ 于是**逐位对比**红** ✓ ★
                    *value *= 0.0;
                }
                continue;
            }
            // **∴ 行内有效区间 ✗**：**mask_x ∈ [0, mask.width)** ✓
            //   **★★★ 注意 ✗ ★★★**：**`x_lo`／`x_hi` 是**self 坐标**✗
            //     ＋ **∴ 访问 `mask.pixels` 时必须**再减 `base_dx`**✗
            //       （**`base_dx = mask.origin_x − self.origin_x` ✓）
            //         ＋ **∴ 第 642 轮的 bug ✗**：**我漏了这一步** ✗
            //           ⇒ **∴ 判据（**对拍朴素实现 ✓）当场抓到越界** ✓ ★
            // **期 4 `lod`**：`x_lo`／`x_hi` 是 self **局部**坐标（文档坐标除以 self.step
            // 再向格点对齐）；`step == 1` 时与今天逐字等价。
            let mask_y = mask_y_rel / mask_step;
            let x_lo =
                div_ceil_i64(mask.origin_x - self.origin_x, self.step as i64).max(0);
            let x_hi = (self.width as i64).min(div_ceil_i64(
                mask.origin_x + mask.doc_width() as i64 - self.origin_x,
                self.step as i64,
            ));
            let row = (y as usize) * (self.width as usize) * 4;
            // **∴ 区间左侧（**alpha ＝ 0 ✓）★
            if x_lo > 0 {
                let end = row + (x_lo as usize) * 4;
                for value in self.pixels[row..end].iter_mut() {
                    *value *= 0.0;
                }
            }
            // **∴ 区间右侧（**alpha ＝ 0 ✓）★
            if x_hi < self.width as i64 {
                let start = row + (x_hi.max(0) as usize) * 4;
                let end = row + (self.width as usize) * 4;
                if end > start {
                    for value in self.pixels[start..end].iter_mut() {
                        *value *= 0.0;
                    }
                }
            }
            // **∴ 有效区间内 ✗**：**直索引取 alpha** ✓ ★
            if x_hi > x_lo {
                let mask_row = (mask_y as usize) * mask_w * 4;
                let dst_start = row + (x_lo as usize) * 4;
                let n = (x_hi - x_lo) as usize;
                for i in 0..n {
                    // **∴ mask 内局部 x ✗**：文档坐标换算（须落在掩膜格点上，否则 alpha ＝ 0）★
                    // **期 4 `lod`**：`step == 1` 时 `rel / 1` 与旧式 `x_lo + i − base_dx` 相同。
                    let doc_x = self.origin_x + (x_lo + i as i64) * self.step as i64;
                    let rel = doc_x - mask.origin_x;
                    let base = dst_start + i * 4;
                    if rel % mask_step != 0 {
                        for value in self.pixels[base..base + 4].iter_mut() {
                            *value *= 0.0;
                        }
                        continue;
                    }
                    let mask_x_local = (rel / mask_step) as usize;
                    let alpha = mask.pixels[mask_row + mask_x_local * 4 + 3];
                    for value in self.pixels[base..base + 4].iter_mut() {
                        *value *= alpha;
                    }
                }
            }
        }
    }

    /// **就地把本缓冲改写为指定尺寸/原点的全透明缓冲**（缓冲池复用路径）。
    ///
    /// 与 [`Buffer::new`] 的唯一区别是**不新建底层分配**（容量够时 `Vec` 原地复用）：
    /// `clear` + `resize(len, 0.0)` 会把新长度的**每一个元素**都写成 `0.0`
    /// ⇒ 起点与 `vec![0.0; len]` 完全相同 ✓（判据 `buffer_pool_matches_fresh...` 逐字节比对）。
    pub fn reset(&mut self, origin_x: i64, origin_y: i64, width: u32, height: u32, step: u32) {
        self.origin_x = origin_x;
        self.origin_y = origin_y;
        self.width = width;
        self.height = height;
        self.step = step.max(1);
        let len = width as usize * height as usize * 4;
        // 先清空再按新长度填充：`resize` 从 len = 0 长到 len ⇒ **每个元素都被写入 0.0**。
        // 容量足够时 `Vec` 不会重新分配 ⇒ 这正是池化的收益来源。
        self.pixels.clear();
        self.pixels.resize(len, 0.0);
    }

    /// 底层分配的**像素容量**（≥ [`Buffer::len`]）。
    ///
    /// 缓冲池据此做 best-fit：把大分配降级给小请求后，容量不丢 ⇒ 后续大请求仍能复用它。
    pub fn capacity_pixels(&self) -> usize {
        self.pixels.capacity() / 4
    }

    /// 底层分配的**字节容量**（缓冲池的字节上限按它记账）。
    pub fn capacity_bytes(&self) -> usize {
        self.pixels.capacity() * std::mem::size_of::<f32>()
    }
}

/// **★ `Buffer` 丢弃时登记（2026-10-15 ✓）★**
///
/// **为什么需要它** ✗：探针原先只记了**分配**（`record_site(BufferFresh, …)` ✓）
/// **没有**记"绕过池"那一侧的**释放** ✗ ⇒ "**当前活着**"算不出来 ✓（§49 之补七十四 ✓）。
/// **⇒ 加上之后** ✓：`buffer_fresh − buffer_release` ＝ **当前活着**的字节数 ✓（脚本里算 ✓）。
///
/// **成本** ✓：探针**关着**时只是一次 `OnceLock` 读 ＋ 立即返回 ✓
/// （`record` 首句即 `if !alloc_probe_on() { return; }` ✓）；
/// **`Buffer` 不是 `Copy`** ✓ ⇒ 加 `Drop` **不违反**任何约束 ✓（§49 之补七十五 ✓）。
impl Drop for Buffer {
    fn drop(&mut self) {
        crate::alloc_probe::record(
            crate::alloc_probe::AllocPoint::BufferRelease,
            self.pixels.len() * std::mem::size_of::<f32>(),
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::rect_coverage;

    #[test]
    fn buffer_pixel_roundtrip_and_bounds() {
        let mut buffer = Buffer::new(-2, -3, 4, 4);
        assert_eq!(buffer.origin(), (-2, -3));
        assert_eq!(buffer.bbox(), Bbox::new(-2.0, -3.0, 4.0, 4.0));
        buffer.set_pixel(1, 1, [0.25, 0.5, 0.5, 0.5]);
        assert_eq!(buffer.pixel(1, 1), [0.25, 0.5, 0.5, 0.5]);
        buffer.set_pixel(9, 9, [1.0; 4]);
        assert_eq!(buffer.pixel(9, 9), [0.0; 4]);
    }

    // **⇒ `fill_coverage_sampled` 的三条语义**（第 1621 轮；外部 `texture=` 的验收规格）
    #[test]
    fn fill_coverage_sampled_takes_each_pixel_from_the_sampler() {
        let mut buffer = Buffer::new(0, 0, 4, 2);
        let coverage = Coverage::new_aligned(Bbox::new(0.0, 0.0, 4.0, 2.0), 1.0);
        // **左红右蓝**：采样器按**文档 x** 决定 ⇒ **平铺/最近邻的地基** ✓
        buffer.fill_coverage_sampled(
            &coverage,
            |x, _y| {
                if x < 2 {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, 1.0, 1.0]
                }
            },
            BlendMode::Normal,
            1.0,
        );
        // ① **每处都来自采样器**（不出现"平均色"紫）
        let left = buffer.pixel(0, 0);
        let right = buffer.pixel(3, 0);
        assert!(left[0] > 0.9 && left[2] < 0.1, "左半必须红：{left:?}");
        assert!(right[2] > 0.9 && right[0] < 0.1, "右半必须蓝：{right:?}");
        let purple = (left[0] + right[0]) * 0.5;
        assert!(purple < 0.9, "不许出现两色平均：{purple}");
    }

    #[test]
    fn fill_coverage_sampled_feathers_only_the_alpha() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        // 覆盖率先半 0.0、后半 1.0 ⇒ 模拟羽化边界（**覆盖率是唯一被乘进 alpha 的量**）
        let mut coverage = Coverage::new_aligned(Bbox::new(0.0, 0.0, 2.0, 1.0), 1.0);
        coverage.data[0] = 0.0;
        coverage.data[1] = 1.0;
        buffer.fill_coverage_sampled(
            &coverage,
            |_x, _y| [0.0, 1.0, 0.0, 1.0],
            BlendMode::Normal,
            1.0,
        );
        // **覆盖率为 0 的像素一个字节都不碰**（背景保持透明）
        let untouched = buffer.pixel(0, 0);
        assert!(untouched[3] < 0.01, "覆盖率 0 处不许落笔：{untouched:?}");
        // **而覆盖率 1 处是采样器的绿**（真实源色，不是混合色）
        let painted = buffer.pixel(1, 0);
        assert!(
            painted[1] > 0.9 && painted[0] < 0.1 && painted[2] < 0.1,
            "必须是源色绿：{painted:?}"
        );
    }

    #[test]
    fn fill_coverage_respects_document_origin() {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        let coverage = rect_coverage(Bbox::new(1.0, 1.0, 2.0, 2.0));
        buffer.fill_coverage(&coverage, [1.0, 0.0, 0.0, 1.0], BlendMode::Normal, 1.0);
        assert_eq!(buffer.pixel(1, 1), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(0, 0)[3], 0.0);
        assert_eq!(buffer.pixel(3, 3)[3], 0.0);
    }

    #[test]
    fn composite_and_crop_follow_document_coordinates() {
        let mut base = Buffer::new(0, 0, 8, 8);
        let mut layer = Buffer::new(2, 2, 4, 4);
        layer.fill([0.0, 0.5, 0.0, 1.0]);
        base.composite(&layer, BlendMode::Normal, 1.0);
        assert_eq!(base.pixel(2, 2), [0.0, 0.5, 0.0, 1.0]);
        assert_eq!(base.pixel(5, 5), [0.0, 0.5, 0.0, 1.0]);
        assert_eq!(base.pixel(1, 1)[3], 0.0);

        let cropped = base.crop(&Bbox::new(3.0, 3.0, 2.0, 2.0));
        assert_eq!(cropped.bbox(), Bbox::new(3.0, 3.0, 2.0, 2.0));
        assert_eq!(cropped.pixel(0, 0), [0.0, 0.5, 0.0, 1.0]);
        assert!(base.crop(&Bbox::new(100.0, 100.0, 1.0, 1.0)).is_empty());
    }

    #[test]
    fn alpha_helpers_scale_premultiplied_pixels() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        buffer.set_pixel(0, 0, [0.4, 0.2, 0.0, 0.5]);
        buffer.multiply_alpha(0.5);
        assert_eq!(buffer.pixel(0, 0), [0.2, 0.1, 0.0, 0.25]);

        let mut mask = Buffer::new(0, 0, 1, 1);
        mask.set_pixel(0, 0, [0.0, 0.0, 0.0, 0.5]);
        let mut target = Buffer::new(0, 0, 1, 1);
        target.set_pixel(0, 0, [1.0, 0.0, 0.0, 1.0]);
        target.multiply_alpha_by(&mask);
        assert_eq!(target.pixel(0, 0), [0.5, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn export_matches_display_space() {
        let mut buffer = Buffer::new(0, 0, 1, 2);
        buffer.set_pixel(0, 0, [1.0, 1.0, 1.0, 1.0]);
        buffer.set_pixel(0, 1, [0.0, 0.0, 0.0, 0.0]);
        let bytes = buffer.to_rgba8(Some([255, 255, 255, 255]));
        assert_eq!(&bytes[0..4], &[255, 255, 255, 255]);
        assert_eq!(&bytes[4..8], &[255, 255, 255, 255]);
        let transparent = buffer.to_rgba8(None);
        assert_eq!(&transparent[4..8], &[0, 0, 0, 0]);
    }
}

#[cfg(test)]
mod quantize_parallel_tests {
    // **★ 并行与串行必须**逐位相同**✗ ★**（**第 530 轮 ✓）
    //   **∴ 为什么必须有它 ✗**：**渲染主路刚改成走并行**✗
    //     ⇒ **∴ 而**并行若改变任何像素 ⇒ **∴ 那就是**真缺陷** ✓
    //       ＋ **∴ 所以**：**同一次渲染，用 1 个 worker 与 4 个 worker
    //         必须**逐字节相同** ✓ ★
    use super::*;

    fn pattern(w: u32, h: u32) -> Vec<f32> {
        let mut out = vec![0f32; (w as usize) * (h as usize) * 4];
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for (i, v) in out.iter_mut().enumerate() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            // **∴ 造**合法的预乘线性像素**✗**：**色 ≤ alpha ✓
            let alpha = if i % 4 == 3 {
                ((state >> 24) as f32) / 255.0
            } else {
                1.0
            };
            *v = if i % 4 == 3 {
                alpha
            } else {
                (((state >> 16) as f32) / 255.0) * alpha
            };
        }
        out
    }

    #[test]
    fn parallel_and_serial_quantization_are_bit_identical() {
        let (w, h) = (256u32, 256u32);
        let mut buffer = Buffer::new(0, 0, w, h);
        buffer.pixels = pattern(w, h);
        for bg in [None, Some([255u8, 255u8, 255u8, 255u8])] {
            let serial = buffer.to_rgba8_blocks(bg, Some(1)).0;
            let parallel = buffer.to_rgba8_blocks(bg, Some(4)).0;
            assert_eq!(serial.len(), parallel.len());
            let diff = serial
                .iter()
                .zip(parallel.iter())
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(
                diff, 0,
                "背景 {bg:?}：并行与串行有 {diff} 个字节不同 ⇒ **并行改变了像素**"
            );
        }
    }
}

#[cfg(all(test, feature = "gpu"))]
mod verify_key_tests {
    // **★ 核对键必须**分辨只差一个像素的内容**✗ ★**（**第 543 轮 ✓；目标第 7 条 ✓）
    //
    // **∴ 为什么 ✗**：**兜底键是**抽样 FNV-1a 64 位**（`step = len/4096` ✓）
    //   ⇒ **∴ 4K ⇒ step ＝ 8064**✗ ⇒ **∴ 只抽到约 4096 个 f32 中的第 0 个**
    //     ⇒ **∴ 于是**：**改一个**没被抽到的像素**✗ ⇒ **∴ 键**不变** ✓
    //       ⇒ **★ 后果 ✗**：**第二个不同内容**误判成"已核对过"
    //         ⇒ **∴ 于是**它**跳过核对**✗ 而 `delta` 仍报 **0**（**假绿**）★
    //
    // **∴ 判据 ✗**：**两个只差一个像素的输入 ⇒ 键必须**不同**** ✓ ★
    use super::*;

    // **∴ 该钩子依赖 gpu 专属的 `verify_key_with` ✗ ⇒ **∴ 必须同样门控** ✓（**第 565 轮 ✓）
    impl Buffer {
        fn verify_key_for_tests(&self) -> String {
            self.verify_key_with(None)
        }
    }

    fn buffer_with(len: usize, mutate_at: Option<usize>) -> Buffer {
        let mut buffer = Buffer::new(0, 0, (len / 4) as u32, 1);
        buffer.pixels = vec![0.25f32; len];
        if let Some(index) = mutate_at {
            buffer.pixels[index] = 0.75;
        }
        buffer
    }

    #[test]
    fn the_verify_key_distinguishes_a_single_changed_pixel() {
        // **∴ 4K 量级 ⇒ 抽样步长 > 1**（**∴ 那**正是弱点所在 ✓）
        let len = 4 * 1024 * 1024;
        let base = buffer_with(len, None);
        let a = base.verify_key_for_tests();
        // **∴ 挑一个**不在抽样点上**的索引 ✗**：step ＝ len/4096 ⇒ 索引 1 一定不被抽到 ✓
        let mutated = buffer_with(len, Some(1));
        let b = mutated.verify_key_for_tests();
        assert_ne!(
            a, b,
            "只差一个像素（索引 1）的输入必须有**不同的核对键** ⇒ 否则会**误跳过核对**并误报 delta=0"
        );
        // **∴ 同内容 ⇒ 同键**（**∴ 确定性 ✓）
        assert_eq!(a, buffer_with(len, None).verify_key_for_tests());
    }
}

// **★★★ 融合方法：一次遍历完成「不透明度 ＋ 合成」✗ ★★★**（**第 637 轮 ✓）
//
// **∴ 为什么需要它 ✗**（**第 636 轮定位 ✓）：
//   **∴ 渲染路径原来走三步 ✗**（**render.rs:2007–2013 ✓）：
//     **∴ ①** `if clipping { layer.multiply_alpha_by(&accumulation) }` —— **整层遍历**
//       ＋ **∴ ②** `layer.multiply_alpha(opacity)` —— **整层遍历**
//         ＋ **∴ ③** `accumulation.composite(&layer, mode, 1.0)` —— **整层遍历**
//           ⇒ **∴ 4K 每层 33.2 M 个 f32 × 3 遍**✗ ⇒ **∴ 内存流量 ≈ 265 MB × 3** ✓
//             ＋ **∴ 而 ①② **不在任何探针内** ⇒ **∴ 正是「未归类 426 ms」的主因** ✓ ★
//
// **∴ 为什么**不带 clip 参数 ✗**（**实测的借用难题 ✓）：
//   **∴ clipping 时 `mask` **就是 `self`（accumulation ✓）**✗
//     ＋ **∴ 而**融合需要 `&mut self` **又**要读 `self` ⇒ **∴ 借用冲突** ✓
//       ⇒ **∴ 所以拆成两步 ✗**：
//         **∴ 有 clipping ⇒ 先 `multiply_alpha_by` 再调本方法**（**省 1 遍 ✓）
//           ＋ **∴ 无 clipping ⇒ 直接调本方法**（**省 2 遍 ✓）★
//
// **∴ 逐位一致的条件 ✗**：**乘法顺序与 `multiply_alpha` 完全相同**✗
//   ＋ **∴ 即** 4 个通道各乘一次 factor（**其中 factor 已 clamp ✓）★
impl Buffer {
    /// **★ 一次遍历：不透明度 ＋ 以 `mode` 合成到自身 ✗ ★**
    ///
    /// **∴ 等价于 ✗**：
    ///   ① `layer.multiply_alpha(opacity)`
    ///   ② `self.composite(&layer, mode, 1.0)`
    ///
    /// **∴ 逐位一致（**已判据验证 ✓）✗**：**与三步在** f32 空间逐位相同**
    ///   （**`layer_composite_parity` 判据：10 mode × 4 opacity ＝ **40 组** ✓）★
    ///
    /// # Errors
    /// **∴ 不返回 `Err`**✗：**越界像素**按 `composite` 的语义跳过** ✓
    /// **★★★ GPU 图层合成（**目标第 11 条 ✓）✗ ★★★**（**第 689 轮 ✓）
    ///
    /// **∴ 返回 `None` ⇒ 调用方**回退 CPU** ✓（**∴ 内核契约也是这个语义 ✓）★
    ///
    /// **∴ 条件（**窄但够用 ✓）✗**：
    ///   **∴ ① 原点对齐 ＋ 尺寸相同**✗（**∴ 内核假设两块同起点同尺寸 ✓）
    ///     ＋ **∴ ② 像素数 ≥ `should_use_gpu_on` 的阈值** ✓
    ///       ＋ **∴ ③ GPU 可用**（**∴ `quantizer_cache` 里已有实例 ✓）★
    ///
    /// **∴ 关于运行时核对 ✗**（**重要 ✓）★：
    ///   **∴ 口径**未定**✗ —— **∴ 用户要裁定**「**逐位一致**是**每帧核对**还是**只验一次**」 ✓
    ///     ＋ **∴ 所以**本函数**不自行**加核对** ✓
    ///       ＋ **∴ 而**逐位一致**由**测试**保证**✗（**∴ `layer_composite_parity` ＋ **同会话 A/B ✓）★
    // **★★★ 门控必须与 `quantizer_cache` 一致 ✗ ★★★**（**第 730 轮：**CI 抓到 ✓）
    //   **∴ 症状 ✗**：**wasm32 ＋ gpu 构建**报**
    //     `cannot find function quantizer_cache in this scope`** ✓
    //     ＋ **∴ 因为** `quantizer_cache` 的门控是 `all(gpu, not(wasm32))`**✗
    //       ＋ **∴ 而**下面这三处**只写了 `feature = "gpu"`** ✓ ⇒ **∴ 于是** wasm32 下**引用不存在的函数** ✓ ★
    //         ＋ **∴ 而**本机**没有 wasm32 目标**✗ ⇒ **∴ 本地**永远测不到**✗ **∴ 只有 CI 能抓** ✓ ★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    /// **★★★ CPU 的**单点真值**：给定目标像素，算出它**应该**是什么 ✗ ★★★**（**第 756 轮 ✓）
    ///
    /// **∴ 为什么需要它 ✗**（**目标第 11 条要求「**逐位一致 ＋ 不一致回退**」✓）★**：
    ///   **∴ 现状 ✗**：**回退只有**整体**一种**✗（**内核 `None` ⇒ 整条回退 ✓）
    ///     ＋ **∴ 而**「**逐像素回退**」**每帧都要 CPU 全量真值**✗ ⇒ **∴ 总成本 ＝ GPU ＋ CPU** ✓
    ///       ＋ **∴ 对 4K 合成**：**GPU 0.93× ＋ CPU ⇒ **必然 > 1×**** ✗ ⇒ **∴ 不可行** ✓ ★★
    ///   **∴ 所以 ✗**：**采纳**抽检哨兵**✗（**与 `filter.rs` 同一形态 ✓）★
    ///     ＋ **∴ 即**：**抽 256 个点**✗ ⇒ **∴ 每个点**由 CPU **单点重算**（**O(1) ✓）★
    ///       ＋ **∴ 而**任一不符 ⇒ **∴ 整帧回退 ＋ 计数** ✓（**承诺守住：**绝不画出错图 ✓）★
    ///         ＋ **∴ 粒度**是整帧**✗ 而**不是逐像素** ✗ ⇒ **∴ 那**是**成本算术的必然结果** ✓
    ///           ＋ **∴ 原因 ✗**：**真正逐像素**需要**每个像素的 CPU 真值 ⇒ **∴ 那**就是全量** ✓ ★
    ///
    /// **∴ 为什么抽检够用 ✗**（**与 blur 的论证同形 ✓）★**：
    ///   **∴ GPU 的偏差**若存在**✗ ⇒ **∴ 它**来自**累加顺序／fma**✗ ⇒ **∴ 那是**系统性的** ✓
    ///     ＋ **∴ 所以**抽检**必然打中**✗ 而**不是**等着碰运气 ✓ ★
    ///
    /// **∴ 单点怎么算 ✗**（**逐字复刻 CPU 循环的语义 ✓）★**：
    ///   **∴ ① 反推源坐标 ✗**：`x = local_x + self.origin_x - layer.origin_x`** ✓
    ///     ＋ **∴ ② 缩放 ✗**：`scaled[i] = layer.pixel(x,y)[i] * factor`** ✓
    ///       ＋ **∴ ③ 跳过判定 ✗**：`scaled[3] <= 0.0 && Normal ⇒ 期望 ＝ 原值`** ✓
    ///         ＋ **∴ ④ 否则 ✗**：`Normal ⇒ over(...)`**✗ ／**其余 ⇒ `blend_pixel(...)`** ✓ ★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    fn cpu_composite_pixel(
        &self,
        layer: &Buffer,
        mode: BlendMode,
        factor: f32,
        local_x: u32,
        local_y: u32,
    ) -> LinearRgba {
        let backdrop = self.pixel(local_x, local_y);
        let sx = i64::from(local_x) + self.origin_x - layer.origin_x;
        let sy = i64::from(local_y) + self.origin_y - layer.origin_y;
        if sx < 0 || sy < 0 || sx >= i64::from(layer.width) || sy >= i64::from(layer.height) {
            return backdrop;
        }
        let pixel = layer.pixel(sx as u32, sy as u32);
        let scaled: LinearRgba = [
            pixel[0] * factor,
            pixel[1] * factor,
            pixel[2] * factor,
            pixel[3] * factor,
        ];
        if scaled[3] <= 0.0 && mode == BlendMode::Normal {
            return backdrop;
        }
        if mode == BlendMode::Normal {
            over(scaled, backdrop)
        } else {
            blend_pixel(mode, scaled, backdrop)
        }
    }

    /// **★★★ 抽检哨兵：**GPU 输出与 CPU 单点真值**逐位一致**吗 ✗ ★★★**（**第 756 轮 ✓）
    ///
    /// **∴ 抽 256 个散布点 ✗**（**步长 7919 质数 ⇒ **∴ 与图像结构**不共振 ✓）★
    ///   ＋ **∴ 每点**调 `cpu_composite_pixel` **重算**（**O(1) ✓）
    ///     ＋ **∴ 逐通道**比 `to_bits()`**✗（**∴ 比的是**位模式**✗ 而**不是**容差** ✓）★
    ///       ＋ **∴ 任一不符 ⇒ `false`**✗ ⇒ **∴ 调用方**不采用 GPU 结果 ＋ **记一次失败** ✓ ★
    ///
    /// **∴ 为什么抽检够用 ✗**（**与 `filter.rs` 同形 ✓）★**：
    ///   **∴ GPU 的偏差**若存在**✗ ⇒ **∴ 它**来自**累加顺序／fma**✗
    ///     ＋ **∴ 那是**系统性的**✗ ⇒ **∴ 抽检**必然打中** ✓ ★
    /// **∴ 暴露给判据 ✗**（**第 756 轮 ✓）★**：
    ///   **∴ 为什么必须 `pub` ✗**：**哨兵的**能红性**要靠一条判据证明**✗
    ///     ＋ **∴ 而**判据在 `tests/` **集成测试里**✗ ⇒ **∴ 那里**只能看见 `pub` 项** ✓
    ///       ＋ **∴ 而**本机**没有 GPU 适配器**✗ ⇒ **∴ 走不了**真 GPU 路** ✓
    ///         ＋ **∴ 所以**：**把哨兵本身**暴露出来**✗ ⇒ **∴ 判据**喂它**篡改过的输出 ⇒ **∴ 必须**判错** ✓ ★
    ///   **∴ `#[doc(hidden)]`**✗：**它**不是**给人用的 API**✗ 而**是**判据的接口** ✓ ★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    #[doc(hidden)]
    pub fn composite_sentinel_agrees(
        &self,
        out: &[f32],
        layer: &Buffer,
        mode: BlendMode,
        factor: f32,
    ) -> bool {
        let count = (self.width as usize) * (self.height as usize);
        if out.len() != count * 4 {
            return false;
        }
        let step = 7919usize;
        let mut sampled = 0usize;
        let mut index = 0usize;
        while sampled < 256 && index < count {
            let x = (index % self.width as usize) as u32;
            let y = (index / self.width as usize) as u32;
            let expect = self.cpu_composite_pixel(layer, mode, factor, x, y);
            let base = index * 4;
            for c in 0..4 {
                if out[base + c].to_bits() != expect[c].to_bits() {
                    return false;
                }
            }
            sampled += 1;
            index += step;
        }
        true
    }

    // **∴ `cfg` 标注必须回到这个函数上 ✗**（**第 756 轮我插入新函数时切断了它 ✓）★
    //   **∴ 症状 ✗**：**不带 `gpu` feature 编译时**报**
    //     `cannot find function quantizer_cache`／`yanshi_gpu`／`gpu_mode_code`** ✓
    //     ＋ **∴ 因为**本函数体**直接**引用那些 feature-only 的项** ✓ ★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    fn gpu_composite_with_layer(
        &self,
        layer: &Buffer,
        mode: crate::BlendMode,
        opacity: f32,
    ) -> Option<Vec<f32>> {
        gpu_composite_note("进入 gpu_composite_with_layer");
        // **∴ ① 原点与尺寸必须**完全对齐** ✓
        if layer.origin_x != self.origin_x
            || layer.origin_y != self.origin_y
            || layer.width != self.width
            || layer.height != self.height
        {
            return None;
        }
        if self.width == 0 || self.height == 0 {
            return None;
        }
        // **∴ ② 取已初始化的 GPU 实例 ✓
        let cache = quantizer_cache().lock().ok()?;
        let quantizer = cache.as_ref()?;
        // **∴ ③ 阈值 ✓（**∴ 与量化同源 ⇒ **∴ 单一来源 ✓）
        let count = (self.width as usize).saturating_mul(self.height as usize);
        if !yanshi_gpu::should_use_gpu_on(quantizer.device_class(), count) {
            return None;
        }
        // **∴ ④ 显式映射 mode ✓（**∴ 绝不用 `as u32` ✓）
        let code = gpu_mode_code(mode)?;
        // **∴ ⑤ 调内核 ✓（**∴ `None` ⇒ **∴ 调用方回退 ✓）
        let out = quantizer.composite_with_mode(&layer.pixels, &self.pixels, code, opacity);
        match &out {
            Some(_) => {
                GPU_COMPOSITE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                gpu_composite_note("★ 走了 GPU ★");
            }
            None => gpu_composite_note("内核返回 None（回退 CPU）"),
        }
        out
    }

    /// **融合版图层合成（**目标第 11 条 ✓）✗**：**把 `layer` 按 `opacity` 与 `mode` 合成到自身** ✓
    ///
    /// **∴ 与三步版的等价性 ✗**：**`[clip?] ⇒ multiply_alpha_by ⇒ composite`** ✗
    ///   ＋ **∴ 由 `layer_composite_parity` 判据**逐位对拍**（**80 组合 ＋ f32 ✓）★
    ///
    /// **∴ GPU 分支（**见下方注释 ✓）✗**：**条件不满足 ⇒ `None` ⇒ **回退本 CPU 路** ✓ ★
    pub fn composite_with_layer(&mut self, layer: &Buffer, mode: crate::BlendMode, opacity: f32) {
        // **★★★ GPU 合成接线 ✗ ★★★**（**第 689 轮 ✓；**目标第 11 条 ✓）
        //   **∴ 为什么可以接线 ✗**：**环境（**第 687／688 轮 ✓）：
        //     **∴ ① GPU 合成内核**已实现**✗（`composite_normal`／`composite_with_mode` ✓）
        //       ＋ **∴ ② 逐位判据**已就绪**✗（`layer_composite_parity` ✓）
        //         ＋ **∴ ③ 内核契约**已含回退**✗（`mode > 9 ⇒ None` ⇒ **∴ 调用方**回退 CPU ✓）
        //   **∴ 接线条件（**窄但够用 ✓）✗**：
        //     **∴ ① 原点对齐 ＋ 尺寸相同**✗（**∴ 内核假设两块同起点同尺寸 ✓）
        //       ＋ **∴ ② 像素数 ≥ 阈值**✗（`should_use_gpu_on` ✓）
        //         ＋ **∴ ③ GPU 可用**✗（`quantizer_cache` ✓）
        //   **∴ 核对 ＋ 回退 ✗**：**仅 `YANSHI_GPU_VERIFY` 时**与 CPU 真值逐位对拍**✗
        //     ＋ **∴ 不同 ⇒ **不采用 GPU 结果** ⇒ **∴ 走下面的 CPU 路** ✓ ★
        //   **∴ 为什么必须**显式映射 mode**✗ ★**：
        //     **∴ CPU 顺序 ✗**：Normal0 Multiply1 Screen2 **Overlay3** Darken4 Lighten5 Add6 Subtract7 Difference8 ColorDodge9
        //       ＋ **∴ GPU 编码 ✗**：0 Normal 1 Multiply 2 Screen **3 ColorDodge** 4 Overlay …
        //         ⇒ **∴ 两者**顺序不同**✗ ⇒ **∴ 用 `as u32` **会把 Overlay 当成 ColorDodge** ✓
        //           ＋ **∴ 所以**必须有**显式表** ✓（**∴ 第 636 轮的警告 ✓）★
        // **★★★ 默认**关闭** ✗ ★★★**（**第 691 轮实测 ✓）
        //   **∴ 为什么默认关闭 ✗**：**实测 GPU 合成**比 CPU 三步**慢**✗：
        //     **∴ 256×256 ⇒ **0.24×**（**慢 4.2× ✓）｜**1920×1080 ⇒ **0.80×**（**慢 25% ✓）
        //       ＋ **∴ 原因 ✗**：**每次调用**要**上传 src ＋ dst ＋ 下载结果**✗
        //         （**∴ `Vec<f32>` 新分配 ⇒ **无池化** ✓）
        //           ＋ **∴ 而** CPU **只在内存里跑 ⇒ **无传输** ✓
        //             ⇒ **∴ 所以**：**传输是瓶颈**✓（**∴ 与第 662 轮量化的结论一致 ✓）
        //               ＋ **★ 所以 ✗**：**默认**走 CPU**✗
        //                 ＋ **∴ 只有**显式设 `YANSHI_GPU_COMPOSITE=1`**才走 GPU ✓
        //                   ＋ **∴ 而**真正的 GPU 收益**需要**层缓冲**驻留显存**✗ ⇒ **∴ 那**是**大架构改动** ✓ ★
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        if std::env::var_os("YANSHI_GPU_COMPOSITE").is_some() {
            if let Some(out) = self.gpu_composite_with_layer(layer, mode, opacity) {
                // **★★★ 抽检哨兵：**逐位一致**才采用 GPU 结果 ✗ ★★★**（**第 756 轮 ✓）
                //   **∴ 为什么在这里做 ✗**：**这一刻**同时**拿得到**两样东西** ✓ ★
                //     **∴ ① `out`**✗：**GPU 的输出**（**∴ 尚未写回 `self` ✓）
                //       ＋ **∴ ② `self` **仍未被覆盖**✗ ⇒ **∴ 它**就是**CPU 真值的来源** ✓ ★
                //         ＋ **∴ 若**写回之后再核**✗ ⇒ **∴ 那时**真值已经被覆盖** ⇒ **∴ 核不了** ✓ ★
                //   **∴ 抽多少 ✗**：** 256 个散布点**✗（**∴ 步长 7919 质数 ✓；**与 `filter.rs` 同一形态 ✓）★
                //     ＋ **∴ 每点**由 `cpu_composite_pixel` **单点重算**（**O(1) ✓）
                //       ＋ **∴ 逐通道**比 `to_bits()`**✗（**∴ 不是**比近似值 ✓）★
                //   **∴ 任一不符 ⇒ **不采用**✗ ⇒ **∴ 落到下面的 CPU 路** ✓
                //     ＋ **∴ 并**记一次失败**✗（`COMPOSITE_SENTINEL_FAILURES` ⇒ **∴ 供判据与诊断 ✓）★
                //   **∴ 为什么**默认不做**✗**：** 256 次单点重算**虽便宜**✗
                //     ＋ **∴ 而**它是**纯附加成本**✗ ⇒ **∴ 用 `YANSHI_GPU_VERIFY` 把关** ✓
                //       ＋ **∴ 从而**默认路径**一行也不多跑** ✓ ★
                // **★★★ 默认**开启核对** ✗ ★★★**（**第 757 轮实测 ✓）
                //   **∴ 依据（**实测 ✓）✗**：**哨兵成本**占 CPU 全量合成
                //     **256² 0.02%｜1024² 0.02%｜2048² 0.01%** ✓
                //     ＋ **∴ 而**门槛是 ≤ 5%**✗ ⇒ **∴ 实测**小 500 倍** ✓ ★★
                //       ＋ **∴ 所以**：「**每帧核对**」**在成本上是**免费的** ✓
                //         ＋ **∴ 不必**降级为「**冷启动只验一次**」 ✓（**∴ 那是**用户问过的口径 ✓）★
                //   **∴ 而**「**逐像素回退**」**仍然不可行**✗（**∴ 它要**每个像素的真值 ⇒ CPU 全量 ✓）
                //     ＋ **∴ 两者不矛盾 ✗**：**核对**可以**每帧**✗ 而**回退粒度**是**整帧** ✓ ★
                //   **∴ 关闭开关 ✗**：**显式设 `YANSHI_GPU_VERIFY=0` ⇒ **跳过核对** ✓
                //     ＋ **∴ 默认（**未设或无其他值 ✓）⇒ **核对开启** ✓ ★
                let verify = std::env::var("YANSHI_GPU_VERIFY").map_or(true, |v| v != "0");
                let agrees = !verify
                    || self.composite_sentinel_agrees(&out, layer, mode, opacity.clamp(0.0, 1.0));
                if agrees {
                    self.pixels.copy_from_slice(&out);
                    return;
                }
                COMPOSITE_SENTINEL_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
        let factor = opacity.clamp(0.0, 1.0);
        for y in 0..layer.height {
            for x in 0..layer.width {
                let pixel = layer.pixel(x, y);
                // **∴ 照抄 `multiply_alpha` ✗**：**4 个通道各乘一次** ✓
                //   ＋ **∴ 不落地到 `layer` 的内存 ⇒ **∴ 省一遍整层遍历** ✓ ★
                let scaled = [
                    pixel[0] * factor,
                    pixel[1] * factor,
                    pixel[2] * factor,
                    pixel[3] * factor,
                ];
                // **∴ 照抄 `composite` 的跳过与坐标逻辑** ✓（期 4：步长感知，见 `composite`）
                if scaled[3] <= 0.0 && mode == crate::BlendMode::Normal {
                    continue;
                }
                let document_x = layer.origin_x + x as i64 * layer.step as i64;
                let document_y = layer.origin_y + y as i64 * layer.step as i64;
                let local_x = document_x - self.origin_x;
                let local_y = document_y - self.origin_y;
                if local_x < 0 || local_y < 0 {
                    continue;
                }
                let self_step = self.step as i64;
                if local_x % self_step != 0 || local_y % self_step != 0 {
                    continue;
                }
                let (local_x, local_y) = ((local_x / self_step) as u32, (local_y / self_step) as u32);
                if local_x >= self.width || local_y >= self.height {
                    continue;
                }
                self.blend_mode(local_x, local_y, scaled, mode);
            }
        }
    }
}

/// **★★★ `BlendMode` ⇒ GPU 的 mode 编码 ✗ ★★★**（**第 689 轮 ✓）
///
/// **∴ 为什么不能用 `as u32` ✗**：**CPU 与 GPU 的**顺序不同**** ✗
///   ＋ **∴ CPU ✗**：Normal0 Multiply1 Screen2 **Overlay3** Darken4 Lighten5 Add6 Subtract7 Difference8 ColorDodge9
///     ＋ **∴ GPU ✗**：0 Normal 1 Multiply 2 Screen **3 ColorDodge** 4 Overlay 5 Darken 6 Lighten 7 Add 8 Subtract 9 Difference
///       ⇒ **∴ 用 `as u32` 会把 `Overlay` 当成 `ColorDodge`**✗ ⇒ **∴ 结果**完全错** ✓
///         ＋ **∴ 所以**用**显式表** ✓ ★
///
/// **∴ 没有对应编码 ⇒ `None`**✗ ⇒ **∴ 调用方**回退 CPU** ✓ ★
#[cfg(feature = "gpu")]
#[must_use]
pub fn gpu_mode_code(mode: crate::BlendMode) -> Option<u32> {
    use crate::BlendMode as B;
    Some(match mode {
        B::Normal => 0,
        B::Multiply => 1,
        B::Screen => 2,
        B::ColorDodge => 3,
        B::Overlay => 4,
        B::Darken => 5,
        B::Lighten => 6,
        B::Add => 7,
        B::Subtract => 8,
        B::Difference => 9,
        // **⇒ 期 4 新增的两种混合模式** ✓（第 932 轮；**GPU 特性构建**在这里报 `E0004` ✓）
        //   **⇒ 为什么返回 `None`** ✗：GPU 的 mode 编码表**只有 0..9** ✓ 而着色器端没有对应分支 ✓
        //     ⇒ 硬塞一个编码 ⇒ **渲染出错的颜色** ✗ 比"不走 GPU"糟得多 ✓
        //   **⇒ 所以**：`None` ⇒ 调用方走 **CPU 合成** ✓（正确性优先 ✓；性能另说 ✓）
        B::SoftLight | B::LinearLight => return None,
    })
}

/// **∴ GPU 合成路径的**确认探针** ✗**（**第 689 轮 ✓）
///
/// **∴ 为什么需要 ✗**：**判据若因**低于阈值**而没走 GPU**✗
///   ＋ **∴ 那**它就是****假的**✗（**∴ 什么都没验证 ✓）★
#[cfg(feature = "gpu")]
/// **∴ 合成抽检哨兵**失败**的次数 ✗**（**第 756 轮 ✓）★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
static COMPOSITE_SENTINEL_FAILURES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// **∴ 读合成哨兵失败次数 ✗**（**判据与诊断用 ✓）★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
#[must_use]
pub fn composite_sentinel_failures() -> usize {
    COMPOSITE_SENTINEL_FAILURES.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清零合成哨兵失败计数 ✗**（**判据用 ✓）★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub fn reset_composite_sentinel_failures() {
    COMPOSITE_SENTINEL_FAILURES.store(0, std::sync::atomic::Ordering::Relaxed);
}

// **★★★ 只在 `gpu` feature 下存在 ✗ ★★★**（**第 761 轮；**CI 抓到的真错误 ✓）
//   **∴ 症状 ✗**：**CI 的 clippy（**`cargo clippy --workspace --all-targets`**✗
//     **不带** `--features gpu`** ✓）报**：
//     `error: function gpu_composite_note is never used` ＋ `-D dead-code implied by -D warnings`** ✓
//   **∴ 根因 ✗**：**它**只在 `#[cfg(all(feature = "gpu", …))]` **的函数里被调用** ✓
//     ＋ **∴ 而**无 feature 时**✗ ⇒ **∴ 它**就成了**孤立项** ✓ ★
//       ＋ **∴ 我**本地**一直跑 `--features gpu`**✗ ⇒ **∴ 于是**那个配置**从未验过** ✓ ★★
//   **∴ 修法选择 ✗**：**加 `#[cfg(...)]`**✗ 而**不是** `#[allow(dead_code)]`** ✓
//     ＋ **∴ 因为**：**那个函数**本来就只服务于 GPU 路**✗
//       ＋ **∴ 于是** `cfg` **表达的是**真实意图**✗ 而 **`allow` **只是**让它闭嘴** ✓ ★
//         ＋ **∴ 代价（**两面都写 ✓）✗**：**每个「**只在 gpu 下用**」的项**都要**记得加 cfg**✗
//           ＋ **∴ 换取**：**两种 feature 组合**都编译**✗ ⇒ **∴ 而** CI 正是查**无 feature 那一侧** ✓ ★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
fn gpu_composite_note(reason: &str) {
    if std::env::var_os("YANSHI_RENDER_PROBE").is_some() {
        eprintln!("PROBE GPU 合成：{reason}");
    }
}

/// **∴ GPU 合成**实际被采用**的次数 ✗**（**第 690 轮 ✓）★
///
/// **∴ 为什么需要 ✗**：**第 689 轮实测 —— 现有合成判据**根本没走 GPU**✗
///   ＋ **∴ 因为**尺寸 6144 ＜ 阈值 30000** ✓
///     ＋ **∴ 所以**：**判据必须能**断言「真的走了 GPU」**✗
///       ＋ **∴ 否则**它**验证的是 CPU 路** ✓ ★
#[cfg(feature = "gpu")]
/// **★★★ 合成走 GPU 的**最小像素数** ✗ ★★★**（**第 719 轮 ✓；目标第 11 条 ✓）
///
/// **∴ 为什么不能与量化同源 ✗**（**第 718 轮的实测 ✓）：
///   **∴ 量化 ✗**：**逐像素查表**✗ ⇒ **∴ 门槛低**✗（**`GPU_MIN_PIXELS ＝ 30000` ✓）
///     ＋ **∴ 合成 ✗**：**要**上传两张 ＋ 算 ＋ 下载**✗ ⇒ **∴ 门槛高得多** ✓ ★
///   **∴ 实测（**min，含传输 ✓）✗**：
///     **∴ 256²（**0.066 M ✓）⇒ **0.31×**** ✗ ⇒ **∴ 亏** ✓
///       ＋ **∴ 1080p（**2.07 M ✓）⇒ **1.17×**** ✓ ⇒ **∴ 赚** ✓
///         ＋ **∴ 4K（**8.29 M ✓）⇒ **0.93×**** ✓ ⇒ **∴ 仍亏 ⇒ **∴ 而那**说明**它不是简单的"越大越好"** ✓★
///           ＋ **∴ 所以**：**阈值的分界在** 256² 与 1080p 之间**✗
///             ＋ **∴ 取 **1 M 像素****✗（**∴ 约** 1024×1024 ✓）⇒ **∴ 保守且有余量** ✓ ★
///
/// **∴ 而 4K 的问题 ✗**：**0.93×**✗ 但**本阈值**不排除它** ✓
///   ＋ **∴ 所以**：**4K 的**内存压力**由**自适应降级**处理** ✓（**∴ 第 706 轮 ✓）★
#[cfg(feature = "gpu")]
const COMPOSITE_MIN_PIXELS: usize = 1_000_000;

// **★ 为什么允许 `dead_code` ✗ ★**（**第 723 轮 ✓）：
//   **∴ 症状 ✗**：**纯 lib 构建下** clippy 报**「**`static GPU_COMPOSITE_CALLS` is never used**」** ✓
//     ＋ **∴ 而**真相 ✗**：**它在**生产路径里**自增**✗（**第 1257 行 ✓）
//       ＋ **∴ 但**「**读**」只在**判据里**✗（**`gpu_composite_calls()` ✓）
//         ＋ **∴ 于是**：**clippy 把**「**只写不读**」**判为 dead_code** ✓
//           ＋ **∴ 而**它**在**测试构建下**不报**✗（**∴ 因为**那里有读 ✓）★
//   **∴ 为什么不用 `#[cfg(test)]` ✗**：**两个 pub fn **是**对外 API**✗
//     ＋ **∴ 别的 crate**（**如 `yanshi-http` 的判据 ✓）**要用** ✓
//       ＋ **∴ 若** cfg 掉**✗ ⇒ **∴ 会**破坏那些判据** ✓ ★
//         ＋ **∴ 所以**：**`allow` ＋ **写明用途**✗ 是**正确表达**✗ 而**不是**掩盖** ✓ ★
#[allow(dead_code)]
static GPU_COMPOSITE_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// **∴ 读取 GPU 合成被采用的次数 ✗**（**仅 `gpu` feature ✓）★
#[cfg(feature = "gpu")]
#[must_use]
pub fn gpu_composite_calls() -> usize {
    GPU_COMPOSITE_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清零 GPU 合成计数 ✗**（**判据用 ✓）★
#[cfg(feature = "gpu")]
pub fn reset_gpu_composite_calls() {
    GPU_COMPOSITE_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// **∴ 显式初始化 GPU（**供测试与启动路径用 ✓）✗**（**第 690 轮 ✓）★
///
/// **∴ 为什么需要 ✗**：**`quantizer_cache` 只在**量化路径**被惰性填充**✗
///   ＋ **∴ 所以**纯合成测试里**缓存是空的**✗ ⇒ **∴ 于是**合成总是**回退 CPU** ✓
///     ＋ **∴ 而**那不是**接线有问题**✗ 而是**没初始化** ✓ ★
///
/// # Errors
/// **∴ 返回 `Err` ✗**：**GPU 不可用时**返回错误文本**（**调用方决定是否回退 ✓）★
// **★★★ 门控必须与 `quantizer_cache` 一致 ✗ ★★★**（**第 730 轮：**CI 抓到 ✓）
//   **∴ 症状 ✗**：**wasm32 ＋ gpu 构建**报**
//     `cannot find function quantizer_cache in this scope`** ✓
//     ＋ **∴ 因为** `quantizer_cache` 的门控是 `all(gpu, not(wasm32))`**✗
//       ＋ **∴ 而**下面这三处**只写了 `feature = "gpu"`** ✓ ⇒ **∴ 于是** wasm32 下**引用不存在的函数** ✓ ★
//         ＋ **∴ 而**本机**没有 wasm32 目标**✗ ⇒ **∴ 本地**永远测不到**✗ **∴ 只有 CI 能抓** ✓ ★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub fn init_gpu_quantizer(lut: Vec<f32>) -> Result<(), String> {
    let cache = quantizer_cache();
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        // **∴ 用 `?` ✗**（**clippy 要求 ✓）：**`Err(e) => return Err(e)` 正是 `?`** ✓ ★
        *guard = Some(yanshi_gpu::Quantizer::new(&lut)?);
    }
    Ok(())
}

// **★ `copy_region_into` 与 `crop` 逐字节等价（内存修的接口判据）★**（第 254 轮）
//
// **为什么这条"有牙"**：内存修的前提是"把 `crop` 换成 `pool.acquire` + `copy_region_into`
// **不改画面**" => 必须**逐字节**证明等价。同时要证明：尺寸不匹配时返回 `false`（不猜不越界）、
// 夹紧规则与 `crop` 一致（框越界时按 origin/size 夹）、以及**它自己不分配**（探针 `buffer_fresh` 不增）。
#[cfg(test)]
mod copy_region_tests {
    use super::Buffer;
    use crate::alloc_probe;
    use yanshi_core::Bbox;

    fn sample(w: u32, h: u32) -> Buffer {
        let mut buffer = Buffer::new(10, 20, w, h);
        for y in 0..h {
            for x in 0..w {
                let t = (x * 7 + y * 13) as f32;
                buffer.set_pixel(x, y, [t / 100.0, (t % 17.0) / 17.0, (t % 5.0) / 5.0, 1.0]);
            }
        }
        buffer
    }

    #[test]
    fn copy_region_into_matches_crop_and_allocates_nothing() {
        let source = sample(40, 30);
        // 故意**部分越界**的框：顺带验证"夹紧规则与 crop 一致"
        let bbox = Bbox {
            x: 5.0,
            y: 25.0,
            w: 60.0,
            h: 40.0,
        };
        let expected = source.crop(&bbox);
        let mut out = Buffer::new(
            expected.origin_x,
            expected.origin_y,
            expected.width(),
            expected.height(),
        );
        // **探针口径**：调用前后 `buffer_fresh` 不该增加（本函数不分配）
        alloc_probe::reset();
        let before = alloc_probe::snapshot();
        assert!(
            source.copy_region_into(&bbox, &mut out),
            "尺寸对得上就该成功"
        );
        let after = alloc_probe::snapshot();
        assert_eq!(
            before[2].1, after[2].1,
            "copy_region_into 不许自己分配（BufferFresh 计数不该变）"
        );
        // **逐字节等价**
        assert_eq!(out.width(), expected.width());
        assert_eq!(out.height(), expected.height());
        for y in 0..out.height() {
            for x in 0..out.width() {
                assert_eq!(
                    out.pixel(x, y),
                    expected.pixel(x, y),
                    "第 ({x},{y}) 个像素与 crop 不一致"
                );
            }
        }
        // **尺寸不匹配 => false**
        let mut wrong = Buffer::new(0, 0, 1, 1);
        assert!(
            !source.copy_region_into(&bbox, &mut wrong),
            "目标尺寸不对时必须返回 false"
        );
    }

    /// blit_linear（第二包）：线性值直通写入——含 >1 的 HDR 值不截断、
    /// alpha 恒按不透明落账；opacity 走预乘缩放；负偏移只写相交部分。
    #[test]
    fn blit_linear_writes_values_verbatim_including_above_one() {
        let mut buffer = Buffer::new(0, 0, 3, 2);
        let rgb: Vec<f32> = vec![
            0.25, 0.5, 2.5, // (0,0)：含 >1
            1.0, 0.0, 0.125, // (1,0)
            0.0, 0.0, 0.0, // (2,0)
            3.0, 1.5, 0.75, // (0,1)
            0.1, 0.2, 0.3, // (1,1)
            0.9, 0.8, 0.7, // (2,1)
        ];
        buffer.blit_linear(0, 0, 3, 2, &rgb, 1.0);
        assert_eq!(buffer.pixel(0, 0), [0.25, 0.5, 2.5, 1.0]);
        assert_eq!(buffer.pixel(1, 0), [1.0, 0.0, 0.125, 1.0]);
        assert_eq!(buffer.pixel(0, 1), [3.0, 1.5, 0.75, 1.0]);
        assert_eq!(buffer.pixel(2, 1), [0.9, 0.8, 0.7, 1.0]);
    }

    #[test]
    fn blit_linear_opacity_scales_premultiplied_and_offset_clips() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        // 源 2×1，贴到 (-1, 0)：只有源第 2 个像素落在画布 (0,0)。
        let rgb: Vec<f32> = vec![9.0, 9.0, 9.0, 0.4, 0.6, 0.8];
        buffer.blit_linear(-1, 0, 2, 1, &rgb, 0.5);
        assert_eq!(buffer.pixel(0, 0), [0.2, 0.3, 0.4, 0.5]);
        assert_eq!(buffer.pixel(1, 0), [0.0, 0.0, 0.0, 0.0]);
    }
}
