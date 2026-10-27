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
//!   回退），frame 与 file_meta 同源同值。期 2 Y-02：MakerNote 厂商
//!   方向（Sony 0x9400／Pentax ShotInfo）与 EXIF 交叉校验，冲突只进
//!   `notes` 警告、不改取值（胜负未定，见 `orientation_conflict_note`）。
//!   ⚠️ 迁移注记：导入期即展开
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
    // Y-02 交叉校验（期 2）：MakerNote 厂商方向与 EXIF 对照。
    // 读不到/不识别 ⇒ None（无校验，与今日一致）；失败不阻断解码。
    let model = metadata.as_ref().map(|m| m.model.as_str()).unwrap_or("");
    let makernote_orientation = read_makernote_orientation(decoder.as_ref(), bytes, model);
    let mut decoded = map_decoded(
        raw,
        metadata.as_ref(),
        &source_format,
        makernote_orientation,
    )?;
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
    makernote_orientation: Option<u16>,
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
    // Y-02：MakerNote 冲突只警告、不改取值（胜负未定，见函数注）。
    if let Some(note) =
        orientation_conflict_note(exif_orientation, makernote_orientation, orientation)
    {
        notes.push(note);
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

/// MakerNote 厂商方向读取（期 2 Y-02）：经解码器公开的 `ifd()` 接口取
/// EXIF IFD，再用 rawler 自家的 `parse_makernote`（ARW／PEF 解码器内部
/// 同款调用，`OffsetMode::Absolute`）解析厂商块。`bytes` 是完整文件字节
/// （解析器要在文件绝对偏移上取外联值，`Cursor` 复用它）。
/// 读不到 EXIF IFD／无 MakerNote／块过短／解析失败 ⇒ `None`（无校验，
/// 与今日一致）；失败只丢校验，不丢解码。`model` 只用于 Pentax 表选择
/// 与 ILCE-7M5/7RM6 的偏移修正，Sony 表选择看块首字节、自识别。
fn read_makernote_orientation(
    decoder: &dyn rawler::decoders::Decoder,
    bytes: &[u8],
    model: &str,
) -> Option<u16> {
    use rawler::decoders::WellKnownIFD;
    use rawler::formats::tiff::ifd::OffsetMode;
    use rawler::formats::tiff::Value;
    use rawler::tags::ExifTag;
    let exif_ifd = decoder.ifd(WellKnownIFD::Exif).ok()??;
    // 厂商块过短（<16 字节）不可能是真 MakerNote，且 rawler 的
    // parse_makernote 会对短数据越界取片——先拦住。
    let entry = exif_ifd.get_entry(ExifTag::MakerNotes)?;
    let mn_len = match &entry.value {
        Value::Undefined(data) | Value::Byte(data) => data.len(),
        _ => return None,
    };
    if mn_len < 16 {
        return None;
    }
    let mn_ifd = exif_ifd
        .parse_makernote(&mut std::io::Cursor::new(bytes), OffsetMode::Absolute, &[])
        .ok()??;
    makernote_orientation_from_ifd(&mn_ifd, model)
}

/// 已解析的 MakerNote IFD → 厂商方向（EXIF 式 1/3/6/8），纯函数。
/// Sony 优先（0x9400 加密块），其次 Pentax ShotInfo 0x0226、LevelInfoK3III
/// 0x022b；三者皆无或值非法 ⇒ `None`。
fn makernote_orientation_from_ifd(mn_ifd: &rawler::formats::tiff::IFD, model: &str) -> Option<u16> {
    if let Some(blob) = undefined_bytes(mn_ifd, 0x9400) {
        if let Some(v) = sony_camera_orientation(blob, model) {
            return Some(v);
        }
    }
    if is_pentax_shotinfo_model(model) {
        if let Some(block) = undefined_bytes(mn_ifd, 0x0226) {
            if let Some(v) = pentax_shotinfo_orientation(block) {
                return Some(v);
            }
        }
    }
    if model.contains("K-3 Mark III") {
        if let Some(block) = undefined_bytes(mn_ifd, 0x022b) {
            if let Some(v) = pentax_levelinfo_orientation(block) {
                return Some(v);
            }
        }
    }
    None
}

/// MakerNote IFD 里取某标签的原始字节（UNDEFINED／BYTE 型）；类型不符 ⇒ `None`。
fn undefined_bytes(mn_ifd: &rawler::formats::tiff::IFD, tag: u16) -> Option<&[u8]> {
    match mn_ifd.entries.get(&tag)?.value {
        rawler::formats::tiff::Value::Undefined(ref bytes)
        | rawler::formats::tiff::Value::Byte(ref bytes) => Some(bytes),
        _ => None,
    }
}

/// Sony `CameraOrientation`（ExifTool Sony.pm Tag9400a/b/c）：MakerNote
/// 标签 0x9400 是一段加密二进制块，表选择看**加密后首字节**——0x07／0x09／
/// 0x0a → 9400a（偏移 0x0028）、0x0c → 9400b（0x0024）、0x23／0x24／0x26／
/// 0x28／0x31／0x32／0x33／0x41／0x43 → 9400c（0x0029）；三表值均为 EXIF
/// 式 1／3／6／8。首字节 0x5e／0xe7／0x04 是双重加密（ExifTool 9.04–9.10
/// 的写 bug），需解两次、按 9400a 读。ILCE-7M5／7RM6 在 0x001e 之后的偏移
/// −1（Tag9400c 0x001e Hook 注明），`CameraOrientation` 落在 0x0028。
/// 老 DSLR 表 Tag9400 的 0x0018（掩码 0x30）／NEX 初代 0x0016（掩码 0xc0）
/// 在现行 ExifTool 里未接线，不做。首字节未识别／块太短／值不在 1／3／6／8
/// ⇒ `None`。
/// 实证（2026-10-25，`p2/probe_9400.py`）：ILCE-6000×2 首字节 0x23、
/// 解密[0x0029]=1；ILCE-7RM3 首字节 0x26、解密[0x0029]=8，均与 IFD0 一致。
fn sony_camera_orientation(blob: &[u8], model: &str) -> Option<u16> {
    let (offset, double) = match blob.first() {
        Some(0x07) | Some(0x09) | Some(0x0a) => (0x0028, false),
        Some(0x0c) => (0x0024, false),
        Some(0x23) | Some(0x24) | Some(0x26) | Some(0x28) | Some(0x31) | Some(0x32)
        | Some(0x33) | Some(0x41) | Some(0x43) => (0x0029, false),
        Some(0x5e) | Some(0xe7) | Some(0x04) => (0x0028, true),
        _ => return None,
    };
    let dec = sony_decipher(blob);
    let dec = if double { sony_decipher(&dec) } else { dec };
    let mut offset = offset;
    if offset == 0x0029 && (model.starts_with("ILCE-7M5") || model.starts_with("ILCE-7RM6")) {
        offset = 0x0028;
    }
    let v = *dec.get(offset)?;
    if matches!(v, 1 | 3 | 6 | 8) {
        Some(u16::from(v))
    } else {
        None
    }
}

/// Sony 0x9400 块解密（ExifTool Sony.pm `Decipher`：置换密码 c = b³ % 249）。
/// 字节 2..=247 按逆表还原，其余直通；立方 mod 249 在 0..=248 上是双射，
/// 逆表唯一（249–255 从不参与加密，直通即是原值）。
fn sony_decipher(data: &[u8]) -> Vec<u8> {
    let mut inv = [0u8; 256];
    for b in 0..=248u32 {
        inv[((b * b * b) % 249) as usize] = b as u8;
    }
    data.iter()
        .map(|&c| {
            if (2..=247).contains(&c) {
                inv[c as usize]
            } else {
                c
            }
        })
        .collect()
}

/// Pentax 机型门：ShotInfo 表只对 K-5／K-7／K-r／K-x 有效（ExifTool
/// 条件 `Model =~ /K-(5|7|r|x)\b/`）。按空白切词精确匹配，`K-50` 这类
/// 不命中。
fn is_pentax_shotinfo_model(model: &str) -> bool {
    model
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .any(|t| matches!(t, "K-5" | "K-7" | "K-r" | "K-x"))
}

/// Pentax ShotInfo（MakerNote 标签 0x0226，ExifTool Pentax.pm）：字节 1
/// 是方向传感器读数——0x10 横／0x20 转 180／0x30 转 90 CW／0x40 转 270 CW；
/// 0x50／0x60（朝上／朝下）无 EXIF 等价 ⇒ 无信号。块短于 2 字节 ⇒ `None`。
fn pentax_shotinfo_orientation(block: &[u8]) -> Option<u16> {
    match block.get(1) {
        Some(0x10) => Some(1),
        Some(0x20) => Some(3),
        Some(0x30) => Some(6),
        Some(0x40) => Some(8),
        _ => None,
    }
}

/// Pentax LevelInfoK3III（MakerNote 标签 0x022b，仅 K-3 Mark III）：
/// 字节 1：0 横／1 转 270 CW／2 转 180／3 转 90 CW；4／5（朝上／朝下）
/// 无 EXIF 等价 ⇒ 无信号。
fn pentax_levelinfo_orientation(block: &[u8]) -> Option<u16> {
    match block.get(1) {
        Some(0) => Some(1),
        Some(1) => Some(8),
        Some(2) => Some(3),
        Some(3) => Some(6),
        _ => None,
    }
}

/// 方向冲突警告（期 2 Y-02，纯函数）。**只警告、不改取值**：IFD0 是标准
/// 字段，编辑器旋转只改 IFD0 不改 MakerNote，单一样本（原文件未随附、
/// 无法独立验真）不足以翻转胜负——合并计划 D-3 批的也是「交叉校验 +
/// 冲突进 warnings」。两路都合法且不同 ⇒ 冲突警告；EXIF 缺失／非法且
/// MakerNote 与回退值不同 ⇒ 追加提示（应用值仍是回退值）；一致或
/// MakerNote 无信号 ⇒ `None`。
fn orientation_conflict_note(
    exif: Option<u16>,
    maker: Option<u16>,
    orientation: u32,
) -> Option<String> {
    let maker = maker?;
    match exif.filter(|v| (1..=8).contains(v)) {
        Some(exif) if u32::from(exif) != u32::from(maker) => Some(format!(
            "方向元数据冲突：EXIF Orientation 为 {exif}，MakerNote CameraOrientation 为 {maker}；已按 EXIF 取 {orientation}，请人工确认"
        )),
        None if u32::from(maker) != orientation => Some(format!(
            "EXIF Orientation 缺失或非法，方向按解码器字段取 {orientation}；MakerNote CameraOrientation 为 {maker}，与回退值不一致，请人工确认"
        )),
        _ => None,
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
    if bytes.len() >= 4 && &bytes[0..4] == b"IIRS" {
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
/// `IIRO` 与 `IIRS`——两者都不是 TIFF 42（`II\x2a\x00`），rawler 本可解码
/// （2026-10-24 展示册选材实测：两格式真文件在此被拒、展开链无此门）。
/// `IIRS` 是老款 Olympus 机身的 ORF 签名（C-5060WZ 等，2026-10-27 探针：
/// olyC5060 的 `IIRS` 在此被拒、E-3 的 `IIRO` 能过；rawler 的 TIFF 读数
/// 不校验魔数值、按 Make 标签分发 OrfDecoder，两签名下游同一条路）。
fn has_raw_magic(bytes: &[u8]) -> bool {
    (bytes.len() >= 4
        && (&bytes[0..4] == b"II\x2a\x00"
            || &bytes[0..4] == b"MM\x00\x2a"
            || &bytes[0..4] == b"II\x55\x00"
            || &bytes[0..4] == b"IIRO"
            || &bytes[0..4] == b"IIRS"))
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
        // 期 2 Y-02：MakerNote 层在 orientation_conflict_note——冲突只警告，
        // 不改此处口径（resolve_orientation 本身未动，本条原样成立）。
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
        // 老款 Olympus ORF：IIRS 签名（版本字 'RS'，C-5060WZ 等）。
        // 2026-10-27：olyC5060 的 IIRS 在此被拒、E-3 的 IIRO 能过。
        assert!(has_raw_magic(b"IIRS\x08\x00\x00\x00rest"));
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
        assert_eq!(sniff_format(b"IIRSrest"), "orf");
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

    // ---- 期 2 Y-02：MakerNote 方向交叉校验 ----
    /// 测试用加密：Sony 置换密码的加密方向（c = b³ % 249），与
    /// `sony_decipher` 互逆（实现侧只提供解密，加密只在测试里造块）。
    fn enc(b: u8) -> u8 {
        (u32::from(b).pow(3) % 249) as u8
    }

    /// 造一段加密的 0x9400 块：首字节（加密后）定表，`at` 处放加密后的方向值。
    fn sony_blob(first_byte: u8, at: usize, orientation: u8) -> Vec<u8> {
        let mut blob = vec![0u8; 64];
        blob[0] = first_byte;
        blob[at] = enc(orientation);
        blob
    }

    /// 造一个只含 `tag` → UNDEFINED 块的 MakerNote IFD（测 IFD 层分发）。
    fn mn_ifd_with(tag: u16, blob: Vec<u8>) -> rawler::formats::tiff::IFD {
        use rawler::formats::tiff::{Entry, Value};
        let mut entries = std::collections::BTreeMap::new();
        entries.insert(
            tag,
            Entry {
                tag,
                value: Value::Undefined(blob),
                embedded: Some(0),
            },
        );
        rawler::formats::tiff::IFD {
            offset: 0,
            base: 0,
            corr: 0,
            next_ifd: 0,
            entries,
            endian: Default::default(),
            sub: std::collections::HashMap::new(),
            chain: vec![],
        }
    }

    #[test]
    fn sony_9400c_reads_orientation_at_0x0029() {
        // 实测锚点：ILCE-6000 首字节 0x23、[0x0029]=1；ILCE-7RM3 首字节
        // 0x26、[0x0029]=8（probe_9400.py，三文件均与 IFD0 一致）。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0029, 8), ""),
            Some(8)
        );
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x23, 0x0029, 1), ""),
            Some(1)
        );
        // 偏移错一位就读到 0 ⇒ None：变异「0x0029→0x0028」时本条即红。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0028, 8), ""),
            None
        );
    }

    #[test]
    fn sony_9400a_and_9400b_offsets() {
        // 9400a（首字节 0x0a）：0x0028；9400b（首字节 0x0c）：0x0024。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x0a, 0x0028, 6), ""),
            Some(6)
        );
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x0c, 0x0024, 3), ""),
            Some(3)
        );
        // 反向：9400a 的块在 0x0029 放值也不许读（那是 Quality2 的位置）。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x0a, 0x0029, 8), ""),
            None
        );
    }

    #[test]
    fn sony_9400_rejects_unknown_first_byte_and_invalid_value() {
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x99, 0x0029, 8), ""),
            None
        );
        // 2 是合法 EXIF 值但 Sony 表里没有（只出 1/3/6/8）⇒ 无信号，不许猜。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0029, 2), ""),
            None
        );
        // 块短于偏移 ⇒ None，不许越界。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0029, 8)[..0x20], ""),
            None
        );
    }

    #[test]
    fn sony_double_cipher_deciphers_twice() {
        // 首字节 0x5e = 双重加密的 0x07；方向值也须加两次密。
        let mut blob = vec![0u8; 64];
        blob[0] = 0x5e;
        blob[0x0028] = enc(enc(6));
        assert_eq!(sony_camera_orientation(&blob, ""), Some(6));
        // 对照：单层加密的 0x07 首字节同样走 9400a。变异「双重加密只解一次」
        // 时上条读到 enc(6)=216 ⇒ 不在 1/3/6/8 ⇒ None，即红。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x07, 0x0028, 6), ""),
            Some(6)
        );
    }

    #[test]
    fn sony_7m5_shifts_offset_by_minus_one() {
        // ExifTool Tag9400c 0x001e Hook 注明：ILCE-7M5／7RM6 在 0x001e 之后
        // 的偏移 −1（无实物验证，按表证钉死；若实测不符，以实物为准改表）。
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0028, 6), "ILCE-7M5"),
            Some(6)
        );
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0029, 8), "ILCE-7M5"),
            None
        );
        assert_eq!(
            sony_camera_orientation(&sony_blob(0x24, 0x0029, 8), "ILCE-7RM3"),
            Some(8)
        );
    }

    #[test]
    fn pentax_shotinfo_and_levelinfo_maps() {
        // ShotInfo 0x0226 字节 1：0x10 横／0x30 转 90 CW；0x50 朝上无信号。
        assert_eq!(pentax_shotinfo_orientation(&[0xf0, 0x10]), Some(1));
        assert_eq!(pentax_shotinfo_orientation(&[0xf0, 0x30]), Some(6));
        assert_eq!(pentax_shotinfo_orientation(&[0xf0, 0x50]), None);
        assert_eq!(pentax_shotinfo_orientation(&[0xf0]), None);
        // LevelInfoK3III 0x022b 字节 1：0 横／1 转 270 CW／3 转 90 CW；4 无信号。
        assert_eq!(pentax_levelinfo_orientation(&[0, 0]), Some(1));
        assert_eq!(pentax_levelinfo_orientation(&[0, 1]), Some(8));
        assert_eq!(pentax_levelinfo_orientation(&[0, 3]), Some(6));
        assert_eq!(pentax_levelinfo_orientation(&[0, 4]), None);
    }

    #[test]
    fn pentax_model_gate_is_exact() {
        assert!(is_pentax_shotinfo_model("PENTAX K-5"));
        assert!(is_pentax_shotinfo_model("PENTAX K-7"));
        assert!(is_pentax_shotinfo_model("PENTAX K-r"));
        assert!(!is_pentax_shotinfo_model("PENTAX K-50"));
        assert!(!is_pentax_shotinfo_model("PENTAX K-3 Mark III"));
        assert!(!is_pentax_shotinfo_model(""));
    }

    #[test]
    fn makernote_orientation_from_ifd_dispatches_by_tag() {
        // Sony 0x9400 不看机型门；Pentax 两表看机型门。
        let sony = mn_ifd_with(0x9400, sony_blob(0x24, 0x0029, 8));
        assert_eq!(makernote_orientation_from_ifd(&sony, ""), Some(8));
        let pentax = mn_ifd_with(0x0226, vec![0xf0, 0x30]);
        assert_eq!(
            makernote_orientation_from_ifd(&pentax, "PENTAX K-5"),
            Some(6)
        );
        assert_eq!(makernote_orientation_from_ifd(&pentax, ""), None);
        let k3 = mn_ifd_with(0x022b, vec![0, 1]);
        assert_eq!(
            makernote_orientation_from_ifd(&k3, "PENTAX K-3 Mark III"),
            Some(8)
        );
        assert_eq!(makernote_orientation_from_ifd(&k3, "PENTAX K-5"), None);
        let empty = mn_ifd_with(0x9405, vec![0; 64]);
        assert_eq!(makernote_orientation_from_ifd(&empty, ""), None);
    }

    #[test]
    fn orientation_conflict_note_warns_without_changing_value() {
        // 冲突（EXIF 3 vs MakerNote 8，报告 Y-02 的 EXTA99 形态）：警告带双方值。
        let note = orientation_conflict_note(Some(3), Some(8), 3).expect("冲突须警告");
        assert!(
            note.contains('3') && note.contains('8'),
            "警告须带双方值：{note}"
        );
        assert!(note.contains("EXIF"), "警告须点名 EXIF：{note}");
        // 一致 ⇒ 静默；MakerNote 无信号 ⇒ 静默。
        assert_eq!(orientation_conflict_note(Some(8), Some(8), 8), None);
        assert_eq!(orientation_conflict_note(Some(1), None, 1), None);
        // EXIF 缺失、回退值 1 与 MakerNote 8 不一致 ⇒ 追加提示（值仍是回退值）。
        let note = orientation_conflict_note(None, Some(8), 1).expect("回退不一致须提示");
        assert!(note.contains('8'), "提示须带 MakerNote 值：{note}");
        // 回退值恰与 MakerNote 一致 ⇒ 不必多说。
        assert_eq!(orientation_conflict_note(None, Some(1), 1), None);
        // EXIF 非法值（0）按缺失走回退分支。
        assert!(orientation_conflict_note(Some(0), Some(8), 1).is_some());
    }
}
