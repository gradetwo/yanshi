//! **纹理填充**（`fill_region(texture=…)`）—— 外部第 71 次报告要的那个能力。
//!
//! **为什么这几条断言能红**（每条都对准一类"看起来对、其实错了"）：
//! ① 源位图**左半红、右半蓝** ⇒ 填完之后目标区域**必须同时含红与蓝**
//!    ⇒ 若取样退化成"全图平均"，整片只剩一个混合色 ⇒ 这条立刻红
//!    （那正是对方 `E1761` 踩过的坑："平均会让深处看不出纹理" ✓）；
//! ② `texture.type=region`（从图层形状取）**尚未实现** ⇒ 必须**报错并点名** region
//!    （静默当"没有纹理"是本仓头号病症 ✓）；
//! ③ 源矩形离目标太近且 `min_distance` 要求更远 ⇒ 必须**报错并点名** `min_distance`
//!    （他的 `test1` 红褐竖痕就是采到了洞边缘的过渡色 ✓）；
//! ④ 响应必须**回显** `source_gap_px`（传了纹理却不说它生效 = 撒谎 ✓）。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_fill_texture", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

/// 8×8 原始 RGBA：**左半红、右半蓝**（`image/x-yanshi-raw` ⇒ 与 `raster_patch` 同一条解码路 ✓）。
fn source_bytes() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 * 8 * 4);
    for _y in 0..8 {
        for x in 0..8 {
            if x < 4 {
                bytes.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                bytes.extend_from_slice(&[0, 0, 255, 255]);
            }
        }
    }
    bytes
}

fn new_document(ctx: &mut ToolContext<'_>) {
    registry().call(
        ctx,
        "new_document",
        &json!({ "doc_id": "doc_fill_texture", "width": 64, "height": 64 }),
    );
}

fn first_layer(ctx: &mut ToolContext<'_>) -> String {
    let layers = registry().call(ctx, "list_layers", &json!({}));
    layers
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| layers.first())
        .and_then(|layer| layer.get("layer_id").or_else(|| layer.get("id")))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("新文档应当有至少一个图层：{layers}"))
        .to_string()
}

/// 调用一个**必须失败**的工具，返回错误文案（**文案要说清原因** ✓ 不是"出错了" ✗）。
///
/// **两条失败路都要认** ✓：工具错误既可能以 `{"ok":false, "context":{"detail":…}}` 返回，
/// 也可能 panic ✓ —— 只认一种就会把"其实成功了"读成"报错了" ✗。
fn expect_error(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> String {
    let response = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry().call(ctx, name, args)
    }));
    match response {
        Ok(value) => {
            assert_eq!(
                value.get("ok").and_then(Value::as_bool),
                Some(false),
                "{name} 本该报错，却成功了 ⇒ 静默降级：{value}"
            );
            value
                .get("context")
                .and_then(|context| context.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string())
        }
        Err(payload) => payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
            })
            .unwrap_or_else(|| "（无法读取错误文案）".to_string()),
    }
}

#[test]
fn textured_fill_takes_each_pixel_from_the_source_instead_of_an_average() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let hash = workspace
        .store()
        .put(&source_bytes())
        .expect("源位图应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }

    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);
    let filled = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
            "color": [0, 0, 0, 255],
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 0, "y": 0, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert_eq!(
        filled.get("ok").and_then(Value::as_bool),
        Some(true),
        "fill_region 没成功：{filled}"
    );
    // ④ 回显：传了纹理就说明它落地了（不说 = 撒谎）。
    let gap = filled
        .get("texture")
        .and_then(|texture| texture.get("source_gap_px"))
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("响应没说纹理生效：{filled}"));
    // 两个矩形在 x 与 y 上各相距 24px ⇒ 欧氏距离 24√2。
    let expected = 24.0_f64.hypot(24.0);
    assert!(
        (gap - expected).abs() < 0.1,
        "源矩形与目标的间距应为 {expected:.2}px，实得 {gap:.2}"
    );

    let samples = registry().call(
        &mut ctx,
        "sample_colors",
        &json!({"points": [[34, 36], [60, 36]]}),
    );
    let samples = samples
        .get("samples")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("sample_colors 形状变了：{samples}"));
    let at = |index: usize| -> [u8; 4] {
        let rgba = samples[index]
            .get("rgba")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("第 {index} 个采样缺 rgba：{:?}", samples[index]));
        let mut out = [0u8; 4];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = value_from(rgba, index);
        }
        out
    };
    // x=34 ⇒ 源里 x=2 ⇒ **红**；x=60 ⇒ 源里 x=4 ⇒ **蓝**（8px 平铺 ✓）。
    let left = at(0);
    let right = at(1);
    assert!(
        left[0] > 200 && left[2] < 60,
        "目标左侧应取到源的**红**半 ⇒ 实测 {left:?}（若是均匀的紫，说明取样退化成了平均 ✗）"
    );
    assert!(
        right[2] > 200 && right[0] < 60,
        "目标右侧应取到源的**蓝**半 ⇒ 实测 {right:?}（若是均匀的紫，说明取样退化成了平均 ✗）"
    );
}

fn value_from(rgba: &[Value], index: usize) -> u8 {
    rgba.get(index)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("rgba 第 {index} 个分量不是数字：{rgba:?}")) as u8
}

#[test]
fn textured_fill_says_why_when_the_source_is_unimplemented_or_too_close() {
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let hash = workspace
        .store()
        .put(&source_bytes())
        .expect("源位图应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }

    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);

    // ② region 未实现 ⇒ 报错**点名** region（静默当"没有纹理"是本仓头号病症）。
    let region = json!({
        "layer_id": layer,
        "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
        "color": [0, 0, 0, 255],
        "texture": {
            "type": "region",
            "source": {"x": 0, "y": 0, "w": 8, "h": 8},
            "width": 8,
            "height": 8
        }
    });
    let message = expect_error(&mut ctx, "fill_region", &region);
    assert!(
        message.contains("region"),
        "错误必须点名 region（缺什么就说缺什么）：{message}"
    );

    // ③ 源离目标太近且 min_distance 要求更远 ⇒ 报错**点名** min_distance。
    let too_close = json!({
        "layer_id": layer,
        "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
        "color": [0, 0, 0, 255],
        "texture": {
            "type": "bitmap",
            "blob_hash": hash,
            "source": {"x": 30, "y": 30, "w": 8, "h": 8},
            "width": 8,
            "height": 8,
            "min_distance": 25
        }
    });
    let message = expect_error(&mut ctx, "fill_region", &too_close);
    assert!(
        message.contains("min_distance"),
        "错误必须点名 min_distance 与实测间距：{message}"
    );

    // ④ **`min_distance_from_target` 是同一个东西的另一种拼写** ✓（外部提案里写的就是它 ✓）：
    // 只收 `min_distance` 会让"照他们文档写"的调用**静默不拦** ✗ ⇒ 两种拼写都要生效 ✓。
    let alias = json!({
        "layer_id": layer,
        "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
        "color": [0, 0, 0, 255],
        "texture": {
            "type": "bitmap",
            "blob_hash": hash,
            "source": {"x": 30, "y": 30, "w": 8, "h": 8},
            "width": 8,
            "height": 8,
            "min_distance_from_target": 25
        }
    });
    let message = expect_error(&mut ctx, "fill_region", &alias);
    assert!(
        message.contains("min_distance"),
        "`min_distance_from_target` 也必须生效（否则照文档写会静默不拦）：{message}"
    );
}

#[test]
fn textured_fill_does_not_require_a_colour() {
    // **传了 texture 就不该再要 `color`** ✓（外部第 12 次实测：「color 仍必填（被 texture 覆盖）」✗
    // ⇒ 那个参数一个像素都用不到，却卡住调用 ✓）。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let hash = workspace
        .store()
        .put(&source_bytes())
        .expect("源位图应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }
    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);
    let filled = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 0, "y": 0, "w": 16, "h": 16},
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 32, "y": 32, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert_eq!(
        filled.get("ok").and_then(Value::as_bool),
        Some(true),
        "给了 texture 时 color 应当可省略：{filled}"
    );
}

#[test]
fn a_png_texture_is_accepted_and_the_effective_mime_is_recorded() {
    // **外部第 74 次实测的那个坑** ✓：拿 PNG 当 `texture.blob_hash`（且不写 mime_type）
    // ⇒ 原来 `fill_region` 回 `ok:true`，**之后**渲染/导出才失败，错误里只有 blob_hash ✗。
    // 现在：字节头是 PNG 签名 ⇒ **自动认出** ✓、把 `image/png` **写进文档** ✓、并**立刻**校验尺寸 ✓。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let png = yanshi_render::png::encode_png(8, 8, &source_bytes()).expect("编码 PNG");
    let hash = workspace
        .store()
        .put(&png)
        .expect("PNG 应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }
    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);
    let filled = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 0, "y": 0, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert_eq!(
        filled.get("ok").and_then(Value::as_bool),
        Some(true),
        "PNG 纹理必须被接受（外部第 74 次：原来这里成功、导出才炸）：{filled}"
    );
    assert_eq!(
        filled
            .get("texture")
            .and_then(|texture| texture.get("mime_type"))
            .and_then(Value::as_str),
        Some("image/png"),
        "生效的 MIME 必须回显成 image/png（自动认出的结果）：{filled}"
    );
    // **渲染侧也必须真能解** ✓：这条正是他们失败的下一步（渲染/导出时才解码）。
    let rendered = registry().call(
        &mut ctx,
        "render_region",
        &json!({"region": [0, 0, 128, 96]}),
    );
    assert_eq!(
        rendered.get("ok").and_then(Value::as_bool),
        Some(true),
        "PNG 纹理的文档必须能渲染（他们的失败点就在这里）：{rendered}"
    );
    // **而且要真的把源图颜色搬过来** ✓（否则"渲染成功"也可能只是形状被跳过 ⇒ 一片白 ✗）。
    let samples = registry().call(
        &mut ctx,
        "sample_colors",
        &json!({"points": [[34, 36], [60, 36]]}),
    );
    let samples = samples
        .get("samples")
        .and_then(Value::as_array)
        .expect("sample_colors 形状变了");
    let channel = |index: usize, slot: usize| -> u64 {
        samples[index]
            .get("rgba")
            .and_then(Value::as_array)
            .and_then(|rgba| rgba.get(slot))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    assert!(
        channel(0, 0) > 200 && channel(0, 2) < 60,
        "PNG 纹理目标左侧应取到源的红半 ⇒ 实测 [{},{},{}]",
        channel(0, 0),
        channel(0, 1),
        channel(0, 2)
    );
    assert!(
        channel(1, 2) > 200 && channel(1, 0) < 60,
        "PNG 纹理目标右侧应取到源的蓝半 ⇒ 实测 [{},{},{}]",
        channel(1, 0),
        channel(1, 1),
        channel(1, 2)
    );
}

#[test]
fn a_truncated_raw_texture_is_rejected_here_with_the_byte_counts() {
    // **早报错** ✓：字节数 ≠ w×h×4 时必须在 `fill_region` 就拒绝，并写出**两个数**与补救办法。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    let truncated = vec![0u8; 100]; // 8×8×4 = 256 ⇒ 差 156 字节
    let hash = workspace
        .store()
        .put(&truncated)
        .expect("字节应能入库")
        .to_string();
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }
    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);
    let message = expect_error(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 0, "y": 0, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert!(
        message.contains("256") && message.contains("100"),
        "错误必须写出「应为 256、实际 100」两个数：{message}"
    );
    assert!(
        message.contains("mime_type"),
        "错误必须告诉用户 PNG 要写 mime_type（否则他只会看到字节数不符）：{message}"
    );
}
