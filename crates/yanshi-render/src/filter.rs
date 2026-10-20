//! 调整与滤镜内核（设计文档 4.2 / 6.1）。
//!
//! 调整（Adjustment）与滤镜（Filter）都是**对象**：作用于同图层中位于其下方（更低 `z_index`）
//! 的内容，符合设计文档「图层只做容器，调整对象化」的定义。
//!
//! 所有运算在**线性光、直通颜色**上执行，再回到预乘表示，保证与合成空间一致（D0 基线）。
//! 需要一个 `radius` 邻域的滤镜（模糊）要求调用方提供带 padding 的缓冲区；
//! [`crate::render`] 会按文档内最大滤镜半径扩展渲染区域后再裁剪。

use crate::buffer::Buffer;
use crate::color::{linear_to_srgb, srgb_to_linear};
use crate::prng::Prng;
use serde_json::Value;

/// 调整类型（`adjustment_type` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdjustmentKind {
    /// 亮度/对比度。
    BrightnessContrast,
    /// 饱和度。
    Saturation,
    /// 反相。
    Invert,
    /// 色阶（简化：输入黑白点 + gamma）。
    Levels,
    /// 曝光（EV）。
    Exposure,
    /// 白平衡（色温 + 色调）。
    WhiteBalance,
    /// 曲线（单调三次插值）。
    Curves,
    /// 色相/饱和度/明度。
    Hsl,
    /// 色调分离。
    Posterize,
    /// 色彩平衡（阴影/中间调/高光）。
    ColorBalance,
    /// 分离色调。
    SplitToning,
    /// 自然饱和度。
    Vibrance,
    /// 黑白转换（加权灰阶 + 可选着色）。
    BlackAndWhite,
    /// 通道混合器（3×3 矩阵 + 常数）。
    ChannelMixer,
    /// 渐变映射（按明度在色带间插值）。
    GradientMap,
    /// 阴影/高光恢复。
    ShadowsHighlights,
    /// 可选颜色（PS 相对模式简化版）。
    SelectiveColor,
    /// LUT（.cube 三线性插值）。
    ColorLut,
}

impl AdjustmentKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "brightness_contrast" | "brightness" => Some(Self::BrightnessContrast),
            "saturation" | "vibrance" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            "levels" => Some(Self::Levels),
            "exposure" => Some(Self::Exposure),
            "white_balance" => Some(Self::WhiteBalance),
            "curves" => Some(Self::Curves),
            "hsl" | "hue_saturation" => Some(Self::Hsl),
            "posterize" => Some(Self::Posterize),
            "color_balance" => Some(Self::ColorBalance),
            "split_toning" => Some(Self::SplitToning),
            // `saturation` 的同义词之一，保持向后兼容。
            "vibrance_natural" => Some(Self::Vibrance),
            "black_and_white" => Some(Self::BlackAndWhite),
            "channel_mixer" => Some(Self::ChannelMixer),
            "gradient_map" => Some(Self::GradientMap),
            "shadows_highlights" => Some(Self::ShadowsHighlights),
            "selective_color" => Some(Self::SelectiveColor),
            "color_lut" => Some(Self::ColorLut),
            _ => None,
        }
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BrightnessContrast => "brightness_contrast",
            Self::Saturation => "saturation",
            Self::Invert => "invert",
            Self::Levels => "levels",
            Self::Exposure => "exposure",
            Self::WhiteBalance => "white_balance",
            Self::Curves => "curves",
            Self::Hsl => "hsl",
            Self::Posterize => "posterize",
            Self::ColorBalance => "color_balance",
            Self::SplitToning => "split_toning",
            Self::Vibrance => "vibrance",
            Self::BlackAndWhite => "black_and_white",
            Self::ChannelMixer => "channel_mixer",
            Self::GradientMap => "gradient_map",
            Self::ShadowsHighlights => "shadows_highlights",
            Self::SelectiveColor => "selective_color",
            Self::ColorLut => "color_lut",
        }
    }

    /// 全部类型。
    pub const ALL: [AdjustmentKind; 18] = [
        Self::BrightnessContrast,
        Self::Saturation,
        Self::Invert,
        Self::Levels,
        Self::Exposure,
        Self::WhiteBalance,
        Self::Curves,
        Self::Hsl,
        Self::Posterize,
        Self::ColorBalance,
        Self::SplitToning,
        Self::Vibrance,
        Self::BlackAndWhite,
        Self::ChannelMixer,
        Self::GradientMap,
        Self::ShadowsHighlights,
        Self::SelectiveColor,
        Self::ColorLut,
    ];
}

/// 滤镜类型（`filter_name` 字段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// 方框模糊（可多趟近似高斯）。
    BoxBlur,
    /// 高斯模糊（按 sigma 采样，确定性）。
    GaussianBlur,
    /// 亮度/对比度（滤镜形式）。
    BrightnessContrast,
    /// 饱和度。
    Saturation,
    /// 反相。
    Invert,
    /// 动感模糊（方向 + 距离）。
    MotionBlur,
    /// 锐化（非锐化掩模）。
    Sharpen,
    /// 噪点（确定性）。
    Noise,
    /// 暗角。
    Vignette,
    /// 辉光（bloom）。
    Glow,
    /// 局部对比度（clarity）。
    Clarity,
    /// 去雾（暗通道先验，大气光由参数给出）。
    Dehaze,
    /// 胶片颗粒。
    FilmGrain,
    /// **厚涂浮雕（P2 #15）**：alpha 当高度场，法向 + 光源的明度偏置。
    Relief,
}

impl FilterKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "box_blur" | "blur" => Some(Self::BoxBlur),
            "gaussian_blur" => Some(Self::GaussianBlur),
            "motion_blur" => Some(Self::MotionBlur),
            "sharpen" => Some(Self::Sharpen),
            "noise" => Some(Self::Noise),
            "vignette" => Some(Self::Vignette),
            "glow" => Some(Self::Glow),
            "clarity" => Some(Self::Clarity),
            "dehaze" => Some(Self::Dehaze),
            "film_grain" => Some(Self::FilmGrain),
            "relief" | "emboss" | "impasto" => Some(Self::Relief),
            "brightness_contrast" => Some(Self::BrightnessContrast),
            "saturation" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            _ => None,
        }
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BoxBlur => "box_blur",
            Self::GaussianBlur => "gaussian_blur",
            Self::MotionBlur => "motion_blur",
            Self::Sharpen => "sharpen",
            Self::Noise => "noise",
            Self::Vignette => "vignette",
            Self::Glow => "glow",
            Self::Clarity => "clarity",
            Self::Dehaze => "dehaze",
            Self::FilmGrain => "film_grain",
            Self::Relief => "relief",
            Self::BrightnessContrast => "brightness_contrast",
            Self::Saturation => "saturation",
            Self::Invert => "invert",
        }
    }

    /// 该滤镜需要的邻域半径（像素）。
    pub fn padding(&self, params: &Value) -> u32 {
        let radius = params
            .get("radius")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let sigma = params
            .get("sigma")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        let passes = params
            .get("passes")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, 4);
        let distance = params
            .get("distance")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .max(0.0);
        match self {
            Self::BoxBlur => (radius.ceil() as u32) * passes as u32,
            Self::GaussianBlur => (sigma * 3.0).ceil() as u32,
            // 动感模糊沿 `distance` 采样、锐化需要方框模糊的半径，区域渲染必须外扩，
            // 否则边缘会采到画外产生接缝。
            Self::MotionBlur => (distance / 2.0).ceil() as u32,
            Self::Sharpen => radius.ceil() as u32,
            // 两趟方框模糊，邻域按 2 倍半径申报（glow 与 clarity 同理）。
            Self::Glow | Self::Clarity => (radius.ceil() as u32) * 2,
            // 去雾内部对透射率图做 radius=4 的模糊，固定外扩 8。
            Self::Dehaze => 8,
            // 胶片颗粒逐像素（块坐标）计算，无邻域。
            Self::FilmGrain => 0,
            _ => 0,
        }
    }
}

/// **黑白转换**：`gray = Σ w·c`（权重由参数给、缺省 Rec709），可选 `tint` 着色
/// （输出 `gray × tint`），无 `tint` 时三通道同值。
///
/// 断言应写**关系**而不是单点 ✓：灰像素（r=g=b）且 `Σw = 1` 时原样返回；
/// 纯红配 `weights=[1,0,0]` ⇒ 输出 = 红通道值 ✓。
pub fn black_and_white(buffer: &mut Buffer, weights: [f32; 3], tint: Option<[f32; 3]>) {
    for_each_straight_color(buffer, |color| {
        let gray = weights[0] * color[0] + weights[1] * color[1] + weights[2] * color[2];
        match tint {
            Some(tint) => {
                color[0] = (gray * tint[0]).clamp(0.0, 1.0);
                color[1] = (gray * tint[1]).clamp(0.0, 1.0);
                color[2] = (gray * tint[2]).clamp(0.0, 1.0);
            }
            None => {
                let gray = gray.clamp(0.0, 1.0);
                *color = [gray, gray, gray];
            }
        }
    });
}

/// **通道混合器**：`out_i = Σ_j M[i][j]·in_j + K[i]`，输出钳制 [0,1]。
///
/// 单位阵 + 零常数 ⇔ 恒等 ✓（这一条同时封住「行列写反」与「常数漏加」两类错 ✓）。
pub fn channel_mixer(buffer: &mut Buffer, matrix: [[f32; 3]; 3], constants: [f32; 3]) {
    for_each_straight_color(buffer, |color| {
        let input = *color;
        for (index, slot) in color.iter_mut().enumerate() {
            *slot = (matrix[index][0] * input[0]
                + matrix[index][1] * input[1]
                + matrix[index][2] * input[2]
                + constants[index])
                .clamp(0.0, 1.0);
        }
    });
}

/// **渐变映射**：以线性 Rec709 明度为横坐标，在 `stops`（[位置, 颜色]，位置 0..1）
/// 之间线性插值。`stops` 少于 2 个时退化为黑白渐变（等同 Rec709 灰阶）——
/// 形状校验在工具层做 ✓，渲染层与 `curves` 控制点不足时同口径（不报错、不假装生效）✓。
pub fn gradient_map(buffer: &mut Buffer, stops: &[(f32, [f32; 3])]) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let fallback = [(0.0, [0.0, 0.0, 0.0]), (1.0, [1.0, 1.0, 1.0])];
    let stops: &[(f32, [f32; 3])] = if stops.len() >= 2 { stops } else { &fallback };
    for_each_straight_color(buffer, |color| {
        let luma = (LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2]).clamp(0.0, 1.0);
        *color = gradient_sample(stops, luma);
    });
}

/// 色带取样：`stops` 须按位置升序、至少 2 个；区间外钳制到端点色
/// ⇒ 横坐标恰为 0／1 时**精确**命中端点 stop ✓。
fn gradient_sample(stops: &[(f32, [f32; 3])], x: f32) -> [f32; 3] {
    let first = stops[0];
    if x <= first.0 {
        return first.1;
    }
    for window in stops.windows(2) {
        let (p0, c0) = window[0];
        let (p1, c1) = window[1];
        if x <= p1 {
            let span = (p1 - p0).max(f32::EPSILON);
            let t = ((x - p0) / span).clamp(0.0, 1.0);
            return [
                c0[0] + (c1[0] - c0[0]) * t,
                c0[1] + (c1[1] - c0[1]) * t,
                c0[2] + (c1[2] - c0[2]) * t,
            ];
        }
    }
    stops[stops.len() - 1].1
}

/// **阴影/高光恢复**：`shadows`／`highlights` 各 -1..1，0 = 不动（恒等 ✓）。
///
/// 掩码按线性明度：暗部权重 `(1−L)²`、高光权重 `L²` ⇒ 白色处暗部权重**恰好 0**、
/// 黑色处高光权重**恰好 0**（不是近似 ✓）⇒
/// 「shadows>0 只提暗部」「highlights<0 只压高光」两条性质精确成立 ✓。
/// 每通道朝目标端移动 `参数 × 权重 × 剩余距离 × 0.5` ⇒ 移动量不超过半程、永不越界 ✓。
pub fn shadows_highlights(buffer: &mut Buffer, shadows: f32, highlights: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let shadows = shadows.clamp(-1.0, 1.0);
    let highlights = highlights.clamp(-1.0, 1.0);
    if shadows == 0.0 && highlights == 0.0 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        let luma = (LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2]).clamp(0.0, 1.0);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        for value in color.iter_mut() {
            let current = *value;
            let mut next = current + toward(shadows, shadow_weight, current);
            next += toward(highlights, highlight_weight, current);
            *value = next.clamp(0.0, 1.0);
        }
    });
}

/// 朝向移动量：正参数朝 1、负参数朝 0，幅度 = `参数 × 权重 × 到该端的距离 × 0.5`
/// （参数自带符号 ⇒ 负参数的结果自然为负 ✓）。
fn toward(amount: f32, weight: f32, value: f32) -> f32 {
    let room = if amount >= 0.0 { 1.0 - value } else { value };
    amount * weight * room * 0.5
}

/// **可选颜色**的 9 个色类名（参数 `adjust` 的键；工具层校验与渲染层共用 ⇒ 只此一处 ✓）。
pub const SELECTIVE_COLOR_RANGES: [&str; 9] = [
    "reds", "yellows", "greens", "cyans", "blues", "magentas", "whites", "neutrals", "blacks",
];

/// **可选颜色**（PS 相对模式的简化版）：RGB → CMYK 后，按像素色类取出**唯一**一档的
/// `[c,m,y,k]` 偏移（下标见 [`SELECTIVE_COLOR_RANGES`]）施加，再转回 RGB。
///
/// 色类判定（硬扇区、不羽化）：低饱和（色度 < 0.08）按 HSL 明度分
/// whites（> 0.75）／blacks（< 0.25）／neutrals；否则按色相 60° 扇区
/// （扇区中心对齐红/黄/绿/青/蓝/品红）。
///
/// **相对模式的口径**：偏移 δ 加到分量 v 上时按「到目标端的剩余量」缩放——
/// `v' = v + δ·(δ ≥ 0 ? 1 − v : v)`。若按 PS 字面 `δ·v` 算，纯红的 c 恒为 0 ⇒
/// reds 的 cyan 滑杆永远无效（静默失效 ✗）；剩余量口径下它生效，
/// 且结果天然落在 [0,1]。全零偏移 ⇒ CMYK 往返恒等 ✓。
pub fn selective_color(buffer: &mut Buffer, ranges: &[[f32; 4]; 9]) {
    for_each_straight_color(buffer, |color| {
        let offset = ranges[selective_range_index(color)];
        if offset == [0.0; 4] {
            return;
        }
        let mut cmyk = rgb_to_cmyk(*color);
        for (slot, delta) in cmyk.iter_mut().zip(offset.iter()) {
            let value = *slot;
            let room = if *delta >= 0.0 { 1.0 - value } else { value };
            *slot = (value + delta * room).clamp(0.0, 1.0);
        }
        *color = cmyk_to_rgb(cmyk);
    });
}

/// 色类下标：规则见 [`selective_color`] 的文档。
fn selective_range_index(color: &[f32; 3]) -> usize {
    let (r, g, b) = (color[0], color[1], color[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    if chroma < 0.08 {
        let lightness = (max + min) / 2.0;
        return if lightness > 0.75 {
            6 // whites
        } else if lightness < 0.25 {
            8 // blacks
        } else {
            7 // neutrals
        };
    }
    // 色相（0..6，整数位对齐红/黄/绿/青/蓝/品红）。
    let hue = if max == r {
        ((g - b) / chroma).rem_euclid(6.0)
    } else if max == g {
        (b - r) / chroma + 2.0
    } else {
        (r - g) / chroma + 4.0
    };
    // 扇区以原色为中心 ⇒ 把色相平移半格再取整。
    ((hue + 0.5) % 6.0) as usize
}

/// RGB → CMYK（朴素减色模型）：`k = 1 − max`，其余 `(1 − c − k)/(1 − k)`；
/// 纯黑（k = 1）时 c/m/y 取 0 防除零。
fn rgb_to_cmyk(color: [f32; 3]) -> [f32; 4] {
    let max = color[0].max(color[1]).max(color[2]);
    let k = 1.0 - max;
    if k >= 1.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let denom = 1.0 - k;
    [
        (1.0 - color[0] - k) / denom,
        (1.0 - color[1] - k) / denom,
        (1.0 - color[2] - k) / denom,
        k,
    ]
}

/// CMYK → RGB：[`rgb_to_cmyk`] 的逆（`(1 − c)(1 − k)`）⇒ 无偏移时往返恒等 ✓。
fn cmyk_to_rgb(cmyk: [f32; 4]) -> [f32; 3] {
    let keep = 1.0 - cmyk[3];
    [
        ((1.0 - cmyk[0]) * keep).clamp(0.0, 1.0),
        ((1.0 - cmyk[1]) * keep).clamp(0.0, 1.0),
        ((1.0 - cmyk[2]) * keep).clamp(0.0, 1.0),
    ]
}

/// **颜色 LUT**（纯函数 ✓，不碰文件系统）：`table` 为 `size³ × 3` 扁平表、
/// **红最快**（与 `.cube` 的 `LUT_3D_SIZE` 数据顺序一致 ✓），逐像素三线性插值。
/// 表的解析与形状校验在工具层完成（参数写入文档前）；
/// 形状不符（`size < 2` 或长度不对）时原样返回——渲染层对历史文档不许炸，
/// 与 `curves` 控制点不足时同口径 ✓。
pub fn color_lut(buffer: &mut Buffer, size: u32, table: &[f32]) {
    let size = size as usize;
    if size < 2 || table.len() != size * size * size * 3 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        *color = lut_sample(size, table, *color);
    });
}

/// 三线性取样：坐标 = `通道值 × (size − 1)`；落在格点上时权重全给该格点 ⇒ 精确命中 ✓。
fn lut_sample(size: usize, table: &[f32], color: [f32; 3]) -> [f32; 3] {
    let scale = (size - 1) as f32;
    let mut lo = [0usize; 3];
    let mut frac = [0.0f32; 3];
    for axis in 0..3 {
        let pos = color[axis].clamp(0.0, 1.0) * scale;
        let base = pos.floor() as usize;
        if base >= size - 1 {
            lo[axis] = size - 2;
            frac[axis] = 1.0;
        } else {
            lo[axis] = base;
            frac[axis] = pos - base as f32;
        }
    }
    let mut out = [0.0f32; 3];
    for dz in 0..2usize {
        for dy in 0..2usize {
            for dx in 0..2usize {
                let weight = corner_weight(dx, frac[0])
                    * corner_weight(dy, frac[1])
                    * corner_weight(dz, frac[2]);
                if weight == 0.0 {
                    continue;
                }
                let index = (((lo[2] + dz) * size + (lo[1] + dy)) * size + (lo[0] + dx)) * 3;
                for (channel, slot) in out.iter_mut().enumerate() {
                    *slot += weight * table[index + channel];
                }
            }
        }
    }
    [
        out[0].clamp(0.0, 1.0),
        out[1].clamp(0.0, 1.0),
        out[2].clamp(0.0, 1.0),
    ]
}

fn corner_weight(offset: usize, frac: f32) -> f32 {
    if offset == 1 {
        frac
    } else {
        1.0 - frac
    }
}

/// 亮度/对比度（`brightness` 为 `-1..1` 的加性偏移，`contrast` 为 `0..2` 的乘性对比）。
pub fn brightness_contrast(buffer: &mut Buffer, brightness: f32, contrast: f32) {
    let offset = brightness;
    let scale = contrast.max(0.0);
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * scale + offset).clamp(0.0, 1.0);
        }
    });
}

/// 饱和度（`amount`：0 = 灰度，1 = 不变，>1 增强）。
pub fn saturation(buffer: &mut Buffer, amount: f32) {
    // 线性光空间的 Rec.709 亮度权重。
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.max(0.0);
    for_each_straight_color(buffer, |color| {
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        for value in color.iter_mut() {
            *value = (luma + (*value - luma) * amount).clamp(0.0, 1.0);
        }
    });
}

/// 反相（在线性光空间取补）。
pub fn invert(buffer: &mut Buffer) {
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = 1.0 - *value;
        }
    });
}

/// 色阶：`black`/`white` 为输入黑白点（`0..1`），`gamma` 为中间调。
/// 色阶：`channel` 为 `rgb` / `r` / `g` / `b`，可按通道单独调整。
pub fn levels_channel(buffer: &mut Buffer, black: f32, white: f32, gamma: f32, channel: &str) {
    let black = black.clamp(0.0, 0.999);
    let white = white.clamp(black + 1e-3, 1.0);
    let inverse_gamma = 1.0 / gamma.max(1e-3);
    let map = |value: f32| {
        let normalized = ((value - black) / (white - black)).clamp(0.0, 1.0);
        libm::powf(normalized, inverse_gamma)
    };
    for_each_straight_color(buffer, |color| match channel {
        "r" => color[0] = map(color[0]),
        "g" => color[1] = map(color[1]),
        "b" => color[2] = map(color[2]),
        _ => {
            for value in color.iter_mut() {
                *value = map(*value);
            }
        }
    });
}

/// 色阶（三通道同时）。
pub fn levels(buffer: &mut Buffer, black: f32, white: f32, gamma: f32) {
    levels_channel(buffer, black, white, gamma, "rgb");
}

/// 自然饱和度：**低饱和**像素提升更强，高饱和像素几乎不动（避免过饱和与色偏）。
///
/// 权重取 `1 - 当前饱和度`（直通空间的 `chroma / max`），因此灰色不产生色偏（权重为 0
/// 时差值也为 0），已饱和的颜色几乎不变。`amount` 为 0 时不改动。
pub fn vibrance(buffer: &mut Buffer, amount: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 2.0);
    if amount <= 0.0 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        let max = color[0].max(color[1]).max(color[2]);
        let min = color[0].min(color[1]).min(color[2]);
        let saturation = if max <= 0.0 {
            0.0
        } else {
            ((max - min) / max).clamp(0.0, 1.0)
        };
        // 低饱和 → 权重接近 1；已饱和 → 接近 0。
        let weight = 1.0 - saturation;
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        for value in color.iter_mut() {
            let boosted = *value + (*value - luma) * amount * weight;
            *value = boosted.clamp(0.0, 1.0);
        }
    });
}

/// 胶片颗粒：按文档坐标 + `seed` 生成的确定性颗粒，颗粒尺寸可分块（`size` 像素见方）。
///
/// 与 [`noise`] 同一条纪律：随机数序号**只由文档坐标与显式 seed 决定**，
/// 绝不用缓冲内序号 —— 否则同一像素在不同 tile 分块下会得到不同颗粒，
/// 客户端按 tile 组合渲染就会与服务端不一致（这条坑此前踩过）。
///
/// 与 `noise` 的区别：颗粒按 `size` 聚成块（更接近胶片），并按亮度加权
/// （`1-(2·luma-1)²`），使中间调颗粒最明显、纯黑纯白处收敛，避免"塑料噪点"。
pub fn film_grain(buffer: &mut Buffer, amount: f32, size: u32, seed: u64) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let size = size.clamp(1, 8) as i64;
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x as i64 + x as i64;
            let document_y = origin.y as i64 + y as i64;
            // 以 size 为粒度取块坐标：块内所有像素共享同一颗粒值。
            let block_x = document_x.div_euclid(size);
            let block_y = document_y.div_euclid(size);
            let index = (block_y as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(block_x as u64);
            let mut rng = Prng::derive(seed, index);
            let grain = rng.signed();
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            let luma =
                (LUMA[0] * pixel[0] + LUMA[1] * pixel[1] + LUMA[2] * pixel[2]) / alpha.max(1e-3);
            let centered = 2.0 * luma.clamp(0.0, 1.0) - 1.0;
            let weight = 1.0 - centered * centered;
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                // 颗粒为加性（胶片颗粒是密度扰动），按 alpha 缩放以保持预乘不变量。
                let delta = grain * amount * weight * 0.25 * alpha;
                out[channel] = pixel[channel] + delta;
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 色彩平衡：按亮度分档（阴影 / 中间调 / 高光）分别做 RGB 偏移。
///
/// 档位权重由线性亮度推导：`shadows = (1-luma)²`、`highlights = luma²`、
/// `midtones = 1 - shadows - highlights`。每个分量取值 -1..1，按档位加权后加到直通通道上。
pub fn color_balance(
    buffer: &mut Buffer,
    shadows: [f32; 3],
    midtones: [f32; 3],
    highlights: [f32; 3],
) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let clamp_component = |value: f32| value.clamp(-1.0, 1.0);
    let shadows = shadows.map(clamp_component);
    let midtones = midtones.map(clamp_component);
    let highlights = highlights.map(clamp_component);
    for_each_straight_color(buffer, |color| {
        let luma = LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2];
        let luma = luma.clamp(0.0, 1.0);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let midtone_weight = (1.0 - shadow_weight - highlight_weight).max(0.0);
        for channel in 0..3 {
            let delta = shadows[channel] * shadow_weight
                + midtones[channel] * midtone_weight
                + highlights[channel] * highlight_weight;
            color[channel] = (color[channel] + delta).clamp(0.0, 1.0);
        }
    });
}

/// 分离色调：阴影与高光分别着色，`balance` 移动分界点，`amount` 控制总强度。
///
/// 权重用 smoothstep 在 `mid ± 0.25` 之间过渡（`mid = 0.5 + balance × 0.25`），
/// 因此权重单调、确定，且 `balance` 的语义直观（正值把分界推向亮部）。
pub fn split_toning(
    buffer: &mut Buffer,
    shadows: [f32; 3],
    highlights: [f32; 3],
    balance: f32,
    amount: f32,
) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    let shadows = shadows.map(|value| value.clamp(-1.0, 1.0));
    let highlights = highlights.map(|value| value.clamp(-1.0, 1.0));
    let mid = 0.5 + balance.clamp(-1.0, 1.0) * 0.25;
    for_each_straight_color(buffer, |color| {
        let luma = (LUMA[0] * color[0] + LUMA[1] * color[1] + LUMA[2] * color[2]).clamp(0.0, 1.0);
        let t = ((luma - (mid - 0.25)) / 0.5).clamp(0.0, 1.0);
        let highlight_weight = t * t * (3.0 - 2.0 * t);
        let shadow_weight = 1.0 - highlight_weight;
        for channel in 0..3 {
            let tint = shadows[channel] * shadow_weight + highlights[channel] * highlight_weight;
            color[channel] = (color[channel] + amount * tint).clamp(0.0, 1.0);
        }
    });
}

/// 色调分离：把每个直通通道量化到 `levels` 个台阶。
pub fn posterize(buffer: &mut Buffer, levels: u32) {
    let levels = levels.clamp(2, 64);
    let steps = (levels - 1) as f32;
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * steps).round() / steps;
        }
    });
}

/// **RGB → HSL 分解**（h/s/l 都是 0..1 的分数）✓ —— `hsl` 调整与 `curves` 的 h/s 通道共用 ✓，
/// 口径只此一处 ✗（两边各算一遍必然漂移 ✓）。
fn hsl_decompose(color: &[f32; 3]) -> (f32, f32, f32) {
    let (r, g, b) = (color[0], color[1], color[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let chroma = max - min;
    let l = (max + min) / 2.0;
    let h = if chroma <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / chroma).rem_euclid(6.0) / 6.0
    } else if max == g {
        (((b - r) / chroma) + 2.0) / 6.0
    } else {
        (((r - g) / chroma) + 4.0) / 6.0
    };
    let s = if l <= 0.0 || l >= 1.0 {
        0.0
    } else {
        chroma / (1.0 - (2.0 * l - 1.0).abs())
    };
    (h, s, l)
}

/// **HSL → RGB 组装** ✓ —— `hsl_decompose` 的逆 ✓。
fn hsl_compose(h: f32, s: f32, l: f32) -> [f32; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h * 6.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        (r1 + m).clamp(0.0, 1.0),
        (g1 + m).clamp(0.0, 1.0),
        (b1 + m).clamp(0.0, 1.0),
    ]
}

/// HSL 调整：色相旋转（度）+ 饱和度倍率 + 明度偏移。
///
/// 与其它调整一致，作用于**直通线性**通道并钳制到 [0,1]。HSL 采用标准定义：
/// 色相 0°=红、120°=绿、240°=蓝；`lightness` 为 HSL 明度的加性偏移。
pub fn hsl(buffer: &mut Buffer, hue_degrees: f32, saturation: f32, lightness: f32) {
    let hue_shift = hue_degrees.clamp(-360.0, 360.0) / 360.0;
    let saturation = saturation.clamp(0.0, 8.0);
    let lightness = lightness.clamp(-1.0, 1.0);
    for_each_straight_color(buffer, |color| {
        let (h, s, l) = hsl_decompose(color);
        // 色相旋转 + 饱和度缩放 + 明度偏移。
        let h = (h + hue_shift).rem_euclid(1.0);
        let s = (s * saturation).clamp(0.0, 1.0);
        let l = (l + lightness).clamp(0.0, 1.0);
        *color = hsl_compose(h, s, l);
    });
}

/// 曝光：线性光下按 EV 整体缩放（`2^ev`）。
pub fn exposure(buffer: &mut Buffer, ev: f32) {
    let factor = libm::powf(2.0, ev.clamp(-10.0, 10.0));
    for_each_straight_color(buffer, |color| {
        for value in color.iter_mut() {
            *value = (*value * factor).clamp(0.0, 1.0);
        }
    });
}

/// 白平衡：`temperature` 正=暖（红升蓝降），`tint` 正=品红（绿降）。
pub fn white_balance(buffer: &mut Buffer, temperature: f32, tint: f32) {
    let temperature = temperature.clamp(-1.0, 1.0);
    let tint = tint.clamp(-1.0, 1.0);
    let warm = 0.35 * temperature;
    let magenta = 0.15 * tint;
    for_each_straight_color(buffer, |color| {
        color[0] = (color[0] * (1.0 + warm + magenta)).clamp(0.0, 1.0);
        color[1] = (color[1] * (1.0 - magenta)).clamp(0.0, 1.0);
        color[2] = (color[2] * (1.0 - warm + magenta)).clamp(0.0, 1.0);
    });
}

/// 单调三次插值（Fritsch–Carlson），用于曲线：保证不过冲、不振荡。
fn monotone_curve(points: &[(f32, f32)], x: f32) -> f32 {
    let n = points.len();
    if n == 0 {
        return x;
    }
    if n == 1 {
        return points[0].1;
    }
    if x <= points[0].0 {
        return points[0].1;
    }
    if x >= points[n - 1].0 {
        return points[n - 1].1;
    }
    // 各段斜率。
    let mut slopes = [0.0f32; 64];
    let mut tangents = [0.0f32; 64];
    for i in 0..n - 1 {
        let dx = (points[i + 1].0 - points[i].0).max(1e-6);
        slopes[i] = (points[i + 1].1 - points[i].1) / dx;
    }
    tangents[0] = slopes[0];
    tangents[n - 1] = slopes[n - 2];
    for i in 1..n - 1 {
        if slopes[i - 1] * slopes[i] <= 0.0 {
            tangents[i] = 0.0;
        } else {
            tangents[i] = (slopes[i - 1] + slopes[i]) / 2.0;
        }
    }
    // Fritsch–Carlson 限制，避免过冲。
    for i in 0..n - 1 {
        if slopes[i].abs() < 1e-9 {
            tangents[i] = 0.0;
            tangents[i + 1] = 0.0;
            continue;
        }
        let a = tangents[i] / slopes[i];
        let b = tangents[i + 1] / slopes[i];
        if a < 0.0 {
            tangents[i] = 0.0;
        }
        if b < 0.0 {
            tangents[i + 1] = 0.0;
        }
        let (a, b) = (tangents[i] / slopes[i], tangents[i + 1] / slopes[i]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            tangents[i] = t * a * slopes[i];
            tangents[i + 1] = t * b * slopes[i];
        }
    }
    let mut segment = 0usize;
    for i in 0..n - 1 {
        if x >= points[i].0 && x <= points[i + 1].0 {
            segment = i;
            break;
        }
    }
    let (x0, y0) = points[segment];
    let (x1, y1) = points[segment + 1];
    let h = (x1 - x0).max(1e-6);
    let t = (x - x0) / h;
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    (h00 * y0 + h10 * h * tangents[segment] + h01 * y1 + h11 * h * tangents[segment + 1])
        .clamp(0.0, 1.0)
}

/// 曲线：控制点在 0..1 归一化空间，可按通道分别作用。
/// 通道：`rgb`/`r`/`g`/`b`（线性 RGB）、`l`（Lab 明度：调亮不偏色）、
/// `h`（HSL 色相：输入色相→输出色相）、`s`（HSL 饱和度）。
pub fn curves(buffer: &mut Buffer, points: &[(f32, f32)], channel: &str) {
    if points.len() < 2 {
        return;
    }
    let mut sorted: Vec<(f32, f32)> = points.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let apply = |color: &mut [f32; 3]| match channel {
        "r" => color[0] = monotone_curve(&sorted, color[0].clamp(0.0, 1.0)),
        "g" => color[1] = monotone_curve(&sorted, color[1].clamp(0.0, 1.0)),
        "b" => color[2] = monotone_curve(&sorted, color[2].clamp(0.0, 1.0)),
        // **Lab 明度曲线** ✓：只动 L，a/b 原样 ⇒ 提亮不偏色（RGB 曲线做不到 ✓）。
        "l" => {
            let lab = crate::color::linear_to_lab(*color);
            let new_l = monotone_curve(&sorted, (lab[0] / 100.0).clamp(0.0, 1.0)) * 100.0;
            let back = crate::color::lab_to_linear([new_l, lab[1], lab[2]]);
            color[0] = back[0].clamp(0.0, 1.0);
            color[1] = back[1].clamp(0.0, 1.0);
            color[2] = back[2].clamp(0.0, 1.0);
        }
        // **HSL 色相曲线** ✓：输入色相（0..1）→输出色相，自动回绕 ✓。
        // 无彩像素（s=0）组装回去仍是灰 ⇒ 曲线动不了它 ✓（符合直觉 ✓）。
        "h" => {
            let (h, s, l) = hsl_decompose(color);
            let new_h = monotone_curve(&sorted, h).rem_euclid(1.0);
            *color = hsl_compose(new_h, s, l);
        }
        // **HSL 饱和度曲线** ✓。
        "s" => {
            let (h, s, l) = hsl_decompose(color);
            let new_s = monotone_curve(&sorted, s.clamp(0.0, 1.0));
            *color = hsl_compose(h, new_s, l);
        }
        _ => {
            for value in color.iter_mut() {
                *value = monotone_curve(&sorted, value.clamp(0.0, 1.0));
            }
        }
    };
    for_each_straight_color(buffer, apply);
}

/// 动感模糊：沿给定角度按 `samples` 个采样点平均（预乘空间平均，边界按 clamp 复制）。
pub fn motion_blur(buffer: &mut Buffer, angle_degrees: f32, distance: f32, samples: u32) {
    let distance = distance.clamp(0.0, 512.0);
    let samples = samples.clamp(2, 64);
    if distance <= 0.0 {
        return;
    }
    let source = buffer.clone();
    let radians = angle_degrees.to_radians();
    let (dx, dy) = (libm::cosf(radians), libm::sinf(radians));
    let width = buffer.width();
    let height = buffer.height();
    // 采样偏移是**循环不变量**（只与 angle/distance/samples 有关），却在每个像素的每次采样里
    // 被重算：512² × 16 次采样下这是纯粹的重复浮点运算。提到循环外后，每个采样点使用的
    // 仍是同样的 `dx * t` / `dy * t` 值，加法顺序与舍入不变，因此**逐位等价**（D0 不受影响）。
    let offsets: Vec<(f32, f32)> = (0..samples)
        .map(|i| {
            // 采样覆盖 [-distance/2, +distance/2]。
            let t = (i as f32 / (samples - 1) as f32 - 0.5) * distance;
            (dx * t, dy * t)
        })
        .collect();
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 4];
            for (offset_x, offset_y) in &offsets {
                let sx = (x as f32 + offset_x).round().clamp(0.0, width as f32 - 1.0) as u32;
                let sy = (y as f32 + offset_y)
                    .round()
                    .clamp(0.0, height as f32 - 1.0) as u32;
                let pixel = source.pixel(sx, sy);
                for channel in 0..4 {
                    sum[channel] += pixel[channel];
                }
            }
            let scale = 1.0 / samples as f32;
            buffer.set_pixel(
                x,
                y,
                [
                    sum[0] * scale,
                    sum[1] * scale,
                    sum[2] * scale,
                    sum[3] * scale,
                ],
            );
        }
    }
}

/// 锐化：非锐化掩模（`src + amount * (src - blur)`），预乘空间并保持 rgb ≤ alpha。
pub fn sharpen(buffer: &mut Buffer, amount: f32, radius: u32) {
    let amount = amount.clamp(0.0, 5.0);
    if amount <= 0.0 {
        return;
    }
    let mut blurred = buffer.clone();
    box_blur(&mut blurred, radius.clamp(1, 8), 1);
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let low = blurred.pixel(x, y);
            let alpha = source[3];
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                let value = source[channel] + amount * (source[channel] - low[channel]);
                out[channel] = value.clamp(0.0, alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 噪点：按 `seed` 逐像素确定性生成（随机量只来自原子 seed，保证可复现）。
pub fn noise(buffer: &mut Buffer, amount: f32, seed: u64) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 {
        return;
    }
    // 噪声序号必须由**文档坐标**决定：客户端按 tile 组合渲染，
    // 若用缓冲内序号，同一像素在不同分块下会得到不同噪声（与服务端不一致）。
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x as i64 + x as i64;
            let document_y = origin.y as i64 + y as i64;
            let index = (document_y as u64)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(document_x as u64);
            let mut rng = Prng::derive(seed, index);
            let factor = 1.0 + amount * rng.signed();
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                out[channel] = (pixel[channel] * factor).clamp(0.0, alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 辉光：提取高光区域模糊后**加性**合成（bloom）。
///
/// `threshold` 为直通亮度阈值，`radius` 为方框模糊半径，`intensity` 为加性强度。
/// 合成后把 rgb 钳制到 alpha，保持预乘不变量。
pub fn glow(buffer: &mut Buffer, threshold: f32, radius: u32, intensity: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let threshold = threshold.clamp(0.0, 1.0);
    let intensity = intensity.clamp(0.0, 4.0);
    if intensity <= 0.0 {
        return;
    }
    // 1) 高光掩模（预乘空间，阈值按直通亮度判定）。
    let mut highlights = Buffer::new(
        buffer.origin().0,
        buffer.origin().1,
        buffer.width(),
        buffer.height(),
    );
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let luma = LUMA[0] * pixel[0] + LUMA[1] * pixel[1] + LUMA[2] * pixel[2];
            if luma > threshold * alpha {
                highlights.set_pixel(x, y, pixel);
            }
        }
    }
    // 2) 模糊高光（两趟方框近似高斯）。
    box_blur(&mut highlights, radius.clamp(1, 64), 2);
    // 3) 加性合成。辉光同时增加 alpha（发光本身会让原本透明处出现光），
    //    再按**合成后**的 alpha 钳制，从而既保持预乘不变量又能扩散到笔迹之外。
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let bloom = highlights.pixel(x, y);
            let alpha = (source[3] + bloom[3] * intensity).clamp(0.0, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                out[channel] = (source[channel] + bloom[channel] * intensity).clamp(0.0, alpha);
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 局部对比度（clarity）：大半径非锐化掩模，并按亮度做中间调加权。
///
/// `detail = src - blur`，`out = src + amount × detail × midtone_weight`，
/// 其中 `midtone_weight = 1 - (2·luma - 1)²`（中间调最强，极暗/极亮处抑制），
/// 避免在纯黑纯白处产生光晕。输出钳制到 alpha，保持预乘不变量。
pub fn clarity(buffer: &mut Buffer, amount: f32, radius: u32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let amount = amount.clamp(0.0, 2.0);
    if amount <= 0.0 {
        return;
    }
    let mut blurred = buffer.clone();
    box_blur(&mut blurred, radius.clamp(2, 64), 2);
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let source = buffer.pixel(x, y);
            let alpha = source[3];
            let low = blurred.pixel(x, y);
            let luma =
                (LUMA[0] * source[0] + LUMA[1] * source[1] + LUMA[2] * source[2]).clamp(0.0, 1.0);
            let centered = 2.0 * luma - 1.0;
            let midtone_weight = (1.0 - centered * centered).clamp(0.0, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                let detail = source[channel] - low[channel];
                out[channel] = (source[channel] + amount * detail * midtone_weight)
                    .max(0.0)
                    .min(alpha.max(0.0));
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 去雾（暗通道先验）：`air` 为**调用方给定**的大气光，`omega` 控制去雾强度，
/// `floor` 是透射率下限（避免除零与过度放大噪声）。
///
/// 大气光必须是参数而不是在滤镜内部估计：否则同一像素在分块渲染与整幅渲染下会得到
/// 不同的全局统计量，`分块 == 整幅` 立刻失效（这类问题此前已踩过）。只读工具
/// `estimate_dehaze` 负责扫描文档给出建议值，再由调用方写进原子参数——这与
/// Phase 4b 的「先预览/估计、再作为 patch 应用」是同一套思路。
pub fn dehaze(buffer: &mut Buffer, air: [f32; 3], omega: f32, floor: f32) {
    let omega = omega.clamp(0.0, 1.0);
    let floor = floor.clamp(0.02, 0.8);
    let air = air.map(|value| value.clamp(0.05, 1.0));
    let air_luma = (0.2126 * air[0] + 0.7152 * air[1] + 0.0722 * air[2]).max(1e-3);
    let width = buffer.width();
    let height = buffer.height();
    // 透射率图（先算再模糊，抑制块状伪影）。
    let mut transmission = Buffer::new(buffer.origin().0, buffer.origin().1, width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            // 暗通道：直通空间的最小通道值。
            let dark = pixel[0].min(pixel[1]).min(pixel[2]);
            let t = (1.0 - omega * dark / air_luma).clamp(floor, 1.0);
            transmission.set_pixel(x, y, [t, t, t, 1.0]);
        }
    }
    box_blur(&mut transmission, 4, 1);
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let t = transmission.pixel(x, y)[0].clamp(floor, 1.0);
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                // 直通空间恢复后重新预乘。
                let straight = pixel[channel] / alpha;
                let recovered = (straight - air[channel]) / t + air[channel];
                out[channel] = (recovered.clamp(0.0, 1.0)) * alpha;
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

/// 暗通道均值与「大气光」估计（供只读工具 `estimate_dehaze` 使用）。
///
/// 返回 `(air, dark_mean)`：`air` 取暗通道最亮的那 0.1% 像素的平均颜色。
pub fn estimate_atmospheric_light(
    width: u32,
    height: u32,
    rgba8: &[u8],
) -> Option<([f32; 3], f32)> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba8.len() < expected || width == 0 || height == 0 {
        return None;
    }
    // 用 (暗通道, 索引) 排序取前 0.1%。
    let mut dark_channels: Vec<(f32, usize)> = Vec::with_capacity(expected / 4);
    let mut sum = 0.0f32;
    for index in 0..expected / 4 {
        let base = index * 4;
        let r = rgba8[base] as f32 / 255.0;
        let g = rgba8[base + 1] as f32 / 255.0;
        let b = rgba8[base + 2] as f32 / 255.0;
        let dark = r.min(g).min(b);
        sum += dark;
        dark_channels.push((dark, index));
    }
    let count = dark_channels.len().max(1);
    dark_channels.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let take = ((count as f32 * 0.001).ceil() as usize).max(1);
    let mut air = [0.0f32; 3];
    for (_, index) in dark_channels.iter().take(take) {
        let base = index * 4;
        air[0] += rgba8[base] as f32 / 255.0;
        air[1] += rgba8[base + 1] as f32 / 255.0;
        air[2] += rgba8[base + 2] as f32 / 255.0;
    }
    let scale = 1.0 / take as f32;
    for value in air.iter_mut() {
        *value = (*value * scale).clamp(0.05, 1.0);
    }
    Some((air, sum / count as f32))
}

/// 暗角：按到画布中心的归一化距离做平滑衰减。
pub fn vignette(
    buffer: &mut Buffer,
    strength: f32,
    radius: f32,
    softness: f32,
    canvas: (f64, f64),
) {
    let strength = strength.clamp(0.0, 1.0);
    if strength <= 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, 2.0);
    let softness = softness.clamp(0.02, 2.0);
    // 以**文档**中心与对角线为基准：tile 渲染时每块缓冲只是文档的一部分，
    // 若以缓冲自身为画布裁，块与块之间会出现明显接缝。
    let (canvas_w, canvas_h) = canvas;
    let (cx, cy) = (canvas_w / 2.0, canvas_h / 2.0);
    let max_distance = ((cx * cx + cy * cy).sqrt()).max(1e-3);
    let origin = buffer.bbox();
    for y in 0..buffer.height() {
        for x in 0..buffer.width() {
            let document_x = origin.x + x as f64;
            let document_y = origin.y + y as f64;
            let nx = ((document_x - cx) / max_distance) as f32;
            let ny = ((document_y - cy) / max_distance) as f32;
            let distance = (nx * nx + ny * ny).sqrt();
            let t = ((distance - radius) / softness).clamp(0.0, 1.0);
            let falloff = 1.0 - strength * (t * t * (3.0 - 2.0 * t));
            let pixel = buffer.pixel(x, y);
            buffer.set_pixel(
                x,
                y,
                [
                    pixel[0] * falloff,
                    pixel[1] * falloff,
                    pixel[2] * falloff,
                    pixel[3],
                ],
            );
        }
    }
}

/// 可分离方框模糊（`passes` 趟近似高斯；边缘按 clamp 复制）。
/// 水平/垂直方框模糊（滑动窗口，O(1)/像素；见文件内 `blur_horizontal` 的说明）。
/// **⚠️ 这不是唯一的方框模糊**（✓，第 371 轮 ✓）：`geometry::feather_coverage` 自带一份，
/// 因为**羽化需要"向外补零"的边界语义** ✓（输出 bbox 外扩、扩出来的一圈是 0 ✓），
/// 而本函数按自己的通用策略处理边界 ✗ ⇒ **两者不能直接互换** ✓
///（实测：换过去会让 `the_falloff_stays_bounded` 红 ✓）。
/// **若要合并**：先统一边界语义 ✓，并接受蒙版输出会变 ✗。
pub fn box_blur(buffer: &mut Buffer, radius: u32, passes: u32) {
    if radius == 0 {
        return;
    }
    for _ in 0..passes.max(1) {
        blur_horizontal(buffer, radius);
        blur_vertical(buffer, radius);
    }
}

/// 高斯模糊（按 `sigma` 生成一维核，可分离；核为确定性浮点权重）。
pub fn gaussian_blur(buffer: &mut Buffer, sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    // **★★★ GPU 分支（**滤镜 GPU 化 ✓）✗ ★★★**（**第 700 轮 ✓；用户第 82 轮的方向 ✓）
    //   **∴ 第 699 轮实测收益 ✗**（**含传输 ✓）：
    //     **∴ 1080p σ=2 ⇒ **2.0×**｜σ=8 ⇒ **3.6×**** ✓
    //       ＋ **∴ 4K σ=2 ⇒ **1.5×**｜σ=8 ⇒ **1.1×**** ✓
    //         ⇒ **∴ 全部档位**正向** ✓ ★
    //   **★★★ 第 246 轮**复测（**∴ 上面那组数**已经过期**✗ ★★★**）
    //     ＋ **∴ 环境 ✗**：**`deviceName = NVIDIA L4`｜`driverInfo 580.82.07`｜
    //       `adapter:Vulkan:DiscreteGpu` ✓
    //     ＋ **∴ 跑法 ✗**：`cargo test -p yanshi-render --release --features gpu
    //       --test gpu_blur_parity -- --ignored --nocapture` ✓（**∴ 它**是 `#[ignore]` 的 ✓）
    //     ＋ **∴ 读数（**中位，**含传输 ✓）✗**：
    //       **∴ 1080p σ=2 ⇒ **4.53×**（**CPU 153.5 ⇒ GPU 33.9 ms ✓）
    //         ＋ **∴ 1080p σ=8 ⇒ **7.29×**（**442.6 ⇒ 60.7 ✓）
    //         ＋ **∴ 4K σ=2 ⇒ **2.04×**（**598.0 ⇒ 293.7 ✓）
    //         ＋ **∴ 4K σ=8 ⇒ **7.54×**（**1665.2 ⇒ 221.0 ✓）
    //           ⇒ **∴ 四档**全部正向 ✗ 而且**比旧数大得多** ✓
    //     ＋ **∴ 所以 ✗**：**阈值那段的"依据"要按**新数**重读** ✓
    //       ⇒ **∴ 而**今天**仍然**保留 2 M**✗ —— **∴ 因为**用户第 246 轮的**短期口径**
    //         是**保守**（**∴ "**只让**超大模糊走 GPU**" ✓）✗
    //         ＋ **∴ 而**新数说明**交点**在 2.07 M **以下** ✗
    //           ⇒ **∴ 待办 ✗**：**把基准**扩到 2 M 以下的尺寸**✗ ⇒ **∴ 找到**真交点**
    //             ⇒ **∴ 那时才**能决定**降不降阈值** ✓（**∴ 先测再改 ✓）★★
    //   **∴ 阈值为什么是 2 M ✗**（**据第 699 轮的数选 ✓）：
    //     **∴ 1080p（**2.07 M ✓）⇒ **2.0–3.6×**✗ 而** 4K（**8.29 M ✓）⇒ 1.1–1.5×** ✓
    //       ＋ **∴ 而**小图上**传输（**~15 ms ✓）**盖过收益**✗（**∴ 第 79／84 轮同类现象 ✓）
    //         ＋ **∴ 所以**：**只有 ≥ 2 M 像素才走 GPU**✗
    //           ＋ **∴ 小于它 ⇒ CPU 本来就快**✗（**∴ 1080p σ=2 的 CPU 才 124 ms ✓）★
    //   **★ 用户第 246 轮的裁定 ✗ ★**：**GPU 只做**真正适合**的三件事**✗
    //     ⇒ **∴ ①** 笔刷渲染 ✗ **∴ ②** 画布呈现（**缩放／旋转／平移的贴屏 ✓）✗
    //       **∴ ③** **滤镜（**本条 ✓）** ✓
    //     ＋ **∴ 而**合成那条线**已归档** ✗（**∴ 实测 0.54–0.56× ✗ 负收益 ✓）★
    //   **∴ 回退 ✗**：**GPU 不可用／返回 `None` ⇒ **∴ 直接落到下面的 CPU 路** ✓ ★
    //   **∴ 逐位口径 ✗**：**GPU 与 CPU 在 **u8 上差 0.0011%**✗（**∴ 1 LSB ✓；**第 698 轮 ✓）
    //     ＋ **∴ 所以**：**严格逐位的调用方**必须自行回退**✗（**∴ 口径待用户裁定 ✓）★
    // **∴ 同样必须**与实现一致的门控 ✗**（**第 729 轮 ✓）★
    // **★ 第 246 轮：门槛**按已有的设备等级**分档 ✗ ★**（**用户裁定 ✓）
    //   ＋ **∴ 已经缓存里有设备 ✗** ⇒ **∴ 用它的等级**的档**（**∴ 不额外创建 ✓）
    //     ＋ **∴ 缓存还是空 ✗** ⇒ **∴ 先用**保守档**判**✗
    //       ⇒ **∴ 于是**：**不会**为了一个小图**去创建设备**（**∴ 那**要 ~32 ms ✓）★
    //         ＋ **∴ 而**代价说清 ✗**：**独显的**低门槛**要等设备**已经被别的操作创建**
    //           之后才生效** ✓（**∴ 典型：**输出量化（≥1 M px ✓）会先创建它 ✓）★★
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    {
        let count = (buffer.width() as usize).saturating_mul(buffer.height() as usize);
        let min_pixels = match crate::buffer::quantizer_device_class() {
            Some(class) => gpu_blur_min_pixels(class),
            None => GPU_BLUR_MIN_PIXELS,
        };
        if count >= min_pixels {
            if let Some(out) = try_gpu_gaussian_blur(buffer, sigma) {
                buffer.pixels_mut().copy_from_slice(&out);
                return;
            }
        }
    }
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut kernel = Vec::with_capacity((radius * 2 + 1) as usize);
    let mut sum = 0.0f32;
    for offset in -radius..=radius {
        let value = libm::expf(-((offset * offset) as f32) / (2.0 * sigma * sigma));
        kernel.push(value);
        sum += value;
    }
    for weight in kernel.iter_mut() {
        *weight /= sum;
    }
    convolve_horizontal(buffer, &kernel, radius);
    convolve_vertical(buffer, &kernel, radius);
}

/// 水平方向方框模糊（**滑动窗口**，O(1)/像素）。
///
/// 设计决策（设计方已批准）：模糊族放宽到 **D1（±1 LSB）**，以换取这里的复杂度下降。
/// 朴素实现每像素累加 `2r+1` 个抽头（O(radius)/像素）；实测 `clarity` 半径 64 在 512² 上要 288ms。
/// 滑动窗口只维护一个运行和：进入窗口加一个、离开窗口减一个 ✓。
///
/// **D0 边界**：浮点求和顺序改变，因此**分块渲染与整幅渲染之间**可能出现 ±1 LSB 的极少数像素差
/// （窗口起点不同 → 累积误差不同）。计算内核的其余部分（几何、混合、折叠、笔触、量化、tile 数据）
/// 仍是 D0 逐字节；服务端与客户端仍完全一致（同一份代码）。
fn blur_horizontal(buffer: &mut Buffer, radius: u32) {
    let width = buffer.width();
    let height = buffer.height();
    if width == 0 {
        return;
    }
    let window = (radius * 2 + 1) as f32;
    let radius = radius as i64;
    let last = width as i64 - 1;
    for y in 0..height {
        // 行快照（与朴素实现一致：先读后写，避免自反馈）。
        let row: Vec<[f32; 4]> = (0..width).map(|x| buffer.pixel(x, y)).collect();
        let mut acc = [0.0f32; 4];
        // 初始窗口：覆盖 [-radius, +radius]（越界按边界复制）。
        for offset in -radius..=radius {
            let pixel = row[offset.clamp(0, last) as usize];
            for channel in 0..4 {
                acc[channel] += pixel[channel];
            }
        }
        for x in 0..width {
            let mut out = acc;
            for value in out.iter_mut() {
                *value /= window;
            }
            buffer.set_pixel(x, y, out);
            // 滑到下一个像素：移除最左、加入最右。
            if (x as i64) + 1 < width as i64 {
                let leaving = row[(x as i64 - radius).clamp(0, last) as usize];
                let entering = row[(x as i64 + radius + 1).clamp(0, last) as usize];
                for channel in 0..4 {
                    acc[channel] += entering[channel] - leaving[channel];
                }
            }
        }
    }
}

/// 垂直方向方框模糊（滑动窗口；与 [`blur_horizontal`] 同一决策与同一 D1 边界）。
fn blur_vertical(buffer: &mut Buffer, radius: u32) {
    let width = buffer.width();
    let height = buffer.height();
    if height == 0 {
        return;
    }
    let window = (radius * 2 + 1) as f32;
    let radius = radius as i64;
    let last = height as i64 - 1;
    for x in 0..width {
        let column: Vec<[f32; 4]> = (0..height).map(|y| buffer.pixel(x, y)).collect();
        let mut acc = [0.0f32; 4];
        for offset in -radius..=radius {
            let pixel = column[offset.clamp(0, last) as usize];
            for channel in 0..4 {
                acc[channel] += pixel[channel];
            }
        }
        for y in 0..height {
            let mut out = acc;
            for value in out.iter_mut() {
                *value /= window;
            }
            buffer.set_pixel(x, y, out);
            if (y as i64) + 1 < height as i64 {
                let leaving = column[(y as i64 - radius).clamp(0, last) as usize];
                let entering = column[(y as i64 + radius + 1).clamp(0, last) as usize];
                for channel in 0..4 {
                    acc[channel] += entering[channel] - leaving[channel];
                }
            }
        }
    }
}

fn convolve_horizontal(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        let row: Vec<[f32; 4]> = (0..width).map(|x| buffer.pixel(x, y)).collect();
        for x in 0..width {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_x = x as i64 + index as i64 - radius as i64;
                let clamped = source_x.clamp(0, width as i64 - 1) as usize;
                let pixel = row[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

fn convolve_vertical(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width();
    let height = buffer.height();
    for x in 0..width {
        let column: Vec<[f32; 4]> = (0..height).map(|y| buffer.pixel(x, y)).collect();
        for y in 0..height {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_y = y as i64 + index as i64 - radius as i64;
                let clamped = source_y.clamp(0, height as i64 - 1) as usize;
                let pixel = column[clamped];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            buffer.set_pixel(x, y, acc);
        }
    }
}

/// 对缓冲区中每个非全透明像素执行直通颜色变换（变换后重新预乘）。
fn for_each_straight_color(buffer: &mut Buffer, mut transform: impl FnMut(&mut [f32; 3])) {
    let width = buffer.width();
    let height = buffer.height();
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let mut color = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
            transform(&mut color);
            buffer.set_pixel(
                x,
                y,
                [color[0] * alpha, color[1] * alpha, color[2] * alpha, alpha],
            );
        }
    }
}

/// 内核支持的调整类型名（工具层据此校验参数，避免造出无法渲染的对象）。
pub const ADJUSTMENT_NAMES: [&str; 18] = [
    "brightness_contrast",
    "saturation",
    "invert",
    "levels",
    "exposure",
    "white_balance",
    "curves",
    "hsl",
    "posterize",
    "color_balance",
    "split_toning",
    "vibrance",
    "black_and_white",
    "channel_mixer",
    "gradient_map",
    "shadows_highlights",
    "selective_color",
    "color_lut",
];

/// 内核支持的滤镜名。
pub const FILTER_NAMES: [&str; 14] = [
    "box_blur",
    "gaussian_blur",
    "motion_blur",
    "sharpen",
    "noise",
    "vignette",
    "glow",
    "clarity",
    "dehaze",
    "film_grain",
    "brightness_contrast",
    "saturation",
    "invert",
    // **P2 #15 厚涂浮雕**（第 254 轮）：**必须在这里登记** —— 工具侧用它做白名单，
    // 漏了就会报"内核未实现该滤镜"（本轮实测踩到：`FilterKind` 加了变体还不够）。
    "relief",
];

/// 解析 `[r,g,b]` 形式的三元组，缺省为零（不改变该档）。
fn triple(value: Option<&Value>) -> [f32; 3] {
    let Some(array) = value.and_then(Value::as_array) else {
        return [0.0, 0.0, 0.0];
    };
    let mut out = [0.0f32; 3];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = array.get(index).and_then(Value::as_f64).unwrap_or(0.0) as f32;
    }
    out
}

/// 解析定长数值数组（长度不符或含非数字 ⇒ `None`，由调用方决定默认值）。
fn fixed_floats(value: Option<&Value>, len: usize) -> Option<Vec<f32>> {
    let array = value?.as_array()?;
    if array.len() != len {
        return None;
    }
    array
        .iter()
        .map(|item| item.as_f64().map(|number| number as f32))
        .collect()
}

/// 黑白转换的权重：缺省 Rec709（与内核多处 `LUMA` 同一组数 ✓）。
fn bw_weights(params: &Value) -> [f32; 3] {
    match fixed_floats(params.get("weights"), 3) {
        Some(values) => [values[0], values[1], values[2]],
        None => [0.2126, 0.7152, 0.0722],
    }
}

/// 混合器的 3×3 矩阵：缺省单位阵。
fn mixer_matrix(params: &Value) -> [[f32; 3]; 3] {
    let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let Some(rows) = params.get("matrix").and_then(Value::as_array) else {
        return identity;
    };
    let mut matrix = identity;
    for (index, row) in rows.iter().take(3).enumerate() {
        if let Some(values) = fixed_floats(Some(row), 3) {
            matrix[index] = [values[0], values[1], values[2]];
        }
    }
    matrix
}

/// 渐变映射的色带：`[[pos, [r,g,b]], ...]`，按 pos 升序排好（调用方不必预先排序）。
fn gradient_stops(params: &Value) -> Vec<(f32, [f32; 3])> {
    let Some(array) = params.get("stops").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut stops: Vec<(f32, [f32; 3])> = array
        .iter()
        .filter_map(|entry| {
            let pair = entry.as_array()?;
            let pos = pair.first()?.as_f64()? as f32;
            let color = fixed_floats(pair.get(1), 3)?;
            Some((pos, [color[0], color[1], color[2]]))
        })
        .collect();
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    stops
}

/// 可选颜色的 9 档偏移：缺省全零（恒等）。
fn selective_ranges(params: &Value) -> [[f32; 4]; 9] {
    let mut ranges = [[0.0f32; 4]; 9];
    let Some(adjust) = params.get("adjust") else {
        return ranges;
    };
    for (index, name) in SELECTIVE_COLOR_RANGES.iter().enumerate() {
        if let Some(values) = fixed_floats(adjust.get(name), 4) {
            ranges[index] = [values[0], values[1], values[2], values[3]];
        }
    }
    ranges
}

/// 解析曲线控制点（`[[x,y], ...]`，0..1 归一化）；非法输入退回恒等曲线。
fn curve_points(params: &Value) -> Vec<(f32, f32)> {
    let Some(values) = params.get("points").and_then(Value::as_array) else {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    };
    let mut points: Vec<(f32, f32)> = values
        .iter()
        .filter_map(|value| {
            let pair = value.as_array()?;
            Some((
                pair.first()?.as_f64()? as f32,
                pair.get(1)?.as_f64()? as f32,
            ))
        })
        .map(|(x, y)| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
        .collect();
    if points.len() < 2 {
        return vec![(0.0, 0.0), (1.0, 1.0)];
    }
    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    points
}

/// 应用一个调整对象；返回是否被识别。
pub fn apply_adjustment(
    buffer: &mut Buffer,
    adjustment_type: &str,
    params: &Value,
    opacity: f32,
) -> bool {
    let Some(kind) = AdjustmentKind::from_name(adjustment_type) else {
        return false;
    };
    if opacity <= 0.0 {
        return true;
    }
    // 以副本计算，再按 opacity 插值，保证不透明度语义正确。
    let mut adjusted = buffer.clone();
    match kind {
        AdjustmentKind::BrightnessContrast => brightness_contrast(
            &mut adjusted,
            params
                .get("brightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params
                .get("contrast")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::Saturation => saturation(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::Invert => invert(&mut adjusted),
        AdjustmentKind::Levels => levels_channel(
            &mut adjusted,
            params.get("black").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("white").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("gamma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or("rgb"),
        ),
        AdjustmentKind::Vibrance => vibrance(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.5) as f32,
        ),
        AdjustmentKind::SplitToning => split_toning(
            &mut adjusted,
            triple(params.get("shadows")),
            triple(params.get("highlights")),
            params.get("balance").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        AdjustmentKind::ColorBalance => color_balance(
            &mut adjusted,
            triple(params.get("shadows")),
            triple(params.get("midtones")),
            triple(params.get("highlights")),
        ),
        AdjustmentKind::Posterize => posterize(
            &mut adjusted,
            params.get("levels").and_then(Value::as_u64).unwrap_or(6) as u32,
        ),
        AdjustmentKind::Exposure => exposure(
            &mut adjusted,
            params.get("ev").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::WhiteBalance => white_balance(
            &mut adjusted,
            params
                .get("temperature")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params.get("tint").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Hsl => hsl(
            &mut adjusted,
            params.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params
                .get("saturation")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
            params
                .get("lightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Curves => curves(
            &mut adjusted,
            &curve_points(params),
            params
                .get("channel")
                .and_then(Value::as_str)
                .unwrap_or("rgb"),
        ),
        AdjustmentKind::BlackAndWhite => {
            let tint =
                fixed_floats(params.get("tint"), 3).map(|values| [values[0], values[1], values[2]]);
            black_and_white(&mut adjusted, bw_weights(params), tint)
        }
        AdjustmentKind::ChannelMixer => {
            let constants = match fixed_floats(params.get("constants"), 3) {
                Some(values) => [values[0], values[1], values[2]],
                None => [0.0, 0.0, 0.0],
            };
            channel_mixer(&mut adjusted, mixer_matrix(params), constants)
        }
        AdjustmentKind::GradientMap => gradient_map(&mut adjusted, &gradient_stops(params)),
        AdjustmentKind::ShadowsHighlights => shadows_highlights(
            &mut adjusted,
            params.get("shadows").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params
                .get("highlights")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::SelectiveColor => selective_color(&mut adjusted, &selective_ranges(params)),
        AdjustmentKind::ColorLut => {
            let size = params.get("size").and_then(Value::as_u64).unwrap_or(0) as u32;
            let table: Vec<f32> = params
                .get("table")
                .and_then(Value::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(Value::as_f64)
                        .map(|number| number as f32)
                        .collect()
                })
                .unwrap_or_default();
            color_lut(&mut adjusted, size, &table)
        }
    }
    if (opacity - 1.0).abs() < f32::EPSILON {
        *buffer = adjusted;
    } else {
        let width = buffer.width();
        let height = buffer.height();
        for y in 0..height {
            for x in 0..width {
                let before = buffer.pixel(x, y);
                let after = adjusted.pixel(x, y);
                let mut mixed = [0.0f32; 4];
                for channel in 0..4 {
                    mixed[channel] = before[channel] + (after[channel] - before[channel]) * opacity;
                }
                buffer.set_pixel(x, y, mixed);
            }
        }
    }
    true
}

/// 应用一个滤镜对象；返回是否被识别。
/// 应用一个滤镜对象；返回是否被识别。
/// **★ 厚涂浮雕（P2 #15）：用**高度场**给明度加光向偏置 ★**（第 254 轮）
///
/// **需求 P2 #15**："高度场（厚涂浮雕：法向 + 光源的明度偏置）"。
///
/// **高度从 alpha 来**：一层里每个笔触的 alpha 就是"这里堆了多少颜料" => 浮雕看**相对高差**，
/// 取 alpha 作高度在本仓语义里是自洽的。**只处理 `alpha > 0`**；邻居空白时按"同高"处理
/// （否则会把笔画**边界**当成坡面 —— 那是边缘，不是高差）。
///
/// **算法（确定性、无随机）**：`h = a`（可乘 `height_gain`）；
/// 法向 `n = normalize(-gain·dh/dx, -gain·dh/dy, 1)`（中心差分）；
/// 光向 `l = (cos e·cos a, cos e·sin a, sin e)`（`azimuth` 0°=+x 逆时针，`elevation` clamp 1..89°）；
/// 明度偏置 `rgb' = rgb · (1 + strength · (n·l))` => 迎光坡提亮、背光压暗。
///
/// **读写口径**（本仓实测的坑）：`Buffer::pixel(x: u32, y: u32)`；**没有 `set_pixel`** =>
/// 写回用 `pixels_mut()`；法向必须基于**同一时刻**的高度快照（否则边改边读）。
pub fn relief(
    buffer: &mut Buffer,
    azimuth_deg: f32,
    elevation_deg: f32,
    strength: f32,
    height_gain: f32,
) {
    let (width, height) = (buffer.width(), buffer.height());
    if width < 3 || height < 3 || strength <= 0.0 {
        return;
    }
    let (az, el) = (
        azimuth_deg.to_radians(),
        elevation_deg.clamp(1.0, 89.0).to_radians(),
    );
    // **必须用 `libm`**（本仓 D0 规矩：内核要"两端同源" ⇒ 不许出现各平台各自实现的浮点函数 ✗）。
    // **实测踩到**：我第一版用了 std 的 `.cos()/.sin()` ⇒ `tool-kernel-determinism.mjs` 当场红：
    // "内核里出现了 1 处『各平台各自实现』的浮点调用" ✓。`.sqrt()` 是允许的（IEEE 要求正确舍入）✓。
    let light = [
        libm::cosf(el) * libm::cosf(az),
        libm::cosf(el) * libm::sinf(az),
        libm::sinf(el),
    ];
    let gain = height_gain.max(0.0);
    let (w, h) = (width as i32, height as i32);
    let snapshot: Vec<f32> = buffer.pixels().to_vec();
    let alpha_at = |pixels: &[f32], x: i32, y: i32| -> f32 {
        let cx = x.clamp(0, w - 1) as usize;
        let cy = y.clamp(0, h - 1) as usize;
        pixels[(cy * width as usize + cx) * 4 + 3].clamp(0.0, 1.0)
    };
    let mut shaded: Vec<(usize, [f32; 3])> = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let a_here = alpha_at(&snapshot, x, y);
            if a_here <= 0.0 {
                continue;
            }
            let hx = |dx: i32, dy: i32| -> f32 {
                let a = alpha_at(&snapshot, x + dx, y + dy);
                if a > 0.0 {
                    a
                } else {
                    a_here
                }
            };
            let dhdx = (hx(1, 0) - hx(-1, 0)) * 0.5 * gain;
            let dhdy = (hx(0, 1) - hx(0, -1)) * 0.5 * gain;
            let (nx, ny) = (-dhdx, -dhdy);
            let norm = (nx * nx + ny * ny + 1.0).sqrt();
            let dot = (nx * light[0] + ny * light[1] + light[2]) / norm;
            // **★ 减掉"常数环境项"（第 254 轮实测修）★**
            //   `dot` 里含 `light[2] = l_z ≈ 0.707`（**与坡面朝向无关** ✗）=> 直接乘进因子会让
            //   **整幅一律提亮** ✗：实测 `strength = 5` 时均值 0.5 ⇒ 0.703（+41%）而对比塌掉 ✗。
            //   **减掉它** => 平面（`nx = ny = 0`、`norm = 1`）处 `tilt` **正好为 0** ✓
            //   => `strength` 只放大"**坡面朝向带来的差**" ✓（浮雕该有的样子 ✓）。
            //   **这是一次行为变更** ✓：所有用过 `relief` 的画面都会变（原来偏"整体提亮" ✗，
            //   现在偏"真浮雕" ✓）—— 判据 `relief_ambient_tests` 守"均值不漂 ＋ 效果存在" ✓。
            // **★ 要减的是"同一归一化下的平面响应" `light[2] / norm`（不是 `light[2]`）★**
            //   我第一版减了 `light[2]` ⇒ **过度修正**（判据当场红：均值 0.5000 ⇒ **0.2841**，
            //   变成"整体压暗" ✗）。原因：倾斜面上 `norm > 1` ⇒ `light[2]/norm < light[2]`
            //   ⇒ 平均 `tilt` 变负 ✗。
            //   减 `light[2] / norm` 之后：`tilt = (nx·lx + ny·ly + lz)/norm − lz/norm`
            //   **正好等于纯坡面项 `(nx·lx + ny·ly) / norm`** ✓ ⇒ 平面处恒为 0 ✓，
            //   斜坡处只反映"迎光/背光" ✓。
            let tilt = dot - light[2] / norm;
            let factor = (1.0 + strength * tilt).clamp(0.0, 4.0);
            let base = (y as usize * width as usize + x as usize) * 4;
            shaded.push((
                base,
                [
                    (snapshot[base] * factor).clamp(0.0, 1.0),
                    (snapshot[base + 1] * factor).clamp(0.0, 1.0),
                    (snapshot[base + 2] * factor).clamp(0.0, 1.0),
                ],
            ));
        }
    }
    let pixels = buffer.pixels_mut();
    for (base, rgb) in shaded {
        pixels[base] = rgb[0];
        pixels[base + 1] = rgb[1];
        pixels[base + 2] = rgb[2];
    }
}

/// 应用一个滤镜对象；返回是否被识别。
///
/// **注**（第 254 轮）：这条注释今天被"新增函数插在它和函数之间"坑过**两次** ——
/// 一次是 `smooth_stroke_points`（tools.rs），一次就是这里。**规矩**：新增函数要插在
/// **整段文档注释之前**，不能插在"文档与函数之间"。
pub fn apply_filter(
    buffer: &mut Buffer,
    filter_name: &str,
    params: &Value,
    opacity: f32,
    canvas: (f64, f64),
) -> bool {
    let Some(kind) = FilterKind::from_name(filter_name) else {
        return false;
    };
    if opacity <= 0.0 {
        return true;
    }
    let mut filtered = buffer.clone();
    match kind {
        FilterKind::BoxBlur => box_blur(
            &mut filtered,
            params.get("radius").and_then(Value::as_u64).unwrap_or(1) as u32,
            params.get("passes").and_then(Value::as_u64).unwrap_or(1) as u32,
        ),
        FilterKind::GaussianBlur => gaussian_blur(
            &mut filtered,
            params.get("sigma").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        FilterKind::MotionBlur => motion_blur(
            &mut filtered,
            params.get("angle").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params
                .get("distance")
                .and_then(Value::as_f64)
                .unwrap_or(8.0) as f32,
            params.get("samples").and_then(Value::as_u64).unwrap_or(16) as u32,
        ),
        FilterKind::Relief => relief(
            &mut filtered,
            params
                .get("azimuth")
                .and_then(Value::as_f64)
                .unwrap_or(135.0) as f32,
            params
                .get("elevation")
                .and_then(Value::as_f64)
                .unwrap_or(45.0) as f32,
            params
                .get("strength")
                .and_then(Value::as_f64)
                .unwrap_or(0.6) as f32,
            params
                .get("height_gain")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        ),
        FilterKind::Sharpen => sharpen(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(2) as u32,
        ),
        FilterKind::Noise => noise(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.05) as f32,
            params.get("seed").and_then(Value::as_u64).unwrap_or(0),
        ),
        FilterKind::FilmGrain => film_grain(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.3) as f32,
            params.get("size").and_then(Value::as_u64).unwrap_or(2) as u32,
            params.get("seed").and_then(Value::as_u64).unwrap_or(0),
        ),
        FilterKind::Dehaze => dehaze(
            &mut filtered,
            triple(params.get("air")),
            params.get("omega").and_then(Value::as_f64).unwrap_or(0.85) as f32,
            params.get("floor").and_then(Value::as_f64).unwrap_or(0.1) as f32,
        ),
        // 默认半径取 8（原为 16）：`box_blur` 是 O(radius)/像素，半径 16 在 512² 上默认就要
        // 约 80ms，对交互式预览过重。**只影响未显式给出 radius 的新调用**：已落地的原子把参数
        // 存在 payload 里，历史结果不受影响；数学本身未改，D0 不受影响。
        FilterKind::Clarity => clarity(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.5) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(8) as u32,
        ),
        FilterKind::Glow => glow(
            &mut filtered,
            params
                .get("threshold")
                .and_then(Value::as_f64)
                .unwrap_or(0.6) as f32,
            params.get("radius").and_then(Value::as_u64).unwrap_or(8) as u32,
            params
                .get("intensity")
                .and_then(Value::as_f64)
                .unwrap_or(0.8) as f32,
        ),
        FilterKind::Vignette => vignette(
            &mut filtered,
            params
                .get("strength")
                .and_then(Value::as_f64)
                .unwrap_or(0.4) as f32,
            params.get("radius").and_then(Value::as_f64).unwrap_or(0.75) as f32,
            params
                .get("softness")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32,
            canvas,
        ),
        FilterKind::BrightnessContrast => brightness_contrast(
            &mut filtered,
            params
                .get("brightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params
                .get("contrast")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
        ),
        FilterKind::Saturation => saturation(
            &mut filtered,
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
        ),
        FilterKind::Invert => invert(&mut filtered),
    }
    if (opacity - 1.0).abs() < f32::EPSILON {
        *buffer = filtered;
    } else {
        let width = buffer.width();
        let height = buffer.height();
        for y in 0..height {
            for x in 0..width {
                let before = buffer.pixel(x, y);
                let after = filtered.pixel(x, y);
                let mut mixed = [0.0f32; 4];
                for channel in 0..4 {
                    mixed[channel] = before[channel] + (after[channel] - before[channel]) * opacity;
                }
                buffer.set_pixel(x, y, mixed);
            }
        }
    }
    true
}

/// 屏幕空间的辅助：线性值 → sRGB 字节（供调试与测试）。
pub fn linear_to_display_byte(value: f32) -> u8 {
    (linear_to_srgb(value) * 255.0 + 0.5)
        .floor()
        .clamp(0.0, 255.0) as u8
}

/// 屏幕空间的辅助：sRGB 字节 → 线性值。
pub fn display_byte_to_linear(byte: u8) -> f32 {
    srgb_to_linear(byte as f32 / 255.0)
}

/// **∴ 滤镜走 GPU 的**最小像素数（**保守档 ✓）✗**（**第 700 轮 ✓；目标：滤镜 GPU 化 ✓）★
///
/// **∴ 为什么是 2 M ✗**（**据第 699 轮实测 ✓）：
///   **∴ 1080p（**2.07 M ✓）⇒ GPU **2.0–3.6×**✗
///     ＋ **∴ 4K（**8.29 M ✓）⇒ GPU **1.1–1.5×** ✓
///       ＋ **∴ 而**小图上**传输的固定成本（**~15 ms ✓）**会盖过收益** ✓
///         ⇒ **∴ 所以**：**低于 2 M 像素时**CPU 本来就快**✗ ⇒ **∴ 不值得**冒传输风险** ✓ ★
///
/// **★★★ 第 246 轮：它**降级成"保守档"**✗ 而**不是**唯一门槛**★★★**（**用户裁定 ✓）
///   ＋ **∴ 用户裁定（**第 246 轮 ✓）✗**：**阈值**按适配器分档** ✓
///   ＋ **∴ 依据 ✗**（**L4 实测 ✗ 5 轮取中位 ✗ GPU 含传输 ✓）**：
///     **∴ 640×480（**0.31 M ✓）⇒ σ=2 **3.53×**｜σ=8 **11.52×** ✓
///       ＋ **∴ 960×540 ⇒ **3.81×／12.34×**✗ **1280×720 ⇒ **2.01×／12.14×**** ✓
///         ＋ **∴ 1600×900 ⇒ **1.64×／3.61×**✗ **1080p ⇒ **2.57×／6.46×**✗ **4K ⇒ **1.85×／5.24×**** ✓
///           ⇒ **★ 每一档**都是正向 ✗ 连最小的 0.31 M 都赢 ⇒ **∴ 交点**在 0.31 M 以下** ✓ ★★
///   ＋ **∴ 所以 ✗**：**独显**降到**实测过的最小尺寸**✗（**307 200 ＝ 640×480 ✓）
///     ＋ **∴ 而**集成／虚拟／**软件渲染**没有实测**✗ ⇒ **∴ 它们**继续用这个 2 M 保守档** ✓
///       ⇒ **∴ 那**正是本仓的纪律**：**不许把没量过的东西当收益** ✓ ★★
#[cfg(feature = "gpu")]
const GPU_BLUR_MIN_PIXELS: usize = 2_000_000;

/// **∴ 独显档：实测过的**最小仍赢**尺寸 ✗**（**第 246 轮 ✓）★
///
/// **∴ 为什么取**实测点本身**✗ 而**不再往下探** ✓**：
///   **∴ 640×480 是**这轮量到的最小尺寸**（**3.53×／11.52× ✓）
///     ⇒ **∴ 比它更小的区间**没有数据**✗
///       ⇒ **∴ 所以**：**门槛**就设在数据边界上**✗ ⇒ **∴ 门槛以上的每一个尺寸**都**有实测背书** ✓ ★★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
const GPU_BLUR_MIN_PIXELS_DISCRETE: usize = 307_200;

/// **★ 模糊的规模门槛：**按适配器等级分档**✗ ★**（**第 246 轮 ✓；用户裁定 ✓）
///
/// **∴ 为什么不能一个常数 ✗**：**同一个 2 M**对**独显是**过保守**✗
///   （**∴ 整段 0.31–2 M 白丢 1.6–12 倍 ✓）＋ **∴ 对**核显**又**没有依据** ✓ ★
///     ⇒ **∴ 所以**：**分档 ✗ 而**且**每档都要有实测或明确说"没有"** ✓ ★★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
#[must_use]
fn gpu_blur_min_pixels(class: yanshi_gpu::DeviceClass) -> usize {
    match class {
        // **∴ 有 L4 实测背书 ✓（**0.31 M 起每一档都赢 ✓）
        yanshi_gpu::DeviceClass::Discrete => GPU_BLUR_MIN_PIXELS_DISCRETE,
        // **∴ 其余**没有实测**✗ ⇒ **∴ 保守档** ✗ ＋ **∴ 而不是**"看起来也行" ✓ ★★
        _ => GPU_BLUR_MIN_PIXELS,
    }
}

/// **∴ 当前缓存设备的**模糊门槛**✗**（**`None` ＝ 还没有设备 ✓）**（**第 246 轮 ✓；判据用 ✓）
///
/// **∴ 为什么要暴露它 ✗**：**判据**没法**自己知道适配器是独显还是核显**✗
///   ⇒ **∴ 而**"0.52 M 该不该走 GPU"**完全取决于那个等级** ✓
///     ⇒ **∴ 于是**：**判据**按这个值来判断**✗ ⇒ **∴ 两个方向**都能有牙** ✓ ★★
///       （**∴ 独显：**门槛低 ⇒ **必须走** ✗ **核显：**门槛高 ⇒ **必须不走** ✓）★★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
#[doc(hidden)]
#[must_use]
pub fn blur_min_pixels_for_cached_device() -> Option<usize> {
    crate::buffer::quantizer_device_class().map(gpu_blur_min_pixels)
}
///
/// **∴ 它守什么 ✗**（**∴ 两条都能红 ✓）★**：
///   **∴ ①** 独显档**必须**低于**保守档** ✗
///     ⇒ **∴ 否则**"分档"是句空话**✗（**∴ 而**用户裁定要的正是**独显放宽** ✓）；
///   **∴ ②** 独显档**必须**是**实测过的最小尺寸**（**307 200 ✓）✗
///     ⇒ **∴ 谁**改了它**✗ 或**把没量过的等级也放开**✗ ⇒ **∴ 本判据**必红** ✓ ★★
#[cfg(all(test, feature = "gpu", not(target_arch = "wasm32")))]
mod blur_threshold_tests {
    use super::*;

    #[test]
    fn the_blur_threshold_is_per_device_class_and_backed_by_measurement() {
        use yanshi_gpu::DeviceClass;
        assert_eq!(
            gpu_blur_min_pixels(DeviceClass::Discrete),
            307_200,
            "独显档应当是 640×480 ＝ 307 200（**L4 实测过的最小仍赢尺寸 ✓）"
        );
        for class in [
            DeviceClass::Integrated,
            DeviceClass::Virtual,
            DeviceClass::Cpu,
            DeviceClass::Other,
        ] {
            assert_eq!(
                gpu_blur_min_pixels(class),
                GPU_BLUR_MIN_PIXELS,
                "**{class:?}** 没有实测 ⇒ 必须留在保守档（不许把没量过的当收益）"
            );
        }
        assert!(
            GPU_BLUR_MIN_PIXELS_DISCRETE < GPU_BLUR_MIN_PIXELS,
            "独显档必须**严格低于**保守档 ⇒ 否则「分档」等于没做"
        );
    }
}

/// **∴ GPU 单帧的**时间预算**（**毫秒 ✓）✗**（**第 706 轮 ✓）★
///
/// **∴ 为什么需要 ✗**（**第 90 轮实测 ✓）：
///   **∴ 核显的 GPU 时间**极不稳定**✗（**∴ 4K σ=2 ⇒ **434–2362 ms**，5.4× ✓）
///     ＋ **∴ 而**最坏情况**比 CPU **慢 3.6×****✗（**∴ 2362 vs 671 ms ✓）
///       ＋ **∴ 所以**：**「**中位 1.30×**」**掩盖了慢帧** ✓
///         ＋ **∴ 对策 ✗**：**给 GPU 一个**墙钟预算**✗
///           ＋ **∴ 超预算 ⇒ **本帧结果仍用（**数据已算好 ✓）但**标记降级**✗
///             ＋ **∴ 降级期内**直接走 CPU**✗ ⇒ **∴ 从而**避免连续慢帧 ✓ ★
///
/// **∴ 取值的依据（**第 90 轮实测 ✓）**：
///   **∴ 1080p σ=2 ⇒ **70–152 ms****✗（**∴ 中位 80 ✓）⇒ **∴ 预算 150 ms ⇒ **偶尔降级 ✓（**∴ 合理 ✓）
///     ＋ **∴ 4K σ=8 ⇒ **907–2312 ms****✗ ⇒ **∴ 必然降级 ⇒ **自动走 CPU** ✓
///       ⇒ **★ 即：**4K 的大图模糊自动退回 CPU**✗ ⇒ **∴ 避免最坏 0.28×** ✓ ★
#[cfg(feature = "gpu")]
const GPU_BLUR_BUDGET_MS: u128 = 150;

/// **∴ 读预算 ✗**（**`YANSHI_GPU_BLUR_BUDGET_MS` 可覆盖 ⇒ **∴ 供判据 ✓）★
#[cfg(feature = "gpu")]
#[must_use]
fn gpu_blur_budget_ms() -> u128 {
    std::env::var("YANSHI_GPU_BLUR_BUDGET_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u128>().ok())
        .unwrap_or(GPU_BLUR_BUDGET_MS)
}

/// **∴ 降级期的长度（**帧数 ✓）✗**（**第 706 轮 ✓）
///
/// **∴ 为什么是 60 ✗**：**约**一秒（**60 fps ✓）
///   ＋ **∴ 从而**：**慢帧后**有一秒**不再试 GPU**✗（**∴ 避免**连续卡顿 ✓）
///     ＋ **∴ 而**一秒后再试**✗ ⇒ **∴ 若**已恢复 ⇒ **∴ 自然回到 GPU** ✓ ★
#[cfg(feature = "gpu")]
const GPU_BLUR_COOLDOWN_FRAMES: u32 = 60;

/// **∴ 降级剩余帧数 ✗**（**0 ⇒ 正常 ✓）★
#[cfg(feature = "gpu")]
static GPU_BLUR_COOLDOWN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// **∴ 因超预算而降级的次数 ✗**（**判据与诊断用 ✓）★
#[cfg(feature = "gpu")]
static GPU_BLUR_DEGRADES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// **∴ 读取降级次数 ✗** ★
#[cfg(feature = "gpu")]
#[must_use]
pub fn blur_degrade_count() -> usize {
    GPU_BLUR_DEGRADES.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清零降级计数 ＋ 冷却 ✗**（**判据用 ✓）★
#[cfg(feature = "gpu")]
pub fn reset_blur_degrade() {
    GPU_BLUR_DEGRADES.store(0, std::sync::atomic::Ordering::Relaxed);
    GPU_BLUR_COOLDOWN.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// **∴ 当前是否处于降级期 ✗**（**供判据与诊断 ✓）★
#[cfg(feature = "gpu")]
#[must_use]
pub fn blur_degraded() -> bool {
    GPU_BLUR_COOLDOWN.load(std::sync::atomic::Ordering::Relaxed) > 0
}

/// **∴ 尝试用 GPU 做高斯模糊 ✗**（**失败 ⇒ `None` ⇒ 调用方回退 CPU ✓）★**（**第 700 轮 ✓）
// **★★★ 门控必须**与 `quantizer_cache` 一致 ✗ ★★★**（**第 729 轮：**CI 抓到 ✓）
//   **∴ 症状 ✗**：**wasm32 ＋ gpu 构建**报**
//     `cannot find function quantizer_cache in module crate::buffer`** ✓
//     ＋ **∴ 因为** `quantizer_cache` 的门控是
//       `all(feature = "gpu", not(target_arch = "wasm32"))`**✗
//       ＋ **∴ 而**本函数**只写了 `feature = "gpu"`**✗ ⇒ **∴ 于是**在 wasm32 下**引用了不存在的函数** ✓ ★
//         ＋ **∴ 而**本机**没有 wasm32 目标**✗ ⇒ **∴ 本地**永远测不到**✗
//           ＋ **★ 所以：**只有 CI 能抓到它**✗ ⇒ **∴ 而那**正是目标第 13 条的价值** ✓ ★
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
fn try_gpu_gaussian_blur(buffer: &Buffer, sigma: f32) -> Option<Vec<f32>> {
    // **∴ 复用**同一个 GPU 实例**✗（**∴ 不重复初始化 ⇒ **∴ 不重复付设备创建成本 ✓）★
    // **∴ 降级期内**直接放弃 GPU**✗（**∴ 从而**避免连续慢帧 ✓）★
    //   **∴ 递减在这里做 ✗**：**每次**尝试**（**无论成功失败 ✓）都消耗一帧冷却** ✓
    //     ＋ **∴ 从而**：**冷却**必然在 60 次尝试后结束**✗ ⇒ **∴ 不会**永久降级 ✓ ★
    let remaining = GPU_BLUR_COOLDOWN.load(std::sync::atomic::Ordering::Relaxed);
    if remaining > 0 {
        GPU_BLUR_COOLDOWN.store(remaining - 1, std::sync::atomic::Ordering::Relaxed);
        return None;
    }
    // **★★★ 第 246 轮：**别等量化路来创建设备**✗ ★★★**（**用户裁定 ✓）
    //   ＋ **∴ 用户裁定（**原文 ✓）✗**：**「**如果有 gpu 用 ✗ 那么就**打开 gpu
    //     能加速且确实有收益的功能**」 ✓
    //   ＋ **∴ 缺口（**本仓的**渲染顺序**决定它是真的 ✓）★**：
    //     **∴ 渲染**先画对象／滤镜**✗ **最后**才量化输出** ✓
    //       ⇒ **∴ 而**创建量化器的**只有**量化路**✗
    //         ⇒ **∴ 于是**：**第一帧的大滤镜**看到空缓存 ⇒ **∴ 回退 CPU** ✓
    //           ⇒ **∴ 明明有 GPU ✗ 而**它**要等到**第二帧**才用得上** ✓ ★★
    //   ＋ **∴ 修法 ✗**：**用与量化路**同一个**get-or-create**✗（`ensure_quantizer` ✓）
    //     ⇒ **∴ 而**它**内部**已经查了 `gpu_disabled()`**✗
    //       ⇒ **∴ 所以**：**`--gpu off` **不会**因此创建设备** ✓ ★★
    if !crate::buffer::ensure_quantizer(crate::color::srgb_encode_table()) {
        return None;
    }
    let cache = crate::buffer::quantizer_cache();
    let guard = cache.lock().ok()?;
    let quantizer = guard.as_ref()?;
    // **∴ 量**墙钟**✗**（**∴ 超预算 ⇒ **降级 ✓）★
    let started = crate::probe::ProbeClock::now();
    let out = quantizer.blur_gaussian(buffer.pixels(), buffer.width(), buffer.height(), sigma)?;
    let elapsed_ms = started.elapsed().as_millis();
    if elapsed_ms > gpu_blur_budget_ms() {
        // **∴ 超预算 ✗**：**本帧结果**仍然使用**✗（**∴ 数据已算好 ⇒ **∴ 丢弃是浪费 ✓）
        //   ＋ **∴ 但**标记**降级**✗ ⇒ **∴ 之后 60 帧**不再试 GPU** ✓ ★
        GPU_BLUR_DEGRADES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        GPU_BLUR_COOLDOWN.store(
            GPU_BLUR_COOLDOWN_FRAMES,
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    // **★★★ 抽检哨兵（**裁定 ② 的档 B ✓）✗ ★★★**（**第 701 轮 ✓）
    //   **∴ 为什么是**抽检**而不是**全量核对**✗ ★**（**决定性算术 ✓）：
    //     **∴ 「**每帧全量核对**」需要** CPU 真值**✗ ⇒ **∴ 总成本 ＝ GPU ＋ CPU** ✓
    //       ＋ **∴ 实测 ✗**：**4K σ=2 ⇒ CPU 608 ms ＋ GPU 416 ms ＝ **1024 ms****✗
    //         ⇒ **★ 1.7× **慢于**纯 CPU**（**∴ 608 ms ✓）★** ⇒ **∴ 收益**变成**负的** ✓
    //           ＋ **∴ 所以**：**核对成本 ≈ 主计算 ⇒ **只能抽检** ✓ ★
    //   **∴ 抽检做法 ✗**：
    //     **∴ 采样 **256 个**分散点**✗（**∴ 用**质数步长避免与图像结构共振 ✓）
    //       ＋ **∴ 每个点**在 CPU 上**单点重算**✗（**∴ O(r²) ⇒ **∴ 256 点**远小于全量 ✓）
    //         ＋ **∴ 任一不符 ⇒ **返回 `None` ⇒ **∴ 调用方**回退整帧 CPU** ✓
    //           ＋ **∴ 并**记一次失败**✗（**∴ 供判据与诊断 ✓）★
    //   **∴ 为什么抽检够用 ✗**：**GPU 的偏差**若存在**✗
    //     ＋ **∴ 它**来自**累加顺序／fma**✗ ⇒ **∴ 那是**系统性的**⇒ **∴ 抽检**必然打中** ✓
    //       ＋ **∴ 而**不是**罕见的随机单点错误** ✓ ★
    let width = buffer.width();
    let height = buffer.height();
    let kernel = gaussian_kernel(sigma)?;
    let radius = ((kernel.len() - 1) / 2) as i64;
    let count = (width as usize) * (height as usize);
    let step = 7919usize; // **∴ 质数 ⇒ **∴ 分散 ✓
    let mut sampled = 0usize;
    let mut index = 0usize;
    while sampled < 256 && index < count {
        let x = (index % width as usize) as u32;
        let y = (index / width as usize) as u32;
        let expect = gaussian_pixel_at(buffer.pixels(), width, height, &kernel, radius, x, y);
        let base = (index % count) * 4;
        for c in 0..4 {
            if out[base + c].to_bits() != expect[c].to_bits() {
                BLUR_SENTINEL_FAILURES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return None;
            }
        }
        sampled += 1;
        index += step;
    }
    Some(out)
}

/// **∴ 抽检哨兵**失败**的次数 ✗**（**第 701 轮 ✓）★
///
/// **∴ 语义 ✗**：**0 ⇒ **没发现偏差** ✓（**∴ 不是**"没核对" ✓ —— **∴ 只有**真跑过哨兵**才可能为 0 ✓）★
static BLUR_SENTINEL_FAILURES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// **∴ 读取抽检失败次数 ✗**（**判据与诊断用 ✓）★
#[must_use]
pub fn blur_sentinel_failures() -> usize {
    BLUR_SENTINEL_FAILURES.load(std::sync::atomic::Ordering::Relaxed)
}

/// **∴ 清零抽检失败计数 ✗**（**判据用 ✓）★
pub fn reset_blur_sentinel_failures() {
    BLUR_SENTINEL_FAILURES.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// **∴ 与 `gaussian_blur` **同一份**核 ✗**（**∴ 公式 ＋ 归一化顺序逐字相同 ✓）★
#[cfg(feature = "gpu")]
#[must_use]
fn gaussian_kernel(sigma: f32) -> Option<Vec<f32>> {
    if sigma <= 0.0 {
        return None;
    }
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut kernel = Vec::with_capacity((radius * 2 + 1) as usize);
    let mut sum = 0.0f32;
    for offset in -radius..=radius {
        let value = libm::expf(-((offset * offset) as f32) / (2.0 * sigma * sigma));
        kernel.push(value);
        sum += value;
    }
    for weight in kernel.iter_mut() {
        *weight /= sum;
    }
    Some(kernel)
}

/// **∴ 单点 CPU 重算 ✗**（**∴ 抽检哨兵用 ⇒ **O(r²) 而不是 O(全帧 ✓）★
///
/// **∴ 与 `gaussian_blur` 的等价性 ✗**：
///   **∴ ① 先**水平**（**每个 ky 一行 ✓）**＋ **∴ ② 再**垂直**✗
///     ＋ **∴ 边界**clamp**✗（**∴ 与 `convolve_*` 相同 ✓）
///       ＋ **∴ 累加顺序**＝ kernel 顺序**✗（**∴ 相同 ✓）★
#[cfg(feature = "gpu")]
#[must_use]
fn gaussian_pixel_at(
    input: &[f32],
    width: u32,
    height: u32,
    kernel: &[f32],
    radius: i64,
    x: u32,
    y: u32,
) -> [f32; 4] {
    let mut acc = [0.0f32; 4];
    for (ki, &w_y) in kernel.iter().enumerate() {
        let sy = (y as i64 + ki as i64 - radius).clamp(0, height as i64 - 1) as u32;
        // **∴ 该行的**水平值**✗（**∴ 对 4 个通道分别算 ✓）★
        let mut row = [0.0f32; 4];
        for (kj, &w_x) in kernel.iter().enumerate() {
            let sx = (x as i64 + kj as i64 - radius).clamp(0, width as i64 - 1) as u32;
            let base = ((sy * width + sx) * 4) as usize;
            for c in 0..4 {
                row[c] += input[base + c] * w_x;
            }
        }
        for c in 0..4 {
            acc[c] += row[c] * w_y;
        }
    }
    acc
}

#[cfg(all(test, feature = "gpu"))]
mod sentinel_tests {
    // **★ 抽检哨兵的**参考实现**必须与**整帧 CPU 路径**在 f32 上**逐位相同**** ✗ ★
    //   （**第 701 轮 ✓）
    //
    //   **∴ 为什么必须验它 ✗**：**哨兵**在 GPU 结果上**抽样比对**✗
    //     ＋ **∴ 而**它的**参考值**来自 `gaussian_pixel_at` **单点重算** ✓
    //       ＋ **∴ 若**它与整帧路径**不等价**✗ ⇒ **∴ 哨兵**要么**误报**（**∴ 收益归零 ✓）
    //         ＋ **∴ 要么**漏报真偏差** ✓ ⇒ **∴ 所以**必须独立验证 ✓ ★
    use super::*;

    #[test]
    fn the_sentinel_reference_matches_the_full_cpu_path() {
        let (w, h) = (48u32, 32u32);
        for &sigma in &[1.0f32, 2.0, 4.0] {
            let mut cpu = Buffer::new(0, 0, w, h);
            {
                let mut st = 0x5E17_1E01u32 | 1;
                for px in cpu.pixels_mut().chunks_exact_mut(4) {
                    st = st.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let a = ((st >> 9) & 0xff) as f32 / 255.0;
                    let b = ((st >> 17) & 0xff) as f32 / 255.0;
                    px.copy_from_slice(&[a * b, a * (1.0 - b), a * 0.5, a]);
                }
            }
            let input = cpu.pixels().to_vec();
            gaussian_blur(&mut cpu, sigma);
            let kernel = gaussian_kernel(sigma).expect("核必须能生成");
            let radius = ((kernel.len() - 1) / 2) as i64;
            let mut diff = 0usize;
            for y in 0..h {
                for x in 0..w {
                    let expect = gaussian_pixel_at(&input, w, h, &kernel, radius, x, y);
                    let got = cpu.pixel(x, y);
                    for c in 0..4 {
                        if expect[c].to_bits() != got[c].to_bits() {
                            diff += 1;
                        }
                    }
                }
            }
            println!("  ★ sigma={sigma} 单点重算 vs 整帧：逐位差异 {diff}");
            assert_eq!(
                diff, 0,
                "单点重算必须与整帧 CPU 路径逐位相同（sigma={sigma}）"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn filled(color: [f32; 4]) -> Buffer {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        buffer.fill(color);
        buffer
    }

    #[test]
    fn brightness_and_contrast_are_linear_in_straight_space() {
        let mut buffer = filled([0.25, 0.25, 0.25, 1.0]);
        brightness_contrast(&mut buffer, 0.25, 1.0);
        assert!((buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-5);
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 2.0);
        assert!((buffer.pixel(0, 0)[0] - 1.0).abs() < 1e-5, "对比度裁到 1.0");
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 0.5);
        assert!((buffer.pixel(0, 0)[0] - 0.25).abs() < 1e-5);
    }

    #[test]
    fn transparency_is_preserved_by_adjustments() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, [0.5, 0.0, 0.0, 0.5]);
        brightness_contrast(&mut buffer, 0.5, 1.0);
        assert_eq!(buffer.pixel(0, 0)[3], 0.5, "alpha 不变");
        assert_eq!(buffer.pixel(1, 0), [0.0; 4], "全透明像素保持全透明");
    }

    #[test]
    fn saturation_zero_is_luma_preserving() {
        let mut buffer = filled([1.0, 0.0, 0.0, 1.0]);
        saturation(&mut buffer, 0.0);
        let pixel = buffer.pixel(0, 0);
        assert!((pixel[0] - 0.2126).abs() < 1e-5, "{pixel:?}");
        assert!((pixel[1] - 0.2126).abs() < 1e-5);
        assert!((pixel[2] - 0.2126).abs() < 1e-5);

        let mut unchanged = filled([0.3, 0.6, 0.9, 1.0]);
        let before = unchanged.pixel(0, 0);
        saturation(&mut unchanged, 1.0);
        let after = unchanged.pixel(0, 0);
        for channel in 0..3 {
            assert!((before[channel] - after[channel]).abs() < 1e-5);
        }
    }

    #[test]
    fn invert_is_involution() {
        let mut buffer = filled([0.2, 0.4, 0.6, 1.0]);
        invert(&mut buffer);
        assert!((buffer.pixel(0, 0)[0] - 0.8).abs() < 1e-5);
        invert(&mut buffer);
        assert!((buffer.pixel(0, 0)[0] - 0.2).abs() < 1e-5);
    }

    /// 新增调整：曝光/白平衡/曲线都必须真的改变像素，且保持预乘不变量 rgb ≤ alpha。
    #[test]
    fn new_adjustments_change_pixels_and_keep_premultiplied() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 2, 1);
            buffer.set_pixel(0, 0, [0.25, 0.25, 0.25, 0.5]);
            buffer.set_pixel(1, 0, [0.1, 0.2, 0.3, 0.4]);
            buffer
        };
        for (name, params) in [
            ("exposure", json!({"ev": 1.0})),
            ("white_balance", json!({"temperature": 0.6, "tint": -0.3})),
            (
                "curves",
                json!({"points": [[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]]}),
            ),
            (
                "curves",
                json!({"points": [[0.0, 0.0], [0.5, 0.2], [1.0, 1.0]], "channel": "r"}),
            ),
        ] {
            let mut buffer = make();
            let before = buffer.pixel(0, 0);
            assert!(
                apply_adjustment(&mut buffer, name, &params, 1.0),
                "{name} 应被识别"
            );
            assert_ne!(before, buffer.pixel(0, 0), "{name} 必须改变像素");
            for (x, y) in [(0u32, 0u32), (1, 0)] {
                let pixel = buffer.pixel(x, y);
                assert!(
                    pixel[0] <= pixel[3] + 1e-3
                        && pixel[1] <= pixel[3] + 1e-3
                        && pixel[2] <= pixel[3] + 1e-3,
                    "{name} 破坏预乘不变量：{pixel:?}"
                );
                assert!(pixel.iter().all(|v| v.is_finite()), "{name} 产出非法值");
            }
        }
        // 曝光 +1EV 在线性光下翻倍。
        let mut buffer = make();
        apply_adjustment(&mut buffer, "exposure", &json!({"ev": 1.0}), 1.0);
        assert!(
            (buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-4,
            "{:?}",
            buffer.pixel(0, 0)
        );
    }

    /// 新增滤镜：动感模糊/锐化/噪点/暗角都要改变像素，噪点必须**由 seed 决定**（可复现）。
    #[test]
    fn new_filters_change_pixels_and_noise_is_seeded() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 4, 1);
            for x in 0..4 {
                // 合法的预乘像素：rgb ≤ alpha。
                buffer.set_pixel(
                    x,
                    0,
                    if x < 2 {
                        [0.2, 0.2, 0.2, 0.5]
                    } else {
                        [0.45, 0.05, 0.05, 0.5]
                    },
                );
            }
            buffer
        };
        for (name, params) in [
            ("motion_blur", json!({"angle": 0.0, "distance": 3.0})),
            ("sharpen", json!({"amount": 1.5, "radius": 1})),
            ("noise", json!({"amount": 0.4, "seed": 7})),
            ("vignette", json!({"strength": 0.8, "radius": 0.1})),
            (
                "glow",
                json!({"threshold": 0.2, "radius": 2, "intensity": 1.0}),
            ),
        ] {
            let mut buffer = make();
            let before: Vec<_> = (0..4).map(|x| buffer.pixel(x, 0)).collect();
            assert!(
                apply_filter(&mut buffer, name, &params, 1.0, (4.0, 1.0)),
                "{name} 应被识别"
            );
            let after: Vec<_> = (0..4).map(|x| buffer.pixel(x, 0)).collect();
            assert_ne!(before, after, "{name} 必须改变像素");
            for pixel in &after {
                assert!(pixel.iter().all(|v| v.is_finite()));
                assert!(
                    pixel[0] <= pixel[3] + 1e-3 && pixel[1] <= pixel[3] + 1e-3,
                    "{name} 破坏预乘不变量：{pixel:?}"
                );
            }
        }
        // 同 seed 同结果，不同 seed 不同结果（确定性来自原子 seed）。
        let run = |seed: u64| {
            let mut buffer = make();
            apply_filter(
                &mut buffer,
                "noise",
                &json!({"amount": 0.6, "seed": seed}),
                1.0,
                (4.0, 1.0),
            );
            (0..4).map(|x| buffer.pixel(x, 0)).collect::<Vec<_>>()
        };
        assert_eq!(run(11), run(11), "同 seed 必须逐位一致");
        assert_ne!(run(11), run(12), "不同 seed 应产生不同噪点");
    }

    /// 动感模糊必须**只沿角度方向**扩散：单个亮点只应沿该方向拉出能量。
    #[test]
    fn motion_blur_spreads_only_along_its_angle() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 16, 16);
            buffer.set_pixel(8, 8, [1.0, 1.0, 1.0, 1.0]);
            buffer
        };
        let blur = |angle: f64| {
            let mut buffer = make();
            apply_filter(
                &mut buffer,
                "motion_blur",
                &json!({"angle": angle, "distance": 8.0, "samples": 8}),
                1.0,
                (16.0, 16.0),
            );
            buffer
        };
        let horizontal = blur(0.0);
        let vertical = blur(90.0);
        // 0°：能量应落在同一行 (4..12, 8)，同一列应基本没有。
        assert!(horizontal.pixel(4, 8)[3] > 0.05, "水平模糊应沿 x 扩散");
        assert!(horizontal.pixel(12, 8)[3] > 0.05, "水平模糊应沿 x 扩散");
        assert!(horizontal.pixel(8, 4)[3] < 0.01, "水平模糊不应沿 y 扩散");
        // 90°：反过来。
        assert!(vertical.pixel(8, 4)[3] > 0.05, "竖直模糊应沿 y 扩散");
        assert!(vertical.pixel(8, 12)[3] > 0.05, "竖直模糊应沿 y 扩散");
        assert!(vertical.pixel(4, 8)[3] < 0.01, "竖直模糊不应沿 x 扩散");
        // 总能量守恒（平均而非叠加）。
        let sum = |buffer: &Buffer| {
            let mut total = 0.0;
            for y in 0..16 {
                for x in 0..16 {
                    total += buffer.pixel(x, y)[3];
                }
            }
            total
        };
        assert!(
            (sum(&horizontal) - 1.0).abs() < 1e-3,
            "模糊应保持总能量：{}",
            sum(&horizontal)
        );
    }

    /// 辉光：亮点应把光晕扩散到邻近暗部，纯暗区域不应被改动，且保持预乘不变量。
    #[test]
    fn glow_bleeds_light_into_neighbours_only() {
        let mut buffer = Buffer::new(0, 0, 16, 16);
        buffer.set_pixel(8, 8, [1.0, 1.0, 1.0, 1.0]);
        let before_corner = buffer.pixel(0, 0);
        glow(&mut buffer, 0.5, 3, 1.0);
        let spark = buffer.pixel(8, 8);
        let around = buffer.pixel(10, 8);
        let corner = buffer.pixel(0, 0);
        assert!(spark[0] > 0.9, "亮点本身应保留：{spark:?}");
        // 单像素高光经 radius 3 两趟模糊后摊得很薄，量级很小但必须非零。
        assert!(around[0] > 0.005, "邻近像素应收到光晕：{around:?}");
        assert!(
            around[3] > 0.005,
            "辉光应同时增加 alpha（否则无法扩散到笔迹外）"
        );
        assert_eq!(corner, before_corner, "远处暗部不应被改动：{corner:?}");
        for y in 0..16 {
            for x in 0..16 {
                let pixel = buffer.pixel(x, y);
                assert!(
                    pixel[0] <= pixel[3] + 1e-3 && pixel[0] >= 0.0 && pixel[0].is_finite(),
                    "辉光破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// HSL：色相旋转 120° 应把红转到绿；饱和度 0 应变灰；明度偏移不该跳出色域。
    #[test]
    fn hsl_rotates_hue_and_desaturates() {
        let red = || {
            let mut buffer = Buffer::new(0, 0, 1, 1);
            buffer.set_pixel(0, 0, [1.0, 0.0, 0.0, 1.0]);
            buffer
        };
        let mut rotated = red();
        hsl(&mut rotated, 120.0, 1.0, 0.0);
        let pixel = rotated.pixel(0, 0);
        assert!(
            pixel[1] > 0.9 && pixel[0] < 0.1,
            "红转 120° 应为绿：{pixel:?}"
        );

        let mut grey = red();
        hsl(&mut grey, 0.0, 0.0, 0.0);
        let pixel = grey.pixel(0, 0);
        assert!(
            (pixel[0] - pixel[1]).abs() < 1e-3 && (pixel[1] - pixel[2]).abs() < 1e-3,
            "饱和度 0 应为中性灰：{pixel:?}"
        );

        // 明度偏移与色相旋转都不该产出越界值（预乘不变量）。
        for (hue, sat, light) in [(0.0, 1.0, 0.9), (200.0, 3.0, -0.5), (359.0, 0.2, 0.4)] {
            let mut buffer = red();
            hsl(&mut buffer, hue, sat, light);
            let pixel = buffer.pixel(0, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0),
                "hsl({hue},{sat},{light}) 越界：{pixel:?}"
            );
        }
    }

    /// 噪点必须**与分块无关**：同一像素在整幅渲染与偏移区域渲染里必须得到同一值。
    #[test]
    fn noise_is_independent_of_the_rendered_region() {
        let full = {
            let mut buffer = Buffer::new(0, 0, 32, 32);
            for y in 0..32 {
                for x in 0..32 {
                    buffer.set_pixel(x, y, [0.5, 0.5, 0.5, 1.0]);
                }
            }
            noise(&mut buffer, 0.5, 99);
            buffer
        };
        // 只渲染右下 16×16（文档坐标偏移 16,16），像素值必须与整幅一致。
        let mut tile = Buffer::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                tile.set_pixel(x, y, [0.5, 0.5, 0.5, 1.0]);
            }
        }
        noise(&mut tile, 0.5, 99);
        for y in 0..16 {
            for x in 0..16 {
                let expected = full.pixel(x + 16, y + 16);
                let actual = tile.pixel(x, y);
                assert!(
                    (expected[0] - actual[0]).abs() < 1e-6,
                    "({x},{y}) 分块噪声 {actual:?} != 整幅 {expected:?}"
                );
            }
        }
    }

    /// 暗角：用**文档尺寸**计算，因此分块渲染与整幅渲染的角落亮度一致，且角落明显暗于中心。
    #[test]
    fn vignette_darkens_corners_and_is_tile_independent() {
        let mut full = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                full.set_pixel(x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        let params = json!({"strength": 0.9, "radius": 0.2, "softness": 0.6});
        vignette(&mut full, 0.9, 0.2, 0.6, (32.0, 32.0));
        let center = full.pixel(16, 16)[0];
        let corner = full.pixel(0, 0)[0];
        assert!(
            corner < center * 0.5,
            "角落应明显暗于中心：{corner} vs {center}"
        );

        // 分块：只渲染右下 16×16 时，几何应与整幅一致（用文档坐标）。
        let mut tile = Buffer::new(16, 16, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                tile.set_pixel(x, y, [1.0, 1.0, 1.0, 1.0]);
            }
        }
        vignette(&mut tile, 0.9, 0.2, 0.6, (32.0, 32.0));
        assert!(
            (tile.pixel(0, 0)[0] - full.pixel(16, 16)[0]).abs() < 1e-6,
            "分块渲染的左上像素应与整幅渲染的 (16,16) 一致"
        );
        assert!(
            (tile.pixel(15, 15)[0] - full.pixel(31, 31)[0]).abs() < 1e-6,
            "分块渲染的右下像素应与整幅渲染的 (31,31) 一致"
        );
        let _ = params;
    }

    /// 运动模糊：结果必须等于「按角度/距离/采样数取样的源像素均值」——
    /// 这条断言把采样偏移与舍入语义**独立重算**一遍并比对，
    /// 因此它同时守护「把偏移提到循环外」这类等价改写（改了语义就会红）。
    #[test]
    fn motion_blur_matches_an_independent_average() {
        let mut source = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = ((x * 7 + y * 13) % 29) as f32 / 29.0;
                source.set_pixel(x, y, [value, value * 0.5, value * 0.25, 1.0]);
            }
        }
        let (angle, distance, samples) = (30.0f32, 6.0f32, 8u32);
        let mut blurred = source.clone();
        motion_blur(&mut blurred, angle, distance, samples);

        // 独立重算：同样的角度/距离/采样数、同样的 round 与 clamp。
        let radians = angle.to_radians();
        let (dx, dy) = (libm::cosf(radians), libm::sinf(radians));
        for y in 0..32u32 {
            for x in 0..32u32 {
                let mut sum = [0.0f32; 4];
                for i in 0..samples {
                    let t = (i as f32 / (samples - 1) as f32 - 0.5) * distance;
                    let sx = (x as f32 + dx * t).round().clamp(0.0, 31.0) as u32;
                    let sy = (y as f32 + dy * t).round().clamp(0.0, 31.0) as u32;
                    let pixel = source.pixel(sx, sy);
                    for channel in 0..4 {
                        sum[channel] += pixel[channel];
                    }
                }
                let scale = 1.0 / samples as f32;
                let expected = [
                    sum[0] * scale,
                    sum[1] * scale,
                    sum[2] * scale,
                    sum[3] * scale,
                ];
                let actual = blurred.pixel(x, y);
                for channel in 0..4 {
                    assert!(
                        (actual[channel] - expected[channel]).abs() < 1e-6,
                        "({x},{y}) 通道 {channel}: 实际 {:?} 与独立重算 {:?} 不符",
                        actual,
                        expected
                    );
                }
            }
        }
    }

    /// 胶片颗粒：同 seed 可复现、不同 seed 不同、块内一致、**与缓冲分块无关**（文档坐标决定）。
    #[test]
    fn film_grain_is_deterministic_seeded_and_tile_independent() {
        let make = |origin_x: i64| {
            let mut buffer = Buffer::new(origin_x, 0, 32, 8);
            buffer.fill([0.5, 0.5, 0.5, 1.0]);
            buffer
        };
        // 同 seed 两次结果一致。
        let mut first = make(0);
        film_grain(&mut first, 0.5, 2, 7);
        let mut second = make(0);
        film_grain(&mut second, 0.5, 2, 7);
        for y in 0..8 {
            for x in 0..32 {
                assert_eq!(
                    first.pixel(x, y),
                    second.pixel(x, y),
                    "同 seed 必须逐位一致"
                );
            }
        }
        // 不同 seed 结果不同。
        let mut other = make(0);
        film_grain(&mut other, 0.5, 2, 8);
        assert_ne!(
            first.pixel(0, 0),
            other.pixel(0, 0),
            "不同 seed 应产生不同颗粒"
        );
        // 块内一致：size=2 时同一 2×2 块内颗粒值相同。
        assert_eq!(
            first.pixel(4, 4),
            first.pixel(5, 4),
            "size=2 时同一块内应共享颗粒值"
        );
        assert_eq!(first.pixel(4, 4), first.pixel(4, 5));

        // **分块无关**：整幅渲染与「从 x=16 起的小区域」渲染在重叠像素上必须一致。
        let mut whole = make(0);
        film_grain(&mut whole, 0.5, 2, 11);
        let mut right = make(16);
        film_grain(&mut right, 0.5, 2, 11);
        for y in 0..8 {
            for x in 0..16 {
                assert_eq!(
                    whole.pixel(x + 16, y),
                    right.pixel(x, y),
                    "颗粒必须只由文档坐标决定（分块渲染不得改变结果）"
                );
            }
        }
        // amount=0 不改动；越界参数不破坏预乘不变量。
        let mut zero = make(0);
        let before = zero.pixel(3, 3);
        film_grain(&mut zero, 0.0, 99, 1);
        assert_eq!(before, zero.pixel(3, 3));
        let mut extremes = Buffer::new(0, 0, 8, 8);
        extremes.fill([0.2, 0.2, 0.2, 0.5]);
        film_grain(&mut extremes, 9.0, 0, u64::MAX);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite() && *v >= 0.0) && pixel[0] <= pixel[3] + 1e-3,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// 去雾：雾化（与大气光混合）的图像经去雾后对比度应回升；平坦雾区保持不变。
    #[test]
    fn dehaze_restores_contrast_on_a_foggy_image() {
        // 构造「彩色场景混白雾」：暗通道先验要求场景**各通道有差异**，
        // 纯灰度图会让 min(r,g,b) 等于亮度本身，先验退化（第一版测试就踩了这个）。
        let air = [0.85f32, 0.88, 0.9];
        let scene = |x: u32| -> [f32; 3] {
            if x < 16 {
                [0.08, 0.08, 0.10]
            } else {
                [0.60, 0.25, 0.20]
            }
        };
        let mut buffer = Buffer::new(0, 0, 32, 8);
        for y in 0..8 {
            for x in 0..32 {
                let base = scene(x);
                buffer.set_pixel(
                    x,
                    y,
                    [
                        base[0] * 0.5 + air[0] * 0.5,
                        base[1] * 0.5 + air[1] * 0.5,
                        base[2] * 0.5 + air[2] * 0.5,
                        1.0,
                    ],
                );
            }
        }
        // 用红通道衡量对比度：亮部偏红、暗部接近中性。
        let before_diff = buffer.pixel(28, 4)[0] - buffer.pixel(4, 4)[0];
        dehaze(&mut buffer, air, 0.9, 0.1);
        let after_diff = buffer.pixel(28, 4)[0] - buffer.pixel(4, 4)[0];
        assert!(
            after_diff > before_diff,
            "去雾应提升对比度：{before_diff} → {after_diff}"
        );
        // 平坦雾区（全为 air）经去雾后仍接近原值。
        let mut flat = Buffer::new(0, 0, 16, 16);
        for y in 0..16 {
            for x in 0..16 {
                flat.set_pixel(x, y, [air[0], air[1], air[2], 1.0]);
            }
        }
        let flat_before = flat.pixel(8, 8)[0];
        dehaze(&mut flat, air, 0.9, 0.1);
        assert!(
            (flat.pixel(8, 8)[0] - flat_before).abs() < 0.05,
            "纯大气光区域不应被大幅改动：{flat_before} → {}",
            flat.pixel(8, 8)[0]
        );
        // 预乘不变量与越界参数。
        let mut extremes = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                extremes.set_pixel(x, y, [0.2, 0.2, 0.2, 0.4]);
            }
        }
        dehaze(&mut extremes, [9.0, -9.0, 0.5], 9.0, -9.0);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite() && *v >= 0.0) && pixel[0] <= pixel[3] + 1e-3,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
        // 估计器：雾图的暗通道均值应明显大于 0，且 air 接近亮部。
        let mut rgba8 = Vec::with_capacity(32 * 8 * 4);
        for _ in 0..8 {
            for x in 0..32 {
                let value = if x < 16 { 0.55f32 } else { 0.8 };
                let byte = (value * 255.0) as u8;
                rgba8.extend_from_slice(&[byte, byte, byte, 255]);
            }
        }
        let (estimated, dark_mean) = estimate_atmospheric_light(32, 8, &rgba8).expect("估计应成功");
        assert!(dark_mean > 0.4, "雾图暗通道均值应偏高：{dark_mean}");
        assert!(estimated[0] > 0.7, "大气光应接近亮部：{estimated:?}");
    }

    /// 局部对比度：软边变陡、平坦区不变、极暗处抑制（中间调加权）。
    #[test]
    fn clarity_steepens_soft_edges_and_spares_flats() {
        // 中灰底上一条模糊的亮带（软边）。
        let mut buffer = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = if (12..20).contains(&x) { 0.6 } else { 0.4 };
                buffer.set_pixel(x, y, [value, value, value, 1.0]);
            }
        }
        // 带内 (16) 比带外 (11) 更亮；clarity 应把这一差拉大。
        let before_left = buffer.pixel(11, 16)[0];
        let before_right = buffer.pixel(16, 16)[0];
        clarity(&mut buffer, 1.0, 8);
        let after_left = buffer.pixel(11, 16)[0];
        let after_right = buffer.pixel(16, 16)[0];
        // 边缘两侧的差应被拉大（左侧更暗、右侧更亮），平坦区基本不变。
        assert!(
            (after_right - after_left) > (before_right - before_left),
            "局部对比度应拉大边缘差：{before_left}/{before_right} → {after_left}/{after_right}"
        );
        let flat_before = 0.4f32;
        let flat_after = buffer.pixel(2, 2)[0];
        assert!(
            (flat_after - flat_before).abs() < 0.02,
            "平坦区不应被改动：{flat_after}"
        );

        // 极暗处的中间调加权应抑制效果。
        let mut dark = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let value = if (12..20).contains(&x) { 0.08 } else { 0.02 };
                dark.set_pixel(x, y, [value, value, value, 1.0]);
            }
        }
        let dark_before = dark.pixel(11, 16)[0];
        clarity(&mut dark, 1.0, 8);
        let dark_delta = (dark.pixel(11, 16)[0] - dark_before).abs();
        let mid_delta = (after_left - before_left).abs();
        assert!(
            dark_delta < mid_delta,
            "极暗处的提升应弱于中间调：{dark_delta} vs {mid_delta}"
        );

        // 预乘不变量与越界参数。
        let mut extremes = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                extremes.set_pixel(x, y, [0.3, 0.3, 0.3, 0.5]);
            }
        }
        clarity(&mut extremes, 9.0, 999);
        for y in 0..8 {
            for x in 0..8 {
                let pixel = extremes.pixel(x, y);
                assert!(
                    pixel.iter().all(|v| v.is_finite())
                        && pixel[0] <= pixel[3] + 1e-3
                        && pixel[0] >= 0.0,
                    "越界参数不得破坏预乘不变量：{pixel:?}"
                );
            }
        }
    }

    /// 自然饱和度：低饱和像素提升更强；灰色不变；已饱和颜色几乎不动。
    #[test]
    fn vibrance_boosts_low_saturation_more() {
        // 灰 / 低饱和 / 高饱和 三个像素。
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.5, 0.5, 0.5, 1.0]);
            buffer.set_pixel(1, 0, [0.45, 0.5, 0.55, 1.0]);
            buffer.set_pixel(2, 0, [0.9, 0.1, 0.1, 1.0]);
            buffer
        };
        let before = make();
        let mut after = make();
        vibrance(&mut after, 1.0);
        // 灰色完全不变。
        assert_eq!(
            before.pixel(0, 0),
            after.pixel(0, 0),
            "灰色不应产生色偏（权重为 0 且差值为 0）"
        );
        // 低饱和像素的通道差应增大。
        let before_low = before.pixel(1, 0);
        let after_low = after.pixel(1, 0);
        assert!(
            (after_low[2] - after_low[0]).abs() > (before_low[2] - before_low[0]).abs(),
            "低饱和像素应被拉开：{before_low:?} → {after_low:?}"
        );
        // 比的是**相对**增益（增益 ÷ 原通道差）：饱和像素的绝对差本来就大 8 倍，
        // 直接比绝对值会得出相反结论（这是写测试时第三次踩同一个坑）。
        let before_high = before.pixel(2, 0);
        let after_high = after.pixel(2, 0);
        let low_base = (before_low[2] - before_low[0]).abs();
        let high_base = (before_high[0] - before_high[1]).abs();
        let low_relative = ((after_low[2] - after_low[0]).abs() - low_base) / low_base;
        let high_relative = ((after_high[0] - after_high[1]).abs() - high_base) / high_base;
        assert!(
            high_relative < low_relative * 0.5,
            "已饱和颜色的**相对**提升应明显更弱：low={low_relative} high={high_relative}"
        );
        // amount=0 不改动；越界参数不越界。
        let mut zero = make();
        let snapshot = zero.pixel(1, 0);
        vibrance(&mut zero, 0.0);
        assert_eq!(snapshot, zero.pixel(1, 0));
        let mut extremes = make();
        vibrance(&mut extremes, 9.0);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界 amount 不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 分离色调：暗部取阴影色、亮部取高光色；`balance` 移动分界。
    #[test]
    fn split_toning_tints_shadows_and_highlights() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.05, 0.05, 0.05, 1.0]);
            buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]);
            // 亮部用 0.6 而不是 0.95：后者加上着色会直接触顶被钳，看不出通道差。
            buffer.set_pixel(2, 0, [0.6, 0.6, 0.6, 1.0]);
            buffer
        };
        // 阴影加蓝、高光加黄。
        let mut toned = make();
        split_toning(&mut toned, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 0.0, 1.0);
        let dark = toned.pixel(0, 0);
        let bright = toned.pixel(2, 0);
        assert!(dark[2] > dark[0] + 0.2, "暗部应偏蓝：{dark:?}");
        // 高光权重在该亮度约 0.78，所以通道差约 0.17（不是满额 0.3）。
        assert!(bright[0] > bright[2] + 0.1, "亮部应偏黄：{bright:?}");

        // balance 把分界点推向亮部（mid 0.5 → 0.75），因此中间灰落入**阴影侧**。
        let mut shifted = make();
        split_toning(&mut shifted, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 1.0, 1.0);
        let neutral = shifted.pixel(1, 0);
        assert!(
            neutral[2] > neutral[0],
            "balance=+1 把分界推高，中间灰应取阴影色：{neutral:?}"
        );
        // 反向（mid → 0.25）时中间灰落入高光侧。
        let mut back = make();
        split_toning(&mut back, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], -1.0, 1.0);
        let neutral_back = back.pixel(1, 0);
        assert!(
            neutral_back[0] > neutral_back[2],
            "balance=-1 把分界压低，中间灰应取高光色：{neutral_back:?}"
        );

        // amount=0 不改动；越界参数不产生非法值。
        let mut zero = make();
        let before = zero.pixel(1, 0);
        split_toning(&mut zero, [1.0; 3], [1.0; 3], 0.0, 0.0);
        assert_eq!(before, zero.pixel(1, 0));
        let mut extremes = make();
        split_toning(&mut extremes, [9.0; 3], [-9.0; 3], 9.0, 9.0);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界参数不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 色彩平衡：阴影档只影响暗部、高光档只影响亮部，中间调影响两者之间的像素。
    #[test]
    fn color_balance_affects_tone_bands_separately() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.05, 0.05, 0.05, 1.0]); // 暗
            buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]); // 中
            buffer.set_pixel(2, 0, [0.95, 0.95, 0.95, 1.0]); // 亮
            buffer
        };
        // 只加阴影档的红：暗部提升最多，亮部几乎不变。
        let mut shadows = make();
        color_balance(&mut shadows, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3]);
        // 注意比较的是**增量**而不是绝对值：绝对值当然仍随底色增大。
        let dark_delta = shadows.pixel(0, 0)[0] - 0.05;
        let mid_delta = shadows.pixel(1, 0)[0] - 0.5;
        let bright_delta = shadows.pixel(2, 0)[0] - 0.95;
        assert!(dark_delta > 0.2, "暗部应被显著提升：{dark_delta}");
        assert!(
            dark_delta > mid_delta && mid_delta > bright_delta,
            "阴影档增量应随亮度递减：{dark_delta}/{mid_delta}/{bright_delta}"
        );
        assert!(bright_delta < 0.01, "亮部几乎不变：{bright_delta}");

        // 只加高光档的蓝：亮部提升最多。
        let mut highlights = make();
        color_balance(&mut highlights, [0.0; 3], [0.0; 3], [0.0, 0.0, 0.3]);
        // 0.95 加 0.271 会触顶被钳到 1.0，因此断言「饱和到白」而不是比增量。
        let bright_blue = highlights.pixel(2, 0)[2];
        let dark_blue_delta = highlights.pixel(0, 0)[2] - 0.05;
        assert!(
            bright_blue > 0.999,
            "亮部高光档应把蓝推到饱和：{bright_blue}"
        );
        assert!(dark_blue_delta < 0.01, "暗部几乎不变：{dark_blue_delta}");

        // 中间调 + 越界参数不应产出非法值。
        let mut extremes = make();
        color_balance(&mut extremes, [9.0, -9.0, 0.0], [1.0; 3], [0.0, 0.0, -9.0]);
        for x in 0..3 {
            let pixel = extremes.pixel(x, 0);
            assert!(
                pixel
                    .iter()
                    .all(|v| (0.0..=1.0).contains(v) && v.is_finite()),
                "越界参数不得产出非法值：{pixel:?}"
            );
        }
    }

    /// 分通道色阶只影响指定通道；色调分离把通道量化到有限台阶。
    #[test]
    fn levels_channel_and_posterize() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 1, 1);
            buffer.set_pixel(0, 0, [0.25, 0.5, 0.75, 1.0]);
            buffer
        };
        let mut red_only = make();
        levels_channel(&mut red_only, 0.0, 1.0, 1.0, "r");
        let pixel = red_only.pixel(0, 0);
        assert!(
            (pixel[1] - 0.5).abs() < 1e-3 && (pixel[2] - 0.75).abs() < 1e-3,
            "只应改红通道：{pixel:?}"
        );

        // 色调分离 3 级（台阶 0 / 0.5 / 1）：用**非中点**输入避免半数舍入的歧义。
        let mut quantized = Buffer::new(0, 0, 1, 1);
        quantized.set_pixel(0, 0, [0.2, 0.5, 0.8, 1.0]);
        posterize(&mut quantized, 3);
        let pixel = quantized.pixel(0, 0);
        assert!((pixel[0] - 0.0).abs() < 1e-3, "0.2 应量化到 0：{pixel:?}");
        assert!((pixel[1] - 0.5).abs() < 1e-3, "0.5 应保持：{pixel:?}");
        assert!((pixel[2] - 1.0).abs() < 1e-3, "0.8 应量化到 1：{pixel:?}");
        // 极端级别数不应产出越界值。
        for levels in [2u32, 4, 64, 1000] {
            let mut buffer = make();
            posterize(&mut buffer, levels);
            let pixel = buffer.pixel(0, 0);
            assert!(
                pixel.iter().all(|v| (0.0..=1.0).contains(v)),
                "levels={levels} 越界：{pixel:?}"
            );
        }
    }

    #[test]
    fn levels_maps_black_and_white_points() {
        let mut buffer = Buffer::new(0, 0, 3, 1);
        buffer.set_pixel(0, 0, [0.25, 0.25, 0.25, 1.0]);
        buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]);
        buffer.set_pixel(2, 0, [0.75, 0.75, 0.75, 1.0]);
        levels(&mut buffer, 0.25, 0.75, 1.0);
        assert!(buffer.pixel(0, 0)[0].abs() < 1e-5);
        assert!((buffer.pixel(1, 0)[0] - 0.5).abs() < 1e-5);
        assert!((buffer.pixel(2, 0)[0] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn box_blur_distributes_energy_and_keeps_alpha() {
        let mut buffer = Buffer::new(0, 0, 8, 1);
        buffer.set_pixel(4, 0, [1.0, 0.0, 0.0, 1.0]);
        box_blur(&mut buffer, 1, 1);
        // 中心保留大部分能量，相邻像素获得能量。
        assert!(buffer.pixel(4, 0)[0] < 1.0);
        assert!(buffer.pixel(3, 0)[0] > 0.0);
        assert!(buffer.pixel(5, 0)[0] > 0.0);
        let total_alpha: f32 = (0..8).map(|x| buffer.pixel(x, 0)[3]).sum();
        assert!((total_alpha - 1.0).abs() < 0.3, "alpha 总能量近似守恒");

        // 模糊后 alpha 最大值下降，但不会出现负值。
        for x in 0..8 {
            let pixel = buffer.pixel(x, 0);
            assert!(pixel[0] >= 0.0 && pixel[3] >= 0.0);
        }
    }

    #[test]
    fn gaussian_blur_is_symmetric_and_normalized() {
        let mut buffer = Buffer::new(0, 0, 9, 1);
        buffer.set_pixel(4, 0, [1.0, 1.0, 1.0, 1.0]);
        gaussian_blur(&mut buffer, 1.0);
        let center = buffer.pixel(4, 0);
        assert!(center[3] > 0.2 && center[3] < 0.6, "center={center:?}");
        for offset in 1..=3u32 {
            let left = buffer.pixel(4 - offset, 0);
            let right = buffer.pixel(4 + offset, 0);
            assert!((left[3] - right[3]).abs() < 1e-6, "对称性");
        }
        assert_eq!(box_blur_padding(), 0);
    }

    fn box_blur_padding() -> u32 {
        FilterKind::BoxBlur.padding(&json!({"radius": 0}))
    }

    #[test]
    fn adjustment_and_filter_dispatch_by_name() {
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "brightness_contrast",
            &json!({"brightness": 0.1, "contrast": 1.0}),
            1.0
        ));
        assert!((buffer.pixel(0, 0)[0] - 0.6).abs() < 1e-5);
        assert!(!apply_adjustment(&mut buffer, "unknown", &json!({}), 1.0));
        assert!(apply_filter(
            &mut buffer,
            "invert",
            &json!({}),
            1.0,
            (2.0, 1.0)
        ));
        assert!((buffer.pixel(0, 0)[0] - 0.4).abs() < 1e-5);
        assert!(!apply_filter(
            &mut buffer,
            "unknown",
            &json!({}),
            1.0,
            (2.0, 1.0)
        ));

        // opacity 插值：一半强度。
        let mut buffer = filled([0.0, 0.0, 0.0, 1.0]);
        apply_filter(&mut buffer, "invert", &json!({}), 0.5, (2.0, 1.0));
        assert!((buffer.pixel(0, 0)[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn filter_padding_reports_neighborhood_needs() {
        assert_eq!(
            FilterKind::BoxBlur.padding(&json!({"radius": 3, "passes": 2})),
            6
        );
        assert_eq!(FilterKind::GaussianBlur.padding(&json!({"sigma": 2.0})), 6);
        assert_eq!(FilterKind::Invert.padding(&json!({})), 0);
    }

    #[test]
    fn curves_lightness_channel_brightens_without_hue_shift() {
        // **Lab 明度曲线**：L 提高，a/b（色相）基本不动 ✓ —— 这是 RGB 曲线做不到的 ✓。
        // 注：用砖红而非纯红——纯红提亮 +27 会超色域被 clamp 剪掉，a/b 漂移是正确的色域裁剪，
        // 不是曲线错（实测 L 84 a 8.2 b 20.2 全程在色域内 ✓）。
        let mut buffer = filled([0.4, 0.25, 0.15, 1.0]);
        let before = crate::color::linear_to_lab([0.4, 0.25, 0.15]);
        curves(&mut buffer, &[(0.0, 0.0), (0.5, 0.8), (1.0, 1.0)], "l");
        let p = buffer.pixel(0, 0);
        let after = crate::color::linear_to_lab([p[0], p[1], p[2]]);
        assert!(
            after[0] > before[0] + 10.0,
            "L 应明显提高：{before:?} → {after:?}"
        );
        assert!(
            (after[1] - before[1]).abs() < 1.0 && (after[2] - before[2]).abs() < 1.0,
            "a/b 应基本不动（色相守恒）：{before:?} → {after:?}"
        );
    }

    #[test]
    fn curves_hue_channel_rotates_hue() {
        // **HSL 色相曲线**：青（h=1/2）→ 黄绿（h=1/4）✓。
        // 用青不用绿：绿的 s=1.0 和 h=1/3 在某些曲线下撞车，测不出"动的是 h 还是 s"（变异验证教训 ✓）。
        let mut buffer = filled([0.0, 0.5, 0.5, 1.0]);
        curves(&mut buffer, &[(0.0, 0.0), (0.5, 0.25), (1.0, 1.0)], "h");
        let p = buffer.pixel(0, 0);
        let (h_after, ..) = super::hsl_decompose(&[p[0], p[1], p[2]]);
        assert!(
            (h_after - 0.25).abs() < 1e-3,
            "色相应为 1/4：{p:?}（h={h_after}）"
        );
    }

    #[test]
    fn curves_saturation_channel_desaturates_to_gray() {
        // **HSL 饱和度曲线**：全压到 0 ⇒ 灰，明度不变 ✓。
        let mut buffer = filled([0.8, 0.2, 0.1, 1.0]);
        let (.., l_before) = super::hsl_decompose(&[0.8, 0.2, 0.1]);
        curves(&mut buffer, &[(0.0, 0.0), (1.0, 0.0)], "s");
        let p = buffer.pixel(0, 0);
        assert!(
            (p[0] - p[1]).abs() < 1e-3 && (p[1] - p[2]).abs() < 1e-3,
            "应成灰：{p:?}"
        );
        assert!(
            (p[0] - l_before).abs() < 1e-3,
            "明度应守恒：{p:?} vs {l_before}"
        );
    }

    #[test]
    fn curves_unknown_channel_still_hits_rgb_fallback() {
        // **未知通道走 rgb 回退**（与原来一致 ✓）—— 工具层校验挡住了非法值，
        // 这里守的是"实现不 panic"。
        let mut buffer = filled([0.2, 0.4, 0.6, 1.0]);
        curves(&mut buffer, &[(0.0, 0.0), (1.0, 1.0)], "nope");
        let p = buffer.pixel(0, 0);
        assert!((p[0] - 0.2).abs() < 1e-4 && (p[2] - 0.6).abs() < 1e-4);
    }

    #[test]
    fn hsl_helpers_are_inverse_of_each_other() {
        // **分解/组装互逆** ✓ —— hsl() 重构后行为不变的依据 ✓。
        for color in [
            [1.0, 0.0, 0.0],
            [0.2, 0.5, 0.8],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ] {
            let (h, s, l) = super::hsl_decompose(&color);
            let back = super::hsl_compose(h, s, l);
            for channel in 0..3 {
                assert!(
                    (back[channel] - color[channel]).abs() < 1e-5,
                    "{color:?} 往返：{back:?}"
                );
            }
        }
    }
}

/// **★ P2 #15 厚涂浮雕的决定性判据（数据层）★**（第 254 轮）
///
/// **要证的命题**：
///   ① `strength = 0` => **一个像素都不许动**（opt-in 语义）；
///   ② **光向真的起作用且有方向**：同一块"隆起"（钟形 alpha = 一堆厚涂颜料），
///      光从**右**来 => **右坡（迎光）更亮**；光从**左**来 => **反号**。
///   ③ 只作用在有颜料的地方（空白保持透明、不上色）；
///   ④ 确定性（同参数逐位相同），且**整体亮度不该被抬走**（浮雕是有方向偏置，不是全局提亮）。
///
/// **★ 为什么样本必须是"隆起"而不是"线性坡面" ★**（本会话实测教训）：
/// 恒定斜率的坡面**每一点法向相同** => 亮度差只在**边缘**出现 => 拿"左右亮度差"去量
/// 其实是在量噪声（实测右光 −0.0063／左光 −0.0285，两次同号，我差点以为符号写反）。
/// 只有**隆起**才有"迎光坡 vs 背光坡"的对比。
#[cfg(test)]
mod relief_tests {
    use super::relief;
    use crate::buffer::Buffer;

    /// 中间隆起的"厚涂堆"：alpha 呈钟形，颜色恒为中性灰。
    fn bump(w: u32, h: u32) -> Buffer {
        let mut buffer = Buffer::new(0, 0, w, h);
        let pixels = buffer.pixels_mut();
        for y in 0..h {
            for x in 0..w {
                let at = ((y * w + x) * 4) as usize;
                let dx = x as f32 / (w - 1).max(1) as f32 - 0.5;
                let dy = y as f32 / (h - 1).max(1) as f32 - 0.5;
                let bell = (1.0 - (dx * dx + dy * dy) * 8.0).clamp(0.0, 1.0);
                pixels[at] = 0.5;
                pixels[at + 1] = 0.5;
                pixels[at + 2] = 0.5;
                pixels[at + 3] = bell;
            }
        }
        buffer
    }

    /// 某一侧坡面的平均亮度（只看有颜料的像素）：`left = true` 取左半边靠外的一带。
    fn flank_luma(buffer: &Buffer, left: bool) -> f32 {
        let (w, h) = (buffer.width(), buffer.height());
        let pixels = buffer.pixels();
        let mut sum: f32 = 0.0;
        let mut n: f32 = 0.0;
        for y in 0..h {
            for x in 0..w {
                let in_band = if left {
                    x >= w / 4 && x < w / 2
                } else {
                    x >= w / 2 && x < w * 3 / 4
                };
                if !in_band {
                    continue;
                }
                let at = ((y * w + x) * 4) as usize;
                if pixels[at + 3] > 0.05 {
                    sum += (pixels[at] + pixels[at + 1] + pixels[at + 2]) / 3.0;
                    n += 1.0;
                }
            }
        }
        sum / n.max(1.0)
    }

    fn mean_luma(buffer: &Buffer) -> f32 {
        let pixels = buffer.pixels();
        let mut sum: f32 = 0.0;
        let mut n: f32 = 0.0;
        for chunk in pixels.chunks_exact(4) {
            if chunk[3] > 0.05 {
                sum += (chunk[0] + chunk[1] + chunk[2]) / 3.0;
                n += 1.0;
            }
        }
        sum / n.max(1.0)
    }

    #[test]
    fn relief_needs_light_direction_and_is_off_by_default() {
        let base = bump(64, 32);
        // ① strength = 0 => 原样
        let mut untouched = base.clone();
        relief(&mut untouched, 0.0, 45.0, 0.0, 30.0);
        assert_eq!(untouched.pixels(), base.pixels(), "strength=0 不许动像素");
        // ② 光从右 vs 从左
        let mut from_right = base.clone();
        relief(&mut from_right, 0.0, 45.0, 0.8, 30.0);
        let mut from_left = base.clone();
        relief(&mut from_left, 180.0, 45.0, 0.8, 30.0);
        let right_light = flank_luma(&from_right, false) - flank_luma(&from_right, true);
        let left_light = flank_luma(&from_left, false) - flank_luma(&from_left, true);
        assert!(
            right_light > 0.0,
            "光从右侧来时**右坡（迎光）**应当更亮（右坡−左坡 = {right_light}）"
        );
        assert!(
            left_light < 0.0,
            "光从左侧来时右坡应当更暗（右坡−左坡 = {left_light}）"
        );
        assert!(
            right_light * left_light < 0.0,
            "换光源方向必须让明暗对比**反号**（右光 {right_light} / 左光 {left_light}）"
        );
        // ③ 空白处不上色
        let pixels = from_right.pixels();
        assert!(pixels[3] <= 1e-6, "左上角空白不该被上色");
        // ④ 确定性 + 整体亮度不被抬走
        let mut again = base.clone();
        relief(&mut again, 0.0, 45.0, 0.8, 30.0);
        assert_eq!(again.pixels(), from_right.pixels(), "同参数必须逐位相同");
        assert!(
            (mean_luma(&from_right) - mean_luma(&base)).abs() < 0.25,
            "浮雕不该把整体亮度抬走（{} vs {}）",
            mean_luma(&from_right),
            mean_luma(&base)
        );
    }
}

// **★ `relief` 不该"整体提亮"（第 254 轮实测发现的设计缺陷）★**
//
// **实测背景**（400×300、软边厚涂堆、`height_gain: 24`）：`strength` 越大，画面**均值一路涨**
// （0→240.6、5→251.7、10→252.0）而**对比反而塌**（标准差 37.8 → 11.4 → 10.8）✗。
// **根因**：明度因子 `1 + strength·(n·l)` 里的 `n·l` 含**常数环境项** `l_z = light[2] ≈ 0.707`
// ⇒ 每一项都被乘上 `1 + 0.707·strength` ⇒ **一律提亮** ✗（不是只有迎光坡提亮）。
//
// **本判据要的两条**：
//   ① **均值不漂**：`strength = 5` 与 `strength = 0` 的平均亮度差 ≤ 2（实测 +11 ⇒ **今天必红** ✓）；
//   ② **对比不塌**：`strength = 5` 的标准差不低于 `strength = 0` 的 0.9 倍
//      （浮雕应当**增强**局部明暗，而不是把它抹平）。
#[cfg(test)]
mod relief_ambient_tests {
    use super::relief;
    use crate::buffer::Buffer;

    /// 中间隆起的厚涂堆（与 `relief_tests::bump` 同一套几何 ✓）。
    fn bump(w: u32, h: u32) -> Buffer {
        let mut buffer = Buffer::new(0, 0, w, h);
        let pixels = buffer.pixels_mut();
        for y in 0..h {
            for x in 0..w {
                let at = ((y * w + x) * 4) as usize;
                let dx = x as f32 / (w - 1).max(1) as f32 - 0.5;
                let dy = y as f32 / (h - 1).max(1) as f32 - 0.5;
                let bell = (1.0 - (dx * dx + dy * dy) * 8.0).clamp(0.0, 1.0);
                pixels[at] = 0.5;
                pixels[at + 1] = 0.5;
                pixels[at + 2] = 0.5;
                pixels[at + 3] = bell;
            }
        }
        buffer
    }

    fn stats(buffer: &Buffer) -> (f32, f32) {
        let pixels = buffer.pixels();
        let mut sum = 0.0f32;
        let mut sum_sq = 0.0f32;
        let mut n = 0.0f32;
        for chunk in pixels.chunks_exact(4) {
            if chunk[3] > 0.05 {
                let luma = (chunk[0] + chunk[1] + chunk[2]) / 3.0;
                sum += luma;
                sum_sq += luma * luma;
                n += 1.0;
            }
        }
        let mean = sum / n.max(1.0);
        let variance = (sum_sq / n.max(1.0) - mean * mean).max(0.0);
        (mean, variance.sqrt())
    }

    #[test]
    fn relief_strength_must_not_brighten_the_whole_picture() {
        let base = bump(64, 32);
        let (base_mean, base_sd) = stats(&base);
        let mut strong = base.clone();
        relief(&mut strong, 135.0, 45.0, 5.0, 24.0);
        let (strong_mean, strong_sd) = stats(&strong);
        // **★ 容差必须紧 ★**：这里的 luma 是 **0..1** 单位 ⇒ 0.05 已经是"5% 的漂移" ✓
        //   （我第一版写成 `<= 2.0` ⇒ 在 0..1 单位里等于**不设限** ✗ ⇒ 判据空转、当场假绿 ✗）。
        assert!(
            (strong_mean - base_mean).abs() <= 0.05,
            "`strength = 5` 把整幅提亮了（均值 {base_mean:.4} ⇒ {strong_mean:.4}，容差 0.05）—— \
             `n·l` 里的常数环境项 `light[2]` 必须减掉，否则 `strength` 一大就过曝 ✗"
        );
        // **浮雕的"效果存在"**：不是所有像素都停在底色上 ✓（否则"均值不漂"可以靠"什么都不做"骗过去 ✗）。
        let base = bump(64, 32);
        let mut mid = base.clone();
        relief(&mut mid, 135.0, 45.0, 1.0, 24.0);
        let changed = mid
            .pixels()
            .chunks_exact(4)
            .zip(base.pixels().chunks_exact(4))
            .filter(|(a, b)| a[3] > 0.05 && (a[0] - b[0]).abs() > 1e-4)
            .count();
        assert!(
            changed > 0,
            "`strength = 1` 一个像素都没改 ⇒ 那「均值不漂」是假的（什么都没做当然不漂 ✗）"
        );
        assert!(
            strong_sd > base_sd,
            "`strength = 5` 之后**对比没有增长**（标准差 {base_sd:.4} ⇒ {strong_sd:.4}）             —— 浮雕应当**增强**局部明暗（修复后实测 +1.4）✗"
        );
    }
}

#[cfg(test)]
mod coloradj_tests {
    use super::*;
    use serde_json::json;

    fn filled(color: [f32; 4]) -> Buffer {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        buffer.fill(color);
        buffer
    }

    fn px(buffer: &Buffer) -> [f32; 3] {
        let pixel = buffer.pixel(0, 0);
        [pixel[0], pixel[1], pixel[2]]
    }

    fn assert_rgb_close(actual: [f32; 3], expected: [f32; 3], label: &str) {
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-5,
                "{label} 通道 {channel}：期望 {}，实得 {}",
                expected[channel],
                actual[channel]
            );
        }
    }

    // ── black_and_white ─────────────────────────────────────────────

    /// 权重 [1,0,0] ⇒ 输出是红通道值的灰阶（三通道同值且等于输入红通道）。
    #[test]
    fn black_and_white_red_only_weights_extract_the_red_channel() {
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [1.0, 0.0, 0.0], None);
        let out = px(&buffer);
        assert_eq!(out[0], out[1], "输出必须三通道同值");
        assert_eq!(out[1], out[2], "输出必须三通道同值");
        assert!(
            (out[0] - 0.7).abs() < 1e-6,
            "输出应等于红通道值，实得 {out:?}"
        );
    }

    /// 恒等式：灰像素在**任意**权重（和为 1）下不变——含默认 Rec709。
    #[test]
    fn black_and_white_gray_pixels_are_fixed_points_for_any_unit_sum_weights() {
        for weights in [[0.2126, 0.7152, 0.0722], [1.0, 0.0, 0.0], [0.5, 0.25, 0.25]] {
            let mut buffer = filled([0.42, 0.42, 0.42, 1.0]);
            black_and_white(&mut buffer, weights, None);
            assert_rgb_close(px(&buffer), [0.42, 0.42, 0.42], "灰像素应不变");
        }
    }

    /// tint 把灰阶按比例着色：白（灰阶 1）配 tint ⇒ 输出 = tint 本身。
    #[test]
    fn black_and_white_tint_scales_the_gray() {
        let mut buffer = filled([1.0, 1.0, 1.0, 1.0]);
        black_and_white(
            &mut buffer,
            [0.2126, 0.7152, 0.0722],
            Some([0.5, 0.25, 0.0]),
        );
        assert_rgb_close(px(&buffer), [0.5, 0.25, 0.0], "白经 tint 后应等于 tint");
    }

    // ── channel_mixer ───────────────────────────────────────────────

    /// 单位阵 + 零常数 ⇔ 恒等（封住行列写反／漏加常数两类错）。
    #[test]
    fn channel_mixer_identity_matrix_is_a_fixed_point() {
        let mut buffer = filled([0.31, 0.57, 0.83, 1.0]);
        channel_mixer(
            &mut buffer,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [0.0, 0.0, 0.0],
        );
        assert_rgb_close(px(&buffer), [0.31, 0.57, 0.83], "单位阵应恒等");
    }

    /// 交换 r/b 两行 ⇒ 红蓝互换、绿不动。
    #[test]
    fn channel_mixer_swapping_rows_swaps_channels() {
        let mut buffer = filled([0.2, 0.5, 0.9, 1.0]);
        channel_mixer(
            &mut buffer,
            [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]],
            [0.0, 0.0, 0.0],
        );
        assert_rgb_close(px(&buffer), [0.9, 0.5, 0.2], "红蓝应互换");
    }

    /// 零矩阵 + 常数 ⇒ 每个像素都等于常数（与输入无关）。
    #[test]
    fn channel_mixer_zero_matrix_yields_the_constants() {
        let mut buffer = filled([0.77, 0.11, 0.55, 1.0]);
        channel_mixer(&mut buffer, [[0.0; 3]; 3], [0.2, 0.3, 0.4]);
        assert_rgb_close(px(&buffer), [0.2, 0.3, 0.4], "零矩阵输出应等于常数");
    }

    // ── gradient_map ────────────────────────────────────────────────

    /// 黑白两 stop：输出 = Rec709 明度的灰（纯红 ⇒ 0.2126 的灰，三通道同值）。
    #[test]
    fn gradient_map_black_white_stops_reduce_to_luma() {
        let stops = [(0.0, [0.0, 0.0, 0.0]), (1.0, [1.0, 1.0, 1.0])];
        let mut buffer = filled([1.0, 0.0, 0.0, 1.0]);
        gradient_map(&mut buffer, &stops);
        let out = px(&buffer);
        assert!(
            (out[0] - 0.2126).abs() < 1e-5,
            "纯红的明度应为 0.2126，实得 {out:?}"
        );
        assert_eq!(out[0], out[1]);
        assert_eq!(out[1], out[2]);
    }

    /// 端点精确：明度 0 的像素精确命中第一个 stop 色、明度 1 的精确命中最后一个。
    #[test]
    fn gradient_map_endpoints_hit_the_stop_colours_exactly() {
        let stops = [(0.0, [0.1, 0.2, 0.3]), (1.0, [0.9, 0.8, 0.7])];
        let mut dark = filled([0.0, 0.0, 0.0, 1.0]);
        gradient_map(&mut dark, &stops);
        assert_eq!(px(&dark), [0.1, 0.2, 0.3], "黑应精确等于首色标");
        let mut bright = filled([1.0, 1.0, 1.0, 1.0]);
        gradient_map(&mut bright, &stops);
        assert_eq!(px(&bright), [0.9, 0.8, 0.7], "白应精确等于末色标");
    }

    /// 中点 stop 也精确：灰 0.5 三段色带 ⇒ 精确命中中间 stop。
    #[test]
    fn gradient_map_middle_stop_is_hit_exactly_at_its_position() {
        let stops = [
            (0.0, [0.0, 0.0, 0.0]),
            (0.5, [0.25, 0.5, 0.75]),
            (1.0, [1.0, 1.0, 1.0]),
        ];
        let mut buffer = filled([0.5, 0.5, 0.5, 1.0]);
        gradient_map(&mut buffer, &stops);
        assert_eq!(px(&buffer), [0.25, 0.5, 0.75], "明度 0.5 应命中中间色标");
    }

    // ── shadows_highlights ──────────────────────────────────────────

    /// 0/0 ⇒ 恒等（任意像素）。
    #[test]
    fn shadows_highlights_zero_is_identity() {
        let mut buffer = filled([0.23, 0.61, 0.05, 1.0]);
        shadows_highlights(&mut buffer, 0.0, 0.0);
        assert_rgb_close(px(&buffer), [0.23, 0.61, 0.05], "0/0 应恒等");
    }

    /// shadows>0：白色**恰好**不动（权重为 0）、暗部被提亮；
    /// 且暗部提升量 > 亮部提升量（掩码单调）。
    #[test]
    fn shadows_lift_shadows_only() {
        let mut white = filled([1.0, 1.0, 1.0, 1.0]);
        shadows_highlights(&mut white, 1.0, 0.0);
        assert_eq!(px(&white), [1.0, 1.0, 1.0], "白色在 shadows 下必须恰好不动");

        let mut dark = filled([0.1, 0.1, 0.1, 1.0]);
        shadows_highlights(&mut dark, 1.0, 0.0);
        let dark_out = px(&dark)[0];
        assert!(dark_out > 0.1, "暗部应被提亮，实得 {dark_out}");

        let mut bright = filled([0.9, 0.9, 0.9, 1.0]);
        shadows_highlights(&mut bright, 1.0, 0.0);
        let bright_lift = px(&bright)[0] - 0.9;
        assert!(
            dark_out - 0.1 > bright_lift,
            "暗部提升量（{}）应大于亮部（{bright_lift}）",
            dark_out - 0.1
        );
    }

    /// highlights<0：黑色**恰好**不动、高光被压暗。
    #[test]
    fn highlights_press_highlights_only() {
        let mut black = filled([0.0, 0.0, 0.0, 1.0]);
        shadows_highlights(&mut black, 0.0, -1.0);
        assert_eq!(
            px(&black),
            [0.0, 0.0, 0.0],
            "黑色在 highlights 下必须恰好不动"
        );

        let mut bright = filled([0.9, 0.9, 0.9, 1.0]);
        shadows_highlights(&mut bright, 0.0, -1.0);
        assert!(px(&bright)[0] < 0.9, "高光应被压暗，实得 {:?}", px(&bright));
    }

    // ── selective_color ─────────────────────────────────────────────

    fn ranges_with(index: usize, offset: [f32; 4]) -> [[f32; 4]; 9] {
        let mut ranges = [[0.0f32; 4]; 9];
        ranges[index] = offset;
        ranges
    }

    /// 全零偏移 ⇒ CMYK 往返恒等（含黑/白/灰/纯色）。
    #[test]
    fn selective_color_zero_offsets_are_identity() {
        for color in [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.5, 0.5, 0.5, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.13, 0.47, 0.91, 1.0],
        ] {
            let mut buffer = filled(color);
            selective_color(&mut buffer, &[[0.0; 4]; 9]);
            assert_rgb_close(
                px(&buffer),
                [color[0], color[1], color[2]],
                "全零偏移应恒等",
            );
        }
    }

    /// reds 加 cyan ⇒ 纯红的 c 从 0 涨上来（红通道被吃掉、变出青）；
    /// 纯绿不属于 reds ⇒ 恰好不动。
    #[test]
    fn selective_color_reds_cyan_affects_red_not_green() {
        // SELECTIVE_COLOR_RANGES 的 reds 下标是 0。
        let ranges = ranges_with(0, [1.0, 0.0, 0.0, 0.0]);
        let mut red = filled([1.0, 0.0, 0.0, 1.0]);
        selective_color(&mut red, &ranges);
        let out = px(&red);
        assert!(
            out[0] < 0.01,
            "纯红加满 cyan 后红通道应被吃光（c=1），实得 {out:?}"
        );
        let mut green = filled([0.0, 1.0, 0.0, 1.0]);
        selective_color(&mut green, &ranges);
        assert_eq!(px(&green), [0.0, 1.0, 0.0], "纯绿不属于 reds，必须恰好不动");
    }

    /// 色类路由：中间灰属 neutrals——动 neutrals 有效、动 whites 无效。
    #[test]
    fn selective_color_gray_routes_to_neutrals_not_whites() {
        let mut via_neutrals = filled([0.5, 0.5, 0.5, 1.0]);
        selective_color(&mut via_neutrals, &ranges_with(7, [0.0, 0.0, 0.0, 1.0]));
        assert_eq!(px(&via_neutrals), [0.0, 0.0, 0.0], "中间灰加满 k 应变黑");

        let mut via_whites = filled([0.5, 0.5, 0.5, 1.0]);
        selective_color(&mut via_whites, &ranges_with(6, [0.0, 0.0, 0.0, 1.0]));
        assert_eq!(px(&via_whites), [0.5, 0.5, 0.5], "中间灰不动 whites 档");
    }

    // ── color_lut ───────────────────────────────────────────────────

    /// 恒等 LUT（size=2）：格点值 = 格点坐标 ⇒ 任意颜色原样返回（含中点三线性）。
    fn identity_table(size: usize) -> Vec<f32> {
        let mut table = Vec::new();
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    let denom = (size - 1) as f32;
                    table.extend_from_slice(&[
                        r as f32 / denom,
                        g as f32 / denom,
                        b as f32 / denom,
                    ]);
                }
            }
        }
        table
    }

    #[test]
    fn color_lut_identity_table_is_a_fixed_point() {
        let table = identity_table(2);
        for color in [
            [0.0, 0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.5, 0.5, 0.5, 1.0],
            [0.25, 0.6, 0.9, 1.0],
        ] {
            let mut buffer = filled(color);
            color_lut(&mut buffer, 2, &table);
            assert_rgb_close(
                px(&buffer),
                [color[0], color[1], color[2]],
                "恒等 LUT 应不动颜色",
            );
        }
    }

    /// 角点精确：反相 LUT 下输入纯红（格点）⇒ 精确输出该格点值 (0,1,1)。
    #[test]
    fn color_lut_corner_values_are_hit_exactly() {
        let mut table = identity_table(2);
        // 把每个格点改成反相（1 − 格点）。
        for value in table.iter_mut() {
            *value = 1.0 - *value;
        }
        let mut buffer = filled([1.0, 0.0, 0.0, 1.0]);
        color_lut(&mut buffer, 2, &table);
        assert_eq!(px(&buffer), [0.0, 1.0, 1.0], "角点输入应精确命中角点输出");
    }

    /// 表形状不符 ⇒ 原样返回（渲染层对坏参数不许动像素）。
    #[test]
    fn color_lut_malformed_table_leaves_pixels_untouched() {
        let mut buffer = filled([0.3, 0.4, 0.5, 1.0]);
        color_lut(&mut buffer, 2, &[0.0, 0.0, 0.0]);
        assert_rgb_close(px(&buffer), [0.3, 0.4, 0.5], "坏表不应改像素");
    }

    // ── 接线（apply_adjustment + 名字往返）────────────────────────────

    /// 6 个新名字都能被 apply_adjustment 认出来（返回 true），且空参数不改像素的期望：
    /// black_and_white 例外——它空参数 = 默认 Rec709 转灰阶，断言输出三通道同值。
    #[test]
    fn apply_adjustment_recognises_the_six_new_names() {
        for name in [
            "black_and_white",
            "channel_mixer",
            "gradient_map",
            "shadows_highlights",
            "selective_color",
            "color_lut",
        ] {
            let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
            assert!(
                apply_adjustment(&mut buffer, name, &json!({}), 1.0),
                "{name} 未被 apply_adjustment 识别"
            );
        }
        // 默认参数下各调整的恒等/确定行为：
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "channel_mixer",
            &json!({}),
            1.0
        ));
        assert_rgb_close(px(&buffer), [0.5, 0.3, 0.1], "channel_mixer 缺省应恒等");
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "shadows_highlights",
            &json!({}),
            1.0
        ));
        assert_rgb_close(
            px(&buffer),
            [0.5, 0.3, 0.1],
            "shadows_highlights 缺省应恒等",
        );
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "black_and_white",
            &json!({}),
            1.0
        ));
        let out = px(&buffer);
        assert_eq!(out[0], out[1], "black_and_white 缺省输出应为灰阶");
        assert_eq!(out[1], out[2], "black_and_white 缺省输出应为灰阶");
    }

    /// 名字往返：ALL 与 ADJUSTMENT_NAMES 恰好同长且一一对应（两张清单不许漂移）。
    #[test]
    fn adjustment_kind_names_roundtrip() {
        assert_eq!(
            AdjustmentKind::ALL.len(),
            ADJUSTMENT_NAMES.len(),
            "AdjustmentKind::ALL 与 ADJUSTMENT_NAMES 长度必须一致"
        );
        for kind in AdjustmentKind::ALL {
            if kind == AdjustmentKind::Vibrance {
                // **`vibrance` 是 `saturation` 的向后兼容别名**（from_name 里并入 Saturation ✓），
                // 往返本就不成立——这是既有口径，不是新增 6 种引入的 ✓。
                assert_eq!(
                    AdjustmentKind::from_name("vibrance"),
                    Some(AdjustmentKind::Saturation),
                    "vibrance 应仍是 saturation 的别名"
                );
                continue;
            }
            assert_eq!(
                AdjustmentKind::from_name(kind.as_str()),
                Some(kind),
                "{} 的 from_name/as_str 不往返",
                kind.as_str()
            );
            assert!(
                ADJUSTMENT_NAMES.contains(&kind.as_str()),
                "{} 不在 ADJUSTMENT_NAMES 里",
                kind.as_str()
            );
        }
    }
}
