//! 包 B（2026-10-21）服务端集成测试：vibrance 两层对齐的全链往返、
//! `get_histogram` 三种作用域、`apply_adjustments` 批量配方的原子校验。
//!
//! 断言口径：精确数值与恒等/非恒等关系，不许只断「不报错」。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn make_registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace, doc: &str) -> ToolContext<'a> {
    ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 建 doc（黑底）+ 图层 L + 整幅低饱和填充 [160,150,140]，返回该填充色渲染后的中心像素。
fn doc_with_low_saturation_fill(workspace: &mut Workspace, doc: &str) {
    workspace
        .create_document(
            NewDocument::new(doc, 16, 16)
                .with_background(json!({"r": 0, "g": 0, "b": 0, "a": 255})),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = make_registry();
    let mut ctx = context(workspace, doc);
    let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
    let filled = registry.call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": "L",
            "shape": {"type": "rect", "x": 0, "y": 0, "w": 16, "h": 16},
            "color": [160, 150, 140],
            "opacity": 1.0
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
}

fn center_pixel(workspace: &mut Workspace, doc: &str) -> Vec<i64> {
    let registry = make_registry();
    let mut ctx = context(workspace, doc);
    let sample = registry.call(&mut ctx, "sample_color", &json!({"x": 8, "y": 8}));
    assert_eq!(sample["ok"], json!(true), "{sample}");
    sample["rgba"]
        .as_array()
        .expect("sample_color 应回 rgba")
        .iter()
        .map(|value| value.as_i64().expect("rgba 分量应为整数"))
        .collect()
}

fn add_adjustment(
    workspace: &mut Workspace,
    doc: &str,
    name: &str,
    params: serde_json::Value,
) -> serde_json::Value {
    let registry = make_registry();
    let mut ctx = context(workspace, doc);
    registry.call(
        &mut ctx,
        "add_adjustment",
        &json!({"layer_id": "L", "adjustment_type": name, "params": params}),
    )
}

/// **vibrance 全链往返**：经工具建 → 渲染必须是真 Vibrance（与 saturation 恒等对照不同）→
/// `list_effects` 读回的名字再提交，渲染逐像素不变（不被偷换）→ 旧别名也可达且同结果、
/// 别名的参数域与规范名一致（amount=9 必须被拒）。
#[test]
fn vibrance_end_to_end_is_reachable_and_roundtrips() {
    // 对照组：saturation(amount=1) 是恒等 ⇒ 中心像素就是填充色本身。
    let mut ws_sat = workspace();
    doc_with_low_saturation_fill(&mut ws_sat, "doc_sat");
    let base = center_pixel(&mut ws_sat, "doc_sat");
    assert_eq!(base, vec![160, 150, 140, 255], "填充色渲染应逐字节还原");
    let added = add_adjustment(&mut ws_sat, "doc_sat", "saturation", json!({"amount": 1.0}));
    assert_eq!(added["ok"], json!(true), "{added}");
    assert_eq!(
        center_pixel(&mut ws_sat, "doc_sat"),
        base,
        "saturation(amount=1) 应恒等（对照基线）"
    );

    // 实验组：vibrance(amount=1) 对低饱和色非恒等（蓝通道明显下降）。
    let mut ws_vib = workspace();
    doc_with_low_saturation_fill(&mut ws_vib, "doc_vib");
    let added = add_adjustment(&mut ws_vib, "doc_vib", "vibrance", json!({"amount": 1.0}));
    assert_eq!(added["ok"], json!(true), "vibrance 应可经工具到达：{added}");
    let vibrance_pixel = center_pixel(&mut ws_vib, "doc_vib");
    assert_ne!(
        vibrance_pixel, base,
        "vibrance(amount=1) 不应与恒等相同（相同 ⇒ 又被偷换成 saturation）"
    );
    assert!(
        vibrance_pixel[2] <= base[2] - 5,
        "低饱和色的蓝通道应被 vibrance 明显拉开：{vibrance_pixel:?} vs 基线 {base:?}"
    );

    // 读回：list_effects 给出的名字就是 vibrance；拿它在新文档里再提交 ⇒ 渲染逐像素相同。
    let registry = make_registry();
    let readback_name = {
        let mut ctx = context(&mut ws_vib, "doc_vib");
        let effects = registry.call(&mut ctx, "list_effects", &json!({"layer_id": "L"}));
        assert_eq!(effects["count"], json!(1), "{effects}");
        effects["effects"][0]["adjustment_type"]
            .as_str()
            .expect("读回应带 adjustment_type")
            .to_owned()
    };
    assert_eq!(readback_name, "vibrance", "读回的名字必须是 vibrance");
    let mut ws_re = workspace();
    doc_with_low_saturation_fill(&mut ws_re, "doc_re");
    let readded = add_adjustment(&mut ws_re, "doc_re", &readback_name, json!({"amount": 1.0}));
    assert_eq!(readded["ok"], json!(true), "{readded}");
    assert_eq!(
        center_pixel(&mut ws_re, "doc_re"),
        vibrance_pixel,
        "读回再提交的渲染必须逐像素一致（往返不被偷换）"
    );

    // 旧别名 vibrance_natural：可达、同结果；且参数域与规范名一致（amount=9 被拒）。
    let mut ws_alias = workspace();
    doc_with_low_saturation_fill(&mut ws_alias, "doc_alias");
    let alias_added = add_adjustment(
        &mut ws_alias,
        "doc_alias",
        "vibrance_natural",
        json!({"amount": 1.0}),
    );
    assert_eq!(alias_added["ok"], json!(true), "别名应可达：{alias_added}");
    assert_eq!(
        center_pixel(&mut ws_alias, "doc_alias"),
        vibrance_pixel,
        "别名与规范名必须同结果"
    );
    let alias_bad = add_adjustment(
        &mut ws_alias,
        "doc_alias",
        "vibrance_natural",
        json!({"amount": 9.0}),
    );
    assert_eq!(
        alias_bad["error_code"],
        json!("invalid_argument"),
        "别名必须按 vibrance 的参数域校验（amount 0..2）：{alias_bad}"
    );
}

/// `get_histogram`：半黑半白文档的文档/区域/图层三种作用域，断精确桶值与摘要。
#[test]
fn get_histogram_scopes_report_exact_buckets() {
    let mut ws = workspace();
    ws.create_document(
        NewDocument::new("doc_hist", 8, 4)
            .with_background(json!({"r": 0, "g": 0, "b": 0, "a": 255})),
        "human:1",
        "session:test",
    )
    .unwrap();
    let registry = make_registry();
    {
        let mut ctx = context(&mut ws, "doc_hist");
        let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        assert_eq!(created["ok"], json!(true), "{created}");
        let filled = registry.call(
            &mut ctx,
            "fill_region",
            &json!({
                "layer_id": "L",
                "shape": {"type": "rect", "x": 4, "y": 0, "w": 4, "h": 4},
                "color": [255, 255, 255],
                "opacity": 1.0
            }),
        );
        assert_eq!(filled["ok"], json!(true), "{filled}");
    }
    let call = |ws: &mut Workspace, args: serde_json::Value| {
        let registry = make_registry();
        let mut ctx = context(ws, "doc_hist");
        registry.call(&mut ctx, "get_histogram", &args)
    };

    // 文档作用域（缺省）：32 像素，16 黑 16 白。
    let doc = call(&mut ws, json!({}));
    assert_eq!(doc["ok"], json!(true), "{doc}");
    assert_eq!(doc["scope"], json!("document"));
    assert_eq!(doc["pixels"], json!(32));
    assert_eq!(doc["channels"]["r"][0], json!(16), "黑半恰 16 个：{doc}");
    assert_eq!(doc["channels"]["r"][255], json!(16), "白半恰 16 个：{doc}");
    assert_eq!(doc["channels"]["a"][255], json!(32));
    assert_eq!(doc["luma"][0], json!(16));
    assert_eq!(doc["luma"][255], json!(16));
    assert_eq!(doc["summary"]["r"]["mean"], json!(127.5));
    assert_eq!(doc["summary"]["r"]["min"], json!(0));
    assert_eq!(doc["summary"]["r"]["max"], json!(255));
    assert_eq!(doc["summary"]["r"]["p95"], json!(255));

    // 区域作用域：左半纯黑、右半纯白。
    let left = call(&mut ws, json!({"region": {"x": 0, "y": 0, "w": 4, "h": 4}}));
    assert_eq!(left["scope"], json!("region"));
    assert_eq!(left["pixels"], json!(16));
    assert_eq!(left["channels"]["r"][0], json!(16));
    assert_eq!(left["summary"]["r"]["mean"], json!(0.0));
    assert_eq!(left["summary"]["r"]["median"], json!(0));
    let right = call(&mut ws, json!({"region": {"x": 4, "y": 0, "w": 4, "h": 4}}));
    assert_eq!(right["channels"]["r"][255], json!(16));
    assert_eq!(right["summary"]["r"]["mean"], json!(255.0));
    assert_eq!(right["summary"]["r"]["median"], json!(255));

    // 图层作用域：走既有单层渲染出口（`render_region_raw_layer`）——它仍合成在文档
    // 背景上（本例黑底）⇒ 左半是黑（r=0、a=255）、右半是白；与文档作用域的区别在
    // 「别的图层不参与」。断它实测的精确口径，不按想象中的透明口径写。
    let layer = call(&mut ws, json!({"layer_id": "L"}));
    assert_eq!(layer["scope"], json!("layer"));
    assert_eq!(layer["pixels"], json!(32));
    assert_eq!(
        layer["channels"]["r"][0],
        json!(16),
        "左半在单层里应为黑：{layer}"
    );
    assert_eq!(layer["channels"]["r"][255], json!(16));
    assert_eq!(layer["channels"]["a"][255], json!(32));

    // 不存在的图层要报错，不许静默回全文档。
    let missing = call(&mut ws, json!({"layer_id": "nope"}));
    assert_eq!(missing["ok"], json!(false), "{missing}");
    assert_eq!(missing["error_code"], json!("reference_not_found"));
}

/// `apply_adjustments`：合法配方按序落成；含非法项时整组不落成、报错指明第几项。
#[test]
fn apply_adjustments_is_ordered_and_atomic_on_invalid() {
    let mut ws = workspace();
    ws.create_document(
        NewDocument::new("doc_adj", 16, 16),
        "human:1",
        "session:test",
    )
    .unwrap();
    let registry = make_registry();
    {
        let mut ctx = context(&mut ws, "doc_adj");
        let created = registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        assert_eq!(created["ok"], json!(true), "{created}");
    }
    let effect_types = |ws: &mut Workspace| -> Vec<String> {
        let registry = make_registry();
        let mut ctx = context(ws, "doc_adj");
        let effects = registry.call(&mut ctx, "list_effects", &json!({"layer_id": "L"}));
        effects["effects"]
            .as_array()
            .expect("list_effects 应回数组")
            .iter()
            .map(|effect| {
                effect["adjustment_type"]
                    .as_str()
                    .expect("应带 adjustment_type")
                    .to_owned()
            })
            .collect()
    };

    // 合法 3 项：落成 3 个、顺序与配方一致。
    let applied = {
        let mut ctx = context(&mut ws, "doc_adj");
        registry.call(
            &mut ctx,
            "apply_adjustments",
            &json!({
                "layer_id": "L",
                "adjustments": [
                    {"adjustment_type": "brightness_contrast", "params": {"brightness": 0.1}},
                    {"adjustment_type": "saturation", "params": {"amount": 1.5}},
                    {"adjustment_type": "invert", "params": {}}
                ]
            }),
        )
    };
    assert_eq!(applied["ok"], json!(true), "{applied}");
    assert_eq!(applied["created"], json!(3));
    assert_eq!(
        applied["object_ids"].as_array().map(Vec::len),
        Some(3),
        "{applied}"
    );
    assert_eq!(
        effect_types(&mut ws),
        vec![
            "brightness_contrast".to_owned(),
            "saturation".to_owned(),
            "invert".to_owned()
        ],
        "落成顺序必须与配方顺序一致"
    );

    // 第 2 项类型非法：第 1 项再合法也不许落成，总数不变，报错指明「第 2 项」。
    let rejected = {
        let mut ctx = context(&mut ws, "doc_adj");
        registry.call(
            &mut ctx,
            "apply_adjustments",
            &json!({
                "layer_id": "L",
                "adjustments": [
                    {"adjustment_type": "exposure", "params": {"ev": 0.5}},
                    {"adjustment_type": "not_a_kind", "params": {}}
                ]
            }),
        )
    };
    assert_eq!(rejected["ok"], json!(false), "{rejected}");
    assert_eq!(rejected["error_code"], json!("invalid_argument"));
    let detail = rejected["context"]["detail"].as_str().unwrap_or("");
    assert!(detail.contains("第 2 项"), "报错必须指明是第几项：{detail}");
    assert_eq!(effect_types(&mut ws).len(), 3, "非法配方整组不落成");

    // 第 1 项参数越界（saturation amount 上限 8）：同样整组不落成、指明「第 1 项」。
    let rejected_param = {
        let mut ctx = context(&mut ws, "doc_adj");
        registry.call(
            &mut ctx,
            "apply_adjustments",
            &json!({
                "adjustments": [
                    {"adjustment_type": "saturation", "params": {"amount": 99.0}, "layer_id": "L"},
                    {"adjustment_type": "invert", "params": {}, "layer_id": "L"}
                ]
            }),
        )
    };
    assert_eq!(rejected_param["ok"], json!(false), "{rejected_param}");
    let detail = rejected_param["context"]["detail"].as_str().unwrap_or("");
    assert!(detail.contains("第 1 项"), "报错必须指明是第几项：{detail}");
    assert_eq!(effect_types(&mut ws).len(), 3, "参数非法的配方整组不落成");

    // 空配方与不存在的图层都要报错。
    let empty = {
        let mut ctx = context(&mut ws, "doc_adj");
        registry.call(
            &mut ctx,
            "apply_adjustments",
            &json!({"layer_id": "L", "adjustments": []}),
        )
    };
    assert_eq!(empty["ok"], json!(false), "{empty}");
    let bad_layer = {
        let mut ctx = context(&mut ws, "doc_adj");
        registry.call(
            &mut ctx,
            "apply_adjustments",
            &json!({
                "layer_id": "ghost",
                "adjustments": [{"adjustment_type": "invert", "params": {}}]
            }),
        )
    };
    assert_eq!(bad_layer["ok"], json!(false), "{bad_layer}");
    assert_eq!(effect_types(&mut ws).len(), 3);
}
