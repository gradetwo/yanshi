//! 油画 / 水彩介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 与 v1 的差别是**宿主提供上下文** ✓：插件在 wasm 里读不到画布 ✗，所以宿主把
//! 「笔尖色 / 目标处已有色 / 载墨 / 湿度」写进插件的**输入缓冲** ✓，插件据此模拟颜料行为 ✓：
//!
//! * **载墨（load）** ✓：每一笔消耗载体里的颜料 ⇒ 越画越淡 ✓，`load` 用尽即停笔 ✓（模拟"没颜料了" ✓）；
//! * **混色（mixing）** ✓：笔尖颜色被**目标处已有色**按湿度拉过去 ⇒ 湿画法里颜色互相吃掉 ✓；
//! * **鬃毛纹理** ✓：每个点由注入的 `seed` 派生一组"鬃毛"通道 ✓ ⇒ 条纹方向固定、可复现 ✓；
//! * **干湿边缘** ✓：边缘略深、内部略浅 ✓（油画堆料的观感 ✓）；
//! * **纹理强度（texture）** ✓：把上面那条鬃毛条纹**与**颗粒的**对比**按需削平 ✓ ——
//!   0 = 原来那支鬃刷（默认 ✓）、1 = 近乎平坦的底涂 ✓。
//!   加这个控件是因为**真实用户反馈**：`size=46`、`wetness=0.65`、密排平行笔触时
//!   整片呈"麻布 / 编织垫"格纹 ✗ ⇒ 艺术家铺不出平滑底色 ✗ ⇒ "多强"该由画的人定 ✓。
//!   削的是**对比**、不是**墨量** ✓（向**这枚 dab 自己夹过之后的**等效电平收 ✓，见 `oil_flat_coverage` ✓）⇒
//!   拉高纹理不会让底涂变浓**也不会变淡** ✓、更不会引入任何新的随机源 ✓（逐字节可复现 ✓）。
//!
//! 边界与 v1 相同 ✓：**零依赖、不导入任何宿主函数** ✓（宿主加载时校验 `imports` 为空 ✓）、
//! 随机性只来自 `seed` ✓、输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（v1 仍是 1 ✓ —— 两个版本并存正是"id + version 随对象记录"要支持的 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素），宿主据此施加配额 ✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9)、纹理强度(10) —— 共 **11** 个 f32 ✓。
///（第一版写成 8 ✗，于是 `input[8 % 8]` 读回了笔尖色 ✓ —— 字段数必须与下标一致 ✓。）
///
/// **纯增量** ✓：`texture` 只**追加**在下标 10 ✓，前 10 个槽的语义与排布一个字节不动 ✓
/// ⇒ 旧宿主只写前 10 个槽 ✓、第 11 个槽在 wasm 线性内存与原生静态里都是 0.0 ✓
/// ⇒ 旧行为**逐字节不变** ✓（`texture_zero_matches_prechange_golden` 用改动前的黄金值钉住这一点 ✓）。
/// 宿主判断"这个字段在不在"用 `yanshi_input_len() >= 44` ✓ ⇒
/// `ABI_VERSION` **保持 2** ✓（加字段 ≠ 改契约 ✓）。
pub const INPUT_FLOATS: u32 = 11;

/// 鬃毛的通道条数（一个笔尖直径里画几道条纹 ✓）。
const BRISTLES: f32 = 7.0;

/// 鬃毛调制的**连续相位解析均值** ✓：`bristle = 0.55 + 0.45 * smoothstep(fract)` ✓，
/// `fract` 若在 [0,1) 上均匀 ⇒ `smoothstep` 均值恰为 0.5 ✓ ⇒ `0.55 + 0.45*0.5 = 0.775` ✓。
///
/// **它只是个退化情形的占位** ✓，不是削平的目标 ✗ —— 真实采样有**两处**让它偏掉：
///
/// 1. `f32::fract()` 对**负数返回负数** ✓（`(-3.2_f32).fract() == -0.2` ✓），
///    而 `ridge` 的自变量 `across/radius * 3.5 + 0.5` 值域是 **[-3, 4]** ✓
///    ⇒ 有近一半像素的 `ridge < 0` ✓ ⇒ `smoothstep(ridge)` 可以到 **+3.9** ✓
///    ⇒ `bristle` 能冲到 **≈2.3**（它**不是** [0.55,1.0] 的有界量 ✗）。
///    （这是原有代码就有的行为 ✓，**不能顺手改** ✗ —— 那会让 `texture = 0` 不再逐字节相同 ✗。）
/// 2. 于是主循环的 `coverage.clamp(0.0, 1.0)` **真的会夹住**这些高峰 ✓
///    ⇒ 有效均值既不是 0.775、也不是未夹的 ≈0.92 ✓。
///
/// 所以真正的目标是**这枚 dab 自己的、夹过之后的加权均值** ✓（`oil_flat_coverage` 现量 ✓）。
/// 这条常量只在"量不出来"时兜底 ✓（`pressure = 0` ⇒ 整枚 dab 本来就全透明 ✓，取什么都一样 ✓）。
const BRISTLE_MEAN: f32 = 0.775;

/// 颗粒调制的**解析均值** ✓：`grain = 1.0 - 0.15 * u` ✓，`u` 均匀 ∈ [0,1) ⇒ 均值 **1.0 - 0.075 = 0.925** ✓。
///
/// 颗粒**故意不逐 dab 现量** ✗：它逐像素独立 ✓、样本量大（`size²` 个 ✓）、振幅又小（0.15 ✓）
/// ⇒ 覆盖率加权的样本均值与解析值实测只差 **0.0x%**（`size = 3` 这种 9 像素的极小 dab 才到 2% ✓）✓。
/// 而"现量"要么**多抽一遍随机数** ✗（那会挪动随机流 ⇒ `texture = 0` 不再逐字节相同 ✗），
/// 要么把 `size²` 个颗粒先存进静态数组 ✗（再多 16KB 静态内存 ✓，只在第二遍用得上 ✓）——
/// 为 0.0x% 付这个代价不值 ✓。
const GRAIN_MEAN: f32 = 0.925;

/// 图案：**只由像素位置与 `seed` 定的鬃毛方向**决定 ✓（**不碰随机流** ✓）。
///
/// 抽成函数是为了让纹理强度能**跑两遍** ✓：第一遍量这枚 dab 的加权平均 ✓、
/// 第二遍才落墨 ✓。两遍都不抽随机数 ✓ ⇒ 随机流的时序与改动前**一模一样** ✓
///（这是"`texture = 0` 逐字节不变"的前提 ✓）。
#[inline]
fn oil_pattern(dx: f32, dy: f32, radius: f32, cos_a: f32, sin_a: f32) -> (f32, f32) {
    // 圆形笔尖的软边 ✓。
    let edge = (1.0 - (dx * dx + dy * dy).sqrt() / radius).clamp(0.0, 1.0);
    // 鬃毛：沿垂直方向的条纹 ⇒ 油画刷痕 ✓（方向由 seed 固定 ✓）。
    let across = dx * cos_a + dy * sin_a;
    let ridge = ((across / radius) * BRISTLES * 0.5 + 0.5).fract();
    let bristle = 0.55 + 0.45 * (3.0 * ridge * ridge - 2.0 * ridge * ridge * ridge);
    (edge, bristle)
}

/// **这枚 dab 自己的**、**夹过之后**的加权平均覆盖电平 ✓ —— 削平的目标 ✓。
///
/// 权重与主循环的覆盖率**同形** ✓：`edge^0.5 * (0.75 + 0.45*rim) * pressure * load` ✓；
/// 而累加的是 **`clamp(权重 * bristle)`** ✓（不是裸的 `bristle` ✗）——
/// 因为主循环真正夹的是**整条覆盖率** ✓，而 `bristle` 能冲到 2.3（见 `BRISTLE_MEAN` 那条 ✓）
/// ⇒ 高峰像素有一部分墨**被夹掉了** ✓。只有把夹掉的这一份算进目标 ✓，
/// `texture = 1` 才能既不压暗、也不提亮 ✓（否则实测会亮 **+2.9%** ✗ —— 试过 ✓）。
///
/// 返回值是"**等效鬃毛电平**" ✓（不是 `[0.55,1.0]` 区间里的平均鬃毛 ✗）：
/// 它满足 `Σ clamp(权重 * 电平) == Σ clamp(权重 * bristle)` 的加权版本 ✓
/// ⇒ 把它当常数代进去，整枚 dab 的覆盖率之和**按定义不变** ✓，
/// 于是"削对比、不改墨量"是可以写成测试的硬承诺 ✓（`texture_keeps_the_average_ink` ✓）。
///
/// 量"自己的"而不是用常数 ✗：实测离散采样下这个等效电平 ≈ **0.81**（`size = 46` ✓），
/// 而且**逐 seed / 逐 size** 都不同 ✓ ⇒ 用常数当目标，`texture = 1` 会把 dab 整体
/// 压暗或提亮好几个百分点 ✗（艺术家一拉平滑、墨色就变 ✗ —— 那不是"更平滑" ✓，
/// 那是**换了颜色** ✗）。
///
/// 只在 `texture > 0` 时调用 ✓：默认路径（旧宿主 ✓）连这一遍都不跑 ✓ ⇒ 无性能回归 ✓。
fn oil_flat_coverage(
    size: u32,
    radius: f32,
    cos_a: f32,
    sin_a: f32,
    pressure: f32,
    load: f32,
) -> f32 {
    let mut weight_sum = 0.0_f32;
    let mut coverage_sum = 0.0_f32;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - radius;
            let dy = y as f32 + 0.5 - radius;
            let (edge, bristle) = oil_pattern(dx, dy, radius, cos_a, sin_a);
            if edge <= 0.0 {
                continue;
            }
            let weight = edge.sqrt() * (0.75 + 0.45 * (1.0 - edge)) * pressure * load;
            weight_sum += weight;
            coverage_sum += (weight * bristle).clamp(0.0, 1.0);
        }
    }
    if weight_sum > 0.0 {
        coverage_sum / weight_sum
    } else {
        // `pressure = 0` ⇒ 每一像素的权重都是 0 ⇒ 整枚 dab 全透明 ✓ ⇒ 目标取什么都一样 ✓。
        BRISTLE_MEAN
    }
}

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

/// 插件 ABI 版本 ✓。
fn oil_abi_version_body() -> u32 {
    ABI_VERSION
}

fn oil_max_dab_body() -> u32 {
    MAX_DAB
}

/// 输入缓冲地址（宿主写入上下文 ✓）。
fn oil_input_ptr_body() -> usize {
    core::ptr::addr_of!(INPUT) as usize
}

/// 输入缓冲长度（字节）✓。
fn oil_input_len_body() -> u32 {
    INPUT_FLOATS * 4
}

/// 输出缓冲地址 ✓。
fn oil_dab_ptr_body() -> usize {
    core::ptr::addr_of!(DAB) as usize
}

/// 把一项调制**向它的均值收** ✓：`t = 0` 原样 ✓、`t = 1` 完全变平 ✓。
///
/// 为什么要写成 `x + (mean - x) * t` 这种"看起来啰嗦"的形式 ✗ 而不写
/// `x * (1.0 - t) + mean * t` ✓：**因为 `t = 0` 必须逐位还原** ✓。
/// 后一种写法在 `t = 0` 时算的是 `x * 1.0 + mean * 0.0` ✓ —— 结果虽然也等于 `x` ✓，
/// 但多了一次乘法与一次加法 ✓；而前一种在 `t = 0` 时是 `x + (mean - x) * 0.0` ✓，
/// `(mean - x) * 0.0` 在 IEEE 下**精确**是 `±0.0` ✓，`x + ±0.0 == x`（`x` 有限 ✓）✓
/// ⇒ 加法也退化成恒等 ✓。本文件是 `no_std` 风格、默认**不做 fast-math** ✓
/// ⇒ 编译器不会用 FMA 把这两步重排掉 ✓ ⇒ 恒等是**结构上**成立的 ✓，不是碰运气 ✓。
#[inline]
fn toward_mean(value: f32, mean: f32, t: f32) -> f32 {
    value + (mean - value) * t
}

/// splitmix64：确定、无外部状态 ✓。
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 落一个点：`seed` 驱动鬃毛与颗粒 ✓，宿主上下文决定颜色与浓淡 ✓。
///
/// 返回写入的字节数（`size * size * 4` ✓）。
fn oil_dab_body(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    // `clamp(1, MAX_DAB)` 与原写法**语义等价**：`size: u32` 除 0 外都 ≥ 1，
    // 且 `MAX_DAB >= 1` ⇒ 不会触发 `clamp` 的 `max < min` panic。
    let size = size.clamp(1, MAX_DAB);
    let pressure = (pressure_milli.min(1000)) as f32 / 1000.0;
    // SAFETY：单线程 wasm，只读写自己声明的固定缓冲 ✓。
    unsafe {
        let input = &*core::ptr::addr_of!(INPUT);
        let (tip_r, tip_g, tip_b, tip_a) = (input[0], input[1], input[2], input[3]);
        let (dst_r, dst_g, dst_b, _dst_a) = (input[4], input[5], input[6], input[7]);
        let load = input[8];
        let wetness = input[9];
        // 纹理强度 ✓：**纯增量**字段 ✓ —— 旧宿主只写前 10 个槽 ⇒ 这里是 0.0 ⇒ 下面两处
        // `toward_mean` 退化为恒等 ✓ ⇒ 与改动前逐字节相同 ✓。
        // 先夹到 0..=1 ✓（要求里的"低于 0 按 0、高于 1 按 1" ✓）；
        // NaN 单独按 0.0 处理 ✓：`clamp` 对 NaN 返回 NaN ✓ ⇒ 整枚 dab 的 alpha 都会被
        // `as u8` 的饱和转换打成 0 ✗（宿主写坏一个槽就整笔画不出来 ✗ —— 不值得 ✓）。
        let texture = input[10].clamp(0.0, 1.0);
        let texture = if texture.is_nan() { 0.0 } else { texture };
        if load <= 0.0 || tip_a <= 0.0 {
            return 0; // 没颜料了 ✓（模拟载墨耗尽 ✓）。
        }
        let radius = size as f32 / 2.0;
        let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
        // 每笔一次"鬃毛方向"，由 seed 决定 ⇒ 同一笔条纹一致、可复现 ✓。
        let bristle_angle =
            (splitmix64(&mut state) as f32 / u32::MAX as f32) * core::f32::consts::PI;
        let (sin_a, cos_a) = (bristle_angle.sin(), bristle_angle.cos());
        // **第一遍（只在 `texture > 0` 时跑）**：量这枚 dab 自己**夹过之后**的加权平均覆盖电平 ✓。
        // `texture = 0` 时取占位值即可 ✓ —— 反正下面 `toward_mean(.., 0.0)` 是恒等 ✓
        // ⇒ 默认路径既不多跑一遍、也不改变任何一位 ✓。
        let flat_level = if texture > 0.0 {
            oil_flat_coverage(size, radius, cos_a, sin_a, pressure, load)
        } else {
            BRISTLE_MEAN
        };
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - radius;
                let dy = y as f32 + 0.5 - radius;
                let (edge, bristle) = oil_pattern(dx, dy, radius, cos_a, sin_a);
                if edge <= 0.0 {
                    let index = ((y * size + x) * 4) as usize;
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                // **纹理强度**削的第一项：条纹**对比** ✓ ——
                // 向**这枚 dab 自己的**等效电平收 ✓（不是向常数、更不是向 1.0 提亮 ✗）。
                let bristle = toward_mean(bristle, flat_level, texture);
                // 干湿边缘：靠近外沿略深（堆料边缘 ✓），内部略浅 ✓。
                let rim = 1.0 - edge;
                let body = bristle * (0.75 + 0.45 * rim);
                // 混色：湿画法里笔尖被目标色按湿度拉过去 ✓。
                let pull = (wetness * 0.65).clamp(0.0, 0.95);
                let mix = |tip: f32, dst: f32| tip * (1.0 - pull) + dst * pull;
                let coverage = (edge.sqrt() * body * pressure * load).clamp(0.0, 1.0);
                // 颗粒：极轻的随机扰动 ⇒ 颜料不是塑料感 ✓（同 seed 可复现 ✓）。
                let grain = 1.0 - 0.15 * ((splitmix64(&mut state) >> 40) as f32 / 16_777_215.0);
                // **纹理强度**削的第二项：逐像素**噪点对比** ✓（同上，向均值收 ✓）。
                // 关键约束 ✓：上面那一次 `splitmix64` **一次都不能省** ✗ ——
                // 省掉它会挪动后续所有像素的随机流 ✓ ⇒ 连 `texture = 0.0` 都不再逐字节相同 ✗
                //（这是整处改动里最脆的一点 ✓：纹理强度只允许**缩放已抽到的随机数** ✓，
                //  绝不允许改变抽取的**次数** ✓）。
                let grain = toward_mean(grain, GRAIN_MEAN, texture);
                let index = ((y * size + x) * 4) as usize;
                dab[index] = ((mix(tip_r, dst_r) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 1] = ((mix(tip_g, dst_g) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 2] = ((mix(tip_b, dst_b) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 3] = ((coverage * grain) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
    size * size * 4
}

// **导出名的平台分叉** ✓（这是让"六个插件能链进同一个二进制"的关键 ✓）：
//
// * **wasm 构建照旧导出 ABI 名** ✓（`yanshi_dab` 等 ✓）—— **已发布的契约一个字节不改** ✓；
// * **原生构建导出唯一名** ✓（`yanshi_oil_dab` 等 ✓）—— 六个插件的符号因此不再冲突 ✓，
//   服务端于是能把它们**全部链进来** ✓（真实用户 P1-3 要的正是这件事 ✓）。
//
// 之所以不直接把旧名改掉 ✗：那会**破坏浏览器端的官方 ABI** ✓ —— 没必要 ✓。

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    oil_abi_version_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_abi_version() -> u32 {
    oil_abi_version_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    oil_max_dab_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_max_dab() -> u32 {
    oil_max_dab_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> usize {
    oil_input_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_input_ptr() -> usize {
    oil_input_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    oil_input_len_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_input_len() -> u32 {
    oil_input_len_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> usize {
    oil_dab_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_dab_ptr() -> usize {
    oil_dab_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    oil_dab_body(seed, size, pressure_milli)
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_oil_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    oil_dab_body(seed, size, pressure_milli)
}

// **本 crate 的第一批测试** ✓ —— 它们钉住的正是这次改动最容易悄悄破坏的三件事 ✓：
// 「旧宿主逐字节不变」✓、「纹理拉高确实更平」✓、「同一输入永远同一字节」✓。

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering};

    /// 上下文（照 ABI v2 的前 10 个 f32 ✓）：笔尖 RGBA / 目标 RGBA / 载墨 / 湿度 ✓。
    /// 固定不变 ✓ ⇒ 用例之间**只差 `texture`** ✓（有差别才说明是这个控件在起作用 ✓）。
    /// 数值取用户报告的那附近 ✓（`wetness = 0.65` ✓），笔尖半径取 23 ⇒ `size = 46` ✓。
    const CONTEXT: [f32; 10] = [
        0.70, 0.25, 0.10, 1.0, // 笔尖色 RGBA ✓
        0.20, 0.30, 0.40, 1.0,  // 目标处已有色 RGBA ✓
        0.90, // 载墨 ✓
        0.65, // 湿度 ✓
    ];

    /// 输入/输出缓冲都是 `static mut` ✓（全局的 ✓），而测试**默认多线程并行** ✓
    /// ⇒ 不加锁就是数据竞争 ✗（"逐字节相同"这类断言会随机翻车 ✗，而且是那种
    /// "本地过、CI 挂"的翻车 ✗）。这里用 `core` 的原子量自旋 ✓ ——
    /// 顺手也就不必为了一个测试锁给"至今不碰 std"的 crate 破例 ✓。
    static LOCK: AtomicBool = AtomicBool::new(false);

    /// 锁的守卫 ✓：`Drop` 即释放 ✓ ⇒ 用例 assert 失败 panic 时**也会**释放 ✓
    ///（手写 `unlock()` 会在 panic 路径上把后面所有用例一起饿死 ✗）。
    struct Guard;

    impl Guard {
        fn acquire() -> Guard {
            while LOCK.swap(true, Ordering::Acquire) {
                core::hint::spin_loop();
            }
            Guard
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            LOCK.store(false, Ordering::Release);
        }
    }

    /// 渲染一枚 dab ✓，返回 RGBA 字节 ✓（`written` 与缓冲长度都已核对 ✓）。
    ///
    /// `texture = None` 用来**模拟旧宿主** ✓：只写前 10 个槽 ✓、第 11 个槽**根本不碰** ✓。
    /// 开始前先把第 11 槽清成 0.0 ✓：新宿主"没有这个概念"时它本来就是 0.0 ✓
    ///（wasm 线性内存零初始化 ✓、原生静态零初始化 ✓）——
    /// 清这一下只是为了让本用例不受**别的用例**写过的值污染 ✓（缓冲是共享的 ✓）。
    fn render(seed: u32, size: u32, pressure_milli: u32, texture: Option<f32>) -> Vec<u8> {
        let _guard = Guard::acquire();
        unsafe {
            let buffer = &mut *core::ptr::addr_of_mut!(INPUT);
            buffer[10] = 0.0;
            for (slot, value) in buffer.iter_mut().zip(CONTEXT.iter()) {
                *slot = *value;
            }
            if let Some(value) = texture {
                buffer[10] = value;
            }
        }
        let written = oil_dab_body(seed, size, pressure_milli);
        assert_eq!(written, size * size * 4, "落墨字节数应当是 size²×4");
        // SAFETY：刚写完的固定缓冲 ✓，长度由上面那行核对过 ✓。
        unsafe {
            core::slice::from_raw_parts(core::ptr::addr_of!(DAB) as *const u8, written as usize)
                .to_vec()
        }
    }

    /// FNV-1a 64 ✓：把"整块字节"压成一个能对着**改动前**比较的数字 ✓
    ///（零依赖 ✓、确定性 ✓、实现只有三行 ⇒ 出错的机会比哈希库还小 ✓）。
    fn fnv1a64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
        }
        hash
    }

    /// 整枚 dab 的 alpha 平均 ✓（"墨量"的可测代理 ✓）。
    fn mean_alpha(bytes: &[u8]) -> f64 {
        let sum: f64 = bytes.iter().skip(3).step_by(4).map(|a| *a as f64).sum();
        sum / (bytes.len() / 4) as f64
    }

    /// 平滑度度量一：**同一行内横向相邻像素 alpha 的平均绝对差** ✓（越高频越敏感 ✓）。
    ///
    /// 为什么不用"整幅 alpha 的标准差"当主度量 ✗：圆笔尖的**软边**本身就贡献巨大方差 ✓，
    /// 而两个版本里那圈软边**逐位一模一样** ✓ ⇒ 它会把纹理的差别**稀释**掉 ✗
    ///（差别是真的、只是被淹没 ✓）。相邻差只看**高频** ✓：
    /// 鬃毛条纹（周期 ≈ 2r/7 ≈ 6.6px ✓）与逐像素颗粒都在这里 ✓，
    /// 而软边在同一行内几乎不变 ✓ ⇒ 这个度量**专治**"格纹" ✓。
    fn alpha_step_mean(bytes: &[u8], size: u32) -> f64 {
        let size = size as usize;
        let mut sum = 0.0_f64;
        let mut count = 0_usize;
        for y in 0..size {
            for x in 0..size - 1 {
                let left = bytes[(y * size + x) * 4 + 3] as f64;
                let right = bytes[(y * size + x + 1) * 4 + 3] as f64;
                sum += (left - right).abs();
                count += 1;
            }
        }
        assert!(count > 0, "至少要有一对相邻像素");
        sum / count as f64
    }

    /// 平滑度度量二：**alpha 的标准差** ✓（要求里提到的另一种度量 ✓，当佐证 ✓）。
    /// 它含软边那一份不变的方差 ✓ ⇒ 期望的下降幅度会比度量一小 ✓，但方向必须一致 ✓。
    fn alpha_std_dev(bytes: &[u8], size: u32) -> f64 {
        // 按下标取 alpha ✓（不用 `chunks_exact(4)` ✓ —— clippy 会建议改用 `as_chunks` ✓，
        // 而 `as_chunks` 在这个工具链上还没稳定 ✗ ⇒ 不值得为一个测试引 nightly ✓）。
        let alphas: Vec<f64> = (0..bytes.len() / 4)
            .map(|pixel| bytes[pixel * 4 + 3] as f64)
            .collect();
        let count = alphas.len();
        assert!(count > 0, "dab 不该是空的");
        assert_eq!(count, (size * size) as usize);
        let mean = alphas.iter().sum::<f64>() / count as f64;
        let variance = alphas.iter().map(|a| (a - mean) * (a - mean)).sum::<f64>() / count as f64;
        variance.sqrt()
    }

    /// **最要紧的那一条** ✓：`texture` **缺席**（旧宿主只写 10 个槽 ✓）与
    /// **显式 0.0** 必须**逐字节相同** ✓，而且缓冲非空、版本仍是 2、`input_len` 是 44 ✓。
    #[test]
    fn texture_zero_is_byte_identical_to_before() {
        let absent = render(1, 46, 1000, None);
        let explicit_zero = render(1, 46, 1000, Some(0.0));
        assert!(!absent.is_empty(), "dab 不该是空的");
        assert_eq!(
            absent, explicit_zero,
            "texture 缺席（旧宿主不写第 11 槽）必须与显式 0.0 逐字节相同"
        );

        // ABI：纯增量 ⇒ 长度长了一个 f32、版本**不动** ✓。
        assert_eq!(INPUT_FLOATS, 11, "输入缓冲涨到 11 个 f32");
        assert_eq!(
            oil_input_len_body(),
            44,
            "yanshi_input_len() 应当是 44 字节"
        );
        assert_eq!(
            yanshi_oil_input_len(),
            44,
            "原生导出名（wasm 下就叫 yanshi_input_len）同样自报 44"
        );
        assert_eq!(oil_abi_version_body(), 2, "版本保持 2");
        assert_eq!(
            yanshi_oil_abi_version(),
            2,
            "宿主靠 input_len >= 44 发现新字段 ⇒ 版本不必、也不该动"
        );
    }

    /// **对着改动前比较** ✓：下面三个哈希是**改之前那版代码**跑出来的 ✓
    ///（做法：把 crate 原样拷到仓库外 ✓、在副本里加同一个渲染探针跑出来 ✓，
    ///  仓库内在这一步之前**没有任何改动** ✓）。
    /// 为什么非要有这一条 ✗：`texture_zero_is_byte_identical_to_before` 只证明
    /// **新代码内部两个分支相等** ✓ —— 两个分支一起写错也能通过 ✗。
    /// 只有改动前的黄金值能钉住"**与旧版一致**" ✓。
    #[test]
    fn texture_zero_matches_prechange_golden() {
        for (seed, size, pressure_milli, expected) in [
            (1_u32, 46_u32, 1000_u32, 0x5d0c_7c00_e32f_31cd_u64),
            (7, 17, 400, 0x83bd_8908_e198_2c2e),
            (99, 3, 1000, 0x606a_2fb0_cfb8_14c7),
        ] {
            let bytes = render(seed, size, pressure_milli, None);
            assert_eq!(bytes.len(), (size * size * 4) as usize);
            assert_eq!(
                fnv1a64(&bytes),
                expected,
                "seed={seed} size={size} pressure={pressure_milli} 的字节与改动前不一致"
            );
        }
    }

    /// 要求 3 ✓：`texture` 拉高必须**measurably** 更平 ✓（不是"感觉上" ✓）——
    /// 两个度量都打印出来 ✓，断言给**明确余量** ✓，免得改天飘一下就变成噪声 ✓。
    #[test]
    fn texture_reduces_grain() {
        let (seed, size, pressure_milli) = (1_u32, 46_u32, 1000_u32);
        let woven_mat = render(seed, size, pressure_milli, None); // 今天的样子 ✓（麻布感 ✓）
        let half = render(seed, size, pressure_milli, Some(0.5));
        let flat = render(seed, size, pressure_milli, Some(1.0));

        let step_none = alpha_step_mean(&woven_mat, size);
        let step_half = alpha_step_mean(&half, size);
        let step_full = alpha_step_mean(&flat, size);
        let std_none = alpha_std_dev(&woven_mat, size);
        let std_full = alpha_std_dev(&flat, size);
        eprintln!(
            "  [smoothness] 横向相邻 alpha 平均差：texture=0.0 -> {step_none:.4}, \
             0.5 -> {step_half:.4}, 1.0 -> {step_full:.4}"
        );
        eprintln!(
            "  [smoothness] alpha 标准差：texture=0.0 -> {std_none:.4}, 1.0 -> {std_full:.4}"
        );

        assert_ne!(
            woven_mat, flat,
            "texture=1.0 必须真的改变画面（否则这控件是假的）"
        );
        assert!(
            step_full < step_none * 0.5,
            "texture=1.0 的相邻差应当不到 0.0 的一半：{step_full:.4} vs {step_none:.4}"
        );
        assert!(
            step_half < step_none && step_full <= step_half,
            "削纹理应当单调：{step_none:.4} >= {step_half:.4} >= {step_full:.4}"
        );
        assert!(
            std_full < std_none,
            "alpha 标准差也应当下降：{std_full:.4} vs {std_none:.4}"
        );
    }

    /// **削纹理不该改墨色** ✓：`texture = 1.0` 的平均 alpha 必须与 0.0 基本相等 ✓。
    ///
    /// 这条不是在挑刺 ✗：前两版实现都栽在这里 ✓ ——
    ///
    /// * 第一版拿"连续相位的解析均值 0.775"当目标 ✗，而真实采样里
    ///   `f32::fract()` 对负数返回负数 ✓ ⇒ `bristle` 能冲到 2.3 ✓ ⇒ 有效均值 ≈ 0.81 ✓
    ///   ⇒ 一拉 `texture = 1` 就把整枚 dab 压暗 **13%** ✗；
    /// * 第二版改成"这枚 dab 自己的鬃毛加权均值" ✓，但**没算被 `clamp` 夹掉的那一份** ✗
    ///   ⇒ 反而**提亮 +2.9%** ✗（高峰像素本来被夹住了 ✓，削平之后不夹了 ✓）。
    ///
    /// 现在向"这枚 dab 自己**夹过之后**的加权平均覆盖电平"收 ✓（`oil_flat_coverage` ✓）
    /// ⇒ 覆盖率之和按定义不变 ✓ ⇒ 实测大 dab 的出入只剩 **0.1~0.2%**（`u8` 截断 ✓）。
    ///
    /// **小 dab 的容差故意放宽** ✓：`size = 3` 只有 9 个像素 ✓，而颗粒是逐像素的随机量 ✓
    /// ⇒ "9 个样本的均值"本身就有 ≈1.6% 的标准差 ✓（削平把它换成解析均值 0.925 ✓，
    /// 于是必然有同量级的电平差 ✓）。这不是 bug ✓ —— 是**噪声项自己的采样误差** ✓，
    /// 而且 `size = 3` 的 dab 小到这种电平差看不出来 ✓。真实笔尖（`size ≥ 16`）卡 0.5% ✓。
    #[test]
    fn texture_keeps_the_average_ink() {
        for (seed, size, pressure_milli) in [
            (1_u32, 46_u32, 1000_u32),
            (2, 46, 1000),
            (7, 17, 400),
            (5, 19, 800),
            (1, 64, 250),
            (99, 3, 1000), // 9 像素的极小 dab ✓：容差放宽到 3%（见上 ✓）。
        ] {
            let before = mean_alpha(&render(seed, size, pressure_milli, None));
            let after = mean_alpha(&render(seed, size, pressure_milli, Some(1.0)));
            let shift = (after - before) / before * 100.0;
            // 颗粒的样本噪声随 1/size 走 ✓：`size ≥ 16` ⇒ 0.5% 足够 ✓；更小的就得放宽 ✓。
            let tolerance = if size >= 16 { 0.5 } else { 3.0 };
            eprintln!(
                "  [ink] seed={seed} size={size} pressure={pressure_milli}: \
                 texture=0.0 平均 alpha={before:.3}, 1.0={after:.3}（{shift:+.2}%，容差 ±{tolerance}%）"
            );
            assert!(
                shift.abs() < tolerance,
                "削平不该改平均墨量：seed={seed} size={size} {before:.3} -> {after:.3}（{shift:+.2}%）"
            );
        }
    }

    /// 要求 4 的另一半 ✓：**同一输入永远同一字节** ✓（确定性是这个插件的立身之本 ✓——
    /// 服务端与浏览器端要画出同一幅画 ✓）。含 1.0 这个端点 ✓。
    #[test]
    fn texture_is_deterministic() {
        for texture in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let first = render(3, 23, 700, Some(texture));
            let second = render(3, 23, 700, Some(texture));
            assert_eq!(first, second, "texture={texture} 两次渲染必须逐字节相同");
        }
    }

    /// 要求 4 ✓：低于 0 按 0 ✓、高于 1 按 1 ✓。
    /// 顺手把 NaN 也钉住 ✓：`clamp` 对 NaN 返回 NaN ✓ ⇒ 整枚 dab 会被打成透明 ✗
    /// ⇒ 代码里显式把 NaN 当 0.0 ✓（宿主写坏一个槽不该让整笔画不出来 ✓）。
    #[test]
    fn texture_is_clamped() {
        let zero = render(5, 19, 800, Some(0.0));
        let one = render(5, 19, 800, Some(1.0));
        let absent = render(5, 19, 800, None);
        for below in [-0.5_f32, -1.0e9, -f32::MIN_POSITIVE] {
            assert_eq!(
                render(5, 19, 800, Some(below)),
                zero,
                "texture={below} 应按 0.0 处理"
            );
        }
        for above in [1.5_f32, 1.0e9, f32::MAX] {
            assert_eq!(
                render(5, 19, 800, Some(above)),
                one,
                "texture={above} 应按 1.0 处理"
            );
        }
        assert_eq!(
            render(5, 19, 800, Some(f32::NAN)),
            absent,
            "NaN 应按 0.0（= 旧行为）处理"
        );
    }
}
