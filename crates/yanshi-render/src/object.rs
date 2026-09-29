//! 对象 → 渲染图元解析（设计文档 4.2）。
//!
//! 解析是**只读**的：把 `Object.data` 中的类型专属字段翻译成内核可渲染的图元，
//! 不修改状态。无法渲染的对象（尚未实现的类型、缺少字体、未实现的位图编解码）
//! 返回 [`Primitive::Unsupported`]，由调用方计入告警而不是让整次渲染失败。

use crate::brush::{BrushSpec, StrokeGeometry};
use crate::color::LinearRgba;
use serde_json::Value;
use yanshi_core::{Bbox, BlobHash, Object, ObjectType};

/// 形状种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    /// 矩形。
    Rect,
    /// 椭圆。
    Ellipse,
    /// 多边形。
    Polygon,
}

/// 渲染图元。
#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    /// 笔触（笔刷 + 笔迹）。
    Stroke {
        /// 笔迹几何。
        geometry: StrokeGeometry,
        /// 笔刷参数。
        brush: BrushSpec,
    },
    /// 形状（填充 + 可选描边）。
    Shape {
        /// 形状种类。
        kind: ShapeKind,
        /// 形状外接盒（文档坐标）。
        bbox: Bbox,
        /// 多边形顶点（`kind = Polygon` 时使用）。
        points: Vec<(f64, f64)>,
        /// 填充色（直通线性）。
        color: LinearRgba,
        /// 描边宽度（像素，0 表示不描边）。
        stroke_width: f64,
        /// 描边颜色。
        stroke_color: Option<LinearRgba>,
    },
    /// 调整对象：作用于同图层中位于其下方的内容。
    Adjustment {
        /// `adjustment_type`。
        kind: String,
        /// 参数。
        params: Value,
    },
    /// 滤镜对象。
    Filter {
        /// `filter_name`。
        name: String,
        /// 参数。
        params: Value,
    },
    /// 位图补丁（AI 语义工具输出、`import_image`）。
    RasterPatch {
        /// blob 引用。
        blob: BlobHash,
        /// 位图宽。
        width: u32,
        /// 位图高。
        height: u32,
        /// 放置位置（文档坐标左上角）。
        offset: (f64, f64),
        /// MIME 类型（内核目前只支持 `image/x-yanshi-raw`）。
        mime_type: String,
    },
    /// 修图（clone_stamp）：从**同一图层已绘制内容**按偏移采样后盖回。
    ///
    /// 指向历史状态的 `source_state_version` 采样尚未实现（设计 Phase 3 后续），
    /// 当前语义等价于「源 = 应用本对象之前的图层内容」，即经典仿制图章。
    Retouch {
        /// 修图类型（目前仅 `clone_stamp`）。
        kind: String,
        /// 采样点列。
        points: Vec<(f64, f64)>,
        /// 采样偏移（源 = 目标 + offset）。
        offset: (f64, f64),
        /// 笔刷大小。
        size: f64,
        /// 硬度（0=软边，1=硬边）。
        hardness: f64,
        /// 不透明度。
        opacity: f64,
        /// 抖动。
        jitter: f64,
    },
    /// 无法渲染。
    Unsupported {
        /// 原因（用于告警与可观测性）。
        reason: String,
    },
}

/// 解析对象的渲染图元。
pub fn parse_object(object: &Object) -> Primitive {
    match object.object_type {
        ObjectType::Stroke => parse_stroke(&object.data),
        ObjectType::Shape => parse_shape(&object.data),
        ObjectType::Adjustment => {
            let kind = object
                .data
                .get("adjustment_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let params = object.data.get("params").cloned().unwrap_or(Value::Null);
            if kind.is_empty() {
                Primitive::Unsupported {
                    reason: "adjustment 缺少 adjustment_type".to_owned(),
                }
            } else {
                Primitive::Adjustment { kind, params }
            }
        }
        ObjectType::Filter => {
            let name = object
                .data
                .get("filter_name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let params = object.data.get("params").cloned().unwrap_or(Value::Null);
            if name.is_empty() {
                Primitive::Unsupported {
                    reason: "filter 缺少 filter_name".to_owned(),
                }
            } else {
                Primitive::Filter { name, params }
            }
        }
        ObjectType::RasterPatch => parse_raster_patch(&object.data),
        ObjectType::Text => Primitive::Unsupported {
            reason: "text 光栅化需要内嵌字体子集（18 章，尚未实现）".to_owned(),
        },
        ObjectType::Retouch => {
            let kind = object
                .data
                .get("retouch_type")
                .and_then(Value::as_str)
                .unwrap_or("clone_stamp")
                .to_owned();
            let points: Vec<(f64, f64)> = object
                .data
                .get("points")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            let pair = value.as_array()?;
                            Some((pair.first()?.as_f64()?, pair.get(1)?.as_f64()?))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if points.is_empty() {
                Primitive::Unsupported {
                    reason: "retouch 缺少 points".to_owned(),
                }
            } else {
                let offset = object
                    .data
                    .get("source_offset")
                    .and_then(Value::as_array)
                    .map(|pair| {
                        (
                            pair.first().and_then(Value::as_f64).unwrap_or(0.0),
                            pair.get(1).and_then(Value::as_f64).unwrap_or(0.0),
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                Primitive::Retouch {
                    kind,
                    points,
                    offset,
                    size: object
                        .data
                        .get("size")
                        .and_then(Value::as_f64)
                        .unwrap_or(24.0),
                    hardness: object
                        .data
                        .get("hardness")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.6),
                    opacity: object
                        .data
                        .get("opacity")
                        .and_then(Value::as_f64)
                        .unwrap_or(1.0),
                    jitter: object
                        .data
                        .get("jitter")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.0),
                }
            }
        }
        ObjectType::Liquify => Primitive::Unsupported {
            reason: "liquify 求解器属 Phase 0/3 验证项".to_owned(),
        },
        ObjectType::Instance | ObjectType::Group => Primitive::Unsupported {
            reason: "实例/组引用解析属 Phase 5（9 章）".to_owned(),
        },
    }
}

fn parse_stroke(data: &Value) -> Primitive {
    let Some(geometry) = StrokeGeometry::from_value(data) else {
        return Primitive::Unsupported {
            reason: "stroke 缺少 points".to_owned(),
        };
    };
    Primitive::Stroke {
        geometry,
        brush: BrushSpec::from_value(data),
    }
}

fn parse_shape(data: &Value) -> Primitive {
    // 形状数据形如：
    //   {"geometry": {"kind": "rect", "bbox": {"x":0,"y":0,"w":10,"h":10}},
    //    "color": [...], "stroke_width": 1.0, "stroke_color": [...]}
    let geometry = data.get("geometry").cloned().unwrap_or(Value::Null);
    let kind = match geometry.get("kind").and_then(Value::as_str) {
        Some("ellipse") => ShapeKind::Ellipse,
        Some("polygon") | Some("path") => ShapeKind::Polygon,
        _ => ShapeKind::Rect,
    };
    let bbox = geometry
        .get("bbox")
        .and_then(Bbox::from_value)
        .or_else(|| data.get("bbox").and_then(Bbox::from_value))
        .unwrap_or_else(|| Bbox::new(0.0, 0.0, 0.0, 0.0));
    let mut points = Vec::new();
    if let Some(array) = geometry.get("points").and_then(Value::as_array) {
        for item in array {
            if let Some(pair) = item.as_array() {
                if pair.len() >= 2 {
                    if let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) {
                        points.push((x, y));
                    }
                }
            }
        }
    }
    let color = data
        .get("color")
        .map(parse_color)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let stroke_width = data
        .get("stroke_width")
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .max(0.0);
    let stroke_color = data.get("stroke_color").map(parse_color);
    Primitive::Shape {
        kind,
        bbox,
        points,
        color,
        stroke_width,
        stroke_color,
    }
}

fn parse_raster_patch(data: &Value) -> Primitive {
    let bitmap = data.get("bitmap").cloned().unwrap_or(Value::Null);
    let Some(hash) = bitmap.get("blob_hash").and_then(Value::as_str) else {
        return Primitive::Unsupported {
            reason: "raster_patch 缺少 bitmap.blob_hash".to_owned(),
        };
    };
    let Ok(blob) = hash.parse::<BlobHash>() else {
        return Primitive::Unsupported {
            reason: format!("非法 blob hash: {hash}"),
        };
    };
    let mime_type = bitmap
        .get("mime_type")
        .and_then(Value::as_str)
        .unwrap_or("image/x-yanshi-raw")
        .to_owned();
    let region = data.get("region").and_then(Bbox::from_value);
    let width = data
        .get("width")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .or_else(|| region.as_ref().map(|bbox| bbox.w.max(0.0) as u32))
        .unwrap_or(0);
    let height = data
        .get("height")
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .or_else(|| region.as_ref().map(|bbox| bbox.h.max(0.0) as u32))
        .unwrap_or(0);
    if width == 0 || height == 0 {
        return Primitive::Unsupported {
            reason: "raster_patch 缺少位图尺寸（width/height 或 region）".to_owned(),
        };
    }
    let offset = region.map(|bbox| (bbox.x, bbox.y)).unwrap_or((0.0, 0.0));
    Primitive::RasterPatch {
        blob,
        width,
        height,
        offset,
        mime_type,
    }
}

/// 解析颜色：支持 `[r, g, b, a]`（线性直通）与 `{"r": 0-255, ...}`（sRGB 字节）。
pub fn parse_color(value: &Value) -> LinearRgba {
    // 统一走 `color::parse_spec_color`（线性数组 / sRGB 字节数组 / 对象 / 十六进制）。
    crate::color::parse_spec_color(value).unwrap_or([0.0, 0.0, 0.0, 1.0])
}

/// 对象的效果包围盒（文档坐标）；无法判定时返回 `None`。
///
/// 该包围盒用于几何 dirty 传播（设计文档 6.6）：调用方取其并集作为失效区域。
pub fn object_bbox(object: &Object) -> Option<Bbox> {
    // 对象的仿射 `transform` 尚未进入内核（Phase 3 起按对象应用），
    // 因此这里返回的是对象**本地**包围盒。
    let local = match parse_object(object) {
        Primitive::Retouch {
            points,
            size,
            jitter,
            ..
        } => {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for (x, y) in &points {
                min_x = min_x.min(*x);
                min_y = min_y.min(*y);
                max_x = max_x.max(*x);
                max_y = max_y.max(*y);
            }
            if !min_x.is_finite() {
                return None;
            }
            let margin = size / 2.0 + jitter;
            Bbox::new(
                min_x - margin,
                min_y - margin,
                (max_x - min_x) + margin * 2.0,
                (max_y - min_y) + margin * 2.0,
            )
        }
        Primitive::Stroke { geometry, brush } => {
            let mut min_x = f64::INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for point in &geometry.points {
                min_x = min_x.min(point.x);
                min_y = min_y.min(point.y);
                max_x = max_x.max(point.x);
                max_y = max_y.max(point.y);
            }
            if !min_x.is_finite() {
                return None;
            }
            let margin = brush.size / 2.0 + brush.jitter;
            Bbox::new(
                min_x - margin,
                min_y - margin,
                (max_x - min_x) + margin * 2.0,
                (max_y - min_y) + margin * 2.0,
            )
        }
        Primitive::Shape {
            kind,
            bbox,
            points,
            stroke_width,
            ..
        } => {
            let base = if kind == ShapeKind::Polygon && !points.is_empty() {
                let min_x = points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
                let min_y = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                let max_x = points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
                let max_y = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                Bbox::new(min_x, min_y, max_x - min_x, max_y - min_y)
            } else {
                bbox
            };
            let margin = stroke_width / 2.0;
            Bbox::new(
                base.x - margin,
                base.y - margin,
                base.w + margin * 2.0,
                base.h + margin * 2.0,
            )
        }
        Primitive::RasterPatch {
            width,
            height,
            offset,
            ..
        } => Bbox::new(offset.0, offset.1, width as f64, height as f64),
        Primitive::Adjustment { .. } | Primitive::Filter { .. } => {
            // 调整/滤镜作用于整层，包围盒由调用方按图层计算。
            Bbox::new(0.0, 0.0, 0.0, 0.0)
        }
        Primitive::Unsupported { .. } => return None,
    };
    Some(local)
}

/// 图层的效果包围盒 = 其存活对象包围盒的并集（未知包围盒的对象使结果保守为整层）。
pub fn layer_bbox(state: &yanshi_core::DocumentState, layer_id: &str) -> Option<Bbox> {
    let mut union: Option<Bbox> = None;
    for object in state.objects_in_layer(layer_id) {
        match object_bbox(object) {
            Some(bbox) if bbox.w > 0.0 && bbox.h > 0.0 => {
                union = Some(match union {
                    Some(current) => current.union(&bbox),
                    None => bbox,
                });
            }
            Some(_) => {}
            None => {
                // 无法判定：保守返回整文档范围。
                return Some(Bbox::new(0.0, 0.0, state.width as f64, state.height as f64));
            }
        }
    }
    union
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use yanshi_core::{Object, ObjectType, Transform};

    fn object(object_type: ObjectType, data: Value) -> Object {
        Object {
            id: "obj_1".to_owned(),
            layer_id: "layer_1".to_owned(),
            object_type,
            z_index: 0,
            visible: true,
            locked: false,
            metadata: Value::Null,
            transform: Transform::IDENTITY,
            style: None,
            versions: vec!["a1".to_owned()],
            current_version: Some("a1".to_owned()),
            created_by: "a1".to_owned(),
            deleted_by: None,
            data,
            blobs: Vec::new(),
        }
    }

    #[test]
    fn stroke_primitive_parses_geometry_and_brush() {
        let object = object(
            ObjectType::Stroke,
            json!({"points": [[0.0, 0.0], [10.0, 0.0]], "size": 4.0, "color": [1.0, 0.0, 0.0, 1.0]}),
        );
        match parse_object(&object) {
            Primitive::Stroke { geometry, brush } => {
                assert_eq!(geometry.points.len(), 2);
                assert_eq!(brush.size, 4.0);
                assert_eq!(brush.color, [1.0, 0.0, 0.0, 1.0]);
            }
            other => panic!("期望 Stroke，得到 {other:?}"),
        }
        let bbox = object_bbox(&object).unwrap();
        assert_eq!(bbox, Bbox::new(-2.0, -2.0, 14.0, 4.0));
    }

    #[test]
    fn shape_primitive_parses_rect_ellipse_polygon() {
        let rect = object(
            ObjectType::Shape,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 1, "y": 2, "w": 3, "h": 4}}, "color": [0.0, 1.0, 0.0, 1.0]}),
        );
        match parse_object(&rect) {
            Primitive::Shape { kind, bbox, .. } => {
                assert_eq!(kind, ShapeKind::Rect);
                assert_eq!(bbox, Bbox::new(1.0, 2.0, 3.0, 4.0));
            }
            other => panic!("期望 Shape，得到 {other:?}"),
        }
        let polygon = object(
            ObjectType::Shape,
            json!({"geometry": {"kind": "polygon", "points": [[0, 0], [4, 0], [4, 4]]}}),
        );
        assert_eq!(
            object_bbox(&polygon).unwrap(),
            Bbox::new(0.0, 0.0, 4.0, 4.0)
        );
        let ellipse = object(
            ObjectType::Shape,
            json!({"geometry": {"kind": "ellipse", "bbox": {"x": 0, "y": 0, "w": 2, "h": 2}}}),
        );
        matches!(parse_object(&ellipse), Primitive::Shape { .. });
    }

    #[test]
    fn raster_patch_requires_size_and_hash() {
        let good = object(
            ObjectType::RasterPatch,
            json!({
                "bitmap": {"blob_hash": format!("sha256:{}", "a".repeat(64)), "size": 16, "mime_type": "image/x-yanshi-raw"},
                "width": 2,
                "height": 2,
                "region": {"x": 5, "y": 6, "w": 2, "h": 2},
            }),
        );
        match parse_object(&good) {
            Primitive::RasterPatch {
                width,
                height,
                offset,
                mime_type,
                ..
            } => {
                assert_eq!((width, height), (2, 2));
                assert_eq!(offset, (5.0, 6.0));
                assert_eq!(mime_type, "image/x-yanshi-raw");
            }
            other => panic!("期望 RasterPatch，得到 {other:?}"),
        }
        assert_eq!(object_bbox(&good).unwrap(), Bbox::new(5.0, 6.0, 2.0, 2.0));

        let missing = object(
            ObjectType::RasterPatch,
            json!({"bitmap": {"blob_hash": "bad"}}),
        );
        assert!(matches!(
            parse_object(&missing),
            Primitive::Unsupported { .. }
        ));
        assert!(object_bbox(&missing).is_none());
    }

    #[test]
    fn unimplemented_types_report_unsupported() {
        for object_type in [
            ObjectType::Text,
            ObjectType::Retouch,
            ObjectType::Liquify,
            ObjectType::Instance,
            ObjectType::Group,
        ] {
            let parsed = parse_object(&object(object_type, json!({})));
            assert!(
                matches!(parsed, Primitive::Unsupported { .. }),
                "{object_type:?} 应报告 unsupported"
            );
        }
    }

    #[test]
    fn adjustments_and_filters_dispatch() {
        let adjustment = object(
            ObjectType::Adjustment,
            json!({"adjustment_type": "saturation", "params": {"amount": 0.5}}),
        );
        match parse_object(&adjustment) {
            Primitive::Adjustment { kind, params } => {
                assert_eq!(kind, "saturation");
                assert_eq!(params["amount"], json!(0.5));
            }
            other => panic!("期望 Adjustment，得到 {other:?}"),
        }
        let filter = object(
            ObjectType::Filter,
            json!({"filter_name": "box_blur", "params": {"radius": 2}}),
        );
        match parse_object(&filter) {
            Primitive::Filter { name, params } => {
                assert_eq!(name, "box_blur");
                assert_eq!(params["radius"], json!(2));
            }
            other => panic!("期望 Filter，得到 {other:?}"),
        }
        let broken = object(ObjectType::Adjustment, json!({}));
        assert!(matches!(
            parse_object(&broken),
            Primitive::Unsupported { .. }
        ));
    }

    #[test]
    fn colors_accept_linear_and_byte_forms() {
        assert_eq!(
            parse_color(&json!([0.1, 0.2, 0.3, 0.4])),
            [0.1, 0.2, 0.3, 0.4]
        );
        let byte = parse_color(&json!({"r": 255, "g": 0, "b": 0}));
        assert!((byte[0] - 1.0).abs() < 1e-6);
        assert_eq!(byte[3], 1.0, "缺省 alpha = 255");
        assert_eq!(parse_color(&json!(null)), [0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn layer_bbox_is_union_of_object_bboxes() {
        let mut state = yanshi_core::DocumentState::empty();
        state.width = 100;
        state.height = 100;
        let layer = yanshi_core::Layer {
            id: "layer_1".to_owned(),
            name: "l".to_owned(),
            layer_type: yanshi_core::LayerType::Raster,
            parent_id: None,
            z_index: 0,
            blend_mode: "normal".to_owned(),
            opacity: 1.0,
            visible: true,
            locked: false,
            alpha_lock: false,
            clipping_mask: false,
            mask_id: None,
            transform: Transform::IDENTITY,
            medium: None,
            style: None,
            metadata: Value::Null,
            blobs: Vec::new(),
            created_by: "a1".to_owned(),
            updated_by: None,
            deleted_by: None,
        };
        state.layers.insert("layer_1".to_owned(), layer);
        let mut first = object(
            ObjectType::Shape,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 0, "y": 0, "w": 10, "h": 10}}}),
        );
        first.id = "obj_a".to_owned();
        let mut second = object(
            ObjectType::Shape,
            json!({"geometry": {"kind": "rect", "bbox": {"x": 20, "y": 20, "w": 10, "h": 10}}}),
        );
        second.id = "obj_b".to_owned();
        state.objects.insert("obj_a".to_owned(), first);
        state.objects.insert("obj_b".to_owned(), second);
        assert_eq!(
            layer_bbox(&state, "layer_1"),
            Some(Bbox::new(0.0, 0.0, 30.0, 30.0))
        );
        assert_eq!(layer_bbox(&state, "layer_missing"), None);
    }
}
