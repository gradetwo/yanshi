//! **Hokusai 接入的第一条证据** ✓：我们 vendor 的 CC0 笔刷**真的能被它加载并落笔** ✓。
//!
//! **这是政策变更的第一个落地物** ✓（用户裁定：采纳 Hokusai ✓）：
//! 本项目此前**只有一个依赖 `wasm-bindgen`** ✓ ⇒ 现在多了 `hokusai-core` / `hokusai-brush` /
//! `hokusai-tile-mem`（以及它们带的 `thiserror` ✓）✓ ⇒ 这条**必须写进 README** ✓，
//! 否则下一个人会以为"零依赖"还是红线 ✓。
//!
//! **为什么先写这条测试** ✓（内核先行 ✓）：`.myb` 解析 + 笔触引擎是**陌生的外部代码** ✓ ⇒
//! 在把 196 支笔刷接进界面之前 ✓，先用**最小的可证事实**把它钉住：
//! **能解析 ✓、能落笔 ✓、真的在 tile 上留下了像素 ✓**。
//! **它证明不了**的也写清 ✗：这里**不**比较像素值 ✓、**不**证明与 libmypaint 的对齐度 ✗
//!（那是 Hokusai 自己的 188/196 parity 结论 ✓，不是我们测的 ✓）。

use hokusai::tile_mem::MemSurface;
use hokusai::{myb, Brush, BrushState};

/// 取一个 vendored 的 CC0 笔刷 ✓（路径从**仓库里**出发 ✓，不依赖外部 checkout ✓）。
fn brush_json(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/brushes")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()))
}

/// **能解析** ✓：`.myb` 是 JSON ✓，Hokusai 的 `myb` 模块负责读它 ✓。
#[test]
fn a_vendored_mypaint_brush_parses() {
    let json = brush_json("2B_pencil.myb");
    let brush: Brush = myb::from_str(&json).expect("vendored 的 .myb 应当能解析");
    // **往返** ✓：写回去还能再读回来 ✓（Hokusai 自称 round-trip safe ✓ ⇒ 这里只验"能读回"✓）。
    let again = myb::to_string_pretty(&brush).expect("应当能再序列化");
    let _: Brush = myb::from_str(&again).expect("序列化结果应当仍能解析");
}

/// **能落笔、并且真的落在 tile 上** ✓ —— 这是"引擎接上了"的最小可证事实 ✓。
#[test]
fn a_vendored_brush_actually_paints_onto_a_surface() {
    let json = brush_json("2B_pencil.myb");
    let brush: Brush = myb::from_str(&json).expect("应当能解析");
    let mut state = BrushState::default();
    let mut surface = MemSurface::new();
    // 第一笔只播种位置 ✓（Hokusai 的语义 ✓），后续每笔才产生 dab ✓。
    let mut pressure = 0.0_f32;
    for step in 0..24 {
        let x = 30.0 + step as f32 * 6.0;
        pressure = (pressure + 0.04).min(1.0);
        brush.stroke_to(&mut state, &mut surface, x, 40.0, pressure, 0.0, 0.0, 0.02);
    }
    assert!(
        surface.tile_count() > 0,
        "落笔之后应当至少有一张 64×64 的 tile 被写过 ⇒ 否则说明引擎没接上 ✗"
    );
    assert!(
        surface.tile(0, 0).is_some(),
        "笔触在 (30..180, 40) ⇒ 应当落在 tile (0,0) 上"
    );
}
