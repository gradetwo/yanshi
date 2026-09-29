//! 变更集（设计文档 5.6 / 12.6）。
//!
//! 变更集是一组原子的逻辑分组：batch 通常对应一个变更集，撤销默认按变更集粒度，
//! 也支持按单个原子粒度。变更集本身不是新原子类型——`changeset_id` 是原子字段。

use crate::atom::{Atom, AtomId, AtomKind};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::{ActorId, ChangesetId, Seq, SessionId, Ulid};
use crate::log::AtomLog;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// 变更集。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changeset {
    /// 变更集 id。
    pub id: ChangesetId,
    /// 名称。
    pub name: String,
    /// 发起者。
    pub actor: ActorId,
    /// 创建时间（Unix 毫秒）。
    pub timestamp: i64,
    /// 成员原子（按 seq 升序）。
    pub atoms: Vec<AtomId>,
    /// 说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl Changeset {
    /// 从日志聚合出一个变更集。
    pub fn from_log(log: &AtomLog, changeset_id: &str) -> Option<Self> {
        let atoms = log.changeset_atoms(changeset_id);
        let first = atoms.first()?;
        Some(Self {
            id: changeset_id.to_owned(),
            name: first
                .message
                .clone()
                .unwrap_or_else(|| changeset_id.to_owned()),
            actor: first.actor.clone(),
            timestamp: first.timestamp,
            atoms: atoms.iter().map(|atom| atom.id.clone()).collect(),
            message: None,
        })
    }

    /// 日志中出现的全部变更集 id。
    pub fn ids_in_log(log: &AtomLog) -> Vec<ChangesetId> {
        let mut ids: Vec<ChangesetId> = log
            .iter()
            .filter_map(|atom| atom.changeset_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// 生成新变更集 id（客户端）。
    pub fn new_id() -> ChangesetId {
        Ulid::new().encode()
    }

    /// 成员原子在日志中的 seq 列表。
    pub fn seqs(&self, log: &AtomLog) -> Vec<Seq> {
        self.atoms.iter().filter_map(|id| log.seq_of(id)).collect()
    }

    /// 整体撤销计划：为每个**当前有效**且未在本计划内重复的状态原子生成 `revert`。
    ///
    /// 设计文档 5.3：`revert` 只恢复/撤销目标原子，不恢复级联链；因此撤销一个变更集
    /// 会产生级联失效，UI 需要提示“以下 N 个编辑不会被恢复”。
    ///
    /// `cross_actor_allowed` 为 false 时，只撤销 `actor` 自己的原子（设计文档 12.5）。
    pub fn revert_plan(
        &self,
        log: &AtomLog,
        actor: &str,
        session: &str,
        cross_actor_allowed: bool,
    ) -> Result<Vec<Atom>> {
        let existing_reverts = crate::fold::compute_suppressed(log.atoms());
        let mut plan = Vec::new();
        for atom_id in &self.atoms {
            let Some(target) = log.get(atom_id) else {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("变更集成员原子 {atom_id} 不在日志中")),
                )
                .with_atom(atom_id.clone()));
            };
            if !target.kind.is_state_effect() {
                continue; // 协作原子与历史原子不可 revert（设计文档 5.4）。
            }
            if existing_reverts.contains(&target.id) {
                continue; // 已被撤销。
            }
            if !cross_actor_allowed && target.actor != actor {
                return Err(YanshiError::new(
                    ErrorCode::PermissionDenied,
                    ErrorContext::detail(format!(
                        "变更集包含 {} 的原子 {atom_id}，跨 actor 撤销需要 owner 权限",
                        target.actor
                    )),
                )
                .with_atom(atom_id.clone()));
            }
            if !target.kind.is_state_effect() && target.kind != AtomKind::Tombstone {
                continue;
            }
            plan.push(
                Atom::new(
                    AtomKind::Revert,
                    actor.to_owned(),
                    session.to_owned(),
                    json!({"target": target.id, "changeset_id": self.id}),
                )
                .with_changeset(self.id.clone())
                .with_message(format!("revert_changeset {}", self.id)),
            );
        }
        Ok(plan)
    }

    /// 该变更集是否只包含协作类原子。
    pub fn is_collab_only(&self, log: &AtomLog) -> bool {
        self.atoms
            .iter()
            .filter_map(|id| log.get(id))
            .all(|atom| atom.kind.is_collab())
    }

    /// 变更集内原子的类型分布。
    pub fn kind_histogram(&self, log: &AtomLog) -> Vec<(AtomKind, usize)> {
        let mut counts: Vec<(AtomKind, usize)> = Vec::new();
        for atom_id in &self.atoms {
            let Some(atom) = log.get(atom_id) else {
                continue;
            };
            match counts.iter_mut().find(|(kind, _)| *kind == atom.kind) {
                Some((_, count)) => *count += 1,
                None => counts.push((atom.kind, 1)),
            }
        }
        counts.sort_by_key(|(kind, _)| *kind);
        counts
    }
}

/// 会话级的变更集构造器：为 batch 内的原子统一打上 `changeset_id`。
#[derive(Debug, Clone)]
pub struct ChangesetBuilder {
    changeset: Changeset,
}

impl ChangesetBuilder {
    /// 新建变更集。
    pub fn begin(actor: impl Into<ActorId>, name: impl Into<String>) -> Self {
        Self {
            changeset: Changeset {
                id: Changeset::new_id(),
                name: name.into(),
                actor: actor.into(),
                timestamp: crate::ids::now_ms(),
                atoms: Vec::new(),
                message: None,
            },
        }
    }

    /// 变更集 id。
    pub fn id(&self) -> &str {
        &self.changeset.id
    }

    /// 说明。
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.changeset.message = Some(message.into());
        self
    }

    /// 把一个原子纳入变更集（写入 `changeset_id` 并登记成员）。
    pub fn attach(&mut self, mut atom: Atom) -> Atom {
        atom.changeset_id = Some(self.changeset.id.clone());
        self.changeset.atoms.push(atom.id.clone());
        atom
    }

    /// 结束并取出变更集。
    pub fn finish(self) -> Changeset {
        self.changeset
    }
}

/// 便捷：会话 + actor 组合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorSession {
    /// 操作者。
    pub actor: ActorId,
    /// 会话。
    pub session: SessionId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fold::{fold_atoms, FoldEngine};
    use crate::state::DocumentState;
    use serde_json::Value;

    fn atom(kind: AtomKind, id: &str, payload: Value) -> Atom {
        Atom::new(kind, "ai:1", "session:ai", payload).with_id(id)
    }

    fn batch_log() -> (AtomLog, Changeset) {
        let mut log = AtomLog::new();
        let mut builder = ChangesetBuilder::begin("ai:1", "house").with_message("画一栋房子");
        for built in [
            atom(
                AtomKind::CreateDocument,
                "a_doc",
                json!({"doc_id": "doc_1", "width": 64, "height": 64}),
            ),
            atom(
                AtomKind::CreateLayer,
                "a_layer",
                json!({"layer_id": "layer_1"}),
            ),
            atom(
                AtomKind::CreateObject,
                "a_obj",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "type": "stroke"}),
            ),
        ] {
            log.append(builder.attach(built)).unwrap();
        }
        (log, builder.finish())
    }

    #[test]
    fn builder_tags_members_and_aggregates() {
        let (log, built) = batch_log();
        assert_eq!(built.name, "house");
        assert_eq!(built.message.as_deref(), Some("画一栋房子"));
        let changeset = Changeset::from_log(&log, &built.id).unwrap();
        assert_eq!(changeset.id, built.id);
        assert_eq!(changeset.actor, "ai:1");
        assert_eq!(changeset.atoms.len(), 3);
        assert_eq!(changeset.seqs(&log), vec![1, 2, 3]);
        assert_eq!(
            changeset.kind_histogram(&log),
            vec![
                (AtomKind::CreateDocument, 1),
                (AtomKind::CreateLayer, 1),
                (AtomKind::CreateObject, 1)
            ]
        );
        assert!(!changeset.is_collab_only(&log));
        assert_eq!(Changeset::ids_in_log(&log).len(), 1);
    }

    #[test]
    fn revert_plan_undoes_the_whole_changeset() {
        let (mut log, changeset) = batch_log();
        let plan = changeset
            .revert_plan(&log, "ai:1", "session:ai", false)
            .unwrap();
        assert_eq!(plan.len(), 3);
        assert!(plan.iter().all(|atom| atom.kind == AtomKind::Revert));
        assert!(plan
            .iter()
            .all(|atom| atom.changeset_id.as_deref() == Some(changeset.id.as_str())));

        for revert in plan {
            log.append(revert).unwrap();
        }
        let state = FoldEngine::new().fold(&log).unwrap().state;
        assert!(state.layers.is_empty(), "变更集整体撤销后图层消失");
        assert!(state.objects.is_empty());
        assert!(state.is_consistent());

        // 已撤销的原子不再重复出现在新的撤销计划里。
        let second = changeset
            .revert_plan(&log, "ai:1", "session:ai", false)
            .unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn revert_plan_rejects_cross_actor_without_owner() {
        let (log, changeset) = batch_log();
        let error = changeset
            .revert_plan(&log, "human:2", "session:b", false)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        let allowed = changeset
            .revert_plan(&log, "human:2", "session:b", true)
            .unwrap();
        assert_eq!(allowed.len(), 3);
    }

    #[test]
    fn collab_atoms_are_skipped_in_revert_plan() {
        let (mut log, built) = batch_log();
        let changeset_id = built.id.clone();
        log.append(
            atom(
                AtomKind::Comment,
                "a_comment",
                json!({"target_atom": "a_obj", "text": "再加个窗户"}),
            )
            .with_changeset(changeset_id.clone()),
        )
        .unwrap();
        let changeset = Changeset::from_log(&log, &changeset_id).unwrap();
        assert_eq!(changeset.atoms.len(), 4);
        let plan = changeset
            .revert_plan(&log, "ai:1", "session:ai", false)
            .unwrap();
        assert_eq!(plan.len(), 3, "协作原子不可 revert");
    }

    #[test]
    fn fold_of_empty_prefix_matches_empty_state() {
        let result = fold_atoms(DocumentState::empty(), &[]);
        assert_eq!(result.state, DocumentState::empty());
    }
}
