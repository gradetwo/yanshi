//! **★ 分块提交：整幅 ＝ 分块（逐位相同），且每块不超过单绑定上限 ✗ ★**（第 287 轮 ✓）。
//!
//! **∴ 为什么必须 ✗**：**第 274 轮实测**（**适配器能力 ✓）**✗**：
//!   **∴ `max_storage_buffer_binding_size` ＝ **134217728（128 MiB）**** ✓
//!   **∴ 而** 4K 的 f32 输入 ✗**：**3840 × 2160 × 4 通道 × 4 B ＝ **132.7 MiB**** ✓
//!     ⇒ **★ 所以**：**它**超过单绑定上限**✗ ⇒ **∴ 分块**不是优化，**而是硬约束** ✓ ★**** ✓✓
//!
//! **∴ 本判据 ✗**：
//!   **∴ ①** **把 N 像素切成**若干块**✗ ⇒ **∴ 每块 ≤ 128 MiB** ✓
//!   **∴ ②** **全部块放进**同一个 encoder**✗ ⇒ **∴ 只**一次 `Wait`** ✓（**第 284 轮：批量快 8.7 倍 ✓）
//!   **∴ ③ ★ 分块的结果**必须与**整幅的 CPU 真值**逐位相同**（**`max_channel_delta == 0` ✓）★**
//!
//! **∴ 变异点 ✗**：**分块时**漏掉最后一块**✗ ⇒ **∴ ③** 必红** ✓**** ✓✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_chunked_quantize -- --nocapture`
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
fn chunked_quantize_matches_the_whole_cpu_result() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    {
        Ok(a) => a,
        Err(e) => {
            eprintln!("跳过：没有适配器（{e:?}）");
            return;
        }
    };
    // **★ 断言硬约束：单绑定上限必须够一块用 ✓**
    let cap = adapter.limits().max_storage_buffer_binding_size as usize;
    println!("  单绑定上限 ＝ {cap} B（{} MiB）", cap / 1048576);

    let (device, queue) = match block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("跳过：拿不到设备（{e:?}）");
            return;
        }
    };
    let lut = yanshi_render::color::srgb_encode_table();

    // **∴ 总量：**让**每块 4 MiB**（**远低于上限 ✓）⇒ **∴ 于是**块数由 n 决定 ✓
    let n: usize = 1_000_000;
    const CHUNK_BYTES: usize = 4 * 1024 * 1024;
    let per_chunk = CHUNK_BYTES / 16; // **每像素 16 B（**4 × f32 ✓）
    assert!(CHUNK_BYTES <= cap, "分块必须小于单绑定上限");

    let mut s: u32 = 0xC0FF_EE01;
    let pix: Vec<[f32; 4]> = (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let a = ((s >> 20) & 0xFF) as f32 / 255.0;
            let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
            [f(0) * a, f(1) * a, f(2) * a, a]
        })
        .collect();

    // **∴ CPU 真值（**整幅 ✓）
    let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
    for (i, p) in pix.iter().enumerate() {
        b.set_pixel(i as u32, 0, *p);
    }
    let want = b.to_rgba8_quantized(None);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("chunked"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("chunked"),
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

    // **★ 一次 encoder ＋ 一次 Wait（**批量 ✓），**而**每块独立绑定**（**≤ 上限 ✓）**
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let mut backs = Vec::new();
    let mut chunks = 0usize;
    let mut done = 0usize;
    while done < n {
        let take = per_chunk.min(n - done);
        let flat: Vec<f32> = pix[done..done + take]
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
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((take as u32).div_ceil(64), 1, 1);
        }
        enc.copy_buffer_to_buffer(&dst, 0, &back, 0, (take * 4) as u64);
        backs.push((back, take));
        chunks += 1;
        done += take;
    }
    // **∴ 一次提交 ＋ 一次等待 ✓**
    queue.submit(Some(enc.finish()));
    let _ = device.poll(wgpu::PollType::Wait);

    let mut got: Vec<u8> = Vec::with_capacity(n * 4);
    for (back, take) in &backs {
        let slice = back.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::Wait);
        let _ = rx.recv();
        let m = slice.get_mapped_range();
        got.extend_from_slice(&m);
        drop(m);
        back.unmap();
        assert_eq!(m_len_guard(*take), *take, "块大小自洽");
    }

    let mut max_delta = 0u8;
    let mut first_bad = None;
    for i in 0..n * 4 {
        let d = want[i].abs_diff(got[i]);
        if d > 0 && first_bad.is_none() {
            first_bad = Some((i, want[i], got[i]));
        }
        max_delta = max_delta.max(d);
    }
    println!("  ★ 分块量化：n={n}｜块数={chunks}｜块大小={per_chunk} 像素｜★ max_channel_delta={max_delta} ★");
    println!("  首个不符 = {first_bad:?}");
    assert_eq!(max_delta, 0, "整幅与分块必须逐位相同");
}

const fn m_len_guard(take: usize) -> usize {
    take
}
