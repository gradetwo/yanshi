//! HEAD 指针与 `state@seq` 求值公式（设计文档 5.5）。
//!
//! ```text
//! state@seq_n := fold(A_n, base_state(H_n))
//!
//!   H_n   := seq ≤ n 中最近一次 declare_head 原子（若存在）
//!   A_n   := 满足 H_n.seq < seq ≤ n 的全部原子
//!   base_state(H) := H.base 指向的状态：
//!       - atom_id      → 该原子时刻的 state@seq（递归定义，seq 严格递减，必然终止）
//!       - checkpoint_id → checkpoint.anchor_seq 对应的 state@seq（或其关联快照）
//!   若 H_n 不存在：base_state := 空白初始文档，A_n := seq ≤ n 的全部原子
//! ```
//!
//! ## 实现约定（对 5.5 的一处收紧）
//!
//! `declare_head` 意味着“求值起点跳变”：`A_n` 不含 `H_n` 自身，也不含 base 与 `H_n`
//! 之间的原子。若允许 `revert` 指向 `H_n` 之前的原子，折叠将无法仅凭 `base_state(H_n)`
//! 表达该撤销（被撤销的效果已经烘进 base）。因此引擎在**提交期**要求：
//!
//! > `revert` / `reapply` 的目标原子 seq 必须大于当前求值起点 seq
//! > （即目标必须位于当前 `declare_head` 之后）。
//!
//! 这一约束使 `A_n` 永远与有效集一致，且折叠区间内不会出现 `declare_head`，
//! 从而 `state@seq_n == fold(完整区间)` 在任意 n 上可复现。

use crate::atom::{Atom, AtomId, AtomKind};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::fold::{compute_suppressed, fold_atoms, FoldEngine, FoldResult, FoldWarning};
use crate::ids::{ActorId, Seq, SessionId};
use crate::log::AtomLog;
use crate::state::{DeclareHead, DocumentState, HeadBase};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// `state@seq` 的求值结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateAt {
    /// 求值目标 seq。
    pub seq: Seq,
    /// 实际折叠起点（`H_n.seq`；无 declare_head 时为 0）。
    pub origin_seq: Seq,
    /// 折叠后的状态。
    pub state: DocumentState,
    /// 区间内被有效 revert 撤销的原子。
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub suppressed: std::collections::BTreeSet<AtomId>,
    /// 折叠警告（级联失效等）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<FoldWarning>,
}

impl StateAt {
    /// 是否无警告。
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }
}

/// `state@seq` 求值缓存（快照之外的进程内记忆化，避免重复折叠）。
#[derive(Debug, Default, Clone)]
pub struct StateAtCache {
    memo: BTreeMap<Seq, StateAt>,
    hits: usize,
    misses: usize,
}

impl StateAtCache {
    /// 空缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// 缓存条目数。
    pub fn len(&self) -> usize {
        self.memo.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.memo.is_empty()
    }

    /// 命中次数。
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// 未命中次数。
    pub fn misses(&self) -> usize {
        self.misses
    }

    /// 清空缓存。
    pub fn clear(&mut self) {
        self.memo.clear();
    }

    /// 取某个 seq 的缓存结果。
    pub fn get(&self, seq: Seq) -> Option<&StateAt> {
        self.memo.get(&seq)
    }

    /// 写入缓存。
    pub fn insert(&mut self, value: StateAt) {
        self.memo.insert(value.seq, value);
    }
}

/// 计算 `state@seq_n`（设计文档 5.5）。
pub fn state_at(log: &AtomLog, n: Seq, cache: &mut StateAtCache) -> Result<StateAt> {
    if let Some(hit) = cache.memo.get(&n) {
        cache.hits += 1;
        return Ok(hit.clone());
    }
    cache.misses += 1;

    let head_atom = log.last_declare_head_upto(n);
    let mut base = DocumentState::empty();
    let mut origin_seq = 0;
    if let Some(head) = head_atom {
        origin_seq = head.seq;
        let base_seq = resolve_base_seq(log, head)?;
        if base_seq >= head.seq {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "declare_head {} 的 base seq {} 不早于自身 seq {}",
                    head.id, base_seq, head.seq
                )),
            )
            .with_atom(head.id.clone()));
        }
        base = state_at(log, base_seq, cache)?.state;
        // 求值起点跳变：此后的折叠以 H 为起点。
        base.declare_head = Some(declare_head_record(head)?);
        base.head_seq = head.seq;
        base.head_atom = Some(head.id.clone());
    }

    let slice = log.range_exclusive_inclusive(origin_seq, n);
    let FoldResult {
        mut state,
        warnings,
        suppressed,
        ..
    } = fold_atoms(base, slice);

    // 折叠到达 n（即使最后一个原子被撤销，“已折叠到”的位置仍是 n）。
    state.head_seq = state
        .head_seq
        .max(if slice.is_empty() { origin_seq } else { n });
    let result = StateAt {
        seq: n,
        origin_seq,
        state,
        suppressed,
        warnings,
    };
    cache.insert(result.clone());
    Ok(result)
}

/// 解析 `declare_head` 的 base 对应的 seq。
pub fn resolve_base_seq(log: &AtomLog, head: &Atom) -> Result<Seq> {
    let base =
        crate::fold::parse_head_base(&head.payload).map_err(|e| e.with_atom(head.id.clone()))?;
    match base {
        HeadBase::Atom(atom_id) => {
            let Some(target) = log.get(&atom_id) else {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("declare_head 的 base 原子 {atom_id} 不在日志中")),
                )
                .with_atom(head.id.clone()));
            };
            Ok(target.seq)
        }
        HeadBase::Checkpoint(checkpoint_id) => checkpoint_anchor_seq(log, &checkpoint_id, head.seq)
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("检查点 {checkpoint_id} 不存在")),
                )
                .with_atom(head.id.clone())
            }),
    }
}

/// 在 `seq ≤ upto` 的日志中查找检查点锚定的 seq。
///
/// 检查点是轻量元数据原子（设计文档 4.5），因此可以从日志中直接解析，
/// 不需要先折叠出状态。
pub fn checkpoint_anchor_seq(log: &AtomLog, checkpoint_id: &str, upto: Seq) -> Option<Seq> {
    log.atoms_upto(upto)
        .iter()
        .rev()
        .filter(|a| matches!(a.kind, AtomKind::CreateCheckpoint | AtomKind::Checkpoint))
        .find(|a| checkpoint_key(a).as_deref() == Some(checkpoint_id))
        .map(|a| crate::atom::payload_u64(&a.payload, "anchor_seq").unwrap_or(a.seq))
}

fn checkpoint_key(atom: &Atom) -> Option<String> {
    match crate::atom::payload_str(&atom.payload, "checkpoint_id") {
        Some(id) => Some(id.to_owned()),
        None => Some(format!("ckpt_{}", atom.id)),
    }
}

fn declare_head_record(head: &Atom) -> Result<DeclareHead> {
    Ok(DeclareHead {
        atom_id: head.id.clone(),
        seq: head.seq,
        base: crate::fold::parse_head_base(&head.payload)
            .map_err(|e| e.with_atom(head.id.clone()))?,
        reason: crate::atom::payload_str(&head.payload, "reason").map(str::to_owned),
    })
}

/// 增量折叠器：跨批次复用上一次的状态（设计文档 6.6 客户端/服务端同步增量折叠）。
///
/// 只有当新增区间内不含 `declare_head`、且没有 `revert`/`reapply` 指向旧区间时，
/// 才在上一次状态之上前推；否则回退到 [`state_at`] 的完整求值。
#[derive(Debug, Default, Clone)]
pub struct IncrementalFolder {
    cache: StateAtCache,
    last: Option<StateAt>,
    incremental_steps: usize,
    full_steps: usize,
}

impl IncrementalFolder {
    /// 空增量折叠器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 折叠到 seq `n`。
    pub fn fold(&mut self, log: &AtomLog, n: Seq) -> Result<StateAt> {
        if let Some(last) = &self.last {
            if last.seq <= n && self.can_extend(log, last, n) {
                let slice = log.range_exclusive_inclusive(last.seq, n);
                let FoldResult {
                    mut state,
                    warnings,
                    suppressed,
                    ..
                } = fold_atoms(last.state.clone(), slice);
                state.head_seq = state.head_seq.max(n);
                let result = StateAt {
                    seq: n,
                    origin_seq: last.origin_seq,
                    state,
                    suppressed,
                    warnings,
                };
                self.incremental_steps += 1;
                self.last = Some(result.clone());
                self.cache.insert(result.clone());
                return Ok(result);
            }
        }
        let result = state_at(log, n, &mut self.cache)?;
        self.full_steps += 1;
        self.last = Some(result.clone());
        Ok(result)
    }

    /// 折叠到当前 head。
    pub fn fold_head(&mut self, log: &AtomLog) -> Result<StateAt> {
        self.fold(log, log.head_seq())
    }

    /// 增量前推次数。
    pub fn incremental_steps(&self) -> usize {
        self.incremental_steps
    }

    /// 完整求值次数。
    pub fn full_steps(&self) -> usize {
        self.full_steps
    }

    /// `state@seq` 记忆化缓存。
    pub fn cache(&self) -> &StateAtCache {
        &self.cache
    }

    fn can_extend(&self, log: &AtomLog, last: &StateAt, n: Seq) -> bool {
        let slice = log.range_exclusive_inclusive(last.seq, n);
        if slice.iter().any(|a| a.kind == AtomKind::DeclareHead) {
            return false;
        }
        // revert/reapply 必须指向旧区间之后的目标（见模块级约定）。
        let suppressed = compute_suppressed(log.atoms_upto(n));
        for atom in slice {
            if matches!(atom.kind, AtomKind::Revert | AtomKind::Reapply) {
                if let Some(target_seq) = atom.target_atom().and_then(|t| log.seq_of(t)) {
                    if target_seq <= last.seq {
                        return false;
                    }
                }
            }
        }
        // 旧区间内的原子若被新区间撤销，撤销动作本身在新区间里，状态可继续前推；
        // 但被撤销的原子必须仍留在版本链上以便审计，因此这里只做上面的保守检查。
        let _ = suppressed;
        true
    }
}

impl FoldEngine {
    /// 折叠整个日志到当前 head（等价于 `state@seq_head`）。
    pub fn fold(&self, log: &AtomLog) -> Result<StateAt> {
        let mut cache = StateAtCache::new();
        state_at(log, log.head_seq(), &mut cache)
    }

    /// 折叠到指定 seq（时间旅行）。
    pub fn fold_upto(&self, log: &AtomLog, n: Seq) -> Result<StateAt> {
        let mut cache = StateAtCache::new();
        state_at(log, n, &mut cache)
    }

    /// 构造 `declare_head` 原子（求值起点跳变，设计文档 5.5）。
    pub fn declare_head_atom(
        &self,
        actor: impl Into<ActorId>,
        session: impl Into<SessionId>,
        base: HeadBase,
        reason: Option<&str>,
    ) -> Atom {
        let base_json = serde_json::to_value(&base).unwrap_or(json!(null));
        let mut payload = json!({ "base": base_json });
        if let Some(reason) = reason {
            payload["reason"] = json!(reason);
        }
        Atom::new(AtomKind::DeclareHead, actor, session, payload)
    }

    /// `revert_to` 通过 `declare_head` 统一实现（设计文档 4.5 / 5.5）。
    pub fn revert_to_atom(
        &self,
        actor: impl Into<ActorId>,
        session: impl Into<SessionId>,
        target_atom: impl Into<AtomId>,
    ) -> Atom {
        self.declare_head_atom(
            actor,
            session,
            HeadBase::Atom(target_atom.into()),
            Some("revert_to"),
        )
    }

    /// `restore_checkpoint` 通过 `declare_head` 统一实现。
    pub fn restore_checkpoint_atom(
        &self,
        actor: impl Into<ActorId>,
        session: impl Into<SessionId>,
        checkpoint_id: impl Into<String>,
    ) -> Atom {
        self.declare_head_atom(
            actor,
            session,
            HeadBase::Checkpoint(checkpoint_id.into()),
            Some("restore_checkpoint"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn atom(kind: AtomKind, id: &str, payload: Value) -> Atom {
        Atom::new(kind, "human:1", "session:a", payload).with_id(id)
    }

    fn push(log: &mut AtomLog, atom: Atom) -> Seq {
        log.append(atom).unwrap().seq()
    }

    fn chain() -> AtomLog {
        let mut log = AtomLog::new();
        push(
            &mut log,
            atom(
                AtomKind::CreateDocument,
                "a_doc",
                json!({"doc_id": "doc_1", "width": 100, "height": 50}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateLayer,
                "a_layer",
                json!({"layer_id": "layer_1"}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        log
    }

    #[test]
    fn state_at_head_equals_full_fold() {
        let log = chain();
        let mut cache = StateAtCache::new();
        let at_head = state_at(&log, log.head_seq(), &mut cache).unwrap();
        assert_eq!(at_head.origin_seq, 0);
        assert_eq!(at_head.state.head_seq, 3);
        assert_eq!(at_head.state.doc_id.as_deref(), Some("doc_1"));
        assert!(at_head.state.is_consistent());
    }

    #[test]
    fn state_at_is_stable_and_cached() {
        let log = chain();
        let mut cache = StateAtCache::new();
        let first = state_at(&log, 2, &mut cache).unwrap();
        assert_eq!(first.state.objects.len(), 0);
        assert_eq!(first.state.layers.len(), 1);
        let second = state_at(&log, 2, &mut cache).unwrap();
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn time_travel_before_and_after_an_edit() {
        let mut log = chain();
        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        let mut cache = StateAtCache::new();
        let at_3 = state_at(&log, 3, &mut cache).unwrap();
        let at_4 = state_at(&log, 4, &mut cache).unwrap();
        assert_eq!(at_3.state.objects["obj_1"].versions, vec!["a_obj"]);
        assert_eq!(
            at_4.state.objects["obj_1"].versions,
            vec!["a_obj", "a_super"]
        );
        assert_eq!(at_4.state.head_seq, 4);
    }

    #[test]
    fn declare_head_jumps_the_evaluation_origin() {
        let mut log = chain();
        // 在 seq 3 之后追加一个版本，再 declare_head 回到 seq 2 的时刻。
        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        let head_atom = Atom::new(
            AtomKind::DeclareHead,
            "human:1",
            "session:a",
            json!({"base": {"type": "atom", "id": "a_layer"}, "reason": "revert_to"}),
        )
        .with_id("a_head");
        push(&mut log, head_atom);
        // 跳变之后的新编辑。
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj2",
                json!({"object_id": "obj_2", "layer_id": "layer_1", "type": "shape"}),
            ),
        );

        let mut cache = StateAtCache::new();
        let head = state_at(&log, log.head_seq(), &mut cache).unwrap();
        assert_eq!(head.origin_seq, 5, "declare_head 原子自身位于 seq 5");
        assert_eq!(
            head.state.declare_head.as_ref().map(|h| h.seq),
            Some(5),
            "求值起点跳到 declare_head"
        );
        assert!(head.state.objects.contains_key("obj_2"));
        assert!(
            !head.state.objects.contains_key("obj_1"),
            "declare_head 之前的对象不在跳变后的状态里"
        );
        assert!(head.state.is_consistent());

        // 跳变之前的时间旅行仍按原历史求值。
        let before = state_at(&log, 3, &mut cache).unwrap();
        assert!(before.state.objects.contains_key("obj_1"));
        assert_eq!(before.origin_seq, 0);
    }

    #[test]
    fn checkpoint_restore_jumps_to_anchor_seq() {
        let mut log = chain();
        push(
            &mut log,
            atom(
                AtomKind::CreateCheckpoint,
                "a_ckpt",
                json!({"checkpoint_id": "ckpt_1", "name": "v1", "anchor_seq": 3}),
            ),
        );
        // 检查点之后再做修改。
        push(
            &mut log,
            atom(AtomKind::Tombstone, "a_del", json!({"object_id": "obj_1"})),
        );
        assert_eq!(checkpoint_anchor_seq(&log, "ckpt_1", 5), Some(3));

        let engine = FoldEngine::new();
        let restore = engine
            .restore_checkpoint_atom("human:1", "session:a", "ckpt_1")
            .with_id("a_restore");
        push(&mut log, restore);

        let mut cache = StateAtCache::new();
        let head = state_at(&log, log.head_seq(), &mut cache).unwrap();
        assert_eq!(head.origin_seq, 6);
        assert!(
            head.state.objects["obj_1"].is_alive(),
            "恢复到检查点锚定时刻，删除不再生效"
        );
        // 检查点原子位于跳变区间内，因此不出现在跳变后的状态里；
        // 它仍可从日志解析（checkpoint_anchor_seq），这正是 HeadBase::Checkpoint 的解析路径。
        assert!(head.state.checkpoints.is_empty());
        assert_eq!(checkpoint_anchor_seq(&log, "ckpt_1", head.seq), Some(3));
    }

    #[test]
    fn incremental_fold_converges_with_full_fold() {
        let mut log = chain();
        let mut folder = IncrementalFolder::new();
        let step1 = folder.fold_head(&log).unwrap();
        assert_eq!(folder.incremental_steps(), 0);
        assert_eq!(folder.full_steps(), 1);

        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        let step2 = folder.fold_head(&log).unwrap();
        push(
            &mut log,
            atom(AtomKind::Tombstone, "a_del", json!({"object_id": "obj_1"})),
        );
        let step3 = folder.fold_head(&log).unwrap();
        assert!(folder.incremental_steps() >= 2, "新增原子走增量前推");

        let mut cache = StateAtCache::new();
        let full = state_at(&log, log.head_seq(), &mut cache).unwrap();
        assert_eq!(step1.state.head_seq, 3);
        assert_eq!(step2.state.head_seq, 4);
        assert_eq!(step3.state, full.state, "增量折叠必须与完整折叠收敛");
        assert!(full.state.is_consistent());
    }

    #[test]
    fn incremental_fold_falls_back_on_declare_head() {
        let mut log = chain();
        let mut folder = IncrementalFolder::new();
        folder.fold_head(&log).unwrap();
        let engine = FoldEngine::new();
        let head = engine
            .revert_to_atom("human:1", "session:a", "a_layer")
            .with_id("a_head");
        push(&mut log, head);
        let after = folder.fold_head(&log).unwrap();
        let mut cache = StateAtCache::new();
        let full = state_at(&log, log.head_seq(), &mut cache).unwrap();
        assert_eq!(after.state, full.state);
        assert_eq!(after.origin_seq, 4);
        assert!(folder.full_steps() >= 2, "declare_head 触发完整求值");
    }

    #[test]
    fn cross_origin_revert_is_rejected_at_commit() {
        let mut log = chain();
        let engine = FoldEngine::new();
        let head = engine
            .revert_to_atom("human:1", "session:a", "a_layer")
            .with_id("a_head");
        push(&mut log, head);
        let mut cache = StateAtCache::new();
        let state = state_at(&log, log.head_seq(), &mut cache).unwrap().state;
        assert_eq!(state.eval_origin_seq(), 4);

        let none = |_: &crate::atom::BlobHash| false;
        let ctx = crate::log::CommitContext::new(&state, &none, "human:1", "session:a");
        let revert_old = atom(AtomKind::Revert, "a_rev_old", json!({"target": "a_obj"}));
        let error = log.append_validated(revert_old, &ctx).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);

        // 指向跳变之后的原子则允许。
        let revert_new = atom(AtomKind::Revert, "a_rev_new", json!({"target": "a_head"}));
        // declare_head 不是 revert 合法目标。
        assert_eq!(
            log.append_validated(revert_new, &ctx).unwrap_err().code,
            ErrorCode::InvalidArgument
        );
    }
}
