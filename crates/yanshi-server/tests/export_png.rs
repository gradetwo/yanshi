//! **`export_png`** ✓ —— 任意尺寸、**真的落盘** ✓（真实用户报的 §P2-6 ✓）。
//!
//! **原症状** ✓：`render_region` 的 `include_image` **上限 512px** ✗，超了只回
//! `yanshi://blob/...` ✗ —— **伪协议** ✓，进程外取不到 ✗ ⇒ 导出这一步**没有出口** ✗。
//! **本测试用"自己的解码器读回来"证明它是真 PNG** ✓，而不是只看返回的字节数 ✓。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

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
            "路径写不进去",
            json!({"path": "/proc/definitely/not/writable/x.png"}),
            "写文件失败",
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
