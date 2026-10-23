//! **`below` 缓存不许每渲一次就整份拷贝** ✓（第 673 轮；那笔 27 倍提速的 CI 侧判据）
//!
//! **⇒ 为什么要在 Rust 里写** ✗：同一件事我已有一条 **node** 判据
//!   （`scripts/check-timelapse-below-cache-clones.mjs` ✓），但它需要 `ffmpeg` 编码 mp4 ✗
//!   ⇒ **CI 上没有 ffmpeg** ⇒ 它按仓规 `exit(3)` **跳过** ⇒ **CI 上一分覆盖都没有** ✗（第 672 轮记的账 ✓）。
//!   **⇒ 而** `Timeline::frame()` **不需要编码器** ✓ ⇒ 在这里量就能**在 CI 上真跑** ✓。
//!
//! **⇒ 判据**：多帧重放之后，`below` 缓存**被克隆的 tile 条数**必须
//!   **远小于**脏 tile 总数（`克隆 * 10 <= 脏 tile` ✓）。
//! **⇒ 变异**：把 `by_sig` 的插入改回**无条件**（第 666 轮之前那样）⇒ 每渲一次拷一份 ⇒ 红 ✓。
use serde_json::{json, Value};
use yanshi_timelapse::Timeline;

fn blob_hash(bytes: &[u8]) -> String {
    yanshi_core::BlobHash::from_bytes(bytes).as_str().to_owned()
}

fn solid(w: u32, h: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity((w as usize) * (h as usize) * 4);
    for _ in 0..(w as usize) * (h as usize) {
        out.extend_from_slice(&rgba);
    }
    out
}

fn atom(id: &str, seq: u64, kind: &str, payload: Value) -> String {
    json!({
        "id": id,
        "seq": seq,
        "kind": kind,
        "actor": "human:test",
        "session": "session:test",
        "timestamp": 0,
        "schema_version": 1,
        "payload": payload,
    })
    .to_string()
}

#[test]
fn the_below_cache_is_not_copied_once_per_render() {
    let (dw, dh) = (1024u32, 1024u32);
    let tile = 128u32;
    let mut timeline = Timeline::new("below_cache", tile, dw, dh, 512 * 1024 * 1024).unwrap();

    let patch = solid(128, 128, [20, 180, 40, 255]);
    let hash = blob_hash(&patch);
    timeline.blob_put(&patch).unwrap();

    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({
                "background": {"r": 255, "g": 255, "b": 255, "a": 255},
                "color_space": "srgb",
                "doc_id": "below_cache",
                "width": dw,
                "height": dh,
            }),
        ))
        .unwrap();
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .unwrap();

    // **6 张散布的图 ⇒ 脏 tile 够多（> 10 ✓）**
    let spots = [
        (40, 40),
        (520, 60),
        (60, 520),
        (540, 540),
        (260, 260),
        (700, 300),
    ];
    for (index, (x, y)) in spots.iter().enumerate() {
        timeline
            .apply(&atom(
                &format!("img_{index}"),
                3 + index as u64,
                "import_image",
                json!({
                    "layer_id": "L1",
                    "object_id": format!("img{index}"),
                    "region": {"x": *x as f64, "y": *y as f64, "w": 128.0, "h": 128.0},
                    "width": 128,
                    "height": 128,
                    "bitmap": {"blob_hash": hash, "mime_type": "image/x-yanshi-raw", "size": patch.len()},
                }),
            ))
            .unwrap();
    }

    yanshi_render::render::reset_below_cache_tiles_cloned();
    // **逐 atom 出一帧**（每帧都会走一次区域渲染 ⇒ 每帧都可能去写 `below` 缓存 ✓）
    for _ in 0..4 {
        let _ = timeline.frame().unwrap();
    }
    let cloned = yanshi_render::render::below_cache_tiles_cloned();
    let (_, _, dirty_tiles_u64, _) = timeline.stats();
    let dirty_tiles = usize::try_from(dirty_tiles_u64).unwrap_or(usize::MAX);
    assert!(
        dirty_tiles > 10,
        "脏 tile 只有 {dirty_tiles} 条 ⇒ 夹具有毛病，测不出'每渲一次拷一份'"
    );
    assert!(
        cloned * 10 <= dirty_tiles,
        "below 缓存克隆了 {cloned} 条 tile，而脏 tile 共 {dirty_tiles} 条 ⇒ 又在每渲一次整份拷贝（第 666 轮那个 bug 回来了）"
    );
}
