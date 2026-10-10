//! **★ 4K 规模的两本账：GPU 侧**按块**（不超 128 MiB 上限），CPU 侧整幅 ✗ ★**（第 289 轮 ✓）。
//!
//! **∴ 为什么单独一条 ✗**：**第 288 轮实测**（**如实 ✓）✗**：
//!   **∴ 把 8 294 400 像素（**132.7 MiB 输入 ✓）**整幅塞进一个绑定**✗
//!     ⇒ **∴ wgpu 报 **Validation Error**** ✓（**∴ 正是**第 274 轮量到的 128 MiB 上限** ✓）**
//!   **⇒ ★ 所以 ✗ ★**：**4K 那一档**必须**按块**✗ ⇒ **∴ CPU 侧**整幅没问题**（**在主存里 ✓）★**** ✓✓
//!
//! **∴ 判据 ✗**：
//!   **∴ ①** **4K 全幅：GPU（**分块批量 ✓）**结果**必须**与 CPU 真值**逐位相同** ✓
//!   **∴ ②** **两本账**（**时间 ＋ 进程 CPU ✓）**都要报** ✓
//!   **∴ ③** **并与阶段一基线对比 ✗**：**4K 冷首帧的**量化项 **489–490 ms** ✓（**第 215／216 轮 ✓）** ✓✓
//!
//! **∴ 变异点 ✗**：**分块时漏最后一块**✗ ⇒ **∴ ①** 必红** ✓**** ✓✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_4k_two_accounts -- --nocapture`
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
    if (exponent == 0xFF) { var m = 0u; if (mantissa != 0u) { m = 0x0200u; } return sign | 0x7C00u | m; }
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
        else { let k = firstLeadingBit(mant); let m10 = (mant << (10u - k)) & 0x3FFu;
               out = sign | ((k + 103u) << 23u) | (m10 << 13u); }
    } else if (exp == 0x1Fu) { out = sign | 0x7F800000u | (mant << 13u); }
    else { out = sign | ((exp + 112u) << 23u) | (mant << 13u); }
    return bitcast<f32>(out);
}
fn q16(v: f32) -> f32 { return f16_bits_to_f32(f32_to_f16_bits(v)); }
fn encode(v: f32) -> f32 {
    let c = clamp(v, 0.0, 1.0);
    let idx = u32(round(c * f32(params.lut_len - 1u)));
    return lut[min(idx, params.lut_len - 1u)];
}
fn byte_of(v: f32) -> u32 { return u32(clamp(floor(encode(v) * 255.0 + 0.5), 0.0, 255.0)); }
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.pixels) { return; }
    let b = i * 4u;
    let x0 = q16(src[b + 0u]); let x1 = q16(src[b + 1u]);
    let x2 = q16(src[b + 2u]); let x3 = q16(src[b + 3u]);
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
fn four_k_two_accounts_with_chunked_gpu() {
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

    // **∴ 4K：**3840 × 2160 ✓
    let n: usize = 3840 * 2160;
    const PER_CHUNK: usize = 262_144; // **∴ 每块 4 MiB 输入 ✓（**远低于 128 MiB ✓）
    let chunks = n.div_ceil(PER_CHUNK);
    // **∴ 输入在内存里只有**一份**（**∴ 逐块切片，不复制整幅 f32 ✓）
    let mut s: u32 = 0x0BAD_F00D;
    let pix: Vec<[f32; 4]> = (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let a = ((s >> 20) & 0xFF) as f32 / 255.0;
            let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
            [f(0) * a, f(1) * a, f(2) * a, a]
        })
        .collect();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("4k"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("4k"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let layout = pipeline.get_bind_group_layout(0);
    let lut_buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (lut.len() * 4) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&lut_buf, 0, as_bytes(lut));

    // **∴ 预先建好每块的缓冲 ＋ 绑定组（**∴ 只测**提交与计算**，**不把建对象算进去 ✓）
    struct Chunk {
        back: wgpu::Buffer,
        bind: wgpu::BindGroup,
        dst: wgpu::Buffer,
        take: usize,
    }
    let mut ck: Vec<Chunk> = Vec::with_capacity(chunks);
    for c in 0..chunks {
        let start = c * PER_CHUNK;
        let take = PER_CHUNK.min(n - start);
        let flat: Vec<f32> = pix[start..start + take]
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect();
        let src = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (flat.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (take * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let params: [u32; 4] = [take as u32, lut.len() as u32, 0, 0];
        let pb = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let back = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (take * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        queue.write_buffer(&src, 0, as_bytes(&flat));
        queue.write_buffer(&pb, 0, as_bytes(&params));
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: lut_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: pb.as_entire_binding(),
                },
            ],
        });
        ck.push(Chunk {
            back,
            bind,
            dst,
            take,
        });
    }

    const RUNS: usize = 3;
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
        for c in &ck {
            {
                let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: None,
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, &c.bind, &[]);
                pass.dispatch_workgroups((c.take as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&c.dst, 0, &c.back, 0, (c.take * 4) as u64);
        }
        queue.submit(Some(enc.finish()));
        let _ = device.poll(wgpu::PollType::Wait);
        gpu_ms.push(t.elapsed().as_secs_f64() * 1000.0);
        gpu_used.push((proc_cpu_seconds() - c0) * 1000.0);
    }

    // **∴ CPU 真值（**整幅 ✓）**＋ 逐位对比** ✓
    let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
    for (i, p) in pix.iter().enumerate() {
        b.set_pixel(i as u32, 0, *p);
    }
    let want = b.to_rgba8_quantized(None);

    let mut got: Vec<u8> = Vec::with_capacity(n * 4);
    for c in &ck {
        let slice = c.back.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::Wait);
        let _ = rx.recv();
        let m = slice.get_mapped_range();
        got.extend_from_slice(&m);
        drop(m);
        c.back.unmap();
    }
    let mut max_delta = 0u8;
    for i in 0..n * 4 {
        max_delta = max_delta.max(want[i].abs_diff(got[i]));
    }

    let med = |xs: &mut Vec<f64>| {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        xs[xs.len() / 2]
    };
    let c_ms = med(&mut cpu_ms);
    let g_ms = med(&mut gpu_ms);
    let c_u = med(&mut cpu_used);
    let g_u = med(&mut gpu_used);
    println!(
        "  ★ 4K 两本账（{n} 像素 ＝ {} MiB 输入；块数 {chunks}；{RUNS} 次取中位数）★",
        n * 16 / 1048576
    );
    println!("  | 路径 | ① 墙钟 ms | ② 进程 CPU ms |");
    println!("  |---|---|---|");
    println!("  | CPU（整幅） | {c_ms:.1} | {c_u:.1} |");
    println!("  | GPU（分块批量） | {g_ms:.1} | {g_u:.1} |");
    println!(
        "  ∴ 时间账：GPU {} {:.1}%",
        if g_ms <= c_ms { "快" } else { "慢" },
        ((1.0 - g_ms / c_ms) * 100.0).abs()
    );
    println!(
        "  ∴ CPU 占用账：GPU {} {:.1}%（**∴ /proc 分辨率 10 ms ✓）",
        if g_u <= c_u { "少" } else { "多" },
        ((1.0 - g_u / c_u) * 100.0).abs()
    );
    println!("  ∴ 逐位：max_channel_delta = {max_delta}（**∴ 必须 0 ✓）");
    println!("  ∴ 阶段一基线（**第 215／216 轮 ✓）：4K 冷首帧的**量化项 **489–490 ms** ✓");
    assert_eq!(max_delta, 0, "4K 上分块 GPU 必须与 CPU 真值逐位相同");
}
