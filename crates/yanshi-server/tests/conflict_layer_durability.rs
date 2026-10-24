//! **冲突图层的持久性** ✓（第 1440 轮审计）：`ensure_conflict_layer` 往内存日志里
//! 追加了一枚系统 `create_layer` 原子并广播 ✓ —— 可它**不是**那次提交的结果原子 ✓，
//! 冲突路径整体返回 `Err` ⇒ `Workspace::commit_timed` 的 `journal(...)` 被 `?` 跳过 ✗
//! ⇒ **内存里有、磁盘上没有** ✗ ⇒ 重启后错误回执里承诺过的冲突图层**凭空消失** ✗。
//!
//! 本文件先让症状**当场红** ✓（修复前），修完后必须绿 ✓。

use serde_json::json;
use yanshi_core::{Atom, AtomKind};
use yanshi_server::{persist::FileStore, DocumentSettings, NewDocument, Workspace};

/// **跑完即删临时目录** ✓（第 818 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_root(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yanshi_conflict_durability_{}_{}",
        name,
        std::process::id()
    ));
    let _cleanup_1 = Cleanup(path.clone());
    let _ = std::fs::remove_dir_all(&path);
    path
}

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

/// **冲突图层要经得起"重启"** ✓：错误回执里给出 `conflict_layer_id` ⇒ 客户端会往它重提交 ✓
/// ⇒ 它必须进了**磁盘日志** ✗不是只在内存里存在一次会话 ✓（落盘工作区的权威是磁盘 ✓）。
#[test]
fn the_conflict_layer_survives_a_reopen_from_disk() {
    let root = temp_root("reopen");
    let conflict_layer_id;
    {
        let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
            .expect("落盘工作区应能建");
        workspace
            .create_document(NewDocument::new("doc_c", 16, 16), "human:a", "session:a")
            .expect("建文档应成功");
        let bytes: Vec<u8> = (0..16 * 16)
            .flat_map(|index| [index as u8, 40, 90, 255])
            .collect();
        let blob = workspace
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
            .commit("doc_c", create_layer, "human:a", true)
            .expect("建正式层应成功");
        let first = sampling_atom(&blob, "L", "a_obj", "a");
        workspace
            .commit("doc_c", first, "human:a", true)
            .expect("第一次采样性替换应成功");
        let second = sampling_atom(&blob, "L", "b_obj", "b");
        let error = workspace
            .commit("doc_c", second, "human:b", true)
            .expect_err("同一 Region 的第二次采样性替换应被拒");
        assert_eq!(error.code, yanshi_core::ErrorCode::Conflict, "{error:?}");
        conflict_layer_id = error
            .context
            .extra
            .get("conflict_layer_id")
            .and_then(serde_json::Value::as_str)
            .expect("错误里应带冲突图层 id")
            .to_owned();
        // 基线里内存**已经**有这枚图层（服务端建好并广播过 ✓）——症状只剩磁盘没跟上 ✗。
        let state = workspace.document("doc_c").expect("文档应开着").state();
        assert!(
            state.layers.contains_key(&conflict_layer_id),
            "冲突图层应先存在于内存（这是广播与回执承诺的形状 ✓）"
        );
    }
    // **模拟重启**：同一 root 重建工作区 ⇒ 磁盘是权威 ✓。
    {
        let _ = root;
        let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
            .expect("重启工作区应能建");
        workspace.open_document("doc_c").expect("既有文档应能重开");
        let state = workspace.document("doc_c").expect("重开后文档应在").state();
        assert!(
            state.layers.contains_key(&conflict_layer_id),
            "重开后冲突图层「{conflict_layer_id}」必须还在 —— 回执里承诺过的东西要落盘 ✗不能只在内存活一次会话"
        );
    }
}

/// **重开之后的第一次提交只许追加** ✗新原子 ✓，**不许把历史重写一遍** ✗（第 1440 轮 ✓）。
///
/// **为什么单独一条** ✓：水位落盘要求"打开时把水位推到 HEAD" ✓
/// —— 漏了那一步的失败方式是**重复追加**（每条历史原子再进 `atoms.jsonl` 一遍 ✓）✗
/// 上面那条测试拦不住它（重复行反而让冲突图层"还在" ✓）。
/// **恒等式口径** ✓：`磁盘行数 == 内存原子数` 且 **seq 无重复** ⇒ 与具体原子数无关 ✓
/// （内核 ②：断关系 ✗不断快照数字 ✓）。
#[test]
fn reopening_then_committing_appends_only_the_new_atom() {
    let root = temp_root("append_only");
    {
        let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
            .expect("落盘工作区应能建");
        workspace
            .create_document(NewDocument::new("doc_w", 16, 16), "human:a", "session:a")
            .expect("建文档应成功");
        for index in 0..3u32 {
            let atom = Atom::new(
                AtomKind::CreateLayer,
                "human:a",
                "session:a",
                json!({"layer_id": format!("L{index}"), "name": index.to_string(), "type": "raster", "z_index": index + 1}),
            );
            workspace
                .commit("doc_w", atom, "human:a", true)
                .expect("建层应成功");
        }
        // **只建了文档、一笔未提交** ✓：`create_document` 原子也必须已经落盘 ✓
        // （变异：去掉建文档处的 `flush_journal` ⇒ 这里重开就报"不存在" ✗ ⇒ 本测试红 ✓）。
        workspace
            .create_document(NewDocument::new("doc_v", 16, 16), "human:a", "session:a")
            .expect("建第二份文档应成功");
    }
    // **重启 + 重开 + 再提交一枚** ✓。
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("重启工作区应能建");
    workspace
        .open_document("doc_v")
        .expect("无提交的文档也应已落盘、能重开");
    workspace.open_document("doc_w").expect("既有文档应能重开");
    let extra = Atom::new(
        AtomKind::CreateLayer,
        "human:a",
        "session:a",
        json!({"layer_id": "L9", "name": "L9", "type": "raster", "z_index": 9}),
    );
    workspace
        .commit("doc_w", extra, "human:a", true)
        .expect("重开后的提交应成功");
    // **直接读磁盘** ✓（不经内存 ✓）：行数与 seq 唯一性都断在**权威文件**上 ✓。
    let on_disk = FileStore::open(&root)
        .expect("文件存储应能打开")
        .load_atoms("doc_w")
        .expect("磁盘日志应能读");
    let in_memory: Vec<yanshi_core::Seq> = workspace
        .document("doc_w")
        .expect("文档应开着")
        .log()
        .atoms()
        .iter()
        .map(|atom| atom.seq)
        .collect();
    let disk_seqs: Vec<yanshi_core::Seq> = on_disk.iter().map(|atom| atom.seq).collect();
    let mut unique = disk_seqs.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        disk_seqs.len(),
        unique.len(),
        "磁盘日志里 seq 不许重复（重写一遍历史 = 落盘水位坏了 ✗）：磁盘 {disk_seqs:?}"
    );
    assert_eq!(
        disk_seqs, in_memory,
        "磁盘应与内存逐条一致（同序 ✗同数 ✓）—— 少一条 = 漏写 ✗，多一条 = 重复追加 ✗"
    );
}
