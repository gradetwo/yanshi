//! 确定性场景生成器：属性测试、fuzz 与基准共用。
//!
//! 生成器按“客户端提交”的真实路径驱动引擎：先按设计文档 6.3 把 blob 写入 CAS，
//! 再用 [`crate::log::AtomLog::append_validated`] 提交原子（走 12.2 提交时校验），
//! 被拒绝的原子**不进日志**并记录错误码。
//!
//! 折叠侧采用设计文档 6.2/6.5 的策略：以快照为基线、只在快照窗口内重放，
//! 因此 10 万原子级别的场景也不会退化成 O(n²)。
//!
//! 生成器使用固定种子的 [`crate::ids::UlidGen`]，同一 seed 完全可复现。

use crate::atom::{Atom, AtomKind, BlobHash, BlobRef};
use crate::blob::{BlobStore, MemoryBlobStore};
use crate::error::ErrorCode;
use crate::fold::{apply, fold_atoms, precondition};
use crate::ids::{Seq, UlidGen};
use crate::log::{AtomLog, CommitContext};
use crate::snapshot::{Snapshot, SnapshotBase};
use crate::state::DocumentState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;

/// 场景配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioConfig {
    /// 随机种子。
    pub seed: u64,
    /// 生成步数（即尝试提交的原子数）。
    pub steps: usize,
    /// 带 blob 的对象写入的字节数（0 表示不产生 blob）。
    pub blob_bytes: usize,
    /// 快照窗口：每积累这么多原子就推进一次基线快照（0 表示不推进，全程重放）。
    pub snapshot_window: usize,
    /// 撤销/恢复操作的千分比。
    pub revert_permille: u32,
    /// 采样性替换（retouch）操作的千分比。
    pub sampling_permille: u32,
    /// 存活实体数量上限（图层 / 对象 / 蒙版 / 风格各自）。
    pub max_entities: usize,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            seed: 2026,
            steps: 200,
            blob_bytes: 0,
            snapshot_window: 0,
            revert_permille: 120,
            sampling_permille: 40,
            max_entities: 12,
        }
    }
}

impl ScenarioConfig {
    /// 小规模场景（属性测试默认）。
    pub fn small(seed: u64) -> Self {
        Self {
            seed,
            ..Self::default()
        }
    }

    /// 大规模 fuzz 场景（Phase 0 出口条件：10 万原子）。
    ///
    /// 撤销比例刻意低于小规模属性测试：10 万原子下每次撤销都要在快照窗口内重放，
    /// 小规模测试负责高密度覆盖撤销语义，这里负责长历史与规模。
    pub fn fuzz(seed: u64, steps: usize) -> Self {
        Self {
            seed,
            steps,
            blob_bytes: 2048,
            snapshot_window: 500,
            revert_permille: 30,
            sampling_permille: 20,
            max_entities: 32,
        }
    }
}

/// 单步记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepRecord {
    /// 客户端生成的原子 id。
    pub atom_id: String,
    /// 原子类型。
    pub kind: AtomKind,
    /// 操作者。
    pub actor: String,
    /// 会话。
    pub session: String,
    /// 提交后的权威 seq（被拒绝时为 None）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq: Option<Seq>,
    /// 是否被接受进日志。
    pub accepted: bool,
    /// 被拒绝时的错误码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<ErrorCode>,
}

/// 生成统计。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ScenarioStats {
    /// 接受的原子数。
    pub accepted: usize,
    /// 拒绝的原子数。
    pub rejected: usize,
    /// 各错误码拒绝次数。
    pub rejections_by_code: BTreeMap<String, usize>,
    /// 生成快照数。
    pub snapshots: usize,
}

/// 生成结果。
pub struct Scenario {
    /// 原子日志。
    pub log: AtomLog,
    /// blob CAS。
    pub store: MemoryBlobStore,
    /// 每步记录。
    pub steps: Vec<StepRecord>,
    /// 统计。
    pub stats: ScenarioStats,
    /// 增量维护的最终状态（应与 `state_at(head)` 收敛）。
    pub final_state: DocumentState,
    /// 生成的快照（按写入顺序）。
    pub snapshots: Vec<Snapshot>,
}

impl Scenario {
    /// 全部 blob hash。
    pub fn blob_hashes(&self) -> Vec<BlobHash> {
        self.store
            .list()
            .map(|entries| entries.into_iter().map(|entry| entry.blob_hash).collect())
            .unwrap_or_default()
    }

    /// 最终状态的活跃 blob Manifest。
    pub fn active_manifest(&self) -> std::collections::BTreeSet<BlobHash> {
        self.final_state.active_blob_manifest()
    }

    /// 接受率（0.0 – 1.0）。
    pub fn acceptance_rate(&self) -> f64 {
        let total = self.stats.accepted + self.stats.rejected;
        if total == 0 {
            1.0
        } else {
            self.stats.accepted as f64 / total as f64
        }
    }
}

/// 极简确定性 RNG（splitmix64）。
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// 以种子构造。
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    /// 下一个 u64。
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `0..bound` 的随机数（bound 为 0 时返回 0）。
    pub fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next_u64() % bound as u64) as usize
        }
    }

    /// 千分比概率。
    pub fn permille(&mut self, permille: u32) -> bool {
        (self.next_u64() % 1000) < u64::from(permille)
    }

    /// 从列表随机取一个元素的克隆。
    pub fn pick<T: Clone>(&mut self, items: &[T]) -> Option<T> {
        if items.is_empty() {
            None
        } else {
            let index = self.below(items.len());
            items.get(index).cloned()
        }
    }

    /// 洗牌（Fisher-Yates）。
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        if items.len() < 2 {
            return;
        }
        for index in (1..items.len()).rev() {
            let swap = self.below(index + 1);
            items.swap(index, swap);
        }
    }
}

/// 会话模型：两个会话交替提交，用于触发跨会话冲突与权限路径。
const SESSIONS: [(&str, &str); 2] = [("human:1", "session:a"), ("ai:1", "session:b")];

/// 生成器维护的实体视图：只保留**当前存活**的实体，避免生成注定被拒绝的原子。
#[derive(Debug, Default)]
struct Model {
    layers: Vec<String>,
    objects: Vec<(String, String)>,
    styles: Vec<String>,
    masks: Vec<String>,
    selections: Vec<String>,
    /// **未锁定**的图层的 id ✓ —— 定向操作只挑这些 ✓（见 `AliveView` 的说明 ✓）。
    unlocked_layers: Vec<String>,
    /// **未锁定**的对象（含归属图层 ✓）。
    unlocked_objects: Vec<(String, String)>,
    /// 可作为 revert/reapply 目标的状态原子（seq 升序，窗口内）。
    revertable: Vec<(String, Seq)>,
    /// 当前处于撤销状态的原子。
    suppressed: Vec<String>,
}

/// 存活实体视图：图层、对象（含归属图层）、风格、蒙版、选区 ✓。
///
/// **另外维护一份"未锁定"视图** ✓ —— 这是长任务 fuzz 里一条失败的修法 ✓：
/// fuzz 会随机 `set_property {"locked": …}` ✓（约一半概率锁上 ✓），
/// 而**图层锁定强制是后来才加上的** ✓ ⇒ 此后针对锁定实体的原子会被**合法拒绝** ✓
/// ⇒ 接受率掉到 **0.685** ✗（拒绝分布里 `permission_denied` 占 30978 条 ✓），
/// 而阈值 0.9 是**加锁之前**定的 ✗。
///
/// **修法不是放宽阈值** ✗（那会削弱这条不变量 ✓），而是让生成器**知道锁** ✓：
/// 定向操作只挑**未锁定**的目标 ✓，锁/解锁那个操作本身仍可作用于**任意**存活实体 ✓
/// —— 这正合本文件的原意 ✓："只保留当前存活的实体，避免生成注定被拒绝的原子" ✓。
#[derive(Debug, Default, Clone)]
struct AliveView {
    layers: Vec<String>,
    objects: Vec<(String, String)>,
    styles: Vec<String>,
    masks: Vec<String>,
    selections: Vec<String>,
    /// 未锁定的图层 ✓。
    unlocked_layers: Vec<String>,
    /// 未锁定的对象 ✓（**其所属图层也未锁定** ✓ —— 锁定校验两层都看 ✓）。
    unlocked_objects: Vec<(String, String)>,
}

fn alive_view(state: &DocumentState) -> AliveView {
    AliveView {
        layers: state
            .alive_layers()
            .iter()
            .map(|layer| layer.id.clone())
            .collect(),
        objects: state
            .alive_objects()
            .iter()
            .map(|object| (object.id.clone(), object.layer_id.clone()))
            .collect(),
        styles: state
            .styles
            .values()
            .filter(|style| !style.is_deleted())
            .map(|style| style.id.clone())
            .collect(),
        masks: state
            .masks
            .values()
            .filter(|mask| !mask.is_deleted())
            .map(|mask| mask.id.clone())
            .collect(),
        unlocked_layers: state
            .alive_layers()
            .iter()
            .filter(|layer| !layer.locked)
            .map(|layer| layer.id.clone())
            .collect(),
        unlocked_objects: state
            .alive_objects()
            .iter()
            .filter(|object| {
                // **两层都要放行** ✓：对象自己没锁 ✓、且它所在的图层也没锁 ✓
                //（锁定校验正是这么判的 ✓ ⇒ 生成器必须与它一致 ✓，否则又会生成注定被拒的原子 ✗）。
                !object.locked
                    && state
                        .layers
                        .get(&object.layer_id)
                        .map(|layer| !layer.locked && !layer.is_deleted())
                        .unwrap_or(false)
            })
            .map(|object| (object.id.clone(), object.layer_id.clone()))
            .collect(),
        selections: state
            .selections
            .values()
            .filter(|selection| !selection.is_deleted())
            .map(|selection| selection.id.clone())
            .collect(),
    }
}

/// 生成一个场景。
pub fn generate(config: &ScenarioConfig) -> Scenario {
    let mut rng = Rng::new(config.seed);
    let mut ids = UlidGen::seeded(config.seed);
    let mut log = AtomLog::new();
    let mut store = MemoryBlobStore::new();
    let mut model = Model::default();
    let mut steps = Vec::with_capacity(config.steps);
    let mut stats = ScenarioStats::default();
    let mut snapshots: Vec<Snapshot> = Vec::new();

    // 折叠基线：快照状态 + 基线之后的原子重放（6.2 / 6.5）。
    let mut base_state = DocumentState::empty();
    let mut base_seq: Seq = 0;
    let mut state = DocumentState::empty();
    // 跨步复用 `state@seq` 缓存：日志只追加，已求值的 seq 结果永远有效。
    let mut state_cache = crate::seq::StateAtCache::new();

    for step in 0..config.steps {
        let session_index = if rng.permille(700) { 0 } else { 1 };
        let (actor, session) = SESSIONS[session_index];
        let atom = build_atom(
            &mut rng, &mut ids, &mut store, &model, &state, config, actor, session, step,
        );
        let atom_id = atom.id.clone();
        let kind = atom.kind;
        let outcome = {
            let exists = |hash: &BlobHash| store.exists(hash);
            let commit = CommitContext::new(&state, &exists, actor, session);
            log.append_validated(atom, &commit)
        };

        let appended = match outcome {
            Ok(appended) => appended,
            Err(error) => {
                stats.rejected += 1;
                *stats
                    .rejections_by_code
                    .entry(error.code.as_str().to_owned())
                    .or_insert(0) += 1;
                steps.push(StepRecord {
                    atom_id,
                    kind,
                    actor: actor.to_owned(),
                    session: session.to_owned(),
                    seq: None,
                    accepted: false,
                    error_code: Some(error.code),
                });
                continue;
            }
        };

        let seq = appended.seq();
        stats.accepted += 1;
        steps.push(StepRecord {
            atom_id: atom_id.clone(),
            kind,
            actor: actor.to_owned(),
            session: session.to_owned(),
            seq: Some(seq),
            accepted: true,
            error_code: None,
        });

        // 折叠推进：declare_head 触发起点跳变（完整求值），撤销/恢复在窗口内重放，
        // 其余原子直接应用。窗口由快照基线保证（6.2 / 6.5）。
        match kind {
            AtomKind::DeclareHead => match crate::seq::state_at(&log, seq, &mut state_cache) {
                Ok(evaluated) => {
                    state = evaluated.state;
                    base_state = state.clone();
                    base_seq = seq;
                }
                Err(_) => state = fold_atoms(base_state.clone(), &[]).state,
            },
            AtomKind::Revert | AtomKind::Reapply => {
                let slice = log.range_exclusive_inclusive(base_seq, seq);
                state = fold_atoms(base_state.clone(), slice).state;
            }
            _ => {
                // 刚追加的原子就是 head，无需按 seq 反查。
                if let Some(head) = log.head_atom().cloned() {
                    // 与折叠器保持一致：precondition 失败即为级联失效，不得单方面应用。
                    if precondition(&state, &head).is_ok() {
                        let _ = apply(&mut state, &head);
                        state.head_seq = head.seq;
                        state.head_atom = Some(head.id.clone());
                    } else {
                        let slice = log.range_exclusive_inclusive(base_seq, seq);
                        state = fold_atoms(base_state.clone(), slice).state;
                    }
                }
            }
        }

        update_model(&mut model, &state, &log, &atom_id, seq, kind, base_seq);
        if config.snapshot_window > 0 && seq - base_seq >= config.snapshot_window as u64 {
            let base_ref = match log.last_declare_head_upto(seq) {
                Some(head) => SnapshotBase::DeclareHead(head.id.clone()),
                None => SnapshotBase::Snapshot("root".to_owned()),
            };
            if let Ok(snapshot) = Snapshot::from_state(seq, base_ref, state.clone()) {
                base_state = snapshot.state.clone();
                base_seq = seq;
                stats.snapshots += 1;
                snapshots.push(snapshot);
            }
        }
    }

    Scenario {
        log,
        store,
        steps,
        stats,
        final_state: state,
        snapshots,
    }
}

fn update_model(
    model: &mut Model,
    state: &DocumentState,
    log: &AtomLog,
    atom_id: &str,
    seq: Seq,
    kind: AtomKind,
    base_seq: Seq,
) {
    // 存活实体直接从折叠状态派生，保证生成器只会挑选真实存活的目标。
    let alive = alive_view(state);
    model.layers = alive.layers;
    model.objects = alive.objects;
    model.styles = alive.styles;
    model.masks = alive.masks;
    model.selections = alive.selections;
    model.unlocked_layers = alive.unlocked_layers;
    model.unlocked_objects = alive.unlocked_objects;

    if kind.is_state_effect() {
        model.revertable.push((atom_id.to_owned(), seq));
    }
    match kind {
        AtomKind::Revert => {
            if let Some(target) = log.get(atom_id).and_then(|atom| atom.target_atom()) {
                model.revertable.retain(|(id, _)| id != target);
                if !model.suppressed.iter().any(|id| id == target) {
                    model.suppressed.push(target.to_owned());
                }
            }
        }
        AtomKind::Reapply => {
            if let Some(target) = log.get(atom_id).and_then(|atom| atom.target_atom()) {
                model.suppressed.retain(|id| id != target);
                if let Some(target_seq) = log.seq_of(target) {
                    model.revertable.push((target.to_owned(), target_seq));
                }
            }
        }
        _ => {}
    }
    // 撤销目标不得越出当前基线窗口，否则窗口重放不再等价于完整折叠。
    model.revertable.retain(|(_, seq)| *seq > base_seq);
    model
        .suppressed
        .retain(|id| log.seq_of(id).map(|seq| seq > base_seq).unwrap_or(false));
}

#[allow(clippy::too_many_arguments)]
fn build_atom(
    rng: &mut Rng,
    ids: &mut UlidGen,
    store: &mut MemoryBlobStore,
    model: &Model,
    state: &DocumentState,
    config: &ScenarioConfig,
    actor: &str,
    session: &str,
    step: usize,
) -> Atom {
    // 实体 id 由步号派生：天然唯一，且不依赖提交是否成功。
    let new_layer = format!("layer_{step}");
    let new_object = format!("obj_{step}");
    let new_style = format!("style_{step}");
    let new_mask = format!("mask_{step}");
    let new_selection = format!("sel_{step}");

    let mut atom = match choose_op(rng, model, state, config, step) {
        Op::CreateDocument => Atom::new(
            AtomKind::CreateDocument,
            actor,
            session,
            json!({
                "doc_id": "doc_1",
                "width": 512,
                "height": 512,
                "color_space": "srgb",
            }),
        ),
        Op::CreateLayer => Atom::new(
            AtomKind::CreateLayer,
            actor,
            session,
            json!({"layer_id": new_layer, "name": new_layer}),
        ),
        Op::CreateNestedLayer => {
            let parent = rng.pick(&model.layers);
            Atom::new(
                AtomKind::CreateLayer,
                actor,
                session,
                json!({"layer_id": new_layer, "parent_id": parent, "type": "layer_group"}),
            )
        }
        Op::CreateObject => {
            let layer_id = rng.pick(&model.layers).unwrap_or_default();
            Atom::new(
                AtomKind::CreateObject,
                actor,
                session,
                json!({
                    "object_id": new_object,
                    "layer_id": layer_id,
                    "type": ObjectKind::pick(rng).as_str(),
                    "z_index": rng.below(8),
                }),
            )
        }
        Op::DrawStroke => {
            // **只往未锁定的图层里落笔** ✓（锁定强制会合法拒绝写到锁层的原子 ✓，
            // 而生成器不该制造注定被拒的原子 ✓ —— 见 `AliveView` 的说明 ✓）。
            let layer_id = rng.pick(&model.unlocked_layers).unwrap_or_default();
            Atom::new(
                AtomKind::DrawStroke,
                actor,
                session,
                json!({
                    "object_id": new_object,
                    "layer_id": layer_id,
                    "brush": "round",
                    "size": 1.0 + rng.below(20) as f64 / 4.0,
                    "data": {"points": [[0.0, 0.0], [rng.below(64) as f64, rng.below(64) as f64]]},
                }),
            )
        }
        Op::Supersede => {
            let (object_id, _) = rng.pick(&model.unlocked_objects).unwrap_or_default();
            Atom::new(
                AtomKind::Supersede,
                actor,
                session,
                json!({"object_id": object_id, "data": {"v": step}}),
            )
        }
        Op::SetPropertyObject => {
            // **先选 key、再选目标** ✓ —— 顺序很关键 ✓：`locked` 这个 key **必须**能作用于
            // 已经锁上的对象 ✓，否则永远解不开锁 ✓（而其它属性只能改未锁定的 ✓）。
            let (key, value) = match rng.below(3) {
                0 => ("visible", json!(rng.permille(500))),
                1 => ("locked", json!(rng.permille(500))),
                _ => ("z_index", json!(rng.below(8))),
            };
            let targets = if key == "locked" {
                &model.objects
            } else {
                &model.unlocked_objects
            };
            let (object_id, _) = rng.pick(targets).unwrap_or_default();
            Atom::new(
                AtomKind::SetProperty,
                actor,
                session,
                json!({"object_id": object_id, "key": key, "value": value}),
            )
        }
        Op::SetPropertyLayer => {
            // 图层的这三个属性都**不涉及锁** ✓ ⇒ 一律只改未锁定的图层 ✓
            //（图层锁也有 `lock_layer`/`unlock_layer` 两个专门工具 ✓，fuzz 由它们覆盖 ✓）。
            let layer_id = rng.pick(&model.unlocked_layers).unwrap_or_default();
            let (key, value) = match rng.below(3) {
                0 => ("visible", json!(rng.permille(500))),
                1 => ("opacity", json!(rng.below(100) as f64 / 100.0)),
                _ => ("blend_mode", json!("multiply")),
            };
            Atom::new(
                AtomKind::SetProperty,
                actor,
                session,
                json!({"layer_id": layer_id, "key": key, "value": value}),
            )
        }
        Op::ReorderLayers => {
            let mut order: Vec<String> = state
                .alive_layers()
                .iter()
                .map(|layer| layer.id.clone())
                .collect();
            rng.shuffle(&mut order);
            Atom::new(
                AtomKind::ReorderLayers,
                actor,
                session,
                json!({"order": order}),
            )
        }
        Op::CreateStyle => Atom::new(
            AtomKind::CreateStyle,
            actor,
            session,
            json!({"style_id": new_style, "name": new_style, "category": "ink"}),
        ),
        Op::StyledObject => {
            let layer_id = rng.pick(&model.layers).unwrap_or_default();
            let style_id = rng.pick(&model.styles).unwrap_or_default();
            Atom::new(
                AtomKind::DrawShape,
                actor,
                session,
                json!({
                    "object_id": new_object,
                    "layer_id": layer_id,
                    "style_id": style_id,
                    "data": {"geometry": {"kind": "rect"}},
                }),
            )
        }
        Op::CreateMask => Atom::new(
            AtomKind::CreateMask,
            actor,
            session,
            json!({"mask_id": new_mask, "shape": {"kind": "rect"}, "feather": 2.0}),
        ),
        Op::BindMask => {
            let layer_id = rng.pick(&model.layers).unwrap_or_default();
            let mask_id = rng.pick(&model.masks).unwrap_or_default();
            Atom::new(
                AtomKind::SetProperty,
                actor,
                session,
                json!({"layer_id": layer_id, "key": "mask_id", "value": mask_id}),
            )
        }
        Op::CreateSelection => Atom::new(
            AtomKind::CreateSelection,
            actor,
            session,
            json!({"selection_id": new_selection, "shape": {"kind": "ellipse"}}),
        ),
        Op::TombstoneObject => {
            let (object_id, _) = rng.pick(&model.unlocked_objects).unwrap_or_default();
            Atom::new(
                AtomKind::Tombstone,
                actor,
                session,
                json!({"object_id": object_id}),
            )
        }
        Op::TombstoneLayer => {
            let layer_id = rng.pick(&model.unlocked_layers).unwrap_or_default();
            Atom::new(
                AtomKind::Tombstone,
                actor,
                session,
                json!({"layer_id": layer_id}),
            )
        }
        Op::CreateCheckpoint => {
            let checkpoint_id = format!("ckpt_{step}");
            Atom::new(
                AtomKind::CreateCheckpoint,
                actor,
                session,
                json!({
                    "checkpoint_id": checkpoint_id,
                    "name": checkpoint_id,
                    "anchor_seq": state.head_seq,
                }),
            )
        }
        Op::DeclareHead => {
            let target = model
                .revertable
                .last()
                .map(|(id, _)| id.clone())
                .unwrap_or_default();
            Atom::new(
                AtomKind::DeclareHead,
                actor,
                session,
                json!({"base": {"type": "atom", "id": target}, "reason": "revert_to"}),
            )
        }
        Op::Retouch => {
            let (object_id, layer_id) = rng.pick(&model.objects).unwrap_or_default();
            Atom::new(
                AtomKind::Retouch,
                actor,
                session,
                json!({
                    "object_id": object_id,
                    "layer_id": layer_id,
                    "region": {
                        "x": rng.below(64) as f64,
                        "y": rng.below(64) as f64,
                        "w": 8.0,
                        "h": 8.0,
                    },
                }),
            )
        }
        Op::RasterPatch => {
            let layer_id = rng.pick(&model.layers).unwrap_or_default();
            let mut payload = json!({
                "object_id": new_object,
                "layer_id": layer_id,
                "type": "raster_patch",
                "sampling": rng.permille(500),
                "prompt": "把天空换成晚霞",
                "region": {"x": 0, "y": 0, "w": 32, "h": 32},
            });
            if config.blob_bytes > 0 {
                let bytes = blob_bytes(step, config.blob_bytes);
                if let Ok(blob_ref) = write_blob(store, &bytes, "image/webp") {
                    payload["bitmap"] = json!(blob_ref);
                }
            }
            Atom::new(AtomKind::CreateObject, actor, session, payload)
        }
        Op::Revert => {
            let target = rng
                .pick(&model.revertable)
                .map(|(id, _)| id)
                .unwrap_or_default();
            Atom::new(AtomKind::Revert, actor, session, json!({"target": target}))
        }
        Op::Reapply => {
            let target = rng.pick(&model.suppressed).unwrap_or_default();
            Atom::new(AtomKind::Reapply, actor, session, json!({"target": target}))
        }
        Op::RevertStaleTarget => {
            // 刻意构造一个指向很旧原子的 revert：覆盖跨求值起点 / 已出窗口的拒绝路径。
            let target = model
                .revertable
                .first()
                .map(|(id, _)| id.clone())
                .unwrap_or_default();
            Atom::new(AtomKind::Revert, actor, session, json!({"target": target}))
        }
        Op::InvalidReference => {
            let object_id = format!("obj_missing_{step}");
            Atom::new(
                AtomKind::Supersede,
                actor,
                session,
                json!({"object_id": object_id, "data": {"v": -1}}),
            )
        }
    };
    atom.id = ids.next_id();
    atom
}

/// 生成与步号相关、但仍可从内容判断的确定性 blob 内容。
fn blob_bytes(step: usize, size: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; size];
    let stamp = (step as u32).to_le_bytes();
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = (index as u8).wrapping_add(stamp[index % 4]);
    }
    bytes
}

fn write_blob(store: &MemoryBlobStore, bytes: &[u8], mime: &str) -> crate::error::Result<BlobRef> {
    crate::blob::stage_blob(store, bytes, mime)
}

/// 对象类型选择。
#[derive(Debug, Clone, Copy)]
enum ObjectKind {
    Stroke,
    Shape,
    Text,
}

impl ObjectKind {
    fn pick(rng: &mut Rng) -> Self {
        match rng.below(3) {
            0 => Self::Stroke,
            1 => Self::Shape,
            _ => Self::Text,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Stroke => "stroke",
            Self::Shape => "shape",
            Self::Text => "text",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum Op {
    CreateDocument,
    CreateLayer,
    CreateNestedLayer,
    CreateObject,
    DrawStroke,
    Supersede,
    SetPropertyObject,
    SetPropertyLayer,
    ReorderLayers,
    CreateStyle,
    StyledObject,
    CreateMask,
    BindMask,
    CreateSelection,
    TombstoneObject,
    TombstoneLayer,
    CreateCheckpoint,
    DeclareHead,
    Retouch,
    RasterPatch,
    Revert,
    Reapply,
    RevertStaleTarget,
    InvalidReference,
}

fn choose_op(
    rng: &mut Rng,
    model: &Model,
    state: &DocumentState,
    config: &ScenarioConfig,
    step: usize,
) -> Op {
    // 开头先建立文档、图层与对象，之后按权重随机。
    if state.doc_id.is_none() {
        return Op::CreateDocument;
    }
    if model.layers.is_empty() {
        return Op::CreateLayer;
    }
    // 保证带 blob 的场景一定覆盖“大二进制外置 + 提交顺序协议”。
    if config.blob_bytes > 0 && step == 2 {
        return Op::RasterPatch;
    }
    if model.objects.is_empty() && !rng.permille(200) {
        return Op::CreateObject;
    }
    if config.revert_permille > 0 && rng.permille(config.revert_permille) {
        return if model.suppressed.is_empty() || rng.permille(400) {
            Op::Revert
        } else {
            Op::Reapply
        };
    }
    if config.sampling_permille > 0 && rng.permille(config.sampling_permille) {
        return Op::Retouch;
    }

    // 实体数量上限：达到上限后只做修改类操作，避免状态无界增长。
    let can_add_layer = model.layers.len() < config.max_entities;
    let can_add_object = model.objects.len() < config.max_entities * 2;
    let can_add_aux = model.styles.len() < config.max_entities
        && model.masks.len() < config.max_entities
        && model.selections.len() < config.max_entities;

    match rng.below(100) {
        0..=9 if can_add_layer => Op::CreateLayer,
        10..=17 if can_add_layer && model.layers.len() > 1 && rng.permille(300) => {
            Op::CreateNestedLayer
        }
        18..=19 if can_add_layer => Op::CreateLayer,
        20..=30 if can_add_object => Op::CreateObject,
        31..=36 if can_add_object => Op::DrawStroke,
        37..=52 => Op::Supersede,
        53..=62 => Op::SetPropertyObject,
        63..=68 => Op::SetPropertyLayer,
        69..=73 if model.layers.len() > 1 => Op::ReorderLayers,
        74..=77 if can_add_aux => Op::CreateStyle,
        78..=80 if can_add_object && !model.styles.is_empty() => Op::StyledObject,
        81..=84 if can_add_aux => Op::CreateMask,
        85..=87 if !model.masks.is_empty() => Op::BindMask,
        88..=90 if can_add_aux => Op::CreateSelection,
        91..=93 if !model.objects.is_empty() => Op::TombstoneObject,
        94 if model.layers.len() > 2 && rng.permille(200) => Op::TombstoneLayer,
        95..=96 => Op::CreateCheckpoint,
        97 if !model.revertable.is_empty() && step > 4 => Op::DeclareHead,
        98 if can_add_object => Op::RasterPatch,
        99 => {
            if rng.permille(500) {
                Op::InvalidReference
            } else {
                Op::RevertStaleTarget
            }
        }
        _ => Op::Supersede,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let seq_a: Vec<u64> = (0..32).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..32).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
        assert!(seq_a.windows(2).all(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn scenario_is_reproducible() {
        let config = ScenarioConfig::small(11);
        let first = generate(&config);
        let second = generate(&config);
        assert_eq!(first.log.len(), second.log.len());
        let ids_a: Vec<&str> = first.log.iter().map(|atom| atom.id.as_str()).collect();
        let ids_b: Vec<&str> = second.log.iter().map(|atom| atom.id.as_str()).collect();
        assert_eq!(ids_a, ids_b);
        assert_eq!(first.steps, second.steps);
    }

    #[test]
    fn scenario_produces_every_session_and_rejections() {
        let scenario = generate(&ScenarioConfig::small(3));
        assert!(scenario.log.len() > 20, "场景应产生足量原子");
        let sessions: std::collections::BTreeSet<&str> = scenario
            .log
            .iter()
            .map(|atom| atom.session.as_str())
            .collect();
        assert!(sessions.contains("session:a"));
        assert!(sessions.contains("session:b"));
        assert!(scenario.stats.rejected > 0, "生成器应覆盖提交被拒绝的路径");
        assert!(scenario.final_state.is_consistent());
    }

    #[test]
    fn acceptance_rate_is_high_for_a_bounded_scene() {
        let scenario = generate(&ScenarioConfig {
            seed: 42,
            steps: 600,
            blob_bytes: 0,
            snapshot_window: 200,
            revert_permille: 100,
            sampling_permille: 30,
            max_entities: 12,
        });
        assert!(
            scenario.acceptance_rate() > 0.8,
            "接受率过低：{:.2}（{:?}）",
            scenario.acceptance_rate(),
            scenario.stats.rejections_by_code
        );
        assert!(scenario.final_state.is_consistent());
        assert_eq!(
            scenario.final_state,
            crate::FoldEngine::new().fold(&scenario.log).unwrap().state,
            "生成器维护的状态必须与完整折叠一致"
        );
    }

    #[test]
    fn scenario_respects_entity_bounds() {
        let config = ScenarioConfig {
            seed: 9,
            steps: 800,
            blob_bytes: 0,
            snapshot_window: 100,
            revert_permille: 80,
            sampling_permille: 0,
            max_entities: 6,
        };
        let scenario = generate(&config);
        assert!(
            scenario.final_state.objects.len() <= config.max_entities * 8,
            "对象数量 {} 超出预期",
            scenario.final_state.objects.len()
        );
    }
}
