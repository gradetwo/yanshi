// **★★★ 判据：优化后的 `multiply_alpha_by` 必须与**朴素实现逐位一致** ✗ ★★★**（**第 642 轮 ✓）
//
// **∴ 朴素实现 ＝ **原实现照抄** ✗**（**∴ 它就是**真值** ✓）：
//   **∴ 每像素 ✗**：`alpha = if mask_x < 0 || mask_y < 0 { 0.0 } else { mask.pixel(mx,my)[3] }`
//     ＋ **∴ 然后** 4 个通道各乘 alpha** ✓ ★
//
// **∴ 必须逐位（**f32 位模式 ✓）✗**：**因为**优化把「逐像素取 alpha」换成了**直索引** ✗
//   ＋ **∴ 若**等价性推导有误（**如**区间端点差一 ✓）⇒ **∴ 只有**部分像素**会差 ⇒ **∴ 必须**全比** ✓ ★
//
// **∴ 覆盖四种情形 ✗**：
//   **∴ ① 同原点同尺寸（**快路径 ✓）
//     ＋ **∴ ② 不同原点（**部分重叠 ⇒ 左右两侧清零 ✓）
//       ＋ **∴ ③ mask 在左上方（**x_lo ＞ 0 ⇒ 左侧清零 ✓）
//         ＋ **∴ ④ mask 在右下方（**x_hi ＜ width ⇒ 右侧清零 ✓）
//           ＋ **∴ ⑤ mask 行完全在外（**整行清零 ✓）★
use yanshi_render::buffer::Buffer;

/// **∴ 一个布局用例 ✗**：**(dst 原点, mask 原点, dst 尺寸, mask 尺寸)** ✓
///   **∴ 定义别名 ✗**（**∴ 否则** `clippy::type_complexity` 会报 ✓）★
type Case = ((i64, i64), (i64, i64), (u32, u32), (u32, u32));

fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

/// **∴ 朴素参考实现 ✗**（**照抄原逻辑 ✓）★
fn multiply_alpha_by_naive(dst: &mut Buffer, mask: &Buffer) {
    let (w, h) = (dst.width(), dst.height());
    for y in 0..h {
        for x in 0..w {
            let document_x = dst.origin().0 + x as i64;
            let document_y = dst.origin().1 + y as i64;
            let mask_x = document_x - mask.origin().0;
            let mask_y = document_y - mask.origin().1;
            let alpha = if mask_x < 0 || mask_y < 0 {
                0.0
            } else {
                mask.pixel(mask_x as u32, mask_y as u32)[3]
            };
            let index = ((y * w + x) * 4) as usize;
            for value in dst.pixels_mut()[index..index + 4].iter_mut() {
                *value *= alpha;
            }
        }
    }
}

#[test]
fn optimized_multiply_alpha_by_is_bit_exact_against_naive() {
    // **∴ 五种布局 ✗**：(**dst 原点, mask 原点, dst 尺寸, mask 尺寸 ✓)★
    let cases: [Case; 5] = [
        ((0, 0), (0, 0), (64, 48), (64, 48)),   // ① 同原点同尺寸
        ((0, 0), (8, 6), (64, 48), (40, 30)),   // ② mask 在内部偏移
        ((10, 10), (0, 0), (64, 48), (30, 30)), // ③ mask 在左上方
        ((0, 0), (40, 30), (64, 48), (40, 40)), // ④ mask 在右下方（**越界 ✓）
        ((0, 0), (0, 200), (32, 32), (16, 16)), // ⑤ mask 行完全在外
    ];
    let mut failures = Vec::new();
    for (i, (d_org, m_org, d_size, m_size)) in cases.into_iter().enumerate() {
        let mut dst = Buffer::new(d_org.0, d_org.1, d_size.0, d_size.1);
        fill(&mut dst, 0x1234_5678 ^ (i as u32));
        let mut mask = Buffer::new(m_org.0, m_org.1, m_size.0, m_size.1);
        fill(&mut mask, 0x9e37_79b9 ^ (i as u32));
        // **∴ A：朴素真值 ★
        let mut a = dst.clone();
        multiply_alpha_by_naive(&mut a, &mask);
        // **∴ B：优化版 ★
        let mut b = dst.clone();
        b.multiply_alpha_by(&mask);
        let pa = a.pixels_mut().to_vec();
        let pb = b.pixels_mut().to_vec();
        let mism = pa
            .iter()
            .zip(pb.iter())
            .filter(|(x, y)| x.to_bits() != y.to_bits())
            .count();
        if mism != 0 {
            let first = pa
                .iter()
                .zip(pb.iter())
                .position(|(x, y)| x.to_bits() != y.to_bits())
                .unwrap_or(0);
            failures.push(format!("情形 {i}：{mism} 个 f32 不同（首个下标 {first}）"));
        }
    }
    for f in &failures {
        println!("  ✗ {f}");
    }
    println!("  共测 5 种布局｜失败 {} 种", failures.len());
    assert!(
        failures.is_empty(),
        "优化版必须与朴素实现逐位一致（f32 位模式）"
    );
}
