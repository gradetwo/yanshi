//! 铅笔介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 铅笔与油画/水彩/马克笔的差别在于"**干**" ✓，这里用三件事表达 ✓：
//!
//! * **压力驱动的深浅** ✓：铅笔的浓淡几乎全来自手劲 ✓ ⇒ 覆盖度按 `pressure^1.5` 走 ✓，
//!   轻压几乎只有一点点石墨 ✓、重压才实 ✓（幂次比线性更接近真实手感 ✓）；
//! * **石墨颗粒** ✓：每个点有细颗粒 ✓（由 `seed` 派生 ⇒ 可复现 ✓），
//!   颗粒让"轻压"呈现为**断续的砂砾感** ✓ 而不是均匀的半透明 ✓ ——
//!   这是铅笔最容易被做丢的特征 ✓；
//! * **几乎不混色、不洇** ✓：干介质不会把下面的颜色拉上来 ✓ ⇒ 完全忽略 `wetness` ✓，
//!   只按很低的固定比例参考目标色 ✓（避免纯色覆盖 ✓ 也避免油画那种湿混 ✓）。
//!
//! 边界与其它插件相同 ✓：**零依赖、不导入任何宿主函数** ✓、随机性只来自 `seed` ✓、
//! 输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（宿主注入笔尖色、目标色、载墨与湿度 ✓ —— 铅笔只用到前两项 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素）✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9) ✓。
pub const INPUT_FLOATS: u32 = 10;

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

fn pencil_abi_version_body() -> u32 {
    ABI_VERSION
}

fn pencil_max_dab_body() -> u32 {
    MAX_DAB
}

fn pencil_input_ptr_body() -> usize {
    core::ptr::addr_of!(INPUT) as usize
}

fn pencil_input_len_body() -> u32 {
    INPUT_FLOATS * 4
}

fn pencil_dab_ptr_body() -> usize {
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
fn pencil_dab_body(seed: u32, size: u32, pressure_milli: u32) -> u32 {
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
        if load <= 0.0 || tip_a <= 0.0 {
            return 0; // 笔芯用完 ✓（宿主按 `paint_load` 递减 ✓）。
        }
        let half = size as f32 / 2.0;
        let mut state = (seed as u64) ^ 0x9E37_79B9_7F4A_7C15;
        // **石墨的粗细**由 seed 决定 ⇒ 同一笔的颗粒走向一致、可复现 ✓。
        // **注意类型转换** ✓：`splitmix64` 返回 **u64** ✓，直接 `as f32` 再除以 `u32::MAX as f32`
        // 会得到约 **4.3e9** 的值 ✗ ⇒ `coarseness` 巨大 ⇒ `grain` 变成大负数 ⇒ 每个像素都被
        // 阈值挡掉 ⇒ 整块 dab **全 0** ✓（实测：写入 1024 字节，而有墨像素 0 ✓）。
        // 先掩到 32 位再归一化 ✓。
        let coarseness = 0.55 + 0.45 * ((splitmix64(&mut state) as u32) as f32 / u32::MAX as f32);
        // 压力幂次 ✓：比线性更"铅笔" ✓（轻压几乎无墨 ✓）。
        let pressure_curve = pressure.powf(1.5);
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 + 0.5 - half;
                let dy = y as f32 + 0.5 - half;
                let distance = (dx * dx + dy * dy).sqrt();
                // 软圆尖 ✓（铅笔尖很小、边缘很软 ✓）。
                let edge = (1.0 - distance / half).clamp(0.0, 1.0);
                if edge <= 0.0 {
                    let index = ((y * size + x) * 4) as usize;
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                let soft = edge.powf(1.35);
                // **石墨颗粒** ✓：高频噪声 ⇒ 轻压时表现为断续砂砾 ✓（而不是均匀半透明 ✓）。
                let noise = (splitmix64(&mut state) >> 40) as f32 / 16_777_215.0;
                let grain = 1.0 - coarseness * (1.0 - noise) * (1.0 - pressure * 0.55);
                // 只在颗粒高于阈值处落石墨 ✓ ⇒ 压力越低、落点越稀疏 ✓。
                let sparse = if grain < 0.28 { 0.0 } else { grain };
                let body = soft * sparse * pressure_curve * load.min(1.0);
                if body <= 0.0 {
                    let index = ((y * size + x) * 4) as usize;
                    dab[index] = 0;
                    dab[index + 1] = 0;
                    dab[index + 2] = 0;
                    dab[index + 3] = 0;
                    continue;
                }
                // **几乎不混色** ✓：干介质不把下面的颜色拉上来 ✓（固定很低的比例 ✓）。
                let mix = |tip: f32, dst: f32| tip * 0.93 + dst * 0.07;
                let index = ((y * size + x) * 4) as usize;
                dab[index] = ((mix(tip_r, dst_r) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 1] = ((mix(tip_g, dst_g) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 2] = ((mix(tip_b, dst_b) * 255.0).clamp(0.0, 255.0)) as u8;
                dab[index + 3] = ((body.clamp(0.0, 1.0)) * 255.0).clamp(0.0, 255.0) as u8;
            }
        }
    }
    size * size * 4
}

// **导出名的平台分叉** ✓（这是让"六个插件能链进同一个二进制"的关键 ✓）：
//
// * **wasm 构建照旧导出 ABI 名** ✓（`yanshi_dab` 等 ✓）—— **已发布的契约一个字节不改** ✓；
// * **原生构建导出唯一名** ✓（`yanshi_pencil_dab` 等 ✓）—— 六个插件的符号因此不再冲突 ✓，
//   服务端于是能把它们**全部链进来** ✓（真实用户 P1-3 要的正是这件事 ✓）。
//
// 之所以不直接把旧名改掉 ✗：那会**破坏浏览器端的官方 ABI** ✓ —— 没必要 ✓。

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    pencil_abi_version_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_abi_version() -> u32 {
    pencil_abi_version_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    pencil_max_dab_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_max_dab() -> u32 {
    pencil_max_dab_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> usize {
    pencil_input_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_input_ptr() -> usize {
    pencil_input_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    pencil_input_len_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_input_len() -> u32 {
    pencil_input_len_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> usize {
    pencil_dab_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_dab_ptr() -> usize {
    pencil_dab_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    pencil_dab_body(seed, size, pressure_milli)
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pencil_dab(seed: u32, size: u32, pressure_milli: u32) -> u32 {
    pencil_dab_body(seed, size, pressure_milli)
}
