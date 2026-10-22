//! **`preview_size` 超过 256 时必须"说出来"**（外部 `bug_report.md` BUG-12 的一半）。
//!
//! **他们报的**：`get_document` 的缩略图只有 64/128/256 三档 ⇒ 细节目检只能走 `render_region`／`export_png`。
//! **⇒ 能力缺口**（任意分辨率）另算 ✓；**本轮修的是"静默夹"** ✗：
//!   原来传 `1024` 会**悄悄**给 256，调用方毫不知情 ✓。
//!
//! **判据断结构量**：
//! ① 要 1024 ⇒ `preview_size_capped === true`、`preview_size_requested === 1024`、
//!    `preview_size_effective === 256`，且 `warnings` 里有一条**点名三档与替代工具** ✓；
//! ② 要 128 ⇒ **不许**出现 `preview_size_capped`（夹了才说 ✓，没夹就别啰嗦 ✓）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_preview_cap", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

#[test]
fn asking_for_a_huge_preview_says_what_you_actually_got() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_preview_cap", "width": 512, "height": 512}),
        );
    }
    let mut ctx = context(&mut workspace);

    // ① 要 1024 ⇒ 必须**明说**实际给了 256。
    let capped = call(&mut ctx, "get_document", &json!({"preview_size": 1024}));
    assert_eq!(
        capped.get("preview_size_capped").and_then(Value::as_bool),
        Some(true),
        "要 1024 而实际只有 256 ⇒ 必须打上 `preview_size_capped` ⇒ {capped}"
    );
    assert_eq!(
        capped.get("preview_size_requested").and_then(Value::as_f64),
        Some(1024.0),
        "必须回显**请求的**档位 ⇒ {capped}"
    );
    assert_eq!(
        capped.get("preview_size_effective").and_then(Value::as_f64),
        Some(256.0),
        "必须回显**实际的**档位 ⇒ {capped}"
    );
    let warnings = capped
        .get("warnings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let joined = warnings
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join("｜");
    assert!(
        joined.contains("64/128/256") && joined.contains("render_region"),
        "警告必须点名**三档**并给出替代工具 ⇒ 实测 {warnings:?}"
    );

    // ② 要 128 ⇒ 没夹，就**不许**出现 `preview_size_capped`（说了才叫说 ✓）。
    let exact = call(&mut ctx, "get_document", &json!({"preview_size": 128}));
    assert!(
        exact.get("preview_size_capped").is_none(),
        "没夹的时候不许打 `preview_size_capped`（否则这个字段就没有鉴别力）⇒ {exact}"
    );
}
