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
//! **★ 阈值取 **10 000**（**第 317 轮：**按用户裁定，以 L4 为准** ✓）★**：
//!   **∴ L4 的八档实测（**第 314 轮 ✓）✗**：
//!     **∴ n = 1 000 ✗**：**GPU **慢 25.1%**** ✓
//!     **∴ n = 3 000 ✗**：**★ GPU **快 59.6%**** ✓（**∴ 首个盈利档 ✓）
//!     **∴ n = 10 000 ✗**：**GPU **快 82.5%**** ✓
//!       ⇒ **∴ 平衡点在 **1 k 与 3 k** 之间** ⇒ **∴ 取 10 000**（**留 3.3 倍余量 ✓）** ✓✓
//! **∴ 旧的 30 000 是**按核显**定的**✗（**核显平衡点 3 k–10 k ✓）
//!   ⇒ **∴ 而**核显**不再是基准**✗（**用户第 317 轮 ✓）** ✓✓
//! **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
//!   **∴ 收益 ✗**：**L4 上 **3 k–10 k** 这一段**现在也会走 GPU** ✓**** ✓✓
//!   **∴ 代价 ✗**：**在**核显**上，**3 k–10 k** 这一段**会走 GPU 而**更慢**** ✓
//!     ⇒ **∴ 而**核显**不是基准**✗ ⇒ **∴ 但**代价**写在这里** ✓**** ✓✓
//! **∴ 下一步（**未做 ✓）★**：**若要**两头都好**✗ ⇒ **∴ 按适配器类型选阈值** ✓

/// **★ 阈值现在住在 `yanshi-gpu` ✗ ★**（第 304 轮 ✓）：
///   **∴ 为什么搬 ✗**：**`yanshi-render` **也要**用它**✗（**∴ 分派点在那里 ✓）
///     ⇒ **∴ 若**两边各存一份**✗ ⇒ **∴ 会**不一致** ✓
///       ⇒ **∴ 所以**：**单一来源 ＝ `yanshi_gpu::GPU_MIN_PIXELS`** ✓**** ✓✓
#[cfg(feature = "gpu")]
pub use yanshi_gpu::GPU_MIN_PIXELS;

#[cfg(feature = "gpu")]
pub use yanshi_gpu::should_use_gpu;

/// **∴ 没有 feature 时的**同一条口径**** ✓（**∴ 于是**判据在任何配置下都能跑 ✓）
#[cfg(not(feature = "gpu"))]
pub const GPU_MIN_PIXELS: usize = 10_000;

/// **∴ 没有 feature 时的**同一条口径**** ✓
#[cfg(not(feature = "gpu"))]
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
            GPU_MIN_PIXELS, 10_000,
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

    /// **★ 委托给**独立 crate** ✗ ★**（第 301 轮 ✓）：
    ///   **∴ 第 300 轮**把 GPU 层单独成 `yanshi-gpu`**✗
    ///     ⇒ **∴ 所以这里**不再**自己建管线、**也不**再带一份 WGSL** ✓
    ///       ⇒ **∴ 于是**：**只有一份五步实现** ✓（**∴ 与**测试**同一份 ✓）** ✓✓
    fn try_run() -> Result<Option<Report>, String> {
        let lut = yanshi_render::color::srgb_encode_table();
        let quantizer = match yanshi_gpu::Quantizer::new(lut) {
            Ok(q) => q,
            // **∴ 没有适配器**不是错误**✗ ⇒ **∴ 如实返回 None** ✓
            Err(e) if e.starts_with("no_adapter") => return Ok(None),
            Err(e) => return Err(e),
        };
        let n: usize = 4096;
        let mut s: u32 = 0x51ED_2701;
        let mut pixels: Vec<f32> = Vec::with_capacity(n * 4);
        for _ in 0..n {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let a = ((s >> 20) & 0xFF) as f32 / 255.0;
            let f = |k: u32| ((s >> (k * 5)) & 0x1F) as f32 / 31.0;
            pixels.extend_from_slice(&[f(0) * a, f(1) * a, f(2) * a, a]);
        }
        let got = quantizer.quantize(&pixels, n)?;

        // **∴ CPU 真值（**同一实现 ✓）
        let mut b = yanshi_render::buffer::Buffer::new(0, 0, n as u32, 1);
        for i in 0..n {
            b.set_pixel(
                i as u32,
                0,
                [
                    pixels[i * 4],
                    pixels[i * 4 + 1],
                    pixels[i * 4 + 2],
                    pixels[i * 4 + 3],
                ],
            );
        }
        let want = b.to_rgba8_quantized(None);
        let mut max_delta = 0u32;
        for i in 0..n * 4 {
            max_delta = max_delta.max(u32::from(want[i].abs_diff(got[i])));
        }
        Ok(Some(Report {
            max_channel_delta: max_delta,
            pixels: n,
            note: format!(
                "GPU 量化自检（与 CPU 真值逐位对比）｜{}",
                quantizer.adapter_note()
            ),
        }))
    }
}
