//! **介质插件宿主** ✓ —— 让**服务端/MCP 侧**也能画出油画、水彩等质感 ✓（真实用户 P1-3 ✓）。
//!
//! **为什么需要单独一个 crate** ✓：服务端 crate **禁止 `unsafe`** ✓（这是好约束 ✓，不破 ✓），
//! 而调 C ABI 必须 `unsafe` ✗ ⇒ 把 FFI **关在一个地方** ✓，对外只暴露**安全接口** ✓。
//!
//! **为什么现在"六个都能链进来"** ✓：插件原本**导出同名符号** ✗ ⇒ 链接器报重复符号 ✗
//! ⇒ 于是给它们加了**按目标平台分叉的导出名** ✓：
//! wasm 构建照旧是 `yanshi_dab` ✓（**已发布 ABI 不变** ✓），原生构建是 `yanshi_oil_dab` 等 ✓
//! ⇒ 六个插件的原生符号**互不冲突** ✓ ⇒ 可以同时链进同一个二进制 ✓✓。
//!
//! **与浏览器端逐条对齐** ✓（否则同一条笔触两侧画出来不一样 ✗）：
//! 输入缓冲 10 个 f32（笔尖 RGBA / 目标 RGBA / 载墨 / 湿度 ✓）、每枚 dab **递增取种** ✓、
//! 细笔尖按压力换算 `max(2, round(size * (0.45 + 0.55p)))` ✓ 且**居中** ✓、**source-over** 叠加 ✓。

use std::sync::Mutex;
use yanshi_core::{Bbox, ErrorCode, ErrorContext, Result, YanshiError};

/// 调插件时的模块级锁 ✓（插件缓冲是全局的 ✓）。
static MEDIUM_LOCK: Mutex<()> = Mutex::new(());

/// 一个介质的元信息 ✓（**id + version 随对象记录** ✓ —— 设计 11.1 的硬要求 ✓）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediumSpec {
    /// 插件 id ✓。
    pub id: &'static str,
    /// 插件版本 ✓。
    pub version: u32,
    /// 单枚 dab 的最大边长 ✓（插件自报 ✓）。
    pub max_dab: u32,
}

/// 已随仓库发行的六个介质 ✓（与查看器的介质表一致 ✓）。
pub const MEDIUMS: [MediumSpec; 6] = [
    MediumSpec {
        id: "oil",
        version: 2,
        max_dab: 64,
    },
    MediumSpec {
        id: "watercolor",
        version: 2,
        max_dab: 64,
    },
    MediumSpec {
        id: "marker",
        version: 2,
        max_dab: 64,
    },
    MediumSpec {
        id: "pencil",
        version: 2,
        max_dab: 64,
    },
    MediumSpec {
        id: "pixel",
        version: 2,
        max_dab: 64,
    },
    MediumSpec {
        id: "example",
        version: 1,
        max_dab: 64,
    },
];

/// 按名字取介质 ✓（未知 ⇒ `None`，由调用方给出**列出可用值**的报错 ✓）。
pub fn spec(name: &str) -> Option<MediumSpec> {
    MEDIUMS.iter().copied().find(|medium| medium.id == name)
}

/// 可用介质名 ✓（报错时用 ✓）。
pub fn medium_names() -> String {
    MEDIUMS.iter().map(|m| m.id).collect::<Vec<_>>().join(" / ")
}

/// 一枚 dab 的上下文 ✓（照 ABI v2 的 10 个 f32 ✓）。
#[derive(Clone, Copy, Debug)]
pub struct DabInput {
    /// 笔尖色 RGBA ✓（0..1 ✓）。
    pub tip: [f32; 4],
    /// 目标处已有色 RGBA ✓（混色用 ✓）。
    pub target: [f32; 4],
    /// 载墨 ✓。
    pub load: f32,
    /// 湿度 ✓。
    pub wetness: f32,
}

/// **归一化输入 → 10 个 f32** ✓。
fn floats_of(input: DabInput) -> [f32; 10] {
    [
        input.tip[0],
        input.tip[1],
        input.tip[2],
        input.tip[3],
        input.target[0],
        input.target[1],
        input.target[2],
        input.target[3],
        input.load,
        input.wetness,
    ]
}

/// **细笔尖 + 压力换算** ✓（与浏览器端**同一公式** ✓）。
pub fn dab_size_for(size: f64, pressure: f64) -> u32 {
    let scaled = size * (0.45 + 0.55 * pressure.clamp(0.0, 1.0));
    (scaled.round() as i64).max(2) as u32
}

/// 把宿主上下文写进插件的输入缓冲 ✓（v2 插件才有 ✓）。
///
/// # Safety
/// 调用方必须持有 `MEDIUM_LOCK` ✓，且传入的指针/长度来自**同一个插件** ✓。
unsafe fn write_input(ptr: usize, len: u32, input: DabInput) {
    let floats = floats_of(input);
    let buffer = std::slice::from_raw_parts_mut(ptr as *mut f32, (len / 4) as usize);
    for (slot, value) in buffer.iter_mut().zip(floats.iter()) {
        *slot = *value;
    }
}

/// 读回插件写好的 dab ✓。
///
/// # Safety
/// 调用方必须持有 `MEDIUM_LOCK` ✓，且 `written`/`ptr` 来自**刚返回的那个插件** ✓。
unsafe fn read_dab(written: u32, ptr: usize) -> Option<(u32, Vec<u8>)> {
    if written == 0 {
        return None;
    }
    let bytes = std::slice::from_raw_parts(ptr as *const u8, written as usize).to_vec();
    Some((written / 4, bytes))
}

/// 拿一次锁 ✓（插件缓冲是全局的 ✓）。
fn lock() -> std::sync::MutexGuard<'static, ()> {
    MEDIUM_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 六个介质各一个四行小函数 ✓ —— **比宏更直白** ✓，也不用跟宏的卫生性规则较劲 ✗。
///
/// 每个函数调的都是该插件**唯一的原生导出** ✓（`yanshi_oil_dab` 等 ✓）
/// ⇒ 六个插件的符号因此**互不冲突** ✓ ⇒ 它们能同时链进同一个二进制 ✓。
fn oil_dab(seed: u32, size: u32, pressure: u32, input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：持锁 + 指针长度来自同一插件 ✓。
    unsafe {
        write_input(
            yanshi_medium_oil::yanshi_oil_input_ptr(),
            yanshi_medium_oil::yanshi_oil_input_len(),
            input,
        );
        let written = yanshi_medium_oil::yanshi_oil_dab(seed, size, pressure);
        read_dab(written, yanshi_medium_oil::yanshi_oil_dab_ptr())
    }
}

fn watercolor_dab(seed: u32, size: u32, pressure: u32, input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：同上 ✓。
    unsafe {
        write_input(
            yanshi_medium_watercolor::yanshi_watercolor_input_ptr(),
            yanshi_medium_watercolor::yanshi_watercolor_input_len(),
            input,
        );
        let written = yanshi_medium_watercolor::yanshi_watercolor_dab(seed, size, pressure);
        read_dab(
            written,
            yanshi_medium_watercolor::yanshi_watercolor_dab_ptr(),
        )
    }
}

fn marker_dab(seed: u32, size: u32, pressure: u32, input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：同上 ✓。
    unsafe {
        write_input(
            yanshi_medium_marker::yanshi_marker_input_ptr(),
            yanshi_medium_marker::yanshi_marker_input_len(),
            input,
        );
        let written = yanshi_medium_marker::yanshi_marker_dab(seed, size, pressure);
        read_dab(written, yanshi_medium_marker::yanshi_marker_dab_ptr())
    }
}

fn pencil_dab(seed: u32, size: u32, pressure: u32, input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：同上 ✓。
    unsafe {
        write_input(
            yanshi_medium_pencil::yanshi_pencil_input_ptr(),
            yanshi_medium_pencil::yanshi_pencil_input_len(),
            input,
        );
        let written = yanshi_medium_pencil::yanshi_pencil_dab(seed, size, pressure);
        read_dab(written, yanshi_medium_pencil::yanshi_pencil_dab_ptr())
    }
}

fn pixel_dab(seed: u32, size: u32, pressure: u32, input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：同上 ✓。
    unsafe {
        write_input(
            yanshi_medium_pixel::yanshi_pixel_input_ptr(),
            yanshi_medium_pixel::yanshi_pixel_input_len(),
            input,
        );
        let written = yanshi_medium_pixel::yanshi_pixel_dab(seed, size, pressure);
        read_dab(written, yanshi_medium_pixel::yanshi_pixel_dab_ptr())
    }
}

/// **`example` 是 v1** ✓：它**没有**输入缓冲接口 ✓ ⇒ 不写上下文 ✓，只调 dab 与 dab_ptr ✓。
fn example_dab(seed: u32, size: u32, pressure: u32, _input: DabInput) -> Option<(u32, Vec<u8>)> {
    let _guard = lock();
    // SAFETY：持锁 + 指针长度来自该插件 ✓。
    unsafe {
        let written = yanshi_medium_example::yanshi_example_dab(seed, size, pressure);
        read_dab(written, yanshi_medium_example::yanshi_example_dab_ptr())
    }
}

/// 画**一枚** dab ⇒ `(像素个数, RGBA 字节)` ✓。
///
/// **注意这不是边长** ✗：插件返回的 `written` 是 `size²×4` **字节** ✓ ⇒ `written/4` 是**像素个数** ✓；
/// **边长要开平方** ✓ —— 我第一版直接拿像素个数当边长 ✓ ⇒ 自检 `side*side*4` 必然失败 ✗
/// ⇒ 表现成"一笔都没落上" ✓（而插件其实一切正常 ✓）。**这类"单位搞错"只有打印出来才看得见** ✓。
pub fn dab(
    medium: &MediumSpec,
    seed: u32,
    size: u32,
    pressure_milli: u32,
    input: DabInput,
) -> Option<(u32, Vec<u8>)> {
    match medium.id {
        "oil" => oil_dab(seed, size, pressure_milli, input),
        "watercolor" => watercolor_dab(seed, size, pressure_milli, input),
        "marker" => marker_dab(seed, size, pressure_milli, input),
        "pencil" => pencil_dab(seed, size, pressure_milli, input),
        "pixel" => pixel_dab(seed, size, pressure_milli, input),
        _ => example_dab(seed, size, pressure_milli, input),
    }
}

/// **源覆盖叠加** ✓（与浏览器端 `drawImage` 的默认行为一致 ✓ —— 直通 alpha ✓）。
fn composite_over(
    dst: &mut [u8],
    src: &[u8],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
    dab: usize,
) {
    for row in 0..dab {
        for col in 0..dab {
            let sx = x + col as isize;
            let sy = y + row as isize;
            if sx < 0 || sy < 0 || sx as usize >= width || sy as usize >= height {
                continue;
            }
            let s = (row * dab + col) * 4;
            let d = (sy as usize * width + sx as usize) * 4;
            let sa = f32::from(src[s + 3]) / 255.0;
            if sa <= 0.0 {
                continue;
            }
            for channel in 0..3 {
                let sd = f32::from(src[s + channel]);
                let dd = f32::from(dst[d + channel]);
                dst[d + channel] = (sd * sa + dd * (1.0 - sa)).round().clamp(0.0, 255.0) as u8;
            }
            let da = f32::from(dst[d + 3]) / 255.0;
            dst[d + 3] = ((sa + da * (1.0 - sa)) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

/// **一整笔介质** ✓：折线走一遍 ✓，每个采样点盖一枚 dab ✓。
///
/// 返回 `(区域的文档坐标包围盒, 该区域的直通 RGBA8)` ✓ ⇒ 交给 `import_image` 就是**一条原子** ✓，
/// 与浏览器端提交的东西**同形** ✓（渲染、回放、撤销全都复用 ✓）。
pub fn paint_stroke(
    medium_name: &str,
    points: &[(f64, f64, f64)],
    size: f64,
    color: [f32; 4],
    load: f64,
    wetness: f64,
) -> Result<(Bbox, Vec<u8>)> {
    let medium = spec(medium_name).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("未知介质 {medium_name} ⇒ 可用：{}", medium_names())),
        )
    })?;
    if points.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("介质笔触需要至少一个点"),
        ));
    }
    if size < 1.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("笔尖大小要 ≥ 1（实测 {size}）")),
        ));
    }
    let half = size / 2.0 + 2.0;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y, _) in points {
        min_x = min_x.min(*x);
        min_y = min_y.min(*y);
        max_x = max_x.max(*x);
        max_y = max_y.max(*y);
    }
    let origin_x = (min_x - half).floor();
    let origin_y = (min_y - half).floor();
    let width = ((max_x + half).ceil() - origin_x).max(1.0) as usize;
    let height = ((max_y + half).ceil() - origin_y).max(1.0) as usize;
    if width.saturating_mul(height) > 4096 * 4096 {
        return Err(YanshiError::new(
            ErrorCode::ResourceExhausted,
            ErrorContext::detail(format!(
                "介质笔触区域过大（{width}×{height}）⇒ 请缩小笔尖或拆成几笔"
            )),
        ));
    }
    let mut region = vec![0u8; width * height * 4];
    let mut counter: u32 = 0;
    let mut painted = 0usize;
    for (index, (x, y, pressure)) in points.iter().enumerate() {
        let pressure = pressure.clamp(0.0, 1.0);
        let dab_size = dab_size_for(size, pressure).min(medium.max_dab);
        counter = counter.wrapping_add(1);
        let seed = counter.wrapping_mul(0x9E37_79B1) ^ 0x5BF0_3635;
        let input = DabInput {
            tip: color,
            // **服务端不回读画布** ✗（没有廉价来源 ✓）⇒ 混色退化为"笔尖自身" ✓。
            // **如实写在文档里** ✓，不假装与浏览器端等效 ✗。
            target: color,
            load: (load * (1.0 - index as f64 / points.len() as f64)).clamp(0.0, 1.0) as f32,
            wetness: wetness.clamp(0.0, 1.0) as f32,
        };
        let Some((pixels, bytes)) = dab(&medium, seed, dab_size, (pressure * 1000.0) as u32, input)
        else {
            continue;
        };
        // **边长 = sqrt(像素个数)** ✓（`written/4` 是像素个数 ✓，不是边长 ✗）。
        if pixels == 0 {
            continue;
        }
        let side = (pixels as f64).sqrt().round() as usize;
        if side == 0 || bytes.len() < side * side * 4 {
            continue;
        }
        let inset = ((size - side as f64) / 2.0).round();
        let cx = (*x - origin_x - size / 2.0 + inset).round() as isize;
        let cy = (*y - origin_y - size / 2.0 + inset).round() as isize;
        composite_over(&mut region, &bytes, width, height, cx, cy, side);
        painted += 1;
    }
    if painted == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("介质一笔都没落上（载墨为 0？）"),
        ));
    }
    Ok((
        Bbox::new(origin_x, origin_y, width as f64, height as f64),
        region,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **六个介质都能链进来、都能画** ✓ —— 这条就是 P1-3 的可达性证明 ✓。
    #[test]
    fn all_six_plugins_link_and_paint() {
        assert_eq!(MEDIUMS.len(), 6);
        for medium in MEDIUMS {
            let points = vec![(20.0, 20.0, 1.0), (32.0, 28.0, 0.7), (44.0, 36.0, 0.4)];
            let (bbox, rgba) =
                paint_stroke(medium.id, &points, 22.0, [0.85, 0.25, 0.12, 1.0], 1.0, 0.5)
                    .unwrap_or_else(|error| panic!("{} 应当画得出来：{error:?}", medium.id));
            let opaque = rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count();
            assert!(
                bbox.w > 10.0 && bbox.h > 5.0,
                "{} 的区域应当覆盖笔迹",
                medium.id
            );
            assert!(opaque > 30, "{} 应当真的落墨（实测 {opaque}）", medium.id);
        }
    }

    /// **压力换算与浏览器端同式** ✓（两侧画得一样的前提 ✓）。
    #[test]
    fn dab_size_follows_pressure_with_a_floor() {
        assert_eq!(dab_size_for(40.0, 1.0), 40);
        assert_eq!(dab_size_for(40.0, 0.0), 18);
        assert_eq!(dab_size_for(4.0, 0.0), 2);
    }

    /// **画出来的笔痕有层次** ✓（若只有一种色阶 ⇒ 插件其实没被真调 ✗）。
    #[test]
    fn the_brush_texture_has_layers() {
        let points = vec![(16.0, 16.0, 1.0), (28.0, 22.0, 1.0), (40.0, 30.0, 1.0)];
        let (_, rgba) = paint_stroke("oil", &points, 26.0, [0.8, 0.2, 0.1, 1.0], 1.0, 0.4).unwrap();
        let mut shades: Vec<u16> = rgba
            .chunks_exact(4)
            .filter(|pixel| pixel[3] > 0)
            .map(|pixel| u16::from(pixel[0]) + u16::from(pixel[1]) + u16::from(pixel[2]))
            .collect();
        shades.sort_unstable();
        shades.dedup();
        assert!(
            shades.len() > 4,
            "笔痕应当有层次（实测 {} 种）",
            shades.len()
        );
    }

    /// **同输入 ⇒ 逐字节相同** ✓。
    #[test]
    fn the_same_stroke_paints_the_same_bytes() {
        let points = vec![(10.0, 10.0, 1.0), (24.0, 18.0, 0.6)];
        let first =
            paint_stroke("watercolor", &points, 18.0, [0.2, 0.4, 0.9, 1.0], 0.8, 0.7).unwrap();
        let second =
            paint_stroke("watercolor", &points, 18.0, [0.2, 0.4, 0.9, 1.0], 0.8, 0.7).unwrap();
        assert_eq!(first.1, second.1);
    }

    /// **未知介质要列出可用值** ✓（用户 §五-18 的原话 ✓）。
    #[test]
    fn unknown_medium_lists_the_available_ones() {
        let error =
            paint_stroke("gouache", &[(0.0, 0.0, 1.0)], 8.0, [1.0; 4], 1.0, 0.0).unwrap_err();
        let detail = error.context.detail.clone().unwrap_or_default();
        assert!(detail.contains("gouache"), "应当点名那个介质：{detail}");
        assert!(detail.contains("oil"), "应当列出可用介质：{detail}");
    }
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    /// **最小往返** ✓：把上下文写进插件的输入缓冲 ✓，再**从同一地址读回来** ✓。
    /// 用来区分"写没写进去"与"插件逻辑不接受" ✓。
    #[test]
    fn the_input_buffer_round_trips() {
        let _guard = lock();
        let ptr = yanshi_medium_oil::yanshi_oil_input_ptr();
        let len = yanshi_medium_oil::yanshi_oil_input_len();
        eprintln!(
            "  oil input_ptr={ptr} len={len} max_dab={}",
            yanshi_medium_oil::yanshi_oil_max_dab()
        );
        // **不要锁死具体长度** ✗：插件可以**向后兼容地加字段** ✓
        //（真实用户报告"油画纹理过重"之后 ✓，油画的输入缓冲就从 10 个 f32 变成了 11 ✓）。
        // 本探针要证的是"**往返一致**" ✓，不是"长度永远等于 40" ✗ ——
        // 锁死常数只会让**合理的扩展**把测试弄红 ✓，而那种红**没有信息量** ✗。
        assert!(
            len >= 40 && len % 4 == 0,
            "输入缓冲应当是若干完整的 f32（实测 {len} 字节）"
        );
        let input = DabInput {
            tip: [0.8, 0.2, 0.1, 1.0],
            target: [0.0, 0.0, 0.0, 1.0],
            load: 1.0,
            wetness: 0.4,
        };
        // SAFETY：持锁 ✓，指针长度由插件自报 ✓。
        unsafe { write_input(ptr, len, input) };
        // 读回来 ✓（不经过插件逻辑 ✓）
        let read = unsafe { std::slice::from_raw_parts(ptr as *const f32, (len / 4) as usize) };
        eprintln!("  读回来：{:?}", read);
        assert_eq!(read[0], 0.8, "笔尖色应当写进去了");
        assert_eq!(read[8], 1.0, "载墨应当写进去了");
        // 再看插件到底认不认 ✓
        let written = yanshi_medium_oil::yanshi_oil_dab(1, 16, 1000);
        eprintln!("  插件返回 written={written}");
        assert!(written > 0, "插件应当落墨（读到的载墨/色都非 0）");
    }
}
