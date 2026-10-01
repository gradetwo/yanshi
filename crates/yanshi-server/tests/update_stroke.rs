//! **`update_stroke`** ✓（设计 10.3 的**三层参数** ✓）—— "画完之后**改这一笔**" ✓。
//!
//! **为什么有这条测试** ✓：这个工具此前**一个测试都没提过** ✗（零覆盖 ✓），
//! 而查看器本轮把它做成了「改笔触」✓ ⇒ 先把行为钉死 ✓。
//! 判据两条**事实** ✓：① **对象数据真的变了** ✓（`get_object` 的响应里对象字段在**顶层** ✓）；
//! ② **渲染出来的像素真的变了** ✓（`render_region` 在文档空间 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_us", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 在图层上画一条短笔触 ✓，返回它的 id ✓。
fn draw(workspace: &mut Workspace, object_id: &str, color: serde_json::Value) {
    let mut ctx = context(workspace);
    let made = registry().call(
        &mut ctx,
        "draw_stroke",
        &json!({
            "layer_id": "L",
            "object_id": object_id,
            "data": { "points": [[6.0, 8.0], [26.0, 8.0]], "size": 6, "color": color, "opacity": 1.0 },
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

/// **改核心参数之后，对象数据与渲染像素都会变** ✓。
#[test]
fn updating_a_strokes_core_changes_its_data_and_its_pixels() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_us", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
    }
    // 先用**纯红**画 ✓（方便断言通道 ✓）。
    draw(&mut workspace, "S1", json!([1.0, 0.0, 0.0, 1.0]));
    let red_before = {
        let mut ctx = context(&mut workspace);
        let got = registry().call(&mut ctx, "get_object", &json!({ "object_id": "S1" }));
        assert_eq!(got["ok"], json!(true), "{got}");
        got["data"]["color"].clone()
    };
    // ① 改成**绿色 + 更粗** ✓
    let updated = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "update_stroke",
            &json!({ "object_id": "S1", "core": { "color": [0.0, 1.0, 0.0, 1.0], "size": 14.0 } }),
        )
    };
    assert_eq!(updated["ok"], json!(true), "{updated}");
    // ② 事实一：对象数据变了 ✓
    let after = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "get_object", &json!({ "object_id": "S1" }))
    };
    let green_after = after["data"]["color"].clone();
    assert_ne!(green_after, red_before, "颜色数据应当变了：{after}");
    // ③ 事实二：渲染像素变了 ✓（在文档空间取区域 ✓）
    let rendered = workspace
        .document_mut("doc_us")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 32.0, 32.0))
        .expect("区域渲染应成功");
    let mut greenish = 0;
    let mut pure_red = 0;
    for pixel in rendered.2.chunks_exact(4) {
        let (r, g) = (pixel[0] as i32, pixel[1] as i32);
        if g > r + 20 {
            greenish += 1;
        }
        if r > g + 20 && i32::from(pixel[2]) < g {
            pure_red += 1;
        }
    }
    assert!(
        greenish > 0,
        "改色之后应当有偏绿的像素 ✓（实测 {greenish}）"
    );
    assert_eq!(pure_red, 0, "原来的纯红不应还占主导 ✓（实测 {pure_red}）");
}

/// **只给核心参数时，没提到的项保持不动** ✓（`core` 是**合并**，不是整体替换 ✓）。
#[test]
fn updating_one_core_field_leaves_the_others_alone() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_us", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
    }
    draw(&mut workspace, "S2", json!([1.0, 0.0, 0.0, 1.0]));
    let size_before = {
        let mut ctx = context(&mut workspace);
        let got = registry().call(&mut ctx, "get_object", &json!({ "object_id": "S2" }));
        got["data"]["size"].clone()
    };
    {
        let mut ctx = context(&mut workspace);
        let done = registry().call(
            &mut ctx,
            "update_stroke",
            &json!({ "object_id": "S2", "core": { "opacity": 0.25 } }),
        );
        assert_eq!(done["ok"], json!(true), "{done}");
    }
    let got = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "get_object", &json!({ "object_id": "S2" }))
    };
    assert_eq!(
        got["data"]["size"], size_before,
        "只改了 opacity ⇒ size 不应变：{got}"
    );
    assert_eq!(got["data"]["opacity"], json!(0.25), "{got}");
}
