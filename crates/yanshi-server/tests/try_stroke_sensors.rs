//! **`try_stroke.sensors` 必须真的能传进去**（第 719 轮；外部审计 S-02 第 ② 组）
//!
//! **⇒ 缺陷**：handler 一直在读 `sensors` ✓（转成压感／倾斜／速度 ✓），
//!   而 `tools_registry.rs` **没声明**它 ✗ ⇒ `validate_args` 只放行声明过的键 ✓
//!   ⇒ 传 `sensors` 会得到 `invalid_argument` ⇒ **这条能力从上线起不可达** ✗
//!   （声明与实现双向不对称：这正是 AGENTS.md 问句 14「这个声明了的参数，哪一条判据
//!    证明它被读过」的**反面** ✓）。
//!
//! **⇒ 判据**：带 `sensors` 调 `try_stroke` ⇒ **必须 `ok: true`** ✓；
//!   且**缺省值不变**（不带 `sensors` 也 `ok: true` ✓）。
//! **变异**：把注册表里那行 `param!("sensors", …)` 删掉 ⇒ 第一条红（`invalid_argument`）✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_try", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn try_stroke_accepts_the_sensors_its_handler_reads() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_try", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    let mut ctx = context(&mut workspace);
    // **不传 `brush`**（测试工作区里没有笔刷资源 ✓）⇒ 它必然失败 ✓
    //   **⇒ 但失败原因必须是"缺 brush"，不能是"不接受参数 sensors"** ✓ ——
    //   后者正是 S-02 的缺陷形态（注册表没声明 ⇒ `validate_args` 拦掉 ⇒ 能力不可达 ✓）。
    let response = registry.call(
        &mut ctx,
        "try_stroke",
        &json!({
            "layer_id": "layer_default",
            "points": [[6.0, 8.0], [22.0, 8.0]],
            "size": 4.0,
            "want_image": false,
            "sensors": {"pressure": 0.5, "tilt_x": 0.1, "tilt_y": -0.2, "velocity": 120.0},
        }),
    );
    let text = response.to_string();
    assert!(
        !text.contains("不接受参数 sensors"),
        "注册表没声明 sensors ⇒ 这个参数被校验拦掉了（handler 却一直在读）：{response}"
    );
    // **正面确认**：参数被受理之后，失败原因是"缺 brush"✓
    assert!(
        text.contains("brush"),
        "失败原因应当是关于 brush 的（说明 sensors 已经通过校验）：{response}"
    );
}
