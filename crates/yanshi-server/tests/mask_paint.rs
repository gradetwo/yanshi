//! 蒙版画笔端到端：`paint_mask` 画/擦 → 渲染 → 像素验证。
//!
//! 覆盖专家方案 §4 的核心判据：
//! 1. 在几何蒙版上 erase 一笔，渲染后该区域内容消失（蒙版生效）；
//! 2. 未被笔触碰到的蒙版区保持原样（增量语义，非全量替换）；
//! 3. `delete_mask` 后蒙版不再生效（内容恢复）。

use serde_json::json;
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_mask", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn render(workspace: &mut Workspace) -> Vec<u8> {
    workspace
        .render_region_raw("doc_mask", Bbox::new(0.0, 0.0, 128.0, 64.0))
        .expect("渲染应成功")
        .2
}

fn pixel(data: &[u8], x: usize, y: usize) -> [u8; 4] {
    let index = (y * 128 + x) * 4;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

/// 建文档 + 图层 + 一条横贯的深色笔触（给蒙版"有东西可遮"）。
fn fresh_with_stroke() -> Workspace {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_mask", 128, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    let mut ctx = context(&mut workspace);
    registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
    let drawn = registry.call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "L",
            "object_id": "s1",
            "data": {
                "points": [[8.0, 32.0], [120.0, 32.0]],
                "size": 12.0,
                "hardness": 1.0,
                "color": {"r": 0, "g": 0, "b": 0, "a": 255}
            }
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    // 挂左半几何蒙版。
    let created = registry.call(
        &mut ctx,
        "create_mask",
        &json!({"mask_id": "m1",
                "shape": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 64, "h": 64}}}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let attached = registry.call(
        &mut ctx,
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "m1"}),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
    workspace
}

/// 蒙版画笔 erase：擦掉的一块在渲染后消失，未擦的保持。
#[test]
fn paint_mask_erase_hides_content() {
    let mut workspace = fresh_with_stroke();
    // 基线：左半（蒙版内）有笔触，右半（蒙版外）无。
    let base = render(&mut workspace);
    assert!(
        pixel(&base, 16, 32)[0] < 120,
        "蒙版内应是深色笔触：{:?}",
        pixel(&base, 16, 32)
    );
    assert!(
        pixel(&base, 96, 32)[0] > 200,
        "蒙版外应是背景：{:?}",
        pixel(&base, 96, 32)
    );

    // 在 (48,32) 处擦除（size 30 → 半径 15，覆盖 x∈[33,63]）。
    let registry = registry();
    let mut ctx = context(&mut workspace);
    let painted = registry.call(
        &mut ctx,
        "paint_mask",
        &json!({
            "mask_id": "m1",
            "points": [[48.0, 32.0], [52.0, 32.0]],
            "mode": "erase",
            "size": 30.0,
        }),
    );
    assert_eq!(painted["ok"], json!(true), "{painted}");
    assert_eq!(painted["deposited"], json!(true), "应有实际沉积：{painted}");

    let after = render(&mut workspace);
    assert!(
        pixel(&after, 48, 32)[0] > 200,
        "擦除区应恢复背景：{:?}",
        pixel(&after, 48, 32)
    );
    assert!(
        pixel(&after, 16, 32)[0] < 120,
        "未擦区应保持深色：{:?}",
        pixel(&after, 16, 32)
    );
}

/// 蒙版画笔 paint：在空蒙版区补回选区。
#[test]
fn paint_mask_paint_restores_content() {
    let mut workspace = fresh_with_stroke();
    let registry = registry();
    // 先擦掉左半的一块。
    let mut ctx = context(&mut workspace);
    let erased = registry.call(
        &mut ctx,
        "paint_mask",
        &json!({
            "mask_id": "m1",
            "points": [[32.0, 32.0]],
            "mode": "erase",
            "size": 40.0,
        }),
    );
    assert_eq!(erased["ok"], json!(true), "{erased}");
    assert_eq!(erased["deposited"], json!(true), "擦除应有沉积：{erased}");
    let hidden = render(&mut workspace);
    assert!(
        pixel(&hidden, 32, 32)[0] > 200,
        "擦除后应是背景：{:?}",
        pixel(&hidden, 32, 32)
    );
    // 再画回来。
    let mut ctx = context(&mut workspace);
    let painted = registry.call(
        &mut ctx,
        "paint_mask",
        &json!({
            "mask_id": "m1",
            "points": [[32.0, 32.0]],
            "mode": "paint",
            "size": 40.0,
        }),
    );
    assert_eq!(painted["ok"], json!(true), "{painted}");
    let restored = render(&mut workspace);
    assert!(
        pixel(&restored, 32, 32)[0] < 120,
        "画回后应是深色笔触：{:?}",
        pixel(&restored, 32, 32)
    );
}

/// `delete_mask`：删除后蒙版不再裁剪（右半内容恢复）。
#[test]
fn delete_mask_restores_full_layer() {
    let mut workspace = fresh_with_stroke();
    let base = render(&mut workspace);
    assert!(
        pixel(&base, 96, 32)[0] > 200,
        "蒙版外应是背景：{:?}",
        pixel(&base, 96, 32)
    );

    let registry = registry();
    let mut ctx = context(&mut workspace);
    let deleted = registry.call(&mut ctx, "delete_mask", &json!({"mask_id": "m1"}));
    assert_eq!(deleted["ok"], json!(true), "{deleted}");

    let after = render(&mut workspace);
    assert!(
        pixel(&after, 96, 32)[0] < 120,
        "删蒙版后右半笔触应恢复：{:?}",
        pixel(&after, 96, 32)
    );
}
