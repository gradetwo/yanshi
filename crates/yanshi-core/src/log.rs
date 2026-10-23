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
    /// **★ 从"某个起点之后"的原子建日志（待办 ① 的"快照 ＋ 尾巴"用 ✓，2026-10-15 ✓）★**
    ///
    /// **为什么需要它** ✗：`with_atoms` 会把 `next_seq` 算成"**最后一个原子 + 1**" ✓
    /// （`log.rs:193` 的 `log.next_seq.max(atom.seq + 1)` ✓）——
    /// **尾巴为空**时它停在 **1** ✗ ⇒ 下一次提交会**重用 `seq`** ✗
    /// ⇒ 日志里出现**两个同 seq 的行** ✗ ⇒ 之后按 seq 的查找／折叠全错 ✗。
    /// **⇒ 本构造器把 `next_seq` 抬到 `base_seq + 1`** ✓（`base_seq = 0` 时与 `with_atoms` 等价 ✓）。
    ///
    /// **注意** ✓：`head_seq()` **不**知道 `base_seq` ✗（空尾巴时仍返回 0 ✓）——
    /// 那是**已知瑕疵** ✗：修它要动 **128 个调用方** ✗ ⇒ 留作**单独一笔** ✓
    /// （见 `docs/design/vermeer-acceptance.md` §49 之补十四 ✓）。
    pub fn with_base_seq(base_seq: Seq, atoms: Vec<Atom>) -> Result<Self> {
        let mut log = Self::with_atoms(atoms)?;
        log.next_seq = log.next_seq.max(base_seq + 1);
        Ok(log)
    }

    /// **从一批原子建日志** ✓（会按 `(seq, id)` 排序 ✓、查重复 ✓、并把 `next_seq` 推到"末个 + 1" ✓）。
    ///
    /// **为什么补这条注释** ✗：我插入 `with_base_seq` 时**只锚在函数签名那一行** ✓
    /// ⇒ 把它**原有的文档注释"吃"进了我的新块** ✗ ⇒ `clippy -D warnings` 直接报
    /// `missing documentation for an associated function` ✓ —— **AGENTS.md 那条坑的第 8 次** ✗✓
    /// （规矩：新增项必须插在**整段 `///` 之前** ✓，不能插在"文档与它描述的项之间" ✗）。
    pub fn with_atoms(mut atoms: Vec<Atom>) -> Result<Self> {
        // 按 (seq, id) 排序，但**不在 key 里 clone id** ✓ —— 原先 `(a.seq, a.id.clone())`
        // 会在每次比较时分配一个字符串 ✗（n log n 次）⇒ 大日志上纯属浪费 ✓。
        atoms.sort_by(|a, b| a.seq.cmp(&b.seq).then_with(|| a.id.cmp(&b.id)));
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
            // 已按 seq 升序 ✓ ⇒ 重复 seq **必然相邻** ✗ ⇒ 只看上一个即可 ✓
            //（原先是 `log.atoms.iter().any(...)` ✗ ⇒ O(n²)：1050 原子 103 ms ✓，
            //  而大文档的原子数会高一个量级 ✓）。
            if log
                .atoms
                .last()
                .is_some_and(|existing| existing.seq == atom.seq)
            {
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

    /// **并入"别的写者"已经落盘的原子**（第 754 轮；台账三百三十的下一步）
    ///
    /// **⇒ 为什么需要**：第 752 轮只把 `next_seq` 推上去 ⇒ **不再撞号** ✓，但另一个进程写的原子
    /// **不进本进程的内存日志** ✗ ⇒ 要看到它们得**重新打开文档** ✓（`get_log` 看不到别人的一笔 ✓）。
    /// **⇒ 做法**：保留它们**自己的 `seq`** ✓（不重新分配 ✗）、按 `seq` 插到正确位置 ✓、
    /// 并把 `next_seq` 推到 `max + 1` ✓；`id` 已存在则**跳过**（幂等 ✓）。
    /// **⇒ 判据**：`two_writers_seqs` 之后再加一条 —— 本进程的 `get_log` **必须看得到**那一笔 ✓。
    pub fn adopt(&mut self, extra: Vec<Atom>) {
        for atom in extra {
            if self.by_id.contains_key(&atom.id) {
                continue;
            }
            let index = self
                .atoms
                .partition_point(|existing| existing.seq <= atom.seq);
            self.next_seq = self.next_seq.max(atom.seq + 1);
            let inserted = atom.id.clone();
            self.by_id.insert(inserted.clone(), index);
            self.atoms.insert(index, atom);
            // **插在中间 ⇒ 从 `index` 起的那些下标都要 +1** ✓（`by_id` 是下标表 ✓）
            //   **⇒ `>= index` 而不是 `> index`** ✓：被挤走的第一个元素**正好在** `index` 上 ✓
            //     （第 756 轮判据抓到的实锤：用 `>` 时 `atom_layer` 停在下标 1 ⇒ 指向了别人 ✓）。
            //   **⇒ 刚插入的那一条要跳过** ✓（它自己的下标就是 `index` ✓）。
            for (key, value) in self.by_id.iter_mut() {
                if key != &inserted && *value >= index {
                    *value += 1;
                }
            }
        }
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
        self.validate_object_type(atom)?;
        self.validate_property_values(atom)?;
        self.validate_document_size(atom)?;
        self.validate_lock(atom, commit.state)?;
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
                // **`mode != "new"` 时允许已存在**（外部 `E1027`，第 773 轮）：`add`/`subtract`/
                // `intersect` 的语义就是「改一个已有的选区」⇒ 若这里一律拒绝，那三个值永远不可达。
                // 注意 `atom.payload` 里的 `mode` 是 `tools.rs` 写进去的，缺省即 `new`。
                let mode = crate::atom::payload_str(&atom.payload, "mode").unwrap_or("new");
                if mode == "new" && state.selections.contains_key(selection_id) {
                    return Err(YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!(
                            "选区 {selection_id} 已存在（要改它请传 mode=add/subtract/intersect）"
                        )),
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
            if state.masks.get(mask_id).is_none_or(|m| m.is_deleted()) {
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
            if state
                .selections
                .get(selection_id)
                .is_none_or(|s| s.is_deleted())
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
            if state.styles.get(style_id).is_none_or(|s| s.is_deleted()) {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("引用的风格 {style_id} 不存在")),
                )
                .with_atom(atom.id.clone()));
            }
        }
        Ok(())
    }

    /// **未知的对象类型必须被拒绝** ✓ —— 否则会被静默兜底成笔迹 ✗。
    ///
    /// 这条规则来自第 44 轮的实测 ✓：`parse_object_type` 的兜底是 `Stroke` ✓
    ///（对**没写 `type`** 的旧调用是合理的 ✓），但 `"path"` 这样**写了却没人认识**的类型
    /// 也会掉进去 ✓ ⇒ 对象建出来了、命令返回 ok ✓、渲染却是空的 ✗ ——
    /// 两个看起来无关的失败（"逐像素差了 3042 字节" ✓ 与"没有 points" ✓）其实是同一个兜底造成的 ✓。
    ///
    /// **为什么放在这一层** ✓：本项目反复吃过"守卫写错层"的亏 ✓ ——
    /// `fold::precondition` 返回 `Err` 只会**警告并跳过** ✓，只有这里的 `Err` 才**拒绝提交** ✓。
    fn validate_object_type(&self, atom: &Atom) -> Result<()> {
        if !matches!(atom.kind, AtomKind::CreateObject | AtomKind::ImportImage) {
            return Ok(());
        }
        let Some(declared) = atom.payload.get("type").and_then(serde_json::Value::as_str) else {
            // 没写 `type` ⇒ 交给兜底 ✓（旧调用与 `DrawStroke` 一类不写类型的路径 ✓）。
            return Ok(());
        };
        let known = [
            "stroke",
            "shape",
            "text",
            "adjustment",
            "filter",
            "raster_patch",
            "retouch",
            "liquify",
            "instance",
            "group",
            "path",
        ];
        if known.contains(&declared) {
            return Ok(());
        }
        Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "未知的对象类型 {declared}（已知：{}）—— 写错类型会被静默当成笔迹，所以这里直接拒绝",
                known.join(", ")
            )),
        )
        .with_atom(atom.id.clone()))
    }

    /// **`set_property` 的值类型校验** ✓（第 1440 轮审计）。
    ///
    /// 原来非法值在折叠层**静默取默认**：`"visible": "yes"` ⇒ `unwrap_or(true)`
    /// 把对象**画成显示** ✗；`type: 未知串` ⇒ 静默换成 `Stroke` ✗ ——
    /// 正是第 44 轮在 `CreateObject` 路上消灭的那类静默兜底，
    /// 但 `SetProperty` 这条路**漏网** ✓（`validate_object_type` 只覆盖
    /// `CreateObject|ImportImage` ✓）。
    ///
    /// **放在提交层** ✓（与 `validate_object_type` 同理由：`fold::precondition`
    /// 的 `Err` 只警告并跳过 ✓，只有这里的 `Err` 才**拒绝提交** ✓）。
    fn validate_property_values(&self, atom: &Atom) -> Result<()> {
        if atom.kind != AtomKind::SetProperty {
            return Ok(());
        }
        let Some(key) = atom.payload.get("key").and_then(serde_json::Value::as_str) else {
            return Ok(());
        };
        let value = atom
            .payload
            .get("value")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let reject = |expect: &str| -> Result<()> {
            Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "set_property(key={key}) 的值不合法：需要{expect}，收到 {value} —— \
                     拒绝静默保持旧值或取默认（第 1440 轮）"
                )),
            )
            .with_atom(atom.id.clone()))
        };
        match key {
            "visible" | "locked" | "alpha_lock" | "clipping_mask" => {
                if !value.is_boolean() {
                    return reject("布尔");
                }
            }
            "z_index" => {
                if value.as_i64().is_none() {
                    return reject("整数");
                }
            }
            "opacity" => {
                if !value.is_number() {
                    return reject("数值");
                }
            }
            "type" => {
                // 分发口径与折叠层一致：有 `object_id` ⇒ 对象，否则 `layer_id` ⇒ 图层 ✓。
                let object_side = atom.payload.get("object_id").is_some();
                let known: &[&str] = if object_side {
                    &[
                        "stroke",
                        "shape",
                        "text",
                        "adjustment",
                        "filter",
                        "raster_patch",
                        "retouch",
                        "liquify",
                        "instance",
                        "group",
                        "path",
                    ]
                } else {
                    &["raster", "vector", "layer_group", "mask"]
                };
                let declared = value.as_str();
                if !declared.is_some_and(|d| known.contains(&d)) {
                    return reject(if object_side {
                        "已知对象类型字符串"
                    } else {
                        "已知图层类型字符串"
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// **`create_document` 的宽高必须是有界整数** ✓（第 1440 轮审计）。
    ///
    /// 折叠层原来是 `payload_u64(...).unwrap_or(0) as u32`：缺省 ⇒ **0 尺寸** ✗、
    /// `2^32+N` ⇒ **截断成 N** ✗，全程无信号 ✓。写进日志前在这里拒绝；
    /// **没写**宽高的旧调用保持兼容 ✓（由上层默认处理）。
    fn validate_document_size(&self, atom: &Atom) -> Result<()> {
        if atom.kind != AtomKind::CreateDocument {
            return Ok(());
        }
        for key in ["width", "height"] {
            let Some(value) = atom.payload.get(key) else {
                continue;
            };
            // 整数（u64）或**整数值的浮点**都接受；其余拒绝 ✓。
            let whole = value
                .as_u64()
                .or_else(|| {
                    value.as_f64().and_then(|f| {
                        if f.fract() == 0.0 && f > 0.0 && f <= u64::from(u32::MAX) as f64 {
                            Some(f as u64)
                        } else {
                            None
                        }
                    })
                })
                .filter(|n| *n > 0 && *n <= 65535);
            if whole.is_none() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "create_document 的 {key} 必须是 1..=65535 的整数，收到 {value} —— \
                        非法值若被静默取 0 或按 u32 截断，会得到没有信号的空文档（第 1440 轮）"
                    )),
                )
                .with_atom(atom.id.clone()));
            }
        }
        Ok(())
    }

    /// **锁定必须被强制** ✓ —— 否则"锁定图层"只是个**点了没用的标签** ✗。
    ///
    /// 开工前我查过：`locked` 只被**记录与回报** ✓（`state`、`list_layers`、`set_property` ✓），
    /// 内核对它**没有任何检查** ✗ —— 也就是说锁了照样能画、能删 ✗。
    /// 这正是本项目一路在消灭的那类"**静默失效**" ✓（看起来做完了、其实什么都没发生 ✗）。
    ///
    /// **放在这一层** ✓：本项目反复吃过"守卫写错层"的亏 ✓ ——
    /// `fold::precondition` 返回 `Err` 只**警告并跳过** ✓，只有 `validate_commit` 的 `Err` 才**拒绝提交** ✓。
    ///
    /// **规则（尽量少而清楚 ✓）**：
    /// * 原子若指向一个**已存在的对象** ✓，而该对象**自己**或**它所在图层**被锁定 ✓ ⇒ 拒绝 ✓；
    /// * `CreateObject` 若指向一个**被锁定的图层** ✓ ⇒ 拒绝 ✓（锁住的图层不该长出新东西 ✓）；
    /// * **例外：管理性属性** ✓ —— `set_property` 改 `locked` / `visible` 仍然允许 ✓，
    ///   否则"解锁"永远做不到 ✓（这是**必须**留的出口 ✓）；
    /// * **图层自身**的原子（改名 / 可见性 / 不透明度 / 顺序 ✓）不受图层锁影响 ✓
    ///   —— 与常见图像软件一致 ✓（锁的是**内容** ✓，不是这个图层对象的管理属性 ✓）。
    ///
    /// **边界如实说明** ✓：`alpha_lock` / `clipping_mask` / `mask_id` 目前**只存不生效** ✓
    ///（它们需要像素级的语义 ✓，不是一条校验能覆盖的 ✓）—— 这是**已知缺口** ✓，
    /// 需要时单独讨论并记录 ✓，本片不擅自发明 ✗。
    fn validate_lock(&self, atom: &Atom, state: &DocumentState) -> Result<()> {
        // **管理性属性**：允许在被锁对象上改"锁"与"可见性" ✓（解锁的唯一出路 ✓）。
        if atom.kind == AtomKind::SetProperty {
            let key = atom.payload.get("key").and_then(serde_json::Value::as_str);
            if matches!(key, Some("locked") | Some("visible")) {
                return Ok(());
            }
        }
        let targets_object = atom
            .payload
            .get("object_id")
            .and_then(serde_json::Value::as_str);
        if let Some(object_id) = targets_object {
            // `Tombstone` 的对象可能仍在态里（墓碑 ✓）⇒ 只对**活着的**对象判锁 ✓。
            if let Some(object) = state.objects.get(object_id) {
                let layer_locked = state
                    .layers
                    .get(object.layer_id.as_str())
                    .map(|layer| layer.locked)
                    .unwrap_or(false);
                let blocked_by = if object.locked {
                    "对象自身已锁定"
                } else if layer_locked {
                    "所在图层已锁定"
                } else {
                    return Ok(());
                };
                // `CreateObject` 用同一个 `object_id` 字段 ✓，但那时对象还不在态里 ✓ ⇒ 走下面的图层判定 ✓。
                return Err(YanshiError::new(
                    ErrorCode::PermissionDenied,
                    ErrorContext::detail(format!(
                        "不能修改 {object_id}：{blocked_by}（先解锁再操作；改 locked/visible 不受限制）"
                    )),
                )
                .with_atom(atom.id.clone()));
            }
            // 对象**还不存在** ⇒ 说明这条原子要**新建**对象 ✓ ⇒ 检查它的目标图层是否被锁 ✓。
            //
            // **这里我第一版写错了** ✗：我限定 `atom.kind == CreateObject` ✓，
            // 而实际上 `draw_stroke` / `draw_shape` / `fill` 等**隐式新建**对象的原子
            // 种类**都不是** `CreateObject` ✗ ⇒ 测试立刻显示"落笔 ok:true" ✗
            //（锁着的图层照样能画 ✓ —— 校验形同虚设 ✓）。
            // **教训** ✓：判据要取**载荷的形状**（有 `object_id` + `layer_id` ✓），
            // 而不是去枚举"我猜会是哪些原子种类" ✗ —— 后者永远会漏 ✓。
            if let Some(layer_id) = atom
                .payload
                .get("layer_id")
                .and_then(serde_json::Value::as_str)
            {
                let locked = state
                    .layers
                    .get(layer_id)
                    .map(|layer| layer.locked)
                    .unwrap_or(false);
                if locked {
                    return Err(YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail(format!(
                            "不能在锁定的图层 {layer_id} 上新建对象（先解锁）"
                        )),
                    )
                    .with_atom(atom.id.clone()));
                }
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

    /// **把 `next_seq` 至少推到 `seq`** ✓（第 752 轮；台账三百三十的**正确修法**）
    ///
    /// **⇒ 为什么要它** ✗：另一个进程（有它自己的计数器 ✓）可能已经把日志写到更后面 ✓
    /// ⇒ 本进程若照自己的 `next_seq` 分配 ⇒ **撞号** ✓ ⇒ 盘上出现重复 `seq` ✓
    /// （打开时只能靠 `repair_duplicate_seqs` 补救 ✓）。
    /// **⇒ 为什么不是"拒绝写"** ✗：第 749 轮试过 ✗ ⇒ 把**合法的两个写者**打回去了 ✓
    /// （`tool-brush-parity-jitter` 就是这样红的 ✓）⇒ **只把号推上去**，不动已有原子 ✓。
    pub fn bump_next_seq_at_least(&mut self, seq: Seq) {
        if self.next_seq <= seq {
            self.next_seq = seq + 1;
        }
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
        // **列出可用对象** ✓（用户 §五-18 ✓）—— 否则调用方只能再调一次列表接口 ✓ 或乱试 ✗。
        None => Err(
            crate::error::missing_reference("对象", object_id, state.objects.keys())
                .with_atom(atom_id.to_owned())
                .with_object(object_id.to_owned()),
        ),
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
        // **列出可用图层** ✓（用户举的例子正是图层 ✓）。
        None => Err(crate::error::missing_reference(
            "图层",
            layer_id,
            state.layers.keys().filter(|id| state.layer_alive(id)),
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

    /// **`adopt` 的语义**（第 756 轮）：保留外来 `seq` ✓、插到正确位置 ✓、同 `id` 幂等 ✓、
    /// `next_seq` 推到 `max + 1` ✓、`by_id` 下标**随插入平移** ✓。
    ///
    /// **⇒ 为什么要"留空洞"** ✗：第 755 轮我把种子设成 1、2，并进来的 4、5 全在**末尾**
    /// ⇒ 平移是**空操作** ⇒ 删掉平移那三行**也照样通过** ✓（变异 M-exit=0）⇒ 判据没鉴别力 ✗。
    /// ⇒ 所以这里种子是 **1、5**（中间有洞 ✓），并进 **3、4** ⇒ 会插到**中间** ✓ ⇒ 平移不再是空操作 ✓。
    ///
    /// **变异**：删掉 `adopt` 里"`by_id` 下标平移"那三行 ⇒ 下面 ④ 红 ✓（实测过 ✓）。
    #[test]
    fn adopt_keeps_foreign_sequences_and_translates_indexes() {
        let mut first = create_doc();
        first.seq = 1;
        let mut far = create_layer();
        far.seq = 5;
        let mut log = AtomLog::with_atoms(vec![first, far]).expect("建日志");
        assert_eq!(log.head_seq(), 5, "两条原子，最大 seq 是 5（中间有空洞）");

        let mut middle_a = create_object().with_id("atom_mid_a");
        middle_a.seq = 3;
        let mut middle_b = create_object().with_id("atom_mid_b");
        middle_b.seq = 4;
        log.adopt(vec![middle_a, middle_b]);

        // ① `next_seq` 必须推到 max + 1 ✓
        assert_eq!(log.next_seq(), 6, "最大 seq 5 ⇒ next_seq 必须是 6");
        assert_eq!(log.len(), 4, "四条原子");
        // ② 按 seq 取得到、且是对的那条 ✓
        assert_eq!(
            log.by_seq(3).map(|atom| atom.id.as_str()),
            Some("atom_mid_a")
        );
        assert_eq!(
            log.by_seq(4).map(|atom| atom.id.as_str()),
            Some("atom_mid_b")
        );
        // ③ 原本在后面的那条仍在 seq 5 ✓
        assert_eq!(
            log.by_seq(5).map(|atom| atom.id.as_str()),
            Some("atom_layer")
        );
        // ④ **`by_id` 必须跟着平移** ✓：`atom_layer` 原来在下标 1，插进两条后必须是下标 3 ✓
        assert_eq!(
            log.by_id.get("atom_layer").copied(),
            Some(3),
            "`by_id` 下标没平移（实测 {:?}）",
            log.by_id.get("atom_layer")
        );
        assert_eq!(
            log.by_id
                .get("atom_layer")
                .map(|index| log.atoms[*index].id.as_str()),
            Some("atom_layer"),
            "`by_id` 指错了原子"
        );
        // ⑤ 再并入同 id ⇒ 幂等 ✓（不重复、号不动）
        let mut again = create_object().with_id("atom_mid_a");
        again.seq = 3;
        log.adopt(vec![again]);
        assert_eq!(log.len(), 4, "同 id 再并入不许变多");
        assert_eq!(log.next_seq(), 6, "幂等之后 next_seq 不变");
    }

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
            json!({ "target": comment_id }),
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

#[cfg(test)]
mod audit_lock_tests_1440 {
    use super::*;
    use serde_json::json;

    fn set_prop(object_id: bool, key: &str, value: serde_json::Value) -> Atom {
        let mut payload = json!({"key": key, "value": value});
        if object_id {
            payload["object_id"] = json!("obj_1");
        } else {
            payload["layer_id"] = json!("layer_1");
        }
        Atom::new(AtomKind::SetProperty, "human:1", "session:a", payload).with_id("a_sp")
    }

    /// **提交层必须拒绝会被折叠层静默吞掉的非法值** ✓（第 1440 轮审计）。
    /// 变异：把 `validate_property_values` 的对应分支改成直接 `Ok(())` ⇒ 红 ✓。
    #[test]
    fn set_property_rejects_values_that_were_silently_defaulted() {
        let log = AtomLog::new();
        // 旧行为：`"visible": "yes"` ⇒ unwrap_or(true) 静默**显示** ✗。
        assert!(
            log.validate_property_values(&set_prop(true, "visible", json!("yes")))
                .is_err(),
            "非布尔的 visible 必须拒绝"
        );
        // 旧行为：未知 type 静默兜底成 Stroke ✗（第 44 轮在 CreateObject 堵过，SetProperty 漏网）。
        assert!(
            log.validate_property_values(&set_prop(true, "type", json!("banana")))
                .is_err(),
            "未知对象类型必须拒绝"
        );
        assert!(
            log.validate_property_values(&set_prop(false, "type", json!("banana")))
                .is_err(),
            "未知图层类型必须拒绝"
        );
        // 正例不许误伤 ✓。
        assert!(log
            .validate_property_values(&set_prop(true, "visible", json!(false)))
            .is_ok());
        assert!(log
            .validate_property_values(&set_prop(true, "type", json!("path")))
            .is_ok());
        assert!(log
            .validate_property_values(&set_prop(false, "opacity", json!(0.5)))
            .is_ok());
        assert!(log
            .validate_property_values(&set_prop(true, "z_index", json!(3)))
            .is_ok());
        // z_index=1.5：折叠层 as_i64 ⇒ None ⇒ 静默保持旧值 ✗ ⇒ 提交层拒绝 ✓。
        assert!(log
            .validate_property_values(&set_prop(true, "z_index", json!(1.5)))
            .is_err());
    }

    /// **create_document 宽高**：0、负数、小数尺寸、`2^32` 截断形态都要拒绝 ✓，
    /// 合法整数与整数值浮点放行 ✓（不误伤现有 JSON 里写 `64.0` 的调用 ✓）。
    #[test]
    fn create_document_rejects_unusable_dimensions() {
        let log = AtomLog::new();
        let doc = |w: serde_json::Value, h: serde_json::Value| {
            Atom::new(
                AtomKind::CreateDocument,
                "human:1",
                "session:a",
                json!({"doc_id": "doc_1", "width": w, "height": h}),
            )
            .with_id("a_doc")
        };
        assert!(log
            .validate_document_size(&doc(json!(0), json!(64)))
            .is_err());
        assert!(log
            .validate_document_size(&doc(json!(-5), json!(64)))
            .is_err());
        assert!(
            log.validate_document_size(&doc(json!(4294967296u64 + 4), json!(64)))
                .is_err(),
            "2^32+4 会被折叠层 as u32 截断成 4 ✗ ⇒ 提交必须拒绝"
        );
        assert!(log
            .validate_document_size(&doc(json!("wide"), json!(64)))
            .is_err());
        assert!(log
            .validate_document_size(&doc(json!(64), json!(64)))
            .is_ok());
        assert!(log
            .validate_document_size(&doc(json!(64.0), json!(64.0)))
            .is_ok());
        // 缺省宽高保持旧兼容 ✓（由上层默认处理）。
        assert!(log
            .validate_document_size(&Atom::new(
                AtomKind::CreateDocument,
                "human:1",
                "session:a",
                json!({"doc_id": "doc_1"})
            ))
            .is_ok());
    }
}
