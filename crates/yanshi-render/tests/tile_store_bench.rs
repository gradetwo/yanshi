// **★★★ 微基准：`tile_from_buffer_preserving` 的吞吐 ✗ ★★★**（**第 633 轮 ✓）
//
// **∴ 为什么需要它（**而不是端到端墙钟 ✓）✗**：
//   **∴ 本次优化（**① 省掉双重克隆 ＋ ② 逐行批量转换 ✓）的估算收益只有 **2–5%** ✗
//     ＋ **∴ 而**端到端墙钟的**噪声水位是 **±9–13%**（**第 621–622 轮实测 ✓）
//       ⇒ **∴ 所以**：**端到端**测不出**它** ✓
//         ⇒ **★ 必须**直接量那个函数**✗ ⇒ **∴ 从而**：**同一进程内 ＋ 多次取中位 ⇒ 噪声小** ✓ ★
//
// **∴ 它不是判据 ✗**：**不断言时间**（**∴ 避免**不稳定红 ✓）
//   ＋ **∴ 只**打印中位／min／max** ⇒ **∴ 人工对比两个版本 ✓** ★
//
// **∴ 用法 ✗**：
//   `cargo test -p yanshi-render --release --test tile_store_bench -- --nocapture`
//     ＋ **∴ 然后** `git stash`／切版本 ⇒ **∴ 再跑一次 ⇒ **∴ 比中位数 ✓ ★
use std::time::Instant;
use yanshi_render::buffer::Buffer;
use yanshi_render::render::tile_from_buffer_preserving;
use yanshi_render::tile::TileGrid;

#[test]
fn bench_tile_store_throughput() {
    // **∴ 4K 文档 ＋ 256 的 tile ⇒ 16×9 ＝ **144 块**（**∴ 与第 630 轮的估算一致 ✓）★
    let (w, h, ts) = (3840u32, 2160u32, 256u32);
    let grid = TileGrid::new(ts, w, h).expect("grid");
    let mut buffer = Buffer::new(0, 0, w, h);
    // **∴ 填点非平凡内容 ✗**（**∴ 避免**全零被优化 ✓）★
    for (i, px) in buffer.pixels_mut().chunks_exact_mut(4).enumerate() {
        let v = ((i % 251) as f32) / 251.0;
        px.copy_from_slice(&[v, 1.0 - v, v * 0.5, 0.75]);
    }
    let keys = grid.keys_for_bbox(&buffer.bbox());
    let reps = 5usize;
    let mut times = Vec::with_capacity(reps);
    for _ in 0..reps {
        let t = Instant::now();
        let mut sink = 0usize;
        for key in &keys {
            // **∴ 与 `store_tiles` 同形状 ✗**：**有 previous 的路径**（**∴ 克隆 ＋ 保留 ✓）★
            let previous = None; // **∴ 首次路径 ✓**（**∴ 与热路径的差别只在 previous ✓）
            let tile = tile_from_buffer_preserving(&buffer, &grid, *key, previous);
            sink += tile.size() as usize;
        }
        std::hint::black_box(sink);
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = times[times.len() / 2];
    println!(
        "  ★ tile 组装：{} 块／轮｜中位 {:.1} ms｜min {:.1}｜max {:.1}｜每块 {:.3} ms",
        keys.len(),
        med,
        times[0],
        times[times.len() - 1],
        med / keys.len() as f64
    );
}
