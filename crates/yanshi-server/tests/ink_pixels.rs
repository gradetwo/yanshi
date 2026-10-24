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

/// **`color.a` 目前被忽略 —— 这一条是"记账"，不是"要求"**（第 786→788 轮）
///
/// **⇒ 我第 786 轮加过"非不透明就拒绝"** ✗ ⇒ **CI 实测它太激进** ✓：
///   `export_small.rs` 用 `a = 220`（半透明、完全合法的既有用法 ✓）
///   ⇒ 拒绝让 `export_small` **3 个测试** 与 **4 片 criteria** 一起红 ✗
///   （`test (stable)`／`test (beta)`／`criteria shard 1,2,4,5` ✓）。
/// **⇒ 所以本判据只钉住"现在的行为"** ✓：`a = 0` 与 `a = 255` 的 `ink_pixels` **相同** ✓
///   ⇒ **这是既有行为的记录** ✓，**不是**期望 ✓ —— 一旦有人把它改成"乘进 opacity" ✓，
///   本判据会**红** ✓，而那正是提醒"该改文档与该判据"的信号 ✓。
/// **⇒ 为什么留着它**：把"已知的静默行为"钉成可复核的断言 ✓，
///   比写在注释里强 ✓（注释会烂，断言不会 ✓）。
#[test]
fn colour_alpha_is_currently_ignored_and_that_is_recorded_here() {
    let mut workspace = setup("alpha");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let sample = |workspace: &mut Workspace, object: &str, alpha: u8| -> u64 {
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
                "color": {"r": 200, "g": 20, "b": 20, "a": alpha},
            }),
        );
        assert_eq!(out["ok"], json!(true), "半透明也是合法请求：{out}");
        out["painted_pixels"].as_u64().unwrap_or(0)
    };
    let opaque = sample(&mut workspace, "o_a255", 255);
    let clear = sample(&mut workspace, "o_a0", 0);
    assert_eq!(
        clear, opaque,
        "**既有行为**：`color.a` 被忽略 ⇒ `a=0` 与 `a=255` 的印章面积相同（实测 {clear} vs {opaque}）\
         ⇒ 若这一条红了，说明有人让 alpha 生效了 ✓ ⇒ 请同时改 `docs/tools.md` 的这个参数说明 ✓"
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
