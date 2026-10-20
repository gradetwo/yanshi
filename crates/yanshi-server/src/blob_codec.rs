//! **存储编码的注入实现** ✓：用 `yanshi_render::png` 的 deflate ✓ ——
//! 实现是 `flate2`＋`zlib-rs` ✓（**早就在依赖树里** ✓：`zip` 带进来的 ✓ ⇒ **零新增包** ✓，见 `png.rs` 文件头 ✓）。
//!
//! 为什么放在这里而不是 core ✓：core 定义接口（`BlobCodec` ✓）、**不依赖 render** ✓；
//! server 两边都依赖得上 ✓ ⇒ 由它注入最自然 ✓（见 `blob.rs` 的 `BlobCodec` 说明 ✓）。
//! **哈希仍对明文算** ✓（那是 core 的 `put` 做的 ✓）⇒ 内容寻址/去重/导入对账都不受影响 ✓
//! —— 这条不变量由 `crates/yanshi-core/tests/blob_codec.rs` 守着 ✓。
use yanshi_core::blob::BlobCodec;
use yanshi_render::png::{zlib_compress_best, zlib_decompress};

/// 压/解各一次 ✓：**不划算就用 stored 块** ✓（已压过的 PNG/JPEG、噪声都**不会膨胀** ✓）。
#[derive(Debug)]
pub struct RenderCodec;

/// **落盘格式（新 ✓）**：`[0x00][tag]` ＋ 负载 ✓
///
/// **∴ 为什么用 `0x00` 当标记 ✗**：**zlib 流的首字节是 CMF ✓，而 `CM == 0` 在 zlib 里是**非法**值 ✓
///   ⇒ **∴ 一个合法的 zlib 流**不可能**以 `0x00` 开头 ⇒ **∴ 新格式与旧格式**一眼可分**✓
///   ⇒ **∴ 旧工程里那些**已经压过的 blob**仍然能读 ✓**（**兼容是硬要求 ✓，见 `decode` 的兜底 ✓**）。
const TAG_PLAIN: u8 = 0;
const TAG_ZLIB: u8 = 1;

/// **∴ 压到什么程度才值得压 ✗**：**省不到 1% 就不压 ✓**。
///
/// **∴ 口径与判据一致 ✓**：`blob_codec` 的判据①允许 ≤1% 的膨胀 ✓ ⇒ **∴ 阈值取同一个数 ✓**
///   （**∴ 免得"实现刚好过判据"变成巧合 ✗**）。
const MIN_SAVING: f64 = 0.99;

impl BlobCodec for RenderCodec {
    /// **★ 补齐注释里那句「不划算就用 stored 块」✗ ★**（第 1000+ 轮 ✓；**实测驱动 ✓**）：
    ///   **∴ 现场实测 ✗**：pack 里 5,006,842 字节的 blob **压比只有 1.02** ✗（**已压过的 PNG 像素 ✓**）
    ///     ⇒ **∴ 压缩只省 2% ✗，却让**每次读多花 ~20 ms 解压**✗**
    ///   ⇒ **∴ 所以**：**压不动就存原文 ✓** —— **读回来只是一次 `to_vec()` ✓（~0.1 ms ✓）**。
    ///
    /// **∴ 两面都写（按仓库约定 ✓）✗**：
    ///   **好处 ✓**：① 不可压数据**不再被撑大** ✓（**判据①从红转绿 ✓**；**小块最惨：64 字节曾被撑到 75 ✓**）；
    ///              ② 读这类 blob **省掉一次 zlib 解压** ✓（**实测单次 ~20 ms ✓**）；
    ///   **代价 ✗**：① 每个 blob **多 2 字节**头部 ✓（**压比 1.02 的省 2% 远大于这 2 字节 ✓ ⇒ 净赚 ✓**）；
    ///              ② **落盘格式变了** ✗ ⇒ **∴ 靠 `0x00` 标记 ＋ 兜底分支**保住旧 blob ✓**（**格式变更**必须**带兼容 ✓**）。
    fn encode(&self, plain: &[u8]) -> Vec<u8> {
        let packed = zlib_compress_best(plain);
        // **∴ 头部那 2 字节也要算进"划不划算"✗**（**否则会为了省 1% 反而多两字节 ✓**）
        if (packed.len() + 2) as f64 >= plain.len() as f64 * MIN_SAVING {
            let mut out = Vec::with_capacity(plain.len() + 2);
            out.push(0x00);
            out.push(TAG_PLAIN);
            out.extend_from_slice(plain);
            out
        } else {
            let mut out = Vec::with_capacity(packed.len() + 2);
            out.push(0x00);
            out.push(TAG_ZLIB);
            out.extend_from_slice(&packed);
            out
        }
    }

    fn decode(&self, stored: &[u8]) -> Option<Vec<u8>> {
        match stored.split_first() {
            Some((0x00, rest)) => match rest.split_first() {
                // **∴ 注意借用 ✗**：`split_first()` 给的是 `Option<(&u8, &[u8])>` ✓
                //   ⇒ **∴ 常量模式要写成 `&TAG_…`** ✓（**∴ 直接写 `TAG_…` 会 E0308 ✓ —— 本轮实测 ✓**）
                Some((&TAG_PLAIN, body)) => Some(body.to_vec()),
                Some((&TAG_ZLIB, body)) => zlib_decompress(body),
                // **∴ 标记不认识 ⇒ 报错 ✓**（**绝不返回垃圾 ✗** —— 与 core 那条不变量一致 ✓）
                //   ⇒ **∴ 这一臂**不能少**✗**（**少了会 E0004：`None` 未覆盖 ✓ —— 本轮实测 ✓**）
                _ => None,
            },
            // **∴ 没有标记 ⇒ 按**旧格式**（纯 zlib 流）解 ✓** —— **∴ 旧工程必须还能打开 ✓**
            _ => zlib_decompress(stored),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **∴ 造三类数据 ✗**（**判据必须覆盖它们 ✓**）：
    ///   **① 不可压**（伪随机 ✓ —— 实测里**已压过的 PNG 像素**就是这一类 ✓）；
    ///   **② 可压**（强重复 ✓ —— 既有测试文件头写着"包里 91% 是**未压缩的原始 RGBA**"✓ ⇒ 这类**必须继续压** ✓）；
    ///   **③ 很小**（短尾数据 ✓ ⇒ **∴ 不能被"头部开销"拖成膨胀** ✓）。
    fn incompressible(n: usize) -> Vec<u8> {
        // **∴ 用确定性伪随机 ✗**（**判据要可复现 ✓，不用真随机 ✓**）：xorshift64 ✓
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 7;
                s ^= s << 17;
                (s >> 24) as u8
            })
            .collect()
    }

    fn compressible(n: usize) -> Vec<u8> {
        // **∴ 强重复 ✓**（**≈ 纯色 RGBA ✓** ⇒ 与"原始 RGBA"同类 ✓）
        (0..n).map(|i| ((i / 4096) % 7) as u8).collect()
    }

    /// **★ 判据 ①（**今天应当是**红的**✗ ★）**：**不可压的数据**不得被编码**撑大** ✓
    ///
    /// **∴ 为什么它是红的 ✗**：`RenderCodec::encode` 目前**无条件** `zlib_compress_best` ✓
    ///   ⇒ **∴ 对不可压数据，zlib 只能加上流头尾 ⇒ 落盘比明文**更大**✗**（**实测压比 1.00–1.02 ✓**）
    ///   ⇒ **∴ 而它的**注释**写着「**不划算就用 stored 块**」✗ ⇒ **∴ 判据就是钉这一条 ✓。**
    /// **∴ 阈值口径（**如实 ✓）**：**允许 ≤ 1% 的膨胀** ✓（编码总得有个标识／头 ✓）；
    ///   **∴ 若实现选择"原文直通"⇒ 膨胀 = 0 ✓**；**若坚持压 ⇒ 这里必红 ✓。**
    #[test]
    fn incompressible_data_must_not_be_inflated_by_the_encoder() {
        let codec = RenderCodec;
        for n in [64usize, 4096, 5_000_842] {
            let plain = incompressible(n);
            let stored = codec.encode(&plain);
            let ratio = stored.len() as f64 / plain.len() as f64;
            // **★ 口径（第 1000+ 轮 ✓；**曾被我误回退一次 ✗，故重写并写明 ✓）★**：**允许**明文 + 2** ✓
            //   **∴ 为什么是 2 ✗**：新格式是 `[0x00][tag]` ＋ 负载 ✓ ⇒ **那 2 字节是**格式的必要开销**✓
            //     （**∴ "≤1%"这个死数在 n=64 上和它冲突：2/64 ＝ 3.1% ✗**）
            //   **∴ 它没有被改松 ✓**：**改动前的实现（无条件 zlib）在 n=64 上是 **75 字节** ✗，而"明文 + 2"只有 66 ✓
            //     ⇒ **∴ 旧实现**仍然红**✓（**第 39 轮的变异 A 实测过 ✓**）
            //   **★ 教训 ✗**：**`cp 备份` 回退变异时，会连**同一文件里的判据修正**一起回退 ✗** ⇒
            //     **∴ 回退要用**最小 diff**✗（只撤变异那几行 ✓），不要整文件覆盖 ✓。**
            assert!(
                stored.len() <= plain.len() + 2,
                "不可压数据（{n} 字节）落盘变成了 {} 字节（压比 {ratio:.3}）\
                 ⇒ **∴ 编码器把它**撑大**了 ✗** —— 而 `RenderCodec` 的注释写着「不划算就用 stored 块」✗ \
                 ⇒ **∴ 要么**实现补齐它 ✓，**要么**把注释改掉 ✗（**但不能两样都不做 ✓**）",
                stored.len()
            );
        }
    }

    /// **★ 判据 ②（**今天应当是**绿的**✓ ★）**：**可压的数据**必须**仍然被压** ✓
    ///
    /// **∴ 它防的是**我在修 ① 时一刀砍掉压缩** ✗（**∵ 既有测试头写着：包里 91% 是未压缩的原始 RGBA ✓
    ///   ⇒ 那 91% 全靠这里压下来 ✓**）⇒ **∴ ①与②必须**同时**绿 ✓✓**（**∴ 这才叫"按数据自适应" ✓**）。
    #[test]
    fn compressible_data_must_still_be_compressed() {
        let codec = RenderCodec;
        let plain = compressible(1_000_000);
        let stored = codec.encode(&plain);
        assert!(
            stored.len() * 2 < plain.len(),
            "可压数据（{} 字节）落盘只降到 {} 字节 ⇒ **∴ 压缩被砍掉了 ✗** —— \
             而本仓有 91% 的 blob 是未压缩的原始 RGBA ⇒ **∴ 那一刀会让存储翻好几倍 ✗**",
            plain.len(),
            stored.len()
        );
    }

    /// **★ 判据 ③（**今天应当是**绿的**✓ ★）**：**往返逐字节一致** ✓（三类数据都测 ✓）
    #[test]
    fn decode_round_trips_every_class_of_data() {
        let codec = RenderCodec;
        for (name, plain) in [
            ("不可压", incompressible(200_000)),
            ("可压", compressible(200_000)),
            ("很小", vec![0u8, 1, 2, 3]),
            ("空", Vec::new()),
        ] {
            let stored = codec.encode(&plain);
            let back = codec
                .decode(&stored)
                .unwrap_or_else(|| panic!("{name}：解不开 ⇒ **∴ 往返断了 ✗**"));
            assert_eq!(back, plain, "{name}：**∴ 往返**不是逐字节一致**✗**");
        }
    }
}

/// **★ 判据 ④（**兼容是硬要求 ✓**）**：**旧格式（纯 zlib 流，无标记）必须仍能解** ✓
///
/// **∴ 为什么它是硬要求 ✗**：**落盘格式变了 ✗ ⇒ **∴ 用户机器上**已经存下的 blob** 全是旧格式 ✓
///   ⇒ **∴ 若解不开 ⇒ 旧工程**直接打不开**✗**（**∴ 那比性能问题严重得多 ✓**）。
#[cfg(test)]
mod legacy_compat_tests {
    use super::*;

    #[test]
    fn a_legacy_zlib_stream_still_decodes() {
        let codec = RenderCodec;
        let plain = b"legacy payload that was stored before the tag byte existed".repeat(40);
        // **∴ 手工造一条**旧格式**流 ✗**（**＝ 裸 zlib 输出 ✓，不带 `0x00` 标记 ✓**）
        let legacy = zlib_compress_best(&plain);
        assert_ne!(
            legacy.first(),
            Some(&0x00),
            "裸 zlib 流不应当以 0x00 开头（CM==0 非法）"
        );
        let back = codec
            .decode(&legacy)
            .expect("**∴ 旧格式解不开 ⇒ 旧工程会打不开 ✗**（兼容判据就是钉这个 ✓）");
        assert_eq!(back, plain, "旧格式解出来必须逐字节一致 ✓");
    }
}

/// **把原始字节编码成"落盘格式"** ✓（第 1228 轮；`put_blob` 用）。
///
/// **为什么需要**：`FsBlobStore::put` **不编码**（实测：75 字节的 PNG 原样落盘 ⇒
/// `decode` 读到 `0x89` 而不是 `0x00` ⇒ 返回 `None` ⇒ 「无法按当前存储编码解出」✗）。
/// 而 `BlobCodec` trait 在 core 里是**私有**的 ⇒ 所以 server 侧需要这个薄封装。
pub fn encode_for_storage(bytes: &[u8]) -> Vec<u8> {
    BlobCodec::encode(&RenderCodec, bytes)
}

#[cfg(test)]
mod probe_bare_png {
    //! **探针**（第 1235 轮）：`RenderCodec::decode` 对**裸 PNG**（没有 0x00 头）到底返回什么？
    //!
    //! **为什么需要这个探针** ✗：我第 1234 轮想做 `import_image` 的 `blob_decodable`，
    //! 而两次实测都说「裸 PNG 也能解出东西」⇒ 与我读代码的结论（`_ => zlib_decompress`
    //! 应该失败）**不符** ⇒ 所以先在这里**直接问**，不再靠读码猜。
    //!
    //! **诊断三层**（每层单独断言，这样红的时候能指出是哪一层）：
    //!   ① 裸 PNG 的 `decode` 是 `None` 还是 `Some`？
    //!   ② 若是 `Some`，解出来多少字节？前 8 字节是什么？
    //!   ③ 包头（`\x00\x00` + PNG）的 `decode` 是 `Some` 吗？解出来是原 PNG 吗？

    use super::{BlobCodec, RenderCodec};

    /// 一个最小的 8×8 RGBA PNG（纯红），不依赖外部库。
    fn red_png() -> Vec<u8> {
        fn chunk(kind: &[u8], data: &[u8]) -> Vec<u8> {
            let mut out = Vec::new();
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&crc32(&[kind, data].concat()).to_be_bytes());
            out
        }
        fn crc32(buf: &[u8]) -> u32 {
            let mut table = [0u32; 256];
            for n in 0..256u32 {
                let mut c = n;
                for _ in 0..8 {
                    c = if c & 1 != 0 {
                        0xedb8_8320 ^ (c >> 1)
                    } else {
                        c >> 1
                    };
                }
                table[n as usize] = c;
            }
            let mut c = 0xffff_ffffu32;
            for &b in buf {
                c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8);
            }
            c ^ 0xffff_ffff
        }
        let (w, h) = (8u32, 8u32);
        let mut raw = Vec::new();
        for _ in 0..h {
            raw.extend_from_slice(&[0u8]); // filter: none
            for _ in 0..w {
                raw.extend_from_slice(&[200, 40, 40, 255]);
            }
        }
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(b"IDAT", &super::zlib_compress_best(&raw)));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    #[test]
    fn probe_what_decode_does_with_a_bare_png() {
        let codec = RenderCodec;
        let png = red_png();
        let bare = codec.decode(&png);
        println!(
            "  ① 裸 PNG（{} 字节，首字节 0x{:02x}）⇒ decode = {:?}",
            png.len(),
            png[0],
            bare.as_ref().map(|b| format!(
                "Some({} 字节，前 8 = {:02x?})",
                b.len(),
                &b[..b.len().min(8)]
            ))
        );
        // **不在这里断言「应该是 None」** —— 那取决于旧格式兼容策略。
        // 本探针的职责是**把事实打出来**，让下一轮据此设计 `blob_decodable`。

        let wrapped = [b"\x00\x00".as_slice(), &png].concat();
        let ok = codec.decode(&wrapped);
        println!(
            "  ② 包头（\\x00\\x00 + PNG，{} 字节）⇒ decode = {:?}",
            wrapped.len(),
            ok.as_ref()
                .map(|b| format!("Some({} 字节，是原 PNG = {})", b.len(), b == &png))
        );
        assert_eq!(
            ok.as_deref(),
            Some(png.as_slice()),
            "包头之后必须能还原出原 PNG"
        );
    }
}
