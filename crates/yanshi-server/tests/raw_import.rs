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

/// 中性灰场夹具（R12 Applied 用例专用）：均匀马赛克 + 单位色彩矩阵 +
/// AsShotNeutral [1.0, 1.0, 0.85]。标定来历（探针实测，勿凭手感改常数）：
/// 缺省矩阵把灰变成强偏色（行和 0.47/1.12/0.83，Planck 距离 1.5，0 锚点），
/// 单位矩阵 + neutral [1,1,1] 得纯灰（单色跳过），neutral R 从 1.15 扫到
/// 0.3 时 (u,v) 随 R 陡移（色调曲线放大，约 3.6 倍于对数线性），R=1.00
/// 一档落成 Applied：锚点色度中位 (u,v)=(−0.084, 0.191)、散布 0.001、
/// Planck 距离 0.076 < 0.12 门，置信 high，(T, Ti)=(0.387, −0.116)；
/// R=0.95 及以下仍跳过——过零点很窄，改常数前先跑探针重标。
fn neutral_fixture_bytes() -> Vec<u8> {
    let mut spec = SyntheticDng {
        width: 24,
        height: 16,
        mosaic: vec![4096; 24 * 16],
        ..SyntheticDng::default()
    };
    spec.color_matrix1 = Some((
        [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        21, // D65
    ));
    spec.color_matrix2 = None;
    spec.as_shot_neutral = [1.0, 1.0, 0.85];
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
    // PNG/显示量化口径，color.rs 头注）。直接 develop 须用导入计划的
    // 同一份参数（Y-03：tone 导入缺省 standard，与
    // `DevelopOptions::default()` 的 Off 不同——两个缺省值分清；
    // 与高光档位 chroma/floor 的既有分歧同口径）。
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
    let direct = yanshi_render::develop_raw_source(&bytes, &expected.options.develop).unwrap();
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
        (json!({"tone": "mild"}), "tone"),
        (json!({"tone_strength": 1.5}), "tone_strength"),
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

/// B4 结构化回显（photo-bugfix-p1）：`lens_match` 与
/// `lens_correction_applied` 必须在 `import_image` 响应顶层可读（驱动
/// gating 用），而不只藏在 `raw_develop` 记录与 warnings 文本里。夹具
/// 镜头不在子集里 ⇒ `unmatched` + 未施加校正；不阻断口径不变。
#[test]
fn lens_match_and_correction_applied_are_top_level_fields() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_auto_echo",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"lens_correction": "auto"},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    assert_eq!(
        imported["lens_match"],
        json!("unmatched"),
        "顶层 lens_match 应与 record 一致：{imported}"
    );
    assert_eq!(
        imported["lens_correction_applied"],
        json!(false),
        "未匹配时不得谎称已校正：{imported}"
    );
    // 恒等式：顶层回显与落账记录同值（两处各自合法却互相矛盾是最坏的漂移）。
    let data = object_data(&workspace, "raw_auto_echo");
    assert_eq!(
        imported["lens_match"], data["raw_develop"]["lens_match"],
        "顶层与 record 的 lens_match 不一致：{imported}"
    );
}

/// B3 回显（photo-bugfix-p1）：声明 region 与方向换算后实际尺寸不匹配
/// 时，回执必须点名裁剪/留边量（只回显、不改语义）。夹具 24×16、方向 1。
#[test]
fn import_image_echoes_crop_and_padding_amounts() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let mut import = |object_id: &str, w: f64, h: f64| {
        call(
            &mut workspace,
            "import_image",
            json!({
                "layer_id": "L", "object_id": object_id,
                "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
                "region": {"x": 0.0, "y": 0.0, "w": w, "h": h},
            }),
        )
    };
    // region 小于实际：展开图超出部分被裁（ojigi 竖拍的同构：横 region
    // 套竖图，下部裁掉）。
    let small = import("raw_crop_small", 10.0, 10.0);
    assert_eq!(small["ok"], json!(true), "{small}");
    assert_eq!(
        small["developed_dimensions"],
        json!({"width": 24, "height": 16})
    );
    assert_eq!(small["cropped_px"], json!({"w": 14.0, "h": 6.0}), "{small}");
    assert_eq!(small["padded_px"], json!({"w": 0.0, "h": 0.0}), "{small}");
    // region 大于实际：声明区内没有像素的部分留白。
    let big = import("raw_crop_big", 30.0, 20.0);
    assert_eq!(big["ok"], json!(true), "{big}");
    assert_eq!(big["cropped_px"], json!({"w": 0.0, "h": 0.0}), "{big}");
    assert_eq!(big["padded_px"], json!({"w": 6.0, "h": 4.0}), "{big}");
    // region 与实际一致：两组都为 0（干净导入也有回显，不是缺字段）。
    let exact = import("raw_crop_exact", 24.0, 16.0);
    assert_eq!(exact["ok"], json!(true), "{exact}");
    assert_eq!(exact["cropped_px"], json!({"w": 0.0, "h": 0.0}), "{exact}");
    assert_eq!(exact["padded_px"], json!({"w": 0.0, "h": 0.0}), "{exact}");
    assert_eq!(
        exact["declared_region"],
        json!({"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0}),
        "{exact}"
    );
}

/// Y-09 回显（photo-bugfix-p1）：`raw_develop` 记录此前只进对象数据，
/// 回执里没有。响应里的 `raw_develop` 必须与对象数据里的同源一致。
#[test]
fn import_image_echoes_raw_develop() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let response = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_dev_echo",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    let echoed = response["raw_develop"].clone();
    assert!(
        echoed.is_object(),
        "响应必须带 raw_develop 记录：{response}"
    );
    assert_eq!(
        echoed,
        object_data(&workspace, "raw_dev_echo")["raw_develop"],
        "响应里的 raw_develop 必须与对象数据同源"
    );
    assert_eq!(echoed["lens_match"], response["lens_match"], "{response}");
}

/// Y-04 警告（photo-bugfix-p1）：RAW 导入缺省烘焙 +0.5EV 是行为，
/// 回执此前不点名。warnings 里必须有一行说清这件事。
#[test]
fn import_image_warns_about_default_raw_exposure() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let response = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_ev_warn",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
        }),
    );
    assert_eq!(response["ok"], json!(true), "{response}");
    assert!(response["default_adjustment"].is_object(), "{response}");
    let warnings = response["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings.iter().any(
            |w| w.as_str().unwrap_or_default().contains("已按 RAW 缺省曝光")
                && w.as_str().unwrap_or_default().contains("EV 烘焙")
                && w.as_str().unwrap_or_default().contains("（基调 standard）")
        ),
        "warnings 必须点名缺省曝光烘焙与缺省基调 standard：{response}"
    );
    // Y-03：显式 tone=off 时警告必须如实写 off，不许还写 standard
    // （真机验证 DSC00264 off 变体当场抓到的假话）。
    let off_response = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_ev_warn_off",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"tone": "off"},
        }),
    );
    assert_eq!(off_response["ok"], json!(true), "{off_response}");
    let off_warnings = off_response["warnings"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        off_warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("（基调 off）")),
        "tone=off 时警告必须写（基调 off）：{off_response}"
    );
    assert!(
        !off_warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("（基调 standard）")),
        "tone=off 时警告不许写（基调 standard）：{off_response}"
    );
}

/// Y-01 白边回显（photo-bugfix-p1，D-1 回显部分）：留白（padded_px > 0）
/// 时 warnings 必须点名「导出压平时呈白边」；干净导入没有这行。
#[test]
fn import_image_warns_about_white_edge_when_padded() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let mut import = |object_id: &str, w: f64, h: f64| {
        call(
            &mut workspace,
            "import_image",
            json!({
                "layer_id": "L", "object_id": object_id,
                "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
                "region": {"x": 0.0, "y": 0.0, "w": w, "h": h},
            }),
        )
    };
    let big = import("raw_edge_big", 30.0, 20.0);
    assert_eq!(big["ok"], json!(true), "{big}");
    let warnings = big["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("白边提示")),
        "留白导入必须点名白边后果：{big}"
    );
    let exact = import("raw_edge_exact", 24.0, 16.0);
    assert_eq!(exact["ok"], json!(true), "{exact}");
    let warnings = exact["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        !warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("白边提示")),
        "干净导入不该有白边提示：{exact}"
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

/// 强红场夹具（Y-05 跳过分支用）：RGGB 相位上 R 拉满、G/B 贴黑电平——
/// 有色但无近 Planck 锚点，应走 `wb_no_neutral_anchor` 跳过。
fn red_fixture_bytes() -> Vec<u8> {
    let mosaic: Vec<u16> = (0..24 * 16)
        .map(|i| {
            let x = i % 24;
            let y = i / 24;
            if x % 2 == 0 && y % 2 == 0 {
                14000
            } else {
                600
            }
        })
        .collect();
    let spec = SyntheticDng {
        width: 24,
        height: 16,
        mosaic,
        ..SyntheticDng::default()
    };
    write_synthetic_dng(&spec)
}

/// R12（Y-05）：合成 RAW 以 wb=auto 导入 ⇒ 响应带 `default_white_balance`，
/// 其 (T, Ti) 与 auto 干跑（同统计、同估计器）逐位相等；落账对象是真
/// white_balance 调整（名 + 存值 + params 三者一致）；响应里不再出现
/// 「一期等同 as_shot」；as_shot 对照不落对象、像素口径不变（见 e2e）。
#[test]
fn wb_auto_import_spawns_real_object() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let created2 = call(&mut workspace, "create_layer", json!({"layer_id": "L2"}));
    assert_eq!(created2["ok"], json!(true), "{created2}");
    // 中性夹具（非梯度夹具）：梯度夹具无近 Planck 锚点，走跳过分支；
    // Applied 用例须用中性灰场（见 neutral_fixture_bytes 的标定注记）。
    let bytes = neutral_fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();

    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_wb_auto",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"wb": "auto"},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    // 回显形状（Y-09 同源邻行 + Y-05 估计事实）：对象 id/类型/参数 +
    // 置信 + rationale（锚点数/召回率/散布点名数字）。
    let echo = imported["default_white_balance"].clone();
    assert!(
        echo.is_object(),
        "wb=auto 必须回显 default_white_balance：{imported}"
    );
    assert_eq!(echo["adjustment_type"], json!("white_balance"));
    let temperature = echo["params"]["temperature"]
        .as_f64()
        .expect("temperature 应为数字");
    let tint = echo["params"]["tint"].as_f64().expect("tint 应为数字");
    assert!(
        (-1.0..=1.0).contains(&temperature) && (-1.0..=1.0).contains(&tint),
        "估计 (T, Ti) 应在白平衡校验域内：({temperature}, {tint})"
    );
    assert!(
        !echo["params"]["temperature"].is_null() && (temperature != 0.0 || tint != 0.0),
        "夹具应有残余偏色、落成非零估计：{echo}"
    );
    assert!(
        ["high", "low"].contains(&echo["confidence"].as_str().unwrap_or("")),
        "回显须带置信 high/low：{echo}"
    );
    let rationale = echo["rationale"].as_str().unwrap_or_default();
    assert!(
        rationale.contains("锚点") && rationale.contains("召回率"),
        "rationale 须点名锚点数与召回率：{rationale}"
    );
    // 「等同 as_shot」的说法必须消失（整份响应逐字查，不只 warnings）。
    assert!(
        !format!("{imported}").contains("一期等同 as_shot"),
        "响应里不许再出现「一期等同 as_shot」：{imported}"
    );
    let warnings = imported["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap_or_default()
            .contains("已按 C1 估计落成缺省白平衡")),
        "warnings 必须点名缺省白平衡已落成：{imported}"
    );
    // 恒等式：回显与落账对象同值（两处各自合法却互相矛盾是最坏的漂移）。
    let wb_id = echo["object_id"].as_str().unwrap().to_owned();
    let data = object_data(&workspace, &wb_id);
    assert_eq!(data["adjustment_type"], json!("white_balance"));
    assert_eq!(
        data["name"],
        json!("RAW 缺省白平衡（自动生成，可改可删）"),
        "缺省对象名必须与豁免判定共用同一常量：{data}"
    );
    assert_eq!(
        data["params"], echo["params"],
        "落账 params 必须与回显逐位相等"
    );
    assert_eq!(
        data["raw_default_wb"], echo["params"],
        "存值必须等于落成估计（auto 凭它认未动过）：{data}"
    );
    // 落账 record 仍记请求值 auto（不许靠本字段猜乘子，见 frame.rs）。
    let record = &object_data(&workspace, "raw_wb_auto")["raw_develop"];
    assert_eq!(record["wb"], json!("auto"));
    // 图层对象数恒等式：位图 + 缺省曝光 + 缺省白平衡 = 3。
    let ids = layer_object_ids(&workspace);
    assert_eq!(ids.len(), 3, "wb=auto 导入后图层应恰好 3 对象：{ids:?}");
    // 逐位相等：同层干跑 auto（wb 组）的计划值必须等于落成值（同统计、
    // 同估计器；strength=1 时缩放是恒等，见 scale_params 中性值 0）。
    let dry = call(
        &mut workspace,
        "auto_retouch",
        json!({"layer_id": "L", "groups": ["wb"], "dry_run": true}),
    );
    assert_eq!(dry["ok"], json!(true), "{dry}");
    let planned = dry["planned"].as_array().cloned().unwrap_or_default();
    assert_eq!(planned.len(), 1, "干跑应恰好规划一组白平衡：{dry}");
    assert_eq!(
        planned[0]["params"], echo["params"],
        "干跑计划值必须与导入落成值逐位相等：{dry} vs {echo}"
    );

    // as-shot 对照（另一图层，keep 集不混图）：不落对象、不改口径。
    let as_shot = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L2", "object_id": "raw_wb_asshot",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"wb": "as_shot"},
        }),
    );
    assert_eq!(as_shot["ok"], json!(true), "{as_shot}");
    assert!(
        as_shot.get("default_white_balance").is_none(),
        "as_shot 导入不许落缺省白平衡：{as_shot}"
    );
    assert!(
        !format!("{as_shot}").contains("一期等同 as_shot"),
        "as_shot 响应也不许出现等同说法：{as_shot}"
    );
    let data2 = object_data(&workspace, "raw_wb_asshot");
    assert_eq!(data2["raw_develop"]["wb"], json!("as_shot"));
}

/// R12（Y-05，跳过分支）：无近 Planck 锚点的有色场 ⇒ 不落对象，
/// warnings 点名 `wb_no_neutral_anchor`，白平衡保持 as-shot；图层
/// 只有位图 + 缺省曝光两对象。G1+G2 双触的 reason 文案另由
/// tools_auto 单元测试钉死（saturated_low 组），此处钉导入侧映射。
#[test]
fn wb_auto_import_skips_honestly_without_neutral_anchor() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    let bytes = red_fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_wb_red",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"wb": "auto"},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    assert!(
        imported.get("default_white_balance").is_none(),
        "无锚点时不许落缺省白平衡：{imported}"
    );
    let warnings = imported["warnings"].as_array().cloned().unwrap_or_default();
    assert!(
        warnings.iter().any(|w| w
            .as_str()
            .unwrap_or_default()
            .contains("wb_no_neutral_anchor")),
        "跳过原因必须点名 wb_no_neutral_anchor：{imported}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap_or_default().contains("保持 as-shot")),
        "跳过时必须明说保持 as-shot：{imported}"
    );
    let ids = layer_object_ids(&workspace);
    assert_eq!(ids.len(), 2, "跳过时图层应只有位图 + 缺省曝光：{ids:?}");
}

/// R12（Y-05，不叠加）：wb=auto 导入后跑 auto（wb 组、无 force）⇒
/// 不报人工对象阻断；缺省白平衡先置零（neutralized 点名），auto 的
/// (T, Ti) 与导入落成值逐位相等；存值挺过置零，重跑仍豁免。
#[test]
fn auto_retouch_zeroes_default_white_balance_instead_of_stacking() {
    let mut workspace = memory_workspace();
    create_layer(&mut workspace);
    // 中性夹具（同测试 1 的理由）：须落成 Applied，梯度夹具走跳过。
    let bytes = neutral_fixture_bytes();
    let raw_hash = workspace.store().put(&bytes).unwrap().to_string();
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_wb_stack",
            "bitmap": {"blob_hash": raw_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 24.0, "h": 16.0},
            "raw_options": {"wb": "auto"},
        }),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");
    let echo = imported["default_white_balance"].clone();
    assert!(
        echo.is_object(),
        "前置：wb=auto 必须落成缺省白平衡：{imported}"
    );
    let wb_id = echo["object_id"].as_str().unwrap().to_owned();
    let landed_params = echo["params"].clone();

    // 首跑：无 force 也必须放行（豁免），缺省对象置零、auto 值逐位相等。
    let auto1 = call(
        &mut workspace,
        "auto_retouch",
        json!({"layer_id": "L", "groups": ["wb"]}),
    );
    assert_eq!(auto1["ok"], json!(true), "{auto1}");
    assert_eq!(
        auto1["neutralized_default_white_balance"],
        json!([wb_id]),
        "首跑必须置零缺省白平衡并点名：{auto1}"
    );
    assert_eq!(
        auto1["neutralized_default_exposure"],
        json!([]),
        "曝光组未跑，缺省曝光不应被置零：{auto1}"
    );
    let applied = auto1["applied"].as_array().cloned().unwrap_or_default();
    assert_eq!(applied.len(), 1, "wb 组应恰好落成一组：{auto1}");
    assert_eq!(applied[0]["object_id"], json!("auto_white_balance"));
    assert_eq!(
        applied[0]["params"], landed_params,
        "auto 落成值必须与导入落成值逐位相等（否则就是叠加）：{auto1}"
    );
    let zeroed = object_data(&workspace, &wb_id);
    assert_eq!(
        zeroed["params"],
        json!({"temperature": 0.0, "tint": 0.0}),
        "缺省白平衡必须被置零：{zeroed}"
    );
    assert_eq!(
        zeroed["raw_default_wb"], landed_params,
        "存值必须挺过置零（重跑认它的凭证）：{zeroed}"
    );
    // 重跑：已置零对象仍豁免（不再重复置零），auto 值仍逐位相等。
    let auto2 = call(
        &mut workspace,
        "auto_retouch",
        json!({"layer_id": "L", "groups": ["wb"]}),
    );
    assert_eq!(auto2["ok"], json!(true), "{auto2}");
    assert_eq!(
        auto2["neutralized_default_white_balance"],
        json!([]),
        "重跑时已置零对象无需再动：{auto2}"
    );
    let applied2 = auto2["applied"].as_array().cloned().unwrap_or_default();
    assert_eq!(applied2.len(), 1, "重跑仍应落成一组：{auto2}");
    assert_eq!(
        applied2[0]["params"], landed_params,
        "重跑 auto 值仍须逐位相等：{auto2}"
    );
    // 恒等式：缺省对象 (0,0) + auto 对象 (T,Ti) = 单份估计，无叠加。
    let final_data = object_data(&workspace, &wb_id);
    assert_eq!(
        final_data["params"],
        json!({"temperature": 0.0, "tint": 0.0}),
        "重跑后缺省对象仍为零：{final_data}"
    );
}
