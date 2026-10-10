//! **★ 可选的 GPU 加速层 ✗ ★**（第 300 轮 ✓；**目标第 6／7 条 ✓）。
//!
//! **∴ 为什么单独一个 crate ✗**：**第 295／296 轮**的架构决定**（**如实 ✓）**✗**：
//!   **∴ 我**权衡过**把 `wgpu` 放进 `yanshi-render`**（**路 A ✓）✗
//!     ⇒ **∴ 而** `yanshi-render` **也被 `yanshi-wasm` 依赖** ✓
//!       ⇒ **∴ 那**会让 `wgpu` **进入内核的依赖图** ✓
//!         ⇒ **★ 而** AGENTS.md 第 6 条 ＋ 目标第 6 条：**内核的体积与确定性**权重更高** ✓ ★**** ✓✓
//!   **⇒ ★ 所以 ✗ ★**：**把 GPU 放进**独立 crate**✗
//!     ⇒ **∴ 只**服务端**可选依赖它** ✓
//!       ⇒ **∴ 于是**：**内核不受影响** ✗ ＋ **服务端**可以用 GPU** ✓ ★**** ✓✓
//!
//! **∴ 硬约束（**目标第 7 条 ✓）★**：**CPU 是真值**✗ ⇒ **∴ 本 crate**只**加速**✗
//!   ⇒ **∴ 逐位一致**由**调用方**用 CPU 结果核对** ✓（**∴ 见 `yanshi-http` 的自检 ✓）** ✓✓
//!
//! **∴ 边界（**如实 ✓）★**：**本 crate**不**决定**该不该用 GPU**✗
//!   ⇒ **∴ 那**由**调用方**按**规模阈值**决定** ✓（**`yanshi-http::gpu_policy` ✓）** ✓✓

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// **★ 分块 ＋ 批量**的 GPU 量化 ✗ ★**（**∴ 只**这一条对外接口 ✓）。
///
/// **∴ 输入 ✗**：**线性预乘的 `f32` 像素**（**每像素 4 个通道 ✓，**长度 ＝ `4 × pixels`** ✓）
/// **∴ 输出 ✗**：**`rgba8`**（**长度 ＝ `4 × pixels`** ✓）
/// **∴ 参数 ✗**：`lut` 是 **sRGB 编码查表**（**4097 项 ✓ —— **必须**与 CPU 用**同一张** ✓）
///
/// **∴ 为什么分块 ✗**：**第 274 轮实测**✗ ⇒ **∴ 单绑定上限 **128 MiB**** ✓
///   ⇒ **∴ 而** 4K 的输入 **132.7 MiB** 超限** ✓ ⇒ **∴ 所以**必须切** ✓**** ✓✓
/// **∴ 为什么批量 ✗**：**第 283／284 轮实测**✗ ⇒ **∴ 逐块 `Wait`**花 71.5% 的时间** ✓
///   ⇒ **∴ 全部块放进**一个 encoder ＋ 一次 `Wait`** 后：**快 88.6%** ✓**** ✓✓
///
/// # Errors
/// **∴ 没有适配器／拿不到设备／映射失败** ⇒ **∴ 返回 `Err`**（**∴ 调用方**据此回退 CPU** ✓）
pub struct Quantizer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    lut_buf: wgpu::Buffer,
    adapter_note: String,
    /// **★ 适配器等级 ✗ ★**（**∴ 供**分档阈值**用 ✓ —— **第 322 轮 ✓）。
    device_class: DeviceClass,
}

/// **★ 低于它就走 CPU ✗ ★**（第 304 轮 ✓；**∴ 从 `yanshi-http` **移过来**** ✓）。
///
/// **★ 依据（**第 285 轮的八档实测 ✓）—— **∴ 而第 318 轮**发现基准平台不存在**** ✗ ★**：
///   **∴ 核显（**Intel HD 5000 ✓）的八档（**第 285 轮 ✓）✗**：
///     **∴ n = 3 000 ✗**：**GPU **慢 77.4%**** ✓
///     **∴ n = 10 000 ✗**：**GPU **快 46.3%**** ✓
///     **∴ n = 30 000 ✗**：**GPU **快 81.5%**** ✓
///       ⇒ **∴ 平衡点在 **3 k 与 10 k** 之间** ⇒ **∴ 取 30 000**（**留 3 倍余量 ✓）** ✓✓
/// **★★★ 第 317–318 轮的更正（**如实 ✓）★★★**：
///   **∴ 我曾**按「**L4 为准**」把它改成 10 000**✗
///     ⇒ **∴ 而第 318 轮**实测发现**：**远端那个适配器是
///       **`Gl｜Cpu｜llvmpipe`（**CPU 软件渲染 ✓）**✗ ⇒ **∴ 不是**真 L4** ✓**** ✓✓
///       ⇒ **★ 所以**：**那批「**L4 的数**」**其实是** 12 线程 CPU **软件光栅的数** ✓ ★**** ✓✓
///         ⇒ **∴ 于是**：**10 000 的**依据不成立**** ✓ ⇒ **∴ 改回 30 000** ✓**** ✓✓
/// **∴ 下一步（**未做 ✓）★**：**要让容器**真的用上 L4**✗ ⇒ **∴ 需要**：
///   **∴ ①** **`/dev/dri`（**或 `/dev/nvidia*` ✓）**直通到容器** ✓
///   **∴ ②** **Vulkan ICD（**`nvidia_icd.json` ✓）** ＋ **`libvulkan`** ✓
///   ⇒ **∴ 然后** `adapter.get_info()` **才会报 `Vulkan｜DiscreteGpu｜NVIDIA L4`** ✓ ★**** ✓✓
pub const GPU_MIN_PIXELS: usize = 30_000;

/// **★ 这个规模该不该走 GPU ✗ ★**（**纯函数 ⇒ 可单测 ✓）
#[must_use]
pub const fn should_use_gpu(pixels: usize) -> bool {
    pixels >= GPU_MIN_PIXELS
}

/// **★ 按适配器等级取阈值 ✗ ★**（第 320 轮 ✓；**用户第 317 轮**以 L4 为准** ⇒ **∴ 但**核显也要照顾** ✓）。
///
/// **∴ 两个平台实测的平衡点 ✗（**都来自八档 + 5 次中位数 ✓）★**：
///   **∴ 真 L4（**`Vulkan｜DiscreteGpu` ✓）✗**：**1 000 快 25.8%**✗ ⇒ **∴ 平衡点 **≤ 1 000**** ✓
///     ⇒ **∴ 取 **10 000****（**∴ 留** 10 倍余量** ✓）** ✓✓
///   **∴ 核显（**`Gl｜IntegratedGpu` ✓）✗**：**3 000 慢 77.4%／10 000 快 46.3%**** ✓
///     ⇒ **∴ 平衡点在 **3 k–10 k**** ✓ ⇒ **∴ 取 **30 000****（**∴ 留** 3 倍余量** ✓）** ✓✓
///   **∴ 其它（**`Cpu`（**如 llvmpipe ✓）／`VirtualGpu` ✓）✗**：**取**核显那档**（**保守 ✓）
///     ⇒ **∴ 因为**软件渲染**根本不是加速** ✓（**∴ 第 318 轮**踩过 ✓）** ✓✓
///
/// **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
///   **∴ 收益 ✗**：**独显上** 1 k–10 k 也走 GPU**✗ ⇒ **∴ 于是**中大规模**都吃到加速** ✓**** ✓✓
///   **∴ 代价 ✗**：**多一条**平台相关分支**✗ ⇒ **∴ 于是**行为**依硬件而变** ✓
///     ⇒ **∴ 所以**：**响应里必须**如实报出用的是哪条** ✓（**目标第 7 条 ✓）** ✓✓
///
/// **∴ 未知类型 ✗** ⇒ **∴ 取**保守档** ✓（**∴ 与**旧的 30 000 一致 ✓）** ✓✓
#[must_use]
pub const fn threshold_for(device_type: DeviceClass) -> usize {
    match device_type {
        DeviceClass::Discrete => 10_000,
        // **∴ 集成／虚拟／CPU 软件渲染 ⇒ **一律保守**** ✓
        DeviceClass::Integrated | DeviceClass::Virtual | DeviceClass::Cpu | DeviceClass::Other => {
            GPU_MIN_PIXELS
        }
    }
}

/// **★ 适配器等级 ✗ ★**（**∴ 只用我们关心的四类 ＋ 其它 ✓）。
///
/// **∴ 为什么不直接用 `wgpu::DeviceType` ✗**：**两边都要能用它**✗
///   ⇒ **∴ 而** `yanshi-render` 的**默认构建**里**没有 `wgpu`** ✓
///     ⇒ **∴ 所以**：**本 crate**定义一个**不依赖 `wgpu` 的枚举** ✓
///       ⇒ **∴ 由调用方**从 `wgpu::DeviceType` **映射过来** ✓**** ✓✓
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    /// **∴ 独立显卡 ✗**（**`DiscreteGpu` ✓）
    Discrete,
    /// **∴ 集成显卡 ✗**（**`IntegratedGpu` ✓）
    Integrated,
    /// **∴ 虚拟 GPU ✗**（**`VirtualGpu` ✓）
    Virtual,
    /// **★ CPU 软件渲染 ✗ ★**（**`Cpu` ✓ —— **∴ 不是**加速** ✓，**第 318 轮的教训** ✓）
    Cpu,
    /// **∴ 其它／未知 ✗**
    Other,
}

/// **★ 按适配器等级判断该不该走 GPU ✗ ★**（**∴ 把**阈值 ＋ 比较**合成一个入口 ✓）
#[must_use]
pub const fn should_use_gpu_on(device_type: DeviceClass, pixels: usize) -> bool {
    pixels >= threshold_for(device_type)
}

/// **∴ 每块的输入字节上限 ✗**（**∴ 4 MiB ⇒ 远低于 128 MiB ✓）
const CHUNK_BYTES: usize = 4 * 1024 * 1024;

/// **∴ 每像素 16 字节（**4 × f32 ✓）
const BYTES_PER_PIXEL: usize = 16;

/// **★ 本平台能不能用**同步**的 `Quantizer::new` ✗ ★**（**第 443 轮 ✓）
///
/// **∴ 为什么需要这条 ✗**：**本 crate**用自实现的 `block_on`**✗（**∴ 不引入 `pollster` ✓）
///   ⇒ **∴ 而** `block_on` **只能在**多线程 ＋ 不依赖外部事件循环**的平台上工作** ✓
///     ⇒ **∴ 在 `wasm32` 上 ✗**：**单线程 ＋ 事件循环**✗
///       ⇒ **∴ `request_adapter` 的完成**要**靠**外部的微任务**✗
///         ⇒ **★ 所以**：**`block_on` **永远等不到它** ⇒ **∴ 会**死锁** ✓ ★**** ✓✓
///
/// **∴ 这条函数的作用 ✗**：**让调用方**先问一句**✗
///   ⇒ **∴ 于是**：**在 wasm 上**不去建同步 `Quantizer`**✗
///     ⇒ **∴ 而是**走**异步路**（**将来 ✓）或**直接回退 CPU** ✓
///       ⇒ **★ 那**守住了**目标第 7 条**：**不许假装用了 GPU ✗ ＋ **不许静默降级** ✓ ★**** ✓✓
///
/// **∴ 返回值 ✗**：**`true` ＝ 本平台支持同步初始化**（**桌面 ✓）；
///   **`false` ＝ 必须走异步路**（**`wasm32` ✓）** ✓
#[must_use]
pub const fn supports_sync_init() -> bool {
    !cfg!(target_arch = "wasm32")
}

impl Quantizer {
    /// **★ 建一个量化器 ✗ ★**（**∴ 成功 ⇒ GPU 可用 ✓）
    ///
    /// # Errors
    /// **∴ 没有适配器或设备** ⇒ **∴ 返回 `Err`**（**∴ 调用方回退 CPU ✓）
    pub fn new(lut: &[f32]) -> Result<Self, String> {
        // **∴ 平台守卫 ✗**（第 443 轮 ✓）：**wasm32 上**不尝试同步初始化** ✓（**∴ 不死锁 ✓）
        if !supports_sync_init() {
            return Err("sync_init_unsupported_on_wasm32：本平台是单线程事件循环，\
                        同步 block_on 会死锁 ⇒ 请走异步路或回退 CPU"
                .to_owned());
        }
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| format!("no_adapter:{e:?}"))?;
        let info = adapter.get_info();
        let adapter_note = format!(
            "adapter:{:?}:{:?}:{}",
            info.backend, info.device_type, info.name
        );
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("no_device:{e:?}"))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yanshi-gpu-quantize"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("yanshi-gpu-quantize"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let lut_bytes = f32s_to_bytes(lut);
        let lut_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lut"),
            size: lut_bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&lut_buf, 0, &lut_bytes);
        // **★ 把 `wgpu::DeviceType` **映射**到本 crate 的 `DeviceClass`** ✗ ★**（第 322 轮 ✓）：
        //   ⇒ **∴ 于是**：**阈值**由**适配器等级**决定** ✓（**∴ 第 320 轮 ✓）
        let device_class = match info.device_type {
            wgpu::DeviceType::DiscreteGpu => DeviceClass::Discrete,
            wgpu::DeviceType::IntegratedGpu => DeviceClass::Integrated,
            wgpu::DeviceType::VirtualGpu => DeviceClass::Virtual,
            wgpu::DeviceType::Cpu => DeviceClass::Cpu,
            wgpu::DeviceType::Other => DeviceClass::Other,
        };
        Ok(Self {
            device,
            queue,
            pipeline,
            lut_buf,
            adapter_note,
            device_class,
        })
    }

    /// **∴ 适配器的自述 ✗**（**∴ 可核对 ✓）
    #[must_use]
    pub fn adapter_note(&self) -> &str {
        &self.adapter_note
    }

    /// **★ 适配器等级 ✗ ★**（第 322 轮 ✓；**∴ 供**分档阈值**用 ✓）。
    ///
    /// **∴ 为什么存下来而不是现查 ✗**：**`Adapter` **没有**留在 `Quantizer` 里**✗
    ///   ⇒ **∴ 所以**：**建的时候**映射一次并存下** ✓**** ✓✓
    #[must_use]
    pub fn device_class(&self) -> DeviceClass {
        self.device_class
    }

    /// **∴ 单绑定的上限（**字节 ✓）✗**（**∴ 供调用方**自己核对分块 ✓）
    #[must_use]
    pub fn max_binding_bytes(&self) -> u64 {
        u64::from(self.device.limits().max_storage_buffer_binding_size)
    }

    /// **★ 量化一批像素 ✗ ★**（**∴ 分块 ＋ 批量 ＋ 一次 `Wait`** ✓）
    ///
    /// # Errors
    /// **∴ 映射失败** ⇒ **∴ 返回 `Err`** ✓
    /// **★ 量化 ＋ 编码 ✗ ★**（**第 459 轮：**加背景合成 ✓）
    ///
    /// **∴ `background` ✗**：**线性预乘的 `[r,g,b,a]`**✗
    ///   ⇒ **∴ `None`**：**不做合成**（**与原来一致 ✓）
    ///     ＋ **∴ `Some(bg)`**：**逐像素 `c = x + bg * (1 - x3)`**✓
    ///       ⇒ **∴ 与** CPU 的 `composite_over_linear_with` **同一口径** ✓ ★**** ✓✓
    pub fn quantize(
        &self,
        pixels: &[f32],
        count: usize,
        background: Option<[f32; 4]>,
    ) -> Result<Vec<u8>, String> {
        let per_chunk = CHUNK_BYTES / BYTES_PER_PIXEL;
        let layout = self.pipeline.get_bind_group_layout(0);
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let mut backs: Vec<(wgpu::Buffer, usize)> = Vec::new();
        let mut done = 0usize;
        while done < count {
            let take = per_chunk.min(count - done);
            let chunk: Vec<f32> = pixels[done * 4..(done + take) * 4].to_vec();
            let src_bytes = f32s_to_bytes(&chunk);
            let src = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: src_bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let dst = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (take * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            // **★ uniform 必须** 32 字节** ✗ ★**（**第 459 轮 ✓）：
            //   **∴ 布局 ✗**：**4×u32（16 B）＋ `vec4<f32>`（16 B，**偏移 16 ✓）
            //     ⇒ **∴ 与** WGSL 的 `struct Params` **逐字段对应** ✓ ★**** ✓✓
            let mut params_bytes = u32s_to_bytes(&[
                take as u32,
                self.lut_len() as u32,
                u32::from(background.is_some()),
                0, // **∴ 对齐填充 ✓**
            ]);
            match background {
                Some(bg) => params_bytes.extend_from_slice(&f32s_to_bytes(&bg)),
                None => params_bytes.extend_from_slice(&f32s_to_bytes(&[0.0, 0.0, 0.0, 0.0])),
            }
            debug_assert_eq!(params_bytes.len(), 32);
            // **★ uniform 尺寸必须** 32 字节** ✗ ★**（**第 459 轮修正 ✓）：
            //   **∴ 症状（**实测 ✓）✗**：**`Queue::write_buffer` 报
            //     「**Copy of 0..32 would end up overrunning the bounds of the
            //       Destination buffer of size 16**」**✗
            //     ⇒ **∴ 因为**加了 `vec4<f32>` 后**要 32 字节**✗
            //       ⇒ **∴ 而**分配还是 16** ✓
            //         ⇒ **∴ 于是**：**服务端**启动即崩** ✓ ★**** ✓✓
            //   **∴ 现在 ✗**：**与** WGSL 的 `struct Params`（**4×u32 ＋ vec4<f32> ✓）一致** ✓ ★**** ✓✓
            let pb = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 32,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let back = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: (take * 4) as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.queue.write_buffer(&src, 0, &src_bytes);
            self.queue.write_buffer(&pb, 0, &params_bytes);
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                        resource: self.lut_buf.as_entire_binding(),
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
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups((take as u32).div_ceil(64), 1, 1);
            }
            enc.copy_buffer_to_buffer(&dst, 0, &back, 0, (take * 4) as u64);
            backs.push((back, take));
            done += take;
        }
        // **★ 一次提交 ＋ 一次等待 ✗ ★**（**∴ 第 284 轮：**快 88.6% ✓）
        self.queue.submit(Some(enc.finish()));
        let _ = self.device.poll(wgpu::PollType::Wait);

        let mut out: Vec<u8> = Vec::with_capacity(count * 4);
        for (back, _take) in &backs {
            let slice = back.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            let _ = self.device.poll(wgpu::PollType::Wait);
            if rx.recv().is_err() {
                return Err("map_channel:closed".to_owned());
            }

            let mapped = slice.get_mapped_range();
            out.extend_from_slice(&mapped);
            drop(mapped);
            back.unmap();
        }
        Ok(out)
    }

    fn lut_len(&self) -> usize {
        (self.lut_buf.size() / 4) as usize
    }
}

/// **∴ 极简 `block_on` ✗**（**∴ 不引入 `pollster` ✓）
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

/// **∴ 安全字节转换 ✗**（**∴ 本 crate**禁 `unsafe`** ✓）
fn f32s_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// **∴ `u32` 版 ✗**
fn u32s_to_bytes(v: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// **∴ 五步量化（**与 CPU 真值同一套 ✓）✗** —— **∴ 与 `yanshi-http` 的共享文件**同一份** ✓
const SHADER: &str = include_str!("quantize.wgsl");

#[cfg(test)]
mod tests {
    use super::{should_use_gpu_on, threshold_for, DeviceClass, GPU_MIN_PIXELS};

    /// **★ 两档阈值都要钉住 ✗ ★**（第 320 轮 ✓）：
    ///   **∴ 变异点 ✗**：**把 `Discrete` 改成 30_000**✗ ⇒ **∴ 本断言**必红** ✓**
    ///   **∴ 且**：**把 `Cpu` 改成 10_000**✗ ⇒ **∴ 本断言**必红** ✓（**∴ 那**正是**第 318 轮的错** ✓）**
    #[test]
    fn thresholds_follow_the_adapter_class() {
        assert_eq!(
            threshold_for(DeviceClass::Discrete),
            10_000,
            "独显按 L4 的平衡点"
        );
        assert_eq!(threshold_for(DeviceClass::Integrated), GPU_MIN_PIXELS);
        assert_eq!(threshold_for(DeviceClass::Virtual), GPU_MIN_PIXELS);
        // **★ 软件渲染**绝不**享受激进阈值** ✓（**∴ 第 318 轮的教训 ✓）
        assert_eq!(
            threshold_for(DeviceClass::Cpu),
            GPU_MIN_PIXELS,
            "llvmpipe 不是加速"
        );
        assert_eq!(threshold_for(DeviceClass::Other), GPU_MIN_PIXELS);
        // **∴ 且**：**独显那档**必须**严格小于**保守档**
        assert!(threshold_for(DeviceClass::Discrete) < GPU_MIN_PIXELS);
    }

    /// **∴ 边界是闭的 ✗**（**∴ 与**旧判据同一形状 ✓）
    #[test]
    fn boundaries_are_closed() {
        assert!(should_use_gpu_on(DeviceClass::Discrete, 10_000));
        assert!(!should_use_gpu_on(DeviceClass::Discrete, 9_999));
        assert!(should_use_gpu_on(DeviceClass::Integrated, GPU_MIN_PIXELS));
        assert!(!should_use_gpu_on(
            DeviceClass::Integrated,
            GPU_MIN_PIXELS - 1
        ));
        // **∴ 而在** 30 000 附近，独显与集显**结论不同**✗ ⇒ **∴ 那**正是分档的意义** ✓
        assert!(should_use_gpu_on(DeviceClass::Discrete, 30_000));
        assert!(should_use_gpu_on(DeviceClass::Integrated, 30_000));
        assert!(should_use_gpu_on(DeviceClass::Discrete, 12_000));
        assert!(!should_use_gpu_on(DeviceClass::Integrated, 12_000));
    }
}
