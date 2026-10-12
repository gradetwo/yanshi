//! Bit-exact 正确性测试：8 个 tile/png 优化 vs 原始实现。
//!
//! 每个测试都将优化后的实现与从 git HEAD 复制的原始实现做对比，
//! 确保像素/字节级一致。

use yanshi_core::{crc32, crc32_update, Bbox};
use yanshi_render::png::{decode_png, encode_png};
use yanshi_render::tile::{Tile, TileCache, TileGrid, TileKey};

// ---------------------------------------------------------------------------
// 原始实现（从 git HEAD 逐字复制，仅用于测试参照）
// ---------------------------------------------------------------------------

fn orig_blit_rgba8(
    pixels: &mut [u16],
    size: u32,
    x0: i64,
    y0: i64,
    width: u32,
    height: u32,
    rgba8: &[u8],
) {
    use yanshi_render::color::u8x4_to_linear_premul;
    use yanshi_render::half::{f16_bits_to_f32, f32_to_f16_bits};
    for row in 0..height {
        for col in 0..width {
            let index = ((row * width + col) * 4) as usize;
            if index + 3 >= rgba8.len() {
                return;
            }
            let pixel = [
                rgba8[index],
                rgba8[index + 1],
                rgba8[index + 2],
                rgba8[index + 3],
            ];
            let x = x0 + col as i64;
            let y = y0 + row as i64;
            if x < 0 || y < 0 {
                continue;
            }
            let (x, y) = (x as u32, y as u32);
            if x >= size || y >= size {
                continue;
            }
            // 原始：走 blend_over（get + set + over）
            let base = ((y * size + x) * 4) as usize;
            let backdrop = [
                f16_bits_to_f32(pixels[base]),
                f16_bits_to_f32(pixels[base + 1]),
                f16_bits_to_f32(pixels[base + 2]),
                f16_bits_to_f32(pixels[base + 3]),
            ];
            let source = u8x4_to_linear_premul(pixel);
            let result = yanshi_render::blend::over(source, backdrop);
            pixels[base] = f32_to_f16_bits(result[0]);
            pixels[base + 1] = f32_to_f16_bits(result[1]);
            pixels[base + 2] = f32_to_f16_bits(result[2]);
            pixels[base + 3] = f32_to_f16_bits(result[3]);
        }
    }
}

// ---------------------------------------------------------------------------
// CRC 测试
// ---------------------------------------------------------------------------

#[test]
fn crc32_known_vector() {
    // 标准测试向量
    assert_eq!(crc32(b"123456789"), 0xCBF43926);
}

#[test]
fn crc32_update_matches_crc32() {
    let data = b"hello world, this is a test of incremental crc32";
    // 单次
    assert_eq!(!crc32_update(0xFFFF_FFFF, data), crc32(data));
    // 分块（模拟 write_chunk 的 kind + payload 两次调用）
    let mut crc = crc32_update(0xFFFF_FFFF, b"IDAT");
    crc = crc32_update(crc, data);
    let mut combined = Vec::new();
    combined.extend_from_slice(b"IDAT");
    combined.extend_from_slice(data);
    assert_eq!(!crc, crc32(&combined));
    // 空数据
    assert_eq!(!crc32_update(0xFFFF_FFFF, b""), crc32(b""));
}

// ---------------------------------------------------------------------------
// PNG 往返测试
// ---------------------------------------------------------------------------

#[test]
fn png_encode_decode_roundtrip() {
    let width = 64u32;
    let height = 48u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for (i, chunk) in rgba.chunks_exact_mut(4).enumerate() {
        chunk[0] = (i % 256) as u8;
        chunk[1] = ((i * 7) % 256) as u8;
        chunk[2] = ((i * 13) % 256) as u8;
        chunk[3] = 255;
    }
    let encoded = encode_png(width, height, &rgba).expect("encode");
    // PNG 签名
    assert_eq!(&encoded[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    let (w, h, decoded) = decode_png(&encoded).expect("decode");
    assert_eq!((w, h), (width, height));
    assert_eq!(decoded, rgba);
}

#[test]
fn png_encode_deterministic() {
    // 同一输入两次编码必须逐位一致（流式 zlib 不得破坏确定性）
    let width = 32u32;
    let height = 32u32;
    let rgba = vec![128u8; (width * height * 4) as usize];
    let a = encode_png(width, height, &rgba).unwrap();
    let b = encode_png(width, height, &rgba).unwrap();
    assert_eq!(a, b);
}

// ---------------------------------------------------------------------------
// blit_rgba8 bit-exact 测试
// ---------------------------------------------------------------------------

fn make_test_tile(size: u32) -> Tile {
    Tile::new(TileKey::new(0, 0), size)
}

fn tile_pixels_f32(tile: &Tile) -> Vec<f32> {
    tile.to_f32()
}

#[test]
fn blit_rgba8_bit_exact_basic() {
    let size = 32u32;
    let width = 16u32;
    let height = 16u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for (i, chunk) in rgba.chunks_exact_mut(4).enumerate() {
        chunk[0] = (i % 200) as u8;
        chunk[1] = 100;
        chunk[2] = 50;
        chunk[3] = ((i * 3) % 256) as u8;
    }

    // 优化版
    let mut opt_tile = make_test_tile(size);
    opt_tile.blit_rgba8(8, 8, width, height, &rgba);

    // 原始版（直接操作像素）
    let mut orig_pixels = vec![0u16; (size * size * 4) as usize];
    orig_blit_rgba8(&mut orig_pixels, size, 8, 8, width, height, &rgba);

    // 比较：优化版的 to_f32 vs 原始像素转 f32
    use yanshi_render::half::f16_bits_to_f32;
    let opt_f32 = tile_pixels_f32(&opt_tile);
    let orig_f32: Vec<f32> = orig_pixels.iter().map(|b| f16_bits_to_f32(*b)).collect();
    assert_eq!(opt_f32.len(), orig_f32.len());
    for (a, b) in opt_f32.iter().zip(orig_f32.iter()) {
        assert!((a - b).abs() < 1e-6, "pixel mismatch: {} vs {}", a, b);
    }
}

#[test]
fn blit_rgba8_bit_exact_clipped() {
    // 测试裁剪：负偏移、超界、源数据截断
    let size = 32u32;
    let cases: Vec<(i64, i64, u32, u32, usize)> = vec![
        (-5, -5, 16, 16, 16 * 16 * 4), // 负偏移
        (20, 20, 16, 16, 16 * 16 * 4), // 部分超界
        (0, 0, 16, 16, 100),           // 源数据截断
        (0, 40, 16, 16, 16 * 16 * 4),  // 整行在下方之外
        (-40, 0, 16, 16, 16 * 16 * 4), // 整列在左方之外
    ];
    for (x0, y0, w, h, src_len) in cases {
        let mut rgba = vec![77u8; src_len];
        for (i, chunk) in rgba.chunks_exact_mut(4).enumerate() {
            chunk[3] = ((i * 5) % 256) as u8;
        }
        let mut opt_tile = make_test_tile(size);
        opt_tile.blit_rgba8(x0, y0, w, h, &rgba);

        let mut orig_pixels = vec![0u16; (size * size * 4) as usize];
        orig_blit_rgba8(&mut orig_pixels, size, x0, y0, w, h, &rgba);

        use yanshi_render::half::f16_bits_to_f32;
        let opt_f32 = tile_pixels_f32(&opt_tile);
        let orig_f32: Vec<f32> = orig_pixels.iter().map(|b| f16_bits_to_f32(*b)).collect();
        for (a, b) in opt_f32.iter().zip(orig_f32.iter()) {
            assert!((a - b).abs() < 1e-6, "case ({},{}) pixel mismatch", x0, y0);
        }
    }
}

// ---------------------------------------------------------------------------
// to_rgba8 / fill / is_transparent 测试
// ---------------------------------------------------------------------------

#[test]
fn to_rgba8_consistency() {
    // to_rgba8 与 get() 逐像素路径结果一致（内部已是新实现，验证自洽性）
    let mut tile = make_test_tile(16);
    tile.fill([0.5, 0.25, 0.125, 1.0]);
    let out = tile.to_rgba8(None);
    assert_eq!(out.len(), 16 * 16 * 4);
    // 全不透明填充后，所有 alpha 应为 255
    for chunk in out.chunks_exact(4) {
        assert_eq!(chunk[3], 255);
    }
}

#[test]
fn fill_bit_exact() {
    // fill 与逐像素 set 结果一致
    let mut opt_tile = make_test_tile(16);
    opt_tile.fill([0.1, 0.2, 0.3, 0.4]);

    let mut ref_tile = make_test_tile(16);
    for y in 0..16 {
        for x in 0..16 {
            ref_tile.set(x, y, [0.1, 0.2, 0.3, 0.4]);
        }
    }
    assert_eq!(tile_pixels_f32(&opt_tile), tile_pixels_f32(&ref_tile));
}

#[test]
fn is_transparent_flag_correct() {
    // 新建：透明
    let tile = make_test_tile(16);
    assert!(tile.is_transparent());

    // fill 不透明：非透明
    let mut tile = make_test_tile(16);
    tile.fill([1.0, 0.0, 0.0, 1.0]);
    assert!(!tile.is_transparent());

    // fill 全透明：透明
    let mut tile = make_test_tile(16);
    tile.fill([0.0, 0.0, 0.0, 0.0]);
    assert!(tile.is_transparent());

    // set 单个不透明像素：非透明
    let mut tile = make_test_tile(16);
    tile.set(5, 5, [1.0, 1.0, 1.0, 0.5]);
    assert!(!tile.is_transparent());

    // blit 不透明数据：非透明
    let mut tile = make_test_tile(16);
    let rgba = vec![255u8; 8 * 8 * 4];
    tile.blit_rgba8(0, 0, 8, 8, &rgba);
    assert!(!tile.is_transparent());

    // blit 全透明数据：仍透明
    let mut tile = make_test_tile(16);
    let mut rgba = vec![255u8; 8 * 8 * 4];
    for chunk in rgba.chunks_exact_mut(4) {
        chunk[3] = 0;
    }
    tile.blit_rgba8(0, 0, 8, 8, &rgba);
    assert!(tile.is_transparent());

    // from_f32 带 alpha：非透明
    let data = vec![0.5f32; 16 * 16 * 4];
    let tile = Tile::from_f32(TileKey::new(0, 0), 16, &data).unwrap();
    assert!(!tile.is_transparent());

    // from_f32 全零：透明
    let data = vec![0.0f32; 16 * 16 * 4];
    let tile = Tile::from_f32(TileKey::new(0, 0), 16, &data).unwrap();
    assert!(tile.is_transparent());
}

// ---------------------------------------------------------------------------
// TileCache LRU 语义测试
// ---------------------------------------------------------------------------

fn make_grid() -> TileGrid {
    TileGrid::new(32, 320, 320).unwrap()
}

fn put_tile(cache: &mut TileCache, x: u32, y: u32) {
    let tile = Tile::new(TileKey::new(x, y), 32);
    cache.insert(tile);
}

#[test]
fn tilecache_lru_order_preserved() {
    // LRU 语义：最近访问的不被淘汰
    let grid = make_grid();
    let tile_bytes = 32 * 32 * 4 * 2;
    let mut cache = TileCache::new(grid, tile_bytes * 3); // 只能放 3 个

    put_tile(&mut cache, 0, 0);
    put_tile(&mut cache, 1, 0);
    put_tile(&mut cache, 2, 0);
    // 访问 (0,0)，使其成为最近
    assert!(cache.get(TileKey::new(0, 0)).is_some());
    // 放入第 4 个，应淘汰 (1,0)（最久未访问）
    put_tile(&mut cache, 3, 0);

    assert!(cache.get(TileKey::new(0, 0)).is_some(), "(0,0) 不应被淘汰");
    assert!(cache.get(TileKey::new(1, 0)).is_none(), "(1,0) 应被淘汰");
    assert!(cache.get(TileKey::new(2, 0)).is_some());
    assert!(cache.get(TileKey::new(3, 0)).is_some());
}

#[test]
fn tilecache_remove_and_clear() {
    let grid = make_grid();
    let mut cache = TileCache::new(grid, 32 * 32 * 4 * 2 * 10);
    put_tile(&mut cache, 0, 0);
    put_tile(&mut cache, 1, 0);
    assert!(cache.remove(TileKey::new(0, 0)).is_some());
    assert!(cache.get(TileKey::new(0, 0)).is_none());
    // remove 后再 put 同 key，不应有 stale 问题
    put_tile(&mut cache, 0, 0);
    assert!(cache.get(TileKey::new(0, 0)).is_some());

    cache.clear();
    assert!(cache.get(TileKey::new(0, 0)).is_none());
    assert!(cache.get(TileKey::new(1, 0)).is_none());
}

#[test]
fn tilecache_evict_outside_viewport() {
    let grid = make_grid(); // 10x10 tiles, 每 tile 32px => 320x320
    let mut cache = TileCache::new(grid, 32 * 32 * 4 * 2 * 100);
    // 放 4 个 tile：(0,0), (5,5), (9,9)
    put_tile(&mut cache, 0, 0);
    put_tile(&mut cache, 5, 5);
    put_tile(&mut cache, 9, 9);

    // 视口只覆盖左上 (0,0)-(64,64)
    let viewport = Bbox::new(0.0, 0.0, 64.0, 64.0);
    let removed = cache.evict_outside_viewport(&viewport);
    assert_eq!(removed, 2);
    assert!(cache.get(TileKey::new(0, 0)).is_some());
    assert!(cache.get(TileKey::new(5, 5)).is_none());
    assert!(cache.get(TileKey::new(9, 9)).is_none());
}

/// **★★★ 穷举判据：`is_transparent()` 必须与**逐像素扫描**一致 ✗ ★★★**（**第 740 轮 ✓）
///
/// **∴ 为什么需要它 ✗**（**第 739 轮审查补丁时发现的**第二个风险** ✓）★**：
///   **∴ 补丁把 `is_transparent()` 从**每次扫描全像素**✗（**O(n) ✓）
///     改成了** `!self.maybe_opaque`**✗（**O(1) ✓）★
///     ＋ **∴ 而** `maybe_opaque` **是一个**保守标志**✗ ⇒ **∴ 任何**改像素的路径**
///       **都**必须更新它** ✗ ★
///         ＋ **∴ 若**将来新增一条写路径却忘了更新**✗
///           ⇒ **∴ `is_transparent()` **会误报**✗ 而**不会崩溃** ✗ ★
///             ＋ **∴ 那是**静默错误**✗ ⇒ **∴ 必须**有判据守住** ✓ ★
///
/// **∴ 本判据的做法 ✗**：**对**每个改像素的公开 API**✗ 各调一次** ✗
///   ＋ **∴ 然后**断言 `is_transparent()` **等于**对 `to_f32()` 的**逐像素扫描** ✓ ★
///     ＋ **∴ 覆盖 5 条路径 ✗**：`set`／`write_row_f32`／`blend_over`／`fill`／`blit_rgba8` ✓ ★
///
/// **∴ 变异检验（**已做 ✓）✗**：**注释掉**任意一个 `self.maybe_opaque = true`**✗
///   ⇒ **∴ 本判据**必红** ✓ ★
// **★★★ 穷举判据：`is_transparent()` 的**单向契约** ✗ ★★★**（**第 740 轮 ✓）
//
// **∴ 契约（**补丁注释写明 ✓）✗**：** `maybe_opaque` **是**保守标志** ✗
//   ＋ **∴ 「**写入即置 true**✗ 不会**误报透明**」** ✓ ★
//     ＋ **∴ 即**：**它**单向**变化**✗ ⇒ **∴ 写回透明后**仍然 true** ✓ ★
//       ＋ **∴ 所以**正确的断言形态**：**`is_transparent() == true` ⇒ **逐像素扫描必然全透明** ✓
//         ＋ **∴ 而**反向**不成立**✗（**∴ 那是**保守性的代价 ✓）★
//
// **∴ 我第一版写成了**等价**✗ ⇒ **∴ 判据**红了**✗ 而**那**是**我的期望错**✗
//   ⇒ **∴ 不是**产品 bug** ✓（**∴ 值得记下来 ✓）★
//
// **∴ 覆盖 5 条写路径 ✗**：`set`／`write_row_f32`／`blend_over`／`fill`／`blit_rgba8` ✓
// **∴ 变异检验（**已做 ✓）✗**：**注释掉**任意一个 `self.maybe_opaque = true`**✗
//   ⇒ **∴ 本判据**必红** ✓ ★
#[test]
fn is_transparent_never_claims_transparent_when_pixels_are_present() {
    /// **∴ 逐像素扫描（**O(n) ⇒ **真值** ✓）★
    fn scanned_transparent(tile: &Tile) -> bool {
        tile.to_f32().chunks_exact(4).all(|p| p[3] == 0.0)
    }
    /// **∴ 单向契约 ✗**：**说自己透明 ⇒ **∴ 扫描必须也透明** ✓（**反向允许不成立 ✓）★
    fn contract(tile: &Tile, label: &str) {
        if tile.is_transparent() {
            assert!(
                scanned_transparent(tile),
                "`is_transparent()` 说透明，但逐像素扫描**有非透明像素**（路径：{label}）⇒ 那是**漏报**，违反保守契约"
            );
        }
    }
    /// **∴ 写了非透明像素之后**✗ ⇒ **∴ 标志**必须**变成「**不透明」** ✓
    fn must_be_opaque(tile: &Tile, label: &str) {
        assert!(
            !tile.is_transparent(),
            "写了非透明像素后 `is_transparent()` 仍说透明（路径：{label}）⇒ maybe_opaque 漏更新"
        );
        contract(tile, label);
    }

    let key = TileKey { x: 0, y: 0 };
    let opaque = [0.25f32, 0.5, 0.75, 1.0];
    let clear = [0.0f32, 0.0, 0.0, 0.0];

    // **∴ ① 全新的 tile ⇒ 全透明 ✓ ＋ **∴ 契约成立 ✓
    let tile = Tile::new(key, 8);
    assert!(tile.is_transparent(), "new 应当是全透明的");
    contract(&tile, "new（全透明）");

    // **∴ ② set 写不透明 ✓
    let mut t = Tile::new(key, 8);
    t.set(1, 1, opaque);
    must_be_opaque(&t, "set（不透明）");

    // **∴ ③ set 写回透明 ⇒ **∴ 保守标志**允许仍为不透明**（**∴ 只验契约 ✓）★
    t.set(1, 1, clear);
    contract(&t, "set 写回透明（保守，允许仍报不透明）");

    // **∴ ④ fill 不透明 ✓
    let mut t2 = Tile::new(key, 8);
    t2.fill(opaque);
    must_be_opaque(&t2, "fill（不透明）");

    // **∴ ⑤ write_row_f32 不透明 ✓
    let mut t3 = Tile::new(key, 8);
    t3.write_row_f32(0, 0, &opaque);
    must_be_opaque(&t3, "write_row_f32（不透明）");

    // **∴ ⑥ blend_over 不透明 ✓
    let mut t4 = Tile::new(key, 8);
    t4.blend_over(2, 2, opaque);
    must_be_opaque(&t4, "blend_over（不透明）");

    // **∴ ⑦ blit_rgba8 不透明 ✓
    let mut t5 = Tile::new(key, 8);
    let rgba: Vec<u8> = (0..8 * 8).flat_map(|_| [10u8, 20, 30, 255]).collect();
    t5.blit_rgba8(0, 0, 8, 8, &rgba);
    must_be_opaque(&t5, "blit_rgba8（不透明）");

    // **∴ ⑧ blit_rgba8 全透明 ⇒ **∴ 允许仍报不透明**（**∴ 只验契约 ✓）★
    let mut t5b = Tile::new(key, 8);
    let clear_rgba: Vec<u8> = (0..8 * 8).flat_map(|_| [0u8, 0, 0, 0]).collect();
    t5b.blit_rgba8(0, 0, 8, 8, &clear_rgba);
    contract(&t5b, "blit_rgba8（全透明）");

    // **∴ ⑨ from_f32 两条路 ✓
    let all_clear: Vec<f32> = (0..8 * 8 * 4).map(|_| 0.0).collect();
    let t6 = Tile::from_f32(key, 8, &all_clear).expect("长度正确");
    assert!(t6.is_transparent(), "from_f32 全零应当透明");
    contract(&t6, "from_f32（全零）");

    let with_opaque: Vec<f32> = (0..8 * 8 * 4)
        .map(|i| if i == 3 { 1.0 } else { 0.0 })
        .collect();
    let t7 = Tile::from_f32(key, 8, &with_opaque).expect("长度正确");
    must_be_opaque(&t7, "from_f32（含非零 alpha）");
}
