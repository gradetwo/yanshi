// **★ A/B：取表一次 vs 每次取表 ✗ ★**（**第 723 轮 ✓；外部补丁的收益验证 ✓）
//
// **∴ 补丁声称 ~15% ✗**：**`u8x4_to_linear_premul` 每像素走 3 次 `OnceLock` 原子检查** ✓
//   ＋ **∴ 而**循环外取一次表 ⇒ **∴ 省掉那 3 次** ✓
//     ＋ **∴ 本基准**直接量这个差别**✗ ⇒ **∴ 从而**验证／否证那 15%** ✓ ★
//
// **∴ 用法 ✗**：`cargo test -p yanshi-render --release --test decode_table_bench -- --ignored --nocapture` ★
use std::time::Instant;
use yanshi_render::color::{
    srgb_decode_table, u8x4_to_linear_premul, u8x4_to_linear_premul_with_table,
};

#[test]
#[ignore = "基准：用 --ignored 显式运行"]
fn decode_table_versus_repeated_lookup() {
    // **∴ 512² ＝ 262 144 像素 ✓（**∴ 与补丁注释同规模 ✓）
    let pixels: Vec<[u8; 4]> = (0..262_144u32)
        .map(|i| {
            let b = i.wrapping_mul(2_654_435_761);
            [b as u8, (b >> 8) as u8, (b >> 16) as u8, (b >> 24) as u8]
        })
        .collect();
    const ROUNDS: usize = 5;
    // **∴ ① 旧路：每像素每次取表 ✓
    let mut best_old = f64::INFINITY;
    for _ in 0..ROUNDS {
        let t = Instant::now();
        let mut acc = 0.0f32;
        for p in &pixels {
            let out = u8x4_to_linear_premul(*p);
            acc += out[0] + out[1] + out[2] + out[3];
        }
        let d = t.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(acc);
        if d < best_old {
            best_old = d;
        }
    }
    // **∴ ② 新路：循环外取表一次 ✓
    let mut best_new = f64::INFINITY;
    for _ in 0..ROUNDS {
        let table = srgb_decode_table();
        let t = Instant::now();
        let mut acc = 0.0f32;
        for p in &pixels {
            let out = u8x4_to_linear_premul_with_table(table, *p);
            acc += out[0] + out[1] + out[2] + out[3];
        }
        let d = t.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(acc);
        if d < best_new {
            best_new = d;
        }
    }
    println!(
        "  ★ 512²（262 144 像素）｜每像素取表 **{best_old:.3} ms**｜取表一次 **{best_new:.3} ms**"
    );
    println!(
        "  ★ 提速 **{:.2}×**（**省 {:.1}%）",
        best_old / best_new,
        (1.0 - best_new / best_old) * 100.0
    );
}
