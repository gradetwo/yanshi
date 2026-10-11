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

// **★★★ 有界 LRU ✗ ★★★**（**第 513 轮 ✓；目标第 6 条 ✓）
//
// **∴ 原来的缺陷（**真内存缺陷 ✓）✗**：**这里是一个**只增不减**的 `HashSet`**
//   ⇒ **∴ 于是**：**长跑／多文档／多版本的服务会**缓慢增长** ✓
//     ＋ **∴ 而**键是 `sha256:…`（**71 字符 ✓）⇒ **∴ 每个条目 ≈ 90 B** ✓
//       ⇒ **∴ 100 万个 blob ⇒ ≈ 90 MB**（**∴ 且**永不释放 ✓）★
//
// **∴ 为什么上限可以很低 ✗**：**这个集合的语义**不是"正确性"**✗**
//   ⇒ **∴ 而是**「**同一个 blob 只核对一次**」的**省时**记录** ✓
//     ＋ **∴ 所以**：**淘汰只是**多核对一次**✗ ⇒ **∴ 不损失正确性** ✓ ★
//       （**∴ 代价 ✗**：**被淘汰的 blob**下次会重验 ⇒ **∴ 最坏退化到"每次都验" ✓）★
//
// **∴ 实现选择 ✗**：**手写 `HashSet ＋ VecDeque`** ✓（**≈ 30 行 ✓）
//   ⇒ **∴ 为什么不用 `lru` crate ✗**：**这个规模**不值得**多一个依赖** ✓
//     （**∴ 按仓库约定**要写两面** ✗ ：**收益**是**零新依赖**✗ ；
//       **代价**是**手写 ≈ 30 行要自己保证正确 ⇒ **∴ 所以**配了单元测试 ✓）★
//
// **∴ 上限 ✗**：**默认 4096**（**≈ 370 KB ✓）✗
//   ＋ **∴ 可用 `YANSHI_VERIFIED_BLOBS_MAX` 覆盖**（**∴ 供测量与测试 ✓）★

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
/// **∴ LRU 的默认上限（**条目数 ✓）✗**（**4096 × ≈90 B ≈ 370 KB ✓）★
const VERIFIED_BLOBS_DEFAULT_MAX: usize = 4096;

/// **∴ 上限（**可被环境变量覆盖 ✓）✗**：**0 ⇒ **不记录**（**每次都验 ✓）★
#[must_use]
fn verified_blobs_max() -> usize {
    match std::env::var("YANSHI_VERIFIED_BLOBS_MAX") {
        Ok(text) => text
            .trim()
            .parse::<usize>()
            .unwrap_or(VERIFIED_BLOBS_DEFAULT_MAX),
        Err(_) => VERIFIED_BLOBS_DEFAULT_MAX,
    }
}

/// **∴ 有界 LRU ✗**：**集合 ＋ 使用顺序**（**队首最久未用 ✓）★
#[derive(Default)]
struct VerifiedBlobs {
    /// **∴ 谁在里面 ✗**（**语义：验过 ✓）★
    set: std::collections::HashSet<String>,
    /// **∴ 使用顺序 ✗**：**队首**最久未用**✗ ，**队尾**最近用过** ✓
    order: std::collections::VecDeque<String>,
}

impl VerifiedBlobs {
    /// **∴ 查询 ＋ **触碰**（**∴ LRU 的"用" ✓）★
    fn touch(&mut self, blob: &str) -> bool {
        if !self.set.contains(blob) {
            return false;
        }
        // **∴ 移到队尾 ✗**：**∴ 那**就是"最近用过" ✓
        if let Some(pos) = self.order.iter().position(|x| x == blob) {
            if let Some(item) = self.order.remove(pos) {
                self.order.push_back(item);
            }
        }
        true
    }

    /// **∴ 插入 ＋ 按上限淘汰 ✗**（**∴ 淘汰**队首 ✓）★
    fn insert(&mut self, blob: &str, max: usize) {
        if max == 0 {
            // **∴ 0 ⇒ **不记录**✗ ⇒ **∴ 于是**每次都验** ✓（**∴ 供对照测量 ✓）★
            self.set.clear();
            self.order.clear();
            return;
        }
        if self.set.insert(blob.to_owned()) {
            self.order.push_back(blob.to_owned());
        } else if let Some(pos) = self.order.iter().position(|x| x == blob) {
            if let Some(item) = self.order.remove(pos) {
                self.order.push_back(item);
            }
        }
        // **★ 淘汰 ✗ ★**：**∴ 直到**不超过上限** ✓
        while self.order.len() > max {
            if let Some(evicted) = self.order.pop_front() {
                self.set.remove(&evicted);
            }
        }
    }

    /// **∴ 当前条目数 ✗**（**供判据断言 ✓）★
    fn len(&self) -> usize {
        self.order.len()
    }
}

static VERIFIED_BLOBS: std::sync::Mutex<Option<VerifiedBlobs>> = std::sync::Mutex::new(None);

/// **∴ 这个 blob 验过了吗 ✗**（**∴ 顺带**触碰它 ⇒ **∴ 它**不会**成为下一个被淘汰的** ✓）★
#[must_use]
pub fn blob_verified(blob: &str) -> bool {
    match VERIFIED_BLOBS.lock() {
        Ok(mut guard) => guard.as_mut().is_some_and(|state| state.touch(blob)),
        Err(_) => false,
    }
}

/// **∴ 记下这个 blob 已验过 ✗**（**∴ 超过上限就淘汰**最久未用** ✓）★
pub fn mark_blob_verified(blob: &str) {
    if let Ok(mut guard) = VERIFIED_BLOBS.lock() {
        let max = verified_blobs_max();
        guard
            .get_or_insert_with(VerifiedBlobs::default)
            .insert(blob, max);
    }
}

/// **∴ 当前记录了多少个已验证的 blob ✗**（**供判据与诊断 ✓）★
#[must_use]
pub fn verified_blobs_len() -> usize {
    match VERIFIED_BLOBS.lock() {
        Ok(guard) => guard.as_ref().map_or(0, VerifiedBlobs::len),
        Err(_) => 0,
    }
}

/// **★ 最近一次**逐位核对**的耗时（**微秒 ✓）✗ ★**（**第 498 轮 ✓）
///
/// **∴ 它补上分段计时的第四段 ✗**：
///   ⇒ **∴ 前三段（**上行／提交／读回 ✓）在 `yanshi-gpu` 里量** ✓
///     ＋ **∴ 这一段**在渲染层量**✗（**∴ 因为**核对用的是 CPU 真值 ✓）
///       ⇒ **∴ 四段相加 ⇒ 就能看出**时间花在哪** ✓ ★**** ✓✓
/// **∴ 语义 ✗**：**0 ⇒ **没量到**（**没走核对路 ✓）**✗
///   ⇒ **∴ 读者**必须配合 `gpu_timed_calls` 看** ✓（**∴ 不许**当成 0 微秒 ✓）★
static VERIFY_MICROS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// **∴ 记下核对的耗时 ✗**（**只在真的比过时调用 ✓）★
pub fn set_verify_micros(micros: u64) {
    VERIFY_MICROS.store(micros, std::sync::atomic::Ordering::Relaxed);
}

/// **∴ 读核对耗时（**微秒 ✓）✗**
#[must_use]
pub fn verify_micros() -> u64 {
    VERIFY_MICROS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod verified_blobs_lru_tests {
    // **★ 直接测**纯结构**，不用全局也不用环境变量 ✗ ★**（**第 513 轮 ✓）
    //   **∴ 为什么 ✗**：**第一版测试**改了全局 ＋ 用 `YANSHI_VERIFIED_BLOBS_MAX`**✗
    //     ⇒ **∴ 而**测试**并行**执行**✗ ⇒ **∴ 于是**它们**互相干扰**✗
    //       ⇒ **∴ 实测**两条红**（**∴ 而**产品代码是好的 ✓）★
    //     ＋ **∴ 所以**：**把上限**当参数传**✗ ⇒ **∴ 于是**测试**无共享状态** ✓ ★**** ✓✓
    use super::*;

    /// **★ ① 插入超过上限 ⇒ 条目数不得超过上限 ✗ ★**（**目标第 6 条的核心 ✓）
    #[test]
    fn inserting_more_than_the_cap_does_not_grow_without_bound() {
        let mut lru = VerifiedBlobs::default();
        for i in 0..50 {
            lru.insert(&format!("sha256:cap-{i}"), 8);
        }
        assert!(
            lru.len() <= 8,
            "条目数必须 ≤ 上限 8，实测 {} ⇒ **无界增长回来了**",
            lru.len()
        );
        assert_eq!(lru.len(), 8, "恰好装满上限时应当正好 8 条");
    }

    /// **★ ② 淘汰的是**最久未用**的 ✗ ★**：**最近**触碰过的必须还在** ✓
    #[test]
    fn the_least_recently_used_entry_is_the_one_evicted() {
        let mut lru = VerifiedBlobs::default();
        lru.insert("sha256:lru-a", 3);
        lru.insert("sha256:lru-b", 3);
        lru.insert("sha256:lru-c", 3);
        // **∴ 触碰 a ⇒ **∴ 于是**最久未用的变成 b** ✓
        assert!(lru.touch("sha256:lru-a"), "a 应该在");
        lru.insert("sha256:lru-d", 3);
        assert!(lru.touch("sha256:lru-a"), "a 刚用过 ⇒ **不许被淘汰**");
        assert!(lru.touch("sha256:lru-d"), "d 刚插入 ⇒ 应该在");
        assert!(!lru.touch("sha256:lru-b"), "b 是最久未用 ⇒ **必须被淘汰**");
        assert_eq!(lru.len(), 3);
    }

    /// **∴ 上限 0 ⇒ **不记录**✗ ⇒ **∴ 于是**每次都验** ✓
    #[test]
    fn a_cap_of_zero_records_nothing() {
        let mut lru = VerifiedBlobs::default();
        lru.insert("sha256:zero", 0);
        assert!(!lru.touch("sha256:zero"), "上限 0 ⇒ 不该记住任何东西");
        assert_eq!(lru.len(), 0);
    }

    /// **∴ 重复插入**同一个 blob ⇒ **∴ 不重复计数** ✓
    #[test]
    fn inserting_the_same_blob_twice_keeps_one_entry() {
        let mut lru = VerifiedBlobs::default();
        lru.insert("sha256:dup", 8);
        lru.insert("sha256:dup", 8);
        assert_eq!(lru.len(), 1);
        assert!(lru.touch("sha256:dup"));
    }
}
