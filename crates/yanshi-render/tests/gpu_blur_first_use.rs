// **★★★ 判据：**有 GPU 时，第一帧的大滤镜**也必须真的走 GPU**** ★★★**
//   （**第 246 轮 ✓；用户裁定「有 GPU 用就打开能加速且有收益的功能」✓）
//
// **∴ 用户裁定（**原文 ✓）✗**：
//   「**如果有 gpu 用 ✗ 那么就**打开 gpu 能加速且确实有收益的功能**」 ✓
//
// **★★★ 它守的那个**真缺口**（**∴ 本判据**先红后绿**✗ 有据 ✓）★★★**
//   **∴ 渲染的**顺序**是**：**先画对象／滤镜 ✗ **最后**才量化输出** ✓
//     ＋ **∴ 而**在此之前**只有**量化路**会创建设备**（`try_quantize_on_gpu_keyed` ✓）
//       ＋ **∴ 而**滤镜路**只**读那个进程级缓存** ✓
//         ⇒ **★ 所以 ✗**：**第一帧的大滤镜**看到**空缓存** ⇒ **∴ 回退 CPU** ✓
//           ⇒ **∴ 明明有 GPU ✗ 而**它**要等到**第二帧**才用得上** ✓ ★★
//   ＋ **∴ 实测代价 ✗**（**源码注释里的既有数 ✓；**第 699 轮 ✓）**：
//     **∴ 1080p σ=2 ⇒ GPU **2.0×**｜σ=8 ⇒ **3.6×**｜4K σ=2 ⇒ **1.5×**｜σ=8 ⇒ **1.1×** ✓
//       ⇒ **∴ 所以**：**第一帧白付的就是这个倍数** ✓ ★
//
// **∴ 修法 ✗**：**创建逻辑**抽成 `buffer::ensure_quantizer`**✗
//   ⇒ **∴ 滤镜路**也用它** ⇒ **∴ 谁先要用**谁创建** ✓（**∴ 两处各写一遍 ⇒ **迟早漂移** ✓）★
//
// **∴ 本判据守**两个方向**✗ ★**：
//   **∴ ① 默认（**允许用 GPU ✓）⇒ **空缓存下的大滤镜**必须**真的走 GPU**** ✗
//     ⇒ **∴ 谁把这条优化拿掉**（**或**改回"只读缓存" ✓）⇒ **∴ 它**必红** ✓ ★★
//   **∴ ② `--gpu off`（**`set_gpu_disabled(true)` ✓）⇒ **一次都不许走 GPU**** ✗
//     ⇒ **∴ 于是**：**"加上 create"**不许**把 `off` 变成**假话** ✓
//       （**∴ 目标第 7 条：**不许**静默用 GPU ✓）★★
//
// **∴ 为什么**单独一个测试文件**✗**：**它**改进程级全局状态**（`set_gpu_disabled` ✓）
//   ＋ **∴ 而** cargo 的测试**同进程多线程**✗ ⇒ **∴ 放进公共文件会**污染别的判据** ✓ ★★
//
// **∴ 跑法 ✗**：`cargo test -p yanshi-render --features gpu --test gpu_blur_first_use`
// **∴ 没有适配器时 ✗**：**如实跳过并打印**✗（**∴ 不是**假装通过 ✓ —— **∴ 本仓的纪律 ✓）
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
fn the_first_big_blur_uses_the_gpu_when_one_is_available() {
    // **∴ 尺寸必须 ≥ 阈值 2 M**✗（**∴ 否则**本来就走 CPU ⇒ **∴ 判据**什么都没测** ✓）★
    //   ＋ **∴ 1920×1080 ＝ 2 073 600 ≥ 2 000 000** ✓
    let (w, h) = (1920u32, 1080u32);
    let lut = srgb_encode_table();
    // **∴ 先问一句"有没有适配器" ✗**（**∴ 用的是**临时实例**✗ **不写缓存 ✓）★
    //   ＋ **∴ 必须**把它留到测试结束**✗ ★**（**∴ L4 实测 ✓）**：
    //     **∴ 早一步 drop 它**✗ ⇒ **∴ 进程**退出时** SIGABRT** ✓
    //       ＋ **∴ 报错行 ✗**：`_glthread_: thread failed to set thread specific data: Success` ✓
    //         ⇒ **∴ 那**是 Mesa／GL 的线程收尾问题**✗ 而**不是判据失败** ✓
    //           （**∴ 判据本身**已经 `1 passed`**✗ 而** cargo 仍按进程退出码报错 ✓）★★
    //     **∴ 而**同进程里**两个 device 都活着**（**如同 `gpu_blur_parity` 的 `_quantizer` ✓）
    //       ⇒ **∴ 退出**干净** ✓ ⇒ **∴ 所以**：**留一个具名变量**✗ 而**不要**在 `if` 里临时判** ✓ ★★
    let probe = yanshi_gpu::Quantizer::new(&lut[..]);
    if probe.is_err() {
        eprintln!("⚠️ 本机 GPU 不可用 ⇒ 跳过（**不是通过**）");
        return;
    }
    // **∴ 保证本判据从**空缓存**开始 ✗**（**∴ 那**正是"第一帧"的语义 ✓）★
    //   ＋ **∴ 若**同一进程里**已经有别的判据**填过它**✗ ⇒ **∴ 本判据**测不到那个缺口** ✓
    //     ⇒ **∴ 所以**：**先**清一次**✗ 并**打印它是不是真的空了** ✓ ★★
    let was_empty = yanshi_render::buffer::clear_quantizer_cache_for_test();
    println!("  ★ 起始缓存为空：{was_empty}（**本判据要的就是空 ✓）");

    // ── 方向 ①：默认（**允许 GPU ✓）＋ **空缓存** ⇒ **第一帧的大滤镜必须走 GPU** ──
    yanshi_render::set_gpu_disabled(false);
    yanshi_render::filter::reset_blur_degrade();
    let before = yanshi_gpu::blur_gpu_calls();
    let mut buf = Buffer::new(0, 0, w, h);
    fill(&mut buf, 0xB102_0001);
    gaussian_blur(&mut buf, 2.0);
    let used = yanshi_gpu::blur_gpu_calls() > before;
    println!("  ★ 第一帧（**空缓存 ✓）1080p σ=2 ⇒ 走了 GPU：{used}");
    assert!(
        used,
        "有 GPU 而**第一帧的大滤镜**没走 GPU ⇒ \
         说明它还在等量化路去创建设备（用户裁定「有 gpu 用就打开能加速且有收益的功能」）"
    );

    // ── 方向 ②：`--gpu off` ⇒ **一次都不许走 GPU**（**∴ 于是**"加上创建"不会让 off 变假话 ✓）──
    yanshi_render::set_gpu_disabled(true);
    yanshi_render::filter::reset_blur_degrade();
    let before_off = yanshi_gpu::blur_gpu_calls();
    let mut buf = Buffer::new(0, 0, w, h);
    fill(&mut buf, 0xB102_0002);
    gaussian_blur(&mut buf, 2.0);
    let used_off = yanshi_gpu::blur_gpu_calls() > before_off;
    yanshi_render::set_gpu_disabled(false);
    println!("  ★ --gpu off（**set_gpu_disabled(true) ✓）1080p σ=2 ⇒ 走了 GPU：{used_off}");
    assert!(
        !used_off,
        "`--gpu off` 之下仍然走了 GPU ⇒ **∴ 那个参数**说了假话**（目标第 7 条）"
    );

    // ── 方向 ③：**门槛按适配器分档**（**第 246 轮 ✓；用户裁定 ✓）──
    //   ＋ **∴ 这一档守的是 ✗**：**0.52 M（**960×540 ✓）**在**独显上必须走 GPU**✗
    //     **而**在核显上必须**不走** ✓ ⇒ **∴ 判据**按缓存的门槛值自己选边** ✓（**∴ 两个方向都有牙 ✓）★★
    let threshold = yanshi_render::filter::blur_min_pixels_for_cached_device()
        .expect("方向①已经创建过设备 ⇒ 这里应当读得到门槛");
    let (w2, h2) = (960u32, 540u32); // 0.52 M
    let count2 = (w2 as usize) * (h2 as usize);
    yanshi_render::filter::reset_blur_degrade();
    let before_small = yanshi_gpu::blur_gpu_calls();
    let mut buf = Buffer::new(0, 0, w2, h2);
    fill(&mut buf, 0xB102_0003);
    gaussian_blur(&mut buf, 2.0);
    let used_small = yanshi_gpu::blur_gpu_calls() > before_small;
    let should = count2 >= threshold;
    println!(
        "  ★ 门槛 {threshold} px｜960x540 ＝ {count2} px ⇒ 应当走 GPU：{should}｜实际走了：{used_small}"
    );
    assert_eq!(
        used_small, should,
        "960×540 的取舍与缓存设备的门槛不一致（门槛 {threshold}）⇒ \
         分档没生效，或门槛与实现不同源"
    );
}
