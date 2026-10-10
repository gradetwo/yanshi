//! **★ GPU 分流策略：小输入走 CPU，大输入才走 GPU ✗ ★**（第 281 轮 ✓；**用户第 593／594 轮 ✓**）。
//!
//! **∴ 借鉴来源 ✗**：**Krita 的 `LoD`**（**按规模选路径 ✓；**GIMP 的 tile 粒度选择 ✓）。
//! **∴ 借鉴 ≠ 照搬 ✗**：
//!   **∴ Krita** 用它选**显示**层级**✗；**我们**用它选**计算路径** ✓
//!   **∴ 且**：**我们的真值**恒在 CPU**✗ ⇒ **∴ GPU 只是**可选加速** ✓
//!
//! **∴ 为什么必须有阈值 ✗**：**实测**（**第 280 轮 ✓）**✗**：
//!   **∴ n = 520 ✗**：**GPU **慢 1944.8%**** ✓（**∴ 固定开销 4.5 ms 主导 ✓）
//!   **∴ n = 100 000 ✗**：**GPU **快 87.4%**** ✓
//!   **∴ n = 4 000 000 ✗**：**GPU **快 99.2%**** ✓
//!   ⇒ **∴ 所以**：**盈亏平衡点在 **520 与 100 k** 之间** ✓
//!     ⇒ **★ 因此**：**低于阈值走 CPU**✗ ⇒ **∴ 否则**小图**会**变慢 19 倍** ✓ ★**** ✓✓
//!
//! **∴ 阈值取 **30 000**（**第 286 轮下调 ✓）✗**：
//!   **∴ 为什么下调 ✗**：**第 285 轮**把规模**细分到八档**✗ ⇒ **∴ 实测 ✗**：
//!     **∴ n = 3 000 ✗**：**GPU **慢 77.4%**** ✓
//!     **∴ n = 10 000 ✗**：**GPU **快 46.3%**** ✓
//!     **∴ n = 30 000 ✗**：**GPU **快 81.5%**** ✓
//!     ⇒ **∴ 所以**：**平衡点在 **3 k 与 10 k** 之间** ✓
//!       ⇒ **∴ 而**第 280 轮的 4 档**把平衡点**估成 100 k****✗（**∴ 档位太疏 ✓）✓
//!   **∴ 取 30 000 ✗**：**在 10 k 的**首个盈利档**之上**留 **3 倍余量** ✓
//!     ⇒ **∴ 于是**：**在**接近平衡点**时仍**保守**走 CPU** ✓**** ✓✓
//!   **∴ 代价（**两面 ✓）**：** 3 k–30 k 之间**放弃**本可获得的收益**（**∴ 那里 GPU 只快 0–81.5% ✓）
//!   **∴ 收益 ✗**：**绝不在小图上退步**（**3 k 处 GPU 慢 77.4% ⇒ **∴ 必须**挡住 ✓）** ✓✓
//!
//! **∴ 变异点 ✗**：**把阈值改成 0**（**一律走 GPU ✓）⇒ **∴ 判据**必红** ✓

/// **∴ 低于它就走 CPU ✗**（**单位：像素 ✓）。
pub const GPU_MIN_PIXELS: usize = 30_000;

/// **★ 这个规模该不该走 GPU ✗ ★**（**纯函数 ⇒ 可单测 ✓）。
#[must_use]
pub const fn should_use_gpu(pixels: usize) -> bool {
    pixels >= GPU_MIN_PIXELS
}

#[cfg(test)]
mod tests {
    use super::*;

    // **★ 判据 ✗ ★**（**第 281 轮 ✓）：
    //   **∴ ①** **小输入（**820／520 ✓）**必须**走 CPU** ✓
    //   **∴ ②** **大输入（**1 M／4 M ✓）**必须**走 GPU** ✓
    //   **∴ ③** **阈值本身**必须走 GPU**（**边界闭合 ✓）** ✓✓
    #[test]
    fn gpu_is_only_used_above_the_measured_break_even() {
        // **∴ 第 280 轮实测的那两个点 ✗**
        assert!(
            !should_use_gpu(520),
            "n=520 实测 GPU 慢 1944.8% ⇒ **∴ 必须**走 CPU**"
        );
        assert!(
            !should_use_gpu(10_000 - 1),
            "n<10k 落在实测平衡点以下 ⇒ **∴ 必须**走 CPU**"
        );
        // **∴ 大输入 ✗**
        assert!(
            should_use_gpu(1_000_000),
            "1 M 实测 GPU 快 96.9% ⇒ 必须走 GPU"
        );
        assert!(
            should_use_gpu(4_000_000),
            "4 M 实测 GPU 快 99.2% ⇒ 必须走 GPU"
        );
        // **∴ 边界 ✗**：**阈值本身走 GPU** ✓
        assert!(should_use_gpu(GPU_MIN_PIXELS));
        assert!(!should_use_gpu(GPU_MIN_PIXELS - 1));

        // **★ 直接钉住阈值本身 ✗ ★**（第 282 轮 ✓；**∴ 由变异检验逼出来 ✓）：
        //   **∴ 为什么补 ✗**：**第 281 轮的变异②**（**阈值 250 000 ⇒ 100 000 ✓）**✗
        //     **∴ 竟然**退出码 0**✗ ⇒ **∴ 因为**上面那些断言**只钉了**具体的 n 值** ✓
        //       ⇒ **∴ 于是**：**把阈值挪到 100 000**✗ ⇒ **∴ 99 999 仍 < 100 000** ⇒ **∴ 断言**仍过** ✓
        //   **⇒ ★ 所以**：**必须有一条**直接说阈值是多少**的断言** ✓ ★**** ✓✓
        //   **∴ 而**它的**变异点**：**改阈值** ⇒ **∴ 必红** ✓**** ✓✓
        assert_eq!(
            GPU_MIN_PIXELS, 30_000,
            "阈值必须钉在第 285 轮八档实测的平衡点之上（3 倍余量）；**∴ 改它要**同时改实测依据**"
        );
    }
}

// **★ GPU 自检：真的跑一次量化并与 CPU 真值逐位对比 ✗ ★**（第 291 轮 ✓；**目标 §6.3 的 ④ ✓**）。
//
// **∴ 为什么需要它 ✗**：**`/health` 的 `max_channel_delta` 一直是 `null`**** ✓
//   **∴ 而** `null` 是**对的**（**∴ 因为**从没比过 ✓）—— **∴ 但现在**可以真的比一次** ✓
//   **⇒ ★ 所以 ✗ ★**：**做完自检后**报出**真实的通道差**✗
//     ⇒ **∴ 于是**：**§6.3 的 ④**「**报出后端 ＋ `max_channel_delta`**」**第一次有实数** ✓ ★**** ✓✓
//
// **∴ 边界（**如实 ✓）★**：
//   **∴ 本自检**只在 `--features gpu` ＋ **有适配器时**跑** ✓
//   **∴ 且**：**它**不改变**渲染路径**✗ ⇒ **∴ 所以** `render_backend` **仍然报 `cpu`** ✓
//     （**∴ 因为**渲染确实**没用 GPU ✓）** ✓✓
//   **∴ 代价 ✗**：**启动后第一次 `/health`**会多花**几十毫秒**（**∴ 自检一次就缓存 ✓）** ✓✓
#[cfg(feature = "gpu")]
/// **★ GPU 量化自检（**`--features gpu` ＋ 有适配器时才跑 ✓）★**
pub mod selfcheck {
    use std::sync::OnceLock;

    /// **∴ 自检结果 ✗**：**`(max_channel_delta, 说明 ✓)**
    /// **∴ 一次自检的结果 ✗**（**∴ 与 CPU 真值逐位对比 ✓）
    #[derive(Debug, Clone)]
    pub struct Report {
        /// **∴ 最大通道差 ✗**（**∴ 0 表示逐位相同 ✓）
        pub max_channel_delta: u32,
        /// **∴ 参与对比的像素数 ✗**
        pub pixels: usize,
        /// **∴ 说明 ✗**
        pub note: String,
    }

    static CACHE: OnceLock<Option<Report>> = OnceLock::new();

    /// **∴ 跑一次并缓存 ✓**（**∴ 没有适配器 ⇒ `None` ✓）
    #[must_use]
    pub fn run() -> Option<Report> {
        CACHE
            .get_or_init(|| match try_run() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("GPU 自检未完成：{e}");
                    None
                }
            })
            .clone()
    }

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll};
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

    // **∴ 与 `tests/gpu_quantize_parity.rs` **同一份**五步实现** ✓（**∴ 不是另写一套 ✓）
    const WGSL: &str = include_str!("gpu_quantize.wgsl");

    fn try_run() -> Result<Option<Report>, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let Ok(adapter) =
            block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            return Ok(None); // **∴ 没有适配器**不是错误**✗ ⇒ **∴ 如实返回 None** ✓
        };
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("拿不到设备：{e:?}"))?;

        let n: usize = 4096;
        let mut s: u32 = 0x51ED_2701;
        let pix: Vec<[f32; 4]> = (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let a = ((s >> 20) & 0xFF) as f32 / 255.0;
                let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
                [f(0) * a, f(1) * a, f(2) * a, a]
            })
            .collect();
        let flat: Vec<f32> = pix.iter().flat_map(|p| p.iter().copied()).collect();
        let lut = yanshi_render::color::srgb_encode_table();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("selfcheck"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("selfcheck"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let layout = pipeline.get_bind_group_layout(0);
        let ab = |bytes: &[u8], usage: wgpu::BufferUsages| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: bytes.len() as u64,
                usage,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, bytes);
            b
        };
        let src = ab(
            &f32s_to_bytes(&flat),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let dst = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (n * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let lut_buf = ab(
            &f32s_to_bytes(lut),
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        );
        let params: [u32; 4] = [n as u32, lut.len() as u32, 0, 0];
        let pb = ab(
            &u32s_to_bytes(&params),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let back = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (n * 4) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
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
        enc.copy_buffer_to_buffer(&dst, 0, &back, 0, (n * 4) as u64);
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

        // **∴ CPU 真值（**同一实现 ✓）
        let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
        for (i, p) in pix.iter().enumerate() {
            b.set_pixel(i as u32, 0, *p);
        }
        let want = b.to_rgba8_quantized(None);
        let mut max_delta = 0u32;
        for i in 0..n * 4 {
            max_delta = max_delta.max(u32::from(want[i].abs_diff(got[i])));
        }
        Ok(Some(Report {
            max_channel_delta: max_delta,
            pixels: n,
            note: "GPU 量化自检（与 CPU 真值逐位对比）".to_owned(),
        }))
    }

    /// **★ 安全字节转换 ✗ ★**（第 291 轮 ✓）：**∴ 本 crate**禁止 `unsafe`**✗
    ///   （**`lib.rs` 的 `#![forbid(unsafe_code)]` ✓）⇒ **∴ 所以**逐元素转换** ✓**** ✓✓
    /// **∴ 代价 ✗**：**多一次拷贝**✗ ⇒ **∴ 而**自检只有 4096 像素** ⇒ **∴ 可忽略** ✓**** ✓✓
    fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(v.len() * 4);
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }

    /// **∴ `u32` 版 ✗**（**∴ 给 uniform 用 ✓）
    fn u32s_to_bytes(v: &[u32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(v.len() * 4);
        for x in v {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out
    }
}
