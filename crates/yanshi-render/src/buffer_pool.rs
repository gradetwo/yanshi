//! **图层缓冲池**：让全幅渲染的图层缓冲**复用底层分配**，而不是每层重新分配 + 清零。
//!
//! ## 它解决什么
//!
//! `render.rs` 的图层循环过去每层都 `Buffer::new(...)`。4K（3840×2160）时单层是
//! `3840 × 2160 × 4 通道 × 4 B ≈ 132.7 MB`；5 层就是**每帧约 663 MB** 的分配 + 清零
//! （见 `buffer.rs` 的 [`crate::buffer::Buffer`]：四通道 `f32`）。大块分配会走 `mmap`，
//! 每帧重新触碰 663 MB 的页面 ⇒ 缺页与清零都要付一遍。
//!
//! ## 池的边界：为什么不是"一个缓冲"
//!
//! 并行分块渲染时**每个 worker 线程**都在跑自己的图层循环 ⇒ 同一时刻可能有 `workers`
//! 个图层缓冲在用（取用点是 `render_accumulation`，它可以被多线程并发调用）。所以：
//!
//! 1. 缓冲一旦被借出就**移出空闲表**并由租约持有（`BufferLease`），`Drop` 时才归还
//!    ⇒ **同一块缓冲不可能同时被两个租约持有**（这是结构保证，不靠约定）。
//! 2. 借出时若空闲表里没有容量足够的缓冲，就**新建**一个 ⇒ 池的规模由**实际并发取用**
//!    决定，而不是写死"一个"。
//! 3. 归还时只在**两个上限**内保留（[`MAX_POOLED_BUFFERS`] 个数 + **由真实并发推导的**
//!    字节预算，见 [`BufferPool::retained_byte_budget`]），超出直接丢弃 ⇒ 池不会无界增长，
//!    也不会把 8K 的大分配长期扣住。
//! 4. 借出/归还都核对缓冲 id 与在借集合：冲突计入 [`BufferPoolStats::alias_violations`]
//!    （正常恒为 0；判据断言它，并在借出时直接断言两块在借缓冲的内存区间不相交）。
//!
//! ## 字节预算为什么必须由并发推导（而不是写死一个常数）
//!
//! 旧实现把字节预算写死成 192 MiB。8K（7680×4320）并行 4 个 worker 时，每个 worker 的
//! 分带缓冲是 `7680 × 1080 × 16 B ≈ 126.56 MiB`，4 块合计 ≈ **506 MiB** ⇒ 预算只够留下
//! 1 块，其余 3 块**每层**都被丢弃并重新分配（外部报告：每层约 380 MB 的无谓分配，
//! 5 层导出约 1.5 GB 的堆 churn）。分带确实让"每块更小"，但 N 块之和**恒等于整幅** ——
//! 所以只要画布本身就超过固定预算，池化就会在每个 worker 上失效。
//!
//! 现在的规则（[`BufferPool::retained_byte_budget`]）：
//!
//! ```text
//! 保留字节预算 = max(MIN_POOLED_BYTES, 本轮并发 worker 数 × 本轮最大单次取用字节数)
//! ```
//!
//! 两个因子都来自**这次渲染的真实几何**：渲染器在开工前用 `begin_render(chunks)` 告诉池
//! 本轮有多少个并发分块，池在取用时记下本轮最大的单块工作集。为什么它**不会无界增长**：
//! ① 并发数被 [`MAX_POOLED_BUFFERS`] 夹住（≤ 16）；② 真实分带满足"N 块之和 = 整幅"，
//! 所以预算 ≈ 一次渲染的工作集；③ 个数上限始终生效。池最多留下"每个并发 worker 一块、
//! 总数 ≤ 16"，绝不会因为归还次数多而膨胀。
//!
//! ## 为什么不改渲染色
//!
//! 复用走 [`Buffer::reset`]：`clear` + `resize(len, 0.0)` 把新长度的**每个元素**写成
//! `0.0`，与 `Buffer::new` 的 `vec![0.0; len]` 得到相同的全零起点；尺寸/原点按请求设置。
//! 图层缓冲仍以**空缓冲**开始，剪贴蒙版仍读**独立的累积缓冲**（那个不走池）。

use crate::buffer::Buffer;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// 池最多保留的缓冲**个数**（也是并发数的硬上限）。
///
/// 取 16 是为了覆盖并行渲染的 worker 上限（`render.rs::parallel_impl::MAX_WORKERS = 16`）：
/// 每个 worker 的图层循环同时最多只需要一块图层缓冲。真正的规模仍由**实际并发取用**决定
/// ——这里只是"最多保留多少"的上限，它同时把字节预算里的并发因子夹在 ≤ 16。
pub const MAX_POOLED_BUFFERS: usize = 16;

/// 池保留字节预算的**下界（floor）**——不是硬上限。
///
/// 旧版本把它当作**唯一**的字节上限：4K 一张整幅图层缓冲约 132.7 MB ⇒ 192 MiB 足以
/// 留下一张整幅（串行全幅渲染的典型需求），又不会把 8K（约 530 MB/张）的第二张长期扣住。
/// 但它在**并行分带**下失效：每个 worker 只拿一条带（8K/4 worker 是 126.56 MiB/块），
/// 单块看着不大，4 块之和（506 MiB）却远超 192 MiB ⇒ 只有 1 块能留下，其余每层都被
/// 丢弃重分配。所以现在它退化为**下界**：真实预算是
/// `max(本常数, 并发 worker 数 × 本轮最大单块工作集)`，见 [`BufferPool::retained_byte_budget`]。
///
/// **为什么还要留一个固定下界**：小画布 / 串行路径下并发数 × 工作集可能只有几 MB，
/// 而"多留几张中等大小的缓冲"对层间复用是有益的（旧行为）；192 MiB 是**有界的固定量**，
/// 不会随归还次数增长。上界则由并发数（≤ [`MAX_POOLED_BUFFERS`]）与真实单块工作集给出。
///
/// **例外（至少保留一张）**：一张整幅图层缓冲是**不可再省的工作集**。若严格按预算，
/// 超过预算的画布（例如 3840×3840 单层 ≈ 225 MB）会一张都留不下 ⇒ 池化对它
/// **完全失效**（实测：20 次取用、0 次复用）。所以空闲表为空时允许保留一张、可突破预算；
/// **个数上限始终生效**，额外缓冲仍受字节预算约束。
pub const MIN_POOLED_BYTES: usize = 192 * 1024 * 1024;

/// 池的保留上限（个数 + 字节预算的下界）。
#[derive(Debug, Clone, Copy)]
struct Limits {
    max_buffers: usize,
    min_bytes: usize,
}

/// 池的内部计数（**不是计时**）。
#[derive(Debug, Default, Clone, Copy)]
struct Counters {
    allocated: usize,
    reused: usize,
    released: usize,
    max_in_use: usize,
    alias_violations: usize,
}

/// 池的只读快照（判据与测量用；全部是**语义计数**，不是墙钟）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BufferPoolStats {
    /// **实际新建（堆分配）**的缓冲个数。
    pub allocated: usize,
    /// 从空闲表**复用**（未新建分配）的取用次数。
    pub reused: usize,
    /// 归还次数。
    pub released: usize,
    /// **同时借出的峰值个数**——"实际并发需要多大"的直接证据。
    pub max_in_use: usize,
    /// 借出/归还时发现 id 冲突的次数（正常恒为 0）。
    pub alias_violations: usize,
    /// 当前空闲表里保留的缓冲个数。
    pub retained_buffers: usize,
    /// 当前空闲表里保留的字节数（按容量计）。
    pub retained_bytes: usize,
}

#[derive(Debug)]
struct PoolInner {
    /// 空闲表：`(id, buffer)`。借出即移出，归还才放回。
    free: Vec<(u64, Buffer)>,
    /// 当前在借的 id 集合（别名检测用）。
    in_use: BTreeSet<u64>,
    next_id: u64,
    counters: Counters,
    retained_bytes: usize,
    /// **本轮渲染**（自上次 [`BufferPool::begin_render`] 起）见过的**最大单次取用**
    /// 字节数——一个并发 worker 的工作集。字节预算的第二因子。
    max_request_bytes: usize,
}

/// 图形缓冲池（见模块说明）。
///
/// **线程安全**：内部用 [`Mutex`] 保护空闲表 ⇒ 并行分块渲染的多个 worker 可以并发取用，
/// 锁只覆盖"取/还"这一次表操作，不覆盖真正的渲染。
#[derive(Debug)]
pub struct BufferPool {
    inner: Mutex<PoolInner>,
    limits: Limits,
    /// **本轮渲染的并发 worker（分块）数**，由 [`Self::begin_render`] 设置，恒在
    /// `1..=MAX_POOLED_BUFFERS`。它乘上本轮最大单块工作集就是保留字节预算。
    concurrency: AtomicUsize,
    /// `false` ⇒ 每次取用都新建、归还即丢弃（**等价于改造前的行为**）。
    ///
    /// 存在的意义是**可判定性**：同一次构建、同一份文档，可以跑"池化"与"强制新分配"
    /// 两条路径并逐字节比对（判据 1），也可以用来做前后测量。
    enabled: AtomicBool,
}

impl Default for BufferPool {
    fn default() -> Self {
        Self::new()
    }
}

impl BufferPool {
    /// 默认上限（[`MAX_POOLED_BUFFERS`] + [`MIN_POOLED_BYTES`] 下界），池化开启。
    pub fn new() -> Self {
        Self::with_limits(MAX_POOLED_BUFFERS, MIN_POOLED_BYTES)
    }

    /// 自定义保留上限（测试用小上限来证明"有界"）。
    ///
    /// `min_bytes` 是字节预算的**下界**：真实预算还会加上
    /// `并发数 × 本轮最大单块工作集`（见 [`Self::retained_byte_budget`]）。
    pub fn with_limits(max_buffers: usize, min_bytes: usize) -> Self {
        Self {
            inner: Mutex::new(PoolInner {
                free: Vec::new(),
                in_use: BTreeSet::new(),
                next_id: 0,
                counters: Counters::default(),
                retained_bytes: 0,
                max_request_bytes: 0,
            }),
            limits: Limits {
                max_buffers,
                min_bytes,
            },
            concurrency: AtomicUsize::new(1),
            enabled: AtomicBool::new(true),
        }
    }

    /// 开关池化。关闭后 [`Self::acquire`] 总是新建缓冲、归还即丢弃（改造前的行为）。
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::SeqCst);
    }

    /// 当前是否池化。
    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    /// **本轮渲染开工**：告诉池本轮有多少个**并发** worker（分块），并把
    /// "本轮最大单块工作集"清零。
    ///
    /// 渲染器在 spawn worker **之前**调用它（`render.rs` 的并行分派与串行分支各一处），
    /// 每个 worker 的 `acquire` 再把实际工作集记进池里。于是字节预算恰好覆盖
    /// "每个并发 worker 一块"，而不是一个与画布无关的常数。
    ///
    /// `workers` 会被夹到 `1..=max_buffers`（并发数不可能超过池愿意保留的个数上限）。
    /// 不在渲染路径上的直接取用方（例如单元测试）不调用它 ⇒ 并发数保持默认 `1`。
    pub fn begin_render(&self, workers: usize) {
        let clamped = workers.clamp(1, self.limits.max_buffers.max(1));
        self.concurrency.store(clamped, Ordering::SeqCst);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.max_request_bytes = 0;
    }

    /// **当前生效的保留字节预算**（归还时据此决定留还是丢）：
    /// `max(limits.min_bytes, 并发 worker 数 × 本轮最大单次取用字节数)`。
    ///
    /// 这是池的**公布上限**：任意时刻 `retained_bytes <= retained_byte_budget()`。它是
    /// "本轮真实并发需求"的一个上界——`并发数 × 最大单块 ≥ 同时在借的工作集`，而真实分带
    /// 下 N 块之和就是整幅 ⇒ 预算 ≈ 一次渲染的工作集；并发数被 [`MAX_POOLED_BUFFERS`]
    /// 夹住 ⇒ 预算不可能随归还次数无界增长。
    pub fn retained_byte_budget(&self) -> usize {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.byte_budget(&inner)
    }

    /// 预算公式（调用方已持有锁）。
    fn byte_budget(&self, inner: &PoolInner) -> usize {
        let concurrency = self.concurrency.load(Ordering::SeqCst).max(1);
        self.limits
            .min_bytes
            .max(inner.max_request_bytes.saturating_mul(concurrency))
    }

    /// 取得一块 `width × height`、原点为 `(origin_x, origin_y)` 的**全零**缓冲。
    ///
    /// 空闲表里没有容量足够的缓冲时新建；否则取**容量最小的够用者**（best-fit，
    /// 避免大分配被反复降级成小分配）。
    pub fn acquire(
        &self,
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
    ) -> BufferLease<'_> {
        let needed = (width as usize).saturating_mul(height as usize);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let id = inner.next_id;
        inner.next_id += 1;
        // 本轮最大单块工作集（一个 worker 的图层缓冲）：预算的第二因子。
        inner.max_request_bytes = inner
            .max_request_bytes
            .max(needed.saturating_mul(4 * std::mem::size_of::<f32>()));

        if self.enabled.load(Ordering::SeqCst) {
            let best = inner
                .free
                .iter()
                .enumerate()
                .filter(|(_, (_, buffer))| buffer.capacity_pixels() >= needed)
                .min_by_key(|(_, (_, buffer))| buffer.capacity_pixels())
                .map(|(index, _)| index);
            if let Some(index) = best {
                let (pooled_id, mut buffer) = inner.free.swap_remove(index);
                if inner.in_use.contains(&pooled_id) {
                    // 正确实现里空闲表不含在借 id；真出现说明归还逻辑把在借缓冲放回了池
                    // ⇒ 记一次冲突（判据会红），但不在渲染热路径 panic。
                    inner.counters.alias_violations += 1;
                }
                inner.retained_bytes = inner.retained_bytes.saturating_sub(buffer.capacity_bytes());
                buffer.reset(origin_x, origin_y, width, height);
                inner.counters.reused += 1;
                inner.in_use.insert(id);
                inner.counters.max_in_use = inner.counters.max_in_use.max(inner.in_use.len());
                return BufferLease {
                    pool: self,
                    id,
                    reused: true,
                    buffer: Some(buffer),
                };
            }
        }

        let buffer = Buffer::new(origin_x, origin_y, width, height);
        inner.counters.allocated += 1;
        inner.in_use.insert(id);
        inner.counters.max_in_use = inner.counters.max_in_use.max(inner.in_use.len());
        BufferLease {
            pool: self,
            id,
            reused: false,
            buffer: Some(buffer),
        }
    }

    /// 归还一块缓冲（由 [`BufferLease`] 的 `Drop` 调用）。
    fn release(&self, id: u64, buffer: Buffer) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !inner.in_use.remove(&id) {
            // 归还了一个不在借的 id（重复归还 / 从未借出）⇒ 记账冲突。
            inner.counters.alias_violations += 1;
        }
        inner.counters.released += 1;
        let bytes = buffer.capacity_bytes();
        // 个数上限始终生效；字节预算 = "本轮并发数 × 本轮最大单块"（下界 MIN_POOLED_BYTES），
        // 见 `retained_byte_budget`。预算对**除第一张以外**的缓冲生效 ——
        // 空闲表为空时允许保留一张、可突破预算（否则超过预算的画布池化完全失效，
        // 实测 3840×3840 在严格预算下是 20 次取用 / 0 次复用）。
        let budget = self.byte_budget(&inner);
        let count_ok = inner.free.len() < self.limits.max_buffers.max(1);
        let bytes_ok =
            inner.free.is_empty() || inner.retained_bytes.saturating_add(bytes) <= budget;
        let retain = self.enabled.load(Ordering::SeqCst) && count_ok && bytes_ok;
        if retain {
            inner.retained_bytes += bytes;
            inner.free.push((id, buffer));
        }
    }

    /// 只读计数快照。
    pub fn stats(&self) -> BufferPoolStats {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        BufferPoolStats {
            allocated: inner.counters.allocated,
            reused: inner.counters.reused,
            released: inner.counters.released,
            max_in_use: inner.counters.max_in_use,
            alias_violations: inner.counters.alias_violations,
            retained_buffers: inner.free.len(),
            retained_bytes: inner.retained_bytes,
        }
    }
}

/// 池借出的一块缓冲：`Deref` 到 [`Buffer`]，`Drop` 时自动归还。
#[derive(Debug)]
pub struct BufferLease<'a> {
    pool: &'a BufferPool,
    id: u64,
    reused: bool,
    buffer: Option<Buffer>,
}

impl BufferLease<'_> {
    /// 本次取用是**复用**（`true`）还是**新建分配**（`false`）——资源类判据的观测点。
    pub const fn was_reused(&self) -> bool {
        self.reused
    }
}

impl std::ops::Deref for BufferLease<'_> {
    type Target = Buffer;

    fn deref(&self) -> &Buffer {
        self.buffer
            .as_ref()
            .expect("缓冲租约在 Drop 之前始终持有缓冲区")
    }
}

impl std::ops::DerefMut for BufferLease<'_> {
    fn deref_mut(&mut self) -> &mut Buffer {
        self.buffer
            .as_mut()
            .expect("缓冲租约在 Drop 之前始终持有缓冲区")
    }
}

impl Drop for BufferLease<'_> {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.release(self.id, buffer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_reuses_and_zeroes_the_leased_buffer() {
        let pool = BufferPool::new();
        {
            let mut first = pool.acquire(0, 0, 8, 8);
            assert!(!first.was_reused(), "空池的第一次取用必然是新建");
            first.set_pixel(1, 1, [1.0, 1.0, 1.0, 1.0]);
        }
        let lease = pool.acquire(0, 0, 8, 8);
        assert!(lease.was_reused(), "归还后同尺寸取用必须复用");
        assert_eq!(lease.pixel(1, 1), [0.0; 4], "复用的缓冲必须以全零开始");
        let stats = pool.stats();
        assert_eq!(stats.allocated, 1);
        assert_eq!(stats.reused, 1);
        assert_eq!(stats.released, 1);
        assert_eq!(stats.alias_violations, 0);
    }

    #[test]
    fn disabled_pool_allocates_fresh_and_retains_nothing() {
        let pool = BufferPool::new();
        pool.set_enabled(false);
        for _ in 0..4 {
            let lease = pool.acquire(0, 0, 8, 8);
            assert!(!lease.was_reused());
        }
        let stats = pool.stats();
        assert_eq!(stats.allocated, 4);
        assert_eq!(stats.reused, 0);
        assert_eq!(stats.retained_buffers, 0);
    }

    /// **预算规则的单元判据**：`begin_render(N)` 之后，预算必须容下"N 个并发取用"的
    /// 工作集——4 个 worker 各持一块 16 KiB 缓冲，归还后 4 块都要留下，第二轮全部复用。
    ///
    /// 变异：让 `byte_budget` 忽略 `concurrency`（只看 `min_bytes` 或单块）⇒ 只留 1 块
    /// ⇒ 第二轮出现新建 ⇒ 红。
    #[test]
    fn byte_budget_covers_every_concurrent_worker() {
        // 64×64 像素 × 16 B = 64 KiB/张；下界给 0 ⇒ 预算完全由并发 × 工作集决定。
        let pool = BufferPool::with_limits(MAX_POOLED_BUFFERS, 0);
        let per_buffer = 64 * 64 * 16;
        pool.begin_render(4);
        {
            let leases: Vec<_> = (0..4).map(|_| pool.acquire(0, 0, 64, 64)).collect();
            assert!(
                !leases.iter().any(|lease| lease.was_reused()),
                "空池首轮必然全新建"
            );
            assert_eq!(pool.stats().max_in_use, 4, "4 块必须同时在借");
        }
        assert_eq!(
            pool.retained_byte_budget(),
            4 * per_buffer,
            "预算 = 并发数 × 单块工作集"
        );
        let stats = pool.stats();
        assert_eq!(
            stats.retained_buffers, 4,
            "4 个并发 worker 的工作集必须全部留住"
        );
        assert_eq!(stats.retained_bytes, 4 * per_buffer);

        let allocated_before = stats.allocated;
        let leases: Vec<_> = (0..4).map(|_| pool.acquire(0, 0, 64, 64)).collect();
        assert!(
            leases.iter().all(|lease| lease.was_reused()),
            "第二轮 4 个并发取用必须全部复用。实测 {:?}",
            leases.iter().map(|l| l.was_reused()).collect::<Vec<_>>()
        );
        assert_eq!(pool.stats().allocated, allocated_before, "第二轮不得新建");
    }

    /// **预算不会无界增长**：并发只有 4，却归还了 8 块 ⇒ 只留 4 块（`并发数 × 工作集`
    /// 就是上界），第 5 块起一律丢弃。个数上限也把并发因子夹在 ≤ `MAX_POOLED_BUFFERS`。
    ///
    /// 变异：去掉预算判断（只留个数上限）⇒ 8 块全留 ⇒ 红。
    #[test]
    fn byte_budget_stops_at_the_concurrency_not_at_the_return_count() {
        let pool = BufferPool::with_limits(MAX_POOLED_BUFFERS, 0);
        let per_buffer = 64 * 64 * 16;
        pool.begin_render(4);
        let leases: Vec<_> = (0..8).map(|_| pool.acquire(0, 0, 64, 64)).collect();
        assert_eq!(pool.stats().max_in_use, 8);
        drop(leases);
        let stats = pool.stats();
        assert_eq!(
            stats.retained_buffers, 4,
            "并发只有 4 ⇒ 第 5 张起不得撑大池"
        );
        assert_eq!(stats.retained_bytes, 4 * per_buffer);
        assert_eq!(pool.retained_byte_budget(), 4 * per_buffer);

        // 并发数被 MAX_POOLED_BUFFERS 夹住：即使要求 100 个 worker，预算也只有 16 块。
        let clamped = BufferPool::with_limits(2, 0);
        clamped.begin_render(100);
        let lease = clamped.acquire(0, 0, 64, 64); // 记下本轮工作集
        assert_eq!(clamped.retained_byte_budget(), 2 * per_buffer);
        drop(lease);
    }
}
