//! 采样性替换的并发冲突检测（设计文档 12.3）。
//!
//! 两类操作语义截然不同：
//!
//! | 类型 | 操作 | 并发策略 |
//! |---|---|---|
//! | 生成性叠加 | draw_stroke、fill、draw_shape、draw_text、erase | 默认 LWW 自然叠加，不走冲突层 |
//! | 采样性替换 | retouch、inpaint、heal、patch | 基于源像素采样，并发互踩才升级冲突处理 |
//!
//! 服务端在提交校验阶段发现同一 Region 的并发采样性替换时**不追加原子**，
//! 直接返回 `conflict` 错误与冲突图层 id；若冲突图层不存在，先以系统 actor
//! （`actor: "system:conflict"`）追加 `create_layer` 原子。

use crate::atom::{payload_str, Atom, AtomKind, Refs};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::{ActorId, LayerId, SessionId};
use crate::state::DocumentState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 系统 actor：创建冲突图层（设计文档 12.3）。
pub const CONFLICT_ACTOR: &str = "system:conflict";

/// 轴对齐包围盒。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bbox {
    /// 左上角 x。
    pub x: f64,
    /// 左上角 y。
    pub y: f64,
    /// 宽。
    pub w: f64,
    /// 高。
    pub h: f64,
}

impl Bbox {
    /// 构造。
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    /// 是否为空盒。
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// 右边界。
    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    /// 下边界。
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    /// 是否相交（共享面积大于 0）。
    pub fn intersects(&self, other: &Bbox) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// 并集。
    pub fn union(&self, other: &Bbox) -> Bbox {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Bbox::new(
            x,
            y,
            self.right().max(other.right()) - x,
            self.bottom().max(other.bottom()) - y,
        )
    }

    /// 从 JSON 读取 `{x, y, w, h}`。
    pub fn from_value(value: &Value) -> Option<Self> {
        let object = value.as_object()?;
        let x = object.get("x")?.as_f64()?;
        let y = object.get("y")?.as_f64()?;
        let w = object.get("w").or_else(|| object.get("width"))?.as_f64()?;
        let h = object.get("h").or_else(|| object.get("height"))?.as_f64()?;
        Some(Self::new(x, y, w, h))
    }
}

/// 冲突信息：返回 `conflict` 错误时一并交给客户端。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictInfo {
    /// 冲突所在图层。
    pub layer_id: LayerId,
    /// 并发区间。
    pub region: Bbox,
    /// 对方原子。
    pub other_atom: String,
    /// 对方操作者。
    pub other_actor: ActorId,
    /// 对方会话。
    pub other_session: SessionId,
}

impl ConflictInfo {
    /// 转成设计文档 5.7 的错误响应。
    pub fn to_error(&self) -> YanshiError {
        YanshiError::new(
            ErrorCode::Conflict,
            ErrorContext::detail(format!(
                "采样性替换与原子 {} 在图层 {} 上并发互踩",
                self.other_atom, self.layer_id
            )),
        )
        .with_layer(self.layer_id.clone())
        .with_atom(self.other_atom.clone())
        .with_extra("conflict_layer_id", json!(self.layer_id))
        .with_extra("region", json!(self.region))
        .with_extra("other_session", json!(self.other_session))
    }
}

/// 该原子是否属于采样性替换（设计文档 12.3）。
///
/// 除 `retouch` / `liquify` 外，语义工具产生的 `RasterPatch` 若标记
/// `sampling: true`（inpaint / heal / patch）同样进入冲突层。
pub fn is_sampling_replace(atom: &Atom) -> bool {
    if atom.kind.is_sampling_replace() {
        return true;
    }
    if !matches!(atom.kind, AtomKind::CreateObject | AtomKind::Supersede) {
        return false;
    }
    payload_str(&atom.payload, "type") == Some("raster_patch")
        && atom
            .payload
            .get("sampling")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// 提取原子影响的区域：优先 `region`，其次 `source_region`、`target_region`。
pub fn atom_region(atom: &Atom) -> Option<Bbox> {
    for key in ["region", "source_region", "target_region"] {
        if let Some(value) = atom.payload.get(key) {
            if let Some(bbox) = Bbox::from_value(value) {
                return Some(bbox);
            }
        }
    }
    None
}

/// 在候选原子（同一并发窗口内的其他会话原子）中查找采样性替换冲突。
///
/// 返回 `Ok(())` 表示无冲突；冲突时返回 [`ConflictInfo`]，由调用方转成错误响应。
pub fn find_conflict(candidates: &[&Atom], atom: &Atom) -> Option<ConflictInfo> {
    if !is_sampling_replace(atom) {
        return None;
    }
    let layer_id = atom.layer_id()?;
    let region = atom_region(atom)?;
    for other in candidates {
        if other.id == atom.id || other.session == atom.session {
            continue;
        }
        if !is_sampling_replace(other) {
            continue;
        }
        if other.layer_id() != Some(layer_id) {
            continue;
        }
        let Some(other_region) = atom_region(other) else {
            continue;
        };
        if region.intersects(&other_region) {
            return Some(ConflictInfo {
                layer_id: layer_id.to_owned(),
                region: region.union(&other_region),
                other_atom: other.id.clone(),
                other_actor: other.actor.clone(),
                other_session: other.session.clone(),
            });
        }
    }
    None
}

/// 构造系统冲突图层原子（设计文档 12.3 第 2 步）。
///
/// `metadata.conflict = true` 标记保留供审计，冲突图层在 `resolve_conflict` 后被 tombstone。
pub fn conflict_layer_atom(
    new_layer_id: impl Into<LayerId>,
    doc_id: Option<&str>,
    z_index: i64,
) -> Atom {
    let layer_id = new_layer_id.into();
    let payload = json!({
        "layer_id": layer_id,
        "doc_id": doc_id,
        "name": "conflict",
        "type": "raster",
        "z_index": z_index,
        "metadata": {"conflict": true},
    });
    Atom::new(
        AtomKind::CreateLayer,
        CONFLICT_ACTOR,
        "session:conflict",
        payload,
    )
}

/// 校验客户端重提交时的落点：冲突图层必须存在（设计文档 12.3 第 3 步）。
pub fn validate_conflict_sink(state: &DocumentState, atom: &Atom) -> Result<()> {
    let Some(layer_id) = atom.layer_id() else {
        return Ok(());
    };
    if state.layer_alive(layer_id) {
        return Ok(());
    }
    Err(YanshiError::new(
        ErrorCode::ReferenceNotFound,
        ErrorContext::detail(format!("冲突图层 {layer_id} 不存在")),
    )
    .with_layer(layer_id))
}

/// 冲突图层的审计信息是否保留（`resolve_conflict` 后仍保留标记）。
pub fn is_conflict_layer(atom: &Atom) -> bool {
    atom.kind == AtomKind::CreateLayer
        && atom
            .payload
            .get("metadata")
            .and_then(|m| m.get("conflict"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

/// 便捷：该原子声明的引用（供调用方做冲突层落点校验时复用）。
pub fn atom_refs(atom: &Atom) -> &Refs {
    &atom.refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retouch(id: &str, session: &str, layer: &str, region: Bbox) -> Atom {
        Atom::new(
            AtomKind::Retouch,
            format!("human:{session}"),
            session,
            json!({
                "object_id": format!("obj_{id}"),
                "layer_id": layer,
                "region": {"x": region.x, "y": region.y, "w": region.w, "h": region.h},
            }),
        )
        .with_id(id)
    }

    #[test]
    fn bbox_intersection_and_union() {
        let a = Bbox::new(0.0, 0.0, 10.0, 10.0);
        let b = Bbox::new(5.0, 5.0, 10.0, 10.0);
        let c = Bbox::new(20.0, 20.0, 1.0, 1.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
        assert!(
            !a.intersects(&Bbox::new(10.0, 0.0, 5.0, 5.0)),
            "仅相切不算相交"
        );
        assert_eq!(a.union(&b), Bbox::new(0.0, 0.0, 15.0, 15.0));
        assert_eq!(a.union(&Bbox::new(0.0, 0.0, 0.0, 0.0)), a);
    }

    #[test]
    fn sampling_conflict_detected_across_sessions() {
        let mine = retouch(
            "atom_a",
            "session:a",
            "layer_1",
            Bbox::new(0.0, 0.0, 10.0, 10.0),
        );
        let theirs = retouch(
            "atom_b",
            "session:b",
            "layer_1",
            Bbox::new(5.0, 5.0, 4.0, 4.0),
        );
        let elsewhere = retouch(
            "atom_c",
            "session:c",
            "layer_1",
            Bbox::new(50.0, 50.0, 4.0, 4.0),
        );
        let other_layer = retouch(
            "atom_d",
            "session:d",
            "layer_2",
            Bbox::new(1.0, 1.0, 4.0, 4.0),
        );

        let info = find_conflict(&[&theirs, &elsewhere, &other_layer], &mine).unwrap();
        assert_eq!(info.layer_id, "layer_1");
        assert_eq!(info.other_atom, "atom_b");
        assert_eq!(info.other_session, "session:b");
        let error = info.to_error();
        assert_eq!(error.code, ErrorCode::Conflict);
        assert!(error.retryable);
        assert_eq!(error.context.extra["conflict_layer_id"], json!("layer_1"));
    }

    #[test]
    fn generative_overlay_never_conflicts() {
        let stroke = Atom::new(
            AtomKind::DrawStroke,
            "human:a",
            "session:a",
            json!({"object_id": "o1", "layer_id": "layer_1", "region": {"x":0,"y":0,"w":9,"h":9}}),
        );
        let other = retouch(
            "atom_b",
            "session:b",
            "layer_1",
            Bbox::new(0.0, 0.0, 9.0, 9.0),
        );
        assert!(!is_sampling_replace(&stroke));
        assert!(find_conflict(&[&other], &stroke).is_none());
    }

    #[test]
    fn same_session_never_conflicts() {
        let mine = retouch(
            "atom_a",
            "session:a",
            "layer_1",
            Bbox::new(0.0, 0.0, 10.0, 10.0),
        );
        let same = retouch(
            "atom_b",
            "session:a",
            "layer_1",
            Bbox::new(1.0, 1.0, 1.0, 1.0),
        );
        assert!(find_conflict(&[&same], &mine).is_none());
    }

    #[test]
    fn semantic_patch_with_sampling_flag_conflicts() {
        let patch = Atom::new(
            AtomKind::CreateObject,
            "ai:1",
            "session:ai",
            json!({
                "object_id": "o_patch",
                "layer_id": "layer_1",
                "type": "raster_patch",
                "sampling": true,
                "region": {"x": 0, "y": 0, "w": 8, "h": 8},
            }),
        );
        assert!(is_sampling_replace(&patch));
        let theirs = retouch(
            "atom_b",
            "session:b",
            "layer_1",
            Bbox::new(4.0, 4.0, 8.0, 8.0),
        );
        assert!(find_conflict(&[&theirs], &patch).is_some());

        // 未标记 sampling 的 RasterPatch 属于生成性叠加。
        let generated = Atom::new(
            AtomKind::CreateObject,
            "ai:1",
            "session:ai",
            json!({"type": "raster_patch", "layer_id": "layer_1"}),
        );
        assert!(!is_sampling_replace(&generated));
    }

    #[test]
    fn conflict_layer_atom_is_marked_for_audit() {
        let atom = conflict_layer_atom("layer_conflict", Some("doc_1"), 3);
        assert_eq!(atom.actor, CONFLICT_ACTOR);
        assert!(is_conflict_layer(&atom));
        assert_eq!(atom.kind, AtomKind::CreateLayer);
        assert_eq!(atom.layer_id(), Some("layer_conflict"));
    }
}
