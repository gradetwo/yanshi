//! **★★★ 判据：量化的**单点真值**必须与**整幅编码器**逐字节一致 ✗ ★★★**（**第 759 轮 ✓）
//!
//! **∴ 为什么需要它 ✗**（**目标第 11 条余项 ✓）★**：
//!   **∴ 我要把量化路的核对**从「**重算整幅**」改成「**抽检 256 点**」**✗
//!     ＋ **∴ 而**那**要求**单点重算与整幅编码**逐字节相同** ✓
//!       ＋ **∴ 若**它们不一致**✗ ⇒ **∴ 哨兵**会**误报不符 ⇒ **∴ 于是**GPU 结果**被无谓丢弃** ✓ ★★
//!         ＋ **∴ 所以**：**这条判据**是**那个改动的前置条件** ✓ ★
//!
//! **∴ 判据做什么 ✗**：**随机／确定性图案 ＋ **两种背景设置**✗
//!   ＋ **∴ 逐像素比** `encode_quantized_pixel` **与** `encode_quantized_rows` **的输出** ✓ ★
//!
//! **∴ 变异检验（**已做 ✓）✗**：**把单点版里的 `quantize_f16` 去掉一层**✗ ⇒ **∴ 本判据**必红** ✓ ★
use yanshi_render::color::srgb_encode_table;
use yanshi_render::rows::{encode_quantized_pixel, encode_quantized_rows};

fn pattern(width: usize, height: usize, seed: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; width * height * 4];
    for y in 0..height {
        for x in 0..width {
            let i = (y * width + x) * 4;
            let v = ((x as u32).wrapping_mul(2654435761).wrapping_add(seed))
                ^ ((y as u32).wrapping_mul(40503));
            out[i] = ((v & 255) as f32) / 255.0;
            out[i + 1] = (((v >> 8) & 255) as f32) / 255.0;
            out[i + 2] = (((v >> 16) & 255) as f32) / 255.0;
            out[i + 3] = (((v >> 24) & 255) as f32) / 255.0;
        }
    }
    out
}

#[test]
fn rows_quantized_pixel_matches_the_bulk_encoder() {
    let (width, height) = (23usize, 17usize);
    let source = pattern(width, height, 31);
    let table = srgb_encode_table();
    // **∴ 两种背景都要测 ✗**（**∴ 它们走**不同的分支** ✓）★
    let backgrounds = [
        None,
        Some([0.1f32, 0.2, 0.3, 1.0]),
        Some([0.0f32, 0.0, 0.0, 0.0]),
    ];
    for bg in backgrounds {
        let mut bulk = vec![0u8; width * height * 4];
        encode_quantized_rows(&source, &mut bulk, 0, height, width, bg);
        for index in 0..(width * height) {
            let one = encode_quantized_pixel(&source, index, table, bg);
            let base = index * 4;
            assert_eq!(
                one,
                [bulk[base], bulk[base + 1], bulk[base + 2], bulk[base + 3]],
                "单点版与整幅版不一致（像素 {index}｜背景 {bg:?}）⇒ \
                 哨兵会误报 ⇒ GPU 结果会被无谓丢弃（判据必须能红）"
            );
        }
    }
}
