//! 调整与滤镜内核（设计文档 4.2 / 6.1）。
//!
//! 调整（Adjustment）与滤镜（Filter）都是**对象**：作用于同图层中位于其下方（更低 `z_index`）
//! 的内容，符合设计文档「图层只做容器，调整对象化」的定义。
//!
//! 所有运算在**线性光、直通颜色**上执行，再回到预乘表示，保证与合成空间一致（D0 基线）。
//! **期 3 A 族例外**：brightness_contrast／color_balance／selective_color／
//! clarity／vignette 五个显示语义算子的**内部计算**在显示空间进行（扩展
//! 保符号 sRGB 编解码进出），接口仍是线性直通缓冲——见各函数文档。
//! 需要一个 `radius` 邻域的滤镜（模糊）要求调用方提供带 padding 的缓冲区；
//! [`crate::render`] 会按文档内最大滤镜半径扩展渲染区域后再裁剪。

use crate::buffer::Buffer;
use crate::color::{linear_to_srgb, srgb_decode_extended, srgb_encode_extended, srgb_to_linear};
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
    /// 校准原色（LR 校准面板式：红/绿/蓝原色色相 ± / 饱和度；矩阵见
    /// `calibrate_primaries_matrix`；显示空间，薄封装于 `channel_mixer` 之上）。
    CalibratePrimaries,
    /// 渐变映射（按明度在色带间插值）。
    GradientMap,
    /// 阴影/高光恢复。
    ShadowsHighlights,
    /// 可选颜色（PS 相对模式简化版）。
    SelectiveColor,
    /// LUT（.cube 三线性插值）。
    ColorLut,
    /// 红眼消除。
    RedEye,
    /// 降噪（自引导滤波 + √Linear + 亮色分离；照片修图）。
    Denoise,
    /// 镜头校正（径向畸变 + TCA + 暗角；照片修图）。
    Lens,
    /// 透视校正（四角拉正 + 可选镜头畸变合并；照片修图）。
    Perspective,
    /// 去雾（Presence 三件套之一；暗通道先验调整版，大气光在调整层内估计）。
    Dehaze,
    /// 局部对比（Presence 三件套之一；大半径引导滤波细节，负值柔化）。
    Clarity,
    /// 纹理（Presence 三件套之一；小尺度高频细节，负值平滑细纹理）。
    Texture,
}

impl AdjustmentKind {
    /// 由字符串解析。
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "brightness_contrast" | "brightness" => Some(Self::BrightnessContrast),
            "saturation" => Some(Self::Saturation),
            "invert" => Some(Self::Invert),
            "levels" => Some(Self::Levels),
            "exposure" => Some(Self::Exposure),
            "white_balance" => Some(Self::WhiteBalance),
            "curves" => Some(Self::Curves),
            "hsl" | "hue_saturation" => Some(Self::Hsl),
            "posterize" => Some(Self::Posterize),
            "color_balance" => Some(Self::ColorBalance),
            "split_toning" => Some(Self::SplitToning),
            // **规范名就是 `vibrance`**（与 `as_str`／`ADJUSTMENT_NAMES`／工具层校验三处一致 ✓）。
            // 此前 `"vibrance"` 被并入 `Saturation` ⇒ 真 Vibrance 只能用这个别名到达，
            // 而别名不在工具层白名单里 ⇒ 两条路都不可达 ✗（2026-10-21 修图实测）。
            // `vibrance_natural` 保留为别名：已用它落存的文档渲染不变 ✓。
            "vibrance" | "vibrance_natural" => Some(Self::Vibrance),
            "black_and_white" => Some(Self::BlackAndWhite),
            "channel_mixer" => Some(Self::ChannelMixer),
            "calibrate_primaries" => Some(Self::CalibratePrimaries),
            "gradient_map" => Some(Self::GradientMap),
            "shadows_highlights" => Some(Self::ShadowsHighlights),
            "selective_color" => Some(Self::SelectiveColor),
            "color_lut" => Some(Self::ColorLut),
            "red_eye" => Some(Self::RedEye),
            "denoise" => Some(Self::Denoise),
            "lens" => Some(Self::Lens),
            "perspective" => Some(Self::Perspective),
            "dehaze" => Some(Self::Dehaze),
            "clarity" => Some(Self::Clarity),
            "texture" => Some(Self::Texture),
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
            Self::CalibratePrimaries => "calibrate_primaries",
            Self::GradientMap => "gradient_map",
            Self::ShadowsHighlights => "shadows_highlights",
            Self::SelectiveColor => "selective_color",
            Self::ColorLut => "color_lut",
            Self::RedEye => "red_eye",
            Self::Denoise => "denoise",
            Self::Lens => "lens",
            Self::Perspective => "perspective",
            Self::Dehaze => "dehaze",
            Self::Clarity => "clarity",
            Self::Texture => "texture",
        }
    }

    /// 全部类型。
    pub const ALL: [AdjustmentKind; 26] = [
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
        Self::CalibratePrimaries,
        Self::GradientMap,
        Self::ShadowsHighlights,
        Self::SelectiveColor,
        Self::ColorLut,
        Self::RedEye,
        Self::Denoise,
        Self::Lens,
        Self::Perspective,
        Self::Dehaze,
        Self::Clarity,
        Self::Texture,
    ];

    /// 该调整在子缓冲（分带/区域）渲染时需要的窗口外扩（像素）。
    ///
    /// 逐像素调整为 0；**降噪**是邻域运算（引导滤波两趟方框 ⇒ 2×半径；色度模糊 1×半径）；
    /// **镜头/透视**是全图逆映射：输出像素的源位置可偏离自身，须按
    /// [`geo_displacement_bound`] 的位移上界 + 双三次采样足迹（±2）外扩，
    /// 否则源像素不在窗口里、采样被判越界写透明（曾表现为并行分带下的交替白带 ✓）。
    /// `canvas` 是整幅画面的文档尺寸（位移按它换算成像素 ✓）。
    pub fn padding(&self, params: &Value, canvas: (f64, f64)) -> u32 {
        let frame = (canvas.0 as f32, canvas.1 as f32);
        match self {
            Self::Lens => {
                let lens = LensParams {
                    k1: params.get("k1").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    k2: params.get("k2").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    tca_r: params.get("tca_r").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                    tca_b: params.get("tca_b").and_then(Value::as_f64).unwrap_or(1.0) as f32,
                    vignette_strength: params
                        .get("vignette_strength")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0) as f32,
                    auto_crop: params
                        .get("auto_crop")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                };
                let scale = geo_auto_crop_scale(None, &lens, frame);
                geo_displacement_bound(None, &lens, scale, frame) + 2
            }
            Self::Perspective => {
                let Some(quad) = quad_param(params) else {
                    return 0;
                };
                let Some(hom) = unit_square_to_quad(quad) else {
                    return 0;
                };
                let lens = LensParams {
                    k1: params.get("k1").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    k2: params.get("k2").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    auto_crop: params
                        .get("auto_crop")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    ..LensParams::default()
                };
                let scale = geo_auto_crop_scale(Some(quad), &lens, frame);
                geo_displacement_bound(Some(&hom), &lens, scale, frame) + 2
            }
            Self::Denoise => {
                let luminance = params
                    .get("luminance")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0) as f32;
                let detail = params
                    .get("detail")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0) as f32;
                let color = params
                    .get("color")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.5)
                    .clamp(0.0, 1.0) as f32;
                // 与 `denoise` 内部同式：Y 引导半径 (1 + detail·8)、两趟 ⇒ ×2；
                // 色度半径 (color·12)、一趟 ⇒ ×1。两者并行路径取大 ✓。
                let y_reach = if luminance > 0.0 {
                    2 * (1.0 + detail * 8.0).round() as u32
                } else {
                    0
                };
                let c_reach = if color > 0.0 {
                    (color * 12.0).round() as u32
                } else {
                    0
                };
                y_reach.max(c_reach)
            }
            Self::Dehaze => {
                // 透射率图方框模糊半径 4（一趟）⇒ 与滤镜版同口径取 8（含余量 ✓）。
                let amount = params.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                if amount == 0.0 {
                    0
                } else {
                    8
                }
            }
            Self::Clarity => {
                // 引导滤波两趟方框（mean 与 a/b 各一趟）⇒ 2×半径(16) ✓。
                let amount = params.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                if amount == 0.0 {
                    0
                } else {
                    32
                }
            }
            Self::Texture => {
                // 同上，半径 3 ⇒ 2×3 ✓。
                let amount = params.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                if amount == 0.0 {
                    0
                } else {
                    6
                }
            }
            _ => 0,
        }
    }
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
            // 浮雕读 ±1 邻域（`alpha_at(x±1, y±1)`）⇒ 必须申报 1，
            // 否则分带/区域渲染在块边界少看一列，与整幅结果不一致（接缝）。
            Self::Relief => 1,
            _ => 0,
        }
    }
}

/// 黑白收尾（D2-2b 共用）：灰度 → `tint` → `amount` 混合，与现行 `black_and_white`
/// 的收尾逐字同形；抽出来只为现行三权路与 8 档路共用，行为不变。
fn bw_output(input: [f32; 3], gray: f32, tint: Option<[f32; 3]>, amount: f32) -> [f32; 3] {
    let bw = match tint {
        Some(tint) => [
            (gray * tint[0]).clamp(0.0, 1.0),
            (gray * tint[1]).clamp(0.0, 1.0),
            (gray * tint[2]).clamp(0.0, 1.0),
        ],
        None => {
            let gray = gray.clamp(0.0, 1.0);
            [gray, gray, gray]
        }
    };
    // 混合方向：amount = 0 ⇒ 输入、amount = 1 ⇒ 黑白结果（反了则两端断言都红）。
    let mut out = input;
    for i in 0..3 {
        out[i] = input[i] * (1.0 - amount) + bw[i] * amount;
    }
    out
}

/// **8 色档黑白混合**（D2-2b）：在现行 RGB 三权灰度上，再按色相档权重缩放。
/// 档权重 `ranges` 与 D2-3 的 hsl 档名一致（`HSL_RANGE_NAMES`），每档一个乘子，
/// 域 [0, 2]、缺省 1.0。色相分类在显示编码空间（与 D2-3 同口径），乘子按像素
/// 饱和度缩放——灰像素（s = 0）对任意档权重都是不动点。
pub fn black_and_white_with_ranges(
    buffer: &mut Buffer,
    weights: [f32; 3],
    tint: Option<[f32; 3]>,
    amount: f32,
    ranges: &[f32; 8],
) {
    let amount = amount.clamp(0.0, 1.0);
    for_each_straight_color(buffer, |color| {
        let input = *color;
        let gray = weights[0] * color[0] + weights[1] * color[1] + weights[2] * color[2];
        let enc = [
            srgb_encode_extended(color[0]),
            srgb_encode_extended(color[1]),
            srgb_encode_extended(color[2]),
        ];
        let (h, s, _) = hsl_decompose(&enc);
        let band_weights = hsl_range_weights(h * 360.0);
        let mut band_mix = 0.0f32;
        for (weight, range) in band_weights.iter().zip(ranges.iter()) {
            band_mix += weight * range.clamp(0.0, 2.0);
        }
        // 乘子按饱和度缩放：灰像素 s = 0 ⇒ 乘子恒 1（不动点）；全饱和像素吃满档效应。
        let multiplier = 1.0 + s * (band_mix - 1.0);
        let gray = gray * multiplier;
        *color = bw_output(input, gray, tint, amount);
    });
}

/// **黑白转换**：`gray = Σ w·c`（权重由参数给、缺省 Rec709），可选 `tint` 着色
/// （输出 `gray × tint`），无 `tint` 时三通道同值；再按 `amount` 与输入混合
/// （`out = mix(input, bw_result, amount)`，期 4 D2-2）。
///
/// `amount` 钳 [0,1]：`amount = 0` ⇒ **逐位** = 输入（恒等）；
/// `amount = 1` ⇒ 逐位 = 现行实现输出（存量零影响：无 amount 键的老文档 = 1.0）。
///
/// 断言应写**关系**而不是单点 ✓：灰像素（r=g=b）且 `Σw = 1` 时原样返回；
/// 纯红配 `weights=[1,0,0]` ⇒ 输出 = 红通道值 ✓。
pub fn black_and_white(
    buffer: &mut Buffer,
    weights: [f32; 3],
    tint: Option<[f32; 3]>,
    amount: f32,
) {
    let amount = amount.clamp(0.0, 1.0);
    for_each_straight_color(buffer, |color| {
        let input = *color;
        let gray = weights[0] * color[0] + weights[1] * color[1] + weights[2] * color[2];
        *color = bw_output(input, gray, tint, amount);
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

// 阴影/高光色度差距软拐点（photo-bugfix magenta-fix，探针 P6；设计文档
// `yanshi-fix-research/photo-bugfix/magenta-fix-DESIGN.md` §2.5/§3）。
// R0 是线性直通阈值（ratio ≤ R0 时 gap = ratio，与现行亮度比回写逐位一致）；
// GMAX 是色度差距放大倍数的渐近上界（ratio → ∞ 时 gap → GMAX）。
// 定量依据：C¹ 连续（ratio = R0 处左右导数均为 1.0）+ 增益预算 + P6 实测
//（forest 品红 0.05%/0.01%，无噪色卡保持率 100%/78%/44%）。
const SHADOWS_HIGHLIGHTS_KNEE_R0: f32 = 1.5;
const SHADOWS_HIGHLIGHTS_KNEE_GMAX: f32 = 2.2;

/// 有理软拐点：把亮度比 `ratio` 映射为色度差距放大倍数 `gap`。
/// ratio ≤ R0 时 gap = ratio（逐位一致）；ratio > R0 时 gap 平滑趋近 GMAX，
/// 在 ratio = R0 处左右导数均为 1.0（C¹ 连续，无马赫带折角）。
fn soft_knee_gap(ratio: f32) -> f32 {
    if ratio <= SHADOWS_HIGHLIGHTS_KNEE_R0 {
        ratio
    } else {
        let excess = ratio - SHADOWS_HIGHLIGHTS_KNEE_R0;
        let span = SHADOWS_HIGHLIGHTS_KNEE_GMAX - SHADOWS_HIGHLIGHTS_KNEE_R0;
        SHADOWS_HIGHLIGHTS_KNEE_R0 + (excess * span) / (excess + span)
    }
}

/// 品红分量尺度（A2 后续 ③ 探针，首探 0.005）：`m = min(R−G, B−G)`（显示空间）
/// 达到 M0 时调制因子恰为 0.5；korean 变品红像素的 m p50 ≈ +0.008（a2-diagnose2
/// 实测），M0=0.005 使其 t ≈ 0.28，gap 增益大部收回；红卡 m<0 不受影响。
const SHADOWS_HIGHLIGHTS_MAGENTA_M0: f32 = 0.005;

/// 品红分量调制因子 `t(m) = 1/(1+(m/M0)²)`：m ≤ 0 时恒为 1（红/灰像素
/// 与现行逐位一致）；m > 0 时衰减，m = M0 处恰为 0.5、m ≫ M0 时趋近 0。
fn magenta_gap_factor(m: f32) -> f32 {
    if m <= 0.0 {
        1.0
    } else {
        let x = m / SHADOWS_HIGHLIGHTS_MAGENTA_M0;
        1.0 / (1.0 + x * x)
    }
}

/// 品红分量感知的 gap：提亮侧（gap > 1）才调制，`gap_eff = 1 + (gap−1)·t(m)`；
/// 压暗侧（gap ≤ 1）原样返回，与现行逐位一致。gap 仍是逐像素标量 ⇒
/// 亮度恒等于 target_luma、色相角严格不变。
fn magenta_aware_gap(gap: f32, magenta: f32) -> f32 {
    if gap <= 1.0 {
        gap
    } else {
        1.0 + (gap - 1.0) * magenta_gap_factor(magenta)
    }
}

/// **阴影/高光恢复**：`shadows`／`highlights` 各 -1..1，0 = 不动（恒等 ✓）。
///
/// 掩码按**显示空间（sRGB 编码）明度**：暗部权重 `(1−L)²`、高光权重 `L²` ⇒ 白色处暗部权重**恰好 0**、
/// 黑色处高光权重**恰好 0**（不是近似 ✓）⇒
/// 「shadows>0 只提暗部」「highlights<0 只压高光」两条性质精确成立 ✓。
/// 显示亮度朝目标端移动 `参数 × 权重 × 剩余距离 × 0.5` ⇒ 移动量不超过半程、永不越界 ✓
///（期 2 A2 起亮度比回写：亮度先动、三通道等比跟随，见下）。
///
/// **作用空间（缺陷修复）**：权重与幅度是按**显示参照值**标定的（参数语义即
/// 「0.15 只把黑抬到约 0.075」），而缓冲是**线性光**——旧实现直接在线性域套这套
/// 数学：sRGB 0.5 在线性域只有 0.214、0.75 只有 0.522 ⇒ 主体色调全挤在低线性区、
/// `(1−L)²` 对几乎全图 ≈ 1、抬升被成倍放大，编码回显示后全图堆成近常数灰
/// （实测：256 级渐变 `shadows=0.6` 输出 std 73.90→**31.43**、黑端 0→**149**；
/// `shadows=0.15` 黑端抬到 **77** 而非应有的 19）。已排除「重复施加」：旧输出与
/// 「线性域单次」模型逐点误差 0、与「两次」差达 21 ⇒ 每像素只施加了一次，病在空间。
/// 现先把直通色编码到显示空间算权重与 `toward()`、再解码回线性写回 ⇒
/// 渲染结果与文档数学**单次**一致；管线其余部分（线性合成）不动。
/// **施加方式（photo-bugfix 期 2 A2：亮度比回写，原地替换逐通道 toward）**：
/// 旧实现把 toward() 逐通道加在显示值上：三通道被拉向同一端点、通道差
/// 被 `(1 − amount·weight·0.5)` 压缩，相对饱和双重下跌（阴影提亮在数学
/// 上就是向灰拉；A/B 对照：shadows 每 +0.2，中间调饱和 −12~−14pp）。
/// 新口径先对显示亮度本身用同一 toward() 公式算目标亮度 `luma'`，再以
/// 亮度比 `r = luma'/luma` 等比缩放三通道显示值——通道比不变，相对饱和
/// 与色相守恒，只有明度按原曲线提升（通则 2，`brightness_contrast`
/// 同款机制）。参数、权重、剂量曲线全不动；灰像素上与旧实现逐位等价
///（回归测试数值不变）。`luma = 0` 的纯黑像素无亮度可缩放 ⇒ 回退逐通道
/// toward（黑位抬升行为不变）。编解码仍用钳制版 `linear_to_srgb` /
/// `srgb_to_linear`（与旧实现一致，HDR 高光行为不动；扩展传递的评估
/// 留后，不随本项改）。
///
/// **色度差距软拐点（photo-bugfix magenta-fix，探针 P6；设计文档 §2.5/§3）**：
/// 亮度照 `target_luma` 提满，通道与亮度的差距按有理软拐点钳制放大：
/// `gap = ratio`（ratio ≤ 1.5 时逐位一致）／
/// `gap = 1.5 + (r−1.5)·0.7/((r−1.5)+0.7)`（ratio > 1.5 时平滑趋近 2.2）。
/// 暗部色度噪声不再被亮度比 2.5~4.4× 放大越过品红阈值，而提亮量、色相角、
/// 中浅暗部（ratio ≤ 1.5）逐位不动。
/// **品红分量感知（A2 后续 ③ 探针）**：提亮侧的 gap 再按 `t(m)` 收敛，
/// `m = min(R−G, B−G)`、`t = 1/(1+(m/0.005)²)`、`gap_eff = 1+(gap−1)·t`——
/// korean 变品红的中调像素（m p50 ≈ +0.008）增益大部收回，红卡/灰像素
///（m ≤ 0）逐位不动。gap 仍是逐像素标量，亮度与色相角结论不变。
/// 软拐点在 ratio = 1.5 处左右导数均为 1.0
///（C¹ 连续，无马赫带折角）；顺带修复 `luma→0+` 与 `luma=0` 的分支跳变
///（`lim_{luma→0+} display[i] = target_luma`，与纯黑回退分支连续）。
/// `luma = 0` 的纯黑回退分支不动；highlights 侧 ratio < 1 时 gap = ratio，逐位不变。
pub fn shadows_highlights(buffer: &mut Buffer, shadows: f32, highlights: f32) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let shadows = shadows.clamp(-1.0, 1.0);
    let highlights = highlights.clamp(-1.0, 1.0);
    if shadows == 0.0 && highlights == 0.0 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        let mut display = [
            linear_to_srgb(color[0]),
            linear_to_srgb(color[1]),
            linear_to_srgb(color[2]),
        ];
        let luma =
            (LUMA[0] * display[0] + LUMA[1] * display[1] + LUMA[2] * display[2]).clamp(0.0, 1.0);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let target_luma = (luma
            + toward(shadows, shadow_weight, luma)
            + toward(highlights, highlight_weight, luma))
        .clamp(0.0, 1.0);
        if luma > 0.0 {
            let ratio = target_luma / luma;
            // 色度差距放大倍数按有理软拐点钳制（magenta-fix，探针 P6）：
            // ratio ≤ R0 时 gap = ratio（与现行亮度比回写逐位一致）；
            // ratio > R0 时 gap 平滑趋近 GMAX。亮度恒等于 target_luma
            //（提亮量不动），色相角严格不变。
            // 品红分量感知调制（A2 后续 ③ 探针）：m = min(R−G, B−G)（显示空间），
            // m>0 的品红向像素收 gap 增益，m≤0 的红/灰像素逐位不动。
            let magenta = (display[0] - display[1]).min(display[2] - display[1]);
            let gap = magenta_aware_gap(soft_knee_gap(ratio), magenta);
            for value in display.iter_mut() {
                *value = (target_luma + (*value - luma) * gap).clamp(0.0, 1.0);
            }
        } else {
            // 纯黑像素无亮度可缩放：回退逐通道 toward，黑位抬升行为与现行一致。
            for value in display.iter_mut() {
                let current = *value;
                let mut next = current + toward(shadows, shadow_weight, current);
                next += toward(highlights, highlight_weight, current);
                *value = next.clamp(0.0, 1.0);
            }
        }
        *color = [
            srgb_to_linear(display[0]),
            srgb_to_linear(display[1]),
            srgb_to_linear(display[2]),
        ];
    });
}

/// 朝向移动量：正参数朝 1、负参数朝 0，幅度 = `参数 × 权重 × 到该端的距离 × 0.5`
/// （参数自带符号 ⇒ 负参数的结果自然为负 ✓）。
/// 期 2 A2 起对显示亮度调用一次（`luma = 0` 时逐通道回退），不再逐通道调用。
fn toward(amount: f32, weight: f32, value: f32) -> f32 {
    let room = if amount >= 0.0 { 1.0 - value } else { value };
    amount * weight * room * 0.5
}

/// **可选颜色**的 9 个色类名（参数 `adjust` 的键；工具层校验与渲染层共用 ⇒ 只此一处 ✓）。
pub const SELECTIVE_COLOR_RANGES: [&str; 9] = [
    "reds", "yellows", "greens", "cyans", "blues", "magentas", "whites", "neutrals", "blacks",
];

/// 线性 sRGB → 线性 ProPhoto（ROMM）复合矩阵：sRGB→XYZ(D65) ∘ Bradford
/// D65→D50 ∘ XYZ→ProPhoto。白点自检：三行和 = 1±0.001（D50 白在
/// ProPhoto 里恰为 (1,1,1)）；往返一致性由测试钉死。A3 的 Melissa 口径用。
const SRGB_TO_PROPHOTO: [[f32; 3]; 3] = [
    [0.529_201, 0.330_110, 0.140_653],
    [0.098_394, 0.873_509, 0.028_148],
    [0.016_861, 0.117_561, 0.864_617],
];

fn mat3_apply(m: &[[f32; 3]; 3], v: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

/// 3×3 求逆（伴随矩阵／行列式）。只对上面的常量矩阵用（可逆性由测试
/// 钉死）；行列式近零返回 None。
fn mat3_inverse(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let (a, b, c) = (m[0][0], m[0][1], m[0][2]);
    let (d, e, f) = (m[1][0], m[1][1], m[1][2]);
    let (g, h, i) = (m[2][0], m[2][1], m[2][2]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if det.abs() < 1e-12 {
        return None;
    }
    let s = 1.0 / det;
    Some([
        [
            (e * i - f * h) * s,
            (c * h - b * i) * s,
            (b * f - c * e) * s,
        ],
        [
            (f * g - d * i) * s,
            (a * i - c * g) * s,
            (c * d - a * f) * s,
        ],
        [
            (d * h - e * g) * s,
            (b * g - a * h) * s,
            (a * e - b * d) * s,
        ],
    ])
}

/// 线性 sRGB → **Melissa 显示值**：经线性 ProPhoto 后以 sRGB TRC 编码
/// （LR 的 Melissa 口径 = ProPhoto 原色 + sRGB 曲线；专家裁定 LR TRC =
/// sRGB TRC）。扩展保符号编码，不钳 [0, 1]。
fn melissa_encode(color: [f32; 3]) -> [f32; 3] {
    let pp = mat3_apply(&SRGB_TO_PROPHOTO, color);
    [
        srgb_encode_extended(pp[0]),
        srgb_encode_extended(pp[1]),
        srgb_encode_extended(pp[2]),
    ]
}

/// [`melissa_encode`] 的逆（逆矩阵由调用方按整图算一次传入）。
fn melissa_decode(d: [f32; 3], inverse: &[[f32; 3]; 3]) -> [f32; 3] {
    let pp = [
        srgb_decode_extended(d[0]),
        srgb_decode_extended(d[1]),
        srgb_decode_extended(d[2]),
    ];
    mat3_apply(inverse, pp)
}

/// 白档保底 ε（O4）：`selective_weights` 里 whites 的 `(1 − q)` 因子
/// 下限。现行门控在 span ≥ 0.08 时把 whites 权重精确截到零，亮饱和
/// 像素吃不到 whites 修正（forest 唇口探针 ③a 证实）；ε=0.20 经
/// §6.1 的交换曲线标定（唇口顽固 15.5%、4 预设波及面 ≤0.52%）。
/// neutrals/blacks 不保底：那两档尚无观测失效，留待后议。
const SELECTIVE_COLOR_WHITES_FLOOR: f32 = 0.20;

/// A3 的九档权重（**PS 差值权重法**的连续化，见 [`selective_color`]）：
/// 与 [`SELECTIVE_COLOR_RANGES`] 同序、和为 1。
fn selective_weights(d: [f32; 3]) -> [f32; 9] {
    let max = d[0].max(d[1]).max(d[2]);
    let min = d[0].min(d[1]).min(d[2]);
    let span = max - min;
    // 色相六档：PS 标度——(max−med) 给 max 通道的原色档、(med−min) 给
    // min 通道对偶的复色档——**按跨度归一化**：权重只依赖色相构成、
    // 不随饱和缩放（FFmpeg selectivecolor 逆向分析里的原始标度是
    // max−med／med−min 本身；归一化是设计探针「同色不同亮 Δ 相等」
    // 的硬要求）。通道并列 max/min 时对应权重恒为 0，不受下标选择影响。
    let mut hue = [0.0f32; 9];
    if span > 1e-6 {
        let med = d[0] + d[1] + d[2] - max - min;
        let primary = if d[0] == max {
            0
        } else if d[1] == max {
            2
        } else {
            4
        };
        let dual = if d[2] == min {
            1
        } else if d[0] == min {
            3
        } else {
            5
        };
        hue[primary] = (max - med) / span;
        hue[dual] = (med - min) / span;
    }
    // 明度三档：按设计的 0.25/0.75 锚点回归本意——blacks 在 L ≤ 0.25
    // 满权、0.375 归零；whites 在 L ≥ 0.75 满权、0.625 起升；neutrals
    // 取余（三档单位分解）。纯 PS 标度在 L = 0.4 给 blacks 0.2 ≠ 0，
    // 与设计探针「0.4 近灰入 neutrals」冲突 ⇒ 以设计探针为准。
    let lightness = (max + min) / 2.0;
    let blacks = ((0.375 - lightness) / 0.125).clamp(0.0, 1.0);
    let whites = ((lightness - 0.625) / 0.125).clamp(0.0, 1.0);
    let neutrals = 1.0 - blacks - whites;
    // 色度门控交叉淡化：近灰（span → 0）由明度组主导、饱和像素由色相
    // 组主导；0.08 沿用旧实现的低饱和界，改在显示（Melissa）口径上判。
    //
    // 白档保底（O4）：现行 `(1 − q)` 在 span ≥ 0.08 时把 whites 权重
    // 精确截到零，亮饱和像素吃不到 whites 修正（forest 唇口探针 ③a
    // 证实：顽固像素 w[6] 中位 0，65.1% 恰好为零）。只给 whites 的
    // `(1 − q)` 因子一个保底 ε——亮饱和像素恢复 ≥ ε·whites 的去色
    // 杠杆，多出来的权重从色相档扣（九档和仍为 1）。neutrals/blacks
    // 不动：那两档尚无观测失效，留待后议（本次只修已观测到的 whites
    // 失效）。`hue_share` 的 `.max(0.0)` 是 f32 舍入 guard：三档和
    // 在数学上 ≤ 1，浮点下可能差 1ulp，钳到 0 防微负权重。
    let q = (span / 0.08).clamp(0.0, 1.0);
    let w6 = whites * (1.0 - q).max(SELECTIVE_COLOR_WHITES_FLOOR);
    let w7 = neutrals * (1.0 - q);
    let w8 = blacks * (1.0 - q);
    let hue_share = (1.0 - w6 - w7 - w8).max(0.0);
    let mut w = [0.0f32; 9];
    for (i, h) in hue.iter().enumerate() {
        w[i] = h * hue_share;
    }
    w[6] = w6;
    w[7] = w7;
    w[8] = w8;
    w
}

/// 在显示值上施加九档 ranges：权重混合得有效偏移 → CMYK 剩余量模型。
/// 有效偏移为零时**逐位**返回原值（恒等不靠浮点往返碰运气）。
fn selective_apply_display(d: [f32; 3], ranges: &[[f32; 4]; 9]) -> [f32; 3] {
    let w = selective_weights(d);
    let mut offset = [0.0f32; 4];
    for (range, weight) in ranges.iter().zip(w.iter()) {
        if *weight != 0.0 {
            for (slot, value) in offset.iter_mut().zip(range.iter()) {
                *slot += value * weight;
            }
        }
    }
    if offset == [0.0; 4] {
        return d;
    }
    let mut cmyk = rgb_to_cmyk(d);
    for (slot, delta) in cmyk.iter_mut().zip(offset.iter()) {
        if *delta != 0.0 {
            let value = *slot;
            let room = if *delta >= 0.0 { 1.0 - value } else { value };
            *slot = (value + delta * room).clamp(0.0, 1.0);
        }
    }
    cmyk_to_rgb_display(cmyk)
}

/// **可选颜色**（**期 3 A3：全面显示空间重写，原地替换旧线性硬分类版**）。
///
/// 旧实现的问题（设计 §2 A3）：分类（唯一一档）算在**线性**值上、色相
/// 60° 硬扇区、偏移在**线性** CMYK 上施加——路由与 LR（显示数据）系统
/// 性错位：33–36° 暖黄在线性分类下被吞进 reds（LR 口径下 yellows 为主
/// 权重）、同参数强度与 LR 参考系统性偏差、扇区边界处偏移突变。
///
/// 新口径（PS 差值权重法 + Melissa 空间，专家 Q1 定版）：
/// 1. 像素线性 sRGB → **Melissa 显示值**（线性 ProPhoto + sRGB TRC，
///    扩展保符号不钳）——为对齐 LR 来源数据而设：LR 的可选颜色工作在
///    Melissa 空间；省掉这步矩阵往返，同参数 Δ 会系统性偏大约
///    35–50%（参照带断言钉死）；
/// 2. 权重 = [`selective_weights`]：色相六档用排序通道差值权重
///    （**禁止色相角硬切**）、明度三档按 0.25/0.75 锚点、两组以色度
///    门控交叉淡化，九档同时加权求和（PS 的多档可重叠语义）；
/// 3. 有效偏移在显示值上经 CMYK 剩余量（相对）模型施加：
///    `v' = v + δ·(δ ≥ 0 ? 1 − v : v)`——纯红的 c 恒为 0 时 reds 的
///    cyan 滑杆仍生效（旧注记的静默失效防线保留），被改分量钳 [0, 1]；
/// 4. 逆矩阵回线性 sRGB。全零 ranges 或像素有效偏移为零 ⇒ 逐位恒等。
pub fn selective_color(buffer: &mut Buffer, ranges: &[[f32; 4]; 9]) {
    if ranges.iter().all(|range| *range == [0.0; 4]) {
        return;
    }
    let inverse = mat3_inverse(&SRGB_TO_PROPHOTO)
        .expect("SRGB_TO_PROPHOTO 是常量矩阵且可逆（由 melissa 往返测试钉死）");
    for_each_straight_color(buffer, |color| {
        let d = melissa_encode(*color);
        let out = selective_apply_display(d, ranges);
        if out != d {
            *color = melissa_decode(out, &inverse);
        }
    });
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

/// CMYK → RGB（显示值版）：[`rgb_to_cmyk`] 的代数逆（`(1 − c)(1 − k)`）。
/// **不钳 [0, 1]**——A3 在 Melissa 显示值上用它，HDR 显示值（> 1）必须
/// 原样往返；代数上对 max > 0 的任意输入逐位逆（未改分量保真）。
fn cmyk_to_rgb_display(cmyk: [f32; 4]) -> [f32; 3] {
    let keep = 1.0 - cmyk[3];
    [
        (1.0 - cmyk[0]) * keep,
        (1.0 - cmyk[1]) * keep,
        (1.0 - cmyk[2]) * keep,
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

/// 亮度/对比度（**期 3 A1：显示空间枢轴口径，原地替换旧线性口径**）。
///
/// 旧实现是 `out = (c·contrast + brightness).clamp(0, 1)` 直接作用于线性
/// 直通值：无枢轴、且把一切钳进 [0, 1]——HDR 高光在第一个 bc 处即被压
/// 平。用户与预设数据给的都是 LR 显示口径参数，旧实现系统性偏离
/// （设计文档 §2 A1；注：设计文档把旧式写成带 0.5 枢轴的形式，与代码
/// 实测不符——实测无枢轴，见 phase3-notes 备忘）。
///
/// 新口径（Adobe 显示语义，与设计 §2 A1 定版一致）：
/// 1. 每通道线性 → **扩展保符号 sRGB 编码**（[`srgb_encode_extended`]，
///    编码前禁止钳 `[0, 1]`，HDR 高光与负值不被第一个显示算子销毁）；
/// 2. 亮度平移 `d' = d + brightness·0.5`（规范刻度：brightness = ±0.5 ⇔
///    中灰显示 ±0.25；此 ±0.25 探针即刻度定义）；
/// 3. 对比绕**显示中灰枢轴 0.5** 缩放 `d'' = (d' − 0.5)·contrast + 0.5`；
/// 4. **亮度比回写（hue-twist 防护）**：`out_lin = in_lin · (L'' / L)`，
///    其中 `L = Σ wᵢ·cᵢ` 是入像素的**线性**亮度、`L'' = Σ wᵢ·dec(d''ᵢ)`
///    是影调映射后像素的线性亮度（w = 0.2126/0.7152/0.0722）。比值在
///    同一量纲（线性）里取——灰像素上它与逐通道显示映射**恰好等价**
///    （刻度探针因此精确成立），彩色像素上只缩放亮度、色相由原像素
///    决定。`L ≤ 0`（纯黑/负亮度像素）时无亮度可缩放 ⇒ 回退逐通道
///    解码（黑位抬升 brightness 仍生效）；`L'' ≤ 0` 时比值钳 0（压黑）。
///
/// 参数契约不变：brightness ∈ [−1, 1]，contrast ∈ [0, 8]（越界在工具层
/// 拒）。neutral（brightness = 0 且 contrast = 1）逐像素恒等（含 HDR 与
/// 负值）。存量文档回放自本期起按新口径（甲案，发布说明明示）。
pub fn brightness_contrast(buffer: &mut Buffer, brightness: f32, contrast: f32) {
    if brightness == 0.0 && contrast == 1.0 {
        return;
    }
    for_each_straight_color(buffer, |c| {
        let d = [
            srgb_encode_extended(c[0]),
            srgb_encode_extended(c[1]),
            srgb_encode_extended(c[2]),
        ];
        let lum_in = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let shifted = [
            (d[0] + brightness * 0.5 - 0.5) * contrast + 0.5,
            (d[1] + brightness * 0.5 - 0.5) * contrast + 0.5,
            (d[2] + brightness * 0.5 - 0.5) * contrast + 0.5,
        ];
        let lum_out = 0.2126 * srgb_decode_extended(shifted[0])
            + 0.7152 * srgb_decode_extended(shifted[1])
            + 0.0722 * srgb_decode_extended(shifted[2]);
        if lum_in > 0.0 {
            let ratio = (lum_out / lum_in).max(0.0);
            *c = [c[0] * ratio, c[1] * ratio, c[2] * ratio];
        } else {
            *c = [
                srgb_decode_extended(shifted[0]),
                srgb_decode_extended(shifted[1]),
                srgb_decode_extended(shifted[2]),
            ];
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
///
/// **作用空间（photo-bugfix 期 2 A1，方案 K：显示口径，原地替换旧线性口径）**：
/// 旧实现直接把 `black`/`white` 当作线性光缓冲上的位置，而著录直觉是显示空间
/// 黑点——显示意图 0.06 实效显示黑点 0.27（错位约 12×）。新口径仿
/// `brightness_contrast`：每通道扩展 sRGB 编码到显示空间做
/// `((d−black)/(white−black))^gamma`、再扩展解码回写；参数键与取值域不变。
/// `black`/`white`/`gamma` 现均为显示空间值：著录直接写显示值，不再经 EOTF
/// 换线性（7b62936 的 6 套线性值已随本期翻回显示尺度；retro_film 0.02 本
/// 就是显示意图，不动）。单通道模式（`r`/`g`/`b`）只编解码该通道。
/// HDR：编码前不钳 `[0, 1]`（通则 1），显示值高于白点的归一化仍钳 1 再解码，
/// 与旧线性版「高于白点 ⇒ 白点」的边界行为一致。
pub fn levels_channel(buffer: &mut Buffer, black: f32, white: f32, gamma: f32, channel: &str) {
    let black = black.clamp(0.0, 0.999);
    let white = white.clamp(black + 1e-3, 1.0);
    let inverse_gamma = 1.0 / gamma.max(1e-3);
    let map = |value: f32| {
        let display = srgb_encode_extended(value);
        let normalized = ((display - black) / (white - black)).clamp(0.0, 1.0);
        srgb_decode_extended(libm::powf(normalized, inverse_gamma))
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

/// 色彩平衡（**期 3 A2：显示空间施加 + Y' 分档 + amount，原地替换**）。
///
/// 旧实现的问题（设计 §2 A2）：分档权重算在**线性**亮度上（与 LR 的感知
/// 明度分档错位）、偏移以 ±1.0 全幅直接加到线性值再钳 [0, 1]（HDR 被
/// 压平）、且**没有 amount**——预设要 30% 强度只能把三档偏移逐个手乘。
/// （注：设计文档把旧权重写成三角形、旧偏移写成 ·0.25，与代码实测不符
/// ——实测权重为二次式、偏移无缩放；见 phase3-notes 备忘。权重**形状**
/// 本期保留实测的二次式，只换它读取的量。）
/// 新口径：
/// 1. 像素线性 → 扩展保符号 sRGB 编码得显示值 `d`（不钳 [0, 1]）；
/// 2. 分档权重按**显示明度 Y'**（sRGB 亮度系数在显示值上的加权和，
///    钳 [0, 1] 只用于权重）取原二次形状：`shadows = (1−Y')²`、
///    `highlights = Y'²`、`midtones = 1 − shadows − highlights`；
/// 3. 混合偏移按设计刻度 ±0.25 乘 `amount` 后**加在显示值上**，再扩展
///    解码回线性。HDR 像素（显示值 > 1）全程不钳、偏移照常施加。
///
/// `amount` 由分发层缺省 1.0；三档全零或 amount ≤ 0 时逐像素恒等。
/// 负 amount 在工具层拒（[0, 1]）。
pub fn color_balance(
    buffer: &mut Buffer,
    shadows: [f32; 3],
    midtones: [f32; 3],
    highlights: [f32; 3],
    amount: f32,
) {
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    if amount <= 0.0 || (shadows == [0.0; 3] && midtones == [0.0; 3] && highlights == [0.0; 3]) {
        return;
    }
    let clamp_component = |value: f32| value.clamp(-1.0, 1.0);
    let shadows = shadows.map(clamp_component);
    let midtones = midtones.map(clamp_component);
    let highlights = highlights.map(clamp_component);
    for_each_straight_color(buffer, |color| {
        let d = [
            srgb_encode_extended(color[0]),
            srgb_encode_extended(color[1]),
            srgb_encode_extended(color[2]),
        ];
        let luma = (LUMA[0] * d[0] + LUMA[1] * d[1] + LUMA[2] * d[2]).clamp(0.0, 1.0);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let midtone_weight = (1.0 - shadow_weight - highlight_weight).max(0.0);
        for channel in 0..3 {
            let delta = (shadows[channel] * shadow_weight
                + midtones[channel] * midtone_weight
                + highlights[channel] * highlight_weight)
                * 0.25
                * amount;
            color[channel] = srgb_decode_extended(d[channel] + delta);
        }
    });
}

/// 分离色调：阴影与高光分别着色，`balance` 移动分界点，`amount` 控制总强度。
///
/// 期 5 Q1-c 口径（设计 §2 第 4 条）：权重取自 **sRGB 亮度 Y'**（显示空间，
/// 通道先扩展编码再加权），着色在显示空间做，解码回线性后按**线性亮度比回写**
/// （恢复原亮度，只留色度变化；黑像素亮度为 0 ⇒ 着色后仍为黑）。
/// 权重仍用 smoothstep 在 `mid ± 0.25` 之间过渡（`mid = 0.5 + balance × 0.25`），
/// 因此权重单调、确定，且 `balance` 的语义直观（正值把分界推向亮部）。
/// 输出域钳 [0,1] 与旧内核一致（显示指涉域；HDR 负值与旧内核同处理）。
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
        let orig = *color;
        let orig_luma = LUMA[0] * orig[0] + LUMA[1] * orig[1] + LUMA[2] * orig[2];
        let d = [
            srgb_encode_extended(orig[0]),
            srgb_encode_extended(orig[1]),
            srgb_encode_extended(orig[2]),
        ];
        let y = LUMA[0] * d[0] + LUMA[1] * d[1] + LUMA[2] * d[2];
        let t = ((y - (mid - 0.25)) / 0.5).clamp(0.0, 1.0);
        let highlight_weight = t * t * (3.0 - 2.0 * t);
        let shadow_weight = 1.0 - highlight_weight;
        let mut lin = [0.0; 3];
        for channel in 0..3 {
            let tint = shadows[channel] * shadow_weight + highlights[channel] * highlight_weight;
            lin[channel] = srgb_decode_extended(d[channel] + amount * tint);
        }
        let new_luma = LUMA[0] * lin[0] + LUMA[1] * lin[1] + LUMA[2] * lin[2];
        let scale = if new_luma > 1e-6 {
            orig_luma / new_luma
        } else {
            1.0
        };
        for channel in 0..3 {
            color[channel] = (lin[channel] * scale).clamp(0.0, 1.0);
        }
    });
}

/// **红眼消除** ✓：只动「红眼型」像素 ⇒ 把 R 压向 max(G,B) ✓。
///
/// **判别**：红度 `r / (g + b + ε)` ✓ —— 闪光灯瞳孔的 R 相对 G+B 可达 5 倍以上 ✓；
/// 皮肤 ~0.75、唇色 ~1.7、砖红 ~1.5 ⇒ 阈值默认 2.0 时**普通红色物体一个都不动** ✓。
/// 权重从阈值起线性升到 2×阈值满权（软过渡，避免边缘光晕 ✓）。
/// `strength` 0..1 缩放压制量（0 = 恒等 ✓）；`threshold` 红度比阈值。
pub fn red_eye(buffer: &mut Buffer, strength: f32, threshold: f32) {
    let strength = strength.clamp(0.0, 1.0);
    let threshold = threshold.clamp(0.5, 10.0);
    if strength <= 0.0 {
        return;
    }
    for_each_straight_color(buffer, |color| {
        let (r, g, b) = (color[0], color[1], color[2]);
        let redness = r / (g + b + 1e-4);
        let weight = ((redness - threshold) / threshold).clamp(0.0, 1.0);
        if weight > 0.0 {
            color[0] = r + strength * weight * (g.max(b) - r);
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

/// **逐通道 HSL 的 8 个色档名**（固定白名单；工具层校验与渲染层共用 ⇒ 只此一处）。
pub const HSL_RANGE_NAMES: [&str; 8] = [
    "reds", "oranges", "yellows", "greens", "aquas", "blues", "purples", "magentas",
];

/// 8 档中心角（度，LR 式）：红 0、橙 30、黄 60、绿 120、青 180、蓝 240、紫 280、品红 315。
const HSL_RANGE_CENTERS: [f32; 8] = [0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 280.0, 315.0];

/// 单档偏移（期 4 D2-3）：`hue` 为度（加性）；`saturation` / `lightness` 为
/// LR 式百分比：饱和度以因子 `(1 + saturation/100)` 乘（灰像素 s = 0 时仍为 0，
/// 不会凭空染色——加性饱和度会把无色像素染上色，那是缺陷不是特性）；
/// 明度以 `lightness/100` 加性偏移。缺省三键均为 0（无效果）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HslRangeOffset {
    /// 色相偏移（度，加性）。
    pub hue: f32,
    /// 饱和度偏移（百分比，以因子 `(1 + saturation/100)` 乘）。
    pub saturation: f32,
    /// 明度偏移（百分比，以 `lightness/100` 加性偏移）。
    pub lightness: f32,
}

/// 色相角的三角窗权重（期 4 D2-3）：权重在档中心为 1、向两侧线性衰减、
/// **在相邻中心处恰好到 0**（中点两档各半——设计自家的边界测试定版；
/// 若按「半宽 = 相邻中心距的一半」字面算，中点两档权重同为 0、归一化除零，
/// 与该测试冲突 ⇒ 以测试为准）。权重和恒为 1（仍做归一化，防浮点漂移）。
fn hsl_range_weights(hue_degrees: f32) -> [f32; 8] {
    let h = hue_degrees.rem_euclid(360.0);
    let mut weights = [0.0f32; 8];
    for (i, center) in HSL_RANGE_CENTERS.iter().enumerate() {
        // 到档中心的有向圆周差（−180, 180]。
        let diff = (h - center + 540.0).rem_euclid(360.0) - 180.0;
        let side_dist = if diff >= 0.0 {
            let next = HSL_RANGE_CENTERS[(i + 1) % 8];
            (next - center).rem_euclid(360.0)
        } else {
            let prev = HSL_RANGE_CENTERS[(i + 7) % 8];
            (center - prev).rem_euclid(360.0)
        };
        weights[i] = if side_dist > 0.0 {
            (1.0 - diff.abs() / side_dist).max(0.0)
        } else {
            0.0
        };
    }
    let sum: f32 = weights.iter().sum();
    if sum > 0.0 {
        for weight in weights.iter_mut() {
            *weight /= sum;
        }
    }
    weights
}

/// **逐通道 HSL**（期 4 D2-3）：在**显示编码空间**分类与运算（照
/// `shadows_highlights` 的「编码→算→解码」结构，编码用期 3 的扩展传递）：
/// 编码后取色相角、按三角窗加权得档位偏移，与全局偏移合成后用既有
/// `hsl_decompose` / `hsl_compose` 施加，再解码回线性。
///
/// 全局参数语义与标量路同形（色相度数加性、饱和度乘子、明度加性），
/// 只是同样移到编码空间施加；档位饱和度为百分比因子、档位明度为百分比偏移。
/// 全部偏移为零时像素**逐位不动**（短路——守「绿块逐位不变」与无用功）。
pub fn hsl_with_ranges(
    buffer: &mut Buffer,
    hue_degrees: f32,
    saturation: f32,
    lightness: f32,
    ranges: &[HslRangeOffset; 8],
) {
    let hue_shift = hue_degrees.clamp(-360.0, 360.0) / 360.0;
    let saturation = saturation.clamp(0.0, 8.0);
    let lightness = lightness.clamp(-1.0, 1.0);
    for_each_straight_color(buffer, |color| {
        let enc = [
            srgb_encode_extended(color[0]),
            srgb_encode_extended(color[1]),
            srgb_encode_extended(color[2]),
        ];
        let (h, s, l) = hsl_decompose(&enc);
        let weights = hsl_range_weights(h * 360.0);
        let mut range_hue = 0.0f32;
        let mut range_sat_factor = 1.0f32;
        let mut range_light = 0.0f32;
        for (weight, offset) in weights.iter().zip(ranges.iter()) {
            range_hue += weight * offset.hue.clamp(-180.0, 180.0);
            range_sat_factor += weight * (offset.saturation.clamp(-100.0, 100.0) / 100.0);
            range_light += weight * (offset.lightness.clamp(-100.0, 100.0) / 100.0);
        }
        // 全零偏移 ⇒ 逐位不动（不经过 decompose/compose 的浮点往返）。
        if hue_shift == 0.0
            && saturation == 1.0
            && lightness == 0.0
            && range_hue == 0.0
            && range_sat_factor == 1.0
            && range_light == 0.0
        {
            return;
        }
        let h = (h + hue_shift + range_hue / 360.0).rem_euclid(1.0);
        let s = (s * saturation * range_sat_factor).clamp(0.0, 1.0);
        let l = (l + lightness + range_light).clamp(0.0, 1.0);
        let composed = hsl_compose(h, s, l);
        *color = [
            srgb_decode_extended(composed[0]),
            srgb_decode_extended(composed[1]),
            srgb_decode_extended(composed[2]),
        ];
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

/// 大气光色度阻尼（**期 3 A4-3 修正一**，专家 Q5）：各通道对亮度的
/// 比值钳 [0.95, 1.05]。估计出的大气光带色度时，逐通道恢复会把色度
/// 误差按 1/t 放大成前景色偏（中性灰反例可达 R/B 比 7×）；阻尼把
/// 估计色度限制在 ±5% 内。两个估计器与恢复入口共用同一阻尼。
fn damp_air_chromaticity(air: [f32; 3]) -> [f32; 3] {
    let luma = (0.2126 * air[0] + 0.7152 * air[1] + 0.0722 * air[2]).max(1e-3);
    air.map(|value| luma * (value / luma).clamp(0.95, 1.05))
}

/// 去雾（暗通道先验）：`air` 为**调用方给定**的大气光，`omega` 控制去雾强度，
/// `floor` 是透射率下限（避免除零与过度放大噪声）。
///
/// 大气光必须是参数而不是在滤镜内部估计：否则同一像素在分块渲染与整幅渲染下会得到
/// 不同的全局统计量，`分块 == 整幅` 立刻失效（这类问题此前已踩过）。只读工具
/// `estimate_dehaze` 负责扫描文档给出建议值，再由调用方写进原子参数——这与
/// Phase 4b 的「先预览/估计、再作为 patch 应用」是同一套思路。
///
/// **期 3 A4-3：保留线性 DCP + 专家四修正**（设计 §2 A4-3 / 专家 Q5）：
/// ① 大气光进恢复前过 [`damp_air_chromaticity`]（色度比钳 [0.95, 1.05]）；
/// ② 透射率先做 γ_t = 0.5 重映射（`t ← √t`，抬升低透射区、防暗部被
/// 1/t 过度放大），再过动态下限 `max(floor, 0.25 − 0.05·omega)`
/// （∈ [0.20, 0.25]；调用方的 floor 仍是下限语义、只许更高）；
/// ③ 恢复走亮度/通道混合：逐通道恢复为主（0.65），混入 β = 0.35 的
/// 「中性大气光（= air 亮度）亮度恢复」——通道色度误差被亮度锚稀释，
/// 但主体仍是逐通道恢复（反过来以亮度为主会把通道对比一并冲淡，
/// 既有对比度回归实测不允许，见 phase3-notes）；
/// ④ 前景中性再锚定：输入近中性（色度 ≤ 0.03）且 t ≥ 0.5（前景）的
/// 像素，恢复后逐通道增益拉回自身亮度，增益钳 [0.90, 1.10]。
pub fn dehaze(buffer: &mut Buffer, air: [f32; 3], omega: f32, floor: f32) {
    let omega = omega.clamp(0.0, 1.0);
    let floor = floor.clamp(0.02, 0.8);
    let air = damp_air_chromaticity(air.map(|value| value.clamp(0.05, 1.0)));
    let air_luma = (0.2126 * air[0] + 0.7152 * air[1] + 0.0722 * air[2]).max(1e-3);
    // 修正二的动态下限（[0.20, 0.25] 带内随 omega 线性取）。
    let t_floor = floor.max(0.25 - 0.05 * omega);
    let width = buffer.width();
    let height = buffer.height();
    // 透射率图（先算再模糊，抑制块状伪影）；γ 重映射与动态下限在取用时施加。
    let mut transmission = Buffer::new(buffer.origin().0, buffer.origin().1, width, height);
    for y in 0..height {
        for x in 0..width {
            let pixel = buffer.pixel(x, y);
            // 暗通道：直通空间的最小通道值。
            let dark = pixel[0].min(pixel[1]).min(pixel[2]);
            let t = (1.0 - omega * dark / air_luma).clamp(0.0, 1.0);
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
            // 修正二：γ_t = 0.5 重映射 + 动态下限。
            let t = libm::sqrtf(transmission.pixel(x, y)[0].clamp(0.0, 1.0)).clamp(t_floor, 1.0);
            let straight = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
            let lum_in = 0.2126 * straight[0] + 0.7152 * straight[1] + 0.0722 * straight[2];
            // 修正三：逐通道恢复 0.65 + 亮度恢复 β = 0.35 混合。
            let lum_recovered = (lum_in - air_luma) / t + air_luma;
            let mut recovered = [0.0f32; 3];
            for channel in 0..3 {
                let per_channel = (straight[channel] - air[channel]) / t + air[channel];
                recovered[channel] = 0.65 * per_channel + 0.35 * lum_recovered;
            }
            // 修正四：前景中性再锚定（近中性输入 + 高透射 ⇒ 增益拉回亮度）。
            let chroma_in = straight[0].max(straight[1]).max(straight[2])
                - straight[0].min(straight[1]).min(straight[2]);
            if chroma_in <= 0.03 && t >= 0.5 {
                let lum_out = 0.2126 * recovered[0] + 0.7152 * recovered[1] + 0.0722 * recovered[2];
                if lum_out > 1e-6 {
                    for value in recovered.iter_mut() {
                        if *value > 1e-6 {
                            let gain = (lum_out / *value).clamp(0.90, 1.10);
                            *value *= gain;
                        }
                    }
                }
            }
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                out[channel] = recovered[channel].clamp(0.0, 1.0) * alpha;
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
    // 期 3 A4-3 修正一：估计值先过色度阻尼再报告（与恢复入口同口径）。
    Some((damp_air_chromaticity(air), sum / count as f32))
}

/// 暗角：按到画布中心的归一化距离做平滑衰减。
///
/// **期 3 A4-2 口径**：衰减乘子作用在**显示空间**（`out_d = d ·
/// (1 − strength·smoothstep(t))`，扩展保符号编解码、HDR 不钳）。旧实现
/// 在线性空间乘同一乘子——中灰处视觉效果约差一倍（乘子 0.7：线性乘
/// 使显示 0.5 → 约 0.43，显示乘直接得 0.35），预设 strength 数据是
/// 按显示直觉定的，旧实现系统性偏弱（设计 §2 A4-2）。几何（文档中心/
/// 对角线归一、smoothstep 形状）不变；乘子 = 1 的像素（中心区）逐位
/// 不动；strength = 0 恒等。
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
            if falloff == 1.0 {
                continue;
            }
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let mut out = [0.0f32; 4];
            out[3] = alpha;
            for channel in 0..3 {
                let straight = pixel[channel] / alpha;
                out[channel] =
                    srgb_decode_extended(srgb_encode_extended(straight) * falloff) * alpha;
            }
            buffer.set_pixel(x, y, out);
        }
    }
}

// **几何重采样底座**（照片修图三模块共用；专家咨询路线：零依赖自研）。
//
// 关键口径（只此一处 ✓）：
// - 插值永远在**预乘 RGBA** 上直接做（**严禁先除 alpha**：透明区的黑会污染半透明边缘 ✓）；
// - 双三次负瓣用**最内圈 2×2 的 min/max 钳制**（消灭 ringing；SDR 下保证 RGB≥0、0≤A≤1 ✓）；
// - 逆映射（遍历输出像素找源坐标），越界采样点写透明（不做边缘复制——warp 的语义是移动像素 ✓）。
//
// **⇒ 为什么是 `//` 而不是 `///`**：这一段是**模块级说明**，它下面紧跟的是
//   `sample_edge_clamped` 的**自己的** doc ⇒ 两个 doc 块之间有空行 ⇒ clippy 报
//   `empty line after doc comment`（第 1314 轮实测）✗

/// 边界复制采样（供双三次取 4×4 邻域用；**只在内核邻域内**用，越界策略由 warp 层决定 ✓）。
fn sample_edge_clamped(buffer: &Buffer, sx: i64, sy: i64) -> [f32; 4] {
    let (w, h) = (buffer.width() as i64, buffer.height() as i64);
    buffer.pixel(
        sx.clamp(0, w - 1).max(0) as u32,
        sy.clamp(0, h - 1).max(0) as u32,
    )
}

/// Catmull-Rom 权重（$b=0,c=0.5$；$t=0$ 时精确命中中心像素 ⇒ 恒等映射位精确 ✓）。
fn catmull_weights(t: f32) -> [f32; 4] {
    let (t2, t3) = (t * t, t * t * t);
    [
        -0.5 * t3 + t2 - 0.5 * t,
        1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
        0.5 * t3 - 0.5 * t2,
    ]
}

/// **双三次采样**（预乘 RGBA 直接插值 + 2×2 内圈钳制）。
pub fn sample_catmull_rom(buffer: &Buffer, x: f32, y: f32) -> [f32; 4] {
    let xi = x.floor() as i64;
    let yi = y.floor() as i64;
    let (tx, ty) = (x - xi as f32, y - yi as f32);
    let mut px = [[[0.0f32; 4]; 4]; 4];
    for (j, row) in px.iter_mut().enumerate() {
        for (i, slot) in row.iter_mut().enumerate() {
            *slot = sample_edge_clamped(buffer, xi + i as i64 - 1, yi + j as i64 - 1);
        }
    }
    let (wx, wy) = (catmull_weights(tx), catmull_weights(ty));
    let mut out = [0.0f32; 4];
    for c in 0..4 {
        let mut row = [0.0f32; 4];
        for j in 0..4 {
            row[j] = wx[0] * px[j][0][c]
                + wx[1] * px[j][1][c]
                + wx[2] * px[j][2][c]
                + wx[3] * px[j][3][c];
        }
        out[c] = wy[0] * row[0] + wy[1] * row[1] + wy[2] * row[2] + wy[3] * row[3];
    }
    // **2×2 内圈钳制**（专家路线）：中心 4 像素的 min/max ⇒ 负瓣超调被剪掉 ✓。
    let (mut lo, mut hi) = ([f32::INFINITY; 4], [f32::NEG_INFINITY; 4]);
    for row in px.iter().take(3).skip(1) {
        for cell in row.iter().take(3).skip(1) {
            for (c, &v) in cell.iter().enumerate() {
                lo[c] = lo[c].min(v);
                hi[c] = hi[c].max(v);
            }
        }
    }
    for c in 0..4 {
        out[c] = out[c].clamp(lo[c], hi[c]);
    }
    out[3] = out[3].clamp(0.0, 1.0);
    for v in out.iter_mut().take(3) {
        *v = v.max(0.0);
    }
    out
}

/// **逆映射遍历器**：对输出画布每个像素用 `map` 求源坐标并采样写回。
/// 越界源坐标写透明；**先读快照再写**（避免自反馈，见 blur_horizontal 的同源教训 ✓）。
pub fn warp_buffer(buffer: &mut Buffer, map: impl Fn(f32, f32) -> (f32, f32)) {
    let (w, h) = (buffer.width(), buffer.height());
    if w == 0 || h == 0 {
        return;
    }
    let snapshot: Vec<f32> = buffer.pixels().to_vec();
    let (ox, oy) = buffer.origin();
    let reader =
        Buffer::from_f32(ox, oy, w, h, &snapshot).expect("warp_buffer：快照尺寸与原 Buffer 一致");
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = map(x as f32, y as f32);
            let pixel = if sx < 0.0 || sy < 0.0 || sx > (w - 1) as f32 || sy > (h - 1) as f32 {
                [0.0; 4]
            } else {
                sample_catmull_rom(&reader, sx, sy)
            };
            buffer.set_pixel(x, y, pixel);
        }
    }
}

/// 单位正方形→任意四边形的单应（Paul Heckbert 闭式：只解一个 2×2 ⇒ 天然数值稳定 ✓）。
/// 角点顺序：`(0,0)→q0,(1,0)→q1,(1,1)→q2,(0,1)→q3`。
#[derive(Debug, Clone, Copy)]
struct UnitSquareToQuad {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
    g: f32,
    h: f32,
}

impl UnitSquareToQuad {
    fn map(&self, u: f32, v: f32) -> (f32, f32) {
        let den = self.g * u + self.h * v + 1.0;
        if den.abs() < 1e-9 {
            return (u, v);
        }
        (
            (self.a * u + self.b * v + self.c) / den,
            (self.d * u + self.e * v + self.f) / den,
        )
    }
}

/// 单位正方形→四边形（Heckbert 闭式）。`None` = 退化（分母≈0；凸校验一般已挡掉 ✓）。
fn unit_square_to_quad(q: [[f32; 2]; 4]) -> Option<UnitSquareToQuad> {
    let (p0, p1, p2, p3) = (q[0], q[1], q[2], q[3]);
    let (d1x, d1y) = (p1[0] - p2[0], p1[1] - p2[1]);
    let (d2x, d2y) = (p3[0] - p2[0], p3[1] - p2[1]);
    let (sx, sy) = (
        (p0[0] - p1[0]) + (p2[0] - p3[0]),
        (p0[1] - p1[1]) + (p2[1] - p3[1]),
    );
    let den = d1x * d2y - d1y * d2x;
    if den.abs() < 1e-12 {
        return None;
    }
    let g = (sx * d2y - sy * d2x) / den;
    let h = (d1x * sy - d1y * sx) / den;
    Some(UnitSquareToQuad {
        a: p1[0] - p0[0] + g * p1[0],
        b: p3[0] - p0[0] + h * p3[0],
        c: p0[0],
        d: p1[1] - p0[1] + g * p1[1],
        e: p3[1] - p0[1] + h * p3[1],
        f: p0[1],
        g,
        h,
    })
}

/// 四边形凸校验（四顶点叉积同号；共线也拒 ✓）——透视四角拖拽的防呆（专家：约 5 行 ✓）。
pub fn quad_is_convex(q: [[f32; 2]; 4]) -> bool {
    let mut sign = 0.0f32;
    for i in 0..4 {
        let (a, b, c) = (q[i], q[(i + 1) % 4], q[(i + 2) % 4]);
        let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if cross.abs() < 1e-9 {
            return false;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }
    true
}

/// 镜头参数（**手动滑杆**；数据库预留接口，首期不做 ✓）。
#[derive(Debug, Clone, Copy)]
pub struct LensParams {
    /// 径向畸变 k1（`r_src = r_dst·(1+k1·r²+k2·r⁴)`）。
    pub k1: f32,
    /// 径向畸变 k2。
    pub k2: f32,
    /// TCA：R 通道缩放系数（G 恒为 1）。
    pub tca_r: f32,
    /// TCA：B 通道缩放系数。
    pub tca_b: f32,
    /// 暗角强度（0..1；`gain = 1/(1+v1·r²+v2·r⁴)`，v1=2·s、v2=0.5·s）。
    pub vignette_strength: f32,
    /// 自动裁剪（默认开；越界填透明时按内接缩放）。
    pub auto_crop: bool,
}

impl Default for LensParams {
    fn default() -> Self {
        Self {
            k1: 0.0,
            k2: 0.0,
            tca_r: 1.0,
            tca_b: 1.0,
            vignette_strength: 0.0,
            auto_crop: true,
        }
    }
}

/// 归一化半径（中心为原点、半对角线为 1；专家口径 ✓）。
fn lens_normalized(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32) {
    let (cx, cy) = (w / 2.0, h / 2.0);
    let rmax = (cx * cx + cy * cy).sqrt().max(1e-9);
    let (nx, ny) = ((x - cx) / rmax, (y - cy) / rmax);
    (nx, ny, nx * nx + ny * ny)
}

/// **径向畸变多项式**（Brown-Conrady/PTLens 族）：`r_src = r_dst·(1+k1·r²+k2·r⁴)`，
/// 在归一化坐标上求值（纯函数，单元测试在 100 点网格上与 f64 参考实现比对 ✓）。
fn radial_distort(nx: f32, ny: f32, k1: f32, k2: f32) -> (f32, f32) {
    let r2 = nx * nx + ny * ny;
    let f = 1.0 + k1 * r2 + k2 * r2 * r2;
    (nx * f, ny * f)
}

/// **几何逆映射的坐标链**（`merged_geo_warp` 的逐像素计算与
/// [`AdjustmentKind::padding`] 的位移上界评估**共用同一份**，避免两处各写一遍后漂移 ✓）：
/// 文档坐标的输出像素 → （自动裁剪缩放 → 单应（若有）→ 镜头多项式）→ 源文档坐标，
/// 并附源位置的归一化半径平方（暗角 gain 用 ✓）。
fn geo_source_for(
    hom: Option<&UnitSquareToQuad>,
    lens: &LensParams,
    scale: f32,
    frame: (f32, f32),
    dx: f32,
    dy: f32,
) -> (f32, f32, f32) {
    let (wf, hf) = frame;
    // auto_crop：绕画面中心缩放（s=1 ⇒ 恒等 ✓）。
    let (u, v) = (0.5 + (dx / wf - 0.5) / scale, 0.5 + (dy / hf - 0.5) / scale);
    let (px, py) = match hom {
        Some(hom) => hom.map(u, v),
        None => (u * wf, v * hf),
    };
    let (_, _, r2) = lens_normalized(px, py, wf, hf);
    // k1=k2=0 ⇒ 直通（避免归一化往返的 f32 舍入破坏恒等的逐位精确 ✓）。
    let (gx, gy) = if lens.k1 != 0.0 || lens.k2 != 0.0 {
        let (cx, cy) = (wf / 2.0, hf / 2.0);
        let (nx, ny, _) = lens_normalized(px, py, wf, hf);
        let (dnx, dny) = radial_distort(nx, ny, lens.k1, lens.k2);
        let rmax = (cx * cx + cy * cy).sqrt().max(1e-9);
        (cx + dnx * rmax, cy + dny * rmax)
    } else {
        (px, py)
    };
    (gx, gy, r2)
}

/// **自动裁剪的缩放系数**（MVP 启发式，只放大不缩小；恒等情形恰为 1 ✓）。
/// 与坐标链分开算是因为位移上界评估也要先拿到它 ✓。
fn geo_auto_crop_scale(quad: Option<[[f32; 2]; 4]>, lens: &LensParams, frame: (f32, f32)) -> f32 {
    let (wf, hf) = frame;
    let mut scale = 1.0f32;
    if lens.auto_crop {
        if let Some(q) = quad {
            let edge = |i: usize, j: usize| {
                let (dx, dy) = (q[i][0] - q[j][0], q[i][1] - q[j][1]);
                (dx * dx + dy * dy).sqrt().max(1e-9)
            };
            let sx = (wf / edge(0, 1)).min(wf / edge(3, 2));
            let sy = (hf / edge(1, 2)).min(hf / edge(0, 3));
            scale = scale.max(sx.min(sy));
        }
        // 桶形畸变把边缘像素向外推 ⇒ 放大裁掉透明边（四角因子取大 ✓）。
        for (px, py) in [(0.0, 0.0), (wf, 0.0), (wf, hf), (0.0, hf)] {
            let (_, _, r2) = lens_normalized(px, py, wf, hf);
            scale = scale.max(1.0 + lens.k1 * r2 + lens.k2 * r2 * r2);
        }
        scale = scale.max(1.0);
    }
    scale
}

/// **透视+镜头合并逆映射**（专家链路，一次采样 ✓）：
/// `(x_out,y_out) → H（若透视）→ (x_undist,y_undist) → 代入正向畸变多项式 D(·) →
/// (x_src,y_src) → 一次采样`。**不需要求畸变多项式的逆** ✓（逆映射顺着物理畸变方向走 ✓）。
/// TCA：R/B 在同一链路里按系数缩放源坐标独立采样（Catmull-Rom 的 2×2 钳制即局部钳制，
/// 专家方案：防高光彩边震荡 ✓）。暗角 gain 在线性光、按**源位置**半径乘（重采样之前 ✓）。
///
/// **坐标口径（缺陷修复）**：`quad`、画面中心、归一化半径都按**整幅画面**（`canvas`）的
/// 文档坐标定义；`buffer` 只是这幅画面的一个窗口（分带/区域渲染时它是子缓冲，
/// `buffer.bbox()` 给出窗口原点）。旧实现把子缓冲当整幅图（按带高归一化、quad 按带本地
/// 坐标解释）⇒ 并行分带时每带下半采样越界写透明、输出交替白带。
/// 窗口外的源像素不在本缓冲里 ⇒ 写透明；渲染器按 [`AdjustmentKind::padding`]
/// 申报的位移上界外扩窗口，正常参数下需要的源像素都在窗口内 ✓。
fn merged_geo_warp(
    buffer: &mut Buffer,
    hom: Option<UnitSquareToQuad>,
    quad: Option<[[f32; 2]; 4]>,
    lens: &LensParams,
    canvas: (f64, f64),
) {
    let (w, h) = (buffer.width(), buffer.height());
    if w == 0 || h == 0 {
        return;
    }
    let frame = (canvas.0 as f32, canvas.1 as f32);
    let scale = geo_auto_crop_scale(quad, lens, frame);
    let (cx, cy) = (frame.0 / 2.0, frame.1 / 2.0);
    let (v1, v2) = (2.0 * lens.vignette_strength, 0.5 * lens.vignette_strength);
    let snapshot: Vec<f32> = buffer.pixels().to_vec();
    let (ox, oy) = buffer.origin();
    let reader = Buffer::from_f32(ox, oy, w, h, &snapshot)
        .expect("merged_geo_warp：快照尺寸与原 Buffer 一致");
    // 窗口原点（文档坐标，f64 框 ⇒ 取 f32 与 vignette 的口径一致 ✓）。
    let bbox = buffer.bbox();
    let (fox, foy) = (bbox.x as f32, bbox.y as f32);
    // 源文档坐标 → 窗口本地坐标后判界（窗口外 ⇒ 透明，与旧整幅语义一致 ✓）。
    let in_window =
        |sx: f32, sy: f32| sx >= 0.0 && sy >= 0.0 && sx <= (w - 1) as f32 && sy <= (h - 1) as f32;
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (fox + x as f32, foy + y as f32);
            let (gx, gy, r2) = geo_source_for(hom.as_ref(), lens, scale, frame, dx, dy);
            let (lx, ly) = (gx - fox, gy - foy);
            if !in_window(lx, ly) {
                buffer.set_pixel(x, y, [0.0; 4]);
                continue;
            }
            let base = sample_catmull_rom(&reader, lx, ly);
            // 暗角 gain（源半径 r2 ✓）。
            let gain = 1.0 / (1.0 + v1 * r2 + v2 * r2 * r2);
            let sample_channel = |ch: usize, tca: f32| {
                if tca == 1.0 {
                    base[ch]
                } else {
                    // TCA 缩放在文档坐标里绕画面中心做，再换回窗口本地坐标采样 ✓。
                    let (sx, sy) = (cx + (gx - cx) * tca - fox, cy + (gy - cy) * tca - foy);
                    if in_window(sx, sy) {
                        sample_catmull_rom(&reader, sx, sy)[ch]
                    } else {
                        0.0
                    }
                }
            };
            buffer.set_pixel(
                x,
                y,
                [
                    (sample_channel(0, lens.tca_r) * gain).max(0.0),
                    (base[1] * gain).max(0.0),
                    (sample_channel(2, lens.tca_b) * gain).max(0.0),
                    base[3],
                ],
            );
        }
    }
}

/// **镜头校正**（手动参数；单次逆映射 ✓）。`k1/k2` 畸变、`tca_r/tca_b` 色差、
/// `vignette_strength` 暗角、`auto_crop` 自动裁剪。
/// `canvas` 是整幅画面的文档尺寸（子缓冲渲染时几何中心/半径按它算，见 `merged_geo_warp` ✓）。
#[allow(clippy::too_many_arguments)]
pub fn lens_correction(
    buffer: &mut Buffer,
    k1: f32,
    k2: f32,
    tca_r: f32,
    tca_b: f32,
    vignette_strength: f32,
    auto_crop: bool,
    canvas: (f64, f64),
) {
    merged_geo_warp(
        buffer,
        None,
        None,
        &LensParams {
            k1,
            k2,
            tca_r,
            tca_b,
            vignette_strength,
            auto_crop,
        },
        canvas,
    );
}

/// **透视校正**（四角拖拽 MVP）。`quad` 为源四边形（顺序 q0..q3 对应矩形四角，
/// **文档坐标**），必须凸（工具层已校验；这里复核，`false` = 拒绝且不动 ✓）。
/// `k1/k2` 可选：与镜头畸变合并一次采样（专家链路 ✓）。
/// `canvas` 是整幅画面的文档尺寸（子缓冲渲染时 quad 按它解释，见 `merged_geo_warp` ✓）。
pub fn perspective_warp(
    buffer: &mut Buffer,
    quad: [[f32; 2]; 4],
    k1: f32,
    k2: f32,
    auto_crop: bool,
    canvas: (f64, f64),
) -> bool {
    if !quad_is_convex(quad) {
        return false;
    }
    let Some(hom) = unit_square_to_quad(quad) else {
        return false;
    };
    merged_geo_warp(
        buffer,
        Some(hom),
        Some(quad),
        &LensParams {
            k1,
            k2,
            auto_crop,
            ..LensParams::default()
        },
        canvas,
    );
    true
}

/// **几何调整的位移上界**（像素）：输出像素与其源像素在 x/y 上的最大距离。
///
/// 算法：在画面上取 33×33 网格逐点走 [`geo_source_for`] 同一条坐标链（含 TCA 的
/// 额外采样点），取最大值后加「一格对角线 + 1px」余量（坐标链是光滑映射，
/// 格内峰值不会离格点太远 ✓）。渲染器据此把子缓冲窗口外扩到装得下全部源像素；
/// 上界超过渲染器的外扩上限时它会显式告警（不静默不一致 ✓）。
fn geo_displacement_bound(
    hom: Option<&UnitSquareToQuad>,
    lens: &LensParams,
    scale: f32,
    frame: (f32, f32),
) -> u32 {
    let (wf, hf) = frame;
    if wf <= 0.0 || hf <= 0.0 {
        return 0;
    }
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let mut bound = 0.0f32;
    const GRID: u32 = 32;
    for iy in 0..=GRID {
        for ix in 0..=GRID {
            let (dx, dy) = (wf * ix as f32 / GRID as f32, hf * iy as f32 / GRID as f32);
            let (gx, gy, _) = geo_source_for(hom, lens, scale, frame, dx, dy);
            for (sx, sy) in [
                (gx, gy),
                (cx + (gx - cx) * lens.tca_r, cy + (gy - cy) * lens.tca_r),
                (cx + (gx - cx) * lens.tca_b, cy + (gy - cy) * lens.tca_b),
            ] {
                bound = bound.max((sx - dx).abs()).max((sy - dy).abs());
            }
        }
    }
    // **★ 不用 `.hypot(`** ✓（第 1335 轮；`tool-kernel-determinism` 抓到）
    //   **为什么**：`.hypot` 是**各平台各自实现**的浮点函数 ⇒ wasm 与 native 可能给不同位
    //     ⇒ 而这里算的是 **padding 上界** ⇒ 差一个 ulp 就可能让分带边界不同 ⇒ **画面不一致** ✗
    //   **修法**：`sqrt` 是 IEEE **要求正确舍入**的 ⇒ `(a*a + b*b).sqrt()` 逐位确定 ✓
    //   **为什么不会溢出**：`a`/`b` 是 `画幅 / 33` ⇒ 量级 10¹ 量级 ⇒ 平方和远小于 `f32::MAX` ✓
    let cell = {
        let ca = wf / GRID as f32;
        let cb = hf / GRID as f32;
        (ca * ca + cb * cb).sqrt()
    };
    (bound + cell + 1.0).ceil() as u32
}

/// **标量方框模糊**（单通道 `Vec<f32>` 版；与 `box_blur` 同一边界语义：clamp 复制 ✓）。
/// 供引导滤波的均值计算用（复用已有基建，专家路线 ✓）。
fn box_blur_scalar(data: &mut [f32], w: u32, h: u32, radius: u32) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    let (w, h) = (w as usize, h as usize);
    let window = (radius as usize * 2 + 1) as f32;
    let r = radius as i64;
    // 水平。
    let mut row = vec![0.0f32; w];
    for y in 0..h {
        let base = y * w;
        row.copy_from_slice(&data[base..base + w]);
        let mut acc = 0.0f32;
        for offset in -r..=r {
            acc += row[offset.clamp(0, w as i64 - 1) as usize];
        }
        for x in 0..w {
            data[base + x] = acc / window;
            if (x as i64) + 1 < w as i64 {
                acc -= row[(x as i64 - r).clamp(0, w as i64 - 1) as usize];
                acc += row[(x as i64 + r + 1).clamp(0, w as i64 - 1) as usize];
            }
        }
    }
    // 垂直。
    let mut col = vec![0.0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = data[y * w + x];
        }
        let mut acc = 0.0f32;
        for offset in -r..=r {
            acc += col[offset.clamp(0, h as i64 - 1) as usize];
        }
        for y in 0..h {
            data[y * w + x] = acc / window;
            if (y as i64) + 1 < h as i64 {
                acc -= col[(y as i64 - r).clamp(0, h as i64 - 1) as usize];
                acc += col[(y as i64 + r + 1).clamp(0, h as i64 - 1) as usize];
            }
        }
    }
}

/// 自引导滤波（guided filter，p=I）：`q = mean(a)·I + mean(b)`，
/// `a = var/(var+ε)`、`b = mean−a·mean`。半径无关 O(N)（专家路线 ✓）。
fn guided_filter_self(y: &mut [f32], w: u32, h: u32, radius: u32, epsilon: f32) {
    let n = (w * h) as usize;
    let mut mean_i = y.to_vec();
    box_blur_scalar(&mut mean_i, w, h, radius);
    let mut mean_ii: Vec<f32> = y.iter().map(|v| v * v).collect();
    box_blur_scalar(&mut mean_ii, w, h, radius);
    let mut a = vec![0.0f32; n];
    let mut b = vec![0.0f32; n];
    for i in 0..n {
        let var = (mean_ii[i] - mean_i[i] * mean_i[i]).max(0.0);
        a[i] = var / (var + epsilon);
        b[i] = mean_i[i] * (1.0 - a[i]);
    }
    box_blur_scalar(&mut a, w, h, radius);
    box_blur_scalar(&mut b, w, h, radius);
    for i in 0..n {
        y[i] = a[i] * y[i] + b[i];
    }
}

/// **降噪**（专家路线：自引导滤波 + √Linear 感知映射 + 亮色分离）。
///
/// 流程：除 alpha 取直通色 → `√c` 感知空间（抵消泊松噪声方差）→ 拆 Y / Cb / Cr →
/// Y 跑自引导滤波（保边）→ Cb/Cr 大半径方框模糊（抹彩噪）→ 平方回线性 RGB → 重乘 alpha。
/// 参数抄 Lightroom（**不暴露 σ_space/σ_range** ✓）：
/// `luminance` 0..1（正则化 ε 与混合权重）、`detail` 0..1（引导半径）、
/// `color` 0..1（色度模糊半径）。
pub fn denoise(buffer: &mut Buffer, luminance: f32, detail: f32, color: f32) {
    let (w, h) = (buffer.width(), buffer.height());
    if w == 0 || h == 0 {
        return;
    }
    let luminance = luminance.clamp(0.0, 1.0);
    let detail = detail.clamp(0.0, 1.0);
    let color = color.clamp(0.0, 1.0);
    if luminance == 0.0 && color == 0.0 {
        return;
    }
    let n = (w * h) as usize;
    let mut r = vec![0.0f32; n];
    let mut g = vec![0.0f32; n];
    let mut b = vec![0.0f32; n];
    let mut alpha = vec![0.0f32; n];
    for (i, pix) in buffer.pixels().chunks_exact(4).enumerate() {
        let a = pix[3];
        alpha[i] = a;
        if a > 0.0 {
            r[i] = (pix[0] / a).max(0.0).sqrt();
            g[i] = (pix[1] / a).max(0.0).sqrt();
            b[i] = (pix[2] / a).max(0.0).sqrt();
        }
    }
    // Y / Cb / Cr（BT.601 式，作用于 √ 空间 ✓）。
    let mut y = vec![0.0f32; n];
    let mut cb = vec![0.0f32; n];
    let mut cr = vec![0.0f32; n];
    for i in 0..n {
        y[i] = 0.299 * r[i] + 0.587 * g[i] + 0.114 * b[i];
        cb[i] = 0.5 * (b[i] - y[i]) / 0.886 + 0.5;
        cr[i] = 0.5 * (r[i] - y[i]) / 0.701 + 0.5;
    }
    // Y 保边降噪（luminance = 混合权重 k + ε 缩放，同向驱动：越大越平滑，专家映射 ✓）。
    if luminance > 0.0 {
        let radius = (1.0 + detail * 8.0).round() as u32;
        let epsilon = 1e-4 + luminance * 5e-3;
        let mut filtered = y.clone();
        guided_filter_self(&mut filtered, w, h, radius, epsilon);
        for i in 0..n {
            y[i] += luminance * (filtered[i] - y[i]);
        }
    }
    // Cb/Cr 大半径抹除。
    let chroma_r = (color * 12.0).round() as u32;
    if chroma_r > 0 {
        box_blur_scalar(&mut cb, w, h, chroma_r);
        box_blur_scalar(&mut cr, w, h, chroma_r);
    }
    // 重建：√ 空间 → 平方回线性 → 重乘 alpha。
    let out = buffer.pixels_mut();
    for i in 0..n {
        let yy = y[i];
        let rr = (yy + 1.402 * (cr[i] - 0.5)).max(0.0);
        let gg = (yy - 0.344136 * (cb[i] - 0.5) - 0.714136 * (cr[i] - 0.5)).max(0.0);
        let bb = (yy + 1.772 * (cb[i] - 0.5)).max(0.0);
        let a = alpha[i];
        let base = i * 4;
        // **钳到 alpha**（守预乘不变量 `rgb ≤ alpha` ✓，与 sharpen/glow/clarity 同纪律 ✓）：
        // √ 空间 YCbCr 逆变换对高色度像素能推出 rr > 1 ⇒ rr²·a 超过 a ✗。
        out[base] = (rr * rr * a).clamp(0.0, a);
        out[base + 1] = (gg * gg * a).clamp(0.0, a);
        out[base + 2] = (bb * bb * a).clamp(0.0, a);
        out[base + 3] = a;
    }
}

/// **大气光估计（调整层内版）**：暗通道最亮的 0.1% 像素的平均直通色
/// （与 [`estimate_atmospheric_light`] 同口径，只是数据源是 `Buffer` 的直通色 ✓）。
/// 排序换成 512 桶直方图取阈值（O(N)、无大额分配；排序版在 1200×800 上是主要瓶颈 ✗）。
/// 无不透明像素时退回近白雾色 `[0.95; 3]`。
fn estimate_air_from_buffer(buffer: &Buffer) -> [f32; 3] {
    const BINS: usize = 512;
    let mut hist = [0u64; BINS];
    let mut total = 0u64;
    for pixel in buffer.pixels().chunks_exact(4) {
        let alpha = pixel[3];
        if alpha <= 0.0 {
            continue;
        }
        let dark = (pixel[0] / alpha)
            .min(pixel[1] / alpha)
            .min(pixel[2] / alpha)
            .clamp(0.0, 1.0);
        let bin = ((dark * (BINS - 1) as f32) as usize).min(BINS - 1);
        hist[bin] += 1;
        total += 1;
    }
    if total == 0 {
        return [0.95, 0.95, 0.95];
    }
    let need = ((total as f64 * 0.001).ceil() as u64).max(1);
    let mut acc = 0u64;
    let mut thresh_bin = BINS - 1;
    for bin in (0..BINS).rev() {
        acc += hist[bin];
        if acc >= need {
            thresh_bin = bin;
            break;
        }
    }
    let thresh = thresh_bin as f32 / (BINS - 1) as f32;
    let mut air = [0.0f32; 3];
    let mut count = 0u64;
    for pixel in buffer.pixels().chunks_exact(4) {
        let alpha = pixel[3];
        if alpha <= 0.0 {
            continue;
        }
        let straight = [pixel[0] / alpha, pixel[1] / alpha, pixel[2] / alpha];
        let dark = straight[0].min(straight[1]).min(straight[2]);
        if dark >= thresh {
            for (slot, value) in air.iter_mut().zip(straight.iter()) {
                *slot += value;
            }
            count += 1;
        }
    }
    if count == 0 {
        return [0.95, 0.95, 0.95];
    }
    let scale = 1.0 / count as f32;
    for value in air.iter_mut() {
        *value = (*value * scale).clamp(0.05, 1.0);
    }
    // 期 3 A4-3 修正一：估计值先过色度阻尼（与恢复入口同口径）。
    damp_air_chromaticity(air)
}

/// **去雾调整（Presence）**：`amount` −1..1，0 = 逐位恒等 ✓。
///
/// 正值走暗通道先验（复用滤镜版 [`dehaze`] 内核）：大气光由
/// [`estimate_air_from_buffer`] 在**当前缓冲内**估计，`omega = amount × 0.95`、
/// 透射率下限名义 0.1（期 3 A4-3 后有效下限为动态值，见 [`dehaze`] 文档）。
/// **与滤镜版的口径差**：滤镜版要求调用方给 `air` 以保证
/// 分块 == 整幅；调整版在缓冲内估计 ⇒ 跨 tile 的大气光可能不同（README 已标注，
/// 这是与 Lightroom 全图估计实现的主要差距之一）。
/// 负值反向加雾：直通色朝大气光混合 `|amount| × 0.6`，对比单调下降（可做雾效/柔化背景）。
pub fn dehaze_adjustment(buffer: &mut Buffer, amount: f32) {
    let amount = amount.clamp(-1.0, 1.0);
    if amount == 0.0 {
        return;
    }
    let air = estimate_air_from_buffer(buffer);
    if amount > 0.0 {
        dehaze(buffer, air, amount * 0.95, 0.1);
    } else {
        let haze = -amount * 0.6;
        for_each_straight_color(buffer, |color| {
            for (channel, slot) in color.iter_mut().enumerate() {
                *slot = (*slot * (1.0 - haze) + air[channel] * haze).clamp(0.0, 1.0);
            }
        });
    }
}

/// **引导滤波细节调整（clarity / texture 共用底座）**。
///
/// **期 3 A4-1 口径（专家 Q1 定版）**：细节必须在 **sRGB 明度 Y'** 上算——
/// 旧实现在线性明度上跑：中间调掩模的「中间」落在线性 0.5 ≈ 显示 0.735
/// （偏高光一档）、线性明度与输出双双钳 [0, 1]（HDR 细节被压平）、
/// epsilon 的量纲也随错空间漂移。新口径：
/// 1. 直通色逐通道扩展编码得显示值，`Y' = 0.2126/0.7152/0.0722` 加权和
///    （不钳：HDR 的 Y' 可 > 1、负值保符号）；
/// 2. Y' 平面跑一趟自引导滤波（复用降噪的 [`guided_filter_self`]，保边），
///    `detail = Y' − base`；
/// 3. 中间调掩模 `1 − (2ŷ − 1)²`，ŷ = Y' 钳 [0, 1]——掩模中心回到
///    **显示中灰**；`new_Y' = Y' + amount·detail·weight`；
/// 4. **亮度比回写**：`scale = new_Y' / Y'`（Y' > 0 时），直通线性色等比
///    缩放——保色相、且**不钳 [0, 1]**（HDR 细节增强后仍是 HDR）。
///    Y' ≤ 0 的像素（纯黑/负亮度）无亮度可缩放 ⇒ 不动（其 detail 在
///    平坦黑场里恒为 0，数学上一致）。
///
/// `amount` −1..1（0 = 逐位恒等 ✓；负值压细节 ⇒ 柔化）。半径/epsilon
/// 数值不变（epsilon 的量纲随平面换成 Y'——这正是刻度重标定的一部分）。
fn guided_detail_adjust(
    buffer: &mut Buffer,
    amount: f32,
    radius: u32,
    epsilon: f32,
    midtone_weighted: bool,
) {
    let amount = amount.clamp(-1.0, 1.0);
    if amount == 0.0 {
        return;
    }
    let (w, h) = (buffer.width(), buffer.height());
    if w == 0 || h == 0 {
        return;
    }
    let n = (w * h) as usize;
    let mut luma = vec![0.0f32; n];
    for (i, pixel) in buffer.pixels().chunks_exact(4).enumerate() {
        let a = pixel[3];
        if a > 0.0 {
            let straight = [pixel[0] / a, pixel[1] / a, pixel[2] / a];
            luma[i] = 0.2126 * srgb_encode_extended(straight[0])
                + 0.7152 * srgb_encode_extended(straight[1])
                + 0.0722 * srgb_encode_extended(straight[2]);
        }
    }
    let mut base = luma.clone();
    guided_filter_self(&mut base, w, h, radius, epsilon);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            let pixel = buffer.pixel(x, y);
            let alpha = pixel[3];
            if alpha <= 0.0 {
                continue;
            }
            let l = luma[i];
            if l <= 0.0 {
                continue;
            }
            let weight = if midtone_weighted {
                let centered = 2.0 * l.clamp(0.0, 1.0) - 1.0;
                (1.0 - centered * centered).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let new_luma = l + amount * (l - base[i]) * weight;
            let scale = (new_luma / l).max(0.0);
            // 数值零守卫：引导滤波在常数平面上的浮点残差（实测 scale 偏差
            // ~2.4e-7）不得改写像素——「平坦区逐位不动」是既有不变量。
            if (scale - 1.0).abs() <= 1e-6 {
                continue;
            }
            buffer.set_pixel(
                x,
                y,
                [pixel[0] * scale, pixel[1] * scale, pixel[2] * scale, alpha],
            );
        }
    }
}

/// **局部对比调整（Presence Clarity）**：`amount` −1..1，0 = 逐位恒等 ✓。
///
/// 大半径（16）自引导滤波保边细节 + 中间调加权 `1 − (2ŷ − 1)²`（ŷ 为显示
/// 明度 Y'，期 3 A4-1：掩模中心在显示中灰；中间调最强、极暗/极亮抑制防光晕 ✓）。
/// 正值增强中频局部对比；负值压中频 ⇒ 柔化（Orton 柔光的那一半靠它近似 ✓）。
pub fn clarity_adjustment(buffer: &mut Buffer, amount: f32) {
    guided_detail_adjust(buffer, amount, 16, 0.005, true);
}

/// **纹理调整（Presence Texture）**：`amount` −1..1，0 = 逐位恒等 ✓。
///
/// 小半径（3）自引导滤波细节、无中间调加权：只动最细一档高频，
/// 中/大尺度结构（clarity 的地盘）基本不碰。正值强化细纹理；负值平滑细纹理（磨皮方向）。
pub fn texture_adjustment(buffer: &mut Buffer, amount: f32) {
    guided_detail_adjust(buffer, amount, 3, 0.001, false);
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

/// 对显示编码 RGBA8 缓冲施加高斯模糊（期 4 D1，频率分离的低频层用）。
///
/// **薄封装，不写第二份模糊**：只做「直 RGBA8 → 预乘 f32 缓冲 →
/// [`gaussian_blur`] → 反预乘回 u8」的往返；模糊核本身就是
/// [`gaussian_blur`]（含其 feature-gated GPU 分支，GPU 与 CPU 在 u8 上
/// 差 ≤1 LSB，见该函数的逐位口径说明——频率分离的重建容差 2/255 覆盖它）。
/// 关键：**不在此做 sRGB 编解码**——输入字节就是显示编码值，模糊
/// 作用在显示编码值上（与设计「对 RGBA8 缓冲施加」同形）；线性空间
/// 的换算由调用方（频率分离的载荷公式）负责，两处不要混。
/// 全透明像素（a = 0）反预乘时按 0 处理，不产生 NaN。
pub fn gaussian_blur_rgba8(pixels: &[u8], width: u32, height: u32, sigma: f32) -> Vec<u8> {
    let count = (width as usize).saturating_mul(height as usize);
    assert_eq!(
        pixels.len(),
        count * 4,
        "gaussian_blur_rgba8 输入长度必须是 width × height × 4"
    );
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut buffer = Buffer::new(0, 0, width, height);
    for (index, chunk) in pixels.chunks_exact(4).enumerate() {
        let x = (index % width as usize) as u32;
        let y = (index / width as usize) as u32;
        let a = chunk[3] as f32 / 255.0;
        buffer.set_pixel(
            x,
            y,
            [
                chunk[0] as f32 / 255.0 * a,
                chunk[1] as f32 / 255.0 * a,
                chunk[2] as f32 / 255.0 * a,
                a,
            ],
        );
    }
    gaussian_blur(&mut buffer, sigma);
    let mut out = vec![0u8; count * 4];
    for (index, chunk) in out.chunks_exact_mut(4).enumerate() {
        let x = (index % width as usize) as u32;
        let y = (index / width as usize) as u32;
        let pixel = buffer.pixel(x, y);
        let a = pixel[3];
        let straight = if a > 0.0 {
            [pixel[0] / a, pixel[1] / a, pixel[2] / a]
        } else {
            [0.0, 0.0, 0.0]
        };
        chunk[0] = (straight[0].clamp(0.0, 1.0) * 255.0).round() as u8;
        chunk[1] = (straight[1].clamp(0.0, 1.0) * 255.0).round() as u8;
        chunk[2] = (straight[2].clamp(0.0, 1.0) * 255.0).round() as u8;
        chunk[3] = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
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

/// 水平卷积的串行实现（原实现原样保留；wasm 与小图回退都走它）。
fn convolve_horizontal_serial(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
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

/// 水平卷积的行带实现：`pixels` 是连续若干行的 f32 切片（行主序，每像素 4 个 f32）。
///
/// 每行先快照再写回（与串行版同一读后写顺序）；每输出像素的抽头累加顺序与串行版
/// 相同 ⇒ 逐位不变。行与行之间无依赖 ⇒ 行带怎么切都不改变任何字节。
fn convolve_horizontal_band(pixels: &mut [f32], width: usize, kernel: &[f32], radius: i32) {
    let row_len = width * 4;
    let rows = pixels.len() / row_len;
    let mut snapshot = vec![0.0f32; row_len];
    for r in 0..rows {
        snapshot.copy_from_slice(&pixels[r * row_len..(r + 1) * row_len]);
        let out_row = &mut pixels[r * row_len..(r + 1) * row_len];
        for x in 0..width {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_x = x as i64 + index as i64 - radius as i64;
                let clamped = source_x.clamp(0, width as i64 - 1) as usize;
                let pixel = &snapshot[clamped * 4..clamped * 4 + 4];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            out_row[x * 4..x * 4 + 4].copy_from_slice(&acc);
        }
    }
}

/// 水平高斯卷积：大图上按行带并行（`crate::rows` 的切法与上限），小图/单核/wasm 走串行。
///
/// 分派条件与 [`crate::Buffer::to_rgba8`] 的并行版同形（worker 数、像素阈值、切得出多块）。
fn convolve_horizontal(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width() as usize;
    let height = buffer.height() as usize;
    if width == 0 || height == 0 {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        convolve_horizontal_serial(buffer, kernel, radius);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = crate::render::workers(None).min(crate::rows::MAX_WORKERS);
        if workers <= 1
            || width * height < crate::rows::PARALLEL_MIN_PIXELS
            || crate::rows::split_bands(height as u32, workers).len() <= 1
        {
            convolve_horizontal_serial(buffer, kernel, radius);
            return;
        }
        let bands = crate::rows::split_bands(height as u32, workers);
        let pixels = buffer.pixels_mut();
        let mut jobs: Vec<&mut [f32]> = Vec::with_capacity(bands.len());
        let mut rest: &mut [f32] = pixels;
        for (_, rows) in &bands {
            let len = *rows as usize * width * 4;
            let (head, tail) = rest.split_at_mut(len);
            jobs.push(head);
            rest = tail;
        }
        std::thread::scope(|scope| {
            for band in jobs {
                scope.spawn(move || convolve_horizontal_band(band, width, kernel, radius));
            }
        });
    }
}

/// 垂直卷积的串行实现（原实现原样保留；wasm 与小图回退都走它）。
fn convolve_vertical_serial(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
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

/// 垂直卷积的行带实现：从整幅输入快照 `snapshot` 读、往 `out_pixels` 的连续行写。
///
/// 串行版是逐列快照：写列 x′ 不影响读列 x（读/写集不交），且列内快照在写之前取 ⇒
/// 整幅快照与逐列快照读到的值相同；每输出像素的抽头累加顺序与串行版相同 ⇒ 逐位不变。
/// 快照是一份整幅 f32 副本（4K 上约 205MB 瞬时，纵向并行换取；横向不需要它）。
fn convolve_vertical_band(
    out_pixels: &mut [f32],
    snapshot: &[f32],
    width: usize,
    height: usize,
    row_start: usize,
    kernel: &[f32],
    radius: i32,
) {
    let row_len = width * 4;
    let rows = out_pixels.len() / row_len;
    for r in 0..rows {
        let y = row_start + r;
        for x in 0..width {
            let mut acc = [0.0f32; 4];
            for (index, weight) in kernel.iter().enumerate() {
                let source_y = y as i64 + index as i64 - radius as i64;
                let clamped = source_y.clamp(0, height as i64 - 1) as usize;
                let base = (clamped * width + x) * 4;
                let pixel = &snapshot[base..base + 4];
                for channel in 0..4 {
                    acc[channel] += pixel[channel] * weight;
                }
            }
            let dst = &mut out_pixels[(r * width + x) * 4..(r * width + x) * 4 + 4];
            dst.copy_from_slice(&acc);
        }
    }
}

/// 垂直高斯卷积：大图上按行带并行（先整幅输入快照一次，再按行带并行写），小图/单核/wasm 走串行。
///
/// 分派条件与 [`crate::Buffer::to_rgba8`] 的并行版同形（worker 数、像素阈值、切得出多块）。
fn convolve_vertical(buffer: &mut Buffer, kernel: &[f32], radius: i32) {
    let width = buffer.width() as usize;
    let height = buffer.height() as usize;
    if width == 0 || height == 0 {
        return;
    }
    #[cfg(target_arch = "wasm32")]
    {
        convolve_vertical_serial(buffer, kernel, radius);
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let workers = crate::render::workers(None).min(crate::rows::MAX_WORKERS);
        if workers <= 1
            || width * height < crate::rows::PARALLEL_MIN_PIXELS
            || crate::rows::split_bands(height as u32, workers).len() <= 1
        {
            convolve_vertical_serial(buffer, kernel, radius);
            return;
        }
        // 整幅输入快照一次（见 convolve_vertical_band 的说明），再按行带并行写。
        let snapshot = buffer.pixels().to_vec();
        let bands = crate::rows::split_bands(height as u32, workers);
        let pixels = buffer.pixels_mut();
        let mut jobs: Vec<(usize, &mut [f32])> = Vec::with_capacity(bands.len());
        let mut rest: &mut [f32] = pixels;
        for (row_start, rows) in &bands {
            let len = *rows as usize * width * 4;
            let (head, tail) = rest.split_at_mut(len);
            jobs.push((*row_start as usize, head));
            rest = tail;
        }
        std::thread::scope(|scope| {
            for (row_start, band) in jobs {
                let snapshot = &snapshot;
                scope.spawn(move || {
                    convolve_vertical_band(band, snapshot, width, height, row_start, kernel, radius)
                });
            }
        });
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
pub const ADJUSTMENT_NAMES: [&str; 26] = [
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
    "calibrate_primaries",
    "gradient_map",
    "shadows_highlights",
    "selective_color",
    "color_lut",
    "red_eye",
    "denoise",
    "lens",
    "perspective",
    "dehaze",
    "clarity",
    "texture",
];

/// 内核支持的滤镜名。
pub const FILTER_NAMES: [&str; 15] = [
    "box_blur",
    // **`blur` 是 `box_blur` 的**别名** ✓**（第 1513 轮；外部实测发现它**到不了**）：
    //   `FilterKind::from_name` 认 `"box_blur" | "blur"` ✓，而**这个白名单漏了 `blur`** ✗
    //   ⇒ 于是 `blur` 会报「内核未实现该滤镜：blur」⇒ **有实现却用不了** ✓
    //   **⇒ 这正是本文件下面注释警告过的那个坑**（「漏了就会报内核未实现」）✓
    "blur",
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

/// **守卫：`from_name` 认识的每个名字都必须在 `FILTER_NAMES` 里** ✓
///
/// **⇒ 为什么需要**（第 1513 轮）：`blur` 曾经**只在 `from_name` 里、不在白名单里** ✗
///   ⇒ 于是它报「内核未实现该滤镜：blur」⇒ **有实现却到不了** ✓
/// **⇒ ⇒ 而这条守卫是**双向**的**：白名单里的每个名字也必须 `from_name` 认 ✓
/// **⇒ ⇒ ⇒ 判据断的是**两个集合相等**（结构量），不是数量 ✓
#[cfg(test)]
mod name_whitelist_tests {
    use super::*;

    #[test]
    fn from_name_and_filter_names_agree() {
        // ① 白名单里每个名字都要能被 from_name 认出来
        for name in FILTER_NAMES {
            assert!(
                FilterKind::from_name(name).is_some(),
                "白名单里的 `{name}` 在 from_name 里认不出来 ⇒ 用户会拿到「内核未实现」✗"
            );
        }
        // ② from_name 认的所有别名都要在白名单里（`blur` 就是在这里被抓到的）
        let aliases = [
            "box_blur",
            "blur",
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
            "relief",
        ];
        for name in aliases {
            let known = FilterKind::from_name(name).is_some();
            let listed = FILTER_NAMES.contains(&name);
            assert_eq!(
                known, listed,
                "`{name}` 的两种归属不一致：from_name 认={known}、白名单有={listed} \
                 ⇒ 这种不一致会让用户看到「内核未实现」而实际上有实现 ✗"
            );
        }
    }
}

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

/// 黑白 8 档权重（D2-2b）：`ranges` 对象每档一个乘子，域 [0, 2]、缺省 1.0；
/// 无 `ranges` 键 ⇒ `None`（分发处走现行三权路）。
fn bw_ranges(params: &Value) -> Option<[f32; 8]> {
    let ranges_obj = params.get("ranges")?.as_object()?;
    let mut ranges = [1.0f32; 8];
    for (index, name) in HSL_RANGE_NAMES.iter().enumerate() {
        if let Some(value) = ranges_obj.get(*name).and_then(Value::as_f64) {
            ranges[index] = value as f32;
        }
    }
    Some(ranges)
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

/// sRGB 编码（符号保持）：线性负值保持为负（越界矩阵元不取 NaN）。
fn srgb_encode_signed(value: f64) -> f64 {
    if value < 0.0 {
        -srgb_encode_signed(-value)
    } else if value <= 0.0031308 {
        12.92 * value
    } else {
        1.055 * libm::pow(value, 1.0 / 2.4) - 0.055
    }
}

/// 解 3×3 线性方程（列向量形式）：M·x = rhs，M 的列由 `cols` 给出。
/// 行列式过小（|det| < 1e-9）返回 None（调用方回退）。
fn solve_3x3_cols(cols: &[[f64; 3]; 3], rhs: [f64; 3]) -> Option<[f64; 3]> {
    let det_of = |m: &[[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[1][0] * (m[0][1] * m[2][2] - m[0][2] * m[2][1])
            + m[2][0] * (m[0][1] * m[1][2] - m[0][2] * m[1][1])
    };
    let determinant = det_of(cols);
    if determinant.abs() < 1e-9 {
        return None;
    }
    let mut out = [0.0; 3];
    for index in 0..3 {
        let mut replaced = *cols;
        replaced[index] = rhs;
        out[index] = det_of(&replaced) / determinant;
    }
    Some(out)
}

/// **校准原色**（LR 校准面板式薄封装）：红/绿/蓝原色色相 ± / 饱和度 → 3×3 矩阵，
/// 交给 `channel_mixer`（常数零）；显示空间，不动内核混合 math。
///
/// 构造（设计文档 `d1c-DESIGN.md` §2，全 f64、末尾转 f32）：
/// - 色相矩阵 H：CIE xy 色度面、D65 白点；每原色相对白点的色度向量旋转
///   θ = 0.5 × hue slider（1 单位 = 0.5°）；亮度求解后得新原色矩阵 B，
///   H = A⁻¹·B（A 为标准 sRGB 矩阵）。H 精确守白（行和 = 1）。
/// - 饱和度矩阵 S：色相取 0，只动饱和度；`f = 1 + sat/100` 缩放色度向量，
///   列 = 固定亮度 × 线性新原色再 sRGB 编码；`sat = −100` ⇒ 该原色塌到亮度灰。
/// - T = S·H（先转色相、后调饱和）。全零参数短路返回精确单位阵。
pub fn calibrate_primaries_matrix(params: &Value) -> [[f32; 3]; 3] {
    const IDENTITY: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    const IDENTITY_64: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    // sRGB 原色 xy 与 D65 白点；原色亮度；白点 XYZ（与 `color.rs` 同组常数）。
    const PRIMARIES: [(f64, f64); 3] = [(0.640, 0.330), (0.300, 0.600), (0.150, 0.060)];
    const WHITE: (f64, f64) = (0.3127, 0.3290);
    const WHITE_XYZ: [f64; 3] = [0.95047, 1.0, 1.08883];
    const LUMINANCE: [f64; 3] = [0.2126, 0.7152, 0.0722];
    const XYZ_TO_RGB: [[f64; 3]; 3] = [
        [3.2406, -1.5372, -0.4986],
        [-0.9689, 1.8758, 0.0415],
        [0.0557, -0.2040, 1.0570],
    ];
    let num = |key: &str| params.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let (red_hue, red_sat, green_hue, green_sat, blue_hue, blue_sat) = (
        num("red_hue"),
        num("red_sat"),
        num("green_hue"),
        num("green_sat"),
        num("blue_hue"),
        num("blue_sat"),
    );
    if red_hue == 0.0
        && red_sat == 0.0
        && green_hue == 0.0
        && green_sat == 0.0
        && blue_hue == 0.0
        && blue_sat == 0.0
    {
        return IDENTITY;
    }
    // 色相矩阵 H（色相全零时短路为单位阵，避免浮点往返误差）。
    let hue_matrix = if red_hue == 0.0 && green_hue == 0.0 && blue_hue == 0.0 {
        IDENTITY_64
    } else {
        let mut xyz_cols = [[0.0f64; 3]; 3];
        for (index, &(px, py)) in PRIMARIES.iter().enumerate() {
            let theta = [red_hue, green_hue, blue_hue][index] * 0.5 * std::f64::consts::PI / 180.0;
            let (dx, dy) = (px - WHITE.0, py - WHITE.1);
            let (sin, cos) = (libm::sin(theta), libm::cos(theta));
            let (rx, ry) = (dx * cos - dy * sin, dx * sin + dy * cos);
            let (nx, ny) = (WHITE.0 + rx, WHITE.1 + ry);
            let (nx, ny) = if ny.abs() < 1e-9 { (px, py) } else { (nx, ny) };
            xyz_cols[index] = [nx / ny, 1.0, (1.0 - nx - ny) / ny];
        }
        let luminance = solve_3x3_cols(&xyz_cols, WHITE_XYZ).unwrap_or(LUMINANCE);
        let mut hue = [[0.0f64; 3]; 3];
        for col in 0..3 {
            let b = [
                luminance[col] * xyz_cols[col][0],
                luminance[col] * xyz_cols[col][1],
                luminance[col] * xyz_cols[col][2],
            ];
            for row in 0..3 {
                hue[row][col] = XYZ_TO_RGB[row][0] * b[0]
                    + XYZ_TO_RGB[row][1] * b[1]
                    + XYZ_TO_RGB[row][2] * b[2];
            }
        }
        hue
    };
    // 饱和度矩阵 S（饱和度全零时短路为单位阵；列向量形式）。
    let sat_cols = if red_sat == 0.0 && green_sat == 0.0 && blue_sat == 0.0 {
        IDENTITY_64
    } else {
        let mut cols = [[0.0f64; 3]; 3];
        for (index, &(px, py)) in PRIMARIES.iter().enumerate() {
            let factor = 1.0 + [red_sat, green_sat, blue_sat][index] / 100.0;
            let (dx, dy) = (px - WHITE.0, py - WHITE.1);
            let (nx, ny) = (WHITE.0 + factor * dx, WHITE.1 + factor * dy);
            let (nx, ny) = if ny.abs() < 1e-9 { (px, py) } else { (nx, ny) };
            let xyz = [nx / ny, 1.0, (1.0 - nx - ny) / ny];
            for row in 0..3 {
                let linear = LUMINANCE[index]
                    * (XYZ_TO_RGB[row][0] * xyz[0]
                        + XYZ_TO_RGB[row][1] * xyz[1]
                        + XYZ_TO_RGB[row][2] * xyz[2]);
                cols[index][row] = srgb_encode_signed(linear);
            }
        }
        cols
    };
    // T = S·H（S 以列向量给出：S[row][k] = sat_cols[k][row]）。
    let mut matrix = [[0.0f32; 3]; 3];
    for row in 0..3 {
        for col in 0..3 {
            matrix[row][col] = (sat_cols[0][row] * hue_matrix[0][col]
                + sat_cols[1][row] * hue_matrix[1][col]
                + sat_cols[2][row] * hue_matrix[2][col]) as f32;
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

/// 解析透视四边形参数（4 个 `[x, y]`；失败返回 None ⇒ 工具层先校验，这里再拒 ✓）。
fn quad_param(params: &Value) -> Option<[[f32; 2]; 4]> {
    let quad = params.get("quad")?.as_array()?;
    if quad.len() != 4 {
        return None;
    }
    let mut out = [[0.0f32; 2]; 4];
    for (i, point) in quad.iter().enumerate() {
        let pair = point.as_array()?;
        if pair.len() != 2 {
            return None;
        }
        for (j, value) in pair.iter().enumerate() {
            out[i][j] = value.as_f64()? as f32;
        }
    }
    Some(out)
}

/// 应用一个调整对象；返回是否被识别。
/// `canvas` 是整幅画面的文档尺寸（几何类调整 lens/perspective 按它解释参数，
/// 与 `apply_filter` 的同名参数同口径 ✓）。
pub fn apply_adjustment(
    buffer: &mut Buffer,
    adjustment_type: &str,
    params: &Value,
    opacity: f32,
    canvas: (f64, f64),
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
            // 期 3 A2 新增 amount：缺省 1.0（与 split_toning 同口径，钉成断言）。
            params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32,
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
        AdjustmentKind::Hsl => {
            let hue = params.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let saturation = params
                .get("saturation")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32;
            let lightness = params
                .get("lightness")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32;
            // 期 4 D2-3：无 `ranges` 键 ⇒ 走现行标量路，逐位不变（分支在分发处
            // 判断，不许新代码路径碰老行为）；有 ⇒ 走显示编码空间的逐通道路。
            match params.get("ranges").and_then(Value::as_object) {
                None => hsl(&mut adjusted, hue, saturation, lightness),
                Some(ranges_obj) => {
                    let mut ranges = [HslRangeOffset::default(); 8];
                    for (index, name) in HSL_RANGE_NAMES.iter().enumerate() {
                        if let Some(range) = ranges_obj.get(*name).and_then(Value::as_object) {
                            ranges[index] = HslRangeOffset {
                                hue: range.get("hue").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                                saturation: range
                                    .get("saturation")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                                lightness: range
                                    .get("lightness")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(0.0)
                                    as f32,
                            };
                        }
                    }
                    hsl_with_ranges(&mut adjusted, hue, saturation, lightness, &ranges)
                }
            }
        }
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
            // 无 amount 键的老文档 = 1.0 = 现行像素（存量零影响）；越界由函数内钳制。
            let amount = params.get("amount").and_then(Value::as_f64).unwrap_or(1.0) as f32;
            // D2-2b：无 `ranges` 键 ⇒ 走现行三权路，逐位不变（分支在分发处
            // 判断，不许新代码路径碰老行为）；有 ⇒ 走 8 档权重路。
            match bw_ranges(params) {
                None => black_and_white(&mut adjusted, bw_weights(params), tint, amount),
                Some(ranges) => black_and_white_with_ranges(
                    &mut adjusted,
                    bw_weights(params),
                    tint,
                    amount,
                    &ranges,
                ),
            }
        }
        AdjustmentKind::ChannelMixer => {
            let constants = match fixed_floats(params.get("constants"), 3) {
                Some(values) => [values[0], values[1], values[2]],
                None => [0.0, 0.0, 0.0],
            };
            channel_mixer(&mut adjusted, mixer_matrix(params), constants)
        }
        AdjustmentKind::CalibratePrimaries => {
            channel_mixer(&mut adjusted, calibrate_primaries_matrix(params), [0.0; 3])
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
        AdjustmentKind::RedEye => red_eye(
            &mut adjusted,
            params
                .get("strength")
                .and_then(Value::as_f64)
                .unwrap_or(1.0) as f32,
            params
                .get("threshold")
                .and_then(Value::as_f64)
                .unwrap_or(2.0) as f32,
        ),
        AdjustmentKind::Denoise => denoise(
            &mut adjusted,
            params
                .get("luminance")
                .and_then(Value::as_f64)
                .unwrap_or(0.5) as f32,
            params.get("detail").and_then(Value::as_f64).unwrap_or(0.5) as f32,
            params.get("color").and_then(Value::as_f64).unwrap_or(0.5) as f32,
        ),
        AdjustmentKind::Lens => lens_correction(
            &mut adjusted,
            params.get("k1").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("k2").and_then(Value::as_f64).unwrap_or(0.0) as f32,
            params.get("tca_r").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params.get("tca_b").and_then(Value::as_f64).unwrap_or(1.0) as f32,
            params
                .get("vignette_strength")
                .and_then(Value::as_f64)
                .unwrap_or(0.0) as f32,
            params
                .get("auto_crop")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            canvas,
        ),
        AdjustmentKind::Perspective => {
            let Some(quad) = quad_param(params) else {
                return false;
            };
            if !perspective_warp(
                &mut adjusted,
                quad,
                params.get("k1").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                params.get("k2").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                params
                    .get("auto_crop")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                canvas,
            ) {
                return false;
            }
        }
        AdjustmentKind::Dehaze => dehaze_adjustment(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Clarity => clarity_adjustment(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
        AdjustmentKind::Texture => texture_adjustment(
            &mut adjusted,
            params.get("amount").and_then(Value::as_f64).unwrap_or(0.0) as f32,
        ),
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
/// **∴ P2-14**：测试也要用它搭串行对照核 ⇒ 门控放宽到 `gpu 或 test`（生产
/// 非 gpu 构建仍不编译它，不留死代码）。
#[cfg(any(feature = "gpu", test))]
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

    /// 期 3 A1 改写（原 `brightness_and_contrast_are_linear_in_straight_space`
    /// 断言的是旧线性公式 `(c·k + b).clamp(0,1)`，语义已被甲案原地替换）：
    /// 新断言钉**显示枢轴口径**——中灰刻度探针、枢轴不动点、HDR 不钳、
    /// 亮度比回写的通道比恒等（hue 防护）。变异：改回线性公式 ⇒ 刻度
    /// 探针（旧式同输入给 +0.468 显示位移）与 HDR 探针立刻红。
    #[test]
    fn brightness_contrast_uses_display_pivot_semantics() {
        // neutral 恒等（含 HDR 与负值：扩展传递往返不钳）。
        let mut buffer = filled([1.8, 0.4, -0.2, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 1.0);
        let pixel = buffer.pixel(0, 0);
        assert!(
            (pixel[0] - 1.8).abs() < 1e-5 && (pixel[2] + 0.2).abs() < 1e-5,
            "{pixel:?}"
        );
        // 刻度探针：显示中灰 0.5（线性 ≈0.2140）+ brightness 0.5 ⇒ 显示 0.75。
        let mut buffer = filled([0.214_041_14, 0.214_041_14, 0.214_041_14, 1.0]);
        brightness_contrast(&mut buffer, 0.5, 1.0);
        let shown = srgb_encode_extended(buffer.pixel(0, 0)[0]);
        assert!((shown - 0.75).abs() < 0.01, "中灰显示应 +0.25：{shown}");
        // 枢轴不动点：显示 0.5 在纯对比下不动；显示 0.75、contrast 0.5 ⇒ 0.625。
        let mut buffer = filled([0.214_041_14, 0.214_041_14, 0.214_041_14, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 2.0);
        let shown = srgb_encode_extended(buffer.pixel(0, 0)[0]);
        assert!((shown - 0.5).abs() < 0.01, "枢轴 0.5 应不动：{shown}");
        let mut buffer = filled([0.522_521_74, 0.522_521_74, 0.522_521_74, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 0.5);
        let shown = srgb_encode_extended(buffer.pixel(0, 0)[0]);
        assert!(
            (shown - 0.625).abs() < 0.01,
            "0.75 绕枢轴减半应得 0.625：{shown}"
        );
        // HDR 不钳 + 色相防护：(4, 1, 0.25) 加对比后 R 仍 > 1 且通道比不变。
        let mut buffer = filled([4.0, 1.0, 0.25, 1.0]);
        brightness_contrast(&mut buffer, 0.0, 1.5);
        let pixel = buffer.pixel(0, 0);
        assert!(pixel[0] > 4.5, "HDR 高光不得被钳到 1：{pixel:?}");
        assert!(
            (pixel[1] / pixel[0] - 0.25).abs() < 1e-3
                && (pixel[2] / pixel[0] - 0.0625).abs() < 1e-3,
            "亮度比回写应保持通道比（色相不扭）：{pixel:?}"
        );
    }

    /// A1 双路一致：调整路径与滤镜路径同参数必须逐像素一致（同一函数）。
    #[test]
    fn brightness_contrast_adjustment_and_filter_paths_agree() {
        let mut via_adjustment = filled([0.3, 0.6, 1.4, 1.0]);
        let mut via_filter = filled([0.3, 0.6, 1.4, 1.0]);
        let params = json!({"brightness": 0.3, "contrast": 1.4});
        apply_adjustment(
            &mut via_adjustment,
            "brightness_contrast",
            &params,
            1.0,
            (4.0, 4.0),
        );
        apply_filter(
            &mut via_filter,
            "brightness_contrast",
            &params,
            1.0,
            (4.0, 4.0),
        );
        assert_eq!(via_adjustment.pixel(0, 0), via_filter.pixel(0, 0));
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
                apply_adjustment(&mut buffer, name, &params, 1.0, (2.0, 1.0)),
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
        apply_adjustment(
            &mut buffer,
            "exposure",
            &json!({"ev": 1.0}),
            1.0,
            (2.0, 1.0),
        );
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

    /// A4-2 显示刻度探针（设计 §2 A4-2）：乘子在显示空间施加——均匀
    /// 显示中灰 0.5 图、角落满衰减（t=1）且 strength=0.3（乘子 0.7）时
    /// 角落显示输出 = 0.5 × 0.7 = 0.35 ± 0.01。旧线性乘子同输入给
    /// enc(0.2140×0.7) ≈ 0.430 ⇒ 变异回线性立刻红。中心（t=0）逐位不动。
    #[test]
    fn vignette_multiplies_in_display_space() {
        let mut buffer = Buffer::new(0, 0, 4, 4);
        let lin = srgb_to_linear(0.5);
        for y in 0..4 {
            for x in 0..4 {
                buffer.set_pixel(x, y, [lin, lin, lin, 1.0]);
            }
        }
        vignette(&mut buffer, 0.3, 0.2, 0.6, (4.0, 4.0));
        let corner = srgb_encode_extended(buffer.pixel(0, 0)[0]);
        assert!(
            (corner - 0.35).abs() < 0.01,
            "角落显示输出应 = 0.35（显示乘子 0.7）：{corner}"
        );
        let center = buffer.pixel(2, 2);
        assert_eq!(center, [lin, lin, lin, 1.0], "中心 t=0 应逐位不动");
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

    /// A4-3 中性灰反例（专家 Q5 的回归）：大面积暗中性前景 + 小块亮暖色
    /// 区域（误导大气光估计偏暖）+ 中性大气光真雾。旧实现下估计 air
    /// (0.875, 0.825, 0.725) 未经阻尼、t 无 γ 重映射，暗前景恢复成
    /// ≈(0, 0.056, 0.176)——中性面被染成蓝色（R/B 比塌到 0）。四修正后
    /// 恢复应近中性：R/B ∈ [0.95, 1.05]、且不被压到 0。
    #[test]
    fn dehaze_neutral_foreground_is_not_cast_blue() {
        let air_true = [0.85f32, 0.85, 0.85];
        let mut buffer = Buffer::new(0, 0, 32, 8);
        for y in 0..8 {
            for x in 0..32 {
                let scene = if x < 24 {
                    [0.10, 0.10, 0.10] // 暗中性前景
                } else {
                    [0.90, 0.80, 0.60] // 亮暖色块（误导估计）
                };
                buffer.set_pixel(
                    x,
                    y,
                    [
                        scene[0] * 0.5 + air_true[0] * 0.5,
                        scene[1] * 0.5 + air_true[1] * 0.5,
                        scene[2] * 0.5 + air_true[2] * 0.5,
                        1.0,
                    ],
                );
            }
        }
        dehaze_adjustment(&mut buffer, 1.0);
        let out = buffer.pixel(4, 4);
        assert!(
            out[0] > 0.15 && out[2] > 0.15,
            "暗前景不应被压到近 0（旧实现 R 通道 ≈0）：{out:?}"
        );
        let ratio = out[0] / out[2];
        assert!(
            (0.95..=1.05).contains(&ratio),
            "中性前景恢复后 R/B 应回到 [0.95, 1.05]：{ratio}（{out:?}）"
        );
    }

    /// A4-3 修正一的直接断言：dehaze 内部先阻尼大气光 ⇒ 传原始色度
    /// air 与传手工阻尼后的 air，恢复输出应一致（容差 2e-3：阻尼在
    /// 阻尼后的点上再作用有 ~5e-4 的不动点漂移）。变异：去掉内部
    /// 阻尼 ⇒ 两路输出按 1/t 放大分叉 ⇒ 红。
    #[test]
    fn dehaze_damps_air_chromaticity_internally() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 32, 8);
            for y in 0..8 {
                for x in 0..32 {
                    let v = if x < 16 { 0.45f32 } else { 0.62 };
                    buffer.set_pixel(x, y, [v, v, v, 1.0]);
                }
            }
            buffer
        };
        let air_raw = [0.90, 0.80, 0.625];
        // 阻尼结果按公式手算写死（luma = 0.8086，比值 1.113/0.989/0.772
        // 钳后 1.05/原值/0.95）——不能调用生产 helper 算，否则变异
        // helper 时两路同变、断言测不到（变异验证实测教训）。
        let air_damped = [0.849_056, 0.80, 0.768_194];
        let mut via_raw = make();
        dehaze(&mut via_raw, air_raw, 0.9, 0.1);
        let mut via_damped = make();
        dehaze(&mut via_damped, air_damped, 0.9, 0.1);
        for x in [4u32, 28] {
            let a = via_raw.pixel(x, 4);
            let b = via_damped.pixel(x, 4);
            for channel in 0..3 {
                assert!(
                    (a[channel] - b[channel]).abs() < 2e-3,
                    "内部阻尼应使两路一致（x={x} 通道 {channel}）：{a:?} vs {b:?}"
                );
            }
        }
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

    /// **名字路由**（2026-10-21）：`apply_adjustment("vibrance")` 必须走 Vibrance、
    /// 不许再被 Saturation 偷换。判据用两条恒等/非恒等关系，不写死像素：
    /// `saturation(amount=1)` 是恒等（作对照基线），`vibrance(amount=1)` 对低饱和像素非恒等，
    /// 且规范名与旧别名 `vibrance_natural` 的结果逐位相同。
    #[test]
    fn vibrance_name_routes_to_vibrance_not_saturation() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 1, 1);
            buffer.set_pixel(0, 0, [0.45, 0.5, 0.55, 1.0]);
            buffer
        };
        let original = make().pixel(0, 0);
        let mut via_canonical = make();
        assert!(apply_adjustment(
            &mut via_canonical,
            "vibrance",
            &json!({"amount": 1.0}),
            1.0,
            (1.0, 1.0)
        ));
        assert_ne!(
            via_canonical.pixel(0, 0),
            original,
            "vibrance(amount=1) 对低饱和像素应非恒等（若与原图相同，多半又被偷换成 saturation 恒等）"
        );
        let mut via_alias = make();
        assert!(apply_adjustment(
            &mut via_alias,
            "vibrance_natural",
            &json!({"amount": 1.0}),
            1.0,
            (1.0, 1.0)
        ));
        assert_eq!(
            via_canonical.pixel(0, 0),
            via_alias.pixel(0, 0),
            "规范名 vibrance 与别名 vibrance_natural 必须逐位一致"
        );
        let mut via_saturation = make();
        assert!(apply_adjustment(
            &mut via_saturation,
            "saturation",
            &json!({"amount": 1.0}),
            1.0,
            (1.0, 1.0)
        ));
        assert_eq!(
            via_saturation.pixel(0, 0),
            original,
            "saturation(amount=1) 应恒等（对照基线）"
        );
    }

    /// 分离色调：暗部取阴影色、亮部取高光色；`balance` 移动分界。
    /// 期 5 Q1-c 语义：权重取自 sRGB Y'，着色后线性亮度恢复——故暗部偏蓝
    /// 只断方向（恢复把蓝拉回，量级不再是旧线性加色的 +0.2），亮度本身断恢复。
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
        let luma = |px: [f32; 4]| 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
        // 阴影加蓝、高光加黄。
        let mut toned = make();
        split_toning(&mut toned, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 0.0, 1.0);
        let dark = toned.pixel(0, 0);
        let bright = toned.pixel(2, 0);
        assert!(dark[2] > dark[0], "暗部应偏蓝：{dark:?}");
        assert!(
            (luma(dark) - 0.05).abs() < 0.005,
            "暗部亮度应恢复：{} vs 0.05",
            luma(dark)
        );
        // 高光在该 Y' 下为满权（显示 0.80 → t 触顶），通道差约 0.32。
        assert!(bright[0] > bright[2] + 0.1, "亮部应偏黄：{bright:?}");
        assert!(
            (luma(bright) - 0.6).abs() < 0.005,
            "亮部亮度应恢复：{} vs 0.6",
            luma(bright)
        );

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

    /// 期 5 Q1-c：权重取自 sRGB Y' 而非线性亮度——线性中灰 0.5（Y' ≈ 0.735）
    /// 在新口径下落入高光侧（黄胜），旧口径下两色均分（R ≈ B）。
    /// 变异：权重改回线性亮度 ⇒ R ≈ B ⇒ 红。
    #[test]
    fn split_toning_weights_follow_display_luma() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        buffer.set_pixel(0, 0, [0.5, 0.5, 0.5, 1.0]);
        split_toning(&mut buffer, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 0.0, 1.0);
        let px = buffer.pixel(0, 0);
        assert!(px[0] > px[2] + 0.05, "Y' 权重下中灰应取高光黄：{px:?}");
    }

    /// 期 5 Q1-c：亮度恢复——纯黑像素着色后仍为黑（亮度 0 乘任何比仍为 0）。
    /// 变异：去掉亮度恢复 ⇒ 黑被阴影色染蓝 ⇒ 红。
    #[test]
    fn split_toning_black_stays_black() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        buffer.set_pixel(0, 0, [0.0, 0.0, 0.0, 1.0]);
        split_toning(&mut buffer, [0.0, 0.0, 0.3], [0.3, 0.3, 0.0], 0.0, 1.0);
        let px = buffer.pixel(0, 0);
        assert_eq!(
            [px[0], px[1], px[2]],
            [0.0, 0.0, 0.0],
            "黑像素不应被着色：{px:?}"
        );
    }

    /// 色彩平衡（期 3 A2 改写：期望按新显示口径公式重算；旧断言钉的是
    /// 线性分档 + 线性全幅相加 + 钳 [0,1]，其中「蓝推到饱和 1.0」一条
    /// 正是旧钳制行为——新口径下高光蓝越过 1.0 保留 HDR，不再饱和到白）。
    /// 阴影档只影响暗部、高光档只影响亮部，中间调影响两者之间。
    #[test]
    fn color_balance_affects_tone_bands_separately() {
        let make = || {
            let mut buffer = Buffer::new(0, 0, 3, 1);
            buffer.set_pixel(0, 0, [0.05, 0.05, 0.05, 1.0]); // 暗
            buffer.set_pixel(1, 0, [0.5, 0.5, 0.5, 1.0]); // 中
            buffer.set_pixel(2, 0, [0.95, 0.95, 0.95, 1.0]); // 亮
            buffer
        };
        // 只加阴影档的红：暗部提升最多，亮部几乎不变（增量按新公式手算：
        // 暗 ≈0.0185、中 ≈0.0081、亮 ≈0.0001，量级远小于旧线性全幅相加）。
        let mut shadows = make();
        color_balance(&mut shadows, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3], 1.0);
        let dark_delta = shadows.pixel(0, 0)[0] - 0.05;
        let mid_delta = shadows.pixel(1, 0)[0] - 0.5;
        let bright_delta = shadows.pixel(2, 0)[0] - 0.95;
        assert!(dark_delta > 0.012, "暗部应被显著提升：{dark_delta}");
        assert!(
            dark_delta > mid_delta && mid_delta > bright_delta,
            "阴影档增量应随亮度递减：{dark_delta}/{mid_delta}/{bright_delta}"
        );
        assert!(bright_delta < 0.002, "亮部几乎不变：{bright_delta}");

        // 只加高光档的蓝：亮部提升最多，且**越过 1.0**（显示施加不钳）。
        let mut highlights = make();
        color_balance(&mut highlights, [0.0; 3], [0.0; 3], [0.0, 0.0, 0.3], 1.0);
        let bright_blue = highlights.pixel(2, 0)[2];
        let dark_blue_delta = highlights.pixel(0, 0)[2] - 0.05;
        assert!(
            bright_blue > 1.05,
            "亮部高光档蓝应越过 1.0 保留 HDR（旧实现钳到 1.0）：{bright_blue}"
        );
        assert!(dark_blue_delta < 0.005, "暗部几乎不变：{dark_blue_delta}");

        // 中间调 + 越界参数不应产出非法值（本组输入下输出仍在 [0,1]）。
        let mut extremes = make();
        color_balance(
            &mut extremes,
            [9.0, -9.0, 0.0],
            [1.0; 3],
            [0.0, 0.0, -9.0],
            1.0,
        );
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

    /// A2 amount：显示域偏移随 amount 线性缩放（0.5 ⇒ 显示位移恰半），
    /// amount = 0 恒等；分发缺省 amount 与显式 1.0 逐像素一致（缺省钉死）。
    #[test]
    fn color_balance_amount_scales_display_offset_and_defaults_to_one() {
        let mut full = filled([0.05, 0.05, 0.05, 1.0]);
        let mut half = filled([0.05, 0.05, 0.05, 1.0]);
        color_balance(&mut full, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3], 1.0);
        color_balance(&mut half, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3], 0.5);
        let base = srgb_encode_extended(0.05);
        let full_shift = srgb_encode_extended(full.pixel(0, 0)[0]) - base;
        let half_shift = srgb_encode_extended(half.pixel(0, 0)[0]) - base;
        assert!(
            (half_shift - full_shift * 0.5).abs() < 1e-4,
            "amount 0.5 的显示位移应恰为全量一半：{half_shift} vs {full_shift}"
        );
        let mut zero = filled([0.05, 0.4, 0.9, 1.0]);
        color_balance(&mut zero, [0.3, 0.0, 0.0], [0.0; 3], [0.0; 3], 0.0);
        assert_eq!(zero.pixel(0, 0), [0.05, 0.4, 0.9, 1.0], "amount=0 必须恒等");
        // 分发缺省：不带 amount 的 JSON 与显式 amount=1 逐像素一致。
        let mut implicit = filled([0.2, 0.5, 0.8, 1.0]);
        let mut explicit = filled([0.2, 0.5, 0.8, 1.0]);
        apply_adjustment(
            &mut implicit,
            "color_balance",
            &json!({"shadows": [0.3, 0.0, 0.0]}),
            1.0,
            (4.0, 4.0),
        );
        apply_adjustment(
            &mut explicit,
            "color_balance",
            &json!({"shadows": [0.3, 0.0, 0.0], "amount": 1.0}),
            1.0,
            (4.0, 4.0),
        );
        assert_eq!(
            implicit.pixel(0, 0),
            explicit.pixel(0, 0),
            "缺省 amount 必须 = 1.0"
        );
    }

    /// A2 HDR：显示值 > 1 的通道偏移按显示刻度足额施加（旧实现先钳 1.0
    /// 再加线性偏移，相对位移被腰斩且结果 ≤ 1）。变异：施加前加回
    /// clamp(0,1) ⇒ 本断言红。
    #[test]
    fn color_balance_applies_full_offset_to_hdr_pixels() {
        let mut buffer = filled([0.8, 0.8, 3.0, 1.0]);
        color_balance(&mut buffer, [0.0; 3], [0.0; 3], [0.0, 0.0, -0.4], 1.0);
        let blue = buffer.pixel(0, 0)[2];
        assert!(
            blue > 2.0,
            "HDR 蓝减偏移后应仍为 HDR（新公式 ≈2.70）：{blue}"
        );
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

    /// 期 2 A1 改写：黑白点是**显示空间**值——输入以显示值 0.25/0.5/0.75
    /// 给（线性 `srgb_to_linear(d)`），`levels(0.25, 0.75, 1.0)` 后显示值
    /// 应为 0/0.5/1。旧线性期望（线性 0.25→0）在此输入下不再成立。
    #[test]
    fn levels_maps_black_and_white_points() {
        let gray = |display: f32| {
            let linear = srgb_to_linear(display);
            [linear, linear, linear, 1.0]
        };
        let mut buffer = Buffer::new(0, 0, 3, 1);
        buffer.set_pixel(0, 0, gray(0.25));
        buffer.set_pixel(1, 0, gray(0.5));
        buffer.set_pixel(2, 0, gray(0.75));
        levels(&mut buffer, 0.25, 0.75, 1.0);
        assert!(buffer.pixel(0, 0)[0].abs() < 1e-5);
        assert!((buffer.pixel(1, 0)[0] - srgb_to_linear(0.5)).abs() < 1e-5);
        assert!((buffer.pixel(2, 0)[0] - 1.0).abs() < 1e-5);
    }

    /// **作用空间（回归，期 2 A1）**：显示意图 `black=0.06` 必须把显示 0.06
    /// 压成黑、而显示 0.27 只压到 `(0.27−0.06)/(1−0.06)`。旧线性口径下
    /// 显示 0.27（线性 0.059）会被同一个 0.06 黑点压成 0 ⇒ 红。
    #[test]
    fn levels_black_point_is_in_display_space() {
        let gray = |display: f32| {
            let linear = srgb_to_linear(display);
            [linear, linear, linear, 1.0]
        };
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, gray(0.06));
        buffer.set_pixel(1, 0, gray(0.27));
        levels(&mut buffer, 0.06, 1.0, 1.0);
        assert!(
            buffer.pixel(0, 0)[0].abs() < 1e-5,
            "显示黑点 0.06 应压成黑，实得 {:?}",
            buffer.pixel(0, 0)
        );
        let expected = srgb_to_linear((0.27 - 0.06) / (1.0 - 0.06));
        assert!(
            (buffer.pixel(1, 0)[0] - expected).abs() < 1e-4,
            "显示 0.27 过 black=0.06 应为线性 {expected:.4}，实得 {:?}",
            buffer.pixel(1, 0)
        );
    }

    /// HDR/负值边界与旧线性版一致：高于白点 ⇒ 白点、低于黑点 ⇒ 黑点
    ///（编码前不钳，但归一化钳 `[0, 1]` 后再解码）。
    #[test]
    fn levels_clamps_out_of_range_display_values() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, [2.0, 2.0, 2.0, 1.0]);
        buffer.set_pixel(1, 0, [-0.5, -0.5, -0.5, 1.0]);
        levels(&mut buffer, 0.0, 1.0, 1.0);
        assert!(
            (buffer.pixel(0, 0)[0] - 1.0).abs() < 1e-5,
            "HDR 高光应钳到白点，实得 {:?}",
            buffer.pixel(0, 0)
        );
        assert!(
            buffer.pixel(1, 0)[0].abs() < 1e-5,
            "负值应钳到黑点，实得 {:?}",
            buffer.pixel(1, 0)
        );
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
            1.0,
            (4.0, 4.0)
        ));
        // 期 3 A1 改写：旧期望 0.6 是旧线性公式（0.5 + 0.1）；新显示口径下
        // 同输入 = dec(enc(0.5) + 0.05) ≈ 0.5795（灰像素上亮度比回写与
        // 逐通道显示映射等价）。
        assert!((buffer.pixel(0, 0)[0] - 0.579_5).abs() < 1e-3);
        assert!(!apply_adjustment(
            &mut buffer,
            "unknown",
            &json!({}),
            1.0,
            (4.0, 4.0)
        ));
        assert!(apply_filter(
            &mut buffer,
            "invert",
            &json!({}),
            1.0,
            (2.0, 1.0)
        ));
        // 期 3 A1 连带改写：invert 作用在上面 bc 的结果上——旧链 1 − 0.6 = 0.4，
        // 新链 1 − 0.5795 = 0.4205（invert 自身语义未变）。
        assert!((buffer.pixel(0, 0)[0] - 0.420_5).abs() < 1e-3);
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

    #[test]
    fn red_eye_corrects_red_eye_pixel_only() {
        // **红眼型像素**（R 远高于 G/B）⇒ R 压到 max(G,B)，G/B 不动 ✓。
        let mut buffer = filled([0.85, 0.08, 0.06, 1.0]);
        red_eye(&mut buffer, 1.0, 2.0);
        let p = buffer.pixel(0, 0);
        assert!((p[0] - 0.08).abs() < 0.01, "「R 应压到 0.08：{p:?}」");
        assert_eq!(p[1], 0.08, "「G 不动」");
        assert_eq!(p[2], 0.06, "「B 不动」");
        // **皮肤与砖红**：R 高但 G/B 不低 ⇒ 一个不动 ✓（误伤红物体是这个调整的头号事故 ✗）。
        for color in [[0.76, 0.57, 0.45], [0.6, 0.25, 0.15], [0.75, 0.2, 0.25]] {
            let mut buffer = filled([color[0], color[1], color[2], 1.0]);
            red_eye(&mut buffer, 1.0, 2.0);
            assert_eq!(
                buffer.pixel(0, 0)[..3],
                color,
                "「非红眼红色应原样：{color:?} → {:?}」",
                buffer.pixel(0, 0)
            );
        }
    }

    #[test]
    fn red_eye_strength_zero_is_identity_and_half_is_halfway() {
        // strength=0 ⇒ 恒等 ✓（哪怕红眼型像素 ✓）。
        let mut buffer = filled([0.85, 0.08, 0.06, 1.0]);
        red_eye(&mut buffer, 0.0, 2.0);
        assert_eq!(buffer.pixel(0, 0)[0], 0.85, "「strength=0 应恒等」");
        // strength=0.5 ⇒ 压制量减半 ✓（线性插值 ✓）。
        let mut buffer = filled([0.85, 0.08, 0.06, 1.0]);
        red_eye(&mut buffer, 0.5, 2.0);
        let p = buffer.pixel(0, 0);
        assert!(
            (p[0] - (0.85 + 0.5 * (0.08 - 0.85))).abs() < 0.01,
            "「半强度应走一半：{p:?}」"
        );
    }

    #[test]
    fn adjustment_kind_names_roundtrip_with_red_eye() {
        // `red_eye` 进了 ALL 与 ADJUSTMENT_NAMES（全量往返由 coloradj_tests 的
        // `adjustment_kind_names_roundtrip` 覆盖，这里只钉新增这一项，避免同一事实写两处 ✓）。
        assert_eq!(
            AdjustmentKind::from_name("red_eye"),
            Some(AdjustmentKind::RedEye),
            "「red_eye 应解析为 RedEye」"
        );
        assert_eq!(
            AdjustmentKind::RedEye.as_str(),
            "red_eye",
            "「RedEye.as_str 应为 red_eye」"
        );
        assert!(
            ADJUSTMENT_NAMES.contains(&"red_eye"),
            "「ADJUSTMENT_NAMES 应包含 red_eye」"
        );
    }

    /// P2-14：并行高斯卷积与串行逐位相同（分带/快照不改任何字节）。
    ///
    /// 两臂：① 生产入口 `gaussian_blur`（大图上走并行分带）对串行两向卷积；
    /// ② 行带函数本身（手动切两带，不依赖核数 ⇒ 单核机器上也验得到带内数学）。
    /// 变异：任一带函数改累加顺序/边界钳制 ⇒ 逐位比较立刻红。
    #[test]
    fn gaussian_blur_parallel_matches_serial_bitwise() {
        // 256×192（49152 像素 ≥ PARALLEL_MIN_PIXELS），内容含渐变、硬边、HDR 与半透明。
        let (width, height) = (256u32, 192u32);
        let mut source = Buffer::new(0, 0, width, height);
        for y in 0..height {
            for x in 0..width {
                let edge = if (x / 32) % 2 == 0 { 1.6 } else { 0.05 };
                let alpha = if (y / 48) % 2 == 0 { 1.0 } else { 0.5 };
                source.set_pixel(
                    x,
                    y,
                    [
                        (x as f32 / width as f32) * edge,
                        (y as f32 / height as f32) * 0.9,
                        0.3 + 0.2 * ((x + y) % 7) as f32,
                        alpha,
                    ],
                );
            }
        }
        let sigma = 8.0f32;
        let kernel = gaussian_kernel(sigma).expect("sigma=8 应有核");
        let radius = ((kernel.len() - 1) / 2) as i32;
        // ① 生产入口对串行。
        let mut via_entry = source.clone();
        gaussian_blur(&mut via_entry, sigma);
        let mut via_serial = source.clone();
        convolve_horizontal_serial(&mut via_serial, &kernel, radius);
        convolve_vertical_serial(&mut via_serial, &kernel, radius);
        assert_eq!(
            via_entry.pixels(),
            via_serial.pixels(),
            "并行高斯模糊必须与串行逐位相同"
        );
        // ② 行带函数本身（手动切两带）。
        let mut via_bands = source.clone();
        let row_len = width as usize * 4;
        let split_row = height as usize / 2;
        {
            let pixels = via_bands.pixels_mut();
            let (top, bottom) = pixels.split_at_mut(split_row * row_len);
            convolve_horizontal_band(top, width as usize, &kernel, radius);
            convolve_horizontal_band(bottom, width as usize, &kernel, radius);
        }
        let snapshot = via_bands.pixels().to_vec();
        {
            let pixels = via_bands.pixels_mut();
            let (top, bottom) = pixels.split_at_mut(split_row * row_len);
            convolve_vertical_band(
                top,
                &snapshot,
                width as usize,
                height as usize,
                0,
                &kernel,
                radius,
            );
            convolve_vertical_band(
                bottom,
                &snapshot,
                width as usize,
                height as usize,
                split_row,
                &kernel,
                radius,
            );
        }
        assert_eq!(
            via_bands.pixels(),
            via_serial.pixels(),
            "手动两带卷积必须与串行逐位相同"
        );
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
        black_and_white(&mut buffer, [1.0, 0.0, 0.0], None, 1.0);
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
            black_and_white(&mut buffer, weights, None, 1.0);
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
            1.0,
        );
        assert_rgb_close(px(&buffer), [0.5, 0.25, 0.0], "白经 tint 后应等于 tint");
    }

    /// **amount = 0 逐位恒等**（期 4 D2-2）：混合方向反了 ⇒ 这里给黑白值而非输入 ⇒ 红。
    #[test]
    fn bw_amount_zero_is_identity() {
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [0.2126, 0.7152, 0.0722], None, 0.0);
        assert_eq!(px(&buffer), [0.7, 0.2, 0.1], "amount=0 必须逐位等于输入");
    }

    /// **amount = 1 逐位等于现行实现输出**（期 4 D2-2 金样本）：
    /// Rec709 灰阶 0.2126·0.7 + 0.7152·0.2 + 0.0722·0.1 = 0.29908。
    #[test]
    fn bw_amount_one_matches_legacy() {
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [0.2126, 0.7152, 0.0722], None, 1.0);
        assert_rgb_close(
            px(&buffer),
            [0.29908, 0.29908, 0.29908],
            "amount=1 应等于旧输出",
        );
    }

    /// **amount = 0.5 为中点**（期 4 D2-2）：输出 = (输入 + 旧输出) / 2 ± 浮点容差。
    #[test]
    fn bw_amount_half_is_midpoint() {
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [0.2126, 0.7152, 0.0722], None, 0.5);
        assert_rgb_close(
            px(&buffer),
            [0.49954, 0.24954, 0.19954],
            "amount=0.5 应为输入与黑白结果的中点",
        );
    }

    /// **amount 越界钳制**（期 4 D2-2）：2.0 ⇒ 1.0、−1.0 ⇒ 0.0。
    #[test]
    fn bw_amount_out_of_range_is_clamped() {
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [0.2126, 0.7152, 0.0722], None, 2.0);
        assert_rgb_close(
            px(&buffer),
            [0.29908, 0.29908, 0.29908],
            "amount=2 应钳为 1",
        );
        let mut buffer = filled([0.7, 0.2, 0.1, 1.0]);
        black_and_white(&mut buffer, [0.2126, 0.7152, 0.0722], None, -1.0);
        assert_eq!(px(&buffer), [0.7, 0.2, 0.1], "amount=−1 应钳为 0");
    }

    /// **无 `ranges` 键 ⇒ 走现行三权路，逐位不变**（D2-2b 金样本回归）：
    /// 分发路径与直接调 `black_and_white` 的结果必须逐位相等。
    #[test]
    fn bw_ranges_absent_bit_identical() {
        let mut via_direct = filled([0.31, 0.57, 0.83, 1.0]);
        black_and_white(&mut via_direct, [0.2126, 0.7152, 0.0722], None, 1.0);
        let mut via_dispatch = filled([0.31, 0.57, 0.83, 1.0]);
        let applied = apply_adjustment(
            &mut via_dispatch,
            "black_and_white",
            &json!({"weights": [0.2126, 0.7152, 0.0722]}),
            1.0,
            (4.0, 4.0),
        );
        assert!(applied, "black_and_white 调整必须被分发识别");
        assert_eq!(
            via_dispatch.pixel(0, 0),
            via_direct.pixel(0, 0),
            "无 ranges 时分发路径必须逐位等于现行三权路"
        );
        assert_eq!(
            via_dispatch.pixel(3, 3),
            via_direct.pixel(3, 3),
            "无 ranges 时分发路径必须逐位等于现行三权路（角像素）"
        );
    }

    /// **reds 档只动红块**（D2-2b）：红块（编码色相 0°）灰度被压、绿块
    /// （编码色相 120°）逐位不变。变异：权重窗写成全宽 ⇒ 绿块被动 ⇒ 红；
    /// 实现忽略 ranges ⇒ 红块不动 ⇒ 红。
    #[test]
    fn bw_range_reds_only_moves_reds() {
        let mut ranges = [1.0f32; 8];
        ranges[0] = 0.0; // reds
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, [0.6, 0.05, 0.05, 1.0]);
        buffer.set_pixel(1, 0, [0.05, 0.6, 0.05, 1.0]);
        black_and_white_with_ranges(&mut buffer, [0.2126, 0.7152, 0.0722], None, 1.0, &ranges);
        // 红块：基础灰 0.16693 × 乘子 (1 − 编码饱和度 0.577) ≈ 0.071。
        let red = buffer.pixel(0, 0);
        assert!(
            (red[0] - 0.071).abs() < 0.01,
            "红块灰度应压到约 0.071，实得 {red:?}"
        );
        assert_eq!(red[0], red[1], "红块输出必须三通道同值");
        assert_eq!(red[1], red[2], "红块输出必须三通道同值");
        // 绿块：乘子恒 1 ⇒ 逐位等于现行三权路。
        let mut legacy = Buffer::new(0, 0, 1, 1);
        legacy.set_pixel(0, 0, [0.05, 0.6, 0.05, 1.0]);
        black_and_white(&mut legacy, [0.2126, 0.7152, 0.0722], None, 1.0);
        assert_eq!(
            buffer.pixel(1, 0),
            legacy.pixel(0, 0),
            "绿块在 reds=0 下必须逐位等于现行输出"
        );
        // 分发 leg：经 apply_adjustment 的 ranges 与直接调用逐位一致。
        let mut via_dispatch = Buffer::new(0, 0, 2, 1);
        via_dispatch.set_pixel(0, 0, [0.6, 0.05, 0.05, 1.0]);
        via_dispatch.set_pixel(1, 0, [0.05, 0.6, 0.05, 1.0]);
        let applied = apply_adjustment(
            &mut via_dispatch,
            "black_and_white",
            &json!({"ranges": {"reds": 0.0}}),
            1.0,
            (2.0, 1.0),
        );
        assert!(applied, "black_and_white 调整必须被分发识别");
        assert_eq!(
            via_dispatch.pixel(0, 0),
            buffer.pixel(0, 0),
            "分发 reds=0 必须与直接调用逐位一致"
        );
        assert_eq!(
            via_dispatch.pixel(1, 0),
            buffer.pixel(1, 0),
            "分发 reds=0 必须与直接调用逐位一致（绿块）"
        );
    }

    /// **档间边界混合**（D2-2b）：编码色相 15°（红/橙正中）的像素，
    /// reds=0 与 oranges=1 各半权重 ⇒ 档混合 0.5、乘子 0.5（编码饱和度 1），
    /// 灰度为基础灰的一半。变异：改成硬分档 ⇒ 乘子为 0 或 1 ⇒ 红。
    #[test]
    fn bw_range_boundary_blends() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        // 线性 (1.0, 0.05087, 0.0) 编码后为 (1.0, 0.25, 0.0)，色相恰 15°（照 D2-3）。
        buffer.set_pixel(0, 0, [1.0, 0.05087, 0.0, 1.0]);
        let mut ranges = [1.0f32; 8];
        ranges[0] = 0.0; // reds（oranges 保持 1.0）
        black_and_white_with_ranges(&mut buffer, [0.2126, 0.7152, 0.0722], None, 1.0, &ranges);
        // 基础灰 0.2126 + 0.7152×0.05087 ≈ 0.24898，乘子 0.5 ⇒ 约 0.124。
        let out = buffer.pixel(0, 0);
        assert!(
            (out[0] - 0.124).abs() < 0.01,
            "边界像素灰度应为基础灰的一半（约 0.124），实得 {out:?}"
        );
    }

    /// **灰像素对任意 ranges 不动**（D2-2b）：灰像素饱和度为 0 ⇒ 乘子恒 1。
    /// 变异：乘子不乘饱和度 ⇒ 灰像素吃 reds 权重 ⇒ 红。
    #[test]
    fn bw_ranges_gray_pixels_unaffected() {
        let mut ranges = [1.0f32; 8];
        ranges[0] = 0.0; // reds
        ranges[3] = 2.0; // greens
        ranges[5] = 0.0; // blues
        let mut buffer = filled([0.42, 0.42, 0.42, 1.0]);
        black_and_white_with_ranges(&mut buffer, [0.2126, 0.7152, 0.0722], None, 1.0, &ranges);
        assert_eq!(
            px(&buffer),
            [0.42, 0.42, 0.42],
            "灰像素在任意 ranges 下都必须逐位不动"
        );
    }

    /// **全档 1.0 ⇒ 与现行输出在 1e-5 内一致**（D2-2b）：档混合恒 1、
    /// 乘子恒 1；三角窗归一化求和有浮点噪声级漂移，故不断言逐位。
    #[test]
    fn bw_ranges_all_ones_near_identical() {
        let mut via_ranges = filled([0.31, 0.57, 0.83, 1.0]);
        black_and_white_with_ranges(
            &mut via_ranges,
            [0.2126, 0.7152, 0.0722],
            None,
            1.0,
            &[1.0; 8],
        );
        let mut via_legacy = filled([0.31, 0.57, 0.83, 1.0]);
        black_and_white(&mut via_legacy, [0.2126, 0.7152, 0.0722], None, 1.0);
        assert_rgb_close(
            px(&via_ranges),
            px(&via_legacy),
            "全档 1.0 应与现行输出一致（1e-5 内）",
        );
    }

    /// **ranges 路的 tint 与 amount 顺序与现行一致**（D2-2b，收尾共用）：
    /// 红块 reds=0 ⇒ 灰约 0.071，tint [0.5, 1.0, 0.5] ⇒ [0.035, 0.071, 0.035]，
    /// amount=0.5 ⇒ 与输入的中点（手算常数，容差 ±0.001）。
    #[test]
    fn bw_ranges_compose_with_tint_and_amount() {
        let mut ranges = [1.0f32; 8];
        ranges[0] = 0.0; // reds
        let mut buffer = filled([0.6, 0.05, 0.05, 1.0]);
        black_and_white_with_ranges(
            &mut buffer,
            [0.2126, 0.7152, 0.0722],
            Some([0.5, 1.0, 0.5]),
            0.5,
            &ranges,
        );
        let out = px(&buffer);
        let expected = [0.3177, 0.0604, 0.0427];
        for channel in 0..3 {
            assert!(
                (out[channel] - expected[channel]).abs() < 0.001,
                "tint + amount 在 ranges 路应与现行同顺序，通道 {channel}：期望 {}，实得 {}",
                expected[channel],
                out[channel]
            );
        }
    }

    // ── hsl ranges ────────────────────────────────────────────────

    /// **无 `ranges` 键 ⇒ 走现行标量路，逐位不变**（期 4 D2-3 金样本回归）：
    /// 分发路径（opacity = 1 时直接写回）与直接调 `hsl` 的结果必须逐位相等。
    #[test]
    fn hsl_ranges_absent_bit_identical() {
        let mut via_scalar = filled([0.31, 0.57, 0.83, 1.0]);
        hsl(&mut via_scalar, 20.0, 1.3, 0.1);
        let mut via_dispatch = filled([0.31, 0.57, 0.83, 1.0]);
        let applied = apply_adjustment(
            &mut via_dispatch,
            "hsl",
            &json!({"hue": 20.0, "saturation": 1.3, "lightness": 0.1}),
            1.0,
            (4.0, 4.0),
        );
        assert!(applied, "hsl 调整必须被分发识别");
        assert_eq!(
            via_dispatch.pixel(0, 0),
            via_scalar.pixel(0, 0),
            "无 ranges 时分发路径必须逐位等于标量路"
        );
        assert_eq!(
            via_dispatch.pixel(3, 3),
            via_scalar.pixel(3, 3),
            "无 ranges 时分发路径必须逐位等于标量路（角像素）"
        );
    }

    /// **reds 档只动红块**（期 4 D2-3）：红块（编码色相 0°）饱和上升、
    /// 绿块（编码色相 120°）逐位不变。变异：权重窗写成全宽 ⇒ 绿块被动 ⇒ 红；
    /// 实现忽略 ranges ⇒ 红块不动 ⇒ 红。
    #[test]
    fn hsl_range_reds_only_moves_reds() {
        let mut buffer = Buffer::new(0, 0, 2, 1);
        buffer.set_pixel(0, 0, [0.6, 0.05, 0.05, 1.0]);
        buffer.set_pixel(1, 0, [0.05, 0.6, 0.05, 1.0]);
        let mut ranges = [HslRangeOffset::default(); 8];
        ranges[0] = HslRangeOffset {
            hue: 0.0,
            saturation: 50.0,
            lightness: 0.0,
        };
        hsl_with_ranges(&mut buffer, 0.0, 1.0, 0.0, &ranges);
        // 红块：编码饱和度应从约 0.575 升到约 0.863（×1.5）。
        let red = buffer.pixel(0, 0);
        let red_enc = [
            srgb_encode_extended(red[0]),
            srgb_encode_extended(red[1]),
            srgb_encode_extended(red[2]),
        ];
        let (_, red_s, _) = hsl_decompose(&red_enc);
        assert!(
            (red_s - 0.863).abs() < 0.02,
            "红块饱和度应升到约 0.863，实测 {red_s}"
        );
        // 绿块：逐位不变。
        assert_eq!(
            buffer.pixel(1, 0),
            [0.05, 0.6, 0.05, 1.0],
            "绿块在 reds 档下必须逐位不变"
        );
    }

    /// **档间边界混合**（期 4 D2-3）：编码色相 15°（红/橙正中）的像素，
    /// reds +30° 与 oranges +10° 各半权重 ⇒ 偏移为均值 +20°，输出色相 35°±2°。
    /// 变异：改成硬分档 ⇒ 偏移为 30° 或 10° ⇒ 红。
    #[test]
    fn hsl_range_boundary_blends() {
        let mut buffer = Buffer::new(0, 0, 1, 1);
        // 线性 (1.0, 0.05087, 0.0) 编码后为 (1.0, 0.25, 0.0)，色相恰 15°。
        buffer.set_pixel(0, 0, [1.0, 0.05087, 0.0, 1.0]);
        let mut ranges = [HslRangeOffset::default(); 8];
        ranges[0] = HslRangeOffset {
            hue: 30.0,
            saturation: 0.0,
            lightness: 0.0,
        };
        ranges[1] = HslRangeOffset {
            hue: 10.0,
            saturation: 0.0,
            lightness: 0.0,
        };
        hsl_with_ranges(&mut buffer, 0.0, 1.0, 0.0, &ranges);
        let out = buffer.pixel(0, 0);
        let out_enc = [
            srgb_encode_extended(out[0]),
            srgb_encode_extended(out[1]),
            srgb_encode_extended(out[2]),
        ];
        let (hue, _, _) = hsl_decompose(&out_enc);
        let hue_deg = hue * 360.0;
        assert!(
            (hue_deg - 35.0).abs() < 2.0,
            "边界像素色相应为 35°±2°，实测 {hue_deg}°"
        );
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

    // ── calibrate_primaries ─────────────────────────────────────────────

    /// 文档值断言的容差：设计文档 §2.4/§4 的钉死值是 3 位小数（原型 f64 算出），
    /// 与实现 f32 值的差在 2e-3 内；构造改动会让它们动 0.01 以上，故 2e-3 既不误报
    /// 也不放过真回归。
    fn assert_rgb_close_doc(actual: [f32; 3], expected: [f32; 3], label: &str) {
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 2e-3,
                "{label} 通道 {channel}：期望 {}，实得 {}",
                expected[channel],
                actual[channel]
            );
        }
    }

    /// 全零参数 ⇒ 逐位单位阵（短路；恒等渲染的根基）。
    #[test]
    fn calibrate_primaries_zero_params_is_exact_identity() {
        let matrix = calibrate_primaries_matrix(&json!({}));
        assert_eq!(
            matrix,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            "全零参数必须逐位单位阵"
        );
        let matrix = calibrate_primaries_matrix(&json!({
            "red_hue": 0.0,
            "red_sat": 0.0,
            "green_hue": 0.0,
            "green_sat": 0.0,
            "blue_hue": 0.0,
            "blue_sat": 0.0,
        }));
        assert_eq!(
            matrix,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            "显式全零也必须逐位单位阵"
        );
    }

    /// 只动色相 ⇒ 每行行和 = 1（白精确守恒），含全部 ±100 档与组合极端；
    /// 矩阵元有限且有界（max|T| ≤ 3）。变异：删掉亮度求解 ⇒ 行和立刻偏离 1。
    #[test]
    fn calibrate_primaries_hue_only_preserves_white() {
        for params in [
            json!({"red_hue": 100.0}),
            json!({"blue_hue": -100.0}),
            json!({"red_hue": 25.0, "blue_hue": -100.0}),
            json!({"red_hue": 100.0, "green_hue": 100.0, "blue_hue": 100.0}),
            json!({"red_hue": -100.0, "green_hue": -100.0, "blue_hue": -100.0}),
        ] {
            let matrix = calibrate_primaries_matrix(&params);
            for (row, values) in matrix.iter().enumerate() {
                let sum: f32 = values.iter().sum();
                assert!(
                    (sum - 1.0).abs() < 1e-3,
                    "只动色相时第 {row} 行行和应为 1，实得 {sum}（{params:?}）"
                );
                for value in values {
                    assert!(value.is_finite(), "矩阵元必须有限：{value}（{params:?}）");
                    assert!(value.abs() <= 3.0, "矩阵元应有界 ≤3：{value}（{params:?}）");
                }
            }
        }
    }

    /// 只动饱和度 ⇒ 蓝 sat −100 时纯蓝变亮度灰、纯红/纯绿不动；白出钉死文档值。
    /// 变异：饱和度改走亮度求解 ⇒ 纯红变黑（设计文档 §2.3 否决的 (A) 构造）。
    #[test]
    fn calibrate_primaries_blue_sat_minus_100_grays_blue() {
        let matrix = calibrate_primaries_matrix(&json!({"blue_sat": -100.0}));
        // 白出（= 行和）：设计文档 §2.4 的文档值。
        let white_out: [f32; 3] = [
            matrix[0].iter().sum(),
            matrix[1].iter().sum(),
            matrix[2].iter().sum(),
        ];
        assert_rgb_close_doc(white_out, [1.298, 1.299, 0.298], "蓝 sat −100 的白出");
        let mut blue = filled([0.0, 0.0, 1.0, 1.0]);
        channel_mixer(&mut blue, matrix, [0.0; 3]);
        assert_rgb_close_doc(px(&blue), [0.298, 0.298, 0.298], "纯蓝应变亮度灰");
        let mut red = filled([1.0, 0.0, 0.0, 1.0]);
        channel_mixer(&mut red, matrix, [0.0; 3]);
        assert_rgb_close_doc(px(&red), [1.0, 0.0, 0.0], "纯红应不动");
        let mut green = filled([0.0, 1.0, 0.0, 1.0]);
        channel_mixer(&mut green, matrix, [0.0; 3]);
        assert_rgb_close_doc(px(&green), [0.0, 1.0, 0.0], "纯绿应不动");
        // sat −50 的白出同样钉死。
        let matrix = calibrate_primaries_matrix(&json!({"blue_sat": -50.0}));
        let white_out: [f32; 3] = [
            matrix[0].iter().sum(),
            matrix[1].iter().sum(),
            matrix[2].iter().sum(),
        ];
        assert_rgb_close_doc(white_out, [1.274, 1.275, 0.502], "蓝 sat −50 的白出");
    }

    /// 红/绿 sat −100 同理：纯红塌到 enc(0.2126) 灰、纯绿塌到 enc(0.7152) 灰
    /// （每个声明的参数都有一条判据证明它被读过）。
    #[test]
    fn calibrate_primaries_red_and_green_sat_minus_100_gray_their_primary() {
        let matrix = calibrate_primaries_matrix(&json!({"red_sat": -100.0}));
        let mut red = filled([1.0, 0.0, 0.0, 1.0]);
        channel_mixer(&mut red, matrix, [0.0; 3]);
        assert_rgb_close_doc(px(&red), [0.4984, 0.4984, 0.4984], "纯红应变亮度灰");
        let matrix = calibrate_primaries_matrix(&json!({"green_sat": -100.0}));
        let mut green = filled([0.0, 1.0, 0.0, 1.0]);
        channel_mixer(&mut green, matrix, [0.0; 3]);
        assert_rgb_close_doc(px(&green), [0.8625, 0.8625, 0.8625], "纯绿应变亮度灰");
    }

    /// 色相方向（sat = 0）：蓝 hue −100 ⇒ 蓝列 XYZ 色度角 ≈188.7°；
    /// 红 hue +25 ⇒ 红列 ≈12.8°（LR 方向约定：负值色相角减小）。
    /// 变异：旋转方向写反 ⇒ 两角各偏约 100°/25°，立刻红。
    #[test]
    fn calibrate_primaries_hue_direction_matches_lr_convention() {
        // 标准 sRGB 矩阵 A：列向量 → XYZ。
        const RGB_TO_XYZ: [[f64; 3]; 3] = [
            [0.4124, 0.3576, 0.1805],
            [0.2126, 0.7152, 0.0722],
            [0.0193, 0.1192, 0.9505],
        ];
        let col_angle = |matrix: [[f32; 3]; 3], col: usize| -> f64 {
            let c = [
                matrix[0][col] as f64,
                matrix[1][col] as f64,
                matrix[2][col] as f64,
            ];
            let xyz = [
                RGB_TO_XYZ[0][0] * c[0] + RGB_TO_XYZ[0][1] * c[1] + RGB_TO_XYZ[0][2] * c[2],
                RGB_TO_XYZ[1][0] * c[0] + RGB_TO_XYZ[1][1] * c[1] + RGB_TO_XYZ[1][2] * c[2],
                RGB_TO_XYZ[2][0] * c[0] + RGB_TO_XYZ[2][1] * c[1] + RGB_TO_XYZ[2][2] * c[2],
            ];
            let sum = xyz[0] + xyz[1] + xyz[2];
            let (x, y) = (xyz[0] / sum, xyz[1] / sum);
            let degrees = libm::atan2(y - 0.3290, x - 0.3127) * 180.0 / std::f64::consts::PI;
            (degrees + 360.0) % 360.0
        };
        let blue = calibrate_primaries_matrix(&json!({"blue_hue": -100.0}));
        let angle = col_angle(blue, 2);
        assert!(
            (angle - 188.7).abs() < 2.0,
            "蓝 hue −100 的色度角应为 188.7°±2°，实测 {angle}°"
        );
        let red = calibrate_primaries_matrix(&json!({"red_hue": 25.0}));
        let angle = col_angle(red, 0);
        assert!(
            (angle - 12.8).abs() < 2.0,
            "红 hue +25 的色度角应为 12.8°±2°，实测 {angle}°"
        );
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

    /// 显示亮度（Rec.709 权重，与 `shadows_highlights` 内同一口径）。
    fn display_luma(px: [f32; 3]) -> f32 {
        0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2]
    }

    /// 相对饱和 `(max−min)/mean`（与 `shadows_highlights_preserves_chroma_ratios` 同口径）。
    fn rel_sat_of(px: [f32; 3]) -> f32 {
        let max = px[0].max(px[1]).max(px[2]);
        let min = px[0].min(px[1]).min(px[2]);
        (max - min) / ((px[0] + px[1] + px[2]) / 3.0)
    }

    /// 合成色卡像素（显示空间）：给定显示亮度与相对饱和，构造 R 高 B 低、
    /// G 为中值的三通道像素。rel_sat = 2p、luma = m·(1 + 0.1404p)，
    /// 其中 0.1404 = 0.2126 − 0.0722（Rec.709 权重差）。
    fn card_pixel(luma: f32, rel_sat: f32) -> [f32; 3] {
        let p = rel_sat / 2.0;
        let m = luma / (1.0 + 0.1404 * p);
        [(1.0 + p) * m, m, (1.0 - p) * m]
    }

    /// 对单个显示空间像素跑 `shadows_highlights`，返回显示空间输出。
    fn run_sh_display(display: [f32; 3], shadows: f32, highlights: f32) -> [f32; 3] {
        let linear = [
            srgb_to_linear(display[0]),
            srgb_to_linear(display[1]),
            srgb_to_linear(display[2]),
        ];
        let mut buffer = filled([linear[0], linear[1], linear[2], 1.0]);
        shadows_highlights(&mut buffer, shadows, highlights);
        let out = px(&buffer);
        [
            linear_to_srgb(out[0]),
            linear_to_srgb(out[1]),
            linear_to_srgb(out[2]),
        ]
    }

    /// 旧亮度比回写（期 2 A2 原式，供剂量矩阵对照）：显示空间等比缩放。
    /// 纯黑回退逐通道 toward，与现行一致。
    fn ratio_scaled(display: [f32; 3], shadows: f32, highlights: f32) -> [f32; 3] {
        let luma = display_luma(display);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let target = (luma
            + toward(shadows, shadow_weight, luma)
            + toward(highlights, highlight_weight, luma))
        .clamp(0.0, 1.0);
        if luma > 0.0 {
            let ratio = target / luma;
            [
                (display[0] * ratio).clamp(0.0, 1.0),
                (display[1] * ratio).clamp(0.0, 1.0),
                (display[2] * ratio).clamp(0.0, 1.0),
            ]
        } else {
            let mut out = display;
            for value in out.iter_mut() {
                let current = *value;
                let mut next = current + toward(shadows, shadow_weight, current);
                next += toward(highlights, highlight_weight, current);
                *value = next.clamp(0.0, 1.0);
            }
            out
        }
    }

    /// 软拐点回写（magenta-fix P6 现行式，供品红调制对照）：显示空间按
    /// soft_knee_gap(ratio) 缩放通道与亮度的差距；纯黑回退与现行一致。
    fn soft_knee_scaled(display: [f32; 3], shadows: f32, highlights: f32) -> [f32; 3] {
        let luma = display_luma(display);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let highlight_weight = luma * luma;
        let target = (luma
            + toward(shadows, shadow_weight, luma)
            + toward(highlights, highlight_weight, luma))
        .clamp(0.0, 1.0);
        if luma > 0.0 {
            let ratio = target / luma;
            let gap = soft_knee_gap(ratio);
            [
                (target + (display[0] - luma) * gap).clamp(0.0, 1.0),
                (target + (display[1] - luma) * gap).clamp(0.0, 1.0),
                (target + (display[2] - luma) * gap).clamp(0.0, 1.0),
            ]
        } else {
            let mut out = display;
            for value in out.iter_mut() {
                let current = *value;
                let mut next = current + toward(shadows, shadow_weight, current);
                next += toward(highlights, highlight_weight, current);
                *value = next.clamp(0.0, 1.0);
            }
            out
        }
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

    /// **作用空间（回归）**：缓冲是线性光、本函数的数学按显示值标定 ⇒
    /// 显示值 0.5（线性 `srgb_to_linear(0.5)`）过 `shadows=0.6` 后，**显示值**必须等于
    /// 单次公式 `0.5 + 0.6×(1−0.5)²×(1−0.5)×0.5 = 0.5375`（±0.004）。
    /// 基线（在线性域直接算）给出线性 ≈0.360 ⇒ 显示 ≈0.634，与期望差 0.097 ⇒ 红。
    #[test]
    fn shadows_highlights_math_is_in_display_space() {
        let linear = srgb_to_linear(0.5);
        let mut buffer = filled([linear, linear, linear, 1.0]);
        shadows_highlights(&mut buffer, 0.6, 0.0);
        let display = linear_to_srgb(px(&buffer)[0]);
        assert!(
            (display - 0.5375).abs() <= 0.004,
            "显示值应为单次公式 0.5375，实得 {display}"
        );
    }

    /// **亮度比回写（回归，期 2 A2）**：彩色像素过 s_h 后通道比与相对饱和
    /// 不变，只有明度按原曲线提升。旧逐通道 toward 下相对饱和会从 1.5 跌
    /// 到约 0.85 ⇒ 红。探针：显示 (0.5, 0.2, 0.1)，shadows=+0.6、
    /// highlights=−0.6 双向同开（目标亮度 0.2566→0.3748，比值 1.46）。
    #[test]
    fn shadows_highlights_preserves_chroma_ratios() {
        let display = [0.5f32, 0.2, 0.1];
        let linear = [
            srgb_to_linear(display[0]),
            srgb_to_linear(display[1]),
            srgb_to_linear(display[2]),
        ];
        let rel_sat = |px: [f32; 3]| {
            let max = px[0].max(px[1]).max(px[2]);
            let min = px[0].min(px[1]).min(px[2]);
            (max - min) / ((px[0] + px[1] + px[2]) / 3.0)
        };
        let mut buffer = filled([linear[0], linear[1], linear[2], 1.0]);
        shadows_highlights(&mut buffer, 0.6, -0.6);
        let out = px(&buffer);
        let out_display = [
            linear_to_srgb(out[0]),
            linear_to_srgb(out[1]),
            linear_to_srgb(out[2]),
        ];
        // 通道比守恒：R:B = 5、G:B = 2 不变（容差 1e-3）。
        assert!(
            (out_display[0] / out_display[2] - 5.0).abs() < 1e-3,
            "R:B 应守恒为 5，实得 {:?}",
            out_display
        );
        assert!(
            (out_display[1] / out_display[2] - 2.0).abs() < 1e-3,
            "G:B 应守恒为 2，实得 {:?}",
            out_display
        );
        // 相对饱和守恒（输入 1.5）。
        assert!(
            (rel_sat(out_display) - 1.5).abs() < 1e-3,
            "相对饱和应守恒为 1.5，实得 {}（{:?}）",
            rel_sat(out_display),
            out_display
        );
        // 明度确实动了（不是恒等）：显示亮度应从 0.2566 提到约 0.3748。
        let luma = |px: [f32; 3]| 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
        assert!(
            (luma(out_display) - 0.3748).abs() < 1e-3,
            "显示亮度应提到约 0.3748，实得 {}",
            luma(out_display)
        );
    }

    /// **软拐点 gap 四档钉死（magenta-fix，设计文档 §4.3(a)）**：
    /// r=1.5/2.0/2.456/4.429 ⇒ g=1.50/1.7917/1.9041/2.0650（容差 1e-3）。
    /// 注：设计文档 §2.5 写的 "2.07" 是 2.065 的两位小数显示取整，测试钉精确值。
    #[test]
    fn shadows_highlights_soft_knee_gap_four_grades() {
        for (ratio, expected) in [
            (1.5f32, 1.5f32),
            (2.0, 1.7917),
            (2.456, 1.9041),
            (4.429, 2.0650),
        ] {
            let got = soft_knee_gap(ratio);
            assert!(
                (got - expected).abs() < 1e-3,
                "r={ratio} 时 gap 应为 {expected}，实得 {got}"
            );
        }
    }

    /// **拐点以下逐位一致**：ratio ≤ 1.5 时 gap 恒等于 ratio（同一值返回），
    /// 含压暗侧（r<1）——软拐点只动提亮侧，不动压暗侧与高光侧。
    #[test]
    fn shadows_highlights_soft_knee_gap_is_identity_below_knee() {
        for ratio in [0.0f32, 0.3, 0.9, 1.0, 1.2, 1.5] {
            assert_eq!(
                soft_knee_gap(ratio),
                ratio,
                "r={ratio} ≤ 1.5 时 gap 必须恒等于 ratio"
            );
        }
    }

    /// **C¹ 连续（magenta-fix，设计文档 §2.5）**：ratio=1.5 处左导数应为 1.0
    ///（线性区；容差 1e-4 覆盖 f32 表示误差——1.5−h 本身的舍入使割线
    /// 偏离 1.0 至多 0.5·ulp(1.5)/h ≈ 6e-5，实测 4.7e-5）；右割线在
    /// h=1e-3 时为 0.99857，与 1.0 的差 ≈ |g''|·h/2 ≈ 1.43e-3
    ///（g''(1.5+) = −2·0.7²/0.7³ ≈ −2.86），极限为 1.0。
    /// 硬 min(2.0) 在此处导数从 1 突变为 0 ⇒ 该测试会红。
    #[test]
    fn shadows_highlights_soft_knee_is_c1_at_knee() {
        let h = 1e-3f32;
        let left = (soft_knee_gap(1.5) - soft_knee_gap(1.5 - h)) / h;
        assert!(
            (left - 1.0).abs() < 1e-4,
            "左导数应为 1.0（容差 1e-4，f32 表示误差），实得 {left}"
        );
        let right = (soft_knee_gap(1.5 + h) - soft_knee_gap(1.5)) / h;
        assert!(
            (right - 1.0).abs() < 2e-3,
            "右割线（h=1e-3）应在 1.0 附近（实得 {right}）；极限 1.0"
        );
    }

    /// **品红调制因子值（A2 后续 ③）**：t(m) 在 m ≤ 0 时恒为 1、m = M0 处恰为 0.5、
    /// m = 2·M0 处为 0.2、m ≫ M0 时趋近 0。若实现漏掉 m ≤ 0 分支（对负 m 也套
    /// 平方公式），t(−0.01) 会变成 0.2 而非 1 ⇒ 此处红。
    #[test]
    fn shadows_highlights_magenta_gap_factor_values() {
        let m0 = SHADOWS_HIGHLIGHTS_MAGENTA_M0;
        for (m, expected) in [
            (-0.01f32, 1.0f32),
            (0.0, 1.0),
            (m0, 0.5),
            (2.0 * m0, 0.2),
            (10.0 * m0, 1.0 / 101.0),
        ] {
            let got = magenta_gap_factor(m);
            assert!(
                (got - expected).abs() < 1e-6,
                "t({m}) 应为 {expected}，实得 {got}"
            );
        }
    }

    /// **m ≤ 0 逐位一致（A2 后续 ③）**：红/灰像素（m ≤ 0）在 ratio > 1.5 的提亮侧，
    /// 输出与未调制的软拐点回写逐位一致（容差 1e-5，覆盖 f32 的 sRGB 往返噪声）。
    /// 每组至少要有一个像素真正参与对照（空集恒真防线）；若实现对 m<0 也调制，
    /// 红像素输出会偏离 ⇒ 此处红。
    #[test]
    fn shadows_highlights_magenta_modulation_is_identity_for_nonpositive_m() {
        let probes = [
            [0.05f32, 0.03, 0.02],
            [0.06, 0.03, 0.02],
            [0.05, 0.05, 0.05],
            [0.10, 0.04, 0.02],
        ];
        let mut checked = 0;
        for display in probes {
            let luma = display_luma(display);
            let shadow_weight = (1.0 - luma) * (1.0 - luma);
            let target = (luma + toward(0.4, shadow_weight, luma)).clamp(0.0, 1.0);
            let ratio = target / luma;
            assert!(ratio > 1.5, "探针像素 ratio 应 > 1.5，实得 {ratio}");
            let magenta = (display[0] - display[1]).min(display[2] - display[1]);
            assert!(magenta <= 0.0, "探针像素 m 应 ≤ 0，实得 {magenta}");
            let got = run_sh_display(display, 0.4, 0.0);
            let want = soft_knee_scaled(display, 0.4, 0.0);
            for channel in 0..3 {
                assert!(
                    (got[channel] - want[channel]).abs() < 1e-5,
                    "像素 {display:?}：m≤0 时应与软拐点回写逐位一致，通道 {channel} 实得 {} 期望 {}",
                    got[channel],
                    want[channel]
                );
            }
            checked += 1;
        }
        assert!(checked > 0, "一个像素都没对照上 ⇒ 测试恒真无效");
    }

    /// **品红像素增益收回（A2 后续 ③）**：品红向像素 [0.10, 0.05, 0.09]
    ///（m = +0.04 ≫ M0，t ≈ 0.015），shadows=+0.4 下输出的通道离差应不到
    /// 未调制的软拐点回写的六成（gap_eff ≈ 1.016 vs gap ≈ 2.02，离差比
    /// = gap_eff/gap ≈ 0.50；门限取 0.6 留 f32 余量），而输出亮度仍等于
    /// target_luma（容差 1e-3）。若实现漏掉调制，离差与对照相等 ⇒ 此处红。
    #[test]
    fn shadows_highlights_magenta_pixel_lift_reduced() {
        let display = [0.10f32, 0.05, 0.09];
        let luma = display_luma(display);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let target = (luma + toward(0.4, shadow_weight, luma)).clamp(0.0, 1.0);
        let ratio = target / luma;
        assert!(ratio > 1.5, "探针像素 ratio 应 > 1.5，实得 {ratio}");
        let got = run_sh_display(display, 0.4, 0.0);
        let want = soft_knee_scaled(display, 0.4, 0.0);
        let spread = |px: [f32; 3]| px[0].max(px[1]).max(px[2]) - px[0].min(px[1]).min(px[2]);
        assert!(
            spread(got) < 0.6 * spread(want),
            "品红像素离差应不到对照的六成，实得 {}（对照 {}）",
            spread(got),
            spread(want)
        );
        let out_luma = display_luma(got);
        assert!(
            (out_luma - target).abs() < 1e-3,
            "输出亮度应等于 target_luma {target}，实得 {out_luma}"
        );
    }

    /// **提亮量守恒（magenta-fix §4.2 的单元层）**：ratio>R0 的暗彩像素，
    /// 输出显示亮度等于 target_luma（容差 1e-3；逐通道 [0,1] 钳制除外，
    /// 本探针三通道均不触顶）。
    #[test]
    fn shadows_highlights_luma_conserved_above_knee() {
        let display = [0.06f32, 0.03, 0.02];
        let luma = display_luma(display);
        let shadow_weight = (1.0 - luma) * (1.0 - luma);
        let target = (luma + toward(0.4, shadow_weight, luma)).clamp(0.0, 1.0);
        let ratio = target / luma;
        assert!(ratio > 1.5, "探针像素 ratio 应 > 1.5，实得 {ratio}");
        let out = run_sh_display(display, 0.4, 0.0);
        assert!(
            out.iter().all(|v| (0.0..=1.0).contains(v)),
            "探针像素不得触顶钳制，实得 {out:?}"
        );
        let out_luma = display_luma(out);
        assert!(
            (out_luma - target).abs() < 1e-3,
            "输出亮度应等于 target_luma {target}，实得 {out_luma}"
        );
    }

    /// **shadows≤0 逐位一致（magenta-fix §4.3(a)）**：shadows≤0 时 ratio≤1，
    /// gap=ratio 数学恒等——输出与旧亮度比回写逐位一致（容差 1e-5，
    /// 覆盖 f32 的 sRGB 往返噪声）。若实现误把软拐点用到压暗侧，此处红。
    #[test]
    fn shadows_highlights_nonpositive_shadows_matches_main() {
        for display in [
            [0.5f32, 0.2, 0.1],
            [0.05, 0.03, 0.02],
            [0.9, 0.7, 0.5],
            [0.2, 0.2, 0.2],
        ] {
            for (shadows, highlights) in [(-0.4f32, 0.0), (-1.0, 0.0), (0.0, -0.5), (-0.2, -0.2)] {
                let got = run_sh_display(display, shadows, highlights);
                let want = ratio_scaled(display, shadows, highlights);
                for channel in 0..3 {
                    assert!(
                        (got[channel] - want[channel]).abs() < 1e-5,
                        "({shadows},{highlights}) 像素 {display:?}：shadows≤0 应与现行逐位一致，\
                         通道 {channel} 实得 {} 期望 {}",
                        got[channel],
                        want[channel]
                    );
                }
            }
        }
    }

    /// **剂量矩阵（magenta-fix §4.3(a) Q6）**：六组 (shadows, highlights)，
    /// 凡使探针像素 ratio ≤ 1.5，输出与旧亮度比回写（期 2 A2 原式）一致
    ///（容差 1e-5，覆盖 f32 的 sRGB 往返噪声；数学上恒等）。
    /// 含压暗侧 (−0.4,0) 与 (0,−0.5)——软拐点只动提亮侧。
    /// 每组至少要有一个像素真正参与对照（空集恒真防线）。
    #[test]
    fn shadows_highlights_dose_matrix_below_knee_matches_main() {
        let probes = [
            [0.2f32, 0.2, 0.2],
            [0.5, 0.5, 0.5],
            [0.8, 0.8, 0.8],
            [0.5, 0.2, 0.1],
            [0.05, 0.03, 0.02],
            [0.9, 0.7, 0.5],
        ];
        for (shadows, highlights) in [
            (0.2f32, 0.0),
            (0.4, -0.35),
            (0.8, 0.0),
            (-0.4, 0.0),
            (0.0, 0.5),
            (0.0, -0.5),
        ] {
            let mut checked = 0;
            for display in probes {
                let luma = display_luma(display);
                if luma == 0.0 {
                    continue;
                }
                let shadow_weight = (1.0 - luma) * (1.0 - luma);
                let highlight_weight = luma * luma;
                let target = (luma
                    + toward(shadows, shadow_weight, luma)
                    + toward(highlights, highlight_weight, luma))
                .clamp(0.0, 1.0);
                let ratio = target / luma;
                if ratio > 1.5 {
                    continue;
                }
                let got = run_sh_display(display, shadows, highlights);
                let want = ratio_scaled(display, shadows, highlights);
                for channel in 0..3 {
                    assert!(
                        (got[channel] - want[channel]).abs() < 1e-5,
                        "({shadows},{highlights}) 像素 {display:?} ratio={ratio:.4}：\
                         r≤1.5 时应与现行一致，通道 {channel} 实得 {} 期望 {}",
                        got[channel],
                        want[channel]
                    );
                }
                checked += 1;
            }
            assert!(
                checked > 0,
                "({shadows},{highlights}) 组一个像素都没对照上 ⇒ 测试恒真无效"
            );
        }
    }

    /// **判据 A（magenta-fix §4.3(b)）**：无噪合成色卡，shadows=+0.4，
    /// rel_sat0=0.40，三档保持率门限 L=0.25/0.10/0.05 ⇒ ≥98%/≥75%/≥40%。
    /// （按公式算得约 100%/77%/46%；门限取设计值。）
    #[test]
    fn shadows_highlights_color_card_retention() {
        for (luma_in, floor) in [(0.25f32, 0.98f32), (0.10, 0.75), (0.05, 0.40)] {
            let display = card_pixel(luma_in, 0.40);
            let rel_in = rel_sat_of(display);
            assert!(
                (rel_in - 0.40).abs() < 1e-3,
                "色卡 L={luma_in} 的 rel_sat 应为 0.40，实得 {rel_in}"
            );
            let out = run_sh_display(display, 0.4, 0.0);
            let retention = rel_sat_of(out) / rel_in;
            assert!(
                retention >= floor,
                "色卡 L={luma_in} 保持率应 ≥ {floor}，实得 {retention:.4}"
            );
        }
    }

    /// **渐变带一阶差分（magenta-fix §4.3(a) Q6-1）**：L∈[0,0.35]、
    /// rel_sat=0.30 的无噪平滑渐变，shadows=+0.4；rel_sat(L) 的分窗斜率
    /// 应连续递减、中间窗不塌陷（硬 min(2.0) 在中间窗塌到 ≈0.01 ⇒ 红）。
    #[test]
    fn shadows_highlights_gradient_slope_has_no_hard_kink() {
        let n = 71usize;
        let mut buffer = Buffer::new(0, 0, n as u32, 1);
        let mut lumas = Vec::with_capacity(n);
        for x in 0..n {
            let luma = 0.35 * (x as f32) / ((n - 1) as f32);
            lumas.push(luma);
            let display = card_pixel(luma, 0.30);
            let linear = [
                srgb_to_linear(display[0]),
                srgb_to_linear(display[1]),
                srgb_to_linear(display[2]),
            ];
            buffer.set_pixel(x as u32, 0, [linear[0], linear[1], linear[2], 1.0]);
        }
        shadows_highlights(&mut buffer, 0.4, 0.0);
        let mut rel_sats = Vec::with_capacity(n);
        for x in 0..n {
            let p = buffer.pixel(x as u32, 0);
            let display = [
                linear_to_srgb(p[0]),
                linear_to_srgb(p[1]),
                linear_to_srgb(p[2]),
            ];
            rel_sats.push(rel_sat_of(display));
        }
        // 分窗斜率：窗内 rel_sat 对 L 的最小二乘斜率。
        let slope = |lo: f32, hi: f32| -> f32 {
            let pts: Vec<(f32, f32)> = lumas
                .iter()
                .zip(rel_sats.iter())
                .filter(|(l, _)| **l >= lo && **l < hi)
                .map(|(l, r)| (*l, *r))
                .collect();
            assert!(pts.len() >= 3, "窗口 [{lo},{hi}) 内点太少");
            let (count, sx, sy, sxx, sxy) = pts.iter().fold(
                (0f32, 0f32, 0f32, 0f32, 0f32),
                |(count, sx, sy, sxx, sxy), (x, y)| {
                    (count + 1.0, sx + x, sy + y, sxx + x * x, sxy + x * y)
                },
            );
            (count * sxy - sx * sy) / (count * sxx - sx * sx)
        };
        let s_low = slope(0.08, 0.12);
        let s_mid = slope(0.14, 0.18);
        let s_high = slope(0.18, 0.22);
        assert!(
            s_mid > 0.1,
            "中间窗斜率应 > 0.1（硬钳制塌到 ≈0.01），实得 {s_mid:.3}（低窗 {s_low:.3}，高窗 {s_high:.3}）"
        );
        assert!(
            s_low > s_mid && s_mid > s_high,
            "斜率应连续递减（低 {s_low:.3} > 中 {s_mid:.3} > 高 {s_high:.3}），无单步塌陷"
        );
    }

    /// **luma→0 连续性（magenta-fix §4.3(a) Q6）**：luma=1e-6 的近黑彩像素
    /// 与 luma=0 的纯黑像素，shadows=+0.4 下输出应在 1e-3 内连续
    ///（目标值均为 target_luma ≈ 0.2）。现行代码在此处把微小非零通道
    /// 炸到 ≈0.34 ⇒ 该测试在旧代码上红。
    #[test]
    fn shadows_highlights_luma_zero_continuity() {
        let near_black = run_sh_display([2e-6, 1e-6, 0.5e-6], 0.4, 0.0);
        let black = run_sh_display([0.0, 0.0, 0.0], 0.4, 0.0);
        for channel in 0..3 {
            assert!(
                (near_black[channel] - black[channel]).abs() < 1e-3,
                "近黑与纯黑输出应在 1e-3 内连续，通道 {channel}：近黑 {}，纯黑 {}",
                near_black[channel],
                black[channel]
            );
        }
        // 目标值本身：纯黑回退 toward(0.4, 1.0, 0.0) = 0.2。
        assert!(
            (black[0] - 0.2).abs() < 1e-3,
            "纯黑输出应为 0.2，实得 {}",
            black[0]
        );
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

    /// reds 加 cyan ⇒ 纯红的 R 被大幅吃掉；纯绿在 reds 档权重为 0 ⇒ 不动。
    /// 期 3 A3 改写：旧实现是线性硬分类（纯红 100% 入 reds ⇒ R < 0.01）；
    /// 新实现 Melissa 显示口径下纯红的 reds 差值权重 ≈ 0.66（余下 0.34 在
    /// yellows），R 降至 ≈0.031——断言按新公式手算值改写（< 0.05）。
    #[test]
    fn selective_color_reds_cyan_affects_red_not_green() {
        // SELECTIVE_COLOR_RANGES 的 reds 下标是 0。
        let ranges = ranges_with(0, [1.0, 0.0, 0.0, 0.0]);
        let mut red = filled([1.0, 0.0, 0.0, 1.0]);
        selective_color(&mut red, &ranges);
        let out = px(&red);
        assert!(
            out[0] < 0.05,
            "纯红加满 cyan 后红通道应被吃到 ≈0.03（差值权重 0.66），实得 {out:?}"
        );
        let mut green = filled([0.0, 1.0, 0.0, 1.0]);
        selective_color(&mut green, &ranges);
        assert_eq!(px(&green), [0.0, 1.0, 0.0], "纯绿不属于 reds，必须恰好不动");
    }

    /// 色类路由（设计探针）：**显示**明度 0.4 的近灰入 neutrals——动
    /// neutrals 有效、动 whites／blacks 都无效。期 3 A3 改写：旧测试用
    /// 线性 0.5 灰（显示 ≈0.735，在新口径下属 whites 主导），探针像素
    /// 改按显示口径构造（线性 = srgb_to_linear(0.4) ≈ 0.1329）。
    #[test]
    fn selective_color_gray_routes_to_neutrals_not_whites() {
        let gray = srgb_to_linear(0.4);
        let mut via_neutrals = filled([gray, gray, gray, 1.0]);
        selective_color(&mut via_neutrals, &ranges_with(7, [0.0, 0.0, 0.0, 1.0]));
        let out = px(&via_neutrals);
        assert!(
            out.iter().all(|v| *v < 0.01),
            "显示 0.4 灰加满 neutrals 的 k 应近黑，实得 {out:?}"
        );
        for index in [6usize, 8] {
            let mut untouched = filled([gray, gray, gray, 1.0]);
            selective_color(&mut untouched, &ranges_with(index, [0.0, 0.0, 0.0, 1.0]));
            assert_eq!(
                px(&untouched),
                [gray, gray, gray],
                "显示 0.4 灰不入档 {index}（6=whites/8=blacks），必须恰好不动"
            );
        }
    }

    /// A3 Melissa 往返：矩阵对（含 HDR 与负值）往返 ≈ 恒等；白点 (1,1,1)
    /// 在 ProPhoto 线性里仍是 (1,1,1)（行和自检）。变异：矩阵写错一位
    /// ⇒ 白点或往返断言红。
    #[test]
    fn selective_color_melissa_round_trip_is_identity() {
        let inverse = mat3_inverse(&SRGB_TO_PROPHOTO).expect("常量矩阵可逆");
        let white = mat3_apply(&SRGB_TO_PROPHOTO, [1.0, 1.0, 1.0]);
        for (i, v) in white.iter().enumerate() {
            assert!((v - 1.0).abs() < 0.002, "白点第 {i} 分量应 ≈1：{white:?}");
        }
        for color in [
            [0.13, 0.47, 0.91],
            [1.0, 0.0, 0.0],
            [2.0, 0.5, 0.1],
            [0.4, -0.1, 0.2],
            [0.0, 0.0, 0.0],
        ] {
            let back = melissa_decode(melissa_encode(color), &inverse);
            for (a, b) in back.iter().zip(color.iter()) {
                assert!(
                    (a - b).abs() < 2e-3,
                    "Melissa 往返应恒等：{color:?} → {back:?}"
                );
            }
        }
    }

    /// A3 路由探针：33–36° 暖黄（显示 (0.85, 0.55, 0.15)）在新口径下
    /// yellows 为主权重——单独动 yellows 的 y 与单独动 reds 的 y，效果
    /// 之比应 ≥ 1.2（手算权重 0.571/0.429 ⇒ ≈1.33）。旧线性硬分类把该
    /// 像素整档吞进 reds（yellows 恒为 0）⇒ 变异回旧分类立刻红。
    #[test]
    fn selective_color_warm_yellow_routes_mainly_to_yellows() {
        let pixel = [
            srgb_to_linear(0.85),
            srgb_to_linear(0.55),
            srgb_to_linear(0.15),
            1.0,
        ];
        let effect = |index: usize| {
            let mut buffer = filled(pixel);
            selective_color(&mut buffer, &ranges_with(index, [0.0, 0.0, -0.3, 0.0]));
            let out = melissa_encode(px(&buffer));
            let base = melissa_encode([pixel[0], pixel[1], pixel[2]]);
            (out[0] - base[0]).abs() + (out[1] - base[1]).abs() + (out[2] - base[2]).abs()
        };
        let via_yellows = effect(1);
        let via_reds = effect(0);
        assert!(
            via_yellows > 0.02 && via_yellows >= via_reds * 1.2,
            "暖黄应以 yellows 为主：yellows {via_yellows} vs reds {via_reds}"
        );
    }

    /// A3 均匀探针：显示 0.35 与 0.65 两灰（关于 0.5 对称 ⇒ neutrals
    /// 权重同为 0.8）加 neutrals c=+0.4，显示 CMYK 的 c 槽位移应相等
    /// （≈0.32）。旧线性分类下两灰的档归属/权重不同 ⇒ 位移不等。
    #[test]
    fn selective_color_neutrals_offset_is_uniform_across_lightness() {
        let delta_c = |display_gray: f32| {
            let lin = srgb_to_linear(display_gray);
            let mut buffer = filled([lin, lin, lin, 1.0]);
            selective_color(&mut buffer, &ranges_with(7, [0.4, 0.0, 0.0, 0.0]));
            let before = rgb_to_cmyk(melissa_encode([lin, lin, lin]))[0];
            let after = rgb_to_cmyk(melissa_encode(px(&buffer)))[0];
            after - before
        };
        let dark = delta_c(0.35);
        let bright = delta_c(0.65);
        assert!(dark > 0.2, "位移必须非空（防安慰剂）：{dark}");
        assert!(
            (dark - bright).abs() < 0.02,
            "同 neutrals 偏移在两亮度下 Δc 应相等：{dark} vs {bright}"
        );
    }

    /// A3 参照带：同参数（reds c +0.3）在 Melissa 口径下的 Δ 应小于
    /// 直接在 sRGB 显示空间施加的 Δ，比值落在专家所述的参照带内
    /// （去掉矩阵往返偏大约 35–50% ⇒ 比值 ≈ 0.67–0.74；断言放宽为
    /// [0.55, 0.85]）。变异：去掉矩阵往返 ⇒ 比值 = 1 ⇒ 红。
    #[test]
    fn selective_color_prophoto_round_trip_damps_delta_into_reference_band() {
        let ranges = ranges_with(0, [0.3, 0.0, 0.0, 0.0]);
        // 中间调红（显示口径构造）：Melissa 与 sRGB 显示值都可定义。
        let lin = [
            srgb_to_linear(0.72),
            srgb_to_linear(0.45),
            srgb_to_linear(0.32),
        ];
        let via_melissa = selective_apply_display(melissa_encode(lin), &ranges);
        let base_melissa = melissa_encode(lin);
        let delta_melissa = (via_melissa[0] - base_melissa[0]).abs()
            + (via_melissa[1] - base_melissa[1]).abs()
            + (via_melissa[2] - base_melissa[2]).abs();
        let display = [0.72, 0.45, 0.32];
        let via_srgb = selective_apply_display(display, &ranges);
        let delta_srgb = (via_srgb[0] - display[0]).abs()
            + (via_srgb[1] - display[1]).abs()
            + (via_srgb[2] - display[2]).abs();
        let ratio = delta_melissa / delta_srgb;
        assert!(
            (0.55..=0.85).contains(&ratio),
            "Melissa/sRGB 的 Δ 比值应落在参照带 [0.55, 0.85]：{ratio}（{delta_melissa} vs {delta_srgb}）"
        );
    }

    /// 白档保底（O4）：亮饱和像素的 whites 权重不再被门控精确截到零。
    /// 唇口类像素（Melissa span ≥ 0.08 且 lightness > 0.625）：现行
    /// w[6] = 0，改后 w[6] = ε·whites（ε=0.20）。变异：把保底改回
    /// 0（或删掉 `.max(...)`）⇒ 本测试红。
    #[test]
    fn selective_color_whites_floor_reaches_bright_saturated_pixels() {
        // 唇口类：span 0.35、L 0.775 ⇒ whites 满权 1.0、q = 1。
        let w = selective_weights([0.95, 0.75, 0.60]);
        assert!(
            (w[6] - 0.20).abs() < 1e-6,
            "亮饱和像素的 whites 权重应被保底到 ε=0.20，实得 w[6]={}",
            w[6]
        );
        // 中饱和：span 0.25、L 0.725 ⇒ whites 0.8、q = 1 ⇒ w[6] = 0.16。
        let w = selective_weights([0.85, 0.70, 0.60]);
        assert!(
            (w[6] - 0.16).abs() < 1e-6,
            "whites 0.8 的亮饱和像素 w[6] 应为 0.16，实得 {}",
            w[6]
        );
        // 门下像素不动：span 0.04、L 0.8 ⇒ whites 1.0、q = 0.5 ⇒
        // (1−q) = 0.5 > ε，保底不触发，w[6] 与现行一致为 0.5。
        let w = selective_weights([0.82, 0.80, 0.78]);
        assert!(
            (w[6] - 0.50).abs() < 1e-6,
            "门下像素的 whites 权重应仍为 (1−q)=0.5，实得 {}",
            w[6]
        );
    }

    /// 九档权重和恒为 1（O4 仍须保的不变量；保底多出来的权重从色相
    /// 档扣，构造保证）。灰／暗／亮饱和／中饱和各取一点。变异：色相
    /// 档仍乘 q 而不按余量分配 ⇒ 和 ≠ 1，本测试红。
    #[test]
    fn selective_color_weights_sum_to_one() {
        for d in [
            [0.4, 0.4, 0.4],
            [0.1, 0.08, 0.05],
            [0.95, 0.75, 0.60],
            [0.85, 0.70, 0.60],
            [0.30, 0.55, 0.90],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0],
        ] {
            let w = selective_weights(d);
            let sum: f32 = w.iter().sum();
            assert!(
                (sum - 1.0).abs() < 1e-6,
                "九档权重和应为 1，像素 {d:?} 实得 {sum}（{w:?}）"
            );
            assert!(
                w.iter().all(|v| *v >= 0.0),
                "权重不许为负，像素 {d:?} 实得 {w:?}"
            );
        }
    }

    /// 白档保底的功能证明：whites M+ 现在能动亮饱和像素。现行门控
    /// 下该像素的 w[6]=0 且色相档 ranges 全零 ⇒ 偏移为零 ⇒ 输出逐
    /// 位等于输入；改后 whites M+1.0 生效，绿通道被压下。变异：保
    /// 底改回 0 ⇒ 输出恒等，本测试红。
    #[test]
    fn selective_color_whites_magnet_moves_bright_saturated_pixel() {
        // 显示值 [0.95, 0.75, 0.60]：唇口类亮饱和像素。
        let d = [0.95, 0.75, 0.60];
        let ranges = ranges_with(6, [0.0, 1.0, 0.0, 0.0]);
        let out = selective_apply_display(d, &ranges);
        assert_ne!(out, d, "whites M+ 应动亮饱和像素，输出却逐位等于输入");
        assert!(
            out[1] < d[1],
            "whites M+（品红 +1）应压下绿通道，实得 {out:?} vs {d:?}"
        );
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
    fn apply_adjustment_recognises_the_seven_new_names() {
        for name in [
            "black_and_white",
            "channel_mixer",
            "gradient_map",
            "shadows_highlights",
            "selective_color",
            "color_lut",
            "calibrate_primaries",
        ] {
            let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
            assert!(
                apply_adjustment(&mut buffer, name, &json!({}), 1.0, (4.0, 4.0)),
                "{name} 未被 apply_adjustment 识别"
            );
        }
        // 默认参数下各调整的恒等/确定行为：
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "channel_mixer",
            &json!({}),
            1.0,
            (4.0, 4.0)
        ));
        assert_rgb_close(px(&buffer), [0.5, 0.3, 0.1], "channel_mixer 缺省应恒等");
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "calibrate_primaries",
            &json!({}),
            1.0,
            (4.0, 4.0)
        ));
        assert_rgb_close(
            px(&buffer),
            [0.5, 0.3, 0.1],
            "calibrate_primaries 缺省应恒等",
        );
        let mut buffer = filled([0.5, 0.3, 0.1, 1.0]);
        assert!(apply_adjustment(
            &mut buffer,
            "shadows_highlights",
            &json!({}),
            1.0,
            (4.0, 4.0)
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
            1.0,
            (4.0, 4.0)
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
        // **vibrance 专项**（2026-10-21 修图实测抓到的偷换）：
        // 规范名 `vibrance` 必须到达真 Vibrance、不许再被 Saturation 冒名；
        // 旧别名 `vibrance_natural` 保留（已落存文档渲染不变）；
        // `saturation` 只认自己的名字。
        assert_eq!(
            AdjustmentKind::from_name("vibrance"),
            Some(AdjustmentKind::Vibrance),
            "vibrance 必须解析为 Vibrance（此前被偷换成 Saturation）"
        );
        assert_eq!(
            AdjustmentKind::from_name("vibrance_natural"),
            Some(AdjustmentKind::Vibrance),
            "vibrance_natural 别名必须保留为 Vibrance"
        );
        assert_eq!(
            AdjustmentKind::from_name("saturation"),
            Some(AdjustmentKind::Saturation),
            "saturation 必须解析为 Saturation"
        );
        assert_eq!(AdjustmentKind::Vibrance.as_str(), "vibrance");
    }
}

/// **照片修图三模块测试**（warp 底座／Heckbert 单应／凸校验／镜头畸变／降噪）。
///
/// 断言口径（AGENTS.md 内核 1/2）：恒等式与关系断言优先，不写死像素数；
/// 每条断言都经过变异验证（改坏实现必须红 ✓）。
#[cfg(test)]
mod photoretouch_tests {
    use super::*;

    fn gradient() -> Buffer {
        let mut buffer = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = (x * 8 + y) as f32 / 63.0;
                buffer.set_pixel(x, y, [v, 1.0 - v, v * 0.5, 1.0]);
            }
        }
        buffer
    }

    #[test]
    fn warp_identity_map_is_exact() {
        // **恒等映射 ⇒ 逐位相同**（双三次在整数坐标精确命中中心像素 ✓）。
        let before = gradient();
        let mut after = before.clone();
        warp_buffer(&mut after, |x, y| (x, y));
        assert_eq!(before.pixels(), after.pixels());
    }

    #[test]
    fn catmull_clamp_kills_ringing() {
        // **2×2 钳制**：相邻两 HDR 亮点 (1,2)/(2,2)=2.0；在 (1.5, 2.0) 采样，
        // 原双三次给出 2×(0.5625+0.5625) = 2.25（超调 +12.5%，手算 ✓）；
        // 内圈 2×2 的 max=2 ⇒ 钳后恰为 2.0 ✓。
        let mut buffer = Buffer::new(0, 0, 5, 5);
        buffer.set_pixel(1, 2, [2.0, 2.0, 2.0, 1.0]);
        buffer.set_pixel(2, 2, [2.0, 2.0, 2.0, 1.0]);
        let p = sample_catmull_rom(&buffer, 1.5, 2.0);
        assert!(
            p[0] <= 2.0 + 1e-6,
            "超调未被 2×2 钳制剪掉（原值 2.25）：{p:?}"
        );
        assert!(p[0] > 1.9, "应在内圈 max 附近：{p:?}");
    }

    #[test]
    fn heckbert_rectangle_is_identity() {
        // **矩形四边形 ⇒ 恒等单应**（网格点逐一比对，容差 1e-4 ✓）。
        let w = 8.0f32;
        let q = [[0.0, 0.0], [w, 0.0], [w, w], [0.0, w]];
        let hom = unit_square_to_quad(q).expect("矩形不应退化");
        for iy in 0..=4 {
            for ix in 0..=4 {
                let (u, v) = (ix as f32 / 4.0, iy as f32 / 4.0);
                let (sx, sy) = hom.map(u, v);
                assert!(
                    (sx - u * w).abs() < 1e-4 && (sy - v * w).abs() < 1e-4,
                    "({u},{v}) → ({sx},{sy})，期望 ({},{})",
                    u * w,
                    v * w
                );
            }
        }
    }

    #[test]
    fn heckbert_matches_independent_dlt() {
        // **Heckbert 闭式 == 独立 DLT 求解**（8×8 高斯消元，f64；两种不同算法族 ✓）。
        let q = [[1.0, 0.0], [9.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let heck = unit_square_to_quad(q).expect("梯形不应退化");
        let dlt = dlt_solve(q);
        let h = [
            heck.a, heck.b, heck.c, heck.d, heck.e, heck.f, heck.g, heck.h,
        ];
        for (i, (a, b)) in h.iter().zip(dlt.iter()).enumerate() {
            assert!((a - b).abs() < 1e-3, "系数 {i} 漂移：Heckbert={a} DLT={b}");
        }
    }

    /// 测试内独立 DLT（8 参数，高斯消元；与 Heckbert 闭式是不同的算法 ✓）。
    fn dlt_solve(q: [[f32; 2]; 4]) -> [f32; 8] {
        // (u,v)∈{(0,0),(1,0),(1,1),(0,1)} → q；h33=1 归一化，8 个方程解 a..h。
        let src = [(0.0f64, 0.0f64), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let mut m = [[0.0f64; 9]; 8];
        for (i, ((u, v), p)) in src.iter().zip(q.iter()).enumerate() {
            let (x, y) = (p[0] as f64, p[1] as f64);
            m[2 * i] = [*u, *v, 1.0, 0.0, 0.0, 0.0, -x * u, -x * v, x];
            m[2 * i + 1] = [0.0, 0.0, 0.0, *u, *v, 1.0, -y * u, -y * v, y];
        }
        // 高斯消元（8×9 增广）。
        for col in 0..8 {
            let mut pivot = col;
            for row in col..8 {
                if m[row][col].abs() > m[pivot][col].abs() {
                    pivot = row;
                }
            }
            m.swap(col, pivot);
            assert!(m[col][col].abs() > 1e-12, "DLT 退化");
            // **`pivot` 先复制**（第 1314 轮）：否则 `m[row]` 与 `m[col]` 同时可变借用 ⇒ 编不过 ✓
            let pivot = m[col];
            for (row, mrow) in m.iter_mut().enumerate() {
                if row != col {
                    let f = mrow[col] / pivot[col];
                    for (cell, &p) in mrow.iter_mut().zip(pivot.iter()).skip(col) {
                        *cell -= f * p;
                    }
                }
            }
        }
        let mut out = [0.0f32; 8];
        for i in 0..8 {
            out[i] = (m[i][8] / m[i][i]) as f32;
        }
        out
    }

    #[test]
    fn convex_rejects_concave_quad() {
        // **凹四边形（箭镞）⇒ 拒绝且 buffer 不动** ✓。
        let dart = [[0.0, 0.0], [4.0, 4.0], [8.0, 0.0], [4.0, 1.0]];
        assert!(!quad_is_convex(dart), "箭镞四边形应判为凹");
        let before = gradient();
        let mut after = before.clone();
        assert!(!perspective_warp(
            &mut after,
            dart,
            0.0,
            0.0,
            true,
            (8.0, 8.0)
        ));
        assert_eq!(before.pixels(), after.pixels());
        // 凸四边形（矩形）放行 ✓。
        let rect = [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]];
        assert!(quad_is_convex(rect));
    }

    #[test]
    fn radial_distort_grid_matches_f64_reference() {
        // **100 点网格 vs f64 独立参考实现，误差 <1e-5**（专家核验法：比坐标映射，不比整图 ✓）。
        let (k1, k2) = (0.15f64, -0.05f64);
        for iy in 0..10 {
            for ix in 0..10 {
                let (nx, ny) = (ix as f32 * 0.2 - 0.9, iy as f32 * 0.2 - 0.9);
                let (dx, dy) = radial_distort(nx, ny, k1 as f32, k2 as f32);
                let (nx64, ny64) = (nx as f64, ny as f64);
                let r2 = nx64 * nx64 + ny64 * ny64;
                let f = 1.0 + k1 * r2 + k2 * r2 * r2;
                let (ex, ey) = (nx64 * f, ny64 * f);
                assert!(
                    (dx as f64 - ex).abs() < 1e-5 && (dy as f64 - ey).abs() < 1e-5,
                    "({nx},{ny}) → ({dx},{dy})，期望 ({ex},{ey})"
                );
            }
        }
    }

    #[test]
    fn perspective_identity_quad_is_exact() {
        // **恒等四边形 ⇒ warp 逐位相同**（auto_crop 在恒等时恰为 1 ✓）。
        let before = gradient();
        let mut after = before.clone();
        let rect = [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]];
        assert!(perspective_warp(
            &mut after,
            rect,
            0.0,
            0.0,
            true,
            (8.0, 8.0)
        ));
        assert_eq!(before.pixels(), after.pixels());
    }

    /// 窗口图案：按**文档坐标**生成的可区分纹理（整幅与子缓冲同源 ⇒ 可逐位比 ✓）。
    fn pattern_window(ox: i64, oy: i64, w: u32, h: u32) -> Buffer {
        let mut buffer = Buffer::new(ox, oy, w, h);
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = ((ox + i64::from(x)) as u32, (oy + i64::from(y)) as u32);
                buffer.set_pixel(
                    x,
                    y,
                    [
                        ((dx * 31 + dy * 17) % 251) as f32 / 251.0,
                        ((dx * 13 + dy * 29) % 241) as f32 / 241.0,
                        ((dx * 7 + dy * 11) % 233) as f32 / 233.0,
                        1.0,
                    ],
                );
            }
        }
        buffer
    }

    #[test]
    fn perspective_on_window_matches_full_frame() {
        // **子缓冲（分带窗口）与整幅必须同算一件事**（缺陷：warp 曾按带本地坐标解释
        // quad/归一化 ⇒ 并行分带输出交替白带 ✓）。画幅 64×48、quad 顶边下拉；
        // 窗口 = 行 8..48；比较位移+采样足迹都在窗口内的行（16..44），须逐位一致 ✓。
        let quad = [[0.0, 8.0], [64.0, 4.0], [64.0, 48.0], [0.0, 48.0]];
        let canvas = (64.0, 48.0);
        let mut full = pattern_window(0, 0, 64, 48);
        assert!(perspective_warp(&mut full, quad, 0.0, 0.0, true, canvas));
        let mut win = pattern_window(0, 8, 64, 40);
        assert!(perspective_warp(&mut win, quad, 0.0, 0.0, true, canvas));
        for y in 16..44u32 {
            for x in 0..64u32 {
                assert_eq!(
                    win.pixel(x, y - 8),
                    full.pixel(x, y),
                    "({x},{y}) 窗口与整幅不一致"
                );
            }
        }
    }

    #[test]
    fn perspective_identity_on_window_is_exact() {
        // **恒等 quad 在窗口上也必须逐位恒等**（窗口原点 ≠ 0 时旧实现按本地尺寸
        // 归一化 ⇒ 采样点整体错位 ✓）。
        let canvas = (64.0, 48.0);
        let quad = [[0.0, 0.0], [64.0, 0.0], [64.0, 48.0], [0.0, 48.0]];
        let before = pattern_window(0, 16, 64, 32);
        let mut after = before.clone();
        assert!(perspective_warp(&mut after, quad, 0.0, 0.0, true, canvas));
        for y in 4..28u32 {
            for x in 0..64u32 {
                assert_eq!(
                    after.pixel(x, y),
                    before.pixel(x, y),
                    "恒等透视在窗口 ({x},{}) 不应改动",
                    y + 16
                );
            }
        }
    }

    #[test]
    fn lens_on_window_matches_full_frame() {
        // **镜头畸变同理**：中心/半径按画幅算 ⇒ 窗口与整幅在足迹内逐位一致
        // （k1=0 时旧实现恰好也是恒等、测不出来；k1≠0 才现形 ✓）。
        // 画幅 64×64、k1=0.25（角点位移 ~11px）；窗口 = 行 12..64；比较行 26..50 ✓。
        let canvas = (64.0, 64.0);
        let mut full = pattern_window(0, 0, 64, 64);
        lens_correction(&mut full, 0.25, 0.0, 1.0, 1.0, 0.0, false, canvas);
        let mut win = pattern_window(0, 12, 64, 52);
        lens_correction(&mut win, 0.25, 0.0, 1.0, 1.0, 0.0, false, canvas);
        for y in 26..50u32 {
            for x in 0..64u32 {
                assert_eq!(
                    win.pixel(x, y - 12),
                    full.pixel(x, y),
                    "({x},{y}) 窗口与整幅不一致"
                );
            }
        }
    }

    #[test]
    fn adjustment_padding_covers_geometry_and_denoise() {
        // **padding 申报必须盖住真实位移**（关系断言 ✓）：渲染器据此把子缓冲窗口
        // 外扩到装得下源像素；申报不足 ⇒ 采样判越界写透明（白带回归 ✓）。
        let frame = (200.0, 150.0);
        let identity = AdjustmentKind::Perspective.padding(
            &serde_json::json!({"quad": [[0, 0], [200, 0], [200, 150], [0, 150]]}),
            frame,
        );
        let shifted = AdjustmentKind::Perspective.padding(
            &serde_json::json!({"quad": [[0, 20], [200, 16], [200, 150], [0, 150]]}),
            frame,
        );
        assert!(
            shifted >= 20,
            "顶边下拉 20px 的透视 padding 应 ≥ 20，实测 {shifted}"
        );
        assert!(
            shifted > identity,
            "非恒等透视的 padding 应大于恒等：{shifted} vs {identity}"
        );
        let lens_off = AdjustmentKind::Lens.padding(&serde_json::json!({"k1": 0.0}), frame);
        let lens_barrel = AdjustmentKind::Lens
            .padding(&serde_json::json!({"k1": 0.35, "auto_crop": false}), frame);
        assert!(
            lens_barrel >= 40,
            "k1=0.35 在 200×150 上角点位移 ~35+余量，padding 应 ≥ 40，实测 {lens_barrel}"
        );
        assert!(
            lens_barrel > lens_off,
            "畸变越大 padding 越大：{lens_barrel} vs {lens_off}"
        );
        let denoise_full = AdjustmentKind::Denoise.padding(
            &serde_json::json!({"luminance": 1.0, "detail": 1.0, "color": 1.0}),
            frame,
        );
        let denoise_off = AdjustmentKind::Denoise.padding(
            &serde_json::json!({"luminance": 0.0, "detail": 1.0, "color": 0.0}),
            frame,
        );
        assert_eq!(denoise_off, 0, "降噪两路都关 ⇒ 无邻域需求");
        assert!(
            denoise_full >= 18,
            "降噪满档 padding 应 ≥ 18，实测 {denoise_full}"
        );
        assert_eq!(
            AdjustmentKind::Invert.padding(&serde_json::json!({}), frame),
            0,
            "逐像素调整不需要外扩"
        );
    }

    #[test]
    fn lens_identity_params_do_nothing() {
        // **默认镜头参数 ⇒ 恒等**（k1=k2=0、tca=1、vignette=0、auto_crop 的 scale=1 ✓）。
        let before = gradient();
        let mut after = before.clone();
        lens_correction(&mut after, 0.0, 0.0, 1.0, 1.0, 0.0, true, (8.0, 8.0));
        assert_eq!(before.pixels(), after.pixels());
    }

    #[test]
    fn lens_distortion_moves_edge_pixel_outward() {
        // **桶形畸变（k1>0）把边缘像素向外推**：顶部亮点经逆映射后采样位置更靠上（手算值 ✓）。
        // (8,2)：中心(8,8)，rmax=√128≈11.314；r²=((2−8)/11.314)²=0.2812；f=1+0.5×0.2812=1.1406；
        // src_y = 8+(2−8)×1.1406 = 1.156 ⇒ 输出 (8,2) 采自 (8,1.16)，亮点被搬走 ✓。
        let mut buffer = Buffer::new(0, 0, 16, 16);
        buffer.set_pixel(8, 2, [1.0, 1.0, 1.0, 1.0]);
        lens_correction(&mut buffer, 0.5, 0.0, 1.0, 1.0, 0.0, false, (16.0, 16.0));
        let p = buffer.pixel(8, 2);
        assert!(
            p[0] < 0.5,
            "桶形畸变后 (8,2) 应变暗（亮点被推向上方）：{p:?}"
        );
    }

    #[test]
    fn denoise_zero_params_is_exact_identity() {
        // **luminance=0 && color=0 ⇒ 提前返回，逐位相同** ✓。
        let before = gradient();
        let mut after = before.clone();
        denoise(&mut after, 0.0, 0.5, 0.0);
        assert_eq!(before.pixels(), after.pixels());
    }

    #[test]
    fn denoise_flat_noise_is_smoothed() {
        // **平坦区脉冲噪点被拉回**，而远处平坦像素纹丝不动（引导滤波 var=0 ⇒ q=mean ✓）。
        let mut buffer = Buffer::filled(0, 0, 32, 32, [0.5, 0.5, 0.5, 1.0]);
        buffer.set_pixel(16, 16, [0.9, 0.9, 0.9, 1.0]);
        denoise(&mut buffer, 1.0, 0.5, 0.0);
        let spike = buffer.pixel(16, 16);
        assert!(spike[0] < 0.7, "脉冲噪点应被拉向平坦值：{spike:?}");
        let flat = buffer.pixel(0, 0);
        assert!((flat[0] - 0.5).abs() < 1e-4, "平坦区不应动：{flat:?}");
    }

    #[test]
    fn denoise_preserves_step_edge() {
        // **强边缘保持**：0.2/0.8 台阶，降噪后边两侧差值仍 > 0.4（引导滤波 var≫ε ⇒ a≈1 ✓）。
        let mut buffer = Buffer::new(0, 0, 8, 8);
        for y in 0..8 {
            for x in 0..8 {
                let v = if x < 4 { 0.2 } else { 0.8 };
                buffer.set_pixel(x, y, [v, v, v, 1.0]);
            }
        }
        denoise(&mut buffer, 1.0, 0.5, 0.0);
        let (l, r) = (buffer.pixel(3, 4)[0], buffer.pixel(4, 4)[0]);
        assert!(r - l > 0.4, "台阶边应保持：左 {l} 右 {r}");
    }
}

/// **Presence 三件套测试（包 D）**：恒等式 / 名字四处一致 / 单调性（数值关系断言 ✓）。
#[cfg(test)]
mod presence_tests {
    use super::*;

    fn pattern_buffer() -> Buffer {
        let mut buffer = Buffer::new(0, 0, 16, 16);
        for y in 0..16u32 {
            for x in 0..16u32 {
                let v = ((x * 31 + y * 17) % 251) as f32 / 251.0;
                buffer.set_pixel(x, y, [v, 1.0 - v, v * 0.5, 1.0]);
            }
        }
        buffer.set_pixel(3, 3, [0.2, 0.1, 0.05, 0.5]);
        buffer
    }

    fn std_of(buffer: &Buffer, channel: usize) -> f32 {
        let values: Vec<f32> = buffer
            .pixels()
            .chunks_exact(4)
            .map(|pixel| pixel[channel])
            .collect();
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / values.len() as f32;
        var.sqrt()
    }

    /// 带通能量：`mean((blur_small(I) − blur_large(I))²)`（R 通道；用方框模糊实现，
    /// 与被测的引导滤波是不同算法族 ⇒ 不是同义反复 ✓）。
    fn band_energy(buffer: &Buffer, small: u32, large: u32) -> f32 {
        let mut lo = buffer.clone();
        box_blur(&mut lo, small, 2);
        let mut hi = buffer.clone();
        box_blur(&mut hi, large, 2);
        let mut sum = 0.0f32;
        let mut count = 0usize;
        for (a, b) in lo.pixels().chunks_exact(4).zip(hi.pixels().chunks_exact(4)) {
            let d = a[0] - b[0];
            sum += d * d;
            count += 1;
        }
        sum / count as f32
    }

    #[test]
    fn presence_zero_amount_is_exact_identity() {
        // **amount = 0（直调与 apply_adjustment 两条路）⇒ 逐位不变** ✓。
        let before = pattern_buffer();
        for apply in [
            dehaze_adjustment as fn(&mut Buffer, f32),
            clarity_adjustment,
            texture_adjustment,
        ] {
            let mut after = before.clone();
            apply(&mut after, 0.0);
            assert_eq!(before.pixels(), after.pixels(), "amount=0 必须逐位恒等");
        }
        for name in ["dehaze", "clarity", "texture"] {
            for params in [serde_json::json!({"amount": 0.0}), serde_json::json!({})] {
                let mut after = before.clone();
                assert!(
                    apply_adjustment(&mut after, name, &params, 1.0, (16.0, 16.0)),
                    "{name} 应被 apply_adjustment 识别"
                );
                assert_eq!(
                    before.pixels(),
                    after.pixels(),
                    "{name} 缺省/零参数必须逐位恒等"
                );
            }
        }
    }

    #[test]
    fn presence_names_agree_in_all_four_places() {
        // **四处一致**：enum 变体 / as_str / from_name / ADJUSTMENT_NAMES（服务端
        // EffectKind::supported 直接取 ADJUSTMENT_NAMES ⇒ 它就是服务端校验名单的单一来源 ✓）。
        let cases = [
            (AdjustmentKind::Dehaze, "dehaze"),
            (AdjustmentKind::Clarity, "clarity"),
            (AdjustmentKind::Texture, "texture"),
        ];
        assert_eq!(AdjustmentKind::ALL.len(), 26);
        assert_eq!(ADJUSTMENT_NAMES.len(), 26);
        for (kind, name) in cases {
            assert_eq!(kind.as_str(), name);
            assert_eq!(
                AdjustmentKind::from_name(kind.as_str()),
                Some(kind),
                "{name} 的 from_name/as_str 必须往返"
            );
            assert!(
                ADJUSTMENT_NAMES.contains(&name),
                "{name} 必须在 ADJUSTMENT_NAMES（服务端校验名单）里"
            );
            assert!(AdjustmentKind::ALL.contains(&kind), "{name} 必须在 ALL 里");
        }
    }

    #[test]
    fn presence_padding_is_declared_and_zero_when_off() {
        // **邻域调整必须申报 padding**（关系断言 ✓）：关 ⇒ 0；开 ⇒ clarity > texture > 0。
        let frame = (200.0, 150.0);
        let off = serde_json::json!({"amount": 0.0});
        let on = serde_json::json!({"amount": 1.0});
        for kind in [
            AdjustmentKind::Dehaze,
            AdjustmentKind::Clarity,
            AdjustmentKind::Texture,
        ] {
            assert_eq!(
                kind.padding(&off, frame),
                0,
                "{} 关时不应外扩",
                kind.as_str()
            );
            assert!(
                kind.padding(&on, frame) > 0,
                "{} 开时必须外扩",
                kind.as_str()
            );
        }
        assert!(
            AdjustmentKind::Clarity.padding(&on, frame)
                > AdjustmentKind::Texture.padding(&on, frame),
            "clarity 半径大于 texture ⇒ padding 应更大"
        );
    }

    /// 合成雾图：上部天空 = 纯雾色（大气光可估 ✓），下部场景两色块被 50% 雾混合。
    fn foggy_scene() -> Buffer {
        let air = [0.85f32, 0.88, 0.90];
        let mut buffer = Buffer::new(0, 0, 32, 16);
        for y in 0..16u32 {
            for x in 0..32u32 {
                let pixel = if y < 4 {
                    [air[0], air[1], air[2], 1.0]
                } else if x < 16 {
                    [
                        0.08 * 0.5 + air[0] * 0.5,
                        0.08 * 0.5 + air[1] * 0.5,
                        0.10 * 0.5 + air[2] * 0.5,
                        1.0,
                    ]
                } else {
                    [
                        0.60 * 0.5 + air[0] * 0.5,
                        0.25 * 0.5 + air[1] * 0.5,
                        0.20 * 0.5 + air[2] * 0.5,
                        1.0,
                    ]
                };
                buffer.set_pixel(x, y, pixel);
            }
        }
        buffer
    }

    #[test]
    fn dehaze_amount_monotonically_restores_fog_contrast() {
        // **单调性**：正 amount 越大，雾图 R 通道标准差（对比度）越大；负值则更低 ✓。
        let base = foggy_scene();
        let c0 = std_of(&base, 0);
        let mut previous = c0;
        for amount in [0.35f32, 0.7, 1.0] {
            let mut after = base.clone();
            dehaze_adjustment(&mut after, amount);
            let contrast = std_of(&after, 0);
            assert!(
                contrast > previous,
                "dehaze {amount} 后对比度应单调上升：{previous} → {contrast}（原始 {c0}）"
            );
            previous = contrast;
        }
        let mut hazed = base.clone();
        dehaze_adjustment(&mut hazed, -0.6);
        assert!(
            std_of(&hazed, 0) < c0,
            "dehaze 负值应加雾降对比：{} vs 原始 {c0}",
            std_of(&hazed, 0)
        );
    }

    /// 中频样本：8px 方块波（周期 16px），中间调 0.45/0.55。
    fn midband_sample() -> Buffer {
        let mut buffer = Buffer::new(0, 0, 64, 64);
        for y in 0..64u32 {
            for x in 0..64u32 {
                let v = if (x / 8) % 2 == 0 { 0.45 } else { 0.55 };
                buffer.set_pixel(x, y, [v, v, v, 1.0]);
            }
        }
        buffer
    }

    #[test]
    fn clarity_amount_monotonically_raises_midband_energy() {
        // **中频带 = blur(2) 与 blur(16) 之差的能量**；正 clarity 应单调抬升、负值压低 ✓。
        let base = midband_sample();
        let e0 = band_energy(&base, 2, 16);
        let mut previous = e0;
        for amount in [0.5f32, 1.0] {
            let mut after = base.clone();
            clarity_adjustment(&mut after, amount);
            let energy = band_energy(&after, 2, 16);
            assert!(
                energy > previous,
                "clarity {amount} 后中频能量应上升：{previous} → {energy}（原始 {e0}）"
            );
            previous = energy;
        }
        let mut softened = base.clone();
        clarity_adjustment(&mut softened, -0.8);
        assert!(
            band_energy(&softened, 2, 16) < e0,
            "clarity 负值应压中频能量：{} vs 原始 {e0}",
            band_energy(&softened, 2, 16)
        );
        // 平坦区不动（detail = 0 ⇒ 任意 amount 逐位恒等）✓。
        let flat = Buffer::filled(0, 0, 32, 32, [0.5, 0.5, 0.5, 1.0]);
        let mut after = flat.clone();
        clarity_adjustment(&mut after, 1.0);
        assert_eq!(flat.pixels(), after.pixels(), "平坦区 clarity 不应改动");
    }

    /// 高频样本：平滑大块底 + 小幅细纹理（±0.04 的确定性hash纹理，无三角函数 ✓）。
    fn highband_sample() -> Buffer {
        let mut buffer = Buffer::new(0, 0, 64, 64);
        for y in 0..64u32 {
            for x in 0..64u32 {
                let base = if x < 32 { 0.45 } else { 0.55 };
                let grain = (((x * 7 + y * 13) % 5) as f32 - 2.0) * 0.02;
                let v = base + grain;
                buffer.set_pixel(x, y, [v, v, v, 1.0]);
            }
        }
        buffer
    }

    #[test]
    fn texture_amount_monotonically_raises_highband_energy() {
        // **高频带 = blur(1) 与 blur(4) 之差的能量**；正 texture 应单调抬升、负值压低 ✓。
        let base = highband_sample();
        let e0 = band_energy(&base, 1, 4);
        let mut previous = e0;
        for amount in [0.5f32, 1.0] {
            let mut after = base.clone();
            texture_adjustment(&mut after, amount);
            let energy = band_energy(&after, 1, 4);
            assert!(
                energy > previous,
                "texture {amount} 后高频能量应上升：{previous} → {energy}（原始 {e0}）"
            );
            previous = energy;
        }
        let mut smoothed = base.clone();
        texture_adjustment(&mut smoothed, -0.8);
        assert!(
            band_energy(&smoothed, 1, 4) < e0,
            "texture 负值应压高频能量：{} vs 原始 {e0}",
            band_energy(&smoothed, 1, 4)
        );
        // **尺度选择性（关系断言）**：texture 对高频带的相对提升应大于对中频带的相对提升 ✓。
        let mid0 = band_energy(&base, 2, 16);
        let mut after = base.clone();
        texture_adjustment(&mut after, 1.0);
        let high_gain = band_energy(&after, 1, 4) / e0;
        let mid_gain = band_energy(&after, 2, 16) / mid0.max(1e-9);
        assert!(
            high_gain > mid_gain,
            "texture 应更偏向高频：高频增益 {high_gain} vs 中频增益 {mid_gain}"
        );
    }

    /// 显示灰阶对（左/右两半）的 clarity 总响应（Σ|ΔY'|）。
    fn clarity_step_response(dark: f32, bright: f32) -> f32 {
        let mut buffer = Buffer::new(0, 0, 32, 16);
        for y in 0..16u32 {
            for x in 0..32u32 {
                let d = if x < 16 { dark } else { bright };
                let lin = srgb_to_linear(d);
                buffer.set_pixel(x, y, [lin, lin, lin, 1.0]);
            }
        }
        let before: Vec<f32> = buffer
            .pixels()
            .chunks_exact(4)
            .map(|p| srgb_encode_extended(p[0]))
            .collect();
        clarity_adjustment(&mut buffer, 1.0);
        buffer
            .pixels()
            .chunks_exact(4)
            .zip(before.iter())
            .map(|(p, b)| (srgb_encode_extended(p[0]) - b).abs())
            .sum()
    }

    /// A4-1 掩模中心探针：小落差（0.04 显示）灰阶对，中心在显示 0.5 的
    /// 一对（0.48/0.52）响应必须**大于**中心在显示 0.75 的一对
    /// （0.73/0.77）。小落差使引导滤波落在 eps 主导区（detail ≈ 值 −
    /// 均值、与平面无关），响应比只由掩模权重与回写导数决定：新口径
    /// ≈ 0.020 vs 0.015（中 > 高）；旧线性平面下掩模中心在线性 0.5
    /// ≈ 显示 0.735，两对权重排位反转（≈0.0147 vs 0.0217，中 < 高）
    /// ⇒ 变异回线性平面红。**大落差不行**：eps 与方差的相互作用会
    /// 把排位搅乱（第一版 0.2 落差探针实测抓不住线性变异，教训）。
    #[test]
    fn clarity_midtone_mask_is_centered_on_display_midgray() {
        let mid = clarity_step_response(0.48, 0.52);
        let high = clarity_step_response(0.73, 0.77);
        assert!(
            mid > high,
            "掩模中心应在显示中灰：中间对响应 {mid} 应大于高光对 {high}"
        );
    }

    /// A4-1 HDR 探针：线性 0.8（Y' ≈ 0.906）/ 2.0（Y' ≈ 1.354）灰阶对——
    /// 边界低侧（掩模权重 > 0）必须有响应；高侧深处保持 2.0 不被钳回
    /// [0, 1]（旧实现明度钳 1 ⇒ 高侧先被压成 1.0）。Y' ≥ 1 区域掩模
    /// 权重为 0 是**设计语义**（clarity 抑制高光防光晕），不是钳制伪影：
    /// 断言区分两者——值原样保留（2.0）而非被压平（1.0）。
    #[test]
    fn clarity_responds_in_hdr_without_clamping() {
        let mut buffer = Buffer::new(0, 0, 32, 16);
        for y in 0..16u32 {
            for x in 0..32u32 {
                let v = if x < 16 { 0.8 } else { 2.0 };
                buffer.set_pixel(x, y, [v, v, v, 1.0]);
            }
        }
        let before = buffer.clone();
        clarity_adjustment(&mut buffer, 1.0);
        assert_ne!(
            before.pixel(12, 8),
            buffer.pixel(12, 8),
            "边界低侧（Y'≈0.906、掩模权重 ≈0.34）必须有响应"
        );
        let deep = buffer.pixel(28, 8);
        assert!(
            (deep[0] - 2.0).abs() < 1e-4,
            "HDR 高侧应原样保留 2.0（掩模抑制 ≠ 钳到 1.0）：{deep:?}"
        );
    }

    /// A4-1 参照能量带：平滑正弦 Y' 图案（周期 32、幅 0.2）上，生产实现
    /// （引导滤波底座）的 ΔY' 能量与参照实现（高斯底座 + 同掩模 + 同
    /// 亮度比回写）的能量之比应在 ±10% 内（设计 §2 A4-1）。σ = 3.63 为
    /// 标定值：使高斯对本正弦的衰减与引导滤波（r=16、eps=0.005）的
    /// 有效衰减一致（σ=3.4 时实测比 1.264，按衰减公式反推标定）。
    #[test]
    fn clarity_energy_matches_gaussian_reference_within_band() {
        let (w, h) = (64u32, 64u32);
        let plane = |x: u32| 0.5 + 0.2 * libm::sinf(2.0 * std::f32::consts::PI * x as f32 / 32.0);
        let mut buffer = Buffer::new(0, 0, w, h);
        for y in 0..h {
            for x in 0..w {
                let lin = srgb_to_linear(plane(x));
                buffer.set_pixel(x, y, [lin, lin, lin, 1.0]);
            }
        }
        let mut prod = buffer.clone();
        clarity_adjustment(&mut prod, 1.0);
        let energy = |buf: &Buffer| -> f64 {
            buf.pixels()
                .chunks_exact(4)
                .enumerate()
                .map(|(i, p)| {
                    let d = srgb_encode_extended(p[0]) - plane((i as u32) % w);
                    (d * d) as f64
                })
                .sum()
        };
        let e_prod = energy(&prod);
        // 参照：可分离高斯（σ=3.63，截断 ±3σ，边缘复制）+ 掩模 + 亮度比回写。
        let sigma = 3.63f32;
        let radius = (3.0 * sigma).ceil() as i32;
        let kernel: Vec<f32> = (-radius..=radius)
            .map(|t| libm::expf(-(t as f32) * (t as f32) / (2.0 * sigma * sigma)))
            .collect();
        let ksum: f32 = kernel.iter().sum();
        let src: Vec<f32> = (0..w * h).map(|i| plane(i % w)).collect();
        let mut blurred = vec![0.0f32; (w * h) as usize];
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let mut acc = 0.0f32;
                for (ki, t) in (-radius..=radius).enumerate() {
                    let cx = (x + t).clamp(0, w as i32 - 1);
                    acc += kernel[ki] * src[(y * w as i32 + cx) as usize];
                }
                blurred[(y * w as i32 + x) as usize] = acc / ksum;
            }
        }
        // 图案只沿 x 变化，纵向高斯为恒等 ⇒ 只做横向一趟即可。
        let mut e_ref = 0.0f64;
        for (i, yv) in src.iter().enumerate() {
            let detail = yv - blurred[i];
            let centered = 2.0 * yv - 1.0;
            let weight = (1.0 - centered * centered).clamp(0.0, 1.0);
            let new_y = yv + detail * weight;
            let lin = srgb_to_linear(*yv) * (new_y / yv);
            let d = srgb_encode_extended(lin) - yv;
            e_ref += (d * d) as f64;
        }
        let ratio = e_prod / e_ref;
        assert!(
            (0.9..=1.1).contains(&ratio),
            "clarity ΔY' 能量与高斯参照之比应在 ±10% 内：{ratio}（prod {e_prod} vs ref {e_ref}）"
        );
    }
}

/// **第 1440 轮审计锁**：两条「申报≠实现」与「不变量被破」的防回归 ✓。
#[cfg(test)]
mod audit_lock_tests_1440 {
    use super::{denoise, FilterKind};
    use crate::buffer::Buffer;
    use serde_json::json;

    /// **relief 的邻域是 ±1**（`alpha_at(x±1, y±1)` ✓），但 padding 的 match
    /// 原来没有 Relief 臂 ⇒ 落 `_ => 0` ✗ ⇒ 分带渲染在块边界少看一列、与整幅不一致。
    /// 变异：删掉 `Self::Relief => 1` 那条臂 ⇒ 本测试红 ✓。
    #[test]
    fn relief_declares_the_neighbourhood_it_reads() {
        assert_eq!(
            FilterKind::Relief.padding(&json!({})),
            1,
            "relief 读 ±1 邻域 ⇒ padding 必须申报 1"
        );
    }

    /// **预乘不变量 `rgb ≤ alpha`**：√ 空间 YCbCr 逆变换对高色度跳变能推出 rr > 1
    /// ⇒ 旧实现 `rr*rr*a` 不钳 ✗（sharpen/glow/clarity 都钳了，唯独这里漏了）。
    /// 变异：把 `(rr*rr*a).clamp(0.0, a)` 改回 `rr*rr*a` ⇒ 本测试红 ✓。
    #[test]
    fn denoise_keeps_premultiplied_invariant() {
        // 红块紧贴青块的高色度跳变，alpha 取 0.6（不变量必须对任意 alpha 成立 ✓）。
        let mut buffer = Buffer::new(0, 0, 32, 32);
        for y in 0..32 {
            for x in 0..32 {
                let color = if x < 16 {
                    [0.95, 0.05, 0.05]
                } else {
                    [0.05, 0.95, 0.95]
                };
                buffer.set_pixel(x, y, [color[0], color[1], color[2], 0.6]);
            }
        }
        // color=1 ⇒ 内部 chroma_r = 12（大半径抹除，正是能把 rr 推出 1 的参数 ✓）。
        denoise(&mut buffer, 0.5, 0.5, 1.0);
        for chunk in buffer.pixels().chunks_exact(4) {
            let a = chunk[3];
            for channel in 0..3 {
                assert!(
                    chunk[channel] <= a + 1e-6,
                    "预乘不变量被破：rgb={} > alpha={a}（像素 {:?}）",
                    chunk[channel],
                    chunk
                );
            }
        }
    }
}
