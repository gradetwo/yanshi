//! **纯橡皮补丁端到端**（Y26-001 路线 B）：`brush_stroke` 用纯橡皮笔刷擦除
//! 已有底色 ⇒ 回执 `ok=true` 且渲染上真的少了墨（修前恒 `ok=false`／
//! `precondition_failed`／`changed_px=0`）；擦除后同一层再画不透明笔 ⇒
//! 墨恢复（erase 补丁不污染后续 source-over）；无标记的旧补丁语义不变
///（由对象解析单测 + 既有回归覆盖，这里只验行为）。
use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **测试结束即删掉临时目录**（`/tmp` 是 1.9 GB 的 tmpfs；判据 check-tests-clean-temp 要求）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **落盘工作区 + 仓库里的 `assets/`** —— `brush_stroke` 要从那儿读 `.myb` 笔刷。
fn workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!("yanshi_erase_{name}_{}", std::process::id()));
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_erase", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_erase", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    ToolRegistry::with_profiles(&Profile::ALL).call(&mut ctx, tool, &args)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_erase", Bbox::new(0.0, 0.0, 128.0, 64.0))
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

/// 建层 + 一条横贯的不透明底色带（给橡皮“有东西可擦”）。
fn fresh_with_base(workspace: &mut Workspace) {
    assert_eq!(
        call(workspace, "create_layer", json!({ "layer_id": "L" }))["ok"],
        json!(true)
    );
    let base = call(
        workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque",
            "points": [[8.0, 32.0, 1.0], [120.0, 32.0, 1.0]], "size": 30,
            "color": [0.45, 0.38, 0.30],
        }),
    );
    assert_eq!(base["ok"], json!(true), "{base}");
}

/// **纯橡皮一笔变绿、渲染上真的少了墨、带外像素不动**。
#[test]
fn pure_eraser_stroke_turns_green_and_really_erases() {
    let mut workspace = workspace("green");
    fresh_with_base(&mut workspace);
    let before = render(&mut workspace);
    let center_before = pixel(&before, 64, 32);
    // 底色带中心必须是不透明底色（否则“擦掉”无从谈起）。
    assert!(
        center_before[3] > 200,
        "底色带中心应不透明，实测 {center_before:?}"
    );

    let erased = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "Hard_Eraser",
            "points": [[8.0, 32.0, 1.0], [120.0, 32.0, 1.0]], "size": 30,
        }),
    );
    assert_eq!(erased["ok"], json!(true), "{erased}");
    assert_eq!(erased["landed"], json!(true), "{erased}");

    let after = render(&mut workspace);
    let center_after = pixel(&after, 64, 32);
    let rgb_delta = (0..3)
        .map(|c| (i32::from(center_before[c]) - i32::from(center_after[c])).unsigned_abs())
        .max()
        .unwrap_or(0);
    assert!(
        rgb_delta > 50,
        "擦除后中心像素 RGB 应大变（底色→底衬），实测 {center_before:?} → {center_after:?}"
    );
    // 带外像素（底色带外、文档内）必须逐字节不动：擦除不能漏到别处。
    assert_eq!(
        pixel(&before, 10, 60),
        pixel(&after, 10, 60),
        "带外像素必须不动"
    );
}

/// **擦除后同一层再画不透明笔 ⇒ 墨恢复**（erase 补丁在下的折叠顺序正确，
/// 且后续 source-over 不受影响）。
#[test]
fn opaque_pen_repaints_over_erased_region() {
    let mut workspace = workspace("repaint");
    fresh_with_base(&mut workspace);
    let erased = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "Hard_Eraser",
            "points": [[8.0, 32.0, 1.0], [120.0, 32.0, 1.0]], "size": 30,
        }),
    );
    assert_eq!(erased["ok"], json!(true), "{erased}");
    let repainted = call(
        &mut workspace,
        "brush_stroke",
        json!({
            "layer_id": "L", "brush": "100%_Opaque",
            "points": [[8.0, 32.0, 1.0], [120.0, 32.0, 1.0]], "size": 30,
            "color": [0.45, 0.38, 0.30],
        }),
    );
    assert_eq!(repainted["ok"], json!(true), "{repainted}");
    let after = render(&mut workspace);
    let center = pixel(&after, 64, 32);
    assert!(center[3] > 200, "重画后中心应恢复不透明，实测 {center:?}");
}
