//! **`below` 缓存不许每次命中都整份复制** ✓（第 686 轮；`bcd5e53b` 那笔的判据）
//!
//! **⇒ 背景（实测）**：缓存账曾显示 `真正复制 93,998 条` 而脏 tile 只有 `7,366` 条 ⇒ **13× 复制** ✗。
//!   根因：LRU 重排需要**自有**的 tile 表，而 `by_sig` 还共享着同一个 `Arc`
//!   ⇒ `try_unwrap` 必然失败 ⇒ **每次命中都整份复制** ✓。修法是重排前先放掉那些共享分组 ✓。
//!
//! **⇒ 为什么在 Rust 里写**：node 侧的判据要 `ffmpeg` 编码 mp4 ✗ ⇒ **CI 上没有** ⇒ 会 `exit(3)` 跳过 ✗
//!   （第 672 轮记的账 ✓）。`Timeline::frame()` **不需要编码器** ⇒ 这里能在 CI 上真跑 ✓。
//!
//! **⇒ 判据**：多帧重放之后，`真正复制` 的 tile 条数必须**远小于**脏 tile 总数（`≤ 1/10` ✓）。
//! **⇒ 变异**：把重排前的 `by_sig.clear()` 去掉（回到 `bcd5e53b` 之前）⇒ 每帧整份复制 ⇒ 红 ✓。
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
fn the_below_cache_does_not_copy_the_table_on_every_hit() {
    let (dw, dh) = (1024u32, 1024u32);
    let mut timeline = Timeline::new("below_copies", 128, dw, dh, 512 * 1024 * 1024).unwrap();
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
                "doc_id": "below_copies",
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
    yanshi_render::render::reset_tiles_actually_copied();
    for _ in 0..4 {
        let _ = timeline.frame().unwrap();
    }
    let copied = yanshi_render::render::tiles_actually_copied();
    let (_, _, dirty_tiles_u64, _) = timeline.stats();
    let dirty_tiles = usize::try_from(dirty_tiles_u64).unwrap_or(usize::MAX);
    assert!(
        dirty_tiles > 10,
        "脏 tile 只有 {dirty_tiles} 条 ⇒ 夹具有毛病"
    );
    assert!(
        copied * 10 <= dirty_tiles,
        "below 缓存**真正复制**了 {copied} 条 tile，而脏 tile 共 {dirty_tiles} 条 ⇒ 又在每次命中整份复制（LRU 重排前没放掉共享引用）"
    );
}
