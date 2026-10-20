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
    let name = name.to_owned();
    let checks: &[(&str, f64, f64)] = match (kind, name.as_str()) {
        (EffectKind::Adjustment, "brightness_contrast") => {
            &[("brightness", -1.0, 1.0), ("contrast", 0.0, 8.0)]
        }
        (EffectKind::Adjustment, "saturation") => &[("amount", 0.0, 8.0)],
        (EffectKind::Adjustment, "invert") => &[],
        (EffectKind::Adjustment, "posterize") => &[("levels", 2.0, 64.0)],
        (EffectKind::Adjustment, "vibrance") => &[("amount", 0.0, 2.0)],
        // 色彩平衡 / 分离色调的档位是 [r,g,b]，分量范围在下面的逐项检查里完成。
        (EffectKind::Adjustment, "color_balance") => &[],
        (EffectKind::Adjustment, "split_toning") => &[("balance", -1.0, 1.0), ("amount", 0.0, 1.0)],
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
        (EffectKind::Adjustment, "curves") => &[],
        (EffectKind::Adjustment, "hsl") => &[
            ("hue", -180.0, 180.0),
            ("saturation", 0.0, 4.0),
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
    if name == "dehaze" {
        // 大气光必须是调用方给定的 [r,g,b]（滤镜内部不做全局估计，否则分块 != 整幅）。
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
