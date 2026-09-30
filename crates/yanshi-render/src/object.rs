//! 对象 → 渲染图元解析（设计文档 4.2）。
//!
//! 解析是**只读**的：把 `Object.data` 中的类型专属字段翻译成内核可渲染的图元，
//! 不修改状态。无法渲染的对象（尚未实现的类型、缺少字体、未实现的位图编解码）
//! 返回 [`Primitive::Unsupported`]，由调用方计入告警而不是让整次渲染失败。

use crate::brush::{BrushSpec, StrokeGeometry, StrokePoint};
use crate::color::LinearRgba;
use serde_json::Value;
use yanshi_core::{Bbox, BlobHash, Object, ObjectType, Transform};

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
    /// 文本（设计 4.2：`text` / `font` / `size` / `color` / `align`）。
    ///
    /// 内核目前只有**内置 5×7 ASCII 位图字体**（路线 A 的最小切片 ✓）：
    /// `font` 字段被接受但暂时只有内置字体一种实现 ✓；CJK 字体子集是后续项 ✓
    /// （设计 1175/1287 行要求"内嵌开源字体子集"，属多轮工程，已记录）。
    Text {
        /// 文本内容（`\n` 换行）。
        text: String,
        /// 字体名（暂只用内置字体）。
        font: String,
        /// 字号（像素高度）。
        size: f64,
        /// 颜色（直通线性）。
        color: LinearRgba,
        /// 对齐：`left` / `center` / `right`。
        align: String,
        /// 左上角位置（文档坐标）。
        position: (f64, f64),
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
        /// 修图类型（`clone_stamp` / `heal`）。
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
        /// 涂抹半径（仅 `smudge` 使用：每 stamp 沿笔迹后退多少像素采样）。
        smudge_length: f64,
    },
    /// 基础液化：把笔迹范围内的像素按方向推开（推力）。
    ///
    /// 采用**反向映射**：目标像素去「应用本对象之前」的副本采样，
    /// 因此不会出现空洞，且位移只由原子参数决定（确定性）。
    Liquify {
        /// 模式：`push`（推力）/ `twirl`（旋转）/ `pinch`（收缩，负强度即膨胀）。
        mode: String,
        /// 笔迹点列。
        points: Vec<(f64, f64)>,
        /// 影响半径（直径 = `size`）。
        size: f64,
        /// 强度（0..2；`pinch` 允许负值表示膨胀）。
        strength: f64,
        /// 方向（`push` 使用；单位向量，归一化后使用）。
        direction: (f64, f64),
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
        ObjectType::Path => parse_path(&object.data),
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
        ObjectType::Text => {
            let position = object
                .data
                .get("position")
                .map(|value| {
                    if let Some(pair) = value.as_array() {
                        (
                            pair.first().and_then(Value::as_f64).unwrap_or(0.0),
                            pair.get(1).and_then(Value::as_f64).unwrap_or(0.0),
                        )
                    } else {
                        (
                            value.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                            value.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                        )
                    }
                })
                .or_else(|| {
                    object
                        .data
                        .get("bbox")
                        .and_then(Bbox::from_value)
                        .map(|b| (b.x, b.y))
                })
                .unwrap_or((0.0, 0.0));
            Primitive::Text {
                text: object
                    .data
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                font: object
                    .data
                    .get("font")
                    .and_then(Value::as_str)
                    .unwrap_or("builtin")
                    .to_owned(),
                size: object
                    .data
                    .get("size")
                    .and_then(Value::as_f64)
                    .unwrap_or(24.0)
                    .max(1.0),
                color: object
                    .data
                    .get("color")
                    .map(parse_color)
                    .unwrap_or([1.0, 1.0, 1.0, 1.0]),
                align: object
                    .data
                    .get("align")
                    .and_then(Value::as_str)
                    .unwrap_or("left")
                    .to_owned(),
                position,
            }
        }
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
                    smudge_length: object
                        .data
                        .get("smudge_length")
                        .and_then(Value::as_f64)
                        .unwrap_or(12.0),
                }
            }
        }
        ObjectType::Liquify => {
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
            let direction = object
                .data
                .get("direction")
                .and_then(Value::as_array)
                .map(|pair| {
                    (
                        pair.first().and_then(Value::as_f64).unwrap_or(0.0),
                        pair.get(1).and_then(Value::as_f64).unwrap_or(0.0),
                    )
                })
                .unwrap_or((0.0, 0.0));
            let mode = object
                .data
                .get("liquify_type")
                .or_else(|| object.data.get("mode"))
                .and_then(Value::as_str)
                .unwrap_or("push")
                .to_owned();
            let known = matches!(mode.as_str(), "push" | "twirl" | "pinch");
            if points.is_empty() {
                Primitive::Unsupported {
                    reason: "liquify 缺少 points".to_owned(),
                }
            } else if !known {
                Primitive::Unsupported {
                    reason: format!("液化模式未实现: {mode}（内核支持 push/twirl/pinch）"),
                }
            } else if mode == "push" && direction.0 == 0.0 && direction.1 == 0.0 {
                Primitive::Unsupported {
                    reason: "push 模式缺少 direction（或为零向量）".to_owned(),
                }
            } else {
                Primitive::Liquify {
                    mode,
                    points,
                    size: object
                        .data
                        .get("size")
                        .and_then(Value::as_f64)
                        .unwrap_or(80.0),
                    strength: object
                        .data
                        .get("strength")
                        .and_then(Value::as_f64)
                        .unwrap_or(0.5),
                    direction,
                }
            }
        }
        ObjectType::Instance | ObjectType::Group => Primitive::Unsupported {
            reason: "实例/组引用解析属 Phase 5（9 章）".to_owned(),
        },
    }
}

/// 每段三次贝塞尔的**固定细分数** ✓。
///
/// **记录选择** ✓：设计没有规定路径如何栅格化 ✓。这里取固定细分（而不是自适应容差 ✓），
/// 因为它的**成本可预测**且**完全确定** ✓（与全项目的确定性要求一致 ✓）；
/// 代价是极长的曲线段会用较多采样点 ✓（固定 16 段足够平滑，见测试 ✓）。
/// 若将来需要更省或更精确，可以换成按弦长自适应 ✓ —— 那会改变渲染结果 ✓，
/// 因此必须与"插件/路径版本"一类机制一起考虑 ✓（就像第 28 轮 `oil.wasm` 的取舍 ✓）。
const PATH_SUBDIVISIONS: usize = 16;

/// **把路径铺平成折线** ✓（`nodes[{x,y,in,out}]` + `closed` ⇒ 点序列 ✓）。
///
/// 相邻节点之间是三次贝塞尔 ✓：`P(t) = (1-t)³·A + 3(1-t)²t·(A+out_A) + 3(1-t)t²·(B+in_B) + t³·B` ✓，
/// 其中 `in`/`out` 是**相对节点的偏移** ✓（与常见钢笔工具一致 ✓）。
/// **零柄 ⇒ 退化成直线** ✓ ⇒ 与同点列的笔迹**逐点一致** ✓（`convert_to_path` 的往返测试就靠这条 ✓）。
pub fn flatten_path(nodes: &[Value], closed: bool) -> Vec<(f64, f64)> {
    let parsed: Vec<(f64, f64, f64, f64, f64, f64)> = nodes
        .iter()
        .filter_map(|node| {
            let x = node.get("x").and_then(Value::as_f64)?;
            let y = node.get("y").and_then(Value::as_f64)?;
            let handle = |key: &str, index: usize| -> f64 {
                node.get(key)
                    .and_then(Value::as_array)
                    .and_then(|pair| pair.get(index))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            };
            Some((
                x,
                y,
                handle("in", 0),
                handle("in", 1),
                handle("out", 0),
                handle("out", 1),
            ))
        })
        .collect();
    if parsed.len() < 2 {
        return parsed
            .into_iter()
            .map(|(x, y, _, _, _, _)| (x, y))
            .collect();
    }
    let segments = if closed {
        parsed.len()
    } else {
        parsed.len() - 1
    };
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(segments * PATH_SUBDIVISIONS + 1);
    for index in 0..segments {
        let (ax, ay, _aix, _aiy, aox, aoy) = parsed[index];
        let (bx, by, bix, biy, _box, _boy) = parsed[(index + 1) % parsed.len()];
        let (c1x, c1y) = (ax + aox, ay + aoy);
        let (c2x, c2y) = (bx + bix, by + biy);
        // **零柄段就是直线 ⇒ 只发端点** ✓，不细分 ✓。
        //
        // 这不是省事的小聪明 ✓，而是**正确性**要求 ✓：我第一版对每段都固定细分 ✗
        // ⇒ `convert_to_path`（节点取原采样点、控制柄留空 ✓）之后，折线比原来**更密** ✓
        // ⇒ 按间距重采样的落点随之偏移 ✓ ⇒ 测试实测到**画面差 3042 字节** ✗。
        // 直线上多插点**不改变几何** ✓，却改变了**采样相位** ✗ —— 这正是"看起来等价、结果不等价"的典型 ✓。
        if aox == 0.0 && aoy == 0.0 && bix == 0.0 && biy == 0.0 {
            // **发这一段的起点** ✓（不是 `continue` ✗）。
            //
            // 我第一版写 `continue` ✗ ⇒ 整段被跳过 ✓，只有循环末尾补的那个终点 ✓
            // ⇒ 折线只剩**一个点** ⇒ `parse_path` 判"至少两个节点" ⇒ 返回 `Unsupported` ⇒
            // **路径一个像素都不画** ✗（探针实测：`转换后墨=0` ✓、换成两点直线仍是 0 ✓）。
            // 这就是"看起来只是省几个采样点"的改动如何变成"完全不渲染"的 ✓ ——
            // 所以**每一段都必须贡献起点** ✓，末点由循环外的统一收尾补上 ✓。
            out.push((ax, ay));
            continue;
        }
        // 段内采样 ✓：只发 0..N-1 ✓（末点由下一段负责 ✓），最后一段补上终点 ✓。
        for step in 0..PATH_SUBDIVISIONS {
            let t = step as f64 / PATH_SUBDIVISIONS as f64;
            let (mt, t2, t3) = (1.0 - t, t * t, t * t * t);
            let (mt2, mt3) = (mt * mt, mt * mt * mt);
            out.push((
                mt3 * ax + 3.0 * mt2 * t * c1x + 3.0 * mt * t2 * c2x + t3 * bx,
                mt3 * ay + 3.0 * mt2 * t * c1y + 3.0 * mt * t2 * c2y + t3 * by,
            ));
        }
    }
    if closed {
        // 闭合路径：末尾回到起点 ✓（让描边首尾相接 ✓）。
        let (ax, ay, _, _, _, _) = parsed[0];
        out.push((ax, ay));
    } else {
        let (bx, by, _, _, _, _) = parsed[parsed.len() - 1];
        out.push((bx, by));
    }
    out
}

/// **路径对象** ✓（设计 792 与 11.1「矢量」所依赖的类型 ✓）。
///
/// **渲染策略（记录选择 ✓）**：把节点铺平成折线 ✓，然后**复用笔迹图元** ✓ ——
/// 于是笔刷参数、`appearance`、选区约束、脏区与命中测试**全部自动继承** ✓，
/// 而"分辨率无关"这件事本来就由"**几何存日志、按视图重新栅格化**"提供 ✓
///（这正是第 30 轮把矢量与光栅介质区分开时写下的结论 ✓）。
fn parse_path(data: &Value) -> Primitive {
    let Some(nodes) = data.get("nodes").and_then(Value::as_array) else {
        return Primitive::Unsupported {
            reason: "path 缺少 nodes".to_owned(),
        };
    };
    let closed = data.get("closed").and_then(Value::as_bool).unwrap_or(false);
    let points = flatten_path(nodes, closed);
    if points.len() < 2 {
        return Primitive::Unsupported {
            reason: "path 至少需要两个节点".to_owned(),
        };
    }
    Primitive::Stroke {
        geometry: StrokeGeometry {
            points: points
                .into_iter()
                .map(|(x, y)| StrokePoint {
                    x,
                    y,
                    pressure: 1.0,
                })
                .collect(),
            // 铺平本身已经是曲线采样 ✓ ⇒ 不再叠加笔迹平滑 ✓（否则会二次平滑 ✓）。
            smooth: false,
        },
        brush: BrushSpec::from_value(data),
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
/// 把对象的仿射 `transform` 施加到**图元几何**上。
///
/// 之前内核**完全不读 `object.transform`** ✗（`object_bbox` 的注释写着"尚未进入内核"），
/// 因此 `move_object` / `transform_object` 返回 `ok: true` 却**没有任何视觉效果** ✓。
/// 这里在解析之后统一施加一次，覆盖所有绘制分支 ✓。
///
/// 纯平移之外的旋转/缩放只对**点集**精确（笔触/形状/修图点列 ✓）；
/// 位图补丁（`RasterPatch`）目前只支持平移 ✓ —— 旋转/缩放需要重采样 ✓，记为后续项 ✓。
/// **实例解析** ✓ —— 设计 9.2 `resolve_object` 的第一步 ✓。
///
/// 返回"master 的图元 + 依次施加的变换" ✓，调用方按 [`transform_primitive`] 逐层施加即可 ✓：
/// 先 master 自己的 `transform` ✓、再 `master_ref.local_transform` ✓、最后实例自身的 `transform` ✓。
///
/// **设计未规定的两处，记录选择 ✓**：
/// * `override` 与 `sync_policy`（除 `all` 外）本片**不支持** ✓ —— 它们需要 9.3 的缓存与依赖图 ✓，
///   因此在**工具层**创建时就明确拒绝 ✓（见 `create_instance` ✓），而不是默默忽略 ✓；
/// * **master 不存在时不报错、也不画** ✓：日志顺序允许"先建实例、后建 master" ✓，
///   解析不到就什么都不画 ✓，master 补齐后**自动恢复** ✓ —— 这是"日志决定渲染"的直接推论 ✓。
///
/// 循环引用**在折叠层就被挡住** ✓（见 `fold.rs` ✓）；这里仍带一个**深度上限** ✓，
/// 作为纵深防御 ✓：万一有环漏过来 ✓，渲染也不能无限递归 ✓（宁可不画 ✓）。
pub fn resolve_instance(
    state: &yanshi_core::DocumentState,
    object: &Object,
) -> Option<(Primitive, Transform)> {
    let mut composed = object.transform;
    // **`override.transform` 是最外层的额外变换** ✓（设计 9.3：「override 只含 transform 时，
    // 直接变换 Master 位图」✓）。设计未规定它与 `local_transform` 的相对次序 ⇒ **记录选择** ✓：
    // `local_transform` 属于**引用**（"取 master 的哪一部分" ✓），`override` 属于**这次引用上的修正** ✓
    // ⇒ override 放在最外 ✓（改它只动这一个实例 ✓，不碰 master、也不碰链接本身 ✓）。
    let override_transform = object
        .data
        .get("override")
        .and_then(|override_value| override_value.get("transform"))
        .map(transform_from_value);
    if let Some(override_transform) = &override_transform {
        composed = compose_transform(override_transform, &composed);
    }
    // **`sync_policy: none` ⇒ 用快照** ✓（设计 9.1 的"不同步"✓）。
    // 设计未规定"不同步"怎么落地 ⇒ **记录选择** ✓：改策略的那一刻由**折叠层**把解析结果
    // 快照进实例自身（`data.snapshot` + `data.snapshot_transform` ✓）⇒ 渲染优先用它 ✓；
    // 改回 `all` 时折叠层**清掉**快照 ✓ ⇒ 立刻恢复跟随 ✓（可来回切换 ✓、可验证 ✓）。
    if object.data.get("sync_policy").and_then(Value::as_str) == Some("none") {
        let snapshot = object.data.get("snapshot")?;
        let snapshot_transform = object
            .data
            .get("snapshot_transform")
            .map(transform_from_value)
            .unwrap_or(Transform::IDENTITY);
        let mut synthetic = object.clone();
        synthetic.object_type = yanshi_core::ObjectType::Shape;
        synthetic.data = snapshot.clone();
        return Some((
            parse_object(&synthetic),
            compose_transform(&composed, &snapshot_transform),
        ));
    }
    let mut cursor = object.clone();
    for _ in 0..64 {
        let master_ref = cursor.data.get("master_ref")?;
        let master_id = master_ref.get("object_id").and_then(Value::as_str)?;
        // `local_transform` ✓（缺省即恒等 ✓）。
        if let Some(local) = master_ref.get("local_transform") {
            let local = transform_from_value(local);
            composed = compose_transform(&local, &composed);
        }
        let master = state.objects.get(master_id)?;
        if master.is_deleted() {
            // master 被删 ⇒ 什么都不画 ✓（但引用还在 ✓，恢复 master 后实例自动回来 ✓）。
            return None;
        }
        if master.object_type == yanshi_core::ObjectType::Instance {
            // 链上还有实例 ⇒ 继续往上 ✓（同样先叠加它自己的变换 ✓）。
            composed = compose_transform(&master.transform, &composed);
            cursor = master.clone();
            continue;
        }
        // 到顶了 ✓：返回 master 的图元与合成好的变换 ✓。
        let primitive = parse_object(master);
        return Some((primitive, compose_transform(&master.transform, &composed)));
    }
    // 超过深度上限：**不画** ✓（宁缺勿递归 ✓）。
    None
}

/// 把 `{matrix, pivot}` 解析成 [`Transform`] ✓（缺省恒等 ✓）。
fn transform_from_value(value: &Value) -> Transform {
    let mut transform = Transform::IDENTITY;
    if let Some(matrix) = value.get("matrix").and_then(Value::as_array) {
        if matrix.len() == 6 {
            for (slot, item) in transform.matrix.iter_mut().zip(matrix) {
                *slot = item.as_f64().unwrap_or(0.0);
            }
        }
    }
    if let Some(pivot) = value.get("pivot").and_then(Value::as_array) {
        if pivot.len() == 2 {
            transform.pivot = [
                pivot[0].as_f64().unwrap_or(0.0),
                pivot[1].as_f64().unwrap_or(0.0),
            ];
        }
    }
    transform
}

/// 变换合成 ✓：`outer ∘ inner` ⇒ 先用 `inner` 再用 `outer` ✓。
///
/// 两者都带 `pivot` ✓，因此这里**只在矩阵层面合成** ✓，并把 `pivot` 归零 ✓ ——
/// 语义上等价于"先绕 inner.pivot 施加 inner ✓，再绕 outer.pivot 施加 outer" ✓
/// 吗？**不等价** ✗。所以这里把 pivot 折进矩阵 ✓：`M' = T(p) · M · T(-p)` ✓，
/// 再相乘 ✓ ⇒ 与逐层施加**完全一致** ✓（这正是 `transform_primitive` 的做法 ✓）。
fn compose_transform(outer: &Transform, inner: &Transform) -> Transform {
    let outer_matrix = flatten(outer);
    let inner_matrix = flatten(inner);
    // 2×3 仿射相乘 ✓（与 `transform_primitive` 同一约定 ✓）。
    let mut matrix = [0.0f64; 6];
    matrix[0] = outer_matrix[0] * inner_matrix[0] + outer_matrix[2] * inner_matrix[1];
    matrix[1] = outer_matrix[1] * inner_matrix[0] + outer_matrix[3] * inner_matrix[1];
    matrix[2] = outer_matrix[0] * inner_matrix[2] + outer_matrix[2] * inner_matrix[3];
    matrix[3] = outer_matrix[1] * inner_matrix[2] + outer_matrix[3] * inner_matrix[3];
    matrix[4] =
        outer_matrix[0] * inner_matrix[4] + outer_matrix[2] * inner_matrix[5] + outer_matrix[4];
    matrix[5] =
        outer_matrix[1] * inner_matrix[4] + outer_matrix[3] * inner_matrix[5] + outer_matrix[5];
    Transform {
        matrix,
        pivot: [0.0, 0.0],
    }
}

/// **带状态的包围盒** ✓ —— 实例要按 master 解析 ✓，所以包围盒必须能拿到 [`DocumentState`] ✓。
///
/// 这一步**不能推迟** ✗（我原本想留下一轮 ✓）：渲染循环按**包围盒裁剪** ✓，
/// 而实例的 `parse_object` 本身没有几何 ✗ ⇒ 用旧口径得到的是空包围盒 ⇒ **实例永远被裁掉** ✓
/// （实测："实例应在 local_transform 指定的位置画出 master（实测 0）" ✓）。
///
/// 做法刻意从简 ✓：构造一个"**master 的副本 + 合成后的变换**"的临时对象 ✓，
/// 交给已有的 [`object_bbox`] ✓ ⇒ 包围盒口径与渲染口径**必然一致** ✓（不会两处各算一套 ✓）。
pub fn object_bbox_in(state: &yanshi_core::DocumentState, object: &Object) -> Option<Bbox> {
    if object.object_type != yanshi_core::ObjectType::Instance {
        return object_bbox(object);
    }
    let (primitive, transform) = resolve_instance(state, object)?;
    // 临时对象只用来复用 `object_bbox` 的图元分支 ✓（`data` 用 master 的 ✓，`transform` 用合成后的 ✓）。
    let mut synthetic = object.clone();
    synthetic.object_type = yanshi_core::ObjectType::Shape;
    synthetic.transform = transform;
    // 让 `parse_object` 走 master 的图元分支 ✓：直接把 master 的 data 塞进来 ✓。
    if let Some(master) = resolved_master(state, object) {
        synthetic.data = master.data.clone();
    }
    let _ = primitive;
    object_bbox(&synthetic)
}

/// 取实例最终解析到的 **master 对象** ✓（跳过中间层的实例 ✓）。
fn resolved_master<'a>(
    state: &'a yanshi_core::DocumentState,
    object: &Object,
) -> Option<&'a Object> {
    let mut cursor = object.clone();
    for _ in 0..64 {
        let master_id = cursor
            .data
            .get("master_ref")
            .and_then(|master_ref| master_ref.get("object_id"))
            .and_then(Value::as_str)?;
        let master = state.objects.get(master_id)?;
        if master.is_deleted() {
            return None;
        }
        if master.object_type == yanshi_core::ObjectType::Instance {
            cursor = master.clone();
            continue;
        }
        return Some(master);
    }
    None
}

/// 把带 `pivot` 的变换折成纯 2×3 矩阵 ✓：`T(pivot) · M · T(-pivot)` ✓。
fn flatten(transform: &Transform) -> [f64; 6] {
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

/// 对图元施加仿射变换 ✓（内核里**唯一**施加对象变换的入口 ✓）。
///
/// 返回图元本身（几何已就地变换 ✓）；恒等变换直接原样返回 ✓。
pub fn transform_primitive(mut primitive: Primitive, transform: &Transform) -> Primitive {
    if transform.is_identity() {
        return primitive;
    }
    let map_point = |x: f64, y: f64| transform.apply_point(x, y);
    match &mut primitive {
        Primitive::Stroke { geometry, .. } => {
            for point in &mut geometry.points {
                let (x, y) = map_point(point.x, point.y);
                point.x = x;
                point.y = y;
            }
        }
        Primitive::Shape { bbox, points, .. } => {
            let (x0, y0) = map_point(bbox.x, bbox.y);
            let (x1, y1) = map_point(bbox.x + bbox.w, bbox.y + bbox.h);
            *bbox = Bbox::new(
                x0.min(x1),
                y0.min(y1),
                (x1 - x0).abs().max(bbox.w.min(0.0).abs()),
                (y1 - y0).abs().max(bbox.h.min(0.0).abs()),
            );
            if bbox.w == 0.0 {
                bbox.w = (x1 - x0).abs();
            }
            if bbox.h == 0.0 {
                bbox.h = (y1 - y0).abs();
            }
            for point in points.iter_mut() {
                let (x, y) = map_point(point.0, point.1);
                *point = (x, y);
            }
        }
        Primitive::Retouch { points, .. } => {
            for point in points.iter_mut() {
                let (x, y) = map_point(point.0, point.1);
                *point = (x, y);
            }
        }
        Primitive::Text { position, .. } => {
            // 文本目前只支持平移（旋转/缩放需要重排与重新栅格化，记为后续项 ✓）。
            let (x, y) = map_point(position.0, position.1);
            *position = (x, y);
        }
        Primitive::RasterPatch { offset, .. } => {
            let (x, y) = map_point(offset.0, offset.1);
            *offset = (x, y);
        }
        Primitive::Liquify { .. } => {
            // 液化的位移场按对象参数在本地计算；变换施加到其作用区域需要额外推导，
            // 记为后续项（当前保持本地坐标 ⇒ 移动液化对象暂不改变结果 ✓ 已记录）。
        }
        // 调整/滤镜作用于整层、无自身几何；Text 目前仍是 Unsupported。
        Primitive::Adjustment { .. } | Primitive::Filter { .. } | Primitive::Unsupported { .. } => {
        }
    }
    primitive
}

/// 对象的文档坐标包围盒（**已施加对象变换**）。
/// 文本栅格化后的整数缩放倍率（内置字形高 7px）。
pub fn text_scale_for_size(size: f64) -> u32 {
    let scale = (size.max(1.0) / crate::font::GLYPH_HEIGHT as f64).round() as i64;
    scale.clamp(1, 64) as u32
}

/// 对象的文档坐标包围盒（**已施加对象变换**）。
pub fn object_bbox(object: &Object) -> Option<Bbox> {
    // 对象的仿射 `transform` 尚未进入内核（Phase 3 起按对象应用），
    // 因此这里返回的是对象**本地**包围盒。
    // 先施加对象变换，再按**变换后**的图元算包围盒 ⇒ 剔除与命中测试都跟着走 ✓
    //（查看器的「移动」工具正是用 list_objects 的 bbox 做命中测试 ✓）。
    let local = match transform_primitive(parse_object(object), &object.transform) {
        Primitive::Text {
            text,
            size,
            position,
            ..
        } => {
            let (width, height) =
                // **与绘制同源** ✓：ASCII 用 5×7 度量 ✓、含 CJK 用图集度量 ✓。
                // 此前这里固定用 5×7 度量 ✗ ⇒ CJK 文本的包围盒远大于实际墨迹 ✓
                //（子 agent 实测 510×119 vs 92×14 ✓）⇒ 脏区规划会把无关区域算进来 ✓，
                // 而真正越界的那一小块又可能算不进去 ✓ ⇒ 画布与缩略图/导出不一致 ✓（一致性自检不通过 ✓）。
                crate::font::measure_sized(text.as_str(), size, 0.0);
            return Some(Bbox::new(
                position.0,
                position.1,
                width as f64,
                height as f64,
            ));
        }
        Primitive::Liquify {
            points,
            size,
            strength,
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
            // 影响半径 + 最大位移（强度按直径缩放）都要计入。
            let reach = size / 2.0 + strength.abs().clamp(0.0, 2.0) * size;
            Bbox::new(
                min_x - reach,
                min_y - reach,
                (max_x - min_x) + reach * 2.0,
                (max_y - min_y) + reach * 2.0,
            )
        }
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

    /// 文本**已经实现**（路线 A 最小切片：内置 5×7 ASCII 位图字体 ✓），
    /// 因此把它从"未实现类型"里移出，并**正向断言**它解析成 `Primitive::Text` ✓
    /// —— 这里不是为了让测试变绿而放宽，而是因为断言描述的是旧状态 ✗，必须显式改写 ✓。
    #[test]
    fn text_parses_into_the_text_primitive() {
        let parsed = parse_object(&object(
            ObjectType::Text,
            json!({"text": "AB", "font": "builtin", "size": 14.0, "align": "center",
                   "position": [3.0, 4.0], "color": {"r": 255, "g": 255, "b": 255, "a": 255}}),
        ));
        match parsed {
            Primitive::Text {
                text,
                size,
                align,
                position,
                ..
            } => {
                assert_eq!(text, "AB");
                assert_eq!(size, 14.0);
                assert_eq!(align, "center");
                assert_eq!(position, (3.0, 4.0));
            }
            other => panic!("Text 应解析为 Primitive::Text，实际 {other:?}"),
        }
        // 包围盒按栅格化尺寸给出（scale = round(14 / 7) = 2 ⇒ "AB" 宽 2×2×5 = 20）。
        let bbox = object_bbox(&object(
            ObjectType::Text,
            json!({"text": "AB", "size": 14.0, "position": [3.0, 4.0]}),
        ))
        .expect("文本应有包围盒");
        assert_eq!((bbox.x, bbox.y), (3.0, 4.0));
        assert_eq!(bbox.w, 20.0);
    }

    #[test]
    fn unimplemented_types_report_unsupported() {
        for object_type in [
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
