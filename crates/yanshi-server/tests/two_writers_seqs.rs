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

    // ③ 本进程（**内存里没有那条**）再画一笔 ⇒ 会复用刚被"别的进程"占掉的号 ✓
    draw(&mut first, "s_after_other_process");
    drop(first);

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
    assert!(
        raw_total > raw_seqs.len(),
        "这一版没能复现「写时撞号」（盘上无重复）⇒ 判据没测到东西，先看注入是否生效"
    );
}
