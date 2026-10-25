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
        let out = registry.call(
            &mut ctx,
            "brush_stroke",
            &json!({
                "layer_id": "layer_default", "object_id": format!("o{i}"),
                // **必须用**可证明可重放**的笔刷** ✓（第 1130 轮；`2B_pencil.myb` 在 `export_small` 里已被证明 ✓）：
                //   ⇒ 否则 `omitted` 恒 0（那 3 笔本来就不可省 ✓）⇒ `omitted == 0` 这条断言**测不出参数** ✗
                //     （第 1130 轮实测：用 `8B_Pencil#1` 时去掉 `include_bitmaps: true`，测试**照样绿** ✗）
                "brush": "2B_pencil.myb",
                "points": [
                    [100.0 + f64::from(i) * 300.0, 100.0, 0.5],
                    [400.0 + f64::from(i) * 300.0, 900.0, 0.9],
                ],
                "size": 40.0, "color": {"r": 20, "g": 20, "b": 200, "a": 255},
            }),
        );
        assert_eq!(out["ok"], json!(true), "{out}");
    }
    let pack = root.join("real.yanshi");
    // **⇒ 必须传 `include_bitmaps: true`** ✓（第 1130 轮；D172 的完整闭环）：
    //   ⇒ 缺省 `false` **故意省掉可重放的笔触位图** ✓ ⇒ 那种包**回放必然缺内容** ✗
    //     ⇒ 所以 timelapse 现在**拒收**缺 blob 的包（`exit(2)` ✓）⇒ 本测试必须自己打一份**完整**的 ✓
    //   **⇒ 这同时是一条判据**：`include_bitmaps: true` ⇒ 包里真装全 ⇒ 缺 blob ＝ 0 ✓
    //     **变异**：把 `include_bitmaps` 去掉（或改 false）⇒ `omitted > 0` ⇒ 缺 blob > 0 ⇒ 红 ✓
    let packed = registry.call(
        &mut ctx,
        "export_project",
        &json!({"path": pack.to_string_lossy(), "include_bitmaps": true}),
    );
    assert_eq!(packed["ok"], json!(true), "{packed}");
    // **⇒ 导出必须**明说**"省了几条 blob"** ✅（第 1118 轮；第 1130 轮改成断 0）：
    //   ⇒ `omitted` 是"被可重放证明省掉"的条数 ✓ ⇒ 传 `include_bitmaps: true` 时**必须是 0** ✓
    //     ⇒ 若这个参数被忽略（或字段没了）⇒ 后者 `is_number` 恒假／前者 > 0 ⇒ 红 ✓
    //   **变异**：把 `tools_io.rs` 响应里的 `"omitted": stats.omitted` 删掉 ⇒ 红 ✓；
    //     把 `service.rs` 的 `if !include_bitmaps` 改成 `if true` ⇒ `omitted > 0` ⇒ 红 ✓
    assert_eq!(
        packed["omitted"],
        json!(0),
        "`include_bitmaps: true` 不许再省位图（否则 timelapse 会拒收）：{packed}"
    );
    assert!(
        packed["blob_count"].as_u64().unwrap_or(0) > 0,
        "完整包里必须有位图（否则这条判据没有意义）：{packed}"
    );
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
    assert!(
        out.status.success(),
        "timelapse 必须成功：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // **⇒ 完整包的"缺 blob"必须是 0** ✓（第 1130 轮）：这条把导出侧与回放侧接成一个闭环 ✓
    //   ⇒ 导出漏装 ⇒ `missing > 0` ⇒ 回放侧 `exit(2)` ⇒ 这里已经红在 status 上 ✓
    //   ⇒ 而这条断言**把数字也钉住** ✓（"成功"只说明不致命，没说"不缺" ✓）
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("缺 blob 0 个"),
        "带全位图的包不该缺 blob（`include_bitmaps: true` 的闭环）：\n{stderr}"
    );
    for line in stdout
        .lines()
        .filter(|line| line.contains("分段账") || line.contains("画布"))
    {
        eprintln!("REAL={line}");
    }
    // ① **真的缩了**：不许有放弃的原子 ✓
    assert!(
        stdout.contains("shrink 放弃 0 个原子（原因 无）"),
        "真包上 `shrink_atom` 必须每一行都能缩：\n{stdout}"
    );
    // ② **真的花了时间** ✓（0 ms 曾经是我误判"空转"的来源 ✓）
    let shrink_ms: f64 = stdout
        .split("shrink ")
        .nth(1)
        .and_then(|rest| rest.split('｜').next())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(
        shrink_ms > 0.0,
        "缩样必须真的花时间（实测 {shrink_ms} ms）：\n{stdout}"
    );
    // ③ **没有黑块** ✓
    assert!(
        stdout.contains("缩小前全零 0"),
        "真包上不许有「缩小前就全零」的块：\n{stdout}"
    );
}
