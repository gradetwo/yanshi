//! 展开链的数据类型：解码产物帧、展开参数、展开产物与 `raw_develop` 留痕记录。
//!
//! 字段口径以设计 v3 为准：帧（5.2 解码映射）、参数与 metadata（5.6）。
//! 本模块不含任何解码器依赖——解码映射在 `decode.rs`、展开数学在 `develop.rs`。

use serde::{Deserialize, Serialize};

use crate::error::{RawError, Result};

/// 通道标识（相机原生 RGB）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    R,
    G,
    B,
}

impl Channel {
    /// 在 RGB 三元组中的下标。
    pub fn index(self) -> usize {
        match self {
            Channel::R => 0,
            Channel::G => 1,
            Channel::B => 2,
        }
    }
}

/// Bayer 四排列（设计 5.2：一期仅支持这四种，其余 CFA 在解码期明确拒绝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CfaPattern {
    Rggb,
    Bggr,
    Grbg,
    Gbrg,
}

impl CfaPattern {
    /// 2×2 单元四格的通道（行主序：[(0,0), (1,0), (0,1), (1,1)]）。
    pub fn cell(self) -> [Channel; 4] {
        match self {
            CfaPattern::Rggb => [Channel::R, Channel::G, Channel::G, Channel::B],
            CfaPattern::Bggr => [Channel::B, Channel::G, Channel::G, Channel::R],
            CfaPattern::Grbg => [Channel::G, Channel::R, Channel::B, Channel::G],
            CfaPattern::Gbrg => [Channel::G, Channel::B, Channel::R, Channel::G],
        }
    }

    /// 像素 (x, y) 处的采样通道。相位以帧左上角为 (0,0) 锚点（设计 5.2）。
    pub fn channel_at(self, x: usize, y: usize) -> Channel {
        self.cell()[(y % 2) * 2 + (x % 2)]
    }

    /// 像素 (x, y) 的 CFA 相位下标（0..4，行主序，与黑／白电平数组同序）。
    pub fn phase_at(x: usize, y: usize) -> usize {
        (y % 2) * 2 + (x % 2)
    }

    /// 排列名（与 metadata 的 `cfa` 字段同字面）。
    pub fn name(self) -> &'static str {
        match self {
            CfaPattern::Rggb => "RGGB",
            CfaPattern::Bggr => "BGGR",
            CfaPattern::Grbg => "GRBG",
            CfaPattern::Gbrg => "GBRG",
        }
    }

    /// 由 2×2 单元（行主序通道）反认排列；不是 Bayer 四排列之一 ⇒ `None`。
    pub fn from_cell(cell: [Channel; 4]) -> Option<Self> {
        [Self::Rggb, Self::Bggr, Self::Grbg, Self::Gbrg]
            .into_iter()
            .find(|pattern| pattern.cell() == cell)
    }
}

/// 一组双光源标定：某标定光源下的 XYZ → 相机原生矩阵（设计 5.5 色彩链）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibrationMatrix {
    /// 标定光源的相关色温（K）；来源光源无 CCT 定版值时为 `None`
    /// （双矩阵且任一端缺色温时不插值、退单矩阵并在展开注记中注明）。
    pub illuminant_kelvin: Option<f32>,
    /// XYZ → 相机原生 RGB 的 3×3 矩阵（行 = 相机通道 R/G/B）。
    pub xyz_to_cam: [[f32; 3]; 3],
}

/// 解码映射后的展开输入帧（设计 5.2）。像素已裁到 active area，
/// 电平数组已换算到「裁后帧相位」下标（裁剪改变奇偶相位时同步平移）。
#[derive(Debug, Clone)]
pub struct RawFrame {
    /// active area 宽／高（像素）。
    pub width: usize,
    pub height: usize,
    /// 原始马赛克读数（DN 单位，行主序，len = width × height）。
    /// 用 f32 承载：u16 读数可精确表示，个别解码器的浮点读数同路。
    pub mosaic: Vec<f32>,
    /// CFA 排列（相位锚点 = 本帧左上角）。
    pub cfa: CfaPattern,
    /// 黑电平：按 CFA 相位 `[R 位, G1 位, G2 位, B 位]` 的行主序四标量，
    /// **四相位独立、绝不合并 G1/G2**（设计 5.2 硬性要求，实证 V5）。
    pub black_levels: [f32; 4],
    /// 白电平：同相位口径四标量。
    pub white_levels: [f32; 4],
    /// as-shot WB 乘子（文件原始量纲，未重归一化；下标 R/G/B）。
    /// 与 `as_shot_neutral` 至少其一有值（解码期保证）。
    pub wb_multipliers: Option<[f32; 3]>,
    /// AsShotNeutral：相机原生中性坐标 n（下标 R/G/B）。
    pub as_shot_neutral: Option<[f32; 3]>,
    /// 色彩标定矩阵 1–2 组（双光源时按色温升序）。
    pub color_matrices: Vec<CalibrationMatrix>,
    /// DNG `BaselineExposure`（EV）。`None` = 当前解码器版本未暴露该标签
    /// （rawler 0.7.2 实测缺口，见 README），展开时按缺失 = 0 处理并注明。
    pub baseline_exposure_ev: Option<f32>,
}

/// 解马赛克算法（设计 5.4）。`Bilinear` 的身份是**回归基线**，不是质量选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Demosaic {
    /// Malvar–He–Cutler（2004）固定核（α=1/2、β=5/8、γ=3/4），一期默认。
    #[default]
    #[serde(rename = "malvar")]
    MalvarHeCutler,
    /// 双线性（去掉全部跨通道校正项），回归基线。
    #[serde(rename = "bilinear")]
    Bilinear,
}

/// WB 来源（设计 5.6 的 `wb` 参数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WbMode {
    /// 文件 as-shot 乘子。
    #[default]
    #[serde(rename = "as_shot")]
    AsShot,
    /// 相机自动估计。一期解码器未提供独立 daylight 估计入口，
    /// 按设计 5.5 定版**等同 as-shot 并在展开注记中注明**，不自研估计算法。
    #[serde(rename = "auto")]
    Auto,
}

/// 展开参数（设计 5.6 的 `raw_options` 子集；镜头参数属后续镜头包）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DevelopOptions {
    pub wb: WbMode,
    pub demosaic: Demosaic,
    /// 用户预曝光（EV），契约范围 [-4.0, +4.0]，越界报错不 clamp（设计 5.6）。
    pub pre_exposure_ev: f32,
}

impl DevelopOptions {
    /// 契约校验（设计 5.6：导入契约层报错、执行层钳制，是本仓既有分工）。
    pub fn validate(&self) -> Result<()> {
        if !self.pre_exposure_ev.is_finite() || !(-4.0..=4.0).contains(&self.pre_exposure_ev) {
            return Err(RawError::InvalidOption(format!(
                "pre_exposure_ev={} 越界（允许 [-4.0, +4.0]）",
                self.pre_exposure_ev
            )));
        }
        Ok(())
    }
}

/// 镜头 EXIF 原始输入（设计 5.6 的 `lens_exif`；镜头包消费，本包只解析落账）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct LensExif {
    pub make: Option<String>,
    pub model: Option<String>,
    pub focal_mm: Option<f32>,
    pub fnumber: Option<f32>,
    pub focus_distance_m: Option<f32>,
}

/// 展开期实际应用的镜头校正留痕（设计 5.9.5 的字段形状，本包恒为 `None`：
/// 镜头校正是后续包，本包只把 schema 一次到位，不许等实施时再改版 metadata）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedLensProfile {
    pub source_db: String,
    pub db_version: String,
    pub lens_id: String,
    pub matched_via: String,
    pub focal_mm_effective: Option<f32>,
    pub sources: serde_json::Value,
    pub distortion_ptlens: Option<[f32; 3]>,
    pub vignetting_pa: Option<[f32; 3]>,
    pub tca: Option<serde_json::Value>,
    pub crop_scale_applied: f32,
}

/// `raw_develop` 留痕记录（设计 5.6 定版形状）：写入对象 metadata，
/// 使二期非破坏性 re-develop 在数据上成为可能（R7）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawDevelopRecord {
    pub version: u32,
    /// 源格式 sniff 结果（"dng" / "cr3" / "raf" / "tiff" / "raw" 等）。
    pub source_format: String,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// 生效的 WB 来源（请求的模式；auto 回退 as-shot 时仍记请求值，
    /// 回退事实记在展开注记里，不许靠本字段猜）。
    pub wb: WbMode,
    pub demosaic: Demosaic,
    /// 用户给的 pre_exposure_ev（原样）。
    pub pre_exposure_ev: f32,
    /// 实际生效的 BaselineExposure（标签不可得时为 0.0，事实见展开注记）。
    pub baseline_exposure_ev: f32,
    /// 实际施加的 WB 乘子（已重归一化到 min_c w_c = 1.0，设计 5.5 硬规范）。
    pub wb_multipliers: [f32; 3],
    pub black_levels: [f32; 4],
    pub white_levels: [f32; 4],
    pub cfa: String,
    /// 本包未应用方向变换（⑧ 在导入链包与 JPEG 路抽共用实现），恒记 1；
    /// 文件原始方向值在 `DecodedRaw::file_meta` 里，下一包接手。
    pub orientation_applied: u32,
    pub decoder_name: String,
    pub decoder_version: String,
    /// 色彩链实际用的插值权重 α（单矩阵时为 `None`）。
    pub color_alpha: Option<f32>,
    pub lens_exif: Option<LensExif>,
    /// 五态：matched / unmatched / manual / db_unavailable / off。
    /// 本包不做镜头匹配，恒为 `None`（镜头包填），不许冒充 unmatched。
    pub lens_match: Option<String>,
    pub applied_lens_profile: Option<AppliedLensProfile>,
}

/// 文件级元数据（解码期解析，与展开产物分开：回显 `raw_meta` 的原料，设计 5.7）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RawFileMeta {
    pub make: Option<String>,
    pub model: Option<String>,
    pub lens: Option<String>,
    pub lens_exif: Option<LensExif>,
    pub iso: Option<u32>,
    pub exposure_time: Option<f32>,
    pub fnumber: Option<f32>,
    pub focal_mm: Option<f32>,
    /// EXIF Orientation 原始值（1–8）；缺失按 1（与 jpeg.rs 兜底口径一致）。
    pub orientation: u32,
    /// 传感器位深（文件口径，如 14）。
    pub bit_depth: Option<u32>,
    pub source_format: String,
}

/// 留痕记录的上下文（解码期才知道的事实：源格式／机型／解码器版本）。
/// 展开本身只消费帧 + 参数，记录由 `develop` 用本上下文组装。
#[derive(Debug, Clone, PartialEq)]
pub struct RecordContext {
    pub source_format: String,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub lens_exif: Option<LensExif>,
    pub decoder_name: String,
    pub decoder_version: String,
}

impl Default for RecordContext {
    fn default() -> Self {
        Self {
            source_format: "raw".to_string(),
            camera_make: None,
            camera_model: None,
            lens: None,
            lens_exif: None,
            decoder_name: "rawler".to_string(),
            decoder_version: String::new(),
        }
    }
}

/// 解码产物：展开输入帧 + 文件元数据。
#[derive(Debug, Clone)]
pub struct DecodedRaw {
    pub frame: RawFrame,
    pub file_meta: RawFileMeta,
    /// 解码期注记（ActiveArea 非法回退、标定矩阵跳过等事实）。
    /// 调用方应并入展开注记一起呈现，不许丢弃（NF2）。
    pub notes: Vec<String>,
}

/// 展开产物：线性 f32 RGB（行主序三元组，线性 sRGB D65 原色，
/// 出口已截零、无负值、无 NaN）+ 留痕记录 + 展开注记。
#[derive(Debug, Clone)]
pub struct DevelopedImage {
    pub width: u32,
    pub height: u32,
    pub rgb_linear: Vec<f32>,
    pub record: RawDevelopRecord,
    /// 展开过程中生效的回退／缺口事实（如 auto WB 回退、BaselineExposure
    /// 不可得、双矩阵缺色温退单矩阵）。注记是信号，不许静默（NF2）。
    pub notes: Vec<String>,
}
