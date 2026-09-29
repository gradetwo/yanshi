//! 端到端示例：提交 → 撤销 → 时间旅行 → GC。
//!
//! 运行：
//!
//! ```bash
//! cargo run -p yanshi-core --example quickstart
//! ```
//!
//! 示例演示设计文档中的核心闭环：
//!
//! 1. Blob 先行（6.3 提交顺序协议），原子后行；
//! 2. 提交时校验（12.2）与 ULID 幂等（5.1）；
//! 3. 折叠求值、`revert` 级联失效（5.3）；
//! 4. `state@seq` 时间旅行（5.5）；
//! 5. Blob 三级生命周期与 GC 根集（6.3 / 原则 21）。

use serde_json::json;
use yanshi_core::blob::{plan_gc, BlobStore, MemoryBlobStore, DEFAULT_ORPHAN_TTL_SECONDS};
use yanshi_core::seq::state_at;
use yanshi_core::{Atom, AtomKind, AtomLog, CommitContext, FoldEngine, StateAtCache};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryBlobStore::new();
    let mut log = AtomLog::new();

    // 1) 大二进制先行：8192 字节的 RasterPatch 位图进入 CAS，原子只存引用。
    let bitmap = vec![0x5Au8; 8192];
    let bitmap_ref = yanshi_core::blob::stage_blob(&store, &bitmap, "image/webp")?;
    println!(
        "blob 已上传：{}（{} 字节，{}）",
        bitmap_ref.blob_hash, bitmap_ref.size, bitmap_ref.mime_type
    );

    // 2) 依次提交原子；每次都走提交时校验。
    let mut pending = vec![
        Atom::new(
            AtomKind::CreateDocument,
            "human:1",
            "session:web",
            json!({"doc_id": "doc_1", "width": 1024, "height": 1024, "color_space": "srgb"}),
        ),
        Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:web",
            json!({"layer_id": "layer_paint", "name": "paint"}),
        ),
        Atom::new(
            AtomKind::CreateObject,
            "ai:1",
            "session:mcp",
            json!({
                "object_id": "obj_sky",
                "layer_id": "layer_paint",
                "type": "raster_patch",
                "prompt": "夕阳下的天空",
                "bitmap": bitmap_ref,
            }),
        ),
    ];

    let mut seqs = Vec::new();
    for atom in pending.drain(..) {
        let state = FoldEngine::new().fold(&log)?.state;
        let exists = |hash: &yanshi_core::BlobHash| store.exists(hash);
        let actor = atom.actor.clone();
        let session = atom.session.clone();
        let commit = CommitContext::new(&state, &exists, &actor, &session);
        let outcome = log.append_validated(atom, &commit)?;
        seqs.push(outcome.seq());
        println!(
            "提交成功：seq={} kind={} duplicate={}",
            outcome.seq(),
            log.by_seq(outcome.seq())
                .map(|a| a.kind.to_string())
                .unwrap_or_default(),
            !outcome.is_appended()
        );
    }

    // 3) ULID 幂等：重放同一原子只返回既有 seq，不产生新记录。
    if let Some(duplicate) = log.by_seq(seqs[2]).cloned() {
        let before = log.len();
        let outcome = log.append(duplicate)?;
        println!(
            "幂等重放：seq={} 日志长度 {} -> {}",
            outcome.seq(),
            before,
            log.len()
        );
    }

    let head_state = FoldEngine::new().fold(&log)?.state;
    println!(
        "HEAD：seq={} 图层 {} 个，对象 {} 个，活跃 blob {} 个",
        head_state.head_seq,
        head_state.alive_layers().len(),
        head_state.alive_objects().len(),
        head_state.active_blob_manifest().len()
    );

    // 4) 撤销 AI 的生成物：blob 从活跃集掉入历史级。
    let revert = Atom::new(
        AtomKind::Revert,
        "human:1",
        "session:web",
        json!({"target": log.by_seq(seqs[2]).map(|a| a.id.clone()).unwrap_or_default()}),
    );
    let state = FoldEngine::new().fold(&log)?.state;
    let exists = |hash: &yanshi_core::BlobHash| store.exists(hash);
    // 跨 actor revert 默认拒绝（12.5）；这里演示 owner 显式放行。
    let commit = CommitContext::new(&state, &exists, "human:1", "session:web")
        .allow_cross_actor_revert(true);
    let outcome = log.append_validated(revert, &commit)?;
    let reverted = FoldEngine::new().fold(&log)?.state;
    println!(
        "撤销 seq={}：对象 {} 个（活跃 blob {} 个）",
        outcome.seq(),
        reverted.alive_objects().len(),
        reverted.active_blob_manifest().len()
    );

    // 5) 时间旅行：回到撤销之前的 seq，状态可重放。
    let mut cache = StateAtCache::new();
    let past = state_at(&log, seqs[2], &mut cache)?;
    println!(
        "时间旅行 state@{}：对象 {} 个，active_blob_manifest {} 个",
        past.seq,
        past.state.alive_objects().len(),
        past.state.active_blob_manifest().len()
    );

    // 6) GC：根集 = 全日志引用闭包，只清理从未被引用的孤儿。
    let orphan = store.put(b"temp-upload")?;
    let now = yanshi_core::now_ms() + 30 * 24 * 60 * 60 * 1000;
    let plan = plan_gc(
        &store,
        &log,
        &reverted.active_blob_manifest(),
        &std::collections::BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )?;
    println!(
        "GC 计划：活跃 {} / 历史 {} / 孤儿 {}（其中超 TTL 可清理 {}）",
        plan.active.len(),
        plan.historical.len(),
        plan.orphans.len(),
        plan.expiring.len()
    );
    let (report, _) = yanshi_core::blob::run_gc(
        &store,
        &log,
        &reverted.active_blob_manifest(),
        &std::collections::BTreeSet::new(),
        now,
        DEFAULT_ORPHAN_TTL_SECONDS,
    )?;
    println!(
        "GC 执行：删除 {} 个孤儿，回收 {} 字节；历史 blob {} 仍可读回：{}",
        report.deleted.len(),
        report.bytes_reclaimed,
        bitmap_ref.blob_hash,
        store.get(&bitmap_ref.blob_hash)?.len()
    );
    println!("孤儿 {} 已清理：{}", orphan, !store.exists(&orphan));

    println!("一致性检查通过：{}", reverted.is_consistent());
    Ok(())
}
