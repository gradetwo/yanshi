//! 镜头校正数学（设计 5.9.2／5.9.3／5.9.6，第四包）：标定插值、
//! 画幅换算、三项校正的评估（暗角增益场 / 畸变+TCA 复合 warp /
//! 自动裁剪），以及「解析结果 ↔ 落账 profile」的双向构造——导入期
//! 与取图期共用这一份实现，参数口径不许有第二个出处。
//!
//! 模型口径（与 Lensfun 公开文档一致，代码为自写实现）：
//! - 畸变只认 PTLens：`r_d = r_u·((1−a−b−c) + c·r_u + b·r_u² + a·r_u³)`，
//!   r 为半对角归一化半径；poly3 等其他模型的条目**跳过并留痕**。
//! - 暗角只认 pa：`V(r) = 1 + k1·r² + k2·r⁴ + k3·r⁶`，增益 `1/V(r)`。
//! - TCA：linear `r_R = vr·r, r_B = vb·r`；poly3 另加三次项
//!   `+ br·r³ / + bb·r³`；G 为参照通道（系数恒 1）。
//! - 画幅换算：`r_model = r_img·(C_cal/C_img)`——在标定口径里评估
//!   完再折回像面，等价于多项式系数按 s 的幂缩放（暗角 k 按
//!   s²/s⁴/s⁶、TCA 三次项按 s²），本模块在构造 plan 时一次性折算，
//!   评估路径只剩像面口径一种，防两处换算不一致。

use crate::frame::{
    AppliedLensProfile, LensCorrectionPlan, LensExif, TcaParams, VignetteModel, WarpModel,
    WarpRectilinearSet,
};
use crate::lens_db::{DistortionEntry, LensEntry, TcaEntry, VignettingEntry};

// ---------------------------------------------------------------------------
// 插值（设计 5.9.3）：畸变/TCA 对焦距线性；暗角三维（焦距线性、
// 光圈 log2 EV 线性、距离 1/D 线性）。范围外一律钳到端档（clamp），
// 不外推——外推径向多项式是经典的画面爆炸源。
// ---------------------------------------------------------------------------

/// 在有序刻度上找包围区间与局部权重 t∈[0,1]（范围外钳到端点）。
fn bracket(scale: &[f32], x: f32) -> (usize, usize, f32) {
    if scale.len() == 1 || x <= scale[0] {
        return (0, 0, 0.0);
    }
    let last = scale.len() - 1;
    if x >= scale[last] {
        return (last, last, 0.0);
    }
    let mut i = 0;
    while i + 1 < scale.len() && scale[i + 1] < x {
        i += 1;
    }
    let t = (x - scale[i]) / (scale[i + 1] - scale[i]);
    (i, i + 1, t)
}

fn sorted_unique(values: impl Iterator<Item = f32>) -> Vec<f32> {
    let mut out: Vec<f32> = values.filter(|v| v.is_finite()).collect();
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    out
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// 畸变插值结果：PTLens 参数 + 是否只有非 PTLens 条目（被跳过）。
pub struct DistortionInterp {
    pub ptlens: Option<[f32; 3]>,
    /// 库里有畸变条目但模型全非 ptlens ⇒ true（调用方留痕「跳过」）。
    pub skipped_non_ptlens: bool,
}

/// 对焦距线性插值 PTLens (a, b, c)；同焦距多条取第一条（库数据口径）。
pub fn interpolate_distortion(entries: &[DistortionEntry], focal: f32) -> DistortionInterp {
    let usable: Vec<&DistortionEntry> = entries
        .iter()
        .filter(|e| e.model.eq_ignore_ascii_case("ptlens"))
        .collect();
    if usable.is_empty() {
        return DistortionInterp {
            ptlens: None,
            skipped_non_ptlens: !entries.is_empty(),
        };
    }
    let scale = sorted_unique(usable.iter().map(|e| e.focal));
    let coeffs_at = |f: f32| -> [f32; 3] {
        let e = usable
            .iter()
            .find(|e| (e.focal - f).abs() < 1e-6)
            .expect("刻度来自条目本身，必有对应条目");
        [e.a, e.b, e.c]
    };
    let (i0, i1, t) = bracket(&scale, focal);
    let ptlens = if i0 == i1 {
        coeffs_at(scale[i0])
    } else {
        lerp3(coeffs_at(scale[i0]), coeffs_at(scale[i1]), t)
    };
    DistortionInterp {
        ptlens: Some(ptlens),
        skipped_non_ptlens: false,
    }
}

/// 对焦距线性插值 TCA（linear/poly3 归一到 `TcaParams`）。
pub fn interpolate_tca(entries: &[TcaEntry], focal: f32) -> Option<TcaParams> {
    let usable: Vec<&TcaEntry> = entries
        .iter()
        .filter(|e| e.model.eq_ignore_ascii_case("linear") || e.model.eq_ignore_ascii_case("poly3"))
        .collect();
    if usable.is_empty() {
        return None;
    }
    let scale = sorted_unique(usable.iter().map(|e| e.focal));
    let at = |f: f32| -> (bool, [f32; 4]) {
        let e = usable
            .iter()
            .find(|e| (e.focal - f).abs() < 1e-6)
            .expect("刻度来自条目本身，必有对应条目");
        (
            e.model.eq_ignore_ascii_case("poly3"),
            [e.vr, e.vb, e.br, e.bb],
        )
    };
    let (i0, i1, t) = bracket(&scale, focal);
    let (poly_a, va) = at(scale[i0]);
    let (poly_b, vb) = at(scale[i1]);
    let mix = |a: f32, b: f32| a + (b - a) * t;
    Some(TcaParams {
        // 两端模型不一致时按 poly3 处理（三次项在一端为 0，插值仍连续）。
        poly3: poly_a || poly_b,
        vr: mix(va[0], vb[0]),
        vb: mix(va[1], vb[1]),
        br: mix(va[2], vb[2]),
        bb: mix(va[3], vb[3]),
    })
}

/// 暗角插值结果（三维插值后的 pa 参数 + 两项「假定」标记）。
pub struct VignettingInterp {
    pub pa: [f32; 3],
    /// EXIF 无光圈 ⇒ 取了该焦距下任一可用光圈档（设计 5.9.3）。
    pub aperture_assumed: bool,
    /// EXIF 无对焦距离 ⇒ 取了最大距离档（设计 5.9.3）。
    pub distance_assumed: bool,
}

/// 暗角三维插值：先在每个包围焦距档内做 (光圈 log2, 距离 1/D) 双线性，
/// 再对焦距线性混合。某焦距档内网格不全（缺角）时该档退化为同空间的
/// 反距离加权（幂 2）——真实 Lensfun 数据个别焦距档确有缺格，静默
/// 丢掉整项比降级插值更糟；网格齐全时走精确双线性，判据按此对账。
pub fn interpolate_vignetting(
    entries: &[VignettingEntry],
    focal: f32,
    aperture: Option<f32>,
    distance: Option<f32>,
) -> Option<VignettingInterp> {
    let usable: Vec<&VignettingEntry> = entries
        .iter()
        .filter(|e| e.model.eq_ignore_ascii_case("pa") && e.aperture > 0.0)
        .collect();
    if usable.is_empty() {
        return None;
    }
    let mut aperture_assumed = false;
    let mut distance_assumed = false;
    // 目标光圈：缺失 ⇒ 任一可用档（取最小光圈值档，行为确定）。
    let target_ap = match aperture {
        Some(a) if a > 0.0 => a,
        _ => {
            aperture_assumed = true;
            usable
                .iter()
                .map(|e| e.aperture)
                .fold(f32::INFINITY, f32::min)
        }
    };
    // 目标距离：缺失 ⇒ 最大距离档（无穷远近似，设计定版）。
    let target_dist = match distance {
        Some(d) if d > 0.0 => d,
        _ => {
            distance_assumed = true;
            usable.iter().map(|e| e.distance).fold(0.0f32, f32::max)
        }
    };
    let at_focal = |f: f32| -> Option<[f32; 3]> {
        let points: Vec<&VignettingEntry> = usable
            .iter()
            .copied()
            .filter(|e| (e.focal - f).abs() < 1e-6)
            .collect();
        if points.is_empty() {
            return None;
        }
        let aps = sorted_unique(points.iter().map(|e| e.aperture));
        // 距离轴按 1/D 排：倒数升序 ⇔ 距离降序，bracket 的下标要与
        // 取值数组严格同序，下面取角点时按这份顺序回查原距离。
        let dists_desc = {
            let mut v = sorted_unique(points.iter().map(|e| e.distance));
            v.reverse();
            v
        };
        let get = |ap: f32, d: f32| -> Option<[f32; 3]> {
            points
                .iter()
                .find(|e| (e.aperture - ap).abs() < 1e-6 && (e.distance - d).abs() < 1e-6)
                .map(|e| [e.k1, e.k2, e.k3])
        };
        // 插值空间（设计 5.9.3）：光圈走 log2 EV、距离走 1/D——
        // bracket 必须在变换后的刻度上找区间，拿原值线性找档会把
        // 中点算偏（本函数的第一个判据就是为此而写）。
        let aps_log: Vec<f32> = aps.iter().map(|&a| libm::log2f(a)).collect();
        let dists_rec: Vec<f32> = dists_desc.iter().map(|&d| 1.0 / d).collect();
        let (a0, a1, ta) = bracket(&aps_log, libm::log2f(target_ap));
        let (d0, d1, td) = bracket(&dists_rec, 1.0 / target_dist);
        let corners = [
            get(aps[a0], dists_desc[d0]),
            get(aps[a0], dists_desc[d1]),
            get(aps[a1], dists_desc[d0]),
            get(aps[a1], dists_desc[d1]),
        ];
        if corners.iter().all(|c| c.is_some()) {
            let c: Vec<[f32; 3]> = corners.into_iter().map(|c| c.unwrap()).collect();
            let lo = lerp3(c[0], c[1], td);
            let hi = lerp3(c[2], c[3], td);
            return Some(lerp3(lo, hi, ta));
        }
        // 网格缺角：(log2 光圈, 1/距离) 空间反距离加权（目标点与样本
        // 重合时直接取该样本，避免 0 距离奇点）。
        let tx = libm::log2f(target_ap);
        let ty = 1.0 / target_dist;
        let mut sum = [0.0f32; 3];
        let mut wsum = 0.0f32;
        for e in &points {
            let dx = libm::log2f(e.aperture) - tx;
            let dy = 1.0 / e.distance - ty;
            let dist2 = dx * dx + dy * dy;
            if dist2 < 1e-12 {
                return Some([e.k1, e.k2, e.k3]);
            }
            let w = 1.0 / dist2;
            for c in 0..3 {
                sum[c] += w * [e.k1, e.k2, e.k3][c];
            }
            wsum += w;
        }
        Some([sum[0] / wsum, sum[1] / wsum, sum[2] / wsum])
    };
    let scale = sorted_unique(usable.iter().map(|e| e.focal));
    let (f0, f1, tf) = bracket(&scale, focal);
    let pa = match (at_focal(scale[f0]), at_focal(scale[f1])) {
        (Some(a), Some(b)) => {
            if f0 == f1 {
                a
            } else {
                lerp3(a, b, tf)
            }
        }
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => return None,
    };
    Some(VignettingInterp {
        pa,
        aperture_assumed,
        distance_assumed,
    })
}

// ---------------------------------------------------------------------------
// 解析：库条目 + 拍摄参数 → 三项参数（库原样口径）+ 逐项状态。
// 文件自带校正的合并在 import 层做（逐项优先），本层只管库一侧。
// ---------------------------------------------------------------------------

/// 一支镜头在本次拍摄参数下的解析结果（参数仍是库原样/标定口径，
/// 画幅折算在构造 plan 时做）。
pub struct LensfunResolution {
    pub distortion: Option<[f32; 3]>,
    pub distortion_skipped_non_ptlens: bool,
    pub tca: Option<TcaParams>,
    pub vignetting: Option<[f32; 3]>,
    pub aperture_assumed: bool,
    pub distance_assumed: bool,
    pub focal_mm_effective: f32,
}

/// 把库条目按拍摄参数解析成三项参数；焦距缺失 ⇒ `None`（无从插值，
/// 自动与手动路径都一样——手动只定镜头身份，不编焦距）。
pub fn resolve_lensfun(entry: &LensEntry, exif: &LensExif) -> Option<LensfunResolution> {
    let focal = exif.focal_mm?;
    if !focal.is_finite() || focal <= 0.0 {
        return None;
    }
    let distortion = interpolate_distortion(&entry.distortion, focal);
    let tca = interpolate_tca(&entry.tca, focal);
    let vig = interpolate_vignetting(
        &entry.vignetting,
        focal,
        exif.fnumber,
        exif.focus_distance_m,
    );
    Some(LensfunResolution {
        distortion: distortion.ptlens,
        distortion_skipped_non_ptlens: distortion.skipped_non_ptlens,
        tca,
        vignetting: vig.as_ref().map(|v| v.pa),
        aperture_assumed: vig.as_ref().map(|v| v.aperture_assumed).unwrap_or(false),
        distance_assumed: vig.as_ref().map(|v| v.distance_assumed).unwrap_or(false),
        focal_mm_effective: focal,
    })
}

// ---------------------------------------------------------------------------
// plan 构造（像面折算的唯一出处）：导入期与取图期（凭 profile 重建）
// 都经这里，评估函数只认像面口径的 plan。
// ---------------------------------------------------------------------------

/// pa 参数的画幅折算：V 的自变量 r 按 s 放大 ⇒ k1 按 s²、k2 按
/// s⁴、k3 按 s⁶ 折算后，同一多项式在像面 r 上成立。
pub fn pa_effective(k: [f32; 3], s: f32) -> [f32; 3] {
    let s2 = s * s;
    [k[0] * s2, k[1] * s2 * s2, k[2] * s2 * s2 * s2]
}

/// 由（三项参数 + 各项来源 + 文件校正参数）组装 plan 与落账 profile。
/// 来源逐项取值："lensfun" / "file_opcode" / "none" /
/// "skipped_non_ptlens"（仅 distortion）。crop_scale 由调用方在拿到
/// 帧尺寸后用 `auto_crop_scale` 算定再回填 profile（plan 本身不存
/// scale，评估时由 develop 现算——同一个函数，两处同值）。
#[allow(clippy::too_many_arguments)]
pub fn build_plan_and_profile(
    distortion: Option<[f32; 3]>,
    tca: Option<TcaParams>,
    vignetting_pa: Option<[f32; 3]>,
    sources: serde_json::Value,
    file_warp: Option<(Vec<WarpRectilinearSet>, [f32; 2])>,
    file_vignette: Option<([f32; 5], [f32; 2])>,
    crop_ratio: f32,
    source_db: &str,
    db_version: &str,
    lens_id: &str,
    matched_via: &str,
    focal_mm_effective: Option<f32>,
) -> (LensCorrectionPlan, AppliedLensProfile) {
    // 先凭引用把 profile 用的 opcode JSON 组好（file_warp/file_vignette
    // 随后要 move 进 plan，先借用后移动，顺序不许反）。
    let opcode_json = if file_warp.is_some() || file_vignette.is_some() {
        let mut obj = serde_json::Map::new();
        if let Some((sets, center)) = &file_warp {
            obj.insert(
                "warp_rectilinear".to_string(),
                serde_json::json!({
                    "sets": sets.iter().map(|s| {
                        serde_json::json!([s.kr[0], s.kr[1], s.kr[2], s.kr[3], s.kt[0], s.kt[1]])
                    }).collect::<Vec<_>>(),
                    "center": [center[0], center[1]],
                }),
            );
        }
        if let Some((k, center)) = &file_vignette {
            obj.insert(
                "fix_vignette_radial".to_string(),
                serde_json::json!({ "k": [k[0], k[1], k[2], k[3], k[4]], "center": [center[0], center[1]] }),
            );
        }
        Some(serde_json::Value::Object(obj))
    } else {
        None
    };
    let mut plan = LensCorrectionPlan::default();
    // 暗角：逐项优先已由调用方裁决——传进来的二选一。
    if let Some((k, center)) = file_vignette {
        plan.vignette = Some(VignetteModel::OpcodeGain { k, center });
    } else if let Some(pa) = vignetting_pa {
        plan.vignette = Some(VignetteModel::Pa(pa_effective(pa, crop_ratio)));
    }
    // warp：文件 WarpRectilinear 在场即整项由它承担（畸变+逐平面
    // 色差同在一个 opcode 里，库 TCA 不叠加——调用方在这种情况
    // 下不会把库 TCA 传进来，见 import 的逐项裁决）。
    if let Some((sets, center)) = file_warp {
        plan.warp = Some(WarpModel::OpcodeWarp { sets, center });
    } else if distortion.is_some() || tca.is_some() {
        plan.warp = Some(WarpModel::PtlensTca {
            ptlens: distortion.unwrap_or([0.0, 0.0, 0.0]),
            tca,
            crop_ratio,
        });
    }
    let tca_json = tca.map(|t| {
        serde_json::json!({
            "model": if t.poly3 { "poly3" } else { "linear" },
            "vr": t.vr, "vb": t.vb, "br": t.br, "bb": t.bb,
        })
    });
    let profile = AppliedLensProfile {
        source_db: source_db.to_string(),
        db_version: db_version.to_string(),
        lens_id: lens_id.to_string(),
        matched_via: matched_via.to_string(),
        focal_mm_effective,
        sources,
        distortion_ptlens: distortion,
        vignetting_pa,
        tca: tca_json,
        crop_scale_applied: 1.0, // 调用方按帧尺寸算定后回填
        crop_ratio: Some(crop_ratio),
        opcode: opcode_json,
    };
    (plan, profile)
}

/// 取图期重建 ＋ **点名丢了哪几项**（第 1440 轮审计）。
///
/// 原 `plan_from_applied` 只说「三项全不可重建 ⇒ `None`」⇒ **部分**失败时
/// 剩下那一项照样成 `Some`，被丢的那项（例如记为 `file_opcode` 却解析不出
/// 参数的暗角）**没人知道** ⇒ 重放悄悄改了像素。返回的 `dropped` 逐项点名，
/// 调用方必须把它变成可见警告（不许吞掉）。
pub fn plan_from_applied_report(
    profile: &AppliedLensProfile,
) -> (Option<LensCorrectionPlan>, Vec<&'static str>) {
    let s = profile.crop_ratio.unwrap_or(1.0);
    let src = |key: &str| -> String {
        profile
            .sources
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("none")
            .to_string()
    };
    // 文件校正参数（profile.opcode 的 JSON 原文）→ 结构。
    let file_warp = profile.opcode.as_ref().and_then(|op| {
        let warp = op.get("warp_rectilinear")?;
        let sets_json = warp.get("sets")?.as_array()?;
        let mut sets = Vec::new();
        for item in sets_json {
            let a = item.as_array()?;
            if a.len() != 6 {
                return None;
            }
            let num = |i: usize| a[i].as_f64().map(|v| v as f32);
            sets.push(WarpRectilinearSet {
                kr: [num(0)?, num(1)?, num(2)?, num(3)?],
                kt: [num(4)?, num(5)?],
            });
        }
        let center = warp.get("center")?.as_array()?;
        Some((
            sets,
            [
                center.first()?.as_f64()? as f32,
                center.get(1)?.as_f64()? as f32,
            ],
        ))
    });
    let file_vignette = profile.opcode.as_ref().and_then(|op| {
        let vig = op.get("fix_vignette_radial")?;
        let k = vig.get("k")?.as_array()?;
        if k.len() != 5 {
            return None;
        }
        let num = |i: usize| k[i].as_f64().map(|v| v as f32);
        let center = vig.get("center")?.as_array()?;
        Some((
            [num(0)?, num(1)?, num(2)?, num(3)?, num(4)?],
            [
                center.first()?.as_f64()? as f32,
                center.get(1)?.as_f64()? as f32,
            ],
        ))
    });
    let tca = profile.tca.as_ref().and_then(|t| {
        Some(TcaParams {
            poly3: t.get("model")?.as_str()? == "poly3",
            vr: t.get("vr")?.as_f64()? as f32,
            vb: t.get("vb")?.as_f64()? as f32,
            br: t.get("br")?.as_f64().unwrap_or(0.0) as f32,
            bb: t.get("bb")?.as_f64().unwrap_or(0.0) as f32,
        })
    });
    let mut plan = LensCorrectionPlan::default();
    if src("vignetting") == "file_opcode" {
        if let Some((k, center)) = file_vignette {
            plan.vignette = Some(VignetteModel::OpcodeGain { k, center });
        }
    } else if let Some(pa) = profile.vignetting_pa {
        plan.vignette = Some(VignetteModel::Pa(pa_effective(pa, s)));
    }
    if src("distortion") == "file_opcode" || src("tca") == "file_opcode" {
        if let Some((sets, center)) = file_warp {
            plan.warp = Some(WarpModel::OpcodeWarp { sets, center });
        }
    } else if profile.distortion_ptlens.is_some() || tca.is_some() {
        plan.warp = Some(WarpModel::PtlensTca {
            ptlens: profile.distortion_ptlens.unwrap_or([0.0, 0.0, 0.0]),
            tca,
            crop_ratio: s,
        });
    }
    let mut dropped: Vec<&'static str> = Vec::new();
    if src("vignetting") != "none" && plan.vignette.is_none() {
        dropped.push("vignetting");
    }
    if (src("distortion") != "none" || src("tca") != "none") && plan.warp.is_none() {
        dropped.push("distortion/tca");
    }
    if plan.vignette.is_none() && plan.warp.is_none() {
        (None, dropped)
    } else {
        (Some(plan), dropped)
    }
}

/// 取图期重建：凭落账 profile 还原 plan（与导入期同一套折算）。
/// profile 残缺到无法重建任一项 ⇒ 返回能重建的部分；三项全无 ⇒ None。
/// **需要知道丢了哪几项的调用方用 `plan_from_applied_report`**（第 1440 轮）。
pub fn plan_from_applied(profile: &AppliedLensProfile) -> Option<LensCorrectionPlan> {
    plan_from_applied_report(profile).0
}

// ---------------------------------------------------------------------------
// 评估：暗角增益场 / warp 源坐标 / 自动裁剪 scale。
// 坐标约定与 develop 其余步骤一致：像素中心在整数坐标，画面中心
// ((w−1)/2, (h−1)/2)，半对角 = 中心到角点的距离。
// ---------------------------------------------------------------------------

/// 画面中心与半对角（像素）。
fn frame_geometry(width: usize, height: usize) -> (f32, f32, f32) {
    let cx = (width as f32 - 1.0) / 2.0;
    let cy = (height as f32 - 1.0) / 2.0;
    (cx, cy, libm::sqrtf(cx * cx + cy * cy))
}

/// 逐像素暗角增益场（与步骤 ② 施加的乘子同一份；同一份场还要喂
/// ④′ 的高光阈值——设计 5.5 的接线义务，第四包履约点）。
pub fn vignette_gain_field(model: &VignetteModel, width: usize, height: usize) -> Vec<f32> {
    let (cx, cy, half_diag) = frame_geometry(width, height);
    let mut field = vec![1.0f32; width * height];
    match *model {
        VignetteModel::Pa(k) => {
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - cx;
                    let dy = y as f32 - cy;
                    let r = libm::sqrtf(dx * dx + dy * dy) / half_diag;
                    let r2 = r * r;
                    let v = 1.0 + k[0] * r2 + k[1] * r2 * r2 + k[2] * r2 * r2 * r2;
                    // V ≤ 0 是标定外推病态（增益发散）：钳到 20 倍增益
                    // 并保持正有限（highlight_recover 的场合法性要求）。
                    field[y * width + x] = 1.0 / v.max(0.05);
                }
            }
        }
        VignetteModel::OpcodeGain { k, center } => {
            // DNG 口径：光心按归一化坐标定位，m = 光心到最远角点距离。
            let ox = center[0] * (width as f32 - 1.0);
            let oy = center[1] * (height as f32 - 1.0);
            let mut m = 0.0f32;
            for (px, py) in [
                (0.0, 0.0),
                (width as f32 - 1.0, 0.0),
                (0.0, height as f32 - 1.0),
                (width as f32 - 1.0, height as f32 - 1.0),
            ] {
                let d = libm::sqrtf((px - ox) * (px - ox) + (py - oy) * (py - oy));
                m = m.max(d);
            }
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 - ox;
                    let dy = y as f32 - oy;
                    let r = if m > 0.0 {
                        libm::sqrtf(dx * dx + dy * dy) / m
                    } else {
                        0.0
                    };
                    let r2 = r * r;
                    let g = 1.0
                        + k[0] * r2
                        + k[1] * r2 * r2
                        + k[2] * r2 * r2 * r2
                        + k[3] * r2 * r2 * r2 * r2
                        + k[4] * r2 * r2 * r2 * r2 * r2;
                    field[y * width + x] = g.clamp(0.05, 100.0);
                }
            }
        }
    }
    field
}

/// warp 评估：输出像素 (x, y) 第 `channel` 通道应从源图哪个坐标采样
/// （浮点像素坐标）。`crop_scale` ≥ 1：输出采样窗先绕画面中心按
/// 1/scale 收缩（自动裁剪），再过 warp 模型。
pub fn warp_source_coord(
    model: &WarpModel,
    channel: usize,
    x: f32,
    y: f32,
    width: usize,
    height: usize,
    crop_scale: f32,
) -> (f32, f32) {
    let (cx, cy, half_diag) = frame_geometry(width, height);
    // 自动裁剪的收缩（两模型共用同一语义）。
    let ox = cx + (x - cx) / crop_scale;
    let oy = cy + (y - cy) / crop_scale;
    match *model {
        WarpModel::PtlensTca {
            ptlens: [a, b, c],
            tca,
            crop_ratio: s,
        } => {
            // 像面归一化位移 → 标定口径（×s）评估 PTLens → 折回（÷s）。
            let dx = (ox - cx) / half_diag * s;
            let dy = (oy - cy) / half_diag * s;
            let r = libm::sqrtf(dx * dx + dy * dy);
            let poly = (1.0 - a - b - c) + c * r + b * r * r + a * r * r * r;
            // 病态参数下 poly 可 ≤ 0（映射折叠）：钳到小正值，采样由
            // reflect 兜底，不许坐标翻负翻出画面外无界。
            let poly = poly.max(0.05);
            let mut sx = dx * poly;
            let mut sy = dy * poly;
            // TCA：对畸变后的半径按通道缩放（G=1 为参照）。
            if let Some(t) = tca {
                let factor = match channel {
                    0 => {
                        if t.poly3 {
                            t.vr + t.br * (sx * sx + sy * sy)
                        } else {
                            t.vr
                        }
                    }
                    2 => {
                        if t.poly3 {
                            t.vb + t.bb * (sx * sx + sy * sy)
                        } else {
                            t.vb
                        }
                    }
                    _ => 1.0,
                };
                sx *= factor;
                sy *= factor;
            }
            (cx + sx / s * half_diag, cy + sy / s * half_diag)
        }
        WarpModel::OpcodeWarp { ref sets, center } => {
            // DNG 规范公式（1.7 第 7 章 WarpRectilinear）：坐标先归一化
            // 到光心系（分母 m = 光心到最远像素距离），径向 f 多项式 +
            // 切向项给出源位移；逐平面系数即逐通道（含横向色差）。
            let ocx = center[0] * (width as f32 - 1.0);
            let ocy = center[1] * (height as f32 - 1.0);
            let mut m = 0.0f32;
            for (px, py) in [
                (0.0, 0.0),
                (width as f32 - 1.0, 0.0),
                (0.0, height as f32 - 1.0),
                (width as f32 - 1.0, height as f32 - 1.0),
            ] {
                let d = libm::sqrtf((px - ocx) * (px - ocx) + (py - ocy) * (py - ocy));
                m = m.max(d);
            }
            if m <= 0.0 {
                return (ox, oy);
            }
            let set = if sets.len() == 3 {
                &sets[channel.min(2)]
            } else {
                &sets[0]
            };
            let dx = (ox - ocx) / m;
            let dy = (oy - ocy) / m;
            let r2 = dx * dx + dy * dy;
            let f = set.kr[0] + set.kr[1] * r2 + set.kr[2] * r2 * r2 + set.kr[3] * r2 * r2 * r2;
            let dtx = set.kt[0] * (2.0 * dx * dy) + set.kt[1] * (r2 + 2.0 * dx * dx);
            let dty = set.kt[1] * (2.0 * dx * dy) + set.kt[0] * (r2 + 2.0 * dy * dy);
            (ocx + m * (f * dx + dtx), ocy + m * (f * dy + dty))
        }
    }
}

/// 自动裁剪 scale（语义对齐 render `geo_auto_crop_scale`：只放大
/// ≥ 1、让 warp 后的采样不越出源图）。算法：在画面周界与对角射线
/// 上密采样输出点，二分找最小 scale 使全部通道的源坐标落回源图
/// 内（含 0.5px 采样边距）；恒等 warp ⇒ 恰 1.0。上限 4.0：病态
/// 参数到顶仍越界时按上限执行、采样端 reflect 兜底（不许无限找）。
pub fn auto_crop_scale(model: &WarpModel, width: usize, height: usize) -> f32 {
    let in_bounds = |scale: f32| -> bool {
        let (w, h) = (width as f32 - 1.0, height as f32 - 1.0);
        // 采样集：周界（步长 ≤ 8px）+ 四条对角线 + 中心十字。
        let mut samples: Vec<(f32, f32)> = Vec::new();
        let step = 8.0f32;
        let mut t = 0.0;
        while t <= w {
            samples.push((t, 0.0));
            samples.push((t, h));
            t += step;
        }
        let mut t = 0.0;
        while t <= h {
            samples.push((0.0, t));
            samples.push((w, t));
            t += step;
        }
        for i in 0..=16 {
            let f = i as f32 / 16.0;
            samples.push((w * f, h * f));
            samples.push((w * (1.0 - f), h * f));
        }
        samples.iter().all(|&(x, y)| {
            (0..3).all(|channel| {
                let (sx, sy) = warp_source_coord(model, channel, x, y, width, height, scale);
                sx >= -0.5 && sx <= w + 0.5 && sy >= -0.5 && sy <= h + 0.5
            })
        })
    };
    if in_bounds(1.0) {
        return 1.0;
    }
    let (mut lo, mut hi) = (1.0f32, 4.0f32);
    if !in_bounds(hi) {
        return hi;
    }
    for _ in 0..32 {
        let mid = (lo + hi) / 2.0;
        if in_bounds(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lens_db::LensEntry;

    fn vig_grid() -> Vec<VignettingEntry> {
        // 规则网格：focal {18, 35} × aperture {4, 8} × distance {1, 10}，
        // k1 = focal/100 + log2(ap)/10 + (1/dist)，其余 0——期望值可手算。
        let mut out = Vec::new();
        for &f in &[18.0f32, 35.0] {
            for &ap in &[4.0f32, 8.0] {
                for &d in &[1.0f32, 10.0] {
                    out.push(VignettingEntry {
                        focal: f,
                        aperture: ap,
                        distance: d,
                        model: "pa".to_string(),
                        k1: f / 100.0 + libm::log2f(ap) / 10.0 + 1.0 / d,
                        k2: 0.0,
                        k3: 0.0,
                    });
                }
            }
        }
        out
    }

    #[test]
    fn vignetting_trilinear_midpoint_matches_hand_computation() {
        let entries = vig_grid();
        // 中点：focal 26.5、aperture 2^2.5、distance 使 1/d = 0.55。
        let got =
            interpolate_vignetting(&entries, 26.5, Some(libm::powf(2.0, 2.5)), Some(1.0 / 0.55))
                .unwrap();
        let expected = 26.5 / 100.0 + 2.5 / 10.0 + 0.55;
        assert!(
            (got.pa[0] - expected).abs() < 1e-5,
            "三维插值中点应 {expected}，实得 {}",
            got.pa[0]
        );
        assert!(!got.aperture_assumed && !got.distance_assumed);
    }

    #[test]
    fn vignetting_missing_aperture_and_distance_set_assumed_flags() {
        let entries = vig_grid();
        let got = interpolate_vignetting(&entries, 18.0, None, None).unwrap();
        assert!(got.aperture_assumed && got.distance_assumed);
        // 缺光圈取最小档 4、缺距离取最大档 10 ⇒ k1 = 0.18 + 0.2 + 0.1。
        assert!((got.pa[0] - 0.48).abs() < 1e-5, "实得 {}", got.pa[0]);
    }

    #[test]
    fn distortion_interpolates_and_clamps_and_skips_non_ptlens() {
        let entries = vec![
            DistortionEntry {
                focal: 18.0,
                model: "ptlens".into(),
                a: 0.02,
                b: -0.04,
                c: 0.01,
            },
            DistortionEntry {
                focal: 35.0,
                model: "ptlens".into(),
                a: 0.0,
                b: 0.0,
                c: 0.0,
            },
        ];
        let mid = interpolate_distortion(&entries, 26.5);
        assert_eq!(mid.ptlens, Some([0.01, -0.02, 0.005]));
        // 范围外钳到端档。
        let clamped = interpolate_distortion(&entries, 100.0);
        assert_eq!(clamped.ptlens, Some([0.0, 0.0, 0.0]));
        // 只有 poly3 ⇒ 跳过并留痕，绝不误读成 ptlens。
        let poly = vec![DistortionEntry {
            focal: 24.0,
            model: "poly3".into(),
            a: 1.0,
            b: 2.0,
            c: 3.0,
        }];
        let skipped = interpolate_distortion(&poly, 24.0);
        assert!(skipped.ptlens.is_none() && skipped.skipped_non_ptlens);
    }

    #[test]
    fn crop_conversion_scales_pa_coefficients_by_powers() {
        // s=2：k 按 4/16/64 缩放（同一 V 在像面 r 上成立的代数恒等）。
        assert_eq!(pa_effective([0.1, 0.2, 0.3], 2.0), [0.4, 3.2, 19.2]);
        assert_eq!(pa_effective([0.1, 0.2, 0.3], 1.0), [0.1, 0.2, 0.3]);
    }

    #[test]
    fn identity_warp_has_unit_crop_scale_and_identity_coords() {
        let model = WarpModel::PtlensTca {
            ptlens: [0.0, 0.0, 0.0],
            tca: None,
            crop_ratio: 1.0,
        };
        assert_eq!(auto_crop_scale(&model, 101, 67), 1.0);
        let (sx, sy) = warp_source_coord(&model, 0, 100.0, 66.0, 101, 67, 1.0);
        assert!((sx - 100.0).abs() < 1e-4 && (sy - 66.0).abs() < 1e-4);
    }

    #[test]
    fn barrel_distortion_needs_crop_above_one() {
        // 强桶形（a 负大）：输出角点采样会越出源图 ⇒ scale > 1。
        let model = WarpModel::PtlensTca {
            ptlens: [-0.2, 0.0, 0.0],
            tca: None,
            crop_ratio: 1.0,
        };
        let scale = auto_crop_scale(&model, 201, 201);
        assert!(scale > 1.0 && scale <= 4.0, "scale={scale}");
    }

    #[test]
    fn resolve_requires_focal() {
        let entry = LensEntry {
            maker: "M".into(),
            model: "M 24mm".into(),
            aliases: vec![],
            mount: None,
            cropfactor: Some(1.0),
            center: None,
            distortion: vec![],
            tca: vec![],
            vignetting: vec![],
        };
        let mut exif = LensExif::default();
        assert!(resolve_lensfun(&entry, &exif).is_none());
        exif.focal_mm = Some(24.0);
        assert!(resolve_lensfun(&entry, &exif).is_some());
    }
}
