//! 原子结构与原子类型（设计文档 5.1 / 5.2 / 6.3）。
//!
//! 原子是唯一的历史事实：只追加、不删除、内容不变。大二进制外置到 Blob CAS，
//! 原子只保存 `{blob_hash, size, mime_type}` 引用。

use crate::error::{ErrorCode, Result, YanshiError};
use crate::ids::{
    now_ms, ActorId, ChangesetId, CheckpointId, LayerId, MaskId, ObjectId, SelectionId, Seq,
    SessionId, StyleId, Ulid,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

/// 当前原子 schema 版本（设计文档 5.1 `schema_version`）。
pub const SCHEMA_VERSION: u32 = 1;

/// 未提交原子的占位 seq。服务端分配的权威 seq 从 1 开始。
pub const SEQ_UNASSIGNED: Seq = 0;

/// 超过该字节数的 payload 走 Blob CAS（设计文档 6.3）。
pub const CAS_THRESHOLD_BYTES: usize = 4096;

/// 原子标识（客户端生成的 ULID 字符串）。
pub use crate::ids::AtomId;

/// 内容寻址存储的 blob 哈希，形如 `sha256:<64 位小写十六进制>`。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobHash(String);

impl BlobHash {
    /// 计算字节内容的 SHA-256 哈希。
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let digest = Sha256::digest(bytes);
        let mut hex = String::with_capacity(7 + 64);
        hex.push_str("sha256:");
        for byte in digest {
            use fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
        }
        Self(hex)
    }

    /// 以字符串引用。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 去掉 `sha256:` 前缀的十六进制部分。
    pub fn hex(&self) -> &str {
        &self.0["sha256:".len()..]
    }
}

impl fmt::Display for BlobHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for BlobHash {
    type Err = YanshiError;

    fn from_str(s: &str) -> Result<Self> {
        let valid = s.len() == 71
            && s.starts_with("sha256:")
            && s[7..].bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if !valid {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                crate::error::ErrorContext::detail(format!("非法 blob hash: {s:?}")),
            ));
        }
        Ok(Self(s.to_owned()))
    }
}

impl TryFrom<String> for BlobHash {
    type Error = YanshiError;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<BlobHash> for String {
    fn from(value: BlobHash) -> Self {
        value.0
    }
}

/// 原子中记录的 blob 引用（设计文档 6.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobRef {
    /// 内容哈希。
    pub blob_hash: BlobHash,
    /// 字节数。
    pub size: u64,
    /// MIME 类型。
    pub mime_type: String,
}

/// 插件引用：`plugin_id + plugin_version` 随原子记录，升级不自动改变旧文档渲染（设计文档 5.1 / 11.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRef {
    /// 插件 id。
    pub id: String,
    /// 插件版本。
    pub version: String,
}

/// 原子声明的外部引用集合，用于 GC 根集闭包与提交时 precondition 校验。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refs {
    /// 引用的 blob。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blobs: Vec<BlobHash>,
    /// 引用的对象。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectId>,
    /// 引用的图层。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerId>,
    /// 引用的选区。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selections: Vec<SelectionId>,
    /// 引用的蒙版。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub masks: Vec<MaskId>,
    /// 引用的风格。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<StyleId>,
    /// 引用的原子（revert / reapply / declare_head 目标）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub atoms: Vec<AtomId>,
    /// 引用的检查点。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<CheckpointId>,
}

impl Refs {
    /// 从 payload 递归收集：识别 `blob_hash`、`*_id` 与 `{type, id}` 引用对。
    ///
    /// 自动收集保证 GC 根集不会因调用方忘记填写 `Refs` 而漏掉 blob——
    /// 漏掉根即等于允许误删历史 blob（原则 21）。
    pub fn from_payload(payload: &Value) -> Self {
        Self::from_payload_with_kind(None, payload)
    }

    /// 带原子类型提示的收集：`revert` / `reapply` 的 `target` 字段是原子引用。
    pub fn from_payload_with_kind(kind: Option<AtomKind>, payload: &Value) -> Self {
        let mut refs = Self::default();
        if let Some(kind) = kind {
            if matches!(kind, AtomKind::Revert | AtomKind::Reapply) {
                if let Some(target) = payload_str(payload, "target") {
                    refs.atoms.push(target.to_owned());
                }
            }
        }
        collect_refs(payload, None, &mut refs);
        refs.dedup();
        refs
    }

    /// 合并另一组引用。
    pub fn merge(&mut self, other: Refs) {
        self.blobs.extend(other.blobs);
        self.objects.extend(other.objects);
        self.layers.extend(other.layers);
        self.selections.extend(other.selections);
        self.masks.extend(other.masks);
        self.styles.extend(other.styles);
        self.atoms.extend(other.atoms);
        self.checkpoints.extend(other.checkpoints);
        self.dedup();
    }

    /// 排序去重。
    pub fn dedup(&mut self) {
        dedup_list(&mut self.blobs);
        dedup_list(&mut self.objects);
        dedup_list(&mut self.layers);
        dedup_list(&mut self.selections);
        dedup_list(&mut self.masks);
        dedup_list(&mut self.styles);
        dedup_list(&mut self.atoms);
        dedup_list(&mut self.checkpoints);
    }

    /// 是否没有任何引用。
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
            && self.objects.is_empty()
            && self.layers.is_empty()
            && self.selections.is_empty()
            && self.masks.is_empty()
            && self.styles.is_empty()
            && self.atoms.is_empty()
            && self.checkpoints.is_empty()
    }

    /// 全部 blob 哈希。
    pub fn blob_set(&self) -> BTreeSet<BlobHash> {
        self.blobs.iter().cloned().collect()
    }
}

fn collect_refs(value: &Value, key: Option<&str>, refs: &mut Refs) {    match value {
        Value::Object(map) => {
            // `{"type": "atom"|"checkpoint", "id": "..."}` 形式的 declare_head.base
            let kind = map.get("type").and_then(Value::as_str);
            let id = map.get("id").and_then(Value::as_str);
            if let (Some(kind), Some(id)) = (kind, id) {
                match kind {
                    "atom" | "declare_head" => refs.atoms.push(id.to_owned()),
                    "checkpoint" => refs.checkpoints.push(id.to_owned()),
                    _ => {}
                }
            }
            for (child_key, child) in map {
                collect_refs(child, Some(child_key.as_str()), refs);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_refs(item, key, refs);
            }
        }
        Value::String(text) => {
            let Some(key) = key else { return };
            match key {
                "blob_hash" | "blob" => {
                    if text.starts_with("sha256:") {
                        if let Ok(hash) = BlobHash::from_str(text) {
                            refs.blobs.push(hash);
                        }
                    }
                }
                "object_id" | "target_object" | "master_object_id" | "new_object_id" => {
                    refs.objects.push(text.to_owned())
                }
                "layer_id" | "target_layer" => refs.layers.push(text.to_owned()),
                "selection_id" => refs.selections.push(text.to_owned()),
                "mask_id" => refs.masks.push(text.to_owned()),
                "style_id" => refs.styles.push(text.to_owned()),
                "atom_id" | "target_atom" | "base_atom_id" => refs.atoms.push(text.to_owned()),
                "checkpoint_id" => refs.checkpoints.push(text.to_owned()),
                _ => {}
            }
        }
        _ => {}
    }
}

/// 排序去重任意标识列表。
fn dedup_list<T: Ord>(list: &mut Vec<T>) {
    list.sort();
    list.dedup();
}

/// 原子类型（设计文档 5.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtomKind {
    // 创建类
    /// 创建文档。
    CreateDocument,
    /// 创建图层。
    CreateLayer,
    /// 创建对象。
    CreateObject,
    /// 创建选区。
    CreateSelection,
    /// 创建蒙版。
    CreateMask,
    /// 创建风格。
    CreateStyle,
    /// 创建检查点。
    CreateCheckpoint,
    /// 导入图像（位图入 Blob CAS）。
    ImportImage,
    // 绘制类
    /// 笔触。
    DrawStroke,
    /// 填充。
    Fill,
    /// 形状。
    DrawShape,
    /// 文本。
    DrawText,
    /// 擦除。
    Erase,
    /// 修图。
    Retouch,
    /// 液化。
    Liquify,
    // 修改类
    /// 对象新版本。
    Supersede,
    /// 移动。
    Move,
    /// 变换。
    Transform,
    /// 设置属性。
    SetProperty,
    /// 重排图层（携带完整目标 z 序）。
    ReorderLayers,
    // 删除类
    /// 墓碑删除。
    Tombstone,
    // 历史类
    /// 撤销目标原子。
    Revert,
    /// 恢复目标原子（不恢复级联链）。
    Reapply,
    /// 检查点标记。
    Checkpoint,
    /// 标签。
    Tag,
    /// 求值起点跳变。
    DeclareHead,
    // 协作类
    /// 评论。
    Comment,
    /// 建议。
    Suggest,
    /// 接受建议。
    AcceptSuggestion,
    /// 拒绝建议。
    RejectSuggestion,
}

impl AtomKind {
    /// schema 中使用的 snake_case 字面量。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreateDocument => "create_document",
            Self::CreateLayer => "create_layer",
            Self::CreateObject => "create_object",
            Self::CreateSelection => "create_selection",
            Self::CreateMask => "create_mask",
            Self::CreateStyle => "create_style",
            Self::CreateCheckpoint => "create_checkpoint",
            Self::ImportImage => "import_image",
            Self::DrawStroke => "draw_stroke",
            Self::Fill => "fill",
            Self::DrawShape => "draw_shape",
            Self::DrawText => "draw_text",
            Self::Erase => "erase",
            Self::Retouch => "retouch",
            Self::Liquify => "liquify",
            Self::Supersede => "supersede",
            Self::Move => "move",
            Self::Transform => "transform",
            Self::SetProperty => "set_property",
            Self::ReorderLayers => "reorder_layers",
            Self::Tombstone => "tombstone",
            Self::Revert => "revert",
            Self::Reapply => "reapply",
            Self::Checkpoint => "checkpoint",
            Self::Tag => "tag",
            Self::DeclareHead => "declare_head",
            Self::Comment => "comment",
            Self::Suggest => "suggest",
            Self::AcceptSuggestion => "accept_suggestion",
            Self::RejectSuggestion => "reject_suggestion",
        }
    }

    /// 是否属于协作类原子：不产生状态效果，也不能被 revert（设计文档 5.4）。
    pub const fn is_collab(self) -> bool {
        matches!(
            self,
            Self::Comment | Self::Suggest | Self::AcceptSuggestion | Self::RejectSuggestion
        )
    }

    /// 是否属于历史类原子。
    pub const fn is_history(self) -> bool {
        matches!(
            self,
            Self::Revert | Self::Reapply | Self::Checkpoint | Self::Tag | Self::DeclareHead
        )
    }

    /// 是否属于绘制类原子（生成性叠加，默认 LWW，不走冲突层）。
    pub const fn is_generative(self) -> bool {
        matches!(
            self,
            Self::DrawStroke
                | Self::Fill
                | Self::DrawShape
                | Self::DrawText
                | Self::Erase
        )
    }

    /// 是否为采样性替换原子（设计文档 12.3 冲突层入口）。
    pub const fn is_sampling_replace(self) -> bool {
        matches!(self, Self::Retouch | Self::Liquify)
    }

    /// 是否为 `revert` 的合法目标：有状态效果的原子类别（设计文档 5.4）。
    ///
    /// `comment`、`suggest` 等协作原子，以及 `revert`/`reapply`/`declare_head`
    /// 等历史原子不在其中。
    pub const fn is_state_effect(self) -> bool {
        matches!(
            self,
            Self::CreateDocument
                | Self::CreateLayer
                | Self::CreateObject
                | Self::CreateSelection
                | Self::CreateMask
                | Self::CreateStyle
                | Self::CreateCheckpoint
                | Self::ImportImage
                | Self::DrawStroke
                | Self::Fill
                | Self::DrawShape
                | Self::DrawText
                | Self::Erase
                | Self::Retouch
                | Self::Liquify
                | Self::Supersede
                | Self::Move
                | Self::Transform
                | Self::SetProperty
                | Self::ReorderLayers
                | Self::Tombstone
        )
    }

    /// 是否触发结构 dirty（设计文档 6.6）。
    pub const fn affects_structure(self) -> bool {
        matches!(
            self,
            Self::Revert
                | Self::Reapply
                | Self::Supersede
                | Self::Tombstone
                | Self::DeclareHead
                | Self::ReorderLayers
                | Self::SetProperty
                | Self::CreateLayer
                | Self::CreateObject
                | Self::CreateMask
                | Self::CreateSelection
        )
    }

    /// 是否触发几何 dirty（设计文档 6.6）。
    pub const fn affects_geometry(self) -> bool {
        matches!(
            self,
            Self::DrawStroke
                | Self::DrawShape
                | Self::DrawText
                | Self::Fill
                | Self::Erase
                | Self::Move
                | Self::Transform
                | Self::Retouch
                | Self::Liquify
                | Self::ImportImage
        )
    }

    /// 重型原子基础集合（设计文档 6.5）。滤镜与 RasterPatch 由 [`Atom::is_heavy`] 按 payload 判定。
    pub const fn is_heavy_kind(self) -> bool {
        matches!(self, Self::Retouch | Self::Liquify | Self::DeclareHead)
    }

    /// 全部原子类型，便于穷举测试。
    pub const ALL: [AtomKind; 30] = [
        Self::CreateDocument,
        Self::CreateLayer,
        Self::CreateObject,
        Self::CreateSelection,
        Self::CreateMask,
        Self::CreateStyle,
        Self::CreateCheckpoint,
        Self::ImportImage,
        Self::DrawStroke,
        Self::Fill,
        Self::DrawShape,
        Self::DrawText,
        Self::Erase,
        Self::Retouch,
        Self::Liquify,
        Self::Supersede,
        Self::Move,
        Self::Transform,
        Self::SetProperty,
        Self::ReorderLayers,
        Self::Tombstone,
        Self::Revert,
        Self::Reapply,
        Self::Checkpoint,
        Self::Tag,
        Self::DeclareHead,
        Self::Comment,
        Self::Suggest,
        Self::AcceptSuggestion,
        Self::RejectSuggestion,
    ];
}

impl fmt::Display for AtomKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 原子（设计文档 5.1）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    /// 客户端生成的 ULID，服务端按 id 去重。
    pub id: AtomId,
    /// 服务端权威序号；未提交时为 [`SEQ_UNASSIGNED`]。
    pub seq: Seq,
    /// 因果记录，非排序依据。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<AtomId>,
    /// 原子类型。
    pub kind: AtomKind,
    /// 操作者。
    pub actor: ActorId,
    /// 会话。
    pub session: SessionId,
    /// 客户端时间戳（毫秒）。
    pub timestamp: i64,
    /// 可选说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// 所属变更集。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changeset_id: Option<ChangesetId>,
    /// schema 版本。
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// 插件来源。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginRef>,
    /// 载荷：大二进制外置，只存 blob 引用。
    #[serde(default)]
    pub payload: Value,
    /// 结构化引用（自动从 payload 收集，可显式补充）。
    #[serde(default, skip_serializing_if = "Refs::is_empty")]
    pub refs: Refs,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Atom {
    /// 构造一个客户端原子：生成 ULID，自动收集 payload 引用，`seq` 待服务端分配。
    pub fn new(
        kind: AtomKind,
        actor: impl Into<ActorId>,
        session: impl Into<SessionId>,
        payload: Value,
    ) -> Self {
        Self {
            id: Ulid::new().encode(),
            seq: SEQ_UNASSIGNED,
            parents: Vec::new(),
            kind,
            actor: actor.into(),
            session: session.into(),
            timestamp: now_ms(),
            message: None,
            changeset_id: None,
            schema_version: SCHEMA_VERSION,
            plugin: None,
            refs: Refs::from_payload_with_kind(Some(kind), &payload),
            payload,
        }
    }

    /// 指定 id（测试、回放与跨端一致场景使用）。
    pub fn with_id(mut self, id: impl Into<AtomId>) -> Self {
        self.id = id.into();
        self
    }

    /// 指定因果父原子。
    pub fn with_parents<I, S>(mut self, parents: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<AtomId>,
    {
        self.parents = parents.into_iter().map(Into::into).collect();
        self
    }

    /// 指定变更集。
    pub fn with_changeset(mut self, changeset_id: impl Into<ChangesetId>) -> Self {
        self.changeset_id = Some(changeset_id.into());
        self
    }

    /// 指定说明。
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// 指定时间戳。
    pub fn with_timestamp(mut self, timestamp: i64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// 指定插件来源。
    pub fn with_plugin(mut self, id: impl Into<String>, version: impl Into<String>) -> Self {
        self.plugin = Some(PluginRef {
            id: id.into(),
            version: version.into(),
        });
        self
    }

    /// 合并显式引用。
    pub fn with_refs(mut self, refs: Refs) -> Self {
        self.refs.merge(refs);
        self
    }

    /// 是否已获得权威 seq。
    pub fn is_submitted(&self) -> bool {
        self.seq != SEQ_UNASSIGNED
    }

    /// `revert` / `reapply` 的目标原子 id。
    pub fn target_atom(&self) -> Option<&str> {
        match self.kind {
            AtomKind::Revert | AtomKind::Reapply => payload_str(&self.payload, "target"),
            _ => None,
        }
    }

    /// 受影响的图层（冲突检测与 dirty 计算共用）。
    pub fn layer_id(&self) -> Option<&str> {
        payload_str(&self.payload, "layer_id")
    }

    /// 受影响的第一个对象。
    pub fn object_id(&self) -> Option<&str> {
        payload_str(&self.payload, "object_id")
    }

    /// 是否重型原子（设计文档 6.5）：retouch / liquify / declare_head，
    /// 以及大尺寸滤镜与 RasterPatch。
    pub fn is_heavy(&self) -> bool {
        if self.kind.is_heavy_kind() {
            return true;
        }
        if !matches!(
            self.kind,
            AtomKind::CreateObject | AtomKind::Supersede | AtomKind::ImportImage
        ) {
            return false;
        }
        matches!(
            payload_str(&self.payload, "type"),
            Some("raster_patch") | Some("filter")
        )
    }

    /// 全部 blob 引用（含 payload 中未登记到 `refs` 的部分，双保险）。
    pub fn all_blob_refs(&self) -> BTreeSet<BlobHash> {
        let mut set = self.refs.blob_set();
        set.extend(Refs::from_payload(&self.payload).blobs);
        set
    }
}

/// 读取 payload 中的字符串字段。
pub fn payload_str<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// 读取 payload 中的整数字段。
pub fn payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(Value::as_u64)
}

/// 读取 payload 中的浮点字段。
pub fn payload_f64(payload: &Value, key: &str) -> Option<f64> {
    payload.get(key).and_then(Value::as_f64)
}

/// 读取 payload 中的布尔字段。
pub fn payload_bool(payload: &Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn blob_hash_is_content_addressed() {
        let a = BlobHash::from_bytes(b"yanshi");
        let b = BlobHash::from_bytes(b"yanshi");
        let c = BlobHash::from_bytes(b"yanshi!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!(a.as_str().starts_with("sha256:"));
        assert_eq!(a.as_str().len(), 71);
        // 已知向量：sha256("yanshi")
        assert_eq!(
            a.hex(),
            "90acebf65bcf64f1516807d1312911a60dfe926fc1c5dabb8d988dbfd651bf6e"
        );
        assert!(BlobHash::from_str(a.as_str()).is_ok());
        assert!(BlobHash::from_str("sha256:XYZ").is_err());
        assert!(BlobHash::from_str("md5:abc").is_err());
    }

    #[test]
    fn refs_are_collected_recursively() {
        let payload = json!({
            "object_id": "obj_1",
            "layer_id": "layer_1",
            "base": {"type": "checkpoint", "id": "ckpt_1"},
            "target_atom": "atom_1",
            "bitmap": {"blob_hash": "sha256:".to_owned() + &"a".repeat(64), "size": 8192, "mime_type": "image/webp"},
            "nested": [{"mask_id": "mask_1"}, {"blob_hash": "sha256:".to_owned() + &"b".repeat(64)}],
            "unrelated": "ignored",
        });
        let refs = Refs::from_payload(&payload);
        assert_eq!(refs.objects, vec!["obj_1"]);
        assert_eq!(refs.layers, vec!["layer_1"]);
        assert_eq!(refs.checkpoints, vec!["ckpt_1"]);
        assert_eq!(refs.atoms, vec!["atom_1"]);
        assert_eq!(refs.masks, vec!["mask_1"]);
        assert_eq!(refs.blobs.len(), 2);
        assert!(!refs.is_empty());
        assert_eq!(refs.blob_set().len(), 2);
    }

    #[test]
    fn revert_and_reapply_targets_are_atom_refs() {
        let payload = json!({"target": "atom_9"});
        let generic = Refs::from_payload(&payload);
        assert!(generic.atoms.is_empty(), "无类型提示时 target 语义未知");
        for kind in [AtomKind::Revert, AtomKind::Reapply] {
            let refs = Refs::from_payload_with_kind(Some(kind), &payload);
            assert_eq!(refs.atoms, vec!["atom_9"]);
        }
        let atom = Atom::new(AtomKind::Revert, "human:1", "s1", payload);
        assert_eq!(atom.refs.atoms, vec!["atom_9"]);
        assert_eq!(atom.target_atom(), Some("atom_9"));
    }

    #[test]
    fn atom_new_assigns_ulid_and_unsubmitted_seq() {
        let atom = Atom::new(AtomKind::DrawStroke, "ai:1", "s1", json!({"object_id": "o"}));
        assert_eq!(atom.seq, SEQ_UNASSIGNED);
        assert!(!atom.is_submitted());
        assert_eq!(atom.id.len(), crate::ids::ULID_LEN);
        assert_eq!(atom.schema_version, SCHEMA_VERSION);
        assert!(atom.id.parse::<Ulid>().is_ok());
    }

    #[test]
    fn revert_target_and_heavy_classification() {
        let revert = Atom::new(AtomKind::Revert, "human:1", "s1", json!({"target": "atom_x"}));
        assert_eq!(revert.target_atom(), Some("atom_x"));
        assert!(!revert.is_heavy());

        let liquify = Atom::new(AtomKind::Liquify, "human:1", "s1", json!({}));
        assert!(liquify.is_heavy());

        let raster = Atom::new(
            AtomKind::CreateObject,
            "ai:1",
            "s1",
            json!({"type": "raster_patch"}),
        );
        assert!(raster.is_heavy());

        let stroke = Atom::new(AtomKind::CreateObject, "ai:1", "s1", json!({"type": "stroke"}));
        assert!(!stroke.is_heavy());
    }

    #[test]
    fn atom_kind_classification_matches_document() {
        // revert 不能作用于协作原子与历史原子。
        assert!(!AtomKind::Comment.is_state_effect());
        assert!(!AtomKind::Suggest.is_state_effect());
        assert!(!AtomKind::AcceptSuggestion.is_state_effect());
        assert!(!AtomKind::RejectSuggestion.is_state_effect());
        assert!(!AtomKind::Revert.is_state_effect());
        assert!(!AtomKind::Reapply.is_state_effect());
        assert!(!AtomKind::DeclareHead.is_state_effect());
        // 有状态效果的类别。
        assert!(AtomKind::CreateObject.is_state_effect());
        assert!(AtomKind::DrawStroke.is_state_effect());
        assert!(AtomKind::Supersede.is_state_effect());
        assert!(AtomKind::Tombstone.is_state_effect());
        assert!(AtomKind::SetProperty.is_state_effect());
        // 采样性替换与生成性叠加分界（12.3）。
        assert!(AtomKind::Retouch.is_sampling_replace());
        assert!(!AtomKind::DrawStroke.is_sampling_replace());
        assert!(AtomKind::DrawStroke.is_generative());
        // schema 字面量与 serde 一致。
        for kind in AtomKind::ALL {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, format!("\"{}\"", kind.as_str()), "kind={kind:?}");
            let back: AtomKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn atom_serde_roundtrip() {
        let atom = Atom::new(
            AtomKind::CreateLayer,
            "human:1",
            "session:1",
            json!({"layer_id": "l1"}),
        )
        .with_message("新建图层")
        .with_changeset("cs_1")
        .with_parents(["parent_1"]);
        let text = serde_json::to_string(&atom).unwrap();
        let back: Atom = serde_json::from_str(&text).unwrap();
        assert_eq!(atom, back);
    }
}
