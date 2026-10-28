//! **P-10（paint-fix-p3）**：batch 子调用的 doc_id 必须与本批一致。
//!
//! **⇒ 断什么** ✓：
//!   ① 子调用 doc_id 与本批不一致 ⇒ 整批 `invalid_argument`，两个文档的原子数都不变；
//!   ② 子调用 doc_id 与本批一致 ⇒ 正常落笔，落点文档 total 增加，另一文档不动；
//!   ③ 不带顶层 doc_id（ctx 是 CLI 缺省 `default`）、子调用带 doc_id ⇒ 明确错误，
//!      不再静默落进 default 文档，default 文档也不被建出来；
//!   ④ 子调用不带 doc_id ⇒ 沿用本批 doc_id，落点仍是顶层文档；
//!   ⑤ 子调用用 `args` 键（不是 `arguments`）带 doc_id 时同样校验。
//!
//! **⇒ 变异** ✓：删掉 `write_batch` 开头的 doc_id 校验 ⇒ ①③⑤ 红（笔落进错文档或
//!   default）；把校验改成只警告不拒绝 ⇒ ①③⑤ 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// docA（512×512）与 docB（256×256），各建一层（`layer_docA` / `layer_docB`）。
fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    for (doc_id, size) in [("docA", 512), ("docB", 256)] {
        workspace
            .create_document(
                NewDocument::new(doc_id, size, size),
                "human:1",
                "session:test",
            )
            .unwrap();
        let mut ctx = ToolContext::new(&mut workspace, doc_id, "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let layer = registry().call(
            &mut ctx,
            "create_layer",
            &json!({"layer_id": format!("layer_{doc_id}"), "name": "ground"}),
        );
        assert_eq!(layer["ok"], json!(true), "建层应成功：{layer}");
    }
    workspace
}

fn head_seq(workspace: &Workspace, doc_id: &str) -> u64 {
    workspace
        .document(doc_id)
        .map(|document| document.head_seq())
        .unwrap_or(0)
}

fn stroke(doc_id: &str, layer_id: &str) -> serde_json::Value {
    json!({"tool": "draw_stroke", "arguments": {
        "doc_id": doc_id,
        "layer_id": layer_id,
        "data": {"points": [[10.0, 10.0], [20.0, 20.0]], "size": 4.0},
    }})
}

fn run_batch(
    workspace: &mut Workspace,
    doc_id: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, "batch", &args)
}

#[test]
fn batch_rejects_subcall_doc_id_mismatch_and_leaves_both_docs_untouched() {
    let mut workspace = workspace();
    let before_a = head_seq(&workspace, "docA");
    let before_b = head_seq(&workspace, "docB");
    let calls: Vec<serde_json::Value> = (0..3).map(|_| stroke("docB", "layer_docB")).collect();
    let made = run_batch(
        &mut workspace,
        "docA",
        json!({"doc_id": "docA", "calls": calls}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "子调用 doc_id 与本批不一致时整批应被拒绝：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("invalid_argument"),
        "拒绝应报 invalid_argument：{made}"
    );
    let detail = made["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("docB") && detail.contains("docA"),
        "拒绝文案要写明两个 doc_id：{made}"
    );
    assert_eq!(
        head_seq(&workspace, "docA"),
        before_a,
        "拒绝后 docA 原子数不变"
    );
    assert_eq!(
        head_seq(&workspace, "docB"),
        before_b,
        "拒绝后 docB 原子数不变"
    );
}

#[test]
fn batch_accepts_matching_subcall_doc_id() {
    let mut workspace = workspace();
    let before_a = head_seq(&workspace, "docA");
    let before_b = head_seq(&workspace, "docB");
    let calls: Vec<serde_json::Value> = (0..3).map(|_| stroke("docA", "layer_docA")).collect();
    let made = run_batch(
        &mut workspace,
        "docA",
        json!({"doc_id": "docA", "calls": calls}),
    );
    assert_eq!(
        made["ok"],
        json!(true),
        "子调用 doc_id 一致时应正常落笔：{made}"
    );
    assert_eq!(made["count"], json!(3));
    assert_eq!(made["fail_count"], json!(0));
    assert_eq!(
        head_seq(&workspace, "docA"),
        before_a + 3,
        "落点文档 docA 应多 3 个原子"
    );
    assert_eq!(head_seq(&workspace, "docB"), before_b, "另一文档 docB 不动");
}

#[test]
fn batch_without_top_level_doc_id_rejects_subcall_doc_id() {
    // ctx doc_id 是 CLI 缺省 `default`；子调用写 docA ⇒ 不一致，整批拒绝，
    // 不再静默落进 default 文档。
    let mut workspace = workspace();
    let before_a = head_seq(&workspace, "docA");
    let calls: Vec<serde_json::Value> = (0..3).map(|_| stroke("docA", "layer_docA")).collect();
    let made = run_batch(&mut workspace, "default", json!({"calls": calls}));
    assert_eq!(
        made["ok"],
        json!(false),
        "不带顶层 doc_id、子调用带 doc_id 时应明确拒绝：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("invalid_argument"),
        "拒绝应报 invalid_argument：{made}"
    );
    let detail = made["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("docA") && detail.contains("default"),
        "拒绝文案要写明两个 doc_id：{made}"
    );
    assert_eq!(
        head_seq(&workspace, "docA"),
        before_a,
        "拒绝后 docA 原子数不变"
    );
    assert!(
        workspace.document("default").is_none(),
        "拒绝后 default 文档不应被建出来"
    );
}

#[test]
fn batch_subcall_without_doc_id_uses_batch_doc_id() {
    // 子调用不带 doc_id ⇒ 沿用本批 doc_id，落点仍是顶层文档。
    let mut workspace = workspace();
    let before_a = head_seq(&workspace, "docA");
    let before_b = head_seq(&workspace, "docB");
    let calls: Vec<serde_json::Value> = (0..3)
        .map(|_| {
            json!({"tool": "draw_stroke", "arguments": {
                "layer_id": "layer_docA",
                "data": {"points": [[10.0, 10.0], [20.0, 20.0]], "size": 4.0},
            }})
        })
        .collect();
    let made = run_batch(
        &mut workspace,
        "docA",
        json!({"doc_id": "docA", "calls": calls}),
    );
    assert_eq!(
        made["ok"],
        json!(true),
        "不带 doc_id 的子调用应沿用本批 doc_id：{made}"
    );
    assert_eq!(made["count"], json!(3));
    assert_eq!(made["fail_count"], json!(0));
    assert_eq!(
        head_seq(&workspace, "docA"),
        before_a + 3,
        "落点文档 docA 应多 3 个原子"
    );
    assert_eq!(head_seq(&workspace, "docB"), before_b, "另一文档 docB 不动");
}

#[test]
fn batch_rejects_mismatch_in_args_key_too() {
    // 子调用用 `args` 键（不是 `arguments`）带 doc_id 时同样校验。
    let mut workspace = workspace();
    let before_a = head_seq(&workspace, "docA");
    let before_b = head_seq(&workspace, "docB");
    let made = run_batch(
        &mut workspace,
        "docA",
        json!({"doc_id": "docA", "calls": [{
            "tool": "draw_stroke",
            "args": {
                "doc_id": "docB",
                "layer_id": "layer_docB",
                "data": {"points": [[10.0, 10.0], [20.0, 20.0]], "size": 4.0},
            },
        }]}),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "args 键里的 doc_id 不一致时整批也应被拒绝：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("invalid_argument"),
        "拒绝应报 invalid_argument：{made}"
    );
    assert_eq!(
        head_seq(&workspace, "docA"),
        before_a,
        "拒绝后 docA 原子数不变"
    );
    assert_eq!(
        head_seq(&workspace, "docB"),
        before_b,
        "拒绝后 docB 原子数不变"
    );
}
