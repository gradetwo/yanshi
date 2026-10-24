//! **风格预设系统**（设计文档 `docs/design/style-preset-system.md`）。
//!
//! **边界**：本模块只做三件事——① 加载并校验预设库（内置 JSON + `YANSHI_PRESET_DIR`
//! 外挂覆盖）；② 把一个预设按 `strength` / `knobs` **展开**成一串已有的调整/滤镜
//! 操作（纯函数，不碰文档）；③ 四个工具的处理器（`list_presets` / `get_preset` /
//! `apply_preset` / `preview_preset`），其中 `apply_preset` 的落成**只走既有路径**：
//! 新对象走 `write_add_effect`、已存在对象走 `write_update_effect`（Supersede 合并）。
//!
//! **为什么不造新原子**：设计文档第一核对项已查清——core 的 `CreateStyle` 载荷是
//! 文档样式实体（画笔/调色板/纹理），不是照片调整配方，两者不是同一物 ⇒ 不硬塞。
//! 预设对象用**确定性 `object_id`**（`preset_<预设id>_<c|s>_<序号>_<类型>`），
//! 二次应用同 id 命中更新路径 ⇒ 「更新而非叠加」。
//!
//! **strength 语义**：风格段每个参数在数据里声明 `neutral`（中性值）；
//! 实际值 = 中性值 +（配方值 − 中性值）× strength，数组/对象逐分量递归；
//! 没有声明中性值的键是**固定参数**（seed/size/radius/channel 等），strength 不动它。
//! ⇒ **可测性质**：strength = 0 时风格段每个可插值参数恰等于其中性值（见本模块测试）。

use super::*;
use serde::{Deserialize, Serialize};

/// 内置预设库（编译期嵌入；外挂目录同名文件可覆盖/新增，见 [`load_preset_library`]）。
const BUILTIN_PRESETS_JSON: &str = include_str!("../data/style-presets.json");

/// 外挂预设目录环境变量（仿镜头库的 `YANSHI_LENSFUN_DIR` 先例）。
const PRESET_DIR_ENV: &str = "YANSHI_PRESET_DIR";

/// 外挂目录里的预设文件名（与内置同一格式；按 `id` 覆盖或新增）。
const PRESET_FILE_NAME: &str = "style-presets.json";

/// 一条预设操作（修正段或风格段的一笔）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PresetOp {
    /// `"adjustment"` 或 `"filter"`。
    pub kind: String,
    /// 调整/滤镜类型名（必须在内核名单内，加载时校验）。
    #[serde(rename = "type")]
    pub type_name: String,
    /// 配方参数（strength = 1 的值）。
    pub params: Value,
    /// 可插值参数的中性值（键是 `params` 的子集）；不在此表里的键是固定参数。
    #[serde(default)]
    pub neutral: Map<String, Value>,
}

/// 一个语义旋钮的映射目标：风格段第 `op` 笔的 `param` 参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct KnobTarget {
    /// 风格段操作下标（0 起）。
    pub op: usize,
    /// 目标参数键。
    pub param: String,
    /// `[低端值, 高端值]`：旋钮取 min 时得低端、取 max 时得高端，中间线性。
    pub range: Vec<Value>,
}

/// 一个语义旋钮（如「暖度」「颗粒」）：调用方给一个标量，映射到少数具体参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PresetKnob {
    /// 旋钮 id（调用方在 `knobs` 对象里用它）。
    pub id: String,
    /// 显示名。
    pub label: String,
    /// 说明（说清它动什么、慎用什么）。
    pub description: String,
    /// 取值下限。
    pub min: f64,
    /// 取值上限。
    pub max: f64,
    /// 缺省值（只作展示/建议；调用方没给这个旋钮时**配方值原样生效**，不用缺省值覆盖）。
    pub default: f64,
    /// 映射目标（可多个参数同动）。
    pub targets: Vec<KnobTarget>,
}

/// 一套预设的完整定义（数据，不是代码）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct StylePreset {
    /// 唯一 id（ASCII 小写下划线）。
    pub id: String,
    /// 中文名。
    pub name_zh: String,
    /// 英文名。
    pub name_en: String,
    /// 状态：`established` / `fixed_v2` / `needs_review`（如实标注，不装成品）。
    pub status: String,
    /// 题材/风格标签（`list_presets` 按它筛）。
    pub tags: Vec<String>,
    /// 适用题材。
    pub suitable: Vec<String>,
    /// 慎用题材/失败模式。
    pub caution: Vec<String>,
    /// 一句话说明。
    pub description: String,
    /// 实测结论、硬约束与 unsupported 技法（如实标注没有对应能力的部分）。
    pub notes: String,
    /// 修正段：`baseline` 是 `full` 或 `light`，`ops` 不参与 strength 插值。
    pub correction: CorrectionSegment,
    /// 风格段操作（参与 strength 插值与 knobs 覆盖）。
    pub style_ops: Vec<PresetOp>,
    /// 语义旋钮（每套 2 个）。
    #[serde(default)]
    pub knobs: Vec<PresetKnob>,
    /// 来源：`builtin` 或 `external`（加载时回填，不在数据文件里写）。
    #[serde(default)]
    pub source: String,
}

/// 修正段（两段结构的第一段）：先校正到中性基线，再施加风格。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CorrectionSegment {
    /// 基线种类：`full`（全量）或 `light`（轻校正，灰纱批次的教训）。
    pub baseline: String,
    /// 修正操作（原样落成，不插值）。
    pub ops: Vec<PresetOp>,
}

/// 预设库（一个 JSON 文件的形状）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PresetLibrary {
    /// 格式版本。
    pub version: u32,
    /// 数据来源说明。
    pub source: String,
    /// 全部预设。
    pub presets: Vec<StylePreset>,
}

/// 展开后的一笔操作（已带确定性 `object_id`，可直接落成）。
#[derive(Debug, Clone, Serialize)]
pub(super) struct ExpandedOp {
    /// 段：`correction` 或 `style`。
    pub segment: String,
    /// 段内序号（0 起）。
    pub index: usize,
    /// `"adjustment"` 或 `"filter"`。
    pub kind: String,
    /// 类型名。
    #[serde(rename = "type")]
    pub type_name: String,
    /// 展开后的实际参数（已插值/覆盖/钳制）。
    pub params: Value,
    /// 确定性对象 id：`preset_<id>_<c|s>_<序号>_<类型>`。
    pub object_id: String,
}

fn preset_err(detail: String) -> YanshiError {
    YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(detail))
}

/// 加载预设库：内置打底；`YANSHI_PRESET_DIR` 若已设置，读其中同名文件按 id 合并
/// （同 id 覆盖、新 id 追加，来源标 `external`）。
///
/// **不许静默降级**：环境变量已设置但文件缺失/读不了/解析失败 ⇒ 直接报错并说清
/// 缺的是哪个文件；绝不装作没设置过环境变量。
pub(super) fn load_preset_library() -> Result<PresetLibrary> {
    let mut library: PresetLibrary = serde_json::from_str(BUILTIN_PRESETS_JSON)
        .map_err(|e| preset_err(format!("内置预设库解析失败（数据文件损坏）：{e}")))?;
    for preset in &mut library.presets {
        preset.source = "builtin".to_owned();
    }
    if let Ok(dir) = std::env::var(PRESET_DIR_ENV) {
        if !dir.trim().is_empty() {
            let path = std::path::Path::new(&dir).join(PRESET_FILE_NAME);
            if !path.exists() {
                return Err(preset_err(format!(
                    "{PRESET_DIR_ENV} 已设置为「{dir}」，但其中缺少预设文件 {}（不会静默忽略外挂目录）",
                    path.display()
                )));
            }
            let text = std::fs::read_to_string(&path).map_err(|e| {
                preset_err(format!("外挂预设文件 {} 读取失败：{e}", path.display()))
            })?;
            let external: PresetLibrary = serde_json::from_str(&text).map_err(|e| {
                preset_err(format!("外挂预设文件 {} 解析失败：{e}", path.display()))
            })?;
            for mut preset in external.presets {
                preset.source = "external".to_owned();
                if let Some(slot) = library.presets.iter_mut().find(|p| p.id == preset.id) {
                    *slot = preset;
                } else {
                    library.presets.push(preset);
                }
            }
        }
    }
    validate_library(&library)?;
    Ok(library)
}

/// 库级校验：id 唯一、状态合法、操作类型在内核名单内、中性键是参数子集、
/// 旋钮目标存在。任一不过 ⇒ 报错并指明是哪套预设、哪一项。
fn validate_library(library: &PresetLibrary) -> Result<()> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for preset in &library.presets {
        if !seen.insert(preset.id.as_str()) {
            return Err(preset_err(format!("预设 id 重复：「{}」", preset.id)));
        }
        if !["established", "fixed_v2", "needs_review"].contains(&preset.status.as_str()) {
            return Err(preset_err(format!(
                "预设「{}」的 status 非法：「{}」（只能是 established / fixed_v2 / needs_review）",
                preset.id, preset.status
            )));
        }
        if preset.correction.baseline != "full" && preset.correction.baseline != "light" {
            return Err(preset_err(format!(
                "预设「{}」的 correction.baseline 非法：「{}」（只能是 full / light）",
                preset.id, preset.correction.baseline
            )));
        }
        let all_ops = preset.correction.ops.iter().chain(preset.style_ops.iter());
        for op in all_ops {
            let supported = match op.kind.as_str() {
                "adjustment" => yanshi_render::ADJUSTMENT_NAMES.contains(&op.type_name.as_str()),
                "filter" => yanshi_render::FILTER_NAMES.contains(&op.type_name.as_str()),
                other => {
                    return Err(preset_err(format!(
                        "预设「{}」的操作 kind 非法：「{other}」（只能是 adjustment / filter）",
                        preset.id
                    )));
                }
            };
            if !supported {
                return Err(preset_err(format!(
                    "预设「{}」用了内核不支持的{}：「{}」",
                    preset.id, op.kind, op.type_name
                )));
            }
            let Some(params) = op.params.as_object() else {
                return Err(preset_err(format!(
                    "预设「{}」的 {} 操作 params 必须是对象",
                    preset.id, op.type_name
                )));
            };
            for key in op.neutral.keys() {
                if !params.contains_key(key) {
                    return Err(preset_err(format!(
                        "预设「{}」的 {} 操作 neutral 声明了参数里没有的键「{key}」",
                        preset.id, op.type_name
                    )));
                }
            }
        }
        for knob in &preset.knobs {
            if !(knob.min < knob.max) || !(knob.min..=knob.max).contains(&knob.default) {
                return Err(preset_err(format!(
                    "预设「{}」的旋钮「{}」区间非法：min {} / max {} / default {}",
                    preset.id, knob.id, knob.min, knob.max, knob.default
                )));
            }
            for target in &knob.targets {
                let Some(op) = preset.style_ops.get(target.op) else {
                    return Err(preset_err(format!(
                        "预设「{}」的旋钮「{}」指向不存在的风格段操作下标 {}（共 {} 笔）",
                        preset.id,
                        knob.id,
                        target.op,
                        preset.style_ops.len()
                    )));
                };
                if !op
                    .params
                    .as_object()
                    .is_some_and(|m| m.contains_key(&target.param))
                {
                    return Err(preset_err(format!(
                        "预设「{}」的旋钮「{}」指向 {} 操作里没有的参数「{}」",
                        preset.id, knob.id, op.type_name, target.param
                    )));
                }
                if target.range.len() != 2 {
                    return Err(preset_err(format!(
                        "预设「{}」的旋钮「{}」的 range 必须是 [低端, 高端] 两项，得到 {} 项",
                        preset.id,
                        knob.id,
                        target.range.len()
                    )));
                }
            }
        }
    }
    Ok(())
}

/// 在库里按 id 找预设；找不到 ⇒ 报错并列出全部可用 id（说清缺什么）。
pub(super) fn find_preset<'a>(
    library: &'a PresetLibrary,
    preset_id: &str,
) -> Result<&'a StylePreset> {
    library
        .presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or_else(|| {
            preset_err(format!(
                "未知预设 preset_id：「{preset_id}」；可用预设共 {} 套：{}",
                library.presets.len(),
                library
                    .presets
                    .iter()
                    .map(|p| p.id.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            ))
        })
}

/// 递归插值：数字线性；数组逐分量（长度必须一致）；对象逐键（键集必须一致）；
/// 布尔/字符串等非数值只允许两端相等（原样返回），否则报错——不许静默取一端。
fn interpolate(recipe: &Value, neutral: &Value, t: f64, what: &str) -> Result<Value> {
    match (recipe, neutral) {
        (Value::Number(r), Value::Number(n)) => {
            let (r, n) = (r.as_f64().unwrap_or(0.0), n.as_f64().unwrap_or(0.0));
            Ok(json!(n + (r - n) * t))
        }
        (Value::Array(r), Value::Array(n)) => {
            if r.len() != n.len() {
                return Err(preset_err(format!(
                    "{what} 的配方值与中性值长度不一致（{} vs {}），无法逐分量插值",
                    r.len(),
                    n.len()
                )));
            }
            let mut out = Vec::with_capacity(r.len());
            for (i, (rv, nv)) in r.iter().zip(n.iter()).enumerate() {
                out.push(interpolate(rv, nv, t, &format!("{what}[{i}]"))?);
            }
            Ok(Value::Array(out))
        }
        (Value::Object(r), Value::Object(n)) => {
            let mut out = Map::new();
            for (key, rv) in r {
                let Some(nv) = n.get(key) else {
                    return Err(preset_err(format!(
                        "{what} 的中性值缺少键「{key}」，无法插值"
                    )));
                };
                out.insert(
                    key.clone(),
                    interpolate(rv, nv, t, &format!("{what}.{key}"))?,
                );
            }
            Ok(Value::Object(out))
        }
        (r, n) => {
            if r == n {
                Ok(r.clone())
            } else {
                Err(preset_err(format!(
                    "{what} 是非数值参数（配方 {r} vs 中性 {n}），不能用 strength 插值；\
                     请把它移出 neutral（作固定参数）或改配方让两端相等"
                )))
            }
        }
    }
}

/// 旋钮映射的插值（range 两端之间按旋钮归一位置取；形状规则同 [`interpolate`]）。
fn interpolate_range(low: &Value, high: &Value, t: f64, what: &str) -> Result<Value> {
    interpolate(high, low, t, what)
}

/// 把展开后的参数钳制到 `validate_effect` 的允许范围（插值/旋钮都可能越界；
/// 钳制表与 `tools_effect.rs` 的数值检查同口径，只收本系统用到的效果）。
fn clamp_params(kind: &str, type_name: &str, params: &mut Value) {
    let checks: &[(&str, f64, f64)] = match (kind, type_name) {
        (_, "brightness_contrast") => &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)],
        (_, "saturation") => &[("amount", 0.0, 8.0)],
        (_, "vibrance") => &[("amount", 0.0, 2.0)],
        (_, "exposure") => &[("ev", -10.0, 10.0)],
        (_, "white_balance") => &[("temperature", -1.0, 1.0), ("tint", -1.0, 1.0)],
        (_, "shadows_highlights") => &[("shadows", -1.0, 1.0), ("highlights", -1.0, 1.0)],
        (_, "levels") => &[
            ("black", 0.0, 0.99),
            ("white", 0.01, 1.0),
            ("gamma", 0.01, 10.0),
        ],
        (_, "hsl") => &[
            ("hue", -180.0, 180.0),
            ("saturation", 0.0, 4.0),
            ("lightness", -1.0, 1.0),
        ],
        (_, "split_toning") => &[("balance", -1.0, 1.0), ("amount", 0.0, 1.0)],
        (_, "black_and_white") => &[("amount", 0.0, 1.0)],
        (_, "dehaze") | (_, "clarity") | (_, "texture") => &[("amount", -1.0, 1.0)],
        (_, "film_grain") => &[("amount", 0.0, 1.0), ("size", 1.0, 8.0)],
        (_, "noise") => &[("amount", 0.0, 1.0)],
        (_, "vignette") => &[
            ("strength", 0.0, 1.0),
            ("radius", 0.0, 2.0),
            ("softness", 0.02, 2.0),
        ],
        (_, "glow") => &[
            ("threshold", 0.0, 1.0),
            ("radius", 1.0, 64.0),
            ("intensity", 0.0, 4.0),
        ],
        (_, "sharpen") => &[("amount", 0.0, 5.0), ("radius", 1.0, 8.0)],
        _ => &[],
    };
    // 三元组类参数（color_balance / split_toning 的影调档、selective_color 的偏移）
    // 分量范围 [-1, 1]，逐分量钳制。
    if let Some(object) = params.as_object_mut() {
        for (key, low, high) in checks {
            if let Some(value) = object.get(*key).and_then(Value::as_f64) {
                let clamped = value.clamp(*low, *high);
                if clamped != value {
                    object.insert(key.to_string(), json!(clamped));
                }
            }
        }
        for band in ["shadows", "midtones", "highlights"] {
            if let Some(array) = object.get_mut(band).and_then(Value::as_array_mut) {
                for component in array.iter_mut() {
                    if let Some(v) = component.as_f64() {
                        *component = json!(v.clamp(-1.0, 1.0));
                    }
                }
            }
        }
    }
}

/// 把一套预设展开成操作清单（纯函数，不碰文档）。
///
/// - `strength`：有限且在 [0, 1]，否则报错并写出收到的值；
/// - `knobs`：键必须是该预设声明的旋钮 id，值必须是 [min, max] 内的数字，
///   否则报错并写出旋钮名与收到的值；没给的旋钮不覆盖（配方值原样）；
/// - 修正段原样（`include_correction = false` 时整段跳过）；风格段逐参数插值后
///   再施加旋钮覆盖，最后统一钳制。
pub(super) fn expand_preset(
    preset: &StylePreset,
    strength: f64,
    knobs: &Map<String, Value>,
    include_correction: bool,
) -> Result<Vec<ExpandedOp>> {
    if !strength.is_finite() || !(0.0..=1.0).contains(&strength) {
        return Err(preset_err(format!(
            "预设「{}」的 strength 必须是 [0, 1] 内的有限数，得到 {strength}",
            preset.id
        )));
    }
    // 旋钮先全量校验（任一非法整组不展开，与落成的原子性口径一致）。
    for (key, value) in knobs {
        let Some(knob) = preset.knobs.iter().find(|k| &k.id == key) else {
            return Err(preset_err(format!(
                "预设「{}」没有旋钮「{key}」；可用旋钮：{}",
                preset.id,
                preset
                    .knobs
                    .iter()
                    .map(|k| k.id.as_str())
                    .collect::<Vec<_>>()
                    .join(" / ")
            )));
        };
        let Some(v) = value.as_f64() else {
            return Err(preset_err(format!(
                "预设「{}」的旋钮「{key}」必须是数字，得到 {value}",
                preset.id
            )));
        };
        if !v.is_finite() || !(knob.min..=knob.max).contains(&v) {
            return Err(preset_err(format!(
                "预设「{}」的旋钮「{key}」必须在 [{}, {}] 内，得到 {v}",
                preset.id, knob.min, knob.max
            )));
        }
    }

    let mut ops: Vec<ExpandedOp> = Vec::new();
    if include_correction {
        for (index, op) in preset.correction.ops.iter().enumerate() {
            let mut params = op.params.clone();
            clamp_params(&op.kind, &op.type_name, &mut params);
            ops.push(ExpandedOp {
                segment: "correction".to_owned(),
                index,
                kind: op.kind.clone(),
                type_name: op.type_name.clone(),
                params,
                object_id: format!("preset_{}_c_{index:02}_{}", preset.id, op.type_name),
            });
        }
    }
    for (index, op) in preset.style_ops.iter().enumerate() {
        let Some(recipe) = op.params.as_object() else {
            return Err(preset_err(format!(
                "预设「{}」的 {} 操作 params 必须是对象",
                preset.id, op.type_name
            )));
        };
        let mut expanded = Map::new();
        for (key, recipe_value) in recipe {
            let value = if let Some(neutral_value) = op.neutral.get(key) {
                interpolate(
                    recipe_value,
                    neutral_value,
                    strength,
                    &format!("预设「{}」{} 的参数 {key}", preset.id, op.type_name),
                )?
            } else if op.type_name == "black_and_white" && key == "amount" {
                // 期 4 D2-2：黑白 op 的 amount 中性值恒为 0（strength=0 ⇒ 恒等），
                // 无论数据里有没有声明 neutral.amount；著录缺省 1.0 时即 strength × 1.0。
                interpolate(
                    recipe_value,
                    &json!(0),
                    strength,
                    &format!("预设「{}」{} 的参数 {key}", preset.id, op.type_name),
                )?
            } else {
                recipe_value.clone() // 固定参数：strength 不动
            };
            expanded.insert(key.clone(), value);
        }
        ops.push(ExpandedOp {
            segment: "style".to_owned(),
            index,
            kind: op.kind.clone(),
            type_name: op.type_name.clone(),
            params: Value::Object(expanded),
            object_id: format!("preset_{}_s_{index:02}_{}", preset.id, op.type_name),
        });
    }
    // 旋钮覆盖（只动风格段；覆盖后仍要钳制）。
    for (key, value) in knobs {
        let knob = preset
            .knobs
            .iter()
            .find(|k| &k.id == key)
            .expect("旋钮已在上方校验过");
        let v = value.as_f64().expect("旋钮值已在上方校验过");
        let t = (v - knob.min) / (knob.max - knob.min);
        for target in &knob.targets {
            let op = ops
                .iter_mut()
                .find(|o| o.segment == "style" && o.index == target.op)
                .expect("旋钮目标已在库校验时确认存在");
            let mapped = interpolate_range(
                &target.range[0],
                &target.range[1],
                t,
                &format!(
                    "预设「{}」旋钮「{}」的目标参数 {}",
                    preset.id, knob.id, target.param
                ),
            )?;
            if let Some(object) = op.params.as_object_mut() {
                object.insert(target.param.clone(), mapped);
            }
        }
    }
    for op in &mut ops {
        if op.segment == "style" {
            clamp_params(&op.kind, &op.type_name, &mut op.params);
        }
    }
    Ok(ops)
}

/// 解析 `apply_preset` / `preview_preset` 的公共参数（strength / knobs / include_correction）。
fn parse_expand_args(
    preset: &StylePreset,
    args: &Value,
) -> Result<(f64, Map<String, Value>, bool)> {
    let strength = match args.get("strength") {
        None => 1.0,
        Some(v) => v.as_f64().ok_or_else(|| {
            preset_err(format!(
                "预设「{}」的 strength 必须是数字，得到 {v}",
                preset.id
            ))
        })?,
    };
    let knobs = match args.get("knobs") {
        None => Map::new(),
        Some(Value::Object(map)) => map.clone(),
        Some(other) => {
            return Err(preset_err(format!(
                "预设「{}」的 knobs 必须是对象（旋钮 id → 数值），得到 {other}",
                preset.id
            )));
        }
    };
    let include_correction = match args.get("include_correction") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(other) => {
            return Err(preset_err(format!(
                "预设「{}」的 include_correction 必须是布尔，得到 {other}",
                preset.id
            )));
        }
    };
    Ok((strength, knobs, include_correction))
}

/// 预设的对外摘要（`list_presets` 用；不含完整配方，配方走 `get_preset`）。
fn preset_summary(preset: &StylePreset) -> Value {
    json!({
        "preset_id": preset.id,
        "name_zh": preset.name_zh,
        "name_en": preset.name_en,
        "status": preset.status,
        "tags": preset.tags,
        "suitable": preset.suitable,
        "caution": preset.caution,
        "description": preset.description,
        "correction_baseline": preset.correction.baseline,
        "correction_op_count": preset.correction.ops.len(),
        "style_op_count": preset.style_ops.len(),
        "knobs": preset.knobs.iter().map(|k| json!({
            "id": k.id, "label": k.label, "min": k.min, "max": k.max, "default": k.default,
        })).collect::<Vec<_>>(),
        "source": preset.source,
    })
}

/// `list_presets`：列预设摘要，可按 `tag`（标签）/ `subject`（适用题材子串）/ `status` 筛。
pub(super) fn read_list_presets(_ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let library = load_preset_library()?;
    let tag = optional_str(args, "tag");
    let subject = optional_str(args, "subject");
    let status = optional_str(args, "status");
    let presets: Vec<Value> = library
        .presets
        .iter()
        .filter(|p| tag.as_deref().is_none_or(|t| p.tags.iter().any(|x| x == t)))
        .filter(|p| {
            subject
                .as_deref()
                .is_none_or(|s| p.suitable.iter().any(|x| x.contains(s)))
        })
        .filter(|p| status.as_deref().is_none_or(|s| p.status == s))
        .map(preset_summary)
        .collect();
    Ok(json!({
        "presets": presets,
        "count": presets.len(),
        "library_version": library.version,
        "library_source": library.source,
    }))
}

/// `get_preset`：返回一套预设的完整定义（含配方与 notes）及 strength=1 的展开预览。
pub(super) fn read_get_preset(_ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let library = load_preset_library()?;
    let preset_id = require_str(args, "preset_id")?;
    let preset = find_preset(&library, &preset_id)?;
    let expanded = expand_preset(preset, 1.0, &Map::new(), true)?;
    Ok(json!({
        "preset": serde_json::to_value(preset).unwrap_or(Value::Null),
        "expanded_default": expanded,
    }))
}

/// 代理分辨率下效果与成品不同的操作（期 4 D2-1）：像素半径类
///（clarity/texture）、邻域阈值类（glow）。逐像素颜色操作与几何归一化
/// 的 vignette 不受分辨率影响，不在此列。规格点名的 grain 在本仓内核
/// 操作名单里不存在（filter.rs 无此名），故不列入——grain 操作若日后
/// 落地，须同步补进本表，否则 scale_notes 会漏报。
const PREVIEW_SCALE_SENSITIVE_OPS: [&str; 3] = ["clarity", "texture", "glow"];

/// `preview_preset` 的 `render` 参数（缺省 max_edge 512、format jpeg）。
struct PreviewRender {
    /// 代理图长边上限（128–2048）。
    max_edge: u32,
    /// 输出编码。
    format: PreviewFormat,
}

/// 预览渲染的输出编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewFormat {
    /// PNG（无损；与 apply 后渲染逐像素比对时用它）。
    Png,
    /// JPEG（有损；base64 体积约为 PNG 的十分之一，默认）。
    Jpeg,
}

/// 解析 `render` 参数：缺席 ⇒ 不渲染（None，老行为逐字不变）；
/// 在场 ⇒ 严格校验（未知键/非法 max_edge/format 一律响亮拒绝，
/// 不静默取缺省）。
fn parse_preview_render(args: &Value) -> Result<Option<PreviewRender>> {
    let Some(render) = args.get("render") else {
        return Ok(None);
    };
    let Some(map) = render.as_object() else {
        return Err(preset_err(format!(
            "preview_preset 的 render 必须是对象（{{max_edge, format}}），得到 {render}"
        )));
    };
    for key in map.keys() {
        if key != "max_edge" && key != "format" {
            return Err(preset_err(format!(
                "preview_preset 的 render 不认键「{key}」；只认 max_edge / format"
            )));
        }
    }
    let max_edge = match map.get("max_edge") {
        None => 512,
        Some(value) => {
            let as_int = value.as_u64().or_else(|| {
                value.as_f64().and_then(|f| {
                    if f.is_finite() && f.fract() == 0.0 && f >= 0.0 {
                        Some(f as u64)
                    } else {
                        None
                    }
                })
            });
            let Some(edge) = as_int else {
                return Err(preset_err(format!(
                    "preview_preset 的 render.max_edge 必须是整数，得到 {value}"
                )));
            };
            if !(128..=2048).contains(&edge) {
                return Err(preset_err(format!(
                    "preview_preset 的 render.max_edge 必须在 [128, 2048] 内，得到 {edge}"
                )));
            }
            edge as u32
        }
    };
    let format = match map.get("format") {
        None => PreviewFormat::Jpeg,
        Some(Value::String(s)) if s == "png" => PreviewFormat::Png,
        Some(Value::String(s)) if s == "jpeg" => PreviewFormat::Jpeg,
        Some(other) => {
            return Err(preset_err(format!(
                "preview_preset 的 render.format 只收 png / jpeg，得到 {other}"
            )));
        }
    };
    Ok(Some(PreviewRender { max_edge, format }))
}

/// 盒式降采样（纯函数）：长边 ≤ max_edge 时原样返回（逐字节同一份，
/// 保证“原生即 ≤max_edge 的图预览与 apply 逐像素相等”的测试前提）；
/// 超出时按盒平均降采样（每个输出像素 = 覆盖源像素的均值，向零取整）。
/// 输入尺寸为 0 时不做除法，直接原样返回，调用方随后响亮拒绝。
fn box_downsample(pixels: &[u8], width: u32, height: u32, max_edge: u32) -> (Vec<u8>, u32, u32) {
    if width == 0 || height == 0 {
        return (Vec::new(), width, height);
    }
    let longest = width.max(height);
    if longest <= max_edge {
        return (pixels.to_vec(), width, height);
    }
    let scale = max_edge as f64 / longest as f64;
    let new_width = ((width as f64 * scale).round() as u32).max(1);
    let new_height = ((height as f64 * scale).round() as u32).max(1);
    let mut out = vec![0u8; (new_width as usize) * (new_height as usize) * 4];
    for y in 0..new_height as usize {
        let y0 = y * height as usize / new_height as usize;
        let y1 = ((y + 1) * height as usize / new_height as usize).max(y0 + 1);
        for x in 0..new_width as usize {
            let x0 = x * width as usize / new_width as usize;
            let x1 = ((x + 1) * width as usize / new_width as usize).max(x0 + 1);
            let mut sum = [0u32; 4];
            let mut count = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let base = (sy * width as usize + sx) * 4;
                    for channel in 0..4 {
                        sum[channel] += pixels[base + channel] as u32;
                    }
                    count += 1;
                }
            }
            let base = (y * new_width as usize + x) * 4;
            for channel in 0..4 {
                out[base + channel] = (sum[channel] / count) as u8;
            }
        }
    }
    (out, new_width, new_height)
}

/// 尺度诚实标注（期 4 D2-1）：展开串里含尺度敏感操作 ⇒ 点名；
/// 不含 ⇒ 空串（测试断“不含时为空”）。
fn preview_scale_notes(ops: &[ExpandedOp]) -> String {
    let mut found: Vec<&str> = Vec::new();
    for op in ops {
        if PREVIEW_SCALE_SENSITIVE_OPS.contains(&op.type_name.as_str())
            && !found.contains(&op.type_name.as_str())
        {
            found.push(op.type_name.as_str());
        }
    }
    if found.is_empty() {
        String::new()
    } else {
        format!(
            "代理图按长边 ≤ max_edge 降采样后渲染：{} 在代理分辨率下效果与成品不同（像素半径/邻域阈值随分辨率变化），逐像素颜色操作与几何归一化的 vignette 不受影响。",
            found.join("、")
        )
    }
}

/// JPEG 编码（期 4 D2-1）：本仓此前只有 JPEG 解码、无编码器，预览
/// 的默认格式需要它（base64 体积约为 PNG 的十分之一）。RGBA 对白底
/// 合成后转 RGB（JPEG 无 alpha 通道）。
fn encode_jpeg(width: u32, height: u32, rgba8: &[u8]) -> Result<Vec<u8>> {
    let mut rgb = Vec::with_capacity((width as usize) * (height as usize) * 3);
    for pixel in rgba8.chunks_exact(4) {
        let alpha = pixel[3] as u32;
        for value in &pixel[..3] {
            rgb.push(((*value as u32 * alpha + 255 * (255 - alpha)) / 255) as u8);
        }
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85)
        .encode(&rgb, width, height, image::ColorType::Rgb8.into())
        .map_err(|error| preset_err(format!("JPEG 编码失败：{error}")))?;
    Ok(buf.into_inner())
}

/// 盒式降采样后的代理图（像素 + 尺寸成组传，避免渲染函数参数超限）。
struct ProxyImage<'a> {
    /// 代理像素（显示编码 RGBA8）。
    pixels: &'a [u8],
    /// 代理宽。
    width: u32,
    /// 代理高。
    height: u32,
}

/// `preview_preset` 的渲染路径（期 4 D2-1）：取源图层整幅像素 → 盒式
/// 降采样 → 临时文档（画布 = 代理尺寸、背景 = 源文档背景）→ 落代理图
/// → 用与 `apply_preset` 同一份落成循环（`land_expanded_ops`）落展开
/// 串 → 整幅渲染 → 按 format 编码 → base64 回带。临时文档的删除由
/// 调用方（`render_preview_image`）在所有出口执行，本函数只负责建
/// 与渲染。
fn render_preview_in_temp_doc(
    ctx: &mut ToolContext<'_>,
    preset: &StylePreset,
    ops: &[ExpandedOp],
    temp_id: &str,
    proxy: &ProxyImage<'_>,
    render: &PreviewRender,
    source_background: Value,
) -> Result<Value> {
    let actor = ctx.actor.clone();
    let session = ctx.session.clone();
    let owner = ctx.owner;
    let role = ctx.role;
    ctx.workspace.create_document(
        crate::NewDocument::new(temp_id, proxy.width, proxy.height)
            .with_background(source_background),
        &actor,
        &session,
    )?;
    let mut temp_ctx =
        ToolContext::new(&mut *ctx.workspace, temp_id, actor, session).with_owner(owner);
    temp_ctx = temp_ctx.with_role(role);
    // 底图：一个图层 + 一张代理位图（与 import_image 同形，直接落原子
    // 以便与后续落成同属临时文档的原子流；复用 D1 的原子 helper）。
    let base_layer_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let hash = temp_ctx.workspace.put_blob(proxy.pixels)?;
    let object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
    let atoms = vec![
        super::tools_retouch::create_layer_atom(
            &temp_ctx,
            &base_layer_id,
            "预览底图",
            None,
            None,
            None,
        ),
        super::tools_retouch::raster_patch_atom(
            &temp_ctx,
            &object_id,
            &base_layer_id,
            &hash,
            proxy.pixels.len(),
            proxy.width,
            proxy.height,
        ),
    ];
    temp_ctx.commit_changeset(atoms, Changeset::new_id())?;
    // 与 apply_preset 同一份落成循环（不是复制品）：临时文档全新，
    // alive 为空 ⇒ 全走 add。
    let alive: BTreeMap<String, &yanshi_core::Object> = BTreeMap::new();
    let landed = land_expanded_ops(&mut temp_ctx, &base_layer_id, &preset.id, ops, &alive)?;
    let bbox = Bbox::new(0.0, 0.0, proxy.width as f64, proxy.height as f64);
    let (width, height, rendered) = temp_ctx.workspace.render_region_raw(temp_id, bbox)?;
    let (bytes, format_name) = match render.format {
        PreviewFormat::Png => (
            yanshi_render::png::encode_png(width, height, &rendered)
                .ok_or_else(|| preset_err("PNG 编码失败：渲染像素无法编码".to_owned()))?,
            "png",
        ),
        PreviewFormat::Jpeg => (encode_jpeg(width, height, &rendered)?, "jpeg"),
    };
    let applied = landed.created.len() + landed.updated.len() + landed.revived.len();
    Ok(json!({
        "image_base64": crate::base64::encode(&bytes),
        "image_width": width,
        "image_height": height,
        "image_format": format_name,
        "ops_applied": applied,
        "scale_notes": preview_scale_notes(ops),
    }))
}

/// 渲染路径的外层：取源像素、降采样、建临时文档、渲染，并在**所有
/// 出口**删除临时文档。删除失败只在成功响应里以 `warnings` 留痕；
/// 错误路径上删除同样会尝试，但错误响应没有 warnings 槽位，删失败
/// 的信息此时无法回带（残留临时文档须手动删除）——如实记录于此。
fn render_preview_image(
    ctx: &mut ToolContext<'_>,
    preset: &StylePreset,
    ops: &[ExpandedOp],
    layer_id: &str,
    render: &PreviewRender,
) -> Result<Value> {
    let state = document_state(ctx)?;
    if state.width == 0 || state.height == 0 {
        return Err(preset_err(format!(
            "源文档画布尺寸为 {}×{}，无法渲染预览",
            state.width, state.height
        )));
    }
    let bbox = Bbox::new(0.0, 0.0, state.width as f64, state.height as f64);
    let (src_width, src_height, source) = ctx
        .workspace
        .render_region_raw_layer(&ctx.doc_id, bbox, layer_id)
        .map_err(|error| {
            if error.code == ErrorCode::ReferenceNotFound {
                preset_err(format!(
                    "源图层 {layer_id} 渲染为空 ⇒ 它是空层或纯调整层，没有可预览的像素：{error}"
                ))
            } else {
                error
            }
        })?;
    let (pixels, width, height) = box_downsample(&source, src_width, src_height, render.max_edge);
    let proxy = ProxyImage {
        pixels: &pixels,
        width,
        height,
    };
    let temp_id = format!("preview_{}", yanshi_core::Ulid::new().encode());
    let outcome = render_preview_in_temp_doc(
        ctx,
        preset,
        ops,
        &temp_id,
        &proxy,
        render,
        state.background.clone(),
    );
    let mut warnings: Vec<String> = Vec::new();
    if let Err(error) = ctx.workspace.delete_document(&temp_id) {
        warnings.push(format!(
            "临时文档 {temp_id} 删除失败：{error}（残留文档需手动删除）"
        ));
    }
    let mut rendered = outcome?;
    rendered["warnings"] = json!(warnings);
    Ok(rendered)
}

/// `preview_preset`：只展开、不落成、不渲染——返回操作清单供 agent 先看再调。
pub(super) fn read_preview_preset(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let library = load_preset_library()?;
    let preset_id = require_str(args, "preset_id")?;
    let preset = find_preset(&library, &preset_id)?;
    let (strength, knobs, include_correction) = parse_expand_args(preset, args)?;
    let ops = expand_preset(preset, strength, &knobs, include_correction)?;
    // 预览也要过一遍真实参数校验：展开结果非法时在这里就报，不等 apply 才发现。
    for op in &ops {
        let kind = effect_kind_of(op)?;
        validate_effect(kind, &op.type_name, &op.params).map_err(|e| {
            preset_err(format!(
                "预设「{}」展开后的 {}（{}）参数非法：{}",
                preset.id,
                op.type_name,
                op.object_id,
                e.context.detail.clone().unwrap_or_default()
            ))
        })?;
    }
    // 不给 render ⇒ 响应与此前逐字一致（老调用方零影响）；给 ⇒ 走临时
    // 文档渲染，渲染字段平铺进响应，notes 不再说“不渲染”。
    let render = parse_preview_render(args)?;
    let mut response = json!({
        "preset_id": preset.id,
        "name_zh": preset.name_zh,
        "status": preset.status,
        "strength": strength,
        "include_correction": include_correction,
        "ops": ops,
        "op_count": ops.len(),
        "notes": "只展开不落成、不渲染；落成请用 apply_preset（同一展开口径）",
        "preset_notes": preset.notes,
    });
    if let Some(render) = render {
        let layer_id = match args.get("layer_id") {
            Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
            _ => {
                return Err(preset_err(
                    "preview_preset 的 render 需要 layer_id（源图层）".to_owned(),
                ));
            }
        };
        let rendered = render_preview_image(ctx, preset, &ops, &layer_id, &render)?;
        for (key, value) in rendered.as_object().expect("渲染回执必须是对象") {
            response[key] = value.clone();
        }
        response["notes"] = json!(
            "只展开不落原文档；渲染走临时文档（已删除），见 image_base64；落成请用 apply_preset（同一展开口径）"
        );
    }
    Ok(response)
}

/// 操作的 kind 字符串 → 既有效果路径的 `EffectKind`。
fn effect_kind_of(op: &ExpandedOp) -> Result<EffectKind> {
    match op.kind.as_str() {
        "adjustment" => Ok(EffectKind::Adjustment),
        "filter" => Ok(EffectKind::Filter),
        other => Err(preset_err(format!(
            "操作 {} 的 kind 非法：「{other}」",
            op.object_id
        ))),
    }
}

/// 已展开操作串的落成结果（期 4 D2-1 抽出，供 `apply_preset` 与
/// `preview_preset` 的渲染路径共用——展开器（`expand_preset`）与
/// 落成循环各只有一份，不许复制第二份）。
pub(super) struct LandedOps {
    /// 新建的对象 id。
    pub created: Vec<String>,
    /// 走更新路径的对象 id。
    pub updated: Vec<String>,
    /// 走期 1 E-b 复活支路的对象 id（不许混进 created 冒充新建）。
    pub revived: Vec<String>,
    /// 最后一笔的工具回执；展开为空时为 None（apply 侧此时 expect 失败，
    /// 与抽出前行为一致；preview 渲染侧不依赖它）。
    pub last: Option<Value>,
}

/// 按序落成一串已展开操作（新走 add、已有走 update、墓碑走 add 的
/// 复活支路）。调用方须先做全量校验（apply 侧的第一阶段）；preview
/// 渲染侧对全新临时文档调用，alive 为空 ⇒ 全走 add。
fn land_expanded_ops(
    ctx: &mut ToolContext<'_>,
    layer_id: &str,
    preset_id: &str,
    ops: &[ExpandedOp],
    alive: &BTreeMap<String, &yanshi_core::Object>,
) -> Result<LandedOps> {
    let mut created: Vec<String> = Vec::new();
    let mut updated: Vec<String> = Vec::new();
    let mut revived: Vec<String> = Vec::new();
    let mut last: Option<Value> = None;
    for op in ops {
        let kind = effect_kind_of(op)?;
        let value = if alive.contains_key(&op.object_id) {
            let result = write_update_effect(
                ctx,
                &json!({
                    "object_id": op.object_id,
                    "params": op.params,
                    "opacity": 1.0,
                }),
                kind,
            )?;
            updated.push(op.object_id.clone());
            result
        } else {
            let result = write_add_effect(
                ctx,
                &json!({
                    "layer_id": layer_id,
                    kind.key(): op.type_name,
                    "params": op.params,
                    "object_id": op.object_id,
                    "name": format!("preset:{preset_id}"),
                    "opacity": 1.0,
                }),
                kind,
            )?;
            // 复活的对象单列点名（期 1 E-b）：不许混进 created 冒充新建。
            if result["revived"] == json!(true) {
                revived.push(op.object_id.clone());
            } else {
                created.push(op.object_id.clone());
            }
            result
        };
        last = Some(value);
    }
    Ok(LandedOps {
        created,
        updated,
        revived,
        last,
    })
}

/// `apply_preset`：展开 → **全量校验**（图层存在、每笔过 `validate_effect`、
/// 已存在的同 id 对象类型/效果名一致）→ 按序落成。
/// 新对象走 `write_add_effect`；已存在的确定性 id 走 `write_update_effect`
/// （Supersede 合并；预设同一笔的参数键集恒定 ⇒ 合并即全量替换该笔参数）。
/// **已删除的同 id 对象（墓碑）走复活路**（期 1 E-b）：一期先查类型/
/// 效果名/图层一致，二期经 `write_add_effect` 的复活支路 Revert 墓碑 +
/// 整组换新，响应以 `revived_object_ids` 点名——二次 apply 不再撞
/// fold 的「已存在」整组失败。
/// 任一校验不过 ⇒ 整组不落成，错误指明是哪套预设、哪一笔、什么错。
pub(super) fn write_apply_preset(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let library = load_preset_library()?;
    let preset_id = require_str(args, "preset_id")?;
    let layer_id = require_str(args, "layer_id")?;
    let preset = find_preset(&library, &preset_id)?;
    let (strength, knobs, include_correction) = parse_expand_args(preset, args)?;
    let ops = expand_preset(preset, strength, &knobs, include_correction)?;

    // ---- 第一阶段：全量校验（这一段一个原子都不提交）----
    let state = document_state(ctx)?;
    if !state.layers.contains_key(&layer_id) {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id} 不存在")),
        )
        .with_layer(layer_id.clone()));
    }
    let alive: BTreeMap<String, &yanshi_core::Object> = state
        .alive_objects()
        .into_iter()
        .map(|object| (object.id.clone(), object))
        .collect();
    // 墓碑对象（期 1 E-b）：确定性 id 撞已删除对象时二期走复活路，
    // 但类型/效果名/图层一致性必须在一期就查完（整组不落成的口径
    // 对复活同样成立）——不一致宁可整组拒绝，不许落一半才发现。
    let tombstoned: BTreeMap<String, &yanshi_core::Object> = state
        .objects
        .values()
        .filter(|object| object.is_deleted())
        .map(|object| (object.id.clone(), object))
        .collect();
    for op in &ops {
        let kind = effect_kind_of(op)?;
        validate_effect(kind, &op.type_name, &op.params).map_err(|e| {
            preset_err(format!(
                "预设「{}」的 {}（{}）参数非法：{}",
                preset.id,
                op.type_name,
                op.object_id,
                e.context.detail.clone().unwrap_or_default()
            ))
        })?;
        if let Some(existing) = tombstoned.get(&op.object_id) {
            if existing.layer_id != layer_id {
                return Err(preset_err(format!(
                    "对象 {} 已被删除且属于图层 {}，与预设要落的图层 {layer_id} 不一致，拒绝复活；请先 restore_object 恢复后移层，或换图层/文档",
                    op.object_id, existing.layer_id
                )));
            }
        }
        if let Some(existing) = alive
            .get(&op.object_id)
            .or_else(|| tombstoned.get(&op.object_id))
        {
            let want_type = match kind {
                EffectKind::Adjustment => ObjectType::Adjustment,
                EffectKind::Filter => ObjectType::Filter,
            };
            if existing.object_type != want_type {
                return Err(preset_err(format!(
                    "对象 {} 已存在但不是{}对象（实际 {:?}），拒绝覆盖；请换图层/文档或先删除该对象",
                    op.object_id,
                    kind.object_type(),
                    existing.object_type
                )));
            }
            let existing_name = existing
                .data
                .get(kind.key())
                .and_then(Value::as_str)
                .unwrap_or_default();
            if existing_name != op.type_name {
                return Err(preset_err(format!(
                    "对象 {} 已存在但效果是「{existing_name}」，与预设「{}」要落的「{}」不一致，拒绝覆盖",
                    op.object_id, preset.id, op.type_name
                )));
            }
        }
    }

    // ---- 第二阶段：按序落成（抽出 land_expanded_ops，与 preview 渲染共用）----
    let landed = land_expanded_ops(ctx, &layer_id, &preset.id, &ops, &alive)?;
    let LandedOps {
        created,
        updated,
        revived,
        last,
    } = landed;
    let mut value = last.expect("展开至少含风格段一笔，不会为空");
    value["preset_id"] = json!(preset.id);
    value["strength"] = json!(strength);
    value["include_correction"] = json!(include_correction);
    value["created_object_ids"] = json!(created);
    value["updated_object_ids"] = json!(updated);
    value["revived_object_ids"] = json!(revived);
    value["revived"] = json!(revived.len());
    value["created"] = json!(created.len());
    value["updated"] = json!(updated.len());
    value["op_count"] = json!(ops.len());
    Ok(value)
}

#[cfg(test)]
mod preset_expander_tests {
    use super::*;

    fn builtin() -> PresetLibrary {
        load_preset_library().expect("内置预设库必须能加载")
    }

    #[test]
    fn builtin_library_has_22_unique_presets() {
        let library = builtin();
        assert_eq!(library.presets.len(), 22, "内置首批预设必须是 22 套");
        let mut ids: Vec<&str> = library.presets.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 22, "预设 id 必须唯一");
        for preset in &library.presets {
            assert_eq!(preset.knobs.len(), 2, "预设 {} 必须有 2 个旋钮", preset.id);
            assert!(
                !preset.style_ops.is_empty(),
                "预设 {} 风格段不能为空",
                preset.id
            );
        }
    }

    #[test]
    fn every_preset_expands_and_passes_validate_effect() {
        let library = builtin();
        for preset in &library.presets {
            for strength in [0.0, 0.5, 1.0] {
                let ops = expand_preset(preset, strength, &Map::new(), true).unwrap_or_else(|e| {
                    panic!("预设 {} strength {strength} 展开失败：{e:?}", preset.id)
                });
                assert_eq!(
                    ops.len(),
                    preset.correction.ops.len() + preset.style_ops.len(),
                    "预设 {} 展开笔数必须等于修正段+风格段",
                    preset.id
                );
                for op in &ops {
                    let kind = effect_kind_of(op).unwrap();
                    validate_effect(kind, &op.type_name, &op.params).unwrap_or_else(|e| {
                        panic!(
                            "预设 {} 的 {}（{}）在 strength {strength} 下参数非法：{e:?}",
                            preset.id, op.type_name, op.object_id
                        )
                    });
                }
            }
        }
    }

    /// 数值感知的深比较：JSON 里 `0` 与 `0.0` 是不同表示但同一个数，
    /// 本系统的恒等式断的是「数」不是「序列化表示」，所以数字按 f64 比、
    /// 数组/对象递归，其余按原值比。（若用 assert_eq，第一版就栽在这里：
    /// 插值产物恒是 f64，而数据文件里中性值写的是整数 0。）
    fn assert_json_number_eq(actual: &Value, expected: &Value, what: &str) {
        match (actual, expected) {
            (Value::Number(a), Value::Number(e)) => {
                assert_eq!(a.as_f64(), e.as_f64(), "{what}：数字必须相等（{a} vs {e}）");
            }
            (Value::Array(a), Value::Array(e)) => {
                assert_eq!(a.len(), e.len(), "{what}：数组长度必须一致");
                for (i, (av, ev)) in a.iter().zip(e.iter()).enumerate() {
                    assert_json_number_eq(av, ev, &format!("{what}[{i}]"));
                }
            }
            (Value::Object(a), Value::Object(e)) => {
                for (key, ev) in e {
                    let av = a.get(key).unwrap_or_else(|| panic!("{what}：缺少键 {key}"));
                    assert_json_number_eq(av, ev, &format!("{what}.{key}"));
                }
            }
            _ => assert_eq!(actual, expected, "{what}"),
        }
    }

    #[test]
    fn strength_zero_style_segment_equals_neutral() {
        let library = builtin();
        for preset in &library.presets {
            let ops = expand_preset(preset, 0.0, &Map::new(), false)
                .unwrap_or_else(|e| panic!("预设 {} strength 0 展开失败：{e:?}", preset.id));
            for (expanded, recipe) in ops.iter().zip(preset.style_ops.iter()) {
                let params = expanded.params.as_object().unwrap();
                for (key, neutral_value) in &recipe.neutral {
                    assert_json_number_eq(
                        params.get(key).expect("展开参数必须含配方全部键"),
                        neutral_value,
                        &format!(
                            "预设 {} 的 {} 参数 {key} 在 strength=0 时必须恰等于中性值",
                            preset.id, recipe.type_name
                        ),
                    );
                }
                // 固定参数（无中性声明）在 strength=0 时必须是配方原值。
                for (key, recipe_value) in recipe.params.as_object().unwrap() {
                    if !recipe.neutral.contains_key(key) {
                        assert_eq!(
                            params.get(key),
                            Some(recipe_value),
                            "预设 {} 的 {} 固定参数 {key} 不许被 strength 动",
                            preset.id,
                            recipe.type_name
                        );
                    }
                }
            }
        }
    }

    /// **黑白 op 的 strength 插值**（期 4 D2-2）：`amount` 的中性值恒为 0，
    /// 即使数据里没有声明 `neutral.amount`；著录 0.8 时 strength 0.5 ⇒ 0.4、
    /// strength 0 ⇒ 0（渲染逐位 = 基底，见 filter 层恒等断言）。
    #[test]
    fn preset_strength_interpolates_bw_amount() {
        let preset: StylePreset = serde_json::from_value(json!({
            "id": "test_bw",
            "name_zh": "测试黑白",
            "name_en": "Test B&W",
            "status": "needs_review",
            "tags": [],
            "suitable": [],
            "caution": [],
            "description": "测试用",
            "notes": "测试用",
            "correction": {"baseline": "full", "ops": []},
            "style_ops": [{
                "kind": "adjustment",
                "type": "black_and_white",
                "params": {"weights": [0.2126, 0.7152, 0.0722], "amount": 0.8},
                "neutral": {}
            }]
        }))
        .expect("测试预设必须能构造");
        let amount_at = |strength: f64| {
            let ops = expand_preset(&preset, strength, &Map::new(), false)
                .unwrap_or_else(|e| panic!("strength {strength} 展开失败：{e:?}"));
            ops[0]
                .params
                .get("amount")
                .and_then(Value::as_f64)
                .expect("展开参数必须含 amount")
        };
        assert!(
            (amount_at(1.0) - 0.8).abs() < 1e-9,
            "strength=1 时 amount 应为著录值 0.8"
        );
        assert!(
            (amount_at(0.5) - 0.4).abs() < 1e-9,
            "strength=0.5 时 amount 应为 0.4"
        );
        assert!(
            (amount_at(0.0) - 0.0).abs() < 1e-9,
            "strength=0 时 amount 应为 0（恒等）"
        );
    }

    #[test]
    fn expand_rejects_bad_strength_and_unknown_knob_with_names() {
        let library = builtin();
        let preset = &library.presets[0];
        let err = expand_preset(preset, 1.5, &Map::new(), true).unwrap_err();
        let detail = err.context.detail.clone().unwrap_or_default();
        assert!(detail.contains("1.5"), "越界报错必须写出收到的值：{detail}");
        let mut knobs = Map::new();
        knobs.insert("no_such_knob".to_owned(), json!(0.5));
        let err = expand_preset(preset, 1.0, &knobs, true).unwrap_err();
        let detail = err.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("no_such_knob"),
            "未知旋钮报错必须写出旋钮名：{detail}"
        );
        // 已知旋钮越界也要报错并写出值。
        let known = preset.knobs[0].id.clone();
        let mut knobs = Map::new();
        knobs.insert(known.clone(), json!(7.0));
        let err = expand_preset(preset, 1.0, &knobs, true).unwrap_err();
        let detail = err.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains(&known) && detail.contains('7'),
            "旋钮越界报错必须写出旋钮名与值：{detail}"
        );
    }

    #[test]
    fn knob_override_changes_only_its_target_param() {
        let library = builtin();
        for preset in &library.presets {
            let knob = &preset.knobs[0];
            let base = expand_preset(preset, 1.0, &Map::new(), true).unwrap();
            let mut knobs = Map::new();
            knobs.insert(knob.id.clone(), json!(knob.max));
            let moved = expand_preset(preset, 1.0, &knobs, true).unwrap();
            assert_eq!(base.len(), moved.len());
            for (b, m) in base.iter().zip(moved.iter()) {
                let is_target = knob
                    .targets
                    .iter()
                    .any(|t| m.segment == "style" && m.index == t.op);
                if !is_target {
                    assert_eq!(
                        b.params, m.params,
                        "预设 {} 旋钮 {} 不许动非目标操作 {}",
                        preset.id, knob.id, m.object_id
                    );
                } else {
                    let target_params: Vec<&str> =
                        knob.targets.iter().map(|t| t.param.as_str()).collect();
                    let bo = b.params.as_object().unwrap();
                    let mo = m.params.as_object().unwrap();
                    for key in bo.keys() {
                        if !target_params.contains(&key.as_str()) {
                            assert_eq!(
                                bo.get(key),
                                mo.get(key),
                                "预设 {} 旋钮 {} 不许动目标操作 {} 的非目标参数 {key}",
                                preset.id,
                                knob.id,
                                m.object_id
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_preset_error_lists_the_missing_id() {
        let library = builtin();
        let err = find_preset(&library, "no_such_preset").unwrap_err();
        let detail = err.context.detail.clone().unwrap_or_default();
        assert!(
            detail.contains("no_such_preset"),
            "未知预设报错必须写出收到的 id：{detail}"
        );
    }

    /// 「抬黑点」类旋钮的方向守卫：fade（小清新）与 matte（电影）必须抬
    /// **曲线首点的输出 y**，旋钮越大首点越高。`levels.black` 是输入黑点，
    /// 加大只会把暗部往黑里压、方向相反——2026-10-23 首版数据曾把这两个
    /// 旋钮错挂在 levels.black 上（描述写「抬量」、实际越拧越黑），本断言
    /// 把方向钉死：谁再把目标改回 levels，这条立刻红。
    #[test]
    fn lift_knobs_raise_curve_first_point_not_levels_black() {
        let library = builtin();
        for (preset_id, knob_id) in [("japanese_airy", "fade"), ("cinema", "matte")] {
            let preset = find_preset(&library, preset_id).unwrap();
            let first_point_y = |knob_value: f64| -> f64 {
                let mut knobs = Map::new();
                knobs.insert(knob_id.to_owned(), json!(knob_value));
                let ops = expand_preset(preset, 1.0, &knobs, false).unwrap();
                let curves = ops
                    .iter()
                    .find(|op| op.segment == "style" && op.type_name == "curves")
                    .unwrap_or_else(|| panic!("预设 {preset_id} 风格段必须有 curves 操作"));
                curves.params["points"][0][1]
                    .as_f64()
                    .expect("曲线首点 y 必须是数字")
            };
            let (low, high) = (first_point_y(0.0), first_point_y(1.0));
            assert!(
                high > low,
                "预设 {preset_id} 旋钮 {knob_id} 必须越拧黑点抬得越高（首点 y：0 档 {low} → 1 档 {high}）"
            );
        }
    }

    /// 盒式降采样：长边 ≤ max_edge 时原样（逐字节同一份）；超出时
    /// 尺寸按长边缩放、2×2 盒均值精确（向零取整）。
    #[test]
    fn box_downsample_identity_and_average() {
        // 2×2 原生 ≤ max_edge ⇒ 原样。
        let pixels = vec![
            10u8, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
        ];
        let (out, w, h) = box_downsample(&pixels, 2, 2, 512);
        assert_eq!((w, h), (2, 2));
        assert_eq!(out, pixels, "长边内不降采样，必须逐字节原样");
        // 4×2（红通道 0/64/128/192 交替）→ 长边 2 ⇒ 2×1，每盒 4 像素均值。
        let mut big = vec![0u8; 4 * 2 * 4];
        for (index, pixel) in big.chunks_exact_mut(4).enumerate() {
            let x = index % 4;
            pixel[0] = (x * 64) as u8;
            pixel[1] = 0;
            pixel[2] = 0;
            pixel[3] = 255;
        }
        let (out, w, h) = box_downsample(&big, 4, 2, 2);
        assert_eq!((w, h), (2, 1));
        assert_eq!(out[0], 32, "左盒红均值 (0+64+0+64)/4 = 32，得到 {}", out[0]);
        assert_eq!(
            out[4], 160,
            "右盒红均值 (128+192+128+192)/4 = 160，得到 {}",
            out[4]
        );
    }

    /// render 参数解析：缺省 max_edge 512 / format jpeg；非法值与未知键
    /// 一律响亮拒绝（不静默取缺省）。
    #[test]
    fn parse_preview_render_defaults_and_rejections() {
        let none = parse_preview_render(&json!({})).unwrap();
        assert!(none.is_none(), "不给 render 必须返回 None（老行为）");
        let render = parse_preview_render(&json!({"render": {}}))
            .unwrap()
            .expect("空 render 对象应取缺省");
        assert_eq!(render.max_edge, 512);
        assert_eq!(render.format, PreviewFormat::Jpeg);
        let render = parse_preview_render(&json!({"render": {"max_edge": 128, "format": "png"}}))
            .unwrap()
            .expect("合法 render 应解析");
        assert_eq!(render.max_edge, 128);
        assert_eq!(render.format, PreviewFormat::Png);
        for bad in [
            json!({"render": {"max_edge": 64}}),
            json!({"render": {"max_edge": 4096}}),
            json!({"render": {"max_edge": 100.5}}),
            json!({"render": {"format": "webp"}}),
            json!({"render": {"quality": 90}}),
            json!({"render": "jpeg"}),
        ] {
            assert!(
                parse_preview_render(&bad).is_err(),
                "非法 render 参数必须拒绝：{bad}"
            );
        }
    }

    /// 尺度标注：含 clarity ⇒ 非空且点名；全是逐像素操作 ⇒ 空串。
    #[test]
    fn preview_scale_notes_names_sensitive_ops() {
        let library = builtin();
        let ccd = library
            .presets
            .iter()
            .find(|p| p.id == "ccd")
            .expect("ccd 预设存在");
        let ops = expand_preset(ccd, 1.0, &Map::new(), true).unwrap();
        assert!(
            ops.iter().any(|op| op.type_name == "clarity"),
            "ccd 展开串应含 clarity（前提）"
        );
        let notes = preview_scale_notes(&ops);
        assert!(
            notes.contains("clarity"),
            "含 clarity 时 scale_notes 必须点名：{notes}"
        );
        // 空分支不用库预设测：22 套库预设全含 clarity/texture/glow
        // 中至少一种（实测），故用合成操作串直接断空分支。
        let clean_ops = [
            ExpandedOp {
                segment: "style".to_owned(),
                index: 0,
                kind: "adjustment".to_owned(),
                type_name: "exposure".to_owned(),
                params: json!({"ev": 0.1}),
                object_id: "synthetic_0".to_owned(),
            },
            ExpandedOp {
                segment: "style".to_owned(),
                index: 1,
                kind: "adjustment".to_owned(),
                type_name: "saturation".to_owned(),
                params: json!({"amount": 1.1}),
                object_id: "synthetic_1".to_owned(),
            },
        ];
        assert_eq!(
            preview_scale_notes(&clean_ops),
            "",
            "不含尺度敏感操作时 scale_notes 必须为空"
        );
    }
}
