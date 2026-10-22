//! JPEG 解码（`jpeg-decoder` 纯 Rust 依赖 ✓）+ EXIF 方向最小解析 ✓。
//!
//! **为什么用外部依赖** ✓：JPEG 的 DCT/Huffman 解码器不在本仓库自研范围内 ✓
//! （与 PSD 走 `ag-psd` 同理 ✓）。**依赖代价**：`jpeg-decoder 0.3`，纯 Rust、
//! 传递依赖为 rayon 一系（rayon/rayon-core/crossbeam-*/either，见 Cargo.lock——
//! 旧说法"传递依赖 0"与 lock 实测不符，以 lock 为准 ✓）、wasm32 可用 ✓。
//!
//! **语义**：
//! - 只收 L8（灰度）与 RGB24（彩色）；CMYK32/L16 等 ⇒ `None`（**显式失败，不静默** ✓）
//! - JPEG 无 alpha ⇒ alpha=255 ✓
//! - EXIF Orientation（APP1）自动应用旋转/翻转 ✓（`jpeg-decoder` 不管 EXIF ✓）
use std::io::Cursor;

use jpeg_decoder::{Decoder, PixelFormat};

/// JPEG magic bytes：SOI（FF D8）+ 后面还有 FF（段标记开头）。
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF
}

/// JPEG → RGBA8（与 `crate::png::decode_png` 返回口径一致 ✓）。
///
/// EXIF 方向在内部自动应用 ✓ ⇒ 调用方拿到的就是**摆正了**的像素 ✓。
pub fn decode_jpeg(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if !is_jpeg(bytes) {
        return None;
    }
    let mut decoder = Decoder::new(Cursor::new(bytes));
    let pixels = decoder.decode().ok()?;
    let info = decoder.info()?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if width == 0 || height == 0 {
        return None;
    }
    let pixel_count = (width as usize) * (height as usize);
    let rgba: Vec<u8> = match info.pixel_format {
        PixelFormat::L8 => {
            if pixels.len() != pixel_count {
                return None;
            }
            let mut out = Vec::with_capacity(pixel_count * 4);
            for &gray in &pixels {
                out.extend_from_slice(&[gray, gray, gray, 255]);
            }
            out
        }
        PixelFormat::RGB24 => {
            if pixels.len() != pixel_count * 3 {
                return None;
            }
            let mut out = Vec::with_capacity(pixel_count * 4);
            for chunk in pixels.chunks_exact(3) {
                out.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            out
        }
        // **CMYK32 / L16 / 未知格式 ⇒ 明确失败** ✗（静默转 RGB 会颜色全错 ✗）。
        _ => return None,
    };
    let orientation = exif_orientation(bytes);
    let (out_w, out_h, out) = apply_orientation(&rgba, width, height, orientation);
    Some((out_w, out_h, out))
}

/// APP1 `Exif\0\0` 段里 IFD0 的 Orientation 标签（0x0112），最小解析。
///
/// 缺失/损坏/格式不对 ⇒ 1（正常）。返回 1..=8。
pub fn exif_orientation(bytes: &[u8]) -> u8 {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return 1;
    }
    let mut pos = 2;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            return 1;
        }
        let marker = bytes[pos + 1];
        pos += 2;
        // 独立标记（无长度字段）与填充字节。
        if marker == 0xD8 {
            continue;
        }
        if marker == 0xD9 {
            return 1;
        }
        if marker == 0x00 {
            continue;
        }
        // SOS 之后是图像数据，不再有文件头段。
        if marker == 0xDA {
            return 1;
        }
        if pos + 2 > bytes.len() {
            return 1;
        }
        let seg_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        if seg_len < 2 || pos + seg_len > bytes.len() {
            return 1;
        }
        if marker == 0xE1 {
            match parse_exif_orientation(&bytes[pos + 2..pos + seg_len]) {
                Some(o) if o != 1 => return o,
                // APP1 不是 EXIF ⇒ 继续找下一个段（缩略图里也可能带 EXIF 头 ✓）。
                _ => {}
            }
        }
        pos += seg_len;
    }
    1
}

/// EXIF 载荷 → Orientation 值。不是 `Exif\0\0` 开头/格式不对 ⇒ None。
fn parse_exif_orientation(payload: &[u8]) -> Option<u8> {
    if payload.len() < 6 || &payload[..6] != b"Exif\0\0" {
        return None;
    }
    let tiff = &payload[6..];
    if tiff.len() < 8 {
        return None;
    }
    let little_endian = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read16 = |bytes: &[u8]| {
        if little_endian {
            u16::from_le_bytes([bytes[0], bytes[1]])
        } else {
            u16::from_be_bytes([bytes[0], bytes[1]])
        }
    };
    let read32 = |bytes: &[u8]| {
        if little_endian {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        } else {
            u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
        }
    };
    if read16(&tiff[2..4]) != 42 {
        return None;
    }
    let ifd0 = read32(&tiff[4..8]) as usize;
    if ifd0 + 2 > tiff.len() {
        return None;
    }
    let count = read16(&tiff[ifd0..ifd0 + 2]) as usize;
    for index in 0..count {
        let entry = ifd0 + 2 + index * 12;
        if entry + 12 > tiff.len() {
            return None;
        }
        if read16(&tiff[entry..entry + 2]) != 0x0112 {
            continue;
        }
        // Orientation 是 SHORT（类型 3）×1：值在 value 字段的前两个字节。
        let (ty, cnt) = (
            read16(&tiff[entry + 2..entry + 4]),
            read32(&tiff[entry + 4..entry + 8]),
        );
        if ty == 3 && cnt == 1 {
            let value = read16(&tiff[entry + 8..entry + 10]);
            if (1..=8).contains(&value) {
                return Some(value as u8);
            }
        }
        return None;
    }
    None
}

/// EXIF Orientation 变换（输出像素 (x,y) 取自源 (sx,sy)，按 EXIF 规范）。
///
/// 索引映射走内核共用实现 [`yanshi_core::orientation`]（RAW 展开链的 f32 路
/// 用同一份，设计 5.7「应抽共用实现而非复制第二份」）；本函数只做 u8 RGBA
/// 的搬运。返回（新宽，新高，新像素）。orientation 非法或长度不对 ⇒ 原样返回。
pub fn apply_orientation(
    rgba: &[u8],
    width: u32,
    height: u32,
    orientation: u8,
) -> (u32, u32, Vec<u8>) {
    let (w, h) = (width as usize, height as usize);
    if orientation == 1 || rgba.len() != w * h * 4 || !(1..=8).contains(&orientation) {
        return (width, height, rgba.to_vec());
    }
    let (out_w, out_h) = yanshi_core::orientation::output_size(w, h, u32::from(orientation));
    let mut out = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let (sx, sy) =
                yanshi_core::orientation::source_coord(x, y, w, h, u32::from(orientation));
            out[(y * out_w + x) * 4..(y * out_w + x) * 4 + 4]
                .copy_from_slice(&rgba[(sy * w + sx) * 4..(sy * w + sx) * 4 + 4]);
        }
    }
    (out_w as u32, out_h as u32, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成 APP1 载荷：小端（II），IFD0 里一个 Orientation 条目。
    fn app1_payload(little_endian: bool, orientation: u16) -> Vec<u8> {
        let mut payload = b"Exif\0\0".to_vec();
        let mut tiff = if little_endian {
            b"II".to_vec()
        } else {
            b"MM".to_vec()
        };
        let push16 = |tiff: &mut Vec<u8>, v: u16| {
            let bytes = if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            };
            tiff.extend_from_slice(&bytes);
        };
        let push32 = |tiff: &mut Vec<u8>, v: u32| {
            let bytes = if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            };
            tiff.extend_from_slice(&bytes);
        };
        push16(&mut tiff, 42);
        push32(&mut tiff, 8); // IFD0 偏移
        push16(&mut tiff, 1); // 1 个条目
        push16(&mut tiff, 0x0112); // tag
        push16(&mut tiff, 3); // SHORT
        push32(&mut tiff, 1); // count
        push16(&mut tiff, orientation); // value 前两字节
        push16(&mut tiff, 0);
        payload.extend_from_slice(&tiff);
        payload
    }

    /// 合成最小 JPEG 文件头（含 APP1）。
    fn fake_jpeg(payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1];
        let seg_len = (payload.len() + 2) as u16;
        bytes.extend_from_slice(&seg_len.to_be_bytes());
        bytes.extend_from_slice(payload);
        bytes.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x08, 0xFF, 0xD9]);
        bytes
    }

    #[test]
    fn exif_orientation_parses_little_and_big_endian() {
        // 小端 Orientation=6。
        let jpeg = fake_jpeg(&app1_payload(true, 6));
        assert_eq!(exif_orientation(&jpeg), 6);
        // 大端 Orientation=3。
        let jpeg = fake_jpeg(&app1_payload(false, 3));
        assert_eq!(exif_orientation(&jpeg), 3);
    }

    #[test]
    fn exif_orientation_defaults_to_one_on_garbage() {
        // 不是 JPEG。
        assert_eq!(exif_orientation(b"not a jpeg"), 1);
        // APP1 不是 Exif 开头。
        let jpeg = fake_jpeg(b"nonsense payload");
        assert_eq!(exif_orientation(&jpeg), 1);
        // 没有 APP1 段。
        assert_eq!(exif_orientation(&[0xFF, 0xD8, 0xFF, 0xD9]), 1);
    }

    /// 3×2 标记缓冲：像素值 = 索引（r=g=b=idx, a=255）。
    fn labeled(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        for index in 0..width * height {
            let v = index as u8;
            out.extend_from_slice(&[v, v, v, 255]);
        }
        out
    }

    fn pixel_index(out: &[u8], out_w: u32, x: u32, y: u32) -> u8 {
        out[((y * out_w + x) * 4) as usize]
    }

    #[test]
    fn orientation_transforms_pin_all_eight_cases() {
        let src = labeled(3, 2);
        // 1：原样。
        let (w, h, out) = apply_orientation(&src, 3, 2, 1);
        assert_eq!((w, h), (3, 2));
        assert_eq!(out, src);
        // 2：水平翻转。out(0,0) ← in(2,0) = 2。
        let (w, h, out) = apply_orientation(&src, 3, 2, 2);
        assert_eq!((w, h), (3, 2));
        assert_eq!(pixel_index(&out, w, 0, 0), 2);
        assert_eq!(pixel_index(&out, w, 2, 1), 3);
        // 3：180°。out(0,0) ← in(2,1) = 5。
        let (w, _h, out) = apply_orientation(&src, 3, 2, 3);
        assert_eq!(pixel_index(&out, w, 0, 0), 5);
        assert_eq!(pixel_index(&out, w, 2, 1), 0);
        // 4：垂直翻转。out(0,0) ← in(0,1) = 3。
        let (w, _h, out) = apply_orientation(&src, 3, 2, 4);
        assert_eq!(pixel_index(&out, w, 0, 0), 3);
        // 5：转置（3×2 → 2×3）。out(0,0) ← in(0,0) = 0；out(1,2) ← in(2,1) = 5。
        let (w, h, out) = apply_orientation(&src, 3, 2, 5);
        assert_eq!((w, h), (2, 3));
        assert_eq!(pixel_index(&out, w, 0, 0), 0);
        assert_eq!(pixel_index(&out, w, 1, 2), 5);
        // 6：顺时针 90°。out(0,0) ← in(0,1) = 3。
        let (w, h, out) = apply_orientation(&src, 3, 2, 6);
        assert_eq!((w, h), (2, 3));
        assert_eq!(pixel_index(&out, w, 0, 0), 3);
        assert_eq!(pixel_index(&out, w, 1, 2), 2);
        // 7：逆转置。out(0,0) ← in(2,1) = 5。
        let (w, h, out) = apply_orientation(&src, 3, 2, 7);
        assert_eq!((w, h), (2, 3));
        assert_eq!(pixel_index(&out, w, 0, 0), 5);
        assert_eq!(pixel_index(&out, w, 1, 2), 0);
        // 8：逆时针 90°。out(0,0) ← in(2,0) = 2。
        let (w, h, out) = apply_orientation(&src, 3, 2, 8);
        assert_eq!((w, h), (2, 3));
        assert_eq!(pixel_index(&out, w, 0, 0), 2);
        assert_eq!(pixel_index(&out, w, 1, 2), 3);
    }

    #[test]
    fn is_jpeg_recognizes_magic_bytes() {
        assert!(is_jpeg(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!is_jpeg(&[0x89, 0x50, 0x4E, 0x47])); // PNG
        assert!(!is_jpeg(&[0xFF, 0xD8])); // 太短
        assert!(!is_jpeg(&[]));
    }

    /// 极简 base64 解码（只给测试夹具用 ✓）。
    fn b64_decode(text: &str) -> Vec<u8> {
        let value = |byte: u8| -> u32 {
            match byte {
                b'A'..=b'Z' => u32::from(byte - b'A'),
                b'a'..=b'z' => u32::from(byte - b'a') + 26,
                b'0'..=b'9' => u32::from(byte - b'0') + 52,
                b'+' => 62,
                b'/' => 63,
                _ => 0,
            }
        };
        let mut out = Vec::new();
        let mut buffer = 0u32;
        let mut bits = 0u32;
        for byte in text.bytes() {
            if byte == b'=' {
                break;
            }
            buffer = (buffer << 6) | value(byte);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
            }
        }
        out
    }

    // 6×4 彩色夹具：每列一种纯色 [红，绿，蓝，黄，品红，青]（Pillow quality=98、subsampling=0 生成——4:2:0 色度抽样会把单像素列混色，夹具必须 4:4:4 ✓）。
    const PLAIN_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAEBAQEBAQEBAQEBAQEBAQIBAQEBAQIBAQECAgICAgICAgIDAwQDAwMDAwICAwQDAwQEBAQEAgMFBQQEBQQEBAT/2wBDAQEBAQEBAQIBAQIEAwIDBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAT/wAARCAAEAAYDAREAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD2P/gnz/yLv/c469/6pv8AZQr9V/Z9f8kNh/8AslMi/wDXl+Op/n9+23/5Qo8a/wDtIDOf/UvxLP/Z";

    // 同图 + EXIF Orientation=6。
    const EXIF6_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/4QAiRXhpZgAATU0AKgAAAAgAAQESAAMAAAABAAYAAAAAAAD/2wBDAAEBAQEBAQEBAQEBAQEBAQIBAQEBAQIBAQECAgICAgICAgIDAwQDAwMDAwICAwQDAwQEBAQEAgMFBQQEBQQEBAT/2wBDAQEBAQEBAQIBAQIEAwIDBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAT/wAARCAAEAAYDAREAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwD2P/gnz/yLv/c469/6pv8AZQr9V/Z9f8kNh/8AslMi/wDXl+Op/n9+23/5Qo8a/wDtIDOf/UvxLP/Z";

    // 8×8 灰度夹具：(i*37)%256 渐变（Pillow 生成；3×2 的极小灰度图 jpeg-decoder 会报
    // "not all components have data"，故用 8×8 ✓）。
    const GRAY_JPEG_B64: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAEBAQEBAQEBAQEBAQEBAQIBAQEBAQIBAQECAgICAgICAgIDAwQDAwMDAwICAwQDAwQEBAQEAgMFBQQEBQQEBAT/wAALCAAIAAgBAREA/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/9oACAEBAAA/AOL8IfBrwr8Q/CuufsW/sW6t4f8ABf7JHgvw/dx/tk/tkx6rbeHPCvxU0rTraSXX9B0HxA7pb2vgq1t4bk6vrxlSPW44poYZV0RZ59f/AP/Z";

    fn approx(pixel: &[u8], expected: [u8; 3], tol: i32, label: &str) {
        for channel in 0..3 {
            let delta = (i32::from(pixel[channel]) - i32::from(expected[channel])).abs();
            assert!(
                delta <= tol,
                "{label} 通道 {channel} 应为 {}±{tol}，实得 {pixel:?}",
                expected[channel]
            );
        }
        assert_eq!(pixel[3], 255, "{label} alpha 应为 255");
    }

    #[test]
    fn decode_colour_jpeg_fixture() {
        let bytes = b64_decode(PLAIN_JPEG_B64);
        let (w, h, rgba) = decode_jpeg(&bytes).expect("彩色夹具应解码成功");
        assert_eq!((w, h), (6, 4));
        // 左上角红、右下角青（有损，宽容差 ✓）。
        approx(&rgba[0..4], [255, 0, 0], 32, "左上角");
        let last = ((3 * 6 + 5) * 4) as usize;
        approx(&rgba[last..last + 4], [0, 255, 255], 32, "右下角");
    }

    #[test]
    fn decode_gray_jpeg_fixture_expands_to_rgba() {
        let bytes = b64_decode(GRAY_JPEG_B64);
        let (w, h, rgba) = decode_jpeg(&bytes).expect("灰度夹具应解码成功");
        assert_eq!((w, h), (8, 8));
        for pixel in rgba.chunks_exact(4) {
            assert_eq!(pixel[0], pixel[1], "灰度应 r=g：{pixel:?}");
            assert_eq!(pixel[1], pixel[2], "灰度应 g=b：{pixel:?}");
            assert_eq!(pixel[3], 255);
        }
        // 夹具是 (i*37)%256 渐变：第一个是 0（近黑）、第七个是 222（亮）。
        assert!(rgba[0] < 16, "第一像素应近黑：{}", rgba[0]);
        let seventh = rgba[6 * 4];
        assert!(seventh > 190, "第七像素应亮：{seventh}");
    }

    #[test]
    fn decode_jpeg_applies_exif_orientation() {
        let bytes = b64_decode(EXIF6_JPEG_B64);
        assert_eq!(exif_orientation(&bytes), 6);
        let (w, h, rgba) = decode_jpeg(&bytes).expect("EXIF 夹具应解码成功");
        // 6×4 顺时针旋转 90° ⇒ 4×6；旋转后 (0,0) 是原 (0,3)=红，(3,5) 是原 (5,0)=青。
        assert_eq!((w, h), (4, 6), "方向 6 应交换宽高");
        approx(&rgba[0..4], [255, 0, 0], 32, "旋转后左上");
        let last = ((5 * 4 + 3) * 4) as usize;
        approx(&rgba[last..last + 4], [0, 255, 255], 32, "旋转后右下");
    }

    #[test]
    fn decode_rejects_non_jpeg_and_truncated() {
        assert!(decode_jpeg(b"\x89PNG\r\n").is_none());
        let bytes = b64_decode(PLAIN_JPEG_B64);
        assert!(
            decode_jpeg(&bytes[..bytes.len() / 2]).is_none(),
            "截断应失败"
        );
    }
}
