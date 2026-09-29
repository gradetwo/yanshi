//! 文档状态模型（设计文档 4.1 – 4.5）。
//!
//! 状态是折叠原子日志的结果，是内存里的缓存；历史才是资产。
//! 本模块只描述状态形状与派生索引，不负责求值（见 [`crate::fold`]）与持久化。

use crate::atom::{AtomId, BlobHash};
use crate::ids::{
    CheckpointId, DocId, LayerId, MaskId, ObjectId, SelectionId, Seq, SnapshotId, StyleId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// 仿射变换（`matrix` 为 2×3 仿射矩阵的 6 个分量，`pivot` 为旋转/缩放枢轴）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// `[a, b, c, d, e, f]`，即 `x' = a·x + c·y + e`，`y' = b·x + d·y + f`。
    pub matrix: [f64; 6],
    /// 变换枢轴。
    pub pivot: [f64; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            pivot: [0.0, 0.0],
        }
    }
}

impl Transform {
    /// 单位变换。
    pub const IDENTITY: Self = Self {
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        pivot: [0.0, 0.0],
    };

    /// 是否为单位变换。
    pub fn is_identity(&self) -> bool {
        self.matrix == Self::IDENTITY.matrix
    }
}

/// 图层类型（设计文档 4.1）。图层只做容器，调整/滤镜/文本/修图全部对象化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerType {
    /// 光栅图层。
    Raster,
    /// 矢量图层。
    Vector,
    /// 图层组（图层层级结构，与对象级 `Group` 语义不同）。
    LayerGroup,
    /// 蒙版图层。
    Mask,
}

/// 图层（设计文档 4.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    /// 图层 id。
    pub id: LayerId,
    /// 名称。
    pub name: String,
    /// 图层类型。
    #[serde(rename = "type")]
    pub layer_type: LayerType,
    /// 父图层（图层组）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<LayerId>,
    /// z 序。
    #[serde(default)]
    pub z_index: i64,
    /// 混合模式。
    #[serde(default = "default_blend_mode")]
    pub blend_mode: String,
    /// 不透明度。
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    /// 可见性。
    #[serde(default = "default_true")]
    pub visible: bool,
    /// 锁定。
    #[serde(default)]
    pub locked: bool,
    /// alpha 锁定。
    #[serde(default)]
    pub alpha_lock: bool,
    /// 剪贴蒙版。
    #[serde(default)]
    pub clipping_mask: bool,
    /// 关联蒙版。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_id: Option<MaskId>,
    /// 变换。
    #[serde(default)]
    pub transform: Transform,
    /// 介质。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    /// 风格引用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// 元数据。
    #[serde(default)]
    pub metadata: Value,
    /// 本图层直接引用的 blob（活跃 Manifest 用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobHash>,
    /// 创建它的原子。
    pub created_by: AtomId,
    /// 最近修改它的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<AtomId>,
    /// 删除它的原子（tombstone）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<AtomId>,
}

impl Layer {
    /// 是否已被 tombstone。
    pub fn is_deleted(&self) -> bool {
        self.deleted_by.is_some()
    }
}

/// 对象类型（设计文档 4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    /// 笔触。
    Stroke,
    /// 形状。
    Shape,
    /// 文本。
    Text,
    /// 调整。
    Adjustment,
    /// 滤镜。
    Filter,
    /// AI 语义工具输出的位图补丁。
    RasterPatch,
    /// 修图对象。
    Retouch,
    /// 液化对象。
    Liquify,
    /// 实例。
    Instance,
    /// 对象组。
    Group,
}

impl ObjectType {
    /// 全部对象类型。
    pub const ALL: [ObjectType; 10] = [
        Self::Stroke,
        Self::Shape,
        Self::Text,
        Self::Adjustment,
        Self::Filter,
        Self::RasterPatch,
        Self::Retouch,
        Self::Liquify,
        Self::Instance,
        Self::Group,
    ];
}

/// 对象（设计文档 4.2）。
///
/// `versions` 与 `current_version` 记录该对象的有效原子链；
/// 类型专属字段（points、geometry、text、params 等）存放在 `data` 中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    /// 对象 id。
    pub id: ObjectId,
    /// 所属图层（权威归属，设计文档 4.1）。
    pub layer_id: LayerId,
    /// 对象类型。
    #[serde(rename = "type")]
    pub object_type: ObjectType,
    /// 图层内 z 序。
    #[serde(default)]
    pub z_index: i64,
    /// 可见性。
    #[serde(default = "default_true")]
    pub visible: bool,
    /// 锁定。
    #[serde(default)]
    pub locked: bool,
    /// 元数据。
    #[serde(default)]
    pub metadata: Value,
    /// 变换。
    #[serde(default)]
    pub transform: Transform,
    /// 风格引用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// 对象版本链（按 seq 升序）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<AtomId>,
    /// 当前有效版本原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_version: Option<AtomId>,
    /// 创建它的原子。
    pub created_by: AtomId,
    /// 删除它的原子（tombstone）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<AtomId>,
    /// 类型专属数据。
    #[serde(default)]
    pub data: Value,
    /// 该对象当前引用到的 blob（活跃 Manifest 用）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobHash>,
}

impl Object {
    /// 是否已被 tombstone。
    pub fn is_deleted(&self) -> bool {
        self.deleted_by.is_some()
    }

    /// 是否存活（未删除）。
    pub fn is_alive(&self) -> bool {
        !self.is_deleted()
    }
}

/// 选区 / 蒙版（设计文档 4.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    /// id。
    pub id: SelectionId,
    /// 几何描述。
    #[serde(default)]
    pub shape: Value,
    /// 羽化半径。
    #[serde(default)]
    pub feather: f64,
    /// 组合模式（new/add/subtract/intersect）。
    #[serde(default = "default_new_mode")]
    pub mode: String,
    /// 反选。
    #[serde(default)]
    pub invert: bool,
    /// 关联图层。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_layer: Option<LayerId>,
    /// 是否做过边缘细化。
    #[serde(default)]
    pub refined_edges: bool,
    /// 引用的 blob。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobHash>,
    /// 创建它的原子。
    pub created_by: AtomId,
    /// 删除它的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<AtomId>,
}

impl Selection {
    /// 是否已被 tombstone。
    pub fn is_deleted(&self) -> bool {
        self.deleted_by.is_some()
    }
}

/// 蒙版（设计文档 4.4），字段与选区一致，语义上可被图层/对象引用。
pub type Mask = Selection;

/// 风格（设计文档 11.2）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// id。
    pub id: StyleId,
    /// 名称。
    pub name: String,
    /// 分类。
    #[serde(default)]
    pub category: String,
    /// 父风格（继承）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_style: Option<StyleId>,
    /// 笔刷集合。
    #[serde(default)]
    pub brushes: Value,
    /// 调色板。
    #[serde(default)]
    pub palette: Value,
    /// 纹理。
    #[serde(default)]
    pub textures: Value,
    /// 渲染参数。
    #[serde(default)]
    pub render_params: Value,
    /// 随机策略。
    #[serde(default)]
    pub seed_policy: Value,
    /// 引用的 blob。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobHash>,
    /// 创建它的原子。
    pub created_by: AtomId,
    /// 删除它的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_by: Option<AtomId>,
}

impl Style {
    /// 是否已被 tombstone。
    pub fn is_deleted(&self) -> bool {
        self.deleted_by.is_some()
    }
}

/// 检查点（设计文档 4.5）：用户语义，轻量元数据原子，锚定 `anchor_seq`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// id。
    pub id: CheckpointId,
    /// 名称。
    pub name: String,
    /// 创建者。
    pub created_by: String,
    /// 说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 创建时的 head seq。
    pub anchor_seq: Seq,
    /// 可选关联快照，保证 O(1) 恢复；用户显式创建者不参与 LRU 清理。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
}

/// 对象引用（设计文档 4.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectRef {
    /// 目标对象。
    pub object_id: ObjectId,
    /// 目标图层。
    pub layer_id: LayerId,
    /// 局部变换。
    #[serde(default)]
    pub local_transform: Transform,
    /// 覆盖。
    #[serde(rename = "override", default, skip_serializing_if = "Option::is_none")]
    pub override_: Option<Value>,
    /// 同步策略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync_policy: Option<Value>,
}

/// `declare_head` 与快照共用的统一引用类型（设计文档 4.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "snake_case")]
pub enum HeadBase {
    /// 引用某个原子时刻的 state@seq。
    Atom(AtomId),
    /// 引用某个检查点的 anchor_seq。
    Checkpoint(CheckpointId),
}

/// 求值起点跳变记录（设计文档 5.5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclareHead {
    /// 触发它的原子。
    pub atom_id: AtomId,
    /// 该原子的 seq。
    pub seq: Seq,
    /// 求值起点引用。
    pub base: HeadBase,
    /// 可选原因（revert_to / restore_checkpoint 等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 文档状态：折叠原子日志的结果（设计文档 4.1 – 4.5）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentState {
    /// 文档 id（`create_document` 前为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_id: Option<DocId>,
    /// 画布宽。
    #[serde(default)]
    pub width: u32,
    /// 画布高。
    #[serde(default)]
    pub height: u32,
    /// 色彩空间。
    #[serde(default = "default_color_space")]
    pub color_space: String,
    /// 背景。
    #[serde(default)]
    pub background: Value,
    /// 介质。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    /// 文档风格。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<StyleId>,
    /// 图层。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<LayerId, Layer>,
    /// 对象。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objects: BTreeMap<ObjectId, Object>,
    /// 选区。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub selections: BTreeMap<SelectionId, Selection>,
    /// 蒙版。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub masks: BTreeMap<MaskId, Selection>,
    /// 风格。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub styles: BTreeMap<StyleId, Style>,
    /// 检查点。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub checkpoints: BTreeMap<CheckpointId, Checkpoint>,
    /// 已折叠到的权威 seq。
    #[serde(default)]
    pub head_seq: Seq,
    /// head 原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_atom: Option<AtomId>,
    /// 当前求值起点（最近一次 `declare_head`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declare_head: Option<DeclareHead>,
}

fn default_blend_mode() -> String {
    "normal".to_owned()
}
fn default_opacity() -> f64 {
    1.0
}
fn default_true() -> bool {
    true
}
fn default_new_mode() -> String {
    "new".to_owned()
}
fn default_color_space() -> String {
    "srgb".to_owned()
}

impl Default for DocumentState {
    fn default() -> Self {
        Self::empty()
    }
}

impl DocumentState {
    /// 空白初始文档：既无 `declare_head` 时的折叠起点，也是 `create_document` 的期待基底。
    pub fn empty() -> Self {
        Self {
            doc_id: None,
            width: 0,
            height: 0,
            color_space: default_color_space(),
            background: Value::Null,
            medium: None,
            style: None,
            layers: BTreeMap::new(),
            objects: BTreeMap::new(),
            selections: BTreeMap::new(),
            masks: BTreeMap::new(),
            styles: BTreeMap::new(),
            checkpoints: BTreeMap::new(),
            head_seq: 0,
            head_atom: None,
            declare_head: None,
        }
    }

    /// 是否没有任何文档内容（`create_document` 尚未生效）。
    pub fn is_blank(&self) -> bool {
        self.doc_id.is_none() && self.layers.is_empty() && self.objects.is_empty()
    }

    /// 当前求值起点 seq：有 `declare_head` 时为其 seq，否则为 0。
    pub fn eval_origin_seq(&self) -> Seq {
        self.declare_head.as_ref().map(|h| h.seq).unwrap_or(0)
    }

    /// 存活图层，按 z 序升序。
    pub fn alive_layers(&self) -> Vec<&Layer> {
        let mut layers: Vec<&Layer> = self.layers.values().filter(|l| !l.is_deleted()).collect();
        layers.sort_by_key(|l| (l.z_index, l.id.clone()));
        layers
    }

    /// 存活对象，按 (图层, z 序, id) 升序。
    pub fn alive_objects(&self) -> Vec<&Object> {
        let mut objects: Vec<&Object> = self.objects.values().filter(|o| !o.is_deleted()).collect();
        objects.sort_by_key(|o| (o.layer_id.clone(), o.z_index, o.id.clone()));
        objects
    }

    /// 某图层内的存活对象（派生索引；权威引用是 `Object.layer_id`）。
    pub fn objects_in_layer(&self, layer_id: &str) -> Vec<&Object> {
        let mut objects: Vec<&Object> = self
            .objects
            .values()
            .filter(|o| o.is_alive() && o.layer_id == layer_id)
            .collect();
        objects.sort_by_key(|o| (o.z_index, o.id.clone()));
        objects
    }

    /// 图层是否存活。
    pub fn layer_alive(&self, layer_id: &str) -> bool {
        self.layers.get(layer_id).is_some_and(|l| !l.is_deleted())
    }

    /// 对象是否存活。
    pub fn object_alive(&self, object_id: &str) -> bool {
        self.objects.get(object_id).is_some_and(|o| !o.is_deleted())
    }

    /// 活跃 blob Manifest：当前折叠状态引用的全部 blob（设计文档 6.3）。
    ///
    /// Manifest 的角色是**活跃集标记与冷热迁移依据，不是 GC 根集**。
    pub fn active_blob_manifest(&self) -> BTreeSet<BlobHash> {
        let mut manifest = BTreeSet::new();
        for layer in self.layers.values().filter(|l| !l.is_deleted()) {
            manifest.extend(layer.blobs.iter().cloned());
        }
        for object in self.objects.values().filter(|o| !o.is_deleted()) {
            manifest.extend(object.blobs.iter().cloned());
        }
        for selection in self.selections.values().filter(|s| !s.is_deleted()) {
            manifest.extend(selection.blobs.iter().cloned());
        }
        for mask in self.masks.values().filter(|m| !m.is_deleted()) {
            manifest.extend(mask.blobs.iter().cloned());
        }
        for style in self.styles.values().filter(|s| !s.is_deleted()) {
            manifest.extend(style.blobs.iter().cloned());
        }
        manifest
    }

    /// 结构不变量检查：折叠结果中不得存在指向已失效实体的引用（设计文档 5.3）。
    ///
    /// 返回全部违规项，属性测试断言其为空。
    pub fn violations(&self) -> Vec<Violation> {
        let mut violations = Vec::new();
        for layer in self.layers.values() {
            if let Some(parent) = &layer.parent_id {
                match self.layers.get(parent) {
                    None => violations.push(Violation::MissingLayerRef {
                        owner: layer.id.clone(),
                        missing: parent.clone(),
                    }),
                    Some(p) if p.is_deleted() => violations.push(Violation::DeletedLayerRef {
                        owner: layer.id.clone(),
                        missing: parent.clone(),
                    }),
                    Some(_) => {}
                }
            }
            if layer.parent_id.as_deref() == Some(layer.id.as_str()) {
                violations.push(Violation::LayerCycle(layer.id.clone()));
            }
            if let Some(mask_id) = &layer.mask_id {
                if !self.masks.get(mask_id).is_some_and(|m| !m.is_deleted()) {
                    violations.push(Violation::MissingMaskRef {
                        owner: layer.id.clone(),
                        missing: mask_id.clone(),
                    });
                }
            }
            if let Some(style_id) = &layer.style {
                if !self.styles.get(style_id).is_some_and(|s| !s.is_deleted()) {
                    violations.push(Violation::MissingStyleRef {
                        owner: layer.id.clone(),
                        missing: style_id.clone(),
                    });
                }
            }
        }
        // 图层组父链环检测。
        for layer in self.layers.values() {
            let mut seen = BTreeSet::new();
            let mut cursor = layer.parent_id.clone();
            while let Some(parent) = cursor {
                if !seen.insert(parent.clone()) {
                    violations.push(Violation::LayerCycle(layer.id.clone()));
                    break;
                }
                cursor = self.layers.get(&parent).and_then(|l| l.parent_id.clone());
            }
        }
        for object in self.objects.values() {
            if object.is_alive() && !self.layer_alive(&object.layer_id) {
                violations.push(Violation::MissingLayerRef {
                    owner: object.id.clone(),
                    missing: object.layer_id.clone(),
                });
            }
            if let Some(current) = &object.current_version {
                if !object.versions.contains(current) {
                    violations.push(Violation::VersionNotInChain {
                        object: object.id.clone(),
                        atom: current.clone(),
                    });
                }
            }
            if object.current_version.is_none() && object.versions.is_empty() {
                violations.push(Violation::EmptyVersionChain(object.id.clone()));
            }
        }
        for mask in self.masks.values().filter(|m| !m.is_deleted()) {
            if let Some(layer) = &mask.linked_layer {
                if !self.layer_alive(layer) {
                    violations.push(Violation::MissingLayerRef {
                        owner: mask.id.clone(),
                        missing: layer.clone(),
                    });
                }
            }
        }
        for selection in self.selections.values().filter(|s| !s.is_deleted()) {
            if let Some(layer) = &selection.linked_layer {
                if !self.layer_alive(layer) {
                    violations.push(Violation::MissingLayerRef {
                        owner: selection.id.clone(),
                        missing: layer.clone(),
                    });
                }
            }
        }
        for checkpoint in self.checkpoints.values() {
            if checkpoint.anchor_seq > self.head_seq {
                violations.push(Violation::CheckpointAheadOfHead {
                    checkpoint: checkpoint.id.clone(),
                    anchor_seq: checkpoint.anchor_seq,
                    head_seq: self.head_seq,
                });
            }
        }
        violations
    }

    /// 结构不变量是否全部成立。
    pub fn is_consistent(&self) -> bool {
        self.violations().is_empty()
    }
}

/// 折叠结果中的违规项（设计文档 5.3「无孤儿引用」不变量的检查目标）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Violation {
    /// 引用了不存在或已删除的图层。
    MissingLayerRef {
        /// 引用者。
        owner: String,
        /// 缺失的图层。
        missing: LayerId,
    },
    /// 引用了已删除的图层。
    DeletedLayerRef {
        /// 引用者。
        owner: String,
        /// 已删除的图层。
        missing: LayerId,
    },
    /// 图层组父链成环。
    LayerCycle(LayerId),
    /// 引用了不存在或已删除的蒙版。
    MissingMaskRef {
        /// 引用者。
        owner: String,
        /// 缺失的蒙版。
        missing: MaskId,
    },
    /// 引用了不存在或已删除的风格。
    MissingStyleRef {
        /// 引用者。
        owner: String,
        /// 缺失的风格。
        missing: StyleId,
    },
    /// `current_version` 不在版本链中。
    VersionNotInChain {
        /// 对象。
        object: ObjectId,
        /// 版本原子。
        atom: AtomId,
    },
    /// 版本链为空：对象没有来源原子。
    EmptyVersionChain(ObjectId),
    /// 检查点锚定的 seq 超出 head。
    CheckpointAheadOfHead {
        /// 检查点。
        checkpoint: CheckpointId,
        /// 锚定 seq。
        anchor_seq: Seq,
        /// 当前 head seq。
        head_seq: Seq,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn layer(id: &str, z: i64) -> Layer {
        Layer {
            id: id.to_owned(),
            name: id.to_owned(),
            layer_type: LayerType::Raster,
            parent_id: None,
            z_index: z,
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
            created_by: format!("atom_{id}"),
            updated_by: None,
            deleted_by: None,
        }
    }

    fn object(id: &str, layer_id: &str, z: i64) -> Object {
        Object {
            id: id.to_owned(),
            layer_id: layer_id.to_owned(),
            object_type: ObjectType::Stroke,
            z_index: z,
            visible: true,
            locked: false,
            metadata: Value::Null,
            transform: Transform::IDENTITY,
            style: None,
            versions: vec![format!("atom_{id}")],
            current_version: Some(format!("atom_{id}")),
            created_by: format!("atom_{id}"),
            deleted_by: None,
            data: json!({}),
            blobs: Vec::new(),
        }
    }

    #[test]
    fn derived_indexes_are_sorted_and_filter_deleted() {
        let mut state = DocumentState::empty();
        state.layers.insert("l2".into(), layer("l2", 2));
        state.layers.insert("l1".into(), layer("l1", 1));
        state.layers.insert("l3".into(), layer("l3", 3));
        state.layers.get_mut("l3").unwrap().deleted_by = Some("atom_del".into());
        state.objects.insert("o2".into(), object("o2", "l1", 2));
        state.objects.insert("o1".into(), object("o1", "l1", 1));
        state.objects.insert("o3".into(), object("o3", "l2", 0));

        let layers: Vec<&str> = state.alive_layers().iter().map(|l| l.id.as_str()).collect();
        assert_eq!(layers, vec!["l1", "l2"]);
        let in_l1: Vec<&str> = state
            .objects_in_layer("l1")
            .iter()
            .map(|o| o.id.as_str())
            .collect();
        assert_eq!(in_l1, vec!["o1", "o2"]);
        assert_eq!(state.alive_objects().len(), 3);
        assert!(!state.layer_alive("l3"));
    }

    #[test]
    fn violations_detect_orphan_references() {
        let mut state = DocumentState::empty();
        state.layers.insert("l1".into(), layer("l1", 0));
        state.objects.insert("o1".into(), object("o1", "l1", 0));
        assert!(state.is_consistent());

        // 对象指向不存在的图层。
        state
            .objects
            .insert("o_bad".into(), object("o_bad", "l_missing", 1));
        let violations = state.violations();
        assert!(violations.iter().any(|v| matches!(
            v,
            Violation::MissingLayerRef { owner, missing }
                if owner == "o_bad" && missing == "l_missing"
        )));

        // 图层组父链成环。
        let mut child = layer("child", 1);
        child.parent_id = Some("parent".into());
        let mut parent = layer("parent", 2);
        parent.parent_id = Some("child".into());
        state.layers.insert("child".into(), child);
        state.layers.insert("parent".into(), parent);
        assert!(state
            .violations()
            .iter()
            .any(|v| matches!(v, Violation::LayerCycle(_))));
    }

    #[test]
    fn active_manifest_tracks_alive_entities_only() {
        let mut state = DocumentState::empty();
        let mut l1 = layer("l1", 0);
        let blob_a = BlobHash::from_bytes(b"a");
        l1.blobs = vec![blob_a.clone()];
        state.layers.insert("l1".into(), l1);

        let mut o1 = object("o1", "l1", 0);
        let blob_b = BlobHash::from_bytes(b"b");
        o1.blobs = vec![blob_b.clone()];
        state.objects.insert("o1".into(), o1);

        let mut o2 = object("o2", "l1", 1);
        o2.deleted_by = Some("atom_del".into());
        o2.blobs = vec![BlobHash::from_bytes(b"c")];
        state.objects.insert("o2".into(), o2);

        let manifest = state.active_blob_manifest();
        assert!(manifest.contains(&blob_a));
        assert!(manifest.contains(&blob_b));
        assert_eq!(manifest.len(), 2, "已 tombstone 对象不进活跃 Manifest");
    }

    #[test]
    fn declare_head_moves_eval_origin() {
        let mut state = DocumentState::empty();
        assert_eq!(state.eval_origin_seq(), 0);
        state.declare_head = Some(DeclareHead {
            atom_id: "atom_h".into(),
            seq: 42,
            base: HeadBase::Checkpoint("ckpt_1".into()),
            reason: Some("restore_checkpoint".into()),
        });
        assert_eq!(state.eval_origin_seq(), 42);
        let json = serde_json::to_value(HeadBase::Atom("atom_1".into())).unwrap();
        assert_eq!(json, json!({"type": "atom", "id": "atom_1"}));
        assert_eq!(
            serde_json::from_value::<HeadBase>(json).unwrap(),
            HeadBase::Atom("atom_1".into())
        );
    }
}
