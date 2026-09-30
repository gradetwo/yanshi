#!/usr/bin/env python3
"""从可再分发的开源字体生成内核用的 **1-bit 位图字体图集**。

设计 1175 / 1287 行要求「内嵌开源字体子集，禁用平台相关 hinting，首版仅 Latin/CJK 基础」，
并特别把「字体版权 / 跨端一致性」列为风险项。本脚本因此只做一件事：
把某个 **OFL 许可**（可再分发、允许派生）的字体，离线光栅化成内核可直接读的位图数据。

为什么用位图而不是矢量：内核是**零依赖**的，自己实现 TrueType 解析 + 栅格化是多轮工程；
位图图集把复杂度留在**生成期**（本脚本，开发工具），运行期只需按码位查表 ✓，
且**跨端完全一致**（没有 hinting、没有平台差异）✓ —— 正合设计的诉求 ✓。

用法（开发期，需要 Pillow）：
    python3 scripts/build-bitmap-font.py --font <ttf/ttc> --out assets/fonts/yanshi-bitmap-16.bin

字形集（按需要可调）：
  * ASCII 0x20–0x7E
  * GB2312 一级字库（3755 个常用汉字，最常用的一批）
  * GB2312 二级字库（3008 个次常用汉字，多为人名/地名用字）
  * GB2312 符号区 0xA1–0xA3 行（中文标点、日文假名、希腊/西里尔等基础符号）

文件格式（小端）：
    magic  "YFNT"           4 字节
    cell_w u8, cell_h u8    单元尺寸（本脚本为 16×16 ⇒ 每字形 32 字节）
    count  u32              字形数
    随后 count × (codepoint u32, bitmap[cell_w*cell_h/8])
字形按码位**升序**存放 ⇒ 内核可二分查找 ✓。
"""

import argparse
import struct
import sys

from PIL import Image, ImageDraw, ImageFont

CELL = 16
THRESHOLD = 96  # 1-bit 阈值：0..255 中位偏上，保证笔画不至于断
MARGIN = 1


def glyph_set() -> list[int]:
    """要生成的字形码位集合（升序）。"""
    codes: set[int] = set()

    # ASCII 可打印区
    codes.update(range(0x20, 0x7F))

    # GB2312 一级字库（0xB0A1–0xD7F9，3755 个常用汉字）
    # + 二级字库（0xD8A1–0xF7FE，3008 个次常用汉字，多为人名/地名用字）
    # 说明：这里只是用 GB2312 的**分区**作为"常用度分档"的现成清单 ✓，
    # 与编码无关 ✓ —— 图集按 **Unicode 码位**索引 ✓，项目全程 UTF-8 ✓。
    for high in range(0xB0, 0xF8):
        for low in range(0xA1, 0xFF):
            try:
                text = bytes([high, low]).decode("gb2312")
            except UnicodeDecodeError:
                continue
            codes.add(ord(text))

    # GB2312 符号区前几行：中文标点、假名、希腊/西里尔字母等
    for high in range(0xA1, 0xA4):
        for low in range(0xA1, 0xFF):
            try:
                text = bytes([high, low]).decode("gb2312")
            except UnicodeDecodeError:
                continue
            codes.add(ord(text))

    return sorted(codes)


def render_glyph(font: ImageFont.FreeTypeFont, char: str) -> bytes:
    """把单个字形光栅化成 16×16 的 1-bit 位图（行主序，每行高位在左）。"""
    image = Image.new("L", (CELL, CELL), 0)
    draw = ImageDraw.Draw(image)
    # 按**字形墨迹包围盒**在格子里居中（而不是按基线）：
    # 直接用基线画会把 CJK 字形的下半截裁掉（本脚本第一版就是这样 ✗）。
    try:
        left, top, right, bottom = font.getbbox(char)
    except Exception:
        left = top = right = bottom = 0
    ink_w = max(0, right - left)
    ink_h = max(0, bottom - top)
    usable = CELL - 2 * MARGIN
    if ink_w > usable or ink_h > usable:
        # 超过可用范围：按比例缩小绘制尺寸（保持整数，避免平台差异）。
        scale = min(usable / max(1, ink_w), usable / max(1, ink_h))
        size = max(6, int(round(getattr(font, "size", CELL) * scale)))
        font = ImageFont.truetype(font.path, size)
        left, top, right, bottom = font.getbbox(char)
        ink_w = max(0, right - left)
        ink_h = max(0, bottom - top)
    offset_x = MARGIN + max(0, (usable - ink_w) // 2) - left
    offset_y = MARGIN + max(0, (usable - ink_h) // 2) - top
    draw.text((offset_x, offset_y), char, font=font, fill=255)

    pixels = image.load()
    rows = []
    for y in range(CELL):
        value = 0
        for x in range(CELL):
            if pixels[x, y] > THRESHOLD:
                value |= 1 << (CELL - 1 - x)
        rows.append(value)
    # 每行 2 字节（16 位），共 32 字节。
    out = bytearray()
    for row in rows:
        out += struct.pack(">H", row)
    return bytes(out)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--font", required=True, help="源字体文件（OFL 等可再分发许可）")
    parser.add_argument("--out", required=True, help="输出 .bin 路径")
    parser.add_argument("--size", type=int, default=CELL, help="光栅化字号（缺省 16）")
    args = parser.parse_args()

    font = ImageFont.truetype(args.font, args.size)
    font.path = args.font  # 供缩放分支使用
    codes = glyph_set()

    glyphs = []
    skipped = 0
    for code in codes:
        char = chr(code)
        if not char.isprintable():
            continue
        bitmap = render_glyph(font, char)
        if not any(bitmap):
            # 该字体没有这个字形（.notdef 为空）：跳过，运行期回退到内置 5×7 或 `?`
            skipped += 1
            continue
        glyphs.append((code, bitmap))

    with open(args.out, "wb") as handle:
        handle.write(b"YFNT")
        handle.write(struct.pack("<BB", CELL, CELL))
        handle.write(struct.pack("<I", len(glyphs)))
        for code, bitmap in glyphs:
            handle.write(struct.pack("<I", code))
            handle.write(bitmap)

    total = 8 + len(glyphs) * (4 + CELL * CELL // 8)
    print(f"字形 {len(glyphs)} 个（跳过无字形 {skipped} 个）⇒ {args.out}（{total} 字节 ≈ {total/1024:.1f}KB）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
