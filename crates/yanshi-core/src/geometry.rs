//! 几何（路线三）的纯函数：仿射合成与选区形状变换。
//!
//! 本模块不碰 blob 存储、不做渲染、不读磁盘 —— 折叠层是纯状态函数，
//! 位图蒙版的重采样由工具层在提交前算好、随原子载荷提交（见工具层说明），
//! 折叠层只按载荷应用（替换 patch），撤销靠排除几何原子恢复原 patch。

use crate::atom::{AtomKind, BlobHash};
use crate::state::{MaskPatchMeta, MaskPatchOp, Transform};
use serde_json::Value;

/// 把带枢轴的变换折成纯 2×3 矩阵：`T(pivot) · M · T(-pivot)`。
///
/// 与渲染层 `flatten` 同一约定 ⇒ 折叠期合成与渲染期逐层施加逐位一致。
pub fn flatten_transform(transform: &Transform) -> [f64; 6] {
    let m = transform.matrix;
    let (px, py) = (transform.pivot[0], transform.pivot[1]);
    [
        m[0],
        m[1],
        m[2],
        m[3],
        m[4] + px - (m[0] * px + m[2] * py),
        m[5] + py - (m[1] * px + m[3] * py),
    ]
}

/// 2×3 仿射相乘：`outer ∘ inner`（先 inner 后 outer），与渲染层约定一致。
pub fn compose_flat(outer: &[f64; 6], inner: &[f64; 6]) -> [f64; 6] {
    [
        outer[0] * inner[0] + outer[2] * inner[1],
        outer[1] * inner[0] + outer[3] * inner[1],
        outer[0] * inner[2] + outer[2] * inner[3],
        outer[1] * inner[2] + outer[3] * inner[3],
        outer[0] * inner[4] + outer[2] * inner[5] + outer[4],
        outer[1] * inner[4] + outer[3] * inner[5] + outer[5],
    ]
}

/// 纯矩阵施加到一点。
pub fn apply_flat(flat: &[f64; 6], x: f64, y: f64) -> (f64, f64) {
    (
        flat[0] * x + flat[2] * y + flat[4],
        flat[1] * x + flat[3] * y + flat[5],
    )
}

/// 纯矩阵是否为恒等矩阵（精确比较：矩阵由工具层构造，不含测量值）。
pub fn is_identity_flat(flat: &[f64; 6]) -> bool {
    flat == &[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

/// 纯矩阵是否为纯平移；是则返回 `(dx, dy)`。
pub fn translation_of_flat(flat: &[f64; 6]) -> Option<(f64, f64)> {
    if flat[0] == 1.0 && flat[1] == 0.0 && flat[2] == 0.0 && flat[3] == 1.0 {
        Some((flat[4], flat[5]))
    } else {
        None
    }
}

/// 从几何原子载荷的 `affine: {matrix, pivot}` 解析变换；缺失或非法时返回单位矩阵。
///
/// 折叠层的 precondition 已把关（非法矩阵直接拒原子），这里的回退只防手写日志。
pub fn transform_from_affine_payload(payload: &Value) -> Transform {
    let Some(value) = payload.get("affine") else {
        return Transform::IDENTITY;
    };
    let mut matrix = Transform::IDENTITY.matrix;
    if let Some(list) = value.get("matrix").and_then(Value::as_array) {
        if list.len() == 6 {
            for (slot, item) in matrix.iter_mut().zip(list) {
                *slot = item.as_f64().unwrap_or(0.0);
            }
        }
    }
    let mut pivot = [0.0f64; 2];
    if let Some(list) = value.get("pivot").and_then(Value::as_array) {
        if list.len() == 2 {
            pivot = [
                list[0].as_f64().unwrap_or(0.0),
                list[1].as_f64().unwrap_or(0.0),
            ];
        }
    }
    Transform { matrix, pivot }
}

/// 几何原子是否移动内容（改界不动内容，其余五种都动）。
pub fn geometry_moves_content(kind: AtomKind) -> bool {
    !matches!(kind, AtomKind::ResizeCanvasBounds)
}

/// 解析几何原子载荷里某蒙版的重采样 patch 列表。
pub fn parse_mask_patches(value: &Value) -> Result<Vec<MaskPatchMeta>, String> {
    let Some(list) = value.as_array() else {
        return Err("mask_patches 必须是 patch 列表".to_owned());
    };
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        out.push(parse_mask_patch(item)?);
    }
    Ok(out)
}

/// 解析单个蒙版 patch（与 `UpdateMask` 同一形状：bbox/target_u8/op/blob）。
pub fn parse_mask_patch(value: &Value) -> Result<MaskPatchMeta, String> {
    let bbox_value = value
        .get("bbox")
        .and_then(Value::as_array)
        .filter(|list| list.len() == 4)
        .ok_or_else(|| "mask patch 缺少 bbox（[x, y, w, h]）".to_owned())?;
    let mut bbox = [0i32; 4];
    for (slot, item) in bbox.iter_mut().zip(bbox_value) {
        *slot = item
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| "mask patch 的 bbox 必须是整数".to_owned())?;
    }
    let target_u8 = value
        .get("target_u8")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or_else(|| "mask patch 缺少 target_u8（0..255）".to_owned())?;
    let op: MaskPatchOp =
        serde_json::from_value(value.get("op").cloned().unwrap_or(Value::Null))
            .map_err(|_| "mask patch 的 op 非法（lerp_deposit / replace_all）".to_owned())?;
    let blob: BlobHash = value
        .get("blob")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "mask patch 缺少合法 blob 哈希".to_owned())?;
    Ok(MaskPatchMeta {
        bbox,
        target_u8,
        op,
        blob,
    })
}

fn map_point_in_array(point: &mut Value, flat: &[f64; 6]) {
    if let Some(pair) = point.as_array_mut() {
        if pair.len() == 2 {
            if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                let (nx, ny) = apply_flat(flat, x, y);
                pair[0] = Value::from(nx);
                pair[1] = Value::from(ny);
            }
        }
    }
}

fn map_bbox_object(bbox: &mut Value, flat: &[f64; 6]) {
    let (Some(x), Some(y), Some(w), Some(h)) = (
        bbox.get("x").and_then(Value::as_f64),
        bbox.get("y").and_then(Value::as_f64),
        bbox.get("w").and_then(Value::as_f64),
        bbox.get("h").and_then(Value::as_f64),
    ) else {
        return;
    };
    let (x0, y0) = apply_flat(flat, x, y);
    let (x1, y1) = apply_flat(flat, x + w, y + h);
    bbox["x"] = Value::from(x0.min(x1));
    bbox["y"] = Value::from(y0.min(y1));
    bbox["w"] = Value::from((x1 - x0).abs());
    bbox["h"] = Value::from((y1 - y0).abs());
}

/// 对选区/蒙版的 `shape` JSON 施加几何仿射。
///
/// 覆盖渲染层 `SelectionGeom` 的全部几何形：
/// rect/ellipse（`{x,y,w,h}` 或 `{bbox}`）、polygon/lasso（`points`）、
/// linear_gradient（`start`/`end`）、radial_gradient（`bbox`）、
/// path 内联 `subpaths`（`start` 与各段 `to`/`c1`/`c2`）。
/// path 引用（`kind: "path"` 且只有 `object_id`）不动 —— 它跟随被引用对象。
/// 未知形原样保留（折叠层不猜形状语义；工具层不为未知形承诺精确性）。
pub fn transform_shape(shape: &mut Value, flat: &[f64; 6]) {
    let Some(obj) = shape.as_object_mut() else {
        return;
    };
    // path 引用：只含 object_id、无内联 subpaths ⇒ 跟随对象，不动。
    let kind = obj
        .get("kind")
        .or_else(|| obj.get("type"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if kind == "path" && obj.get("object_id").is_some() && obj.get("subpaths").is_none() {
        return;
    }
    let had_bbox = obj.contains_key("bbox");
    if let Some(bbox) = obj.get_mut("bbox") {
        map_bbox_object(bbox, flat);
    }
    // `{x,y,w,h}` 与 `bbox` 二选一：有 bbox 的形不重复映射 x/y/w/h。
    if !had_bbox
        && obj.contains_key("x")
        && obj.contains_key("y")
        && obj.contains_key("w")
        && obj.contains_key("h")
    {
        let (Some(x), Some(y), Some(w), Some(h)) = (
            obj.get("x").and_then(Value::as_f64),
            obj.get("y").and_then(Value::as_f64),
            obj.get("w").and_then(Value::as_f64),
            obj.get("h").and_then(Value::as_f64),
        ) else {
            return;
        };
        let (x0, y0) = apply_flat(flat, x, y);
        let (x1, y1) = apply_flat(flat, x + w, y + h);
        obj["x"] = Value::from(x0.min(x1));
        obj["y"] = Value::from(y0.min(y1));
        obj["w"] = Value::from((x1 - x0).abs());
        obj["h"] = Value::from((y1 - y0).abs());
    }
    if let Some(points) = obj.get_mut("points").and_then(Value::as_array_mut) {
        for point in points.iter_mut() {
            map_point_in_array(point, flat);
        }
    }
    if let Some(start) = obj.get_mut("start") {
        map_point_in_array(start, flat);
    }
    if let Some(end) = obj.get_mut("end") {
        map_point_in_array(end, flat);
    }
    if let Some(subpaths) = obj.get_mut("subpaths").and_then(Value::as_array_mut) {
        for sub in subpaths.iter_mut() {
            if let Some(start) = sub.get_mut("start") {
                map_point_in_array(start, flat);
            }
            if let Some(segments) = sub.get_mut("segments").and_then(Value::as_array_mut) {
                for seg in segments.iter_mut() {
                    for key in ["to", "c1", "c2"] {
                        if let Some(point) = seg.get_mut(key) {
                            map_point_in_array(point, flat);
                        }
                    }
                }
            }
        }
    }
}
