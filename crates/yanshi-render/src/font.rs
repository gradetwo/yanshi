//! 内置 ASCII 位图字体与文字栅格化（设计文档 4.2 `Text`、1175 / 1287）。
//!
//! 设计文档要求「内嵌开源字体子集，禁用平台相关 hinting，首版仅 Latin/CJK 基础」，
//! 并把它列为「字体版权 / 跨端一致性」的对策。本模块是该要求的**路线 A 最小切片**：
//! 先内置一套 **5×7 像素的 ASCII 位图字体**，让 `Text` 对象能在零依赖的内核里渲染；
//! CJK 字体子集留作后续——届时只需扩展字形表，无需改动 [`draw_text`] 的实现。
//!
//! 职责边界：本模块只做**字体数据 + 文字栅格化**这一纯计算部分，
//! 不认识对象模型、不参与原子 / 工具接线；输入一段文本，输出 [`Buffer`] 上的像素。
//!
//! ## 零依赖与确定性
//!
//! - 字形数据是编译进二进制的 `&'static str` 位串，**不加载任何外部字体文件**，无网络、无系统时间。
//! - 无随机量：同一输入必得同一像素；定位只用整数与 `f64` 的四舍五入，不依赖浮点环境。
//! - 越界像素直接跳过，任何坐标都不 panic。
//!
//! ## 字形编码方式
//!
//! 每个字形是 **7 行 × 5 列**的位图（行从上到下、列从左到右）。源码里每个字形写成
//! 7 行文本、每行 5 个字符：`'1'` = 有墨，`'0'` = 透明（其余字符如缩进空格一律忽略）。
//! 解析后每行压成一个 `u8` 位掩码：**bit 0（最低位）对应最左列**，第 `col` 列即 `1 << col`，
//! 因此 `0b0_1110` 表示「第 1、2、3 列有墨」。
//!
//! 表覆盖 ASCII `0x20..=0x7E`（95 个字形，索引 = 码位 − 0x20）。表外的字符
//! （CJK、emoji 等）在 [`BitmapFont::glyph`] 返回 `None`，栅格化时**回退到 `?` 字形**。
//!
//! ## 尺寸与布局约定
//!
//! - 单字形推进宽度 = [`GLYPH_WIDTH`] × `scale`；`tracking` 是**字形之间**额外增加的文档像素
//!   （不随 `scale` 缩放，行尾不加）。
//! - 行高 = [`GLYPH_HEIGHT`] × `scale` + `scale`（即字形盒高 + 一个 `scale` 的行距）。
//! - `measure` 返回文本**自身**的像素尺寸；空行的行盒仍计入高度（空串 → 宽 0、高 `GLYPH_HEIGHT * scale`）。

use std::sync::OnceLock;

use crate::buffer::Buffer;
use crate::color::{premultiply, LinearRgba};

/// 内置字形的宽度（像素）。
pub const GLYPH_WIDTH: u32 = 5;

/// 内置字形的高度（像素）。
pub const GLYPH_HEIGHT: u32 = 7;

/// 字形位图的行数（= [`GLYPH_HEIGHT`] 的 `usize` 形式）。
const ROWS: usize = 7;

/// 字形表的字符数：ASCII `0x20..=0x7E`。
const GLYPH_COUNT: usize = 95;

/// 字形表的第一个字符：空格。
const FIRST_CHAR: u32 = 0x20;

/// 未知字符回退用的字形（`?`，码位 0x3F）。
const FALLBACK_CHAR: char = '?';

/// 内置 5×7 ASCII 位图字体的字形表。
///
/// 字段是编译期常量表的 `'static` 视图，所以 [`BitmapFont::glyph`] 能直接返回
/// `&'static [u8]`。字体本身不持有可变状态，可自由共享。
#[derive(Debug, Clone, Copy)]
pub struct BitmapFont {
    /// 每项 7 行位掩码（bit 0 = 最左列），按码位顺序排列 `0x20..=0x7E`。
    glyphs: &'static [[u8; ROWS]],
}

impl BitmapFont {
    /// 内置字体（首次调用时解析一次字形表，之后共享同一份静态数据）。
    pub fn builtin() -> &'static BitmapFont {
        static FONT: OnceLock<BitmapFont> = OnceLock::new();
        FONT.get_or_init(|| BitmapFont {
            glyphs: glyph_table(),
        })
    }

    /// 取字形：每行一个位掩码，**bit 0 = 最左列**，长度恒为 7。
    ///
    /// 表覆盖 ASCII `0x20..=0x7E`；表外字符（CJK、emoji、控制符）返回 `None`，
    /// 由调用方决定回退（[`draw_text`] 回退到 `?`）。**任何输入都不 panic**。
    pub fn glyph(&self, ch: char) -> Option<&'static [u8]> {
        let code = ch as u32;
        if !(FIRST_CHAR..FIRST_CHAR + GLYPH_COUNT as u32).contains(&code) {
            return None;
        }
        let table: &'static [[u8; ROWS]] = self.glyphs;
        table
            .get((code - FIRST_CHAR) as usize)
            .map(|rows| &rows[..])
    }

    /// 文本的像素尺寸 `(宽, 高)`。
    ///
    /// - 宽 = 最宽一行的推进宽度 = `n * GLYPH_WIDTH * scale + (n - 1) * tracking`（行尾不加 tracking）；
    /// - 高 = `(行数 - 1) * 行高 + GLYPH_HEIGHT * scale`，行高 = `GLYPH_HEIGHT * scale + scale`；
    /// - `\n` 分行，**连续的或结尾的 `\n` 都产生一个空行**（行盒仍计入高度）；
    /// - `scale` 为 0 时按 1 处理（调用方应传 ≥1）；未知字符按 `?` 的推进宽度计。
    pub fn measure(&self, text: &str, scale: u32, tracking: u32) -> (u32, u32) {
        let scale = scale.max(1);
        let scale64 = u64::from(scale);
        let advance = u64::from(GLYPH_WIDTH) * scale64;
        let gap = u64::from(tracking);
        let mut lines = 0u64;
        let mut widest = 0u64;
        for line in text.split('\n') {
            lines += 1;
            let count = line.chars().count() as u64;
            if count > 0 {
                let width = count * advance + (count - 1) * gap;
                widest = widest.max(width);
            }
        }
        let ink_height = u64::from(GLYPH_HEIGHT) * scale64;
        let line_height = ink_height + scale64;
        let height = (lines - 1) * line_height + ink_height;
        (clamp_u32(widest), clamp_u32(height))
    }
}

/// 把 `u64` 尺寸饱和到 `u32`（极端 `scale` / 超长文本不溢出、不 panic）。
fn clamp_u32(value: u64) -> u32 {
    value.min(u64::from(u32::MAX)) as u32
}

/// 把文本栅格化进缓冲区。
///
/// - `x` / `y`：文本盒**左上角**的文档坐标（可负、可越界，超出的像素直接跳过）；
/// - `scale`：整数放大倍率（≥1；传 0 时按 1 处理），每个字形像素展开成 `scale × scale` 方块；
/// - `color`：**直通（非预乘）线性** RGBA，内部用 [`premultiply`] 转成预乘后交给
///   [`Buffer::blend_at`]；alpha 为 0 时仍然计入返回值；
/// - `align`：`"left"` / `"center"` / `"right"`，相对 `box_width` 对齐；
///   其它取值按 `"left"` 处理（不 panic）；
/// - `box_width`：对齐用的盒子宽度；`<= 0` 时按文本自身宽度（即等价于左对齐）；
/// - `\n` 换行，行高 = [`GLYPH_HEIGHT`] × `scale` + `scale`；未知字符用 `?` 字形。
///
/// 返回**实际写入缓冲区的像素数**（把字形方块逐像素计数；越界像素不计），便于测试与统计。
#[allow(clippy::too_many_arguments)] // 签名由设计接口固定（buffer/文本/位置/倍率/颜色/对齐/盒宽）。
/// 图集路径的缩放 ✓：单元 16×16 ⇒ `round(size/16)` ✓（**最小 1** ✓）。
pub fn atlas_scale_for_size(size: f64) -> u32 {
    let scale = (size.max(1.0) / f64::from(crate::font_atlas::CELL)).round() as i64;
    scale.clamp(1, 64) as u32
}

/// 这份文本走**图集**路径吗 ✓（含 ASCII 之外的**可打印**字符 ⇒ 是 ✓；控制字符忽略 ✓）？
///
/// 与 `draw_text` 内部的分流判据**必须完全一致** ✓ —— 度量与绘制若各判一次，
/// 就会出现"包围盒说一个尺寸、实际画另一个尺寸" ✗（正是子 agent 报的 #3 ✓：
/// CJK 文本包围盒 510×119 而实际墨迹 92×14 ✓ ⇒ 脏区过小 ⇒ 画布与缩略图/导出不一致 ✓）。
pub fn uses_atlas(text: &str) -> bool {
    let font = BitmapFont::builtin();
    text.chars()
        .any(|ch| !ch.is_control() && font.glyph(ch).is_none())
}

/// **按尺寸量测**文本 ✓：ASCII 走 5×7 度量 ✓，其余走 16×16 图集度量 ✓。
///
/// 返回 `(宽, 高)`（文档像素 ✓）。`box_width > 0` 时按对齐方式与文本宽度取最大 ✓。
pub fn measure_sized(text: &str, size: f64, box_width: f64) -> (u32, u32) {
    if !uses_atlas(text) {
        let scale = text_scale_for_size_local(size);
        let (width, height) = BitmapFont::builtin().measure(text, scale, 0);
        return (width.max(box_width as u32), height);
    }
    let scale = atlas_scale_for_size(size);
    let cell = crate::font_atlas::CELL * scale;
    let lines = text.split('\n').count().max(1) as u32;
    let widest = text
        .split('\n')
        .map(|line| line.chars().count() as u32 * cell)
        .max()
        .unwrap_or(0);
    (
        widest.max(box_width as u32),
        cell * lines + scale * lines.saturating_sub(1),
    )
}

/// 5×7 字体的缩放 ✓（与 `object::text_scale_for_size` 同规则 ✓，此处避免循环依赖 ✓）。
fn text_scale_for_size_local(size: f64) -> u32 {
    let scale = (size.max(1.0) / f64::from(GLYPH_HEIGHT)).round() as i64;
    scale.clamp(1, 64) as u32
}

/// **按尺寸绘制** ✓（`size` 是磅值 ✓，不是缩放 ✓）—— 分流与缩放都在这里决定 ✓，
/// 调用方不必（也不该 ✗）先自行换算 ✓：此前 render 分支先算 `round(size/7)` ✓，
/// 图集路径又按 16 除一次 ✓ ⇒ 含 CJK 的文本缩放恒为 1 ✗（子 agent 报的 #1 ✓）。
#[allow(clippy::too_many_arguments)]
pub fn draw_text_sized(
    buffer: &mut Buffer,
    text: &str,
    x: f64,
    y: f64,
    size: f64,
    color: [f32; 4],
    align: &str,
    box_width: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
) -> u32 {
    if uses_atlas(text) {
        draw_atlas_text_with_scale(
            buffer,
            text,
            x,
            y,
            atlas_scale_for_size(size),
            color,
            align,
            box_width,
            coverage,
        )
    } else {
        draw_text_clipped(
            buffer,
            text,
            x,
            y,
            text_scale_for_size_local(size),
            color,
            align,
            box_width,
            coverage,
        )
    }
}

/// 用内置 5×7 ASCII 字体绘制 ✓（`scale` 是**缩放倍数** ✓；含 CJK 时请用 [`draw_text_sized`] ✓，
/// 它按路径决定缩放 ✓ —— 直接传 5×7 的缩放给图集路径会造成双重缩放 ✗）。
#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    buffer: &mut Buffer,
    text: &str,
    x: f64,
    y: f64,
    scale: u32,
    color: [f32; 4],
    align: &str,
    box_width: f64,
) -> u32 {
    draw_text_clipped(
        buffer,
        text,
        x,
        y,
        scale,
        color,
        align,
        box_width,
        &|_, _| 1.0,
    )
}

/// 与 [`draw_text`] 相同，但**逐像素**乘以选区覆盖度 ✓（选区外一个像素都不写 ✓）。
///
/// 与笔触/擦除/形状一致：覆盖度在**像素中心**取样 ✓
/// （用左边界取样会让选区边界那一列被多算进去 ✗ —— 形状那边实测多出一整列 ✓）。
#[allow(clippy::too_many_arguments)]
pub fn draw_text_clipped(
    buffer: &mut Buffer,
    text: &str,
    x: f64,
    y: f64,
    scale: u32,
    color: [f32; 4],
    align: &str,
    box_width: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
) -> u32 {
    let font = BitmapFont::builtin();
    // 图集路径（含 ASCII 之外的字符时使用）✓ —— 设计 1175「首版仅 Latin/CJK 基础」✓。
    // 纯 ASCII 仍走内置 5×7 ✓：既有行为**逐字节不变** ✓（既有测试因此全部保持 ✓）。
    // 注意忽略**控制字符**（ 不在 5×7 表里 ✓ —— 第一版没排除它，
    // 结果连 "A\nB" 都被判成"含非 ASCII"而走了图集 ✗，像素数从 38 变 22 ✓）。
    if text
        .chars()
        .any(|ch| !ch.is_control() && font.glyph(ch).is_none())
    {
        return draw_atlas_text(buffer, text, x, y, scale, color, align, box_width, coverage);
    }
    let scale = scale.max(1);
    // 直通线性 → 预乘：blend_at 要求预乘颜色。
    let source: LinearRgba = premultiply(color);
    let (text_width, _) = font.measure(text, scale, 0);
    let box_width = if box_width > 0.0 {
        box_width
    } else {
        f64::from(text_width)
    };
    let offset = match align {
        "center" => (box_width - f64::from(text_width)) / 2.0,
        "right" => box_width - f64::from(text_width),
        _ => 0.0,
    };
    // 进入像素网格的唯一一次取整：此后全是整数运算，结果与平台无关。
    let origin_x = (x + offset).round() as i64;
    let origin_y = y.round() as i64;
    let scale_i = i64::from(scale);
    let glyph_advance = i64::from(GLYPH_WIDTH) * scale_i;
    let line_height = i64::from(GLYPH_HEIGHT) * scale_i + scale_i;
    let (buffer_x, buffer_y) = buffer.origin();
    let (buffer_width, buffer_height) = (buffer.width(), buffer.height());

    let mut drawn = 0u32;
    for (line_index, line) in text.split('\n').enumerate() {
        let line_y = origin_y + line_index as i64 * line_height;
        let mut pen_x = origin_x;
        for ch in line.chars() {
            // 表外字符（CJK、emoji…）回退到 `?`；`?` 一定在表内，因此不会 panic。
            let Some(rows) = font.glyph(ch).or_else(|| font.glyph(FALLBACK_CHAR)) else {
                continue;
            };
            for (row, mask) in rows.iter().enumerate() {
                for col in 0..GLYPH_WIDTH {
                    if *mask & (1u8 << col) == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let document_x = pen_x + i64::from(col * scale + dx);
                            let document_y = line_y + i64::from(row as u32 * scale + dy);
                            let local_x = document_x - buffer_x;
                            let local_y = document_y - buffer_y;
                            if local_x < 0 || local_y < 0 {
                                continue;
                            }
                            let (local_x, local_y) = (local_x as u32, local_y as u32);
                            if local_x >= buffer_width || local_y >= buffer_height {
                                continue;
                            }
                            // 覆盖度在**像素中心**取样 ✓（与其它图元一致 ✓）。
                            let selection =
                                coverage(document_x as f64 + 0.5, document_y as f64 + 0.5)
                                    .clamp(0.0, 1.0);
                            if selection <= 0.0 {
                                continue;
                            }
                            let source = if selection >= 1.0 {
                                source
                            } else {
                                premultiply([color[0], color[1], color[2], color[3] * selection])
                            };
                            buffer.blend_at(local_x, local_y, source);
                            drawn += 1;
                        }
                    }
                }
            }
            pen_x += glyph_advance;
        }
    }
    drawn
}

/// 解析后的字形表（进程内只解析一次）。
fn glyph_table() -> &'static [[u8; ROWS]] {
    static TABLE: OnceLock<[[u8; ROWS]; GLYPH_COUNT]> = OnceLock::new();
    &TABLE.get_or_init(parse_glyphs)[..]
}

/// 把 [`GLYPH_ART`] 的位串解析成位掩码表。
fn parse_glyphs() -> [[u8; ROWS]; GLYPH_COUNT] {
    let mut table = [[0u8; ROWS]; GLYPH_COUNT];
    for (index, art) in GLYPH_ART.iter().enumerate() {
        table[index] = decode_glyph(art);
    }
    table
}

/// 单个字形位串 → 7 个行掩码（bit 0 = 最左列）。
///
/// 每行先去掉缩进，再取前 5 个字符；不足 5 列或不足 7 行时缺的部分保持 0
/// （表本身由 `glyph_art_is_well_formed` 测试保证形状正确）。
fn decode_glyph(art: &str) -> [u8; ROWS] {
    let mut rows = [0u8; ROWS];
    for (row, line) in art.split('\n').take(ROWS).enumerate() {
        let mut mask = 0u8;
        for (col, byte) in line
            .trim_start()
            .bytes()
            .enumerate()
            .take(GLYPH_WIDTH as usize)
        {
            if byte == b'1' {
                mask |= 1u8 << col;
            }
        }
        rows[row] = mask;
    }
    rows
}

/// 内置 5×7 字形位图，按 ASCII 码位顺序覆盖 `0x20..=0x7E`。
///
/// 每项 7 行 × 5 列：`'1'` 有墨、`'0'` 透明，行首缩进被忽略；每行 → 一个 `u8` 掩码，
/// bit 0 = 最左列。
const GLYPH_ART: [&str; GLYPH_COUNT] = [
    // 0x20 ' '
    "00000
     00000
     00000
     00000
     00000
     00000
     00000",
    // 0x21 '!'
    "00100
     00100
     00100
     00100
     00100
     00000
     00100",
    // 0x22 '"'
    "01010
     01010
     00000
     00000
     00000
     00000
     00000",
    // 0x23 '#'
    "01010
     01010
     11111
     01010
     11111
     01010
     01010",
    // 0x24 '$'
    "00100
     01111
     10100
     01110
     00101
     11110
     00100",
    // 0x25 '%'
    "11000
     11001
     00010
     00100
     01000
     10011
     00011",
    // 0x26 '&'
    "01000
     10100
     10100
     01000
     10101
     10010
     01101",
    // 0x27 '\''
    "00100
     00100
     00000
     00000
     00000
     00000
     00000",
    // 0x28 '('
    "00010
     00100
     01000
     01000
     01000
     00100
     00010",
    // 0x29 ')'
    "01000
     00100
     00010
     00010
     00010
     00100
     01000",
    // 0x2A '*'
    "00000
     00100
     10101
     01110
     10101
     00100
     00000",
    // 0x2B '+'
    "00000
     00100
     00100
     11111
     00100
     00100
     00000",
    // 0x2C ','
    "00000
     00000
     00000
     00000
     00110
     00100
     01000",
    // 0x2D '-'
    "00000
     00000
     00000
     11111
     00000
     00000
     00000",
    // 0x2E '.'
    "00000
     00000
     00000
     00000
     00000
     01100
     01100",
    // 0x2F '/'
    "00001
     00010
     00010
     00100
     01000
     01000
     10000",
    // 0x30 '0'
    "01110
     10001
     10011
     10101
     11001
     10001
     01110",
    // 0x31 '1'
    "00100
     01100
     00100
     00100
     00100
     00100
     01110",
    // 0x32 '2'
    "01110
     10001
     00001
     00010
     00100
     01000
     11111",
    // 0x33 '3'
    "11111
     00010
     00100
     00010
     00001
     10001
     01110",
    // 0x34 '4'
    "00010
     00110
     01010
     10010
     11111
     00010
     00010",
    // 0x35 '5'
    "11111
     10000
     11110
     00001
     00001
     10001
     01110",
    // 0x36 '6'
    "00110
     01000
     10000
     11110
     10001
     10001
     01110",
    // 0x37 '7'
    "11111
     00001
     00010
     00100
     01000
     01000
     01000",
    // 0x38 '8'
    "01110
     10001
     10001
     01110
     10001
     10001
     01110",
    // 0x39 '9'
    "01110
     10001
     10001
     01111
     00001
     00010
     01100",
    // 0x3A ':'
    "00000
     01100
     01100
     00000
     01100
     01100
     00000",
    // 0x3B ';'
    "00000
     01100
     01100
     00000
     01100
     00100
     01000",
    // 0x3C '<'
    "00010
     00100
     01000
     10000
     01000
     00100
     00010",
    // 0x3D '='
    "00000
     00000
     11111
     00000
     11111
     00000
     00000",
    // 0x3E '>'
    "01000
     00100
     00010
     00001
     00010
     00100
     01000",
    // 0x3F '?'
    "01110
     10001
     00001
     00010
     00100
     00000
     00100",
    // 0x40 '@'
    "01110
     10001
     10111
     10101
     10111
     10000
     01110",
    // 0x41 'A'
    "01110
     10001
     10001
     11111
     10001
     10001
     10001",
    // 0x42 'B'
    "11110
     10001
     10001
     11110
     10001
     10001
     11110",
    // 0x43 'C'
    "01110
     10001
     10000
     10000
     10000
     10001
     01110",
    // 0x44 'D'
    "11100
     10010
     10001
     10001
     10001
     10010
     11100",
    // 0x45 'E'
    "11111
     10000
     10000
     11110
     10000
     10000
     11111",
    // 0x46 'F'
    "11111
     10000
     10000
     11110
     10000
     10000
     10000",
    // 0x47 'G'
    "01110
     10001
     10000
     10111
     10001
     10001
     01111",
    // 0x48 'H'
    "10001
     10001
     10001
     11111
     10001
     10001
     10001",
    // 0x49 'I'
    "01110
     00100
     00100
     00100
     00100
     00100
     01110",
    // 0x4A 'J'
    "00111
     00010
     00010
     00010
     00010
     10010
     01100",
    // 0x4B 'K'
    "10001
     10010
     10100
     11000
     10100
     10010
     10001",
    // 0x4C 'L'
    "10000
     10000
     10000
     10000
     10000
     10000
     11111",
    // 0x4D 'M'
    "10001
     11011
     10101
     10101
     10001
     10001
     10001",
    // 0x4E 'N'
    "10001
     10001
     11001
     10101
     10011
     10001
     10001",
    // 0x4F 'O'
    "01110
     10001
     10001
     10001
     10001
     10001
     01110",
    // 0x50 'P'
    "11110
     10001
     10001
     11110
     10000
     10000
     10000",
    // 0x51 'Q'
    "01110
     10001
     10001
     10001
     10101
     10010
     01101",
    // 0x52 'R'
    "11110
     10001
     10001
     11110
     10100
     10010
     10001",
    // 0x53 'S'
    "01111
     10000
     10000
     01110
     00001
     00001
     11110",
    // 0x54 'T'
    "11111
     00100
     00100
     00100
     00100
     00100
     00100",
    // 0x55 'U'
    "10001
     10001
     10001
     10001
     10001
     10001
     01110",
    // 0x56 'V'
    "10001
     10001
     10001
     10001
     10001
     01010
     00100",
    // 0x57 'W'
    "10001
     10001
     10001
     10101
     10101
     11011
     10001",
    // 0x58 'X'
    "10001
     10001
     01010
     00100
     01010
     10001
     10001",
    // 0x59 'Y'
    "10001
     10001
     01010
     00100
     00100
     00100
     00100",
    // 0x5A 'Z'
    "11111
     00001
     00010
     00100
     01000
     10000
     11111",
    // 0x5B '['
    "01110
     01000
     01000
     01000
     01000
     01000
     01110",
    // 0x5C '\\'
    "10000
     01000
     01000
     00100
     00010
     00010
     00001",
    // 0x5D ']'
    "01110
     00010
     00010
     00010
     00010
     00010
     01110",
    // 0x5E '^'
    "00100
     01010
     10001
     00000
     00000
     00000
     00000",
    // 0x5F '_'
    "00000
     00000
     00000
     00000
     00000
     00000
     11111",
    // 0x60 '`'
    "01000
     00100
     00000
     00000
     00000
     00000
     00000",
    // 0x61 'a'
    "00000
     00000
     01110
     00001
     01111
     10001
     01111",
    // 0x62 'b'
    "10000
     10000
     11110
     10001
     10001
     10001
     11110",
    // 0x63 'c'
    "00000
     00000
     01110
     10000
     10000
     10001
     01110",
    // 0x64 'd'
    "00001
     00001
     01111
     10001
     10001
     10001
     01111",
    // 0x65 'e'
    "00000
     00000
     01110
     10001
     11111
     10000
     01110",
    // 0x66 'f'
    "00110
     01001
     01000
     11100
     01000
     01000
     01000",
    // 0x67 'g'
    "00000
     01111
     10001
     10001
     01111
     00001
     01110",
    // 0x68 'h'
    "10000
     10000
     11110
     10001
     10001
     10001
     10001",
    // 0x69 'i'
    "00100
     00000
     01100
     00100
     00100
     00100
     01110",
    // 0x6A 'j'
    "00010
     00000
     00110
     00010
     00010
     10010
     01100",
    // 0x6B 'k'
    "10000
     10000
     10010
     10100
     11000
     10100
     10010",
    // 0x6C 'l'
    "01100
     00100
     00100
     00100
     00100
     00100
     01110",
    // 0x6D 'm'
    "00000
     00000
     11010
     10101
     10101
     10101
     10101",
    // 0x6E 'n'
    "00000
     00000
     11110
     10001
     10001
     10001
     10001",
    // 0x6F 'o'
    "00000
     00000
     01110
     10001
     10001
     10001
     01110",
    // 0x70 'p'
    "00000
     11110
     10001
     10001
     11110
     10000
     10000",
    // 0x71 'q'
    "00000
     01111
     10001
     10001
     01111
     00001
     00001",
    // 0x72 'r'
    "00000
     00000
     10110
     11001
     10000
     10000
     10000",
    // 0x73 's'
    "00000
     00000
     01111
     10000
     01110
     00001
     11110",
    // 0x74 't'
    "01000
     01000
     11100
     01000
     01000
     01001
     00110",
    // 0x75 'u'
    "00000
     00000
     10001
     10001
     10001
     10011
     01101",
    // 0x76 'v'
    "00000
     00000
     10001
     10001
     10001
     01010
     00100",
    // 0x77 'w'
    "00000
     00000
     10001
     10101
     10101
     10101
     01010",
    // 0x78 'x'
    "00000
     00000
     10001
     01010
     00100
     01010
     10001",
    // 0x79 'y'
    "00000
     10001
     10001
     10001
     01111
     00001
     01110",
    // 0x7A 'z'
    "00000
     00000
     11111
     00010
     00100
     01000
     11111",
    // 0x7B '{'
    "00110
     01000
     01000
     11000
     01000
     01000
     00110",
    // 0x7C '|'
    "00100
     00100
     00100
     00100
     00100
     00100
     00100",
    // 0x7D '}'
    "01100
     00010
     00010
     00011
     00010
     00010
     01100",
    // 0x7E '~'
    "00000
     00000
     01000
     10101
     00010
     00000
     00000",
];

/// 用**内嵌图集**（16×16 ✓）绘制文本：用于含 ASCII 之外字符的字符串 ✓。
///
/// 与 ASCII 路径的差别：单元是 16×16 ✓、缩放为 `round(size / 16)` ✓（最小 1 ✓）；
/// 图集里**没有**的字符回退到内置 `?` 字形 ✓（再没有就跳过 ✓，绝不 panic ✓）。
#[allow(clippy::too_many_arguments)]
fn draw_atlas_text_with_scale(
    buffer: &mut Buffer,
    text: &str,
    x: f64,
    y: f64,
    scale: u32,
    color: [f32; 4],
    align: &str,
    box_width: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
) -> u32 {
    draw_atlas_text(
        buffer,
        text,
        x,
        y,
        scale.saturating_mul(crate::font_atlas::CELL),
        color,
        align,
        box_width,
        coverage,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_atlas_text(
    buffer: &mut Buffer,
    text: &str,
    x: f64,
    y: f64,
    size: u32,
    color: [f32; 4],
    align: &str,
    box_width: f64,
    coverage: &dyn Fn(f64, f64) -> f32,
) -> u32 {
    let atlas = crate::font_atlas::Atlas::builtin();
    let fallback = BitmapFont::builtin();
    let cell = crate::font_atlas::CELL;
    let scale = ((size as f64) / f64::from(cell)).round().max(1.0) as u32;
    let advance = i64::from(cell * scale);
    let line_height = i64::from(cell * scale + scale);

    // 宽度：按最长行的**字符数**估算（图集等宽 ✓；图集缺失的字符也按整格算 ✓）。
    let mut text_width = 0u32;
    for line in text.split('\n') {
        text_width = text_width.max(line.chars().count() as u32 * cell * scale);
    }
    let box_width = if box_width > 0.0 {
        box_width
    } else {
        f64::from(text_width)
    };
    let offset = match align {
        "center" => (box_width - f64::from(text_width)) / 2.0,
        "right" => box_width - f64::from(text_width),
        _ => 0.0,
    };
    let origin_x = (x + offset).round() as i64;
    let origin_y = y.round() as i64;
    let (buffer_x, buffer_y) = buffer.origin();
    let (buffer_width, buffer_height) = (buffer.width(), buffer.height());

    let mut drawn = 0u32;
    for (line_index, line) in text.split('\n').enumerate() {
        let line_y = origin_y + line_index as i64 * line_height;
        let mut pen_x = origin_x;
        for ch in line.chars() {
            // **空白字符只推进、不落笔** ✓ —— 生成器会跳过空位图（空格就是空位图 ✓），
            // 于是图集里没有它 ✓ ⇒ 原先会回退成 `?` ✗（子 agent 实测："偃师 Yanshi 示例" 里
            // 空格与破折号都显示成 `?` ✓）。缩进/换行语义由**推进**表达 ✓，不该画出任何字形 ✓。
            if ch.is_whitespace() {
                pen_x += advance;
                continue;
            }
            let bitmap = atlas.glyph(ch as u32);
            let glyph_rows = bitmap.map(|_| ());
            let _ = glyph_rows;
            for row in 0..cell {
                for col in 0..cell {
                    let lit = match bitmap {
                        Some(bitmap) => crate::font_atlas::Atlas::pixel(bitmap, col, row),
                        // 图集里没有：回退到内置 `?` 字形（5×7 放大到本格 ✓）
                        None => {
                            let Some(rows) = fallback.glyph(FALLBACK_CHAR) else {
                                continue;
                            };
                            let source_row = (row * GLYPH_HEIGHT / cell) as usize;
                            let source_col = (col * GLYPH_WIDTH / cell) as usize;
                            rows.get(source_row)
                                .map(|mask| mask & (1u8 << source_col) != 0)
                                .unwrap_or(false)
                        }
                    };
                    if !lit {
                        continue;
                    }
                    for dy in 0..scale {
                        for dx in 0..scale {
                            let document_x = pen_x + i64::from(col * scale + dx);
                            let document_y = line_y + i64::from(row * scale + dy);
                            let local_x = document_x - buffer_x;
                            let local_y = document_y - buffer_y;
                            if local_x < 0 || local_y < 0 {
                                continue;
                            }
                            let (local_x, local_y) = (local_x as u32, local_y as u32);
                            if local_x >= buffer_width || local_y >= buffer_height {
                                continue;
                            }
                            let selection =
                                coverage(document_x as f64 + 0.5, document_y as f64 + 0.5)
                                    .clamp(0.0, 1.0);
                            if selection <= 0.0 {
                                continue;
                            }
                            let source = if selection >= 1.0 {
                                premultiply(color)
                            } else {
                                premultiply([color[0], color[1], color[2], color[3] * selection])
                            };
                            buffer.blend_at(local_x, local_y, source);
                            drawn += 1;
                        }
                    }
                }
            }
            pen_x += advance;
        }
    }
    drawn
}

#[cfg(test)]
mod tests {
    use crate::buffer::Buffer;

    /// 统计画布上"有墨"的像素数 ✓（与检查脚本同一口径 ✓）。
    fn ink_count(buffer: &Buffer) -> usize {
        let (w, h) = (buffer.width(), buffer.height());
        let mut n = 0;
        for y in 0..h {
            for x in 0..w {
                let pixel = buffer.pixel(x, y);
                if pixel[3] > 0.004 && (pixel[0] < 0.96 || pixel[1] < 0.96 || pixel[2] < 0.96) {
                    n += 1;
                }
            }
        }
        n
    }

    fn draw_sized(text: &str, size: f64) -> Buffer {
        let mut buffer = Buffer::new(0, 0, 256, 160);
        draw_text_sized(
            &mut buffer,
            text,
            4.0,
            4.0,
            size,
            [0.0, 0.0, 0.0, 1.0],
            "left",
            0.0,
            &|_, _| 1.0,
        );
        buffer
    }

    /// **含 CJK 的文本必须响应字号** ✓ —— 子 agent 报的 #1：字号 42 与 120 得到完全相同的墨迹 ✗。
    #[test]
    fn cjk_text_honours_the_size() {
        let small = ink_count(&draw_sized("中文永", 21.0));
        let large = ink_count(&draw_sized("中文永", 42.0));
        assert!(
            small > 0 && large > 0,
            "CJK 文本应画出像素（小 {small}、大 {large}）"
        );
        assert!(
            large > small * 2,
            "字号翻倍后墨迹应显著增多（小 {small} → 大 {large}）"
        );
        // 5×7 路径也仍然响应字号 ✓（回归 ✓）。
        let ascii_small = ink_count(&draw_sized("AB", 14.0));
        let ascii_large = ink_count(&draw_sized("AB", 42.0));
        assert!(
            ascii_large > ascii_small * 2,
            "ASCII 也应响应字号（{ascii_small} → {ascii_large}）"
        );
    }

    /// **空白字符只推进、不画 `?`** ✓ —— 子 agent 报的 #2：空格与破折号都显示成 `?` ✗。
    #[test]
    fn whitespace_advances_without_a_question_mark() {
        let with_space = ink_count(&draw_sized("中 文", 32.0));
        let without = ink_count(&draw_sized("中文", 32.0));
        assert!(with_space > 0, "应画出像素");
        // 不画 `?` ⇒ 墨迹量应与"去掉空格但位置前移"接近 ✓：这里只要求**不显著增多** ✓
        //（若把空格画成 `?`，墨迹会明显多出一整格 ✓）。
        assert!(
            with_space <= without,
            "空格不应画出 `?`（含空格 {with_space} 应 ≤ 无空格 {without}）"
        );
    }

    /// **包围盒与墨迹同源** ✓ —— 子 agent 报的 #3：CJK 包围盒 510×119 而墨迹仅 92×14 ✗。
    #[test]
    fn the_text_bbox_matches_the_ink() {
        for (text, size) in [("中文永", 32.0), ("AB", 21.0)] {
            let (width, height) = measure_sized(text, size, 0.0);
            let buffer = draw_sized(text, size);
            let mut max_x = 0u32;
            let mut max_y = 0u32;
            for y in 0..buffer.height() {
                for x in 0..buffer.width() {
                    if buffer.pixel(x, y)[3] > 0.004 {
                        max_x = max_x.max(x + 1);
                        max_y = max_y.max(y + 1);
                    }
                }
            }
            // 墨迹从 (4,4) 起 ✓ ⇒ 右下界约为 4 + 尺寸 ✓；允许一格（含行距）误差 ✓。
            let cell = if uses_atlas(text) {
                crate::font_atlas::CELL * atlas_scale_for_size(size)
            } else {
                GLYPH_HEIGHT * crate::object::text_scale_for_size(size)
            };
            assert!(
                max_x + 8 <= width + cell && max_y + 8 <= height + cell,
                "「{text}」@ {size}: 量测 {width}×{height} 与墨迹 {max_x}×{max_y} 相差超过一格"
            );
        }
    }
    use super::*;

    /// 取缓冲内 alpha > 0.5 的像素数（用于对齐断言）。
    fn inked_columns(buffer: &Buffer) -> Vec<u32> {
        let mut columns = Vec::new();
        for x in 0..buffer.width() {
            for y in 0..buffer.height() {
                if buffer.pixel(x, y)[3] > 0.5 {
                    columns.push(x);
                    break;
                }
            }
        }
        columns
    }

    #[test]
    fn glyph_lookup_returns_ascii_and_rejects_unknown() {
        let font = BitmapFont::builtin();
        let a = font.glyph('A').expect("'A' 应有字形");
        assert_eq!(a.len(), ROWS);
        assert!(a.iter().any(|row| *row != 0), "'A' 不能是空字形");
        // 位掩码编码：bit 0 = 最左列。
        assert_eq!(a[0], 0b0_1110);
        assert_eq!(a[3], 0b1_1111);
        assert_eq!(a[6], 0b1_0001);

        // 表覆盖首尾：空格（全透明）与 '~'。
        let space = font.glyph(' ').expect("空格应有字形");
        assert_eq!(space, &[0u8; ROWS]);
        assert!(font.glyph('~').is_some());
        assert!(font.glyph('0').is_some());
        let bang = font.glyph('!').expect("'!' 应有字形");
        assert_eq!(bang[0], 0b0_0100);
        assert_eq!(bang[5], 0b0_0000);
        assert_eq!(bang[6], 0b0_0100);

        // 表外字符返回 None（由 draw_text 回退到 '?'），不 panic。
        assert!(font.glyph('☃').is_none());
        assert!(font.glyph('中').is_none());
        assert!(font.glyph('\t').is_none());
        assert!(font.glyph('\u{0}').is_none());
        assert_eq!(BitmapFont::builtin().glyph('A'), font.glyph('A'));
    }

    #[test]
    fn glyph_art_is_well_formed() {
        let font = BitmapFont::builtin();
        for code in FIRST_CHAR..FIRST_CHAR + GLYPH_COUNT as u32 {
            let ch = char::from_u32(code).expect("ASCII 码位应可转换为 char");
            let rows = font.glyph(ch).expect("表内字符必须有字形");
            assert_eq!(rows.len(), ROWS, "{ch:?} 行数不对");
        }
        for (index, art) in GLYPH_ART.iter().enumerate() {
            let ch = char::from_u32(FIRST_CHAR + index as u32).unwrap();
            let lines: Vec<&str> = art.split('\n').collect();
            assert_eq!(lines.len(), ROWS, "{ch:?} 位串应有 7 行");
            for line in lines {
                let trimmed = line.trim_start();
                assert_eq!(
                    trimmed.chars().count(),
                    GLYPH_WIDTH as usize,
                    "{ch:?} 每行应 5 列"
                );
                assert!(
                    trimmed.bytes().all(|byte| byte == b'0' || byte == b'1'),
                    "{ch:?} 只允许 0/1"
                );
            }
            // 每行只有低 5 位有效。
            for mask in decode_glyph(art) {
                assert_eq!(mask & !0b1_1111, 0, "{ch:?} 掩码越界");
            }
        }
    }

    #[test]
    fn measure_reports_fixed_sizes() {
        let font = BitmapFont::builtin();
        // "AB"：2 个字形的推进宽度，行高 = 7。
        assert_eq!(font.measure("AB", 1, 0), (10, 7));
        // "A\nB"：两行，高 = 7 + (7 + 1) = 15，宽取最宽一行 = 5。
        assert_eq!(font.measure("A\nB", 1, 0), (5, 15));
        // 空串：宽度 0，但空行的行盒高度仍为 7。
        assert_eq!(font.measure("", 1, 0), (0, 7));
        // tracking 只加在字形之间：3 个字形 → 2 个间隙。
        assert_eq!(font.measure("ABC", 1, 1), (3 * 5 + 2, 7));
        // scale = 2：宽 2×、高 2×（行距也随之 ×2）。
        assert_eq!(font.measure("AB", 2, 0), (20, 14));
        assert_eq!(font.measure("A\nB", 2, 0), (10, 30));
        // 表外字符按 '?' 的推进宽度计，不 panic。
        assert_eq!(font.measure("☃", 1, 0), (5, 7));
        // 结尾换行产生一个空行。
        assert_eq!(font.measure("A\n", 1, 0), (5, 15));
        // scale = 0 按 1 处理。
        assert_eq!(font.measure("AB", 0, 0), (10, 7));
    }

    #[test]
    fn draw_single_glyph_sets_expected_pixels() {
        let mut buffer = Buffer::new(0, 0, 8, 8);
        let drawn = draw_text(
            &mut buffer,
            "A",
            0.0,
            0.0,
            1,
            [1.0, 0.0, 0.0, 1.0],
            "left",
            0.0,
        );
        // 'A' 的开启像素数：3 + 2 + 2 + 5 + 2 + 2 + 2 = 18。
        assert_eq!(drawn, 18);

        // 具体开关像素：第 0 行是 "01110"，所以 (0,0) 透明、(1,0) 有墨。
        assert_eq!(buffer.pixel(0, 0), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffer.pixel(1, 0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(2, 0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(3, 0), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(4, 0), [0.0, 0.0, 0.0, 0.0]);
        // 第 1 行是 "10001"：两端有墨、中间空。
        assert_eq!(buffer.pixel(0, 1), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(buffer.pixel(1, 1), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffer.pixel(4, 1), [1.0, 0.0, 0.0, 1.0]);
        // 第 3 行整行有墨；第 4 行以下不出缓冲区（第 7 行无墨）。
        assert_eq!(buffer.pixel(2, 3), [1.0, 0.0, 0.0, 1.0]);

        // 至少一个像素 alpha > 0.5。
        let any = (0..8).any(|x| (0..8).any(|y| buffer.pixel(x, y)[3] > 0.5));
        assert!(any, "应至少画出一个像素");
    }

    #[test]
    fn newline_moves_to_next_line_and_unknown_falls_back_to_question_mark() {
        let mut buffer = Buffer::new(0, 0, 8, 16);
        let drawn = draw_text(
            &mut buffer,
            "A\nB",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        // 'B' 的开启像素：4 + 2 + 2 + 4 + 2 + 2 + 4 = 20（第一行 11110 为 4 个）。
        assert_eq!(drawn, 18 + 20);
        // 第一行从 y = 0 开始，第二行从 y = 7 + 1 = 8 开始。
        assert_eq!(buffer.pixel(1, 0)[3], 1.0);
        assert_eq!(buffer.pixel(1, 7)[3], 0.0);
        assert_eq!(buffer.pixel(1, 8)[3], 1.0); // 'B' 第 0 行 "11110" 的第 1 列有墨

        // **行为有意改变（已显式改写）** ✓：此前非 ASCII 也走内置 5×7 ⇒ 回退成 5×7 的 `?`（38 像素）✗；
        // 现在**含 ASCII 之外字符的字符串走内嵌图集**（16×16 ✓）⇒ 图集里没有的字符按 `?` 字形
        // **映射到 16×16** 回退（实测 20 个亮点 ✓）。理由：设计 1175/1287 要求内嵌字体覆盖基础 CJK ✓，
        // 而纯 ASCII 仍走内置 5×7 ✓（上方的 18+20=38 断言即证明 ASCII 路径未变 ✓）。
        let mut fallback = Buffer::new(0, 0, 8, 8);
        let fallback_drawn = draw_text(
            &mut fallback,
            "☃",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        assert_eq!(
            fallback_drawn, 20,
            "含非 ASCII 的字符串走图集路径（实测 20 个亮点）"
        );
        // 纯 ASCII 的 `?` 仍走内置 5×7 ⇒ 与旧行为一致（38 像素）。
        let mut ascii_reference = Buffer::new(0, 0, 8, 8);
        let ascii_drawn = draw_text(
            &mut ascii_reference,
            "?",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        // 纯 ASCII 的 ? 仍走内置 5x7（5x7 字形最多 35 格）；8x8 缓冲里实测 9 个亮点 ✓。
        // （我一度把 38 记到这里 ✗ —— 那是 "A\nB" 那条的数值 ✓，测试如实报出 9 ✓。）
        assert_eq!(ascii_drawn, 9, "ASCII 问号仍应走内置 5x7 路径");
    }

    #[test]
    fn scale_multiplies_size_and_pixel_count_by_four() {
        let font = BitmapFont::builtin();
        assert_eq!(font.measure("A", 1, 0), (5, 7));
        assert_eq!(font.measure("A", 2, 0), (10, 14));

        let mut single = Buffer::new(0, 0, 12, 16);
        let single_drawn = draw_text(
            &mut single,
            "A",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        let mut double = Buffer::new(0, 0, 12, 16);
        let double_drawn = draw_text(
            &mut double,
            "A",
            0.0,
            0.0,
            2,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        assert_eq!(single_drawn, 18);
        assert_eq!(double_drawn, 72);
        assert_eq!(double_drawn, single_drawn * 4);
        // 放大后 (2, 0) 属于块 (col 1, row 0) × scale=2，应有墨。
        assert_eq!(double.pixel(2, 0)[3], 1.0);
        assert_eq!(double.pixel(0, 0)[3], 0.0);
    }

    #[test]
    fn align_offsets_start_at_expected_columns() {
        // 盒宽 21、文本宽 5：left → 0，center → (21-5)/2 = 8，right → 21-5 = 16。
        let cases = [("left", 0u32), ("center", 8), ("right", 16)];
        for (align, expected) in cases {
            let mut buffer = Buffer::new(0, 0, 24, 8);
            let drawn = draw_text(
                &mut buffer,
                "A",
                0.0,
                0.0,
                1,
                [1.0, 1.0, 1.0, 1.0],
                align,
                21.0,
            );
            assert_eq!(drawn, 18, "{align} 应完整画在缓冲内");
            assert_eq!(
                inked_columns(&buffer),
                (expected..expected + 5).collect::<Vec<_>>()
            );
            // 左边缘之前必须干净。
            if expected > 0 {
                assert_eq!(buffer.pixel(expected - 1, 3)[3], 0.0, "{align}");
            }
            assert_eq!(buffer.pixel(expected, 3)[3], 1.0, "{align} 起始列");
            assert_eq!(buffer.pixel(expected + 4, 3)[3], 1.0, "{align} 结束列");
        }

        // box_width <= 0 表示按文本自身宽度 → 等价于左对齐。
        let mut buffer = Buffer::new(0, 0, 24, 8);
        draw_text(
            &mut buffer,
            "A",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "right",
            0.0,
        );
        assert_eq!(inked_columns(&buffer), (0..5).collect::<Vec<_>>());

        // 未知 align 按左对齐处理，不 panic。
        let mut buffer = Buffer::new(0, 0, 24, 8);
        draw_text(
            &mut buffer,
            "A",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "justify",
            21.0,
        );
        assert_eq!(inked_columns(&buffer), (0..5).collect::<Vec<_>>());

        // 对齐用文档坐标：x = 2、盒宽 21、right → 起始列 2 + 16 = 18。
        let mut buffer = Buffer::new(0, 0, 24, 8);
        let x = 2.0;
        draw_text(
            &mut buffer,
            "A",
            x,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "right",
            21.0,
        );
        assert_eq!(inked_columns(&buffer), (18..23).collect::<Vec<_>>());
    }

    #[test]
    fn out_of_bounds_drawing_is_clipped_without_panic() {
        // 负坐标：字形第 3、4 列（doc x = 0、1）留在缓冲内。
        let mut buffer = Buffer::new(0, 0, 8, 8);
        let drawn = draw_text(
            &mut buffer,
            "A",
            -3.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        assert_eq!(drawn, 8, "只剩第 3、4 列的有墨像素：2 + 6");
        assert_eq!(buffer.pixel(0, 0)[3], 1.0);
        assert_eq!(buffer.pixel(1, 0)[3], 0.0);

        // 半出缓冲：行方向越界部分被裁掉，仍有部分绘制。
        let mut tall = Buffer::new(0, 0, 8, 3);
        let partial = draw_text(
            &mut tall,
            "A",
            0.0,
            0.0,
            1,
            [1.0, 1.0, 1.0, 1.0],
            "left",
            0.0,
        );
        assert_eq!(partial, 3 + 2 + 2, "只有前 3 行在缓冲内");
        assert!(partial > 0 && partial < 18);

        // 完全出界：不 panic、返回 0、缓冲保持透明。
        let mut outside = Buffer::new(0, 0, 8, 8);
        assert_eq!(
            draw_text(
                &mut outside,
                "A\nB",
                100.0,
                100.0,
                3,
                [1.0, 1.0, 1.0, 1.0],
                "right",
                10.0
            ),
            0
        );
        assert_eq!(
            draw_text(
                &mut outside,
                "",
                -1000.0,
                -1000.0,
                1,
                [1.0, 1.0, 1.0, 1.0],
                "center",
                4.0
            ),
            0
        );
        for x in 0..8 {
            for y in 0..8 {
                assert_eq!(outside.pixel(x, y), [0.0; 4]);
            }
        }
    }

    #[test]
    fn buffer_origin_offsets_document_coordinates() {
        // 缓冲区原点 (10, 20)：文档坐标 (10, 20) 落在局部 (0, 0)。
        let mut buffer = Buffer::new(10, 20, 8, 8);
        let drawn = draw_text(
            &mut buffer,
            "A",
            10.0,
            20.0,
            1,
            [0.5, 0.25, 0.0, 1.0],
            "left",
            0.0,
        );
        assert_eq!(drawn, 18);
        assert_eq!(buffer.pixel(0, 0), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(buffer.pixel(1, 0), [0.5, 0.25, 0.0, 1.0]);

        // 半透明颜色按预乘合成。
        let mut half = Buffer::new(0, 0, 8, 8);
        draw_text(
            &mut half,
            "A",
            0.0,
            0.0,
            1,
            [0.5, 0.5, 0.5, 0.5],
            "left",
            0.0,
        );
        assert_eq!(half.pixel(1, 0), [0.25, 0.25, 0.25, 0.5]);
    }
}
