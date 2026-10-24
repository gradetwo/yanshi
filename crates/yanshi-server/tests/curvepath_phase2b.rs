//! **整合增量（phase 2b）端到端判据：形状入口统一**。
//!
//! 针对二期形状入口的三个缺陷（外部测试者实测踩中）：
//! - `smart_*` / `fit_background_field` 的 shape 解析不认 path，错误信息
//!   还把 path 写成「不支持」；
//! - 缺 `type` 时 `{"d": …}` 被静默吞成缺省 rect (0,0,100,100)——不报错、
//!   结果全错（静默降级）；
//! - `create_mask` 只认 `kind`，`type:"path"` 绕过校验落库且渲不出。
//!
//! 判据纪律：不断「不报错」，断**作用像素**——path 形状真实生效的区域、
//! 以及缺省矩形假象会污染的区域必须保持原样。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: &Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, args)
}

/// brush 类工具要读真 .myb ⇒ 落盘工作区 + 仓库 assets（与一、二期同款脚手架）。
fn assets_workspace(name: &str) -> Workspace {
    let root = std::env::temp_dir().join(format!(
        "yanshi_curvepath2b_ws_{}_{}",
        name,
        std::process::id()
    ));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::with_file_store(root, DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets))
}

fn make_doc(workspace: &mut Workspace, doc: &str, width: u32, height: u32) {
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    let made = call(workspace, doc, "create_layer", &json!({ "layer_id": "L" }));
    assert_eq!(made["ok"], json!(true), "{made}");
}

fn render(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> Vec<u8> {
    workspace
        .render_region_raw(doc, Bbox::new(0.0, 0.0, width as f64, height as f64))
        .expect("渲染应成功")
        .2
}

fn px(data: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        data[index],
        data[index + 1],
        data[index + 2],
        data[index + 3],
    ]
}

fn is_red(pixel: [u8; 4]) -> bool {
    pixel[0] > 150 && pixel[1] < 100 && pixel[2] < 100
}

/// 盘内是否有墨（深色笔迹落在透明/白底上都算）。
fn has_ink(data: &[u8], width: u32, height: u32, cx: u32, cy: u32, r: u32) -> bool {
    for y in cy.saturating_sub(r)..=(cy + r).min(height - 1) {
        for x in cx.saturating_sub(r)..=(cx + r).min(width - 1) {
            let p = px(data, width, x, y);
            if p[3] > 40 && p[0] < 120 && p[1] < 120 && p[2] < 120 {
                return true;
            }
        }
    }
    false
}

/// 三角形：(20,100) (100,20) (180,100)，斜边方程 x+y=120 / 斜边2 y=x−80…右侧边过 (100,20)(180,100)。
const TRIANGLE_D: &str = "M20 100 L100 20 L180 100 Z";

fn triangle_subpaths() -> Value {
    json!([{
        "start": [20, 100],
        "closed": true,
        "segments": [
            {"op": "L", "to": [100, 20]},
            {"op": "L", "to": [180, 100]}
        ]
    }])
}

fn draw_shape(
    workspace: &mut Workspace,
    doc: &str,
    object_id: &str,
    geometry: Value,
    rgb: (u8, u8, u8),
) {
    let made = call(
        workspace,
        doc,
        "draw_shape",
        &json!({
            "layer_id": "L",
            "object_id": object_id,
            "data": {
                "geometry": geometry,
                "color": {"r": rgb.0, "g": rgb.1, "b": rgb.2, "a": 255}
            }
        }),
    );
    assert_eq!(made["ok"], json!(true), "{made}");
}

// ---------------------------------------------------------------------------
// 1. smart_fill：path 形状真实生效（断作用像素，不断「不报错」）
// ---------------------------------------------------------------------------

fn assert_smart_fill_triangle(name: &str, shape: Value) {
    let mut ws = assets_workspace(name);
    make_doc(&mut ws, "s", 200, 120);
    let filled = call(
        &mut ws,
        "s",
        "smart_fill",
        &json!({
            "layer_id": "L",
            "shape": shape,
            "brush": "2B_pencil",
            "color": [0.0, 0.0, 0.0, 1.0]
        }),
    );
    assert_eq!(filled["ok"], json!(true), "{filled}");
    let img = render(&mut ws, "s", 200, 120);
    // 三角形质心附近必须有墨（path 真生效）。
    assert!(
        has_ink(&img, 200, 120, 100, 73, 8),
        "三角形质心 (100,73) 附近应当有排线墨迹"
    );
    // 缺省矩形 (0,0,100,100) 内、三角形外的三处必须无墨：
    // 若 d 被吞成矩形或 path 被当成矩形包围盒，这里会变红。
    for (x, y) in [(15, 15), (30, 30), (50, 15)] {
        assert!(
            !has_ink(&img, 200, 120, x, y, 6),
            "三角形外 ({x},{y}) 不应有墨（距斜边 ≥ 39px，笔刷外溢到不了）"
        );
    }
}

#[test]
fn smart_fill_with_typed_path_d_paints_inside_the_path_only() {
    assert_smart_fill_triangle("typed", json!({"type": "path", "d": TRIANGLE_D}));
}

#[test]
fn smart_fill_with_bare_d_paints_inside_the_path_only() {
    // 缺 type 的 {"d": …}：旧行为是静默吞成 (0,0,100,100) 矩形。
    assert_smart_fill_triangle("bare", json!({"d": TRIANGLE_D}));
}

// ---------------------------------------------------------------------------
// 2. fit_background_field：mask=path 时只有 path 内部被填
// ---------------------------------------------------------------------------

#[test]
fn fit_background_field_with_path_mask_fills_only_inside_the_path() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "bg", 200, 120);
    // 场景：白底 + 黑三角（与 mask 同形）+ 黑方块 (10,10)-(30,30)。
    // 黑方块在三角形外、但在旧缺省矩形 (0,0,100,100) 内——它是判别点：
    // mask 若被吞成缺省矩形，方块会被一起填白。
    draw_shape(
        &mut ws,
        "bg",
        "white",
        json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 200, "h": 120}}),
        (255, 255, 255),
    );
    draw_shape(
        &mut ws,
        "bg",
        "tri",
        json!({"kind": "path", "d": TRIANGLE_D}),
        (0, 0, 0),
    );
    draw_shape(
        &mut ws,
        "bg",
        "sq",
        json!({"kind": "rect", "bbox": {"x": 10, "y": 10, "w": 20, "h": 20}}),
        (0, 0, 0),
    );
    let before = render(&mut ws, "bg", 200, 120);
    assert!(px(&before, 200, 100, 73)[0] < 60, "拟合前三角中心应是黑的");
    assert!(px(&before, 200, 20, 20)[0] < 60, "拟合前方块中心应是黑的");

    // degree=0 ⇒ 用掩膜外样本均值（≈白）填满掩膜内。
    let fitted = call(
        &mut ws,
        "bg",
        "fit_background_field",
        &json!({
            "layer_id": "L",
            "mask": {"type": "path", "d": TRIANGLE_D},
            "degree": 0
        }),
    );
    assert_eq!(fitted["ok"], json!(true), "{fitted}");
    // 判据钉死（不许只断 >0）：mask_bbox 必须是 path 自己的包围盒，
    // filled_pixels 必须与 path 的解析面积（底 160 × 高 80 / 2 = 6400）
    // 在容差内一致——缺省矩形假象会给出 bbox (0,0,100,100) 与 10000，
    // 两条都当场穿帮（gradetwo 的判据只断 filled>0，10000 也能过）。
    assert_eq!(
        fitted["mask_bbox"],
        json!([20.0, 20.0, 180.0, 100.0]),
        "mask_bbox 应是三角形的包围盒，实测 {fitted}"
    );
    let filled = fitted["filled_pixels"].as_u64().unwrap_or(0);
    assert!(
        (5760..=7040).contains(&filled),
        "filled_pixels 应 ≈ 6400（三角形面积 ±10%），实测 {filled}"
    );
    let after = render(&mut ws, "bg", 200, 120);
    assert!(
        px(&after, 200, 100, 73)[0] > 200,
        "三角中心应被拟合场填亮，实测 {:?}",
        px(&after, 200, 100, 73)
    );
    assert!(
        px(&after, 200, 20, 20)[0] < 60,
        "三角形外的黑方块必须保持黑（mask 被吞成矩形时这里会被填亮），实测 {:?}",
        px(&after, 200, 20, 20)
    );
}

// ---------------------------------------------------------------------------
// 3. create_mask：type 别名 + d 写法与 kind+subpaths 逐像素等价
// ---------------------------------------------------------------------------

fn masked_red_doc(ws: &mut Workspace, doc: &str, shape: Value) -> Vec<u8> {
    make_doc(ws, doc, 200, 120);
    draw_shape(
        ws,
        doc,
        "content",
        json!({"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 200, "h": 120}}),
        (200, 30, 30),
    );
    let created = call(
        ws,
        doc,
        "create_mask",
        &json!({"mask_id": "mk", "shape": shape}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    let attached = call(
        ws,
        doc,
        "set_property",
        &json!({"layer_id": "L", "key": "mask_id", "value": "mk"}),
    );
    assert_eq!(attached["ok"], json!(true), "{attached}");
    render(ws, doc, 200, 120)
}

#[test]
fn create_mask_type_alias_and_d_forms_match_kind_subpaths() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    let a = masked_red_doc(
        &mut ws,
        "mA",
        json!({"kind": "path", "subpaths": triangle_subpaths()}),
    );
    let b = masked_red_doc(&mut ws, "mB", json!({"type": "path", "d": TRIANGLE_D}));
    let c = masked_red_doc(&mut ws, "mC", json!({"kind": "path", "d": TRIANGLE_D}));
    for (name, img) in [("A", &a), ("B", &b), ("C", &c)] {
        assert!(
            is_red(px(img, 200, 100, 73)),
            "{name}：三角形内应放行红色，实测 {:?}",
            px(img, 200, 100, 73)
        );
        assert!(
            !is_red(px(img, 200, 10, 10)),
            "{name}：三角形外应被蒙版挡住，实测 {:?}",
            px(img, 200, 10, 10)
        );
    }
    // 三种写法落库后是同一种形状 ⇒ 整幅逐字节一致。
    assert_eq!(a, b, "type+d 写法应与 kind+subpaths 逐像素一致");
    assert_eq!(a, c, "kind+d 写法应与 kind+subpaths 逐像素一致");
}

#[test]
fn create_mask_path_entry_errors_are_specific() {
    let mut ws = Workspace::in_memory(DocumentSettings::default());
    make_doc(&mut ws, "mE", 200, 120);
    let cases: Vec<(Value, &str)> = vec![
        // d 与 subpaths 同给（kind 写法）。
        (
            json!({"mask_id": "e1", "shape": {"kind": "path", "d": TRIANGLE_D, "subpaths": triangle_subpaths()}}),
            "只能给一个",
        ),
        // type 别名什么都不给 ⇒ 校验必须照样触发（旧行为是绕过校验落库）。
        (
            json!({"mask_id": "e2", "shape": {"type": "path"}}),
            "必须给",
        ),
        // d 语法坏 ⇒ 入口报错带原因，不落库。
        (
            json!({"mask_id": "e3", "shape": {"type": "path", "d": "M0 0 X9 9"}}),
            "d 非法",
        ),
        // type 别名 + 坏 subpaths ⇒ 同一份校验的原话。
        (
            json!({"mask_id": "e4", "shape": {"type": "path", "subpaths": [
                {"start": [0, 0], "segments": [{"op": "X", "to": [1, 1]}]}
            ]}}),
            "shape 非法",
        ),
    ];
    for (args, needle) in cases {
        let failed = call(&mut ws, "mE", "create_mask", &args);
        assert_ne!(failed["ok"], json!(true), "应当失败：{failed}");
        let text = failed.to_string();
        assert!(text.contains(needle), "错误应含「{needle}」，实测：{text}");
    }
}
