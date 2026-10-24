//! C1-b 判据：预览档展开（`DevelopOptions.max_output_edge`）。
//! 断结构与恒等式：尺寸被压到长边 = cap、record 如实记档、全档
//! 逐位不变（None 时整步跳过）、越界响亮报错、旧记录（无
//! `developed_max_edge` 字段）解析为全档。

use yanshi_raw::frame::{CalibrationMatrix, CfaGrid, CfaPattern, Channel, RecordContext};
use yanshi_raw::import::parse_raw_options;
use yanshi_raw::{develop, DevelopOptions, RawFrame};

fn gradient_frame(w: usize, h: usize) -> RawFrame {
    let mut mosaic = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            mosaic[y * w + x] = 4000.0 + ((x * 7 + y * 13) % 8000) as f32;
        }
    }
    RawFrame {
        width: w,
        height: h,
        mosaic,
        cfa: CfaGrid::from_bayer(CfaPattern::Rggb),
        black_levels: vec![512.0; 4],
        white_levels: vec![16383.0; 4],
        wb_multipliers: Some([1.8, 1.0, 1.5]),
        as_shot_neutral: None,
        color_matrices: vec![CalibrationMatrix {
            illuminant_kelvin: Some(6504.0),
            xyz_to_cam: [
                [0.7355, -0.1828, -0.0836],
                [-0.4262, 1.2845, 0.2576],
                [-0.0698, 0.1123, 0.7846],
            ],
        }],
        baseline_exposure_ev: Some(0.0),
        orientation: 1,
    }
}

#[test]
fn preview_edge_caps_output_and_is_recorded() {
    let frame = gradient_frame(1600, 1200);
    let ctx = RecordContext::default();

    // 预览档：长边恰为 cap、短边按比例（1200×800÷1600 = 600）。
    let options = DevelopOptions {
        max_output_edge: Some(800),
        ..DevelopOptions::default()
    };
    let image = develop(&frame, &options, &ctx).expect("预览档展开");
    assert_eq!((image.width, image.height), (800, 600));
    assert_eq!(
        image.record.developed_max_edge,
        Some(800),
        "record 必须记档"
    );
    assert!(
        image.notes.iter().any(|n| n.contains("预览档")),
        "展开注记必须点名预览档：{:?}",
        image.notes
    );

    // 全档：尺寸原样、record 不记档。
    let full = develop(&frame, &DevelopOptions::default(), &ctx).expect("全档展开");
    assert_eq!((full.width, full.height), (1600, 1200));
    assert_eq!(full.record.developed_max_edge, None);

    // cap ≥ 长边：尺寸不变，但档位照样记（记的是「按预览档展开」）。
    let options = DevelopOptions {
        max_output_edge: Some(8192),
        ..DevelopOptions::default()
    };
    let image = develop(&frame, &options, &ctx).expect("大 cap 预览档");
    assert_eq!((image.width, image.height), (1600, 1200));
    assert_eq!(image.record.developed_max_edge, Some(8192));
}

#[test]
fn preview_downscale_preserves_constant_field() {
    // 恒色场经面积加权降采样仍是恒色场（内区逐像素近恒等）：
    // 滤波若权重归一错了，这里立刻露馅。
    let (w, h) = (240usize, 160usize);
    let grid = CfaGrid::from_bayer(CfaPattern::Rggb);
    let mut frame = gradient_frame(w, h);
    for y in 0..h {
        for x in 0..w {
            frame.mosaic[y * w + x] = match grid.channel_at(x, y) {
                Channel::R => 9000.0,
                Channel::G => 6000.0,
                Channel::B => 4000.0,
            };
        }
    }
    let ctx = RecordContext::default();
    let full = develop(&frame, &DevelopOptions::default(), &ctx).expect("全档");
    let options = DevelopOptions {
        max_output_edge: Some(64),
        ..DevelopOptions::default()
    };
    let preview = develop(&frame, &options, &ctx).expect("预览档");
    assert_eq!((preview.width, preview.height), (64, 42));
    // 内区同位像素：预览（降采样自同一恒色场）与全档之差应 ~0。
    let (fx, fy) = (120usize, 80usize);
    let (px, py) = (32usize, 21usize);
    for c in 0..3 {
        let f = full.rgb_linear[(fy * 240 + fx) * 3 + c];
        let p = preview.rgb_linear[(py * 64 + px) * 3 + c];
        assert!(
            (f - p).abs() < 1e-5,
            "通道 {c} 恒色场被降采样改色：全档 {f} vs 预览 {p}"
        );
    }
}

#[test]
fn preview_edge_out_of_range_is_a_loud_error() {
    let frame = gradient_frame(24, 16);
    for bad in [63u32, 8193] {
        let options = DevelopOptions {
            max_output_edge: Some(bad),
            ..DevelopOptions::default()
        };
        let error = develop(&frame, &options, &RecordContext::default())
            .expect_err("越界的 max_output_edge 必须响亮报错、不许 clamp");
        assert!(
            error.to_string().contains("max_output_edge"),
            "错误必须点名参数：{error}"
        );
    }
}

#[test]
fn preview_record_serde_defaults_to_full_tier_for_legacy_records() {
    let frame = gradient_frame(24, 16);
    let image = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect("全档展开");
    // 本包写出的 record 带字段（null）⇒ 解析回 None。
    let value = serde_json::to_value(&image.record).expect("record 可序列化");
    assert_eq!(value["developed_max_edge"], serde_json::Value::Null);
    let parsed: yanshi_raw::RawDevelopRecord =
        serde_json::from_value(value.clone()).expect("record 可解析");
    assert_eq!(parsed.developed_max_edge, None);
    // 旧记录（字段整把缺失）⇒ 同样解析为 None = 全档。
    let mut legacy = value;
    legacy
        .as_object_mut()
        .expect("record 是对象")
        .remove("developed_max_edge");
    let parsed: yanshi_raw::RawDevelopRecord =
        serde_json::from_value(legacy).expect("旧记录必须可解析");
    assert_eq!(parsed.developed_max_edge, None);
}

#[test]
fn raw_options_preview_max_edge_parsing() {
    let parsed = parse_raw_options(Some(&serde_json::json!({"preview_max_edge": 800})))
        .expect("合法 preview_max_edge 应可解析");
    assert_eq!(parsed.develop.max_output_edge, Some(800));
    // 缺省（不给键）= 全档。
    let parsed = parse_raw_options(None).expect("空 options");
    assert_eq!(parsed.develop.max_output_edge, None);
    // 越界／类型错：逐项响亮报错。
    for bad in [
        serde_json::json!({"preview_max_edge": 63}),
        serde_json::json!({"preview_max_edge": 8193}),
        serde_json::json!({"preview_max_edge": 800.5}),
        serde_json::json!({"preview_max_edge": "800"}),
    ] {
        let error = parse_raw_options(Some(&bad)).expect_err("非法 preview_max_edge 必须报错");
        assert!(
            error.to_string().contains("preview_max_edge"),
            "错误必须点名参数：{error}（输入 {bad}）"
        );
    }
}
