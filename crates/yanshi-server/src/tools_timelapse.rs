//! **绘画过程视频导出**（`export_timelapse`）。
//!
//! **为什么需要它**：过程视频此前只有 Web 端有（`assets/timelapse-local.js`，
//! 浏览器里 WebCodecs 编码）；MCP 工具面没有出口 ⇒ agent 拿不到过程视频。
//! 本工具在服务端补上：atom 日志**单趟增量重放**抓帧 ⇒ rawvideo 直灌
//! **ffmpeg 子进程**（libx264）出 MP4。
//!
//! **快是硬指标，两条禁令**：
//! - **严禁每帧从零重放、严禁每帧整幅重渲** —— 用一份临时 [`Document`] 逐
//!   atom `commit`（折叠是增量的），状态只前进；抓帧走**脏区增量**：累积每条
//!   atom 的 `dirty_bbox`，到抓帧点只渲脏区、贴回持久帧缓冲（实测整幅重渲
//!   ~2.4 s/帧 ⇒ 千笔文档会到分钟级；脏区渲染与面积成正比，紧框约 55 ms）。
//!   脏区规划与产品自己的增量预览共用同一套 `plan_dirty` ⇒ 可信度同源。
//! - **严禁 PNG 帧序列落盘中转** —— 帧在内存里合成白底、转 rgb24 后直接写
//!   ffmpeg 的 stdin，零中间编码、零中间文件。
//!
//! **ffmpeg 缺失 = 直接报错**（所有者裁定）：编码开始前先探测一次；
//! 找不到就报带安装提醒的错误，不降级、不生成半成品。输出先写同目录
//! `.part` 临时文件，成功才改名到目标路径 ⇒ 失败时目标路径不会留下残片。
//!
//! **Job**：导出登记为源文档上的一个 job（kind = `export_timelapse`），
//! 每抓一帧更新一次进度（`get_job` 可读 `progress`），完成时把结果写进
//! job 的 `result`；失败走 job 的 `failed` 并带同一份错误。
//!
//! **选帧与 Web 端保持同一观感**：缺省每 10 个 atom 一帧、30 fps、
//! 最后一帧恒为终态；透明像素合成到白底（与 Web 端一致，否则编码后发黑）。

use super::*;

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

use crate::document::{Document, DocumentSettings};

/// 缺省每多少个 atom 抓一帧（与 Web 端 `DEFAULT_FRAME_EVERY` 同值）。
const DEFAULT_FRAME_EVERY: usize = 10;
/// 缺省帧率（与 Web 端 `DEFAULT_FPS` 同值）。
const DEFAULT_FPS: u32 = 30;
/// 缺省输出宽度上限（保持纵横比、从不放大）。
const DEFAULT_WIDTH: u32 = 2048;
/// 缺省 CRF（libx264；18~24 是「肉眼近无损 ~ 常规交付」的常用段）。
const DEFAULT_CRF: u32 = 20;
/// 帧数硬上限：超过时**加大步长**（过程仍完整，只是更粗），绝不截断过程。
const DEFAULT_MAX_FRAMES: usize = 1800;
/// 导出 job 的 TTL：大文档导出可能数分钟，给足一小时，别让 job 中途过期。
const TIMELAPSE_JOB_TTL_SECONDS: i64 = 3600;

/// **缺 ffmpeg 时的错误**（所有者裁定：报错 + 安装提醒，不降级）。
///
/// 文案是契约的一部分：必须明说缺的是 ffmpeg、并给出三平台的装法 ⇒
/// 调用方（人或 agent）照着做就能解，不必再猜。
fn missing_ffmpeg_error() -> YanshiError {
    YanshiError::new(
        ErrorCode::PreconditionFailed,
        ErrorContext::detail(
            "导出过程视频需要 ffmpeg（H.264 编码），但在 PATH 里找不到 ffmpeg ⇒ 先安装再重试：\
             Debian/Ubuntu：sudo apt install ffmpeg；macOS：brew install ffmpeg；\
             Windows：winget install Gyan.FFmpeg（或从 ffmpeg.org 下载并把 bin 目录加入 PATH）",
        ),
    )
}

/// **探测 ffmpeg 是否可用**（编码开始前调用一次）。
///
/// 只认「能不能启动」：`NotFound` ⇒ 上面的缺失错误；其它启动失败也如实报，
/// 不把环境问题伪装成别的东西。
fn probe_ffmpeg() -> Result<()> {
    probe_ffmpeg_program("ffmpeg")
}

/// [`probe_ffmpeg`] 的可测内核：程序名可换 ⇒ 单测用一个不存在的名字
/// 就能命中缺失分支，不必动进程的 PATH（测试并行跑，改环境变量会互相污染）。
fn probe_ffmpeg_program(program: &str) -> Result<()> {
    match Command::new(program)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(missing_ffmpeg_error()),
        Err(error) => Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("启动 ffmpeg 探测失败：{error}")),
        )),
    }
}

/// **算步长**：每多少个 atom 抓一帧。
///
/// 优先级：显式 `frame_every` ＞ `duration_sec` 反推（目标帧数 = fps × 时长）。
/// 之后还要过 `max_frames` 这一关：预计帧数超上限就把步长加大到刚好装得下
/// ⇒ 过程完整、只是更粗（与「截断」是两回事，最后一帧永远在计划里）。
fn compute_step(
    total_atoms: usize,
    frame_every: Option<u64>,
    duration_sec: Option<f64>,
    fps: u32,
    max_frames: usize,
) -> usize {
    let mut step = match frame_every {
        Some(every) => (every as usize).max(1),
        None => match duration_sec {
            Some(seconds) if seconds > 0.0 => {
                let target_frames = ((fps as f64 * seconds).round() as usize).max(1);
                total_atoms.div_ceil(target_frames).max(1)
            }
            _ => DEFAULT_FRAME_EVERY,
        },
    };
    let floor = total_atoms.div_ceil(max_frames.max(1));
    step = step.max(floor).max(1);
    step
}

/// **抓帧计划**：提交完第 k 个 atom 后抓帧的 k 列表（1-based、升序、去重）。
///
/// 与 Web 端同一规则：`k % step == 0` 或 k 是最后一个 atom。
/// 计划里恒含 `total_atoms` ⇒ 最后一帧是终态（哪怕步长把它跳过去）。
fn plan_capture_counts(total_atoms: usize, step: usize) -> Vec<usize> {
    let mut counts = Vec::new();
    if total_atoms == 0 {
        return counts;
    }
    let step = step.max(1);
    let mut k = step;
    while k < total_atoms {
        counts.push(k);
        k += step;
    }
    counts.push(total_atoms);
    counts
}

/// **输出尺寸**：按宽度上限等比缩小、从不放大，且两边向下取偶
/// （`yuv420p` 要求偶数边，奇数边 ffmpeg 会直接拒绝）。
fn output_size(doc_w: u32, doc_h: u32, width_cap: u32) -> (u32, u32) {
    if doc_w == 0 || doc_h == 0 {
        return (0, 0);
    }
    let scale = ((width_cap.max(2)) as f64 / doc_w as f64).min(1.0);
    let even_floor = |value: f64| -> u32 {
        let rounded = (value.round() as u32).max(2);
        rounded - rounded % 2
    };
    (
        even_floor(doc_w as f64 * scale).max(2),
        even_floor(doc_h as f64 * scale).max(2),
    )
}

/// **一帧像素：RGBA 合成到白底、转 rgb24**（与 Web 端同语义）。
///
/// 文档背景之外的透明像素若直接送 yuv 会变黑 ⇒ Web 端早已定调合白底，
/// 服务端跟齐，避免同一文档两端导出观感不同。
fn flatten_to_rgb24(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        let alpha = pixel[3] as u32;
        if alpha == 255 {
            out.extend_from_slice(&pixel[..3]);
        } else if alpha == 0 {
            out.extend_from_slice(&[255, 255, 255]);
        } else {
            for channel in &pixel[..3] {
                let value = (*channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255;
                out.push(value as u8);
            }
        }
    }
    out
}

/// **ffmpeg 的命令行**（单独成函数 ⇒ 单测能钉住编码参数，不必真跑编码）。
///
/// 输入是 stdin 的 rawvideo（rgb24、无声），输出 libx264 + `yuv420p`
/// （播放器兼容面最宽）；`+faststart` 把 moov 提前，方便边下边播。
fn ffmpeg_args(
    out_w: u32,
    out_h: u32,
    fps: u32,
    crf: u32,
    output: &std::path::Path,
) -> Vec<String> {
    vec![
        "-y".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pix_fmt".into(),
        "rgb24".into(),
        "-s".into(),
        format!("{out_w}x{out_h}"),
        "-r".into(),
        fps.to_string(),
        "-i".into(),
        "-".into(),
        "-an".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "veryfast".into(),
        "-crf".into(),
        crf.to_string(),
        "-pix_fmt".into(),
        "yuv420p".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.display().to_string(),
    ]
}

/// **ffmpeg 子进程**：帧从 stdin 灌入；stderr 必须有专线线程抽干
/// （ffmpeg 的进度日志很能写，主线程只写不读会把管道撑满 ⇒ 双向死锁）。
struct FfmpegSink {
    child: Child,
    stdin: ChildStdin,
    stderr_drain: Option<std::thread::JoinHandle<String>>,
}

impl FfmpegSink {
    fn spawn(args: &[String]) -> Result<Self> {
        let mut child = Command::new("ffmpeg")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    missing_ffmpeg_error()
                } else {
                    YanshiError::new(
                        ErrorCode::PreconditionFailed,
                        ErrorContext::detail(format!("启动 ffmpeg 失败：{error}")),
                    )
                }
            })?;
        let stdin = child.stdin.take().expect("stdin 已设为 piped");
        let mut stderr = child.stderr.take().expect("stderr 已设为 piped");
        let stderr_drain = std::thread::spawn(move || {
            use std::io::Read;
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            // 只留尾巴：报错时给人看的是最后几行，不是整本日志。
            const KEEP: usize = 4096;
            if text.len() > KEEP {
                text = text[text.len() - KEEP..].to_string();
            }
            text
        });
        Ok(Self {
            child,
            stdin,
            stderr_drain: Some(stderr_drain),
        })
    }

    fn write_frame(&mut self, rgb: &[u8]) -> Result<()> {
        self.stdin.write_all(rgb).map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("往 ffmpeg 写帧失败（编码器可能已退出）：{error}")),
            )
        })
    }

    /// 关 stdin、等退出；非零退出码 ⇒ 把 stderr 尾巴带进错误。
    fn finish(mut self) -> Result<()> {
        drop(self.stdin);
        let status = self.child.wait().map_err(|error| {
            YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!("等 ffmpeg 退出失败：{error}")),
            )
        })?;
        let stderr = self
            .stderr_drain
            .take()
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();
        if status.success() {
            Ok(())
        } else {
            Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "ffmpeg 编码失败（{status}）。stderr 尾部：{}",
                    stderr.trim()
                )),
            ))
        }
    }
}

/// 两个 bbox 的并集。
fn union_bbox(a: Bbox, b: Bbox) -> Bbox {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.w).max(b.x + b.w);
    let bottom = (a.y + a.h).max(b.y + b.h);
    Bbox::new(x, y, right - x, bottom - y)
}

/// 把一块区域像素贴进整幅帧缓冲（同尺寸 RGBA、逐行拷贝）。
fn paste_region(frame: &mut [u8], frame_w: u32, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
    let stride = frame_w as usize * 4;
    let row_bytes = w as usize * 4;
    for row in 0..h as usize {
        let dst = (y as usize + row) * stride + x as usize * 4;
        let src = row * row_bytes;
        if dst + row_bytes <= frame.len() && src + row_bytes <= rgba.len() {
            frame[dst..dst + row_bytes].copy_from_slice(&rgba[src..src + row_bytes]);
        }
    }
}

/// 一次重放的产出统计。
struct ReplayOutcome {
    /// 实际抓到的帧数。
    frames: usize,
    /// 重放时被临时内核拒收的 atom（照 Web 端口径：警告 + 跳过，不中断导出）。
    warnings: Vec<String>,
}

/// **单趟增量重放抓帧**：把源文档的 atom 依次提交进一份临时文档，
/// 到计划点就把当前画面送给 `sink`。
///
/// 画面维护在一块**持久帧缓冲**里：首帧整幅渲染；之后每帧只渲「上次抓帧
/// 以来所有 atom 的 `dirty_bbox` 并集」（外扩 1px 防抗锯齿接缝），贴回缓冲。
/// 任一 atom 没给 `dirty_bbox`（全幅变更，如建文档/建层）⇒ 该帧回退整幅。
/// 这与产品增量预览用的是同一套脏区规划 ⇒ 帧的正确性与预览同源。
///
/// `sink` 拿到的是**已合成白底、已缩放到输出尺寸**的 rgb24 字节；
/// 生产路径接 [`FfmpegSink`]，单测接计数假 sink ⇒ 帧管线逻辑与编码器解耦，
/// **测试不需要 ffmpeg**（所有者裁定：CI 不为这个功能装 ffmpeg）。
#[allow(clippy::too_many_arguments)]
fn replay_frames(
    atoms: &[Atom],
    store: std::sync::Arc<dyn yanshi_core::BlobStore>,
    settings: DocumentSettings,
    scratch_id: &str,
    capture_counts: &BTreeSet<usize>,
    out_w: u32,
    out_h: u32,
    on_progress: &mut dyn FnMut(usize, usize),
    sink: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<ReplayOutcome> {
    let mut scratch = Document::open(store, scratch_id, Vec::new(), settings)?;
    let mut outcome = ReplayOutcome {
        frames: 0,
        warnings: Vec::new(),
    };
    // 持久帧缓冲（文档分辨率 RGBA）+ 自上次抓帧以来的待处理脏区。
    let mut frame: Option<(u32, u32, Vec<u8>)> = None;
    let mut pending: Option<Bbox> = None;
    let mut pending_full = true;
    for (index, atom) in atoms.iter().enumerate() {
        let committed = index + 1;
        let actor = atom.actor.clone();
        match scratch.commit_as(atom.clone(), &actor, true, None) {
            Ok(result) => {
                if !result.duplicate {
                    match result.dirty_bbox {
                        Some([x, y, w, h]) => {
                            let dirty = Bbox::new(x, y, w, h);
                            pending = Some(match pending {
                                Some(previous) => union_bbox(previous, dirty),
                                None => dirty,
                            });
                        }
                        None => pending_full = true,
                    }
                }
            }
            Err(error) => {
                outcome.warnings.push(format!(
                    "第 {committed} 个 atom（{}）重放时被拒：{}",
                    atom.kind.as_str(),
                    error.context.detail.as_deref().unwrap_or("")
                ));
                continue;
            }
        }
        if !capture_counts.contains(&committed) {
            continue;
        }
        let state = scratch.state();
        let (doc_w, doc_h) = (state.width, state.height);
        if doc_w == 0 || doc_h == 0 {
            continue;
        }
        let size_changed = frame
            .as_ref()
            .map(|(w, h, _)| *w != doc_w || *h != doc_h)
            .unwrap_or(false);
        if pending_full || size_changed || frame.is_none() {
            let region = Bbox::new(0.0, 0.0, doc_w as f64, doc_h as f64);
            let (width, height, rgba) = scratch.render_region_raw(region)?;
            frame = Some((width, height, rgba));
        } else if let (Some(dirty), Some((frame_w, _, buffer))) = (pending, &mut frame) {
            // 脏区取整并外扩 1px，再钳进画布（区域渲染在整数坐标上才逐像素可信）。
            let x0 = ((dirty.x.floor() as i64) - 1).max(0);
            let y0 = ((dirty.y.floor() as i64) - 1).max(0);
            let x1 = ((dirty.x + dirty.w).ceil() as i64 + 1).min(doc_w as i64);
            let y1 = ((dirty.y + dirty.h).ceil() as i64 + 1).min(doc_h as i64);
            if x1 > x0 && y1 > y0 {
                let region = Bbox::new(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64);
                let (width, height, rgba) = scratch.render_region_raw(region)?;
                paste_region(buffer, *frame_w, x0 as u32, y0 as u32, width, height, &rgba);
            }
        }
        pending = None;
        pending_full = false;
        let (_, _, buffer) = frame.as_ref().expect("抓帧前帧缓冲必然已建");
        let scaled = if doc_w == out_w && doc_h == out_h {
            buffer.clone()
        } else {
            yanshi_core::resample::resample_rgba(
                buffer,
                doc_w,
                doc_h,
                out_w,
                out_h,
                yanshi_core::resample::ResampleFilter::Bilinear,
            )
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "帧缩放失败（{doc_w}×{doc_h} ⇒ {out_w}×{out_h}）"
                    )),
                )
            })?
        };
        sink(&flatten_to_rgb24(&scaled))?;
        outcome.frames += 1;
        on_progress(outcome.frames, capture_counts.len());
    }
    Ok(outcome)
}

/// **`export_timelapse`** —— 把绘画过程导出成 MP4（见模块头）。
pub(super) fn write_export_timelapse(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let started = std::time::Instant::now();
    let path = guarded_output_path(&require_str(args, "path")?)?;
    let doc_id = optional_str(args, "doc_id").unwrap_or_else(|| ctx.doc_id.clone());

    // 参数（越界值钳到可用范围，而不是让 ffmpeg 去报错）。
    let fps = optional_count(args, "fps")?
        .map(|value| value as u32)
        .unwrap_or(DEFAULT_FPS)
        .clamp(1, 60);
    let frame_every = optional_count(args, "frame_every")?;
    let duration_sec = args.get("duration_sec").and_then(Value::as_f64);
    let width_cap = optional_count(args, "width")?
        .map(|value| value as u32)
        .unwrap_or(DEFAULT_WIDTH)
        .max(16);
    let crf = optional_count(args, "crf")?
        .map(|value| value as u32)
        .unwrap_or(DEFAULT_CRF)
        .min(51);
    let max_frames = optional_count(args, "max_frames")?
        .map(|value| value as usize)
        .unwrap_or(DEFAULT_MAX_FRAMES)
        .max(1);

    // 源文档快照：atom 全量克隆 + store + settings（之后重放不再借工作区，
    // 进度更新要拿 `document_mut` 时就不会撞借用）。
    let (atoms, store, settings, source_w, source_h) = {
        let document = ctx.workspace.document(&doc_id).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!("文档 {doc_id} 未打开 ⇒ 无法导出过程视频")),
            )
        })?;
        let state = document.state();
        (
            document.log().atoms().to_vec(),
            ctx.workspace.store(),
            ctx.workspace.settings().clone(),
            state.width,
            state.height,
        )
    };
    // **空过程要说清**：只有 `create_document` 一个 atom ⇒ 没有任何过程可导。
    if atoms.len() <= 1 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "文档 {doc_id} 里还没有绘画过程（只有 {} 个 atom）⇒ 先画几笔再导出",
                atoms.len()
            )),
        ));
    }
    if source_w == 0 || source_h == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("文档 {doc_id} 尺寸不可用（{source_w}×{source_h}）")),
        ));
    }

    // **编码开始前探测 ffmpeg**（所有者裁定）：缺了在建 job、抓帧之前就响亮
    // 失败 ⇒ 不产文件、不留 job 残骸，也省下整趟重放的白工。放在文档校验之后
    // ⇒ 参数/文档错误永远优先于环境错误（无 ffmpeg 的机器上照样能测错误路径）。
    probe_ffmpeg()?;

    let total = atoms.len();
    let step = compute_step(total, frame_every, duration_sec, fps, max_frames);
    let capture: BTreeSet<usize> = plan_capture_counts(total, step).into_iter().collect();
    let (out_w, out_h) = output_size(source_w, source_h, width_cap);

    // 临时输出：与目标同目录、先写它，成功才改名（失败不留半成品）。
    let tmp_path = path.with_extension("part.mp4");

    // job 登记（源文档上）：开始 ⇒ running，每帧刷 progress，终局 complete/fail。
    let job_id = {
        let document = ctx.workspace.document_mut(&doc_id)?;
        let id = document.jobs_mut().submit_with_ttl(
            "export_timelapse",
            ctx.session.clone(),
            ctx.now,
            TIMELAPSE_JOB_TTL_SECONDS,
        );
        let _ = document.jobs_mut().begin(&id, ctx.now);
        id
    };
    let fail_job = |ctx: &mut ToolContext<'_>, error: &YanshiError| {
        if let Ok(document) = ctx.workspace.document_mut(&doc_id) {
            let _ = document.jobs_mut().fail(&job_id, error, ctx.now);
        }
    };

    let run = (|| -> Result<ReplayOutcome> {
        let mut sink = FfmpegSink::spawn(&ffmpeg_args(out_w, out_h, fps, crf, &tmp_path))?;
        let mut progress = |done: usize, planned: usize| {
            if let Ok(document) = ctx.workspace.document_mut(&doc_id) {
                let _ = document.jobs_mut().set_progress(
                    &job_id,
                    json!({
                        "phase": "render",
                        "frames_done": done,
                        "frames_total": planned,
                    }),
                    ctx.now,
                );
            }
        };
        let mut write_frame = |rgb: &[u8]| sink.write_frame(rgb);
        let outcome = replay_frames(
            &atoms,
            store,
            settings,
            &format!("{doc_id}·timelapse"),
            &capture,
            out_w,
            out_h,
            &mut progress,
            &mut write_frame,
        )?;
        sink.finish()?;
        Ok(outcome)
    })();

    let outcome = match run {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = std::fs::remove_file(&tmp_path);
            fail_job(ctx, &error);
            return Err(error);
        }
    };
    if outcome.frames == 0 {
        let error = YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "文档 {doc_id} 重放后一帧都没抓到（{} 个 atom）⇒ 没有可导出的过程",
                total
            )),
        );
        let _ = std::fs::remove_file(&tmp_path);
        fail_job(ctx, &error);
        return Err(error);
    }
    std::fs::rename(&tmp_path, &path).map_err(|error| {
        let wrapped = YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "视频已编码但改名到 {} 失败：{error}",
                path.display()
            )),
        );
        fail_job(ctx, &wrapped);
        wrapped
    })?;
    let bytes = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
    let result = json!({
        "path": path,
        "job_id": job_id,
        "job_status": "committed",
        "frames": outcome.frames,
        "atoms": total,
        "step": step,
        "fps": fps,
        "duration_sec": (outcome.frames as f64 / fps as f64 * 100.0).round() / 100.0,
        "width": out_w,
        "height": out_h,
        "source_width": source_w,
        "source_height": source_h,
        "bytes": bytes,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "warnings": outcome.warnings,
    });
    if let Ok(document) = ctx.workspace.document_mut(&doc_id) {
        let _ = document
            .jobs_mut()
            .complete_with_result(&job_id, result.clone(), ctx.now);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_atom(layer: &str, object: &str, x: f64, y: f64, w: f64, h: f64, green: u8) -> Atom {
        Atom::new(
            AtomKind::DrawShape,
            "human:1".to_owned(),
            "session:test".to_owned(),
            json!({
                "layer_id": layer,
                "object_id": object,
                "data": {
                    "geometry": {"kind": "rect", "bbox": {"x": x, "y": y, "w": w, "h": h}},
                    "color": {"r": 200, "g": green, "b": 40, "a": 255},
                },
            }),
        )
    }

    #[test]
    fn step_comes_from_frame_every_then_duration_then_default() {
        // 显式 frame_every 优先。
        assert_eq!(compute_step(1000, Some(10), None, 30, 1800), 10);
        assert_eq!(compute_step(1000, Some(10), Some(5.0), 30, 1800), 10);
        // duration 反推：1000 atom、目标 150 帧 ⇒ step = ceil(1000/150) = 7。
        assert_eq!(compute_step(1000, None, Some(5.0), 30, 1800), 7);
        // 都没给 ⇒ 缺省 10（与 Web 端一致）。
        assert_eq!(compute_step(1000, None, None, 30, 1800), 10);
        // frame_every 给 0 也不能除零 ⇒ 钳到 1。
        assert_eq!(compute_step(50, Some(0), None, 30, 1800), 1);
    }

    #[test]
    fn max_frames_enlarges_the_step_instead_of_truncating() {
        // 100 万 atom、每 10 一帧本该 10 万帧 ⇒ 步长必须涨到 ceil(1e6/1800)=556，
        // 而计划仍含最后一帧（过程完整、只是更粗）。
        let step = compute_step(1_000_000, Some(10), None, 30, 1800);
        assert_eq!(step, 556);
        let plan = plan_capture_counts(1_000_000, step);
        assert!(plan.len() <= 1800, "帧数应被压回上限内：{}", plan.len());
        assert_eq!(plan.last(), Some(&1_000_000), "最后一帧恒为终态");
    }

    #[test]
    fn capture_plan_always_ends_at_the_final_atom() {
        assert_eq!(plan_capture_counts(0, 10), Vec::<usize>::new());
        assert_eq!(plan_capture_counts(1, 10), vec![1]);
        assert_eq!(plan_capture_counts(20, 10), vec![10, 20]);
        // 步长不整除总数时，尾巴补上终态、且不重复。
        assert_eq!(plan_capture_counts(25, 10), vec![10, 20, 25]);
        assert_eq!(plan_capture_counts(105, 10).last(), Some(&105));
    }

    #[test]
    fn output_size_never_upscales_and_stays_even() {
        // 小于上限 ⇒ 原尺寸。
        assert_eq!(output_size(800, 600, 2048), (800, 600));
        // 超过上限 ⇒ 等比缩到宽 2048。
        assert_eq!(output_size(4096, 2048, 2048), (2048, 1024));
        // 奇数边向下取偶（yuv420p 的硬要求）。
        assert_eq!(output_size(801, 601, 2048), (800, 600));
        let (w, h) = output_size(3840, 2160, 1000);
        assert_eq!(w % 2, 0);
        assert_eq!(h % 2, 0);
        assert!(w <= 1000 && h <= 2160, "只缩不放：{w}×{h}");
        // 退化尺寸不产出 0。
        assert_eq!(output_size(0, 100, 2048), (0, 0));
    }

    #[test]
    fn flatten_composites_transparency_onto_white() {
        // 全透明 ⇒ 白；全不透明 ⇒ 原色；半透明红 ⇒ 粉。
        let rgba = vec![
            10, 20, 30, 0, // 透明
            10, 20, 30, 255, // 不透明
            255, 0, 0, 128, // 半透明
        ];
        let rgb = flatten_to_rgb24(&rgba);
        assert_eq!(rgb.len(), 9);
        assert_eq!(&rgb[0..3], &[255, 255, 255]);
        assert_eq!(&rgb[3..6], &[10, 20, 30]);
        assert_eq!(&rgb[6..9], &[255, 127, 127]);
    }

    #[test]
    fn ffmpeg_args_pin_the_encoder_contract() {
        let args = ffmpeg_args(640, 480, 30, 20, std::path::Path::new("out.part.mp4"));
        let joined = args.join(" ");
        assert!(joined.contains("-f rawvideo -pix_fmt rgb24 -s 640x480 -r 30 -i -"));
        assert!(joined.contains("-c:v libx264 -preset veryfast -crf 20"));
        assert!(joined.contains("-pix_fmt yuv420p"));
        assert!(args.last().map(String::as_str) == Some("out.part.mp4"));
    }

    #[test]
    fn missing_ffmpeg_error_names_the_tool_and_the_install_paths() {
        let error = probe_ffmpeg_program("ffmpeg-that-does-not-exist-yanshi").unwrap_err();
        let detail = error.context.detail.as_deref().unwrap_or("");
        assert!(detail.contains("ffmpeg"), "要明说缺什么：{detail}");
        assert!(
            detail.contains("apt install ffmpeg"),
            "要有 Debian 装法：{detail}"
        );
        assert!(
            detail.contains("brew install ffmpeg"),
            "要有 macOS 装法：{detail}"
        );
        assert!(detail.contains("winget"), "要有 Windows 装法：{detail}");
    }

    #[test]
    fn replay_with_a_fake_sink_produces_planned_frames_without_ffmpeg() {
        // 帧管线（重放 + 选帧 + 缩放 + 白底合成）用假 sink 验逻辑 ⇒
        // 这条测试**不依赖 ffmpeg**，CI 里照样跑。
        let store = std::sync::Arc::new(yanshi_core::MemoryBlobStore::new());
        let mut source = Document::create(
            store.clone(),
            crate::document::NewDocument::new("doc_tl", 64, 48),
            "human:1",
            "session:test",
            DocumentSettings::default(),
        )
        .unwrap();
        source
            .commit(Atom::new(
                AtomKind::CreateLayer,
                "human:1".to_owned(),
                "session:test".to_owned(),
                json!({"layer_id": "L1", "name": "图层1"}),
            ))
            .unwrap();
        for index in 0..9 {
            source
                .commit(rect_atom(
                    "L1",
                    &format!("s{index}"),
                    4.0 + index as f64 * 6.0,
                    8.0,
                    5.0,
                    24.0,
                    (30 + index * 20) as u8,
                ))
                .unwrap();
        }
        let atoms = source.log().atoms().to_vec();
        // atoms = create + layer + 9 shape = 11；step 5 ⇒ 抓帧点 {5, 10, 11}。
        let capture: BTreeSet<usize> = plan_capture_counts(atoms.len(), 5).into_iter().collect();
        assert_eq!(capture.len(), 3);
        let mut frames: Vec<Vec<u8>> = Vec::new();
        let mut progress: Vec<(usize, usize)> = Vec::new();
        let outcome = replay_frames(
            &atoms,
            store,
            DocumentSettings::default(),
            "doc_tl·timelapse-test",
            &capture,
            64,
            48,
            &mut |done, planned| progress.push((done, planned)),
            &mut |rgb| {
                frames.push(rgb.to_vec());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(outcome.frames, 3);
        assert!(
            outcome.warnings.is_empty(),
            "重放不应有拒收：{:?}",
            outcome.warnings
        );
        assert_eq!(progress, vec![(1, 3), (2, 3), (3, 3)]);
        for frame in &frames {
            assert_eq!(frame.len(), 64 * 48 * 3, "rgb24 帧大小 = 宽×高×3");
        }
        // 内容在动：第一帧与最后一帧不同（形状逐个出现）。
        assert_ne!(frames[0], frames[2], "过程帧应当互不相同");
        // 最后一帧含最后一个形状的绿色（g = 30 + 8*20 = 190）附近像素 ⇒
        // 至少有一个像素接近 (200, 190, 40)（白底合成后不透明色不变）。
        let last = frames.last().unwrap();
        let has_last_color = last
            .chunks_exact(3)
            .any(|pixel| pixel[0] > 180 && pixel[1] > 170 && pixel[2] < 80);
        assert!(has_last_color, "最后一帧应包含最后一个形状的颜色");
        // **脏区增量的根基断言**：贴出来的最后一帧必须与「整幅渲染终态」
        // 逐像素一致 —— 否则脏区规划/贴回有洞，过程视频会悄悄丢内容。
        let (full_w, full_h, full_rgba) = source
            .render_region_raw(Bbox::new(0.0, 0.0, 64.0, 48.0))
            .unwrap();
        assert_eq!((full_w, full_h), (64, 48));
        assert_eq!(
            &flatten_to_rgb24(&full_rgba),
            last,
            "脏区增量帧必须与整幅渲染逐像素一致"
        );
    }
}
