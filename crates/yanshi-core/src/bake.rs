//! bake_layer 折叠语义与仿射校验（专家评审定版 + 第二轮 12 条）。
//!
//! 本模块只放**纯函数**：仿射矩阵、几何门禁、Fold Horizon 计算、
//! 缺失 Blob / 哈希不一致 / 跨机 Undo 的结构化错误构造。
//! 渲染（采样）与工具（回执）分别在 yanshi-render 与 yanshi-server。

use crate::atom::{Atom, AtomKind};
use crate::error::{ErrorCode, ErrorContext, Result, YanshiError};
use crate::ids::{LayerId, Seq};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// 仿射矩阵 `[a, b, c, d, e, f]`，与 [`crate::state::Transform`] 同约定：
/// `x' = a*x + c*y + e`，`y' = b*x + d*y + f`，枢轴固定为画布原点 (0, 0)。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Affine2D(pub [f64; 6]);

impl Default for Affine2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2D {
    /// 单位矩阵。
    pub const IDENTITY: Self = Self([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// 是否为单位矩阵（容差 0，避免 identity 误判为变换）。
    pub fn is_identity(&self) -> bool {
        self.0 == Self::IDENTITY.0
    }

    /// 应用到一点。
    pub fn apply_point(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    /// 复合：先 `inner` 后 `outer`，即 `outer * inner`。
    pub fn compose(outer: Affine2D, inner: Affine2D) -> Affine2D {
        let [a2, b2, c2, d2, e2, f2] = outer.0;
        let [a1, b1, c1, d1, e1, f1] = inner.0;
        Affine2D([
            a2 * a1 + c2 * b1,
            b2 * a1 + d2 * b1,
            a2 * c1 + c2 * d1,
            b2 * c1 + d2 * d1,
            a2 * e1 + c2 * f1 + e2,
            b2 * e1 + d2 * f1 + f2,
        ])
    }

    /// 逆矩阵；奇异时返回 `None`（调用方必须报错，绝不静默退化）。
    pub fn inverse(&self) -> Option<Affine2D> {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        if !det.is_finite() || det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Affine2D([
            d * inv_det,
            -b * inv_det,
            -c * inv_det,
            a * inv_det,
            (c * f - d * e) * inv_det,
            (b * e - a * f) * inv_det,
        ]))
    }
}

/// 包内有效 Bake 的一行引用（导入策略检查与重放自愈共用；不用 5 元组，
/// 避免 clippy type_complexity）。
#[derive(Debug, Clone)]
pub struct BakeBlobRef {
    /// 图层 id。
    pub layer_id: String,
    /// 该 Bake 原子的 seq（审计与指引用）。
    pub bake_seq: u64,
    /// 水位：该 Bake 覆盖到的最大原子 seq。
    pub up_to_seq: u64,
    /// 基底 Blob 哈希（`sha256:…` 文本）。
    pub blob_hash: String,
    /// 该 Bake 是否把图层掩膜压进了基底（重放须同口径）。
    pub flatten_mask: bool,
}

/// 几何门禁：旋转只接受 90° 倍数；非均匀缩放默认关闭。
///
/// 剪切（列不正交）直接拒绝：BakedBase 的仿射只覆盖旋转 + 缩放 + 平移，
/// 剪切不在第二轮 12 条的口径内，静默接受会改变像素语义。
pub fn validate_affine(matrix: Affine2D, allow_non_uniform: bool) -> Result<()> {
    let [a, b, c, d, _, _] = matrix.0;
    if !matrix.0.iter().all(|v| v.is_finite()) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("仿射矩阵含非有限值"),
        ));
    }
    let dot = a * c + b * d;
    let scale_x = (a * a + b * b).sqrt();
    let scale_y = (c * c + d * d).sqrt();
    if scale_x <= 0.0 || scale_y <= 0.0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("仿射矩阵退化：某一轴缩放为 0"),
        ));
    }
    let ortho_tol = 1e-9 * scale_x * scale_y;
    if dot.abs() > ortho_tol.max(1e-12) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("BakedBase 仿射不支持剪切：请先 bake_layer 固化后再用旋转/缩放"),
        ));
    }
    if !allow_non_uniform && (scale_x - scale_y).abs() > 1e-9 * scale_x.max(scale_y) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("非均匀缩放默认关闭：请显式传 allow_non_uniform_scale=true 确认"),
        ));
    }
    let angle = b.atan2(a);
    let quarter = std::f64::consts::FRAC_PI_2;
    let turns = angle / quarter;
    if (turns - turns.round()).abs() > 1e-6 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("BakedBase 旋转只接受 90° 倍数"),
        ));
    }
    Ok(())
}

/// 层内图元内容修改类原子：受 Fold Horizon 剪枝。
pub fn is_bake_content_kind(kind: AtomKind) -> bool {
    matches!(
        kind,
        AtomKind::DrawStroke
            | AtomKind::DrawShape
            | AtomKind::DrawText
            | AtomKind::Fill
            | AtomKind::Erase
            | AtomKind::Retouch
            | AtomKind::Liquify
            | AtomKind::Supersede
            | AtomKind::CreateObject
            | AtomKind::ImportImage
            | AtomKind::Move
            | AtomKind::Transform
            | AtomKind::SetProperty
            | AtomKind::Tombstone
            | AtomKind::ReorderObjects
    )
}

/// 图层生命周期与外壳属性类原子：永不被 Horizon 剪枝。
pub fn is_bake_shell_kind(kind: AtomKind) -> bool {
    matches!(
        kind,
        AtomKind::CreateLayer
            | AtomKind::ReorderLayers
            | AtomKind::SetBackground
            | AtomKind::CreateSelection
            | AtomKind::UpdateSelection
            | AtomKind::CreateMask
            | AtomKind::UpdateMask
            | AtomKind::DeleteMask
    )
}

/// 从一段有效原子算 Fold Horizon：`layer_id -> max up_to_seq`。
///
/// 只统计**有效**（不在 `suppressed` 内）的 BakeLayer 原子；被 revert 的
/// BakeLayer 不产生屏障，这是 Undo 后旧原子恢复的机制保证。
pub fn compute_bake_horizon(
    atoms: &[Atom],
    suppressed: &BTreeSet<String>,
) -> BTreeMap<LayerId, Seq> {
    let mut horizon: BTreeMap<LayerId, Seq> = BTreeMap::new();
    for atom in atoms {
        if atom.kind != AtomKind::BakeLayer || suppressed.contains(&atom.id) {
            continue;
        }
        let Some(layer_id) = atom.payload.get("layer_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(up_to_seq) = atom.payload.get("up_to_seq").and_then(|v| v.as_u64()) else {
            continue;
        };
        let entry = horizon.entry(layer_id.to_owned()).or_insert(0);
        *entry = (*entry).max(up_to_seq);
    }
    horizon
}

/// 目标层判定：优先用当前状态的对象归属，其次用原子载荷的 `layer_id`。
pub fn bake_target_layer(
    atom: &Atom,
    object_layer: impl Fn(&str) -> Option<LayerId>,
) -> Option<LayerId> {
    if let Some(object_id) = atom.payload.get("object_id").and_then(|v| v.as_str()) {
        if let Some(layer_id) = object_layer(object_id) {
            return Some(layer_id);
        }
    }
    atom.payload
        .get("layer_id")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
}

/// 缺失 Baked Blob 的严格错误（常规折叠唯一出口，绝不自动重放）。
pub fn missing_baked_blob_error(layer_id: &str, blob_hash: &str, bake_seq: Seq) -> YanshiError {
    YanshiError::new(
        ErrorCode::MissingBakedBlob,
        ErrorContext::detail(format!(
            "图层 {layer_id} 的压平基底缺失：blob {blob_hash} 不在存储中（bake_seq={bake_seq}），请调 recover_baked_blobs 自愈或经 put_blob 上传后重试"
        )),
    )
    .with_layer(layer_id.to_owned())
    .with_blob(blob_hash.to_owned())
    .with_extra("bake_seq", serde_json::json!(bake_seq))
    .with_extra("replay_recoverable", serde_json::json!(true))
}

/// 重放哈希不一致（缺省 strict）。
pub fn hash_mismatch_error(bake_seq: Seq, expected_hash: &str, actual_hash: &str) -> YanshiError {
    YanshiError::new(
        ErrorCode::PreconditionFailed,
        ErrorContext::detail(format!(
            "重放自愈哈希不一致：bake_seq={bake_seq} 期望 {expected_hash}，重建得到 {actual_hash}，拒绝写入（显式 on_hash_mismatch=append_rebake 可追加新 Bake 原子）"
        )),
    )
    .with_extra("bake_seq", serde_json::json!(bake_seq))
    .with_extra("expected_hash", serde_json::json!(expected_hash))
    .with_extra("actual_hash", serde_json::json!(actual_hash))
}

/// 跨机 Undo 缺历史 Blob 的原子级拦截错误（状态不回退）。
pub fn undo_missing_historical_blob_error(
    layer_id: &str,
    undone_bake_seq: Seq,
    required_bake_seq: Seq,
    missing_blob_hash: &str,
) -> YanshiError {
    YanshiError::new(
        ErrorCode::MissingBakedBlob,
        ErrorContext::detail(format!(
            "Undo 被拦截：图层 {layer_id} 需要历史压平基底 {missing_blob_hash}（required_bake_seq={required_bake_seq}），请调 recover_baked_blobs(layer_id, target_seq) 从本地原子日志重放，或经 put_blob 上传 Blob 后重试 undo"
        )),
    )
    .with_layer(layer_id.to_owned())
    .with_blob(missing_blob_hash.to_owned())
    .with_extra("error_code", serde_json::json!("MISSING_HISTORICAL_BLOB_ON_UNDO"))
    .with_extra("undone_bake_seq", serde_json::json!(undone_bake_seq))
    .with_extra("required_bake_seq", serde_json::json!(required_bake_seq))
    .with_extra("replay_recoverable", serde_json::json!(true))
    .with_extra(
        "recovery_action",
        serde_json::json!(format!(
            "recover_baked_blobs(layer_id={layer_id}, target_seq={required_bake_seq})"
        )),
    )
}

/// bake_layer 门禁参数（第二轮 12 条第 12 条：取代原 brief 的 reject_on_lossy）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BakeLayerParams {
    /// 允许把文本 / 矢量栅格化（默认 false：默认直接 is_error 拦截）。
    #[serde(default)]
    pub allow_rasterize_editable: bool,
    /// 允许丢弃越界像素（默认 false：默认直接 is_error 拦截）。
    #[serde(default)]
    pub allow_clip_off_canvas: bool,
    /// 把图层级独立掩膜压进基底（默认 false：掩膜保留独立存在）。
    #[serde(default)]
    pub flatten_mask: bool,
    /// 允许非均匀缩放（仅用于压平后仿射门禁；bake 本身不接受缩放参数）。
    #[serde(default)]
    pub allow_non_uniform_scale: bool,
}

/// bake 回执警告级别（第二轮 12 条第 12 条）。
pub mod warning_level {
    /// 无损信号：已压平、无操作。
    pub const INFO: &str = "INFO";
    /// 可编辑性有损：文本 / 矢量 / 掩膜被栅格化。
    pub const LOSSY_EDITABILITY: &str = "LOSSY_EDITABILITY";
    /// 像素有损：越界像素被裁剪。
    pub const LOSSY_PIXELS: &str = "LOSSY_PIXELS";
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn atom(kind: AtomKind, id: &str, seq: Seq, payload: serde_json::Value) -> Atom {
        let mut a = Atom::new(kind, "actor".to_owned(), "session".to_owned(), payload);
        a.id = id.to_owned();
        a.seq = seq;
        a
    }

    #[test]
    fn affine_compose_identity_and_translate() {
        let t = Affine2D([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);
        assert_eq!(Affine2D::compose(t, Affine2D::IDENTITY), t);
        assert_eq!(Affine2D::compose(Affine2D::IDENTITY, t), t);
        assert_eq!(t.apply_point(1.0, 2.0), (11.0, 22.0));
        let rot90 = Affine2D([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]);
        assert_eq!(rot90.apply_point(1.0, 0.0), (0.0, 1.0));
        assert!(rot90.inverse().is_some());
    }

    #[test]
    fn affine_gate_rejects_shear_and_45_degrees() {
        assert!(validate_affine(Affine2D::IDENTITY, false).is_ok());
        assert!(validate_affine(Affine2D([1.0, 0.5, 0.0, 1.0, 0.0, 0.0]), false).is_err());
        let deg45 = std::f64::consts::FRAC_PI_4;
        let m = Affine2D([
            deg45.cos(),
            deg45.sin(),
            -deg45.sin(),
            deg45.cos(),
            0.0,
            0.0,
        ]);
        assert!(validate_affine(m, false).is_err());
        assert!(validate_affine(Affine2D([2.0, 0.0, 0.0, 1.0, 0.0, 0.0]), false).is_err());
        assert!(validate_affine(Affine2D([2.0, 0.0, 0.0, 1.0, 0.0, 0.0]), true).is_ok());
    }

    #[test]
    fn horizon_ignores_suppressed_bake() {
        let atoms = vec![
            atom(
                AtomKind::BakeLayer,
                "a_bake",
                10,
                json!({"layer_id": "L1", "up_to_seq": 9, "blob_hash": "sha256:00"}),
            ),
            atom(
                AtomKind::BakeLayer,
                "a_bake2",
                20,
                json!({"layer_id": "L1", "up_to_seq": 19, "blob_hash": "sha256:11"}),
            ),
        ];
        let suppressed: BTreeSet<String> = ["a_bake2".to_owned()].into_iter().collect();
        let horizon = compute_bake_horizon(&atoms, &suppressed);
        assert_eq!(horizon.get("L1"), Some(&9));
        let horizon2 = compute_bake_horizon(&atoms, &BTreeSet::new());
        assert_eq!(horizon2.get("L1"), Some(&19));
    }

    #[test]
    fn missing_blob_error_carries_replay_flag() {
        let e = missing_baked_blob_error("L1", "sha256:ab", 9);
        assert_eq!(e.code, ErrorCode::MissingBakedBlob);
        assert_eq!(
            e.context.extra.get("replay_recoverable"),
            Some(&serde_json::json!(true))
        );
    }
}
