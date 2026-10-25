//! **E2：段重放的帧必须与逐原子重放逐像素一致**（设计文档 §7-2）。
//!
//! **∴ 为什么这是段化的命门 ✗**：段化只许省"每原子一次的全状态深拷贝"，
//!   不许改任何一帧的像素——脏区并集少一个 tile、回退/切分路走错一步，
//!   帧就会悄悄丢内容 ✗ ⇒ 所以同包双路跑（段路 vs 原子路），每帧逐字节比 ✓
//!
//! **∴ 变异（能红的根据 ✓，2026-10-26 实测）**：
//!   ＋ 把 `apply_segment` 里脏区并集的累积改成"只记最后一个 atom" ⇒
//!     第 2 帧起丢 tile ⇒ 本文件第一条红 ✓（前提：包带
//!     `"type": "raster_patch"`，否则脏区退化成整层、变异测不出来——
//!     2026-10-26 曾因此假绿，见 `brush_package` 的注释）
//!   ＋ 把 `segment_needs_atomic_path` 的判定恒取假 ⇒ 帧臂**不**红（快路
//!     的 `fold` 续折 + `plan_dirty_with_log` 递归 revert 目标已能正确
//!     处理；回退是纵深防御）⇒ 但 revert／declare_head 两臂的**计数臂**红
//!     （增量步数从 ≈250 跌到 2）✓
//!   ＋ 把 `fold_counters` 的 `incremental_steps` 恒报 0 ⇒ 计数臂红 ✓

use serde_json::{json, Value};
use yanshi_timelapse::Timeline;

fn blob_hash(bytes: &[u8]) -> String {
    yanshi_core::BlobHash::from_bytes(bytes).as_str().to_owned()
}

/// **一块纯色 RGBA ✗**（够用来验位置，不需要图案 ✓）。
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

/// **确定性伪随机**（无外部依赖的 LCG；同一 seed 同一包 ✓）。
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state >> 33
}

/// **N 笔纯色补丁包**：`create_document` + `create_layer` + N 个
/// `import_image`（48×48，位置/颜色由 LCG 定，与 E2 基线包同形）。
/// 返回（原子行，{颜色 → 位图}）。
fn brush_package(n: usize) -> (Vec<String>, Vec<(String, Vec<u8>)>) {
    let (dw, dh) = (512u32, 512u32);
    let palette: [[u8; 4]; 4] = [
        [220, 40, 40, 255],
        [40, 40, 220, 255],
        [40, 200, 80, 255],
        [230, 200, 40, 255],
    ];
    let mut blobs: Vec<(String, Vec<u8>)> = Vec::new();
    for rgba in palette {
        let px = solid(48, 48, rgba);
        blobs.push((blob_hash(&px), px));
    }
    let mut atoms = vec![
        atom(
            "a0",
            1,
            "create_document",
            json!({
                "background": {"r": 255, "g": 255, "b": 255, "a": 255},
                "color_space": "srgb",
                "doc_id": "tl_seg",
                "width": dw,
                "height": dh,
            }),
        ),
        atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ),
    ];
    let mut rng = 20261026u64;
    for i in 0..n {
        let (x, y) = ((lcg(&mut rng) % 465) as f64, (lcg(&mut rng) % 465) as f64);
        let color = (lcg(&mut rng) % 4) as usize;
        let (hash, _) = &blobs[color];
        atoms.push(atom(
            &format!("p{i}"),
            (i + 3) as u64,
            "import_image",
            json!({
                // **`"type": "raster_patch"` 不可省**：缺了它折叠层把对象当默认
                // `Stroke`（`fold.rs` 的 `parse_object_type` 兜底），`parse_stroke`
                // 得不到点 ⇒ 包围盒未知 ⇒ 脏区退化成整层 ⇒ 段路脏区并集的变异
                // （只记最后一个 atom）测不出来（实测：单原子脏区即整幅 16 tile）。
                // 真实包（`gen_e2_pkg.py`）带这个字段，测试包必须同形。
                "type": "raster_patch",
                "layer_id": "L1",
                "object_id": format!("o{i}"),
                "region": {"x": x, "y": y, "w": 48.0, "h": 48.0},
                "width": 48,
                "height": 48,
                "bitmap": {"blob_hash": hash, "mime_type": "image/x-yanshi-raw", "size": 48 * 48 * 4},
            }),
        ));
    }
    (atoms, blobs)
}

fn build(doc: &str, blobs: &[(String, Vec<u8>)]) -> Timeline {
    let mut t = Timeline::new(doc, 128, 512, 512, 256 * 1024 * 1024).expect("建 Timeline");
    for (_, bytes) in blobs {
        t.blob_put(bytes).expect("写 blob");
    }
    t
}

/// **折叠器计数**（`fold_counters` 的四元组：增量／续折／全量／重放原子数）。
type FoldCounters = (usize, usize, usize, usize);

/// **双路跑**：原子路（逐个 `apply`）与段路（按 `segment` 切段调
/// `apply_segment`），在每个出帧边界都出一帧，返回两路的帧序列与段路的
/// 折叠器计数（供回退臂断言“回退确被执行”）。
fn run_both(
    atoms: &[String],
    blobs: &[(String, Vec<u8>)],
    segment: usize,
    frame_every: usize,
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>, FoldCounters) {
    let mut atomic = build("tl_seg_atomic", blobs);
    let mut frames_atomic: Vec<Vec<u8>> = Vec::new();
    for (i, line) in atoms.iter().enumerate() {
        atomic.apply(line).expect("原子路 apply");
        if (i + 1) % frame_every == 0 {
            frames_atomic.push(atomic.frame().expect("原子路出帧").to_vec());
        }
    }
    frames_atomic.push(atomic.frame().expect("原子路尾帧").to_vec());

    let mut segmented = build("tl_seg_segmented", blobs);
    let mut frames_segmented: Vec<Vec<u8>> = Vec::new();
    let mut consumed = 0usize;
    for chunk in atoms.chunks(segment) {
        let lines: Vec<String> = chunk.to_vec();
        segmented.apply_segment(&lines).expect("段路 apply_segment");
        consumed += chunk.len();
        if consumed % frame_every == 0 {
            frames_segmented.push(segmented.frame().expect("段路出帧").to_vec());
        }
    }
    frames_segmented.push(segmented.frame().expect("段路尾帧").to_vec());
    let counters = segmented.fold_counters();
    (frames_atomic, frames_segmented, counters)
}

/// **段路帧与原子路帧逐像素一致**（600 笔、段长 250、出帧每 250）。
#[test]
fn segment_replay_matches_atomic_frame_for_frame() {
    let (atoms, blobs) = brush_package(600);
    let (frames_atomic, frames_segmented, _counters) = run_both(&atoms, &blobs, 250, 250);
    assert_eq!(
        frames_atomic.len(),
        frames_segmented.len(),
        "帧数必须相同（原子路 {} 帧、段路 {} 帧）",
        frames_atomic.len(),
        frames_segmented.len()
    );
    for (index, (a, b)) in frames_atomic
        .iter()
        .zip(frames_segmented.iter())
        .enumerate()
    {
        assert_eq!(a, b, "第 {index} 帧必须逐字节相同");
    }
}

/// **含 revert 的段走回退路，帧仍须一致**：第 300 笔被 revert（目标在段首
/// 之前），该段回退逐原子——帧不能因此不同。
#[test]
fn segment_with_revert_falls_back_and_still_matches() {
    let (mut atoms, blobs) = brush_package(600);
    // revert 放在第 301 个原子（seq 301，目标是第 300 笔 `p297`）：
    // 它落在第 2 段（251..500）里 ⇒ 该段整段回退逐原子。
    let target = "p297";
    atoms.insert(300, atom("r0", 301, "revert", json!({"target": target})));
    // 插入后 seq 要重排（`apply_atom` 要求连续 seq）。
    let atoms: Vec<String> = atoms
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let mut value: Value = serde_json::from_str(line).expect("原子行是合法 JSON");
            value["seq"] = json!(i + 1);
            value.to_string()
        })
        .collect();
    let (frames_atomic, frames_segmented, counters) = run_both(&atoms, &blobs, 250, 250);
    assert_eq!(frames_atomic.len(), frames_segmented.len());
    for (index, (a, b)) in frames_atomic
        .iter()
        .zip(frames_segmented.iter())
        .enumerate()
    {
        assert_eq!(a, b, "含 revert 的包：第 {index} 帧必须逐字节相同");
    }
    // **回退确被执行**：含 revert 的段（250 个原子）整段回退逐原子 ⇒
    // 增量步数应为段内原子数级（≈250），而不是快路的每段 1 次；若判定被
    // 恒取假（快路），增量步数只有 2 ⇒ 这条红。
    let (inc, _reb, _full, _replayed) = counters;
    assert!(
        inc > 100,
        "含 revert 的段必须走逐原子回退（增量步数应 >100，实测 {inc}）"
    );
}

/// **含 declare_head 的段走切分路，帧仍须一致**：`declare_head` 以第 100
/// 笔为新起点（`revert_to` 语义），该段整段回退逐原子。
#[test]
fn segment_with_declare_head_falls_back_and_still_matches() {
    let (mut atoms, blobs) = brush_package(600);
    atoms.insert(
        300,
        atom(
            "h0",
            301,
            "declare_head",
            json!({"base": {"type": "atom", "id": "p99"}, "reason": "test"}),
        ),
    );
    let atoms: Vec<String> = atoms
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let mut value: Value = serde_json::from_str(line).expect("原子行是合法 JSON");
            value["seq"] = json!(i + 1);
            value.to_string()
        })
        .collect();
    let (frames_atomic, frames_segmented, counters) = run_both(&atoms, &blobs, 250, 250);
    assert_eq!(frames_atomic.len(), frames_segmented.len());
    for (index, (a, b)) in frames_atomic
        .iter()
        .zip(frames_segmented.iter())
        .enumerate()
    {
        assert_eq!(a, b, "含 declare_head 的包：第 {index} 帧必须逐字节相同");
    }
    // **切分确被执行**：同 revert 臂（250 原子的段回退逐原子 ⇒ 增量步数 >100）。
    let (inc, _reb, _full, _replayed) = counters;
    assert!(
        inc > 100,
        "含 declare_head 的段必须走逐原子回退（增量步数应 >100，实测 {inc}）"
    );
}

/// **仪器自证**：同包同原子数，段长 250 时 `incremental_steps` 为段数级、
/// 段长 1 时退回 N 量级；两路 `replayed_atoms` 都 ≈ N、`full_steps` 都 = 1。
#[test]
fn fold_counters_prove_segmenting_is_what_changed_the_cost() {
    let (atoms, blobs) = brush_package(600);
    let n = atoms.len();

    let mut wide = build("tl_seg_wide", &blobs);
    for chunk in atoms.chunks(250) {
        wide.apply_segment(chunk).expect("段路 apply_segment");
    }
    let (inc_wide, _reb_wide, full_wide, replayed_wide) = wide.fold_counters();

    let mut narrow = build("tl_seg_narrow", &blobs);
    for line in &atoms {
        narrow
            .apply_segment(std::slice::from_ref(line))
            .expect("段长 1 apply_segment");
    }
    let (inc_narrow, _reb_narrow, full_narrow, replayed_narrow) = narrow.fold_counters();

    assert_eq!(
        replayed_wide, n,
        "段路重放原子数必须 ≈ N（实测 {replayed_wide}，N={n}）"
    );
    assert_eq!(
        replayed_narrow, n,
        "段长 1 重放原子数也必须 ≈ N（实测 {replayed_narrow}）"
    );
    assert_eq!(
        full_wide, 1,
        "段路全量折叠必须恰好 1 次（实测 {full_wide}）"
    );
    assert_eq!(
        full_narrow, 1,
        "段长 1 全量折叠也必须恰好 1 次（实测 {full_narrow}）"
    );
    assert_eq!(
        inc_wide, 2,
        "段长 250 时增量步数应为段数-1（实测 {inc_wide}）"
    );
    assert_eq!(
        inc_narrow,
        n - 1,
        "段长 1 时增量步数应退回 N-1（实测 {inc_narrow}，N={n}）"
    );
    assert!(
        inc_narrow > inc_wide * 100,
        "段长 1 的增量步数（{inc_narrow}）必须比段长 250（{inc_wide}）大两个数量级"
    );
}
