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

/// 通用 CFA 网格（设计 5.2 推广，ISSUE-3）：周期 `period × period` 的方阵
/// 通道排布，是帧内的统一 CFA 载体——Bayer（period=2）与富士 X-Trans
/// （period=6）同路。Bayer 四排列仍由 `CfaPattern` 命名与判据，
/// `CfaGrid::bayer()` 命中时展开走 MHC 原路、像素逐位不变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfaGrid {
    period: usize,
    /// 行主序 period² 格通道。
    cells: Vec<Channel>,
    /// 排列名：Bayer 取排列名（"RGGB" 等）；其余取解码器给出的
    /// 模式串（如 X-Trans 的 36 字符排列），原样进 record／metadata。
    name: String,
}

impl CfaGrid {
    /// 由周期与行主序通道格构造；形状不符（period<2 或格数 ≠ period²）⇒ 报错。
    pub fn new(period: usize, cells: Vec<Channel>, name: String) -> Result<Self> {
        if period < 2 || cells.len() != period * period {
            return Err(RawError::InvalidFrame(format!(
                "CFA 网格形状非法：period={period}，格数={}",
                cells.len()
            )));
        }
        Ok(Self {
            period,
            cells,
            name,
        })
    }

    /// 由 Bayer 排列构造（period=2，name=排列名）。
    pub fn from_bayer(pattern: CfaPattern) -> Self {
        Self {
            period: 2,
            cells: pattern.cell().to_vec(),
            name: pattern.name().to_string(),
        }
    }

    /// period=2 且逐格命中 Bayer 四排列之一 ⇒ `Some`（解马赛克据此分流）。
    pub fn bayer(&self) -> Option<CfaPattern> {
        if self.period != 2 {
            return None;
        }
        CfaPattern::from_cell([self.cells[0], self.cells[1], self.cells[2], self.cells[3]])
    }

    /// CFA 周期（Bayer=2，X-Trans=6）。
    pub fn period(&self) -> usize {
        self.period
    }

    /// 排列名（与 record／metadata 的 `cfa` 字段同字面）。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 像素 (x, y) 处的采样通道。相位以帧左上角为 (0,0) 锚点。
    pub fn channel_at(&self, x: usize, y: usize) -> Channel {
        self.cells[(y % self.period) * self.period + (x % self.period)]
    }

    /// 像素 (x, y) 的 CFA 相位下标（0..period²，行主序，与黑／白电平数组同序）。
    pub fn phase_at(&self, x: usize, y: usize) -> usize {
        (y % self.period) * self.period + (x % self.period)
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
    /// CFA 网格（相位锚点 = 本帧左上角）。
    pub cfa: CfaGrid,
    /// 黑电平：按 CFA 相位的行主序 period² 标量，
    /// **逐相位独立、绝不合并同色相位**（设计 5.2 硬性要求，实证 V5；
    /// Bayer 时即 `[R 位, G1 位, G2 位, B 位]` 四标量）。
    pub black_levels: Vec<f32>,
    /// 白电平：同相位口径 period² 标量。
    pub white_levels: Vec<f32>,
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
    /// 文件 EXIF Orientation（1–8；缺失／损坏按 1，与 jpeg.rs 兜底同口径）。
    /// 展开步骤 ⑧ 据此变换（第二包接入；第一包时此值只解析未应用）。
    pub orientation: u32,
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

/// 高光恢复档位（④′ 的行为选择）：第三包保底机制与第五包真色度恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HighlightMode {
    /// 第三包保底机制（解析阈值 + 单调性下界 + 色度门控 desat）为止，
    /// 不做色度估计。旧记录（无此字段）按此档重展开，保证已入库对象
    /// 的像素不因引擎升级漂移。
    #[default]
    #[serde(rename = "floor")]
    Floor,
    /// 第五包：在保底机制之后追加真色度恢复（未饱和通道 + 邻域纹理
    /// 传播估计饱和通道的色度与层次）。新导入缺省此档（导入计划层
    /// 决定，`DevelopOptions::default()` 本身仍是 Floor，见上）。
    #[serde(rename = "chroma")]
    Chroma,
}

/// 展开参数（设计 5.6 的 `raw_options` 子集；镜头意图在 `import` 层
/// 解析为 `LensCorrectionPlan` 后经 `develop_with_lens` 注入，不进本结构体）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DevelopOptions {
    pub wb: WbMode,
    pub demosaic: Demosaic,
    /// 用户预曝光（EV），契约范围 [-4.0, +4.0]，越界报错不 clamp（设计 5.6）。
    pub pre_exposure_ev: f32,
    /// 高光恢复档位（缺省 Floor：`develop()` 的历史行为逐位不变；
    /// 新导入由导入计划显式置 Chroma 并落账，取图侧按记录重展开）。
    pub highlight: HighlightMode,
    /// 预览档长边上限（C1-b，期 2）：`Some(edge)` 时展开链尾把输出
    /// 压到长边 ≤ edge（面积加权降采样），且 record 记
    /// `developed_max_edge` —— 预览是**有标记的近似**，不许冒充
    /// 成品。`None`（缺省）= 全档，现行行为逐位不变。合法域
    /// 64–8192，`validate` 守（越界响亮报错、不 clamp）。
    pub max_output_edge: Option<u32>,
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
        if let Some(edge) = self.max_output_edge {
            if !(64..=8192).contains(&edge) {
                return Err(RawError::InvalidOption(format!(
                    "max_output_edge={edge} 越界（允许 64–8192）"
                )));
            }
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

/// 展开期实际应用的镜头校正留痕（设计 5.9.5 定版字段形状；第四包起
/// 真正填充）。`distortion_ptlens`／`vignetting_pa` 存的是**镜头库原样
/// 参数**（出处可查），取图侧凭 `crop_ratio` 用同一套换算函数重建
/// 像面有效参数——重建规则只有一个出处（`lens::plan_from_applied`），
/// 不许两处各算一遍。
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
    /// 画幅换算比 s = C_cal / C_img（标定画幅 / 本图画幅，设计 5.9.3）；
    /// 旧记录无此字段 ⇒ `None`，重建时按 s=1 处理（与当时的恒 None
    /// profile 一起出现时不生效，因为旧记录 profile 本身为 None）。
    #[serde(default)]
    pub crop_ratio: Option<f32>,
    /// 文件自带校正（DNG OpcodeList2）实际采用的参数原文（JSON）：
    /// `{warp_rectilinear: {sets: [[kr0,kr1,kr2,kr3,kt0,kt1]...], center: [cx, cy]},
    ///   fix_vignette_radial: {k: [k0..k4], center: [cx, cy]}}`，
    /// 哪一项生效由 `sources` 逐项标注。镜头库路径恒 `None`。
    #[serde(default)]
    pub opcode: Option<serde_json::Value>,
}

/// `raw_develop` 留痕记录（设计 5.6 定版形状）：写入对象 metadata，
/// 使二期非破坏性 re-develop 在数据上成为可能（R7）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawDevelopRecord {
    pub version: u32,
    /// 源 RAW 文件的 blob hash（设计 5.6 定版字段）：展开期（develop）不
    /// 知道存储、恒 `None`；导入期由入库端填入落账（re-develop 与审计凭它
    /// 回到源字节）。第一包结构漏了这个字段，第二包补上；`serde(default)`
    /// 保证第一包写出的无此字段记录仍可解析。
    #[serde(default)]
    pub source_blob: Option<String>,
    /// 源格式 sniff 结果（"dng" / "cr3" / "raf" / "tiff" / "raw" 等）。
    pub source_format: String,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    /// 生效的 WB 来源（请求的模式；auto 回退 as-shot 时仍记请求值，
    /// 回退事实记在展开注记里，不许靠本字段猜）。
    pub wb: WbMode,
    /// 高光恢复档位（第五包补字段）：第三／四包写出的记录无此字段，
    /// `serde(default)` = Floor，保证旧对象按当时的保底机制重展开、
    /// 像素不漂移；新导入由导入计划落实际档位。
    #[serde(default)]
    pub highlight_recovery: HighlightMode,
    pub demosaic: Demosaic,
    /// 用户给的 pre_exposure_ev（原样）。
    pub pre_exposure_ev: f32,
    /// 实际生效的 BaselineExposure（标签不可得时为 0.0，事实见展开注记）。
    pub baseline_exposure_ev: f32,
    /// 实际施加的 WB 乘子（已重归一化到 min_c w_c = 1.0，设计 5.5 硬规范）。
    pub wb_multipliers: [f32; 3],
    /// 黑／白电平（逐相位 period²，裁后帧口径；Bayer 时为四相位）。
    pub black_levels: Vec<f32>,
    pub white_levels: Vec<f32>,
    pub cfa: String,
    /// 实际应用的方向变换值（第二包起记真实值：= 文件 EXIF Orientation，
    /// 缺失／非法按 1；变换表与 JPEG 路共用 `yanshi_core::orientation`）。
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
    /// 本次展开的档位（C1-b，期 2）：`Some(edge)` = 按预览档展开
    /// （输出长边被压到 ≤ edge），`None` = 全档。旧记录无此字段
    /// ⇒ `serde(default)` 解析为 `None` = 全档，与旧回放口径一致。
    /// 取图侧重展开时凭它复原档位、渲染缓存凭它分键——预览与
    /// 成品永不共用一份缓存。
    #[serde(default)]
    pub developed_max_edge: Option<u32>,
}

impl RawDevelopRecord {
    /// **重放前的自洽检查**（第 1440 轮审计）。
    ///
    /// `lens_match` 与 `applied_lens_profile` 是一对互相约束的字段：前者说
    /// 「施加过哪一类校正」、后者是「实际施加的参数」。二者矛盾时旧代码**照常展开**
    /// 且不吭声 ⇒ 重放像素与导入当时不同而调用方无信号。返回的每条文字都必须
    /// 进响应的 warnings（`develop_raw_source_recorded` 负责搬运）。
    pub fn replay_gaps(&self) -> Vec<String> {
        let mut out = Vec::new();
        match (&self.lens_match, &self.applied_lens_profile) {
            (Some(label), None) if label == "matched" || label == "manual" => out.push(format!(
                "记录的 lens_match={label} 声称施加过镜头校正，却没有 applied_lens_profile ⇒ \
                 本次按无校正展开，像素与导入当时不一致"
            )),
            (None, Some(_)) => out.push(
                "记录带有 applied_lens_profile 却缺 lens_match ⇒ 出处未知，重放按 \
                 lens_match=record_unspecified 记账（旧版在这里凭空写成 matched，把出处编造了一次）"
                    .to_string(),
            ),
            _ => {}
        }
        out
    }
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
    /// EXIF DateTimeOriginal **原文字符串**（不解析、不换算时区，设计 5.7）。
    pub date_time_original: Option<String>,
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
    /// 本图画幅的裁切系数 C_img：由 EXIF 35mm 等效焦距 / 实际焦距
    /// 推出（两者齐备时）；取不到 ⇒ `None`，镜头画幅换算按 s=1 并
    /// 在导入注记中注明（不许编一个数字进去）。
    pub crop_factor: Option<f32>,
    /// 文件自带镜头校正（DNG OpcodeList2 中本引擎认得的两项；
    /// 解码期在 `decode` 解析，非 DNG / 无此标签 ⇒ `None`）。
    pub file_lens: Option<FileLensCorrection>,
}

/// DNG WarpRectilinear 的一组系数（单平面或全平面共用，DNG 规范 1.7
/// 第 7 章）：径向 kr0..kr3 + 切向 kt0/kt1，参数为文件原样 f64→f32。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WarpRectilinearSet {
    pub kr: [f32; 4],
    pub kt: [f32; 2],
}

/// 文件自带镜头校正（DNG OpcodeList2 解析产物，第四包）：只收本引擎
/// 会执行的两项；其余 opcode 在注记里点名跳过，不静默吞。
#[derive(Debug, Clone, PartialEq)]
pub struct FileLensCorrection {
    /// WarpRectilinear（opcode 1）：1 组（全平面共用）或 3 组（R/G/B
    /// 逐平面——畸变与横向色差在同一 opcode 里逐项给出）。
    pub warp_sets: Option<Vec<WarpRectilinearSet>>,
    /// Warp 的归一化光心（相对左上像素，(0.5, 0.5) = 画面中心）。
    pub warp_center: Option<[f32; 2]>,
    /// FixVignetteRadial（opcode 3）的增益多项式系数 k0..k4：
    /// 增益 g = 1 + k0·r² + k1·r⁴ + k2·r⁶ + k3·r⁸ + k4·r¹⁰
    /// （r 为相对光心的归一化距离，分母是光心到最远像素的距离）。
    pub vignette_k: Option<[f32; 5]>,
    /// FixVignetteRadial 的归一化光心。
    pub vignette_center: Option<[f32; 2]>,
    /// 列表里出现但本引擎不执行的 opcode ID（留痕用，去重保序）。
    pub skipped_opcodes: Vec<u32>,
}

/// 暗角模型（像面有效口径，展开期消费）：两种来源在本层归一——
/// develop 只认「逐像素增益场」这一种产物，不关心参数出处。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VignetteModel {
    /// Lensfun `pa` 模型（已按画幅换算折算到像面）：衰减
    /// V(r) = 1 + k1·r² + k2·r⁴ + k3·r⁶，增益 = 1/V(r)，
    /// r 为半对角归一化半径（角点 r=1）。
    Pa([f32; 3]),
    /// DNG FixVignetteRadial（文件原样）：增益
    /// g = 1 + k0·r² + k1·r⁴ + k2·r⁶ + k3·r⁸ + k4·r¹⁰ 直接是乘子，
    /// r 的分母是光心到最远像素的距离、光心可偏离画面中心。
    OpcodeGain { k: [f32; 5], center: [f32; 2] },
}

/// 几何 warp 模型（步骤 ⑤ 复合重采样，展开期消费）。
#[derive(Debug, Clone, PartialEq)]
pub enum WarpModel {
    /// Lensfun PTLens 畸变 + TCA：参数为**库原样**（标定画幅口径），
    /// `crop_ratio` 为 s = C_cal/C_img，评估时在标定口径里算完再折回
    /// 像面（换算式只有一个出处：`lens::warp_source_coord`）。
    PtlensTca {
        /// PTLens (a, b, c)。
        ptlens: [f32; 3],
        /// TCA 有效参数（模型名 + vr/vb/br/bb）；`None` = 无 TCA 项。
        tca: Option<TcaParams>,
        crop_ratio: f32,
    },
    /// DNG WarpRectilinear（文件原样）：1 组或 3 组系数 + 归一化光心，
    /// 按 DNG 规范公式逐平面评估（畸变与横向色差同在此一项）。
    OpcodeWarp {
        sets: Vec<WarpRectilinearSet>,
        center: [f32; 2],
    },
}

impl WarpModel {
    /// 数学上的恒等 warp 判定：PTLens 系数全零（P(r) ≡ 1）且 TCA
    /// 为单位，或 WarpRectilinear 每组 kr=(1,0,0,0)、kt=(0,0)。
    /// 恒等时展开层整步跳过重采样——不是省事，是**精确性**：恒等
    /// 函数经归一化半径的浮点往返会引入末位误差，「零校正必须与
    /// 无校正逐位一致」的纪律（D5）只能靠不计算来保证。
    pub fn is_identity(&self) -> bool {
        match self {
            WarpModel::PtlensTca { ptlens, tca, .. } => {
                *ptlens == [0.0, 0.0, 0.0]
                    && tca.is_none_or(|t| t.vr == 1.0 && t.vb == 1.0 && t.br == 0.0 && t.bb == 0.0)
            }
            WarpModel::OpcodeWarp { sets, .. } => sets
                .iter()
                .all(|s| s.kr == [1.0, 0.0, 0.0, 0.0] && s.kt == [0.0, 0.0]),
        }
    }
}

/// TCA 有效参数（Lensfun linear / poly3 两模型归一后的形状）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TcaParams {
    /// `true` = poly3（带三次项 br/bb），`false` = linear。
    pub poly3: bool,
    pub vr: f32,
    pub vb: f32,
    pub br: f32,
    pub bb: f32,
}

/// 一次展开要施加的镜头校正计划（第四包）：导入期由镜头库匹配 /
/// 文件自带校正解析产出（`import`），或取图期由落账记录重建
/// （`lens::plan_from_applied`）。`develop_with_lens` 只消费本结构体，
/// 不查库、不读文件——展开保持纯函数。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LensCorrectionPlan {
    /// 步骤 ② 的暗角（马赛克域增益场 + 高光阈值接线共用同一份场）。
    pub vignette: Option<VignetteModel>,
    /// 步骤 ⑤ 的畸变 + TCA 复合 warp（一次重采样）。
    pub warp: Option<WarpModel>,
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

/// 入库识别探针的产物（设计 5.3）：字节被认作 RAW 时给出源格式与机型概要。
/// 识别口径 = magic 签名 ∧ 解码器能认出该文件（见 `decode::probe_raw`）；
/// 纯 TIFF 位图（解码器不认）不会命中，不许把普通 TIFF 误判成 RAW。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawProbe {
    /// 源格式 sniff 结果（与留痕记录的 `source_format` 同口径）。
    pub source_format: String,
    /// 相机厂商（元数据可得时；取不到给 `None`，不阻断识别）。
    pub make: Option<String>,
    /// 相机型号（同上）。
    pub model: Option<String>,
}

/// `raw_meta` 回显（设计 5.7 字段清单）：回显与对象 metadata **共用同一份
/// 解析结果**——由 [`crate::decode::raw_meta`] 从同一次解码产物组装，
/// 禁止两处各解析一遍。字段缺失 ⇒ 该键为 `null`（JSON 侧），不整表失败。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawMeta {
    pub make: Option<String>,
    pub model: Option<String>,
    /// 镜头厂商／型号（分开给，与 `lens_exif` 同源）。
    pub lens_make: Option<String>,
    pub lens_model: Option<String>,
    /// 镜头显示名（make + model 拼接后的既有口径，与 record 的 `lens` 同值）。
    pub lens: Option<String>,
    pub iso: Option<u32>,
    /// 曝光时间（秒）。
    pub exposure_time_s: Option<f32>,
    pub fnumber: Option<f32>,
    pub focal_mm: Option<f32>,
    pub focus_distance_m: Option<f32>,
    /// DateTimeOriginal 原文字符串。
    pub date_time_original: Option<String>,
    /// EXIF Orientation（1–8，缺失按 1）。
    pub orientation: u32,
    /// CFA 排列名（"RGGB" 等）。
    pub cfa: String,
    /// 传感器位深（文件口径）。
    pub bit_depth: Option<u32>,
    /// 黑／白电平（逐相位 period²，裁后帧口径，与 record 同值）。
    pub black_levels: Vec<f32>,
    pub white_levels: Vec<f32>,
    /// 文件口径的 WB 乘子（未重归一化；重归一化后的生效值在 record 里）。
    pub wb_multipliers: Option<[f32; 3]>,
    /// active area 宽／高（展开产物尺寸口径，方向变换前）。
    pub width: u32,
    pub height: u32,
    pub source_format: String,
}
