//! 几何 / 结构双 dirty 传播（设计文档 6.6 / 8.4）。
//!
//! | 类型 | 触发 | 传播范围 |
//! |---|---|---|
//! | **几何 dirty** | draw_stroke、fill、move 等 | 受影响对象的 bbox 并集 |
//! | **结构 dirty** | revert、supersede、tombstone、declare_head、z_index、visible、图层归属变更 | 依赖图传播闭包中所有对象的效果 bbox 并集 |
//!
//! 属性变更（`z_index`、`visible`、归属）不对应几何 bbox，必须通过依赖图算出受影响对象集，
//! 再取其 bbox 并集标记 tile 失效——这正是本模块的职责。
//!
//! ## 保守性约定
//!
//! 无法精确判定依赖关系时（实例/组引用尚未解析、调整对象影响范围未知、目标实体已从状态中消失），
//! 本模块**宁可放大**失效范围（整层或整文档），因为这只会多渲染，不会留下陈旧 tile。

use crate::object::{layer_bbox, object_bbox};
use crate::tile::{TileGrid, TileKey};
use serde_json::Value;
use std::collections::{BTreeSet, VecDeque};
use yanshi_core::{Atom, AtomKind, AtomLog, Bbox, DocumentState, ObjectId};

/// dirty 类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyKind {
    /// 无变化。
    None,
    /// 几何 dirty：只有 bbox 内的像素需要重算。
    Geometry,
    /// 结构 dirty：依赖闭包内所有对象的 bbox 需要重算。
    Structure,
    /// 全量失效（求值起点跳变等）。
    Full,
}

/// dirty 集合。
#[derive(Debug, Clone, PartialEq)]
pub struct DirtySet {
    /// 类型。
    pub kind: DirtyKind,
    /// 受影响的对象（结构 dirty 时非空）。
    pub objects: BTreeSet<ObjectId>,
    /// 受影响的文档区域（几何 dirty 或结构闭包的 bbox 并集）。
    pub bbox: Option<Bbox>,
    /// 触发原因（可观测性）。
    pub reason: String,
}

impl DirtySet {
    /// 无变化。
    pub fn none() -> Self {
        Self {
            kind: DirtyKind::None,
            objects: BTreeSet::new(),
            bbox: None,
            reason: "no-op".to_owned(),
        }
    }

    /// 全量失效。
    pub fn full(reason: impl Into<String>) -> Self {
        Self {
            kind: DirtyKind::Full,
            objects: BTreeSet::new(),
            bbox: None,
            reason: reason.into(),
        }
    }

    /// 几何 dirty。
    pub fn geometry(bbox: Bbox, reason: impl Into<String>) -> Self {
        Self {
            kind: DirtyKind::Geometry,
            objects: BTreeSet::new(),
            bbox: Some(bbox),
            reason: reason.into(),
        }
    }

    /// 结构 dirty（对象闭包 + bbox）。
    pub fn structure(
        objects: BTreeSet<ObjectId>,
        bbox: Option<Bbox>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            kind: DirtyKind::Structure,
            objects,
            bbox,
            reason: reason.into(),
        }
    }

    /// 是否全量失效。
    pub fn is_full(&self) -> bool {
        self.kind == DirtyKind::Full
    }

    /// 是否无需失效。
    pub fn is_none(&self) -> bool {
        self.kind == DirtyKind::None
    }

    /// 合并两个 dirty（取更保守者）。
    pub fn union(self, other: DirtySet) -> DirtySet {
        let kind = match (self.kind, other.kind) {
            (DirtyKind::Full, _) | (_, DirtyKind::Full) => DirtyKind::Full,
            (DirtyKind::Structure, _) | (_, DirtyKind::Structure) => DirtyKind::Structure,
            (DirtyKind::Geometry, _) | (_, DirtyKind::Geometry) => DirtyKind::Geometry,
            _ => DirtyKind::None,
        };
        let mut objects = self.objects;
        objects.extend(other.objects);
        let bbox = match (self.bbox, other.bbox) {
            (Some(a), Some(b)) => Some(a.union(&b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        DirtySet {
            kind: if kind == DirtyKind::Full {
                DirtyKind::Full
            } else {
                kind
            },
            objects,
            bbox,
            reason: format!("{} + {}", self.reason, other.reason),
        }
    }

    /// 覆盖整个文档（保守失效）。
    pub fn whole_document(state: &DocumentState, reason: impl Into<String>) -> Self {
        Self {
            kind: DirtyKind::Full,
            objects: BTreeSet::new(),
            bbox: Some(Bbox::new(0.0, 0.0, state.width as f64, state.height as f64)),
            reason: reason.into(),
        }
    }
}

/// 需要结构 dirty 传播的属性键（`set_property`）。
const STRUCTURE_PROPERTIES: [&str; 6] = [
    "z_index", "visible", "layer_id", "mask_id", "style_id", "type",
];

/// 仅几何影响的原子类型。
const GEOMETRY_KINDS: [AtomKind; 8] = [
    AtomKind::DrawStroke,
    AtomKind::DrawShape,
    AtomKind::DrawText,
    AtomKind::Fill,
    AtomKind::Erase,
    AtomKind::Retouch,
    AtomKind::Liquify,
    AtomKind::ImportImage,
];

/// 依据原子规划失效范围（设计文档 6.6）。
///
/// `previous` 为变更前的状态，用于计算被删除对象的包围盒；
/// 传 `None` 时对「目标已消失」的情况保守返回全量失效。
pub fn plan_dirty(
    state: &DocumentState,
    previous: Option<&DocumentState>,
    atom: &Atom,
) -> DirtySet {
    match atom.kind {
        AtomKind::Comment
        | AtomKind::Suggest
        | AtomKind::AcceptSuggestion
        | AtomKind::RejectSuggestion
        | AtomKind::Tag
        | AtomKind::CreateCheckpoint
        | AtomKind::Checkpoint => DirtySet::none(),
        AtomKind::DeclareHead => {
            DirtySet::full("declare_head 求值起点跳变，全量 tile 失效（5.5/6.5）")
        }
        AtomKind::Revert | AtomKind::Reapply => DirtySet::full(format!(
            "{} 改变有效集，按结构 dirty 保守全量失效",
            atom.kind
        )),
        AtomKind::CreateDocument => DirtySet::full("create_document"),
        AtomKind::CreateLayer | AtomKind::ReorderLayers => {
            DirtySet::whole_document(state, format!("{} 改变图层结构", atom.kind))
        }
        AtomKind::CreateSelection | AtomKind::CreateMask | AtomKind::CreateStyle => {
            DirtySet::none()
        }
        AtomKind::Tombstone => dirty_for_tombstone(state, previous, atom),
        AtomKind::SetProperty => dirty_for_property(state, previous, atom),
        AtomKind::Move | AtomKind::Transform | AtomKind::Supersede => {
            dirty_for_object(state, previous, atom, "结构/几何修改")
        }
        kind if GEOMETRY_KINDS.contains(&kind) => {
            if atom.object_id().is_some() {
                dirty_for_object(state, previous, atom, "几何修改")
            } else {
                dirty_for_layer(state, atom.layer_id(), format!("{kind} 影响整层"))
            }
        }
        _ => DirtySet::none(),
    }
}

/// 结合日志规划失效范围：`revert` / `reapply` 会尽量归约到目标原子的 dirty。
pub fn plan_dirty_with_log(
    state: &DocumentState,
    previous: Option<&DocumentState>,
    log: &AtomLog,
    atom: &Atom,
) -> DirtySet {
    match atom.kind {
        AtomKind::Revert | AtomKind::Reapply => {
            let Some(target_id) = atom.target_atom() else {
                return DirtySet::full("revert/reapply 缺少 target");
            };
            let Some(target) = log.get(target_id) else {
                return DirtySet::full(format!("revert 目标 {target_id} 不在日志中"));
            };
            if target.kind == AtomKind::DeclareHead {
                return DirtySet::full("撤销 declare_head 起点变化，全量失效");
            }
            let inner = plan_dirty_with_log(state, previous, log, target);
            // 撤销/恢复都会改变有效集：放大为结构 dirty，但保留目标的 bbox。
            match inner.kind {
                DirtyKind::None => DirtySet::full(format!("撤销 {} 无几何线索", target.kind)),
                DirtyKind::Geometry => {
                    DirtySet::structure(BTreeSet::new(), inner.bbox, "撤销几何修改")
                }
                DirtyKind::Structure => {
                    DirtySet::structure(inner.objects, inner.bbox, "撤销结构修改")
                }
                DirtyKind::Full => DirtySet::full("撤销目标本身全量失效"),
            }
        }
        _ => plan_dirty(state, previous, atom),
    }
}

fn dirty_for_object(
    state: &DocumentState,
    previous: Option<&DocumentState>,
    atom: &Atom,
    reason: impl Into<String>,
) -> DirtySet {
    let reason = reason.into();
    let Some(object_id) = atom.object_id() else {
        return dirty_for_layer(state, atom.layer_id(), reason);
    };
    if let Some(object) = state.objects.get(object_id) {
        if let Some(bbox) = object_bbox(object) {
            if bbox.w > 0.0 && bbox.h > 0.0 {
                return DirtySet::geometry(bbox, format!("{reason}: {object_id}"));
            }
        }
        // 对象存在但包围盒未知（文本/实例等）：整层失效。
        return dirty_for_layer(
            state,
            Some(object.layer_id.as_str()),
            format!("{reason}: {object_id} 包围盒未知"),
        );
    }
    // 对象已被删除：用变更前的包围盒。
    if let Some(previous) = previous {
        if let Some(object) = previous.objects.get(object_id) {
            if let Some(bbox) = object_bbox(object) {
                return DirtySet::geometry(bbox, format!("{reason}: {object_id} 已删除"));
            }
        }
    }
    DirtySet::whole_document(state, format!("{reason}: {object_id} 状态中不存在"))
}

fn dirty_for_layer(
    state: &DocumentState,
    layer_id: Option<&str>,
    reason: impl Into<String>,
) -> DirtySet {
    let reason = reason.into();
    let Some(layer_id) = layer_id else {
        return DirtySet::whole_document(state, reason);
    };
    let objects: BTreeSet<ObjectId> = state
        .objects_in_layer(layer_id)
        .iter()
        .map(|object| object.id.clone())
        .collect();
    let bbox = layer_bbox(state, layer_id);
    if objects.is_empty() && bbox.is_none() {
        DirtySet::none()
    } else {
        DirtySet::structure(objects, bbox, reason)
    }
}

fn dirty_for_tombstone(
    state: &DocumentState,
    previous: Option<&DocumentState>,
    atom: &Atom,
) -> DirtySet {
    if let Some(object_id) = atom.object_id() {
        // 删除对象会改变同层后续对象的 z 序与合成结果：升级为结构 dirty，
        // 闭包 = 该对象（变更前 bbox）+ 同层仍存活的对象。
        let layer_id = previous
            .and_then(|previous| previous.objects.get(object_id))
            .map(|object| object.layer_id.clone())
            .or_else(|| {
                state
                    .objects
                    .get(object_id)
                    .map(|object| object.layer_id.clone())
            });
        let mut objects: BTreeSet<ObjectId> = std::iter::once(object_id.to_owned()).collect();
        let mut bbox: Option<Bbox> = previous
            .and_then(|previous| previous.objects.get(object_id))
            .and_then(object_bbox)
            .or_else(|| state.objects.get(object_id).and_then(object_bbox));
        if let Some(layer_id) = &layer_id {
            for sibling in state.objects_in_layer(layer_id) {
                objects.insert(sibling.id.clone());
                if let Some(candidate) = object_bbox(sibling) {
                    bbox = Some(match bbox {
                        Some(current) => current.union(&candidate),
                        None => candidate,
                    });
                }
            }
            if let Some(layer_bounds) = layer_bbox(state, layer_id) {
                bbox = Some(match bbox {
                    Some(current) => current.union(&layer_bounds),
                    None => layer_bounds,
                });
            }
        }
        DirtySet::structure(objects, bbox, "tombstone 影响整层")
    } else if let Some(layer_id) = atom.layer_id() {
        // 图层 tombstone 会级联删除后代与其中对象。
        let dirty = dirty_for_layer(state, Some(layer_id), "图层 tombstone".to_owned());
        if let Some(previous) = previous {
            let objects: BTreeSet<ObjectId> = previous
                .objects
                .values()
                .filter(|object| object.layer_id == layer_id)
                .map(|object| object.id.clone())
                .collect();
            let bbox = previous
                .objects
                .values()
                .filter(|object| object.layer_id == layer_id)
                .filter_map(object_bbox)
                .reduce(|a, b| a.union(&b));
            dirty.union(DirtySet::structure(
                objects,
                bbox,
                "图层 tombstone（变更前）",
            ))
        } else {
            dirty
        }
    } else {
        DirtySet::none()
    }
}

fn dirty_for_property(
    state: &DocumentState,
    previous: Option<&DocumentState>,
    atom: &Atom,
) -> DirtySet {
    let key = atom
        .payload
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let structural = STRUCTURE_PROPERTIES.contains(&key);
    if !structural {
        // 例如 opacity / blend_mode / 自定义属性：按受影响实体处理。
        return if atom.object_id().is_some() {
            dirty_for_object(state, previous, atom, "属性修改")
        } else {
            dirty_for_layer(state, atom.layer_id(), format!("属性修改 {key}"))
        };
    }
    // 图层归属变更会让对象在两个图层间移动：两侧都要重算，保守全量失效。
    if key == "layer_id" && atom.object_id().is_some() {
        return DirtySet::whole_document(state, "图层归属变更 layer_id");
    }
    match atom.object_id() {
        Some(object_id) => {
            let seeds: BTreeSet<ObjectId> = std::iter::once(object_id.to_owned()).collect();
            let closure = structure_closure(state, &seeds);
            let mut bbox: Option<Bbox> = None;
            for id in &closure {
                let from_state = state.objects.get(id).and_then(object_bbox);
                let from_previous = previous
                    .and_then(|p| p.objects.get(id))
                    .and_then(object_bbox);
                if let Some(candidate) = from_state.or(from_previous) {
                    bbox = Some(match bbox {
                        Some(current) => current.union(&candidate),
                        None => candidate,
                    });
                }
            }
            if bbox.is_none() {
                return DirtySet::whole_document(state, format!("结构属性 {key} 无几何线索"));
            }
            DirtySet::structure(closure, bbox, format!("结构属性 {key}"))
        }
        None => {
            let layer_id = atom.layer_id();
            let mut dirty = dirty_for_layer(state, layer_id, format!("结构属性 {key}"));
            // 图层归属/父链变化可能影响后代图层。
            if key == "parent_id" || key == "layer_id" {
                dirty = DirtySet::whole_document(state, format!("图层归属变更 {key}"));
            }
            dirty
        }
    }
}

/// 依赖图传播闭包（设计文档 6.6）：从种子对象出发，收集所有可能受其影响的对象。
///
/// 闭包包含：
/// - 种子对象自身；
/// - 同图层中 `z_index` 不小于种子的对象（其上方的调整/滤镜对象会作用于它）；
/// - 以种子为 master 的实例；
/// - 成员引用种子的对象组；
/// - 实例/组的 master 对象（反向：实例的渲染结果依赖 master）。
pub fn structure_closure(state: &DocumentState, seeds: &BTreeSet<ObjectId>) -> BTreeSet<ObjectId> {
    let mut closure = seeds.clone();
    let mut queue: VecDeque<ObjectId> = seeds.iter().cloned().collect();

    // 预索引：实例 → master、组 → 成员。
    let mut instances_of: Vec<(ObjectId, ObjectId)> = Vec::new();
    let mut groups_of: Vec<(ObjectId, ObjectId)> = Vec::new();
    for object in state.objects.values() {
        if object.is_deleted() {
            continue;
        }
        if let Some(master) = object
            .data
            .get("master_ref")
            .and_then(|value| value.get("object_id"))
            .and_then(Value::as_str)
        {
            instances_of.push((object.id.clone(), master.to_owned()));
        }
        if let Some(members) = object.data.get("members").and_then(Value::as_array) {
            for member in members {
                if let Some(member_id) = member
                    .get("object_id")
                    .and_then(Value::as_str)
                    .or_else(|| member.as_str())
                {
                    groups_of.push((object.id.clone(), member_id.to_owned()));
                }
            }
        }
    }

    while let Some(current) = queue.pop_front() {
        let Some(object) = state.objects.get(&current) else {
            continue;
        };
        let mut affected: Vec<ObjectId> = Vec::new();

        // 1) 同层中位于其上的对象（调整/滤镜会作用于下方所有内容）。
        for sibling in state.objects_in_layer(&object.layer_id) {
            if sibling.z_index >= object.z_index {
                affected.push(sibling.id.clone());
            }
        }
        // 2) 以当前对象为 master 的实例。
        for (instance, master) in &instances_of {
            if master == &current {
                affected.push(instance.clone());
            }
        }
        // 3) 引用当前对象的组。
        for (group, member) in &groups_of {
            if member == &current {
                affected.push(group.clone());
            }
        }
        // 4) 当前对象如果是实例，其 master 改变也会影响自身（反向传播）。
        if let Some(master) = object
            .data
            .get("master_ref")
            .and_then(|value| value.get("object_id"))
            .and_then(Value::as_str)
        {
            affected.push(master.to_owned());
        }

        for id in affected {
            if state.objects.contains_key(&id) && closure.insert(id.clone()) {
                queue.push_back(id);
            }
        }
    }
    closure
}

/// 把 dirty 集合转换为需要失效的 tile（设计文档 6.6）。
///
/// 全量失效返回网格内全部 tile。
pub fn invalidated_tiles(grid: &TileGrid, state: &DocumentState, dirty: &DirtySet) -> Vec<TileKey> {
    match dirty.kind {
        DirtyKind::None => Vec::new(),
        DirtyKind::Full => grid.all_keys(),
        _ => {
            let bbox =
                dirty
                    .bbox
                    .unwrap_or(Bbox::new(0.0, 0.0, state.width as f64, state.height as f64));
            grid.keys_for_bbox(&bbox)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use yanshi_core::{Layer, LayerType, Object, ObjectType, Transform};

    fn state_with_layers() -> DocumentState {
        let mut state = DocumentState::empty();
        state.width = 128;
        state.height = 128;
        for (index, id) in ["layer_1", "layer_2"].iter().enumerate() {
            state.layers.insert(
                (*id).to_owned(),
                Layer {
                    id: (*id).to_owned(),
                    name: (*id).to_owned(),
                    layer_type: LayerType::Raster,
                    parent_id: None,
                    z_index: index as i64,
                    blend_mode: "normal".to_owned(),
                    opacity: 1.0,
                    visible: true,
                    locked: false,
                    alpha_lock: false,
                    clipping_mask: false,
                    mask_id: None,
                    transform: Transform::IDENTITY,
                    medium: None,
                    style: None,
                    metadata: Value::Null,
                    blobs: Vec::new(),
                    created_by: "a1".to_owned(),
                    updated_by: None,
                    deleted_by: None,
                },
            );
        }
        state
    }

    fn shape(id: &str, layer: &str, z: i64, x: f64, y: f64, w: f64, h: f64) -> Object {
        Object {
            id: id.to_owned(),
            layer_id: layer.to_owned(),
            object_type: ObjectType::Shape,
            z_index: z,
            visible: true,
            locked: false,
            metadata: Value::Null,
            transform: Transform::IDENTITY,
            style: None,
            versions: vec!["a1".to_owned()],
            current_version: Some("a1".to_owned()),
            created_by: "a1".to_owned(),
            deleted_by: None,
            data: json!({"geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": w, "h": h}}}),
            blobs: Vec::new(),
        }
    }

    fn stroke_atom(kind: AtomKind, payload: Value) -> Atom {
        Atom::new(kind, "human:1", "s", payload).with_id("atom_x")
    }

    #[test]
    fn geometry_change_invalidates_only_object_bbox() {
        let mut state = state_with_layers();
        state.objects.insert(
            "obj_a".to_owned(),
            shape("obj_a", "layer_1", 0, 10.0, 10.0, 20.0, 20.0),
        );
        state.objects.insert(
            "obj_b".to_owned(),
            shape("obj_b", "layer_1", 1, 100.0, 100.0, 10.0, 10.0),
        );
        let atom = stroke_atom(
            AtomKind::DrawStroke,
            json!({"object_id": "obj_a", "layer_id": "layer_1", "data": {"points": [[10.0, 10.0]]}}),
        );
        let dirty = plan_dirty(&state, None, &atom);
        assert_eq!(dirty.kind, DirtyKind::Geometry);
        assert!(dirty.objects.is_empty());
        assert_eq!(dirty.bbox, Some(Bbox::new(10.0, 10.0, 20.0, 20.0)));

        let grid = TileGrid::new(32, 128, 128).unwrap();
        let tiles = invalidated_tiles(&grid, &state, &dirty);
        assert_eq!(tiles, vec![TileKey::new(0, 0)]);
    }

    #[test]
    fn property_change_propagates_through_structure_closure() {
        let mut state = state_with_layers();
        state.objects.insert(
            "obj_a".to_owned(),
            shape("obj_a", "layer_1", 0, 0.0, 0.0, 10.0, 10.0),
        );
        state.objects.insert(
            "obj_b".to_owned(),
            shape("obj_b", "layer_1", 2, 40.0, 40.0, 10.0, 10.0),
        );
        let atom = stroke_atom(
            AtomKind::SetProperty,
            json!({"object_id": "obj_a", "key": "z_index", "value": 5}),
        );
        let dirty = plan_dirty(&state, None, &atom);
        assert_eq!(dirty.kind, DirtyKind::Structure);
        // 闭包包含同层中 z_index >= 0 的所有对象。
        assert!(dirty.objects.contains("obj_a"));
        assert!(dirty.objects.contains("obj_b"));
        let bbox = dirty.bbox.unwrap();
        assert_eq!(bbox, Bbox::new(0.0, 0.0, 50.0, 50.0));
    }

    #[test]
    fn instance_and_group_references_enter_closure() {
        let mut state = state_with_layers();
        state.objects.insert(
            "master".to_owned(),
            shape("master", "layer_1", 0, 0.0, 0.0, 10.0, 10.0),
        );
        let mut instance = shape("inst", "layer_2", 0, 0.0, 0.0, 10.0, 10.0);
        instance.object_type = ObjectType::Instance;
        instance.data = json!({"master_ref": {"object_id": "master", "layer_id": "layer_1"}});
        state.objects.insert("inst".to_owned(), instance);
        let mut group = shape("group", "layer_2", 1, 0.0, 0.0, 10.0, 10.0);
        group.object_type = ObjectType::Group;
        group.data = json!({"members": [{"object_id": "master"}]});
        state.objects.insert("group".to_owned(), group);

        let seeds: BTreeSet<ObjectId> = std::iter::once("master".to_owned()).collect();
        let closure = structure_closure(&state, &seeds);
        assert!(closure.contains("master"));
        assert!(closure.contains("inst"), "实例依赖 master");
        assert!(closure.contains("group"), "组成员依赖 master");
    }

    #[test]
    fn declare_head_and_history_atoms_invalidate_everything() {
        let state = state_with_layers();
        let grid = TileGrid::new(32, 128, 128).unwrap();
        for kind in [AtomKind::DeclareHead, AtomKind::Revert, AtomKind::Reapply] {
            let atom = stroke_atom(
                kind,
                json!({"base": {"type": "atom", "id": "a1"}, "target": "a1"}),
            );
            let dirty = plan_dirty(&state, None, &atom);
            assert!(dirty.is_full(), "{kind} 应全量失效");
            assert_eq!(
                invalidated_tiles(&grid, &state, &dirty).len(),
                grid.tile_count()
            );
        }
    }

    #[test]
    fn tombstone_escalates_to_full_layer_structure() {
        let mut state = state_with_layers();
        let mut previous = state.clone();
        previous.objects.insert(
            "obj_a".to_owned(),
            shape("obj_a", "layer_1", 0, 5.0, 5.0, 10.0, 10.0),
        );
        let mut deleted = shape("obj_a", "layer_1", 0, 5.0, 5.0, 10.0, 10.0);
        deleted.deleted_by = Some("atom_x".to_owned());
        state.objects.insert("obj_a".to_owned(), deleted);

        let atom = stroke_atom(AtomKind::Tombstone, json!({"object_id": "obj_a"}));
        let dirty = plan_dirty(&state, Some(&previous), &atom);
        assert_eq!(dirty.kind, DirtyKind::Structure);
        assert!(dirty.bbox.is_some());
    }

    #[test]
    fn collab_and_annotation_atoms_are_no_ops() {
        let state = state_with_layers();
        for kind in [
            AtomKind::Comment,
            AtomKind::Suggest,
            AtomKind::Tag,
            AtomKind::CreateMask,
        ] {
            let atom = stroke_atom(kind, json!({"target_atom": "a1"}));
            assert!(plan_dirty(&state, None, &atom).is_none(), "{kind} 不应失效");
        }
    }

    #[test]
    fn layer_membership_change_invalidates_whole_document() {
        let mut state = state_with_layers();
        state.objects.insert(
            "obj_a".to_owned(),
            shape("obj_a", "layer_1", 0, 0.0, 0.0, 4.0, 4.0),
        );
        let atom = stroke_atom(
            AtomKind::SetProperty,
            json!({"object_id": "obj_a", "key": "layer_id", "value": "layer_2"}),
        );
        let dirty = plan_dirty(&state, None, &atom);
        assert_eq!(dirty.kind, DirtyKind::Full, "图层归属变更保守全量失效");
        let bbox = dirty.bbox.unwrap();
        assert_eq!((bbox.w, bbox.h), (128.0, 128.0));
    }

    #[test]
    fn dirty_union_takes_most_conservative() {
        let geometry = DirtySet::geometry(Bbox::new(0.0, 0.0, 10.0, 10.0), "g");
        let full = DirtySet::full("f");
        assert!(geometry.clone().union(full).is_full());
        let other = DirtySet::geometry(Bbox::new(5.0, 5.0, 10.0, 10.0), "g2");
        let merged = geometry.union(other);
        assert_eq!(merged.bbox, Some(Bbox::new(0.0, 0.0, 15.0, 15.0)));
        assert!(DirtySet::none().union(DirtySet::none()).is_none());
    }
}
