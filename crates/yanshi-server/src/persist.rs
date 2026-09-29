//! 文件持久化（设计文档 18 章：本地文件 + Blob CAS）。
//!
//! 目录布局：
//!
//! ```text
//! <root>/
//!   blobs/sha256/<ab>/<cd>/<hex>      # CAS（FsBlobStore，hash 分桶）
//!   docs/<doc_id>/atoms.jsonl         # 原子日志（每行一个 JSON 原子，含权威 seq）
//!   docs/<doc_id>/meta.json           # 文档元数据与 capability token
//!   docs/<doc_id>/render.png          # 最近一次 HEAD 渲染缓存（14.5「打开即图片」）
//!   docs/<doc_id>/render.seq          # 渲染缓存对应的 seq
//! ```

use crate::token::{CapabilityToken, Role};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use yanshi_core::blob::FsBlobStore;
use yanshi_core::{Atom, ErrorCode, ErrorContext, Result, Seq, YanshiError};

/// 文件存储布局。
#[derive(Debug, Clone)]
pub struct FileStore {
    root: PathBuf,
}

/// 文档元数据（meta.json）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentMeta {
    /// 文档 id。
    pub doc_id: String,
    /// 画布宽。
    pub width: u32,
    /// 画布高。
    pub height: u32,
    /// 创建时间（Unix 毫秒）。
    pub created_at: i64,
    /// 已发放的 capability token（12.7）。
    #[serde(default)]
    pub tokens: Vec<TokenRecord>,
}

/// 持久化的令牌记录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenRecord {
    /// 令牌。
    pub token: String,
    /// 绑定 actor。
    pub actor: String,
    /// 角色。
    pub role: Role,
}

impl FileStore {
    /// 打开（或创建）根目录。
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("docs")).map_err(|error| io_error(&root, error))?;
        fs::create_dir_all(root.join("blobs")).map_err(|error| io_error(&root, error))?;
        Ok(Self { root })
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// CAS（`<root>/blobs`）。
    pub fn blob_store(&self) -> Result<FsBlobStore> {
        FsBlobStore::open(self.root.join("blobs"))
    }

    /// 文档目录。
    pub fn doc_dir(&self, doc_id: &str) -> PathBuf {
        self.root.join("docs").join(doc_id)
    }

    fn atoms_path(&self, doc_id: &str) -> PathBuf {
        self.doc_dir(doc_id).join("atoms.jsonl")
    }

    fn meta_path(&self, doc_id: &str) -> PathBuf {
        self.doc_dir(doc_id).join("meta.json")
    }

    /// 追加一个原子（JSONL）。
    pub fn append_atom(&self, doc_id: &str, atom: &Atom) -> Result<()> {
        let dir = self.doc_dir(doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let path = self.atoms_path(doc_id);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| io_error(&path, error))?;
        let line = serde_json::to_string(atom).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("原子序列化失败: {error}")),
            )
            .with_atom(atom.id.clone())
        })?;
        writeln!(file, "{line}").map_err(|error| io_error(&path, error))?;
        file.flush().map_err(|error| io_error(&path, error))?;
        Ok(())
    }

    /// 读取全部原子（忽略空行与损坏的尾行）。
    pub fn load_atoms(&self, doc_id: &str) -> Result<Vec<Atom>> {
        let path = self.atoms_path(doc_id);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io_error(&path, error)),
        };
        let mut atoms = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Atom>(line) {
                Ok(atom) => atoms.push(atom),
                Err(error) => {
                    // 崩溃残留的半行：只允许最后一行损坏。
                    if index + 1 == text.lines().count() {
                        break;
                    }
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("原子日志第 {} 行损坏: {error}", index + 1)),
                    ));
                }
            }
        }
        Ok(atoms)
    }

    /// 写入文档元数据。
    pub fn save_meta(&self, meta: &DocumentMeta) -> Result<()> {
        let dir = self.doc_dir(&meta.doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let path = self.meta_path(&meta.doc_id);
        let text = serde_json::to_string_pretty(meta).map_err(|error| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("元数据序列化失败: {error}")),
            )
        })?;
        fs::write(&path, text).map_err(|error| io_error(&path, error))?;
        Ok(())
    }

    /// 读取文档元数据（不存在返回 `None`）。
    pub fn load_meta(&self, doc_id: &str) -> Result<Option<DocumentMeta>> {
        let path = self.meta_path(doc_id);
        match fs::read_to_string(&path) {
            Ok(text) => {
                let meta = serde_json::from_str(&text).map_err(|error| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("元数据解析失败: {error}")),
                    )
                })?;
                Ok(Some(meta))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(&path, error)),
        }
    }

    /// 记录一个令牌。
    pub fn record_token(
        &self,
        doc_id: &str,
        token: &CapabilityToken,
        actor: &str,
        role: Role,
    ) -> Result<()> {
        let mut meta = self.load_meta(doc_id)?.unwrap_or(DocumentMeta {
            doc_id: doc_id.to_owned(),
            width: 0,
            height: 0,
            created_at: yanshi_core::now_ms(),
            tokens: Vec::new(),
        });
        meta.tokens.retain(|record| record.token != token.as_str());
        meta.tokens.push(TokenRecord {
            token: token.as_str().to_owned(),
            actor: actor.to_owned(),
            role,
        });
        self.save_meta(&meta)
    }

    /// 保存渲染缓存（`render.png` + `render.seq`）。
    pub fn save_render(&self, doc_id: &str, seq: Seq, png: &[u8]) -> Result<PathBuf> {
        let dir = self.doc_dir(doc_id);
        fs::create_dir_all(&dir).map_err(|error| io_error(&dir, error))?;
        let image_path = dir.join("render.png");
        let seq_path = dir.join("render.seq");
        // 先写临时文件再 rename，避免崩溃留下半张图（与 CAS 同一策略）。
        let tmp = dir.join("render.png.tmp");
        fs::write(&tmp, png).map_err(|error| io_error(&tmp, error))?;
        fs::rename(&tmp, &image_path).map_err(|error| io_error(&image_path, error))?;
        fs::write(&seq_path, seq.to_string()).map_err(|error| io_error(&seq_path, error))?;
        Ok(image_path)
    }

    /// 读取渲染缓存。
    pub fn load_render(&self, doc_id: &str) -> Result<Option<(Seq, Vec<u8>)>> {
        let dir = self.doc_dir(doc_id);
        let image_path = dir.join("render.png");
        let seq_path = dir.join("render.seq");
        let png = match fs::read(&image_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(&image_path, error)),
        };
        let seq = fs::read_to_string(&seq_path)
            .ok()
            .and_then(|text| text.trim().parse::<Seq>().ok())
            .unwrap_or(0);
        Ok(Some((seq, png)))
    }

    /// 列出已有文档 id（按字典序）。
    pub fn list_documents(&self) -> Result<Vec<String>> {
        let docs = self.root.join("docs");
        let mut ids = Vec::new();
        let entries = match fs::read_dir(&docs) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(ids),
            Err(error) => return Err(io_error(&docs, error)),
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_owned());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }
}

fn io_error(path: &Path, error: std::io::Error) -> YanshiError {
    YanshiError::new(
        ErrorCode::ResourceExhausted,
        ErrorContext::detail(format!("持久化 IO 失败 {:?}: {error}", path)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(tag: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!("yanshi-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn atom(id: &str, seq: Seq) -> Atom {
        let mut atom = Atom::new(
            yanshi_core::AtomKind::CreateLayer,
            "human:1",
            "session:a",
            json!({"layer_id": "layer_1"}),
        )
        .with_id(id);
        atom.seq = seq;
        atom
    }

    #[test]
    fn atoms_round_trip_as_jsonl() {
        let root = temp_root("atoms");
        let store = FileStore::open(&root).unwrap();
        for index in 1..=5 {
            store
                .append_atom("doc_1", &atom(&format!("a{index}"), index))
                .unwrap();
        }
        let atoms = store.load_atoms("doc_1").unwrap();
        assert_eq!(atoms.len(), 5);
        assert_eq!(atoms[4].seq, 5);
        assert_eq!(atoms[0].id, "a1");
        assert!(store.load_atoms("doc_missing").unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn truncated_tail_line_is_tolerated() {
        let root = temp_root("tail");
        let store = FileStore::open(&root).unwrap();
        store.append_atom("doc_1", &atom("a1", 1)).unwrap();
        store.append_atom("doc_1", &atom("a2", 2)).unwrap();
        // 模拟崩溃：追加半行。
        let path = store.doc_dir("doc_1").join("atoms.jsonl");
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"id\":\"a3\",\"seq\":3").unwrap();
        drop(file);
        let atoms = store.load_atoms("doc_1").unwrap();
        assert_eq!(atoms.len(), 2, "半行被忽略");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn meta_and_tokens_round_trip() {
        let root = temp_root("meta");
        let store = FileStore::open(&root).unwrap();
        assert!(store.load_meta("doc_1").unwrap().is_none());
        store
            .save_meta(&DocumentMeta {
                doc_id: "doc_1".to_owned(),
                width: 64,
                height: 32,
                created_at: 123,
                tokens: Vec::new(),
            })
            .unwrap();
        let token = CapabilityToken::generate();
        store
            .record_token("doc_1", &token, "human:1", Role::Editor)
            .unwrap();
        let meta = store.load_meta("doc_1").unwrap().unwrap();
        assert_eq!((meta.width, meta.height), (64, 32));
        assert_eq!(meta.tokens.len(), 1);
        assert_eq!(meta.tokens[0].actor, "human:1");
        assert_eq!(meta.tokens[0].role, Role::Editor);

        // 重复记录同一 token 不产生重复条目。
        store
            .record_token("doc_1", &token, "human:1", Role::Owner)
            .unwrap();
        let meta = store.load_meta("doc_1").unwrap().unwrap();
        assert_eq!(meta.tokens.len(), 1);
        assert_eq!(meta.tokens[0].role, Role::Owner);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn render_cache_round_trip_and_document_listing() {
        let root = temp_root("render");
        let store = FileStore::open(&root).unwrap();
        assert!(store.load_render("doc_1").unwrap().is_none());
        store.save_render("doc_1", 42, b"png-bytes").unwrap();
        let (seq, bytes) = store.load_render("doc_1").unwrap().unwrap();
        assert_eq!(seq, 42);
        assert_eq!(bytes, b"png-bytes");
        assert_eq!(store.list_documents().unwrap(), vec!["doc_1"]);
        store.append_atom("doc_2", &atom("a1", 1)).unwrap();
        assert_eq!(store.list_documents().unwrap(), vec!["doc_1", "doc_2"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn blob_store_is_content_addressed_under_root() {
        use yanshi_core::blob::BlobStore;
        let root = temp_root("blobs");
        let store = FileStore::open(&root).unwrap();
        let blobs = store.blob_store().unwrap();
        let hash = blobs.put(b"bitmap").unwrap();
        assert!(blobs.exists(&hash));
        assert_eq!(blobs.get(&hash).unwrap(), b"bitmap");
        assert!(store.root().join("blobs").join("sha256").is_dir());
        let _ = fs::remove_dir_all(&root);
    }
}
