//! 导入计划（设计 5.6／5.7，第二包）：把「一份 RAW 字节 + 调用方给的
//! `raw_options`」变成导入期需要的全部产物——落账用的 `raw_develop` 记录、
//! 回显用的 `raw_meta`、以及必须让调用方看见的 warnings。
//!
//! 为什么在本 crate 组装：record 与 meta 的字段语义归 yanshi-raw（设计 9.3），
//! server 只做不透明透传与落账；两份产物同源于同一次解码，禁止调用方再
//! 解析一遍字节（设计 5.7「禁止两处各解析」）。
//!
//! D5 口径（2026-10-22 定版）：`lens_correction` 缺省 `"off"`；本包无镜头库，
//! `"auto"` ⇒ `lens_match = "db_unavailable"` + warning（不阻断导入）；
//! `lens_override` 非空 ⇒ 响亮报错（手动指定是调用方的明确意图，静默忽略
//! 等于骗他——本包如实说「镜头库尚未接入」，不许冒充查无此镜）。

use serde_json::Value;

use crate::decode::{raw_meta, record_context};
use crate::develop::{
    as_shot_neutral, assemble_record, composite_color_matrix, renormalized_wb_multipliers,
};
use crate::error::{RawError, Result};
use crate::frame::{DecodedRaw, Demosaic, DevelopOptions, RawDevelopRecord, RawMeta, WbMode};

/// `raw_options` 解析后的镜头校正意图（本包只落痕，不执行校正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LensCorrection {
    /// D5 定版缺省：不动像素。
    Off,
    /// 调用方要求自动校正；本包无镜头库 ⇒ 落 `db_unavailable`。
    Auto,
}

/// 解析后的导入参数全集（展开参数 + 镜头意图）。
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRawOptions {
    pub develop: DevelopOptions,
    pub lens_correction: LensCorrection,
    pub lens_override: Option<String>,
}

impl Default for ParsedRawOptions {
    fn default() -> Self {
        Self {
            develop: DevelopOptions::default(),
            lens_correction: LensCorrection::Off,
            lens_override: None,
        }
    }
}

/// 导入计划：导入期一次算齐的产物（record 落账、meta 回显、warnings 呈现）。
#[derive(Debug, Clone)]
pub struct ImportPlan {
    pub record: RawDevelopRecord,
    pub meta: RawMeta,
    pub warnings: Vec<String>,
    pub options: ParsedRawOptions,
}

/// `raw_options` 的合法键全集（多一个键即报错——拼写错误不许静默吞掉）。
const RAW_OPTION_KEYS: [&str; 5] = [
    "wb",
    "demosaic",
    "pre_exposure_ev",
    "lens_correction",
    "lens_override",
];

/// 解析并校验 `raw_options`（设计 5.6 参数表）：枚举外／越界／类型不对
/// 一律 `InvalidOption` 逐项指明，不 clamp、不静默取舍。
pub fn parse_raw_options(raw_options: Option<&Value>) -> Result<ParsedRawOptions> {
    let mut parsed = ParsedRawOptions::default();
    let Some(value) = raw_options else {
        return Ok(parsed);
    };
    let Some(object) = value.as_object() else {
        return Err(RawError::InvalidOption(
            "raw_options 必须是对象 {wb, demosaic, pre_exposure_ev, lens_correction, lens_override}"
                .to_string(),
        ));
    };
    for key in object.keys() {
        if !RAW_OPTION_KEYS.contains(&key.as_str()) {
            return Err(RawError::InvalidOption(format!(
                "raw_options 不接受参数 {key}（拼写错误？）；可用参数：{}",
                RAW_OPTION_KEYS.join(", ")
            )));
        }
    }
    if let Some(wb) = object.get("wb") {
        parsed.develop.wb = match wb.as_str() {
            Some("as_shot") => WbMode::AsShot,
            Some("auto") => WbMode::Auto,
            _ => {
                return Err(RawError::InvalidOption(format!(
                    "raw_options.wb 非法：{wb}（可选 \"as_shot\" | \"auto\"）"
                )))
            }
        };
    }
    if let Some(demosaic) = object.get("demosaic") {
        parsed.develop.demosaic = match demosaic.as_str() {
            Some("malvar") => Demosaic::MalvarHeCutler,
            Some("bilinear") => Demosaic::Bilinear,
            _ => {
                return Err(RawError::InvalidOption(format!(
                    "raw_options.demosaic 非法：{demosaic}（可选 \"malvar\" | \"bilinear\"）"
                )))
            }
        };
    }
    if let Some(ev) = object.get("pre_exposure_ev") {
        let Some(ev) = ev.as_f64() else {
            return Err(RawError::InvalidOption(format!(
                "raw_options.pre_exposure_ev 必须是数字，实际为 {ev}"
            )));
        };
        parsed.develop.pre_exposure_ev = ev as f32;
    }
    parsed.develop.validate()?;
    if let Some(lens_correction) = object.get("lens_correction") {
        parsed.lens_correction = match lens_correction.as_str() {
            Some("off") => LensCorrection::Off,
            Some("auto") => LensCorrection::Auto,
            _ => {
                return Err(RawError::InvalidOption(format!(
                    "raw_options.lens_correction 非法：{lens_correction}（可选 \"auto\" | \"off\"）"
                )))
            }
        };
    }
    if let Some(lens_override) = object.get("lens_override") {
        if !lens_override.is_null() {
            let Some(text) = lens_override.as_str() else {
                return Err(RawError::InvalidOption(format!(
                    "raw_options.lens_override 必须是字符串或 null，实际为 {lens_override}"
                )));
            };
            parsed.lens_override = Some(text.to_string());
        }
    }
    Ok(parsed)
}

/// 由解码产物 + 已解析参数组装导入计划。
///
/// record 的生效量（重归一化 WB 乘子／色彩链 α／Baseline 缺省）与 `develop`
/// 走同一批函数现算，保证「导入期落账的 record」与「取图期按 record 重展开」
/// 逐字段一致（判据在集成测试里对账）。
pub fn plan_import(decoded: &DecodedRaw, raw_options: Option<&Value>) -> Result<ImportPlan> {
    let options = parse_raw_options(raw_options)?;
    if let Some(override_name) = &options.lens_override {
        return Err(RawError::InvalidOption(format!(
            "raw_options.lens_override 指定了镜头「{override_name}」，但镜头库尚未接入\
             （镜头校正在第四包落地），本包不支持手动指定镜头——请去掉 lens_override，\
             或等镜头包合入后再指定（不静默忽略手动指定，设计 5.8）"
        )));
    }
    let frame = &decoded.frame;
    let mut warnings = decoded.notes.clone();
    // 生效量与 develop 同源现算（同一批函数、同一份帧）。
    let multipliers = renormalized_wb_multipliers(frame)?;
    let neutral = as_shot_neutral(frame)?;
    let mut color_notes = Vec::new();
    let composite = composite_color_matrix(&frame.color_matrices, neutral, &mut color_notes)?;
    warnings.extend(color_notes);
    if options.develop.wb == WbMode::Auto {
        warnings.push(
            "wb=auto：一期等同 as_shot（rawler 0.7.2 无独立 daylight 估计入口，设计 5.5 定版）"
                .to_string(),
        );
    }
    let baseline = match frame.baseline_exposure_ev {
        Some(ev) => ev,
        None => {
            warnings.push(
                "BaselineExposure 不可得（rawler 0.7.2 未暴露该标签），按 0 EV 处理".to_string(),
            );
            0.0
        }
    };
    let meta = raw_meta(decoded);
    // D5 口径落痕：off / db_unavailable 二选一（本包不出现 matched/manual）。
    let lens_match = match options.lens_correction {
        LensCorrection::Auto => {
            warnings.push(
                "lens_correction=auto：镜头库尚未接入（第四包），本次导入未做镜头校正\
                 （lens_match=db_unavailable）"
                    .to_string(),
            );
            Some("db_unavailable".to_string())
        }
        LensCorrection::Off => {
            if let Some(lens) = &meta.lens {
                warnings.push(format!(
                    "已识别镜头 {lens}，镜头校正未启用（lens_correction=off，D5 定版缺省）"
                ));
            }
            Some("off".to_string())
        }
    };
    let ctx = record_context(decoded);
    let record = assemble_record(
        frame,
        &options.develop,
        &ctx,
        multipliers,
        baseline,
        composite.alpha,
        lens_match,
    );
    Ok(ImportPlan {
        record,
        meta,
        warnings,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_match_d5_and_design_table() {
        let parsed = parse_raw_options(None).unwrap();
        assert_eq!(parsed.develop.wb, WbMode::AsShot);
        assert_eq!(parsed.develop.demosaic, Demosaic::MalvarHeCutler);
        assert_eq!(parsed.develop.pre_exposure_ev, 0.0);
        assert_eq!(parsed.lens_correction, LensCorrection::Off);
        assert_eq!(parsed.lens_override, None);
    }

    #[test]
    fn illegal_values_are_loud_and_specific() {
        for (options, needle) in [
            (json!({"wb": "daylight"}), "wb"),
            (json!({"demosaic": "ahd"}), "demosaic"),
            (json!({"pre_exposure_ev": 4.5}), "pre_exposure_ev"),
            (json!({"pre_exposure_ev": "1"}), "pre_exposure_ev"),
            (json!({"lens_correction": "on"}), "lens_correction"),
            (json!({"lens_overrid": "x"}), "lens_overrid"),
        ] {
            let error = parse_raw_options(Some(&options)).unwrap_err();
            assert!(
                error.to_string().contains(needle),
                "错误要指明字段 {needle}：{error}"
            );
        }
    }
}
