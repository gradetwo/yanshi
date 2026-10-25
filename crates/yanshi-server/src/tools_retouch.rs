//! 期 4 D1：频率分离 / 减淡加深（Retouch 层工具）。
//!
//! 两个工具都遵循「一个 changeset 落成」：先算像素、放 blob、再把
//! 一组原子一次提交；任何一步失败 ⇒ 零原子落账（不留半套组）。
//!
//! **空间口径（实测优先，见 phase4-notes.md）**：引擎位图层在**线性光
//! 预乘空间**合成（`tile.rs` + `blend.rs` 卷首），所以频率分离的载荷
//! 公式 HF = (I − LF)/2 + 0.5 必须落在**合成发生的线性空间**里算，
//! 再编码回 u8 烘焙；模糊本身仍在显示编码像素上算（与设计「对
//! RGBA8 缓冲施加」同形）。烘焙字节是显示编码，单看高频层仍是
//! 中灰细节图，编辑语义不变；半透明像素的组 alpha 不精确保持源
//! alpha（图层 alpha 合成规则所限），FS 按不透明照片层设计。

use serde_json::{json, Value};
use yanshi_core::{
    Atom, AtomKind, Bbox, Changeset, ErrorCode, ErrorContext, ObjectType, Result, YanshiError,
};
use yanshi_render::{
    blend_pixel,
    color::{srgb_decode_extended, srgb_encode_extended},
    filter::gaussian_blur_rgba8,
    BlendMode,
};

use super::{document_state, optional_str, require_str, ToolContext};
use crate::service::Workspace;

/// 频率分离重建容差（实测修正，见 [`fs_bake`] 的说明）：显示归一化单位。
/// 设计原界 2/255 只在中高调成立；近黑强细节处 8 位 HF 字节的步进
/// 经线性光 ×2 与暗部 EOTF 斜率（≈12.92）放大后可达约 16 LSB，
/// 这是字节网格的物理极限，不是公式错误——自检界按实测放宽，
/// 但仍远小于公式改坏时的误差（去掉 /2 ⇒ 误差直达 1.0 量级）。
const FS_MAX_ERROR: f32 = 20.0 / 255.0;
/// 频率分离重建均值容差（实测修正：全幅渐变严苛图上均值 2.08 LSB，
/// 故门限取 3——门限的职责是抓住公式改坏（均值会到 30+ 量级），
/// 而不是认证不可见；不可见性由中高调 max ≤ 2 认证）。
const FS_MEAN_ERROR: f32 = 3.0 / 255.0;
/// FS / D&B 烘焙内存系数：源图 + 低频 + 高频 ≈ 12 B/px（设计 D1-1）。
const FS_BYTES_PER_PIXEL: u64 = 12;
/// FS 半径合法域（设计 D1-1）。
const FS_RADIUS_RANGE: (f64, f64) = (1.0, 64.0);
/// D&B 曲线中点合法域（设计 D1-2：0.5 = 无效果）。
const DB_MID_RANGE: (f64, f64) = (0.05, 0.95);

/// 给定低频线性值与候选高频字节，走一遍 linear_light 重建，
/// 返回重建后的显示编码字节（与引擎合成同式，含输出钳制）。
fn fs_rebuild_byte(low_linear: f32, high_byte: u8) -> u8 {
    let high_linear = srgb_decode_extended(high_byte as f32 / 255.0);
    let out = (low_linear + 2.0 * high_linear - 1.0).clamp(0.0, 1.0);
    (srgb_encode_extended(out).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// FS 查表（P2-14）：三张 64K 字节表，把 `fs_bake` 里两轮逐像素 f32 循环
/// （每像素每通道两次 `powf` 级编解码）换成查表 + 整数运算，输出逐位不变。
///
/// 精确性的根据：每张表的表项都由**同一个**原函数逐项生成（`OnceLock` 只算一次，
/// 与 `color.rs` 的 256 项精确解码表同构）——查表只是省掉重复计算，不引入任何
/// 新算式；循环里剩下的整数比较/取整与原式相同。
/// * HF 最近编码字节：键 `(src_byte, low_byte)` → `nearest`（原 HF 循环的
///   `round(encode((decode(src) − decode(low))/2 + 0.5))`）。
/// * 重建字节：键 `(low_byte, candidate)` → [`fs_rebuild_byte`]（`low_linear` =
///   `decode(low_byte)`，与 HF 循环传的值相同）。
/// * 自检重建字节：键 `(high_byte, low_byte)` → 自检循环的 `rebuilt`——只计
///   全不透明像素（`alpha_s = alpha_b = 1.0`），故重建字节只依赖这两个字节；
///   表项按自检循环原样调用 [`blend_pixel`]（`linear_light`）算出。
fn fs_hf_nearest_table() -> &'static [u8; 65536] {
    static TABLE: std::sync::OnceLock<[u8; 65536]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u8; 65536];
        for src in 0..256usize {
            for low in 0..256usize {
                let input = srgb_decode_extended(src as f32 / 255.0);
                let low_value = srgb_decode_extended(low as f32 / 255.0);
                let hf = (input - low_value) / 2.0 + 0.5;
                table[(src << 8) | low] =
                    (srgb_encode_extended(hf).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        table
    })
}

fn fs_rebuild_table() -> &'static [u8; 65536] {
    static TABLE: std::sync::OnceLock<[u8; 65536]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u8; 65536];
        for low in 0..256usize {
            let low_linear = srgb_decode_extended(low as f32 / 255.0);
            for candidate in 0..256usize {
                table[(low << 8) | candidate] = fs_rebuild_byte(low_linear, candidate as u8);
            }
        }
        table
    })
}

fn fs_check_rebuild_table() -> &'static [u8; 65536] {
    static TABLE: std::sync::OnceLock<[u8; 65536]> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0u8; 65536];
        for high in 0..256usize {
            for low in 0..256usize {
                // 与自检循环原样（`alpha_s = alpha_b = 1.0` ⇒ 预乘 = 解码值本身）；
                // 三通道填同一值 ⇒ 任一通道的 `out` 都相同，取 `out[0]` 即可。
                let decoded_high = srgb_decode_extended(high as f32 / 255.0);
                let decoded_low = srgb_decode_extended(low as f32 / 255.0);
                let source_premul = [decoded_high, decoded_high, decoded_high, 1.0];
                let backdrop_premul = [decoded_low, decoded_low, decoded_low, 1.0];
                let out = blend_pixel(BlendMode::LinearLight, source_premul, backdrop_premul);
                let straight = if out[3] > 0.0 { out[0] / out[3] } else { 0.0 };
                table[(high << 8) | low] =
                    (srgb_encode_extended(straight).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
        table
    })
}

/// 频率分离烘焙（纯函数，可单测）：源显示编码 RGBA8 ⇒（低频，高频，
/// 重建最大误差，重建均值误差，中高调最大误差，不透明像素占比）。
///
/// 低频 = 显示编码空间高斯模糊；高频载荷在**线性空间**取
/// HF_lin = (I_lin − LF_lin)/2 + 0.5 再编码回 u8（见模块卷首）。
///
/// **字节选取：最近重建，而非最近编码**（实测修正）：HF 字节按
/// `round(encode(target))` 取时，暗部像素的重建误差会被 EOTF
/// 陡峭段放大——HF 在中灰处 1 字节步进 ≈ 0.005 线性值，经线性光
/// ×2 再经暗部斜率 ≈12.92 编码后可达 16 LSB（合成图实测：源 0、
/// LF 42–140 的条纹像素重建为 15–16）。而钳制平台的存在使邻字节
/// 常常能精确重建（上例中 185 字节 ⇒ 输出钳制到 0 ⇒ 精确）。
/// 所以：先取最近编码字节，若它重建误差 > 2 LSB，则在 ±2 窗内
/// 按重建误差最小重选（窗内含原字节 ⇒ 逐像素永不比最近编码差）。
/// 即使如此，字节网格的物理极限仍在：近黑强细节处仍可能有数
/// LSB 残差——自检界与响应字段如实反映这一点，不虚报 2 LSB。
///
/// 自检用引擎自己的 [`blend_pixel`]（linear_light）把烘焙字节重新
/// 合成一遍，与源字节逐像素比对；超容差 ⇒ 直接 Err（不许静默
/// 落一个重建不回来的组）。
/// 误差统计口径：只计**全不透明邻域**像素（源 alpha 255 且模糊后
/// alpha 255）——半透明邻域的组 alpha 本就无法精确保持，计入只会
/// 制造假失败；占比随结果回带，调用方一眼可见统计覆盖了多少。
/// 零合格像素 ⇒ PreconditionFailed（见自检段）。
///
/// `fs_bake` 的输出：低频/高频烘焙字节 + 重建误差统计 + 分段耗时（P2-14）。
#[derive(Debug)]
pub(super) struct FsBakeOutput {
    /// 低频层字节（显示编码 RGBA8）。
    pub low: Vec<u8>,
    /// 高频层字节（显示编码 RGBA8；中性灰是 188，不是 128）。
    pub high: Vec<u8>,
    /// 整体最大重建误差（0–1 归一化）。
    pub max_error: f32,
    /// 整体平均重建误差。
    pub mean_error: f32,
    /// 中高调（源字节 ≥ 128）最大重建误差。
    pub bright_max_error: f32,
    /// 被统计的全不透明邻域像素占比。
    pub opaque_ratio: f32,
    /// 低频模糊耗时（毫秒）。
    pub blur_ms: f64,
    /// 高频载荷循环耗时（毫秒）。
    pub hf_ms: f64,
    /// 自检循环耗时（毫秒）。
    pub check_ms: f64,
}

pub(super) fn fs_bake(source: &[u8], width: u32, height: u32, sigma: f32) -> Result<FsBakeOutput> {
    let count = (width as usize).saturating_mul(height as usize);
    if source.len() != count * 4 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "频率分离输入长度 {} 不是 {width}×{height}×4",
                source.len()
            )),
        ));
    }
    let blur_start = std::time::Instant::now();
    let low = gaussian_blur_rgba8(source, width, height, sigma);
    let blur_ms = blur_start.elapsed().as_secs_f64() * 1000.0;
    // 查表在循环外取一次（`OnceLock` 的原子检查只付一次；与 `color.rs` 的
    // `srgb_decode_table` 同一动机）。
    let nearest_table = fs_hf_nearest_table();
    let rebuild_table = fs_rebuild_table();
    let hf_start = std::time::Instant::now();
    let mut high = vec![0u8; count * 4];
    for index in 0..count {
        let base = index * 4;
        for channel in 0..3 {
            let src_byte = source[base + channel];
            let low_byte = low[base + channel];
            let nearest = nearest_table[((src_byte as usize) << 8) | low_byte as usize];
            let nearest_error = (rebuild_table[((low_byte as usize) << 8) | nearest as usize]
                as i32
                - src_byte as i32)
                .unsigned_abs();
            if nearest_error <= 2 {
                high[base + channel] = nearest;
            } else {
                // 暗部/强对比处才搜：窗内按重建误差最小重选。
                let mut best_byte = nearest;
                let mut best_error = nearest_error;
                for candidate in nearest.saturating_sub(2)..=nearest.saturating_add(2) {
                    if candidate == nearest {
                        continue;
                    }
                    let error = (rebuild_table[((low_byte as usize) << 8) | candidate as usize]
                        as i32
                        - src_byte as i32)
                        .unsigned_abs();
                    if error < best_error
                        || (error == best_error
                            && (candidate as i32 - nearest as i32).abs()
                                < (best_byte as i32 - nearest as i32).abs())
                    {
                        best_byte = candidate;
                        best_error = error;
                    }
                }
                high[base + channel] = best_byte;
            }
        }
        // 高频层 alpha = 源 alpha（低频层 alpha = 模糊后 alpha；组 alpha
        // 合成规则见模块卷首的局限说明）。
        high[base + 3] = source[base + 3];
    }
    let hf_ms = hf_start.elapsed().as_secs_f64() * 1000.0;
    // 自检：用引擎合成函数把烘焙字节重新合成，与源字节比对。
    // 口径（实测修正）：只计「全不透明邻域」像素——源 alpha = 255 且
    // 模糊后 alpha = 255。半透明邻域的组 alpha 本就无法精确保持
    // （out_alpha = αs + αb(1−αs) ≠ 源 alpha），把那里的像素计入
    // 只会让工具在任何带羽化边缘的图层上都自检失败（实测：透明
    // 边界处误差可达 0.51）。零合格像素 ⇒ 前置失败（不是自检失败）。
    let check_table = fs_check_rebuild_table();
    let check_start = std::time::Instant::now();
    let mut max_error = 0.0f32;
    let mut bright_max_error = 0.0f32;
    let mut sum_error = 0.0f32;
    let mut counted = 0usize;
    for index in 0..count {
        let base = index * 4;
        if source[base + 3] != 255 || low[base + 3] != 255 {
            continue;
        }
        for channel in 0..3 {
            // 只计全不透明像素 ⇒ `alpha_s = alpha_b = 1.0`，重建字节只依赖
            // `(high 字节，low 字节)`，查表（表项即原式，见 `fs_check_rebuild_table`）。
            let rebuilt =
                check_table[((high[base + channel] as usize) << 8) | low[base + channel] as usize];
            let error =
                (rebuilt as i32 - source[base + channel] as i32).unsigned_abs() as f32 / 255.0;
            max_error = max_error.max(error);
            // 中高调界 128：残差 ≈ HF 字节步进（≈0.0054 线性值）×
            // 暗部 EOTF 斜率；≥128 时斜率 ≤1.12 ⇒ 残差 ≤2 LSB（实测）。
            if source[base + channel] >= 128 {
                bright_max_error = bright_max_error.max(error);
            }
            sum_error += error;
            counted += 1;
        }
    }
    let check_ms = check_start.elapsed().as_secs_f64() * 1000.0;
    if counted == 0 {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "图层没有全不透明像素（均匀半透明或全透明邻域）⇒ 频率分离无法重建，拒绝落组",
            ),
        ));
    }
    let mean_error = sum_error / counted as f32;
    let opaque_ratio = if count > 0 {
        counted as f32 / (count * 3) as f32
    } else {
        0.0
    };
    if max_error > FS_MAX_ERROR || mean_error > FS_MEAN_ERROR {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "频率分离自检失败：重建最大误差 {max_error:.5}（限 {FS_MAX_ERROR:.5}）、\
                 均值误差 {mean_error:.5}（限 {FS_MEAN_ERROR:.5}）⇒ 烘焙没有重建回源图，\
                 拒绝落组（不留半套层）"
            )),
        ));
    }
    Ok(FsBakeOutput {
        low,
        high,
        max_error,
        mean_error,
        bright_max_error,
        opaque_ratio,
        blur_ms,
        hf_ms,
        check_ms,
    })
}

/// 显示空间曲线中点 ⇒ 内核线性空间曲线点（D1-2）。
///
/// 内核 curves 点作用在线性缓冲上（预设数据里的 EOTF 转换后小值
/// 即证），所以 x 与 y **都要**过 EOTF 解码：只转 y 会把中点从显示
/// 0.5（线性 0.214）挪到线性 0.5（显示 0.735），中调位置整个错位。
/// 端点 (0,0)/(1,1) 是 EOTF 不动点，原样保留。
fn db_curve_points(mid_display: f64) -> Value {
    let x = srgb_decode_extended(0.5);
    let y = srgb_decode_extended(mid_display as f32);
    json!([[0.0, 0.0], [x, y], [1.0, 1.0]])
}

/// 目标层存在且存活，否则按名报错（FS / D&B 共用）。
fn require_alive_layer(state: &yanshi_core::DocumentState, layer_id: &str) -> Result<()> {
    match state.layers.get(layer_id) {
        None => Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id} 不存在")),
        )),
        Some(layer) if !state.layer_alive(layer_id) => Err(YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!("图层 {layer_id}（「{}」）已删除", layer.name)),
        )),
        _ => Ok(()),
    }
}

/// 组名：显式 name 优先，否则用源图层名（D&B 幂等键与 FS 组名共用此规则）。
fn group_base_name(state: &yanshi_core::DocumentState, args: &Value, layer_id: &str) -> String {
    optional_str(args, "name")
        .or_else(|| state.layers.get(layer_id).map(|layer| layer.name.clone()))
        .unwrap_or_else(|| "未命名".to_owned())
}

/// 新图层原子（CreateLayer；不给 z_index ⇒ fold 自动取存活最大 +1，
/// 组内按原子顺序自下而上排布）。
/// 期 4 D2-1 起供预设预览的临时文档共用（pub(super) ⇒ tools 树内可见）。
pub(super) fn create_layer_atom(
    ctx: &ToolContext<'_>,
    layer_id: &str,
    name: &str,
    layer_type: Option<&str>,
    parent_id: Option<&str>,
    blend_mode: Option<&str>,
) -> Atom {
    let mut payload = json!({"layer_id": layer_id, "name": name});
    if let Some(kind) = layer_type {
        payload["type"] = json!(kind);
    }
    if let Some(parent) = parent_id {
        payload["parent_id"] = json!(parent);
    }
    if let Some(mode) = blend_mode {
        payload["blend_mode"] = json!(mode);
    }
    Atom::new(
        AtomKind::CreateLayer,
        ctx.actor.clone(),
        ctx.session.clone(),
        payload,
    )
}

/// 位图对象原子（CreateObject + raster_patch；与 import_image 同形，
/// 但这里直接落原子以便与组同属一个 changeset）。
/// 期 4 D2-1 起供预设预览的临时文档共用（pub(super) ⇒ tools 树内可见）。
pub(super) fn raster_patch_atom(
    ctx: &ToolContext<'_>,
    object_id: &str,
    layer_id: &str,
    blob_hash: &str,
    size: usize,
    width: u32,
    height: u32,
) -> Atom {
    Atom::new(
        AtomKind::CreateObject,
        ctx.actor.clone(),
        ctx.session.clone(),
        json!({
            "object_id": object_id,
            "layer_id": layer_id,
            "type": "raster_patch",
            "bitmap": {
                "blob_hash": blob_hash,
                "size": size,
                "mime_type": "image/x-yanshi-raw",
            },
            "region": {"x": 0.0, "y": 0.0, "w": width as f64, "h": height as f64},
            "width": width,
            "height": height,
        }),
    )
}

/// 频率分离：把源图层烘焙成低频 / 高频两层 + 两层绘画空层，装进一个组。
///
/// 参数：`layer_id`（必填）、`radius`（缺省 8.0，域 [1, 64]，即高斯
/// sigma）、`name`（缺省源图层名）。原图层保留不动（隐藏与否由
/// 调用方决定，工具不擅自改）。
/// 返回：组与低/高层 id、半径、重建误差（最大/均值/统计占比）与烘焙
/// 快照说明。低频层在下、高频层 blend_mode=linear_light 在上；两层
/// 绘画空层分别供修低频色调与高频细节。
pub(super) fn write_frequency_separation(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let radius = args.get("radius").and_then(Value::as_f64).unwrap_or(8.0);
    if !radius.is_finite() || !(FS_RADIUS_RANGE.0..=FS_RADIUS_RANGE.1).contains(&radius) {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "radius 必须在 [{}，{}] 内（高斯 sigma），得到 {radius}",
                FS_RADIUS_RANGE.0, FS_RADIUS_RANGE.1
            )),
        ));
    }
    let state = document_state(ctx)?;
    require_alive_layer(&state, &layer_id)?;
    let (canvas_w, canvas_h) = (state.width, state.height);
    let base_name = group_base_name(&state, args, &layer_id);
    // 对象级空层检查（渲染级检查不够：空层单层渲染会落到文档背景
    // ——缺省不透明白底——于是“空层”会伪装成一张纯色图通过自检；
    // 实测：空层 FS 曾返回全 0 误差通过。对象级才是真空）。
    let alive = state.alive_objects();
    let layer_objects: Vec<_> = alive
        .iter()
        .filter(|object| object.layer_id == layer_id)
        .collect();
    if layer_objects.is_empty() {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("图层 {layer_id} 没有任何对象 ⇒ 没有可分离的像素")),
        ));
    }
    if layer_objects.iter().all(|object| {
        matches!(
            object.object_type,
            ObjectType::Adjustment | ObjectType::Filter
        )
    }) {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "图层 {layer_id} 只有调整/滤镜对象，没有可分离的像素"
            )),
        ));
    }
    // 内存闸（C2 口径）：源图 + 低频 + 高频 ≈ 12 B/px，超预算 ⇒
    // ResourceExhausted 并给两个数字，不等 OOM killer 动手。
    let pixels = (canvas_w as u64).saturating_mul(canvas_h as u64);
    let estimated = pixels.saturating_mul(FS_BYTES_PER_PIXEL);
    let budget = Workspace::memory_budget_bytes();
    if estimated > budget {
        return Err(YanshiError::new(
            ErrorCode::ResourceExhausted,
            ErrorContext::detail(format!(
                "频率分离估算内存 {} MB 超过文档内存预算 {} MB（{}×{} 像素 × {} B/px）⇒ \
                 拒绝烘焙（调大 YANSHI_DOC_MEMORY_BUDGET_MB 或先缩小画布）",
                estimated / 1_024 / 1_024,
                budget / 1_024 / 1_024,
                canvas_w,
                canvas_h,
                FS_BYTES_PER_PIXEL
            )),
        ));
    }
    // 整幅源像素（只拍这一层；空层/纯调整层 ⇒ 渲染侧 ReferenceNotFound
    // 或全透明 ⇒ 一律转 PreconditionFailed 并说清缺什么）。
    let bbox = Bbox::new(0.0, 0.0, canvas_w as f64, canvas_h as f64);
    let render_start = std::time::Instant::now();
    let (width, height, source) =
        match ctx
            .workspace
            .render_region_raw_layer(&ctx.doc_id, bbox, &layer_id)
        {
            Ok(rendered) => rendered,
            Err(error) if error.code == ErrorCode::ReferenceNotFound => {
                return Err(YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(format!(
                        "图层 {layer_id} 渲染为空 ⇒ 它是空层或纯调整层，没有可分离的像素：{error}"
                    )),
                ));
            }
            Err(error) => return Err(error),
        };
    let render_ms = render_start.elapsed().as_secs_f64() * 1000.0;
    if source.chunks_exact(4).all(|pixel| pixel[3] == 0) {
        return Err(YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!(
                "图层 {layer_id} 全幅透明 ⇒ 没有可分离的像素（空层或纯调整层）"
            )),
        ));
    }
    // 烘焙（含自检；自检失败 ⇒ Err，零原子落账）。
    let baked = fs_bake(&source, width, height, radius as f32)?;
    let FsBakeOutput {
        low,
        high,
        max_error,
        mean_error,
        bright_max_error,
        opaque_ratio,
        blur_ms,
        hf_ms,
        check_ms,
    } = baked;
    // blob 先落（原子引用它们；放 blob 失败 ⇒ 同样零原子）。
    let low_hash = ctx.workspace.put_blob(&low)?;
    let high_hash = ctx.workspace.put_blob(&high)?;
    let group_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let low_layer_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let low_paint_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let high_layer_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let high_paint_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let low_object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
    let high_object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
    let group_name = format!("「{base_name} · 频率分离」");
    let atoms = vec![
        create_layer_atom(ctx, &group_id, &group_name, Some("layer_group"), None, None),
        create_layer_atom(
            ctx,
            &low_layer_id,
            &format!("{base_name} · 低频"),
            None,
            Some(&group_id),
            None,
        ),
        raster_patch_atom(
            ctx,
            &low_object_id,
            &low_layer_id,
            &low_hash,
            low.len(),
            width,
            height,
        ),
        create_layer_atom(
            ctx,
            &low_paint_id,
            &format!("{base_name} · 低频绘画"),
            None,
            Some(&group_id),
            None,
        ),
        create_layer_atom(
            ctx,
            &high_layer_id,
            &format!("{base_name} · 高频"),
            None,
            Some(&group_id),
            Some("linear_light"),
        ),
        raster_patch_atom(
            ctx,
            &high_object_id,
            &high_layer_id,
            &high_hash,
            high.len(),
            width,
            height,
        ),
        create_layer_atom(
            ctx,
            &high_paint_id,
            &format!("{base_name} · 高频绘画"),
            None,
            Some(&group_id),
            None,
        ),
    ];
    let changeset = Changeset::new_id();
    ctx.commit_changeset(atoms, changeset)?;
    Ok(json!({
        "ok": true,
        "group_id": group_id,
        "low_layer_id": low_layer_id,
        "high_layer_id": high_layer_id,
        "radius": radius,
        "reconstruction_max_error": max_error,
        "reconstruction_mean_error": mean_error,
        "reconstruction_max_error_bright": bright_max_error,
        "reconstruction_pixel_ratio": opaque_ratio,
        "fs_timings": {
            "render_ms": render_ms,
            "blur_ms": blur_ms,
            "hf_ms": hf_ms,
            "check_ms": check_ms,
        },
        "note": "低频/高频是导入时刻的像素快照（烘焙）：源图层此后的修改不传播，与 raster patch 同类；半透明像素的组 alpha 不精确保持源 alpha，频率分离按不透明照片层设计。高频层的中性灰是 188（线性 0.5 的 sRGB 编码），不是 PS 式的 128——在高频层上涂抹请用 188 作中性色，用 128 会压暗。重建刻画（实测）：中高调（源字节 ≥ 128）≤ 2 LSB；暗部受 8 位 HF 字节网格限制，残差随暗度增大，近黑强细节处可达约 16 LSB，均值 ≤ 3 LSB；自检统计只计全不透明邻域像素，占比见 reconstruction_pixel_ratio。",
    }))
}

/// 减淡加深建组：curves 变体建两个带全黑蒙版的曲线层；gray 变体建
/// 一个 50% 灰 soft_light 层。重复调用同名组 ⇒ already_exists，不叠第二套。
pub(super) fn write_dodge_burn_setup(ctx: &mut ToolContext<'_>, args: &Value) -> Result<Value> {
    let layer_id = require_str(args, "layer_id")?;
    let mode = optional_str(args, "mode").unwrap_or_else(|| "curves".to_owned());
    if !matches!(mode.as_str(), "curves" | "gray") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("mode 只收 curves / gray，得到 {mode}")),
        ));
    }
    let dodge_mid = args
        .get("dodge_mid")
        .and_then(Value::as_f64)
        .unwrap_or(0.62);
    let burn_mid = args.get("burn_mid").and_then(Value::as_f64).unwrap_or(0.38);
    for (key, value) in [("dodge_mid", dodge_mid), ("burn_mid", burn_mid)] {
        if !value.is_finite() || !(DB_MID_RANGE.0..=DB_MID_RANGE.1).contains(&value) {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "{key} 必须在 [{}，{}] 内（0.5 = 无效果），得到 {value}",
                    DB_MID_RANGE.0, DB_MID_RANGE.1
                )),
            ));
        }
    }
    let state = document_state(ctx)?;
    require_alive_layer(&state, &layer_id)?;
    let (canvas_w, canvas_h) = (state.width, state.height);
    let base_name = group_base_name(&state, args, &layer_id);
    let group_name = format!("「{base_name} · 减淡加深」");
    // 幂等：同名存活组已存在 ⇒ 返回既有 id，不叠第二套（沿
    // apply_preset「更新而非叠加」家法；同名是唯一键，见工具说明）。
    if let Some(group_id) = state
        .layers
        .iter()
        .find(|(_, layer)| layer.name == group_name && state.layer_alive(&layer.id))
        .map(|(id, _)| id)
    {
        let mut layer_ids: Vec<String> = state
            .layers
            .values()
            .filter(|layer| layer.parent_id.as_deref() == Some(group_id.as_str()))
            .filter(|layer| state.layer_alive(&layer.id))
            .map(|layer| layer.id.clone())
            .collect();
        layer_ids.sort();
        let object_ids: Vec<String> = state
            .alive_objects()
            .iter()
            .filter(|object| layer_ids.contains(&object.layer_id))
            .map(|object| object.id.clone())
            .collect();
        let mask_ids: Vec<String> = state
            .layers
            .values()
            .filter(|layer| layer_ids.contains(&layer.id))
            .filter_map(|layer| layer.mask_id.clone())
            .collect();
        // **P2-10**（期 2 块 2）：`mask_ids` 只有数组、无角色——调用方只能按
        //   `[dodge, burn]` 顺序猜 ✗。⇒ 回执加 `masks: {dodge, burn}`，按**图层名**认角色
        //   （复用路径上数组顺序不可靠——这里是按 id 排过的集合，不能反推角色 ✗）；
        //   gray 组没有蒙版 ⇒ 两格都是 null。数组本身不动（兼容）。
        let mask_of = |role: &str| -> Value {
            state
                .layers
                .values()
                .filter(|layer| layer_ids.contains(&layer.id))
                .find(|layer| layer.name == role)
                .and_then(|layer| layer.mask_id.clone())
                .map(Value::String)
                .unwrap_or(Value::Null)
        };
        return Ok(json!({
            "ok": true,
            "already_exists": true,
            "group_id": group_id,
            "layer_ids": layer_ids,
            "object_ids": object_ids,
            "mask_ids": mask_ids,
            "masks": {"dodge": mask_of("减淡 · Dodge"), "burn": mask_of("加深 · Burn")},
            "note": "同名减淡加深组已存在 ⇒ 复用既有组，没有新建（删掉旧组后可重建）。",
        }));
    }
    let group_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
    let mut atoms = vec![create_layer_atom(
        ctx,
        &group_id,
        &group_name,
        Some("layer_group"),
        None,
        None,
    )];
    let (layer_ids, object_ids, mask_ids, note) = match mode.as_str() {
        "gray" => {
            // 50% 灰烘焙层（fill_region 在本仓同样落位图快照，见工具说明）+
            // soft_light 混合：灰层本身不改变画面，黑/白涂抹即减淡/加深。
            let gray: Vec<u8> = (0..(canvas_w as usize) * (canvas_h as usize))
                .flat_map(|_| [128u8, 128u8, 128u8, 255u8])
                .collect();
            let gray_hash = ctx.workspace.put_blob(&gray)?;
            let gray_layer_id = format!("layer_{}", yanshi_core::Ulid::new().encode());
            let gray_object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
            atoms.push(create_layer_atom(
                ctx,
                &gray_layer_id,
                &format!("{base_name} · 灰层"),
                None,
                Some(&group_id),
                Some("soft_light"),
            ));
            atoms.push(raster_patch_atom(
                ctx,
                &gray_object_id,
                &gray_layer_id,
                &gray_hash,
                gray.len(),
                canvas_w,
                canvas_h,
            ));
            (
                vec![gray_layer_id],
                vec![gray_object_id],
                Vec::new(),
                "灰层法：图层本身是 50% 灰烘焙快照（soft_light 下不改变画面）；用黑/白低流量笔触涂抹即加深/减淡。单层直观，代价是烘焙——灰层不跟随下方修改。",
            )
        }
        _ => {
            // curves 变体：两层各挂一条曲线 + 一张全黑蒙版（画布 rect +
            // invert ⇒ 覆盖率处处为 0，生效区用 paint_mask 以白色涂出）。
            let mut layer_ids = Vec::new();
            let mut object_ids = Vec::new();
            let mut mask_ids = Vec::new();
            for (label, mid) in [("减淡 · Dodge", dodge_mid), ("加深 · Burn", burn_mid)] {
                let layer_id_new = format!("layer_{}", yanshi_core::Ulid::new().encode());
                let object_id = format!("obj_{}", yanshi_core::Ulid::new().encode());
                let mask_id = format!("mask_{}", yanshi_core::Ulid::new().encode());
                atoms.push(create_layer_atom(
                    ctx,
                    &layer_id_new,
                    label,
                    None,
                    Some(&group_id),
                    None,
                ));
                atoms.push(Atom::new(
                    AtomKind::CreateObject,
                    ctx.actor.clone(),
                    ctx.session.clone(),
                    json!({
                        "object_id": object_id,
                        "layer_id": layer_id_new,
                        "type": "adjustment",
                        "z_index": 0,
                        "data": {
                            "adjustment_type": "curves",
                            "params": {
                                "channel": "rgb",
                                "points": db_curve_points(mid),
                            },
                            "opacity": 1.0,
                            "name": label,
                        },
                    }),
                ));
                atoms.push(Atom::new(
                    AtomKind::CreateMask,
                    ctx.actor.clone(),
                    ctx.session.clone(),
                    json!({
                        "mask_id": mask_id,
                        "shape": {
                            "kind": "rect",
                            "bbox": {
                                "x": 0,
                                "y": 0,
                                "w": canvas_w,
                                "h": canvas_h,
                            },
                        },
                        "feather": 0.0,
                        "mode": "new",
                        "invert": true,
                    }),
                ));
                atoms.push(Atom::new(
                    AtomKind::SetProperty,
                    ctx.actor.clone(),
                    ctx.session.clone(),
                    json!({
                        "layer_id": layer_id_new,
                        "key": "mask_id",
                        "value": mask_id,
                    }),
                ));
                layer_ids.push(layer_id_new);
                object_ids.push(object_id);
                mask_ids.push(mask_id);
            }
            (
                layer_ids,
                object_ids,
                mask_ids,
                "曲线法：两层初始全黑蒙版 ⇒ 曲线暂不生效；用 paint_mask 以白色低不透明度笔刷涂抹生效区（只改蒙版，不动曲线）。曲线中点已按 §9.1 EOTF 转线性落盘，工具参数里给的是显示空间值。",
            )
        }
    };
    let changeset = Changeset::new_id();
    ctx.commit_changeset(atoms, changeset)?;
    // **P2-10**（期 2 块 2）：新鲜建组时按建层顺序认角色——curves 循环先减淡后加深 ⇒
    //   `mask_ids[0]` 是 dodge、`[1]` 是 burn；gray 变体没有蒙版 ⇒ 两格 null。
    //   （与复用路径的按名认角色互为印证：同一组的两次调用必须给出同一张映射。）
    let masks = if mode == "curves" {
        json!({
            "dodge": mask_ids.first().cloned(),
            "burn": mask_ids.get(1).cloned(),
        })
    } else {
        json!({"dodge": Value::Null, "burn": Value::Null})
    };
    Ok(json!({
        "ok": true,
        "already_exists": false,
        "group_id": group_id,
        "layer_ids": layer_ids,
        "object_ids": object_ids,
        "mask_ids": mask_ids,
        "masks": masks,
        "mode": mode,
        "note": note,
    }))
}

#[cfg(test)]
mod retouch_tests {
    use super::{
        db_curve_points, fs_bake, fs_check_rebuild_table, fs_hf_nearest_table, fs_rebuild_byte,
        fs_rebuild_table, FsBakeOutput,
    };
    use yanshi_render::{
        blend_pixel,
        color::{srgb_decode_extended, srgb_encode_extended},
        BlendMode,
    };

    /// 合成测试图（64×48，全不透明）：水平渐变 + 高频竖条纹 + 纯色块。
    fn synthetic(width: u32, height: u32) -> Vec<u8> {
        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let base = (y * width as usize + x) * 4;
                let gradient = (x * 255 / width as usize) as u8;
                let stripe = if (x / 2) % 2 == 0 { 255 } else { 0 };
                let (r, g, b) = if x >= 48 && y >= 32 {
                    (200, 30, 30)
                } else if y < 8 {
                    (gradient, stripe, 128)
                } else {
                    (gradient, 255 - gradient, 90)
                };
                pixels[base] = r;
                pixels[base + 1] = g;
                pixels[base + 2] = b;
                pixels[base + 3] = 255;
            }
        }
        pixels
    }

    /// 烘焙自检的容差必须真生效：合成图上整体 max ≤ 20/255、
    /// mean ≤ 3/255、中高调 max ≤ 2/255（实测刻画界，见 fs_bake 说明）；
    /// 纯色块内部高频必须是线性 0.5 的 sRGB 编码（188 ± 1），
    /// 否则「高频层」根本没在分离细节。
    #[test]
    fn fs_bake_reconstructs_within_tolerance() {
        let (width, height) = (64, 48);
        let source = synthetic(width, height);
        let baked = fs_bake(&source, width, height, 8.0).expect("合成图烘焙应当通过自检");
        let FsBakeOutput {
            low,
            high,
            max_error,
            mean_error,
            bright_max_error,
            opaque_ratio: ratio,
            ..
        } = baked;
        // 实测刻画界（见 fs_bake 说明）：严苛合成图上整体最大误差受 8 位
        // 物理极限约束；中高调（源字节 ≥ 128）仍应 ≤ 2 LSB，均值 ≤ 3 LSB。
        assert!(max_error <= 20.0 / 255.0, "最大误差超刻画界：{max_error}");
        assert!(mean_error <= 3.0 / 255.0, "均值误差超刻画界：{mean_error}");
        assert!(
            bright_max_error <= 2.0 / 255.0,
            "中高调最大误差应 ≤ 2 LSB，得到 {bright_max_error}"
        );
        assert_eq!(ratio, 1.0, "全不透明图统计占比应为 1");
        assert_ne!(low, source, "低频必须是模糊过的，不能原样返回源图");
        assert_ne!(high, source, "高频层是细节载荷，不能原样返回源图");
        // 纯色块内部高频必须是“线性 0.5 的 sRGB 编码”（I == LF ⇒
        // HF_lin = 0.5 ⇒ encode(0.5)×255 = 187.6 ⇒ 188 ± 1）——注意不是
        // 128：线性空间烘焙的中性点在字节上是 188（PS 显示空间烘焙
        // 才是 128；两者中性灰不同，见工具说明，误用 128 涂抹会压暗）。
        // 注意：sigma=8 时模糊半径 24，16×16 的块根本没有“内部”
        // （窗内总混进块外像素），故中性点断言改用 sigma=2 的独立烘焙
        // （半径 6，像素 (56,40) 的窗全在块内）。
        let high_small = fs_bake(&source, width, height, 2.0)
            .expect("小半径烘焙应当通过自检")
            .high;
        let base = (40 * width as usize + 56) * 4;
        for channel in 0..3 {
            let value = high_small[base + channel] as i32;
            assert!(
                (187..=189).contains(&value),
                "纯色块内部高频应为线性 0.5 的编码（188 ± 1），通道 {channel} 得到 {value}"
            );
        }
    }

    /// 变异守卫（独立重算）：测试自己用引擎 blend_pixel 把烘焙字节
    /// 重新合成，与源图比对——即使工具内的自检被改成恒过，本条
    /// 仍能抓住烘焙公式被改坏（例如 H 的 /2 被去掉 ⇒ 误差爆）。
    #[test]
    fn fs_bake_recomposites_via_engine_blend() {
        let (width, height) = (64, 48);
        let source = synthetic(width, height);
        let baked = fs_bake(&source, width, height, 8.0).unwrap();
        let FsBakeOutput { low, high, .. } = baked;
        let mut max_error = 0.0f32;
        let mut sum_error = 0.0f32;
        let mut counted = 0usize;
        for index in 0..(width as usize) * (height as usize) {
            let base = index * 4;
            let mut s = [0.0f32; 4];
            let mut b = [0.0f32; 4];
            for channel in 0..3 {
                s[channel] = srgb_decode_extended(high[base + channel] as f32 / 255.0);
                b[channel] = srgb_decode_extended(low[base + channel] as f32 / 255.0);
            }
            s[3] = 1.0;
            b[3] = 1.0;
            let out = blend_pixel(BlendMode::LinearLight, s, b);
            for channel in 0..3 {
                let rebuilt =
                    (srgb_encode_extended(out[channel]).clamp(0.0, 1.0) * 255.0).round() as u8;
                let error =
                    (rebuilt as i32 - source[base + channel] as i32).unsigned_abs() as f32 / 255.0;
                max_error = max_error.max(error);
                sum_error += error;
                counted += 1;
            }
        }
        assert!(max_error <= 20.0 / 255.0, "独立重算最大误差：{max_error}");
        assert!(
            sum_error / counted as f32 <= 3.0 / 255.0,
            "独立重算均值误差：{}",
            sum_error / counted as f32
        );
    }

    /// 透明像素不计入误差统计（其 straight RGB 无定义），且不导致假失败。
    #[test]
    fn fs_bake_excludes_transparent_pixels() {
        let (width, height) = (32, 32);
        let mut source = synthetic(width, height);
        for pixel in source.chunks_exact_mut(4).take((width as usize) * 8) {
            pixel[3] = 0;
        }
        let ratio = fs_bake(&source, width, height, 4.0)
            .expect("含透明区的图也不应自检失败")
            .opaque_ratio;
        assert!(ratio < 1.0, "透明区被排除后占比应小于 1，得到 {ratio}");
        assert!(ratio > 0.0, "仍有不透明像素被统计");
    }

    /// 输入长度不对 ⇒ 响亮拒绝，不静默截断。
    #[test]
    fn fs_bake_rejects_bad_length() {
        let error = fs_bake(&[0u8; 10], 4, 4, 2.0).unwrap_err();
        assert!(
            format!("{error}").contains("不是 4×4×4"),
            "错误要说清长度：{error}"
        );
    }

    /// D&B 曲线中点：x 与 y 都要过 EOTF（只转 y ⇒ x 仍是 0.5 ⇒ 红）。
    #[test]
    fn db_curve_midpoint_is_eotf_converted_both_axes() {
        let points = db_curve_points(0.62);
        let mid = &points[1];
        let x = mid[0].as_f64().unwrap();
        let y = mid[1].as_f64().unwrap();
        let expected_x = srgb_decode_extended(0.5) as f64;
        let expected_y = srgb_decode_extended(0.62) as f64;
        assert!(
            (x - expected_x).abs() < 1e-4,
            "中点 x 应为 decode(0.5)={expected_x}，得到 {x}"
        );
        assert!(
            (y - expected_y).abs() < 1e-4,
            "中点 y 应为 decode(0.62)={expected_y}，得到 {y}"
        );
        assert_ne!(x, 0.5, "x 仍是显示 0.5 ⇒ EOTF 转换被漏掉");
        assert_ne!(y, 0.62, "y 仍是显示 0.62 ⇒ EOTF 转换被漏掉");
    }

    /// P2-14：三张 FS 查表与原函数逐项相同（全 65536 项，不抽检）。
    ///
    /// 表项既然就是原函数的值，查表只是省掉重复计算；本条把"逐项相同"钉死：
    /// 任一表项算错（索引错位、公式抄错）⇒ 本条红。`fs_bake` 的端到端测试
    /// （误差界 + 独立重算）是第二道网。
    #[test]
    fn fs_lookup_tables_match_original_functions() {
        let nearest_table = fs_hf_nearest_table();
        let rebuild_table = fs_rebuild_table();
        let check_table = fs_check_rebuild_table();
        for src in 0..256usize {
            for low in 0..256usize {
                let src_byte = src as u8;
                let low_byte = low as u8;
                let input = srgb_decode_extended(src_byte as f32 / 255.0);
                let low_value = srgb_decode_extended(low_byte as f32 / 255.0);
                let hf = (input - low_value) / 2.0 + 0.5;
                let expected = (srgb_encode_extended(hf).clamp(0.0, 1.0) * 255.0).round() as u8;
                assert_eq!(
                    nearest_table[(src << 8) | low],
                    expected,
                    "HF 最近字节表在 (src={src_byte}, low={low_byte}) 与原式不符"
                );
                let expected_rebuild = fs_rebuild_byte(low_value, src_byte);
                assert_eq!(
                    rebuild_table[(low << 8) | src],
                    expected_rebuild,
                    "重建表在 (low={low_byte}, candidate={src_byte}) 与原函数不符"
                );
                let decoded_high = srgb_decode_extended(src_byte as f32 / 255.0);
                let decoded_low = srgb_decode_extended(low_byte as f32 / 255.0);
                let source_premul = [decoded_high, decoded_high, decoded_high, 1.0];
                let backdrop_premul = [decoded_low, decoded_low, decoded_low, 1.0];
                let out = blend_pixel(BlendMode::LinearLight, source_premul, backdrop_premul);
                let straight = if out[3] > 0.0 { out[0] / out[3] } else { 0.0 };
                let expected_check =
                    (srgb_encode_extended(straight).clamp(0.0, 1.0) * 255.0).round() as u8;
                assert_eq!(
                    check_table[(src << 8) | low],
                    expected_check,
                    "自检重建表在 (high={src_byte}, low={low_byte}) 与原式不符"
                );
            }
        }
    }
}
