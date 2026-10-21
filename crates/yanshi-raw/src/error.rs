//! 展开链的结构化错误（口径对齐设计 5.8 错误分类表：零 panic、响亮失败）。

/// yanshi-raw 的全部失败形态。解码与展开任一环节失败都走这里，
/// 不许静默降级、不许画半张图（设计 NF2）。
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RawError {
    /// 解码器拒绝（文件损坏／格式不支持／未知机型），含解码器错误原文。
    #[error("RAW 解码失败：{0}")]
    Decode(String),
    /// 解码产物不是单通道 Bayer 马赛克（一期只支持 Bayer 四排列）。
    #[error("不支持的 RAW 数据形态：{0}")]
    UnsupportedData(String),
    /// CFA 不是 Bayer 四排列之一（X-Trans 等）。
    #[error("该文件的 CFA 为 {pattern}，一期仅支持 Bayer 四排列（RGGB/BGGR/GRBG/GBRG）")]
    NonBayerCfa { pattern: String },
    /// 黑／白电平非法（white ≤ black，除法无意义）。
    #[error("RAW 电平数据非法：white {white} ≤ black {black}（相位 {phase}）")]
    InvalidLevels {
        phase: usize,
        white: f32,
        black: f32,
    },
    /// 文件未提供可用的白平衡乘子。
    #[error("文件未提供可用的白平衡乘子（as-shot WB 缺失或非法）")]
    MissingWhiteBalance,
    /// 文件未提供色彩标定矩阵（ColorMatrix），复合色彩链无从构造。
    #[error("文件未提供色彩标定矩阵（ColorMatrix 缺失）")]
    MissingColorMatrix,
    /// 色彩矩阵奇异（行列式近零），不可逆。
    #[error("色彩标定矩阵奇异、不可逆（det={det})")]
    SingularColorMatrix { det: f32 },
    /// 展开参数非法（契约层报错、不静默 clamp，设计 5.6）。
    #[error("展开参数非法：{0}")]
    InvalidOption(String),
    /// 帧数据自相矛盾（尺寸与像素数对不上等）。
    #[error("RAW 帧数据非法：{0}")]
    InvalidFrame(String),
    /// 出口仍出现非有限值（+Inf），管线内部必有病变，响亮失败不许带病出门。
    #[error("展开出口出现非有限像素值（{0}），管线内部病变")]
    NonFiniteOutput(String),
}

pub type Result<T> = std::result::Result<T, RawError>;
