//! 内嵌的 **1-bit 位图字体图集**（Latin + 基础 CJK）✓。
//!
//! 设计 1175 / 1287 行要求「内嵌开源字体子集，禁用平台相关 hinting，首版仅 Latin/CJK 基础」，
//! 并把「字体版权 / 跨端一致性」列为风险项 ✓。做法与理由见 `assets/fonts/README.md`：
//! 复杂度留在**生成期**（`scripts/build-bitmap-font.py` ✓，开发工具 ✓），
//! 运行期零依赖、零文件 IO（`include_bytes!` ✓）、零平台差异 ✓ —— wasm 同样可用 ✓。
//!
//! 覆盖：ASCII + **GB2312 一级与二级字库**（3755 + 3008 个汉字 ✓）+ GB2312 符号区 ✓。
//!
//! 字体来源：**Noto Sans CJK SC Regular**（`noto-fonts-cjk 20240730-1` ✓），
//! **SIL OFL 1.1** ✓（授权全文随仓库：`assets/fonts/LICENSE-OFL-NotoSansCJK.txt` ✓）。
//!
//! 文件格式（小端）：
//! ```text
//! magic "YFNT" | cell_w u8 | cell_h u8 | count u32 | count × (codepoint u32, bitmap[32])
//! ```
//! 码位**升序**存放 ⇒ 二分查找 ✓。

/// 图集二进制（编译期嵌入 ⇒ 无运行时文件读取 ✓，wasm 亦可用 ✓）。
static ATLAS: &[u8] = include_bytes!("../../../assets/fonts/yanshi-bitmap-16.bin");

/// 单元尺寸（边长，像素）。
pub const CELL: u32 = 16;

/// 每字形位图字节数（16×16 ÷ 8）。
pub const GLYPH_BYTES: usize = (CELL * CELL / 8) as usize;

const HEADER_BYTES: usize = 4 + 1 + 1 + 4;

/// 解析后的图集视图（零拷贝 ✓）。
#[derive(Debug, Clone, Copy)]
pub struct Atlas {
    data: &'static [u8],
    count: u32,
}

impl Atlas {
    /// 内嵌图集（首次调用解析头部 ✓；头部非法时退化为**空图集**而不是 panic ✓）。
    pub fn builtin() -> Atlas {
        let data = ATLAS;
        if data.len() < HEADER_BYTES || &data[..4] != b"YFNT" {
            return Atlas {
                data: &[],
                count: 0,
            };
        }
        let count = u32::from_le_bytes([data[6], data[7], data[8], data[9]]);
        // 头部声明的数量若超出文件长度，按实际可容纳的数量截断（防止越界 ✓）。
        let available = ((data.len() - HEADER_BYTES) / (4 + GLYPH_BYTES)) as u32;
        Atlas {
            data,
            count: count.min(available),
        }
    }

    /// 字形数量。
    pub fn len(&self) -> u32 {
        self.count
    }

    /// 是否为空图集。
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// 按码位取字形位图（**二分查找** ✓，码位升序 ✓）。
    pub fn glyph(&self, code: u32) -> Option<&'static [u8]> {
        let entry = 4 + GLYPH_BYTES;
        let (mut low, mut high) = (0u32, self.count);
        while low < high {
            let mid = (low + high) / 2;
            let offset = HEADER_BYTES + mid as usize * entry;
            let stored = u32::from_le_bytes([
                self.data[offset],
                self.data[offset + 1],
                self.data[offset + 2],
                self.data[offset + 3],
            ]);
            match stored.cmp(&code) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => {
                    let start = offset + 4;
                    return Some(&self.data[start..start + GLYPH_BYTES]);
                }
            }
        }
        None
    }

    /// 某行（0..CELL）最左边开始的 16 位掩码（高位在左 ✓，与生成脚本一致 ✓）。
    pub fn row(bitmap: &[u8], y: u32) -> u16 {
        let offset = (y as usize) * 2;
        u16::from_be_bytes([bitmap[offset], bitmap[offset + 1]])
    }

    /// 某个像素是否被点亮 ✓。
    pub fn pixel(bitmap: &[u8], x: u32, y: u32) -> bool {
        if x >= CELL || y >= CELL {
            return false;
        }
        Self::row(bitmap, y) & (1 << (CELL - 1 - x)) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_atlas_parses_and_covers_latin_and_cjk() {
        let atlas = Atlas::builtin();
        assert!(!atlas.is_empty(), "内嵌图集应可用（头部或路径有问题？）");
        assert_eq!(
            atlas.len(),
            7116,
            "字形数量应与生成结果一致（GB2312 一级 + 二级）"
        );
        for (ch, code) in [
            ('A', 0x41u32),
            ('中', 0x4E2D),
            ('永', 0x6C38),
            ('。', 0x3002),
        ] {
            assert!(
                atlas.glyph(code).is_some(),
                "图集应包含「{ch}」(U+{code:04X})"
            );
        }
        // 图集之外（emoji）应为 None ⇒ 调用方回退到内置 5×7 或 `?` ✓。
        assert!(atlas.glyph(0x1F600).is_none());
        assert!(atlas.glyph(0x0).is_none());
    }

    #[test]
    fn a_known_glyph_has_the_expected_pixels() {
        let atlas = Atlas::builtin();
        let zhong = atlas.glyph(0x4E2D).expect("「中」应在图集里");
        // 生成脚本产出的「中」（**写死**，可被突变检验抓住 ✓）：
        //   第 0 行留白 ✓；第 3 行是中间那笔竖画 `.......##.......` ⇒ 0x0180 ✓；
        //   第 4 行是横画 `..############..` ⇒ 0x3FFC ✓。
        // （第一版把两行的行号读错了一位 ✗，测试如实报出 384 vs 16380 ✓。）
        assert_eq!(Atlas::row(zhong, 0), 0, "「中」第 0 行应为空（上方留白）");
        assert_eq!(Atlas::row(zhong, 3), 0x0180, "「中」第 3 行应为中间竖画");
        assert_eq!(Atlas::row(zhong, 4), 0x3FFC, "「中」第 4 行应为横画");
        assert!(Atlas::pixel(zhong, 7, 3), "竖画所在的列（第 7 列）应被点亮");
        assert!(!Atlas::pixel(zhong, 0, 3), "该行第 0 列应为空");
        assert!(Atlas::pixel(zhong, 2, 4), "横画覆盖第 2 列");
    }
}
