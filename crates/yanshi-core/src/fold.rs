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
use crate::state::{
    Checkpoint, DeclareHead, DocumentState, HeadBase, Layer, LayerType, Object, ObjectType,
    Selection, Style, Transform,
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

/// 计算被有效 revert 撤销的原子集合（设计文档 5.3 有效集）。
pub fn compute_suppressed(atoms: &[Atom]) -> BTreeSet<AtomId> {
    /// 有效动作。
    #[derive(Clone, Copy, PartialEq)]
    enum Action {
        Revert,
        Reapply,
    }

    // 目标原子 id -> 作用于它的有效动作（seq 最大者，即最先在倒序扫描中遇到者）。
    let mut latest_action: BTreeMap<AtomId, (Seq, Action)> = BTreeMap::new();
    let mut suppressed = BTreeSet::new();

    for atom in atoms.iter().rev() {
        let existing = latest_action.get(&atom.id).copied();
        match atom.kind {
            AtomKind::Revert => {
                // revert 自身被更晚的有效 revert 撤销时失效（revert(revert(x)) ≡ reapply(x)）。
                let effective = !matches!(existing, Some((_, Action::Revert)));
                if effective {
                    if let Some(target) = atom.target_atom() {
                        latest_action
                            .entry(target.to_owned())
                            .or_insert((atom.seq, Action::Revert));
                    }
                } else {
                    suppressed.insert(atom.id.clone());
                }
            }
            AtomKind::Reapply => {
                // reapply 不是 revert 的合法目标（5.4），永远有效。
                if let Some(target) = atom.target_atom() {
                    latest_action
                        .entry(target.to_owned())
                        .or_insert((atom.seq, Action::Reapply));
                }
            }
            _ => {
                if matches!(existing, Some((_, Action::Revert))) {
                    suppressed.insert(atom.id.clone());
                }
            }
        }
    }
    suppressed
}

/// 折叠一段原子：`base` 为起点状态，`atoms` 必须按 seq 升序。
///
/// 区间内若出现 `declare_head`，按设计文档 5.5 它应由 `state@seq` 公式处理，
/// 本函数只记录警告（见 [`crate::seq`]）。
pub fn fold_atoms(base: DocumentState, atoms: &[Atom]) -> FoldResult {
    let suppressed = compute_suppressed(atoms);
    let mut state = base;
    let mut warnings = Vec::new();
    let mut applied = Vec::new();

    for atom in atoms {
        if suppressed.contains(&atom.id) {
            continue;
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

fn missing_error(kind: &str, id: &str) -> YanshiError {
    err(ErrorCode::ReferenceNotFound, format!("{kind} {id} 不存在"))
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
                    return Err(missing_error("父图层", parent));
                }
            }
        }
        AtomKind::CreateObject | AtomKind::ImportImage => {
            let object_id = required(atom, "object_id")?;
            // **实例必须在折叠层就能挡住循环引用** ✓（设计 9.3「循环引用检测：拒绝创建并返回错误」✓）——
            // 只在工具层检查是不够的 ✗：原子可以直接写进日志 ✓，
            // 而重放时遇到环会**无限递归** ✓（渲染时解析 master 会一直往下走 ✓）。
            if atom.kind == AtomKind::CreateObject
                && atom.payload.get("type").and_then(Value::as_str) == Some("instance")
            {
                let master = atom
                    .payload
                    .get("data")
                    .and_then(|data| data.get("master_ref"))
                    .and_then(|master_ref| master_ref.get("object_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        err(
                            ErrorCode::InvalidArgument,
                            "instance 缺少 data.master_ref.object_id".to_owned(),
                        )
                    })?;
                // 顺着 master 链走一遍 ✓：走到自己 ⇒ 成环 ✓（含**自引用** ✓）。
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
                        Some(object)
                            if object.object_type == crate::state::ObjectType::Instance =>
                        {
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
                        // **master 暂时不存在不算错** ✓ —— 日志顺序允许先建实例后建 master ✓，
                        // 渲染时解析不到就什么都不画 ✓（见 `render.rs` ✓），master 补齐后自动恢复 ✓。
                        _ => break,
                    }
                }
            }
            if state.objects.contains_key(object_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("对象 {object_id} 已存在"),
                ));
            }
            let layer_id = required(atom, "layer_id")?;
            if !state.layer_alive(layer_id) {
                return Err(missing_error("图层", layer_id));
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
                        return Err(missing_error("图层", layer_id));
                    }
                }
            }
        }
        AtomKind::Supersede
        | AtomKind::Move
        | AtomKind::Transform
        | AtomKind::Tombstone
        | AtomKind::SetProperty => {
            if let Some(object_id) = payload_str(&atom.payload, "object_id") {
                match state.objects.get(object_id) {
                    Some(object) if object.is_deleted() => {
                        return Err(tombstone_error("对象", object_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("对象", object_id)),
                }
            } else if let Some(layer_id) = payload_str(&atom.payload, "layer_id") {
                match state.layers.get(layer_id) {
                    Some(layer) if layer.is_deleted() => {
                        return Err(tombstone_error("图层", layer_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("图层", layer_id)),
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
                    None => return Err(missing_error("选区", selection_id)),
                }
            } else if let Some(mask_id) = payload_str(&atom.payload, "mask_id") {
                match state.masks.get(mask_id) {
                    Some(mask) if mask.is_deleted() => {
                        return Err(tombstone_error("蒙版", mask_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("蒙版", mask_id)),
                }
            } else if let Some(style_id) = payload_str(&atom.payload, "style_id") {
                match state.styles.get(style_id) {
                    Some(style) if style.is_deleted() => {
                        return Err(tombstone_error("风格", style_id))
                    }
                    Some(_) => {}
                    None => return Err(missing_error("风格", style_id)),
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
                    None => return Err(missing_error("图层", layer_id)),
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
        AtomKind::CreateSelection => {
            let selection_id = required(atom, "selection_id")?;
            if state.selections.contains_key(selection_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("选区 {selection_id} 已存在"),
                ));
            }
        }
        AtomKind::CreateMask => {
            let mask_id = required(atom, "mask_id")?;
            if state.masks.contains_key(mask_id) {
                return Err(err(
                    ErrorCode::PreconditionFailed,
                    format!("蒙版 {mask_id} 已存在"),
                ));
            }
        }
        AtomKind::CreateStyle => {
            let style_id = required(atom, "style_id")?;
            if state.styles.contains_key(style_id) {
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
        AtomKind::Tag
        | AtomKind::Comment
        | AtomKind::Suggest
        | AtomKind::AcceptSuggestion
        | AtomKind::RejectSuggestion => {}
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
            "mask_id" => state.masks.get(id).is_some_and(|m| !m.is_deleted()),
            "parent_id" | "layer_id" => state.layer_alive(id),
            _ => true,
        };
        if !alive {
            return Err(missing_error(key, id));
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
                id: object_id.clone(),
                layer_id,
                object_type,
                z_index: payload_u64(&atom.payload, "z_index")
                    .map(|z| z as i64)
                    .unwrap_or(0),
                visible: payload_bool(&atom.payload, "visible").unwrap_or(true),
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
                    id: object_id.clone(),
                    layer_id,
                    object_type,
                    z_index: payload_u64(&atom.payload, "z_index")
                        .map(|z| z as i64)
                        .unwrap_or(0),
                    visible: payload_bool(&atom.payload, "visible").unwrap_or(true),
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
                if let Some(object) = state.objects.get_mut(&object_id) {
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
            let selection = parse_selection(atom, selection_id.clone(), blobs);
            state.selections.insert(selection_id, selection);
        }

        AtomKind::CreateMask => {
            let mask_id = required(atom, "mask_id")?.to_owned();
            let mask = parse_selection(atom, mask_id.clone(), blobs);
            state.masks.insert(mask_id, mask);
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

fn parse_selection(atom: &Atom, id: String, blobs: Vec<crate::atom::BlobHash>) -> Selection {
    Selection {
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
        created_by: atom.id.clone(),
        deleted_by: None,
    }
}

fn supersede_object(
    state: &mut DocumentState,
    atom: &Atom,
    object_id: &str,
    blobs: Option<Vec<crate::atom::BlobHash>>,
) -> Result<()> {
    let Some(object) = state.objects.get_mut(object_id) else {
        return Err(missing_error("对象", object_id));
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
        "visible" => object.visible = value.as_bool().unwrap_or(true),
        "locked" => object.locked = value.as_bool().unwrap_or(false),
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
        "visible" => layer.visible = value.as_bool().unwrap_or(true),
        "locked" => layer.locked = value.as_bool().unwrap_or(false),
        "alpha_lock" => layer.alpha_lock = value.as_bool().unwrap_or(false),
        "clipping_mask" => layer.clipping_mask = value.as_bool().unwrap_or(false),
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
    let mut targets = vec![layer_id.to_owned()];
    let mut cursor = 0;
    while cursor < targets.len() {
        let current = targets[cursor].clone();
        cursor += 1;
        for layer in state.layers.values() {
            if layer.parent_id.as_deref() == Some(current.as_str()) {
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
        assert_eq!(order, vec!["layer_1", "layer_2"]);

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
        assert_eq!(result.warning_count(WarningKind::CascadeInvalidation), 1);
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
}
