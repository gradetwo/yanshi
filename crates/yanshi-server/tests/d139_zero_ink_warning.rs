//! **D139：落了墨但画面没变 ⇒ 必须给警告**（第 811 轮；用 `qoder` 给的调用复现 ✓）
//!
//! **⇒ 他们的复现调用**（原样）：800×800、背景 128 灰、`oil-06-paint`、`size 2.0`、
//!   `color #848484`/`color_to #828282`、`opacity 0.35`、`hardness 0.62`、
//!   `points [[400,400,0.9],[400.5,400,0.8]]`、`dabs_per_actual_radius=dabs_per_basic_radius=3`、`humanize=false`。
//!   **⇒ 我这边实测**：`ok=true`、`painted_pixels=8`、`ink_footprint_pixels=6`、`steps=2`、
//!     **`max_channel_delta=0`** ✓ ⇒ **账面有墨、画面无变化** ✓。
//!
//! **⇒ 判据**：① 这一笔必须带 `warning`，且文案同时提到「落了」与「画面」✓；
//!   ② **对照**：一笔正常的不透明笔**不许**带 `warning` ✓（否则警告就没意义 ✓）。
//! **⇒ 变异**：删掉那段检查 ⇒ ① 红 ✓；改成无条件警告 ⇒ ② 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn setup(tag: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_d139_{tag}_{}", std::process::id()));
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
            NewDocument::new("doc_d139", 800, 800)
                .with_background(json!({"r": 128, "g": 128, "b": 128, "a": 255})),
            "human:1",
            "s",
        )
        .expect("create_document");
    workspace
}

#[test]
fn a_stroke_that_leaves_the_picture_untouched_says_so() {
    let mut workspace = setup("silent");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut workspace, "doc_d139", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
    let quiet = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_d139",
            "brush": "oil-06-paint",
            "points": [[400.0, 400.0, 0.9], [400.5, 400.0, 0.8]],
            "size": 2.0,
            "color": {"r": 132, "g": 132, "b": 132, "a": 255},
            "color_to": {"r": 130, "g": 130, "b": 130, "a": 255},
            "opacity": 0.35,
            "hardness": 0.62,
            "humanize": false,
            "brush_params": {"dabs_per_actual_radius": 3.0, "dabs_per_basic_radius": 3.0},
        }),
    );
    assert_eq!(quiet["ok"], json!(true), "这组参数是合法的：{quiet}");
    assert!(
        quiet["ink_footprint_pixels"].as_u64().unwrap_or(0) > 0,
        "这一笔确实落了墨（足迹 > 0）：{quiet}"
    );
    assert_eq!(
        quiet["max_channel_delta"],
        json!(0),
        "而画面一个像素都没变（这正是静默零墨）：{quiet}"
    );
    let warning = quiet["warning"].as_str().unwrap_or_default();
    assert!(
        warning.contains("落了") && warning.contains("画面"),
        "落了墨但画面没变时必须给警告，且要说清这两件事：{quiet}"
    );

    // ② 对照：正常不透明的一笔不许带警告
    let mut ctx = ToolContext::new(&mut workspace, "doc_d139", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 8_000);
    let loud = registry.call(
        &mut ctx,
        "brush_stroke",
        &json!({
            "layer_id": "layer_default", "object_id": "o_loud",
            "brush": "8B_Pencil#1",
            "points": [[100.0, 100.0, 1.0], [200.0, 200.0, 1.0]],
            "size": 20.0,
            "color": {"r": 255, "g": 255, "b": 255, "a": 255},
        }),
    );
    assert_eq!(loud["ok"], json!(true), "{loud}");
    assert!(
        loud["max_channel_delta"].as_u64().unwrap_or(0) > 0,
        "对照笔必须真的改了画面：{loud}"
    );
    assert!(
        loud["warning"].is_null(),
        "改了画面的一笔不许带这个警告：{loud}"
    );
}
