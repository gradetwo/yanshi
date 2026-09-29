//! 工作区/服务：多文档注册表与会话编排（设计文档 3 章服务端）。
//!
//! [`Workspace`] 负责：文档创建与打开、持久化（原子 JSONL + CAS + 渲染缓存）、
//! 提交路径的落盘、令牌发放与鉴权、以及给工具层使用的统一入口。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use yanshi_core::blob::{BlobStore, MemoryBlobStore};
use yanshi_core::{Atom, Bbox, ChangesetId, ErrorCode, ErrorContext, Result, Seq, YanshiError};
use yanshi_render::thumb::ThumbKind;

use crate::document::{
    CommitResult, Document, DocumentSettings, NewDocument, RenderStatus, RenderedPreview,
};
use crate::persist::{DocumentMeta, FileStore};
use crate::token::{CapabilityToken, Principal, Role, TransportKind};

/// 文档缩略图尺寸档位（7.3 分级；`Skip` 表示不生成）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocThumbSize {
    /// 不生成缩略图。
    Skip,
    /// 64px。
    S64,
    /// 128px。
    S128,
    /// 256px（默认）。
    S256,
}

impl DocThumbSize {
    /// 对应的缩略图类型。
    pub const fn kind(self) -> ThumbKind {
        match self {
            Self::S64 => ThumbKind::Doc64,
            Self::S128 => ThumbKind::Doc128,
            Self::Skip | Self::S256 => ThumbKind::Doc256,
        }
    }

    /// 由参数解析（`64` / `128` / `256` / `false`）。
    pub fn parse(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Bool(false) => Self::Skip,
            serde_json::Value::Bool(true) | serde_json::Value::Null => Self::S256,
            serde_json::Value::Number(number) => match number.as_u64() {
                Some(64) | Some(32) => Self::S64,
                Some(128) => Self::S128,
                _ => Self::S256,
            },
            serde_json::Value::String(text) => match text.as_str() {
                "skip" | "none" | "false" => Self::Skip,
                "64" => Self::S64,
                "128" => Self::S128,
                _ => Self::S256,
            },
            _ => Self::S256,
        }
    }
}

/// 文档摘要（文档列表）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSummary {
    /// 文档 id。
    pub doc_id: String,
    /// 画布宽。
    pub width: u32,
    /// 画布高。
    pub height: u32,
    /// 原子数。
    pub atoms: usize,
    /// head seq。
    pub head_seq: Seq,
    /// 图层数。
    pub layers: usize,
    /// 对象数。
    pub objects: usize,
    /// 创建时间。
    pub created_at: i64,
    /// 是否已持久化到磁盘。
    pub persisted: bool,
}

/// 多文档工作区。
pub struct Workspace {
    store: Arc<dyn BlobStore>,
    documents: BTreeMap<String, Document>,
    persist: Option<FileStore>,
    settings: DocumentSettings,
    created: u64,
}

impl std::fmt::Debug for Workspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workspace")
            .field("documents", &self.documents.keys().collect::<Vec<_>>())
            .field("persisted", &self.persist.is_some())
            .finish()
    }
}

impl Workspace {
    /// 内存工作区（测试与嵌入式使用）。
    pub fn in_memory(settings: DocumentSettings) -> Self {
        Self {
            store: Arc::new(MemoryBlobStore::new()),
            documents: BTreeMap::new(),
            persist: None,
            settings,
            created: 0,
        }
    }

    /// 以文件存储构造（打开时加载已有文档索引）。
    pub fn with_file_store(
        root: impl Into<std::path::PathBuf>,
        settings: DocumentSettings,
    ) -> Result<Self> {
        let persist = FileStore::open(root)?;
        let store: Arc<dyn BlobStore> = Arc::new(persist.blob_store()?);
        Ok(Self {
            store,
            documents: BTreeMap::new(),
            persist: Some(persist),
            settings,
            created: 0,
        })
    }

    /// CAS。
    pub fn store(&self) -> Arc<dyn BlobStore> {
        Arc::clone(&self.store)
    }

    /// 持久化层。
    pub const fn persist(&self) -> Option<&FileStore> {
        self.persist.as_ref()
    }

    /// 设置。
    pub const fn settings(&self) -> &DocumentSettings {
        &self.settings
    }

    /// 已打开的文档 id。
    pub fn document_ids(&self) -> Vec<String> {
        self.documents.keys().cloned().collect()
    }

    /// 新建文档（可选持久化）。
    pub fn create_document(
        &mut self,
        spec: NewDocument,
        actor: &str,
        session: &str,
    ) -> Result<&mut Document> {
        let doc_id = spec.doc_id.clone();
        if self.documents.contains_key(&doc_id) {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("文档 {doc_id} 已打开")),
            ));
        }
        if let Some(persist) = &self.persist {
            if !persist.load_atoms(&doc_id)?.is_empty() {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!("文档 {doc_id} 已存在于磁盘")),
                ));
            }
        }
        let document = Document::create(
            Arc::clone(&self.store),
            spec.clone(),
            actor,
            session,
            self.settings.clone(),
        )?;
        self.documents.insert(doc_id.clone(), document);
        self.created += 1;

        if let Some(persist) = &self.persist {
            let document = self.documents.get(&doc_id).expect("刚插入");
            for atom in document.log().atoms() {
                persist.append_atom(&doc_id, atom)?;
            }
            persist.save_meta(&DocumentMeta {
                doc_id: doc_id.clone(),
                width: spec.width,
                height: spec.height,
                created_at: yanshi_core::now_ms(),
                tokens: Vec::new(),
            })?;
        }
        self.document_mut(&doc_id)
    }

    /// 打开（或返回已打开的）文档：从磁盘加载日志并重建状态。
    pub fn open_document(&mut self, doc_id: &str) -> Result<&mut Document> {
        if !self.documents.contains_key(doc_id) {
            let (atoms, render) = match &self.persist {
                Some(persist) => (persist.load_atoms(doc_id)?, persist.load_render(doc_id)?),
                None => (Vec::new(), None),
            };
            if atoms.is_empty() {
                return Err(YanshiError::new(
                    ErrorCode::ReferenceNotFound,
                    ErrorContext::detail(format!("文档 {doc_id} 不存在")),
                ));
            }
            let mut document = Document::open(
                Arc::clone(&self.store),
                doc_id,
                atoms,
                self.settings.clone(),
            )?;
            if let Some((seq, png)) = render {
                // 打开即图片（14.5/6.2）：直接复用持久化渲染缓存，不重放历史。
                let hash = self.store.put(&png)?;
                document.mark_rendered(seq, Some(hash));
            }
            // 恢复令牌（12.7）。
            if let Some(persist) = &self.persist {
                if let Some(meta) = persist.load_meta(doc_id)? {
                    for record in meta.tokens {
                        document.authority_mut().restore(
                            &record.token,
                            &record.actor,
                            record.role,
                        )?;
                    }
                }
            }
            self.documents.insert(doc_id.to_owned(), document);
        }
        self.document_mut(doc_id)
    }

    /// 打开或创建（工具层 `open_document` 的语义）。
    pub fn open_or_create(
        &mut self,
        spec: NewDocument,
        actor: &str,
        session: &str,
    ) -> Result<&mut Document> {
        let doc_id = spec.doc_id.clone();
        if self.documents.contains_key(&doc_id) {
            return self.document_mut(&doc_id);
        }
        let exists = match &self.persist {
            Some(persist) => !persist.load_atoms(&doc_id)?.is_empty(),
            None => false,
        };
        if exists {
            self.open_document(&doc_id)
        } else {
            self.create_document(spec, actor, session)
        }
    }

    /// 只读文档引用。
    pub fn document(&self, doc_id: &str) -> Option<&Document> {
        self.documents.get(doc_id)
    }

    /// 可变文档引用。
    pub fn document_mut(&mut self, doc_id: &str) -> Result<&mut Document> {
        self.documents.get_mut(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })
    }

    /// 关闭文档（释放内存；持久化内容保留）。
    pub fn close_document(&mut self, doc_id: &str) -> bool {
        self.documents.remove(doc_id).is_some()
    }

    /// 文档列表（磁盘上的也包含，标记 `persisted`）。
    pub fn list_documents(&self) -> Result<Vec<DocumentSummary>> {
        let mut summaries: BTreeMap<String, DocumentSummary> = BTreeMap::new();
        for (doc_id, document) in &self.documents {
            summaries.insert(
                doc_id.clone(),
                DocumentSummary {
                    doc_id: doc_id.clone(),
                    width: document.state().width,
                    height: document.state().height,
                    atoms: document.atom_count(),
                    head_seq: document.head_seq(),
                    layers: document.state().layers.len(),
                    objects: document.state().objects.len(),
                    created_at: document.created_at(),
                    persisted: self.persist.is_some(),
                },
            );
        }
        if let Some(persist) = &self.persist {
            for doc_id in persist.list_documents()? {
                if summaries.contains_key(&doc_id) {
                    continue;
                }
                let meta = persist.load_meta(&doc_id)?;
                let atoms = persist.load_atoms(&doc_id)?;
                summaries.insert(
                    doc_id.clone(),
                    DocumentSummary {
                        doc_id: doc_id.clone(),
                        width: meta.as_ref().map(|meta| meta.width).unwrap_or(0),
                        height: meta.as_ref().map(|meta| meta.height).unwrap_or(0),
                        atoms: atoms.len(),
                        head_seq: atoms.last().map(|atom| atom.seq).unwrap_or(0),
                        layers: 0,
                        objects: 0,
                        created_at: meta.as_ref().map(|meta| meta.created_at).unwrap_or(0),
                        persisted: true,
                    },
                );
            }
        }
        Ok(summaries.into_values().collect())
    }

    /// 提交原子并落盘（服务端权威路径）。
    pub fn commit(
        &mut self,
        doc_id: &str,
        atom: Atom,
        actor: &str,
        owner: bool,
    ) -> Result<CommitResult> {
        let result = {
            let document = self.document_mut(doc_id)?;
            document.commit_as(atom, actor, owner, None)?
        };
        self.journal(doc_id, &result)?;
        Ok(result)
    }

    /// 以变更集提交多个原子（batch，5.6）。
    pub fn commit_changeset(
        &mut self,
        doc_id: &str,
        atoms: Vec<Atom>,
        actor: &str,
        owner: bool,
        changeset_id: ChangesetId,
    ) -> Result<Vec<CommitResult>> {
        let mut results = Vec::with_capacity(atoms.len());
        for atom in atoms {
            let result = {
                let document = self.document_mut(doc_id)?;
                document.commit_as(atom, actor, owner, Some(changeset_id.clone()))?
            };
            self.journal(doc_id, &result)?;
            results.push(result);
        }
        Ok(results)
    }

    /// 渲染区域。
    ///
    /// 只有覆盖整幅画布的渲染才落盘为「HEAD 渲染缓存」（14.5 打开即图片）；
    /// 局部 dirty 渲染虽然也进 CAS，但不会覆盖文档级缓存。
    /// 渲染区域并返回原始 RGBA8（供 `patch` 抓取源像素；不触碰渲染缓存状态）。
    pub fn render_region_raw(&mut self, doc_id: &str, bbox: Bbox) -> Result<(u32, u32, Vec<u8>)> {
        self.document_mut(doc_id)?.render_region_raw(bbox)
    }

    /// 渲染区域、写入渲染缓存并返回可展示的预览（含 PNG blob 与取回地址）。
    pub fn render_region(&mut self, doc_id: &str, bbox: Bbox) -> Result<RenderedPreview> {
        let (preview, head, png, full_frame) = {
            let document = self.document_mut(doc_id)?;
            let (width, height) = (document.state().width, document.state().height);
            let preview = document.render_region(bbox)?;
            let png = document.store().get(&preview.blob_hash)?;
            let covers =
                bbox.x <= 0.0 && bbox.y <= 0.0 && bbox.w >= width as f64 && bbox.h >= height as f64;
            (preview, document.head_seq(), png, covers)
        };
        if full_frame {
            if let Some(persist) = &self.persist {
                let _ = persist.save_render(doc_id, head, &png);
            }
        }
        Ok(preview)
    }

    /// 生成缩略图；文档级缩略图会落盘为渲染缓存（14.5）。
    pub fn thumbnail(
        &mut self,
        doc_id: &str,
        kind: ThumbKind,
        target: Option<Bbox>,
    ) -> Result<RenderedPreview> {
        let (preview, head, png) = {
            let document = self.document_mut(doc_id)?;
            let preview = document.thumbnail(kind, target)?;
            let png = document.store().get(&preview.blob_hash)?;
            (preview, document.head_seq(), png)
        };
        if target.is_none() && kind.is_document_level() {
            if let Some(persist) = &self.persist {
                let _ = persist.save_render(doc_id, head, &png);
            }
        }
        Ok(preview)
    }

    /// 确保存在文档级缩略图并返回它的地址（6.2「打开即图片」）。
    ///
    /// 已有缓存（渲染过全幅或从磁盘恢复）直接返回；否则按 `preview_size` 生成一张。
    pub fn ensure_document_thumbnail(
        &mut self,
        doc_id: &str,
        size: DocThumbSize,
    ) -> Result<Option<String>> {
        {
            let document = self.document_mut(doc_id)?;
            // 只有与 HEAD 一致的缩略图才算「打开即图片」的缓存；落后则重新生成。
            if document.document_thumbnail_is_current() {
                if let Some(url) = document.document_thumbnail_url() {
                    return Ok(Some(url));
                }
            }
        }
        if matches!(size, DocThumbSize::Skip) {
            return Ok(None);
        }
        let kind = size.kind();
        let preview = self.thumbnail(doc_id, kind, None)?;
        Ok(Some(preview.url))
    }

    /// 渲染状态。
    pub fn render_status(&mut self, doc_id: &str, atom_id: &str) -> Result<RenderStatus> {
        let document = self.document_mut(doc_id)?;
        document.render_status(atom_id)
    }

    /// 发放令牌并持久化（12.7）。
    pub fn issue_token(
        &mut self,
        doc_id: &str,
        actor: &str,
        role: Role,
    ) -> Result<CapabilityToken> {
        let token = {
            let document = self.document_mut(doc_id)?;
            document.issue_token(actor, role)
        };
        if let Some(persist) = &self.persist {
            persist.record_token(doc_id, &token, actor, role)?;
        }
        Ok(token)
    }

    /// 鉴权（决定提交时的 actor 与权限）。
    ///
    /// 文档若尚未打开但磁盘存在，会先按需加载（含 token 恢复，12.7）——
    /// HTTP/WS 请求带着 token 打进来时不该因为进程刚重启就失败。
    pub fn authorize(
        &mut self,
        doc_id: &str,
        token: Option<&CapabilityToken>,
        transport: TransportKind,
        fallback_actor: &str,
    ) -> Result<Principal> {
        if !self.documents.contains_key(doc_id) {
            let exists = match &self.persist {
                Some(persist) => !persist.load_atoms(doc_id)?.is_empty(),
                None => false,
            };
            if exists {
                self.open_document(doc_id)?;
            }
        }
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        document.authorize(token, transport, fallback_actor)
    }

    /// 文档 capability URL（打开文档时返回内嵌 token 的 URL，12.7）。
    pub fn capability_url(&self, doc_id: &str, token: &CapabilityToken) -> String {
        format!("yanshi://doc/{doc_id}?token={token}")
    }

    /// 文档数量。
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// 是否没有打开的文档。
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// 累计创建过的文档数。
    pub const fn created(&self) -> u64 {
        self.created
    }

    /// 文档摘要 JSON（`get_document` 工具用）。
    pub fn summary_json(&self, doc_id: &str) -> Result<Value> {
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        let state = document.state();
        Ok(json!({
            "doc_id": doc_id,
            "width": state.width,
            "height": state.height,
            "color_space": state.color_space,
            "background": state.background,
            "head_seq": document.head_seq(),
            "head_atom": state.head_atom,
            "eval_origin_seq": state.eval_origin_seq(),
            "layers": state.layers.len(),
            "objects": state.objects.len(),
            "atoms": document.atom_count(),
            "rendered_seq": document.render_watermark(),
            "medium": state.medium,
        }))
    }

    fn journal(&self, doc_id: &str, result: &CommitResult) -> Result<()> {
        if result.duplicate {
            return Ok(());
        }
        let Some(persist) = &self.persist else {
            return Ok(());
        };
        let document = self.document(doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开")),
            )
        })?;
        if let Some(atom) = document.log().by_seq(result.seq) {
            persist.append_atom(doc_id, atom)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!("yanshi-ws-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn in_memory_workspace_creates_and_commits() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        assert!(workspace.is_empty());
        {
            let document = workspace
                .create_document(NewDocument::new("doc_1", 32, 32), "human:1", "session:a")
                .unwrap();
            assert_eq!(document.head_seq(), 1);
        }
        let result = workspace
            .commit(
                "doc_1",
                Atom::new(
                    yanshi_core::AtomKind::CreateLayer,
                    "human:1",
                    "session:a",
                    json!({"layer_id": "layer_1"}),
                ),
                "human:1",
                false,
            )
            .unwrap();
        assert_eq!(result.seq, 2);
        assert_eq!(workspace.created(), 1);
        assert_eq!(workspace.document_ids(), vec!["doc_1"]);

        let summary = workspace.summary_json("doc_1").unwrap();
        assert_eq!(summary["width"], json!(32));
        assert_eq!(summary["head_seq"], json!(2));

        // 重复创建同名文档被拒绝。
        assert_eq!(
            workspace
                .create_document(NewDocument::new("doc_1", 32, 32), "human:1", "session:a")
                .unwrap_err()
                .code,
            ErrorCode::PreconditionFailed
        );
        assert!(workspace.close_document("doc_1"));
        assert!(workspace.document("doc_1").is_none());
    }

    #[test]
    fn file_workspace_survives_restart_with_tokens_and_render_cache() {
        let root = temp_root("restart");
        let settings = DocumentSettings::default();
        let token;
        {
            let mut workspace = Workspace::with_file_store(&root, settings.clone()).unwrap();
            workspace
                .create_document(NewDocument::new("doc_1", 64, 48), "human:1", "session:a")
                .unwrap();
            for atom in [
                Atom::new(
                    yanshi_core::AtomKind::CreateLayer,
                    "human:1",
                    "session:a",
                    json!({"layer_id": "layer_1"}),
                ),
                Atom::new(
                    yanshi_core::AtomKind::DrawStroke,
                    "human:1",
                    "session:a",
                    json!({
                        "object_id": "obj_1",
                        "layer_id": "layer_1",
                        "data": {"points": [[4.0, 4.0], [40.0, 20.0]], "size": 4.0},
                    }),
                ),
            ] {
                workspace.commit("doc_1", atom, "human:1", false).unwrap();
            }
            token = workspace
                .issue_token("doc_1", "human:2", Role::Editor)
                .unwrap();
            let preview = workspace
                .render_region("doc_1", Bbox::new(0.0, 0.0, 64.0, 48.0))
                .unwrap();
            assert_eq!(preview.mime_type, "image/png");
            assert!(workspace.capability_url("doc_1", &token).contains("token="));
        }

        // 重启：新工作区从磁盘恢复。
        let mut workspace = Workspace::with_file_store(&root, settings).unwrap();
        assert!(workspace.is_empty(), "打开时按需加载");
        let summaries = workspace.list_documents().unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].doc_id, "doc_1");
        assert_eq!(summaries[0].width, 64);

        let document = workspace.open_document("doc_1").unwrap();
        assert_eq!(document.head_seq(), 3);
        assert_eq!(
            document.render_watermark(),
            3,
            "打开即图片：渲染缓存直接可用（14.5）"
        );
        assert!(document.latest_preview_url().is_some());
        // 令牌恢复后可鉴权。
        let principal = workspace
            .authorize("doc_1", Some(&token), TransportKind::Http, "x")
            .unwrap();
        assert_eq!(principal.actor, "human:2");
        assert!(principal.role.can_edit());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn changeset_commit_shares_one_id() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        workspace
            .create_document(NewDocument::new("doc_1", 16, 16), "human:1", "s")
            .unwrap();
        let atoms = vec![
            Atom::new(
                yanshi_core::AtomKind::CreateLayer,
                "human:1",
                "s",
                json!({"layer_id": "layer_1"}),
            ),
            Atom::new(
                yanshi_core::AtomKind::DrawStroke,
                "human:1",
                "s",
                json!({"object_id": "obj_1", "layer_id": "layer_1", "data": {"points": [[1.0, 1.0]]}}),
            ),
        ];
        let changeset_id = yanshi_core::Changeset::new_id();
        let results = workspace
            .commit_changeset("doc_1", atoms, "human:1", false, changeset_id.clone())
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|result| result.changeset_id.as_deref() == Some(changeset_id.as_str())));
        let document = workspace.document("doc_1").unwrap();
        assert_eq!(document.log().changeset_atoms(&changeset_id).len(), 2);
    }

    #[test]
    fn permission_denied_for_unknown_documents() {
        let mut workspace = Workspace::in_memory(DocumentSettings::default());
        assert_eq!(
            workspace
                .commit(
                    "doc_missing",
                    Atom::new(yanshi_core::AtomKind::Comment, "human:1", "s", json!({})),
                    "human:1",
                    false
                )
                .unwrap_err()
                .code,
            ErrorCode::ReferenceNotFound
        );
        assert!(workspace.open_document("doc_missing").is_err());
    }
}
