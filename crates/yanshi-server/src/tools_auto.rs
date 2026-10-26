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
//! P-01 M2：落成期间暂置 `silent` + `wait_for_render`（不产中间预览），
//! 落成后只渲一次终态预览；对象集合与参数值与改前完全一致。
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
//! 旧对象（v1 已知局限，见 phase4-notes.md）：`skipped` 项里点名残留的
//! `stale_object_id` 并提示手动删除（photo-bugfix-p1；默认删除的误删
//! 风险是期 5 明确记录的否决理由，故只回显、不删）。

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Value};
use yanshi_core::{Bbox, ErrorCode, ErrorContext, ObjectType, Result, YanshiError};
use yanshi_render::stats::{image_stats_rgba8, noise_sigma_rgba8, ImageStats, NOISE_KAPPA};

use super::presets::{box_downsample, land_expanded_ops, ExpandedOp, LandedOps};
use super::tools_bitmap::{DEFAULT_RAW_EXPOSURE_EV, RAW_DEFAULT_EXPOSURE_NAME};
use super::tools_effect::validate_effect;
use super::{document_state, optional_str, write_update_effect, EffectKind, ToolContext};
use crate::timings::Phase;

// ---------------------------------------------------------------------------
// 标定常数（回测前为初值，见 phase4-notes.md「D3 待标定」）
// ---------------------------------------------------------------------------

/// 曝光中调目标（线性中位；夜景 0.08、雪景 0.18、
/// 舞台 ×0.9 均为结构调整，不是权重收缩）。
///
/// 0.09 由 3a 期标定（2026-10-25）：旧值 0.12 作纯增益目标时渲染 p50
/// 系统性高于金成图（中位分支三张 +0.04/+0.09/+0.16；金 p50 中心 0.33
/// 对应线性 ≈0.09；levels 白点拉伸 ×1/0.75 是放大器，见 p3a-DESIGN.md）。
const EXPOSURE_T_MID: f32 = 0.09;
/// 曝光高光护栏目标（线性 P99.5，初值 0.95）。
const EXPOSURE_T_CLIP: f32 = 0.95;
/// 高光护栏锚点饱和门限：P99.5 ≥ 此值时锚点已触顶、失信息量，
/// 回退到 P99（再饱和则退 P95），见 C5-2。
const EXPOSURE_ANCHOR_SATURATED: f32 = 0.98;
/// 高光护栏小亮斑判据：P99 < 此比 × P99.5 时顶部 0.5% 是孤立亮斑
/// （灯池/太阳盘），不应用它帽死整图，改用 P99 作锚，见 C2。
/// 标定：01242 线性比 ≈0.85，其余 5 张 ≥0.94。
const EXPOSURE_ANCHOR_PATCH_RATIO: f32 = 0.90;
/// 小亮斑判据的下界（长尾保护）：P99 < 此比 × P99.5 时顶部 0.5%
/// 横跨很宽的亮度带（钠灯街面/立面那种成片亮区），不是孤立亮斑，
/// 护栏正在做真功——沿用 P99.5，不放松。标定（三点）：colorcast
/// 0.52 放松即坏（ev 0.755→1.2 夜景上钳，road 门破）；01242
/// ≈0.85、underexposed 0.89 放松即好；下界取 0.75 留裕量。
const EXPOSURE_ANCHOR_TAIL_MIN: f32 = 0.75;
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
/// 白平衡锚点过少时的置信低门限（设计三触发之外的扩展，见估计器注）。
/// 定值 100（C1 阶段 3 标定：回测集未触及——最小锚点数 53198，属退化
/// 情形地板；无证据改动）。
const WB_MIN_CANDIDATES: usize = 100;
/// 白平衡护栏 G2：钳制饱和检测边（C1 §6；钳制域 ±0.85，留 0.01 容差——
/// 反解落在 0.84–0.85 之间即视为「顶到边」；反解天然算出 0.845 而未被
/// 钳制时同样是「诉求近表达域边界」，同等处理）。
/// **永久保留**（C1 §6.5）：分轴反解后每轴仍有钳制边，饱和检测是 A3
/// 教训的制度化，不随重设计废止；阈值 0.84 由阶段 2 复核。
const WB_SATURATION_EDGE: f32 = 0.84;
/// C1 阶段 1（S1+S2）：锚点集质量门——锚点召回率低于此值 ⇒ 置信 low
/// （定值，C1 阶段 3 标定：0.4 使 forest 满权重 ⇒ ΔT 0.160 出带；
/// 0.6 使 bathroom 召回 0.5695 < 0.6 且饱和 ⇒ 整组跳过）。
const WB_ANCHOR_MIN_INLIER_RATIO: f32 = 0.5;
/// C1 阶段 1（S1+S2）：锚点集质量门——色度散布（MAD）高于此值 ⇒
/// 置信 low（定值，C1 阶段 3 标定：落在 beamcreek 0.1225 与
/// overexposed 0.1935 之间的间隙内；0.12 会误标 beamcreek）。
const WB_ANCHOR_MAX_SPREAD: f32 = 0.15;

/// 色彩：vibrance 触发带（S̄ < 0.34 加 vibrance；S̄ > 0.50 减 saturation；
/// 其间为甜点带，不动）。
const COLOR_VIBRANCE_TRIGGER: f32 = 0.34;
/// 色彩：saturation 触发带上沿。
const COLOR_SATURATION_TRIGGER: f32 = 0.50;
/// 色彩：暖调豁免门限——S̄ > 0.50 时暖色相占比（hue_histogram 桶 0–5 与 35 号
/// 之和，S·V 加权）超此值即判定暖调内容，跳过 saturation 回落
/// （A-05：暖调场景的高饱和是内容，不是过饱和）；非暖场景仍回落。
const COLOR_WARM_EXEMPT: f32 = 0.5;
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
///
/// 本期只做上限 0.45 → 0.15 止血（C3/A-04）：金参考最大值 0.16，旧上限
/// 放任 3–10 倍过量（A-04 的 0.421 即上限产物）。触发改看暗通道低百分位
/// （dark_p10）的重设计推迟到期 4（与 C1 WB 重设计同批）：回测证明当前
/// colorcast 触点的 road 门全靠旧均值公式的 dehaze 0.039 在撑（金配方
/// 该处用分区 WB、dehaze 为 0；auto 的 WB 估计 (0, 0) 已坏），p10 触发
/// 一上就把 road 门从 −43.0% 打回 −0.7%。待 C1 给出真 WB 杠杆后再换
/// 触发变量；`dark_p10` 统计量与回显已就位，供期 4 标定复用，见 p3a-DESIGN.md。
const DEHAZE_TRIGGER: f32 = 0.04;
/// 去雾系数。
const DEHAZE_COEF: f32 = 1.2;
/// 去雾上限。
const DEHAZE_MAX: f32 = 0.15;

/// 降噪：σ̂ → luminance 系数与上限；ISO 先验档位。
/// 系数 20.0 由期 4 回测定版（2026-10-24）：旧初值 6.0 在三张金
/// 参考上系统性偏小 3.3–7.3 倍（中位 3.8 倍）；×20 使 luminance
/// 中位偏差由 0.185 降到 0.032（欠曝 0.218/金 0.25、过曝
/// 0.152/0.15，夜景 ORF 0.137/0.30 仍低，属暗景提亮后噪声放大的
/// 结构残差，不靠再放大全局系数追——低 ISO 上限 0.25 与高 ISO
/// 地板 0.32/0.45 继续兜底两端）。
const DENOISE_SIGMA_COEF: f32 = 20.0;
/// 降噪 luminance 上限。
const DENOISE_MAX: f32 = 0.8;
/// 降噪 color = luminance × 系数；detail 为模式常数（不随强度缩放）。
/// 系数 1.5 由期 4 回测定版（2026-10-24）：旧值 0.8 下三张金
/// color/luminance 比恒为 1.4/2.0/1.5（中位 1.5），auto 派生值
/// 系统性偏低（中位差 0.178）；×1.5 后中位差 0.071（欠曝
/// 0.327/0.35、过曝 0.229/0.30，夜景 0.206/0.45 仍低，同 luminance
/// 的暗景结构残差）；color 仍受 min(…, 1.0) 封顶。
const DENOISE_COLOR_COEF: f32 = 1.5;
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

/// 组白名单（设计固定 8 组；去雾挂在细节组下，不另立组；曲线组 p3b-2
/// 新增，落 WB 之后、色彩之前）。
const GROUPS: [&str; 8] = [
    "exposure", "levels", "tone", "wb", "curves", "color", "detail", "noise",
];

/// 色阶黑点中性门限（线性标度；p3b-2 A-03：由 0.01 降为 0.001——
/// 金黑点 ≈ 线性 p0.1（≈0.0015），0.01 门限把一切真黑点都钉成 0）。
const LEVELS_BLACK_NEUTRAL: f32 = 0.001;

/// 影调高光审美压基底（p3b-2：金高光压是审美性的，clip=0 时金仍压
/// −0.25；非夜景一律先压基底，再按 clip 分档附加）。
const TONE_HIGHLIGHT_AESTHETIC_BASE: f32 = 0.25;
/// 高光 clip 附加压（clip_high_ratio > 0.02 时附加）。
const TONE_HIGHLIGHT_CLIP_HIGH: f32 = 0.20;
/// 高光 clip 附加压（clip_high_ratio > 0.005 时附加）。
const TONE_HIGHLIGHT_CLIP_MID: f32 = 0.05;

/// 曲线暗端 lift 系数（p3b-2 C4：L = 系数 × 曝光前显示 p25）。
const CURVES_LIFT_COEF: f32 = 0.40;
/// 曲线暗端 lift 上限。
const CURVES_LIFT_MAX: f32 = 0.05;
/// 曲线顶端压缩基底（金共有的审美顶压，clip=0 的欠曝金顶压 −0.045）。
const CURVES_TOP_BASE: f32 = 0.045;
/// 曲线顶端压缩 clip 系数（C = 基底 + 系数 × sqrt(clip)，次线性——
/// clip 比饱和快，顶压不应随 clip 线性疯长）。
const CURVES_TOP_CLIP_COEF: f32 = 0.40;
/// 曲线顶端压缩上限。
const CURVES_TOP_MAX: f32 = 0.20;
/// 逐通道曲线顶端门限（channel_clip_high 超过它才发该通道曲线）。
const CURVES_CHANNEL_CLIP_GATE: f32 = 0.02;
/// 逐通道曲线顶端下拉系数（D = 系数 × clip_c）。
const CURVES_CHANNEL_TOP_COEF: f32 = 1.1;
/// 逐通道曲线顶端下拉上限。
const CURVES_CHANNEL_TOP_MAX: f32 = 0.12;

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
    /// 组名（8 组之一）。
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

/// 主导色相桶（桶号，占比）：色相直方图最大桶；并列时取小编号
/// （确定性输出）。分类器与回显共用。
fn dominant_hue(stats: &ImageStats) -> (usize, f32) {
    stats.hue_histogram.iter().enumerate().fold(
        (0usize, 0.0f32),
        |(best_bucket, best), (bucket, share)| {
            if *share > best {
                (bucket, *share)
            } else {
                (best_bucket, best)
            }
        },
    )
}

/// 场景分类（纯函数；门限见模块常量）。
fn classify_scene(stats: &ImageStats) -> SceneFlags {
    let p50 = stats.luma_p50 as f32 / 255.0;
    let p5 = stats.luma_p5 as f32 / 255.0;
    let p95 = stats.luma_p95 as f32 / 255.0;
    let (_, max_hue_share) = dominant_hue(stats);
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
    // 高光护栏锚点选择（C2 + C5-2）：P99.5 触顶失信息量、或被孤立亮斑
    // （灯池/太阳盘）抬高时，改用更低百分位作锚；否则沿用 P99.5。
    // 小亮斑放松只在带内（0.75 ≤ P99/P99.5 < 0.90）：比值过低是长尾
    // （成片亮区，护栏正在做真功），放松会把 ev 放过大（colorcast
    // 实测 0.52 ⇒ ev 1.2、road 门破）。锚点只决定「护栏看哪一档亮度」，
    // T_clip=0.95 本身不动。
    let (anchor, anchor_name) = if p995 >= EXPOSURE_ANCHOR_SATURATED {
        if stats.linear_p99 < EXPOSURE_ANCHOR_SATURATED {
            (stats.linear_p99, "P99（P99.5 已触顶回退）")
        } else {
            (stats.linear_p95, "P95（P99.5/P99 皆触顶回退）")
        }
    } else if stats.linear_p99 < EXPOSURE_ANCHOR_PATCH_RATIO * p995
        && stats.linear_p99 >= EXPOSURE_ANCHOR_TAIL_MIN * p995
    {
        (stats.linear_p99, "P99（顶部亮斑孤立，P99.5 失代表性）")
    } else {
        (p995, "P99.5")
    };
    let mid_ratio = if p50 > 0.0 {
        t_mid / p50
    } else {
        f32::INFINITY
    };
    let clip_ratio = if anchor > 0.0 {
        EXPOSURE_T_CLIP / anchor
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
        "线性 P50={p50:.4}、P99.5={p995:.4}（护栏锚 {anchor_name}={anchor:.4}），目标中位 {t_mid}（{target_note}）\
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

/// 色阶估计：曝光缩放后的黑/白点。百分位是单调变换下的分位数（显示
/// 编码单调 ⇒ 显示 p0.2 = 编码(线性 p0.2)），故直接用线性 p0.2/p99.8
/// × 2^ev、不必重排样本；钳制与中性/钉死判定仍在线性域做（端点物理
/// 位置与旧口径一致），**发射时才编码成显示值**（期 2 A1：内核 levels
/// 已改显示口径，auto 须发显示值，否则黑点会被再压约 12×）。
/// 返回 None（整组恒等 ⇒ 跳过）或 (black, white, 白点是否被钳到 1.0)。
fn estimate_levels(
    stats: &ImageStats,
    ev: f32,
    confidence: Confidence,
) -> Option<(f32, f32, bool, String)> {
    use yanshi_render::color::srgb_encode_extended;
    let scale = 2.0f32.powf(ev);
    let black_raw = scale * stats.linear_p01;
    let white_raw = scale * stats.linear_p998;
    let black = clamp(black_raw, LEVELS_BLACK_RANGE);
    let white = clamp(white_raw, LEVELS_WHITE_RANGE);
    let white_pinned = white_raw >= 0.995;
    let black_neutral = black_raw <= LEVELS_BLACK_NEUTRAL;
    if black_neutral && white_pinned {
        return None;
    }
    let black_final = if black_neutral { 0.0 } else { black };
    let white_final = if white_pinned { 1.0 } else { white };
    let shrunk_black = shrink(black_final, 0.0, confidence.weight());
    let shrunk_white = shrink(white_final, 1.0, confidence.weight());
    // 发射显示值：端点是同一物理点，只是内核现在按显示空间插值。
    let black_out = srgb_encode_extended(shrunk_black);
    let white_out = srgb_encode_extended(shrunk_white);
    let rationale = format!(
        "曝光缩放 ×{scale:.3} 后线性 p0.1={black_raw:.4}、p99.8={white_raw:.4}，\
         钳制黑 [{:.2}, {:.2}]、白 [{:.2}, {:.2}] ⇒ 线性黑 {black_final:.4}、白 {white_final:.4}\
         （白点 ≥0.995 时钉 1.0、黑点 ≤0.001 时钉 0；置信 {} 权重 {}）⇒ 发显示黑 {black_out:.4}、白 {white_out:.4}",
        LEVELS_BLACK_RANGE.0,
        LEVELS_BLACK_RANGE.1,
        LEVELS_WHITE_RANGE.0,
        LEVELS_WHITE_RANGE.1,
        confidence.as_str(),
        confidence.weight(),
    );
    Some((black_out, white_out, white_pinned, rationale))
}

/// 曲线估计（p3b-2 C4：新增第 8 组）：全局 RGB S 曲线 + 逐通道顶端曲线。
///
/// RGB S（5 个控制点，x 固定）：暗端 lift `L = 0.40 × p25_pre`（钳
/// [0, 0.05]）——p25_pre 量暗部有多暗，暗部越暗暗端越要提起来立结构；
/// 顶端压缩 `C = 0.045 + 0.40 × sqrt(clip)`（钳 [0, 0.20]）——基底
/// 0.045 是金共有的审美顶压，clip 项次线性（clip 比饱和快，顶压不应
/// 随 clip 线性疯长）。控制点 y 单调增（L ≤ 0.05、C ≤ 0.20 时恒成立，
/// 最紧一段 0.5→0.75 差 0.25−0.5L−0.3C ≥ 0.165），内核单调插值安全。
/// 逐通道只做顶端：`clip_c > 0.02` 时发 `[[0,0],[0.75,0.75],[1,1−D]]`，
/// `D = min(1.1 × clip_c, 0.12)`；暗端留恒等（金修正 0.005–0.008，
/// 但 WB 残差不可测、锚点失衡解释不了，无可验证的驱动量——已知边界）。
/// RGB 曲线恒发（C 基底 0.045 保证非恒等）；逐通道无触发时只发 RGB。
struct CurvesEstimate {
    /// RGB S 曲线 5 个控制点（[x, y]）。
    rgb_points: [[f32; 2]; 5],
    /// 逐通道顶端曲线：(通道名，顶端下拉 D)。
    channel_curves: Vec<(&'static str, f32)>,
    /// 估值说明（点名 L/C 的统计来源与数值）。
    rationale: String,
}

fn estimate_curves(stats: &ImageStats) -> CurvesEstimate {
    use yanshi_render::color::srgb_encode_extended;
    let p25_pre = srgb_encode_extended(stats.linear_p25);
    let lift = clamp(CURVES_LIFT_COEF * p25_pre, (0.0, CURVES_LIFT_MAX));
    let top = clamp(
        CURVES_TOP_BASE + CURVES_TOP_CLIP_COEF * stats.clip_high_ratio.sqrt(),
        (0.0, CURVES_TOP_MAX),
    );
    let rgb_points = [
        [0.0, 0.0],
        [0.25, 0.25 + lift],
        [0.5, 0.5 + 0.5 * lift - 0.1 * top],
        [0.75, 0.75 - 0.4 * top],
        [1.0, 1.0 - top],
    ];
    let mut channel_curves = Vec::new();
    for (channel, clip_c) in [
        ("r", stats.channel_clip_high[0]),
        ("b", stats.channel_clip_high[2]),
    ] {
        if clip_c > CURVES_CHANNEL_CLIP_GATE {
            channel_curves.push((
                channel,
                (CURVES_CHANNEL_TOP_COEF * clip_c).min(CURVES_CHANNEL_TOP_MAX),
            ));
        }
    }
    let channel_note = if channel_curves.is_empty() {
        "逐通道顶端：各通道 clip ≤ 0.02，不发".to_owned()
    } else {
        channel_curves
            .iter()
            .map(|(ch, d)| format!("{ch} 顶 −{d:.3}"))
            .collect::<Vec<_>>()
            .join("、")
    };
    let rationale = format!(
        "曝光前显示 p25={p25_pre:.3} ⇒ 暗 lift L={lift:.3}；clip_hi={:.3} ⇒ 顶压 C={top:.3}；{channel_note}",
        stats.clip_high_ratio,
    );
    CurvesEstimate {
        rgb_points,
        channel_curves,
        rationale,
    }
}

/// 影调估计：曝光前显示 p25（p3b-2 解耦：shadows 量「暗部结构再立起来」，
/// 不是「提亮后还暗不暗」——曝光一提亮，曝光后 p25 过触发点就把 shadows
/// 压到 0 是旧 bug）与 clip_hi；夜景 highlights 归零；逆光 shadows
/// 叠加转移量；highlights 另有审美压基底 −0.25（金亮而不爆也压）。
fn estimate_tone(
    stats: &ImageStats,
    ev: f32,
    backlit_transfer: f32,
    scene: &SceneFlags,
    confidence: Confidence,
) -> (f32, f32, String) {
    use yanshi_render::color::srgb_encode_extended;
    let p25_pre = srgb_encode_extended(stats.linear_p25);
    let mut shadows = clamp(
        (TONE_SHADOW_TRIGGER - p25_pre) / TONE_SHADOW_TRIGGER,
        (0.0, 1.0),
    ) * TONE_BUDGET;
    shadows = (shadows + backlit_transfer).min(TONE_BUDGET);
    let highlights = if scene.night {
        0.0
    } else {
        let clip_extra = if stats.clip_high_ratio > 0.02 {
            TONE_HIGHLIGHT_CLIP_HIGH
        } else if stats.clip_high_ratio > 0.005 {
            TONE_HIGHLIGHT_CLIP_MID
        } else {
            0.0
        };
        -TONE_HIGHLIGHT_AESTHETIC_BASE - clip_extra
    };
    let shrunk_shadows = shrink(shadows, 0.0, confidence.weight());
    let shrunk_highlights = shrink(highlights, 0.0, confidence.weight());
    let rationale = format!(
        "曝光前显示 p25={p25_pre:.3}（触发点 {}；曝光 ev={ev:.2} 不参与触发，p3b-2 解耦）⇒ shadows={shadows:.3}\
         （逆光转移 +{backlit_transfer:.3}，预算 ±{}）；审美压基底 −{} + clip_hi={:.3} 附加 ⇒ highlights={highlights:.3}\
         （夜景归零）；置信 {} 权重 {}",
        TONE_SHADOW_TRIGGER,
        TONE_BUDGET,
        TONE_HIGHLIGHT_AESTHETIC_BASE,
        stats.clip_high_ratio,
        confidence.as_str(),
        confidence.weight(),
    );
    (shrunk_shadows, shrunk_highlights, rationale)
}

/// 白平衡增益反推：由目标增益 ĝ 求 (T, Ti)，分轴比值反解（C1 阶段 2，S3）。
///
/// 内核增益（filter.rs white_balance）：R×(1+0.35T+0.15Ti)、
/// G×(1−0.15Ti)、B×(1−0.35T+0.15Ti)。两轴各取自己的比值、互不借用
/// 对方的信息：T 轴只看 R/B 比 ρ = ĝR/ĝB，取 Ti = 0 时的精确反解
/// T = (ρ−1)/(0.35(ρ+1))；Ti 轴只看 G/√(RB) 比 s = ĝG/√(ĝR·ĝB)，
/// 取 T = 0 时的精确反解 Ti = (1−s)/(0.15(1+s))。
/// 阶段 1 的联合 Gauss–Newton 修正被取消：它把三通道共模残差（增益
/// 整体缩放）倒进 Ti——中性锚点 (u, v) = (0.2, −0.2) 本无 G 方向偏色，
/// 联合反解却给出 Ti ≈ +0.089（R6）；分轴反解给 Ti = 0（精确）。
/// 代价是系统偏差：真实 (T0, Ti0) 下两轴互相污染对方的比值，
/// |T0|, |Ti0| ≤ 0.25 时每轴偏差 ≤ 0.02，极值 (±0.5, ∓0.5) 时 Ti
/// 偏差达 −0.060——见 `wb_synthetic_bias_decoupled` 的实测分档断言，
/// 不是推理值。偏差随 |Ti| 增大而增大（T 轴公式忽略 Ti 对 R/B 的
/// 影响），但温和区（auto 的实际工作区）内 ≤ 0.02。
/// 返回值逐轴钳制到 `WB_RANGE`；饱和检测由调用方逐轴做。
fn invert_wb_gains(target: [f32; 3]) -> (f32, f32) {
    let (gr, gg, gb) = (target[0], target[1], target[2]);
    let rho = gr / gb;
    let temperature = (rho - 1.0) / (0.35 * (rho + 1.0));
    let s = gg / (gr * gb).sqrt();
    let tint = (1.0 - s) / (0.15 * (1.0 + s));
    (clamp(temperature, WB_RANGE), clamp(tint, WB_RANGE))
}

/// `estimate_wb` 的出口：落成或跳过。跳过必带 reason（点名触发的门编号
/// 与事实数值，C1 §6 G3——不许静默降级；`plan_auto` 的跳过组仍走既有
/// `skipped` 路径，只 reason 来源改由本枚举提供）。
enum WbOutcome {
    /// 落成：(temperature, tint, confidence, rationale)。
    Applied(f32, f32, Confidence, String),
    /// 跳过整组：reason 点名门编号与事实数值。
    Skipped(String),
}

/// 由锚点色度中位 (u, v) = (ln(R/G), ln(B/G)) 求目标增益（几何均值
/// 归一，与旧 `mu` 路径同构：R = e^u、G = 1、B = e^v，几何均值
/// e^((u+v)/3)；中性锚点 (0, 0) ⇒ 增益 [1, 1, 1]）。
fn wb_gains_from_anchor_chroma(chroma_median: [f32; 2]) -> [f32; 3] {
    let (u, v) = (chroma_median[0], chroma_median[1]);
    [
        ((v - 2.0 * u) / 3.0).exp(),
        ((u + v) / 3.0).exp(),
        ((u - 2.0 * v) / 3.0).exp(),
    ]
}

/// 白平衡估计（C1 阶段 2，S3：分轴反解 + 分轴置信；S1 逐像素色度锚 +
/// S2 Planck 滤波提前不变；定位说明：RAW 的机内白平衡已在 develop 时施加，
/// 本组估的是**显影后残余色偏**，不是重做机内白平衡）。
/// 输入是新锚点集统计（信号带内过 Planck 门的逐像素近中性集的色度中位）；
/// 增益反解走分轴 `invert_wb_gains`：T 只看 R/B 比、Ti 只看 G/√(RB) 比，
/// 各轴独立（阶段 1 的联合 Gauss–Newton 修正已取消，见该函数注）。
/// 置信分轴（S3）：锚点集质量（数 / 召回率 / 散布）是两轴共用的证据，
/// 质量低 ⇒ 两轴各收缩 0.5；G2 饱和是逐轴的证据，饱和轴收缩 0.25、
/// 另一轴不受影响；舞台收缩逐轴取更小者。组置信 high 当且仅当质量好且
/// 两轴皆未饱和。
/// 置信重定义（取代现行三量触发）：锚点数 < 100 / 召回率低 / 散布大 ⇒ low；
/// 锚点集为空 ⇒ `wb_no_neutral_anchor` 诚实跳过（D-WB3 的合格形态之一；
/// 旧"增益无定义"跳过是不合格形态，不许再报）。
/// 护栏继承（C1 §6.5）：G2（钳制饱和检测）永久保留，分轴后每轴各有钳制边
/// （阈值 0.84 由阶段 2 复核：保留——阶段 1 的护栏结果在分轴下逐项复现，
/// 见 `wb_guardrail_*`）；G1 的 0.005 锚点下限由信号带下沿 0.02 + 对数地板
/// 接替；G3（reason / rationale 点名）永久保留。
/// 返回落成或跳过（跳过必带 reason，点名门编号与事实数值）。
fn estimate_wb(stats: &ImageStats, scene: &SceneFlags) -> WbOutcome {
    if scene.monochrome {
        return WbOutcome::Skipped("单色画面，白平衡无物理意义".to_owned());
    }
    if stats.wb_anchor_count == 0 {
        return WbOutcome::Skipped(format!(
            "wb_no_neutral_anchor：信号带 [{:.2}, {:.2}] 内 {} 像素、过 Planck 门者 0，锚点集为空，白平衡整组跳过",
            stats.wb_anchor_luma_band.0,
            stats.wb_anchor_luma_band.1,
            stats.wb_anchor_band_count
        ));
    }
    let gains = wb_gains_from_anchor_chroma(stats.wb_anchor_chroma_median);
    let (temperature, tint) = invert_wb_gains(gains);
    // 护栏 G2（§6.5 永久保留）：反解顶到钳制边 = 诉求超出表达域。
    // 分轴检测（S3）：每轴各有一条钳制边，饱和只认定在顶到的那条轴上。
    let t_saturated = temperature.abs() >= WB_SATURATION_EDGE;
    let ti_saturated = tint.abs() >= WB_SATURATION_EDGE;
    let saturated = t_saturated || ti_saturated;
    // 锚点集质量（§4.3，取代现行三量触发）：锚点数 / 召回率 / 散布。
    let small = stats.wb_anchor_count < WB_MIN_CANDIDATES;
    let low_inlier = stats.wb_anchor_inlier_ratio < WB_ANCHOR_MIN_INLIER_RATIO;
    let wide_spread = stats.wb_anchor_chroma_spread > WB_ANCHOR_MAX_SPREAD;
    let quality_low = small || low_inlier || wide_spread;
    // 锚点不可信且反解已饱和 ⇒ 任何落成值都是错的，跳过（§6.3 G1+G2
    // 组合处置在新锚点下的形态：G1 的 0.005 下限由信号带下沿接替）。
    // 跳过仍是整组（S3 不改跳过规则，只改落成时的权重分配）。
    if saturated && quality_low {
        let saturated_axes = match (t_saturated, ti_saturated) {
            (true, true) => "T 轴与 Ti 轴均",
            (true, false) => "T 轴",
            (false, true) => "Ti 轴",
            (false, false) => unreachable!(),
        };
        return WbOutcome::Skipped(format!(
            "锚点质量低（锚点数 {}（门 {}）、召回率 {:.3}（门 {}）、散布 {:.3}（门 {}））且反解 (T, Ti)=({:.3}, {:.3}) {saturated_axes}顶到钳制边（G2），白平衡整组跳过",
            stats.wb_anchor_count,
            WB_MIN_CANDIDATES,
            stats.wb_anchor_inlier_ratio,
            WB_ANCHOR_MIN_INLIER_RATIO,
            stats.wb_anchor_chroma_spread,
            WB_ANCHOR_MAX_SPREAD,
            temperature,
            tint
        ));
    }
    // 分轴置信与权重（S3）：组置信 high 当且仅当质量好且两轴皆未饱和；
    // 每轴权重 = min（该轴 G2 0.25（若饱和）、质量 0.5（若质量低）、
    // 舞台权重），一轴饱和不拖带另一轴。
    let confidence = if quality_low || saturated {
        Confidence::Low
    } else {
        Confidence::High
    };
    let mut weight_t = 1.0f32;
    let mut weight_ti = 1.0f32;
    if t_saturated {
        weight_t = weight_t.min(0.25);
    }
    if ti_saturated {
        weight_ti = weight_ti.min(0.25);
    }
    if quality_low {
        weight_t = weight_t.min(0.5);
        weight_ti = weight_ti.min(0.5);
    }
    let mut guard_note = String::new();
    if saturated {
        let saturated_desc = match (t_saturated, ti_saturated) {
            (true, true) => "T 轴与 Ti 轴均顶到钳制边，强制 low、权重 0.25（两轴各）",
            (true, false) => "T 轴顶到钳制边，强制 low、T 轴权重 0.25，Ti 轴不受影响",
            (false, true) => "Ti 轴顶到钳制边，强制 low、Ti 轴权重 0.25，T 轴不受影响",
            (false, false) => unreachable!(),
        };
        guard_note = format!(
            "；G2：反解 (T, Ti)=({:.3}, {:.3}) {saturated_desc}",
            temperature, tint
        );
    } else if quality_low {
        guard_note = format!(
            "；锚点质量低（锚点数 {}、召回率 {:.3}、散布 {:.3}），强制 low、权重 0.5",
            stats.wb_anchor_count, stats.wb_anchor_inlier_ratio, stats.wb_anchor_chroma_spread
        );
    }
    let mut scene_note = String::new();
    if scene.stage {
        let stage_weight = if quality_low { 0.2 } else { 0.5 };
        weight_t = weight_t.min(stage_weight);
        weight_ti = weight_ti.min(stage_weight);
        scene_note = if guard_note.is_empty() {
            format!("；舞台场景收缩权重 {weight_t}")
        } else if (weight_t - weight_ti).abs() < f32::EPSILON {
            format!("；舞台场景收缩权重 {stage_weight}，与护栏取更小者 ⇒ {weight_t}")
        } else {
            format!(
                "；舞台场景收缩权重 {stage_weight}，与护栏取更小者 ⇒ T {weight_t} / Ti {weight_ti}"
            )
        };
    }
    let shrunk_t = shrink(temperature, 0.0, weight_t);
    let shrunk_ti = shrink(tint, 0.0, weight_ti);
    let rationale = format!(
        "显影后残余色偏：信号带 [{:.2}, {:.2}] 内 {} 像素，过 Planck 门锚点 {} 像素（召回率 {:.3}），锚点色度中位 (u, v)=({:.3}, {:.3})、散布 {:.3} ⇒ 目标增益 [{:.3}, {:.3}, {:.3}] ⇒ (T, Ti)=({temperature:.3}, {tint:.3})（分轴反解：T 由 R/B 比、Ti 由 G/√(RB) 比，各轴独立）；置信 {} ⇒ 权重 T {weight_t} / Ti {weight_ti} 收缩后 ({shrunk_t:.3}, {shrunk_ti:.3}){guard_note}{scene_note}；RAW 机内白平衡已在显影时施加，本组只纠残余偏色",
        stats.wb_anchor_luma_band.0,
        stats.wb_anchor_luma_band.1,
        stats.wb_anchor_band_count,
        stats.wb_anchor_count,
        stats.wb_anchor_inlier_ratio,
        stats.wb_anchor_chroma_median[0],
        stats.wb_anchor_chroma_median[1],
        stats.wb_anchor_chroma_spread,
        gains[0],
        gains[1],
        gains[2],
        confidence.as_str(),
    );
    WbOutcome::Applied(shrunk_t, shrunk_ti, confidence, rationale)
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
        // 暖调豁免（A-05）：暖色相占多数时高饱和是内容（日落/钨丝灯/
        // 暖木），不是过饱和，不抽 saturation；非暖场景仍回落。
        let warm_share: f32 =
            stats.hue_histogram[0..6].iter().sum::<f32>() + stats.hue_histogram[35];
        if warm_share > COLOR_WARM_EXEMPT {
            branch = "S̄ > 0.50 但暖调占多数 ⇒ 暖调豁免，saturation 不动";
        } else {
            saturation = 1.0 + clamp((0.46 - mean_s) * 0.8, (-0.3, 0.0));
            branch = "S̄ > 0.50 ⇒ saturation 回落（乘子口径，1.0 = 恒等）";
        }
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
    // 去雾（C3/A-04）：本期只压上限 0.45 → 0.15 止血，触发仍用均值公式
    // (dark_mean−0.04)×1.2。p10 触发推迟到期 4：回测证明 colorcast 的 road
    // 门全靠均值公式的 dehaze 0.039 在撑（见上常量注释），现阶段不可换。
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
         暗通道均值 {:.3}（p10 {:.3}）⇒ dehaze={:.3}（触发点 {}，上限 {}）{}",
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
        stats.dark_p10,
        dehaze,
        DEHAZE_TRIGGER,
        DEHAZE_MAX,
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
    let night_clause = if scene.night {
        "（夜景档位 ×2）"
    } else {
        ""
    };
    let rationale = format!(
        "σ̂={}、ISO={}{}⇒ 降噪 luminance={}、color=×{}、detail={}（模式常数）；\
         锐化={}{}",
        sigma.map_or("缺失".to_owned(), |value| format!("{value:.4}")),
        iso.map_or("未知".to_owned(), |value| value.to_string()),
        night_clause,
        denoise
            .as_ref()
            .map_or("跳过".to_owned(), |d| format!("{:.3}", d.luminance)),
        DENOISE_COLOR_COEF,
        DENOISE_DETAIL,
        sharpen
            .as_ref()
            .map_or("跳过（σ̂ > 0.28）".to_owned(), |s| format!(
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
    // 曲线组（p3b-2）：params 是非数值的 points 数组 + channel 字符串，
    // 上面通用数值循环碰不到；按点把 y 向恒等线（y=x）插值缩放。
    if type_name == "curves" {
        if let Some(Value::Array(points)) = map.get_mut("points") {
            for point in points.iter_mut() {
                if let Value::Array(pair) = point {
                    if pair.len() == 2 {
                        if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                            pair[1] = json!(x + (y - x) * (strength as f64));
                        }
                    }
                }
            }
        }
        return;
    }
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
                reason: "黑点 ≤0.001 且白点 ≥0.995，色阶已在中性位".to_owned(),
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

    // 白平衡（单色硬跳过；锚点集为空 ⇒ `wb_no_neutral_anchor` 诚实跳过；
    // 锚点质量低 + 反解饱和 ⇒ 护栏跳过整组（reason 点名门编号与事实数值）；
    // 估计恰为 (0, 0) ⇒ 恒等跳过）。
    if want("wb") {
        match estimate_wb(stats, &scene) {
            WbOutcome::Applied(temperature, tint, confidence, rationale)
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
            WbOutcome::Applied(_, _, _, _) => skipped.push(SkippedEntry {
                group: "wb",
                object_id: "auto_white_balance".to_owned(),
                reason: "白平衡估计为 (0, 0)，已无残余偏色".to_owned(),
            }),
            WbOutcome::Skipped(reason) => skipped.push(SkippedEntry {
                group: "wb",
                object_id: "auto_white_balance".to_owned(),
                reason,
            }),
        }
    } else {
        skip_unrequested("wb", "auto_white_balance", &mut skipped);
    }

    // 曲线（p3b-2 C4：第 8 组；RGB S 恒发，逐通道顶端按 clip 门限发；
    // 位置在 WB 之后、色彩之前，与金 recipe 顺序一致）。
    if want("curves") {
        let curves = estimate_curves(stats);
        objects.push(PlannedObject {
            group: "curves",
            object_id: "auto_curves".to_owned(),
            label: "自动基线 · 曲线".to_owned(),
            kind: "adjustment",
            type_name: "curves",
            params: json!({"channel": "rgb", "points": curves.rgb_points}),
            confidence: ev_confidence,
            rationale: curves.rationale.clone(),
        });
        for (channel, top) in curves.channel_curves {
            objects.push(PlannedObject {
                group: "curves",
                object_id: format!("auto_curves_{channel}"),
                label: format!("自动基线 · 曲线 · {channel} 通道"),
                kind: "adjustment",
                type_name: "curves",
                params: json!({"channel": channel, "points": [[0.0, 0.0], [0.75, 0.75], [1.0, 1.0 - top]]}),
                confidence: ev_confidence,
                rationale: curves.rationale.clone(),
            });
        }
    } else {
        skip_unrequested("curves", "auto_curves", &mut skipped);
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

/// RAW 缺省曝光豁免判定（期 4 D3 回测发现的缺口）：RAW 导入在源图层
/// 生成一条缺省 exposure 对象（名与 ev 见 `tools_bitmap` 同名常量）。
/// 它是显影管线的一部分，不是人工精修，故 force 检查不把它当「人工
/// 对象」；但只认「系统状态」的：类型 exposure + 名全匹配 + ev 为
/// 0.5（未动过的导入值）或 0.0（已被 auto 置零过的同一对象——重跑仍
/// 豁免；用户手动置零亦无害：该对象对渲染的贡献本就为零，无叠加）。
/// 其余一切（改过 ev、换过名、非 exposure 类型）一律视为人工。
fn is_raw_default_exposure(object_type: ObjectType, data: &Value) -> bool {
    object_type == ObjectType::Adjustment
        && data.get("adjustment_type").and_then(Value::as_str) == Some("exposure")
        && data.get("name").and_then(Value::as_str) == Some(RAW_DEFAULT_EXPOSURE_NAME)
        && matches!(
            data.get("params").and_then(|params| params.get("ev")).and_then(Value::as_f64),
            Some(ev) if ev == DEFAULT_RAW_EXPOSURE_EV || ev == 0.0
        )
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
    // C5-8（photo-bugfix-p1）：重跑时新跳过的组若层上有旧 `auto_*` 对象
    // 残留，响应须点名（只回显、不删——默认删除的误删风险是期 5 明确记录
    // 的否决理由）。此处先取快照：其后只做渲染读，不落原子，`skipped`
    // 组装时它仍是当前状态。
    let stale_auto: HashSet<String> = alive
        .iter()
        .filter(|object| object.layer_id == layer_id && object.id.starts_with("auto_"))
        .map(|object| object.id.clone())
        .collect();
    // force 检查：非 auto 调整/滤镜对象里，RAW 缺省曝光对象（系统状态）
    // 豁免——其 id 留待落成时处理（曝光落成则置零，曝光跳过则原状）。
    let (raw_defaults, manual): (Vec<String>, Vec<String>) = alive
        .iter()
        .filter(|object| {
            object.layer_id == layer_id
                && matches!(
                    object.object_type,
                    ObjectType::Adjustment | ObjectType::Filter
                )
                && !object.id.starts_with("auto_")
        })
        .map(|object| {
            if is_raw_default_exposure(object.object_type, &object.data) {
                (Some(object.id.clone()), None)
            } else {
                (None, Some(object.id.clone()))
            }
        })
        .fold(
            (Vec::new(), Vec::new()),
            |(mut defaults, mut manual), (default, manual_one)| {
                if let Some(id) = default {
                    defaults.push(id);
                }
                if let Some(id) = manual_one {
                    manual.push(id);
                }
                (defaults, manual)
            },
        );
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
            let mut item = json!({
                "group": entry.group,
                "object_id": entry.object_id,
                "reason": entry.reason,
            });
            // C5-8（photo-bugfix-p1）：本次跳过、但层上旧对象仍存活——
            // 点名残留 id + 提示手动删；不删（误删风险见上）。
            if stale_auto.contains(&entry.object_id) {
                item["stale_object_id"] = json!(entry.object_id);
                item["reason"] = json!(format!(
                    "{}；注意：层上旧对象 {} 仍存活并继续参与渲染，请手动删除（delete_object）",
                    entry.reason, entry.object_id
                ));
            }
            item
        })
        .collect();
    let scene_json: Vec<Value> = plan.scene.names().iter().map(|name| json!(name)).collect();
    // 回显新锚点与分类依据（3a 期新增字段；旧字段不动）：护栏锚 P99、
    // 去雾下沿 dark_p10、主导色相桶与占比（分类误触可复核）。
    // 3b 期回显扩展（旧字段不动）：curves/tone/levels 标定与 WB 护栏
    // 复核用的百分位与逐通道低百分位（只回显、不改估计）。
    let (hue_max_bucket, hue_max_share) = dominant_hue(&stats);
    let stats_json = json!({
        "luma_p50": stats.luma_p50,
        "linear_p01": stats.linear_p01,
        "linear_p02": stats.linear_p02,
        "linear_p5": stats.linear_p5,
        "linear_p25": stats.linear_p25,
        "linear_p50": stats.linear_p50,
        "linear_p95": stats.linear_p95,
        "linear_p99": stats.linear_p99,
        "linear_p995": stats.linear_p995,
        "linear_p998": stats.linear_p998,
        "clip_high_ratio": stats.clip_high_ratio,
        "channel_clip_high": stats.channel_clip_high,
        "mean_saturation": stats.mean_saturation,
        "saturation_p90": stats.saturation_p90,
        "skin_proxy_ratio": stats.skin_proxy_ratio,
        "dark_mean": stats.dark_mean,
        "dark_p10": stats.dark_p10,
        "hue_max_bucket": hue_max_bucket,
        "hue_max_share": hue_max_share,
        "micro_contrast": stats.micro_contrast,
        "hf_ratio": stats.hf_ratio,
        "noise_sigma": noise_sigma,
        "iso": iso,
        "wb_anchor_count": stats.wb_anchor_count,
        "wb_anchor_band_count": stats.wb_anchor_band_count,
        "wb_anchor_inlier_ratio": stats.wb_anchor_inlier_ratio,
        "wb_anchor_chroma_median": stats.wb_anchor_chroma_median,
        "wb_anchor_chroma_spread": stats.wb_anchor_chroma_spread,
        "wb_anchor_luma_band": stats.wb_anchor_luma_band,
    });

    // ---- 只报告（dry_run / strength=0）：零原子 ----
    if dry_run || strength == 0.0 {
        let planned: Vec<Value> = plan
            .objects
            .iter()
            .map(|object| entry_of(object, &object.params))
            .collect();
        let mut note = if dry_run {
            "dry_run：只报告，不落成".to_owned()
        } else {
            "strength=0：只报告，不落成".to_owned()
        };
        if !raw_defaults.is_empty() {
            note.push_str("；目标层有导入缺省曝光（ev 0.5），落成时若自动曝光落成会先置零");
        }
        return Ok(json!({
            "layer_id": layer_id,
            "layer_name": layer_name,
            "applied": [],
            "planned": planned,
            "skipped": skipped_json,
            "scene_flags": scene_json,
            "stats": stats_json,
            "neutralized_default_exposure": [],
            "note": note,
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
    // P-01 M2：打包落成——落成循环里每笔 `write_add/update_effect`
    // 都以 `finish_mutation` 收尾渲一次预览（N 笔 ⇒ N 次全栈预览，
    // 24MP 上 ≈ 73–85 s），其中 `auto_sharpen` 还是 filter 对象
    //（CreateObject 的 `type == "filter"` 是 heavy 原子、带 job，
    // 见 `Atom::is_heavy`）——光置 `silent` 压不住它：`finish_mutation`
    // 见 job 仍会走 `render_region_complete_jobs` 再渲一次。因此
    // 落成期间同时暂置 `silent = true` 与 `wait_for_render = false`
    //（batch 置 `silent` 的既定做法 + 同一规矩：共享上下文上的标志，
    // 用完必须还原）；落成后只渲一次终态预览（它收尾完成全部 job，
    // 见 `Document::render_region_preview` 的 `complete_render_jobs`）。
    // 对象集合与参数值与改前完全一致，只少中间预览。
    let exposure_lands = ops.iter().any(|op| op.object_id == "auto_exposure");
    let opened_here = ctx
        .workspace
        .begin_changeset(&ctx.doc_id, &ctx.session)
        .is_some();
    let prev_silent = ctx.silent;
    let prev_wait_for_render = ctx.wait_for_render;
    ctx.silent = true;
    ctx.wait_for_render = false;
    let landed: Result<(LandedOps, Vec<String>)> = (|| {
        // RAW 缺省曝光置零（曝光落成时）：未动过的缺省对象（ev 0.5）
        // 必须先置零，否则缺省 +0.5 与自动估计叠加，落成结果 ≠ 源上
        // 估计；已被置零过的（ev 0.0）无需再动。置零与落成同属一个
        // changeset，失败时注册表事务层一并回滚。
        let mut neutralized: Vec<String> = Vec::new();
        if exposure_lands {
            for default_id in &raw_defaults {
                let state_now = document_state(ctx)?;
                let current_ev = state_now
                    .alive_objects()
                    .iter()
                    .find(|object| object.id == *default_id)
                    .and_then(|object| {
                        object
                            .data
                            .get("params")
                            .and_then(|params| params.get("ev"))
                            .and_then(Value::as_f64)
                    });
                drop(state_now);
                if current_ev == Some(DEFAULT_RAW_EXPOSURE_EV) {
                    write_update_effect(
                        ctx,
                        &json!({"object_id": default_id, "params": {"ev": 0.0}}),
                        EffectKind::Adjustment,
                    )?;
                    neutralized.push(default_id.clone());
                }
            }
        }
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
        result.map(|landed| (landed, neutralized))
    })();
    ctx.silent = prev_silent;
    ctx.wait_for_render = prev_wait_for_render;
    let (landed, neutralized) = match landed {
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

    // ---- 终态预览（一次；P-01 M2）----
    // 与改前每笔 `finish_mutation` 同一条渲染路，只做一次。调用方
    // 不等渲染（`wait_for_render = false`）⇒ 不渲（与改前每笔一致）；
    // 等渲染且要图 ⇒ 降采样预览（M1 路）；等渲染但不要图 ⇒ 只渲染
    // 跑完 job（轻原子无 job 也走这条：一次全幅脏区渲染，`preview_calls`
    // 同样记 1 次——与 `finish_mutation` 两分支都记 `Phase::Preview`
    // 同口径）。零对象时不渲（与改前一致：循环没跑 ⇒ 一次预览也没
    // 有）。失败路径已在上面提前返回，不渲（回滚后旧预览仍有效，比
    // 留中间态预览更正确）。
    if !ops.is_empty() && ctx.wait_for_render {
        let preview_started = std::time::Instant::now();
        let region = Bbox::new(0.0, 0.0, doc_width as f64, doc_height as f64);
        if ctx.preview && !ctx.silent {
            ctx.workspace.render_region_preview(&ctx.doc_id, region)?;
        } else {
            ctx.workspace
                .render_region_complete_jobs(&ctx.doc_id, region)?;
        }
        ctx.time(Phase::Preview, preview_started);
    }

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
        if object.object_id == "auto_exposure" && !neutralized.is_empty() {
            let rationale = entry["rationale"].as_str().unwrap_or_default().to_owned();
            entry["rationale"] = json!(format!(
                "{rationale}；已将导入缺省曝光对象（{}，ev 0.5）置零，避免与本估计叠加",
                neutralized.join("、")
            ));
        }
        applied.push(entry);
    }
    let mut note =
        "自动基线已落成：每组一笔确定性对象（细节/降噪组多对象），同层重跑原地更新；收缩与强度缩放依据见各 applied 项 rationale"
            .to_owned();
    if !neutralized.is_empty() {
        note.push_str(&format!(
            "；已将导入缺省曝光对象（{}，ev 0.5）置零，避免与自动曝光叠加",
            neutralized.join("、")
        ));
    } else if !raw_defaults.is_empty() && !exposure_lands {
        note.push_str("；导入缺省曝光（ev 0.5）保持原状（自动曝光未落成）");
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
        "neutralized_default_exposure": neutralized,
        "note": note,
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
            linear_p01: 0.0,
            linear_p02: 0.0,
            linear_p5: 0.0,
            linear_p25: 0.0,
            linear_p50: 0.0,
            linear_p95: 0.0,
            linear_p99: 0.0,
            linear_p995: 0.0,
            linear_p998: 0.0,
            mean_saturation: 0.3,
            saturation_p90: 0.0,
            hue_histogram: [0.0; 36],
            skin_proxy_ratio: 0.0,
            wb_anchor_count: 1000,
            wb_anchor_band_count: 1000,
            wb_anchor_inlier_ratio: 1.0,
            wb_anchor_chroma_median: [0.0, 0.0],
            wb_anchor_chroma_spread: 0.0,
            wb_anchor_luma_band: (0.02, 0.5),
            dark_mean: 0.0,
            dark_p10: 0.0,
            micro_contrast: 0.04,
            hf_ratio: 0.5,
        };
        fill(&mut stats);
        stats
    }

    /// 曝光闭式：中位分支与高光护栏分支各取小者（变异：护栏分支
    /// 被删 ⇒ 第二组算出 +1.0 而非 −0.06 ⇒ 红）；另覆锚点选择
    /// （C2 小亮斑回退 P99、长尾保护沿用 P99.5、C5-2 触顶回退 P99）。
    #[test]
    fn exposure_closed_form_branches() {
        let no_scene = SceneFlags::default();
        // 中位分支：P50=0.03 ⇒ 0.09/0.03=3.0；P99.5=0.5、P99=0.49（宽亮区，
        // 锚 P99.5）⇒ 0.95/0.5=1.9；取小 1.9 ⇒ log2 = 0.926。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.03;
            stats.linear_p99 = 0.49;
            stats.linear_p995 = 0.5;
        });
        let (ev, _, rationale, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 1.9f32.log2()).abs() < 1e-4,
            "中位分支应取 1.9，实得 {ev}"
        );
        assert!(
            rationale.contains("护栏锚 P99.5=0.5000"),
            "宽亮区应沿用 P99.5 作锚，实得：{rationale}"
        );
        // 高光护栏分支 + C5-2 回退：P50=0.06 ⇒ 0.09/0.06=1.5；
        // P99.5=0.99 已触顶 ⇒ 回退 P99=0.9 作锚 ⇒ 0.95/0.9=1.0556；
        // 取小 1.0556 ⇒ log2 = 0.0782（变异：回退被删 ⇒ 锚 0.99 ⇒
        // 0.9596 ⇒ log2 = −0.0595 ⇒ 红）。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.06;
            stats.linear_p99 = 0.9;
            stats.linear_p995 = 0.99;
        });
        let (ev, _, rationale, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 1.0556f32.log2()).abs() < 1e-3,
            "触顶回退后护栏分支应取 1.0556，实得 {ev}"
        );
        assert!(
            rationale.contains("P99（P99.5 已触顶回退）"),
            "rationale 应点名回退锚，实得：{rationale}"
        );
        // C2 小亮斑（01242 实测数）：P50=0.0464 ⇒ 0.09/0.0464=1.94；
        // P99.5=0.6874、P99=0.586（0.586 < 0.90×0.6874）⇒ 锚 P99 ⇒
        // 0.95/0.586=1.621；取小 1.621 ⇒ log2 = 0.697（变异：松绑被删 ⇒
        // 锚 0.6874 ⇒ 1.382 ⇒ log2 = 0.467 ⇒ 红）。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.0464;
            stats.linear_p99 = 0.586;
            stats.linear_p995 = 0.6874;
        });
        let (ev, _, rationale, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 1.621f32.log2()).abs() < 1e-3,
            "小亮斑回退后护栏分支应取 1.621，实得 {ev}"
        );
        assert!(
            rationale.contains("顶部亮斑孤立"),
            "rationale 应点名小亮斑回退，实得：{rationale}"
        );
        // C2 长尾保护（colorcast 实测数）：P50=0.0054 ⇒ 中位比 16.7；
        // P99.5=0.5630、P99=0.2931（比 0.52 < 下界 0.75，成片亮区，
        // 非孤立亮斑）⇒ 沿用 P99.5 ⇒ 0.95/0.5630=1.6874；取小 1.6874
        // ⇒ log2 = 0.755（变异：下界被删 ⇒ 锚 P99 ⇒ 1.696 ⇒ 红；
        // 回测里该变异使 ev 0.755→1.2、road 门破）。
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.0054;
            stats.linear_p99 = 0.2931;
            stats.linear_p995 = 0.5630;
        });
        let (ev, _, rationale, _) = estimate_exposure(&stats, &no_scene);
        assert!(
            (ev - 1.6874f32.log2()).abs() < 1e-3,
            "长尾应沿用 P99.5 作锚，护栏分支应取 1.6874，实得 {ev}"
        );
        assert!(
            rationale.contains("护栏锚 P99.5=0.5630"),
            "长尾应沿用 P99.5 作锚，实得：{rationale}"
        );
    }

    /// 色阶取曝光缩放后百分位：同一统计、两个 ev ⇒ 黑白点跟着动
    /// （变异：色阶用缩放前百分位 ⇒ 两组相同 ⇒ 红）。
    /// 期 2 A1 改写：发射的是**显示值**（线性百分位编码后发出）——
    /// ev=0：线性黑 0.02 → 显示 0.1517、白 0.9 → 0.9547；
    /// ev=1：线性黑 0.04 → 显示 0.2209、白钳 1.0。
    /// p3b-2 A-03：黑点统计量由线性 p0.2 改为 p0.1（金黑点 ≈ p0.1）。
    #[test]
    fn levels_follow_exposure_scaling() {
        // 黑点取 0.02（0.001 按规则视为中性 ⇒ 0.0，故不用 0.001 作缩放断言）。
        let stats = stats_with(|stats| {
            stats.linear_p01 = 0.02;
            stats.linear_p998 = 0.9;
        });
        let (black0, white0, _, _) =
            estimate_levels(&stats, 0.0, Confidence::High).expect("非恒等应落成");
        let (black1, white1, _, _) =
            estimate_levels(&stats, 1.0, Confidence::High).expect("非恒等应落成");
        assert!(
            (black0 - 0.1517).abs() < 1e-4,
            "ev=0 时黑点应为显示 0.1517（线性 0.02），实得 {black0}"
        );
        assert!(
            (white0 - 0.9547).abs() < 1e-4,
            "ev=0 时白点应为显示 0.9547（线性 0.9），实得 {white0}"
        );
        assert!(
            (black1 - 0.2209).abs() < 1e-4,
            "ev=1 时黑点应为显示 0.2209（线性 0.04），实得 {black1}"
        );
        assert!(
            (white1 - 1.0).abs() < 1e-6,
            "ev=1 时白点应钳到 1.0，实得 {white1}"
        );
    }

    /// 白平衡反推：目标增益 ⇒ (T, Ti) ⇒ 内核增益与目标差 < 0.01。
    /// 阶段 2（S3）改写：反解是分轴比值公式（T 只看 R/B 比、Ti 只看
    /// G/√(RB) 比），不再是阶段 1 的联合 Gauss–Newton 修正。断言按分轴
    /// 重写：除逐通道增益差 < 0.01 外，另钉分轴精确值——目标
    /// [1.1073, 0.9966, 0.9060] 的分轴反解为 (T, Ti) ≈ (0.286, 0.017)，
    /// 联合反解在此目标上给出 (0.288, 0.037)（Ti 被共模残差拖带
    /// +0.020），故精确值断言同时是分轴/联合的判别器。
    /// （变异：改回联合反解 ⇒ Ti ≈ 0.037 ⇒ 红；T 公式系数 0.35 改
    /// 0.30 ⇒ T ≈ 0.333 ⇒ 红。）
    #[test]
    fn wb_inversion_reproduces_kernel_gains() {
        let target = [1.1073f32, 0.9966, 0.9060];
        let (temperature, tint) = invert_wb_gains(target);
        assert!(
            (temperature - 0.286).abs() < 0.002 && (tint - 0.017).abs() < 0.002,
            "分轴反解应 ≈ (0.286, 0.017)，实得 ({temperature:.4}, {tint:.4})"
        );
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
        // 分轴比值在单轴为 0 时精确：R−B 差由 T 单独决定，修正后
        // R−B 差不应被另一轴的残差带偏到 0.01 外（与上断言同界，互为印证）。
        assert!(
            (gains[0] - gains[2] - (target[0] - target[2])).abs() < 0.01,
            "R−B 差应保持"
        );
    }

    /// 白平衡锚点质量触发 ⇒ 置信低 + 收缩权重进 rationale（C1 阶段 1：
    /// 旧 log-chromaticity 三量触发已退役——置信改算锚点集自身质量，
    /// 见 `estimate_wb`；本测试由 `wb_log_chromaticity_trigger_shrinks`
    /// 改写，口径变更记入提交说明，不许静默放宽）。
    /// 对照组（散布 0）应 high + 全额；触发组（散布 0.3 > 0.15 门）
    /// 应 low + 权重 0.5（变异：散布门被删 ⇒ 触发组 high ⇒ 红）。
    #[test]
    fn wb_anchor_quality_trigger_shrinks() {
        // 锚点色度中位 (0, ln 0.8)：目标增益 [0.928, 0.928, 1.161]，
        // 分轴反解 (T, Ti) ≈ (−0.317, 0.371)，未顶钳制边（阶段 2 改写：
        // 阶段 1 的联合反解给 (−0.332, 0.356)，Ti 被共模残差拖带）。
        let base = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
        });
        let scene = SceneFlags::default();
        let (t0, ti0, confidence0, _) = match estimate_wb(&base, &scene) {
            WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                (temperature, tint, confidence, rationale)
            }
            WbOutcome::Skipped(reason) => panic!("门上统计应落成，实得跳过：{reason}"),
        };
        assert_eq!(confidence0, Confidence::High, "散布 0 时应 high");
        assert!(
            (t0 - (-0.317)).abs() < 0.01 && (ti0 - 0.371).abs() < 0.01,
            "门上统计输出应 ≈ (−0.317, 0.371)，实得 ({t0:.3}, {ti0:.3})"
        );
        let wide = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
            stats.wb_anchor_chroma_spread = 0.3;
        });
        let (confidence, rationale) = match estimate_wb(&wide, &scene) {
            WbOutcome::Applied(_, _, confidence, rationale) => (confidence, rationale),
            WbOutcome::Skipped(reason) => panic!("散布大应 low 落成、不是跳过，实得跳过：{reason}"),
        };
        assert_eq!(confidence, Confidence::Low, "散布 0.3 应触发低置信");
        assert!(
            rationale.contains("权重 0.5") && rationale.contains("锚点质量低"),
            "rationale 应点名锚点质量低与收缩权重，实得：{rationale}"
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
            stats.linear_p99 = 0.49;
            stats.linear_p995 = 0.5;
        });
        let perturbed = stats_with(|stats| {
            stats.linear_p50 = 0.061;
            stats.linear_p99 = 0.49;
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
        assert!(matches!(wb, WbOutcome::Skipped(_)), "单色时白平衡应跳过");
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
        assert!((0.7..=1.0).contains(&saturation), "乘子应在 [0.7, 1.0] 内");
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

    /// 缺省曝光豁免判定：名 + 类型 + ev 三者全匹配才豁免。
    #[test]
    fn raw_default_exposure_predicate() {
        let name = RAW_DEFAULT_EXPOSURE_NAME;
        let untouched = json!({
            "adjustment_type": "exposure",
            "name": name,
            "params": {"ev": 0.5},
        });
        assert!(
            is_raw_default_exposure(ObjectType::Adjustment, &untouched),
            "未动过的缺省对象应豁免"
        );
        let already_zeroed = json!({
            "adjustment_type": "exposure",
            "name": name,
            "params": {"ev": 0.0},
        });
        assert!(
            is_raw_default_exposure(ObjectType::Adjustment, &already_zeroed),
            "已被 auto 置零过的同一对象重跑时仍应豁免"
        );
        let edited = json!({
            "adjustment_type": "exposure",
            "name": name,
            "params": {"ev": 0.8},
        });
        assert!(
            !is_raw_default_exposure(ObjectType::Adjustment, &edited),
            "改过 ev 的缺省对象应视为人工（变异：豁免条件丢掉 ev 检查即红）"
        );
        let renamed = json!({
            "adjustment_type": "exposure",
            "name": "我自己调的曝光",
            "params": {"ev": 0.5},
        });
        assert!(
            !is_raw_default_exposure(ObjectType::Adjustment, &renamed),
            "换过名的同值对象应视为人工"
        );
        let wrong_type = json!({
            "adjustment_type": "exposure",
            "name": name,
            "params": {"ev": 0.5},
        });
        assert!(
            !is_raw_default_exposure(ObjectType::Filter, &wrong_type),
            "滤镜类型即使名与值全中也不豁免"
        );
    }

    /// T_mid 标定（C5-6）：01244 实测数（P50=0.0438、P99.5=0.2612、
    /// P99=0.25，宽亮区锚 P99.5）⇒ 中位分支 0.09/0.0438=2.055 ⇒
    /// ev=1.039（旧 T_mid 0.12 时为 1.453，过曝 0.8 档；变异：T_mid
    /// 改回 0.12 ⇒ 1.453 ⇒ 红）。
    #[test]
    fn t_mid_recalibrated_mid_branch() {
        let stats = stats_with(|stats| {
            stats.linear_p50 = 0.0438;
            stats.linear_p99 = 0.25;
            stats.linear_p995 = 0.2612;
        });
        let (ev, _, _, _) = estimate_exposure(&stats, &SceneFlags::default());
        assert!(
            (ev - 2.0548f32.log2()).abs() < 1e-3,
            "T_mid=0.09 时中位分支应取 2.055，实得 {ev}"
        );
    }

    /// 去雾上限止血（C3/A-04）：01242 实测 dark_mean=0.224 ⇒ 均值公式
    /// (0.224−0.04)×1.2=0.221，钳到上限 0.15（旧上限 0.45 时放任 0.221，
    /// 金 0.03；A-04 的 0.421 同理不可再现。变异：上限改回 0.45 ⇒
    /// 0.221 ⇒ 红）；colorcast 实测 dark_mean=0.072 ⇒ 0.0384，上限下
    /// 不着——回测 road 门全靠该值在撑（金配方该处用分区 WB），本期不
    /// 动触发只压上限正是为此。触发改看 dark_p10 的重设计推迟到期 4
    /// （与 C1 WB 同批，见常量注释与 p3a-DESIGN.md）。
    #[test]
    fn dehaze_cap_bounds_systematic_overuse() {
        let indoor = stats_with(|stats| {
            stats.dark_mean = 0.224;
            stats.dark_p10 = 0.027;
        });
        let (_, _, dehaze, rationale) = estimate_detail(&indoor, &SceneFlags::default());
        assert!(
            (dehaze - 0.15).abs() < 1e-6,
            "室内 dark_mean=0.224 时 dehaze 应钳到上限 0.15，实得 {dehaze}"
        );
        assert!(
            rationale.contains("暗通道均值 0.224"),
            "rationale 应点名暗通道均值，实得：{rationale}"
        );
        let street = stats_with(|stats| {
            stats.dark_mean = 0.072;
            stats.dark_p10 = 0.004;
        });
        let (_, _, kept, _) = estimate_detail(&street, &SceneFlags::default());
        assert!(
            (kept - 0.0384).abs() < 1e-4,
            "colorcast dark_mean=0.072 时 dehaze 应保持 0.0384（road 门所依），实得 {kept}"
        );
        let clear = stats_with(|stats| {
            stats.dark_mean = 0.04;
            stats.dark_p10 = 0.01;
        });
        let (_, _, zero, _) = estimate_detail(&clear, &SceneFlags::default());
        assert!(
            zero == 0.0,
            "dark_mean 在触发点 0.04 时 dehaze 应为 0，实得 {zero}"
        );
    }

    /// 暖调豁免（A-05）：S̄=0.55 且暖色相占多数 ⇒ saturation 留 1.0；
    /// 同 S̄ 但主导冷色相 ⇒ 仍按原式回落 0.928（变异：豁免被删 ⇒ 暖调组
    /// 也给 0.928 ⇒ 红）。
    #[test]
    fn warm_exemption_skips_saturation_cut() {
        let warm = stats_with(|stats| {
            stats.mean_saturation = 0.55;
            stats.hue_histogram[3] = 0.9;
        });
        let (_, saturation, rationale) = estimate_color(&warm, &SceneFlags::default());
        assert!(
            (saturation - 1.0).abs() < 1e-6,
            "暖调 S̄=0.55 时 saturation 应豁免留 1.0，实得 {saturation}"
        );
        assert!(
            rationale.contains("暖调豁免"),
            "rationale 应点名暖调豁免，实得：{rationale}"
        );
        let cool = stats_with(|stats| {
            stats.mean_saturation = 0.55;
            stats.hue_histogram[20] = 0.9;
        });
        let (_, saturation, _) = estimate_color(&cool, &SceneFlags::default());
        assert!(
            (saturation - 0.928).abs() < 1e-4,
            "冷调 S̄=0.55 时 saturation 应回落到 0.928，实得 {saturation}"
        );
    }

    /// 护栏 §6.4-1（C1 阶段 1 改写）：01242 实测锚点色度（旧 p2.5
    /// [0.0018, 0.0015, 0.0009] 的 (u, v) = (ln 1.2, ln 0.6) ≈
    /// (0.182, −0.511)，反解双顶钳制边 (−0.850, 0.765)）+ 锚点集过小
    /// ⇒ 质量低 + 饱和 ⇒ 跳过整组，reason 点名 G2 与锚点事实；`plan_auto`
    /// 的 skipped 项须原样带出该 reason（变异：跳过被删 ⇒ 落成 ⇒ 红；
    /// G2 门被删 ⇒ 质量低单触 low + 0.5 落成 ⇒ 红；小锚点门被删 ⇒
    /// G2 单触 low + 0.25 落成 ⇒ 红）。
    /// 口径变更（记入提交说明）：G1 的 0.005 锚点下限已由信号带下沿
    /// 接替（§6.5 复核），本测试的"双触"现为"锚点质量低 + G2 饱和"。
    #[test]
    fn wb_guardrail_both_gates_skip() {
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.1823, -0.5108];
            stats.wb_anchor_count = 50;
            stats.wb_anchor_band_count = 60;
            stats.wb_anchor_inlier_ratio = 50.0 / 60.0;
        });
        let reason = match estimate_wb(&stats, &SceneFlags::default()) {
            WbOutcome::Skipped(reason) => reason,
            WbOutcome::Applied(temperature, tint, _, _) => {
                panic!("01242 锚点 + 饱和反解应跳过，实得落成 ({temperature:.3}, {tint:.3})")
            }
        };
        assert!(
            reason.contains("G2") && reason.contains("顶到钳制边"),
            "跳过原因须点名 G2 与钳制饱和，实得：{reason}"
        );
        assert!(
            reason.contains("锚点数 50"),
            "跳过原因须写明锚点过小事实，实得：{reason}"
        );
        let inputs = EstimateInputs {
            stats: &stats,
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
        assert!(
            !plan
                .objects
                .iter()
                .any(|object| object.object_id == "auto_white_balance"),
            "双触跳过时不许落成白平衡对象"
        );
        let skipped = plan
            .skipped
            .iter()
            .find(|entry| entry.object_id == "auto_white_balance")
            .expect("双触跳过时白平衡应有 skipped 项");
        assert!(
            skipped.reason.contains("G2") && skipped.reason.contains("顶到钳制边"),
            "响应里的跳过原因须点名 G2 与钳制饱和，实得：{}",
            skipped.reason
        );
    }

    /// 护栏 §6.4-2：锚点质量好 + 反解双顶钳制边 ⇒ G2 单触 ⇒ 强制
    /// low + 权重 0.25，rationale 点名 G2（变异：G2 门被删 ⇒ high +
    /// (−0.850, 0.850) 全额落成 ⇒ 红；权重 0.25 被改回 0.5 ⇒
    /// (−0.425, 0.425) ⇒ 红）。
    #[test]
    fn wb_guardrail_clamp_only_forces_low_quarter_weight() {
        // 锚点色度中位 (0, ln 0.3)：几何均值归一 ⇒ 目标增益
        // [0.669, 0.669, 2.232]，反解 (T, Ti) = (−0.850, 0.850)
        // 双顶钳制边；锚点集质量好，G2 单触。
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -1.2040];
        });
        let (temperature, tint, confidence, rationale) =
            match estimate_wb(&stats, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => panic!("G2 单触应落成，实得跳过：{reason}"),
            };
        assert_eq!(confidence, Confidence::Low, "G2 单触应强制 low");
        assert!(
            (temperature - (-0.2125)).abs() < 1e-6,
            "G2 单触温度应为 −0.85 × 0.25 = −0.2125，实得 {temperature}"
        );
        assert!(
            (tint - 0.2125).abs() < 1e-6,
            "G2 单触 tint 应为 0.85 × 0.25 = 0.2125，实得 {tint}"
        );
        assert!(
            rationale.contains("G2") && rationale.contains("权重 0.25"),
            "rationale 应点名 G2 与权重 0.25，实得：{rationale}"
        );
    }

    /// 护栏 G1 的阶段 1 形态：锚点集过小（< 100）、反解未饱和 ⇒
    /// 强制 low + 权重 0.5（沿用现行 low 规则），rationale 点名锚点
    /// 质量低（变异：小锚点门被删 ⇒ high + 全额 (0, 0.392) 落成 ⇒ 红）。
    /// 口径变更（记入提交说明）：旧 G1 是"锚点 p2.5 < 0.005"，已由
    /// 信号带下沿接替；本测试改钉"锚点数 < 100 ⇒ low + 0.5"。
    /// 阶段 2 改写：分轴反解给 (T, Ti) ≈ (0, 0.392)，阶段 1 的联合反解
    /// 给 (0, 0.346)（Ti 被共模残差压低），tint 期望随之 0.173 → 0.196。
    #[test]
    fn wb_guardrail_anchor_only_forces_low_half_weight() {
        // 锚点色度中位 (ln(0.004/0.0045), ln(0.004/0.0045)) ≈
        // (−0.118, −0.118)：分轴反解 (T, Ti) ≈ (0, 0.392)，未顶钳制边；
        // 锚点数 50 < 100，质量低单触。
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [-0.1178, -0.1178];
            stats.wb_anchor_count = 50;
            stats.wb_anchor_band_count = 50;
            stats.wb_anchor_inlier_ratio = 1.0;
        });
        let (temperature, tint, confidence, rationale) =
            match estimate_wb(&stats, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => panic!("G1 单触应落成，实得跳过：{reason}"),
            };
        assert_eq!(confidence, Confidence::Low, "小锚点单触应强制 low");
        assert!(
            temperature.abs() < 1e-6,
            "小锚点单触温度应 ≈ 0，实得 {temperature}"
        );
        assert!(
            (tint - 0.196).abs() < 0.002,
            "小锚点单触 tint 应 ≈ 0.392 × 0.5 = 0.196，实得 {tint}"
        );
        assert!(
            rationale.contains("锚点质量低") && rationale.contains("权重 0.5"),
            "rationale 应点名锚点质量低与权重 0.5，实得：{rationale}"
        );
    }

    /// 护栏 × 舞台（§6.3）：stage 收缩与护栏收缩取更小者，不许叠加。
    /// G2 单触（护栏 0.25）+ 舞台锚点质量好（舞台 0.5）⇒ 0.25；
    /// 锚点质量低（护栏 0.5）+ 舞台低（0.2）⇒ 0.2；G2 + 锚点质量低
    /// ⇒ 护栏跳过整组（§6.3 G1+G2 组合处置的新锚点形态——反解已饱和
    /// 且锚点不可信时任何落成值都是错的）。
    /// （变异：取更小者被改成只取护栏 ⇒ 第二组给 0.5 ⇒ 红；被改成
    /// 只取舞台 ⇒ 第一组给 0.5 ⇒ 红；跳过被删 ⇒ 第三组落成 ⇒ 红。）
    #[test]
    fn wb_guardrail_stage_takes_min_weight() {
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -1.2040];
        });
        let stage = SceneFlags {
            stage: true,
            ..SceneFlags::default()
        };
        let (temperature, tint, rationale) = match estimate_wb(&stats, &stage) {
            WbOutcome::Applied(temperature, tint, _, rationale) => (temperature, tint, rationale),
            WbOutcome::Skipped(reason) => panic!("G2 单触 + 舞台应落成，实得跳过：{reason}"),
        };
        assert!(
            (temperature - (-0.2125)).abs() < 1e-6 && (tint - 0.2125).abs() < 1e-6,
            "min(0.25, 0.5) 应为 0.25 ⇒ (−0.2125, 0.2125)，实得 ({temperature}, {tint})"
        );
        assert!(
            rationale.contains("与护栏取更小者"),
            "rationale 应点名取更小者，实得：{rationale}"
        );
        // 锚点质量低（散布 0.3）+ 反解未饱和 + 舞台低 ⇒ min(0.5, 0.2) = 0.2。
        let low_stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
            stats.wb_anchor_chroma_spread = 0.3;
        });
        let (temperature, tint) = match estimate_wb(&low_stats, &stage) {
            WbOutcome::Applied(temperature, tint, _, _) => (temperature, tint),
            WbOutcome::Skipped(reason) => {
                panic!("质量低 + 舞台低（未饱和）应落成，实得跳过：{reason}")
            }
        };
        assert!(
            (temperature - (-0.0635)).abs() < 0.002 && (tint - 0.0743).abs() < 0.002,
            "min(0.5, 0.2) 应为 0.2 ⇒ (−0.317, 0.371) × 0.2，实得 ({temperature}, {tint})"
        );
        // G2 + 锚点质量低 ⇒ 跳过（不是 min 落成）。
        let saturated_low = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -1.2040];
            stats.wb_anchor_chroma_spread = 0.3;
        });
        match estimate_wb(&saturated_low, &stage) {
            WbOutcome::Skipped(reason) => assert!(
                reason.contains("G2") && reason.contains("顶到钳制边"),
                "跳过原因须点名 G2 与钳制饱和，实得：{reason}"
            ),
            WbOutcome::Applied(temperature, tint, _, _) => {
                panic!("G2 + 锚点质量低应跳过，实得落成 ({temperature:.3}, {tint:.3})")
            }
        };
    }

    /// R5 `wb_sodium_no_clamp_high_confidence`：钠灯合成统计（B 锚点
    /// ≈0、目标 B 增益 ≈7.4×）⇒ 不以 high 置信顶到钳制边落成。
    /// 两档（皆为合格）：(A) 统计层诚实路径——钠灯像素过不了 Planck
    /// 门，锚点集为空 ⇒ `wb_no_neutral_anchor` 跳过（D-WB3）；
    /// (B) 估计层应力——锚点色度中位 (0, ln 0.05) 迫使反解双顶钳制边
    /// ⇒ low + 权重 0.25 落成，rationale 点名 G2 饱和
    /// （变异：去掉饱和检测 ⇒ (B) high + (−0.850, 0.850) 全额 ⇒ 红；
    /// 去掉空锚点跳过 ⇒ (A) 落成 ⇒ 红）。
    #[test]
    fn wb_sodium_no_clamp_high_confidence() {
        let empty = stats_with(|stats| {
            stats.wb_anchor_count = 0;
            stats.wb_anchor_band_count = 5000;
            stats.wb_anchor_inlier_ratio = 0.0;
        });
        let reason = match estimate_wb(&empty, &SceneFlags::default()) {
            WbOutcome::Skipped(reason) => reason,
            WbOutcome::Applied(temperature, tint, confidence, _) => panic!(
                "钠灯空锚点应跳过，实得落成 ({temperature:.3}, {tint:.3}) 置信 {}",
                confidence.as_str()
            ),
        };
        assert!(
            reason.contains("wb_no_neutral_anchor"),
            "钠灯空锚点跳过原因须点名 wb_no_neutral_anchor，实得：{reason}"
        );
        let saturated = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -2.9957];
        });
        let (temperature, tint, confidence, rationale) =
            match estimate_wb(&saturated, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => {
                    panic!("锚点质量好时饱和应 low 落成，实得跳过：{reason}")
                }
            };
        assert_eq!(confidence, Confidence::Low, "饱和反解不许报 high");
        assert!(
            (temperature - (-0.2125)).abs() < 1e-6 && (tint - 0.2125).abs() < 1e-6,
            "饱和应为 (−0.85, 0.85) × 0.25，实得 ({temperature:.3}, {tint:.3})"
        );
        assert!(
            rationale.contains("G2") && rationale.contains("权重 0.25"),
            "rationale 应点名 G2 与权重 0.25，实得：{rationale}"
        );
    }

    /// R7 `wb_confidence_uses_anchor_set`：同一统计、锚点质量连续变化
    /// 时输出连续变化（无隐藏台阶）；门触发本身是显式台阶，必须在
    /// rationale 里点名，不能静默跳变。单色跳过部分沿用既有断言
    /// `scene_shrinkage_no_hidden_steps`，此处不重复。
    /// 口径变更（记入提交说明）：旧"G1 下限"显式台阶现为"锚点数 100 门"。
    /// （变异：估计值改成常量 ⇒ 两组输出相同 ⇒ 红；门触发不点名 ⇒
    /// 第二组 rationale 无"锚点质量低" ⇒ 红。）
    #[test]
    fn wb_confidence_uses_anchor_set() {
        // 连续变化：锚点色度微扰（u 0 → ln 1.05），两组皆在门上且反解
        // 未饱和 ⇒ 输出应连续变化、无跳变。
        let base = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
        });
        let perturbed = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0488, -0.2231];
        });
        let (t0, ti0, confidence0, _) = match estimate_wb(&base, &SceneFlags::default()) {
            WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                (temperature, tint, confidence, rationale)
            }
            WbOutcome::Skipped(reason) => panic!("门上统计应落成，实得跳过：{reason}"),
        };
        assert_eq!(confidence0, Confidence::High, "门上统计应 high");
        assert!(
            (t0 - (-0.317)).abs() < 0.01 && (ti0 - 0.371).abs() < 0.01,
            "门上统计输出应 ≈ (−0.317, 0.371)，实得 ({t0:.3}, {ti0:.3})"
        );
        let (t1, ti1, _, _) = match estimate_wb(&perturbed, &SceneFlags::default()) {
            WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                (temperature, tint, confidence, rationale)
            }
            WbOutcome::Skipped(reason) => panic!("微扰统计应落成，实得跳过：{reason}"),
        };
        assert!(
            (t1 - t0).abs() < 0.15 && (ti1 - ti0).abs() < 0.15,
            "锚点微扰时输出应微扰，实得 ({t0:.3}, {ti0:.3}) → ({t1:.3}, {ti1:.3})"
        );
        assert!(
            (t1 - t0).abs() > 1e-6 || (ti1 - ti0).abs() > 1e-6,
            "输出须随锚点变化（常量估计即红）"
        );
        // 显式台阶：同一色度中位，锚点数越过 100 门（1000 → 50）⇒
        // 强制 low + 权重 0.5，rationale 点名锚点质量低（不是静默跳变）。
        let above = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
            stats.wb_anchor_count = 1000;
        });
        let below = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.0, -0.2231];
            stats.wb_anchor_count = 50;
            stats.wb_anchor_band_count = 50;
            stats.wb_anchor_inlier_ratio = 1.0;
        });
        let (_, _, confidence_above, _) = match estimate_wb(&above, &SceneFlags::default()) {
            WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                (temperature, tint, confidence, rationale)
            }
            WbOutcome::Skipped(reason) => panic!("门上统计应落成，实得跳过：{reason}"),
        };
        assert_eq!(confidence_above, Confidence::High, "门上应 high");
        let (temperature_below, tint_below, confidence_below, rationale_below) =
            match estimate_wb(&below, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => {
                    panic!("小锚点应 low 落成、不是跳过，实得跳过：{reason}")
                }
            };
        assert_eq!(
            confidence_below,
            Confidence::Low,
            "越过锚点数下限应强制 low"
        );
        assert!(
            rationale_below.contains("锚点质量低"),
            "越过锚点数下限时 rationale 应点名锚点质量低，实得：{rationale_below}"
        );
        assert!(
            (temperature_below - (-0.159)).abs() < 0.002 && (tint_below - 0.186).abs() < 0.002,
            "小锚点应为 (−0.317, 0.371) × 0.5，实得 ({temperature_below:.3}, {tint_below:.3})"
        );
    }

    // R1–R4（阶段 1，统计层）：已在 `yanshi-render/src/stats.rs` 的测试
    // 模块落实为真测试（`wb_anchor_is_per_pixel` /
    // `wb_anchor_excludes_no_signal` /
    // `wb_planck_prefilter_excludes_green` /
    // `wb_anchor_stats_streaming_exact`）；此处的阶段 0 占位随阶段 1
    // 落地删除，不留 `todo!` 占位。

    /// R6 `wb_decoupled_axes`（C1 阶段 2，S3 落地）：合成锚点（R/B 证据
    /// 足、G 证据缺）⇒ T 轴正常落成、Ti 轴精确为 0 且 rationale 分轴写明。
    /// 锚点 (u, v) = (0.2, −0.2)：u + v = 0 ⇒ 无 G 方向偏色（目标增益
    /// G/√(RB) = 1）；分轴反解 T ≈ −0.564、Ti = 0。阶段 1 的联合反解
    /// 在此锚点上给出 Ti ≈ +0.089（Gauss–Newton 步把共模残差倒进 Ti），
    /// 是 T→Ti 的拖带；分轴反解消除它。
    /// （变异：改回联合反解 ⇒ Ti ≈ 0.089 ⇒ 红；T 公式改回 (R−B)/0.7
    /// 初值 ⇒ T ≈ −0.575 ⇒ 红。）
    #[test]
    fn wb_decoupled_axes_r6() {
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.2, -0.2];
        });
        let (temperature, tint, confidence, rationale) =
            match estimate_wb(&stats, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => panic!("R6 锚点应落成，实得跳过：{reason}"),
            };
        assert_eq!(
            confidence,
            Confidence::High,
            "R6 锚点质量好且未饱和，应 high"
        );
        assert!(
            (temperature - (-0.564)).abs() < 0.002,
            "R6 的 T 应 ≈ −0.564（分轴比值反解），实得 {temperature:.4}"
        );
        assert!(
            tint.abs() < 1e-6,
            "R6 的 Ti 应精确为 0（锚点无 G 方向偏色），实得 {tint:.6}"
        );
        assert!(
            rationale.contains("分轴反解"),
            "rationale 应点名分轴反解，实得：{rationale}"
        );
    }

    /// S3 分轴权重：T 轴饱和、Ti 轴未饱和时，T 轴收缩 0.25、Ti 轴全额
    /// 落成——一轴饱和不拖带另一轴（阶段 1 的共享权重会把 Ti 也压到
    /// 0.25）。锚点 (u, v) = (0.2468, −0.3668)：R/B 比迫使 T 顶到
    /// −0.85，G/√(RB) 比给出 Ti ≈ 0.200。
    /// （变异：权重改回共享 ⇒ Ti = 0.05 ⇒ 红；饱和检测改回整组 ⇒
    /// Ti 也 0.25 ⇒ 红。）
    #[test]
    fn wb_decoupled_axes_single_axis_saturation() {
        let stats = stats_with(|stats| {
            stats.wb_anchor_chroma_median = [0.2468, -0.3668];
        });
        let (temperature, tint, confidence, rationale) =
            match estimate_wb(&stats, &SceneFlags::default()) {
                WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                    (temperature, tint, confidence, rationale)
                }
                WbOutcome::Skipped(reason) => panic!("单轴饱和应落成，实得跳过：{reason}"),
            };
        assert_eq!(confidence, Confidence::Low, "一轴饱和应强制 low");
        assert!(
            (temperature - (-0.2125)).abs() < 1e-6,
            "饱和的 T 轴应为 −0.85 × 0.25 = −0.2125，实得 {temperature:.4}"
        );
        assert!(
            (tint - 0.200).abs() < 0.002,
            "未饱和的 Ti 轴应全额 ≈ 0.200，实得 {tint:.4}"
        );
        assert!(
            rationale.contains("T 轴顶到钳制边") && rationale.contains("Ti 轴不受影响"),
            "rationale 应分轴点名 T 轴饱和、Ti 轴不受影响，实得：{rationale}"
        );
    }

    /// §4.4 合成图断言先行（阶段 2）：已知 (T0, Ti0) 的合成中性渐变
    /// ⇒ 锚点色度中位反推目标增益 ⇒ 分轴反解的系统偏差实测分档。
    /// 温和档 |T0|, |Ti0| ≤ 0.25（auto 的实际工作区）：每轴偏差 ≤ 0.02；
    /// 极值档 |T0|, |Ti0| ≤ 0.5：偏差 ≤ 0.065（实测最坏 Ti 偏差 −0.060，
    /// 在 (T0, Ti0) = (±0.5, −0.5)；分轴公式忽略另一轴对比值的影响，
    /// 偏差随 |Ti| 增大而增大——这是分轴的已知代价，不是 bug）。
    /// 合成统计经 `estimate_wb` 全程（锚点中位 → 目标增益 → 分轴反解 →
    /// 权重），两档的反解皆未饱和、质量好，权重 1.0 不掩盖偏差。
    /// （变异：T 公式系数 0.35 改 0.30 ⇒ 温和档 T 偏差在 (0.25, 0.25)
    /// 处达 0.031 ⇒ 红；Ti 公式系数 0.15 改 0.10 ⇒ 温和档 Ti 偏差
    /// 达 0.107 ⇒ 红；偏差界放宽到 0.1 ⇒ 极值档不再钉住实测值 ⇒ 红。）
    #[test]
    fn wb_synthetic_bias_decoupled() {
        // 已知 (T0, Ti0) 的内核增益 → 锚点色度中位 (u, v) 的逆映射：
        // u = ln(ĝG/ĝR)，v = ln(ĝG/ĝB)（`wb_gains_from_anchor_chroma` 的逆）。
        fn synthetic_stats(t0: f32, ti0: f32) -> ImageStats {
            let gains = [
                1.0 + 0.35 * t0 + 0.15 * ti0,
                1.0 - 0.15 * ti0,
                1.0 - 0.35 * t0 + 0.15 * ti0,
            ];
            let u = (gains[1] / gains[0]).ln();
            let v = (gains[1] / gains[2]).ln();
            stats_with(|stats| {
                stats.wb_anchor_chroma_median = [u, v];
            })
        }
        let scene = SceneFlags::default();
        let mut worst_moderate = 0.0f32;
        for t0 in [-0.25f32, -0.125, 0.0, 0.125, 0.25] {
            for ti0 in [-0.25f32, -0.125, 0.0, 0.125, 0.25] {
                let stats = synthetic_stats(t0, ti0);
                let (temperature, tint, _, _) = match estimate_wb(&stats, &scene) {
                    WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                        (temperature, tint, confidence, rationale)
                    }
                    WbOutcome::Skipped(reason) => {
                        panic!("合成 ({t0}, {ti0}) 应落成，实得跳过：{reason}")
                    }
                };
                let bias_t = (temperature - t0).abs();
                let bias_ti = (tint - ti0).abs();
                worst_moderate = worst_moderate.max(bias_t).max(bias_ti);
                assert!(
                    bias_t <= 0.02 && bias_ti <= 0.02,
                    "温和档合成 ({t0}, {ti0}) 分轴偏差应 ≤ 0.02，实得 ({temperature:.4}, {tint:.4})"
                );
            }
        }
        assert!(
            worst_moderate > 0.005,
            "温和档最坏偏差应 > 0.005（断言须真的量到分轴偏差，不是恒 0 的安慰剂），实得 {worst_moderate:.4}"
        );
        let mut worst_wide = 0.0f32;
        for t0 in [-0.5f32, -0.3, 0.3, 0.5] {
            for ti0 in [-0.5f32, -0.3, 0.3, 0.5] {
                let stats = synthetic_stats(t0, ti0);
                let (temperature, tint, _, _) = match estimate_wb(&stats, &scene) {
                    WbOutcome::Applied(temperature, tint, confidence, rationale) => {
                        (temperature, tint, confidence, rationale)
                    }
                    WbOutcome::Skipped(reason) => {
                        panic!("合成 ({t0}, {ti0}) 应落成，实得跳过：{reason}")
                    }
                };
                let bias_t = (temperature - t0).abs();
                let bias_ti = (tint - ti0).abs();
                worst_wide = worst_wide.max(bias_t).max(bias_ti);
                assert!(
                    bias_t <= 0.065 && bias_ti <= 0.065,
                    "极值档合成 ({t0}, {ti0}) 分轴偏差应 ≤ 0.065，实得 ({temperature:.4}, {tint:.4})"
                );
            }
        }
        assert!(
            (0.04..0.065).contains(&worst_wide),
            "极值档最坏偏差应落在 [0.04, 0.065]（钉住实测值 0.060），实得 {worst_wide:.4}"
        );
    }

    /// p3b-2 tone：shadows 触发用曝光前 p25（解耦）。合成：linear_p25
    /// =0.015、ev=1.0 ⇒ 曝光前显示 p25=0.128 < 0.16 < 曝光后 0.190，
    /// 旧公式（曝光后）给 0，新公式给 (0.16−0.128)/0.16×0.3≈0.0593。
    /// 变异：触发改回曝光后 p25 ⇒ shadows=0 ⇒ 红。
    #[test]
    fn tone_shadows_uses_pre_exposure_p25() {
        let stats = stats_with(|stats| {
            stats.linear_p25 = 0.015;
            stats.clip_high_ratio = 0.0;
        });
        let (shadows, _, _) =
            estimate_tone(&stats, 1.0, 0.0, &SceneFlags::default(), Confidence::High);
        assert!(
            shadows > 0.0,
            "曝光前 p25 在触发点下时 shadows 不许为 0，实得 {shadows}"
        );
        assert!(
            (shadows - 0.0593).abs() < 1e-3,
            "shadows 应≈0.0593（曝光前公式），实得 {shadows}"
        );
    }

    /// p3b-2 tone：highlights 审美压基底 −0.25 + clip 分档附加。
    /// clip=0 ⇒ −0.25（金亮而不爆也压）；clip=0.01 ⇒ −0.30；
    /// clip=0.0725 ⇒ −0.45；夜景 ⇒ 0（clip 再大也归零）。
    /// 变异：基底删掉 ⇒ clip=0 时 highlights=0 ⇒ 红；分档改回
    /// 旧档位（−0.3/−0.18）⇒ clip=0.0725 时 −0.3 ≠ −0.45 ⇒ 红。
    #[test]
    fn tone_highlights_aesthetic_base_and_clip_tiers() {
        let no_scene = SceneFlags::default();
        let no_clip = stats_with(|stats| {
            stats.linear_p25 = 0.5;
            stats.clip_high_ratio = 0.0;
        });
        let (_, highlights, _) = estimate_tone(&no_clip, 0.0, 0.0, &no_scene, Confidence::High);
        assert!(
            (highlights + 0.25).abs() < 1e-4,
            "clip=0 时 highlights 应为审美基底 −0.25，实得 {highlights}"
        );
        let mid_clip = stats_with(|stats| {
            stats.linear_p25 = 0.5;
            stats.clip_high_ratio = 0.01;
        });
        let (_, highlights, _) = estimate_tone(&mid_clip, 0.0, 0.0, &no_scene, Confidence::High);
        assert!(
            (highlights + 0.30).abs() < 1e-4,
            "clip=0.01 时 highlights 应为 −0.30，实得 {highlights}"
        );
        let high_clip = stats_with(|stats| {
            stats.linear_p25 = 0.5;
            stats.clip_high_ratio = 0.0725;
        });
        let (_, highlights, _) = estimate_tone(&high_clip, 0.0, 0.0, &no_scene, Confidence::High);
        assert!(
            (highlights + 0.45).abs() < 1e-4,
            "clip=0.0725 时 highlights 应为 −0.45，实得 {highlights}"
        );
        let night = SceneFlags {
            night: true,
            ..SceneFlags::default()
        };
        let (_, highlights, _) = estimate_tone(&high_clip, 0.0, 0.0, &night, Confidence::High);
        assert!(
            highlights == 0.0,
            "夜景时 highlights 应归零，实得 {highlights}"
        );
    }

    /// p3b-2 C4：RGB S 曲线 L/C 公式（欠曝/过曝两组金统计量拟合）。
    /// 欠曝（p25=0.00381、clip=0）：L=0.0194、C=0.045；过曝
    /// （p25=0.009638、clip=0.0725）：L=0.0390、C=0.1527。
    /// 另断关系：y 单调增、y(0.25)>0.25（暗 lift）、y(1)<1（顶压）。
    /// 变异：L/C 公式常量化 ⇒ 控制点 y 偏离 ⇒ 红。
    #[test]
    fn curves_rgb_lift_and_top_formula() {
        let under = stats_with(|stats| {
            stats.linear_p25 = 0.00381;
            stats.clip_high_ratio = 0.0;
        });
        let estimate = estimate_curves(&under);
        let expected_under = [
            [0.0, 0.0],
            [0.25, 0.26943],
            [0.5, 0.50522],
            [0.75, 0.732],
            [1.0, 0.955],
        ];
        for (got, want) in estimate.rgb_points.iter().zip(expected_under.iter()) {
            assert!(
                (got[0] - want[0]).abs() < 1e-4 && (got[1] - want[1]).abs() < 1e-3,
                "欠曝 RGB 曲线控制点应≈{want:?}，实得 {got:?}"
            );
        }
        assert!(
            estimate.channel_curves.is_empty(),
            "欠曝各通道 clip 为 0 时不应发逐通道曲线"
        );
        let over = stats_with(|stats| {
            stats.linear_p25 = 0.009638;
            stats.clip_high_ratio = 0.0725;
        });
        let estimate = estimate_curves(&over);
        let expected_over = [
            [0.0, 0.0],
            [0.25, 0.28900],
            [0.5, 0.50423],
            [0.75, 0.68892],
            [1.0, 0.84730],
        ];
        for (got, want) in estimate.rgb_points.iter().zip(expected_over.iter()) {
            assert!(
                (got[0] - want[0]).abs() < 1e-4 && (got[1] - want[1]).abs() < 1e-3,
                "过曝 RGB 曲线控制点应≈{want:?}，实得 {got:?}"
            );
        }
        // 关系断言：单调增（内核单调插值的前提）、暗 lift、顶压。
        for window in estimate.rgb_points.windows(2) {
            assert!(
                window[1][1] > window[0][1],
                "RGB 曲线 y 必须单调增，实得 {window:?}"
            );
        }
        assert!(
            estimate.rgb_points[1][1] > 0.25 && estimate.rgb_points[4][1] < 1.0,
            "RGB 曲线应暗 lift（y(0.25)>0.25）且顶压（y(1)<1），实得 {:?}",
            estimate.rgb_points
        );
    }

    /// p3b-2 C4：逐通道顶端门限与下拉。过曝 channel_clip_high
    /// [0.0832, 0.0733, 0.0987] ⇒ r D=0.0915、b D=0.1086（G 通道
    /// 不发——金 r/b 顶端下拉，G 恒等）；全通道 clip ≤ 0.02 ⇒ 不发。
    /// 变异：门限 0.02 改 0 ⇒ 全通道都发 ⇒ 红；系数 1.1 改 1.0 ⇒
    /// D 值偏离 ⇒ 红。
    #[test]
    fn curves_channel_top_gate_and_pull_down() {
        let over = stats_with(|stats| {
            stats.linear_p25 = 0.009638;
            stats.clip_high_ratio = 0.0725;
            stats.channel_clip_high = [0.0832, 0.0733, 0.0987];
        });
        let estimate = estimate_curves(&over);
        assert_eq!(
            estimate.channel_curves.len(),
            2,
            "过曝应发 r、b 两条逐通道曲线，实得 {:?}",
            estimate.channel_curves
        );
        assert_eq!(estimate.channel_curves[0].0, "r");
        assert!(
            (estimate.channel_curves[0].1 - 0.0915).abs() < 1e-3,
            "r 顶端下拉应≈0.0915，实得 {}",
            estimate.channel_curves[0].1
        );
        assert_eq!(estimate.channel_curves[1].0, "b");
        assert!(
            (estimate.channel_curves[1].1 - 0.1086).abs() < 1e-3,
            "b 顶端下拉应≈0.1086，实得 {}",
            estimate.channel_curves[1].1
        );
        let quiet = stats_with(|stats| {
            stats.linear_p25 = 0.00381;
            stats.clip_high_ratio = 0.0;
            stats.channel_clip_high = [0.000126, 0.0, 0.0];
        });
        let estimate = estimate_curves(&quiet);
        assert!(
            estimate.channel_curves.is_empty(),
            "各通道 clip ≤ 0.02 时不应发逐通道曲线，实得 {:?}",
            estimate.channel_curves
        );
    }

    /// p3b-2 A-03：黑点用线性 p0.1 + 中性门限 0.001。p0.1=0.0015、
    /// ev=0 ⇒ 发显示黑 ≈0.0194（旧 p0.2 公式在 p02=0 时给 0）；
    /// p0.1=0.0005 且白点顶 ⇒ 跳过（旧公式在 p02=0.02 时会发）；
    /// p0.1=0.005（新门限之上、旧门限之下）⇒ 落成 ≈0.0610。
    /// 变异：统计量改回 p0.2 ⇒ 第一组黑 0 ⇒ 红；门限改回 0.01 ⇒
    /// 第三组变 None ⇒ 红。
    #[test]
    fn levels_black_uses_p01() {
        let dark = stats_with(|stats| {
            stats.linear_p01 = 0.0015;
            stats.linear_p02 = 0.0;
            stats.linear_p998 = 0.5;
        });
        let (black, _, _, _) = estimate_levels(&dark, 0.0, Confidence::High).expect("黑点应落成");
        assert!(
            (black - 0.01938).abs() < 1e-3,
            "p0.1=0.0015 时显示黑应≈0.0194，实得 {black}"
        );
        assert!(black > 0.0, "真黑点不许被钉成 0");
        let neutral = stats_with(|stats| {
            stats.linear_p01 = 0.0005;
            stats.linear_p02 = 0.02;
            stats.linear_p998 = 1.0;
        });
        assert!(
            estimate_levels(&neutral, 0.0, Confidence::High).is_none(),
            "p0.1=0.0005 且白点顶时色阶应整体跳过"
        );
        // 门限分界：p0.1=0.005 在新门限（0.001）之上、在旧门限（0.01）
        // 之下 ⇒ 新规则落成、旧规则跳过（变异：门限改回 0.01 ⇒
        // 本组变 None ⇒ 红）。
        let mid = stats_with(|stats| {
            stats.linear_p01 = 0.005;
            stats.linear_p02 = 0.0;
            stats.linear_p998 = 1.0;
        });
        let (black, _, _, _) =
            estimate_levels(&mid, 0.0, Confidence::High).expect("p0.1=0.005 时黑点应落成");
        assert!(
            (black - 0.0610).abs() < 1e-3,
            "p0.1=0.005 时显示黑应≈0.0610，实得 {black}"
        );
    }

    /// p3b-2 scale_params：curves 按点向恒等线插值缩放。strength=0.5
    /// 时 y' = x + (y−x)×0.5；strength=0 时全回恒等。
    /// 变异：删掉 curves 分支 ⇒ strength=0.5 时点不动 ⇒ 红。
    #[test]
    fn scale_params_curves_interpolates_toward_identity() {
        let mut params = json!({"channel": "rgb", "points": [[0, 0], [0.25, 0.3], [0.5, 0.5], [0.75, 0.7], [1, 0.9]]});
        scale_params("curves", &mut params, 0.5);
        let points = params["points"].as_array().expect("points 应为数组");
        let ys: Vec<f64> = points
            .iter()
            .map(|point| point[1].as_f64().expect("y 应为数"))
            .collect();
        let expected = [0.0, 0.275, 0.5, 0.725, 0.95];
        for (got, want) in ys.iter().zip(expected.iter()) {
            assert!(
                (got - want).abs() < 1e-6,
                "strength=0.5 时 y 应≈{want}，实得 {got}"
            );
        }
        let mut params = json!({"channel": "rgb", "points": [[0, 0], [0.25, 0.3], [0.5, 0.5], [0.75, 0.7], [1, 0.9]]});
        scale_params("curves", &mut params, 0.0);
        let points = params["points"].as_array().expect("points 应为数组");
        for point in points {
            let (x, y) = (
                point[0].as_f64().expect("x 应为数"),
                point[1].as_f64().expect("y 应为数"),
            );
            assert!(
                (y - x).abs() < 1e-9,
                "strength=0 时曲线应回恒等，实得 ({x}, {y})"
            );
        }
    }

    /// p3b-2 plan_auto：curves 组在 WB 之后、色彩之前落成；GROUPS
    /// 共 8 组且含 curves。变异：删掉 curves 块 ⇒ 无 auto_curves ⇒
    /// 红；挪到色彩后 ⇒ 顺序断言红。
    #[test]
    fn plan_auto_curves_after_wb_before_color() {
        assert_eq!(GROUPS.len(), 8, "GROUPS 应为 8 组");
        assert!(GROUPS.contains(&"curves"), "GROUPS 应含 curves");
        let stats = stats_with(|stats| {
            stats.linear_p25 = 0.00381;
            stats.linear_p50 = 0.01208;
            stats.linear_p95 = 0.14856;
            stats.linear_p998 = 0.40539;
            stats.clip_high_ratio = 0.0;
            stats.channel_clip_high = [0.000126, 0.0, 0.0];
            stats.mean_saturation = 0.3;
        });
        let inputs = EstimateInputs {
            stats: &stats,
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
        let index = |object_id: &str| {
            plan.objects
                .iter()
                .position(|object| object.object_id == object_id)
        };
        let tone = index("auto_tone").expect("影调应落成");
        let curves = index("auto_curves").expect("曲线应落成");
        let vibrance = index("auto_vibrance").expect("vibrance 应落成");
        assert!(
            tone < curves && curves < vibrance,
            "curves 应在 tone 之后、vibrance 之前，实得 tone={tone} curves={curves} vibrance={vibrance}"
        );
        let object = &plan.objects[curves];
        assert_eq!(object.group, "curves");
        assert_eq!(object.type_name, "curves");
        assert_eq!(object.params["channel"], json!("rgb"));
        assert_eq!(
            object.params["points"].as_array().map(Vec::len),
            Some(5),
            "RGB S 曲线应有 5 个控制点"
        );
    }
}
