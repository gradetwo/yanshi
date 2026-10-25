//! C2 判据（期 2）：文档内存预算闸 + LRU 卸载 + 可观测。
//! 预算经 env 注入（`YANSHI_DOC_MEMORY_BUDGET_MB`，沿 GC 闸
//! 「可调常量才可测」形态）；每份文档的估算字节从
//! `loaded_documents()` 实测读出后反推预算值，不对原子数做
//! 脆弱假设。env 是进程全局——本文件全部用例由一把 Mutex
//! 串行化，并在结束时清掉 env。
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **⇒ 临时目录必须在测试结束时删掉** ✓（第 930 轮；`check-tests-clean-temp` 抓到的 ✓）
/// **⇒ 为什么** ✗：`/tmp` 是 **1.9 GB tmpfs** ✓ ⇒ 漏删会把 `bash` 都撑死 ✗
pub struct TempCleanup(pub std::path::PathBuf);
impl Drop for TempCleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_c2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn call(workspace: &mut Workspace, doc_id: &str, name: &str, args: &Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, name, args)
}

/// 建一份带 `fills` 个填充原子的文档（估算 ≈ (初始原子 + fills)
/// × 300 KB + 缓存实测）。
fn make_doc(workspace: &mut Workspace, doc_id: &str, fills: usize) {
    workspace
        .create_document(NewDocument::new(doc_id, 64, 64), "human:1", "session:test")
        .unwrap_or_else(|error| panic!("建文档 {doc_id} 失败：{error}"));
    let layers = call(workspace, doc_id, "list_layers", &json!({}));
    let layer = layers["layers"][0]["layer_id"]
        .as_str()
        .expect("新文档应有图层")
        .to_owned();
    for index in 0..fills {
        let out = call(
            workspace,
            doc_id,
            "fill_region",
            &json!({
                "layer_id": layer,
                "shape": {"type": "rect", "x": (index % 8) * 8, "y": 0, "w": 8, "h": 8},
                "color": [10, 20, 30, 255]
            }),
        );
        assert_eq!(out["ok"], json!(true), "fill_region 失败：{out}");
    }
}

fn loaded_ids(workspace: &Workspace) -> Vec<String> {
    workspace
        .loaded_documents()
        .into_iter()
        .map(|info| info.doc_id)
        .collect()
}

fn set_budget_mb(mb: u64) {
    std::env::set_var("YANSHI_DOC_MEMORY_BUDGET_MB", mb.to_string());
}

fn clear_env() {
    std::env::remove_var("YANSHI_DOC_MEMORY_BUDGET_MB");
    std::env::remove_var("YANSHI_DOC_MAX_LOADED");
}

/// 预算反推：让「已载入 + 待载入」恰好超限、卸一份即达标。
/// 返回应设的 MB 数（向下取整，但保证卸一份后 ≤ 预算）。
fn budget_forcing_one_eviction(workspace: &Workspace, incoming_estimate: u64) -> u64 {
    let infos = workspace.loaded_documents();
    let loaded: u64 = infos.iter().map(|info| info.estimated_bytes).sum();
    let total = loaded + incoming_estimate;
    let smallest = infos
        .iter()
        .map(|info| info.estimated_bytes)
        .min()
        .expect("至少一份已载入文档");
    // 卸一份后 loaded' = loaded - smallest；要求 loaded' <= budget < total。
    let want = total - 1;
    assert!(
        loaded - smallest <= want,
        "反推前提不成立：卸一份仍超（loaded {loaded} smallest {smallest}）"
    );
    let mb = want / (1_024 * 1_024);
    assert!(
        (loaded - smallest) <= mb * 1_024 * 1_024 && mb * 1_024 * 1_024 < total,
        "MB 取整后区间失效（loaded {loaded} smallest {smallest} total {total} mb {mb}）"
    );
    mb
}

#[test]
fn doc_budget_evicts_lru_idle_document() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    let root = temp_dir("lru");
    let _cleanup = TempCleanup(root.clone());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default()).unwrap();
    make_doc(&mut workspace, "doc_a", 12);
    make_doc(&mut workspace, "doc_b", 12);
    make_doc(&mut workspace, "doc_c", 12);
    make_doc(&mut workspace, "doc_d", 12);
    // 热开 A 一次：LRU 次序变为 B（最久未用）< C < D < A。
    workspace.open_document("doc_a").unwrap();
    // D 作为待载入：先关掉它（持久化在盘）。
    assert!(workspace.close_document("doc_d"));
    let incoming = workspace
        .loaded_documents()
        .into_iter()
        .next()
        .map(|info| info.estimated_bytes)
        .unwrap();
    let mb = budget_forcing_one_eviction(&workspace, incoming);
    set_budget_mb(mb);

    workspace.open_document("doc_d").unwrap();

    let ids = loaded_ids(&workspace);
    assert!(
        !ids.contains(&"doc_b".to_string()),
        "最久未用的 B 应被卸载，实际在场：{ids:?}"
    );
    for kept in ["doc_a", "doc_c", "doc_d"] {
        assert!(ids.contains(&kept.to_string()), "{kept} 应在场：{ids:?}");
    }
    let stats = workspace.doc_eviction_stats();
    assert_eq!(stats.evictions, 1, "恰好卸载一份：{stats:?}");
    assert_eq!(stats.last_evicted_doc.as_deref(), Some("doc_b"));
    assert!(
        stats
            .last_reason
            .as_deref()
            .unwrap_or("")
            .contains("内存预算"),
        "卸载原因应点名内存预算：{stats:?}"
    );
    clear_env();
}

#[test]
fn doc_budget_never_evicts_busy_document() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    let root = temp_dir("busy");
    let _cleanup = TempCleanup(root.clone());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default()).unwrap();
    make_doc(&mut workspace, "doc_a", 12);
    make_doc(&mut workspace, "doc_b", 12);
    make_doc(&mut workspace, "doc_c", 12);
    // 热开 B：LRU 次序 A（最久未用）< C < B。若不查 busy，
    // 卸载候选正是 A。
    workspace.open_document("doc_b").unwrap();
    assert!(workspace.close_document("doc_c"));
    let incoming = workspace
        .loaded_documents()
        .into_iter()
        .next()
        .map(|info| info.estimated_bytes)
        .unwrap();
    let mb = budget_forcing_one_eviction(&workspace, incoming);
    set_budget_mb(mb);
    // A 进入 inflight（使用中）——预算闸必须跳过它、改卸 B。
    let inflight = workspace.inflight().clone();
    let _op_guard = inflight
        .begin("doc_a", "test_tool", "human:1", "session:test", 1_000)
        .expect("inflight 登记应成功");

    workspace.open_document("doc_c").unwrap();

    let ids = loaded_ids(&workspace);
    assert!(
        ids.contains(&"doc_a".to_string()),
        "busy 的 A 绝不能被卸载：{ids:?}"
    );
    assert!(
        !ids.contains(&"doc_b".to_string()),
        "应改卸次久未用且空闲的 B：{ids:?}"
    );
    let stats = workspace.doc_eviction_stats();
    assert_eq!(stats.evictions, 1, "{stats:?}");
    assert!(stats.skipped_busy >= 1, "跳过 busy 必须计数：{stats:?}");
    clear_env();
}

#[test]
fn doc_budget_exhausted_errors_with_numbers() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    let root = temp_dir("exhausted");
    let _cleanup = TempCleanup(root.clone());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default()).unwrap();
    make_doc(&mut workspace, "doc_a", 12);
    make_doc(&mut workspace, "doc_b", 12);
    make_doc(&mut workspace, "doc_c", 12);
    assert!(workspace.close_document("doc_c"));
    // 全部已载入文档都 busy + 预算压到 1 MB：无可卸、必超限。
    set_budget_mb(1);
    let inflight = workspace.inflight().clone();
    let _guard_a = inflight
        .begin("doc_a", "test_tool", "human:1", "session:test", 1_000)
        .expect("inflight A");
    let _guard_b = inflight
        .begin("doc_b", "test_tool", "human:1", "session:test", 1_000)
        .expect("inflight B");

    let error = workspace
        .open_document("doc_c")
        .expect_err("全部 busy 且超预算必须响亮拒绝，不许硬开到 OOM");
    assert_eq!(error.code, yanshi_core::ErrorCode::ResourceExhausted);
    let detail = format!("{error}");
    assert!(
        detail.contains("上限 1 MB"),
        "错误必须给出上限数字：{detail}"
    );
    assert!(
        detail.contains("估算内存将达"),
        "错误必须给出估算数字：{detail}"
    );
    assert_eq!(
        workspace.doc_eviction_stats().evictions,
        0,
        "拒绝路径不许偷偷卸载"
    );
    clear_env();
}

#[test]
fn closed_document_reopens_identical() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    let root = temp_dir("reopen");
    let _cleanup = TempCleanup(root.clone());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default()).unwrap();
    make_doc(&mut workspace, "doc_a", 12);
    make_doc(&mut workspace, "doc_b", 12);
    let state_before = serde_json::to_string(
        &serde_json::to_value(workspace.document("doc_a").unwrap().state().clone()).unwrap(),
    )
    .unwrap();
    let head_before = workspace.document("doc_a").unwrap().head_seq();
    // 用预算把 A 卸掉（A 是 LRU），再放宽预算重开。预算按实测
    // 估算反推：A+B 超限、卸掉 A 后 B 单独装得下。
    let b_estimate = workspace
        .loaded_documents()
        .into_iter()
        .find(|info| info.doc_id == "doc_b")
        .map(|info| info.estimated_bytes)
        .unwrap();
    assert!(workspace.close_document("doc_b"));
    let mb = budget_forcing_one_eviction(&workspace, b_estimate);
    set_budget_mb(mb);
    workspace.open_document("doc_b").unwrap();
    assert!(
        !loaded_ids(&workspace).contains(&"doc_a".to_string()),
        "A 应已被预算卸载"
    );
    clear_env();
    workspace.open_document("doc_a").unwrap();
    let state_after = serde_json::to_string(
        &serde_json::to_value(workspace.document("doc_a").unwrap().state().clone()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        state_before, state_after,
        "卸载是纯内存动作：重开后状态必须与卸载前逐字节一致"
    );
    assert_eq!(head_before, workspace.document("doc_a").unwrap().head_seq());
    clear_env();
}

/// **RSS 修复 paint-rss：预算闸也接在新建上**.
///
/// 修前闸只在 `open_document_inner` 里，新建走 `create_document` 全程绕闸 ⇒
/// 连续新建 30 个文档、预算 2GB 也一次不触发卸载，直接顶到 OOM（定位报告
/// §5 第 3 条）。本判据钉住：预算恰好"卸一份即达标"时，新建一份必须卸载
/// 最久未用的空闲文档（而不是等打开时才卸、更不是直接放行到 OOM）。
#[test]
fn doc_budget_evicts_lru_idle_document_on_create() {
    let _guard = ENV_LOCK.lock().unwrap();
    clear_env();
    let root = temp_dir("create");
    let _cleanup = TempCleanup(root.clone());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default()).unwrap();
    make_doc(&mut workspace, "doc_a", 12);
    make_doc(&mut workspace, "doc_b", 12);
    make_doc(&mut workspace, "doc_c", 12);
    // 新建文档磁盘原子数为 0、入量估算为 0：预算取"已载入总量 - 最小一份 + 1 字节"
    // 的下取整——新建时已载入总量超预算，卸最久未用的一份即达标。
    let incoming = 0u64;
    let mb = budget_forcing_one_eviction(&workspace, incoming);
    set_budget_mb(mb);

    workspace
        .create_document(NewDocument::new("doc_new", 64, 64), "human:1", "session:test")
        .unwrap_or_else(|error| panic!("预算内新建应成功：{error}"));

    let ids = loaded_ids(&workspace);
    assert!(
        !ids.contains(&"doc_a".to_string()),
        "新建时最久未用的 A 应被卸载，实际在场：{ids:?}"
    );
    for kept in ["doc_b", "doc_c", "doc_new"] {
        assert!(ids.contains(&kept.to_string()), "{kept} 应在场：{ids:?}");
    }
    let stats = workspace.doc_eviction_stats();
    assert_eq!(stats.evictions, 1, "恰好卸载一份：{stats:?}");
    assert!(stats
        .last_reason
        .as_deref()
        .unwrap_or("")
        .contains("内存预算"));
    clear_env();
}
