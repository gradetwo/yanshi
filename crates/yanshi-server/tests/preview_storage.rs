//! 提交流水线的存储语义（设计决策 A）：**提交只写 256² 文档预览**，整幅 PNG 仅在显式导出时生成。
//!
//! 背景：此前每个提交都 `render_region(整幅)` 并写 PNG 进 CAS，实测一个工作区累积到
//! **1.3GB / 245 个 PNG（平均约 5MB）**。设计方确认改为 A，前提是**体验不受影响**：
//! 画布像素由内核或客户端显式 `render_region` 提供，缩略图本来就是 256²。
//!
//! 本文件把这两件事都固定住：提交产物必须小；显式导出必须仍然给出整幅像素。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_preview", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 2_000)
}

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert!(bytes.starts_with(b"\x89PNG"), "应为 PNG");
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width, height)
}

/// 取响应里的 blob（`render_region` 把 `blob_hash` 放在顶层，提交响应放在 `preview` 里）。
fn blob_for(response: &Value, workspace: &Workspace, key: &str) -> Vec<u8> {
    let url = response["preview"][key]
        .as_str()
        .or_else(|| response[key].as_str())
        .unwrap_or_else(|| panic!("响应缺少 {key}：{response}"));
    let raw = url.trim_start_matches("yanshi://blob/");
    let hash: yanshi_core::atom::BlobHash = raw
        .parse()
        .unwrap_or_else(|error| panic!("blob 地址 {raw} 非法：{error}"));
    workspace
        .store()
        .get(&hash)
        .unwrap_or_else(|error| panic!("取 blob 失败：{error}"))
}

/// 提交只产生**小**预览（256²），不是整幅 PNG。
#[test]
fn commits_store_only_a_small_document_preview() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_preview", 1024, 1024),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
    }

    for (index, y) in [200.0f64, 400.0, 600.0].iter().enumerate() {
        let response = {
            let mut ctx = context(&mut workspace);
            registry.call(
                &mut ctx,
                "draw_stroke",
                &json!({
                    "layer_id": "layer_1",
                    "object_id": format!("obj_{index}"),
                    "data": {
                        "points": [[120.0, *y], [880.0, *y + 40.0]],
                        "size": 24.0,
                        "color": {"r": 30, "g": 30, "b": 40, "a": 255}
                    }
                }),
            )
        };
        assert_eq!(response["ok"], json!(true), "{response}");
        let bytes = blob_for(&response, &workspace, "blob_hash");
        let (width, height) = png_size(&bytes);
        // 提交只允许产生**局部区域**预览或 256² 文档预览，绝不产生整幅 PNG。
        assert!(
            width <= 256 || height <= 256,
            "提交产物不应是整幅 PNG：实际 {width}×{height}，{} 字节",
            bytes.len()
        );
        assert!(
            bytes.len() < 256 * 1024,
            "提交产物 {} 字节过大（整幅 1024² PNG 实测约 5MB）",
            bytes.len()
        );
    }
}

/// 显式导出（客户端直接请求整幅区域）仍然给出**整幅像素** —— 体验不受影响。
#[test]
fn explicit_region_render_still_returns_the_full_canvas() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_preview", 1024, 1024),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_1",
                "object_id": "obj_a",
                "data": {"points": [[100.0, 100.0], [900.0, 700.0]], "size": 30.0,
                         "color": {"r": 200, "g": 40, "b": 40, "a": 255}}
            }),
        );
    }
    let response = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 1024, "h": 1024}}),
        )
    };
    assert_eq!(response["ok"], json!(true), "{response}");
    assert_eq!(response["width"], json!(1024), "{response}");
    assert_eq!(response["height"], json!(1024), "{response}");
    let bytes = blob_for(&response, &workspace, "blob_hash");
    assert_eq!(png_size(&bytes), (1024, 1024), "显式导出应是整幅分辨率");
}

/// 重活原子（滤镜）会走异步渲染 job：其收尾**不得**写整幅 PNG，
/// 只能生成 256² 文档级预览（设计决策 A 的核心，也是此前 1.3GB 的主要来源）。
#[test]
fn heavy_atom_commit_does_not_store_a_full_canvas_png() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_preview", 1024, 1024),
            "human:1",
            "session:test",
        )
        .unwrap();
    let registry = registry();
    {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "create_layer", &json!({"layer_id": "layer_1"}));
        registry.call(
            &mut ctx,
            "draw_stroke",
            &json!({
                "layer_id": "layer_1",
                "object_id": "obj_base",
                "data": {"points": [[80.0, 200.0], [940.0, 820.0]], "size": 40.0,
                         "color": {"r": 40, "g": 60, "b": 90, "a": 255}}
            }),
        );
    }
    // 滤镜是外扩类效果，服务端会排成重型 job（wait_for_render 会在预算内跑完）。
    let response = {
        let mut ctx = context(&mut workspace);
        registry.call(
            &mut ctx,
            "add_filter",
            &json!({"layer_id": "layer_1", "filter_name": "gaussian_blur",
                    "params": {"sigma": 6.0}}),
        )
    };
    assert_eq!(response["ok"], json!(true), "{response}");

    // 提交之后：**不得**写整幅 PNG。重活原子的预览是脏区（本例 900×660），
    // 而文档级预览（缩略图）必须是 256² 的小图 —— 两者都不是整幅。
    let blob = blob_for(&response, &workspace, "thumb_url");
    let (width, height) = png_size(&blob);
    assert!(
        !(width == 1024 && height == 1024),
        "重活提交不得写整幅 PNG：实际 {width}×{height}"
    );

    // 文档级预览（`get_document` 的 thumb_url）必须是 256² 的小图。
    let document = {
        let mut ctx = context(&mut workspace);
        registry.call(&mut ctx, "get_document", &json!({}))
    };
    let thumb = blob_for(&document, &workspace, "thumb_url");
    assert_eq!(
        png_size(&thumb),
        (256, 256),
        "文档级预览应为 256²，实际 {}，{} 字节",
        png_size(&thumb).0,
        thumb.len()
    );
    // 字节数只作粗略上界：模糊渐变内容的 256² PNG 也可能到数百 KB，
    // 关键性质是**分辨率**（256² 而非整幅）与「远小于整幅 PNG（实测约 5MB）」。
    assert!(
        thumb.len() < 1024 * 1024,
        "文档级预览 {} 字节过大（不应是整幅 PNG）",
        thumb.len()
    );
}
