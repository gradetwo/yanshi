//! `erase` 必须真的擦掉像素。
//!
//! 用户报告 + API 实测发现的缺陷：`erase` 返回 `ok: true` 但**着色像素一个都不减少** ✗。
//! 根因：`ObjectType` 里没有 Erase 变体 ✓，fold 按 `payload.type` 决定类型、缺省落到
//! **Stroke** ✗；而渲染层也**没有擦除图元、没有 destination-out 语义** ✗✓ ——
//! 两处都缺，于是"橡皮"完全不生效 ✓。
//!
//! 修法：擦除归类为 `Retouch` 且 `retouch_type = "erase"`（折叠层补齐 ✓，因为查看器经
//! `/api/atoms` 直提原子 ✓），渲染层按覆盖度扣除 alpha ✓（与画笔共用同一套衰减数学 ✓）。

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
    ToolContext::new(workspace, "doc_erase", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 统计区域内**仍是底色**（偏绿）的像素数。
///
/// 注意：不能用"不透明像素数"来判断擦除 ✗ —— `render_region_raw` 会把图层合成到**白色背景**上，
/// 被擦掉的区域变成"白色不透明"，不透明计数因此不变 ✓（我第一次就是这么写错的）。
fn green_pixels(workspace: &mut Workspace, x: f64, y: f64, w: f64, h: f64) -> usize {
    let (_, _, pixels) = workspace
        .render_region_raw("doc_erase", Bbox::new(x, y, w, h))
        .expect("区域渲染应成功");
    pixels
        .chunks_exact(4)
        .filter(|pixel| {
            // 底色 (40,160,90) 转显示空间后绿通道明显高于红/蓝。
            pixel[1] as i32 > pixel[0] as i32 + 20 && pixel[1] as i32 > pixel[2] as i32 + 20
        })
        .count()
}

#[test]
fn erase_reduces_alpha_along_the_stroke() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_erase", 256, 256),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
        // 铺满底色。
        let filled = registry.call(
            &mut ctx,
            "fill",
            &json!({"layer_id": "L",
                    "data": {"color": {"r": 40, "g": 160, "b": 90, "a": 255},
                             "region": {"x": 0, "y": 0, "w": 256, "h": 256}}}),
        );
        assert_eq!(filled["ok"], json!(true), "{filled}");
    }
    let before = green_pixels(&mut workspace, 0.0, 0.0, 256.0, 256.0);
    assert_eq!(before, 256 * 256, "底色应铺满整幅");

    let erased = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "erase",
            &json!({"layer_id": "L",
                    "data": {"points": [[40.0, 128.0], [216.0, 128.0]],
                             "size": 40.0, "hardness": 1.0, "opacity": 1.0}}),
        )
    };
    assert_eq!(erased["ok"], json!(true), "{erased}");

    // 调试：折叠后该对象究竟是什么类型、data 里有什么。
    {
        let document = workspace.document_mut("doc_erase").unwrap();
        let objects: Vec<String> = document
            .state()
            .alive_objects()
            .into_iter()
            .map(|object| {
                format!(
                    "{} type={:?} data={}",
                    object.id, object.object_type, object.data
                )
            })
            .collect();
        eprintln!("调试：对象 = {objects:#?}");
    }
    let after = green_pixels(&mut workspace, 0.0, 0.0, 256.0, 256.0);
    assert!(
        after < before,
        "擦除后底色像素必须减少：{before} → {after}（此前橡皮完全不生效）"
    );
    // 笔迹中心应被擦透（露出白背景 ⇒ 不再是底色）。
    let centre = green_pixels(&mut workspace, 120.0, 120.0, 16.0, 16.0);
    assert_eq!(centre, 0, "笔迹中心应被完全擦除（仍是底色的像素 {centre}）");
    // 远离笔迹的角落不受影响。
    let corner = green_pixels(&mut workspace, 0.0, 0.0, 24.0, 24.0);
    assert_eq!(
        corner,
        24 * 24,
        "左上角不应被擦到（仍是底色的像素 {corner}）"
    );
}
