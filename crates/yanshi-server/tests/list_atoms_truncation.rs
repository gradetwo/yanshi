//! **`list_atoms` 的 `truncated` 不许假阳性**（第 733 轮；外部审计 S-07 第 ② 项）
//!
//! **⇒ 缺陷**：`truncated = hits.len() >= limit` ✗ ⇒
//!   ① 匹配数**正好等于** `limit`、后面**没有**更多 ⇒ 也报 `truncated: true` ✓（假阳性 ✓）；
//!   ② `limit: 0` ⇒ `0 >= 0` ⇒ 同样报截断 ✓（而一条都没返回 ✓）。
//!
//! **⇒ 为什么必须传 `include_non_stroke: true`** ✓（第 733 轮实测）：
//!   缺省**只列带 `payload.source` 的笔迹** ✓ ⇒ 我上一轮的测试拿到的 `count` 是 **0** ✗
//!   ⇒ 断言无从观察 ✓（那次判据被我删掉了 ✓，这是接上它的那次 ✓）。
//!
//! **⇒ 判据**（跨 `limit` 的恒等式 ✓）：先问全量得 `total` ⇒
//!   · `expected = limit > 0 && limit < total`
//!   · `truncated` 必须等于 `expected` ✓，且 `truncated` 与 `has_more` **一致** ✓
//! **变异**：改回 `hits.len() >= limit`（且不截断列表）⇒ `limit == total` 那条红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_la", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn list_atoms_only_reports_truncation_when_something_is_left() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_la", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);
    for index in 0..3 {
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("s{index}"),
                "data": {"points": [[4.0, 4.0 + index as f64 * 4.0], [20.0, 4.0 + index as f64 * 4.0]],
                         "size": 3.0, "hardness": 1.0,
                         "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }

    let all = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"limit": 500, "include_non_stroke": true}),
    );
    let total = all["count"].as_u64().unwrap_or(0);
    assert!(
        total >= 4,
        "整份日志至少 4 条（建文档 + 3 笔），实测 {total} ⇒ 判据测不到东西：{all}"
    );

    for limit in [0_u64, 1, 2, 3, 10] {
        let page = registry.call(
            &mut ctx,
            "list_atoms",
            &json!({"limit": limit, "include_non_stroke": true}),
        );
        assert_eq!(page["ok"], json!(true), "limit={limit}: {page}");
        let count = page["count"].as_u64().unwrap_or(99);
        let truncated = page["truncated"].as_bool().unwrap_or(false);
        let has_more = page["has_more"].as_bool().unwrap_or(false);
        assert_eq!(
            truncated, has_more,
            "limit={limit}：truncated 与 has_more 必须一致（同一件事的两个名字）"
        );
        let expected = limit > 0 && limit < total;
        assert_eq!(
            truncated, expected,
            "limit={limit}：全量 {total} 条、这一页 {count} 条，却说 truncated={truncated}（正确值 {expected}）⇒ 假阳性／假阴性"
        );
    }
}

/// **`list_atoms` 必须给出过滤之后的匹配总数**（第 779 轮；外部 `qoder` 报告 D7）
///
/// **⇒ 他们的实测**：`list_atoms(limit=2000)` ⇒ `returned=500 total=None truncated=True`
/// ⇒ **只看得见最新 500 条**、回执**不说总数** ⇒ 大图审计跑出「500 笔 PASS」的假象 ✓。
///
/// **⇒ 判据**：3 条原子、`limit=1` ⇒ `count=1` **且 `total=3`** ✓（`truncated=true` ✓）；
/// 再 `limit=10` ⇒ `count=3`、**`total=3`**、`truncated=false` ✓。
/// **⇒ 变异**：把 `total` 换成 `count`（或省略）⇒ 第一条 `total=1≠3` ⇒ 红 ✓。
#[test]
fn list_atoms_reports_the_total_number_of_matches() {
    let root = std::env::temp_dir().join(format!("yanshi_atoms_total_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace =
        Workspace::with_file_store(root, DocumentSettings::default()).expect("落盘工作区");
    workspace
        .create_document(NewDocument::new("doc_tot", 64, 64), "human:1", "s")
        .expect("create_document");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    for index in 0..3 {
        let mut ctx = ToolContext::new(&mut workspace, "doc_tot", "human:1", "s")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let response = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("s{index}"),
                "data": {"points": [[2.0, 2.0], [40.0, 40.0]], "size": 3.0, "hardness": 1.0,
                         "color": {"r": 10, "g": 20, "b": 30, "a": 255}},
            }),
        );
        assert_eq!(response["ok"], json!(true), "{response}");
    }

    let mut ctx = ToolContext::new(&mut workspace, "doc_tot", "human:1", "s").with_owner(true);
    let page = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"limit": 1, "include_non_stroke": true}),
    );
    assert_eq!(page["count"], json!(1), "一页应当只给 1 条：{page}");
    assert_eq!(
        page["total"],
        json!(4),
        "`total` 必须是过滤后的匹配总数（3 笔 ＋ 建图层等记账原子）：{page}"
    );
    assert_eq!(
        page["truncated"],
        json!(true),
        "还有更多 ⇒ truncated 必须为真：{page}"
    );

    let mut ctx = ToolContext::new(&mut workspace, "doc_tot", "human:1", "s").with_owner(true);
    let all = registry.call(
        &mut ctx,
        "list_atoms",
        &json!({"limit": 50, "include_non_stroke": true}),
    );
    assert_eq!(
        all["total"], all["count"],
        "一页装得下时 `total` 与 `count` 必须相等：{all}"
    );
    assert_eq!(all["truncated"], json!(false), "装得下就不许报截断：{all}");
}

/// 落盘工作区 + 内置资产目录指向**仓库里的 `assets/`** ✓（要画**真笔刷**的笔迹，缺省过滤才列得出 ✓）。
fn brush_workspace(root: &std::path::Path, doc_id: &str, width: u32, height: u32) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new(doc_id, width, height), "human:1", "s")
        .expect("create_document");
    workspace
}

fn brush_call(
    workspace: &mut Workspace,
    doc_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "s").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// **P2-4：`total` 只数过滤后的匹配，页满之后记账原子不许混进来**（期 2 块 1）
///
/// **⇒ 缺陷**：`read_list_atoms` 里"收到 limit+1 条就不再收"的判断放在**全部过滤器之前** ⇒
///   页满之后所有原子都被**无过滤**计入 `matched`：无 offset 时 `limit=20` 很快填满，随后
///   记账原子（建文档／建图层）也被数进 `total`，实测虚高 +3/+4 ✗。
///
/// **⇒ 判据**：3 笔真笔刷 + 记账原子，`limit=1`（缺省只列笔迹）⇒ `count=1` **且 `total=3`** ✓；
///   再逐页翻完 ⇒ 各页条数之和 `== total` 且 `atom_id` 无重无漏 ✓。
/// **⇒ 变异**：把"不再收"的判断移回过滤器之前 ⇒ `total=5`（2 个记账原子被数进来）⇒ 红 ✓。
#[test]
fn list_atoms_total_counts_only_filtered_atoms() {
    let root = std::env::temp_dir().join(format!("yanshi_atoms_p24_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = brush_workspace(&root, "doc_p24", 64, 64);
    assert_eq!(
        brush_call(
            &mut workspace,
            "doc_p24",
            "create_layer",
            json!({"layer_id": "L", "name": "L"})
        )["ok"],
        json!(true)
    );
    for index in 0..3 {
        let drawn = brush_call(
            &mut workspace,
            "doc_p24",
            "brush_stroke",
            json!({
                "layer_id": "L", "object_id": format!("s{index}"), "brush": "2B_pencil", "size": 8,
                "points": [[8.0, 8.0 + index as f64 * 12.0, 0.8], [24.0, 8.0 + index as f64 * 12.0, 0.8]],
                "color": {"r": 10, "g": 20, "b": 30, "a": 255},
                "preview": false, "wait_for_render": false,
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }

    let all = brush_call(
        &mut workspace,
        "doc_p24",
        "list_atoms",
        json!({"limit": 5000}),
    );
    assert_eq!(all["ok"], json!(true), "{all}");
    assert_eq!(all["count"], json!(3), "缺省只列笔迹：3 笔 ⇒ {all}");
    assert_eq!(
        all["total"],
        json!(3),
        "全量时 total 必须是过滤后的匹配数：{all}"
    );

    let page = brush_call(&mut workspace, "doc_p24", "list_atoms", json!({"limit": 1}));
    assert_eq!(page["count"], json!(1), "一页只给 1 条：{page}");
    assert_eq!(
        page["total"],
        json!(3),
        "小页时 total 仍必须是 3（页满之后记账原子不许被数进 total）：{page}"
    );
    assert_eq!(
        page["truncated"],
        json!(true),
        "还有更多 ⇒ truncated 必须为真：{page}"
    );

    let mut ids = std::collections::HashSet::new();
    let mut sum = 0u64;
    for offset in 0..3 {
        let p = brush_call(
            &mut workspace,
            "doc_p24",
            "list_atoms",
            json!({"limit": 1, "offset": offset}),
        );
        assert_eq!(p["ok"], json!(true), "offset={offset}：{p}");
        sum += p["count"].as_u64().unwrap_or(99);
        for atom in p["atoms"].as_array().unwrap() {
            assert!(
                ids.insert(atom["atom_id"].as_str().unwrap_or("").to_owned()),
                "翻页取到重复原子：{p}"
            );
        }
    }
    assert_eq!(sum, 3, "各页条数之和必须 == total（3）");
    assert_eq!(ids.len(), 3, "翻页无重无漏");
}

/// **D3：上限 500 → 5000，600 笔文档一次取完**（期 2 块 1）
///
/// **⇒ 判据**：600 笔真笔刷、`limit=2000` ⇒ `count=600`、`total=600`、`truncated=false` ✓；
///   `offset` 翻页（每页 200）各页条数之和 `== 600` 且无重无漏 ✓。
/// **⇒ 变异**：上限改回 500 ⇒ `count=500`、`truncated=true` ⇒ 红 ✓；
///   `total` 改成恒等于返回条数 ⇒ 小页上 `total=1≠600` 红（见上一条）✓。
#[test]
fn list_atoms_limit_5000_returns_600_strokes() {
    let root = std::env::temp_dir().join(format!("yanshi_atoms_d3_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = brush_workspace(&root, "doc_d3", 400, 400);
    assert_eq!(
        brush_call(
            &mut workspace,
            "doc_d3",
            "create_layer",
            json!({"layer_id": "L", "name": "L"})
        )["ok"],
        json!(true)
    );
    for index in 0..600 {
        let x = 20.0 + (index % 20) as f64 * 18.0;
        let y = 20.0 + (index / 20) as f64 * 12.0;
        let drawn = brush_call(
            &mut workspace,
            "doc_d3",
            "brush_stroke",
            json!({
                "layer_id": "L", "object_id": format!("s{index}"), "brush": "2B_pencil", "size": 8,
                "points": [[x, y, 0.8], [x + 10.0, y, 0.8]],
                "color": {"r": 10, "g": 20, "b": 30, "a": 255},
                "preview": false, "wait_for_render": false,
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "第 {index} 笔：{drawn}");
    }

    let all = brush_call(
        &mut workspace,
        "doc_d3",
        "list_atoms",
        json!({"limit": 2000, "want_thumb": false}),
    );
    assert_eq!(all["ok"], json!(true), "{all}");
    // **大包不打印整包 JSON**：600 条原子刷屏 ✓，只摘要三个数。
    let summary = format!(
        "count={} total={} truncated={}",
        all["count"], all["total"], all["truncated"]
    );
    assert_eq!(
        all["count"],
        json!(600),
        "limit=2000 必须返回全部 600 笔（上限已提至 5000）：{summary}"
    );
    assert_eq!(all["total"], json!(600), "total 必须是 600：{summary}");
    assert_eq!(
        all["truncated"],
        json!(false),
        "装得下就不许报截断：{summary}"
    );

    let mut ids = std::collections::HashSet::new();
    let mut sum = 0u64;
    for offset in (0..600).step_by(200) {
        let p = brush_call(
            &mut workspace,
            "doc_d3",
            "list_atoms",
            json!({"limit": 200, "offset": offset, "want_thumb": false}),
        );
        assert_eq!(
            p["ok"],
            json!(true),
            "offset={offset}：count={}",
            p["count"]
        );
        sum += p["count"].as_u64().unwrap_or(9999);
        for atom in p["atoms"].as_array().unwrap() {
            assert!(
                ids.insert(atom["atom_id"].as_str().unwrap_or("").to_owned()),
                "翻页取到重复原子：{p}"
            );
        }
    }
    assert_eq!(sum, 600, "各页条数之和必须 == total（600）");
    assert_eq!(ids.len(), 600, "翻页无重无漏");
}
