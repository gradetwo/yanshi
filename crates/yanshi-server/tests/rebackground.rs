//! **改已有文档底色 `rebackground`**（反馈报告 §4 单拆项）。
//!
//! 四条缺一不可 ✓：
//! ① 改底色后渲染像素确实变化（不是只改了状态字段、画面却不动的静默退化 ✗）；
//! ② 非法颜色明确报错、且报错后底色与日志都不动（不错色、不偷改 ✗）；
//! ③ 冷重开后底色仍在（新进程级工作区 + 全量日志重放，不是内存状态 ✗）；
//! ④ 原子重放与撤销往返一致（重放得新底色、撤销回旧底色、恢复又回新底色 ✓）。

use serde_json::json;
use yanshi_core::fold::fold_atoms;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（纹理背景测试同款 ✓；`/tmp` 是 tmpfs，夹具留着会写满 ✗）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_rebg_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_bg", 64, 48),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, "doc_bg", "human:1", "session:test");
    registry().call(&mut ctx, tool, &args)
}

/// 全幅渲染，取一个像素（RGBA8）。
fn pixel(workspace: &mut Workspace, x: u32, y: u32) -> [u8; 4] {
    let (width, _height, pixels) = workspace
        .document_mut("doc_bg")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 64.0, 48.0))
        .expect("区域渲染应成功");
    let offset = ((y * width + x) * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn assert_close(actual: [u8; 4], expected: [u8; 4], what: &str) {
    for channel in 0..4 {
        let delta = (i16::from(actual[channel]) - i16::from(expected[channel])).abs();
        assert!(
            delta <= 2,
            "{what}：通道 {channel} 偏离过大（实际 {actual:?}，期望 {expected:?}）"
        );
    }
}

/// **改底色后渲染像素确实变化** ✓ —— 中心与角落都变，且回执说清改前改后。
#[test]
fn rebackground_changes_rendered_pixels() {
    let root = temp_dir("pixels");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    assert_close(
        pixel(&mut workspace, 32, 24),
        [255, 255, 255, 255],
        "改前中心像素",
    );
    let changed = call(
        &mut workspace,
        "rebackground",
        json!({ "color": { "r": 30, "g": 60, "b": 120, "a": 255 } }),
    );
    assert_eq!(changed["ok"], json!(true), "{changed}");
    assert_eq!(
        changed["dirty_kind"],
        json!("full"),
        "底色是整幅复合基底 ⇒ 必须全量失效：{changed}"
    );
    assert_eq!(
        changed["previous_background"],
        json!({ "r": 255, "g": 255, "b": 255, "a": 255 }),
        "回执要说清改前底色：{changed}"
    );
    assert_eq!(
        changed["background"],
        json!({ "r": 30, "g": 60, "b": 120, "a": 255 }),
        "回执要说清改后底色：{changed}"
    );
    assert_close(
        pixel(&mut workspace, 32, 24),
        [30, 60, 120, 255],
        "改后中心像素",
    );
    assert_close(
        pixel(&mut workspace, 0, 0),
        [30, 60, 120, 255],
        "改后角落像素",
    );
    let state = workspace.document("doc_bg").unwrap().state().clone();
    assert_eq!(
        state.background,
        json!({ "r": 30, "g": 60, "b": 120, "a": 255 }),
        "文档状态里的底色必须同步换掉"
    );
}

/// **非法颜色明确报错、且什么都不改** ✓ —— 缺失/非对象/缺通道/非整数/越界全拒。
#[test]
fn rebackground_rejects_bad_color_without_changing_anything() {
    let root = temp_dir("badcolor");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    for (what, args) in [
        ("不给颜色", json!({})),
        ("颜色非对象", json!({ "color": "blue" })),
        ("缺通道", json!({ "color": { "r": 1, "g": 2 } })),
        ("非整数", json!({ "color": { "r": 1.5, "g": 2, "b": 3 } })),
        ("越界", json!({ "color": { "r": 256, "g": 2, "b": 3 } })),
        ("负数", json!({ "color": { "r": -1, "g": 2, "b": 3 } })),
    ] {
        let reply = call(&mut workspace, "rebackground", args);
        assert_eq!(reply["ok"], json!(false), "{what} 必须明确报错：{reply}");
    }
    let state = workspace.document("doc_bg").unwrap().state().clone();
    assert_eq!(
        state.background,
        json!({ "r": 255, "g": 255, "b": 255, "a": 255 }),
        "连串非法调用后底色必须还是白"
    );
    let has_set_background = workspace
        .document("doc_bg")
        .unwrap()
        .log()
        .iter()
        .any(|atom| atom.kind == yanshi_core::AtomKind::SetBackground);
    assert!(
        !has_set_background,
        "非法调用不许落底色原子（否则重放会把错色扶正）"
    );
}

/// **冷重开后底色仍在** ✓ —— 全新工作区对象 + 全量日志重放，不是内存状态。
#[test]
fn rebackground_survives_cold_reopen() {
    let root = temp_dir("reopen");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    let changed = call(
        &mut workspace,
        "rebackground",
        json!({ "color": { "r": 10, "g": 200, "b": 90, "a": 255 } }),
    );
    assert_eq!(changed["ok"], json!(true), "{changed}");
    assert!(workspace.close_document("doc_bg"), "关闭应当成功");
    drop(workspace);
    let mut reopened = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("同一落盘根上应当能重建工作区");
    reopened.open_document("doc_bg").expect("冷重开应当成功");
    let state = reopened.document("doc_bg").unwrap().state().clone();
    assert_eq!(
        state.background,
        json!({ "r": 10, "g": 200, "b": 90, "a": 255 }),
        "冷重开后底色必须还是改过的那个"
    );
    let (kind, count) = {
        let log = reopened.document("doc_bg").unwrap().log();
        let found: Vec<_> = log
            .iter()
            .filter(|atom| atom.kind == yanshi_core::AtomKind::SetBackground)
            .collect();
        let kind = found
            .first()
            .map(|atom| atom.kind.as_str().to_owned())
            .unwrap_or_default();
        (kind, found.len())
    };
    assert_eq!(
        kind, "set_background",
        "日志里的底色原子字面量必须是 set_background"
    );
    assert_eq!(count, 1, "只应有一条底色原子");
    assert_close(
        pixel(&mut reopened, 32, 24),
        [10, 200, 90, 255],
        "冷重开后渲染像素",
    );
}

/// **重放一致 + 撤销往返** ✓ —— 从头折叠日志得新底色；撤销回白、恢复回新色。
#[test]
fn rebackground_replays_and_reverts() {
    let root = temp_dir("replay");
    let _cleanup = Cleanup(root.clone());
    let mut workspace = workspace(&root);
    let changed = call(
        &mut workspace,
        "rebackground",
        json!({ "color": { "r": 30, "g": 60, "b": 120, "a": 255 } }),
    );
    assert_eq!(changed["ok"], json!(true), "{changed}");
    let atom_id = changed["atom_id"].as_str().unwrap_or_default().to_owned();
    assert!(!atom_id.is_empty(), "回执必须给底色原子的 id：{changed}");
    // **重放**：从头折叠整条日志 ⇒ 底色必须与在线状态逐字一致。
    let (atoms, live_background) = {
        let document = workspace.document("doc_bg").unwrap();
        (
            document.log().iter().cloned().collect::<Vec<_>>(),
            document.state().background.clone(),
        )
    };
    let replayed = fold_atoms(Default::default(), &atoms);
    assert!(
        replayed.warnings.is_empty(),
        "重放不该有折叠警告：{:?}",
        replayed.warnings
    );
    assert_eq!(
        replayed.state.background, live_background,
        "重放底色必须与在线状态一致"
    );
    assert_eq!(
        replayed.state.background,
        json!({ "r": 30, "g": 60, "b": 120, "a": 255 }),
        "重放底色必须是改过的那个"
    );
    // **撤销**：底色回白、像素回白。
    let reverted = call(&mut workspace, "revert", json!({ "atom_id": atom_id }));
    assert_eq!(reverted["ok"], json!(true), "{reverted}");
    let state = workspace.document("doc_bg").unwrap().state().clone();
    assert_eq!(
        state.background,
        json!({ "r": 255, "g": 255, "b": 255, "a": 255 }),
        "撤销底色原子后底色必须回建文档时的白"
    );
    assert_close(
        pixel(&mut workspace, 32, 24),
        [255, 255, 255, 255],
        "撤销后中心像素",
    );
    // **恢复**：底色与像素都回新色。
    let reapplied = call(&mut workspace, "reapply", json!({ "atom_id": atom_id }));
    assert_eq!(reapplied["ok"], json!(true), "{reapplied}");
    let state = workspace.document("doc_bg").unwrap().state().clone();
    assert_eq!(
        state.background,
        json!({ "r": 30, "g": 60, "b": 120, "a": 255 }),
        "恢复底色原子后底色必须回新色"
    );
    assert_close(
        pixel(&mut workspace, 32, 24),
        [30, 60, 120, 255],
        "恢复后中心像素",
    );
}
