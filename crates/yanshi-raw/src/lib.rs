//! yanshi-raw：RAW 解码边界 + 线性展开主链（设计 v3「RAW 导入」的第一包）。
//!
//! 构成（设计 9.1）：
//! - `decode`：文件字节 → 自有 `RawFrame`（**全仓唯一允许接触 rawler 的模块**）；
//! - `develop`：展开主链 ① 归一化 → ③ WB → ④ MHC 解马赛克 → ④′ 高光恢复
//!   → ⑥ 复合色彩链 → ⑦ pre_exposure → ⑨ 出口截零，纯函数、无 IO、
//!   逐位确定性（NF1）。
//!
//! 包序：展开主链（第一包）、入库链路（第二包）、高光恢复（第三包）
//! 已落地；镜头校正三项（② ⑤，第四包）尚未实现、不在此冒充。
//! 消费入口在 `yanshi-render`（feature `raw`）：`develop_raw_source`。
//!
//! 确定性纪律：浮点超越函数只用 `libm::` 自由函数；平台相关的浮点
//! method 调用（powf／atan2 等的方法式写法）禁止出现在本 crate（含测试），
//! 交付前 grep 自查。

pub mod decode;
pub mod develop;
pub mod error;
pub mod frame;
pub mod import;

#[cfg(feature = "test-support")]
pub mod dng_fixture;

pub use decode::{decode_bytes, probe_raw, raw_meta, record_context};
pub use develop::develop;
pub use error::{RawError, Result};
pub use frame::{
    CalibrationMatrix, CfaPattern, Channel, DecodedRaw, Demosaic, DevelopOptions, DevelopedImage,
    LensExif, RawDevelopRecord, RawFileMeta, RawFrame, RawMeta, RawProbe, RecordContext, WbMode,
};
pub use import::{plan_import, ImportPlan, LensCorrection, ParsedRawOptions};
