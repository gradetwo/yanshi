//! **待办 ① 的判据（第 1 段 ✓）**：打开文档时"**从磁盘读了多少原子**"这个读数**是真的** ✓
//!
//! **为什么需要它** ✗（详见 `docs/design/vermeer-acceptance.md` §49 之补 ✓）：
//! 待办 ①（快照落盘 ＋ 加载走"快照 ＋ 尾巴"）是**性能／存储**改动 ✓ ——
//! 若判据断言**时间**或**内存峰值** ✗ ⇒ 极易抖动 ✗（本机与 CI 不可比 ✓）⇒
//! 所以改断言"**打开时读入了多少**" ✓：**确定性** ✓、与机器快慢无关 ✓、且**变异能红** ✓。
//!
//! **本文件是两段式** ✓：
//!   * **第 1 段（本文件现在就有 ✓）**：还没有快照 ⇒ 读数**必须等于原子总数** ✓；
//!   * **第 2 段（等①的快照落地后再加 ✓）**：有快照 ⇒ 读数**必须明显变小** ✓
//!     ⇒ 并把加载改回**全量读**做**变异** ⇒ 这一段**必须红** ✓。
use serde_json::json;
use yanshi_core::{Atom, AtomKind, Seq};
use yanshi_server::persist::FileStore;
use yanshi_server::{DocumentSettings, Workspace};

fn atom(id: &str, seq: Seq, kind: AtomKind) -> Atom {
    let mut atom =
        Atom::new(kind, "human:1", "session:a", json!({"layer_id": "layer_1"})).with_id(id);
    atom.seq = seq;
    atom
}

fn temp_root(tag: &str) -> std::path::PathBuf {
    let root =
        std::env::temp_dir().join(format!("yanshi-openreadout-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// **第 1 段** ✓：没有快照时，`atoms_read_on_open()` **必须等于**磁盘上的原子数 ✓。
#[test]
fn atoms_read_on_open_equals_the_whole_log_without_a_snapshot() {
    let root = temp_root("who1e");
    let doc = "doc_readout";
    let store = FileStore::open(&root).unwrap();
    // 第一颗用 `CreateDocument`（真实文档的开头就是它 ✓），其余用 `CreateLayer` ✓。
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    let total: Seq = 9;
    for seq in 2..=total {
        store
            .append_atom(doc, &atom(&format!("a{seq}"), seq, AtomKind::CreateLayer))
            .unwrap();
    }
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    let document = workspace.open_document(doc).expect("打开应当成功 ✓");
    let read = document.atoms_read_on_open();
    println!("  磁盘上原子 {total} 颗 ⇒ 打开时读到 {read} 颗 ✓");
    assert_eq!(
        read, total as u64,
        "还没有快照 ⇒ 打开必须读入**全部**原子 ✓（实测 {read}／{total} ✓）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **`load_atoms_since` 本身的行为** ✓（待办 ① 的"尾巴"就是靠它 ✓）：
/// 只收 `seq > since` 的那些 ✓ ⇒ `since = 0` 时与 `load_atoms` **等价** ✓。
#[test]
fn load_atoms_since_returns_only_the_tail() {
    let root = temp_root("since7");
    let doc = "doc_since";
    let store = FileStore::open(&root).unwrap();
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    for seq in 2..=9 {
        store
            .append_atom(doc, &atom(&format!("a{seq}"), seq, AtomKind::CreateLayer))
            .unwrap();
    }
    let all = store.load_atoms(doc).unwrap();
    assert_eq!(all.len(), 9, "全部应当是 9 颗 ✓");
    let since0 = store.load_atoms_since(doc, 0).unwrap();
    assert_eq!(since0.len(), 9, "since=0 ⇒ 与 load_atoms 等价 ✓");
    let tail = store.load_atoms_since(doc, 7).unwrap();
    assert_eq!(
        tail.len(),
        2,
        "seq>7 ⇒ 只剩 8／9 两颗 ✓（实测 {} ✓）",
        tail.len()
    );
    assert!(tail.iter().all(|a| a.seq > 7), "尾巴里不许混进旧原子 ✗");
    let none = store.load_atoms_since(doc, 9).unwrap();
    assert!(none.is_empty(), "since=最末 ⇒ 空 ✓");
    let _ = std::fs::remove_dir_all(&root);
}
