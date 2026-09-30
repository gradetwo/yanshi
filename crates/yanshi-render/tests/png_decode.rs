//! PNG **解码**（设计 791 行：`import_image` 接受 JPEG/PNG/WebP ✓）。
//!
//! 浏览器端已经能用 `createImageBitmap` 解码任意格式 ✓ ⇒ **界面**这条路不缺 ✓；
//! 缺的是**工具/API**这条路 ✗ —— 服务端此前只收原始 RGBA ✓，
//! 界面是自己先解成 raw 再上传的 ✓，于是把这个缺口遮住了 ✓。
//!
//! 测试分三层 ✓，一层比一层更"像真实的别人家的 PNG" ✓：
//! ① **往返**：本仓库编码器产出的 PNG（固定 Huffman + filter 0 ✓）解回来必须**逐字节一致** ✓；
//! ② **真实编码器**：PIL/zlib 产出的 PNG ✓，其中 `large_rgb.png` 用的是 **dynamic Huffman**（BTYPE=2 ✓）
//!    —— 这才是现实里绝大多数 PNG 的形态 ✓，也是最容易写错的一条路径 ✓；
//! ③ **显式拒绝**：16 位、CRC 损坏、截断 ⇒ 必须返回 `None` ✓ 而不是画出半张图 ✓。

use yanshi_render::png::{decode_png, encode_png};

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("读取夹具 {path:?} 失败：{error}"))
}

/// ① **往返**：自己编码 ⇒ 自己解码 ⇒ 逐字节一致 ✓。
#[test]
fn encoding_then_decoding_round_trips_exactly() {
    for (width, height) in [(1u32, 1u32), (7, 5), (64, 40)] {
        let mut rgba: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
        for index in 0..(width * height) {
            rgba.extend_from_slice(&[
                (index % 251) as u8,
                ((index * 7) % 253) as u8,
                ((index * 13) % 247) as u8,
                if index % 5 == 0 { 128 } else { 255 },
            ]);
        }
        let encoded = encode_png(width, height, &rgba).expect("应能编码");
        let (decoded_width, decoded_height, decoded) =
            decode_png(&encoded).expect("自己编码的 PNG 必须能解回来");
        assert_eq!(
            (decoded_width, decoded_height),
            (width, height),
            "尺寸应一致"
        );
        assert_eq!(decoded, rgba, "像素应逐字节一致（{width}×{height}）");
    }
}

/// ② **真实编码器产出的 PNG** ✓ —— 含 **dynamic Huffman** ✓ 与自适应 filter ✓。
#[test]
fn a_real_encoders_png_decodes_to_the_expected_pixels() {
    // `large_rgb.png`：64×64 RGB ✓，生成公式是测试里可复算的 ✓（见夹具生成脚本的注释 ✓）。
    let bytes = fixture("large_rgb.png");
    let (width, height, rgba) = decode_png(&bytes).expect("真实编码器的 PNG 应能解开");
    assert_eq!((width, height), (64, 64));
    assert_eq!(rgba.len(), 64 * 64 * 4);
    for y in 0..64usize {
        for x in 0..64usize {
            let index = (y * 64 + x) * 4;
            let noise = (x * 7 + y * 13) % 23;
            let expected = [
                ((x * 3 + noise) % 256) as u8,
                ((y * 5 + noise * 2) % 256) as u8,
                (((x ^ y) + noise) % 256) as u8,
                255,
            ];
            assert_eq!(
                &rgba[index..index + 4],
                &expected,
                "({x},{y}) 的像素应与生成公式一致"
            );
        }
    }

    // RGB（无 alpha 通道 ✓）⇒ 解码后 alpha 必须是**不透明** ✓，而不是 0 ✓。
    let (_, _, rgb_only) = decode_png(&fixture("gradient_rgb.png")).expect("RGB PNG 应能解开");
    assert!(
        rgb_only.chunks_exact(4).all(|pixel| pixel[3] == 255),
        "RGB PNG 解码后 alpha 应为 255"
    );

    // RGBA ✓：**半透明必须保留** ✓（否则导入会丢掉透明度 ✗）。
    let (_, _, with_alpha) = decode_png(&fixture("flat_rgba.png")).expect("RGBA PNG 应能解开");
    assert_eq!(with_alpha.len(), 6 * 5 * 4);
    // 生成的图：x < 3 不透明、x ≥ 3 为 96 ✓。
    for y in 0..5usize {
        for x in 0..6usize {
            let index = (y * 6 + x) * 4;
            let expected_alpha = if x < 3 { 255 } else { 96 };
            assert_eq!(
                with_alpha[index + 3],
                expected_alpha,
                "({x},{y}) 的 alpha 应保留（RGBA 导入不能丢透明度）"
            );
        }
    }
}

/// ③ **不支持的形态必须显式拒绝** ✓ —— 返回 `Ok` 却画出半张图是本项目最忌的"静默错误" ✗。
#[test]
fn unsupported_or_corrupt_pngs_are_refused() {
    // 16 位深 ✓。
    assert!(
        decode_png(&fixture("sixteen_bit.png")).is_none(),
        "16 位应被拒绝"
    );

    // **CRC 损坏** ✓：改一个像素字节，块 CRC 就对不上 ✓。
    let mut broken = fixture("large_rgb.png");
    let idat = broken
        .windows(4)
        .position(|window| window == b"IDAT")
        .expect("夹具应有 IDAT");
    let target = idat + 20;
    broken[target] ^= 0xFF;
    assert!(decode_png(&broken).is_none(), "CRC 损坏应被拒绝");

    // 截断 ✓。
    let truncated = &fixture("large_rgb.png")[..40];
    assert!(decode_png(truncated).is_none(), "截断应被拒绝");

    // 根本不是 PNG ✓。
    assert!(decode_png(b"not a png at all").is_none(), "非 PNG 应被拒绝");
    assert!(decode_png(&[]).is_none(), "空输入应被拒绝");
}
