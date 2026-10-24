//! Photo/Svg workflow 预设：白名单常量与三层组成口径的防漂移测试（F 项）。
//!
//! 口径来源：Wang 2026-10-24 两轮裁定 ——
//! 「ALL 要包含所有的，所以必须加进去」；「增加一个 default 作为和 ALL 区分，
//! ALL 包含系统所有功能和工具，default 只包含常用的」。
//! 组成模型定为三层：
//! 1. `Profile::ALL`：含全部 profile；`with_profiles(&ALL)` 与 `full()` 恒等于
//!    注册表全量，任何情况下不收窄；
//! 2. `Profile::DEFAULT`（新）：缺省会话（mcp/http）常用集，成员为基底层
//!    Core/History/Changeset/Retouch/Structure + Photo；不触发白名单收窄，
//!    工具集 = 成员分层并集（全量的真子集，且包含 `PHOTO_TOOLS` 全部）；
//! 3. 白名单收窄只对最终用户显式点名的子集请求生效：`[Photo]` 恰为
//!    `PHOTO_TOOLS`、`[Svg]` 恰为 `SVG_TOOLS`，显式混合按设计 §7 公式
//!    （分层展开集 ∩ 白名单并集）。
//!
//! `IMPLEMENTED` 常量保留（`--profile all` 的展开与已实现全集参照），不再是缺省。

use std::collections::BTreeSet;

use yanshi_server::{profile_summary, Profile, ToolRegistry, PHOTO_TOOLS, SVG_TOOLS};

fn names(registry: &ToolRegistry) -> BTreeSet<&'static str> {
    registry.tools().iter().map(|tool| tool.name).collect()
}

fn name_set(list: &[&'static str]) -> BTreeSet<&'static str> {
    list.iter().copied().collect()
}

/// 不变量 4（防漂移）：两份白名单逐名对账 —— 已注册、无重复、长度合设计 §7，
/// 且每个名字的注册层位落在该预设的基底层集合内（否则求交会静默丢名）。
#[test]
fn photo_allowlist_names_all_registered() {
    let full = ToolRegistry::full();
    // 设计 §7 的清单规模（名字逐个在下面对账；长度钉死防整组漂移）。
    assert_eq!(PHOTO_TOOLS.len(), 93, "PHOTO_TOOLS 长度与设计 §7 不符");
    assert_eq!(SVG_TOOLS.len(), 89, "SVG_TOOLS 长度与设计 §7 不符");
    let cases: [(&[&'static str], &[Profile]); 2] = [
        (
            PHOTO_TOOLS,
            &[
                Profile::Core,
                Profile::History,
                Profile::Changeset,
                Profile::Retouch,
                Profile::Structure,
            ],
        ),
        (
            SVG_TOOLS,
            &[
                Profile::Core,
                Profile::History,
                Profile::Changeset,
                Profile::Structure,
            ],
        ),
    ];
    for (list, base_layers) in cases {
        assert_eq!(
            name_set(list).len(),
            list.len(),
            "白名单内有重复名：{list:?}"
        );
        for name in list {
            let spec = full
                .get(name)
                .unwrap_or_else(|| panic!("白名单名字 {name} 未在注册表中（常量漂移或名字写错）"));
            assert!(
                base_layers.contains(&spec.profile),
                "白名单名字 {name} 的注册层位 {} 不在其预设基底层内，求交会丢名",
                spec.profile.as_str()
            );
        }
    }
    // Svg 的基底不含 Retouch：白名单里不得混入任何 Retouch 层工具。
    for name in SVG_TOOLS {
        assert_ne!(
            full.get(name).map(|spec| spec.profile),
            Some(Profile::Retouch),
            "SVG_TOOLS 不应包含 Retouch 层工具 {name}"
        );
    }
}

/// 不变量 1：ALL 组成与 full() 恒为全量；`all` 简写（IMPLEMENTED）同为全量。
/// 判据取「白名单并集之外」的工具在场 —— 若字面求交规则被误启用，
/// 这些工具会整组消失（Collab/Annotation/Conflict 与部分 Core 工具）。
#[test]
fn all_and_full_never_narrow() {
    let full = ToolRegistry::full();
    let via_all = ToolRegistry::with_profiles(&Profile::ALL);
    let via_implemented = ToolRegistry::with_profiles(&Profile::IMPLEMENTED);
    assert_eq!(
        names(&full),
        names(&via_all),
        "with_profiles(&ALL) 必须等于全量"
    );
    assert_eq!(
        names(&full),
        names(&via_implemented),
        "with_profiles(&IMPLEMENTED)（`--profile all` 的展开）必须等于全量"
    );
    // 白名单并集之外的哨兵工具：评论（Collab）、批注（Annotation）、
    // 冲突消解（Conflict）、智能填充（Core，均不在 PHOTO/SVG 名单内）。
    for sentinel in [
        "comment",
        "create_annotation",
        "resolve_conflict",
        "smart_fill",
    ] {
        assert!(
            full.get(sentinel).is_some(),
            "全量注册表缺少哨兵工具 {sentinel}"
        );
    }
    let union_len = name_set(PHOTO_TOOLS).union(&name_set(SVG_TOOLS)).count();
    assert!(
        full.tools().len() > union_len,
        "全量必须严格大于 PHOTO∪SVG 白名单并集"
    );
}

/// 不变量 2（新口径）：缺省组成 DEFAULT 不触发收窄 —— 工具集为成员分层并集，
/// 是全量的真子集，且包含 PHOTO_TOOLS 全部工具（Photo 只贡献成员资格）。
#[test]
fn default_composition_is_unnarrowed_common_subset() {
    let default = ToolRegistry::with_profiles(&Profile::DEFAULT);
    let full = ToolRegistry::full();
    let default_names = names(&default);
    for name in PHOTO_TOOLS {
        assert!(
            default_names.contains(name),
            "缺省常用集必须包含 PHOTO_TOOLS 全部工具，缺 {name}"
        );
    }
    assert!(
        default.tools().len() < full.tools().len(),
        "缺省常用集必须是全量的真子集"
    );
    // 缺省集必须严格大于照片名单本身：若 DEFAULT 被误收窄到 PHOTO_TOOLS，
    // 下面这些「在成员分层内、但不在照片白名单」的工具会整批消失
    // （变异验证实测：只断言「包含 PHOTO_TOOLS + 真子集」时收窄版也能假过，
    // 故在此钉死并集语义）。
    assert!(
        default.tools().len() > PHOTO_TOOLS.len(),
        "缺省常用集应大于 PHOTO_TOOLS（分层并集 ≠ 白名单本身）"
    );
    for union_only in ["smart_fill", "calibrate"] {
        assert!(
            default.get(union_only).is_some(),
            "缺省常用集应含分层并集工具 {union_only}（不在 PHOTO_TOOLS 内）"
        );
    }
    // 常用集内的哨兵：变更集（历史上因缺组在查看器面板报过「未知工具」）。
    assert!(
        default.get("begin_changeset").is_some(),
        "缺省常用集必须含 Changeset 层（begin_changeset）"
    );
    // 专用层不得混入缺省：评论 / 批注 / 冲突消解。
    for excluded in ["comment", "create_annotation", "resolve_conflict"] {
        assert!(
            default.get(excluded).is_none(),
            "专用层工具 {excluded} 不得出现在缺省常用集"
        );
    }
}

/// 成员表钉死：DEFAULT / IMPLEMENTED / ALL 的成员逐项断言，
/// Wang 日后调整成员时，本测试的 diff 即成员表 diff。
#[test]
fn composition_membership_is_pinned() {
    // DEFAULT 成员表（裁定原文：「default 只包含常用的」；基底层 + Photo 主线）。
    let expected_default = [
        Profile::Core,
        Profile::History,
        Profile::Changeset,
        Profile::Retouch,
        Profile::Structure,
        Profile::Photo,
    ];
    assert_eq!(Profile::DEFAULT.len(), expected_default.len());
    for profile in Profile::ALL {
        assert_eq!(
            Profile::DEFAULT.contains(&profile),
            expected_default.contains(&profile),
            "DEFAULT 成员表漂移：{}",
            profile.as_str()
        );
    }
    // ALL 与 IMPLEMENTED 的关系：只差未开发的 Semantic；Photo/Svg 两边都在。
    assert_eq!(Profile::ALL.len(), Profile::IMPLEMENTED.len() + 1);
    let diff: Vec<Profile> = Profile::ALL
        .iter()
        .copied()
        .filter(|profile| !Profile::IMPLEMENTED.contains(profile))
        .collect();
    assert_eq!(diff, vec![Profile::Semantic]);
    for profile in [Profile::Photo, Profile::Svg] {
        assert!(Profile::ALL.contains(&profile), "ALL 必须包含 {profile:?}");
        assert!(
            Profile::IMPLEMENTED.contains(&profile),
            "IMPLEMENTED 必须包含 {profile:?}"
        );
    }
    // 字符串往返与 `all` 简写：parse(as_str(p)) == p；parse_list("all") == IMPLEMENTED。
    for profile in Profile::ALL {
        assert_eq!(
            Profile::parse(profile.as_str()).ok(),
            Some(profile),
            "profile 字符串往返失败：{}",
            profile.as_str()
        );
    }
    assert_eq!(
        Profile::parse_list("all").expect("all 简写应可解析"),
        Profile::IMPLEMENTED.to_vec()
    );
    assert_eq!(
        Profile::parse_list("photo").expect("photo 应可解析"),
        vec![Profile::Photo]
    );
}

/// 不变量 3（Photo）：显式 `[Photo]` 恰为 PHOTO_TOOLS 集合，一名不多一名不少。
#[test]
fn profile_photo_exposes_exactly_allowlist() {
    let registry = ToolRegistry::with_profiles(&[Profile::Photo]);
    assert_eq!(
        names(&registry),
        name_set(PHOTO_TOOLS),
        "with_profiles(&[Photo]) 必须恰为 PHOTO_TOOLS"
    );
}

/// 不变量 3（Svg）：显式 `[Svg]` 恰为 SVG_TOOLS 集合。
#[test]
fn profile_svg_exposes_exactly_allowlist() {
    let registry = ToolRegistry::with_profiles(&[Profile::Svg]);
    assert_eq!(
        names(&registry),
        name_set(SVG_TOOLS),
        "with_profiles(&[Svg]) 必须恰为 SVG_TOOLS"
    );
}

/// 不变量 3（混合）：显式混合按设计 §7 公式 —— 分层展开集 ∩ 白名单并集；
/// 不带 Photo/Svg 的显式请求不触发收窄。
#[test]
fn explicit_mixes_follow_allowlist_intersection() {
    let photo = name_set(PHOTO_TOOLS);
    let svg = name_set(SVG_TOOLS);
    let union: BTreeSet<&'static str> = photo.union(&svg).copied().collect();

    // photo+svg（经 parse_list 的 CLI 路径）= 两份白名单的并集。
    let listed = Profile::parse_list("photo,svg").expect("photo,svg 应可解析");
    assert_eq!(
        names(&ToolRegistry::with_profiles(&listed)),
        union,
        "[Photo, Svg] 必须是两份白名单的并集"
    );
    // 混入已包含层 / 其他预设，不改变白名单结果。
    assert_eq!(
        names(&ToolRegistry::with_profiles(&[
            Profile::Photo,
            Profile::Retouch
        ])),
        photo,
        "[Photo, Retouch] 应仍恰为 PHOTO_TOOLS"
    );
    assert_eq!(
        names(&ToolRegistry::with_profiles(&[
            Profile::Svg,
            Profile::Retouch
        ])),
        svg,
        "[Svg, Retouch] 应仍恰为 SVG_TOOLS（Retouch 不在 Svg 基底内）"
    );
    assert_eq!(
        names(&ToolRegistry::with_profiles(&[
            Profile::Photo,
            Profile::Oil
        ])),
        photo,
        "[Photo, Oil] 应仍恰为 PHOTO_TOOLS"
    );
    // 不带 Photo/Svg 的显式请求：白名单不参与，纯分层并集。
    let plain = ToolRegistry::with_profiles(&[Profile::Core, Profile::Collab]);
    assert!(
        plain.get("smart_fill").is_some() && plain.get("list_comments").is_some(),
        "无 Photo/Svg 的请求不得被白名单收窄"
    );
    assert!(
        plain.get("add_adjustment").is_none(),
        "分层未启用的工具不得出现"
    );
}

/// 设计 §7 的 summary 对账：显式预设的 profile_summary 计数 == 白名单长度。
#[test]
fn profile_summary_counts_match_allowlist_lengths() {
    let photo_summary = profile_summary(&ToolRegistry::with_profiles(&[Profile::Photo]));
    assert_eq!(
        photo_summary["count"].as_u64(),
        Some(PHOTO_TOOLS.len() as u64),
        "photo 的 profile_summary 计数必须等于 PHOTO_TOOLS 长度"
    );
    let svg_summary = profile_summary(&ToolRegistry::with_profiles(&[Profile::Svg]));
    assert_eq!(
        svg_summary["count"].as_u64(),
        Some(SVG_TOOLS.len() as u64),
        "svg 的 profile_summary 计数必须等于 SVG_TOOLS 长度"
    );
}
