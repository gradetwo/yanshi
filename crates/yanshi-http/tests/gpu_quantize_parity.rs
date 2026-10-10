//! **★ GPU 量化必须与 CPU 真值**逐位相同** ✗ ★**（第 277 轮 ✓；**目标第 7 条 ✓）。
//!
//! **∴ 真值 ✗**：`yanshi_render` 的 `Buffer::to_rgba8_quantized`（**唯一逐像素实现 ✓）
//! **∴ 五步（**第 273 轮读出 ✓）✗**：
//!   ① `quantize_f16`（**已逐位证明 ✓，第 276 轮 ✓）
//!   ② `a = clamp(x[3], 0, 1)`；**`a <= 0` ⇒ `[0,0,0,0]`**
//!   ③ `encode_with_table`（**★ 纯查表 ✗ —— **无插值** ✓）⇒ **∴ 上传 LUT 即可一致** ✓
//!   ④ `byte(v) = floor(encode * 255 + 0.5).clamp(0,255)`
//!   ⑤ `out = [byte(x0/a), byte(x1/a), byte(x2/a), floor(a*255+0.5).clamp(0,255)]`
//!
//! **∴ 变异点 ✗**：**把 `floor(x+0.5)` 改成 `round`**✗ ⇒ **∴ 在**负数或 .5 边界**上会分叉** ✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_quantize_parity`
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
    if (exponent == 0xFF) {
        var m = 0u;
        if (mantissa != 0u) { m = 0x0200u; }
        return sign | 0x7C00u | m;
    }
    let unbiased = exponent - 127;
    if (unbiased > 15) { return sign | 0x7C00u; }
    if (unbiased >= -14) {
        var half_exp = u32(unbiased + 15);
        var half_mant = mantissa >> 13u;
        let round_bit = (mantissa >> 12u) & 1u;
        let sticky = (mantissa & 0x0FFFu) != 0u;
        if (round_bit == 1u && (sticky || (half_mant & 1u) == 1u)) {
            half_mant = half_mant + 1u;
            if (half_mant == 0x0400u) {
                half_mant = 0u;
                half_exp = half_exp + 1u;
                if (half_exp >= 0x1Fu) { return sign | 0x7C00u; }
            }
        }
        return sign | (half_exp << 10u) | half_mant;
    }
    if (unbiased < -25) { return sign; }
    let shift = u32(-unbiased - 14);
    let total_shift = 13u + shift;
    if (total_shift >= 32u) { return sign; }
    let full = mantissa | 0x00800000u;
    var half_mant = full >> total_shift;
    let remainder = full & ((1u << total_shift) - 1u);
    let halfway = 1u << (total_shift - 1u);
    if (remainder > halfway || (remainder == halfway && (half_mant & 1u) == 1u)) {
        half_mant = half_mant + 1u;
    }
    return sign | half_mant;
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
            let mant10 = (mant << (10u - k)) & 0x3FFu;
            out = sign | ((k + 103u) << 23u) | (mant10 << 13u);
        }
    } else if (exp == 0x1Fu) {
        out = sign | 0x7F800000u | (mant << 13u);
    } else {
        out = sign | ((exp + 112u) << 23u) | (mant << 13u);
    }
    return bitcast<f32>(out);
}

fn q16(v: f32) -> f32 { return f16_bits_to_f32(f32_to_f16_bits(v)); }

// **∴ 纯查表 ✗**（**∴ 与 `color.rs::encode_with_table` 同形 ✓）
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
    let r = byte_of(x0 / a);
    let g = byte_of(x1 / a);
    let bl = byte_of(x2 / a);
    let al = u32(clamp(floor(a * 255.0 + 0.5), 0.0, 255.0));
    dst[i] = r | (g << 8u) | (bl << 16u) | (al << 24u);
}
"#;

fn as_bytes<T>(v: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[test]
fn gpu_quantize_matches_cpu_bit_for_bit() {
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

    // **∴ 输入 ✗**：**覆盖 alpha=0／负值／超范围／极小 alpha ＋ 伪随机** ✓
    let mut pix: Vec<[f32; 4]> = vec![
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, 0.0],
        [0.5, 0.5, 0.5, 1.0],
        [0.0, 0.0, 0.0, 0.5],
        [-0.1, 0.2, 1.5, 1.0],
        [1e-9, 1e-8, 1e-7, 1e-6],
        [0.25, 0.5, 0.75, 0.25],
        [0.9, 0.9, 0.9, 0.1],
    ];
    let mut s: u32 = 0xDEAD_BEEF;
    for _ in 0..512 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
        let a = ((s >> 20) & 0xFF) as f32 / 255.0;
        pix.push([f(0) * a, f(1) * a, f(2) * a, a]);
    }
    let n = pix.len();

    // **∴ 真值 ✗**：**CPU 的 `to_rgba8_quantized`** ✓
    let mut buf = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
    for (i, p) in pix.iter().enumerate() {
        buf.set_pixel(i as u32, 0, *p);
    }
    let want: Vec<u8> = buf.to_rgba8_quantized(None);

    // **∴ LUT ✗**：**上传 CPU 的同一张表** ✓
    let lut = yanshi_render::color::srgb_encode_table();
    let flat: Vec<f32> = pix.iter().flat_map(|p| p.iter().copied()).collect();

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("quantize"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("quantize"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

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
    // **∴ uniform 要对齐 16 B ✓**
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
    enc.copy_buffer_to_buffer(&dst_buf, 0, &back, 0, (n * 4) as u64);
    queue.submit(Some(enc.finish()));

    let slice = back.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait);
    let _ = rx.recv();
    let mapped = slice.get_mapped_range();
    let got: Vec<u8> = mapped.to_vec();
    drop(mapped);
    back.unmap();

    let mut max_delta = 0u8;
    let mut first_bad = None;
    for i in 0..n {
        for c in 0..4 {
            let d = want[i * 4 + c].abs_diff(got[i * 4 + c]);
            if d > 0 && first_bad.is_none() {
                first_bad = Some((i, c, pix[i], want[i * 4 + c], got[i * 4 + c]));
            }
            max_delta = max_delta.max(d);
        }
    }
    println!("  GPU 量化：n={n}｜★ max_channel_delta={max_delta} ★｜首个不符={first_bad:?}");
    assert_eq!(max_delta, 0, "量化必须与 CPU 真值逐位相同");
}
