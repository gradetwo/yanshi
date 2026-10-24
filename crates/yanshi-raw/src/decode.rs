//! 解码边界（设计 5.2）：**全仓唯一允许 `use rawler` 的文件**（治理规矩同
//! 「photo.rs 是唯一 JPEG 编解码接点」）。职责只有两件：把文件字节解码成
//! rawler 的 `RawImage`，再把 `RawImage` 映射成本仓自有的 `RawFrame`——
//! rawler 类型绝不外泄到展开管线（解码器可换、管线与判据不动）。
//!
//! ⚠️ 绝不调用 rawler 的 `apply_scaling` 做归一化：它内部的
//! `correct_blacklevel_cfa` 会把负值 clip 到 0，与设计 5.2「下界不钳、
//! 出口才截零」直接冲突。归一化在 `develop::normalize_mosaic` 自行实现。
//!
//! 已知解码器缺口（rawler 0.7.2 实测，见包 README 缺口清单）：
//! - **BaselineExposure 读不到**：标签常量存在（DNG tag 50730），但解码器
//!   不往 `RawImage` 上挂（`dng_tags` 从未被任何 decoder 填充）；
//!   本模块如实返回 `baseline_exposure_ev: None`，不许硬编、不许猜。
//! - `RawImage::cropped_cfa()` 在 0.7.2 是 `todo!()`，相位对齐在本模块
//!   按 rawler 自家口径手工完成（见 `map_frame` 的相位换算注释）。
//! - **方向（Orientation）**：rawler 0.7.2 的 `RawImage` 构造器把
//!   orientation 写死 Normal（旁注 TODO fixme），构造后只有 DNG 与
//!   QTK 解码器回填——其余格式（RW2／ORF／CR2／NEF／ARW…）的真值只在
//!   EXIF 里。本模块以 `resolve_orientation` 拾取（EXIF 优先、字段
//!   回退），frame 与 file_meta 同源同值。⚠️ 迁移注记：导入期即展开
//!   落盘，**修复前已导入的竖拍文档位图已经是横的**，本修复只对新
//!   导入生效，存量文档的正解是重新导入。

use rawler::decoders::RawDecodeParams;
use rawler::imgop::Rect;
use rawler::rawsource::RawSource;
use rawler::{Orientation, RawImage, RawImageData};

use crate::error::{RawError, Result};
use crate::frame::{
    CalibrationMatrix, Channel, DecodedRaw, LensExif, RawFileMeta, RawFrame, RecordContext,
};

/// 解码器身份（留痕记录用）。版本字面与 Cargo.toml 的 rawler 依赖锁定一致，
/// 升级依赖时必须同步改这里——`decoder_version_matches_lockfile` 测试守着。
pub const DECODER_NAME: &str = "rawler";
pub const DECODER_VERSION: &str = "0.7.2";

/// 字节 → 解码产物（帧 + 文件元数据）。任一环节失败走 `RawError`，不 panic。
pub fn decode_bytes(bytes: &[u8]) -> Result<DecodedRaw> {
    let source_format = sniff_format(bytes);
    let source = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&source)
        .map_err(|e| RawError::Decode(format!("无法识别 RAW 格式：{e}")))?;
    let params = RawDecodeParams::default();
    let raw = decoder
        .raw_image(&source, &params, false)
        .map_err(|e| RawError::Decode(e.to_string()))?;
    // 元数据整表失败不阻断展开（设计 5.7）：取不到就 None，展开照常。
    let mut metadata = decoder.raw_metadata(&source, &params).ok();
    // rawler 0.7.2 的通用 EXIF 路径不读 LensMake／LensModel（只有 RAF／TFR
    // 解码器单独补读，decoders/raf.rs:406）——镜头名是 raw_meta 与 D5 提示的
    // 依据，在此经解码器公开的 ifd() 接口从 EXIF IFD 补读，口径与 RAF 一致。
    // 补读失败不连累解码：镜头名缺失给 None（设计 5.7）。
    if let Some(metadata) = &mut metadata {
        if metadata.exif.lens_make.is_none() || metadata.exif.lens_model.is_none() {
            use rawler::decoders::WellKnownIFD;
            use rawler::tags::ExifTag;
            if let Ok(Some(exif_ifd)) = decoder.ifd(WellKnownIFD::Exif) {
                if metadata.exif.lens_make.is_none() {
                    metadata.exif.lens_make = exif_ifd
                        .get_entry(ExifTag::LensMake)
                        .and_then(|entry| entry.value.as_string().cloned());
                }
                if metadata.exif.lens_model.is_none() {
                    metadata.exif.lens_model = exif_ifd
                        .get_entry(ExifTag::LensModel)
                        .and_then(|entry| entry.value.as_string().cloned());
                }
            }
        }
    }
    let mut decoded = map_decoded(raw, metadata.as_ref(), &source_format)?;
    attach_file_lens_and_crop(&mut decoded, decoder.as_ref(), metadata.as_ref());
    Ok(decoded)
}

/// 第四包补挂：文件自带镜头校正（DNG OpcodeList2）与本图画幅系数。
/// 两者都只在 DNG/TIFF 系文件里经 IFD 可得；取不到不报错——镜头库
/// 路径照常走，只是逐项优先时少了文件一侧（导入注记会说明画幅
/// 换算按 s=1 的事实，不在这里静默）。
fn attach_file_lens_and_crop(
    decoded: &mut DecodedRaw,
    decoder: &dyn rawler::decoders::Decoder,
    metadata: Option<&rawler::decoders::RawMetadata>,
) {
    use rawler::decoders::WellKnownIFD;
    use rawler::formats::tiff::Value;
    use rawler::tags::DngTag;
    // OpcodeList2（DNG tag 51009）：先 Raw IFD 再根 IFD（各家 DNG
    // 写法落点不一，两处都探）。
    for wk in [WellKnownIFD::Raw, WellKnownIFD::Root] {
        if let Ok(Some(ifd)) = decoder.ifd(wk) {
            if let Some(entry) = ifd.get_entry(DngTag::OpcodeList2) {
                let bytes = match &entry.value {
                    Value::Undefined(bytes) | Value::Byte(bytes) => Some(bytes.clone()),
                    _ => None,
                };
                if let Some(bytes) = bytes {
                    let (parsed, mut notes) = crate::dng_opcode::parse_opcode_list2(&bytes);
                    decoded.file_lens = parsed;
                    if let Some(file_lens) = &decoded.file_lens {
                        if !file_lens.skipped_opcodes.is_empty() {
                            notes.push(format!(
                                "文件 OpcodeList2 中未执行的 opcode：{:?}（本引擎只执行 WarpRectilinear 与 FixVignetteRadial）",
                                file_lens.skipped_opcodes
                            ));
                        }
                    }
                    decoded.notes.append(&mut notes);
                    break;
                }
            }
        }
    }
    // 画幅系数：EXIF 35mm 等效焦距 / 实际焦距。rawler 的 Exif 结构
    // 未收 35mm 等效字段，经 EXIF IFD 直读（口径同镜头名补读）。
    let focal = metadata
        .and_then(|m| m.exif.focal_length)
        .map(|r| r.as_f32());
    if let (Some(focal), Ok(Some(exif_ifd))) = (focal, decoder.ifd(WellKnownIFD::Exif)) {
        use rawler::tags::ExifTag;
        if let Some(entry) = exif_ifd.get_entry(ExifTag::FocalLengthIn35mmFormat) {
            let equiv = match &entry.value {
                Value::Short(values) => values.first().map(|&v| v as f32),
                Value::Long(values) => values.first().map(|&v| v as f32),
                _ => None,
            };
            if let Some(equiv) = equiv {
                if focal > 0.0 && equiv > 0.0 {
                    decoded.crop_factor = Some(equiv / focal);
                }
            }
        }
    }
}

/// `RawImage` → 自有帧 + 元数据（映射规则全在这一处，设计 5.2）。
fn map_decoded(
    raw: RawImage,
    metadata: Option<&rawler::decoders::RawMetadata>,
    source_format: &str,
) -> Result<DecodedRaw> {
    if raw.cpp != 1 {
        return Err(RawError::UnsupportedData(format!(
            "每像素采样数 cpp={}（已是多通道数据），一期只支持单通道马赛克",
            raw.cpp
        )));
    }
    let (full_w, full_h) = (raw.width, raw.height);
    // active area 裁剪（设计 5.2：展开坐标系 = active area）。裁剪窗
    // 先算（C1-a，期 2）：像素只物化裁剪区——直接从整数源逐行转 f32
    // 进目标 Vec，不再先整幅转 f32 再拷一份裁剪（旧形态下全幅 f32 与
    // 裁剪 f32 同时存活，大图多吃一整幅 4 B/px）。转换是逐值恒等
    // 映射（u16 → f32 精确），产物与旧路逐位相同。
    let (ax, ay, crop_w, crop_h, mut notes) = active_crop(&raw, full_w, full_h);
    // 马赛克像素：整数读数原样进 f32；浮点读数一期不支持（HDR 浮点 DNG 等，
    // 量纲约定未定版，不许拍脑袋选缩放），响亮报错、记 README 缺口。
    let mosaic: Vec<f32> = match &raw.data {
        RawImageData::Integer(values) => {
            if values.len() != full_w * full_h {
                return Err(RawError::InvalidFrame(format!(
                    "解码像素数 {} 与全幅尺寸 {full_w}×{full_h} 对不上",
                    values.len()
                )));
            }
            let mut mosaic = Vec::with_capacity(crop_w * crop_h);
            for y in 0..crop_h {
                let start = (ay + y) * full_w + ax;
                mosaic.extend(values[start..start + crop_w].iter().map(|&v| v as f32));
            }
            mosaic
        }
        RawImageData::Float(_) => {
            return Err(RawError::UnsupportedData(
                "浮点马赛克数据（float DNG 等）一期不支持".to_string(),
            ));
        }
    };
    // CFA 相位对齐（关键）：rawler 的 `camera.cfa` 锚在**全幅数据原点**
    // （DNG 解码器会把文件内 active-area 相对的 CFAPattern 反向平移回全幅
    // 口径，见 rawler dng.rs `get_cfa`），黑／白电平的四相位数组同理锚在
    // 数据原点（rawler 自家 `correct_blacklevel_cfa` 即按 (y%2)*2+(x%2)
    // 作用于未裁数据）。因此裁后帧的相位下标要按裁剪原点 (ax, ay) 的奇偶
    // 换算，绝不能把全幅相位数组直接套到裁后帧上。
    let cfa = derive_cfa(&raw, ax, ay, crop_w, crop_h)?;
    // 电平数组：levels 为空时 rawler 的 as_bayer_array 会 panic，先守一道。
    if raw.blacklevel.levels.is_empty() || raw.whitelevel.0.is_empty() {
        return Err(RawError::InvalidFrame("解码器未给出黑／白电平".to_string()));
    }
    let period = cfa.period();
    // 黑电平带自报 RepeatDim（rawler BlackLevel.width/height），按周期口径换算。
    let black_full: Vec<f32> = raw.blacklevel.levels.iter().map(|v| v.as_f32()).collect();
    let black_levels = remap_levels(
        &black_full,
        raw.blacklevel.width,
        raw.blacklevel.height,
        period,
        ax,
        ay,
    )?;
    // 白电平无自报形状：长度 1 ⇒ 全相位同值；长度 = period² ⇒ 逐相位。
    let white_full: Vec<f32> = raw.whitelevel.0.iter().map(|&v| v as f32).collect();
    let (white_w, white_h) = match white_full.len() {
        1 => (1, 1),
        n if n == period * period => (period, period),
        n => {
            return Err(RawError::InvalidFrame(format!(
                "白电平长度 {n} 与 CFA 周期 {period} 对不齐"
            )));
        }
    };
    let white_levels = remap_levels(&white_full, white_w, white_h, period, ax, ay)?;
    // WB：wb_coeffs 是 RGBE 四元组，Bayer 一期只取 RGB 三项（第 4 项 E 通道
    // 在 DNG 路径上是 NaN 占位，误读会毒化整链）。任一项非法 ⇒ 解码期拒绝，
    // 不把坏乘子留给展开期。
    let wb = [raw.wb_coeffs[0], raw.wb_coeffs[1], raw.wb_coeffs[2]];
    let wb_multipliers = if wb.iter().all(|&v| v > 0.0 && v.is_finite()) {
        Some(wb)
    } else {
        None
    };
    let as_shot_neutral = wb_multipliers.map(|w| [1.0 / w[0], 1.0 / w[1], 1.0 / w[2]]);
    // 色彩标定矩阵：HashMap<Illuminant, 平铺矩阵> → 自有结构（9 元取 3×3；
    // 12 元是 RGBE 四通道，取前三行 RGB）。色温由光源枚举映射（标准 CCT）。
    let mut calibrations: Vec<CalibrationMatrix> = Vec::new();
    for (illuminant, flat) in &raw.color_matrix {
        let matrix = match flat.len() {
            9 | 12 => {
                let mut m = [[0.0f32; 3]; 3];
                for i in 0..3 {
                    for j in 0..3 {
                        m[i][j] = flat[i * 3 + j];
                    }
                }
                m
            }
            other => {
                notes.push(format!(
                    "色彩标定矩阵长度为 {other}（预期 9/12），已跳过该组标定"
                ));
                continue;
            }
        };
        calibrations.push(CalibrationMatrix {
            illuminant_kelvin: illuminant_kelvin(*illuminant),
            xyz_to_cam: matrix,
        });
    }
    // 双标定按色温升序；超过两组时保留色温两端（rawler 实际至多双标定）。
    calibrations.sort_by(|a, b| {
        a.illuminant_kelvin
            .unwrap_or(f32::INFINITY)
            .partial_cmp(&b.illuminant_kelvin.unwrap_or(f32::INFINITY))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if calibrations.len() > 2 {
        let last = calibrations[calibrations.len() - 1];
        let first = calibrations[0];
        calibrations = vec![first, last];
        notes.push("色彩标定矩阵超过两组，仅保留色温两端".to_string());
    }
    // 方向拾取（期 1 B 项）：EXIF 优先、解码器字段回退，frame 与
    // file_meta 同源同值。回退发生时把事实写进 notes——尤其非 DNG
    // 文件 EXIF 缺失时，解码器字段恒为 Normal、未必是真值，不许
    // 静默按 1 糊过去。
    let exif_orientation = metadata.and_then(|m| m.exif.orientation);
    let orientation = resolve_orientation(raw.orientation, exif_orientation);
    if !matches!(exif_orientation, Some(v) if (1..=8).contains(&v)) {
        notes.push(match exif_orientation {
            Some(v) => format!(
                "EXIF Orientation 值 {v} 超出 1–8，已忽略；方向按解码器字段取 {orientation}"
            ),
            None => format!(
                "文件无可用 EXIF Orientation，方向按解码器字段取 {orientation}（rawler 0.7.2 对非 DNG／QTK 格式该字段恒为 Normal，未必是真值）"
            ),
        });
    }
    let frame = RawFrame {
        width: crop_w,
        height: crop_h,
        mosaic,
        cfa,
        black_levels,
        white_levels,
        wb_multipliers,
        as_shot_neutral,
        color_matrices: calibrations,
        // rawler 0.7.2 未暴露 BaselineExposure（本文件头注的实测缺口）：
        // 如实 None；develop 会按 0 处理并写进展开注记，不许在此硬编。
        baseline_exposure_ev: None,
        orientation,
    };
    let file_meta = map_file_meta(&raw, metadata, source_format, orientation);
    Ok(DecodedRaw {
        frame,
        file_meta,
        notes,
        // 画幅与文件自带校正由 decode_bytes 在解码器在场时补挂
        // （map_decoded 只拿得到 RawImage，拿不到 IFD）；此处先给
        // 缺省，decode_bytes 覆盖。
        crop_factor: None,
        file_lens: None,
    })
}

/// active area → (左, 上, 宽, 高)。缺失 = 全幅；形状非法 = 回退全幅
/// （宽高口径以解码器为准，个别老文件的 ActiveArea 写错不许让展开全灭，
/// 但回退事实要留痕——见调用处 notes）。
fn active_crop(
    raw: &RawImage,
    full_w: usize,
    full_h: usize,
) -> (usize, usize, usize, usize, Vec<String>) {
    let mut notes = Vec::new();
    let Some(rect) = &raw.active_area else {
        return (0, 0, full_w, full_h, notes);
    };
    let [l, t, r, b] = rect_ltrb(rect);
    if r <= l || b <= t || r > full_w || b > full_h {
        notes.push(format!(
            "ActiveArea [{l}, {t}, {r}, {b}] 形状非法，已回退为全幅 {full_w}×{full_h}"
        ));
        return (0, 0, full_w, full_h, notes);
    }
    (l, t, r - l, b - t, notes)
}

fn rect_ltrb(rect: &Rect) -> [usize; 4] {
    rect.as_ltrb()
}

/// 由全幅 CFA 在裁后帧上推导 CFA 网格，并用探针点校验周期性
/// （设计 5.2 推广，ISSUE-3）。支持任意方阵周期：Bayer（2×2，须命中
/// 四排列之一，严格性与旧口径一致）与 X-Trans（6×6）同路；非方阵、
/// 含非 RGB 通道、探针不符 ⇒ 明确拒绝，不许硬套解马赛克。
fn derive_cfa(
    raw: &RawImage,
    ax: usize,
    ay: usize,
    crop_w: usize,
    crop_h: usize,
) -> Result<crate::frame::CfaGrid> {
    let cfa = &raw.camera.cfa;
    if !cfa.is_valid() {
        return Err(RawError::UnsupportedData(
            "解码器未给出有效 CFA（单色／线性数据一期不支持）".to_string(),
        ));
    }
    let channel = |fx: usize, fy: usize| -> Option<Channel> {
        match cfa.color_at(fy, fx) {
            0 => Some(Channel::R),
            1 => Some(Channel::G),
            2 => Some(Channel::B),
            _ => None,
        }
    };
    if cfa.width != cfa.height || cfa.width < 2 {
        return Err(RawError::NonBayerCfa {
            pattern: cfa.name.clone(),
        });
    }
    let period = cfa.width;
    let mut cells = Vec::with_capacity(period * period);
    for y in 0..period {
        for x in 0..period {
            let Some(c) = channel(ax + x, ay + y) else {
                return Err(RawError::NonBayerCfa {
                    pattern: cfa.name.clone(),
                });
            };
            cells.push(c);
        }
    }
    if period == 2 {
        let cell = [cells[0], cells[1], cells[2], cells[3]];
        let Some(pattern) = crate::frame::CfaPattern::from_cell(cell) else {
            return Err(RawError::NonBayerCfa {
                pattern: cfa.name.clone(),
            });
        };
        // 2-周期探针：裁后帧的四角 + 中心邻域，全幅坐标逐点对照推导出的排列。
        let probes_x = [
            0usize,
            1,
            crop_w / 2,
            crop_w.saturating_sub(2),
            crop_w.saturating_sub(1),
        ];
        let probes_y = [
            0usize,
            1,
            crop_h / 2,
            crop_h.saturating_sub(2),
            crop_h.saturating_sub(1),
        ];
        for &y in &probes_y {
            for &x in &probes_x {
                if x >= crop_w || y >= crop_h {
                    continue;
                }
                if channel(ax + x, ay + y) != Some(pattern.channel_at(x, y)) {
                    return Err(RawError::NonBayerCfa {
                        pattern: cfa.name.clone(),
                    });
                }
            }
        }
        return Ok(crate::frame::CfaGrid::from_bayer(pattern));
    }
    let grid = crate::frame::CfaGrid::new(period, cells, cfa.name.clone())?;
    // 周期探针：四角 + 中心各取一整周期块，全幅坐标逐格对照推导出的网格
    // （防解码器给出的周期与真实排列不符、网格在帧内漂移）。
    let anchors_x = [0usize, crop_w / 2, crop_w.saturating_sub(period)];
    let anchors_y = [0usize, crop_h / 2, crop_h.saturating_sub(period)];
    for &oy in &anchors_y {
        for &ox in &anchors_x {
            if ox + period > crop_w || oy + period > crop_h {
                continue;
            }
            for dy in 0..period {
                for dx in 0..period {
                    if channel(ax + ox + dx, ay + oy + dy)
                        != Some(grid.channel_at(ox + dx, oy + dy))
                    {
                        return Err(RawError::NonBayerCfa {
                            pattern: cfa.name.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(grid)
}

/// 全幅相位电平 → 裁后帧相位电平（周期 N 口径）：裁后相位 (x, y) 处的
/// 电平等于全幅相位 ((ax+x)%N, (ay+y)%N) 处的电平（两边同为 N-周期）。
/// 源数组须按其自报的 RepeatDim (src_w × src_h) 解释：与 CFA 同周期
/// （N×N）时逐相位换算；1×1 时全相位广播同值；其余形状与目标周期
/// 对不齐 ⇒ 响亮报错，不许猜（旧 2×2 口径是本函数的 N=2 特例）。
fn remap_levels(
    full: &[f32],
    src_w: usize,
    src_h: usize,
    period: usize,
    ax: usize,
    ay: usize,
) -> Result<Vec<f32>> {
    if full.len() == 1 && src_w == 1 && src_h == 1 {
        return Ok(vec![full[0]; period * period]);
    }
    if src_w != period || src_h != period || full.len() != period * period {
        return Err(RawError::InvalidFrame(format!(
            "电平数组形状 {src_w}×{src_h}（{} 元）与 CFA 周期 {period} 对不齐",
            full.len()
        )));
    }
    let mut out = vec![0.0f32; period * period];
    for (p, slot) in out.iter_mut().enumerate() {
        let (x, y) = (p % period, p / period);
        *slot = full[((ay + y) % period) * period + ((ax + x) % period)];
    }
    Ok(out)
}

/// 文件级元数据映射（设计 5.7 的 `raw_meta` 原料）。整表缺失 ⇒ 全 None，
/// 不阻断展开；镜头口径 focal/fnumber 取 EXIF 实测（定版不取镜头库标称值，
/// 5.6／Q4 裁决④：库的三元组在归一化下退化为标量，不许冒充修正）。
fn map_file_meta(
    raw: &RawImage,
    metadata: Option<&rawler::decoders::RawMetadata>,
    source_format: &str,
    orientation: u32,
) -> RawFileMeta {
    let exif = metadata.map(|m| &m.exif);
    let lens_from_desc = metadata.and_then(|m| m.lens.as_ref()).map(|lens| {
        if lens.lens_make.is_empty() {
            lens.lens_model.clone()
        } else {
            format!("{} {}", lens.lens_make, lens.lens_model)
        }
    });
    let lens_from_exif =
        exif.and_then(
            |e| match (e.lens_make.as_deref(), e.lens_model.as_deref()) {
                (Some(make), Some(model)) => Some(format!("{make} {model}")),
                (None, Some(model)) => Some(model.to_string()),
                (Some(make), None) => Some(make.to_string()),
                (None, None) => None,
            },
        );
    let focal_mm = exif.and_then(|e| e.focal_length).map(|r| r.as_f32());
    let fnumber = exif.and_then(|e| e.fnumber).map(|r| r.as_f32());
    let lens_exif = if exif.is_some() || lens_from_desc.is_some() {
        Some(LensExif {
            make: exif.and_then(|e| e.lens_make.clone()),
            model: exif.and_then(|e| e.lens_model.clone()).or_else(|| {
                metadata
                    .and_then(|m| m.lens.as_ref())
                    .map(|l| l.lens_model.clone())
            }),
            focal_mm,
            fnumber,
            focus_distance_m: exif.and_then(|e| e.subject_distance).map(|r| r.as_f32()),
        })
    } else {
        None
    };
    RawFileMeta {
        make: Some(raw.make.clone()),
        model: Some(raw.model.clone()),
        lens: lens_from_desc.or(lens_from_exif),
        lens_exif,
        iso: exif.and_then(|e| e.iso_speed.or_else(|| e.iso_speed_ratings.map(u32::from))),
        exposure_time: exif.and_then(|e| e.exposure_time).map(|r| r.as_f32()),
        fnumber,
        focal_mm,
        date_time_original: exif.and_then(|e| e.date_time_original.clone()),
        // 与 frame 同源：用 map_decoded 已解析的方向值，不许在此另算一遍。
        orientation,
        bit_depth: Some(raw.bps as u32),
        source_format: source_format.to_string(),
    }
}

/// 方向解析（期 1 B 项）：EXIF Orientation 优先，解码器字段回退。
///
/// rawler 0.7.2 的 `RawImage.orientation` 只对 DNG／QTK 可信（构造器
/// 写死 Normal、两解码器回填），其余格式的真值在
/// `metadata.exif.orientation`。EXIF 值合法（1–8）即采用；缺失或
/// 非法 ⇒ 回退解码器字段（`Unknown` 经 `orientation_u8` 兜底为 1）。
/// 纯函数 ⇒ 解析口径可单测钉死，不依赖任何真文件。
fn resolve_orientation(raw_orientation: Orientation, exif_tag: Option<u16>) -> u32 {
    match exif_tag {
        Some(v) if (1..=8).contains(&v) => u32::from(v),
        _ => orientation_u8(raw_orientation),
    }
}

/// `rawler::Orientation` → EXIF Orientation 数值（1–8，标准 TIFF 口径）。
fn orientation_u8(orientation: Orientation) -> u32 {
    match orientation {
        Orientation::Normal => 1,
        Orientation::HorizontalFlip => 2,
        Orientation::Rotate180 => 3,
        Orientation::VerticalFlip => 4,
        Orientation::Transpose => 5,
        Orientation::Rotate90 => 6,
        Orientation::Transverse => 7,
        Orientation::Rotate270 => 8,
        Orientation::Unknown => 1,
    }
}

/// 标定光源 → 相关色温（标准 CCT，DNG/CIE 口径）。
/// 无 CCT 定版值的光源（荧光灯各型等）返回 `None`——不许编一个数字进去，
/// 复合色彩链遇 `None` 端会退单矩阵并注明（`develop::composite_color_matrix`）。
fn illuminant_kelvin(illuminant: rawler::imgop::xyz::Illuminant) -> Option<f32> {
    use rawler::imgop::xyz::Illuminant::*;
    match illuminant {
        A | Tungsten => Some(2856.0),
        B => Some(4889.0),
        C => Some(6774.0),
        D50 => Some(5000.0),
        D55 | Daylight | Flash | FineWeather => Some(5500.0),
        D65 | CloudyWeather => Some(6504.0),
        D75 | Shade => Some(7500.0),
        IsoStudioTungsten => Some(3200.0),
        _ => None,
    }
}

/// 源格式 sniff（留痕记录用）：只认文件头魔数，不猜机型。
/// TIFF 家族细分（DNG 与厂商 TIFF-RAW 魔数相同）靠解码成功后的标签区分
/// 不在本包范围：TIFF 一律记 "tiff"，DNG 的确切识别随导入链包补。
fn sniff_format(bytes: &[u8]) -> String {
    if bytes.len() >= 16 && &bytes[0..16] == b"FUJIFILMCCD-RAW " {
        return "raf".to_string();
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        if bytes[8..12] == *b"crx " {
            return "cr3".to_string();
        }
        return "bmff".to_string();
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"II\x55\x00" {
        return "rw2".to_string();
    }
    if bytes.len() >= 4 && &bytes[0..4] == b"IIRO" {
        return "orf".to_string();
    }
    if bytes.len() >= 4 && (&bytes[0..4] == b"II\x2a\x00" || &bytes[0..4] == b"MM\x00\x2a") {
        return "tiff".to_string();
    }
    "raw".to_string()
}

/// RAW 容器 magic 签名白名单（探针第一道门；过门还要 rawler 认得出）。
///
/// TIFF 家族之外必须显式列变体签名，否则真 RAW 在第一道门就被误杀：
/// Panasonic RW2 的版本字是 0x55（`II\x55\x00`）、Olympus ORF 是
/// `IIRO`——两者都不是 TIFF 42（`II\x2a\x00`），rawler 本可解码
/// （2026-10-24 展示册选材实测：两格式真文件在此被拒、展开链无此门）。
fn has_raw_magic(bytes: &[u8]) -> bool {
    (bytes.len() >= 4
        && (&bytes[0..4] == b"II\x2a\x00"
            || &bytes[0..4] == b"MM\x00\x2a"
            || &bytes[0..4] == b"II\x55\x00"
            || &bytes[0..4] == b"IIRO"))
        || (bytes.len() >= 16 && &bytes[0..16] == b"FUJIFILMCCD-RAW ")
        || (bytes.len() >= 12 && &bytes[4..8] == b"ftyp" && &bytes[8..12] == b"crx ")
}

/// 入库识别探针（设计 5.3，第二包）：判断一份字节是不是相机 RAW。
///
/// 口径 = **magic 签名 ∧ 解码器认得出**，两条都过才算：
/// - magic 只认 RAW 容器签名（TIFF 家族含 RW2／ORF 变体签名／FUJIFILMCCD-RAW／CR3 的 ftyp crx）；
/// - 再让 rawler 的解码器查找认一次——普通 TIFF 位图（截图等）魔数相同但
///   解码器不认 ⇒ 不命中，不许把位图误判成 RAW 存成 rawsrc（存了读不出
///   比当场拒绝更糟，NF2）。
///
/// 命中后顺带给出机型概要（上传端 `decoded_from` 的原料）：元数据取不到
/// 时 make/model 给 `None`，识别本身不失败（设计 5.7：像素才是主语）。
/// 不命中 ⇒ `None`（调用方按「不是 RAW」走既有路径，不报错）。
pub fn probe_raw(bytes: &[u8]) -> Option<crate::frame::RawProbe> {
    if !has_raw_magic(bytes) {
        return None;
    }
    let source = RawSource::new_from_slice(bytes);
    let decoder = rawler::get_decoder(&source).ok()?;
    let (make, model) = match decoder.raw_metadata(&source, &RawDecodeParams::default()) {
        Ok(metadata) => {
            let make = if metadata.make.is_empty() {
                None
            } else {
                Some(metadata.make.clone())
            };
            let model = if metadata.model.is_empty() {
                None
            } else {
                Some(metadata.model.clone())
            };
            (make, model)
        }
        Err(_) => (None, None),
    };
    Some(crate::frame::RawProbe {
        source_format: sniff_format(bytes),
        make,
        model,
    })
}

/// 组装 `raw_meta` 回显（设计 5.7）：原料是同一次 `decode_bytes` 的产物，
/// 回显与 metadata 落账共用这一份，禁止调用方再解析一遍字节。
pub fn raw_meta(decoded: &DecodedRaw) -> crate::frame::RawMeta {
    let meta = &decoded.file_meta;
    let frame = &decoded.frame;
    crate::frame::RawMeta {
        make: meta.make.clone(),
        model: meta.model.clone(),
        lens_make: meta.lens_exif.as_ref().and_then(|lens| lens.make.clone()),
        lens_model: meta.lens_exif.as_ref().and_then(|lens| lens.model.clone()),
        lens: meta.lens.clone(),
        iso: meta.iso,
        exposure_time_s: meta.exposure_time,
        fnumber: meta.fnumber,
        focal_mm: meta.focal_mm,
        focus_distance_m: meta
            .lens_exif
            .as_ref()
            .and_then(|lens| lens.focus_distance_m),
        date_time_original: meta.date_time_original.clone(),
        orientation: meta.orientation,
        cfa: frame.cfa.name().to_string(),
        bit_depth: meta.bit_depth,
        black_levels: frame.black_levels.clone(),
        white_levels: frame.white_levels.clone(),
        wb_multipliers: frame.wb_multipliers,
        width: frame.width as u32,
        height: frame.height as u32,
        source_format: meta.source_format.clone(),
    }
}

/// 由解码产物组装留痕上下文（render 挂接点与集成测试共用）。
pub fn record_context(decoded: &DecodedRaw) -> RecordContext {
    RecordContext {
        source_format: decoded.file_meta.source_format.clone(),
        camera_make: decoded.file_meta.make.clone(),
        camera_model: decoded.file_meta.model.clone(),
        lens: decoded.file_meta.lens.clone(),
        lens_exif: decoded.file_meta.lens_exif.clone(),
        decoder_name: DECODER_NAME.to_string(),
        decoder_version: DECODER_VERSION.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_version_matches_lockfile() {
        // DECODER_VERSION 是手写常量：与 Cargo.lock 里 rawler 的实际锁定版本
        // 对照，依赖升级忘了同步就红（留痕记录里的解码器版本不许说谎）。
        let manifest = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
        )
        .expect("workspace Cargo.lock 可读");
        let needle = "name = \"rawler\"";
        let pos = manifest.find(needle).expect("Cargo.lock 里有 rawler");
        let tail = &manifest[pos..];
        let version_line = tail
            .lines()
            .find(|l| l.starts_with("version = "))
            .expect("rawler 条目有 version 行");
        assert!(
            version_line.contains(DECODER_VERSION),
            "DECODER_VERSION={DECODER_VERSION} 与 Cargo.lock 的 {version_line} 不一致"
        );
    }

    #[test]
    fn resolve_orientation_prefers_valid_exif_over_decoder_field() {
        // 期 1 B 项钉子：EXIF 合法值压过解码器字段——变异成「只用
        // raw.orientation」时本条即红（ojigi RW2 的实测形态：
        // 解码器写死 Normal、EXIF 是 8）。
        assert_eq!(resolve_orientation(Orientation::Normal, Some(8)), 8);
        assert_eq!(resolve_orientation(Orientation::Rotate90, Some(8)), 8);
        assert_eq!(resolve_orientation(Orientation::Rotate270, Some(3)), 3);
        assert_eq!(resolve_orientation(Orientation::Normal, Some(1)), 1);
        // EXIF 缺失 ⇒ 回退解码器字段（DNG 回填路靠这条）。
        assert_eq!(resolve_orientation(Orientation::Rotate90, None), 6);
        assert_eq!(resolve_orientation(Orientation::Rotate270, None), 8);
        // EXIF 非法值（0／9）不许采用，回退字段；字段 Unknown 兜底 1。
        assert_eq!(resolve_orientation(Orientation::Normal, Some(0)), 1);
        assert_eq!(resolve_orientation(Orientation::Rotate90, Some(9)), 6);
        assert_eq!(resolve_orientation(Orientation::Unknown, None), 1);
        assert_eq!(resolve_orientation(Orientation::Unknown, Some(5)), 5);
    }

    #[test]
    fn raw_magic_covers_rw2_and_orf_headers() {
        // Panasonic RW2：TIFF 魔数变体 II\x55\x00（版本字 0x55 而非 0x2a）。
        assert!(has_raw_magic(b"II\x55\x00\x08\x00\x00\x00rest"));
        // Olympus ORF：IIRO 签名（版本字 'RO'）。
        assert!(has_raw_magic(b"IIRO\x08\x00\x00\x00rest"));
        // 既有签名不许回退。
        assert!(has_raw_magic(b"II\x2a\x00rest"));
        assert!(has_raw_magic(b"MM\x00\x2arest"));
        assert!(has_raw_magic(b"FUJIFILMCCD-RAW rest"));
        let mut cr3 = b"\x00\x00\x00\x18".to_vec();
        cr3.extend_from_slice(b"ftypcrx ");
        assert!(has_raw_magic(&cr3));
        // 非 RAW 容器不许命中。
        assert!(!has_raw_magic(b"\x89PNG\r\n\x1a\nrest"));
        assert!(!has_raw_magic(b"\xff\xd8\xff\xe0jpeg"));
        assert!(!has_raw_magic(b""));
        assert!(!has_raw_magic(b"II"));
        // sniff 与探针同口径：新签名给出真格式名，不许笼统落 "raw"。
        assert_eq!(sniff_format(b"II\x55\x00rest"), "rw2");
        assert_eq!(sniff_format(b"IIROrest"), "orf");
    }

    #[test]
    fn phase_remap_follows_crop_origin_parity() {
        // 全幅相位 [10, 20, 30, 40]，裁剪原点 (1, 0)：裁后相位 0 处对应
        // 全幅 (1, 0) → 相位 1 → 20；整表应左旋一位行内交换。
        // （remap_levels 的 N=2 特例，与旧 remap_phases 逐位同口径。）
        assert_eq!(
            remap_levels(&[10.0, 20.0, 30.0, 40.0], 2, 2, 2, 1, 0).unwrap(),
            vec![20.0, 10.0, 40.0, 30.0]
        );
        assert_eq!(
            remap_levels(&[10.0, 20.0, 30.0, 40.0], 2, 2, 2, 0, 1).unwrap(),
            vec![30.0, 40.0, 10.0, 20.0]
        );
        assert_eq!(
            remap_levels(&[10.0, 20.0, 30.0, 40.0], 2, 2, 2, 1, 1).unwrap(),
            vec![40.0, 30.0, 20.0, 10.0]
        );
        assert_eq!(
            remap_levels(&[10.0, 20.0, 30.0, 40.0], 2, 2, 2, 0, 0).unwrap(),
            vec![10.0, 20.0, 30.0, 40.0]
        );
    }

    #[test]
    fn level_remap_generalizes_to_xtrans_period() {
        // 6×6 全幅电平 = 相位下标本身；裁剪原点 (1, 2) 时裁后相位 (x, y)
        // 须取全幅相位 ((1+x)%6, (2+y)%6) 的值——逐相位恒等式核对。
        let full: Vec<f32> = (0..36).map(|p| p as f32).collect();
        let out = remap_levels(&full, 6, 6, 6, 1, 2).unwrap();
        for y in 0..6 {
            for x in 0..6 {
                assert_eq!(out[y * 6 + x], full[((2 + y) % 6) * 6 + ((1 + x) % 6)]);
            }
        }
        // 1×1 源 ⇒ 全相位广播；形状对不齐 ⇒ 响亮报错，不许猜。
        assert_eq!(remap_levels(&[7.0], 1, 1, 6, 3, 4).unwrap(), vec![7.0; 36]);
        assert!(remap_levels(&[1.0, 2.0, 3.0, 4.0], 2, 2, 6, 0, 0).is_err());
        assert!(remap_levels(&full, 6, 6, 2, 0, 0).is_err());
    }
}
