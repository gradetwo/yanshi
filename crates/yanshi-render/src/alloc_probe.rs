//! **★ 渲染分配的探针（DEF-03 / 内存账目的**前置条件**）★**（第 254 轮）
//!
//! **为什么要它**：这一题我**连续错了三次**（"每层一个全幅缓冲" ✗、"无上限 tile 缓存" ✗、
//! 早先"import_image 1.7 GB" ✗）—— 三次都是**拿一个数字当结论**。`/health` 的聚合计数
//! （`rss_bytes`、`cache_stats`）**回答不了**"这一帧的字节是**谁**分配的、是**新分配**还是**复用**"
//! => 所以先加这个探针，让账目从推断变成实测。
//!
//! **门控**：只在 `YANSHI_ALLOC_PROBE` 开着时累加（环境变量**每进程读一次**，`OnceLock`）
//! => **生产路径零开销** ✓（与 `brush.rs` 里 `YANSHI_RENDER_PROBE` 同一套规矩）。
//!
//! **口径**：只记**字节数**与**次数**，按**固定的几个分配点**分类（定长数组 ＋ 原子 => 无锁、无分配，
//! 不会因为"记录分配"而自己产生分配 ✓）。**不记调用栈、不记地址**（那会引入锁与分配 ✓）。

use std::sync::atomic::{AtomicU64, Ordering};

/// 分配点（**定长** => 用数组索引，避免任何运行期分配）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocPoint {
    /// `BufferPool::acquire` **命中已有块**（复用 => 不产生新分配）。
    PoolReuse,
    /// `BufferPool::acquire` **未命中 => 真的新分配**。
    PoolFresh,
    /// `Buffer::new`（直接用 `vec![0.0; …]`，绕过池的那些路径）。
    BufferFresh,
    /// 归还给池（`release`）—— 用来对照"借/还"是否配对。
    PoolRelease,
    /// 其他显式登记的点（当前未用，留位）。
    Other,
}

const POINTS: usize = 5;

fn slot(point: AllocPoint) -> usize {
    match point {
        AllocPoint::PoolReuse => 0,
        AllocPoint::PoolFresh => 1,
        AllocPoint::BufferFresh => 2,
        AllocPoint::PoolRelease => 3,
        AllocPoint::Other => 4,
    }
}

static BYTES: [AtomicU64; POINTS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static COUNTS: [AtomicU64; POINTS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// 探针开着吗（**只读一次环境变量** => 生产路径上只是一次 `OnceLock` 读）。
#[must_use]
pub fn alloc_probe_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("YANSHI_ALLOC_PROBE").is_some())
}

/// 记一笔（探针关着时**立即返回**，只多一次 `OnceLock` 读）。
pub fn record(point: AllocPoint, bytes: usize) {
    if !alloc_probe_on() {
        return;
    }
    let index = slot(point);
    BYTES[index].fetch_add(bytes as u64, Ordering::Relaxed);
    COUNTS[index].fetch_add(1, Ordering::Relaxed);
}

/// 快照：`(名字, 字节, 次数)`，按枚举顺序（**顺序稳定** => 判据可直接对账）。
#[must_use]
pub fn snapshot() -> [(&'static str, u64, u64); POINTS] {
    [
        (
            "pool_reuse",
            BYTES[0].load(Ordering::Relaxed),
            COUNTS[0].load(Ordering::Relaxed),
        ),
        (
            "pool_fresh",
            BYTES[1].load(Ordering::Relaxed),
            COUNTS[1].load(Ordering::Relaxed),
        ),
        (
            "buffer_fresh",
            BYTES[2].load(Ordering::Relaxed),
            COUNTS[2].load(Ordering::Relaxed),
        ),
        (
            "pool_release",
            BYTES[3].load(Ordering::Relaxed),
            COUNTS[3].load(Ordering::Relaxed),
        ),
        (
            "other",
            BYTES[4].load(Ordering::Relaxed),
            COUNTS[4].load(Ordering::Relaxed),
        ),
    ]
}

/// 清零（判据/测量用；**不改变门控状态**）。
pub fn reset() {
    for index in 0..POINTS {
        BYTES[index].store(0, Ordering::Relaxed);
        COUNTS[index].store(0, Ordering::Relaxed);
    }
}

/// 一行可读摘要（给 `/health` 与判据用；**不用 serde** => 内核里不引入依赖）。
#[must_use]
pub fn summary() -> String {
    snapshot()
        .iter()
        .map(|(name, bytes, count)| format!("{name}: {count} 次 / {bytes} B"))
        .collect::<Vec<_>>()
        .join("｜")
}

#[cfg(test)]
mod tests {
    use super::{record, reset, snapshot, AllocPoint};

    /// **探针的本职**：记账要**分类准确**、**可以清零**、并且**关着的时候不许影响别的东西**。
    /// 注意：本测试**不设环境变量**（探针多半是关的）=> 它只校验"关着时是安全的空操作"，
    /// 以及"开了之后累加正确"（用 `YANSHI_ALLOC_PROBE` 跑一遍才算全验，见脚本）。
    #[test]
    fn probe_records_and_resets() {
        reset();
        let before = snapshot();
        record(AllocPoint::Other, 123);
        let after = snapshot();
        // 关着时：什么都不动（生产零开销的口径）
        if std::env::var_os("YANSHI_ALLOC_PROBE").is_none() {
            assert_eq!(before, after, "探针关着时不许记账");
        } else {
            let (_, bytes, count) = after[4];
            let (_, before_bytes, before_count) = before[4];
            assert_eq!(bytes, before_bytes + 123);
            assert_eq!(count, before_count + 1);
        }
        reset();
        let cleared = snapshot();
        assert!(cleared
            .iter()
            .all(|(_, bytes, count)| *bytes == 0 && *count == 0));
    }
}
