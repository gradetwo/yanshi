//! 笔刷「湿笔」接线：**载墨量 / 湿度耗墨 / 混色** ✓（设计 11.1 明确 MVP 要支持纹理、湿度和载墨量 ✓）。
//!
//! 与曲线/动力学/纹理**同一条盖章路径** ✓（`stamp_stroke_configured` ✓），
//! 因此同样守住"无 `appearance` 时逐字节一致"的回归底线 ✓。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_paint", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_paint", Bbox::new(0.0, 0.0, 128.0, 64.0))
        .expect("渲染应成功")
        .2
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
    let index = (y * 128 + x) * 4;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

fn fresh() -> Workspace {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_paint", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    workspace
}

fn stroke(workspace: &mut Workspace, object_id: &str, appearance: Option<serde_json::Value>) {
    let registry = registry();
    let mut ctx = context(workspace);
    let mut data = json!({
        "points": [[8.0, 32.0], [120.0, 32.0]],
        "size": 12.0,
        "hardness": 1.0,
        "color": {"r": 0, "g": 0, "b": 0, "a": 255}
    });
    if let Some(appearance) = appearance {
        data["appearance"] = appearance;
    }
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({"layer_id": "L", "object_id": object_id, "data": data}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
}

/// 载墨量 + 湿度：墨沿笔迹**逐渐耗尽** ⇒ 笔迹前段比后段更"实" ✓。
#[test]
fn a_wet_stroke_fades_along_its_length() {
    let mut plain = fresh();
    stroke(&mut plain, "s_plain", None);
    let plain = render(&mut plain);

    let mut wet = fresh();
    stroke(
        &mut wet,
        "s_wet",
        Some(json!({"paint_load": 1.0, "wetness": 1.0})),
    );
    let wet = render(&mut wet);

    // 普通笔触：前后段应基本一致（这里比"后段不**比**前段更浅"）。
    let start_plain = pixel(&plain, 24, 32);
    let end_plain = pixel(&plain, 104, 32);
    let wet_start = pixel(&wet, 24, 32);
    let wet_end = pixel(&wet, 104, 32);
    eprintln!(
        "调试：普通 前{:?} 后{:?}｜湿笔 前{:?} 后{:?}",
        start_plain, end_plain, wet_start, wet_end
    );

    assert!(
        wet_start[3] > 0,
        "湿笔起笔处应有墨（前置条件），实际 {:?}",
        wet_start
    );
    // 墨量衰减的可观测结果：后段比前段更淡（alpha 或颜色更接近白底）。
    assert!(
        wet_end[3] < wet_start[3] || wet_end[0] > wet_start[0],
        "湿笔应沿笔迹变淡（前 {:?} → 后 {:?}）",
        wet_start,
        wet_end
    );
    // 而普通笔触不应有这样的衰减。
    assert!(
        end_plain[3] >= start_plain[3] && end_plain[0] <= start_plain[0] + 2,
        "普通笔触不应沿笔迹变淡（前 {:?} → 后 {:?}）",
        start_plain,
        end_plain
    );
}

/// 混色：把**目标处已有颜色**按 `mixing × 湿度` 混进笔尖 ✓ ——
/// 在黑底上用红色笔、`mixing = 1`，落墨应明显被底色拉红 ✓。
#[test]
fn mixing_pulls_the_destination_colour_into_the_brush() {
    let setup = |workspace: &mut Workspace| {
        let registry = registry();
        let mut ctx = context(workspace);
        let filled = registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L", "object_id": "a_bg",
                    "data": {"color": {"r": 0, "g": 0, "b": 0, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 128, "h": 64}}}),
        );
        assert_eq!(filled["ok"], json!(true), "{filled}");
    };

    let mut plain = fresh();
    setup(&mut plain);
    {
        let registry = registry();
        let mut ctx = context(&mut plain);
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "z_stroke",
                    "data": {"points": [[8.0, 32.0], [120.0, 32.0]], "size": 12.0, "hardness": 1.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let plain = render(&mut plain);

    let mut mixed = fresh();
    setup(&mut mixed);
    {
        let registry = registry();
        let mut ctx = context(&mut mixed);
        let drawn = registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({"layer_id": "L", "object_id": "z_stroke",
                    "data": {"points": [[8.0, 32.0], [120.0, 32.0]], "size": 12.0, "hardness": 1.0,
                             "color": {"r": 255, "g": 0, "b": 0, "a": 255},
                             "appearance": {"mixing": 1.0, "wetness": 1.0}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let mixed = render(&mut mixed);

    let plain_px = pixel(&plain, 40, 32);
    let mixed_px = pixel(&mixed, 40, 32);
    eprintln!("调试：不混色 {:?}｜混色 {:?}", plain_px, mixed_px);
    // 黑底 + 红笔：不混色时红通道高、蓝绿低；混色后被黑底拉暗 ⇒ 红通道更低 ✓。
    assert!(
        mixed_px[0] < plain_px[0],
        "混色应把目标（黑底）混进笔尖 ⇒ 红色变暗（不混色 {:?} → 混色 {:?}）",
        plain_px,
        mixed_px
    );
}

/// 湿笔参数同样确定：同参数 ⇒ 逐字节一致 ✓。
#[test]
fn wet_settings_are_deterministic() {
    let appearance = || json!({"paint_load": 1.0, "wetness": 0.8, "mixing": 0.4, "seed": 5});
    let mut first = fresh();
    stroke(&mut first, "s1", Some(appearance()));
    let a = render(&mut first);
    let mut second = fresh();
    stroke(&mut second, "s1", Some(appearance()));
    let b = render(&mut second);
    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    assert_eq!(differing, 0, "湿笔参数必须确定（差异字节 {differing}）");
}

/// **回归底线**：`paint_load = 0`（无限墨）且无其它参数 ⇒ 与不传 appearance 一致 ✓。
#[test]
fn unlimited_paint_is_treated_as_no_appearance() {
    let mut plain = fresh();
    stroke(&mut plain, "s1", None);
    let without = render(&mut plain);

    let mut unlimited = fresh();
    stroke(&mut unlimited, "s1", Some(json!({"paint_load": 0.0})));
    let with = render(&mut unlimited);

    let differing = without
        .iter()
        .zip(with.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(differing, 0, "无限墨应视为无外观（差异字节 {differing}）");
}
