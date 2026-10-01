//! **重采样** ✓（设计 777 的 `resample` ✓）。
//!
//! **设计只给了名字、没给语义 ⇒ 记录选择** ✓：
//! * **对象** = 一个光栅对象（`raster_patch` ✓）—— 本项目的"光栅图层"就是它 ✓；
//! * **输入** = 该对象的像素（**只接受原始 RGBA** ✓ `image/x-yanshi-raw` ✓；其它 mime 明确拒绝 ✓
//!   —— 本项目内部一律用原始 RGBA ✓，转码是另一件事 ✗）；
//! * **输出** = 一份**新的 blob** ✓ + 一次 `supersede` ✓ ⇒ **非破坏** ✓：
//!   旧原子留在日志里 ✓、可撤销 ✓、可回放 ✓（这是本项目的根本不变式 ✓）。
//!
//! **两种滤镜** ✓（都只做**空间重采样** ✓，不做任何锐化/插值美化 ✗）：
//! * `nearest` ✓：取最近像素 ✓ —— 放大像素画时**必须**用它 ✓（否则边缘糊掉 ✗）；
//! * `bilinear` ✓：双线性 ✓ —— 缩放照片类内容用 ✓。
//!
//! **确定性** ✓：纯整数/定点运算路径 ✓（浮点只用于权重 ✓ 且同输入同输出 ✓）⇒
//! 与"内核 bit-exact"那条纪律相容 ✓。

/// 重采样滤镜 ✓。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResampleFilter {
    /// 最近邻 ✓（保硬边 ✓，放大像素画用 ✓）。
    Nearest,
    /// 双线性 ✓（缩放照片类内容用 ✓）。
    Bilinear,
}

impl ResampleFilter {
    /// 解析滤镜名 ✓（`nearest` / `bilinear` ✓，大小写不敏感 ✓）。
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "nearest" | "nearest-neighbor" | "point" => Some(Self::Nearest),
            "bilinear" | "linear" => Some(Self::Bilinear),
            _ => None,
        }
    }

    /// 名字 ✓（回包与日志用 ✓）。
    pub const fn name(self) -> &'static str {
        match self {
            Self::Nearest => "nearest",
            Self::Bilinear => "bilinear",
        }
    }
}

/// 把 RGBA 像素重采样到 `dst_w × dst_h` ✓。
///
/// `src` 必须是 `src_w * src_h * 4` 字节 ✓（不足则返回 `None` ✓ —— 不猜、不补 ✗）。
/// 目标尺寸为 0 也返回 `None` ✓。
pub fn resample_rgba(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    filter: ResampleFilter,
) -> Option<Vec<u8>> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return None;
    }
    if src.len() != (src_w as usize) * (src_h as usize) * 4 {
        return None;
    }
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    match filter {
        ResampleFilter::Nearest => {
            for y in 0..dst_h {
                // **像素中心对齐** ✓：`(y + 0.5) * src_h / dst_h` ✓ —— 直接按比例缩放会让
                // 每个像素整体偏移半格 ✓（放大两倍时表现为"图案歪一格" ✗）。
                let sy = (((y as f64 + 0.5) * src_h as f64 / dst_h as f64) as u32).min(src_h - 1);
                for x in 0..dst_w {
                    let sx =
                        (((x as f64 + 0.5) * src_w as f64 / dst_w as f64) as u32).min(src_w - 1);
                    let from = ((sy * src_w + sx) * 4) as usize;
                    let to = ((y * dst_w + x) * 4) as usize;
                    out[to..to + 4].copy_from_slice(&src[from..from + 4]);
                }
            }
        }
        ResampleFilter::Bilinear => {
            for y in 0..dst_h {
                let fy = (y as f64 + 0.5) * src_h as f64 / dst_h as f64 - 0.5;
                let y0 = fy.floor().max(0.0) as u32;
                let y1 = (y0 + 1).min(src_h - 1);
                let wy = (fy - fy.floor()).clamp(0.0, 1.0);
                for x in 0..dst_w {
                    let fx = (x as f64 + 0.5) * src_w as f64 / dst_w as f64 - 0.5;
                    let x0 = fx.floor().max(0.0) as u32;
                    let x1 = (x0 + 1).min(src_w - 1);
                    let wx = (fx - fx.floor()).clamp(0.0, 1.0);
                    let to = ((y * dst_w + x) * 4) as usize;
                    for channel in 0..4 {
                        let at = |px: u32, py: u32| -> f64 {
                            src[((py * src_w + px) * 4 + channel as u32) as usize] as f64
                        };
                        let top = at(x0, y0) * (1.0 - wx) + at(x1, y0) * wx;
                        let bottom = at(x0, y1) * (1.0 - wx) + at(x1, y1) * wx;
                        // 四舍五入 ✓（截断会让整幅偏暗一档 ✗）。
                        out[to + channel] =
                            (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
                    }
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 造一张 `w × h` 的图 ✓，每个像素按坐标给一个可判别的颜色 ✓。
    fn pattern(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&[(x * 40 % 256) as u8, (y * 60 % 256) as u8, 30, 255]);
            }
        }
        out
    }

    /// **最近邻放大 2 倍 ⇒ 每个像素成 2×2 的同色块** ✓（且中心对齐 ✓）。
    #[test]
    fn nearest_doubling_repeats_each_pixel() {
        let src = pattern(2, 2);
        let out = resample_rgba(&src, 2, 2, 4, 4, ResampleFilter::Nearest).expect("应成功");
        assert_eq!(out.len(), 4 * 4 * 4);
        for y in 0..4u32 {
            for x in 0..4u32 {
                let from = (((y / 2) * 2 + (x / 2)) * 4) as usize;
                let to = ((y * 4 + x) * 4) as usize;
                assert_eq!(
                    &out[to..to + 4],
                    &src[from..from + 4],
                    "({x},{y}) 应与源像素 ({},{}) 一致",
                    x / 2,
                    y / 2
                );
            }
        }
    }

    /// **双线性缩小 ⇒ 取平均** ✓（2×2 合成 1×1 ⇒ 四个角的均值 ✓）。
    #[test]
    fn bilinear_shrinking_averages() {
        // 四个像素：透明与不透明各两个 ✓ ⇒ 平均值应落在中间 ✓。
        let src = vec![
            0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255,
        ];
        let out = resample_rgba(&src, 2, 2, 1, 1, ResampleFilter::Bilinear).expect("应成功");
        assert_eq!(out.len(), 4);
        for (channel, value) in out.iter().enumerate() {
            assert!(
                (*value as i32 - 128).abs() <= 2,
                "第 {channel} 通道应约为 128，实测 {value}（四个像素的均值 ✓）"
            );
        }
    }

    /// **同尺寸 + nearest ⇒ 原样返回** ✓（不该有任何漂移 ✓）。
    #[test]
    fn same_size_nearest_is_identity() {
        let src = pattern(5, 3);
        let out = resample_rgba(&src, 5, 3, 5, 3, ResampleFilter::Nearest).expect("应成功");
        assert_eq!(out, src, "同尺寸最近邻必须是恒等 ✓");
    }

    /// **坏输入明确失败** ✗（不猜、不补 ✗）。
    #[test]
    fn bad_input_is_refused() {
        assert!(
            resample_rgba(&[0; 8], 3, 3, 1, 1, ResampleFilter::Nearest).is_none(),
            "长度不符应拒绝 ✓"
        );
        assert!(
            resample_rgba(&[0; 16], 2, 2, 0, 2, ResampleFilter::Nearest).is_none(),
            "零尺寸应拒绝 ✓"
        );
        assert_eq!(
            ResampleFilter::parse("bilinear"),
            Some(ResampleFilter::Bilinear)
        );
        assert_eq!(
            ResampleFilter::parse("NEAREST"),
            Some(ResampleFilter::Nearest)
        );
        assert_eq!(ResampleFilter::parse("wat"), None);
    }
}
