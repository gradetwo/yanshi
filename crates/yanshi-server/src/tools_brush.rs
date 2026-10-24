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

pub(super) fn write_brush_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    // **裁剪一共吃掉多少像素**（跨选区累计，供 `painted_pixels_clipped` 用）✓
    let mut clipped_away = 0usize;
    // **选区内的覆盖率**（`None` = 这一笔没裁剪）（第 1567 轮）
    let mut clip_coverage: Option<f64> = None;
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

    let layer_id = require_str(args, "layer_id")?;
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
    let sensors_source = args.get("sensors").cloned().unwrap_or(Value::Null);
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
    let symmetry_name = optional_str(args, "symmetry").unwrap_or_else(|| "none".to_owned());
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
    let size_source = args.get("size").cloned().unwrap_or(Value::Null);
    let color_source = args.get("color").cloned().unwrap_or(Value::Null);
    let color_to_source = args.get("color_to").cloned().unwrap_or(Value::Null);
    let smooth_source = json!(optional_bool(args, "smooth").unwrap_or(false));
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
    let opacity_source = args.get("opacity").cloned().unwrap_or(Value::Null);
    let hardness_source = args.get("hardness").cloned().unwrap_or(Value::Null);
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
    // **`color.a` 非 255 ⇒ 响亮拒绝** ✓（第 786 轮；我自己扫描时发现的）
    //   **⇒ 缺陷**：`brush_color_to_srgb_bytes` 只返回 `[u8; 3]` ✓ ⇒ **alpha 被整个丢掉** ✗
    //     ⇒ 调用方传 `color.a = 0`（**完全透明** ✓）却得到**不透明**的一笔 ✓ —— 这不是"忽略" ✓
    //     而是**与意图相反** ✓（实测：`a=0`／`a=1`／`a=255` 的 `ink_pixels` 一模一样 ✓）。
    //   **⇒ 为什么选择拒绝而不是实现**：笔刷的不透明度归 `.myb` 自己的 `opacity` ＋ `glaze` ✓
    //     ⇒ 把 `color.a` 乘进去会**悄悄改变每一笔的外观** ✓（既有画作会变 ✗）；
    //     而"静默地做反了"比"说出来做不到"更坏 ✗ ⇒ 先说清楚 ✓，见 `opacity` ✓。
    if let Some(alpha) = args
        .get("color")
        .and_then(|color| color.get("a"))
        .and_then(Value::as_f64)
    {
        // **两种写法都接受**：0..1 的小数 ✓ 或 0..255 的字节 ✓（`parse_spec_color` 的口径 ✓）。
        let byte = if alpha <= 1.0 { alpha * 255.0 } else { alpha };
        if byte.round() < 255.0 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "`color.a` = {alpha} 不被支持（**笔刷笔触的 alpha 归笔刷自己的 `opacity`** ✓）\
                     ⇒ 请传 `a: 255`（或 1.0）并用 `opacity`／`glaze` 控制透明度 ✓。\
                     **为什么拒绝**：这条路径**只取 RGB**（`brush_color_to_srgb_bytes` ✓）\
                     ⇒ 传 `a: 0` 会得到**不透明**的一笔 ✓（**与意图相反** ✓），而我们不愿猜测 ✓。"
                )),
            ));
        }
    }

    // **落笔走唯一的实现** ✓（`paint_brush` ✓，与 `brush_preview` 共用 ✓）。
    let paint = paint_brush(
        ctx,
        &brush_name,
        &points,
        args.get("size").and_then(Value::as_f64),
        args.get("color"),
        args.get("color_to"),
        glaze_opacity,
        args.get("hardness").and_then(Value::as_f64),
        // **文档里必须喂底图** ✓ —— 涂抹类笔刷靠它工作 ✓。
        true,
        // **★ P0 #4：把调用方给的字段覆盖传进去 ✗ ★**（**∴ 我第一版补成了 `None` ✗ ⇒ ①② 假绿 ✓）★
        merged_brush_params.as_ref(),
        color_mixing,
        &dtimes,
        None,
        sensors,
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
        fed_base,
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
                rgba[index] = 0;
                rgba[index + 1] = 0;
                rgba[index + 2] = 0;
                rgba[index + 3] = 0;
                dropped += 1;
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
    if symmetry != yanshi_render::Symmetry::None {
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
            let (mirror_x0, mirror_y0) = match symmetry {
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
                let vertical = symmetry == yanshi_render::Symmetry::Vertical;
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
            "points": points_source,
            "size": size_source,
            "color": color_source,
            "color_to": color_to_source,
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
        "object_id": optional_str(args, "object_id"),
        "bitmap": {
            "blob_hash": composite.to_string(),
            "size": rgba.len(),
            "mime_type": "image/x-yanshi-raw",
        },
        "region": {"x": x0, "y": y0, "w": width, "h": height},
    });
    let mut value = write_import_image(ctx, &import_args)?;
    value["brush"] = json!(name);
    value["steps"] = json!(steps);
    value["painted_pixels"] = json!(painted);
    // **裁剪后的真实落墨量** ✓（第 1553 轮；外部 `B6`）
    //   **⇒ 为什么两个都要**：`painted_pixels` 是**印章面积**（裁剪前）⇒ 用它算覆盖率
    //     会得出「笔触都落上了」的错觉（他们实测：选区 166 万像素、累计报 793 万）✗
    //   **⇒ ⇒ 所以**同时给：`painted_pixels`（raw）与 `painted_pixels_clipped`（**真实落墨**）✓
    //   **⇒ ⇒ ⇒ 没有裁剪时两者相等**（`clipped_away == 0`）⇒ **老调用方不受影响** ✓
    value["painted_pixels_clipped"] = json!(painted.saturating_sub(clipped_away));
    // **`ink_pixels`：真正落上墨的像素数** ✓（第 784 轮；外部 `qoder` 的 D139）
    //   **⇒ 与 `painted_pixels` 的区别**（这条是本轮新增的**可测量** ✓）：
    //     `painted_pixels` ＝ **印章面积**（铺开了多少 ✓）；
    //     `ink_pixels`     ＝ 合成后 **alpha 非零** 的像素数（真的画上了多少 ✓）。
    //   **⇒ 为什么需要它** ✗：他们的 D139 说「有印章但落墨 0」是**静默区** ✓
    //     ⇒ 而 `painted_pixels` **看不出**这件事 ✓（全透明一笔照样 `painted=324` ✓，实测 ✓）。
    //   **⇒ 为什么现在只给数、不给 warning** ✗：**warning 那条分支我暂时造不出触发条件** ✓
    //     （`color.a=0` 实测**不是**全透明 ✓：`painted=324 / ink=324` ✓）⇒ 按规矩
    //     **不提交没有判据的分支** ✓ ⇒ 先只加字段 ✓，warning 等能复现了再加 ✓。
    let ink_pixels = rgba.chunks_exact(4).filter(|px| px[3] != 0).count();
    value["ink_pixels"] = json!(ink_pixels);
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
        if !time_sensitive {
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
            let (hue, sat, value) = brush_color_to_hsv(value)?;
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
            let from_bytes = brush_color_to_srgb_bytes(from)?;
            let to_bytes = brush_color_to_srgb_bytes(color_to)?;
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
                );
                if std::env::var_os("YANSHI_OPEN_TIMING").is_some() {
                    eprintln!("stamp_steps side=server steps={steps}");
                }
                steps
            }
        }
    };
    ctx.time(Phase::Raster, stamp_started);

    // **Hokusai 的 tile 是 fix15（u16, 0..32767）** ✓ ⇒ 转成我们用的 RGBA8 ✓（`>> 7` 正好 0..255 ✓）。
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
    // **∴ 算法 ✓**：**不碰 `rgba` ✗**（**∴ 纯读 ✓**）⇒ **∴ 在清零点之前量，才是**原始**落墨幅度 ✓**
    //   ⇒ **∴ 于是**它报的是"引擎到底改了多少"✗，与后续掩膜无关 ✓
    let max_delta = {
        let mut max_delta: u8 = 0;
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
            // **∴ 而有基准时 ✗ **不改**✗**（**∴ base 是真实像素 ✓ ⇒ **∴ 逐通道比较本来就对 ✓**）✓
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
                let old_byte = before
                    .and_then(|pixels| pixels.get(index + channel))
                    .copied()
                    .unwrap_or(255);
                // **∴ 有基准 ⇒ 直接比 ✓；无基准 ⇒ 先把该通道合成到白底 ✓**（**∴ 后者才等于"肉眼看到"✓**）
                let current = if before.is_some() {
                    i32::from(rgba[index + channel])
                } else if channel == 3 {
                    // **∴ alpha 通道不与白底比 ✗**（**∴ 它不是可见颜色 ✓**）
                    continue;
                } else {
                    let alpha = i32::from(rgba[index + 3]);
                    (i32::from(rgba[index + channel]) * alpha + 255 * (255 - alpha)) / 255
                };
                let diff = current - i32::from(old_byte);
                let diff = u8::try_from(diff.unsigned_abs().min(255)).unwrap_or(255);
                delta = delta.max(diff);
            }
            max_delta = max_delta.max(delta);
        }
        max_delta
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
    if painted == 0 {
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
        // **∴ 他们实测 ✓**：**∴ 同一支 `calligraphy` ✗ ⇒ **∴ `size=3.456` ＋ 33 点 ⇒ **3/3 无像素**✗
        //   而 `size=8／16` **都成功**✗** ⇒ **∴ 所以**我原来那句「它在当前引擎下画不出来，**换一支**」✗
        //   ⇒ **∴ 是**把"本笔条件不满足"✗ 说成了**"整支笔刷不可用"**✗** ⇒ **∴ 会误导创作者换笔 ✓**
        // ⇒ **∴ 所以 ✓**：**∴ 只描述**本次笔触未产生像素**✗ ＋ **摆出本笔参数**✗ ＋
        //   **明确说**"同刷换参数通常能画"✗ ⇒ **∴ 而**换笔**只作为**条件性出路**✗** ✓
        // **建议要按**本笔的比值**排序** ✓（第 1552 轮；外部 `B3`）
        //   **⇒ 他们的实测**：`size=120`、两点、路径总长 **15.2px** 的笔被拒画，
        //     而报错首推「加大 size 到 4 以上」⇒ **对 `size=120` 是无效建议** ✗
        //     （**真实原因是路径太短** ⇒ `length / size = 0.13`）
        //   **⇒ ⇒ 所以**：**比值 < 1 ⇒ 首推"加长路径"**；**`size < 4` 时**才首推"加大 size"** ✓
        //     （`size < 4` 那档的原因**不同**：抗锯齿把小半径的 `hardness` 压成负数）
        let hint: String = if dabs_basic <= 0.0 {
            let n = points.len();
            let path_len: f64 = points
                .windows(2)
                .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
                .sum();
            // **按比值给两条不同的排序** ✓
            let advice: String = match size {
                Some(sz) if sz >= 4.0 && path_len < sz => format!(
                    "① **加长路径**（本笔 {n} 点、总长 {path_len:.1}px，而 size={sz}px ⇒ 比值 {:.2} ⇒ \
                     **路径撑不满一个 dab 间距**）✓；② 若确实想用大笔刷，再**加大 size**（但对本笔无用）；\
                     ③ 换一支能画的笔刷",
                    path_len / sz
                ),
                Some(sz) if sz < 4.0 => format!(
                    "① **加大 size 到 4 以上**（本笔 size={sz}px ⇒ 抗锯齿把小半径的 `hardness` \
                     压成负数）✓；② 加长路径（本笔 {n} 点、总长 {path_len:.1}px）；③ 换一支能画的笔刷"
                ),
                _ => format!(
                    "① **加长路径**（本笔 {n} 点、总长 {path_len:.1}px）✓；② 加大 size；③ 换一支能画的笔刷"
                ),
            };
            // **结论句要跟着**真实原因**走** ✓（外部 `B3` ✗）：`size=120` ＋ 15px 路径时，
            //   首推"加大 size"是**无效建议** ✓ ⇒ 而**表头**原先仍写"几乎都是 size 太小" ✗
            //   ⇒ 现在表头与建议**同一个判据**（`size >= 4 && path_len < size` ⇒ 路径太短）✓。
            //   另：外壳已经写了「这一笔没落下任何像素 ⇒ 笔刷「X」」✗ ⇒ 这里**不再重复**那句话 ✓
            //   （实测原来会打印成「…没落下任何像素 ⇒ 笔刷「calligraphy.myb」这一笔没落下任何像素…」✗）。
            let cause: String = match size {
                Some(sz) if sz >= 4.0 && path_len < sz => format!(
                    "**路径太短**（撑不满一个 dab 间距：总长 {path_len:.1}px，size={sz}px ⇒ 比值 {:.2}）",
                    path_len / sz
                ),
                Some(sz) if sz < 4.0 => format!(
                    "**size 太小**（{sz}px：抗锯齿把小半径的 `hardness` 压成负数）"
                ),
                _ => "**这一笔没落下任何像素**".to_string(),
            };
            format!(
                "（`painted_pixels=0`）⇒ {cause} ⇒ \
                 `size={size:?}` ⇒ 半径约 size/2 px ⇒ 笔刷的抗锯齿分支（`stroke.rs` 的 `current_fadeout < aa_min`）\
                 会把 `hardness` 调成**负数** ⇒ `draw_dab_default` 在 `hardness <= 0` 时整 dab 拒画。\
                 **已实测**（第 731 轮）：`calligraphy` 在 `size=1/2/3` 全不落墨、`size=4` 起正常；\
                 同一 size=3 下 `2B_pencil`／`100%_Opaque`／`Round_Bl` 都能落墨 ⇒ **不是 size 本身的问题**，\
                 是那支笔的 `hardness`／抗锯齿参数在小半径下被推到负值。\
                 **这不代表整支笔刷不可用** ✗：同一支笔在**更大的 size 或更长路径**上通常能落墨 \
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
        fed_base: reads_canvas,
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
