//! **P-12：draw_* 回执的像素取证字段** ✓
//!
//! **判据** ✓：
//! ① 黑线（非静默）⇒ 五项字段都是数值，`painted > 0`、`max_channel_delta == 255`、
//!    `clipped == painted`（全在画布内）、`footprint == clipped`、`landed == true`、
//!    `delta_regime == "canvas_base"`、`changed_pixels > 0`。
//! ② 白上画白 ⇒ `painted > 0`（覆盖口径，不是“变了几个像素”）、`max_channel_delta == 0`、
//!    `delta_median == 0`、`changed_pixels == 0`、`landed == false`、
//!    `landed_reason == "invisible_on_background"`。
//! ③ 形状与填充 ⇒ 同样带数值取证。
//! ④ 静默 batch 里的 draw_stroke 子调用 ⇒ 五项字段在位但为 null，`evidence_note` 点名
//!    “静默”，且 batch 的 `stroke_evidence` 不编数字（仍是 `{index, tool, ok: true}`）。
//! ⑤ `wait_for_render=false` ⇒ 五项字段为 null，`evidence_note` 点名 `wait_for_render=false`。
//! ⑥ 擦除 ⇒ 同样带数值取证，`max_channel_delta > 0`（黑线被擦回白底）。
//!
//! **变异** ✓：把 `attach_draw_evidence` 的字段贴附删掉 ⇒ ①②③⑥ 红（字段变 null）；
//! 把 `draw_evidence_capture` 的 silent 分支改成照常渲染 ⇒ ④ 红（字段变数值）；
//! 把 `draw_evidence_compare` 的合成改成直接比 tile RGB ⇒ ② 红（白上画白报 255）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_draw_ev", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// **非静默 ctx**（`wait_for_render=true` ⇒ 本次调用实际渲染 ⇒ 取证应给实数）。
fn context(workspace: &mut Workspace) -> ToolContext<'_> {
    ToolContext::new(workspace, "doc_draw_ev", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = context(workspace);
    registry().call(&mut ctx, tool, &args)
}

/// 五项取证字段必须都是数值（`null` 在这里就是红，不是“没值也行”）。
fn five_fields(value: &serde_json::Value) -> (u64, u64, u64, u64, String) {
    let painted = value["painted_pixels"]
        .as_u64()
        .unwrap_or_else(|| panic!("painted_pixels 必须是数值：{value}"));
    let clipped = value["painted_pixels_clipped"]
        .as_u64()
        .unwrap_or_else(|| panic!("painted_pixels_clipped 必须是数值：{value}"));
    let footprint = value["ink_footprint_pixels"]
        .as_u64()
        .unwrap_or_else(|| panic!("ink_footprint_pixels 必须是数值：{value}"));
    let delta = value["max_channel_delta"]
        .as_u64()
        .unwrap_or_else(|| panic!("max_channel_delta 必须是数值：{value}"));
    let regime = value["delta_regime"]
        .as_str()
        .unwrap_or_else(|| panic!("delta_regime 必须是字符串：{value}"))
        .to_owned();
    (painted, clipped, footprint, delta, regime)
}

/// **① 黑线 ⇒ 五项数值、`max == 255`、`landed == true`** ✓。
#[test]
fn black_line_carries_numeric_evidence() {
    let mut workspace = workspace();
    let drawn = call(
        &mut workspace,
        "draw_stroke",
        json!({
            "layer_id": "layer_default",
            "object_id": "line1",
            "data": {"points": [[20.0, 20.0], [180.0, 180.0]], "size": 5.0, "hardness": 1.0,
                     "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let (painted, clipped, footprint, delta, regime) = five_fields(&drawn);
    assert!(painted > 0, "黑线应有覆盖：{drawn}");
    assert_eq!(clipped, painted, "全在画布内 ⇒ clipped == painted：{drawn}");
    assert_eq!(footprint, clipped, "对 draw 足迹恒等于画布内覆盖：{drawn}");
    assert_eq!(delta, 255, "黑线落在白底上 ⇒ max 应为 255：{drawn}");
    assert_eq!(regime, "canvas_base", "比对用的就是落笔前的真画布：{drawn}");
    assert_eq!(drawn["landed"], json!(true), "{drawn}");
    assert_eq!(drawn["landed_reason"], json!("painted"), "{drawn}");
    let changed = drawn["changed_pixels"]
        .as_u64()
        .expect("changed_pixels 必须是数值");
    assert!(changed > 0, "黑线应改了像素：{drawn}");
    assert!(changed <= clipped, "改了的像素不应超过覆盖：{drawn}");
    let median = drawn["delta_median"]
        .as_u64()
        .expect("delta_median 必须是数值");
    assert!(
        median > 0 && median <= delta,
        "中位值应在 (0, max] 内：{drawn}"
    );
}

/// **② 白上画白 ⇒ `painted > 0` 而 `max == 0`** ✓（覆盖口径，不是“变了几个像素”）。
#[test]
fn white_on_white_reports_coverage_but_zero_delta() {
    let mut workspace = workspace();
    let drawn = call(
        &mut workspace,
        "draw_stroke",
        json!({
            "layer_id": "layer_default",
            "object_id": "white1",
            "data": {"points": [[20.0, 20.0], [180.0, 180.0]], "size": 5.0, "hardness": 1.0,
                     "color": [1.0, 1.0, 1.0, 1.0]},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let (painted, _clipped, _footprint, delta, _regime) = five_fields(&drawn);
    assert!(painted > 0, "白线也有覆盖 ⇒ painted 应大于 0：{drawn}");
    assert_eq!(delta, 0, "白上画白 ⇒ 画面一个像素都没变：{drawn}");
    assert_eq!(drawn["delta_median"], json!(0), "{drawn}");
    assert_eq!(drawn["changed_pixels"], json!(0), "{drawn}");
    assert_eq!(drawn["landed"], json!(false), "{drawn}");
    assert_eq!(
        drawn["landed_reason"],
        json!("invisible_on_background"),
        "{drawn}"
    );
}

/// **③ 形状与填充 ⇒ 同样带数值取证** ✓。
#[test]
fn shape_and_fill_carry_numeric_evidence() {
    let mut workspace = workspace();
    let shaped = call(
        &mut workspace,
        "draw_shape",
        json!({
            "layer_id": "layer_default",
            "object_id": "rect1",
            "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 40, "h": 30}},
                     "color": {"r": 0, "g": 0, "b": 0, "a": 255}},
        }),
    );
    assert_eq!(shaped["ok"], json!(true), "{shaped}");
    let (painted, clipped, _footprint, delta, _regime) = five_fields(&shaped);
    assert!(painted > 0, "黑矩形应有覆盖：{shaped}");
    assert_eq!(
        clipped, painted,
        "全在画布内 ⇒ clipped == painted：{shaped}"
    );
    assert_eq!(delta, 255, "黑矩形落在白底上 ⇒ max 应为 255：{shaped}");

    let filled = call(
        &mut workspace,
        "fill",
        json!({
            "layer_id": "layer_default",
            "object_id": "fill1",
            "data": {"color": {"r": 0, "g": 0, "b": 0, "a": 255},
                     "region": {"x": 100, "y": 100, "w": 40, "h": 30}},
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
    let (painted, clipped, _footprint, delta, _regime) = five_fields(&filled);
    assert_eq!(painted, 40 * 30, "填充覆盖应等于矩形面积：{filled}");
    assert_eq!(clipped, painted, "{filled}");
    assert_eq!(delta, 255, "{filled}");
}

/// **④ 静默 batch 里的 draw_stroke ⇒ 字段在位但为 null + note 点名“静默”** ✓。
#[test]
fn silent_batch_draw_stroke_reports_nulls_with_a_note() {
    let mut workspace = workspace();
    let made = call(
        &mut workspace,
        "batch",
        json!({
            "calls": [{"tool": "draw_stroke", "arguments": {
                "layer_id": "layer_default", "object_id": "batch1",
                "data": {"points": [[20.0, 20.0], [180.0, 180.0]], "size": 5.0,
                         "color": [0.0, 0.0, 0.0, 1.0]},
            }}],
            "silent": true,
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let result = &made["calls"][0]["result"];
    assert_eq!(result["ok"], json!(true), "{result}");
    // 五项字段在位（键存在）但为 null（不是缺失 ⇒ 调用方不用猜“支不支持”）。
    for key in [
        "painted_pixels",
        "painted_pixels_clipped",
        "ink_footprint_pixels",
        "max_channel_delta",
        "delta_regime",
    ] {
        assert!(
            result.get(key).is_some(),
            "字段 {key} 应在位（为 null 也比缺失强）：{result}"
        );
        assert!(
            result[key].is_null(),
            "静默调用 ⇒ {key} 应为 null：{result}"
        );
    }
    let note = result["evidence_note"]
        .as_str()
        .unwrap_or_else(|| panic!("静默调用应带 evidence_note：{result}"));
    assert!(note.contains("静默"), "note 应点名静默：{note}");
    // batch 的 stroke_evidence 不编数字（draw 子调用仍是“非笔触摘要”）。
    assert_eq!(
        made["stroke_evidence"][0],
        json!({"index": 0, "tool": "draw_stroke", "ok": true}),
        "静默取证为 null ⇒ 摘要不应编数字：{made}"
    );
}

/// **⑤ `wait_for_render=false` ⇒ 字段为 null + note 点名** ✓。
#[test]
fn wait_for_render_false_reports_nulls_with_a_note() {
    let mut workspace = workspace();
    let mut ctx = ToolContext::new(&mut workspace, "doc_draw_ev", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(false, 4_000);
    let drawn = registry().call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "layer_default",
            "object_id": "nowait1",
            "data": {"points": [[20.0, 20.0], [180.0, 180.0]], "size": 5.0,
                     "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    assert!(drawn["painted_pixels"].is_null(), "{drawn}");
    assert!(drawn["max_channel_delta"].is_null(), "{drawn}");
    let note = drawn["evidence_note"]
        .as_str()
        .unwrap_or_else(|| panic!("应带 evidence_note：{drawn}"));
    assert!(
        note.contains("wait_for_render=false"),
        "note 应点名 wait_for_render=false：{note}"
    );
}

/// **⑥ 擦除 ⇒ 同样带数值取证** ✓（黑线被擦回白底 ⇒ max > 0）。
///
/// erase 口径：`painted == changed_pixels`（实际擦除的像素，不是笔触几何足迹——
/// erase 只扣图层 alpha，透明底 tile 上看不见）；`ink_mass` 恒 0（不产墨）。
#[test]
fn erase_carries_numeric_evidence() {
    let mut workspace = workspace();
    let drawn = call(
        &mut workspace,
        "draw_stroke",
        json!({
            "layer_id": "layer_default",
            "object_id": "erase_base",
            "data": {"points": [[20.0, 100.0], [180.0, 100.0]], "size": 10.0, "hardness": 1.0,
                     "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let erased = call(
        &mut workspace,
        "erase",
        json!({
            "layer_id": "layer_default",
            "object_id": "erase1",
            "data": {"points": [[20.0, 100.0], [180.0, 100.0]], "size": 10.0, "hardness": 1.0},
        }),
    );
    assert_eq!(erased["ok"], json!(true), "{erased}");
    let (painted, _clipped, _footprint, delta, _regime) = five_fields(&erased);
    assert!(painted > 0, "擦除应有覆盖：{erased}");
    assert!(delta > 0, "黑线被擦回白底 ⇒ max 应大于 0：{erased}");
    // erase 口径恒等式：painted 就是实际擦除的像素（== changed）。
    assert_eq!(
        painted,
        erased["changed_pixels"].as_u64().unwrap(),
        "erase 的 painted 应等于 changed_pixels：{erased}"
    );
    assert_eq!(erased["ink_mass"], json!(0), "erase 不产墨：{erased}");
}
