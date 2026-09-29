//! PNG 输出（设计文档 7.5 / 18 章）。
//!
//! 零依赖编码器：8 位 RGBA、无隔行、每条扫描线使用 filter 0，
//! deflate 使用 **stored（未压缩）块**，因此输出完全确定、可逐字节复现。
//!
//! 设计文档最终要求 WebP/AVIF 与 zstd；这些属于**传输层**编码，
//! 内核先提供确定性 PNG 作为可视化与验收载体（`图像用 WebP/AVIF` 见 14.4）。

use std::io::Write;
use std::path::Path;

/// PNG 魔数。
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// deflate stored 块的最大负载。
const STORED_BLOCK_MAX: usize = 65_535;

/// 把 8 位 RGBA 像素编码为 PNG。
///
/// `rgba8` 长度必须是 `width * height * 4`，否则返回 `None`。
pub fn encode_png(width: u32, height: u32, rgba8: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let expected = (width as usize) * (height as usize) * 4;
    if rgba8.len() != expected {
        return None;
    }

    // 原始数据：每条扫描线前加一个 filter 字节（0 = None）。
    let mut raw = Vec::with_capacity(expected + height as usize);
    for row in 0..height as usize {
        raw.push(0);
        let start = row * width as usize * 4;
        raw.extend_from_slice(&rgba8[start..start + width as usize * 4]);
    }

    let mut out = Vec::with_capacity(raw.len() + 128);
    out.extend_from_slice(&SIGNATURE);
    write_chunk(&mut out, b"IHDR", &ihdr(width, height));
    write_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    write_chunk(&mut out, b"IEND", &[]);
    Some(out)
}

/// 写 PNG 文件。
pub fn write_png_file(
    path: impl AsRef<Path>,
    width: u32,
    height: u32,
    rgba8: &[u8],
) -> std::io::Result<()> {
    let encoded = encode_png(width, height, rgba8).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "像素数据长度与尺寸不匹配")
    })?;
    let mut file = std::fs::File::create(path)?;
    file.write_all(&encoded)?;
    file.flush()
}

fn ihdr(width: u32, height: u32) -> [u8; 13] {
    let mut header = [0u8; 13];
    header[0..4].copy_from_slice(&width.to_be_bytes());
    header[4..8].copy_from_slice(&height.to_be_bytes());
    header[8] = 8; // bit depth
    header[9] = 6; // color type: RGBA
    header[10] = 0; // compression
    header[11] = 0; // filter
    header[12] = 0; // interlace
    header
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(payload);
    let mut crc_input = Vec::with_capacity(4 + payload.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(payload);
    out.extend_from_slice(&yanshi_core::crc32(&crc_input).to_be_bytes());
}

/// zlib 容器 + stored deflate 块。
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / STORED_BLOCK_MAX * 5 + 16);
    out.push(0x78); // CMF: deflate, 32K window
    out.push(0x01); // FLG: 无字典、最快压缩级别（校验位使 0x7801 合法）
    if raw.is_empty() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xFF, 0xFF]);
    } else {
        let mut offset = 0usize;
        while offset < raw.len() {
            let remaining = raw.len() - offset;
            let length = remaining.min(STORED_BLOCK_MAX);
            let is_last = offset + length >= raw.len();
            out.push(u8::from(is_last));
            let length16 = length as u16;
            out.extend_from_slice(&length16.to_le_bytes());
            out.extend_from_slice(&(!length16).to_le_bytes());
            out.extend_from_slice(&raw[offset..offset + length]);
            offset += length;
        }
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// zlib 使用的 Adler-32 校验。
pub fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in bytes {
        a = (a + u32::from(*byte)) % MOD;
        b = (b + a) % MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_pixels() -> Vec<u8> {
        vec![
            255, 0, 0, 255, // 红
            0, 255, 0, 255, // 绿
            0, 0, 255, 255, // 蓝
            0, 0, 0, 0, // 透明
        ]
    }

    #[test]
    fn adler32_matches_known_vector() {
        // 标准向量：adler32("Wikipedia") = 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn png_structure_is_valid() {
        let pixels = tiny_pixels();
        let png = encode_png(2, 2, &pixels).unwrap();
        assert_eq!(&png[0..8], &SIGNATURE);
        // 第一个块必须是 IHDR。
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes([png[8], png[9], png[10], png[11]]), 13);
        assert_eq!(&png[16..20], &2u32.to_be_bytes());
        assert_eq!(&png[20..24], &2u32.to_be_bytes());
        assert_eq!(png[24], 8);
        assert_eq!(png[25], 6);
        // 末尾是 IEND。
        assert_eq!(&png[png.len() - 8..png.len() - 4], b"IEND");
        // IHDR CRC 自校验。
        let ihdr_crc = u32::from_be_bytes([png[29], png[30], png[31], png[32]]);
        assert_eq!(ihdr_crc, yanshi_core::crc32(&png[12..29]));
    }

    #[test]
    fn idat_payload_carries_filtered_scanlines() {
        let pixels = tiny_pixels();
        let png = encode_png(2, 2, &pixels).unwrap();
        let idat_start = png.windows(4).position(|w| w == b"IDAT").unwrap();
        let length = u32::from_be_bytes([
            png[idat_start - 4],
            png[idat_start - 3],
            png[idat_start - 2],
            png[idat_start - 1],
        ]) as usize;
        let payload = &png[idat_start + 4..idat_start + 4 + length];
        // zlib 头 + stored 块头 + 原始数据 + adler32。
        assert_eq!(&payload[0..2], &[0x78, 0x01]);
        assert_eq!(payload[2], 1, "单块且为最后一块");
        let block_len = u16::from_le_bytes([payload[3], payload[4]]) as usize;
        assert_eq!(block_len, 2 * (1 + 2 * 4), "两条扫描线，各含 filter 字节");
        let raw = &payload[7..7 + block_len];
        assert_eq!(raw[0], 0, "scanline 0 filter = 0");
        assert_eq!(&raw[1..5], &[255u8, 0, 0, 255]);
        assert_eq!(&raw[5..9], &[0u8, 255, 0, 255], "行 0 第二像素为绿色");
        assert_eq!(raw[9], 0, "scanline 1 filter = 0");
        assert_eq!(&raw[10..14], &[0u8, 0, 255, 255], "行 1 第一像素为蓝色");
        // 最后 4 字节是 adler32。
        let adler = u32::from_be_bytes([
            payload[payload.len() - 4],
            payload[payload.len() - 3],
            payload[payload.len() - 2],
            payload[payload.len() - 1],
        ]);
        assert_eq!(adler, adler32(raw));
    }

    #[test]
    fn large_images_use_multiple_stored_blocks() {
        let width = 64u32;
        let height = 300u32;
        let pixels = vec![128u8; (width * height * 4) as usize];
        let png = encode_png(width, height, &pixels).unwrap();
        let idat_start = png.windows(4).position(|w| w == b"IDAT").unwrap();
        let length = u32::from_be_bytes([
            png[idat_start - 4],
            png[idat_start - 3],
            png[idat_start - 2],
            png[idat_start - 1],
        ]) as usize;
        let payload = &png[idat_start + 4..idat_start + 4 + length];
        // 找到最后一块标记。
        let mut offset = 2usize;
        let mut blocks = 0;
        loop {
            let last = payload[offset];
            let block_len = u16::from_le_bytes([payload[offset + 1], payload[offset + 2]]) as usize;
            offset += 5 + block_len;
            blocks += 1;
            if last == 1 {
                break;
            }
        }
        assert!(blocks >= 2, "300 行 × 257 字节应分多个 stored 块");
    }

    #[test]
    fn invalid_inputs_are_rejected() {
        assert!(encode_png(0, 4, &[]).is_none());
        assert!(encode_png(2, 2, &[0u8; 8]).is_none());
    }

    #[test]
    fn writing_a_file_produces_the_same_bytes() {
        let pixels = tiny_pixels();
        let expected = encode_png(2, 2, &pixels).unwrap();
        let mut path = std::env::temp_dir();
        path.push(format!("yanshi-png-test-{}.png", std::process::id()));
        write_png_file(&path, 2, 2, &pixels).unwrap();
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written, expected);
        let _ = std::fs::remove_file(&path);
    }
}
