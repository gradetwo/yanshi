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
/// deflate stored 块的最大负载 ✓（**只在测试里作为对照基线** ✓，见 `deflate_tests` ✓）。
#[cfg(test)]
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
    // 固定 Huffman + LZ77 ✓（`zlib_stored` 保留为对照 ✓，见测试 ✓）。
    write_chunk(&mut out, b"IDAT", &zlib_fixed(&raw));
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

// ---------------------------------------------------------------------------
// deflate（固定 Huffman + LZ77）✓
// ---------------------------------------------------------------------------
//
// **为什么手写** ✓：设计只允许 `wasm-bindgen` 一个依赖 ✓ ⇒ 不能在仓库里引 zlib/miniz ✗。
// 此前用 deflate 的 **stored（未压缩）块** ✓，输出因此完全确定、可逐字节复现 ✓，
// 但也**完全不压缩** ✗ —— 子 agent 实测 960×640 导出 **2,458,493 字节** ✓
//（≈ 原始 RGBA 2,457,600 ✓，约 20× 膨胀 ✓）。
//
// 这里实现 **固定 Huffman 码表 + LZ77** ✓（RFC 1951 §3.2.6 ✓）：
// 码表是规范里写死的 ✓ ⇒ 不需要动态 Huffman 的码长传输 ✓，实现小且**完全确定** ✓
//（无时间、无随机、无浮点 ✓ ⇒ 同一份像素永远得到同一串字节 ✓）。
// 对绘画作品（大片平色 + 重复纹理 ✓）压缩效果已经很可观 ✓。

/// 固定 Huffman 字面/长度码（RFC 1951 §3.2.6）✓，返回（码字、位数）✓。
fn fixed_literal_code(symbol: u16) -> (u16, u8) {
    match symbol {
        0..=143 => (0x30 + symbol, 8),
        144..=255 => (0x190 + (symbol - 144), 9),
        256..=279 => (symbol - 256, 7),
        _ => (0xC0 + (symbol - 280), 8),
    }
}

/// 长度码 257..=285 的基准值与额外位数 ✓。
const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// 距离码 0..=29 的基准值与额外位数 ✓。
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

const LZ_WINDOW: usize = 32_768;
const LZ_MIN_MATCH: usize = 3;
const LZ_MAX_MATCH: usize = 258;
/// 哈希链的最大回溯步数 ✓（速度与压缩率的折中常数 ✓，取值不影响正确性 ✓）。
const LZ_MAX_CHAIN: usize = 96;
const HASH_BITS: u32 = 15;

/// 位写入器 ✓：Huffman 码**高位在前** ✓，额外位**低位在前** ✓（RFC 1951 §3.1.1 ✓）。
struct BitWriter {
    out: Vec<u8>,
    bit_buffer: u32,
    bit_count: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            out: Vec::new(),
            bit_buffer: 0,
            bit_count: 0,
        }
    }

    /// 低位在前写入 `count` 位 ✓（用于额外位 ✓）。
    fn write_lsb(&mut self, value: u32, count: u8) {
        self.bit_buffer |= (value & ((1u32 << count) - 1)) << self.bit_count;
        self.bit_count += u32::from(count);
        while self.bit_count >= 8 {
            self.out.push((self.bit_buffer & 0xFF) as u8);
            self.bit_buffer >>= 8;
            self.bit_count -= 8;
        }
    }

    /// 高位在前写入 Huffman 码 ✓。
    fn write_code(&mut self, code: u16, count: u8) {
        for index in (0..count).rev() {
            let bit = (code >> index) & 1;
            self.write_lsb(u32::from(bit), 1);
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            self.out.push((self.bit_buffer & 0xFF) as u8);
        }
        self.out
    }
}

fn hash3(data: &[u8], at: usize) -> usize {
    let a = u32::from(data[at]);
    let b = u32::from(data[at + 1]);
    let c = u32::from(data[at + 2]);
    (((a << 10) ^ (b << 5) ^ c) & ((1 << HASH_BITS) - 1)) as usize
}

/// 用固定 Huffman + LZ77 生成 deflate 位流 ✓（单块、BFINAL=1 ✓）。
fn deflate_fixed(raw: &[u8]) -> Vec<u8> {
    let mut writer = BitWriter::new();
    // BFINAL=1 ✓、BTYPE=01（固定 Huffman ✓）—— 三位按低位在前 ✓。
    writer.write_lsb(1, 1);
    writer.write_lsb(1, 2);

    if raw.is_empty() {
        let (code, bits) = fixed_literal_code(256);
        writer.write_code(code, bits);
        return writer.finish();
    }

    let mut head = vec![u32::MAX; 1 << HASH_BITS];
    let mut prev = vec![u32::MAX; raw.len()];
    let mut position = 0usize;

    while position < raw.len() {
        let mut best_length = 0usize;
        let mut best_distance = 0usize;
        if position + LZ_MIN_MATCH <= raw.len() {
            let hash = hash3(raw, position);
            let mut candidate = head[hash];
            let limit = position.saturating_sub(LZ_WINDOW);
            let max_length = LZ_MAX_MATCH.min(raw.len() - position);
            let mut steps = 0;
            while candidate != u32::MAX && steps < LZ_MAX_CHAIN {
                let at = candidate as usize;
                if at < limit {
                    break;
                }
                // 先看能否比当前最优更长 ✓（省掉大量逐字节比较 ✓）。
                if best_length == 0
                    || (at + best_length < raw.len()
                        && raw[at + best_length] == raw[position + best_length])
                {
                    let mut length = 0usize;
                    while length < max_length && raw[at + length] == raw[position + length] {
                        length += 1;
                    }
                    if length > best_length {
                        best_length = length;
                        best_distance = position - at;
                        if length >= max_length {
                            break;
                        }
                    }
                }
                candidate = prev[at];
                steps += 1;
            }
        }

        if best_length >= LZ_MIN_MATCH {
            // 长度码 ✓。
            let mut slot = 0usize;
            while slot + 1 < LENGTH_BASE.len() && LENGTH_BASE[slot + 1] <= best_length as u16 {
                slot += 1;
            }
            let (code, bits) = fixed_literal_code(257 + slot as u16);
            writer.write_code(code, bits);
            let extra = u32::from(best_length as u16 - LENGTH_BASE[slot]);
            writer.write_lsb(extra, LENGTH_EXTRA[slot]);
            // 距离码 ✓（5 位固定码 ✓ + 额外位 ✓）。
            let mut dslot = 0usize;
            while dslot + 1 < DIST_BASE.len() && DIST_BASE[dslot + 1] <= best_distance as u16 {
                dslot += 1;
            }
            writer.write_code(dslot as u16, 5);
            let dextra = u32::from(best_distance as u16 - DIST_BASE[dslot]);
            writer.write_lsb(dextra, DIST_EXTRA[dslot]);

            // 把匹配区间内的每个位置都登记进哈希链 ✓（否则后续匹配会漏掉 ✓）。
            // 用迭代器而不是下标循环 ✓ —— clippy 的 `needless_range_loop` 会指出
            // "循环变量只用来索引" ✓，而这里确实可以写成更直接的遍历 ✓。
            let register_until =
                (position + best_length).min(raw.len().saturating_sub(LZ_MIN_MATCH - 1));
            for (offset, slot) in prev
                .iter_mut()
                .enumerate()
                .take(register_until)
                .skip(position)
            {
                let hash = hash3(raw, offset);
                *slot = head[hash];
                head[hash] = offset as u32;
            }
            position += best_length;
        } else {
            let (code, bits) = fixed_literal_code(u16::from(raw[position]));
            writer.write_code(code, bits);
            if position + LZ_MIN_MATCH <= raw.len() {
                let hash = hash3(raw, position);
                prev[position] = head[hash];
                head[hash] = position as u32;
            }
            position += 1;
        }
    }

    let (code, bits) = fixed_literal_code(256);
    writer.write_code(code, bits);
    writer.finish()
}

/// zlib 包装 + 固定 Huffman deflate ✓（与 [`zlib_stored`] 同格式、不同压缩方式 ✓）。
fn zlib_fixed(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() / 4 + 64);
    out.push(0x78); // CMF: deflate, 32K 窗口 ✓
    out.push(0x01); // FLG: 无字典 ✓（0x7801 能被 31 整除 ✓，合法 ✓）
    out.extend_from_slice(&deflate_fixed(raw));
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

/// zlib 容器 + **stored（未压缩）deflate 块** ✓。
///
/// **仅供测试** ✓：它是"未压缩基线" ✓，用来对比固定 Huffman 的收益 ✓
///（子 agent 报的 G4：960×640 导出 2,458,493 字节 ≈ 原始 RGBA ✓，约 20× 膨胀 ✓）。
#[cfg(test)]
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
        // **有意改写** ✓：编码器已从 deflate 的 **stored（未压缩）块** 改为
        // **固定 Huffman + LZ77** ✓（子 agent 报的 G4：导出 2,458,493 字节 ≈ 原始 RGBA ✗）。
        // 这条测试的**原意**是"IDAT 解压后正好是带 filter 的扫描线" ✓ —— 保留原意 ✓，
        // 但不再断言某种**块布局** ✗（那是实现细节 ✓，且正是本轮要换掉的东西 ✓）。
        assert_eq!(&payload[0..2], &[0x78, 0x01], "zlib 头");
        let inflated = super::deflate_tests::inflate_fixed_zlib(payload);
        // **由测试数据构造期望** ✓ —— 不再手抄 ✗（我第一版手抄错了末像素 ✓，
        // 而"期望"一旦手写就很容易与被测数据脱节 ✓）。这里仍能验证结构 ✓：
        // 每条扫描线前面必须正好有一个 filter 字节 0 ✓，其后是**逐字节相同**的像素 ✓。
        let pixels = tiny_pixels();
        let mut expected: Vec<u8> = Vec::with_capacity(2 * (1 + 8));
        for row in 0..2 {
            expected.push(0);
            expected.extend_from_slice(&pixels[row * 8..(row + 1) * 8]);
        }
        assert_eq!(inflated, expected, "解压后应是两条带 filter 的扫描线");
    }

    /// **有意改写** ✓：本条原本断言"大图会拆成多个 stored 块" ✗ ——
    /// 那是 `stored` 编码的实现细节 ✓，而本轮换成了固定 Huffman + LZ77 ✓。
    /// **原意保留** ✓：大图也必须能被**正确解回**（尺寸、filter、像素一个不差 ✓）。
    #[test]
    fn large_images_round_trip_through_the_compressed_idat() {
        let width = 64u32;
        let height = 300u32;
        let pixels: Vec<u8> = (0..width * height * 4)
            .map(|index| (index % 253) as u8)
            .collect();
        let png = encode_png(width, height, &pixels).unwrap();
        let idat_start = png.windows(4).position(|w| w == b"IDAT").unwrap();
        let length = u32::from_be_bytes([
            png[idat_start - 4],
            png[idat_start - 3],
            png[idat_start - 2],
            png[idat_start - 1],
        ]) as usize;
        let payload = &png[idat_start + 4..idat_start + 4 + length];
        let inflated = super::deflate_tests::inflate_fixed_zlib(payload);
        let stride = width as usize * 4;
        assert_eq!(
            inflated.len(),
            (stride + 1) * height as usize,
            "解压后的行数/行长"
        );
        for row in 0..height as usize {
            assert_eq!(
                inflated[row * (stride + 1)],
                0,
                "第 {row} 行的 filter 应为 0"
            );
            assert_eq!(
                &inflated[row * (stride + 1) + 1..row * (stride + 1) + 1 + stride],
                &pixels[row * stride..(row + 1) * stride],
                "第 {row} 行的像素"
            );
        }
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

#[cfg(test)]
mod deflate_tests {
    use super::*;

    /// 固定 Huffman 的**解压**（仅测试用 ✓）—— 它是本文件压缩器的**独立验证者** ✓。
    ///
    /// 刻意写成"照着 RFC 再实现一遍" ✓（而不是复用压缩侧的码表构造 ✓）：
    /// 若两侧共用同一段有错的逻辑 ✓，往返测试会**双双通过**而实际输出仍不合法 ✗。
    /// 这里从 RFC 的码表定义重新解码 ✓，并逐位校验 zlib 包装与 adler32 ✓。
    struct BitReader<'a> {
        data: &'a [u8],
        position: usize,
        bit: u32,
        buffer: u32,
    }

    impl<'a> BitReader<'a> {
        fn new(data: &'a [u8]) -> Self {
            Self {
                data,
                position: 0,
                bit: 0,
                buffer: 0,
            }
        }

        fn read_lsb(&mut self, count: u8) -> u32 {
            while self.bit < u32::from(count) {
                let byte = self.data[self.position];
                self.position += 1;
                self.buffer |= u32::from(byte) << self.bit;
                self.bit += 8;
            }
            let mask = if count == 0 { 0 } else { (1u32 << count) - 1 };
            let value = self.buffer & mask;
            self.buffer >>= count;
            self.bit -= u32::from(count);
            value
        }

        /// 高位在前读 Huffman 码 ✓。
        fn read_code(&mut self, count: u8) -> u16 {
            let mut value = 0u16;
            for _ in 0..count {
                value = (value << 1) | self.read_lsb(1) as u16;
            }
            value
        }
    }

    fn fixed_decode_symbol(reader: &mut BitReader<'_>) -> u16 {
        // 逐位读码字 ✓，与 RFC 的码表逐个比对 ✓（288 个符号很少 ✓，清晰优先 ✓）。
        let mut code = 0u16;
        for width in 1..=9u8 {
            code = (code << 1) | reader.read_lsb(1) as u16;
            for symbol in 0..288u16 {
                let (expected, bits) = fixed_literal_code(symbol);
                if bits == width && expected == code {
                    return symbol;
                }
            }
        }
        panic!("不是合法的固定 Huffman 码字");
    }

    /// 解压 `zlib_fixed` 的输出 ✓（只支持本编码器产出的那种流 ✓：单块固定 Huffman ✓）。
    pub(super) fn inflate_fixed_zlib(stream: &[u8]) -> Vec<u8> {
        assert!(stream.len() > 6, "zlib 流太短");
        assert_eq!(stream[0], 0x78, "CMF 应为 deflate/32K");
        assert_eq!(
            (u16::from(stream[0]) << 8 | u16::from(stream[1])) % 31,
            0,
            "zlib 头校验不通过"
        );
        let mut reader = BitReader::new(&stream[2..]);
        assert_eq!(reader.read_lsb(1), 1, "应为最后一块");
        assert_eq!(reader.read_lsb(2), 1, "应为固定 Huffman 块");
        let mut out: Vec<u8> = Vec::new();
        loop {
            let symbol = fixed_decode_symbol(&mut reader);
            match symbol {
                0..=255 => out.push(symbol as u8),
                256 => break,
                _ => {
                    let slot = (symbol - 257) as usize;
                    let length =
                        LENGTH_BASE[slot] as usize + reader.read_lsb(LENGTH_EXTRA[slot]) as usize;
                    let distance_slot = reader.read_code(5) as usize;
                    let distance = DIST_BASE[distance_slot] as usize
                        + reader.read_lsb(DIST_EXTRA[distance_slot]) as usize;
                    assert!(
                        distance > 0 && distance <= out.len(),
                        "距离越界：{distance}"
                    );
                    let start = out.len() - distance;
                    for offset in 0..length {
                        let byte = out[start + offset];
                        out.push(byte);
                    }
                }
            }
        }
        // adler32 必须对得上 ✓（否则说明流虽然"看起来解开了"，内容其实是坏的 ✓）。
        let tail = &stream[reader.position + 2..];
        let expected = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
        assert_eq!(expected, adler32(&out), "adler32 不匹配");
        out
    }

    fn sample_image() -> Vec<u8> {
        // 近似绘画作品 ✓：大块平色 + 渐变 + 少量噪声 ✓（子 agent 的 960×640 作品是同一形态 ✓）。
        let (width, height) = (160u32, 120u32);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let sky = y < 60;
                let (r, g, b) = if sky {
                    ((140 + (x % 40)) as u8, (180 + (y % 30)) as u8, 230u8)
                } else {
                    let green = 90 + ((x * y) % 25) as u8;
                    (60, green, 70)
                };
                let alpha = if (x + y) % 97 == 0 { 200 } else { 255 };
                rgba.extend_from_slice(&[r, g, b, alpha]);
            }
        }
        rgba
    }

    /// **往返必须逐字节一致** ✓（压缩正确性的第一道关 ✓）。
    #[test]
    fn the_fixed_huffman_stream_round_trips() {
        for raw in [
            Vec::new(),
            b"a".to_vec(),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            b"abcdefghijklmnopqrstuvwxyz0123456789".to_vec(),
            (0..=255u8).collect::<Vec<u8>>(),
            sample_image(),
        ] {
            let compressed = zlib_fixed(&raw);
            let restored = inflate_fixed_zlib(&compressed);
            assert_eq!(restored, raw, "往返不一致（原始 {} 字节）", raw.len());
        }
    }

    /// **必须真的压缩** ✓ —— 子 agent 报的 G4：导出 2,458,493 字节 ≈ 原始 RGBA ✗（约 20× 膨胀 ✓）。
    /// 这里对同一份像素比较"固定 Huffman"与"stored 基线" ✓（`zlib_stored` 保留作对照 ✓）。
    #[test]
    fn fixed_huffman_beats_the_stored_baseline() {
        let raw = sample_image();
        let compressed = zlib_fixed(&raw);
        let baseline = zlib_stored(&raw);
        assert!(
            compressed.len() * 2 < raw.len(),
            "压缩后应显著小于原始：{} vs {}",
            compressed.len(),
            raw.len()
        );
        assert!(
            compressed.len() < baseline.len(),
            "应优于未压缩基线：{} vs {}",
            compressed.len(),
            baseline.len()
        );
    }

    /// **输出的 PNG 必须仍然合法** ✓：结构完整 ✓、IDAT 解压后正是"每行 filter 0 + 该行 RGBA" ✓。
    #[test]
    fn the_encoded_png_still_carries_the_expected_scanlines() {
        let (width, height) = (40u32, 30u32);
        let raw: Vec<u8> = (0..width * height * 4)
            .map(|index| (index % 251) as u8)
            .collect();
        let png = encode_png(width, height, &raw).expect("应能编码");
        assert_eq!(&png[..8], &SIGNATURE, "PNG 魔数");
        // 找 IDAT ✓（IHDR 固定 25 字节 ✓ ⇒ IDAT 长度字段紧随其后 ✓）。
        let idat_len = u32::from_be_bytes([png[33], png[34], png[35], png[36]]) as usize;
        assert_eq!(&png[37..41], b"IDAT", "第二个块应为 IDAT");
        let payload = &png[41..41 + idat_len];
        let inflated = inflate_fixed_zlib(payload);
        let stride = width as usize * 4;
        assert_eq!(
            inflated.len(),
            (stride + 1) * height as usize,
            "解压后的扫描线长度"
        );
        for row in 0..height as usize {
            assert_eq!(
                inflated[row * (stride + 1)],
                0,
                "第 {row} 行的 filter 应为 0"
            );
            assert_eq!(
                &inflated[row * (stride + 1) + 1..row * (stride + 1) + 1 + stride],
                &raw[row * stride..(row + 1) * stride],
                "第 {row} 行的像素"
            );
        }
        assert_eq!(
            encode_png(width, height, &raw).unwrap(),
            png,
            "编码必须完全确定 ✓"
        );
    }
}
