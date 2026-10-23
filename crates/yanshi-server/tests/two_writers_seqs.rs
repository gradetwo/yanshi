//! **两个写者不能悄悄写出重复 `seq`**（第 748 轮；台账三百三十的「根因待定」）
//!
//! **⇒ 症状**（外部 §238，1 小时工作受损）：一个原子与另一原子**同 `seq`** ⇒ `AtomLog::with_atoms`
//!   拒绝加载 ⇒ **整份文档打不开** ✓（止血已做：打开时重编号 ✓；**根因一直待定** ✓）。
//!
//! **⇒ 本判据要回答的问题**：**两个 `Workspace` 指向同一个落盘根、各自打开同一文档、各画一笔**
//!   ⇒ 磁盘上的 `atoms.jsonl` 会不会出现**重复 `seq`**？✓（台账里的候选 ①：两个进程各有自己的 `next_seq` ✓）
//!
//! **⇒ 判据**：上述情形跑完之后，重新打开该文档**必须成功**，且日志里的 `seq` **必须唯一** ✓。
//! **变异**：让第二个写者复用陈旧的 `next_seq` ⇒ 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace(root: &std::path::Path) -> Workspace {
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default()).expect("落盘工作区")
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_two", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn draw(workspace: &mut Workspace, object_id: &str) {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = context(workspace);
    let response = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": object_id,
            "data": {"points": [[4.0, 4.0], [20.0, 20.0]], "size": 3.0, "hardness": 1.0,
                     "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
}

#[test]
fn two_writers_on_the_same_root_do_not_duplicate_sequences() {
    let root = std::env::temp_dir().join(format!("yanshi_two_writers_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");

    // ① 第一个写者建文档并画一笔 —— **工作区留在作用域里** ✓（模拟"进程一直在跑"✓）
    let mut first = workspace(&root);
    first
        .create_document(
            NewDocument::new("doc_two", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    draw(&mut first, "s_first");

    // ② **模拟"另一个进程刚写了一笔"** ✓：直接往盘上的 `atoms.jsonl` **追加**一条原子 ✓
    //   （这正是台账三百三十的候选 ①：另一个进程有它**自己的 `next_seq`** ✓）
    //   ⇒ 而本进程**仍在内存里**（不会重读）⇒ 下一次提交**必然**复用同一个号 ✓。
    //   **⚠ 我前两版都写错了**：先后打开／另一工作区打开都会**重新读盘** ⇒ 自然不撞 ⇒ 判据测不到东西 ✗。
    let journal = root.join("docs").join("doc_two").join("atoms.jsonl");
    let text = std::fs::read_to_string(&journal).expect("读日志");
    let last = text
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .expect("至少一条原子");
    let mut atom: serde_json::Value = serde_json::from_str(last).expect("解析最后一条原子");
    atom["id"] = json!("01ZZZZZZZZZZZZZZZZZZZZZZZZ");
    atom["seq"] = json!(atom["seq"].as_u64().expect("seq") + 1);
    atom["actor"] = json!("human:other-process");
    let mut appended = text.clone();
    if !appended.ends_with('\n') {
        appended.push('\n');
    }
    appended.push_str(&atom.to_string());
    appended.push('\n');
    std::fs::write(&journal, appended).expect("写回日志");

    // ③ 本进程（**内存里没有那条**）再画一笔 ⇒ **必须成功** ✓，而 `seq` **必须不撞** ✓
    //   （第 752 轮修法：分配前把 `next_seq` 推到"盘上最后一条 + 1" ✓ —— 不是拒绝写 ✗）
    draw(&mut first, "s_after_other_process");

    // ④ 第三个读者重新打开 ⇒ 不许因为重复 seq 打不开 ✓
    let mut reader = workspace(&root);
    let opened = reader.open_or_create(
        NewDocument::new("doc_two", 32, 32),
        "human:3",
        "session:reader",
    );
    assert!(
        opened.is_ok(),
        "两个写者之后文档打不开了（重复 seq ⇒ 整份作废）：{:?}",
        opened.err()
    );
    // ⑤ 日志里的 seq 必须唯一 ✓
    let document = reader.document("doc_two").expect("已打开");
    let mut seqs: Vec<u64> = document.log().atoms().iter().map(|atom| atom.seq).collect();
    let total = seqs.len();
    seqs.sort_unstable();
    seqs.dedup();
    assert_eq!(
        seqs.len(),
        total,
        "日志里有重复 seq（{total} 条原子、只有 {} 个不同 seq）⇒ 两个写者各写了同一个号",
        seqs.len()
    );

    // ⑥ **但盘上的原始文件里确实有重复** ✓（第 748 轮实测）：
    //   台账三百三十的根因**在这里被复现** ✓ —— 另一个进程占掉的号，本进程照写不误 ✓；
    //   之所以还能打开，是 `Document::open` 里的 `repair_duplicate_seqs`（第 1505 轮的止血 ✓）
    //   把重复的**推到了 max+1** ✓ ⇒ **止血有效 ✓，而根因（写时不校验盘上已到几号）仍在** ✗。
    //   **⇒ 这正是"止血不是修复"的可量证据** ✓：读侧干净、写侧仍会撞 ✓。
    let raw = std::fs::read_to_string(&journal).expect("再读日志");
    let mut raw_seqs: Vec<u64> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter_map(|atom| atom["seq"].as_u64())
        .collect();
    let raw_total = raw_seqs.len();
    raw_seqs.sort_unstable();
    raw_seqs.dedup();
    eprintln!(
        "盘上原始日志：{raw_total} 条原子、{} 个不同 seq（重复 {} 个）",
        raw_seqs.len(),
        raw_total - raw_seqs.len()
    );
    assert_eq!(
        raw_total,
        raw_seqs.len(),
        "盘上出现重复 seq（{raw_total} 条原子、{} 个不同）⇒ 分配前没与盘上对齐",
        raw_seqs.len()
    );
    // ⑦ **另一个写者的那一笔必须"进得来"** ✓（第 754 轮）：本进程的 `get_log` 要能看到它 ✓
    //   （第 752 轮只推号 ⇒ 看不到 ⇒ 要重开文档 ✗；这一条正是那次留下的缺口 ✓）。
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = context(&mut first);
    let log = registry.call(&mut ctx, "get_log", &json!({"limit": 50}));
    let seen: Vec<String> = log["atoms"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|entry| entry["actor"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        seen.iter().any(|actor| actor == "human:other-process"),
        "本进程的 `get_log` 看不到另一个写者那一笔 ⇒ 没把盘上的原子并进内存：{seen:?}"
    );
    drop(first);
    // **第 748 轮这里断的是"盘上必须有重复"** ✓（复现用 ✓）；第 752 轮修好之后
    //   **反过来**：盘上**必须没有重复** ✓ ⇒ 那条复现断言已换成下面的唯一性断言 ✓。
}

/// **删掉文档后，"上次看到的盘上长度"也必须被清掉**（第 767 轮）
///
/// **⇒ 缺陷（第 762 轮我自己引入的）**：`disk_len_seen` 按 `doc_id` 记 ✓ ⇒ 删除后条目留着 ✗
/// ⇒ ① 小内存泄漏；② **同 id 重建后，若新文件长度恰好等于旧长度 ⇒ 缓存命中 ⇒ 跳过读盘**
/// ⇒ **漏掉别的写者的原子**（低概率、静默 ✓）。
///
/// **⇒ 判据**：建两个带持久化的文档（各有一次提交 ⇒ 各有条目 ✓）⇒ 删掉一个 ⇒
/// 条目数必须**减一** ✓。**变异**：去掉 `self.disk_len_seen.remove(doc_id)` ⇒ 条目数不变 ⇒ 红 ✓。
#[test]
fn deleting_a_document_clears_its_journal_length_cache() {
    let root = std::env::temp_dir().join(format!("yanshi_disklen_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = workspace(&root);

    // **`draw()` 里写死了 `doc_two`** ✓ ⇒ 这里用能指定文档的调用（第 767 轮踩到过 ✓）
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    for doc in ["doc_a", "doc_b"] {
        workspace
            .create_document(NewDocument::new(doc, 32, 32), "human:1", "session:test")
            .expect("create_document");
        let mut ctx = ToolContext::new(&mut workspace, doc, "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let response = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("s_{doc}"),
                "data": {"points": [[4.0, 4.0], [20.0, 20.0]], "size": 3.0, "hardness": 1.0,
                         "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
            }),
        );
        assert_eq!(response["ok"], json!(true), "{doc}: {response}");
    }
    let before = workspace.disk_len_cache_entries();
    assert!(
        before >= 2,
        "两个文档各提交过 ⇒ 至少两条缓存条目，实测 {before}"
    );

    workspace.delete_document("doc_a").expect("删除");
    let after = workspace.disk_len_cache_entries();
    assert_eq!(
        after,
        before - 1,
        "删掉 doc_a 之后缓存条目应当是 {} 条，实测 {after} ⇒ 删除没清（第 762 轮引入的泄漏）",
        before - 1
    );
}
