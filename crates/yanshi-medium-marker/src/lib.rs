//! 马克笔介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 马克笔的三个特征 ✓，都在这一个 `yanshi_dab` 里表达 ✓：
//!
//! * **平头（chisel）笔尖** ✓：覆盖不是圆，而是一个**沿 seed 固定方向**的扁矩形 ✓
//!   ⇒ 同一笔方向一致 ✓、转弯时自然出现宽窄变化 ✓（这是马克笔最直观的特征 ✓）；
//! * **叠色变深** ✓：马克笔是半透明的 ✓，两次叠在同一处会**更深** ✓ ⇒
//!   这里读宿主给的**目标处已有色** ✓，把新墨按"已有色越深、叠得越多"加深 ✓
//!   （与油画的"混色"相反 ✓：油画把颜色拉过去 ✓，马克笔是**乘法式叠深** ✓）；
//! * **轻微洇边** ✓：平头边缘有一点极淡的渗色 ✓（纸张吸墨的观感 ✓），但不做水彩那种大片渗开 ✓。
//!
//! 边界与其它插件相同 ✓：**零依赖、不导入任何宿主函数** ✓（宿主加载时校验 `imports` 为空 ✓）、
//! 随机性只来自 `seed` ✓、输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（宿主注入笔尖色、目标色、载墨与湿度 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素）✓ —— 马克笔头可以宽一些 ✓，但配额仍由宿主施加 ✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9) ✓。
pub const INPUT_FLOATS: u32 = 10;

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

fn marker_abi_version_body() -> u32 {
    ABI_VERSION
}

fn marker_max_dab_body() -> u32 {
    MAX_DAB
}

fn marker_input_ptr_body() -> usize {
    core::ptr::addr_of!(INPUT) as usize
}

fn marker_input_len_body() -> u32 {
    INPUT_FLOATS * 4
}

fn marker_dab_ptr_body() -> usize {
    core::ptr::addr_of!(DAB) as usize
}

/// splitmix64：确定、无外部状态 ✓。
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 落一个点 ✓。返回写入字节数（`size * size * 4` ✓）。
fn marker_dab_body(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    // `clamp(1, MAX_DAB)` 与原写法**语义等价**：`size: u32` 除 0 外都 ≥ 1，
    // 且 `MAX_DAB >= 1` ⇒ 不会触发 `clamp` 的 `max < min` panic。
    let size = size.clamp(1, MAX_DAB);
    let pressure = (pressure_milli.min(1000)) as f32 / 1000.0;
    // SAFETY：单线程 wasm，只读写自己声明的固定缓冲 ✓。
    unsafe {
        let input = &*core::ptr::addr_of!(INPUT);
        let (tip_r, tip_g, tip_b, tip_a) = (input[0], input[1], input[2], input[3]);
        let (dst_r, dst_g, dst_b, dst_a) = (input[4], input[5], input[6], input[7]);
        let load = input[8];
        if load <= 0.0 || tip_a <= 0.0 {
            return 0; // 墨水用尽 ✓（宿主按 `paint_load` 递减 ✓）。
        }
        let half = size as f32 / 2.0;
        let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
        // **平头方向**：由 seed 决定 ⇒ 同一笔方向一致、可复现 ✓。
        // **先掩到 32 位再归一化** ✓ —— `splitmix64` 是 u64 ✓，直接 `as f32 / u32::MAX as f32`
        // 会得到约 4.3e9 ✗（铅笔插件因此整块 dab 全 0 ✓，见那边的说明 ✓）。
        // 这里虽然只是喂给 `sin/cos` ✓（超大角度的结果仍在 [-1,1] ✓），但语义上就该是 0..PI 的均匀角 ✓。
        let nib_angle =
            ((splitmix64(&mut state) as u32) as f32 / u32::MAX as f32) * core::f32::consts::PI;
        let (sin_a, cos_a) = (nib_angle.sin(), nib_angle.cos());
        // 笔头是**扁的** ✓：沿长轴 1.0、沿短轴约 0.42 ⇒ 明显的宽窄变化 ✓。
        let nib_ratio = 0.42;
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - half;
                let dy = y as f32 + 0.5 - half;
                // 转到笔头坐标 ✓。
                let along = dx * cos_a + dy * sin_a;
                let across = -dx * sin_a + dy * cos_a;
                // 扁矩形覆盖 ✓（用平滑的"超椭圆"避免硬像素 ✓）。
                let u = (along / half).abs();
                let v = (across / (half * nib_ratio)).abs();
                let cover = (1.0 - (u.max(v)).powi(4)).clamp(0.0, 1.0);
                // **轻微洇边** ✓：外圈一圈很淡的渗色 ✓（幂次让它快速衰减 ✓）。
                let bleed = (1.0 - u.max(v)).clamp(0.0, 1.0).powi(6) * 0.18;
                let alpha_shape = (cover * 0.94 + bleed).clamp(0.0, 1.0);
                if alpha_shape <= 0.0 {
                    let index = ((y * size + x) * 4) as usize;
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                // **叠色变深** ✓：目标处越"已有墨"（alpha 高、颜色深）⇒ 新墨越深 ✓。
                // 用目标的 alpha 作为"已经画过几层"的代理 ✓（宿主给的是**画布上的实际颜色** ✓）。
                let layers = dst_a.clamp(0.0, 1.0);
                let darken = 1.0 - 0.38 * layers;
                let mix = |tip: f32, dst: f32| (tip * 0.72 + dst * 0.28) * darken;
                let coverage = (alpha_shape * pressure * load.min(1.0)).clamp(0.0, 1.0);
                // 纸张颗粒：极轻 ✓（马克笔在纸上也不是纯平色 ✓）。
                let grain = 1.0 - 0.05 * ((splitmix64(&mut state) >> 40) as f32 / 16_777_215.0);
                let index = ((y * size + x) * 4) as usize;
                dab[index] = ((mix(tip_r, dst_r) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 1] = ((mix(tip_g, dst_g) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 2] = ((mix(tip_b, dst_b) * 255.0).clamp(0.0, 255.0)) as u8;
                // **叠色变深**也要体现在 alpha 上 ✓：已有墨的地方再叠一层几乎不透明 ✓。
                let alpha = (coverage * (0.82 + 0.18 * layers) * grain).clamp(0.0, 1.0);
                dab[index + 3] = (alpha * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
    size * size * 4
}

// **导出名的平台分叉** ✓（这是让"六个插件能链进同一个二进制"的关键 ✓）：
//
// * **wasm 构建照旧导出 ABI 名** ✓（`yanshi_dab` 等 ✓）—— **已发布的契约一个字节不改** ✓；
// * **原生构建导出唯一名** ✓（`yanshi_marker_dab` 等 ✓）—— 六个插件的符号因此不再冲突 ✓，
//   服务端于是能把它们**全部链进来** ✓（真实用户 P1-3 要的正是这件事 ✓）。
//
// 之所以不直接把旧名改掉 ✗：那会**破坏浏览器端的官方 ABI** ✓ —— 没必要 ✓。

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    marker_abi_version_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_abi_version() -> u32 {
    marker_abi_version_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    marker_max_dab_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_max_dab() -> u32 {
    marker_max_dab_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> usize {
    marker_input_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_input_ptr() -> usize {
    marker_input_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    marker_input_len_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_input_len() -> u32 {
    marker_input_len_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> usize {
    marker_dab_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_dab_ptr() -> usize {
    marker_dab_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    marker_dab_body(seed, size, pressure_milli)
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_marker_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    marker_dab_body(seed, size, pressure_milli)
}
