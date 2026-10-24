//! RAW 第二包（入库链）判据，feature `raw` 开时编译：
//!   a 端到端（put_blob → import_image → 落账 record 逐项 → 渲染像素与
//!     直接 develop 一致，PNG 量化口径 ±1 LSB）；
//!   b/c raw_meta 与夹具写入值、raw_options 误用与非法值的响亮报错；
//!   e 注册表差集恰好 = raw_options；h D1 缺省曝光调整恰一条、PNG 不生成；
//!   j put_blob 通道 hash = 内容寻址 hash（与 store 直存一致）。
#![cfg(feature = "raw")]

use serde_json::{json, Value};
use std::path::PathBuf;
use yanshi_core::Bbox;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng, SyntheticExif};
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

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_raw2_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn file_workspace(root: &std::path::Path) -> Workspace {
    let mut workspace = Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_raw", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn memory_workspace() -> Workspace {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    workspace
        .create_document(
            NewDocument::new("doc_raw", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    workspace
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, "doc_raw", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn create_layer(workspace: &mut Workspace) {
    let created = call(workspace, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");
}

/// 带完整 EXIF 的 24×16 合成 DNG（马赛克为位置梯度，渲染比对才有分辨力）。
fn fixture_bytes() -> Vec<u8> {
    let mosaic: Vec<u16> = (0..24 * 16)
        .map(|i| 600 + ((i * 37) % 14000) as u16)
        .collect();
    let mut spec = SyntheticDng {
        width: 24,
        height: 16,
        mosaic,
        ..SyntheticDng::default()
    };
    spec.exif = Some(SyntheticExif {
        orientation: Some(1),
        exposure_time: Some((1, 125)),
        fnumber: Some((28, 10)),
        iso: Some(200),
        date_time_original: Some("2026:10:21 12:34:56".to_string()),
        subject_distance: Some((35, 10)),
        focal_length: Some((24, 1)),
        lens_make: Some("FixtureOptics".to_string()),
        lens_model: Some("Fixture 24mm f/2.8".to_string()),
    });
    write_synthetic_dng(&spec)
}

fn object_data(workspace: &Workspace, object_id: &str) -> Value {
    let state = workspace.document("doc_raw").unwrap().state().clone();
    state
        .objects
        .get(object_id)
        .map(|object| object.data.clone())
        .unwrap_or(Value::Null)
}

fn layer_object_ids(workspace: &Workspace) -> Vec<String> {
    let state = workspace.document("doc_raw").unwrap().state().clone();
    state
        .objects_in_layer("L")
        .iter()
        .map(|object| object.id.clone())
        .collect()
}

#[test]
fn end_to_end_raw_import_records_metadata_and_renders_like_direct_develop() {
    let root = temp_dir("e2e");
    let _cleanup_1 = Cleanup(root.clone());
    let bytes = fixture_bytes();
    std::fs::write(root.join("shot.dng"), &bytes).unwrap();
    let mut workspace = file_workspace(&root);
    create_layer(&mut workspace);

    // 入库（put_blob 的 path 通道）：字节被识别为 RAW、原样存入。
    let absolute = root.join("shot.dng").to_string_lossy().into_owned();
    let put = call(
        &mut workspace,
        "put_blob",
        json!({"path": absolute, "mime_type": "image/x-dcraw"}),
    );
    assert_eq!(put["ok"], json!(true), "{put}");
    assert_eq!(put["mime_type"], json!("image/x-yanshi-rawsrc"), "{put}");
    assert_eq!(put["detected"]["format"], json!("raw"), "{put}");
    assert_eq!(put["detected"]["make"], json!("YanshiFixture"), "{put}");
    let blob_hash = put["blob_hash"].as_str().unwrap().to_owned();
    // 判据 j（put_blob 侧）：hash = 内容寻址结果（与 store 直存同字节同 hash），
    // 且存入的字节与源逐字节相同。
    let direct_hash = workspace.store().put(&bytes).unwrap().to_string();
    assert_eq!(blob_hash, direct_hash, "put_blob 的 hash 必须等于内容寻址");
    let parsed: yanshi_core::BlobHash = blob_hash.parse().unwrap();
    let stored = workspace.store().get(&parsed).unwrap();
    assert_eq!(stored, bytes, "存入字节必须与源逐字节相同（原样入库）");

    // 导入：响应带 raw_meta + D1 缺省调整；对象落账 raw_develop。
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_1",
            "bitmap": {"blob_hash": blob_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    assert_eq!(imported["blob_decodable"], json!(true), "{imported}");
    // 判据 b（响应侧）：raw_meta 与夹具写入值逐项相等（全量逐项在
    // yanshi-raw 的 pkg2_chain 测试里钉死，这里钉响应真的带同一份）。
    let meta = &imported["raw_meta"];
    assert_eq!(meta["make"], json!("YanshiFixture"), "{imported}");
    assert_eq!(meta["model"], json!("Synthetic One"), "{imported}");
    assert_eq!(meta["lens"], json!("FixtureOptics Fixture 24mm f/2.8"));
    assert_eq!(meta["iso"], json!(200));
    assert_eq!(meta["date_time_original"], json!("2026:10:21 12:34:56"));
    assert_eq!(meta["orientation"], json!(1));
    assert_eq!(meta["cfa"], json!("RGGB"));
    // D5：缺省 off + 识别到镜头 ⇒ warnings 必须点名镜头与未启用。
    let warnings = imported["warnings"].as_array().expect("应有 warnings");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("FixtureOptics")
                && w.as_str().unwrap_or("").contains("未启用")),
        "D5 提示缺失：{imported}"
    );
    // 判据 h：响应回显缺省调整（值与 README 定版 +0.5 一致）。
    assert_eq!(
        imported["default_adjustment"]["adjustment_type"],
        json!("exposure")
    );
    assert_eq!(imported["default_adjustment"]["params"]["ev"], json!(0.5));
    let adjustment_id = imported["default_adjustment"]["object_id"]
        .as_str()
        .unwrap()
        .to_owned();

    // 落账 record 与导入计划逐项相等（同一份解析，禁止两处各算）。
    let data = object_data(&workspace, "raw_1");
    let record = &data["raw_develop"];
    let decoded = yanshi_raw::decode_bytes(&bytes).unwrap();
    let mut expected = yanshi_raw::plan_import(&decoded, None).unwrap();
    expected.record.source_blob = Some(blob_hash.clone());
    let expected_json = serde_json::to_value(&expected.record).unwrap();
    assert_eq!(*record, expected_json, "落账 record 必须与导入计划逐项相等");
    assert_eq!(record["lens_match"], json!("off"));
    assert_eq!(record["orientation_applied"], json!(1));

    // 判据 h（状态侧）：图层上恰好一条调整对象、原生形态、可查。
    let ids = layer_object_ids(&workspace);
    assert_eq!(ids.len(), 2, "图层应只有位图对象 + 一条缺省调整：{ids:?}");
    let adjustment = object_data(&workspace, &adjustment_id);
    assert_eq!(adjustment["adjustment_type"], json!("exposure"));
    assert_eq!(adjustment["params"]["ev"], json!(0.5));

    // 删掉缺省调整（D1 可删）后渲染：像素与直接 develop 一致（±1 LSB，
    // PNG/显示量化口径，color.rs 头注）。
    let deleted = call(
        &mut workspace,
        "delete_object",
        json!({"object_id": adjustment_id}),
    );
    assert_eq!(deleted["ok"], json!(true), "{deleted}");
    let (w, h, rgba) = workspace
        .render_region_raw("doc_raw", Bbox::new(0.0, 0.0, 24.0, 16.0))
        .unwrap();
    assert_eq!((w, h), (24, 16));
    let direct =
        yanshi_render::develop_raw_source(&bytes, &yanshi_raw::DevelopOptions::default()).unwrap();
    for y in 0..16usize {
        for x in 0..24usize {
            let src = (y * 24 + x) * 3;
            let dst = (y * 24 + x) * 4;
            for channel in 0..3 {
                let want = yanshi_render::color::linear_to_byte(direct.rgb_linear[src + channel]);
                let got = rgba[dst + channel];
                assert!(
                    (i32::from(want) - i32::from(got)).abs() <= 1,
                    "像素 ({x},{y}) 通道 {channel} 与直接 develop 不一致：渲染 {got} vs develop {want}"
                );
            }
            assert_eq!(rgba[dst + 3], 255, "RAW 补丁应不透明");
        }
    }
}

#[test]
fn raw_options_on_non_raw_blob_and_illegal_values_are_loud_errors() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    // 一张普通 PNG blob（1×1 标准测试图）。
    let png = yanshi_server::base64::decode(
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
    )
    .unwrap();
    let png_hash = workspace.store().put(&png).unwrap().to_string();
    let misused = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "png_1",
            "bitmap": {"blob_hash": png_hash, "size": png.len(), "mime_type": "image/png"},
            "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
            "raw_options": {"wb": "auto"},
        }),
    );
    assert_eq!(misused["ok"], json!(false), "{misused}");
    assert!(
        format!("{misused}").contains("raw_options"),
        "报错必须指明 raw_options：{misused}"
    );

    // RAW blob + 非法枚举 / 越界 EV / lens_override：逐项响亮报错。
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    for (options, needle) in [
        (json!({"wb": "daylight"}), "wb"),
        (json!({"pre_exposure_ev": 4.5}), "pre_exposure_ev"),
        (
            json!({"lens_override": "FixtureOptics Fixture 24mm f/2.8"}),
            "lens_override",
        ),
    ] {
        let rejected = call(
            &mut workspace,
            "import_image",
            json!({
                "layer_id": "L", "object_id": "raw_bad",
                "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
                "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
                "raw_options": options,
            }),
        );
        assert_eq!(rejected["ok"], json!(false), "{rejected}");
        assert!(
            format!("{rejected}").contains(needle),
            "报错必须指明 {needle}：{rejected}"
        );
    }

    // PNG 正常导入不生成缺省调整、不带 raw_meta（判据 h 的反面）。
    let png_import = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "png_ok",
            "bitmap": {"blob_hash": png_hash, "size": png.len(), "mime_type": "image/png"},
            "region": {"x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0},
        }),
    );
    assert_eq!(png_import["ok"], json!(true), "{png_import}");
    assert!(png_import.get("raw_meta").is_none(), "{png_import}");
    assert!(
        png_import.get("default_adjustment").is_none(),
        "{png_import}"
    );
    let ids = layer_object_ids(&workspace);
    assert_eq!(
        ids,
        vec!["png_ok".to_string()],
        "PNG 导入不应生成调整对象：{ids:?}"
    );
}

#[test]
fn lens_correction_auto_records_unmatched_without_blocking() {
    // 第四包起镜头库已接入：夹具镜头（FixtureOptics）不在打包子集
    // 里 ⇒ lens_match=unmatched。第二包时此处断言 db_unavailable，
    // 行为变化来自第四包落地，断言随之更新；不阻断导入的口径不变。
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_auto",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"lens_correction": "auto", "pre_exposure_ev": 0.25},
        }),
    );
    assert_eq!(
        imported["ok"],
        json!(true),
        "auto 未匹配不应阻断导入：{imported}"
    );
    let data = object_data(&workspace, "raw_auto");
    assert_eq!(data["raw_develop"]["lens_match"], json!("unmatched"));
    assert_eq!(data["raw_develop"]["pre_exposure_ev"], json!(0.25));
    let warnings = imported["warnings"].as_array().expect("应有 warnings");
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("unmatched")),
        "auto 的 warning 必须说清 unmatched：{imported}"
    );
}

#[test]
fn registry_diff_is_exactly_the_raw_options_group() {
    let registry = registry();
    let spec = registry.get("import_image").expect("import_image 应注册");
    let mut names: Vec<&str> = spec.params.iter().map(|param| param.name).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "bitmap",
            "layer_id",
            "medium",
            "object_id",
            "raw_options",
            "region"
        ],
        "feature 开时 import_image 参数差集必须恰好是 raw_options（基线五项 + 它）"
    );
}
