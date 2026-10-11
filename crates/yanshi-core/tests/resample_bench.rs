//! **★ `resample_rgba` 的微基准 ✗ ★**（**第 525 轮 ✓）
//!
//! **∴ 为什么必须有它 ✗**：**服务端测出来的 950 ms（**4K→512 ✓）**混了 HTTP／组装／PNG** ✓
//!   ＋ **∴ 而**上一轮消除 f64／闭包的尝试**没有收益**（**反而慢 11% ✓）
//!     ⇒ **∴ 于是**：**必须在**函数级**重新测 ⇒ **∴ 并分而治之** ✓ ★
//!
//! **∴ 用法 ✗**：`cargo test -p yanshi-core --release --test resample_bench -- --ignored --nocapture`
//!
//! **∴ 分成三块 ✗**：
//!   ⇒ **∴ ① 完整 ✗**：**真调 `resample_rgba`** ✓
//!     ＋ **∴ ② 只算坐标 ✗**：**只算每输出像素的 `x0`／`x1`／`wx`（**不读源、不写输出 ✓）
//!       ＋ **∴ ③ 只采样 ＋ 写 ✗**：**用**预先算好的坐标**做双线性 ＋ 写输出** ✓
//!         ⇒ **∴ ②＋③ 与 ① 一比 ⇒ **∴ 就能看出**坐标计算**占多少** ✓ ★

use std::time::Instant;
use yanshi_core::resample::{resample_rgba, ResampleFilter};

fn pattern(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (w as usize) * (h as usize) * 4];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for byte in out.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    out
}

/// **∴ ② 只算坐标 ✗**（**不读源、不写输出 ✓）★
fn coords_only(src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> u64 {
    let mut acc = 0u64;
    for y in 0..dst_h {
        let fy = (y as f64 + 0.5) * src_h as f64 / dst_h as f64 - 0.5;
        let y0 = fy.floor().max(0.0) as u32;
        let _y1 = (y0 + 1).min(src_h - 1);
        let wy = (fy - fy.floor()).clamp(0.0, 1.0);
        for x in 0..dst_w {
            let fx = (x as f64 + 0.5) * src_w as f64 / dst_w as f64 - 0.5;
            let x0 = fx.floor().max(0.0) as u32;
            let _x1 = (x0 + 1).min(src_w - 1);
            let wx = (fx - fx.floor()).clamp(0.0, 1.0);
            acc = acc.wrapping_add((x0 as u64) ^ (wx as u64) ^ (wy as u64));
        }
    }
    acc
}

/// **∴ ③ 只采样 ＋ 写 ✗**（**坐标预先算好 ✓）★
fn sample_only(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> u64 {
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for y in 0..dst_h {
        let fy = (y as f64 + 0.5) * src_h as f64 / dst_h as f64 - 0.5;
        let y0 = fy.floor().max(0.0) as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let wy = (fy - fy.floor()).clamp(0.0, 1.0);
        for x in 0..dst_w {
            let fx = (x as f64 + 0.5) * src_w as f64 / dst_w as f64 - 0.5;
            let x0 = fx.floor().max(0.0) as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let wx = (fx - fx.floor()).clamp(0.0, 1.0);
            let to = ((y * dst_w + x) * 4) as usize;
            for channel in 0..4 {
                let at = |px: u32, py: u32| -> f64 {
                    src[((py * src_w + px) * 4 + channel as u32) as usize] as f64
                };
                let top = at(x0, y0) * (1.0 - wx) + at(x1, y0) * wx;
                let bottom = at(x0, y1) * (1.0 - wx) + at(x1, y1) * wx;
                out[to + channel] =
                    (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out.iter().map(|b| *b as u64).sum()
}

#[test]
#[ignore = "微基准：用 --ignored --nocapture 手动跑"]
fn bilinear_4k_to_512_breakdown() {
    let (sw, sh, dw, dh) = (3840u32, 2160u32, 512u32, 512u32);
    let src = pattern(sw, sh);
    let runs = 5;

    let mut full = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let out = resample_rgba(&src, sw, sh, dw, dh, ResampleFilter::Bilinear).expect("应当成功");
        full.push(t.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(out);
    }
    let mut coords = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let acc = coords_only(sw, sh, dw, dh);
        coords.push(t.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(acc);
    }
    let mut sample = Vec::new();
    for _ in 0..runs {
        let t = Instant::now();
        let acc = sample_only(&src, sw, sh, dw, dh);
        sample.push(t.elapsed().as_secs_f64() * 1000.0);
        std::hint::black_box(acc);
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let (f, c, s) = (med(&mut full), med(&mut coords), med(&mut sample));
    println!();
    println!("  ★ {sw}x{sh} → {dw}x{dh}（{runs} 次取中位）★");
    println!("  · ① 完整 resample_rgba      {f:8.1} ms");
    println!(
        "  · ② 只算坐标（不读不写）    {c:8.1} ms（{:.1}%）",
        100.0 * c / f
    );
    println!(
        "  · ③ 只采样＋写（坐标已算）  {s:8.1} ms（{:.1}%）",
        100.0 * s / f
    );
    println!(
        "  · ②＋③ 合计                 {:8.1} ms（{:.1}%）",
        c + s,
        100.0 * (c + s) / f
    );
    println!(
        "  · 每个输出像素              {:.0} ns",
        1e6 * f / f64::from(dw * dh)
    );
}
