//! **运行时字节比对的"原生"一侧** —— 为 `scripts/wasm-smoke.sh` 的同一份输入算 SHA-256。
//!
//! 为什么需要它：wasm 与原生"同一份代码、不同目标"只是**构造性论证** ✓，
//! 不是**测量** ✗。`scripts/wasm-smoke.sh` 原来只看长度与"没 panic" ✗ ⇒
//! 像素内容悄悄变了（量化、查表、滤镜、编码器）脚本照样绿 ✓。
//! 这里让原生侧在同一份场景文件上真跑一遍，把两个摘要按可解析的格式打到 stdout ✓，
//! 由脚本抓取并与 node 里的 wasm 摘要**逐字符比较** ✓。
//!
//! **不写死期望值** ✓：本文件只**产出**摘要 ✓，不知道"正确"的摘要是什么 ✓
//! —— 期望值由这次运行的原生侧现算 ✓（硬编码一份观测值等于把"某次的样子"当成判据 ✗）。
//! 测试自身断言的是**确定性**（同一输入两次渲染逐字节相同 ✓），不是某个常数 ✓。
//!
//! 输入只有一份 ✓：`tests/data/wasm_smoke_scene.json`（脚本通过 `YANSHI_WASM_PARITY_SCENE`
//! 把**同一个路径**同时交给 node 与这里 ✓，避免"两份输入各自漂移" ✗）。

use std::path::PathBuf;

use serde_json::Value;
use yanshi_core::{Bbox, BlobHash};
use yanshi_wasm::Kernel;

/// 摘要行前缀（`scripts/wasm-smoke.sh` 按它抓取 ✓；改动这里脚本会以
/// "取不到原生摘要"失败 ✓，而不是静默跳过 ✓）。
const RGBA_MARKER: &str = "YANSHI_PARITY_NATIVE_RGBA=";
/// PNG 摘要行前缀。
const PNG_MARKER: &str = "YANSHI_PARITY_NATIVE_PNG=";

/// 场景文件：脚本用环境变量指定**同一个路径** ✓；单独跑 `cargo test` 时退回固定夹具 ✓。
fn scene_path() -> PathBuf {
    std::env::var("YANSHI_WASM_PARITY_SCENE").map_or_else(
        |_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/wasm_smoke_scene.json"),
        PathBuf::from,
    )
}

/// 在同一份场景上渲染，打印 RGBA/PNG 的 sha256（十六进制小写，与 node `createHash` 同口径）。
#[test]
fn native_digests_for_the_shared_smoke_scene() {
    let path = scene_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读取共用场景 {} 失败：{error}", path.display()));
    let scene: Value = serde_json::from_str(&text).expect("共用场景必须是 JSON 对象");

    let doc_id = scene["doc_id"].as_str().expect("场景缺少 doc_id");
    let tile_size = scene["tile_size"].as_u64().expect("场景缺少 tile_size") as u32;
    let width = scene["width"].as_u64().expect("场景缺少 width") as u32;
    let height = scene["height"].as_u64().expect("场景缺少 height") as u32;
    let memory_limit = scene["memory_limit"]
        .as_u64()
        .expect("场景缺少 memory_limit") as usize;
    let render = &scene["render"];
    let x = render["x"].as_f64().expect("场景缺少 render.x");
    let y = render["y"].as_f64().expect("场景缺少 render.y");
    let w = render["w"].as_f64().expect("场景缺少 render.w");
    let h = render["h"].as_f64().expect("场景缺少 render.h");

    // **与脚本文本完全同序的构造** ✓：新建 → 整串原子一次装载 → 同一块区域渲染 ✓。
    let mut kernel = Kernel::new(doc_id, tile_size, width, height, memory_limit)
        .expect("构造内核失败（场景参数非法）");
    kernel
        .load_atoms_json(&scene["atoms"].to_string())
        .expect("装载共用场景的原子失败");

    let bbox = Bbox::new(x, y, w, h);
    let first = kernel.render_region(bbox).expect("区域渲染失败");
    assert_eq!(
        first.rgba8.len(),
        (w as usize) * (h as usize) * 4,
        "RGBA 长度与请求区域不符"
    );

    // **确定性自检** ✓：同一输入两次渲染必须逐字节相同 ✓（不是黄金常数 ✓）。
    let second = kernel.render_region(bbox).expect("第二次区域渲染失败");
    assert_eq!(
        first.rgba8, second.rgba8,
        "同一输入两次渲染的 RGBA 必须逐字节相同"
    );
    assert_eq!(
        first.png, second.png,
        "同一输入两次渲染的 PNG 必须逐字节相同"
    );
    assert_eq!(&first.png[0..8], b"\x89PNG\r\n\x1a\n", "PNG 魔数不符");

    println!("{RGBA_MARKER}{}", BlobHash::from_bytes(&first.rgba8).hex());
    println!("{PNG_MARKER}{}", BlobHash::from_bytes(&first.png).hex());
}
