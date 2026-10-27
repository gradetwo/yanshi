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
    let layout = layout_text(db, text, family, size, align)?;
    Some(rasterize_layout(&layout, color))
}

/// **★ 一行文本的排字布局（**`rasterize`／子集化／溯源锚点共用 ✓）★**
///
/// **∴ 为什么抽出来** ✓：**栅格化、子集化、基线锚点**三处都要同一套中间量
/// （ascent／墨迹框／advance／画布尺寸 ✓）⇒ **各算一次**会漂移 ✗ ⇒ **所以**布局只算一次 ✓。
/// **∴ 字段全是 `rasterize` 原来算过的量** ✓（**多出来的三项** `descent`／`units_per_em`／
/// `line_height_units` 是锚点与有效字号要的 ✓；**排字与包围盒的公式一行没动** ✓）。
pub struct TextLayout {
    /// 字体文件字节（`fontdb` 给的那份拷贝 ✓；子集化直接用它 ✓）。
    pub bytes: Vec<u8>,
    /// `fontdb` 的面索引（TTC 里第几支 ✓；单文件字体为 0 ✓）。
    pub face_index: u32,
    /// 请求字号（磅值 ✓）。
    pub size: f32,
    /// 已缩放 ascent（像素 ✓）。
    pub ascent: f32,
    /// 已缩放 descent（像素 ✓）。
    pub descent: f32,
    /// `units_per_em`（`hhea` 表 ✓）。
    pub units_per_em: f32,
    /// 行高（`hhea.ascent − descent`，字体单位 ✓；`ab_glyph` 的缩放分母 ✓）。
    pub line_height_units: f32,
    /// 总 advance（像素 ✓；`pen_x − 2.0` ✓）。
    pub advance_width: f32,
    /// 墨迹框左缘（画布坐标 ✓）。
    pub min_x: f32,
    /// 墨迹框上缘（画布坐标 ✓）。
    pub min_y: f32,
    /// 墨迹宽（像素 ✓）。
    pub ink_w: u32,
    /// 墨迹高（像素 ✓）。
    pub ink_h: u32,
    /// 位图宽（`ink_w + 4` ✓）。
    pub width: u32,
    /// 位图高（`ink_h + 4` ✓）。
    pub height: u32,
    /// 对齐后的画布宽（`center` 画布加宽、`right` 右移 ✓）。
    pub canvas_w: u32,
    /// 对齐偏移（画布坐标 ✓；`left` 为 0 ✓）。
    pub offset_x: u32,
    /// 已排字形（带 scale 与位置 ✓；栅格化与子集化共用 ✓）。
    pub placed: Vec<ab_glyph::Glyph>,
    /// 排字用的缩放（`PxScale::from(size)` ✓）。
    pub scale: ab_glyph::PxScale,
}

/// **★ 排一行文本，返回布局（**不画像素 ✓）★**
///
/// **∴ 与 `rasterize` 同源** ✓：**排字、量包围盒、对齐偏移**就是 `rasterize` 原来的前半段 ✓
/// ⇒ **锚点与位图**不可能各说各话 ✓。
/// **∴ 找不到那支字体 ⇒ `None`** ✓（**∴ 调用方**如实报错 ✓）。
pub fn layout_text(
    db: &fontdb::Database,
    text: &str,
    family: &str,
    size: f32,
    align: &str,
) -> Option<TextLayout> {
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
    let descent = scaled.descent();
    // **∴ `ab_glyph` 按行高缩放，不按 `units_per_em`** ✓（2026-10-27 实测：Noto CJK
    // Bold 60pt 实际 em 只有 41.44px、Fredoka 80pt 实际 66.116px ✓）⇒ **SVG 的
    // `font-size` 必须用换算后的有效尺寸（`size × upem ／ 行高`，见 `effective_font_size` ✓），
    // 否则 Chromium 的字比引擎大 45%（CJK ✓）⇒ **这两个数随布局一起返回** ✓。
    // `ab_glyph::Font::units_per_em` 是 `Option<f32>`（无 head 表的字体会是
    // `None`）；取不到时按 1000（PostScript 常规值）回退——只影响有效字号换算，
    // 锚点与位图都不用它 ✓。
    let units_per_em = font.units_per_em().unwrap_or(1000.0);
    let line_height_units = font.ascent_unscaled() - font.descent_unscaled();
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
    let advance_width = pen_x - 2.0;
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
    // **∴ 对齐 ✗**：**按**实际墨迹宽度**在一张更宽的画布上偏移** ✓（**left／center／right ✓）★
    let (canvas_w, offset_x) = match align {
        "center" => (ink_w.saturating_mul(2).max(width), ink_w / 2),
        "right" => (width.saturating_add(ink_w), ink_w),
        _ => (width, 0),
    };
    Some(TextLayout {
        bytes,
        face_index: index,
        size,
        ascent,
        descent,
        units_per_em,
        line_height_units,
        advance_width,
        min_x,
        min_y,
        ink_w,
        ink_h,
        width,
        height,
        canvas_w,
        offset_x,
        placed,
        scale,
    })
}

/// **★ 按布局画 RGBA（**`rasterize` 的画图一半 ✓）★**
///
/// **∴ 公式与原来逐行一致** ✓（**覆盖度取 max、直通 alpha、按 `offset_x` 平移 ✓**）：
/// 布局里的字节解析过一次 ⇒ 这里再解析一定成功（`expect` 有据 ✓）。
pub fn rasterize_layout(layout: &TextLayout, color: [u8; 4]) -> (u32, u32, Vec<u8>) {
    use ab_glyph::{Font, FontRef, ScaleFont};
    let font = FontRef::try_from_slice_and_index(&layout.bytes, layout.face_index)
        .expect("布局里的字体字节生成布局时已解析过 ⇒ 这里一定能解析");
    let scaled = font.as_scaled(layout.scale);
    let (width, height) = (layout.width, layout.height);
    let (min_x, min_y) = (layout.min_x, layout.min_y);
    let mut coverage = vec![0f32; (width * height) as usize];
    for glyph in &layout.placed {
        if let Some(outlined) = scaled.outline_glyph(glyph.clone()) {
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
    let (canvas_w, offset_x) = (layout.canvas_w, layout.offset_x);
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
    (canvas_w, height, rgba)
}

/// **★ 子集化上限（**设计 §2.1 ✓）**：**超过这个尺寸的子集不嵌入** ⇒ 回退 raster ✓
/// （CJK 子集实测 ~38KB，离上限远 ✓；整文件嵌入是明确否决的退化 ✓）。
pub const SUBSET_MAX_BYTES: usize = 512 * 1024;

/// **★ OFL 授权 allowlist（**设计 §2.1 ✓）**：**只有这些族的子集可以嵌入 SVG ✓**
/// （其余字体只栅格化、不嵌入 ⇒ 回退 raster、原因 `font_license_unknown` ✓）。
/// **∴ 按族名匹配** ✓：**设计点名的就是这些字符串**（`draw_text{font:…}` 传的
/// 就是族名 ✓）；**系统里同名不同许可的字体是理论风险**（`Fredoka` 本身就是
/// OFL ✓；真撞上时回退方向是安全的——不嵌总比错嵌强 ✓）。
pub fn is_subset_allowlisted(family: &str) -> bool {
    const ALLOWLIST: &[&str] = &[
        "Inter",
        "JetBrains Mono",
        "Fredoka",
        "Fredoka Bold",
        "Baloo 2",
        "Cherry Bomb One",
        "Titan One",
        "Noto Sans CJK SC",
        "Noto Sans CJK SC Bold",
    ];
    ALLOWLIST
        .iter()
        .any(|name| name.eq_ignore_ascii_case(family))
}

/// **★ 给布局做 TTF 子集（**`allsorts` + `Minimal` profile ✓）★**
///
/// **∴ 为什么是 allsorts + Minimal** ✓（2026-10-27 评估，见实现报告）：
///   ＋ **收益**：纯 Rust、确定性（同一文本两次子集逐字节相同 ✓）、`Minimal` 含 OS/2 ⇒ Chromium
///     OTS 通过 ✓；CFF 字体也保留 cmap（`@font-face` 必须 ✓）。
///   ＋ **代价**：GPOS/GSUB 被丢掉 ⇒ 字距（kern）丢失（"Congrats!" 80pt 上 `r→a` −1.6px，
///     CJK 不受影响 ✓）；产出是 TTF 子集不是 WOFF2（体积仍远小于上限 ✓）。
/// **∴ 失败 ⇒ `None`** ✓（**∴ 调用方**回退 raster 并记原因 ✓，不静默 ✓）。
/// **∴ 注意**：**`Pdf` profile 缺 OS/2 ⇒ Chromium 拒绝** ✗（实测 ✓）⇒ **必须用 `Minimal`** ✓。
pub fn subset_layout(layout: &TextLayout) -> Option<Vec<u8>> {
    use allsorts::binary::read::ReadScope;
    use allsorts::font_data::FontData;
    use allsorts::subset::{subset, CmapTarget, SubsetProfile};
    use std::collections::BTreeSet;
    let mut glyph_ids: BTreeSet<u16> = BTreeSet::new();
    glyph_ids.insert(0); // `.notdef` ✓
    for glyph in &layout.placed {
        glyph_ids.insert(glyph.id.0);
    }
    let glyph_ids: Vec<u16> = glyph_ids.into_iter().collect();
    let scope = ReadScope::new(&layout.bytes);
    let font_file = scope.read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(layout.face_index as usize).ok()?;
    subset(
        &provider,
        &glyph_ids,
        &SubsetProfile::Minimal,
        CmapTarget::Unicode,
    )
    .ok()
}

/// **★ SVG 用的有效字号（**设计 §4.3 校准 ✓）★**：**`ab_glyph` 按行高缩放** ⇒
/// `size` 是行高对应的磅值，不是 em ⇒ **SVG `font-size` 必须换算**
/// （`size × upem ／ 行高`）⇒ 否则 Chromium 的字比引擎大 45%（CJK ✓）。
pub fn effective_font_size(layout: &TextLayout) -> f32 {
    layout.size * layout.units_per_em / layout.line_height_units
}

/// **★ 文本锚点（**设计 §2.3 + §4.3 校准 ✓）★**
///
/// **∴ 坐标系** ✓：**`position` 是 em-box 左上角**（位图 `region` 的原点 ✓）；
/// **基线锚点** = `position + 画布内基线` ✓（`origin_x = 4.0 − min_x + offset_x`、
/// `baseline_y = ascent − min_y + 4.0` ✓，`4.0` 是两侧各 2px 的边距 ✓）。
/// **∴ 对齐** ✓：**`start` = 首字形原点**；**`middle` = 原点 + `advance/2`**；
/// **`end` = 原点 + `advance`** ✓（2026-10-27 校准：`middle` 用画布中心会左偏 2px，
/// 用 advance 中心才与引擎逐像素一致 ✓）。
/// **∴ 返回 `(锚点 x, 基线 y, 锚名)`** ✓（**锚名进溯源、序列化时写 `text-anchor` ✓**）。
pub fn text_anchor(
    layout: &TextLayout,
    position: (f64, f64),
    align: &str,
) -> (f64, f64, &'static str) {
    let origin_x = position.0 + 4.0 - f64::from(layout.min_x) + f64::from(layout.offset_x);
    let baseline_y = position.1 + f64::from(layout.ascent) - f64::from(layout.min_y) + 4.0;
    match align {
        "center" => (
            origin_x + f64::from(layout.advance_width) / 2.0,
            baseline_y,
            "middle",
        ),
        "right" => (
            origin_x + f64::from(layout.advance_width),
            baseline_y,
            "end",
        ),
        _ => (origin_x, baseline_y, "start"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 仓库预置字体目录（`assets/fonts/`，`Inter` 与 `JetBrains Mono` ✓）。
    fn repo_fonts_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("assets")
            .join("fonts")
    }

    fn test_db() -> fontdb::Database {
        let mut db = fontdb::Database::new();
        db.load_fonts_dir(repo_fonts_dir());
        db
    }

    #[test]
    fn subset_is_deterministic_and_smaller_than_source() {
        let db = test_db();
        let layout =
            layout_text(&db, "Hello", "Inter", 40.0, "left").expect("仓库预置字体 Inter 应能排字");
        let first = subset_layout(&layout).expect("Inter 子集应能生成");
        let second = subset_layout(&layout).expect("同一布局第二次子集应能生成");
        assert_eq!(first, second, "同一文本两次子集必须逐字节相同（确定性 ✓）");
        assert!(!first.is_empty(), "子集不能为空");
        assert!(
            first.len() < layout.bytes.len(),
            "子集（{} B）应小于源字体（{} B）",
            first.len(),
            layout.bytes.len()
        );
        // **变异**：把 `Minimal` 换成 `Pdf` ⇒ 子集仍确定但缺 OS/2 ⇒ Chromium 拒绝；
        // 本单测不验 OTS（那是接受探针的事 ✓），只验确定性与尺寸 ✓。
    }

    #[test]
    fn rasterize_matches_layout_split() {
        // **重构守卫** ✓：`rasterize` 拆成 `layout_text` + `rasterize_layout` 之后，
        // 两条路的像素必须逐字节相同 ⇒ 拆分没改任何数学 ✓。
        // **变异**：改 `rasterize_layout` 的覆盖度循环（如 `max` 改 `+`）⇒ 本单测红 ✓。
        let db = test_db();
        let color = [200u8, 60, 40, 255];
        let via_rasterize =
            rasterize(&db, "Hello", "Inter", 40.0, color, "center").expect("Inter 应能栅格化");
        let layout = layout_text(&db, "Hello", "Inter", 40.0, "center").expect("Inter 应能排字");
        let via_layout = rasterize_layout(&layout, color);
        assert_eq!(via_rasterize, via_layout, "拆分前后像素必须一致");
    }

    #[test]
    fn anchor_formula_matches_layout_fields() {
        // **锚点公式**（长手写出，不调 `text_anchor` 本身 ✓）：`origin_x = 4 − min_x + offset_x`、
        // `baseline_y = ascent − min_y + 4`；`middle` 再加 `advance/2` ✓。
        // **变异**：把 `text_anchor` 的 `4.0` 改成 `2.0` ⇒ 本单测红 ✓。
        let db = test_db();
        let layout = layout_text(&db, "Hello", "Inter", 40.0, "center").expect("Inter 应能排字");
        let (x, y, anchor) = text_anchor(&layout, (20.0, 100.0), "center");
        let expect_x = 20.0 + 4.0 - f64::from(layout.min_x)
            + f64::from(layout.offset_x)
            + f64::from(layout.advance_width) / 2.0;
        let expect_y = 100.0 + f64::from(layout.ascent) - f64::from(layout.min_y) + 4.0;
        assert_eq!(anchor, "middle");
        assert!(
            (x - expect_x).abs() < 1e-6,
            "middle 锚点 x 应为 {expect_x}，实际 {x}"
        );
        assert!(
            (y - expect_y).abs() < 1e-6,
            "基线 y 应为 {expect_y}，实际 {y}"
        );
        let (sx, _, sa) = text_anchor(&layout, (20.0, 100.0), "left");
        assert_eq!(sa, "start");
        assert!((sx - (expect_x - f64::from(layout.advance_width) / 2.0)).abs() < 1e-6);
        let (ex, _, ea) = text_anchor(&layout, (20.0, 100.0), "right");
        assert_eq!(ea, "end");
        assert!((ex - (expect_x + f64::from(layout.advance_width) / 2.0)).abs() < 1e-6);
    }

    #[test]
    fn effective_font_size_is_smaller_than_requested_size() {
        // **行高 > upem 的字体** ⇒ 有效字号 < 请求字号 ✓。
        // **变异**：把 `effective_font_size` 的除法改成乘法 ⇒ 本单测红 ✓。
        let db = test_db();
        let layout = layout_text(&db, "Hello", "Inter", 40.0, "left").expect("Inter 应能排字");
        let effective = effective_font_size(&layout);
        assert!(effective > 0.0, "有效字号必须为正");
        assert!(
            effective < layout.size,
            "有效字号（{effective}）应小于请求字号（{}，行高 > upem ⇒ ab_glyph 按行高缩放 ✓）",
            layout.size
        );
        assert!(
            (effective - layout.size * layout.units_per_em / layout.line_height_units).abs() < 1e-6
        );
    }

    #[test]
    fn allowlist_covers_design_families() {
        assert!(is_subset_allowlisted("Inter"));
        assert!(is_subset_allowlisted("Fredoka"));
        assert!(is_subset_allowlisted("Noto Sans CJK SC Bold"));
        assert!(is_subset_allowlisted("fredoka"), "族名匹配不区分大小写");
        assert!(!is_subset_allowlisted("Arial"));
        assert!(!is_subset_allowlisted(""));
        // **变异**：把 allowlist 里的 "Fredoka" 删掉 ⇒ 本单测红 ✓。
    }
}
