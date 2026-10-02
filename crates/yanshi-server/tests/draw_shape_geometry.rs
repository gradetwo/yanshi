//! **`draw_shape` 的几何校验** ✓ —— 真实用户报的 P0：「**画了个寂寞**」✗。
//!
//! **原症状** ✓：四种常见写法**全部返回 `ok: true`** ✓，而对象 bbox 是 `[0,0,0,0]` ✗、渲染一片空白 ✗
//! ⇒ **存了一个解析不了的几何** ✓ ⇒ 用户以为是自己画错了 ✗。
//! **修法** ✓：在共享的 `write_draw` 里校验 ✓ ⇒ **解析不了就明确报错** ✗，并且**不留空对象** ✓。
//! **本测试就是那条复现** ✓（把"当时踩到的四种写法"逐个钉住 ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_shape", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_shape", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
}

fn object_count(workspace: &mut Workspace) -> usize {
    let mut ctx = context(workspace);
    let listed = registry().call(&mut ctx, "list_objects", &json!({}));
    listed["objects"].as_array().map(Vec::len).unwrap_or(0)
}

/// **用户报的四种写法：必须明确报错，且不留空对象** ✓。
#[test]
fn unparseable_geometry_is_refused_loudly_and_leaves_nothing_behind() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let cases: Vec<(&str, serde_json::Value)> = vec![
        (
            "type/x/y/w/h",
            json!({"type": "rect", "x": 10, "y": 10, "w": 60, "h": 40}),
        ),
        (
            "shape/rect",
            json!({"shape": "rect", "rect": {"x": 10, "y": 10, "w": 60, "h": 40}}),
        ),
        ("rect 数组", json!({"rect": [10, 10, 60, 40]})),
        ("rect 缺 bbox", json!({"kind": "rect"})),
        (
            "bbox 宽为 0",
            json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 0, "h": 40}}),
        ),
        (
            "polygon 只有两点",
            json!({"kind": "polygon", "points": [[0, 0], [4, 0]]}),
        ),
        (
            "未知 kind",
            json!({"kind": "triangle", "bbox": {"x": 0, "y": 0, "w": 4, "h": 4}}),
        ),
    ];
    for (index, (label, geometry)) in cases.iter().enumerate() {
        let before = object_count(&mut workspace);
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "draw_shape",
                &json!({
                    "layer_id": "L",
                    "object_id": format!("bad{index}"),
                    "data": { "geometry": geometry, "color": {"r": 200, "g": 60, "b": 60, "a": 255} },
                }),
            )
        };
        // ① **明确报错** ✗（而不是 ok:true ✗）
        assert_eq!(
            refused["ok"],
            json!(false),
            "「{label}」应当被明确拒绝，实测：{refused}"
        );
        // ② **原因要能教人写对** ✓（提到 geometry 或 kind 这类关键词 ✓）
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("geometry")
                || detail.contains("kind")
                || detail.contains("bbox")
                || detail.contains("points"),
            "「{label}」的拒绝原因应当说明该写什么，实测：{detail}"
        );
        // ③ **不留空对象** ✓（这是原症状里最坑的一点 ✓：报错之前它已经落库了 ✗）
        assert_eq!(
            object_count(&mut workspace),
            before,
            "「{label}」被拒之后不应留下任何对象"
        );
    }
}

/// **正确的写法必须照常能画** ✓（校验不能误伤 ✓），且 bbox 真的是那个尺寸 ✓。
#[test]
fn the_documented_geometry_forms_still_work() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let good: Vec<(&str, serde_json::Value)> = vec![
        (
            "rect + bbox 对象",
            json!({"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 40, "h": 30}}),
        ),
        (
            "rect + bbox 数组",
            json!({"kind": "rect", "bbox": [10, 10, 40, 30]}),
        ),
        (
            "ellipse + bbox",
            json!({"kind": "ellipse", "bbox": {"x": 4, "y": 4, "w": 20, "h": 16}}),
        ),
        (
            "polygon + points",
            json!({"kind": "polygon", "points": [[0, 0], [20, 0], [10, 18]]}),
        ),
    ];
    for (index, (label, geometry)) in good.iter().enumerate() {
        let made = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "draw_shape",
                &json!({
                    "layer_id": "L",
                    "object_id": format!("good{index}"),
                    "data": { "geometry": geometry, "color": {"r": 60, "g": 120, "b": 200, "a": 255} },
                }),
            )
        };
        assert_eq!(made["ok"], json!(true), "「{label}」应当能画：{made}");
        let got = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "get_object",
                &json!({ "object_id": format!("good{index}") }),
            )
        };
        let bbox = got["bbox"].as_array().cloned().unwrap_or_default();
        assert_eq!(bbox.len(), 4, "「{label}」应当有真实的 bbox：{got}");
        // **不能是零面积** ✓（原症状就是零面积 ✓）
        let w = bbox[2].as_f64().unwrap_or(0.0);
        let h = bbox[3].as_f64().unwrap_or(0.0);
        assert!(w > 0.0 && h > 0.0, "「{label}」的 bbox 不应为零面积：{got}");
    }
}

/// **扁平写法真的能画** ✓（真实用户第二次报告 ✓）。
///
/// **用户原话** ✓："矩形需要 `{kind:"rect", bbox:{x,y,w,h}}` 而非扁平的 `{kind:"rect", x,y,w,h}`，
/// 虽有错误提示引导，但初始学习成本较高" ✓。
/// **本测试证的是"能画"而不只是"不报错"** ✓ —— 这正是最初 P0 的教训 ✓：
/// **"接受却画不出来"比"直接拒绝"更坏** ✗。
#[test]
fn the_flat_bbox_spelling_really_draws() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "flat",
                    // **扁平写法** ✓：四个字段直接写在 geometry 上 ✓
                    "data": {"geometry": {"kind": "rect", "x": 8, "y": 10, "w": 40, "h": 30},
                             "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        )
    };
    assert_eq!(made["ok"], json!(true), "{made}");
    // ① 包围盒非零 ✓
    let got = {
        let mut ctx = context(&mut workspace);
        registry().call(&mut ctx, "get_object", &json!({ "object_id": "flat" }))
    };
    let bbox: Vec<f64> = got["bbox"]
        .as_array()
        .map(|values| values.iter().filter_map(|value| value.as_f64()).collect())
        .unwrap_or_default();
    assert_eq!(bbox.len(), 4, "应当有包围盒：{got}");
    assert!(
        bbox[2] > 30.0 && bbox[3] > 20.0,
        "包围盒应当是画上去的尺寸：{bbox:?}"
    );
    // ② **真的落了墨** ✓（这条才排除了"接受但空白" ✗）
    let inked = {
        let (_, _, pixels) = workspace
            .document_mut("doc_shape")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 64.0, 64.0))
            .expect("区域渲染应成功");
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    assert!(
        inked > 300,
        "扁平写法必须真的画出东西（实测 {inked} 个暗像素；40×30 = 1200 ⇒ 远多于 300 ✓）"
    );
}
