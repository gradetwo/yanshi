//! 解码冒烟（设计 6.1）：合成 DNG（手写 TIFF）经 rawler 真解码 → 帧映射 →
//! 全链展开。断的是**映射口径**（相位锚定、电平换算、标定排序），
//! 不是像素快照——夹具图案用常量场，解析期望随手可算。

use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_raw::frame::RecordContext;
use yanshi_raw::{decode_bytes, develop, record_context, CfaPattern, DevelopOptions};

/// 中性场景的夹具：马赛克逐通道 DN 与 AsShotNeutral [0.58, 1, 0.72] 成比例
/// （k=50 ⇒ R/G/B 位 DN = 512 + [29, 50, 36]，整数精确成比例），
/// 展开后应为中性常数 v = 50/15871。把马赛克写成跨通道常数不是中性场景
/// （链会正确地渲染成一个非中性色——第一版测试在此被实测纠正过）。
fn neutral_fixture() -> SyntheticDng {
    let dn = [541u16, 562, 548]; // R, G, B
    SyntheticDng {
        mosaic: (0..64)
            .map(|i| {
                let (x, y) = (i % 8, i / 8);
                let channel = yanshi_raw::CfaPattern::Rggb.channel_at(x, y);
                dn[channel.index()]
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn synthetic_dng_decodes_and_develops_end_to_end() {
    let bytes = write_synthetic_dng(&neutral_fixture());
    let decoded = decode_bytes(&bytes).expect("合成 DNG 必须能被 rawler 解码");
    let frame = &decoded.frame;
    assert_eq!((frame.width, frame.height), (8, 8));
    assert_eq!(frame.cfa, CfaPattern::Rggb);
    assert_eq!(frame.black_levels, [512.0; 4]);
    assert_eq!(frame.white_levels, [16383.0; 4]);
    // AsShotNeutral [0.58, 1, 0.72] ⇒ 乘子 [1/0.58, 1, 1/0.72]。
    let wb = frame.wb_multipliers.expect("应有 WB 乘子");
    assert!((wb[0] - 1.0 / 0.58).abs() < 1e-4, "wb={wb:?}");
    assert!((wb[1] - 1.0).abs() < 1e-6, "wb={wb:?}");
    assert!((wb[2] - 1.0 / 0.72).abs() < 1e-4, "wb={wb:?}");
    // 双标定按色温升序：A(2856) 在前、D65(6504) 在后。
    assert_eq!(frame.color_matrices.len(), 2);
    assert_eq!(frame.color_matrices[0].illuminant_kelvin, Some(2856.0));
    assert_eq!(frame.color_matrices[1].illuminant_kelvin, Some(6504.0));
    // 文件元数据：机型来自夹具，TIFF 家族的源格式口径为 "tiff"
    //（细分 dng/厂商格式靠导入期的文件扩展名，本包如实记家族，见 README）。
    assert_eq!(decoded.file_meta.make.as_deref(), Some("YanshiFixture"));
    assert_eq!(decoded.file_meta.model.as_deref(), Some("Synthetic One"));
    assert_eq!(decoded.file_meta.source_format, "tiff");
    // 全链：中性场（见 neutral_fixture）⇒ 中性常数 v = 50/(16383−512)。
    // 留痕上下文走 decode 侧的真实组装（record_context），不拿空占位充数。
    let out = develop(frame, &DevelopOptions::default(), &record_context(&decoded)).unwrap();
    let v = 50.0f32 / (16383.0 - 512.0);
    assert!((out.width, out.height) == (8, 8));
    for (i, &value) in out.rgb_linear.iter().enumerate() {
        assert!((value - v).abs() < 1e-5, "分量 {i} 应为 {v}，实得 {value}");
    }
    assert_eq!(out.record.decoder_name, "rawler");
    assert!(!out.record.decoder_version.is_empty());
    assert_eq!(
        out.record.wb_multipliers[1], 1.0,
        "重归一化后 G 必须是 min=1 的那个"
    );
}

#[test]
fn active_area_crop_remaps_cfa_and_black_phases() {
    // 全幅 10×10、ActiveArea (top,left,bottom,right) = (1,1,9,9) ⇒ 裁后 8×8。
    // DNG 的 CFAPattern 相对 ActiveArea 写（夹具写 RGGB）：rawler 把它反平移回
    // 全幅口径；本包再按裁剪原点 (1,1) 推导裁后排列——两程换算后必须回到 RGGB，
    // 黑电平四相位必须按 (1,1) 奇偶整体换位（[a,b,c,d] → [d,c,b,a]）。
    let spec = SyntheticDng {
        width: 10,
        height: 10,
        mosaic: vec![1024; 100],
        active_area: Some([1, 1, 9, 9]),
        black_levels: [(100, 1), (200, 1), (300, 1), (400, 1)],
        ..Default::default()
    };
    let bytes = write_synthetic_dng(&spec);
    let decoded = decode_bytes(&bytes).expect("带 ActiveArea 的 DNG 必须可解");
    let frame = &decoded.frame;
    assert_eq!((frame.width, frame.height), (8, 8));
    assert_eq!(
        frame.cfa,
        CfaPattern::Rggb,
        "裁后 CFA 必须回到 ActiveArea 口径的 RGGB"
    );
    assert_eq!(
        frame.black_levels,
        [400.0, 300.0, 200.0, 100.0],
        "裁剪原点 (1,1) 时四相位应整体换位"
    );
    // 裁后帧可全链展开（常量场仍是中性常数，只是黑电平逐相位不同——
    // 常量马赛克下每通道采样值不同，展开产物只需有限非负，不在此断数值）。
    let out = develop(frame, &DevelopOptions::default(), &RecordContext::default()).unwrap();
    assert!(out.rgb_linear.iter().all(|&v| v.is_finite() && v >= 0.0));
}

#[test]
fn baseline_exposure_written_but_not_surfaced_by_rawler_0_7_2() {
    // ★ 缺口钉子：夹具把 BaselineExposure = +0.5EV 写进文件，rawler 0.7.2
    // 仍然读不到（RawImage 不带该字段、dng_tags 从未被填充）。
    // 解码器哪天暴露了它，这条会红——那是好事：回来把 decode 接上并更新 README。
    let spec = SyntheticDng {
        baseline_exposure: Some((1, 2)),
        ..Default::default()
    };
    let bytes = write_synthetic_dng(&spec);
    let decoded = decode_bytes(&bytes).expect("带 BaselineExposure 的 DNG 必须可解");
    assert_eq!(
        decoded.frame.baseline_exposure_ev, None,
        "rawler 0.7.2 不应能给出 BaselineExposure；若此断言变红，说明新版已暴露——请接上 decode 并更新缺口清单"
    );
    // 展开侧按缺失 = 0 处理并留痕（注记里必须能找到这句事实）。
    let out = develop(
        &decoded.frame,
        &DevelopOptions::default(),
        &RecordContext::default(),
    )
    .unwrap();
    assert_eq!(out.record.baseline_exposure_ev, 0.0);
    assert!(
        out.notes.iter().any(|n| n.contains("BaselineExposure")),
        "注记必须交代 BaselineExposure 不可得，实得 {:?}",
        out.notes
    );
}

#[test]
fn garbage_bytes_fail_loudly_not_panic() {
    assert!(decode_bytes(b"definitely not a raw file").is_err());
    assert!(decode_bytes(&[]).is_err());
    // TIFF 头对但 IFD 是空的：也必须是 Err，不是 panic。
    let mut fake_tiff = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
    fake_tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    assert!(decode_bytes(&fake_tiff).is_err());
}
