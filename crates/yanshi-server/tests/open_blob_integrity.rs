//! **打开文档时报告缺失的 blob**（外部 `~/patch/bug_report.md` BUG-19 残留）。
//!
//! **他们报的**：「缺件检测已遍布各处，只差**打开时的统一入口**」⇒ blob 被**带外删除**时
//! （手删目录／换机／备份不全）现在只有"画到它那一刻"才发现（可能几天以后）。
//!
//! **判据断结构量**：
//! ① 引用一个**真实存在**的 blob ⇒ `blob_integrity.missing == 0`、`missing_sample` 空 ✓；
//! ② 把它从存储里**删掉**再打开 ⇒ `missing == 1` 且 `missing_sample` 里有那个哈希 ✓；
//! ③ **打开本身仍然成功**（`opened: true`）✓ —— 只报不修，不许因为缺件就打不开 ✓。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets))
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_integrity", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn mask_bytes() -> Vec<u8> {
    vec![255u8; 16 * 16 * 4]
}

#[test]
fn opening_a_document_reports_blobs_that_went_missing() {
    let mut workspace = workspace();
    let hash = workspace
        .store()
        .put(&mask_bytes())
        .expect("蒙版应能入库")
        .to_string();
    let parsed: yanshi_core::BlobHash = hash.parse().expect("哈希应可解析");
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_integrity", "width": 64, "height": 64}),
        );
        // 让文档**引用**这个 blob（蒙版是最轻的引用方式）。
        let made = call(
            &mut ctx,
            "create_mask",
            &json!({
                "mask_id": "mk",
                "shape": {"kind": "bitmap", "blob_hash": hash, "width": 16, "height": 16}
            }),
        );
        assert_eq!(
            made.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：蒙版没建出来：{made}"
        );

        // ① 完好时：missing = 0
        let opened = call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_integrity", "width": 64, "height": 64}),
        );
        let integrity = opened
            .get("blob_integrity")
            .cloned()
            .unwrap_or_else(|| panic!("打开时必须报 blob_integrity：{opened}"));
        assert_eq!(
            integrity.get("missing").and_then(Value::as_u64),
            Some(0),
            "完好时 missing 必须是 0 ⇒ {integrity}"
        );
        assert!(
            integrity
                .get("missing_sample")
                .and_then(Value::as_array)
                .map(|list| list.is_empty())
                .unwrap_or(false),
            "完好时 missing_sample 必须是空数组（**字段要在**，不许缺省）⇒ {integrity}"
        );
    }

    // ② **带外删除**（模拟手删目录／换机／备份不全）⇒ 再打开必须报出来。
    workspace.store().remove(&parsed).expect("删 blob");
    let mut ctx = context(&mut workspace);
    let reopened = call(
        &mut ctx,
        "new_document",
        &json!({"doc_id": "doc_integrity", "width": 64, "height": 64}),
    );
    assert_eq!(
        reopened.get("opened").and_then(Value::as_bool),
        Some(true),
        "缺件不该让「打开」失败（只报不修）⇒ {reopened}"
    );
    let integrity = reopened
        .get("blob_integrity")
        .cloned()
        .unwrap_or_else(|| panic!("打开时必须报 blob_integrity：{reopened}"));
    assert_eq!(
        integrity.get("missing").and_then(Value::as_u64),
        Some(1),
        "删掉被引用的 blob 之后 missing 必须是 1 ⇒ {integrity}"
    );
    let sample = integrity
        .get("missing_sample")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        sample
            .iter()
            .filter_map(Value::as_str)
            .any(|text| text == hash),
        "missing_sample 必须**点名**缺的那个哈希 ⇒ {sample:?}"
    );
}
