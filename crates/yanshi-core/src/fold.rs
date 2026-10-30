//! 折叠求值（设计文档 5.3 / 5.4）。
//!
//! 折叠是把原子日志按 `seq` 线性扫描、维护每个对象有效原子链、生成当前状态的过程：
//!
//! ```text
//! fold(atoms, base_state):
//!   state = base_state
//!   for atom in atoms sorted by seq:
//!     if ∃ 有效 revert 原子 r: r.target == atom.id and r.seq > atom.seq:
//!       continue
//!     if atom.precondition not satisfied(state):
//!       record_warning(atom, "cascade_invalidation")
//!       continue
//!     state = apply(state, atom)
//!   return state
//! ```
//!
//! 进入日志的原子均已通过提交时校验（12.2），因此**折叠期 precondition 失败一律来自
//! 级联失效**：记录警告后跳过，不产生状态效果。折叠器不持有“提交期校验状态”信息。
//!
//! ## 有效集
//!
//! 一个原子是否被撤销，取决于**比它晚**的 revert/reapply：
//!
//! - 把 revert/reapply 视作“动作”，按 seq 从大到小扫描，只记录**有效动作**。
//! - `revert(x)` 若被更晚的有效 `revert` 指向，则自身失效——即 `revert(revert(x)) ≡ reapply(x)`。
//! - `reapply` 不能被 revert（不在 5.4 的合法目标类别中），因此永远有效。
//! - 目标 x 只被**seq 最大**的那个有效动作决定：是 revert 则失效，是 reapply 则生效。
//! - `reapply` 只恢复目标原子，**不恢复级联链**。

use crate::atom::{payload_bool, payload_f64, payload_str, payload_u64, Atom, AtomId, AtomKind};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::Seq;
use crate::log::AtomLog;
use crate::state::{
    Checkpoint, DeclareHead, DocumentState, HeadBase, Layer, LayerType, MaskPatchMeta, MaskPatchOp,
    Object, ObjectType, Selection, Style, Transform,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// 折叠警告类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningKind {
    /// 级联失效：该原子的依赖已被 revert，跳过且无状态效果（设计文档 5.3）。
    CascadeInvalidation,
    /// 其他 precondition 失败（如 `create_document` 重复、`create_layer` 重名）。
    PreconditionFailed,
    /// `declare_head` 出现在被折叠区间内：其语义需由 `state@seq` 公式处理（设计文档 5.5）。
    DeclareHeadOutsideFormula,
}

/// **折叠警告滚动上限**（P-8）：累积警告向量只保留最近 [`MAX_FOLD_WARNINGS`] 条。
///
/// 此前警告向量无限累积：增量折叠每折一次就把历史警告全拷一份
/// （`chain_warnings`），提交回执每笔又全量序列化一次
/// （`warnings_to_json`）⇒ 每笔成本随 N 线性增长。滚动上限把
/// 两处都钉死在 O(上限)；回执只丢最旧的警告，最新警告总在。
/// 上限必须在**所有**累积路径上一致施加（`state_at`、`ladder_fold`、
/// `chain_warnings`），否则增量折叠与冷重放的警告会分歧（测试钉住）。
pub const MAX_FOLD_WARNINGS: usize = 128;

/// 折叠警告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldWarning {
    /// 触发警告的原子。
    pub atom_id: AtomId,
    /// 原子 seq。
    pub seq: Seq,
    /// 警告类型。
    pub kind: WarningKind,
    /// 细节。
    pub detail: String,
}

/// 折叠结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FoldResult {
    /// 折叠后的状态。
    pub state: DocumentState,
    /// 警告集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<FoldWarning>,
    /// 被有效 revert 撤销的原子。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub suppressed: BTreeSet<AtomId>,
    /// 实际产生状态效果的原子（按 seq 升序）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<AtomId>,
}

impl FoldResult {
    /// 该原子是否被 revert 撤销。
    pub fn is_suppressed(&self, atom_id: &str) -> bool {
        self.suppressed.contains(atom_id)
    }

    /// 是否无警告。
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }

    /// 指定警告类型的条数。
    pub fn warning_count(&self, kind: WarningKind) -> usize {
        self.warnings.iter().filter(|w| w.kind == kind).count()
    }
}

/// 折叠引擎：无状态，配置项极少，便于客户端与服务端共享同一实现。
#[derive(Debug, Clone, Copy, Default)]
pub struct FoldEngine {
    /// 重放上限（设计文档 6.2：普通原子重放上限 1000）。0 表示不限制。
    pub max_replay: usize,
}

impl FoldEngine {
    /// 默认引擎（不限制重放长度；上限由 `state@seq` 层按快照策略施加）。
    pub fn new() -> Self {
        Self { max_replay: 0 }
    }

    /// 设置重放上限。
    pub fn with_max_replay(mut self, max_replay: usize) -> Self {
        self.max_replay = max_replay;
        self
    }

    /// 在 `base` 之上折叠一段原子（须按 seq 升序，且区间内不含 `declare_head`）。
    pub fn fold_slice(&self, base: DocumentState, atoms: &[Atom]) -> FoldResult {
        fold_atoms(base, atoms)
    }
}

/// **E2 段折叠的落点说明**：段原语就是 [`crate::seq::IncrementalFolder::fold`]
/// —— 每段调一次、而不是每原子调一次。
///
/// 为什么不另造一个 `fold_segment(base, atoms)`：`fold_slice` 只看得见段内原子，
/// 段内 `revert` 指向段首之前的目标时，它算不出"目标的效果要从 base 里去掉"
/// （`compute_suppressed` 的视野只有本段）；而 `IncrementalFolder::fold`
/// 本来就有三条路——前推／续折（`resume_before`）／完整求值——且不变量是
/// "无论走哪条路都与 `state_at` 完整求值逐字段一致"，段折叠直接复用它，
/// 跨段 revert 走续折、段内 `declare_head` 走完整求值，**同一事实只写一处**。
///
/// 段折叠的脏区并集不在这里算：它由调用方（`Kernel`／`Document`）对段内原子
/// 逐个 `plan_dirty_with_log(末态, Some(段首态), …)` 再并起来（见下）。
/// 段首态只保留最新一份（滚动 checkpoint），不随段数增长。
///
/// **但有一类段不能走段折叠**：见 [`segment_needs_atomic_path`]。
/// 该原子种类的提交/脏区需要"提交前的状态"（与服务端提交路径同一张表）。
///
/// 与 `crates/yanshi-server/src/document.rs` 的 `needs_previous_state` 是
/// **同一张表**——那边的本地函数已改为调这里，表只写一处。
pub fn atom_needs_previous_state(kind: AtomKind) -> bool {
    matches!(
        kind,
        AtomKind::Tombstone
            | AtomKind::Supersede
            | AtomKind::Move
            | AtomKind::Transform
            | AtomKind::SetProperty
            | AtomKind::Revert
            | AtomKind::Reapply
            | AtomKind::ReorderLayers
            | AtomKind::ReorderObjects
            | AtomKind::BakeLayer
    )
}

/// **该段是否必须走逐原子路径**（E2 回退条件）：段内含 `declare_head`，
/// 或含任一需要"提交前状态"的结构类原子（[`atom_needs_previous_state`]）
/// 时，段折叠的保守脏区并集不再可信——段内一次差分可能漏掉中间态才脏的
/// tile（`move` 的旧位置、被删对象的旧包围盒都只在逐原子差分里出现）
/// ⇒ 整段回退逐原子 `apply`／`commit`，正确性与逐原子路径完全相同。
pub fn segment_needs_atomic_path(atoms: &[Atom]) -> bool {
    atoms
        .iter()
        .any(|atom| atom.kind == AtomKind::DeclareHead || atom_needs_previous_state(atom.kind))
}

/// 计算被有效 revert 撤销的原子集合（设计文档 5.3 有效集）。
pub fn compute_suppressed(atoms: &[Atom]) -> BTreeSet<AtomId> {
    EffectiveActions::from_atoms(atoms).suppressed().clone()
}

/// 有效集里的一个动作（设计文档 5.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertAction {
    /// `revert`：目标原子失效。
    Revert,
    /// `reapply`：目标原子恢复生效。
    Reapply,
}

/// **可增量前推**的有效动作表（设计文档 5.3 有效集）。
///
/// 语义与 [`compute_suppressed`] **逐位一致**（后者现在就是它的薄封装），
/// 但它额外保留"目标原子 → seq 最大的生效动作"这张表，因此日志**尾部追加**
/// 时只需处理新增区间：新增原子的 `seq` 严格更大，不可能被既有动作指向。
///
/// 为什么需要它：`IncrementalFolder` 原先一遇到"`revert` 指向旧区间"就退回
/// **从 0 全量重放**（见 `seq` 模块的历史注释）。有了这张表，前推就能先算出
/// "到底哪些原子的撤销状态真的变了"，只在**变化确实影响折叠结果**时才回退，
/// 并且回退也能从还原点续折而不是从 0 重来。
///
/// 唯一的例外是"撤销撤销"（动作的目标本身是 `revert`/`reapply`）：那时级联长度不定
/// （`revert(revert(x)) ≡ reapply(x)` 会继续影响 x 的目标），[`Self::extend`] 返回
/// `None`，调用方整表重算即可 —— 这条路径正确但与增量无关。
#[derive(Debug, Default, Clone)]
pub struct EffectiveActions {
    /// 目标原子 → (seq 最大的生效动作的 seq, 动作)。
    actions: BTreeMap<AtomId, (Seq, RevertAction)>,
    /// 被有效 revert 撤销的原子。
    suppressed: BTreeSet<AtomId>,
}

impl EffectiveActions {
    /// 整表重算（与 [`compute_suppressed`] 同一趟倒序扫描）。
    pub fn from_atoms(atoms: &[Atom]) -> Self {
        let mut effective = Self::default();
        for atom in atoms.iter().rev() {
            match atom.kind {
                AtomKind::Revert => {
                    // revert 自身被更晚的有效 revert 撤销时失效（revert(revert(x)) ≡ reapply(x)）。
                    let revert_of_revert = matches!(
                        effective.actions.get(&atom.id),
                        Some((_, RevertAction::Revert))
                    );
                    if revert_of_revert {
                        effective.suppressed.insert(atom.id.clone());
                    } else if let Some(target) = atom.target_atom() {
                        effective
                            .actions
                            .entry(target.to_owned())
                            .or_insert((atom.seq, RevertAction::Revert));
                    }
                }
                AtomKind::Reapply => {
                    // **reapply 自己也可能被 revert** ✓（提交层 5.4 已放开
                    // `revert` 的目标可以是 `Reapply` ✓，见 `log.rs` 的 `revertible` ✓）：
                    // 那时这条 reapply 自身失效，**不**把目标改回生效 ✓，
                    // 更早的 `Revert` 重新胜出 ⇒ `revert(reapply(x)) ≡ 维持 x 被抑制` ✓。
                    // （第 1440 轮审计查明：这里原来是**无条件登记** ⇒ `draw→revert→reapply
                    // →revert(reapply)` 折完笔画**仍生效** ✗，undo/redo 不对称 ✓。）
                    let revert_of_reapply = matches!(
                        effective.actions.get(&atom.id),
                        Some((_, RevertAction::Revert))
                    );
                    if revert_of_reapply {
                        effective.suppressed.insert(atom.id.clone());
                    } else if let Some(target) = atom.target_atom() {
                        effective
                            .actions
                            .entry(target.to_owned())
                            .or_insert((atom.seq, RevertAction::Reapply));
                    }
                }
                _ => {
                    if matches!(
                        effective.actions.get(&atom.id),
                        Some((_, RevertAction::Revert))
                    ) {
                        effective.suppressed.insert(atom.id.clone());
                    }
                }
            }
        }
        effective
    }

    /// 在尾部追加一批 `seq` 严格更大的原子；返回**撤销状态发生变化**的原子 id。
    ///
    /// `origin` 是当前求值起点：只有位于 `origin` 之后的原子才可能被撤销
    /// （模块级约定：`revert`/`reapply` 的目标必须晚于求值起点），因此这里只把
    /// 该区间内的目标记进有效集 —— 与 [`Self::from_atoms`] 作用在同一区间时等价。
    ///
    /// 返回 `None` 表示这批原子里出现了"撤销撤销"——它的目标是 `revert`/`reapply`
    /// 本身，级联可能任意长，调用方应改用 [`Self::from_atoms`] 整表重算。
    pub fn extend(&mut self, atoms: &[Atom], log: &AtomLog, origin: Seq) -> Option<Vec<AtomId>> {
        // 先把这批动作筛成 `(目标, seq, 动作)`；顺带做"撤销撤销"体检。
        let mut pending: Vec<(&str, Seq, RevertAction)> = Vec::new();
        for atom in atoms {
            if !matches!(atom.kind, AtomKind::Revert | AtomKind::Reapply) {
                continue;
            }
            let Some(target) = atom.target_atom() else {
                continue;
            };
            match log.get(target) {
                // "撤销撤销"：目标本身是历史动作 ⇒ 目标自身的注册状态会级联变化。
                Some(found) if matches!(found.kind, AtomKind::Revert | AtomKind::Reapply) => {
                    return None;
                }
                // 跨起点撤销（提交期已被拒绝）⇒ 不影响有效集。
                Some(found) if found.seq <= origin => continue,
                // 目标不在日志里（非法日志）⇒ 同样不影响有效集。
                Some(_) => {}
                None => continue,
            }
            let action = if atom.kind == AtomKind::Revert {
                RevertAction::Revert
            } else {
                RevertAction::Reapply
            };
            pending.push((target, atom.seq, action));
        }

        // "目标 → seq 最大者胜出"：倒序登记（先到先得），与 `compute_suppressed` 同一套次序。
        pending.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        let mut changed: Vec<AtomId> = Vec::new();
        for (target, seq, action) in pending {
            if self.set_action(target, seq, action) {
                changed.push(target.to_owned());
            }
        }
        Some(changed)
    }

    /// 当前被有效 revert 撤销的原子集合。
    pub fn suppressed(&self) -> &BTreeSet<AtomId> {
        &self.suppressed
    }

    /// 指定原子当前的最新生效动作（没有则为 `None`）。
    pub fn action_of(&self, atom_id: &str) -> Option<RevertAction> {
        self.actions.get(atom_id).map(|(_, action)| *action)
    }

    /// 登记一个动作（seq 更大者胜出），返回目标的撤销状态是否因此翻转。
    fn set_action(&mut self, target: &str, seq: Seq, action: RevertAction) -> bool {
        if let Some((existing, _)) = self.actions.get(target) {
            if *existing > seq {
                return false; // 已经有一个更晚的动作胜出。
            }
        }
        self.actions.insert(target.to_owned(), (seq, action));
        let suppressed = action == RevertAction::Revert;
        let was = self.suppressed.contains(target);
        if suppressed == was {
            return false;
        }
        if suppressed {
            self.suppressed.insert(target.to_owned());
        } else {
            self.suppressed.remove(target);
        }
        true
    }
}

/// 折叠一段原子：`base` 为起点状态，`atoms` 必须按 seq 升序。
///
/// 区间内若出现 `declare_head`，按设计文档 5.5 它应由 `state@seq` 公式处理，
/// 本函数只记录警告（见 [`crate::seq`]）。
pub fn fold_atoms(base: DocumentState, atoms: &[Atom]) -> FoldResult {
    let suppressed = compute_suppressed(atoms);
    fold_atoms_skipping(base, atoms, &suppressed)
}

/// 折叠一段原子，但**跳过调用方给出的撤销集合**。
///
/// 与 [`fold_atoms`] 的唯一区别是有效集由外面传进来：`compute_suppressed(atoms)`
/// 只看得见这一段，而调用方（增量折叠器）持有**整条历史**的有效集。
/// 逐原子前推时这一点是必需的 —— 否则 `revert` 找不到它撤销的目标（目标不在这一段里）。
///
/// `FoldResult::suppressed` 返回"传入集合 ∩ 这段原子"。
pub fn fold_atoms_skipping(
    base: DocumentState,
    atoms: &[Atom],
    suppressed: &BTreeSet<AtomId>,
) -> FoldResult {
    let suppressed: BTreeSet<AtomId> = atoms
        .iter()
        .filter(|atom| suppressed.contains(&atom.id))
        .map(|atom| atom.id.clone())
        .collect();
    let mut state = base;
    let mut warnings = Vec::new();
    let mut applied = Vec::new();
    // Fold Horizon（bake）：有效 BakeLayer 原子的 layer -> max up_to_seq。
    // 段内预扫描只用本段原子；增量段从已含压平态的 base 续折时，段内
    // 原子 seq 恒大于 base 的水位，不会误剪（base 侧水位由状态自带）。
    let horizon = crate::bake::compute_bake_horizon(atoms, &suppressed);
    // 对象 -> 图层映射：先取 base 状态，再随段内创建原子顺序更新。
    // 被剪枝的创建原子也要登记归属，否则其后续 Move/Transform 无法判定剪枝。
    let mut object_layer: std::collections::BTreeMap<String, String> = state
        .objects
        .values()
        .map(|o| (o.id.clone(), o.layer_id.clone()))
        .collect();

    for atom in atoms {
        if suppressed.contains(&atom.id) {
            continue;
        }
        // Horizon 剪枝（BakeLayer 本身与外壳类原子永不剪枝）。
        if atom.kind != AtomKind::BakeLayer && !crate::bake::is_bake_shell_kind(atom.kind) {
            let target = crate::bake::bake_target_layer(atom, |id| object_layer.get(id).cloned());
            if let Some(layer_id) = target {
                if let Some(water) = horizon.get(&layer_id) {
                    if atom.seq <= *water && crate::bake::is_bake_content_kind(atom.kind) {
                        // 仍登记创建归属（见上），但不产生状态效果、不进 applied。
                        if matches!(
                            atom.kind,
                            AtomKind::CreateObject
                                | AtomKind::ImportImage
                                | AtomKind::DrawStroke
                                | AtomKind::DrawShape
                                | AtomKind::DrawText
                                | AtomKind::Fill
                                | AtomKind::Erase
                                | AtomKind::Retouch
                                | AtomKind::Liquify
                                | AtomKind::Supersede
                        ) {
                            if let (Some(oid), Some(lid)) = (
                                atom.payload.get("object_id").and_then(|v| v.as_str()),
                                atom.payload.get("layer_id").and_then(|v| v.as_str()),
                            ) {
                                object_layer.insert(oid.to_owned(), lid.to_owned());
                            }
                        }
                        continue;
                    }
                }
            }
        }
        // 登记本原子的创建归属（供后续原子判定剪枝）。
        if matches!(
            atom.kind,
            AtomKind::CreateObject
                | AtomKind::ImportImage
                | AtomKind::DrawStroke
                | AtomKind::DrawShape
                | AtomKind::DrawText
                | AtomKind::Fill
                | AtomKind::Erase
                | AtomKind::Retouch
                | AtomKind::Liquify
                | AtomKind::Supersede
        ) {
            if let (Some(oid), Some(lid)) = (
                atom.payload.get("object_id").and_then(|v| v.as_str()),
                atom.payload.get("layer_id").and_then(|v| v.as_str()),
            ) {
                object_layer.insert(oid.to_owned(), lid.to_owned());
            }
        }
        if atom.kind == AtomKind::DeclareHead {
            warnings.push(FoldWarning {
                atom_id: atom.id.clone(),
                seq: atom.seq,
                kind: WarningKind::DeclareHeadOutsideFormula,
                detail: "declare_head 需按 state@seq 公式求值（5.5）".to_owned(),
            });
            continue;
        }
        match precondition(&state, atom) {
            Ok(()) => match apply(&mut state, atom) {
                Ok(()) => {
                    applied.push(atom.id.clone());
                }
                Err(error) => warnings.push(cascade_warning(atom, &error)),
            },
            Err(error) => warnings.push(cascade_warning(atom, &error)),
        }
    }
    // head 指针 = 本次折叠区间内 seq 最大的原子（无论是否产生状态效果）：
    // 撤销、协作原子同样推进 head，文档的 head 是“历史位置”而非“最后一次有效修改”。
    if let Some(last) = atoms.last() {
        state.head_seq = state.head_seq.max(last.seq);
        state.head_atom = Some(last.id.clone());
    }
    FoldResult {
        state,
        warnings,
        suppressed,
        applied,
    }
}

fn cascade_warning(atom: &Atom, error: &YanshiError) -> FoldWarning {
    FoldWarning {
        atom_id: atom.id.clone(),
        seq: atom.seq,
        kind: match error.code {
            ErrorCode::PreconditionFailed | ErrorCode::ReferenceNotFound => {
                WarningKind::CascadeInvalidation
            }
            _ => WarningKind::PreconditionFailed,
        },
        detail: error
            .context
            .detail
            .clone()
            .unwrap_or_else(|| error.to_string()),
    }
}

fn err(code: ErrorCode, detail: impl Into<String>) -> YanshiError {
    YanshiError::new(code, ErrorContext::detail(detail))
}

fn required<'a>(atom: &'a Atom, key: &str) -> Result<&'a str> {
    payload_str(&atom.payload, key).ok_or_else(|| {
        err(
            ErrorCode::InvalidArgument,
            format!("{} 缺少字段 {key}", atom.kind),
        )
        .with_atom(atom.id.clone())
    })
}

fn tombstone_error(kind: &str, id: &str) -> YanshiError {
    err(
        ErrorCode::PreconditionFailed,
        format!("{kind} {id} 已被 tombstone"),
    )
}

/// **"不存在"必须说清"有哪些"** ✓（真实用户 §五-18 的原话：
/// "图层不存在时返回 reference_not_found 但不说可用图层有哪些" ✓）。
///
/// **为什么值得** ✓：调用方（尤其 agent ✓）拿到 `{kind} {id} 不存在` 只能**猜** ✓
/// ⇒ 要么再调一次列表接口 ✓、要么乱试几个 id ✗。把可用值直接放进报错 ✓ ⇒ **一次就能自纠** ✓。
///
/// **上限 8 个** ✓：长文档可能有几百个图层 ✓ ⇒ 报错**不能变成一屏** ✗
///（那会让真正的错因被淹没 ✓ —— 本轮刚在一条打印整幅像素的测试上吃过这个亏 ✓）；
/// 超出就省略号收尾 ✓。**空集合单独说** ✓：`（当前没有任何图层）` 比列一串空值有用得多 ✓。
fn missing_error<'a, I>(kind: &str, id: &str, available: I) -> YanshiError
where
    I: IntoIterator<Item = &'a String>,
{
    // **一处实现、两处调用** ✓（折叠期与 HEAD 检查 ✓）⇒ 规则不会漂移 ✓。
    crate::error::missing_reference(kind, id, available)
}

/// 折叠期 precondition 检查：只依赖当前状态（设计文档 5.3）。
pub fn precondition(state: &DocumentState, atom: &Atom) -> Result<()> {
    match atom.kind {
        AtomKind::CreateDocument => {
            if state.doc_id.is_some() {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    "文档已存在，重复 create_document",
                ));
            }
        }
        AtomKind::CreateLayer => {
            let layer_id = required(atom, "layer_id")?;
            // **只与"存活"的图层冲突** ✓ —— 折叠层保留**墓碑** ✓（`deleted_by` ✓，删除可撤销的基础 ✓），
            // 但墓碑**不该占用 id** ✗：子 agent 实测 删掉 `L_sky` 后再 `create_layer L_sky` 会报
            // "图层 L_sky 已存在" ✗，而对同一个 id 落笔又报"图层 L_sky 已删除" ✗ ——
            // 同一个 id 同时"存在"又"已删除" ✓，两句提示自相矛盾 ✓。
            // 紧邻的 `parent_id` 检查用的就是 `layer_alive` ✓ —— 正确谓词本来就在旁边 ✓。
            if state.layer_alive(layer_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("图层 {layer_id} 已存在"),
                ));
            }
            if let Some(parent) = payload_str(&atom.payload, "parent_id") {
                if !state.layer_alive(parent) {
                    return Err(missing_error(
                        "父图层",
                        parent,
                        state.layers.keys().filter(|id| state.layer_alive(id)),
                    ));
                }
                // **∴ 存活不等于不成环** ✗：同名 id 的**墓碑**仍在（删除可撤销 ✓），
                // 而 `layer_alive(layer_id)` 在墓碑上是 false ⇒ 允许重建 ✓
                // ⇒ 于是"以自己为父"（或挂在自称后代的层下）可以构造出来 ✓ ⇒ 必须在这里挡 ✓。
                check_layer_parent_cycle(state, layer_id, parent)?;
            }
        }
        AtomKind::CreateObject | AtomKind::ImportImage => {
            let object_id = required(atom, "object_id")?;
            // **实例必须在折叠层就能挡住循环引用** ✓（设计 9.3「循环引用检测：拒绝创建并返回错误」✓）——
            // 只在工具层检查是不够的 ✗：原子可以直接写进日志 ✓，
            // 而重放时遇到环会**无限递归** ✓（渲染时解析 master 会一直往下走 ✓）。
            // **两条入口都要挡环** ✓：创建实例 ✓，以及**后续改 `master_ref`** ✓
            //（设计 9.4 的 `link_to_master` ✓）—— 只在创建时检查是不够的 ✗：
            // 先建 a→b ✓ 再把 c 指到 a ✓ 再把 b 改指到 c ✓ 就成环了 ✓（每一步都"看起来"合法 ✓）。
            let link_target = if atom.kind == AtomKind::CreateObject
                && atom.payload.get("type").and_then(Value::as_str) == Some("instance")
            {
                atom.payload
                    .get("data")
                    .and_then(|data| data.get("master_ref"))
                    .and_then(|master_ref| master_ref.get("object_id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            } else if atom.kind == AtomKind::SetProperty
                && atom.payload.get("key").and_then(Value::as_str) == Some("master_ref")
            {
                atom.payload
                    .get("value")
                    .and_then(|value| value.get("object_id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            } else {
                None
            };
            if let Some(master) = link_target {
                // 走链判定抽在 `check_instance_cycle` 里 ✓（`SetProperty` 改 `master_ref` 时也调它 ✓，
                // 保证"是不是环"**只有一处判定** ✓，不会两处各判一套 ✓）。
                check_instance_cycle(state, object_id, &master)?;
            }
            if let Some(existing) = state.objects.get(object_id) {
                // **撞墓碑与撞存活是两回事，文案必须分开** ✓（期 1 E-b）：
                // 墓碑 id 不是「已存在」——再对它落笔会报「已被删除」，
                // 笼统说「已存在」两句自相矛盾（与 CreateLayer 的教训同形）。
                // fold 层不做复活（复活是工具层的 revive-and-replace），
                // 但出路要在这里点名 ✓。
                if existing.is_deleted() {
                    return Err(err(
                        ErrorCode::PreconditionFailed,
                        format!(
                            "对象 {object_id} 已被删除；可用 restore_object 恢复，或经支持复活的写入路径（add_adjustment／add_filter／apply_preset 等）以同 id 重建"
                        ),
                    ));
                }
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("对象 {object_id} 已存在"),
                ));
            }
            let layer_id = required(atom, "layer_id")?;
            if !state.layer_alive(layer_id) {
                return Err(missing_error(
                    "图层",
                    layer_id,
                    state.layers.keys().filter(|id| state.layer_alive(id)),
                ));
            }
        }
        AtomKind::DrawStroke
        | AtomKind::DrawShape
        | AtomKind::DrawText
        | AtomKind::Fill
        | AtomKind::Erase
        | AtomKind::Retouch
        | AtomKind::Liquify => {
            let object_id = required(atom, "object_id")?;
            match state.objects.get(object_id) {
                Some(object) if object.is_deleted() => {
                    return Err(tombstone_error("对象", object_id))
                }
                Some(_) => {}
                None => {
                    let layer_id = required(atom, "layer_id")?;
                    if !state.layer_alive(layer_id) {
                        return Err(missing_error(
                            "图层",
                            layer_id,
                            state.layers.keys().filter(|id| state.layer_alive(id)),
                        ));
                    }
                }
            }
        }
        // **★ 命名锚点（真机报告 2026-10-15）★**
        //
        // 写进**文档状态** ⇒ 随原子日志持久化 ✓ ⇒ **跨会话、跨重启都不漂移** ✓ ——
        // 这正是"草稿层的脸和彩色层的五官对不齐"的解药 ✓（用户的根因分析 ✓：
        // 每个新会话都重新目测估算 ⇒ 必然漂移 ✗；而 MCP 坐标系本身没有问题 ✓）。
        //
        // **空名字要拒** ✗：没有名字的锚点谁也引用不到 ⇒ 那是调用方的错 ✓（要报清楚 ✓）。
        AtomKind::SetAnchor => {
            let name = payload_str(&atom.payload, "name").unwrap_or_default();
            if name.is_empty() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{} 需要非空的 name", atom.kind)),
                )
                .with_atom(atom.id.clone()));
            }
            let Some(x) = payload_f64(&atom.payload, "x") else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{} 需要数值 x", atom.kind)),
                )
                .with_atom(atom.id.clone()));
            };
            let Some(y) = payload_f64(&atom.payload, "y") else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{} 需要数值 y", atom.kind)),
                )
                .with_atom(atom.id.clone()));
            };
            // **只校验，不改状态** ✓ —— 改状态在 `apply` 那一趟 ✓（两趟分工 ✓）。
            let _ = (x, y);
        }
        // 删除一个锚点：**不存在就报错** ✓（静默成功会让调用方以为"删掉了" ✗）。
        AtomKind::DeleteAnchor => {
            let name = payload_str(&atom.payload, "name").unwrap_or_default();
            if !state.anchors.contains_key(name) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{} 找不到锚点 {name}", atom.kind)),
                )
                .with_atom(atom.id.clone()));
            }
        }
        AtomKind::Supersede
        | AtomKind::Move
        | AtomKind::Transform
        | AtomKind::Tombstone
        | AtomKind::SetProperty => {
            // **改 `master_ref` 时的成环检查挂在这里** ✓ —— 我第一版把它写进了
            // `CreateObject` 的校验分支 ✗，而 `SetProperty` 原子**根本不走那里** ✓
            // ⇒ 测试当场抓到"通过改写制造环必须被拒：… ok:true" ✓。
            // 教训 ✓：**校验要挂在"能收到这类原子的分支"上** ✓，而不是"我恰好正在改的那个分支"上 ✓。
            if atom.payload.get("key").and_then(Value::as_str) == Some("master_ref") {
                if let (Some(object_id), Some(master)) = (
                    payload_str(&atom.payload, "object_id"),
                    atom.payload
                        .get("value")
                        .and_then(|value| value.get("object_id"))
                        .and_then(Value::as_str),
                ) {
                    check_instance_cycle(state, object_id, master)?;
                }
            }
            // **★ `parent_id` 改写也要挡环 ✗ ★**（2026-10-28）：只挡创建是不够的 ✓
            // （与 `master_ref` 那条同一个道理 ✓）—— 先 A⇐B、再 B⇐A，每一步单看都合法 ✓。
            // **∴ 挂在**这里**而不是 `CreateLayer` 分支 ✓**：`SetProperty` 原子根本不走那里 ✓
            // （上一段注释里已有同一个坑的教训 ✓）。
            if atom.payload.get("key").and_then(Value::as_str) == Some("parent_id") {
                if let (Some(layer_id), Some(parent)) = (
                    payload_str(&atom.payload, "layer_id"),
                    atom.payload.get("value").and_then(Value::as_str),
                ) {
                    check_layer_parent_cycle(state, layer_id, parent)?;
                }
            }
            if let Some(object_id) = payload_str(&atom.payload, "object_id") {
                match state.objects.get(object_id) {
                    Some(object) if object.is_deleted() => {
                        return Err(tombstone_error("对象", object_id))
                    }
                    Some(object) => {
                        // 液化网格变换拦截（第二轮第 3 条）：transform_primitive
                        // 不变换 Liquify 网格，变换液化对象本身是静默退化，
                        // 必须报错并提示先 bake_layer 固化液化后再变换。
                        if matches!(atom.kind, AtomKind::Move | AtomKind::Transform)
                            && object.object_type == crate::state::ObjectType::Liquify
                        {
                            return Err(err(
                                ErrorCode::PreconditionFailed,
                                "液化对象未压平：transform_primitive 不变换液化网格，请先 bake_layer 固化液化后再变换",
                            ));
                        }
                        // BakedBase 仿射门禁（第二轮第 1 条）：压平后追加的
                        // Move/Transform 只复合进基底变换；旋转限 90° 倍数，
                        // 非均匀缩放须显式 allow_non_uniform_scale。
                        if matches!(atom.kind, AtomKind::Move | AtomKind::Transform)
                            && object.data.get("baked_base").and_then(Value::as_bool) == Some(true)
                        {
                            let cur = crate::bake::Affine2D(object.transform.matrix);
                            let allow = atom
                                .payload
                                .get("allow_non_uniform_scale")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            let next = if let Some(delta) = atom.payload.get("delta") {
                                let dx = delta.get("dx").and_then(Value::as_f64).unwrap_or(0.0);
                                let dy = delta.get("dy").and_then(Value::as_f64).unwrap_or(0.0);
                                let mut m = cur.0;
                                m[4] += dx;
                                m[5] += dy;
                                crate::bake::Affine2D(m)
                            } else if atom.payload.get("transform").is_some() {
                                crate::bake::Affine2D(transform_from_payload(&atom.payload).matrix)
                            } else {
                                cur
                            };
                            // delta 平移不改变旋转/缩放，validate 直接通过；
                            // 绝对变换走完整门禁。
                            if atom.payload.get("transform").is_some() {
                                crate::bake::validate_affine(next, allow)?;
                            }
                        }
                    }
                    None => return Err(missing_error("对象", object_id, state.objects.keys())),
                }
            } else if let Some(layer_id) = payload_str(&atom.payload, "layer_id") {
                match state.layers.get(layer_id) {
                    Some(layer) if layer.is_deleted() => {
                        return Err(tombstone_error("图层", layer_id))
                    }
                    Some(_) => {}
                    None => {
                        return Err(missing_error(
                            "图层",
                            layer_id,
                            state.layers.keys().filter(|id| state.layer_alive(id)),
                        ))
                    }
                }
            } else if let Some(selection_id) = payload_str(&atom.payload, "selection_id") {
                // **校验层曾与实际能力不一致**：`apply` 的 Tombstone 分支早就支持
                // selection_id / mask_id / style_id ✓，但这里硬性要求 object_id 或 layer_id ✗，
                // 于是"删除选区/蒙版/风格"的原子会被判为 PreconditionFailed 并**静默跳过** ✓ ——
                // 这正是"工具返回 ok、日志里有原子、但状态没变"的根因 ✓。
                match state.selections.get(selection_id) {
                    Some(selection) if selection.is_deleted() => {
                        return Err(tombstone_error("选区", selection_id))
                    }
                    Some(_) => {}
                    None => {
                        return Err(missing_error("选区", selection_id, state.selections.keys()))
                    }
                }
            } else if let Some(mask_id) = payload_str(&atom.payload, "mask_id") {
                match state.masks.get(mask_id) {
                    Some(mask) if mask.is_deleted() => {
                        return Err(tombstone_error("蒙版", mask_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("蒙版", mask_id, state.masks.keys())),
                }
            } else if let Some(style_id) = payload_str(&atom.payload, "style_id") {
                match state.styles.get(style_id) {
                    Some(style) if style.is_deleted() => {
                        return Err(tombstone_error("风格", style_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("风格", style_id, state.styles.keys())),
                }
            } else if atom.kind != AtomKind::SetProperty {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    format!("{} 需要 object_id 或 layer_id", atom.kind),
                ));
            }
            if atom.kind == AtomKind::SetProperty && payload_str(&atom.payload, "key").is_none() {
                return Err(err(ErrorCode::InvalidArgument, "set_property 缺少 key"));
            }
        }
        AtomKind::ReorderLayers => {
            let order = payload_ids(&atom.payload, "order")?;
            for layer_id in &order {
                match state.layers.get(layer_id) {
                    Some(layer) if layer.is_deleted() => {
                        return Err(tombstone_error("图层", layer_id))
                    }
                    Some(_) => {}
                    None => {
                        return Err(missing_error(
                            "图层",
                            layer_id,
                            state.layers.keys().filter(|id| state.layer_alive(id)),
                        ))
                    }
                }
            }
            // 携带完整目标 z 序（设计文档 5.3）：绝对序快照必须覆盖全部存活图层。
            let alive: BTreeSet<&String> = state
                .layers
                .values()
                .filter(|l| !l.is_deleted())
                .map(|l| &l.id)
                .collect();
            let listed: BTreeSet<&String> = order.iter().collect();
            if alive != listed {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    "reorder_layers 必须携带完整目标 z 序（存活图层集合不一致）",
                ));
            }
        }
        AtomKind::ReorderObjects => {
            let layer_id = required(atom, "layer_id")?;
            match state.layers.get(layer_id) {
                Some(layer) if layer.is_deleted() => return Err(tombstone_error("图层", layer_id)),
                Some(_) => {}
                None => {
                    return Err(missing_error(
                        "图层",
                        layer_id,
                        state.layers.keys().filter(|id| state.layer_alive(id)),
                    ))
                }
            }
            let order = payload_ids(&atom.payload, "order")?;
            // 重复 id 先报（比集合差更具体；变异：去掉本分支 ⇒ R4 红）。
            let mut seen: BTreeSet<&String> = BTreeSet::new();
            let mut duplicates: Vec<&String> = Vec::new();
            for id in &order {
                if !seen.insert(id) {
                    duplicates.push(id);
                }
            }
            if !duplicates.is_empty() {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!(
                        "reorder_objects 的 order 含重复对象 id：{}",
                        duplicates
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join("、")
                    ),
                ));
            }
            // 该层全部存活对象的集合。
            let alive: BTreeSet<&String> = state
                .objects
                .values()
                .filter(|object| object.is_alive() && object.layer_id == layer_id)
                .map(|object| &object.id)
                .collect();
            // 逐个分类 order 里的 id：未知 / 已删 / 跨层。
            for id in &order {
                match state.objects.get(id) {
                    None => {
                        return Err(missing_error("对象", id, alive.iter().copied()));
                    }
                    Some(object) if object.is_deleted() => {
                        return Err(tombstone_error("对象", id));
                    }
                    Some(object) if object.layer_id != layer_id => {
                        return Err(err(
                            ErrorCode::PreconditionFailed,
                            format!(
                                "对象 {id} 属于图层 {}，不在目标图层 {layer_id} 内",
                                object.layer_id
                            ),
                        ));
                    }
                    Some(_) => {}
                }
            }
            let listed: BTreeSet<&String> = order.iter().collect();
            if alive != listed {
                let missing: Vec<&String> = alive.difference(&listed).copied().collect();
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!(
                        "reorder_objects 必须携带完整目标 z 序（图层 {layer_id}：期望 {} 个对象，实际 {} 个；缺失：{}）",
                        alive.len(),
                        listed.len(),
                        missing
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join("、")
                    ),
                ));
            }
        }
        AtomKind::CreateSelection => {
            let selection_id = required(atom, "selection_id")?;
            // **`mode != "new"` 时允许已存在**（外部 `E1027`，第 773 轮）：`add`/`subtract`/`intersect`
            // 的语义就是「改一个已有的选区」⇒ 若这里一律拒绝，那三个值永远不可达（死参数）。
            // ⇒ 只有 `new`（缺省）才要求 id 未占用。
            let mode = atom
                .payload
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("new");
            // **只与"存活"的选区冲突** ✓（与 `CreateLayer` 同一条教训 ✓）：
            // 折叠层保留墓碑 ✓（删除可撤销的基础 ✓），但墓碑**不该占用 id** ✗ ——
            // 原来这里是 `contains_key` ✓，于是 `delete_selection` 之后同一个 id
            // **永远建不回来** ✗，而 `list_selections` 又说它不存在 ✗（两句自相矛盾 ✓）。
            if mode == "new" && state.selection_alive(selection_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("选区 {selection_id} 已存在（要改它请传 mode=add/subtract/intersect）"),
                ));
            }
        }
        AtomKind::CreateMask => {
            let mask_id = required(atom, "mask_id")?;
            // 同 `CreateSelection` ✓：墓碑不占用 id ✓（实测 `delete_mask` 后重建会被拒 ✗）。
            if state.mask_alive(mask_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("蒙版 {mask_id} 已存在"),
                ));
            }
        }
        AtomKind::UpdateMask | AtomKind::DeleteMask => {
            // mask_id 必填；存在性由 `check_auxiliary_refs` 统一检查（无孤儿引用）。
            required(atom, "mask_id")?;
        }
        AtomKind::UpdateSelection => {
            // selection_id 必填；存在性由 `check_auxiliary_refs` 统一检查（无孤儿引用）。
            required(atom, "selection_id")?;
        }
        AtomKind::CreateStyle => {
            let style_id = required(atom, "style_id")?;
            // 同上 ✓：墓碑不占用 id ✓。
            if state.style_alive(style_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("风格 {style_id} 已存在"),
                ));
            }
        }
        AtomKind::CreateCheckpoint | AtomKind::Checkpoint => {
            if let Some(checkpoint_id) = payload_str(&atom.payload, "checkpoint_id") {
                if state.checkpoints.contains_key(checkpoint_id) {
                    return Err(err(
                        ErrorCode::PreconditionFailed,
                        format!("检查点 {checkpoint_id} 已存在"),
                    ));
                }
            }
        }
        AtomKind::Revert | AtomKind::Reapply => {
            // 目标原子是否存在于本次折叠范围内由调用方保证（见 seq 模块）；
            // 折叠期无法凭状态判定，故只校验字段存在。
            if atom.target_atom().is_none() {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    format!("{} 缺少 target", atom.kind),
                ));
            }
        }
        AtomKind::DeclareHead => {
            // 由 state@seq 公式处理（5.5）。
        }
        AtomKind::ResizeCanvasBounds
        | AtomKind::ScaleCanvas
        | AtomKind::RotateCanvas
        | AtomKind::FlipCanvas
        | AtomKind::CropCanvas
        | AtomKind::ExtendCanvas => {
            // 几何原子改画布尺寸与全部对象变换：必须已有文档、新尺寸 ≥ 1、
            // 仿射矩阵 6 个分量全部有限。工具层先行校验，这里是重放时的把关。
            if state.doc_id.is_none() {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    "几何操作需要先创建文档",
                ));
            }
            let width = payload_u64(&atom.payload, "new_width").unwrap_or(0);
            let height = payload_u64(&atom.payload, "new_height").unwrap_or(0);
            if width < 1 || height < 1 {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    "几何操作的新画布尺寸必须 ≥ 1",
                ));
            }
            let matrix = atom
                .payload
                .get("affine")
                .and_then(|affine| affine.get("matrix"))
                .and_then(Value::as_array);
            let finite = matrix
                .map(|list| {
                    list.len() == 6
                        && list
                            .iter()
                            .all(|item| item.as_f64().map(f64::is_finite).unwrap_or(false))
                })
                .unwrap_or(false);
            if !finite {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    "几何原子的 affine.matrix 必须是 6 个有限数",
                ));
            }
        }
        AtomKind::BakeLayer => {
            let layer_id = required(atom, "layer_id")?;
            if !state.layer_alive(layer_id) {
                return Err(missing_error(
                    "图层",
                    layer_id,
                    state.layers.keys().filter(|id| state.layer_alive(id)),
                ));
            }
            let blob_hash = required(atom, "blob_hash")?;
            if blob_hash.parse::<crate::atom::BlobHash>().is_err() {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    format!("bake_layer blob_hash 非法：{blob_hash}"),
                ));
            }
            if atom.payload.get("up_to_seq").and_then(Value::as_u64).is_none() {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    "bake_layer 缺少 up_to_seq",                ));
            }
        }
        AtomKind::Tag
        | AtomKind::Comment
        | AtomKind::Suggest
        | AtomKind::AcceptSuggestion
        | AtomKind::RejectSuggestion
        // `set_background` 没有 id 引用可查 ✓（载荷形状由 `apply` 那一臂把关 ✓，
        // 缺 `background` 时 `apply` 报错 ⇒ 折叠记级联警告、原子跳过 ✓）。
        | AtomKind::SetBackground => {}
    }
    check_auxiliary_refs(state, atom)
}

/// 检查辅助引用（`style_id` / `mask_id` / `parent_id` 与 `set_property` 的属性值）。
///
/// 这些引用必须指向当时存活的实体，否则被撤销的 `create_mask` 等原子会留下
/// 指向已失效实体的引用，破坏 5.3「无孤儿引用」不变量。
fn check_auxiliary_refs(state: &DocumentState, atom: &Atom) -> Result<()> {
    let mut checks: Vec<(&str, &str)> = Vec::new();
    for key in ["style_id", "mask_id", "parent_id"] {
        // 创建类原子声明的实体 id 自身尚不存在，不是“辅助引用”。
        let is_primary = matches!(
            (atom.kind, key),
            (AtomKind::CreateMask, "mask_id") | (AtomKind::CreateStyle, "style_id")
        );
        if is_primary {
            continue;
        }
        if let Some(id) = payload_str(&atom.payload, key) {
            checks.push((key, id));
        }
    }
    if atom.kind == AtomKind::SetProperty {
        if let (Some(key), Some(value)) = (
            payload_str(&atom.payload, "key"),
            atom.payload.get("value").and_then(Value::as_str),
        ) {
            match key {
                "mask_id" => checks.push(("mask_id", value)),
                "style_id" => checks.push(("style_id", value)),
                "parent_id" => checks.push(("parent_id", value)),
                "layer_id" => checks.push(("layer_id", value)),
                _ => {}
            }
        }
    }
    for (key, id) in checks {
        let alive = match key {
            "style_id" => state.styles.get(id).is_some_and(|s| !s.is_deleted()),
            // Q2 双表：`UpdateMask` 的 mask_id 允许指向选区（paint_mask 修整选区）。
            "mask_id" if atom.kind == AtomKind::UpdateMask => {
                state.masks.get(id).is_some_and(|m| !m.is_deleted())
                    || state.selections.get(id).is_some_and(|s| !s.is_deleted())
            }
            "mask_id" => state.masks.get(id).is_some_and(|m| !m.is_deleted()),
            "parent_id" | "layer_id" => state.layer_alive(id),
            _ => true,
        };
        if !alive {
            // **按字段名给出对应的可用值** ✓（这里的 `key` 是字段名 ✓，所以报错会写成
            // `layer_id x 不存在（现有图层：…）` ✓ —— 比原来那句光秃秃的"不存在"有用得多 ✓）。
            let available: Vec<&String> = match key {
                "style_id" => state.styles.keys().collect(),
                "mask_id" => state.masks.keys().collect(),
                _ => state
                    .layers
                    .keys()
                    .filter(|id| state.layer_alive(id))
                    .collect(),
            };
            return Err(missing_error(key, id, available));
        }
    }
    Ok(())
}

/// 应用一个原子（调用方须先通过 [`precondition`]）。
// `contains_key` + `insert` 分支在这里是刻意的：两条路径的语义与副作用完全不同。
#[allow(clippy::map_entry)]
pub fn apply(state: &mut DocumentState, atom: &Atom) -> Result<()> {
    let blobs: Vec<crate::atom::BlobHash> = atom.all_blob_refs().into_iter().collect();
    match atom.kind {
        // **★ 命名锚点（真机报告 2026-10-15）★**：写进**文档状态** ⇒ 随原子日志持久化 ✓
        //   ⇒ 跨会话、跨重启都不漂移 ✓（校验已在 `precondition` 那一趟做过 ✓）。
        AtomKind::SetAnchor => {
            let name = payload_str(&atom.payload, "name").unwrap_or_default();
            state.anchors.insert(
                name.to_owned(),
                crate::state::Anchor {
                    x: payload_f64(&atom.payload, "x").unwrap_or(0.0),
                    y: payload_f64(&atom.payload, "y").unwrap_or(0.0),
                    note: payload_str(&atom.payload, "note").map(str::to_owned),
                },
            );
        }
        AtomKind::DeleteAnchor => {
            let name = payload_str(&atom.payload, "name").unwrap_or_default();
            state.anchors.remove(name);
        }
        AtomKind::CreateDocument => {
            state.doc_id = payload_str(&atom.payload, "doc_id").map(str::to_owned);
            state.width = payload_u64(&atom.payload, "width").unwrap_or(0) as u32;
            state.height = payload_u64(&atom.payload, "height").unwrap_or(0) as u32;
            if let Some(color_space) = payload_str(&atom.payload, "color_space") {
                state.color_space = color_space.to_owned();
            }
            state.background = atom
                .payload
                .get("background")
                .cloned()
                .unwrap_or(Value::Null);
            state.medium = payload_str(&atom.payload, "medium").map(str::to_owned);
            state.style = payload_str(&atom.payload, "style_id").map(str::to_owned);
            // **新文档初始就带一个默认图层** —— 进初始状态、**不额外产生原子**；z=-1 当最底层基底，
            // 且不干扰既有"最大 z + 1"规则。身份用 `layer_default`（若叫 `layer_1` 会与后续建层撞名 ⇒ 60 条失败 ✗）。
            let default_layer_id = "layer_default".to_owned();
            state.layers.insert(
                default_layer_id.clone(),
                Layer {
                    id: default_layer_id,
                    name: "图层 1".to_owned(),
                    layer_type: parse_layer_type(None),
                    parent_id: None,
                    z_index: -1,
                    blend_mode: "normal".to_owned(),
                    opacity: 1.0,
                    visible: true,
                    locked: false,
                    alpha_lock: false,
                    clipping_mask: false,
                    mask_id: None,
                    transform: transform_from_payload(&Value::Null),
                    medium: None,
                    style: None,
                    metadata: Value::Null,
                    blobs: Vec::new(),
                    created_by: atom.id.clone(),
                    updated_by: None,
                    deleted_by: None,
                },
            );
        }
        // **几何（路线三）**：画布尺寸 + 逐对象变换复合 + 锚点/选区/蒙版跟随。
        // 渲染不动：对象 `transform` 字段的合成就是渲染口径（`transform_primitive`
        // 逐层施加与此处的矩阵合成同一约定）。撤销 = 重折时排除本原子，干净回退。
        AtomKind::ResizeCanvasBounds
        | AtomKind::ScaleCanvas
        | AtomKind::RotateCanvas
        | AtomKind::FlipCanvas
        | AtomKind::CropCanvas
        | AtomKind::ExtendCanvas => {
            apply_geometry(state, atom)?;
        }
        // **改已有文档底色**：只写 `state.background`，与 `CreateDocument` 同形
        // （`{r, g, b, a}`）⇒ 渲染侧 `parse_background(&state.background)` 直接读到新底色 ✓。
        // 缺 `background` 不静默清成透明：`apply` 报错 ⇒ 折叠记一条级联警告、该原子跳过 ✓
        // （工具层永远写完整底色，正常日志走不到这条 ✓）。
        AtomKind::SetBackground => {
            let background = atom.payload.get("background").ok_or_else(|| {
                err(
                    ErrorCode::InvalidArgument,
                    "set_background 缺少 background（应为 {r, g, b, a}）",
                )
            })?;
            state.background = background.clone();
        }
        AtomKind::CreateLayer => {
            let layer_id = required(atom, "layer_id")?.to_owned();
            let z_index = payload_u64(&atom.payload, "z_index")
                .map(|z| z as i64)
                .unwrap_or_else(|| {
                    state
                        .alive_layers()
                        .iter()
                        .map(|l| l.z_index + 1)
                        .max()
                        .unwrap_or(0)
                });
            let layer = Layer {
                id: layer_id.clone(),
                name: payload_str(&atom.payload, "name")
                    .unwrap_or(&layer_id)
                    .to_owned(),
                layer_type: parse_layer_type(payload_str(&atom.payload, "type")),
                parent_id: payload_str(&atom.payload, "parent_id").map(str::to_owned),
                z_index,
                blend_mode: payload_str(&atom.payload, "blend_mode")
                    .unwrap_or("normal")
                    .to_owned(),
                opacity: payload_f64(&atom.payload, "opacity").unwrap_or(1.0),
                visible: payload_bool(&atom.payload, "visible").unwrap_or(true),
                locked: payload_bool(&atom.payload, "locked").unwrap_or(false),
                alpha_lock: payload_bool(&atom.payload, "alpha_lock").unwrap_or(false),
                clipping_mask: payload_bool(&atom.payload, "clipping_mask").unwrap_or(false),
                mask_id: payload_str(&atom.payload, "mask_id").map(str::to_owned),
                transform: transform_from_payload(&atom.payload),
                medium: payload_str(&atom.payload, "medium").map(str::to_owned),
                style: payload_str(&atom.payload, "style_id").map(str::to_owned),
                metadata: atom.payload.get("metadata").cloned().unwrap_or(Value::Null),
                blobs: blobs.clone(),
                created_by: atom.id.clone(),
                updated_by: None,
                deleted_by: None,
            };
            state.layers.insert(layer_id, layer);
        }
        AtomKind::CreateObject | AtomKind::ImportImage => {
            let object_id = required(atom, "object_id")?.to_owned();
            let layer_id = required(atom, "layer_id")?.to_owned();
            let object_type = parse_object_type(payload_str(&atom.payload, "type"));
            let object = Object {
                seq: atom.seq,
                id: object_id.clone(),
                layer_id,
                object_type,
                z_index: payload_u64(&atom.payload, "z_index")
                    .map(|z| z as i64)
                    .unwrap_or(0),
                visible: payload_bool(&atom.payload, "visible").unwrap_or(true),
                enabled: payload_bool(&atom.payload, "enabled").unwrap_or(true),
                locked: payload_bool(&atom.payload, "locked").unwrap_or(false),
                metadata: atom.payload.get("metadata").cloned().unwrap_or(Value::Null),
                transform: transform_from_payload(&atom.payload),
                style: payload_str(&atom.payload, "style_id").map(str::to_owned),
                versions: vec![atom.id.clone()],
                current_version: Some(atom.id.clone()),
                created_by: atom.id.clone(),
                deleted_by: None,
                data: object_data(atom.kind, &atom.payload),
                blobs,
            };
            state.objects.insert(object_id, object);
        }
        AtomKind::DrawStroke
        | AtomKind::DrawShape
        | AtomKind::DrawText
        | AtomKind::Fill
        | AtomKind::Erase
        | AtomKind::Retouch
        | AtomKind::Liquify
        | AtomKind::Supersede => {
            let object_id = required(atom, "object_id")?.to_owned();
            if state.objects.contains_key(&object_id) {
                supersede_object(state, atom, &object_id, Some(blobs))?
            } else {
                let layer_id = required(atom, "layer_id")?.to_owned();
                let object_type = object_type_for(atom.kind, payload_str(&atom.payload, "type"));
                let object = Object {
                    seq: atom.seq,
                    id: object_id.clone(),
                    layer_id,
                    object_type,
                    z_index: payload_u64(&atom.payload, "z_index")
                        .map(|z| z as i64)
                        .unwrap_or(0),
                    visible: payload_bool(&atom.payload, "visible").unwrap_or(true),
                    enabled: payload_bool(&atom.payload, "enabled").unwrap_or(true),
                    locked: payload_bool(&atom.payload, "locked").unwrap_or(false),
                    metadata: atom.payload.get("metadata").cloned().unwrap_or(Value::Null),
                    transform: transform_from_payload(&atom.payload),
                    style: payload_str(&atom.payload, "style_id").map(str::to_owned),
                    versions: vec![atom.id.clone()],
                    current_version: Some(atom.id.clone()),
                    created_by: atom.id.clone(),
                    deleted_by: None,
                    data: object_data(atom.kind, &atom.payload),
                    blobs,
                };
                state.objects.insert(object_id, object);
            }
        }
        AtomKind::Move | AtomKind::Transform => {
            let object_id = required(atom, "object_id")?.to_owned();
            supersede_object(state, atom, &object_id, None)?;
            // **`delta` 与 `transform` 语义不同** ✓（子 agent 报的 #6/#7 ✓）：
            //   * `delta`（增移 ✓）⇒ 在对象**当前**变换上**复合** ✓；
            //   * `transform`（绝对 ✓）⇒ **直接赋值** ✓ —— 包括**单位矩阵** ✓：
            //     此前用 `if !transform.is_identity()` 把单位变换**丢掉** ✗
            //     ⇒ 对象**永远回不到原点** ✓（子 agent 实测：`delta:{0,0}` 返回 ok 却不改包围盒 ✓）。
            let delta = atom.payload.get("delta").map(|value| {
                (
                    value.get("dx").and_then(Value::as_f64).unwrap_or(0.0),
                    value.get("dy").and_then(Value::as_f64).unwrap_or(0.0),
                )
            });
            let has_transform = atom.payload.get("transform").is_some();
            let transform = transform_from_payload(&atom.payload);
            if let Some(object) = state.objects.get_mut(&object_id) {
                if let Some((dx, dy)) = delta {
                    // 文档坐标的平移 ✓：在矩阵的平移分量上叠加 ✓
                    //（`p' = pivot + M·(p − pivot)` ✓ ⇒ `e,f` 加上 (dx,dy) 即整体平移 ✓）。
                    object.transform.matrix[4] += dx;
                    object.transform.matrix[5] += dy;
                } else if has_transform {
                    object.transform = transform;
                }
            }
        }
        AtomKind::SetProperty => {
            let key = required(atom, "key")?.to_owned();
            let value = atom.payload.get("value").cloned().unwrap_or(Value::Null);
            if let Some(object_id) = payload_str(&atom.payload, "object_id") {
                let object_id = object_id.to_owned();
                supersede_object(state, atom, &object_id, None)?;
                // **组变换：作用到成员自身** ✓（设计 9.4 的 `set_group_transform` ✓）。
                //
                // **设计未规定"组变换如何落到成员上" ⇒ 记录选择 ✓**：
                // 设计给出的是一套**派生解析**（9.2 `resolve_object` ✓、9.3 缓存与失效 ✓），
                // 那是一条完整链 ✓（实例共享 Master 缓存 ✓、依赖图传播 ✓、循环检测 ✓）。
                // 本片**只做组** ✓，采用最小且**处处自洽**的做法 ✓：
                // 把**平移增量**加到每个成员自己的 `transform` 上 ✓ ⇒
                // 渲染 ✓、`object_bbox` ✓、命中测试 ✓、脏区规划 ✓ **全部无需改动就正确** ✓；
                // 同时把增量**累积**到组自身的 `group_transform` 上 ✓，便于读取与后续迁移到派生解析 ✓。
                //
                // **边界（明确记下 ✓）**：本片只接受**平移**形式 `{dx, dy}` ✓；
                // 一般仿射矩阵、成员级 override、组嵌套（组套组 ✓）都**尚未实现** ✓ ——
                // 它们需要 9.2/9.3 那套派生解析 ✓，属于下一步 ✓，此处**显式拒绝**而不静默忽略 ✓。
                if key == "group_transform" {
                    let is_group = state
                        .objects
                        .get(&object_id)
                        .map(|object| object.object_type == crate::state::ObjectType::Group)
                        .unwrap_or(false);
                    if is_group {
                        let members: Vec<String> = state
                            .objects
                            .get(&object_id)
                            .and_then(|object| object.data.get("members"))
                            .and_then(Value::as_array)
                            .map(|items| {
                                items
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default();
                        let (dx, dy) = group_translation(&value).ok_or_else(|| {
                            err(
                                ErrorCode::InvalidArgument,
                                "本片只支持平移形式的组变换 {dx, dy}（一般仿射与嵌套组尚未实现）"
                                    .to_owned(),
                            )
                        })?;
                        for member in &members {
                            if let Some(object) = state.objects.get_mut(member) {
                                object.transform.matrix[4] += dx;
                                object.transform.matrix[5] += dy;
                            }
                        }
                    }
                }
                // **改 `sync_policy` 时同步维护快照** ✓（设计 9.1 的"不同步"✓）。
                //
                // **设计未规定"不同步"怎么落地 ⇒ 记录选择** ✓：改成 `none` 的那一刻，
                // 把**当前解析结果**（master 的 `data` ✓ 与合成变换 ✓）快照进实例自身 ✓
                // ⇒ 渲染优先用它 ✓（见 `resolve_instance` ✓）；改回 `all` 就**清掉**快照 ✓
                // ⇒ 立刻恢复跟随 ✓（可来回切换 ✓、可验证 ✓）。
                //
                // **必须放在折叠层** ✓：快照要**由日志决定** ✓ ——
                // 它是"那一刻的状态"✓，重放时必须在**同一个 seq** 得到同一份快照 ✓，
                // 否则同一份日志在不同时候会渲染出不同结果 ✗。
                if key == "sync_policy" {
                    let policy = value.as_str().unwrap_or("all").to_owned();
                    if policy != "all" && policy != "none" {
                        return Err(err(
                            ErrorCode::InvalidArgument,
                            format!(
                                "本片只支持 sync_policy: all 或 none（收到 {policy}）——                                  partial 需要设计 9.3 的依赖图传播"
                            ),
                        ));
                    }
                    let snapshot = if policy == "none" {
                        resolve_master_snapshot(state, &object_id)
                    } else {
                        None
                    };
                    if policy == "none" && snapshot.is_none() {
                        return Err(err(
                            ErrorCode::PreconditionFailed,
                            format!("实例 {object_id} 的 master 当前不可解析，无法快照"),
                        ));
                    }
                    if let Some(object) = state.objects.get_mut(&object_id) {
                        apply_property_to_object(object, &key, &value);
                        match snapshot {
                            Some((data, transform)) => {
                                object.data["snapshot"] = data;
                                object.data["snapshot_transform"] = serde_json::json!({
                                    "matrix": transform.matrix,
                                    "pivot": transform.pivot,
                                });
                            }
                            None => {
                                // **必须真正删掉** ✓（置 `Null` 会让渲染走进快照分支 ✗ ⇒ 解不出图元 ✓）。
                                if let Some(map) = object.data.as_object_mut() {
                                    map.remove("snapshot");
                                    map.remove("snapshot_transform");
                                }
                            }
                        }
                    }
                } else if let Some(object) = state.objects.get_mut(&object_id) {
                    apply_property_to_object(object, &key, &value);
                }
            } else if let Some(layer_id) = payload_str(&atom.payload, "layer_id") {
                let layer_id = layer_id.to_owned();
                if let Some(layer) = state.layers.get_mut(&layer_id) {
                    apply_property_to_layer(layer, &key, &value);
                    layer.updated_by = Some(atom.id.clone());
                }
            }
        }
        AtomKind::ReorderLayers => {
            let order = payload_ids(&atom.payload, "order")?;
            for (index, layer_id) in order.iter().enumerate() {
                if let Some(layer) = state.layers.get_mut(layer_id) {
                    layer.z_index = index as i64;
                    layer.updated_by = Some(atom.id.clone());
                }
            }
        }
        AtomKind::ReorderObjects => {
            // 注意：`Object` 没有 `updated_by` 字段（只有 `Layer` 有），
            // 故只改 z_index——与 `set_property z_index` 单改的对象分支同口径。
            let order = payload_ids(&atom.payload, "order")?;
            for (index, object_id) in order.iter().enumerate() {
                if let Some(object) = state.objects.get_mut(object_id) {
                    object.z_index = index as i64;
                }
            }
        }
        AtomKind::Tombstone => {
            if let Some(object_id) = payload_str(&atom.payload, "object_id") {
                if let Some(object) = state.objects.get_mut(object_id) {
                    object.deleted_by = Some(atom.id.clone());
                }
            } else if let Some(layer_id) = payload_str(&atom.payload, "layer_id") {
                tombstone_layer(state, layer_id, &atom.id);
            } else if let Some(selection_id) = payload_str(&atom.payload, "selection_id") {
                if let Some(selection) = state.selections.get_mut(selection_id) {
                    selection.deleted_by = Some(atom.id.clone());
                }
            } else if let Some(mask_id) = payload_str(&atom.payload, "mask_id") {
                if let Some(mask) = state.masks.get_mut(mask_id) {
                    mask.deleted_by = Some(atom.id.clone());
                }
            } else if let Some(style_id) = payload_str(&atom.payload, "style_id") {
                if let Some(style) = state.styles.get_mut(style_id) {
                    style.deleted_by = Some(atom.id.clone());
                }
            }
        }
        AtomKind::CreateSelection => {
            let selection_id = required(atom, "selection_id")?.to_owned();
            let selection = parse_selection(atom, selection_id.clone(), blobs)?;
            state.selections.insert(selection_id, selection);
        }

        AtomKind::CreateMask => {
            let mask_id = required(atom, "mask_id")?.to_owned();
            let mask = parse_selection(atom, mask_id.clone(), blobs)?;
            state.masks.insert(mask_id, mask);
        }
        AtomKind::UpdateSelection => {
            // Q1 `transform_selection`：原地更新 shape/feather/invert，不改 id/mode/patches。
            let selection_id = required(atom, "selection_id")?.to_owned();
            let Some(sel) = state.selections.get_mut(&selection_id) else {
                return Err(missing_error(
                    "选区",
                    &selection_id,
                    state.selections.keys(),
                ));
            };
            if sel.is_deleted() {
                return Err(tombstone_error("选区", &selection_id));
            }
            if let Some(shape) = atom.payload.get("shape") {
                sel.shape = shape.clone();
            }
            if let Some(feather) = atom.payload.get("feather").and_then(Value::as_f64) {
                sel.feather = feather;
            }
            if let Some(invert) = atom.payload.get("invert").and_then(Value::as_bool) {
                sel.invert = invert;
            }
            for blob in &blobs {
                if !sel.blobs.contains(blob) {
                    sel.blobs.push(blob.clone());
                }
            }
        }
        AtomKind::UpdateMask => {
            let mask_id = required(atom, "mask_id")?.to_owned();
            // Q2 双表寻址：蒙版表优先，选区表次之（`paint_mask` 可直接修整选区边缘，
            // 即 Photoshop 的「快速蒙版」语义；`Mask = Selection` 同构体故可复用）。
            let mask = if let Some(m) = state.masks.get_mut(&mask_id) {
                m
            } else if let Some(s) = state.selections.get_mut(&mask_id) {
                s
            } else {
                return Err(missing_error("蒙版", &mask_id, state.masks.keys()));
            };
            if mask.is_deleted() {
                return Err(tombstone_error("蒙版", &mask_id));
            }
            let patch = parse_mask_patch(atom.payload.get("patch").unwrap_or(&Value::Null))?;
            if patch.op == MaskPatchOp::ReplaceAll {
                // 快照截断：之前的增量链不再需要重放（blob 仍留在原子日志里供撤销）。
                mask.patches.clear();
            }
            // blob 进生命周期引用（导出/GC 可见）；patches 里另有合成元数据。
            for blob in &blobs {
                if !mask.blobs.contains(blob) {
                    mask.blobs.push(blob.clone());
                }
            }
            mask.patches.push(patch);
        }
        AtomKind::DeleteMask => {
            let mask_id = required(atom, "mask_id")?.to_owned();
            let Some(mask) = state.masks.get_mut(&mask_id) else {
                return Err(missing_error("蒙版", &mask_id, state.masks.keys()));
            };
            // Tombstone：不清空 blobs/patches，撤销时可无损回退。
            mask.deleted_by = Some(atom.id.clone());
        }
        AtomKind::CreateStyle => {
            let style_id = required(atom, "style_id")?.to_owned();
            let style = Style {
                id: style_id.clone(),
                name: payload_str(&atom.payload, "name")
                    .unwrap_or(&style_id)
                    .to_owned(),
                category: payload_str(&atom.payload, "category")
                    .unwrap_or("")
                    .to_owned(),
                parent_style: payload_str(&atom.payload, "parent_style").map(str::to_owned),
                brushes: atom.payload.get("brushes").cloned().unwrap_or(Value::Null),
                palette: atom.payload.get("palette").cloned().unwrap_or(Value::Null),
                textures: atom.payload.get("textures").cloned().unwrap_or(Value::Null),
                render_params: atom
                    .payload
                    .get("render_params")
                    .cloned()
                    .unwrap_or(Value::Null),
                seed_policy: atom
                    .payload
                    .get("seed_policy")
                    .cloned()
                    .unwrap_or(Value::Null),
                blobs,
                created_by: atom.id.clone(),
                deleted_by: None,
            };
            state.styles.insert(style_id, style);
        }
        AtomKind::CreateCheckpoint | AtomKind::Checkpoint => {
            let checkpoint_id = payload_str(&atom.payload, "checkpoint_id")
                .map(str::to_owned)
                .unwrap_or_else(|| format!("ckpt_{}", atom.id));
            let anchor_seq = payload_u64(&atom.payload, "anchor_seq").unwrap_or(atom.seq);
            let checkpoint = Checkpoint {
                id: checkpoint_id.clone(),
                name: payload_str(&atom.payload, "name")
                    .unwrap_or(&checkpoint_id)
                    .to_owned(),
                created_by: atom.actor.clone(),
                message: atom.message.clone(),
                anchor_seq,
                snapshot_id: payload_str(&atom.payload, "snapshot_id").map(str::to_owned),
            };
            state.checkpoints.insert(checkpoint_id, checkpoint);
        }
        AtomKind::BakeLayer => {
            // 折叠期只做状态替换，不渲染、不读存储（存储在渲染/工具层校验）：
            // 清空目标层全部存活对象，写入单个压平基底对象（见 object.rs 的
            // BakedBase 图元）；图层 ID/名称/混合/不透明度/z 序/掩膜一律不动。
            // 压平后追加的 Move/Transform 作用在这个基底对象上（对象变换），
            // 下一次 bake 由工具层按累积变换一次采样后重置为 IDENTITY。
            let layer_id = required(atom, "layer_id")?.to_owned();
            let blob_hash = required(atom, "blob_hash")?.to_owned();
            let up_to_seq = atom
                .payload
                .get("up_to_seq")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if !state.layers.contains_key(&layer_id) {
                return Err(err(
                    ErrorCode::ReferenceNotFound,
                    format!("图层 {layer_id} 不存在"),
                ));
            }
            let object_ids: Vec<String> = state
                .objects
                .values()
                .filter(|o| o.layer_id == layer_id && o.is_alive())
                .map(|o| o.id.clone())
                .collect();
            for oid in object_ids {
                state.objects.remove(&oid);
            }
            let baked_id = format!("baked:{layer_id}");
            let data = serde_json::json!({
                "baked_base": true,
                "blob_hash": blob_hash,
                "source_up_to_seq": up_to_seq,
                "width": state.width,
                "height": state.height,
            });
            let object = Object {
                seq: atom.seq,
                id: baked_id,
                layer_id: layer_id.clone(),
                object_type: ObjectType::RasterPatch,
                z_index: 0,
                visible: true,
                enabled: true,
                locked: false,
                metadata: Value::Null,
                transform: transform_from_payload(&Value::Null),
                style: None,
                versions: vec![atom.id.clone()],
                current_version: Some(atom.id.clone()),
                created_by: atom.id.clone(),
                deleted_by: None,
                data,
                blobs: blobs.clone(),
            };
            state.objects.insert(object.id.clone(), object);
        }
        AtomKind::DeclareHead => {
            state.declare_head = Some(DeclareHead {
                atom_id: atom.id.clone(),
                seq: atom.seq,
                base: parse_head_base(&atom.payload)?,
                reason: payload_str(&atom.payload, "reason").map(str::to_owned),
            });
        }
        AtomKind::Revert | AtomKind::Reapply | AtomKind::Tag => {
            // 撤销/恢复的效果完全由有效集决定（pass 1），本身无状态效果；
            // tag 只作为日志中的元数据标记。
        }
        AtomKind::Comment
        | AtomKind::Suggest
        | AtomKind::AcceptSuggestion
        | AtomKind::RejectSuggestion => {
            // 协作原子不产生状态效果，不参与折叠（设计文档 5.2 注）。
        }
    }
    Ok(())
}

/// 解析 `declare_head` 的 base 引用。
pub fn parse_head_base(payload: &Value) -> Result<HeadBase> {
    if let Some(base) = payload.get("base") {
        let kind = base.get("type").and_then(Value::as_str);
        let id = base.get("id").and_then(Value::as_str);
        if let (Some(kind), Some(id)) = (kind, id) {
            return match kind {
                "atom" | "declare_head" => Ok(HeadBase::Atom(id.to_owned())),
                "checkpoint" => Ok(HeadBase::Checkpoint(id.to_owned())),
                other => Err(err(
                    ErrorCode::InvalidArgument,
                    format!("未知的 declare_head.base.type: {other}"),
                )),
            };
        }
    }
    if let Some(atom_id) = payload_str(payload, "base_atom_id") {
        return Ok(HeadBase::Atom(atom_id.to_owned()));
    }
    if let Some(checkpoint_id) = payload_str(payload, "checkpoint_id") {
        return Ok(HeadBase::Checkpoint(checkpoint_id.to_owned()));
    }
    Err(err(
        ErrorCode::InvalidArgument,
        "declare_head 缺少 base（形如 {type, id}）",
    ))
}

fn parse_layer_type(value: Option<&str>) -> LayerType {
    match value {
        Some("vector") => LayerType::Vector,
        Some("layer_group") => LayerType::LayerGroup,
        Some("mask") => LayerType::Mask,
        _ => LayerType::Raster,
    }
}

fn parse_object_type(value: Option<&str>) -> ObjectType {
    match value {
        Some("shape") => ObjectType::Shape,
        Some("text") => ObjectType::Text,
        Some("adjustment") => ObjectType::Adjustment,
        Some("filter") => ObjectType::Filter,
        Some("raster_patch") => ObjectType::RasterPatch,
        Some("retouch") => ObjectType::Retouch,
        Some("liquify") => ObjectType::Liquify,
        Some("instance") => ObjectType::Instance,
        Some("group") => ObjectType::Group,
        // **`path`** ✓（设计 792 的路径对象 ✓，用户已裁决新增该类型 ✓）。
        Some("path") => ObjectType::Path,
        // **兜底是 `Stroke`** ✗ —— 这曾经让 `"path"` 被**静默当成笔迹** ✓：
        // 对象建出来了 ✓、命令返回 ok ✓、渲染却是空的 ✗
        //（第 44 轮实测：`convert_to_path` 之后"逐像素对比差 3042 字节" ✓、`path_edit` 报"没有 points" ✓
        //  两个看起来无关的失败其实是**同一个**静默兜底造成的 ✓）。
        // 现在未知的**非空**类型会在提交层被**拒绝** ✓（见 `log.rs` 的 `validate_object_type` ✓），
        // 兜底只用于**没写 `type`** 的旧调用 ✓。
        _ => ObjectType::Stroke,
    }
}

fn object_type_for(kind: AtomKind, declared: Option<&str>) -> ObjectType {
    match kind {
        AtomKind::DrawShape => ObjectType::Shape,
        AtomKind::DrawText => ObjectType::Text,
        AtomKind::Retouch => ObjectType::Retouch,
        // 擦除也是一种"改像素"的对象：`ObjectType` 里没有 Erase 变体，按 `Retouch` 归类，
        // 并用 `retouch_type = "erase"` 指定具体行为（否则会落到默认的 **Stroke** ✗，
        // 被当成没有颜色的笔触 ⇒ 一点像素都不擦，这正是用户报告的现象）。
        AtomKind::Erase => ObjectType::Retouch,
        AtomKind::Liquify => ObjectType::Liquify,
        _ => parse_object_type(declared),
    }
}

fn object_data(kind: AtomKind, payload: &Value) -> Value {
    let mut data = payload
        .get("data")
        .cloned()
        .unwrap_or_else(|| payload.clone());
    // `erase` 原子由工具或客户端直接提交，通常不带 `retouch_type`；渲染层按
    // `data.retouch_type` 分派具体行为，所以这里**在折叠时**补上默认值 ✓。
    // 放在折叠层而不是工具层，是因为客户端可以经 `/api/atoms` 直提原子（查看器就是这么做的），
    // 只改工具层会漏掉那条路径 ✓。
    if kind == AtomKind::Erase {
        if let Some(object) = data.as_object_mut() {
            object
                .entry("retouch_type".to_owned())
                .or_insert_with(|| Value::String("erase".to_owned()));
        }
    }
    data
}

/// 解析实例**最终** master 的 `data` 与**合成变换** ✓，供 `sync_policy: none` 做快照 ✓。
///
/// **必须与渲染口径一致** ✓（`yanshi-render` 的 `resolve_instance` ✓）：
/// 变换次序是 master 自身 → 每层 `local_transform` → 实例自身 ✓，`pivot` 先折进矩阵再相乘 ✓。
/// 两处若各算一套 ✓，快照出来的画面就会与"跟随"时**不一样** ✗（用户一改策略画面就跳 ✓）。
fn resolve_master_snapshot(
    state: &DocumentState,
    instance_id: &str,
) -> Option<(Value, crate::state::Transform)> {
    fn flatten(transform: &crate::state::Transform) -> [f64; 6] {
        let m = transform.matrix;
        let (px, py) = (transform.pivot[0], transform.pivot[1]);
        [
            m[0],
            m[1],
            m[2],
            m[3],
            m[4] + px - (m[0] * px + m[2] * py),
            m[5] + py - (m[1] * px + m[3] * py),
        ]
    }
    fn compose(
        outer: &crate::state::Transform,
        inner: &crate::state::Transform,
    ) -> crate::state::Transform {
        let a = flatten(outer);
        let b = flatten(inner);
        let mut matrix = [0.0f64; 6];
        matrix[0] = a[0] * b[0] + a[2] * b[1];
        matrix[1] = a[1] * b[0] + a[3] * b[1];
        matrix[2] = a[0] * b[2] + a[2] * b[3];
        matrix[3] = a[1] * b[2] + a[3] * b[3];
        matrix[4] = a[0] * b[4] + a[2] * b[5] + a[4];
        matrix[5] = a[1] * b[4] + a[3] * b[5] + a[5];
        crate::state::Transform {
            matrix,
            pivot: [0.0, 0.0],
        }
    }
    let instance = state.objects.get(instance_id)?;
    let mut composed = instance.transform;
    let mut cursor = instance.clone();
    for _ in 0..64 {
        let master_ref = cursor.data.get("master_ref")?;
        let master_id = master_ref.get("object_id").and_then(Value::as_str)?;
        if let Some(local) = master_ref.get("local_transform") {
            let local = transform_from_payload(&serde_json::json!({ "transform": local }));
            composed = compose(&local, &composed);
        }
        let master = state.objects.get(master_id)?;
        if master.is_deleted() {
            return None;
        }
        if master.object_type == crate::state::ObjectType::Instance {
            composed = compose(&master.transform, &composed);
            cursor = master.clone();
            continue;
        }
        return Some((master.data.clone(), compose(&master.transform, &composed)));
    }
    None
}

/// **实例的成环检查** ✓（设计 9.3「循环引用检测：拒绝创建并返回错误」✓）。
///
/// 顺着 `master` 的链往上走 ✓：走到 `object_id` 自己 ⇒ 成环 ✓（含**自引用** ✓）；
/// 走过已经访问过的节点 ⇒ 链本身成环 ✓；超过 64 层 ⇒ 也按环处理 ✓（纵深防御 ✓）。
/// **只在这里判定** ✓ ⇒ 创建实例 ✓ 与改写 `master_ref` ✓ 两条入口行为必然一致 ✓。
pub(crate) fn check_instance_cycle(
    state: &DocumentState,
    object_id: &str,
    master: &str,
) -> Result<()> {
    let mut cursor = master.to_owned();
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if cursor == object_id {
            return Err(err(
                ErrorCode::InvalidArgument,
                format!("循环引用：实例 {object_id} 通过 {master} 回到了自己"),
            ));
        }
        if !visited.insert(cursor.clone()) {
            return Err(err(
                ErrorCode::InvalidArgument,
                format!("循环引用：{master} 所在的 master 链已经成环"),
            ));
        }
        if visited.len() > 64 {
            return Err(err(
                ErrorCode::InvalidArgument,
                "master 链过深（超过 64 层）—— 疑似循环引用".to_owned(),
            ));
        }
        match state.objects.get(&cursor) {
            Some(object) if object.object_type == crate::state::ObjectType::Instance => {
                let Some(next) = object
                    .data
                    .get("master_ref")
                    .and_then(|master_ref| master_ref.get("object_id"))
                    .and_then(Value::as_str)
                else {
                    break;
                };
                cursor = next.to_owned();
            }
            // 非实例（或尚不存在 ✓）：链到此为止 ✓。
            // **master 暂时不存在不算错** ✓ —— 引用在写入时必须存在 ✓（12.2 ✓），
            // 但**删除**它之后引用仍在 ✓，渲染时解析不到就什么都不画 ✓（见 `render.rs` ✓）。
            _ => break,
        }
    }
    Ok(())
}

/// **图层父链的成环检查** ✓（2026-10-28，审计 Batch 2）：`parent_id` 由载荷直接写入 ✓
/// ⇒ 不挡的话 `A.parent=B` ＋ `B.parent=A` 就是一个环 ✗ ——
/// 删除级联（`tombstone_layer` ✓）、`violations()` 与渲染侧的组遍历都要走父链 ✓，
/// 环会让它们**无限走**（`tombstone_layer` 实测会无限增长 `targets` ✗）。
///
/// **只在这里判定** ✓（与 `check_instance_cycle` 同形 ✓）：创建图层 ✓ 与 `SetProperty` 改写 ✓
/// 两条入口共用一份 ✓。**自引用**（`parent == 自己` ✓）、**走到自己** ✓、**父链本身已环** ✓、
/// **超过 64 层** ✓ 都按环拒绝 ✓（纵深防御与实例侧同口径 ✓）。
///
/// **∴ 两道闸都调它（与 `check_instance_cycle` 同形 ✓）**：
/// **提交期**在 `log::AtomLog::validate_layer_parent_cycles` ✓（那里返回 `Err` 才是**拒绝提交** ✓），
/// **重放期**在这里的 `precondition` ✓（手工写进日志的环会被跳过并留下警告 ✓）。
/// 只有重放期那一份是不够的 ✗：`precondition` 失败只记警告 ⇒ 原子被静默丢弃而工具返回 `ok: true` ✗。
///
/// **∴ 为什么写入闸之外 `violations()` 仍保留环检测 ✗**：
/// 它还得能指认**旧数据**里的环 ✓ ⇒ 两者不冲突 ✓。
pub(crate) fn check_layer_parent_cycle(
    state: &DocumentState,
    layer_id: &str,
    parent_id: &str,
) -> Result<()> {
    let mut cursor = parent_id.to_owned();
    let mut visited = std::collections::BTreeSet::new();
    loop {
        if cursor == layer_id {
            return Err(err(
                ErrorCode::InvalidArgument,
                format!("图层父链成环：{layer_id} 以 {parent_id} 为父层时会回到自己"),
            ));
        }
        if !visited.insert(cursor.clone()) {
            return Err(err(
                ErrorCode::InvalidArgument,
                format!("图层父链成环：{parent_id} 所在的父链已经成环"),
            ));
        }
        if visited.len() > 64 {
            return Err(err(
                ErrorCode::InvalidArgument,
                "图层父链过深（超过 64 层）—— 疑似成环".to_owned(),
            ));
        }
        match state
            .layers
            .get(&cursor)
            .and_then(|layer| layer.parent_id.clone())
        {
            Some(next) => cursor = next,
            None => break,
        }
    }
    Ok(())
}
/// 解析**平移**形式的组变换 ✓：`{dx, dy}` ✓（设计与 `move_object` 的增量命名一致 ✓）。
///
/// 其它形式（一般仿射矩阵 ✓）**显式拒绝** ✓ —— 它们需要设计 9.2 的派生解析 ✓，
/// 属于下一步 ✓；静默忽略会让用户以为生效了 ✗。
fn group_translation(value: &Value) -> Option<(f64, f64)> {
    let dx = value.get("dx").and_then(Value::as_f64)?;
    let dy = value.get("dy").and_then(Value::as_f64)?;
    if !dx.is_finite() || !dy.is_finite() {
        return None;
    }
    Some((dx, dy))
}

/// 几何原子的折叠应用（路线三）。
///
/// - 画布尺寸写 `new_width`/`new_height`。
/// - 存活对象（跳过已删除、实例、组）把几何仿射复合进 `transform` 字段：
///   实例的内容经由 master 的变换移动（实例合成里 master 在最外层），故实例
///   自身不复合，否则内容会被作用两次；组经由成员移动，组自身不复合。
/// - 锚点按仿射映射为新画布坐标（枢轴点自身不动，符合锚点 A 语义）。
/// - 选区与蒙版的形状按同一仿射变换；path 引用跟随对象，不动。
/// - 位图 patch：纯平移时按整数平移量移 bbox；恒等时不动；缩放/旋转/翻转时
///   工具层必须随原子提交 `mask_patches`（重采样结果），缺失即报错——
///   折叠层没有 blob 存储，无法自行重采样，静默保留旧 patch 等于错位。
/// - 原 patch 仍在日志历史里，撤销本原子即恢复（无需额外保留动作）。
fn apply_geometry(state: &mut DocumentState, atom: &Atom) -> Result<()> {
    use crate::geometry::*;
    let affine = transform_from_affine_payload(&atom.payload);
    let flat = flatten_transform(&affine);
    let new_width = payload_u64(&atom.payload, "new_width").unwrap_or(0) as u32;
    let new_height = payload_u64(&atom.payload, "new_height").unwrap_or(0) as u32;
    if new_width < 1 || new_height < 1 {
        return Err(err(
            ErrorCode::InvalidArgument,
            "几何操作的新画布尺寸必须 ≥ 1",
        ));
    }
    let is_translation = translation_of_flat(&flat);
    let is_identity = is_identity_flat(&flat);
    // 非平移仿射下，带 patch 的选区/蒙版必须在载荷里有重采样结果，先验一遍。
    if !is_identity && is_translation.is_none() {
        let patches_map = atom.payload.get("mask_patches");
        for (table, table_name) in [(&state.selections, "选区"), (&state.masks, "蒙版")] {
            for (id, selection) in table.iter() {
                if selection.is_deleted() || selection.patches.is_empty() {
                    continue;
                }
                let Some(list) = patches_map.and_then(|map| map.get(id)) else {
                    return Err(err(
                        ErrorCode::InvalidArgument,
                        format!(
                            "几何原子缺少{table_name} {id} 的重采样 patch（位图蒙版在缩放/旋转/翻转下必须由工具层重采样后随原子提交）"
                        ),
                    ));
                };
                parse_mask_patches(list)
                    .map_err(|reason| err(ErrorCode::InvalidArgument, reason))?;
            }
        }
    }
    // 单位仿射（改界）不移动内容：对象变换原样不动（不把非零 pivot 清零）。
    if !is_identity {
        for object in state.objects.values_mut() {
            if object.is_deleted() {
                continue;
            }
            if object.object_type == crate::state::ObjectType::Instance
                || object.object_type == crate::state::ObjectType::Group
            {
                continue;
            }
            let old_flat = flatten_transform(&object.transform);
            let composed = compose_flat(&flat, &old_flat);
            object.transform = crate::state::Transform {
                matrix: composed,
                pivot: [0.0, 0.0],
            };
        }
    }
    for anchor in state.anchors.values_mut() {
        let (x, y) = apply_flat(&flat, anchor.x, anchor.y);
        anchor.x = x;
        anchor.y = y;
    }
    for selection in state
        .selections
        .values_mut()
        .chain(state.masks.values_mut())
    {
        if selection.is_deleted() {
            continue;
        }
        // 单位仿射（改界）不移动内容：形状原样不动，连 JSON 表示都不重写。
        if !is_identity {
            transform_shape(&mut selection.shape, &flat);
        }
        if selection.patches.is_empty() || is_identity {
            continue;
        }
        if let Some((dx, dy)) = is_translation {
            if dx.fract() != 0.0 || dy.fract() != 0.0 {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    "位图 patch 的平移量必须是整数像素",
                ));
            }
            let (Ok(dx), Ok(dy)) = (i32::try_from(dx as i64), i32::try_from(dy as i64)) else {
                return Err(err(
                    ErrorCode::InvalidArgument,
                    "位图 patch 的平移量超出整数范围",
                ));
            };
            for patch in selection.patches.iter_mut() {
                patch.bbox[0] += dx;
                patch.bbox[1] += dy;
            }
        } else {
            let list = atom
                .payload
                .get("mask_patches")
                .and_then(|map| map.get(&selection.id))
                .expect("上一步已验过重采样 patch 存在");
            selection.patches = parse_mask_patches(list)
                .map_err(|reason| err(ErrorCode::InvalidArgument, reason))?;
        }
    }
    state.width = new_width;
    state.height = new_height;
    Ok(())
}

fn transform_from_payload(payload: &Value) -> Transform {
    let Some(value) = payload.get("transform") else {
        return Transform::IDENTITY;
    };
    if let Some(matrix) = value.get("matrix").and_then(Value::as_array) {
        if matrix.len() == 6 {
            let mut out = [0.0f64; 6];
            for (slot, item) in out.iter_mut().zip(matrix) {
                *slot = item.as_f64().unwrap_or(0.0);
            }
            let mut pivot = [0.0f64; 2];
            if let Some(p) = value.get("pivot").and_then(Value::as_array) {
                if p.len() == 2 {
                    pivot = [p[0].as_f64().unwrap_or(0.0), p[1].as_f64().unwrap_or(0.0)];
                }
            }
            return Transform { matrix: out, pivot };
        }
    }
    Transform::IDENTITY
}

fn parse_selection(
    atom: &Atom,
    id: String,
    blobs: Vec<crate::atom::BlobHash>,
) -> Result<Selection> {
    Ok(Selection {
        id,
        shape: atom.payload.get("shape").cloned().unwrap_or(Value::Null),
        feather: payload_f64(&atom.payload, "feather").unwrap_or(0.0),
        mode: payload_str(&atom.payload, "mode")
            .unwrap_or("new")
            .to_owned(),
        invert: payload_bool(&atom.payload, "invert").unwrap_or(false),
        linked_layer: payload_str(&atom.payload, "linked_layer").map(str::to_owned),
        refined_edges: payload_bool(&atom.payload, "refined_edges").unwrap_or(false),
        blobs,
        // Q2：`create_mask(from_selection)` / `create_selection(from_mask)` 互转时
        // 携带位图增量链（`paint_mask` 的 patches）。
        // **坏 `patches` 必须报错** ✓（第 710 轮；外部审计 R-13）
        //   **⇒ 缺陷**：原来 `.ok()` 把解析失败**静默**变成空 `Vec` ✗ ⇒ 携带位图增量的
        //     `create_mask(from_selection)`／`create_selection(from_mask)` 会**丢掉整条增量链** ✓
        //     而调用方看到 `ok:true` ✗（对比 `parse_mask_patch` 的严格口径 ✓）。
        patches: match atom.payload.get("patches") {
            None | Some(Value::Null) => Vec::new(),
            Some(value) => serde_json::from_value(value.clone()).map_err(|error| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "patches 解析失败 ⇒ 拒绝静默丢弃整条位图增量链（{}）",
                        error
                    )),
                )
                .with_object(atom.id.clone())
            })?,
        },
        created_by: atom.id.clone(),
        deleted_by: None,
    })
}

/// 解析 `UpdateMask` 原子 payload 里的 `patch` 对象。
///
/// 缺字段/非法值直接报错（不静默降级：坏 patch 进链会导致渲染期跳过，
/// 调用方应立刻知道）。
fn parse_mask_patch(value: &Value) -> Result<MaskPatchMeta> {
    let bad =
        |detail: &str| YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(detail));
    let bbox_array = value
        .get("bbox")
        .and_then(Value::as_array)
        .ok_or_else(|| bad("UpdateMask 的 patch 缺少 bbox（[x, y, w, h]）"))?;
    if bbox_array.len() != 4 {
        return Err(bad("UpdateMask 的 patch.bbox 必须是 4 元组"));
    }
    let mut bbox = [0i32; 4];
    for (i, item) in bbox_array.iter().enumerate() {
        bbox[i] =
            item.as_i64()
                .ok_or_else(|| bad("UpdateMask 的 patch.bbox 元素必须是整数"))? as i32;
    }
    let target_u8 = value
        .get("target_u8")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| bad("UpdateMask 的 patch 缺少 target_u8（0..255）"))?;
    let op: MaskPatchOp =
        serde_json::from_value(value.get("op").cloned().unwrap_or(Value::Null))
            .map_err(|_| bad("UpdateMask 的 patch.op 非法（lerp_deposit / replace_all）"))?;
    let blob: crate::atom::BlobHash = value
        .get("blob")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| bad("UpdateMask 的 patch 缺少合法 blob 哈希"))?;
    Ok(MaskPatchMeta {
        bbox,
        target_u8,
        op,
        blob,
    })
}

fn supersede_object(
    state: &mut DocumentState,
    atom: &Atom,
    object_id: &str,
    blobs: Option<Vec<crate::atom::BlobHash>>,
) -> Result<()> {
    let Some(object) = state.objects.get_mut(object_id) else {
        return Err(missing_error("对象", object_id, state.objects.keys()));
    };
    if object.is_deleted() {
        return Err(tombstone_error("对象", object_id));
    }
    object.versions.push(atom.id.clone());
    object.current_version = Some(atom.id.clone());
    if atom.payload.get("data").is_some() {
        object.data = object_data(atom.kind, &atom.payload);
    }
    if let Some(layer_id) = payload_str(&atom.payload, "layer_id") {
        object.layer_id = layer_id.to_owned();
    }
    if let Some(z_index) = payload_u64(&atom.payload, "z_index") {
        object.z_index = z_index as i64;
    }
    if let Some(blobs) = blobs {
        object.blobs = blobs;
    }
    Ok(())
}

fn apply_property_to_object(object: &mut Object, key: &str, value: &Value) {
    match key {
        // **非布尔 ⇒ 保持旧值** ✗→✓（第 1440 轮审计）：原来 `unwrap_or(true)`
        // 会把 `"visible": "yes"` 静默画成**显示** —— 画错东西且无信号，
        // 正是本仓禁忌的静默降级。提交层现已直接拒绝非法值（`log.rs` 的
        // `validate_property_values`），这里对**手工日志**保持"不动"而非"猜默认" ✓。
        "visible" => {
            if let Some(flag) = value.as_bool() {
                object.visible = flag;
            }
        }
        // G-03 bypass：与 `visible` 同纪律，非布尔保持旧值。
        "enabled" => {
            if let Some(flag) = value.as_bool() {
                object.enabled = flag;
            }
        }
        "locked" => {
            if let Some(flag) = value.as_bool() {
                object.locked = flag;
            }
        }
        "z_index" => object.z_index = value.as_i64().unwrap_or(object.z_index),
        "layer_id" => {
            if let Some(layer_id) = value.as_str() {
                object.layer_id = layer_id.to_owned();
            }
        }
        "type" => {
            if let Some(kind) = value.as_str() {
                object.object_type = parse_object_type(Some(kind));
            }
        }
        "metadata" => object.metadata = value.clone(),
        "transform" => {
            let wrapper = json!({ "transform": value });
            object.transform = transform_from_payload(&wrapper);
        }
        other => {
            if let Some(map) = object.data.as_object_mut() {
                map.insert(other.to_owned(), value.clone());
            } else if let Some(object_map) = as_object_mut(&mut object.metadata) {
                object_map.insert(other.to_owned(), value.clone());
            }
        }
    }
}

fn as_object_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut()
}

fn apply_property_to_layer(layer: &mut Layer, key: &str, value: &Value) {
    match key {
        // 与对象侧同纪律：**非布尔保持旧值**，不 `unwrap_or` 猜默认 ✓（第 1440 轮）。
        "visible" => {
            if let Some(flag) = value.as_bool() {
                layer.visible = flag;
            }
        }
        "locked" => {
            if let Some(flag) = value.as_bool() {
                layer.locked = flag;
            }
        }
        "alpha_lock" => {
            if let Some(flag) = value.as_bool() {
                layer.alpha_lock = flag;
            }
        }
        "clipping_mask" => {
            if let Some(flag) = value.as_bool() {
                layer.clipping_mask = flag;
            }
        }
        "opacity" => layer.opacity = value.as_f64().unwrap_or(layer.opacity),
        "blend_mode" => {
            if let Some(mode) = value.as_str() {
                layer.blend_mode = mode.to_owned();
            }
        }
        "z_index" => layer.z_index = value.as_i64().unwrap_or(layer.z_index),
        "name" => {
            if let Some(name) = value.as_str() {
                layer.name = name.to_owned();
            }
        }
        "parent_id" => layer.parent_id = value.as_str().map(str::to_owned),
        "mask_id" => layer.mask_id = value.as_str().map(str::to_owned),
        "type" => layer.layer_type = parse_layer_type(value.as_str()),
        "metadata" => layer.metadata = value.clone(),
        other => {
            if let Some(map) = as_object_mut(&mut layer.metadata) {
                map.insert(other.to_owned(), value.clone());
            }
        }
    }
}

/// 图层 tombstone 级联：后代图层与其中的对象一并标记，避免产生孤儿引用。
fn tombstone_layer(state: &mut DocumentState, layer_id: &str, atom_id: &str) {
    // **★ 必须去重 ✗ ★**（2026-10-28，审计 Batch 2）：父链可以成环 ✓（旧数据／直接写日志的原子 ✓）
    // ⇒ 没有 `seen` 时 `targets` 会**无限增长**（同一对父子每轮各加一次 ✓）⇒ 删除一个图层就把进程拖死 ✗。
    // 写入闸（`check_layer_parent_cycle` ✓）只保证**新**原子不成环 ✗ —— 旧文档里的环仍会走到这里 ✓。
    let mut targets = vec![layer_id.to_owned()];
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    seen.insert(layer_id.to_owned());
    let mut cursor = 0;
    while cursor < targets.len() {
        let current = targets[cursor].clone();
        cursor += 1;
        for layer in state.layers.values() {
            if layer.parent_id.as_deref() == Some(current.as_str()) && seen.insert(layer.id.clone())
            {
                targets.push(layer.id.clone());
            }
        }
    }
    for target in &targets {
        if let Some(layer) = state.layers.get_mut(target) {
            if layer.deleted_by.is_none() {
                layer.deleted_by = Some(atom_id.to_owned());
            }
        }
        let object_ids: Vec<String> = state
            .objects
            .values()
            .filter(|o| &o.layer_id == target && o.deleted_by.is_none())
            .map(|o| o.id.clone())
            .collect();
        for object_id in object_ids {
            if let Some(object) = state.objects.get_mut(&object_id) {
                object.deleted_by = Some(atom_id.to_owned());
            }
        }
    }
}

fn payload_ids(payload: &Value, key: &str) -> Result<Vec<String>> {
    let Some(array) = payload.get(key).and_then(Value::as_array) else {
        return Err(err(
            ErrorCode::InvalidArgument,
            format!("字段 {key} 必须是 id 数组"),
        ));
    };
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| err(ErrorCode::InvalidArgument, format!("{key} 含非字符串元素")))
        })
        .collect()
}

#[cfg(test)]
mod tests {

    fn bake_atom(id: &str, seq: Seq, payload: serde_json::Value) -> Atom {
        let mut a = Atom::new(
            AtomKind::BakeLayer,
            "actor".to_owned(),
            "session".to_owned(),
            payload,
        );
        a.id = id.to_owned();
        a.seq = seq;
        a
    }

    fn bake_fixture_state() -> DocumentState {
        // 最小文档：建文档 + 建层 + 两笔（用 apply 直推，避免工具层）。
        let mut state = DocumentState::default();
        let mut doc = Atom::new(
            AtomKind::CreateDocument,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"doc_id": "d1", "width": 16, "height": 16}),
        );
        doc.id = "a_doc".to_owned();
        doc.seq = 1;
        apply(&mut state, &doc).unwrap();
        let mut layer = Atom::new(
            AtomKind::CreateLayer,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"layer_id": "L1", "name": "L1"}),
        );
        layer.id = "a_layer".to_owned();
        layer.seq = 2;
        apply(&mut state, &layer).unwrap();
        for (i, seq) in [(1, 3), (2, 4)] {
            let mut stroke = Atom::new(
                AtomKind::DrawStroke,
                "a".to_owned(),
                "s".to_owned(),
                serde_json::json!({"object_id": format!("o{i}"), "layer_id": "L1"}),
            );
            stroke.id = format!("a_stroke{i}");
            stroke.seq = seq;
            apply(&mut state, &stroke).unwrap();
        }
        state
    }

    #[test]
    fn bake_horizon_skips_pre_bake_atoms_and_bake_replaces_objects() {
        let base = bake_fixture_state();
        assert_eq!(base.objects_in_layer("L1").len(), 2);
        let bake = bake_atom(
            "a_bake",
            5,
            serde_json::json!({"layer_id": "L1", "up_to_seq": 4, "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"}),
        );
        // 全量折叠：先有两笔，再有 BakeLayer；两笔被剪枝，BakeLayer 生效。
        let mut atoms: Vec<Atom> = Vec::new();
        let mut doc = Atom::new(
            AtomKind::CreateDocument,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"doc_id": "d1", "width": 16, "height": 16}),
        );
        doc.id = "a_doc".to_owned();
        doc.seq = 1;
        atoms.push(doc);
        let mut layer = Atom::new(
            AtomKind::CreateLayer,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"layer_id": "L1", "name": "L1"}),
        );
        layer.id = "a_layer".to_owned();
        layer.seq = 2;
        atoms.push(layer);
        for (i, seq) in [(1, 3), (2, 4)] {
            let mut stroke = Atom::new(
                AtomKind::DrawStroke,
                "a".to_owned(),
                "s".to_owned(),
                serde_json::json!({"object_id": format!("o{i}"), "layer_id": "L1"}),
            );
            stroke.id = format!("a_stroke{i}");
            stroke.seq = seq;
            atoms.push(stroke);
        }
        atoms.push(bake);
        let result = fold_atoms(DocumentState::default(), &atoms);
        let objs = result.state.objects_in_layer("L1");
        assert_eq!(objs.len(), 1, "压平后层内只剩单个基底对象");
        assert_eq!(
            objs[0].data.get("baked_base").and_then(|v| v.as_bool()),
            Some(true)
        );
        // 两笔被剪枝：applied 里没有它们，BakeLayer 在。
        assert!(!result.applied.contains(&"a_stroke1".to_owned()));
        assert!(result.applied.contains(&"a_bake".to_owned()));
        // Undo BakeLayer（revert 后全量重折）：两笔恢复，基底消失。
        let mut atoms2 = atoms.clone();
        let mut revert = Atom::new(
            AtomKind::Revert,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"target": "a_bake"}),
        );
        revert.id = "a_revert".to_owned();
        revert.seq = 6;
        atoms2.push(revert);
        let result2 = fold_atoms(DocumentState::default(), &atoms2);
        assert_eq!(result2.state.objects_in_layer("L1").len(), 2);
    }

    #[test]
    fn bake_shell_atoms_are_not_pruned() {
        // 建层原子 seq <= horizon 时仍执行：否则 BakeLayer 无层可落。
        let mut atoms: Vec<Atom> = Vec::new();
        let mut doc = Atom::new(
            AtomKind::CreateDocument,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"doc_id": "d1", "width": 16, "height": 16}),
        );
        doc.id = "a_doc".to_owned();
        doc.seq = 1;
        atoms.push(doc);
        let mut layer = Atom::new(
            AtomKind::CreateLayer,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"layer_id": "L1", "name": "L1"}),
        );
        layer.id = "a_layer".to_owned();
        layer.seq = 2;
        atoms.push(layer);
        atoms.push(bake_atom("a_bake", 3, serde_json::json!({"layer_id": "L1", "up_to_seq": 2, "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"})));
        let result = fold_atoms(DocumentState::default(), &atoms);
        assert!(result.state.layer_alive("L1"));
        assert_eq!(result.state.objects_in_layer("L1").len(), 1);
    }

    #[test]
    fn baked_base_gate_rejects_45_degrees_in_precondition() {
        // 兜底门禁：45° 绝对变换在 precondition 即失败；90° 通过。
        // （工具层门禁是用户可见的拒绝；fold 门禁只作回放兜底——
        // precondition 失败在折叠里是级联警告，不是提交失败。）
        let mut atoms: Vec<Atom> = Vec::new();
        let mut doc = Atom::new(
            AtomKind::CreateDocument,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"doc_id": "d1", "width": 16, "height": 16}),
        );
        doc.id = "a_doc".to_owned();
        doc.seq = 1;
        atoms.push(doc);
        let mut layer = Atom::new(
            AtomKind::CreateLayer,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"layer_id": "L1", "name": "L1"}),
        );
        layer.id = "a_layer".to_owned();
        layer.seq = 2;
        atoms.push(layer);
        atoms.push(bake_atom("a_bake", 3, serde_json::json!({"layer_id": "L1", "up_to_seq": 2, "blob_hash": "sha256:0000000000000000000000000000000000000000000000000000000000000000"})));
        let baked = fold_atoms(DocumentState::default(), &atoms).state;
        assert_eq!(baked.objects_in_layer("L1").len(), 1);
        let mut mv45 = Atom::new(
            AtomKind::Move,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"object_id": "baked:L1", "transform": {"matrix": [std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2, 0.0, 0.0]}}),
        );
        mv45.id = "a_mv45".to_owned();
        mv45.seq = 4;
        assert!(
            precondition(&baked, &mv45).is_err(),
            "45° 变换必须被门禁拒绝"
        );
        let mut mv90 = Atom::new(
            AtomKind::Move,
            "a".to_owned(),
            "s".to_owned(),
            serde_json::json!({"object_id": "baked:L1", "transform": {"matrix": [0.0, 1.0, -1.0, 0.0, 0.0, 0.0]}}),
        );
        mv90.id = "a_mv90".to_owned();
        mv90.seq = 4;
        assert!(precondition(&baked, &mv90).is_ok(), "90° 变换必须通过门禁");
    }
    /// **坏 `patches` 不许被静默丢弃** ✓（第 710 轮；外部审计 R-13）
    ///
    /// **⇒ 判据**：`create_selection` 带上**格式错误**的 `patches` ⇒ `apply` 必须返回
    ///   `InvalidArgument` ✓；`patches` 缺失或为 `null` ⇒ 仍是空 `Vec`（老日志兼容 ✓）。
    /// **变异**：把解析改回 `.ok()`（静默丢弃）⇒ 第一条红 ✓。
    #[test]
    fn a_malformed_patches_chain_is_rejected_not_silently_dropped() {
        use super::*;
        let mut state = DocumentState::empty();
        state.doc_id = Some("doc_r13".to_owned());
        let bad = json!({
            "selection_id": "sel_bad",
            "shape": {"kind": "rect", "x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0},
            "patches": "this is not a patch list",
        });
        let error = apply(&mut state, &atom(AtomKind::CreateSelection, "a_bad", bad))
            .expect_err("坏 patches 必须报错（不许静默丢弃整条增量链）");
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{error:?}");
        // 缺字段 ⇒ 允许（老日志没有 patches ✓）
        let ok = json!({
            "selection_id": "sel_ok",
            "shape": {"kind": "rect", "x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0},
        });
        apply(&mut state, &atom(AtomKind::CreateSelection, "a_ok", ok))
            .expect("没有 patches 的老日志必须照常应用");
        assert!(state.selections.contains_key("sel_ok"));
    }

    use super::*;

    fn atom(kind: AtomKind, id: &str, payload: Value) -> Atom {
        Atom::new(kind, "human:1", "session:a", payload).with_id(id)
    }

    /// 追加并分配权威 seq（与 `AtomLog::append` 一致：从 1 开始）。
    fn push(atoms: &mut Vec<Atom>, mut atom: Atom) {
        atom.seq = atoms.len() as Seq + 1;
        atoms.push(atom);
    }

    fn fold(atoms: &[Atom]) -> FoldResult {
        fold_atoms(DocumentState::empty(), atoms)
    }

    /// 选区也能被 tombstone 删除（`apply` 的 Tombstone 分支支持 selection_id ✓）。
    /// 这条测试是为了定位一个真实症状：工具层提交了 `{"selection_id": ...}` 的 tombstone，
    /// 日志里也确实有该原子，但选区的 `deleted_by` 仍是 None ✗。
    #[test]
    fn tombstone_marks_a_selection_deleted() {
        let mut atoms = Vec::new();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({"selection_id": "sel_1",
                       "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}}}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_sel_del",
                json!({"selection_id": "sel_1"}),
            ),
        );
        let result = fold(&atoms);
        for warning in &result.warnings {
            eprintln!("调试：折叠警告 {:?}", warning);
        }
        let selection = result
            .state
            .selections
            .get("sel_1")
            .expect("选区应存在于状态中");
        assert!(
            selection.deleted_by.is_some(),
            "tombstone 必须把选区标记为已删除（deleted_by）"
        );
    }

    /// **墓碑不该占用 id** ✓（真机复现的 bug ✓）：
    /// `delete_selection` 之后 ✓ `list_selections` 报 0 条 ✓（正确 ✓）——
    /// 但用**同一个 id** 再 `create_selection` 却回「选区 Q1 已存在」✗
    /// ⇒ 两句自相矛盾 ✓、而且**没有任何出路**（只能弃用整个文档 ✓）。
    /// 根因是这里的检查写成了 `contains_key` ✓ ⇒ **把墓碑也算成占用** ✗；
    /// 而 `CreateLayer` 早就改成"只与存活的冲突" ✓（见本文件 `CreateLayer` 的注释 ✓）——
    /// **本条测试锁住"同一个教训不能只学一次"** ✓（蒙版／风格同形 ✓）。
    #[test]
    fn a_deleted_selection_or_mask_frees_its_id_for_reuse() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({"selection_id": "sel_1",
                       "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}}}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_sel_del",
                json!({"selection_id": "sel_1"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateMask,
                "a_mask",
                json!({"mask_id": "mask_1",
                       "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}}}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_mask_del",
                json!({"mask_id": "mask_1"}),
            ),
        );
        let folded = fold(&atoms);
        assert!(folded.is_clean(), "{:?}", folded.warnings);
        let mut state = folded.state;
        assert!(!state.selection_alive("sel_1"), "折叠后该选区应是墓碑");
        assert!(!state.mask_alive("mask_1"), "折叠后该蒙版应是墓碑");

        // 同一个 id **重新创建必须被接受** ✓（这才是"删除可撤销、但 id 不报废"的语义 ✓）。
        let again = atom(
            AtomKind::CreateSelection,
            "a_sel_again",
            json!({"selection_id": "sel_1",
                   "shape": {"kind": "rect", "bbox": {"x": 4, "y": 4, "w": 6, "h": 6}}}),
        );
        precondition(&state, &again)
            .expect("已删除的选区 id 必须可以重新创建（与 CreateLayer 同一条规则）");
        apply(&mut state, &again).expect("重建选区应能应用");
        assert!(
            state.selection_alive("sel_1"),
            "重建后必须复活（deleted_by 清掉）"
        );

        let mask_again = atom(
            AtomKind::CreateMask,
            "a_mask_again",
            json!({"mask_id": "mask_1",
                   "shape": {"kind": "rect", "bbox": {"x": 1, "y": 1, "w": 4, "h": 4}}}),
        );
        precondition(&state, &mask_again).expect("已删除的蒙版 id 必须可以重新创建");

        // 但**存活**的那一个仍然要拒 ✗ —— 否则就是无声覆盖用户正在用的选区 ✓。
        let alive = atom(
            AtomKind::CreateSelection,
            "a_sel_dup",
            json!({"selection_id": "sel_1",
                   "shape": {"kind": "rect", "bbox": {"x": 9, "y": 9, "w": 2, "h": 2}}}),
        );
        let error = precondition(&state, &alive).expect_err("存活的选区必须拒绝同名创建");
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("已存在"),
            "撞的是存活实体 ⇒ 文案应说「已存在」，实得：{detail}"
        );
    }

    /// create_document → create_layer → create_object
    fn chain() -> Vec<Atom> {
        let mut atoms = Vec::new();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateDocument,
                "a_doc",
                json!({"doc_id": "doc_1", "width": 100, "height": 50}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_layer",
                json!({"layer_id": "layer_1", "name": "L1"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        atoms
    }

    #[test]
    fn fold_builds_document_state() {
        let atoms = chain();
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!(result.state.doc_id.as_deref(), Some("doc_1"));
        assert_eq!(result.state.width, 100);
        assert_eq!(result.state.height, 50);
        assert_eq!(result.state.head_seq, 3);
        assert_eq!(result.state.head_atom.as_deref(), Some("a_obj"));
        assert!(result.state.is_consistent());
        assert_eq!(result.applied.len(), 3);
    }

    #[test]
    fn create_object_on_tombstone_error_names_the_way_out() {
        // 期 1 E-b 顺带修：fold 层 CreateObject 撞墓碑时，文案必须点名
        // 状态（已被删除）与出路（restore_object／支持复活的写入路径），
        // 不许再笼统报「已存在」——那与「落笔报已删除」自相矛盾。
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_obj_del",
                json!({"object_id": "obj_1"}),
            ),
        );
        let state = fold(&atoms).state;
        let recreate = atom(
            AtomKind::CreateObject,
            "a_obj_again",
            json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
        );
        let error = precondition(&state, &recreate)
            .expect_err("墓碑 id 的 CreateObject 必须在 fold 层被拒（复活只走工具层）");
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("已被删除"),
            "文案必须点名墓碑状态，实得：{detail}"
        );
        assert!(
            detail.contains("restore_object"),
            "文案必须给出恢复出路，实得：{detail}"
        );
    }

    #[test]
    fn fold_is_idempotent() {
        let atoms = chain();
        let first = fold(&atoms);
        let second = fold(&atoms);
        assert_eq!(first.state, second.state);
        assert_eq!(first.applied, second.applied);
    }

    #[test]
    fn revert_removes_effect_and_reapply_restores_target_only() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"size": 10}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_revert", json!({"target": "a_super"})),
        );
        let reverted = fold(&atoms);
        assert!(reverted.is_suppressed("a_super"));
        assert_eq!(
            reverted.state.objects["obj_1"].current_version.as_deref(),
            Some("a_obj")
        );
        assert!(reverted.state.is_consistent());

        push(
            &mut atoms,
            atom(AtomKind::Reapply, "a_reapply", json!({"target": "a_super"})),
        );
        let reapplied = fold(&atoms);
        assert!(!reapplied.is_suppressed("a_super"));
        assert_eq!(
            reapplied.state.objects["obj_1"].current_version.as_deref(),
            Some("a_super")
        );
        assert_eq!(reapplied.state.objects["obj_1"].data, json!({"size": 10}));
    }

    #[test]
    fn revert_of_revert_equals_reapply() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r1", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r2", json!({"target": "a_r1"})),
        );

        let result = fold(&atoms);
        assert!(result.is_suppressed("a_r1"), "revert 被 revert 后自身失效");
        assert!(!result.is_suppressed("a_super"), "等价于 reapply(a_super)");
        assert_eq!(
            result.state.objects["obj_1"].current_version.as_deref(),
            Some("a_super")
        );
    }

    /// **revert(reapply(x)) ≡ 维持 x 被抑制** ✓（第 1440 轮审计锁）。
    ///
    /// 提交层已允许 `revert` 指向 `Reapply` ✓（`log.rs` 的 `revertible` ✓），
    /// 但折叠层原来**无条件登记** Reapply ⇒ `draw→revert→reapply→revert(reapply)`
    /// 折完笔画**仍生效** ✗（撤销按钮按下去没有效果 —— undo/redo 不对称 ✓）。
    /// 变异：把 `Reapply` 分支的 `revert_of_reapply` 体检删掉 ⇒ 本测试红 ✓。
    #[test]
    fn revert_of_reapply_suppresses_target_again() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r1", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Reapply, "a_ap", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r2", json!({"target": "a_ap"})),
        );

        let result = fold(&atoms);
        assert!(
            result.is_suppressed("a_ap"),
            "reapply 自己被 revert ⇒ 它应当失效"
        );
        assert!(
            result.is_suppressed("a_super"),
            "更早的 revert(a_r1) 重新胜出 ⇒ a_super 必须回到被抑制"
        );
        assert_ne!(
            result.state.objects["obj_1"].data,
            json!({"v": 2}),
            "a_super 被抑制 ⇒ data 不许是它的版本"
        );
    }

    /// G-03 bypass：`SetProperty` 能开关 `enabled`；缺省启用；非法值保持旧值。
    /// 变异：把 `apply_property_to_object` 的 enabled 分支删掉 ⇒ 红 ✓。
    #[test]
    fn set_property_enabled_toggles_bypass() {
        let mut atoms = chain();
        let result = fold(&atoms);
        assert!(
            result.state.objects["obj_1"].enabled,
            "新建对象缺省启用（enabled=true）"
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_disable",
                json!({"object_id": "obj_1", "key": "enabled", "value": false}),
            ),
        );
        let result = fold(&atoms);
        assert!(
            !result.state.objects["obj_1"].enabled,
            "enabled=false 必须落到对象上"
        );
        // 手工日志里的非法值（提交层会拒绝，这里是重放纵深防御 ✓）：
        // 与 visible 同纪律，非布尔保持旧值，不许猜默认。
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_junk",
                json!({"object_id": "obj_1", "key": "enabled", "value": "yes"}),
            ),
        );
        let result = fold(&atoms);
        assert!(
            !result.state.objects["obj_1"].enabled,
            "非法值必须保持旧值（停用），不许猜默认 ✗"
        );
    }

    /// **非布尔的属性值不许翻转状态** ✓（第 1440 轮审计锁）。
    /// 变异：把 `apply_property_to_object` 的 visible 分支改回 `unwrap_or(true)` ⇒ 红 ✓。
    #[test]
    fn set_property_invalid_bool_keeps_old_value() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_hide",
                json!({"object_id": "obj_1", "key": "visible", "value": false}),
            ),
        );
        // 手工日志里的非法值（提交层会拒绝，这里是重放纵深防御 ✓）：
        // 旧实现 `unwrap_or(true)` 会把隐藏的对象**静默画成显示** ✗。
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_junk",
                json!({"object_id": "obj_1", "key": "visible", "value": "yes"}),
            ),
        );
        let result = fold(&atoms);
        assert!(
            !result.state.objects["obj_1"].visible,
            "非法值必须保持旧值（隐藏），不许猜默认 ✗"
        );
    }

    /// 增量维护的有效动作表必须与整表重算**逐位一致**。
    ///
    /// 含"撤销撤销"（`revert(revert(x))`）——`extend` 对这类目标返回 `None`
    /// （级联长度不定），此时调用方整表重算，这里也把那条路验一遍。
    #[test]
    fn effective_actions_extend_matches_full_scan() {
        let mut full = chain();
        push(
            &mut full,
            atom(
                AtomKind::Supersede,
                "a_super1",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut full,
            atom(
                AtomKind::CreateObject,
                "a_obj2",
                json!({"object_id": "obj_2", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut full,
            atom(AtomKind::Revert, "a_r1", json!({"target": "a_super1"})),
        );
        push(
            &mut full,
            atom(AtomKind::Reapply, "a_re1", json!({"target": "a_obj2"})),
        );
        push(
            &mut full,
            atom(AtomKind::Revert, "a_r2", json!({"target": "a_r1"})),
        );
        push(
            &mut full,
            atom(AtomKind::Revert, "a_r3", json!({"target": "a_obj2"})),
        );
        push(
            &mut full,
            atom(AtomKind::Reapply, "a_re2", json!({"target": "a_super1"})),
        );
        push(
            &mut full,
            atom(AtomKind::Revert, "a_r4", json!({"target": "a_doc"})),
        );
        let log = crate::log::AtomLog::with_atoms(full.clone()).unwrap();

        // 每 2 个原子前推一次，与整表重算比对。
        let mut incremental = EffectiveActions::default();
        for chunk in full.chunks(2) {
            if incremental.extend(chunk, &log, 0).is_none() {
                // "撤销撤销"⇒ 调用方整表重算（这里重算到该 chunk 末尾）。
                let upto = chunk.last().unwrap().seq;
                incremental = EffectiveActions::from_atoms(log.atoms_upto(upto));
            }
            let upto = chunk.last().unwrap().seq;
            let reference = EffectiveActions::from_atoms(log.atoms_upto(upto));
            assert_eq!(
                incremental.suppressed(),
                reference.suppressed(),
                "seq {upto} 处的撤销集合必须一致"
            );
        }
        assert_eq!(
            incremental.suppressed(),
            &compute_suppressed(full.as_slice()),
            "整条日志上也要与 compute_suppressed 一致"
        );
    }

    /// 未生效（被撤销）的 revert 不登记自己的动作 —— `revert(revert(x)) ≡ reapply(x)`。
    #[test]
    fn effective_actions_track_revert_of_revert() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r1", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r2", json!({"target": "a_r1"})),
        );

        let actions = EffectiveActions::from_atoms(&atoms);
        assert_eq!(actions.action_of("a_r1"), Some(RevertAction::Revert));
        assert_eq!(
            actions.action_of("a_super"),
            None,
            "r1 失效后不再登记它对 a_super 的动作"
        );
        assert!(actions.suppressed().contains("a_r1"));
        assert!(!actions.suppressed().contains("a_super"));
        assert_eq!(actions.suppressed(), &compute_suppressed(&atoms));
    }

    /// `fold_atoms_skipping` 在传入"由外面算出的有效集"时必须与 `fold_atoms` 等价。
    #[test]
    fn fold_atoms_skipping_matches_fold_atoms() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r1", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj2",
                json!({"object_id": "obj_2", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );

        let direct = fold_atoms(DocumentState::empty(), &atoms);
        let skipped =
            fold_atoms_skipping(DocumentState::empty(), &atoms, &compute_suppressed(&atoms));
        assert_eq!(direct.state, skipped.state);
        assert_eq!(direct.suppressed, skipped.suppressed);
        assert_eq!(direct.warnings, skipped.warnings);
        assert_eq!(direct.applied, skipped.applied);

        // 逐原子前推（增量折叠器建还原点走的就是这条路）也必须收敛到同一个结果。
        let mut state = DocumentState::empty();
        let mut warnings = Vec::new();
        for a in &atoms {
            let one = fold_atoms_skipping(state, std::slice::from_ref(a), &direct.suppressed);
            state = one.state;
            warnings.extend(one.warnings);
        }
        assert_eq!(state, direct.state, "逐原子前推必须与一次折完一致");
        assert_eq!(warnings, direct.warnings);
    }

    #[test]
    fn later_revert_wins_over_earlier_reapply() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Reapply, "a_re", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_rev", json!({"target": "a_super"})),
        );
        let result = fold(&atoms);
        assert!(result.is_suppressed("a_super"), "seq 更大的 revert 胜出");
    }

    #[test]
    fn cascade_invalidation_is_recorded_not_applied() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(AtomKind::Tombstone, "a_del", json!({"object_id": "obj_1"})),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Revert,
                "a_revert_create",
                json!({"target": "a_obj"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_prop",
                json!({"object_id": "obj_1", "key": "visible", "value": false}),
            ),
        );

        let result = fold(&atoms);
        assert!(result.is_suppressed("a_obj"));
        assert!(
            !result.is_suppressed("a_del"),
            "tombstone 本身没有被 revert"
        );
        assert_eq!(result.warning_count(WarningKind::CascadeInvalidation), 2);
        let warned: Vec<&str> = result.warnings.iter().map(|w| w.atom_id.as_str()).collect();
        assert!(warned.contains(&"a_prop"));
        assert!(warned.contains(&"a_del"));
        assert!(!result.state.objects.contains_key("obj_1"));
        assert!(result.state.is_consistent());
    }

    #[test]
    fn extreme_phase0_sequence_has_no_orphans() {
        // create → supersede → revert(supersede) → revert(create) → reapply(create)
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::Supersede,
                "a_super",
                json!({"object_id": "obj_1", "data": {"v": 2}}),
            ),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r_super", json!({"target": "a_super"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_r_create", json!({"target": "a_obj"})),
        );
        push(
            &mut atoms,
            atom(AtomKind::Reapply, "a_re_create", json!({"target": "a_obj"})),
        );

        let result = fold(&atoms);
        assert!(!result.is_suppressed("a_obj"), "reapply 恢复创建原子");
        assert!(
            result.is_suppressed("a_super"),
            "revert(supersede) 之后 supersede 失效"
        );
        assert!(
            !result.is_suppressed("a_r_create"),
            "revert 自身没有被 revert"
        );
        assert!(
            result.state.is_consistent(),
            "{:?}",
            result.state.violations()
        );
        assert!(result.state.objects.contains_key("obj_1"));
        assert_eq!(
            result.state.objects["obj_1"].current_version.as_deref(),
            Some("a_obj"),
            "supersede 仍处于撤销状态，当前版本回到创建原子"
        );
        assert_eq!(result.state.objects["obj_1"].versions, vec!["a_obj"]);
    }

    #[test]
    fn tombstone_keeps_history_but_hides_object() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(AtomKind::Tombstone, "a_del", json!({"object_id": "obj_1"})),
        );
        let result = fold(&atoms);
        let object = &result.state.objects["obj_1"];
        assert!(object.is_deleted());
        assert_eq!(object.deleted_by.as_deref(), Some("a_del"));
        assert!(result.state.alive_objects().is_empty());
        assert!(result.state.is_consistent());
    }

    #[test]
    fn layer_tombstone_cascades_to_children_and_objects() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_group",
                json!({"layer_id": "group_1", "type": "layer_group"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_child",
                json!({"layer_id": "layer_child", "parent_id": "group_1"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_child_obj",
                json!({"object_id": "obj_child", "layer_id": "layer_child", "type": "shape"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_del_group",
                json!({"layer_id": "group_1"}),
            ),
        );

        let result = fold(&atoms);
        assert!(!result.state.layer_alive("group_1"));
        assert!(!result.state.layer_alive("layer_child"));
        assert!(!result.state.object_alive("obj_child"));
        assert!(result.state.is_consistent());
    }

    /// **父链成环的改写必须在折叠期被跳过** ✓（2026-10-28，审计 Batch 2）。
    ///
    /// 提交闸（`log.rs` 的 `validate_layer_parent_cycles` ✓）挡新原子 ✓；
    /// 这一条守的是**重放期** ✓：别的客户端／旧数据直接写进日志的环不许产生状态效果 ✓。
    /// **变异**：删掉 `precondition` 里 `parent_id` 那一段 ⇒ `g1.parent_id` 变成 `Some("g2")` ⇒
    /// 第一条断言红 ✓（实测过 ✓）。
    #[test]
    fn a_parent_id_rewrite_that_closes_a_cycle_is_skipped_by_the_fold() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_g1",
                json!({"layer_id": "g1", "type": "layer_group"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_g2",
                json!({"layer_id": "g2", "parent_id": "g1"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_cycle",
                json!({"layer_id": "g1", "key": "parent_id", "value": "g2"}),
            ),
        );
        let folded = fold(&atoms);
        assert_eq!(
            folded.state.layers["g1"].parent_id, None,
            "成环的改写不许生效"
        );
        assert!(
            folded.state.is_consistent(),
            "{:?}",
            folded.state.violations()
        );
        assert!(
            folded
                .warnings
                .iter()
                .any(|w| w.atom_id == "a_cycle" && w.detail.contains("成环")),
            "{:?}",
            folded.warnings
        );
    }

    /// **旧数据里的环不许把删除拖死** ✓（2026-10-28，审计 Batch 2）。
    ///
    /// `tombstone_layer` 按父链收集后代 ✓；没有 `seen` 时环会让 `targets` **无限增长** ✗。
    /// **变异**：把 `seen.insert(...)` 去掉 ⇒ 本测试**不返回**（实测：10 秒后仍在跑，被 `timeout` 杀掉 ✓）。
    #[test]
    fn tombstoning_over_a_preexisting_parent_cycle_terminates() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_g1",
                json!({"layer_id": "g1", "type": "layer_group"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_g2",
                json!({"layer_id": "g2", "parent_id": "g1"}),
            ),
        );
        let folded = fold(&atoms);
        let mut state = folded.state;
        // 直接改内存状态模拟旧数据（写入闸不会让这条进日志 ✓，所以只能从这里造 ✓）。
        state.layers.get_mut("g1").unwrap().parent_id = Some("g2".to_owned());
        let result = fold_atoms(
            state,
            &[atom(
                AtomKind::Tombstone,
                "a_del_g1",
                json!({"layer_id": "g1"}),
            )],
        );
        assert!(result.state.layers["g1"].is_deleted());
        assert!(
            result.state.layers["g2"].is_deleted(),
            "环上的另一个节点也必须被级联删除"
        );
    }

    /// **成环检查自己的四条出口** ✓（2026-10-28，审计 Batch 2）：
    /// 走到自己 ✓、父链本身已环 ✓、深度爆掉 ✓、正常 64 层放行 ✓。
    /// **变异**：删掉 `visited` 判重 ⇒ 「父链本身已环」那条会变成走上限／死循环 ✗；
    /// 删掉深度上限 ⇒ 65 层那条不报错 ⇒ 红 ✓。
    #[test]
    fn check_layer_parent_cycle_rejects_cycles_and_depth_bombs() {
        let mut atoms = chain();
        for i in 0..65 {
            let payload = if i == 0 {
                json!({"layer_id": "l0", "type": "layer_group"})
            } else {
                json!({"layer_id": format!("l{i}"), "type": "layer_group", "parent_id": format!("l{}", i - 1)})
            };
            push(
                &mut atoms,
                atom(AtomKind::CreateLayer, &format!("a_l{i}"), payload),
            );
        }
        let state = fold(&atoms).state;

        // ① 沿着父链走回自己：把 l0 挂到 l64 下面 ⇒ l64 → … → l0 回到自己。
        let error = check_layer_parent_cycle(&state, "l0", "l64").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{error:?}");
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(detail.contains("回到自己"), "{detail}");

        // ② 父链本身已环（旧数据）：l0 ⇐ l2、l2 ⇐ l0，从第三方挂上去也要报。
        let mut cyclic = state.clone();
        cyclic.layers.get_mut("l0").unwrap().parent_id = Some("l2".to_owned());
        cyclic.layers.get_mut("l2").unwrap().parent_id = Some("l0".to_owned());
        let error = check_layer_parent_cycle(&cyclic, "new_layer", "l1").unwrap_err();
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(detail.contains("已经成环"), "{detail}");

        // ③ 深度：65 层父链 ⇒ 拒绝（上限 64）。
        let error = check_layer_parent_cycle(&state, "new_layer", "l64").unwrap_err();
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(detail.contains("过深"), "{detail}");

        // ④ 64 层父链（l0..l63）⇒ 放行。
        assert!(check_layer_parent_cycle(&state, "new_layer", "l63").is_ok());
    }

    #[test]
    fn reorder_layers_is_absolute_and_last_writer_wins() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_layer2",
                json!({"layer_id": "layer_2"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::ReorderLayers,
                "a_order1",
                json!({"order": ["layer_2", "layer_1"]}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::ReorderLayers,
                "a_order2",
                json!({"order": ["layer_1", "layer_2"]}),
            ),
        );
        let result = fold(&atoms);
        assert_eq!(result.state.layers["layer_1"].z_index, 0);
        assert_eq!(result.state.layers["layer_2"].z_index, 1);
        let order: Vec<&str> = result
            .state
            .alive_layers()
            .iter()
            .map(|l| l.id.as_str())
            .collect();
        assert_eq!(order, vec!["layer_default", "layer_1", "layer_2"]);

        // 不完整的 z 序快照被拒绝（级联失效）。
        push(
            &mut atoms,
            atom(
                AtomKind::ReorderLayers,
                "a_order_bad",
                json!({"order": ["layer_1"]}),
            ),
        );
        let result = fold(&atoms);
        assert_eq!(result.warning_count(WarningKind::CascadeInvalidation), 3);
    }

    /// Y-08 期 4：重排测试的底链——`layer_1` 内 3 对象（obj_1/obj_b/obj_c，
    /// 创建缺省 z 0/1/2），另 `layer_2` 内 1 对象（obj_x，供跨层用例）。
    fn chain_objects() -> Vec<Atom> {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj_b",
                json!({"object_id": "obj_b", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj_c",
                json!({"object_id": "obj_c", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_layer2",
                json!({"layer_id": "layer_2"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj_x",
                json!({"object_id": "obj_x", "layer_id": "layer_2", "type": "stroke"}),
            ),
        );
        atoms
    }

    /// 某图层内存活对象的 id 序列（自底向上）。
    fn layer_order(state: &DocumentState, layer_id: &str) -> Vec<String> {
        state
            .objects_in_layer(layer_id)
            .iter()
            .map(|object| object.id.clone())
            .collect()
    }

    fn reorder_objects(id: &str, layer_id: &str, order: Value) -> Atom {
        atom(
            AtomKind::ReorderObjects,
            id,
            json!({"layer_id": layer_id, "order": order}),
        )
    }

    #[test]
    fn reorder_objects_changes_render_order() {
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            reorder_objects("a_reorder", "layer_1", json!(["obj_c", "obj_1", "obj_b"])),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!(
            layer_order(&result.state, "layer_1"),
            vec!["obj_c", "obj_1", "obj_b"],
            "重排后对象顺序必须等于提交的 order"
        );
        let zs: Vec<i64> = result
            .state
            .objects_in_layer("layer_1")
            .iter()
            .map(|object| object.z_index)
            .collect();
        assert_eq!(zs, vec![0, 1, 2], "重排后 z_index 必须稠密 0..n-1");
        assert!(result.state.is_consistent());
    }

    #[test]
    fn reorder_objects_rejects_partial_order() {
        let atoms = chain_objects();
        let state = fold(&atoms).state;
        let bad = reorder_objects("a_bad", "layer_1", json!(["obj_1", "obj_b"]));
        let error = precondition(&state, &bad).expect_err("缺一个存活对象必须被拒");
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("obj_c"),
            "报错必须列出缺失的 id，实得：{detail}"
        );
        // 折叠层：坏原子只产生一条级联失效警告，顺序保持原样。
        let mut atoms = atoms;
        push(&mut atoms, bad);
        let result = fold(&atoms);
        assert_eq!(
            result.warning_count(WarningKind::CascadeInvalidation),
            1,
            "坏原子应恰好产生一条级联失效警告，实得：{:?}",
            result.warnings
        );
        assert_eq!(
            layer_order(&result.state, "layer_1"),
            vec!["obj_1", "obj_b", "obj_c"],
            "坏重排不得改动顺序"
        );
    }

    #[test]
    fn reorder_objects_rejects_unknown_and_tombstoned() {
        let atoms = chain_objects();
        let state = fold(&atoms).state;
        // 未知 id。
        let unknown = reorder_objects(
            "a_unknown",
            "layer_1",
            json!(["obj_1", "obj_b", "no_such_obj"]),
        );
        let error = precondition(&state, &unknown).expect_err("未知 id 必须被拒");
        assert!(
            error
                .context
                .detail
                .clone()
                .unwrap_or_default()
                .contains("no_such_obj"),
            "未知 id 的报错须点名该 id，实得：{error}"
        );
        // 已删对象。
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_del_b",
                json!({"object_id": "obj_b"}),
            ),
        );
        let state = fold(&atoms).state;
        let tombstoned = reorder_objects("a_tomb", "layer_1", json!(["obj_1", "obj_b", "obj_c"]));
        let error = precondition(&state, &tombstoned).expect_err("已删对象必须被拒");
        assert!(
            error
                .context
                .detail
                .clone()
                .unwrap_or_default()
                .contains("已被 tombstone"),
            "已删对象的报错须点名墓碑状态，实得：{error}"
        );
        // 跨层 id：必须点名它所属的图层。
        let state = fold(&chain_objects()).state;
        let cross = reorder_objects("a_cross", "layer_1", json!(["obj_1", "obj_b", "obj_x"]));
        let error = precondition(&state, &cross).expect_err("跨层 id 必须被拒");
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("obj_x") && detail.contains("layer_2"),
            "跨层 id 的报错须点名所属图层，实得：{detail}"
        );
    }

    #[test]
    fn reorder_objects_rejects_duplicate() {
        let state = fold(&chain_objects()).state;
        let bad = reorder_objects("a_dup", "layer_1", json!(["obj_1", "obj_1", "obj_c"]));
        let error = precondition(&state, &bad).expect_err("重复 id 必须被拒");
        assert!(
            error
                .context
                .detail
                .clone()
                .unwrap_or_default()
                .contains("重复"),
            "重复 id 的报错须点名重复，实得：{error}"
        );
    }

    #[test]
    fn reorder_objects_revert_restores_order() {
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            reorder_objects("a_reorder", "layer_1", json!(["obj_c", "obj_1", "obj_b"])),
        );
        let reordered = fold(&atoms);
        assert_eq!(
            layer_order(&reordered.state, "layer_1"),
            vec!["obj_c", "obj_1", "obj_b"]
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_revert", json!({"target": "a_reorder"})),
        );
        let reverted = fold(&atoms);
        assert!(reverted.is_suppressed("a_reorder"));
        assert_eq!(
            layer_order(&reverted.state, "layer_1"),
            vec!["obj_1", "obj_b", "obj_c"],
            "revert 重排后顺序须回到排前"
        );
        assert!(reverted.state.is_consistent());
    }

    #[test]
    fn reorder_objects_overwrites_single_z_edits() {
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_single",
                json!({"object_id": "obj_1", "key": "z_index", "value": 99}),
            ),
        );
        let edited = fold(&atoms);
        assert_eq!(
            layer_order(&edited.state, "layer_1"),
            vec!["obj_b", "obj_c", "obj_1"],
            "前置：单改 z_index=99 应把 obj_1 顶到栈顶"
        );
        push(
            &mut atoms,
            reorder_objects("a_reorder", "layer_1", json!(["obj_c", "obj_1", "obj_b"])),
        );
        let result = fold(&atoms);
        assert_eq!(
            layer_order(&result.state, "layer_1"),
            vec!["obj_c", "obj_1", "obj_b"],
            "重排须覆盖先前的单改 z_index"
        );
        assert_eq!(
            result.state.objects["obj_1"].z_index, 1,
            "单改值须被重排覆盖"
        );
    }

    #[test]
    fn reorder_objects_revert_keeps_later_single_edits() {
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            reorder_objects("a_reorder", "layer_1", json!(["obj_c", "obj_1", "obj_b"])),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_single",
                json!({"object_id": "obj_c", "key": "z_index", "value": 99}),
            ),
        );
        let edited = fold(&atoms);
        assert_eq!(
            layer_order(&edited.state, "layer_1"),
            vec!["obj_1", "obj_b", "obj_c"],
            "前置：排后单改应把 obj_c 顶到栈顶"
        );
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_revert", json!({"target": "a_reorder"})),
        );
        let result = fold(&atoms);
        assert!(result.is_suppressed("a_reorder"));
        // **钉死**：revert 重排后，排后的单改仍生效——obj_c 保持 z=99 在顶，
        // 其余两对象回到排前的 z 值。注意 fold 层 `CreateObject` 缺省 z 是 0
        //（max+1 是工具层 `write_add_effect` 的行为），故排前 z 全为 0。
        // 这不是 bug，是日志折叠的既有语义。
        assert_eq!(
            result.state.objects["obj_c"].z_index, 99,
            "revert 重排不得抹掉排后的单改 z_index"
        );
        assert_eq!(result.state.objects["obj_1"].z_index, 0);
        assert_eq!(result.state.objects["obj_b"].z_index, 0);
        assert_eq!(
            layer_order(&result.state, "layer_1"),
            vec!["obj_1", "obj_b", "obj_c"]
        );
    }

    #[test]
    fn reorder_objects_new_object_without_z_index_defaults_to_zero_at_fold_level() {
        // 设计 R8 的"缺省 max+1 落栈顶"是**工具层**行为（`write_add_effect`）；
        // fold 层 `CreateObject` 不给 z_index 时缺省就是 0——此处钉死该分工，
        // 工具层的 max+1 由服务端测试 `reorder_objects_new_object_lands_on_top` 验收。
        let mut atoms = chain_objects();
        push(
            &mut atoms,
            reorder_objects("a_reorder", "layer_1", json!(["obj_c", "obj_1", "obj_b"])),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_obj_d",
                json!({"object_id": "obj_d", "layer_id": "layer_1", "type": "stroke"}),
            ),
        );
        let result = fold(&atoms);
        assert_eq!(
            result.state.objects["obj_d"].z_index, 0,
            "fold 层新建对象无 z_index 时缺省为 0（不是 max+1）"
        );
        assert_eq!(
            layer_order(&result.state, "layer_1"),
            vec!["obj_c", "obj_d", "obj_1", "obj_b"],
            "z=0 的新对象按 (z_index, seq) 排在同 z 的 obj_c 之后、obj_1 之前"
        );
    }

    #[test]
    fn reorder_objects_empty_layer_empty_order_is_accepted_by_fold() {
        // fold 认集合等式（空虚真），工具层才挡无意义调用——分工见设计 §3.3/§3.4。
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateLayer,
                "a_empty",
                json!({"layer_id": "layer_empty"}),
            ),
        );
        let state = fold(&atoms).state;
        let empty = reorder_objects("a_empty_reorder", "layer_empty", json!([]));
        precondition(&state, &empty).expect("空图层 + 空 order 在 fold 层应接受");
        push(&mut atoms, empty);
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
    }

    #[test]
    fn set_property_updates_layer_and_object() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_layer_prop",
                json!({"layer_id": "layer_1", "key": "opacity", "value": 0.5}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_obj_prop",
                json!({"object_id": "obj_1", "key": "visible", "value": false}),
            ),
        );
        let result = fold(&atoms);
        assert_eq!(result.state.layers["layer_1"].opacity, 0.5);
        assert!(!result.state.objects["obj_1"].visible);
        // 属性修改产生新版本。
        assert_eq!(result.state.objects["obj_1"].versions.len(), 2);
        assert!(result.state.is_consistent());
    }

    #[test]
    fn declare_head_in_slice_is_flagged_for_formula_handling() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::DeclareHead,
                "a_head",
                json!({"base": {"type": "atom", "id": "a_layer"}}),
            ),
        );
        let result = fold(&atoms);
        assert_eq!(
            result.warning_count(WarningKind::DeclareHeadOutsideFormula),
            1
        );
    }

    #[test]
    fn reverting_mask_creation_cascades_to_dependent_layer_property() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateMask,
                "a_mask",
                json!({"mask_id": "mask_1", "shape": {"kind": "rect"}}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetProperty,
                "a_bind_mask",
                json!({"layer_id": "layer_1", "key": "mask_id", "value": "mask_1"}),
            ),
        );
        // 绑定有效：图层指向存在的蒙版。
        let bound = fold(&atoms);
        assert_eq!(
            bound.state.layers["layer_1"].mask_id.as_deref(),
            Some("mask_1")
        );
        assert!(bound.state.is_consistent());

        // 撤销蒙版创建：绑定原子级联失效，图层不再指向失效蒙版。
        push(
            &mut atoms,
            atom(AtomKind::Revert, "a_del_mask", json!({"target": "a_mask"})),
        );
        let result = fold(&atoms);
        assert!(result.is_suppressed("a_mask"));
        assert_eq!(result.warning_count(WarningKind::CascadeInvalidation), 1);
        assert!(result.state.layers["layer_1"].mask_id.is_none());
        assert!(
            result.state.is_consistent(),
            "{:?}",
            result.state.violations()
        );
    }

    #[test]
    fn object_with_style_reference_keeps_no_orphans() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateStyle,
                "a_style",
                json!({"style_id": "style_1", "name": "ink"}),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::DrawStroke,
                "a_stroke",
                json!({
                    "object_id": "obj_styled",
                    "layer_id": "layer_1",
                    "style_id": "style_1",
                    "data": {"points": [[0, 0], [1, 1]]},
                }),
            ),
        );
        let styled = fold(&atoms);
        assert_eq!(
            styled.state.objects["obj_styled"].style.as_deref(),
            Some("style_1")
        );
        assert!(styled.state.is_consistent());

        push(
            &mut atoms,
            atom(
                AtomKind::Revert,
                "a_del_style",
                json!({"target": "a_style"}),
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_suppressed("a_style"));
        assert_eq!(
            result.warning_count(WarningKind::CascadeInvalidation),
            1,
            "依赖失效风格的笔触级联失效（跳过而非产生孤儿引用）"
        );
        assert!(!result.state.objects.contains_key("obj_styled"));
        assert!(
            result.state.is_consistent(),
            "{:?}",
            result.state.violations()
        );
    }

    #[test]
    fn base_state_is_preserved_and_warnings_are_empty_for_empty_slice() {
        let base = fold(&chain()).state;
        let result = fold_atoms(base.clone(), &[]);
        assert_eq!(result.state, base);
        assert!(result.warnings.is_empty());
        assert!(result.applied.is_empty());
    }

    // ---- 几何（路线三）----

    /// 几何原子载荷：`affine: {matrix, pivot}` + 新尺寸。
    fn geometry_atom(
        kind: AtomKind,
        id: &str,
        matrix: [f64; 6],
        pivot: [f64; 2],
        w: u32,
        h: u32,
    ) -> Atom {
        atom(
            kind,
            id,
            json!({
                "affine": { "matrix": matrix, "pivot": pivot },
                "new_width": w,
                "new_height": h,
            }),
        )
    }

    /// 撤销往返的比较口径：只比内容字段（head_seq/head_atom 本来就会变）。
    fn content_of(state: &DocumentState) -> Value {
        let objects: Vec<Value> = state
            .objects
            .values()
            .map(|object| {
                json!({
                    "id": object.id,
                    "deleted": object.is_deleted(),
                    "matrix": object.transform.matrix,
                    "pivot": object.transform.pivot,
                })
            })
            .collect();
        let selections: Vec<Value> = state
            .selections
            .values()
            .map(|selection| {
                json!({
                    "id": selection.id,
                    "shape": selection.shape,
                    "patches": selection.patches.iter().map(|patch| patch.bbox).collect::<Vec<_>>(),
                })
            })
            .collect();
        json!({
            "width": state.width,
            "height": state.height,
            "anchors": state.anchors,
            "objects": objects,
            "selections": selections,
        })
    }

    #[test]
    fn geometry_scale_composes_object_transform_about_anchor() {
        let mut atoms = chain();
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!(result.state.width, 200);
        assert_eq!(result.state.height, 100);
        let object = &result.state.objects["obj_1"];
        // 平移折叠进矩阵：x' = 2(x-50)+50 = 2x-50；y' = 2(y-25)+25 = 2y-25。
        assert_eq!(object.transform.matrix, [2.0, 0.0, 0.0, 2.0, -50.0, -25.0]);
        assert_eq!(object.transform.pivot, [0.0, 0.0]);
    }

    #[test]
    fn geometry_crop_translates_content_and_selection_and_patches() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "rect", "x": 20, "y": 30, "w": 10, "h": 10 },
                    "patches": [{
                        "bbox": [15, 25, 5, 5],
                        "target_u8": 255,
                        "op": "replace_all",
                        "blob": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                    }],
                }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::CropCanvas,
                "a_crop",
                [1.0, 0.0, 0.0, 1.0, -10.0, -20.0],
                [0.0, 0.0],
                80,
                30,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!((result.state.width, result.state.height), (80, 30));
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, -10.0, -20.0]
        );
        // 形状坐标是 f64：变换后写出 10.0（不是整数 10）。
        assert_eq!(
            result.state.selections["sel_1"].shape,
            json!({ "kind": "rect", "x": 10.0, "y": 10.0, "w": 10.0, "h": 10.0 })
        );
        assert_eq!(
            result.state.selections["sel_1"].patches[0].bbox,
            [5, 5, 5, 5]
        );
    }

    #[test]
    fn geometry_rotate_90_swaps_canvas_and_composes_rotation() {
        let mut atoms = chain();
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::RotateCanvas,
                "a_rot",
                [0.0, 1.0, -1.0, 0.0, 0.0, 0.0],
                [50.0, 25.0],
                50,
                100,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!((result.state.width, result.state.height), (50, 100));
        // 顺时针 90° 绕中心：x' = -(y-25)+50 = -y+75；y' = (x-50)+25 = x-25。
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [0.0, 1.0, -1.0, 0.0, 75.0, -25.0]
        );
    }

    #[test]
    fn geometry_flip_mirrors_about_anchor() {
        let mut atoms = chain();
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::FlipCanvas,
                "a_flip",
                [-1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [50.0, 25.0],
                100,
                50,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!((result.state.width, result.state.height), (100, 50));
        // 左右翻绕中心：x' = -(x-50)+50 = -x+100；y' = y。
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [-1.0, 0.0, 0.0, 1.0, 100.0, 0.0]
        );
    }

    #[test]
    fn geometry_resize_bounds_changes_size_only() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "rect", "x": 20, "y": 30, "w": 10, "h": 10 },
                }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ResizeCanvasBounds,
                "a_resize",
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0],
                300,
                200,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!((result.state.width, result.state.height), (300, 200));
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
        // 改界不碰形状：输入的整数表示原样保留。
        assert_eq!(
            result.state.selections["sel_1"].shape,
            json!({ "kind": "rect", "x": 20, "y": 30, "w": 10, "h": 10 })
        );
    }

    #[test]
    fn geometry_anchors_move_with_content_and_pivot_anchor_stays() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::SetAnchor,
                "a_anchor",
                json!({ "name": "a1", "x": 20, "y": 30 }),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::SetAnchor,
                "a_anchor_pivot",
                json!({ "name": "pivot", "x": 50, "y": 25 }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        // 锚点 A：枢轴坐标不动，其余锚点跟随内容走。
        assert_eq!(result.state.anchors["pivot"].x, 50.0);
        assert_eq!(result.state.anchors["pivot"].y, 25.0);
        assert_eq!(result.state.anchors["a1"].x, -10.0);
        assert_eq!(result.state.anchors["a1"].y, 35.0);
    }

    #[test]
    fn geometry_skips_deleted_instance_and_group_objects() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_inst",
                json!({
                    "object_id": "inst_1",
                    "layer_id": "layer_1",
                    "type": "instance",
                    "data": { "master_ref": { "object_id": "obj_1" } },
                }),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateObject,
                "a_group",
                json!({ "object_id": "grp_1", "layer_id": "layer_1", "type": "group" }),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::Tombstone,
                "a_del",
                json!({ "object_id": "obj_1" }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        // 实例内容经由 master 移动（master 已删则不渲染），实例自身不复合；
        // 组自身不在渲染路径上；已删对象不碰。
        assert_eq!(
            result.state.objects["inst_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(
            result.state.objects["grp_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert!(result.state.objects["obj_1"].is_deleted());
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn geometry_missing_mask_patches_is_an_observable_error_not_silent() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "rect", "x": 0, "y": 0, "w": 10, "h": 10 },
                    "patches": [{
                        "bbox": [0, 0, 10, 10],
                        "target_u8": 255,
                        "op": "replace_all",
                        "blob": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                    }],
                }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let result = fold(&atoms);
        // 非平移仿射 + 带 patch 的蒙版 + 载荷里没有 mask_patches ⇒ 原子被跳过并留警告。
        assert!(
            !result.warnings.is_empty(),
            "必须留可观测信号，不能静默接受"
        );
        assert_eq!((result.state.width, result.state.height), (100, 50));
        assert_eq!(
            result.state.objects["obj_1"].transform.matrix,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn geometry_mask_patches_payload_replaces_patches() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "rect", "x": 0, "y": 0, "w": 10, "h": 10 },
                    "patches": [{
                        "bbox": [0, 0, 10, 10],
                        "target_u8": 255,
                        "op": "replace_all",
                        "blob": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
                    }],
                }),
            ),
        );
        let mut scale = geometry_atom(
            AtomKind::ScaleCanvas,
            "a_scale",
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            [50.0, 25.0],
            200,
            100,
        );
        scale.payload["mask_patches"] = json!({
            "sel_1": [{
                "bbox": [0, 0, 20, 20],
                "target_u8": 255,
                "op": "replace_all",
                "blob": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            }],
        });
        push(&mut atoms, scale);
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!(result.state.selections["sel_1"].patches.len(), 1);
        assert_eq!(
            result.state.selections["sel_1"].patches[0].bbox,
            [0, 0, 20, 20]
        );
    }

    #[test]
    fn geometry_unknown_selection_shape_is_left_unchanged() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "weird", "data": [1, 2, 3] },
                }),
            ),
        );
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let result = fold(&atoms);
        assert!(result.is_clean(), "{:?}", result.warnings);
        assert_eq!(
            result.state.selections["sel_1"].shape,
            json!({ "kind": "weird", "data": [1, 2, 3] })
        );
    }

    #[test]
    fn geometry_undo_by_exclusion_restores_content() {
        let mut atoms = chain();
        push(
            &mut atoms,
            atom(
                AtomKind::SetAnchor,
                "a_anchor",
                json!({ "name": "a1", "x": 20, "y": 30 }),
            ),
        );
        push(
            &mut atoms,
            atom(
                AtomKind::CreateSelection,
                "a_sel",
                json!({
                    "selection_id": "sel_1",
                    "shape": { "kind": "rect", "x": 20, "y": 30, "w": 10, "h": 10 },
                }),
            ),
        );
        let base = fold(&atoms).state;
        push(
            &mut atoms,
            geometry_atom(
                AtomKind::ScaleCanvas,
                "a_scale",
                [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
                [50.0, 25.0],
                200,
                100,
            ),
        );
        let moved = fold(&atoms).state;
        assert_eq!((moved.width, moved.height), (200, 100));
        // 撤销 = 折叠时排除几何原子：内容字段与几何前完全一致。
        atoms.pop();
        let restored = fold(&atoms).state;
        assert_eq!(content_of(&restored), content_of(&base));
    }

    #[test]
    fn geometry_old_log_serializes_without_new_keys() {
        let state = fold(&chain()).state;
        let value = serde_json::to_value(&state).expect("状态可序列化");
        // 空锚点表不写键（serde skip-if-empty）：旧文档逐字节不变的序列化侧证据。
        assert!(
            value.get("anchors").is_none(),
            "空 anchors 表不应写出键：{}",
            value
        );
        let object = &value["objects"]["obj_1"];
        assert_eq!(
            object["transform"]["matrix"],
            json!([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
        );
    }
}
