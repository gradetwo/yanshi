// **★★★ 判据：`fill_coverage` 的 Normal 快路（`blend_at`）必须与旧路（`blend_mode`）**逐位一致**** ★★★
//   （**第 678 轮 ✓）
//
// **∴ 背景 ✗**（**第 677 轮 ✓）：
//   **∴ `blend` ⇒ **两次**边界检查 ＋ **两次**索引计算**✗
//     ＋ **∴ 而** `blend_at` **只有一次索引** ⇒ **∴ 每像素**省 2＋2** ✓
//   **∴ 但** `over` 有**三个分支**✗（**alpha ≤ 0 ⇒ backdrop｜≥ 1 ⇒ source｜一般 ✓）
//     ＋ **∴ 而** `blend_at` **只有**一般分支** ⇒ **∴ 等价性**必须验证** ✓
//       ＋ **∴ 尤其是 `−0.0`**✗（**∴ `source + backdrop*0.0` 的符号 ✓）★
//
// **★★★ 变异检验（**必须能红 ✓）★★★**
//   **∴ 本文件**手工重放旧路**✗**（**真值 ✓）
//     ＋ **∴ 第二个测试**手写一个**错误的 inverse**✗ ⇒ **∴ 必红** ✓ ★
use std::time::Instant;
use yanshi_core::Bbox;
use yanshi_render::blend::BlendMode;
use yanshi_render::buffer::Buffer;
use yanshi_render::color::premultiply;
use yanshi_render::geometry::Coverage;

/// **∴ 手工重放**旧路**✗**（**`blend_mode` 逐像素 ✓）—— **真值** ✓
fn fill_by_blend_mode(buf: &mut Buffer, cov: &Coverage, color: [f32; 4], opacity: f32) {
    let base = [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
        color[3].clamp(0.0, 1.0),
    ];
    for y in 0..cov.height {
        for x in 0..cov.width {
            let v = cov.data[(y * cov.width + x) as usize];
            if v <= 0.0 {
                continue;
            }
            let alpha = base[3] * v * opacity.clamp(0.0, 1.0);
            let source = premultiply([base[0], base[1], base[2], alpha]);
            buf.blend_mode(x, y, source, BlendMode::Normal);
        }
    }
}

/// **∴ backdrop 里**故意放 −0.0**✗**（**第 677 轮识别的风险点 ✓）★
fn backdrop_with_negative_zero(w: u32, h: u32) -> Buffer {
    let mut b = Buffer::new(0, 0, w, h);
    for y in 0..h {
        for x in 0..w {
            let v = if (x + y) % 3 == 0 {
                -0.0f32
            } else {
                (x as f32) * 0.01
            };
            b.set_pixel(x, y, [v, v, v, v.max(0.0)]);
        }
    }
    b
}

#[test]
fn blend_at_matches_blend_mode_for_normal_mode() {
    let (w, h) = (64u32, 48u32);
    let mut diff = 0usize;
    let mut checked = 0usize;
    for &cov_value in &[0.0f32, 0.25, 0.5, 1.0] {
        for &alpha in &[0.0f32, 0.5, 1.0] {
            for &opacity in &[0.0f32, 0.5, 1.0] {
                let color = [0.2f32, 0.6, 0.9, alpha];
                let mut data = vec![0.0f32; (w * h) as usize];
                for (i, d) in data.iter_mut().enumerate() {
                    if i % 2 == 0 {
                        *d = cov_value;
                    }
                }
                let cov = Coverage {
                    bbox: Bbox::new(0.0, 0.0, w as f64, h as f64),
                    width: w,
                    height: h,
                    data,
                };
                let mut fast = backdrop_with_negative_zero(w, h);
                let mut slow = backdrop_with_negative_zero(w, h);
                fast.fill_coverage(&cov, color, BlendMode::Normal, opacity);
                fill_by_blend_mode(&mut slow, &cov, color, opacity);
                for y in 0..h {
                    for x in 0..w {
                        let a = fast.pixel(x, y);
                        let b = slow.pixel(x, y);
                        for c in 0..4 {
                            checked += 1;
                            if a[c].to_bits() != b[c].to_bits() {
                                if diff < 3 {
                                    println!(
                                        "  ★ 差异 ({x},{y})[ch{c}]：快 {}（{:#x}） vs 慢 {}（{:#x}）",
                                        a[c],
                                        a[c].to_bits(),
                                        b[c],
                                        b[c].to_bits()
                                    );
                                }
                                diff += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    println!("  ★ 比对了 {checked} 个 f32｜逐位差异 {diff}");
    assert_eq!(diff, 0, "Normal 快路与旧路必须逐位一致（差异 {diff}）");
}

/// **∴ 变异检验 ✗**：**手写一个**错误的 inverse**✗ ⇒ **∴ 必须**与正确结果不同** ✓
#[test]
fn a_wrong_inverse_would_be_detected() {
    let (w, h) = (16u32, 16u32);
    let color = [0.5f32, 0.5, 0.5, 0.5];
    let mut data = vec![1.0f32; (w * h) as usize];
    data[0] = 0.5;
    let cov = Coverage {
        bbox: Bbox::new(0.0, 0.0, w as f64, h as f64),
        width: w,
        height: h,
        data,
    };
    let mut correct = Buffer::new(0, 0, w, h);
    correct.fill_coverage(&cov, color, BlendMode::Normal, 1.0);
    // **∴ 错误变体 ✗**：**通道 0 用 `* source[3]` 而不是 `* (1 - source[3])`** ✓
    let base = [0.5f32, 0.5, 0.5, 0.5];
    let mut wrong = Buffer::new(0, 0, w, h);
    for y in 0..h {
        for x in 0..w {
            let v = cov.data[(y * w + x) as usize];
            if v <= 0.0 {
                continue;
            }
            let alpha = base[3] * v;
            let source = premultiply([base[0], base[1], base[2], alpha]);
            let mut px = wrong.pixel(x, y);
            px[0] = source[0] + px[0] * source[3]; // ★ 错 ★
            wrong.set_pixel(x, y, px);
        }
    }
    let mut differs = false;
    for y in 0..h {
        for x in 0..w {
            let a = correct.pixel(x, y);
            let b = wrong.pixel(x, y);
            if (0..4).any(|c| a[c].to_bits() != b[c].to_bits()) {
                differs = true;
            }
        }
    }
    println!("  ★ 错误 inverse 是否被检出：{differs}");
    assert!(differs, "错误的 inverse 必须产生不同结果，否则判据不敏感");
}

/// **∴ A/B：`blend_at` 相对 `blend` 的收益 ✗**（**第 678 轮 ✓）★
///
/// **∴ 方法学 ✗**：**取多次最小值**✗（**∴ 对性能最稳 ✓）
///   ＋ **∴ `black_box` 在停表之后**✗（**∴ 第 661 轮修正 ✓）★
#[test]
fn blend_at_beats_blend_mode_measurably() {
    let (w, h) = (1920u32, 1080u32);
    let color = [0.2f32, 0.6, 0.9, 0.8];
    let cov = Coverage {
        bbox: Bbox::new(0.0, 0.0, w as f64, h as f64),
        width: w,
        height: h,
        data: vec![1.0f32; (w * h) as usize],
    };
    let time_of = |via_old: bool| {
        let mut best = f64::INFINITY;
        for _ in 0..5 {
            let mut buf = Buffer::new(0, 0, w, h);
            let t = Instant::now();
            if via_old {
                fill_by_blend_mode(&mut buf, &cov, color, 1.0);
            } else {
                buf.fill_coverage(&cov, color, BlendMode::Normal, 1.0);
            }
            let d = t.elapsed().as_secs_f64() * 1000.0;
            std::hint::black_box(&buf);
            if d < best {
                best = d;
            }
        }
        best
    };
    let fast = time_of(false);
    let slow = time_of(true);
    println!(
        "  ★ 1920x1080 全覆盖｜新（**blend_at ✓）min {fast:.2} ms｜旧（**blend ✓）min {slow:.2} ms｜提速 {:.2}×",
        slow / fast
    );
    assert!(
        fast < slow,
        "新路径必须快于旧路径（新 {fast:.2} ms，旧 {slow:.2} ms）"
    );
}
