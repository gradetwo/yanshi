//! **D1：`batch` 每笔取证摘要 `stroke_evidence`**（期 2b）✓
//!
//! 摘要与 `calls` 同下标：`stroke_evidence.len() == count`；每笔笔触给
//! `landed` / `painted_pixels_clipped` / `ink_mass` / `max_channel_delta`
//!（字段名与 `brush_stroke` 回执一致 ✓；`medium_stroke` 的 `paint_delta`
//! 在此归一为 `max_channel_delta` ✓）；失败的笔带完整 `error` ✓。
//!
//! **判据** ✓：① 10 笔 ⇒ 每笔摘要四字段都是数值、与子结果回执的同名字段一致、
//! `ok_count + fail_count == count`、顶层无图（`preview` null、子结果无 `image`）；
//! ② 1 笔故意失败（不存在的笔刷）⇒ 顶层 `ok:false`、`fail_count == 1`、
//! 失败笔摘要带完整 `error`；③ 1 笔 `medium_stroke` ⇒ 摘要字段由 `paint_delta` /
//! `painted_pixels` 归一而来，仍为数值；④ 非笔触子调用（`draw_shape`）⇒
//! 摘要只有 `{index, tool, ok: true}`，不编取证数字。
//!
//! **变异**（判据脚本头有对应条目）：删掉一次 `push` ⇒ ① 的长度恒等式红；
//! 把 `paint_delta` 的回落删掉 ⇒ ③ 红；把 `error` 换成空对象 ⇒ ② 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

/// **落盘工作区 + 仓库里的 `assets/`** ✓ —— `brush_stroke` 要从那儿读 `.myb` 笔刷 ✓。
fn workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_d1_{name}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_d1", 320, 240),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_d1", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn stroke_call(i: u32) -> serde_json::Value {
    // 每笔错开 y，避免同一位置叠加（摘要数值互不干扰，断言读的是每笔自己的回执 ✓）。
    let y = 30.0 + (i as f64) * 18.0;
    json!({"tool": "brush_stroke", "arguments": {
        "layer_id": "L", "brush": "100%_Opaque",
        "points": [[40.0, y, 1.0], [280.0, y, 1.0]], "size": 30,
        "color": [0.45, 0.38, 0.30],
    }})
}

/// 读一笔摘要的四个取证字段；任一字段不是数值就 panic（`null` 必须红 ✓，
/// 不是「没值也行」✗—— 字段缺失时实现给 `null`，断言在这里把它当红 ✓）。
fn four_fields(entry: &serde_json::Value) -> (bool, u64, u64, u64) {
    let landed = entry["landed"]
        .as_bool()
        .unwrap_or_else(|| panic!("landed 必须是布尔：{entry}"));
    let clipped = entry["painted_pixels_clipped"]
        .as_u64()
        .unwrap_or_else(|| panic!("painted_pixels_clipped 必须是数值：{entry}"));
    let ink = entry["ink_mass"]
        .as_u64()
        .unwrap_or_else(|| panic!("ink_mass 必须是数值：{entry}"));
    let delta = entry["max_channel_delta"]
        .as_u64()
        .unwrap_or_else(|| panic!("max_channel_delta 必须是数值：{entry}"));
    (landed, clipped, ink, delta)
}

/// **10 笔 ⇒ 每笔有取证摘要、四字段与回执一致、顶层无图** ✓。
#[test]
fn ten_strokes_each_get_an_evidence_summary() {
    let mut workspace = workspace("ten");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let calls: Vec<_> = (0..10).map(stroke_call).collect();
    // **本分支 silent 缺省仍是 false** ✓（期 2 的缺省改动不在本包）⇒ 这里显式传 `silent: true`。
    let made = call(
        &mut workspace,
        "batch",
        json!({ "calls": calls, "silent": true }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    // **恒等式** ✓（D1 之前就有的那条）：`ok_count + fail_count == count`。
    assert_eq!(made["count"], json!(10), "{made}");
    assert_eq!(made["ok_count"], json!(10), "{made}");
    assert_eq!(made["fail_count"], json!(0), "{made}");
    assert_eq!(
        made["ok_count"].as_u64().unwrap() + made["fail_count"].as_u64().unwrap(),
        made["count"].as_u64().unwrap(),
        "ok_count + fail_count == count"
    );
    // **摘要长度恒等式** ✓（D1 的主恒等式）：与 `calls` 同下标 ⇒ 长度 == count。
    let evidence = made["stroke_evidence"]
        .as_array()
        .expect("stroke_evidence 必须是数组");
    assert_eq!(evidence.len(), 10, "摘要缺笔 ⇒ 红：{made}");
    // **无图** ✓：顶层 `preview` 为 null，且每个子结果都没有 `image` / `preview`。
    assert!(made["preview"].is_null(), "静默批处理顶层不应带图：{made}");
    for (i, entry) in made["calls"].as_array().unwrap().iter().enumerate() {
        let result = &entry["result"];
        assert!(result.get("image").is_none(), "第 {i} 笔子结果不应带图");
        assert!(
            result.get("preview").is_none() || result["preview"].is_null(),
            "第 {i} 笔子结果不应带预览"
        );
        // **摘要与回执一致** ✓：摘要的四个字段与子结果回执的同名字段逐笔相等。
        let (landed, clipped, ink, delta) = four_fields(&evidence[i]);
        assert_eq!(
            landed,
            result["landed"].as_bool().unwrap(),
            "第 {i} 笔 landed 应与回执一致"
        );
        assert_eq!(
            clipped,
            result["painted_pixels_clipped"].as_u64().unwrap(),
            "第 {i} 笔 clipped 应与回执一致"
        );
        assert_eq!(
            ink,
            result["ink_mass"].as_u64().unwrap(),
            "第 {i} 笔 ink_mass 应与回执一致"
        );
        assert_eq!(
            delta,
            result["max_channel_delta"].as_u64().unwrap(),
            "第 {i} 笔 delta 应与回执一致"
        );
        // **恒等式** ✓（A1/A2 的那条）：`0 ≤ ink_mass ≤ ink_footprint_pixels × 255`。
        let footprint = result["ink_footprint_pixels"].as_u64().unwrap();
        assert!(ink <= footprint * 255, "第 {i} 笔墨量不应超过足迹×255");
        assert_eq!(
            evidence[i]["index"],
            json!(i),
            "第 {i} 笔摘要 index 应与 calls 同下标"
        );
        assert_eq!(evidence[i]["tool"], json!("brush_stroke"));
        assert_eq!(evidence[i]["ok"], json!(true));
    }
}

/// **1 笔故意失败 ⇒ 顶层 `ok:false`、失败笔摘要带完整 `error`** ✓。
#[test]
fn one_failing_stroke_reports_ok_false_and_carries_its_error() {
    let mut workspace = workspace("fail");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let mut calls: Vec<_> = (0..10).map(stroke_call).collect();
    // 第 4 笔用不存在的笔刷 ⇒ 故意失败（输入能触发 ✓，不是空集合上的恒真断言 ✗）。
    calls[4]["arguments"]["brush"] = json!("不存在的笔刷.myb");
    let made = call(
        &mut workspace,
        "batch",
        json!({ "calls": calls, "silent": true }),
    );
    assert_eq!(
        made["ok"],
        json!(false),
        "有子调用失败 ⇒ 顶层 ok 应为 false（A6）：{made}"
    );
    assert_eq!(made["ok_count"], json!(9), "{made}");
    assert_eq!(made["fail_count"], json!(1), "{made}");
    assert_eq!(
        made["ok_count"].as_u64().unwrap() + made["fail_count"].as_u64().unwrap(),
        10,
        "ok_count + fail_count == count"
    );
    let evidence = made["stroke_evidence"]
        .as_array()
        .expect("stroke_evidence 必须是数组");
    assert_eq!(evidence.len(), 10, "失败笔的摘要也不能缺：{made}");
    // 失败的那笔带完整 `error`（与 `failed_calls` 里的同一份渲染错误 ✓）。
    let failed = &evidence[4];
    assert_eq!(failed["ok"], json!(false), "{failed}");
    assert_eq!(
        failed["error"]["error_code"],
        json!("reference_not_found"),
        "不存在的笔刷应报 reference_not_found：{failed}"
    );
    assert!(
        failed["error"]["context"]["detail"].is_string(),
        "错误应带可读 detail：{failed}"
    );
    assert_eq!(
        failed["error"], made["failed_calls"][0]["error"],
        "摘要里的 error 应与 failed_calls 里的同一份渲染错误一致"
    );
    // 其余 9 笔仍有数值摘要 ✓。
    for (i, entry) in evidence.iter().enumerate() {
        if i == 4 {
            continue;
        }
        let _ = four_fields(entry);
    }
}

/// **`medium_stroke` 的摘要由同义字段归一而来** ✓（不改 medium 的回执）。
#[test]
fn a_medium_stroke_summary_maps_its_synonyms() {
    let mut workspace = workspace("medium");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "batch",
        json!({
            "calls": [{"tool": "medium_stroke", "arguments": {
                "layer_id": "L", "medium": "oil",
                "points": [[40.0, 120.0, 1.0], [280.0, 120.0, 1.0]], "size": 30,
                "color": [0.45, 0.38, 0.30],
            }}],
            "silent": true,
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let result = &made["calls"][0]["result"];
    // medium 回执本来的同义字段（不在摘要里另起名字 ✓）：`paint_delta` + `painted_pixels`。
    assert!(
        result["paint_delta"].is_number(),
        "medium 回执应带 paint_delta：{result}"
    );
    assert!(
        result["painted_pixels"].is_number(),
        "medium 回执应带 painted_pixels：{result}"
    );
    let evidence = &made["stroke_evidence"][0];
    let (landed, clipped, ink, delta) = four_fields(evidence);
    assert_eq!(landed, result["landed"].as_bool().unwrap());
    assert_eq!(
        clipped,
        result["painted_pixels"].as_u64().unwrap(),
        "medium 的 painted_pixels 即 clipped（无选区裁剪）"
    );
    assert_eq!(ink, result["ink_mass"].as_u64().unwrap());
    assert_eq!(
        delta,
        result["paint_delta"].as_u64().unwrap(),
        "medium 的 paint_delta 归一为 max_channel_delta"
    );
}

/// **非笔触子调用不编取证数字** ✓（不是笔 ⇒ 只有 `{index, tool, ok: true}`）。
#[test]
fn a_non_stroke_call_gets_no_fabricated_evidence() {
    let mut workspace = workspace("shape");
    assert_eq!(
        call(&mut workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let made = call(
        &mut workspace,
        "batch",
        json!({
            "calls": [
                stroke_call(0),
                {"tool": "draw_shape", "arguments": {"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 40, "h": 30}},
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}},
            ],
            "silent": true,
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
    let evidence = made["stroke_evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), 2);
    let _ = four_fields(&evidence[0]);
    assert_eq!(
        evidence[1],
        json!({"index": 1, "tool": "draw_shape", "ok": true}),
        "非笔触摘要不应编数字：{evidence:?}"
    );
}
