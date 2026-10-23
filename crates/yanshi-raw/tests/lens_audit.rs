//! **第 1440 轮审计：第四包（镜头校正重放）＋第五包（高光档位）的静默降级** ✓
//!
//! 上游 `c3743c12..e8b0ac47` 把「凭落账记录重放 RAW 展开」接进了
//! `develop_raw_source_recorded`。审计查出四处**矛盾输入或降级不吭声**，
//! 本文件逐处一条判据（**每条都能红**，变异见每处注释）。
//!
//! 判据断的全是**结构量**（错误码、点名文字、集合内容）——**不断像素** ✓
//! （AGENTS.md 内核 ①：像素数跨不了机器）。
use serde_json::json;
use yanshi_raw::import::parse_raw_options;
use yanshi_raw::lens::plan_from_applied_report;
use yanshi_raw::{AppliedLensProfile, RawDevelopRecord};

/// **① 显式写 `lens_correction:"off"` 又给 `lens_override` ⇒ 必须报错** ✓
///
/// 旧行为：`resolve_lens` 的 off 早退条件写着「且没给 override」⇒
/// override 一在场就**跳过 off**，照样查库、照样改像素 ⇒
/// **用户显式关校正的意图被静默吃掉**（静默降级第 4 形态：非法组合看起来正常）。
///
/// ⚠ **只判显式写过的那个 `off`**：`lens_correction` 的缺省本身就是 `Off`，
/// 而「只给 `lens_override`」正是第四包的手动点名路径（`lens_pkg4.rs:208` 断言它
/// 必须成功）⇒ 拿缺省值当矛盾会把合法输入一起拒掉。所以这一条同时是
/// **过度严格**的回归保险。
/// 变异：删掉 `import.rs` 里那段矛盾检查 ⇒ 红 ✓；把判据写成按缺省值判
/// （`parsed.lens_correction == Off`）⇒ 最后那条「只给 override」的断言红 ✓
#[test]
fn off_with_override_is_rejected_instead_of_secretly_correcting() {
    let err = parse_raw_options(Some(&json!({
        "lens_correction": "off",
        "lens_override": "FixtureOptics Fixture 24mm f/2.8",
    })))
    .expect_err("显式 off ＋ override 必须被拒（旧版接受它并照样做校正）");
    let text = err.to_string();
    assert!(
        text.contains("互相矛盾"),
        "错误信息要点名矛盾，实测：{text}"
    );
    assert!(
        text.contains("off"),
        "错误信息要带上被矛盾的档位，实测：{text}"
    );

    // 合法组合不许被误伤（**四个方向**都要成立，否则就是把缺陷换成新缺陷）：
    assert!(parse_raw_options(Some(&json!({"lens_correction": "off"}))).is_ok());
    assert!(parse_raw_options(Some(
        &json!({"lens_correction": "off", "lens_override": null})
    ))
    .is_ok());
    assert!(parse_raw_options(Some(&json!({
        "lens_correction": "auto",
        "lens_override": "FixtureOptics Fixture 24mm f/2.8",
    })))
    .is_ok());
    // 只给 override（不写 lens_correction）⇒ 第四包的手动点名，必须仍被接受
    assert!(
        parse_raw_options(Some(
            &json!({"lens_override": "FixtureOptics Fixture 24mm f/2.8"})
        ))
        .is_ok(),
        "手动点名（不写 lens_correction）不许被当成矛盾"
    );
}

/// 造一份「暗角记为 `file_opcode` 但 opcode 里没有可用参数、
/// 畸变走库」的 profile ⇒ 暗角**重建不出来**，畸变**重建得出**。
fn half_broken_profile() -> AppliedLensProfile {
    serde_json::from_value(json!({
        "source_db": "fixture",
        "db_version": "1",
        "lens_id": "FixtureOptics Fixture 24mm f/2.8",
        "matched_via": "matched",
        "focal_mm_effective": 24.0,
        "sources": { "vignetting": "file_opcode", "distortion": "file_opcode", "tca": "none" },
        "distortion_ptlens": null,
        "vignetting_pa": [-0.30, 0.05, 0.0],
        "tca": null,
        "crop_scale_applied": 1.0,
        "crop_ratio": 1.0,
        "opcode": { "warp_rectilinear": { "sets": [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]], "center": [0.5, 0.5] } },
    }))
    .expect("profile 形状应与 serde 一致")
}

/// **② 部分重建失败必须点名丢了哪一项** ✓
///
/// 旧 `plan_from_applied` 只在**三项全丢**时返回 `None` ⇒ 像上面这种
/// 「畸变重建成功、暗角重建失败」照样得 `Some` ⇒ 暗角校正**静默消失**，
/// 重放像素与导入当时不同却没有任何信号。
/// 变异：把 `lens.rs` 里 `dropped` 的两处 push 删掉 ⇒ ② 红 ✓
#[test]
fn partially_rebuildable_plan_names_the_item_it_lost() {
    let profile = half_broken_profile();
    let (plan, dropped) = plan_from_applied_report(&profile);
    assert!(
        plan.is_some(),
        "畸变项可重建 ⇒ plan 应当仍在（本条不是要它整体失败）"
    );
    assert_eq!(
        dropped,
        vec!["vignetting"],
        "被丢的暗角必须点名（sources 记的是 file_opcode，而 opcode 里没有 fix_vignette_radial）"
    );
    // **plan 里确实没有暗角** ⇒ 与 dropped 的点名互相印证（字段之间要写恒等关系）
    assert!(
        plan.unwrap().vignette.is_none(),
        "既然丢了，plan 里就不该有暗角"
    );

    // 对照：**全都重建得出 ⇒ dropped 必须为空**（否则这条判据只是在恒真上安慰我）
    let whole = serde_json::from_value::<AppliedLensProfile>(json!({
        "source_db": "fixture",
        "db_version": "1",
        "lens_id": "FixtureOptics Fixture 24mm f/2.8",
        "matched_via": "matched",
        "focal_mm_effective": 24.0,
        "sources": { "vignetting": "lensfun", "distortion": "lensfun", "tca": "lensfun" },
        "distortion_ptlens": [0.01, -0.03, 0.02],
        "vignetting_pa": [-0.30, 0.05, 0.0],
        "tca": { "model": "linear", "vr": 1.0, "vb": 1.0 },
        "crop_scale_applied": 1.0,
        "crop_ratio": 1.0,
        "opcode": null,
    }))
    .unwrap();
    let (_plan2, dropped2) = plan_from_applied_report(&whole);
    assert!(
        dropped2.is_empty(),
        "三项都可重建时不该点名任何项，实测 {dropped2:?}"
    );

    // 对照二：**丢的是畸变而不是暗角** ⇒ 点名的必须是 `distortion/tca`
    // （`sources` 记 lensfun 路线，可 `distortion_ptlens` 与 `tca` 都是空的）
    let no_warp = serde_json::from_value::<AppliedLensProfile>(json!({
        "source_db": "fixture",
        "db_version": "1",
        "lens_id": "FixtureOptics Fixture 24mm f/2.8",
        "matched_via": "matched",
        "focal_mm_effective": 24.0,
        "sources": { "vignetting": "lensfun", "distortion": "lensfun", "tca": "lensfun" },
        "distortion_ptlens": null,
        "vignetting_pa": [-0.30, 0.05, 0.0],
        "tca": null,
        "crop_scale_applied": 1.0,
        "crop_ratio": 1.0,
        "opcode": null,
    }))
    .unwrap();
    let (plan3, dropped3) = plan_from_applied_report(&no_warp);
    assert_eq!(
        dropped3,
        vec!["distortion/tca"],
        "畸变重建不出来要点名那一项（暗角仍可重建 ⇒ plan 不该整体消失）"
    );
    assert!(plan3.is_some(), "还剩暗角 ⇒ plan 应当仍在");
}

/// 造一份 lens_match / applied_lens_profile 组合的记录（其余字段取合法值）。
fn record_with(lens_match: Option<&str>, with_profile: bool) -> RawDevelopRecord {
    let profile = if with_profile {
        serde_json::to_value(half_broken_profile()).unwrap()
    } else {
        serde_json::Value::Null
    };
    serde_json::from_value(json!({
        "version": 4,
        "source_blob": null,
        "source_format": "dng",
        "camera_make": "Fixture",
        "camera_model": "Cam",
        "lens": "Fixture 24mm",
        "wb": "as_shot",
        "highlight_recovery": "floor",
        "demosaic": "malvar",
        "pre_exposure_ev": 0.0,
        "baseline_exposure_ev": 0.0,
        "wb_multipliers": [1.0, 1.0, 1.0],
        "black_levels": [0.0, 0.0, 0.0, 0.0],
        "white_levels": [16383.0, 16383.0, 16383.0, 16383.0],
        "cfa": "rggb",
        "orientation_applied": 1,
        "decoder_name": "fixture",
        "decoder_version": "1",
        "color_alpha": null,
        "lens_exif": null,
        "lens_match": lens_match,
        "applied_lens_profile": profile,
    }))
    .expect("记录形状应与 serde 一致")
}

/// **③ `lens_match` 声称施加过校正、却没有 profile ⇒ 必须点名矛盾** ✓
///
/// 旧代码在这种情况下走「无校正」分支且**不警告**（只有「有 profile 但三项全丢」
/// 才警告）⇒ 记录与像素互相矛盾而调用方无信号。
/// 变异：删掉 `frame.rs::replay_gaps` 的第一个 match 分支 ⇒ 红 ✓
#[test]
fn record_claiming_correction_without_profile_is_flagged() {
    for label in ["matched", "manual"] {
        let gaps = record_with(Some(label), false).replay_gaps();
        assert_eq!(
            gaps.len(),
            1,
            "lens_match={label} 缺 profile 必须恰好点名一次，实测 {gaps:?}"
        );
        assert!(
            gaps[0].contains(label) && gaps[0].contains("applied_lens_profile"),
            "点名要带上标签与缺的那个字段，实测 {}",
            gaps[0]
        );
    }
    // 自洽的组合不许误伤（**恒假**的判据等于没判）：
    assert!(
        record_with(Some("off"), false).replay_gaps().is_empty(),
        "off 本来就没校正"
    );
    assert!(
        record_with(Some("unmatched"), false)
            .replay_gaps()
            .is_empty(),
        "unmatched 本来就没校正"
    );
    assert!(
        record_with(None, false).replay_gaps().is_empty(),
        "两个字段都缺 ⇒ 没什么可矛盾"
    );
}

/// **④ 出处不许编造** ✓：有 profile 却缺 `lens_match` ⇒ 点名「record_unspecified」，
/// 而不是旧版的 `unwrap_or("matched")` 凭空写一次出处。
/// 变异：把 `replay_gaps` 的第二个分支删掉 ⇒ 红 ✓
#[test]
fn missing_provenance_is_named_not_invented() {
    let gaps = record_with(None, true).replay_gaps();
    assert_eq!(
        gaps.len(),
        1,
        "缺 lens_match 但有 profile ⇒ 必须点名，实测 {gaps:?}"
    );
    assert!(
        gaps[0].contains("record_unspecified"),
        "要点名新用的标签（旧版在这里写成 matched），实测 {}",
        gaps[0]
    );
    assert!(
        !gaps[0].contains("matched ⇒"),
        "不许反过来把 matched 说成正确出处"
    );
    // 自洽组合：有 profile 也有 lens_match ⇒ 不点名
    assert!(record_with(Some("matched"), true).replay_gaps().is_empty());
}

// ---------------------------------------------------------------------------
// ⑤–⑧ **取图侧的搬运**：上面三条是「能说出矛盾」，这一组证明矛盾真的
// 被搬进 `develop_raw_source_recorded` 的 `warnings`（否则 `replay_gaps`
// 只是库里一个没人调的函数）。断的全是**结构与文字**，不断像素 ✓
// ---------------------------------------------------------------------------

/// 合成 DNG 的字节（与 `render_hook.rs` 同法：8×8 常数 RGGB）。
fn fixture_bytes() -> Vec<u8> {
    use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
    let dn = [541u16, 562, 548];
    write_synthetic_dng(&SyntheticDng {
        mosaic: (0..64)
            .map(|i| {
                let (x, y) = (i % 8, i / 8);
                dn[yanshi_raw::CfaPattern::Rggb.channel_at(x, y).index()]
            })
            .collect(),
        ..Default::default()
    })
}

/// 取一份**真实记录**（展开期产出）再改镜头字段 ⇒ 记录形状永远跟 serde 同步，
/// 不像手写 JSON 那样会在上游加字段时悄悄过期。
fn record_json(mutate: impl FnOnce(&mut serde_json::Value)) -> serde_json::Value {
    let bytes = fixture_bytes();
    let base = yanshi_render::develop_raw_source(&bytes, &yanshi_raw::DevelopOptions::default())
        .expect("合成 DNG 必须能展开");
    let mut value = serde_json::to_value(&base.record).unwrap();
    mutate(&mut value);
    value
}

/// **⑤ 「声称校正却无参数」必须出现在取图警告里** ✓
/// 变异：删掉 `render.rs` 的 `warnings.extend(record.replay_gaps())` ⇒ 红 ✓
#[test]
fn replay_warning_reaches_the_render_side_when_profile_is_missing() {
    let bytes = fixture_bytes();
    let value = record_json(|v| {
        v["lens_match"] = json!("matched");
        v["applied_lens_profile"] = json!(null);
    });
    let mut warnings = Vec::new();
    yanshi_render::develop_raw_source_recorded(&bytes, Some(&value), &mut warnings)
        .expect("矛盾记录仍可展开（要的是**可见**，不是拒绝）");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("lens_match=matched") && w.contains("applied_lens_profile")),
        "取图警告要点名这对矛盾字段，实测 {warnings:?}"
    );
}

/// **⑥ 部分丢失的那一项要被点名到警告里** ✓
/// 变异：删掉 `render.rs` 里 `for item in dropped` 那段 ⇒ 红 ✓
#[test]
fn partially_dropped_lens_item_is_named_in_the_replay_warnings() {
    let bytes = fixture_bytes();
    let profile = serde_json::to_value(half_broken_profile()).unwrap();
    let value = record_json(|v| {
        v["lens_match"] = json!("matched");
        v["applied_lens_profile"] = profile.clone();
    });
    let mut warnings = Vec::new();
    yanshi_render::develop_raw_source_recorded(&bytes, Some(&value), &mut warnings)
        .expect("可部分重建的 profile 必须仍能展开");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("vignetting") && w.contains("未施加")),
        "被丢的暗角要在警告里点名（旧版这里只有「三项全丢」才吭声），实测 {warnings:?}"
    );
    // 恒等关系：**只**丢暗角 ⇒ 不许把畸变也报成丢了
    assert!(
        !warnings.iter().any(|w| w.contains("distortion/tca")),
        "畸变可重建 ⇒ 不许误报，实测 {warnings:?}"
    );
}

/// **⑦ 缺出处时回写记录的标签是 `record_unspecified`，不是编造的 `matched`** ✓
/// 变异：把 `render.rs` 的 `unwrap_or("record_unspecified")` 改回 `unwrap_or("matched")` ⇒ 红 ✓
#[test]
fn replay_records_unknown_provenance_instead_of_inventing_matched() {
    let bytes = fixture_bytes();
    let profile = serde_json::to_value(half_broken_profile()).unwrap();
    let value = record_json(|v| {
        v["lens_match"] = json!(null);
        v["applied_lens_profile"] = profile.clone();
    });
    let mut warnings = Vec::new();
    let out = yanshi_render::develop_raw_source_recorded(&bytes, Some(&value), &mut warnings)
        .expect("有 profile 就要走带校正的展开");
    assert_eq!(
        out.record.lens_match.as_deref(),
        Some("record_unspecified"),
        "出处未知要如实记账"
    );
    assert!(
        warnings.iter().any(|w| w.contains("record_unspecified")),
        "同时要在警告里点名，实测 {warnings:?}"
    );
}

/// **⑧ 无记录时的警告要说清用了哪一档高光** ✓（缺省是 `floor`，而导入契约是 `chroma`）
/// 变异：把 `render.rs` 里那句警告的 `{highlight}` 部分删掉 ⇒ 红 ✓
#[test]
fn missing_record_warning_names_the_highlight_mode() {
    let bytes = fixture_bytes();
    let mut warnings = Vec::new();
    yanshi_render::develop_raw_source_recorded(&bytes, None, &mut warnings)
        .expect("缺记录要能按缺省展开");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("缺少 raw_develop 记录") && w.contains("高光档位 = floor")),
        "缺省展开的警告必须点名实际档位，实测 {warnings:?}"
    );
}
