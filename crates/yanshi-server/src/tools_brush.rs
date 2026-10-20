//! **笔刷落笔工具**（`brush_stroke`）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **注**：`brush_preview` 不在这里 —— 它与选区 helper 交错，留待后续。
//! 本模块的顶层项/方法/字段是 `pub(super)`（`tools.rs` 的 `#[cfg(test)]` 会引用）。

use super::*;

pub(super) fn write_brush_stroke(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
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
    // **风格变换** ✓（缺省 ⇒ 原样 ✓，见 `apply_brush_style` ✓）。
    let points = apply_brush_style(points, args)?;
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
                    "未知 color_mixing {name} ⇒ 可用：rgb（缺省）/ pigment"
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
    let merged_brush_params = merge_intent_brush_params(args);
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
                let number = |key: &str| bbox.get(key).and_then(Value::as_f64).unwrap_or(0.0);
                Some((
                    selection_id.clone(),
                    yanshi_core::Bbox::new(number("x"), number("y"), number("w"), number("h")),
                ))
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
                            yanshi_render::selection::ensure_bitmap_masks_registered(
                                &[&sel.shape],
                                &|h| {
                                    h.parse::<yanshi_core::BlobHash>()
                                        .ok()
                                        .and_then(|hash| ctx.workspace.store().get(&hash).ok())
                                },
                            );
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
        let mut dropped = 0usize;
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
                    0.0
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
    value["max_channel_delta"] = json!(max_delta);
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
}

impl PrimShape {
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
