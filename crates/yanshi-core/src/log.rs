//! Append-Only 原子日志（设计文档 5.1 / 12.1 / 12.2）。
//!
//! - 原子只追加，不删除、不修改；服务端按接收顺序分配权威 `seq`。
//! - 客户端生成 ULID，服务端按 id 去重，从而获得幂等与重试安全。
//! - 提交时在**当前状态**上做 precondition 校验：引用不存在 → 拒绝；
//!   引用已被 tombstone → 拒绝；blob 缺失 → 拒绝（提交顺序协议，6.3）。

use crate::atom::{Atom, AtomId, AtomKind, BlobHash};
use crate::conflict::{find_conflict, is_sampling_replace};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::Seq;
use crate::state::{DocumentState, HeadBase};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// 追加结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AppendOutcome {
    /// 首次写入，获得新 seq。
    Appended {
        /// 权威序号。
        seq: Seq,
    },
    /// id 已存在：幂等命中，返回既有 seq。
    Duplicate {
        /// 既有权威序号。
        seq: Seq,
    },
}

impl AppendOutcome {
    /// 权威序号。
    pub fn seq(self) -> Seq {
        match self {
            Self::Appended { seq } | Self::Duplicate { seq } => seq,
        }
    }

    /// 是否首次写入。
    pub fn is_appended(self) -> bool {
        matches!(self, Self::Appended { .. })
    }
}

/// 提交时校验所需的上下文（设计文档 12.2）。
pub struct CommitContext<'a> {
    /// 当前状态（当前 HEAD 的折叠结果）。
    pub state: &'a DocumentState,
    /// blob 是否存在（提交顺序协议：blob 先行，原子后行）。
    pub blob_exists: &'a dyn Fn(&BlobHash) -> bool,
    /// 提交者。
    pub actor: &'a str,
    /// 提交会话。
    pub session: &'a str,
    /// 是否允许跨 actor revert（默认 false，需 owner 权限，设计文档 12.5）。
    pub allow_cross_actor_revert: bool,
}

impl<'a> CommitContext<'a> {
    /// 构造。测试或不涉及 blob 的场景可传入恒返回 `false` 的闭包。
    pub fn new(
        state: &'a DocumentState,
        blob_exists: &'a dyn Fn(&BlobHash) -> bool,
        actor: &'a str,
        session: &'a str,
    ) -> Self {
        Self {
            state,
            blob_exists,
            actor,
            session,
            allow_cross_actor_revert: false,
        }
    }

    /// 允许跨 actor revert。
    pub fn allow_cross_actor_revert(mut self, allow: bool) -> Self {
        self.allow_cross_actor_revert = allow;
        self
    }
}

/// 冲突检测的并发窗口上限，与设计文档 6.2 的重放上限一致。
pub const CONFLICT_WINDOW_ATOMS: Seq = 1000;

/// 原子引用的实体必须存在、必须不存在，或不做检查。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefMode {
    /// 必须存在且未删除。
    MustExist,
    /// 必须不存在（创建类原子）。
    MustNotExist,
    /// 不检查。
    Ignore,
}

fn object_ref_mode(kind: AtomKind) -> RefMode {
    match kind {
        AtomKind::CreateObject | AtomKind::ImportImage => RefMode::MustNotExist,
        AtomKind::Supersede
        | AtomKind::Move
        | AtomKind::Transform
        | AtomKind::SetProperty
        | AtomKind::Tombstone => RefMode::MustExist,
        // 绘制类：对象不存在则创建，存在则叠加新版本。
        AtomKind::DrawStroke
        | AtomKind::DrawShape
        | AtomKind::DrawText
        | AtomKind::Fill
        | AtomKind::Erase
        | AtomKind::Retouch
        | AtomKind::Liquify => RefMode::Ignore,
        _ => RefMode::Ignore,
    }
}

fn layer_ref_mode(kind: AtomKind) -> RefMode {
    match kind {
        AtomKind::CreateLayer => RefMode::MustNotExist,
        AtomKind::CreateDocument => RefMode::Ignore,
        _ => RefMode::MustExist,
    }
}

/// 主实体的蒙版/选区/风格引用模式：创建类原子要求“尚不存在”，其余要求“存在且未删除”。
fn aux_ref_mode(kind: AtomKind) -> RefMode {
    match kind {
        AtomKind::CreateMask | AtomKind::CreateSelection | AtomKind::CreateStyle => {
            RefMode::MustNotExist
        }
        AtomKind::CreateDocument => RefMode::Ignore,
        _ => RefMode::MustExist,
    }
}

/// Append-Only 原子日志。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtomLog {
    atoms: Vec<Atom>,
    #[serde(skip)]
    by_id: HashMap<AtomId, usize>,
    next_seq: Seq,
}

impl AtomLog {
    /// 空日志。服务端分配的 seq 从 1 开始，0 表示未提交。
    pub fn new() -> Self {
        Self {
            atoms: Vec::new(),
            by_id: HashMap::new(),
            next_seq: 1,
        }
    }

    /// 由已带 seq 的原子恢复日志（服务端重启恢复、测试构造）。
    ///
    /// 原子按 seq 排序；id 与 seq 必须唯一。
    pub fn with_atoms(mut atoms: Vec<Atom>) -> Result<Self> {
        atoms.sort_by_key(|a| (a.seq, a.id.clone()));
        let mut log = Self::new();
        for atom in atoms {
            if !atom.is_submitted() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("原子 {} 缺少权威 seq", atom.id)),
                )
                .with_atom(atom.id.clone()));
            }
            if log.by_id.contains_key(&atom.id) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("原子 id 重复: {}", atom.id)),
                )
                .with_atom(atom.id.clone()));
            }
            if log.atoms.iter().any(|existing| existing.seq == atom.seq) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("seq {} 重复", atom.seq)),
                )
                .with_atom(atom.id.clone()));
            }
            log.next_seq = log.next_seq.max(atom.seq + 1);
            log.by_id.insert(atom.id.clone(), log.atoms.len());
            log.atoms.push(atom);
        }
        Ok(log)
    }

    /// 无条件追加（服务端内部、测试与重放路径）。按 id 幂等去重。
    ///
    /// 面向客户端提交的路径请用 [`AtomLog::append_validated`]。
    pub fn append(&mut self, mut atom: Atom) -> Result<AppendOutcome> {
        if let Some(index) = self.by_id.get(&atom.id) {
            return Ok(AppendOutcome::Duplicate {
                seq: self.atoms[*index].seq,
            });
        }
        atom.seq = self.next_seq;
        self.next_seq += 1;
        self.by_id.insert(atom.id.clone(), self.atoms.len());
        self.atoms.push(atom);
        Ok(AppendOutcome::Appended {
            seq: self.next_seq - 1,
        })
    }

    /// 提交时校验（设计文档 12.2）+ 追加。
    ///
    /// 校验失败时原子**不进日志**，返回 `precondition_failed` / `reference_not_found`
    /// / `conflict` / `permission_denied` 等错误。
    pub fn append_validated(
        &mut self,
        atom: Atom,
        commit: &CommitContext<'_>,
    ) -> Result<AppendOutcome> {
        // 幂等命中优先于一切校验：重试同一原子必须成功。
        if let Some(index) = self.by_id.get(&atom.id) {
            return Ok(AppendOutcome::Duplicate {
                seq: self.atoms[*index].seq,
            });
        }
        self.validate_commit(&atom, commit)?;
        self.append(atom)
    }

    /// 仅做提交时校验，不写入。
    pub fn validate_commit(&self, atom: &Atom, commit: &CommitContext<'_>) -> Result<()> {
        self.validate_blobs(atom, commit)?;
        self.validate_primary_refs(atom, commit.state)?;
        self.validate_secondary_refs(atom, commit.state)?;
        // **实例的成环检查必须在这里** ✓ —— 这里返回 `Err` 才是**拒绝提交** ✓。
        //
        // 我第一版只把它放进 `fold::precondition` ✗ ⇒ 那一条**返回 Err 时只推一条警告并跳过** ✓
        //（见 `fold.rs` 的折叠循环 ✓）⇒ 原子被**静默丢弃** ✓、工具却返回 `ok: true` ✓，
        // 表现为"链接没生效但也没报错" ✓（测试输出：`通过改写制造环必须被拒：… ok:true` ✓）。
        // `fold::precondition` 的那一份**保留** ✓ 作为重放期的纵深防御 ✓
        //（手工写进日志的环在折叠时也会被跳过并留下警告 ✓），但**判定提交与否的是这里** ✓。
        self.validate_instance_cycles(atom, commit.state)?;
        self.validate_history_refs(atom, commit)?;
        self.validate_conflict(atom)?;
        Ok(())
    }

    fn validate_blobs(&self, atom: &Atom, commit: &CommitContext<'_>) -> Result<()> {
        for hash in atom.all_blob_refs() {
            if !(commit.blob_exists)(&hash) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!(
                        "原子 {} 引用的 blob 尚未上传（违反提交顺序协议）",
                        atom.id
                    )),
                )
                .with_atom(atom.id.clone())
                .with_blob(hash.to_string()));
            }
        }
        Ok(())
    }

    fn validate_primary_refs(&self, atom: &Atom, state: &DocumentState) -> Result<()> {
        let primary_object = atom.object_id().map(str::to_owned);
        let primary_layer = atom.layer_id().map(str::to_owned);

        if let Some(object_id) = &primary_object {
            match object_ref_mode(atom.kind) {
                RefMode::MustExist => {
                    check_object_exists(state, &atom.id, object_id, atom.kind)?;
                }
                RefMode::MustNotExist => {
                    if state.objects.contains_key(object_id) {
                        return Err(YanshiError::new(
                            ErrorCode::PreconditionFailed,
                            ErrorContext::detail(format!("对象 {object_id} 已存在")),
                        )
                        .with_atom(atom.id.clone())
                        .with_object(object_id.clone()));
                    }
                }
                RefMode::Ignore => {}
            }
        }
        // set_property 的属性值引用（mask_id / style_id / parent_id / layer_id）。
        if atom.kind == AtomKind::SetProperty {
            if let (Some(key), Some(value)) = (
                crate::atom::payload_str(&atom.payload, "key"),
                atom.payload
                    .get("value")
                    .and_then(serde_json::Value::as_str),
            ) {
                let alive = match key {
                    "mask_id" => state.masks.get(value).is_some_and(|m| !m.is_deleted()),
                    "style_id" => state.styles.get(value).is_some_and(|s| !s.is_deleted()),
                    "parent_id" | "layer_id" => state.layer_alive(value),
                    _ => true,
                };
                if !alive {
                    return Err(YanshiError::new(
                        ErrorCode::ReferenceNotFound,
                        ErrorContext::detail(format!(
                            "set_property {key} 引用的实体 {value} 不存在或已删除"
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
        }
        // 蒙版 / 选区 / 风格的主实体引用（创建类原子要求尚不存在）。
        if aux_ref_mode(atom.kind) == RefMode::MustNotExist {
            if let Some(mask_id) = crate::atom::payload_str(&atom.payload, "mask_id") {
                if state.masks.contains_key(mask_id) {
                    return Err(YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!("蒙版 {mask_id} 已存在")),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
            if let Some(selection_id) = crate::atom::payload_str(&atom.payload, "selection_id") {
                if state.selections.contains_key(selection_id) {
                    return Err(YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!("选区 {selection_id} 已存在")),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
            if let Some(style_id) = crate::atom::payload_str(&atom.payload, "style_id") {
                if state.styles.contains_key(style_id) {
                    return Err(YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!("风格 {style_id} 已存在")),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
        }
        if let Some(layer_id) = &primary_layer {
            match layer_ref_mode(atom.kind) {
                RefMode::MustExist => {
                    check_layer_exists(state, &atom.id, layer_id, atom.kind)?;
                }
                RefMode::MustNotExist => {
                    // 与 `fold.rs` 的 CreateLayer 保持**同一条规则** ✓：墓碑 id 可复用 ✓
                    //（否则这里先拒 ✓、那里后拒 ✓，两条路径又会长出不一致的提示 ✓）。
                    if state.layer_alive(layer_id) {
                        return Err(YanshiError::new(
                            ErrorCode::PreconditionFailed,
                            ErrorContext::detail(format!("图层 {layer_id} 已存在")),
                        )
                        .with_atom(atom.id.clone())
                        .with_layer(layer_id.clone()));
                    }
                }
                RefMode::Ignore => {}
            }
        }
        Ok(())
    }

    /// 实例成环（设计 9.3 ✓）：创建实例 ✓ 与改写 `master_ref` ✓ 两条入口都查 ✓。
    fn validate_instance_cycles(&self, atom: &Atom, state: &DocumentState) -> Result<()> {
        let master = if atom.kind == AtomKind::CreateObject
            && atom.payload.get("type").and_then(serde_json::Value::as_str) == Some("instance")
        {
            atom.payload
                .get("data")
                .and_then(|data| data.get("master_ref"))
                .and_then(|master_ref| master_ref.get("object_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        } else if atom.kind == AtomKind::SetProperty
            && atom.payload.get("key").and_then(serde_json::Value::as_str) == Some("master_ref")
        {
            atom.payload
                .get("value")
                .and_then(|value| value.get("object_id"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        } else {
            None
        };
        let (Some(master), Some(object_id)) = (
            master,
            atom.payload
                .get("object_id")
                .and_then(serde_json::Value::as_str),
        ) else {
            return Ok(());
        };
        crate::fold::check_instance_cycle(state, object_id, &master)
    }

    fn validate_secondary_refs(&self, atom: &Atom, state: &DocumentState) -> Result<()> {
        // 纯协作原子（不产生状态效果、不参与折叠，见设计 5.2 注）不校验二次引用：
        // `refs` 是**递归**收集的（供依赖追踪/CAS 使用），而协作原子会把 payload 里
        // 嵌套的内容（例如 suggest 的 patch 步骤）也收集进来。那些引用属于**将来要执行的
        // 步骤**，不属于该原子自身，按「必须已存在」校验会误拒「补丁内先建后用」这类用法。
        if matches!(
            atom.kind,
            AtomKind::Comment
                | AtomKind::Suggest
                | AtomKind::AcceptSuggestion
                | AtomKind::RejectSuggestion
        ) {
            return Ok(());
        }
        let primary_object = atom.object_id();
        for object_id in &atom.refs.objects {
            if primary_object == Some(object_id.as_str()) {
                continue;
            }
            check_object_exists(state, &atom.id, object_id, atom.kind)?;
        }
        let primary_layer = atom.layer_id();
        for layer_id in &atom.refs.layers {
            if primary_layer == Some(layer_id.as_str()) {
                continue;
            }
            if !state.layer_alive(layer_id) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("引用的图层 {layer_id} 不存在")),
                )
                .with_atom(atom.id.clone())
                .with_layer(layer_id.clone()));
            }
        }
        let primary_mask = if atom.kind == AtomKind::CreateMask {
            crate::atom::payload_str(&atom.payload, "mask_id")
        } else {
            None
        };
        let primary_selection = if atom.kind == AtomKind::CreateSelection {
            crate::atom::payload_str(&atom.payload, "selection_id")
        } else {
            None
        };
        let primary_style = if atom.kind == AtomKind::CreateStyle {
            crate::atom::payload_str(&atom.payload, "style_id")
        } else {
            None
        };
        for mask_id in &atom.refs.masks {
            if primary_mask == Some(mask_id.as_str()) {
                continue;
            }
            if !state.masks.get(mask_id).is_some_and(|m| !m.is_deleted()) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("引用的蒙版 {mask_id} 不存在")),
                )
                .with_atom(atom.id.clone()));
            }
        }
        for selection_id in &atom.refs.selections {
            if primary_selection == Some(selection_id.as_str()) {
                continue;
            }
            if !state
                .selections
                .get(selection_id)
                .is_some_and(|s| !s.is_deleted())
            {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("引用的选区 {selection_id} 不存在")),
                )
                .with_atom(atom.id.clone()));
            }
        }
        for style_id in &atom.refs.styles {
            if primary_style == Some(style_id.as_str()) {
                continue;
            }
            if !state.styles.get(style_id).is_some_and(|s| !s.is_deleted()) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("引用的风格 {style_id} 不存在")),
                )
                .with_atom(atom.id.clone()));
            }
        }
        Ok(())
    }

    fn validate_history_refs(&self, atom: &Atom, commit: &CommitContext<'_>) -> Result<()> {
        match atom.kind {
            AtomKind::Revert | AtomKind::Reapply => {
                let Some(target_id) = atom.target_atom() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("{} 缺少 target", atom.kind)),
                    )
                    .with_atom(atom.id.clone()));
                };
                let Some(target) = self.get(target_id) else {
                    return Err(YanshiError::new(
                        ErrorCode::ReferenceNotFound,
                        ErrorContext::detail(format!("目标原子 {target_id} 不在日志中")),
                    )
                    .with_atom(atom.id.clone()));
                };
                // **可以撤销"撤销"本身** ✓（设计 793：`revert(revert(x)) ≡ reapply(x)` ✓）。
                //
                // 原先这里只允许"有状态效果的原子" ✓ ⇒ `Revert`/`Reapply` 不是 ✓ ⇒ **被拒** ✗，
                // 于是设计那条恒等式**在提交层根本走不通** ✗（实测两句错误：
                // `不能 revert 协作/历史原子 …（revert）` ✓ 与 `不能 reapply …（revert）` ✓），
                // 而**折叠层却已经实现了**它 ✓（动作表按 target 取最新动作 ✓）—— 两层不一致 ✓。
                //
                // **用户已拍板：放开校验** ✓（2026-* 的裁决 ④ ✓）。放开范围刻意**只到历史原子** ✓：
                // `Revert`/`Reapply` 允许 ✓；**协作原子**（评论/建议/标注 …）**仍然拒绝** ✗ ——
                // 它们既不产生状态效果 ✓、也不是历史动作 ✓，撤销它们没有语义 ✓。
                let revertible = target.kind.is_state_effect()
                    || matches!(target.kind, AtomKind::Revert | AtomKind::Reapply);
                if !revertible {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "不能 {} 协作原子 {}（{}）—— 只有产生状态效果的原子与 revert/reapply 本身可以撤销",
                            atom.kind, target.id, target.kind
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
                // 跨 declare_head 的撤销无法仅凭 base_state(H_n) 表达（见 seq 模块约定）。
                if target.seq <= commit.state.eval_origin_seq() {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "目标原子 {} (seq {}) 位于当前求值起点 seq {} 之前，需先回到该起点或重新提交等价操作",
                            target.id,
                            target.seq,
                            commit.state.eval_origin_seq()
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
                if atom.kind == AtomKind::Revert
                    && target.actor != commit.actor
                    && !commit.allow_cross_actor_revert
                {
                    return Err(YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail(format!(
                            "跨 actor revert 需要 owner 权限：目标是 {} 的原子",
                            target.actor
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
            AtomKind::DeclareHead => {
                self.validate_declare_head(atom, commit.state)?;
            }
            AtomKind::CreateCheckpoint | AtomKind::Checkpoint => {
                if let Some(anchor) = crate::atom::payload_u64(&atom.payload, "anchor_seq") {
                    if anchor > self.head_seq() {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "检查点锚定 seq {anchor} 超过当前 head {}",
                                self.head_seq()
                            )),
                        )
                        .with_atom(atom.id.clone()));
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// `declare_head` 的 base 必须指向**已存在且不晚于当前 head** 的原子或检查点，
    /// 否则 `state@seq` 公式的递归定义无法终止（设计文档 5.5）。
    fn validate_declare_head(&self, atom: &Atom, state: &DocumentState) -> Result<()> {
        let base = match crate::fold::parse_head_base(&atom.payload) {
            Ok(base) => base,
            Err(error) => return Err(error.with_atom(atom.id.clone())),
        };
        match base {
            HeadBase::Atom(atom_id) => {
                let Some(target) = self.get(&atom_id) else {
                    return Err(YanshiError::new(
                        ErrorCode::ReferenceNotFound,
                        ErrorContext::detail(format!(
                            "declare_head 的 base 原子 {atom_id} 不在日志中"
                        )),
                    )
                    .with_atom(atom.id.clone()));
                };
                if target.seq > self.head_seq() {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "declare_head 的 base 原子 seq {} 晚于当前 head {}",
                            target.seq,
                            self.head_seq()
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
            HeadBase::Checkpoint(checkpoint_id) => {
                // 检查点从日志解析（设计文档 4.5：轻量元数据原子），
                // 因此即使它不在当前折叠窗口内也能作为求值起点。
                let anchor =
                    crate::seq::checkpoint_anchor_seq(self, &checkpoint_id, self.head_seq())
                        .or_else(|| {
                            state
                                .checkpoints
                                .get(&checkpoint_id)
                                .map(|checkpoint| checkpoint.anchor_seq)
                        })
                        .ok_or_else(|| {
                            YanshiError::new(
                                ErrorCode::ReferenceNotFound,
                                ErrorContext::detail(format!("检查点 {checkpoint_id} 不存在")),
                            )
                            .with_atom(atom.id.clone())
                        })?;
                if anchor > self.head_seq() {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "检查点锚定 seq {anchor} 晚于当前 head {}",
                            self.head_seq()
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
            }
        }
        Ok(())
    }

    fn validate_conflict(&self, atom: &Atom) -> Result<()> {
        if !is_sampling_replace(atom) {
            return Ok(());
        }
        let candidates: Vec<&Atom> = self.concurrent_window(atom).collect();
        if let Some(info) = find_conflict(&candidates, atom) {
            return Err(info.to_error());
        }
        Ok(())
    }

    /// 并发窗口：`parents` 所指向 head 之后的原子（离线重连时 parents 可能落后）。
    ///
    /// 窗口按设计文档 6.2 的重放上限截断为最近 [`CONFLICT_WINDOW_ATOMS`] 个原子：
    /// 超出窗口的并发互踩不再由服务端自动判定，交由 `resolve_conflict` 人工处理。
    fn concurrent_window<'a>(&'a self, atom: &'a Atom) -> impl Iterator<Item = &'a Atom> + 'a {
        let parents_head = atom
            .parents
            .iter()
            .filter_map(|parent| self.seq_of(parent))
            .max()
            .unwrap_or(0);
        let base = parents_head.max(self.head_seq().saturating_sub(CONFLICT_WINDOW_ATOMS));
        self.atoms
            .iter()
            .filter(move |existing| existing.seq > base && existing.id != atom.id)
    }

    /// 日志长度。
    pub fn len(&self) -> usize {
        self.atoms.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.atoms.is_empty()
    }

    /// 下一个待分配的 seq。
    pub fn next_seq(&self) -> Seq {
        self.next_seq
    }

    /// 当前 head seq（空日志为 0）。
    pub fn head_seq(&self) -> Seq {
        self.atoms.last().map(|a| a.seq).unwrap_or(0)
    }

    /// 当前 head 原子。
    pub fn head_atom(&self) -> Option<&Atom> {
        self.atoms.last()
    }

    /// 按 id 取原子。
    pub fn get(&self, id: &str) -> Option<&Atom> {
        self.by_id.get(id).map(|index| &self.atoms[*index])
    }

    /// 按 id 取权威 seq。
    pub fn seq_of(&self, id: &str) -> Option<Seq> {
        self.by_id.get(id).map(|index| self.atoms[*index].seq)
    }

    /// 按 seq 取原子。
    ///
    /// 二分查找 O(log n)：seq 连续时等价于下标 -1，但 [`AtomLog::with_atoms`]
    /// 允许从带空洞的日志恢复，因此这里不做下标假设。
    pub fn by_seq(&self, seq: Seq) -> Option<&Atom> {
        if seq == 0 {
            return None;
        }
        let index = self.atoms.partition_point(|atom| atom.seq < seq);
        self.atoms.get(index).filter(|atom| atom.seq == seq)
    }

    /// 全部原子（seq 升序）。
    pub fn atoms(&self) -> &[Atom] {
        &self.atoms
    }

    /// 迭代器（seq 升序）。
    pub fn iter(&self) -> std::slice::Iter<'_, Atom> {
        self.atoms.iter()
    }

    /// `(after, upto]` 区间：`A_n := H_n.seq < seq ≤ n`（设计文档 5.5）。
    pub fn range_exclusive_inclusive(&self, after: Seq, upto: Seq) -> &[Atom] {
        let start = self.atoms.partition_point(|a| a.seq <= after);
        let end = self.atoms.partition_point(|a| a.seq <= upto);
        &self.atoms[start..end]
    }

    /// `seq ≤ upto` 的全部原子。
    pub fn atoms_upto(&self, upto: Seq) -> &[Atom] {
        let end = self.atoms.partition_point(|a| a.seq <= upto);
        &self.atoms[..end]
    }

    /// `seq ≤ upto` 中最近一次 `declare_head` 原子（设计文档 5.5 的 `H_n`）。
    pub fn last_declare_head_upto(&self, upto: Seq) -> Option<&Atom> {
        self.atoms_upto(upto)
            .iter()
            .rev()
            .find(|a| a.kind == AtomKind::DeclareHead)
    }

    /// 全部 `declare_head` 原子。
    pub fn declare_heads(&self) -> impl Iterator<Item = &Atom> {
        self.atoms
            .iter()
            .filter(|a| a.kind == AtomKind::DeclareHead)
    }

    /// 指定类型的全部原子。
    pub fn atoms_of_kind(&self, kind: AtomKind) -> impl Iterator<Item = &Atom> {
        self.atoms.iter().filter(move |a| a.kind == kind)
    }

    /// 某变更集的原子。
    pub fn changeset_atoms(&self, changeset_id: &str) -> Vec<&Atom> {
        self.atoms
            .iter()
            .filter(|a| a.changeset_id.as_deref() == Some(changeset_id))
            .collect()
    }

    /// **GC 根集的 blob 部分 = 全日志原子引用闭包**（设计文档 6.3 / 原则 21）。
    ///
    /// 只要某个 blob 被日志中任意原子引用过，它就是历史资产，永不删除。
    pub fn blob_roots(&self) -> BTreeSet<BlobHash> {
        let mut roots = BTreeSet::new();
        for atom in &self.atoms {
            roots.extend(atom.all_blob_refs());
        }
        roots
    }

    /// 引用了指定 blob 的原子（可观测性与排障用）。
    pub fn atoms_referencing_blob(&self, hash: &BlobHash) -> Vec<&Atom> {
        self.atoms
            .iter()
            .filter(|a| a.all_blob_refs().contains(hash))
            .collect()
    }
}

fn check_object_exists(
    state: &DocumentState,
    atom_id: &str,
    object_id: &str,
    kind: AtomKind,
) -> Result<()> {
    match state.objects.get(object_id) {
        None => Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 在当前 HEAD 中不存在")),
        )
        .with_atom(atom_id.to_owned())
        .with_object(object_id.to_owned())),
        Some(object) if object.is_deleted() => Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("对象 {object_id} 已被 tombstone，{kind} 被拒绝")),
        )
        .with_atom(atom_id.to_owned())
        .with_object(object_id.to_owned())),
        Some(_) => Ok(()),
    }
}

fn check_layer_exists(
    state: &DocumentState,
    atom_id: &str,
    layer_id: &str,
    kind: AtomKind,
) -> Result<()> {
    match state.layers.get(layer_id) {
        None => Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id} 在当前 HEAD 中不存在")),
        )
        .with_atom(atom_id.to_owned())
        .with_layer(layer_id.to_owned())),
        Some(layer) if layer.is_deleted() => Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("图层 {layer_id} 已删除，{kind} 被拒绝")),
        )
        .with_atom(atom_id.to_owned())
        .with_layer(layer_id.to_owned())),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_doc() -> Atom {
        Atom::new(
            AtomKind::CreateDocument,
            "human:1",
            "session:a",
            json!({"doc_id": "doc_1", "width": 64, "height": 64}),
        )
        .with_id("atom_doc")
    }

    fn create_layer() -> Atom {
        Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1", "name": "L1"}),
        )
        .with_id("atom_layer")
    }

    fn create_object() -> Atom {
        Atom::new(
            AtomKind::CreateObject,
            "human:1",
            "session:a",
            json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
        )
        .with_id("atom_obj")
    }

    fn log_with_head() -> AtomLog {
        let mut log = AtomLog::new();
        log.append(create_doc()).unwrap();
        log.append(create_layer()).unwrap();
        log.append(create_object()).unwrap();
        log
    }

    fn state_at_head(log: &AtomLog) -> DocumentState {
        crate::fold::fold_atoms(DocumentState::empty(), log.atoms()).state
    }

    #[test]
    fn seq_is_assigned_in_receive_order() {
        let mut log = AtomLog::new();
        assert_eq!(log.head_seq(), 0);
        assert_eq!(log.append(create_doc()).unwrap().seq(), 1);
        assert_eq!(log.append(create_layer()).unwrap().seq(), 2);
        assert_eq!(log.head_seq(), 2);
        assert_eq!(log.head_atom().unwrap().id, "atom_layer");
        assert_eq!(log.seq_of("atom_doc"), Some(1));
        assert_eq!(log.by_seq(2).unwrap().id, "atom_layer");
    }

    #[test]
    fn duplicate_id_is_idempotent() {
        let mut log = AtomLog::new();
        let first = log.append(create_doc()).unwrap();
        assert!(first.is_appended());
        let mut replay = create_doc();
        replay.actor = "human:2".to_owned();
        let second = log.append(replay).unwrap();
        assert_eq!(second, AppendOutcome::Duplicate { seq: first.seq() });
        assert_eq!(log.len(), 1);
        assert_eq!(
            log.get("atom_doc").unwrap().actor,
            "human:1",
            "先写入者胜出，重试不覆盖"
        );
    }

    #[test]
    fn range_and_head_queries() {
        let log = log_with_head();
        assert_eq!(log.range_exclusive_inclusive(0, 2).len(), 2);
        assert_eq!(log.range_exclusive_inclusive(1, 3).len(), 2);
        assert_eq!(log.range_exclusive_inclusive(3, 3).len(), 0);
        assert_eq!(log.atoms_upto(1).len(), 1);
        assert!(log.last_declare_head_upto(3).is_none());
        assert_eq!(log.changeset_atoms("cs_x").len(), 0);
    }

    #[test]
    fn commit_validation_rejects_missing_blob() {
        let mut log = log_with_head();
        let state = state_at_head(&log);
        let blob = BlobHash::from_bytes(b"payload");
        let atom = Atom::new(
            AtomKind::CreateObject,
            "human:1",
            "session:a",
            json!({
                "object_id": "obj_blob",
                "layer_id": "layer_1",
                "type": "raster_patch",
                "bitmap": {"blob_hash": blob, "size": 7, "mime_type": "image/webp"},
            }),
        );
        let none = |_: &BlobHash| false;
        let ctx = CommitContext::new(&state, &none, "human:1", "session:a");
        let err = log.append_validated(atom.clone(), &ctx).unwrap_err();
        assert_eq!(err.code, ErrorCode::ReferenceNotFound);
        assert_eq!(err.context.blob_hash.as_deref(), Some(blob.as_str()));
        assert_eq!(log.len(), 3, "校验失败不进日志");

        // blob 先行后，同一原子可提交。
        let yes = |_: &BlobHash| true;
        let ctx = CommitContext::new(&state, &yes, "human:1", "session:a");
        assert!(log.append_validated(atom, &ctx).unwrap().is_appended());
    }

    #[test]
    fn commit_validation_rejects_dangling_and_tombstoned_refs() {
        let mut log = log_with_head();
        let state = state_at_head(&log);
        let none = |_: &BlobHash| false;
        let ctx = CommitContext::new(&state, &none, "human:1", "session:a");

        let dangling = Atom::new(
            AtomKind::SetProperty,
            "human:1",
            "session:a",
            json!({"object_id": "obj_missing", "key": "visible", "value": false}),
        );
        assert_eq!(
            log.append_validated(dangling, &ctx).unwrap_err().code,
            ErrorCode::ReferenceNotFound
        );

        let mut deleted_state = state.clone();
        deleted_state.objects.get_mut("obj_1").unwrap().deleted_by = Some("atom_del".into());
        let ctx = CommitContext::new(&deleted_state, &none, "human:1", "session:a");
        let touch_deleted = Atom::new(
            AtomKind::SetProperty,
            "human:1",
            "session:a",
            json!({"object_id": "obj_1", "key": "visible", "value": false}),
        );
        assert_eq!(
            log.append_validated(touch_deleted, &ctx).unwrap_err().code,
            ErrorCode::PreconditionFailed
        );
    }

    #[test]
    fn commit_validation_rejects_duplicate_creation() {
        let mut log = log_with_head();
        let state = state_at_head(&log);
        let none = |_: &BlobHash| false;
        let ctx = CommitContext::new(&state, &none, "human:1", "session:a");
        let again = Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1"}),
        );
        assert_eq!(
            log.append_validated(again, &ctx).unwrap_err().code,
            ErrorCode::PreconditionFailed
        );
    }

    #[test]
    fn revert_target_must_be_state_effect_and_permission_checked() {
        let mut log = log_with_head();
        let state = state_at_head(&log);
        let none = |_: &BlobHash| false;

        // 不能 revert 协作原子。
        let comment = Atom::new(
            AtomKind::Comment,
            "human:1",
            "session:a",
            json!({"target_atom": "atom_obj", "text": "nice"}),
        );
        let comment_id = comment.id.clone();
        log.append(comment).unwrap();
        let ctx = CommitContext::new(&state, &none, "human:1", "session:a");
        let revert_comment = Atom::new(
            AtomKind::Revert,
            "human:1",
            "session:a",
            json!({"target": comment_id}),
        );
        assert_eq!(
            log.append_validated(revert_comment, &ctx).unwrap_err().code,
            ErrorCode::InvalidArgument
        );

        // 跨 actor revert 默认拒绝，owner 权限放行。
        let cross = Atom::new(
            AtomKind::Revert,
            "human:2",
            "session:b",
            json!({"target": "atom_obj"}),
        );
        let cross_ctx = CommitContext::new(&state, &none, "human:2", "session:b");
        assert_eq!(
            log.append_validated(cross.clone(), &cross_ctx)
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );
        let owner_ctx = cross_ctx.allow_cross_actor_revert(true);
        assert!(log
            .append_validated(cross, &owner_ctx)
            .unwrap()
            .is_appended());
    }

    #[test]
    fn gc_root_set_is_full_log_reference_closure() {
        let mut log = AtomLog::new();
        let blob_a = BlobHash::from_bytes(b"a");
        let blob_b = BlobHash::from_bytes(b"b");
        log.append(create_doc()).unwrap();
        log.append(
            Atom::new(
                AtomKind::CreateObject,
                "ai:1",
                "session:ai",
                json!({"object_id": "o1", "layer_id": "l1", "bitmap": {"blob_hash": blob_a}}),
            )
            .with_id("atom_blob_a"),
        )
        .unwrap();
        // 该原子随后被 revert，但 blob 仍是历史资产。
        log.append(
            Atom::new(
                AtomKind::Revert,
                "human:1",
                "session:a",
                json!({"target": "atom_blob_a"}),
            )
            .with_id("atom_revert"),
        )
        .unwrap();

        let roots = log.blob_roots();
        assert!(roots.contains(&blob_a));
        assert!(!roots.contains(&blob_b));
        assert_eq!(log.atoms_referencing_blob(&blob_a).len(), 1);
    }

    #[test]
    fn by_seq_handles_gaps_and_misses() {
        let mut log = AtomLog::new();
        log.append(create_doc()).unwrap();
        log.append(create_layer()).unwrap();
        log.append(create_object()).unwrap();
        assert_eq!(log.by_seq(0), None);
        assert_eq!(log.by_seq(1).map(|a| a.id.as_str()), Some("atom_doc"));
        assert_eq!(log.by_seq(3).map(|a| a.id.as_str()), Some("atom_obj"));
        assert_eq!(log.by_seq(4), None);
        assert_eq!(log.by_seq(99), None);

        // 带空洞的日志（恢复路径）同样正确。
        let mut sparse = log.atoms().to_vec();
        sparse[2].seq = 10;
        let sparse = AtomLog::with_atoms(sparse).unwrap();
        assert_eq!(sparse.by_seq(2).map(|a| a.id.as_str()), Some("atom_layer"));
        assert_eq!(sparse.by_seq(3), None);
        assert_eq!(sparse.by_seq(10).map(|a| a.id.as_str()), Some("atom_obj"));
    }

    #[test]
    fn restore_from_atoms_requires_unique_seq() {
        let log = log_with_head();
        let restored = AtomLog::with_atoms(log.atoms().to_vec()).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(restored.next_seq(), 4);
        assert_eq!(restored.head_seq(), 3);

        let mut duplicated = log.atoms().to_vec();
        duplicated.push(create_doc());
        assert!(AtomLog::with_atoms(duplicated).is_err());
    }

    #[cfg(test)]
    mod collab_ref_tests {
        use super::*;
        use crate::atom::{Atom, AtomKind};
        use serde_json::json;

        /// 协作原子 payload 里嵌套的 patch 步骤引用了尚未存在的实体时，
        /// **不得**把它当成该原子自身的引用而拒绝（否则「补丁内先建层再画」根本无法记录）。
        #[test]
        fn nested_patch_refs_do_not_invalidate_a_suggestion() {
            let atom = Atom::new(
                AtomKind::Suggest,
                "human:1",
                "session:web",
                json!({
                    "patch": [
                        {"tool": "create_layer", "arguments": {"layer_id": "layer_new"}},
                        {"tool": "draw_shape", "arguments": {
                            "layer_id": "layer_new",
                            "data": {"geometry": {"kind": "rect",
                                    "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}},
                                    "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}}
                    ]
                }),
            );
            // refs 仍是**递归**收集的（依赖追踪需要），但二次引用校验对协作原子豁免。
            assert!(
                atom.refs.layers.iter().any(|id| id == "layer_new"),
                "递归收集应保留嵌套引用：{:?}",
                atom.refs.layers
            );
            let state = DocumentState::empty();
            let log = AtomLog::new();
            assert!(
                log.validate_secondary_refs(&atom, &state).is_ok(),
                "协作原子不应因嵌套引用被拒"
            );
        }
    }
}
