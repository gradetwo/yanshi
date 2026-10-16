//! **★ 多边形覆盖度的**正确性**判据 ✗ ★**（2026-10-16 ✓）
//!
//! **∴ 为什么需要它 ✗**（**实测为据 ✓**）：本仓已有一条"快路 vs 旧路真值"的**对拍**判据
//!   （`geometry.rs` 里的 A/B 探针 ＋ `polygon_coverage_fast_path` ✓），
//!   但**它只验"两条路一致"✗，不验"结果对不对"✗** ——
//!   **∴ 两条路可以"一致地错"** ✗（**实测见下 ✓**）。
//!   **∴ 本轮实测证据 ✗**：三角形 `[(20,20),(180,20),(100,130)]` 上，
//!   A/B 探针报「**逐位一致=true**」✓，而两条路算出来的覆盖度之和是 **8800** ✓
//!   （**恰为 bbox 面积 17600 的一半 ⇒ 这才是对的 ✓**）；问题出在**别处**（服务端下游 ✓），
//!   **∴ 但那条对拍判据当时并不能替我们分辨"引擎对／错"** ✗ —— 它无法排除"一致地错" ✓。
//! **∴ 本判据补的正是这一课 ✓**：钉死"**算得对**"的几条硬事实 ✓。
//!
//! **∴ 判什么 ✗**（**三条，各自都能独立红 ✓**）：
//!   ① **三角形覆盖度之和 ＝ bbox 面积的一半** ✓（正确栅格化的签名 ✓；矩形则是 **全满** ✓）；
//!   ② **内点在多边形内、外点在外** ✓（`point_in_polygon` ✓ —— 含"bbox 内但三角形外"那个点 ✓）；
//!   ③ **最大覆盖度为 1.0** ✓（不能全 < 1 ✓ —— 否则"半透明"会冒充"画了" ✗）。
use yanshi_render::geometry::{point_in_polygon, polygon_coverage};

#[test]
fn triangle_coverage_is_exactly_half_of_its_bbox() {
    let triangle = [(20.0, 20.0), (180.0, 20.0), (100.0, 130.0)];
    let coverage = polygon_coverage(&triangle, 4);
    let expected_bbox = (180.0 - 20.0) * (130.0 - 20.0);
    assert_eq!(coverage.width, 160, "bbox 宽度必须来自**全部点** ✓");
    assert_eq!(coverage.height, 110, "bbox 高度必须来自**全部点** ✓");
    let sum: f32 = coverage.data.iter().copied().sum();
    let expected_half = (expected_bbox / 2.0) as f32;
    assert!(
        (sum - expected_half).abs() < 1.0,
        "三角形覆盖度之和必须 ≈ bbox 面积的一半（实测 {sum} ✓，期望 ≈ {expected_half} ✓）\
         ⇒ 若明显偏小 ⇒ 栅格化漏了面积 ✓；若等于 {expected_bbox} ⇒ 退化成了矩形 ✓"
    );
    let max = coverage.data.iter().copied().fold(0.0f32, f32::max);
    assert!(
        (max - 1.0).abs() < 1e-6,
        "最大覆盖度必须是 1.0（实测 {max} ✓）⇒ 否则整块偏淡 ✓"
    );
}

#[test]
fn triangle_interior_and_exterior_are_classified() {
    let triangle = [(20.0, 20.0), (180.0, 20.0), (100.0, 130.0)];
    assert!(point_in_polygon(100.0, 40.0, &triangle), "三角形内的点必须判为**内** ✓");
    assert!(point_in_polygon(100.0, 110.0, &triangle), "靠近底边的内点也必须判为**内** ✓");
    assert!(
        !point_in_polygon(30.0, 120.0, &triangle),
        "**bbox 内、但三角形外**的点必须判为**外** ✓（这条正是「只按矩形」会漏掉的那类点 ✓）"
    );
    assert!(!point_in_polygon(5.0, 140.0, &triangle), "完全在外的点必须判为**外** ✓");
}

#[test]
fn rectangle_polygon_is_fully_covered() {
    let rectangle = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
    let coverage = polygon_coverage(&rectangle, 4);
    let sum: f32 = coverage.data.iter().copied().sum();
    assert!(
        (sum - 100.0).abs() < 1e-3,
        "矩形多边形应当**全满**（实测 {sum} ✓，期望 100 ✓）"
    );
}
