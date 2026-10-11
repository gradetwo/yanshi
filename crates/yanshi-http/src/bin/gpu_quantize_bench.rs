//! **★ GPU 量化微基准（macOS／Linux 通用）✗ ★**
//!
//! **∴ 为什么需要它 ✗**：`tool-macos-gpu-benefit.mjs` 测的是**端到端** `render_region` ✓
//!   ⇒ **∴ 而** L4 上的规模曲线测的是**纯量化**（f32→u8，不含合成 ✓）
//!     ⇒ **∴ 两种口径**不能直接比** ✓
//!       ⇒ **∴ 于是**：本 binary 只测量化一步 ✓，与 L4 的 `gpu_scale_sweep` 同口径 ✓
//!         ⇒ **∴ macOS 上也能画出**规模曲线** ✓ ★**** ✓✓
//!
//! **∴ 用法 ✗**：
//!   `cargo build --release -p yanshi-http --bin gpu_quantize_bench --features gpu`
//!   `./target/release/gpu_quantize_bench --sizes 1000,3000,10000,30000,100000,300000,1000000,4000000 --runs 5`
//!
//! **∴ 输出 ✗**：JSON 数组到 stdout，每项：
//!   `{ n, cpu_ms, gpu_ms, cpu_used_ms, gpu_used_ms, max_channel_delta, adapter_note }`
//!   （ms 均为 5 次中位数；`cpu_used_ms`／`gpu_used_ms` 为进程 CPU 时间中位数 ✓）
//!
//! **∴ CPU 时间口径 ✗**：`libc::getrusage(RUSAGE_SELF)` ✓
//!   ⇒ **∴ macOS／Linux 通用** ✓（**∴ 不像 `/proc/self/stat` 只在 Linux 有 ✓**）
//!
//! **∴ 硬约束 ✗**：`max_channel_delta` 必须为 0 ✓（**∴ GPU 与 CPU 逐位一致 ✓**）
//!   ⇒ **∴ 若**非 0 ⇒ **∴ 本项 `ok: false`** ✓**** ✓✓

#![cfg(feature = "gpu")]

use std::task::{Context, Poll};

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut fut = Box::pin(fut);
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
        std::thread::yield_now();
    }
}

/// **★ 进程 CPU 时间（秒）✗ ★**：`getrusage(RUSAGE_SELF)` 的 utime+stime ✓
/// ⇒ **∴ macOS／Linux 通用** ✓
fn proc_cpu_seconds() -> f64 {
    unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut usage) != 0 {
            return 0.0;
        }
        let u = usage.ru_utime;
        let s = usage.ru_stime;
        (u.tv_sec as f64 + u.tv_usec as f64 / 1e6) + (s.tv_sec as f64 + s.tv_usec as f64 / 1e6)
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

fn parse_args() -> (Vec<usize>, usize) {
    let args: Vec<String> = std::env::args().collect();
    let mut sizes = vec![
        1_000usize, 3_000, 10_000, 30_000, 100_000, 300_000, 1_000_000, 4_000_000,
    ];
    let mut runs = 5usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--sizes" => {
                i += 1;
                if i < args.len() {
                    sizes = args[i]
                        .split(',')
                        .filter_map(|s| s.trim().parse().ok())
                        .collect();
                }
            }
            "--runs" => {
                i += 1;
                if i < args.len() {
                    runs = args[i].parse().unwrap_or(5);
                }
            }
            _ => {}
        }
        i += 1;
    }
    (sizes, runs)
}

fn main() {
    let (sizes, runs) = parse_args();

    // **∴ LUT 与 L4 测试同一张表 ✓**（`yanshi_render::color::srgb_encode_table` ✓）
    let lut = yanshi_render::color::srgb_encode_table();
    let quantizer = match yanshi_gpu::Quantizer::new(lut) {
        Ok(q) => q,
        Err(e) => {
            eprintln!(
                "{{\"error\": \"GPU 不可用（不是产品问题）：{}\"}}",
                e.replace('"', "'")
            );
            std::process::exit(3);
        }
    };
    let adapter_note = quantizer.adapter_note().to_owned();

    let mut out = Vec::new();
    for &n in &sizes {
        // **∴ 输入：**与 L4 测试**相同的伪随机预乘像素** ✓
        let mut s: u32 = 0x1234_5678;
        let pix: Vec<[f32; 4]> = (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let a = ((s >> 20) & 0xFF) as f32 / 255.0;
                let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
                [f(0) * a, f(1) * a, f(2) * a, a]
            })
            .collect();
        let flat: Vec<f32> = pix.iter().flat_map(|p| p.iter().copied()).collect();

        // **∴ CPU 基准（**5 次 ✓）**
        let mut cpu_ms = Vec::with_capacity(runs);
        let mut cpu_used = Vec::with_capacity(runs);
        let mut cpu_out: Vec<u8> = Vec::new();
        for _ in 0..runs {
            let c0 = proc_cpu_seconds();
            let t = std::time::Instant::now();
            let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
            for (i, p) in pix.iter().enumerate() {
                b.set_pixel(i as u32, 0, *p);
            }
            cpu_out = b.to_rgba8_quantized(None);
            cpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            cpu_used.push((proc_cpu_seconds() - c0) * 1000.0);
        }

        // **∴ GPU（**5 次 ✓）**
        let mut gpu_ms = Vec::with_capacity(runs);
        let mut gpu_used = Vec::with_capacity(runs);
        let mut gpu_out: Vec<u8> = Vec::new();
        let mut gpu_err: Option<String> = None;
        for _ in 0..runs {
            let c0 = proc_cpu_seconds();
            let t = std::time::Instant::now();
            match block_on(quantizer.quantize_async(&flat, n, None)) {
                Ok(v) => {
                    gpu_out = v;
                    gpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                    gpu_used.push((proc_cpu_seconds() - c0) * 1000.0);
                }
                Err(e) => {
                    gpu_err = Some(e);
                    break;
                }
            }
        }

        // **∴ 逐位一致校验 ✓**
        let (max_delta, ok) = if let Some(e) = gpu_err {
            eprintln!(
                "{{\"warning\": \"n={} GPU 失败：{}\"}}",
                n,
                e.replace('"', "'")
            );
            (u32::MAX, false)
        } else if cpu_out.len() != gpu_out.len() {
            (u32::MAX, false)
        } else {
            let mut d = 0u32;
            for (a, b) in cpu_out.iter().zip(gpu_out.iter()) {
                let diff = (*a as i32 - *b as i32).unsigned_abs();
                if diff > d {
                    d = diff;
                }
            }
            (d, d == 0)
        };

        out.push(format!(
            "{{\"n\": {}, \"cpu_ms\": {:.3}, \"gpu_ms\": {:.3}, \
             \"cpu_used_ms\": {:.3}, \"gpu_used_ms\": {:.3}, \
             \"max_channel_delta\": {}, \"ok\": {}, \"adapter_note\": \"{}\"}}",
            n,
            median(cpu_ms),
            if gpu_ms.is_empty() {
                f64::NAN
            } else {
                median(gpu_ms)
            },
            median(cpu_used),
            if gpu_used.is_empty() {
                f64::NAN
            } else {
                median(gpu_used)
            },
            max_delta,
            ok,
            adapter_note
                .replace('"', "'")
                .chars()
                .take(120)
                .collect::<String>(),
        ));
    }
    println!("[{}]", out.join(","));
}
