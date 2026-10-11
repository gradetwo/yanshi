//! **★ 可选的 GPU 加速层 ✗ ★**（第 300 轮 ✓；**目标第 6／7 条 ✓）。
//!
//! **∴ 为什么单独一个 crate ✗**：**第 295／296 轮**的架构决定**（**如实 ✓）**✗**：
//!   **∴ 我**权衡过**把 `wgpu` 放进 `yanshi-render`**（**路 A ✓）✗
//!     ⇒ **∴ 而** `yanshi-render` **也被 `yanshi-wasm` 依赖** ✓
//!       ⇒ **∴ 那**会让 `wgpu` **进入内核的依赖图** ✓
//!         ⇒ **★ 而** AGENTS.md 第 6 条 ＋ 目标第 6 条：**内核的体积与确定性**权重更高** ✓ ★**** ✓✓
//!   **⇒ ★ 所以 ✗ ★**：**把 GPU 放进**独立 crate**✗
//!     ⇒ **∴ 只**服务端**可选依赖它** ✓
//!       ⇒ **∴ 于是**：**内核不受影响** ✗ ＋ **服务端**可以用 GPU** ✓ ★**** ✓✓
//!
//! **∴ 硬约束（**目标第 7 条 ✓）★**：**CPU 是真值**✗ ⇒ **∴ 本 crate**只**加速**✗
//!   ⇒ **∴ 逐位一致**由**调用方**用 CPU 结果核对** ✓（**∴ 见 `yanshi-http` 的自检 ✓）** ✓✓
//!
//! **∴ 边界（**如实 ✓）★**：**本 crate**不**决定**该不该用 GPU**✗
//!   ⇒ **∴ 那**由**调用方**按**规模阈值**决定** ✓（**`yanshi-http::gpu_policy` ✓）** ✓✓
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::time::Instant;

/// **★ 分块 ＋ 批量**的 GPU 量化 ✗ ★**（**∴ 只**这一条对外接口 ✓）。
///
/// **∴ 输入 ✗**：**线性预乘的 `f32` 像素**（**每像素 4 个通道 ✓，**长度 ＝ `4 × pixels`** ✓）
/// **∴ 输出 ✗**：**`rgba8`**（**长度 ＝ `4 × pixels`** ✓）
/// **∴ 参数 ✗**：`lut` 是 **sRGB 编码查表**（**4097 项 ✓ —— **必须**与 CPU 用**同一张** ✓）
///
/// **∴ 为什么分块 ✗**：**第 274 轮实测**✗ ⇒ **∴ 单绑定上限 **128 MiB**** ✓
///   ⇒ **∴ 而** 4K 的输入 **132.7 MiB** 超限** ✓ ⇒ **∴ 所以**必须切** ✓**** ✓✓
/// **∴ 为什么批量 ✗**：**第 283／284 轮实测**✗ ⇒ **∴ 逐块 `Wait`**花 71.5% 的时间** ✓
///   ⇒ **∴ 全部块放进**一个 encoder ＋ 一次 `Wait`** 后：**快 88.6%** ✓**** ✓✓
///
/// # Errors
/// **∴ 没有适配器／拿不到设备／映射失败** ⇒ **∴ 返回 `Err`**（**∴ 调用方**据此回退 CPU** ✓）
pub struct Quantizer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    lut_buf: wgpu::Buffer,
    adapter_note: String,
    /// **★ 适配器等级 ✗ ★**（**∴ 供**分档阈值**用 ✓ —— **第 322 轮 ✓）。
    device_class: DeviceClass,
}

/// **★ 低于它就走 CPU ✗ ★**（第 304 轮 ✓；**∴ 从 `yanshi-http` **移过来**** ✓）。
///
/// **★ 依据（**第 285 轮的八档实测 ✓）—— **∴ 而第 318 轮**发现基准平台不存在**** ✗ ★**：
///   **∴ 核显（**Intel HD 5000 ✓）的八档（**第 285 轮 ✓）✗**：
///     **∴ n = 3 000 ✗**：**GPU **慢 77.4%**** ✓
///     **∴ n = 10 000 ✗**：**GPU **快 46.3%**** ✓
///     **∴ n = 30 000 ✗**：**GPU **快 81.5%**** ✓
///       ⇒ **∴ 平衡点在 **3 k 与 10 k** 之间** ⇒ **∴ 取 30 000**（**留 3 倍余量 ✓）** ✓✓
/// **★★★ 第 317–318 轮的更正（**如实 ✓）★★★**：
///   **∴ 我曾**按「**L4 为准**」把它改成 10 000**✗
///     ⇒ **∴ 而第 318 轮**实测发现**：**远端那个适配器是
///       **`Gl｜Cpu｜llvmpipe`（**CPU 软件渲染 ✓）**✗ ⇒ **∴ 不是**真 L4** ✓**** ✓✓
///       ⇒ **★ 所以**：**那批「**L4 的数**」**其实是** 12 线程 CPU **软件光栅的数** ✓ ★**** ✓✓
///         ⇒ **∴ 于是**：**10 000 的**依据不成立**** ✓ ⇒ **∴ 改回 30 000** ✓**** ✓✓
/// **∴ 下一步（**未做 ✓）★**：**要让容器**真的用上 L4**✗ ⇒ **∴ 需要**：
///   **∴ ①** **`/dev/dri`（**或 `/dev/nvidia*` ✓）**直通到容器** ✓
///   **∴ ②** **Vulkan ICD（**`nvidia_icd.json` ✓）** ＋ **`libvulkan`** ✓
///   ⇒ **∴ 然后** `adapter.get_info()` **才会报 `Vulkan｜DiscreteGpu｜NVIDIA L4`** ✓ ★**** ✓✓
pub const GPU_MIN_PIXELS: usize = 30_000;

/// **★ 这个规模该不该走 GPU ✗ ★**（**纯函数 ⇒ 可单测 ✓）
#[must_use]
pub const fn should_use_gpu(pixels: usize) -> bool {
    pixels >= GPU_MIN_PIXELS
}

/// **★ 按适配器等级取阈值 ✗ ★**（第 320 轮 ✓；**用户第 317 轮**以 L4 为准** ⇒ **∴ 但**核显也要照顾** ✓）。
///
/// **∴ 两个平台实测的平衡点 ✗（**都来自八档 + 5 次中位数 ✓）★**：
///   **∴ 真 L4（**`Vulkan｜DiscreteGpu` ✓）✗**：**1 000 快 25.8%**✗ ⇒ **∴ 平衡点 **≤ 1 000**** ✓
///     ⇒ **∴ 取 **10 000****（**∴ 留** 10 倍余量** ✓）** ✓✓
///   **∴ 核显（**`Gl｜IntegratedGpu` ✓）✗**：**3 000 慢 77.4%／10 000 快 46.3%**** ✓
///     ⇒ **∴ 平衡点在 **3 k–10 k**** ✓ ⇒ **∴ 取 **30 000****（**∴ 留** 3 倍余量** ✓）** ✓✓
///   **∴ 其它（**`Cpu`（**如 llvmpipe ✓）／`VirtualGpu` ✓）✗**：**取**核显那档**（**保守 ✓）
///     ⇒ **∴ 因为**软件渲染**根本不是加速** ✓（**∴ 第 318 轮**踩过 ✓）** ✓✓
///
/// **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
///   **∴ 收益 ✗**：**独显上** 1 k–10 k 也走 GPU**✗ ⇒ **∴ 于是**中大规模**都吃到加速** ✓**** ✓✓
///   **∴ 代价 ✗**：**多一条**平台相关分支**✗ ⇒ **∴ 于是**行为**依硬件而变** ✓
///     ⇒ **∴ 所以**：**响应里必须**如实报出用的是哪条** ✓（**目标第 7 条 ✓）** ✓✓
///
/// **∴ 未知类型 ✗** ⇒ **∴ 取**保守档** ✓（**∴ 与**旧的 30 000 一致 ✓）** ✓✓
#[must_use]
pub const fn threshold_for(device_type: DeviceClass) -> usize {
    match device_type {
        DeviceClass::Discrete => 10_000,
        // **∴ 集成／虚拟／CPU 软件渲染 ⇒ **一律保守**** ✓
        DeviceClass::Integrated | DeviceClass::Virtual | DeviceClass::Cpu | DeviceClass::Other => {
            GPU_MIN_PIXELS
        }
    }
}

/// **★ 适配器等级 ✗ ★**（**∴ 只用我们关心的四类 ＋ 其它 ✓）。
///
/// **∴ 为什么不直接用 `wgpu::DeviceType` ✗**：**两边都要能用它**✗
///   ⇒ **∴ 而** `yanshi-render` 的**默认构建**里**没有 `wgpu`** ✓
///     ⇒ **∴ 所以**：**本 crate**定义一个**不依赖 `wgpu` 的枚举** ✓
///       ⇒ **∴ 由调用方**从 `wgpu::DeviceType` **映射过来** ✓**** ✓✓
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// **∴ 独立显卡 ✗**（**`DiscreteGpu` ✓）
    Discrete,
    /// **∴ 集成显卡 ✗**（**`IntegratedGpu` ✓）
    Integrated,
    /// **∴ 虚拟 GPU ✗**（**`VirtualGpu` ✓）
    Virtual,
    /// **★ CPU 软件渲染 ✗ ★**（**`Cpu` ✓ —— **∴ 不是**加速** ✓，**第 318 轮的教训** ✓）
    Cpu,
    /// **∴ 其它／未知 ✗**
    Other,
}

/// **★ 按适配器等级判断该不该走 GPU ✗ ★**（**∴ 把**阈值 ＋ 比较**合成一个入口 ✓）
#[must_use]
pub const fn should_use_gpu_on(device_type: DeviceClass, pixels: usize) -> bool {
    pixels >= threshold_for(device_type)
}

/// **★ 每块的输入字节上限 ✗ ★**（**第 496 轮：**从写死改为**按适配器上限自适应** ✓）
///
/// **∴ 为什么必须自适应 ✗**（**第 495 轮在 L4 上的数据 ✓）**：
///   **∴ 原来写死 `4 MiB` ✗** ⇒ **∴ 每块 ＝ `4 MiB / 16 B` ＝ **262 144 像素**** ✓
///     ＋ **∴ 于是**：**4K 整幅（**3840×2160 ＝ 8 294 400 像素 ✓）
///       ⇒ **∴ 需要 **32 块**** ✓
///         ⇒ **∴ 32 次**：建 buffer ＋ dispatch ＋ 读回（`map_async` ＋ `Wait` ✓）✓
///   **∴ 而**实测已经把瓶颈指到"**传输／分块／同步**"**✗**：
///     ⇒ **∴ 核对只占 11%**（**207 ms ✓）✗
///       ＋ **∴ 所以**：**剩下的大头**极可能就是**块数与往返** ✓ ★**** ✓✓
///   **∴ 而** L4 的 `max_storage_buffer_binding_size` ＝ **2047 MiB**✗
///     ⇒ **∴ 单块**理论上能放 **1.3 亿像素** ✓
///       ⇒ **∴ 4K**一块就够** ✓ ⇒ **∴ 块数 32 ⇒ 1** ✓ ★**** ✓✓
///
/// **∴ 下限（**`CHUNK_BYTES_MIN` ✓）✗**：**4 MiB**✗
///   ⇒ **∴ 保证**极小的适配器（**如上限只有几 MiB ✓）也**能工作** ✓
/// **★ 上限（**`CHUNK_BYTES_MAX` ✓）✗ ★**：**4 MiB**✗
///   **∴ 它是**实测选出来的**✗ ，**不是猜的** ✓**（**第 496 轮 A/B ✓）**：
///     **∴ 在 L4（**真独显 ✓）上**同一份代码**只改分块大小**✗**：
///       | 分块 | 墙钟中位 | CPU 中位 | 峰值 RSS |
///       |---|---|---|---|
///       | **4 MiB（**32 块 ✓）** | **1831 ms** | **2260 ms** | **3082 MB** |
///       | **自适应 64 MiB（**1 块 ✓）** | **1936 ms** | **2370 ms** | **3432 MB** |
///       ⇒ **★ 放大分块**慢了 5.7% ＋ CPU 多 4.9% ＋ 内存多 11%** ✓
///         ⇒ **∴ 所以**：**"往返次数是大头"这个推断**错了** ✓
///           ＋ **∴ 大 buffer 的**分配／缓存成本** ＞ **省下的往返** ✓ ★**** ✓✓
///     **∴ 结论 ✗**：**上限回到 **4 MiB**** ✗ ⇒ **∴ 与**实测最优**一致** ✓
///       ＋ **∴ 而**自适应**仍然保留**✗ —— **∴ 它**保证**极小的适配器**也能跑** ✓
///         （**∴ 且**大图仍会按 `count/4` 细分 ⇒ **∴ 不会**一次吃掉整幅** ✓）★**** ✓✓
///   **∴ 对照开关照旧 ✗**：`YANSHI_GPU_CHUNK_BYTES=<字节数>`**（**∴ 显式覆盖 ⇒ 便于再 A/B ✓）** ✓
///   ⇒ **∴ 为什么需要它 ✗**（**两面之代价 ✓）**：
///     **∴ 收益**：**块少 ⇒ 往返少 ⇒ 更快** ✓
///     **∴ 代价**：**单块**占的内存／显存**上升** ✓
///       （**∴ 若**放到 256 MiB✗ ⇒ **∴ 4K 一块就能装下整帧 132.7 MiB ✓）
///       ⇒ **★ 而**那**违反了一个**已有的守卫**✗ ★**：
///         `yanshi-render/tests/quantize_peak_memory.rs` 的
///         `no_full_frame_f32_copy_is_allocated` 断言
///         **「量化期间任何**单次分配**必须**小于整幅 f32 副本**」** ✓
///         ⇒ **∴ 所以**：**上限压到 64 MiB**✗ ＋ **∴ 并**再按 `count/4` 细分** ✓
///           ⇒ **∴ 于是**：**1024×512（**frame_bytes 8 MiB ✓）
///             ⇒ **∴ 单块 ＝ 2 MiB ＜ 8 MiB ⇒ **守卫绿**** ✓ ★**** ✓✓
///   **∴ 为什么不直接把守卫放宽 ✗**（**∴ 我**没擅自改 ✓）：
///     ⇒ **∴ 那条守卫守的是**真 bug**（**"为了省事把整帧 crop 一份" ✓）
///       ⇒ **∴ 放宽它**要**用户裁定** ✓（**∴ 见设计文档的待裁定项 ✓）★**** ✓✓
const CHUNK_BYTES_MIN: usize = 4 * 1024 * 1024;
const CHUNK_BYTES_MAX: usize = 4 * 1024 * 1024;

/// **∴ 每像素 16 字节（**4 × f32 ✓）
const BYTES_PER_PIXEL: usize = 16;

/// **★ 主机侧分段计时 ✗ ★**（**第 498 轮 ✓）
///
/// **∴ 为什么不用 GPU 时间戳 ✗**（**第 497 轮的教训 ✓）**：
///   **∴ 我**想用 `TIMESTAMP_QUERY` 量 GPU 自己的时间**✗
///     ⇒ **∴ 连撞两条 wgpu 规则**✗ ⇒ **∴ 服务**僵死** ✓（**已记档 ✓）**
///       ＋ **∴ 而**即使量到，**GPU 时间**也解释不了**主机的等待** ✓
///         ⇒ **★ 所以 ✗ ★**：**先量**主机侧的三段**✗**
///           ⇒ **∴ 它**不用任何 feature**✗ ⇒ **∴ 零风险** ✓
///             ＋ **∴ 且**足以定位**：**是**上行贵**✗ 、**提交换**✗ 、**还是**读回贵** ✓ ★**** ✓✓
///
/// **∴ 三段（**都在本 crate ✓）✗**：
///   ⇒ **∴ ① `upload` ✗**：**每块建 4 个 buffer ＋ 2 次 `write_buffer`**（**含 `f32→u8` 构造 ✓）
///     ＋ **∴ ② `submit` ✗**：**`queue.submit` ＋ 一次 `poll`** ✓
///       ＋ **∴ ③ `readback` ✗**：**逐块的 `map_async` ＋ 等待** ✓
///         ＋ **∴ 第四段（**逐位核对 ✓）在 `yanshi-render`** ✗ ⇒ **∴ 那边单独量** ✓ ★**** ✓✓
///
/// **∴ 单位 ✗**：**纳秒存 ⇒ 微秒读** ✓（**∴ `u64` 纳秒：4K 也才 10^9 量级 ⇒ 不溢出 ✓）★
static STAGE_UPLOAD_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static STAGE_SUBMIT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static STAGE_READBACK_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static STAGE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// **★ 读最近一次的分段计时 ✗ ★**
///
/// **∴ 返回 `(上行微秒, 提交微秒, 读回微秒, 次数)` ✗**：
///   ⇒ **∴ 次数 ＝ 0 ⇒ **三段都不可读**** ✓（**∴ 不许**当成 0 ✓）★
#[must_use]
pub fn last_stage_timing() -> (u64, u64, u64, u32) {
    let us = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed) / 1000;
    (
        us(&STAGE_UPLOAD_NS),
        us(&STAGE_SUBMIT_NS),
        us(&STAGE_READBACK_NS),
        STAGE_CALLS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// **★ 本平台能不能用**同步**的 `Quantizer::new` ✗ ★**（**第 443 轮 ✓）
///
/// **∴ 为什么需要这条 ✗**：**本 crate**用自实现的 `block_on`**✗（**∴ 不引入 `pollster` ✓）
///   ⇒ **∴ 而** `block_on` **只能在**多线程 ＋ 不依赖外部事件循环**的平台上工作** ✓
///     ⇒ **∴ 在 `wasm32` 上 ✗**：**单线程 ＋ 事件循环**✗
///       ⇒ **∴ `request_adapter` 的完成**要**靠**外部的微任务**✗
///         ⇒ **★ 所以**：**`block_on` **永远等不到它** ⇒ **∴ 会**死锁** ✓ ★**** ✓✓
///
/// **∴ 这条函数的作用 ✗**：**让调用方**先问一句**✗
///   ⇒ **∴ 于是**：**在 wasm 上**不去建同步 `Quantizer`**✗
///     ⇒ **∴ 而是**走**异步路**（**将来 ✓）或**直接回退 CPU** ✓
///       ⇒ **★ 那**守住了**目标第 7 条**：**不许假装用了 GPU ✗ ＋ **不许静默降级** ✓ ★**** ✓✓
///
/// **∴ 返回值 ✗**：**`true` ＝ 本平台支持同步初始化**（**桌面 ✓）；
///   **`false` ＝ 必须走异步路**（**`wasm32` ✓）** ✓
#[must_use]
pub const fn supports_sync_init() -> bool {
    !cfg!(target_arch = "wasm32")
}

impl Quantizer {
    /// **★ 建一个量化器 ✗ ★**（**∴ 成功 ⇒ GPU 可用 ✓）
    ///
    /// # Errors
    /// **∴ 没有适配器或设备** ⇒ **∴ 返回 `Err`**（**∴ 调用方回退 CPU ✓）
    /// **★ 同步构造（**薄包装 ✓）✗ ★**（**第 468 轮 ✓）
    ///
    /// **∴ 平台守卫 ✗**（第 443 轮 ✓）**：**wasm32 上**不尝试同步初始化** ✓（**∴ 不死锁 ✓）
    ///   ⇒ **∴ 而是**响亮失败**✗ ⇒ **∴ 调用方**走 `new_async` 或回退 CPU** ✓
    ///
    /// # Errors
    /// **∴ 平台不支持同步初始化／没有适配器／没有设备** ⇒ **∴ 返回 `Err`** ✓
    pub fn new(lut: &[f32]) -> Result<Self, String> {
        if !supports_sync_init() {
            return Err("sync_init_unsupported_on_wasm32：本平台是单线程事件循环，\
                        同步 block_on 会死锁 ⇒ 请走 new_async 或回退 CPU"
                .to_owned());
        }
        block_on(Self::new_async_inner(lut))
    }

    /// **★ 异步构造 ✗ ★**（**第 468 轮 ✓；**wasm 侧用它 ✓）
    ///
    /// **∴ 与同步版的差别 ✗**：**只有**等待方式不同**✗
    ///   ⇒ **∴ 逻辑**是**同一份**（**`new_async_inner` ✓）★**** ✓✓
    ///
    /// # Errors
    /// **∴ 没有适配器／没有设备** ⇒ **∴ 返回 `Err`** ✓
    pub async fn new_async(lut: &[f32]) -> Result<Self, String> {
        Self::new_async_inner(lut).await
    }

    /// **★ 唯一的异步实现 ✗ ★**（**第 468 轮 ✓；**`gpu-webgpu-discussion.md` 第 445 轮 ✓）：
    ///   **∴ 为什么抽出来 ✗**：**同步版**用自实现的 `block_on`**✗
    ///     ⇒ **∴ 而**它**在 wasm32 上**会死锁** ✓
    ///       ⇒ **∴ 所以**：**核心必须是 `async`**✗
    ///         ＋ **∴ 同步版**只做**薄包装**（`block_on` ✓）
    ///           ＋ **∴ wasm 侧**直接用 `new_async`** ✓
    ///             ⇒ **★ 那**让**同一条逻辑**服务**两个平台** ✓ ★**** ✓✓
    async fn new_async_inner(lut: &[f32]) -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .map_err(|e| format!("no_adapter:{e:?}"))?;
        let info = adapter.get_info();
        let adapter_note = format!(
            "adapter:{:?}:{:?}:{}",
            info.backend, info.device_type, info.name
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| format!("no_device:{e:?}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yanshi-gpu-quantize"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("yanshi-gpu-quantize"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let lut_bytes = f32s_to_bytes(lut);
        let lut_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lut"),
            size: lut_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&lut_buf, 0, &lut_bytes);
        // **★ 把 `wgpu::DeviceType` **映射**到本 crate 的 `DeviceClass`** ✗ ★**（第 322 轮 ✓）：
        //   ⇒ **∴ 于是**：**阈值**由**适配器等级**决定** ✓（**∴ 第 320 轮 ✓）
        let device_class = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => DeviceClass::Discrete,
            wgpu::DeviceType::IntegratedGpu => DeviceClass::Integrated,
            wgpu::DeviceType::VirtualGpu => DeviceClass::Virtual,
            wgpu::DeviceType::Cpu => DeviceClass::Cpu,
            wgpu::DeviceType::Other => DeviceClass::Other,
        };
        Ok(Self {
            device,
            queue,
            pipeline,
            lut_buf,
            adapter_note,
            device_class,
        })
    }

    /// **∴ 适配器的自述 ✗**（**∴ 可核对 ✓）
    #[must_use]
    pub fn adapter_note(&self) -> &str {
        &self.adapter_note
    }

    /// **★ 适配器等级 ✗ ★**（第 322 轮 ✓；**∴ 供**分档阈值**用 ✓）。
    ///
    /// **∴ 为什么存下来而不是现查 ✗**：**`Adapter` **没有**留在 `Quantizer` 里**✗
    ///   ⇒ **∴ 所以**：**建的时候**映射一次并存下** ✓**** ✓✓
    #[must_use]
    pub fn device_class(&self) -> DeviceClass {
        self.device_class
    }

    /// **∴ 单绑定的上限（**字节 ✓）✗**（**∴ 供调用方**自己核对分块 ✓）
    #[must_use]
    pub fn max_binding_bytes(&self) -> u64 {
        u64::from(self.device.limits().max_storage_buffer_binding_size)
    }

    /// **∴ 适配器允许的**单缓冲**上限（**字节 ✓）✗**
    #[must_use]
    pub fn max_buffer_bytes(&self) -> u64 {
        self.device.limits().max_buffer_size
    }

    /// **★ 每块处理多少像素 ✗ ★**（**第 496 轮：**自适应 ✓）
    ///
    /// **∴ 取值（**四者取最小 ✓）✗**：
    ///   1. `max_storage_buffer_binding_size` 的 **3/4**（**留余量给 dst ＋ params ✓）
    ///   2. `max_buffer_size` 的 **3/4**
    ///   3. [`CHUNK_BYTES_MAX`]（**封顶 ⇒ 不吃光显存 ✓）
    ///   4. 再由 [`CHUNK_BYTES_MIN`] **兜底**（**极小的适配器也能跑 ✓）
    ///   5. **再按 `count / 4` 细分**（**∴ 见下 ✓）
    ///
    /// **∴ 最后**除以 16 字节／像素** ✓
    ///
    /// **∴ 为什么是 3/4 而不是全用 ✗**（**两面 ✓）**：
    ///   **∴ `dst` 也是**同一个每绑定上限**✗ ⇒ **∴ 它只占 src 的 1/4**（4 B vs 16 B ✓）
    ///     ⇒ **∴ 理论上**src 用满也不超** ✓
    ///       ＋ **∴ 但**驱动可能把 params／LUT 也算进同一档** ✓
    ///         ⇒ **∴ 所以**留 1/4 余量 ⇒ **∴ 代价**是块数最多多 1/3** ✓
    ///           （**∴ 4K 上**仍是 1 块 ✓ ⇒ **∴ 代价**为 0** ✓）★**** ✓✓
    ///
    /// **∴ 对照开关 ✗**（**∴ 只为测量 ⇒ 两面 ✓）**：
    ///   ⇒ **∴ `YANSHI_GPU_CHUNK_BYTES=<字节数>`**✗
    ///     ⇒ **∴ 它**强制分块大小**✗ ⇒ **∴ 于是**：**同一台机器上能**A/B** ✓
    ///       ＋ **∴ 代价 ✗**：**它**绕过了自适应**✗ ⇒ **∴ 只许**测量用** ✓
    ///         （**∴ 生产路径**不许设它** ✓）★**** ✓✓
    #[must_use]
    pub fn per_chunk_pixels(&self, count: usize) -> usize {
        // **∴ 对照开关优先 ✗**（**∴ 显式覆盖 ⇒ 便于 A/B ✓）
        if let Some(raw) = std::env::var_os("YANSHI_GPU_CHUNK_BYTES") {
            if let Some(n) = raw.to_str().and_then(|v| v.trim().parse::<usize>().ok()) {
                if n >= BYTES_PER_PIXEL {
                    return n / BYTES_PER_PIXEL;
                }
            }
        }
        let binding = self.max_binding_bytes();
        let buffer = self.max_buffer_bytes();
        let budget = binding
            .min(buffer)
            .saturating_mul(3)
            .saturating_div(4)
            .clamp(CHUNK_BYTES_MIN as u64, CHUNK_BYTES_MAX as u64);
        // **★ 再按 `count/4` 细分 ✗ ★**（**第 496 轮 ✓）
        //   **∴ 为什么 ✗**：**单块不许**接近整幅** ✗
        //     ⇒ **∴ 因为**守卫断言**单次分配 ＜ 整幅 f32** ✓
        //       ＋ **∴ 而** 1024×512 的整幅只有 8 MiB** ✓
        //         ⇒ **∴ 若**按 64 MiB 封顶 ⇒ **∴ 一块就装下整幅**✗ ⇒ **∴ 守卫**红** ✓
        //           ⇒ **∴ 所以**：**min(预算, count/4 对应的字节数)** ✓ ★**** ✓✓
        let quarter_bytes = (count.saturating_mul(BYTES_PER_PIXEL) / 4) as u64;
        let budget = budget.min(quarter_bytes.max(CHUNK_BYTES_MIN as u64));
        let pixels = (budget as usize) / BYTES_PER_PIXEL;
        // **∴ 防御：**永不为 0**✗**（**∴ 否则**死循环** ✓）
        pixels.max(1)
    }

    /// **★ 量化一批像素 ✗ ★**（**∴ 分块 ＋ 批量 ＋ 一次 `Wait`** ✓）
    ///
    /// # Errors
    /// **∴ 映射失败** ⇒ **∴ 返回 `Err`** ✓
    /// **★ 量化 ＋ 编码 ✗ ★**（**第 459 轮：**加背景合成 ✓）
    ///
    /// **∴ `background` ✗**：**线性预乘的 `[r,g,b,a]`**✗
    ///   ⇒ **∴ `None`**：**不做合成**（**与原来一致 ✓）
    ///     ＋ **∴ `Some(bg)`**：**逐像素 `c = x + bg * (1 - x3)`**✓
    ///       ⇒ **∴ 与** CPU 的 `composite_over_linear_with` **同一口径** ✓ ★**** ✓✓
    /// **★ 同步量化（**薄包装 ✓）✗ ★**（**第 471 轮 ✓）
    ///
    /// # Errors
    /// **∴ 平台不支持／量化或映射失败** ⇒ **∴ 返回 `Err`** ✓
    pub fn quantize(
        &self,
        pixels: &[f32],
        count: usize,
        background: Option<[f32; 4]>,
    ) -> Result<Vec<u8>, String> {
        if !supports_sync_init() {
            return Err("sync_wait_unsupported_on_wasm32：本平台是单线程事件循环，\
                        同步等待会死锁 ⇒ 请用 quantize_async 或回退 CPU"
                .to_owned());
        }
        block_on(self.quantize_async_inner(pixels, count, background))
    }

    /// **★ 异步量化 ✗ ★**（**wasm 侧用它 ✓）（**第 471 轮 ✓）
    ///
    /// # Errors
    /// **∴ 量化或映射失败** ⇒ **∴ 返回 `Err`** ✓
    pub async fn quantize_async(
        &self,
        pixels: &[f32],
        count: usize,
        background: Option<[f32; 4]>,
    ) -> Result<Vec<u8>, String> {
        self.quantize_async_inner(pixels, count, background).await
    }

    /// **★ 唯一的异步量化实现 ✗ ★**（**第 471 轮 ✓）
    async fn quantize_async_inner(
        &self,
        pixels: &[f32],
        count: usize,
        background: Option<[f32; 4]>,
    ) -> Result<Vec<u8>, String> {
        // **★ 自适应分块 ✗ ★**（**第 496 轮 ✓；**原来写死 4 MiB ⇒ 4K 要 32 块 ✓）
        let per_chunk = self.per_chunk_pixels(count);
        let layout = self.pipeline.get_bind_group_layout(0);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let mut backs: Vec<(wgpu::Buffer, usize)> = Vec::new();
        let mut done = 0usize;
        // **∴ 分段累加器 ✗**（**∴ 一次调用内累加 ⇒ 最后写静态 ✓）★
        let mut upload_ns = 0u64;
        let mut submit_ns = 0u64;
        let mut readback_ns = 0u64;
        while done < count {
            let take = per_chunk.min(count - done);
            // **★ 不要再拷一份 `Vec<f32>` ✗ ★**（**第 496 轮 ✓）
            //   **∴ 原来 ✗**：`let chunk = pixels[…].to_vec();` ＋ `f32s_to_bytes(&chunk)`
            //     ⇒ **∴ 于是**：**同一段像素**存在**两份**（f32 一份 ＋ bytes 一份 ✓）
            //       ⇒ **∴ 4K 一块时**＝ 132.7 MiB ＋ 132.7 MiB ＝ **265 MiB** ✓
            //         ＋ **∴ 而那**正是守卫要防的形状** ✓ ★**** ✓✓
            //   **∴ 现在 ✗**：**直接从 `pixels` 的切片生成 bytes** ✓
            //     ⇒ **∴ 峰值**只剩 bytes 那一份** ✓ ★**** ✓✓
            let src_bytes = f32s_to_bytes(&pixels[done * 4..(done + take) * 4]);
            // **∴ 分段①：上行（**建 buffer ＋ 写 buffer ✓）★
            let t_up = Instant::now();
            let src = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: src_bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let dst = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (take * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            // **★ uniform 必须** 32 字节** ✗ ★**（**第 459 轮 ✓）：
            //   **∴ 布局 ✗**：**4×u32（16 B）＋ `vec4<f32>`（16 B，**偏移 16 ✓）
            //     ⇒ **∴ 与** WGSL 的 `struct Params` **逐字段对应** ✓ ★**** ✓✓
            let mut params_bytes = u32s_to_bytes(&[
                take as u32,
                self.lut_len() as u32,
                u32::from(background.is_some()),
                0, // **∴ 对齐填充 ✓**
            ]);
            match background {
                Some(bg) => params_bytes.extend_from_slice(&f32s_to_bytes(&bg)),
                None => params_bytes.extend_from_slice(&f32s_to_bytes(&[0.0, 0.0, 0.0, 0.0])),
            }
            debug_assert_eq!(params_bytes.len(), 32);
            // **★ uniform 尺寸必须** 32 字节** ✗ ★**（**第 459 轮修正 ✓）：
            //   **∴ 症状（**实测 ✓）✗**：**`Queue::write_buffer` 报
            //     「**Copy of 0..32 would end up overrunning the bounds of the
            //       Destination buffer of size 16**」**✗
            //     ⇒ **∴ 因为**加了 `vec4<f32>` 后**要 32 字节**✗
            //       ⇒ **∴ 而**分配还是 16** ✓
            //         ⇒ **∴ 于是**：**服务端**启动即崩** ✓ ★**** ✓✓
            //   **∴ 现在 ✗**：**与** WGSL 的 `struct Params`（**4×u32 ＋ vec4<f32> ✓）一致** ✓ ★**** ✓✓
            let pb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let back = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (take * 4) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&src, 0, &src_bytes);
            self.queue.write_buffer(&pb, 0, &params_bytes);
            upload_ns += t_up.elapsed().as_nanos() as u64;
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: src.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: dst.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.lut_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: pb.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups((take as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&dst, 0, &back, 0, (take * 4) as u64);
            backs.push((back, take));
            done += take;
        }
        // **★ 一次提交 ＋ 一次等待 ✗ ★**（**∴ 第 284 轮：**快 88.6% ✓）
        // **∴ 分段②：提交（**submit ＋ 一次 poll ✓）★
        let t_sub = Instant::now();
        self.queue.submit(Some(enc.finish()));
        // **★ 不阻塞 ✗ ★**（**第 471 轮 ✓）：**真正的同步在下面 `map_async` 的 await ✓
        let _ = self.device.poll(wgpu::PollType::Poll);
        submit_ns += t_sub.elapsed().as_nanos() as u64;

        // **∴ 分段③：读回（**逐块 map ＋ 等待 ✓）★
        let t_rb = Instant::now();
        let mut out: Vec<u8> = Vec::with_capacity(count * 4);
        for (back, _take) in &backs {
            let slice = back.slice(..);
            let signal = std::sync::Arc::new(OnceSignal::new(&self.device));
            let signal_in_cb = std::sync::Arc::clone(&signal);
            slice.map_async(wgpu::MapMode::Read, move |_r| {
                signal_in_cb.signal();
            });
            let _ = self.device.poll(wgpu::PollType::Poll);
            signal.wait().await;

            let mapped = slice.get_mapped_range();
            out.extend_from_slice(&mapped);
            drop(mapped);
            back.unmap();
        }
        readback_ns += t_rb.elapsed().as_nanos() as u64;
        // **∴ 写静态 ✗**：**每次覆盖 ⇒ 读的就是**最近一次** ✓
        STAGE_UPLOAD_NS.store(upload_ns, std::sync::atomic::Ordering::Relaxed);
        STAGE_SUBMIT_NS.store(submit_ns, std::sync::atomic::Ordering::Relaxed);
        STAGE_READBACK_NS.store(readback_ns, std::sync::atomic::Ordering::Relaxed);
        STAGE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(out)
    }

    fn lut_len(&self) -> usize {
        (self.lut_buf.size() / 4) as usize
    }
}

/// **∴ 极简 `block_on` ✗**（**∴ 不引入 `pollster` ✓）
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    use std::task::{Context, Poll};
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut fut = Box::pin(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// **∴ 安全字节转换 ✗**（**∴ 本 crate**禁 `unsafe`** ✓）
fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// **∴ `u32` 版 ✗**
fn u32s_to_bytes(v: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// **∴ 五步量化（**与 CPU 真值同一套 ✓）✗** —— **∴ 与 `yanshi-http` 的共享文件**同一份** ✓
const SHADER: &str = include_str!("quantize.wgsl");

/// **★ 极简 oneshot（**自带推进 ✓）✗ ★**（**第 471 轮 ✓）
///
/// **∴ 为什么需要它 ✗**：**`map_async` 的回调**在**别的线程／微任务**里跑**✗
///   ⇒ **∴ 而**同步版**原来用 `mpsc::recv()` 阻塞等** ✓
///     ⇒ **∴ 而在 wasm 上**阻塞**会**卡住事件循环** ✓
///
/// **★ 关键（**第 470 轮的教训 ✓）★**：**`device.poll(PollType::Poll)` **不阻塞**✗
///   ⇒ **∴ 所以**：**必须有东西**反复推进** ✓
///     ⇒ **∴ 而** `block_on` **只 poll 我们的 future**✗
///       ⇒ **∴ 它**不推进 `wgpu` 的内部队列** ✓
///         ⇒ **★ 因此**：**本 future **每被 poll 一次**就**推进一次 `device`**** ✓
///           ⇒ **∴ 于是**：**两平台**都能工作** ✓
///             （**∴ 同步：**`block_on` 反复 poll ✓；**wasm：**事件循环唤醒 ✓）★**** ✓✓
struct OnceSignal {
    /// **∴ 拥有一个 `Device` 克隆 ✗**（**∴ `wgpu::Device` 内部是 Arc ⇒ 克隆很便宜 ✓）
    ///   ⇒ **∴ 于是**：**回调闭包**可以**'static** ✗（**∴ 不借用 self ✓）★**** ✓✓
    device: wgpu::Device,
    waker: std::sync::Mutex<Option<std::task::Waker>>,
    ready: std::sync::atomic::AtomicBool,
}

impl OnceSignal {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            waker: std::sync::Mutex::new(None),
            ready: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// **∴ 由回调调用 ✗**（**∴ 置位 ＋ 唤醒 ✓）
    fn signal(&self) {
        self.ready.store(true, std::sync::atomic::Ordering::Release);
        if let Ok(mut slot) = self.waker.lock() {
            if let Some(waker) = slot.take() {
                waker.wake();
            }
        }
    }

    /// **★ 等它被唤醒（**并且自己推进 device ✓）✗ ★**
    async fn wait(&self) {
        std::future::poll_fn(|cx| {
            if self.ready.load(std::sync::atomic::Ordering::Acquire) {
                return std::task::Poll::Ready(());
            }
            // **★ 每轮都推进一次 ✗ ★**（**第 471 轮 ✓；**∴ 那是**本 future**的职责** ✓）
            let _ = self.device.poll(wgpu::PollType::Poll);
            if let Ok(mut slot) = self.waker.lock() {
                *slot = Some(cx.waker().clone());
            }
            std::task::Poll::Pending
        })
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::{should_use_gpu_on, threshold_for, DeviceClass, GPU_MIN_PIXELS};

    /// **★ 两档阈值都要钉住 ✗ ★**（第 320 轮 ✓）：
    ///   **∴ 变异点 ✗**：**把 `Discrete` 改成 30_000**✗ ⇒ **∴ 本断言**必红** ✓**
    ///   **∴ 且**：**把 `Cpu` 改成 10_000**✗ ⇒ **∴ 本断言**必红** ✓（**∴ 那**正是**第 318 轮的错** ✓）**
    #[test]
    fn thresholds_follow_the_adapter_class() {
        assert_eq!(
            threshold_for(DeviceClass::Discrete),
            10_000,
            "独显按 L4 的平衡点"
        );
        assert_eq!(threshold_for(DeviceClass::Integrated), GPU_MIN_PIXELS);
        assert_eq!(threshold_for(DeviceClass::Virtual), GPU_MIN_PIXELS);
        // **★ 软件渲染**绝不**享受激进阈值** ✓（**∴ 第 318 轮的教训 ✓）
        assert_eq!(
            threshold_for(DeviceClass::Cpu),
            GPU_MIN_PIXELS,
            "llvmpipe 不是加速"
        );
        assert_eq!(threshold_for(DeviceClass::Other), GPU_MIN_PIXELS);
        // **∴ 且**：**独显那档**必须**严格小于**保守档**
        assert!(threshold_for(DeviceClass::Discrete) < GPU_MIN_PIXELS);
    }

    /// **∴ 边界是闭的 ✗**（**∴ 与**旧判据同一形状 ✓）
    #[test]
    fn boundaries_are_closed() {
        assert!(should_use_gpu_on(DeviceClass::Discrete, 10_000));
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 9_999));
        assert!(should_use_gpu_on(DeviceClass::Integrated, GPU_MIN_PIXELS));
        assert!(!should_use_gpu_on(
            DeviceClass::Integrated,
            GPU_MIN_PIXELS - 1
        ));
        // **∴ 而在** 30 000 附近，独显与集显**结论不同**✗ ⇒ **∴ 那**正是分档的意义** ✓
        assert!(should_use_gpu_on(DeviceClass::Discrete, 30_000));
        assert!(should_use_gpu_on(DeviceClass::Integrated, 30_000));
        assert!(should_use_gpu_on(DeviceClass::Discrete, 12_000));
        assert!(!should_use_gpu_on(DeviceClass::Integrated, 12_000));
    }
}
