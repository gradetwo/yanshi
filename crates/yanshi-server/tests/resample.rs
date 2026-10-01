//! `resample` ✓（设计 777 ✓）。
//!
//! **设计只给了名字 ⇒ 这里记录选择** ✓（与 `yanshi_core::resample` 的模块说明一致 ✓）：
//! 对象 = `raster_patch` ✓；只接受**原始 RGBA** ✓（其它 mime 明确拒绝 ✗）；
//! 产出**新 blob + `supersede`** ✓ ⇒ **非破坏** ✓（旧原子仍在日志里 ✓，可撤销、可回放 ✓）；
//! **缺省 `bilinear`** ✓（介质是连续调 ✓），像素画请显式选 `nearest` ✓。
//!
//! 算法本身（最近邻 / 双线性 / 中心对齐 / 坏输入）由内核的单元测试覆盖 ✓；
//! **本文件只验管道** ✓：blob 先落库 ✓、描述符保留 ✓、来源记录 ✓、参数校验 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_rs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_rs", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
}

fn put_pattern(workspace: &mut Workspace, w: u32, h: u32) -> (String, Vec<u8>) {
    let mut bytes = Vec::new();
    for y in 0..h {
        for x in 0..w {
            bytes.extend_from_slice(&[(x * 90) as u8, (y * 120) as u8, 30, 255]);
        }
    }
    let hash = workspace
        .store()
        .put(&bytes)
        .expect("入库应成功")
        .to_string();
    (hash, bytes)
}

fn import(
    workspace: &mut Workspace,
    blob: &str,
    object_id: &str,
    w: u32,
    h: u32,
    medium: bool,
) -> serde_json::Value {
    let mut args = json!({"layer_id": "L", "object_id": object_id,
        "bitmap": {"blob_hash": blob, "size": (w * h * 4) as u64, "mime_type": "image/x-yanshi-raw"},
        "region": {"x": 0.0, "y": 0.0, "w": w as f64, "h": h as f64}});
    if medium {
        // 介质描述符 ✓：设计 11.1 要求它随原子记录 ✓ ⇒ 重采样时必须**原样保留** ✓。
        args["medium"] = json!({"id": "oil", "version": 2});
    }
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "import_image", &args)
}

fn data_of(workspace: &mut Workspace, object_id: &str) -> serde_json::Value {
    let state = workspace.document_mut("doc_rs").unwrap().state().clone();
    state
        .objects
        .get(object_id)
        .map(|object| object.data.clone())
        .unwrap_or(serde_json::Value::Null)
}

/// **放大 2 倍** ✓：新 blob 落库 ✓、尺寸更新 ✓、来源与滤镜记录 ✓、介质描述符保留 ✓。
#[test]
fn resampling_upscales_into_a_new_blob_and_keeps_provenance() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let (blob, _bytes) = put_pattern(&mut workspace, 2, 2);
    let imported = import(&mut workspace, &blob, "obj_rs", 2, 2, true);
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let resampled = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "resample",
            &json!({"object_id": "obj_rs", "scale": 2.0, "filter": "nearest"}),
        )
    };
    assert_eq!(resampled["ok"], json!(true), "{resampled}");
    assert_eq!(resampled["target"]["width"], json!(4), "{resampled}");
    assert_eq!(resampled["target"]["height"], json!(4), "{resampled}");
    let data = data_of(&mut workspace, "obj_rs");
    assert_eq!(data["width"], json!(4), "对象尺寸应更新：{data}");
    assert_eq!(data["height"], json!(4), "{data}");
    // **介质描述符原样在** ✓（设计 11.1 ✓）。
    assert_eq!(
        data["medium"]["id"],
        json!("oil"),
        "介质描述符必须保留：{data}"
    );
    assert_eq!(data["medium"]["version"], json!(2), "{data}");
    // **来源可追溯** ✓：后来人能看出"这份像素不是插件当初画的那份" ✓。
    assert_eq!(data["resampled_from"]["filter"], json!("nearest"), "{data}");
    assert_eq!(data["resampled_from"]["blob_hash"], json!(blob), "{data}");
    // **新 blob 真的落库了** ✓（blob 先行 ✓），且大小对 ✓。
    let new_hash: yanshi_core::BlobHash = data["bitmap"]["blob_hash"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .expect("新哈希应可解析");
    assert_ne!(new_hash.to_string(), blob, "应当是**新的**内容哈希 ✓");
    let bytes = workspace
        .store()
        .get(&new_hash)
        .expect("新 blob 应能取回 ✓");
    assert_eq!(bytes.len(), 4 * 4 * 4, "新位图应是 4×4×4 字节 ✓");
    // **旧 blob 仍在** ✓（非破坏 ✓：日志里那条原子还引用着它 ✓）。
    let old_hash: yanshi_core::BlobHash = blob.parse().expect("旧哈希可解析");
    assert!(
        workspace.store().exists(&old_hash),
        "旧 blob 不许被删（可撤销、可回放要靠它）✓"
    );
}

/// **坏输入一律明确拒绝** ✗（不猜、不静默 ✓）。
#[test]
fn resample_validates_its_inputs() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let (blob, _bytes) = put_pattern(&mut workspace, 4, 4);
    let imported = import(&mut workspace, &blob, "obj_ok", 4, 4, false);
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        ("缺尺寸", json!({"object_id": "obj_ok"}), "width"),
        (
            "未知滤镜",
            json!({"object_id": "obj_ok", "scale": 2.0, "filter": "wat"}),
            "滤镜",
        ),
        (
            "对象不存在",
            json!({"object_id": "nope", "scale": 2.0}),
            "不存在",
        ),
        (
            "同尺寸最近邻",
            json!({"object_id": "obj_ok", "width": 4, "height": 4, "filter": "nearest"}),
            "恒等",
        ),
    ];
    for (name, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "resample", &args)
        };
        assert_eq!(refused["ok"], json!(false), "{name} 应被拒绝：{refused}");
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "{name} 的错误应提到「{expected}」，实测：{detail}"
        );
    }
    // **非光栅对象** ✓（拿一条笔迹来试 ✓）。
    {
        let mut ctx = context(&mut workspace);
        let drawn = registry().call(
            &mut ctx,
            "draw_stroke",
            &json!({
            "layer_id": "L",
            "data": {"points": [[4.0, 4.0], [40.0, 40.0]], "size": 6.0,
                     "color": {"r": 10, "g": 10, "b": 10, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let stroke_id = {
        let state = workspace.document_mut("doc_rs").unwrap().state().clone();
        state
            .objects
            .values()
            .find(|object| object.object_type != yanshi_core::ObjectType::RasterPatch)
            .map(|object| object.id.clone())
            .expect("应能找到一个非光栅对象 ✓")
    };
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "resample",
            &json!({"object_id": stroke_id, "scale": 2.0}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "非光栅对象应被拒绝：{refused}");
    assert!(
        refused["context"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("光栅"),
        "错误应说明只作用于光栅对象：{refused}"
    );
}
