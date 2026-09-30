//! 油画 / 水彩介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 与 v1 的差别是**宿主提供上下文** ✓：插件在 wasm 里读不到画布 ✗，所以宿主把
//! 「笔尖色 / 目标处已有色 / 载墨 / 湿度」写进插件的**输入缓冲** ✓，插件据此模拟颜料行为 ✓：
//!
//! * **载墨（load）** ✓：每一笔消耗载体里的颜料 ⇒ 越画越淡 ✓，`load` 用尽即停笔 ✓（模拟"没颜料了" ✓）；
//! * **混色（mixing）** ✓：笔尖颜色被**目标处已有色**按湿度拉过去 ⇒ 湿画法里颜色互相吃掉 ✓；
//! * **鬃毛纹理** ✓：每个点由注入的 `seed` 派生一组"鬃毛"通道 ✓ ⇒ 条纹方向固定、可复现 ✓；
//! * **干湿边缘** ✓：边缘略深、内部略浅 ✓（油画堆料的观感 ✓）。
//!
//! 边界与 v1 相同 ✓：**零依赖、不导入任何宿主函数** ✓（宿主加载时校验 `imports` 为空 ✓）、
//! 随机性只来自 `seed` ✓、输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（v1 仍是 1 ✓ —— 两个版本并存正是"id + version 随对象记录"要支持的 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素），宿主据此施加配额 ✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9) —— 共 **10** 个 f32 ✓。
///（第一版写成 8 ✗，于是 `input[8 % 8]` 读回了笔尖色 ✓ —— 字段数必须与下标一致 ✓。）
pub const INPUT_FLOATS: u32 = 10;

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

/// 插件 ABI 版本 ✓。
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    MAX_DAB
}

/// 输入缓冲地址（宿主写入上下文 ✓）。
#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> u32 {
    core::ptr::addr_of!(INPUT) as u32
}

/// 输入缓冲长度（字节）✓。
#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    INPUT_FLOATS * 4
}

/// 输出缓冲地址 ✓。
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> u32 {
    core::ptr::addr_of!(DAB) as u32
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
#[no_mangle]
pub extern "C" fn yanshi_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    let size = if size == 0 {
        1
    } else if size > MAX_DAB {
        MAX_DAB
    } else {
        size
    };
    let pressure = (pressure_milli.min(1000)) as f32 / 1000.0;
    // SAFETY：单线程 wasm，只读写自己声明的固定缓冲 ✓。
    unsafe {
        let input = &*core::ptr::addr_of!(INPUT);
        let (tip_r, tip_g, tip_b, tip_a) = (input[0], input[1], input[2], input[3]);
        let (dst_r, dst_g, dst_b, _dst_a) = (input[4], input[5], input[6], input[7]);
        let load = input[8];
        let wetness = input[9];
        if load <= 0.0 || tip_a <= 0.0 {
            return 0; // 没颜料了 ✓（模拟载墨耗尽 ✓）。
        }
        let radius = size as f32 / 2.0;
        let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
        // 每笔一次"鬃毛方向"，由 seed 决定 ⇒ 同一笔条纹一致、可复现 ✓。
        let bristle_angle =
            (splitmix64(&mut state) as f32 / u32::MAX as f32) * core::f32::consts::PI;
        let (sin_a, cos_a) = (bristle_angle.sin(), bristle_angle.cos());
        let bristles = 7.0;
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - radius;
                let dy = y as f32 + 0.5 - radius;
                let distance = (dx * dx + dy * dy).sqrt();
                // 圆形笔尖的软边 ✓。
                let edge = (1.0 - distance / radius).clamp(0.0, 1.0);
                if edge <= 0.0 {
                    let index = ((y * size + x) * 4) as usize;
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                // 鬃毛：沿垂直方向的条纹 ⇒ 油画刷痕 ✓（方向由 seed 固定 ✓）。
                let across = dx * cos_a + dy * sin_a;
                let ridge = ((across / radius) * bristles * 0.5 + 0.5).fract();
                let bristle = 0.55 + 0.45 * (3.0 * ridge * ridge - 2.0 * ridge * ridge * ridge);
                // 干湿边缘：靠近外沿略深（堆料边缘 ✓），内部略浅 ✓。
                let rim = 1.0 - edge;
                let body = bristle * (0.75 + 0.45 * rim);
                // 混色：湿画法里笔尖被目标色按湿度拉过去 ✓。
                let pull = (wetness * 0.65).clamp(0.0, 0.95);
                let mix = |tip: f32, dst: f32| tip * (1.0 - pull) + dst * pull;
                let coverage = (edge.sqrt() * body * pressure * load).clamp(0.0, 1.0);
                // 颗粒：极轻的随机扰动 ⇒ 颜料不是塑料感 ✓（同 seed 可复现 ✓）。
                let grain = 1.0 - 0.15 * ((splitmix64(&mut state) >> 40) as f32 / 16_777_215.0);
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
