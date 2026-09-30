//! 「另存为副本」：新 id 的文档必须与源文档**渲染逐字节一致**，且原文档保持可用。
//!
//! 设计**没有规定**文档命名/重命名 ✗ —— 文档以 `doc_id` 为主键 ✓，日志、令牌与持久化路径
//! 都以它为准 ✓。用户确认采用**路线 A：另存为副本**（新 id、原文档保留、可逆 ✓），
//! 而不是真改名（迁移 id 会牵动日志/持久化/令牌，不可逆 ✗）。
//!
//! 实现是**逐原子原样重放** ✓，因此副本的折叠状态与渲染结果应与源文档完全一致 ✓。

use serde_json::json;
use yanshi_core::{AtomKind, Bbox};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc_id: &'static str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc_id, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn a_duplicate_renders_identically_and_leaves_the_source_alone() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_src", 128, 128),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace, "doc_src");
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L",
                    "data": {"color": {"r": 30, "g": 140, "b": 220, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 128}}}),
        );
        registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "r1",
                    "data": {"geometry": {"kind": "ellipse", "bbox": {"x": 20, "y": 20, "w": 40, "h": 30}},
                             "color": {"r": 250, "g": 40, "b": 40, "a": 255}}}),
        );
    }
    let source_atoms = {
        let document = workspace.document_mut("doc_src").unwrap();
        document
            .log()
            .atoms()
            .iter()
            .filter(|atom| atom.kind != AtomKind::CreateDocument)
            .count()
    };

    let copied = workspace
        .duplicate_document("doc_src", "doc_copy", "human:1", "session:test")
        .expect("另存为副本应成功");
    assert_eq!(
        copied, source_atoms,
        "副本应重放源文档的全部原子（跳过源文档自建事件）：{copied} vs {source_atoms}"
    );

    // 两者渲染必须**逐字节一致**。
    let source = workspace
        .render_region_raw("doc_src", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("源文档渲染应成功");
    let copy = workspace
        .render_region_raw("doc_copy", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("副本渲染应成功");
    assert_eq!(source.2.len(), copy.2.len(), "两者渲染尺寸应一致");
    let differing = source
        .2
        .iter()
        .zip(copy.2.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(
        differing, 0,
        "副本必须与源文档逐字节一致（差异字节 {differing}）"
    );

    // 源文档仍然存在且可继续修改（另存为**不移动**任何东西 ✓）。
    {
        let mut ctx = context(&mut workspace, "doc_src");
        let drawn = registry.call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "r2",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 90, "y": 90, "w": 20, "h": 20}},
                             "color": {"r": 10, "g": 250, "b": 10, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "源文档应仍可修改：{drawn}");
    }
    // 副本不随之变化（两者是独立文档 ✓）。
    let copy_after = workspace
        .render_region_raw("doc_copy", Bbox::new(0.0, 0.0, 128.0, 128.0))
        .expect("副本渲染应成功");
    assert_eq!(
        copy_after.2, copy.2,
        "改动源文档不应影响副本（两个独立文档）"
    );
}
