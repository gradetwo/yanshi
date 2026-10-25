//! **原子引用的 blob 不在包里 ⇒ 必须响亮记账 ＋ 拒不出片，不许"跑完退出码 0、画面全空"**（第 780／1130 轮）
//!
//! **⇒ 外部（`qoder` 的 D172）实测**：`yanshi-timelapse` 吃一个"原子引用的 blob 不在包里"的包
//! ⇒ **`rc=0`、一笔不落、一句警告都没有** ⇒ 产出一条**看起来正常的空影片**
//! （14,950 个原子跑完、退出码 0 ✓，而唯一那条全局汇总「⚠️ N 个 atom 没被内核接受」**从未出现** ✓）。
//!
//! **⇒ 为什么它难被发现**：失败在**数据缺失**这一侧 ✓，不在"应用原子"这一侧 ✓
//! ⇒ 任何只看 apply 失败数的汇总都报"一切正常" ✓ ⇒ **静默降级** ✓。
//!
//! **⇒ 判据**：造一个包——`atoms.jsonl` 里有 `import_image` 原子引用某个 64 位哈希，
//! 而包里**没有**那个 blob 文件 ⇒ 跑二进制 ⇒ **stderr 必须提到「缺」＋「blob」** ✓
//! ＋ **退出码必须是 2**（拒不出片 ✓，第 1130 轮补上：只喊不改退出码 = 调用方照样拿到空影片 ✗）。
//! **⇒ 变异**：把那段审计删掉 ⇒ stderr 里没有任何提示 ⇒ 红 ✓；去掉 `exit(2)` ⇒ 退出码断言红 ✓。
//!
//! **⇒ 为什么跑二进制而不是库**：包读取与警告都在 `main.rs`（可执行文件）里 ✓
//! ⇒ 用 `CARGO_BIN_EXE_yanshi-timelapse` ✓；并且用 `YANSHI_TIMELAPSE_RAW_DIR`
//! ⇒ **不需要 ffmpeg** ✓（落盘 RGB 帧 ✓）。
use std::process::Command;
/// **测试结束即删掉临时目录** ✓（第 814 轮；`/tmp` 是 1.9 GB 的 tmpfs ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

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
    let _cleanup = Cleanup(dir.clone());
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
    // **⇒ 而且必须拒不出片（rc=2）** ✓（第 1130 轮；D172 的完整闭环）：
    //   ⇒ 只"喊一句"但仍 `rc=0` ⇒ 调用方脚本会把那条空影片当真 ✓（＝静默降级 ✓）
    //   ⇒ **变异**：去掉 `std::process::exit(2)` ⇒ 这里红 ✓
    //   **⇒ 为什么 2 不是 1**：1 是"渲染/编码失败"✓；2 是"输入前提不成立"（包不完整 ✓）
    assert_eq!(
        result.status.code(),
        Some(2),
        "缺 blob 的包必须拒不出片（退出码 2），实测 {:?}：\n{combined}",
        result.status.code()
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
    let _cleanup = Cleanup(dir.clone());
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
    // **⇒ 而且必须给**量级**（D188 追记；第 1131 轮）**：小包只差半级、交付尺度差十几级 ✓
    //   ⇒ 只说"会变"不够 ✓（用户会以为只是细节糊一点 ✗）
    //   **变异**：把警告里「随原子数增长」与两档数删掉 ⇒ 这条红 ✓
    assert!(
        stderr.contains("原子数") && stderr.contains("22,926"),
        "降采样警告必须给出**随原子数增长**的两档量级（D188），实测 stderr：\n{stderr}"
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
    let _cleanup = Cleanup(dir.clone());
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

/// **`shrink` 的时间不许算进 `apply`**（第 804 轮；外部 `art` 第 121 次 / `qoder` D188）
///
/// **⇒ 我自己的仪器缺陷**：`t_apply` 原来把 `shrink_atom`（每块位图最近邻降采样 ✓）也算进去 ✗
/// ⇒ `SHRINK=1` 下的「apply ms/atom」**混进降采样成本** ✓ ⇒ 他们那条 O(n²) 里至少一部分是这个 ✓。
///
/// **⇒ 判据**：分段账里必须**同时有 `apply` 与 `shrink`** ✓，且带位图的包在 `SHRINK=1` 下
/// **`shrink` > 0 ms** ✓。
/// **⚠ 上一轮我为什么没写成**：那段横幅在 **`if probe`** 里 ✓（`main.rs:628` ✓）
/// ⇒ `probe` 来自 **`YANSHI_TIMELAPSE_PROBE=1`** ✓ ⇒ 不设它**根本没有这一行** ✓
/// ⇒ 判据必红且原因是"没打印" ✗ —— **这一条就是那次教训的修复** ✓。
/// **⇒ 变异**：把 `shrink` 并回 `apply` ⇒ 第一条红 ✓。
#[test]
fn shrink_time_is_accounted_separately_from_apply() {
    let dir = std::env::temp_dir().join(format!("yanshi_shrink_acct_{}", std::process::id()));
    let _cleanup = Cleanup(dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let (w, h) = (2560u32, 2160u32);
    // **位图要够大** ✓：64×64 的降采样**不到 1 ms** ⇒ 打印成 `shrink 0` ✗（第 804 轮实测）
    //   ⇒ 1536×1536 ＝ 9.4 MB 裸 RGBA ⇒ 降采样 ≥ 几 ms ✓
    let (bw, bh) = (1536u32, 1536u32);
    let blob: Vec<u8> = (0..bw * bh)
        .flat_map(|index| {
            let value = (index % 251) as u8;
            [value, value / 2, 255 - value, 255]
        })
        .collect();
    let hash = yanshi_core::BlobHash::from_bytes(&blob);
    // **`as_str()` 带 `sha256:` 前缀** ✓ 而**包里的文件名是裸十六进制** ✓（第 852 轮查清）
    //   **⇒ 缺陷（我的夹具）**：我原来用 `as_str()` 去拼目录名与文件名 ✗
    //     ⇒ 文件名成了 `sha256:…` ✓ 而 `main.rs` 把「路径最后一段」当 key ✓
    //       ⇒ key = `sha256:…` 而查表时会**剥掉前缀** ✓ ⇒ **永远查不到** ✗
    //         ⇒ `shrink_atom` 对**每一行**都返回 `None` ⇒ `SHRINK=1` 空转 ✓
    //           （`shrink 放弃 1 个原子（原因 blob 查不到×1）` 就是这么来的 ✓）
    let hex = hash.hex().to_owned();
    let blob_dir = dir.join("blobs/sha256").join(&hex[..2]).join(&hex[2..4]);
    std::fs::create_dir_all(&blob_dir).expect("blob 目录");
    std::fs::write(blob_dir.join(&hex), &blob).expect("blob");
    let hash = hash.as_str().to_owned();
    std::fs::write(
        dir.join("meta.json"),
        serde_json::json!({"width": w, "height": h, "doc_id": "d_acct", "fps": 24}).to_string(),
    )
    .expect("meta");
    std::fs::write(dir.join("blobs.encoding"), "none").expect("encoding");
    let atoms = [
        serde_json::json!({"id": "a0", "seq": 1, "kind": "create_document", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"doc_id": "d_acct", "width": w, "height": h, "color_space": "srgb"}}),
        serde_json::json!({"id": "a1", "seq": 2, "kind": "create_layer", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "name": "L1", "type": "raster"}}),
        serde_json::json!({"id": "a2", "seq": 3, "kind": "import_image", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "object_id": "o1",
                        // **`shrink_atom` 要 `region` + `width`/`height`** ✓（第 852 轮）
                        //   ⇒ 这个夹具原来只有 `bbox` ✗ ⇒ `size_of("width")?` 直接 `None` ✓
                        //     ⇒ 分段账显示 `放弃 …（原因 无）` —— 因为那一处我还没分桶 ✓
                        "bbox": [0.0, 0.0, f64::from(bw), f64::from(bh)],
                        "region": {"x": 0.0, "y": 0.0, "w": f64::from(bw), "h": f64::from(bh)},
                        "width": bw, "height": bh,
                        "bitmap": {"blob_hash": hash, "mime_type": "image/x-yanshi-raw",
                                   "size": blob.len()}}}),
    ];
    let text: String = atoms.iter().map(|atom| format!("{atom}\n")).collect();
    std::fs::write(dir.join("atoms.jsonl"), text).expect("atoms");
    assert!(
        Command::new("tar")
            .arg("-cf")
            .arg(dir.join("acct.yanshi"))
            .arg("-C")
            .arg(&dir)
            .args(["meta.json", "atoms.jsonl", "blobs.encoding", "blobs"])
            .status()
            .expect("tar")
            .success(),
        "打不出包"
    );

    let out = Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
        .arg(dir.join("acct.yanshi"))
        .arg(dir.join("out.mp4"))
        .arg("1")
        .env("YANSHI_TIMELAPSE_SHRINK", "1")
        // **分段账只在 probe 下打印** ✓（这正是上一轮踩的坑 ✓）
        .env("YANSHI_TIMELAPSE_PROBE", "1")
        .env("YANSHI_TIMELAPSE_RAW_DIR", dir.join("frames"))
        .output()
        .expect("跑 timelapse");
    let stdout = String::from_utf8_lossy(&out.stdout);
    eprintln!(
        "SUMMARY={}",
        stdout
            .lines()
            .find(|l| l.contains("分段账"))
            .unwrap_or("（无）")
    );
    assert!(
        stdout.contains("shrink"),
        "分段账里必须**单独**列出 `shrink`（否则它会被算进 apply）：\n{stdout}"
    );
    // **黑块计数必须在分段账里** ✓（第 847 轮；`art` 第 127 次的「5 块纯黑矩形」）
    //   ⇒ 他们说末帧有 5 块纯黑，而"5 个位图缩成全零"是最吻合的解释 ✓
    //   ⇒ 这两个数让他们那个「5」能与我的计数**对账** ✓。
    // **`shrink_atom` 放弃的次数** ✓（第 850 轮）：先只**要求它被打印出来** ✓
    //   **⇒ 实测：这个"健康"夹具 `放弃 1 个原子`** ✗ ⇒ 说明 `shrink` 对**唯一那一行**都没生效 ✓
    //     ⇒ 那 `SHRINK=1` 就是**空转** ✓ —— 正是 `D188` 的形态 ✓
    //   **⇒ 所以这里先**不断言 0** ✓：断言 0 现在会红，而红的是**产品**不是判据 ✓
    //     ⇒ 下一步是把原因分桶（`shrink_skips` 按 `None` 的出处拆开 ✓）⇒ 那时才能断言"健康包 = 0" ✓
    // **原因分桶必须打印** ✓（第 851 轮）：`shrink 放弃 N 个原子（原因 …）`
    // **夹具修好之后：健康的包必须 `放弃 0`** ✓（第 852 轮）
    //   **⇒ 为什么这条现在能断言 0**：第 851 轮实测是「放弃 1（原因 blob 查不到）」✓
    //     ⇒ 根因是**我的夹具**把 blob 文件命名成 `sha256:…`（`as_str()` 带前缀 ✗）而包里是裸 hex ✓
    //     ⇒ 修好夹具之后，`shrink_atom` 应当**每一行都能缩** ✓
    assert!(
        stdout.contains("shrink 放弃 0 个原子（原因 无）"),
        "修好夹具之后，健康的包不许再有放弃的原子：\n{stdout}"
    );
    assert!(
        stdout.contains("（原因 "),
        "分段账里必须打印 `shrink 放弃 N 个原子`（它是「缩样到底生效没有」的唯一直接信号）：\n{stdout}"
    );
    assert!(
        stdout.contains("黑块") && stdout.contains("缩小前全零") && stdout.contains("缩小后全零"),
        "分段账里必须有**黑块**计数（缩小前全零／缩小后全零）：\n{stdout}"
    );
    // **不断言 `shrink > 0`** ✗（第 804 轮实测）：`shrink_atom` 对 `import_image` 只**改几何**
    //   （bbox 换成输出尺寸 ✓）⇒ 位图缩放留给渲染器 ✓ ⇒ 它**本来就可能是 0 毫秒** ✓
    //   （实测 1536² 的位图也是 `shrink 0` ✓，且 stderr 里**没有**「这一行没能缩小」✓）
    //   ⇒ 所以判据只断**两个字段分开存在** ✓ —— 那才是"它没被算进 apply"的证据 ✓。
    let has_apply = stdout.contains("apply ");
    let has_shrink = stdout.contains("shrink ");
    assert!(
        has_apply && has_shrink,
        "分段账必须把 `apply` 与 `shrink` **分开列**（否则降采样成本会被算进 apply）：\n{stdout}"
    );
}

/// **只缺 `tiles` 解码索引 ⇒ **不许**致命**（第 1130 轮；D172 闭环的必要条件）
///
/// **⇒ 为什么**：`tiles` 是**分块解码索引** ✓，渲染侧的契约是「索引缺失／块取不到 ⇒ **落回整幅**」✓
///   （`render.rs:4350` ✓）⇒ 所以缺它**不会**让画面缺内容 ✓，只会慢一点 ✓。
///   ⇒ 而本仓 `export_project` 的 `wanted` **只收 `refs.blobs`** ✓ ⇒ **自产包永远不带 tiles** ✓
///     ⇒ 若把它算成"缺 blob" ⇒ **拒绝自己的包** ✗
///       （第 1130 轮 `real_package_shrinks` 实测：6 个引用里 3 个是 tiles ⇒ `rc=2` ✗）。
///
/// **⇒ 判据（三条）**：① `rc=0` ✓ ② 「缺 blob」是 **0 个** ✓
///   ③ 「tiles 索引缺失」**仍要可见**（不许因为这个豁免而变成静默 ✗）✓
/// **⇒ 变异**：把 `tile_index_hashes` 的排除去掉 ⇒ ① 变 `rc=2` ⇒ 红 ✓。
#[test]
fn a_missing_tile_index_alone_is_not_fatal() {
    let dir = std::env::temp_dir().join(format!("yanshi_tile_only_{}", std::process::id()));
    let _cleanup = Cleanup(dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建目录");
    let (w, h) = (64u32, 64u32);
    let mut blob = vec![0u8; (w * h * 4) as usize];
    for px in blob.chunks_exact_mut(4) {
        px.copy_from_slice(&[10, 200, 30, 255]);
    }
    let hash = yanshi_core::BlobHash::from_bytes(&blob);
    let hex = hash.hex().to_owned();
    let blob_dir = dir.join("blobs/sha256").join(&hex[..2]).join(&hex[2..4]);
    std::fs::create_dir_all(&blob_dir).expect("blob 目录");
    std::fs::write(blob_dir.join(&hex), &blob).expect("blob");
    // **tiles 指向一个**不存在**的索引 blob** ✓（渲染侧应当退回整幅 ✓）
    let tile_hash = "b".repeat(64);
    std::fs::write(
        dir.join("meta.json"),
        serde_json::json!({"width": w, "height": h, "doc_id": "d_tiles", "fps": 24}).to_string(),
    )
    .expect("meta");
    std::fs::write(dir.join("blobs.encoding"), "none").expect("encoding");
    let atoms = [
        serde_json::json!({"id": "a0", "seq": 1, "kind": "create_document", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"doc_id": "d_tiles", "width": w, "height": h, "color_space": "srgb"}}),
        serde_json::json!({"id": "a1", "seq": 2, "kind": "create_layer", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "name": "L1", "type": "raster"}}),
        serde_json::json!({"id": "a2", "seq": 3, "kind": "import_image", "actor": "human:t",
            "session": "s", "timestamp": 0, "schema_version": 1,
            "payload": {"layer_id": "L1", "object_id": "o1",
                        "region": {"x": 0.0, "y": 0.0, "w": f64::from(w), "h": f64::from(h)},
                        "width": w, "height": h, "type": "raster_patch",
                        "tiles": format!("sha256:{tile_hash}"),
                        "bitmap": {"blob_hash": hash.as_str(), "mime_type": "image/x-yanshi-raw",
                                   "size": blob.len()}}}),
    ];
    let text: String = atoms.iter().map(|atom| format!("{atom}\n")).collect();
    std::fs::write(dir.join("atoms.jsonl"), text).expect("atoms");
    assert!(
        Command::new("tar")
            .arg("-cf")
            .arg(dir.join("tiles.yanshi"))
            .arg("-C")
            .arg(&dir)
            .args(["meta.json", "atoms.jsonl", "blobs.encoding", "blobs"])
            .status()
            .expect("tar")
            .success(),
        "打不出包"
    );
    let frames = dir.join("frames");
    let result = Command::new(env!("CARGO_BIN_EXE_yanshi-timelapse"))
        .arg(dir.join("tiles.yanshi"))
        .arg(dir.join("out.mp4"))
        .arg("1")
        .env("YANSHI_TIMELAPSE_RAW_DIR", &frames)
        .output()
        .expect("跑 timelapse");
    let stderr = String::from_utf8_lossy(&result.stderr);
    let stdout = String::from_utf8_lossy(&result.stdout);
    let combined = format!("{stdout}\n{stderr}");
    // ① **不许因为它缺就拒绝**（像素本体在 ⇒ 画面完整 ✓）
    assert!(
        result.status.success(),
        "只缺 tiles 索引的包必须能出片（实测 rc={:?}）：\n{combined}",
        result.status.code()
    );
    // ② **必需 blob 的缺口是 0** ✓
    assert!(
        stderr.contains("缺 blob 0 个"),
        "只缺 tiles 索引时，必需 blob 必须是 0 缺：\n{stderr}"
    );
    // ③ **但仍然要看见它** ✓（豁免 ≠ 静默）
    assert!(
        stderr.contains("tiles") && stderr.contains("1 个"),
        "tiles 索引缺失必须仍然可见（豁免不等于静默）：\n{stderr}"
    );
}
