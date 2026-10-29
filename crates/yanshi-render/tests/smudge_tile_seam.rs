//! **大尺寸涂抹的分块接缝判据**（反馈期 2：`MAX_EFFECT_PADDING` 128 → 512）。
//!
//! 机制：涂抹 `size 320 + smudge_length 26` 的区域外扩是
//! `26 + 320 / 2 + 2 = 188px`，旧上限 128 下分块（区域）渲染的外扩被钳，
//! 导致跨界印章的源采样越出区域缓冲而被跳过，分块与整幅静默不一致。
//!
//! 几何（刻意让接缝像素只由一枚跨界印章决定）：
//! * 底色左红（x < 200）右蓝（x >= 200），画布 640×480；
//! * 涂抹笔迹从 (64, 240) 到 (160, 240)，笔尺 320 ⇒ 印章间距 48，
//!   落笔印章为 112 与 160（首枚无方向被跳过）；
//! * 区域分界 x = 319：接缝像素 (319, 240) 距印章 160 仅 159.5px，
//!   只被该印章覆盖（印章 112 的盘只到 x = 272），
//!   而该印章的采样点在 (134, 240)——红色区，距分界 185px；
//! * 旧上限 128 时右区域缓冲从 319 − 128 = 191 起，采样点 134 在缓冲外
//!   ⇒ 该印章被跳过 ⇒ 接缝像素保持蓝色，与整幅（红色）不一致；
//!   新上限 512 时缓冲从 319 − 188 = 131 起，采样点在内 ⇒ 与整幅一致。
//!
//! 判据：整幅渲染与左/右两块区域渲染在重叠处**逐字节相同**，
//! 且接缝像素确实被涂抹改红（非空转：无涂抹对照仍为蓝）。
//! 旧上限下本判据红（右区域接缝像素为蓝，整幅为红）。

use serde_json::json;
use yanshi_core::blob::MemoryBlobStore;
use yanshi_core::{Bbox, DocumentState, Layer, LayerType, Object, ObjectType, Transform};
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

fn object(id: &str, layer_id: &str, kind: ObjectType, z: i64, data: serde_json::Value) -> Object {
    Object {
        seq: 0,
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type: kind,
        z_index: z,
        visible: true,
        enabled: true,
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
    }
}

/// 左红右蓝底色 + 可选的大尺寸涂抹（报告组合：size 320，smudge_length 26）。
fn smudge_document(with_smudge: bool) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_smudge_tile_seam".to_owned());
    state.width = 640;
    state.height = 480;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state
        .layers
        .insert("layer_1".to_owned(), layer("layer_1", 0));
    state.objects.insert(
        "left".to_owned(),
        object(
            "left",
            "layer_1",
            ObjectType::Shape,
            0,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 200, "h": 480}},
                   "color": {"r": 220, "g": 20, "b": 20, "a": 255}}),
        ),
    );
    state.objects.insert(
        "right".to_owned(),
        object(
            "right",
            "layer_1",
            ObjectType::Shape,
            1,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 200, "y": 0, "w": 440, "h": 480}},
                   "color": {"r": 20, "g": 20, "b": 220, "a": 255}}),
        ),
    );
    if with_smudge {
        state.objects.insert(
            "smudge".to_owned(),
            object(
                "smudge",
                "layer_1",
                ObjectType::Retouch,
                2,
                json!({"retouch_type": "smudge", "points": [[64.0, 240.0], [160.0, 240.0]],
                       "size": 320.0, "hardness": 1.0, "opacity": 1.0, "smudge_length": 26.0}),
            ),
        );
    }
    state
}

fn renderer() -> Renderer {
    let grid = TileGrid::new(64, 640, 480).unwrap();
    Renderer::with_budget(grid, 256 * 1024 * 1024).with_max_workers(1)
}

/// 整幅裁切与区域渲染逐字节比对（区域原点为文档坐标 `origin`）。
fn assert_region_matches_whole(
    whole: &yanshi_render::render::RegionRender,
    region: &yanshi_render::render::RegionRender,
    origin_x: u32,
    origin_y: u32,
    tag: &str,
) {
    assert_eq!(region.rgba8.len() as u32, region.width * region.height * 4);
    let mut diff = 0usize;
    for y in 0..region.height {
        for x in 0..region.width {
            let whole_index = (((origin_y + y) * whole.width + origin_x + x) * 4) as usize;
            let region_index = ((y * region.width + x) * 4) as usize;
            for channel in 0..4 {
                if whole.rgba8[whole_index + channel] != region.rgba8[region_index + channel] {
                    diff += 1;
                }
            }
        }
    }
    assert_eq!(
        diff, 0,
        "{tag}：区域渲染与整幅裁切必须逐字节相同，实测 {diff} 字节不同"
    );
}

#[test]
fn large_smudge_region_renders_match_whole_across_tile_boundary() {
    let store = MemoryBlobStore::new();
    let state = smudge_document(true);
    let mut renderer = renderer();
    let whole = renderer.render_document(&state, &store).unwrap();

    // 非空转：接缝像素在整幅里确实被涂抹改红，无涂抹对照仍为蓝。
    let seam = whole.pixel(319, 240).expect("接缝像素应在画布内");
    assert!(
        seam[0] > 180 && seam[2] < 90,
        "整幅接缝像素应被涂抹拖红：{seam:?}"
    );
    let plain = renderer
        .render_document(&smudge_document(false), &store)
        .unwrap();
    let plain_seam = plain.pixel(319, 240).expect("对照像素应在画布内");
    assert!(
        plain_seam[2] > 180 && plain_seam[0] < 90,
        "无涂抹对照的接缝像素应为蓝：{plain_seam:?}"
    );

    // 左区域（对照：旧上限下也应一致）与右区域（本期回归：旧上限下不一致）。
    let left = renderer
        .render_region(&state, &store, Bbox::new(0.0, 0.0, 319.0, 480.0))
        .unwrap();
    assert_region_matches_whole(&whole, &left, 0, 0, "左区域");
    let right = renderer
        .render_region(&state, &store, Bbox::new(319.0, 0.0, 321.0, 480.0))
        .unwrap();
    assert_region_matches_whole(&whole, &right, 319, 0, "右区域");
    let right_seam = right.pixel(0, 240).expect("右区域接缝像素应在区域内");
    assert_eq!(
        right_seam, seam,
        "右区域接缝像素必须等于整幅：{right_seam:?} vs {seam:?}"
    );
    // 截断告警放在最后断言：旧上限下本测试应先红在上面的接缝比对（而非本行），
    // 以证明接缝比对本身就是有效的回归信号；新上限下此处须无告警。
    assert!(
        whole.stats.unsupported.is_empty(),
        "外扩 188px ≤ 新上限 512，不应有截断告警：{:?}",
        whole.stats.unsupported
    );
}
