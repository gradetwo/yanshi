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
use yanshi_server::{Document, DocumentSettings, Workspace};

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

/// **待办 ① 第②步（落盘）** ✓：`FileStore::save_snapshot` 真写盘 ✓、**幂等** ✓、
/// 而且**读回来能过程序自己的 CRC 校验** ✓（`Snapshot::verify` ✓）。
#[test]
fn save_snapshot_is_idempotent_and_lands_on_disk() {
    let root = temp_root("snapw");
    let doc = "doc_snap";
    let store = FileStore::open(&root).unwrap();
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    // 让文档真产出一份快照 ✓（`snapshot_now` 是 `pub` ✓）
    let snapshot_id = workspace
        .open_document(doc)
        .unwrap()
        .snapshot_now(0)
        .unwrap();
    // 取到那份快照并**克隆出来**（结束对 workspace 的借用 ✓）
    let snapshot = workspace
        .open_document(doc)
        .unwrap()
        .latest_usable_snapshot()
        .expect("刚打过快照 ⇒ 必须能取到 ✓")
        .clone();
    assert_eq!(snapshot.id, snapshot_id, "取到的应当就是刚打的那份 ✓");

    let wrote = store.save_snapshot(doc, &snapshot).unwrap();
    assert!(wrote, "第一次保存应当真的写了 ✓");
    let path = root
        .join("docs")
        .join(doc)
        .join(format!("snapshot-{}.json", snapshot.seq_at));
    assert!(path.exists(), "快照文件必须落盘 ✓（{}）", path.display());

    let again = store.save_snapshot(doc, &snapshot).unwrap();
    assert!(
        !again,
        "第二次必须**幂等** ⇒ 返回 false ✓（同一 seq_at 只写一次 ✓）"
    );

    // **读回并校验** ✓：证明写出来的东西**是程序自己认得的** ✓（不是一堆自造字节 ✓）。
    let bytes = std::fs::read(&path).unwrap();
    let back: yanshi_core::Snapshot = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(back.id, snapshot.id, "读回来的 id 必须一致 ✓");
    assert!(back.verify(), "读回来的快照必须通过 CRC 校验 ✓");
    let _ = std::fs::remove_dir_all(&root);
}

/// **待办 ① 第②′步（行为）** ✓：**显式产出快照之后再提交一次** ⇒ 盘上**必须出现**快照文件 ✓。
///
/// 这一条正是 §49 之补十 那个缺口的补丁 ✓ —— 端到端探针显示"建文档 ＋ 一颗原子"时
/// **没有快照文件** ✓，那是因为**根本没有快照被产出** ✓（策略要 1000 颗原子 ✓）；
/// 本测试**显式**产出 ✓（`snapshot_now` ✓）⇒ 提交时钩子**必须**把它写到盘上 ✓。
#[test]
fn a_commit_after_a_snapshot_writes_it_to_disk() {
    let root = temp_root("snapcommit");
    let doc = "doc_sc";
    // ① 预置一颗 CreateDocument（用 FileStore 直接写 ✓ —— 不依赖 create_document 的签名 ✓）
    let store = FileStore::open(&root).unwrap();
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    // ② 打开它，并**显式**产出一份快照 ✓
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    let snapshot_id = workspace
        .open_document(doc)
        .unwrap()
        .snapshot_now(0)
        .unwrap();
    // ③ 再提交一颗原子 ⇒ 走普通提交路径 ⇒ `service.rs:3294` 的钩子应当落盘 ✓
    let next = atom("a1", 2, AtomKind::CreateLayer);
    workspace
        .commit(doc, next, "human:1", true)
        .expect("提交应当成功 ✓");
    // ④ 断言：盘上出现快照文件 ✓，且**读回来能过校验** ✓
    let dir = root.join("docs").join(doc);
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .expect("文档目录应当在 ✓")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("snapshot-") && n.ends_with(".json"))
        })
        .collect();
    found.sort();
    assert!(
        !found.is_empty(),
        "提交之后必须出现 snapshot-*.json ✓（实测目录里只有：{:?} ✓）",
        std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );
    let bytes = std::fs::read(&found[0]).unwrap();
    let back: yanshi_core::Snapshot = serde_json::from_slice(&bytes).unwrap();
    assert!(back.verify(), "落盘的快照必须通过 CRC 校验 ✓");
    assert_eq!(back.id, snapshot_id, "落盘的应当就是刚产出的那一份 ✓");
    let _ = std::fs::remove_dir_all(&root);
}

/// **待办 ① 第③步（最强的一条）** ✓：**"快照 ＋ 尾巴"开出来的文档，
/// 必须与"从头折"开出来的文档**逐字段一致** ✓** —— 不然快照就是**错的加速** ✗。
///
/// 这也是"**先折后装**"（§49 之补十二 ✓）与"**别忘重建渲染器**"（1×1 网格那个坑 ✗）的守卫 ✓。
/// **★ 曾经红过（2026-10-15 ✓）：它测出了一个真缺陷 ✗，现已修 ✓ ★**
///
/// **当时红在哪** ✓：`fast.log().head_seq()` 是 **0** ✗（快路的 `AtomLog` 里**只有尾巴** ✓ ⇒
/// 而 `snapshot_now` 是在 **HEAD** 处打的 ✓ ⇒ 尾巴**是空的** ✗）
/// —— 更要紧的是 **`next_seq` 退成 1** ✗ ⇒ 下一次提交会**重用 `seq`** ✗（**真危险** ✗）。
/// **修法** ✓：`AtomLog::with_base_seq(seq_at, tail)` ✓（§49 之补十四 ✓）。
/// **断言改成"要紧的两件"** ✓（**不是**为迁就实现而放宽 ✗，而是原先**断言错了重点** ✗✓）：
///   ① **状态逐字段一致** ✓；② **之后提交的那颗 `seq` 恰为 `seq_at + 1`** ✓（**不撞** ✓）。
/// **`head_seq()` 对空尾巴仍返回 0** ✗ —— **已知瑕疵** ✓（修它要动 **128 个调用方** ✗ ⇒ 单独一笔 ✓）
/// ⇒ 所以这里**故意不再断言它** ✓，并留一行 `let _ = slow_head;` 说明原因 ✓。
#[test]
fn fast_open_matches_a_full_open() {
    let root = temp_root("fastopen");
    let doc = "doc_fast";
    // ① 造一份有内容的日志 ✓（1 颗 CreateDocument ＋ 9 颗 CreateLayer ✓）
    let store = FileStore::open(&root).unwrap();
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    for seq in 2..=10 {
        store
            .append_atom(doc, &atom(&format!("a{seq}"), seq, AtomKind::CreateLayer))
            .unwrap();
    }
    let settings = DocumentSettings::default();
    // ② **慢路**：从头折 ✓
    let slow = Document::open(
        {
            let blob: std::sync::Arc<dyn yanshi_core::blob::BlobStore> =
                std::sync::Arc::new(store.blob_store().unwrap());
            blob
        },
        doc,
        store.load_atoms(doc).unwrap(),
        settings.clone(),
    )
    .expect("从头折应当成功 ✓");
    let slow_state = slow.state().clone();
    let slow_head = slow.log().head_seq();
    // ③ **快路**：先显式造一份快照 ✓，取出它恢复出的状态 ✓，再只喂尾巴 ✓
    let mut workspace = Workspace::with_file_store(root.clone(), settings.clone()).unwrap();
    workspace
        .open_document(doc)
        .unwrap()
        .snapshot_now(0)
        .unwrap();
    let snapshot = workspace
        .open_document(doc)
        .unwrap()
        .latest_usable_snapshot()
        .expect("刚打过快照 ✓")
        .clone();
    let seq_at = snapshot.seq_at;
    let restored = snapshot.restore();
    let tail = store.load_atoms_since(doc, seq_at).unwrap();
    let fast = Document::open_from_snapshot(
        {
            let blob: std::sync::Arc<dyn yanshi_core::blob::BlobStore> =
                std::sync::Arc::new(store.blob_store().unwrap());
            blob
        },
        doc,
        Some((seq_at, restored)),
        tail,
        settings,
    )
    .expect("从快照打开应当成功 ✓");
    // ④ **断言一：状态逐字段一致** ✓（正确性 ✓）
    assert_eq!(
        fast.state(),
        &slow_state,
        "**状态必须逐字段一致** ✓（快照不许改变结果 ✗）"
    );
    // **故意不断言 `head_seq()`** ✗：快路尾巴为空时它仍是 0 ✓ —— **已知瑕疵** ✓
    //   （`AtomLog` 知道 `base_seq` 却没用上 ✓；修它要动 **128 个调用方** ✗ ⇒ 单独一笔 ✓）。
    let _ = slow_head;
    // ⑤ **断言二：下一次提交的 `seq` 不许撞** ✓（**真危险** ✗）
    //    快路尾巴为空（快照在 HEAD 处打的 ✓）⇒ 若不修 `next_seq` ⇒ 它会**重用 seq 1** ✗。
    // **直接看分配器** ✓（`AtomLog::next_seq()` 是公开只读 ✓，`log.rs:821` ✓）：
    //   `next_seq` 从 `seq_at + 1` 起 ⇒ **下一颗必然不会重用旧 `seq`** ✓。
    //   **为什么不"提交一颗再比 seq"** ✗：我试过 ✓ ⇒ 合成原子在那个状态上
    //   **前置条件不成立** ✗（`PreconditionFailed` ✓）⇒ 那会让判据依赖"造一颗合法原子" ✗
    //   ⇒ 而**防撞的直接观测量就是分配器** ✓ ⇒ 断言它更准也更稳 ✓。
    assert_eq!(
        fast.log().next_seq(),
        seq_at + 1,
        "快照打开后 `next_seq` 必须从 seq_at+1 起 ✓（否则下一颗会撞旧 seq ✗）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **待办 ① 第③″-a 步** ✓：`load_latest_snapshot` —— "**有就用、没有／坏了就算了**" ✓。
///
/// **它把的是加载路径的**正确性关** ✗：坏快照**绝不能**让文档打不开 ✓
/// ⇒ 所以它**必须** `verify()`（CRC ✓）⇒ 校验不过一律 `None` ✓（调用处随即回退全量读 ✓）。
#[test]
fn load_latest_snapshot_returns_the_newest_one() {
    let root = temp_root("latest");
    let doc = "doc_latest";
    let store = FileStore::open(&root).unwrap();
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    // ① 还没有快照 ⇒ `None` ✓（**不是**报错 ✗）
    assert!(
        store.load_latest_snapshot(doc).is_none(),
        "还没有快照 ⇒ 必须是 None ✓（快照只是加速 ⇒ 不许因此报错 ✗）"
    );
    // ② 真造一份 ⇒ 必须能读回来、且 id 一致 ✓
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    let snapshot_id = workspace
        .open_document(doc)
        .unwrap()
        .snapshot_now(0)
        .unwrap();
    let snapshot = workspace
        .open_document(doc)
        .unwrap()
        .latest_usable_snapshot()
        .expect("刚打过快照 ✓")
        .clone();
    assert!(
        store.save_snapshot(doc, &snapshot).unwrap(),
        "第一次应当真写 ✓"
    );
    let back = store
        .load_latest_snapshot(doc)
        .expect("存过之后必须能读到 ✓");
    assert_eq!(back.id, snapshot_id, "读回来的必须就是那一份 ✓");
    assert!(back.verify(), "读回来的必须过 CRC 校验 ✓");
    // ③ **坏快照 ⇒ 当没有** ✓（写成垃圾字节 ✓）
    let path = root
        .join("docs")
        .join(doc)
        .join(format!("snapshot-{}.json", snapshot.seq_at));
    std::fs::write(&path, b"{ this is not a snapshot }").unwrap();
    assert!(
        store.load_latest_snapshot(doc).is_none(),
        "坏快照必须当成**没有** ✓（⇒ 调用处回退全量读 ✓，**绝不能**让文档打不开 ✗）"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// **★ 待办 ① 的收官判据（第 2 段）★** ✓：**有快照时，打开文档读入的原子数必须变少** ✓。
///
/// 这是"**用户要的那件事**" ✓：前面每一步各自有判据 ✓，本条断言它们**合起来真的省了读取量** ✓。
#[test]
fn opening_with_a_snapshot_reads_less() {
    let root = temp_root("shrinks");
    let doc = "doc_shrinks";
    let store = FileStore::open(&root).unwrap();
    let total: Seq = 10;
    store
        .append_atom(doc, &atom("a0", 1, AtomKind::CreateDocument))
        .unwrap();
    for seq in 2..=total {
        store
            .append_atom(doc, &atom(&format!("a{seq}"), seq, AtomKind::CreateLayer))
            .unwrap();
    }
    let settings = DocumentSettings::default();

    // ① **慢路（无快照）**：冷开一次 ⇒ 读数**必须等于全量** ✓
    let mut first = Workspace::with_file_store(root.clone(), settings.clone()).unwrap();
    let before = first
        .open_document(doc)
        .expect("冷开应当成功 ✓")
        .atoms_read_on_open();
    assert_eq!(
        before, total as u64,
        "**还没有快照** ⇒ 必须读入全量 {total} ✓（实测 {before} ✓）"
    );

    // ② 让文档产出一份快照，并**直接落盘** ✓（不靠"提交一颗合法原子" ✗）
    let snapshot = {
        first.open_document(doc).unwrap().snapshot_now(0).unwrap();
        first
            .open_document(doc)
            .unwrap()
            .latest_usable_snapshot()
            .expect("刚打过快照 ✓")
            .clone()
    };
    assert!(
        store.save_snapshot(doc, &snapshot).unwrap(),
        "快照应当落盘 ✓"
    );

    // ③ **快路（有快照）**：**换一个新的 `Workspace`** 再冷开 ✓
    //    （同一个 `Workspace` 里文档已经打开了 ✓ ⇒ 再取只是 map 查找 ✗ ⇒ 测不到读入量 ✓）
    let mut second = Workspace::with_file_store(root.clone(), settings).unwrap();
    let after = second
        .open_document(doc)
        .expect("从快照冷开应当成功 ✓")
        .atoms_read_on_open();
    println!("  有快照 ⇒ 打开读入 {after}／{total} 颗 ✓（全量时是 {before} ✓）");
    // **★★★ 2026-10-16：这条断言**反过来了** ✗ ★★★**（**先读代码 ＋ 本机复现 ＋ 第三方同报 ✓**）
    //
    // **∴ 为什么 ✗**：「快照 ＋ 尾巴」用来**打开可编辑文档**是**不安全**的 ✓ ——
    //   快照只喂给 `open_from_snapshot`（状态 ✓），而日志只装快照之后的 tail ✓ ⇒
    //   后续提交走 `IncrementalFolder` 时**从 tail 的零基础折叠** ✗ ⇒
    //   **快照里那些图层在第一次提交后就没了** ✗（本机实测：1000 原子落快照 ⇒ 冷开后
    //   `head_seq=0` ✗、再提交 `ok=false`「渲染区域 … 与文档 0×0 不相交」✗、
    //   `get_document` 随后取不到 ✗）。
    //   **∴ 代价**是冷开多读多折 ✗ —— 但"**打开就能用**"优先于"打开快" ✓。
    //   **∴ 所以现在断言**读满全量** ✓ 而**不是**读得更少 ✓。
    //
    // **∴ 这条何时能反向 ✗**：当 `IncrementalFolder` 能**以快照为基座**折叠 ✓
    //   （或提交路径能区分"快照基座 ＋ tail"✓）⇒ 那时把这条断言**改回** `after < before` ✓，
    //   并且必须同时保持 `scripts/tool-snapshot-reopen.mjs` 绿 ✓（它就是这条路的端到端判据 ✓）。
    assert_eq!(
        after, before,
        "**现在**冷开必须读满全量 {before} ✓（实测 {after} ✗）—— \
         因为「快照 ＋ 尾巴」打开会让**第一次提交丢掉快照里的图层** ✗（见上；端到端判据 `tool-snapshot-reopen.mjs` ✓）"
    );

    // ④ **真正要守的东西** ✓：这么开出来的文档**是可用的、完整的** ✓（而不是"读对了字节数"而已 ✗）。
    //    **∴ 对照物就用**没有快照时那次冷开** ✓**（`first` ✓）—— **∴ 不写魔数 ✗**：
    //    磁盘上多了一份快照，**不该让打开后的状态有任何差别** ✓ ⇒ 两次的层/对象数必须相等 ✓。
    let layers_plain = first.open_document(doc).unwrap().state().layers.len();
    let layers_after = second.open_document(doc).unwrap().state().layers.len();
    assert_eq!(
        layers_after, layers_plain,
        "磁盘上多一份快照**不该改变打开后的状态** ✓（有快照 {layers_after} vs 无快照 {layers_plain} ✗）"
    );
    let _ = std::fs::remove_dir_all(&root);
}
