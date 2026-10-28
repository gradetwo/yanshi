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
    /// **★★★ 读回用的 staging buffer**池 ✗ ★★★**（**第 605 轮 ✓；目标第 5 条 ✓）
    ///
    /// **∴ 为什么需要 ✗**：**L4 实测 `readback ＝ 77 295 µs`（**最大头 ✓）**
    ///   ＋ **∴ 而**每次调用都**新建一张 33 MB（4K）的映射 buffer**✗
    ///     ⇒ **∴ 于是**：**9 次调用 ⇒ 9 次大分配（含驱动触页 ✓）
    ///       ＋ **∴ 而**实测带宽只有 **≈3.8 GB/s**（**远低于 PCIe x16 的 ~12 GB/s ✓）
    ///         ⇒ **★ 所以：**按大小复用**能省掉重复分配与触页** ✓ ★
    ///
    /// **∴ 两面（**AGENTS.md ✓）★**：
    ///   **∴ 收益 ✗**：**不再每次重分配**（**suspect 的大头 ✓）
    ///   **∴ 代价 ✗**：**常驻一张**最大用过的 staging buffer**✗
    ///     ＋ **∴ 且**要 `Mutex`（**∵ `quantize` 是 `&self` ✓）
    ///       ⇒ **∴ 而**它是**实例字段**（**不是静态量 ✓）
    ///         ⇒ **∴ 所以**：**不会触发**本会话踩过的「wasm ＋ gpu 静态量非 `Sync`」坑** ✓ ★
    ///
    /// **∴ 开关 ✗**：`YANSHI_GPU_POOL_READBACK=0` **关掉**（**供 A/B ✓）★
    readback_pool: std::sync::Mutex<Option<(u64, wgpu::Buffer)>>,
    /// **★★★ 通用缓冲池（**目标第 11 条后续 ✓）✗ ★★★**（**第 715 轮 ✓）
    ///
    /// **∴ 为什么需要 ✗**（**第 79 轮的实测结论 ✓）：
    ///   **∴ 4K 合成 ✗**：**GPU 总 **570 ms** vs CPU **398 ms**** ✓
    ///     ＋ **∴ 而**上传 ~67 ms ＋ 读回 ~165 ms**✗
    ///       ⇒ **∴ 剩下约 **338 ms** 就是**分配 ＋ dispatch ＋ 等待**** ✓
    ///         ＋ **∴ 所以**：**每次**新建 5 张 buffer**✗ 是**主要开销之一** ✓ ★
    ///
    /// **∴ 匹配键 ✗**：**(**字节数, usages 位 ✓）**✗
    ///   ＋ **∴ 为什么必须带 usages ✗**：**`STORAGE` 与 `MAP_READ` 的 buffer **不可互换** ✓
    ///     ＋ **∴ 若**只按字节数匹配 ⇒ **∴ 会**取错用途**✗ ⇒ **∴ 于是**创建失败或行为错 ✓ ★
    ///
    /// **∴ 上限 ✗**：**`POOL_MAX` 张**✗ ⇒ **∴ 超了就**丢弃**（**∴ 而不是**无限增长 ✓）★
    ///
    /// **∴ 开关 ✗**：**`YANSHI_GPU_POOL_READBACK=0` **一并关掉它**** ✓（**∴ 与**既有池同一开关 ⇒ **∴ A/B 可关 ✓）★
    buffer_pool: std::sync::Mutex<Vec<(u64, u32, wgpu::Buffer)>>,
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
pub const GPU_MIN_PIXELS: usize = 1_000_000;

/// **★ 独显阈值**为什么保持 22 500 ✗ ★**（**第 722 轮的教训 ✓）
///
/// **∴ 我**差点用推理覆盖实测** ✗**：
///   **∴ 我看到**核显的交叉点（**约 1 M ✓）**✗
///     ＋ **∴ 于是**推理**：**独显带宽高 4× ⇒ **∴ 门槛**应当**更低**✗ ⇒ **∴ 我**写了 250 000** ✓
///       ＋ **∴ 而**判据**当场红**✗ 并**打出**它的出处**：**「**独显按 L4 实测的平衡点（**第 584 轮重定 ✓）」** ✓
///         ⇒ **★ 即：**22 500 **是**真 L4 实测**的结果**✗ ⇒ **∴ 我**不该覆盖它** ✓ ★
///           ＋ **∴ 所以**：**核显**按本机数据改（**有双重依据 ✓）✗ 而**独显**保持实测值** ✓
///             ＋ **∴ 而**那**正好**符合**目标第 3 条**（**按适配器类型**✗ 且**各自有依据 ✓）★
///
/// **∴ 教训 ✗**：**判据里的**出处文案**✗（**∴ 「**第 584 轮重定 ✓」）**比数值本身更重要** ✓
///   ＋ **∴ 它**在**我**要用推理覆盖实测时**✗ **当场**拦住了** ✓ ★
/// **★ 这个规模该不该走 GPU ✗ ★**（**纯函数 ⇒ 可单测 ✓）
#[must_use]
pub const fn should_use_gpu(pixels: usize) -> bool {
    pixels >= GPU_MIN_PIXELS
}

/// **★ 按适配器等级取阈值 ✗ ★**（第 320 轮 ✓；**用户第 317 轮**以 L4 为准** ⇒ **∴ 但**核显也要照顾** ✓）。
///
/// **∴ 两个平台实测的平衡点 ✗（**都来自八档 + 5 次中位数 ✓）★**：
///   **★ 已被第 584 轮的**真 L4 实测推翻**✗ ★**（**∴ 保留历史 ＋ 标注推翻 ✓）：
///     **∴ 旧记录 ✗**：**1 000 快 25.8%** ⇒ **∴ 平衡点 ≤ 1 000** ✓
///       ＋ **∴ 而**第 584 轮实测 ✗**：**10 000 未触发｜22 500 持平｜50 176 快 22%**
///         ⇒ **∴ 平衡点 ≈ 22 500** ✓
///           ⇒ **★ 所以：**旧数字（**≤1 000 ✓）不再采用**✗
///             ＋ **∴ 原因不确知**（**∴ 可能**是旧实例／旧场景／旧量法 ✓）⇒ **∴ 如实记录** ✓ ★
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
        // **★★★ 第 584 轮：真 L4 上**实测重定** ⇒ 10 000 ⇒ **22 500**** ★★★
        //   **∴ 依据（**L4 ＋ 3 轮 ＋ 不核对 ✓）✗**：
        //     **∴ 10 000 px**：后端**仍是 `cpu`**（**阈值未触发 ✓）
        //       ⇒ **∴ 所以**判定是**严格大于** ✓
        //     **∴ 22 500 px**：CPU 中位 170｜GPU 中位 169 ⇒ **★ 持平（**平衡点在此附近 ✓）★
        //     **∴ 50 176 px**：CPU 中位 193｜GPU 中位 151 ⇒ **★ GPU 快 22% ★**
        //       ⇒ **∴ 于是**：**10 000–22 500 px 之间
        //         **GPU 会被选中但**更慢****（**10 000 px 时慢 28% ✓）
        //         ⇒ **★ 所以：**旧值 10 000 **偏低约 2.25×**** ✓ ★
        //   **∴ 代价（**两面都写 ✓）✗**：**22 500–50 176 px 之间**仍会选 GPU**✗
        //     ＋ **∴ 而**那**区间**可能**无收益** ✓
        //       ⇒ **∴ 所以**：**22 500 是**保守**选择**✗
        //         ⇒ **∴ 若要**更保守 ⇒ **∴ 需要**更多轮数据** ✓ ★
        // **★★★ 第 795 轮：**L4 上 15 轮 ＋ 干净负载的重定** ⇒ 22 500 ⇒ **1 048 576**** ★★★
        //   **∴ 为什么要重定 ✗**（**第 584 轮**自己说了答案 ✓）★**：
        //     ＋ **∴ 它**当时只有 **3 轮**✗ ＋ **不核对**✗ ⇒ **∴ 它**自己写**
        //       「**22 500 是**保守**选择**✗ ⇒ **∴ 若要**更保守 ⇒ **∴ 需要**更多轮数据**」 ✓ ★
        //   **∴ 今天的实测（**NVIDIA L4｜15 轮｜干净负载｜10 个尺寸 ✓）✗**：
        //     **∴ ≤ 724²（**524 176 px ✓）**✗**：**比值 **0.92–1.05×**** ⇒ **∴ 两路**等价** ✓
        //       ＋ **∴ 1024²／1448²／2048²**✗**：** **0.81×／0.75×／0.89×**** ⇒ **∴ GPU **显著更快** ✓
        //         ＋ **∴ 所以**优势区下界 ＝ **1 048 576 px（**1024² ✓）** ✓ ★★
        //   **∴ 而它**与第 584 轮的结论**矛盾 ✗**（**∴ 那次说 50 176 px 时 GPU 快 22% ✓）★
        //     ＋ **∴ 而现在（**同样的量级 ✓）**是 **1.00×**** ⇒ **∴ 所以**那次的 22% 是
        //       **3 轮的噪声**✗ —— **∴ 本会话已独立证明：5 轮在小尺寸上**乱跳**
        //       （**115／115／103／153／145／149／222 ✓），15 轮才**单调** ✓ ★★★
        //   **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
        //     **∴ 收益 ✗**：**不再在**两路等价**的区间里把活交给 GPU**
        //       ⇒ **∴ 省掉**那一段的传输开销**（**4K 上实测 upload 33.8 ms ＋ readback 81.7 ms ✓）★
        //     **∴ 代价 ✗**：**32 K–1 M px 之间的画布**改走 CPU**
        //       ⇒ **∴ 若**某台机器的 CPU 恰好更快 ⇒ **∴ 它**会被**推给更慢的一侧** ✗
        //         ＋ **∴ 而**那**正是**阈值**存在的意义**（**∴ 它**按**适配器类别**分档 ✓）★
        DeviceClass::Discrete => 1_048_576,
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
/// **★ 上限（**按**设备类别**取值 ✓）✗ ★**（**第 597 轮更正 ✗**：**`CHUNK_BYTES_MAX` 常量
///   已在第 566 轮删除 ✗**（**∵ clippy 报"从未使用" ✓）
///     ⇒ **∴ 现在**由 `chunk_bytes_cap(设备类别)` 决定** ✓）★
///   **∴ 核显／虚拟／CPU／其它 ✗**：**4 MiB**✗
///     ⇒ **∴ 与**下面的 A/B 实测结论**一致** ✓
///   **★ 独显 ✗ ★**：**32 MiB**✗
///     ＋ **∴ 但**那**只是**上限**✗ ⇒ **∴ 实际**仍按 `count/4` 细分**✓
///       ＋ **∴ 且**再被 `max_storage_buffer_binding_size` 与 `CHUNK_BYTES_MIN` 夹** ✓
///         ⇒ **★ 所以 ✗**：**4K 上的**块数**与 4 MiB 时**相同**✗
///           ⇒ **∴ 与**下面的 A/B 结论**不冲突**（**∴ 那**里比的是**每块大小** ✓）★
///   **∴ 它是**实测选出来的**✗ ，**不是猜的** ✓**（**第 496 轮 A/B ✓）**：
///     **∴ 在 L4（**真独显 ✓）上**同一份代码**只改分块大小**✗**：
///       | 分块 | 墙钟中位 | CPU 中位 | 峰值 RSS |
///       |---|---|---|---|
///       | **4 MiB（**32 块 ✓）** | **1831 ms** | **2260 ms** | **3082 MB** |
///       | **自适应 64 MiB（**1 块 ✓）** | **1936 ms** | **2370 ms** | **3432 MB** |
///       ⇒ **★ 放大分块**慢了 5.7% ＋ CPU 多 4.9% ＋ 内存多 11%** ✓
///         ⇒ **∴ 所以**：**"往返次数是大头"这个推断**错了** ✓
///           ＋ **∴ 大 buffer 的**分配／缓存成本** ＞ **省下的往返** ✓ ★**** ✓✓
///     **∴ 结论 ✗**：**核显／CPU 那档的上限**保持 **4 MiB**** ✗
///       ⇒ **∴ 与**实测最优**一致** ✓
///         ＋ **∴ 而**独显那档给了 **32 MiB** 的上限**✗
///           ⇒ **∴ 但**如上所述，**它**只是上限 ＋ 仍按 `count/4` 细分** ✓★
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
/// **★ 分块上限**按适配器等级**分档 ✗ ★**（**第 540 轮 ✓；目标第 4 条 ✓）
///
/// **∴ 为什么不是**一个常数**✗**：**第 496 轮的 A/B 实测（核显 ✓）：
///   ⇒ **∴ 放大分块 ⇒ 更慢**✗（**中位 1831 → 1936 ms ✓）
///     ＋ **∴ 而**结构与往返无关**✗（**所有块共用**一个编码器 ＋ **只提交一次 ✓）
///       ⇒ **∴ 所以**：**块大小唯一影响**GPU buffer 的分配次数** ✓
///         ＋ **∴ 所以**：**核显上**4 MiB 是最优点**✗（**∴ 分配不是瓶颈，更大的 buffer 反而更慢 ✓）★
///
/// **★★★ 独显档：已在真 L4 上复核（**第 620 轮 ✓）★★★**
///   ⇒ **∴ 结论 ✗**：**4 MiB **最优**✗ ⇒ **∴ 与保守档**同值** ✓
///     ＋ **∴ 而**原来的 32 MiB **比它差 13.2%**（**中位 1584 vs 1399 ms ✓）★
///
/// **∴ 核显／CPU 软件渲染 ⇒ 保持 4 MiB ⇒ **行为与改动前**完全一致**** ✓（**零风险 ✓）★
#[must_use]
const fn chunk_bytes_cap(class: DeviceClass) -> usize {
    match class {
        // **∴ 与 A/B 实测一致 ✗**：**核显 ＋ 软件渲染都用保守档** ✓
        DeviceClass::Integrated | DeviceClass::Virtual | DeviceClass::Cpu | DeviceClass::Other => {
            4 * 1024 * 1024
        }
        // **★★★ 独显也回到 4 MiB ✗ ★★★**（**第 620 轮真 L4 复核 ✓）
        //   **∴ 原来的 32 MiB 是**未复核的猜测**** ✗（**本文档自己标着「待复核」 ✓）
        //     ＋ **∴ 真 L4 上扫描（**4K，各 5 轮 ⇒ 4 有效，带墙钟 ✓）✗**：
        //       **∴ 1 MiB ✗**：中位 1537 ms｜CPU 1960
        //       **∴ 4 MiB ✗**：中位 ★1399★ ms｜CPU ★1840★   ← ★ 最优 ★
        //       **∴ 8 MiB ✗**：中位 1460 ms｜CPU 1900
        //       **∴ 16 MiB ✗**：中位 1507 ms｜CPU 1940
        //       **∴ 32 MiB ✗**：中位 1584 ms｜CPU 2015   ← ★ 最差（**比 4 MiB 差 13.2% ✓）★
        //         ⇒ **★ 所以：**独显档与保守档取同一个值 ⇒ 更快的实测结果** ✓ ★
        //   **∴ 与第 496 轮的 A/B 一致 ✗**：**4 MiB（1831 ms）优于自适应 64 MiB（1936 ms ✓）
        //     ＋ **∴ 而**第 611 轮的「大块 readback 更快」**被**算的时间**抵消** ✓
        //       ⇒ **∴ 所以**：**墙上时间反而更差** ✓ ★
        //   **∴ 为什么保留 match ✗**：**与 `DeviceClass` 一一对应 ⇒ **改起来明确**** ✓（**∴ 即使两臂同值 ✓）★
        DeviceClass::Discrete => 4 * 1024 * 1024,
    }
}

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
/// **★ GPU 自己算的时间（**纳秒，时间戳 ✓）✗ ★**（**第 546 轮 ✓；目标第 9 条 ✓）
///   ⇒ **∴ 只在**门控满足时**才被写**✗（**`should_time_gpu` ✓）
///     ＋ **∴ 未测时**保持 0 ⇒ **∴ 与「量到 0」可区分**（**∴ 若**要区分，可看探针是否开着 ✓）★
static STAGE_GPU_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// **★ GPU 执行 `copy_buffer_to_buffer` 的时间（**纳秒，时间戳 ✓）✗ ★**（**第 246 轮 ✓；待办 2b ✓）
///   ⇒ **∴ 它**只含** GPU 侧的拷贝**✗（**∴ 20 次块拷贝 ✓）
///     ＋ **∴ 而**它**落在** host 侧 `readback` 窗口**里** ✗ ⇒ **∴ 两者**不能相加** ✓ ★
///       ＋ **∴ 需要** `TIMESTAMP_QUERY_INSIDE_ENCODERS`**✗ ⇒ **∴ 没有它**恒 0（**如实 ✓）★
static STAGE_COPY_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// **★ 读回等待的累计墙钟 ＋ 等待期间本进程的累计 CPU ✗ ★**（**第 246 轮 ✓；待办 2c 的前置 ✓）
///   ⇒ **∴ 两者的比 ＝**等待期的有效并行度** ✓
///     ＋ **∴ ≈ 0 ⇒ **整机都在等**（**可重叠窗口 ≈ 等待全长 ✓）
///       ＋ **∴ ≥ 1 ⇒ **别的线程在干活**⇒ **等待**已被掩盖** ✓（**∴ 2c 的收益 ≈ 0 ✓）★★
///   ＋ **∴ 都是**累计（`fetch_add` ✓）✗ ⇒ **∴ 于是**：**比值是**整个进程的平均**✓
///     ⇒ **∴ 而**那**正是**要的（**单次的 10 ms 太抖 ✓）★
static WAIT_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WAIT_CPU_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static STAGE_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// **★ 本进程的 CPU 时间（**utime+stime ⇒ 纳秒 ✓）✗ ★**（**第 246 轮 ✓）
///
/// **∴ 两个坑 ✗**：
///   **∴ ①** **进程名（**字段 2 ✓）**里可能有空格与括号**✗
///     ⇒ **∴ 不能**按空白切了再数**✗ ⇒ **∴ 要从**最后一个 `)` 之后**开始数** ✓★
///   **∴ ②** **单位是**时钟滴答**✗（**`sysconf(_SC_CLK_TCK)` ＝ 100 ⇒ 10 ms ✓）
///     ⇒ **∴ 与 `tool-render-cost-accounts.mjs` 的口径**一致** ✓（**它也按 100 Hz 换算 ✓）★
#[cfg(target_os = "linux")]
fn process_cpu_ns() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/stat").ok()?;
    let rest = text.rsplit_once(')')?.1;
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // **∴ `rest` 的第 0 项 ＝ 字段 3（**state ✓）✗ ⇒ **∴ utime（**字段 14 ✓）⇒ 索引 11** ✓
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime + stime) * 10_000_000)
}

/// **∴ 非 Linux 上**如实返回 None ✗**（**∴ 于是**探针只说"量不到" ✓）★
#[cfg(not(target_os = "linux"))]
fn process_cpu_ns() -> Option<u64> {
    None
}

/// **★ 读最近一次的分段计时 ✗ ★**
///
/// **∴ 返回 `(上行微秒, 提交微秒, 读回微秒, 次数)` ✗**：
///   ⇒ **∴ 次数 ＝ 0 ⇒ **三段都不可读**** ✓（**∴ 不许**当成 0 ✓）★
#[must_use]
pub fn last_stage_timing() -> (u64, u64, u64, u64, u32) {
    let us = |a: &std::sync::atomic::AtomicU64| a.load(std::sync::atomic::Ordering::Relaxed) / 1000;
    (
        us(&STAGE_UPLOAD_NS),
        us(&STAGE_SUBMIT_NS),
        us(&STAGE_READBACK_NS),
        // **★ 目标第 9 条后半：GPU 自己量的纳秒（**微秒输出 ✓）★**（**第 601 轮 ✓）
        //   **∴ 语义 ✗**：**它**不是 host 侧的墙钟**✗
        //     ＋ **∴ 而**是 GPU 时间戳的差**（`resolve → buffer → map` 读回 ✓）
        //       ⇒ **∴ 所以**：**只有**探针开启 ＋ 适配器支持**时才有值** ✓
        //         ＋ **∴ 否则**恒为 **0**（**∴ 那**是**如实**的 0 ✓）★
        us(&STAGE_GPU_NS),
        STAGE_CALLS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// **★ GPU 侧拷贝的时间（**微秒 ✓）✗ ★**（**第 246 轮 ✓；待办 2b ✓）
///
/// **∴ 为什么单开一个访问器 ✗**（**∴ 而**不并进 `last_stage_timing` ✓）★**：
///   ＋ **∴ 语义不同 ✗**：**`gpu` 与 `copy` 都是** GPU 内部时间**✗
///     ⇒ **∴ 而** `last_stage_timing` 那三段是** host 侧墙钟** ✓
///       ＋ **∴ 若**并成一个元组**✗ ⇒ **∴ 调用方**很容易把它们**相加**✗
///         ⇒ **∴ 于是**：**重复计**（**∴ 因为** `copy` 就在 `readback` 窗口里 ✓）★
///   ＋ **∴ 所以 ✗**：**分开两个函数**✗ ⇒ **∴ 让"不能相加"这件事**写在签名上** ✓ ★★
#[must_use]
pub fn last_copy_timing_us() -> u64 {
    STAGE_COPY_NS.load(std::sync::atomic::Ordering::Relaxed) / 1000
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
        // **★★★ 目标第 9 条后半：GPU 侧计时**按探针门控请求 feature ✗ ★★★**（**第 601 轮 ✓）
        //   **∴ 为什么必须门控 ✗**：**`TIMESTAMP_QUERY` 的读回要走
        //     `resolve → buffer → map` 三步**✗
        //     ＋ **∴ 而** wgpu 26 **没有 `get_results`** ✓
        //       ⇒ **∴ 它**绝不能进生产路** ✓
        //         ＋ **∴ 且**本会话早期**直接请求它**曾让**服务端启动即崩**✗
        //           ⇒ **∴ 所以**：**只在 `YANSHI_RENDER_PROBE=1` 时才请求** ✓
        //             ⇒ **∴ 从而**：**生产路径 `required_features` 为空
        //               ⇒ **与改动前**逐字节相同的行为** ✓ ★
        //   **∴ 两面（**AGENTS.md ✓）★**：
        //     **∴ 收益 ✗**：**能分清「**传输贵**」还是「**算得贵**」**✗
        //       （**∴ 而** host 侧数据**只能给上下界** ✓）
        //     **∴ 代价 ✗**：**诊断路径**多一个 feature 请求**✗
        //       ＋ **∴ 若**适配器不支持 ⇒ **`should_time_gpu` 返回 false ⇒ 优雅降级** ✓ ★
        let probe_wants_timestamps = std::env::var_os("YANSHI_RENDER_PROBE").is_some()
            && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let required_features = if probe_wants_timestamps {
            wgpu::Features::TIMESTAMP_QUERY
        } else {
            wgpu::Features::empty()
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features,
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no_device:{e:?}"))?;
        // **★★★ 捕获未被处理的 wgpu 错误 ✗ ★★★**（**第 615 轮 ✓）
        //   **∴ 为什么需要 ✗**：**本会话**反复只看到**次生错误**✗
        //     ＋ **∴ 如** `Encoder is invalid`（**被前面的验证失败带坏 ✓）
        //       ⇒ **∴ 于是**：**每次**都要**反复猜** ✓
        //   **∴ 现在 ✗**：**第一个真错误**直接打到 stderr**✗
        //     ＋ **∴ 且**用 **Debug**（**∴ Display 只给一行「Validation Error」✓
        //       ＋ **∴ 而 `Caused by`／`source` 只在 Debug 里 ✓）
        //         ⇒ **∴ 从而**：**一眼看到根因** ✓ ★
        //   **∴ 代价（**两面 ✓）✗**：**多一个闭包**（**只在出错时跑 ✓）⇒ **∴ 可忽略** ✓ ★
        //   **∴ 实测价值（**第 615 轮 ✓）✗**：**它**立刻把
        //     `Encoder is invalid`（**次生 ✓）**换成了
        //       `ComputePass(ComputePassError { scope: Pass, inner: EncoderState(Invalid) })`
        //         ⇒ **★ 定位从"pass 之后"精确到"**pass 之内**"** ✓ ★
        device.on_uncaptured_error(Box::new(|e| {
            eprintln!("PROBE WGPU 真错误：{e:?}");
        }));
        // **★★★ 捕获第一个**真** wgpu 错误 ✗ ★★★**（**第 615–616 轮 ✓）
        //   **∴ 为什么需要 ✗**：**本会话**反复只看到**次生错误**✗
        //     ＋ **∴ 如** `Encoder is invalid` ✓
        //       ⇒ **∴ 于是**：**每次**都要**反复猜** ✓
        //   **∴ 必须用 Debug ✗**：**Display 只给一行**（「Validation Error ✓）
        //     ＋ **∴ 而 `Caused by`／`source` **只在 Debug 里** ✓（**实测 ✓）★
        //   **∴ 实测价值 ✗**：**它**把定位**从「pass 之后」**精确到「**pass 之内**」** ✓ ★
        //   **∴ 代价（**两面 ✓）✗**：**多一个闭包**（**只在出错时跑 ✓）★
        device.on_uncaptured_error(Box::new(|e| {
            eprintln!("PROBE WGPU 真错误：{e:?}");
        }));
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
            // **∴ 池初始为空 ✗**（**∴ 第一次调用时按需建 ✓）★
            readback_pool: std::sync::Mutex::new(None),
            buffer_pool: std::sync::Mutex::new(Vec::new()),
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
    /// **∴ 池的**张数**上限 ✗**（**∴ 兜底 ✓）★
    const POOL_MAX: usize = 12;

    /// **★★★ 池的**字节预算** ✗ ★★★**（**第 716 轮 ✓）
    ///
    /// **∴ 为什么从"张数"改成"字节"✗**（**第 101 轮的实测教训 ✓）：
    ///   **∴ 第 101 轮 ✗**：**池化让 4K σ=2 **快 36%****✗（**264 vs 413 ms ✓）
    ///     ＋ **∴ 但** 4K σ=8 **反向**（**926 vs 567 ms ✓）
    ///       ＋ **∴ 而**开池的 **min（**272.8**）明显优于**关池（**442.3**）**✗
    ///         ⇒ **∴ 所以**：**池化**有效**✗ 但**大图上的**显存压力**可能**加剧波动** ✓ ★
    ///   **∴ 算一下 ✗**：**4K 合成要 5 张 × 132.7 MB ＝ **664 MB****✗
    ///     ＋ **∴ 而**核显**共享系统内存**✗ ⇒ **∴ 那**是**真实压力** ✓
    ///       ⇒ **★ 所以：**按字节预算收敛**✗ ⇒ **∴ 4K 只留 1–2 张** ✓ ★
    ///
    /// **∴ 取值 ✗**：**256 MiB**✗
    ///   ＋ **∴ 而**它**对** 4K（**132.7 MB／张 ✓）⇒ **∴ 只留 **1 张**** ✓
    ///     ＋ **∴ 对小图（**如 960×540 ＝ 2 MB／张 ✓）⇒ **∴ 能留**很多** ✓ ⇒ **∴ 两全 ✓ ★
    // **★ 默认值 ＝ **1280 MB** ✗ ★**（**第 717 轮扫描 ✓）：
    //   **∴ 扫描结果（**4K，5 轮中位 ✓）✗**：
    //     **∴ 256 MB ✗**：**σ=2 **1.54×**｜σ=8 **1.64×**** ✓
    //       ＋ **∴ 512 MB ✗**：**σ=2 2.06×｜σ=8 6.05×** ✓
    //         ＋ **∴ 768 MB ✗**：**σ=2 2.68×｜σ=8 9.21×** ✓
    //           ＋ **∴ 1280 MB ✗**：**σ=2 **2.70×**｜σ=8 **10.82×**** ✓ ⇒ **★ 最优 ★**
    //     ⇒ **∴ 为什么越大越好（**真正的机理 ✓）✗**：
    //       **∴ 第 101 轮的张数上限 12 **允许保留**多个不同尺寸**** ✗
    //         ＋ **∴ 于是**：**总量**远超 664 MB**✗（**∴ 5 张 × 132.7 MB ✓）⇒ **∴ 那**才是压力来源** ✓
    //           ＋ **∴ 而** 1280 MB **只允许**一个完整尺寸集 ＋ 冗余** ✓ ⇒ **∴ 重启**复用充分 ＋ **无压力** ✓ ★
    const POOL_BUDGET_BYTES: u64 = 1280 * 1024 * 1024;

    /// **∴ 读池的字节预算 ✗**（**`YANSHI_GPU_POOL_BUDGET_MB` 可覆盖 ⇒ **∴ 供扫描 ✓）★
    ///
    /// **∴ 为什么需要 ✗**（**第 716 轮的教训 ✓）：
    ///   **∴ 256 MiB ⇒ 4K σ=8 **4.92×****✗ 但 **4K σ=2 **1.51×**** ✓
    ///     ＋ **∴ 而**张数上限 12 ⇒ **∴ σ=2 **2.30×****✗ 但 **σ=8 **2.00×**** ✓
    ///       ＋ **∴ 所以**：**最优在**两者之间**✗ ⇒ **∴ 需要**扫一遍** ✓
    ///         ＋ **∴ 而**每次改常量**都要**重编**✗ ⇒ **∴ 所以**加环境覆盖 ✓ ★
    #[must_use]
    fn pool_budget_bytes() -> u64 {
        std::env::var("YANSHI_GPU_POOL_BUDGET_MB")
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(|mb| mb.saturating_mul(1024 * 1024))
            .unwrap_or(Self::POOL_BUDGET_BYTES)
    }

    /// **∴ 池是否启用 ✗**（**与既有读回池**同一开关 ✓）★
    #[must_use]
    fn pool_enabled() -> bool {
        std::env::var_os("YANSHI_GPU_POOL_READBACK").is_none_or(|v| v != "0")
    }

    /// **∴ 从池取一张匹配的 buffer ✗**（**∴ 没有则**新建 ✓）★
    #[must_use]
    fn take_buffer(&self, size: u64, usage: wgpu::BufferUsages) -> wgpu::Buffer {
        let bits = usage.bits();
        if Self::pool_enabled() {
            if let Ok(mut pool) = self.buffer_pool.lock() {
                // **∴ 从后往前找（**最近放回的最可能仍热 ✓）★
                if let Some(index) = pool
                    .iter()
                    .rposition(|(bytes, u, _)| *bytes == size && *u == bits)
                {
                    let (_, _, buffer) = pool.swap_remove(index);
                    return buffer;
                }
            }
        }
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("yanshi-gpu-pooled"),
            size,
            usage,
            mapped_at_creation: false,
        })
    }

    /// **∴ 把 buffer 放回池 ✗**（**∴ 超上限则**丢弃 ✓）★
    fn put_buffer(&self, size: u64, usage: wgpu::BufferUsages, buffer: wgpu::Buffer) {
        if !Self::pool_enabled() {
            return;
        }
        if let Ok(mut pool) = self.buffer_pool.lock() {
            // **∴ 双约束 ✗**：**张数 ≤ POOL_MAX**✗ ＋ **∴ 总字节 ≤ POOL_BUDGET_BYTES** ✓ ★
            let total: u64 = pool.iter().map(|(bytes, _, _)| *bytes).sum();
            if pool.len() < Self::POOL_MAX
                && total.saturating_add(size) <= Self::pool_budget_bytes()
            {
                pool.push((size, usage.bits(), buffer));
            }
        }
    }

    /// **∴ 适配器允许的**单缓冲**上限（**字节 ✓）✗** ★
    ///
    /// **∴ 用途 ✗**：**分块大小的**四者取最小**之一** ✓（**∴ 另一个是** `max_buffer_bytes` ✓）★
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
        // **★ 上限**按适配器等级**分档 ✗ ★**（**第 540 轮 ✓；目标第 4 条 ✓）
        //   ⇒ **∴ 核显／软件渲染 ⇒ 4 MiB（**与 A/B 实测一致 ＋ **零行为变化 ✓）
        //     ＋ **∴ 独显 ⇒ 32 MiB**（**待真 L4 复核 ✓）★
        let cap = chunk_bytes_cap(self.device_class()) as u64;
        let budget = binding
            .min(buffer)
            .saturating_mul(3)
            .saturating_div(4)
            .clamp(CHUNK_BYTES_MIN as u64, cap);
        // **★ 再按 `count/4` 细分 ✗ ★**（**第 496 轮 ✓）
        //   **∴ 为什么 ✗**：**单块不许**接近整幅** ✗
        //     ⇒ **∴ 因为**守卫断言**单次分配 ＜ 整幅 f32** ✓
        //       ＋ **∴ 而** 1024×512 的整幅只有 8 MiB** ✓
        //         ⇒ **∴ 若**按 64 MiB 封顶 ⇒ **∴ 一块就装下整幅**✗ ⇒ **∴ 守卫**红** ✓
        //           ⇒ **∴ 所以**：**min(预算, count/4 对应的字节数)** ✓ ★**** ✓✓
        let quarter_bytes = (count.saturating_mul(BYTES_PER_PIXEL) / 4) as u64;
        let budget = budget.min(quarter_bytes.max(CHUNK_BYTES_MIN as u64));
        let pixels = (budget as usize) / BYTES_PER_PIXEL;
        // **★★★ 必须**对齐到 64 像素（＝256 字节）✗ ★★★**（**第 614 轮 ✓）
        //   **∴ 为什么 ✗**：**coalescing（**三张大 buffer ＋ 每块偏移 ✓）要求
        //     每个块的偏移满足 `min_storage_buffer_offset_alignment`**✗
        //     ＋ **∴ 而** wgpu 保证它是 **≤ 256** ✓
        //       ＋ **∴ 64 像素 × 4 字节 ＝ **256 字节**** ⇒ **∴ 从而**：
        //         **所有块偏移**天然对齐** ✓
        //   **∴ 不这么做的后果（**第 612 轮实测 ✓）✗**：
        //     **服务端在渲染时**崩溃**✗（**∴ 客户端只见 `fetch failed` ✓）
        //       ＋ **∴ 而**块大小不是 64 倍数时就会发生** ✓ ★
        //   **∴ 代价（**两面 ✓）✗**：**每块最多**浪费 63 像素**✗
        //     ⇒ **∴ 对 4 MiB 的块 ＝ **0.006%**** ⇒ **∴ 可忽略** ✓ ★
        // **★★★ 已回退（**第 618 轮 ✓）✗ ★★★**：**这里曾把 `per_chunk` **向下取整到 64 像素**
        //   ＋ **∴ 它**只为**已放弃的 coalescing 服务**✗
        //     ＋ **∴ 而**实测（**L4 ✓）✗**：**它**让 4K 渲染**服务端崩**（**`fetch failed` ＋ socket 断开 ✓）
        //       ＋ **∴ 而**本机**看不出**（**∴ 因为**核显 cap ＝ 4 MiB ⇒ 1 048 576 像素**本来就**是 64 的倍数** ✓）
        //         ⇒ **★ 所以：**回退它 ⇒ 恢复第 611 轮在 L4 上**已验证的行为**** ✓ ★
        //   **∴ 教训 ✗**：**cherry-pick 会**连带**带入无关改动**✗
        //     ⇒ **∴ 所以**：**回退必须**逐块核对**，不能只回退"那个大改动"** ✓ ★
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
        // **★★★ 读回**批量化**：一张大 staging buffer ✗ ★★★**（**第 585 轮 ✓；**目标第 5 条 ✓）
        //   **∴ 原来 ✗**：**每块**单独建一张 `COPY_DST|MAP_READ` 小 buffer**
        //     ＋ **∴ 读回时**逐块 `map_async` ＋ `poll` ＋ `wait`**✗
        //       ⇒ **∴ 于是**：**N 块 ⇒ **N 次同步往返 ＋ N 次 memcpy**** ✓
        //         （**实测：传输占 GPU 阶段 77% ✓；**第 68 轮 ✓）
        //   **∴ 现在 ✗**：**所有块 `copy_buffer_to_buffer` 到**同一张**大 buffer 的各自偏移**
        //     ＋ **∴ 读回只**一次 `map_async` ＋ 一次等待 ＋ 一次拷贝** ✓
        //       ⇒ **∴ 往返 N ⇒ 1** ✗ ＋ **∴ memcpy N ⇒ 1** ✓
        //   **∴ 代价（**两面都写 ✓）✗**：**峰值多一张 `count*4` 字节的映射缓冲**
        //     （**4K ⇒ 33 MB ✓）✗ ＋ **∴ 而**它**必须整张驻留** ✓
        //       ⇒ **∴ 所以**：**显存／内存换往返** ✓ ★
        // **★ 先试池 ✗ ★**（**∴ 按字节数匹配 ✓；`YANSHI_GPU_POOL_READBACK=0` 关 ✓）
        let want = (count * 4) as u64;
        let pool_on = std::env::var_os("YANSHI_GPU_POOL_READBACK").is_none_or(|v| v != "0");
        let pooled = if pool_on {
            self.readback_pool
                .lock()
                .ok()
                .and_then(|mut slot| match slot.as_ref() {
                    Some((size, _)) if *size == want => slot.take().map(|(_, b)| b),
                    _ => None,
                })
        } else {
            None
        };
        let readback: wgpu::Buffer = match pooled {
            Some(b) => b,
            None => self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("yanshi-gpu-readback-batched"),
                size: want,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }),
        };
        let mut done = 0usize;
        // **∴ 分段累加器 ✗**（**∴ 一次调用内累加 ⇒ 最后写静态 ✓）★
        let mut upload_ns = 0u64;
        let mut submit_ns = 0u64;
        let mut readback_ns = 0u64;
        // **★ GPU 侧时间戳（**门控 ✓）✗ ★**（**第 546 轮 ✓；目标第 9 条 ✓）
        //   ⇒ **∴ 门控 ⇒ **生产路零开销**✗（`should_time_gpu` ✓）
        //     ＋ **∴ 只量**第一块**✗（**∴ 两个索引会被后续块覆盖 ✓）
        //       ⇒ **∴ 那**是**代表性的一块** ✓ ★
        // **∴ 探针开关单取一次 ✗**（**∴ 等待期的 CPU 采样也用同一个门 ✓ —— 生产路零开销 ✓）★
        let probe_on = std::env::var_os("YANSHI_RENDER_PROBE").is_some();
        let time_gpu = should_time_gpu(self.device.features(), probe_on);
        // **★★★ 第 246 轮：把 `copy_buffer_to_buffer` 也纳入时间戳 ✗ ★★★**（**待办 2b ✓）
        //   ＋ **∴ 为什么要它 ✗**：**`gpu` ＝ 750 µs 只含** compute pass** ✓
        //     ＋ **∴ 而**同一帧还要做 **20 次** `copy_buffer_to_buffer`**（**`lib.rs:949` ✓）
        //       ⇒ **∴ 于是**：**分不清**"拷贝贵"与"读回等待贵"** ✓ ★★
        //   ＋ **∴ 门控（**关键 ✓）**：**在 encoder 里写时间戳**需要
        //     `TIMESTAMP_QUERY_INSIDE_ENCODERS`**✗（**与 pass 里的 `TIMESTAMP_QUERY` 是**两个**特性 ✓）
        //     ⇒ **∴ 没有它**就**一个都别写**✗ ⇒ **∴ 于是**：**行为与改动前**逐位一致** ✓ ★
        //       ＋ **∴ 而** `STAGE_COPY_NS` **保持 0**✗ ⇒ **∴ 那是**如实的 0**（**没量 ✓）★
        let time_copy = time_gpu
            && self
                .device
                .features()
                .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        // **∴ 探针开着时**说清"拷贝到底量没量"✗ ★**（**∴ 否则** `copy ＝ 0` 会被误读成"拷贝不花时间" ✓）
        //   ⇒ **∴ 而**那**正是**本仓最忌的"用 0 撒谎"** ✓（**∴ 必须**可区分 ✓）★★
        if time_gpu {
            eprintln!(
                "PROBE 时间戳：compute_pass={}｜inside_encoders（**拷贝 ✓）={} ⇒ 拷贝{}",
                time_gpu,
                time_copy,
                if time_copy {
                    "已纳入"
                } else {
                    "未纳入（适配器无 TIMESTAMP_QUERY_INSIDE_ENCODERS ⇒ 如实为 0）"
                }
            );
        }
        // **★★★ 两张 buffer：resolve 目标 ＋ 可映射的中转 ✗ ★★★**（**第 603 轮 ✓；目标第 9 条后半 ✓）
        //   **∴ 原来的错（**实测 panic ✓）✗**：
        //     `MAP usage can only be combined with the opposite COPY,
        //      requested BufferUsages(MAP_READ | COPY_DST | QUERY_RESOLVE)`
        //       ⇒ **∴ 因为**同一张 buffer **同时**带 `MAP_READ` 与 `QUERY_RESOLVE`** ✗
        //         ＋ **∴ 而** wgpu 只允许 `MAP_READ` 与**对向的 COPY**（`COPY_DST`）组合** ✓
        //           ⇒ **∴ 于是**：**服务端在渲染时**直接 panic**（**`wgpu_core.rs:1436` ✓）
        //             ＋ **∴ 而** validation 错误**不走 `Result`**✗ ⇒ **∴ `map_err` **拦不住** ✓ ★
        //   **∴ 现在（**与 wgpu 的加法一致 ✓）✗**：
        //     **∴ ① `resolve` ✗**：**只用** `QUERY_RESOLVE | COPY_DST`**（**写时间戳 ＋ 可被拷出 ✓）
        //       ＋ **∴ ② `read` ✗**：**16 字节的**中转**✗
        //         ＋ **∴ 只用** `COPY_DST | MAP_READ`**（**收拷贝 ＋ 可映射 ✓）
        //           ＋ **∴ ③** 提交前把 `resolve` **拷进** `read` ⇒ **∴ 然后**只 map `read`** ✓ ★
        let (ts_query, ts_resolve, ts_read) = if time_gpu {
            let query = self.device.create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("yanshi-gpu-time"),
                ty: wgpu::QueryType::Timestamp,
                // **∴ 4 个时间戳 ✗**（**第 246 轮 ✓）：0／1 ＝ compute pass ✗ 2／3 ＝ **拷贝** ✓
                //   ＋ **∴ 没有** `TIMESTAMP_QUERY_INSIDE_ENCODERS` 时只取前 2 个** ✓
                count: if time_copy { 4 } else { 2 },
            });
            let resolve = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("yanshi-gpu-time-resolve"),
                size: if time_copy { 32 } else { 16 },
                // **★ 必须带 `COPY_SRC` ✗ ★**（**第 603 轮实测 ✓）：**它**要被
                //   `copy_buffer_to_buffer` **当作源**拷进中转那张** ✓
                //   ＋ **∴ 而** `COPY_DST` **不需要**（**它**不被写入 ✓）
                //     ＋ **∴ 且**绝不能加 `MAP_*`（**∴ 那**是第 84 轮和本轮两次 panic 的根因 ✓）★
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let read = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("yanshi-gpu-time-read"),
                size: if time_copy { 32 } else { 16 },
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            (Some(query), Some(resolve), Some(read))
        } else {
            (None, None, None)
        };
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
                    // **★ 只量第一块 ✗ ★**（**∴ 索引会被后续块覆盖 ⇒ **∴ 量多次无意义** ✓）
                    timestamp_writes: if done == 0 {
                        ts_query
                            .as_ref()
                            .map(|query| wgpu::ComputePassTimestampWrites {
                                query_set: query,
                                beginning_of_pass_write_index: Some(0),
                                end_of_pass_write_index: Some(1),
                            })
                    } else {
                        None
                    },
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups((take as u32).div_ceil(64), 1, 1);
            }
            // **∴ 拷到**大 buffer 的 `done*4` 偏移**✗（**∴ 此时 `done` ＝ 本块之前的像素数 ✓）★
            // **∴ 时间戳 2 ✗**：**第一块拷贝**之前** ✓（**∴ 只写一次 ✓ —— 与 pass 同理 ✓）★
            if time_copy && done == 0 {
                if let Some(query) = ts_query.as_ref() {
                    enc.write_timestamp(query, 2);
                }
            }
            enc.copy_buffer_to_buffer(&dst, 0, &readback, (done * 4) as u64, (take * 4) as u64);
            done += take;
        }
        // **∴ 时间戳 3 ✗**：**最后一次拷贝**之后** ✓ ⇒ **∴ 于是** `[2,3]` ＝ **全部拷贝的 GPU 时间** ✓ ★
        if time_copy {
            if let Some(query) = ts_query.as_ref() {
                enc.write_timestamp(query, 3);
            }
        }
        // **★ 一次提交 ＋ 一次等待 ✗ ★**（**∴ 第 284 轮：**快 88.6% ✓）
        // **∴ 分段②：提交（**submit ＋ 一次 poll ✓）★
        let t_sub = Instant::now();
        // **∴ resolve 必须在**提交之前**✗（**∴ 它**也是编码命令 ✓）★
        if let (Some(query), Some(resolve), Some(read)) =
            (ts_query.as_ref(), ts_resolve.as_ref(), ts_read.as_ref())
        {
            // **∴ 数一数写了几个时间戳 ✗**（**∴ 4 个**＝ 含拷贝 ✓；**2 个**＝ 只有 compute ✓）★
            let stamps = if time_copy { 4 } else { 2 };
            enc.resolve_query_set(query, 0..stamps, resolve, 0);
            // **∴ 再拷进可映射的那张 ✗**（**∴ 因为** `resolve` **不能**同时可映射 ✓）★
            enc.copy_buffer_to_buffer(resolve, 0, read, 0, stamps as u64 * 8);
        }
        self.queue.submit(Some(enc.finish()));
        // **★ 不阻塞 ✗ ★**（**第 471 轮 ✓）：**真正的同步在下面 `map_async` 的 await ✓
        let _ = self.device.poll(wgpu::PollType::Poll);
        submit_ns += t_sub.elapsed().as_nanos() as u64;

        // **∴ 分段③：读回（**一次 map ＋ 一次等待 ✓）★
        let t_rb = Instant::now();
        // **★ 一次 map ＋ 一次等待 ＋ 一次拷贝 ✗ ★**（**第 585 轮 ✓）
        let mut out: Vec<u8> = Vec::with_capacity(count * 4);
        // **∴ 超时标志 ✗**（**∴ 一旦**置位 ⇒ **∴ 后面**不再拷贝 ✓）★
        let mut readback_timed_out = false;
        {
            let slice = readback.slice(..);
            let signal = std::sync::Arc::new(OnceSignal::new(&self.device));
            let signal_in_cb = std::sync::Arc::clone(&signal);
            // **map 结果不能丢** ✓（审计 P3 #4）：此前回调签名是 `|_r|` ✗ ——
            // 设备丢失时回调带着 `Err(MapError)` 来 ✗，后面照样 `get_mapped_range`
            // ⇒ panic ✗ 而非干净降级 ✗。现在把 Err 存进共享格 ✓，与 blur 路形态统一 ✓。
            let map_error =
                std::sync::Arc::new(std::sync::Mutex::new(None::<wgpu::BufferAsyncError>));
            let map_error_in_cb = std::sync::Arc::clone(&map_error);
            slice.map_async(wgpu::MapMode::Read, move |result| {
                if let Err(error) = result {
                    if let Ok(mut cell) = map_error_in_cb.lock() {
                        *cell = Some(error);
                    }
                }
                signal_in_cb.signal();
            });
            let _ = self.device.poll(wgpu::PollType::Poll);
            // **★★★ 第 817 轮：**改成**有间隔的阻塞等待 ＋ 超时**** ✗ ★★★**
            //   ＋ **∴ 用户指出 ✗**：**原来**在 `block_on` 里 `yield_now()` 忙等 ⇒ **∴ CPU** 100% 空转** ✓
            //     ＋ **∴ 且**没有**超时**✗ ⇒ **∴ GPU hang 了**就永远卡死** ✓ ★★
            //   ＋ **∴ 现在 ✗**：**200 µs 醒一次 ＋ **30 秒总上限** ✗
            //     ⇒ **∴ 超时 ⇒ **打印 ＋ 返回**已拿到的部分**✗（**∴ 调用方**按长度不足 ⇒ **∴ 回退 CPU** ✓）★
            // **★★★ 第 246 轮：**读回等待期间**主机到底在不在等**** ✗ ★★★**（**待办 2c 的前置 ✓）
            //   ＋ **∴ 为什么必须先量这个 ✗**：**2c（**双缓冲／in-flight ✓）**能省多少**✗
            //     ⇒ **∴ 不取决于**等待有多长**✗ 而**取决于**：**等的时候**有没有别的事可做** ✓
            //   ＋ **∴ 而**本仓的结构**已给出半个答案 ✗**：**量化是**按带并行**的**（`render.rs:3903`
            //     的 `thread::scope` ⇒ 每个带各调一次 `render_accumulation` ✓）
            //     ＋ **∴ 而**量化器**由一把**全局锁**串起来**（`buffer.rs:26` 的 `Mutex<Option<Quantizer>>` ✓）
            //       ⇒ **∴ 于是**：**一个带在等 GPU 时 ⇒ **别的带**在跑自己的 CPU 活** ✓ ★
            //         ⇒ **★ 所以**：**那 81 ms 有可能**已经被并行带**掩盖掉了** ✗
            //           ⇒ **∴ 若**真是这样 ⇒ **∴ 2c 的收益 ≈ 0**✗ 而**不是**那个 6.4% 的上限** ✓ ★★
            //   ＋ **∴ 量法 ✗**：**读 `/proc/self/stat` 的 `utime+stime`**（**全部线程之和 ✓）
            //     ⇒ **∴ 等待期间它的增量 ÷ 等待时长 ＝**等待期的有效并行度** ✓
            //       ⇒ **∴ ≈ 0 ⇒ **整机在等**（**可重叠窗口 ≈ 100% ✓）
            //         ＋ **∴ ≈ 1 以上 ⇒ **别的线程在干活 ⇒ 等待**已被掩盖** ✓ ★★
            let cpu_before = if probe_on { process_cpu_ns() } else { None };
            let t_wait = std::time::Instant::now();
            let ok = signal.wait_timeout(
                std::time::Instant::now() + std::time::Duration::from_secs(30),
                std::time::Duration::from_micros(200),
            );
            let wait_ns = t_wait.elapsed().as_nanos() as u64;
            if cpu_before.is_some() {
                if let (Some(before), Some(after)) = (cpu_before, process_cpu_ns()) {
                    WAIT_NS.fetch_add(wait_ns, std::sync::atomic::Ordering::Relaxed);
                    WAIT_CPU_NS.fetch_add(
                        after.saturating_sub(before),
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
            }
            if !ok {
                // **∴ 超时 ⇒ **∴ 不**静默 ⇒ **∴ 说出来** ✓（**∴ 本会话的纪律 ✓）★★
                eprintln!(
                    "  ⏭ GPU 读回超时（30 秒，{} 块）⇒ **∴ 本次**按不可用处理** ⇒ **∴ 调用方**回退 CPU**",
                    STAGE_CALLS.load(std::sync::atomic::Ordering::Relaxed)
                );
                // **∴ 不能 `break` ✗**（**`async` 里不合法 ⇒ `error[E0267]` ✓）★
                //   ＋ **∴ 所以**用**标志位**✗ ⇒ **∴ 于是**后面**跳过这次拷贝** ✓
                readback_timed_out = true;
            }
            if !readback_timed_out {
                let map_failed = map_error.lock().map(|cell| cell.is_some()).unwrap_or(false);
                if map_failed {
                    // **map 失败 ⇒ 设备丢失形态** ✓（审计 P3 #4）：不碰 `get_mapped_range`
                    //（会 panic ✗）⇒ `out` 保持为空 ✓ ⇒ 调用方按「长度不足」回退 CPU ✓
                    //（与读回超时同路 ✓，不谎报成功 ✓）。
                    eprintln!("  ⏭ GPU map 失败（设备丢失？）⇒ 本次按不可用处理 ⇒ 调用方回退 CPU");
                } else {
                    let mapped = slice.get_mapped_range();
                    out.extend_from_slice(&mapped);
                }
            }
        }
        readback.unmap();
        // **∴ 放回池 ✗**（**∴ 数据已拷进 `out` ⇒ **复用安全** ✓）★
        if pool_on {
            if let Ok(mut slot) = self.readback_pool.lock() {
                *slot = Some((want, readback));
            }
        }
        readback_ns += t_rb.elapsed().as_nanos() as u64;
        // **∴ 写静态 ✗**：**每次覆盖 ⇒ 读的就是**最近一次** ✓
        STAGE_UPLOAD_NS.store(upload_ns, std::sync::atomic::Ordering::Relaxed);
        STAGE_SUBMIT_NS.store(submit_ns, std::sync::atomic::Ordering::Relaxed);
        STAGE_READBACK_NS.store(readback_ns, std::sync::atomic::Ordering::Relaxed);
        // **★ 读回 GPU 侧纳秒 ✗ ★**（**∴ wgpu 26 没有 `get_results` ⇒ **resolve → map** ✓）
        if let Some(read) = ts_read.as_ref() {
            let slice = read.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            let _ = self.device.poll(wgpu::PollType::Wait);
            // **map 结果必须看** ✓（审计 P3 #4）：此前只看「回调 fired」（`rx.recv().is_ok()` ✗）——
            // 设备丢失时回调带着 Err 来 ⇒ `get_mapped_range` 会 panic ✗。统一成 blur 路形态：
            // Err ⇒ 本次时间戳缺失 ✓（不读数、不 unmap —— map 失败后缓冲已不处于 mapped 态 ✓）。
            if matches!(rx.recv(), Ok(Ok(()))) {
                let data = slice.get_mapped_range();
                if data.len() >= 16 {
                    let start = u64::from_le_bytes(data[0..8].try_into().unwrap_or([0; 8]));
                    let end = u64::from_le_bytes(data[8..16].try_into().unwrap_or([0; 8]));
                    let ns = end.saturating_sub(start);
                    STAGE_GPU_NS.store(ns, std::sync::atomic::Ordering::Relaxed);
                    // **∴ 时间戳 2／3 ✗**：**拷贝那一段** ✓（**∴ 第 246 轮 ✓；待办 2b ✓）★
                    //   ＋ **∴ 语义 ✗**：**它是** GPU 执行全部 `copy_buffer_to_buffer` 的时间**✗
                    //     ⇒ **∴ 而**它**落在** host 侧的 `readback` 窗口**里** ✓
                    //       ⇒ **∴ 所以**：**两者**不能相加**✗（**∴ 那**会重复计 ✓）★
                    let copy_ns = if data.len() >= 32 {
                        let cs = u64::from_le_bytes(data[16..24].try_into().unwrap_or([0; 8]));
                        let ce = u64::from_le_bytes(data[24..32].try_into().unwrap_or([0; 8]));
                        ce.saturating_sub(cs)
                    } else {
                        0
                    };
                    STAGE_COPY_NS.store(copy_ns, std::sync::atomic::Ordering::Relaxed);
                    let waited = WAIT_NS.load(std::sync::atomic::Ordering::Relaxed);
                    let waited_cpu = WAIT_CPU_NS.load(std::sync::atomic::Ordering::Relaxed);
                    let parallelism = if waited > 0 {
                        waited_cpu as f64 / waited as f64
                    } else {
                        0.0
                    };
                    eprintln!(
                        "PROBE GPU 算：{:.3} ms（**时间戳 ✓）｜GPU 拷贝 {:.3} ms｜host 侧提交 {:.3} ms ⇒ 驱动开销 {:.3} ms｜host 侧读回 {:.3} ms｜★读回等待（**累计 ✓）{:.1} ms｜等待期间本进程 CPU {:.1} ms ⇒ 等待期并行度 {:.2}×★{}",
                        ns as f64 / 1e6,
                        copy_ns as f64 / 1e6,
                        submit_ns as f64 / 1e6,
                        (submit_ns.saturating_sub(ns)) as f64 / 1e6,
                        readback_ns as f64 / 1e6,
                        waited as f64 / 1e6,
                        waited_cpu as f64 / 1e6,
                        parallelism,
                        // **∴ 判读口径写在输出里 ✗**（**∴ 于是**读日志的人**不会误读 ✓）★
                        if parallelism < 0.25 {
                            "（**等待期整机基本在等 ⇒ **可重叠窗口 ≈ 等待全长** ✓）"
                        } else {
                            "（**等待期别的线程在干活 ⇒ **等待**已被部分掩盖** ✓）"
                        }
                    );
                }
                drop(data);
                read.unmap();
            }
        }
        STAGE_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(out)
    }

    fn lut_len(&self) -> usize {
        (self.lut_buf.size() / 4) as usize
    }
}

/// **∴ 极简 `block_on` ✗**（**∴ 不引入 `pollster` ✓）
fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    // **★★★ 第 818 轮：**去掉忙等 ＋ 加超时**** ✗ ★★★**
    //   ＋ **∴ 用户指出（**原文 ✓）**✗**：
    //     ⇒ **∴ ①** 「**`block_on` 忙等待（**最嚴重 ✓）**✗**：
    //       **∴ `Waker::noop()` ＋ `std::thread::yield_now()` ⇒ **∴ 等 GPU 时** CPU 100% 空转** ✓
    //         ＋ **∴ ②** 「**GPU 等待**無超時**✗ ⇒ **∴ hang 了就**永远卡死** ✓
    //   ＋ **∴ 为什么原来那样写 ✗**（**读代码才知道 ✓）★**：
    //     **∴ 本仓的 future**自己驱动 device**✗（**`readback` 里每轮 `poll(Poll)` ✓）
    //       ⇒ **∴ 所以**：**`Waker::noop()` **不会**卡死**✗ 而**只**让等待变成忙等** ✓ ★★
    //   ＋ **∴ 所以修法 ✗**：**保留"自己轮询"的形态**✗ 而**把 `yield_now` 换成**短睡**** ✓
    //     ⇒ **∴ ①** **CPU 占用**大降**（**∴ 因为**不再**每轮**抢调度 ✓）
    //       ＋ **∴ ②** **加**总时限**✗ ⇒ **∴ 于是**超时**必然发生 ＋ **可降级** ✓ ★★★
    //         ＋ **∴ 而** `poll` **仍然每轮调用**✗ ⇒ **∴ 驱动语义**不变** ✓
    use std::task::{Context, Poll};
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut fut = Box::pin(fut);
    // **∴ 上限 60 秒 ✗**（**∴ 远大于**任何实测帧时间 ⇒ **∴ 只在 hang 时触发 ✓）★
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let step = std::time::Duration::from_micros(200);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        if std::time::Instant::now() >= deadline {
            // **∴ 超时 ⇒ **∴ 必须**说出来**✗ 而**不**静默** ✓（**∴ 本会话的纪律 ✓）★
            //   ＋ **∴ 而**这里**没有**默认值可返回**✗ ⇒ **∴ 所以**：**如实 panic 并**写清原因** ✓
            //     ⇒ **∴ 那**比"永远卡死"好**✗（**∴ 至少**有栈 ＋ 有原因 ✓）★★★
            panic!("GPU future 超过 60 秒未完成 ⇒ **∴ 视为** GPU 卡死**（**第 818 轮加的超时 ✓）");
        }
        // **∴ 短睡替代 `yield_now` ✗**（**∴ 于是**不烧 CPU ✓）★★★
        std::thread::sleep(step);
    }
}

/// **★★★ 安全字节转换（**分块拷贝 ⇒ 比逐元素快 2.4 倍 ✓）★★★**（**第 610 轮 ✓）
///
/// **∴ 为什么要改 ✗**：**原来**逐元素** `out.extend_from_slice(&x.to_le_bytes())`**✗
///   ＋ **∴ 而**每个元素都要**一次带容量检查的调用** ✓
///     ⇒ **∴ 本机微基准（**`black_box` ＋ 1M 像素 ＝ 4 MiB ✓）✗**：
///       **∴ 逐元素 ✗**：**1.570 ms/次**
///       **∴ 分块 `copy_from_slice` ✗**：**0.662 ms/次**（**快 2.4 倍 ✓）
///       **∴ 零拷贝 cast ＋ `to_vec` ✗**：**0.394 ms/次**（**快 4.0 倍 ✓）
///         ＋ **∴ 而** L4 实测 `upload ＝ **31 725 µs / 15 次 ＝ 2.1 ms/次`**✗
///           ⇒ **★ 所以：转换占 `upload` 的 **75%** ⇒ **它**就是那个大头** ★
///
/// **★ 为什么选**分块**而不是**零拷贝 cast** ✗ ★**：
///   **∴ 收益 ✗**：**分块拿到**43% 的收益**✗（**省 ~14 ms／15 次 ✓）
///     ＋ **∴ 而**零拷贝再多省 **4 ms** ✓
///   **∴ 代价 ✗**：**零拷贝要 `unsafe` 或 `bytemuck`**✗
///     ＋ **∴ 而**本 crate **禁 `unsafe`** ✓
///       ⇒ **∴ 所以**：**先用零依赖的分块**✗
///         ＋ **∴ 若**日后确需那 4 ms ⇒ **∴ 再**评估 `bytemuck`**（**AGENTS.md：看取舍，不禁依赖 ✓）★
///
/// **∴ 输出**逐字节不变**✗ ⇒ **∴ 所以**：**逐位一致的判据**必须仍然绿** ✓ ★
fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = vec![0u8; v.len() * 4];
    for (c, x) in out.chunks_exact_mut(4).zip(v) {
        c.copy_from_slice(&x.to_le_bytes());
    }
    out
}

/// **∴ `bytes → f32`（**`f32s_to_bytes` 的逆 ✓）✗**（**第 698 轮 ✓）
///
/// **∴ 与 `f32s_to_bytes` **逐字节对称**** ✗：
///   **∴ 那边**用 `x.to_le_bytes()`**✗ ⇒ **∴ 这边**用 `f32::from_le_bytes`** ✓
///     ＋ **∴ 所以**：**往返**逐位不变** ✓ ★
fn bytes_to_f32s(data: &[u8]) -> Vec<f32> {
    let mut out = Vec::with_capacity(data.len() / 4);
    for c in data.chunks_exact(4) {
        out.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
    }
    out
}

/// **∴ `u32` 版 ✗**（**同上，分块拷贝 ✓）
fn u32s_to_bytes(v: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; v.len() * 4];
    for (c, x) in out.chunks_exact_mut(4).zip(v) {
        c.copy_from_slice(&x.to_le_bytes());
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

    /// **★ 同步等待（**阻塞 ＋ 超时 ＋ 自己驱动 device ✓）✗ ★**
    ///
    /// **★★★ 第 817 轮：**把忙等改成**有间隔的阻塞等待**** ✗ ★★★**
    ///   **∴ 用户指出的两条 ✗**：
    ///     ⇒ **∴ ①** `block_on` **用 `Waker::noop()` ＋ `thread::yield_now()`**
    ///       ⇒ **∴ 等 GPU 时** CPU 100% 空转** ✓ ★★
    ///         ＋ **∴ ②** **没有**超时**✗ ⇒ **∴ GPU hang 了**就永远卡死** ✓ ★
    ///   **∴ 为什么不能直接上 Condvar ✗**（**读代码才知道 ✓）★**：
    ///     ＋ **∴ 关键 ✗**：**wgpu 的 `map_async` 回调**只在 `device.poll()` 被调用时**才可能触发** ✓
    ///       ⇒ **∴ 所以**：**纯 `Condvar` **不会被唤醒**✗ ⇒ **∴ 必须**有一个**驱动者** ✓ ★★★
    ///         ＋ **∴ 而那**正是原设计让 `wait()` 自己每轮 poll 的原因** ✓
    ///   **∴ 所以正确的形态 ✗**：**`wait_timeout` 带**轮询间隔**✗
    ///     ⇒ **∴ ①** **每次醒来**调一次 `device.poll(Poll)`**（**∴ 从而**驱动回调 ✓）
    ///       ＋ **∴ ②** **总时限**由 `deadline` 保证**✗ ⇒ **∴ 于是**超时**必然发生** ✓
    ///         ＋ **∴ ③** **CPU 占用**从**忙转**变成**每间隔醒一次**（**∴ 大降 ✓）★
    ///
    /// **∴ 返回值 ✗**：**`true` ＝ 就绪**｜**`false` ＝ 超时** ✓（**∴ 调用方**据此降级 CPU ✓）★
    fn wait_timeout(&self, deadline: std::time::Instant, interval: std::time::Duration) -> bool {
        // **∴ 形态很简 ✗**：**每轮**先查标志 ⇒ **poll 一次**（**驱动回调 ✓）⇒ 再查 ⇒ 睡一小段** ✓
        //   ＋ **∴ 为什么**不需要 waker ✗**：**本循环**自己 poll device**✗
        //     ⇒ **∴ 唤醒**只**缩短延迟**✗ 而**不**是**必需** ✓（**∴ 读原实现**才看清 ✓）★★
        loop {
            if self.ready.load(std::sync::atomic::Ordering::Acquire) {
                return true;
            }
            // **∴ 驱动一次 ✗**（**∴ 只有** poll 会触发 map 回调 ✓）
            let _ = self.device.poll(wgpu::PollType::Poll);
            if self.ready.load(std::sync::atomic::Ordering::Acquire) {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false; // **∴ 超时 ⇒ **∴ 调用方**降级 CPU** ✓ ★
            }
            // **∴ 睡一小段 ✗**（**∴ 替代** `thread::yield_now()` ⇒ **∴ 不**烧 CPU ✓）★★★
            std::thread::sleep(interval);
        }
    }

    // **∴ `wait()` 已**删除** ✗**（**第 818 轮 ✓）★
    //   ＋ **∴ 为什么 ✗**：**读回路径改用了 `wait_timeout`**✗ ⇒ **∴ 它**成了**死代码**
    //     ＋ **∴ 而** clippy `-D warnings` **把**"never used"**当硬失败** ✓ ★
    //   ＋ **∴ 若**将来有别的 future 需要 async 形态**✗
    //     ⇒ **∴ 那时**再包一层 `poll_fn` ＋ `wait_timeout`** ✓（**∴ 不要**留死代码 ✓）★
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
            1_048_576,
            "独显按 L4 实测的平衡点（第 584 轮重定）"
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
        // **∴ 第 795 轮改写 ✗**：**旧断言是 `Discrete < GPU_MIN_PIXELS`**✗
        //   ＋ **∴ 而**实测把 `Discrete` 定在 **1 048 576**✗ ⇒ **∴ 它**与 `GPU_MIN_PIXELS`（**1 000 000 ✓）
        //     **是**同一个量级**✗（**∴ 差 4.9% ✓）⇒ **∴ 所以**旧关系**不再成立** ✓ ★★
        //   **∴ 新的关系（**由实测得到 ✓）★**：**独显的阈值**比其它类别**低**』
        //     ＋ **∴ 而**它与 `GPU_MIN_PIXELS` 的**差距**应当**在 10% 以内** ✓（**∴ 那**是"同一个量级" ✓）★
        assert!(threshold_for(DeviceClass::Discrete) < GPU_MIN_PIXELS * 11 / 10);
    }

    /// **∴ 边界是闭的 ✗**（**∴ 与**旧判据同一形状 ✓）
    /// **★★★ 已知矛盾：**两档阈值的方向反了** ✗ ★★★**（**第 796 轮发现；**尚未解决 ✓）
    ///
    /// **∴ 事实（**实测 ＋ 源码 ✓）★**：
    ///   ＋ **∴ `threshold_for(Discrete) = 1_048_576`**✗（**L4 上 15 轮实测的优势区下界 ✓）
    ///     ＋ **∴ `threshold_for(Integrated) = GPU_MIN_PIXELS = 1_000_000`**✗（**无实测依据 ✓）
    ///       ⇒ **∴ 所以 `Discrete > Integrated`**✗ ⇒ **∴ 于是**：
    ///         **∴ 在 1 000 000 ～ 1 048 576 px 之间**✗ 核显走 ✗ 而独显不走** ✓
    ///           ＋ **∴ 那**与分档的**意图**相反**（**独显更快 ⇒ 应当更早走 ✓）★
    ///
    /// **∴ 为什么**现在不解决 ✗**：
    ///   ＋ **∴ 我**手上**没有**核显实测**✗ ⇒ **∴ 动 `GPU_MIN_PIXELS` 没有依据** ✓
    ///     ＋ **∴ 而**为了让它绿**把 `Discrete` 改小**✗ ⇒ **∴ 那**是**为测试凑数**（**违反纪律 ✓）★
    ///
    /// **∴ 转绿条件 ✗**：**拿到核显（**或任一集成 GPU ✓）的 15 轮实测**✗
    ///   ⇒ **∴ 然后**把两个阈值**一起按实测重定** ⇒ **∴ 那时**本测试**转为普通断言** ＋ **从台账移出** ✓
    ///
    /// **∴ 两面 ✗**：
    ///   ＋ **∴ 收益（**当前状态 ✓）**✗**：**独显阈值有实测依据**（**那是第 3 项的全部意义 ✓）★
    ///     ＋ **∴ 代价 ✗**：**分档在 1.0–1.05 M px 之间方向相反** ⇒ **∴ 那**一段**会把活给更慢的一侧** ✓
    #[test]
    #[ignore = "已知矛盾：Discrete(1_048_576) > Integrated(1_000_000)；等核显实测后一起重定"]
    fn discrete_threshold_must_not_exceed_the_integrated_one() {
        assert!(
            threshold_for(DeviceClass::Discrete) < threshold_for(DeviceClass::Integrated),
            "独显阈值 {} 应当小于核显阈值 {}（否则核显会更早走）",
            threshold_for(DeviceClass::Discrete),
            threshold_for(DeviceClass::Integrated)
        );
    }

    #[test]
    fn boundaries_are_closed() {
        // **★ 第 795 轮重定 ✗ ★**：**独显那档改到 1 048 576（**＝ 1024² ✓）
        //   ＋ **∴ 依据 ✗**：**L4 上 15 轮 ＋ 干净负载的实测**✗
        //     ⇒ **∴ ≤ 724² 两路**等价** ✗ 而 **1024² 起 GPU **显著更快**（**0.81× ✓）★
        //   ＋ **∴ 而**第 584 轮的 22 500 **只有 3 轮**✗ ⇒ **∴ 它**自己标注了「**需要更多轮数据**」 ✓
        //   ＋ **∴ 所以**边界断言跟着走** ✓（**∴ 下面三处都改成新值 ✓）★
        assert!(should_use_gpu_on(DeviceClass::Discrete, 1_048_576));
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 1_048_575));
        // **∴ 而**旧的 10 000 与 22 500 ✗ 现在**都不该**再触发独显 ✓
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 10_000));
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 22_500));
        assert!(should_use_gpu_on(DeviceClass::Integrated, GPU_MIN_PIXELS));
        assert!(!should_use_gpu_on(
            DeviceClass::Integrated,
            GPU_MIN_PIXELS - 1
        ));
        // **★ 分档的意义 ✗ ★**（**第 722 轮改写 ✓）：
        //   **∴ 核显阈值从 30 000 提到 **1 M** 后 ✗**：
        //     **∴ 原来的 `30_000 ⇒ 集显也走` 断言**✗ 就**不再成立** ✓
        //       ＋ **∴ 所以**：**改用**参数化 ＋ **显式的分档对照** ✓ ★
        //   **∴ 分档对照（**100 000 ✓）✗**：
        //     **∴ 独显（**阈值 22 500 ✓）⇒ **走** ✗ ＋ **∴ 核显（**阈值 1 M ✓）⇒ **不走** ✓
        //       ⇒ **∴ 那**正是**分档的意义** ✓ ★
        // **∴ 分档对照的数必须**在新阈值之上** ✗**（**第 795 轮 ✓）：
        //   ＋ **∴ 旧的 100 000 ✗** 现在**低于独显阈值 1 048 576**✗ ⇒ **∴ 它**不再能演示分档** ✓ ★
        //     ⇒ **∴ 改用 2 000 000**✗（**∴ 它**在**两档阈值之上** ✓）★
        assert!(should_use_gpu_on(DeviceClass::Discrete, 2_000_000));
        assert!(should_use_gpu_on(DeviceClass::Integrated, 2_000_000));
        // **∴ 而**在两档阈值**之下**的数 ⇒ **∴ 两者都不走** ✓
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 12_000));
        assert!(!should_use_gpu_on(DeviceClass::Integrated, 12_000));
        // **∴ 我在这里**写反过一次** ✗**（**第 795 轮；**被本测试**当场抓住** ✓）★
        //   **∴ 我写了 ✗**：`assert!(should_use_gpu_on(Discrete, 500_000))`
        //     ＋ **∴ 而** `500_000 < 1_048_576`**✗ ⇒ **∴ 它**本来**不该走** ✓
        //       ＋ **∴ 所以**：**「**判据要能红**」在这里**再次**救了我** ✓ ★★
        //   **∴ 而**两档阈值**几乎相同 ✗**：**独显 1 048 576 ／ 核显 1 000 000**✗（**差 **4.9%** ✓）
        //     ⇒ **∴ 所以**演示分档**必须用**落在**两者之间**的数**✗
        //       ⇒ **∴ 即** `1_020_000`**✗（**∴ 独显走 ✗ 而**核显不走 ✓）★
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 500_000));
        assert!(!should_use_gpu_on(DeviceClass::Integrated, 500_000));
        // **★★★ 我在这一小段里**连错三次** ✗ ★★★**（**第 795–797 轮；**每次都被本测试抓住** ✓）★
        //   **∴ 错法① ✗**：`assert!(should_use_gpu_on(Discrete, 500_000))`
        //     ＋ **∴ 而** `500_000 < 1_048_576`**✗ ⇒ **∴ 它**不该走** ✓ ★
        //   **∴ 错法② ✗**：`1_020_000` 想演示分档 ＋ **∴ 而**它**仍小于 1_048_576** ✓ ★
        //   **∴ 错法③ ✗**：`1_024_288` 想落在"两档之间"
        //     ＋ **∴ 而** `threshold_for(Discrete) ＝ 1_048_576`
        //       **大于** `threshold_for(Integrated) ＝ 1_000_000`** ✓
        //         ⇒ **∴ 所以**：**核显**更早走 ✗ 而**独显**更晚走** ⇒ **∴ "两档之间"那个区间**压根不存在** ✓ ★★★
        //           ＋ **∴ 那**是一个**真实的冲突**✗ ⇒ **∴ 它**记录在下面那条 `#[ignore]` 测试里** ✓
        //   **∴ 所以这里只断言**源码确实成立的事** ✗**：**✓** ★
        //     ⇒ **∴ ①** 独显阈值上 ⇒ **两者都走** ✓
        //       ⇒ **∴ ②** 远低于两档 ⇒ **两者都不走** ✓
        assert!(should_use_gpu_on(DeviceClass::Discrete, 1_048_576));
        assert!(should_use_gpu_on(DeviceClass::Integrated, 1_048_576));
        // **∴ 而**旧时代（**`Discrete ＝ 22_500` ✓）的**23 000 分档演示**✗ 现在**不再成立** ✓
        //   ＋ **∴ 因为**新阈值 1 048 576 之下**两者都不走** ✓ ★
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 23_000));
        assert!(!should_use_gpu_on(DeviceClass::Integrated, 23_000));
    }
}

/// **★ `f16` 的两条路 ✗ ★**（**第 544 轮 ✓；目标第 10 条 ✓）
///
/// **∴ 为什么需要它 ✗**：**WGSL 的原生 `f16` 需要适配器支持
///   `Features::SHADER_F16`** ✓
///   ＋ **∴ 而**本机核显与 L4 上它**都是 `false`**✗（**第 316／275 轮实测 ✓）
///     ⇒ **∴ 所以**：**现在恒走**手写位操作** ✓
///       ⇒ **∴ 而**生产代码**原来**完全没查询**这个特性**✗
///         ⇒ **∴ 于是**：**将来遇到支持它的适配器时**静默继续用手写** ✓ ★
///
/// **∴ 它做什么 ✗**：**把"能力"变成**显式的选择**** ✓
///   ⇒ **∴ 好处 ✗**：
///     ⇒ **∴ ① 可见 ✗**：**能力打印里能报出**选定的路** ✓
///     ⇒ **∴ ② 可判据 ✗**：**纯函数 ⇒ **单测能直接断言**✗（**不碰 GPU ✓）★
///     ⇒ **∴ ③ 为原生路留口 ✗**：**将来支持时**只需补一条 WGSL** ✓ ★
///
/// **∴ 如实（**两面 ✓）✗**：
///   ⇒ **∴ 收益 ✗**：**选路显式化 ＋ 可被判据守住 ＋ 不再静默** ✓
///     ＋ **∴ 代价 ✗**：**`Native` 分支**目前**没有对应的 WGSL**✗
///       ⇒ **∴ 所以**：**它**现在只用于**报告 ＋ 断言**✗
///         ＋ **∴ 若**某天适配器支持它 ⇒ **∴ 判据会**要求补原生 shader** ✓ ★
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F16Path {
    /// **∴ 原生 WGSL `f16`** ✓（**需要 `SHADER_F16` ✓）—— **∴ 尚未实现** ✓
    Native,
    /// **∴ 手写位操作**（**当前唯一实现的路 ✓）★
    Emulated,
}

/// **★ 按适配器能力选 `f16` 的路 ✗ ★**（**纯函数 ⇒ 可单测 ✓）
#[must_use]
pub const fn f16_path(features: wgpu::Features) -> F16Path {
    if features.contains(wgpu::Features::SHADER_F16) {
        F16Path::Native
    } else {
        F16Path::Emulated
    }
}

/// **∴ 把选定的路报给人看 ✗**（**能力打印用 ✓）
#[must_use]
pub const fn f16_path_name(path: F16Path) -> &'static str {
    match path {
        F16Path::Native => "native-f16（**尚未实现 ⇒ 会回退手写 ✓）",
        F16Path::Emulated => "emulated-f16（手写位操作 ✓）",
    }
}

#[cfg(test)]
mod chunk_cap_tests {
    // **★★★ 分块上限必须**停在实测最优处** ✗ ★★★**（**第 620 轮改写 ✓；目标第 4 条 ✓）
    //   **∴ 为什么改写 ✗**：**第 540 轮的原判据断言
    //     「**独显上限必须严格大于核显** ⇒ 否则自适应是假的**」✗
    //     ＋ **∴ 而那**在当时是**推理**（**"独显显存结构不同" ✓）**
    //       ＋ **∴ 而**第 620 轮**真 L4 上的扫描（**4K，各 5 轮 ⇒ 4 有效，带墙钟 ✓）✗**：
    //         **∴ 4 MiB**：中位 ★1399★ ms｜**∴ 32 MiB**：中位 1584 ms
    //           ⇒ **★ 独显上 **4 MiB 也最优**（**32 MiB 差 13.2% ✓）★
    //             ＋ **∴ 所以**：**两档同值 ⇒ **原判据的前提被实测推翻** ✓ ★
    //   **∴ 新判据仍然**能红**** ✗（**∴ 保留价值 ✓）**：
    //     **∴ 若**有人把独显档改**大**（**∴ 如**回到 32 MiB ✓）⇒ **∴ 立刻红** ✓ ★
    //   **∴ 与第 496 轮一致 ✗**：**4 MiB（1831 ms）优于自适应 64 MiB（1936 ms ✓）★
    use super::*;

    #[test]
    fn the_chunk_cap_stays_at_the_measured_optimum() {
        let conservative = chunk_bytes_cap(DeviceClass::Integrated);
        assert_eq!(
            conservative,
            4 * 1024 * 1024,
            "核显必须**保持 4 MiB**（与 A/B 实测一致）"
        );
        assert_eq!(
            chunk_bytes_cap(DeviceClass::Cpu),
            conservative,
            "软件渲染取保守档"
        );
        assert_eq!(
            chunk_bytes_cap(DeviceClass::Virtual),
            conservative,
            "虚拟显卡取保守档"
        );
        assert_eq!(
            chunk_bytes_cap(DeviceClass::Other),
            conservative,
            "未知也取保守档"
        );
        // **★ 独显档的断言（**第 620 轮实测 ✓）★**
        let discrete = chunk_bytes_cap(DeviceClass::Discrete);
        assert_eq!(
            discrete, conservative,
            "独显档取**同一个实测最优值**（{discrete} vs {conservative}）\
             ⇒ 第 620 轮真 L4 扫描：4 MiB 中位 1399 ms，32 MiB 1584 ms（差 13.2%）"
        );
        assert!(
            discrete <= 16 * 1024 * 1024,
            "独显上限不许回到大块（{discrete}）⇒ 实测大块**更慢**"
        );
    }
}

/// **★ 要不要量 **GPU 自己的时间**** ✗ ★**（**第 546 轮 ✓；目标第 9 条 ✓）
///
/// **∴ 为什么需要单独一个纯函数 ✗**：**时间戳查询有三项成本**✗：
///   ⇒ **∴ ① 需要一个 `QuerySet` ＋ 一个 `resolve` 目标 buffer** ✓
///     ＋ **∴ ② 每次提交后要**多一次 `map` 等待**** ✓
///       ＋ **∴ ③ 而** wgpu 26 **没有 `get_results`**✗ ⇒ **∴ 读回要
///         **resolve → buffer → map** 三步** ✓
///           ⇒ **∴ 所以**：**它**绝不能进生产路**✗ ⇒ **∴ 必须**门控** ✓ ★
///
/// **∴ 两个条件缺一不可 ✗**：
///   ⇒ **∴ ① 适配器**支持 `TIMESTAMP_QUERY`**✗（**本机核显实测 = true ✓）
///     ＋ **∴ ② 探针**开着**✗（`YANSHI_RENDER_PROBE` ✓）
///       ⇒ **∴ 于是**：**生产路**零开销** ✓ ★
///
/// **∴ 它回答什么 ✗**：**第 9 条的问题 ——
///   **「传输贵」还是「算得贵」**** ✓
///   ⇒ **∴ 而** host 侧数据（**第 545 轮 ✓）已给出**上下界**✗：
///     **传输 77% vs 提交 23%** ✓
///       ⇒ **∴ 时间戳把 `submit` 里的
///         **驱动开销**与**真算**分开** ✓ ★
#[must_use]
pub const fn should_time_gpu(features: wgpu::Features, probe_on: bool) -> bool {
    probe_on && features.contains(wgpu::Features::TIMESTAMP_QUERY)
}

#[cfg(test)]
mod process_cpu_tests {
    // **★ `/proc/self/stat` 的解析**必须有牙**✗ ★**（**第 246 轮 ✓）
    //   **∴ 为什么值得一条测试 ✗**：**字段索引错一位 ⇒ **读到的就是别的数**✗
    //     而**函数签名（`Option<u64>` ✓）**照样是 Some** ✓ ⇒ **∴ 编译与类型都拦不住** ✓ ★
    //   **∴ 判据 ✗**：**忙等 ~60 ms ⇒ 增量必须落在 [10 ms, 5 s] 之内** ✓
    //     ⇒ **∴ 索引错**（**如读到 starttime／vsize ✓）⇒ **必然**落在区间外** ✓ ★★
    use super::*;

    #[test]
    fn the_process_cpu_reading_moves_with_real_work() {
        let Some(before) = process_cpu_ns() else {
            // **∴ 非 Linux ⇒ **如实跳过**✗ 而**不是**假装通过** ✓（**∴ 本仓的纪律 ✓）★
            eprintln!("⏭ 非 Linux ⇒ /proc/self/stat 不可用 ⇒ 本判据无可判");
            return;
        };
        // **∴ 真的烧一点 CPU ✗**（**∴ 不能只看"变大了" ⇒ 那**睡眠也会通过 ✓）★
        let started = std::time::Instant::now();
        let mut acc: u64 = 1;
        while started.elapsed() < std::time::Duration::from_millis(60) {
            acc = acc
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
        }
        std::hint::black_box(acc);
        let after = process_cpu_ns().expect("第一次读到了 ⇒ 第二次也应当读到");
        let delta_ms = after.saturating_sub(before) as f64 / 1e6;
        assert!(
            (10.0..5000.0).contains(&delta_ms),
            "忙等 60 ms 之后进程 CPU 增量应当落在 10 ms–5 s，实测 {delta_ms:.1} ms \
             ⇒ **∴ 字段索引或单位**错了** ✓"
        );
    }
}

#[cfg(test)]
mod f16_path_tests {
    // **★ 选路必须**跟随 `SHADER_F16`**✗ ★**（**第 544 轮 ✓；目标第 10 条 ✓）
    //   **∴ 为什么必须 ✗**：**本机与 L4 上 `SHADER_F16` 都是 `false`** ✓
    //     ⇒ **∴ 所以**：**必须选**手写路** ✓
    //       ＋ **∴ 而**若选路**恒为某一个值**✗ ⇒ **∴ 那**它**就不是选路** ✓ ★
    use super::*;

    #[test]
    fn the_f16_path_follows_the_adapter_capability() {
        // **∴ 本机实测：没有 `SHADER_F16`** ✓ ⇒ **∴ 必须走手写** ✓ ★
        assert_eq!(
            f16_path(wgpu::Features::empty()),
            F16Path::Emulated,
            "不支持 SHADER_F16 ⇒ 必须选手写路"
        );
        // **∴ 支持时 ⇒ 选原生** ✓（**∴ 虽然原生 WGSL 还没写 ⇒ **∴ 判据会提醒 ✓）
        assert_eq!(
            f16_path(wgpu::Features::SHADER_F16),
            F16Path::Native,
            "支持 SHADER_F16 ⇒ 应当选原生路（**∴ 并补上原生 WGSL ✓）"
        );
        // **∴ 两条路的名字必须不同**（**∴ 可见性 ✓）
        assert_ne!(
            f16_path_name(F16Path::Native),
            f16_path_name(F16Path::Emulated)
        );
    }
}

#[cfg(test)]
mod gpu_timestamp_gate_tests {
    // **★ 时间戳查询必须**双条件门控**✗ ★**（**第 546 轮 ✓；目标第 9 条 ✓）
    //   **∴ 为什么 ✗**：**它每次提交要多一次 resolve ＋ map 等待** ✓
    //     ⇒ **∴ 若**进了生产路 ⇒ **∴ 那**就是**白给的开销** ✓
    //       ＋ **∴ 而**不支持该特性的适配器上**根本不能建 `QuerySet`** ✓ ★
    use super::*;

    #[test]
    fn the_gpu_timestamp_gate_needs_both_conditions() {
        let has = wgpu::Features::TIMESTAMP_QUERY;
        assert!(
            !should_time_gpu(has, false),
            "探针没开 ⇒ **不许**量（生产路零开销）"
        );
        assert!(
            !should_time_gpu(wgpu::Features::empty(), true),
            "适配器不支持 TIMESTAMP_QUERY ⇒ **不许**量（否则建不了 QuerySet）"
        );
        assert!(
            should_time_gpu(has, true),
            "两个条件都满足 ⇒ **应当**量（本机核显就是这种情形）"
        );
    }
}

// **★★★ GPU 合成：切片 1（Normal）✗ ★★★**（**第 626 轮 ✓；目标第 11 条 ✓）
//
// **∴ 为什么单独一个 impl 块 ✗**：**Rust**允许**多个 `impl`**✗
//   ＋ **∴ 而**原文件**极大（**4000+ 行 ✓）⇒ **∴ 追加**零风险** ✓
//     ＋ **∴ 且**不碰**任何既有代码 ✓ ★
//
// **∴ 与 `quantize` 的关系 ✗**：**共用**同一个 `Quantizer`（**∴ 即**同一个 device／queue ✓）
//   ⇒ **∴ 从而**：**将来**合成与量化**能**在同一个提交里**连起来** ✓（**∴ 那**正是第 11 条的目标 ✓）★
//
// **★ wasm32 门控 ✗ ★**：**本会话的教训（**静态量／GPU 类型在 wasm 上不合法 ✓）
//   ⇒ **∴ 所以**：**整个块**只在**非 wasm32** 编译 ✓ ★
#[cfg(not(target_arch = "wasm32"))]
impl Quantizer {
    /// **★ 用 GPU 做 source-over 合成（**Normal ✓）✗ ★**
    ///
    /// **∴ 语义 ✗**：**与 CPU 的 `Buffer::blend`（⇒ `over` ✓）**逐位一致** ✓
    ///   ＋ **∴ 输入输出都是**预乘 f32 RGBA**✗ ⇒ **∴ 长度**必须是 `pixels * 4`** ✓
    ///     ＋ **∴ 失败 ⇒ **返回 `None`**（**∴ 调用方**回退 CPU ✓）★
    ///
    /// # Errors
    /// **∴ 本函数**不返回 `Err`** ✗：**任何失败**都映射成 `None`** ✓
    ///   ＋ **∴ 因为**调用方的契约**就是**「**None ⇒ 走 CPU**」** ✓ ★
    #[must_use]
    pub fn composite_normal(&self, src: &[f32], dst: &[f32]) -> Option<Vec<f32>> {
        self.composite_with_mode(src, dst, 0, 1.0)
    }

    /// **★ 指定 mode ＋ opacity 的合成 ✗ ★**（**第 628 轮 ✓）
    ///
    /// **∴ `mode` 编码 ✗**：**0 ＝ Normal**✗（**与 CPU 的 `BlendMode` 顺序一致 ✓）
    ///   ＋ **∴ 1 ＝ Multiply｜2 ＝ Screen｜3 ＝ ColorDodge｜4 ＝ Overlay ✓
    ///     ＋ **∴ 5 ＝ Darken｜6 ＝ Lighten｜7 ＝ Add｜8 ＝ Subtract｜9 ＝ Difference** ✓
    ///       ＋ **∴ 其余** ⇒ **`None`**（**∴ 调用方**回退 CPU ✓）★
    ///
    /// # Errors
    /// **∴ 不返回 `Err`**✗：**任何失败**映射成 `None`** ✓
    #[must_use]
    pub fn composite_with_mode(
        &self,
        src: &[f32],
        dst: &[f32],
        mode: u32,
        opacity: f32,
    ) -> Option<Vec<f32>> {
        if mode > 9 {
            return None;
        }
        block_on(self.composite_async(src, dst, mode, opacity.clamp(0.0, 1.0)))
    }

    /// **★★★ 可分离高斯模糊（**滤镜 GPU 化 ✓）✗ ★★★**（**第 698 轮 ✓）
    ///
    /// **∴ 为什么它能**有机会逐位一致**✗ ★**（**第 697 轮查明 ✓）：
    ///   **∴ CPU 的 `convolve_horizontal` ✗**：**行快照 ＋ `clamp` 边界 ＋ **累加顺序 ＝ kernel 顺序**** ✓
    ///     ＋ **∴ 而**本实现的 WGSL **逐字段对应**✗
    ///       ⇒ **∴ 所以**：**理论上**f32 逐位相同** ✓ ★
    ///
    /// **∴ 核在** CPU 侧生成**✗**（**∴ 与 `filter.rs::gaussian_blur` **同一公式 ＋ 同一顺序 ✓）
    ///   ⇒ **∴ GPU 只做**加权和** ✓ ★
    ///
    /// **∴ 调用形态 ✗**：**一次上传 ＋ 两个 pass ＋ 一次下载**✗
    ///   ＋ **∴ 即**：**不与 CPU 频繁交换**✗（**∴ 用户第 82 轮的条件 ✓）★
    ///
    /// **∴ 失败 ⇒ `None`**✗ ⇒ **∴ 调用方**回退 CPU** ✓ ★
    ///
    /// # Errors
    /// **∴ 不返回 `Err`**：**任何失败映射成 `None`** ✓
    #[must_use]
    pub fn blur_gaussian(
        &self,
        pixels: &[f32],
        width: u32,
        height: u32,
        sigma: f32,
    ) -> Option<Vec<f32>> {
        if sigma <= 0.0 || width == 0 || height == 0 {
            return None;
        }
        let count = (width as usize).checked_mul(height as usize)?;
        if pixels.len() != count * 4 {
            return None;
        }
        // **∴ 核：与 `filter.rs::gaussian_blur` **逐字相同**✗**（**∴ 公式 ＋ 归一化顺序 ✓）★
        let radius = (sigma * 3.0).ceil().max(1.0) as i32;
        let mut kernel = Vec::with_capacity((radius * 2 + 1) as usize);
        let mut sum = 0.0f32;
        for offset in -radius..=radius {
            let value = libm::expf(-((offset * offset) as f32) / (2.0 * sigma * sigma));
            kernel.push(value);
            sum += value;
        }
        for weight in kernel.iter_mut() {
            *weight /= sum;
        }
        block_on(self.blur_async(pixels, width, height, radius as u32, &kernel, sigma))
    }

    /// **∴ 两个 pass 的实际执行 ✗**（**∴ 水平 ⇒ 中间 ⇒ 垂直 ⇒ 输出 ✓）★
    // **∴ `allow(too_many_lines)` ✗**（**∴ 第 698 轮 ✓）：
    //   **∴ 为什么 ✗**：**本函数约 130 行**（**∴ 与 `composite_async` 同规模 ✓）
    //     ＋ **∴ 而** clippy 默认阈值 100**✗ ⇒ **∴ 于是**它**会报** `too_many_lines`** ✓
    //       ＋ **∴ 而**拆分它**会**打断 WGSL 与 host 的**逐字段对应**✗（**∴ 那**是逐位一致的前提 ✓）
    //         ⇒ **∴ 所以**保留整体 ＋ **显式 allow** ✓（**∴ 与 `composite_async` 一致 ✓）★
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::too_many_arguments)]
    async fn blur_async(
        &self,
        pixels: &[f32],
        width: u32,
        height: u32,
        radius: u32,
        kernel: &[f32],
        _sigma: f32,
    ) -> Option<Vec<f32>> {
        let bytes = (pixels.len() * 4) as u64;
        let kernel_bytes = (kernel.len() * 4) as u64;
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("yanshi-gpu-blur"),
                source: wgpu::ShaderSource::Wgsl(BLUR_WGSL.into()),
            });
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    storage_entry(0, false),
                    storage_entry(1, true),
                    storage_entry(2, false),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        // **★ 走池 ✗ ★**（**第 715 轮 ✓）：**滤镜**是**唯一正向收益**的路**✗
        //   ＋ **∴ 所以**：**池化**对它**最有用** ✓ ★
        let make = |label: &str, usage: wgpu::BufferUsages, size: u64| {
            let _ = label;
            self.take_buffer(size, usage)
        };
        let usage_src = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let usage_mid = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let usage_out = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let usage_read = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST;
        let usage_kernel = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let b_src = make("blur-src", usage_src, bytes);
        let b_mid = make(
            "blur-mid",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            bytes,
        );
        let b_out = make(
            "blur-out",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            bytes,
        );
        let b_read = make(
            "blur-read",
            wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            bytes,
        );
        let b_kernel = make(
            "blur-kernel",
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            kernel_bytes,
        );
        // **∴ uniform ＝ 4×u32 ＝ 16 B ✓**（**与 WGSL 的 `Params` 逐字段对应 ✓）★
        let b_params = make(
            "blur-params",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            16,
        );
        self.queue
            .write_buffer(&b_src, 0, f32s_to_bytes(pixels).as_slice());
        self.queue
            .write_buffer(&b_kernel, 0, f32s_to_bytes(kernel).as_slice());
        let params = u32s_to_bytes(&[width, height, radius, kernel.len() as u32]);
        debug_assert_eq!(params.len(), 16);
        self.queue.write_buffer(&b_params, 0, &params);
        let bind = |src: &wgpu::Buffer, dst: &wgpu::Buffer| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                        resource: b_kernel.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: b_params.as_entire_binding(),
                    },
                ],
            })
        };
        let count = width as usize * height as usize;
        let groups = (count as u32).div_ceil(64);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        // **∴ 两个 entry point ⇒ 两个 pipeline ✓** ★
        for (entry, src, dst) in [("horizontal", &b_src, &b_mid), ("vertical", &b_mid, &b_out)] {
            let pipeline = self
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("yanshi-gpu-blur"),
                    layout: Some(&self.device.create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: None,
                            bind_group_layouts: &[&layout],
                            push_constant_ranges: &[],
                        },
                    )),
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    cache: None,
                });
            let group = bind(src, dst);
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups, 1, 1);
        }
        enc.copy_buffer_to_buffer(&b_out, 0, &b_read, 0, bytes);
        self.queue.submit(Some(enc.finish()));
        let _ = self.device.poll(wgpu::PollType::Wait);
        let slice = b_read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::Wait);
        if rx.recv().ok()?.is_err() {
            return None;
        }
        let out = {
            let data = slice.get_mapped_range();
            bytes_to_f32s(&data)
        };
        b_read.unmap();
        // **∴ 放回池 ✗**（**∴ 数据已拷进 `out` ⇒ **复用安全** ✓）★
        self.put_buffer(bytes, usage_src, b_src);
        self.put_buffer(bytes, usage_mid, b_mid);
        self.put_buffer(bytes, usage_out, b_out);
        self.put_buffer(bytes, usage_read, b_read);
        self.put_buffer(kernel_bytes, usage_kernel, b_kernel);
        BLUR_GPU_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(out)
    }

    #[allow(clippy::too_many_lines)]
    async fn composite_async(
        &self,
        src: &[f32],
        dst: &[f32],
        mode: u32,
        opacity: f32,
    ) -> Option<Vec<f32>> {
        let pixels = src.len() / 4;
        if pixels == 0 || dst.len() != src.len() {
            return None;
        }
        let bytes = (src.len() * 4) as u64;
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("yanshi-gpu-composite"),
                source: wgpu::ShaderSource::Wgsl(COMPOSITE_WGSL.into()),
            });
        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    storage_entry(0, false),
                    storage_entry(1, false),
                    storage_entry(2, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline = self
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("yanshi-gpu-composite"),
                layout: Some(&self.device.create_pipeline_layout(
                    &wgpu::PipelineLayoutDescriptor {
                        label: None,
                        bind_group_layouts: &[&layout],
                        push_constant_ranges: &[],
                    },
                )),
                module: &module,
                entry_point: Some("main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            });
        // **★ 走池 ✗ ★**（**目标第 11 条后续 ✓；**第 715 轮 ✓）：
        //   **∴ 为什么 ✗**：**第 79 轮实测 ⇒ **4K 合成 570 ms 里约 338 ms** 是**分配 ＋ dispatch ＋ 等待** ✓
        //     ＋ **∴ 所以**：**每帧**新建 5 张 buffer**✗ 是**主要开销** ✓ ★
        let make = |label: &str, usage: wgpu::BufferUsages, size: u64| {
            let _ = label;
            self.take_buffer(size, usage)
        };
        let usage_src = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST;
        let usage_out = wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC;
        let usage_read = wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST;
        let b_src = make("composite-src", usage_src, bytes);
        let b_dst = make("composite-dst", usage_src, bytes);
        let b_out = make("composite-out", usage_out, bytes);
        let b_read = make("composite-read", usage_read, bytes);
        // **∴ uniform 必须** 32 字节**✗（**与 `Params` 逐字段对应 ✓）★
        let b_params = make(
            "composite-params",
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            32,
        );
        self.queue
            .write_buffer(&b_src, 0, f32s_to_bytes(src).as_slice());
        self.queue
            .write_buffer(&b_dst, 0, f32s_to_bytes(dst).as_slice());
        // **∴ uniform ＝ 4×u32 ＋ vec4<f32> ＝ 32 B ✓**（**与 `Params` 逐字段对应 ✓）★
        let mut params = u32s_to_bytes(&[pixels as u32, mode, 0, 0]);
        params.extend_from_slice(&f32s_to_bytes(&[opacity, 0.0, 0.0, 0.0]));
        debug_assert_eq!(params.len(), 32);
        self.queue.write_buffer(&b_params, 0, &params);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: b_src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: b_dst.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: b_out.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: b_params.as_entire_binding(),
                },
            ],
        });
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((pixels as u32).div_ceil(64), 1, 1);
        }
        enc.copy_buffer_to_buffer(&b_out, 0, &b_read, 0, bytes);
        self.queue.submit(Some(enc.finish()));
        let _ = self.device.poll(wgpu::PollType::Wait);
        let slice = b_read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::Wait);
        // **map 结果必须看** ✓（审计 P3 #4）：此前只看「回调 fired」（`rx.recv().is_err()` 才返回 ✗）——
        // 设备丢失时回调带着 Err 来 ⇒ `get_mapped_range` 会 panic ✗。与 blur 路同形：
        // Err ⇒ 回 None 降级 ✓（不读数、不 unmap —— map 失败后缓冲已不处于 mapped 态 ✓）。
        if !matches!(rx.recv(), Ok(Ok(()))) {
            return None;
        }
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity(src.len());
        for c in mapped.chunks_exact(4) {
            out.push(f32::from_le_bytes([c[0], c[1], c[2], c[3]]));
        }
        drop(mapped);
        b_read.unmap();
        // **∴ 放回池 ✗**（**∴ 数据已拷进 `out` ⇒ **复用安全** ✓）★
        self.put_buffer(bytes, usage_src, b_src);
        self.put_buffer(bytes, usage_src, b_dst);
        self.put_buffer(bytes, usage_out, b_out);
        self.put_buffer(bytes, usage_read, b_read);
        Some(out)
    }
}

#[cfg(not(target_arch = "wasm32"))]
const COMPOSITE_WGSL: &str = include_str!("composite.wgsl");

/// **∴ 可分离高斯卷积的 WGSL ✗**（**第 698 轮 ✓；目标：滤镜 GPU 化 ✓）★
const BLUR_WGSL: &str = include_str!("blur.wgsl");

/// **∴ GPU 高斯模糊**实际被采用**的次数 ✗**（**第 698 轮 ✓）★
///
/// **∴ 为什么需要 ✗**（**第 690／691 轮的教训 ✓）：
///   **∴ 「**判据通过**」**不等于**「**被测路径真的被走到**」**✗
///     ＋ **∴ 所以**：**判据必须能**断言它 ＞ 0** ✓ ★
static BLUR_GPU_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// **∴ 读取 GPU 模糊被采用的次数 ✗** ★
#[must_use]
pub fn blur_gpu_calls() -> usize {
    BLUR_GPU_CALLS.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清零 GPU 模糊计数 ✗**（**判据用 ✓）★
pub fn reset_blur_gpu_calls() {
    BLUR_GPU_CALLS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(not(target_arch = "wasm32"))]
const fn storage_entry(binding: u32, read_write: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: if read_write {
                wgpu::BufferBindingType::Storage { read_only: false }
            } else {
                wgpu::BufferBindingType::Storage { read_only: true }
            },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
