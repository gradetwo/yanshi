//! 折叠代数不变量的属性测试（设计文档 5.3）。
//!
//! 五条不变量：
//!
//! 1. **幂等性**：`fold(atoms) == fold(atoms)`
//! 2. **收敛性**：不同起点到达相同 head 的结果一致
//! 3. **无孤儿引用**：折叠结果中不存在指向已失效原子的引用
//! 4. **revert-reapply 往返**：`revert(x)` 后 `reapply(x)` 恢复 `x` 的效果（不恢复级联链）
//! 5. **历史可重放**：任意 `state@seq_n` 在日志与 blob 完整时均可重放，GC 不破坏此性质
//!
//! 场景由 [`yanshi_core::testkit`] 生成：全部原子走真实的提交校验路径，
//! 被拒绝的原子不进日志。

use proptest::prelude::*;
use std::collections::BTreeSet;
use yanshi_core::blob::BlobStore;
use yanshi_core::blob::{plan_gc, run_gc, DEFAULT_ORPHAN_TTL_SECONDS};
use yanshi_core::fold::{compute_suppressed, fold_atoms};
use yanshi_core::ids::now_ms;
use yanshi_core::seq::state_at;
use yanshi_core::testkit::{generate, Scenario, ScenarioConfig};
use yanshi_core::{
    Atom, AtomKind, AtomLog, FoldEngine, IncrementalFolder, StateAtCache, CAS_THRESHOLD_BYTES,
};

fn scenario(seed: u64, steps: usize, blob_bytes: usize) -> Scenario {
    generate(&ScenarioConfig {
        seed,
        steps,
        blob_bytes,
        snapshot_window: 0,
        revert_permille: 150,
        sampling_permille: 30,
        max_entities: 8,
    })
}

/// 取 `n`，并夹到 `[0, head]`。
fn clamp_seq(n: usize, log: &AtomLog) -> u64 {
    (n as u64).min(log.head_seq())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, max_shrink_iters: 64, ..ProptestConfig::default() })]

    /// 不变量 1：幂等性。
    #[test]
    fn fold_is_idempotent(seed in any::<u64>(), steps in 4usize..48) {
        let scenario = scenario(seed, steps, 0);
        let first = FoldEngine::new().fold(&scenario.log).unwrap();
        let second = FoldEngine::new().fold(&scenario.log).unwrap();
        prop_assert_eq!(first.state.clone(), second.state);
        prop_assert!(first.state.is_consistent(), "违规: {:?}", first.state.violations());
    }

    /// 不变量 3：无孤儿引用（对全部生成场景成立）。
    #[test]
    fn fold_never_leaves_orphan_references(seed in any::<u64>(), steps in 4usize..48) {
        let scenario = scenario(seed, steps, 0);
        let folded = FoldEngine::new().fold(&scenario.log).unwrap();
        prop_assert!(
            folded.state.violations().is_empty(),
            "折叠结果存在孤儿引用: {:?}",
            folded.state.violations()
        );
        prop_assert!(scenario.final_state.is_consistent());
    }

    /// 不变量 2：收敛性（增量折叠 == 完整折叠 == 生成器维护的状态）。
    #[test]
    fn incremental_fold_converges_with_full_fold(seed in any::<u64>(), steps in 4usize..48) {
        let scenario = scenario(seed, steps, 0);
        let mut folder = IncrementalFolder::new();
        let mut chunk = 0usize;
        // 分批推进，模拟新原子陆续到达。
        while chunk < scenario.log.len() {
            chunk += 3;
            let upto = scenario.log.atoms().get(chunk.saturating_sub(1)).map(|a| a.seq).unwrap_or(0);
            folder.fold(&scenario.log, upto).unwrap();
        }
        let incremental = folder.fold_head(&scenario.log).unwrap();
        let full = state_at(&scenario.log, scenario.log.head_seq(), &mut StateAtCache::new()).unwrap();
        prop_assert_eq!(incremental.state.clone(), full.state.clone());
        prop_assert_eq!(full.state.clone(), scenario.final_state.clone());
    }

    /// 不变量 5：历史可重放——任意 `state@seq_n` 两次求值一致且结构自洽。
    #[test]
    fn any_state_at_seq_is_replayable(seed in any::<u64>(), steps in 4usize..48, raw_n in 0usize..64) {
        let scenario = scenario(seed, steps, 0);
        let n = clamp_seq(raw_n, &scenario.log);
        let first = state_at(&scenario.log, n, &mut StateAtCache::new()).unwrap();
        let second = state_at(&scenario.log, n, &mut StateAtCache::new()).unwrap();
        prop_assert_eq!(first.state.clone(), second.state.clone());
        prop_assert!(first.state.violations().is_empty(), "{:?}", first.state.violations());
        prop_assert!(first.state.head_seq <= n);
        prop_assert_eq!(first.seq, n);

        // 无 declare_head 的日志：state@n 必须等于对前缀的整体折叠。
        if scenario.log.last_declare_head_upto(n).is_none() {
            let direct = fold_atoms(Default::default(), scenario.log.atoms_upto(n));
            prop_assert_eq!(first.state.clone(), direct.state);
        }
    }

    /// 不变量 4：revert-reapply 往返恢复目标原子的效果，且不改变文档内容。
    #[test]
    fn revert_then_reapply_restores_content(
        seed in any::<u64>(),
        steps in 4usize..48,
        raw_pick in 0usize..64,
    ) {
        let scenario = scenario(seed, steps, 0);
        let before = FoldEngine::new().fold(&scenario.log).unwrap().state;
        let suppressed = compute_suppressed(scenario.log.atoms());
        let candidates: Vec<&Atom> = scenario
            .log
            .iter()
            .filter(|atom| {
                atom.kind.is_state_effect()
                    && !suppressed.contains(&atom.id)
                    && atom.seq > before.eval_origin_seq()
            })
            .collect();
        let Some(target) = candidates.get(raw_pick % candidates.len().max(1)).copied() else {
            return Ok(());
        };

        let mut log = scenario.log.clone();
        let revert = Atom::new(
            AtomKind::Revert,
            target.actor.clone(),
            target.session.clone(),
            serde_json::json!({"target": target.id}),
        );
        log.append(revert).unwrap();
        let reverted = FoldEngine::new().fold(&log).unwrap();
        prop_assert!(reverted.state.is_consistent(), "{:?}", reverted.state.violations());

        let reapply = Atom::new(
            AtomKind::Reapply,
            target.actor.clone(),
            target.session.clone(),
            serde_json::json!({"target": target.id}),
        );
        log.append(reapply).unwrap();
        let restored = FoldEngine::new().fold(&log).unwrap();
        prop_assert!(
            restored.state.same_content(&before),
            "revert+reapply 后文档内容必须回到撤销前"
        );
        prop_assert!(restored.state.is_consistent());
    }

    /// 不变量 5（GC 部分）：GC 只清理孤儿，历史与活跃 blob 永不删除，重放结果不变。
    #[test]
    fn gc_preserves_replayability(
        seed in any::<u64>(),
        steps in 4usize..40,
        blob_bytes in 1024usize..2600,
    ) {
        let scenario = scenario(seed, steps, blob_bytes);
        let head = scenario.log.head_seq();
        let before = state_at(&scenario.log, head, &mut StateAtCache::new()).unwrap().state;

        let manifest = scenario.final_state.active_blob_manifest();
        let roots = scenario.log.blob_roots();
        let now = now_ms() + 30 * 24 * 60 * 60 * 1000;
        let plan = plan_gc(
            &scenario.store,
            &scenario.log,
            &manifest,
            &BTreeSet::new(),
            now,
            DEFAULT_ORPHAN_TTL_SECONDS,
        )
        .unwrap();
        prop_assert!(
            plan.expiring.iter().all(|entry| !roots.contains(&entry.blob_hash)),
            "GC 计划不得包含日志引用闭包中的 blob"
        );
        let (_report, _plan) = run_gc(
            &scenario.store,
            &scenario.log,
            &manifest,
            &BTreeSet::new(),
            now,
            DEFAULT_ORPHAN_TTL_SECONDS,
        )
        .unwrap();

        // 历史与活跃 blob 全部仍可读回。
        for hash in &roots {
            prop_assert!(
                scenario.store.exists(hash),
                "GC 后历史 blob {hash} 必须仍存在（原则 21）"
            );
            prop_assert!(scenario.store.get(hash).is_ok());
        }
        // 重放结果不因 GC 改变。
        let after = state_at(&scenario.log, head, &mut StateAtCache::new()).unwrap().state;
        prop_assert_eq!(before, after);
    }

    /// 广播边界（6.8）：所有原子元数据保持轻量，大二进制只以 blob 引用形式出现。
    #[test]
    fn atom_metadata_stays_small_and_references_existing_blobs(
        seed in any::<u64>(),
        steps in 4usize..40,
    ) {
        let scenario = scenario(seed, steps, 4096);
        let mut with_blobs = 0usize;
        for atom in scenario.log.iter() {
            let payload_bytes = serde_json::to_vec(&atom.payload).unwrap().len();
            prop_assert!(
                payload_bytes < CAS_THRESHOLD_BYTES + 1024,
                "原子 {} 的 payload 达到 {payload_bytes} 字节，大二进制必须外置到 CAS",
                atom.id
            );
            let refs = atom.all_blob_refs();
            if !refs.is_empty() {
                with_blobs += 1;
                for hash in refs {
                    prop_assert!(
                        scenario.store.exists(&hash),
                        "原子 {} 引用的 blob 必须已上传（提交顺序协议）",
                        atom.id
                    );
                }
            }
        }
        prop_assert!(with_blobs > 0, "场景应产生携带 blob 的原子");
    }

    /// 提交时校验：被拒绝的原子绝不进入日志，且 seq 连续。
    #[test]
    fn rejected_atoms_never_enter_the_log(seed in any::<u64>(), steps in 4usize..48) {
        let scenario = scenario(seed, steps, 0);
        let rejected: BTreeSet<&str> = scenario
            .steps
            .iter()
            .filter(|step| !step.accepted)
            .map(|step| step.atom_id.as_str())
            .collect();
        for id in &rejected {
            prop_assert!(scenario.log.get(id).is_none(), "被拒绝的原子 {id} 不应进日志");
        }
        // seq 从 1 连续分配。
        for (index, atom) in scenario.log.iter().enumerate() {
            prop_assert_eq!(atom.seq, index as u64 + 1);
        }
        // ULID 幂等：重放同一原子返回既有 seq。
        if let Some(atom) = scenario.log.head_atom() {
            let mut log = scenario.log.clone();
            let outcome = log.append(atom.clone()).unwrap();
            prop_assert!(!outcome.is_appended());
            prop_assert_eq!(outcome.seq(), atom.seq);
            prop_assert_eq!(log.len(), scenario.log.len());
        }
    }
}
