//! 历史折叠的**可复现**基准（默认 `#[ignore]`，不进 CI 时间预算）。
//!
//! 用法（拿真实工程日志当输入，例如 `/tmp/parrot1k/root/atoms.jsonl`）：
//!
//! ```text
//! YANSHI_PERF_LOG=/tmp/parrot1k/root/atoms.jsonl \
//!   cargo test -p yanshi-core --test history_fold_bench -- --ignored --nocapture
//! ```
//!
//! 它回答一个问题：**每次提交的历史折叠成本会不会随日志长度增长**。
//! 做法是从 seq 1 逐条前推整条日志，按深度分桶统计单次折叠耗时，并同时打印
//! `incremental_steps / rebase_steps / full_steps / replayed_atoms` 四个计数器 ——
//! 判据是**语义的**（计数器），耗时只是旁证（见 `src/seq.rs` 里的
//! `appending_reverts_never_falls_back_to_a_full_replay`）。

use std::time::{Duration, Instant};
use yanshi_core::seq::{state_at, IncrementalFolder, StateAtCache};
use yanshi_core::{Atom, AtomKind, AtomLog};

fn load(path: &str) -> Option<AtomLog> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut atoms: Vec<Atom> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        atoms.push(serde_json::from_str(line).expect("原子 JSON"));
    }
    Some(AtomLog::with_atoms(atoms).expect("日志"))
}

#[test]
#[ignore = "需要真实工程日志（YANSHI_PERF_LOG）与秒级以上的时间预算"]
fn per_commit_fold_cost_by_depth() {
    let Ok(path) = std::env::var("YANSHI_PERF_LOG") else {
        eprintln!("跳过：未设置 YANSHI_PERF_LOG");
        return;
    };
    let Some(log) = load(&path) else {
        eprintln!("跳过：读不到 {path}");
        return;
    };
    let head = log.head_seq();
    const BUCKET: u64 = 150;
    #[derive(Default, Clone, Copy)]
    struct Acc {
        plain: Duration,
        revert: Duration,
        plain_n: u32,
        revert_n: u32,
    }
    let mut buckets = vec![Acc::default(); ((head / BUCKET) + 1) as usize];
    let mut folder = IncrementalFolder::new();
    let started = Instant::now();
    for seq in 1..=head {
        let kind = log.by_seq(seq).expect("原子").kind;
        let t0 = Instant::now();
        let folded = folder.fold(&log, seq).expect("折叠");
        let cost = t0.elapsed();
        // 旁证：完整求值在同一步上的成本（增量折叠要打败的就是它）。
        let _ = folded;
        let acc = &mut buckets[((seq - 1) / BUCKET) as usize];
        if matches!(kind, AtomKind::Revert | AtomKind::Reapply) {
            acc.revert += cost;
            acc.revert_n += 1;
        } else {
            acc.plain += cost;
            acc.plain_n += 1;
        }
    }
    let total = started.elapsed();
    println!(
        "{path}: atoms={head} 逐条前推总耗时 {total:?}｜incremental={} rebase={} full={} replayed_atoms={}",
        folder.incremental_steps(),
        folder.rebase_steps(),
        folder.full_steps(),
        folder.replayed_atoms()
    );
    for (i, acc) in buckets.iter().enumerate() {
        if acc.plain_n + acc.revert_n == 0 {
            continue;
        }
        let low = i as u64 * BUCKET + 1;
        let high = ((i as u64 + 1) * BUCKET).min(head);
        println!(
            "  seq {low:>5}-{high:<5} | 普通提交 {:>10.3?} | 撤销提交 {:>10.3?}",
            acc.plain / acc.plain_n.max(1),
            acc.revert / acc.revert_n.max(1),
        );
    }
    // 与完整求值逐字段一致（基准也必须先是对的）。
    let incremental = folder.fold_head(&log).expect("收尾折叠");
    let full = state_at(&log, head, &mut StateAtCache::new()).expect("完整求值");
    assert!(incremental == full, "增量折叠必须与完整求值逐字段一致");
}
