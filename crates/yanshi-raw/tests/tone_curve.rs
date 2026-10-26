//! Y-03 基调曲线判据（集成层，设计 §6.1）：
//!
//! - R1：`tone=off` 时 `develop()` 输出与现行链逐位一致——本文件断
//!   「off ≡ 强度 0 的 standard」（两条路都整步跳过）与「off 的 record
//!   落账」；「现有 golden 零改动」的另一半由 `buffer_reuse_golden.rs`
//!   等金测试全套通过守（本包不改任何金值）。
//! - R7：record 往返——旧记录（无 tone 字段）重展开 ≡ 现行像素；
//!   新记录（standard/strength）重展开与首次展开逐位一致；
//!   `record_json` 含 tone 两字段。
//! - R9：同一帧两次展开逐位一致（确定性 NF1）。
//! - 注记：tone 生效进 notes；`tone=off` 时给了 strength 须点名忽略。
//!
//! 形状按仓库纪律：夹具是合成 DNG（`dng_fixture`，不落二进制资产）；
//! 渐变马赛克让 tone 在暗/中/亮三段都有活干——若夹具全是常数场，
//! 「off ≠ standard」的反安慰剂断言可能恒假。

use serde_json::{json, Value};
use yanshi_raw::decode::record_context;
use yanshi_raw::develop::tone_curve_value;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_raw::{decode_bytes, develop, plan_import, DevelopOptions, HighlightMode, ToneMode};

/// 渐变马赛克夹具（12×8）：DN 从 ~0 线性爬到满量程——tone 在暗部、
/// 中间调、高光三段都有输入可改；常数场夹具测不出「曲线真的跑了」。
fn gradient_bytes() -> Vec<u8> {
    let (w, h) = (12u32, 8u32);
    let mut mosaic = Vec::with_capacity((w * h) as usize);
    for i in 0..w * h {
        mosaic.push((u32::from(i) * 16383 / u32::from(w * h - 1)) as u16);
    }
    write_synthetic_dng(&SyntheticDng {
        width: w,
        height: h,
        mosaic,
        ..SyntheticDng::default()
    })
}

/// 与 `render.rs` 取图侧同口径：凭 record 重建 `DevelopOptions`。
/// 旧记录缺 tone 字段时按 `serde(default)` 口径回落（Off / 0.0）。
fn options_from_record(record: &Value) -> DevelopOptions {
    DevelopOptions {
        wb: serde_json::from_value(record["wb"].clone()).unwrap(),
        demosaic: serde_json::from_value(record["demosaic"].clone()).unwrap(),
        pre_exposure_ev: record["pre_exposure_ev"].as_f64().unwrap() as f32,
        highlight: serde_json::from_value(record["highlight_recovery"].clone()).unwrap(),
        tone: record
            .get("tone")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(ToneMode::Off),
        tone_strength: record
            .get("tone_strength")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32,
        max_output_edge: record
            .get("developed_max_edge")
            .and_then(|v| v.as_u64())
            .map(|e| e as u32),
    }
}

#[test]
fn tone_off_is_bitwise_identity() {
    // R1：tone=off ≡ 强度 0 的 standard（两条路都整步跳过，逐位一致）。
    let decoded = decode_bytes(&gradient_bytes()).expect("合成 DNG 应可解码");
    let ctx = record_context(&decoded);
    let off = DevelopOptions {
        tone: ToneMode::Off,
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let zero = DevelopOptions {
        tone: ToneMode::Standard,
        tone_strength: 0.0,
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let a = develop(&decoded.frame, &off, &ctx).unwrap();
    let b = develop(&decoded.frame, &zero, &ctx).unwrap();
    assert_eq!(
        a.rgb_linear, b.rgb_linear,
        "tone=off 与强度 0 的 standard 必须逐位一致"
    );
    assert_eq!(a.record.tone, ToneMode::Off);
    assert_eq!(b.record.tone, ToneMode::Standard);
    // 反安慰剂：同一帧开 standard（强度 1.0）必须真的改像素——
    // 若这条恒假，上面那条「一致」就是「两条路都没跑」的空断言。
    let on = DevelopOptions {
        tone: ToneMode::Standard,
        tone_strength: 1.0,
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let c = develop(&decoded.frame, &on, &ctx).unwrap();
    assert_ne!(
        a.rgb_linear, c.rgb_linear,
        "standard 强度 1.0 必须改像素（否则 tone 步没跑）"
    );
    assert!(
        c.notes
            .iter()
            .any(|n| n.contains("基调曲线 standard 已应用")),
        "tone 生效须进展开注记，实得 {:?}",
        c.notes
    );
    // 强度 0.5 的输出应在 off 与全强度之间（逐像素夹在两者之间——
    // lerp 的单调性，不靠具体数值）。
    let half = DevelopOptions {
        tone: ToneMode::Standard,
        tone_strength: 0.5,
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let d = develop(&decoded.frame, &half, &ctx).unwrap();
    for i in 0..a.rgb_linear.len() {
        let (lo, hi) = if a.rgb_linear[i] <= c.rgb_linear[i] {
            (a.rgb_linear[i], c.rgb_linear[i])
        } else {
            (c.rgb_linear[i], a.rgb_linear[i])
        };
        assert!(
            d.rgb_linear[i] >= lo - 1e-6 && d.rgb_linear[i] <= hi + 1e-6,
            "强度 0.5 的像素 #{i} 应夹在 off 与全强度之间"
        );
    }
    // tone=off 时给了非缺省 strength，注记须点名忽略（设计 §3.1）。
    let ignored = DevelopOptions {
        tone: ToneMode::Off,
        tone_strength: 0.5,
        highlight: HighlightMode::Floor,
        ..DevelopOptions::default()
    };
    let e = develop(&decoded.frame, &ignored, &ctx).unwrap();
    assert_eq!(
        e.rgb_linear, a.rgb_linear,
        "tone=off 时 strength 必须被忽略（像素不变）"
    );
    assert!(
        e.notes
            .iter()
            .any(|n| n.contains("tone=off") && n.contains("已忽略")),
        "tone=off 时给了 strength 须在注记里点名，实得 {:?}",
        e.notes
    );
}

#[test]
fn tone_record_roundtrip() {
    // R7：旧记录重展开 ≡ 现行像素；新记录重展开与首次展开逐位一致；
    // `record_json` 含 tone 两字段。
    let decoded = decode_bytes(&gradient_bytes()).expect("合成 DNG 应可解码");
    let ctx = record_context(&decoded);
    // 新导入（缺省 standard）：plan_import 落账 + 首次展开。
    let plan = plan_import(&decoded, None).expect("缺省 raw_options 应可解析");
    assert_eq!(plan.options.develop.tone, ToneMode::Standard);
    assert_eq!(plan.options.develop.tone_strength, 1.0);
    let first = develop(&decoded.frame, &plan.options.develop, &ctx).unwrap();
    assert_eq!(first.record.tone, ToneMode::Standard);
    let record_json = serde_json::to_value(&first.record).unwrap();
    assert_eq!(record_json["tone"], json!("standard"));
    assert_eq!(record_json["tone_strength"], json!(1.0));
    // 新记录重展开：凭 record 重建 options ⇒ 与首次展开逐位一致。
    let replay_options = options_from_record(&record_json);
    let replay = develop(&decoded.frame, &replay_options, &ctx).unwrap();
    assert_eq!(
        replay.rgb_linear, first.rgb_linear,
        "新记录重展开须与首次展开逐位一致"
    );
    // 旧记录（删掉 tone 两字段）⇒ 反序列化为 Off / 0.0，重展开 ≡ off。
    let mut old_json = record_json.clone();
    old_json.as_object_mut().unwrap().remove("tone");
    old_json.as_object_mut().unwrap().remove("tone_strength");
    let old_options = options_from_record(&old_json);
    assert_eq!(old_options.tone, ToneMode::Off);
    assert_eq!(old_options.tone_strength, 0.0);
    let old = develop(&decoded.frame, &old_options, &ctx).unwrap();
    // 对照组须与记录的其它档位一致：记录来自 plan_import 缺省
    // （highlight=chroma），只把 tone 换成 Off——否则差的是高光档
    // 位，不是 tone（本判据第一次写错就栽在这里）。
    let off = DevelopOptions {
        tone: ToneMode::Off,
        highlight: HighlightMode::Chroma,
        ..DevelopOptions::default()
    };
    let current = develop(&decoded.frame, &off, &ctx).unwrap();
    assert_eq!(
        old.rgb_linear, current.rgb_linear,
        "旧记录重展开须 ≡ 现行 tone=off 像素"
    );
    // 旧记录重展开 ≠ 新记录像素（tone 真的在 record 里起了作用）。
    assert_ne!(
        old.rgb_linear, first.rgb_linear,
        "旧记录与新记录像素应不同（否则 tone 没落账）"
    );
}

#[test]
fn tone_determinism_nf1() {
    // R9：同一帧两次展开逐位一致；曲线求值本身是纯 f32 算术（NF1）。
    let decoded = decode_bytes(&gradient_bytes()).expect("合成 DNG 应可解码");
    let ctx = record_context(&decoded);
    let options = DevelopOptions {
        tone: ToneMode::Standard,
        tone_strength: 0.7,
        highlight: HighlightMode::Chroma,
        ..DevelopOptions::default()
    };
    let a = develop(&decoded.frame, &options, &ctx).unwrap();
    let b = develop(&decoded.frame, &options, &ctx).unwrap();
    assert_eq!(a.rgb_linear, b.rgb_linear, "两次展开须逐位一致");
    assert_eq!(a.record, b.record, "两次展开的 record 须一致");
    // 曲线求值无平台相关 method：同一输入两次求值逐位一致。
    for x in [0.0f32, 0.02, 0.18, 0.5, 0.9, 1.0, 1.5, -1.0] {
        assert_eq!(
            tone_curve_value(x, 0.7).to_bits(),
            tone_curve_value(x, 0.7).to_bits(),
            "tone_curve_value({x}) 两次求值须逐位一致"
        );
    }
}
