//! **颜色抖动（破色）** ✓ —— 真实用户 §六-13 要的那件事 ✓。
//!
//! **用户原话** ✓："每次 `draw_stroke` 都要传完整 hex 颜色 ✓，没有'在画布上混色'的机制 ✗
//! ⇒ 专业油画的'**破色**'（broken color）与'并置'技巧难以实现 ✓，
//! agent 只能预先算好几十个 hex 值硬编码" ✗；
//! 他要的是"笔触支持 `color_jitter`（**在给定色相邻范围内随机取色** ✓）" ✓。
//!
//! **三条判据** ✓：① 缺省 `0` 时与"根本没这个键"**逐字节相同** ✓（老文档一个像素不变 ✓）；
//! ② 打开之后同一笔里的颜色**确实更花** ✓（不然等于没做 ✓）；③ 颜色**在目标色附近游走** ✓、
//! **整体不跑偏** ✓（不然就是"换了颜色" ✗，不是"破色" ✓）。

use serde_json::json;
use yanshi_core::{
    DocumentState, Layer, LayerType, MemoryBlobStore, Object, ObjectType, Transform,
};
use yanshi_render::render::Renderer;
use yanshi_render::tile::TileGrid;

fn layer(id: &str, z: i64) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: z,
        blend_mode: "normal".to_owned(),
        opacity: 1.0,
        visible: true,
        locked: false,
        alpha_lock: false,
        clipping_mask: false,
        mask_id: None,
        transform: Transform::IDENTITY,
        medium: None,
        style: None,
        metadata: serde_json::Value::Null,
        blobs: Vec::new(),
        created_by: "a1".to_owned(),
        updated_by: None,
        deleted_by: None,
    }
}

/// 画一条**宽笔**的横线 ✓，返回渲染结果 ✓。`jitter` 为 `None` 表示**根本不写这个键** ✓。
fn render_stroke(jitter: Option<f64>) -> Vec<u8> {
    let mut data = json!({
        "points": [[40.0, 60.0], [80.0, 60.0], [120.0, 60.0], [160.0, 60.0]],
        "size": 26,
        "color": [0.55, 0.35, 0.25, 1.0],
        "smooth": false,
        "seed": 7,
    });
    if let Some(jitter) = jitter {
        // **走既有的 `appearance.dynamics` 机制** ✓（不另起一套参数 ✗）。
        //
        // **注意路径** ✓：是 **`appearance.dynamics`** ✓，不是顶层 `dynamics` ✗ ——
        // 我第一版就写成了顶层 ✓ ⇒ "打开抖动"那条测试**测出来 16 vs 16** ✗ ⇒
        // **一眼看出参数没生效** ✓（这正是"判据要能区分有没有生效"的价值 ✓）。
        data["appearance"] = json!({ "dynamics": { "color_jitter": jitter } });
    }
    let object = Object {
        id: "s1".to_owned(),
        layer_id: "L".to_owned(),
        object_type: ObjectType::Stroke,
        z_index: 0,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec!["a1".to_owned()],
        current_version: Some("a1".to_owned()),
        created_by: "a1".to_owned(),
        deleted_by: None,
        data,
        blobs: Vec::new(),
    };
    let store = MemoryBlobStore::new();
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_jitter".to_owned());
    state.width = 200;
    state.height = 120;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state.layers.insert("L".to_owned(), layer("L", 0));
    state.objects.insert("s1".to_owned(), object);
    let grid = TileGrid::new(64, 200, 120).unwrap();
    let mut renderer = Renderer::new(grid);
    renderer.render_document(&state, &store).unwrap().rgba8
}

/// **缺省 0 ⇒ 与"根本没这个键"逐字节相同** ✓（本项目的铁律 ✓）。
#[test]
fn a_jitter_of_zero_is_byte_identical_to_no_jitter_key() {
    let without = render_stroke(None);
    let with_zero = render_stroke(Some(0.0));
    assert_eq!(
        without, with_zero,
        "color_jitter=0 必须与不写这个键逐字节相同"
    );
    assert!(
        without.chunks_exact(4).any(|pixel| pixel[3] > 0),
        "应当真的画了东西"
    );
}

/// **打开之后同一笔里的颜色更花** ✓（这条直接测"破色"有没有发生 ✓）。
#[test]
fn jitter_makes_the_stroke_more_colourful() {
    let plain = render_stroke(None);
    let jittered = render_stroke(Some(0.5));
    // 数"不同的颜色"的个数 ✓（量化到 8 级 ✓，避开浮点噪声 ✓）。
    let shades = |rgba: &[u8]| -> usize {
        let mut seen = std::collections::BTreeSet::new();
        for pixel in rgba.chunks_exact(4) {
            if pixel[3] > 40 {
                seen.insert((pixel[0] / 8, pixel[1] / 8, pixel[2] / 8));
            }
        }
        seen.len()
    };
    let plain_shades = shades(&plain);
    let jittered_shades = shades(&jittered);
    eprintln!("  破色：不开 {plain_shades} 种色阶 ⇒ 开 0.5 {jittered_shades} 种");
    assert!(
        plain_shades > 0,
        "不开抖动时也应当有颜色（否则测试没测到东西）"
    );
    assert!(
        jittered_shades > plain_shades,
        "开了 color_jitter 之后同一笔里的颜色应当更花（不开 {plain_shades} vs 开 {jittered_shades}）"
    );
    // **确定性** ✓：同输入 ⇒ 逐字节相同 ✓（D0 ✓）。
    assert_eq!(
        jittered,
        render_stroke(Some(0.5)),
        "同一份输入应当逐字节相同"
    );
}

/// **颜色在目标色附近游走、整体不跑偏** ✓（否则就是"换了颜色" ✗，不是破色 ✓）。
#[test]
fn jitter_wanders_around_the_target_without_drifting_away() {
    let plain = render_stroke(None);
    let jittered = render_stroke(Some(0.5));
    // 只统计**上了墨**的像素 ✓（背景是纯白 ✓ ⇒ 把背景算进去会把均值拉跑 ✗ —— 这个坑我踩过 ✓）。
    let mean = |rgba: &[u8]| -> (f64, f64, f64) {
        let mut sums = (0.0, 0.0, 0.0);
        let mut count: f64 = 0.0;
        for pixel in rgba.chunks_exact(4) {
            if pixel[3] > 40 {
                sums.0 += f64::from(pixel[0]);
                sums.1 += f64::from(pixel[1]);
                sums.2 += f64::from(pixel[2]);
                count += 1.0;
            }
        }
        (
            sums.0 / count.max(1.0),
            sums.1 / count.max(1.0),
            sums.2 / count.max(1.0),
        )
    };
    let (r0, g0, b0) = mean(&plain);
    let (r1, g1, b1) = mean(&jittered);
    eprintln!("  均值：不开 ({r0:.1}, {g0:.1}, {b0:.1}) ⇒ 开 ({r1:.1}, {g1:.1}, {b1:.1})");
    // **每个通道的均值都不该跑超过 12 级** ✓（±0.5 的抖动幅度对 255 级来说很小 ✓）。
    for (name, plain_side, jittered_side) in [("r", r0, r1), ("g", g0, g1), ("b", b0, b1)] {
        assert!(
            (plain_side - jittered_side).abs() < 12.0,
            "{name} 通道的均值跑偏了 ⇒ 那是换色不是破色：{plain_side:.1} vs {jittered_side:.1}"
        );
    }
}
