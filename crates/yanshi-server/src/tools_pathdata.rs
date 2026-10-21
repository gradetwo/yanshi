//! **路径数据面工具**（A1：`get_path` 读回 + `edit_path` 节点级编辑）。
//!
//! 此前 path 形状**只写不读**：`draw_shape` 能建、`get_object` 只能拿原始
//! `data` JSON（调用方得自己懂入库形态），改一条边只能整形重发。本模块补
//! 数据面：读回给结构化 subpaths + 等价 `d` + bbox + 总长；编辑给节点级
//! 最小集（move/insert/delete/set_handle），几何运算全在渲染层纯函数
//! （`yanshi_render::path`）里，本模块只做寻址、校验与落原子。
//!
//! **依赖**：`use super::*` 拿 `tools.rs` 的公共 helper（同 crate 子模块）。

use super::*;

/// 一个已解析的 path 形状对象（工具层寻址结果）。
pub(super) struct LoadedPath {
    /// 对象 id。
    pub(super) object_id: String,
    /// 对象所在图层。
    pub(super) layer_id: String,
    /// 对象 `data` 的克隆（编辑时在此基础上改 geometry 后 Supersede）。
    pub(super) data: Value,
    /// 解析出的子路径列（与 `object::parse_object` 同一份解析）。
    pub(super) subpaths: Vec<yanshi_render::path::Subpath>,
    /// 填充规则（对象 geometry 的 `fill_rule`，缺省 evenodd）。
    pub(super) fill_rule: yanshi_render::path::FillRule,
}

/// 按 `object_id` 寻址一个 path 形状对象（与 `create_mask` 的 object_id
/// 引用**同一口径**）：不存在 / 已删除 ⇒ `ReferenceNotFound` 并点名 id；
/// 存在但不是 path 形状 ⇒ `InvalidArgument` 并点名 id。绝不静默回退。
pub(super) fn load_path_object(ctx: &mut ToolContext<'_>, object_id: &str) -> Result<LoadedPath> {
    let state = document_state(ctx)?;
    let Some(object) = state.objects.get(object_id) else {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 不存在")),
        )
        .with_object(object_id.to_owned()));
    };
    if object.deleted_by.is_some() {
        return Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("对象 {object_id} 已被删除")),
        )
        .with_object(object_id.to_owned()));
    }
    match yanshi_render::object::parse_object(object) {
        yanshi_render::object::Primitive::Shape {
            kind: yanshi_render::object::ShapeKind::Path,
            subpaths,
            fill_rule,
            ..
        } => Ok(LoadedPath {
            object_id: object_id.to_owned(),
            layer_id: object.layer_id.clone(),
            data: object.data.clone(),
            subpaths,
            fill_rule,
        }),
        _ => Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "对象 {object_id} 不是 path 形状（只认 draw_shape 建的 kind=path 对象）"
            )),
        )
        .with_object(object_id.to_owned())),
    }
}

/// `[x, y]` 参数点（有限数）；`label` 进错误信息（如「to」）。
fn pathdata_point(args: &Value, key: &str) -> Result<(f64, f64)> {
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    let Some(pair) = args.get(key).and_then(Value::as_array) else {
        return Err(invalid(format!("参数 {key} 必须是 [x, y] 两个有限数")));
    };
    if pair.len() != 2 {
        return Err(invalid(format!("参数 {key} 必须是 [x, y] 两个有限数")));
    }
    let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) else {
        return Err(invalid(format!("参数 {key} 必须是 [x, y] 两个有限数")));
    };
    if !x.is_finite() || !y.is_finite() {
        return Err(invalid(format!("参数 {key} 必须是 [x, y] 两个有限数")));
    }
    Ok((x, y))
}

/// 非负整数下标参数（`subpath` / `node` / `segment`）。
fn pathdata_index(args: &Value, key: &str) -> Result<usize> {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("参数 {key} 必须是非负整数下标")),
            )
        })
}

/// path 形状的完整读回（`get_path`，只读）。
///
/// 返回的是对象**本地坐标**几何（不含 `object.transform`，与 `get_object`
/// 的 `data` 同一口径）：结构化 `subpaths`、可直接塞回 `draw_shape` 的
/// `geometry`、等价 SVG `d`（最短往返数字 ⇒ 再解析逐位一致）、控制点
/// 包围盒 `bbox`、展平总长 `length`（闭合含回首段）、每条子路径的
/// `node_counts`（`edit_path` 的节点下标口径 = 段数 + 1）。
pub(super) fn read_get_path(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let loaded = load_path_object(ctx, &object_id)?;
    let subpaths_json = yanshi_render::object::subpaths_to_json(&loaded.subpaths);
    let (bx, by, bw, bh) =
        yanshi_render::object::path_control_bbox(&loaded.subpaths).unwrap_or((0.0, 0.0, 0.0, 0.0));
    Ok(json!({
        "ok": true,
        "object_id": loaded.object_id,
        "layer_id": loaded.layer_id,
        "kind": "path",
        "fill_rule": loaded.fill_rule.as_str(),
        "geometry": {
            "kind": "path",
            "fill_rule": loaded.fill_rule.as_str(),
            "subpaths": subpaths_json,
        },
        "subpaths": subpaths_json,
        "d": yanshi_render::path::subpaths_to_svg_d(&loaded.subpaths),
        "bbox": {"x": bx, "y": by, "w": bw, "h": bh},
        "length": yanshi_render::path::subpaths_total_length(&loaded.subpaths),
        "node_counts": loaded
            .subpaths
            .iter()
            .map(|sub| sub.segments.len() + 1)
            .collect::<Vec<_>>(),
    }))
}

/// 节点级编辑（`edit_path`，写）：四个算子共用一条路——改内存里的
/// subpaths 克隆 → 用同一份校验复验整份几何 → 写回 `data.geometry`
/// （subpaths 与 bbox 同步更新，bbox 不许留旧值骗脏区）→ Supersede 落原子。
///
/// 节点 / 控制柄语义（锚点下标、刚性平移跟随、de Casteljau 插点、删点
/// 合并规则、直段升格补柄）全部定义在 `yanshi_render::path` 的四个纯
/// 函数文档里，本函数不另立口径。
pub(super) fn write_edit_path(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let object_id = require_str(args, "object_id")?;
    let op = require_str(args, "op")?;
    let invalid = |message: String| {
        YanshiError::new(ErrorCode::InvalidArgument, ErrorContext::detail(message))
    };
    if !["move_node", "insert_node", "delete_node", "set_handle"].contains(&op.as_str()) {
        return Err(invalid(format!(
            "未知的 edit_path 算子 {op}（已实现：move_node / insert_node / delete_node / set_handle）"
        )));
    }
    let loaded = load_path_object(ctx, &object_id)?;
    let subpath_index = pathdata_index(args, "subpath")?;
    if subpath_index >= loaded.subpaths.len() {
        return Err(invalid(format!(
            "subpath 下标 {subpath_index} 越界（对象 {object_id} 共 {} 条子路径）",
            loaded.subpaths.len()
        )));
    }
    let mut subpaths = loaded.subpaths.clone();
    let sub = &mut subpaths[subpath_index];
    let mut new_node: Option<usize> = None;
    let op_result = match op.as_str() {
        "move_node" => {
            let node = pathdata_index(args, "node")?;
            let to = pathdata_point(args, "to")?;
            yanshi_render::path::move_path_node(sub, node, to)
        }
        "insert_node" => {
            let segment = pathdata_index(args, "segment")?;
            let t = args.get("t").and_then(Value::as_f64).unwrap_or(0.5);
            match yanshi_render::path::insert_path_node(sub, segment, t) {
                Ok(index) => {
                    new_node = Some(index);
                    Ok(())
                }
                Err(reason) => Err(reason),
            }
        }
        "delete_node" => {
            let node = pathdata_index(args, "node")?;
            yanshi_render::path::delete_path_node(sub, node)
        }
        _ => {
            // set_handle
            let node = pathdata_index(args, "node")?;
            let side_name = require_str(args, "side")?;
            let Some(side) = yanshi_render::path::HandleSide::parse(&side_name) else {
                return Err(invalid(format!("side 只认 in / out，得到 {side_name}")));
            };
            let to = pathdata_point(args, "to")?;
            yanshi_render::path::set_path_handle(sub, node, side, to)
        }
    };
    op_result.map_err(|reason| invalid(format!("edit_path {op} 失败：{reason}")))?;

    // 复验整份几何（同一份校验）：编辑产物若不合法，宁可整单拒绝也不落库。
    let subpaths_json = yanshi_render::object::subpaths_to_json(&subpaths);
    let check = json!({"fill_rule": loaded.fill_rule.as_str(), "subpaths": subpaths_json});
    yanshi_render::object::parse_path_subpaths(&check)
        .map_err(|reason| invalid(format!("编辑后几何非法（已拒绝落库）：{reason}")))?;

    let (bx, by, bw, bh) =
        yanshi_render::object::path_control_bbox(&subpaths).unwrap_or((0.0, 0.0, 0.0, 0.0));
    let mut data = loaded.data.clone();
    let Some(geometry) = data.get_mut("geometry").and_then(Value::as_object_mut) else {
        return Err(YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!(
                "对象 {object_id} 的 data.geometry 不是对象，无法写回（数据形态异常）"
            )),
        ));
    };
    geometry.insert("subpaths".to_owned(), subpaths_json.clone());
    geometry.insert(
        "bbox".to_owned(),
        json!({"x": bx, "y": by, "w": bw, "h": bh}),
    );
    let result = ctx.commit(
        AtomKind::Supersede,
        json!({"object_id": object_id, "data": data}),
    )?;
    Ok(json!({
        "ok": true,
        "op": op,
        "object_id": object_id,
        "subpath": subpath_index,
        "new_node": new_node,
        "subpaths": subpaths_json,
        "d": yanshi_render::path::subpaths_to_svg_d(&subpaths),
        "bbox": {"x": bx, "y": by, "w": bw, "h": bh},
        "length": yanshi_render::path::subpaths_total_length(&subpaths),
        "node_counts": subpaths
            .iter()
            .map(|sub| sub.segments.len() + 1)
            .collect::<Vec<_>>(),
        "head": result.head_seq,
    }))
}
