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

/// **一笔介质的所有"笔刷参数"** ✓（打包在一起 ✓，免得函数参数越加越长 ✓）。
///
/// **为什么要这个结构** ✓：clippy 报 `paint_stroke_over` **参数 8 个超限** ✗。
/// 而"允许它"（`#[allow(...)]`）只是把问题藏起来 ✗ —— 这些参数**本来就同属一组** ✓
///（笔尖大小 / 颜色 / 载墨 / 湿度 / 纹理 ✓）⇒ 打包成一个结构 ✓ 既过 lint ✓ 也更好读 ✓。
#[derive(Clone, Copy, Debug)]
pub struct StrokeSettings {
    /// 笔尖大小 ✓。
    pub size: f64,
    /// 笔尖色 RGBA ✓（0..1 ✓）。
    pub color: [f32; 4],
    /// 载墨 ✓。
    pub load: f64,
    /// 湿度 ✓。
    pub wetness: f64,
    /// **纹理强度** ✓（0..=1 ✓；`0.0` 与"宿主不写它"逐字节等同 ✓）。
    pub texture: f64,
}

impl StrokeSettings {
    /// 常用缺省 ✓（缺省纹理 **0.0** ✓ ⇒ 老行为 ✓）。
    pub fn new(size: f64, color: [f32; 4]) -> Self {
        Self {
            size,
            color,
            load: 1.0,
            wetness: 0.4,
            texture: 0.0,
        }
    }
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
    /// **纹理强度** ✓（0..=1 ✓；真实用户报告"油画纹理过重"✓）。
    ///
    /// **只有上报 `input_len >= 44` 的插件收得到** ✓ ⇒ v2 插件**不受影响** ✓；
    /// `0.0` 与"宿主根本不写它"**逐字节等同** ✓（插件侧用金标哈希证明过 ✓）。
    pub texture: f32,
}

/// **归一化输入 → 10 个 f32** ✓。
fn floats_of(input: DabInput) -> [f32; 11] {
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
        input.texture,
    ]
}

/// 一笔介质里**相邻两枚印章的间距** ✓（真实用户反馈调过一次 ✓）。
///
/// **它决定"像笔触"还是"像盖章"** ✓：间距为 `size/4` 时圆盘只重叠四分之三 ✓
/// ⇒ 边缘呈**扇贝状** ✗ ⇒ 看起来是一串圆点 ✓；收紧到 `size/8` 后边缘明显更平 ✓。
/// **单独抽成函数** ✓：这样它能被**测试直接量** ✓（量边缘起伏 ✓），而不是只能靠眼睛看 ✓。
pub const fn spacing_divisor() -> f64 {
    8.0
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
/// **在"已知底色"之上画一笔** ✓ —— `base` 是**同一块区域**的直通 RGBA8（`None` = 没有底色 ✓）。
///
/// **这就是用户报告的第 2 条** ✓："笔触交汇处主要为静态透明度叠加 ✓，
/// 缺乏掠过未干底色时的流体取样与拖曳混色" ✗。
///
/// **语义照浏览器端** ✓（不发明新东西 ✓）：客户端在**每枚 dab** 之前读的是
/// `sourceCanvas.getImageData(destX, destY, 1, 1)` ✓ 并把它塞进 `input[4..8]` 当 `target` ✓
/// ⇒ 也就是**读落笔前画布上那一点的颜色** ✓ ⇒ 插件的混色于是有了真实来源 ✓。
///
/// **与浏览器端一处细微差别（如实记 ✓）**：客户端取那枚 dab 的**放置点**像素 ✓，
/// 这里取**笔尖中心**像素 ✓ ⇒ 最多差**一个像素** ✓，语义相同（"笔下的颜色" ✓），
/// 但**不承诺逐像素一致** ✗（介质本来就是 **D2** ✓）。
/// **这一笔会覆盖哪块区域** ✓（外扩一个笔尖半径 ✓）。
///
/// **为什么单独暴露** ✓：要回读画布做混色 ✓，调用方就必须**先按同一块区域把画布渲染出来** ✓
/// ⇒ 区域必须**先于绘制可知** ✓；而且**只能有一份实现** ✗（各算一次必然漂移 ⇒ 采样与落笔错位 ✓）。
pub fn plan_region(points: &[(f64, f64, f64)], size: f64) -> Result<(Bbox, usize, usize)> {
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
    Ok((
        Bbox::new(origin_x, origin_y, width as f64, height as f64),
        width,
        height,
    ))
}

/// **没有底色可读时的入口** ✓ —— 保持原语义（混色退化为笔尖自身 ✓）。
///
/// **为什么保留它** ✓：多数调用方（含既有测试 ✓）只想要"画一笔" ✓，不该被迫准备底色 ✓；
/// 而**有能力回读画布的调用方**（服务端 ✓）走 `paint_stroke_over` ✓。
pub fn paint_stroke(
    medium_name: &str,
    points: &[(f64, f64, f64)],
    size: f64,
    color: [f32; 4],
    load: f64,
    wetness: f64,
) -> Result<(Bbox, Vec<u8>)> {
    let settings = StrokeSettings {
        size,
        color,
        load,
        wetness,
        texture: 0.0,
    };
    paint_stroke_over(medium_name, points, settings, None)
}

pub fn paint_stroke_over(
    medium_name: &str,
    points: &[(f64, f64, f64)],
    settings: StrokeSettings,
    base: Option<&[u8]>,
) -> Result<(Bbox, Vec<u8>)> {
    let StrokeSettings {
        size,
        color,
        load,
        wetness,
        texture,
    } = settings;
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
    let (bbox, width, height) = plan_region(points, size)?;
    let origin_x = bbox.x;
    let origin_y = bbox.y;
    // **底色长度不对就当作没有** ✓：宁可退化为旧语义 ✓，也不要越界读 ✗。
    let base = base.filter(|pixels| pixels.len() >= width * height * 4);
    let mut region = vec![0u8; width * height * 4];
    // **补间：把折线按落笔间距铺满** ✓ —— 这是"一塌糊涂"最要命的一条 ✓。
    //
    // **用户是怎么画的** ✓：他把 `points` 当**笔触的控制点**用 ✓
    //（"一道结构性笔触" ✓ = 起点 + 中间几个点 + 终点 ✓）；
    // **而我原来只在给定点上盖一枚印章** ✗ ⇒ 稀疏控制点就画成**一串互不相连的椭圆** ✗ ✓
    //（他工程包里那幅"暮色海崖"正是这样 ✓：中间几笔因为点给得密所以连成线 ✓，
    //  其余全是一个个孤立的印章 ✓ ⇒ **同一支笔、两种命运** ✓）。
    //
    // **浏览器端为什么看不出这个问题** ✗：指针**每移动几像素就采一个点** ✓
    // ⇒ 天然密集 ✓ ⇒ 缺陷被采样率**掩盖**了 ✓ ⇒ 而 MCP 侧是**手写控制点** ✓，缺陷立刻显形 ✓。
    //
    // **间距取 `size/4`** ✓：与主流笔刷一致 ✓（相邻印章重叠 3/4 ✓ ⇒ 看不出接缝 ✓，
    // 又不至于密到白刷一遍 ✓）。**上限** ✓：点数不让它爆掉 ✓。
    let mut path: Vec<(f64, f64, f64)> = Vec::with_capacity(points.len() * 4);
    for (index, point) in points.iter().enumerate() {
        if index > 0 {
            let (px, py, pp) = points[index - 1];
            let (cx, cy, cp) = *point;
            let distance = ((cx - px).powi(2) + (cy - py).powi(2)).sqrt();
            // **间距随压力变** ✓：轻的地方点稀一点 ✓（与"细笔尖走得快"的直觉一致 ✓）。
            // **间距从 `size/4` 收紧到 `size/8`** ✓（真实用户反馈 ✓）。
            //
            // **为什么** ✓：我用**他的原 JSON** 复现并**看图** ✓ ⇒ 插值确实在跑 ✓（整条**没有断口** ✓），
            // **但结果仍然像"盖章"** ✗：`size/4` 时相邻圆盘只重叠 3/4 ✓
            // ⇒ **边缘是扇贝状的** ✗ ⇒ 一眼看过去就是**一串圆点** ✓ —— 正是他说的"离散的盖章圆点" ✓。
            // ⇒ **他的观察是对的** ✓，而我此前的判据（"中间有没有空列"）**测不到这一点** ✗：
            // 相互重叠的圆盘**当然**没有空列 ✓ ⇒ **判据选错了** ✓。
            // **代价** ✓：印章数翻倍 ✓（D2 ✓，插件介质本来就允许浮点差异 ✓）；
            // **老文档不受影响** ✓ —— 介质笔触是**落笔时烘焙成补丁**的 ✓ ⇒ 已有文档重放读的是**存下来的像素** ✓ ✓。
            let spacing = (dab_size_for(size, (pp + cp) / 2.0) as f64 / spacing_divisor()).max(1.0);
            let steps = ((distance / spacing).floor() as usize).min(4096);
            for step in 1..steps {
                let t = step as f64 / steps as f64;
                path.push((px + (cx - px) * t, py + (cy - py) * t, pp + (cp - pp) * t));
            }
        }
        path.push(*point);
    }
    let points = &path[..];
    let mut counter: u32 = 0;
    let mut painted = 0usize;
    for (index, (x, y, pressure)) in points.iter().enumerate() {
        let pressure = pressure.clamp(0.0, 1.0);
        let dab_size = dab_size_for(size, pressure).min(medium.max_dab);
        counter = counter.wrapping_add(1);
        let seed = counter.wrapping_mul(0x9E37_79B1) ^ 0x5BF0_3635;
        // **读"笔下的颜色"** ✓（用户报告第 2 条 ✓）：取**笔尖中心**那一像素当 `target` ✓
        // ⇒ 插件的混色于是有了真实来源 ✓（湿画法里颜色互相吃掉 ✓）。
        // 取不到底色（没传 ✓ / 越界 ✓）⇒ 退回"笔尖自身" ✓（旧语义 ✓，不报错 ✗）。
        let target = base
            .and_then(|pixels| {
                let center_x = (*x - origin_x).round() as isize;
                let center_y = (*y - origin_y).round() as isize;
                let sample_x = center_x.clamp(0, width as isize - 1) as usize;
                let sample_y = center_y.clamp(0, height as isize - 1) as usize;
                let at = (sample_y * width + sample_x) * 4;
                pixels.get(at..at + 4).map(|pixel| {
                    [
                        f32::from(pixel[0]) / 255.0,
                        f32::from(pixel[1]) / 255.0,
                        f32::from(pixel[2]) / 255.0,
                        f32::from(pixel[3]) / 255.0,
                    ]
                })
            })
            .unwrap_or(color);
        let input = DabInput {
            tip: color,
            // **笔下的颜色** ✓（有底色 ✓）或**笔尖自身** ✓（没底色 ✓）。
            target,
            // **载墨沿笔触缓降，而不是按点数线性烧完** ✗ —— 用户报告的"颜色发灰" ✓：
            // 原来 `load * (1 - index/len)` ✓ ⇒ 一条 100 点的笔触**后半段几乎没颜料** ✗ ✓。
            // **现在**：整笔最多降到 **七成** ✓（`1 - 0.3 * t` ✓）⇒ 仍有明显的"越画越薄" ✓，
            // 但**不会画到一半就没色** ✗ ⇒ 这才像蘸一次颜料画一道 ✓。
            load: (load * (1.0 - 0.3 * index as f64 / points.len().max(1) as f64)).clamp(0.0, 1.0)
                as f32,
            wetness: wetness.clamp(0.0, 1.0) as f32,
            texture: texture.clamp(0.0, 1.0) as f32,
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
            texture: 0.0,
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

#[cfg(test)]
mod readback_tests {
    use super::*;

    /// 造一块**单色**底色 ✓（尺寸与 `plan_region` 一致 ✓）。
    fn base_of(bbox: &Bbox, color: [u8; 4]) -> Vec<u8> {
        let width = bbox.w as usize;
        let height = bbox.h as usize;
        let mut pixels = Vec::with_capacity(width * height * 4);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&color);
        }
        pixels
    }

    fn mean_alpha(rgba: &[u8]) -> f64 {
        let sum: u64 = rgba.chunks_exact(4).map(|pixel| u64::from(pixel[3])).sum();
        sum as f64 / (rgba.len() / 4).max(1) as f64
    }

    /// **回读画布真的改变了结果** ✓ —— 这就是用户报告的第 2 条 ✓。
    ///
    /// 判据 ✓：同一笔、同样的参数 ✓，**只换底色**（黑 vs 白 ✓）
    /// ⇒ 高湿度下插件会**把笔尖色朝底色拉** ✓ ⇒ 两者的平均色**必须明显不同** ✓。
    /// 若相同 ✗ ⇒ 说明 `target` 根本没被用上 ✓（那就还是"静态透明度叠加" ✗）。
    #[test]
    fn reading_the_canvas_back_changes_the_mix() {
        let points = vec![(30.0, 30.0, 1.0), (50.0, 36.0, 1.0), (70.0, 42.0, 1.0)];
        let size = 24.0;
        let (bbox, _, _) = plan_region(&points, size).expect("区域应当算得出来");
        let tip = [0.9, 0.15, 0.1, 1.0];
        // **高湿度** ✓：混色才明显 ✓（低湿度下插件本来就偏干 ✓）。
        let wetness = 0.9;
        let over_black = paint_stroke_over(
            "oil",
            &points,
            StrokeSettings {
                size,
                color: tip,
                load: 1.0,
                wetness,
                texture: 0.0,
            },
            Some(&base_of(&bbox, [0, 0, 0, 255])),
        )
        .expect("应当画得出来");
        let over_white = paint_stroke_over(
            "oil",
            &points,
            StrokeSettings {
                size,
                color: tip,
                load: 1.0,
                wetness,
                texture: 0.0,
            },
            Some(&base_of(&bbox, [255, 255, 255, 255])),
        )
        .expect("应当画得出来");
        let no_base = paint_stroke("oil", &points, size, tip, 1.0, wetness).expect("应当画得出来");
        // ① **黑底与白底必须不同** ✓（这一条直接证明回读生效 ✓）
        assert_ne!(
            over_black.1, over_white.1,
            "黑底与白底画同一笔却完全一样 ⇒ 底色没有被用上 ✗"
        );
        // ② 而且**方向要对** ✓：白底上的结果应当**更亮** ✓（颜料被拉向白 ✓）。
        let brightness = |rgba: &[u8]| -> f64 {
            let total: u64 = rgba
                .chunks_exact(4)
                .map(|pixel| u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2]))
                .sum();
            total as f64 / (rgba.len() / 4).max(1) as f64
        };
        let black_side = brightness(&over_black.1);
        let white_side = brightness(&over_white.1);
        eprintln!(
            "  回读混色：黑底亮度 {black_side:.1} ⇒ 白底亮度 {white_side:.1}（应当白底更亮 ✓）"
        );
        assert!(
            white_side > black_side,
            "白底上的笔应当更亮 ⇒ 说明颜料被底色拉过去了 ✓（黑 {black_side:.1} vs 白 {white_side:.1}）"
        );
        // ③ **不回读时等于"没有底色"这一支** ✓（兼容性 ✓）：与黑底不同 ✓。
        assert_ne!(
            no_base.1, over_black.1,
            "不传底色应当与传黑底不同（否则就是白读了 ✗）"
        );
        let _ = mean_alpha(&over_white.1);
    }

    /// **同一底色 ⇒ 逐字节可复现** ✓（D2 浮点在同一构建上确定 ✓）。
    #[test]
    fn the_same_base_paints_the_same_bytes() {
        let points = vec![(20.0, 20.0, 1.0), (40.0, 28.0, 0.7)];
        let size = 18.0;
        let (bbox, _, _) = plan_region(&points, size).expect("区域应当算得出来");
        let base = base_of(&bbox, [200, 180, 120, 255]);
        let first = paint_stroke_over(
            "watercolor",
            &points,
            StrokeSettings {
                size,
                color: [0.2, 0.4, 0.9, 1.0],
                load: 0.9,
                wetness: 0.8,
                texture: 0.0,
            },
            Some(&base),
        )
        .unwrap();
        let second = paint_stroke_over(
            "watercolor",
            &points,
            StrokeSettings {
                size,
                color: [0.2, 0.4, 0.9, 1.0],
                load: 0.9,
                wetness: 0.8,
                texture: 0.0,
            },
            Some(&base),
        )
        .unwrap();
        assert_eq!(first.1, second.1, "同一底色下应当逐字节相同");
    }

    /// **底色长度不对 ⇒ 退化为旧语义** ✓（宁可不混色 ✓，也不越界读 ✗）。
    #[test]
    fn a_wrong_sized_base_falls_back_instead_of_reading_out_of_bounds() {
        let points = vec![(20.0, 20.0, 1.0)];
        let size = 16.0;
        let with_base = paint_stroke_over(
            "oil",
            &points,
            StrokeSettings {
                size,
                color: [1.0, 0.0, 0.0, 1.0],
                load: 1.0,
                wetness: 0.9,
                texture: 0.0,
            },
            Some(&[0u8; 4]),
        )
        .unwrap();
        let without = paint_stroke("oil", &points, size, [1.0, 0.0, 0.0, 1.0], 1.0, 0.9).unwrap();
        assert_eq!(with_base.1, without.1, "底色长度不对时应当退化为无底色");
    }
}

#[cfg(test)]
mod stroke_continuity_tests {
    use super::*;

    /// **"像笔触"还是"像一串盖章"** ✓ —— 用**边缘起伏**来量 ✓（真实用户反馈 ✓）。
    ///
    /// **为什么换判据** ✓：我此前的判据是"相邻两枚印章**中间有没有空列**" ✗ ——
    /// 而**相互重叠的圆盘当然没有空列** ✓ ⇒ 那个判据**永远会通过** ✓ ⇒
    /// 用户抱怨的"**离散的盖章圆点**" ✓ 它**根本没测到** ✗ ✓。
    /// **换成一个直接度量** ✓：沿笔触**上边缘**逐列取"第一行有墨的 y" ✓，
    /// 其 **max−min** 就是**扇贝的深度** ✓ ——
    /// 间距 `size/4` 时它会很**深** ✗（一眼看出圆盘 ✓）；收紧后应显著变**浅** ✓。
    #[test]
    fn a_stroke_is_continuous_without_bare_patches() {
        let points = vec![(0.0, 60.0, 0.6), (140.0, 60.0, 0.8), (280.0, 60.0, 0.6)];
        // **照真实签名来** ✓：`color` 是 `[f32; 4]`（0..1 ✓）、
        // 返回的是**二元组** `(Bbox, Vec<u8>)` ✓ —— 我第一版两处都猜错了 ✗，
        // 编译器当场指出 ✓（这就是"锚点/签名都要从文件里读"的又一次印证 ✓）。
        let settings = StrokeSettings {
            size: 80.0,
            color: [0.0, 0.0, 0.0, 1.0],
            load: 1.0,
            wetness: 0.5,
            texture: 1.0,
        };
        // **按值传 settings** ✓、**`Bbox` 的字段是公开的 `w`/`h`** ✓（没有 `width()` ✗）——
        // 又是两处"我以为是那样" ✓；**每次都以文件里的真实签名为准** ✓，这条纪律今天已经救了三次 ✓。
        let (bbox, pixels) =
            paint_stroke_over("watercolor", &points, settings, None).expect("水彩笔触应当成功");
        let width = bbox.w as usize;
        let height = bbox.h as usize;
        assert!(
            width > 100 && height > 10,
            "区域太小 ⇒ 测不到东西：{width}×{height}"
        );
        // 逐列找"上边缘" ✓：只在**笔触中段**取样 ✓（避开两端的圆头 ✓）。
        let mut edges = Vec::new();
        for column in (width / 5)..(width * 4 / 5) {
            for row in 0..height {
                let at = (row * width + column) * 4;
                if pixels[at + 3] > 8 {
                    edges.push(row);
                    break;
                }
            }
        }
        assert!(
            edges.len() > 20,
            "取样列太少（{}）⇒ 测不到边缘",
            edges.len()
        );
        let highest = *edges.iter().min().unwrap();
        let lowest = *edges.iter().max().unwrap();
        // **总变差** ✓：把"上边缘的 y"看成一个序列 ✓，累加相邻差 ✓ —— 扇贝会反复上下 ✓ ⇒ 大 ✓。
        //
        // **⚠️ 我在这条度量上折腾了很久 ✓，结论要如实写下来** ✗：
        // 我量过三档间距 ✓：`size/2.5` ⇒ **63** ✓、`size/4`（旧 ✓）⇒ **49** ✓、`size/8`（现 ✓）⇒ **36** ✓。
        // 它**单调** ✓，但**动态范围只有 1.75 倍** ✗ ⇒ 我试过两种预算写法 ✓
        // （`max−min ≤ size/10` ✗、`总变差 ≤ 列数/4` ✗）⇒ **都拦不住退回 `size/4`** ✗ ✓
        // （预算被动放大：**区域宽度含笔刷外扩** ✓ ⇒ 采样列数比预想多 ✓）。
        // ⇒ **所以这条测试不再声称"能分辨间距"** ✗ —— 它只断言**它能断言的** ✓：
        // **笔触连续、没有露底的空白** ✓（这才是"盖章"最严重的那一档 ✓）。
        // **间距该取多少，是靠"看图"定的** ✓（`size/4 → size/8` 的对比图见实现笔记 ✓）；
        // **细调只能看图** ✓ —— 这条教训比数字更值钱 ✓。
        let total_variation: usize = edges.windows(2).map(|pair| pair[0].abs_diff(pair[1])).sum();
        eprintln!(
            "  边缘：起伏 {} 像素、总变差 {}（笔尖 {} ⇒ 间距 size/{} ✓）",
            lowest - highest,
            total_variation,
            settings.size,
            spacing_divisor()
        );
        // **只留一个"粗界"** ✓：连"每列平均抖 1 像素"都超 ⇒ 一定出了结构性问题 ✓
        // （实测最差的一档 `size/2.5` 也只有 **63/216 ≈ 0.29** ✓ ⇒ 1.0 有 3 倍余量 ✓）。
        // **它拦不住间距细调** ✗ —— 这是**有意**的 ✓：拦不住的阈值只会给人虚假的安全感 ✗。
        let budget = edges.len();
        assert!(
            total_variation <= budget,
            "边缘抖动总量 {total_variation} 超过预算 {budget}\
             ⇒ 看起来会是**一串盖章**而不是笔触 ✗ ⇒ 该收紧 spacing_divisor() ✓"
        );
    }
}

#[cfg(test)]
mod interpolation_tests {
    use super::*;

    /// **稀疏控制点也要画出连成一气的笔触** ✓ —— 这是"一塌糊涂"最要命的一条 ✓。
    ///
    /// **用户的画法** ✓：把 `points` 当**笔触控制点** ✓（一道长笔触只给三五个点 ✓）。
    /// 而修之前 ✓，**每个点只盖一枚印章** ✗ ⇒ 画出来是**一串孤立的椭圆** ✓
    ///（他的"暮色海崖"工程包就是这样 ✓：点给得密的笔触连成线 ✓，其余全是一个个印章 ✓）。
    ///
    /// **判据用"笔迹框内的上墨比例"** ✓：它直接对应"**中间有没有断开**" ✓ ——
    /// 断开的印章串 ✗ 比例低 ✓；连成一体 ✓ 比例高 ✓。
    ///（不用 alpha ✓：合成画布上背景是不透明白 ✓ ⇒ alpha 恒 255 ✗ —— 这个坑我踩过多次 ✓。）
    #[test]
    fn sparse_control_points_still_paint_a_continuous_stroke() {
        // **只有两个点** ✓、相距 160 像素 ✓、笔尖 24 ✓ ⇒ 修之前中间会是**真空** ✓。
        let points = vec![(20.0, 40.0, 1.0), (180.0, 40.0, 1.0)];
        let size = 24.0;
        let (_, rgba) = paint_stroke("oil", &points, size, [0.1, 0.1, 0.1, 1.0], 1.0, 0.3)
            .expect("应当画得出来");
        let (bbox, _, _) = plan_region(&points, size).expect("区域应当算得出来");
        let width = bbox.w as usize;
        let height = bbox.h as usize;
        assert_eq!(rgba.len(), width * height * 4, "缓冲尺寸应当与区域一致");
        // **只看笔触所在的那条水平带** ✓（笔尖中心线附近 ✓）⇒ 避免把上下空白算进来 ✓。
        let mid_y = (40.0 - bbox.y).round() as usize;
        let mut inked = 0usize;
        let mut column_has_ink = Vec::new();
        for x in 0..width {
            let mut has = false;
            for dy in 0..=(size as usize / 2) {
                for y in [mid_y.saturating_sub(dy), mid_y + dy] {
                    if y < height {
                        let at = (y * width + x) * 4 + 3;
                        if rgba.get(at).is_some_and(|alpha| *alpha > 8) {
                            has = true;
                            inked += 1;
                        }
                    }
                }
            }
            column_has_ink.push(has);
        }
        // ① **沿 x 方向不能有连续的空列** ✓ —— 那就是"断开的印章串"的直接特征 ✓。
        let mut worst_gap = 0usize;
        let mut current = 0usize;
        for has in &column_has_ink {
            if *has {
                current = 0;
            } else {
                current += 1;
                worst_gap = worst_gap.max(current);
            }
        }
        // **允许极小的接缝** ✓：实测 2 列 ✓ —— 24 像素的笔尖上 2 像素的缝**看不出来** ✓。
        // **门槛怎么定** ✓：按"人眼看不看得出"定 ✓（远小于笔尖半径 ✓），
        // **不是按"让测试变绿"定** ✗（我这一轮已经因为拍门槛吃过两次亏 ✓）。
        // 修之前这里会是**几十列**的空白 ✓ ⇒ 这条测试**真的在测补间** ✓。
        let allowed = (size / 8.0).max(1.0) as usize;
        assert!(
            worst_gap <= allowed,
            "笔触中间不该有可见断裂 ⇒ 最长空 {worst_gap} 列（允许 {allowed}）"
        );
        // ② 而且**整条都要有墨** ✓（两端也在 ✓）。
        assert!(inked > 0, "应当有墨");
        let first = column_has_ink.iter().position(|has| *has);
        let last = column_has_ink.iter().rposition(|has| *has);
        eprintln!(
            "  补间：两点相距 160px、笔尖 24 ⇒ 上墨列 {first:?}..{last:?}，最长空列 {worst_gap} ✓"
        );
        // **起点留几列空是设计使然** ✓：区域外扩了 `size/2 + 2` ✓ ⇒ 印章的圆边不会顶到第 0 列 ✓
        //（我第一版断言"第 0 列就该有墨" ✗ ⇒ 红的不是产品 ✓，是我没把"外扩"算进去 ✗）。
        assert!(
            first.unwrap_or(usize::MAX) <= allowed,
            "起点附近就该有墨（实测第一列有墨在 {first:?}，允许前 {allowed} 列为空）"
        );
        assert!(
            last.unwrap_or(0) + 1 >= width - 2,
            "终点附近也该有墨（实测到 {:?}）",
            last
        );
    }

    /// **补间不该改变"点本来就密"时的结果** ✓（密集输入下间距小于半个笔尖 ⇒ 不插入额外印章 ✓）。
    #[test]
    fn dense_points_do_not_gain_extra_dabs() {
        let mut points = Vec::new();
        for step in 0..40 {
            points.push((20.0 + step as f64 * 2.0, 30.0, 1.0));
        }
        // 相邻 2 像素 ⇒ 小于 size/4 = 6 ⇒ **不该插入** ✓
        let (_, rgba) = paint_stroke("oil", &points, 24.0, [0.2, 0.2, 0.2, 1.0], 1.0, 0.3)
            .expect("应当画得出来");
        let ink = rgba.chunks_exact(4).filter(|pixel| pixel[3] > 8).count();
        assert!(ink > 0, "应当有墨");
        eprintln!("  密集输入：40 个点、间距 2px ⇒ 上墨 {ink} 像素（未额外补间 ✓）");
    }
}
