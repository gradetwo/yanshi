//! **`brush_stroke` 必须区分"印章面积"与"真正落墨"**（第 784 轮；外部 `qoder` D139 的可测量部分）
//!
//! **⇒ 为什么要这个字段** ✗：他们的 D139 说「有印章、但一个像素都没落墨」是**静默区** ✓
//!   而回执里只有 `painted_pixels`（**印章面积** ✓）⇒ 它**看不出**这件事 ✓
//!   （实测：一笔全透明参数照样 `painted_pixels = 324` ✓）。
//!
//! **⇒ 判据（恒等式，跨输入成立 ✓）**：
//!   ① `ink_footprint_pixels ≤ painted_pixels` ✓ —— 落墨像素不可能超过印章面积 ✓；
//!   ② 不透明的一笔 `ink_footprint_pixels ≥ 1` ✓ —— 否则这个字段没有意义 ✓；
//!   ③ 两条接口口径不同：`size=6` 与 `size=1.5` 的 `painted_pixels` **必须不同** ✓
//!      （证明它真的随参数变 ✓，不是常量 ✓）。
//! **⇒ 变异**：① 把 `ink_footprint_pixels` 写成 `painted + 1` ⇒ 第一条红 ✓；
//!   ② 写成 `0` ⇒ 第二条红 ✓。
//! **⇒ 坦白**：把 `ink_pixels` 直接写成 `painted_pixels` 的**副本**时本判据**抓不到** ✓
//!   （实测 `size=6` 两者都是 324 ✓）⇒ 那时要靠 D139 的复现来抓 ✓，而**我还缺那组参数** ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};
/// **测试结束即删掉临时目录** ✓（第 814 轮；`/tmp` 是 1.9 GB 的 tmpfs ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup(tag: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_inkpx_{tag}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    assert!(assets.is_dir(), "找不到资产根：{}", assets.display());
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new("doc_inkpx", 64, 64), "human:1", "s")
        .expect("create_document");
    workspace
}

fn stroke(workspace: &mut Workspace, object_id: &str, size: f64) -> serde_json::Value {
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": object_id,
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
            "size": size,
            "color": {"r": 200, "g": 20, "b": 20, "a": 255},
        }),
    )
}

#[test]
fn ink_pixels_never_exceeds_the_stamp_area_and_is_positive_for_an_opaque_stroke() {
    let mut workspace = setup("main");
    let thick = stroke(&mut workspace, "o_thick", 6.0);
    assert_eq!(thick["ok"], json!(true), "{thick}");
    let painted = thick["painted_pixels"].as_u64().expect("painted_pixels");
    let ink = thick["ink_footprint_pixels"]
        .as_u64()
        .expect("ink_footprint_pixels 必须存在（名字要量它真正量的东西）");
    // **别名必须与主字段同值** ✓（第 808 轮；外部 D192 的改名建议）
    assert_eq!(
        thick["ink_pixels"], thick["ink_footprint_pixels"],
        "`ink_pixels` 只是兼容别名，值必须与 `ink_footprint_pixels` 相同"
    );
    // ② 不透明的一笔必须真的落墨
    assert!(
        ink >= 1,
        "不透明的一笔 `ink_pixels` 必须 ≥1，实测 {ink}：{thick}"
    );
    // ① 恒等式：落墨像素 ≤ 印章面积
    assert!(
        ink <= painted,
        "`ink_pixels`({ink}) 不可能超过 `painted_pixels`({painted})：{thick}"
    );

    // ③ 细笔：印章面积必须随 size 变（证明两个字段都在随参数动 ✓）
    let thin = stroke(&mut workspace, "o_thin", 1.5);
    assert_eq!(thin["ok"], json!(true), "{thin}");
    let thin_painted = thin["painted_pixels"].as_u64().expect("painted_pixels");
    let thin_ink = thin["ink_footprint_pixels"]
        .as_u64()
        .expect("ink_footprint_pixels");
    assert!(
        thin_painted < painted,
        "细笔的印章面积应当更小（{thin_painted} vs {painted}）⇒ 两个字段都要随 size 动：{thin}"
    );
    assert!(
        thin_ink <= thin_painted,
        "`ink_pixels`({thin_ink}) 不可能超过 `painted_pixels`({thin_painted})：{thin}"
    );
}

/// **`color.a` 乘进整笔 opacity** ✓（P-19，paint-fix 期 7）。
///
/// **⇒ 旧判据只钉住"现在的行为"** ✓：`a = 0` 与 `a = 255` 的 `ink_pixels` **相同** ✓
///   ⇒ 那是第 786→788 轮"接受但不生效"的记录 ✓，本轮修法落地后它**必须改** ✓
///   （判据原文自己写着"一旦有人把它改成乘进 opacity，本判据会红" ✓）。
///
/// **⇒ 新行为**：`a=255` 与不给 `a` 逐项相同（与修前逐位一致 ✓）；`a=110` 真透明
/// （`ink_mass` 与 `max_channel_delta` 都比不透明那笔小 ✓）；`a=0` 按既有零墨规矩
/// 被拒（`precondition_failed`，与显式 `opacity: 0` 同款 ✓），什么都不落下、也不留原子 ✓；
/// 拒绝文案必须说清是 opacity 为 0，不能误导去"加长路径" ✓。
/// **⇒ 为什么不用 `ink_footprint_pixels` 断 `a=110`** ✗：它数的是 alpha 非零的像素 ✓ ⇒
/// 半透明时足迹几何基本不变、只有 alpha 变小 ✓ ⇒ 真透明的量是 `ink_mass`（Σalpha ✓，
/// 随 opacity 线性变）与 `max_channel_delta` ✓。
/// **⇒ 变异**：把乘法去掉 ⇒ `a=0` 那笔又成功落下（`ok=true`，足迹回到 270）⇒ 拒绝那格红 ✓；
/// 把零 opacity 文案分支删掉 ⇒ 文案回到"加长路径" ⇒ 含 `opacity 是 0` 那格红 ✓；
/// 把 `factor < 1.0` 的跳过删掉 ⇒ `a=255` 与不给 `a` 的字段仍相等（系数 1.0 恒等 ✓，
/// 这一格抓不到，但 `a=110` 那格照样红 ✓）。
#[test]
fn colour_alpha_multiplies_into_opacity() {
    let mut workspace = setup("alpha");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let sample =
        |workspace: &mut Workspace, object: &str, color: serde_json::Value| -> serde_json::Value {
            let mut ctx = ToolContext::new(workspace, "doc_inkpx", "human:1", "s")
                .with_owner(true)
                .with_wait_for_render(true, 4_000);
            let out = registry.call(
                &mut ctx,
                "brush_stroke",
                &json!({
                    "layer_id": "layer_default", "object_id": object,
                    "brush": "8B_Pencil#1",
                    "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
                    "size": 6.0,
                    "color": color,
                }),
            );
            assert_eq!(
                out["ok"],
                json!(true),
                "半透明也是合法请求（{object}）：{out}"
            );
            out
        };
    let opaque = sample(
        &mut workspace,
        "o_a255",
        json!({"r": 200, "g": 20, "b": 20, "a": 255}),
    );
    let no_alpha = sample(
        &mut workspace,
        "o_no_a",
        json!({"r": 200, "g": 20, "b": 20}),
    );
    // **a=1（a=255）行为不变** ✓：与不给 `a` 逐项相同。
    for field in [
        "painted_pixels",
        "ink_footprint_pixels",
        "ink_mass",
        "max_channel_delta",
    ] {
        assert_eq!(
            opaque[field], no_alpha[field],
            "a=255 必须与不给 a 逐项相同（{field}）：{opaque} vs {no_alpha}"
        );
    }
    let half = sample(
        &mut workspace,
        "o_a110",
        json!({"r": 200, "g": 20, "b": 20, "a": 110}),
    );
    // **a<1 真透明** ✓：印章面积不变（几何没变 ✓），墨量与色差都变小。
    assert_eq!(
        half["painted_pixels"], opaque["painted_pixels"],
        "半透明只改透明度，不改印章面积：{half} vs {opaque}"
    );
    let mass_opaque = opaque["ink_mass"].as_u64().expect("ink_mass");
    let mass_half = half["ink_mass"].as_u64().expect("ink_mass");
    assert!(
        mass_half > 0 && mass_half < mass_opaque,
        "a=110 的墨量应在 0 与不透明之间，实测 {mass_half} vs {mass_opaque}：{half}"
    );
    let delta_opaque = opaque["max_channel_delta"]
        .as_u64()
        .expect("max_channel_delta");
    let delta_half = half["max_channel_delta"]
        .as_u64()
        .expect("max_channel_delta");
    assert!(
        delta_half > 0 && delta_half < delta_opaque,
        "a=110 的色差应在 0 与不透明之间，实测 {delta_half} vs {delta_opaque}：{half}"
    );
    // 数组写法同口径：`[200, 20, 20, 110]` 的墨量与对象式 `a=110` 相同。
    let half_array = sample(&mut workspace, "o_a110_arr", json!([200, 20, 20, 110]));
    assert_eq!(
        half_array["ink_mass"], half["ink_mass"],
        "数组式 a=110 的墨量应与对象式相同：{half_array} vs {half}"
    );
    // **a=0：按既有零墨规矩拒绝，不落墨也不留原子** ✓ —— 有效 opacity 是 0 ⇒
    // raster 零像素 ⇒ `paint_brush` 的 `painted == 0` 前置条件触发 ✓（与显式
    // `opacity: 0` 同款，不是 P-19 新立的规矩 ✓）。"全透明"在这里的意思就是
    // "什么都不落下"；拒绝的文案必须说清是 opacity 为 0（P-19 补的分支 ✓），
    // 不能再让人去"加长路径" ✓。
    let stroke_log_total = |workspace: &mut Workspace| -> u64 {
        let mut ctx = ToolContext::new(workspace, "doc_inkpx", "human:1", "s")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry.call(
            &mut ctx,
            "get_stroke_log",
            &json!({"doc_id": "doc_inkpx", "limit": 1}),
        )["total"]
            .as_u64()
            .unwrap_or(0)
    };
    let total_before = stroke_log_total(&mut workspace);
    let mut ctx = ToolContext::new(&mut workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let clear = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_a0",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
            "size": 6.0,
            "color": {"r": 200, "g": 20, "b": 20, "a": 0},
        }),
    );
    assert_eq!(clear["ok"], json!(false), "a=0 的笔不应成功落下：{clear}");
    assert_eq!(
        clear["error_code"],
        json!("precondition_failed"),
        "a=0 应按零墨规矩拒绝：{clear}"
    );
    let detail = clear["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("opacity 是 0"),
        "a=0 的拒绝文案要说清是 opacity 为 0，不能误导去加长路径：{clear}"
    );
    assert_eq!(
        stroke_log_total(&mut workspace),
        total_before,
        "被拒的 a=0 笔不应留原子：{clear}"
    );
    // **`color.a` 不再进 warnings** ✓：半透明那笔的警告里不许再有"被忽略"。
    if let Some(warnings) = half["warnings"].as_array() {
        assert!(
            warnings
                .iter()
                .all(|note| !note.as_str().unwrap_or_default().contains("被忽略")),
            "color.a 已生效，不应再报被忽略：{half}"
        );
    }
    // **`color_to.a` 仍被忽略** ✓：渐变末端 alpha 没处放 ⇒ 警告保留。
    let mut ctx = ToolContext::new(&mut workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let gradient = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_grad_a",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
            "size": 6.0,
            "color": {"r": 200, "g": 20, "b": 20, "a": 255},
            "color_to": {"r": 20, "g": 20, "b": 200, "a": 0},
        }),
    );
    assert_eq!(gradient["ok"], json!(true), "{gradient}");
    let warnings = gradient["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|note| note.as_str().unwrap_or_default().contains("color_to.a")),
        "color_to.a 仍被忽略，必须警告：{gradient}"
    );
}

/// **`max_channel_delta` 必须说清自己在哪个 regime 下量的**（第 808 轮；外部 `qoder` D193）
///
/// **⇒ 他们的实测**：`reads_canvas=false` 的一支**先把补丁合成到白底**再与 255 比 ✓
/// ⇒ 在**非白底文档**上 `delta = (255 − patch_rgb) × alpha/255` ✓
/// ⇒ 一笔导出 PNG 差 0 像素的笔，这里**报正数** ✗ ⇒ **不能当 zero-ink 阈值** ✓。
///
/// **⇒ 判据**：回执里 `delta_regime` 必须存在，且**与 `fed_base` 一致** ✓
/// （`fed_base=false` ⇒ `white_base`；`true` ⇒ `canvas_base`）✓。
/// **变异**：把 `delta_regime` 写成常量 `"canvas_base"` ⇒ 第一条红 ✓。
#[test]
fn max_channel_delta_names_the_regime_it_was_measured_in() {
    let mut workspace = setup("regime");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    // `8B_Pencil` 不读画布 ⇒ `fed_base=false` ⇒ `white_base`
    let out = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_regime",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
            "size": 6.0,
            "color": {"r": 200, "g": 20, "b": 20, "a": 255},
        }),
    );
    assert_eq!(out["ok"], json!(true), "{out}");
    let regime = out["delta_regime"].as_str().unwrap_or_default();
    let fed = out["fed_base"].as_bool().unwrap_or(false);
    // **三值**（第 810 轮）：吃底图 ⇒ `canvas_base`；否则**读到文档背景** ⇒
    //   `document_background_base`；连背景都没有才 `white_base` ✓。
    let expected = if fed {
        "canvas_base"
    } else {
        "document_background_base"
    };
    assert_eq!(
        regime, expected,
        "`delta_regime` 必须与 `fed_base` 一致（fed_base={fed}）⇒ 否则调用方会把"
    );
    assert!(
        !regime.is_empty(),
        "`max_channel_delta` 旁边必须有 `delta_regime`（这是它唯一的 regime 说明）：{out}"
    );
}

/// **`max_channel_delta` 在没有基准时必须比**文档背景**，不是恒白 255**（第 809 轮）
///
/// **⇒ 他们的建议 (i)**（`qoder` 的 D193／D194）：「无基准时比 `state.background` 的实测背景而不是 255」✓
/// **⇒ 缺陷的量级**：灰底（128）文档上
///   · 一笔**纯白**笔 —— 真实改变 = `|255 − 128| = 127` ✓，旧算法（白底比）报 **0** ✗；
///   · 一笔**什么都没改**的笔 —— 旧算法报 **127** ✗ ⇒ **符号都反了** ✓。
///
/// **⇒ 判据**：灰底 128 的文档上画一笔**纯白** ⇒ `max_channel_delta` 必须 **≥ 120**（真改变）✓；
/// 而同一文档上画一笔**与背景同色**（128 灰）⇒ 必须 **≤ 2** ✓（它真的没改变什么）。
/// **变异**：改回恒白 255 ⇒ 第一条报 0、第二条报 127 ⇒ 两条都红 ✓。
#[test]
fn the_channel_delta_compares_against_the_document_background() {
    for (tag, doc, color, expect_low, weight_of_expectation) in [
        (
            "white_on_grey",
            "doc_bg_white",
            json!({"r": 255, "g": 255, "b": 255, "a": 255}),
            120u64,
            "纯白笔在灰底上必须报出真实改变",
        ),
        (
            "grey_on_grey",
            "doc_bg_grey",
            json!({"r": 128, "g": 128, "b": 128, "a": 255}),
            0u64,
            "同色笔必须报「没改变」",
        ),
    ] {
        let root = std::env::temp_dir().join(format!("yanshi_bg_{tag}_{}", std::process::id()));
        let _cleanup = Cleanup(root.clone());
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建根");
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("assets");
        let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
            .expect("落盘工作区")
            .with_assets_dir(Some(assets));
        workspace
            .create_document(
                NewDocument::new(doc, 64, 64)
                    .with_background(json!({"r": 128, "g": 128, "b": 128, "a": 255})),
                "human:1",
                "s",
            )
            .expect("create_document");
        let registry = ToolRegistry::with_profiles(&Profile::ALL);
        let mut ctx = ToolContext::new(&mut workspace, doc, "human:1", "s")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        let out = registry.call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("o_{tag}"),
                "brush": "8B_Pencil#1",
                "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
                "size": 6.0,
                "color": color,
            }),
        );
        assert_eq!(out["ok"], json!(true), "{out}");
        let delta = out["max_channel_delta"]
            .as_u64()
            .expect("max_channel_delta");
        if expect_low == 0 {
            assert!(
                delta <= 2,
                "与背景同色的笔**什么都没改变**，delta 必须接近 0，实测 {delta} ⇒ {weight_of_expectation}: {out}"
            );
        } else {
            assert!(
                delta >= expect_low,
                "纯白笔在 128 灰底上真实改变约 127，delta 必须 ≥ {expect_low}，实测 {delta} ⇒ {weight_of_expectation}: {out}"
            );
        }
    }
}

/// **预览回执与提交回执用同一套字段**（第 832 轮；外部 §7 第 10 条"没有公共词汇"）
///
/// **⇒ 缺陷**：`swatch_brush_strokes` 每项没有 `ink_footprint_pixels`／`delta_regime` ✗
///   而 `brush_stroke` 两个都有 ✓ ⇒ 同一个问题两条路径两套字段 ✗。
///
/// **⇒ 判据**：每个 `ok` 的 swatch 项都必须有
///   ① `ink_footprint_pixels` ② `ink_pixels` 别名（同值）③ `delta_regime`（三值之一）✓。
/// **变异**：删掉 swatch 里那三行 ⇒ 红 ✓。
#[test]
fn swatch_items_carry_the_same_ink_fields_as_a_committed_stroke() {
    let mut workspace = setup("swatch_ink");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    // **颜色参数名是 `colors`（数组），元素是 `{r,g,b,a}` 或 `#RRGGBB`** ✓
    //   （上一轮我传 `color` 与 `[[r,g,b,a]]` 都被拒 ✓ —— 报错信息里写着可用参数 ✓）
    let out = registry.call(
        &mut ctx,
        "swatch_brush_strokes",
        &json!({
            "brushes": ["8B_Pencil#1", "oil-06-paint"],
            "size": 12.0,
            "colors": [{"r": 200, "g": 20, "b": 20, "a": 255}],
            "preview_size": "64",
        }),
    );
    assert_eq!(
        out["ok"],
        json!(true),
        "swatch 必须成功：{}",
        out["context"]["detail"]
    );
    let items = out["swatches"].as_array().expect("回执里必须有 swatches");
    assert!(!items.is_empty(), "{out}");
    let mut checked = 0;
    for item in items {
        if item["ok"] != json!(true) {
            continue;
        }
        checked += 1;
        let footprint = item["ink_footprint_pixels"]
            .as_u64()
            .unwrap_or_else(|| panic!("swatch 项缺 ink_footprint_pixels：{item}"));
        let alias = item["ink_pixels"]
            .as_u64()
            .unwrap_or_else(|| panic!("swatch 项缺 ink_pixels 别名：{item}"));
        assert_eq!(footprint, alias, "别名必须与主字段同值：{item}");
        let regime = item["delta_regime"].as_str().unwrap_or_default();
        assert!(
            ["canvas_base", "document_background_base", "white_base"].contains(&regime),
            "swatch 项的 delta_regime 必须是三值之一，实测 {regime:?}：{item}"
        );
        // 足迹必须 ≤ 印章面积（与提交路径同一条恒等式 ✓）
        if let Some(painted) = item["painted_pixels"].as_u64() {
            assert!(
                footprint <= painted,
                "足迹不可能超过印章面积（footprint={footprint}, painted={painted}）：{item}"
            );
        }
        // **不透明的笔刷 ⇒ 足迹必须 > 0** ✓（第 863 轮）
        //   **⇒ 为什么加**：原来只断"字段在"与"≤ 印章面积" ✓
        //     ⇒ 一个**恒为 0** 的 `ink_footprint_pixels` **照样通过** ✗（安慰剂 ✓）
        //     ⇒ 这两支笔都是**不透明**的 ⇒ 足迹一定 > 0 ✓
        assert!(
            footprint > 0,
            "不透明的笔刷预览里，足迹必须 > 0（否则这个字段是恒零的安慰剂）：{item}"
        );
    }
    assert!(
        checked > 0,
        "至少要有一项成功，否则这条判据什么也没验：{out}"
    );
}
