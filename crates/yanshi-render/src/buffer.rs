//! 区域像素缓冲（设计文档 6.4 / 8.3）。
//!
//! Tile 是缓存与持久化的唯一物化形式；**渲染计算在区域缓冲区上进行**，
//! 这样滤镜、图层混合等需要邻域或整层的运算可以正确工作，最后再切片进 tile 缓存。

use crate::blend::{blend_pixel, over, BlendMode};
use crate::color::{composite_over_background, linear_premul_to_u8x4, premultiply, LinearRgba};
use crate::geometry::Coverage;
use yanshi_core::Bbox;

/// 线性光、预乘 alpha 的区域缓冲区（行主序）。
#[derive(Debug, Clone, PartialEq)]
pub struct Buffer {
    origin_x: i64,
    origin_y: i64,
    width: u32,
    height: u32,
    pixels: Vec<f32>,
}

impl Buffer {
    /// 全透明缓冲区，原点为文档坐标 `(origin_x, origin_y)`。
    pub fn new(origin_x: i64, origin_y: i64, width: u32, height: u32) -> Self {
        Self {
            origin_x,
            origin_y,
            width,
            height,
            pixels: vec![0.0; (width * height * 4) as usize],
        }
    }

    /// 单色填充缓冲区。
    pub fn filled(
        origin_x: i64,
        origin_y: i64,
        width: u32,
        height: u32,
        pixel: LinearRgba,
    ) -> Self {
        let mut buffer = Self::new(origin_x, origin_y, width, height);
        buffer.fill(pixel);
        buffer
    }

    /// 宽。
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// 高。
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// 原点（文档坐标）。
    pub const fn origin(&self) -> (i64, i64) {
        (self.origin_x, self.origin_y)
    }

    /// 缓冲区覆盖的文档范围。
    pub fn bbox(&self) -> Bbox {
        Bbox::new(
            self.origin_x as f64,
            self.origin_y as f64,
            self.width as f64,
            self.height as f64,
        )
    }

    /// 像素总数。
    pub fn len(&self) -> usize {
        (self.width * self.height) as usize
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// 原始 f32 切片（线性预乘）。
    /// 可变访问底层 f32 像素（用于就地量化；不改尺寸与原点）。
    pub fn pixels_mut(&mut self) -> &mut [f32] {
        &mut self.pixels
    }

    /// 只读访问底层 f32 像素（行优先，RGBA 预乘线性）。
    pub fn as_f32(&self) -> &[f32] {
        &self.pixels
    }

    /// 像素（越界返回全透明）。
    pub fn pixel(&self, x: u32, y: u32) -> LinearRgba {
        if x >= self.width || y >= self.height {
            return [0.0; 4];
        }
        let index = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[index],
            self.pixels[index + 1],
            self.pixels[index + 2],
            self.pixels[index + 3],
        ]
    }

    /// 写入像素（越界忽略）。
    pub fn set_pixel(&mut self, x: u32, y: u32, pixel: LinearRgba) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = ((y * self.width + x) * 4) as usize;
        self.pixels[index..index + 4].copy_from_slice(&pixel);
    }

    /// 以 source-over 叠加。
    pub fn blend(&mut self, x: u32, y: u32, source: LinearRgba) {
        let backdrop = self.pixel(x, y);
        self.set_pixel(x, y, over(source, backdrop));
    }

    /// 以指定混合模式叠加。
    pub fn blend_mode(&mut self, x: u32, y: u32, source: LinearRgba, mode: BlendMode) {
        if mode == BlendMode::Normal {
            // 热路径：预乘表示的 source-over 不需要反预乘。
            self.blend(x, y, source);
            return;
        }
        let backdrop = self.pixel(x, y);
        self.set_pixel(x, y, blend_pixel(mode, source, backdrop));
    }

    /// 直接索引的 source-over（调用方保证坐标已裁剪在缓冲区内）。
    ///
    /// 用于笔刷 stamping 等内层热路径：省去逐像素的边界判断与局部坐标换算。
    pub fn blend_at(&mut self, x: u32, y: u32, source: LinearRgba) {
        debug_assert!(x < self.width && y < self.height, "blend_at 越界");
        let index = ((y * self.width + x) * 4) as usize;
        let inverse = 1.0 - source[3];
        self.pixels[index] = source[0] + self.pixels[index] * inverse;
        self.pixels[index + 1] = source[1] + self.pixels[index + 1] * inverse;
        self.pixels[index + 2] = source[2] + self.pixels[index + 2] * inverse;
        self.pixels[index + 3] = source[3] + self.pixels[index + 3] * inverse;
    }

    /// 整体填充。
    pub fn fill(&mut self, pixel: LinearRgba) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk.copy_from_slice(&pixel);
        }
    }

    /// 用直通颜色 + 覆盖率合成（`opacity` 为整体不透明度）。
    pub fn fill_coverage(
        &mut self,
        coverage: &Coverage,
        color: LinearRgba,
        mode: BlendMode,
        opacity: f32,
    ) {
        let base = [
            color[0].clamp(0.0, 1.0),
            color[1].clamp(0.0, 1.0),
            color[2].clamp(0.0, 1.0),
            color[3].clamp(0.0, 1.0),
        ];
        // 只遍历「覆盖率网格 ∩ 本缓冲」——未裁剪的调用方（例如蒙版）也不会白跑。
        let doc_x0 = coverage.bbox.x as i64;
        let doc_y0 = coverage.bbox.y as i64;
        let start_x = (self.origin_x - doc_x0).max(0) as u32;
        let start_y = (self.origin_y - doc_y0).max(0) as u32;
        let end_x = (self.origin_x + self.width as i64 - doc_x0).min(coverage.width as i64);
        let end_y = (self.origin_y + self.height as i64 - doc_y0).min(coverage.height as i64);
        if end_x <= start_x as i64 || end_y <= start_y as i64 {
            return;
        }
        let opacity = opacity.clamp(0.0, 1.0);
        for y in start_y..end_y as u32 {
            let local_y = (doc_y0 + y as i64 - self.origin_y) as u32;
            let row = (y * coverage.width) as usize;
            for x in start_x..end_x as u32 {
                let coverage_value = coverage.data[row + x as usize];
                if coverage_value <= 0.0 {
                    continue;
                }
                let local_x = (doc_x0 + x as i64 - self.origin_x) as u32;
                let alpha = base[3] * coverage_value * opacity;
                let source = premultiply([base[0], base[1], base[2], alpha]);
                self.blend_mode(local_x, local_y, source, mode);
            }
        }
    }

    /// 以 `mode` 把整块 `source` 合成到自身对应位置（按文档坐标对齐）。
    pub fn composite(&mut self, source: &Buffer, mode: BlendMode, opacity: f32) {
        let factor = opacity.clamp(0.0, 1.0);
        for y in 0..source.height {
            for x in 0..source.width {
                let pixel = source.pixel(x, y);
                let scaled = [
                    pixel[0] * factor,
                    pixel[1] * factor,
                    pixel[2] * factor,
                    pixel[3] * factor,
                ];
                if scaled[3] <= 0.0 && mode == BlendMode::Normal {
                    continue;
                }
                let document_x = source.origin_x + x as i64;
                let document_y = source.origin_y + y as i64;
                let local_x = document_x - self.origin_x;
                let local_y = document_y - self.origin_y;
                if local_x < 0 || local_y < 0 {
                    continue;
                }
                let (local_x, local_y) = (local_x as u32, local_y as u32);
                if local_x >= self.width || local_y >= self.height {
                    continue;
                }
                self.blend_mode(local_x, local_y, scaled, mode);
            }
        }
    }

    /// 以 u8 RGBA（显示空间，直通 alpha）覆盖写入，用于 RasterPatch / import_image。
    ///
    /// `x`/`y` 为文档坐标左上角；`opacity` 为额外不透明度。
    pub fn blit_rgba8(
        &mut self,
        x: i64,
        y: i64,
        width: u32,
        height: u32,
        rgba8: &[u8],
        opacity: f32,
    ) {
        let factor = opacity.clamp(0.0, 1.0);
        for row in 0..height {
            for col in 0..width {
                let index = ((row * width + col) * 4) as usize;
                if index + 3 >= rgba8.len() {
                    return;
                }
                let document_x = x + col as i64;
                let document_y = y + row as i64;
                let local_x = document_x - self.origin_x;
                let local_y = document_y - self.origin_y;
                if local_x < 0 || local_y < 0 {
                    continue;
                }
                let (local_x, local_y) = (local_x as u32, local_y as u32);
                if local_x >= self.width || local_y >= self.height {
                    continue;
                }
                let bytes = [
                    rgba8[index],
                    rgba8[index + 1],
                    rgba8[index + 2],
                    rgba8[index + 3],
                ];
                let mut pixel = crate::color::u8x4_to_linear_premul(bytes);
                for value in pixel.iter_mut() {
                    *value *= factor;
                }
                self.blend(local_x, local_y, pixel);
            }
        }
    }

    /// 裁剪到文档坐标范围（不相交则返回空缓冲区）。
    pub fn crop(&self, bbox: &Bbox) -> Buffer {
        let x0 = (bbox.x.floor() as i64).max(self.origin_x);
        let y0 = (bbox.y.floor() as i64).max(self.origin_y);
        let x1 = ((bbox.x + bbox.w).ceil() as i64).min(self.origin_x + self.width as i64);
        let y1 = ((bbox.y + bbox.h).ceil() as i64).min(self.origin_y + self.height as i64);
        let width = (x1 - x0).max(0) as u32;
        let height = (y1 - y0).max(0) as u32;
        let mut out = Buffer::new(x0, y0, width, height);
        for y in 0..height {
            for x in 0..width {
                let source_x = (x0 + x as i64 - self.origin_x) as u32;
                let source_y = (y0 + y as i64 - self.origin_y) as u32;
                out.set_pixel(x, y, self.pixel(source_x, source_y));
            }
        }
        out
    }

    /// 导出为 u8 RGBA（显示空间）；`background` 为不透明底色。
    pub fn to_rgba8(&self, background: Option<[u8; 4]>) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len() * 4);
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel = self.pixel(x, y);
                let bytes = match background {
                    Some(bg) => composite_over_background(pixel, bg),
                    None => linear_premul_to_u8x4(pixel),
                };
                out.extend_from_slice(&bytes);
            }
        }
        out
    }

    /// 仅保留 alpha 通道，颜色置零（用于裁剪 / 蒙版测试）。
    pub fn keep_alpha_only(&mut self) {
        for chunk in self.pixels.chunks_exact_mut(4) {
            chunk[0] = 0.0;
            chunk[1] = 0.0;
            chunk[2] = 0.0;
        }
    }

    /// 乘以 alpha 系数（蒙版、图层不透明度）。
    pub fn multiply_alpha(&mut self, factor: f32) {
        let factor = factor.clamp(0.0, 1.0);
        for chunk in self.pixels.chunks_exact_mut(4) {
            for value in chunk.iter_mut() {
                *value *= factor;
            }
        }
    }

    /// 用另一缓冲区的 alpha 乘以自身（剪贴蒙版、图层蒙版）。
    pub fn multiply_alpha_by(&mut self, mask: &Buffer) {
        for y in 0..self.height {
            for x in 0..self.width {
                let document_x = self.origin_x + x as i64;
                let document_y = self.origin_y + y as i64;
                let mask_x = document_x - mask.origin_x;
                let mask_y = document_y - mask.origin_y;
                let alpha = if mask_x < 0 || mask_y < 0 {
                    0.0
                } else {
                    mask.pixel(mask_x as u32, mask_y as u32)[3]
                };
                let index = ((y * self.width + x) * 4) as usize;
                for value in self.pixels[index..index + 4].iter_mut() {
                    *value *= alpha;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::rect_coverage;

    #[test]
    fn buffer_pixel_roundtrip_and_bounds() {
        let mut buffer = Buffer::new(-2, -3, 4, 4);
        assert_eq!(buffer.origin(), (-2, -3));
        assert_eq!(buffer.bbox(), Bbox::new(-2.0, -3.0, 4.0, 4.0));
        buffer.set_pixel(1, 1, [0.25, 0.5, 0.5, 0.5]);
        assert_eq!(buffer.pixel(1, 1), [0.25, 0.5, 0.5, 0.5]);
        buffer.set_pixel(9, 9, [1.0; 4]);
        assert_eq!(buffer.pixel(9, 9), [0.0; 4]);
    }

    #[test]
    fn fill_coverage_respects_document_origin() {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        let coverage = rect_coverage(Bbox::new(1.0, 1.0, 2.0, 2.0));
        buffer.fill_coverage(&coverage, [1.0, 0.0, 0.0, 1.0], BlendMode::Normal, 1.0);
        assert_eq!(buffer.pixel(1, 1), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(0, 0)[3], 0.0);
        assert_eq!(buffer.pixel(3, 3)[3], 0.0);
    }

    #[test]
    fn composite_and_crop_follow_document_coordinates() {
        let mut base = Buffer::new(0, 0, 8, 8);
        let mut layer = Buffer::new(2, 2, 4, 4);
        layer.fill([0.0, 0.5, 0.0, 1.0]);
        base.composite(&layer, BlendMode::Normal, 1.0);
        assert_eq!(base.pixel(2, 2), [0.0, 0.5, 0.0, 1.0]);
        assert_eq!(base.pixel(5, 5), [0.0, 0.5, 0.0, 1.0]);
        assert_eq!(base.pixel(1, 1)[3], 0.0);

        let cropped = base.crop(&Bbox::new(3.0, 3.0, 2.0, 2.0));
        assert_eq!(cropped.bbox(), Bbox::new(3.0, 3.0, 2.0, 2.0));
        assert_eq!(cropped.pixel(0, 0), [0.0, 0.5, 0.0, 1.0]);
        assert!(base.crop(&Bbox::new(100.0, 100.0, 1.0, 1.0)).is_empty());
    }

    #[test]
    fn alpha_helpers_scale_premultiplied_pixels() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        buffer.set_pixel(0, 0, [0.4, 0.2, 0.0, 0.5]);
        buffer.multiply_alpha(0.5);
        assert_eq!(buffer.pixel(0, 0), [0.2, 0.1, 0.0, 0.25]);

        let mut mask = Buffer::new(0, 0, 1, 1);
        mask.set_pixel(0, 0, [0.0, 0.0, 0.0, 0.5]);
        let mut target = Buffer::new(0, 0, 1, 1);
        target.set_pixel(0, 0, [1.0, 0.0, 0.0, 1.0]);
        target.multiply_alpha_by(&mask);
        assert_eq!(target.pixel(0, 0), [0.5, 0.0, 0.0, 0.5]);
    }

    #[test]
    fn export_matches_display_space() {
        let mut buffer = Buffer::new(0, 0, 1, 2);
        buffer.set_pixel(0, 0, [1.0, 1.0, 1.0, 1.0]);
        buffer.set_pixel(0, 1, [0.0, 0.0, 0.0, 0.0]);
        let bytes = buffer.to_rgba8(Some([255, 255, 255, 255]));
        assert_eq!(&bytes[0..4], &[255, 255, 255, 255]);
        assert_eq!(&bytes[4..8], &[255, 255, 255, 255]);
        let transparent = buffer.to_rgba8(None);
        assert_eq!(&transparent[4..8], &[0, 0, 0, 0]);
    }
}
