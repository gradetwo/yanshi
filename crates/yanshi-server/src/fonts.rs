//! **★ 服务端真字体（字体方案 Phase 3）★**：枚举系统字体 ＋ 把文本栅格化成位图。
//!
//! **∴ 为什么在服务端做 ✗ 而不是改内核** ✓ ★★★**：
//!   ＋ **∴ 内核（`yanshi-render`）同时供**wasm 包**用**✗（`yanshi-wasm` ✓）
//!     ⇒ **∴ 若把 `fontdb`／`ab_glyph` 放进内核 ⇒ **浏览器包的体积**直接涨** ✓ ★★
//!   ＋ **∴ 所以 ✗**：**服务端**自己栅格化 ⇒ 产出**位图补丁** ⇒ `import_image` ✓
//!     ＋ **∴ 这与 Phase 1（**Web 侧用 Canvas 渲染后入库 ✓）**完全同构** ✓
//!       ⇒ **∴ 于是**：**内核与 wasm**一行不改**✗ 两条路各自用**手边现成的**字体能力** ✓ ★★★
//!
//! **∴ 库的选择（**按 AGENTS.md 的清单 ✓）✗**：
//!   ＋ **`fontdb` ✗**：**枚举**系统字体**（**Linux 用 fontconfig／macOS 用 Core Text／
//!     Windows 用 DirectWrite ✓ —— **正是方案点名的那三样 ✓，**已经打包好了** ✓）
//!     ＋ **∴ 它还是 `resvg` 的字体后端** ✓（**∴ 供应链上**被广泛依赖 ✓）★
//!   ＋ **`ab_glyph` ✗**：**把字形光栅化成覆盖度**（**纯 Rust ✓ MIT ✓ 成熟 ✓）★
//!     ＋ **∴ 只服务端用 ✗** ⇒ **∴ wasm 与内核**零依赖增长** ✓ ★★
//!   ＋ **∴ 确定性 ✗**：**方案已明确**放弃"字体渲染跨端一致"** ✓
//!     ＋ **∴ 所以 ✗**：**`builtin`（**位图图集 ✓）**仍是**默认**✗ 老文档像素零变更** ✓ ★★
//!
//! **∴ 边界（**如实写 ✓）✗**：
//!   ＋ **∴ 只做**单行、左/中/右对齐**✗ 不做**换行、连字、复杂文种整形（**Arabic 等 ✓）** ✓
//!     ＋ **∴ 要整形就得引 `rustybuzz`／`swash` ✗（**那是另一个量级的取舍 ✓）⇒ **∴ 记档 ✓
//!   ＋ **∴ 真字体那条路产出的是**位图对象**（`import_image` ✓）✗ **不是可编辑的文本对象** ✓
//!     ＋ **∴ 与 Web 侧（**Phase 1 ✓）**同一语义 ✓ ⇒ **要可编辑就用 `builtin`** ✓ ★★

use std::path::PathBuf;
use std::sync::OnceLock;

/// **∴ 字体库只建一次 ✗**（**`load_system_fonts` 是几十到几百毫秒量级 ✓）★
///
/// **∴ `preload_dirs` 只在**第一次**调用时生效** ✓（**∴ 服务进程里 assets 目录不会变 ✓）★
static DATABASE: OnceLock<fontdb::Database> = OnceLock::new();

/// **★ 取（或建）字体库 ✗ ★**（**第 254 轮 ✓）★
pub fn database(preload_dirs: &[PathBuf]) -> &'static fontdb::Database {
    DATABASE.get_or_init(|| {
        let mut db = fontdb::Database::new();
        // **系统字体** ✓（fontconfig／Core Text／DirectWrite ✓，由 `fontdb` 分派 ✓）。
        db.load_system_fonts();
        // **预置字体** ✓（Phase 2 那两支在 `assets/fonts/` ✓）。
        for dir in preload_dirs {
            if dir.is_dir() {
                db.load_fonts_dir(dir);
            }
        }
        db
    })
}

/// **★ 列出所有字体族 ✗ ★**（**去重 + 排序 ✗ 稳定输出 ⇒ 判据可复现 ✓）★
pub fn families(db: &fontdb::Database) -> Vec<String> {
    let mut names: Vec<String> = db
        .faces()
        .flat_map(|face| face.families.iter().map(|(name, _)| name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// **★ 把一行文本栅格化成 RGBA（**透明底 ✓）✗ ★**（**第 254 轮 ✓）★
///
/// **∴ 返回 `(宽度, 高度, RGBA)` ✗**：**与 `import_image` 要的原始位图**同一形状** ✓
///   （**∴ `mime_type: "image/x-yanshi-raw"` ✓**）★
/// **∴ 找不到那支字体 ⇒ `None`** ✓（**∴ 调用方**如实报错**✗ 不静默退回位图** ✓）★★
pub fn rasterize(
    db: &fontdb::Database,
    text: &str,
    family: &str,
    size: f32,
    color: [u8; 4],
    align: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    use ab_glyph::{Font, FontRef, Glyph, PxScale, ScaleFont};
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        ..Default::default()
    };
    let id = db.query(&query)?;
    // **∴ 字体数据要在**库的借用期**内用 ✗ ⇒ **∴ 拷一份**（**∴ 几 MB 一次性 ✓）★
    let (bytes, index) = db.with_face_data(id, |data, index| (data.to_vec(), index))?;
    let font = FontRef::try_from_slice_and_index(&bytes, index).ok()?;
    let scale = PxScale::from(size);
    // **∴ 用**已缩放**的那个视图 ✗**（`as_scaled` ✓）：**它给的是**像素单位**的
    // `h_advance`／`kern`／`ascent`** ✓（**∴ 未缩放那套还要自己乘 `size/units_per_em` ✓
    //  —— **∴ 那正是**容易算错的地方 ✓）★★
    let scaled = font.as_scaled(scale);
    // **∴ 排字（**这点粘合自己写 ✓）✗ 光栅化仍交给库** ✓（**方案："不自己实现字体光栅化" ✓）★
    //   ＋ **∴ 只做**单行 + 字距（kern）**✗ 不做换行／连字／复杂文种整形** ✓（**已记档 ✓）★
    let ascent = scaled.ascent();
    let mut pen_x = 2.0_f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    let mut placed: Vec<Glyph> = Vec::new();
    for ch in text.chars() {
        let glyph_id = scaled.glyph_id(ch);
        if let Some(previous) = prev {
            pen_x += scaled.kern(previous, glyph_id);
        }
        placed.push(Glyph {
            id: glyph_id,
            scale,
            // **∴ 基线放在 `ascent` 处 ✗**（**∴ 于是**字形整体落在画布内 ✓）★
            position: ab_glyph::point(pen_x, ascent + 2.0),
        });
        pen_x += scaled.h_advance(glyph_id);
        prev = Some(glyph_id);
    }
    // **∴ 先量包围盒 ✗ 再决定画布多大** ✓（**∴ 与 Phase 1 的 `measureText` 同一个道理 ✓）★
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for glyph in &placed {
        if let Some(outlined) = scaled.outline_glyph(glyph.clone()) {
            let bounds = outlined.px_bounds();
            min_x = min_x.min(bounds.min.x);
            min_y = min_y.min(bounds.min.y);
            max_x = max_x.max(bounds.max.x);
            max_y = max_y.max(bounds.max.y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    // **∴ 边距 2px ✗**（**∴ 抗锯齿的覆盖度不会被切掉 ✓）★
    let ink_w = (max_x - min_x).ceil().max(1.0) as u32;
    let ink_h = (max_y - min_y).ceil().max(1.0) as u32;
    let width = ink_w + 4;
    let height = ink_h + 4;
    let mut coverage = vec![0f32; (width * height) as usize];
    for glyph in placed {
        if let Some(outlined) = scaled.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|x, y, c| {
                let px = bounds.min.x + x as f32 - min_x + 2.0;
                let py = bounds.min.y + y as f32 - min_y + 2.0;
                if px < 0.0 || py < 0.0 {
                    return;
                }
                let (px, py) = (px as u32, py as u32);
                if px >= width || py >= height {
                    return;
                }
                let at = (py * width + px) as usize;
                // **∴ 字形可能重叠 ✗ 取较大的覆盖度** ✓（**`max` 比累加更稳 ✓）★
                if c > coverage[at] {
                    coverage[at] = c;
                }
            });
        }
    }
    // **∴ 对齐 ✗**：**按**实际墨迹宽度**在一张更宽的画布上偏移** ✓（**left／center／right ✓）★
    let (canvas_w, offset_x) = match align {
        "center" => (ink_w.saturating_mul(2).max(width), ink_w / 2),
        "right" => (width.saturating_add(ink_w), ink_w),
        _ => (width, 0),
    };
    let mut rgba = vec![0u8; (canvas_w * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let c = coverage[(y * width + x) as usize];
            if c <= 0.0 {
                continue;
            }
            let dx = x + offset_x;
            if dx >= canvas_w {
                continue;
            }
            let at = ((y * canvas_w + dx) * 4) as usize;
            // **∴ 直通 alpha（**非预乘 ✓）✗ 与画布 `getImageData` 同一口径** ✓（**Phase 1 ✓）★
            rgba[at] = color[0];
            rgba[at + 1] = color[1];
            rgba[at + 2] = color[2];
            rgba[at + 3] = (c.clamp(0.0, 1.0) * f32::from(color[3])).round() as u8;
        }
    }
    Some((canvas_w, height, rgba))
}
