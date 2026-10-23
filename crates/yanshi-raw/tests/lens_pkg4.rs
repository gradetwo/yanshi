//! 第四包判据（镜头校正）：Lensfun 解析/匹配/插值的集成面、三项
//! 校正全链、文件自带校正逐项优先、lens_override 真实现、off 逐位
//! 一致。形状按仓库纪律：warp 用**线性斜坡 + 公式手算**作 oracle
//! （双线性插值对线性函数精确，输出应恰等于源坐标），不用实现比实现。

use serde_json::json;
use yanshi_raw::develop::warp_rgb;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng, SyntheticExif};
use yanshi_raw::frame::{CalibrationMatrix, FileLensCorrection, WarpRectilinearSet};
use yanshi_raw::lens::{
    auto_crop_scale, build_plan_and_profile, plan_from_applied, vignette_gain_field,
};
use yanshi_raw::lens_db::{bundled_db, find_lens, parse_lensfun_xml, LensDb};
use yanshi_raw::{
    decode_bytes, develop, develop_with_lens, plan_import_with_db, CfaGrid, CfaPattern, DecodedRaw,
    DevelopOptions, HighlightMode, LensCorrectionPlan, LensExecution, RawFileMeta, RawFrame,
    RecordContext, VignetteModel, WarpModel,
};

fn realistic_calibrations() -> Vec<CalibrationMatrix> {
    vec![
        CalibrationMatrix {
            illuminant_kelvin: Some(2856.0),
            xyz_to_cam: [
                [0.7683, -0.3276, 0.0299],
                [-0.3630, 1.0892, 0.3161],
                [-0.0162, 0.0671, 0.7133],
            ],
        },
        CalibrationMatrix {
            illuminant_kelvin: Some(6504.0),
            xyz_to_cam: [
                [0.6640, -0.1847, -0.0503],
                [-0.5238, 1.3010, 0.2474],
                [-0.0993, 0.1673, 0.6527],
            ],
        },
    ]
}

fn base_frame(width: usize, height: usize) -> RawFrame {
    RawFrame {
        width,
        height,
        mosaic: vec![0.0; width * height],
        cfa: CfaGrid::from_bayer(CfaPattern::Rggb),
        black_levels: vec![512.0; 4],
        white_levels: vec![16383.0; 4],
        wb_multipliers: Some([1.72, 1.0, 1.39]),
        as_shot_neutral: None,
        color_matrices: realistic_calibrations(),
        baseline_exposure_ev: None,
        orientation: 1,
    }
}

/// 与 pkg2 夹具同口径的 EXIF DNG（镜头 FixtureOptics Fixture 24mm）。
fn exif_fixture_bytes() -> Vec<u8> {
    let mut spec = SyntheticDng {
        width: 6,
        height: 4,
        mosaic: vec![1024; 24],
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

/// 夹具镜头的库（手写小库：参数取整档值，便于逐项对账）。
fn fixture_db() -> LensDb {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<lensdatabase version="2">
    <lens>
        <maker>FixtureOptics</maker>
        <model>FixtureOptics Fixture 24mm f/2.8</model>
        <mount>Fixture</mount>
        <cropfactor>1.0</cropfactor>
        <calibration>
            <distortion model="ptlens" focal="24" a="0.01" b="-0.03" c="0.02"/>
            <tca model="linear" focal="24" vr="1.002" vb="0.998"/>
            <vignetting model="pa" focal="24" aperture="2.8" distance="3.5" k1="-0.30" k2="0.05" k3="0.0"/>
        </calibration>
    </lens>
</lensdatabase>"#;
    let (lenses, skipped) = parse_lensfun_xml(xml);
    assert_eq!(skipped, 0);
    LensDb {
        source: "test-fixture".to_string(),
        version: "test-1".to_string(),
        lenses,
        skipped_entries: 0,
    }
}

// ---------------------------------------------------------------------------
// 打包子集与匹配（判据：识别 → 匹配）
// ---------------------------------------------------------------------------

#[test]
fn bundled_subset_matches_common_lenses() {
    let db = bundled_db();
    assert_eq!(db.lenses.len(), 6, "打包子集应含 6 支摘录镜头");
    // EXIF 常见写法：无厂商前缀、无空格。
    let (entry, via) = find_lens(&db, None, Some("EF-S18-135mm f/3.5-5.6 IS USM"))
        .expect("EF-S 18-135 应在子集中命中");
    assert!(entry.model.contains("18-135"));
    assert!(via == "exact" || via == "contains");
    // 完整厂商+型号写法。
    let (_, via) = find_lens(&db, Some("Canon"), Some("Canon EF 24-105mm f/4L IS USM"))
        .expect("24-105 应命中");
    assert_eq!(via, "exact");
    assert!(find_lens(&db, None, Some("不存在的厂商 12mm f/1.4")).is_none());
}

#[test]
fn ambiguous_containment_is_unmatched() {
    // 两支同系列变焦：只给系列前缀 ⇒ 包含命中两支 ⇒ 歧义不认。
    let xml = r#"<lensdatabase version="2">
    <lens><maker>Acme</maker><model>Acme Zoom 24-70mm f/2.8</model></lens>
    <lens><maker>Acme</maker><model>Acme Zoom 24-105mm f/4</model></lens>
    </lensdatabase>"#;
    let (lenses, _) = parse_lensfun_xml(xml);
    let db = LensDb {
        source: "t".into(),
        version: "t".into(),
        lenses,
        skipped_entries: 0,
    };
    assert!(find_lens(&db, None, Some("Acme Zoom")).is_none());
    assert!(find_lens(&db, None, Some("Acme Zoom 24-70mm f/2.8")).is_some());
}

// ---------------------------------------------------------------------------
// 全链：auto 匹配 → plan/profile 落账 → 带校正展开（判据：全链走通 +
// record 与 develop 同源 + 自动裁剪落账与现算同值）
// ---------------------------------------------------------------------------

#[test]
fn auto_full_chain_matches_corrects_and_records() {
    let decoded = decode_bytes(&exif_fixture_bytes()).unwrap();
    let db = fixture_db();
    let plan = plan_import_with_db(
        &decoded,
        Some(&json!({"lens_correction": "auto"})),
        Some(&db),
    )
    .unwrap();
    assert_eq!(plan.record.lens_match.as_deref(), Some("matched"));
    let profile = plan
        .record
        .applied_lens_profile
        .as_ref()
        .expect("matched 必须有 applied profile");
    assert_eq!(profile.lens_id, "FixtureOptics Fixture 24mm f/2.8");
    assert_eq!(profile.distortion_ptlens, Some([0.01, -0.03, 0.02]));
    assert_eq!(profile.vignetting_pa, Some([-0.30, 0.05, 0.0]));
    assert_eq!(profile.sources["distortion"], json!("lensfun"));
    assert_eq!(profile.sources["vignetting"], json!("lensfun"));
    assert_eq!(profile.sources["tca"], json!("lensfun"));
    assert_eq!(profile.focal_mm_effective, Some(24.0));
    let lens_plan = plan.lens_plan.as_ref().expect("matched 必须有执行 plan");
    // 落账的 crop scale 与凭 plan 现算同值（同一个函数的两处调用）。
    if let Some(warp) = &lens_plan.warp {
        assert_eq!(
            profile.crop_scale_applied,
            auto_crop_scale(warp, decoded.frame.width, decoded.frame.height)
        );
    }
    // 凭落账 profile 重建 plan 必须与导入期 plan 逐项相等（取图路径
    // 与导入路径同源的判据）。
    let rebuilt = plan_from_applied(profile).expect("profile 应可重建 plan");
    assert_eq!(&rebuilt, lens_plan);
    // 带校正展开：输出尺寸不变（恒等于 active area），且与导入期
    // record 逐字段同源（同参数 + 同 profile + 同解码上下文组装）。
    let developed = develop_with_lens(
        &decoded.frame,
        &plan.options.develop,
        &yanshi_raw::record_context(&decoded),
        Some(LensExecution {
            plan: lens_plan,
            profile,
            lens_match: "matched",
        }),
    )
    .unwrap();
    assert_eq!(
        (developed.width, developed.height),
        (decoded.frame.width as u32, decoded.frame.height as u32)
    );
    assert_eq!(plan.record, developed.record);
}

// ---------------------------------------------------------------------------
// lens_override 真实现（判据：手动命中 + 查无此镜报错）
// ---------------------------------------------------------------------------

#[test]
fn lens_override_manual_and_unknown_errors() {
    let decoded = decode_bytes(&exif_fixture_bytes()).unwrap();
    let db = fixture_db();
    let plan = plan_import_with_db(
        &decoded,
        Some(&json!({"lens_override": "FixtureOptics Fixture 24mm f/2.8"})),
        Some(&db),
    )
    .unwrap();
    assert_eq!(plan.record.lens_match.as_deref(), Some("manual"));
    let profile = plan.record.applied_lens_profile.as_ref().unwrap();
    assert_eq!(profile.matched_via, "manual");
    let error = plan_import_with_db(
        &decoded,
        Some(&json!({"lens_override": "FixtureOptics 不存在的 85mm"})),
        Some(&db),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("lens_override"),
        "报错要指明 lens_override：{error}"
    );
}

// ---------------------------------------------------------------------------
// 文件自带校正逐项优先（判据：防重复——文件在场项不叠加库同项）
// ---------------------------------------------------------------------------

#[test]
fn file_opcode_items_win_over_db_per_item() {
    let mut decoded = DecodedRaw {
        frame: base_frame(8, 8),
        file_meta: RawFileMeta {
            lens_exif: Some(yanshi_raw::LensExif {
                make: Some("FixtureOptics".to_string()),
                model: Some("Fixture 24mm f/2.8".to_string()),
                focal_mm: Some(24.0),
                fnumber: Some(2.8),
                focus_distance_m: Some(3.5),
            }),
            ..RawFileMeta::default()
        },
        notes: Vec::new(),
        crop_factor: None,
        file_lens: Some(FileLensCorrection {
            warp_sets: None,
            warp_center: None,
            vignette_k: Some([0.25, 0.0, 0.0, 0.0, 0.0]),
            vignette_center: Some([0.5, 0.5]),
            skipped_opcodes: Vec::new(),
        }),
    };
    decoded.frame.mosaic = vec![6000.0; 64];
    let db = fixture_db();
    let plan = plan_import_with_db(
        &decoded,
        Some(&json!({"lens_correction": "auto"})),
        Some(&db),
    )
    .unwrap();
    let profile = plan.record.applied_lens_profile.as_ref().unwrap();
    // 暗角归文件、畸变/TCA 归库：逐项来源如实落账。
    assert_eq!(profile.sources["vignetting"], json!("file_opcode"));
    assert_eq!(profile.sources["distortion"], json!("lensfun"));
    assert_eq!(profile.sources["tca"], json!("lensfun"));
    assert_eq!(
        profile.vignetting_pa, None,
        "文件在场时库 pa 不许进 profile 冒充"
    );
    assert!(profile.opcode.is_some(), "文件参数原文必须落账以便取图重建");
    let lens_plan = plan.lens_plan.as_ref().unwrap();
    assert!(
        matches!(lens_plan.vignette, Some(VignetteModel::OpcodeGain { .. })),
        "执行 plan 的暗角必须是文件模型"
    );
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("FixVignetteRadial") && w.contains("未叠加")),
        "防重复的裁决要进 warnings：{:?}",
        plan.warnings
    );
    // profile 重建往返一致。
    let rebuilt = plan_from_applied(profile).unwrap();
    assert_eq!(&rebuilt, lens_plan);
    let _ = WarpRectilinearSet {
        kr: [1.0, 0.0, 0.0, 0.0],
        kt: [0.0, 0.0],
    };
}

// ---------------------------------------------------------------------------
// db_unavailable 只剩库整体为空一种情形
// ---------------------------------------------------------------------------

#[test]
fn empty_db_with_auto_is_db_unavailable() {
    let decoded = decode_bytes(&exif_fixture_bytes()).unwrap();
    let empty = LensDb {
        source: "empty".into(),
        version: "0".into(),
        lenses: Vec::new(),
        skipped_entries: 0,
    };
    let plan = plan_import_with_db(
        &decoded,
        Some(&json!({"lens_correction": "auto"})),
        Some(&empty),
    )
    .unwrap();
    assert_eq!(plan.record.lens_match.as_deref(), Some("db_unavailable"));
    assert!(plan.lens_plan.is_none());
}

// ---------------------------------------------------------------------------
// off / 零参数恒等：与第三包逐位一致（D5 纪律的判据）
// ---------------------------------------------------------------------------

#[test]
fn identity_plan_is_bitwise_identical_to_plain_develop() {
    let mut frame = base_frame(24, 16);
    for (i, v) in frame.mosaic.iter_mut().enumerate() {
        *v = 800.0 + ((i * 37) % 15000) as f32;
    }
    let options = DevelopOptions {
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let plain = develop(&frame, &options, &RecordContext::default()).unwrap();
    let (plan, profile) = build_plan_and_profile(
        Some([0.0, 0.0, 0.0]),
        None,
        Some([0.0, 0.0, 0.0]),
        json!({"distortion": "lensfun", "vignetting": "lensfun", "tca": "none"}),
        None,
        None,
        1.0,
        "test",
        "test",
        "Zero Lens",
        "exact",
        Some(24.0),
    );
    let with = develop_with_lens(
        &frame,
        &options,
        &RecordContext::default(),
        Some(LensExecution {
            plan: &plan,
            profile: &profile,
            lens_match: "matched",
        }),
    )
    .unwrap();
    assert_eq!(plain.rgb_linear, with.rgb_linear, "零参数校正必须逐位恒等");
}

// ---------------------------------------------------------------------------
// warp 的公式 oracle：线性斜坡经 PTLens warp 后，输出值应恰等于按
// 公式手算的源坐标（双线性对线性函数精确）。
// ---------------------------------------------------------------------------

#[test]
fn ptlens_warp_matches_formula_oracle_on_ramp() {
    let (w, h) = (64usize, 64usize);
    // R 通道 = x 斜坡，G 通道 = y 斜坡，B = 0。
    let mut rgb = vec![0.0f32; w * h * 3];
    for y in 0..h {
        for x in 0..w {
            rgb[(y * w + x) * 3] = x as f32;
            rgb[(y * w + x) * 3 + 1] = y as f32;
        }
    }
    let (a, b, c) = (0.03f32, -0.08f32, 0.04f32);
    let model = WarpModel::PtlensTca {
        ptlens: [a, b, c],
        tca: None,
        crop_ratio: 1.0,
    };
    assert_eq!(auto_crop_scale(&model, w, h), 1.0, "本组参数不应触发裁剪");
    let out = warp_rgb(&rgb, w, h, &model);
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let half = libm::sqrtf(cx * cx + cy * cy);
    // 取源坐标不出界的内点（贴边点会撞 reflect-101 边界折返，
    // 斜坡 oracle 在折返区不成立——那是边界口径，不是 warp 公式）。
    for (x, y) in [(50usize, 32usize), (10, 10), (58, 4), (32, 58)] {
        let dx = (x as f32 - cx) / half;
        let dy = (y as f32 - cy) / half;
        let r = libm::sqrtf(dx * dx + dy * dy);
        let poly = (1.0 - a - b - c) + c * r + b * r * r + a * r * r * r;
        let expected_sx = cx + dx * poly * half;
        let expected_sy = cy + dy * poly * half;
        let got_x = out[(y * w + x) * 3];
        let got_y = out[(y * w + x) * 3 + 1];
        assert!(
            (got_x - expected_sx).abs() < 0.02,
            "({x},{y}) 源 x 应 {expected_sx}，实得 {got_x}"
        );
        assert!(
            (got_y - expected_sy).abs() < 0.02,
            "({x},{y}) 源 y 应 {expected_sy}，实得 {got_y}"
        );
    }
}

#[test]
fn opcode_warp_identity_set_is_bitwise_identity() {
    let (w, h) = (16usize, 12usize);
    let rgb: Vec<f32> = (0..w * h * 3).map(|i| (i % 97) as f32 / 7.0).collect();
    let model = WarpModel::OpcodeWarp {
        sets: vec![WarpRectilinearSet {
            kr: [1.0, 0.0, 0.0, 0.0],
            kt: [0.0, 0.0],
        }],
        center: [0.5, 0.5],
    };
    let out = warp_rgb(&rgb, w, h, &model);
    assert_eq!(out, rgb, "kr0=1 其余 0 的 WarpRectilinear 必须是恒等");
}

// ---------------------------------------------------------------------------
// 暗角：角部亮度比判据（校正后角部/中心 ≈ 1，未校正时 ≈ V(角)）
// ---------------------------------------------------------------------------

#[test]
fn vignetting_correction_restores_corner_brightness() {
    let (w, h) = (32usize, 32usize);
    let pa = [-0.30f32, 0.05, 0.0];
    // 模拟带暗角的马赛克：归一化值 = 0.5 × V(r)（V 按像素自身半径）。
    let mut frame = base_frame(w, h);
    let (cx, cy) = ((w as f32 - 1.0) / 2.0, (h as f32 - 1.0) / 2.0);
    let half = libm::sqrtf(cx * cx + cy * cy);
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = libm::sqrtf(dx * dx + dy * dy) / half;
            let v = 1.0 + pa[0] * r * r + pa[1] * r * r * r * r;
            frame.mosaic[y * w + x] = 512.0 + 0.5 * (16383.0 - 512.0) * v;
        }
    }
    let options = DevelopOptions {
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let plain = develop(&frame, &options, &RecordContext::default()).unwrap();
    let (plan, profile) = build_plan_and_profile(
        None,
        None,
        Some(pa),
        json!({"distortion": "none", "vignetting": "lensfun", "tca": "none"}),
        None,
        None,
        1.0,
        "test",
        "test",
        "Vig Lens",
        "exact",
        Some(24.0),
    );
    let fixed = develop_with_lens(
        &frame,
        &options,
        &RecordContext::default(),
        Some(LensExecution {
            plan: &plan,
            profile: &profile,
            lens_match: "matched",
        }),
    )
    .unwrap();
    let lum = |img: &yanshi_raw::DevelopedImage, x: usize, y: usize| {
        let o = (y * w + x) * 3;
        (img.rgb_linear[o] + img.rgb_linear[o + 1] + img.rgb_linear[o + 2]) / 3.0
    };
    let center_plain = lum(&plain, 16, 16);
    let ratio_plain = lum(&plain, 0, 0) / center_plain;
    let ratio_fixed = lum(&fixed, 0, 0) / lum(&fixed, 16, 16);
    assert!(
        ratio_plain < 0.85,
        "未校正时角部应明显偏暗（V(1)=0.75），实得比值 {ratio_plain}"
    );
    assert!(
        ratio_fixed > 0.93,
        "校正后角部/中心应接近 1，实得 {ratio_fixed}"
    );
    // 增益场本身：角点增益 = 1/V(1) = 1/0.75。
    let field = vignette_gain_field(&VignetteModel::Pa(pa), w, h);
    assert!((field[0] - 1.0 / 0.75).abs() < 1e-5);
    assert!((field[16 * w + 16] - 1.0).abs() < 0.02);
    let _ = LensCorrectionPlan::default();
}
