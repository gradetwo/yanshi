//! **从 `yanshi-brush-wasm` 照抄过来的笔刷预览内核** ✓（(A)③：让**一份**实现同时服务服务端与浏览器 ✓）。
//!
//! **纪律** ✓：这里是**照抄**，**不改语义** ✗ —— 改语义就等于造出**第三份实现** ✓，
//! 而那正是本项要消灭的东西 ✓。C-ABI 那层（`#[no_mangle]` 的六个导出 ✓）**没有搬** ✓ ——
//! 绑定生成（wasm-bindgen）会接管内存管道 ✓（见第 189 轮 ✓）。
//!
//! 与门面的唯一差别 ✓：可见性（`pub(crate)` ✓，供 `lib.rs` 里的 bindgen 方法调用 ✓）。
#![allow(dead_code)]

use serde::Deserialize;

/// **最后一次失败的原因** ✓ —— 门面当年有独立的错误通道 ✓，我第一版把它丢了 ✗
/// ⇒ "失败 = 空" ✗ ⇒ **说不出为什么** ✓（而本仓库最反对这个 ✗）⇒ 现在补回来 ✓。
static LAST_ERROR: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// 记下原因 ✓（由下面的 `paint` 在失败时调用 ✓）。
pub(crate) fn set_error(reason: &str) {
    if let Ok(mut slot) = LAST_ERROR.lock() {
        *slot = reason.to_owned();
    }
}

/// 取走原因 ✓（取走即清 ✓ —— 免得把一次失败的原因**粘到下一次成功**上 ✗）。
pub(crate) fn take_error() -> String {
    LAST_ERROR.lock().map(|mut slot| std::mem::take(&mut *slot)).unwrap_or_default()
}

#[derive(Deserialize)]
pub(crate) struct Colour {
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

pub(crate) fn opaque_alpha() -> u8 {
    255
}

#[derive(Deserialize)]
pub(crate) struct Region {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[derive(Deserialize)]
pub(crate) struct PaintRequest {
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
pub(crate) fn colour_to_hsv(colour: &Colour) -> (f32, f32, f32) {
    // **只留一处实现** —— 与服务端**同一个函数**（第 36 轮收拢 ✓）。
    // **不要再除以 360** ✗：那一处回的**已经是 0–1 的圆周分数** ✓（它自己除了 360 ✓）。
    // 第 73 轮就是在这里多除了一次 ✗ ⇒ 蓝被画成红 ✓ ⇒ 15 组判据里只有 blue 三组红 ✓。
    yanshi_render::color::rgb_to_hsv(colour.r, colour.g, colour.b)
}

/// 用**请求里的设置**把笔刷调好 ✓（顺序与服务端一致 ✓：不透明度 → 硬度 → 颜色 → 大小 ✓）。
pub(crate) fn configure(mut brush: hokusai::Brush, request: &PaintRequest) -> hokusai::Brush {
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
pub(crate) fn stamp(
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
pub(crate) fn read_back(surface: &hokusai::tile_mem::MemSurface, region: &Region) -> Vec<u8> {
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
pub(crate) fn paint(request_json: &str) -> Result<Vec<u8>, String> {
    let request: PaintRequest = match serde_json::from_str(request_json) {
        Ok(request) => request,
        Err(error) => {
            let reason = format!("请求不是合法 JSON：{error}");
            set_error(&reason);
            return Err(reason);
        }
    };
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
