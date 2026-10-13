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
//!       ＋ **∴ 而**它**认全部 atom 种类**（`draw_stroke`／`draw_shape`／`draw_text`／`erase`／… ✓）
//!         ⇒ **∴ 手绘笔触**终于会出现在视频里** ✓ ★★★
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
use serde_json::Value;

/// 输出尺寸上限（**只缩不放 ✓**）。
const MAX_W: u32 = 1920;
/// 输出尺寸上限（**只缩不放 ✓**）。
const MAX_H: u32 = 1080;
/// 内核内存预算（**与浏览器同档 ✓**）：4K 工程 replay 需要它 ✓。
const KERNEL_MEMORY_LIMIT: usize = 2 * 1024 * 1024 * 1024;
/// 内核算术的 tile 尺寸（**与浏览器同一档 ✓**）。
const TILE_SIZE: u32 = 256;

fn main() -> Result<()> {
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

    // **∴ 用仓库自己的内核 replay ✗**（**与浏览器同一个引擎 ✓）
    let mut kernel =
        yanshi_wasm::Kernel::new(&doc_id, TILE_SIZE, doc_w, doc_h, KERNEL_MEMORY_LIMIT)
            .map_err(|e| anyhow!("建内核失败：{} {}", e.code, e.detail))?;
    for (hex, data) in &blobs {
        let got = kernel
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
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "23",
            &out_mp4,
        ])
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            anyhow!("起不来 ffmpeg（{e}）⇒ 这个导出需要一个**外部 ffmpeg**，请先装上或放进 PATH")
        })?;
    let mut stdin = ffmpeg.stdin.take().expect("stdin 是 piped");

    let mut frames = 0usize;
    let mut apply_failures = 0usize;
    let mut first_failure: Option<String> = None;

    for (i, line) in atom_lines.iter().enumerate() {
        if i >= end_idx {
            break;
        }
        if let Err(error) = kernel.apply_atom_json(line) {
            apply_failures += 1;
            if first_failure.is_none() {
                first_failure = Some(format!("第 {i} 行：{} {}", error.code, error.detail));
            }
        }
        if i >= start_idx && (i + 1 - start_idx) % frame_every == 0 {
            push_frame(
                &mut kernel,
                &mut stdin,
                &mut frames,
                doc_w,
                doc_h,
                out_w,
                out_h,
            )?;
        }
    }
    push_frame(
        &mut kernel,
        &mut stdin,
        &mut frames,
        doc_w,
        doc_h,
        out_w,
        out_h,
    )?;
    drop(stdin);
    let status = ffmpeg.wait().context("等 ffmpeg 失败")?;
    if !status.success() {
        bail!("ffmpeg 退出码 {:?} ⇒ 没有产出可用的视频", status.code());
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
    Ok(())
}

/// **∴ 渲一帧并送进 ffmpeg ✗**（**∴ 写成自由函数 ✗ 不用闭包 ✓）★
///
/// **∴ 为什么不用闭包 ✗**：**闭包要同时可变借用 `kernel`／`stdin`／`frames`**✗
///   ＋ **∴ 而**外面那圈循环**也在用 `kernel`**✗ ⇒ **∴ 借用检查**不会过** ✓
///     ⇒ **∴ 于是**：**显式传参 ✗ 借用关系写在签名里 ✓** ★
fn push_frame(
    kernel: &mut yanshi_wasm::Kernel,
    stdin: &mut ChildStdin,
    frames: &mut usize,
    doc_w: u32,
    doc_h: u32,
    out_w: u32,
    out_h: u32,
) -> Result<()> {
    let region = yanshi_core::Bbox::new(0.0, 0.0, f64::from(doc_w), f64::from(doc_h));
    let rendered = kernel
        .render_region(region)
        .map_err(|e| anyhow!("渲染失败：{} {}", e.code, e.detail))?;
    let rgb = to_rgb_scaled(&rendered.rgba8, doc_w, doc_h, out_w, out_h);
    stdin.write_all(&rgb).context("写帧到 ffmpeg 失败")?;
    *frames += 1;
    Ok(())
}

/// **RGBA8 → RGB24 ✗ 并缩放到 `(out_w, out_h)` ✓**（**∴ 盒平均 ✗ 比最近邻干净 ✓）★
///
/// **∴ 为什么按盒平均 ✗**：**缩小时最近邻会**丢掉笔画**✗（**一条细线可能整条被抽掉 ✓）⇒
///   **∴ 而**盒平均对每个目标像素取源矩形均值**✗ ⇒ **∴ 细线**会变淡 ✗ 而**不会消失** ✓ ★★
fn to_rgb_scaled(src: &[u8], src_w: u32, src_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (out_w as usize) * (out_h as usize) * 3];
    let (sw, sh) = (src_w as usize, src_h as usize);
    for oy in 0..out_h as usize {
        let y0 = oy * sh / out_h as usize;
        let y1 = (((oy + 1) * sh) / out_h as usize).max(y0 + 1).min(sh);
        for ox in 0..out_w as usize {
            let x0 = ox * sw / out_w as usize;
            let x1 = (((ox + 1) * sw) / out_w as usize).max(x0 + 1).min(sw);
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            let mut n = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * sw + x) * 4;
                    if i + 3 < src.len() {
                        r += u32::from(src[i]);
                        g += u32::from(src[i + 1]);
                        b += u32::from(src[i + 2]);
                        n += 1;
                    }
                }
            }
            let n = n.max(1);
            let o = (oy * out_w as usize + ox) * 3;
            out[o] = (r / n) as u8;
            out[o + 1] = (g / n) as u8;
            out[o + 2] = (b / n) as u8;
        }
    }
    out
}
