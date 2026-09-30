# Bitmap font atlas

`yanshi-bitmap-16.bin` is a **1-bit, 16×16 bitmap atlas** rasterised offline from an open-source font,
so the kernel can stay dependency-free and render text identically on every platform (no hinting, no
platform font differences), which is what the design asks for in its font and cross-platform notes.

## Provenance

| | |
|---|---|
| Source font | **Noto Sans CJK SC, Regular** (`/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc`) |
| Package | `noto-fonts-cjk 20240730-1` (verified with `pacman -Qo`) |
| Licence | **SIL Open Font License 1.1** — see [`LICENSE-OFL-NotoSansCJK.txt`](LICENSE-OFL-NotoSansCJK.txt) |
| Derived data | rasterised bitmaps of a glyph subset; redistribution of derived data is permitted by the OFL with attribution (this file) |

## Coverage

4108 glyphs, sorted by code point so the kernel can binary-search:

* ASCII `U+0020`–`U+007E`
* **GB2312 level-1: 3755 common hanzi** (`0xB0A1`–`0xD7F9`)
* GB2312 symbol rows (`0xA1`–`0xA3`): CJK punctuation, kana, Greek and Cyrillic basics

Glyphs the source font does not provide are omitted; the kernel falls back to its built-in 5×7 ASCII
font and finally to `?`.

## Format (little-endian)

```
magic  "YFNT"            4 bytes
cell_w u8, cell_h u8     cell size (16 × 16 here ⇒ 32 bytes per glyph)
count  u32               number of glyphs
count × (codepoint u32, bitmap[cell_w*cell_h/8])   row-major, MSB leftmost
```

## Regenerating

`scripts/build-bitmap-font.py` is a **development-time** tool — it needs Pillow and is not a build
dependency of the crate; the generated `.bin` is committed so builds stay offline and reproducible.

```sh
python3 -m venv /tmp/fontvenv && /tmp/fontvenv/bin/pip install pillow
/tmp/fontvenv/bin/python scripts/build-bitmap-font.py \
    --font /usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc \
    --out assets/fonts/yanshi-bitmap-16.bin
```
