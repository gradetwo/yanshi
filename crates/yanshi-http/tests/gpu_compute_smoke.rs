//! **★ GPU 计算的**最小冒烟判据**：真的在 GPU 上跑一次，并与 CPU 逐位对比 ✗ ★**（第 271 轮 ✓）。
//!
//! **∴ 为什么需要它 ✗**：**第 270 轮**已证明能拿到 `Device`**✗
//!   ⇒ **∴ 但**拿到设备**不等于**能算** ✓
//!     ⇒ **★ 所以**：**必须**真的 dispatch 一次**✗ ⇒ **∴ 并**与 CPU 结果**逐位对比** ✓ ★**** ✓✓
//!
//! **∴ 判据 ✗**：
//!   **∴ ①** **GPU 能 dispatch 一个 compute shader** ✓
//!   **∴ ②** **结果与 CPU 的**逐位相同**（**`max_channel_delta == 0` ✓）** ✓✓
//!     ⇒ **∴ 那**是**目标第 7 条**「**逐位一致**只在 CPU 上承诺**」的**地基** ✓
//!       （**∴ 现在**这个算子**是**精确的**✗ ⇒ **∴ 所以**可以要求逐位 0 ✓）
//!
//! **∴ 变异点 ✗**：**把 shader 的运算改错**（**如 `x + 1` ✓）⇒ **∴ ②** 必红** ✓
//!
//! **∴ 不新增依赖 ✗**：**future**用手写 `block_on`** ✓（**∴ 与 `server.rs` 同法 ✓）** ✓✓
//!
//! **∴ 只在 `--features gpu` 下编译 ✓**：
//!   `cargo test -p yanshi-http --features gpu --test gpu_compute_smoke`
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
    // **∴ 一个**精确**算子 ✗**：**逐通道加 7**（**u32 无溢出风险 ✓）
    //   ⇒ **∴ 所以**：**GPU 与 CPU 的结果**必须**逐位相同** ✓**** ✓✓
    dst[i] = src[i] + 7u;
}
"#;

#[test]
fn gpu_compute_matches_cpu_bit_for_bit() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    {
        Ok(a) => a,
        Err(e) => {
            // **∴ 没有适配器**不是**产品缺陷**✗ ⇒ **∴ 明确跳过并说明** ✓
            eprintln!("跳过：本机没有可用适配器（{e:?}）");
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

    let n: usize = 4096;
    let src: Vec<u32> = (0..n as u32).map(|i| i.wrapping_mul(2654435761)).collect();
    let cpu: Vec<u32> = src.iter().map(|v| v.wrapping_add(7)).collect();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quantize-smoke"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("quantize-smoke"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let mk = |bytes: u64, read_only: bool| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: if read_only {
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST
            } else {
                wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC
            },
            mapped_at_creation: false,
        })
    };
    let bytes = (n * 4) as u64;
    let src_buf = mk(bytes, true);
    let dst_buf = mk(bytes, false);
    queue.write_buffer(&src_buf, 0, bytemuck_cast(&src));

    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: src_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: dst_buf.as_entire_binding(),
            },
        ],
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
    }
    enc.copy_buffer_to_buffer(&dst_buf, 0, &readback, 0, bytes);
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    // **∴ 等 map 完成 ✗**（**∴ 不睡固定时间 ✓）** ✓✓
    let _ = device.poll(wgpu::PollType::Wait);
    let _ = rx.recv();
    let mapped = slice.get_mapped_range();
    let got: Vec<u32> = mapped
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    drop(mapped);
    readback.unmap();

    // **★ ② 逐位对比 ✗ ★**
    let mut max_delta = 0u32;
    let mut first_bad = None;
    for (i, (a, b)) in cpu.iter().zip(got.iter()).enumerate() {
        let d = a.abs_diff(*b);
        if d > 0 && first_bad.is_none() {
            first_bad = Some((i, *a, *b));
        }
        max_delta = max_delta.max(d);
    }
    println!("  GPU 计算冒烟：n={n}｜max_channel_delta={max_delta}");
    assert_eq!(max_delta, 0, "精确算子必须逐位一致；首个不符 {first_bad:?}");
}

fn bytemuck_cast(v: &[u32]) -> &[u8] {
    // **∴ 不引入 `bytemuck` ✗**（**∴ 它**会**再多一个包 ✓）⇒ **∴ 手写安全转换** ✓**** ✓✓
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}
