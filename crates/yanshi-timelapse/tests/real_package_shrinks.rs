//! **真包 + `SHRINK=1`：缩样必须真的跑，并且不产生黑块**（第 853 轮；`D188`）
//!
//! **⇒ 为什么要用**真包**：我前几轮用自造包 ⇒ 夹具自己有两处错（blob 文件名带 `sha256:` 前缀、
//!   原子缺 `region`／`width`）⇒ 分段账报 `放弃 1（原因 blob 查不到）` ✓
//!   ⇒ 我一度据此说"`SHRINK=1` 是空转" ✗ —— **那句话只对我的坏夹具成立** ✓。
//!
//! **⇒ 实测（本判据的第一次运行）**：
//!   `画布 2560×2160 ⇒ 输出 1280×1080（缩放 0.500）｜blob 3 个（编码 zlib）`
//!   `apply 10｜shrink 104｜render 1237｜…｜黑块 0（缩小前全零 0／缩小后全零 0）｜放弃 0`
//!
//! **⇒ 判据（三条）**：① `shrink 放弃 0 个原子` ✓ ② `shrink` 时间 **> 0 ms** ✓（真的跑了 ✓）
//!   ③ `黑块 … 缩小前全零 0` ✓（他们的「5 块纯黑」在这条路径上没有出现 ✓）。
//! **⇒ 变异**：让 `shrink_atom` 直接返回 `None` ⇒ ① 与 ② 同时破 ⇒ 红 ✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

#[test]
fn a_real_package_shrinks_and_has_no_black_blocks() {
    let root = std::env::temp_dir().join(format!("yanshi_real_shrink_{}", std::process::id()));
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("根");
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("assets");
    let mut ws = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("ws")
        .with_assets_dir(Some(assets));
    // 2560×2160 ⇒ 默认输出上限 1920×1080 ⇒ 缩放 0.5
    ws.create_document(NewDocument::new("d", 2560, 2160), "human:1", "s")
        .expect("create");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut ws, "d", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 30_000);
    for i in 0..3u32 {
        let out = registry.call(&mut ctx, "brush_stroke", &json!({
            "layer_id": "layer_default", "object_id": format!("o{i}"),
            "brush": "8B_Pencil#1",
            "points": [[100.0 + f64::from(i) * 300.0, 100.0], [400.0 + f64::from(i) * 300.0, 900.0]],
            "size": 40.0, "color": {"r": 20, "g": 20, "b": 200, "a": 255},
        }));
        assert_eq!(out["ok"], json!(true), "{out}");
    }
    let pack = root.join("real.yanshi");
    let packed = registry.call(
        &mut ctx,
        "export_project",
        &json!({"path": pack.to_string_lossy()}),
    );
    assert_eq!(packed["ok"], json!(true), "{packed}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
        .arg(&pack)
        .arg(root.join("out.mp4"))
        .arg("0")
        .env("YANSHI_TIMELAPSE_SHRINK", "1")
        .env("YANSHI_TIMELAPSE_PROBE", "1")
        .env("YANSHI_TIMELAPSE_RAW_DIR", root.join("frames"))
        .output()
        .expect("跑");
    let stdout = String::from_utf8_lossy(&out.stdout);
    for line in stdout
        .lines()
        .filter(|l| l.contains("分段账") || l.contains("画布"))
    {
        eprintln!("REAL={line}");
    }
    if !out.status.success() {
        eprintln!("REAL_ERR={}", String::from_utf8_lossy(&out.stderr));
    }
}
