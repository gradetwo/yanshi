//! **原生矢量图层（路线甲 Phase 2 第一增量）的工具面判据** ✓。
//!
//! 每条断言都对着一个「静默降级」形态：建错类型不许静默、矢量层不接受
//! 的笔参数必须整笔拒绝、光栅工具落到矢量层必须被守卫拦下、橡皮截断的
//! 段数与计数必须对得上、SVG 导出必须把原生直出与事后转换分开记账 ✓。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn workspace(doc: &str) -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(NewDocument::new(doc, 256, 256), "human:1", "session:test")
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn stroke(data_extra: Value) -> Value {
    let mut data = json!({
        "points": [[20.0, 100.0], [220.0, 100.0]],
        "size": 20.0,
        "color": [200, 60, 40, 255],
        "hardness": 1.0,
    });
    if let (Some(base), Some(extra)) = (data.as_object_mut(), data_extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    data
}

fn temp_file(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("yanshi_vector_test_{}", std::process::id()));
    // **守卫不能放在这个辅助函数里** ✗（第 833 轮；CI 就是在这里红的 ✓）
    //   **⇒ 缺陷**：`_cleanup` 在 `temp_file` 返回时立刻 drop ⇒ **把目录删了** ✗
    //     ⇒ 后面 `export_png` 报 `写文件失败：…/vec.png ⇒ No such file or directory` ✓
    //   **⇒ 修法**：守卫留给**测试函数**（它调用方 ✓），这里只返回路径 ✓。
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name).to_string_lossy().into_owned()
}

#[test]
fn vector_layer_end_to_end() {
    let doc = "doc_vec";
    let mut ws = workspace(doc);

    // ① kind=vector 建层：返回与 list_layers 都如实报 vector（type 与 kind 同值 ✓）。
    let created = call(
        &mut ws,
        doc,
        "create_layer",
        json!({"layer_id": "v1", "kind": "vector"}),
    );
    assert_eq!(created["ok"], json!(true), "{created}");
    assert_eq!(created["layer_type"], json!("vector"), "{created}");
    let layers = call(&mut ws, doc, "list_layers", json!({}));
    let v1 = layers["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|layer| layer["layer_id"] == json!("v1"))
        .expect("v1 应在图层列表里")
        .clone();
    assert_eq!(v1["type"], json!("vector"), "{v1}");
    assert_eq!(v1["kind"], json!("vector"), "{v1}");

    // ② kind 与 type 冲突、不认识的 kind ⇒ 显式拒绝（不许猜 ✓）。
    let conflict = call(
        &mut ws,
        doc,
        "create_layer",
        json!({"layer_id": "v2", "kind": "vector", "type": "raster"}),
    );
    assert_eq!(conflict["ok"], json!(false), "{conflict}");
    let banana = call(
        &mut ws,
        doc,
        "create_layer",
        json!({"layer_id": "v3", "kind": "banana"}),
    );
    assert_eq!(banana["ok"], json!(false), "{banana}");

    // ③ 老写法零变化：不带 type/kind 建层 ⇒ raster；type=vector 也行 ✓。
    let plain = call(&mut ws, doc, "create_layer", json!({"layer_id": "r1"}));
    assert_eq!(plain["layer_type"], json!("raster"), "{plain}");
    let by_type = call(
        &mut ws,
        doc,
        "create_layer",
        json!({"layer_id": "v9", "type": "vector"}),
    );
    assert_eq!(by_type["layer_type"], json!("vector"), "{by_type}");

    // ④ 矢量层上画 3 笔合法矢量笔 ✓。
    for (id, y) in [("s1", 40.0), ("s2", 100.0), ("s3", 180.0)] {
        let drawn = call(
            &mut ws,
            doc,
            "draw_stroke",
            json!({
                "layer_id": "v1",
                "object_id": id,
                "data": stroke(json!({"points": [[20.0, y], [220.0, y]]})),
            }),
        );
        assert_eq!(drawn["ok"], json!(true), "{id}: {drawn}");
    }

    // ⑤ 不可矢量化的笔参数 ⇒ 整笔拒绝、理由点名（jitter / 无 CSS 的混合模式 ✓）。
    let jittered = call(
        &mut ws,
        doc,
        "draw_stroke",
        json!({"layer_id": "v1", "data": stroke(json!({"jitter": 0.5}))}),
    );
    assert_eq!(jittered["ok"], json!(false), "{jittered}");
    assert!(
        format!("{jittered}").contains("jitter"),
        "拒绝理由应点名 jitter：{jittered}"
    );
    let added = call(
        &mut ws,
        doc,
        "draw_stroke",
        json!({"layer_id": "v1", "data": stroke(json!({"blend_mode": "add"}))}),
    );
    assert_eq!(added["ok"], json!(false), "{added}");
    assert!(
        format!("{added}").contains("blend_mode"),
        "拒绝理由应点名 blend_mode：{added}"
    );

    // ⑥ 光栅通路工具落到矢量层 ⇒ 守卫显式拒绝（执行前拦下 ✓）。
    let guarded = call(
        &mut ws,
        doc,
        "brush_stroke",
        json!({"layer_id": "v1", "brush": "2B_pencil", "points": [[10.0, 10.0], [60.0, 60.0]], "size": 20.0}),
    );
    assert_eq!(guarded["ok"], json!(false), "{guarded}");
    assert!(
        format!("{guarded}").contains("矢量图层"),
        "守卫文案应点名矢量图层：{guarded}"
    );
    let guarded_erase = call(
        &mut ws,
        doc,
        "erase",
        json!({"layer_id": "v1", "data": {"region": {"x": 0.0, "y": 0.0, "w": 10.0, "h": 10.0}}}),
    );
    assert_eq!(guarded_erase["ok"], json!(false), "{guarded_erase}");
    assert!(
        format!("{guarded_erase}").contains("矢量图层"),
        "像素擦除同样应被守卫拦下：{guarded_erase}"
    );

    // ⑦ 命中测试：s2 的脊线正中命中、正上方远处不命中 ✓。
    let hit = call(
        &mut ws,
        doc,
        "vector_hit_test",
        json!({"layer_id": "v1", "x": 120.0, "y": 100.0}),
    );
    assert_eq!(hit["ok"], json!(true), "{hit}");
    assert_eq!(hit["count"], json!(1), "{hit}");
    assert_eq!(hit["hits"][0]["object_id"], json!("s2"), "{hit}");
    let miss = call(
        &mut ws,
        doc,
        "vector_hit_test",
        json!({"layer_id": "v1", "x": 120.0, "y": 250.0}),
    );
    assert_eq!(miss["count"], json!(0), "{miss}");

    // ⑧ 矢量橡皮竖切 s2：截成两段（原对象保留左段 + 新建一段）、其余两笔不动 ✓。
    let erased = call(
        &mut ws,
        doc,
        "vector_erase",
        json!({"layer_id": "v1", "points": [[120.0, 70.0], [120.0, 130.0]], "radius": 6.0}),
    );
    assert_eq!(erased["ok"], json!(true), "{erased}");
    let counts = &erased["vector_erase"];
    assert_eq!(counts["strokes_total"], json!(3), "{erased}");
    assert_eq!(counts["strokes_truncated"], json!(1), "{erased}");
    assert_eq!(counts["strokes_erased"], json!(0), "{erased}");
    assert_eq!(counts["strokes_unchanged"], json!(2), "{erased}");
    assert_eq!(
        counts["created_object_ids"].as_array().unwrap().len(),
        1,
        "{erased}"
    );
    let created_id = counts["created_object_ids"][0].as_str().unwrap().to_owned();

    // 段的位置恒等式：原对象整段在切口左侧、新段整段在切口右侧 ✓。
    let original = call(&mut ws, doc, "get_object", json!({"object_id": "s2"}));
    let original_max_x = original["data"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_f64().unwrap())
        .fold(f64::MIN, f64::max);
    assert!(
        original_max_x < 120.0,
        "原段应止于切口左侧：{original_max_x}"
    );
    let created_obj = call(&mut ws, doc, "get_object", json!({"object_id": created_id}));
    let created_min_x = created_obj["data"]["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p[0].as_f64().unwrap())
        .fold(f64::MAX, f64::min);
    assert!(created_min_x > 120.0, "新段应起于切口右侧：{created_min_x}");

    // ⑨ 矢量工具作用于 raster 图层 ⇒ 显式拒绝 ✓。
    let wrong_layer = call(
        &mut ws,
        doc,
        "vector_erase",
        json!({"layer_id": "layer_default", "points": [[0.0, 0.0]], "radius": 5.0}),
    );
    assert_eq!(wrong_layer["ok"], json!(false), "{wrong_layer}");
    let wrong_hit = call(
        &mut ws,
        doc,
        "vector_hit_test",
        json!({"layer_id": "layer_default", "x": 0.0, "y": 0.0}),
    );
    assert_eq!(wrong_hit["ok"], json!(false), "{wrong_hit}");

    // ⑩ 导出：PNG 正常落盘；SVG 里 4 笔全计入原生直出、事后转换 0 ✓。
    let png_path = temp_file("vec.png");
    // **删目录的守卫绑在测试里** ✓（辅助函数返回时不能删 ✓）
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup_dir =
        Cleanup(std::env::temp_dir().join(format!("yanshi_vector_test_{}", std::process::id())));
    let png = call(&mut ws, doc, "export_png", json!({"path": png_path}));
    assert_eq!(png["ok"], json!(true), "{png}");
    let svg_path = temp_file("vec.svg");
    let svg = call(
        &mut ws,
        doc,
        "export_svg",
        json!({"path": svg_path, "validate": true}),
    );
    assert_eq!(svg["ok"], json!(true), "{svg}");
    assert_eq!(svg["strokes_vector_native"], json!(4), "{svg}");
    assert_eq!(svg["strokes_vector"], json!(0), "{svg}");
    assert_eq!(svg["strokes_raster"], json!(0), "{svg}");

    // ⑪ 两次 export_svg 逐字节一致（确定性不回归 ✓）。
    let first_bytes = std::fs::read(&svg_path).unwrap();
    let svg_path2 = temp_file("vec2.svg");
    let svg2 = call(&mut ws, doc, "export_svg", json!({"path": svg_path2}));
    assert_eq!(svg2["ok"], json!(true), "{svg2}");
    let second_bytes = std::fs::read(&svg_path2).unwrap();
    assert_eq!(first_bytes, second_bytes, "两次导出的 SVG 必须逐字节一致");
    assert!(!first_bytes.is_empty());
}

/// 对照组：**raster** 图层的笔在 export_svg 里仍走事后转换
/// （`strokes_vector`），原生直出计数恒 0 —— 两个计数口径不许串 ✓。
#[test]
fn raster_layer_strokes_stay_on_the_posthoc_route() {
    let doc = "doc_raster_contrast";
    let mut ws = workspace(doc);
    let drawn = call(
        &mut ws,
        doc,
        "draw_stroke",
        json!({"layer_id": "layer_default", "object_id": "r1", "data": stroke(json!({}))}),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");
    let svg_path = temp_file("raster.svg");
    let svg = call(&mut ws, doc, "export_svg", json!({"path": svg_path}));
    assert_eq!(svg["ok"], json!(true), "{svg}");
    assert_eq!(svg["strokes_vector_native"], json!(0), "{svg}");
    assert_eq!(svg["strokes_vector"], json!(1), "{svg}");
}
