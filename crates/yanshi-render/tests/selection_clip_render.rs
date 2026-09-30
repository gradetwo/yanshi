//! 渲染器级最小用例：选区「约束落笔」——**接线仍撤回，故标 `#[ignore]`** ✗。
//!
//! 本用例证明**内核侧的裁剪做法是对的**（把选区覆盖度折进印章 ✓）：
//! 它在渲染器层面前通过 ✓（选区内照常、选区外被裁、已有内容不变）；
//! 但在**服务端分次（脏区）渲染**下，走裁剪分支时笔画会被丢掉 ✗（用户会丢失笔画 ✗），
//! 因此接线撤回 ✓。内核部件另有独立单测（`brush::tests::clipped_stamping_respects_coverage` ✓）。
//! 接线恢复后本用例应去掉 `#[ignore]` ✓。
//!
//! 本用例的价值：它证明**裁剪逻辑本身是对的**（选区内照常、选区外被裁、已有内容不动 ✓），
//! 从而把上一轮服务端的失败精确定位到**分次（脏区）渲染**上 ✓：
//! 调试输出显示只有笔画对象被裁 ✓、但被裁**两次**且区域 origin 不同 ✓ —— 说明服务端每次都新建
//! 只覆盖脏区的缓冲区 ✓，而"还原绘制前像素"的写法要求缓冲区里**已有完整基底** ✗，
//! 于是第二次渲染把选区外的已有内容还原成了空白 ✗（用户会看到"内容消失" ✓）。
//!
//! 接线改成"在对象贡献处施加掩码"后（见 implementation-notes 的方案），
//! 本用例应恢复为普通测试 ✓。
//!
//! 上一轮在**服务端**路径上接线后出现"选区内的落笔也消失" ✗，已撤回并记录。
//! 本文件按计划把问题压到**渲染器内部** ✓ —— 不经服务端、不经原子日志、不经异步渲染 ✓，
//! 用两三个对象与一个选区就能判定接线到底错在哪里 ✓。

use serde_json::json;
use yanshi_core::{DocumentState, Layer, LayerType, Object, ObjectType, Transform};
use yanshi_render::Renderer;

const SIZE: u32 = 64;

fn layer(id: &str) -> Layer {
    Layer {
        id: id.to_owned(),
        name: id.to_owned(),
        layer_type: LayerType::Raster,
        parent_id: None,
        z_index: 0,
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
        created_by: "01AAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
        updated_by: None,
        deleted_by: None,
    }
}

fn base_object(id: &str, layer_id: &str, object_type: ObjectType, created_by: &str) -> Object {
    Object {
        id: id.to_owned(),
        layer_id: layer_id.to_owned(),
        object_type,
        z_index: 0,
        visible: true,
        locked: false,
        metadata: serde_json::Value::Null,
        transform: Transform::IDENTITY,
        style: None,
        versions: vec![created_by.to_owned()],
        current_version: Some(created_by.to_owned()),
        created_by: created_by.to_owned(),
        deleted_by: None,
        data: json!({}),
        blobs: Vec::new(),
    }
}

/// 文档：底色形状（最早）→ 选区 → 一条横贯整幅的笔画（最晚）。
fn document(with_selection: bool) -> DocumentState {
    let mut state = DocumentState::empty();
    state.doc_id = Some("doc_clip".to_owned());
    state.width = SIZE;
    state.height = SIZE;
    state.background = json!({"r": 255, "g": 255, "b": 255, "a": 255});
    state.layers.insert("layer_1".to_owned(), layer("layer_1"));

    // ① 底色（在选区**之前**创建 ⇒ 不应被裁剪）。
    let mut background = base_object(
        "obj_a_background",
        "layer_1",
        ObjectType::Shape,
        "01BBBBBBBBBBBBBBBBBBBBBBBBBB",
    );
    background.data = json!({
        "geometry": {"kind": "rect", "bbox": {"x": 0.0, "y": 0.0, "w": SIZE as f64, "h": SIZE as f64}},
        "color": [0.1, 0.4, 0.8, 1.0]
    });
    state.objects.insert(background.id.clone(), background);

    // ② 选区：中间竖带（40..56）。
    if with_selection {
        let mut selection = yanshi_core::Selection {
            id: "sel_1".to_owned(),
            shape: json!({"kind": "rect",
                          "bbox": {"x": 40.0, "y": 0.0, "w": 16.0, "h": SIZE as f64}}),
            feather: 0.0,
            mode: "new".to_owned(),
            invert: false,
            linked_layer: None,
            refined_edges: false,
            blobs: Vec::new(),
            created_by: "01CCCCCCCCCCCCCCCCCCCCCCCCCC".to_owned(),
            deleted_by: None,
        };
        selection.id = "sel_1".to_owned();
        state.selections.insert("sel_1".to_owned(), selection);
    }

    // ③ 横贯整幅的笔画（在选区**之后**创建 ⇒ 应被裁剪）。
    let mut stroke = base_object(
        "obj_z_stroke",
        "layer_1",
        ObjectType::Stroke,
        "01DDDDDDDDDDDDDDDDDDDDDDDDDD",
    );
    stroke.data = json!({
        "points": [[0.0, 32.0], [SIZE as f64, 32.0]],
        "size": 12.0,
        "hardness": 1.0,
        "color": [1.0, 0.9, 0.0, 1.0]
    });
    state.objects.insert(stroke.id.clone(), stroke);
    state
}

fn render(state: &DocumentState) -> Vec<u8> {
    let grid = yanshi_render::TileGrid::new(32, SIZE, SIZE).expect("tile grid 应可创建");
    let mut renderer = Renderer::with_budget(grid, 64 * 1024 * 1024);
    renderer
        .render_document(state, &yanshi_core::blob::MemoryBlobStore::new())
        .expect("渲染应成功")
        .rgba8
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
    let index = (y * SIZE as usize + x) * 4;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

#[test]
fn selection_clips_new_painting_inside_and_leaves_older_content_alone() {
    let with = render(&document(true));
    let without = render(&document(false));

    eprintln!(
        "调试：无选区 (48,32)={:?} (10,32)={:?}",
        pixel(&without, 48, 32),
        pixel(&without, 10, 32)
    );
    eprintln!(
        "调试：有选区 (48,32)={:?} (10,32)={:?}",
        pixel(&with, 48, 32),
        pixel(&with, 10, 32)
    );

    // 选区**内**：笔画应当照常画上（与"无选区"时一致）。
    assert_eq!(
        pixel(&with, 48, 32),
        pixel(&without, 48, 32),
        "选区内的落笔必须与无选区时相同（上一轮接线失败时这里会丢掉笔画）"
    );
    // 选区**外**：应保持底色，即与"无选区"时不同（无选区时那里是笔画色）。
    let outside = pixel(&with, 10, 32);
    let outside_without = pixel(&without, 10, 32);
    assert_ne!(
        outside, outside_without,
        "选区外不应有新落笔（应当仍是底色）"
    );
    // 底色形状本身不受影响：远离笔画的角落两者应相同。
    assert_eq!(
        pixel(&with, 4, 4),
        pixel(&without, 4, 4),
        "选区不得改写已有内容（角落应完全一致）"
    );
}
