//! EXIF Orientation（1–8）的**共用索引映射**：唯一的事实来源。
//!
//! 变换表按设计《RAW 导入》5.7 定版（与 TIFF/EXIF 规范一致）：
//! 1 恒等；2 水平翻转；3 旋转 180°；4 垂直翻转；5 转置（主对角线）；
//! 6 顺时针 90°；7 反转置；8 逆时针 90°。5/6/7/8 时输出宽高互换。
//!
//! 为什么放内核（yanshi-core）：消费方有两处且依赖方向不许成环——
//! `yanshi-render::jpeg`（u8 RGBA 路）与 `yanshi-raw` 的展开链（f32 线性路，
//! render 依赖 raw、不许反向依赖）。映射本身只是坐标算术、无像素类型，
//! 落在共同下游的核心层，两边共用一份、不许各写一份后漂移。

/// 方向变换后的输出尺寸 `(宽, 高)`：5–8 时宽高互换，其余不变。
/// `orientation` 不在 1..=8 时按 1（恒等）处理——与各消费方既有兜底口径一致
/// （方向缺失／损坏按 1，见设计 5.7 与 jpeg.rs 的 `exif_orientation` 兜底）。
pub fn output_size(width: usize, height: usize, orientation: u32) -> (usize, usize) {
    match orientation {
        5..=8 => (height, width),
        _ => (width, height),
    }
}

/// 输出像素 `(x, y)` 应取自源图哪个像素 `(sx, sy)`。
///
/// 源尺寸为 `(width, height)`；`(x, y)` 必须落在 [`output_size`] 给出的
/// 输出尺寸内。`orientation` 不在 1..=8 时按 1 处理（同上）。
pub fn source_coord(
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    orientation: u32,
) -> (usize, usize) {
    let (w, h) = (width, height);
    match orientation {
        2 => (w - 1 - x, y),
        3 => (w - 1 - x, h - 1 - y),
        4 => (x, h - 1 - y),
        5 => (y, x),
        6 => (y, h - 1 - x),
        7 => (w - 1 - y, h - 1 - x),
        8 => (w - 1 - y, x),
        _ => (x, y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_size_swaps_only_for_5_to_8() {
        for orientation in 1..=4 {
            assert_eq!(output_size(3, 2, orientation), (3, 2), "方向 {orientation}");
        }
        for orientation in 5..=8 {
            assert_eq!(output_size(3, 2, orientation), (2, 3), "方向 {orientation}");
        }
        // 非法值按 1（恒等）兜底，不许 panic、不许互换。
        assert_eq!(output_size(3, 2, 0), (3, 2));
        assert_eq!(output_size(3, 2, 9), (3, 2));
    }

    #[test]
    fn source_coord_is_a_bijection_for_every_orientation() {
        // 每个方向都必须是置换：输出每个像素恰好取一个不同源像素
        // （漏一个／重一个都会让画面缺块或重影，单点断言抓不住，要断集合）。
        for orientation in 1..=8 {
            let (w, h) = (3usize, 2usize);
            let (out_w, out_h) = output_size(w, h, orientation);
            let mut seen = std::collections::BTreeSet::new();
            for y in 0..out_h {
                for x in 0..out_w {
                    let (sx, sy) = source_coord(x, y, w, h, orientation);
                    assert!(sx < w && sy < h, "方向 {orientation} 取到界外源像素");
                    assert!(
                        seen.insert((sx, sy)),
                        "方向 {orientation} 的源像素 ({sx},{sy}) 被取了两次"
                    );
                }
            }
            assert_eq!(seen.len(), w * h, "方向 {orientation} 不是全置换");
        }
    }

    #[test]
    fn known_mappings_match_the_exif_table() {
        // 非对称尺寸 3×2，逐方向钉一个代表点（与设计 5.7 变换表手算对照）。
        let (w, h) = (3usize, 2usize);
        assert_eq!(source_coord(0, 0, w, h, 1), (0, 0));
        assert_eq!(source_coord(0, 0, w, h, 2), (2, 0)); // 水平翻转：左上取源右上
        assert_eq!(source_coord(0, 0, w, h, 3), (2, 1)); // 180°：左上取源右下
        assert_eq!(source_coord(0, 0, w, h, 4), (0, 1)); // 垂直翻转：左上取源左下
        assert_eq!(source_coord(0, 0, w, h, 5), (0, 0)); // 转置：对角线不动点
        assert_eq!(source_coord(1, 0, w, h, 5), (0, 1)); // 转置：(x,y)↦(y,x)
        assert_eq!(source_coord(0, 0, w, h, 6), (0, 1)); // 顺时针 90°：左上取源左下
        assert_eq!(source_coord(0, 0, w, h, 7), (2, 1)); // 反转置：左上取源右下
        assert_eq!(source_coord(0, 0, w, h, 8), (2, 0)); // 逆时针 90°：左上取源右上
    }
}
