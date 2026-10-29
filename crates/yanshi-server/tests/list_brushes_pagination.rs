//! `list_brushes` 分页：先过滤再切片，缺省行为不变。
//!
//! 覆盖：
//! ① 不传 limit/offset ⇒ 完整列表＋分页字段与旧口径一致（count == 数组长、
//!   total == count、has_more 为假、next_offset 为空）；
//! ② limit/offset 翻页 ⇒ 两页名字恰好是裸调用列表的对应切片、互不重叠、
//!   next_offset 链式衔接；
//! ③ filter＋分页 ⇒ 先过滤再切片（分页结果等于过滤后全量的对应切片，
//!   不是先切片再过滤）；
//! ④ limit 为 0 ⇒ 空页＋真实 total（只取总数的探针，不报错）；
//! ⑤ offset 超出总数 ⇒ 空页、不报错、has_more 为假；
//! ⑥ 非法值（负数、小数、字符串）⇒ invalid_argument，不许静默当缺省；
//! ⑦ 整数浮点（5.0）⇒ 接受为 5（与 get_log 的计数参数同一口径）。
//!
//! 每条断言的输入都真实触发被查情况（非空集恒真）：裸调用总数先断言 > 0，
//! 过滤用资产里确实有 11 支的 pencil（断言过滤后 ≥ 6 支才做切片对照）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 跑完即删临时目录。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// 落盘工作区 + 内置资产目录指向仓库里的 `assets/`（要列真笔刷）。
fn workspace(root: &std::path::Path, doc_id: &str) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new(doc_id, 300, 200), "human:1", "s")
        .expect("create_document");
    workspace
}

fn call(workspace: &mut Workspace, doc_id: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "s").with_owner(true);
    registry().call(&mut ctx, "list_brushes", &args)
}

fn names(response: &Value) -> Vec<String> {
    response["brushes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|brush| brush["name"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// **并行测试各用各的目录**：同一测试二进制里的用例共享进程 id，
/// 只用 pid 作目录名时后建文档的用例会撞上“文档已存在于磁盘”而误红
/// （盒上首跑 5 过 2 败正是此因，不是分页逻辑问题）。
static NEXT_DIR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn setup() -> (Cleanup, Workspace) {
    let n = NEXT_DIR.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("yanshi_lbpage_{}_{}", std::process::id(), n));
    let cleanup = Cleanup(root.clone());
    let workspace = workspace(&root, "doc_lbpage");
    (cleanup, workspace)
}

#[test]
fn bare_call_returns_full_list_with_consistent_pagination_fields() {
    let (_cleanup, mut ws) = setup();
    let bare = call(&mut ws, "doc_lbpage", json!({}));
    assert_eq!(bare["ok"], json!(true), "{bare}");
    let total = bare["total"].as_u64().expect("裸调用必须带 total");
    assert!(total > 0, "资产里应有笔刷，否则下面全是空集恒真：{bare}");
    assert_eq!(
        bare["count"],
        json!(total),
        "裸调用 count 应等于 total：{bare}"
    );
    assert_eq!(
        names(&bare).len() as u64,
        total,
        "裸调用数组长度应等于 total：{bare}"
    );
    assert_eq!(bare["offset"], json!(0), "{bare}");
    assert_eq!(bare["has_more"], json!(false), "{bare}");
    assert!(bare["next_offset"].is_null(), "{bare}");
    assert_eq!(bare["brushes_available"], json!(true), "{bare}");
}

#[test]
fn limit_and_offset_page_through_the_bare_list() {
    let (_cleanup, mut ws) = setup();
    let bare = call(&mut ws, "doc_lbpage", json!({}));
    let bare_names = names(&bare);
    let total = bare_names.len();
    assert!(total >= 20, "翻页对照需要至少 20 支：{total}");

    let page1 = call(&mut ws, "doc_lbpage", json!({"limit": 10}));
    assert_eq!(page1["ok"], json!(true), "{page1}");
    assert_eq!(page1["count"], json!(10), "{page1}");
    assert_eq!(page1["total"], json!(total), "{page1}");
    assert_eq!(page1["offset"], json!(0), "{page1}");
    assert_eq!(page1["has_more"], json!(true), "{page1}");
    assert_eq!(page1["next_offset"], json!(10), "{page1}");
    assert_eq!(
        names(&page1),
        bare_names[..10],
        "第 1 页应等于裸列表前 10 支"
    );

    let next = page1["next_offset"]
        .as_u64()
        .expect("第 1 页必须给下一页起点");
    let page2 = call(&mut ws, "doc_lbpage", json!({"limit": 10, "offset": next}));
    assert_eq!(page2["ok"], json!(true), "{page2}");
    assert_eq!(
        names(&page2),
        bare_names[10..20],
        "第 2 页应等于裸列表第 10..20 支"
    );
    assert_eq!(page2["offset"], json!(10), "{page2}");
    let page1_names = names(&page1);
    let page2_names = names(&page2);
    assert!(
        page1_names.iter().all(|name| !page2_names.contains(name)),
        "两页不得重叠：{page1_names:?} vs {page2_names:?}"
    );
}

#[test]
fn filter_applies_before_slicing() {
    let (_cleanup, mut ws) = setup();
    let filtered = call(&mut ws, "doc_lbpage", json!({"filter": "pencil"}));
    assert_eq!(filtered["ok"], json!(true), "{filtered}");
    let filtered_names = names(&filtered);
    assert!(
        filtered_names.len() >= 6,
        "pencil 过滤后应至少 6 支才做切片对照：{filtered_names:?}"
    );
    let paged = call(
        &mut ws,
        "doc_lbpage",
        json!({"filter": "pencil", "limit": 3}),
    );
    assert_eq!(paged["ok"], json!(true), "{paged}");
    assert_eq!(paged["total"], json!(filtered_names.len()), "{paged}");
    assert_eq!(
        names(&paged),
        filtered_names[..3],
        "分页结果应等于过滤后全量的前 3 支"
    );
    let paged2 = call(
        &mut ws,
        "doc_lbpage",
        json!({"filter": "pencil", "limit": 3, "offset": 3}),
    );
    assert_eq!(
        names(&paged2),
        filtered_names[3..6],
        "过滤＋offset 的页应等于过滤后全量的第 3..6 支"
    );
}

#[test]
fn limit_zero_is_a_count_only_probe() {
    let (_cleanup, mut ws) = setup();
    let bare = call(&mut ws, "doc_lbpage", json!({}));
    let total = bare["total"].as_u64().expect("裸调用必须带 total");
    let probe = call(&mut ws, "doc_lbpage", json!({"limit": 0}));
    assert_eq!(probe["ok"], json!(true), "{probe}");
    assert_eq!(probe["count"], json!(0), "{probe}");
    assert!(
        probe["brushes"]
            .as_array()
            .is_some_and(|list| list.is_empty()),
        "limit 为 0 必须给空页：{probe}"
    );
    assert_eq!(
        probe["total"],
        json!(total),
        "limit 为 0 也必须给真实 total：{probe}"
    );
    assert_eq!(probe["has_more"], json!(true), "{probe}");
    assert_eq!(probe["next_offset"], json!(0), "{probe}");
    // brushes_available 看的是总数，不是本页条数：空页也不该报“无笔刷”。
    assert_eq!(probe["brushes_available"], json!(true), "{probe}");
}

#[test]
fn offset_beyond_total_gives_an_empty_page_not_an_error() {
    let (_cleanup, mut ws) = setup();
    let bare = call(&mut ws, "doc_lbpage", json!({}));
    let total = bare["total"].as_u64().expect("裸调用必须带 total");
    let page = call(&mut ws, "doc_lbpage", json!({"offset": total + 50}));
    assert_eq!(page["ok"], json!(true), "{page}");
    assert_eq!(page["count"], json!(0), "{page}");
    assert_eq!(page["total"], json!(total), "{page}");
    assert_eq!(page["has_more"], json!(false), "{page}");
    assert!(page["next_offset"].is_null(), "{page}");
}

#[test]
fn invalid_pagination_args_are_rejected() {
    let (_cleanup, mut ws) = setup();
    for args in [
        json!({"limit": -1}),
        json!({"offset": -1}),
        json!({"limit": 1.5}),
        json!({"offset": 2.5}),
        json!({"limit": "10"}),
    ] {
        let response = call(&mut ws, "doc_lbpage", args.clone());
        assert_eq!(
            response["ok"],
            json!(false),
            "非法分页参数被接受了：{args} ⇒ {response}"
        );
        assert_eq!(
            response["error_code"],
            json!("invalid_argument"),
            "错误码必须是 invalid_argument：{args} ⇒ {response}"
        );
    }
}

#[test]
fn integral_float_limit_is_accepted_as_an_integer() {
    let (_cleanup, mut ws) = setup();
    let response = call(&mut ws, "doc_lbpage", json!({"limit": 5.0}));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert_eq!(response["count"], json!(5), "{response}");
}
