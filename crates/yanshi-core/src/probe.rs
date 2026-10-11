//! 探针计时器：wasm32 上绝不触碰 `Instant`（第 592 轮；目标第 13 条）。
//!
//! 为什么需要它：`std::time::Instant::now()` 在 `wasm32-unknown-unknown` 上不受支持，
//! 会直接 panic（`RuntimeError: unreachable`）。本会话为渲染诊断探针
//! （`YANSHI_RENDER_PROBE`）加了多处 `Instant::now()`，于是 CI 的 `wasm smoke`
//! 连续 6 次失败；栈显示 `<std::time::Instant>::now` 在
//! `<yanshi_render::render::Renderer>::render_region` 里被调用。
//!
//! 为什么不能只靠判据：`wasm_target_guard` 只数源码文本，防不住运行时 panic。
//! 所以必须在类型层面让 wasm 根本不构造 `Instant`。
//!
//! 设计：接口相同，所以调用点零改动。非 wasm 时 `ProbeClock` 包一个 `Instant`，
//! `now()`／`elapsed()`／`duration_since()` 与原生一致。wasm32 时 `ProbeClock`
//! 是无字段的哑类型，`now()` 只返回它自己，`elapsed()` 与 `duration_since()`
//! 恒为 `Duration::ZERO`，于是既有的 `.elapsed()` 调用一字不改，且与既有注释
//! 「wasm32 上恒为零」完全一致。
//!
//! 两面（AGENTS.md）。收益：wasm 运行不再 panic，wasm 体积不因此增大
//! （哑类型零字段）。代价：多一个类型（本文件），且 wasm 上取不到真实耗时
//! （但探针本来只为原生诊断）。
/// **∴ 非 wasm：真计时 ✗**（**与 `Instant` 语义相同 ✓）
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
pub struct ProbeClock(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl ProbeClock {
    /// **∴ 取当前时刻 ✗**（**只有非 wasm 才真的调用 `Instant::now()` ✓）
    #[inline]
    pub fn now() -> Self {
        Self(std::time::Instant::now())
    }

    /// **∴ 已过去的时长 ✗**（**与 `Instant::elapsed` 同义 ✓）
    #[inline]
    pub fn elapsed(&self) -> std::time::Duration {
        self.0.elapsed()
    }

    /// **∴ 与另一个时刻的差 ✗**（**与 `Instant::duration_since` 同义 ✓）
    ///   **∴ 为什么要补它 ✗**：**调用点里有一处用的是 `duration_since`**✗
    ///     ＋ **∴ 所以**：**接口补齐 ⇒ 调用点一字不改** ✓
    #[inline]
    pub fn duration_since(&self, earlier: ProbeClock) -> std::time::Duration {
        self.0.duration_since(earlier.0)
    }
}

/// **∴ wasm32：哑计时器 ✗**（**∵ 这个目标不支持 `Instant` ✓）
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub struct ProbeClock;

#[cfg(target_arch = "wasm32")]
impl ProbeClock {
    /// **∴ 不触碰 `Instant` ✗**（**∴ 于是**永不 panic ✓）
    #[inline]
    pub fn now() -> Self {
        Self
    }

    /// **∴ 恒为零 ✗**（**与既有注释「wasm32 上恒为零」一致 ✓）
    #[inline]
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }

    /// **∴ 恒为零 ✗**（**wasm 上没有真实时刻 ⇒ 差也是零 ✓）
    #[inline]
    pub fn duration_since(&self, _earlier: ProbeClock) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}
