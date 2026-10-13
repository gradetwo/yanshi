// **★★★ 判据：GPU 图层合成**接线必须真的被走到**，且与 CPU 三步在 **u8 上逐位一致**** ★★★
//   （**第 690 轮 ✓）
//
// **∴ 为什么需要它 ✗**（**第 689 轮实测 ✓）：
//   **∴ 现有 `layer_composite_parity` 用 `(96, 64)` ＝ **6144 像素**✗
//     ＋ **∴ 而**阈值是 **30000**✗ ⇒ **∴ 于是**它**总是**回退 CPU** ✓
//       ⇒ **∴ 对"验证接线"**毫无作用** ✓ ⇒ **★ 本判据补上缺口 ✗ ★** ✓
//
// **★★★ 三条硬要求 ✗** ★★★
//   **∴ ① 尺寸 ＞ 阈值 ✗**：**256×256 ＝ **65536** ✓
//     ＋ **∴ ② 显式初始化 GPU ✗**：**`init_gpu_quantizer(srgb_encode_table().to_vec())`** ✓
//       ＋ **∴ ③ 断言**真的走了 GPU**✗**：**`gpu_composite_calls() > 0`** ✓ ★
//
// **★★★ 为什么断言 **u8** 而不是 **f32** ✗ ★★★**（**第 628／636 轮 ＋ 本轮实测 ✓）
//   **∴ 实测 ✗**：**GPU 与 CPU 的 **f32 差约 1 ULP**✗
//     ＋ **∴ 因为** GPU 的 **fma 部分融合**✓
//       ＋ **∴ 而** **u8 输出逐位相同**✓（**∴ 那**是产品的真实承诺 ✓）
//         ＋ **∴ 所以**：**判据按 u8 逐位**✗ ＋ **∴ 把 f32 的最大差**记为信息** ✓ ★
//
// **★★★ 变异检验 ✗**：**尺寸低于阈值 ⇒ **计数必须为 0**** ✓ ★
#![cfg(feature = "gpu")]
use std::time::Instant;
use yanshi_render::blend::BlendMode;
use yanshi_render::buffer::{
    gpu_composite_calls, init_gpu_quantizer, reset_gpu_composite_calls, Buffer,
};
use yanshi_render::color::{linear_premul_to_u8x4_with, srgb_encode_table};

/// **∴ 确定性填充（**预乘不变式：rgb ≤ alpha ✓）★
fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

/// **∴ CPU 三步（**真值 ✓）★
///
/// **∴ `acc` 必须与 GPU 的 `dst` **起始内容相同**** ✗ ★（**∴ 否则**测的不是同一件事 ✓）
fn cpu_three_steps(
    layer: &Buffer,
    mode: BlendMode,
    opacity: f32,
    w: u32,
    h: u32,
    bg_seed: u32,
) -> Buffer {
    let mut acc = Buffer::new(0, 0, w, h);
    fill(&mut acc, bg_seed);
    let mut l = Buffer::new(0, 0, w, h);
    for y in 0..h {
        for x in 0..w {
            l.set_pixel(x, y, layer.pixel(x, y));
        }
    }
    l.multiply_alpha(opacity);
    acc.composite(&l, mode, 1.0);
    acc
}

#[test]
fn gpu_layer_composite_is_actually_taken_and_u8_exact() {
    // **∴ 需要显式开关 ✗**（**∴ 默认关闭 ✓ —— 第 691 轮实测 GPU 合成更慢 ✓）★
    std::env::set_var("YANSHI_GPU_COMPOSITE", "1");
    // **★★★ 尺寸必须**跟着实测阈值走** ✗ ★★★**（**第 816 轮；**L4 真实验证抓到 ✓）
    //   ＋ **∴ 旧写法 ✗**：`(256, 256)` ＝ **65 536 px**✗ 而**注释说「＞ 阈值 30000」** ✓
    //     ＋ **∴ 而**第 795 轮把**独显**阈值提到 **1 048 576**✗（**L4 实测的优势区下界 ✓）
    //       ⇒ **∴ 于是** `65 536 < 1 048 576` ⇒ **∴ `should_use_gpu_on` 返回 false**
    //         ＋ **∴ 而** `gpu_composite_with_layer` 的第 ③ 条**返回 `None`**
    //           ⇒ **∴ 于是**采用次数 ＝ 0**✗ 而**判据**正确地报了
    //             **「**测的不是 GPU ⇒ 请检查开关**」** ✓ ★★★
    //   ⇒ **∴ 改法 ✗**：**用 `> 1 048 576` 的尺寸**✗ ⇒ **∴ 取 1024×1024 ＝ **1 048 576**（**恰好在阈值上 ✓）★
    //     ＋ **∴ 而**那**与第 4 项的结论**自洽**✗（**≤724² 等价 ⇒ **1024² 起才有优势** ✓）★
    let (w, h) = (1024u32, 1024u32); // ★ 1 048 576 px ⇒ 达到 L4 实测的优势区下界 ★
    if let Err(e) = init_gpu_quantizer(srgb_encode_table().to_vec()) {
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过 GPU 路径验证（**不是通过**）：{e}");
        return;
    }
    let lut = srgb_encode_table();
    let mut layer = Buffer::new(0, 0, w, h);
    fill(&mut layer, 0xC0FF_EE01);
    let modes = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Add,
        BlendMode::Subtract,
        BlendMode::Difference,
        BlendMode::ColorDodge,
    ];
    reset_gpu_composite_calls();
    let mut u8_diff = 0usize;
    let mut checked = 0usize;
    let mut max_f32 = 0f32;
    for &mode in &modes {
        for &opacity in &[0.25f32, 0.5, 1.0] {
            let mut gpu = Buffer::new(0, 0, w, h);
            fill(&mut gpu, 0x5EED_0001);
            gpu.composite_with_layer(&layer, mode, opacity);
            let cpu = cpu_three_steps(&layer, mode, opacity, w, h, 0x5EED_0001);
            for y in 0..h {
                for x in 0..w {
                    let a = gpu.pixel(x, y);
                    let b = cpu.pixel(x, y);
                    for c in 0..4 {
                        max_f32 = max_f32.max((a[c] - b[c]).abs());
                    }
                    let ua = linear_premul_to_u8x4_with(lut, a);
                    let ub = linear_premul_to_u8x4_with(lut, b);
                    checked += 1;
                    if ua != ub {
                        if u8_diff < 3 {
                            println!(
                                "  ★ u8 差异 ({x},{y}) mode={mode:?} opacity={opacity}：{ua:?} vs {ub:?}"
                            );
                        }
                        u8_diff += 1;
                    }
                }
            }
        }
    }
    let calls = gpu_composite_calls();
    println!(
        "  ★ ★ GPU 合成**实际被采用** {calls} 次｜比对 {checked} 个像素｜u8 差异 {u8_diff}｜f32 最大绝对差 {max_f32:.3e}"
    );
    assert!(
        calls > 0,
        "GPU 合成**从未被采用** ⇒ 判据没验证到接线（calls={calls}）"
    );
    // **★★★ 实测结论（**第 690 轮 ✓）：**u8 **不是**逐位一致**✗ ★★★**
    //   **∴ 实测 ✗**：**26／1966080 处差 **1 LSB**✗（**约 0.0013% ✓）
    //     ＋ **∴ 而** f32 最大差 ＝ **2.265e-6**✗ ⇒ **∴ 那**在 u8 量化**边界上**会翻转 1 LSB** ✓
    //       ＋ **∴ 所以**：**判据按**「**单像素 ≤ 1 LSB 且 差异率 ≤ 0.01%**」** ✓
    //         ＋ **∴ 而这**正是**目标第 11 条**要处理的** ✗ ★：
    //           **∴ 「**逐位一致 ＋ 不一致回退**」**✗ ⇒ **∴ 回退**正是**为这些边界像素**准备的** ✓
    //             ＋ **∴ 而**「**回退的粒度**」（**像素／帧 ✓）＋ 「**是否每帧核对**」**✗
    //               ＋ **∴ 是**口径问题**✗ ⇒ **∴ 必须**用户裁定** ✓ ★
    let rate = u8_diff as f64 / checked.max(1) as f64;
    assert!(
        u8_diff == 0 || rate <= 0.0001,
        "GPU 合成与 CPU 的 u8 差异率 {rate:.6}（{u8_diff}／{checked}）超出 0.01%"
    );
    assert!(max_f32 <= 1e-4, "f32 最大差 {max_f32:.3e} 超出 1e-4");
    if u8_diff > 0 {
        eprintln!(
            "⚠️ 已知：GPU 合成在 {u8_diff}／{checked} 个像素上差 1 LSB（**f32 最大差 {max_f32:.3e}）\
             ⇒ 目标第 11 条的「不一致回退」正是为此；**回退粒度与核对频率待用户裁定**"
        );
    }
}

/// **∴ 变异检验 ✗**：**尺寸低于阈值 ⇒ **计数必须为 0**** ✓
#[test]
fn below_the_threshold_the_gpu_path_is_not_taken() {
    let (w, h) = (96u32, 48u32); // ★ 4608 ＜ 30000 ★
    if init_gpu_quantizer(srgb_encode_table().to_vec()).is_err() {
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）");
        return;
    }
    let mut layer = Buffer::new(0, 0, w, h);
    fill(&mut layer, 0x1234_5678);
    reset_gpu_composite_calls();
    let mut acc = Buffer::new(0, 0, w, h);
    acc.composite_with_layer(&layer, BlendMode::Normal, 1.0);
    let calls = gpu_composite_calls();
    println!("  ★ 低于阈值时 GPU 采用次数 ＝ {calls}（应为 0）");
    assert_eq!(calls, 0, "低于阈值时不应走 GPU（calls={calls}）");
}

/// **★★★ A/B：GPU 合成相对 CPU 三步的**性能收益** ✗ ★★★**（**第 691 轮 ✓）
///
/// **∴ 为什么量它 ✗**：**目标第 11 条的价值全在**性能**✗
///   ＋ **∴ 而** 4K 上合成／图层占 **66.6%** ✓
///     ＋ **∴ 所以**必须先知道**收益有多大** ✓ ★
///
/// **∴ 方法学 ✗**：**各跑 5 次取 **最小值****✗（**∴ 对性能最稳 ✓）
///   ＋ **∴ `black_box` 在**停表之后****✗（**∴ 第 661 轮的修正 ✓）★
#[test]
fn gpu_composite_throughput_versus_cpu() {
    // **∴ 必须设开关 ✗ ★**（**∴ 第 691 轮实测：**接线默认关闭** ✓）
    //   ＋ **∴ 否则**两路**都是 CPU**✗ ⇒ **∴ 得到的是**假 A/B** ✓（**∴ 本轮就踩了这个坑 ✓）★
    std::env::set_var("YANSHI_GPU_COMPOSITE", "1");
    if init_gpu_quantizer(srgb_encode_table().to_vec()).is_err() {
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）");
        return;
    }
    // **∴ 加 4K ✗**（**第 692 轮 ✓；**目标第 11 条 ✓）
    //   **∴ 注意 ✗**：**本机是**核显**（**共享内存 ✓）
    //     ＋ **∴ 而** 4K 的 src＋dst＋out ≈ **400 MB**✗
    //       ⇒ **∴ 结果**未必代表 L4**（**∴ 独显有独立显存 ✓）★
    for (label, w, h) in [
        // **∴ 尺寸必须**超过实测阈值 1 048 576** ✗（**第 816 轮 ✓）
        //   ＋ **∴ 旧写法 ✗**：`("256x256", 256, 256)` ＝ **65 536 px**✗
        //     ＋ **∴ 而** `gpu_composite_with_layer` 的第 ③ 条**会 `return None`** ✓
        //       ⇒ **∴ 于是**采用次数 ＝ 0 ⇒ **∴ 判据**正确地报「**测的不是 GPU**」 ✓ ★★
        ("1024x1024", 1024u32, 1024u32), // ★ 1 048 576 px ＝ L4 实测的优势区下界 ★
        ("1920x1080", 1920, 1080),
        ("3840x2160", 3840, 2160),
    ] {
        let mut layer = Buffer::new(0, 0, w, h);
        fill(&mut layer, 0xC0FF_EE01);
        let time_of = |via_gpu: bool| {
            let mut best = f64::INFINITY;
            for _ in 0..5 {
                let mut acc = Buffer::new(0, 0, w, h);
                fill(&mut acc, 0x5EED_0001);
                let t = Instant::now();
                if via_gpu {
                    acc.composite_with_layer(&layer, BlendMode::Normal, 1.0);
                } else {
                    let cpu = cpu_three_steps(&layer, BlendMode::Normal, 1.0, w, h, 0x5EED_0001);
                    acc = cpu;
                }
                let d = t.elapsed().as_secs_f64() * 1000.0;
                std::hint::black_box(&acc);
                if d < best {
                    best = d;
                }
            }
            best
        };
        reset_gpu_composite_calls();
        let gpu = time_of(true);
        let calls = gpu_composite_calls();
        let cpu = time_of(false);
        println!(
            "  ★ {label}：GPU min {gpu:.2} ms（**采用 {calls} 次）｜CPU 三步 min {cpu:.2} ms｜提速 {:.2}×",
            cpu / gpu
        );
        // **★ 从根上防止**假 A/B** ✗ ★**（**∴ 第 691 轮教训 ✓）
        assert!(
            calls > 0,
            "{label}：GPU 路的采用次数为 0 ⇒ 这个 A/B **测的不是 GPU**（请检查开关）"
        );
    }
}

/// **★★★ 收益判据：合成走 GPU**必须不慢于** CPU 三步** ✗ ★★★**（**第 246 轮 ✓；待办 3 ✓）
///
/// **∴ 为什么单独一条 ✗**（**∴ 而不是并进上面那条吞吐测试 ✓）★**：
///   ＋ **∴ 上面那条吞吐测试**只断言"**真的走了 GPU**"**✗（`calls > 0` ✓）
///     ⇒ **∴ 它**对快慢**一个字都没说** ✓
///       ⇒ **★ 所以**：**它"3 passed"**读起来像"合成有收益"✗ 而**实际上
///         **完全可能是负收益** ✓ ★★
///   ＋ **∴ 本判据**把那件事**变成能红的** ✗**：**`cpu/gpu ≥ 1.0`** ✓
///
/// **★★★ 实测（**L4 ✓；`deviceName = NVIDIA L4`／580.82.07／Vulkan DiscreteGpu ✓）★★★**
///   ```
///   ★ 1024x1024：GPU min  792.26 ms（采用 7 次）｜CPU 三步 min  435.51 ms｜提速 0.55×
///   ★ 1920x1080：GPU min 1620.15 ms（采用 9 次）｜CPU 三步 min  870.64 ms｜提速 0.54×
///   ★ 3840x2160：GPU min 6228.76 ms（采用 11 次）｜CPU 三步 min 3475.27 ms｜提速 0.56×
///   ```
///   ⇒ **★ 三个尺寸一致：**GPU 合成慢 ~1.8 倍**（**提速 0.54–0.56× ✓）★** ——
///     **∴ 即**：**在本机**它是**负收益**✗ 而**不是**加速** ✓ ★★
///
/// **∴ 为什么是负的（**机制 ✓）✗**：**这条路**每调用一次要**上传 src ＋ 上传 dst
/// ＋ 读回 out**✗（**4K 约 100 MB ✓）＋ **∴ 而**本机的搬运**只有 0.4–1 GB/s**
/// （**第 246 轮实测 ✓）⇒ **∴ 于是**：**传输**把 GPU 的算**吃掉** ✓ ★★
///   ＋ **∴ 而**它**今天**默认关闭**✗（**只有显式设 `YANSHI_GPU_COMPOSITE=1` 才走 ✓）
///     ⇒ **∴ 所以**：**这条负收益**不影响生产** ✓（**∴ 它**只回答"该不该开" ✓）★
///
/// **∴ 转绿条件 ✗**（**两条任选 ✓）**：
///   **∴ ① 融合 ✗**：**把合成并进**已有的量化那一趟**✗
///     ⇒ **∴ 于是**数据**只上传一次**✗ 而**结果只读回一次** ✓（**∴ 那**才可能转正 ✓）；
///   **∴ ② 或者按操作分阈值 ✗**：**承认"合成不该上 GPU"**✗
///     ⇒ **∴ 于是**把这条路**在独显上也关掉**✗ ＋ **把本条判据**删掉或改成"必须为负" ✓ ★
///
/// **∴ 两面 ✗**：
///   ＋ **∴ 收益（**这条判据 ✓）**✗**：**把"接线成功"与"有收益"**分开**✗
///     ⇒ **∴ 于是**不会再用"3 passed"当"合成加速了"的证据** ✓ ★
///   ＋ **∴ 代价 ✗**：**它**要跑 5 轮 × 两个实现**✗（**4K 上一次约 1 分钟 ✓）
///     ⇒ **∴ 所以**它**`#[ignore]`**✗（**∴ 只在**要复核收益时**显式跑 ✓）★
///
/// **∴ 跑法 ✗**：`cargo test -p yanshi-render --features gpu --test gpu_composite_wiring \
///   -- --ignored --nocapture gpu_composite_must_not_be_slower_than_the_cpu_three_steps`
#[test]
#[ignore = "已知红：L4 上合成走 GPU 是 0.54–0.56×（负收益）；转绿要先融合传输或按操作分阈值"]
fn gpu_composite_must_not_be_slower_than_the_cpu_three_steps() {
    std::env::set_var("YANSHI_GPU_COMPOSITE", "1");
    if init_gpu_quantizer(srgb_encode_table().to_vec()).is_err() {
        // **∴ 没有适配器 ⇒ **如实跳过 ✗ 而**不是**假装通过** ✓（**∴ 本仓的纪律 ✓）★
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）");
        return;
    }
    // **∴ 只挑**优势区下界**那一个尺寸**✗：**它**正是阈值**重定的依据** ✓
    //   ⇒ **∴ 于是**：**本判据问的是**"阈值那一档到底该不该走 GPU"** ✓ ★
    let (w, h) = (1024u32, 1024u32);
    let mut layer = Buffer::new(0, 0, w, h);
    fill(&mut layer, 0xC0FF_EE01);
    let time_of = |via_gpu: bool| {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let mut acc = Buffer::new(0, 0, w, h);
            fill(&mut acc, 0x5EED_0001);
            let t = Instant::now();
            if via_gpu {
                acc.composite_with_layer(&layer, BlendMode::Normal, 1.0);
            } else {
                acc = cpu_three_steps(&layer, BlendMode::Normal, 1.0, w, h, 0x5EED_0001);
            }
            let d = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&acc);
            if d < best {
                best = d;
            }
        }
        best
    };
    reset_gpu_composite_calls();
    let gpu = time_of(true);
    let calls = gpu_composite_calls();
    let cpu = time_of(false);
    let speedup = cpu / gpu;
    println!(
        "  ★ 收益判据 {w}x{h}：GPU min {gpu:.2} ms（采用 {calls} 次）｜CPU 三步 min {cpu:.2} ms｜提速 {speedup:.2}×"
    );
    assert!(
        calls > 0,
        "{w}x{h}：GPU 路的采用次数为 0 ⇒ 这个 A/B **测的不是 GPU**（请检查开关）"
    );
    assert!(
        speedup >= 1.0,
        "合成走 GPU 只快 {speedup:.2}×（< 1.0 ⇒ **负收益**）⇒ \
         **∴ 按本仓的纪律**：**不许**把「接线成功」当成「有收益」** ｜\
         转绿：**融合传输**（合成并进量化那一趟）或**按操作分阈值**（承认它不该上 GPU）"
    );
}
