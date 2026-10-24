//! 期 4 D1 集成测试：频率分离 / 减淡加深（走真实工具注册表 + 落盘工作区）。
//!
//! 与单元测试的分工：单元测试钉住烘焙公式与自检界；这里钉住工具落成
//! ——组结构、渲染重建、空层拒绝零原子、蒙版全黑行为、灰层填充、幂等。

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_retouch_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn workspace(root: &std::path::Path, doc: &str, width: u32, height: u32) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new(doc, width, height),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, doc: &str, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test").with_owner(true);
    registry().call(&mut ctx, tool, &args)
}

/// 合成源图（64×48，全不透明）：水平渐变 + 高频竖条纹 + 纯色块。
fn synthetic(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height as usize {
        for x in 0..width as usize {
            let base = (y * width as usize + x) * 4;
            let gradient = (x * 255 / width as usize) as u8;
            let stripe = if (x / 2) % 2 == 0 { 255 } else { 0 };
            let (r, g, b) = if x >= 48 && y >= 32 {
                (200, 30, 30)
            } else if y < 8 {
                (gradient, stripe, 128)
            } else {
                (gradient, 255 - gradient, 90)
            };
            pixels[base] = r;
            pixels[base + 1] = g;
            pixels[base + 2] = b;
            pixels[base + 3] = 255;
        }
    }
    pixels
}

/// 建源图层并导入合成图，返回图层 id。
fn setup_source(workspace: &mut Workspace, doc: &str, width: u32, height: u32) -> String {
    let layer = call(
        workspace,
        doc,
        "create_layer",
        json!({"layer_id": "src", "name": "源图"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let pixels = synthetic(width, height);
    let hash = workspace.put_blob(&pixels).expect("放源图 blob");
    let imported = call(
        workspace,
        doc,
        "import_image",
        json!({
            "layer_id": "src",
            "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
            "region": {"x": 0, "y": 0, "w": width, "h": height},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    "src".to_owned()
}

fn full_bbox(width: u32, height: u32) -> Bbox {
    Bbox::new(0.0, 0.0, width as f64, height as f64)
}

/// 频率分离端到端：组渲染（源图层隐藏后）与源图逐像素比对，
/// 界与单元测试的实测刻画界一致（引擎渲染多走一层瓦片 f16，
/// 故亮区界放宽到 4 LSB——若实测 tighter 再收紧，见注释）。
#[test]
fn frequency_separation_reconstructs_source() {
    let root = temp_dir("fs");
    let doc = "doc_fs";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let (_, _, reference) = workspace
        .render_region_raw_layer(doc, full_bbox(width, height), &src)
        .expect("源图层应当能渲染");
    let result = call(
        &mut workspace,
        doc,
        "frequency_separation",
        json!({"layer_id": src, "radius": 8.0}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    let group_id = result["group_id"].as_str().expect("回组 id").to_owned();
    let low_id = result["low_layer_id"].as_str().expect("回低频层 id");
    let high_id = result["high_layer_id"].as_str().expect("回高频层 id");
    // 组结构：组内恰好 4 层（自下而上：低频、低频绘画、高频、高频绘画）。
    let state = workspace.document(doc).unwrap().state().clone();
    let children: Vec<_> = state
        .layers
        .values()
        .filter(|layer| layer.parent_id.as_deref() == Some(group_id.as_str()))
        .collect();
    assert_eq!(children.len(), 4, "组内应恰好 4 层");
    let high_layer = state.layers.get(high_id).expect("高频层存在");
    assert_eq!(
        high_layer.blend_mode, "linear_light",
        "高频层必须是 linear_light"
    );
    assert!(state.layers.get(low_id).is_some(), "低频层存在");
    // 源图层隐藏后整幅渲染 = 组单独渲染，与源图比对。
    let hidden = call(
        &mut workspace,
        doc,
        "update_layer",
        json!({"layer_id": src, "patch": {"visible": false}}),
    );
    assert_eq!(hidden["ok"], json!(true), "{hidden}");
    let (_, _, rendered) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("组应当能渲染");
    assert_eq!(rendered.len(), reference.len());
    let mut max_error = 0i32;
    let mut bright_max_error = 0i32;
    let mut sum_error = 0i64;
    for (index, (a, b)) in rendered.iter().zip(reference.iter()).enumerate() {
        // alpha 通道不参与重建比较（组 alpha 规则见工具说明）。
        if index % 4 == 3 {
            continue;
        }
        let error = (*a as i32 - *b as i32).unsigned_abs() as i32;
        max_error = max_error.max(error);
        if *b >= 128 {
            bright_max_error = bright_max_error.max(error);
        }
        sum_error += error as i64;
    }
    let counted = (rendered.len() / 4 * 3) as i64;
    assert!(max_error <= 20, "组渲染最大误差超刻画界：{max_error}");
    assert!(
        bright_max_error <= 4,
        "组渲染中高调最大误差超引擎界：{bright_max_error}"
    );
    assert!(
        sum_error as f64 / counted as f64 <= 3.0,
        "组渲染均值误差超刻画界：{}",
        sum_error as f64 / counted as f64
    );
}

/// 空层调用 ⇒ precondition_failed，且 head_seq 不变（零原子落账）。
/// 变异：错误被吞（返回 ok）⇒ 本条红；原子偷偷落账 ⇒ head_seq 变 ⇒ 红。
#[test]
fn frequency_separation_rejects_empty_layer() {
    let root = temp_dir("fs_empty");
    let doc = "doc_fs_empty";
    let mut workspace = workspace(&root, doc, 32, 32);
    let layer = call(
        &mut workspace,
        doc,
        "create_layer",
        json!({"layer_id": "empty", "name": "空层"}),
    );
    assert_eq!(layer["ok"], json!(true), "{layer}");
    let before = workspace.document(doc).unwrap().state().head_seq;
    let result = call(
        &mut workspace,
        doc,
        "frequency_separation",
        json!({"layer_id": "empty", "radius": 8.0}),
    );
    assert_eq!(result["ok"], json!(false), "空层必须拒绝：{result}");
    assert_eq!(
        result["error_code"],
        json!("precondition_failed"),
        "空层拒绝必须是 precondition_failed：{result}"
    );
    assert!(
        result.to_string().contains("没有可分离的像素"),
        "错误要说清缺什么：{result}"
    );
    let after = workspace.document(doc).unwrap().state().head_seq;
    assert_eq!(
        before, after,
        "拒绝时零原子落账：head_seq {before} → {after}"
    );
}

/// D&B curves 变体：两层、两 curves 对象（EOTF 转换后的中点）、
/// 两蒙版全黑（行为证明：蒙版下曲线完全不生效，整幅渲染 = 源图）。
#[test]
fn dodge_burn_curves_mode_creates_masked_pair() {
    let root = temp_dir("db_curves");
    let doc = "doc_db_curves";
    let (width, height) = (64, 48);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let (_, _, reference) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("源图应当能渲染");
    let result = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    assert_eq!(result["already_exists"], json!(false));
    let layer_ids = result["layer_ids"].as_array().expect("回层 id").len();
    let object_ids = result["object_ids"].as_array().expect("回对象 id").len();
    let mask_ids = result["mask_ids"].as_array().expect("回蒙版 id").len();
    assert_eq!(layer_ids, 2, "curves 变体应建 2 层");
    assert_eq!(object_ids, 2, "curves 变体应建 2 个 curves 对象");
    assert_eq!(mask_ids, 2, "curves 变体应建 2 张蒙版");
    // 对象参数：中点是 EOTF 转换后的线性点（不是显示值原样）。
    let state = workspace.document(doc).unwrap().state().clone();
    let mut mids = Vec::new();
    for object in state.alive_objects() {
        if object.data["adjustment_type"] == json!("curves") {
            let points = object.data["params"]["points"].as_array().unwrap();
            mids.push((
                points[1][0].as_f64().unwrap(),
                points[1][1].as_f64().unwrap(),
            ));
        }
    }
    assert_eq!(mids.len(), 2, "应恰好 2 条 curves 曲线");
    // 显示 0.5 → 线性 0.2140；减淡 0.62 → 0.3423；加深 0.38 → 0.1192（±0.002）。
    for (x, y) in &mids {
        assert!(
            (x - 0.2140).abs() < 0.002,
            "曲线中点 x 应为 decode(0.5)：{x}"
        );
        assert!(
            (y - 0.3423).abs() < 0.002 || (y - 0.1192).abs() < 0.002,
            "曲线中点 y 应为 decode(0.62) 或 decode(0.38)：{y}"
        );
    }
    // 蒙版结构：全画布 rect + invert ⇒ 覆盖率处处为 0，且无涂抹补丁。
    for mask in state.masks.values() {
        if mask.is_deleted() {
            continue;
        }
        assert_eq!(mask.invert, true, "D&B 蒙版必须 invert（全黑起点）");
        assert!(mask.patches.is_empty(), "新蒙版不应有涂抹补丁");
    }
    // 行为证明：蒙版全黑 ⇒ 曲线零生效 ⇒ 整幅渲染与源图逐字节相同。
    let (_, _, rendered) = workspace
        .render_region_raw(doc, full_bbox(width, height))
        .expect("D&B 组应当能渲染");
    assert_eq!(rendered, reference, "全黑蒙版下曲线必须零生效");
}

/// D&B gray 变体：灰层填充 128 灰、blend_mode=soft_light。
#[test]
fn dodge_burn_gray_mode_creates_gray_layer() {
    let root = temp_dir("db_gray");
    let doc = "doc_db_gray";
    let (width, height) = (32, 32);
    let mut workspace = workspace(&root, doc, width, height);
    let src = setup_source(&mut workspace, doc, width, height);
    let result = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "gray"}),
    );
    assert_eq!(result["ok"], json!(true), "{result}");
    let layer_id = result["layer_ids"][0]
        .as_str()
        .expect("回灰层 id")
        .to_owned();
    let state = workspace.document(doc).unwrap().state().clone();
    let gray_layer = state.layers.get(&layer_id).expect("灰层存在");
    assert_eq!(gray_layer.blend_mode, "soft_light", "灰层必须是 soft_light");
    // 填充值断言不能直接单层渲染：单层渲染会把 soft_light 合成到底色上
    // （128 灰是近中性色 ⇒ 结果 ≈ 底色，不是 128）。改走行为断言：
    // 先把 blend 切回 normal 再单层渲染 ⇒ 此时应逐像素都是 128 灰。
    let unblended = call(
        &mut workspace,
        doc,
        "update_layer",
        json!({"layer_id": layer_id, "patch": {"blend_mode": "normal"}}),
    );
    assert_eq!(unblended["ok"], json!(true), "{unblended}");
    let (_, _, pixels) = workspace
        .render_region_raw_layer(doc, full_bbox(width, height), &layer_id)
        .expect("灰层应当能渲染");
    assert!(
        pixels.chunks_exact(4).all(|p| p == [128, 128, 128, 255]),
        "灰层每个像素都应是 128 灰不透明"
    );
}

/// D&B 幂等：二次调用返回 already_exists 且 head_seq 不变、id 相同。
#[test]
fn dodge_burn_setup_is_idempotent() {
    let root = temp_dir("db_idem");
    let doc = "doc_db_idem";
    let mut workspace = workspace(&root, doc, 32, 32);
    let src = setup_source(&mut workspace, doc, 32, 32);
    let first = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(first["ok"], json!(true), "{first}");
    let before = workspace.document(doc).unwrap().state().head_seq;
    let second = call(
        &mut workspace,
        doc,
        "dodge_burn_setup",
        json!({"layer_id": src, "mode": "curves"}),
    );
    assert_eq!(second["ok"], json!(true), "{second}");
    assert_eq!(
        second["already_exists"],
        json!(true),
        "二次调用必须 already_exists"
    );
    assert_eq!(
        second["group_id"], first["group_id"],
        "二次调用必须回同一组"
    );
    assert_eq!(
        second["layer_ids"], first["layer_ids"],
        "二次调用必须回同一层"
    );
    let after = workspace.document(doc).unwrap().state().head_seq;
    assert_eq!(before, after, "幂等复用时零原子落账");
}
