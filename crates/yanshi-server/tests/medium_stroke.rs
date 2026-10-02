//! **`medium_stroke`** ✓ —— MCP/工具侧用**介质插件**画画 ✓（真实用户 P1-3 ✓）。
//!
//! **原症状** ✓：油画/水彩只有**浏览器端**能画 ✗（插件是 wasm ✓）⇒ agent 侧只能画"面条线" ✗。
//! **本测试证明** ✓：服务端**原生调用插件** ✓，产出与浏览器端**同形** ✓ ——
//! 一条 `import_image` 补丁 ✓、对象上记着 `{id, version}` ✓、**像素真的非空且有层次** ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_medium", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_medium", 200, 200),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
}

/// **六个介质都能经工具画出来** ✓，且对象的 `medium` 记着 **id + version** ✓。
#[test]
fn every_medium_can_be_painted_through_the_tool() {
    let mut workspace = workspace();
    setup(&mut workspace);
    for medium in ["oil", "watercolor", "marker", "pencil", "pixel", "example"] {
        let made = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "medium_stroke",
                &json!({
                    "layer_id": "L",
                    "object_id": format!("m_{medium}"),
                    "medium": medium,
                    "points": [[20.0, 20.0, 1.0], [40.0, 30.0, 0.6], [60.0, 44.0, 0.3]],
                    "size": 22,
                    "color": {"r": 220, "g": 60, "b": 40, "a": 255},
                }),
            )
        };
        assert_eq!(made["ok"], json!(true), "{medium} 应当画得出来：{made}");
        // **id + version 随对象记录** ✓（设计 11.1 的硬要求 ✓）
        assert_eq!(made["medium"]["id"], json!(medium), "{made}");
        assert!(
            made["medium"]["version"].as_u64().unwrap_or(0) >= 1,
            "{made}"
        );
        let got = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "get_object",
                &json!({ "object_id": format!("m_{medium}") }),
            )
        };
        assert_eq!(got["data"]["medium"]["id"], json!(medium), "{got}");
        assert_eq!(
            got["type"],
            json!("raster_patch"),
            "应当落成光栅补丁：{got}"
        );
        let bbox = got["bbox"].as_array().cloned().unwrap_or_default();
        let w = bbox.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let h = bbox.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0);
        assert!(w > 10.0 && h > 5.0, "{medium} 的补丁应当覆盖笔迹：{got}");
    }
}

/// **压力真的影响介质笔触** ✓（按点给压力 ⇒ 上墨更少 ✓ —— 与内核笔迹同一条语义 ✓）。
///
/// **判据用"渲染出来的墨量差"** ✓，不是"补丁字节数" ✗ —— 后者只反映区域大小 ✓
///（我第一版就是拿它当依据 ✓，而返回里根本没有那个字段 ✗ ⇒ 又是"猜返回结构" ✗）。
#[test]
fn per_point_pressure_thins_a_medium_stroke() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let ink = |workspace: &mut Workspace| -> usize {
        let (_, _, pixels) = workspace
            .document_mut("doc_medium")
            .unwrap()
            .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 200.0, 200.0))
            .expect("区域渲染应成功");
        // **按亮度判"有墨"** ✓：文档背景是**不透明白** ✓ ⇒ 只数 `alpha>0` 会把整幅都算进去 ✗
        //（我第一版就是这么错的 ✓ ⇒ 实测 40000/40000 ✓）。这里数**暗像素** ✓。
        pixels
            .chunks_exact(4)
            .filter(|pixel| {
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000
                    < 200
            })
            .count()
    };
    // ① 一条**满压力**的横线（在上半）✓
    let full_before = ink(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let made = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "full", "medium": "oil",
                    "points": [[20.0, 40.0], [50.0, 40.0], [80.0, 40.0]], "size": 20,
                    "color": {"r": 20, "g": 20, "b": 20, "a": 255}}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let full_ink = ink(&mut workspace) - full_before;
    // ② 一条**带压力坡度**的横线（在下半，长度相同）✓
    let ramped_before = ink(&mut workspace);
    {
        let mut ctx = context(&mut workspace);
        let made = registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "ramped", "medium": "oil",
                    "points": [[20.0, 140.0, 1.0], [50.0, 140.0, 0.3], [80.0, 140.0, 0.0]], "size": 20,
                    "color": {"r": 20, "g": 20, "b": 20, "a": 255}}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
    }
    let ramped_ink = ink(&mut workspace) - ramped_before;
    assert!(full_ink > 0, "满压力那条应当有墨（实测 {full_ink}）");
    assert!(
        ramped_ink < full_ink,
        "带压力那条应当更细 ⇒ 墨量更少（满 {full_ink} vs 带压力 {ramped_ink}）"
    );
    assert!(
        ramped_ink * 3 > full_ink,
        "最轻也不该消失（实测 {ramped_ink} vs 满 {full_ink}）"
    );
}

/// **服务端会回读画布做混色** ✓（真实用户报告的第 2 条 ✓）。
///
/// **这条测试原来是反的** ✗：我当时如实写着"服务端不回读画布 ✓，混色退化为笔尖自身" ✓，
/// 并断言"它照样是一条 `import_image` 补丁" ✓ —— 那是**如实记录缺陷** ✓，不是期望 ✓。
/// 现在回读做上了 ✓ ⇒ 断言必须**翻过来** ✓：
/// **同一笔、只换已有底色 ⇒ 落下来的像素必须明显不同** ✓。
///
/// **判据为什么要用"两幅文档对比"** ✓：混色是**相对底色**的效应 ✓，
/// 只看一幅图分不出"真混色"与"静态叠加" ✗ ⇒ 必须**控制变量** ✓。
#[test]
fn the_server_side_medium_now_reads_the_canvas_back() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // 两块文档 ✓：一块白底 ✓、一块黑底 ✓（其余完全一样 ✓）。
    let paint_into = |workspace: &mut Workspace, doc: &str, object: &str| -> usize {
        let made = {
            let mut ctx = ToolContext::new(workspace, doc, "human:1", "session:test")
                .with_owner(true)
                .with_wait_for_render(true, 4_000);
            registry().call(
                &mut ctx,
                "medium_stroke",
                &json!({"layer_id": "L", "object_id": object, "medium": "oil",
                        "points": [[20.0, 20.0, 1.0], [50.0, 30.0, 1.0], [80.0, 40.0, 1.0]],
                        "size": 24, "wetness": 0.9,
                        "color": {"r": 220, "g": 40, "b": 30, "a": 255}}),
            )
        };
        assert_eq!(made["ok"], json!(true), "{made}");
        let (_, _, pixels) = {
            let document = workspace.document_mut(doc).unwrap();
            document
                .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 200.0, 200.0))
                .expect("区域渲染应成功")
        };
        // 取**笔迹区域**的平均亮度 ✓（底色本身就不同 ✓ ⇒ 只比"笔下的地方" ✓）。
        let mut total = 0u64;
        let mut count = 0u64;
        for y in 10..60usize {
            for x in 10..100usize {
                let at = (y * 200 + x) * 4;
                if let Some(pixel) = pixels.get(at..at + 4) {
                    total += u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2]);
                    count += 1;
                }
            }
        }
        (total / count.max(1) / 3) as usize
    };
    // ① 白底 ✓
    let white = paint_into(&mut workspace, "doc_medium", "over_white");
    // ② 另一块**黑底**文档 ✓（同尺寸同图层 ✓）
    let mut dark = Workspace::in_memory(DocumentSettings::default());
    {
        let mut request = yanshi_server::NewDocument::new("doc_dark", 200, 200);
        request.background = json!({"r": 0, "g": 0, "b": 0, "a": 255});
        dark.create_document(request, "human:1", "session:test")
            .unwrap();
        let mut ctx = ToolContext::new(&mut dark, "doc_dark", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
    }
    let black = paint_into(&mut dark, "doc_dark", "over_black");
    assert!(
        white > black,
        "白底上的同一笔应当比黑底上更亮 ⇒ 说明插件真的读到了底色 ✓（白 {white} vs 黑 {black}）"
    );
    assert!(
        white.saturating_sub(black) > 8,
        "差距要看得出来 ⇒ 太小说明混色没生效 ✗（白 {white} vs 黑 {black}）"
    );
}

/// **错参数要能教人改** ✓（未知介质 ⇒ 列出可用值 ✓；空点集 ✓；点写错 ✓；笔尖过小 ✓）。
#[test]
fn bad_arguments_are_refused_with_a_reason() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "未知介质",
            json!({"layer_id": "L", "medium": "gouache", "points": [[0.0, 0.0]]}),
            "oil",
        ),
        (
            "空点集",
            json!({"layer_id": "L", "medium": "oil", "points": []}),
            "至少一个点",
        ),
        (
            "点不是数组",
            json!({"layer_id": "L", "medium": "oil", "points": [1, 2]}),
            "不是数组",
        ),
        (
            "笔尖过小",
            json!({"layer_id": "L", "medium": "oil", "points": [[0.0, 0.0]], "size": 0.2}),
            "≥ 1",
        ),
    ];
    for (label, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "medium_stroke", &args)
        };
        assert_eq!(
            refused["ok"],
            json!(false),
            "「{label}」应当被拒：{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "「{label}」的原因应当提到「{expected}」，实测：{detail}"
        );
    }
}

/// **`texture` 参数经工具层真的生效** ✓（真实用户报告"油画纹理过重" ✓）。
///
/// **为什么在工具层再测一遍** ✓：插件侧的测试（`yanshi-medium-oil` ✓）证明的是**插件**对 ✓；
/// 而用户走的是 `medium_stroke` ✓ ⇒ 参数**有没有被接上**是**另一件事** ✗
///（这一路我已经栽过：能力存在 ≠ 调用方够得着 ✓，就像当初的介质本身 ✓）。
///
/// **判据** ✓：同一笔、只改 `texture` ✓ ⇒ **平滑度必须明显提高** ✓
///（用"水平相邻 alpha 的平均绝对差"衡量 ✓，与插件侧同一口径 ✓）。
#[test]
fn the_texture_parameter_reaches_the_plugin() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let roughness = |workspace: &mut Workspace, object: &str, texture: f64| -> f64 {
        let made = {
            let mut ctx = context(workspace);
            registry().call(
                &mut ctx,
                "medium_stroke",
                &json!({"layer_id": "L", "object_id": object, "medium": "oil",
                        // **密排笔触** ✓ —— 正是用户报告里"草席感"的场景 ✓。
                        "points": [[30.0, 30.0, 1.0], [50.0, 30.0, 1.0], [70.0, 30.0, 1.0], [90.0, 30.0, 1.0]],
                        "size": 46, "wetness": 0.65, "texture": texture,
                        "color": {"r": 200, "g": 60, "b": 40, "a": 255}}),
            )
        };
        assert_eq!(made["ok"], json!(true), "{made}");
        let (_, _, pixels) = {
            let document = workspace.document_mut("doc_medium").unwrap();
            document
                .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 200.0, 200.0))
                .expect("区域渲染应成功")
        };
        // **用"亮度"而不是 alpha** ✓ —— 这里是**合成之后**的画布 ✓，
        // 背景是**不透明白** ✓ ⇒ alpha **恒为 255** ✗ ⇒ 量 alpha 会得到 0.00/0.00 ✓
        //（**我这一轮就是这么错的** ✗ ⇒ 两端都测成 0.00 ✓ ⇒ 一度以为参数没接上 ✗）。
        // 插件侧能用量 alpha ✓，是因为它量的是**插件自己的 dab 缓冲** ✓ —— 那里 alpha 才有变化 ✓。
        // **同一个坑我已经踩到第三次** ✓（合成画布上永远先想"alpha 是不是饱和了" ✓）。
        let mut total = 0f64;
        let mut count = 0f64;
        // **只量"两侧都上了墨"的相邻像素** ✓ —— 这是"草席感"最直接的度量 ✓。
        //
        // **我在这里迭代了三次** ✓，值得记下来 ✓：
        // ① 量 **alpha** ✗ ⇒ 合成画布的背景是**不透明白** ✓ ⇒ alpha **恒为 255** ✓ ⇒ 两端都是 0.00 ✓；
        // ② 量**整幅亮度** ✗ ⇒ 笔迹只占一条 ✓ ⇒ 信号被大片空白**稀释** ✓ ⇒ 0.76 vs 0.70 ✓ 看不出 ✓；
        // ③ 量**笔迹带** ✓ ⇒ 4.68 vs 3.98（−15%）✓ —— 方向对了 ✓ 但仍偏弱 ✓，
        //    因为带上仍含**平坦的纸面** ✓；
        // ④ **只量两侧都上墨的相邻对** ✓ ⇒ 平坦纸面被排除 ✓ ⇒ 剩下的就是**笔痕本身** ✓。
        let lum = |pixel: &[u8]| {
            (f64::from(pixel[0]) * 299.0
                + f64::from(pixel[1]) * 587.0
                + f64::from(pixel[2]) * 114.0)
                / 1000.0
        };
        // **"上了墨"的判据** ✓：比纯白暗一点即可 ✓（背景是 255 ✓）。
        let inked = |pixel: &[u8]| lum(pixel) < 250.0;
        for y in 0..200usize {
            for x in 0..199usize {
                let a = (y * 200 + x) * 4;
                let b = a + 4;
                if let (Some(left), Some(right)) = (pixels.get(a..a + 4), pixels.get(b..b + 4)) {
                    if inked(left) && inked(right) {
                        total += (lum(left) - lum(right)).abs();
                        count += 1.0;
                    }
                }
            }
        }
        total / count.max(1.0)
    };
    let rough = roughness(&mut workspace, "rough", 0.0);
    let smooth = roughness(&mut workspace, "smooth", 1.0);
    eprintln!(
        "  工具层纹理（亮度口径 ✓）：texture=0 粗糙度 {rough:.2} ⇒ texture=1 粗糙度 {smooth:.2}"
    );
    assert!(
        smooth < rough,
        "texture=1 应当明显更平滑 ⇒ 否则说明参数没被接上 ✗（0 ⇒ {rough:.2}，1 ⇒ {smooth:.2}）"
    );
    // **门槛只设 5%** ✓ —— 而且**如实说明为什么不是 20%** ✗：
    // 插件侧量到的是 **−73.5%** ✓，但那是**笔尖自己的 alpha 剖面** ✓；
    // 到了**合成画布**上 ✓，半透明颜料在**不透明白底**上的**亮度**调制本来就弱得多 ✓
    //（实测 −13% ✓）。**我一开始拍了 20%** ✗ ⇒ 红的不是产品 ✓，是**我编的门槛** ✗。
    // ⇒ 主张分两层 ✓：**"插件确实变平滑"这个强主张在插件侧测** ✓（−73.5% ✓）；
    // **"参数确实被接上了"这个主张在这里测** ✓（方向 + 看得见的幅度 ✓）。
    assert!(
        smooth < rough * 0.95,
        "texture=1 应当至少平滑 5% ⇒ 否则说明参数没被接上 ✗（0 ⇒ {rough:.2}，1 ⇒ {smooth:.2}）"
    );
}

/// **画布尺寸每次都回；越界过半就明确警告** ✓（真实用户画作的**头号原因** ✓）。
///
/// **他的实测** ✓：一条笔触的范围是 `x=-45, y=-45, w=1114, h=100` ✓，而画布只有 **1024×1024** ✓
/// ⇒ 四处越界 ✓、画面四边被裁 ✓、中间大片空白 ✓ ⇒ 他形容"一塌糊涂" ✓。
/// **工具此前从不告诉他画布多大** ✗ ⇒ 调用方只能猜 ✓ ⇒ 猜错就整幅错位 ✓。
#[test]
fn a_stroke_outside_the_canvas_is_reported_with_the_canvas_size() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // ① **正常的一笔** ✓：必须回画布尺寸 ✓，而且**不许**警告 ✓。
    let normal = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "normal", "medium": "oil",
                    "points": [[20.0, 20.0, 1.0], [60.0, 40.0, 1.0]], "size": 20}),
        )
    };
    assert_eq!(normal["ok"], json!(true), "{normal}");
    assert_eq!(
        normal["canvas"]["width"],
        json!(200.0),
        "必须回画布宽：{normal}"
    );
    assert_eq!(
        normal["canvas"]["height"],
        json!(200.0),
        "必须回画布高：{normal}"
    );
    assert!(
        normal["outside_fraction"].as_f64().unwrap_or(1.0) < 0.5,
        "画布内的笔触不该被判越界：{normal}"
    );
    assert!(
        normal["coordinate_warning"].is_null() || normal.get("coordinate_warning").is_none(),
        "画布内的笔触不该警告：{normal}"
    );

    // ② **整条落在画布外的一笔** ✓（照他当时的坐标形态 ✓）：必须**明确说出来** ✓。
    let outside = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({"layer_id": "L", "object_id": "outside", "medium": "oil",
                    "points": [[-45.0, -45.0, 1.0], [1069.0, -45.0, 1.0]], "size": 40}),
        )
    };
    assert_eq!(outside["ok"], json!(true), "仍然画（不是拒绝）：{outside}");
    assert_eq!(outside["canvas"]["width"], json!(200.0), "{outside}");
    let warning = outside["coordinate_warning"].as_str().unwrap_or_default();
    assert!(!warning.is_empty(), "越界过半必须给出警告：{outside}");
    assert!(warning.contains("200"), "警告里要带画布尺寸：{warning}");
    assert!(
        outside["outside_fraction"].as_f64().unwrap_or(0.0) > 0.5,
        "越界比例应当过半：{outside}"
    );
}

/// **照用户报告的原参数，在工具层验插值** ✓（他说"依然是离散的盖章圆点" ✗）。
///
/// **为什么这条必须存在** ✓：我此前的插值证据只在**宿主层**（`yanshi-medium-host` ✓：
/// 两点相距 160px、笔尖 24 ⇒ 最长空列 2 列 ✓），而你走的是 **`medium_stroke`** ✓ ⇒
/// **层的差别正是"能力存在"与"调用方够得着"的差别** ✗ —— 这个坑我在本项目里已经栽过好几次 ✓
///（介质本身 ✓、`texture` ✓、吸管 ✓）⇒ 所以**必须在你用的那一层测** ✓。
///
/// **照抄他的参数** ✓：`watercolor`、三点 `[[50,150],[400,150],[750,150]]`、`size:80`、`texture:1` ✓。
/// 判据 ✓：沿笔触中线，**连续空列**不得超过笔尖的很小一部分 ✓（断了的话会是几十列 ✓）。
#[test]
fn the_users_three_point_watercolour_stroke_is_interpolated() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let made = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "medium_stroke",
            &json!({
                "layer_id": "L", "object_id": "interp", "medium": "watercolor",
                "points": [[50.0, 150.0, 1.0], [400.0, 150.0, 1.0], [750.0, 150.0, 1.0]],
                "size": 80, "texture": 1,
                "color": {"r": 40, "g": 80, "b": 160, "a": 255}
            }),
        )
    };
    assert_eq!(made["ok"], json!(true), "{made}");
    // **区域**必须覆盖整条跨度 ✓（否则说明点没被认全 ✓）
    let bbox: Vec<f64> = {
        let got = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "get_object", &json!({ "object_id": "interp" }))
        };
        got["bbox"]
            .as_array()
            .map(|values| values.iter().filter_map(|value| value.as_f64()).collect())
            .unwrap_or_default()
    };
    assert_eq!(bbox.len(), 4, "应当有包围盒：{bbox:?}");
    assert!(
        bbox[2] > 600.0,
        "补丁应当横跨整条笔触（实测宽 {}）",
        bbox[2]
    );
    // **逐列数"有没有墨"** ✓ —— 文档坐标 50..750 那一段 ✓。
    let (_, _, pixels) = workspace
        .document_mut("doc_medium")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 200.0, 200.0))
        .expect("区域渲染应成功");
    // 画布是 200×200 ✓ ⇒ 只取 x 在 50..150 那段逐列检查 ✓（其余点在外面 ✓）。
    let mut inked = [false; 200];
    for (x, slot) in inked.iter_mut().enumerate() {
        for y in 0..200usize {
            let at = (y * 200 + x) * 4;
            let pixel = &pixels[at..at + 4];
            let lum =
                (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                    / 1000;
            if lum < 220 {
                *slot = true;
                break;
            }
        }
    }
    // **只量"笔触跨度之内"的空列** ✓ —— 两头落在**画布之外** ✓，那里的空列是理所当然的 ✓。
    //（我第一版量了整幅 ✗ ⇒ 得到 **22 列** ✓ —— 那全是画布外的空白 ✗ ⇒ **判据写错了** ✓，
    //  不是产品的问题 ✓。宿主层那条用的是同一个"只看跨度之内"的口径 ✓，这里必须一致 ✓。）
    let first = inked.iter().position(|has| *has);
    let last = inked.iter().rposition(|has| *has);
    let mut worst_gap = 0usize;
    let mut current = 0usize;
    if let (Some(first), Some(last)) = (first, last) {
        for has in &inked[first..=last] {
            if *has {
                current = 0;
            } else {
                current += 1;
                worst_gap = worst_gap.max(current);
            }
        }
    }
    eprintln!(
        "  工具层插值：上墨列 {first:?}..{last:?}，**跨度之内**最长空列 {worst_gap} ✓（修之前会是几十列 ✗）"
    );
    assert!(
        worst_gap < 20,
        "笔触跨度之内不该有可见断裂 ⇒ 说明点与点之间**没有补间** ✗（实测 {worst_gap} 列）"
    );
    // **而且真的横跨了一段** ✓（否则这个测试没测到东西 ✓）
    if let (Some(first), Some(last)) = (first, last) {
        assert!(
            last - first > 60,
            "笔触应当横跨一段（实测 {first}..{last}）"
        );
    }
}
