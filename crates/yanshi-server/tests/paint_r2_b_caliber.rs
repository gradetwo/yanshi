//! 名画临摹第 2 轮修复程序 B 期（口径与文案簇）改动相关测试。
//!
//! 覆盖：项 5（batch 顶层误传子工具参数的归属提示）、项 7/8（export 回执的
//! 派生口径分栏）、项 10（list_brushes 涂抹标记）、项 11（gradient_blend 缺省
//! steps）、项 13（list_effects 的 impasto 别名条目）。
//!
//! 每条断言的输入都真实触发被查情况（非空集恒真）：batch 测三个 key（命中
//! layer_id、命中通用子工具参数 size、纯拼写错误 layerr_id）；export 测一
//! 个存储里在、一个存储里不在的 tile 引用；笔刷测涂抹与非涂抹各一支。

use serde_json::{json, Value};
use yanshi_server::tools::ALL_TOOLS;
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

/// 落盘工作区 + 内置资产目录指向仓库里的 `assets/`（要画真笔刷）。
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

fn call(workspace: &mut Workspace, doc_id: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "s").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

fn detail_of(response: &Value) -> String {
    response["context"]["detail"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn spec_summary(name: &str) -> &'static str {
    ALL_TOOLS
        .iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("注册表里找不到工具 {name}"))
        .summary
}

// ---------- 项 5：batch 顶层误传子工具参数 ----------

#[test]
fn batch_top_level_layer_id_gets_placement_hint() {
    let root = std::env::temp_dir().join(format!("yanshi_b5_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b5");
    let response = call(
        &mut ws,
        "doc_b5",
        "batch",
        json!({"calls": [], "layer_id": "L"}),
    );
    assert_eq!(response["ok"], json!(false), "{response}");
    let detail = detail_of(&response);
    assert!(
        detail.contains("batch 不接受参数 layer_id"),
        "原有报错格式须保留：{detail}"
    );
    assert!(
        detail.contains("请放在 calls 内各笔 arguments 里"),
        "须追加归属提示：{detail}"
    );
}

#[test]
fn batch_top_level_other_subtool_param_gets_hint_but_typo_does_not() {
    let root = std::env::temp_dir().join(format!("yanshi_b5b_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b5b");
    // 通用化：`size` 不是硬编码的 layer_id，它是 brush_stroke 等子工具的参数，
    // 同样应命中归属提示。
    let sized = call(
        &mut ws,
        "doc_b5b",
        "batch",
        json!({"calls": [], "size": 10}),
    );
    assert_eq!(sized["ok"], json!(false), "{sized}");
    let sized_detail = detail_of(&sized);
    assert!(
        sized_detail.contains("请放在 calls 内各笔 arguments 里"),
        "通用子工具参数也应提示归属：{sized_detail}"
    );
    // 反向：纯拼写错误没有任何工具接受它 ⇒ 不许凭空给归属提示。
    let typo = call(
        &mut ws,
        "doc_b5b",
        "batch",
        json!({"calls": [], "layerr_id": "L"}),
    );
    assert_eq!(typo["ok"], json!(false), "{typo}");
    let typo_detail = detail_of(&typo);
    assert!(
        !typo_detail.contains("请放在"),
        "无归属可指时不许追加提示：{typo_detail}"
    );
}

// ---------- 项 7/8：export 回执的派生口径分栏（出口侧） ----------

#[test]
fn export_receipt_separates_designed_unpackaged_derived() {
    let root = std::env::temp_dir().join(format!("yanshi_b78_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b78");
    assert_eq!(
        call(&mut ws, "doc_b78", "create_layer", json!({"layer_id": "L"}))["ok"],
        json!(true)
    );
    // 一个存储里在的 tile 引用：先 put_blob 入库，再拿它的哈希当 tile 索引。
    let put = call(
        &mut ws,
        "doc_b78",
        "put_blob",
        json!({"data_base64": "aGVsbG8="}),
    );
    assert_eq!(put["ok"], json!(true), "{put}");
    let present_tile = put["blob_hash"].as_str().expect("put_blob 应回 blob_hash");
    // 一个存储里不在的 tile 引用：64 个 0，存储里不可能有它。
    let missing_tile = format!("sha256:{}", "0".repeat(64));
    let atoms_path = root.join("docs").join("doc_b78").join("atoms.jsonl");
    let mut atoms = std::fs::read_to_string(&atoms_path).expect("原子日志应已落盘");
    atoms.push_str(&format!(
        "{{\"payload\":{{\"tiles\":\"{present_tile}\"}}}}\n"
    ));
    atoms.push_str(&format!(
        "{{\"payload\":{{\"tiles\":\"{missing_tile}\"}}}}\n"
    ));
    std::fs::write(&atoms_path, atoms).expect("写回原子日志");
    let exported = call(
        &mut ws,
        "doc_b78",
        "export_project",
        json!({"path": root.join("doc_b78.yanshi").to_string_lossy()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");
    // 按设计未装包的派生 = 2（在库 1 个 + 缺失 1 个）；存储缺失的 = 1；本体缺 0。
    assert_eq!(exported["derived_not_packaged"], json!(2), "{exported}");
    assert_eq!(exported["missing_derived"], json!(1), "{exported}");
    assert_eq!(exported["missing_body"], json!(0), "{exported}");
    // 恒等式（本夹具）：未装包总数 = 存储缺失 + 存储里在的。
    assert_eq!(
        exported["derived_not_packaged"].as_u64().unwrap(),
        exported["missing_derived"].as_u64().unwrap() + 1,
        "{exported}"
    );
    let note = exported["missing_derived_note"]
        .as_str()
        .expect("回执须有 missing_derived_note");
    assert!(
        note.contains("存储里缺失的派生索引"),
        "missing_derived 须加注为存储口径：{note}"
    );
    assert!(
        note.contains("derived_not_packaged"),
        "加注须指路到未装包分栏：{note}"
    );
}

// ---------- 项 10：list_brushes 涂抹标记 ----------

#[test]
fn list_brushes_marks_smudge_brushes() {
    let root = std::env::temp_dir().join(format!("yanshi_b10_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b10");
    let listed = call(
        &mut ws,
        "doc_b10",
        "list_brushes",
        json!({"filter": "blending_knife"}),
    );
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let brushes = listed["brushes"].as_array().cloned().unwrap_or_default();
    let knife = brushes
        .iter()
        .find(|brush| brush["name"].as_str() == Some("blending_knife.myb"))
        .expect("资产里应有 blending_knife.myb");
    assert_eq!(knife["smudge"], json!(1.0), "{knife}");
    assert_eq!(knife["smudge_note"], json!("涂抹类：须先有颜料"), "{knife}");
    // 反向：非涂抹笔（smudge = 0）不许带涂抹标记。
    let listed_opaque = call(
        &mut ws,
        "doc_b10",
        "list_brushes",
        json!({"filter": "100%_Opaque"}),
    );
    let opaque_brushes = listed_opaque["brushes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let opaque = opaque_brushes
        .iter()
        .find(|brush| brush["name"].as_str() == Some("100%_Opaque.myb"))
        .expect("资产里应有 100%_Opaque.myb");
    assert_eq!(opaque["smudge"], json!(0.0), "{opaque}");
    assert!(
        opaque.get("smudge_note").is_none(),
        "非涂抹笔不许带涂抹标记：{opaque}"
    );
}

// ---------- 项 11：gradient_blend 缺省 steps ----------

#[test]
fn gradient_blend_defaults_to_64_steps_and_clamp_holds() {
    let root = std::env::temp_dir().join(format!("yanshi_b11_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b11");
    assert_eq!(
        call(&mut ws, "doc_b11", "create_layer", json!({"layer_id": "L"}))["ok"],
        json!(true)
    );
    let blended = call(
        &mut ws,
        "doc_b11",
        "gradient_blend",
        json!({
            "layer_id": "L",
            "from": {"x": 40, "y": 100, "color": "#204080"},
            "to": {"x": 260, "y": 100, "color": "#f0e0c0"},
            "brush": "2B_pencil", "size": 24,
        }),
    );
    assert_eq!(blended["ok"], json!(true), "{blended}");
    assert_eq!(blended["strokes"], json!(64), "{blended}");
    // 子笔回执不带 object_id（既有行为，`object_ids` 为空是旧状，不是本期改动），
    // 故不断它；改断首末色：64 步须走完整个渐变。
    assert_eq!(blended["first_color"], json!("#204080"), "{blended}");
    assert_eq!(blended["last_color"], json!("#f0e0c0"), "{blended}");
    // 下限 clamp 不变：steps=1 仍钳到 2。
    assert_eq!(
        call(
            &mut ws,
            "doc_b11",
            "create_layer",
            json!({"layer_id": "L2"})
        )["ok"],
        json!(true)
    );
    let clamped = call(
        &mut ws,
        "doc_b11",
        "gradient_blend",
        json!({
            "layer_id": "L2",
            "from": {"x": 40, "y": 100, "color": "#204080"},
            "to": {"x": 260, "y": 100, "color": "#f0e0c0"},
            "brush": "2B_pencil", "size": 24, "steps": 1,
        }),
    );
    assert_eq!(clamped["ok"], json!(true), "{clamped}");
    assert_eq!(clamped["strokes"], json!(2), "{clamped}");
    // 注册表摘要写明 steps 可调。
    let summary = spec_summary("gradient_blend");
    assert!(summary.contains("64"), "摘要须写缺省 64：{summary}");
    assert!(summary.contains("2..200"), "摘要须写可调范围：{summary}");
    let steps_desc = ALL_TOOLS
        .iter()
        .find(|spec| spec.name == "gradient_blend")
        .and_then(|spec| spec.params.iter().find(|param| param.name == "steps"))
        .expect("gradient_blend 应有 steps 参数")
        .description;
    assert!(
        steps_desc.contains("64"),
        "steps 参数说明须写缺省 64：{steps_desc}"
    );
}

// ---------- 项 13：list_effects 的 impasto 别名条目 ----------

#[test]
fn list_effects_lists_impasto_alias_without_touching_filter_names() {
    let root = std::env::temp_dir().join(format!("yanshi_b13_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let mut ws = workspace(&root, "doc_b13");
    let listed = call(&mut ws, "doc_b13", "list_effects", json!({}));
    assert_eq!(listed["ok"], json!(true), "{listed}");
    let aliases = listed["filter_aliases"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let impasto = aliases
        .iter()
        .find(|entry| entry["name"].as_str() == Some("impasto"))
        .expect("list_effects 应有 impasto 条目");
    assert_eq!(impasto["alias_of"], json!("relief"), "{impasto}");
    assert_eq!(impasto["note"], json!("同 relief"), "{impasto}");
    // 内核白名单不动：impasto 不进 supported.filters，relief 仍在。
    let filters = listed["supported"]["filters"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        filters.iter().any(|name| name.as_str() == Some("relief")),
        "relief 须仍在白名单里：{listed}"
    );
    assert!(
        !filters.iter().any(|name| name.as_str() == Some("impasto")),
        "impasto 只是别名条目，不许混进内核白名单：{listed}"
    );
    // 注册表摘要指路到别名条目。
    assert!(
        spec_summary("list_effects").contains("impasto"),
        "list_effects 摘要须提及 impasto 别名"
    );
}
