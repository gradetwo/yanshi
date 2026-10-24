//! 导入计划（设计 5.6／5.7，第二包；镜头与高光档位第四/五包）：
//! 把「一份 RAW 字节 + 调用方给的 `raw_options`」变成导入期需要的
//! 全部产物——落账用的 `raw_develop` 记录、回显用的 `raw_meta`、
//! 镜头校正计划、以及必须让调用方看见的 warnings。
//!
//! 为什么在本 crate 组装：record 与 meta 的字段语义归 yanshi-raw（设计 9.3），
//! server 只做不透明透传与落账；两份产物同源于同一次解码，禁止调用方再
//! 解析一遍字节（设计 5.7「禁止两处各解析」）。
//!
//! D5 口径（2026-10-22 定版，第四包沿用）：`lens_correction` 缺省
//! `"off"`（不动像素、与前三包逐位一致）。第四包起镜头库已接入：
//! `"auto"` 走「文件自带校正逐项优先 + Lensfun 匹配」解析（见
//! `resolve_lens`），库整体不可用才落 `db_unavailable`；
//! `lens_override` 接真实现——点名库中不存在的镜头仍响亮报错
//! （手动指定是调用方的明确意图，静默忽略等于骗他）。
//!
//! 高光档位（第五包）：`highlight_recovery` 缺省 `"chroma"`——新导入
//! 走真色度恢复并把档位落进 record；`"floor"` 回到第三包保底机制。
//! 旧对象无此字段、按 Floor 重展开（见 frame 的字段口径）。

use serde_json::Value;

use crate::decode::{raw_meta, record_context};
use crate::develop::{
    as_shot_neutral, assemble_record, composite_color_matrix, renormalized_wb_multipliers,
};
use crate::error::{RawError, Result};
use crate::frame::{
    AppliedLensProfile, DecodedRaw, Demosaic, DevelopOptions, HighlightMode, LensCorrectionPlan,
    RawDevelopRecord, RawMeta, WbMode,
};
use crate::lens;
use crate::lens_db::{self, LensDb};

/// `raw_options` 解析后的镜头校正意图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LensCorrection {
    /// D5 定版缺省：不动像素。
    Off,
    /// 自动：文件自带校正逐项优先 + Lensfun 库匹配补齐。
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
        // 导入缺省与 DevelopOptions::default 的区别只在高光档位：
        // 新导入缺省 chroma（第五包），而 develop() 的库级缺省保持
        // Floor——旧记录/旧调用方的像素口径不许被缺省值偷偷改掉。
        let develop = DevelopOptions {
            highlight: HighlightMode::Chroma,
            ..DevelopOptions::default()
        };
        Self {
            develop,
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
    /// 本次导入解析出的镜头校正计划（record 里 applied profile 的
    /// 孪生体）：导入期若要立即出图，用它调 `develop_with_lens`；
    /// 取图期则凭 record 的 profile 重建（`lens::plan_from_applied`），
    /// 两条路同源同值。
    pub lens_plan: Option<LensCorrectionPlan>,
}

/// `raw_options` 的合法键全集（多一个键即报错——拼写错误不许静默吞掉）。
const RAW_OPTION_KEYS: [&str; 7] = [
    "wb",
    "demosaic",
    "pre_exposure_ev",
    "lens_correction",
    "lens_override",
    "highlight_recovery",
    "preview_max_edge",
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
            "raw_options 必须是对象 {wb, demosaic, pre_exposure_ev, lens_correction, lens_override, highlight_recovery, preview_max_edge}"
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
    // 预览档（C1-b，期 2）：导入即按预览档落账时用——record 会记
    // developed_max_edge，此后每次取图都按预览档重展开（要成品
    // 须以全档重新导入/改档，不许拿预览记录冒充全档）。
    if let Some(edge) = object.get("preview_max_edge") {
        let Some(edge) = edge.as_u64() else {
            return Err(RawError::InvalidOption(format!(
                "raw_options.preview_max_edge 必须是整数，实际为 {edge}"
            )));
        };
        if !(64..=8192).contains(&edge) {
            return Err(RawError::InvalidOption(format!(
                "raw_options.preview_max_edge={edge} 越界（允许 64–8192）"
            )));
        }
        parsed.develop.max_output_edge = Some(edge as u32);
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
    // **显式写 off 又给 override 是互相矛盾的输入 ⇒ 响亮报错**（第 1440 轮审计）。
    // 旧行为：`resolve_lens` 的 off 早退条件是「off **且**没给 override」⇒ override
    // 一在场就跳过 off，照样查库、照样改像素 ⇒ 用户写下的 off 被静默吃掉。
    //
    // ⚠ 只判**显式写过**的 `off`：`lens_correction` 缺省就是 `Off`（D5 定版），
    // 而「只给 `lens_override`」正是第四包的手动点名路径（`lens_pkg4.rs:208` 断言
    // 它必须成功）⇒ 按缺省值判会把合法输入一起拒掉（本仓「对方断言的取值集合
    // 要和我们对齐」那条）。
    let explicit_off = object.get("lens_correction").and_then(|v| v.as_str()) == Some("off");
    if explicit_off && parsed.lens_override.is_some() {
        return Err(RawError::InvalidOption(format!(
            "raw_options 互相矛盾：显式写了 lens_correction=\"off\" 又给了 \
             lens_override=\"{}\"——要做校正请写 \"auto\"（或不写 lens_correction），\
             不许用 override 把显式的 off 暗中打开（第 1440 轮）",
            parsed.lens_override.as_deref().unwrap_or("")
        )));
    }
    if let Some(highlight) = object.get("highlight_recovery") {
        parsed.develop.highlight = match highlight.as_str() {
            Some("chroma") => HighlightMode::Chroma,
            Some("floor") => HighlightMode::Floor,
            _ => {
                return Err(RawError::InvalidOption(format!(
                "raw_options.highlight_recovery 非法：{highlight}（可选 \"chroma\" | \"floor\"）"
            )))
            }
        };
    }
    Ok(parsed)
}

/// 由解码产物 + 已解析参数组装导入计划。
///
/// record 的生效量（重归一化 WB 乘子／色彩链 α／Baseline 缺省）与 `develop`
/// 走同一批函数现算，保证「导入期落账的 record」与「取图期按 record 重展开」
/// 逐字段一致（判据在集成测试里对账）。
pub fn plan_import(decoded: &DecodedRaw, raw_options: Option<&Value>) -> Result<ImportPlan> {
    plan_import_with_db(decoded, raw_options, None)
}

/// 同 `plan_import`，但镜头库可由调用方注入（测试与外挂部署用）：
/// `db = None` 时用进程生效库（打包子集 + `YANSHI_LENSFUN_DIR` 外挂）。
pub fn plan_import_with_db(
    decoded: &DecodedRaw,
    raw_options: Option<&Value>,
    db: Option<&LensDb>,
) -> Result<ImportPlan> {
    let options = parse_raw_options(raw_options)?;
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
    // 镜头解析（第四包）：off 只落痕；auto/override 走完整解析。
    let (lens_match, applied, lens_plan) = resolve_lens(
        decoded,
        &meta,
        &options,
        db,
        frame.width,
        frame.height,
        &mut warnings,
    )?;
    let ctx = record_context(decoded);
    let record = assemble_record(
        frame,
        &options.develop,
        &ctx,
        multipliers,
        baseline,
        composite.alpha,
        lens_match,
        applied,
    );
    Ok(ImportPlan {
        record,
        meta,
        warnings,
        options,
        lens_plan,
    })
}

/// 镜头解析总入口：返回 (lens_match 标签, 落账 profile, 执行 plan)。
///
/// 优先级与状态机（设计 5.9 定版）：
/// - `lens_override` 在场 ⇒ 手动路径：库里点名查，查无此镜响亮报错；
///   命中后仍以文件自带校正逐项优先（手动只定镜头身份，不覆盖文件
///   已声明的校正——那是相机厂对这支镜头的逐项裁决）。
/// - `auto` ⇒ EXIF 镜头名匹配库；文件自带校正（OpcodeList2）逐项
///   优先、库补其余项；只有文件项、库未命中时同样成 plan，
///   lens_match 记 "matched"、出处记 file_opcode（五态里没有
///   「file」态，像素上确有校正生效时不许记 unmatched 骗人）。
/// - 库整体为空（打包子集解析失败的灾难情形）且无文件项 ⇒
///   db_unavailable + warning，不阻断导入（沿用第二包口径）。
/// - `off` ⇒ 只落痕（识别到镜头时 warning 点名「未启用」）。
#[allow(clippy::too_many_arguments)]
fn resolve_lens(
    decoded: &DecodedRaw,
    meta: &RawMeta,
    options: &ParsedRawOptions,
    db: Option<&LensDb>,
    width: usize,
    height: usize,
    warnings: &mut Vec<String>,
) -> Result<(
    Option<String>,
    Option<AppliedLensProfile>,
    Option<LensCorrectionPlan>,
)> {
    if options.lens_correction == LensCorrection::Off && options.lens_override.is_none() {
        if let Some(lens) = &meta.lens {
            warnings.push(format!(
                "已识别镜头 {lens}，镜头校正未启用（lens_correction=off，D5 定版缺省）"
            ));
        }
        return Ok((Some("off".to_string()), None, None));
    }
    // 生效库：注入优先，否则进程库（子集保底 + 外挂合并）。
    let owned_db;
    let db: &LensDb = match db {
        Some(db) => db,
        None => {
            let (loaded, mut notes) = lens_db::default_db();
            warnings.append(&mut notes);
            owned_db = loaded;
            &owned_db
        }
    };
    let file = decoded.file_lens.as_ref();
    let has_file_items = file
        .map(|f| f.warp_sets.is_some() || f.vignette_k.is_some())
        .unwrap_or(false);
    // 镜头身份：override 手动点名，或 EXIF 自动匹配。
    let manual = options.lens_override.is_some();
    let found = if let Some(name) = &options.lens_override {
        match lens_db::find_lens_by_name(db, name) {
            Some(found) => Some(found),
            None => {
                return Err(RawError::InvalidOption(format!(
                    "raw_options.lens_override 指定了镜头「{name}」，但镜头库\
                     （{source}，{version}，共 {count} 支）中没有这支镜头——\
                     请核对厂商与型号写法，或先把该镜头数据外挂进镜头库\
                     （不静默忽略手动指定，设计 5.8）",
                    source = db.source,
                    version = db.version,
                    count = db.lenses.len()
                )));
            }
        }
    } else {
        let exif = decoded.file_meta.lens_exif.as_ref();
        lens_db::find_lens(
            db,
            exif.and_then(|e| e.make.as_deref()),
            exif.and_then(|e| e.model.as_deref()),
        )
    };
    if found.is_none() && !has_file_items {
        if db.lenses.is_empty() {
            warnings.push(
                "lens_correction=auto：镜头库不可用（打包子集损坏且无外挂库），\
                 本次导入未做镜头校正（lens_match=db_unavailable）"
                    .to_string(),
            );
            return Ok((Some("db_unavailable".to_string()), None, None));
        }
        warnings.push(format!(
            "lens_correction=auto：镜头「{}」未在镜头库（{}，{}）中匹配到，\
             本次导入未做镜头校正（lens_match=unmatched）",
            meta.lens.as_deref().unwrap_or("（EXIF 无镜头名）"),
            db.source,
            db.version
        ));
        return Ok((Some("unmatched".to_string()), None, None));
    }
    // 逐项合并：文件项在场即该项归文件，库只补缺项（防重复校正）。
    let exif = decoded.file_meta.lens_exif.clone().unwrap_or_default();
    let resolution = found.and_then(|(entry, _)| lens::resolve_lensfun(entry, &exif));
    let (entry, via) = found.unzip();
    if found.is_some() && resolution.is_none() {
        warnings.push(
            "已匹配镜头，但 EXIF 缺焦距、无从插值标定参数：镜头库三项本次均未施加\
             （文件自带校正如有仍生效）"
                .to_string(),
        );
    }
    let res = resolution.as_ref();
    let file_warp = file.and_then(|f| {
        f.warp_sets
            .as_ref()
            .map(|sets| (sets.clone(), f.warp_center.unwrap_or([0.5, 0.5])))
    });
    let file_vignette = file.and_then(|f| {
        f.vignette_k
            .map(|k| (k, f.vignette_center.unwrap_or([0.5, 0.5])))
    });
    fn source_of(has_file: bool, has_db: bool) -> &'static str {
        if has_file {
            "file_opcode"
        } else if has_db {
            "lensfun"
        } else {
            "none"
        }
    }
    let distortion_src = source_of(
        file_warp.is_some(),
        res.and_then(|r| r.distortion).is_some(),
    );
    let tca_src = if file_warp.is_some() {
        "file_opcode"
    } else if res.and_then(|r| r.tca).is_some() {
        "lensfun"
    } else {
        "none"
    };
    let vignetting_src = source_of(
        file_vignette.is_some(),
        res.and_then(|r| r.vignetting).is_some(),
    );
    if let Some(r) = res {
        if r.distortion_skipped_non_ptlens && file_warp.is_none() {
            warnings.push(
                "镜头库该镜头的畸变标定不是 PTLens 模型：本包只执行 PTLens，\
                 畸变项已跳过（不误读参数）"
                    .to_string(),
            );
        }
        if file_warp.is_some() && r.distortion.is_some() {
            warnings.push(
                "文件自带 WarpRectilinear 在场：畸变与横向色差逐项以文件为准，\
                 镜头库同项未叠加（防重复校正）"
                    .to_string(),
            );
        }
        if file_vignette.is_some() && r.vignetting.is_some() {
            warnings.push(
                "文件自带 FixVignetteRadial 在场：暗角以文件为准，镜头库同项未叠加\
                 （防重复校正）"
                    .to_string(),
            );
        }
    }
    // 画幅换算 s = C_cal / C_img：两端齐备才算，缺一端按 1 并留痕。
    let (crop_ratio, crop_assumed) = match (entry.and_then(|e| e.cropfactor), decoded.crop_factor) {
        (Some(cal), Some(img)) if img > 0.0 => (cal / img, false),
        _ => (1.0, entry.is_some()),
    };
    if crop_assumed {
        warnings.push(
            "画幅换算缺一端系数（标定画幅或本图画幅不可得），已按 s=1 处理；\
             换算比与假定事实已记入 applied_lens_profile"
                .to_string(),
        );
    }
    let distortion = if file_warp.is_some() {
        None
    } else {
        res.and_then(|r| r.distortion)
    };
    let tca = if file_warp.is_some() {
        None
    } else {
        res.and_then(|r| r.tca)
    };
    let vignetting_pa = if file_vignette.is_some() {
        None
    } else {
        res.and_then(|r| r.vignetting)
    };
    let any_item = file_warp.is_some()
        || file_vignette.is_some()
        || distortion.is_some()
        || tca.is_some()
        || vignetting_pa.is_some();
    if !any_item {
        // 匹配到了镜头（或文件只有不执行项）但三项皆空：如实落账，
        // 不许拿空 profile 冒充校正过。
        warnings.push(
            "镜头校正无任何可执行标定项（库条目无标定/模型不受支持，文件亦无自带校正），\
             本次未施加校正"
                .to_string(),
        );
        return Ok((
            Some(if manual { "manual" } else { "matched" }.to_string()),
            None,
            None,
        ));
    }
    let mut sources = serde_json::json!({
        "distortion": distortion_src,
        "vignetting": vignetting_src,
        "tca": tca_src,
        "aperture_assumed": res.map(|r| r.aperture_assumed).unwrap_or(false),
        "distance_assumed": res.map(|r| r.distance_assumed).unwrap_or(false),
        "crop_assumed": crop_assumed,
    });
    if let Some(e) = entry {
        if e.center.is_some() {
            // <center> 光心偏移本阶段不消费（设计定版）：回显里如实标注。
            sources["center"] = serde_json::json!("ignored");
        }
    }
    let (lens_id, source_db, db_version, matched_via) = match entry {
        Some(e) => (
            e.lens_id(),
            db.source.clone(),
            db.version.clone(),
            if manual {
                "manual".to_string()
            } else {
                via.unwrap_or("exact").to_string()
            },
        ),
        None => (
            meta.lens
                .clone()
                .unwrap_or_else(|| "文件自带校正（DNG OpcodeList2）".to_string()),
            "dng_opcode".to_string(),
            "dng".to_string(),
            "file_opcode".to_string(),
        ),
    };
    let (plan, mut profile) = lens::build_plan_and_profile(
        distortion,
        tca,
        vignetting_pa,
        sources,
        file_warp,
        file_vignette,
        crop_ratio,
        &source_db,
        &db_version,
        &lens_id,
        &matched_via,
        res.map(|r| r.focal_mm_effective).or(exif.focal_mm),
    );
    // 自动裁剪 scale 在导入期按帧尺寸算定落账；develop 取图时用
    // 同一个函数现算，两处同值（集成判据对账）。
    if let Some(warp) = &plan.warp {
        profile.crop_scale_applied = lens::auto_crop_scale(warp, width, height);
    }
    Ok((
        Some(if manual { "manual" } else { "matched" }.to_string()),
        Some(profile),
        Some(plan),
    ))
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
