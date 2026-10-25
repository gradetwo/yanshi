//! **`export_timelapse`（绘画过程视频导出）的工具面测试**。
//!
//! **测试分工（所有者裁定：这个功能不在 GitHub CI 上测真编码）**：
//! - 帧管线逻辑（选帧/缩放/缺 ffmpeg 文案）由 `tools_timelapse` 的单元测试
//!   用假 sink 覆盖，**不依赖 ffmpeg**，哪里都能跑；
//! - 本文件的常驻用例只断言**两个世界都成立**的不变量（空过程报错、
//!   有 ffmpeg 就成功且有文件 / 没 ffmpeg 就报缺失错误且不留文件）；
//! - 真编码端到端（ffprobe 验流、抽帧对内容、千笔耗时）是**本地验证**：
//!   需 `YANSHI_TEST_TIMELAPSE_E2E=1` 才跑，缺 ffmpeg/ffprobe 时打印原因
//!   并 skip —— CI 不必装 ffmpeg、更不会因此变红。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace_with_doc(doc_id: &str, width: u32, height: u32) -> Workspace {
    // 资产目录指向仓库里的 assets/（brush_stroke 的 .myb 在那里，与 brush_stroke.rs 同一做法）。
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace =
        Workspace::in_memory(DocumentSettings::default()).with_assets_dir(Some(assets));
    workspace
        .create_document(
            NewDocument::new(doc_id, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(
    workspace: &mut Workspace,
    doc_id: &str,
    tool: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let mut ctx = ToolContext::new(workspace, doc_id, "human:1", "session:test");
    ctx.wait_for_render = false;
    ctx.silent = true;
    ctx.preview = false;
    registry().call(&mut ctx, tool, &args)
}

fn ffmpeg_available() -> bool {
    std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn temp_output(name: &str) -> std::path::PathBuf {
    let dir =
        std::env::temp_dir().join(format!("yanshi_timelapse_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("process.mp4")
}

/// 空文档（只有 create_document）⇒ 报「没有绘画过程」，且与 ffmpeg 在不在无关。
#[test]
fn an_empty_document_is_refused_with_a_clear_reason() {
    let mut workspace = workspace_with_doc("doc_empty", 320, 240);
    let out = temp_output("empty");
    let result = call(
        &mut workspace,
        "doc_empty",
        "export_timelapse",
        json!({"path": out.display().to_string()}),
    );
    assert_eq!(result["ok"], json!(false), "{result}");
    let text = result.to_string();
    assert!(
        text.contains("绘画过程"),
        "报错要说清是没有过程可导：{result}"
    );
    assert!(!out.exists(), "失败不许留文件");
}

/// **双世界不变量**：有 ffmpeg ⇒ 成功 + 非空文件 + job 已 committed；
/// 没 ffmpeg ⇒ 明确的缺失错误（含安装提醒）+ 不留任何文件（含 .part）。
#[test]
fn export_either_succeeds_fully_or_fails_without_a_trace() {
    let mut workspace = workspace_with_doc("doc_dual", 320, 240);
    assert_eq!(
        call(
            &mut workspace,
            "doc_dual",
            "create_layer",
            json!({"layer_id": "L1"})
        )["ok"],
        json!(true)
    );
    for index in 0..6 {
        let made = call(
            &mut workspace,
            "doc_dual",
            "draw_shape",
            json!({
                "layer_id": "L1",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 10 + index * 40, "y": 60, "w": 30, "h": 100}},
                    "color": {"r": 220, "g": 60 + index * 20, "b": 50, "a": 255},
                },
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let out = temp_output("dual");
    let result = call(
        &mut workspace,
        "doc_dual",
        "export_timelapse",
        json!({"path": out.display().to_string(), "frame_every": 2}),
    );
    if ffmpeg_available() {
        assert_eq!(result["ok"], json!(true), "{result}");
        // atoms = create + layer + 6 shape = 8；step 2 ⇒ 帧点 {2,4,6,8} = 4 帧。
        assert_eq!(result["frames"], json!(4), "{result}");
        assert_eq!(result["job_status"], json!("committed"), "{result}");
        let bytes = std::fs::metadata(&out).unwrap().len();
        assert!(bytes > 0, "MP4 不应为空");
        assert_eq!(result["bytes"], json!(bytes), "{result}");
        // job 里能读到同一份结果。
        let job = call(
            &mut workspace,
            "doc_dual",
            "get_job",
            json!({"job_id": result["job_id"]}),
        );
        assert_eq!(job["status"], json!("committed"), "{job}");
        assert_eq!(job["result"]["frames"], json!(4), "{job}");
    } else {
        assert_eq!(result["ok"], json!(false), "{result}");
        let text = result.to_string();
        assert!(text.contains("ffmpeg"), "要明说缺 ffmpeg：{result}");
        assert!(
            text.contains("apt install ffmpeg"),
            "要带安装提醒：{result}"
        );
        assert!(!out.exists(), "缺 ffmpeg 不许留文件");
        assert!(
            !out.with_extension("part.mp4").exists(),
            "缺 ffmpeg 不许留 .part 半成品"
        );
    }
}

/// 真编码端到端是否被允许跑（本地验证开关 + 工具齐全）。
fn e2e_gate() -> bool {
    if std::env::var_os("YANSHI_TEST_TIMELAPSE_E2E").is_none() {
        eprintln!(
            "skip: 真编码端到端是本地验证（所有者裁定 CI 不覆盖）⇒ 设 YANSHI_TEST_TIMELAPSE_E2E=1 才跑"
        );
        return false;
    }
    if !ffmpeg_available() {
        eprintln!("skip: 本机没有 ffmpeg ⇒ 真编码端到端跳过（缺失路径已由常驻用例覆盖）");
        return false;
    }
    true
}

fn ffprobe_json(path: &std::path::Path) -> serde_json::Value {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=codec_name,width,height,nb_read_frames,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .expect("ffprobe 应可运行");
    assert!(
        output.status.success(),
        "ffprobe 失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// 抽出第 n 帧为 PNG 并读回 RGB 均值（与 export_png 的像素做近似比对）。
fn frame_mean_rgb(path: &std::path::Path, frame: usize, dir: &std::path::Path) -> (f64, f64, f64) {
    let png_path = dir.join(format!("frame{frame}.png"));
    let status = std::process::Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-i"])
        .arg(path)
        .args(["-vf", &format!("select='eq(n\\,{frame})'"), "-vframes", "1"])
        .arg(&png_path)
        .status()
        .expect("ffmpeg 抽帧应可运行");
    assert!(status.success(), "抽帧失败");
    let bytes = std::fs::read(&png_path).unwrap();
    let (width, height, rgba) =
        yanshi_render::png::decode_png(&bytes).expect("抽出的帧应是合法 PNG");
    let _ = (width, height);
    let pixels = rgba.len() / 4;
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for pixel in rgba.chunks_exact(4) {
        r += pixel[0] as u64;
        g += pixel[1] as u64;
        b += pixel[2] as u64;
    }
    (
        r as f64 / pixels as f64,
        g as f64 / pixels as f64,
        b as f64 / pixels as f64,
    )
}

fn png_mean_rgb(bytes: &[u8]) -> (f64, f64, f64) {
    let (_, _, rgba) = yanshi_render::png::decode_png(bytes).expect("export_png 应是合法 PNG");
    let pixels = rgba.len() / 4;
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for pixel in rgba.chunks_exact(4) {
        r += pixel[0] as u64;
        g += pixel[1] as u64;
        b += pixel[2] as u64;
    }
    (
        r as f64 / pixels as f64,
        g as f64 / pixels as f64,
        b as f64 / pixels as f64,
    )
}

/// **真编码端到端（本地验证）**：混合 brush_stroke / medium_stroke / draw_shape
/// 画两段，ffprobe 验流参数，抽帧与两个阶段的 export_png 分别对内容。
#[test]
fn e2e_real_encode_matches_probe_and_stage_renders() {
    if !e2e_gate() {
        return;
    }
    let doc = "doc_e2e";
    let mut workspace = workspace_with_doc(doc, 480, 360);
    assert_eq!(
        call(
            &mut workspace,
            doc,
            "create_layer",
            json!({"layer_id": "L1"})
        )["ok"],
        json!(true)
    );

    let draw_phase = |workspace: &mut Workspace, offset: f64, green: u8| {
        // 3 个形状 + 1 笔 brush + 1 笔 medium = 5 个 atom。
        for index in 0..3 {
            let made = call(
                workspace,
                doc,
                "draw_shape",
                json!({
                    "layer_id": "L1",
                    "data": {
                        "geometry": {"kind": "rect", "bbox": {"x": offset + index as f64 * 90.0, "y": 40.0, "w": 60.0, "h": 120.0}},
                        "color": {"r": 210, "g": green, "b": 60, "a": 255},
                    },
                }),
            );
            assert_eq!(made["ok"], json!(true), "{made}");
        }
        let brush = call(
            workspace,
            doc,
            "brush_stroke",
            json!({
                "layer_id": "L1", "brush": "2B_pencil", "size": 18,
                "points": [[offset + 10.0, 260.0, 0.7], [offset + 120.0, 300.0, 0.9], [offset + 230.0, 250.0, 0.6]],
            }),
        );
        assert_eq!(brush["ok"], json!(true), "{brush}");
        let medium = call(
            workspace,
            doc,
            "medium_stroke",
            json!({
                "layer_id": "L1", "medium": "oil", "size": 28,
                "color": {"r": 40, "g": 90, "b": 200, "a": 255},
                "points": [[offset + 20.0, 200.0, 0.8], [offset + 140.0, 210.0, 0.8], [offset + 240.0, 190.0, 0.7]],
            }),
        );
        assert_eq!(medium["ok"], json!(true), "{medium}");
    };

    draw_phase(&mut workspace, 10.0, 80);
    // 阶段一结束：atoms = create(1) + layer(1) + 5 = 7。先存一张阶段图。
    let dir = std::env::temp_dir().join(format!("yanshi_timelapse_e2e_{}", std::process::id()));
    let _cleanup_1 = Cleanup(dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stage_png = dir.join("stage.png");
    let exported = call(
        &mut workspace,
        doc,
        "export_png",
        json!({"path": stage_png.display().to_string()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");

    draw_phase(&mut workspace, 130.0, 180);
    let final_png = dir.join("final.png");
    let exported = call(
        &mut workspace,
        doc,
        "export_png",
        json!({"path": final_png.display().to_string()}),
    );
    assert_eq!(exported["ok"], json!(true), "{exported}");

    // atoms 共 12；frame_every=7 ⇒ 抓帧点 {7, 12}：第 1 帧 = 阶段一终局，第 2 帧 = 全终局。
    let video = dir.join("process.mp4");
    let result = call(
        &mut workspace,
        doc,
        "export_timelapse",
        json!({"path": video.display().to_string(), "frame_every": 7, "fps": 30, "width": 480}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["frames"], json!(2), "{result}");
    assert_eq!(result["atoms"], json!(12), "{result}");

    // 同一文档再导一次：MP4 字节不必一致（编码器时间戳），但帧数/时长必须一致。
    let video2 = dir.join("process2.mp4");
    let again = call(
        &mut workspace,
        doc,
        "export_timelapse",
        json!({"path": video2.display().to_string(), "frame_every": 7, "fps": 30, "width": 480}),
    );
    assert_eq!(again["ok"], json!(true), "{again}");
    assert_eq!(again["frames"], result["frames"], "两次导出的帧数必须一致");
    assert_eq!(
        again["duration_sec"], result["duration_sec"],
        "两次导出的时长必须一致"
    );

    let probe = ffprobe_json(&video);
    let stream = &probe["streams"][0];
    assert_eq!(stream["codec_name"], json!("h264"), "{probe}");
    assert_eq!(stream["width"], json!(480), "{probe}");
    assert_eq!(stream["height"], json!(360), "{probe}");
    assert_eq!(stream["nb_read_frames"], json!("2"), "{probe}");
    let duration = stream["duration"].as_str().unwrap().parse::<f64>().unwrap();
    assert!(
        (duration - 2.0 / 30.0).abs() < 0.05,
        "时长应 ≈ 2 帧 / 30fps：{duration}"
    );

    // 内容对位：视频帧均值 ≈ 对应阶段 PNG 均值（H.264 有损 ⇒ 用宽松界）。
    let stage_mean = png_mean_rgb(&std::fs::read(&stage_png).unwrap());
    let final_mean = png_mean_rgb(&std::fs::read(&final_png).unwrap());
    let frame0 = frame_mean_rgb(&video, 0, &dir);
    let frame1 = frame_mean_rgb(&video, 1, &dir);
    let close = |a: (f64, f64, f64), b: (f64, f64, f64)| {
        (a.0 - b.0).abs() < 6.0 && (a.1 - b.1).abs() < 6.0 && (a.2 - b.2).abs() < 6.0
    };
    assert!(
        close(frame0, stage_mean),
        "第 1 帧应对应阶段一：帧 {frame0:?} vs PNG {stage_mean:?}"
    );
    assert!(
        close(frame1, final_mean),
        "最后帧应对应终局：帧 {frame1:?} vs PNG {final_mean:?}"
    );
    assert!(!close(frame0, frame1), "两个阶段的内容应当不同");
}

/// **千笔级耗时（本地验证）**：1100 个 atom 的文档导出，打印分段耗时供记录，
/// 只断言「帧数与计划一致、文件可被 ffprobe 读」——耗时数字不做硬断言
/// （机器差异太大，写进 README 由人读）。
#[test]
fn e2e_thousand_atom_document_timing() {
    if !e2e_gate() {
        return;
    }
    let doc = "doc_perf";
    let mut workspace = workspace_with_doc(doc, 1024, 768);
    assert_eq!(
        call(
            &mut workspace,
            doc,
            "create_layer",
            json!({"layer_id": "L1"})
        )["ok"],
        json!(true)
    );
    let draw_started = std::time::Instant::now();
    for index in 0..1000 {
        let x = (index % 25) as f64 * 40.0;
        let y = (index / 25) as f64 * 18.0;
        let made = call(
            &mut workspace,
            doc,
            "draw_shape",
            json!({
                "layer_id": "L1",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": 30.0, "h": 12.0}},
                    "color": {"r": 120, "g": (index % 255) as u8, "b": 90, "a": 255},
                },
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    for index in 0..100 {
        let made = call(
            &mut workspace,
            doc,
            "brush_stroke",
            json!({
                "layer_id": "L1", "brush": "2B_pencil", "size": 10,
                "points": [[20.0 + index as f64 * 9.0, 740.0, 0.6], [60.0 + index as f64 * 9.0, 720.0, 0.8]],
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    eprintln!(
        "perf: 画 1100 个 atom 用时 {:?}（ setup，不计入导出）",
        draw_started.elapsed()
    );

    let dir = std::env::temp_dir().join(format!("yanshi_timelapse_perf_{}", std::process::id()));
    let _cleanup_2 = Cleanup(dir.clone());
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let video = dir.join("process.mp4");
    let started = std::time::Instant::now();
    let result = call(
        &mut workspace,
        doc,
        "export_timelapse",
        json!({"path": video.display().to_string(), "frame_every": 10, "fps": 30, "width": 1024}),
    );
    let elapsed = started.elapsed();
    assert_eq!(result["ok"], json!(true), "{result}");
    // atoms = 1 + 1 + 1100 = 1102；step 10 ⇒ floor(1102/10)=110 个整点 + 终局 = 111 帧。
    assert_eq!(result["frames"], json!(111), "{result}");
    assert_eq!(result["elapsed_ms"], result["elapsed_ms"], "{result}");
    eprintln!(
        "perf: 1102 atom / 111 帧 / 1024×768 导出总耗时 {:?}（工具自报 {} ms）",
        elapsed, result["elapsed_ms"]
    );
    let probe = ffprobe_json(&video);
    assert_eq!(
        probe["streams"][0]["nb_read_frames"],
        json!("111"),
        "{probe}"
    );
}

/// **E3：`export_timelapse` 回执回传分段账（apply／render／shrink 三段 ms）**。
///
/// **四层齐**：实现（`ReplayOutcome` 三段 + 回执三字段）／输入能触发
/// （有 ffmpeg 时 6 形状真编码一趟）／断言（三字段是 u64、apply＋render
/// ≤ 总 `elapsed_ms`、shrink 恒为 0）／变异（回执里删任一字段 ⇒ `is_u64`
/// 红；计时改纳秒上报 ⇒ 「≤ elapsed」红）。
/// 没 ffmpeg 时跳过（缺失路径已由常驻用例覆盖）；小工作量下 apply／render
/// 可能为 0 ms ⇒ 这里不断 `> 0`（强断言在 `tools_timelapse` 的单元测试里）。
#[test]
fn export_timelapse_receipt_carries_segment_ms() {
    if !ffmpeg_available() {
        eprintln!("skip: 没有 ffmpeg ⇒ 回执分段账不可判（缺失路径已由常驻用例覆盖）");
        return;
    }
    let mut workspace = workspace_with_doc("doc_seg", 320, 240);
    assert_eq!(
        call(
            &mut workspace,
            "doc_seg",
            "create_layer",
            json!({"layer_id": "L1"})
        )["ok"],
        json!(true)
    );
    for index in 0..6 {
        let made = call(
            &mut workspace,
            "doc_seg",
            "draw_shape",
            json!({
                "layer_id": "L1",
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": 10 + index * 40, "y": 60, "w": 30, "h": 100}},
                    "color": {"r": 220, "g": 60 + index * 20, "b": 50, "a": 255},
                },
            }),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let out = temp_output("seg");
    let result = call(
        &mut workspace,
        "doc_seg",
        "export_timelapse",
        json!({"path": out.display().to_string(), "frame_every": 2}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    // atoms = create + layer + 6 shape = 8；step 2 ⇒ 帧点 {2,4,6,8} = 4 帧。
    assert_eq!(result["frames"], json!(4), "{result}");
    let elapsed = result["elapsed_ms"]
        .as_u64()
        .unwrap_or_else(|| panic!("回执要有 elapsed_ms（u64）：{result}"));
    for field in ["apply_ms", "render_ms", "shrink_ms"] {
        assert!(
            result[field].is_u64(),
            "回执必须有 `{field}`（u64 ms）：{result}"
        );
    }
    let apply = result["apply_ms"].as_u64().unwrap();
    let render = result["render_ms"].as_u64().unwrap();
    assert!(
        apply + render <= elapsed,
        "分段账是总耗时的一部分（apply {apply} + render {render} ≤ elapsed {elapsed}）：{result}"
    );
    assert_eq!(
        result["shrink_ms"],
        json!(0),
        "MCP 路无原子级 shrink ⇒ shrink_ms 恒为 0：{result}"
    );
}
