//! §9.3 挂接点集成证明：经 `yanshi-render` 的 `develop_raw_source`
//! （feature `raw`）走完整字节 → 解码 → 展开链，并核对留痕记录经内核
//! 边界回来后字段完整（这条链是下一包导入链路的唯一展开入口）。

use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_raw::DevelopOptions;

#[test]
fn render_hook_develops_synthetic_dng() {
    // 中性场景夹具（与 decode_smoke 同法：逐通道 DN 与 AsShotNeutral
    // [0.58, 1, 0.72] 以 k=50 整数成比例）⇒ 展开为中性常数 50/15871。
    let dn = [541u16, 562, 548];
    let spec = SyntheticDng {
        mosaic: (0..64)
            .map(|i| {
                let (x, y) = (i % 8, i / 8);
                dn[yanshi_raw::CfaPattern::Rggb.channel_at(x, y).index()]
            })
            .collect(),
        ..Default::default()
    };
    let bytes = write_synthetic_dng(&spec);
    let out = yanshi_render::develop_raw_source(&bytes, &DevelopOptions::default())
        .expect("挂接点应能展开合成 DNG");
    assert_eq!((out.width, out.height), (8, 8));
    let v = 50.0f32 / (16383.0 - 512.0);
    for (i, &value) in out.rgb_linear.iter().enumerate() {
        assert!((value - v).abs() < 1e-5, "分量 {i} 应为 {v}，实得 {value}");
    }
    // 留痕记录经内核入口回来：上下文（机型／解码器）由 decode 侧组装。
    assert_eq!(out.record.camera_make.as_deref(), Some("YanshiFixture"));
    assert_eq!(out.record.camera_model.as_deref(), Some("Synthetic One"));
    assert_eq!(out.record.decoder_name, "rawler");
    assert_eq!(out.record.cfa, "RGGB");
    assert!(out.record.color_alpha.is_some(), "双标定应有插值权重");
    // 解码注记与展开注记合并在同一出口（BaselineExposure 事实在内）。
    assert!(
        out.notes.iter().any(|n| n.contains("BaselineExposure")),
        "注记应含 BaselineExposure 不可得，实得 {:?}",
        out.notes
    );
}

#[test]
fn render_hook_maps_develop_option_errors() {
    let bytes = write_synthetic_dng(&SyntheticDng::default());
    let options = DevelopOptions {
        pre_exposure_ev: 9.0, // 契约越界
        ..Default::default()
    };
    let error = yanshi_render::develop_raw_source(&bytes, &options)
        .expect_err("越界的 pre_exposure_ev 必须经挂接点响亮报错");
    assert!(
        error.to_string().contains("pre_exposure_ev"),
        "错误信息应点名参数，实得 {error}"
    );
}
