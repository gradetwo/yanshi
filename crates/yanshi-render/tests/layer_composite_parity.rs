// **★★★ 判据：融合版 `composite_with_layer` 必须与「三步」**逐位一致** ✗ ★★★**（**第 637 轮 ✓）
//
// **∴ 为什么必须**逐位**而不只是 u8 ✗**：
//   **∴ 融合的风险是**乘法顺序**✗（**∴ 必须**先乘 opacity，与 `multiply_alpha` 相同 ✓）
//     ＋ **∴ 而**顺序错了**可能小到 u8 空间看不出来**✗
//       ⇒ **★ 所以：**必须在 f32 空间比**（**比 u8 更严 ✓）★
//
// **∴ 两条路径（**都要测 ✓）✗**：
//   **∴ A（**三步，真值 ✓）✗**：`[clip?] multiply_alpha_by ⇒ multiply_alpha ⇒ composite(…, 1.0)`
//     ＋ **∴ B（**融合 ✓）✗**：`[clip?] multiply_alpha_by ⇒ composite_with_layer(layer, mode, opacity)` ✓
//       ＋ **∴ 断言 ✗**：**两者的 f32 **位模式**完全相同** ✓ ★
use yanshi_render::blend::BlendMode;
use yanshi_render::buffer::Buffer;

fn fill(buf: &mut Buffer, seed: u32) {
    let mut st = seed | 1;
    for px in buf.pixels_mut().chunks_exact_mut(4) {
        st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let a = ((st >> 9) & 0xff) as f32 / 255.0;
        let b = ((st >> 17) & 0xff) as f32 / 255.0;
        // **∴ 预乘不变式 ✗**：**rgb ≤ alpha**（**与真实像素一致 ✓）★
        px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
    }
}

#[test]
fn fused_layer_composite_is_bit_exact_against_three_steps() {
    let (w, h) = (96u32, 64u32);
    let mut failures = Vec::new();
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
    for (mi, mode) in modes.into_iter().enumerate() {
        for &opacity in &[1.0f32, 0.5, 0.25, 0.0] {
            for &clip_on in &[false, true] {
                let mut layer = Buffer::new(0, 0, w, h);
                fill(&mut layer, 0x1234_5678 ^ (mi as u32) ^ opacity.to_bits());
                let mut acc = Buffer::new(0, 0, w, h);
                fill(&mut acc, 0x9e37_79b9);
                let mut mask = Buffer::new(0, 0, w, h);
                fill(&mut mask, 0x0bad_f00d);
                let clip = clip_on.then_some(&mask);
                // **∴ A：三步（**真值 ✓）★
                let mut a = acc.clone();
                let mut layer_a = layer.clone();
                if let Some(m) = clip {
                    layer_a.multiply_alpha_by(m);
                }
                layer_a.multiply_alpha(opacity);
                a.composite(&layer_a, mode, 1.0);
                // **∴ B：融合（**有 clip 时先做那一步 ✓）★
                let mut b = acc.clone();
                let mut layer_b = layer.clone();
                if let Some(m) = clip {
                    layer_b.multiply_alpha_by(m);
                }
                b.composite_with_layer(&layer_b, mode, opacity);
                // **∴ 逐位比较（**f32 位模式 ✓）★
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
                    failures.push(format!(
                        "mode={mode:?} opacity={opacity} clip={clip_on}：{mism} 个 f32 不同（首个 {first}）"
                    ));
                }
            }
        }
    }
    for f in &failures {
        println!("  ✗ {f}");
    }
    println!(
        "  共测 {} 组（10 mode × 4 opacity × 2 clip）｜失败 {} 组",
        modes.len() * 4 * 2,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "融合版必须与三步逐位一致（f32 位模式）"
    );
}
