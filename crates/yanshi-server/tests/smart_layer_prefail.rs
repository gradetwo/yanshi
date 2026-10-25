//! **P2-6**：`smart_*` 前置图层校验（fail-fast）+ 内层预览缺省关。
//!
//! **⇒ 两条行为** ✓（各有判据 ✓）：
//!   ① 给了不存在的 `layer_id` ⇒ **落笔前**就报 `reference_not_found`，且错误里列出现有图层
//!      （probe2 的教训：`smart_hatch_follow` 曾算完 1259 笔、`raster_ms=41586` 后才报，空烧 50s ✓）；
//!   ② 内层笔触缺省不等渲染 ⇒ 一次 `smart_fill` 的响应里 `timings.preview_calls == 0`
//!      （`preview_calls` 是语义计数 ✓，与墙钟无关 ⇒ 跨机器稳定 ✓）；
//!      显式传 `wait_for_render: true` ⇒ `preview_calls > 0`（证明这个参数**被读过** ✓，
//!      不是只声明不生效 ✓）。
//!
//! **⇒ 变异** ✓：逐个删掉某工具的校验 ⇒ 对应那条 fail-fast 测试红 ✓；
//!   把 `set_inner_wait_for_render` 的缺省改回 `true` ⇒ ② 的缺省臂红、显式臂仍绿 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_smart", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_smart", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "yanshi_smart_prefail_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

/// 落盘工作区 + 资产目录指向**仓库里的 `assets/`** ✓ —— 接线测试要真的落笔 ⇒
/// 内存工作区里没有笔刷（`calligraphy` 都找不到 ⇒ 笔触一条都落不下 ✗）。
/// fail-fast 测试不需要笔刷 ⇒ 仍用内存工作区（快 ✓）。
fn workspace_with_assets(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new("doc_smart", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

/// **fail-fast 断言** ✓：不存在的图层 ⇒ `reference_not_found`，且错误里列出现有图层 ✓。
///   **注意** ✓：旧路径（内层笔触落盘时 fold 才查图层）也会报同一个错误码 ✓ ⇒
///   只断错误码**变异不红** ✗ ⇒ 必须断错误文本里的「落笔前校验」标记 —— 那是前置校验独有的 ✓，
///   证明错误来自落笔**前**、不是算完才报 ✓。
fn assert_missing_layer_rejected(tool: &str, args: serde_json::Value) {
    let mut workspace = workspace();
    let mut ctx = context(&mut workspace);
    let made = registry().call(&mut ctx, tool, &args);
    assert_eq!(
        made["ok"],
        json!(false),
        "{tool} 给了不存在的图层应当失败：{made}"
    );
    assert_eq!(
        made["error_code"],
        json!("reference_not_found"),
        "{tool} 应当报 reference_not_found：{made}"
    );
    let detail = made["context"]["detail"].as_str().unwrap_or("");
    assert!(
        detail.contains("落笔前校验"),
        "{tool} 的错误应当来自前置校验（不是算完才在 fold 里报）：{made}"
    );
    assert!(
        detail.contains("layer_default"),
        "{tool} 的错误应当列出现有图层（调用方才知道该传什么）：{made}"
    );
}

#[test]
fn smart_fill_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_fill",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
            "color": [0.8, 0.2, 0.2, 1.0],
        }),
    );
}

#[test]
fn smart_blend_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_blend",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
            "color_a": [0.8, 0.2, 0.2, 1.0],
            "color_b": [0.2, 0.2, 0.8, 1.0],
        }),
    );
}

#[test]
fn smart_glaze_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_glaze",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
            "color": [0.8, 0.6, 0.2, 1.0],
        }),
    );
}

#[test]
fn smart_edge_soften_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_edge_soften",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
        }),
    );
}

#[test]
fn smart_texture_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_texture",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
            "texture_type": "canvas",
        }),
    );
}

#[test]
fn smart_sample_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_sample",
        json!({
            "layer_id": "layer_nope",
            "ref": "layer_default",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
        }),
    );
}

#[test]
fn smart_erase_shape_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_erase_shape",
        json!({
            "layer_id": "layer_nope",
            "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
        }),
    );
}

#[test]
fn smart_hatch_follow_rejects_missing_layer_before_painting() {
    assert_missing_layer_rejected(
        "smart_hatch_follow",
        json!({
            "layer_id": "layer_nope",
            "guides": [[[10, 10], [50, 10]]],
            "color": [0.2, 0.2, 0.2, 1.0],
        }),
    );
}

#[test]
fn smart_select_rejects_missing_layer_before_flooding() {
    assert_missing_layer_rejected(
        "smart_select",
        json!({
            "layer_id": "layer_nope",
            "seed": [10, 10],
        }),
    );
}

/// **内层预览缺省关** ✓：缺省调 `smart_fill` ⇒ 不产生任何一次预览阶段记数 ✓；
/// 显式 `wait_for_render: true` ⇒ 每笔内层笔触都等渲染 ⇒ `preview_calls > 0` ✓。
#[test]
fn smart_fill_inner_strokes_skip_render_by_default() {
    let root = temp_dir("wiring");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace_with_assets(&root);
    let args = json!({
        "layer_id": "layer_default",
        "shape": {"type": "rect", "x": 10, "y": 10, "w": 40, "h": 30},
        "color": [0.8, 0.2, 0.2, 1.0],
    });
    // ① **缺省** ✓：内层只落原子、不等渲染 ⇒ `preview_calls == 0` ✓。
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "smart_fill", &args)
    };
    assert_eq!(made["ok"], json!(true), "smart_fill 应当成功：{made}");
    let preview_calls = made["timings"]["preview_calls"]
        .as_u64()
        .unwrap_or(u64::MAX);
    assert_eq!(
        preview_calls, 0,
        "缺省时内层笔触不应产生预览阶段记数（否则每笔都在等脏区渲染）：{made:?}"
    );
    // ② **显式 `wait_for_render: true`** ✓：恢复逐笔等渲染 ⇒ 至少一次预览记数 ✓
    //    —— 这条同时证明注册表声明的这个参数**真的被 handler 读了** ✓。
    let mut args_true = args.clone();
    args_true["wait_for_render"] = json!(true);
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "smart_fill", &args_true)
    };
    assert_eq!(made["ok"], json!(true), "显式 true 时也应当成功：{made}");
    let preview_calls = made["timings"]["preview_calls"].as_u64().unwrap_or(0);
    assert!(
        preview_calls > 0,
        "显式 wait_for_render:true 时内层笔触应当等渲染（preview_calls > 0）：{made:?}"
    );
}
