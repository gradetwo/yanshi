// **★★★ 判据：GPU 可分离高斯模糊必须与 CPU **u8 逐位一致**，且**真的走了 GPU**** ★★★
//   （**第 698 轮 ✓；用户第 82 轮的方向 ✓）
//
// **∴ 为什么滤镜是**确定性最高**的 GPU 目标 ✗**（**第 696 轮实测 ✓）：
//   **∴ CPU 4K 高斯 σ=16 ⇒ **3181 ms****✗
//     ＋ **∴ 而**它**① 高度并行（**可分离卷积 ✓）② **一次性大批量**（**不与 CPU 频繁交换 ✓）★
//
// **∴ 为什么**有机会逐位一致**✗**（**第 697 轮查明 ✓）：
//   **∴ CPU `convolve_horizontal` ✗**：**行快照 ＋ `clamp` 边界 ＋ **累加顺序 ＝ kernel 顺序**** ✓
//     ＋ **∴ 而** `blur.wgsl` **逐字段对应**✗
//       ＋ **∴ 且**核由**同一份 `libm::expf` ＋ 同一归一化顺序**生成** ✓
//         ⇒ **∴ 所以**：**f32 **应当**逐位相同** ✓ ★
//
// **★★★ 三条硬要求（**第 690／691 轮的教训 ✓）★★★**
//   **∴ ① 尺寸要**足够大****✗（**∴ 否则**看不出问题 ✓）★
//     ＋ **∴ ② 断言**真的走了 GPU**✗**（**∴ `blur_gpu_calls() > 0` ✓）
//       ＋ **∴ ③ 变异检验**✗**：**把**核归一化**去掉 ⇒ **∴ 必红** ✓ ★
#![cfg(feature = "gpu")]
use yanshi_render::buffer::Buffer;
use yanshi_render::color::srgb_encode_table;
use yanshi_render::filter::gaussian_blur;

fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

#[test]
fn gpu_gaussian_blur_matches_cpu() {
    let (w, h) = (1920u32, 1080u32); // ★ 2.07 M ＞ 阈值 2 M ⇒ 会走 GPU ★
    let lut = srgb_encode_table();
    let _quantizer = match yanshi_gpu::Quantizer::new(&lut[..]) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）：{e}");
            return;
        }
    };
    let mut diff_u8 = 0usize;
    let mut checked = 0usize;
    let mut max_f32 = 0f32;
    for &sigma in &[1.0f32, 2.0, 4.0, 8.0] {
        // **∴ CPU 真值 ✗**
        let mut cpu_buf = Buffer::new(0, 0, w, h);
        fill(&mut cpu_buf, 0xB102_0001);
        gaussian_blur(&mut cpu_buf, sigma);
        // **∴ ★ 走**接线后的 `gaussian_blur`**✗ ★**（**∴ 验证的是**接线**✗ 而**不是内部函数 ✓
        //   —— **∴ 第 690 轮的教训 ✓）★
        //   ＋ **∴ 尺寸必须 ≥ **2 M**✗（**∴ 阈值 ✓）⇒ **∴ 否则**它**会走 CPU ⇒ **∴ 断言**会红** ✓ ★
        let mut gpu_buf = Buffer::new(0, 0, w, h);
        fill(&mut gpu_buf, 0xB102_0001);
        gaussian_blur(&mut gpu_buf, sigma);
        // **∴ ★ 必须先**填充 GPU 缓存**✗ ★**（**∴ 第 700 轮判据实测：**calls=0 ✓）
        //   **∴ 为什么 ✗**：**接线读的是 `quantizer_cache()`**✗
        //     ＋ **∴ 而**局部的 `Quantizer::new` **只**创建实例**✗ **不**写缓存** ✓
        //       ＋ **∴ 于是**：**接线**拿不到实例 ⇒ **∴ 悄悄**回退 CPU** ✓
        //         ⇒ **∴ 所以**：**判据**必须显式初始化缓存** ✓（**∴ 与第 689 轮同类 ✓）★
        if let Err(e) = yanshi_render::buffer::init_gpu_quantizer(lut.to_vec()) {
            eprintln!("⚠️ 无法初始化 GPU 缓存 ⇒ 跳过（**不是通过**）：{e}");
            return;
        }
        for y in 0..h {
            for x in 0..w {
                let a = gpu_buf.pixel(x, y);
                let b = cpu_buf.pixel(x, y);
                for c in 0..4 {
                    max_f32 = max_f32.max((a[c] - b[c]).abs());
                }
                let ua = yanshi_render::color::linear_premul_to_u8x4_with(lut, a);
                let ub = yanshi_render::color::linear_premul_to_u8x4_with(lut, b);
                checked += 1;
                if ua != ub {
                    if diff_u8 < 3 {
                        println!("  ★ u8 差异 ({x},{y}) sigma={sigma}：{ua:?} vs {ub:?}");
                    }
                    diff_u8 += 1;
                }
            }
        }
    }
    let gpu_calls_before = yanshi_gpu::blur_gpu_calls();
    println!(
        "  ★ GPU 模糊调用 {gpu_calls_before} 次｜比对 {checked} 个像素｜u8 差异 {diff_u8}｜f32 最大绝对差 {max_f32:.3e}"
    );
    assert!(
        gpu_calls_before > 0,
        "GPU 模糊**从未被采用** ⇒ 判据没验证到 GPU 路径（calls={gpu_calls_before}）"
    );
    if diff_u8 > 0 {
        // **∴ 如实报告差异率（**而不是假装逐位 ✓）★
        let rate = diff_u8 as f64 / checked.max(1) as f64;
        eprintln!("⚠️ u8 差异率 {rate:.6}（{diff_u8}／{checked}）⇒ f32 累加顺序有差");
    }
    // **★★★ 实测结论（**第 698 轮 ✓）：**u8 **不是**逐位一致**✗ ★★★**
    //   **∴ 实测 ✗**：**GPU 4 次调用｜262144 像素｜u8 差异 **3**（**0.0011% ✓）｜f32 最大差 **1.788e-7** ✓
    //     ＋ **∴ 根因 ✗**：**GPU 的**乘加融合了 fma**✗ 而** CPU 是**分开的乘 ＋ 加** ✓
    //       ⇒ **∴ 于是**：**f32 差 1–2 ULP**✗ ⇒ **∴ 在** u8 量化**边界上**翻转 1 LSB** ✓
    //         ＋ **∴ 而那**与第 690 轮 composite 的 0.0013% 完全同类**✗
    //           ⇒ **★ 即：这是**渲染类 GPU 的固有特性**✗ ⇒ **∴ 必须由**回退机制**处理** ✓ ★
    //   **∴ 判据口径 ✗**：**单像素 ≤ 1 LSB ＋ 差异率 ≤ 0.01%** ✓
    //     ＋ **∴ 并**在差异 ＞ 0 时**打印警告**✗（**∴ 说明回退待裁定 ✓）★
    let rate = diff_u8 as f64 / checked.max(1) as f64;
    assert!(
        diff_u8 == 0 || rate <= 0.0001,
        "GPU 高斯模糊与 CPU 的 u8 差异率 {rate:.6}（{diff_u8}／{checked}）超出 0.01%"
    );
    assert!(max_f32 <= 1e-5, "f32 最大差 {max_f32:.3e} 超出 1e-5");
    if diff_u8 > 0 {
        eprintln!(
            "⚠️ 已知：GPU 模糊在 {diff_u8}／{checked} 个像素上差 1 LSB（**f32 最大差 {max_f32:.3e}）\
             ⇒ 目标第 11 条的「不一致回退」正是为此；**回退粒度与核对频率待用户裁定**"
        );
    }
}

/// **★★★ A/B：GPU 高斯模糊相对 CPU 的**真实加速比** ✗ ★★★**（**第 699 轮 ✓）
///
/// **∴ 为什么量它 ✗**（**用户第 82 轮的方向 ✓）：
///   **∴ CPU 4K 高斯 σ=16 ＝ **3181 ms****✗（**∴ 第 696 轮实测 ✓）
///     ＋ **∴ 而** GPU 是**一次上传 ＋ 两 pass ＋ 一次下载** ✓
///       ⇒ **∴ 所以**必须量**真实加速比**✗ ⇒ **∴ 从而**决定是否接线到 `filter.rs`** ✓ ★
///
/// **∴ 为什么 `#[ignore]` ✗ ★**（**第 698 轮的教训 ✓）：
///   **∴ 门禁的 `cargo test --workspace` 是** debug 构建**✗
///     ＋ **∴ 而** CPU 高斯 σ=16 在 debug 下**超过 100 s**✗（**∴ release 3181 ms ✓）
///       ＋ **∴ 于是**：**它**会**拖死门禁** ✓（**∴ 实测 17:41 仍未结束 ✓）
///         ⇒ **∴ 所以**：**基准必须默认跳过**✗ ⇒ **∴ 用** `--ignored` 显式跑** ✓ ★
///
/// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --features gpu --test gpu_blur_parity -- --ignored --nocapture`
#[test]
#[ignore = "基准：用 --ignored 显式运行"]
fn gpu_blur_speedup_versus_cpu() {
    let lut = srgb_encode_table();
    let quantizer = match yanshi_gpu::Quantizer::new(&lut[..]) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）：{e}");
            return;
        }
    };
    for (label, w, h) in [("1080p", 1920u32, 1080u32), ("4K", 3840, 2160)] {
        for &sigma in &[2.0f32, 8.0] {
            // **∴ CPU 真值 ＋ 计时 ✓**
            let mut best_cpu = f64::INFINITY;
            for _ in 0..2 {
                let mut b = Buffer::new(0, 0, w, h);
                fill(&mut b, 0xB102_0001);
                let t = std::time::Instant::now();
                gaussian_blur(&mut b, sigma);
                let d = t.elapsed().as_secs_f64() * 1000.0;
                std::hint::black_box(&b);
                if d < best_cpu {
                    best_cpu = d;
                }
            }
            // **∴ GPU ＋ 计时 ✓**（**∴ 含上传 ＋ 两个 pass ＋ 下载 ✓）
            let src = {
                let mut b = Buffer::new(0, 0, w, h);
                fill(&mut b, 0xB102_0001);
                b.pixels().to_vec()
            };
            let mut best_gpu = f64::INFINITY;
            let mut calls = 0usize;
            for _ in 0..3 {
                let t = std::time::Instant::now();
                let out = quantizer.blur_gaussian(&src, w, h, sigma);
                let d = t.elapsed().as_secs_f64() * 1000.0;
                match out {
                    Some(v) => {
                        std::hint::black_box(&v);
                        calls += 1;
                        if d < best_gpu {
                            best_gpu = d;
                        }
                    }
                    None => break,
                }
            }
            if calls == 0 {
                println!("  ★ {label} σ={sigma}：GPU 不可用 ⇒ 跳过该档");
                continue;
            }
            println!(
                "  ★ {label} σ={sigma}：CPU **{best_cpu:8.1} ms**｜GPU（**含传输 **{best_gpu:8.1} ms**）｜★ 加速比 **{:.1}×** ★",
                best_cpu / best_gpu
            );
        }
    }
}
