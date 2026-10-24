//! **`brush_stroke` 必须区分"印章面积"与"真正落墨"**（第 784 轮；外部 `qoder` D139 的可测量部分）
//!
//! **⇒ 为什么要这个字段** ✗：他们的 D139 说「有印章、但一个像素都没落墨」是**静默区** ✓
//!   而回执里只有 `painted_pixels`（**印章面积** ✓）⇒ 它**看不出**这件事 ✓
//!   （实测：一笔全透明参数照样 `painted_pixels = 324` ✓）。
//!
//! **⇒ 判据（恒等式，跨输入成立 ✓）**：
//!   ① `ink_pixels ≤ painted_pixels` ✓ —— 落墨像素不可能超过印章面积 ✓；
//!   ② 不透明的一笔 `ink_pixels ≥ 1` ✓ —— 否则这个字段没有意义 ✓；
//!   ③ 两条接口口径不同：`size=6` 与 `size=1.5` 的 `painted_pixels` **必须不同** ✓
//!      （证明它真的随参数变 ✓，不是常量 ✓）。
//! **⇒ 变异**：① 把 `ink_pixels` 写成 `painted + 1` ⇒ 第一条红 ✓；
//!   ② 写成 `0` ⇒ 第二条红 ✓。
//! **⇒ 坦白**：把 `ink_pixels` 直接写成 `painted_pixels` 的**副本**时本判据**抓不到** ✓
//!   （实测 `size=6` 两者都是 324 ✓）⇒ 那时要靠 D139 的复现来抓 ✓，而**我还缺那组参数** ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn setup(tag: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_inkpx_{tag}_{}", std::process::id()));
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
    let ink = thick["ink_pixels"].as_u64().expect("ink_pixels 必须存在");
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
    let thin_ink = thin["ink_pixels"].as_u64().expect("ink_pixels");
    assert!(
        thin_painted < painted,
        "细笔的印章面积应当更小（{thin_painted} vs {painted}）⇒ 两个字段都要随 size 动：{thin}"
    );
    assert!(
        thin_ink <= thin_painted,
        "`ink_pixels`({thin_ink}) 不可能超过 `painted_pixels`({thin_painted})：{thin}"
    );
}

/// **`color.a` 不被支持时必须说出来，而不是画成不透明**（第 786 轮；我扫描时发现的）
///
/// **⇒ 实测（第 785 轮扫描）**：`a=0`／`a=1`／`a=255` 的 `ink_pixels` **一模一样**
/// （212/212、216/216、355/355 ✓）⇒ `brush_color_to_srgb_bytes` **只取 RGB** ✓
/// ⇒ 传 `a: 0`（完全透明）会得到**不透明**的一笔 ✓ —— **与意图相反** ✓。
///
/// **⇒ 判据**：`color.a = 0` ⇒ `ok=false` 且文案提到 `color.a` 与 `opacity` ✓；
/// `color.a = 255` ⇒ 正常落墨 ✓（证明拒绝不是"什么都拒" ✓）。
/// **⇒ 变异**：去掉那段检查 ⇒ 第一条红 ✓。
#[test]
fn a_transparent_colour_is_refused_instead_of_painting_opaque() {
    let mut workspace = setup("alpha");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_inkpx", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    let refused = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_alpha0",
            "brush": "8B_Pencil#1",
            "points": [[8.0, 8.0, 1.0], [40.0, 40.0, 1.0]],
            "size": 6.0,
            "color": {"r": 200, "g": 20, "b": 20, "a": 0},
        }),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "`color.a = 0` 必须被拒（否则会画成不透明，与意图相反）：{refused}"
    );
    let text = refused.to_string();
    assert!(
        text.contains("color.a") && text.contains("opacity"),
        "文案要说清「alpha 归笔刷的 opacity」：{refused}"
    );

    // 对照：不透明的颜色照常落墨
    let solid = stroke(&mut workspace, "o_alpha255", 6.0);
    assert_eq!(solid["ok"], json!(true), "{solid}");
    assert!(
        solid["ink_pixels"].as_u64().unwrap_or(0) > 0,
        "不透明的颜色必须真的落墨：{solid}"
    );
}
