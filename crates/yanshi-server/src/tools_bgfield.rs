//! **`fit_background_field`**：把**掩膜外**的像素拟合成**平滑背景场**，铺进**掩膜内**（第 1447 轮）。
//!
//! **⇒ 为什么有这个工具** ✓：外部（apple2011）在做"只画背景"，他的原话是
//!   「一个工具直接把"掩膜外像素"拟合成平滑背景场并铺上 ⇒ 我就不用在自己这边写多项式」✓
//! **⇒ ⇒ 而这个需求在本仓**本来就重复过**：`smart_sample` 也在用 8×8 的场近似参考图，
//!   而他刚刚诊断出「插值点太少 ⇒ 必然斑块/竖纹」（`E1408`）⇒ **多项式场正好治根** ✓
//!
//! **⇒ 与 `smart_sample` 的分工**（写清楚，免得混用 ✗）：
//! - **`smart_sample`**：把场变成**笔触**（要笔刷物理、要"画"的感觉）
//! - **`fit_background_field`**：把场变成**像素**（不要笔触，只要"平滑的底"）
//!   ⇒ **两者互补**：底用这个（一次落平），上面再用笔触画 ✓
//!
//! **⇒ 为什么用**多项式**而不是 bicubic 插值** ✗：
//! 多项式是**全局**的（整幅一个式子）⇒ **不存在"格点"** ⇒ **结构上不可能有折痕** ✓；
//! 而 bicubic 只是把折痕从 C⁰ 提到 C¹，采样线仍在 ✓
//! ⇒ 代价：多项式**不能表示高频**（细节多的背景会糊）⇒ 所以 `degree` 可调到 3，且**返回残差**让调用方判断 ✓

use serde_json::{json, Value};

use yanshi_core::Bbox;
use yanshi_render::png::encode_png;

use super::*;

/// 多项式项数（degree ⇒ 项数）。
#[inline]
fn term_count(degree: u32) -> usize {
    ((degree + 1) * (degree + 2) / 2) as usize
}

/// 二维单项式基（归一化坐标 `u,v ∈ [0,1]`）在下标 `i` 处的值。
///
/// 顺序：`1, u, v, u², uv, v², u³, u²v, uv², v³`（三角序 ✓）。
#[inline]
fn basis(i: usize, u: f64, v: f64) -> f64 {
    // 把线性下标展开成 (a,b)，a+b ≤ degree：按行枚举
    let mut idx = 0usize;
    let mut a_max = 0usize;
    loop {
        for a in (0..=a_max).rev() {
            let b = a_max - a;
            if idx == i {
                return u.powi(a as i32) * v.powi(b as i32);
            }
            idx += 1;
        }
        a_max += 1;
        if a_max > 8 {
            return 0.0;
        }
    }
}

/// 高斯消元（列主元）解 `m x = b`；奇异时返回 `None`。
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        // 列主元
        let mut piv = col;
        for row in col + 1..n {
            if a[row][col].abs() > a[piv][col].abs() {
                piv = row;
            }
        }
        if a[piv][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let d = a[col][col];
        for slot in a[col].iter_mut().skip(col) {
            *slot /= d;
        }
        b[col] /= d;
        for row in 0..n {
            if row == col {
                continue;
            }
            let f = a[row][col];
            if f == 0.0 {
                continue;
            }
            // 先取出枢轴行（避免同时借用 `a` 的两行 ✗）
            let pivot: Vec<f64> = a[col].iter().skip(col).copied().collect();
            for (k, slot) in a[row].iter_mut().enumerate().skip(col) {
                *slot -= f * pivot[k - col];
            }
            b[row] -= f * b[col];
        }
    }
    Some(b)
}

/// **`fit_background_field`** ✓
pub(super) fn write_fit_background_field(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let mask = args.get("mask").ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("缺少 mask（掩膜形状，写法同 fill_region 的 shape）"),
        )
    })?;
    let parsed = parse_smart_shape(mask, "fit_background_field")?;
    let degree = args
        .get("degree")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        // **`degree=0` 允许** ✓（第 1458 轮）：它只有常数项 ⇒ **等于"用掩膜外的均值填满"** ✓
        //   ⇒ 这是个**合理的用途**（不要任何渐变、只要"和周围一致的平均色"）✗ 之前被 `clamp(1,3)` 挡掉 ✗
        .clamp(0, 3) as u32;
    let stride = args
        .get("sample_stride")
        .and_then(Value::as_u64)
        .unwrap_or(2)
        .clamp(1, 16) as usize;
    let margin_frac = args
        .get("margin")
        .and_then(Value::as_f64)
        .unwrap_or(0.25)
        .clamp(0.0, 2.0);

    let (bx0, by0, bx1, by1) = parsed.bbox;
    let (mw, mh) = ((bx1 - bx0).max(1.0), (by1 - by0).max(1.0));
    // **采样区**：掩膜 bbox 外扩 `margin` 倍 ⇒ 拟合有足够"环外"样本 ✓
    let mx = (mw * margin_frac).max(16.0);
    let my = (mh * margin_frac).max(16.0);
    let (doc_w, doc_h) = ctx
        .workspace
        .document(&ctx.doc_id)
        .map(|d| (d.state().width as f64, d.state().height as f64))
        .unwrap_or((4096.0, 4096.0));
    let sx0 = (bx0 - mx).clamp(0.0, doc_w - 1.0);
    let sy0 = (by0 - my).clamp(0.0, doc_h - 1.0);
    let sx1 = (bx1 + mx).clamp(sx0 + 1.0, doc_w);
    let sy1 = (by1 + my).clamp(sy0 + 1.0, doc_h);

    let region = Bbox::new(sx0, sy0, sx1, sy1);
    let (rw, rh, pixels) = ctx
        .workspace
        .render_region_raw_layer(&ctx.doc_id, region, &layer_id)?;
    if rw == 0 || rh == 0 || pixels.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("图层在掩膜周围没有像素 ⇒ 无从拟合".to_owned()),
        ));
    }

    let n = term_count(degree);
    // 正规方程 AᵀA c = Aᵀb（每通道一套 b，共用 AᵀA ✓）
    let mut ata = vec![vec![0.0f64; n]; n];
    let mut atb = [vec![0.0f64; n], vec![0.0f64; n], vec![0.0f64; n]];
    // **样本自身的波动**（第 1475 轮；外部 §224 提出「绝对阈值在低对比图上偏严」）
    //   ⇒ 一个**绝对**灰度阈值不可能同时适合**高对比**与**低对比**的底 ✓
    //   ⇒ ⇒ 所以同时报 `sample_std` 与**无量纲**的 `residual_ratio = rms / std` ✓
    let mut sum = [0.0f64; 3];
    let mut sumsq = [0.0f64; 3];
    let mut samples = 0usize;
    let use_count = rw as usize * rh as usize;
    let mut cache: Vec<[f64; 10]> = Vec::with_capacity(use_count);
    let mut ys: Vec<[f64; 3]> = Vec::with_capacity(use_count);
    for (idx, px) in pixels.chunks_exact(4).enumerate() {
        let gx = (idx % rw as usize) as f64;
        let gy = (idx / rw as usize) as f64;
        let dx = sx0 + gx + 0.5;
        let dy = sy0 + gy + 0.5;
        // 只取**掩膜外**的像素 ✓（掩膜内是要被填掉的区域）
        if parsed.contains(dx, dy) {
            continue;
        }
        if px[3] < 8 {
            continue;
        }
        if (gx as usize) % stride != 0 || (gy as usize) % stride != 0 {
            continue;
        }
        let u = (dx - sx0) / (sx1 - sx0).max(1e-9);
        let v = (dy - sy0) / (sy1 - sy0).max(1e-9);
        let mut row = [0.0f64; 10];
        for (i, slot) in row.iter_mut().enumerate().take(n) {
            *slot = basis(i, u, v);
        }
        for i in 0..n {
            for j in 0..n {
                ata[i][j] += row[i] * row[j];
            }
            for c in 0..3 {
                atb[c][i] += row[i] * f64::from(px[c]);
            }
        }
        for c in 0..3 {
            let v = f64::from(px[c]);
            sum[c] += v;
            sumsq[c] += v * v;
        }
        cache.push(row);
        ys.push([f64::from(px[0]), f64::from(px[1]), f64::from(px[2])]);
        samples += 1;
    }
    if samples < n * 2 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "掩膜外可用的采样点只有 {samples} 个（degree {degree} 至少要 {} 个）⇒ 请减小 degree 或扩大掩膜周围区域",
                n * 2
            )),
        ));
    }

    let mut coeffs = [[0.0f64; 10]; 3];
    for c in 0..3 {
        let sol = solve(ata.clone(), atb[c].clone()).ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("拟合矩阵奇异（采样点退化？）⇒ 请换 degree 或加大 margin"),
            )
        })?;
        for (i, v) in sol.iter().enumerate() {
            coeffs[c][i] = *v;
        }
    }
    // 残差（RMS）
    let mut sq = [0.0f64; 3];
    for (row, y) in cache.iter().zip(ys.iter()) {
        for c in 0..3 {
            let mut pred = 0.0;
            for (i, r) in row.iter().enumerate().take(n) {
                pred += coeffs[c][i] * r;
            }
            let d = pred - y[c];
            sq[c] += d * d;
        }
    }
    let rms = [
        (sq[0] / samples as f64).sqrt(),
        (sq[1] / samples as f64).sqrt(),
        (sq[2] / samples as f64).sqrt(),
    ];

    // **样本标准差**（总体，按通道）✓
    let sample_std = {
        let n_f = samples as f64;
        let mut sd = [0.0f64; 3];
        for c in 0..3 {
            let mean = sum[c] / n_f;
            sd[c] = (sumsq[c] / n_f - mean * mean).max(0.0).sqrt();
        }
        sd
    };

    // **铺进掩膜内**：只在掩膜 bbox 内建图，掩膜外的像素**全透明**（这样 import_image 只覆盖掩膜 ✓）
    let (ow, oh) = (mw.ceil() as u32, mh.ceil() as u32);
    if ow == 0 || oh == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("掩膜 bbox 为空".to_owned()),
        ));
    }
    let mut out = vec![0u8; (ow as usize) * (oh as usize) * 4];
    let mut filled = 0usize;
    // **填出来的场是不是**几乎单色**（第 1451 轮；外部 `§222`）
    //   **⇒ 为什么** ✗：他们报「对象存在但渲染空白」⇒ 我复现后发现**对象真的画了、画的是纯白**
    //     ⇒ 因为**掩膜外的样本本来就是白的** ⇒ 拟合出 255 ⇒ `mean 255 / std 0` ⇒ **看起来像没画** ✗
    //   **⇒ ⇒ 所以**：把"场几乎单色"这件事**说出来** ⇒ 调用方立刻知道"不是没画，是画成了单色" ✓
    let mut lo = u8::MAX;
    let mut hi = u8::MIN;
    // **过冲被夹紧的像素数**（第 1470 轮；与 `field_uniform` 同一类诊断）
    //   **⇒ 为什么** ✗：高阶多项式**会过冲** ⇒ 值可能 < 0 或 > 255 ⇒ 我**静默 clamp** ✗
    //     ⇒ 调用方看到"填进去了"，却不知道**有多少像素被改过值** ⇒ 与 `§222` 同类 ✓
    let mut clamped = 0usize;
    for oy in 0..oh {
        for ox in 0..ow {
            let dx = bx0 + f64::from(ox) + 0.5;
            let dy = by0 + f64::from(oy) + 0.5;
            if !parsed.contains(dx, dy) {
                continue;
            }
            let u = (dx - sx0) / (sx1 - sx0).max(1e-9);
            let v = (dy - sy0) / (sy1 - sy0).max(1e-9);
            let mut row = [0.0f64; 10];
            for (i, slot) in row.iter_mut().enumerate().take(n) {
                *slot = basis(i, u, v);
            }
            let i = ((oy as usize) * (ow as usize) + (ox as usize)) * 4;
            for c in 0..3 {
                let mut pred = 0.0;
                for (k, r) in row.iter().enumerate().take(n) {
                    pred += coeffs[c][k] * r;
                }
                // **故意不采纳 clippy 的 `!(-0.5..=255.5).contains(&pred)`** ✗（第 740 轮实测）：
                //   两种写法只在 **NaN** 上不同（`NaN < -0.5 ⇒ false`、`NaN > 255.5 ⇒ false` ⇒
                //   现式**不把 NaN 记进 `clamped`**；而 `contains` 对 NaN 返回 false ⇒ 改写**会多记** ✓）。
                //   本函数对 NaN 的既有语义是「**不算被夹回**」⇒ 现式才保得住 ✓（改完行为就变 ✗）。
                #[allow(clippy::manual_range_contains)]
                if pred < -0.5 || pred > 255.5 {
                    clamped += 1;
                }
                let v = pred.round().clamp(0.0, 255.0) as u8;
                out[i + c] = v;
                if v < lo {
                    lo = v;
                }
                if v > hi {
                    hi = v;
                }
            }
            out[i + 3] = 255;
            filled += 1;
        }
    }
    if filled == 0 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("掩膜内没有一个像素（形状太小或不在画布内）".to_owned()),
        ));
    }

    // 落成位图对象（与 import_image 同一条路径 ✓）
    let png = encode_png(ow, oh, &out).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("PNG 编码失败".to_owned()),
        )
    })?;
    let blob_hash = ctx.workspace.store().put(&png)?;
    let placed = super::tools_bitmap::write_import_image(
        ctx,
        &json!({
            "layer_id": layer_id,
            // **默认 id 必须**每次唯一** ✓（第 1448 轮修）
            //   **⇒ 原缺陷** ✗：写死成 `bgfield_{doc}` ⇒ **同一文档第二次调用报「对象已存在」** ✗
            //     ⇒ 于是"先试 degree 1 再试 3"这种**正常用法**直接失败 ✗（判据 ⑤ 当场抓到 ✓）
            //   **⇒ 修法**：带上文档的 `head_seq`（单调递增 ⇒ 唯一）✓
            //     调用方给了 `object_id` 就用它的 ✓
            "object_id": args
                .get("object_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| format!(
                    "bgfield_{}_{}",
                    ctx.doc_id,
                    ctx.workspace
                        .document(&ctx.doc_id)
                        .map(|d| d.state().head_seq)
                        .unwrap_or(0)
                )),
            "bitmap": {
                "blob_hash": blob_hash.to_string(),
                "size": png.len(),
                "mime_type": "image/png",
            },
            "region": { "x": bx0, "y": by0, "width": ow, "height": oh },
        }),
    )?;
    // **`import_image` 的失败必须**传播** ✓（第 1474 轮）
    //   **⇒ 为什么** ✗：我原来把它的结果原样塞进 `placed` 就返回 `ok: true` ⇒
    //     若它失败（图层类型不接受位图、blob 不可解码…）⇒ 我会**报成功而什么都没画** ✗
    //     ⇒ 那正是 `§222` 那一类"静默"的另一种形状 ✓
    //   **⇒ ⇒ 诚实说明**：**我构造不出**能触发它的 API 场景（矢量层实测也成功 ✓）⇒
    //     所以这条是**防御性**的：**不触发则行为不变**，触发则**给出真实原因** ✓
    if placed.get("ok").and_then(Value::as_bool) != Some(true) {
        let why = placed
            .get("context")
            .and_then(|c| c.get("detail"))
            .and_then(Value::as_str)
            .unwrap_or("（import_image 没给原因）");
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "拟合完成但**落位失败** ⇒ {why}（图层 {layer_id} 可能不接受位图对象）"
            )),
        ));
    }

    // **`warnings` 必须在 `json!` 之前算好** ✗（`json!` 里不能写语句块 ⇒ 第 1470 轮又踩一次）
    let mut warnings: Vec<String> = Vec::new();
    if (hi.saturating_sub(lo)) < 2 {
        warnings.push(format!(
            "拟合出来的场**几乎是单色**（{lo}..{hi}，跨度 < 2）⇒ 这**不是没画**，是**画成了单色** ✗；\
             常见原因：**掩膜外的像素本来就是单色**（空图层、或外面全被某个纯色盖住）\
             ⇒ 想让场有变化，请确保掩膜外是**真的有明暗变化**的背景"
        ));
    }
    if clamped > 0 {
        warnings.push(format!(
            "有 **{clamped}** 个通道值**超出 0-255** 被夹紧（多项式**过冲**）⇒ 这些像素的颜色**不是**拟合值 ✗\
             ⇒ 常见原因：**degree 太高**（换小一阶）或**掩膜外有离群亮/暗点**（加大 `margin` 让采样更远）"
        ));
    }
    let warnings_value = if warnings.is_empty() {
        Value::Null
    } else {
        json!(warnings)
    };

    Ok(json!({
        "ok": true,
        "degree": degree,
        // **生效值也报出来** ✓（第 1472 轮）：`sample_stride`/`margin` 被 `clamp` 时**原来静默** ✗
        //   ⇒ 与 `degree` 一致（它本来就报**生效值**）⇒ 让调用方**看得见夹紧** ✓
        // **改名成 `effective_*`** ✓（第 1478 轮；**外部的直觉**：`sample_stride`/`margin` 看起来像**输入**）
        //   ⇒ 响应里它们是**生效值**（可能被夹紧过）⇒ 名字**必须**说清这一点 ✓
        "effective_sample_stride": stride,
        "effective_margin": margin_frac,
        "samples": samples,
        "residual_rms": [rms[0], rms[1], rms[2]],
        // **相对残差（无量纲）** ⇒ 高对比与低对比**可比** ✓（第 1475 轮）
        "sample_std": [sample_std[0], sample_std[1], sample_std[2]],
        "residual_ratio": [
            if sample_std[0] > 1e-9 { rms[0] / sample_std[0] } else { 0.0 },
            if sample_std[1] > 1e-9 { rms[1] / sample_std[1] } else { 0.0 },
            if sample_std[2] > 1e-9 { rms[2] / sample_std[2] } else { 0.0 },
        ],
        "mask_bbox": [bx0, by0, bx1, by1],
        "filled_pixels": filled,
        "field_range": [lo, hi],
        "field_uniform": (hi.saturating_sub(lo)) < 2,
        "clamped_pixels": clamped,
        "warnings": warnings_value,
        "placed": placed,
    }))
}
