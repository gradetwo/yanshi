//! **原子引用的 blob 不在包里 ⇒ 必须响亮记账，不许"跑完退出码 0、画面全空"**（第 780 轮）
//!
//! **⇒ 外部（`qoder` 的 D172）实测**：`yanshi-timelapse` 吃一个"原子引用的 blob 不在包里"的包
//! ⇒ **`rc=0`、一笔不落、一句警告都没有** ⇒ 产出一条**看起来正常的空影片**
//! （14,950 个原子跑完、退出码 0 ✓，而唯一那条全局汇总「⚠️ N 个 atom 没被内核接受」**从未出现** ✓）。
//!
//! **⇒ 为什么它难被发现**：失败在**数据缺失**这一侧 ✓，不在"应用原子"这一侧 ✓
//! ⇒ 任何只看 apply 失败数的汇总都报"一切正常" ✓ ⇒ **静默降级** ✓。
//!
//! **⇒ 判据**：造一个包——`atoms.jsonl` 里有 `import_image` 原子引用某个 64 位哈希，
//! 而包里**没有**那个 blob 文件 ⇒ 跑二进制 ⇒ **stderr 必须提到「缺」＋「blob」** ✓。
//! **⇒ 变异**：把那段审计删掉 ⇒ stderr 里没有任何提示 ⇒ 红 ✓。
//!
//! **⇒ 为什么跑二进制而不是库**：包读取与警告都在 `main.rs`（可执行文件）里 ✓
//! ⇒ 用 `CARGO_BIN_EXE_yanshi-timelapse` ✓；并且用 `YANSHI_TIMELAPSE_RAW_DIR`
//! ⇒ **不需要 ffmpeg** ✓（落盘 RGB 帧 ✓）。
use std::process::Command;

fn package_with_missing_blob(dir: &std::path::Path) -> std::path::PathBuf {
    let meta = serde_json::json!({"width": 64, "height": 64, "doc_id": "d_broken", "fps": 24});
    let hash = "a".repeat(64);
    let atom = serde_json::json!({
        "id": "a0", "seq": 1, "kind": "create_document", "actor": "human:t", "session": "s",
        "timestamp": 0, "schema_version": 1,
        "payload": {"doc_id": "d_broken", "width": 64, "height": 64, "color_space": "srgb"}
    });
    let layer = serde_json::json!({
        "id": "a1", "seq": 2, "kind": "create_layer", "actor": "human:t", "session": "s",
        "timestamp": 0, "schema_version": 1,
        "payload": {"layer_id": "L1", "name": "L1", "type": "raster"}
    });
    // **这一笔引用的 blob 不在包里** ✓
    let stroke = serde_json::json!({
        "id": "a2", "seq": 3, "kind": "import_image", "actor": "human:t", "session": "s",
        "timestamp": 0, "schema_version": 1,
        "payload": {"layer_id": "L1", "object_id": "o1", "bbox": [0.0, 0.0, 64.0, 64.0],
                    "bitmap": {"blob_hash": hash, "mime_type": "image/x-yanshi-raw", "size": 16384}}
    });
    std::fs::write(dir.join("meta.json"), meta.to_string()).expect("meta");
    let mut atoms = String::new();
    for value in [atom, layer, stroke] {
        atoms.push_str(&value.to_string());
        atoms.push('\n');
    }
    std::fs::write(dir.join("atoms.jsonl"), atoms).expect("atoms");
    let pack = dir.join("broken.yanshi");
    let status = Command::new("tar")
        .arg("-cf")
        .arg(&pack)
        .arg("-C")
        .arg(dir)
        .args(["meta.json", "atoms.jsonl"])
        .status()
        .expect("tar");
    assert!(status.success(), "打不出包");
    pack
}

#[test]
fn a_package_missing_a_referenced_blob_says_so() {
    let dir = std::env::temp_dir().join(format!("yanshi_missing_blob_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let pack = package_with_missing_blob(&dir);
    let frames = dir.join("frames");
    let out = dir.join("out.mp4");

    let binary = env!("CARGO_BIN_EXE_yanshi-timelapse");
    let result = Command::new(binary)
        .arg(&pack)
        .arg(&out)
        .arg("1")
        .env("YANSHI_TIMELAPSE_RAW_DIR", &frames)
        .output()
        .expect("跑 timelapse");
    let stderr = String::from_utf8_lossy(&result.stderr);
    let stdout = String::from_utf8_lossy(&result.stdout);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains("缺") && combined.contains("blob"),
        "包里缺了被引用的 blob，而程序一个字都没说 ⇒ 会产出一条看起来正常的空影片（rc={:?}）：\n{combined}",
        result.status.code()
    );
    // **而且要说清楚"缺几个"** ✓（不是笼统的"有问题"）
    assert!(
        combined.contains("缺 blob") || combined.contains("缺 1 个"),
        "文案要给出**个数**，实测：\n{combined}"
    );
}

/// **`SHRINK=1` 必须说清「画会变」**（第 794 轮；外部 `qoder` 的 **D188**）
///
/// **⇒ 他们的实测**：`test1` 跑完 115 帧，**末帧与交付画差 `16.40 lv` 均值、`81.43%` 像素差 > 1 lv**，
///   而**中帧 `14.93 lv` 反而更近** ⇒ 片子越到结尾离作品越远 ✓
///   ⇒ 而 banner **只报「渲染量约降 57×」，不报画会变** ✗。
///
/// **⇒ 根因**（他们读码＋普查，我核过）：`brush_stroke` 最终走 `import_image` 的 `ctx.commit` ✓
///   ⇒ journal 里**一笔 = 一块像素**、**没有笔刷参数可供缩** ✓ ⇒ 这条路径只能把**每一块**
///   最近邻降采样 ✓ ⇒ **与"把整幅画缩一遍"不是同一件事** ✓。
///
/// **⇒ 判据**：带 `YANSHI_TIMELAPSE_SHRINK=1` 跑真二进制 ⇒ stderr 必须出现「**画会变**」✓。
/// **变异**：去掉那段警告 ⇒ 只剩「渲染量约降 N×」⇒ 红 ✓。
#[test]
fn the_shrink_path_says_the_picture_will_change() {
    let dir = std::env::temp_dir().join(format!("yanshi_shrink_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    // 画布要**大于** `MAX_W/MAX_H`（1920×1080）才会真的走降采样路径
    let meta = serde_json::json!({"width": 2560, "height": 2160, "doc_id": "d_shrink", "fps": 24});
    let atoms = [
        serde_json::json!({"id": "a0", "seq": 1, "kind": "create_document", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"doc_id": "d_shrink", "width": 2560, "height": 2160, "color_space": "srgb"}}),
        serde_json::json!({"id": "a1", "seq": 2, "kind": "create_layer", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "name": "L1", "type": "raster"}}),
    ];
    std::fs::write(dir.join("meta.json"), meta.to_string()).expect("meta");
    let text: String = atoms
        .iter()
        .map(|atom| format!("{atom}\n"))
        .collect::<Vec<_>>()
        .join("");
    std::fs::write(dir.join("atoms.jsonl"), text).expect("atoms");
    let pack = dir.join("shrink.yanshi");
    assert!(
        Command::new("tar")
            .arg("-cf")
            .arg(&pack)
            .arg("-C")
            .arg(&dir)
            .args(["meta.json", "atoms.jsonl"])
            .status()
            .expect("tar")
            .success(),
        "打不出包"
    );

    let out = dir.join("out.mp4");
    let result = Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
        .arg(&pack)
        .arg(&out)
        .arg("1")
        .env("YANSHI_TIMELAPSE_SHRINK", "1")
        .env("YANSHI_TIMELAPSE_RAW_DIR", dir.join("frames"))
        .output()
        .expect("跑 timelapse");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("画会变") || stderr.contains("不是同一幅"),
        "降采样回放必须说清「画会变」，实测 stderr：\n{stderr}"
    );
}

/// **`--max-dim` 必须真的改输出尺寸**（第 795 轮；外部 `qoder` D188 的建议）
///
/// **⇒ 为什么要有参数**：`MAX_W/MAX_H` 写死 1920×1080 ⇒ 8192 高的画布只能落 `0.13184` ✓
/// ⇒ 笔触肌理在过程视频里看不见 ✓（他们的场景是审片 ✓）。分辨率该由**用途**决定 ✓。
///
/// **⇒ 判据**：同一份 2560×2160 的包，默认跑 ⇒ banner 里输出 `1920×1080`-级 ✓；
/// 带 `--max-dim 512` 跑 ⇒ banner 里输出**不超过 512** ✓（且仍是偶数 ✓）。
/// **变异**：忽略该参数（用回常量）⇒ 第二条仍报大到 1000+ ⇒ 红 ✓。
#[test]
fn the_max_dim_flag_changes_the_output_size() {
    let dir = std::env::temp_dir().join(format!("yanshi_maxdim_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let meta = serde_json::json!({"width": 2560, "height": 2160, "doc_id": "d_dim", "fps": 24});
    let atoms = [
        serde_json::json!({"id": "a0", "seq": 1, "kind": "create_document", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"doc_id": "d_dim", "width": 2560, "height": 2160, "color_space": "srgb"}}),
        serde_json::json!({"id": "a1", "seq": 2, "kind": "create_layer", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "name": "L1", "type": "raster"}}),
    ];
    std::fs::write(dir.join("meta.json"), meta.to_string()).expect("meta");
    let text: String = atoms.iter().map(|atom| format!("{atom}\n")).collect();
    std::fs::write(dir.join("atoms.jsonl"), text).expect("atoms");
    let pack = dir.join("dim.yanshi");
    assert!(Command::new("tar")
        .arg("-cf")
        .arg(&pack)
        .arg("-C")
        .arg(&dir)
        .args(["meta.json", "atoms.jsonl"])
        .status()
        .expect("tar")
        .success());

    let run = |extra: &[&str], tag: &str| -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
            .arg(&pack)
            .arg(dir.join(format!("out-{tag}.mp4")))
            .arg("1")
            .args(extra)
            .env(
                "YANSHI_TIMELAPSE_RAW_DIR",
                dir.join(format!("frames-{tag}")),
            )
            .output()
            .expect("跑 timelapse");
        format!(
            "{}\n{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
            out.status
        )
    };

    let default_out = run(&[], "default");
    let small_out = run(&["--max-dim", "512"], "small");
    // **输出尺寸在 stdout 的 banner 里**（`画布 W×H ⇒ 输出 w×h`）
    let small_size = small_out
        .split("输出 ")
        .nth(1)
        .and_then(|rest| rest.split("×").next())
        .and_then(|w| w.trim().parse::<u32>().ok());
    let default_size = default_out
        .split("输出 ")
        .nth(1)
        .and_then(|rest| rest.split("×").next())
        .and_then(|w| w.trim().parse::<u32>().ok());
    // **默认按 1920×1080 收**：2560×2160 ⇒ 受**高**限制 ⇒ scale 0.5 ⇒ **1280×1080** ✓
    //   （我第一版写成 1920 是错的：`scale` 取两边的 **min** ✓，不是先按宽收 ✓。）
    assert_eq!(
        default_size,
        Some(1280),
        "默认应当受 1080 高限制（2560×2160 ⇒ scale 0.5 ⇒ 1280×1080）：\n{default_out}"
    );
    assert_eq!(
        small_size,
        Some(512),
        "`--max-dim 512` 应当把宽收到 512：\n{small_out}"
    );
}
