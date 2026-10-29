//! 反馈报告 3 期 E（b）：`get_document` 回执里的提示字段。
//!
//! 只加信息，不改行为：本次读的仍是会话当前文档（`ctx.doc_id`），
//! 旧字段（`doc_id` / 尺寸 / 对象数等）原样返回；新增的只是
//! `session_doc_id`（显式点名会话当前文档）与 `documents` /
//! `documents_count`（与 `list_documents` 同口径的全部文档清单）。
//!
//! 注意：本文件**未在本段跑过**（盒子无空闲构建位，见交付包说明首行
//! “测试待补”）；断言写给总协调在盒子空闲后补跑，不谎称已绿。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc_id: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn create(workspace: &mut Workspace, doc_id: &str, width: u32, height: u32) {
    workspace
        .create_document(
            NewDocument::new(doc_id, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
}

/// 两份文档并存时：`get_document` 的清单必须与 `list_documents` 逐项一致，
/// 且 `session_doc_id` 必须就是会话当前文档（也是旧的 `doc_id` 字段）。
#[test]
fn get_document_reports_session_doc_and_full_document_list() {
    let mut workspace = workspace();
    create(&mut workspace, "hint_first", 160, 120);
    create(&mut workspace, "hint_second", 64, 64);
    let registry = registry();

    let listed = {
        let mut ctx = context(&mut workspace, "hint_first");
        registry.call(&mut ctx, "list_documents", &json!({}))
    };
    let summary = {
        let mut ctx = context(&mut workspace, "hint_first");
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };

    // 旧字段原样：读的仍是会话当前文档，尺寸与对象数不变。
    assert_eq!(summary["doc_id"], json!("hint_first"), "{summary}");
    assert_eq!(summary["width"], json!(160), "{summary}");
    assert_eq!(summary["height"], json!(120), "{summary}");
    assert_eq!(summary["objects"], json!(0), "{summary}");

    // 新增字段 1：会话文档 id 显式点名，且与旧 doc_id 恒等。
    assert_eq!(summary["session_doc_id"], json!("hint_first"), "{summary}");
    assert_eq!(
        summary["session_doc_id"], summary["doc_id"],
        "session_doc_id 必须恒等于 doc_id：{summary}"
    );

    // 新增字段 2：清单与 list_documents 逐项一致（同一口径，不是第二套数）。
    assert_eq!(
        summary["documents"], listed["documents"],
        "{summary} / {listed}"
    );
    assert_eq!(
        summary["documents_count"], listed["count"],
        "{summary} / {listed}"
    );
    assert_eq!(summary["documents_count"], json!(2), "{summary}");
    let ids: Vec<&str> = summary["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["doc_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["hint_first", "hint_second"], "{summary}");
    // 清单条目含 list_documents 口径的宽高与计数，不只是 id。
    let second = &summary["documents"][1];
    assert_eq!(second["width"], json!(64), "{summary}");
    assert_eq!(second["height"], json!(64), "{summary}");
    assert_eq!(second["layers"], json!(1), "{summary}");
    assert_eq!(second["objects"], json!(0), "{summary}");
}

/// 只有一份文档时清单也必须在场（空集合上的断言恒真陷阱：count=1 才算在验）。
#[test]
fn get_document_hint_present_for_single_document() {
    let mut workspace = workspace();
    create(&mut workspace, "hint_only", 80, 60);
    let registry = registry();
    let summary = {
        let mut ctx = context(&mut workspace, "hint_only");
        registry.call(&mut ctx, "get_document", &json!({"preview_size": false}))
    };
    assert_eq!(summary["session_doc_id"], json!("hint_only"), "{summary}");
    assert_eq!(summary["documents_count"], json!(1), "{summary}");
    assert_eq!(
        summary["documents"][0]["doc_id"],
        json!("hint_only"),
        "{summary}"
    );
}
