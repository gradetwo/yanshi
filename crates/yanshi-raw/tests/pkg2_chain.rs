//! 第二包判据（yanshi-raw 侧）：方向变换、EXIF 元数据解析、入库探针、
//! 导入计划（D5 口径 + record 与 develop 同源）。
//!
//! 形状按仓库纪律：方向用**手算期望表**（不拿实现比实现）；两份展开的
//! 关系用**置换恒等式**断言；meta 逐项对夹具写入值。

use serde_json::json;
use yanshi_raw::decode::{probe_raw, raw_meta};
use yanshi_raw::develop::apply_orientation_rgb;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng, SyntheticExif};
use yanshi_raw::{decode_bytes, plan_import, DevelopOptions, RecordContext};

/// 带完整 EXIF 的夹具（6×4，马赛克为常量场 1024）。
fn exif_fixture(orientation: u16) -> Vec<u8> {
    let mut spec = SyntheticDng {
        width: 6,
        height: 4,
        mosaic: vec![1024; 24],
        ..SyntheticDng::default()
    };
    spec.exif = Some(SyntheticExif {
        orientation: Some(orientation),
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

// ---------------------------------------------------------------------------
// 方向：手算期望（3×2 RGB 平面，像素值 = 行主序下标 ×3 起的三元组）
// ---------------------------------------------------------------------------

#[test]
fn orientation_rgb_matches_hand_computed_tables() {
    // 源平面 3×2：像素编号 0..5（值 = 编号），逐方向手算输出编号序。
    let rgb: Vec<f32> = (0..6).flat_map(|i| [i as f32, 0.0, 0.0]).collect();
    let cases: [(u32, usize, usize, [usize; 6]); 8] = [
        (1, 3, 2, [0, 1, 2, 3, 4, 5]),
        (2, 3, 2, [2, 1, 0, 5, 4, 3]),
        (3, 3, 2, [5, 4, 3, 2, 1, 0]),
        (4, 3, 2, [3, 4, 5, 0, 1, 2]),
        (5, 2, 3, [0, 3, 1, 4, 2, 5]),
        (6, 2, 3, [3, 0, 4, 1, 5, 2]),
        (7, 2, 3, [5, 2, 4, 1, 3, 0]),
        (8, 2, 3, [2, 5, 1, 4, 0, 3]),
    ];
    for (orientation, out_w, out_h, order) in cases {
        let (w, h, out) = apply_orientation_rgb(&rgb, 3, 2, orientation);
        assert_eq!((w, h), (out_w, out_h), "方向 {orientation} 的输出尺寸");
        let got: Vec<usize> = out.chunks_exact(3).map(|px| px[0] as usize).collect();
        assert_eq!(got, order, "方向 {orientation} 的像素位置");
    }
}

// ---------------------------------------------------------------------------
// 元数据：夹具写入值逐项回显（解析失败会在这里现形）
// ---------------------------------------------------------------------------

#[test]
fn raw_meta_echoes_every_fixture_value() {
    let bytes = exif_fixture(6);
    let decoded = decode_bytes(&bytes).expect("合成 DNG 应可解码");
    assert_eq!(
        decoded.file_meta.orientation, 6,
        "IFD0 Orientation 应被解析"
    );
    assert_eq!(decoded.frame.orientation, 6, "帧方向应与文件一致");
    let meta = raw_meta(&decoded);
    assert_eq!(meta.make.as_deref(), Some("YanshiFixture"));
    assert_eq!(meta.model.as_deref(), Some("Synthetic One"));
    assert_eq!(meta.lens_make.as_deref(), Some("FixtureOptics"));
    assert_eq!(meta.lens_model.as_deref(), Some("Fixture 24mm f/2.8"));
    assert_eq!(
        meta.lens.as_deref(),
        Some("FixtureOptics Fixture 24mm f/2.8")
    );
    assert_eq!(meta.iso, Some(200));
    assert!(
        (meta.exposure_time_s.unwrap() - 0.008).abs() < 1e-6,
        "曝光 1/125 应为 0.008s：{:?}",
        meta.exposure_time_s
    );
    assert!((meta.fnumber.unwrap() - 2.8).abs() < 1e-6);
    assert!((meta.focal_mm.unwrap() - 24.0).abs() < 1e-6);
    assert!((meta.focus_distance_m.unwrap() - 3.5).abs() < 1e-6);
    assert_eq!(
        meta.date_time_original.as_deref(),
        Some("2026:10:21 12:34:56")
    );
    assert_eq!(meta.orientation, 6);
    assert_eq!(meta.cfa, "RGGB");
    assert_eq!(meta.bit_depth, Some(16));
    assert_eq!(meta.black_levels, [512.0; 4]);
    assert_eq!(meta.white_levels, [16383.0; 4]);
    assert_eq!((meta.width, meta.height), (6, 4));
}

// ---------------------------------------------------------------------------
// 探针：RAW 命中、位图与伪 TIFF 不命中
// ---------------------------------------------------------------------------

#[test]
fn probe_recognises_raw_and_rejects_lookalikes() {
    let dng = exif_fixture(1);
    let probe = probe_raw(&dng).expect("合成 DNG 应被探针认作 RAW");
    assert_eq!(probe.source_format, "tiff");
    assert_eq!(probe.make.as_deref(), Some("YanshiFixture"));
    assert_eq!(probe.model.as_deref(), Some("Synthetic One"));
    // PNG 签名：不是 RAW 容器。
    assert!(probe_raw(b"\x89PNG\r\n\x1a\nrest").is_none());
    // 只有 TIFF 魔数、内容是垃圾：解码器不认 ⇒ 不许误判成 RAW。
    let mut fake_tiff = b"II\x2a\x00".to_vec();
    fake_tiff.extend_from_slice(&[0u8; 64]);
    assert!(probe_raw(&fake_tiff).is_none());
    assert!(probe_raw(b"").is_none());
}

// ---------------------------------------------------------------------------
// 展开端到端：方向 6 的产物 = 方向 1 产物的置换（恒等式），且尺寸互换、
// record 记真实方向。
// ---------------------------------------------------------------------------

#[test]
fn develop_applies_orientation_as_exact_permutation() {
    let plain = decode_bytes(&exif_fixture(1)).unwrap();
    let rotated = decode_bytes(&exif_fixture(6)).unwrap();
    let options = DevelopOptions::default();
    let ctx = RecordContext::default();
    let a = yanshi_raw::develop(&plain.frame, &options, &ctx).unwrap();
    let b = yanshi_raw::develop(&rotated.frame, &options, &ctx).unwrap();
    assert_eq!((a.width, a.height), (6, 4));
    assert_eq!((b.width, b.height), (4, 6), "方向 6 应互换宽高");
    assert_eq!(a.record.orientation_applied, 1);
    assert_eq!(b.record.orientation_applied, 6);
    // 置换恒等式：b(x,y) == a(sx,sy)，映射用内核共用表（已由手算表钉死）。
    for y in 0..b.height as usize {
        for x in 0..b.width as usize {
            let (sx, sy) = yanshi_core::orientation::source_coord(x, y, 6, 4, 6);
            let src = (sy * 6 + sx) * 3;
            let dst = (y * b.width as usize + x) * 3;
            assert_eq!(
                b.rgb_linear[dst..dst + 3],
                a.rgb_linear[src..src + 3],
                "({x},{y}) 应取自 ({sx},{sy})"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 导入计划：D5 三分支 + record 与 develop 同源
// ---------------------------------------------------------------------------

#[test]
fn plan_import_d5_branches() {
    let decoded = decode_bytes(&exif_fixture(1)).unwrap();
    // 缺省（off）+ 识别到镜头 ⇒ lens_match=off 且 warning 点名镜头与未启用。
    let plan = plan_import(&decoded, None).unwrap();
    assert_eq!(plan.record.lens_match.as_deref(), Some("off"));
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("FixtureOptics Fixture 24mm f/2.8") && w.contains("未启用")),
        "off 且有镜头时必须明确提示未启用：{:?}",
        plan.warnings
    );
    // auto ⇒ db_unavailable + warning，不阻断。
    let plan = plan_import(&decoded, Some(&json!({"lens_correction": "auto"}))).unwrap();
    assert_eq!(plan.record.lens_match.as_deref(), Some("db_unavailable"));
    assert!(
        plan.warnings.iter().any(|w| w.contains("db_unavailable")),
        "auto 无库时 warning 要说清：{:?}",
        plan.warnings
    );
    // lens_override ⇒ 响亮报错（不静默忽略）。
    let error = plan_import(
        &decoded,
        Some(&json!({"lens_override": "FixtureOptics Fixture 24mm f/2.8"})),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("lens_override"),
        "报错要指明 lens_override：{error}"
    );
}

#[test]
fn plan_record_matches_develop_record_field_by_field() {
    let decoded = decode_bytes(&exif_fixture(1)).unwrap();
    let options_json = json!({"wb": "auto", "demosaic": "bilinear", "pre_exposure_ev": 0.5});
    let plan = plan_import(&decoded, Some(&options_json)).unwrap();
    let developed = yanshi_raw::develop(
        &decoded.frame,
        &plan.options.develop,
        &RecordContext {
            source_format: decoded.file_meta.source_format.clone(),
            camera_make: decoded.file_meta.make.clone(),
            camera_model: decoded.file_meta.model.clone(),
            lens: decoded.file_meta.lens.clone(),
            lens_exif: decoded.file_meta.lens_exif.clone(),
            decoder_name: "rawler".to_string(),
            decoder_version: yanshi_raw::decode::DECODER_VERSION.to_string(),
        },
    )
    .unwrap();
    // 除 lens_match（develop 不做镜头落痕、恒 None；plan 按 D5 落 off）外逐字段一致。
    let mut from_develop = developed.record.clone();
    from_develop.lens_match = plan.record.lens_match.clone();
    assert_eq!(plan.record, from_develop);
    assert_eq!(plan.meta.make.as_deref(), Some("YanshiFixture"));
}
