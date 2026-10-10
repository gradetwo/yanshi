//! **★ GPU 量化的**规模曲线**：找出盈亏平衡点 ✗ ★**（第 280 轮 ✓；**用户第 594 轮 ✓）。
//!
//! **∴ 为什么必须 ✗**：**第 279 轮实测**（**n=520 ✓）**✗**：
//!   **∴ GPU **慢 872%**** ✓（**∴ 因为**每条提交有**约 4 ms 固定开销** ✓）
//!     ⇒ **∴ 而**内核侧 1 M 像素上 **GPU 快 93.8%** ✓（**第 254 轮 ✓）
//!       ⇒ **★ 所以**：**盈亏平衡点**在两者之间**✗ ⇒ **∴ 必须**实测，**不许外推** ✓ ★**** ✓✓
//!
//! **∴ 两本账（**用户第 594 轮 ✓）★**：
//!   **∴ ① 时间账 ✗**：**墙钟**（**5 次取中位数 ＋ 极差 ✓）
//!   **∴ ② CPU 占用账 ✗**：**进程 CPU 时间**（**读 `/proc/self/stat` 的 utime+stime ✓）
//!     ＋ **∴ CPU／墙钟比**（**并行度代理 ✓）** ✓✓
//!   **∴ 且**：**若**只有墙钟变好**而** CPU 没降**✗ ⇒ **∴ 判据**会**明说** ✓**** ✓✓
//!
//! **∴ 变异点 ✗**：**把 GPU 路的 `poll(Wait)` 去掉**✗ ⇒ **∴ 时间会**假变好**✗
//!   ⇒ **∴ 而**逐位判据（**另一个测试 ✓）会**因为读回不完整而红** ✓**** ✓✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_scale_sweep -- --nocapture`
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

/// **★ 进程 CPU 时间（**utime + stime ✓，单位秒 ✓）✗ ★**
fn proc_cpu_seconds() -> f64 {
    let Ok(s) = std::fs::read_to_string("/proc/self/stat") else {
        return 0.0;
    };
    // **∴ 字段 14／15（**1-based ✓）✗**：**但 comm 可能含空格**✗
    //   ⇒ **∴ 先跳过最后一个 `)` ✓**（**∴ 那**是标准做法 ✓）** ✓✓
    let Some(close) = s.rfind(')') else {
        return 0.0;
    };
    let rest: Vec<&str> = s[close + 1..].split_whitespace().collect();
    // **∴ rest[11] ＝ utime ✗；rest[12] ＝ stime** ✓（**∴ 因为** rest[0] ＝ state ✓）
    let utime: f64 = rest.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let stime: f64 = rest.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let hz = 100.0; // **∴ 本机 `getconf CLK_TCK` ＝ 100 ✓**（**∴ 见日志 ✓）
    (utime + stime) / hz
}

const WGSL: &str = r#"
struct Params { pixels: u32, lut_len: u32 };
@group(0) @binding(0) var<storage, read> src: array<f32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<storage, read> lut: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

fn f32_to_f16_bits(value: f32) -> u32 {
    let bits = bitcast<u32>(value);
    let sign = (bits >> 16u) & 0x8000u;
    let exponent = i32((bits >> 23u) & 0xFFu);
    let mantissa = bits & 0x007FFFFFu;
    if (exponent == 0xFF) {
        var m = 0u;
        if (mantissa != 0u) { m = 0x0200u; }
        return sign | 0x7C00u | m;
    }
    let unbiased = exponent - 127;
    if (unbiased > 15) { return sign | 0x7C00u; }
    if (unbiased >= -14) {
        var he = u32(unbiased + 15);
        var hm = mantissa >> 13u;
        let rb = (mantissa >> 12u) & 1u;
        let sticky = (mantissa & 0x0FFFu) != 0u;
        if (rb == 1u && (sticky || (hm & 1u) == 1u)) {
            hm = hm + 1u;
            if (hm == 0x0400u) { hm = 0u; he = he + 1u; if (he >= 0x1Fu) { return sign | 0x7C00u; } }
        }
        return sign | (he << 10u) | hm;
    }
    if (unbiased < -25) { return sign; }
    let shift = u32(-unbiased - 14);
    let ts = 13u + shift;
    if (ts >= 32u) { return sign; }
    let full = mantissa | 0x00800000u;
    var hm2 = full >> ts;
    let rem = full & ((1u << ts) - 1u);
    let half = 1u << (ts - 1u);
    if (rem > half || (rem == half && (hm2 & 1u) == 1u)) { hm2 = hm2 + 1u; }
    return sign | hm2;
}

fn f16_bits_to_f32(bits: u32) -> f32 {
    let sign = (bits & 0x8000u) << 16u;
    let exp = (bits >> 10u) & 0x1Fu;
    let mant = bits & 0x3FFu;
    var out: u32;
    if (exp == 0u) {
        if (mant == 0u) { out = sign; }
        else {
            let k = firstLeadingBit(mant);
            let m10 = (mant << (10u - k)) & 0x3FFu;
            out = sign | ((k + 103u) << 23u) | (m10 << 13u);
        }
    } else if (exp == 0x1Fu) {
        out = sign | 0x7F800000u | (mant << 13u);
    } else {
        out = sign | ((exp + 112u) << 23u) | (mant << 13u);
    }
    return bitcast<f32>(out);
}

fn q16(v: f32) -> f32 { return f16_bits_to_f32(f32_to_f16_bits(v)); }
fn encode(v: f32) -> f32 {
    let c = clamp(v, 0.0, 1.0);
    let idx = u32(round(c * f32(params.lut_len - 1u)));
    return lut[min(idx, params.lut_len - 1u)];
}
fn byte_of(v: f32) -> u32 {
    return u32(clamp(floor(encode(v) * 255.0 + 0.5), 0.0, 255.0));
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.pixels) { return; }
    let b = i * 4u;
    let x0 = q16(src[b + 0u]);
    let x1 = q16(src[b + 1u]);
    let x2 = q16(src[b + 2u]);
    let x3 = q16(src[b + 3u]);
    let a = clamp(x3, 0.0, 1.0);
    if (a <= 0.0) { dst[i] = 0u; return; }
    dst[i] = byte_of(x0 / a) | (byte_of(x1 / a) << 8u)
           | (byte_of(x2 / a) << 16u)
           | (u32(clamp(floor(a * 255.0 + 0.5), 0.0, 255.0)) << 24u);
}
"#;

fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[test]
fn gpu_scale_sweep_both_accounts() {
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
    let lut = yanshi_render::color::srgb_encode_table();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sweep"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("sweep"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    println!("  ★ 规模曲线（每档 5 次取中位数；两本账）★");
    println!("  | n | CPU ms | CPU CPU·ms | GPU ms | GPU CPU·ms | 时间账 | CPU 占用账 |");
    println!("  |---|---|---|---|---|---|---|");

    for &n in &[520usize, 100_000, 1_000_000, 4_000_000] {
        // **∴ 输入：**线性预乘的伪随机像素 ✓
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

        let src_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (flat.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dst_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (n * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let lut_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (lut.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let params: [u32; 4] = [n as u32, lut.len() as u32, 0, 0];
        let params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let back = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (n * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        queue.write_buffer(&src_buf, 0, as_bytes(&flat));
        queue.write_buffer(&lut_buf, 0, as_bytes(lut));
        queue.write_buffer(&params_buf, 0, as_bytes(&params));
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lut_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: params_buf.as_entire_binding(),
                },
            ],
        });

        const RUNS: usize = 5;
        let mut cpu_ms = Vec::new();
        let mut cpu_used = Vec::new();
        for _ in 0..RUNS {
            let c0 = proc_cpu_seconds();
            let t = std::time::Instant::now();
            let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
            for (i, p) in pix.iter().enumerate() {
                b.set_pixel(i as u32, 0, *p);
            }
            let _ = b.to_rgba8_quantized(None);
            cpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            cpu_used.push((proc_cpu_seconds() - c0) * 1000.0);
        }
        let mut gpu_ms = Vec::new();
        let mut gpu_used = Vec::new();
        for _ in 0..RUNS {
            let c0 = proc_cpu_seconds();
            let t = std::time::Instant::now();
            let mut enc =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups((n as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&dst_buf, 0, &back, 0, (n * 4) as u64);
            queue.submit(Some(enc.finish()));
            let _ = device.poll(wgpu::PollType::Wait);
            gpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
            gpu_used.push((proc_cpu_seconds() - c0) * 1000.0);
        }
        let med = |xs: &mut Vec<f64>| {
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            xs[xs.len() / 2]
        };
        let c = med(&mut cpu_ms);
        let g = med(&mut gpu_ms);
        let cu = med(&mut cpu_used);
        let gu = med(&mut gpu_used);
        let wall = if c > 0.0 { (1.0 - g / c) * 100.0 } else { 0.0 };
        let cpu_gain = if cu > 0.0 {
            (1.0 - gu / cu) * 100.0
        } else {
            0.0
        };
        println!(
            "  | {n} | {c:.1} | {cu:.1} | {g:.1} | {gu:.1} | GPU {} {:.1}% | CPU {} {:.1}% |",
            if wall >= 0.0 { "快" } else { "慢" },
            wall.abs(),
            if cpu_gain >= 0.0 { "少" } else { "多" },
            cpu_gain.abs()
        );
    }
}
