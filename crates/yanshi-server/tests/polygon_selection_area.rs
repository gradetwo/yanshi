//! **多边形选区的 `area` 不许再报 0**（`bug_report.md` BUG-06 那一族）。
//!
//! **为什么**：`list_selections` 的 `area` 对 `polygon`/`lasso` 原来一律 `(0.0, false)` ✗
//! ⇒ 而 `area: 0.0` **长得像"这个选区是空的"** ✓ ⇒ 调用方据此以为"没选中任何东西" ✗。
//!
//! **判据断结构量**：三角形 `(0,0)-(40,0)-(20,40)` ⇒ 鞋带面积 = `0.5 × 40 × 40 = 800` ✓，
//! 且 `area_exact: true` ✓；顶点不足 3 个 ⇒ 仍报 `(0.0, false)`（**说"算不出来"**，不是假装 0 ✓）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_poly_area", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn area_of(ctx: &mut ToolContext<'_>, selection_id: &str) -> (f64, bool) {
    let listed = call(ctx, "list_selections", &json!({}));
    let entry = listed
        .get("selections")
        .and_then(Value::as_array)
        .and_then(|selections| {
            selections
                .iter()
                .find(|s| s.get("selection_id").and_then(Value::as_str) == Some(selection_id))
        })
        .cloned()
        .unwrap_or_else(|| panic!("没列出选区 {selection_id}：{listed}"));
    (
        entry.get("area").and_then(Value::as_f64).unwrap_or(-1.0),
        entry
            .get("area_exact")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    )
}

#[test]
fn a_polygon_selection_reports_its_shoelace_area() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_poly_area", "width": 64, "height": 64}),
        );
        let made = call(
            &mut ctx,
            "create_selection",
            &json!({
                "selection_id": "tri",
                "shape": {"kind": "polygon", "points": [[0, 0], [40, 0], [20, 40]]}
            }),
        );
        assert_eq!(
            made.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：多边形选区没建出来：{made}"
        );
    }
    let mut ctx = context(&mut workspace);
    let (area, exact) = area_of(&mut ctx, "tri");
    assert_eq!(area, 800.0, "三角形面积必须是 0.5×40×40 = 800（不是 0）");
    assert!(exact, "鞋带公式算得出来 ⇒ `area_exact` 必须是 true");
}
