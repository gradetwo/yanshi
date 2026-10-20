//! **笔触总弧长**：`hokusai` 的 `pressure_shape` / `stroke_taper` 都靠它算进度。
//!
//! **为什么抽到 `yanshi-core`**（第三轮复审 **K-01**，P0）：原来这个计算只在
//! **服务端**（`tools.rs` 的 `prime_stroke_total_length`）做，**wasm 侧没有**
//! ⇒ `hokusai::BrushState::stroke_total_length` 在 wasm 里恒 `0.0`
//! ⇒ `stroke.rs:462` 的 `if state.stroke_total_length > 1e-…` 不成立
//! ⇒ **`pressure_shape` / `stroke_taper` 在 wasm 里不生效** ⇒ **双宿主输出不一致** ✗
//!
//! **为什么放 `yanshi-core` 而不是 `vendor/hokusai-core`**：vendor 是第三方代码，
//! 改它会让上游同步变难；而 `yanshi-core` 是**两个宿主都依赖**的crate
//! ⇒ 一处定义、两处调用 ⇒ 与「同一事实只写一处」一致 ✓

/// 折线的总弧长（`f32` 累加，与原来的 `.windows(2).map(...).sum::<f32>()` 逐位一致）。
///
/// **为什么保持 `f32` 累加顺序**：`pressure_shape` 是**浮点敏感**的（收锋曲线），
/// 改成 `f64` 再转会让已通过的量测漂移 ⇒ 这里**不改精度**，只改**位置**（从 server 移到 core）。
pub fn total_length(points: &[(f64, f64, f64)]) -> f32 {
    points
        .windows(2)
        .map(|w| {
            let dx = (w[1].0 - w[0].0) as f32;
            let dy = (w[1].1 - w[0].1) as f32;
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::total_length;

    #[test]
    fn straight_line_is_its_length() {
        let points = [(0.0, 0.0, 1.0), (3.0, 4.0, 1.0)];
        assert!(
            (total_length(&points) - 5.0).abs() < 1e-4,
            "3-4-5 直角三角的斜边"
        );
    }

    #[test]
    fn empty_and_single_are_zero() {
        assert_eq!(total_length(&[]), 0.0);
        assert_eq!(total_length(&[(1.0, 2.0, 1.0)]), 0.0);
    }

    #[test]
    fn multi_segment_sums() {
        let points = [(0.0, 0.0, 1.0), (3.0, 4.0, 1.0), (3.0, 14.0, 1.0)];
        assert!((total_length(&points) - 15.0).abs() < 1e-4, "5 + 10 = 15");
    }
}
