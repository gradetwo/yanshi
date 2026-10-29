//! **第 4 轮第 2 期笔刷 dab 参数守门** ✓（`paint-r4-p2`）：
//!
//! 第 3 轮分诊定的三个 dab 缺陷（`TRIAGE-ROUND3.md` §1：素描 P1、油画 P-2、
//! 油画 P-1）都是 `.myb` 参数层面的问题，修法也是改参数 —— 参数一改回，
//! 缺陷就原样回来，而且渲染目检不会天天跑。所以这里把三处修好的参数
//! 下限定死：**只断参数，不画素素**（像素数跨不了机器，见仓库 `AGENTS.md`
//! 内核 1；3x 目检证据在交付包的对照图里，不在这里）。
//!
//! 它证明不了的也写清 ✗：参数在位 ≠ 画面一定通过 —— 最终验收仍是
//! 探针 PNG 的 1:1 + 3x 目检（交付包 `CHANGES-*.md` 里有 verdict）。

use hokusai::{myb, Brush, BrushInput, BrushSetting};

fn brush(name: &str) -> Brush {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/brushes")
        .join(name);
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不到 {}：{error}", path.display()));
    myb::from_str(&json).unwrap_or_else(|error| panic!("{name} 应当能解析：{error}"))
}

fn base(brush: &Brush, setting: BrushSetting) -> f32 {
    brush.get(setting).base_value
}

/// 某设置在全部输入曲线上的最大求值 = base + 各输入曲线最大值之和
/// （曲线分段线性，最大值必在某个 knot 上；这是上界，够守门用）。
fn max_evaluated(brush: &Brush, setting: BrushSetting) -> f32 {
    let value = brush.get(setting);
    let mut total = value.base_value;
    for mapping in &value.inputs {
        let peak = mapping
            .points
            .iter()
            .map(|(_, output)| *output)
            .fold(f32::NEG_INFINITY, f32::max);
        if peak.is_finite() {
            total += peak;
        }
    }
    let _ = BrushInput::Pressure;
    total
}

/// 素描 P1：2B 单 dab 必须是盘，不是颗粒簇。根因是硬度 0.12（dab 可见
/// 核只剩约 2px）+ 随机偏移 1.22（dab 中心散到 ±1.22 倍半径外）。
/// 修后：硬度 0.7，随机偏移求值恒 ≤ 0（base 0 + 全 ≤ 0 的压感曲线）。
#[test]
fn pencil_2b_dab_is_a_disk_not_a_grain_cluster() {
    let brush = brush("2B_pencil.myb");
    let hardness = base(&brush, BrushSetting::Hardness);
    assert!(
        hardness >= 0.5,
        "2B 硬度 {hardness} < 0.5 ⇒ dab 核又缩回颗粒点（修前 0.12）"
    );
    let offset = max_evaluated(&brush, BrushSetting::OffsetByRandom);
    assert!(
        offset <= 0.05,
        "2B 随机偏移上界 {offset} > 0.05 ⇒ stray 点又会散出主盘（修前约 0.52）"
    );
}

/// 油画 P-2：oil-01 / oil-03 的 dab 必须是实心体，不是放射鬃毛。
/// 根因是椭圆比 7.1（dab 是沿方向的细长条）+ 随机偏移 + 跟踪噪声。
/// 修后：椭圆比 base 1.0，随机偏移与跟踪噪声 base 均为 0。
#[test]
fn oil_01_and_oil_03_dabs_are_solid_not_bristled() {
    for name in ["oil-01-paint.myb", "oil-03-paint.myb"] {
        let brush = brush(name);
        let ratio = base(&brush, BrushSetting::EllipticalDabRatio);
        assert!(
            ratio <= 1.5,
            "{name} 椭圆比 {ratio} > 1.5 ⇒ dab 又变回细长鬃毛条（修前 7.1）"
        );
        let offset = base(&brush, BrushSetting::OffsetByRandom);
        assert!(
            offset == 0.0,
            "{name} 随机偏移 {offset} ≠ 0 ⇒ dab 中心又会散开（修前 0.6/0.4）"
        );
        let noise = base(&brush, BrushSetting::TrackingNoise);
        assert!(
            noise == 0.0,
            "{name} 跟踪噪声 {noise} ≠ 0 ⇒ 落点又会抖出毛刺（修前 0.2）"
        );
    }
}

/// 油画 P-1：新增厚涂笔刷必须在位，且锥度机制在位。
/// 厚涂 = extreme_round_l 的实心漆体（硬度 0.89）+ pressure_shape -1
/// 的头粗尾尖包络（起笔饱满、收笔收尖；见 stroke.rs 的负值分支）。
#[test]
fn impasto_comma_brush_exists_with_head_thick_tail_thin_taper() {
    let brush = brush("impasto_comma.myb");
    let taper = base(&brush, BrushSetting::PressureShape);
    assert!(
        (taper - (-1.0)).abs() < 1e-6,
        "impasto_comma 的 pressure_shape {taper} ≠ -1.0 ⇒ 头粗尾尖包络丢了"
    );
    let hardness = base(&brush, BrushSetting::Hardness);
    assert!(
        hardness >= 0.8,
        "impasto_comma 硬度 {hardness} < 0.8 ⇒ 漆体边缘不再 crisp"
    );
    let offset = base(&brush, BrushSetting::OffsetByRandom);
    assert!(
        offset == 0.0,
        "impasto_comma 随机偏移 {offset} ≠ 0 ⇒ 单弯体会散成颗粒带"
    );
}
