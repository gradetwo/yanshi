//! **原生一侧的笔刷字节**（第 245 轮）：判定"wasm 与 x86_64 是否逐字节一致"。
//!
//! **为什么需要它**：`scripts/wasm-smoke.sh` 的对照走**原子渲染**，不含 `.myb` 笔刷
//! ⇒ 笔刷路径上"两个目标是否一致"**没有**判据 ✓。而 9 支带随机的笔在 wasm 与服务端
//! 之间不一致（40 条真差异 ✓），候选只剩"编译目标不同 ⇒ f32 舍入不同" ✓。
//!
//! **它只产出字节** ✓：写到 `YANSHI_NATIVE_BRUSH_OUT`（缺省 /tmp/native_brush.bin）。
//! 期望值由**外部**（服务端报的 blob）提供 ⇒ 这里**不写死**任何观测值 ✓。
use std::path::PathBuf;

#[test]
fn native_brush_region_bytes() {
    let brush = PathBuf::from("../../assets/brushes/8B_Pencil#1.myb");
    let myb = std::fs::read_to_string(&brush)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", brush.display()));
    // 与判据完全相同的输入（pointList 与 size 见 scripts/kernel-brush-parity.mjs:37-38）。
    let request = serde_json::json!({
        "myb": myb,
        "points": [[40.0, 40.0, 1.0], [80.0, 40.0, 1.0]],
        "size": 40.0,
        // **用判据自己的 red**（scripts/kernel-brush-parity.mjs:42 的 colours[0]）
        // ⇒ 才能与"wasm vs 服务端 = 5757"这个数直接比较 ✓。
        "color": { "r": 255, "g": 0, "b": 0, "a": 255 },
        "opacity": null,
        "hardness": null,
        // 与服务端**实测报出**的 region 一致（第 246 轮实测：x=16 y=16 w=88 h=48）。
        "region": { "x": 16, "y": 16, "w": 88, "h": 48 },
    });
    let bytes = yanshi_wasm::paint_brush_bytes_for_test(&request.to_string())
        .expect("paint_brush 返回空 ⇒ 画不出来");
    assert_eq!(bytes.len(), 88 * 48 * 4, "字节数不符");
    let out = std::env::var("YANSHI_NATIVE_BRUSH_OUT")
        .map_or_else(|_| PathBuf::from("/tmp/native_brush.bin"), PathBuf::from);
    std::fs::write(&out, &bytes).expect("写不出文件");
    println!("NATIVE_BRUSH bytes={} out={}", bytes.len(), out.display());
}
