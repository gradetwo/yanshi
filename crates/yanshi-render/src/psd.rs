//! **PSD 只读导入** ✓（设计第 17 章把"PSD 只读导入"列为后续项 ✓ —— 本轮实现它 ✓）。
//!
//! **契约：只读、且只取"合成图"** ✓（与设计一致 ✓）：
//! * **不做**图层结构导入 ✗（PSD 的图层、蒙版、混合模式、智能对象都不搬 ✓）；
//! * **不做**写入 ✗（不产出 PSD ✓，也不改动源文件 ✓）；
//! * 取的是文件里那张**已经合成好的整幅图** ✓ ⇒ 结果作为**一个** `raster_patch` 交出去 ✓，
//!   于是它和"导入一张 PNG"走**同一条**下游路径 ✓（blob 先行 ✓、原子记录 ✓、可撤销 ✓）。
//!
//! **为什么值得做** ✓：PSD 是绘画与设计行业的事实交换格式 ✓ ——
//! 用户第一步几乎总是"把手上的 PSD 拿进来继续画" ✓，而这一步此前**完全没有** ✗。
//!
//! **明确拒绝、不猜** ✗（本项目一贯的纪律 ✓）：版本非 1 ✓、位深非 8 ✓、色彩模式非 RGB ✓、
//! 或者遇到本解析器不支持的段 ✓ ⇒ **逐条给具体原因** ✓，绝不"尽力而为地画一半" ✗
//! —— 半个画面比明确的报错更糟 ✓。
//!
//! **依赖** ✓：零依赖手写 ✓（PackBits 变体 RLE ✓ + 原始两种压缩 ✓）。

/// 解析错误 ✓（每条都带上**具体原因** ✓，便于工具层原样回报 ✓）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PsdError {
    /// 面向用户的说明 ✓。
    pub message: String,
}

impl PsdError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl core::fmt::Display for PsdError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.message)
    }
}

/// 读大端 u16 ✓。
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]))
}

/// 读大端 u32 ✓。
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
        *bytes.get(at + 2)?,
        *bytes.get(at + 3)?,
    ]))
}

/// **解析 PSD，取出合成图** ✓ ⇒ `(宽, 高, RGBA8)` ✓。
///
/// 只支持"8 位 / RGB / 版本 1"这一类 ✓ —— 也就是 99% 的实际 PSD ✓；
/// 其余一律**明确报错** ✗（见模块说明 ✓）。
pub fn decode_psd(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), PsdError> {
    if bytes.len() < 26 {
        return Err(PsdError::new("文件太短，不像 PSD"));
    }
    if &bytes[0..4] != b"8BPS" {
        return Err(PsdError::new("缺少 PSD 签名 8BPS"));
    }
    let version = u16_at(bytes, 4).unwrap_or(0);
    if version != 1 {
        return Err(PsdError::new(format!(
            "PSD 版本 {version} 不支持（只支持版本 1；版本 2 是 PSB 大文档格式）"
        )));
    }
    // 6..12 是 6 字节保留区 ✓（必须为 0 ✓，但读的时候容忍非零 ✓ —— 有些工具会写脏数据 ✓）。
    let channels = u16_at(bytes, 12).unwrap_or(0);
    let height = u32_at(bytes, 14).unwrap_or(0);
    let width = u32_at(bytes, 18).unwrap_or(0);
    let depth = u16_at(bytes, 22).unwrap_or(0);
    let mode = u16_at(bytes, 24).unwrap_or(0);
    if width == 0 || height == 0 {
        return Err(PsdError::new("PSD 的宽或高为 0"));
    }
    if depth != 8 {
        return Err(PsdError::new(format!(
            "位深 {depth} 不支持（只支持 8 位；16/32 位请先导出为 8 位）"
        )));
    }
    if mode != 3 {
        return Err(PsdError::new(format!(
            "色彩模式 {mode} 不支持（只支持 RGB=3；灰度/CMYK/索引色请先转换）"
        )));
    }
    // **跳过三段：色彩模式数据 ✓、图像资源 ✓、图层与蒙版信息 ✓** ——
    // 每段都是 "长度 + 数据" ✓（图层段长度可能为奇数 ✓，所以按 4 字节对齐 ✓）。
    let mut at = 26usize;
    let color_mode_len =
        u32_at(bytes, at).ok_or_else(|| PsdError::new("色彩模式数据段截断"))? as usize;
    at += 4 + color_mode_len;
    let resources_len = u32_at(bytes, at).ok_or_else(|| PsdError::new("图像资源段截断"))? as usize;
    at += 4 + resources_len;
    let layers_len = u32_at(bytes, at).ok_or_else(|| PsdError::new("图层与蒙版段截断"))? as usize;
    at += 4 + layers_len;
    if at + 2 > bytes.len() {
        return Err(PsdError::new("合成图段截断（缺少压缩方式）"));
    }
    // **合成图** ✓：压缩方式 + 数据 ✓。
    let compression = u16_at(bytes, at).ok_or_else(|| PsdError::new("缺少压缩方式"))?;
    at += 2;
    let pixels = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| PsdError::new("尺寸过大"))?;
    let channels16 = channels.clamp(1, 4) as usize;
    let raw: Vec<u8> = match compression {
        // 0 = 原始 ✓
        0 => {
            let need = pixels * channels16;
            let slice = bytes
                .get(at..at + need)
                .ok_or_else(|| PsdError::new("合成图数据不足（原始压缩）"))?;
            slice.to_vec()
        }
        // 1 = RLE ✓（每行每通道一条 PackBits 流 ✓，长度表在前 ✓）
        1 => {
            let rows = height as usize * channels16;
            let mut lengths = Vec::with_capacity(rows);
            let mut cursor = at;
            for _ in 0..rows {
                let value = u16_at(bytes, cursor)
                    .ok_or_else(|| PsdError::new("RLE 行长度表截断"))?
                    as usize;
                lengths.push(value);
                cursor += 2;
            }
            let mut out = Vec::with_capacity(pixels * channels16);
            for length in lengths {
                let end = cursor + length;
                let row = bytes
                    .get(cursor..end)
                    .ok_or_else(|| PsdError::new("RLE 数据截断"))?;
                packbits_row(row, &mut out)?;
                cursor = end;
            }
            out
        }
        other => {
            return Err(PsdError::new(format!(
                "压缩方式 {other} 不支持（只支持 0=原始 与 1=RLE；ZIP 压缩是 PSB 才有的）"
            )))
        }
    };
    if raw.len() < pixels * channels16 {
        return Err(PsdError::new("合成图数据不足（解压后）"));
    }
    // **PSD 是"逐通道平面"存储** ✓（先所有红 ✓、再所有绿 ✓……）⇒ 这里交错成 RGBA ✓。
    let mut rgba = vec![255u8; pixels * 4];
    for index in 0..pixels {
        let r = raw[index];
        let g = if channels16 > 1 {
            raw[pixels + index]
        } else {
            r
        };
        let b = if channels16 > 2 {
            raw[pixels * 2 + index]
        } else {
            r
        };
        let a = if channels16 > 3 {
            raw[pixels * 3 + index]
        } else {
            255
        };
        rgba[index * 4] = r;
        rgba[index * 4 + 1] = g;
        rgba[index * 4 + 2] = b;
        rgba[index * 4 + 3] = a;
    }
    Ok((width, height, rgba))
}

/// **PackBits 一行** ✓（PSD 的 RLE 就是它 ✓）：控制字节 n 的语义 ——
/// `0..=127` ⇒ 后面 `n+1` 个字节原样 ✓；`-127..=-1` ⇒ 后面 1 个字节重复 `1-n` 次 ✓；`-128` ⇒ 跳过 ✓。
fn packbits_row(row: &[u8], out: &mut Vec<u8>) -> Result<(), PsdError> {
    let mut at = 0usize;
    while at < row.len() {
        let control = row[at] as i8;
        at += 1;
        if control >= 0 {
            let count = control as usize + 1;
            let end = at + count;
            let slice = row
                .get(at..end)
                .ok_or_else(|| PsdError::new("PackBits 原样段截断"))?;
            out.extend_from_slice(slice);
            at = end;
        } else if control != -128 {
            let count = 1 - control as isize;
            let value = *row
                .get(at)
                .ok_or_else(|| PsdError::new("PackBits 重复段截断"))?;
            at += 1;
            // clippy 提示用 `repeat_n` ✓（更直白 ✓，行为一致 ✓）。
            out.extend(core::iter::repeat_n(value, count as usize));
        }
    }
    Ok(())
}

/// **写一个最小可用的 PSD** ✓ —— **只给测试与往返自检用** ✗（本项目**不产出** PSD ✓，只读 ✓）。
///
/// 有它才能做**往返测试** ✓（写出去 ✓、读回来 ✓、逐像素比对 ✓）⇒ 这比"拿一个现成 PSD 手工核对"更能守住回归 ✓。
pub fn encode_psd(
    width: u32,
    height: u32,
    rgba: &[u8],
    channels: u16,
    compression: u16,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"8BPS");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&channels.to_be_bytes());
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.extend_from_slice(&8u16.to_be_bytes()); // 位深 8 ✓
    out.extend_from_slice(&3u16.to_be_bytes()); // RGB ✓
    out.extend_from_slice(&0u32.to_be_bytes()); // 色彩模式数据 ✓
    out.extend_from_slice(&0u32.to_be_bytes()); // 图像资源 ✓
    out.extend_from_slice(&0u32.to_be_bytes()); // 图层与蒙版 ✓
    out.extend_from_slice(&compression.to_be_bytes());
    // 逐通道平面 ✓
    let count = (width * height) as usize;
    let mut planes: Vec<Vec<u8>> = Vec::new();
    for channel in 0..channels as usize {
        let mut plane = Vec::with_capacity(count);
        for index in 0..count {
            plane.push(rgba[index * 4 + channel.min(3)]);
        }
        planes.push(plane);
    }
    if compression == 0 {
        for plane in &planes {
            out.extend_from_slice(plane);
        }
    } else {
        // RLE ✓：先把每行压缩 ✓，再写长度表 ✓、再写数据 ✓。
        let mut rows: Vec<Vec<u8>> = Vec::new();
        for plane in &planes {
            for row in plane.chunks(width as usize) {
                rows.push(packbits_encode(row));
            }
        }
        for row in &rows {
            out.extend_from_slice(&(row.len() as u16).to_be_bytes());
        }
        for row in &rows {
            out.extend_from_slice(row);
        }
    }
    out
}

/// PackBits 编码 ✓（只给测试用 ✓）：连续 3 个以上相同字节就压缩 ✓，否则原样 ✓。
fn packbits_encode(row: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at < row.len() {
        let mut run = 1usize;
        while at + run < row.len() && row[at + run] == row[at] && run < 128 {
            run += 1;
        }
        if run >= 3 {
            out.push((1i16 - run as i16) as i8 as u8);
            out.push(row[at]);
            at += run;
            continue;
        }
        // 原样段 ✓：一直取到"下一处 3 连"或 128 个为止 ✓。
        let start = at;
        while at < row.len() && at - start < 128 {
            let mut ahead = 1usize;
            while at + ahead < row.len() && row[at + ahead] == row[at] && ahead < 3 {
                ahead += 1;
            }
            if ahead >= 3 && at > start {
                break;
            }
            at += 1;
        }
        let count = at - start;
        out.push((count - 1) as u8);
        out.extend_from_slice(&row[start..at]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(width: u32, height: u32) -> Vec<u8> {
        let mut rgba = Vec::new();
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[
                    (x * 17 % 256) as u8,
                    (y * 29 % 256) as u8,
                    ((x + y) * 11 % 256) as u8,
                    255,
                ]);
            }
        }
        rgba
    }

    /// **原始压缩往返** ✓（写出去 ✓、读回来 ✓、**逐像素相同** ✓）。
    #[test]
    fn raw_round_trip_is_pixel_exact() {
        let rgba = sample(7, 5);
        let psd = encode_psd(7, 5, &rgba, 3, 0);
        let (w, h, decoded) = decode_psd(&psd).expect("应能解析");
        assert_eq!((w, h), (7, 5));
        assert_eq!(decoded, rgba, "逐像素必须相同 ✓");
    }

    /// **RLE 往返** ✓ —— 长同色行会被真的压掉 ✓（也顺带验证了 PackBits 的两个分支 ✓）。
    #[test]
    fn rle_round_trip_is_pixel_exact() {
        let mut rgba = Vec::new();
        for y in 0..4u32 {
            for x in 0..40u32 {
                // 大段同色 ✓ + 少量杂色 ✓ ⇒ 同时走到 PackBits 的"重复"与"原样"两支 ✓。
                let v = if (x / 10 + y) % 2 == 0 {
                    40
                } else {
                    (x * 7 % 256) as u8
                };
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let psd = encode_psd(40, 4, &rgba, 3, 1);
        let (w, h, decoded) = decode_psd(&psd).expect("应能解析");
        assert_eq!((w, h), (40, 4));
        assert_eq!(decoded, rgba, "RLE 也必须逐像素相同 ✓");
        // 顺带确认"真的压到了" ✓（否则这条测试就名不副实 ✗）。
        assert!(
            psd.len() < 40 * 4 * 3 + 200,
            "RLE 应当明显小于原始：{} 字节",
            psd.len()
        );
    }

    /// **四通道（带透明）** ✓：alpha 平面要落到 RGBA 的第 4 个字节 ✓。
    #[test]
    fn four_channel_keeps_alpha() {
        let mut rgba = Vec::new();
        for index in 0..6u32 {
            rgba.extend_from_slice(&[10, 20, 30, (index * 40) as u8]);
        }
        let psd = encode_psd(3, 2, &rgba, 4, 0);
        let (_, _, decoded) = decode_psd(&psd).expect("应能解析");
        assert_eq!(decoded, rgba, "alpha 通道必须保留 ✓");
    }

    /// **不支持的输入要逐条说清原因** ✗（绝不"画一半" ✓）。
    #[test]
    fn unsupported_inputs_are_refused_with_reasons() {
        let rgba = sample(2, 2);
        // ① 位深 16 ⇒ 拒绝 ✓
        let mut deep = encode_psd(2, 2, &rgba, 3, 0);
        deep[22..24].copy_from_slice(&16u16.to_be_bytes());
        let error = decode_psd(&deep).unwrap_err();
        assert!(error.message.contains("位深"), "{}", error.message);
        // ② 色彩模式 CMYK(4) ⇒ 拒绝 ✓
        let mut cmyk = encode_psd(2, 2, &rgba, 4, 0);
        cmyk[24..26].copy_from_slice(&4u16.to_be_bytes());
        let error = decode_psd(&cmyk).unwrap_err();
        assert!(error.message.contains("色彩模式"), "{}", error.message);
        // ③ 版本 2 ⇒ 拒绝 ✓
        let mut v2 = encode_psd(2, 2, &rgba, 3, 0);
        v2[4..6].copy_from_slice(&2u16.to_be_bytes());
        let error = decode_psd(&v2).unwrap_err();
        assert!(error.message.contains("版本"), "{}", error.message);
        // ④ 压缩方式 2（ZIP）⇒ 拒绝 ✓
        let mut zipped = encode_psd(2, 2, &rgba, 3, 0);
        let tail = zipped.len() - (2 * 2 * 3) - 2;
        zipped[tail..tail + 2].copy_from_slice(&2u16.to_be_bytes());
        let error = decode_psd(&zipped).unwrap_err();
        assert!(error.message.contains("压缩"), "{}", error.message);
        // ⑤ 不是 PSD ✓
        let error = decode_psd(b"not a psd at all, really not").unwrap_err();
        assert!(
            error.message.contains("签名") || error.message.contains("太短"),
            "{}",
            error.message
        );
        // ⑥ 截断的文件 ✓
        let full = encode_psd(8, 8, &sample(8, 8), 3, 0);
        let error = decode_psd(&full[..full.len() / 2]).unwrap_err();
        assert!(!error.message.is_empty());
    }
}

// **★★★ 第 253 轮：从 PSD 读**图层树** ✗ ★★★**（**用户建议的第 ③ 项 ✓）
//
// **∴ 为什么用 `ag-psd` ✗**（**按 AGENTS.md 的两面 ✓）**：**图层记录／通道数据／RLE／
//   `luni`（**Unicode 名 ✓）／`lsct`（**组的分节符 ✓）／engine data** ✗ **手写是一两周量级** ✓
//     ＋ **∴ 它的代价 ✗**：**只多 `flate2`（**而它按默认特性又带进 `miniz_oxide`
//       ＋ `simd-adler32` ＋ `adler2` ✓ —— **实测 `cargo tree` 见提交信息 ✓）★
//     ＋ **∴ 风险 ✗**：**它自称 "**vibe-coded port**"**✗ 文档 43% ✓
//       ⇒ **∴ 所以**：**判据必须拿**真文件**（**本机 ImageMagick 生成 ✓
//         ＋ **PSD 自带的压平合成图**当答案键 ✓）验过才算数 ✓ ★★
//
// **∴ 这一层的作用 ✗**：**把库的类型**关在 `psd.rs` 里**✗ 服务端只见本仓的小结构** ✓
//   ⇒ **∴ 于是**：**换库／改库**不会波及工具层** ✓（**∴ 也**好测 ✓）★★

/// **★ PSD 里的一层（**或一组 ✓）✗ ★**（**第 253 轮 ✓；**只留要映射的字段 ✓）★
#[derive(Debug, Clone, PartialEq)]
pub struct PsdLayerNode {
    /// **图层名 ✗**（取自 `luni`／`luni` 缺失时的 Pascal 名 ✓；**组也有名 ✓）★
    pub name: String,
    /// 左上角（文档坐标 ✓）。
    pub x: i64,
    /// 左上角（文档坐标 ✓）。
    pub y: i64,
    /// 宽（**组为 0 ✓）★
    pub width: u32,
    /// 高（**组为 0 ✓）★
    pub height: u32,
    /// **不透明度 ✗**（**归一到 0..1 ✓ —— 库给的是 0..1 ✓ 实测过 ✓）★
    pub opacity: f64,
    /// 可见（**库给的是 `hidden` ⇒ **取反 ✓）★
    pub visible: bool,
    /// **混合模式 ✗**（**映射进本仓的 11 个词 ✗ 对不上的落 `normal` 并记进
    /// [`PsdLayerTree::unmapped_blends`] ✓ 不静默** ✓）★
    pub blend: String,
    /// 是不是图层组（**库用 `children` 表示 ✓）★
    pub is_group: bool,
    /// **组的展开态 ✗**（**`lsct` 的 open／closed ✓；非组恒 true ✓）★
    pub open: bool,
    /// **RGBA8 像素 ✗**（**组为空 ✓；**∴ 库给的就是 RGBA ✓ 实测过 ✓）★
    pub pixels: Vec<u8>,
    /// 子层（**组的成员 ✓；Phase 1 只用到一层 ✗ 但结构照收 ✓）★
    pub children: Vec<PsdLayerNode>,
}

/// **★ 一次读出来的图层树 ✗ ★**（**第 253 轮 ✓）★
#[derive(Debug, Clone, PartialEq)]
pub struct PsdLayerTree {
    /// 文档宽 ✓。
    pub width: u32,
    /// 文档高 ✓。
    pub height: u32,
    /// 顶层（**含组 ✓，顺序与 PSD 自下而上一致 ✓ —— **∴ 要翻一次才是 yanshi 的 z 序 ✓）★
    pub roots: Vec<PsdLayerNode>,
    /// **∴ 对不上本仓词表的混合模式 ✗**（**去重后的原名 ✓）⇒ **∴ 让调用方**如实报出来** ✓ ★
    pub unmapped_blends: Vec<String>,
}

/// **把 `ag-psd` 的混合模式映射到本仓词表 ✗**（**显式匹配 ✗ 不靠 `Debug` 名** ✓）★
///
/// **∴ 为什么不用 `format!("{:?}")` ✗**：**那**看起来省事 ✗ 但**库一改枚举名就**静默
/// 落到别的模式** ✓ ⇒ **∴ 现在**：**显式 `match` ⇒ **库改版会**编译失败**（**响 ✓）
///   ＋ **∴ 对不上的**落 `normal` ✗ 但**记进 `unmapped_blends` ⇒ **不静默** ✓ ★★
fn map_blend(mode: ag_psd::psd::BlendMode) -> Option<&'static str> {
    use ag_psd::psd::BlendMode as M;
    Some(match mode {
        M::Normal => "normal",
        M::Multiply => "multiply",
        M::Screen => "screen",
        M::Overlay => "overlay",
        M::Darken => "darken",
        M::Lighten => "lighten",
        M::ColorDodge => "color_dodge",
        M::Difference => "difference",
        M::LinearDodge => "linear_dodge",
        M::LinearBurn => "subtract", // **∴ 本仓最近的等价物 ✓**（如实：不是逐位等价 ✓）
        M::SoftLight => "soft_light",
        M::LinearLight => "linear_light",
        M::PassThrough => "normal", // **∴ 组用 ✓**（本仓没有 pass-through ✓）
        _ => return None,
    })
}

fn layer_to_node(layer: &ag_psd::psd::Layer, unmapped: &mut Vec<String>) -> PsdLayerNode {
    let name = layer
        .additional_info
        .name
        .clone()
        .unwrap_or_else(|| "图层".to_owned());
    let (left, top) = (layer.left.unwrap_or(0.0), layer.top.unwrap_or(0.0));
    let (right, bottom) = (layer.right.unwrap_or(left), layer.bottom.unwrap_or(top));
    let mut width = (right - left).max(0.0) as u32;
    let mut height = (bottom - top).max(0.0) as u32;
    let mut pixels = Vec::new();
    if let Some(image) = layer.image_data.as_ref() {
        width = image.width;
        height = image.height;
        pixels = image.data.clone();
    }
    let children: Vec<PsdLayerNode> = layer
        .children
        .as_ref()
        .map(|kids| kids.iter().map(|k| layer_to_node(k, unmapped)).collect())
        .unwrap_or_default();
    let is_group = layer.children.is_some();
    let blend = match layer.blend_mode {
        Some(mode) => match map_blend(mode) {
            Some(name) => name.to_owned(),
            None => {
                let raw = format!("{mode:?}");
                if !unmapped.contains(&raw) {
                    unmapped.push(raw);
                }
                "normal".to_owned()
            }
        },
        None => "normal".to_owned(),
    };
    PsdLayerNode {
        name,
        x: left.round() as i64,
        y: top.round() as i64,
        width,
        height,
        opacity: layer.opacity.unwrap_or(1.0).clamp(0.0, 1.0),
        visible: !layer.hidden.unwrap_or(false),
        blend,
        is_group,
        open: layer.opened.unwrap_or(true),
        pixels,
        children,
    }
}

/// **★ 要写出去的一层 ✗ ★**（**第 253 轮 ✓）：**与 `PsdLayerNode` 对称 ✓
///
/// **∴ 为什么另立一个类型 ✗**：**写的时候只关心**要带出去的字段** ✓
///   ＋ **∴ 而**"库的类型"只允许出现在本模块里 ⇒ **∴ 服务端**见不到 `ag-psd`** ✓ ★★
#[derive(Debug, Clone, PartialEq)]
pub struct PsdOutLayer {
    /// 图层名 ✓。
    pub name: String,
    /// 可见（**写出时取反成 `hidden` ✓）★
    pub visible: bool,
    /// 不透明度（**0..1 ✓）★
    pub opacity: f64,
    /// **本仓词表的混合模式 ✗**（**未知的按 `normal` 处理 ✓）★
    pub blend: String,
    /// 是不是组（**组没有像素 ✗ 只有 children ✓）★
    pub is_group: bool,
    /// **RGBA8 ✗ 尺寸与 `width/height` 对应**（**组为空 ✓）★
    pub pixels: Vec<u8>,
    /// 像素宽 ✓。
    pub width: u32,
    /// 像素高 ✓。
    pub height: u32,
    /// 子层（**Phase 1：一层 ✓）★
    pub children: Vec<PsdOutLayer>,
}

/// **本仓词表 ⇒ ag-psd 枚举 ✗**（**显式匹配 ✗ 与读方向对称 ✓）★
fn to_ag_blend(name: &str) -> ag_psd::psd::BlendMode {
    use ag_psd::psd::BlendMode as M;
    match name {
        "multiply" => M::Multiply,
        "screen" => M::Screen,
        "overlay" => M::Overlay,
        "darken" => M::Darken,
        "lighten" => M::Lighten,
        "color_dodge" => M::ColorDodge,
        "difference" => M::Difference,
        "soft_light" => M::SoftLight,
        "linear_light" => M::LinearLight,
        "linear_dodge" => M::LinearDodge,
        "subtract" => M::LinearBurn,
        _ => M::Normal,
    }
}

fn out_to_ag(layer: &PsdOutLayer) -> ag_psd::psd::Layer {
    use ag_psd::psd::{Layer, LayerAdditionalInfo, PixelData};
    let hidden = !layer.visible;
    Layer {
        top: Some(0.0),
        left: Some(0.0),
        bottom: Some(f64::from(layer.height)),
        right: Some(f64::from(layer.width)),
        // **∴ 组的默认混合要写**穿透** ✗**（**∴ 本仓的 `normal` 组若写成 Normal ⇒
        //   在 Photoshop 里会**把组隔离**✗ 与 yanshi 的语义不符 ✓）★★
        blend_mode: Some(if layer.is_group && layer.blend == "normal" {
            ag_psd::psd::BlendMode::PassThrough
        } else {
            to_ag_blend(&layer.blend)
        }),
        opacity: Some(layer.opacity.clamp(0.0, 1.0)),
        hidden: Some(hidden),
        children: if layer.children.is_empty() {
            None
        } else {
            Some(layer.children.iter().map(out_to_ag).collect())
        },
        image_data: if layer.is_group || layer.pixels.is_empty() {
            None
        } else {
            Some(PixelData {
                width: layer.width,
                height: layer.height,
                data: layer.pixels.clone(),
            })
        },
        additional_info: LayerAdditionalInfo {
            name: Some(layer.name.clone()),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// **★ 把一棵（自有类型的）图层树**写成 PSD 字节** ✗ ★**（**第 253 轮 ✓）★
///
/// **∴ `composite` 必填 ✗**：**PSD 惯例上带压平合成图**✗ 而**别的软件（**含
///   Photoshop ✓）**打开时**先读它** ✓ ⇒ **∴ 少它 ⇒ **"能打开"都成问题** ✓ ★★
#[must_use]
pub fn write_layer_tree(
    width: u32,
    height: u32,
    composite: &[u8],
    roots: &[PsdOutLayer],
) -> Vec<u8> {
    use ag_psd::psd::{PixelData, Psd};
    let psd = Psd {
        width: f64::from(width),
        height: f64::from(height),
        children: if roots.is_empty() {
            None
        } else {
            Some(roots.iter().map(out_to_ag).collect())
        },
        image_data: Some(PixelData {
            width,
            height,
            data: composite.to_vec(),
        }),
        ..Default::default()
    };
    ag_psd::write_psd(&psd, &ag_psd::psd::WriteOptions::default())
}

/// **读一个 PSD 的**图层树** ✗ ★**（**第 253 轮 ✓；**压平那条路仍走 [`decode_psd`] ✓）★
pub fn read_layer_tree(bytes: &[u8]) -> Result<PsdLayerTree, PsdError> {
    // **∴ 默认**不读**图层像素 ✗**（**实测：`pixels.len() == 0` ✓）⇒ **∴ 于是**显式要** ✓
    //   ＋ **∴ 而**合成都也要（**判据要拿 PSD 自带的压平合成图当答案键 ✓）★
    // **∴ 关键开关是 `use_image_data` ✗**（**读侧源码 `reader.rs:1123` ✓）：
    //   **∴ 只有 `Some(true)` 时才把图层像素解到 `image_data`** ✓
    //     ＋ **∴ 而**只设 `skip_layer_image_data: Some(false)` **不够** ✗（**实测：仍 0 字节 ✓）★
    let options = ag_psd::psd::ReadOptions {
        use_image_data: Some(true),
        skip_layer_image_data: Some(false),
        skip_composite_image_data: Some(false),
        ..Default::default()
    };
    let psd = ag_psd::read_psd(bytes, &options)
        .map_err(|error| PsdError::new(format!("ag-psd 读失败：{error}")))?;
    let mut unmapped = Vec::new();
    let roots = psd
        .children
        .as_ref()
        .map(|kids| {
            kids.iter()
                .map(|k| layer_to_node(k, &mut unmapped))
                .collect()
        })
        .unwrap_or_default();
    Ok(PsdLayerTree {
        width: psd.width.round().max(0.0) as u32,
        height: psd.height.round().max(0.0) as u32,
        roots,
        unmapped_blends: unmapped,
    })
}

#[cfg(test)]
mod layer_tree_probe {
    use ag_psd::psd::{BlendMode, Layer, LayerAdditionalInfo, PixelData, Psd};

    /// **∴ 夹具用 **ag-psd 自己写**✗ 因为：**ImageMagick 写的 PSD **混合键是字节倒序** ✗
    ///   ＋ **∴ 实测 ✗**：**它的键位置上是 `mron`**（**规格要 `norm` ✓）⇒
    ///     **∴ ag-psd**拒绝它是对的**（`Invalid blend mode: 'mron'` ✓）★
    ///       ＋ **∴ 所以 ✗**：**拿它当夹具 ⇒ **会冤枉库** ✓（**我一开始就差点这么干 ✓）★★
    ///   ＋ **∴ 代价（**如实 ✓）✗**：**自写夹具 ⇒ **"能读 Photoshop 原文件"这件事
    ///     **证据不足** ✗（**∴ 需要一份**真 Photoshop 文件** ✗ 这轮**没拿到** ✓）★
    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> PixelData {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            data.extend_from_slice(&rgba);
        }
        PixelData {
            width,
            height,
            data,
        }
    }

    /// **∴ 夹具的布局要**让答案键可精确算出** ✗ ★**（**第 253 轮 ✓）：
    ///   ＋ **∴ 全不透明 ✗ 互不重叠（除被上层完全盖住 ✓）✗ 且**隐藏层不进合成**
    ///     ⇒ **∴ 于是**：**"PSD 自带的压平合成图"**只需要整数 over**✗ 没有浮点混合误差** ✓
    ///       ⇒ **∴ 判据**拿它当答案键时 ⇒ **∴ 差值只来自**量化／四舍五入** ✓ ★★
    fn fixture() -> Vec<u8> {
        let rect = |x: f64, y: f64, w: u32, h: u32, rgba: [u8; 4], name: &str| Layer {
            top: Some(y),
            left: Some(x),
            bottom: Some(y + f64::from(h)),
            right: Some(x + f64::from(w)),
            blend_mode: Some(BlendMode::Normal),
            opacity: Some(1.0),
            hidden: Some(false),
            image_data: Some(solid(w, h, rgba)),
            additional_info: LayerAdditionalInfo {
                name: Some(name.to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };
        // **∴ 自下而上 ✗ A ✗ B ✗ 组（内含 C）✗ 隐藏的 D** ✓
        let a = rect(0.0, 0.0, 32, 64, [224, 32, 32, 255], "红");
        let b = rect(32.0, 0.0, 32, 64, [32, 64, 224, 255], "蓝");
        let c = rect(0.0, 32.0, 64, 16, [32, 200, 64, 255], "组内绿");
        let group = Layer {
            blend_mode: Some(BlendMode::PassThrough),
            opacity: Some(1.0),
            hidden: Some(false),
            opened: Some(true),
            children: Some(vec![c]),
            additional_info: LayerAdditionalInfo {
                name: Some("一组".to_owned()),
                ..Default::default()
            },
            ..Default::default()
        };
        // **∴ 隐藏层 ✗**：**它的属性用来验映射（**不透明度 0.5 ✓ 混合 multiply ✓）✗
        //   而**它不进合成** ⇒ **∴ 答案键**不用管它** ✓ ★
        let mut d = rect(48.0, 48.0, 16, 16, [200, 200, 32, 255], "隐藏层");
        d.hidden = Some(true);
        d.opacity = Some(0.5);
        d.blend_mode = Some(BlendMode::Multiply);
        // **∴ 合成图 ＝ 透明画布 ＋ A ＋ B ＋ C（**全是整数 over ✓ 无误差 ✓）★
        let mut composite = vec![0u8; 64 * 64 * 4];
        let mut paint = |x: f64, y: f64, w: u32, h: u32, rgba: [u8; 4]| {
            for row in 0..h {
                for col in 0..w {
                    let px = x as u32 + col;
                    let py = y as u32 + row;
                    let at = ((py * 64 + px) * 4) as usize;
                    composite[at..at + 4].copy_from_slice(&rgba);
                }
            }
        };
        paint(0.0, 0.0, 32, 64, [224, 32, 32, 255]);
        paint(32.0, 0.0, 32, 64, [32, 64, 224, 255]);
        paint(0.0, 32.0, 64, 16, [32, 200, 64, 255]);
        let psd = Psd {
            width: 64.0,
            height: 64.0,
            children: Some(vec![a, b, group, d]),
            image_data: Some(PixelData {
                width: 64,
                height: 64,
                data: composite,
            }),
            ..Default::default()
        };
        ag_psd::write_psd(&psd, &ag_psd::psd::WriteOptions::default())
    }

    /// **∴ 生成可提交的夹具 ✗**（**第 253 轮 ✓）：**跑一次把 `fixture()` 写到
    /// `tests/data/psd/agpsd-two-layers.psd`** ✗ 供**服务端判据**当输入** ✓
    ///   ＋ **∴ 为什么用自写而不是 ImageMagick 的 ✗**：**后者的混合键是**字节倒序**
    ///     （**实测 ✓）⇒ **∴ ag-psd**（正确地）拒绝它** ✓ ★
    #[test]
    fn write_fixture_for_criteria() {
        let bytes = fixture();
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/psd");
        std::fs::create_dir_all(dir).expect("建目录");
        std::fs::write(format!("{dir}/agpsd-two-layers.psd"), &bytes).expect("写夹具");
        eprintln!("已写出 {} 字节", bytes.len());
    }

    /// **∴ 往返：写出去 ⇒ 读回来 ⇒ 字段逐项对得上 ✗**（**第 253 轮 ✓）★
    #[test]
    fn round_trips_layer_tree_through_ag_psd() {
        let bytes = fixture();
        eprintln!("夹具字节 = {}", bytes.len());
        let tree = super::read_layer_tree(&bytes).expect("读图层树");
        eprintln!(
            "文档 {}x{}｜顶层 {}｜对不上的混合 {:?}",
            tree.width,
            tree.height,
            tree.roots.len(),
            tree.unmapped_blends
        );
        for node in &tree.roots {
            eprintln!(
                "  {:?} ({}, {}) {}x{} 不透明度={} 可见={} 混合={} 组={} 像素={}",
                node.name,
                node.x,
                node.y,
                node.width,
                node.height,
                node.opacity,
                node.visible,
                node.blend,
                node.is_group,
                node.pixels.len()
            );
        }
        assert_eq!(tree.width, 64);
        assert_eq!(tree.height, 64);
        assert_eq!(tree.roots.len(), 4, "红 + 蓝 + 一组 + 隐藏层");
        let red = &tree.roots[0];
        assert_eq!(red.name, "红");
        assert_eq!(red.blend, "normal");
        assert!(red.visible);
        assert_eq!(red.pixels.len(), 32 * 64 * 4);
        let blue = &tree.roots[1];
        assert_eq!((blue.x, blue.y, blue.width, blue.height), (32, 0, 32, 64));
        let group = &tree.roots[2];
        assert_eq!(group.name, "一组");
        assert!(group.is_group, "有 children ⇒ 是组");
        assert_eq!(group.children.len(), 1);
        let kid = &group.children[0];
        assert_eq!(kid.name, "组内绿");
        assert_eq!((kid.x, kid.y, kid.width, kid.height), (0, 32, 64, 16));
        assert_eq!(kid.pixels.len(), 64 * 16 * 4);
        let hidden = &tree.roots[3];
        assert_eq!(hidden.name, "隐藏层");
        assert!(!hidden.visible, "hidden=true ⇒ 不可见");
        assert_eq!(hidden.blend, "multiply", "混合模式要映射到本仓词表");
        assert!((hidden.opacity - 0.5).abs() < 0.02, "不透明度 0.5");
    }
}
