//! **X-5：`get_stroke_log` 的 `total` 只数 live 原子** ✓
//!
//! **判据** ✓：
//! ① 2 笔之后 ⇒ `total == 2`、`reverted_excluded == 0`、条目 2 条。
//! ② `undo_last` 撤 1 笔之后 ⇒ `total == 1`、`reverted_excluded == 1`、条目只剩第 1 笔、
//!    恒等式 `total + reverted_excluded == 2`；分页（`limit=1`）与 `total` 一致。
//! ③ `reapply` 把那笔接回来之后 ⇒ `total == 2`、`reverted_excluded == 0`。
//! ④ 两处视图对同一批 live 原子一致（口径不同，数法各守其义）：`list_atoms` 缺省
//!    只列带 `source` 的原子（draw 笔没有 `source` ⇒ `total == 0`、
//!    `hidden_non_stroke == 3`，既有行为）；`include_non_stroke=true` 时 `total == 3`
//!   （live 的第 1 笔 + Revert 原子本身 + create_document 原子），第 2 笔不在其中。
//!
//! **变异** ✓：把“跳过已撤销原子”那行删掉 ⇒ ② 红（`total` 回到 2）；
//! 把 `reverted_excluded` 的累加删掉 ⇒ ② 红（恒等式破）；
//! 把最近动作改成“只认 Revert”（不认 Reapply）⇒ ③ 红（`total` 仍是 1）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_x5", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn context(workspace: &mut Workspace) -> ToolContext<'_> {
    ToolContext::new(workspace, "doc_x5", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(&mut ctx, tool, &args)
}

fn draw(workspace: &mut Workspace, object_id: &str, y: f64) -> serde_json::Value {
    call(
        workspace,
        "draw_stroke",
        json!({
            "layer_id": "layer_default",
            "object_id": object_id,
            "data": {"points": [[20.0, y], [180.0, y]], "size": 5.0,
                     "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    )
}

fn stroke_log(workspace: &mut Workspace, args: serde_json::Value) -> serde_json::Value {
    call(workspace, "get_stroke_log", args)
}

/// **① 2 笔之后 ⇒ `total == 2`** ✓。
#[test]
fn two_strokes_log_total_two() {
    let mut workspace = workspace();
    let first = draw(&mut workspace, "x5_a", 50.0);
    assert_eq!(first["ok"], json!(true), "{first}");
    let second = draw(&mut workspace, "x5_b", 150.0);
    assert_eq!(second["ok"], json!(true), "{second}");

    let log = stroke_log(&mut workspace, json!({"limit": 10}));
    assert_eq!(log["total"], json!(2), "{log}");
    assert_eq!(log["reverted_excluded"], json!(0), "{log}");
    assert_eq!(log["entries"].as_array().unwrap().len(), 2, "{log}");
}

/// **② 撤 1 笔 ⇒ `total == 1`、`reverted_excluded == 1`、恒等式成立** ✓。
#[test]
fn after_undo_total_counts_only_live_strokes() {
    let mut workspace = workspace();
    let first = draw(&mut workspace, "x5_a", 50.0);
    let second = draw(&mut workspace, "x5_b", 150.0);
    assert_eq!(first["ok"], json!(true), "{first}");
    assert_eq!(second["ok"], json!(true), "{second}");

    let undone = call(&mut workspace, "undo_last", json!({"count": 1}));
    assert_eq!(undone["undone_count"], json!(1), "{undone}");

    let log = stroke_log(&mut workspace, json!({"limit": 10}));
    assert_eq!(log["total"], json!(1), "撤销后 total 应只数 live 笔：{log}");
    assert_eq!(log["reverted_excluded"], json!(1), "{log}");
    assert_eq!(
        log["total"].as_u64().unwrap() + log["reverted_excluded"].as_u64().unwrap(),
        2,
        "total + reverted_excluded 应等于全部匹配数：{log}"
    );
    let entries = log["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1, "{log}");
    assert_eq!(
        entries[0]["atom_id"], first["atom_id"],
        "剩下的应是第 1 笔：{log}"
    );
    // 分页与 `total` 一致（`limit=1` 时 1 条即全部，无下一页）。
    let page = stroke_log(&mut workspace, json!({"limit": 1, "offset": 0}));
    assert_eq!(page["returned"], json!(1), "{page}");
    assert_eq!(page["total"], json!(1), "{page}");
    assert_eq!(page["next_offset"], json!(null), "{page}");
    assert_eq!(page["truncated"], json!(false), "{page}");
}

/// **③ `reapply` 接回来 ⇒ `total` 回到 2** ✓。
#[test]
fn after_reapply_total_returns_to_two() {
    let mut workspace = workspace();
    let _first = draw(&mut workspace, "x5_a", 50.0);
    let second = draw(&mut workspace, "x5_b", 150.0);
    let undone = call(&mut workspace, "undo_last", json!({"count": 1}));
    assert_eq!(undone["undone_count"], json!(1), "{undone}");

    let reapplied = call(
        &mut workspace,
        "reapply",
        json!({"atom_id": second["atom_id"].clone()}),
    );
    assert_eq!(reapplied["ok"], json!(true), "{reapplied}");

    let log = stroke_log(&mut workspace, json!({"limit": 10}));
    assert_eq!(
        log["total"],
        json!(2),
        "reapply 回来后 total 应回到 2：{log}"
    );
    assert_eq!(log["reverted_excluded"], json!(0), "{log}");
    assert_eq!(log["entries"].as_array().unwrap().len(), 2, "{log}");
}

/// **④ 两处视图对同一批 live 原子一致** ✓（口径不同，数法各守其义）。
///
/// `get_stroke_log` 的 `total` 只数 live 笔（== 1）；`list_atoms` 缺省只列带
/// `source` 的原子——`draw_stroke` 的净荷里没有 `source`，所以缺省视图里
/// `total == 0`、`hidden_non_stroke == 3`（第 1 笔 + create_document 原子 +
/// Revert 原子；第 2 笔在 hidden 之前就因“已撤销”被跳过，不计入 hidden；
/// 这是 `list_atoms` 缺省过滤的既有行为，本期不动，只在这里如实断言）。
/// `include_non_stroke=true` 时 `list_atoms` 列出全部 live 原子：live 的第 1 笔 +
/// Revert 原子本身 + create_document 原子（== 3），其中 live 笔的 atom_id 与
/// `get_stroke_log` 的条目一致。
#[test]
fn list_atoms_total_agrees_with_stroke_log_total() {
    let mut workspace = workspace();
    let first = draw(&mut workspace, "x5_a", 50.0);
    let second = draw(&mut workspace, "x5_b", 150.0);
    let undone = call(&mut workspace, "undo_last", json!({"count": 1}));
    assert_eq!(undone["undone_count"], json!(1), "{undone}");

    let log = stroke_log(&mut workspace, json!({"limit": 10}));
    assert_eq!(log["total"], json!(1), "{log}");

    // 缺省视图：draw 笔没有 `source` ⇒ 全被藏掉（既有行为，本期不动）。
    let atoms_default = call(&mut workspace, "list_atoms", json!({"limit": 10}));
    assert_eq!(atoms_default["total"], json!(0), "{atoms_default}");
    assert_eq!(
        atoms_default["hidden_non_stroke"],
        json!(3),
        "第 1 笔 + create_document 原子 + Revert 原子被藏掉（第 2 笔因已撤销在 hidden 之前就被跳过）：{atoms_default}"
    );

    // 全量视图：live 原子 = 第 1 笔 + Revert 原子本身 + create_document 原子；
    // 第 2 笔已被撤销，不在其中。
    let atoms = call(
        &mut workspace,
        "list_atoms",
        json!({"limit": 10, "include_non_stroke": true}),
    );
    assert_eq!(atoms["total"], json!(3), "{atoms}");
    let ids: Vec<&str> = atoms["atoms"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|atom| atom["atom_id"].as_str())
        .collect();
    assert!(
        ids.contains(&first["atom_id"].as_str().unwrap()),
        "live 的第 1 笔应在全量视图里：{atoms}"
    );
    assert!(
        !ids.contains(&second["atom_id"].as_str().unwrap()),
        "已撤销的第 2 笔不应在全量视图里：{atoms}"
    );
}
