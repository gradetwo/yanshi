//! **笔刷落笔工具**（`brush_stroke`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **注**：`brush_preview` 不在这里 —— 它与选区 helper 交错，留待后续。
//! 本模块的顶层项/方法/字段是 `pub(super)`（`tools.rs` 的 `#[cfg(test)]` 会引用）。

use super::*;

/// **真实传感器输入**（`brush_stroke` 的 `sensors` 参数）✓。
///
/// 此前宿主把 `stroke_to` 的 xtilt/ytilt 恒喂 0、引擎又把 `viewzoom` /
/// `barrel_rotation` 写死 0 ⇒ 19 个输入里 7 个是死的（C1 现状清单 §3 实测）。
/// 本结构把四路真实信号带进来：xtilt/ytilt 走 `stroke_to` 的既有参数，
/// viewzoom / barrel_rotation 走 `hokusai::Brush` 的宿主字段（不进 `.myb`）。
/// **缺省全 0 = 历史行为逐位一致**（引擎注释与回归测试守住）。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct BrushSensors {
    /// 笔尖倾斜 X 分量（-1..1，与引擎 `stroke_to` 的 xtilt 同口径）。
    pub xtilt: f32,
    /// 笔尖倾斜 Y 分量（-1..1）。
    pub ytilt: f32,
    /// 笔杆旋转（度，0..360）⇒ 引擎 `barrel_rotation` 输入。
    pub barrel_rotation_deg: f32,
    /// 视图缩放（`log(倍率)`，1 倍 = 0）⇒ 引擎 `viewzoom` 输入。
    pub viewzoom: f32,
}

/// 从 `sensors` 参数对象解析并校验（越界**报错**，不静默夹紧 —— 与 pressure 同纪律）。
pub(super) fn brush_sensors_from_value(value: &Value) -> Result<BrushSensors> {
    let Some(obj) = value.as_object() else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(
                "sensors 必须是对象 {xtilt, ytilt, twist_deg, viewzoom}（都可省略）".to_string(),
            ),
        ));
    };
    let field = |key: &str, lo: f64, hi: f64| -> Result<f32> {
        match obj.get(key) {
            None | Some(Value::Null) => Ok(0.0),
            Some(raw) => {
                let Some(v) = raw.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("sensors.{key} 必须是数字")),
                    ));
                };
                if !v.is_finite() || !(lo..=hi).contains(&v) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "sensors.{key} 必须在 {lo}..{hi}（给了 {v}）⇒ 越界不静默夹紧"
                        )),
                    ));
                }
                Ok(v as f32)
            }
        }
    };
    Ok(BrushSensors {
        xtilt: field("xtilt", -1.0, 1.0)?,
        ytilt: field("ytilt", -1.0, 1.0)?,
        barrel_rotation_deg: field("twist_deg", 0.0, 360.0)?,
        viewzoom: field("viewzoom", -8.0, 8.0)?,
    })
}

/// **`color` 里带了会被忽略的 alpha 吗（B4）**：对象式含 `a` 键，或数组 4 元。
/// （只认 `color.a` 这一处：`#RRGGBBAA` 的 alpha 是颜色值的一部分，不叫 `color.a`。）
fn color_has_alpha(value: &Value) -> bool {
    if let Some(object) = value.as_object() {
        return object.contains_key("a");
    }
    value.as_array().is_some_and(|items| items.len() == 4)
}

/// **`color.a` 折成的透明度系数** ✓（P-19：把 `a` 乘进有效 opacity ✓）。
///
/// **口径与颜色解析一致** ✓：
/// * 对象式 `{a}` 按 0..255 字节（与 `parse_colour_value` 同款；浮点 `a` 解析器同样不认 ✓）；
/// * 数组 4 元按 `parse_spec_color` 的量级切换：任一分量 > 1 ⇒ 字节域
///   （`a == 1.0` 例外按不透明，第 948 轮的陷阱同款处理 ✓）；全 ≤ 1 ⇒ 线性域，`a` 直接是 0..1；
/// * `color_space: "linear"` 时数组不做量级切换，`a` 直接是 0..1（与
///   `parse_spec_color_with_space` 的 Linear 分支同款 ✓）。
/// * hex 写法（`#RRGGBBAA`）**不在这里** ✓：它的 alpha 仍按 P2-8 只警告、不生效 ✓，
///   别把两个口径混在一起 ✓。
/// * 无 `a` ⇒ `None`（不碰 opacity ✓）；`a = 255` / `1.0` ⇒ `Some(1.0)` ⇒ 调用方跳过设置 ✓
///   （与修前逐位相同，连 `.myb` 里 `opaque` 随压力的曲线都不动 ✓）。
fn color_alpha_factor(
    value: &Value,
    space: Option<yanshi_render::color::ColorSpace>,
) -> Option<f64> {
    if let Some(object) = value.as_object() {
        let a = object.get("a")?.as_u64()?;
        return Some(a.min(255) as f64 / 255.0);
    }
    let items = value.as_array()?;
    if items.len() != 4 {
        return None;
    }
    let numbers: Vec<f64> = items.iter().filter_map(Value::as_f64).collect();
    if numbers.len() != 4 {
        return None;
    }
    let a = numbers[3];
    if space == Some(yanshi_render::color::ColorSpace::Linear) {
        return Some(a.clamp(0.0, 1.0));
    }
    if numbers.iter().any(|component| *component > 1.0) {
        if a == 1.0 {
            Some(1.0)
        } else {
            Some(a.clamp(0.0, 255.0) / 255.0)
        }
    } else {
        Some(a.clamp(0.0, 1.0))
    }
}

/// **画一段并提交** ✓（反馈期 4：涂抹预算闸自动拆分的落点 ✓）。
///
/// 原先 `write_brush_stroke` 里“单次 `paint_brush` → 单次提交”的一段，原样抽出：
/// 选区裁剪、位图对称合并、blob 落库、`write_import_image` 提交全在里面 ⇒
/// 单笔与拆分段走**同一段代码**，不存在“拆分专用落笔路”与主路漂移的可能 ✓。
/// 调用方责任：
/// * 每段提交前必须重设 `ctx.pending_group_id`（`commit` 里 `take()` 只取走一次 ⇒
///   不重设就只有首段带组）；本函数在提交前统一设成 `seg_group_id`（`Some` 时），
///   单笔调用传 `None` ⇒ 沿用主函数开头已设的调用方 `group_id`，行为一位不动 ✓。
/// * `seg_points_source` 记该段的**落笔后点列**（拆分点列是变换后的最终点列，
///   无法可靠映回变换前控制点；`smooth` 记 `seg_smooth`，拆分段一律 `false` ⇒
///   重放时不再对已平滑点列二次平滑，见 `write_brush_stroke` 的拆分分支注释）。
#[allow(clippy::too_many_arguments)]
fn paint_and_commit_brush_segment(
    ctx: &mut ToolContext<'_>,
    args: &Value,
    seg_points: &[(f64, f64, f64)],
    seg_dtimes: &[f64],
    seg_points_source: &Value,
    seg_times_source: &Value,
    seg_smooth: bool,
    seg_glaze_opacity: Option<f64>,
    seg_brush_params: Option<&Value>,
    seg_color_mixing: yanshi_render::color::ColorMixing,
    seg_sensors: BrushSensors,
    seg_color_space: Option<yanshi_render::color::ColorSpace>,
    seg_symmetry: yanshi_render::Symmetry,
    seg_group_id: Option<&str>,
    seg_force_fresh_object: bool,
) -> Result<BrushSegmentPainted> {
    // **裁剪累计量是每段自己的** ✓（主函数不再持有跨段累计；聚合在调用方做）。
    let mut clipped_away = 0usize;
    let mut clip_coverage: Option<f64> = None;
    // **与主函数同口径的来源回显** ✓（全从 `args` 原样取，段间无差别 ⇒ 不作参数）。
    let seg_brush_name = require_str(args, "brush")?;
    let layer_id = require_str(args, "layer_id")?;
    let size_source = args.get("size").cloned().unwrap_or(Value::Null);
    let color_source = args.get("color").cloned().unwrap_or(Value::Null);
    let color_to_source = args.get("color_to").cloned().unwrap_or(Value::Null);
    let smooth_source = json!(seg_smooth);
    let opacity_source = args.get("opacity").cloned().unwrap_or(Value::Null);
    let hardness_source = args.get("hardness").cloned().unwrap_or(Value::Null);
    let symmetry_name = optional_str(args, "symmetry").unwrap_or_else(|| "none".to_owned());
    let sensors_source = args.get("sensors").cloned().unwrap_or(Value::Null);
    // **组 id 每提交必重设** ✓（`commit` 的 `take()` 语义，见本函数文档）：
    //   拆分段传 `Some` ⇒ 每段都带同一组；单笔传 `None` ⇒ 不动主函数已设的值。
    if let Some(group_id) = seg_group_id {
        ctx.pending_group_id = Some(group_id.to_owned());
    }
    let paint = paint_brush(
        ctx,
        &seg_brush_name,
        seg_points,
        args.get("size").and_then(Value::as_f64),
        args.get("color"),
        args.get("color_to"),
        seg_glaze_opacity,
        args.get("hardness").and_then(Value::as_f64),
        // **文档里必须喂底图** ✓ —— 涂抹类笔刷靠它工作 ✓。
        true,
        // **★ P0 #4：把调用方给的字段覆盖传进去 ✗ ★**（**∴ 我第一版补成了 `None` ✗ ⇒ ①② 假绿 ✓）★
        seg_brush_params,
        seg_color_mixing,
        seg_dtimes,
        None,
        seg_sensors,
        seg_color_space,
    )?;
    let BrushPaint {
        name,
        x0,
        y0,
        width,
        height,
        mut rgba,
        steps,
        painted,
        max_delta,
        delta_distribution,
        fed_base,
        short_stroke_hint,
    } = paint;
    // **blob 先行** ✓，与 `medium_stroke` 完全同路 ✓。
    // **选区裁剪**（需求 P0-3）：不在选区内的像素**不许进图层** ⇒ 与"引擎没碰过"同等对待 ✓
    // —— 与 `brush.rs:515` 的注释一字不差："覆盖度 ≤ 0 的印章**直接跳过**" ✓。
    // **位置为什么在这里** ✓：这是 `paint_brush` 返回值的解构点之后、落库之前 ✓ ⇒
    // `x0/y0/width/height/rgba` **必然已在作用域** ✓（前三次我把循环插进函数内部 ✗，三次都落在声明之前 ✗）。
    // **落笔路径的最后一段光栅工作** ✓（P2）：选区裁剪 ＋ blob 落库（CAS 写入 ✓）——
    // 报告里的模型把这段与"dab 生成"混在一起 ✓，现在分开算 ✓。
    let composite_started = std::time::Instant::now();
    // **★ 统一实现（第 1195 轮，外部 §164 问题②）**：
    //   「`clip_to_selection` 缺省或空串 ⇒ 不裁剪」必须**一处改、全部受益** ✓
    //   ⇒ 原来 `optional_str` 对 **空串**返回 `Some("")`（不是 None）⇒
    //     任何调用者传了空串（`unwrap_or_default()` 的常见写法）都会去找 `id=""` 的选区
    //     ⇒ 抛 `ReferenceNotFound` ⇒ **整批笔触全失败** ✗（7 个 smart_* 都中过）
    //   ⇒ 所以在**唯一入口**过滤空串 ⇒ 调用者不必各自记得 ✓
    if let Some(id) = optional_str(args, "clip_to_selection").filter(|s| !s.trim().is_empty()) {
        let state = document_state(ctx)?;
        // Q4 快路径：`smart_*` 预热的缓存命中时，直接从缓存网格采样，
        // 跳过下面的慢速光栅化（document_state 拉取 + 形状光栅化 + 羽化模糊）。
        // （"latest" 在预热时已解析为具体 id；"combined" 不进缓存，走慢路径。）
        let fast_mask: Option<std::sync::Arc<PrecomputedClipMask>> = ctx
            .cached_clip_mask
            .clone()
            .and_then(|(cid, m)| if cid == id { Some(m) } else { None });
        // ②(b)：组合选区裁剪。当 id 为 "combined"，或为 "latest"/"1"/"true"/"all"
        // 且存在 Add/Subtract/Intersect 组合链时，用渲染层现成的 SelectionSet
        // 做逐像素组合覆盖度（每个子选区有 bbox 快速剔除，2~3 个选区仅 +2ns）。
        // 单选区或无组合链时走原有的单选区 O(1) 路径，零性能影响。
        let want_combined = id.as_str() == "combined" || {
            let mut count = 0usize;
            let mut has_chain = false;
            for s in state.selections.values().filter(|s| !s.is_deleted()) {
                count += 1;
                if s.mode != "new" {
                    has_chain = true;
                }
            }
            matches!(id.as_str(), "1" | "true" | "latest" | "all") && count >= 2 && has_chain
        };
        let combined_set: Option<yanshi_render::selection::SelectionSet> = if want_combined {
            let mut ordered: Vec<_> = state
                .selections
                .values()
                .filter(|s| !s.is_deleted())
                .collect();
            ordered.sort_by(|a, b| a.created_by.as_str().cmp(b.created_by.as_str()));
            let mut set = yanshi_render::selection::SelectionSet::new();
            for s in ordered {
                set.push(yanshi_render::selection::SelectionShape::from_selection(s));
            }
            Some(set)
        } else {
            None
        };
        let alive: Vec<(String, yanshi_core::Bbox)> = state
            .selections
            // ★ **必须用 map 的 key（= `selection_id`），不能用 `selection.id`** ★
            //   `fold.rs:1146` 是 `insert(selection_id, selection)` ⇒ key 是 `selection_id`
            //   而 `selection.id` 是**另一个值** ⇒ 拿它去 `state.selections.get()` **必然 None**
            //   ⇒ 于是 `sel=None` ⇒ `shape_kind` 退回 `"rect"` ⇒ `rect_coverage_clipped`
            //     ⇒ **bbox 内全选 ⇒ 位图选区的洞也被涂** ✗（第 911 轮，外部 §142 的现象）
            .iter()
            .filter(|(_, selection)| !selection.is_deleted())
            .filter_map(|(selection_id, selection)| {
                // 第一层：缺 bbox 的选区（纯点列多边形 ✓）不许被丢掉 ✓
                let owned_bbox;
                let bbox = match selection.shape.get("bbox") {
                    Some(bbox) => bbox,
                    None => {
                        // ★ rect／ellipse 的形状是 x/y/width/height 或 cx/cy/rx/ry，**没有 points** ✗ ★
                        // ⇒ 原来这里只读 `points` ⇒ `?` 静默返回 None ⇒ filter_map 丢掉该选区 ✗
                        // ⇒ **∴ 于是 clip 报"找不到选区 ⇒ 现有：（一个都没有）"** ✗（Muse 的 §13「幽灵选区」✓）
                        // ⇒ **∴ 这里先把 rect／ellipse 的矩形算出来 ✓，再退回 points ✓**。
                        let num = |k: &str| selection.shape.get(k).and_then(Value::as_f64);
                        // ★ **`create_selection` 存的是 `w`/`h`，不是 `width`/`height`** ★
                        //   ⇒ 原来只读 `width`/`height` ⇒ **永远 None** ⇒ rect 选区被 filter_map 丢掉 ✗
                        //   ⇒ 于是 `brush_stroke(clip_to_selection="<rect 选区>")` 报
                        //     「找不到可裁剪的选区 ⇒ 现有：（一个都没有）」✗（第 901 轮实测）
                        //   ⇒ 修：**先读 `w`/`h`（实际格式 ✓），再退回 `width`/`height`（兼容旧写法 ✓）**
                        let wh = |k1: &str, k2: &str| num(k1).or_else(|| num(k2));
                        if let (Some(x), Some(y), Some(w), Some(h)) =
                            (num("x"), num("y"), wh("w", "width"), wh("h", "height"))
                        {
                            owned_bbox = json!({"x": x, "y": y, "w": w, "h": h});
                            &owned_bbox
                        } else if let (Some(cx), Some(cy), Some(rx), Some(ry)) =
                            (num("cx"), num("cy"), num("rx"), num("ry"))
                        {
                            owned_bbox =
                                json!({"x": cx - rx, "y": cy - ry, "w": rx * 2.0, "h": ry * 2.0});
                            &owned_bbox
                        } else if selection.shape.get("kind").and_then(Value::as_str)
                            == Some("bitmap")
                        {
                            // **★ 第 6 条：`bitmap` 选区 ★**（第 1606 轮；外部 `E1742` 收窄后的真因）
                            //   **⇒ 现象**：`create_mask(shape.kind=bitmap)` 成功 ✓、
                            //     `create_selection(from_mask=…)` 成功 ✓、`list_selections` **也列得出**（`shape = {kind:"bitmap", blob_hash}`）✓
                            //     **而 `brush_stroke(clip_to_selection="s1")` 报「找不到可裁剪的选区」** ✗
                            //   **⇒ ⇒ 根因与 `B1` 同族**：`alive` 的 bbox 推导**没有 `bitmap` 分支**
                            //     （它只有 `blob_hash`）⇒ `?` ⇒ 被 `filter_map` 丢掉 ✓
                            //   **⇒ ⇒ ⇒ 所以**：**位图掩膜的 bbox 取自**掩膜 blob 的尺寸**
                            //     （`kind: bitmap` 形如 `{blob_hash, width?, height?}`；
                            //      没给尺寸时**退回整幅**，因为位图掩膜按约定覆盖全画布）✓
                            let num = |k: &str| selection.shape.get(k).and_then(Value::as_f64);
                            let (w, h) = (num("width"), num("height"));
                            match (w, h) {
                                (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                                    owned_bbox = json!({"x": 0.0, "y": 0.0, "w": w, "h": h});
                                }
                                _ => {
                                    owned_bbox = json!({
                                        "x": 0.0, "y": 0.0,
                                        "w": state.width as f64, "h": state.height as f64,
                                    });
                                }
                            }
                            &owned_bbox
                        } else if selection.shape.get("subpaths").is_some()
                            || selection.shape.get("d").is_some()
                        {
                            // **第 5 条：路径选区** ✓（第 1549 轮；外部 `B1`）
                            //   **⇒ 为什么需要**：上面四条（bbox／x,y,w,h／cx,cy,rx,ry／points）
                            //     **都取不到** ⇒ `?` ⇒ `filter_map` 丢掉 ⇒ `alive` 空 ⇒
                            //     报「找不到可裁剪的选区 ⇒ 现有一个都没有」✗
                            //   **⇒ ⇒ 两种落库形态**：`create_selection` 把 `d` **归一成 `subpaths`**
                            //     （**不留 `d`**）⇒ 所以**两种都要认** ✓
                            let geometry: Option<Value> =
                                if let Some(subpaths) = selection.shape.get("subpaths") {
                                    Some(json!({
                                        "subpaths": subpaths,
                                        "fill_rule": selection.shape.get("fill_rule")
                                            .cloned()
                                            .unwrap_or_else(|| json!("evenodd")),
                                    }))
                                } else {
                                    selection
                                        .shape
                                        .get("d")
                                        .and_then(Value::as_str)
                                        .and_then(|d| crate::tools::parse_svg_d(d).ok())
                                        .map(|subpaths| json!({"subpaths": subpaths}))
                                };
                            let parsed = geometry.and_then(|g| {
                                yanshi_render::object::parse_path_subpaths(&g).ok()
                            })?;
                            let (ax, ay, bx, by) =
                                yanshi_render::object::path_control_bbox(&parsed.subpaths)?;
                            owned_bbox = json!({"x": ax, "y": ay, "w": bx - ax, "h": by - ay});
                            &owned_bbox
                        } else {
                            let points = selection.shape.get("points")?.as_array()?;
                            let (mut ax, mut ay) = (f64::INFINITY, f64::INFINITY);
                            let (mut bx, mut by) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                            for point in points {
                                let pair = point.as_array()?;
                                let x = pair.first()?.as_f64()?;
                                let y = pair.get(1)?.as_f64()?;
                                ax = ax.min(x);
                                ay = ay.min(y);
                                bx = bx.max(x);
                                by = by.max(y);
                            }
                            if !ax.is_finite() || !bx.is_finite() {
                                return None;
                            }
                            owned_bbox = json!({"x": ax, "y": ay, "w": bx - ax, "h": by - ay});
                            &owned_bbox
                        }
                    }
                };
                // **`bbox` 有两种写法** ✗：对象 `{x,y,w,h}` ✓ 与**数组 `[x,y,w,h]`** ✓
                //   （`create_selection{from_mask}` 写的就是**数组** ✓）
                //   ⇒ 原来这里只 `bbox.get("x")` ✗ ⇒ **数组读出 (0,0,0,0)** ⇒ 裁剪框**空**
                //   ⇒ **位图选区做裁剪时一个像素都不落**（`ok:true`、`painted_pixels_clipped=0` ✗，
                //     实测第 2665 轮：`clip_coverage=0.0`、选区 `area=0.0` ✓）
                //   ⇒ 现在**先用 `Bbox::from_value`（两种都认 ✓）**，失败再退回对象字段 ✓。
                // **对象与数组两种 `bbox` 都要认** ✓：`create_selection{from_mask}` 写的是**数组** ✓，
                //   而 `Bbox::from_value` **只收对象** ⇒ 数组会退化成 `(0,0,0,0)`（裁剪框空 ✗）。
                //   ⚠️ **但这一处**在本轮实测里**不是**关键路径（把数组分支去掉，判据照样绿 ✓）——
                //   真正让位图选区落墨的是 `parse_geom` 认 `blob_hash` ✓（见台账「三百七十八」）。
                //   保留它是**防御性**的：任何走 `alive` 的路径拿到数组 bbox 时都不会再退化成 0 ✓。
                let parsed = bbox
                    .as_array()
                    .and_then(|values| {
                        Some(yanshi_core::Bbox::new(
                            values.first()?.as_f64()?,
                            values.get(1)?.as_f64()?,
                            values.get(2)?.as_f64()?,
                            values.get(3)?.as_f64()?,
                        ))
                    })
                    .or_else(|| yanshi_core::Bbox::from_value(bbox));
                let number = |key: &str| bbox.get(key).and_then(Value::as_f64).unwrap_or(0.0);
                let bbox_value = match parsed {
                    Some(value) => value,
                    None => {
                        yanshi_core::Bbox::new(number("x"), number("y"), number("w"), number("h"))
                    }
                };
                Some((selection_id.clone(), bbox_value))
            })
            .collect();
        let wanted = matches!(id.as_str(), "1" | "true" | "latest" | "all");
        // ②(b)："combined" 不找名字叫 combined 的选区，直接用组合集。
        let picked = if combined_set.is_some() && id.as_str() == "combined" {
            // 占位：组合路径不需要单个选区；alive 非空已由 want_combined 保证。
            alive.last().cloned()
        } else if wanted {
            alive.last().cloned()
        } else {
            alive.iter().find(|(name, _)| name == &id).cloned()
        };
        let Some((name, bbox)) = picked else {
            return Err(YanshiError::new(
                ErrorCode::ReferenceNotFound,
                ErrorContext::detail(format!(
                    "找不到可裁剪的选区 {id} ⇒ 现有：{}（也可用 \"latest\" 取最新那个 ✓）",
                    if alive.is_empty() {
                        "（一个都没有 ✓）".to_string()
                    } else {
                        alive
                            .iter()
                            .map(|(name, _)| name.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )),
            ));
        };
        let clip = yanshi_core::Bbox::new(x0 as f64, y0 as f64, width as f64, height as f64);
        // 第二层：覆盖度按形状分派 ✓（原来硬编码矩形 ✗）
        // ★ 取完整 Selection：shape 只有几何，feather/invert 在 struct 顶层字段；
        // 只读 shape 会静默丢弃二者（mode 对单个选区裁剪无意义，故不处理）。
        // ②(b)：组合路径时跳过单选区网格构建（逐像素走 SelectionSet）。
        // Q2：选区带 paint_mask 增量（patches 非空）时，用 CompiledMask 求值
        // （「几何底座 + patch 链」语义；SelectionShape 不读 patches）。
        // CompiledMask 已内含 invert/feather，故跳过后面的二次处理。
        // Q4 快路径：缓存命中时直接采样（已含全部语义），跳过慢速光栅化。
        let coverage = if let Some(mask) = fast_mask {
            let gw = width.max(1) as u32;
            let gh = height.max(1) as u32;
            let mut data = vec![0.0f32; (gw * gh) as usize];
            for (i, v) in data.iter_mut().enumerate() {
                let px = x0 + (i % gw as usize) as i32;
                let py = y0 + (i / gw as usize) as i32;
                let cx = px - mask.x0;
                let cy = py - mask.y0;
                *v = if cx >= 0 && cy >= 0 && (cx as u32) < mask.width && (cy as u32) < mask.height
                {
                    mask.data[cy as usize * mask.width as usize + cx as usize]
                } else {
                    0.0
                };
            }
            Some(yanshi_render::geometry::Coverage {
                bbox: clip,
                width: gw,
                height: gh,
                data,
            })
        } else if combined_set.is_none() {
            let sel = state.selections.get(&name);
            let (coverage, skip_post) = if let Some(sel) = sel.filter(|s| !s.patches.is_empty()) {
                let load_blob = |h: &str| {
                    h.parse::<yanshi_core::BlobHash>()
                        .ok()
                        .and_then(|hash| ctx.workspace.store().get(&hash).ok())
                };
                let compiled = yanshi_render::selection::CompiledMask::build(
                    sel,
                    state.width as i32,
                    state.height as i32,
                    &load_blob,
                );
                let gw = width.max(1) as u32;
                let gh = height.max(1) as u32;
                let mut data = vec![0.0f32; (gw * gh) as usize];
                for (i, v) in data.iter_mut().enumerate() {
                    let px = x0 + (i % gw as usize) as i32;
                    let py = y0 + (i / gw as usize) as i32;
                    *v = compiled.sample(px, py);
                }
                (
                    yanshi_render::geometry::Coverage {
                        bbox: clip,
                        width: gw,
                        height: gh,
                        data,
                    },
                    true,
                )
            } else {
                let shape = sel.map(|s| s.shape.clone()).unwrap_or(Value::Null);
                let shape_kind = shape.get("kind").and_then(Value::as_str).unwrap_or("rect");
                let shape_points: Vec<(f64, f64)> = shape
                    .get("points")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| {
                                let pair = item.as_array()?;
                                Some((pair.first()?.as_f64()?, pair.get(1)?.as_f64()?))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let coverage = match shape_kind {
                    "polygon" if shape_points.len() >= 3 => {
                        yanshi_render::geometry::polygon_coverage_clipped(&shape_points, 4, &clip)
                    }
                    "ellipse" => yanshi_render::geometry::ellipse_coverage_clipped(bbox, 4, &clip),
                    "bitmap" => {
                        // 位图选区（`smart_select`）：先确保 blob 已解码注册，再逐像素查表。
                        // 未注册成功时覆盖率全 0（宁可不落笔，不可错落笔）。
                        let gw = width.max(1) as u32;
                        let gh = height.max(1) as u32;
                        let mut data = vec![0.0f32; (gw * gh) as usize];
                        if let Some(sel) = sel {
                            let failed = yanshi_render::selection::ensure_bitmap_masks_registered(
                                &[&sel.shape],
                                &|h| {
                                    h.parse::<yanshi_core::BlobHash>()
                                        .ok()
                                        .and_then(|hash| ctx.workspace.store().get(&hash).ok())
                                },
                            );
                            // **解不开就响亮报错** ✓（第 2665 轮：原来静默 ⇒ `ok:true` 而一个像素不落 ✗）
                            if let Some(blob) = failed.first() {
                                return Err(YanshiError::new(
                                    ErrorCode::InvalidArgument,
                                    ErrorContext::detail(format!(
                                        "位图选区的蒙版 blob 解不开：{blob} ⇒ 支持 **8 位灰度 PNG**、**RGBA PNG（取 alpha）**、**raw RGBA（w×h×4，取 alpha）**、**raw L8（w×h）**；raw 形态必须在 shape 里给 `width`/`height` ✓ —— 以前这里**静默**当成「什么都没选」⇒ `ok:true` 而一个像素不落 ✗"
                                    )),
                                ));
                            }
                            let shape =
                                yanshi_render::selection::SelectionShape::from_selection(sel);
                            for (i, v) in data.iter_mut().enumerate() {
                                let px = x0 as f64 + (i % gw as usize) as f64;
                                let py = y0 as f64 + (i / gw as usize) as f64;
                                *v = shape.coverage(px, py);
                            }
                        }
                        yanshi_render::geometry::Coverage {
                            bbox: clip,
                            width: gw,
                            height: gh,
                            data,
                        }
                    }
                    // **路径选区**不许当成矩形** ✓**（第 1549 轮；外部 `B1` 的**真正**根因）
                    //   **⇒ 症状**：`bbox` 装不下就报"找不到"。而**找到了**之后会**当成矩形** ✗
                    //     ⇒ 于是 `clip_to_selection` 用**整个 bbox** 裁剪（**选多了**，不是选少了）
                    //   **⇒ ⇒ 所以我加 `path` 分支**：走与 `draw_shape` **同一份** `path_coverage_clipped` ✓
                    //     ⇒ **控制点之外的部分不再被误覆盖** ✓
                    //   **⇒ ⇒ ⇒ `subpaths` 与 `d` 两种落库形态都认**（`create_selection` 归一成 `subpaths`）✓
                    "path" => {
                        // **先拿到 `subpaths`（已有或从 `d` 解析），再交给与 `draw_shape`
                        // 同一份的 `path_coverage_clipped`** ✓
                        // 任何一步失败 ⇒ **退回 bbox 矩形**（**保持旧行为，不静默变成"不落墨"**）✓
                        let geometry: Option<Value> = if let Some(subpaths) = shape.get("subpaths")
                        {
                            Some(json!({
                                "subpaths": subpaths,
                                "fill_rule": shape.get("fill_rule")
                                    .cloned()
                                    .unwrap_or_else(|| json!("evenodd")),
                            }))
                        } else {
                            shape
                                .get("d")
                                .and_then(Value::as_str)
                                .and_then(|d| crate::tools::parse_svg_d(d).ok())
                                .map(|subpaths| json!({"subpaths": subpaths}))
                        };
                        let fallback =
                            || yanshi_render::geometry::rect_coverage_clipped(bbox, &clip);
                        match geometry
                            .and_then(|g| yanshi_render::object::parse_path_subpaths(&g).ok())
                        {
                            Some(parsed) => yanshi_render::geometry::path_coverage_clipped(
                                &parsed.subpaths,
                                4,
                                &clip,
                                parsed.fill_rule,
                            ),
                            None => fallback(),
                        }
                    }
                    _ => yanshi_render::geometry::rect_coverage_clipped(bbox, &clip),
                };
                (coverage, false)
            };
            // ★ 应用之前被静默丢弃的 invert/feather（与蒙版路径 render.rs apply_layer_mask 一致：
            // 反选逐像素取反；羽化对方框模糊，半径取 feather/2）。
            // Q2：CompiledMask 路径（skip_post）已内含二者，跳过。
            let mut coverage = coverage;
            if !skip_post {
                if let Some(sel) = sel {
                    if sel.invert {
                        for v in coverage.data.iter_mut() {
                            *v = 1.0 - *v;
                        }
                    }
                    if sel.feather > 0.0 {
                        let radius = (sel.feather / 2.0).round().max(1.0) as u32;
                        box_blur_coverage(&mut coverage, radius);
                    }
                }
            }
            Some(coverage)
        } else {
            None
        };
        // **这一笔的选区是否反选** ✓（第 1550 轮；`B2`：网格外默认值要用它）
        let invert_selection = state
            .selections
            .get(&name)
            .map(|s| s.invert)
            .unwrap_or(false);
        let mut dropped = 0usize;
        // **裁剪一共吃掉多少像素**（跨选区累计，供 `painted_pixels_clipped` 用）✓
        //   **⇒ 外部 `B6`**：`painted_pixels` 是**裁剪前**的 ⇒ 用它算覆盖率会得出
        //     「笔触都落上了」的错觉（他们实测：选区 166 万像素，累计报 793 万）✗
        for index in (0..rgba.len()).step_by(4) {
            let pixel = index / 4;
            let step = width.max(1);
            let document_x = f64::from(x0) + (pixel % step) as f64;
            let document_y = f64::from(y0) + (pixel / step) as f64;
            // ②(b)：组合选区时逐像素查 SelectionSet（各形状自带 mode/invert/feather）；
            // 否则走原有的单选区 Coverage 网格采样。
            let inside = if let Some(set) = combined_set.as_ref() {
                f64::from(set.coverage(document_x, document_y)).clamp(0.0, 1.0)
            } else if let Some(coverage) = coverage.as_ref() {
                // `Coverage` 是**裸掩码**（`bbox`/`width`/`height`/`data: Vec<f32>` ✓）⇒ 直接按下标取样 ✓
                //（`coverage(x,y)` 那个方法是 **`SelectionShape`** 的 ✗ —— 我一开始把它当成了本类型的 ✓）。
                let column = (document_x - coverage.bbox.x).floor();
                let row = (document_y - coverage.bbox.y).floor();
                let inside_bounds = column >= 0.0
                    && row >= 0.0
                    && column < coverage.width as f64
                    && row < coverage.height as f64;
                if inside_bounds {
                    f64::from(
                        coverage.data[row as usize * coverage.width as usize + column as usize]
                            .clamp(0.0, 1.0),
                    )
                } else {
                    // **覆盖网格外**的默认值取决于**反选*** ✗→✓（第 1550 轮；外部 `B2`）
                    //   **⇒ 症状**：`create_selection(invert=true)` + `clip_to_selection`
                    //     ⇒ **`ok=true` 但一个像素都不落** ✗（外部两次独立实验都是 0）
                    //   **⇒ ⇒ 根因**：覆盖网格**只覆盖选区自己的 `bbox`**（`90..210`）
                    //     ⇒ 而笔触横贯 `20..280` ⇒ **大部分像素在网格外** ⇒ 这里一律给 `0`
                    //     ⇒ 于是"网格外"被当成"未覆盖" ⇒ **反选恰恰要求那里是 1** ✗✗
                    //   **⇒ ⇒ ⇒ 所以**：**反选时网格外 = 1（覆盖）**；**不反选时网格外 = 0** ✓
                    //     这正是 `coverage()` 里 `if self.invert { 1.0 - base }` 的同一套语义 ✓
                    if invert_selection {
                        1.0
                    } else {
                        0.0
                    }
                }
            } else {
                1.0
            };
            if inside <= 0.0 {
                // **只数真正落过墨的像素** ✓（A3）：`painted` 数的是补丁里**任一通道非零**的像素 ⇒
                //   被裁掉的透明像素本来就没画，数进来会让 `painted_pixels_clipped = painted - dropped`
                //   偏小（实测：半裁一笔报 0.32 而不是 ~0.5 ✗，差的正是裁掉区域里的透明像素 ✓）。
                if rgba[index] != 0
                    || rgba[index + 1] != 0
                    || rgba[index + 2] != 0
                    || rgba[index + 3] != 0
                {
                    dropped += 1;
                }
                rgba[index] = 0;
                rgba[index + 1] = 0;
                rgba[index + 2] = 0;
                rgba[index + 3] = 0;
            } else if inside < 1.0 {
                rgba[index + 3] = (f64::from(rgba[index + 3]) * inside)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
        }
        eprintln!("  选区 {name} 裁剪：清掉 {dropped} 个选区外像素 ✓");
        clipped_away += dropped;
        // **记下"这一笔在选区内的覆盖率"** ✓（第 1567 轮；外部的方法学建议）
        //   **⇒ 他们的原话**：「**检查中间产物**比检查最终产物更有效」⇒ 因为最终产物里
        //     **错误会被后续步骤掩盖** ⇒ 而"**让中间状态变成可见的字段**"是同一个思路 ✓
        //   **⇒ ⇒ 而 `B1`/`B2` 那次正是**中间状态不可见**：`ok=true`、`painted_pixels=5438`
        //     ⇒ **而实际墨 = 0** ⇒ **我花了两轮才看出"覆盖度是 0"** ✗
        //   **⇒ ⇒ ⇒ 所以**：把"**没有被裁掉的像素占比**"报出来 ⇒ **0.0 就是"全被裁掉了"** ✓
        //     ⇒ 调用方**一眼**就知道，而不必去采样或看图 ✓
        let kept = painted.saturating_sub(dropped) as f64;
        let total = painted.max(1) as f64;
        clip_coverage = Some(kept / total);
    }
    // **★★★ 对称：按**位图**镜像并合并** ✗ ★★★**（**第 251 轮 ✓）
    //   ＋ **∴ 做法 ✗**：**把这一笔自己的位图**翻转**✗ 摆到**镜像后的位置** ✓
    //     ＋ **∴ 两半重叠时（**笔迹跨过中线 ✓）** ⇒ **∴ 逐像素取**alpha 较大**的那个**
    //       （**∴ 不是相加 ✗ 否则中线上会**双倍变深** ✓ —— **∴ 与 Krita 的观感一致 ✓）★
    //   ＋ **∴ 为什么在**位图**这一层做 ✗**：**① 它**只含这一笔**（**不碰图层已有内容 ✓）✗
    //     **② 不碰 `points` ⇒ **不会连成一条线**（**上面那条实测教训 ✓）✗
    //     **③ 与笔刷种类无关**（**纹理／抖动笔刷的镜像就是**翻转**✓ —— 正是对称的语义 ✓）★★
    // **∴ 注意类型 ✗**：`paint_brush` 回来的 `x0/y0/width/height` 是 **`usize`** ✓
    //   ⇒ **∴ 与画布尺寸做算术时**统一用 `i64`**✗ 避免符号/宽度混乱** ✓（**实测：混用报 E0308 ✓）★
    let (mut rgba, mut x0, mut y0, mut width, mut height) = (rgba, x0, y0, width, height);
    if seg_symmetry != yanshi_render::Symmetry::None {
        let (doc_w, doc_h) = ctx
            .workspace
            .document(&ctx.doc_id)
            .map(|document| {
                (
                    document.state().width as i64,
                    document.state().height as i64,
                )
            })
            .unwrap_or((0, 0));
        if doc_w > 0 && doc_h > 0 {
            let (x0i, y0i) = (x0 as i64, y0 as i64);
            let (wi, hi) = (width as i64, height as i64);
            let (mirror_x0, mirror_y0) = match seg_symmetry {
                yanshi_render::Symmetry::Vertical => (doc_w - (x0i + wi), y0i),
                yanshi_render::Symmetry::Horizontal => (x0i, doc_h - (y0i + hi)),
                yanshi_render::Symmetry::None => (x0i, y0i),
            };
            let nx0 = x0i.min(mirror_x0);
            let ny0 = y0i.min(mirror_y0);
            let nx1 = (x0i + wi).max(mirror_x0 + wi);
            let ny1 = (y0i + hi).max(mirror_y0 + hi);
            let nw = usize::try_from((nx1 - nx0).max(0)).unwrap_or(0);
            let nh = usize::try_from((ny1 - ny0).max(0)).unwrap_or(0);
            if nw > 0 && nh > 0 {
                let mut merged = vec![0u8; nw * nh * 4];
                // **∴ 贴一块位图（**可选翻转 ✓）✗ alpha 较大者胜** ✓
                let mut paste =
                    |src: &[u8], sw: usize, sh: usize, at_x: i64, at_y: i64, flip: bool| {
                        for row in 0..sh {
                            for col in 0..sw {
                                let src_col = if flip { sw - 1 - col } else { col };
                                let si = (row * sw + src_col) * 4;
                                if si + 3 >= src.len() {
                                    continue;
                                }
                                let a = src[si + 3];
                                if a == 0 {
                                    continue;
                                }
                                let dx = at_x - nx0 + col as i64;
                                let dy = at_y - ny0 + row as i64;
                                if dx < 0 || dy < 0 || dx >= nw as i64 || dy >= nh as i64 {
                                    continue;
                                }
                                let di = (dy as usize * nw + dx as usize) * 4;
                                if a >= merged[di + 3] {
                                    merged[di..di + 4].copy_from_slice(&src[si..si + 4]);
                                }
                            }
                        }
                    };
                let vertical = seg_symmetry == yanshi_render::Symmetry::Vertical;
                paste(&rgba, width, height, x0i, y0i, false);
                paste(&rgba, width, height, mirror_x0, mirror_y0, vertical);
                rgba = merged;
                // **∴ `x0/y0` 是 **`i32`**✗ 而 `width/height` 是 `usize`** ✓
                //   ⇒ **∴ 各按各的类型回转** ✓（**实测：混用会 E0308 ✓）★
                x0 = i32::try_from(nx0).unwrap_or(0);
                y0 = i32::try_from(ny0).unwrap_or(0);
                width = nw;
                height = nh;
            }
        }
    }
    let composite = ctx.workspace.store().put(&rgba)?;
    ctx.time(Phase::Raster, composite_started);
    let import_args = json!({
        // **来源参数** ✓（`update_stroke` 靠它重跑 ✓）：**原始控制点** ✓（不是加密后的 ✓）、
        // 当时给的 size / color / smooth ✓ —— 都是**调用方给的原话** ✓，不是我们的中间量 ✓。
        "source": {
            "kind": "brush",
            "brush": name,
            // **`brush_params` 也要进 `source`**（X-14）：落笔时合并后的 dab 密度等覆盖
            //（意图参数展开 + 显式 `brush_params`，显式优先 ✓）—— 不记它，重放就用笔刷
            // 缺省值跑，字节必然不同 ✓；没传记 `null`，与其它来源字段同例。
            "brush_params": seg_brush_params.cloned().unwrap_or(Value::Null),
            "points": seg_points_source,
            // **点列时间也要进 `source`**（P-2）：见本函数段点列时间 ✓。
            "times": seg_times_source,
            "size": size_source,
            "color": color_source,
            "color_to": color_to_source,
            // **显式 `color_space` 也要进 `source`**（C1）：否则 `update_stroke` / 重放时
            // 丢掉空间、笔变色；没传记 `null`，与其它来源字段同例。
            "color_space": args.get("color_space").cloned().unwrap_or(Value::Null),
            // **混色模式也要进 `source`**（P-2）：`pigment` / `linear` 的笔，重放用缺省
            // `Rgb` 会画出不同颜色 ✓；没传记 `null`，与其它来源字段同例。
            "color_mixing": args.get("color_mixing").cloned().unwrap_or(Value::Null),
            "smooth": smooth_source,
            "opacity": opacity_source,
            "hardness": hardness_source,
            // **显式记下随机种子**（第 1062 轮）：内核的抖动由 `(seed, index)` 派生
            //（`render/src/brush.rs:4` 明说"同一份历史在任何平台重放出相同笔迹"），
            // 而它从**对象 `data`** 读 `seed`（`brush.rs:206/229`），**缺省 0**（`:199`）。
            // **不写会依赖默认值**：一旦默认值变了，同一份历史就重放不出同样的像素 ——
            // 而"位图不进工程包、打开时按参数重放"正依赖这条确定性（第 1061 轮）。
            // 这里的默认值与引擎保持一致（0），调用方给了就用调用方的。
            "seed": args.get("seed").and_then(serde_json::Value::as_u64).unwrap_or(0),
            // **∴ 对称也要进 `source` ✗**（**∴ 否则 `update_stroke` 重跑出来的
            //   是**没镜像的那一半** ✓）★
            "symmetry": symmetry_name,
            // **传感器也要进 `source`**（同理：重放时 tilt/viewzoom 效果不能丢 ✓；
            // 没传时记 null，重放侧还原成缺省全 0 ✓）。
            "sensors": sensors_source,
        },
        "layer_id": layer_id,
        "object_id": if seg_force_fresh_object {
            None
        } else {
            optional_str(args, "object_id")
        },
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": x0, "y": y0, "w": width, "h": height},
    });
    let value = write_import_image(ctx, &import_args)?;

    // **墨量与足迹与主函数旧口径同一遍遍历** ✓（原在提交后由 `rgba` 算，现段内算完即带走；
    //   `rgba` 在此已是裁剪后、对称合并后的缓冲 ⇒ 与旧值逐位一致）。
    let (ink_footprint_pixels, ink_mass) =
        rgba.chunks_exact(4)
            .fold((0usize, 0u64), |(footprint, mass), px| {
                if px[3] != 0 {
                    (footprint + 1, mass + u64::from(px[3]))
                } else {
                    (footprint, mass)
                }
            });
    Ok(BrushSegmentPainted {
        value,
        brush: name,
        steps,
        painted,
        dropped: clipped_away,
        clip_coverage,
        ink_footprint_pixels,
        ink_mass,
        max_delta,
        delta_distribution,
        fed_base,
        short_stroke_hint,
        region_x: x0,
        region_y: y0,
        region_w: width,
        region_h: height,
    })
}

/// **一段落笔 + 提交的产物** ✓（`write_brush_stroke` 的回执尾段据此组装；拆分时按段聚合）。
struct BrushSegmentPainted {
    /// `write_import_image` 的回执（含该段提交的 `atom_id`／`seq`／`dirty_bbox`／`preview`）。
    value: Value,
    /// 笔刷名（`paint_brush` 回的解析后资产名）。
    brush: String,
    /// 引擎步数。
    steps: usize,
    /// 裁剪前铺开面积。
    painted: usize,
    /// 选区裁掉的落墨像素数（该段）。
    dropped: usize,
    /// 选区内覆盖率（该段；未裁剪时 `None`）。
    clip_coverage: Option<f64>,
    /// 足迹像素数（裁剪后缓冲的 alpha 非零数）。
    ink_footprint_pixels: usize,
    /// 墨量（`Σalpha`）。
    ink_mass: u64,
    /// 最大通道改变量。
    max_delta: u8,
    /// 差值分布。
    delta_distribution: DeltaDistribution,
    /// 是否吃了底图。
    fed_base: bool,
    /// 短笔触兜底提示（反馈期 1：路径撑不满 dab 间距时延长落一枚 dab 的诊断事实；无兜底时 `None`）。
    short_stroke_hint: Option<String>,
    /// 该段提交区域（文档坐标）。
    region_x: i32,
    region_y: i32,
    region_w: usize,
    region_h: usize,
}

/// **拆分决策** ✓（反馈期 4）：返回 `Some` 当且仅当同时满足——
/// * 探针笔刷（`size` + `brush_params` 覆盖后）按 `check_smudge_budget` 本会 Err；
/// * 可达 `drag + size/2 + 2 ≤ MAX_EFFECT_PADDING`（期 2 新上限，当前值 512，
///   这里直接读渲染常量、不另写一个数 ⇒ 期 2 改上限时此处自动跟上）；
/// * 拆分计划成立，且每段重算成本 ≤ 预算（否则不拆）。
///
/// 其余一切情形返回 `None` ⇒ 主函数走原单笔路，行为一位不动：
/// 探针取不到笔刷／覆盖失败时也返回 `None`（原路会报出真正的错，不在这里改写错因）；
/// 可达超限时同样 `None` ⇒ 原路的预算 Err 就是任务说的“可达超限仍走原 Err”。
struct SmudgeSplitDecision {
    /// 沿用的调用方 `group_id`，或服务端生成的 `split_<ulid>`。
    group_id: String,
    /// 按弧长均分的段点列（相邻段共享接点）。
    segments: Vec<Vec<(f64, f64, f64)>>,
    /// 与段点列对齐的段时间（长度不对齐时为空 ⇒ 距离步进老行为）。
    segment_dtimes: Vec<Vec<f64>>,
    /// 每段预测成本（毫秒，皆 ≤ 预算）。
    segment_costs_ms: Vec<f64>,
    /// 每段弧长（px）。
    segment_path_lengths_px: Vec<f64>,
    /// 可达量（px，≤ 期 2 上限）。
    reach_px: f64,
}

fn decide_smudge_split(
    ctx: &mut ToolContext<'_>,
    args: &Value,
    points: &[(f64, f64, f64)],
    dtimes: &[f64],
    merged_brush_params: &Option<Value>,
    sensors: BrushSensors,
    size: Option<f64>,
) -> Option<SmudgeSplitDecision> {
    let brush_name = require_str(args, "brush").ok()?;
    // **探针笔刷与 `paint_brush` 的预算相关应用逐字同序** ✓：
    // 载笔刷 → 传感器 → `size` 改半径 → `brush_params` 最后覆盖（它优先）。
    // 颜色／不透明度／硬度不影响预算输入，不探。
    let (_, mut brush) = load_brush(ctx.workspace, &brush_name).ok()?;
    brush.viewzoom = sensors.viewzoom;
    brush.barrel_rotation = sensors.barrel_rotation_deg;
    if let Some(diameter) = size {
        if diameter <= 0.0 {
            return None;
        }
        brush.set(
            hokusai::BrushSetting::Radius,
            hokusai::SettingValue::constant(libm::log(diameter / 2.0) as f32),
        );
    }
    if let Some(params) = merged_brush_params {
        apply_brush_params(&mut brush, params).ok()?;
    }
    // **只有“本会 Err”才往下走** ✓：预算内直接 `None`（单笔路一位不动）。
    check_smudge_budget(&brush, points, size).err()?;
    let radius_px = f64::from(brush.get(hokusai::BrushSetting::Radius).base_value).exp();
    let reach_px = smudge_reach_px(&brush)?;
    // **可达闸** ✓：超期 2 上限 ⇒ 不拆分（原路预算 Err 即是交代，不另造错因）。
    if reach_px > yanshi_render::MAX_EFFECT_PADDING as f64 {
        return None;
    }
    let path_len = brush_points_path_len_px(points);
    let plan = smudge_split_plan(path_len, radius_px)?;
    let segments = split_points_by_arclength(points, plan.segments);
    if segments.len() != plan.segments {
        return None;
    }
    // **每段重算成本并断言 ≤ 预算** ✓（草稿要求；均分保证成立，算一遍是防线不是摆设）。
    let mut segment_costs_ms = Vec::with_capacity(segments.len());
    let mut segment_path_lengths_px = Vec::with_capacity(segments.len());
    for seg in &segments {
        let seg_len = brush_points_path_len_px(seg);
        let seg_cost = smudge_stroke_cost_ms(seg_len, radius_px);
        if seg_cost > SMUDGE_BUDGET_MS {
            return None;
        }
        segment_path_lengths_px.push(seg_len);
        segment_costs_ms.push(seg_cost);
    }
    let segment_dtimes = split_dtimes_by_arclength(points, dtimes, plan.segments)
        .unwrap_or_else(|| vec![Vec::new(); plan.segments]);
    let group_id = optional_str(args, "group_id")
        .unwrap_or_else(|| format!("split_{}", yanshi_core::Ulid::new().encode()));
    Some(SmudgeSplitDecision {
        group_id,
        segments,
        segment_dtimes,
        segment_costs_ms,
        segment_path_lengths_px,
        reach_px,
    })
}

pub(super) fn write_brush_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **裁剪累计量现由 `paint_and_commit_brush_segment` 按段返回、分支处聚合** ✓
    // Validate output options before rasterization or committing a stroke.
    let anchor_mode = optional_str(args, "anchor_mode").unwrap_or_else(|| "data".to_owned());
    if anchor_mode != "data" {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "anchor_mode 只支持 data（缺省）—— {anchor_mode} 还没实现：\
                 dual（两张图：干净 ＋ 带 Overlay）需要额外的缩略图合成通道，已记录在需求文档 P1 #12"
            )),
        ));
    }
    // **笔迹标签**：`brush_stroke` 最终走 `write_import_image` 的 `ctx.commit` ⇒ 那里会取走它 ✓。
    ctx.pending_label = optional_str(args, "label");
    ctx.pending_group_id = optional_str(args, "group_id");
    // ★ **`wait_for_render` 参数 ✗**：**∴ 缺省不给 ⇒ 保持 true（现有行为 ✓）**
    //   ⇒ **∴ 给了就就地改 ✓ ⇒ **∴ 不等渲染时省掉那 66% ✓**
    if let Some(wait) = args.get("wait_for_render").and_then(Value::as_bool) {
        ctx.set_wait_for_render(wait);
    }

    // **预览开关** ✓（第 5 轮 ✓）：只影响"要不要编这张 PNG" ✓，不影响 job ✓。
    if !optional_bool(args, "preview").unwrap_or(true) {
        ctx.preview = false;
    }

    require_str(args, "layer_id")?;
    let brush_name = require_str(args, "brush")?;
    let points = parse_brush_points(args)?;
    // **★ 退化成"一个点"就自动展开（真机报告 P2 #1）★** —— 画"点"不该报错 ✓。
    let points = expand_degenerate_points(points, args.get("size").and_then(Value::as_f64));
    // **★ P0 #3：取每个点的真实时间（没有就是 -1 ⇒ 落回 STEP_SECONDS）★**
    let dtimes = parse_brush_point_times(args)?;
    // **真实传感器**（缺省不传 ⇒ 全 0 ⇒ 与历史行为逐位一致；传了非法值报错 ✓）。
    let sensors = match args.get("sensors") {
        Some(value) if !value.is_null() => brush_sensors_from_value(value)?,
        _ => BrushSensors::default(),
    };
    // **风格变换** ✓（缺省 ⇒ 原样 ✓，见 `apply_brush_style` ✓）。
    let points = apply_brush_style(points, args)?;
    // **曲线尺吸附**（二期 `path_guide`）：只拉落点坐标 (x, y)，压感逐位不动；
    // 时间戳在点列之外按下标携带（`dtimes`），点数与顺序不变 ⇒ 同样不受影响。
    // 缺省不传 ⇒ 点列逐位不变（回归锁）；传了 ⇒ 吸附后的点列即此后的一切
    // 来源（含下方 `points_source` 的回放记录），与 style 变换同例。
    let points: Vec<(f64, f64, f64)> = if args.get("path_guide").is_some() {
        let coords: Vec<(f64, f64)> = points.iter().map(|p| (p.0, p.1)).collect();
        let snapped = apply_path_guide_xy(ctx, args, coords)?;
        points
            .into_iter()
            .zip(snapped)
            .map(|((_, _, pressure), (x, y))| (x, y, pressure))
            .collect()
    } else {
        points
    };
    // **★★★ 对称模式 ✗ ★★★**（**第 251 轮 ✓）：**这一条路是**服务端自己栅格化**的** ✓
    //   ＋ **∴ 仓库自己写明 ✗**：**"提交一条笔触走的是**本文件这一份**（**`hokusai` ✓）
    //     ＋ **∴ 而** `yanshi-render::brush` 是**渲染那条路**用的** ✓
    //       ⇒ **∴ 所以**：**两条都要接 ✗ 否则 UI 里**看不到对称** ✓ ★★
    //   ＋ **∴ 这一条怎么接 ✗**：**把**点序列镜像一份**✗ 一起栅格化** ✓
    //     （**∴ 对**只按位置盖章的路径 ✗ 镜像点序列与**逐 dab 镜像**等价 ✓ 且改动最小 ✓）★
    //   ＋ **∴ 而** `source` 里要**记下 symmetry** ✗ ⇒ **∴ `update_stroke`**重跑时**才会同样镜像** ✓
    //     （**∴ 本仓的确定性纪律：**位图不进包 ✗ 打开时按参数重放** ✓）★★
    // **∴ `String` 不能用 `&str` 字面量做模式 ✗**（**实测：expected String, found &str ✓）
    //   ⇒ **∴ 先 `as_str()`** ✓ ★
    // **∴ 解析抽到 `parse_symmetry_arg` ✗ 与 medium_stroke 共用一份词表** ✓（**第 253 轮 ✓）★
    let symmetry = parse_symmetry_arg(args)?;
    // **★★★ 实测踩到 ✗：不能把镜像点**接在同一条折线**上** ✓ ★★★**
    //   ＋ **∴ 我第一版**就是这么写的 ✗ ⇒ **∴ 栅格化器**把两点之间的位移也画了**
    //     ⇒ **∴ 量出来是**一条横穿画布的连续墨带**（**52..202 ✓）✗ 不是两个笔尖** ✓
    //       ⇒ **∴ 教训 ✗**：**"点序列"是一条**连续笔画**✗** ⇒ **∴ 加两点 ≠ 画两笔** ✓ ★★
    //   ＋ **∴ 正解（**本函数后面按**位图**做 ✓）**：**原样交给 `paint_brush` 只画一笔**✗
    //     **再把这一笔自己的位图**翻转**✗ 摆到镜像位置** ✓ ★★★
    // **调用方给的原话** ✓（重跑时要用它 ✓，而不是"平滑之后的中间量" ✗）。
    let points_source = json!(points
        .iter()
        .map(|(x, y, pressure)| json!([x, y, pressure]))
        .collect::<Vec<_>>());
    // **点列时间也要进 `source`**（P-2）：每点的第 4 个元素 `time_ms`（没有记 `null` ✓）——
    // 它决定落笔时的速度输入，速度敏感的笔刷缺了它就重放不出同一笔 ✓；
    // 与 `points_source` 同一下标 ✓（风格/吸附不改点数 ✓，时间与点一一对应 ✓）。
    let times_source = args
        .get("points")
        .and_then(Value::as_array)
        .map(|items| {
            Value::Array(
                items
                    .iter()
                    .map(|item| {
                        item.as_array()
                            .and_then(|pair| pair.get(3))
                            .cloned()
                            .unwrap_or(Value::Null)
                    })
                    .collect(),
            )
        })
        .unwrap_or(Value::Null);
    let color_source = args.get("color").cloned().unwrap_or(Value::Null);
    let color_to_source = args.get("color_to").cloned().unwrap_or(Value::Null);
    // **★ `opacity` 越界要**报错**，不许静默夹紧** ✓（第 1419 轮；外部 D-12）
    //   **⇒ 原缺陷** ✗：`opacity: 5` **静默变成 1.0**（`opacity.clamp(0.0, 1.0)`）⇒
    //     调用方以为「5 是个很强的透明度」⇒ 实际**和 1.0 一样** ✗
    //   **⇒ ⇒ 所以**在这里就拒绝**并说清范围 ✓（负值同样）
    if let Some(raw) = args.get("opacity").and_then(Value::as_f64) {
        if !(0.0..=1.0).contains(&raw) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "opacity 必须在 0..1（给了 {raw}）⇒ 越界不再静默夹紧；0 = 全透明，1 = 全不透明"
                )),
            ));
        }
    }
    // **落笔之前先问"这条笔迹在画布上吗"** ✗（本轮新增 ✓）。
    //
    // **为什么必须有** ✓：外部 MCP 报告**连着两轮**把"坐标非线性偏移"当 bug 报 ✓，
    // 而它量到的很可能正是**被画布裁掉一半的墨** ✗ —— 因为当时这种情况会一路走到
    // "这一笔没落下任何像素" ✓，再被那句"**换一支笔刷**"✗ 引到错误方向 ✓
    //（agent 于是去查坐标映射 ✓，量到的包围盒当然是**残缺的** ✓）。
    // ⇒ 现在：**完全在画布外** ⇒ 明确拒绝 ✓ 并写出**画布尺寸与笔迹范围** ✓；
    //   **部分越界** ⇒ 照画 ✓，但在响应里带 `clipped` 与 `warnings` ✓（不静默 ✓）。
    // `brush_stroke_region` 回的是 **(x0, y0, x1, y1, 宽, 高)** ✓（`i32` ✓，已含笔尖半径 ✓）。
    let (doc_width, doc_height) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| {
            (
                document.state().width as i32,
                document.state().height as i32,
            )
        })
        .unwrap_or((0, 0));
    let (x0, y0, x1, y1, _, _) =
        brush_stroke_region(&points, args.get("size").and_then(Value::as_f64));
    let entirely_outside = x1 <= 0 || y1 <= 0 || x0 >= doc_width || y0 >= doc_height;
    let clipped = x0 < 0 || y0 < 0 || x1 > doc_width || y1 > doc_height;
    if entirely_outside {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
            "整条笔迹都在画布**外面** ⇒ 一枚 dab 都不会落上 ✓。画布是 {doc_width}×{doc_height} \
             （原点在**左上角** ✓，坐标就是**文档像素** ✓），而这条笔迹的范围是 \
             x {x0}..{x1}、y {y0}..{y1}（含笔尖半径）⇒ 把坐标挪进画布内即可 ✓ \
             —— 这不是笔刷的问题 ✗，**别去换笔刷** ✓"
            )),
        ));
    }
    // **可选平滑** ✓（`smooth: true` ⇒ 把控制点当 **Catmull-Rom 样条** ✓）——
    // 与 `draw_stroke` 的 `data.smooth` **同一个实现** ✓；不给 ⇒ **逐字节不变** ✓（老调用方不受影响 ✓）。
    let points = if optional_bool(args, "smooth").unwrap_or(false) {
        smooth_stroke_points(&points)
    } else {
        points
    };
    // **★ P1 #11 humanize：拟人模式（默认 true）★**（第 254 轮）
    //   需求文档附录二：非对称起收笔包络 —— 起笔 12%（快升）、行笔 60%（平稳）、收笔 28%（长尾）。
    //   **爆炸半径怎么控住**（本轮的设计决定，写清楚免得后人踩）：
    //     * **调用方显式给了每个点的压力** => `humanize` **什么都不做**（一个像素都不改）
    //       => 既有的像素级判据不受影响；
    //     * **没给压力（缺省 0.5）** => 才套包络 —— 这就是「默认就好用」：AI 通常只给坐标。
    //   **包络施加在**平滑之后**的点上**：Catmull-Rom 已经把压力插值平滑过，再套包络才不会出折角。
    // **★ P1 #8 曲率驻留（**opt-in**）★**（第 254 轮）
    //   `dwell` 缺省 **不给** ⇒ `apply_curvature_dwell` 都不调用 ⇒ 点列**一个都不动**
    //   （这是本仓纪律：会改像素的新行为必须显式打开 ✓）。
    //   给了才按 κ^(2/3) 在**转弯处**多插点（手在拐角天然减速的物理 ✓）。
    let points = match args.get("dwell").and_then(Value::as_f64) {
        Some(strength) => apply_curvature_dwell(&points, strength),
        None => points,
    };
    // **★ P1 #8 生理颤动（opt-in）★**（第 254 轮）
    //   `tremor: 0..1` 缺省不给 => `apply_tremor` 都不调用 => 点列一个都不动（逐字节不变 ✓）。
    //   给了才叠**带限**的 1D value noise（手腕 λ≈90px ＋ 震颤 λ≈28px，沿局部法向）——
    //   与仓库既有的**白噪声**抖动（`apply_brush_style` 的 `scatter_next`）不同：
    //   白噪声相邻点独立（像砂纸），带限噪声相邻点相关（像手）。
    let points = match args.get("tremor").and_then(Value::as_f64) {
        Some(amount) => {
            let seed = args.get("tremor_seed").and_then(Value::as_u64).unwrap_or(0);
            apply_tremor(&points, amount, seed)
        }
        None => points,
    };
    let mut points = points;
    // **★ 压感包络的开关（真机报告 P1：`pressure_curve`）★**（第 254 轮）
    //
    // 真机反馈"**笔触太硬，压力变化没用**"✗ —— 根因是**程序生成的 pressure 是恒定值**
    //（`0.8` 这种 ✓），而包络原来**只在"完全没给 pressure"时**才加 ✓ => 给了恒定值就绕过了 ✓。
    // **现在**：
    //   * `auto`（缺省 ✓）：**没给 pressure** ⇒ 加包络（老行为 ✓）；**给了但整笔恒定** ⇒ **也加** ✓；
    //     **给了有起伏的** ⇒ **原样尊重** ✓（不叠两层 ✗）；
    //   * `flat`（`none`/`off` 同义 ✓）：**永不动** ✓（技术绘图 ✓）；
    //   * **`humanize: false` ⇒ 一律不动** ✓ —— 这是硬约束
    //     （"`humanize:false` 时老行为逐字节不变" ✓），所以它**优先于** `pressure_curve` ✓。
    // **★ 缺省是 `flat`（2026-10-15 真机报告后的修正）★**：我第一版把缺省设成 `auto` ⇒
    //   **默认行为变了** ✗ ⇒ CI 里 `kernel-brush-parity`／`tool-brush-parity-jitter`／
    //   `tool-brush-style`／`tool-brush-dynamics` **同时红** ✗（它们正是比"同一笔触"的 ✓）。
    //   => 按本仓纪律（**改默认行为要极慎重** ✓）**缺省改回 `flat`（＝与以前逐字节相同 ✓）** ✓，
    //   **想要"起轻行重收轻"请显式传 `pressure_curve: 「auto」`** ✓（summary 里已写明 ✓）。
    //   **要不要把它变成默认** —— 那是一次**产品决策** ✓，需要同时更新上面那几条笔刷判据
    //   并说清代价 ✓，不该顺手做掉 ✗。
    let pressure_curve = optional_str(args, "pressure_curve").unwrap_or_else(|| "flat".to_owned());
    // **★ `pressure_curve` 只该管"新增的那一半"（2026-10-15 的教训）★**
    //   我第一版写成"`flat` ⇒ 什么都不做" ✗ ⇒ 连**"完全没给 pressure"时 humanize 的包络**
    //   也一起没了 ✗✗ ⇒ CI 的 `tool-humanize.mjs` 当场红 ✓
    //   （判据原话：**默认指纹 == `humanize:false` 指纹** ⇒ 那就等于 humanize 失效 ✗）。
    //   **正确语义**：
    //     * **完全没给 pressure** ⇒ 老行为：`humanize` 开着就加包络 ✓（与 `pressure_curve` 无关 ✓）；
    //     * **给了 pressure** ⇒ 只有 `pressure_curve: 「auto」` 才在"整笔恒定"时**补**包络 ✓
    //       （缺省 `flat` ⇒ 不动你的 pressure ✓）；
    //     * **`humanize: false`** ⇒ 一律不动 ✓（硬约束 ✓）。
    let envelope_wanted = if !all_points_have_pressure(args) {
        true
    } else {
        matches!(pressure_curve.as_str(), "auto") && points_are_flat(&points)
    };
    // **B4：包络没补上时要出声**（只回显，不改行为、不改缺省）——**必须在套包络之前算**：
    //   `apply_humanize_envelope` 就地改压力，算晚了就看不出原来是起伏还是恒定。
    //   只在显式传了才信号（缺省调用不刷警告）：
    //   * 显式 `pressure_curve:"auto"` + 点里压力有起伏 ⇒ `auto` 不生效；
    //   * 显式 `humanize:true` + 点里带压力 + 包络未生效 ⇒ 与上一条合并为一条，注明原因
    //     （起伏压力 / 恒定压力 + `flat` 缺省）；`humanize:false` 是显式关包络，不出声。
    let envelope_warning: Option<String> = {
        let auto_explicit = args.get("pressure_curve").and_then(Value::as_str) == Some("auto");
        let humanize_explicit = optional_bool(args, "humanize") == Some(true);
        let has_pressure = all_points_have_pressure(args);
        let flat = points_are_flat(&points);
        let envelope_applied = optional_bool(args, "humanize").unwrap_or(true) && envelope_wanted;
        if auto_explicit && has_pressure && !flat {
            Some(if humanize_explicit {
                "`pressure_curve:auto` 不生效：点里压力已有起伏，包络只在恒定压力时补；`humanize:true` 也因此不生效".to_owned()
            } else {
                "`pressure_curve:auto` 不生效：点里压力已有起伏，包络只在恒定压力时补".to_owned()
            })
        } else if humanize_explicit && has_pressure && !envelope_applied {
            Some(if flat {
                "`humanize:true` 不生效：点里压力恒定但 `pressure_curve` 不是 `auto`（缺省 `flat`），包络只在恒定压力 + `auto` 时补".to_owned()
            } else {
                "`humanize:true` 不生效：点里压力已有起伏，包络只在恒定压力 + `pressure_curve:auto` 时补".to_owned()
            })
        } else {
            None
        }
    };
    if optional_bool(args, "humanize").unwrap_or(true) && envelope_wanted {
        apply_humanize_envelope(&mut points);
    }
    // **★ P1 #10 罩染（opt-in）★**（第 254 轮）
    //   `glaze: true` => 3 遍（可调 `glaze_passes`）**微偏移薄涂**：几何由
    //   `apply_glaze_passes` **交错展开**（p1(A),p2(B),p3(C),p4(A)…）—— 先整遍 A 再整遍 B 会在
    //   两遍之间留一条**回程跳线** ✗；交错之后相邻点只差几像素 => 得到**加宽的柔带** ✓。
    //   **透明度**：整笔除以遍数（三遍叠加 ≈ 原来的浓度 ✓）。
    //   **不给 `glaze` / 给 false** => 遍数 1 => 点列与透明度**都不动**（逐字节不变 ✓）。
    // **★ P2 #16 混色模式 ★**（第 254 轮）
    //   `rgb`（**缺省**）=> 与仓库既有的 `lerp_srgb` 完全同口径 => **逐字节不变**；
    //   `pigment` => 光谱 K/S 混色（黄+蓝=绿，而不是灰）。
    let color_mixing = match optional_str(args, "color_mixing") {
        Some(name) => yanshi_render::color::ColorMixing::parse(&name).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "未知 color_mixing {name} ⇒ 可用：rgb（缺省）/ linear（物理正确混色）/ pigment（光谱颜料）"
                )),
            )
        })?,
        None => yanshi_render::color::ColorMixing::Rgb,
    };
    let (points, glaze_passes) = if optional_bool(args, "glaze").unwrap_or(false) {
        let passes = args
            .get("glaze_passes")
            .and_then(Value::as_u64)
            .unwrap_or(3)
            .clamp(1, 8) as usize;
        // "微偏移"：间距与笔尖尺寸成比例（缺省 size 24 => 间距约 4.3px）
        let size = args
            .get("size")
            .and_then(Value::as_f64)
            .unwrap_or(24.0)
            .max(1.0);
        (apply_glaze_passes(&points, passes, size * 0.18), passes)
    } else {
        (points, 1usize)
    };
    let glaze_opacity = args
        .get("opacity")
        .and_then(Value::as_f64)
        .map(|value| value / glaze_passes as f64);
    // 意图参数（hand_tremor 等）展开成 brush_params 别名，与显式 brush_params 合并（显式优先）
    let merged_brush_params = merge_intent_brush_params(args)?;
    // **`color.a`：P-19 已把它乘进有效 opacity** ✓（paint-fix 期 7；见 `paint_brush` 里那段 ✓）。
    //   此前是"接受但不生效"（第 788 轮；`2dfd7d2b` 的拒绝**太激进** ✗）
    //   **⇒ CI 实测**：`export_small.rs` 用 `a = 220` ✓（半透明 ✓，完全合法的既有用法 ✓）
    //     ⇒ 拒绝让 `export_small` 的 **3 个测试** 与 **4 片 criteria** 一起红 ✗
    //     （`test (stable)` / `test (beta)` / `criteria shard 1,2,4,5` ✓）。
    //   **⇒ 结论**：**"忽略 alpha"是既有行为、且有调用方依赖** ✓ ⇒ 不能直接改成拒绝 ✗；
    //     修法是**把 `a/255` 乘进笔刷的有效 opacity** ✓ —— 现已落地 ✓，
    //     `a=255` 时与以前逐位相同 ✓，`a<255` 时整笔变透明 ✓（判据见 `ink_pixels` 那条 ✓）。
    //     `export_small` 的 `a=220` 现在画出来会比以前淡一点（0.86 倍不透明度 ✓），
    //     它的测试是导出前后自比对 ✓ ⇒ 不受影响 ✓。

    // **落笔走唯一的实现** ✓（`paint_brush` ✓，与 `brush_preview` 共用 ✓）。
    // **显式 `color_space`**（C1）：只认 `"srgb"` / `"linear"`，不传 ⇒ 量级切换；
    // 作用于 `color` 与 `color_to` 两处；非法值在这里就 `invalid_argument`（落笔前拒绝）。
    let color_space = parse_color_space_arg(args)?;
    // **反馈期 4：涂抹预算闸自动拆分** ✓（5.2；阈值按期 2 新上限 512 校准，见决策函数）。
    //
    // **两路的回执尾段是同一段** ✓：分支只产出下面这些统一量（拆分路聚合、单笔路直取），
    // 之后 `value["brush"]` 起的全部回执字段（`landed`／`anchors`／`warnings`／`delta_regime`
    // 等）不分路 ⇒ 拆分不会悄悄改掉老字段的口径。
    //
    // **拆分路的关键语义** ✓（设计草稿已验证的结论，直接落实）：
    // * 每段一次 `paint_brush` + 一次 `write_import_image` 提交（段内喂底图读的是
    //   已提交状态 ⇒ 后段看得到前段刚落的墨，不抹空白；对称镜像在段内做 ⇒ 每段都镜像）；
    // * 各段同属一次 `brush_stroke` 调用 ⇒ 天然同一变更集（`commit` 的变更集复用）；
    //   组 id 每段提交前由 helper 重设（`take()` 只取走一次，不重设就只有首段带组）；
    // * 段中途失败 ⇒ 先以同组 `revert_group` 收回已提交段，再回原错（不留半截笔画）；
    // * 素 `undo_last` 按连续同一 `object_id` 归并 ⇒ 只撤最后一段，**不是**整组撤销的
    //   等价物（回执 `undo_note` 明说整组撤销走 `revert_group`，不改 `collect_gestures` 语义）；
    // * 段 `source` 记该段的落笔后点列、`smooth: false`（点列已是平滑后结果，重放时
    //   不再二次平滑；`glaze`／`tremor`／`dwell` 本就不进 `source`，与单笔路同例）
    //   与段时间不进段 `source`（`times: null` ⇒ 重放按距离步进；落笔当次仍用插值时间）。
    let (
        mut value,
        name,
        steps,
        painted,
        clipped_away,
        clip_coverage,
        ink_footprint_pixels,
        ink_mass,
        max_delta,
        delta_distribution,
        fed_base,
        x0,
        y0,
        width,
        height,
        short_stroke_hint,
    ) = if let Some(plan) = decide_smudge_split(
        ctx,
        args,
        &points,
        &dtimes,
        &merged_brush_params,
        sensors,
        args.get("size").and_then(Value::as_f64),
    ) {
        let mut seg_values: Vec<BrushSegmentPainted> = Vec::with_capacity(plan.segments.len());
        let mut committed = 0usize;
        let mut failed: Option<YanshiError> = None;
        for (index, seg_points) in plan.segments.iter().enumerate() {
            let seg_points_source = json!(seg_points
                .iter()
                .map(|(x, y, pressure)| json!([x, y, pressure]))
                .collect::<Vec<_>>());
            let seg = paint_and_commit_brush_segment(
                ctx,
                args,
                seg_points,
                &plan.segment_dtimes[index],
                &seg_points_source,
                &Value::Null,
                false,
                glaze_opacity,
                merged_brush_params.as_ref(),
                color_mixing,
                sensors,
                color_space,
                symmetry,
                Some(&plan.group_id),
                true,
            );
            match seg {
                Ok(seg) => {
                    committed += 1;
                    seg_values.push(seg);
                }
                Err(error) => {
                    failed = Some(error);
                    break;
                }
            }
        }
        if let Some(error) = failed {
            // **段中途失败先回滚** ✓：清掉待取的组／标签（免得回滚原子误带组），
            // 再以同组 `revert_group` 收回已提交段；回滚本身失败也不掩盖原错。
            ctx.pending_group_id = None;
            ctx.pending_label = None;
            let rolled_back = write_revert_group(ctx, &json!({"group_id": plan.group_id}))
                .ok()
                .and_then(|v| v["reverted_count"].as_u64())
                .unwrap_or(0);
            let message = format!("{error}");
            return Err(YanshiError::new(
                error.code,
                ErrorContext::detail(format!(
                    "涂抹拆分落笔第 {} 段失败（已提交 {committed} 段，已同组 revert {rolled_back} 段，不留半截笔画）⇒ 原错：{message}",
                    committed + 1
                )),
            ));
        }
        // **聚合** ✓：画量／墨量求和，差值分位数取各段最大（保守口径），区域取并集，
        // 覆盖率按“总留存／总铺开”合并（与单笔路 `kept / total` 同口径）。
        let total_painted: usize = seg_values.iter().map(|s| s.painted).sum();
        let total_dropped: usize = seg_values.iter().map(|s| s.dropped).sum();
        let any_clipped = seg_values.iter().any(|s| s.clip_coverage.is_some());
        let merged_coverage = if any_clipped {
            Some(
                (total_painted.saturating_sub(total_dropped) as f64)
                    / (total_painted.max(1) as f64),
            )
        } else {
            None
        };
        let (ux0, uy0, ux1, uy1) = seg_values.iter().fold(
            (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
            |(ax0, ay0, ax1, ay1), s| {
                (
                    ax0.min(s.region_x),
                    ay0.min(s.region_y),
                    ax1.max(s.region_x + s.region_w as i32),
                    ay1.max(s.region_y + s.region_h as i32),
                )
            },
        );
        // **回执主体取末段提交** ✓（`atom_id`／`seq`／`preview` 皆为末段提交后的终态），
        // 脏区改写成各段区域并集（末段一家的脏区会漏掉前面段的区域）。
        let mut value = seg_values
            .last()
            .expect("拆分至少两段，末段必在")
            .value
            .clone();
        value["dirty_bbox"] = json!([ux0, uy0, ux1 - ux0, uy1 - uy0]);
        // **拆分字段：不静默拆分** ✓（草稿 §5 的字段全数在位）。
        value["split"] = json!(true);
        value["split_segments"] = json!(plan.segments.len());
        value["split_group_id"] = json!(plan.group_id);
        value["segment_costs_ms"] = json!(plan.segment_costs_ms);
        value["segment_path_lengths_px"] = json!(plan.segment_path_lengths_px);
        value["segment_atom_ids"] = json!(seg_values
            .iter()
            .map(|s| s.value["atom_id"].clone())
            .collect::<Vec<_>>());
        value["reach_cap_px"] = json!(yanshi_render::MAX_EFFECT_PADDING);
        value["reach_px"] = json!(plan.reach_px);
        value["undo_note"] = json!(format!(
            "本次落笔已拆成 {} 段、同属一组 {}：整组撤销请调 revert_group（素 undo_last 只撤最后一段，不是整组撤销）",
            plan.segments.len(),
            plan.group_id
        ));
        let all = &seg_values;
        let steps = all.iter().map(|s| s.steps).sum();
        let painted = all.iter().map(|s| s.painted).sum();
        let ink_footprint_pixels = all.iter().map(|s| s.ink_footprint_pixels).sum();
        let ink_mass = all.iter().map(|s| s.ink_mass).sum();
        let max_delta = all.iter().map(|s| s.max_delta).max().unwrap_or(0);
        let delta_distribution = DeltaDistribution {
            median: all
                .iter()
                .map(|s| s.delta_distribution.median)
                .max()
                .unwrap_or(0),
            p90: all
                .iter()
                .map(|s| s.delta_distribution.p90)
                .max()
                .unwrap_or(0),
            p99: all
                .iter()
                .map(|s| s.delta_distribution.p99)
                .max()
                .unwrap_or(0),
            changed_pixels: all
                .iter()
                .map(|s| s.delta_distribution.changed_pixels)
                .sum(),
        };
        let fed_base = all.iter().any(|s| s.fed_base);
        let name = all.last().map(|s| s.brush.clone()).unwrap_or_default();
        (
            value,
            name,
            steps,
            painted,
            total_dropped,
            merged_coverage,
            ink_footprint_pixels,
            ink_mass,
            max_delta,
            delta_distribution,
            fed_base,
            ux0,
            uy0,
            (ux1 - ux0).max(0) as usize,
            (uy1 - uy0).max(0) as usize,
            all.iter().find_map(|s| s.short_stroke_hint.clone()),
        )
    } else {
        // **单笔路** ✓：与重构前同一段代码（helper 内），回执量直取，无聚合。
        let seg = paint_and_commit_brush_segment(
            ctx,
            args,
            &points,
            &dtimes,
            &points_source,
            &times_source,
            optional_bool(args, "smooth").unwrap_or(false),
            glaze_opacity,
            merged_brush_params.as_ref(),
            color_mixing,
            sensors,
            color_space,
            symmetry,
            None,
            false,
        )?;
        (
            seg.value,
            seg.brush,
            seg.steps,
            seg.painted,
            seg.dropped,
            seg.clip_coverage,
            seg.ink_footprint_pixels,
            seg.ink_mass,
            seg.max_delta,
            seg.delta_distribution,
            seg.fed_base,
            seg.region_x,
            seg.region_y,
            seg.region_w,
            seg.region_h,
            seg.short_stroke_hint,
        )
    };
    value["brush"] = json!(name);
    value["steps"] = json!(steps);
    value["painted_pixels"] = json!(painted);
    // **同值字段 `painted_pixels_raw`** ✓（A3 并存方案）：`painted_pixels` 是**裁剪前**的铺开面积，
    //   与 `painted_pixels_raw` 完全同值 ⇒ 老调用方不用改，新调用方一眼看出"这是 raw 口径" ✓。
    value["painted_pixels_raw"] = json!(painted);
    // **裁剪后的真实落墨量** ✓（第 1553 轮；外部 `B6`）
    //   **⇒ 为什么两个都要**：`painted_pixels` 是**印章面积**（裁剪前）⇒ 用它算覆盖率
    //     会得出「笔触都落上了」的错觉（他们实测：选区 166 万像素、累计报 793 万）✗
    //   **⇒ ⇒ 所以**同时给：`painted_pixels`（raw）与 `painted_pixels_clipped`（**真实落墨**）✓
    //   **⇒ ⇒ ⇒ 没有裁剪时两者相等**（`clipped_away == 0`）⇒ **老调用方不受影响** ✓
    value["painted_pixels_clipped"] = json!(painted.saturating_sub(clipped_away));
    // **`ink_footprint_pixels`：落过墨的**足迹**像素数** ✓（第 808 轮改名；外部 `qoder` 的 D192）
    //   **⇒ 为什么改名（他们说对了）** ✗：原来叫 `ink_pixels` ✓ 而谓词是
    //     `rgba.chunks_exact(4).filter(|px| px[3] != 0).count()` ✓ ⇒ 它数的是
    //     **印章足迹 × alpha 幸存** ✓（alpha 来自覆盖率 × opacity ✓）
    //     ⇒ **颜色一格都没改的笔也报"满是墨"** ✗ ⇒ **名字承诺"落墨"，谓词量的是足迹** ✓。
    //   **⇒ 所以**：名字改成**它真正量的东西** ✓（`ink_footprint_pixels` ✓），并保留
    //     `ink_pixels` 作为**同值的兼容别名** ✓（免得他们已写好的脚本立刻断掉 ✓；
    //     两个字段同源 ✓，注释里说清"别名" ✓）。
    //   **⇒ 与 `painted_pixels` 的区别**：
    //     `painted_pixels`         ＝ **印章面积**（铺开了多少 ✓）；
    //     `ink_footprint_pixels`   ＝ 合成后 **alpha 非零** 的像素数（足迹上活下来多少 ✓）；
    //     **两者都不回答"画面被改了多少"** ✗ —— 那需要"合成前后的 RGB 差"，
    //     而 `max_channel_delta` 在 `reads_canvas=false` 的一支里**合到白底**再比 ✓
    //     ⇒ **在非白底文档上它不等于"交付图被改了多少"** ✓（见台账第四百七十六节）。
    //   **⇒ 为什么需要它** ✗：他们的 D139 说「有印章但落墨 0」是**静默区** ✓
    //     ⇒ 而 `painted_pixels` **看不出**这件事 ✓（全透明一笔照样 `painted=324` ✓，实测 ✓）。
    //   **⇒ 为什么现在只给数、不给 warning** ✗：**warning 那条分支我暂时造不出触发条件** ✓
    //     （`color.a=0` 实测**不是**全透明 ✓：`painted=324 / ink=324` ✓）⇒ 按规矩
    //     **不提交没有判据的分支** ✓ ⇒ 先只加字段 ✓，warning 等能复现了再加 ✓。
    // **墨量 `ink_mass` 与足迹同一遍遍历** ✓（A2）：`Σalpha`（落墨像素 alpha 字节之和，u64）⇒
    //   恒等式 `0 ≤ ink_mass ≤ ink_footprint_pixels × 255`；现由
    //   `paint_and_commit_brush_segment` 在段内从**裁剪后**缓冲算出（拆分路为各段求和）⇒
    //   Σalpha 天然与 `painted_pixels_clipped` 同口径，对称合并也在此之前 ⇒ 镜像一半已含在内 ✓。
    value["ink_footprint_pixels"] = json!(ink_footprint_pixels);
    value["ink_mass"] = json!(ink_mass);
    // **兼容别名** ✓：值完全相同 ✓，只在注释里说明"同源、语义是足迹" ✓
    value["ink_pixels"] = json!(ink_footprint_pixels);
    // **覆盖率只在裁剪时出现**（不裁剪时不出现，而不是给个 1.0 让人以为是量出来的）✓
    // **⇒ ★ 但它单独**不能**用于验收 ★**（第 1573 轮自查；用外部那条判据检验自己）
    //   **⇒ 我实测**：一笔**极小**、且**完全在选区内** ⇒ **`clip_coverage = 1.0`**，
    //     而 **`painted_pixels_clipped = 10`** ⇒ **几乎没画，覆盖率却是满分** ✗
    //   **⇒ ⇒ 所以**：**`clip_coverage` 必须和 `painted_pixels_clipped` 一起读** ✓
    //     （**前者答"裁剪消化了多少"，后者答"实际落了多少"** ⇒ **两个一起才是"效果"**）
    //   **⇒ ⇒ ⇒ 而这条来自外部的验收设计结论**：「一条指标如果能被**廉价地满足**，
    //     它就不能**单独**作为验收」✓
    if let Some(coverage) = clip_coverage {
        value["clip_coverage"] = json!(coverage);
    }
    value["max_channel_delta"] = json!(max_delta);
    // **差值分布** ✓（P-15）：与 `max_channel_delta` 同一遍遍历算出，只统计改了的像素 ✓。
    //   单像素最大值会被边缘伪影主导（255 可能只来自个别像素），而 `delta_median`
    //   接近 0 ⇒ 说明那 255 不是整笔的幅度。
    value["delta_median"] = json!(delta_distribution.median);
    value["delta_p90"] = json!(delta_distribution.p90);
    value["delta_p99"] = json!(delta_distribution.p99);
    value["changed_pixels"] = json!(delta_distribution.changed_pixels);
    // **落笔顶层结论** ✓（A1/B1）：`landed` + `landed_reason` ⇒ 调用方不用再自己读五个字段拼结论 ✓。
    //   判定见 `landed_reason`（`tools.rs`，与 `medium_stroke` 共用同一函数 ✓）。
    let (landed_reason, landed) = landed_reason(
        painted.saturating_sub(clipped_away) as u64,
        ink_footprint_pixels as u64,
        u64::from(max_delta),
    );
    value["landed"] = json!(landed);
    value["landed_reason"] = json!(landed_reason);
    // **本次调用生效参数的回显** ✓（A5）：`wait_for_render`／`wait_budget_ms`／`preview`／
    //   `silent`／`doc_id` 读 `ctx`（本函数开头已把当次调用的 `wait_for_render`／`preview`
    //   写进 `ctx` ✓），`color_space` 按显式参数与 `args["color"]` 推导 ✓（C1；见 `applied_echo` ✓）。
    value["applied"] = applied_echo(ctx, args);
    // **`max_channel_delta` 的 regime 必须**机器可读**** ✓（第 808 轮；外部 `qoder` 的 D193）
    //   **⇒ 他们的实测**：`reads_canvas=false` 的那一支**先把补丁合成到白底**再与 `255` 比 ✓
    //     ⇒ 在**非白底文档**上 `delta = (255 − patch_rgb) × alpha/255` ✓
    //     ⇒ 一笔**导出 PNG 差 0 像素**的笔，这里**报正数** ✗ ⇒ **不能当 zero-ink 阈值** ✓。
    //   **⇒ 他们已经收窄了结论**（我们的注释把"无基准 ⇒ 合白底"写成故意定义 ✓、回执也带 `fed_base` ✓）
    //     ⇒ 可执行的那一半是：**调用方要能一眼看出这个数是在哪个 regime 下量的** ✓。
    //   **⇒ 所以**：加一个字符串字段 ✓ —— `white_base`（没吃底图 ⇒ 合白底后与 255 比 ✓）
    //     ／`canvas_base`（吃了底图 ⇒ 与真底图比 ✓）✓。**文档写了，但字段比文档强** ✓。
    // **背景是否真的读到** ✓（`paint_brush` 里那份用于比较 ✓；这里只为给 regime 贴标签 ✓）
    // **背景只读一个布尔，不许为它克隆整份文档状态** ✓（P-8）：`document_state`
    // 克隆 `objects` 全表（N 个对象），每画一笔付一次 ⇒ 这里直接借 `document()`
    // 读背景，不克隆 ✓；文档不在 ⇒ false（与 `document_state` 出错时同例 ✓）。
    let background_from_state = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| {
            document
                .state()
                .background
                .get("r")
                .and_then(Value::as_u64)
                .is_some()
        })
        .unwrap_or(false);
    // **三个 regime** ✓（第 810 轮）：吃了底图 ⇒ `canvas_base` ✓；
    //   没吃底图但**读到了文档背景** ⇒ `document_background_base` ✓（第 809 轮改成比背景 ✓）；
    //   连背景都没有（老日志／无背景）⇒ `white_base` ✓ 兜底 ✓。
    //   **⇒ 为什么标签要分三种**：第 809 轮把比较基准换成背景之后，标签仍写 `white_base` 就是**错的** ✗
    //     —— 实测（128 灰底）`delta_regime` 仍报 `white_base` ✓（本轮发现 ✓）。
    value["delta_regime"] = json!(if fed_base {
        "canvas_base"
    } else if background_from_state {
        "document_background_base"
    } else {
        "white_base"
    });
    // **传感器回执**：传了才出现（不静默 ✓）—— 调用方能确认四路信号都被采纳。
    if args.get("sensors").is_some_and(|v| !v.is_null()) {
        value["sensors_applied"] = json!({
            "xtilt": sensors.xtilt,
            "ytilt": sensors.ytilt,
            "twist_deg": sensors.barrel_rotation_deg,
            "viewzoom": sensors.viewzoom,
        });
    }
    // ★ **`render_backend`：这台机器走的是哪条渲染路** ✗**（第 108 轮 ✓）
    //
    // **∴ 为什么加 ✗**：**∴ 4K 单笔的墙钟预算 100 ms ✗ 在**某些机器上物理不可达**✗
    //   ⇒ **∴ 实测 ✓**：**∴ 本机 `raster_ms` **3.47**✗；CI **165.07**✗ ⇒ **∴ **47.6 倍**✗**
    //   ⚠ **∴ 而我先前把原因写成"本机有 GPU ✗／CI 无 GPU"✗ ⇒ **∴ 已**被否掉**✗**：
    //     **∴ 两地的 `render_backend` **都是 `cpu`**✗** ⇒ **∴ 所以**真实原因**仍未定**✗**
    //     （**∴ 台账第 118 节是当时的猜测 ✓，第 120／121／123 节修正 ✓**）
    //   ⇒ **∴ 而**更硬的事实 ✓：**∴ 光栅**与面积无关**✗**（**∴ 1024² **4.68**✗／4096² **3.79**✗ ✓**）
    //     ⇒ **∴ 所以** CI 的 165 ms ✗ 也是**常量成本**✗ ⇒ **∴ 指向**同步／提交／回读**✗** ✓
    //   ⇒ **∴ 而**判据 `tool-stroke-latency-4k` **无法**从墙钟看出这一点**✗**
    //     ⇒ **∴ 于是**它**在 CI 上**稳定假红**✗**（**∴ 235／240／224 ms ✓**）✓
    // ⇒ **∴ 所以**把后端**明说出来**✗ ⇒ **∴ 判据就能**按后端决定这条预算是否适用**✗**
    //
    // **∴ 与 `medium_stroke` 的 `render_backend` **同一个函数**✗**（`backend_label` ✓）⇒ **∴ 两族一致 ✓**
    // **∴ 只报不判 ✗**：**∴ 不改变任何返回语义**✓
    value["render_backend"] = json!(backend_label());
    value["region"] = json!({"x": x0, "y": y0, "w": width, "h": height});
    // **★ P1 #12 视觉锚点（data 模式）★**（第 254 轮）
    //   需求："AI 看缩略图要能精准接下一笔，不能盲猜坐标"。文档自己说缩略图那一半**已经有了**
    //   （框架在 `ctx.commit` 时附）=> 这里补的是**结构化锚点**那一半：
    //     * `last_stroke.start/end`：这一笔的首末控制点（**精确坐标**，不用从像素里估）；
    //     * `last_stroke.bbox`：这一笔的脏区（复用上面已经算好的 region）；
    //     * `grid`：坐标标尺（`grid_spacing` 可调，缺省 200 —— 与需求文档里的示例一致）；
    //     * `canvas`：画布尺寸（AI 好把锚点映射回缩略图）。
    // **★ 问题 1（P0）：`time_ms` 对"不吃时间"的笔刷要**明确报警**，不要静默忽略 ★**（第 254 轮）
    //
    // 只有配了 `dabs_per_second` 的笔刷（或速度敏感项 `speed1_*`/`speed2_*` 非 0）才吃时间戳；
    // **默认笔刷按几何距离出 dab** ⇒ 时间戳被忽略 ✓。新功能实测的反馈是：
    // **"静默无效果是最糟糕的 UX"** ✗（用户会以为 `time_ms` 没生效 ✓）。
    // **只在用户真的给了 `time_ms` 时才查这支笔刷** ⇒ 其它情况**零开销** ✓。
    if dtimes.iter().any(|seconds| *seconds > 0.0) {
        let time_sensitive = load_brush(ctx.workspace, &brush_name)
            // **只看 `dabs_per_second`** ✓ —— 实测教训：`100%_Opaque.myb` 的
            // `dabs_per_second` 是 **0.0**（所以它**不按时间出 dab** ✓，正合用户报告 ✓），
            // 但它的 `speed1_gamma = 2.87`／`speed1_slowness = 0.04` **非零** ✗
            // => 我第一版把这三个"或"起来 => **漏报**了 ✗。
            // **`speed1_*` 只是速度映射的形状参数** ✓，光有它不等于"按时间出 dab" ✓。
            // **★ 必须**先合并 `brush_params` 的覆盖**（真机报告 P0 #1 的修法）★**
            //   只读磁盘 `.myb` 的**原始值**会**误报** ✗ —— 实测：`100%_Opaque` 磁盘上
            //   `dabs_per_second = 0` ✗，但调用方用 `brush_params` 把它设成了 **200** ✓，
            //   这时**不该**再警告 ✓（用户最想骂的就是这条误报 ✓）。
            //   覆盖失败（字段名拼错之类）就按原值判断 —— 真正的报错由落笔那条路给出 ✓。
            .map(|(_, mut brush)| {
                if let Some(params) = merged_brush_params.as_ref() {
                    let _ = apply_brush_params(&mut brush, params);
                }
                brush.get(hokusai::BrushSetting::DabsPerSecond).base_value > 0.0
            })
            // 查不到这支笔刷就别乱警告（后面会自己报"找不到笔刷" ✓）
            .unwrap_or(true);
        // **P-16：同一会话同一笔刷只报一次** ✓ —— 原来每笔都写 ✓，20,000 笔
        // 就是 20,000 条同样的警告 ✓。去重状态在 `Workspace` 上（键 = 会话 + 笔刷 ✓）⇒
        // 换会话重新报 ✓（新调用方可能没见过首次警告 ✓）。
        if !time_sensitive
            && !ctx
                .workspace
                .mark_time_warning_shown(&ctx.session, &brush_name)
        {
            value["warning"] = json!(
                "这支笔刷**不按时间出 dab** ✗：它的 `dabs_per_second` 是 0 ⇒ 它按**几何距离**出 dab，\
                 你给的时间戳**不会改变 dab 的密度**（笔迹的疏密不听时间）。\
                 **注意**：如果这支笔刷的 `speed1_*` 非零，时间仍会**通过速度映射**影响笔迹 \
                 （那是另一条通路 ✓，不是这条警告说的东西）。\
                 **想按时间出 dab** ⇒ 换一支 `dabs_per_second > 0` 的笔刷（`list_brushes` 可查），\
                 或用 `brush_params` 给这支设上（例如 `{\"dabs_per_second\": 200}` ✓）。"
            );
        }
    }
    // **★ 视觉锚点（P1 ⑫）★**：`data`（缺省）＝ 一张干净图 ＋ 结构化锚点；
    //   `dual`（两张图：干净 ＋ 带 Overlay）**本版未实现** => 明确报错，不静默当 data。
    {
        let spacing = args
            .get("grid_spacing")
            .and_then(Value::as_f64)
            .unwrap_or(200.0);
        let (canvas_w, canvas_h) = ctx
            .workspace
            .document_mut(&ctx.doc_id)
            .map(|document| (document.state().width, document.state().height))
            .unwrap_or((0, 0));
        value["anchors"] = json!({
            "mode": "data",
            "last_stroke": {
                "start": points.first().map(|(x, y, _)| json!([x, y])).unwrap_or(Value::Null),
                "end": points.last().map(|(x, y, _)| json!([x, y])).unwrap_or(Value::Null),
                "bbox": [x0, y0, width, height],
            },
            "grid": {"spacing": spacing, "origin": [0, 0]},
            "canvas": {"width": canvas_w, "height": canvas_h},
            "note": "锚点是**画布坐标**；缩略图与画布等比缩放，按 canvas 尺寸换算即可",
        });
    }
    // **再报出这一笔是否吃了底图**（第 215 轮）：判据据此分类比对 ✓
    //（吃了 ⇒ 比"喂同样底图"；没吃 ⇒ 比"空表面"✓），不再自己猜名单 ✓。
    value["fed_base"] = json!(fed_base);
    // **部分越界要说出来** ✗（不静默 ✓）：否则调用方会以为"我要的那一片都画到了" ✓。
    value["clipped"] = json!(clipped);
    if clipped {
        value["warnings"] = json!([format!(
            "笔迹有一部分在画布外 ⇒ 已被裁掉 ✓（画布 {doc_width}×{doc_height}，笔迹范围 \
             x {x0}..{x1}、y {y0}..{y1}）—— 画布外的墨不会落上，这不是坐标偏移 ✗"
        )]);
    }
    // **B4：传了却不生效的参数进 `warnings`**（与上面"部分越界"那条同一形态：已有就 push，没有就建）。
    //   P-19 之后只有 `color_to.a` 还被忽略 ✓：`color.a` 已乘进 opacity ✓；
    //   渐变只用单一 opacity，末端 alpha 没处放 ✓ ⇒ 仍警告，不静默 ✓。
    let mut b4_notes: Vec<String> = Vec::new();
    if let Some(note) = envelope_warning {
        b4_notes.push(note);
    }
    if args.get("color_to").is_some_and(color_has_alpha) {
        b4_notes.push(
            "`color_to.a` 目前被忽略：渐变只用单一 opacity，透明度请用 `opacity` 参数".to_owned(),
        );
    }
    if !b4_notes.is_empty() {
        match value.get_mut("warnings") {
            Some(Value::Array(items)) => items.extend(b4_notes.into_iter().map(|note| json!(note))),
            _ => value["warnings"] = json!(b4_notes),
        }
    }
    // **反馈期 1（3.6a）：短笔触兜底提示** —— 路径撑不满 dab 间距时不再拒绝，
    // 而是延长落一枚 dab；诊断事实（原长／间距／size）降为 `warnings` 里的一条提示，
    // 与“部分越界”／B4 同形态（已有就 push，没有就建）。
    // 旧拒绝文案仍留在 `paint_brush` 的 0 墨分支里，那是其它真 0 墨情形（透明／涂抹／不可见色）
    // 的最后防线，不是本兜底的残留。
    if let Some(hint) = short_stroke_hint {
        match value.get_mut("warnings") {
            Some(Value::Array(items)) => items.push(json!(hint)),
            _ => value["warnings"] = json!([hint]),
        }
    }
    // **D139 的静默零墨：印章落了、画面没变** ✓（第 811 轮；探针确认此处两个值已就位 ✓）
    //   **⇒ 实测（他们的调用）**：`ink_footprint_pixels=6`、`max_channel_delta=0` ✓
    //     ⇒ 账面上"落了 6 个像素的墨" ✓ 而**画面一个像素都没变** ✗ ⇒ `ok=true` 且无提醒 ✗。
    //   **⇒ 为什么上一轮发不出来**：那段断点当时**不在编译进来的路径上** ✓（我先移动、又删过 ✓）——
    //     探针本轮证实：**在这一刻** `max_delta == 0` 且 `ink_footprint_pixels == 6` ✓ ⇒ 条件成立 ✓。
    //   **⇒ 所以**：`footprint > 0 且 delta == 0` ⇒ **响亮警告** ✓（不是错误 ✓：参数本身合法 ✓）。
    if value["ink_footprint_pixels"].as_u64().unwrap_or(0) > 0 && max_delta == 0 {
        // **用 `warnings` 数组，与"部分越界"那条同一形态** ✓（第 812 轮；外部 §7 第 10 条的同一诉求）
        //   **⇒ 缺陷**：我原来写 `value["warning"]`（单数字段 ✓）而上面"越界"那条写 `warnings`（数组 ✓）
        //     ⇒ **同一个响应里两种形态** ✗ ⇒ 调用方要写两条分支 ✓（正是"没有公共词汇" ✓）。
        //   **⇒ 修法**：**都进 `warnings` 数组** ✓（已有就 push ✓，没有就建 ✓）。
        let note = format!(
            "这一笔**落了 {} 个像素的墨，但画面一个像素都没变**（`max_channel_delta = 0`，基准见 `delta_regime`）\
             ⇒ 常见原因：① **太细**（`size` 小于一像素覆盖）② **太透明**（`opacity`／笔刷自带 opacity 太低）\
             ③ **颜色与背景几乎相同**（`color` 与文档背景差得太少）\
             **判据：`ink_footprint_pixels > 0` 只说明「墨落在足迹上」；`max_channel_delta = 0` 才说明「看不见」** ✓",
            value["ink_footprint_pixels"]
        );
        match value.get_mut("warnings") {
            Some(Value::Array(items)) => items.push(json!(note)),
            _ => value["warnings"] = json!([note]),
        }
    }
    // **P2-8：`#RRGGBBAA` 的 alpha 被静默丢掉** ✓（期 2 块 2）：
    //   画笔颜色只取 RGB（`brush_color_to_srgb_bytes` 回 `[u8; 3]`）⇒ 8 位 hex 的
    //   alpha 在此丢掉；B4 的回显只认对象式 `a` 键与数组 4 元，hex 写法没信号 ✗。
    //   ⇒ 半透明 alpha（< 255）时进 `warnings`，说清"被忽略、透明度请用 `opacity`" ✓；
    //   不透明 `FF` 与 6 位 hex 不报（没损失信息 ✓）。
    for (key, source) in [("color", &color_source), ("color_to", &color_to_source)] {
        if let Some(alpha) = hex_alpha_dropped(source) {
            let note = format!(
                "`{key}` 用的 `#RRGGBBAA` 带半透明 alpha（{alpha}/255）⇒ 画笔颜色只取 RGB，\
                 这个 alpha **被忽略**（画面不会变透明）；透明度请用 `opacity` 参数"
            );
            match value.get_mut("warnings") {
                Some(Value::Array(items)) => items.push(json!(note)),
                _ => value["warnings"] = json!([note]),
            }
        }
    }
    Ok(value)
}

/// **智能填充**：输入形状 + 主方向，自动生成带人味的排线填充。
///
/// 与 `fill_region` 的区别：`fill_region` 是 flat 的塑料感填充；
/// `smart_fill` 是一笔一笔用真实笔刷画出来的排线，每条线带方向抖动 +
/// `hand_tremor` + `pressure_shape`，像真人排线。
///
/// smart_* 共享：带孔形状解析（② 带孔形状的基础设施）。
///
/// 把原来散落在 5 个 `write_smart_*` 里的 shape 解析收敛到一处，
/// 并支持 `"holes"`（outer − inner）：`contains()` 自动减去孔洞。
/// 排线类工具用 `contains()` 做游程切分时，穿过孔洞的线会在孔边缘
/// 自然断开成独立笔触（带起收压感），而不是被选区硬裁成刀切边。
#[derive(Debug, Clone)]
pub enum PrimShape {
    /// 轴对齐矩形：左上角 `(x, y)`，宽 `w`、高 `h`。
    Rect {
        /// 左上角 x。
        x: f64,
        /// 左上角 y。
        y: f64,
        /// 宽。
        w: f64,
        /// 高。
        h: f64,
    },
    /// 椭圆：中心 `(cx, cy)`，半轴 `rx`、`ry`（`rx == ry` 时即圆）。
    Ellipse {
        /// 中心 x。
        cx: f64,
        /// 中心 y。
        cy: f64,
        /// x 半轴。
        rx: f64,
        /// y 半轴。
        ry: f64,
    },
    /// 任意多边形：顶点序列，隐式闭合。
    Polygon {
        /// 顶点序列。
        points: Vec<(f64, f64)>,
    },
    /// 任意路径（曲线/路径一、二期的数据模型）：结构化子路径 + 填充规则。
    ///
    /// `rings` 是构造时用**同一份展平**（`path::flatten_subpath`）得到的
    /// 折线环，`contains` 走**同一份多环填充判定**（`path::rings_fill_contains`，
    /// 即渲染层 `path_fill_contains` 的后半段）——工具层不另起展平/命中实现。
    Path {
        /// 结构化子路径（已过 `object::parse_path_subpaths` 校验）。
        subpaths: Vec<yanshi_render::path::Subpath>,
        /// 填充规则。
        fill_rule: yanshi_render::path::FillRule,
        /// 展平折线环（与 `subpaths` 一一对应，构造时派生，不单独落库）。
        rings: Vec<Vec<(f64, f64)>>,
    },
}

impl PrimShape {
    /// 由结构化子路径构造 path 形状：展平只在这里做一次（同一份
    /// `path::flatten_subpath`），之后 contains/bbox/area 都读折线环。
    pub fn from_path(
        subpaths: Vec<yanshi_render::path::Subpath>,
        fill_rule: yanshi_render::path::FillRule,
    ) -> PrimShape {
        let rings = subpaths
            .iter()
            .map(|sub| yanshi_render::path::flatten_subpath(sub, yanshi_render::path::FLATTEN_TOL))
            .collect();
        PrimShape::Path {
            subpaths,
            fill_rule,
            rings,
        }
    }

    /// 外接盒 `(x0, y0, x1, y1)`。
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        match self {
            PrimShape::Rect { x, y, w, h } => (*x, *y, x + w, y + h),
            PrimShape::Ellipse { cx, cy, rx, ry } => (cx - rx, cy - ry, cx + rx, cy + ry),
            PrimShape::Polygon { points } => {
                if points.is_empty() {
                    (0.0, 0.0, 100.0, 100.0)
                } else {
                    let mut x0 = f64::MAX;
                    let mut y0 = f64::MAX;
                    let mut x1 = f64::MIN;
                    let mut y1 = f64::MIN;
                    for (px, py) in points {
                        x0 = x0.min(*px);
                        y0 = y0.min(*py);
                        x1 = x1.max(*px);
                        y1 = y1.max(*py);
                    }
                    (x0, y0, x1, y1)
                }
            }
            PrimShape::Path { rings, .. } => {
                let mut x0 = f64::INFINITY;
                let mut y0 = f64::INFINITY;
                let mut x1 = f64::NEG_INFINITY;
                let mut y1 = f64::NEG_INFINITY;
                for ring in rings {
                    for (px, py) in ring {
                        x0 = x0.min(*px);
                        y0 = y0.min(*py);
                        x1 = x1.max(*px);
                        y1 = y1.max(*py);
                    }
                }
                if x0.is_finite() {
                    (x0, y0, x1, y1)
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                }
            }
        }
    }

    /// 点是否在形状内。退化多边形（<3 点）退化为外接盒判定（与旧行为一致）。
    pub fn contains(&self, px: f64, py: f64) -> bool {
        match self {
            PrimShape::Rect { x, y, w, h } => px >= *x && px <= x + w && py >= *y && py <= y + h,
            PrimShape::Ellipse { cx, cy, rx, ry } => {
                if *rx <= 0.0 || *ry <= 0.0 {
                    return false;
                }
                let dx = (px - cx) / rx;
                let dy = (py - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            PrimShape::Polygon { points } => {
                if points.len() < 3 {
                    let (bx0, by0, bx1, by1) = self.bbox();
                    return px >= bx0 && px <= bx1 && py >= by0 && py <= by1;
                }
                // Ray casting
                let mut inside = false;
                let n = points.len();
                let mut j = n - 1;
                for i in 0..n {
                    let (xi, yi) = points[i];
                    let (xj, yj) = points[j];
                    if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
                        inside = !inside;
                    }
                    j = i;
                }
                inside
            }
            PrimShape::Path {
                rings, fill_rule, ..
            } => {
                let refs: Vec<&[(f64, f64)]> = rings.iter().map(Vec::as_slice).collect();
                yanshi_render::path::rings_fill_contains(&refs, *fill_rule, (px, py))
            }
        }
    }

    /// 形状面积（rect 精确；ellipse π·rx·ry；polygon 鞋带公式）。
    pub fn area(&self) -> f64 {
        match self {
            PrimShape::Rect { w, h, .. } => w.max(0.0) * h.max(0.0),
            PrimShape::Ellipse { rx, ry, .. } => std::f64::consts::PI * rx.max(0.0) * ry.max(0.0),
            PrimShape::Polygon { points } => {
                let n = points.len();
                if n < 3 {
                    return 0.0;
                }
                let mut a = 0.0;
                for i in 0..n {
                    let (x0, y0) = points[i];
                    let (x1, y1) = points[(i + 1) % n];
                    a += x0 * y1 - x1 * y0;
                }
                a.abs() / 2.0
            }
            PrimShape::Path {
                subpaths,
                fill_rule,
                ..
            } => {
                // 面积 = 填充覆盖率之和，走渲染层同一条光栅路
                // （`geometry::path_coverage_clipped`，与 filled_pixels 同口径）。
                // 不用鞋带公式：边界自交的路径（波浪线 + 闭合弦那种双叶形）
                // 带符号鞋带会把两叶相减，而填充语义要的是两叶之和。
                let (x0, y0, x1, y1) = self.bbox();
                if x1 <= x0 || y1 <= y0 {
                    return 0.0;
                }
                let clip = yanshi_core::Bbox::new(x0.floor(), y0.floor(), x1.ceil(), y1.ceil());
                let coverage =
                    yanshi_render::geometry::path_coverage_clipped(subpaths, 4, &clip, *fill_rule);
                coverage.data.iter().map(|v| f64::from(*v)).sum()
            }
        }
    }

    /// 与旧代码 `shape_type` 字符串的对应：rect / circle / ellipse / polygon。
    /// circle 是 rx==ry 的 ellipse 的别名（旧 smart_* 只认 circle）。
    pub fn kind_str(&self) -> &'static str {
        match self {
            PrimShape::Rect { .. } => "rect",
            PrimShape::Ellipse { rx, ry, .. } => {
                if (rx - ry).abs() < 1e-9 {
                    "circle"
                } else {
                    "ellipse"
                }
            }
            PrimShape::Polygon { .. } => "polygon",
            PrimShape::Path { .. } => "path",
        }
    }

    /// 外层为多边形时返回顶点（非多边形返回空）。
    pub fn polygon_points(&self) -> Vec<(f64, f64)> {
        match self {
            PrimShape::Polygon { points } => points.clone(),
            _ => vec![],
        }
    }
}

// ===== 同族追加（第 1270 轮）：paint_brush + apply_brush_style =====

/// **涂抹笔触的成本预算** ✓（P-4）：`smudge > 0` 且拖曳长度 > 0 的笔，印章成本 ≈
/// 路径长 × 半径² × k（每步在 `(2r)²` 盘里逐像素涂抹；k 由修后实测标定，见下）。
/// 预测超过 30 秒 ⇒ 报 `invalid_argument`（不静默跑 7 分钟），并写明数值与改法。
pub(super) const SMUDGE_BUDGET_MS: f64 = 30_000.0;
/// 标定（2026-10-28，修后，yanshi-4）：watercolor-02 size 100/200/300 给
/// 2.57e-4～2.71e-4 ms/px³；取 3.5e-4 留约 30% 余量（盖住压力导致的半径上浮）。
pub(super) const SMUDGE_COST_PER_PX3_MS: f64 = 3.5e-4;

/// 涂抹印章的预测耗时（毫秒）：路径长（px）× 半径²（px²）× k。
pub(super) fn smudge_stroke_cost_ms(path_len_px: f64, radius_px: f64) -> f64 {
    path_len_px * radius_px * radius_px * SMUDGE_COST_PER_PX3_MS
}

/// **拆分安全系数** ✓（反馈期 4）：段长上限 = 预算 ÷ (r² × k) × 本系数。
/// 0.9 吸收压力致半径上浮（k 本身已有约 30% 余量，见 `SMUDGE_COST_PER_PX3_MS` 注释；
/// 系数再小就会把段数虚增一个量级，0.9 是“刚好盖住上浮”的取舍）。
pub(super) const SMUDGE_SPLIT_SAFETY: f64 = 0.9;
/// **拆分段数上限** ✓（反馈期 4）：段数超过它 ⇒ 拆分无意义，保留原 Err 路径。
/// 64 段在半径 130px 时覆盖约 29 万 px 路径（整幅 4K 对角线的 50 倍以上）；
/// 超过它的长笔不是“涂抹一笔”，而是调用方该分批的作业。
pub(super) const SMUDGE_SPLIT_MAX_SEGMENTS: usize = 64;
/// **拆分段长下限** ✓（反馈期 4）：段长上限低于它 ⇒ 段比一枚 dab 间距还短，
/// 拆出来可能是 0-dab 空段 ⇒ 不拆分，保留原 Err 路径。
pub(super) const SMUDGE_SPLIT_MIN_SEGMENT_PX: f64 = 100.0;

/// **拆分计划** ✓（反馈期 4）：超预算长笔按预算切成的段数。
/// 均分（而非按上限切出末段余数）⇒ 各段成本均衡，且每段成本 ≤ 预算 × 安全系数。
pub(super) struct SmudgeSplitPlan {
    /// 段数（≥ 2）。
    pub segments: usize,
}

/// **拆分段长上限** ✓：预算 ÷ (r² × k) × 安全系数（毫秒与 k 的单位约掉，得 px）。
pub(super) fn smudge_split_segment_limit_px(radius_px: f64) -> f64 {
    SMUDGE_BUDGET_MS / (radius_px * radius_px * SMUDGE_COST_PER_PX3_MS) * SMUDGE_SPLIT_SAFETY
}

/// **拆分计划** ✓（反馈期 4；纯函数，可单测）：
/// * 预算内 ⇒ `None`（不拆分，走原单笔路）；
/// * 段长上限 < 下限，或段数 > 上限 ⇒ `None`（退化，保留原 Err 路径）；
/// * 否则 `Some`（段数 = 向上取整，段长 = 路径均分 ⇒ 每段成本 ≤ 预算 × 0.9）。
pub(super) fn smudge_split_plan(path_len_px: f64, radius_px: f64) -> Option<SmudgeSplitPlan> {
    if path_len_px <= 0.0 || radius_px <= 0.0 {
        return None;
    }
    if smudge_stroke_cost_ms(path_len_px, radius_px) <= SMUDGE_BUDGET_MS {
        return None;
    }
    let limit = smudge_split_segment_limit_px(radius_px);
    if limit < SMUDGE_SPLIT_MIN_SEGMENT_PX {
        return None;
    }
    let segments = (path_len_px / limit).ceil() as usize;
    if !(2..=SMUDGE_SPLIT_MAX_SEGMENTS).contains(&segments) {
        return None;
    }
    Some(SmudgeSplitPlan { segments })
}

/// **涂抹可达量** ✓（反馈期 4；与期 2 同口径）：
/// `drag + size/2 + 2`，其中 `drag` = 求值后 `smudge_length` × 半径（与
/// `check_smudge_budget` 里的拖曳算法逐字一致），`size/2` = 半径。
/// 预算闸不生效的笔（`smudge = 0` 或拖曳 0）⇒ `None`（拆分决策不调用它）。
pub(super) fn smudge_reach_px(brush: &hokusai::Brush) -> Option<f64> {
    let smudge_amount = brush.get(hokusai::BrushSetting::Smudge).base_value;
    if smudge_amount <= 0.0 {
        return None;
    }
    let radius_px = f64::from(brush.get(hokusai::BrushSetting::Radius).base_value).exp();
    let drag_length =
        radius_evaluated_value(brush, hokusai::BrushSetting::SmudgeLength).max(0.0) * radius_px;
    if drag_length <= 0.0 {
        return None;
    }
    Some(drag_length + radius_px + 2.0)
}

/// **按累计弧长均分点列** ✓（反馈期 4；纯函数，可单测）：
/// 每段 = 接点（起）＋ 段内全部原点 ＋ 接点（止）；相邻段**共享接点**
/// （接点坐标与压力线性插值 ⇒ 不留缝隙，折线不被拉直）；
/// 零长腿（连续重复点）跳过，不除零；`segments < 2` 或总长 0 ⇒ 原样返回一整段。
pub(super) fn split_points_by_arclength(
    points: &[(f64, f64, f64)],
    segments: usize,
) -> Vec<Vec<(f64, f64, f64)>> {
    if segments < 2 || points.len() < 2 {
        return vec![points.to_vec()];
    }
    let mut cumulative = Vec::with_capacity(points.len());
    cumulative.push(0.0);
    for w in points.windows(2) {
        let leg = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
        cumulative.push(cumulative.last().copied().unwrap_or(0.0) + leg);
    }
    let total = cumulative.last().copied().unwrap_or(0.0);
    if total <= 0.0 {
        return vec![points.to_vec()];
    }
    let at = |distance: f64| -> (f64, f64, f64) {
        if distance <= 0.0 {
            return points[0];
        }
        if distance >= total {
            return points[points.len() - 1];
        }
        for i in 0..points.len() - 1 {
            let (a, b) = (cumulative[i], cumulative[i + 1]);
            if b > a && distance >= a && distance <= b {
                let t = (distance - a) / (b - a);
                return (
                    points[i].0 + (points[i + 1].0 - points[i].0) * t,
                    points[i].1 + (points[i + 1].1 - points[i].1) * t,
                    points[i].2 + (points[i + 1].2 - points[i].2) * t,
                );
            }
        }
        points[points.len() - 1]
    };
    (0..segments)
        .map(|i| {
            let start = total * i as f64 / segments as f64;
            let end = total * (i + 1) as f64 / segments as f64;
            let mut seg = vec![at(start)];
            // 段内原点（严格在起止之间 ⇒ 接点恰落原点时不重复进段）。
            for (j, point) in points.iter().enumerate() {
                if cumulative[j] > start && cumulative[j] < end {
                    seg.push(*point);
                }
            }
            seg.push(at(end));
            seg
        })
        .collect()
}

/// **点列时间按同一弧长切分** ✓（反馈期 4；与点列切分同接点、同插值）：
/// 长度与点列不对齐（平滑改了点数时）⇒ `None`（调用方改传空切片 ⇒ 距离步进老行为）。
pub(super) fn split_dtimes_by_arclength(
    points: &[(f64, f64, f64)],
    dtimes: &[f64],
    segments: usize,
) -> Option<Vec<Vec<f64>>> {
    if dtimes.len() != points.len() {
        return None;
    }
    let triples: Vec<(f64, f64, f64)> = points
        .iter()
        .zip(dtimes.iter())
        .map(|(&(x, y, _), &t)| (x, y, t))
        .collect();
    Some(
        split_points_by_arclength(&triples, segments)
            .into_iter()
            .map(|seg| seg.into_iter().map(|(_, _, t)| t).collect())
            .collect(),
    )
}

/// **折线总长** ✓（与 `check_smudge_budget` 里的路径算法逐字一致）。
pub(super) fn brush_points_path_len_px(points: &[(f64, f64, f64)]) -> f64 {
    points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

/// **每步涂抹量（D 期，项 14）**：`stamp_stroke_from` 每 2px 插值步调一次
/// `smudge_dab`，而引擎每 dab 间距才盖一枚 dab ⇒ 旧实现把一名义量施加了
/// `间距 / 2px` 倍（水彩 size 300：间距 18.75px ⇒ 约 9.4 倍），且每像素累计
/// 步数 = 直径 / 2px 随半径增长 ⇒ 大笔拖曳更重（探针列均变异系数 size 300
/// 为 0.38、size 600 为 0.76 的尺寸 scaling 根源）。
/// 归一：`amount_step = 1 - (1 - amount)^(2 / spacing)`，使每 dab 间距内的
/// 累计拖曳恰为一名义量（与间距无关 ⇒ 与尺寸无关；`amount = 0/1` 时恒等）。
/// `spacing` 取引擎 `count_dabs_to` 的同一口径：`dpar / actual_radius +
/// dpbr / base_radius`（`dabs_per_second` 项：三支涉事笔刷皆为 0，见 .myb；
/// 其它笔刷若有时间项，本式只计距离项，量取下界，不放大）。
/// `actual_radius = base_radius = radius_px`（引擎注记：实际半径不受随机扰动）。
/// 边界：`spacing ≤ 0` 或两率皆 0（引擎不盖 dab）时返回原量，不放大不缩小。
pub(super) fn smudge_step_amount(amount: f64, radius_px: f64, brush: &hokusai::Brush) -> f64 {
    if amount <= 0.0 || amount >= 1.0 || radius_px <= 0.0 {
        return amount.clamp(0.0, 1.0);
    }
    let dpar = f64::from(
        brush
            .get(hokusai::BrushSetting::DabsPerActualRadius)
            .base_value,
    );
    let dpbr = f64::from(
        brush
            .get(hokusai::BrushSetting::DabsPerBasicRadius)
            .base_value,
    );
    let dabs_per_px = (dpar + dpbr) / radius_px;
    if dabs_per_px <= 0.0 || !dabs_per_px.is_finite() {
        return amount;
    }
    let spacing_px = 1.0 / dabs_per_px;
    1.0 - (1.0 - amount).powf(2.0 / spacing_px)
}

/// 涂抹预算检查 ✓：只对「涂抹真的会跑」的笔生效（`smudge > 0` 且拖曳长度 > 0）。
/// `smudge_length = 0` 的笔（如 oil-01-paint）涂抹是空操作，不在此列。
/// 半径与涂抹量都取**覆盖后**的笔刷值（`size`／`brush_params` 已应用）。
pub(super) fn check_smudge_budget(
    brush: &hokusai::Brush,
    points: &[(f64, f64, f64)],
    size: Option<f64>,
) -> Result<()> {
    let smudge_amount = brush.get(hokusai::BrushSetting::Smudge).base_value;
    if smudge_amount <= 0.0 {
        return Ok(());
    }
    let radius_px = f64::from(brush.get(hokusai::BrushSetting::Radius).base_value).exp();
    // 拖曳长度取求值后值（`radius_evaluated_value`）：`brush_radius` 曲线
    // 要算进去，大尺寸下它等于 base，小尺寸下它更大——预算的闸门与报错数字
    // 都要与实际跑的拖曳一致。
    let drag_length =
        radius_evaluated_value(brush, hokusai::BrushSetting::SmudgeLength).max(0.0) * radius_px;
    if drag_length <= 0.0 {
        return Ok(());
    }
    let path_len: f64 = points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum();
    let predicted_ms = smudge_stroke_cost_ms(path_len, radius_px);
    if predicted_ms > SMUDGE_BUDGET_MS {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "这一笔涂抹成本太高：预测约 {:.0} 秒（路径 {:.0} px × 半径 {:.0} px，预算 30 秒）⇒ \
                 涂抹笔刷的印章成本随半径平方增长。改法（任选）：把 size 从 {:.0} 改小，\
                 或把路径拆成几笔，或把 brush_params.smudge_length 改小。\
                 （smudge = {smudge_amount}，拖曳长度 {drag_length:.1} px）",
                predicted_ms / 1000.0,
                path_len,
                radius_px,
                size.unwrap_or(radius_px * 2.0),
            )),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn paint_brush(
    ctx: &mut ToolContext<'_>,
    brush_name: &str,
    points: &[(f64, f64, f64)],
    size: Option<f64>,
    color: Option<&Value>,
    // **末端颜色** ✓（给了 ⇒ 沿笔迹从 `color` 渐变到它 ✓ = Loaded Brush ✓）。
    color_to: Option<&Value>,
    // **这一笔的不透明度** ✓（`0..1`；`None` = 用 `.myb` 自带的 ✓）—— 落到 MyPaint 的 `opaque` ✓。
    opacity: Option<f64>,
    // **笔尖硬度** ✓（`0..1`；`None` = 用 `.myb` 自带的 ✓）—— 落到 MyPaint 的 `hardness` ✓。
    hardness: Option<f64>,
    feed_base: bool,
    brush_params: Option<&Value>,
    // **★ P2 #16：`color_to` 渐变的混色模式 ★**（缺省 `Rgb` ⇒ 与今天逐字节相同）
    color_mixing: yanshi_render::color::ColorMixing,
    // **★ P0 #3：每个点的真实时间步长（秒）★**（空切片 ⇒ 老行为不变）
    dtimes: &[f64],
    background: Option<[u8; 4]>,
    // **真实传感器**（缺省全 0 ⇒ 与历史行为逐位一致；xtilt/ytilt 在落笔时传给
    // `stamp_stroke_from`，viewzoom / barrel_rotation 在下面写到 Brush 宿主字段）。
    sensors: BrushSensors,
    // **显式颜色单位**（C1）：`Some` ⇒ 按该空间解释 `color` / `color_to`；
    // `None` ⇒ 量级切换（缺省，与以前逐位一致）。
    color_space: Option<yanshi_render::color::ColorSpace>,
) -> Result<BrushPaint> {
    let (name, mut brush) = load_brush(ctx.workspace, brush_name)?;
    brush.viewzoom = sensors.viewzoom;
    brush.barrel_rotation = sensors.barrel_rotation_deg;
    if let Some(opacity) = opacity {
        brush.set(
            hokusai::BrushSetting::Opaque,
            hokusai::SettingValue::constant(opacity.clamp(0.0, 1.0) as f32),
        );
    }
    if let Some(hardness) = hardness {
        brush.set(
            hokusai::BrushSetting::Hardness,
            hokusai::SettingValue::constant(hardness.clamp(0.0, 1.0) as f32),
        );
    }
    // **`size` 是"直径像素"** ✓，而 MyPaint 的设置叫 `radius_logarithmic` ✓（存的是 ln(半径) ✓）。
    // **颜色与 `size` 相互独立** ✓（本轮修的「ok 却没有效果」✗ 之一：
    // 颜色覆盖此前**嵌在 `if let Some(diameter)` 里面** ✓ ⇒ 只给 `color` 不给 `size` 时
    // **颜色被静默丢掉** ✗，而描述明明写着"可带 color 画彩色" ✓ ⇒ 现在两者独立 ✓，
    // 判据 `brush_stroke_colours_without_a_size` ✓）。
    // **为什么颜色用 `constant` 是对的** ✓（与半径那处不同 ✗）：调用方给的是"**这一笔用什么颜色**" ✓
    // ⇒ 它就是**恒定色** ✓ ⇒ 覆盖 `.myb` 里可能有的"颜色随压力变化"曲线 ✓ **正是本意** ✓
    //（而半径那处不该清曲线 ✓ —— 两者语义不同 ✓）。
    if let Some(value) = color {
        if !value.is_null() {
            let (hue, sat, value) = brush_color_to_hsv_with_space(value, color_space)?;
            brush.set(
                hokusai::BrushSetting::ColorH,
                hokusai::SettingValue::constant(hue),
            );
            brush.set(
                hokusai::BrushSetting::ColorS,
                hokusai::SettingValue::constant(sat),
            );
            brush.set(
                hokusai::BrushSetting::ColorV,
                hokusai::SettingValue::constant(value),
            );
        }
    }
    if let Some(diameter) = size {
        if diameter <= 0.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("size 必须大于 0"),
            ));
        }
        // **`SettingValue` 是个结构体，不是枚举** ✗ —— 构造器是 `constant(f32)` ✓
        //（我第一版猜了 `Float(..)` ✓ ⇒ 编译器当场指出 ✓）。
        brush.set(
            hokusai::BrushSetting::Radius,
            // **必须用 `libm::log`，不能用 `.ln()`**（第 200 轮）：
            // `.ln()` 落到系统 libm（glibc）⇒ 与内核的 `libm::log` **差最后一位** ⇒
            // 半径差一位 ⇒ 每枚 dab 形状不同 ⇒ 全量判据 389/796 次不同。
            // 内核注释早记过这条教训（"这一层里也曾有 `.ln(` 与 `.powi(2)`"）。
            hokusai::SettingValue::constant(libm::log(diameter / 2.0) as f32),
        );
    }

    // **反馈期 1（3.6a）：短笔画单 dab 兜底** —— 原路径撑不满一枚 dab 间距时，
    // 轻点至少落一枚 dab，不再 0 落墨报错；诊断事实降为回执里的提示（见 `short_stroke_hint`）。
    // 间距口径：引擎每 dab 的位移 ≈ 半径 / (dabs_per_actual_radius + dabs_per_basic_radius)
    // （`count_dabs_to`：dist/radius*dpar + dist/base_radius*dpbr；起笔时 actual=base）。
    // 延长到 1.5× 间距（留余量对冲压感曲线造成的半径变化）；视觉上仍是一次轻点，不是拉出一条线。
    // 落点语义：起笔处不动（`extend_short_path`：多点沿末段方向延长，单点沿 +x 延长）。
    // 总长 ≥ 间距的正常笔画原样不动。只对会落墨的笔生效（opaque > 0）：
    // 透明笔（P-19）与纯涂抹笔的 0 墨仍是正确行为，不由这里兜底。
    // 注意：`brush_params` 在下面才事后覆盖（它能改 dabs/opaque），本处用的是覆盖前的值；
    // 覆盖后才把间距改大的笔，兜底可能不够，仍走下面的 0 墨分支（残留，见包说明）。
    // 位置：必须在区域计算之前（延长后的端点要进区域），且在 `size` 已写入半径之后。
    let owned_points: Vec<(f64, f64, f64)>;
    let mut short_stroke_hint: Option<String> = None;
    let points: &[(f64, f64, f64)] = {
        let radius = f64::from(brush.get(hokusai::BrushSetting::Radius).base_value).exp();
        let opaque = brush.get(hokusai::BrushSetting::Opaque).base_value;
        let dabs_sum = f64::from(
            brush
                .get(hokusai::BrushSetting::DabsPerActualRadius)
                .base_value,
        ) + f64::from(
            brush
                .get(hokusai::BrushSetting::DabsPerBasicRadius)
                .base_value,
        );
        let total_len: f64 = points
            .windows(2)
            .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
            .sum();
        if radius.is_finite()
            && radius > 0.0
            && opaque > 0.0
            && dabs_sum > 0.0
            && total_len.is_finite()
            && total_len < radius / dabs_sum
        {
            let spacing = radius / dabs_sum;
            match extend_short_path(points, spacing, spacing * 1.5) {
                Some(extended) => {
                    let n = points.len();
                    let size_text = match size {
                        Some(sz) => format!("{sz}"),
                        None => format!("{:.1}", radius * 2.0),
                    };
                    short_stroke_hint = Some(format!(
                        "短笔触兜底：原路径太短（{n} 个点、路径总长 {total_len:.1}px，撑不满一个 dab 间距约 {spacing:.1}px，size={size_text}）⇒ 起笔处不动、沿末段方向延长至 {:.1}px，已落下一枚 dab；这不代表整支笔刷不可用，想要更长的笔触请加长路径",
                        spacing * 1.5
                    ));
                    owned_points = extended;
                    &owned_points
                }
                None => points,
            }
        } else {
            points
        }
    };

    // **P-19：`color.a` 乘进有效 opacity** ✓ —— 此前 `a` 只进 warnings，不进渲染 ✓。
    //   `a=255`（或无 `a`）⇒ 系数 1.0 ⇒ 这里**什么都不做** ✓（与修前逐位相同，
    //   连 `.myb` 里 `opaque` 随压力的曲线都不动 ✓）；`a<255` ⇒ 整笔按比例变透明 ✓。
    //   `opacity` 没给时 ⇒ 以笔刷自带的 `opaque` 为底再乘系数 ✓（与显式 `opacity` 同例 ✓）。
    //   与显式 `opacity` 一样 ✓：设了就是常量 ⇒ 会压平 `.myb` 里 `opaque` 随压力的曲线 ✓；
    //   而后来的 `brush_params{opaque}` 仍优先（它最后应用 ✓）。
    //   `color_to.a` 仍被忽略（渐变只用单一 opacity ✓，见下面 B4 那条）⇒ 警告保留 ✓。
    if let Some(factor) = color.and_then(|value| color_alpha_factor(value, color_space)) {
        if factor < 1.0 {
            let base = match opacity {
                Some(given) => given,
                None => brush.get(hokusai::BrushSetting::Opaque).base_value as f64,
            };
            brush.set(
                hokusai::BrushSetting::Opaque,
                hokusai::SettingValue::constant((base * factor).clamp(0.0, 1.0) as f32),
            );
        }
    }

    let mut state = hokusai::BrushState::default();
    prime_stroke_total_length(&mut state, points);
    let mut surface = hokusai::tile_mem::MemSurface::new();
    // **区域要在落笔之前就算出来** ✗ —— 先在下面算了一遍 ✓，才能把**底图**喂进 surface ✓。
    // **同一个公式只写一份** ✓（抽成 `brush_stroke_region` ✓）：落笔前喂底图用它 ✓、
    // 落笔后读回像素也用它 ✓ ⇒ **两处不可能算出不同的区域** ✗（那会让底图与结果错位 ✓）。
    // **先裁到画布内** ✓ —— 否则底图会因为"尺寸对不上"被整块跳过 ✓
    // ⇒ 靠近边缘的涂抹会**静默退化成没东西可抹** ✗（第 33 轮实测 ✓）。
    let (canvas_width, canvas_height) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|document| (document.state().width, document.state().height))
        .unwrap_or((0, 0));
    let seed_region = PaintRegion::from_bounds(brush_stroke_region(points, size))
        .clamped(canvas_width, canvas_height);
    // **预览的底色 ✗**（外部复测 QY-G02）：给了就先把 seed_region 铺满它
    // ⇒ 预览能反映"落在深色底上是什么样"，而不是永远白底
    if let Some([r, g, b, a]) = background {
        let (w, h) = (seed_region.width as i32, seed_region.height as i32);
        if w > 0 && h > 0 {
            let mut flat = Vec::with_capacity(w as usize * h as usize * 4);
            for _ in 0..(w * h) {
                flat.extend_from_slice(&[r, g, b, a]);
            }
            yanshi_render::brush::feed_base(
                &mut surface,
                seed_region.x0,
                seed_region.y0,
                w,
                h,
                &flat,
            );
        }
    }
    // **先把图层现有的像素喂进 surface** ✓ —— 这是**涂抹类笔刷**能不能工作的关键 ✓。
    //
    // **实测诊断** ✓：`ramon-Knife` 一直"画不出东西" ✗，我先前以为是坏笔刷 ✗、还写了"换一支笔刷" ✗。
    // 与能画的 `classic-knife` **逐字段对比**之后才看清 ✓：**唯一的实质差异是 `smudge = 1.0`** ✓
    // ⇒ 它靠"**抹开画布上已有的颜色**"工作 ✓ ⇒ **空白画布上它本来就该什么都不出** ✗ ✓ ——
    // **那是正确行为** ✓，而我的实现给它一块**全新的空 surface** ✗ ⇒ 它**永远**没东西可抹 ✓。
    // **做法与介质那条路完全一致** ✓：`render_region_raw` 取底图 ✓（那边叫 `base` ✓），
    // 再写进 surface ✓ ⇒ **两条引擎共用同一种"有底图"的语义** ✓。
    // **`render_region_raw` 回的是 `(宽, 高, rgba)`** ✗（不是裸像素 ✓）⇒ 解构 ✓，
    // 并且**尺寸对不上就跳过** ✓ —— 宁可"这次没有底图" ✓，也不要**错位**地抹 ✓
    //（错位的涂抹会**悄悄改坏画面** ✓，比不生效难查得多 ✓）。
    // **喂进去的底图要留着** ✓ —— 读回时用它把"笔刷没碰过的像素"剔掉 ✓
    //（否则会把**笔触的包围矩形**原样复制成一个新对象 ✗，见下面读回那一段的说明 ✓）。
    // **只有会读画布的笔刷才喂底图** ✓ —— 其余笔刷喂了反而要额外剔掉"没碰过的底图" ✓
    //（见 `brush_reads_the_canvas` ✓，以及 `paint_brush` 里读回时对"笔刷没碰过的像素"的剔除 ✓）。
    // **★ P0 #4：`brush_params` **最后**应用 ⇒ **它优先** ✗ ★**（**第 254 轮自查修 ✓）
    //   ＋ **∴ 为什么必须在最后 ✗**：**判据实测抓到的 ✓**——**我第一版放在 `load_brush` 之后 ✓
    //     而 `size`／`color`／`opacity`／`hardness` 这几个**专用参数**是**之后**才写进笔刷的**✗
    //     ⇒ **∴ 于是** `brush_params{opaque:0.15}` 被调用方传的 `opacity:1.0` **覆盖掉** ✓
    //       ⇒ **∴ 判据里"传了却没变化" ✗ 看着像功能没做** ✓ ★★
    //   ＋ **∴ 语义 ✗**：**`brush_params` 是**逃生舱**（覆盖 `.myb` 任意字段 ✓）
    //     ⇒ **∴ 它**优先于那几个顺手参数**✓（**要通用覆盖就得说了算 ✓）★★
    //   ＋ **∴ 顺序也影响**读画布判定**（`brush_reads_the_canvas` 读的就是这些字段 ✓）
    //     ⇒ **∴ 放这里 ⇒ 判定看到的也是覆盖后的值 ✓ ★★★
    if let Some(params) = brush_params {
        let applied = apply_brush_params(&mut brush, params)?;
        let _ = applied;
    }
    let reads_canvas = feed_base && brush_reads_the_canvas(&brush);
    // **涂抹预算** ✓（P-4）：预测超 30 秒的涂抹笔直接报 `invalid_argument`，不静默跑几分钟。
    //   只对「涂抹真的会跑」的笔生效（`smudge > 0` 且拖曳长度 > 0；`smudge_length = 0` 的笔
    //   如 oil-01-paint 的涂抹是空操作，走 `smudge_dab` 早返，成本不在此列）。
    check_smudge_budget(&brush, points, size)?;
    // **光栅阶段之一：喂底图** ✓（P2）—— `render_region_raw` 是真正的渲染，
    // 涂抹类笔刷每次落笔都要付这份钱 ✓；测试方原先只能从外部猜它占多少 ✓。
    let base_started = std::time::Instant::now();
    if reads_canvas {
        if let Ok((base_width, base_height, base)) = ctx.workspace.render_region_raw(
            &ctx.doc_id,
            yanshi_core::Bbox::new(
                seed_region.x0 as f64,
                seed_region.y0 as f64,
                seed_region.width as f64,
                seed_region.height as f64,
            ),
        ) {
            let base_ok = base_width as usize == seed_region.width
                && base_height as usize == seed_region.height
                && base.len() >= seed_region.width * seed_region.height * 4;
            if !base_ok {
                // 说清为什么没喂 ✓（静默跳过会让"涂抹没用"这个问题**又变回谜** ✗）。
                eprintln!(
                    "  底图尺寸不符（要 {}×{}，拿到 {}×{}）⇒ 这次不喂底图（涂抹类笔刷将没有东西可抹）",
                    seed_region.width, seed_region.height, base_width, base_height
                );
            }
            if base_ok {
                // **调共享实现**（第 243 轮）：这段循环原先在这里与内核各有一份 ✗
                // ⇒ 而"两份实现必然漂移"是本项目记录过的头号病 ✓。
                // 内核那一份已在第 245 轮改成调 yanshi_render::brush::feed_base ✓。
                let fed = yanshi_render::brush::feed_base(
                    &mut surface,
                    seed_region.x0,
                    seed_region.y0,
                    seed_region.width as i32,
                    seed_region.height as i32,
                    &base,
                );
                if !fed {
                    eprintln!("  底图尺寸不符 ⇒ 这次不喂底图（涂抹类笔刷将没有东西可抹）");
                }
            }
        }
    }
    ctx.time(Phase::Raster, base_started);
    // **落笔前的快照** ✓（只在喂了底图时需要 ✓）：用来判断"引擎有没有把这一像素**挪动**过" ✓。
    // **为什么不能只看"和底图逐字节相等"** ✗：引擎盖 dab 时会把同一 tile 里**没碰过的像素**
    // 重新量化 ±1..3 个台阶 ✓（喂进去的纯白 255 读回来是 254 ✓）⇒ 必须留一个小容差 ✓；
    // 而**很淡的笔刷**（`2B_pencil` 在白底上）本来就只差几个台阶 ✓
    // ⇒ 单靠容差会把**真的落下的墨**剔掉 ✗（实测：5467 个改动像素只剩 47 ✓）
    // ⇒ 所以**落墨掩膜与"挪动过"是"或"的关系** ✓：这支笔放了颜料的像素**一律保留** ✓。
    let before_pixels = if reads_canvas {
        Some(read_surface_region(&surface, seed_region).0)
    } else {
        None
    };
    // **光栅阶段之二：dab 生成** ✓（P2）—— `stamp_stroke` 是 Hokusai 内部的落墨循环 ✓，
    // 它不可中断 ✗（见 `crate::inflight` ✓）⇒ 取消的安全点只能落在**段之间** ✓。
    let stamp_started = std::time::Instant::now();
    let steps = match color_to {
        // **一笔多色（Loaded Brush）** ✓：把路径按弧长切段 ✓、**每段换一次笔刷颜色** ✓，
        // 但 `surface` 与 `BrushState` **一路共用** ✓ ⇒ 落下来的还是**一条笔迹** ✓
        //（dab 间距连续 ✓、不会在换色点重复播种 ✓ —— 那正是 `stamp_stroke_from` 存在的理由 ✓）。
        //
        // **为什么要求两个颜色都给** ✓：只给末端色的话，"起点色"只能猜 `.myb` 自带色 ✓
        // ⇒ 猜错就是"界面里看到的和画出来的不一样" ✗ ⇒ 明确要求 ✓（错误里说清 ✓）。
        Some(color_to) => {
            let Some(from) = color else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(
                        "要给 color_to（一笔多色）就必须同时给 color ✓：起点色不猜 —— \
                         不给的话只能拿 .myb 自带色当起点 ✗，画面会与调用方以为的不一样 ✓",
                    ),
                ));
            };
            let from_bytes = brush_color_to_srgb_bytes_with_space(from, color_space)?;
            let to_bytes = brush_color_to_srgb_bytes_with_space(color_to, color_space)?;
            // 每段约 4px ✓、最多 128 段 ✓（再多也看不出来 ✓，却白花时间 ✗）。
            let dense = densify_for_ramp(points, 4.0, 128);
            let seats = dense.len().saturating_sub(1);
            let mut previous: Option<(f64, f64, f64)> = None;
            let mut steps = 0usize;
            let mut cursor = 0usize;
            while cursor < seats {
                // **取消安全点** ✓（P1）：一笔多色会切成最多 128 段 ✓ ⇒ 段与段之间可停 ✓。
                ctx.check_cancelled()?;
                let t = if seats == 0 {
                    0.0
                } else {
                    cursor as f64 / seats as f64
                };
                // **P2 #16**：`Rgb` => 与仓库既有的 `lerp_srgb` 同口径（逐字节相同）；
                // `Pigment` => 光谱 K/S 混色（黄+蓝=绿，而不是灰）。
                let bytes =
                    yanshi_render::color::mix_colors(color_mixing, from_bytes, to_bytes, t as f32);
                let (hue, saturation, value) =
                    yanshi_render::color::rgb_to_hsv(bytes[0], bytes[1], bytes[2]);
                brush.set(
                    hokusai::BrushSetting::ColorH,
                    hokusai::SettingValue::constant(hue),
                );
                brush.set(
                    hokusai::BrushSetting::ColorS,
                    hokusai::SettingValue::constant(saturation),
                );
                brush.set(
                    hokusai::BrushSetting::ColorV,
                    hokusai::SettingValue::constant(value),
                );
                steps += stamp_stroke_from(
                    &brush,
                    &mut state,
                    &mut surface,
                    &dense[cursor..=cursor + 1],
                    &mut previous,
                    dtimes,
                    sensors,
                    false,
                );
                cursor += 1;
            }
            steps
        }
        None => {
            if std::env::var_os("YANSHI_OPEN_TIMING").is_some() {
                let radius = brush.get(hokusai::BrushSetting::Radius).base_value;
                let opaque = brush.get(hokusai::BrushSetting::Opaque).base_value;
                let hard = brush.get(hokusai::BrushSetting::Hardness).base_value;
                let dabs_basic = brush
                    .get(hokusai::BrushSetting::DabsPerBasicRadius)
                    .base_value;
                let dabs_actual = brush
                    .get(hokusai::BrushSetting::DabsPerActualRadius)
                    .base_value;
                eprintln!(
                    "stamp_input side=server radius={radius:?} opaque={opaque:?} hardness={hard:?} dabs_basic={dabs_basic:?} dabs_actual={dabs_actual:?} points={points:?}"
                );
            }
            {
                // **★ 第二条落笔路也要吃到真实时间（P0 #3）★**（第 254 轮实测修）
                //   之前这里调的是 `stamp_stroke(...)`，那个包装内部固定传 `&[]`
                //   => 走这条分支时 `time_ms` 白传了（判据实测：几何相同、只改间隔，画面
                //   逐字节相同）。改成显式走 `stamp_stroke_from(..., dtimes)`，用局部
                //   `previous` 保持「每次调用从头开始」的语义（与 `stamp_stroke` 一致）。
                let mut previous_here = None;
                let steps = stamp_stroke_from(
                    &brush,
                    &mut state,
                    &mut surface,
                    points,
                    &mut previous_here,
                    dtimes,
                    sensors,
                    false,
                );
                if std::env::var_os("YANSHI_OPEN_TIMING").is_some() {
                    eprintln!("stamp_steps side=server steps={steps}");
                }
                steps
            }
        }
    };
    ctx.time(Phase::Raster, stamp_started);

    // **文档背景** ✓（第 809 轮）：`max_channel_delta` 在没有基准时要合成到**它**上 ✓
    // **背景只读三个字节，不许为它克隆整份文档状态** ✓（P-2）：`document_state` 克隆
    // `objects` 全表（水彩 11,024 个对象）⇒ 每重放一笔付约 75 毫秒 ✓（实测：水彩
    // 4,667 次重放里读回段占 350 秒，stamp 只占 0.5 秒 ✓）⇒ 这里直接借 `document()`
    // 读背景，不克隆 ✓；文档不在 ⇒ 白底兜底（与读不到通道时同例；落笔前已容忍文档不在 ✓）。
    let background_rgb: Vec<u8> = {
        let background = ctx
            .workspace
            .document(&ctx.doc_id)
            .map(|document| document.state().background.clone())
            .unwrap_or(Value::Null);
        (0..3)
            .map(|channel| {
                background
                    .get(["r", "g", "b"][channel])
                    .and_then(Value::as_u64)
                    .map(|value| value.min(255) as u8)
                    .unwrap_or(255)
            })
            .collect()
    };
    // **Hokusai 的 tile 是 fix15（u16, 0..=32768）** ✓ ⇒ 转成我们用的 RGBA8 ✓（满值 32768 须先 clamp 再 `>> 7`，否则回绕成 0 ✗）。
    // **同一个区域公式** ✓（落笔前喂底图用的就是它 ✓）。
    // **同一份（已裁的）区域** ✓ —— 读回与掩膜都必须与喂底图用的那个完全一致 ✗。
    let region = seed_region;
    // **光栅阶段之三：读回与掩膜** ✓（P2）。
    let read_started = std::time::Instant::now();
    let (mut rgba, mut painted) = read_surface_region(&surface, region);
    // ★ **`max_channel_delta` ✗**（第 20 轮 ✓；**∴ 用户报的薄涂笔零落笔 ✓**）
    //
    // **∴ 为什么需要它 ✗**：`painted_pixels` 只数"字节变了"✗ ⇒ **∴ 差 1 LSB 也算 ✓**
    //   ⇒ **∴ 实测 `thin_watercolor` 在 size=20 下 `painted_pixels=146` ✗ 而**最大单通道只差 6**✗、
    //      中位差 **1**✗ ⇒ **∴ 肉眼全白 ✗，而 `ok=true` ＋ `painted_pixels>0` ⇒ **∴ 静默失败 ✓**
    //   ⇒ **∴ 而现在下一行那段 `CANVAS_DRIFT_TOLERANCE = 3` ✗ 的闸门**正好把差 1 的真落墨清零**✗
    //      ⇒ **∴ 所以这一项不仅是诊断 ✓，它也是**识别"薄涂笔"**✗ 的依据 ✓**。
    //
    // **∴ 算法 ✓**：**不碰 `rgba` ✗**（**∴ 纯读 ✓**）⇒ **∴ 量的是"导出图上
    //   实际变了多少"** ✓ —— 与 `draw_evidence_compare` 同一口径 ✓：
    //   tile 按 Normal source-over 合成到落笔前像素上再比 RGB，alpha 不参与 ✓。
    //   **∴ 旧口径的错**（名画第 3 轮 §7 glazing 项）：喂底图分支曾逐通道
    //   直比（含 alpha、未合成）⇒ 涂抹笔晕边低 alpha 像素报 |a−255| ≈ 244–254，
    //   而导出 RGB 合成后仅差 4 级 ⇒ 回执 254 冒充了导出效果 ✗。
    //   **∴ 残留**：本遍在落墨掩膜清零之前量 ⇒ 被掩膜清零的像素
    //   （四通道与底图差均 ≤ `CANVAS_DRIFT_TOLERANCE` = 3）最多贡献 3 级，
    //   属漂移容差量级，不是另一套口径 ✓。
    let (max_delta, delta_distribution) = {
        let mut max_delta: u8 = 0;
        // **差值直方图** ✓（P-15）：同一遍遍历里记，与 `max_delta` 同一个像素、同一个差值 ✓。
        let mut hist = [0u32; 256];
        let before = before_pixels.as_deref();
        for index in (0..rgba.len()).step_by(4) {
            // ★ **无基准时必须跳过**未触碰的透明像素 ✗** ★**（第 118 轮 ✓；**∴ 外部复测 QY-D03 ✓**）
            //
            // **∴ 报告实测 ✓**：**∴ `fed_base=false` ✗ 的白色笔 ✗ ⇒ **∴ 本字段返回 **255**✗
            //   而**合成后的真实最大 RGB 差是 **0**✗** ⇒ **∴ 灰笔真实差 31 ✗ 也报 255 ✗**
            // **∴ 根因 ✓**：**∴ 引擎缓冲里的空白像素是 **RGBA(0,0,0,0)**✗
            //   ⇒ **∴ 而我逐通道按**白底 255**✗ 比较** ⇒ **∴ 差恒 255 ✗ ⇒ **∴ 饱和 ✓**
            // **∴ 所以 ✓**：**∴ 无基准时 ✗ 只让**有 alpha**✗ 的像素参与 ✓
            //   ⇒ **∴ 于是**白笔（255 vs 255）⇒ **0** ✓／近白 ⇒ **1** ✓／灰 ⇒ **31** ✓／深 ⇒ **205** ✓** ✓
            // **∴ 而有基准时 ✗ 也要先合成再比 ✗**（名画第 3 轮 §7 glazing 项）：
            //   直通 tile 的 RGB 不是画面颜色 ⇒ 低 alpha 晕边像素的 RGB 直比
            //   与 alpha 直比都会虚报几百级 ⇒ 必须按 Normal source-over 合成
            //   到落笔前像素上再比 RGB（与 `draw_evidence_compare` 同式 ✓），
            //   alpha 通道不参与（它不是可见颜色 ✓，与无基准分支同例 ✓）。
            if before.is_none() && rgba[index + 3] == 0 {
                continue;
            }
            // ★ **无基准时必须**先合成到白底**✗ 再比 ✗** ★**（**∴ 外部复测 QY-D03 的第二层 ✓**）
            //
            // **∴ 为什么还要合成 ✗**：**∴ 只跳过 `alpha == 0` ✗ 不够 ✓**
            //   ⇒ **∴ 实测 ✓**：**∴ 白笔仍报 **254**✗** ⇒ **∴ 因为**抗锯齿**边缘半透明**✗
            //     （**∴ RGB 接近 0 ✗ 而 alpha 很低 ✓**）⇒ **∴ 与 255 差 **254**✗ 但它**肉眼不可见**✗** ✓
            // **∴ 所以 ✓**：**∴ 按 alpha 合成到白底 ✗ ⇒ **∴ 得**可见 RGB**✗ ⇒ **∴ 再比 ✓**
            //   ⇒ **∴ 公式 ✓**：`c = rgb·a + 255·(1−a)` ✗（**∴ a 归一化 ✓**）
            //   ⇒ **∴ 于是**白笔（a=1）⇒ **c=255 ⇒ delta=0** ✓；半透明边缘 ⇒ **c 靠近 255 ⇒ 差小** ✓
            let mut delta: u8 = 0;
            for channel in 0..4 {
                // **∴ 没有基准时按**白底**✗**（**∴ 不读画布的笔直接写色 ✓ ⇒ **∴ 与白底比才是真实幅度 ✓**）
                //   ⇒ **∴ 否则**这类笔会报 0 ✗**（**∴ `pointy_ink` 实测就是 0 ✗ ⇒ **∴ 那会误导 ✓**）
                // **没有基准时比**文档自己的背景**，不是恒白 255** ✓
                //   （第 809 轮；外部 `qoder` 的 D194／D193 建议 (i)）
                //   **⇒ 缺陷**：原来的 255 是"白底文档"的假设 ✓ ⇒ 在**灰底(128)** 文档上
                //     一笔**纯白**笔（真实改变 = |255−128| = 127 ✓）会被算成 **0** ✗；
                //     反过来一笔"什么都没改"的笔会被算成 **127** ✗ —— **符号都反了** ✓。
                //   **⇒ 修法**：无基准时拿 `background` 的对应通道作 old ✓（读不到才退回 255 ✓）。
                // `background_rgb` 已经归一成 3 个 `u8` ✓ ⇒ 直接取 ✓（读不到才退回 255 ✓）
                let background_byte =
                    |channel: usize| -> u8 { *background_rgb.get(channel).unwrap_or(&255) };
                let old_byte = before
                    .and_then(|pixels| pixels.get(index + channel))
                    .copied()
                    .unwrap_or_else(|| background_byte(channel));
                // **∴ 有基准 ⇒ 先按 Normal source-over 合成到落笔前像素上再比 RGB ✓；
                //   无基准 ⇒ 先把该通道合成到背景上再比 ✓**（**∴ 两者才都等于"肉眼看到"✓**）
                let current = if before.is_some() {
                    if channel == 3 {
                        // **∴ alpha 通道不参与 ✗**（**∴ 它不是可见颜色 ✓**）
                        continue;
                    }
                    // `old_byte` 在有基准时就是落笔前该通道的值 ✓ ⇒ 与
                    // `draw_evidence_compare` 同式：`tile·a + before·(255−a)` ✓。
                    let alpha = i32::from(rgba[index + 3]);
                    (i32::from(rgba[index + channel]) * alpha + i32::from(old_byte) * (255 - alpha))
                        / 255
                } else if channel == 3 {
                    // **∴ alpha 通道不参与 ✗**（**∴ 它不是可见颜色 ✓**）
                    continue;
                } else {
                    // **合成到**文档背景**上 ✓**（不是白底 ✓）
                    let alpha = i32::from(rgba[index + 3]);
                    let base = i32::from(background_byte(channel));
                    (i32::from(rgba[index + channel]) * alpha + base * (255 - alpha)) / 255
                };
                let diff = current - i32::from(old_byte);
                let diff = u8::try_from(diff.unsigned_abs().min(255)).unwrap_or(255);
                delta = delta.max(diff);
            }
            max_delta = max_delta.max(delta);
            if delta > 0 {
                hist[delta as usize] += 1;
            }
        }
        (max_delta, delta_distribution(&hist))
    };
    // **只保留"这一笔真的碰到过"的像素** ✗ —— 绝不能把喂进去的底图**原样复制**出来 ✓：
    // 那会把**笔触的包围矩形**烘成一个新对象 ✗ ⇒ 删掉底下的东西之后，
    // 画面里会留下一块**直角矩形幽灵** ✓ —— 这正是用户报的「`Flat2#1` 画叶子出矩形 artifact」✓
    //（实测复现 ✓：半透明绿矩形 + 一条 2B_pencil ✓ ⇒ 删掉绿矩形 ✓ ⇒ 矩形区域里**还剩 1076 个绿像素** ✗）。
    if reads_canvas {
        // **把 size 先落成变量** ✓（第 646 轮）：直接写 `size.unwrap_or(20.0)` 会让
        // `scripts/check-deposit-mask-time-base.mjs` 的 ③ 号检查**匹配不到 `dtimes`**
        // （它的正则 `brush_deposit_mask\([^)]*\)` 在嵌套括号处就断了 ✗）⇒ CI 红。
        let paint_size = size.unwrap_or(20.0);
        let deposit = brush_deposit_mask(&brush, paint_size, points, region, dtimes);
        let before = before_pixels.unwrap_or_default();
        // 引擎在"没碰过的像素"上的量化漂移上限 ✓（见快照处的说明 ✓）。
        const CANVAS_DRIFT_TOLERANCE: i32 = 3;
        for index in (0..rgba.len()).step_by(4) {
            // **留着它** ⟺ 这支笔在这里**落过颜料** ✓，或者这一像素被**明显挪动/覆盖**过 ✓。
            let deposited = deposit.get(index + 3).copied().unwrap_or(0) > 0;
            let moved = before.len() >= index + 4
                && (0..4).any(|channel| {
                    (i32::from(rgba[index + channel]) - i32::from(before[index + channel])).abs()
                        > CANVAS_DRIFT_TOLERANCE
                });
            if !(deposited || moved) {
                if rgba[index] != 0
                    || rgba[index + 1] != 0
                    || rgba[index + 2] != 0
                    || rgba[index + 3] != 0
                {
                    painted -= 1;
                }
                rgba[index] = 0;
                rgba[index + 1] = 0;
                rgba[index + 2] = 0;
                rgba[index + 3] = 0;
            }
        }
    }
    ctx.time(Phase::Raster, read_started);
    // **反馈期 1 注**：撑不满 dab 间距的短笔触已在上面兜底（延长落一枚 dab），
    // 走到这里的 0 墨是**其它**真 0 墨情形（透明／涂抹／不可见色等），下面的分因诊断仍适用。
    if painted == 0 {
        // **P-19：有效 opacity 是 0 时，零墨是调用方要的透明，不是不够长** ✓ ——
        // `color.a=0` / `opacity: 0` 都会把 `Opaque` 压到 0 ⇒ 一枚 dab 都不会有墨 ✓ ⇒
        // 这时若还报"加长路径"，就是误导（照着改也改不好 ✓）。先说清真正原因 ✓。
        let opaque = brush.get(hokusai::BrushSetting::Opaque).base_value;
        if opaque <= 0.0 {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(
                    "这一笔没落下任何像素 ⇒ 有效 opacity 是 0（`color.a=0` 或 `opacity: 0`）✓：\
                     透明度为 0 的笔本来就不会落墨 ✓，不是路径太短、也不是笔刷不对 ✓。\
                     **想要墨** ⇒ 把 `color.a` / `opacity` 设大；**想要的就是透明** ⇒ 这笔不用画 ✓。"
                        .to_owned(),
                ),
            ));
        }
        // **说清原因，并给出路** ✓（"错误里要能照着改" 是本项目的既定规矩 ✓）。
        //
        // **实测过的事** ✓：`ramon-Knife.myb` **无论给不给 `size` 都不出墨** ✗ ——
        // 它的 `dabs_per_basic_radius` 是 **0** ✓ ⇒ 按基本半径算间距时**间距趋于无穷** ✓
        // ⇒ **一枚印章都不落** ✗（`dabs_per_actual_radius` 那一套没被用上 ✓）。
        // ⇒ 这类笔刷**在当前引擎下画不出来** ✓ ⇒ 与其只报"没有墨" ✗，不如**指出来** ✓
        //（否则调用方会以为是自己参数给错 ✓ —— 我第一版的消息正是这样误导的 ✗）。
        // **先看它是不是"涂抹类"** ✓ —— 那类笔刷靠**抹开画布上已有的颜色**工作 ✓
        // ⇒ 空白区域上它**本来就该什么都不出** ✓（`ramon-Knife` 正是这一类 ✓：
        // 与能画的 `classic-knife` 逐字段对比 ✓ ⇒ **唯一实质差异是 `smudge = 1.0`** ✓）。
        // **我此前那条"换一支笔刷"是误导** ✗ ⇒ 现在按真实原因分三种说 ✓。
        let smudge = brush.get(hokusai::BrushSetting::Smudge).base_value;
        if smudge > 0.0 {
            return Err(YanshiError::new(
                ErrorCode::PreconditionFailed,
                ErrorContext::detail(format!(
                    "这一笔没落下任何像素 ⇒ 笔刷「{name}」是**涂抹类**（smudge={smudge}）✓：\
                     它靠**抹开画布上已有的颜色**工作 ⇒ 这块区域上大概**还没有颜色**可抹 ✓。\
                     **怎么改（三条，按省事排序）**：\
                     ① **先铺底色再抹** —— 用一支不透明的笔刷（如 `100%_Opaque`）在这块地方画一笔，\
                        然后再用这支涂抹笔刷抹 ✓（湿画法里「先铺后揉」就是这个顺序 ✓）；\
                     ② **用 `swatch_brush_strokes` 试它之前，也先铺一层底色** ✓（swatch 不写文档 ⇒ 铺底色那笔可以随便画 ✓）；\
                     ③ 换一块**已经有颜色**的区域试，或换一支 **非涂抹**（`smudge = 0`）的笔刷 ✓。"
                )),
            ));
        }
        let dabs_basic = brush
            .get(hokusai::BrushSetting::DabsPerBasicRadius)
            .base_value;
        // **兜底消息要落在真实根因上**（外部独立测试 DEF-04）：调用方**往往已经给了 size**，
        // "试试给 size" 会把人引向已经试过的路。真实机制是：dab 间距随 size 增大
        // （间距 ≈ 半径 / dabs_per_basic_radius），路径总长撑不满一枚 dab 间距时就是 0 dab。
        // ★ **`dabs_basic <= 0` 不等于「整支笔刷不可用」✗** ★**（外部复测 QY-D02 ✓）
        //
        // **∴ 他们实测 ✓**（P2-13 之前；修后小 size 也能落墨）：**∴ 同一支 `calligraphy` ✗ ⇒ **∴ `size=3.456` ＋ 33 点 ⇒ **3/3 无像素**✗
        //   而 `size=8／16` **都成功**✗** ⇒ **∴ 所以**我原来那句「它在当前引擎下画不出来，**换一支**」✗
        //   ⇒ **∴ 是**把"本笔条件不满足"✗ 说成了**"整支笔刷不可用"**✗** ⇒ **∴ 会误导创作者换笔 ✓**
        // ⇒ **∴ 所以 ✓**：**∴ 只描述**本次笔触未产生像素**✗ ＋ **摆出本笔参数**✗ ＋
        //   **明确说**"同刷换参数通常能画"✗ ⇒ **∴ 而**换笔**只作为**条件性出路**✗** ✓
        // **建议要按**本笔的比值**排序** ✓（第 1552 轮；外部 `B3`）
        //   **⇒ 他们的实测**：`size=120`、两点、路径总长 **15.2px** 的笔被拒画，
        //     而报错首推「加大 size 到 4 以上」⇒ **对 `size=120` 是无效建议** ✗
        //     （**真实原因是路径太短** ⇒ `length / size = 0.13`）
        //   **⇒ ⇒ 所以**：**比值 < 1 ⇒ 首推"加长路径"**（P2-13 之后不分 size 档：
        //     抗锯齿那档的原因已修掉——`hardness_new <= 0` 时跳过 AA 调整，
        //     小 size 也能落墨 ⇒ 剩下的 0 墨一律是路径撑不满 dab 间距）。
        let hint: String = if dabs_basic <= 0.0 {
            let n = points.len();
            let path_len: f64 = points
                .windows(2)
                .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
                .sum();
            // **按比值给两条不同的排序** ✓（P2-13 之后不分 size 档：路径太短是
            //   唯一剩下的 0 墨原因 ⇒ `path_len < size` 一律首推"加长路径" ✓）。
            let advice: String = match size {
                Some(sz) if path_len < sz => format!(
                    "① **加长路径**（本笔 {n} 点、总长 {path_len:.1}px，而 size={sz}px ⇒ 比值 {:.2} ⇒ \
                     **路径撑不满一个 dab 间距**）✓；② 若确实想用大笔刷，再**加大 size**（但对本笔无用）；\
                     ③ 换一支能画的笔刷",
                    path_len / sz
                ),
                _ => format!(
                    "① **加长路径**（本笔 {n} 点、总长 {path_len:.1}px）✓；② 加大 size；③ 换一支能画的笔刷"
                ),
            };
            // **结论句要跟着**真实原因**走** ✓（外部 `B3` ✗）：`size=120` ＋ 15px 路径时，
            //   首推"加大 size"是**无效建议** ✓ ⇒ 而**表头**原先仍写"几乎都是 size 太小" ✗
            //   ⇒ 现在表头与建议**同一个判据**（`path_len < size` ⇒ 路径太短，不分 size 档）✓。
            //   另：外壳已经写了「这一笔没落下任何像素 ⇒ 笔刷「X」」✗ ⇒ 这里**不再重复**那句话 ✓
            //   （实测原来会打印成「…没落下任何像素 ⇒ 笔刷「calligraphy.myb」这一笔没落下任何像素…」✗）。
            let cause: String = match size {
                Some(sz) if path_len < sz => format!(
                    "**路径太短**（撑不满一个 dab 间距：总长 {path_len:.1}px，size={sz}px ⇒ 比值 {:.2}）",
                    path_len / sz
                ),
                _ => "**这一笔没落下任何像素**".to_string(),
            };
            format!(
                "（`painted_pixels=0`）⇒ {cause} ⇒ \
                 `size={size:?}` ⇒ 半径约 size/2 px ⇒ 0 墨的原因是**路径撑不满 dab 间距** \
                 （间距 ≈ 半径 / dabs_per_basic_radius，本笔 dabs_per_basic_radius=0 时间距钳在下限 1.0px）。\
                 **不是抗锯齿**：P2-13 已修掉 AA 那档——`stroke.rs` 的抗锯齿分支在 `hardness` 将变负时\
                 **跳过调整**，保留原半径/硬度，小 size 也能落墨 \
                 （`calligraphy` size=1/2/3 在短/长路径上都落墨，见 `tool-calligraphy-small-size.mjs`）。\
                 **这不代表整支笔刷不可用** ✗：同一支笔在**更长路径**上通常能落墨 \
                 （本次 {n} 个点、路径总长 {path_len:.1}px）。\
                 **怎么改（按**本笔的真实比值**排序，不是一律先加大 size）**：{advice}\
                 注：`dabs_per_basic_radius=0` **不是**原因 —— `stroke.rs` 把间距钳在下限 1.0，不会趋于无穷。"
            )
        } else {
            let n = points.len();
            let path_len: f64 = points
                .windows(2)
                .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
                .sum();
            match size {
                Some(sz) => format!(
                    "size 已经是 {sz}px ⇒ 问题不在缺参数，而在**路径撑不满 dab 间距**：\
                     本笔 {n} 个点、路径总长 {:.0}px；dab 间距随 size 增大 \
                     （≈ 笔刷半径 / dabs_per_basic_radius）⇒ 两条出路：\
                     ① 加长路径 / 加密点数（让总长达到间距的数倍）；\
                     ② 减小 size 或换 dab 更密的笔刷 ✓",
                    path_len.max(0.0)
                ),
                None => "试试给 size，或换一支笔刷 ✓".to_owned(),
            }
        };
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("这一笔没落下任何像素 ⇒ 笔刷「{name}」{hint}")),
        ));
    }
    Ok(BrushPaint {
        name,
        x0: region.x0,
        y0: region.y0,
        width: region.width,
        height: region.height,
        rgba,
        steps,
        painted,
        max_delta,
        delta_distribution,
        fed_base: reads_canvas,
        short_stroke_hint,
    })
}

/// **笔触风格** ✓（AI 画家需求 P2-8）：**只改压感/位置，不碰渲染** ✓。
/// **缺省（没给 `style`）⇒ 原样返回** ⇒ 老行为**逐字节不变** ✓（本仓库的老规矩 ✓）。
/// `sketchy` 的抖动由**确定性 PRNG** 生成 ✓（同输入 ⇒ 同结果 ✓，否则与本仓库底线冲突 ✗）。
pub(super) fn apply_brush_style(
    mut points: Vec<(f64, f64, f64)>,
    args: &Value,
) -> Result<Vec<(f64, f64, f64)>> {
    let Some(style) = optional_str(args, "style") else {
        return Ok(points);
    };
    match style.as_str() {
        "confident" => {
            // **起笔重、收笔轻** ✓：沿笔画把压力从 1.15 线性收到 0.55 ✓（**位置不动** ✓）。
            let last = points.len().max(2) - 1;
            for (index, point) in points.iter_mut().enumerate() {
                let t = index as f64 / last as f64;
                point.2 = (point.2 * (1.15 - 0.60 * t)).clamp(0.0, 1.0);
            }
            Ok(points)
        }
        "sketchy" => {
            // **抖动与断笔** ✓：幅度取"笔尖的百分之几"这个量级 ✓；种子由点数派生 ⇒ 可复现 ✓。
            let mut state = 0x5DEE_CE66_D000_0000_u64 ^ (points.len() as u64);
            for point in points.iter_mut() {
                let jitter_x = (scatter_next(&mut state) - 0.5) * 2.5;
                let jitter_y = (scatter_next(&mut state) - 0.5) * 2.5;
                point.0 += jitter_x;
                point.1 += jitter_y;
                // **断笔**：压力乘一个 0.55~1.05 的系数 ✓（轻的地方更像"擦过"✓）。
                point.2 = (point.2 * (0.55 + 0.50 * scatter_next(&mut state))).clamp(0.0, 1.0);
            }
            Ok(points)
        }
        other => Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "未知笔触风格 {other} ⇒ 可用：confident / sketchy（不给 = 原样 ✓）"
            )),
        )),
    }
}
