//! 合成 DNG 夹具（设计 6.1）：手写最小合法 TIFF/DNG（单 IFD、小端、单 strip、
//! 无压缩），让解码冒烟与集成测试不依赖任何真机文件。
//!
//! 本模块只经 `test-support` feature 暴露：生产构建不带写 DNG 的代码。
//! 标签号逐个对过 rawler 0.7.2 的 `tags.rs`（DngTag）与 DNG 1.4 规范，
//! 不是凭记忆写的：DNGVersion=50706、BlackLevel=50714、WhiteLevel=50717、
//! ColorMatrix1/2=50721/50722、AsShotNeutral=50728、BaselineExposure=50730、
//! CalibrationIlluminant1/2=50778/50779、ActiveArea=50829。

/// 合成 DNG 的规格：马赛克图案由调用方以「解析可算」的方式生成
///（常量场／渐变／棋盘／冲激），断言才有解析期望可对照。
pub struct SyntheticDng {
    pub width: u32,
    pub height: u32,
    /// 全幅马赛克（u16，行主序，len = width × height）。
    pub mosaic: Vec<u16>,
    /// CFAPattern 四字节（CFAPlaneColor 下标：0=R、1=G、2=B），如 RGGB = [0,1,1,2]。
    pub cfa_pattern: [u8; 4],
    /// 全尺寸 CFAPattern（ISSUE-3 补）：`Some((字节, 周期))` 时覆盖
    /// `cfa_pattern` 与两个 RepeatDim 标签（如 X-Trans 36 字节／周期 6）；
    /// `None`（缺省）时写出与旧夹具逐字节一致。
    pub cfa_pattern_full: Option<(Vec<u8>, u16)>,
    /// 全尺寸 BlackLevel（与 `cfa_pattern_full` 配套）：`Some` 时覆盖
    /// 四相位 `black_levels`，元数须 = 周期²。
    pub black_levels_full: Option<Vec<(u32, u32)>>,
    /// BlackLevel 四相位（行主序），写为 RATIONAL。锚定口径按 rawler 的
    /// 消费方式：锚在**全幅数据原点**（其 `correct_blacklevel_cfa` 即如此
    /// 作用于未裁数据，且 get_blacklevels 不做 ActiveArea 平移——与
    /// CFAPattern 的 active-area 相对口径不同，冒烟测试把这条钉死）。
    pub black_levels: [(u32, u32); 4],
    pub white_level: u32,
    /// (矩阵, 标定光源 EXIF 值)：ColorMatrix1 + CalibrationIlluminant1。
    pub color_matrix1: Option<([[f32; 3]; 3], u16)>,
    pub color_matrix2: Option<([[f32; 3]; 3], u16)>,
    /// AsShotNeutral 三元组（RATIONAL 写入，分母 100000）。
    pub as_shot_neutral: [f32; 3],
    /// ActiveArea (top, left, bottom, right)——DNG 的原生顺序。
    pub active_area: Option<[u32; 4]>,
    /// BaselineExposure（SRational）：夹具故意写上它——rawler 0.7.2 读不到，
    /// 冒烟测试断言「写了也读不到」以钉住这个缺口（解码器升级暴露它时测试转红，
    /// 强制回头更新缺口清单，这正是断言该有的牙齿）。
    pub baseline_exposure: Option<(i32, u32)>,
    pub make: String,
    pub model: String,
    /// EXIF 段（第二包补）：缺省 `None` = 与第一包夹具逐字节一致（无 EXIF IFD、
    /// 无 Orientation 标签）；给值时写出 IFD0 的 Orientation（若有）与
    /// ExifIFD 指针（34665）+ EXIF IFD，供 raw_meta／方向判据逐项对账。
    pub exif: Option<SyntheticExif>,
}

/// 合成 EXIF 内容：字段全部可选，只写给出的（EXIF 稀疏是常态，夹具要能
/// 造「只给了两项」的稀疏文件）。标签号对过 rawler 0.7.2 的 `tags.rs`。
#[derive(Default)]
pub struct SyntheticExif {
    /// IFD0 的 Orientation（274，SHORT）。
    pub orientation: Option<u16>,
    /// ExposureTime（33434，RATIONAL，如 (1, 125)）。
    pub exposure_time: Option<(u32, u32)>,
    /// FNumber（33437，RATIONAL，如 (28, 10) = f/2.8）。
    pub fnumber: Option<(u32, u32)>,
    /// ISOSpeedRatings（34855，SHORT）。
    pub iso: Option<u16>,
    /// DateTimeOriginal（36867，ASCII 原文）。
    pub date_time_original: Option<String>,
    /// SubjectDistance（37382，RATIONAL，米）。
    pub subject_distance: Option<(u32, u32)>,
    /// FocalLength（37386，RATIONAL，mm）。
    pub focal_length: Option<(u32, u32)>,
    /// LensMake（42035，ASCII）。
    pub lens_make: Option<String>,
    /// LensModel（42036，ASCII）。
    pub lens_model: Option<String>,
}

impl Default for SyntheticDng {
    fn default() -> Self {
        Self {
            width: 8,
            height: 8,
            mosaic: vec![1024; 64],
            cfa_pattern: [0, 1, 1, 2],
            cfa_pattern_full: None,
            black_levels_full: None,
            black_levels: [(512, 1), (512, 1), (512, 1), (512, 1)],
            white_level: 16383,
            color_matrix1: Some((
                [
                    [0.7355, -0.1828, -0.0836],
                    [-0.4262, 1.2845, 0.2576],
                    [-0.0698, 0.1123, 0.7846],
                ],
                21, // D65
            )),
            color_matrix2: Some((
                [
                    [0.5844, -0.0506, -0.1067],
                    [-0.6724, 1.5265, 0.2291],
                    [-0.0837, 0.1436, 0.6759],
                ],
                17, // A
            )),
            as_shot_neutral: [0.58, 1.0, 0.72],
            active_area: None,
            baseline_exposure: None,
            make: "YanshiFixture".to_string(),
            model: "Synthetic One".to_string(),
            exif: None,
        }
    }
}

/// TIFF 字段值（只实现夹具用到的类型）。
enum Value {
    Bytes(Vec<u8>),
    Ascii(String),
    Shorts(Vec<u16>),
    Longs(Vec<u32>),
    Rationals(Vec<(u32, u32)>),
    SRationals(Vec<(i32, u32)>),
}

impl Value {
    fn type_code(&self) -> u16 {
        match self {
            Value::Bytes(_) => 1,
            Value::Ascii(_) => 2,
            Value::Shorts(_) => 3,
            Value::Longs(_) => 4,
            Value::Rationals(_) => 5,
            Value::SRationals(_) => 10,
        }
    }

    fn count(&self) -> u32 {
        match self {
            Value::Bytes(v) => v.len() as u32,
            Value::Ascii(s) => s.len() as u32 + 1, // 含 NUL 结尾
            Value::Shorts(v) => v.len() as u32,
            Value::Longs(v) => v.len() as u32,
            Value::Rationals(v) => v.len() as u32,
            Value::SRationals(v) => v.len() as u32,
        }
    }

    fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Value::Bytes(v) => out.extend_from_slice(v),
            Value::Ascii(s) => {
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
            Value::Shorts(v) => {
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            Value::Longs(v) => {
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            Value::Rationals(v) => {
                for (num, den) in v {
                    out.extend_from_slice(&num.to_le_bytes());
                    out.extend_from_slice(&den.to_le_bytes());
                }
            }
            Value::SRationals(v) => {
                for (num, den) in v {
                    out.extend_from_slice(&num.to_le_bytes());
                    out.extend_from_slice(&den.to_le_bytes());
                }
            }
        }
        out
    }
}

/// f32 → SRational（分母 100000，冒烟精度足够；矩阵只要求可逆且量纲现实）。
fn srational(v: f32) -> (i32, u32) {
    (libm::roundf(v * 100000.0) as i32, 100000)
}

fn rational(v: f32) -> (u32, u32) {
    (libm::roundf(v * 100000.0) as u32, 100000)
}

/// CFA／BlackLevel 的 RepeatDim：全尺寸规格给出周期时用它，否则 2×2。
fn cfa_repeat_dim(spec: &SyntheticDng) -> Vec<u16> {
    match &spec.cfa_pattern_full {
        Some((_, period)) => vec![*period, *period],
        None => vec![2, 2],
    }
}

/// CFAPattern 字节：全尺寸规格优先，否则四字节 Bayer 规格。
fn cfa_pattern_bytes(spec: &SyntheticDng) -> Vec<u8> {
    match &spec.cfa_pattern_full {
        Some((bytes, _)) => bytes.clone(),
        None => spec.cfa_pattern.to_vec(),
    }
}

/// BlackLevel 有理数列：全尺寸规格优先，否则四相位规格。
fn black_levels_of(spec: &SyntheticDng) -> Vec<(u32, u32)> {
    match &spec.black_levels_full {
        Some(levels) => levels.clone(),
        None => spec.black_levels.to_vec(),
    }
}

/// 按规格写出 DNG 字节。布局：头(8) → IFD → 溢出值区 → strip 像素区；
/// 条目按标签升序（TIFF 规范要求，解码器实现多半也默认）。
pub fn write_synthetic_dng(spec: &SyntheticDng) -> Vec<u8> {
    assert_eq!(
        spec.mosaic.len(),
        spec.width as usize * spec.height as usize,
        "夹具马赛克尺寸与规格对不上"
    );
    let strip_bytes = (spec.mosaic.len() * 2) as u32;
    let mut entries: Vec<(u16, Value)> = vec![
        (256, Value::Longs(vec![spec.width])),   // ImageWidth
        (257, Value::Longs(vec![spec.height])),  // ImageLength
        (258, Value::Shorts(vec![16])),          // BitsPerSample
        (259, Value::Shorts(vec![1])),           // Compression = 无
        (262, Value::Shorts(vec![32803])),       // Photometric = CFA
        (271, Value::Ascii(spec.make.clone())),  // Make
        (272, Value::Ascii(spec.model.clone())), // Model
        // StripOffsets 占位，最后回填（条目 273）。
        (273, Value::Longs(vec![0])),
        (277, Value::Shorts(vec![1])),                // SamplesPerPixel
        (278, Value::Longs(vec![spec.height])),       // RowsPerStrip
        (279, Value::Longs(vec![strip_bytes])),       // StripByteCounts
        (284, Value::Shorts(vec![1])),                // PlanarConfiguration
        (33421, Value::Shorts(cfa_repeat_dim(spec))), // CFARepeatPatternDim
        (33422, Value::Bytes(cfa_pattern_bytes(spec))), // CFAPattern
        (50706, Value::Bytes(vec![1, 4, 0, 0])),      // DNGVersion
        (50707, Value::Bytes(vec![1, 0, 0, 0])),      // DNGBackwardVersion
        (50708, Value::Ascii(spec.model.clone())),    // UniqueCameraModel
        (50710, Value::Bytes(vec![0, 1, 2])),         // CFAPlaneColor
        (50711, Value::Shorts(vec![1])),              // CFALayout
        (50713, Value::Shorts(cfa_repeat_dim(spec))), // BlackLevelRepeatDim
        (50714, Value::Rationals(black_levels_of(spec))), // BlackLevel
        (50717, Value::Longs(vec![spec.white_level])), // WhiteLevel
        (
            50728, // AsShotNeutral
            Value::Rationals(spec.as_shot_neutral.iter().map(|&v| rational(v)).collect()),
        ),
    ];
    if let Some((matrix, illuminant)) = &spec.color_matrix1 {
        entries.push((50778, Value::Shorts(vec![*illuminant]))); // CalibrationIlluminant1
        entries.push((
            50721, // ColorMatrix1
            Value::SRationals(matrix.iter().flatten().map(|&v| srational(v)).collect()),
        ));
    }
    if let Some((matrix, illuminant)) = &spec.color_matrix2 {
        entries.push((50779, Value::Shorts(vec![*illuminant]))); // CalibrationIlluminant2
        entries.push((
            50722, // ColorMatrix2
            Value::SRationals(matrix.iter().flatten().map(|&v| srational(v)).collect()),
        ));
    }
    if let Some(area) = &spec.active_area {
        entries.push((50829, Value::Longs(area.to_vec()))); // ActiveArea
    }
    if let Some((num, den)) = &spec.baseline_exposure {
        entries.push((50730, Value::SRationals(vec![(*num, *den)]))); // BaselineExposure
    }
    // EXIF（第二包补）：Orientation 进 IFD0；其余进 EXIF IFD（经 34665 指针）。
    let mut exif_entries: Vec<(u16, Value)> = Vec::new();
    if let Some(exif) = &spec.exif {
        if let Some(orientation) = exif.orientation {
            entries.push((274, Value::Shorts(vec![orientation]))); // Orientation
        }
        if let Some(value) = exif.exposure_time {
            exif_entries.push((33434, Value::Rationals(vec![value]))); // ExposureTime
        }
        if let Some(value) = exif.fnumber {
            exif_entries.push((33437, Value::Rationals(vec![value]))); // FNumber
        }
        if let Some(iso) = exif.iso {
            exif_entries.push((34855, Value::Shorts(vec![iso]))); // ISOSpeedRatings
        }
        if let Some(text) = &exif.date_time_original {
            exif_entries.push((36867, Value::Ascii(text.clone()))); // DateTimeOriginal
        }
        if let Some(value) = exif.subject_distance {
            exif_entries.push((37382, Value::Rationals(vec![value]))); // SubjectDistance
        }
        if let Some(value) = exif.focal_length {
            exif_entries.push((37386, Value::Rationals(vec![value]))); // FocalLength
        }
        if let Some(text) = &exif.lens_make {
            exif_entries.push((42035, Value::Ascii(text.clone()))); // LensMake
        }
        if let Some(text) = &exif.lens_model {
            exif_entries.push((42036, Value::Ascii(text.clone()))); // LensModel
        }
    }
    let has_exif_ifd = !exif_entries.is_empty();
    entries.sort_by_key(|(tag, _)| *tag);
    exif_entries.sort_by_key(|(tag, _)| *tag);

    // 布局：头(8) → IFD0 → EXIF IFD（若有）→ 溢出值区 → strip。
    // ExifIFD 指针（34665）的值就是 EXIF IFD 的绝对偏移，布局定下后回填，
    // 因此先按「含指针条目」的条数算 IFD0 尺寸，再把指针条目插进去。
    let ifd0_count = entries.len() + usize::from(has_exif_ifd);
    let ifd0_bytes = 2 + ifd0_count as u32 * 12 + 4;
    let exif_ifd_offset = 8 + ifd0_bytes;
    if has_exif_ifd {
        entries.push((34665, Value::Longs(vec![exif_ifd_offset]))); // ExifIFD 指针
        entries.sort_by_key(|(tag, _)| *tag);
    }
    let exif_bytes = if has_exif_ifd {
        2 + exif_entries.len() as u32 * 12 + 4
    } else {
        0
    };
    let mut serialized: Vec<(u16, Value, Vec<u8>)> = Vec::new();
    for (tag, value) in entries.into_iter().chain(exif_entries) {
        let bytes = value.bytes();
        serialized.push((tag, value, bytes));
    }
    let mut cursor = exif_ifd_offset + exif_bytes;
    let mut value_offsets: Vec<u32> = Vec::with_capacity(serialized.len());
    for (_, _, bytes) in &serialized {
        if bytes.len() > 4 {
            value_offsets.push(cursor);
            cursor += bytes.len() as u32;
            if cursor % 2 == 1 {
                cursor += 1; // TIFF 值区按字对齐
            }
        } else {
            value_offsets.push(0);
        }
    }
    let strip_offset = cursor;

    // 逐 IFD 序列化：条目在 serialized 里是 [IFD0…, EXIF…] 顺序拼接。
    let write_ifd = |out: &mut Vec<u8>, range: std::ops::Range<usize>| {
        out.extend_from_slice(&(range.len() as u16).to_le_bytes());
        for i in range {
            let (tag, value, bytes) = &serialized[i];
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&value.type_code().to_le_bytes());
            out.extend_from_slice(&value.count().to_le_bytes());
            if *tag == 273 {
                out.extend_from_slice(&strip_offset.to_le_bytes());
            } else if bytes.len() <= 4 {
                let mut inline = [0u8; 4];
                inline[..bytes.len()].copy_from_slice(bytes);
                out.extend_from_slice(&inline);
            } else {
                out.extend_from_slice(&value_offsets[i].to_le_bytes());
            }
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // 无下一 IFD
    };
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"II");
    out.extend_from_slice(&42u16.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 紧随文件头
    write_ifd(&mut out, 0..ifd0_count);
    if has_exif_ifd {
        let exif_count = serialized.len() - ifd0_count;
        write_ifd(&mut out, ifd0_count..ifd0_count + exif_count);
    }
    for (i, (_, _, bytes)) in serialized.iter().enumerate() {
        if bytes.len() > 4 {
            // 对齐到先前算好的偏移（前面可能有对齐填充）。
            while (out.len() as u32) < value_offsets[i] {
                out.push(0);
            }
            out.extend_from_slice(bytes);
        }
    }
    while (out.len() as u32) < strip_offset {
        out.push(0);
    }
    for sample in &spec.mosaic {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}
