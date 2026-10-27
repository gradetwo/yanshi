//! **run_gc 根集必须覆盖磁盘上未加载的文档**（GC 隔离包诚实记录的同类缺口）。
//!
//! **⇒ 缺口**：`Workspace::gc_inputs` 只遍历本进程内存里的文档
//! （`self.documents.values()`）。磁盘上已落盘、但本进程从未加载的文档，
//! 其日志引用对根集零贡献 ⇒ 只被这类文档引用的旧 blob（年龄 ≥ 7 天 TTL）
//! 会被判成孤儿删掉，文档再打开就缺像素。
//! **⇒ 场景**：工作区 A 建文档 D、入库 blob X、提交引用 X 的原子后丢弃
//! 工作区（D 只剩磁盘上的 atoms.jsonl）；工作区 B 在同一 root 上新建、
//! 从不打开 D，跑 `collect_garbage`。
//! **⇒ 断言看身份不看计数**（blob_gc.rs 的教训：一次提交会顺带产生预览
//! blob，当时无人引用 ⇒ 进孤儿级 ⇒ 全局计数会被搅乱）。
//!
//! **对照**：B 先打开 D 再跑 GC，X 必须保留（内存文档路径本就覆盖根集）。
//! **否定情形**（brief 里的“内存里持有但不落盘”）：file-store 工作区的
//! commit 同步写 atoms.jsonl（本文件 setup 里断言），所以“在内存、不在盘上”
//! 的文档只存在于 put→commit 的瞬时窗口——那时 blob 是真正的孤儿，
//! 要 7 天后才会被 GC 碰，已在窗口外；不在本测试范围。

use serde_json::json;
use yanshi_core::{now_ms, Atom, AtomKind, BlobHash};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

/// **跑完即删临时目录**（`/tmp` 是小 tmpfs，夹具留着会写满）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_gcroot_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("建根");
    path
}

/// 引用一个 blob 的采样原子（与 delete_document_blob_grace.rs 同形）。
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

/// 建一份文档、入库一个 blob、提交引用它的原子，然后丢弃工作区；
/// 返回 blob 哈希（`sha256:…` 形）。文档只剩磁盘上的 atoms.jsonl。
fn setup_disk_doc(root: &std::path::Path, doc_id: &str) -> String {
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
    // 前提断言：commit 同步落盘，丢弃工作区后日志仍在盘上。
    let log = root.join("docs").join(doc_id).join("atoms.jsonl");
    assert!(
        log.exists() && log.metadata().expect("日志应能 stat").len() > 0,
        "commit 后 atoms.jsonl 应在盘上且非空，否则本测试的前提不成立：{log:?}"
    );
    assert!(
        std::fs::read_to_string(&log)
            .expect("日志应能读")
            .contains(&hash),
        "磁盘日志里应有引用该 blob 的原子"
    );
    hash
}

/// **磁盘文档引用的旧 blob，不得被未加载它的工作区 GC 删掉**。
#[test]
fn disk_doc_blob_survives_gc_from_unloading_workspace() {
    let root = temp_root("diskdoc");
    let _cleanup = Cleanup(root.clone());
    let hash = setup_disk_doc(&root, "doc_disk");
    let parsed: BlobHash = hash.parse().expect("哈希应能解析");
    // B：同一 root 的新工作区，从不打开 D。
    let mut workspace_b = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("第二个工作区应能建");
    assert!(
        workspace_b.document_mut("doc_disk").is_err(),
        "B 从未打开 D，内存里不应有它"
    );
    // now 推后 8 天 ⇒ X 的年龄超过 7 天 TTL（service_flow.rs 同款做法）。
    let future = now_ms() + 8 * 24 * 60 * 60 * 1000;
    let (report, _plan) = workspace_b.collect_garbage(future).expect("GC 应能跑");
    assert!(
        !report.deleted.contains(&parsed),
        "磁盘文档引用的旧 blob 不得被 GC 删，删了：{report:?}"
    );
    assert!(
        workspace_b.store().exists(&parsed),
        "GC 跑完 blob 必须还在盘上"
    );
}

/// **对照：B 打开 D 后再跑 GC，X 同样必须保留**（内存文档路径本就覆盖）。
#[test]
fn opened_disk_doc_blob_survives_gc() {
    let root = temp_root("opened");
    let _cleanup = Cleanup(root.clone());
    let hash = setup_disk_doc(&root, "doc_opened");
    let parsed: BlobHash = hash.parse().expect("哈希应能解析");
    let mut workspace_b = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("第二个工作区应能建");
    workspace_b
        .open_document("doc_opened")
        .expect("B 打开 D 应成功");
    let future = now_ms() + 8 * 24 * 60 * 60 * 1000;
    let (report, _plan) = workspace_b.collect_garbage(future).expect("GC 应能跑");
    assert!(
        !report.deleted.contains(&parsed),
        "已打开文档引用的旧 blob 不得被 GC 删，删了：{report:?}"
    );
    assert!(
        workspace_b.store().exists(&parsed),
        "GC 跑完 blob 必须还在盘上"
    );
}

/// **对照：真正的孤儿照删**（修法不能把 GC 变成摆设）。
#[test]
fn true_orphan_still_collected() {
    let root = temp_root("orphan");
    let _cleanup = Cleanup(root.clone());
    let hash = setup_disk_doc(&root, "doc_orphan");
    let parsed: BlobHash = hash.parse().expect("哈希应能解析");
    // 再入库一个无人引用的 blob，同样推后 8 天。
    let workspace_b = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("第二个工作区应能建");
    let orphan_bytes: Vec<u8> = (0..16 * 16)
        .flat_map(|index| [index as u8 ^ 0x55, 11, 22, 33])
        .collect();
    let orphan = workspace_b
        .store()
        .put(&orphan_bytes)
        .expect("孤儿 blob 应能入库");
    let future = now_ms() + 8 * 24 * 60 * 60 * 1000;
    let (report, _plan) = workspace_b.collect_garbage(future).expect("GC 应能跑");
    assert!(
        report.deleted.contains(&orphan),
        "真正的孤儿必须照删，删了：{report:?}"
    );
    assert!(
        !report.deleted.contains(&parsed),
        "磁盘文档引用的旧 blob 不得被 GC 删，删了：{report:?}"
    );
}
