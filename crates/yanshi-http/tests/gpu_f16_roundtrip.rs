//! **★ GPU 手写 `f16` 往返必须与 CPU 的 `half::quantize_f16` **逐位相同**** ✗ ★**（第 275 轮 ✓）。
//!
//! **∴ 为什么必须手写 ✗**：**实测**（**第 274 轮 ✓）**✗**：
//!   **∴ 本机适配器 `SHADER_F16 = false`** ✓ ⇒ **∴ WGSL 里**没有原生 `f16`** ✓
//!     ⇒ **∴ 所以**：**必须**用位操作复现** ✓（**∴ 而**它是**量化路径里最难移植的一段** ✓）** ✓✓
//!
//! **∴ 判据 ✗**：**对一大批输入**（**含**边界 ✓）**✗**
//!   ⇒ **∴ GPU 的 `f16(f32 → f16 → f32)`**必须**与 `yanshi_render::half::quantize_f16`**逐位相同** ✓**
//!
//! **∴ 变异点 ✗**：**把**舍入改成**截断**（**去掉 `round_bit` 判断 ✓）⇒ **∴ 必红** ✓
//!
//! `cargo test -p yanshi-http --features gpu --test gpu_f16_roundtrip`
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

/// **∴ 与 `half.rs::f32_to_f16_bits` **逐字对应**** ✓（**∴ 但**用**位运算**写成 WGSL 可编译的形式 ✓）。
/// **∴ 注意 ✗**：**WGSL 没有 `u16`**✗ ⇒ **∴ 全程用 `u32`**，**只在最后取值** ✓**** ✓✓
const WGSL: &str = r#"
@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;

fn f32_to_f16_bits(value: f32) -> u32 {
    let bits = bitcast<u32>(value);
    let sign = (bits >> 16u) & 0x8000u;
    let exponent = i32((bits >> 23u) & 0xFFu);
    let mantissa = bits & 0x007FFFFFu;

    // **∴ NaN／Inf ✗**
    if (exponent == 0xFF) {
        var m = 0u;
        if (mantissa != 0u) { m = 0x0200u; }
        return sign | 0x7C00u | m;
    }
    let unbiased = exponent - 127;

    // **∴ 溢出 → Inf ✗**
    if (unbiased > 15) { return sign | 0x7C00u; }

    // **∴ 正规 ✗**：**round-to-nearest-even** ✓
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

    // **∴ 次正规 ✗**
    if (unbiased < -25) { return sign; }
    let shift = u32(-unbiased - 14);
    let total_shift = 13u + shift;
    if (total_shift >= 32u) { return sign; }
    let full = mantissa | 0x00800000u;      // **补上隐含的 1** ✓
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
        if (mant == 0u) {
            out = sign;                                  // **±0** ✓
        } else {
            // **∴ 次正规 → 正规化 ✗**
            var e = -1i;
            var m = mant;
            loop {
                if ((m & 0x400u) != 0u) { break; }
                m = m << 1u;
                e = e - 1i;
            }
            m = m & 0x3FFu;
            out = sign | (u32(i32(127i) + e + 1i) << 23u) | (m << 13u);
        }
    } else if (exp == 0x1Fu) {
        out = sign | 0x7F800000u | (mant << 13u);        // **Inf／NaN** ✓
    } else {
        out = sign | ((exp + 112u) << 23u) | (mant << 13u);
    }
    return bitcast<f32>(out);
}

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= arrayLength(&src)) { return; }
    let x = bitcast<f32>(src[i]);
    dst[i] = bitcast<u32>(f16_bits_to_f32(f32_to_f16_bits(x)));
}
"#;

fn gpu_roundtrip(device: &wgpu::Device, queue: &wgpu::Queue, input: &[f32]) -> Vec<f32> {
    let n = input.len();
    let bytes = (n * 4) as u64;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("f16-roundtrip"),
        source: wgpu::ShaderSource::Wgsl(WGSL.into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("f16-roundtrip"),
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
    queue.write_buffer(&src, 0, as_bytes(input));
    let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
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
    enc.copy_buffer_to_buffer(&dst, 0, &back, 0, bytes);
    queue.submit(Some(enc.finish()));
    let slice = back.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = device.poll(wgpu::PollType::Wait);
    let _ = rx.recv();
    let mapped = slice.get_mapped_range();
    let out: Vec<f32> = mapped
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    drop(mapped);
    back.unmap();
    out
}

fn as_bytes(v: &[f32]) -> &[u8] {
    // **∴ 不引入 `bytemuck` ✓**
    unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, std::mem::size_of_val(v)) }
}

#[test]
fn gpu_f16_roundtrip_matches_cpu_bit_for_bit() {
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

    // **∴ 输入集 ✗**：**边界 ＋ 随机 ＋ 次正规 ＋ 溢出** ✓
    let mut input: Vec<f32> = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.1,
        -0.1,
        65504.0,
        100_000.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e-9,
        6.0e-8,
        6.1e-5,
        6.0e-5,
        0.9999999,
        1.0000001,
        2.0,
        1024.0,
        2048.0,
        4096.0,
    ];
    // **∴ 伪随机 ✗**：**线性同余，**确定性 ✓（**∴ 不用随机库 ✓）
    let mut s: u32 = 0x1234_5678;
    for _ in 0..4096 {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        // **∴ 覆盖 [0,1) ＋ 负数 ＋ 大数 ＋ 小数 ✓**
        let u = (s >> 8) as f32 / (1u32 << 24) as f32;
        input.push(match s % 4 {
            0 => u,
            1 => -u,
            2 => u * 100.0,
            _ => u * 1e-7,
        });
    }

    let gpu = gpu_roundtrip(&device, &queue, &input);
    assert_eq!(gpu.len(), input.len());

    let mut max_delta = 0u32;
    let mut first_bad = None;
    for (i, x) in input.iter().enumerate() {
        let want = yanshi_render::half::quantize_f16(*x);
        let a = want.to_bits();
        let b = gpu[i].to_bits();
        // **∴ 逐位比 ✗**（**∴ 不是**比浮点近似 ✓）
        if a != b {
            if first_bad.is_none() {
                first_bad = Some((i, *x, want, gpu[i], a, b));
            }
            max_delta += 1;
        }
    }
    println!(
        "  GPU f16 往返：n={}｜不符数={max_delta}｜首个不符={:?}",
        input.len(),
        first_bad
    );
    assert_eq!(max_delta, 0, "f16 往返必须逐位一致（**∴ 不是**近似 ✓）");
}
