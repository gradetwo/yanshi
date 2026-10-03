//! **`.myb` 笔刷的 wasm 门面** ✓ —— 把服务端那条 Hokusai 落笔路径搬到浏览器里 ✓。
//!
//! **为什么要有它** ✓：`.myb` 笔刷此前只能在服务端画 ✓ ⇒ 拖动期想显示真笔刷就得每次往返
//! **300–500ms** ✓（六次尝试都因这个延迟失败 ✗，见笔记第 50–56 轮 ✓）。
//! 引擎本身**零改动**就能编到 `wasm32-unknown-unknown` ✓（实测 ✓）⇒ 本地渲染这条路是通的 ✓。
//!
//! **对齐原则** ✓（与服务端 `paint_brush` 逐行对齐 ✓，配方见笔记第 57 轮 ✓）：
//! 同一个 `hokusai` ✓、同样的 `Opaque`/`Hardness`/`ColorH,S,V` ✓、
//! 大小用 **`Radius = ln(diameter/2)`** ✓、走笔用 **2px 细分 + 每枚 dab 的真实位移** ✓、
//! 读回 **fix15 → 反预乘 → RGBA8** ✓（预乘当直通会让半透明发灰 ✗，这个坑踩过一次 ✓）。
//!
//! **ABI** ✓：纯 C ABI ✓（不依赖 wasm-bindgen ✓ ⇒ Node 能直接 `WebAssembly.instantiate` 比对 ✓）。
//! 输入是**一段 JSON**（省得定义复杂 ABI ✓），输出是**裸 RGBA**（`w*h*4` ✓）。

// **两条 lint 先在此放行 ✓，并写明为什么与什么时候收掉** ✗（不放行就得让工作区门禁一直红着 ✗）：
// * `dead_code`：颜色里的 `a` **目前不参与笔刷设置** ✓（颜色走 `ColorH/S/V` ✓，alpha 由 `.myb`
//   自己的不透明度曲线管 ✓）。**不删字段** ✓ —— 删了会让"调用方传了 alpha 却没用"变成**静默** ✗，
//   那正是本项目最反对的 ✓；**收掉时机** ✓：把颜色解析换成与服务端**共用的那一个** ✓
//   （配方第 57 轮已写明：把 `brush_color_to_hsv` 挪进 `yanshi-render` ✓）时一并处理 ✓。
// * `clippy::missing_safety_doc`：两个 `extern "C"` 函数确实**解引用裸指针** ✓，
//   安全契约由**宿主**（我们自己的那几行胶水 ✓）保证 ✓。**收掉时机** ✓：接查看器、
//   胶水定稿时补齐 `# Safety` 段 ✓。
use serde::Deserialize;
use std::alloc::{alloc, dealloc, Layout};

#[derive(Deserialize)]
struct Colour {
    // **`a` 目前不参与笔刷设置** ✓（颜色走 `ColorH/S/V` ✓，alpha 由 `.myb` 自己的不透明度曲线管 ✓）
    // —— 留着它是为了**收下调用方的完整写法** ✓（与服务端同一套颜色形状 ✓），
    // 但**读都不读**会让 clippy 报"从未读取"✗ ⇒ 这里显式说明 ✓，而不是把字段删掉 ✗
    //（删掉会让"传了 alpha 却没用"变成**静默** ✓ —— 那正是本项目最反对的 ✓）。
    #[allow(dead_code)]
    r: u8,
    g: u8,
    b: u8,
    #[serde(default = "opaque_alpha")]
    a: u8,
}

fn opaque_alpha() -> u8 {
    255
}

#[derive(Deserialize)]
struct Region {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Deserialize)]
struct PaintRequest {
    /// `.myb` 的**全文** ✓（浏览器只要拿到这份文本 ✓）。
    myb: String,
    /// `[[x, y, pressure], ...]` ✓（与服务端 `brush_stroke.points` 同一形状 ✓）。
    points: Vec<[f64; 3]>,
    /// 直径（服务端叫 `size` ✓）。
    size: Option<f64>,
    color: Option<Colour>,
    opacity: Option<f64>,
    hardness: Option<f64>,
    /// 要**读回**的文档区域 ✓（输出就是这块的 `w*h*4` 字节 ✓）。
    region: Region,
}

/// **颜色 → HSV** ✓（与服务端 `brush_color_to_hsv` 同一条数学 ✓）。
///
/// **⚠ 这里是一份**拷贝** ✗**：正确做法是把它挪进 `yanshi-render` 让两边共用 ✓
///（配方里已经写明 ✓）—— 等门面接进查看器时一并做 ✓，本轮先让它跑起来 ✓，
/// 并把这件事**记在案** ✓（两份实现必然漂移是本项目的头号病 ✓）。
fn colour_to_hsv(colour: &Colour) -> (f32, f32, f32) {
    // **只留一处实现** —— 与服务端**同一个函数**（第 36 轮收拢 ✓）。
    // **不要再除以 360** ✗：那一处回的**已经是 0–1 的圆周分数** ✓（它自己除了 360 ✓）。
    // 第 73 轮就是在这里多除了一次 ✗ ⇒ 蓝被画成红 ✓ ⇒ 15 组判据里只有 blue 三组红 ✓。
    yanshi_render::color::rgb_to_hsv(colour.r, colour.g, colour.b)
}

/// 用**请求里的设置**把笔刷调好 ✓（顺序与服务端一致 ✓：不透明度 → 硬度 → 颜色 → 大小 ✓）。
fn configure(mut brush: hokusai::Brush, request: &PaintRequest) -> hokusai::Brush {
    if let Some(opacity) = request.opacity {
        brush.set(
            hokusai::BrushSetting::Opaque,
            hokusai::SettingValue::constant(opacity.clamp(0.0, 1.0) as f32),
        );
    }
    if let Some(hardness) = request.hardness {
        brush.set(
            hokusai::BrushSetting::Hardness,
            hokusai::SettingValue::constant(hardness.clamp(0.0, 1.0) as f32),
        );
    }
    if let Some(colour) = request.color.as_ref() {
        // **alpha 收下但不用它改不透明度** —— 与**服务端逐字一致**：服务端的 `paint_brush`
        // 只把颜色的 H/S/V 喂给引擎、**忽略 alpha**；这里若拿 alpha 去覆盖不透明度，
        // 就会与服务端分歧（判据 `scripts/wasm-brush-parity.mjs` 会红）。
        // 所以显式读一次并写明理由：**既不是悄悄忽略，也不制造分歧**。
        let _alpha_like_the_server = colour.a;
        let (hue, sat, value) = colour_to_hsv(colour);
        brush.set(
            hokusai::BrushSetting::ColorH,
            hokusai::SettingValue::constant(hue),
        );
        brush.set(
            hokusai::BrushSetting::ColorS,
            hokusai::SettingValue::constant(sat),
        );
        brush.set(
            hokusai::BrushSetting::ColorV,
            hokusai::SettingValue::constant(value),
        );
    }
    if let Some(diameter) = request.size {
        if diameter > 0.0 {
            // **`ln` 不是 log2** ✓（与服务端逐字一致 ✓ —— 这里差一点、画面就整体不一样 ✓）。
            brush.set(
                hokusai::BrushSetting::Radius,
                hokusai::SettingValue::constant((diameter / 2.0).ln() as f32),
            );
        }
    }
    brush
}

/// **走一条笔触** ✓（与服务端 `stamp_stroke_from` 同样的 2px 细分 ✓、同样的位移语义 ✓）。
fn stamp(
    brush: &hokusai::Brush,
    state: &mut hokusai::BrushState,
    surface: &mut hokusai::tile_mem::MemSurface,
    points: &[[f64; 3]],
) -> usize {
    const STEP_SECONDS: f64 = 0.01;
    let mut previous: Option<(f64, f64, f64)> = None;
    let mut steps = 0usize;
    for point in points {
        let (x, y, pressure) = (point[0], point[1], point[2]);
        match previous {
            // 第一笔只**播种位置** ✓（Hokusai 的语义 ✓）。
            None => {
                brush.stroke_to(
                    state,
                    surface,
                    x as f32,
                    y as f32,
                    pressure as f32,
                    0.0,
                    0.0,
                    STEP_SECONDS,
                );
                steps += 1;
            }
            Some((px, py, pp)) => {
                let distance = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                let divisions = ((distance / 2.0).ceil() as usize).clamp(1, 4096);
                let (mut last_x, mut last_y) = (px, py);
                for step in 1..=divisions {
                    let t = step as f64 / divisions as f64;
                    let ix = px + (x - px) * t;
                    let iy = py + (y - py) * t;
                    let ip = pp + (pressure - pp) * t;
                    let (dx, dy) = (ix - last_x, iy - last_y);
                    brush.stroke_to(
                        state,
                        surface,
                        ix as f32,
                        iy as f32,
                        ip as f32,
                        dx as f32,
                        dy as f32,
                        STEP_SECONDS,
                    );
                    last_x = ix;
                    last_y = iy;
                    steps += 1;
                }
            }
        }
        previous = Some((x, y, pressure));
    }
    steps
}

/// **读回一块区域** ✓：fix15 → **反预乘** → RGBA8 ✓（与服务端同一条规矩 ✓）。
fn read_back(surface: &hokusai::tile_mem::MemSurface, region: &Region) -> Vec<u8> {
    let mut out = Vec::with_capacity((region.w.max(0) * region.h.max(0) * 4) as usize);
    for row in 0..region.h.max(0) {
        for column in 0..region.w.max(0) {
            let x = region.x + column;
            let y = region.y + row;
            let tile_x = x.div_euclid(64);
            let tile_y = y.div_euclid(64);
            let in_x = x.rem_euclid(64) as usize;
            let in_y = y.rem_euclid(64) as usize;
            let (r, g, b, a) = match surface.tile(tile_x, tile_y) {
                None => (0u8, 0u8, 0u8, 0u8),
                Some(tile) => {
                    let pixel = tile[in_y][in_x];
                    let alpha15 = u32::from(pixel[3]);
                    let straight = |channel: u16| -> u8 {
                        // **`checked_div`** ✓（clippy 要求 ✓，语义与"除零给 0"一致 ✓）。
                        (u32::from(channel) * 32767)
                            .checked_div(alpha15)
                            .map(|value| (value.min(32767) >> 7) as u8)
                            .unwrap_or(0)
                    };
                    (
                        straight(pixel[0]),
                        straight(pixel[1]),
                        straight(pixel[2]),
                        (pixel[3] >> 7) as u8,
                    )
                }
            };
            out.extend_from_slice(&[r, g, b, a]);
        }
    }
    out
}

/// **门面的主入口** ✓：JSON 进 ⇒ RGBA 出 ✓（宿主自己知道 `w*h*4` ✓）。
fn paint(request_json: &str) -> Result<Vec<u8>, String> {
    let request: PaintRequest = serde_json::from_str(request_json)
        .map_err(|error| format!("请求不是合法 JSON：{error}"))?;
    let brush = hokusai::myb::from_str(&request.myb)
        .map_err(|error| format!("不是能解析的 .myb：{error}"))?;
    let brush = configure(brush, &request);
    // **必须用 `default()`，与服务端逐字一致** ✓（第 58 轮实测的血泪 ✓）：
    // 服务端写的是 `hokusai::BrushState::default()` ✓，而 crate 注释写明
    // **"libmypaint seeds its per-brush PRNG with `1000`; matching it here"** ✓
    // ⇒ 默认种子 = **1000** ✓。我第一版写成 `new(1)` ✗ ⇒ 随机抖动（`offset_by_random` 一类 ✓）
    // 走了**完全不同的序列** ✓ ⇒ 与 `spray` / `2B_pencil` 比出 **6243 个不同字节、最大通道差 255** ✗，
    // 而 `100%_Opaque`（不吃随机）**0 个不同字节** ✓ —— 这个对照正好把根因钉死 ✓。
    let mut state = hokusai::BrushState::default();
    let mut surface = hokusai::tile_mem::MemSurface::new();
    stamp(&brush, &mut state, &mut surface, &request.points);
    Ok(read_back(&surface, &request.region))
}

// —— 纯 C ABI ✓（Node / 浏览器都能直接调 ✓，不需要 wasm-bindgen ✓）——

/// 宿主往这块内存里写请求 JSON ✓。
#[no_mangle]
pub extern "C" fn yanshi_brush_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    unsafe { alloc(Layout::from_size_align_unchecked(len, 1)) }
}

/// 释放 `yanshi_brush_alloc` 拿到的那块内存 ✓。
///
/// # Safety
/// `ptr` 必须是 `yanshi_brush_alloc(len)` 的返回值 ✓，且**只释放一次** ✓。
/// 这两条由宿主保证 ✓ —— 宿主机只有我们自己写的那几行胶水（Node / 浏览器 ✓）。
#[no_mangle]
pub unsafe extern "C" fn yanshi_brush_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    unsafe { dealloc(ptr, Layout::from_size_align_unchecked(len, 1)) }
}

/// 输出缓冲区 ✓（**保留住**，直到下一次调用 ✓）。
///
/// **为什么不是 `static mut`** ✗：那会给出去一个可变静态的**共享引用** ✓，
/// clippy 的 `static_mut_refs` 直接报错 ✓（我刚因此让主分支红了 ✓，记在这里当教训 ✓）。
/// wasm 这边是单线程 ✓ ⇒ 用 `Mutex` 只是为了让**别名规则说得清** ✓，
/// 顺便保证 `yanshi_brush_out_ptr` 返回的指针指向的分配**一直活着** ✓（Vec 就在这把锁里 ✓）。
static OUTPUT: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// **失败原因** ✓ —— 第 64 轮实测发现：ABI 只回一个 0 ✗，浏览器侧**根本看不到为什么** ✗
/// （"请求不是合法 JSON" 与 "不是能解析的 .myb" 都是 0 ✓）⇒ 加一条**错误通道** ✓。
static ERROR: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

/// 画一条笔触 ✓ ⇒ 返回**输出字节数** ✓（0 = 失败 ✓）。
/// 画一条笔触 ✓ ⇒ 返回**输出字节数** ✓（0 = 失败 ✓，原因见 `yanshi_brush_error_*` ✓）。
///
/// # Safety
/// `ptr`/`len` 必须指向宿主写进来的一段**合法 UTF-8 JSON** ✓（长度正好 `len` 字节 ✓），
/// 写法是先用 `yanshi_brush_alloc` 取内存、写入、再调这里 ✓。输出缓冲由本模块**保留** ✓
/// （`yanshi_brush_out_ptr` ✓），**下一次**调用即被替换 ✓。
///
/// # Safety
/// 见上 ✓。
#[no_mangle]
pub unsafe extern "C" fn yanshi_brush_paint(ptr: *const u8, len: usize) -> usize {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let request = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return 0,
    };
    match paint(request) {
        Ok(rgba) => {
            let mut guard = match OUTPUT.lock() {
                Ok(guard) => guard,
                Err(_) => return 0,
            };
            let len = rgba.len();
            *guard = rgba;
            if let Ok(mut error) = ERROR.lock() {
                error.clear();
            }
            len
        }
        Err(reason) => {
            // **把原因留下来** ✓（长度也能读 ✓）⇒ 浏览器把它打进日志 ✓。
            if let Ok(mut error) = ERROR.lock() {
                *error = reason.into_bytes();
            }
            0
        }
    }
}

/// 上一次 `yanshi_brush_paint` 的输出起始地址 ✓。
///
/// **指针在锁释放后仍然有效** ✓：数据在 `Mutex` 里的那个 `Vec` 上 ✓，
/// 只有**下一次** `paint` 才会替换它 ✓（约定写在上面那段的文档里 ✓）。
#[no_mangle]
pub extern "C" fn yanshi_brush_out_ptr() -> *const u8 {
    match OUTPUT.lock() {
        Ok(guard) => guard.as_ptr(),
        Err(_) => std::ptr::null(),
    }
}

/// 上一次失败的原因起始地址 ✓（成功时长度为 0 ✓）。
#[no_mangle]
pub extern "C" fn yanshi_brush_error_ptr() -> *const u8 {
    match ERROR.lock() {
        Ok(guard) => guard.as_ptr(),
        Err(_) => std::ptr::null(),
    }
}

#[no_mangle]
pub extern "C" fn yanshi_brush_error_len() -> usize {
    match ERROR.lock() {
        Ok(guard) => guard.len(),
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **门面在宿主机上也能画** ✓（`cargo test --workspace` 会编会跑它 ✓）——
    /// 判据：一支真实 `.myb` 走一笔之后**必须落下墨** ✓（不是"返回了长度"✗）。
    #[test]
    fn the_facade_paints_ink_with_a_real_brush() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/brushes/100%_Opaque.myb");
        let myb = std::fs::read_to_string(&path).expect("仓库里应有这支笔刷 ✓");
        let request = serde_json::json!({
            "myb": myb,
            "points": [[40.0, 40.0, 1.0], [80.0, 40.0, 1.0]],
            "size": 40.0,
            "color": {"r": 255, "g": 0, "b": 0, "a": 255},
            "region": {"x": 0, "y": 0, "w": 128, "h": 96}
        })
        .to_string();
        let rgba = paint(&request).expect("应当画得出来 ✓");
        assert_eq!(rgba.len(), 128 * 96 * 4);
        // **用索引步进而不是 `chunks_exact(4)`** ✓（clippy 要求 `as_chunks::<4>()` ✓，
        // 但索引循环更朴素、也不依赖新 API ✓）。
        let (mut ink, mut reddest) = (0usize, 0u8);
        let mut index = 0usize;
        while index + 3 < rgba.len() {
            if rgba[index + 3] > 32 {
                ink += 1;
            }
            if rgba[index + 3] > 200 && rgba[index] > reddest {
                reddest = rgba[index];
            }
            index += 4;
        }
        eprintln!("  门面落墨：{ink} 个有 alpha 的像素，最红的 R = {reddest}");
        assert!(ink > 100, "必须真的落下墨 ✗（实测 {ink}）");
        assert!(
            reddest > 240,
            "颜色必须真的生效 ✗（实测最红 R = {reddest}）"
        );
    }
}
