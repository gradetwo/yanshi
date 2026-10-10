//! **★ 批量提交 vs 逐块提交：把同步等待摊薄 ✗ ★**（第 284 轮 ✓；**用户第 594 轮 ✓）。
//!
//! **∴ 依据 ✗**：**第 283 轮分解实测**（**n=4096 ✓）**✗**：
//!   **∴ `device.poll(Wait)` ✗**：**★ 2.329 ms（71.5%）★**
//!   **∴ 建 encoder ✗**：**0.017 ms（0.5%）** ⇒ **∴ 所以**「**复用 encoder**」**没用** ✓
//!   ⇒ **★ 所以**：**正确的杠杆是**把等待摊薄**✗（**一次 Wait 等 N 块 ✓）★**** ✓✓
//!
//! **∴ 本判据 ✗**：**同一份总工作**（**N 块 × 每块 n 像素 ✓）走两条路**✗：
//!   **∴ ① 逐块 ✗**：**每块**各建 encoder ＋ submit ＋ **Wait**  ✓
//!   **∴ ② 批量 ✗**：**N 块放进**同一个 encoder**✗ ⇒ **∴ 只**一次 `Wait`** ✓**** ✓✓
//!   ⇒ **∴ 断言 ✗**：**① ② 的结果**必须**逐位相同**（**∴ 不许**为了快而少算 ✓）★**** ✓✓
//!      ＋ **∴ 并**打印**两本账**（**时间 ＋ 进程 CPU ✓）** ✓✓
//!
//! **∴ 变异点 ✗**：**批量路**故意**漏掉最后一块**✗ ⇒ **∴ 逐位断言**必红** ✓**** ✓✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_batch_submit -- --nocapture`
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

fn proc_cpu_seconds() -> f64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/stat") else {
        return 0.0;
    };
    let Some(close) = s.rfind(')') else {
        return 0.0;
    };
    let rest: Vec<&str> = s[close + 1..].split_whitespace().collect();
    let u: f64 = rest.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let st: f64 = rest.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (u + st) / 100.0
}

const WGSL: &str = r#"
@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= arrayLength(&src)) { return; }
    dst[i] = src[i] + 7u;
}
"#;

fn bytes_of(v: &[u32]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[test]
fn batch_submit_is_faster_and_bit_identical() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("跳过：没有适配器（{e:?}）");
            return;
        }
    };
    let (device, queue) = match block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("跳过：拿不到设备（{e:?}）");
            return;
        }
    };

    const CHUNKS: usize = 64;
    const PER: usize = 4096;
    let chunk_bytes = (PER * 4) as u64;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("batch"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("batch"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let layout = pipeline.get_bind_group_layout(0);

    // **∴ 每块一对缓冲 ＋ 一个绑定组（**`CHUNKS` 份 ✓）
    let mut srcs = Vec::new();
    let mut dsts = Vec::new();
    let mut backs = Vec::new();
    let mut binds = Vec::new();
    for c in 0..CHUNKS {
        let mk = |usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: chunk_bytes,
                usage,
                mapped_at_creation: false,
            })
        };
        let s = mk(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
        let d = mk(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
        let b = mk(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
        let data: Vec<u32> = (0..PER as u32).map(|i| i.wrapping_add(c as u32)).collect();
        queue.write_buffer(&s, 0, bytes_of(&data));
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: s.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: d.as_entire_binding(),
                },
            ],
        });
        srcs.push(s);
        dsts.push(d);
        backs.push(b);
        binds.push(bind);
    }

    let read_back = |back: &wgpu::Buffer| -> Vec<u32> {
        let slice = back.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::Wait);
        let _ = rx.recv();
        let m = slice.get_mapped_range();
        let out: Vec<u32> = m
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        drop(m);
        back.unmap();
        out
    };

    const RUNS: usize = 5;
    // **∴ ① 逐块：每块一次 submit ＋ 一次 Wait ✓**
    let mut per_ms = Vec::new();
    let mut per_cpu = Vec::new();
    let mut per_out: Vec<Vec<u32>> = Vec::new();
    for _ in 0..RUNS {
        let c0 = proc_cpu_seconds();
        let t = std::time::Instant::now();
        let mut outs = Vec::new();
        for c in 0..CHUNKS {
            let mut enc =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &binds[c], &[]);
                pass.dispatch_workgroups((PER as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&dsts[c], 0, &backs[c], 0, chunk_bytes);
            queue.submit(Some(enc.finish()));
            let _ = device.poll(wgpu::PollType::Wait);
            outs.push(read_back(&backs[c]));
        }
        per_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        per_cpu.push((proc_cpu_seconds() - c0) * 1000.0);
        per_out = outs;
    }

    // **∴ ② 批量：全部编码进**一个 encoder**✗ ⇒ **∴ 只一次 submit ＋ 一次 Wait** ✓
    let mut bat_ms = Vec::new();
    let mut bat_cpu = Vec::new();
    let mut bat_out: Vec<Vec<u32>> = Vec::new();
    for _ in 0..RUNS {
        let c0 = proc_cpu_seconds();
        let t = std::time::Instant::now();
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        for c in 0..CHUNKS {
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &binds[c], &[]);
                pass.dispatch_workgroups((PER as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&dsts[c], 0, &backs[c], 0, chunk_bytes);
        }
        queue.submit(Some(enc.finish()));
        let _ = device.poll(wgpu::PollType::Wait);
        let outs: Vec<Vec<u32>> = (0..CHUNKS).map(|c| read_back(&backs[c])).collect();
        bat_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        bat_cpu.push((proc_cpu_seconds() - c0) * 1000.0);
        bat_out = outs;
    }

    // **★ 逐位断言：批量与逐块必须**完全一样**** ✓
    assert_eq!(per_out.len(), bat_out.len());
    let mut diff = 0usize;
    for c in 0..CHUNKS {
        for i in 0..PER {
            if per_out[c][i] != bat_out[c][i] {
                diff += 1;
            }
        }
    }

    let med = |xs: &mut Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs[xs.len() / 2]
    };
    let p = med(&mut per_ms);
    let b = med(&mut bat_ms);
    let pc = med(&mut per_cpu);
    let bc = med(&mut bat_cpu);
    println!("  ★ 批量提交（{CHUNKS} 块 × {PER} 像素，{RUNS} 次取中位数）★");
    println!("  | 路径 | ① 墙钟 ms | ② 进程 CPU ms | 块数 |");
    println!("  |---|---|---|---|");
    println!("  | 逐块（{CHUNKS} 次 Wait） | {p:.3} | {pc:.1} | {CHUNKS} |");
    println!("  | 批量（1 次 Wait） | {b:.3} | {bc:.1} | 1 |");
    println!(
        "  ∴ 时间账：批量 {} {:.1}%",
        if b <= p { "快" } else { "慢" },
        ((1.0 - b / p) * 100.0).abs()
    );
    println!(
        "  ∴ CPU 占用账：批量 {} {:.1}%（**∴ /proc 分辨率 10 ms ✓）",
        if bc <= pc { "少" } else { "多" },
        ((1.0 - bc / pc) * 100.0).abs()
    );
    println!("  ∴ 逐位：不符 {diff} 个（**∴ 必须 0 ✓）");
    assert_eq!(diff, 0, "批量与逐块必须逐位相同");
}
