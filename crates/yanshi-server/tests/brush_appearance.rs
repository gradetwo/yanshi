//! 笔刷「物理参数」接线：曲线 / 动力学 / 纹理 ✓（设计 808 行 `advanced.appearance`）。
//!
//! 硬性要求：**没有 `appearance` 时渲染与接线前逐字节一致** ✓ ——
//! 这是本次接线的回归底线（本会话多次因"新路径与旧路径语义不同"踩坑 ✓）。

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
    ToolContext::new(workspace, "doc_brush", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_brush", Bbox::new(0.0, 0.0, 128.0, 64.0))
        .expect("渲染应成功")
        .2
}

fn ink(workspace: &mut Workspace) -> usize {
    render(workspace)
        .chunks_exact(4)
        .filter(|pixel| pixel[3] > 8 && (pixel[0] < 240 || pixel[1] < 240 || pixel[2] < 240))
        .count()
}

/// 画一笔（可带 `appearance`），返回存活像素数。
fn stroke_with(
    workspace: &mut Workspace,
    object_id: &str,
    appearance: Option<serde_json::Value>,
) -> usize {
    let registry = registry();
    {
        let mut ctx = context(workspace);
        let mut data = json!({
            "points": [[16.0, 32.0], [112.0, 32.0]],
            "size": 16.0,
            "hardness": 1.0,
            "color": {"r": 20, "g": 20, "b": 20, "a": 255}
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
    ink(workspace)
}

fn fresh() -> Workspace {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_brush", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    workspace
}

#[test]
fn a_plain_stroke_still_works() {
    let mut workspace = fresh();
    let plain = stroke_with(&mut workspace, "s_plain", None);
    assert!(plain > 0, "普通笔触应画出像素（前置条件），实际 {plain}");
}

/// **回归底线**：显式传入"空 appearance"与完全不传，渲染必须逐字节一致 ✓。
#[test]
fn an_empty_appearance_is_byte_identical_to_no_appearance() {
    let mut plain_workspace = fresh();
    stroke_with(&mut plain_workspace, "s1", None);
    let plain = render(&mut plain_workspace);

    let mut empty_workspace = fresh();
    stroke_with(
        &mut empty_workspace,
        "s1",
        Some(json!({"size_curve": [[0.0, 1.0], [1.0, 1.0]], "seed": 0})),
    );
    let empty = render(&mut empty_workspace);

    // **不要**在断言里直接比较整个像素数组 ✗ —— 失败时会把 buffer 全打印出来（输出爆掉 ✓）。
    // 改为比较差异字节数与校验和 ✓，失败信息也因此可读 ✓。
    let differing = plain
        .iter()
        .zip(empty.iter())
        .filter(|(a, b)| a != b)
        .count();
    let sum = |data: &[u8]| -> u64 { data.iter().map(|byte| u64::from(*byte)).sum() };
    assert_eq!(
        differing, 0,
        "空 appearance（常量曲线、无动力学）必须与不传 appearance 逐字节一致（差异字节 {differing}，校验和 {} vs {}）",
        sum(&plain),
        sum(&empty)
    );
}

/// 大小曲线：整条曲线的值都取 0.4 ⇒ 笔迹明显更细 ✓（像素数应显著下降）。
#[test]
fn a_size_curve_thins_the_stroke() {
    let mut wide = fresh();
    let wide_ink = stroke_with(&mut wide, "s_wide", None);

    let mut thin = fresh();
    let thin_ink = stroke_with(
        &mut thin,
        "s_thin",
        Some(json!({"size_curve": [[0.0, 0.4], [1.0, 0.4]]})),
    );
    assert!(
        thin_ink < wide_ink,
        "大小曲线取 0.4 后笔迹应更细（{thin_ink} 应小于 {wide_ink}）"
    );
    assert!(thin_ink > 0, "变细后仍应有墨");
}

/// 动力学确定性：同种子 ⇒ 逐字节一致；不同种子 ⇒ 结果不同 ✓。
#[test]
fn dynamics_are_deterministic_per_seed() {
    let appearance = |seed: u64| json!({"seed": seed, "dynamics": {"jitter": 4.0, "scatter": 0.4}});

    let mut first = fresh();
    stroke_with(&mut first, "s1", Some(appearance(7)));
    let a = render(&mut first);

    let mut second = fresh();
    stroke_with(&mut second, "s1", Some(appearance(7)));
    let b = render(&mut second);
    let differing = a.iter().zip(b.iter()).filter(|(x, y)| x != y).count();
    assert_eq!(
        differing, 0,
        "同 seed 的动力学必须逐字节一致（差异字节 {differing}）"
    );

    let mut third = fresh();
    stroke_with(&mut third, "s1", Some(appearance(99)));
    let c = render(&mut third);
    let differing = a.iter().zip(c.iter()).filter(|(x, y)| x != y).count();
    assert!(differing > 0, "不同 seed 的动力学应产生不同结果");
}

/// 程序化纹理：加噪点纹理后 alpha 被调制 ⇒ 结果与无纹理不同 ✓，且仍确定 ✓。
#[test]
fn a_texture_modulates_the_stroke() {
    let mut plain = fresh();
    stroke_with(&mut plain, "s1", None);
    let without = render(&mut plain);

    let mut textured = fresh();
    stroke_with(
        &mut textured,
        "s1",
        Some(json!({"texture": {"kind": "grain", "scale": 4.0, "strength": 0.8, "seed": 3}})),
    );
    let with = render(&mut textured);
    let differing = without
        .iter()
        .zip(with.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert!(differing > 0, "纹理应改变笔触的落墨（alpha 被调制）");

    let mut again = fresh();
    stroke_with(
        &mut again,
        "s1",
        Some(json!({"texture": {"kind": "grain", "scale": 4.0, "strength": 0.8, "seed": 3}})),
    );
    let repeat = render(&mut again);
    let differing = with
        .iter()
        .zip(repeat.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(differing, 0, "同参数纹理必须确定 ✓（差异字节 {differing}）");
}
