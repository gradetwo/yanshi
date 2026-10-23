//! **★★★ 增量帧必须与"从零整幅重渲"**逐字节一致**✗ ★★★**（**第 249 轮 ✓）
//!
//! **∴ 为什么这是这次优化的**命门** ✗**：**增量只做两件事 ✗**：
//!   ＋ **∴ ① 只渲**脏 tile 并集的 bbox**✗** ＋ **∴ ② 把它 **blit** 进持久帧缓冲** ✓
//!     ⇒ **∴ 而**这两步**任何一步错**（**偏移差 1 ✗ tile→像素换算错 ✗ 忘了并集 ✓）
//!       都**不会崩**✗ ⇒ **∴ 只会**画错** ✓ ⇒ **∴ 所以**必须有一条**逐字节**的判据 ✓ ★★
//!
//! **∴ 判据怎么写 ✗**：**每吃一个 atom ⇒ **增量出一帧**✗ 然后**从零重放同样多的
//!   atom**✗ **丢掉脏区那一次 ✗ 再出一次（**此时没有脏信息 ⇒ **整幅 ✓）★
//!     ⇒ **∴ 两张图**必须逐字节相同** ✓（**∴ 不一致就报第几帧 ✗ 便于定位 ✓）★★
//!
//! **∴ 变异（**能红的根据 ✓）✗**：**把 `blit` 里的 `dx` 改成 `dx + 1`**✗
//!   ⇒ **∴ 第二次 blit 会**错一格**⇒ **∴ 判据**必红** ✓
//!     ＋ **∴ 而**把"并集"退回"只记最后一个 atom"**✗ ⇒ **∴ 也会红** ✓ ★★
//!
//! **∴ 为什么用 `import_image` 造夹具 ✗**：**它的 blob 是**裸 RGBA**
//!   （`mime_type = image/x-yanshi-raw` ✓）⇒ **∴ 判据**不用编解码器**✗
//!     ⇒ **∴ 于是**它**只测"脏区 ＋ blit"这件事** ✓ ★

use serde_json::{json, Value};
use yanshi_timelapse::Timeline;

fn blob_hash(bytes: &[u8]) -> String {
    yanshi_core::BlobHash::from_bytes(bytes).as_str().to_owned()
}

/// **一块纯色 RGBA ✗**（**∴ 够用来验位置 ✗ 不需要图案 ✓）★
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
fn incremental_frames_match_a_full_render_byte_for_byte() {
    let (dw, dh) = (512u32, 384u32);
    let tile = 128u32;
    let mem = 256 * 1024 * 1024;

    let red = solid(64, 48, [220, 40, 40, 255]);
    let blue = solid(64, 48, [40, 40, 220, 255]);
    let (h_red, h_blue) = (blob_hash(&red), blob_hash(&blue));

    let atoms = [
        atom(
            "a0",
            1,
            "create_document",
            json!({
                "background": {"r": 255, "g": 255, "b": 255, "a": 255},
                "color_space": "srgb",
                "doc_id": "tl_test",
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
        // **∴ 两块**不重叠**的图 ✗ ⇒ **∴ 两次 blit 都要对 ✓** ★
        atom(
            "a2",
            3,
            "import_image",
            json!({
                "layer_id": "L1",
                "object_id": "o1",
                "region": {"x": 32.0, "y": 24.0, "w": 64.0, "h": 48.0},
                "width": 64,
                "height": 48,
                "bitmap": {"blob_hash": h_red, "mime_type": "image/x-yanshi-raw", "size": 64 * 48 * 4},
            }),
        ),
        atom(
            "a3",
            4,
            "import_image",
            json!({
                "layer_id": "L1",
                "object_id": "o2",
                "region": {"x": 300.0, "y": 200.0, "w": 64.0, "h": 48.0},
                "width": 64,
                "height": 48,
                "bitmap": {"blob_hash": h_blue, "mime_type": "image/x-yanshi-raw", "size": 64 * 48 * 4},
            }),
        ),
    ];

    let build = |doc: &str| -> Timeline {
        let mut t = Timeline::new(doc, tile, dw, dh, mem).expect("建 Timeline");
        t.blob_put(&red).expect("写红块");
        t.blob_put(&blue).expect("写蓝块");
        t
    };

    let mut inc = build("tl_inc");
    for (i, line) in atoms.iter().enumerate() {
        inc.apply(line).expect("增量 apply");
        let got = inc.frame().expect("增量出帧").to_vec();

        // **★★★ 参照必须**绕开 blit** ✗ ★★★**（**∴ 这是我第一版的错 ✗ 它**测不出 blit 的错** ✓）
        //   ＋ **∴ 第一版怎么写的 ✗**：**参照也走 `Timeline`**✗ ⇒ **∴ 而**它**同样经过 `blit`** ✓
        //     ＋ **∴ 于是**：**blit 里错一格 ⇒ **两边一起错**✗ ⇒ **∴ 判据**照样绿** ✓
        //       ＋ **∴ 变异实测 ✗**：**把行偏移 +1 ✗ 判据**仍然 1 passed** ✓ ⇒ **∴ 它**没有牙** ✓
        //         ⇒ **∴ 现在 ✗**：**参照改成**直接问内核**（`Kernel::render_region` ✓）
        //           ⇒ **∴ 那条路**不经过我们的 blit**✗ ⇒ **∴ blit 一错就**必红** ✓ ★★★
        let mut reference =
            yanshi_wasm::Kernel::new("tl_ref", tile, dw, dh, mem).expect("建参照内核");
        reference.blob_put(&red).expect("参照写红块");
        reference.blob_put(&blue).expect("参照写蓝块");
        for line in &atoms[..=i] {
            reference.apply_atom_json(line).expect("参照 apply");
        }
        let want = reference
            .render_region(yanshi_core::Bbox::new(
                0.0,
                0.0,
                f64::from(dw),
                f64::from(dh),
            ))
            .expect("参照整幅渲染")
            .rgba8;

        assert_eq!(
            got.len(),
            want.len(),
            "第 {i} 帧尺寸不同：{} vs {}",
            got.len(),
            want.len()
        );
        // **∴ 逐字节比 ✗ 不一致就报**第一个不同的位置** ✓（**∴ 便于定位 ✗ 而不是只说"不等" ✓）★
        if got != want {
            let at = got.iter().zip(want.iter()).position(|(a, b)| a != b);
            panic!(
                "第 {i} 帧与整幅重渲**不一致**（首个不同字节 {}）：增量 {:?} vs 整幅 {:?}",
                at.map_or("无（长度差）".to_owned(), |p| p.to_string()),
                at.map(|p| &got[p.saturating_sub(2)..(p + 3).min(got.len())]),
                at.map(|p| &want[p.saturating_sub(2)..(p + 3).min(want.len())]),
            );
        }
    }
}

/// **判据**：`frame_rgb_scaled` 必须真的产出一帧（不能全零），且与「整幅缩小」逐字节相同。
///
/// **为什么要有它**（外部第 86 次实测）：`f3bcde39` 把帧管线切到"只重算脏区"之后，
/// `yanshi-timelapse` 跑完 **exit 0**，但 mp4 只有 **261 字节**、`ffprobe` 读不出流 ⇒
/// **产出空帧却报成功**。那条路径当时只有纯函数判据（部分 == 整幅），**缺这一条**。
///
/// **变异**：把 `frame_rgb_scaled` 里首次的 `last_dirty_px = None` 改成 `Some((0, 0, 0, 0))`
/// ⇒ 一个输出像素都不写 ⇒ 全零 ⇒ 红。
#[test]
fn scaled_frame_is_never_empty() {
    let (dw, dh) = (64u32, 64u32);
    let mut timeline = Timeline::new("tl_scaled", 64, dw, dh, 256 * 1024 * 1024).expect("kernel");
    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({
                "background": {"r": 255, "g": 255, "b": 255, "a": 255},
                "color_space": "srgb",
                "doc_id": "tl_scaled",
                "width": dw,
                "height": dh,
            }),
        ))
        .expect("create_document");
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .expect("create_layer");

    let scaled = timeline
        .frame_rgb_scaled(16, 16)
        .expect("frame_rgb_scaled")
        .to_vec();
    assert_eq!(scaled.len(), 16 * 16 * 3, "尺寸必须是 out_w*out_h*3");
    assert!(
        scaled.iter().any(|byte| *byte != 0),
        "缩小后的帧不能全零（全零 ⇒ ffmpeg 收到空流 ⇒ mp4 只剩头，外部第 86 次就是这么坏的）"
    );

    let frame = timeline.frame().expect("frame").to_vec();
    let mut full = vec![0u8; 16 * 16 * 3];
    yanshi_timelapse::scale_rgb_region(&frame, (dw, dh), &mut full, (16, 16), (0, 16), (0, 16));
    assert_eq!(scaled, full, "增量缩小必须与整幅缩小逐字节相同");
}

/// **新增一个小矩形 ⇒ 这一帧的脏区必须远小于整幅**（第 580 轮，实测驱动）。
///
/// **为什么**：我一度用**键名写错的合成包**（`"kind": "shape"` 而不是 `"type": "shape"`）测出
/// 「每帧 768 tile = 整幅」，于是误判"脏区机制坏了" ✗ —— 改正键名后**这条判据立刻绿** ✓
/// ⇒ 现在它守的是：**别再把"我的测试数据写错"当成产品缺陷** ✓。
///
/// **变异**：`plan_dirty` 对 `CreateObject` 退回 `dirty_for_layer`／`DirtySet::full` ⇒ 整幅 ⇒ 红。
#[test]
fn a_new_small_object_does_not_dirty_the_whole_canvas() {
    let (dw, dh) = (256u32, 256u32);
    let mut timeline = Timeline::new("tl_dirty", 64, dw, dh, 256 * 1024 * 1024).expect("kernel");
    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({"background": {"r": 255, "g": 255, "b": 255, "a": 255},
                   "color_space": "srgb", "doc_id": "tl_dirty", "width": dw, "height": dh}),
        ))
        .expect("create_document");
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .expect("create_layer");
    let _ = timeline.frame();
    let before = timeline.stats().2;
    timeline
        .apply(&atom(
            "a2",
            3,
            "create_object",
            json!({"object_id": "o1", "layer_id": "L1", "type": "shape",
                   "data": {"geometry": {"kind": "rect",
                                         "bbox": {"x": 40.0, "y": 40.0, "w": 24.0, "h": 24.0}},
                            "color": {"r": 10, "g": 200, "b": 30, "a": 255}}}),
        ))
        .expect("create_object");
    let _ = timeline.frame();
    let after = timeline.stats().2;
    let delta = after.saturating_sub(before);
    // 256×256、tile 64 ⇒ 全画布 16 个 tile；一个 24×24 的矩形最多碰到 4 个 tile
    assert!(
        delta <= 4,
        "只新增一个 24×24 的矩形 ⇒ 这一帧的脏 tile 必须 ≤ 4，实测 {delta}（整幅是 16）"
    );
}

/// **只建过文档/图层 ⇒ 那一帧必须是背景色，且不算"整幅渲染"**（第 620 轮）。
///
/// **为什么**：`create_document` / `create_layer` 是结构性失效 ⇒ 旧实现直接渲整幅 ✓
/// ⇒ 4608×8192 上**每帧约 27 s** ⇒ 外部第 92 次的"卡死"就是这两帧起步 ✗
/// ⇒ 现在直接按背景色填缓冲 ✓（3072² 的包实测 **25.3 s → 0.6 s** ✓）。
///
/// **变异**：去掉那条快路径 ⇒ `full_renders` 从 0 变 1 ⇒ 红 ✓；
/// 背景色取错（比如恒白）⇒ 像素断言红 ✓。
#[test]
fn blank_frames_are_background_not_full_renders() {
    let (dw, dh) = (64u32, 64u32);
    let mut timeline = Timeline::new("tl_blank", 64, dw, dh, 64 * 1024 * 1024).expect("kernel");
    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({"background": {"r": 10, "g": 20, "b": 30, "a": 255},
                   "color_space": "srgb", "doc_id": "tl_blank", "width": dw, "height": dh}),
        ))
        .expect("create_document");
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .expect("create_layer");
    let frame = timeline.frame().expect("frame");
    assert_eq!(frame.len(), (dw * dh * 4) as usize, "帧缓冲大小不对");
    assert_eq!(
        &frame[..4],
        &[10, 20, 30, 255],
        "空白帧必须是文档的背景色（不是白、也不是没初始化）"
    );
    assert!(
        frame.chunks_exact(4).all(|px| px == [10, 20, 30, 255]),
        "空白帧的每个像素都必须是背景色"
    );
    let (full_renders, dirty_renders, _, _) = timeline.stats();
    assert_eq!(
        full_renders, 0,
        "只建文档/图层 ⇒ 不该发生整幅渲染（4608×8192 上每帧约 27 s）"
    );
    assert_eq!(dirty_renders, 0, "空白帧也不该产生脏区渲染");
}

/// **没有任何变化的帧必须复用上一帧，不许整幅重渲**（第 624 轮）。
///
/// **为什么**：旧实现「空脏区 ⇒ 渲整幅」✗ ⇒ 收尾那一帧白花 **~27 s**（4608×8192 实测）✓
/// 而它和上一帧**逐字节相同** ✓ ⇒ 纯浪费 ✓。
///
/// **变异**：把复用那条去掉 ⇒ `full_renders` 从 1 变 2 ⇒ 红 ✓。
#[test]
fn a_frame_with_no_changes_reuses_the_last_one() {
    let (dw, dh) = (64u32, 64u32);
    let mut timeline = Timeline::new("tl_reuse", 64, dw, dh, 64 * 1024 * 1024).expect("kernel");
    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({"background": {"r": 255, "g": 255, "b": 255, "a": 255},
                   "color_space": "srgb", "doc_id": "tl_reuse", "width": dw, "height": dh}),
        ))
        .expect("create_document");
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .expect("create_layer");
    timeline
        .apply(&atom(
            "a2",
            3,
            "create_object",
            json!({"object_id": "o1", "layer_id": "L1", "type": "shape",
                   "data": {"geometry": {"kind": "rect",
                                         "bbox": {"x": 8.0, "y": 8.0, "w": 16.0, "h": 16.0}},
                            "color": {"r": 10, "g": 200, "b": 30, "a": 255}}}),
        ))
        .expect("create_object");
    let first = timeline.frame().expect("first frame").to_vec();
    let (full_after_first, _, _, _) = timeline.stats();
    let second = timeline.frame().expect("second frame").to_vec();
    let (full_after_second, _, _, _) = timeline.stats();
    assert_eq!(second, first, "没有变化的第二帧必须与第一帧逐字节相同");
    assert_eq!(
        full_after_second, full_after_first,
        "没有变化的帧不该再触发整幅渲染（4608×8192 上一次就是约 27 秒）"
    );
}

/// **擦除之后不许留残影**（第 712 轮；外部审计 R-05）
///
/// **⇒ 缺陷**：`blit` 把 `render_region` 的结果 **`over` 到上一帧**上 ✓
///   而 `render_region` 返回的是**所有可见图层的最终合成** ⇒ 某一区域**变透明**（对象被隐藏／
///   被擦掉）时，`over` 遇到 `a=0` **直接跳过** ✗ ⇒ **旧像素留在帧里** ⇒ 视频里是**残影** ✓
///   （半透明对象每次重渲还会**叠加加深** ✓ —— 同一个根因 ✓）。
///
/// **⇒ 修法**：合成基准从「上一帧」换成 **`self.background`** ✓ —— 渲染结果是"这一区域现在应该长什么样"，
///   所以它该**盖住**旧像素，而不是**叠在**旧像素上 ✓。
///
/// **⇒ 判据**：画一块不透明绿方块 ⇒ 帧中心是绿的 ✓；**隐藏那个图层** ⇒ 再取帧，
///   同一像素必须回到**背景色**（不是绿）✓。**变异**：把基准改回上一帧 ⇒ 红 ✓。
#[test]
fn hiding_a_layer_clears_its_pixels_instead_of_leaving_a_ghost() {
    let (dw, dh) = (64u32, 64u32);
    // **故意不传 `background`** ✓（审计 R-05 的根因之一 ✓）：时间线默认白底，
    //   而**内核**那边是 `null`（渲染出来是**透明**）⇒ 这正是「`over` 遇到 `a=0` 直接跳过」的场景 ✓。

    let mut timeline = Timeline::new("tl_ghost", 32, dw, dh, 64 * 1024 * 1024).expect("kernel");
    timeline
        .apply(&atom(
            "a0",
            1,
            "create_document",
            json!({"color_space": "srgb", "doc_id": "tl_ghost", "width": dw, "height": dh}),
        ))
        .expect("create_document");
    timeline
        .apply(&atom(
            "a1",
            2,
            "create_layer",
            json!({"layer_id": "L1", "name": "L1"}),
        ))
        .expect("create_layer");
    timeline
        .apply(&atom(
            "a2",
            3,
            "create_object",
            json!({"object_id": "o1", "layer_id": "L1", "type": "shape",
                   "data": {"geometry": {"kind": "rect",
                                         "bbox": {"x": 16.0, "y": 16.0, "w": 24.0, "h": 24.0}},
                            "color": {"r": 20, "g": 200, "b": 40, "a": 255}}}),
        ))
        .expect("create_object");
    let painted = {
        let frame = timeline.frame().expect("frame with the green square");
        let at = ((28 * dw + 28) * 4) as usize;
        [frame[at], frame[at + 1], frame[at + 2]]
    };
    assert!(
        painted[1] > painted[0] + 40 && painted[1] > painted[2] + 40,
        "绿方块没画上（实测 {painted:?}）⇒ 判据测不到东西"
    );
    // 擦掉那一块 ⇒ 应该回到背景色（残留 = 视频里的残影 ✓）
    timeline
        .apply(&atom(
            "a3",
            4,
            "erase",
            json!({"object_id": "erase1", "layer_id": "L1", "type": "retouch",
                   "data": {"kind": "erase", "size": 40.0, "opacity": 1.0,
                            "points": [[28.0, 28.0]]}}),
        ))
        .expect("erase");
    let after = {
        let frame = timeline.frame().expect("frame after hiding the layer");
        let at = ((28 * dw + 28) * 4) as usize;
        [frame[at], frame[at + 1], frame[at + 2]]
    };
    assert_eq!(
        after,
        [255, 255, 255],
        "擦除之后那个像素还是 {after:?}（应为背景 255,255,255）⇒ 帧里留了残影"
    );
}
