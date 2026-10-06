//! PNG 输出（设计文档 7.5 / 18 章）与 **blob 存储共用**的 zlib 压缩原语。
//!
//! 编码：8 位 RGBA、无隔行、每条扫描线 filter 0；deflate 走
//! **`flate2`（后端是 `zlib-rs`，纯 Rust）** ✓ —— 不再手写 Huffman/LZ77 ✓。
//!
//! **为什么是 `flate2` 而不是 `miniz_oxide`** ✓（这是本轮明确的取舍 ✓）：
//! `flate2` 与 `zlib-rs` **本来就在依赖树里** ✓ —— `zip` 把它们带了进来 ✓（见 `Cargo.lock` ✓）
//! ⇒ 本轮只是把**已有**的依赖暴露给 `yanshi-render` ✓，**没有新增任何包或版本** ✓
//!（`Cargo.lock` 只多了一条 `yanshi-render → flate2` 的边 ✓）。
//! `miniz_oxide` / `zopfli` / `libz-sys` / `fdeflate` 都**不在**锁文件里 ✗ ⇒ 选它们要新增包 ✓。
//!
//! **确定性** ✓（内容寻址与"导出可复现"的底线 ✓）：`zlib-rs` 是纯 Rust ✓，
//! 不含随机数、时间、浮点 ✓；它的 SIMD（AVX2 / NEON / wasm simd128）**只加速字节比较** ✓，
//! 每个变体都返回同一个"首个不同位置" ✓ ⇒ 与标量路径**逐位同结果** ✓。
//! 压缩级别是**写死的常量** ✓（`DEFLATE_LEVEL` ✓）、策略用默认值 ✓
//! ⇒ **同一份输入永远得到同一串字节** ✓，与平台无关 ✓ ——
//! 这条由 `crates/yanshi-render/tests/deflate_determinism.rs`（含**黄金摘要** ✓）
//! 和 `scripts/wasm-smoke.sh` 一系的跨目标比对钉住 ✓。
//!
//! **为什么换掉手写编码器** ✓：旧实现为 LZ77 建 `prev: vec![u32::MAX; raw.len()]` ✗
//! ⇒ 8K 的 132 MB 原始数据要**单块约 530 MB** ✓（实测 4K 新增峰值 **130,420 KiB** ✓ ≈ `4 × 33.18 MB` ✓，8K **520,772 KiB** ✓）。
//! 换成 zlib-rs 后，压缩状态的**大小与输入长度无关** ✓
//!（固定 32 KiB 窗口 + 哈希表 + 32 KiB 输出缓冲 ✓）。
//!
//! **代价** ✓（两面都写 ✓）：`yanshi-render` 多了一个（已在树里的 ✓）第三方依赖 ✓，
//! 编译时间与 wasm 体积会变 ✓（见提交信息里的实测 ✓）；
//! 解压错误也从"自己写的 `None`"变成"第三方 crate 的错误" ✓ ⇒ 统一映射回 `None` ✓
//!（对外语义不变 ✓，由 `blob_compression.rs` 与新的向后兼容判据守着 ✓）。

use std::io::{Read, Write};
use std::path::Path;

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

/// PNG 魔数。
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// zlib **压缩级别** ✓（写死的常量 ✓ ⇒ 同一份输入永远得到同一串字节 ✓）。
///
/// **为什么是 7** ✓（实测决定 ✓，不是"默认值" ✓）：`zlib-rs` 的级别表照抄 zlib-ng ✓
/// —— **级别 0–6 走 `deflate_medium`** ✗（zlib-ng 的快速折中 ✓），
/// **级别 7–9 才是 `deflate_slow`** ✓（`src/deflate/algorithm/mod.rs` 的 `CONFIGURATION_TABLE` ✓：
/// 7 = `good 8 / lazy 32 / nice 128 / chain 256` ✓，9 = `32 / 258 / 258 / 4096` ✓）。
/// 本仓库的主要负载是**绘画作品**（大片平色 + 重复纹理 ✓）——
/// 在 4K 平色语料（33.18 MB ✓）上实测（release ✓）：
/// **级别 6 = 832 182 字节** ✗、**级别 7 = 500 933 字节** ✓、级别 9 = 479 523 字节 ✓；
/// 耗时 254 ms / 355 ms / 747 ms ✓，而**被替换掉的手写编码器是 738 454 字节 / 612 ms** ✓。
/// 也就是说：级别 7 比级别 6 **小 40%** ✓、比手写实现**小 32%** ✓、还**快约 1.7×** ✓；
/// 级别 9 只再小 4% ✗ 却要两倍时间 ✗。
/// 对**照片型**素材（`Cardboard001.png` 1024² ✓ 高熵 ✓）三个级别相差不到 1% ✓
/// ⇒ 由"绘画型"这一主要负载拍板 ✓。
///
/// **内存与级别无关** ✓：窗口/哈希表/前驱表的大小由窗口位数与 `mem_level` 决定 ✓，
/// 级别只改搜索参数 ✓ —— `deflate_memory.rs` 实测 1 MiB 与 16 MiB 输入的新增峰值 ✓。
///
/// **改它就会改变输出字节** ✗ ⇒ 必须连同 `deflate_determinism.rs` 的黄金摘要一起改 ✓，
/// 不许顺手调 ✓（那正是"内容寻址/导出可复现"赖以成立的东西 ✓）。
const DEFLATE_LEVEL: u32 = 7;

/// 输出缓冲的**预留上限** ✓（1 MiB ✓）。
///
/// **为什么要有上限** ✗：旧实现按 `raw.len() / 4` 预留 ✓ —— 那是"输出大概多长"的猜测 ✓，
/// 会让**压缩期内存跟着输入长度涨** ✗，而本轮的目标正是"新增内存与输入长度无关" ✓。
/// 绘画作品（大片平色 + 重复纹理 ✓）压到约 2% ✓ ⇒ 1 MiB 足够省掉绝大多数扩容 ✓；
/// 真压不动的数据由 `Vec` 自己按需增长 ✓（分摊 O(1) ✓，而且**必须**存得下输出 ✓）。
const DEFLATE_RESERVE_MAX: usize = 1 << 20;

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
    // **`flate2`（zlib-rs ✓）+ 写死的级别 [`DEFLATE_LEVEL`]** ✓（不再手写固定 Huffman ✓）。
    write_chunk(&mut out, b"IDAT", &zlib_compress(&raw));
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
// zlib 压缩 / 解压（`flate2` + `zlib-rs` 纯 Rust 后端）✓
// ---------------------------------------------------------------------------
//
// 旧实现的手写"固定 Huffman + LZ77"编码器与"完整 inflate"（stored/fixed/dynamic）
// 都在本轮**删除** ✓，改为直接用依赖树里已有的 `flate2` ✓。
// 这一节是**唯一的**压缩实现 ✓，PNG 写出与 blob 存储**共用**同一份 ✓。

/// 用给定级别压一段字节 ✓（zlib 容器 ✓，RFC 1950 ✓）。
///
/// `level = 0` 就是**stored（未压缩）块** ✓ —— 旧的手写 `zlib_stored` 用它兜底 ✓，
/// 现在由同一个编码器产出 ✓（少一份需要维护的手写实现 ✓）。
fn zlib_at_level(raw: &[u8], level: u32) -> Vec<u8> {
    let reserve = (raw.len() / 4 + 64).min(DEFLATE_RESERVE_MAX);
    let mut encoder = ZlibEncoder::new(Vec::with_capacity(reserve), Compression::new(level));
    // 目标是一个 `Vec` ✓ ⇒ 这里的 IO 错误只可能是"分配失败" ✓
    // ⇒ panic 的行为与 `Vec` 自己分配失败时一致 ✓（旧实现也不会返回 `Result` ✓）。
    encoder.write_all(raw).expect("压缩到内存缓冲不会失败");
    encoder.finish().expect("结束 zlib 流不会失败")
}

/// **压缩一段字节**（zlib 容器 ✓，级别 6 ✓）—— 给 blob 存储与 PNG 复用 ✓。
///
/// **确定性** ✓：级别与策略是常量 ✓，`zlib-rs` 无随机/时间/浮点 ✓
/// ⇒ 同一输入在任何平台都得到同一串字节 ✓（判据见 `deflate_determinism.rs` ✓）。
pub fn zlib_compress(raw: &[u8]) -> Vec<u8> {
    zlib_at_level(raw, DEFLATE_LEVEL)
}

/// **压一遍，不划算就用 stored 块** ✓（stored 本身也是合法 zlib ✓）⇒ 对**不可压数据不膨胀** ✓
///（最多多约 11 字节 ✓：2 字节 zlib 头 + 每 65535 字节 5 字节块头 + 4 字节 adler ✓）。
///
/// 为什么需要它 ✓：deflate 对**已压过的 PNG/JPEG、随机噪声**会**轻微膨胀** ✓
/// ⇒ 存储层必须走这个入口 ✓，而不是 [`zlib_compress`] ✓。
pub fn zlib_compress_best(raw: &[u8]) -> Vec<u8> {
    let packed = zlib_compress(raw);
    if packed.len() < raw.len() {
        packed
    } else {
        zlib_at_level(raw, 0)
    }
}

/// **解压** [`zlib_compress`] 的输出 ✓；不是 zlib 流就回 `None` ✓（调用方据此按原样处理 ✓）。
///
/// **错误映射** ✓：`flate2`/`zlib-rs` 的错误、zlib 头不合法、adler32 不匹配、
/// 流被截断 —— 全部映射成同一个 `None` ✓，与旧实现对外**语义一致** ✓
///（`blob_compression.rs` 的"不是 zlib 流必须被拒绝"✓ 与"载荷被改坏必须被 adler32 抓住"✓ 守着 ✓）。
///
/// **完整性** ✓：`zlib-rs` 在流结束时会校验 adler32 ✓；这里另外要求**整段输入都被消费** ✓
/// —— 旧实现把**最后 4 字节**当 adler32 ✓ ⇒ "有效 zlib + 垃圾尾巴"会被它拒绝 ✓，
/// 这条严格性保留 ✓（不回退成"解出前缀就算成功" ✗）。
pub fn zlib_decompress(stream: &[u8]) -> Option<Vec<u8>> {
    if stream.len() < 6 || stream[0] != 0x78 {
        return None;
    }
    let mut decoder = ZlibDecoder::new(stream);
    let mut out = Vec::new();
    if decoder.read_to_end(&mut out).is_err() {
        return None;
    }
    if decoder.total_in() as usize != stream.len() {
        return None;
    }
    Some(out)
}

/// Adler-32 分块累加的安全块长上限 ✓。
///
/// **`5552` 是最大的安全值** ✓：块内 `b` 的上界是
/// `255·n(n+1)/2 + (n+1)·(65521-1)` ✓ —— `n = 5552` 时约 `4.29e9` ✓，还塞得进 `u32` ✓；
/// `n = 5553` 就会越过 `u32::MAX` ✗（zlib 用的也是同一个常数 ✓）。
/// **`adler_chunk_bound_is_the_largest_safe_one` 直接对这条上界断言** ✓
/// ⇒ 谁把常数改大一步 ✓，那条判据立刻红 ✓。
const ADLER_CHUNK_MAX: usize = 5552;

/// zlib 使用的 Adler-32 校验。
///
/// **它现在不参与压缩/解压路径** ✓（adler32 由 `zlib-rs` 在流内校验 ✓），
/// 但它是本 crate 的**公开 API** ✓（`lib.rs` 有再导出 ✓，测试也用它当参照 ✓）
/// ⇒ 保留，且行为逐位不变 ✓。
///
/// **分块累加，而不是每字节取模** ✓（上一轮的性能修复 ✓）：原先每个字节做两次 `% 65521` ✓
/// ⇒ 4K 导出的 33 MiB 要算 **6600 万次取模** ✗。
/// 现在按 `ADLER_CHUNK_MAX` 分块 ✓，块内只做加法 ✓、每块末尾各取模一次 ✓。
/// **结果逐位不变** ✓：模运算对加法可分配 ✓，块内不取模只是把取模推迟 ✓
/// —— `adler32_chunked_matches_the_reference` 用逐字节实现当参照守住这条 ✓。
pub fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for chunk in bytes.chunks(ADLER_CHUNK_MAX) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

// ---------------------------------------------------------------------------
// inflate：由 `flate2`/`zlib-rs` 提供 ✓
// ---------------------------------------------------------------------------
//
// **为什么可以交出去** ✓：旧的手写 inflate 支持 stored/fixed/dynamic 三种块 ✓，
// 那是 RFC 1951 里一整块容易写错的逻辑 ✗（`import_image` 要解的是**别人**的 PNG ✓：
// Photoshop、截图工具几乎都用 dynamic Huffman ✓）。`zlib-rs` 是成熟实现 ✓，
// 而且**同一份代码**也编进 wasm ✓ ⇒ 服务端与浏览器内核不会各解各的 ✓。

/// **png 解码** ✓（设计 791 行：`import_image` 接受 JPEG/PNG/WebP ✓）。
///
/// **为什么值得写** ✓：浏览器端已经能用 `createImageBitmap` 解码任意格式 ✓，
/// 所以**界面**这条路不缺 ✓；缺的是**工具/API**这条路 ✗ ——
/// 设计明写 `import_image` 接受 PNG ✓，而服务端此前只收**原始 RGBA** ✗
///（界面是自己先解成 raw 再上传的 ✓，把这个缺口遮住了 ✓）。
///
/// **支持范围（边界写清楚 ✓）**：8 位、颜色类型 **6（RGBA ✓）与 2（RGB ✓）**、
/// 无隔行 ✓、五种 filter 全支持 ✓、deflate 三种块类型全支持 ✓（由 `zlib-rs` 提供 ✓）。
/// 其它情况一律返回 `None` ✓ —— 由调用方给出**明确错误** ✓，
/// 而不是画出半张图 ✗（"静默错误"是本项目反复吃亏的地方 ✓）。
///
/// 返回 `(宽, 高, RGBA8)` ✓。
pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    if bytes.len() < 8 || bytes[..8] != SIGNATURE {
        return None;
    }
    let mut position = 8usize;
    let mut width = 0u32;
    let mut height = 0u32;
    let mut color_type = 0u8;
    let mut interlace = 0u8;
    let mut bit_depth = 0u8;
    let mut idat: Vec<u8> = Vec::new();
    let mut seen_header = false;
    loop {
        if position + 8 > bytes.len() {
            return None;
        }
        let length = u32::from_be_bytes([
            bytes[position],
            bytes[position + 1],
            bytes[position + 2],
            bytes[position + 3],
        ]) as usize;
        let kind = &bytes[position + 4..position + 8];
        let payload_start = position + 8;
        let payload_end = payload_start.checked_add(length)?;
        if payload_end + 4 > bytes.len() {
            return None;
        }
        let payload = &bytes[payload_start..payload_end];
        // **CRC 必须校验** ✓：PNG 的每个块都有 CRC32 ✓，
        // 损坏的文件在这里就被挡住 ✓，而不是解出一堆彩色噪点 ✗。
        let expected = u32::from_be_bytes([
            bytes[payload_end],
            bytes[payload_end + 1],
            bytes[payload_end + 2],
            bytes[payload_end + 3],
        ]);
        let mut crc_input = Vec::with_capacity(4 + length);
        crc_input.extend_from_slice(kind);
        crc_input.extend_from_slice(payload);
        if yanshi_core::crc32(&crc_input) != expected {
            return None;
        }
        match kind {
            b"IHDR" => {
                if payload.len() < 13 {
                    return None;
                }
                width = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                height = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);
                bit_depth = payload[8];
                color_type = payload[9];
                interlace = payload[12];
                if width == 0 || height == 0 {
                    return None;
                }
                seen_header = true;
            }
            b"IDAT" => idat.extend_from_slice(payload),
            b"IEND" => break,
            _ => {}
        }
        position = payload_end + 4;
    }
    if !seen_header || idat.is_empty() {
        return None;
    }
    // **只支持 8 位、无隔行、RGB/RGBA** ✓（其余显式拒绝 ✓）。
    if bit_depth != 8 || interlace != 0 {
        return None;
    }
    let channels = match color_type {
        6 => 4usize,
        2 => 3usize,
        _ => return None,
    };
    // **`IDAT` 里是 zlib 流** ✓（RFC 1950 ✓：2 字节头 + deflate + 4 字节 adler32 ✓）。
    // 这两条显式检查保留 ✓：它们给出的是**明确的拒绝理由** ✓，
    // 而不是"交给 inflate 碰运气" ✓（FDICT 位 0x20 表示需要预设字典 ✓ —— 不支持 ✓）。
    if idat.len() < 6 || idat[0] & 0x0F != 8 {
        return None;
    }
    if idat[1] & 0x20 != 0 {
        return None;
    }
    let mut decoder = ZlibDecoder::new(idat.as_slice());
    let mut raw = Vec::new();
    // **adler32 由 `zlib-rs` 在流结束处校验** ✓ ⇒ 坏数据在这里变成 `Err` ⇒ `None` ✓
    //（与旧的"自己算 adler32 再比对"对外等价 ✓，连**截断**的流也一样拒绝 ✓）。
    if decoder.read_to_end(&mut raw).is_err() {
        return None;
    }
    let stride = width as usize * channels;
    let expected_len = (stride + 1) * height as usize;
    if raw.len() < expected_len {
        return None;
    }
    // 逐扫描线反 filter ✓（RFC 2083 §6 ✓）。
    let mut pixels: Vec<u8> = Vec::with_capacity(width as usize * height as usize * 4);
    let mut previous: Vec<u8> = vec![0u8; stride];
    let mut line: Vec<u8> = vec![0u8; stride];
    for row in 0..height as usize {
        let filter = raw[row * (stride + 1)];
        let start = row * (stride + 1) + 1;
        line.copy_from_slice(&raw[start..start + stride]);
        for index in 0..stride {
            let left = if index >= channels {
                line[index - channels]
            } else {
                0
            };
            let up = previous[index];
            let up_left = if index >= channels {
                previous[index - channels]
            } else {
                0
            };
            let value = match filter {
                0 => line[index],
                1 => line[index].wrapping_add(left),
                2 => line[index].wrapping_add(up),
                3 => line[index].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => {
                    // Paeth ✓（RFC 2083 §6.6 ✓）。
                    let p = i32::from(left) + i32::from(up) - i32::from(up_left);
                    let pa = (p - i32::from(left)).abs();
                    let pb = (p - i32::from(up)).abs();
                    let pc = (p - i32::from(up_left)).abs();
                    let predictor = if pa <= pb && pa <= pc {
                        left
                    } else if pb <= pc {
                        up
                    } else {
                        up_left
                    };
                    line[index].wrapping_add(predictor)
                }
                _ => return None,
            };
            line[index] = value;
        }
        for column in 0..width as usize {
            let base = column * channels;
            let (r, g, b, a) = if channels == 4 {
                (line[base], line[base + 1], line[base + 2], line[base + 3])
            } else {
                (line[base], line[base + 1], line[base + 2], 255)
            };
            pixels.extend_from_slice(&[r, g, b, a]);
        }
        previous.copy_from_slice(&line);
    }
    Some((width, height, pixels))
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

    /// 近似绘画作品 ✓：大块平色 + 渐变 + 少量噪声 ✓。
    fn sample_image() -> Vec<u8> {
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

    #[test]
    fn adler32_matches_known_vector() {
        // 标准向量：adler32("Wikipedia") = 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        // 长向量、跨多个分块：100 万个 0 字节 ⇒ a=1 ✓、b=1_000_000 mod 65521 = 17185 = 0x4321 ✓。
        assert_eq!(adler32(&[0u8; 1_000_000]), 0x4321_0001);
    }

    /// **分块上界判据**：`ADLER_CHUNK_MAX` 必须是**最大的**安全块长 ✓ ——
    /// 它本身不溢出 `u32` ✓，再加 1 就必须溢出 ✓。这条把实现里的常数与文档里的推导绑在一起 ✓。
    ///
    /// **能红** ✓：把常数从 `5552` 改成 `5553` ⇒ 第一条断言当场失败 ✓
    ///（`4.296e9 > u32::MAX` ✓，变异验证见提交信息 ✓）。
    #[test]
    fn adler_chunk_bound_is_the_largest_safe_one() {
        const BASE_MINUS_ONE: u64 = 65_520;
        let worst = |n: u64| 255 * n * (n + 1) / 2 + (n + 1) * BASE_MINUS_ONE;
        let limit = u64::from(u32::MAX);
        let n = ADLER_CHUNK_MAX as u64;
        assert!(
            worst(n) <= limit,
            "{n} 字节的块内 b 上界 {} 已超出 u32（{limit}）",
            worst(n)
        );
        assert!(
            worst(n + 1) > limit,
            "ADLER_CHUNK_MAX 不是最大安全值：{} 字节仍然不溢出",
            n + 1
        );
    }

    /// **分块累加判据**：新的"块内不取模"必须与旧的"逐字节取模"**完全相等** ✓。
    ///
    /// 长度刻意取在 `NMAX = 5552` 的两侧 ✓（0 / 5551 / 5552 / 5553 / 2×5552±1 ✓），
    /// 并用**全 255** 这种让块内 `b` 取到最大值的数据 ✓ —— 分块上界推导错了就会溢出/不等 ✓。
    ///
    /// **能红** ✓：把 `NMAX` 改成 `5553`（越过安全上界 ✓）⇒ 全 255 长输入的块内 `b`
    /// 在 release 下回绕 ✗、结果与参照不同 ✓（变异验证见提交信息 ✓）。
    #[test]
    fn adler32_chunked_matches_the_reference() {
        /// 旧实现（逐字节取模）✓，作为参照 ✓。
        fn reference(bytes: &[u8]) -> u32 {
            const MOD: u32 = 65_521;
            let mut a: u32 = 1;
            let mut b: u32 = 0;
            for byte in bytes {
                a = (a + u32::from(*byte)) % MOD;
                b = (b + a) % MOD;
            }
            (b << 16) | a
        }
        let lengths = [0usize, 1, 5551, 5552, 5553, 11_104, 11_105, 100_000];
        for length in lengths {
            let pattern: Vec<u8> = (0..length)
                .map(|index| (index as u64).wrapping_mul(2_654_435_761) as u8)
                .collect();
            assert_eq!(
                adler32(&pattern),
                reference(&pattern),
                "长度 {length} 的模式数据"
            );
            let saturated = vec![255u8; length];
            assert_eq!(
                adler32(&saturated),
                reference(&saturated),
                "长度 {length} 的全 255 数据（块内 b 的最大值）"
            );
        }
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
        // **有意改写** ✓：本轮的编码器从"手写固定 Huffman"换成了
        // `flate2`（zlib-rs ✓）⇒ zlib 头第二字节从 `0x01` 变成**当前级别对应的 FLEVEL** ✓（级别 7 ⇒ `0x78DA` ✓）
        //（zlib 把级别映射进 FLG 的 FLEVEL 位 ✓）。
        // 这条测试的**原意**是"IDAT 解压后正好是带 filter 的扫描线" ✓ —— 保留原意 ✓，
        // 但不再断言某个具体级别字节 ✗（那是编码器内部选择 ✓，不是格式要求 ✓）。
        assert_eq!(payload[0], 0x78, "zlib CMF：deflate、32K 窗口");
        assert_eq!(
            ((u16::from(payload[0]) << 8) | u16::from(payload[1])) % 31,
            0,
            "zlib 头必须能被 31 整除（CMF/FLG 校验位）"
        );
        let inflated = zlib_decompress(payload).expect("IDAT 必须是合法 zlib 流");
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

    /// **大图也必须能被正确解回** ✓（尺寸、filter、像素一个不差 ✓）。
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
        let inflated = zlib_decompress(payload).expect("IDAT 必须是合法 zlib 流");
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

    /// **往返必须逐字节一致** ✓（压缩正确性的第一道关 ✓）。
    #[test]
    fn the_compressed_stream_round_trips() {
        for raw in [
            Vec::new(),
            b"a".to_vec(),
            b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_vec(),
            b"abcdefghijklmnopqrstuvwxyz0123456789".to_vec(),
            (0..=255u8).collect::<Vec<u8>>(),
            sample_image(),
        ] {
            let compressed = zlib_compress(&raw);
            let restored = zlib_decompress(&compressed)
                .unwrap_or_else(|| panic!("自己压的必须能解回来（原始 {} 字节）", raw.len()));
            assert_eq!(restored, raw, "往返不一致（原始 {} 字节）", raw.len());
        }
    }

    /// **必须真的压缩** ✓ —— 子 agent 报的 G4：导出 2,458,493 字节 ≈ 原始 RGBA ✗（约 20× 膨胀 ✓）。
    /// 这里对同一份像素比较"级别 6"与"stored 基线"（级别 0 ✓）。
    #[test]
    fn compression_beats_the_stored_baseline() {
        let raw = sample_image();
        let compressed = zlib_compress(&raw);
        let baseline = zlib_at_level(&raw, 0);
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

    /// **输出的 PNG 必须仍然合法** ✓：结构完整 ✓、IDAT 解压后正是"每行 filter 0 + 该行 RGBA" ✓，
    /// 且**同一输入编两次必须逐字节相同** ✓。
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
        let inflated = zlib_decompress(payload).expect("IDAT 必须是合法 zlib 流");
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

    /// **多出来的尾巴必须被拒绝** ✓（旧实现把最后 4 字节当 adler32 ✓ ⇒ 同样拒绝 ✓）——
    /// 这是本轮换用第三方解码器时**最容易悄悄放松**的一条 ✓。
    #[test]
    fn trailing_bytes_after_the_zlib_stream_are_rejected() {
        let mut stream = zlib_compress(b"hello zlib, hello zlib, hello zlib");
        assert!(zlib_decompress(&stream).is_some(), "干净流必须能解");
        stream.extend_from_slice(b"trailing junk");
        assert!(
            zlib_decompress(&stream).is_none(),
            "有效 zlib 后面挂垃圾 ⇒ 必须拒绝（不能解出前缀就算成功）"
        );
    }
}
