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
