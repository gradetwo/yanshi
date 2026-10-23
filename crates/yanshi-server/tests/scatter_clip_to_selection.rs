//! **`scatter_strokes` 也要能按选区裁剪**（外部 `~/patch/bug_report.md` BUG-04 残留第 3 条）。
//!
//! **他们报的**：「`scatter_strokes` 是全部落笔工具中**唯一没有接入** `clip_to_selection` 的」
//! ⇒ 在剪影边缘附近撒点**只能靠调用侧自行钳制** ✗，或改用 `brush_stroke + clip_to_selection`（丢掉散点语义）。
//!
//! **本判据断结构量**：选区 = 左半 `rect(0,0,32,64)` ⇒ 撒 `count=400` 笔之后
//! ① **选区里**必须有墨（否则"裁剪"可以靠"一笔都不画"作弊 ✗）；
//! ② **选区外**必须**一点墨都没有**（这就是裁剪的全部意义）；
//! ③ 响应要**回显** `clip_to_selection`（不回声 = 调用方无法确认它生效 ✓）。
//!
//! **变异**：把 `clip_to_selection` 不透传（`if let Some` 那段删掉）⇒ ② 红（右半出现墨）。

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
    ToolContext::new(workspace, "doc_scatter_clip", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

/// 取若干个点的 RGBA（`null` 记成全透明）。
fn samples(ctx: &mut ToolContext<'_>, points: &[[i32; 2]]) -> Vec<[u64; 4]> {
    let reply = call(ctx, "sample_colors", &json!({ "points": points }));
    reply
        .get("samples")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let mut out = [255u64, 255, 255, 255];
                    if let Some(values) = item.get("rgba").and_then(Value::as_array) {
                        for (index, slot) in out.iter_mut().enumerate() {
                            *slot = values.get(index).and_then(Value::as_u64).unwrap_or(255);
                        }
                    }
                    out
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 与白底不同即为"有墨"（新文档是**不透明白底** ✓ ⇒ 不能用 alpha 判断 ✓）。
fn has_ink(rgba: [u64; 4]) -> bool {
    rgba != [255, 255, 255, 255]
}

#[test]
fn scatter_strokes_only_lands_inside_the_selection() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_scatter_clip", "width": 64, "height": 64}),
        );
        // 选区 = **左半**（x 0..32）。
        let selection = call(
            &mut ctx,
            "create_selection",
            &json!({
                "selection_id": "left_half",
                "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 32, "h": 64}}
            }),
        );
        assert_eq!(
            selection.get("ok").and_then(Value::as_bool),
            Some(true),
            "前置：选区没建出来：{selection}"
        );
    }
    let mut ctx = context(&mut workspace);

    let scattered = call(
        &mut ctx,
        "scatter_strokes",
        &json!({
            "layer_id": "layer_default",
            "seed": 20261022,
            "area": {"x": 4, "y": 4, "w": 56, "h": 56},
            "palette": ["#101010"],
            "brush": "100%_Opaque",
            "count": 400,
            "size_range": [6, 10],
            "clip_to_selection": "left_half"
        }),
    );
    assert_eq!(
        scattered.get("ok").and_then(Value::as_bool),
        Some(true),
        "撒点本当成功：{scattered}"
    );
    assert!(
        scattered
            .get("strokes")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0,
        "至少要有落笔（否则本判据的 ① 只是「什么都没做」✗）：{scattered}"
    );
    // ③ 回显
    assert_eq!(
        scattered.get("clip_to_selection").and_then(Value::as_str),
        Some("left_half"),
        "响应必须回显生效的选区（不回声 ⇒ 调用方无法确认它生效 ✓）：{scattered}"
    );

    // ① 选区里必须有墨（左半取 8 点）
    let inside = samples(
        &mut ctx,
        &[
            [6, 8],
            [10, 20],
            [16, 14],
            [20, 30],
            [24, 40],
            [28, 12],
            [12, 50],
            [30, 55],
        ],
    );
    let inked_inside = inside.iter().filter(|rgba| has_ink(**rgba)).count();
    assert!(
        inked_inside > 0,
        "选区**里面**必须有墨（否则裁掉的其实是全部 ✗）⇒ 实测 {inked_inside}/8 有墨：{inside:?}"
    );

    // ② 选区外必须**一点墨都没有**（右半取 12 点）
    let outside = samples(
        &mut ctx,
        &[
            [36, 8],
            [40, 20],
            [44, 14],
            [48, 30],
            [52, 40],
            [56, 12],
            [38, 50],
            [42, 55],
            [46, 6],
            [50, 24],
            [54, 48],
            [58, 36],
        ],
    );
    let inked_outside = outside.iter().filter(|rgba| has_ink(**rgba)).count();
    assert_eq!(
        inked_outside, 0,
        "选区**外面**必须一点墨都没有（这就是裁剪的全部意义）⇒ 实测 {inked_outside}/12 有墨：{outside:?}"
    );
}

#[test]
fn scatter_keeps_the_whole_dab_inside_the_polygon_not_just_the_anchor() {
    // **外部 `bug_report.md` BUG-04 残留第 2 条**：锚点在内，但**笔段第二点与 dab 半径**仍可越出多边形 ✓
    // 构型：三角形 `(8,8)-(56,8)-(32,56)` ⇒ **取它外接盒里的"角"**（例如 (12,44)、(52,44)）
    // 这些点**盒内、形外** ⇒ 修好之后那里必须**一点墨都没有** ✓（修之前半径 5px ⇒ 边缘会有墨 ✓）。
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_scatter_clip", "width": 64, "height": 64}),
        );
    }
    let mut ctx = context(&mut workspace);
    let scattered = call(
        &mut ctx,
        "scatter_strokes",
        &json!({
            "layer_id": "layer_default",
            "seed": 20261023,
            "area": {"points": [[8, 8], [56, 8], [32, 56]]},
            "palette": ["#101010"],
            "brush": "100%_Opaque",
            "count": 300,
            "size_range": [8, 12]
        }),
    );
    assert_eq!(
        scattered.get("ok").and_then(Value::as_bool),
        Some(true),
        "撒点应当成功：{scattered}"
    );
    let placed = scattered
        .get("strokes")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let skipped = scattered
        .get("skipped_outside")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert!(placed > 0, "至少要有落笔（否则判据是空转）：{scattered}");
    // **恒等式**：`落笔 + 越界跳过 ≤ count`（计数器之间不许自相矛盾 ✓）
    assert!(
        placed + skipped <= 300,
        "`strokes + skipped_outside` 不许超过 `count` ⇒ 实测 {placed} + {skipped}"
    );

    // 形内（三角形中部）必须有墨
    let inside = samples(&mut ctx, &[[32, 20], [28, 26], [36, 28], [30, 34]]);
    assert!(
        inside.iter().any(|rgba| has_ink(*rgba)),
        "三角形内部必须有墨 ⇒ 实测 {inside:?}"
    );
    // **形外但盒内**：这些点距边最近约 3~5px ⇒ 半径 5px 的 dab 以前会漏进来
    let outside = samples(
        &mut ctx,
        &[
            [12, 44],
            [14, 40],
            [52, 44],
            [50, 40],
            [32, 4],
            [8, 4],
            [56, 4],
        ],
    );
    let inked = outside.iter().filter(|rgba| has_ink(**rgba)).count();
    assert_eq!(
        inked, 0,
        "多边形**外面**（但在外接盒内）不许有墨 ⇒ 实测 {inked}/7 有墨：{outside:?}"
    );
}

/// **第 1440 轮审计**：`direction` 拼错（"digonal"/"45deg"）曾被 `.ok()` 吞掉 ⇒
/// **静默退回 random** ✗。现在明确报错并列出可用形态 ✓。
/// 变异（退回 `.ok().map(...)`）⇒ 第一段的 `Some(false)` 断言拿不到 ⇒ 红 ✓。
#[test]
fn scatter_direction_typo_is_refused_not_randomized() {
    let mut workspace = workspace();
    {
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_scatter_clip", "width": 64, "height": 64}),
        );
    }
    let base = |direction: &str| {
        json!({
            "layer_id": "layer_default",
            "seed": 7,
            "area": {"x": 4, "y": 4, "w": 56, "h": 56},
            "palette": ["#101010"],
            "brush": "100%_Opaque",
            "count": 2,
            "direction": direction
        })
    };
    let mut ctx = context(&mut workspace);

    let refused = call(&mut ctx, "scatter_strokes", &base("digonal"));
    assert_eq!(
        refused.get("ok").and_then(Value::as_bool),
        Some(false),
        "拼错的 direction 必须被拒（不许静默 random）：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    for allowed in ["horizontal", "vertical", "random"] {
        assert!(
            detail.contains(allowed),
            "要列出可用形态 {allowed}：{detail}"
        );
    }
    assert!(detail.contains("digonal"), "要点名坏输入：{detail}");

    // 数字角度仍然可用（拒绝只针对**解析不了**的）✓。
    let angled = call(&mut ctx, "scatter_strokes", &base("45"));
    assert_eq!(
        angled.get("ok").and_then(Value::as_bool),
        Some(true),
        "45（度）应当照常接受：{angled}"
    );
}
