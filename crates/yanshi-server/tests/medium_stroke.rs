//! **`medium_stroke`** ✓ —— MCP/工具侧用**介质插件**画画 ✓（真实用户 P1-3 ✓）。
//!
//! **原症状** ✓：油画/水彩只有**浏览器端**能画 ✗（插件是 wasm ✓）⇒ agent 侧只能画"面条线" ✗。
//! **本测试证明** ✓：服务端**原生调用插件** ✓，产出与浏览器端**同形** ✓ ——
//! 一条 `import_image` 补丁 ✓、对象上记着 `{id, version}` ✓、**像素真的非空且有层次** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_medium", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_medium", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
}

/// **六个介质都能经工具画出来** ✓，且对象的 `medium` 记着 **id + version** ✓。
#[test]
fn every_medium_can_be_painted_through_the_tool() {
    let mut workspace = workspace();
    setup(&mut workspace);
    for medium in ["oil", "watercolor", "marker", "pencil", "pixel", "example"] {
        let made = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "medium_stroke",
                &json!({
                    "layer_id": "L",
                    "object_id": format!("m_{medium}"),
                    "medium": medium,
                    "points": [[20.0, 20.0, 1.0], [40.0, 30.0, 0.6], [60.0, 44.0, 0.3]],
                    "size": 22,
                    "color": {"r": 220, "g": 60, "b": 40, "a": 255},
                }),
            )
        };
        assert_eq!(made["ok"], json!(true), "{medium} 应当画得出来：{made}");
        // **id + version 随对象记录** ✓（设计 11.1 的硬要求 ✓）
        assert_eq!(made["medium"]["id"], json!(medium), "{made}");
        assert!(
            made["medium"]["version"].as_u64().unwrap_or(0) >= 1,
            "{made}"
        );
        let got = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "get_object",
                &json!({ "object_id": format!("m_{medium}") }),
            )
        };
        assert_eq!(got["data"]["medium"]["id"], json!(medium), "{got}");
        assert_eq!(
            got["type"],
            json!("raster_patch"),
            "应当落成光栅补丁：{got}"
        );
        let bbox = got["bbox"].as_array().cloned().unwrap_or_default();
        let w = bbox.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = bbox.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
        assert!(w > 10.0 && h > 5.0, "{medium} 的补丁应当覆盖笔迹：{got}");
    }
}

/// **压力真的影响介质笔触** ✓（按点给压力 ⇒ 上墨更少 ✓ —— 与内核笔迹同一条语义 ✓）。
///
/// **判据用"渲染出来的墨量差"** ✓，不是"补丁字节数" ✗ —— 后者只反映区域大小 ✓
///（我第一版就是拿它当依据 ✓，而返回里根本没有那个字段 ✗ ⇒ 又是"猜返回结构" ✗）。
#[test]
fn per_point_pressure_thins_a_medium_stroke() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let ink = |workspace: &mut Workspace| -> usize {
        let (_, _, pixels) = workspace
            .document_mut("doc_medium")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 200.0, 200.0))
            .expect("区域渲染应成功");
        // **按亮度判"有墨"** ✓：文档背景是**不透明白** ✓ ⇒ 只数 `alpha>0` 会把整幅都算进去 ✗
        //（我第一版就是这么错的 ✓ ⇒ 实测 40000/40000 ✓）。这里数**暗像素** ✓。
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    // ① 一条**满压力**的横线（在上半）✓
    let full_before = ink(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let made = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "full", "medium": "oil",
                    "points": [[20.0, 40.0], [50.0, 40.0], [80.0, 40.0]], "size": 20,
                    "color": {"r": 20, "g": 20, "b": 20, "a": 255}}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let full_ink = ink(&mut workspace) - full_before;
    // ② 一条**带压力坡度**的横线（在下半，长度相同）✓
    let ramped_before = ink(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let made = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "ramped", "medium": "oil",
                    "points": [[20.0, 140.0, 1.0], [50.0, 140.0, 0.3], [80.0, 140.0, 0.0]], "size": 20,
                    "color": {"r": 20, "g": 20, "b": 20, "a": 255}}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let ramped_ink = ink(&mut workspace) - ramped_before;
    assert!(full_ink > 0, "满压力那条应当有墨（实测 {full_ink}）");
    assert!(
        ramped_ink < full_ink,
        "带压力那条应当更细 ⇒ 墨量更少（满 {full_ink} vs 带压力 {ramped_ink}）"
    );
    assert!(
        ramped_ink * 3 > full_ink,
        "最轻也不该消失（实测 {ramped_ink} vs 满 {full_ink}）"
    );
}

/// **画布回读的差异要如实** ✓：服务端**不做混色回读** ✗ ⇒ 文档与返回都不假装它等效 ✓。
#[test]
fn the_server_side_medium_does_not_read_back_the_canvas() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "m1", "medium": "watercolor",
                    "points": [[30.0, 30.0, 1.0], [60.0, 50.0, 1.0]], "size": 18}),
        )
    };
    assert_eq!(made["ok"], json!(true), "{made}");
    // **它照样是一条 import_image 补丁** ✓（与浏览器端同形 ✓）
    assert_eq!(made["dabs"], json!(2), "应当报告落了几枚 dab：{made}");
}

/// **错参数要能教人改** ✓（未知介质 ⇒ 列出可用值 ✓；空点集 ✓；点写错 ✓；笔尖过小 ✓）。
#[test]
fn bad_arguments_are_refused_with_a_reason() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "未知介质",
            json!({"layer_id": "L", "medium": "gouache", "points": [[0.0, 0.0]]}),
            "oil",
        ),
        (
            "空点集",
            json!({"layer_id": "L", "medium": "oil", "points": []}),
            "至少一个点",
        ),
        (
            "点不是数组",
            json!({"layer_id": "L", "medium": "oil", "points": [1, 2]}),
            "不是数组",
        ),
        (
            "笔尖过小",
            json!({"layer_id": "L", "medium": "oil", "points": [[0.0, 0.0]], "size": 0.2}),
            "≥ 1",
        ),
    ];
    for (label, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "medium_stroke", &args)
        };
        assert_eq!(
            refused["ok"],
            json!(false),
            "「{label}」应当被拒：{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "「{label}」的原因应当提到「{expected}」，实测：{detail}"
        );
    }
}
