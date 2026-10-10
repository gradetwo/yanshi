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
