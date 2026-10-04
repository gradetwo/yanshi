//! 笔刷动力学与程序化纹理（设计文档 10.3 / 11.1）。
//!
//! `advanced.appearance` 中的 `dynamics`、`seed`、`texture` 三个键在此实现：
//!
//! - [`Dynamics`]：逐印章（stamp）的位置 / 尺寸 / 不透明度 / 角度 / 间距抖动。
//! - [`Texture`]：与坐标绑定的程序化 alpha 纹理（noise / grain）。
//!
//! ## 确定性（D0）
//!
//! 本模块不读取系统时间、不访问网络、不使用线程调度，也不维护任何可变状态：
//! 所有随机量都来自 [`Prng::derive`] 以 `(seed, index)` 派生的独立随机源，
//! 因此 `(seed, index, 输入)` 三元组确定后输出逐位可复现；纹理只依赖整数格点
//! 哈希与坐标，同一坐标在任何图层、任何 tile 分块下都得到同一取值。
//!
//! 本模块是纯结构 / 纯函数：不接触 [`crate::buffer::Buffer`]、`Tile` 或文件系统。

use crate::prng::Prng;
use serde_json::Value;

/// 一个印章（stamp）的动力学输出。
///
/// 仅描述“在哪里、多大、多透明、转多少”，颜色与合成由调用方（笔刷 / 渲染层）决定。
#[derive(Debug, Clone, PartialEq)]
pub struct StampParams {
    /// 抖动后的中心 x（像素，文档坐标）。
    pub x: f64,
    /// 抖动后的中心 y（像素，文档坐标）。
    pub y: f64,
    /// 抖动后的半径（像素，恒 `>= 0`）。
    pub radius: f64,
    /// 抖动后的 alpha（恒落在 `0..=1`）。
    pub alpha: f32,
    /// 随机旋转角（弧度，恒落在 `[0, 2π)`；未开启旋转时为 `0`）。
    pub rotation: f32,
}

/// 笔刷动力学参数（`advanced.appearance` 的 `dynamics` / `seed` 子集）。
///
/// 所有字段都可缺省：缺省值即“无效果”，此时 [`Dynamics::stamp_params`] 原样返回输入。
#[derive(Debug, Clone)]
pub struct Dynamics {
    /// 位置抖动像素半径（`>= 0`）。
    pub jitter: f32,
    /// 随机半径偏移比例（`0..=1`）。
    pub scatter: f32,
    /// 尺寸随机比例（`0..=1`）。
    pub size_variance: f32,
    /// 是否随机旋转印章。
    pub rotation: bool,
    /// 随机角度幅度（弧度，`0..=π`）。
    pub angle_variance: f32,
    /// 间距随机比例（`0..=1`）。
    pub spacing_variance: f32,
    /// 每处印章个数（`1..=4`）。
    pub count: u32,
    /// 不透明度抖动比例（`0..=1`，见 [`Dynamics::from_appearance`] 的 `opacity_jitter`）。
    pub opacity_jitter: f32,
    /// **颜色抖动**（`0..=1` ✓）—— 真实用户提的"**破色**" ✓。
    ///
    /// **用户原话** ✓："每次 `draw_stroke` 都要传完整 hex 颜色 ✓，没有'在画布上混色'的机制 ✗
    /// ⇒ 专业油画的'**破色**'（broken color）与'并置'技巧难以实现 ✓，
    /// agent 只能预先算好几十个 hex 值硬编码" ✗。
    /// **他要的是** ✓："笔触支持 `color_jitter`（**在给定色相邻范围内随机取色** ✓，模拟手调色的不均匀 ✓）" ✓
    /// —— 本字段就是它 ✓，语义**逐字照他的话** ✓。
    ///
    /// **为什么缺省 0 必须逐字节等同** ✓：这是本项目的铁律 ✓ ——
    /// 老文档的观感**一个像素都不能变** ✗ ⇒ `0.0` 时**整段扰动代码都不执行** ✓（不是"乘 0" ✓）。
    pub color_jitter: f32,
    /// 原子种子；所有随机量由它与印章序号派生。
    pub seed: u64,
    /// 程序化纹理。
    pub texture: Texture,
}

impl Default for Dynamics {
    fn default() -> Self {
        Self {
            jitter: 0.0,
            scatter: 0.0,
            size_variance: 0.0,
            rotation: false,
            angle_variance: 0.0,
            spacing_variance: 0.0,
            count: 1,
            opacity_jitter: 0.0,
            color_jitter: 0.0,
            seed: 0,
            texture: Texture::none(),
        }
    }
}

impl Dynamics {
    /// 从 `advanced.appearance` 对象解析动力学参数。
    ///
    /// 读取的键：
    ///
    /// - `dynamics.jitter`：位置抖动像素半径（缺省 `0`，钳制到 `>= 0`）。
    /// - `dynamics.scatter`：随机半径偏移比例（缺省 `0`，钳制到 `0..=1`）。
    /// - `dynamics.size_variance`：尺寸随机比例（缺省 `0`，钳制到 `0..=1`）。
    /// - `dynamics.rotation`：是否随机旋转（缺省 `false`）。
    /// - `dynamics.angle_variance`：随机角度幅度，弧度（缺省 `0`，钳制到 `0..=π`）。
    /// - `dynamics.spacing_variance`：间距随机比例（缺省 `0`，钳制到 `0..=1`）。
    /// - `dynamics.count`：每处印章个数（缺省 `1`，钳制到 `1..=4`）。
    /// - `dynamics.opacity_jitter`：**扩展键**，alpha 抖动比例（缺省 `0`，
    ///   钳制到 `0..=1`）。设计文档只列出了前七项，但需求要求实现 alpha 抖动，
    ///   故以该键承载；缺省时对 alpha 无影响。
    /// - `seed`：整数（缺省 `0`）。
    /// - `texture`：对象或字符串，见 [`Texture::from_value`]。
    ///
    /// 非对象输入（`null`、字符串等）返回 [`Dynamics::default`]。
    pub fn from_appearance(appearance: &Value) -> Self {
        let dynamics = appearance.get("dynamics");
        let seed = appearance
            .get("seed")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let texture = Texture::from_value(appearance.get("texture"));
        Self {
            jitter: clamp_min(read_f32(dynamics, "jitter"), 0.0),
            scatter: clamp_unit(read_f32(dynamics, "scatter")),
            size_variance: clamp_unit(read_f32(dynamics, "size_variance")),
            rotation: read_bool(dynamics, "rotation"),
            angle_variance: clamp_range(
                read_f32(dynamics, "angle_variance"),
                0.0,
                std::f32::consts::PI,
            ),
            spacing_variance: clamp_unit(read_f32(dynamics, "spacing_variance")),
            count: read_count(dynamics),
            opacity_jitter: clamp_unit(read_f32(dynamics, "opacity_jitter")),
            // **扩展键** ✓ `color_jitter`：0 = 关 ✓（缺省 ✓，与既有一致 ✓）。
            color_jitter: clamp_unit(read_f32(dynamics, "color_jitter")),
            seed,
            texture,
        }
    }

    /// 每处印章个数（`1..=4`），供调用方决定循环次数。
    pub fn count(&self) -> u32 {
        self.count.clamp(1, 4)
    }

    /// 是否为“全关闭”动力学：此时 [`Dynamics::stamp_params`] 必须恒等。
    pub fn is_noop(&self) -> bool {
        self.jitter == 0.0
            && self.scatter == 0.0
            && self.size_variance == 0.0
            && !self.rotation
            && self.angle_variance == 0.0
            && self.opacity_jitter == 0.0
            && self.color_jitter == 0.0
    }

    /// 计算第 `index` 个印章的抖动参数。
    ///
    /// `x` / `y` 为印章中心（文档坐标，像素），`radius` 为半径（像素），
    /// `alpha` 为不透明度。随机量全部来自 `Prng::derive(self.seed, index)`,
    /// 因此同一 `(seed, index, x, y, radius, alpha)` 永远给出完全相同的输出。
    ///
    /// 语义与钳制：
    ///
    /// - **位置**：`jitter` 理解为“抖动圆的半径”，在圆内**面积均匀**取偏移
    ///   （`r = jitter·√u`，方向 `θ ∈ [0, 2π)`），这样抖动不会向中心堆积。
    /// - **半径**：`radius · (1 + scatter·s₁ + size_variance·s₂)`，其中 `s₁, s₂ ∈ [-1, 1]`，
    ///   结果钳制到 `>= 0`。两个键单独或同时使用都只在基础半径附近抖动。
    /// - **alpha**：`alpha · (1 - opacity_jitter·u)`，`u ∈ [0, 1)`，结果钳制到 `0..=1`；
    ///   关闭时乘子恒为 `1.0`。
    /// - **旋转**：`rotation` 为真时取 `[0, 2π)` 全随机角；否则取
    ///   `[-angle_variance, angle_variance]`；两者都关闭时为 `0`。
    ///
    /// 若全部动力学关闭（见 [`Dynamics::is_noop`]），输出与输入逐字段相等。
    pub fn stamp_params(&self, index: u64, x: f64, y: f64, radius: f64, alpha: f32) -> StampParams {
        let mut rng = Prng::derive(self.seed, index);

        // 位置抖动：面积均匀的圆盘采样。
        let (dx, dy) = if self.jitter > 0.0 {
            let angle = unit_to(rng.unit(), 0.0, std::f32::consts::TAU);
            let distance = self.jitter * rng.unit().sqrt();
            (
                f64::from(distance * libm::cosf(angle)),
                f64::from(distance * libm::sinf(angle)),
            )
        } else {
            (0.0, 0.0)
        };

        // 半径抖动：scatter 与 size_variance 各自贡献一路有符号比例。
        let mut out_radius = radius;
        if self.scatter > 0.0 {
            out_radius *= f64::from(1.0 + self.scatter * rng.signed());
        }
        if self.size_variance > 0.0 {
            out_radius *= f64::from(1.0 + self.size_variance * rng.signed());
        }

        // alpha 抖动：只减小、不放大，钳制到 0..=1。
        let mut out_alpha = alpha;
        if self.opacity_jitter > 0.0 {
            out_alpha *= 1.0 - self.opacity_jitter * rng.unit();
        }

        // 角度抖动：rotation 优先（全随机），否则用 angle_variance。
        let rotation = if self.rotation {
            unit_to(rng.unit(), 0.0, std::f32::consts::TAU)
        } else if self.angle_variance > 0.0 {
            rng.signed() * self.angle_variance
        } else {
            0.0
        };

        StampParams {
            x: x + dx,
            y: y + dy,
            radius: if radius > 0.0 {
                out_radius.max(0.0)
            } else {
                0.0
            },
            alpha: out_alpha.clamp(0.0, 1.0),
            rotation,
        }
    }

    /// 第 `index` 个印章的间距缩放系数。
    ///
    /// 返回 `1 + spacing_variance·s`（`s ∈ [-1, 1]`，钳制到 `>= 0`）：调用方用它
    /// 缩放基础间距（`spacing` 由笔刷决定），得到“疏密不均”的笔触。
    /// `spacing_variance` 为 `0` 时恒返回 `1.0`。
    pub fn spacing_scale(&self, index: u64) -> f64 {
        if self.spacing_variance <= 0.0 {
            return 1.0;
        }
        let s = Prng::derive(self.seed, index).signed() * self.spacing_variance;
        f64::from(1.0 + s).max(0.0)
    }
}

/// 程序化纹理的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureKind {
    /// 无纹理：对 alpha 无影响。
    None,
    /// 低频取值噪声（柔和的大块明暗）。
    Noise,
    /// 高频颗粒噪声（细密砂粒感）。
    Grain,
}

/// 与坐标绑定的程序化 alpha 纹理（`appearance.texture`）。
///
/// 只依赖整数格点哈希与坐标插值，因此同一坐标在跨图层、跨 tile 分块、跨次渲染时
/// 取值恒定；不引入任何外部依赖，也不使用 [`Prng`] 的可变状态序列。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Texture {
    /// 纹理种类。
    pub kind: TextureKind,
    /// 特征尺度（像素）：相邻格点间距，越小纹理越密。
    pub scale: f32,
    /// 强度（`0..=1`）：`0` 无影响，`1` 时纹理可把 alpha 压到 0。
    pub strength: f32,
    /// 纹理自身的种子。
    pub seed: u64,
}

impl Default for Texture {
    fn default() -> Self {
        Self::none()
    }
}

impl Texture {
    /// 无纹理（对 alpha 无影响）。
    pub fn none() -> Self {
        Self {
            kind: TextureKind::None,
            scale: DEFAULT_NOISE_SCALE,
            strength: 0.0,
            seed: 0,
        }
    }

    /// 从 `appearance.texture` 解析纹理。
    ///
    /// 支持两种写法：
    ///
    /// - 字符串：`"noise"` / `"grain"`，`scale` / `strength` / `seed` 取缺省值；
    /// - 对象：`{"kind": "noise"|"grain", "scale": <f32>, "strength": <0..=1>, "seed": <u64>}`。
    ///
    /// `None`、`null`、未知 `kind`、非法数值都退化为无纹理。`strength` 钳制到 `0..=1`，
    /// `scale` 钳制到 `>= 0.5`（避免除零与采样爆炸）。
    pub fn from_value(value: Option<&Value>) -> Self {
        let Some(value) = value else {
            return Self::none();
        };
        match value {
            // 字符串简写：只有种类，参数取缺省值。
            Value::String(name) => parse_texture_kind(name)
                .map(|kind| Self::with_defaults(kind, 0))
                .unwrap_or_else(Self::none),
            Value::Object(_) => {
                let Some(kind) = value
                    .get("kind")
                    .and_then(Value::as_str)
                    .and_then(parse_texture_kind)
                else {
                    return Self::none();
                };
                let scale = clamp_min(
                    value
                        .get("scale")
                        .and_then(as_f32)
                        .unwrap_or(default_scale(kind)),
                    0.5,
                );
                let strength = clamp_unit(
                    value
                        .get("strength")
                        .and_then(as_f32)
                        .unwrap_or(DEFAULT_STRENGTH),
                );
                let seed = value
                    .get("seed")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                if strength == 0.0 {
                    return Self::none();
                }
                Self {
                    kind,
                    scale,
                    strength,
                    seed,
                }
            }
            _ => Self::none(),
        }
    }

    /// 该种类的缺省 `scale` / `strength`。
    fn with_defaults(kind: TextureKind, seed: u64) -> Self {
        Self {
            kind,
            scale: default_scale(kind),
            strength: DEFAULT_STRENGTH,
            seed,
        }
    }

    /// 是否对 alpha 无影响。
    pub fn is_identity(&self) -> bool {
        self.kind == TextureKind::None || self.strength <= 0.0
    }

    /// 纹理在 `(x, y)` 处对 `alpha` 的影响。
    ///
    /// 语义为乘性插值：`alpha · (1 - strength + strength · n)`，其中 `n ∈ [0, 1]`
    /// 是 `(x, y)` 处的确定性噪声；结果钳制到 `0..=1`。
    /// 无纹理时（[`TextureKind::None`]、`Null` 输入或 `strength == 0`）恒等返回钳制后的 `alpha`。
    ///
    /// 同一坐标恒定：`n` 只由 `(x, y, seed, kind, scale)` 决定，不依赖调用顺序或分块方式。
    pub fn modulate(&self, x: f64, y: f64, alpha: f32) -> f32 {
        let alpha = alpha.clamp(0.0, 1.0);
        if self.is_identity() {
            return alpha;
        }
        let n = self.sample(x, y);
        if !n.is_finite() {
            return alpha;
        }
        (alpha * (1.0 - self.strength + self.strength * n)).clamp(0.0, 1.0)
    }

    /// 归一化噪声值（`0..=1`）；无纹理时返回 `1.0`（即不削弱）。
    pub fn sample(&self, x: f64, y: f64) -> f32 {
        if self.kind == TextureKind::None {
            return 1.0;
        }
        // grain 用更高频率（更细的格子），noise 用低频。
        let frequency = match self.kind {
            TextureKind::Grain => self.scale * GRAIN_FREQUENCY,
            _ => self.scale,
        };
        value_noise(x, y, frequency, self.seed)
    }
}

/// `noise` 的缺省特征尺度（像素）。
const DEFAULT_NOISE_SCALE: f32 = 16.0;
/// 纹理的缺省强度。
const DEFAULT_STRENGTH: f32 = 1.0;
/// `grain` 相对 `noise` 的频率倍数。
const GRAIN_FREQUENCY: f32 = 4.0;
/// 哈希乘法常量（与 [`Prng`] 同族的 splitmix64 常量，保持风格一致）。
const HASH_A: u64 = 0xBF58_476D_1CE4_E5B9;
const HASH_B: u64 = 0x94D0_49BB_1331_11EB;

/// 种类缺省尺度。
fn default_scale(kind: TextureKind) -> f32 {
    match kind {
        TextureKind::Grain => DEFAULT_NOISE_SCALE / GRAIN_FREQUENCY,
        _ => DEFAULT_NOISE_SCALE,
    }
}

/// 解析纹理种类名（未知名返回 `None`）。
fn parse_texture_kind(name: &str) -> Option<TextureKind> {
    match name.trim().to_ascii_lowercase().as_str() {
        "noise" => Some(TextureKind::Noise),
        "grain" => Some(TextureKind::Grain),
        _ => None,
    }
}

/// 读取 JSON 数字为 `f32`（整数与浮点都接受）。
fn as_f32(value: &Value) -> Option<f32> {
    value.as_f64().and_then(|raw| {
        if raw.is_finite() {
            Some(raw as f32)
        } else {
            None
        }
    })
}

/// 从可选对象读取 `f32` 字段。
fn read_f32(object: Option<&Value>, key: &str) -> f32 {
    object
        .and_then(|object| object.get(key))
        .and_then(as_f32)
        .unwrap_or_default()
}

/// 从可选对象读取布尔字段。
fn read_bool(object: Option<&Value>, key: &str) -> bool {
    object
        .and_then(|object| object.get(key))
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

/// 读取 `count` 并钳制到 `1..=4`。
fn read_count(object: Option<&Value>) -> u32 {
    object
        .and_then(|object| object.get("count"))
        .and_then(Value::as_u64)
        .map(|raw| u32::try_from(raw).unwrap_or(u32::MAX).clamp(1, 4))
        .unwrap_or(1)
}

/// 线性映射一个 `[0, 1)` 随机数到 `[low, high)`。
fn unit_to(unit: f32, low: f32, high: f32) -> f32 {
    low + (high - low) * unit
}

/// 钳制到 `0..=1`（NaN 退化为 `0`）。
fn clamp_unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// 钳制到 `>= min`（NaN 退化为 `min`）。
fn clamp_min(value: f32, min: f32) -> f32 {
    if value.is_nan() {
        min
    } else {
        value.max(min)
    }
}

/// 钳制到 `low..=high`（NaN 退化为 `low`）。
fn clamp_range(value: f32, low: f32, high: f32) -> f32 {
    if value.is_nan() {
        low
    } else {
        value.clamp(low, high)
    }
}

/// 整数格点哈希（splitmix64 混合），保证跨平台一致的纯整数运算。
fn lattice_hash(ix: i64, iy: i64, seed: u64) -> u32 {
    let mut z = (ix as u64)
        .wrapping_mul(HASH_A)
        .wrapping_add((iy as u64).wrapping_mul(HASH_B))
        .wrapping_add(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(HASH_A);
    z = (z ^ (z >> 27)).wrapping_mul(HASH_B);
    z ^= z >> 31;
    (z >> 32) as u32
}

/// 格点值归一化到 `[0, 1)`（整数转 f64 精确，再降为 f32，故取值逐位确定）。
fn lattice_value(ix: i64, iy: i64, seed: u64) -> f32 {
    (f64::from(lattice_hash(ix, iy, seed)) / 4_294_967_296.0) as f32
}

/// 取值噪声：整数格点哈希 + 平滑双线性插值，值域 `[0, 1]`。
///
/// `frequency` 为相邻格点间距（像素，`> 0`），`(x, y)` 为文档坐标。
/// 同一坐标永远得到同一取值，与调用顺序、图层、tile 分块无关。
fn value_noise(x: f64, y: f64, frequency: f32, seed: u64) -> f32 {
    if frequency <= 0.0 || !x.is_finite() || !y.is_finite() {
        return 1.0;
    }
    let fx = x / f64::from(frequency);
    let fy = y / f64::from(frequency);
    if !fx.is_finite() || !fy.is_finite() {
        return 1.0;
    }
    // 坐标绝对值过大时 f64 已无法表示相邻格点，退化为不削弱，避免无意义结果。
    if fx.abs() >= 1.0e15 || fy.abs() >= 1.0e15 {
        return 1.0;
    }
    let x0 = fx.floor();
    let y0 = fy.floor();
    let tx = smoothstep(fx - x0);
    let ty = smoothstep(fy - y0);
    let x0 = x0 as i64;
    let y0 = y0 as i64;
    let top = lerp(
        lattice_value(x0, y0, seed),
        lattice_value(x0.wrapping_add(1), y0, seed),
        tx,
    );
    let bottom = lerp(
        lattice_value(x0, y0.wrapping_add(1), seed),
        lattice_value(x0.wrapping_add(1), y0.wrapping_add(1), seed),
        tx,
    );
    lerp(top, bottom, ty).clamp(0.0, 1.0)
}

/// `f32` 双线性插值。
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 平滑插值权重 `t²(3-2t)`（`f64` 计算，保证跨平台一致）。
fn smoothstep(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 全关闭的外观（无 dynamics / seed / texture）。
    fn closed() -> Value {
        json!({})
    }

    /// 开启全部抖动的外观。
    fn wild() -> Value {
        json!({
            "seed": 1,
            "dynamics": {
                "jitter": 5.0,
                "scatter": 0.5,
                "size_variance": 0.25,
                "rotation": true,
                "angle_variance": 1.0,
                "spacing_variance": 0.5,
                "count": 4,
                "opacity_jitter": 0.5,
            },
        })
    }

    #[test]
    fn from_appearance_reads_every_key_and_seed() {
        let dynamics = Dynamics::from_appearance(&wild());
        assert_eq!(dynamics.seed, 1);
        assert_eq!(dynamics.jitter, 5.0);
        assert_eq!(dynamics.scatter, 0.5);
        assert_eq!(dynamics.size_variance, 0.25);
        assert!(dynamics.rotation);
        assert_eq!(dynamics.angle_variance, 1.0);
        assert_eq!(dynamics.spacing_variance, 0.5);
        assert_eq!(dynamics.count(), 4);
        assert_eq!(dynamics.opacity_jitter, 0.5);
        assert!(!dynamics.is_noop());
    }

    #[test]
    fn from_appearance_clamps_out_of_range_keys() {
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": 9,
            "dynamics": {
                "jitter": -4.0,
                "scatter": 7.0,
                "size_variance": -1.0,
                "angle_variance": 9.0,
                "spacing_variance": -0.5,
                "count": 99,
                "opacity_jitter": 3.0,
            },
        }));
        assert_eq!(dynamics.jitter, 0.0);
        assert_eq!(dynamics.scatter, 1.0);
        assert_eq!(dynamics.size_variance, 0.0);
        assert_eq!(dynamics.angle_variance, std::f32::consts::PI);
        assert_eq!(dynamics.spacing_variance, 0.0);
        assert_eq!(dynamics.count(), 4);
        assert_eq!(dynamics.opacity_jitter, 1.0);
    }

    #[test]
    fn from_appearance_tolerates_garbage() {
        let dynamics = Dynamics::from_appearance(&json!("not an object"));
        assert!(dynamics.is_noop());
        assert_eq!(dynamics.seed, 0);
        assert_eq!(dynamics.count(), 1);
        assert!(dynamics.texture.is_identity());
        // 缺省 count 为 1，非法 count 也回落到 1。
        let dynamics = Dynamics::from_appearance(&json!({"dynamics": {"count": "many"}}));
        assert_eq!(dynamics.count(), 1);
    }

    #[test]
    fn closed_dynamics_is_identity() {
        let dynamics = Dynamics::from_appearance(&closed());
        assert!(dynamics.is_noop());
        let params = dynamics.stamp_params(7, 12.5, -3.25, 4.0, 0.75);
        assert_eq!(params.x, 12.5);
        assert_eq!(params.y, -3.25);
        assert_eq!(params.radius, 4.0);
        assert_eq!(params.alpha, 0.75);
        assert_eq!(params.rotation, 0.0);
        assert_eq!(dynamics.spacing_scale(7), 1.0);
        // 任意 index 都是恒等。
        for index in 0..16 {
            let params = dynamics.stamp_params(index, 1.0, 2.0, 3.0, 0.5);
            assert_eq!(params, dynamics.stamp_params(index, 1.0, 2.0, 3.0, 0.5));
            assert_eq!(params.radius, 3.0);
            assert_eq!(params.alpha, 0.5);
        }
    }

    #[test]
    fn same_input_is_bit_identical_across_calls() {
        let dynamics = Dynamics::from_appearance(&wild());
        let first = dynamics.stamp_params(3, 100.0, 200.0, 8.0, 0.9);
        let second = dynamics.stamp_params(3, 100.0, 200.0, 8.0, 0.9);
        assert_eq!(first, second);
        assert_eq!(first.x.to_bits(), second.x.to_bits());
        assert_eq!(first.y.to_bits(), second.y.to_bits());
        assert_eq!(first.radius.to_bits(), second.radius.to_bits());
        assert_eq!(first.alpha.to_bits(), second.alpha.to_bits());
        assert_eq!(first.rotation.to_bits(), second.rotation.to_bits());
        // 重新解析同一外观也必须一致。
        let reparsed = Dynamics::from_appearance(&wild());
        assert_eq!(reparsed.stamp_params(3, 100.0, 200.0, 8.0, 0.9), first);
        assert_eq!(
            reparsed.spacing_scale(3).to_bits(),
            dynamics.spacing_scale(3).to_bits()
        );
    }

    #[test]
    fn different_indices_give_different_jitter() {
        let dynamics = Dynamics::from_appearance(&wild());
        let seen: Vec<StampParams> = (0..8)
            .map(|index| dynamics.stamp_params(index, 50.0, 50.0, 6.0, 1.0))
            .collect();
        assert!(
            seen.windows(2).any(|pair| pair[0] != pair[1]),
            "不同 index 必须给出不同抖动"
        );
        // 至少一对在具体字段上不同。
        assert!(
            seen.iter()
                .any(|params| params.x != seen[0].x || params.radius != seen[0].radius),
            "抖动应体现在 x 或 radius 上"
        );
        // 不同 seed 也应不同。
        let other = Dynamics::from_appearance(&json!({
            "seed": 2,
            "dynamics": {"jitter": 5.0, "scatter": 0.5, "size_variance": 0.25},
        }));
        assert_ne!(other.stamp_params(3, 50.0, 50.0, 6.0, 1.0), seen[3]);
    }

    #[test]
    fn pinned_values_for_seed_one_index_three() {
        // 写死数值：seed=1、index=3、size_variance=1、基础半径 10 ⇒ 半径必须精确等于此值。
        // 该断言由具体数值构成，任何改变派生顺序或公式的改动都会被抓住。
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": 1,
            "dynamics": {"size_variance": 1.0},
        }));
        let params = dynamics.stamp_params(3, 0.0, 0.0, 10.0, 1.0);
        assert_eq!(params.radius, 7.201_895_713_806_152_f64);
        // 同一 seed 下 index 不同，半径不同（写死值亦不同）。
        let other = dynamics.stamp_params(4, 0.0, 0.0, 10.0, 1.0).radius;
        assert_eq!(other, 15.411_670_207_977_295_f64);
        assert_ne!(other, 7.201_895_713_806_152_f64);
        // 位置、alpha、旋转未被开启，必须等于输入。
        assert_eq!(params.x, 0.0);
        assert_eq!(params.y, 0.0);
        assert_eq!(params.alpha, 1.0);
        assert_eq!(params.rotation, 0.0);
    }

    #[test]
    fn pinned_values_for_position_jitter() {
        // jitter 为 5 时，seed=1、index=3 的偏移写死（圆盘采样）。
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": 1,
            "dynamics": {"jitter": 5.0},
        }));
        let params = dynamics.stamp_params(3, 0.0, 0.0, 4.0, 1.0);
        assert_eq!(params.x, -1.989_521_503_448_486_3_f64);
        assert_eq!(params.y, 2.402_004_241_943_359_4_f64);
        // 偏移必落在 jitter 半径内。
        assert!(libm::hypot(params.x, params.y) <= 5.0 + 1.0e-9);
    }

    #[test]
    fn jitter_is_pointwise_deterministic_for_every_index() {
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": 987_654_321,
            "dynamics": {"jitter": 3.0, "scatter": 0.4, "opacity_jitter": 0.3, "rotation": true},
        }));
        for index in 0..64 {
            let a = dynamics.stamp_params(index, 3.5, -2.5, 9.0, 0.6);
            let b = dynamics.stamp_params(index, 3.5, -2.5, 9.0, 0.6);
            assert_eq!(a, b);
            assert!(a.radius >= 0.0);
            assert!((0.0..=1.0).contains(&a.alpha));
            assert!((0.0..std::f32::consts::TAU).contains(&a.rotation));
            let offset = libm::hypot(a.x - 3.5, a.y + 2.5);
            assert!(offset <= 3.0 + 1.0e-6, "offset={offset}");
        }
    }

    #[test]
    fn spacing_scale_is_deterministic_and_bounded() {
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": 42,
            "dynamics": {"spacing_variance": 0.5},
        }));
        for index in 0..32 {
            let a = dynamics.spacing_scale(index);
            let b = dynamics.spacing_scale(index);
            assert_eq!(a.to_bits(), b.to_bits());
            assert!((0.5..=1.5).contains(&a), "scale={a}");
        }
        let closed = Dynamics::from_appearance(&closed());
        assert_eq!(closed.spacing_scale(3), 1.0);
    }

    #[test]
    fn extreme_inputs_are_clamped_without_panic() {
        let dynamics = Dynamics::from_appearance(&json!({
            "seed": u64::MAX,
            "dynamics": {
                "jitter": 1.0e30,
                "scatter": 1.0,
                "size_variance": 1.0,
                "rotation": true,
                "angle_variance": 3.0,
                "spacing_variance": 1.0,
                "count": 4,
                "opacity_jitter": 1.0,
            },
        }));
        // radius = 0 必须保持 0，且 alpha=1 被钳制在 0..=1。
        let params = dynamics.stamp_params(u64::MAX, 0.0, 0.0, 0.0, 1.0);
        assert_eq!(params.radius, 0.0);
        assert!(params.alpha <= 1.0 && params.alpha >= 0.0);
        assert!(params.x.is_finite() && params.y.is_finite());
        assert!((0.0..std::f32::consts::TAU).contains(&params.rotation));
        // alpha 输入越界时输出仍被钳制。
        let params = dynamics.stamp_params(1, 0.0, 0.0, 5.0, 4.0);
        assert_eq!(params.alpha, 1.0);
        let params = dynamics.stamp_params(1, 0.0, 0.0, 5.0, -3.0);
        assert_eq!(params.alpha, 0.0);
        // 负数半径被夹到 0，而不是产生 NaN 或负值。
        let params = dynamics.stamp_params(2, 1.0, 1.0, -8.0, 0.5);
        assert_eq!(params.radius, 0.0);
        // 极大间距比例仍非负。
        assert!(dynamics.spacing_scale(u64::MAX) >= 0.0);
    }

    #[test]
    fn texture_none_is_identity_modulation() {
        let texture = Texture::from_value(None);
        assert!(texture.is_identity());
        for (x, y, alpha) in [
            (0.0, 0.0, 0.0_f32),
            (1.5, 2.5, 0.25),
            (-100.0, 99.0, 0.5),
            (1.0e9, -1.0e9, 1.0),
        ] {
            assert_eq!(texture.modulate(x, y, alpha), alpha);
        }
        // null / 未知 kind / strength=0 同样恒等。
        // 注意 `{"kind":"noise"}` 缺省 strength=1，是有纹理，不在本列表内。
        for value in [
            json!(null),
            json!("unknown"),
            json!({}),
            json!({"kind": "nope", "strength": 1.0}),
            json!({"kind": "noise", "strength": 0.0}),
        ] {
            let texture = Texture::from_value(Some(&value));
            assert!(texture.is_identity(), "value={value}");
            assert_eq!(texture.modulate(3.0, 4.0, 0.7), 0.7);
        }
        // 字符串简写等价于缺省参数对象。
        assert_eq!(
            Texture::from_value(Some(&json!("noise"))).kind,
            TextureKind::Noise
        );
        assert_eq!(
            Texture::from_value(Some(&json!("grain"))).kind,
            TextureKind::Grain
        );
    }

    #[test]
    fn noise_texture_is_constant_at_a_coordinate() {
        let texture = Texture::from_value(Some(&json!({
            "kind": "noise",
            "scale": 8.0,
            "strength": 1.0,
            "seed": 7,
        })));
        assert!(!texture.is_identity());
        for (x, y) in [
            (0.0, 0.0),
            (0.5, 0.5),
            (12.25, -3.75),
            (-1024.0, 2048.0),
            (1.0e6 + 0.5, 3.25),
        ] {
            let base = texture.sample(x, y);
            let again = texture.sample(x, y);
            assert_eq!(base.to_bits(), again.to_bits(), "坐标 ({x}, {y}) 必须恒定");
            assert!((0.0..=1.0).contains(&base), "noise={base}");
            assert_eq!(
                texture.modulate(x, y, 0.8).to_bits(),
                texture.modulate(x, y, 0.8).to_bits()
            );
        }
        // 不同 seed / kind 在同一坐标给出不同纹理。
        let other = Texture::from_value(Some(&json!({
            "kind": "noise", "scale": 8.0, "strength": 1.0, "seed": 8,
        })));
        assert_ne!(texture.sample(12.25, -3.75), other.sample(12.25, -3.75));
        let grain = Texture::from_value(Some(&json!({
            "kind": "grain", "scale": 8.0, "strength": 1.0, "seed": 7,
        })));
        assert_ne!(texture.sample(12.25, -3.75), grain.sample(12.25, -3.75));
    }

    #[test]
    fn texture_modulation_stays_in_unit_range_and_uses_strength() {
        let texture = Texture::from_value(Some(&json!({
            "kind": "grain",
            "scale": 2.0,
            "strength": 0.5,
            "seed": 12345,
        })));
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for step in 0..4096 {
            let x = f64::from(step) * 0.37;
            let y = f64::from(step) * -0.11;
            let alpha = 1.0_f32;
            let out = texture.modulate(x, y, alpha);
            assert!((0.0..=1.0).contains(&out), "out={out}");
            assert!(out <= alpha, "纹理只能削弱，不能放大");
            assert!(out >= alpha * (1.0 - 0.5) - 1.0e-6);
            min = min.min(out);
            max = max.max(out);
        }
        assert!(max - min > 0.05, "纹理应有可见起伏：min={min} max={max}");
    }

    #[test]
    fn texture_extreme_values_do_not_panic() {
        // 极端 scale / strength / seed。
        let texture = Texture::from_value(Some(&json!({
            "kind": "noise",
            "scale": 1.0e30,
            "strength": 1.0e30,
            "seed": u64::MAX,
        })));
        assert!((0.0..=1.0).contains(&texture.modulate(0.0, 0.0, 0.5)));
        let texture = Texture::from_value(Some(&json!({
            "kind": "noise",
            "scale": -100.0,
            "strength": -5.0,
            "seed": 1,
        })));
        assert!(texture.is_identity());
        assert_eq!(texture.modulate(0.0, 0.0, 0.5), 0.5);
        // 极端坐标（含非有限值）不 panic。
        let texture = Texture::from_value(Some(&json!({
            "kind": "grain", "scale": 4.0, "strength": 1.0, "seed": 3,
        })));
        for (x, y) in [
            (f64::MAX, f64::MIN),
            (1.0e300, -1.0e300),
            (f64::NAN, 0.0),
            (f64::INFINITY, f64::NEG_INFINITY),
        ] {
            let out = texture.modulate(x, y, 0.5);
            assert!((0.0..=1.0).contains(&out), "out={out}");
        }
    }

    #[test]
    fn texture_parse_uses_kind_defaults_and_seed() {
        let noise = Texture::from_value(Some(&json!({"kind": "noise"})));
        assert_eq!(noise.kind, TextureKind::Noise);
        assert_eq!(noise.scale, DEFAULT_NOISE_SCALE);
        assert_eq!(noise.strength, DEFAULT_STRENGTH);
        let grain = Texture::from_value(Some(&json!({"kind": "grain"})));
        assert_eq!(grain.scale, DEFAULT_NOISE_SCALE / GRAIN_FREQUENCY);
        // scale 下限保护：过小的 scale 被夹到 0.5。
        let tiny = Texture::from_value(Some(
            &json!({"kind": "noise", "scale": 0.0, "strength": 1.0}),
        ));
        assert_eq!(tiny.scale, 0.5);
        // 缺省 seed 为 0（整数）。
        assert_eq!(noise.seed, 0);
        assert_eq!(Texture::default(), Texture::none());
    }
}
