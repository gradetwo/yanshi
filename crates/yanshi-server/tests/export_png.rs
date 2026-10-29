//! **`export_png`** ✓ —— 任意尺寸、**真的落盘** ✓（真实用户报的 §P2-6 ✓）。
//!
//! **原症状** ✓：`render_region` 的 `include_image` **上限 512px** ✗，超了只回
//! `yanshi://blob/...` ✗ —— **伪协议** ✓，进程外取不到 ✗ ⇒ 导出这一步**没有出口** ✗。
//! **本测试用"自己的解码器读回来"证明它是真 PNG** ✓，而不是只看返回的字节数 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_png", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

/// 造一个 800×600 的文档并画两个形状 ✓（**故意大于 512** ✓ —— 那是原症状的边界 ✓）。
fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_png", 800, 600),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
    registry().call(
        &mut ctx,
        "draw_shape",
        &json!({"layer_id": "L", "object_id": "sq",
                "data": {"geometry": {"kind": "rect", "bbox": {"x": 40, "y": 40, "w": 300, "h": 200}},
                         "color": {"r": 220, "g": 60, "b": 40, "a": 255}}}),
    );
}

fn temp_path(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("yanshi_export_{}_{}.png", name, std::process::id()));
    path
}

/// **大于 512 也能导出，而且是一个真 PNG** ✓（用自己的解码器读回来 ✓）。
#[test]
fn exporting_a_large_canvas_writes_a_real_png() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let path = temp_path("large");
    let _ = std::fs::remove_file(&path);
    let exported = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert_eq!(
        exported["width"],
        json!(800),
        "应当按原尺寸导出：{exported}"
    );
    assert_eq!(exported["height"], json!(600), "{exported}");
    assert_eq!(exported["scaled"], json!(false), "{exported}");
    // ① **文件真的存在** ✓ ② **字节数与返回一致** ✓
    let bytes = std::fs::read(&path).expect("导出文件应当存在");
    assert_eq!(bytes.len() as u64, exported["bytes"].as_u64().unwrap_or(0));
    // ③ **它是真 PNG** ✓：用项目**自己的**解码器读回来 ✓（零依赖 ✓，也顺带验了编码器 ✓）。
    let (width, height, rgba) = yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG");
    assert_eq!((width, height), (800, 600), "PNG 尺寸应当一致");
    assert_eq!(rgba.len(), 800 * 600 * 4, "像素数应当一致");
    // ④ **画的东西真的在里面** ✓：至少有一个像素偏红 ✓（不是一张空图 ✓）。
    let reddish = rgba
        .chunks_exact(4)
        .filter(|pixel| pixel[0] > 120 && pixel[1] < 120)
        .count();
    assert!(
        reddish > 1000,
        "导出图里应当有画的那个形状（实测 {reddish} 个偏红像素）"
    );
    let _ = std::fs::remove_file(&path);
}

/// **能指定尺寸** ✓（`width`+`height` 与 `max_edge` 两条路 ✓）。
#[test]
fn the_export_size_can_be_asked_for() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // ① 显式宽高 ✓
    let path = temp_path("exact");
    let _ = std::fs::remove_file(&path);
    let exported = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy(), "width": 200, "height": 150 }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert_eq!(
        (exported["width"].as_u64(), exported["height"].as_u64()),
        (Some(200), Some(150))
    );
    assert_eq!(exported["scaled"], json!(true), "{exported}");
    let bytes = std::fs::read(&path).expect("应当写出文件");
    let (width, height, _) = yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG");
    assert_eq!((width, height), (200, 150));
    let _ = std::fs::remove_file(&path);

    // ② `max_edge` ✓：800×600 限到 400 ⇒ 400×300 ✓（等比 ✓）
    let path2 = temp_path("edge");
    let _ = std::fs::remove_file(&path2);
    let limited = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path2.to_string_lossy(), "max_edge": 400 }),
        )
    };
    assert_eq!(limited["ok"], json!(true), "{limited}");
    assert_eq!(
        (limited["width"].as_u64(), limited["height"].as_u64()),
        (Some(400), Some(300)),
        "等比缩放应当保持比例：{limited}"
    );
    let _ = std::fs::remove_file(&path2);
}

/// **错误要能教人改** ✓（宽高只给一个 ✓ / `max_edge` 与宽高同给 ✓ / 算法名不认识 ✓ / 路径写不进去 ✓）。
#[test]
fn bad_arguments_are_refused_with_a_reason() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let path = temp_path("bad");
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "只给宽",
            json!({"path": path.to_string_lossy(), "width": 100}),
            "一起给",
        ),
        (
            "两种尺寸方式同给",
            json!({"path": path.to_string_lossy(), "width": 100, "height": 100, "max_edge": 50}),
            "二选一",
        ),
        (
            "算法名不认识",
            json!({"path": path.to_string_lossy(), "filter": "lanczos"}),
            "nearest",
        ),
        (
            // **审计驱动的话术变更**：`/proc/...` 现在在**更早**的路径守卫处就被拒了
            //（第三方代码审计 P0 #1：导出原先能写任意路径 ⇒ 现在只允许导出目录与系统临时目录）。
            // 该用例的**本意**（坏路径必须被拒、且给出原因）不变 ✓，只是原因换了措辞 ✓。
            "路径在允许目录之外",
            json!({"path": "/proc/definitely/not/writable/x.png"}),
            "拒绝写",
        ),
    ];
    for (label, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "export_png", &args)
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

/// **拒绝文案要打印当前生效导出目录的绝对路径** ✓（反馈报告 2 期 4）：
/// 旧文案只给规则与改法、不给当前值 ⇒ 调用方仍要猜“现在到底是哪个目录” ✗；
/// 新文案保留旧句、末尾追加当前导出目录绝对路径 ✓（沙箱策略不动 ✓）。
#[test]
fn export_rejection_names_the_effective_export_dir() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({"path": "/proc/definitely/not/writable/x.png"}),
        )
    };
    assert_eq!(refused["ok"], json!(false), "{refused}");
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    // ① 旧句一字不动 ✓
    assert!(
        detail.contains("拒绝写 /proc/definitely/not/writable/x.png：只能写进导出目录（缺省 ./exports，可用 YANSHI_EXPORT_DIR 改）或系统临时目录"),
        "旧句应当保留：{detail}"
    );
    // ② 新值：当前生效导出目录的绝对路径（缺省 ./exports 按进程工作目录解析；
    // 环境变量若已设置则按它解析；只读环境变量，不改它 ⇒ 不干扰同二进制其他测试 ✓）。
    let expected = match std::env::var("YANSHI_EXPORT_DIR") {
        Ok(dir) => {
            let path = std::path::PathBuf::from(dir);
            if path.is_absolute() {
                path
            } else {
                std::env::current_dir()
                    .expect("测试进程应当有工作目录")
                    .join(path)
            }
        }
        Err(_) => std::env::current_dir()
            .expect("测试进程应当有工作目录")
            .join("exports"),
    };
    let expected = expected.to_string_lossy().into_owned();
    assert!(
        detail.contains(expected.as_str()),
        "拒绝文案应当打印当前导出目录绝对路径 {expected}，实测：{detail}"
    );
}

/// **整幅导出要顺手刷新"打开即图片"缓存** ✓（真实用户工程包里那张**空 `render.png`** 的根因 ✓）。
///
/// **他四个包实测** ✓：缓存 `render.seq` 是 **5–6** ✓，而原子最高 **97–217** ✓
/// ⇒ 缓存停在**刚建文档时的空白** ✗ ⇒ 包里那份预览是空的 ✓。
/// **根因** ✓：缓存只在"整幅渲染 / 文档级缩略图"时更新 ✓，而导出走 `render_region_raw` ✓ **绕过缓存** ✗。
///
/// **本测试** ✓：画完之后整幅导出 ⇒ 缓存必须**追上 head** ✓、而且**不是空白** ✓。
#[test]
fn a_full_frame_export_refreshes_the_render_cache() {
    let root = std::env::temp_dir().join(format!("yanshi_cache_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_cache", 240, 180),
            "human:1",
            "session:test",
        )
        .unwrap();
    {
        let mut ctx = ToolContext::new(&mut workspace, "doc_cache", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(&mut ctx, "create_layer", &json!({ "layer_id": "L" }));
        let drawn = registry().call(
            &mut ctx,
            "draw_shape",
            &json!({"layer_id": "L", "object_id": "s1",
                    "data": {"geometry": {"kind": "rect", "bbox": {"x": 20, "y": 20, "w": 120, "h": 90}},
                             "color": {"r": 0, "g": 0, "b": 0, "a": 255}}}),
        );
        assert_eq!(drawn["ok"], json!(true), "{drawn}");
    }
    let path = std::env::temp_dir().join(format!("yanshi_cache_export_{}.png", std::process::id()));
    let exported = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_cache", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(true, 4_000);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    // ① **缓存文件存在** ✓，② **它的 seq 追上了 head** ✓
    let cache_png = root.join("docs").join("doc_cache").join("render.png");
    let cache_seq = root.join("docs").join("doc_cache").join("render.seq");
    assert!(
        cache_png.exists(),
        "整幅导出之后应当写出渲染缓存：{}",
        cache_png.display()
    );
    let head = workspace.document("doc_cache").unwrap().head_seq();
    let cached: u64 = std::fs::read_to_string(&cache_seq)
        .expect("缓存应当有 seq")
        .trim()
        .parse()
        .expect("seq 应当是数字");
    assert_eq!(
        cached, head,
        "缓存的 seq 必须追上 head ⇒ 否则它还是过期的 ✗"
    );
    // ③ **缓存里不是空白** ✓（这正是用户看到的那张空图 ✓）
    let bytes = std::fs::read(&cache_png).expect("缓存应当可读");
    let (width, height, rgba) = yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG");
    assert_eq!((width, height), (240, 180));
    let inked = rgba
        .chunks_exact(4)
        .filter(|pixel| {
            (u32::from(pixel[0]) * 299 + u32::from(pixel[1]) * 587 + u32::from(pixel[2]) * 114)
                / 1000
                < 200
        })
        .count();
    assert!(
        inked > 1000,
        "缓存里应当有画的东西（实测 {inked} 个暗像素）"
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&root);
}

/// **期 4 `lod`**：`export_png` 的回执必须**真的读 `lod`** ✓（不是只在 schema 里声明 ✓）。
///
/// **∴ 为什么要有这条 ✗**：`lod` 若被静默忽略 ⇒ 回执照样回显 `lod: 1`（写死）或
///   `lod: 0`（缺省）✓，而画面仍是全分辨率 ✗ —— **回执撒谎**（静默降级形态 ✓）。
///   **∴ 所以**：断**尺寸**（`width`/`height` 真的减半 ✓）＋ 断**恒等式**
///   （`len == w*h*4` ✓、`scaled` 规则 ✓）—— 尺寸是 `lod` 被读过的**证据** ✓。
///   **变异**：把 `render_region_raw_with_lod` 的 `lod` 写死成 0 ⇒ `width` 仍 800 ⇒ 红 ✓。
#[test]
fn lod_is_read_and_reported() {
    let mut workspace = workspace();
    setup(&mut workspace);
    // ① `lod: 1`：800×600 ⇒ 400×300，`scaled` 必须真 ✓
    let path = temp_path("lod1");
    let _ = std::fs::remove_file(&path);
    let exported = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy(), "lod": 1 }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    assert_eq!(
        exported["lod"],
        json!(1),
        "回执必须回显生效档位：{exported}"
    );
    assert_eq!(
        (exported["width"].as_u64(), exported["height"].as_u64()),
        (Some(400), Some(300)),
        "lod:1 必须真的减半渲染：{exported}"
    );
    assert_eq!(
        (
            exported["source_width"].as_u64(),
            exported["source_height"].as_u64()
        ),
        (Some(400), Some(300)),
        "source_width/source_height 是本次实际渲染出的像素尺寸：{exported}"
    );
    assert_eq!(
        exported["scaled"],
        json!(true),
        "lod > 0 ⇒ scaled 必须真：{exported}"
    );
    let bytes = std::fs::read(&path).expect("导出文件应当存在");
    assert_eq!(bytes.len() as u64, exported["bytes"].as_u64().unwrap_or(0));
    let (w, h, rgba) = yanshi_render::png::decode_png(&bytes).expect("应当是合法 PNG");
    assert_eq!((w, h), (400, 300), "PNG 尺寸必须与回执一致");
    assert_eq!(rgba.len(), 400 * 300 * 4, "像素数恒等式 len == w*h*4");
    // ② `lod: 0` 与不给 `lod`：与今天相同（`scaled` 假、`lod` 回显 0 ✓）
    for args in [
        json!({ "path": temp_path("lod0").to_string_lossy(), "lod": 0 }),
        json!({ "path": temp_path("loddef").to_string_lossy() }),
    ] {
        let exported = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "export_png", &args)
        };
        assert_eq!(exported["ok"], json!(true), "{exported}");
        assert_eq!(exported["lod"], json!(0), "缺省档位是 0：{exported}");
        assert_eq!(
            (exported["width"].as_u64(), exported["height"].as_u64()),
            (Some(800), Some(600)),
            "lod:0 必须是全分辨率：{exported}"
        );
        assert_eq!(
            exported["scaled"],
            json!(false),
            "lod:0 且无尺寸参数 ⇒ scaled 假：{exported}"
        );
    }
    let _ = std::fs::remove_file(&path);
}

/// **期 4 `lod`**：超出 0..=3 / 非整数 ⇒ `invalid_argument`（不静默钳制 ✓）。
#[test]
fn lod_rejects_out_of_range_and_fractional() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let path = temp_path("lod_bad");
    let cases: Vec<(&str, serde_json::Value, &str)> = vec![
        (
            "lod: 4 超出上限",
            json!({ "path": path.to_string_lossy(), "lod": 4 }),
            "0..=3",
        ),
        (
            "lod: -1 负数",
            json!({ "path": path.to_string_lossy(), "lod": -1 }),
            "不小于 0 的整数",
        ),
        // **小数在更早的 schema 层就被拒**（`ParamKind::Integer` 的类型检查）⇒
        // 文案是「类型应为 integer」，但错误码同样是 `invalid_argument` ✓。
        (
            "lod: 1.5 小数",
            json!({ "path": path.to_string_lossy(), "lod": 1.5 }),
            "integer",
        ),
    ];
    for (label, args, expected) in cases {
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(&mut ctx, "export_png", &args)
        };
        assert_eq!(
            refused["ok"],
            json!(false),
            "「{label}」应当被拒：{refused}"
        );
        assert_eq!(
            refused["error_code"],
            json!("invalid_argument"),
            "「{label}」必须是 invalid_argument：{refused}"
        );
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("lod") && detail.contains(expected),
            "「{label}」的原因应当提到 lod 与「{expected}」，实测：{detail}"
        );
    }
}

/// **期 4 `region`**：完全在画布外 ⇒ 现有 `invalid_argument` 文案（行为不变，只补判据 ✓）。
#[test]
fn region_fully_outside_is_refused() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let path = temp_path("region_out");
    let refused = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy(),
                     "region": {"x": 900, "y": 700, "w": 100, "h": 100} }),
        )
    };
    assert_eq!(
        refused["ok"],
        json!(false),
        "完全在画布外的区域应当被拒：{refused}"
    );
    assert_eq!(
        refused["error_code"],
        json!("invalid_argument"),
        "必须是 invalid_argument：{refused}"
    );
    let detail = refused["context"]["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("不相交"),
        "原因应当是现有「渲染区域与文档不相交」文案，实测：{detail}"
    );
}

/// **期 4 缓存**：同 `(region, lod, fingerprint)` 二次导出 ⇒ 命中 +1 且告警一致 ✓，
/// 且 `lod: 0` 的条目**不得**替 `lod: 1` 命中（键含 lod 维 ✓）。
///
/// **变异**：把 `BlockKey` 的 `lod` 维去掉 ⇒ 第二次 `lod: 1` 导出命中 `lod: 0` 的条目 ⇒
/// 像素尺寸错（200×150 的位置上是 400×300 的字节）⇒ `width` 断言红 ✓。
#[test]
fn lod_cache_hit_identity() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let region = json!({"x": 0, "y": 0, "w": 400, "h": 300});
    let export = |workspace: &mut Workspace, lod: u64| -> serde_json::Value {
        let path = temp_path(&format!("cache_lod{lod}_{}", std::process::id()));
        let mut ctx = context(workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy(), "region": region.clone(), "lod": lod }),
        )
    };
    let stats_before = workspace.region_cache_stats("doc_png").unwrap();
    let first = export(&mut workspace, 1);
    assert_eq!(first["ok"], json!(true), "{first}");
    let first_bytes = std::fs::read(temp_path(&format!("cache_lod1_{}", std::process::id())))
        .expect("第一次导出应当落盘");
    let stats_first = workspace.region_cache_stats("doc_png").unwrap();
    let second = export(&mut workspace, 1);
    assert_eq!(second["ok"], json!(true), "{second}");
    let stats_second = workspace.region_cache_stats("doc_png").unwrap();
    // ① 第二次命中：hits +1 ✓
    assert_eq!(
        stats_second.hits,
        stats_first.hits + 1,
        "同 (region, lod) 二次导出必须命中（hits {} ⇒ {}）",
        stats_first.hits,
        stats_second.hits
    );
    // ② 告警一致 ✓ 且字节相同 ✓（命中返回的是同一条目 ✓）
    assert_eq!(
        first["warnings"], second["warnings"],
        "命中时的告警必须与第一次一致"
    );
    let second_bytes = std::fs::read(temp_path(&format!("cache_lod1_{}", std::process::id())))
        .expect("第二次导出应当落盘");
    assert_eq!(first_bytes, second_bytes, "命中必须返回同一份字节");
    // ③ 中间插一次 `lod: 0`：不同键 ⇒ 不命中（hits 不变 ✓），再导 `lod: 1` 又命中 +1 ✓
    let _zero = export(&mut workspace, 0);
    let stats_after_zero = workspace.region_cache_stats("doc_png").unwrap();
    assert_eq!(
        stats_after_zero.hits, stats_second.hits,
        "lod:0 的条目不得替 lod:1 命中（hits 应当不变）"
    );
    let _third = export(&mut workspace, 1);
    let stats_third = workspace.region_cache_stats("doc_png").unwrap();
    assert_eq!(
        stats_third.hits,
        stats_second.hits + 1,
        "第三次 lod:1 导出应当再次命中"
    );
    assert!(stats_before.hits <= stats_first.hits, "首轮统计应当单调");
}

/// **期 4 像素等价（server 路）**：`lod: 1` 的导出 ≈ 全幅同倍率 bilinear 下采样，
/// 最大单通道差 ≤ 1（设计 §2 E1 验收 ② ✓）。
///
/// **夹具**：`setup` 的矩形边全在偶数坐标（40/340/40/240）⇒ 点采样与 bilinear 在边处
/// 数学上一致（见 `crates/yanshi-render/tests/lod_render.rs` 文件头的取舍说明 ✓）。
/// 笔触/纹理等真实内容上的实测最大差见 8K 文档量测（包说明）。
#[test]
fn lod_pixel_equivalence_on_real_content() {
    use yanshi_core::resample::{resample_rgba, ResampleFilter};
    let mut workspace = workspace();
    setup(&mut workspace);
    let full_path = temp_path("equiv_full");
    let full = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": full_path.to_string_lossy(), "deterministic": true }),
        )
    };
    assert_eq!(full["ok"], json!(true), "{full}");
    let lod_path = temp_path("equiv_lod1");
    let lod = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": lod_path.to_string_lossy(), "lod": 1, "deterministic": true }),
        )
    };
    assert_eq!(lod["ok"], json!(true), "{lod}");
    let full_bytes = std::fs::read(&full_path).expect("全幅导出应当落盘");
    let lod_bytes = std::fs::read(&lod_path).expect("lod:1 导出应当落盘");
    let (fw, fh, full_rgba) =
        yanshi_render::png::decode_png(&full_bytes).expect("全幅应当是合法 PNG");
    let (lw, lh, lod_rgba) =
        yanshi_render::png::decode_png(&lod_bytes).expect("lod:1 应当是合法 PNG");
    assert_eq!((fw, fh), (800, 600));
    assert_eq!((lw, lh), (400, 300));
    let reference = resample_rgba(&full_rgba, fw, fh, lw, lh, ResampleFilter::Bilinear)
        .expect("下采样参照应可算");
    let max_diff = lod_rgba
        .iter()
        .zip(reference.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        max_diff <= 1,
        "lod:1 与 bilinear 下采样参照的最大单通道差是 {max_diff}，超出 ≤1 的验收"
    );
    let _ = std::fs::remove_file(&full_path);
    let _ = std::fs::remove_file(&lod_path);
}

/// **P-3（期 1）**：`export_png` 回执必须用一句话说清 `rendered_seq` 是预览水位 ✓
/// （旧行为：只给两个数字，调用方误读成"渲染不完整" ⇒ 白跑一遍重导 ✓）。
///
/// **变异**：删掉 `rendered_seq_note` 字段 ⇒ 本条红 ✓。
#[test]
fn export_png_receipt_names_rendered_seq_as_preview_watermark() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let path = temp_path("receipt-note");
    let exported = {
        let mut ctx = ToolContext::new(&mut workspace, "doc_png", "human:1", "session:test")
            .with_owner(true)
            .with_wait_for_render(false, 0);
        registry().call(
            &mut ctx,
            "export_png",
            &json!({ "path": path.to_string_lossy() }),
        )
    };
    assert_eq!(exported["ok"], json!(true), "{exported}");
    let note = exported["rendered_seq_note"].as_str().unwrap_or("");
    assert!(
        note.contains("预览水位") && note.contains("与本次导出完整性无关"),
        "回执必须说清 rendered_seq 是预览水位、与导出完整性无关：{exported}"
    );
    // 数字本身照旧：两个字段都在，语义由 note 锚定。
    assert!(
        exported["head_seq"].is_number() && exported["rendered_seq"].is_number(),
        "head_seq 与 rendered_seq 必须仍在回执里：{exported}"
    );
    let _ = std::fs::remove_file(&path);
}
