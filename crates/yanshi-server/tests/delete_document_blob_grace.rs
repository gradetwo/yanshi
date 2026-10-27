//! **删文档不得误删别的服务刚写入的 blob**（GC 隔离分期；双服务同 root）。
//!
//! **⇒ 根因**：`delete_document` 按**本进程**可见的引用集删 blob——
//! 别的进程刚 `put`、还没 `import` 的 blob 在这里是"零引用" ⇒ 被删，
//! 对方 `import_image` 取不到文件（A2 并行梯度里文件没了的那一半）。
//! **⇒ 修法**：删 blob 只删年龄 ≥ `DELETE_DOCUMENT_BLOB_GRACE_SECONDS` 的；
//! 新鲜的留下来当孤儿，由 TTL GC 回收；回执用 `blobs_deferred_fresh` 说清。
//! **变异**：把 `delete_document` 里的年龄门槛去掉 ⇒ ① 红 ✓。

use serde_json::json;
use std::time::{Duration, SystemTime};
use yanshi_core::{Atom, AtomKind, BlobHash};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

/// **跑完即删临时目录** ✓（`/tmp` 是小 tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let path =
        std::env::temp_dir().join(format!("yanshi_delgrace_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建根");
    path
}

/// 引用一个 blob 的采样原子（与 `conflict_layer_durability.rs` 同形）。
fn sampling_atom(blob: &str, layer_id: &str, object_id: &str, session: &str) -> Atom {
    Atom::new(
        AtomKind::CreateObject,
        format!("human:{session}"),
        session,
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "sampling": true,
            "bitmap": {"blob_hash": blob, "size": 256, "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 4, "y": 4, "w": 4, "h": 4},
            "width": 4,
            "height": 4,
        }),
    )
}

/// 建一份文档、入库一个 blob、让文档引用它；返回工作区与 blob 哈希（`sha256:…` 形）。
fn setup_doc_with_blob(root: &std::path::Path, doc_id: &str) -> (Workspace, String) {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应能建");
    workspace
        .create_document(NewDocument::new(doc_id, 32, 32), "human:a", "session:a")
        .expect("建文档应成功");
    let bytes: Vec<u8> = (0..16 * 16)
        .flat_map(|index| [index as u8, 40, 90, 255])
        .collect();
    let hash = workspace
        .store()
        .put(&bytes)
        .expect("blob 应能入库")
        .to_string();
    let create_layer = Atom::new(
        AtomKind::CreateLayer,
        "human:a",
        "session:a",
        json!({"layer_id": "L", "name": "L", "type": "raster", "z_index": 1}),
    );
    workspace
        .commit(doc_id, create_layer, "human:a", true)
        .expect("建图层应成功");
    let object = sampling_atom(&hash, "L", "obj", "session:a");
    workspace
        .commit(doc_id, object, "human:a", true)
        .expect("引用 blob 的原子应能提交");
    (workspace, hash)
}

/// CAS 文件路径（与 `FsBlobStore::path_of` 同布局：`blobs/sha256/<h2>/<h4>/<hex>`）。
fn blob_file_path(root: &std::path::Path, hash: &str) -> std::path::PathBuf {
    let hex = hash.strip_prefix("sha256:").expect("哈希应带 sha256: 前缀");
    root.join("blobs")
        .join("sha256")
        .join(&hex[0..2])
        .join(&hex[2..4])
        .join(hex)
}

/// 把 blob 文件的 mtime 往回拨 `secs` 秒（造"旧 blob"）。
fn backdate(path: &std::path::Path, secs: u64) {
    let old = SystemTime::now() - Duration::from_secs(secs);
    std::fs::File::open(path)
        .expect("blob 文件应在盘上")
        .set_times(std::fs::FileTimes::new().set_modified(old))
        .expect("回拨 mtime 应成功");
}

/// **刚 put 的 blob 不得随文档删除而被删** ✓：
///
/// 别的服务刚 `put`、还没 `import` 的 blob，在删文档的服务看来是"零引用"；
/// 删了它，对方的 `import_image` 就取不到文件 ✗ ⇒ 新鲜的必须留下来 ✓
///（当孤儿，由 TTL GC 回收 ✓）。
#[test]
fn fresh_blob_survives_delete_document() {
    let root = temp_root("fresh");
    let _cleanup_1 = Cleanup(root.clone());
    let (mut workspace, hash) = setup_doc_with_blob(&root, "doc_fresh");
    let parsed: BlobHash = hash.parse().expect("哈希应能解析");
    assert!(workspace.store().exists(&parsed), "删前 blob 应在盘上");
    let response = workspace
        .delete_document("doc_fresh")
        .expect("删文档应成功");
    assert_eq!(
        response["blobs_deleted"].as_u64(),
        Some(0),
        "刚 put 的 blob 不得被删，实测回执：{response}"
    );
    assert_eq!(
        response["blobs_deferred_fresh"].as_u64(),
        Some(1),
        "被拦下的新鲜 blob 必须在回执里说清，实测回执：{response}"
    );
    assert!(
        workspace.store().exists(&parsed),
        "删完刚 put 的 blob 必须还在（别的服务正要用它 import）"
    );
}

/// **旧 blob 照删** ✓：年龄门槛只拦新鲜的，不拦正常回收 ✓
///（删文档立刻还空间的行为不能丢 ✓）。
#[test]
fn old_blob_is_still_deleted() {
    let root = temp_root("old");
    let _cleanup_1 = Cleanup(root.clone());
    let (mut workspace, hash) = setup_doc_with_blob(&root, "doc_old");
    let parsed: BlobHash = hash.parse().expect("哈希应能解析");
    backdate(&blob_file_path(&root, &hash), 3600);
    let response = workspace.delete_document("doc_old").expect("删文档应成功");
    assert_eq!(
        response["blobs_deleted"].as_u64(),
        Some(1),
        "旧 blob 应当随文档删除而删，实测回执：{response}"
    );
    assert_eq!(
        response["blobs_deferred_fresh"].as_u64(),
        Some(0),
        "旧 blob 不应计入 deferred，实测回执：{response}"
    );
    assert!(!workspace.store().exists(&parsed), "旧 blob 删完就不该还在");
}
