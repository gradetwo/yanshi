//! 水彩介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 水彩与油画（`yanshi-medium-oil` ✓）的差别不在参数 ✓ 而在**行为** ✓：
//!
//! * **渗开的不规则边界** ✓：水在纸纤维上扩散 ⇒ 边界不是圆 ✓，
//!   这里用"按角度取确定性噪声调制半径"来模拟 ✓（同一 `seed` 下形状可复现 ✓）；
//! * **边缘沉积** ✓：颜料随水被推到湿润区的边缘并沉积 ⇒ 外沿更深、内部更淡 ✓
//!   （这是水彩最标志性的观感 ✓）；
//! * **留白 / 纸感** ✓：颜料是**半透明**的 ✓（整体 alpha 远低于油画 ✓），
//!   并叠一层纸纹颗粒 ✓ ⇒ 纸的白会透出来 ✓；
//! * **湿度** ✓：宿主给出的湿度越高 ✓，笔尖颜色被**目标处已有色**拉得越强 ✓（水带颜料 ✓）。
//!
//! 边界与其它插件一致 ✓：**零依赖、不导入任何宿主函数** ✓、随机性只来自 `seed` ✓、
//! 输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（与油画同版 ✓ —— 输入缓冲提供笔尖色/目标色/载墨/湿度 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素），宿主据此施加配额 ✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9) ✓。
pub const INPUT_FLOATS: u32 = 10;

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    MAX_DAB
}

#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> u32 {
    core::ptr::addr_of!(INPUT) as u32
}

#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    INPUT_FLOATS * 4
}

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

/// 平滑插值 ✓。
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// 由 `seed` 与**角度**得到确定性的边界扰动 ✓（模拟水在纸上的扩散形状 ✓）。
fn edge_wobble(seed: u64, angle: f32) -> f32 {
    // 三个不同频率的正弦叠加 ⇒ 形状自然又不规则 ✓，且完全确定 ✓。
    let mut state = seed ^ ((angle * 1000.0) as u64).wrapping_mul(0x9E37_79B9);
    let base = splitmix64(&mut state) as f32 / u32::MAX as f32;
    // 用 `core::f32::consts` 的常量而不是手写近似值 ✓ —— clippy 的 `approx_constant` 抓到过
    // 我写的 6.283 / 3.141 / 1.570 ✓（字面量近似常量既易错也不表意 ✓）。
    let a = (angle * 3.0 + base * core::f32::consts::TAU).sin();
    let b = (angle * 7.0 + base * core::f32::consts::PI).sin();
    let c = (angle * 13.0 + base * core::f32::consts::FRAC_PI_2).sin();
    (a * 0.5 + b * 0.3 + c * 0.2) * 0.5 + 0.5
}

/// 落一个水彩点 ✓：不规则边界 + 边缘沉积 + 半透明纸感 ✓。
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
        let (dst_r, dst_g, dst_b, _) = (input[4], input[5], input[6], input[7]);
        let load = input[8];
        let wetness = input[9];
        if load <= 0.0 || tip_a <= 0.0 {
            return 0; // 没水/没颜料 ✓。
        }
        let radius = size as f32 / 2.0;
        let mut state = (seed as u64) ^ 0xA076_1D64_78BD_642F;
        // 水彩整体**半透明** ✓：基础颜料量远低于油画 ✓（留白 ✓）。
        let pigment_base = 0.42 * pressure * load;
        let pull = (wetness * 0.8).clamp(0.0, 0.9); // 水多则目标色混得更多 ✓。
        let mix = |tip: f32, dst: f32| tip * (1.0 - pull) + dst * pull;
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - radius;
                let dy = y as f32 + 0.5 - radius;
                let distance = (dx * dx + dy * dy).sqrt();
                let angle = dy.atan2(dx);
                // 边界半径按角度扰动 ✓（0.72–1.0 ✓）⇒ 不规则水痕 ✓。
                let wobble = 0.72 + 0.28 * edge_wobble(seed as u64, angle);
                let edge_radius = radius * wobble;
                let index = ((y * size + x) * 4) as usize;
                if distance > edge_radius {
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                let t = distance / edge_radius.max(0.001);
                // 内部淡、外沿深 ✓（边缘沉积 ✓）：用一条"到边缘距离"的钟形曲线 ✓。
                let ring = 1.0 - ((t - 0.82) / 0.18).abs();
                let deposit = 0.55 + 0.75 * smooth(ring.clamp(0.0, 1.0));
                // 纸纹颗粒 ✓（确定性 ✓）+ 轻微的水痕流动 ✓。
                let grain = 1.0 - 0.22 * ((splitmix64(&mut state) >> 40) as f32 / 16_777_215.0);
                let flow = 0.9 + 0.1 * (angle * 5.0 + seed as f32).sin();
                let coverage = (pigment_base * deposit * grain * flow).clamp(0.0, 1.0);
                dab[index] = (mix(tip_r, dst_r) * 255.0).clamp(0.0, 255.0) as u8;
                dab[index + 1] = (mix(tip_g, dst_g) * 255.0).clamp(0.0, 255.0) as u8;
                dab[index + 2] = (mix(tip_b, dst_b) * 255.0).clamp(0.0, 255.0) as u8;
                dab[index + 3] = (coverage * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
    size * size * 4
}
