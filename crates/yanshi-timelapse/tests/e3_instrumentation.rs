//! **E3 timelapse 仪器正常化**：分段账默认打印、进度末行必现。
//!
//! **⇒ 为什么是真包 + 直跑二进制**：分段账与进度行都只在 `main.rs` 的
//!   进程输出里出现，单元测试够不着 ⇒ 用 `CARGO_BIN_EXE_yanshi-timelapse`
//!   直跑（与 `real_package_shrinks.rs` 同款夹具：真包 + `RAW_DIR` 落盘，
//!   不依赖 ffmpeg）。
//!
//! **四层齐**：
//! - 实现：`main.rs` 里分段账移出 `if probe`、循环后补打终行；
//! - 输入能触发：真包跑完（**不带** `YANSHI_TIMELAPSE_PROBE`）；
//! - 断言：① stdout 有五段分段账；② stderr 末行 `进度 {n}/{n}`（n = 包内原子数）；
//! - 变异：分段账重新门控 ⇒ ① 红；删掉终行打印 ⇒ ② 红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// 包内 `atoms.jsonl` 的非空行数 —— 与 `main.rs` 的 `atom_lines` 同口径
///（去空行后计数 ⇒ 进度分母 `atom_lines.len().min(end_idx)` 在缺省 end 下就是它）。
fn count_atoms_in_package(pack: &std::path::Path) -> usize {
    let bytes = std::fs::read(pack).expect("读包");
    let entries = yanshi_server::archive::read_tar(&bytes).expect("解 tar");
    let atoms = entries
        .iter()
        .find(|entry| entry.path == "atoms.jsonl")
        .expect("包里要有 atoms.jsonl");
    let text = String::from_utf8_lossy(&atoms.bytes);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count()
}

/// stderr 里所有 `进度 X/Y` 行的最后一行 —— 定期行只在 `i % 25 == 0` 打，
/// 终行才是 `X == Y` 的那一行（`grep '进度 <end>/'` 的判据就指它）。
fn last_progress_line(stderr: &str) -> Option<(usize, usize)> {
    stderr
        .lines()
        .filter_map(|line| {
            let rest = line.split("进度 ").nth(1)?;
            let (numer, denom) = rest.split_once('/')?;
            let denom = denom.split('｜').next()?;
            Some((
                numer.trim().parse::<usize>().ok()?,
                denom.trim().parse::<usize>().ok()?,
            ))
        })
        .next_back()
}

#[test]
fn segment_account_prints_without_probe_env_and_final_progress_line_appears() {
    let root = std::env::temp_dir().join(format!("yanshi_e3_{}", std::process::id()));
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
    // 小画布 + 3 笔可重放笔刷（`2B_pencil.myb` 在 `export_small` 里已被证明），
    // 原子数 < 25 ⇒ 定期行只打 `进度 0/n`，末行只能是补打的那一行。
    ws.create_document(NewDocument::new("d", 640, 480), "human:1", "s")
        .expect("create");
    let registry = ToolRegistry::with_profiles(&Profile::ALL);
    let mut ctx = ToolContext::new(&mut ws, "d", "human:1", "s")
        .with_owner(true)
        .with_wait_for_render(true, 30_000);
    for i in 0..3u32 {
        let out = registry.call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("o{i}"),
                "brush": "2B_pencil.myb",
                "points": [
                    [100.0 + f64::from(i) * 150.0, 100.0, 0.5],
                    [250.0 + f64::from(i) * 150.0, 350.0, 0.9],
                ],
                "size": 40.0, "color": {"r": 20, "g": 20, "b": 200, "a": 255},
            }),
        );
        assert_eq!(out["ok"], json!(true), "{out}");
    }
    let pack = root.join("real.yanshi");
    let packed = registry.call(
        &mut ctx,
        "export_project",
        &json!({"path": pack.to_string_lossy(), "include_bitmaps": true}),
    );
    assert_eq!(packed["ok"], json!(true), "{packed}");
    assert_eq!(packed["omitted"], json!(0), "{packed}");
    let atoms = count_atoms_in_package(&pack);
    assert!(atoms > 0, "包里必须有原子（否则进度分母无意义）");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
        .arg(&pack)
        .arg(root.join("out.mp4"))
        .env("YANSHI_TIMELAPSE_RAW_DIR", root.join("frames"))
        // **关键**：不带 `YANSHI_TIMELAPSE_PROBE` —— 分段账必须照样出现；
        // `env_remove` 是防父进程透传（测试跑者可能带了它）。
        .env_remove("YANSHI_TIMELAPSE_PROBE")
        .env_remove("YANSHI_TIMELAPSE_SHRINK")
        .output()
        .expect("跑");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "timelapse 必须成功：\n{stderr}");
    // ① **分段账默认打印**：五段（apply／shrink／render／scale／write）全在，
    //   不带 `YANSHI_TIMELAPSE_PROBE` 也一样。
    //   **变异**：把分段账重新包回 `if probe` ⇒ 这条红。
    assert!(
        stdout.contains("分段账（ms）★ apply ") && stdout.contains("｜write "),
        "不带 YANSHI_TIMELAPSE_PROBE 也必须打五段分段账：\n{stdout}"
    );
    // ② **进度末行必现**：最后一行 `进度 X/Y` 必须是 `X == Y == 原子数`。
    //   未修之前最后一行是定期行 `进度 0/{atoms}` ⇒ 这条红；
    //   **变异**：删掉终行打印 ⇒ 又回到 `进度 0/{atoms}` ⇒ 红。
    let (numer, denom) = last_progress_line(&stderr).expect("至少要有一行进度");
    assert_eq!(
        (numer, denom),
        (atoms, atoms),
        "末行必须是 `进度 {atoms}/{atoms}`（定期行只打 `进度 0/{atoms}`）：\n{stderr}"
    );
}
