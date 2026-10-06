//! 像素介质插件 —— **ABI v2** ✓（设计 11.1：其他介质通过 WASM 插件扩展 ✓）。
//!
//! 像素笔刷的定义性特征是**没有抗锯齿** ✓ —— 它不是"看起来像方块" ✓，
//! 而是**二进制覆盖** ✓：方形之内 alpha 全满、方形之外完全透明 ✓，中间没有过渡 ✓。
//! 因此这里刻意不做三件"其它插件都在做"的事 ✓：
//!
//! * **不做软边** ✗：边缘是**像素级硬切** ✓（这正是像素画要的 ✓）；
//! * **不做颗粒** ✗：像素画的颜色必须**精确等于笔尖色** ✓（实测：不做混色、不做噪声 ✓）；
//! * **不做混色** ✗：与目标处已有色无关 ✓（叠色由**宿主的合成**决定 ✓，插件保持无状态 ✓）。
//!
//! 仍遵守全部边界 ✓：**零依赖、不导入任何宿主函数** ✓、随机性只来自 `seed` ✓
//!（这里其实**用不到**随机 ✓ —— 但 ABI 要求"不同 seed 应产生不同结果"是**不该**施加给像素笔刷的通则 ✗，
//!  见 `scripts/medium-abi-check.mjs` 里对这一条的按介质处理 ✓）、输出上限自报 `yanshi_max_dab` ✓、浮点属 **D2** ✓。

/// ABI 版本：**2** ✓（宿主注入笔尖色、目标色、载墨与湿度 ✓ —— 像素笔刷只用到笔尖色 ✓）。
pub const ABI_VERSION: u32 = 2;
/// 单点最大边长（像素）✓ —— 像素画常用 1..16，上限给足但仍受宿主配额约束 ✓。
pub const MAX_DAB: u32 = 64;
/// 输入缓冲长度：笔尖 RGBA(0-3)、目标 RGBA(4-7)、载墨(8)、湿度(9) ✓。
pub const INPUT_FLOATS: u32 = 10;

static mut DAB: [u8; (MAX_DAB * MAX_DAB * 4) as usize] = [0; (MAX_DAB * MAX_DAB * 4) as usize];
static mut INPUT: [f32; INPUT_FLOATS as usize] = [0.0; INPUT_FLOATS as usize];

fn pixel_abi_version_body() -> u32 {
    ABI_VERSION
}

fn pixel_max_dab_body() -> u32 {
    MAX_DAB
}

fn pixel_input_ptr_body() -> usize {
    core::ptr::addr_of!(INPUT) as usize
}

fn pixel_input_len_body() -> u32 {
    INPUT_FLOATS * 4
}

fn pixel_dab_ptr_body() -> usize {
    core::ptr::addr_of!(DAB) as usize
}

/// 落一个点 ✓。返回写入字节数（`size * size * 4` ✓）。
///
/// **完全确定、且与 `seed` 无关** ✓ —— 像素笔刷没有"鬃毛/颗粒"这类需要随机的东西 ✓。
fn pixel_dab_body(_seed: u32, size: u32, pressure_milli: u32) -> u32 {
    // `clamp(1, MAX_DAB)` 与原写法**语义等价**：`size: u32` 除 0 外都 ≥ 1，
    // 且 `MAX_DAB >= 1` ⇒ 不会触发 `clamp` 的 `max < min` panic。
    let size = size.clamp(1, MAX_DAB);
    // 压力只决定"**画不画**" ✓（像素画没有半透明深浅 ✓ hmm: 轻微压力仍应落笔 ✓）——
    // 阈值取很低即可 ✓：像素笔刷的手感来自"要么有一个像素、要么没有" ✓。
    let paints = pressure_milli > 0;
    // SAFETY：单线程 wasm，只读写自己声明的固定缓冲 ✓。
    unsafe {
        let input = &*core::ptr::addr_of!(INPUT);
        let (tip_r, tip_g, tip_b, tip_a) = (input[0], input[1], input[2], input[3]);
        let load = input[8];
        let dab = &mut *core::ptr::addr_of_mut!(DAB);
        if !paints || load <= 0.0 || tip_a <= 0.0 {
            // 不落笔时也要**清零** ✓：缓冲区是所有调用共用的 ✓，
            // 留着上一次的像素会被宿主当成这一笔的结果 ✗（内核按返回长度读取 ✓）。
            // 用切片填充而不是下标循环 ✓（clippy 的 `needless_range_loop` 会指出
            // "循环变量只用来索引" ✓，而这里本来就该写成整段清零 ✓）。
            dab[..((size * size * 4) as usize)].fill(0);
            return 0;
        }
        // **精确的笔尖色** ✓（不做任何混色/噪声/边缘衰减 ✓）。
        let r = (tip_r.clamp(0.0, 1.0) * 255.0).round() as u8;
        let g = (tip_g.clamp(0.0, 1.0) * 255.0).round() as u8;
        let b = (tip_b.clamp(0.0, 1.0) * 255.0).round() as u8;
        let a = (tip_a.clamp(0.0, 1.0) * 255.0).round() as u8;
        for index in (0..((size * size) as usize)).map(|pixel| pixel * 4) {
            dab[index] = r;
            dab[index + 1] = g;
            dab[index + 2] = b;
            dab[index + 3] = a;
        }
    }
    size * size * 4
}

// **导出名的平台分叉** ✓（这是让"六个插件能链进同一个二进制"的关键 ✓）：
//
// * **wasm 构建照旧导出 ABI 名** ✓（`yanshi_dab` 等 ✓）—— **已发布的契约一个字节不改** ✓；
// * **原生构建导出唯一名** ✓（`yanshi_pixel_dab` 等 ✓）—— 六个插件的符号因此不再冲突 ✓，
//   服务端于是能把它们**全部链进来** ✓（真实用户 P1-3 要的正是这件事 ✓）。
//
// 之所以不直接把旧名改掉 ✗：那会**破坏浏览器端的官方 ABI** ✓ —— 没必要 ✓。

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_abi_version() -> u32 {
    pixel_abi_version_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_abi_version() -> u32 {
    pixel_abi_version_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_max_dab() -> u32 {
    pixel_max_dab_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_max_dab() -> u32 {
    pixel_max_dab_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_ptr() -> usize {
    pixel_input_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_input_ptr() -> usize {
    pixel_input_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_input_len() -> u32 {
    pixel_input_len_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_input_len() -> u32 {
    pixel_input_len_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab_ptr() -> usize {
    pixel_dab_ptr_body()
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_dab_ptr() -> usize {
    pixel_dab_ptr_body()
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn yanshi_dab(_seed: u32, size: u32, pressure_milli: u32) -> u32 {
    pixel_dab_body(_seed, size, pressure_milli)
}

#[cfg(not(target_arch = "wasm32"))]
#[no_mangle]
pub extern "C" fn yanshi_pixel_dab(_seed: u32, size: u32, pressure_milli: u32) -> u32 {
    pixel_dab_body(_seed, size, pressure_milli)
}
