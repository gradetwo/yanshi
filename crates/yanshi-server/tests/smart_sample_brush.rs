//! **`smart_sample.brush` 必须真的能传进去**（第 720 轮；外部审计 S-02 第 ② 组）
//!
//! **⇒ 缺陷**：handler 一直读 `brush` ✓（缺省 `calligraphy` ✓），而注册表**没声明**它 ✗
//!   ⇒ `validate_args` 只放行声明过的键 ✓ ⇒ 传 `brush` 得到 `invalid_argument`
//!   ⇒ **用户没法换笔刷** ✓（能力不可达 ✓）。
//!
//! **⇒ 判据**：带 `brush` 调 `smart_sample` ⇒ **不许**出现「不接受参数 brush」✓。
//!   **变异**：删掉那行 `param!("brush", …)` ⇒ 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_smart", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn smart_sample_accepts_the_brush_argument_its_handler_reads() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_smart", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);
    // 参数不一定齐（ref／shape 是必填 ✓）—— **但失败原因不许是"不接受 brush"** ✓
    let response = registry.call(
        &mut ctx,
        "smart_sample",
        &json!({
            "layer_id": "layer_default",
            "ref": "layer_default",
            "shape": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": 16.0, "h": 16.0}},
            "brush": "calligraphy",
        }),
    );
    let text = response.to_string();
    assert!(
        !text.contains("不接受参数 brush"),
        "注册表没声明 brush ⇒ 换笔刷这个能力被校验拦掉了（handler 却一直在读）：{response}"
    );
}
