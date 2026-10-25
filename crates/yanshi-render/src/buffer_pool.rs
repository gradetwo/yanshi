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
//! ① 并发数被 [`MAX_POOLED_BUFFERS`] 夹住（现为 4）；② 真实分带满足"N 块之和 = 整幅"，
//! 所以预算 ≈ 一次渲染的工作集；③ 个数上限始终生效。池最多留下"每个并发 worker 一块、
//! 总数 ≤ [`MAX_POOLED_BUFFERS`]"，绝不会因为归还次数多而膨胀。
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
/// **历史**：原先取 16 是为了覆盖并行渲染的 worker 上限（`rows.rs::MAX_WORKERS = 16`）——
/// 每个 worker 的图层循环同时最多只需要一块图层缓冲。真正的规模仍由**实际并发取用**决定，
/// 这里只是「最多保留多少」的上限。
/// **★ 为什么现在是 4（而不是更小）✗★**（2026-10-15 ✓；§49 之补一百一十五／一百一十七 ✓）：
/// **不变量**是「**缓存上限 ≥ 并发数**」✓ —— 单元判据
/// `byte_budget_covers_every_concurrent_worker` 用 **4 个并发 worker** 把它钉住了 ✓：
/// 上限小于 4 ⇒ 并发下**每轮重新分配** ✗（抖动 ✗）⇒ 判据**必红** ✓。
/// ⇒ ⇒ 所以 4 是"**在并发不变量之内能取的最小值**"✓（16 ⇒ 4 已省 12 块 ✓）。
pub const MAX_POOLED_BUFFERS: usize = 4;

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

/// **★ 单个缓冲的保留上限 —— 按**尺寸**分级（2026-10-15 加 ✓）★**
///
/// **为什么** ✗：4K 下一个**整幅** f32 工作缓冲就是 **268 MB** ✓。
/// 实测（4K ＋ 4 层整幅渲染 ✓）：RSS 增量 **872 MB** ✓ ——
/// 而分配探针里**没有一个桶是 268 MB** ✗ ⇒ 那 872 MB 不是"此刻在用"的缓冲 ✓，
/// 而是**池为了复用留住**的几张（重复渲染实测 Δ0 ✓，§36／§41 ✓）。
///
/// **为什么不用"一刀切"** ✗：给保留字节加**绝对上限**是第 254 轮**试过、已撤**的 ✓
/// （`MAX_RETAINED_BYTES` ⇒ **所有尺寸**的复用一起坏 ⇒ **11× CPU 回归** ✗）。
/// 本常数**只挡大块** ✓：小缓冲照旧保留 ✓ ⇒ 常见尺寸的复用与 Δ0 **不受影响** ✓。
///
/// **取 64 MiB 的理由** ✓：2K 整幅 ＝ 4096×... 见下 —— 2048² × 4ch × 4B ≈ **67 MB** ✓，
/// 略高于本值 ⇒ **2K 及以下的整幅缓冲仍被保留** ✓（常见画布尺寸 ✓），
/// 而 **4K／8K 的整幅（268 MB／1 GB）与半幅都不留** ✗ ⇒ 峰值可降约 268 MB／张 ✓。
/// **★ 已回退、暂未启用（2026-10-15 ✓）★**
///
/// 试过：单个缓冲超过它就**不保留** ✓ ⇒ 4K 峰值 872 → 621 MB ✓、预算判据转绿 ✓；
/// **但 CI 的 4K 一笔延迟判据变红** ✗（4K 下每次重新分配 268 MB ✗）。
/// **硬约束优先** ⇒ **回退** ✓；保留此常量是为了让下一版**不必重新推导**这个数 ✓，
/// 但**启用它之前必须**：① 设计一个**不牺牲复用**的峰值削减法 ✓；
/// ② 判据里**加上 `tool-stroke-latency-4k.mjs`** ✓（本轮漏了它 ✗ ⇒ 代价没在本机暴露 ✓）。
pub const MAX_RETAINED_PER_BUFFER_BYTES: usize = 64 * 1024 * 1024;

/// **★ 大缓冲最多保留几张（RSS 修复 paint-rss，2026-10-26）★**
///
/// 2026-10-15 那版的教训是"**一张都不留** ⇒ 每笔重分配 ⇒ 延迟变红"；本规则是
/// 同一常数的另一半用法——**只留一张**：`release` 里容量超过
/// [`MAX_RETAINED_PER_BUFFER_BYTES`] 的缓冲，空闲表里已有同级大缓冲时就丢弃
/// （小缓冲仍走原有字节预算）。一笔的复用只需要**一张**全幅／带缓冲（串行全幅路
/// 与并行带路各自同时在用的也就一两张），第 2 张起只是跨渲染、跨文档的滞留：
/// 4K 全配方文档实测每文档滞留 ~260MB（RSS 定位报告 §1），池里多张 66–133MB
/// 缓冲是其中最大头。验收必须同时跑 `tool-stroke-latency-4k.mjs`（只看内存
/// 数会重蹈 2026-10-15 的覆辙）。
pub const MAX_RETAINED_LARGE_BUFFERS: usize = 1;

// **★ 保留字节的绝对上限 —— **试过、已撤**（第 254 轮 A/B 实测）★**
//
// **当时为什么加**：预算公式只有下界 ＋ 并发 × 单块 => 全幅渲染时
// `begin_render_with(chunks, 2)` 会把预算算到 ~2.1 GB（4K 单块 268 MB × chunks × 2），
// 担心大画布把成百 MB–GB 级缓冲长期扣住。于是加了 `MAX_RETAINED_BYTES = 384 MiB`。
//
// **★ A/B 实测（同机、同一份 4K＋4 层真实素材场景、中位数、热身剔除）★**：
//   * **有上限**：墙钟 **519.9 ms**｜进程 CPU **510.0 ms**｜预热 **41.0 s**｜峰值 RSS **1567 MB**
//   * **无上限**：墙钟 ** 47.0 ms**｜进程 CPU ** 40.0 ms**｜预热 ** 3.4 s**｜峰值 RSS **1573 MB**
//   => **∴ 11 倍 CPU 回归 ✗ 而内存**一点没省**（1573 vs 1567 MB，属噪声 ✓）★★★
//   => **∴ 结论 ✗**：**上限会让**归还的块被丢弃**✗ 于是每层都要重新分配 268 MB 的 f32 缓冲**
//      （4K × 4 层 ⇒ 十几次 malloc/free 与 page fault ✓）
//      ⇒ **∴ 它换来的是**保留量的有界性**✗ 代价却是渲染路径的 11 倍 ✗ 且**对那笔
//        "同时在用"的内存账毫无帮助** ✓ ⇒ **∴ 撤掉 ✓** ★★★
//
// **★ 教训（写在这里，别再犯）★**：
//   ＋ 判断"某个上限无害"不能只靠推理（"它只管保留、不该影响在用"）—— **必须两本账量** ✓。
//     我上一轮量到的是"**没变省**"（909 ⇒ 895 MB ✓），当时**没有**同时量墙钟/CPU 的 A/B，
//     于是把一个 11 倍回归留在了 main 上 ✓。
//   ＋ 真正的解在别处（见 `docs/design/render-memory-account.md` 的旋钮 1/2/3：
//     按内存预算夹 `chunks`、工作缓冲 f32⇒f16、真正的分带合成 ✓）。

// 池的保留上限（个数 + 字节预算的下界）。
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
    /// 当前空闲表里保留的**大**缓冲个数（容量超过
    /// [`MAX_RETAINED_PER_BUFFER_BYTES`] 的张数；RSS 修复 paint-rss
    /// 的观测口——正常应 ≤ [`MAX_RETAINED_LARGE_BUFFERS`]）。
    pub retained_large_buffers: usize,
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
    /// **★ 每个并发 worker 同时在借几块 ✗ ★**（第 22 轮 ✓；**修预算少算的因子 ✓）。
    ///
    /// **∴ 为什么必须有它 ✗**：**实测**（§21 ✓）**4 带 4 层**时**池**反复丢缓冲 ✗
    ///   ⇒ **∴ 页错 +31%／VmHWM +36%／CPU +83%** ✗，**而**墙钟**反而比 2 带更慢 ✓**
    ///     ⇒ **∴ 根因 ✗**：**预算 ＝ 并发数 × **一块**带缓冲**✗
    ///       **而**实际工作集 ＝ 并发数 × **一块**带缓冲 × **每块的缓冲数** ✓**** ✓✓
    ///         ⇒ **★ 所以**：**少算的那一维**必须**显式告诉池 ✓ ★**** ✓✓
    /// **∴ 默认 1 ✗**：**∴ 不调用者**（**如**单元测试 ✓）**行为**完全不变 ✓**** ✓✓
    buffers_per_worker: AtomicUsize,
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
    /// **★ 运行时旋钮：池的"最多缓存几块"（2026-10-15 ✓）★**
    ///
    /// **为什么加它** ✗（目标书 乙(i) 的原话 ✓）："**先加可关闭开关**（默认保持现状 ⇒
    /// **零行为变化** ✓），再同机 A/B ✓，**数据支持才默认打开**"。
    /// **⇒ 默认就是 [`MAX_POOLED_BUFFERS`]（现为 4 ✓）** ✓ —— 依据是下面那张 A/B 表 ✓
    /// **外加并发不变量** ✓（§49 之补一百一十五／一百一十七 ✓）。
    ///
    /// **实测（同机 A/B ✓，只改这一个数 ✓）** ✓：
    /// | 项 | 16（旧默认 ✗） | 2 | **4（现默认 ✓）** |
    /// |---|---|---|---|
    /// | 4K＋4 层**内存增量** | **886 MB** ✗（超预算 800 ✓） | 610 MB ✓ | **634 MB** ✓（≤ 800 ✓）|
    /// | 4K **一笔墙钟**中位 | 27.5 ms ✓ | 25.8 ms ✓ | **22.0 ms** ✓ |
    /// | 并发不变量（上限 ≥ 并发数 ✓） | ✓ | **✗ 单元判据必红** | **✓** |
    /// ⇒ ⇒ **2 省得最多却破坏不变量** ✗（4 个并发 worker 下每轮重新分配 ✓）
    /// ⇒ **∴ 4 是"不变量之内能取的最小值"** ✓：比 16 省 **252 MB** 且**不慢笔** ✓
    /// （§49 之补一百一十七 ✓）。
    ///
    /// **只想临时试的人** ✓：`YANSHI_MAX_POOLED_BUFFERS=8 yanshi-serve …` ✓
    /// （**别往下试** ✗：小于并发数会破坏不变量 ✓ ⇒ 单元判据必红 ✓）
    /// （照 `YANSHI_ALLOC_PROBE` ✓／`YANSHI_BELOW_SIG_GROUPS` ✓ 的先例 ✓；
    /// 环境变量**每进程只读一次** ✓ ⇒ 热路径上只是一次 `OnceLock` 读 ✓）。
    fn max_pooled_buffers() -> usize {
        static ON: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
        *ON.get_or_init(|| {
            std::env::var("YANSHI_MAX_POOLED_BUFFERS")
                .ok()
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(MAX_POOLED_BUFFERS)
        })
    }

    /// 默认上限（[`MAX_POOLED_BUFFERS`] + [`MIN_POOLED_BYTES`] 下界），池化开启。
    pub fn new() -> Self {
        Self::with_limits(Self::max_pooled_buffers(), MIN_POOLED_BYTES)
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
            buffers_per_worker: AtomicUsize::new(1),
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
        // **∴ 旧签名保留 ✗**：**默认"**每并发一块 ✓"**✗ ⇒ **∴ 既有的调用方／判据**零改动 ✓**** ✓✓
        self.begin_render_with(workers, 1);
    }

    /// **★ 显式告诉池：**每并发同时在借几块** ✗ ★**（第 22 轮 ✓）。
    ///
    /// **∴ 取值 ✗**：**渲染时**每条带**至少同时持有**「**累积缓冲 ✓」与
    ///   「**当前层缓冲 ✓」两块 ⇒ **∴ 调用方**传 **2** ✓**** ✓✓
    pub fn begin_render_with(&self, workers: usize, buffers_per_worker: usize) {
        let clamped = workers.clamp(1, self.limits.max_buffers.max(1));
        self.concurrency.store(clamped, Ordering::SeqCst);
        self.buffers_per_worker
            .store(buffers_per_worker.max(1), Ordering::SeqCst);
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
        // **★ 少算的那一维 ✗ ★**（第 22 轮 ✓）：**每并发**同时在借几块** ✓
        let per_worker = self.buffers_per_worker.load(Ordering::SeqCst).max(1);
        // **★ 预算公式 ✗ ★**（第 22 轮 ✓）：**下界（`min_bytes`）与"并发 × 每并发块数 ×
        //   本轮最大请求"取大 ✓ ⇒ **够用**就保留 ✓
        //   ＋ **∴ 曾经在这里再用一个绝对上限夹"别囤" ✗ 已经**撤掉**（第 254 轮 A/B ✓）**：
        //     实测**墙钟 47.0 ⇒ 519.9 ms（11 倍）✗ 而 RSS 1573 ⇒ 1567 MB（等于没省）**
        //     ⇒ **∴ 原因 ✗**：**上限让**归还的块被丢弃**✗ 每层都要重新分配 268 MB 的 f32 缓冲**
        //     ⇒ **∴ 有条界性诉求的话 ✗ 请走"按内存预算夹 `chunks`"那条路（见
        //       `docs/design/render-memory-account.md` 旋钮 1 ✓），**不要再夹保留量** ✗ ★★★
        self.limits.min_bytes.max(
            inner
                .max_request_bytes
                .saturating_mul(concurrency)
                .saturating_mul(per_worker),
        )
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
        self.acquire_stepped(origin_x, origin_y, width, height, 1)
    }

    /// 与 [`BufferPool::acquire`] 相同，但取样步长为 `step`（期 4 `lod` 用）：
    /// 复用块的容量按**像素数**算（`width × height` 已是 LOD 像素数），`reset` 时
    /// 把步长写进缓冲，池化复用不会把 `step == 1` 的旧缓冲错进 LOD 渲染。
    pub fn acquire_stepped(
        &self,
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        step: u32,
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
                buffer.reset(origin_x, origin_y, width, height, step);
                inner.counters.reused += 1;
                inner.in_use.insert(id);
                inner.counters.max_in_use = inner.counters.max_in_use.max(inner.in_use.len());
                // **分配探针**（DEF-03 的前置条件）：命中已有块 => 复用，不产生新分配。
                crate::alloc_probe::record(
                    crate::alloc_probe::AllocPoint::PoolReuse,
                    buffer.capacity_bytes(),
                );
                return BufferLease {
                    pool: self,
                    id,
                    reused: true,
                    buffer: Some(buffer),
                };
            }
        }

        let buffer = Buffer::new_stepped(origin_x, origin_y, width, height, step);
        // **分配探针**：池里没有合适的块 => 这里才是**真的新分配**。
        crate::alloc_probe::record(
            crate::alloc_probe::AllocPoint::PoolFresh,
            buffer.capacity_bytes(),
        );
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
        // **分配探针**：归还（用来对照"借/还"是否配对）。
        crate::alloc_probe::record(crate::alloc_probe::AllocPoint::PoolRelease, bytes);
        // 个数上限始终生效；字节预算 = "本轮并发数 × 本轮最大单块"（下界 MIN_POOLED_BYTES），
        // 见 `retained_byte_budget`。预算对**除第一张以外**的缓冲生效 ——
        // 空闲表为空时允许保留一张、可突破预算（否则超过预算的画布池化完全失效，
        // 实测 3840×3840 在严格预算下是 20 次取用 / 0 次复用）。
        let budget = self.byte_budget(&inner);
        let count_ok = inner.free.len() < self.limits.max_buffers.max(1);
        let bytes_ok =
            inner.free.is_empty() || inner.retained_bytes.saturating_add(bytes) <= budget;
        // **★★ 2026-10-15：这里**试过"按尺寸分级"（超过 64 MiB 就不保留）⇒ **已回退** ✗ ★★**
        //   **账目** ✓：4K 内存预算判据确实从 **872 MB 降到 621 MB** ✓、判据转绿 ✓；
        //   但 **CI 的 `tool-stroke-latency-4k.mjs` 变红** ✗（`88a748ff` 那次是 11/11 全绿 ✓
        //   ⇒ 那是**我的改动引入的** ✓）—— 因为 4K 下图层缓冲不再保留 ⇒
        //   **每次都要重新分配 268 MB** ✓ ⇒ **CPU／延迟**吃到这笔账 ✓。
        //   **本机测不出** ✗：本机同一判据是 **中位 23.9 ms／预算 100 ms**（4 倍余量 ✓）
        //   ⇒ 而 CI runner 更慢 ⇒ 在那边就超了 ✓ ⇒ **只有 CI 能看见这笔代价** ✓。
        //   **判决** ✓：**硬约束优先** ✗ —— 「4K 一笔 ≤ ~100 ms」是**用户硬要求** ✓，
        //   而内存预算只是一条**可登记的**判据 ✓ ⇒ ⇒ **回退**这项改动 ✓，
        //   把 `tool-render-memory-budget.mjs` 退回**已知红**（诚实 ✓）。
        //   **下一版要做的事** ✓：找一个**不牺牲复用**的峰值削减法 ——
        //   例如保留**一张**整幅（沿用本文件既有的"至少保留一张"例外 ✓）、
        //   或**边合成边释放**（把峰值降在**持有**那一侧，而不是靠"不保留" ✗）✓；
        //   并且**验证必须包含 `tool-stroke-latency-4k.mjs`** ✓（本轮漏了它 ✗）。
        //   **★ 2026-10-26（RSS 修复 paint-rss）已落地"保留一张"这半 ✓ ★**：见下面的
        //   `large_ok`——超过 64 MiB 的缓冲最多保留一张（`MAX_RETAINED_LARGE_BUFFERS`），
        //   第 2 张起丢弃；"一张都不留"那半仍是禁区（延迟硬约束 ✓）。小缓冲不受此限。
        let large_ok = bytes <= MAX_RETAINED_PER_BUFFER_BYTES
            || inner
                .free
                .iter()
                .filter(|(_, buffer)| {
                    buffer.capacity_bytes() > MAX_RETAINED_PER_BUFFER_BYTES
                })
                .count()
                < MAX_RETAINED_LARGE_BUFFERS;
        let retain = self.enabled.load(Ordering::SeqCst) && count_ok && bytes_ok && large_ok;
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
            retained_large_buffers: inner
                .free
                .iter()
                .filter(|(_, buffer)| {
                    buffer.capacity_bytes() > MAX_RETAINED_PER_BUFFER_BYTES
                })
                .count(),
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

    /// **RSS 修复 paint-rss：大缓冲最多保留一张，但留下的那张必须复用**。
    ///
    /// 2026-10-15 的教训是两半："一张都不留" ⇒ 每笔重分配 268 MB ⇒
    /// `tool-stroke-latency-4k` 变红；"留多张" ⇒ 每文档滞留数百 MB。
    /// 本判据钉住中间态：3 次归还 3 张 64 MiB 以上的缓冲 ⇒ 只留 1 张；
    /// 随后同尺寸取用必须命中留下的那张（`was_reused`），否则延迟不变量已丢。
    ///
    /// 变异：把 `MAX_RETAINED_LARGE_BUFFERS` 改回 4 ⇒ `retained_large_buffers`
    /// 变 3 ⇒ 红；把大缓冲判定改成"一张都不留" ⇒ 第二轮 `was_reused` 为假 ⇒ 红。
    #[test]
    fn large_buffers_retain_at_most_one_but_still_reuse() {
        // 字节预算给满（只留个数上限与大缓冲规则做主），避免预算先挡住。
        let pool = BufferPool::with_limits(MAX_POOLED_BUFFERS, usize::MAX);
        // 2050×2048×16 B ≈ 64.06 MiB：刚过 64 MiB 的"大"线；2048×2048 恰为 64 MiB，不算大。
        let large_w = 2050u32;
        let large_h = 2048u32;
        let large_bytes = (large_w as usize) * (large_h as usize) * 16;
        assert!(
            large_bytes > MAX_RETAINED_PER_BUFFER_BYTES,
            "测试前设：{large_bytes} 必须超过大缓冲线"
        );
        // 三张必须**同时在借**再一起归还：逐张借还会在第二张就命中复用，
        // 测不到"归还第 2 张大缓冲时丢弃"这条规则。
        let leases: Vec<_> = (0..3)
            .map(|_| pool.acquire(0, 0, large_w, large_h))
            .collect();
        assert_eq!(pool.stats().allocated, 3, "三张同时在借应全是新建");
        drop(leases);
        let stats = pool.stats();
        assert_eq!(
            stats.retained_large_buffers, MAX_RETAINED_LARGE_BUFFERS,
            "3 张大缓冲归还后只能留 {MAX_RETAINED_LARGE_BUFFERS} 张：{stats:?}"
        );
        assert_eq!(
            stats.retained_buffers, MAX_RETAINED_LARGE_BUFFERS,
            "本轮只有大缓冲归还，留下的张数应与大缓冲张数一致：{stats:?}"
        );
        assert_eq!(stats.retained_bytes, large_bytes);

        // 留下的那张必须能复用：下一轮同尺寸取用不得新建（否则就是"一张都不留"的回退）。
        let allocated_before = pool.stats().allocated;
        let lease = pool.acquire(0, 0, large_w, large_h);
        assert!(
            lease.was_reused(),
            "保留的那张大缓冲必须被复用，否则 4K 一笔要重分配 {large_bytes} 字节"
        );
        assert_eq!(pool.stats().allocated, allocated_before, "复用轮不得新建");
        drop(lease);

        // 边界：恰为 64 MiB 的缓冲不算"大"，两张都应留下（小缓冲走原有预算）。
        let small_pool = BufferPool::with_limits(MAX_POOLED_BUFFERS, usize::MAX);
        let edge_bytes = 2048usize * 2048 * 16;
        assert_eq!(edge_bytes, MAX_RETAINED_PER_BUFFER_BYTES);
        for _ in 0..2 {
            drop(small_pool.acquire(0, 0, 2048, 2048));
        }
        let edge_stats = small_pool.stats();
        assert_eq!(
            edge_stats.retained_large_buffers, 0,
            "恰为 64 MiB 不算大：{edge_stats:?}"
        );
        assert_eq!(edge_stats.retained_buffers, 2, "两张 64 MiB 都应留下");
    }
}
