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
use crate::fold::{
    fold_atoms, fold_atoms_skipping, EffectiveActions, FoldEngine, FoldResult, FoldWarning,
};
use crate::ids::{ActorId, Seq, SessionId};
use crate::log::AtomLog;
use crate::state::{DeclareHead, DocumentState, HeadBase};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

    /// 该原子是否被有效 revert 撤销（设计文档 5.3 有效集）。
    pub fn is_suppressed(&self, atom_id: &str) -> bool {
        self.suppressed.contains(atom_id)
    }
}

/// `state@seq` 求值缓存（快照之外的进程内记忆化，避免重复折叠）。
///
/// 缓存是**有界**的：`state@seq` 的结果是完整状态，长日志下无界缓存会吃掉数 GB 内存。
/// 超出容量时按插入顺序淘汰最早的条目；被淘汰只是回到重新折叠，不影响正确性。
#[derive(Debug, Clone)]
pub struct StateAtCache {
    memo: BTreeMap<Seq, StateAt>,
    order: VecDeque<Seq>,
    max_entries: usize,
    hits: usize,
    misses: usize,
}

impl Default for StateAtCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StateAtCache {
    /// 默认容量（32 个状态）。
    pub const DEFAULT_CAPACITY: usize = 32;

    /// 以默认容量构造空缓存。
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    /// 指定容量构造空缓存（容量至少为 1）。
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            memo: BTreeMap::new(),
            order: VecDeque::new(),
            max_entries: max_entries.max(1),
            hits: 0,
            misses: 0,
        }
    }

    /// 容量上限。
    pub fn capacity(&self) -> usize {
        self.max_entries
    }

    /// 当前条目数。
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
        self.order.clear();
    }

    /// 取某个 seq 的缓存结果。
    pub fn get(&self, seq: Seq) -> Option<&StateAt> {
        self.memo.get(&seq)
    }

    /// 取 `seq < upto` 中**最新**的、求值起点为 `origin` 的缓存结果。
    ///
    /// 增量折叠用它做"还原点"：撤销状态变化只影响 `earliest` 之后的原子，
    /// 因此可以拿 `earliest` 之前最近的一份完整状态当作重新折叠的起点，
    /// 而不必从 seq 0 全量重放。
    pub fn newest_before(&self, upto: Seq, origin: Seq) -> Option<&StateAt> {
        self.memo
            .range(..upto)
            .rev()
            .find(|(_, state)| state.origin_seq == origin)
            .map(|(_, state)| state)
    }

    /// 丢弃 `seq ≥ from` 的缓存条目。
    ///
    /// **为什么必须有**：缓存里的每一条都是"某时刻的完整状态"。一旦某个 seq 更小的
    /// 原子被新来的 `revert` 改了撤销状态，**所有起点 ≥ 该原子 seq 的旧状态就作废了**
    /// （它们当时把该原子的效果折进去了）。实测：漏掉这一步 ⇒ 4K 画作的增量折叠
    /// 与完整折叠在 `layer_default.created_by` 上分歧。
    pub fn discard_from(&mut self, from: Seq) {
        if self.memo.is_empty() {
            return;
        }
        let _discarded = self.memo.split_off(&from);
        self.order.retain(|seq| *seq < from);
    }

    /// 写入缓存并执行淘汰。
    pub fn insert(&mut self, value: StateAt) {
        let seq = value.seq;
        if self.memo.insert(seq, value).is_none() {
            self.order.push_back(seq);
        }
        while self.memo.len() > self.max_entries {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.memo.remove(&oldest);
        }
    }
}

/// 计算 `state@seq_n`（设计文档 5.5）。
///
/// 实现为**迭代**而非递归：`declare_head` 链可能很长（重型原子会强制快照并留下跳变），
/// 递归会耗尽栈空间。链上每一层都会写入缓存，重复求值 O(1)。
pub fn state_at(log: &AtomLog, n: Seq, cache: &mut StateAtCache) -> Result<StateAt> {
    if let Some(hit) = cache.memo.get(&n) {
        cache.hits += 1;
        return Ok(hit.clone());
    }
    cache.misses += 1;

    /// 一层 `declare_head` 跳变：把 `state@from` 作为 base，折叠 `(from, upto]`。
    struct Level {
        head: DeclareHead,
        /// 折叠区间上界（含）。
        upto: Seq,
        /// 折叠区间下界（不含），等于该 `declare_head` 的 seq。
        from: Seq,
    }

    // 阶段一：沿 declare_head 链向内走，直到命中缓存或链底。
    let mut levels: Vec<Level> = Vec::new();
    let mut cursor = n;
    let mut cached_tail: Option<StateAt> = None;
    while let Some(head) = log.last_declare_head_upto(cursor) {
        if let Some(hit) = cache.memo.get(&cursor) {
            cache.hits += 1;
            cached_tail = Some(hit.clone());
            break;
        }
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
        levels.push(Level {
            head: declare_head_record(head)?,
            upto: cursor,
            from: head.seq,
        });
        cursor = base_seq;
    }

    let mut warnings: Vec<FoldWarning> = Vec::new();
    let mut suppressed: BTreeSet<AtomId> = BTreeSet::new();

    // 阶段二：求出链底状态 `state@cursor`（缓存命中，或从空白起点折叠 (0, cursor]）。
    let mut state = match cached_tail {
        Some(hit) => {
            warnings.extend(hit.warnings);
            suppressed.extend(hit.suppressed);
            hit.state
        }
        None if cursor == 0 => DocumentState::empty(),
        None => {
            let slice = log.range_exclusive_inclusive(0, cursor);
            let FoldResult {
                mut state,
                warnings: tail_warnings,
                suppressed: tail_suppressed,
                ..
            } = fold_atoms(DocumentState::empty(), slice);
            state.head_seq = state.head_seq.max(cursor);
            warnings.extend(tail_warnings.iter().cloned());
            suppressed.extend(tail_suppressed.iter().cloned());
            cache.insert(StateAt {
                seq: cursor,
                origin_seq: 0,
                state: state.clone(),
                suppressed: tail_suppressed,
                warnings: tail_warnings,
            });
            state
        }
    };

    // 阶段三：由内向外逐层折叠，每层都写回缓存。
    for level in levels.iter().rev() {
        state.declare_head = Some(level.head.clone());
        state.head_seq = level.from;
        state.head_atom = Some(level.head.atom_id.clone());

        let slice = log.range_exclusive_inclusive(level.from, level.upto);
        let folded = fold_atoms(state, slice);
        state = folded.state;
        state.head_seq = state.head_seq.max(if slice.is_empty() {
            level.from
        } else {
            level.upto
        });
        warnings.extend(folded.warnings.iter().cloned());
        suppressed.extend(folded.suppressed.iter().cloned());

        // **写回缓存时必须用"累积到本层为止"的有效集与警告**，而不是本层的**局部**集合 ✗：
        // 缓存条目会被当作链上"已算好的尾状态"复用（阶段一的 `cache.memo.get(&cursor)`），
        // 那里要的是"seq ≤ 该 seq 的全部有效集/警告"✓。此前存的是局部集合 ⇒ 复用时
        // **少报**内层的撤销与警告 ✗ ⇒ `state@seq` 的结果取决于缓存里恰好有什么 ✗
        //（实测：同一份日志在 upto=33 处给出 `{}` 与 `{"…"}` 两个不同的有效集 ✓）。
        cache.insert(StateAt {
            seq: level.upto,
            origin_seq: level.from,
            state: state.clone(),
            suppressed: suppressed.clone(),
            warnings: warnings.clone(),
        });
    }

    // `origin_seq` = `H_n.seq`，即 **seq ≤ n 中最近一次 `declare_head`**（模块文档的
    // 定义）⇒ 取**最外**一层，而不是 `declare_head` 链最里面那一层。
    //
    // **这曾经取决于缓存内容** ✗：阶段一沿着链向内走，命中缓存就提前停下 ⇒ 链长 > 1
    // 时"命中缓存 ⇒ 只压入最外层"、"没命中 ⇒ 一路走到最内层"，两种情况下这里给出的
    // `origin_seq` 不同（实测同一份日志 32 vs 10 ✓）。而缓存是旁路、不该改变语义 ✓
    //（`state@seq` 的结果必须可复现 ✓）。`log.rs` 的提交期校验用的是
    // `state.eval_origin_seq()`（= `declare_head.seq`，也是"最近一层"✓）⇒ 取最外层才一致 ✓。
    let origin_seq = levels.first().map(|level| level.from).unwrap_or(0);
    if levels.is_empty() {
        state.head_seq = state.head_seq.max(n);
    }
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

/// 常驻还原点的步长：每这么多 seq 留一份完整状态，供"撤销旧原子"时续折。
const RESUME_STRIDE: Seq = 32;
/// 常驻还原点上限（覆盖 `RESUME_STRIDE × STRIDE_CAPACITY` 个 seq）。
const STRIDE_CAPACITY: usize = 16;
/// `state@seq` 记忆化容量（完整求值在 `declare_head` 链上每层写一条）。
///
/// 增量前推**不再**往这里写：每写一条就是一次完整状态深拷贝，实测反而更慢
/// （见 [`IncrementalFolder::remember`]）⇒ 它只是 `state_at` 自己的记忆化，
/// 外加几个顺带的还原点。
const RESUME_CAPACITY: usize = 32;

/// 增量折叠器：跨批次复用上一次的状态（设计文档 6.6 客户端/服务端同步增量折叠）。
///
/// **为什么不能简单地"见 revert 就全量重放"**：真实 4K 画作（492 原子，其中 **79 个
/// `revert`**）实测 ⇒ 逐条前推 494 次里 **80 次退回全量**、折叠量 **34884** 个原子而
/// 理想只需 **494** 个（**70.6×**）⇒ 单次提交成本随历史线性增长。
///
/// 现在的三条路：
///
/// 1. **前推**：新增区间不含 `declare_head`，且[`EffectiveActions`]告诉我们"已折叠区间里
///    没有任何原子的撤销状态变化" ⇒ 直接在上一次状态之上折新增区间（O(新增)）。
/// 2. **续折（rebase）**：撤销状态确实变了（例如 `revert` 指向旧原子）⇒ 取**变化点之前
///    最近的一份还原点**重新折叠 `(还原点, n]`（O(距离 + 步长)），**而不是从 seq 0 重来**。
/// 3. **完整求值**：区间内出现 `declare_head`（求值起点跳变），或找不到 `n` 之前的还原点
///    ⇒ 回到 [`state_at`]。这条路的语义与 [`FoldEngine::fold`] 完全一致。
///
/// 不变量：无论走哪条路，返回的 [`StateAt`]（状态、有效集、警告）都与
/// [`state_at`] 的完整求值**逐字段一致**。
#[derive(Debug, Clone)]
pub struct IncrementalFolder {
    cache: StateAtCache,
    last: Option<StateAt>,
    /// 当前求值起点（`declare_head` 之后的起点；无跳变时为 0）。
    origin: Seq,
    /// 当前起点内已折叠原子的有效动作表。
    actions: EffectiveActions,
    /// 起点**之内**各层累积的撤销集合（`declare_head` 跳变时保留）。
    carry: BTreeSet<AtomId>,
    /// 还原点阶梯（按 seq 升序）：完整求值时按几何级数留点（近 head 密、远处稀），
    /// 逐条前推时每 `RESUME_STRIDE` 个 seq 补一个点。
    stride: VecDeque<(Seq, StateAt)>,
    incremental_steps: usize,
    rebase_steps: usize,
    full_steps: usize,
    replayed_atoms: usize,
}

impl Default for IncrementalFolder {
    fn default() -> Self {
        Self {
            cache: StateAtCache::with_capacity(RESUME_CAPACITY),
            last: None,
            origin: 0,
            actions: EffectiveActions::default(),
            carry: BTreeSet::new(),
            stride: VecDeque::new(),
            incremental_steps: 0,
            rebase_steps: 0,
            full_steps: 0,
            replayed_atoms: 0,
        }
    }
}

impl IncrementalFolder {
    /// 空增量折叠器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 折叠到 seq `n`。
    pub fn fold(&mut self, log: &AtomLog, n: Seq) -> Result<StateAt> {
        // **按值取出、按值折进去** ✓：`StateAt` 里是完整文档状态（真实 4K 画作实测
        // 316 KiB JSON、318 个对象 ✓）⇒ 每折一次深拷贝一份要 ~15ms ✗，
        // 远超过折叠本身 ⇒ 这里全程用移动而不是克隆（`fold_atoms` 本来就按值收 base ✓）。
        let Some(last) = self.last.take() else {
            return self.full_fold(log, n);
        };
        // 时间旅行（往回折）不属于"前推"，交给完整求值。
        if last.seq > n {
            self.last = Some(last);
            return self.full_fold(log, n);
        }
        if last.seq == n {
            let result = last.clone();
            self.last = Some(last);
            return Ok(result);
        }
        let last_seq = last.seq;
        let origin = last.origin_seq;
        let slice = log.range_exclusive_inclusive(last_seq, n);
        if slice.iter().any(|atom| atom.kind == AtomKind::DeclareHead) {
            self.last = Some(last);
            return self.full_fold(log, n);
        }

        // 一、把新增区的撤销动作并进有效集，得到"撤销状态真的变了"的原子。
        let changed = match self.actions.extend(slice, log, origin) {
            Some(changed) => changed,
            None => {
                // "撤销撤销"：级联长度不定 ⇒ 整表重算，取差集。
                let rebuilt =
                    EffectiveActions::from_atoms(log.range_exclusive_inclusive(origin, n));
                let changed: Vec<AtomId> = self
                    .actions
                    .suppressed()
                    .symmetric_difference(rebuilt.suppressed())
                    .cloned()
                    .collect();
                self.actions = rebuilt;
                changed
            }
        };

        // 二、撤销状态翻转 ⇒ 所有 seq ≥ 翻转点的旧状态都**作废**（它们当时把被翻转的
        //     原子折进去了）⇒ 立刻从缓存与还原点阶梯里剔除，避免拿陈旧状态当起点。
        if let Some(flip) = changed
            .iter()
            .filter_map(|id| log.get(id).map(|atom| atom.seq))
            .min()
        {
            self.cache.discard_from(flip);
            self.stride.retain(|(seq, _)| *seq < flip);
        }

        // 三、只有**已折叠区间内、且会进折叠**的原子状态翻转才需要重新折叠；
        //     `revert`/`reapply` 自身无状态效果，翻转它们不影响结果。
        let earliest = changed
            .iter()
            .filter_map(|id| log.get(id).map(|atom| (atom.seq, atom.kind)))
            .filter(|(seq, kind)| *seq <= last_seq && affects_fold(*kind))
            .map(|(seq, _)| seq)
            .min();

        let StateAt {
            state,
            warnings: base_warnings,
            ..
        } = last;

        let Some(earliest) = earliest else {
            // 前推：已折叠区间的有效集没变，只折新增区间。
            let folded = fold_atoms(state, slice);
            let mut state = folded.state;
            state.head_seq = state.head_seq.max(n);
            let result = StateAt {
                seq: n,
                origin_seq: origin,
                state,
                suppressed: self.reported_suppressed(),
                warnings: chain_warnings(&base_warnings, &folded.warnings),
            };
            self.incremental_steps += 1;
            self.replayed_atoms += slice.len();
            return Ok(self.remember(result));
        };

        // 四、续折：从"变化点之前最近的还原点"重新折叠，而不是从 seq 0。
        let Some(base) = self.resume_before(earliest, origin) else {
            return self.full_fold(log, n);
        };
        let head = log.range_exclusive_inclusive(base.seq, n);
        let StateAt {
            state: base_state,
            warnings: base_warnings,
            ..
        } = base;
        let folded = fold_atoms(base_state, head);
        let mut state = folded.state;
        state.head_seq = state.head_seq.max(n);
        let result = StateAt {
            seq: n,
            origin_seq: origin,
            state,
            suppressed: self.reported_suppressed(),
            warnings: chain_warnings(&base_warnings, &folded.warnings),
        };
        self.incremental_steps += 1;
        self.rebase_steps += 1;
        self.replayed_atoms += head.len();
        Ok(self.remember(result))
    }

    /// 折叠到当前 head。
    pub fn fold_head(&mut self, log: &AtomLog) -> Result<StateAt> {
        self.fold(log, log.head_seq())
    }

    /// 增量前推次数（含续折；即"没有从求值起点重来"的次数）。
    pub fn incremental_steps(&self) -> usize {
        self.incremental_steps
    }

    /// 其中**续折**的次数（撤销旧原子而改用还原点续折）。
    pub fn rebase_steps(&self) -> usize {
        self.rebase_steps
    }

    /// 完整求值次数。
    pub fn full_steps(&self) -> usize {
        self.full_steps
    }

    /// 累计实际折叠过的原子数（可观测性：完整求值按 `n` 计）。
    pub fn replayed_atoms(&self) -> usize {
        self.replayed_atoms
    }

    /// `state@seq` 记忆化缓存。
    pub fn cache(&self) -> &StateAtCache {
        &self.cache
    }

    /// 对外报告的撤销集合（起点内各层 + 当前起点里的有效集）。
    fn reported_suppressed(&self) -> BTreeSet<AtomId> {
        let mut suppressed = self.carry.clone();
        suppressed.extend(self.actions.suppressed().iter().cloned());
        suppressed
    }

    /// 取 `seq < upto` 中最新的还原点：先看逐 seq 的缓存，再看步长采样的常驻点。
    fn resume_before(&self, upto: Seq, origin: Seq) -> Option<StateAt> {
        let from_cache = self.cache.newest_before(upto, origin).cloned();
        let from_stride = self
            .stride
            .iter()
            .rev()
            .find(|(seq, state)| *seq < upto && state.origin_seq == origin)
            .map(|(_, state)| state.clone());
        match (from_cache, from_stride) {
            (Some(a), Some(b)) => Some(if a.seq >= b.seq { a } else { b }),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// 完整求值（与 [`FoldEngine::fold`] 同一条路），并重建有效集记账。
    fn full_fold(&mut self, log: &AtomLog, n: Seq) -> Result<StateAt> {
        self.actions = EffectiveActions::from_atoms(log.atoms_upto(n));
        // 整条区间内没有 `declare_head` ⇒ 完整求值就是"从空白折 (0, n]" ⇒ 顺手
        // 在**同一趟**里留下还原点阶梯（每多一个点只多一次深拷贝，不多扫一趟）。
        // **为什么必须顺手留**：打开 1050 原子工程后完整求值只给一个终态 ⇒ 阶梯是空的 ⇒
        // 之后第一次撤销又要从 0 重来（实测完整求值 ~103ms、续折 ~20ms）。
        let span = log.range_exclusive_inclusive(0, n);
        let result = if span.iter().any(|atom| atom.kind == AtomKind::DeclareHead) {
            let result = state_at(log, n, &mut self.cache)?;
            if result.origin_seq != self.origin {
                // 求值起点跳变 ⇒ 旧还原点不再适用。
                self.stride.clear();
                self.origin = result.origin_seq;
            }
            self.actions =
                EffectiveActions::from_atoms(log.range_exclusive_inclusive(result.origin_seq, n));
            result
        } else {
            if self.origin != 0 {
                self.stride.clear();
                self.origin = 0;
            }
            self.ladder_fold(log, n)
        };
        // 起点**之内**各层的撤销集合由 `state_at` 累积，这里把差额保留下来。
        self.carry = result
            .suppressed
            .difference(self.actions.suppressed())
            .cloned()
            .collect();
        self.full_steps += 1;
        self.replayed_atoms += n as usize;
        Ok(self.remember(result))
    }

    /// 从空白折 `(0, n]`，沿途在几何级数的 seq 上留还原点。
    ///
    /// 还原点取 `n - RESUME_STRIDE × 2^k`（k = 0,1,2…）：越靠近 head 越密
    /// ⇒"撤销刚才那几笔"总能就近续折；越远越稀 ⇒ 条目数是对数级（实测 1050 原子
    /// 只需 6 个点、约 110ms 深拷贝），不会让完整求值变慢一个数量级。
    ///
    /// 逐原子前推必须显式传有效集（[`fold_atoms_skipping`]）：`compute_suppressed`
    /// 只看得见单原子那一段，找不到被撤销的目标。
    fn ladder_fold(&mut self, log: &AtomLog, n: Seq) -> StateAt {
        let mut points: Vec<Seq> = Vec::new();
        let mut distance = RESUME_STRIDE;
        while points.len() < STRIDE_CAPACITY && distance < n {
            points.push(n - distance);
            distance *= 2;
        }
        let skip = self.actions.suppressed().clone();
        let mut state = DocumentState::empty();
        let mut warnings: Vec<FoldWarning> = Vec::new();
        let mut ladder: Vec<(Seq, StateAt)> = Vec::new();
        let mut seen: BTreeSet<AtomId> = BTreeSet::new();
        for atom in log.atoms_upto(n) {
            let folded = fold_atoms_skipping(state, std::slice::from_ref(atom), &skip);
            state = folded.state;
            warnings.extend(folded.warnings);
            if skip.contains(&atom.id) {
                seen.insert(atom.id.clone());
            }
            if points.contains(&atom.seq) {
                ladder.push((
                    atom.seq,
                    StateAt {
                        seq: atom.seq,
                        origin_seq: 0,
                        state: state.clone(),
                        suppressed: seen.clone(),
                        warnings: warnings.clone(),
                    },
                ));
            }
        }
        state.head_seq = state.head_seq.max(n);
        for entry in ladder {
            if !self.stride.iter().any(|(seq, _)| *seq == entry.0) {
                self.stride.push_back(entry);
            }
        }
        self.stride.make_contiguous().sort_by_key(|(seq, _)| *seq);
        while self.stride.len() > STRIDE_CAPACITY {
            self.stride.pop_front();
        }
        StateAt {
            seq: n,
            origin_seq: 0,
            state,
            suppressed: skip,
            warnings,
        }
    }

    /// 记录一次求值结果：更新上一次状态与还原点阶梯。
    ///
    /// **故意不往 [`StateAtCache`] 里塞** ✓：真实 4K 画作的 `StateAt` 深拷贝约 ~15ms ✓，
    /// 而"逐 seq 的密集还原点"每折一次就要多拷一份 ✗ ⇒ 实测总耗时反而上升 ✓。
    /// 还原点改为**按步长采样**（`RESUME_STRIDE`）✓：既保住"撤销旧原子时不必从 0 重来"✓，
    /// 又把每折一次的深拷贝压到 **1 份**（只给 `self.last` ✓）。
    fn remember(&mut self, result: StateAt) -> StateAt {
        if result.seq % RESUME_STRIDE == 0 {
            self.stride.push_back((result.seq, result.clone()));
            while self.stride.len() > STRIDE_CAPACITY {
                self.stride.pop_front();
            }
        }
        self.last = Some(result.clone());
        result
    }
}

/// 该原子种类的撤销状态翻转会不会改变折叠结果。
///
/// `revert`/`reapply` 自身没有状态效果（设计文档 5.3）：它们只改有效集，
/// 而被它们改动的目标原子会单独出现在变化集合里。
fn affects_fold(kind: AtomKind) -> bool {
    !matches!(kind, AtomKind::Revert | AtomKind::Reapply)
}

/// 拼接两段警告：还原点之前 + 之后，与"一次折完"的顺序一致。
fn chain_warnings(base: &[FoldWarning], extra: &[FoldWarning]) -> Vec<FoldWarning> {
    let mut warnings = Vec::with_capacity(base.len() + extra.len());
    warnings.extend(base.iter().cloned());
    warnings.extend(extra.iter().cloned());
    warnings
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
    fn state_at_cache_discard_from_drops_stale_resume_points() {
        let log = chain();
        let mut cache = StateAtCache::new();
        for n in 1..=3 {
            state_at(&log, n, &mut cache).unwrap();
        }
        assert_eq!(cache.get(2).map(|state| state.seq), Some(2));
        cache.discard_from(2);
        assert!(cache.get(1).is_some(), "seq < from 的条目必须保留");
        assert!(cache.get(2).is_none(), "seq ≥ from 的条目必须作废");
        assert!(cache.get(3).is_none());
        // 作废之后仍能正常写入与淘汰。
        state_at(&log, 3, &mut cache).unwrap();
        assert_eq!(cache.get(3).map(|state| state.seq), Some(3));
        assert!(cache.newest_before(3, 0).is_some());
        assert!(cache.newest_before(1, 0).is_none());
    }

    /// `state@seq` 的结果**不得依赖缓存里恰好有什么**（缓存是旁路，不是语义）。
    ///
    /// 这条判据抓到过 `state_at` 的两个真问题（都在本轮修掉）：
    /// ① `declare_head` 链长 > 1 时，`origin_seq` 取的是"最内层"还是"最外层"，
    ///    取决于阶段一是否在链中间命中缓存（实测同一份日志 32 vs 10）；
    /// ② 链上每一层写回缓存时存的是**本层局部**的有效集/警告，而复用时按**累积**解读
    ///    ⇒ 命中缓存就少报内层的撤销（实测同一份日志 `{}` vs `{"…"}`）。
    #[test]
    fn state_at_is_independent_of_cache_contents() {
        let mut log = chain();
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj2",
                json!({"object_id": "obj_2", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj3",
                json!({"object_id": "obj_3", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        // 撤销落在"链底尾巴"里：它给尾巴那一层带来一个非空有效集。
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_early", json!({"target": "a_obj2"})),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj4",
                json!({"object_id": "obj_4", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        let engine = FoldEngine::new();
        // DH1 的 base 落在 seq 7 ⇒ 链底尾巴是 (0, 7]，包含上面那条 revert。
        push(
            &mut log,
            engine
                .revert_to_atom("human:1", "session:a", "a_obj4")
                .with_id("a_head1"),
        );
        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super4",
                json!({"object_id": "obj_4", "data": {"v": 2}}),
            ),
        );
        // DH2 的 base = `a_super4`（seq 9）⇒ 它是链上的一个"内层 upto"。
        push(
            &mut log,
            engine
                .revert_to_atom("human:1", "session:a", "a_super4")
                .with_id("a_head2"),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj5",
                json!({"object_id": "obj_5", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_outer", json!({"target": "a_obj5"})),
        );
        let head = log.head_seq();
        assert!(head > 10);

        // 参考：全新缓存，一次算到底。
        let fresh = state_at(&log, head, &mut StateAtCache::new()).unwrap();
        assert!(fresh.origin_seq > 0, "这条日志应当有 declare_head 跳变");
        assert_eq!(fresh.suppressed.len(), 2, "尾巴一条 + 最外层一条");

        // 预热：**只**算 head-1 —— 它会在链上留下一个"内层 upto"条目，
        // 且不会用一次"以该 seq 为 n"的完整求值把它覆盖掉。
        let mut primed = StateAtCache::new();
        let _ = state_at(&log, head - 1, &mut primed).unwrap();
        let after_prime = state_at(&log, head, &mut primed).unwrap();
        assert!(
            after_prime == fresh,
            "缓存内容改变了 state@seq 的结果：origin {} vs {}，suppressed {} vs {}",
            after_prime.origin_seq,
            fresh.origin_seq,
            after_prime.suppressed.len(),
            fresh.suppressed.len()
        );

        // 正序 / 反序预热每个前缀也必须一致（覆盖各种命中位置）。
        for order in [false, true] {
            let mut cache = StateAtCache::new();
            let ns: Vec<Seq> = if order {
                (1..head).rev().collect()
            } else {
                (1..head).collect()
            };
            for n in ns {
                let _ = state_at(&log, n, &mut cache).unwrap();
            }
            assert!(
                state_at(&log, head, &mut cache).unwrap() == fresh,
                "按 {} 序预热后结果变了",
                if order { "反" } else { "正" }
            );
        }
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
        assert_eq!(first.state.layers.len(), 2);
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

    /// 一段"真实形状"的历史：`objects` 个对象，外加若干指向旧原子的撤销。
    ///
    /// 与真实 4K 画作同形（`import_image` 建对象 + 成批 `revert` 撤销较早的对象），
    /// 只是对象更小、便于单测秒级跑完。
    fn history_with_reverts(objects: usize) -> AtomLog {
        let mut log = chain();
        let mut created: Vec<String> = Vec::new();
        for i in 0..objects {
            let id = format!("a_obj_{i}");
            push(
                &mut log,
                atom(
                    AtomKind::CreateObject,
                    &id,
                    json!({"object_id": format!("obj_{i}"), "layer_id": "layer_1", "type": "stroke"}),
                ),
            );
            created.push(id);
            if i >= 8 && i % 8 == 0 {
                // 撤销 3 个原子之前建的对象（和真实工作流的"撤销刚才那几笔"同形）。
                let target = created[created.len() - 3].clone();
                push(
                    &mut log,
                    atom(
                        AtomKind::Revert,
                        &format!("a_rev_{i}"),
                        json!({"target": target}),
                    ),
                );
            }
        }
        log
    }

    /// **判据（可红 ✓）**：日志里含 `revert`，在它上面继续追加提交（尤其是继续撤销旧原子）
    /// 必须走**增量前推／续折**，不得再退回 [`state_at`] 的完整求值。
    ///
    /// 变异验证：把 `IncrementalFolder::fold` 里"找不到还原点就续折"的那一支改回
    /// `can_extend` 式的"见到 `revert` 指向旧区间就返回 false"，本测试立刻红在
    /// `full_steps() == 1` 上（实测 `full_steps` 从 1 变成 9）。
    #[test]
    fn appending_reverts_never_falls_back_to_a_full_replay() {
        let mut log = history_with_reverts(120);
        let mut folder = IncrementalFolder::new();
        // 打开文档：一次完整求值（这一次全量是应该的）。
        folder.fold_head(&log).unwrap();
        assert_eq!(folder.full_steps(), 1, "打开只应完整求值一次");
        let base_incremental = folder.incremental_steps();
        // 取"已折叠区间里最靠后的若干个**仍然有效**的建对象原子"当撤销目标：
        // 既落在旧区间（旧实现据此判 false ⇒ 全量重放），又不是 revert/reapply 本身。
        let suppressed = EffectiveActions::from_atoms(log.atoms())
            .suppressed()
            .clone();
        let targets: Vec<String> = log
            .iter()
            .rev()
            .filter(|a| a.kind == AtomKind::CreateObject && !suppressed.contains(&a.id))
            .take(9)
            .map(|a| a.id.clone())
            .collect();
        assert_eq!(targets.len(), 9);

        let mut reverts = 0usize;
        for (i, target) in targets.iter().enumerate() {
            push(
                &mut log,
                atom(
                    AtomKind::Revert,
                    &format!("a_late_revert_{i}"),
                    json!({"target": target}),
                ),
            );
            reverts += 1;
            let incremental = folder.fold_head(&log).unwrap();
            let full = state_at(&log, log.head_seq(), &mut StateAtCache::new()).unwrap();
            assert!(
                incremental == full,
                "第 {i} 次撤销后必须与完整求值逐字段一致（state/suppressed/warnings）"
            );
        }

        assert_eq!(
            folder.full_steps(),
            1,
            "撤销旧原子不得退回完整求值（判定条件被改回去这里就会变红）"
        );
        assert_eq!(folder.incremental_steps(), base_incremental + reverts);
        assert_eq!(
            folder.rebase_steps(),
            reverts,
            "撤销旧原子走的是『从还原点续折』这一支"
        );
    }

    /// 等效性：交错的 `revert` / `reapply` / 撤销-撤销 下，**每一个前缀**的增量结果都必须
    /// 与 [`state_at`] 的完整求值逐字段一致（状态 ＋ 有效集 ＋ 警告）。
    #[test]
    fn incremental_fold_equals_full_replay_across_reverts() {
        let mut log = chain();
        // 对象 A：先建，再撤销，再恢复，再撤销。
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj_a",
                json!({"object_id": "obj_a", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_a1", json!({"target": "a_obj_a"})),
        );
        push(
            &mut log,
            atom(AtomKind::Reapply, "a_re_a1", json!({"target": "a_obj_a"})),
        );
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_a2", json!({"target": "a_obj_a"})),
        );
        // 对象 B：创建后被 supersede，再撤销那次 supersede。
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj_b",
                json!({"object_id": "obj_b", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super_b",
                json!({"object_id": "obj_b", "data": {"v": 2}}),
            ),
        );
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_b", json!({"target": "a_super_b"})),
        );
        // 撤销-撤销：`revert(revert(x)) ≡ reapply(x)`（这条走"整表重算"那一支）。
        push(
            &mut log,
            atom(
                AtomKind::Revert,
                "a_rev_a1_again",
                json!({"target": "a_rev_a1"}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::CreateObject,
                "a_obj_c",
                json!({"object_id": "obj_c", "layer_id": "layer_1", "type": "shape"}),
            ),
        );
        push(
            &mut log,
            atom(AtomKind::Reapply, "a_re_c", json!({"target": "a_obj_c"})),
        );
        push(
            &mut log,
            atom(
                AtomKind::Supersede,
                "a_super_a",
                json!({"object_id": "obj_a", "data": {"v": 3}}),
            ),
        );
        push(
            &mut log,
            atom(
                AtomKind::Revert,
                "a_rev_super_a",
                json!({"target": "a_super_a"}),
            ),
        );

        let mut folder = IncrementalFolder::new();
        let mut chunks = 0usize;
        for upto in 1..=log.head_seq() {
            let incremental = folder.fold(&log, upto).unwrap();
            let full = state_at(&log, upto, &mut StateAtCache::new()).unwrap();
            assert!(
                incremental == full,
                "seq {upto} 处增量折叠与完整求值分歧（state/suppressed/warnings）"
            );
            chunks += 1;
        }
        assert_eq!(chunks as u64, log.head_seq());
        let head = folder.fold_head(&log).unwrap();
        assert_eq!(
            head,
            state_at(&log, log.head_seq(), &mut StateAtCache::new()).unwrap()
        );
    }

    /// 还原点在"撤销状态变化"之后必须作废：否则会拿一份**陈旧**的完整状态当续折起点。
    ///
    /// 两步走才抓得住：① 撤销一个**很靠前**的原子（翻转点很小 ⇒ 所有起点 ≥ 它的旧状态
    /// 必须作废）；② 再撤销一个**中间**的原子 —— 这时若①没有作废那些旧状态，
    /// 就会拿"撤销之前"折出来的中间状态当起点，状态悄悄偏离完整求值。
    ///
    /// 变异验证：注释掉 `IncrementalFolder::fold` 里的 `self.cache.discard_from(flip)` 与
    /// `self.stride.retain(...)`，本测试当场红（实测红在 `doc_id` 仍是 `Some("doc_1")`）。
    #[test]
    fn stale_resume_points_are_dropped_when_a_revert_lands() {
        let mut log = chain();
        let mut folder = IncrementalFolder::new();
        folder.fold_head(&log).unwrap();
        // 一路前推，把还原点阶梯铺起来。
        for i in 0..80 {
            push(
                &mut log,
                atom(
                    AtomKind::CreateObject,
                    &format!("a_fill_{i}"),
                    json!({"object_id": format!("obj_fill_{i}"), "layer_id": "layer_1", "type": "stroke"}),
                ),
            );
            folder.fold_head(&log).unwrap();
        }
        // ① 撤销 seq 最小的那个原子（`create_document`）⇒ 起点 ≥ 它的旧状态全部作废。
        let doc_id = log.by_seq(1).unwrap().id.clone();
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_doc", json!({"target": doc_id})),
        );
        let after_first = folder.fold_head(&log).unwrap();
        assert!(
            after_first == state_at(&log, log.head_seq(), &mut StateAtCache::new()).unwrap(),
            "① 之后就必须与完整求值一致"
        );

        // ② 再撤销一个**中间**的原子：这里若还留着①之前折出来的中间状态就会错。
        let middle = log
            .iter()
            .find(|a| a.kind == AtomKind::CreateObject && a.seq > 40)
            .expect("中间应有一个建对象原子")
            .id
            .clone();
        push(
            &mut log,
            atom(AtomKind::Revert, "a_rev_middle", json!({"target": middle})),
        );
        let incremental = folder.fold_head(&log).unwrap();
        let full = state_at(&log, log.head_seq(), &mut StateAtCache::new()).unwrap();
        assert!(
            incremental == full,
            "陈旧还原点会让状态悄悄偏离完整求值：doc_id={:?}（应已被撤销）",
            incremental.state.doc_id
        );
        assert_eq!(
            incremental.state.doc_id, None,
            "① 撤销的 create_document 不能复活"
        );
    }
}
