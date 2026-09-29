//! 标注通道（设计文档 4.6 / 13.4）。
//!
//! 标注走**独立 append-only 通道**，不进入主原子日志，不参与折叠。
//! 更新以「追加新版本」表达，读取时取每个 id 的最新版本。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use yanshi_core::{
    ActorId, AtomId, Bbox, DocId, ErrorCode, ErrorContext, ObjectId, Result, Seq, Ulid, YanshiError,
};

/// 标注标识。
pub type AnnotationId = String;

/// 标注类型（4.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationType {
    /// 区域。
    Region,
    /// 对象。
    Object,
    /// 箭头。
    Arrow,
    /// 文字批注。
    Text,
    /// 涂鸦。
    Doodle,
    /// 高亮。
    Highlight,
}

impl AnnotationType {
    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Region => "region",
            Self::Object => "object",
            Self::Arrow => "arrow",
            Self::Text => "text",
            Self::Doodle => "doodle",
            Self::Highlight => "highlight",
        }
    }

    /// 由字符串解析（未知类型报参数错误）。
    pub fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "region" => Self::Region,
            "object" => Self::Object,
            "arrow" => Self::Arrow,
            "text" => Self::Text,
            "doodle" => Self::Doodle,
            "highlight" => Self::Highlight,
            other => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("未知标注类型 {other}")),
                ))
            }
        })
    }
}

/// 标注意图（13.4：人类告诉 AI 改哪里、怎么改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationIntent {
    /// 修改。
    Modify,
    /// 新增。
    Add,
    /// 删除。
    Remove,
    /// 替换。
    Replace,
    /// 风格。
    Style,
    /// 移动。
    Move,
    /// 缩放。
    Resize,
    /// 颜色。
    Color,
}

impl AnnotationIntent {
    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Modify => "modify",
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Replace => "replace",
            Self::Style => "style",
            Self::Move => "move",
            Self::Resize => "resize",
            Self::Color => "color",
        }
    }

    /// 由字符串解析。
    pub fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "modify" => Self::Modify,
            "add" => Self::Add,
            "remove" => Self::Remove,
            "replace" => Self::Replace,
            "style" => Self::Style,
            "move" => Self::Move,
            "resize" => Self::Resize,
            "color" => Self::Color,
            other => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("未知标注意图 {other}")),
                ))
            }
        })
    }

    /// 全部取值。
    pub const ALL: [AnnotationIntent; 8] = [
        Self::Modify,
        Self::Add,
        Self::Remove,
        Self::Replace,
        Self::Style,
        Self::Move,
        Self::Resize,
        Self::Color,
    ];
}

/// 标注状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationStatus {
    /// 待处理。
    Pending,
    /// 已解决。
    Resolved,
    /// 已拒绝。
    Rejected,
}

impl AnnotationStatus {
    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Rejected => "rejected",
        }
    }

    /// 由字符串解析。
    pub fn parse(text: &str) -> Result<Self> {
        Ok(match text {
            "pending" => Self::Pending,
            "resolved" => Self::Resolved,
            "rejected" => Self::Rejected,
            other => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("未知标注状态 {other}")),
                ))
            }
        })
    }
}

/// 标注目标。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum AnnotationTarget {
    /// 区域（文档坐标）。
    Region {
        /// 包围盒。
        bbox: Bbox,
    },
    /// 对象引用。
    Object {
        /// 对象 id。
        object_id: ObjectId,
    },
}

/// 标注（4.6 的字段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// 标识。
    pub id: AnnotationId,
    /// 文档。
    pub doc_id: DocId,
    /// 作者。
    pub actor: ActorId,
    /// 创建时的 head seq。
    pub head_seq: Seq,
    /// 类型。
    #[serde(rename = "type")]
    pub annotation_type: AnnotationType,
    /// 目标。
    pub target: AnnotationTarget,
    /// 内容（文本或路径）。
    pub content: String,
    /// 意图。
    pub intent: AnnotationIntent,
    /// 状态。
    pub status: AnnotationStatus,
    /// 创建时间。
    pub created_at: i64,
    /// 更新时间。
    pub updated_at: i64,
    /// 关联建议。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion_id: Option<String>,
    /// 解决它的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<AtomId>,
    /// 解决时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<i64>,
    /// 版本号（append-only 通道内的修订次数）。
    #[serde(default)]
    pub revision: u32,
}

/// 取某标注的最新版本（内部辅助）。
fn latest_of(versions: &mut [Annotation]) -> Annotation {
    versions
        .last()
        .cloned()
        .unwrap_or_else(|| unreachable!("版本列表非空"))
}

/// 创建标注的入参。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewAnnotation {
    /// 类型。
    #[serde(rename = "type")]
    pub annotation_type: AnnotationType,
    /// 目标。
    pub target: AnnotationTarget,
    /// 内容。
    #[serde(default)]
    pub content: String,
    /// 意图。
    pub intent: AnnotationIntent,
    /// 关联建议。
    #[serde(default)]
    pub suggestion_id: Option<String>,
}

/// 标注过滤条件。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationFilter {
    /// 按状态过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AnnotationStatus>,
    /// 按作者过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<ActorId>,
    /// 按意图过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<AnnotationIntent>,
    /// 按对象目标过滤。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<ObjectId>,
    /// 按关联建议过滤（4b：查询「引用了某条建议的标注」）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion_id: Option<String>,
}

/// 标注通道（append-only 版本链）。
#[derive(Debug, Default)]
pub struct AnnotationStore {
    /// id → 全部版本（追加序）。
    versions: BTreeMap<AnnotationId, Vec<Annotation>>,
    /// 追加顺序。
    order: Vec<AnnotationId>,
    appended: u64,
}

impl AnnotationStore {
    /// 空通道。
    pub fn new() -> Self {
        Self::default()
    }

    /// 追加次数（含修订）。
    pub fn appended(&self) -> u64 {
        self.appended
    }

    /// 标注个数（每个 id 一个当前版本）。
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    /// 追加新标注。
    pub fn create(
        &mut self,
        doc_id: &str,
        actor: impl Into<ActorId>,
        head_seq: Seq,
        input: NewAnnotation,
        now: i64,
    ) -> AnnotationId {
        let id = format!("ann_{}", Ulid::new().encode());
        let annotation = Annotation {
            id: id.clone(),
            doc_id: doc_id.to_owned(),
            actor: actor.into(),
            head_seq,
            annotation_type: input.annotation_type,
            target: input.target,
            content: input.content,
            intent: input.intent,
            status: AnnotationStatus::Pending,
            created_at: now,
            updated_at: now,
            suggestion_id: input.suggestion_id,
            resolved_by: None,
            resolved_at: None,
            revision: 1,
        };
        self.versions.insert(id.clone(), vec![annotation]);
        self.order.push(id.clone());
        self.appended += 1;
        id
    }

    /// 更新（追加新版本）。
    pub fn update(
        &mut self,
        id: &str,
        content: Option<String>,
        intent: Option<AnnotationIntent>,
        target: Option<AnnotationTarget>,
        now: i64,
    ) -> Result<Annotation> {
        let versions = self.versions.get_mut(id).ok_or_else(|| not_found(id))?;
        let Some(latest) = versions.last().cloned() else {
            return Err(not_found(id));
        };
        let mut next = latest;
        if let Some(content) = content {
            next.content = content;
        }
        if let Some(intent) = intent {
            next.intent = intent;
        }
        if let Some(target) = target {
            next.target = target;
        }
        next.updated_at = now;
        next.revision += 1;
        versions.push(next.clone());
        self.appended += 1;
        Ok(next)
    }

    /// 标记解决（可由某个原子触发，见 12.6 `accept_suggestion → reapply`）。
    ///
    /// 状态机（设计 4.6 只列了三态，未规定转换规则，此处记录决定）：
    /// * 已 resolved → **幂等**返回当前版本（不新增版本、不动 revision）；
    /// * 已 rejected → `precondition_failed`（必须先重开，避免历史自相矛盾）。
    pub fn resolve(&mut self, id: &str, atom_id: Option<AtomId>, now: i64) -> Result<Annotation> {
        self.transition_gated(id, AnnotationStatus::Resolved, atom_id, now)
    }

    /// 标记拒绝。规则与 [`Self::resolve`] 对称（幂等 / 不得从 resolved 直接翻转）。
    pub fn reject(&mut self, id: &str, now: i64) -> Result<Annotation> {
        self.transition_gated(id, AnnotationStatus::Rejected, None, now)
    }

    /// 重开为待处理：允许从 resolved / rejected 回到 pending，并清空 `resolved_by`/`resolved_at`。
    pub fn reopen(&mut self, id: &str, now: i64) -> Result<Annotation> {
        self.transition(id, AnnotationStatus::Pending, None, now)?;
        let versions = self.versions.get_mut(id).ok_or_else(|| not_found(id))?;
        if let Some(latest) = versions.last_mut() {
            latest.resolved_by = None;
            latest.resolved_at = None;
        }
        Ok(latest_of(versions))
    }

    /// 删除：append-only 通道里删除等价于标记拒绝并清空内容（**允许从任意状态**执行）。
    pub fn delete(&mut self, id: &str, now: i64) -> Result<Annotation> {
        let annotation = self.transition(id, AnnotationStatus::Rejected, None, now)?;
        let versions = self.versions.get_mut(id).ok_or_else(|| not_found(id))?;
        if let Some(latest) = versions.last_mut() {
            latest.content.clear();
        }
        Ok(annotation)
    }

    /// 取当前版本。
    pub fn get(&self, id: &str) -> Result<Annotation> {
        self.versions
            .get(id)
            .and_then(|versions| versions.last().cloned())
            .ok_or_else(|| not_found(id))
    }

    /// 修订历史。
    pub fn history(&self, id: &str) -> Vec<Annotation> {
        self.versions.get(id).cloned().unwrap_or_default()
    }

    /// 列表（按追加顺序，取每个 id 的当前版本）。
    pub fn list(&self, filter: &AnnotationFilter) -> Vec<Annotation> {
        self.order
            .iter()
            .filter_map(|id| self.versions.get(id))
            .filter_map(|versions| versions.last())
            .filter(|annotation| matches_filter(annotation, filter))
            .cloned()
            .collect()
    }

    /// 待处理标注数（AI 侧轮询 `list_annotations(status=pending)` 或 WS 事件）。
    pub fn pending_count(&self) -> usize {
        self.list(&AnnotationFilter {
            status: Some(AnnotationStatus::Pending),
            ..AnnotationFilter::default()
        })
        .len()
    }

    /// 带状态机闸门的转换：幂等 + 禁止 resolved ↔ rejected 直接翻转。
    fn transition_gated(
        &mut self,
        id: &str,
        status: AnnotationStatus,
        atom_id: Option<AtomId>,
        now: i64,
    ) -> Result<Annotation> {
        let current = self.get(id)?;
        if current.status == status {
            // 幂等：不新增版本、不递增 revision。
            return Ok(current);
        }
        if matches!(
            (current.status, status),
            (AnnotationStatus::Resolved, AnnotationStatus::Rejected)
                | (AnnotationStatus::Rejected, AnnotationStatus::Resolved)
        ) {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "标注 {id} 当前为 {}，不能直接翻转为 {}：请先重开（status=pending）",
                    current.status.as_str(),
                    status.as_str()
                )),
            ));
        }
        self.transition(id, status, atom_id, now)
    }

    fn transition(
        &mut self,
        id: &str,
        status: AnnotationStatus,
        atom_id: Option<AtomId>,
        now: i64,
    ) -> Result<Annotation> {
        let versions = self.versions.get_mut(id).ok_or_else(|| not_found(id))?;
        let Some(latest) = versions.last().cloned() else {
            return Err(not_found(id));
        };
        let mut next = latest;
        next.status = status;
        next.updated_at = now;
        next.revision += 1;
        if let Some(atom_id) = atom_id {
            next.resolved_by = Some(atom_id);
            next.resolved_at = Some(now);
        }
        versions.push(next.clone());
        self.appended += 1;
        Ok(next)
    }
}

fn matches_filter(annotation: &Annotation, filter: &AnnotationFilter) -> bool {
    if let Some(status) = filter.status {
        if annotation.status != status {
            return false;
        }
    }
    if let Some(actor) = &filter.actor {
        if &annotation.actor != actor {
            return false;
        }
    }
    if let Some(intent) = filter.intent {
        if annotation.intent != intent {
            return false;
        }
    }
    if let Some(suggestion_id) = &filter.suggestion_id {
        if annotation.suggestion_id.as_deref() != Some(suggestion_id.as_str()) {
            return false;
        }
    }
    if let Some(object_id) = &filter.object_id {
        match &annotation.target {
            AnnotationTarget::Object {
                object_id: target, ..
            } if target == object_id => {}
            _ => return false,
        }
    }
    true
}

fn not_found(id: &str) -> YanshiError {
    YanshiError::new(
        ErrorCode::ReferenceNotFound,
        ErrorContext::detail(format!("标注 {id} 不存在")),
    )
    .with_extra("annotation_id", serde_json::json!(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(intent: AnnotationIntent) -> NewAnnotation {
        NewAnnotation {
            annotation_type: AnnotationType::Region,
            target: AnnotationTarget::Region {
                bbox: Bbox::new(10.0, 10.0, 20.0, 20.0),
            },
            content: "这里的天空太亮了".to_owned(),
            intent,
            suggestion_id: None,
        }
    }

    #[test]
    fn create_update_resolve_are_append_only() {
        let mut store = AnnotationStore::new();
        let id = store.create(
            "doc_1",
            "human:1",
            7,
            input(AnnotationIntent::Modify),
            1_000,
        );
        assert_eq!(store.len(), 1);
        assert_eq!(store.appended(), 1);

        let updated = store
            .update(&id, Some("改成晚霞".to_owned()), None, None, 2_000)
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.content, "改成晚霞");
        assert_eq!(store.appended(), 2, "更新也走 append-only");
        assert_eq!(store.history(&id).len(), 2, "保留全部版本");
        assert_eq!(store.get(&id).unwrap().content, "改成晚霞");

        let resolved = store
            .resolve(&id, Some("atom_1".to_owned()), 3_000)
            .unwrap();
        assert_eq!(resolved.status, AnnotationStatus::Resolved);
        assert_eq!(resolved.resolved_by.as_deref(), Some("atom_1"));
        assert_eq!(store.pending_count(), 0);
        assert_eq!(store.len(), 1, "同一 id 只有一条当前版本");
    }

    #[test]
    fn delete_marks_rejected_without_removing_history() {
        let mut store = AnnotationStore::new();
        let id = store.create("doc_1", "human:1", 1, input(AnnotationIntent::Remove), 0);
        store.delete(&id, 5).unwrap();
        let current = store.get(&id).unwrap();
        assert_eq!(current.status, AnnotationStatus::Rejected);
        assert!(current.content.is_empty());
        assert_eq!(store.history(&id).len(), 2);
    }

    #[test]
    fn filters_cover_status_actor_intent_and_object() {
        let mut store = AnnotationStore::new();
        let first = store.create("doc_1", "human:1", 1, input(AnnotationIntent::Modify), 0);
        let mut object_input = input(AnnotationIntent::Style);
        object_input.target = AnnotationTarget::Object {
            object_id: "obj_1".to_owned(),
        };
        let second = store.create("doc_1", "ai:1", 2, object_input, 0);
        store.resolve(&first, None, 10).unwrap();

        let pending = store.list(&AnnotationFilter {
            status: Some(AnnotationStatus::Pending),
            ..AnnotationFilter::default()
        });
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, second);

        let by_actor = store.list(&AnnotationFilter {
            actor: Some("ai:1".to_owned()),
            ..AnnotationFilter::default()
        });
        assert_eq!(by_actor.len(), 1);
        assert_eq!(by_actor[0].id, second);

        let by_object = store.list(&AnnotationFilter {
            object_id: Some("obj_1".to_owned()),
            ..AnnotationFilter::default()
        });
        assert_eq!(by_object.len(), 1);

        let by_intent = store.list(&AnnotationFilter {
            intent: Some(AnnotationIntent::Modify),
            ..AnnotationFilter::default()
        });
        assert_eq!(by_intent.len(), 1);
        assert_eq!(by_intent[0].id, first);
    }

    #[test]
    fn unknown_ids_and_names_report_errors() {
        let mut store = AnnotationStore::new();
        assert_eq!(
            store.get("ann_missing").unwrap_err().code,
            ErrorCode::ReferenceNotFound
        );
        assert_eq!(
            store
                .update("ann_missing", None, None, None, 0)
                .unwrap_err()
                .code,
            ErrorCode::ReferenceNotFound
        );
        assert_eq!(
            AnnotationType::parse("nope").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        assert_eq!(
            AnnotationIntent::parse("nope").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        assert_eq!(
            AnnotationStatus::parse("nope").unwrap_err().code,
            ErrorCode::InvalidArgument
        );
        assert_eq!(AnnotationIntent::ALL.len(), 8);
    }

    #[test]
    fn annotations_never_touch_the_atom_log() {
        // 标注通道与原子日志完全独立：这里只用本地通道 API 验证其自洽性。
        let mut store = AnnotationStore::new();
        for index in 0..5 {
            store.create(
                "doc_1",
                "human:1",
                index as Seq,
                input(AnnotationIntent::Add),
                index as i64,
            );
        }
        assert_eq!(store.len(), 5);
        assert_eq!(store.pending_count(), 5);
        assert_eq!(store.appended(), 5);
    }
}
