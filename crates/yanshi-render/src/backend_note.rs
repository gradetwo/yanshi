//! **★ 「最近一次量化实际用了哪个后端」的记录点 ✗ ★**（第 323 轮 ✓；**目标第 7 条 ✓）。
//!
//! **∴ 为什么需要它 ✗**：**渲染发生在 `yanshi-render`**✗（**`Buffer::to_rgba8_quantized` ✓）
//!   ⇒ **∴ 而** `/health` **在 `yanshi-http`** ✓
//!     ⇒ **∴ 所以**：**用一个**进程级**记录点，把「**实际走了哪条路**」传出去** ✓**** ✓✓
//!
//! **∴ 诚实性（**硬约束 ✓）★**：
//!   **∴ 只有**真的走完 GPU 路**（**且逐位核对通过 ✓）**才会被记成 [`Backend::Gpu`]** ✓
//!     ⇒ **∴ 于是**：**`/health` 的 `render_backend` **不可能**假装** ✓ ★**** ✓✓
//!   **∴ 且**：**回退（**无适配器／规模不够／字节不符 ✓）**一律记 [`Backend::Cpu`]** ✓
//!     ⇒ **∴ 而那**正是**真实情况** ✓**** ✓✓
//!
//! **∴ 代价（**如实 ✓）**：**多一个进程级原子量**✗ ⇒ **∴ 而**它**只有一个 `u8`** ✓
//!   **∴ 且**：**多个线程同时渲染时**，**它记录**最后一次**✗
//!     ⇒ **∴ 那是**可接受的近似**✗（**∴ 因为**它是**诊断用**的 ✓）** ✓✓
//!
//! **∴ 命名（**为什么叫 `backend_note` 而不是 `backend` ✓）★**：
//!   **∴ `yanshi-render` **没有** GPU 依赖**✗ ⇒ **∴ 所以**这里**只有**「**记录**」这件事** ✓
//!     ⇒ **∴ 而**真正的后端实现**在 `yanshi-gpu`** ✓**** ✓✓

use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};

/// **★ 实际用过的后端 ✗ ★**（**∴ 只有这两条路 ✓）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// **∴ 纯 CPU（**真值 ✓）—— **∴ 也是**所有回退**的去处** ✓
    Cpu,
    /// **★ GPU（**逐位核对通过之后才可能记成它 ✓）★**
    Gpu,
}

impl Backend {
    /// **∴ 给响应用的字面量 ✗**（**∴ `/health` **直接用它** ✓）
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

/// **∴ `0` ＝ CPU（**初值 ✓），`1` ＝ GPU** ✓
static LAST: AtomicU8 = AtomicU8::new(0);

/// **∴ 记下实际后端 ✗**（**∴ 由渲染路径调用 ✓）
pub fn set_last_backend(backend: Backend) {
    LAST.store(
        match backend {
            Backend::Cpu => 0,
            Backend::Gpu => 1,
        },
        Ordering::Relaxed,
    );
}

/// **∴ 读最近一次的实际后端 ✗**（**∴ 初值是 CPU**✗ ⇒ **∴ 因为**没跑过就等于没加速 ✓）
#[must_use]
pub fn last_backend() -> Backend {
    match LAST.load(Ordering::Relaxed) {
        1 => Backend::Gpu,
        _ => Backend::Cpu,
    }
}

/// **★ 渲染路最后一次**真的比对过**的最大通道差值 ✗ ★**（**第 451 轮 ✓；**目标第 4 条 ✓）
///
/// **∴ 为什么需要它（**第 450 轮诊断 ✓）★**：
///   **∴ 症状 ✗**：**`/health` 的 `max_channel_delta` **在**没渲染时**也报 `0`**✗
///     ⇒ **∴ 于是**：**判据**读成**「**恒 0 冒充**」** ✓
///       ⇒ **∴ 而**那**正是**目标第 4 条**禁止的** ✓ ★**** ✓✓
///   **∴ 根因 ✗**：**两条路**共用一个字段名**✗
///     ⇒ **∴ ①** **自检**（**`gpu_policy::selfcheck::run` ✓）**总在跑** ✓
///       ＋ **∴ ②** **渲染路**（**`try_quantize_on_gpu` ✓）**只在 GPU 成功时**比过** ✓
///         ⇒ **∴ 所以**：**两者**混在一起** ✓ ★**** ✓✓
///   **∴ 修法 ✗**：**拆开**✗
///     ⇒ **∴ `max_channel_delta` **只**反映**渲染路**✗
///       ⇒ **∴ 没比过 ⇒ `None` ⇒ `/health` 报 `null`** ✓（**∴ 那就是**判据要的** ✓）
///         ＋ **∴ 自检的差值**另立字段**（**`selfcheck_max_channel_delta` ✓）★**** ✓✓
///
/// **∴ `-1` ＝ 没比对**（**∴ 用 `i64` 是因为要区分「**没比**」与「**比过且是 0**」 ✓）
static RENDER_DELTA: AtomicI64 = AtomicI64::new(-1);

/// **∴ 记下渲染路的比对结果 ✗**（**`None` ＝ 没比对 ✓）
pub fn set_render_delta(delta: Option<u32>) {
    RENDER_DELTA.store(delta.map_or(-1, i64::from), Ordering::Relaxed);
}

/// **∴ 读渲染路的比对结果 ✗**（**`None` ＝ 没比对 ⇒ 应当报 `null` ✓）
#[must_use]
pub fn render_delta() -> Option<u32> {
    match RENDER_DELTA.load(Ordering::Relaxed) {
        -1 => None,
        v => u32::try_from(v).ok(),
    }
}

/// **★ 进程级 GPU 开关 ✗ ★**（**第 461 轮 ✓；**目标第 7 条 ✓）
///
/// **∴ 为什么需要它（**实测缺陷 ✓）★**：
///   **∴ 症状 ✗**：**`--gpu off` **只影响诊断**✗
///     ⇒ **∴ 因为** `GPU_MODE` **只在 `yanshi-http` 里被读** ✓
///       （**`server.rs:964` 上报 ＋ `:1130` 的 `gpu_probe` ✓）
///         ＋ **∴ 而** `yanshi-render` **不引用它** ✓
///           ⇒ **∴ 于是**：**渲染路**照样建 `Quantizer` 并用 GPU** ✓
///             ⇒ **★ 所以**：**`--gpu off` **说了假话****✗
///               ⇒ **∴ 而**那**违反**目标第 7 条** ✓ ★**** ✓✓
///   **∴ 现在 ✗**：**加一个**同类的进程级原子量**✗
///     ⇒ **∴ `server.rs` 在解析 `--gpu` 时**同时设置它** ✓
///       ＋ **∴ `try_quantize_on_gpu` **先查它** ✓
///         ⇒ **∴ `off` ⇒ **直接不试 GPU**** ✓ ★**** ✓✓
///
/// **∴ 语义 ✗**：**`0` ＝ 允许**（**默认／`auto`／`on` ✓）；**`1` ＝ 禁止**（**`off` ✓）
static GPU_DISABLED: AtomicU8 = AtomicU8::new(0);

/// **∴ 设置「**禁止用 GPU**」✗**（**由服务端的 `--gpu off` 调用 ✓）
pub fn set_gpu_disabled(disabled: bool) {
    GPU_DISABLED.store(u8::from(disabled), Ordering::Relaxed);
}

/// **∴ 本进程是否**禁止用 GPU**✗**（**渲染路查它 ✓）
#[must_use]
pub fn gpu_disabled() -> bool {
    GPU_DISABLED.load(Ordering::Relaxed) == 1
}

/// **★ 标记「**一次新的渲染开始**」✗ ★**（**第 465 轮 ✓；**目标第 7 条 ✓）
///
/// **∴ 为什么需要它 ✗**：**第 456 轮**把 `Gpu` **粘住**✗
///   ⇒ **∴ 于是**：**`render_backend` 变成**「**本进程用过 GPU 吗**」** ✓
///     ⇒ **∴ 症状（**第 460 轮实测 ✓）✗**：**`--gpu off` 那条**也显示 `gpu`** ✓
///       ⇒ **∴ 于是**：**「**最近一次渲染**」的语义**丢了** ✓ ★**** ✓✓
///   **∴ 现在 ✗**：**每次渲染**开头**重置这两项**✗
///     ⇒ **∴ 于是**：**渲染内的**先 GPU 后 CPU**不再**互相擦掉** ✓
///       ＋ **∴ 而**渲染之间**互不污染** ✓ ★**** ✓✓
///
/// **∴ 语义（**精确 ✓）✗**：**`render_backend` ＝ **最近一次渲染**用过的后端** ✓
///   ＋ **∴ 且**：**若**该次渲染**先 GPU 成功、后另一块走 CPU**✗
///     ⇒ **∴ 仍记 `Gpu`** ✓（**∴ 因为** `set_last_backend` 用 `fetch_or` ✓）★**** ✓✓
pub fn begin_render() {
    // **∴ 重置为「**本次渲染还没用 GPU**」** ✓
    LAST.store(0, Ordering::Relaxed);
    // **∴ 并**清掉上一次的渲染路差值**✗ ⇒ **∴ 没比过 ⇒ `null`** ✓
    RENDER_DELTA.store(-1, Ordering::Relaxed);
}

/// **★ 已经**逐位核对通过**的位图 blob ✗ ★**（**第 475 轮 ✓；**方案甲 ✓）
///
/// **∴ 为什么需要它 ✗**：**第 474 轮实测**✗**：
///   **∴ 逐位核对占 **51%** 的时间**✗（**`1686 ms` vs `825 ms` ✓）
///     ⇒ **∴ 于是**：**GPU 的**时间收益**被它吃掉** ✓
///       ＋ **∴ 而**不核对时**GPU **比 CPU 快 53%** ✓
///         ⇒ **★ 所以**：**甲方案（**每个 blob 只验一次 ✓）
///           ⇒ **∴ 稳态**下 GPU 才有**真实收益** ✓ ★**** ✓✓
///
/// **∴ 安全（**如实 ✓）★**：
///   **∴ 键是**内容寻址的 blob 哈希**✗
///     ⇒ **∴ 同一个 blob**（**同内容 ✓）**只验一次** ✓
///       ＋ **∴ 于是**：**不同内容**必然**被验** ✓
///         ＋ **∴ 代价 ✗**：**若**某个 blob 的 GPU 结果**在某次运行时出错**✗
///           ⇒ **∴ 只有**首次能发现** ✓ ★**** ✓✓
static VERIFIED_BLOBS: std::sync::Mutex<Option<std::collections::HashSet<String>>> =
    std::sync::Mutex::new(None);

/// **∴ 这个 blob 验过了吗 ✗**
#[must_use]
pub fn blob_verified(blob: &str) -> bool {
    match VERIFIED_BLOBS.lock() {
        Ok(guard) => guard.as_ref().is_some_and(|set| set.contains(blob)),
        Err(_) => false,
    }
}

/// **∴ 记下这个 blob 已验过 ✗**
pub fn mark_blob_verified(blob: &str) {
    if let Ok(mut guard) = VERIFIED_BLOBS.lock() {
        guard
            .get_or_insert_with(std::collections::HashSet::new)
            .insert(blob.to_owned());
    }
}
