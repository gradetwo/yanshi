// **★★★ 微基准：填 4K 缓冲的几种写法 ✗ ★★★**（**第 661 轮 ✓）
//
// **∴ 上下文 ✗**（**第 659／660 轮 ✓）：
//   **∴ 现状 ✗**：`chunks_exact_mut(4)` ＋ `copy_from_slice(&[f32;4])` ✓
//     ＋ **∴ 4K 上 68.8 ms ⇒ **等效仅 ~1.9 GB/s****✗（**∴ 而内存带宽应 ≥ 10 GB/s ✓）
//       ＋ **∴ 而**触页／清零**都被实测否定**✗（**≤ 2.7 ms ✓）
//         ⇒ **∴ 所以**瓶颈是**写循环本身** ✓ ★
//
// **∴ 候选 ✗**：
//   **∴ ① 现状 ✗**：`chunks_exact_mut(4)` ＋ `copy_from_slice`
//   **∴ ② `u128` 宽写 ✗**：**把 `[f32;4]` 当**一个 16 字节值**写** ✓
//   **∴ ③ `vec![[f32;4]; n]` ✗**：**让分配器 ＋ 批量构造**，**再** `copy_from_slice` 到 `Vec<f32>`** ✓
//
// **∴ 注意 ✗**：`black_box` **触碰数据** ＋ **非零数据** ✓
use std::time::Instant;

const N: usize = 3840 * 2160 * 4;

fn a_chunks(pixel: [f32; 4]) -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    for c in v.chunks_exact_mut(4) {
        c.copy_from_slice(&pixel);
    }
    v
}

fn b_u128(pixel: [f32; 4]) -> Vec<f32> {
    let mut v = vec![0.0f32; N];
    let wide: u128 = unsafe { std::mem::transmute(pixel) };
    for c in v.chunks_exact_mut(4) {
        // **∴ 一次写 16 字节 ✓
        let bytes: &mut [u8; 16] = unsafe { &mut *(c.as_mut_ptr() as *mut [u8; 16]) };
        *bytes = wide.to_ne_bytes();
    }
    v
}

fn c_vec4(pixel: [f32; 4]) -> Vec<f32> {
    let packed: Vec<[f32; 4]> = vec![pixel; N / 4];
    let mut v = vec![0.0f32; N];
    // **∴ 从 `[f32;4]` 批量拷 ✓
    let src: &[f32] = unsafe { std::slice::from_raw_parts(packed.as_ptr() as *const f32, N) };
    v.copy_from_slice(src);
    v
}

#[test]
fn bench_wider_write() {
    let pixel = [0.25f32, 0.5, 0.75, 1.0];
    let rounds = 5usize;
    let mut ta = Vec::new();
    let mut tb = Vec::new();
    let mut tc = Vec::new();
    for _ in 0..rounds {
        let t = Instant::now();
        let a = a_chunks(pixel);
        ta.push(t.elapsed().as_secs_f64() * 1000.0);
        // **★★★ 读必须**移出计时** ✗ ★★★**（**第 661 轮自我修正 ✓）
        //   **∴ 原来 ✗**：**`black_box(…fold…)` **在计时之内**✗
        //     ＋ **∴ 于是**一次完整的 132.7 MB **读**被计入**✗
        //       ⇒ **∴ 测的**是**写 ＋ 读 ＝ 265 MB**✗ ⇒ **∴ 绝对值**偏高约 2×** ✓
        //         ＋ **∴ 现在 ✗**：**先停表，再 `black_box`** ✓ ★
        std::hint::black_box(a.iter().fold(0.0f32, |s, v| s + *v));
        let t = Instant::now();
        let b = b_u128(pixel);
        tb.push(t.elapsed().as_secs_f64() * 1000.0);
        // **★★★ 读必须**移出计时** ✗ ★★★**（**第 661 轮自我修正 ✓）
        //   **∴ 原来 ✗**：**`black_box(…fold…)` **在计时之内**✗
        //     ＋ **∴ 于是**一次完整的 132.7 MB **读**被计入**✗
        //       ⇒ **∴ 测的**是**写 ＋ 读 ＝ 265 MB**✗ ⇒ **∴ 绝对值**偏高约 2×** ✓
        //         ＋ **∴ 现在 ✗**：**先停表，再 `black_box`** ✓ ★
        std::hint::black_box(b.iter().fold(0.0f32, |s, v| s + *v));
        let t = Instant::now();
        let c = c_vec4(pixel);
        tc.push(t.elapsed().as_secs_f64() * 1000.0);
        // **★★★ 读必须**移出计时** ✗ ★★★**（**第 661 轮自我修正 ✓）
        //   **∴ 原来 ✗**：**`black_box(…fold…)` **在计时之内**✗
        //     ＋ **∴ 于是**一次完整的 132.7 MB **读**被计入**✗
        //       ⇒ **∴ 测的**是**写 ＋ 读 ＝ 265 MB**✗ ⇒ **∴ 绝对值**偏高约 2×** ✓
        //         ＋ **∴ 现在 ✗**：**先停表，再 `black_box`** ✓ ★
        std::hint::black_box(c.iter().fold(0.0f32, |s, v| s + *v));
    }
    let med = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let (ma, mb, mc) = (med(&mut ta), med(&mut tb), med(&mut tc));
    println!(
        "  ★ 4K｜① 现状 chunks + copy_from_slice 中位 {ma:.1} ms（{:.1} GB/s）",
        132.7 / ma * 1000.0 / 1024.0
    );
    println!(
        "  ★ 4K｜② u128 宽写                     中位 {mb:.1} ms（{:.1} GB/s）",
        132.7 / mb * 1000.0 / 1024.0
    );
    println!(
        "  ★ 4K｜③ vec![[f32;4]] + 批量拷        中位 {mc:.1} ms（{:.1} GB/s）",
        132.7 / mc * 1000.0 / 1024.0
    );
    println!(
        "  ⇒ 最快 {:?}｜相对现状省 {:.1} ms",
        if mb < mc && mb < ma {
            "②"
        } else if mc < ma {
            "③"
        } else {
            "①（现状已最优）"
        },
        ma - mb.min(mc)
    );
}
