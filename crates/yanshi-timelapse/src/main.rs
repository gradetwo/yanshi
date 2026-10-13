//! yanshi-timelapse: 将 .yanshi 工程导出为绘画过程视频。
//!
//! 用法: yanshi-timelapse <输入.yanshi.zip> <输出.mp4> [每N个atom一帧]
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::time::Instant;

// 直接输出 1080p
const W: u32 = 1920;
const H: u32 = 1080;
const SCALE: f64 = 0.5;

#[derive(Debug)]
struct Patch {
    id: String,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    blob_path: String,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 || args.len() > 6 {
        eprintln!(
            "用法: {} <输入.yanshi.zip> [输出.mp4] [每N个atom一帧=10] [起始atom=0] [结束atom=全部]",
            args[0]
        );
        eprintln!("示例: {} in.zip", args[0]);
        eprintln!("      {} in.zip out.mp4 5 100 500", args[0]);
        std::process::exit(1);
    }
    let zip_path = &args[1];
    // 输出文件名默认: 输入文件名 + -timelapse.mp4
    // 空字符串也视为未指定
    let default_out = {
        let p = std::path::Path::new(zip_path);
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let base = stem.strip_suffix(".yanshi").unwrap_or(&stem);
        format!("{}-timelapse.mp4", base)
    };
    let out_mp4: &str = match args.get(2) {
        Some(s) if !s.is_empty() => s,
        _ => &default_out,
    };
    let frame_every: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10);
    let start_atom: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let end_atom: Option<usize> = args.get(5).and_then(|s| s.parse().ok());
    println!("输入: {}", zip_path);
    println!("输出: {}", out_mp4);
    println!(
        "每 {} 个 atom 一帧, 范围: {}..{}",
        frame_every,
        start_atom,
        end_atom
            .map(|e| e.to_string())
            .unwrap_or("结束".to_string())
    );

    let t0 = Instant::now();

    // 1. 读 atoms.jsonl
    println!("读取 atoms...");
    let zf = File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(zf)?;
    let tar_name = zip.file_names().next().unwrap().to_string();
    let mut tar_data = Vec::new();
    zip.by_name(&tar_name)?.read_to_end(&mut tar_data)?;
    drop(zip);

    let mut tar = tar::Archive::new(&tar_data[..]);
    let mut atoms_json = String::new();
    let mut seen = 0;
    for entry in tar.entries()? {
        let mut e = entry?;
        let p = e.path()?;
        let ps = p.to_str().unwrap_or("");
        if seen < 3 {
            seen += 1;
        }
        if ps == "atoms.jsonl" || ps.ends_with("/atoms.jsonl") || ps == "./atoms.jsonl" {
            e.read_to_string(&mut atoms_json)?;
            break;
        }
    }
    if atoms_json.is_empty() {
        anyhow::bail!("未找到 atoms.jsonl");
    }
    let t1 = Instant::now();

    let mut patches: Vec<Patch> = Vec::new();
    let mut reverted: HashSet<String> = HashSet::new();
    let mut line_count = 0;
    let mut parse_fail = 0;
    for line in atoms_json.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        line_count += 1;
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => {
                parse_fail += 1;
                continue;
            }
        };
        match v.get("kind").and_then(|k| k.as_str()) {
            Some("import_image") => {
                let p = &v["payload"];
                let reg = &p["region"];
                let bh_raw = p["bitmap"]["blob_hash"].as_str().unwrap_or("");
                if bh_raw.is_empty() {
                    continue;
                }
                let bh = bh_raw.replace("sha256:", "");
                let id = v["id"].as_str().unwrap_or("").to_string();
                if id.is_empty() {
                    continue;
                }
                let (x, y) = match (
                    reg.get("x").and_then(|v| v.as_f64()),
                    reg.get("y").and_then(|v| v.as_f64()),
                ) {
                    (Some(x), Some(y)) => (x as i32, y as i32),
                    _ => continue,
                };
                let (w, h) = match (p["width"].as_u64(), p["height"].as_u64()) {
                    (Some(w), Some(h)) => (w as u32, h as u32),
                    _ => continue,
                };
                patches.push(Patch {
                    id,
                    x,
                    y,
                    w,
                    h,
                    blob_path: format!("blobs/sha256/{}/{}/{}", &bh[0..2], &bh[2..4], bh),
                });
            }
            Some("revert") => {
                if let Some(t) = v["payload"].get("target").and_then(|t| t.as_str()) {
                    reverted.insert(t.to_string());
                }
            }
            _ => {}
        }
    }
    let active: Vec<&Patch> = patches
        .iter()
        .filter(|p| !reverted.contains(&p.id))
        .collect();
    println!(
        // **∴ 三个数都要报 ✗**（合入时 clippy 指出 `line_count`／`parse_fail` 算了没用 ✓）：
        //   ＋ **∴ `line_count`** ＝ atoms.jsonl 的**真实行数**（含非图像类 atom ✓）；
        //   ＋ **∴ `parse_fail`** ＝ **坏行数**（**∴ 非 0 ⇒ **这个工程可能不完整 ✗ 必须让人看见 ✓）；
        //   ＋ **∴ `patches.len()`** ＝ **import_image 条数**（≠ 行数 ✗ 别的 kind 也占行 ✓）★
        "  {} 行（{} 行解析失败）｜import_image {}｜有效 {}｜耗时 {:.1}s",
        line_count,
        parse_fail,
        patches.len(),
        active.len(),
        t1.duration_since(t0).as_secs_f32()
    );

    // 2. 提取 blob (单线程, 实测比 rayon 更快)
    println!("提取 blob...");
    let t2 = Instant::now();
    let need: HashSet<&str> = active.iter().map(|p| p.blob_path.as_str()).collect();
    let mut tar2 = tar::Archive::new(&tar_data[..]);
    let mut blob_cache: HashMap<String, Vec<u8>> = HashMap::new();
    let mut blob_dims: HashMap<String, (u32, u32)> = HashMap::new();
    // path -> (w, h)
    let dim_map: HashMap<&str, (u32, u32)> = active
        .iter()
        .map(|p| (p.blob_path.as_str(), (p.w, p.h)))
        .collect();
    for entry in tar2.entries()? {
        let mut e = entry?;
        let path = e.path()?.to_str().unwrap().to_string();
        if need.contains(path.as_str()) {
            let mut data = Vec::new();
            e.read_to_end(&mut data)?;
            // 降采样到目标分辨率
            let (w, h) = dim_map[path.as_str()];
            let w2 = (w as f64 * SCALE) as u32;
            let h2 = (h as f64 * SCALE) as u32;
            let mut dst = vec![0u8; (w2 * h2 * 4) as usize];
            for yy in 0..h2 {
                for xx in 0..w2 {
                    let sx = ((xx * 2).min(w - 1)) as usize;
                    let sy = ((yy * 2).min(h - 1)) as usize;
                    let si = (sy * w as usize + sx) * 4;
                    let di = (yy as usize * w2 as usize + xx as usize) * 4;
                    if si + 4 <= data.len() && di + 4 <= dst.len() {
                        dst[di..di + 4].copy_from_slice(&data[si..si + 4]);
                    }
                }
            }
            blob_cache.insert(path.clone(), dst);
            blob_dims.insert(path, (w2, h2));
            if blob_cache.len() % 200 == 0 {
                println!("  {}/{}", blob_cache.len(), need.len());
            }
            if blob_cache.len() >= need.len() {
                break;
            }
        }
    }
    drop(tar_data);
    println!(
        "  {} blobs, 耗时 {:.1}s",
        blob_cache.len(),
        t2.elapsed().as_secs_f32()
    );

    // 3+4. replay + 直接 pipe 给 ffmpeg (无中间 PNG 文件)
    println!("replay + pipe to ffmpeg...");
    let t3 = Instant::now();
    let mut canvas = vec![255u8; (W * H * 4) as usize];
    let mut ffmpeg = std::process::Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
            &format!("{}x{}", W, H),
            "-framerate",
            "30",
            "-i",
            "-",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast", // 最快 preset
            "-pix_fmt",
            "yuv420p",
            "-crf",
            "23",
            out_mp4,
        ])
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut stdin = ffmpeg.stdin.take().unwrap();
    // RGBA -> 需要去掉 alpha, ffmpeg 要 yuv420p, 但我们先送 rgba 让 ffmpeg 转
    // 实际上直接送 rgb24 更快, 这里先转
    use std::io::Write;
    let mut fi = 0;
    let mut rgb_buf = vec![0u8; (W * H * 3) as usize];
    // RGBA->RGB 转换 (单线程, 实测比 rayon 更快)
    let rgba_to_rgb = |canvas: &[u8], rgb_buf: &mut [u8]| {
        for j in 0..(W * H) as usize {
            rgb_buf[j * 3] = canvas[j * 4];
            rgb_buf[j * 3 + 1] = canvas[j * 4 + 1];
            rgb_buf[j * 3 + 2] = canvas[j * 4 + 2];
        }
    };
    // 应用 start 之前的所有 atom (不渲染, 只为建立正确画布状态)
    // 然后在 [start, end) 范围内按 frame_every 渲染
    let end_idx = end_atom.unwrap_or(active.len()).min(active.len());
    let start_idx = start_atom.min(end_idx);
    println!(
        "  应用 {} 个 atoms, 渲染范围 {}..{}",
        active.len(),
        start_idx,
        end_idx
    );

    for (i, p) in active.iter().enumerate() {
        if i >= end_idx {
            break;
        }
        if let Some(data) = blob_cache.get(p.blob_path.as_str()) {
            let (w2, h2) = blob_dims[p.blob_path.as_str()];
            let x = (p.x as f64 * SCALE) as i32;
            let y = (p.y as f64 * SCALE) as i32;
            composite(&mut canvas, data, w2, h2, x, y);
        }
        // 只在 [start, end) 范围内按 frame_every 渲染
        if i >= start_idx && (i + 1 - start_idx) % frame_every == 0 {
            fi += 1;
            rgba_to_rgb(&canvas, &mut rgb_buf);
            stdin.write_all(&rgb_buf)?;
        }
    }
    fi += 1;
    rgba_to_rgb(&canvas, &mut rgb_buf);
    stdin.write_all(&rgb_buf)?;
    drop(stdin);
    let status = ffmpeg.wait()?;
    if !status.success() {
        eprintln!("ffmpeg 失败");
    }
    println!(
        "  {} 帧, replay+编码 耗时 {:.1}s",
        fi,
        t3.elapsed().as_secs_f32()
    );
    println!("总耗时: {:.1}s (上一版 14.4s)", t0.elapsed().as_secs_f32());
    Ok(())
}

/// 整数 alpha 混合, dst 是 1920x1080 RGBA8
fn composite(dst: &mut [u8], src: &[u8], sw: u32, sh: u32, x: i32, y: i32) {
    let (sw, sh) = (sw as i32, sh as i32);
    let (w, h) = (W as i32, H as i32);
    let sx0 = 0.max(-x);
    let sy0 = 0.max(-y);
    let dx0 = 0.max(x);
    let dy0 = 0.max(y);
    let dx1 = w.min(x + sw);
    let dy1 = h.min(y + sh);
    if dx1 <= dx0 || dy1 <= dy0 {
        return;
    }
    let sx1 = sx0 + (dx1 - dx0);
    let sy1 = sy0 + (dy1 - dy0);
    // **∴ 这两个端点不是摆设 ✗**（合入时 clippy 说它们没用 ✓）：
    //   ＋ **∴ 它们**表达"**源矩形不越界**"✗ ⇒ **∴ 用断言把它变成**会红的事实** ✓ ★
    debug_assert!(
        sx1 <= sw && sy1 <= sh,
        "源矩形越界: ({sx1},{sy1}) vs ({sw},{sh})"
    );

    for row in 0..(dy1 - dy0) {
        let dy = (dy0 + row) as usize;
        let sy = (sy0 + row) as usize;
        for col in 0..(dx1 - dx0) {
            let dx = (dx0 + col) as usize;
            let sx = (sx0 + col) as usize;
            let di = (dy * W as usize + dx) * 4;
            let si = (sy * sw as usize + sx) * 4;
            let sa = src[si + 3] as u32;
            if sa == 0 {
                continue;
            }
            if sa == 255 {
                dst[di..di + 4].copy_from_slice(&src[si..si + 4]);
                continue;
            }
            let isa = 255 - sa;
            // RGB
            for c in 0..3 {
                let v = (src[si + c] as u32 * sa + dst[di + c] as u32 * isa) / 255;
                dst[di + c] = v as u8;
            }
            // A
            let da = dst[di + 3] as u32;
            dst[di + 3] = (sa + da * isa / 255) as u8;
        }
    }
}
