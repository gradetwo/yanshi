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

    // ② **`region` 现在可实现**（第 2680 轮）⇒ 这里的负例换成"**缺 `source`**"：
    //    静默当"没有纹理"仍是本仓头号病症 ✗ ⇒ 缺什么就要点名什么 ✓。
    //    （`region` 的正例见本文件最后一条 `a_region_texture_snapshots_...` ✓）
    let region = json!({
        "layer_id": layer,
        "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
        "color": [0, 0, 0, 255],
        "texture": {"type": "region"}
    });
    let message = expect_error(&mut ctx, "fill_region", &region);
    assert!(
        message.contains("region") && message.contains("source"),
        "缺 `source` 时必须同时点名 region 与 source（缺什么就说缺什么）：{message}"
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

#[test]
fn a_path_shaped_texture_fill_only_covers_the_path_not_its_bbox() {
    // **外部第 75 次的新发现** ✓：用 `rect` 填洞会把**洞外的像素**也刷掉
    // （他们实测 ①=29.77 ⇒ 天空/草地被外包矩形盖掉 ✓）⇒ 要只填洞内必须用 **`path`** 形状 ✓。
    // 本测试就是他们"下一步"要打的那一调：`fill_region(shape=path, texture=bitmap)` ✓，
    // 并且**同时**验证"只盖 path、不盖它的外包矩形"（否则 rect 也过 ✓ ⇒ 又变成那个坑 ✗）。
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
    // 三角形 (16,16)-(56,16)-(36,56)：质心 ≈ (36,29) 在形内；
    // **(52,48)** 在**外接盒内（16..56）但形外**（该 y 上三角形只占 x≈32..40）⇒ 这一条才有鉴别力✓。
    // ⇒ 注意：**别取太靠边的点** —— `sample_colors` 会把"落在渲染区之外"的点标成
    //   `outside:true` 且 `rgba:null` ✓（我第一次取 (70,70) 就是这样 ✗）。
    // ⇒ 我第一版取的 **(34,34)** 其实**在三角形里**（它离顶点 (32,32) 很近）✗ ⇒ 断言写错了 ✓
    //   （与第 904 轮那次「采样点落在矩形里」是同一类错 ✓ —— 采样点要**先算边界** ✓）。
    // ⚠️ 文档是 **64×64**（本文件的 `new_document` 助手 ✓）⇒ **形状与采样点都必须落在里面** ✗
    //   我第一版用了 (32,32)-(72,32)-(52,72) ＋ 采样 (68,55) ⇒ **点出了画布** ⇒
    //   `sample_colors` 回 `outside:true`、`rgba:null` ✗（**又是"采样点先算边界"** ✓ —— 这次边界是**画布** ✓）。
    let triangle = json!({
        "kind": "path",
        "subpaths": [{
            "start": [16.0, 16.0],
            "closed": true,
            "segments": [{"op": "L", "to": [56.0, 16.0]}, {"op": "L", "to": [36.0, 56.0]}]
        }]
    });
    let filled = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": triangle,
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
        "path 形状的纹理填充必须被接受（外部 B8 之后已支持 path）：{filled}"
    );
    let samples = registry().call(
        &mut ctx,
        "sample_colors",
        &json!({"points": [[36, 29], [52, 48]]}),
    );
    let samples = samples
        .get("samples")
        .and_then(Value::as_array)
        .expect("sample_colors 形状变了");
    let rgba = |index: usize| -> [u64; 4] {
        let values = samples[index]
            .get("rgba")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("缺 rgba：samples={samples:?}"));
        let mut out = [0u64; 4];
        for (index, slot) in out.iter_mut().enumerate() {
            *slot = values.get(index).and_then(Value::as_u64).unwrap_or(0);
        }
        out
    };
    // ① **形内**必须吃到源纹理（红或蓝 ⇒ 只要不是白底就行）。
    let inside = rgba(0);
    assert!(
        inside[0] > 200 || inside[2] > 200,
        "三角形内部必须填上源纹理（红/蓝）⇒ 实测 {inside:?}"
    );
    // ② **外接盒里、形外**的那一点必须**仍是白底** ⇒ 否则就是又用了 rect（他们踩的那个坑 ✗）。
    let corner = rgba(1);
    assert_eq!(
        corner,
        [255, 255, 255, 255],
        "path 填充不许盖到它的外包矩形：形外那点必须还是白底 ⇒ 实测 {corner:?}"
    );
}

/// 在目标矩形里横向取 `count` 个像素（步长 1，y 固定）。
fn row_alpha_and_colour(ctx: &mut ToolContext<'_>, x0: i32, y: i32, count: i32) -> Vec<[u64; 4]> {
    let points: Vec<[i32; 2]> = (0..count).map(|i| [x0 + i, y]).collect();
    let samples = registry()
        .call(ctx, "sample_colors", &json!({"points": points}))
        .get("samples")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    samples
        .iter()
        .map(|sample| {
            let mut out = [0u64; 4];
            if let Some(values) = sample.get("rgba").and_then(Value::as_array) {
                for (index, slot) in out.iter_mut().enumerate() {
                    *slot = values.get(index).and_then(Value::as_u64).unwrap_or(0);
                }
            }
            out
        })
        .collect()
}

/// 与 `x + period` 那一列**完全相同**的对数（`jitter=0` 时应当满格 ✓）。
fn repeated_pairs(row: &[[u64; 4]], period: usize) -> (usize, usize) {
    let mut same = 0usize;
    let mut total = 0usize;
    for index in 0..row.len().saturating_sub(period) {
        total += 1;
        if row[index] == row[index + period] {
            same += 1;
        }
    }
    (same, total)
}

#[test]
fn jitter_zero_tiles_exactly_and_jitter_one_breaks_the_period() {
    // **外部第 76 次实测** ✓：`fill_region(shape=path, texture=bitmap)` 通了（①=0.33 ✓），
    // 而**源块被原样平铺几十次 ⇒ 视觉上是「墙纸」** ✗ ⇒ 他们要 `texture.jitter` ✓。
    // 本判据钉两件**结构**事（不断像素数）：
    //   ① `jitter` 缺省 ⇒ **横向 `period=8` 的每一对必须逐字节相同** ✓（= 旧行为，不许被扰动 ✓）
    //   ② `jitter=1` ⇒ **至少有一对不再相同** ✓（= 周期真的被打散了 ✓）
    // 两件事**成对**才有意义：只断 ② 的话，"永远加确定性噪声"也能过 ✗；
    // 只断 ① 的话，"jitter 被忽略"也能过 ✗。
    for (jitter, expect_periodic) in [(None, true), (Some(1.0), false)] {
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
        let mut texture = json!({
            "type": "bitmap",
            "blob_hash": hash,
            "source": {"x": 0, "y": 0, "w": 8, "h": 8},
            "width": 8,
            "height": 8
        });
        if let Some(value) = jitter {
            texture["jitter"] = json!(value);
        }
        let filled = registry().call(
            &mut ctx,
            "fill_region",
            &json!({
                "layer_id": layer,
                "shape": {"type": "rect", "x": 32, "y": 32, "w": 32, "h": 32},
                "texture": texture
            }),
        );
        assert_eq!(
            filled.get("ok").and_then(Value::as_bool),
            Some(true),
            "jitter={jitter:?} 时 fill_region 必须成功：{filled}"
        );
        assert_eq!(
            filled
                .get("texture")
                .and_then(|texture| texture.get("jitter"))
                .and_then(Value::as_f64),
            Some(jitter.unwrap_or(0.0)),
            "生效的 jitter 必须回显（缺省 0）：{filled}"
        );
        let row = row_alpha_and_colour(&mut ctx, 34, 40, 24);
        let (same, total) = repeated_pairs(&row, 8);
        if expect_periodic {
            assert_eq!(
                same, total,
                "jitter 缺省时必须**原样平铺**（周期 8 ⇒ 每一对都相同）⇒ 实测 {same}/{total} 对相同"
            );
        } else {
            assert!(
                same < total,
                "jitter=1 时必须**打散周期**（至少一对不相同）⇒ 实测 {same}/{total} 对相同"
            );
        }
    }
}

#[test]
fn jitter_out_of_range_is_rejected_by_name() {
    // 越界不许静默夹：渲染侧拒绝会变成 `unsupported` 警告 ＋ 一个像素都不画 ⇒ 那是静默降级 ✗。
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
                "height": 8,
                "jitter": 1.5
            }
        }),
    );
    assert!(
        message.contains("jitter") && message.contains("0.0..=1.0"),
        "错误必须点名 jitter 与区间（不是只报「参数非法」）：{message}"
    );
}

#[test]
fn a_small_source_block_warns_about_repeating_its_features() {
    // **外部第 78 次实测的"图案重复"** ✓：`jitter` 生效了（错开网格 ✓），但**源块里的特征**
    // （红花／裙摆残影）被**反复复制** ⇒ 他们的结论是「选源三原则：小 30~50px／同质／离洞 ≥25px」✓。
    // **工具能算的**：目标面积 ÷ 源面积 ≈ 要复制多少次 ✓ ⇒ **超过 ~12 次就提醒** ✓（只提醒、不改行为 ✓）。
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

    // ① 大目标 ＋ 8×8 源 ⇒ 平铺约 150 次 ⇒ 必须**提醒**。
    let big = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 4, "y": 4, "w": 120, "h": 80},
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 100, "y": 4, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert_eq!(
        big.get("ok").and_then(Value::as_bool),
        Some(true),
        "大目标那一笔应当成功：{big}"
    );
    let tiles = big
        .get("texture")
        .and_then(|texture| texture.get("tiles"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    assert!(
        tiles >= 12.0,
        "`texture.tiles` 必须回显平铺次数（120×80 ÷ 8×8 = 150）⇒ 实测 {tiles}"
    );
    let warnings = big
        .get("warnings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(
        warnings
            .iter()
            .filter_map(Value::as_str)
            .any(|text| text.contains("平铺") && text.contains("同质")),
        "小源块铺进大目标必须提醒（特征会被复制 ＋ 给三条选源建议）⇒ {warnings:?}"
    );

    // ② 小目标 ＋ 大源块 ⇒ **不许**出现这条提醒（否则提醒就没有鉴别力 ✗）。
    let small = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 4, "y": 4, "w": 16, "h": 16},
            "texture": {
                "type": "bitmap",
                "blob_hash": hash,
                "source": {"x": 40, "y": 20, "w": 64, "h": 64},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert_eq!(
        small.get("ok").and_then(Value::as_bool),
        Some(true),
        "小目标那一笔应当成功：{small}"
    );
    assert!(
        small.get("warnings").is_none(),
        "源块比目标还大时**不该**提醒平铺（提醒必须有鉴别力）⇒ {small}"
    );
}

#[test]
fn a_region_texture_snapshots_the_document_instead_of_being_unsupported() {
    // **外部第 71~78 次一直撞的那条**「`texture.type:"region"` 未实现」✗：
    // 他们真正要的是"**从已经有颜色的地方取纹理**"✓ ⇒ 现在把它**就地拍成位图** ✓
    // （逐像素取源、平铺、`jitter`、`min_distance` 全部继承 ✓，而且**字节进了文档引用的 blob** ⇒ 重放确定 ✓）。
    let mut workspace = Workspace::in_memory(DocumentSettings::default());
    {
        let mut ctx = context(&mut workspace);
        new_document(&mut ctx);
    }
    let mut ctx = context(&mut workspace);
    let layer = first_layer(&mut ctx);
    // 先铺一块**左红右蓝**的"画布内容"（8×8 的两半，落在 (100,4) 附近）。
    let left = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 52, "y": 4, "w": 4, "h": 8},
            "color": [255, 0, 0, 255]
        }),
    );
    assert_eq!(
        left.get("ok").and_then(Value::as_bool),
        Some(true),
        "前置：红半没画上：{left}"
    );
    let right = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 56, "y": 4, "w": 4, "h": 8},
            "color": [0, 0, 255, 255]
        }),
    );
    assert_eq!(
        right.get("ok").and_then(Value::as_bool),
        Some(true),
        "前置：蓝半没画上：{right}"
    );

    // 再从那块区域取纹理，填到 (32,32,32,32)。
    let filled = registry().call(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 8, "y": 20, "w": 32, "h": 32},
            "texture": {"type": "region", "source": {"x": 52, "y": 4, "w": 8, "h": 8}}
        }),
    );
    assert_eq!(
        filled.get("ok").and_then(Value::as_bool),
        Some(true),
        "`texture.type=region` 必须已经可用（不许再回「尚未实现」）：{filled}"
    );
    assert_eq!(
        filled
            .get("texture")
            .and_then(|texture| texture.get("from_region"))
            .and_then(Value::as_bool),
        Some(true),
        "响应必须说明「这次是就地拍下来的」：{filled}"
    );
    // **逐像素取源**：目标左半红、右半蓝（平均会变紫 ⇒ 这条红）。
    let samples = registry().call(
        &mut ctx,
        "sample_colors",
        &json!({"points": [[10, 36], [36, 36]]}),
    );
    let samples = samples
        .get("samples")
        .and_then(Value::as_array)
        .expect("sample_colors 形状变了");
    let rgba = |index: usize| -> [u64; 3] {
        let values = samples[index]
            .get("rgba")
            .and_then(Value::as_array)
            .expect("缺 rgba");
        [
            values.first().and_then(Value::as_u64).unwrap_or(0),
            values.get(1).and_then(Value::as_u64).unwrap_or(0),
            values.get(2).and_then(Value::as_u64).unwrap_or(0),
        ]
    };
    let inside_left = rgba(0);
    let inside_right = rgba(1);
    assert!(
        inside_left[0] > 200 && inside_left[1] < 60 && inside_left[2] < 60,
        "目标左半必须取到源的红 ⇒ 实测 {inside_left:?}"
    );
    assert!(
        inside_right[2] > 200 && inside_right[0] < 60,
        "目标右半必须取到源的蓝 ⇒ 实测 {inside_right:?}"
    );
}

#[test]
fn a_misspelled_texture_key_is_rejected_with_the_available_ones() {
    // **外部 `bug_report.md` BUG-15 残留**：子对象里的**拼写错误被静默忽略** ✗
    // ⇒ 用户以为设了、其实没设 ✓（最典型：`blobhash` vs `blob_hash` ✓）。
    // 本判据钉：**未知键必须响亮** ＋ 给出**可用键清单** ＋（拼写近的）**近似提示** ✓。
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
    let message = expect_error(
        &mut ctx,
        "fill_region",
        &json!({
            "layer_id": layer,
            "shape": {"type": "rect", "x": 4, "y": 4, "w": 16, "h": 16},
            "texture": {
                "type": "bitmap",
                "blobhash": hash,                 // ★ 拼写错误
                "source": {"x": 40, "y": 20, "w": 8, "h": 8},
                "width": 8,
                "height": 8
            }
        }),
    );
    assert!(
        message.contains("blobhash"),
        "错误必须**点名**那个未知键（否则用户不知道是哪个键写错了）⇒ {message}"
    );
    assert!(
        message.contains("blob_hash"),
        "错误必须给出**可用键清单**（含正确的 `blob_hash`）⇒ {message}"
    );
    assert!(
        message.contains("你是想写") && message.contains("blob_hash"),
        "拼写接近时要给**近似提示**（`blobhash` ⇒ `blob_hash`）⇒ {message}"
    );
}
