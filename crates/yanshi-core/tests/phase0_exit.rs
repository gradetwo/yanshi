//! Phase 0 出口条件与极端序列（设计文档 16 / 19）。
//!
//! - 极端原子序列：`create → supersede → revert(supersede) → revert(create) → reapply(create)`，
//!   验证无孤儿对象与悬挂指针。
//! - `revert → GC 周期 → reapply`，验证历史级 blob 取回正确（GC 不破坏可回放性）。
//! - 10 万原子折叠 fuzz（`#[ignore]`，由 CI 的 `--ignored` 长跑作业执行）。

use serde_json::json;
use std::collections::BTreeSet;
use std::time::Instant;
use yanshi_core::blob::{plan_gc, run_gc, BlobStore, DEFAULT_ORPHAN_TTL_SECONDS};
use yanshi_core::fold::fold_atoms;
use yanshi_core::seq::state_at;
use yanshi_core::testkit::{generate, ScenarioConfig};
use yanshi_core::{
    Atom, AtomKind, AtomLog, BlobHash, DocumentState, FoldEngine, MemoryBlobStore, StateAtCache,
};

fn atom(kind: AtomKind, id: &str, payload: serde_json::Value) -> Atom {
    Atom::new(kind, "ai:1", "session:ai", payload).with_id(id)
}

fn fold(log: &AtomLog) -> DocumentState {
    FoldEngine::new().fold(log).unwrap().state
}

#[test]
fn extreme_sequence_has_no_orphans_or_dangling_pointers() {
    let mut log = AtomLog::new();
    log.append(atom(
        AtomKind::CreateDocument,
        "a_doc",
        json!({"doc_id": "doc_1", "width": 256, "height": 256}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::CreateLayer,
        "a_layer",
        json!({"layer_id": "layer_1"}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::CreateObject,
        "a_create",
        json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::Supersede,
        "a_super",
        json!({"object_id": "obj_1", "data": {"v": 2}}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::Revert,
        "a_revert_super",
        json!({"target": "a_super"}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::Revert,
        "a_revert_create",
        json!({"target": "a_create"}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::Reapply,
        "a_reapply_create",
        json!({"target": "a_create"}),
    ))
    .unwrap();

    let result = FoldEngine::new().fold(&log).unwrap();
    assert!(result.is_suppressed("a_super"));
    assert!(!result.is_suppressed("a_create"));
    assert!(
        result.state.violations().is_empty(),
        "{:?}",
        result.state.violations()
    );

    let object = &result.state.objects["obj_1"];
    assert_eq!(
        object.versions,
        vec!["a_create"],
        "被撤销的 supersede 不在版本链上"
    );
    assert_eq!(object.current_version.as_deref(), Some("a_create"));
    assert!(!object.is_deleted());
    assert_eq!(object.layer_id, "layer_1");
    assert!(result.state.layer_alive(&object.layer_id));

    // 折叠结果与任意起点求值一致。
    for n in 0..=log.head_seq() {
        let evaluated = state_at(&log, n, &mut StateAtCache::new()).unwrap();
        assert!(evaluated.state.violations().is_empty(), "n={n}");
    }
}

#[test]
fn revert_gc_cycle_then_reapply_keeps_history_retrievable() {
    let store = MemoryBlobStore::new();
    let bitmap = vec![9u8; 8192];
    let blob_hash = store.put(&bitmap).unwrap();

    let mut log = AtomLog::new();
    log.append(atom(
        AtomKind::CreateDocument,
        "a_doc",
        json!({"doc_id": "doc_1", "width": 128, "height": 128}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::CreateLayer,
        "a_layer",
        json!({"layer_id": "layer_1"}),
    ))
    .unwrap();
    log.append(atom(
        AtomKind::CreateObject,
        "a_patch",
        json!({
            "object_id": "obj_patch",
            "layer_id": "layer_1",
            "type": "raster_patch",
            "bitmap": {"blob_hash": blob_hash, "size": 8192, "mime_type": "image/webp"},
        }),
    ))
    .unwrap();
    let state_with_patch = fold(&log);
    assert!(state_with_patch.active_blob_manifest().contains(&blob_hash));

    // revert：blob 从活跃集掉到历史级。
    log.append(atom(
        AtomKind::Revert,
        "a_revert",
        json!({"target": "a_patch"}),
    ))
    .unwrap();
    let reverted = fold(&log);
    assert!(!reverted.active_blob_manifest().contains(&blob_hash));

    // GC 周期：历史级 blob 必须保留。
    let now = yanshi_core::now_ms() + 90 * 24 * 60 * 60 * 1000;
    let plan = plan_gc(
        &store,
        &log,
        &reverted.active_blob_manifest(),
        &BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )
    .unwrap();
    assert!(plan.roots.contains(&blob_hash));
    assert!(plan
        .historical
        .iter()
        .any(|entry| entry.blob_hash == blob_hash));
    assert!(plan.expiring.is_empty(), "被日志引用的 blob 永不过期");
    let (report, _) = run_gc(
        &store,
        &log,
        &reverted.active_blob_manifest(),
        &BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )
    .unwrap();
    assert!(report.deleted.is_empty());
    assert!(store.exists(&blob_hash));

    // reapply：历史级 blob 取回成功，状态回到撤销前。
    log.append(atom(
        AtomKind::Reapply,
        "a_reapply",
        json!({"target": "a_patch"}),
    ))
    .unwrap();
    let restored = fold(&log);
    assert!(restored.same_content(&state_with_patch));
    assert!(restored.active_blob_manifest().contains(&blob_hash));
    assert_eq!(store.get(&blob_hash).unwrap(), bitmap);
}

#[test]
fn orphans_are_reclaimed_only_after_ttl() {
    let store = MemoryBlobStore::new();
    let orphan = store.put_at(b"never-referenced", 1_000).unwrap();
    let log = AtomLog::new();
    let now = 1_000_000_000;

    let (report, _) = run_gc(
        &store,
        &log,
        &BTreeSet::new(),
        &BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )
    .unwrap();
    assert_eq!(report.deleted, vec![orphan.clone()]);
    assert!(!store.exists(&orphan));
    assert_eq!(report.bytes_reclaimed, 16);
}

#[test]
#[ignore = "长跑 fuzz：10 万原子，CI 用 --ignored 执行（Phase 0 出口条件）"]
fn fuzz_100k_atoms_keeps_all_invariants() {
    let config = ScenarioConfig::fuzz(202_600_00, 100_000);
    let started = Instant::now();
    let scenario = generate(&config);
    let generation = started.elapsed();

    let head = scenario.log.head_seq();
    assert!(
        scenario.log.len() >= 90_000,
        "接受率过低：{} / {}",
        scenario.log.len(),
        config.steps
    );

    // 完整折叠（从空白起点重放全部原子）。
    let fold_started = Instant::now();
    let full = state_at(&scenario.log, head, &mut StateAtCache::new()).unwrap();
    let fold_elapsed = fold_started.elapsed();

    assert!(
        full.state.violations().is_empty(),
        "10 万原子折叠后存在孤儿引用: {:?}",
        &full.state.violations()[..full.state.violations().len().min(5)]
    );
    assert_eq!(
        full.state, scenario.final_state,
        "增量维护的状态必须与完整折叠收敛"
    );

    // GC 不破坏可回放性。
    let manifest = full.state.active_blob_manifest();
    let roots: BTreeSet<BlobHash> = scenario.log.blob_roots();
    let now = yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000;
    let plan = plan_gc(
        &scenario.store,
        &scenario.log,
        &manifest,
        &BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )
    .unwrap();
    assert!(plan
        .expiring
        .iter()
        .all(|entry| !roots.contains(&entry.blob_hash)));
    for hash in &roots {
        assert!(scenario.store.exists(hash), "历史 blob {hash} 被误删");
    }

    // 稳定复现：同一日志再折叠一次结果一致（幂等性）。
    let again = fold_atoms(DocumentState::empty(), scenario.log.atoms());
    assert_eq!(again.state, full.state);

    println!(
        "10 万原子 fuzz：原子 {} / 快照 {} / 接受 {} / 拒绝 {} / 生成 {:?} / 折叠 {:?} / blob {}",
        scenario.log.len(),
        scenario.snapshots.len(),
        scenario.stats.accepted,
        scenario.stats.rejected,
        generation,
        fold_elapsed,
        roots.len()
    );
}
