//! 期 4 D3：自动模式 `auto_retouch`（Retouch 层工具）。
//!
//! 每个参数的值都由有原理的算法从图像统计里求出；甜点带只作
//! 先验（收缩目标）/钳制/低置信回退，不作估计本身。估计器是纯函数
//! `ImageStats → Params`（另加全分辨率噪声 σ̂ 与 ISO 先验），
//! 可单测、可回测；工具层只负责取数、校验、落成、报告。
//!
//! 响应形状（设计 §5 D3 + 落实时的三处定形，见 phase4-notes.md）：
//! - `applied[]`：**只含实际落成的对象**（每项：group、object_id、
//!   type、params、confidence、rationale）；
//! - `dry_run` 或 `strength = 0` 时 `applied: []`，另给同形的
//!   `planned[]`（收缩后、强度缩放前的估计值——答「算法认为该是
//!   什么」，而非「落了什么」）；
//! - `skipped[]`：每对象一项（未请求的组也逐项列出，原因写明
//!   「未请求（不在 groups 白名单内）」，不许悄悄消失）；
//! - `scene_flags[]` 与 `stats{}`（估计用到的关键统计 + noise/iso）。
//!
//! 多对象组（设计「每组一对象」字面容不下，故定形）：细节组落
//! `auto_clarity`/`auto_texture`/`auto_dehaze` 三个对象，降噪组落
//! `auto_denoise`/`auto_sharpen` 两个对象；其余组单对象。去雾归
//! 细节组（Presence 三件套同组，见 `estimate_detail`）。
//!
//! 对象命名：单对象组「自动基线 · <组名>」，多对象组
//! 「自动基线 · <组名> · <type>」（如「自动基线 · 细节 · clarity」）。
//! 同层重跑原地更新（确定性 id + 期 1 E-b 复活路）；跨层重跑则在
//! 原层原地更新（与 `apply_preset` 的确定性 id 同语义，见工具说明）。
//!
//! 估计取自**未调整的源**，不是图层的当前渲染：工具以
//! `render_layer_objects_subset` 只渲染目标层的位图/RAW 源对象
//! （调整/滤镜对象一律排除在外——含已落成的 auto 对象与人工对象）。
//! 否则重跑会在已修正的画面上再估一次，估计自我抵消、重跑不幂等
//! （实测：合成图上第二跑的饱和度估计落入甜点带，第一跑落成的
//! saturation 对象变残留）。子集渲染背景置空（调用方语义），
//! 不透明照片层上与单层渲染逐像素一致；透明像素不计入统计。
//!
//! 落成复用 D2-1 的 `land_expanded_ops`（展开器与落成循环各一份，
//! 不许复制第二份）：auto 的操作串以 `ExpandedOp` 表达，名字由
//! namer 闭包按 object_id 查表给（apply/preview 仍传 preset 名闭包）。
//! 单 changeset：调用方已打开变更集时并入（不关闭）；否则 auto
//! 自己 `begin_changeset`，成功后关闭，失败时只关闭标记——原子回滚
//! 由注册表事务层负责（与 `apply_preset` 同机制；本函数不手动
//! revert，重复 revert 会二次撤销）。第一阶段已全量校验
//! （`validate_effect` + 更新路径类型一致），落成失败只剩不可预期的
//! workspace 错误。
//!
//! 恒等对象跳过（逐对象规则）：估计值恰为恒等（ev=0、(T, Ti)=(0, 0)、
//! vibrance=0、saturation=1.0、clarity/texture=0、dehaze=0、
//! denoise luminance=0、色阶/影调双中性）时不落该对象——
//! `applied[]` 只含真正改变画面的对象；重跑时某组新跳过不会删掉
//! 旧对象（v1 已知局限，见 phase4-notes.md），需手动删除。

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};
use yanshi_core::{Bbox, ErrorCode, ErrorContext, ObjectType, Result, YanshiError};
use yanshi_render::stats::{image_stats_rgba8, noise_sigma_rgba8, ImageStats, NOISE_KAPPA};

use super::presets::{box_downsample, land_expanded_ops, ExpandedOp, LandedOps};
use super::tools_effect::validate_effect;
use super::{document_state, optional_str, EffectKind, ToolContext};

// ---------------------------------------------------------------------------
// 标定常数（回测前为初值，见 phase4-notes.md「D3 待标定」）
// ---------------------------------------------------------------------------

/// 曝光中调目标（线性中位，初值 0.12；夜景 0.08、雪景 0.18、
/// 舞台 ×0.9 均为结构调整，不是权重收缩）。
const EXPOSURE_T_MID: f32 = 0.12;
/// 曝光高光护栏目标（线性 P99.5，初值 0.95）。
const EXPOSURE_T_CLIP: f32 = 0.95;
/// 曝光 ΔEV 钳制域。
const EXPOSURE_EV_RANGE: (f32, f32) = (-1.5, 2.0);
/// 夜景曝光上钳（保守）。
const EXPOSURE_EV_NIGHT_MAX: f32 = 1.2;
/// 判定「高光已 clip」的 clip_hi 门限（曝光置信低）。
const EXPOSURE_CLIP_LOW_CONF: f32 = 0.08;

/// 色阶黑/白钳制域（线性域）。
const LEVELS_BLACK_RANGE: (f32, f32) = (0.0, 0.06);
/// 色阶白钳制域（线性域）。
const LEVELS_WHITE_RANGE: (f32, f32) = (0.72, 1.0);

/// 影调 shadows 触发点（曝光后显示 p25）与 s_h 双向预算（§9.1）。
const TONE_SHADOW_TRIGGER: f32 = 0.16;
/// s_h 单向预算上限。
const TONE_BUDGET: f32 = 0.3;

/// 白平衡钳制域（内核 temperature/tint 域 [−1, 1] 内的保守域）。
const WB_RANGE: (f32, f32) = (-0.85, 0.85);
/// 白平衡候选过少时的置信低门限（设计三触发之外的扩展，见估计器注）。
const WB_MIN_CANDIDATES: usize = 100;

/// 色彩：vibrance 触发带（S̄ < 0.34 加 vibrance；S̄ > 0.50 减 saturation；
/// 其间为甜点带，不动）。
const COLOR_VIBRANCE_TRIGGER: f32 = 0.34;
/// 色彩：saturation 触发带上沿。
const COLOR_SATURATION_TRIGGER: f32 = 0.50;
/// vibrance 系数与上限；肤色代理高时上限收紧。
const COLOR_VIBRANCE_COEF: f32 = 1.6;
/// vibrance 上限（常规）。
const COLOR_VIBRANCE_MAX: f32 = 0.5;
/// vibrance 上限（肤色代理 > 0.18 时）。
const COLOR_VIBRANCE_SKIN_MAX: f32 = 0.3;
/// 肤色代理门限（vibrance 收紧）。
const COLOR_SKIN_VIBRANCE: f32 = 0.18;
/// 肤色代理门限（细节肖像保护）。
const DETAIL_SKIN_PORTRAIT: f32 = 0.25;

/// 细节：微对比目标 E*（**未标定初值**，回测以三源 E 值 + 金 clarity
/// 反推，见 phase4-notes.md）。
const DETAIL_E_TARGET: f32 = 0.04;
/// 细节：高频比目标 R*（**未标定初值**，同上）。
const DETAIL_R_TARGET: f32 = 0.5;
/// 细节：clarity 钳制域；肖像保护时上沿收紧。
const DETAIL_CLARITY_RANGE: (f32, f32) = (-0.10, 0.30);
/// 细节：clarity 肖像保护上沿。
const DETAIL_CLARITY_PORTRAIT_MAX: f32 = 0.08;
/// 细节：texture 钳制域。
const DETAIL_TEXTURE_RANGE: (f32, f32) = (-0.15, 0.25);
/// 细节：微对比量近零门限（画面近乎平坦 ⇒ clarity/texture 无意义）。
const DETAIL_FLAT_E: f32 = 0.001;

/// 去雾：暗通道均值触发点与系数、上限。
const DEHAZE_TRIGGER: f32 = 0.04;
/// 去雾系数。
const DEHAZE_COEF: f32 = 1.2;
/// 去雾上限。
const DEHAZE_MAX: f32 = 0.45;

/// 降噪：σ̂ → luminance 系数与上限；ISO 先验档位。
const DENOISE_SIGMA_COEF: f32 = 6.0;
/// 降噪 luminance 上限。
const DENOISE_MAX: f32 = 0.8;
/// 降噪 color = luminance × 系数；detail 为模式常数（不随强度缩放）。
const DENOISE_COLOR_COEF: f32 = 0.8;
/// 降噪 detail 模式常数。
const DENOISE_DETAIL: f32 = 0.6;
/// 锐化：amount = clamp(0.9 − σ̂ × 系数，域)；σ̂ 超门限 ⇒ 跳过锐化。
const SHARPEN_BASE: f32 = 0.9;
/// 锐化 σ̂ 系数。
const SHARPEN_SIGMA_COEF: f32 = 2.2;
/// 锐化 amount 域。
const SHARPEN_RANGE: (f32, f32) = (0.25, 0.8);
/// 锐化跳过门限（σ̂ > 此值 ⇒ 锐化只会放大噪声）。
const SHARPEN_SKIP_SIGMA: f32 = 0.28;
/// σ̂ 缺失时的锐化 amount（置信低）。
const SHARPEN_NO_SIGMA: f32 = 0.4;

/// 场景分类门限（设计 §5 D3）。
const SCENE_MONOCHROME_S: f32 = 0.045;
/// 舞台：最大色相桶占比门限。
const SCENE_STAGE_HUE: f32 = 0.45;
/// 舞台/夜景/雪景/逆光的显示 p50 门限（字节/255）。
const SCENE_STAGE_P50: f32 = 0.25;
/// 雪景 p50 门限。
const SCENE_SNOW_P50: f32 = 0.78;
/// 雪景 clip_hi 上限。
const SCENE_SNOW_CLIP: f32 = 0.03;
/// 夜景 p50 门限。
const SCENE_NIGHT_P50: f32 = 0.07;
/// 逆光 p5/p95/p50 门限（字节/255）。
const SCENE_BACKLIT_P5: f32 = 0.05;
/// 逆光 p95 门限。
const SCENE_BACKLIT_P95: f32 = 0.90;
/// 逆光 p50 区间。
const SCENE_BACKLIT_P50: (f32, f32) = (0.3, 0.6);

/// 代理图长边上限（设计 D3：统计在 ≤1024 代理上算；噪声例外，
/// 全分辨率另算，见 `noise_sigma_rgba8` 文档）。
const PROXY_MAX_EDGE: u32 = 1024;

/// 白平衡置信三触发（设计 Q2-c）：Planck 距离中位、Sarle BC、ρ。
const WB_PLANCK_TRIGGER: f32 = 0.12;
/// Sarle BC 双峰门限。
const WB_SARLE_TRIGGER: f32 = 0.555;
/// ρ 下限。
const WB_RHO_TRIGGER: f32 = 0.015;

/// 组白名单（设计固定 7 组；去雾挂在细节组下，不另立组）。
const GROUPS: [&str; 7] = [
    "exposure", "levels", "tone", "wb", "color", "detail", "noise",
];

/// 置信：高 ⇒ 权重 1；低 ⇒ 权重 0.5（设计「减半施加」统一规则）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confidence {
    /// 高置信。
    High,
    /// 低置信。
    Low,
}

impl Confidence {
    /// 收缩权重。
    const fn weight(self) -> f32 {
        match self {
            Confidence::High => 1.0,
            Confidence::Low => 0.5,
        }
    }

    /// 响应里的字符串。
    const fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Low => "low",
        }
    }
}

/// 场景旗标（全部进 `scene_flags`；除单色外一律收缩、不硬跳过）。
#[derive(Debug, Clone, Copy, Default)]
struct SceneFlags {
    /// 单色（S̄ < 0.045）：wb/color 硬跳过的唯一保留情形。
    monochrome: bool,
    /// 舞台（单色相桶 > 0.45 且 p50 < 0.25）。
    stage: bool,
    /// 雪景（p50 > 0.78 且 clip_hi < 3%）。
    snow: bool,
    /// 夜景（p50 < 0.07）。
    night: bool,
    /// 逆光（p5 < 0.05 且 p95 > 0.90 且 p50 ∈ [0.3, 0.6]）。
    backlit: bool,
}

impl SceneFlags {
    /// 响应里的旗标名表（按固定顺序，确定性输出）。
    fn names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        if self.monochrome {
            names.push("monochrome");
        }
        if self.stage {
            names.push("stage");
        }
        if self.snow {
            names.push("snow");
        }
        if self.night {
            names.push("night");
        }
        if self.backlit {
            names.push("backlit");
        }
        names
    }
}

/// 估计输入（统计 + 全分辨率噪声 + ISO 先验）。
struct EstimateInputs<'a> {
    /// 代理图统计。
    stats: &'a ImageStats,
    /// 全分辨率噪声 σ̂（0–1）；图像过小 ⇒ None。
    noise_sigma: Option<f32>,
    /// RAW ISO 先验（非 RAW 或无记录 ⇒ None）。
    iso: Option<u32>,
}

/// 一笔计划对象（收缩后、强度缩放前；工具层缩放后落成）。
struct PlannedObject {
    /// 组名（7 组之一）。
    group: &'static str,
    /// 确定性对象 id（`auto_<group>` 或多对象组的 `auto_<type>`）。
    object_id: String,
    /// 对象中文名（落成时的 `name`）。
    label: String,
    /// `"adjustment"` 或 `"filter"`（`ExpandedOp.kind` 用）。
    kind: &'static str,
    /// 内核类型名。
    type_name: &'static str,
    /// 参数（收缩后、强度缩放前）。
    params: Value,
    /// 置信。
    confidence: Confidence,
    /// 估计依据（必须点名数字与收缩/跳过原因，不许空话）。
    rationale: String,
}

/// 一项跳过（每对象一项；未请求的组也逐项列出）。
struct SkippedEntry {
    /// 组名。
    group: &'static str,
    /// 本会用的对象 id（确定性 id，纵使未落成也点名）。
    object_id: String,
    /// 跳过原因（非空）。
    reason: String,
}

/// 一次估计的完整计划（工具层据此落成或报告）。
struct AutoPlan {
    /// 计划对象（按落成顺序：曝光→色阶→影调→白平衡→色彩→细节→降噪）。
    objects: Vec<PlannedObject>,
    /// 跳过项。
    skipped: Vec<SkippedEntry>,
    /// 场景旗标。
    scene: SceneFlags,
}

// ---------------------------------------------------------------------------
// 纯估计器
// ---------------------------------------------------------------------------

/// 场景分类（纯函数；门限见模块常量）。
fn classify_scene(stats: &ImageStats) -> SceneFlags {
    let p50 = stats.luma_p50 as f32 / 255.0;
    let p5 = stats.luma_p5 as f32 / 255.0;
    let p95 = stats.luma_p95 as f32 / 255.0;
    let max_hue_share = stats
        .hue_histogram
        .iter()
        .fold(0.0f32, |max, share| max.max(*share));
    SceneFlags {
        monochrome: stats.mean_saturation < SCENE_MONOCHROME_S,
        stage: max_hue_share > SCENE_STAGE_HUE && p50 < SCENE_STAGE_P50,
        snow: p50 > SCENE_SNOW_P50 && stats.clip_high_ratio < SCENE_SNOW_CLIP,
        night: p50 < SCENE_NIGHT_P50,
        backlit: p5 < SCENE_BACKLIT_P5
            && p95 > SCENE_BACKLIT_P95
            && p50 >= SCENE_BACKLIT_P50.0
            && p50 <= SCENE_BACKLIT_P50.1,
    }
}

/// 钳制辅助。
fn clamp(value: f32, range: (f32, f32)) -> f32 {
    value.clamp(range.0, range.1)
}

/// 收缩：`w·估计 + (1−w)·先验`（统一规则；先验 = 该参数的中性值）。
fn shrink(estimate: f32, neutral: f32, weight: f32) -> f32 {
    weight * estimate + (1.0 - weight) * neutral
}

/// 曝光估计：线性域闭式 ΔEV = log2(min(T_mid/P50, T_clip/P99.5))。
/// 返回（收缩后 ev、置信、依据、逆光转移量——供影调组用，
/// 非逆光时为 0；转移 = 减半掉的 ev 差 × 1/3，见 `estimate_tone`）。
fn estimate_exposure(stats: &ImageStats, scene: &SceneFlags) -> (f32, Confidence, String, f32) {
    let mut t_mid = EXPOSURE_T_MID;
    let mut target_note = String::new();
    if scene.night {
        t_mid = 0.08;
        target_note = "夜景目标 0.08".to_owned();
    } else if scene.snow {
        t_mid = 0.18;
        target_note = "雪景目标 0.18".to_owned();
    } else if scene.stage {
        t_mid = EXPOSURE_T_MID * 0.9;
        target_note = "舞台目标 ×0.9 保守".to_owned();
    }
    let p50 = stats.linear_p50;
    let p995 = stats.linear_p995;
    let mid_ratio = if p50 > 0.0 {
        t_mid / p50
    } else {
        f32::INFINITY
    };
    let clip_ratio = if p995 > 0.0 {
        EXPOSURE_T_CLIP / p995
    } else {
        f32::INFINITY
    };
    let binding = if mid_ratio <= clip_ratio {
        "中位分支"
    } else {
        "高光护栏分支"
    };
    let mut ev = clamp(mid_ratio.min(clip_ratio).log2(), EXPOSURE_EV_RANGE);
    if scene.night {
        ev = ev.min(EXPOSURE_EV_NIGHT_MAX);
    }
    let confidence = if stats.clip_high_ratio > EXPOSURE_CLIP_LOW_CONF || p50 <= 0.0 || p995 <= 0.0
    {
        Confidence::Low
    } else {
        Confidence::High
    };
    let mut transfer = 0.0f32;
    if scene.backlit {
        let halved = ev * 0.5;
        transfer = (ev - halved) / 3.0;
        ev = halved;
    }
    let shrunk = shrink(ev, 0.0, confidence.weight());
    let rationale = format!(
        "线性 P50={p50:.4}、P99.5={p995:.4}，目标中位 {t_mid}（{target_note}）\
         + 高光护栏 {}，取小者（{}）⇒ ΔEV={ev:.3}；置信 {}（clip_hi={:.3}）\
         ⇒ 权重 {} 收缩后 {shrunk:.3}{}",
        EXPOSURE_T_CLIP,
        binding,
        confidence.as_str(),
        stats.clip_high_ratio,
        confidence.weight(),
        if scene.backlit {
            format!("；逆光：曝光减半，差额 1/3 转影调（+{transfer:.3}）")
        } else {
            String::new()
        },
    );
    (shrunk, confidence, rationale, transfer)
}

/// 色阶估计：曝光缩放后的显示黑/白点（单调变换下百分位可换算，
/// 故直接用线性 p0.2/p99.8 × 2^ev，不必重排样本），再按 §9.1
/// EOTF 映射落线性参数（decode∘encode 抵消 ⇒ 即缩放后线性值）。
/// 返回 None（整组恒等 ⇒ 跳过）或 (black, white, 白点是否被钳到 1.0)。
fn estimate_levels(
    stats: &ImageStats,
    ev: f32,
    confidence: Confidence,
) -> Option<(f32, f32, bool, String)> {
    let scale = 2.0f32.powf(ev);
    let black_raw = scale * stats.linear_p02;
    let white_raw = scale * stats.linear_p998;
    let black = clamp(black_raw, LEVELS_BLACK_RANGE);
    let white = clamp(white_raw, LEVELS_WHITE_RANGE);
    let white_pinned = white_raw >= 0.995;
    let black_neutral = black_raw <= 0.01;
    if black_neutral && white_pinned {
        return None;
    }
    let black_final = if black_neutral { 0.0 } else { black };
    let white_final = if white_pinned { 1.0 } else { white };
    let shrunk_black = shrink(black_final, 0.0, confidence.weight());
    let shrunk_white = shrink(white_final, 1.0, confidence.weight());
    let rationale = format!(
        "曝光缩放 ×{scale:.3} 后线性 p0.2={black_raw:.4}、p99.8={white_raw:.4}，\
         钳制黑 [{:.2}, {:.2}]、白 [{:.2}, {:.2}] ⇒ 黑 {black_final:.4}、白 {white_final:.4}\
         （白点 ≥0.995 时钉 1.0、黑点 ≤0.01 时钉 0；置信 {} 权重 {}）",
        LEVELS_BLACK_RANGE.0,
        LEVELS_BLACK_RANGE.1,
        LEVELS_WHITE_RANGE.0,
        LEVELS_WHITE_RANGE.1,
        confidence.as_str(),
        confidence.weight(),
    );
    Some((shrunk_black, shrunk_white, white_pinned, rationale))
}

/// 影调估计：曝光后显示 p25（同上理：encode(2^ev × 线性 p25)）
/// 与 clip_hi；夜景 highlights 归零；逆光 shadows 叠加转移量。
fn estimate_tone(
    stats: &ImageStats,
    ev: f32,
    backlit_transfer: f32,
    scene: &SceneFlags,
    confidence: Confidence,
) -> (f32, f32, String) {
    use yanshi_render::color::srgb_encode_extended;
    let p25_post = srgb_encode_extended(2.0f32.powf(ev) * stats.linear_p25);
    let mut shadows = clamp(
        (TONE_SHADOW_TRIGGER - p25_post) / TONE_SHADOW_TRIGGER,
        (0.0, 1.0),
    ) * TONE_BUDGET;
    shadows = (shadows + backlit_transfer).min(TONE_BUDGET);
    let mut highlights = if stats.clip_high_ratio > 0.02 {
        -0.3
    } else if stats.clip_high_ratio > 0.005 {
        -0.18
    } else {
        0.0
    };
    if scene.night {
        highlights = 0.0;
    }
    let shrunk_shadows = shrink(shadows, 0.0, confidence.weight());
    let shrunk_highlights = shrink(highlights, 0.0, confidence.weight());
    let rationale = format!(
        "曝光后显示 p25={p25_post:.3}（触发点 {}）⇒ shadows={shadows:.3}\
         （逆光转移 +{backlit_transfer:.3}，预算 ±{}）；clip_hi={:.3} ⇒ highlights={highlights:.3}\
         （夜景归零）；置信 {} 权重 {}",
        TONE_SHADOW_TRIGGER,
        TONE_BUDGET,
        stats.clip_high_ratio,
        confidence.as_str(),
        confidence.weight(),
    );
    (shrunk_shadows, shrunk_highlights, rationale)
}

/// 白平衡增益反推：由目标增益 ĝ 求 (T, Ti)，再做一次线性空间
/// Gauss–Newton 修正（见函数内推导）。
///
/// 内核增益（filter.rs white_balance）：R×(1+0.35T+0.15Ti)、
/// G×(1−0.15Ti)、B×(1−0.35T+0.15Ti)。初值取 G 定 Ti、
/// R−B 差定 T（此二约束在初值处精确成立）；残差
/// rC = ĝC − gC 的最小二乘一步：JᵀJ 对角（T 列与 Ti 列正交：
/// 0.35×0.15 + 0×(−0.15) + (−0.35)×0.15 = 0）⇒
/// ΔT = (rR − rB)/0.7，ΔTi = (rR − rG + rB)/0.45。
/// 三通道 2 自由度本就超定，修正后残差是结构剩余（见测试），
/// 不是公式错误——测试断言修正后增益与目标差 < 0.01。
fn invert_wb_gains(target: [f32; 3]) -> (f32, f32) {
    let (gr, gg, gb) = (target[0], target[1], target[2]);
    let mut temperature = (gr - gb) / 0.7;
    let mut tint = (1.0 - gg) / 0.15;
    let gains = |t: f32, ti: f32| -> [f32; 3] {
        [
            1.0 + 0.35 * t + 0.15 * ti,
            1.0 - 0.15 * ti,
            1.0 - 0.35 * t + 0.15 * ti,
        ]
    };
    let predicted = gains(temperature, tint);
    let residual = [gr - predicted[0], gg - predicted[1], gb - predicted[2]];
    temperature += (residual[0] - residual[2]) / 0.7;
    tint += (residual[0] - residual[1] + residual[2]) / 0.45;
    (clamp(temperature, WB_RANGE), clamp(tint, WB_RANGE))
}

/// 白平衡估计（线性域、掩码后候选的低百分位 p2.5；定位说明：
/// RAW 的机内白平衡已在 develop 时施加，本组估的是**显影后残余
/// 色偏**，不是重做机内白平衡）。
/// 返回 None（低百分位通道为零 ⇒ 增益无定义 ⇒ 跳过）或
/// (temperature, tint, confidence, rationale)。
fn estimate_wb(stats: &ImageStats, scene: &SceneFlags) -> Option<(f32, f32, Confidence, String)> {
    if scene.monochrome {
        return None;
    }
    let mu = stats.wb_channel_p25;
    if mu.iter().any(|value| *value <= 1e-6) {
        return None;
    }
    let geomean = (mu[0] * mu[1] * mu[2]).powf(1.0 / 3.0);
    let gains = [geomean / mu[0], geomean / mu[1], geomean / mu[2]];
    let (temperature, tint) = invert_wb_gains(gains);
    let log_chrom_low = stats.wb_planck_distance_median > WB_PLANCK_TRIGGER
        || stats.wb_sarle_bc > WB_SARLE_TRIGGER
        || stats.wb_rho < WB_RHO_TRIGGER;
    let candidates_low = stats.wb_candidate_count < WB_MIN_CANDIDATES;
    let confidence = if log_chrom_low || candidates_low {
        Confidence::Low
    } else {
        Confidence::High
    };
    let mut weight = confidence.weight();
    let mut scene_note = String::new();
    if scene.stage {
        weight = if log_chrom_low { 0.2 } else { 0.5 };
        scene_note = format!("；舞台场景收缩权重 {weight}");
    }
    let shrunk_t = shrink(temperature, 0.0, weight);
    let shrunk_ti = shrink(tint, 0.0, weight);
    let rationale = format!(
        "显影后残余色偏：掩码后候选 {} 像素（占比 {:.3}），线性低百分位 \
         p2.5=[{:.4}, {:.4}, {:.4}] ⇒ 目标增益 [{:.3}, {:.3}, {:.3}] ⇒ \
         (T, Ti)=({temperature:.3}, {tint:.3})（一次修正后）；置信 {}（Planck 距离中位 {:.3}、\
         Sarle BC {:.3}、ρ {:.4}、候选数 {}）⇒ 权重 {weight} 收缩后 \
         ({shrunk_t:.3}, {shrunk_ti:.3}){scene_note}；RAW 机内白平衡已在显影时施加，\
         本组只纠残余偏色",
        stats.wb_candidate_count,
        stats.wb_candidate_ratio,
        mu[0],
        mu[1],
        mu[2],
        gains[0],
        gains[1],
        gains[2],
        confidence.as_str(),
        stats.wb_planck_distance_median,
        stats.wb_sarle_bc,
        stats.wb_rho,
        stats.wb_candidate_count,
    );
    Some((shrunk_t, shrunk_ti, confidence, rationale))
}

/// 色彩估计：vibrance 优先（S̄ < 0.34 时加 vibrance，saturation 不动；
/// S̄ > 0.50 时减 saturation，vibrance 不动；其间甜点带不动）。
/// 返回 (vibrance, saturation, rationale)；单色时调用方跳过整组。
///
/// **底稿修正（内核语义为准）**：内核 `saturation(amount)` 是乘子
/// （1.0 = 恒等），故底稿「saturation = clamp((0.46−S̄)×0.8, −0.3, 0)」
/// 缺 +1.0——照底稿会给负值，内核域 [0, 8] 虽放行数值但语义是
/// 负乘子钳零 = 整图去灰，错得离谱。此处实现为
/// `1.0 + clamp((0.46−S̄)×0.8, −0.3, 0)` ∈ [0.7, 1.0]。
fn estimate_color(stats: &ImageStats, scene: &SceneFlags) -> (f32, f32, String) {
    let mean_s = stats.mean_saturation;
    let mut vibrance = 0.0f32;
    let mut saturation = 1.0f32;
    let mut branch = "甜点带 [0.34, 0.50]，不动";
    if mean_s < COLOR_VIBRANCE_TRIGGER {
        let max = if stats.skin_proxy_ratio > COLOR_SKIN_VIBRANCE {
            COLOR_VIBRANCE_SKIN_MAX
        } else {
            COLOR_VIBRANCE_MAX
        };
        vibrance = clamp(
            (COLOR_VIBRANCE_TRIGGER - mean_s) * COLOR_VIBRANCE_COEF,
            (0.0, max),
        );
        branch = "S̄ < 0.34 ⇒ vibrance 优先";
    } else if mean_s > COLOR_SATURATION_TRIGGER {
        saturation = 1.0 + clamp((0.46 - mean_s) * 0.8, (-0.3, 0.0));
        branch = "S̄ > 0.50 ⇒ saturation 回落（乘子口径，1.0 = 恒等）";
    }
    let weight = if scene.stage { 0.5 } else { 1.0 };
    let shrunk_v = shrink(vibrance, 0.0, weight);
    let shrunk_s = shrink(saturation, 1.0, weight);
    let rationale = format!(
        "S̄={mean_s:.3}（{branch}；肤色代理 {:.3}）⇒ vibrance={vibrance:.3}、\
         saturation={saturation:.3}；舞台收缩权重 {weight} ⇒ \
         ({shrunk_v:.3}, {shrunk_s:.3})",
        stats.skin_proxy_ratio,
    );
    (shrunk_v, shrunk_s, rationale)
}

/// 细节估计：clarity（微对比 E 对目标 E*）、texture（高频比对目标
/// R*），同式 clamp；去雾（暗通道均值）挂本组。
/// 返回 (clarity, texture, dehaze, rationale)；clarity/texture 为 None
/// 时表示画面近乎平坦 ⇒ 该两项跳过（去雾仍可独立落成）。
fn estimate_detail(
    stats: &ImageStats,
    scene: &SceneFlags,
) -> (Option<f32>, Option<f32>, f32, String) {
    let flat = stats.micro_contrast < DETAIL_FLAT_E;
    let mut clarity = None;
    let mut texture = None;
    if !flat {
        let mut clarity_raw = clamp(
            (DETAIL_E_TARGET - stats.micro_contrast) / DETAIL_E_TARGET * 0.5,
            DETAIL_CLARITY_RANGE,
        );
        let mut texture_raw = clamp(
            (DETAIL_R_TARGET - stats.hf_ratio) / DETAIL_R_TARGET * 0.5,
            DETAIL_TEXTURE_RANGE,
        );
        if stats.skin_proxy_ratio > DETAIL_SKIN_PORTRAIT {
            clarity_raw = clarity_raw.min(DETAIL_CLARITY_PORTRAIT_MAX);
            texture_raw = texture_raw.min(0.0);
        }
        // 细节置信恒低（v1 标定未定 ⇒ 一律减半施加，见设计）。
        clarity = Some(shrink(clarity_raw, 0.0, Confidence::Low.weight()));
        texture = Some(shrink(texture_raw, 0.0, Confidence::Low.weight()));
    }
    let _ = scene;
    let dehaze = if stats.dark_mean <= DEHAZE_TRIGGER {
        0.0
    } else {
        clamp(
            (stats.dark_mean - DEHAZE_TRIGGER) * DEHAZE_COEF,
            (0.0, DEHAZE_MAX),
        )
    };
    let rationale = format!(
        "微对比 E={:.4}（目标 E*={}，未标定初值）⇒ clarity={}；高频比 R={:.3}\
         （目标 R*={}，未标定初值）⇒ texture={}；细节置信恒低 ⇒ 权重 0.5；\
         暗通道均值 {:.3} ⇒ dehaze={:.3}（触发点 {}）{}",
        stats.micro_contrast,
        DETAIL_E_TARGET,
        clarity.map_or("跳过（画面近乎平坦）".to_owned(), |value| format!(
            "{value:.3}"
        )),
        stats.hf_ratio,
        DETAIL_R_TARGET,
        texture.map_or("跳过（画面近乎平坦）".to_owned(), |value| format!(
            "{value:.3}"
        )),
        stats.dark_mean,
        dehaze,
        DEHAZE_TRIGGER,
        if flat {
            "；画面近乎平坦，clarity/texture 跳过"
        } else {
            ""
        },
    );
    (clarity, texture, dehaze, rationale)
}

/// 降噪估计值（luminance 为主量；detail 为模式常数；color 由主量派生）。
struct DenoiseEstimate {
    /// 主量。
    luminance: f32,
    /// 模式常数（不随强度缩放）。
    detail: f32,
    /// 派生量（luminance × 系数）。
    color: f32,
}

/// 锐化估计值。
struct SharpenEstimate {
    /// 锐化量。
    amount: f32,
    /// 半径（模式常数，不随强度缩放）。
    radius: i64,
}

/// 降噪与锐化估计（σ̂ + ISO 先验）。
/// 返回 (denoise, sharpen, rationale)；denoise 为 None ⇒ 降噪对象
/// 跳过（σ̂ 与 ISO 皆无）；sharpen 为 None ⇒ 锐化对象跳过
/// （σ̂ > 0.28 时锐化只会放大噪声）。
fn estimate_noise(
    inputs: &EstimateInputs,
    scene: &SceneFlags,
) -> (Option<DenoiseEstimate>, Option<SharpenEstimate>, String) {
    let sigma = inputs.noise_sigma;
    let mut iso = inputs.iso;
    if scene.night {
        iso = iso.map(|value| value.saturating_mul(2));
    }
    let mut confidence_note = String::new();
    let luminance: Option<f32> = match sigma {
        Some(value) => {
            let mut base = clamp(value * DENOISE_SIGMA_COEF, (0.0, DENOISE_MAX));
            if let Some(iso_value) = iso {
                if iso_value >= 3200 {
                    base = base.max(0.45);
                } else if iso_value >= 1600 {
                    base = base.max(0.32);
                } else if iso_value <= 400 {
                    base = base.min(0.25);
                }
            }
            Some(base)
        }
        None => {
            // σ̂ 缺失 ⇒ ISO 档位本身作估计值（档位值取自设计 tier 的
            // 地板/上限结构，中档与低档为本实现的操作定义，见 notes）。
            confidence_note = "（σ̂ 缺失，ISO 先验估计，置信低）".to_owned();
            match iso {
                Some(value) if value >= 3200 => Some(0.45),
                Some(value) if value >= 1600 => Some(0.32),
                Some(value) if value <= 400 => Some(0.10),
                Some(_) => Some(0.20),
                None => None,
            }
        }
    };
    let denoise = luminance.map(|lum| DenoiseEstimate {
        luminance: lum,
        detail: DENOISE_DETAIL,
        color: (lum * DENOISE_COLOR_COEF).min(1.0),
    });
    let sharpen = match sigma {
        Some(value) if value > SHARPEN_SKIP_SIGMA => None,
        Some(value) => Some(SharpenEstimate {
            amount: clamp(SHARPEN_BASE - value * SHARPEN_SIGMA_COEF, SHARPEN_RANGE),
            radius: 1,
        }),
        None => Some(SharpenEstimate {
            amount: SHARPEN_NO_SIGMA,
            radius: 1,
        }),
    };
    let rationale = format!(
        "σ̂={}、ISO={}（夜景档位 ×2）⇒ 降噪 luminance={}、color=×{}、detail={}（模式常数）；\
         锐化={}{}",
        sigma.map_or("缺失".to_owned(), |value| format!("{value:.4}")),
        iso.map_or("未知".to_owned(), |value| value.to_string()),
        denoise.as_ref().map_or("跳过".to_owned(), |d| format!("{:.3}", d.luminance)),
        DENOISE_COLOR_COEF,
        DENOISE_DETAIL,
        sharpen.as_ref().map_or("跳过（σ̂ > 0.28）".to_owned(), |s| format!(
            "amount {:.3}、radius {}",
            s.amount, s.radius
        )),
        confidence_note,
    );
    (denoise, sharpen, rationale)
}

/// 强度缩放：`中性 + (估计 − 中性) × strength`（denoise detail 与
/// sharpen radius 为模式常数，不缩放——调用方按类型名跳过对应键）。
fn scale_params(type_name: &str, params: &mut Value, strength: f32) {
    let Some(map) = params.as_object_mut() else {
        return;
    };
    let neutral = |key: &str| -> f32 {
        match (type_name, key) {
            ("levels", "white") | ("levels", "gamma") | ("saturation", "amount") => 1.0,
            ("denoise", "detail") | ("sharpen", "radius") => f32::NAN,
            _ => 0.0,
        }
    };
    for (key, value) in map.iter_mut() {
        let Some(number) = value.as_f64() else {
            continue;
        };
        let base = neutral(key);
        if base.is_nan() {
            continue;
        }
        *value = json!((base as f64) + (number - (base as f64)) * (strength as f64));
    }
}

/// 组装完整计划（纯函数；强度缩放不在此做，由工具层落成前做，
/// 以便 dry_run/strength=0 时报告收缩后估计值）。
fn plan_auto(inputs: &EstimateInputs, requested: &[String]) -> AutoPlan {
    let stats = inputs.stats;
    let scene = classify_scene(stats);
    let mut objects: Vec<PlannedObject> = Vec::new();
    let mut skipped: Vec<SkippedEntry> = Vec::new();
    let want = |group: &str| requested.iter().any(|name| name == group);
    let skip_unrequested =
        |group: &'static str, object_id: &str, skipped: &mut Vec<SkippedEntry>| {
            skipped.push(SkippedEntry {
                group,
                object_id: object_id.to_owned(),
                reason: "未请求（不在 groups 白名单内）".to_owned(),
            });
        };

    // 曝光（全组共用置信；色阶/影调继承它，见设计「同批样本」）。
    let (ev, ev_confidence, ev_rationale, transfer) = estimate_exposure(stats, &scene);
    if want("exposure") {
        if ev == 0.0 {
            skipped.push(SkippedEntry {
                group: "exposure",
                object_id: "auto_exposure".to_owned(),
                reason: "曝光估计为 0，已在目标中位".to_owned(),
            });
        } else {
            objects.push(PlannedObject {
                group: "exposure",
                object_id: "auto_exposure".to_owned(),
                label: "自动基线 · 曝光".to_owned(),
                kind: "adjustment",
                type_name: "exposure",
                params: json!({"ev": ev}),
                confidence: ev_confidence,
                rationale: ev_rationale,
            });
        }
    } else {
        skip_unrequested("exposure", "auto_exposure", &mut skipped);
    }

    // 色阶（曝光缩放后百分位；整组恒等 ⇒ 跳过整对象）。
    if want("levels") {
        match estimate_levels(stats, ev, ev_confidence) {
            Some((black, white, _, rationale)) => objects.push(PlannedObject {
                group: "levels",
                object_id: "auto_levels".to_owned(),
                label: "自动基线 · 色阶".to_owned(),
                kind: "adjustment",
                type_name: "levels",
                params: json!({"black": black, "white": white, "gamma": 1.0}),
                confidence: ev_confidence,
                rationale,
            }),
            None => skipped.push(SkippedEntry {
                group: "levels",
                object_id: "auto_levels".to_owned(),
                reason: "黑点 ≤0.01 且白点 ≥0.995，色阶已在中性位".to_owned(),
            }),
        }
    } else {
        skip_unrequested("levels", "auto_levels", &mut skipped);
    }

    // 影调。
    let (shadows, highlights, tone_rationale) =
        estimate_tone(stats, ev, transfer, &scene, ev_confidence);
    if want("tone") {
        if shadows == 0.0 && highlights == 0.0 {
            skipped.push(SkippedEntry {
                group: "tone",
                object_id: "auto_tone".to_owned(),
                reason: "shadows 与 highlights 估计皆为 0，影调已在中性位".to_owned(),
            });
        } else {
            objects.push(PlannedObject {
                group: "tone",
                object_id: "auto_tone".to_owned(),
                label: "自动基线 · 影调".to_owned(),
                kind: "adjustment",
                type_name: "shadows_highlights",
                params: json!({"shadows": shadows, "highlights": highlights}),
                confidence: ev_confidence,
                rationale: tone_rationale,
            });
        }
    } else {
        skip_unrequested("tone", "auto_tone", &mut skipped);
    }

    // 白平衡（单色硬跳过；低百分位为零 ⇒ 增益无定义 ⇒ 跳过；
    // 估计恰为 (0, 0) ⇒ 恒等跳过）。
    if want("wb") {
        match estimate_wb(stats, &scene) {
            Some((temperature, tint, confidence, rationale))
                if temperature != 0.0 || tint != 0.0 =>
            {
                objects.push(PlannedObject {
                    group: "wb",
                    object_id: "auto_white_balance".to_owned(),
                    label: "自动基线 · 白平衡".to_owned(),
                    kind: "adjustment",
                    type_name: "white_balance",
                    params: json!({"temperature": temperature, "tint": tint}),
                    confidence,
                    rationale,
                })
            }
            Some(_) => skipped.push(SkippedEntry {
                group: "wb",
                object_id: "auto_white_balance".to_owned(),
                reason: "白平衡估计为 (0, 0)，已无残余偏色".to_owned(),
            }),
            None => skipped.push(SkippedEntry {
                group: "wb",
                object_id: "auto_white_balance".to_owned(),
                reason: if scene.monochrome {
                    "单色画面，白平衡无物理意义".to_owned()
                } else {
                    "线性低百分位通道为零，增益无定义".to_owned()
                },
            }),
        }
    } else {
        skip_unrequested("wb", "auto_white_balance", &mut skipped);
    }

    // 色彩（单色硬跳过；vibrance 与 saturation 各一对象）。
    if want("color") {
        if scene.monochrome {
            for (object_id, _) in [
                ("auto_vibrance", "vibrance"),
                ("auto_saturation", "saturation"),
            ] {
                skipped.push(SkippedEntry {
                    group: "color",
                    object_id: object_id.to_owned(),
                    reason: "单色画面，色彩调整无物理意义".to_owned(),
                });
            }
        } else {
            let (vibrance, saturation, rationale) = estimate_color(stats, &scene);
            // 逐对象恒等跳过：vibrance 分支时 saturation 恒等 ⇒ 只落
            // vibrance；反之亦然；甜点带时两项皆跳过。
            if vibrance != 0.0 {
                objects.push(PlannedObject {
                    group: "color",
                    object_id: "auto_vibrance".to_owned(),
                    label: "自动基线 · 色彩 · vibrance".to_owned(),
                    kind: "adjustment",
                    type_name: "vibrance",
                    params: json!({"amount": vibrance}),
                    confidence: Confidence::High,
                    rationale: rationale.clone(),
                });
            } else {
                skipped.push(SkippedEntry {
                    group: "color",
                    object_id: "auto_vibrance".to_owned(),
                    reason: "vibrance 估计为 0（甜点带或已达目标）".to_owned(),
                });
            }
            if saturation != 1.0 {
                objects.push(PlannedObject {
                    group: "color",
                    object_id: "auto_saturation".to_owned(),
                    label: "自动基线 · 色彩 · saturation".to_owned(),
                    kind: "adjustment",
                    type_name: "saturation",
                    params: json!({"amount": saturation}),
                    confidence: Confidence::High,
                    rationale,
                });
            } else {
                skipped.push(SkippedEntry {
                    group: "color",
                    object_id: "auto_saturation".to_owned(),
                    reason: "saturation 已在恒等位 1.0".to_owned(),
                });
            }
        }
    } else {
        skip_unrequested("color", "auto_vibrance", &mut skipped);
        skip_unrequested("color", "auto_saturation", &mut skipped);
    }

    // 细节（clarity/texture/dehaze 三对象；平坦时前两项跳过）。
    if want("detail") {
        let (clarity, texture, dehaze, rationale) = estimate_detail(stats, &scene);
        match clarity {
            Some(value) if value != 0.0 => objects.push(PlannedObject {
                group: "detail",
                object_id: "auto_clarity".to_owned(),
                label: "自动基线 · 细节 · clarity".to_owned(),
                kind: "adjustment",
                type_name: "clarity",
                params: json!({"amount": value}),
                confidence: Confidence::Low,
                rationale: rationale.clone(),
            }),
            Some(_) => skipped.push(SkippedEntry {
                group: "detail",
                object_id: "auto_clarity".to_owned(),
                reason: "clarity 估计为 0（微对比量恰在目标）".to_owned(),
            }),
            None => skipped.push(SkippedEntry {
                group: "detail",
                object_id: "auto_clarity".to_owned(),
                reason: "微对比量近零，画面近乎平坦".to_owned(),
            }),
        }
        match texture {
            Some(value) if value != 0.0 => objects.push(PlannedObject {
                group: "detail",
                object_id: "auto_texture".to_owned(),
                label: "自动基线 · 细节 · texture".to_owned(),
                kind: "adjustment",
                type_name: "texture",
                params: json!({"amount": value}),
                confidence: Confidence::Low,
                rationale: rationale.clone(),
            }),
            Some(_) => skipped.push(SkippedEntry {
                group: "detail",
                object_id: "auto_texture".to_owned(),
                reason: "texture 估计为 0（高频比恰在目标）".to_owned(),
            }),
            None => skipped.push(SkippedEntry {
                group: "detail",
                object_id: "auto_texture".to_owned(),
                reason: "微对比量近零，画面近乎平坦".to_owned(),
            }),
        }
        if dehaze > 0.0 {
            objects.push(PlannedObject {
                group: "detail",
                object_id: "auto_dehaze".to_owned(),
                label: "自动基线 · 细节 · dehaze".to_owned(),
                kind: "adjustment",
                type_name: "dehaze",
                params: json!({"amount": dehaze}),
                confidence: Confidence::High,
                rationale,
            });
        } else {
            skipped.push(SkippedEntry {
                group: "detail",
                object_id: "auto_dehaze".to_owned(),
                reason: "暗通道均值 ≤ 0.04，无雾可去".to_owned(),
            });
        }
    } else {
        skip_unrequested("detail", "auto_clarity", &mut skipped);
        skip_unrequested("detail", "auto_texture", &mut skipped);
        skip_unrequested("detail", "auto_dehaze", &mut skipped);
    }

    // 降噪（denoise/sharpen 两对象）。
    if want("noise") {
        let (denoise, sharpen, rationale) = estimate_noise(inputs, &scene);
        match denoise {
            Some(estimate) if estimate.luminance != 0.0 => objects.push(PlannedObject {
                group: "noise",
                object_id: "auto_denoise".to_owned(),
                label: "自动基线 · 降噪 · denoise".to_owned(),
                kind: "adjustment",
                type_name: "denoise",
                params: json!({"luminance": estimate.luminance, "detail": estimate.detail, "color": estimate.color}),
                confidence: Confidence::Low,
                rationale: rationale.clone(),
            }),
            Some(_) => skipped.push(SkippedEntry {
                group: "noise",
                object_id: "auto_denoise".to_owned(),
                reason: "降噪量估计为 0（测得无噪声）".to_owned(),
            }),
            None => skipped.push(SkippedEntry {
                group: "noise",
                object_id: "auto_denoise".to_owned(),
                reason: "σ̂ 与 ISO 先验皆无，无法估计降噪量".to_owned(),
            }),
        }
        match sharpen {
            Some(estimate) => objects.push(PlannedObject {
                group: "noise",
                object_id: "auto_sharpen".to_owned(),
                label: "自动基线 · 降噪 · sharpen".to_owned(),
                kind: "filter",
                type_name: "sharpen",
                params: json!({"amount": estimate.amount, "radius": estimate.radius}),
                confidence: Confidence::Low,
                rationale,
            }),
            None => skipped.push(SkippedEntry {
                group: "noise",
                object_id: "auto_sharpen".to_owned(),
                reason: "σ̂ > 0.28，锐化只会放大噪声".to_owned(),
            }),
        }
    } else {
        skip_unrequested("noise", "auto_denoise", &mut skipped);
        skip_unrequested("noise", "auto_sharpen", &mut skipped);
    }

    AutoPlan {
        objects,
        skipped,
        scene,
    }
}

// ---------------------------------------------------------------------------
// 工具层
// ---------------------------------------------------------------------------

/// 工具错误辅助（InvalidArgument/PreconditionFailed/ReferenceNotFound）。
fn auto_err(code: ErrorCode, detail: String) -> YanshiError {
    YanshiError::new(code, ErrorContext::detail(detail))
}

/// 默认源图层：按 z_index 升序，首个含 ≥1 个 alive `raster_patch`
/// 对象的图层（RAW 导入同样落 `raster_patch`，见 `import_image`）。
fn resolve_default_layer(state: &yanshi_core::DocumentState) -> Result<String> {
    let mut layers: Vec<_> = state.layers.values().collect();
    layers.sort_by_key(|layer| layer.z_index);
    let alive = state.alive_objects();
    for layer in layers {
        let has_bitmap = alive.iter().any(|object| {
            object.layer_id == layer.id && object.object_type == ObjectType::RasterPatch
        });
        if has_bitmap {
            return Ok(layer.id.clone());
        }
    }
    Err(auto_err(
        ErrorCode::PreconditionFailed,
        "文档里没有含位图/RAW 对象的图层，自动模式无源可估".to_owned(),
    ))
}

/// 目标层 ISO 先验：该层 alive `raster_patch` 对象的
/// `data["raw_meta"]["iso"]`（首个可读到的正整数；无 ⇒ None）。
fn resolve_iso(state: &yanshi_core::DocumentState, layer_id: &str) -> Option<u32> {
    for object in state.alive_objects() {
        if object.layer_id != layer_id || object.object_type != ObjectType::RasterPatch {
            continue;
        }
        let iso = object
            .data
            .get("raw_meta")
            .and_then(|meta| meta.get("iso"))
            .and_then(Value::as_u64)?;
        if iso > 0 && iso <= u32::MAX as u64 {
            return Some(iso as u32);
        }
    }
    None
}

/// `auto_retouch`：统计 → 估计 →（校验 →）落成 → 报告。
pub(super) fn write_auto_retouch(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // ---- 参数解析（非法值一律报错，不静默默认）----
    let layer_arg = optional_str(args, "layer_id");
    let strength = args.get("strength").and_then(Value::as_f64).unwrap_or(1.0);
    if !(0.0..=1.0).contains(&strength) {
        return Err(auto_err(
            ErrorCode::InvalidArgument,
            format!("strength 必须在 [0, 1] 内，实得 {strength}"),
        ));
    }
    let groups: Vec<String> = match args.get("groups") {
        None => GROUPS.iter().map(|name| name.to_string()).collect(),
        Some(Value::Array(items)) => {
            let mut names = Vec::new();
            for item in items {
                let Some(name) = item.as_str() else {
                    return Err(auto_err(
                        ErrorCode::InvalidArgument,
                        "groups 必须是字符串数组".to_owned(),
                    ));
                };
                if !GROUPS.contains(&name) {
                    return Err(auto_err(
                        ErrorCode::InvalidArgument,
                        format!("groups 含未知组「{name}」，只认 {}", GROUPS.join(" / ")),
                    ));
                }
                if !names.contains(&name.to_owned()) {
                    names.push(name.to_owned());
                }
            }
            names
        }
        Some(_) => {
            return Err(auto_err(
                ErrorCode::InvalidArgument,
                "groups 必须是字符串数组".to_owned(),
            ));
        }
    };
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let force = args.get("force").and_then(Value::as_bool).unwrap_or(false);

    // ---- 图层解析与 force 检查（落成前全部做完）----
    let state = document_state(ctx)?;
    let layer_id = match &layer_arg {
        Some(id) => {
            if !state.layers.contains_key(id) {
                return Err(auto_err(
                    ErrorCode::ReferenceNotFound,
                    format!("图层 {id} 不存在"),
                ));
            }
            id.clone()
        }
        None => resolve_default_layer(&state)?,
    };
    let layer = state.layers.get(&layer_id).expect("图层已验存在");
    let layer_name = layer.name.clone();
    let alive = state.alive_objects();
    let manual: Vec<String> = alive
        .iter()
        .filter(|object| {
            object.layer_id == layer_id
                && matches!(
                    object.object_type,
                    ObjectType::Adjustment | ObjectType::Filter
                )
                && !object.id.starts_with("auto_")
        })
        .map(|object| object.id.clone())
        .collect();
    if !manual.is_empty() && !force {
        return Err(auto_err(
            ErrorCode::PreconditionFailed,
            format!(
                "图层「{layer_name}」已有非自动调整/滤镜对象（{}），自动模式不会 \
                 在人工调过的基础上叠加；确认要覆盖请传 force=true",
                manual.join("、")
            ),
        ));
    }
    let iso = resolve_iso(&state, &layer_id);
    let (doc_width, doc_height) = (state.width, state.height);
    // 子集渲染的 keep 集：目标层 alive 位图/RAW 源对象（调整/滤镜
    // 对象排除在外，见模块卷首——重跑幂等的关键）。
    let keep: std::collections::BTreeSet<String> = alive
        .iter()
        .filter(|object| {
            object.layer_id == layer_id && object.object_type == ObjectType::RasterPatch
        })
        .map(|object| object.id.clone())
        .collect();
    if keep.is_empty() {
        return Err(auto_err(
            ErrorCode::PreconditionFailed,
            format!("图层「{layer_name}」没有位图/RAW 源对象，自动模式无源可估"),
        ));
    }
    drop(state);

    // ---- 渲染：全幅（噪声用）+ 代理（其余统计用）----
    let bbox = Bbox::new(0.0, 0.0, doc_width as f64, doc_height as f64);
    let (src_width, src_height, full_pixels) = ctx
        .workspace
        .render_layer_objects_subset(&ctx.doc_id, bbox, &layer_id, &keep)
        .map_err(|_| {
            auto_err(
                ErrorCode::PreconditionFailed,
                format!("图层「{layer_name}」源对象渲染失败，自动模式无源可估"),
            )
        })?;
    if src_width == 0 || src_height == 0 || full_pixels.is_empty() {
        return Err(auto_err(
            ErrorCode::PreconditionFailed,
            "源图层渲染尺寸为零，自动模式无源可估".to_owned(),
        ));
    }
    let noise_sigma = noise_sigma_rgba8(&full_pixels, src_width, src_height, NOISE_KAPPA);
    let (proxy_pixels, proxy_width, proxy_height) =
        box_downsample(&full_pixels, src_width, src_height, PROXY_MAX_EDGE);
    let stats = image_stats_rgba8(&proxy_pixels, proxy_width, proxy_height);
    if stats.count == 0 {
        return Err(auto_err(
            ErrorCode::PreconditionFailed,
            "代理图无不透明像素，自动模式无源可估".to_owned(),
        ));
    }

    // ---- 估计（纯函数）----
    let inputs = EstimateInputs {
        stats: &stats,
        noise_sigma,
        iso,
    };
    let plan = plan_auto(&inputs, &groups);

    // ---- 报告素材（applied/planned/skipped 共用形状）----
    let entry_of = |object: &PlannedObject, params: &Value| {
        json!({
            "group": object.group,
            "object_id": object.object_id,
            "type": object.type_name,
            "params": params,
            "confidence": object.confidence.as_str(),
            "rationale": object.rationale,
        })
    };
    let skipped_json: Vec<Value> = plan
        .skipped
        .iter()
        .map(|entry| {
            json!({
                "group": entry.group,
                "object_id": entry.object_id,
                "reason": entry.reason,
            })
        })
        .collect();
    let scene_json: Vec<Value> = plan.scene.names().iter().map(|name| json!(name)).collect();
    let stats_json = json!({
        "luma_p50": stats.luma_p50,
        "linear_p50": stats.linear_p50,
        "linear_p995": stats.linear_p995,
        "clip_high_ratio": stats.clip_high_ratio,
        "mean_saturation": stats.mean_saturation,
        "saturation_p90": stats.saturation_p90,
        "skin_proxy_ratio": stats.skin_proxy_ratio,
        "dark_mean": stats.dark_mean,
        "micro_contrast": stats.micro_contrast,
        "hf_ratio": stats.hf_ratio,
        "noise_sigma": noise_sigma,
        "iso": iso,
        "wb_candidate_ratio": stats.wb_candidate_ratio,
        "wb_planck_distance_median": stats.wb_planck_distance_median,
        "wb_sarle_bc": stats.wb_sarle_bc,
        "wb_rho": stats.wb_rho,
    });

    // ---- 只报告（dry_run / strength=0）：零原子 ----
    if dry_run || strength == 0.0 {
        let planned: Vec<Value> = plan
            .objects
            .iter()
            .map(|object| entry_of(object, &object.params))
            .collect();
        return Ok(json!({
            "layer_id": layer_id,
            "layer_name": layer_name,
            "applied": [],
            "planned": planned,
            "skipped": skipped_json,
            "scene_flags": scene_json,
            "stats": stats_json,
            "note": if dry_run {
                "dry_run：只报告，不落成"
            } else {
                "strength=0：只报告，不落成"
            },
        }));
    }

    // ---- 第一阶段：全量校验（每笔过 validate_effect；更新路径要求
    // 类型一致——与 apply_preset 第一阶段同纪律）----
    let state = document_state(ctx)?;
    let alive_map: BTreeMap<String, &yanshi_core::Object> = state
        .alive_objects()
        .iter()
        .filter(|object| object.layer_id == layer_id)
        .map(|object| (object.id.clone(), *object))
        .collect();
    let mut ops: Vec<ExpandedOp> = Vec::new();
    let mut labels: HashMap<String, String> = HashMap::new();
    for (index, object) in plan.objects.iter().enumerate() {
        let mut params = object.params.clone();
        scale_params(object.type_name, &mut params, strength as f32);
        let kind = match object.kind {
            "adjustment" => EffectKind::Adjustment,
            "filter" => EffectKind::Filter,
            other => {
                return Err(auto_err(
                    ErrorCode::InvalidArgument,
                    format!("内部错误：未知 kind「{other}」"),
                ));
            }
        };
        validate_effect(kind, object.type_name, &params).map_err(|error| {
            auto_err(
                ErrorCode::InvalidArgument,
                format!(
                    "自动估计值未通过校验（{} {}）：{}",
                    object.group, object.type_name, error
                ),
            )
        })?;
        if let Some(existing) = alive_map.get(&object.object_id) {
            let expected = match kind {
                EffectKind::Adjustment => ObjectType::Adjustment,
                EffectKind::Filter => ObjectType::Filter,
            };
            if existing.object_type != expected {
                return Err(auto_err(
                    ErrorCode::InvalidArgument,
                    format!(
                        "对象 {} 已存在但类型不是 {}，自动模式不覆盖异类对象",
                        object.object_id, object.kind
                    ),
                ));
            }
        }
        labels.insert(object.object_id.clone(), object.label.clone());
        ops.push(ExpandedOp {
            segment: "auto".to_owned(),
            index,
            kind: object.kind.to_owned(),
            type_name: object.type_name.to_owned(),
            params,
            object_id: object.object_id.clone(),
        });
    }
    drop(state);

    // ---- 落成（单 changeset；谁打开谁关闭）----
    let opened_here = ctx
        .workspace
        .begin_changeset(&ctx.doc_id, &ctx.session)
        .is_some();
    let landed: Result<LandedOps> = (|| {
        // 落成循环要的是「当前 alive 且在目标层」的映射；第一阶段的
        // 映射不复用（其间状态可能已变），此处重新读。
        let state_now = document_state(ctx)?;
        let alive_refs: BTreeMap<String, &yanshi_core::Object> = state_now
            .alive_objects()
            .iter()
            .filter(|object| object.layer_id == layer_id)
            .map(|object| (object.id.clone(), *object))
            .collect();
        // 注意：alive_refs 借 state_now，而 land_expanded_ops 借 ctx——
        // 两者互不相干；state_now 在调用返回后才释放。
        let result = land_expanded_ops(ctx, &layer_id, &ops, &alive_refs, &|op| {
            labels.get(&op.object_id).cloned().unwrap_or_default()
        });
        drop(state_now);
        result
    })();
    let landed = match landed {
        Ok(landed) => {
            if opened_here {
                ctx.workspace.close_changeset(&ctx.doc_id, &ctx.session);
            }
            landed
        }
        Err(error) => {
            // 只关闭自己打开的标记，不手动 revert：注册表事务层会在
            // 工具返回 Err 后回滚已落原子（与 apply_preset 同机制），
            // 手动 revert 再叠加一次会二次撤销。
            if opened_here {
                ctx.workspace.close_changeset(&ctx.doc_id, &ctx.session);
            }
            return Err(error);
        }
    };

    // ---- 回执（applied 只含落成对象，含更新/复活路径点名）----
    let mut applied: Vec<Value> = Vec::new();
    for object in &plan.objects {
        let mut params = object.params.clone();
        scale_params(object.type_name, &mut params, strength as f32);
        let mut entry = entry_of(object, &params);
        if landed.updated.contains(&object.object_id) {
            entry["updated"] = json!(true);
        }
        if landed.revived.contains(&object.object_id) {
            entry["revived"] = json!(true);
        }
        applied.push(entry);
    }
    Ok(json!({
        "layer_id": layer_id,
        "layer_name": layer_name,
        "applied": applied,
        "created": landed.created,
        "updated": landed.updated,
        "revived": landed.revived,
        "skipped": skipped_json,
        "scene_flags": scene_json,
        "stats": stats_json,
        "note": "自动基线已落成：每组一笔确定性对象（细节/降噪组多对象），同层重跑原地更新；收缩与强度缩放依据见各 applied 项 rationale",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yanshi_render::stats::ImageStats;

    /// 手工 ImageStats（只填估计器读的字段；其余为零）。
    fn stats_with(mut fill: impl FnMut(&mut ImageStats)) -> ImageStats {
        let mut stats = ImageStats {
            width: 64,
            height: 48,
            count: 3072,
            luma_p01: 0,
            luma_p1: 0,
            luma_p5: 0,
            luma_p25: 0,
            luma_p50: 128,
            luma_p75: 0,
            luma_p95: 255,
            luma_p99: 0,
            luma_p999: 0,
            clip_low_ratio: 0.0,
            clip_high_ratio: 0.0,
            channel_clip_high: [0.0; 3],
            linear_p02: 0.0,
            linear_p5: 0.0,
            linear_p25: 0.0,
            linear_p50: 0.0,
            linear_p95: 0.0,
            linear_p995: 0.0,
            linear_p998: 0.0,
            mean_saturation: 0.3,
            saturation_p90: 0.0,
            hue_histogram: [0.0; 36],
            skin_proxy_ratio: 0.0,
            wb_candidate_count: 1000,
            wb_candidate_ratio: 1.0,
            wb_channel_p20: [0.0; 3],
            wb_channel_p25: [0.0; 3],
            wb_channel_p30: [0.0; 3],
            wb_planck_distance_median: 0.0,
            wb_sarle_bc: 0.0,
            wb_rho: 1.0,
            dark_mean: 0.0,
            micro_contrast: 0.04,
            hf_ratio: 0.5,
        };
        fill(&mut stats);
        stats
    }

    /// 曝光闭式：中位分支与高光护栏分支各取小者（变异：护栏分支
    /// 被删 ⇒ 第二组算出 +1.0 而非 −0.06 ⇒ 红）。
    #[test]
    fn exposure_closed_form_branches() {
        let no_scene = SceneFlags::default();
        // 中位分支：P50=0.03 ⇒ 0.12/0.03=4.0；P99.5=0.5 ⇒ 0.95/0.5=1.9；
        // 取小 1.9 ⇒ log2 = 0.926。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.03;
            stats.linear_p995 = 0.5;
        });
        let (ev, _, _, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 1.9f32.log2()).abs() < 1e-4,
            "中位分支应取 1.9，实得 {ev}"
        );
        // 高光护栏分支：P50=0.12 ⇒ 1.0；P99.5=0.99 ⇒ 0.9596；
        // 取小 0.9596 ⇒ log2 = −0.0595。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.12;
            stats.linear_p995 = 0.99;
        });
        let (ev, _, _, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 0.9596f32.log2()).abs() < 1e-3,
            "护栏分支应取 0.9596，实得 {ev}"
        );
    }

    /// 色阶取曝光缩放后百分位：同一统计、两个 ev ⇒ 黑白点跟着动
    /// （变异：色阶用缩放前百分位 ⇒ 两组相同 ⇒ 红）。
    #[test]
    fn levels_follow_exposure_scaling() {
        // 黑点取 0.02（0.01 按规则视为中性 ⇒ 0.0，故不用 0.01 作缩放断言）。
        let stats = stats_with(|stats| {
            stats.linear_p02 = 0.02;
            stats.linear_p998 = 0.9;
        });
        let (black0, white0, _, _) =
            estimate_levels(&stats, 0.0, Confidence::High).expect("非恒等应落成");
        let (black1, white1, _, _) =
            estimate_levels(&stats, 1.0, Confidence::High).expect("非恒等应落成");
        assert!(
            (black0 - 0.02).abs() < 1e-6,
            "ev=0 时黑点应为 0.02，实得 {black0}"
        );
        assert!(
            (white0 - 0.9).abs() < 1e-6,
            "ev=0 时白点应为 0.9，实得 {white0}"
        );
        assert!(
            (black1 - 0.04).abs() < 1e-6,
            "ev=1 时黑点应为 0.04，实得 {black1}"
        );
        assert!(
            (white1 - 1.0).abs() < 1e-6,
            "ev=1 时白点应钳到 1.0，实得 {white1}"
        );
    }

    /// 白平衡反推：掩码后低百分位 ⇒ (T, Ti) ⇒ 内核增益与目标差 < 0.01
    /// （变异：修正步被删 ⇒ 残差 0.003 级仍过？本组 μ 选得使初值
    /// 残差已 < 0.01，故另断言修正后 G 通道不被修坏 + 修正步存在
    /// 性由 `invert_wb_gains` 的 R−B 差分断言覆盖，见下）。
    #[test]
    fn wb_inversion_reproduces_kernel_gains() {
        let target = [1.1073f32, 0.9966, 0.9060];
        let (temperature, tint) = invert_wb_gains(target);
        let gains = [
            1.0 + 0.35 * temperature + 0.15 * tint,
            1.0 - 0.15 * tint,
            1.0 - 0.35 * temperature + 0.15 * tint,
        ];
        for channel in 0..3 {
            assert!(
                (gains[channel] - target[channel]).abs() < 0.01,
                "通道 {channel} 修正后增益 {} 与目标 {} 差超 0.01",
                gains[channel],
                target[channel]
            );
        }
        // 初值约束的精确性：G 定 Ti、R−B 差定 T——修正步是最小二乘，
        // 不应把已精确的约束修坏到 0.01 外（与上断言同界，互为印证）。
        assert!(
            (gains[0] - gains[2] - (target[0] - target[2])).abs() < 0.01,
            "R−B 差应保持"
        );
    }

    /// 白平衡 log-chromaticity 触发 ⇒ 置信低 + 收缩权重进 rationale
    /// （变异：触发条件被删 ⇒ 置信 high ⇒ 红）。
    #[test]
    fn wb_log_chromaticity_trigger_shrinks() {
        let stats = stats_with(|stats| {
            stats.wb_channel_p25 = [0.45, 0.5, 0.55];
            stats.wb_planck_distance_median = 0.2;
        });
        let scene = SceneFlags::default();
        let (_, _, confidence, rationale) = estimate_wb(&stats, &scene).expect("非单色应估出");
        assert_eq!(confidence, Confidence::Low, "Planck 距离 0.2 应触发低置信");
        assert!(
            rationale.contains("权重 0.5"),
            "rationale 应点名收缩权重，实得：{rationale}"
        );
    }

    /// 场景收缩无隐藏台阶：同置信域内统计微扰 ⇒ 曝光输出微扰
    /// （变异：任一硬跳过被塞进非单色场景 ⇒ 第二组输出跳变 ⇒ 红）；
    /// 单色 ⇒ 白平衡跳过且原因非空（设计保留的唯一硬跳过）。
    #[test]
    fn scene_shrinkage_no_hidden_steps() {
        let scene = SceneFlags::default();
        let base = stats_with(|stats| {
            stats.linear_p50 = 0.06;
            stats.linear_p995 = 0.5;
        });
        let perturbed = stats_with(|stats| {
            stats.linear_p50 = 0.061;
            stats.linear_p995 = 0.5;
        });
        let (ev0, _, _, _) = estimate_exposure(&base, &scene);
        let (ev1, _, _, _) = estimate_exposure(&perturbed, &scene);
        assert!(
            (ev1 - ev0).abs() < 0.05,
            "P50 微扰 0.001 时 ev 应微扰，实得 {ev0} → {ev1}"
        );
        let mono = stats_with(|stats| {
            stats.mean_saturation = 0.01;
        });
        let mono_scene = classify_scene(&mono);
        assert!(mono_scene.monochrome, "S̄=0.01 应判单色");
        let wb = estimate_wb(&mono, &mono_scene);
        assert!(wb.is_none(), "单色时白平衡应跳过");
        let inputs = EstimateInputs {
            stats: &mono,
            noise_sigma: None,
            iso: None,
        };
        let plan = plan_auto(
            &inputs,
            &GROUPS
                .iter()
                .map(|name| name.to_string())
                .collect::<Vec<_>>(),
        );
        let skipped = plan
            .skipped
            .iter()
            .find(|entry| entry.object_id == "auto_white_balance")
            .expect("单色时白平衡应有 skipped 项");
        assert!(!skipped.reason.is_empty(), "跳过原因不许为空");
    }

    /// 饱和度底稿修正：S̄=0.55 时 saturation 应为 1.0 + 负向修正
    /// （0.928），不是底稿的 −0.072（变异：丢 +1.0 ⇒ 负值 ⇒ 红，
    /// 且负值过不了 validate_effect 的 [0, 8] 域）。
    #[test]
    fn saturation_is_multiplier_not_delta() {
        let stats = stats_with(|stats| {
            stats.mean_saturation = 0.55;
        });
        let (_, saturation, _) = estimate_color(&stats, &SceneFlags::default());
        assert!(
            (saturation - 0.928).abs() < 1e-4,
            "S̄=0.55 时 saturation 应为 0.928（乘子口径），实得 {saturation}"
        );
        assert!(
            saturation >= 0.7 && saturation <= 1.0,
            "乘子应在 [0.7, 1.0] 内"
        );
    }

    /// 细节公式形：E 低于目标 ⇒ clarity 为正且在域内；E 高于目标
    /// ⇒ clarity 为负；平坦 ⇒ clarity/texture 跳过（plan 层断言）。
    #[test]
    fn detail_formula_shape() {
        let low_e = stats_with(|stats| {
            stats.micro_contrast = 0.02;
            stats.hf_ratio = 0.25;
        });
        let (clarity, texture, _, _) = estimate_detail(&low_e, &SceneFlags::default());
        let clarity = clarity.expect("E=0.02 非平坦应估出");
        // (0.04−0.02)/0.04×0.5 = 0.25，置信恒低减半 ⇒ 0.125。
        assert!(
            (clarity - 0.125).abs() < 1e-6,
            "clarity 应为 0.125，实得 {clarity}"
        );
        let texture = texture.expect("非平坦应估出");
        // (0.5−0.25)/0.5×0.5 = 0.25，减半 ⇒ 0.125。
        assert!(
            (texture - 0.125).abs() < 1e-6,
            "texture 应为 0.125，实得 {texture}"
        );
        let flat = stats_with(|stats| {
            stats.micro_contrast = 0.0;
        });
        let (clarity, texture, _, _) = estimate_detail(&flat, &SceneFlags::default());
        assert!(
            clarity.is_none() && texture.is_none(),
            "平坦时细节两项应跳过"
        );
    }
}
