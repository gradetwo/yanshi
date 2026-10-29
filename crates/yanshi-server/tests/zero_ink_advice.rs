//! **零像素笔触的建议必须落在真实原因上**（外部评测包 `B3`）。
//!
//! **他们报的**：`size=120`、两点、路径总长 **15.2px** 的笔被拒画，而报错首推
//! 「加大 size 到 4 以上」⇒ 对 `size=120` 是**无效建议**（真实原因是路径太短）。
//!
//! **本判据钉三条**（各自对应一次真实的误诊）：
//! ① **大笔刷 ＋ 短路径不再变成 0 像素** —— 当前 HEAD 实测 **9712 像素、steps=9** ✓
//!    （他们那次是整笔拒画 ⇒ 这条会盯住回归）；
//! ② **超短路径现在兜底落墨**（反馈期 1，3.6a：`calligraphy` size=2、路径总长 0.4px ⇒
//!    撑不满一个 dab 间距；旧行为是拒绝，新行为是 ok:true、落墨、回执 warnings 里
//!    带兜底提示，诊断事实（原长／间距／size）保留在提示里）。
//! ③ **真零墨仍拒绝**（与 ② 同几何、只把 color.a 置 0 ⇒ 有效 opacity 为 0；
//!    透明笔不兜底 ⇒ “0 像素不许 ok:true”的不变量移到这里）。
//!
//! **变异**：把真零墨改回 `ok:true` ⇒ ③ 红（该拒绝的成了成功）；把兜底提示删掉 ⇒
//! ② 红（诊断事实丢了）；把透明笔也兜底 ⇒ ③ 红（不该落墨的落了墨）。

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
    ToolContext::new(workspace, "doc_zero_ink", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn detail_of(body: &Value) -> String {
    body.get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn a_large_brush_on_a_short_path_paints_and_a_tiny_size_still_explains_itself() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_zero_ink", "width": 200, "height": 200}),
        );
    }
    let mut ctx = context(&mut workspace);

    // ① **他们那一笔的构型**：size=120、两点、15.2px ⇒ 现在必须**真的落墨**。
    let short = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "brush": "oil-06-paint",
            "size": 120,
            "points": [[40.0, 40.0, 1.0], [55.2, 40.0, 1.0]]
        }),
    );
    assert_eq!(
        short.get("ok").and_then(Value::as_bool),
        Some(true),
        "大笔刷 ＋ 短路径不该整笔拒画（外部 B3 的构型）：{short}"
    );
    let painted = short
        .get("painted_pixels")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(
        painted > 0,
        "这一笔必须落墨（他们当时是 0 像素）⇒ 实测 painted_pixels={painted}"
    );

    // ② **超短路径现在兜底落墨**（反馈期 1，3.6a：size=2 的书法笔、路径总长 0.4px ⇒
    //    撑不满一个 dab 间距；旧行为是拒绝，新行为是沿末段方向延长到 1.5× 间距、
    //    至少落一枚 dab，诊断事实降为回执 warnings 里的一条兜底提示）。
    let tiny = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "brush": "calligraphy",
            "size": 2,
            "points": [[40.0, 40.0, 1.0], [40.4, 40.0, 1.0]]
        }),
    );
    assert_eq!(
        tiny.get("ok").and_then(Value::as_bool),
        Some(true),
        "超短路径不再拒绝（反馈期 1 兜底落墨）：{tiny}"
    );
    assert!(
        tiny.get("painted_pixels")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0,
        "兜底必须真的落墨（调查实测 4 像素）：{tiny}"
    );
    let warnings: Vec<String> = tiny
        .get("warnings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("dab 间距") && w.contains("路径总长")),
        "超短路径回执必须带兜底提示（诊断事实保留在 warnings 里）：{warnings:?}"
    );

    // ③ **真零墨仍拒绝**（与 ② 同几何、只把 color.a 置 0 ⇒ 有效 opacity 为 0；
    //    透明笔不兜底，0 墨仍是正确行为 ⇒ 拒绝；“0 像素不许 ok:true”的不变量移到这里）。
    let zero = call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default",
            "brush": "calligraphy",
            "size": 2,
            "points": [[40.0, 40.0, 1.0], [40.4, 40.0, 1.0]],
            "color": [0.0, 0.0, 0.0, 0.0]
        }),
    );
    assert_eq!(
        zero.get("ok").and_then(Value::as_bool),
        Some(false),
        "0 像素不许回 ok:true（那是静默降级）：{zero}"
    );
    let detail = detail_of(&zero);
    assert!(
        !detail.is_empty(),
        "真零墨拒绝必须带原因（失败没有信号不能接受）：{zero}"
    );
}
