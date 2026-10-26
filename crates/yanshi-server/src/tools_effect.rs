//! **滤镜/效果的参数校验**（`validate_effect` 及其辅助）。
//!
//! **为什么拆出来**（色彩体系规划 **Phase 2** + 外部评审报告第 15 项）。
//! 本模块是**纯搬运**：行为**零变化**（全量测试通过即验收）。
//!
//! **边界**：起点 `validate_effect`，终点 `write_add_effect` 之前（后者是工具入口，留在 `tools.rs`）。

use super::*;

pub(super) fn validate_effect(kind: EffectKind, name: &str, params: &Value) -> Result<()> {
    // **★ 滤镜别名也要放行 ★**（第 1000+ 轮 ✓）： 早已把
    // ／ 映射到  ✓（），
    // 但  这个**被  对账**的常量里只列了  ✗
    // ⇒ **∴ 直接给常量加名字会破坏契约（实测 CI 红 ✓）⇒ **∴ 在这一层放行 ✓**，
    // **∴ 常量保持 14 ✓、 也能接受别名 ✓。**
    let aliased = kind == EffectKind::Filter && matches!(name, "emboss" | "impasto");
    // **调整别名也要放行**（2026-10-21 包 B）：`vibrance_natural` 是内核 `from_name`
    // 保留的 Vibrance 旧别名（已落存文档靠它渲染）；规范名 `vibrance` 本就在白名单里。
    // 此前别名被这一关拒绝 ⇒ 报「内核未实现」⇒ 真 Vibrance 两条路都不可达 ✗。
    let aliased = aliased || (kind == EffectKind::Adjustment && name == "vibrance_natural");
    if !kind.supported().contains(&name) && !aliased {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "内核未实现该{}：{name}；支持 {}",
                match kind {
                    EffectKind::Adjustment => "调整类型",
                    EffectKind::Filter => "滤镜",
                },
                kind.supported().join(" / ")
            )),
        ));
    }
    let Some(object) = params.as_object() else {
        if params.is_null() {
            return Ok(());
        }
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("params 必须是 JSON 对象"),
        ));
    };
    let number = |key: &str| object.get(key).and_then(Value::as_f64);
    // 参数范围按内核的数学假设逐项校验。注意同名参数在不同效果里含义不同
    // （`radius` 对模糊滤镜是像素、对暗角是归一化半径；`amount` 对饱和度/锐化/噪点各异），
    // 因此必须**按效果名**分别限定，否则会出现「合法参数被拒」或「非法参数放行」。
    // 别名按规范名查表（只影响本函数的校验口径，落存的原始名字不动）：
    // `vibrance_natural` 与 `vibrance` 同一张参数表（amount 0..2）。
    let name = match name {
        "vibrance_natural" => "vibrance".to_owned(),
        other => other.to_owned(),
    };
    let checks: &[(&str, f64, f64)] = match (kind, name.as_str()) {
        (EffectKind::Adjustment, "brightness_contrast") => {
            &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)]
        }
        (EffectKind::Adjustment, "saturation") => &[("amount", 0.0, 8.0)],
        (EffectKind::Adjustment, "invert") => &[],
        (EffectKind::Adjustment, "posterize") => &[("levels", 2.0, 64.0)],
        (EffectKind::Adjustment, "red_eye") => &[("strength", 0.0, 1.0), ("threshold", 0.5, 10.0)],
        (EffectKind::Adjustment, "vibrance") => &[("amount", 0.0, 2.0)],
        // 色彩平衡的档位是 [r,g,b]（分量范围在下面的逐项检查里完成）；
        // amount 是期 3 A2 新增的数值键 [0,1]（非空表会同时启用未知键拒绝，
        // shadows/midtones/highlights 都在 NON_NUMERIC 白名单里，不受影响）。
        (EffectKind::Adjustment, "color_balance") => &[("amount", 0.0, 1.0)],
        (EffectKind::Adjustment, "split_toning") => &[("balance", -1.0, 1.0), ("amount", 0.0, 1.0)],
        (EffectKind::Adjustment, "denoise") => &[
            ("luminance", 0.0, 1.0),
            ("detail", 0.0, 1.0),
            ("color", 0.0, 1.0),
        ],
        (EffectKind::Adjustment, "lens") => &[
            ("k1", -1.0, 1.0),
            ("k2", -1.0, 1.0),
            ("tca_r", 0.9, 1.1),
            ("tca_b", 0.9, 1.1),
            ("vignette_strength", 0.0, 1.0),
        ],
        // 透视四边形在下面的逐项检查里完成（形状 + 凸性）。
        (EffectKind::Adjustment, "perspective") => &[("k1", -1.0, 1.0), ("k2", -1.0, 1.0)],
        (EffectKind::Adjustment, "levels") => &[
            ("black", 0.0, 1.0),
            ("white", 0.0, 1.0),
            ("gamma", 0.01, 10.0),
        ],
        (EffectKind::Adjustment, "exposure") => &[("ev", -10.0, 10.0)],
        (EffectKind::Adjustment, "white_balance") => {
            &[("temperature", -1.0, 1.0), ("tint", -1.0, 1.0)]
        }
        (EffectKind::Adjustment, "shadows_highlights") => {
            &[("shadows", -1.0, 1.0), ("highlights", -1.0, 1.0)]
        }
        // Presence 三件套（包 D）：统一单参数 amount −1..1，0 = 恒等（与 shadows_highlights 同口径 ✓）。
        (EffectKind::Adjustment, "dehaze") => &[("amount", -1.0, 1.0)],
        (EffectKind::Adjustment, "clarity") => &[("amount", -1.0, 1.0)],
        (EffectKind::Adjustment, "texture") => &[("amount", -1.0, 1.0)],
        (EffectKind::Adjustment, "curves") => &[],
        (EffectKind::Adjustment, "hsl") => &[
            ("hue", -360.0, 360.0),
            ("saturation", 0.0, 8.0),
            ("lightness", -1.0, 1.0),
        ],
        (EffectKind::Filter, "box_blur") => &[("radius", 1.0, 128.0), ("passes", 1.0, 8.0)],
        (EffectKind::Filter, "gaussian_blur") => &[("sigma", 0.05, 128.0)],
        (EffectKind::Filter, "motion_blur") => &[
            ("angle", -360.0, 360.0),
            ("distance", 0.0, 512.0),
            ("samples", 2.0, 64.0),
        ],
        (EffectKind::Filter, "sharpen") => &[("amount", 0.0, 5.0), ("radius", 1.0, 8.0)],
        (EffectKind::Filter, "noise") => &[("amount", 0.0, 1.0), ("seed", 0.0, u64::MAX as f64)],
        (EffectKind::Filter, "clarity") => &[("amount", 0.0, 2.0), ("radius", 2.0, 64.0)],
        (EffectKind::Filter, "dehaze") => &[("omega", 0.0, 1.0), ("floor", 0.02, 0.8)],
        (EffectKind::Filter, "film_grain") => &[("amount", 0.0, 1.0), ("size", 1.0, 8.0)],
        (EffectKind::Filter, "glow") => &[
            ("threshold", 0.0, 1.0),
            ("radius", 1.0, 64.0),
            ("intensity", 0.0, 4.0),
        ],
        (EffectKind::Filter, "vignette") => &[
            ("strength", 0.0, 1.0),
            ("radius", 0.0, 2.0),
            ("softness", 0.02, 2.0),
        ],
        (EffectKind::Filter, "brightness_contrast") => {
            &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)]
        }
        (EffectKind::Filter, "saturation") => &[("amount", 0.0, 8.0)],
        (EffectKind::Filter, "invert") => &[],
        _ => &[],
    };
    for (key, low, high) in checks {
        if let Some(value) = number(key) {
            if !(*low..=*high).contains(&value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{name} 的 {key} 必须在 [{low}, {high}] 内，得到 {value}"
                    )),
                ));
            }
        }
    }
    // **未知参数键必须**报错**，不许静默忽略 ✓**（第 1516 轮；外部实测被它坑过 ✗）
    //   **⇒ 他们的原话**：「我试 `black_point` 时它**接受了但什么都没做**」
    //     ⇒ **「接受但无效」比「拒绝」更难查** ✓（与本仓「静默降级」同一族）
    //   **⇒ ⇒ 只在这个效果的键表**非空**时检查** ✓：`curves`／`color_balance` 等的表是空的
    //     （它们的参数是嵌套结构或在别处校验）⇒ **不能一律拒** ✗
    //   **⇒ ⇒ ⇒ 而 `levels` 额外接受 `channel`** ⇒ 也算已知键 ✓
    if !checks.is_empty() {
        // **非数值键的全局白名单** ✓（第 1519 轮修：第一版只写了 `levels` 的 `channel`，
        //   ⇒ **`dehaze` 的 `air` 被误拒** ⇒ `test (stable)` 红了两条 ✗）。
        //   **⇒ 这些键是**数组/结构**（`air: [r,g,b]`、`points`、`matrix`、`weights`…），
        //     `checks` 表只收**数值**键，所以它们天然不在表里 ✓
        //   **⇒ ⇒ 而"白名单"必须覆盖**所有滤镜读过的键**（`params.get(...)` 的并集）✓
        const NON_NUMERIC: &[&str] = &[
            "adjust",
            "air",
            "angle",
            "auto_crop",
            "balance",
            "channel",
            "color",
            "constants",
            "detail",
            "highlights",
            "hue",
            "levels",
            "matrix",
            "midtones",
            "omega",
            "path",
            "points",
            "quad",
            "ranges",
            "samples",
            "seed",
            "shadows",
            "size",
            "stops",
            "table",
            "tca_b",
            "tca_r",
            "tint",
            "weights",
        ];
        let extra: &[&str] = NON_NUMERIC;
        for key in object.keys() {
            let known = checks.iter().any(|(k, _, _)| k == key) || extra.contains(&key.as_str());
            if !known {
                // **只列**本效果自己的数值键** ✓（第 1519 轮修：第一版把 25 个全局键全列出来 ⇒ 太吵）
                let mut allowed: Vec<&str> = checks.iter().map(|(k, _, _)| *k).collect();
                allowed.sort_unstable();
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "{name} 不认识参数 `{key}`（**不会静默忽略**）⇒ 数值键：{}；\
                         另有若干数组类键（如 air／points／matrix／weights）按各自说明使用",
                        allowed.join(" / ")
                    )),
                ));
            }
        }
    }
    if name == "levels" {
        if let Some(channel) = object.get("channel").and_then(Value::as_str) {
            // 分通道色阶：与 curves 同一套通道命名。
            if !["rgb", "r", "g", "b"].contains(&channel) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "levels 的 channel 只能是 rgb/r/g/b，得到 {channel}"
                    )),
                ));
            }
        }
        if let (Some(black), Some(white)) = (number("black"), number("white")) {
            if black >= white {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "levels 需要 black < white，得到 {black} / {white}"
                    )),
                ));
            }
        }
    }
    if name == "dehaze" && kind == EffectKind::Filter {
        // 大气光必须是调用方给定的 [r,g,b]（滤镜内部不做全局估计，否则分块 != 整幅）。
        // **只限滤镜版**：调整版 dehaze（包 D）在渲染层内估计大气光、不收 air 参数，
        // 若不分 kind 拦截，调整版会被误要求 air（vibrance 式跨层口径不一致）✗。
        let air = object.get("air").ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 需要 air: [r,g,b]（可用 estimate_dehaze 估计）"),
            )
        })?;
        let Some(array) = air.as_array() else {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 的 air 必须是 [r,g,b]"),
            ));
        };
        if array.len() != 3 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("dehaze 的 air 必须是三个分量"),
            ));
        }
        for component in array {
            let Some(value) = component.as_f64() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("dehaze 的 air 分量必须是数字"),
                ));
            };
            if !(0.0..=1.0).contains(&value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "dehaze 的 air 分量必须在 [0, 1] 内，得到 {value}"
                    )),
                ));
            }
        }
    }
    if name == "color_balance" || name == "split_toning" {
        let bands: &[&str] = if name == "color_balance" {
            &["shadows", "midtones", "highlights"]
        } else {
            &["shadows", "highlights"]
        };
        for key in bands {
            let key = *key;
            let Some(value) = object.get(key) else {
                continue;
            };
            let Some(array) = value.as_array() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{name} 的 {key} 必须是 [r,g,b]")),
                ));
            };
            if array.len() != 3 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{name} 的 {key} 必须是三个分量")),
                ));
            }
            for component in array {
                let Some(value) = component.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("{name} 的 {key} 分量必须是数字")),
                    ));
                };
                if !(-1.0..=1.0).contains(&value) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "{name} 的 {key} 分量必须在 [-1, 1] 内，得到 {value}"
                        )),
                    ));
                }
            }
        }
    }
    if name == "lens" || name == "perspective" {
        // auto_crop 若给出必须是布尔（**静默降级**形态④：非法输入看起来正常 ✗）。
        if let Some(value) = object.get("auto_crop") {
            if !value.is_boolean() {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!("{name} 的 auto_crop 必须是布尔，得到 {value}")),
                ));
            }
        }
    }
    if name == "perspective" {
        // 四边形：4 个 [x, y]，必须凸（叉积同号；内核与工具层用同一判据 ✓）。
        let quad = object.get("quad").ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("perspective 需要 quad: [[x,y]×4]"),
            )
        })?;
        let points = quad.as_array().ok_or_else(|| {
            YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("perspective 的 quad 必须是 4 个 [x,y]"),
            )
        })?;
        if points.len() != 4 {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "perspective 的 quad 必须有 4 个点，得到 {}",
                    points.len()
                )),
            ));
        }
        let mut coords = [[0.0f64; 2]; 4];
        for (i, point) in points.iter().enumerate() {
            let pair = point
                .as_array()
                .filter(|array| array.len() == 2)
                .ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("perspective 的 quad[{i}] 必须是 [x,y]")),
                    )
                })?;
            for (j, value) in pair.iter().enumerate() {
                coords[i][j] = value.as_f64().ok_or_else(|| {
                    YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("perspective 的 quad[{i}][{j}] 必须是数字")),
                    )
                })?;
            }
        }
        let mut sign = 0.0f64;
        for i in 0..4 {
            let (a, b, c) = (coords[i], coords[(i + 1) % 4], coords[(i + 2) % 4]);
            let cross = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
            if cross.abs() < 1e-9 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("perspective 的四边形必须严格凸（共线顶点不接受）"),
                ));
            }
            if sign == 0.0 {
                sign = cross.signum();
            } else if cross.signum() != sign {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("perspective 的四边形必须凸（检测到凹顶点）"),
                ));
            }
        }
    }
    if name == "curves" {
        if let Some(channel) = object.get("channel").and_then(Value::as_str) {
            if !["rgb", "r", "g", "b", "l", "h", "s"].contains(&channel) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "curves 的 channel 只能是 rgb/r/g/b/l/h/s，得到 {channel}"
                    )),
                ));
            }
        }
        if let Some(points) = object.get("points") {
            let Some(points) = points.as_array() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("curves 的 points 必须是 [[x,y], ...] 数组"),
                ));
            };
            if points.len() < 2 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("curves 至少需要两个控制点"),
                ));
            }
            for point in points {
                let Some(pair) = point.as_array() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("curves 的控制点必须是 [x,y]"),
                    ));
                };
                if pair.len() != 2 {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("curves 的控制点必须是 [x,y]"),
                    ));
                }
                for value in pair {
                    let Some(value) = value.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("curves 控制点必须是数字"),
                        ));
                    };
                    if !(0.0..=1.0).contains(&value) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "curves 控制点必须在 [0,1] 归一化空间，得到 {value}"
                            )),
                        ));
                    }
                }
            }
        }
    }
    if name == "black_and_white" {
        // weights：三个数字即可（允许负权／大于 1 的权，只要有限）。
        if let Some(weights) = object.get("weights") {
            let Some(array) = weights.as_array().filter(|array| array.len() == 3) else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("black_and_white 的 weights 必须是三个分量"),
                ));
            };
            for component in array {
                let Some(value) = component.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("black_and_white 的 weights 分量必须是数字"),
                    ));
                };
                if !value.is_finite() {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "black_and_white 的 weights 分量必须是有限数，得到 {value}"
                        )),
                    ));
                }
            }
        }
        // 期 4 D2-2：amount 可选，数字且在 [0,1] 内（越界报错，不静默钳制）。
        if let Some(amount) = object.get("amount") {
            let Some(value) = amount.as_f64() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("black_and_white 的 amount 必须是数字"),
                ));
            };
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "black_and_white 的 amount 必须在 [0,1] 内，得到 {value}"
                    )),
                ));
            }
        }
        if let Some(tint) = object.get("tint") {
            let Some(array) = tint.as_array().filter(|array| array.len() == 3) else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("black_and_white 的 tint 必须是 [r,g,b]"),
                ));
            };
            for component in array {
                let Some(value) = component.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("black_and_white 的 tint 分量必须是数字"),
                    ));
                };
                if !(0.0..=1.0).contains(&value) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "black_and_white 的 tint 分量必须在 [0,1] 内，得到 {value}"
                        )),
                    ));
                }
            }
        }
        // D2-2b：ranges 结构校验——档名白名单、每档一个乘子、域 [0, 2]；
        // 未知档名/非数字/越界 ⇒ 报错且零落账（调用方在落成前全量校验）。
        if let Some(ranges) = object.get("ranges") {
            let Some(ranges_obj) = ranges.as_object() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("black_and_white 的 ranges 必须是对象"),
                ));
            };
            for (range_name, range_value) in ranges_obj {
                if !yanshi_render::filter::HSL_RANGE_NAMES.contains(&range_name.as_str()) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "black_and_white 的 ranges 档名「{range_name}」不认识；可用：{}",
                            yanshi_render::filter::HSL_RANGE_NAMES.join(" / ")
                        )),
                    ));
                }
                let Some(number) = range_value.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "black_and_white 的 ranges.{range_name} 必须是数字"
                        )),
                    ));
                };
                if !number.is_finite() || !(0.0..=2.0).contains(&number) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "black_and_white 的 ranges.{range_name} 必须在 [0, 2] 内，得到 {number}"
                        )),
                    ));
                }
            }
        }
    }
    // 期 4 D2-3：hsl 的 ranges 结构校验——档名白名单、三键数值域；
    // 未知档名/未知键 ⇒ 报错且零落账（调用方在落成前全量校验）。
    if name == "hsl" {
        if let Some(ranges) = object.get("ranges") {
            let Some(ranges_obj) = ranges.as_object() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("hsl 的 ranges 必须是对象"),
                ));
            };
            for (range_name, range_value) in ranges_obj {
                if !yanshi_render::filter::HSL_RANGE_NAMES.contains(&range_name.as_str()) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "hsl 的 ranges 档名「{range_name}」不认识；可用：{}",
                            yanshi_render::filter::HSL_RANGE_NAMES.join(" / ")
                        )),
                    ));
                }
                let Some(range_obj) = range_value.as_object() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("hsl 的 ranges.{range_name} 必须是对象")),
                    ));
                };
                for (key, value) in range_obj {
                    let domain = match key.as_str() {
                        "hue" => (-180.0, 180.0),
                        "saturation" | "lightness" => (-100.0, 100.0),
                        other => {
                            return Err(YanshiError::new(
                                ErrorCode::InvalidArgument,
                                ErrorContext::detail(format!(
                                    "hsl 的 ranges.{range_name} 有未知键「{other}」；只收 hue / saturation / lightness"
                                )),
                            ));
                        }
                    };
                    let Some(number) = value.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "hsl 的 ranges.{range_name}.{key} 必须是数字"
                            )),
                        ));
                    };
                    if !number.is_finite() || !(domain.0..=domain.1).contains(&number) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "hsl 的 ranges.{range_name}.{key} 必须在 [{}, {}] 内，得到 {number}",
                                domain.0, domain.1
                            )),
                        ));
                    }
                }
            }
        }
    }
    if name == "channel_mixer" {
        if let Some(matrix) = object.get("matrix") {
            let Some(rows) = matrix.as_array().filter(|array| array.len() == 3) else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("channel_mixer 的 matrix 必须是 3×3"),
                ));
            };
            for row in rows {
                let Some(columns) = row.as_array().filter(|array| array.len() == 3) else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("channel_mixer 的 matrix 必须是 3×3"),
                    ));
                };
                for value in columns {
                    let Some(number) = value.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("channel_mixer 的 matrix 必须是数字"),
                        ));
                    };
                    if !number.is_finite() {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "channel_mixer 的 matrix 必须是有限数，得到 {number}"
                            )),
                        ));
                    }
                }
            }
        }
        if let Some(constants) = object.get("constants") {
            let Some(array) = constants.as_array().filter(|array| array.len() == 3) else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("channel_mixer 的 constants 必须是三个分量"),
                ));
            };
            for component in array {
                let Some(value) = component.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("channel_mixer 的 constants 分量必须是数字"),
                    ));
                };
                if !value.is_finite() {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "channel_mixer 的 constants 分量必须是有限数，得到 {value}"
                        )),
                    ));
                }
            }
        }
    }
    if name == "calibrate_primaries" {
        for key in [
            "red_hue",
            "red_sat",
            "green_hue",
            "green_sat",
            "blue_hue",
            "blue_sat",
        ] {
            if let Some(value) = object.get(key) {
                let Some(number) = value.as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!("calibrate_primaries 的 {key} 必须是数字")),
                    ));
                };
                if !number.is_finite() || !(-100.0..=100.0).contains(&number) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "calibrate_primaries 的 {key} 必须在 [-100, 100] 之间，得到 {number}"
                        )),
                    ));
                }
            }
        }
    }
    if name == "gradient_map" {
        if let Some(stops) = object.get("stops") {
            let Some(array) = stops.as_array() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("gradient_map 的 stops 必须是 [[位置,[r,g,b]],...] 数组"),
                ));
            };
            if array.len() < 2 {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("gradient_map 的 stops 至少需要两个色标"),
                ));
            }
            for stop in array {
                let Some(pair) = stop.as_array().filter(|pair| pair.len() == 2) else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("gradient_map 的色标必须是 [位置, [r,g,b]]"),
                    ));
                };
                let Some(pos) = pair[0].as_f64() else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("gradient_map 色标位置必须是数字"),
                    ));
                };
                if !(0.0..=1.0).contains(&pos) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "gradient_map 色标位置必须在 [0,1] 内，得到 {pos}"
                        )),
                    ));
                }
                let Some(color) = pair[1].as_array().filter(|color| color.len() == 3) else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail("gradient_map 色标颜色必须是 [r,g,b]"),
                    ));
                };
                for component in color {
                    let Some(value) = component.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("gradient_map 色标颜色分量必须是数字"),
                        ));
                    };
                    if !(0.0..=1.0).contains(&value) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "gradient_map 色标颜色分量必须在 [0,1] 内，得到 {value}"
                            )),
                        ));
                    }
                }
            }
        }
    }
    if name == "selective_color" {
        if let Some(adjust) = object.get("adjust") {
            let Some(map) = adjust.as_object() else {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("selective_color 的 adjust 必须是对象"),
                ));
            };
            for (key, value) in map {
                if !yanshi_render::SELECTIVE_COLOR_RANGES.contains(&key.as_str()) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "selective_color 的 adjust 未知色类 {key}；可选 {}",
                            yanshi_render::SELECTIVE_COLOR_RANGES.join(" / ")
                        )),
                    ));
                }
                let Some(array) = value.as_array().filter(|array| array.len() == 4) else {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "selective_color 的 {key} 必须是 [c,m,y,k] 四个分量"
                        )),
                    ));
                };
                for component in array {
                    let Some(offset) = component.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "selective_color 的 {key} 分量必须是数字"
                            )),
                        ));
                    };
                    if !(-1.0..=1.0).contains(&offset) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "selective_color 的 {key} 分量必须在 [-1,1] 内，得到 {offset}"
                            )),
                        ));
                    }
                }
            }
        }
    }
    if name == "color_lut" {
        // 给了 path：文件内容由 add/update 路径上的 resolve_lut_path 解析校验
        // （preview 走同一校验函数但不读盘 ⇒ 在这里放行 path 本身）。
        let has_path = object.get("path").and_then(Value::as_str).is_some();
        let size = object.get("size").and_then(Value::as_u64);
        let table = object.get("table").and_then(Value::as_array);
        match (size, table) {
            (Some(size), Some(table)) => {
                if !(2..=64).contains(&size) {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "color_lut 的 size 必须在 [2,64] 内，得到 {size}"
                        )),
                    ));
                }
                let expected = (size as usize).pow(3) * 3;
                if table.len() != expected {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(format!(
                            "color_lut 的 table 长度应为 {expected}（size³×3），得到 {}",
                            table.len()
                        )),
                    ));
                }
                for component in table {
                    let Some(value) = component.as_f64() else {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail("color_lut 的 table 分量必须是数字"),
                        ));
                    };
                    if !(0.0..=1.0).contains(&value) {
                        return Err(YanshiError::new(
                            ErrorCode::InvalidArgument,
                            ErrorContext::detail(format!(
                                "color_lut 的 table 分量必须在 [0,1] 内，得到 {value}"
                            )),
                        ));
                    }
                }
            }
            (None, None) => {
                if !has_path {
                    return Err(YanshiError::new(
                        ErrorCode::InvalidArgument,
                        ErrorContext::detail(
                            "color_lut 需要 path（.cube 文件）或直接给 size + table",
                        ),
                    ));
                }
            }
            _ => {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail("color_lut 的 size 与 table 必须成对出现"),
                ));
            }
        }
    }
    Ok(())
}

/// **Presence 三件套的服务端校验（包 D）**：名单放行 + amount 范围 + 调整版 dehaze 不收 air。
#[cfg(test)]
mod presence_validation_tests {
    use super::super::EffectKind;
    use super::validate_effect;
    use serde_json::json;

    #[test]
    fn presence_adjustments_pass_server_validation() {
        for name in ["dehaze", "clarity", "texture"] {
            assert!(
                validate_effect(EffectKind::Adjustment, name, &json!({"amount": 0.5})).is_ok(),
                "{name} amount=0.5 应通过服务端校验"
            );
            assert!(
                validate_effect(EffectKind::Adjustment, name, &json!({"amount": -1.0})).is_ok(),
                "{name} amount=-1 应通过（边界）"
            );
            assert!(
                validate_effect(EffectKind::Adjustment, name, &json!({"amount": 1.5})).is_err(),
                "{name} amount=1.5 应被服务端拒绝"
            );
        }
    }

    #[test]
    fn black_and_white_amount_is_validated_in_zero_to_one() {
        // 期 4 D2-2：amount 可选，数字且在 [0,1] 内；越界报错，不静默钳制。
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"weights": [0.2126, 0.7152, 0.0722], "amount": 0.5})
            )
            .is_ok(),
            "amount=0.5 应通过"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"weights": [0.2126, 0.7152, 0.0722]})
            )
            .is_ok(),
            "无 amount 键（老文档 = 1.0）应通过"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"weights": [0.2126, 0.7152, 0.0722], "amount": 1.5})
            )
            .is_err(),
            "amount=1.5 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"weights": [0.2126, 0.7152, 0.0722], "amount": "half"})
            )
            .is_err(),
            "amount 非数字应被拒绝"
        );
    }

    #[test]
    fn bw_ranges_unknown_key_rejected() {
        // D2-2b：未知档名/非数字/越界值 ⇒ 报错（落成前全量校验 ⇒ 零落账）。
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"rose": 1.0}})
            )
            .is_err(),
            "未知档名 rose 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"reds": "high"}})
            )
            .is_err(),
            "非数字档值应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"reds": 2.5}})
            )
            .is_err(),
            "档值 2.5 超出 [0,2] 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"reds": -0.5}})
            )
            .is_err(),
            "档值 -0.5 超出 [0,2] 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"reds": 1.5, "greens": 0.5}})
            )
            .is_ok(),
            "合法 ranges 应通过"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "black_and_white",
                &json!({"ranges": {"reds": 2.0}})
            )
            .is_ok(),
            "边界值 2.0 应通过"
        );
    }

    #[test]
    fn hsl_ranges_unknown_key_rejected() {
        // 期 4 D2-3：未知档名/未知键/越界值 ⇒ 报错（落成前全量校验 ⇒ 零落账）。
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "hsl",
                &json!({"ranges": {"rose": {"saturation": 10}}})
            )
            .is_err(),
            "未知档名 rose 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "hsl",
                &json!({"ranges": {"reds": {"sat": 10}}})
            )
            .is_err(),
            "未知键 sat 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "hsl",
                &json!({"ranges": {"reds": {"saturation": 150}}})
            )
            .is_err(),
            "饱和度 150 超出 [-100,100] 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "hsl",
                &json!({"ranges": {"reds": {"saturation": 50}}})
            )
            .is_ok(),
            "合法 ranges 应通过"
        );
    }

    #[test]
    fn hsl_scalar_bounds_align_with_renderer() {
        // G-09 补充件：标量表须与渲染器 `hsl_with_ranges` 的钳制逐值对齐
        // （hue ±360、saturation 0..8、lightness ±1）；旧表 hue ±180 / sat 0..4
        // 会把渲染器接得住的合法调用先拒了。
        for (params, what) in [
            (json!({"hue": 270.0}), "hue 270 应通过（渲染器钳 ±360）"),
            (json!({"hue": -360.0}), "hue -360 应通过（边界）"),
            (
                json!({"saturation": 6.0}),
                "saturation 6 应通过（渲染器钳 0..8）",
            ),
            (json!({"saturation": 8.0}), "saturation 8 应通过（边界）"),
            (
                json!({"lightness": 1.0}),
                "lightness 1 应通过（边界，未动）",
            ),
        ] {
            assert!(
                validate_effect(EffectKind::Adjustment, "hsl", &params).is_ok(),
                "{what}"
            );
        }
        for (params, what) in [
            (json!({"hue": 361.0}), "hue 361 应被拒绝"),
            (json!({"saturation": 8.1}), "saturation 8.1 应被拒绝"),
            (json!({"saturation": -0.1}), "saturation -0.1 应被拒绝"),
            (json!({"lightness": 1.1}), "lightness 1.1 应被拒绝"),
        ] {
            assert!(
                validate_effect(EffectKind::Adjustment, "hsl", &params).is_err(),
                "{what}"
            );
        }
    }

    #[test]
    fn adjustment_dehaze_does_not_require_air_but_filter_dehaze_does() {
        // **回归守卫**：air 必填检查若不分 kind，调整版 dehaze 会被误拒（vibrance 式口径不一致）✗。
        assert!(
            validate_effect(EffectKind::Adjustment, "dehaze", &json!({"amount": 0.5})).is_ok(),
            "调整版 dehaze 不应要求 air"
        );
        assert!(
            validate_effect(EffectKind::Filter, "dehaze", &json!({})).is_err(),
            "滤镜版 dehaze 仍应要求 air"
        );
        assert!(
            validate_effect(
                EffectKind::Filter,
                "dehaze",
                &json!({"air": [0.85, 0.88, 0.9]})
            )
            .is_ok(),
            "滤镜版 dehaze 给了 air 应通过"
        );
    }

    #[test]
    fn calibrate_primaries_params_are_validated_in_minus_100_to_100() {
        // D1-c：六个参数各可选、数字且在 [-100, 100] 内；越界/非数字报错，不静默钳制。
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "calibrate_primaries",
                &json!({"red_hue": 25.0, "blue_hue": -100.0})
            )
            .is_ok(),
            "合法参数应通过"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "calibrate_primaries",
                &json!({"red_sat": 100.0, "green_sat": -100.0})
            )
            .is_ok(),
            "边界 ±100 应通过"
        );
        assert!(
            validate_effect(EffectKind::Adjustment, "calibrate_primaries", &json!({})).is_ok(),
            "无参数（全缺省 0）应通过"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "calibrate_primaries",
                &json!({"blue_hue": 100.5})
            )
            .is_err(),
            "blue_hue=100.5 超出 [-100,100] 应被拒绝"
        );
        assert!(
            validate_effect(
                EffectKind::Adjustment,
                "calibrate_primaries",
                &json!({"red_sat": "high"})
            )
            .is_err(),
            "red_sat 非数字应被拒绝"
        );
    }
}
