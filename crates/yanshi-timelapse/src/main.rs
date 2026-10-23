//! yanshi-timelapse: 将 `.yanshi` 工程导出为**绘画过程视频**（timelapse）。
//!
//! 用法：
//! ```text
//! yanshi-timelapse <输入.yanshi> [输出.mp4] [每N个atom一帧=10] [起始atom=0] [结束atom=全部]
//! ```
//!
//! **★★★ 第 249 轮合入时改了设计 ✗：**复用仓库自己的东西 ✗ 不另写一套** ✓ ★★★**
//!
//! **∴ 为什么 ✗**（**交付包那版跑本仓夹具**直接失败** ✓）**：
//!   ＋ **∴ 交付版自己解 `zip→tar`**✗ 而**本仓导出的是**裸 tar**（`file` 的结果就是
//!     `POSIX tar archive` ✓）⇒ **∴ 实测** `Error: invalid Zip archive`** ✓；
//!   ＋ **∴ 交付版把 blob 当**裸 RGBA**✗ 而**本仓 `blobs.encoding = zlib`** ⇒ **∴ 带图的工程**
//!     **出乱码** ✓（**∴ 不是"画得不对"✗ 是**根本不是像素** ✓）；
//!   ＋ **∴ 交付版只认 `import_image`**✗ 而**本仓的手绘笔触是 `draw_stroke`** ⇒ **∴ 一笔都不会
//!     出现** ✓（**∴ JS 那版认 `brush_stroke` ✗ 而**本仓不叫这个名字** ✓）。
//!
//! **∴ 现在的分工（**每一件都是仓库里**已有且被测试过**的 ✓）★**：
//!   * **∴ 读包 ✗**：`yanshi_server::archive::read_tar`（**本仓**零依赖**的手写 tar 读取器 ✓）
//!     ⇒ **∴ 于是**`zip`／`tar` **两个依赖**整个去掉了** ✓ ★★
//!   * **∴ 解 blob ✗**：`yanshi_render::zlib_decompress`（**`blobs.encoding = zlib` 时 ✓**
//!     ＋ **∴ 不是 zlib ⇒ **原样用**✗ 两种编码都兼容 ✓）★
//!   * **∴ replay ＋ 渲染 ✗**：`yanshi_wasm::Kernel`（`apply_atom_json` ＋ `render_region` ✓）
//!     ⇒ **∴ 与浏览器**同一个引擎**✗ ⇒ **∴ 两端**不会画出不同东西** ✓ ★★
//!     ＋ **∴ 而**它**认全部 atom 种类**（`draw_stroke`／`draw_shape`／`draw_text`／
//!     `erase`／… ✓）⇒ **∴ 手绘笔触**终于会出现在视频里** ✓ ★★★
//!   * **∴ 编码 ✗**：外部 `ffmpeg`（**管道 ✗ 不落中间 PNG ✓）—— **∴ 这是运行期外部依赖 ✗
//!     必须让人知道**（**∴ spawn 失败时**会给一句人话 ✓）★
//!
//! **∴ 参数与交付说明对齐 ✗**：只有输入必填 ✓ 其余都有默认 ✓
//!   ＋ **∴ 尺寸从 `meta.json` 读 ✗**（**不再写死 1920×1080 ✓）⇒ **∴ 大图**按比例缩到
//!     **≤1920×1080**（**只缩不放 ✓ 且**用盒平均 ✗ 比最近邻干净 ✓）★

use std::collections::HashMap;
use std::io::Write;
use std::process::{ChildStdin, Command, Stdio};
use std::time::Instant;

// **∴ 坑（**合入时踩到的 ✓）✗**：**`KernelError` **没有 `Display`**✗（**它只给
//   `code`／`detail` 两个**pub 字段** ＋ `to_json()` ✓）⇒ **∴ `{e}` 编不过** ✓
//     ＋ **∴ 于是**：**错误信息必须**显式拼出来**✗ ⇒ **∴ 这样也**更诚实**（**码 ＋ 说明都留着 ✓）★
use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};

/// 输出尺寸上限（**只缩不放 ✓**）。
const MAX_W: u32 = 1920;
/// 输出尺寸上限（**只缩不放 ✓**）。
const MAX_H: u32 = 1080;
/// 内核内存预算（**与浏览器同档 ✓**）：4K 工程 replay 需要它 ✓。
const KERNEL_MEMORY_LIMIT: usize = 2 * 1024 * 1024 * 1024;
/// 内核算术的 tile 尺寸（**与浏览器同一档 ✓**）。
const TILE_SIZE: u32 = 64;

fn main() -> Result<()> {
    // **读包计时起点** ✓（第 612 轮）：外部要"阶段计时"，而"读包"是**最容易被误判成渲染**的一段 ✓
    let t_read = std::time::Instant::now();
    let args: Vec<String> = std::env::args().collect();
    if !(2..=6).contains(&args.len()) {
        eprintln!(
            "用法: {} <输入.yanshi> [输出.mp4] [每N个atom一帧=10] [起始atom=0] [结束atom=全部]",
            args[0]
        );
        eprintln!("示例: {} in.yanshi", args[0]);
        eprintln!("      {} in.yanshi out.mp4 5 100 500", args[0]);
        std::process::exit(2);
    }
    let input = &args[1];
    let default_out = {
        let stem = std::path::Path::new(input)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let base = stem.strip_suffix(".yanshi").unwrap_or(&stem).to_owned();
        format!("{base}-timelapse.mp4")
    };
    let out_mp4 = match args.get(2) {
        Some(s) if !s.is_empty() => s.clone(),
        _ => default_out,
    };
    let frame_every = args
        .get(3)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(10)
        .max(1);
    let start_atom = args
        .get(4)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let end_atom = args.get(5).and_then(|s| s.parse::<usize>().ok());

    println!("输入: {input}");
    println!("输出: {out_mp4}");
    println!(
        "每 {frame_every} 个 atom 一帧｜范围 {start_atom}..{}",
        end_atom.map_or("结束".to_owned(), |e| e.to_string())
    );

    let t0 = Instant::now();
    let bytes = std::fs::read(input).with_context(|| format!("读不到输入文件 {input}"))?;
    let entries = yanshi_server::archive::read_tar(&bytes)
        .map_err(|e| anyhow!("读包失败（本仓的 .yanshi 是**裸 tar**）：{e}"))?;

    // **∴ 把包里我们认识的几样挑出来 ✗**（**其余**原样忽略 ✗ 但**报个数** ✓）
    let mut atoms_text = String::new();
    let mut meta_text = String::new();
    let mut encoding = String::from("none");
    let mut blobs: HashMap<String, Vec<u8>> = HashMap::new();
    let mut skipped_entries = 0usize;
    for entry in &entries {
        match entry.path.as_str() {
            "atoms.jsonl" => atoms_text = String::from_utf8_lossy(&entry.bytes).into_owned(),
            "meta.json" => meta_text = String::from_utf8_lossy(&entry.bytes).into_owned(),
            "blobs.encoding" => {
                encoding = String::from_utf8_lossy(&entry.bytes).trim().to_owned();
            }
            path if path.starts_with("blobs/sha256/") => {
                let hex = path.rsplit('/').next().unwrap_or_default().to_owned();
                // **∴ zlib 就解 ✗ 不是就原样 ✓** ⇒ **∴ 两种编码都兼容 ✗ 而**不猜** ✓ ★
                let raw = match encoding.as_str() {
                    "zlib" => yanshi_render::zlib_decompress(&entry.bytes)
                        .unwrap_or_else(|| entry.bytes.clone()),
                    _ => entry.bytes.clone(),
                };
                blobs.insert(hex, raw);
            }
            _ => skipped_entries += 1,
        }
    }
    if atoms_text.is_empty() {
        bail!("包里没有 atoms.jsonl ⇒ 这不像一个 .yanshi 工程包");
    }

    // **∴ 尺寸从 `meta.json` 读 ✗**（**交付版写死 1920×1080 ＋ `SCALE=0.5` ⇒ **900×640 的工程
    //   只会画出左上角那一块** ✓）★
    let meta: Value = serde_json::from_str(&meta_text).unwrap_or(Value::Null);
    let doc_w = meta.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
    let doc_h = meta.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
    if doc_w == 0 || doc_h == 0 {
        bail!("meta.json 里没有可用的 width/height ⇒ 判不了画布尺寸");
    }
    let doc_id = meta
        .get("doc_id")
        .and_then(Value::as_str)
        .unwrap_or("timelapse")
        .to_owned();
    let read_ms = t_read.elapsed().as_millis();
    let scale = f64::min(
        1.0,
        f64::min(
            f64::from(MAX_W) / f64::from(doc_w),
            f64::from(MAX_H) / f64::from(doc_h),
        ),
    );
    let out_w = ((f64::from(doc_w) * scale).round() as u32).max(1);
    let out_h = ((f64::from(doc_h) * scale).round() as u32).max(1);
    println!(
        "画布 {doc_w}×{doc_h} ⇒ 输出 {out_w}×{out_h}（缩放 {scale:.3}）｜blob {} 个（编码 {}）｜忽略条目 {}",
        blobs.len(),
        encoding,
        skipped_entries
    );
    // **读包/解码的开销也报出来** ✓（第 612 轮；外部第 91 次要"阶段计时"）
    //   ⇒ 他们的包 0.3 GB / 400 blob ⇒ **如果时间花在读包上**，那么"渲染慢"这个结论就是错的 ✓
    //   ⇒ 打印到 **stderr** ✓（与进度行同一路 ✓，被 timeout 杀掉时也能看到 ✓）。
    eprintln!(
        "读包 {read_ms} ms｜blob {} 个（编码 {encoding}）｜忽略条目 {skipped_entries}",
        blobs.len()
    );

    let layout = Layout {
        doc_w,
        doc_h,
        out_w,
        out_h,
    };
    // **∴ 用仓库自己的内核 replay ✗**（**与浏览器同一个引擎 ✓）★
    //   ＋ **∴ 而**增量重放放在 `yanshi_timelapse::Timeline` 里 ✗**（**它**只渲脏区并集 ✓）
    //     ＋ **∴ 逐行 apply ⇒ **脏 tile 并集**长起来 ⇒ **出帧时只渲那一块** ✓ ★★
    // **★ 降采样回放（第 630 轮）★**：目标输出比画布小 ⇒ **按输出尺寸回放** ✓
    //   ⇒ 现在渲 `4608×8192` 只为得到 `608×1080`：**56× 白算** ✗（外部第 93 次同结论 ✓）。
    //   ⇒ 用 `YANSHI_TIMELAPSE_SHRINK=1` 显式开启（默认不变 ✓）；只处理
    //     `create_document` / `create_layer` / `import_image` 三种原子，
    //     遇到别的 kind **整包放弃**（并说一句 ✓）——宁可不缩，也不出错图 ✗。
    let mut shrink = std::env::var_os("YANSHI_TIMELAPSE_SHRINK").is_some() && scale < 1.0;
    let (shrink_w, shrink_h) = (out_w, out_h);
    // **预先检查整包能不能缩** ✓：只认这三种原子 ✓（别的 kind 有独立几何语义 ⇒ 不猜 ✓）
    if shrink {
        let supported = |line: &str| {
            matches!(
                serde_json::from_str::<Value>(line)
                    .ok()
                    .and_then(|atom| atom.get("kind").and_then(Value::as_str).map(str::to_owned))
                    .as_deref(),
                Some("create_document") | Some("create_layer") | Some("import_image")
            )
        };
        let unsupported = atoms_text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .find(|line| !supported(line));
        if let Some(line) = unsupported {
            let kind = serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|atom| atom.get("kind").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| "?".to_owned());
            eprintln!(
                "⚠️ 降采样回放：包里出现 `{kind}` 原子（不在支持清单里）⇒ **整包按原尺寸回放** ✓ \
                 （宁可不缩，也不出错图 ✗）"
            );
            shrink = false;
        }
    }
    let (play_w, play_h) = if shrink {
        (shrink_w, shrink_h)
    } else {
        (doc_w, doc_h)
    };
    if shrink {
        eprintln!(
            "降采样回放：画布 {doc_w}×{doc_h} ⇒ **按输出 {play_w}×{play_h} 回放**（渲染量约降 {:.0}×）✓",
            1.0 / (f64::from(play_w) * f64::from(play_h) / (f64::from(doc_w) * f64::from(doc_h)))
        );
    }
    let mut timeline =
        yanshi_timelapse::Timeline::new(&doc_id, TILE_SIZE, play_w, play_h, KERNEL_MEMORY_LIMIT)
            .map_err(|e| anyhow!("建内核失败：{} {}", e.code, e.detail))?;
    for (hex, data) in &blobs {
        let got = timeline
            .blob_put(data)
            .map_err(|e| anyhow!("写 blob {hex} 失败：{} {}", e.code, e.detail))?;
        // **∴ 顺手做一次**完整性核对**✗**：**包内路径名**就是它的内容哈希** ✓
        //   ＋ **∴ 不一致 ⇒ **包坏了 / 我们解错了编码**✗ ⇒ **∴ 必须让人看见 ✗ 不许静默 ✓ ★★
        if !got.ends_with(hex) {
            eprintln!("⚠️ blob {hex} 的实际哈希是 {got} ⇒ 包或编码有问题");
        }
    }

    // **∴ 逐行 apply ✗**：**保留原始行 ✗ 直接喂内核**（**∴ 不做"只认 import_image"那种过滤 ✓）★
    let atom_lines: Vec<&str> = atoms_text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let end_idx = end_atom.unwrap_or(atom_lines.len()).min(atom_lines.len());
    let start_idx = start_atom.min(end_idx);

    // **★ `meta.json` 的尺寸必须与 `create_document` 原子**一致**** ✓
    //   （第 1349 轮；外部「视频只有 48 字节」换来的）
    //   **⇒ 症状** ✗：不一致时 ⇒ 内核按**原子**的尺寸建文档、ffmpeg 按 **meta** 的尺寸出帧
    //     ⇒ 重放中途报「渲染区域 … 与文档 64×64 不相交」⇒ **中止**
    //     ⇒ **mp4 只剩 ffmpeg 写的头**（实测 261 字节 + 一行 Error）⇒ 外部看到 48 字节，
    //        像「卡住」⇒ **而真正的原因在那一行 Error 里** ✗
    //   **⇒ 所以先比一次** ⇒ 报错同时写出**两边的数字** ✓
    if let Some(line) = atom_lines
        .iter()
        .find(|line| line.contains("\"create_document\""))
    {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            // **尺寸在 `payload.width` / `payload.height`** ✓（不是 `payload.canvas.*`）
            //   ⇒ **实测 dump 才知道**：`payload` 的键是
            //     `background` / `color_space` / `doc_id` / `height` / `width` ✓
            let payload = v.get("payload");
            let aw = payload
                .and_then(|p| p.get("width"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let ah = payload
                .and_then(|p| p.get("height"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            if aw != 0 && ah != 0 && (aw != doc_w || ah != doc_h) {
                bail!(
                    "meta.json 说画布 {doc_w}×{doc_h}，而 atoms.jsonl 的 create_document 说 {aw}×{ah} \
                     ⇒ 两者必须一致（内核按**原子**建文档、ffmpeg 按 **meta** 出帧 ⇒ 不一致会在中途失败、\
                     只留下 mp4 的头）⇒ 请把 meta.json 改成与原子一致"
                );
            }
        }
    }

    let t_replay = Instant::now();

    // **∴ ffmpeg 起不来时要给一句人话 ✗**（**∴ 交付版只 `eprintln!("ffmpeg 失败")` ✓）★
    let mut ffmpeg = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{out_w}x{out_h}"),
            "-framerate",
            "30",
            "-i",
            "-",
            "-c:v",
            // **编码器要按可用性挑** ✓（第 601 轮）：外部那台机器若没有 `libx264`，
            //   ffmpeg 只会写出文件头（我们之前还会 exit 0）⇒ 现在按 `-encoders` 探测，
            //   依次尝试 `libx264` → `mpeg4`（两者都常见 ✓），并把选中的名字打进 banner ✓。
            &pick_encoder(),
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "23",
            &out_mp4,
        ])
        .stdin(Stdio::piped())
        // **ffmpeg 的 stderr 必须留档** ✓（第 599 轮）：原来 `Stdio::null()` **把它丢了** ✗
        //   ⇒ 外部报「261 字节空视频」时**拿不到那行 Error** ✓（他们回了两轮都没法给 ✓）
        //   ⇒ 现在写进 `<输出>.ffmpeg.log` ✓，失败时把**末几行**打出来并指路 ✓。
        .stderr(Stdio::from(
            std::fs::File::create(ffmpeg_log(std::path::Path::new(&out_mp4)))
                .map_err(|e| anyhow!("建 ffmpeg 日志失败（{e}）⇒ 检查输出目录是否可写"))?,
        ))
        .spawn()
        .map_err(|e| {
            anyhow!("起不来 ffmpeg（{e}）⇒ 这个导出需要一个**外部 ffmpeg**，请先装上或放进 PATH")
        })?;
    let mut stdin = ffmpeg.stdin.take().expect("stdin 是 piped");

    let mut frames = 0usize;
    // **缩小后的 RGB 缓冲**（复用，每帧整幅重算 ⇒ 第 568 轮先保证正确）
    let mut scaled: Vec<u8> = vec![0u8; (layout.out_w as usize) * (layout.out_h as usize) * 3];
    let mut apply_failures = 0usize;
    let mut first_failure: Option<String> = None;
    // **★★★ 第 249 轮：**先立账 ✗ 再谈优化** ★★★**（**门控 `YANSHI_TIMELAPSE_PROBE` ✓）
    //   ＋ **∴ 分四段 ✗**：**apply（**喂 atom ✓）｜render（**内核渲 ✓）｜scale（**缩放＋去 alpha ✓）
    //     ｜write（**写进 ffmpeg ✓）⇒ **∴ 一眼看出**钱花在哪一段** ✓ ★★
    //   ＋ **∴ 为什么**必须自己量 ✗**：**交付版说"Rust 单线程 pipe 最快、rayon 反而慢 28%"** ✓
    //     ＋ **∴ 而**那套数**是在**它自己的简化合成**上量的**✗ ⇒ **∴ 与真管线**不可比** ✓ ★★
    let probe = std::env::var_os("YANSHI_TIMELAPSE_PROBE").is_some();
    let mut account = Account::default();

    // **★ 进度必须落盘 ★**（第 611 轮；外部第 91 次实测）
    //   他们的包在 `timeout 240` 下被杀 ⇒ **一行诊断都没有** ⇒ 只能从"没有 ffmpeg 输出"反推
    //   ⇒ 所以每 25 个 atom 往 **stderr** 打一行 ✓（被杀之后仍能看出卡在哪一段 ✓）。
    let progress_every = 25usize;
    let started = Instant::now();
    for (i, line) in atom_lines.iter().enumerate() {
        if i >= end_idx {
            break;
        }
        if i % progress_every == 0 {
            eprintln!(
                "  进度 {i}/{}｜已渲 {} 帧｜apply {} ms｜render {} ms｜累计 {} s",
                atom_lines.len().min(end_idx),
                frames,
                account.apply_ns / 1_000_000,
                account.render_ns / 1_000_000,
                started.elapsed().as_secs()
            );
        }
        let t_apply = Instant::now();
        // **降采样回放** ✓：这一行的几何按输出尺寸改写（位图也一起降采样 ✓）
        let shrunk: Option<String> = if shrink {
            shrink_atom(line, &blobs, play_w, play_h, doc_w, doc_h, &mut timeline)
        } else {
            None
        };
        let line_to_apply = shrunk.as_deref().unwrap_or(line);
        if let Err(error) = timeline.apply(line_to_apply) {
            apply_failures += 1;
            if first_failure.is_none() {
                first_failure = Some(format!("第 {i} 行：{} {}", error.code, error.detail));
            }
        }
        let atom_apply_ns = t_apply.elapsed().as_nanos();
        account.apply_ns += atom_apply_ns;
        // **★ 逐个 atom 的 apply 耗时 ★**（第 615 轮；外部第 92 次实测）
        //   他们的日志停在 `进度 0/209` 之后 ⇒ 说明**卡在第 1~10 个 atom 之间** ✓
        //   ⇒ 前 10 个 atom **每个都打印**（带 kind）✓；之后**只打印超过 2 秒的** ✓（避免刷屏 ✓）
        if i < 10 || atom_apply_ns >= 2_000_000_000 {
            let kind = serde_json::from_str::<Value>(line)
                .ok()
                .and_then(|atom| atom.get("kind").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_else(|| "?".to_owned());
            eprintln!(
                "    atom {i}（{kind}）apply {} ms",
                atom_apply_ns / 1_000_000
            );
        }
        if i >= start_idx && (i + 1 - start_idx) % frame_every == 0 {
            push_frame(
                &mut timeline,
                &mut stdin,
                &mut frames,
                layout,
                &mut account,
                &mut scaled,
            )?;
        }
    }
    push_frame(
        &mut timeline,
        &mut stdin,
        &mut frames,
        layout,
        &mut account,
        &mut scaled,
    )?;
    drop(stdin);
    let status = ffmpeg.wait().context("等 ffmpeg 失败")?;
    if !status.success() {
        bail!(
            "ffmpeg 退出码 {:?} ⇒ 没有产出可用的视频。ffmpeg 的 stderr 在 {}：\n{}",
            status.code(),
            ffmpeg_log(std::path::Path::new(&out_mp4)).display(),
            tail_of(&ffmpeg_log(std::path::Path::new(&out_mp4)), 12)
        );
    }
    // **★ 一帧都没写出去 ⇒ 必须报错 ★**（第 589 轮；外部第 86-88 次实测）
    //   他们的环境里 `ffmpeg` **返回成功**、可 mp4 只有 **261 字节**（只有文件头、`ffprobe` 读不出流）✗
    //   ⇒ 而那时我们**照样 exit 0** ✗ ⇒ 最坏的一种"成功" ✓
    //   ⇒ 所以：**帧数为 0 就是失败** ✓（不猜他们那边为什么送不进去，但绝不再说"成功" ✓）。
    ensure_frames_written(
        frames,
        start_idx,
        end_idx,
        frame_every,
        layout.out_w,
        layout.out_h,
    )?;
    if probe {
        let ms = |ns: u128| ns as f64 / 1e6;
        account.frame_render_ns.sort_unstable();
        let median = account
            .frame_render_ns
            .get(account.frame_render_ns.len() / 2)
            .map_or(0.0, |v| ms(*v));
        // **∴ 语义证据 ✗**（**∴ 不看墙钟也能看出"脏区路真的走了" ✓）★
        let (full_renders, dirty_renders, dirty_tiles, dirty_runs) = timeline.stats();
        println!(
            "  ★ 渲染账 ★ 整幅 {full_renders} 次｜脏区 {dirty_renders} 次（**共 {dirty_tiles} 个 tile ✗ {dirty_runs} 条 run ✓）★"
        );
        println!(
            "  ★ 分段账（ms）★ apply {:.0}｜render {:.0}（每帧中位 {:.1}）｜scale {:.0}｜write {:.0}",
            ms(account.apply_ns),
            ms(account.render_ns),
            median,
            ms(account.scale_ns),
            ms(account.write_ns)
        );
    }
    if apply_failures > 0 {
        eprintln!(
            "⚠️ {apply_failures} 个 atom 没被内核接受（首个：{}）⇒ 视频可能缺内容",
            first_failure.unwrap_or_default()
        );
    }
    println!(
        "  {} 帧｜apply {}..{}｜replay+编码 {:.1}s｜总 {:.1}s",
        frames,
        start_idx,
        end_idx,
        t_replay.elapsed().as_secs_f32(),
        t0.elapsed().as_secs_f32()
    );
    // **渲染账** ✓（第 626 轮）：外部重跑时能直接看到"整幅渲了几次／省了几次" ✓
    let (full, dirty, tiles, runs) = timeline.stats();
    println!(
        "  渲染账：整幅 {full} 次｜脏区 {dirty} 次（{tiles} tile／{runs} run）｜复用上一帧 {} 次",
        timeline.reused_frames()
    );
    Ok(())
}

/// **∴ 渲一帧并送进 ffmpeg ✗**（**∴ 写成自由函数 ✗ 不用闭包 ✓）★
///
/// **∴ 为什么不用闭包 ✗**：**闭包要同时可变借用 `kernel`／`stdin`／`frames`**✗
///   ＋ **∴ 而**外面那圈循环**也在用 `kernel`**✗ ⇒ **∴ 借用检查**不会过** ✓
///     ⇒ **∴ 于是**：**显式传参 ✗ 借用关系写在签名里 ✓** ★
/// **★ 分段账 ✗**（**第 249 轮 ✓）：**四段纳秒 ＋ 每帧渲染耗时（**算中位 ✓）★
///
/// **∴ 为什么收成一个结构体 ✗**：**11 个参数**clippy 直接拒**✗（**`too_many_arguments` ✓）
///   ＋ **∴ 而**我**不想用 `#[allow]` 盖住它**✗ ⇒ **∴ 于是**：**账目**本来就是一个东西** ✗
///     ⇒ **∴ 收成 `Account` ✗ 参数回到 8 个 ✓** ★★
/// **★ 帧的几何 ✗**（**第 249 轮 ✓）：**源画布 ＋ 目标尺寸 ✓
///
/// **∴ 为什么也收起来 ✗**：**`push_frame` 收到 8 个参数**clippy 仍然嫌多**✗（**7 是上限 ✓）
///   ＋ **∴ 而**这四个数**本来就是一件事**（**一张帧的坐标变换 ✓）⇒ **∴ 于是**：**打包 ✓）
///     ⇒ **∴ 参数回到 5 个 ✓** ★
#[derive(Clone, Copy)]
struct Layout {
    /// 源画布宽 ✓。
    doc_w: u32,
    /// 源画布高 ✓。
    doc_h: u32,
    /// 目标宽 ✓。
    out_w: u32,
    /// 目标高 ✓。
    out_h: u32,
}

#[derive(Default)]
struct Account {
    /// 喂 atom 的总纳秒 ✓。
    apply_ns: u128,
    /// 内核渲染的总纳秒 ✓。
    render_ns: u128,
    /// 缩放 ＋ 去 alpha 的总纳秒 ✓。
    scale_ns: u128,
    /// 写进 ffmpeg 的总纳秒 ✓。
    write_ns: u128,
    /// 每一帧的渲染纳秒（**用来算中位 ✗ 只看总和不看抖动 ✓）★
    frame_render_ns: Vec<u128>,
}

fn push_frame(
    timeline: &mut yanshi_timelapse::Timeline,
    stdin: &mut ChildStdin,
    frames: &mut usize,
    layout: Layout,
    account: &mut Account,
    scaled: &mut [u8],
) -> Result<()> {
    let t_render = Instant::now();
    // **∴ 交给 `Timeline::frame()` ✗**：**它内部决定**只渲脏区并集还是整幅**✗
    //   ＋ **∴ 并**把结果 blit 进持久缓冲**✗ ⇒ **∴ 这里拿到的是**整幅 RGBA** ✓ ★★
    // **第 568 轮：先退回"整幅缩小"** ✗ —— 外部实测「增量缩小 ⇒ mp4 只有 261 字节」（`f3bcde39`）✗
    //   ⇒ **正确性优先**：帧管线回到已知可用的整幅缩小 ✓；增量路径要**先有自己的判据**再启用 ✓
    //   （`Timeline::frame_rgb_scaled` 与 `scale_rgb_region` 都还在，等一条能证伪"空帧"的判据 ✓）。
    // **只重算脏区对应的输出像素** ✓（第 583 轮：正确键名的包实测 **scale 占 33%** ⇒ 值得）
    let frame = timeline
        .frame_rgb_scaled(layout.out_w, layout.out_h)
        .map_err(|e| anyhow!("渲染失败：{} {}", e.code, e.detail))?;
    let render_ns = t_render.elapsed().as_nanos();
    account.render_ns += render_ns;
    account.frame_render_ns.push(render_ns);
    let t_scale = Instant::now();
    let copy = scaled.len().min(frame.len());
    scaled[..copy].copy_from_slice(&frame[..copy]);
    account.scale_ns += t_scale.elapsed().as_nanos();
    // **绝不静默产出空帧** ✓（外部第 86 次：退出码 0 而 mp4 只有 261 字节 ⇒ 那是最坏的一种"成功"✗）
    if scaled.iter().all(|byte| *byte == 0) {
        anyhow::bail!(
            "第 {} 帧缩小后全是 0（画布 {}×{} ⇒ 输出 {}×{}）⇒ 拒绝写出空帧",
            *frames + 1,
            layout.doc_w,
            layout.doc_h,
            layout.out_w,
            layout.out_h
        );
    }
    let rgb = &scaled[..];
    let t_write = Instant::now();
    stdin.write_all(rgb).context("写帧到 ffmpeg 失败")?;
    account.write_ns += t_write.elapsed().as_nanos();
    *frames += 1;
    Ok(())
}

/// **没写出一帧就算失败** ✓（第 589 轮；外部第 86–88 次：`ffmpeg` 成功、mp4 只有 261 字节文件头）
///
/// 抽成纯函数是为了**能测**：真实触发要等 ffmpeg 拒绝输入，本机复现不了 ✓
/// （**变异**：把 `frames == 0` 的判断去掉 ⇒ 判据 `zero_frames_is_an_error` 红 ✓）。
/// ffmpeg 的 stderr 落盘位置（与输出同目录，便于用户直接看）。
/// 挑一个本机 ffmpeg **确实有**的编码器（`libx264` 优先，退回 `mpeg4`，再退回 `mpeg2video`）。
///
/// **为什么**：外部报「261 字节空视频、exit 0」，而最常见的环境差异就是**没有 libx264 的 ffmpeg 构建**。
/// 猜不如问：跑一次 `ffmpeg -hide_banner -encoders`，从输出里挑 ✓。
fn pick_encoder() -> String {
    let listed = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    // **顺序**：`libx264` → `libopenh264` → `mpeg4` → `mpeg2video` ✓
    //   （外部第 90 次实测：他们有 `libx264`、**没有** `libopenh264` ⇒ 所以 libx264 必须排第一 ✓；
    //    反过来若某机器只有 openh264，也能退回它 ✓）
    // **★ 不光"有"，还要"真的吃得下我们这帧" ★**（第 606 轮）
    //   外部第 90 次的实测：他们有 `libx264`、也有 `mpeg4`，可**输出仍是 261 字节** ✗
    //   ⇒ 说明"编码器存在"**不等于**"这条命令能跑通" ✓
    //   ⇒ 所以每个候选都**真跑一次**（同样的 rawvideo 输入 ＋ `-f null -` 丢弃输出 ✓）
    //     第一个跑通的才算 ✓；都不通就退回 `libx264` 让 ffmpeg 自己报错（错误会留档 ✓）。
    for candidate in ["libx264", "libopenh264", "mpeg4", "mpeg2video"] {
        if listed.contains(candidate) && encoder_accepts_a_frame(candidate) {
            return candidate.to_owned();
        }
    }
    // 一个都没探到（或 ffmpeg 不认 `-encoders`）⇒ 仍用 libx264，让 ffmpeg 自己报错（错误现在会留档 ✓）
    "libx264".to_owned()
}

/// **真喂一帧给它**，看这个编码器在**这条命令形状**下能不能跑通（第 606 轮）。
///
/// 用**与正式导出相同的输入形状**（`-f rawvideo -pix_fmt rgb24 -s WxH -i -`），
/// 只是把输出 `-f null -` 丢掉 ⇒ 不会写文件，也不会污染用户的目录 ✓。
///
/// **为什么必须真跑**：外部实测「编码器存在 ⇒ 仍然 261 字节」⇒ 存在性不是可用性 ✓。
fn encoder_accepts_a_frame(encoder: &str) -> bool {
    const PROBE_W: u32 = 64;
    const PROBE_H: u32 = 64;
    let mut child = match std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            "64x64",
            "-i",
            "-",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => return false,
    };
    let frame = vec![0u8; (PROBE_W * PROBE_H * 3) as usize];
    let wrote = std::io::Write::write_all(&mut stdin, &frame).is_ok();
    drop(stdin);
    wrote && child.wait().map(|status| status.success()).unwrap_or(false)
}

fn ffmpeg_log(out_mp4: &std::path::Path) -> std::path::PathBuf {
    let mut path = out_mp4.to_path_buf();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "out".to_owned());
    path.set_file_name(format!("{name}.ffmpeg.log"));
    path
}

/// 取文件末尾若干行（失败时打给用户看）。
fn tail_of(path: &std::path::Path, lines: usize) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let collected: Vec<&str> = text.lines().rev().take(lines).collect();
    collected.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn ensure_frames_written(
    frames: usize,
    start_idx: usize,
    end_idx: usize,
    frame_every: usize,
    out_w: u32,
    out_h: u32,
) -> Result<()> {
    if frames == 0 {
        bail!(
            "一帧都没写出去（apply {start_idx}..{end_idx}、每 {frame_every} 个 atom 一帧）\
             ⇒ ffmpeg 只写了文件头，拒绝把它当成成功。\
             请检查 ffmpeg 是否能吃 `-f rawvideo -pix_fmt rgb24 -s {out_w}x{out_h}`\
             （ffmpeg 的 stderr 在同目录 `<输出>.ffmpeg.log`）"
        );
    }
    Ok(())
}

#[cfg(test)]
mod encoder_tests {
    use super::pick_encoder;

    /// **判据**：挑出来的编码器必须**真的在本机 `ffmpeg -encoders` 里**（第 601 轮）。
    /// **变异**：让它返回 `definitely-not-an-encoder` ⇒ 红（若本机 ffmpeg 可用）。
    #[test]
    fn picked_encoder_exists_in_ffmpeg() {
        let listed = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default();
        let picked = pick_encoder();
        if listed.is_empty() {
            // 本机没有 ffmpeg ⇒ 这条判不了（不是产品问题）
            eprintln!("⊘ 不可判：本机没有可用的 ffmpeg ⇒ 跳过编码器检查");
            return;
        }
        assert!(
            listed.contains(&picked),
            "挑出来的编码器 `{picked}` 不在 `ffmpeg -encoders` 里 ⇒ 会在外部机器上写出空视频"
        );
        // **★ 还要"真的吃得下" ★**（第 606 轮）：外部有 `libx264` 却仍 261 字节 ⇒ 存在性≠可用性
        assert!(
            super::encoder_accepts_a_frame(&picked),
            "挑出来的编码器 `{picked}` 存在，但**喂一帧就失败** ⇒ 它会写出只有文件头的空视频"
        );
    }
}

/// **把一条原子改写成"按输出尺寸"的版本** ✓（第 630 轮，降采样回放）。
///
/// 只处理三种原子（调用方已预先确认整包只含这三种 ✓）：
/// - `create_document`：画布宽高改成 `play_w × play_h` ✓；
/// - `create_layer`：原样 ✓；
/// - `import_image`：`region` 按比例缩，**位图本身也按最近邻降采样**并重新入库 ✓；
///   `mime_type` 只支持 `image/x-yanshi-raw`，别的返回 `None` ⇒ 调用方按原样用会出错图，
///   所以调用方在预检查里就该拦住，这里再兜一层 ✓。
///
/// 返回 `None` ⇒ 这一行不能缩 ⇒ 调用方用原始行（**几何不再一致** ⇒ 因此只在预检查通过时才会走到这里 ✓）。
fn shrink_atom(
    line: &str,
    blobs: &HashMap<String, Vec<u8>>,
    play_w: u32,
    play_h: u32,
    doc_w: u32,
    doc_h: u32,
    timeline: &mut yanshi_timelapse::Timeline,
) -> Option<String> {
    let sx = f64::from(play_w) / f64::from(doc_w);
    let sy = f64::from(play_h) / f64::from(doc_h);
    let mut atom: Value = serde_json::from_str(line).ok()?;
    let kind = atom.get("kind").and_then(Value::as_str)?.to_owned();
    match kind.as_str() {
        "create_document" => {
            if let Some(payload) = atom.get_mut("payload").and_then(Value::as_object_mut) {
                payload.insert("width".to_owned(), Value::from(play_w));
                payload.insert("height".to_owned(), Value::from(play_h));
            }
        }
        "create_layer" => {}
        "import_image" => {
            let payload = atom.get_mut("payload")?.as_object_mut()?;
            let bitmap = payload.get("bitmap")?.clone();
            let mime = bitmap
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("");
            if mime != "image/x-yanshi-raw" {
                return None;
            }
            let hash = bitmap.get("blob_hash").and_then(Value::as_str)?.to_owned();
            let raw = blobs.get(&hash)?;
            let src_w = bitmap.get("width").and_then(Value::as_u64)? as u32;
            let src_h = bitmap.get("height").and_then(Value::as_u64)? as u32;
            if raw.len() < (src_w as usize) * (src_h as usize) * 4 {
                return None;
            }
            let dst_w = ((src_w as f64 * sx).round() as u32).max(1);
            let dst_h = ((src_h as f64 * sy).round() as u32).max(1);
            // **最近邻** ⇒ 确定性 ✓、无插值核 ✓（预览片够用 ✓）
            let mut small = Vec::with_capacity((dst_w as usize) * (dst_h as usize) * 4);
            for y in 0..dst_h {
                let sy_src = ((u64::from(y) * u64::from(src_h)) / u64::from(dst_h)) as usize;
                for x in 0..dst_w {
                    let sx_src = ((u64::from(x) * u64::from(src_w)) / u64::from(dst_w)) as usize;
                    let at = (sy_src * src_w as usize + sx_src) * 4;
                    small.extend_from_slice(&raw[at..at + 4]);
                }
            }
            let new_hash = timeline.blob_put(&small).ok()?;
            if let Some(entry) = payload.get_mut("bitmap").and_then(Value::as_object_mut) {
                entry.insert("blob_hash".to_owned(), Value::from(new_hash));
                entry.insert("width".to_owned(), Value::from(dst_w));
                entry.insert("height".to_owned(), Value::from(dst_h));
            }
            if let Some(region) = payload.get_mut("region").and_then(Value::as_object_mut) {
                let mut scale = |key: &str, factor: f64| {
                    if let Some(value) = region.get(key).and_then(Value::as_f64) {
                        let scaled = value * factor;
                        region.insert(key.to_owned(), json!(scaled));
                    }
                };
                scale("x", sx);
                scale("y", sy);
                scale("w", sx);
                scale("h", sy);
            }
        }
        _ => return None,
    }
    serde_json::to_string(&atom).ok()
}

#[cfg(test)]
mod frame_count_tests {
    use super::ensure_frames_written;

    /// **判据**：0 帧必须报错；>0 帧必须通过（第 589 轮）。
    /// **变异**：去掉 `frames == 0` 的判断 ⇒ 第一行断言会红。
    #[test]
    fn zero_frames_is_an_error() {
        assert!(
            ensure_frames_written(0, 0, 209, 10, 608, 1080).is_err(),
            "一帧都没有就必须报错（外部实测：ffmpeg 成功但 mp4 只有 261 字节文件头）"
        );
        assert!(
            ensure_frames_written(6, 0, 209, 10, 608, 1080).is_ok(),
            "写出了帧就必须通过"
        );
    }
}
