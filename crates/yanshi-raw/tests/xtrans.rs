//! ISSUE-3 判据：富士 X-Trans（6×6 CFA）解码与展开。
//! 断言口径守仓规：断结构与恒等式（周期／相位／计数／采样位精确／
//! 恒色场恒等），不断跨机器的像素统计。合成 DNG 经 decode 全路，
//! 帧级判据经 develop 公开入口；真 RAF 文件按 NF3 只在本地验、不进仓。

use yanshi_raw::develop::{demosaic, normalize_mosaic};
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_raw::frame::{CalibrationMatrix, RecordContext};
use yanshi_raw::{
    decode_bytes, develop, CfaGrid, CfaPattern, Channel, Demosaic, DevelopOptions, RawFrame,
};

/// 富士 X-Trans 标准 6×6 排列（rawler 给出的 pattern 名原文）。
const XTRANS_PATTERN: &str = "GGRGGBGGBGGRBRGRBGGGBGGRGGRGGBRBGBRG";

fn xtrans_grid() -> CfaGrid {
    let cells: Vec<Channel> = XTRANS_PATTERN
        .chars()
        .map(|c| match c {
            'R' => Channel::R,
            'G' => Channel::G,
            'B' => Channel::B,
            other => panic!("pattern 含未知字符 {other}"),
        })
        .collect();
    CfaGrid::new(6, cells, XTRANS_PATTERN.to_string()).expect("6×6 网格必须可构造")
}

fn d65_calibration() -> CalibrationMatrix {
    CalibrationMatrix {
        illuminant_kelvin: Some(6500.0),
        xyz_to_cam: [
            [0.7355, -0.1828, -0.0836],
            [-0.4262, 1.2845, 0.2576],
            [-0.0698, 0.1123, 0.7846],
        ],
    }
}

fn xtrans_frame(width: usize, height: usize, mosaic: Vec<f32>) -> RawFrame {
    RawFrame {
        width,
        height,
        mosaic,
        cfa: xtrans_grid(),
        black_levels: vec![512.0; 36],
        white_levels: vec![16383.0; 36],
        wb_multipliers: Some([1.8, 1.0, 1.5]),
        as_shot_neutral: None,
        color_matrices: vec![d65_calibration()],
        baseline_exposure_ev: None,
        orientation: 1,
    }
}

#[test]
fn xtrans_grid_matches_fuji_pattern_structure() {
    let grid = xtrans_grid();
    assert_eq!(grid.period(), 6);
    assert_eq!(grid.bayer(), None, "X-Trans 不许被误认成 Bayer");
    assert_eq!(grid.name(), XTRANS_PATTERN);
    // 通道计数是 X-Trans 的结构恒等式：G 20、R 8、B 8。
    let mut counts = [0usize; 3];
    for y in 0..6 {
        for x in 0..6 {
            counts[grid.channel_at(x, y).index()] += 1;
        }
    }
    assert_eq!(counts, [8, 20, 8], "R／G／B 格数应为 8／20／8");
    // 周期性：平移整周期后通道不变。
    for y in 0..12 {
        for x in 0..12 {
            assert_eq!(grid.channel_at(x + 6, y), grid.channel_at(x, y));
            assert_eq!(grid.channel_at(x, y + 6), grid.channel_at(x, y));
        }
    }
    // 形状非法必须响亮失败，不许静默构造。
    assert!(CfaGrid::new(6, vec![Channel::G; 35], "bad".to_string()).is_err());
    assert!(CfaGrid::new(1, vec![Channel::G; 1], "bad".to_string()).is_err());
}

#[test]
fn bayer_grid_roundtrips_through_pattern() {
    // 恒等式：CfaGrid::from_bayer 与 CfaPattern 在每个坐标上同通道同相位。
    for pattern in [
        CfaPattern::Rggb,
        CfaPattern::Bggr,
        CfaPattern::Grbg,
        CfaPattern::Gbrg,
    ] {
        let grid = CfaGrid::from_bayer(pattern);
        assert_eq!(grid.bayer(), Some(pattern));
        assert_eq!(grid.name(), pattern.name());
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(grid.channel_at(x, y), pattern.channel_at(x, y));
                assert_eq!(grid.phase_at(x, y), CfaPattern::phase_at(x, y));
            }
        }
    }
}

#[test]
fn xtrans_constant_color_field_is_exact() {
    let grid = xtrans_grid();
    let (w, h) = (24usize, 18usize);
    let mut plane = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            plane[y * w + x] = match grid.channel_at(x, y) {
                Channel::R => 0.8,
                Channel::G => 0.5,
                Channel::B => 0.3,
            };
        }
    }
    for kind in [Demosaic::MalvarHeCutler, Demosaic::Bilinear] {
        let out = demosaic(&plane, w, h, grid.clone(), kind);
        for pixel in out.chunks_exact(3) {
            assert!(
                (pixel[0] - 0.8).abs() < 1e-6
                    && (pixel[1] - 0.5).abs() < 1e-6
                    && (pixel[2] - 0.3).abs() < 1e-6,
                "恒色场应逐像素 [0.8, 0.5, 0.3]，实得 {pixel:?}"
            );
        }
    }
}

#[test]
fn xtrans_gradient_stays_close_and_samples_are_exact() {
    let grid = xtrans_grid();
    let (w, h) = (60usize, 60usize);
    let truth = |x: usize, y: usize| -> [f32; 3] {
        [
            0.15 + 0.5 * (x as f32 / (w - 1) as f32),
            0.15 + 0.5 * (y as f32 / (h - 1) as f32),
            0.4,
        ]
    };
    let mut plane = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let t = truth(x, y);
            plane[y * w + x] = t[grid.channel_at(x, y).index()];
        }
    }
    let out = demosaic(&plane, w, h, grid.clone(), Demosaic::MalvarHeCutler);
    for y in 0..h {
        for x in 0..w {
            let o = (y * w + x) * 3;
            // 采样位本通道必须逐位取原样本（高光天花板语义的根）。
            let here = grid.channel_at(x, y);
            assert_eq!(
                out[o + here.index()],
                plane[y * w + x],
                "({x},{y}) 采样位本通道被插值稀释"
            );
            // 内区（避开边界反射带）与平滑真值的偏差须小。
            if (7..w - 7).contains(&x) && (7..h - 7).contains(&y) {
                let t = truth(x, y);
                for c in 0..3 {
                    assert!(
                        (out[o + c] - t[c]).abs() < 0.02,
                        "({x},{y}) 通道 {c} 偏差过大：{} vs {}",
                        out[o + c],
                        t[c]
                    );
                }
            }
        }
    }
}

#[test]
fn xtrans_normalize_uses_each_phases_own_black_level() {
    let (w, h) = (12usize, 12usize);
    let mut frame = xtrans_frame(w, h, vec![1000.0; w * h]);
    frame.black_levels = (0..36).map(|p| 100.0 + p as f32).collect();
    frame.white_levels = vec![2000.0; 36];
    let plane = normalize_mosaic(&frame).expect("36 相位电平必须可归一化");
    for y in 0..h {
        for x in 0..w {
            let phase = frame.cfa.phase_at(x, y);
            let black = 100.0 + phase as f32;
            let expect = (1000.0 - black) / (2000.0 - black);
            assert!(
                (plane[y * w + x] - expect).abs() < 1e-6,
                "({x},{y}) 相位 {phase} 用错了黑电平"
            );
        }
    }
}

#[test]
fn xtrans_full_develop_chain_runs_and_records_grid() {
    let (w, h) = (48usize, 36usize);
    let grid = xtrans_grid();
    let mut mosaic = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            mosaic[y * w + x] = match grid.channel_at(x, y) {
                Channel::R => 9000.0,
                Channel::G => 6000.0,
                Channel::B => 4000.0,
            };
        }
    }
    let frame = xtrans_frame(w, h, mosaic);
    let image = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect("X-Trans 帧必须能走完整展开链");
    assert_eq!((image.width, image.height), (w as u32, h as u32));
    assert!(
        image.rgb_linear.iter().all(|v| v.is_finite()),
        "展开产物含非有限值"
    );
    let record = &image.record;
    assert_eq!(record.cfa, XTRANS_PATTERN);
    assert_eq!(record.black_levels.len(), 36);
    assert_eq!(record.white_levels.len(), 36);
}

#[test]
fn synthetic_xtrans_dng_decodes_to_grid_frame() {
    let pattern_idx: Vec<u8> = XTRANS_PATTERN
        .chars()
        .map(|c| match c {
            'R' => 0,
            'G' => 1,
            'B' => 2,
            other => panic!("pattern 含未知字符 {other}"),
        })
        .collect();
    let (w, h) = (36u32, 36u32);
    let mut spec = SyntheticDng {
        width: w,
        height: h,
        mosaic: vec![0; (w * h) as usize],
        cfa_pattern_full: Some((pattern_idx, 6)),
        black_levels_full: Some((0..36).map(|i| (100 + i, 1)).collect()),
        ..SyntheticDng::default()
    };
    for y in 0..h as usize {
        for x in 0..w as usize {
            spec.mosaic[y * w as usize + x] = match XTRANS_PATTERN
                .chars()
                .nth((y % 6) * 6 + (x % 6))
                .expect("pattern 36 字符")
            {
                'R' => 12000,
                'G' => 8000,
                _ => 5000,
            };
        }
    }
    let bytes = write_synthetic_dng(&spec);
    let decoded = decode_bytes(&bytes).expect("合成 X-Trans DNG 必须可解码");
    let frame = &decoded.frame;
    assert_eq!(frame.cfa.period(), 6);
    assert_eq!(frame.cfa.bayer(), None);
    assert_eq!(frame.cfa.name(), XTRANS_PATTERN);
    assert_eq!(
        frame.black_levels,
        (0..36).map(|i| 100.0 + i as f32).collect::<Vec<_>>(),
        "无裁剪时 36 相位黑电平应原样落帧"
    );
    assert_eq!(frame.white_levels, vec![16383.0; 36]);
    // 解出的帧要能全链展开（结构断言：尺寸／有限性，不像素数）。
    let image = develop(frame, &DevelopOptions::default(), &RecordContext::default())
        .expect("解出的 X-Trans 帧必须可展开");
    assert_eq!((image.width, image.height), (w, h));
    assert!(image.rgb_linear.iter().all(|v| v.is_finite()));
}

#[test]
fn xtrans_preview_tier_uses_quick_demosaic_and_names_it() {
    // C3-d（期 2）：预览档 + 网格 CFA ⇒ 快速解马赛克，且注记
    // 点名；全档展开不许出现快速路注记。
    let (w, h) = (48usize, 36usize);
    let mut mosaic = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            mosaic[y * w + x] = 2000.0 + ((x * 31 + y * 57) % 9000) as f32;
        }
    }
    let frame = xtrans_frame(w, h, mosaic);
    let options = DevelopOptions {
        max_output_edge: Some(64),
        ..DevelopOptions::default()
    };
    let image = develop(&frame, &options, &RecordContext::default()).expect("预览档展开");
    assert_eq!(image.record.developed_max_edge, Some(64));
    assert!(
        image.notes.iter().any(|n| n.contains("快速解马赛克")),
        "预览档注记必须点名快速解马赛克：{:?}",
        image.notes
    );
    let full = develop(
        &frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .expect("全档展开");
    assert!(
        !full.notes.iter().any(|n| n.contains("快速解马赛克")),
        "全档不许走快速路：{:?}",
        full.notes
    );
}
