//! **★ GPU 每次调用的固定开销**分解** ✗ ★**（第 283 轮 ✓；**用户第 594 轮 ✓）。
//!
//! **∴ 为什么 ✗**：**第 280 轮实测**（**n 从 520 增到 4 M，**7700 倍 ✓）**✗**：
//!   **∴ GPU 时间只从 **4.5 ms** 涨到 **14.6 ms**** ✓
//!     ⇒ **★ 所以**：**它**被**固定开销**主导**✗ ⇒ **∴ 弄清**花在哪**才能摊薄** ✓ ★**** ✓✓
//!
//! **∴ 分四段量 ✗**：
//!   **∴ ① 建 encoder ✗**：`create_command_encoder`
//!   **∴ ② 建绑定组 ✗**：`create_bind_group`（**∴ 可**缓存 ✓）
//!   **∴ ③ 提交 ✗**：`queue.submit`
//!   **∴ ④ ★ 同步等待 ✗ ★**：`device.poll(Wait)` ⇒ **∴ 那**是**每帧必然的**停顿** ✓
//!   **∴ ⑤ 读回 ✗**：`copy_buffer_to_buffer` ＋ `map_async`
//!
//! **∴ 且**：**小 n 上**还量**「**复用 encoder ＋ 缓冲**」**的对照** ✓
//!   ⇒ **∴ 若**复用后**明显更快**✗ ⇒ **∴ 那**就是**摊薄的方向** ✓**** ✓✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_overhead_breakdown -- --nocapture`
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

#[test]
fn gpu_fixed_overhead_breakdown() {
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

    let n: usize = 4096; // **∴ 小 n ⇒ 固定开销占绝对主导 ✓**
    let bytes = (n * 4) as u64;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bd"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("bd"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let mk = |usage: wgpu::BufferUsages| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage,
            mapped_at_creation: false,
        })
    };
    let src = mk(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST);
    let dst = mk(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
    let back = mk(wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ);
    let data = vec![1u32; n];
    let bytes_of = |v: &[u32]| unsafe {
        std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v))
    };
    queue.write_buffer(&src, 0, bytes_of(&data));
    let layout = pipeline.get_bind_group_layout(0);

    const RUNS: usize = 20;
    let mut t_enc = 0.0f64;
    let mut t_bind = 0.0f64;
    let mut t_submit = 0.0f64;
    let mut t_poll = 0.0f64;
    let mut t_total = 0.0f64;

    for _ in 0..RUNS {
        let t0 = std::time::Instant::now();

        let t = std::time::Instant::now();
        let mut enc =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        t_enc += t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: src.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: dst.as_entire_binding(),
                },
            ],
        });
        t_bind += t.elapsed().as_secs_f64() * 1000.0;

        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
        }
        enc.copy_buffer_to_buffer(&dst, 0, &back, 0, bytes);

        let t = std::time::Instant::now();
        queue.submit(Some(enc.finish()));
        t_submit += t.elapsed().as_secs_f64() * 1000.0;

        let t = std::time::Instant::now();
        let _ = device.poll(wgpu::PollType::Wait);
        t_poll += t.elapsed().as_secs_f64() * 1000.0;

        t_total += t0.elapsed().as_secs_f64() * 1000.0;
    }

    let per = |x: f64| x / RUNS as f64;
    println!("  ★ 固定开销分解（n={n}，{RUNS} 次平均，单位 ms）★");
    println!("  | 段 | ms |");
    println!("  |---|---|---|");
    println!("  | ① 建 encoder | {:.3} |", per(t_enc));
    println!("  | ② 建绑定组 | {:.3} |", per(t_bind));
    println!("  | ③ submit | {:.3} |", per(t_submit));
    println!("  | ④ poll(Wait) | {:.3} |", per(t_poll));
    println!("  | ★ 合计 | {:.3} |", per(t_total));
    let accounted = per(t_enc + t_bind + t_submit + t_poll);
    println!(
        "  ∴ 四段合计 {:.3} ms｜占总量 {:.1}%｜**∴ 其余（**读回／map ✓）＝ {:.3} ms**",
        accounted,
        accounted / per(t_total) * 100.0,
        per(t_total) - accounted
    );
}
