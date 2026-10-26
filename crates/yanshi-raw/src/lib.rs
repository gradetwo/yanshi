//! yanshi-raw：RAW 解码边界 + 线性展开主链（设计 v3「RAW 导入」）。
//!
//! 构成（设计 9.1）：
//! - `decode`：文件字节 → 自有 `RawFrame`（**全仓唯一允许接触 rawler 的模块**）；
//! - `develop`：展开主链 ① 归一化 → ② 暗角 → ③ WB → ④ MHC 解马赛克
//!   → ④′ 高光恢复 → ⑤ 畸变/TCA 重采样 → ⑥ 复合色彩链 → ⑦
//!   pre_exposure → ⑨ 出口截零，纯函数、无 IO、逐位确定性（NF1）；
//! - `lens_db` / `lens`：Lensfun 数据（只用数据、自写解析）与镜头
//!   校正数学（插值、画幅换算、逐项优先合并、plan 双向构造）；
//! - `dng_opcode`：DNG OpcodeList2 解析（文件自带校正的来源）。
//!
//! 包序：展开主链（第一包）、入库链路（第二包）、高光恢复保底
//! （第三包）、镜头校正三项（第四包）、真色度恢复（第五包）均已
//! 落地。消费入口在 `yanshi-render`（feature `raw`）：
//! `develop_raw_source` / `develop_raw_source_recorded`。
//!
//! 确定性纪律：浮点超越函数只用 `libm::` 自由函数；平台相关的浮点
//! method 调用（powf／atan2 等的方法式写法）禁止出现在本 crate（含测试），
//! 交付前 grep 自查。

pub mod decode;
pub mod develop;
pub mod dng_opcode;
pub mod error;
pub mod frame;
pub mod import;
pub mod lens;
pub mod lens_db;

#[cfg(feature = "test-support")]
pub mod dng_fixture;

pub use decode::{decode_bytes, probe_raw, raw_meta, record_context};
pub use develop::{develop, develop_with_lens, LensExecution};
pub use error::{RawError, Result};
pub use frame::{
    AppliedLensProfile, CalibrationMatrix, CfaGrid, CfaPattern, Channel, DecodedRaw, Demosaic,
    DevelopOptions, DevelopedImage, FileLensCorrection, HighlightMode, LensCorrectionPlan,
    LensExif, RawDevelopRecord, RawFileMeta, RawFrame, RawMeta, RawProbe, RecordContext, TcaParams,
    ToneMode, VignetteModel, WarpModel, WarpRectilinearSet, WbMode,
};
pub use import::{plan_import, plan_import_with_db, ImportPlan, LensCorrection, ParsedRawOptions};
