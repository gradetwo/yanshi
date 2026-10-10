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

use std::sync::atomic::{AtomicU8, Ordering};

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
