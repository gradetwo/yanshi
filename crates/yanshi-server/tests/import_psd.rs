//! `import_psd` ✓（设计第 17 章的"PSD 只读导入" ✓）。
//!
//! **契约** ✓：只取**合成图** ✓ ⇒ 一个 `raster_patch` ✓；**不导入图层结构** ✗、**不写回 PSD** ✗。
//! 断言分两类 ✓：① **像素要对** ✓（用渲染器提供的写入器造一份 PSD ✓ ⇒ 往返比对 ✓）；
//! ② **不支持的输入要明确报错** ✗（绝不"画一半" ✓）。

use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_psd", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

fn setup(workspace: &mut Workspace) {
    workspace
        .create_document(
            NewDocument::new("doc_psd", 64, 64),
            "human:1",
            "session:test",
        )
        .unwrap();
    let mut ctx = context(workspace);
    registry().call(&mut ctx, "create_layer", &json!({"layer_id": "L"}));
}

fn pattern(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[
                (x * 13 % 256) as u8,
                (y * 31 % 256) as u8,
                ((x + y) * 7 % 256) as u8,
                255,
            ]);
        }
    }
    rgba
}

/// **导入成功 ⇒ 合成图的像素进入文档** ✓（用 `render_region` 在**文档空间**逐像素比对 ✓）。
#[test]
fn importing_a_psd_brings_in_its_composite() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let (width, height) = (24u32, 16u32);
    let rgba = pattern(width, height);
    let psd = yanshi_render::psd::encode_psd(width, height, &rgba, 3, 1);
    let blob = workspace
        .store()
        .put(&psd)
        .expect("PSD 入库应成功")
        .to_string();
    let imported = {
        let mut ctx = context(&mut workspace);
        registry().call(
            &mut ctx,
            "import_psd",
            &json!({
                "layer_id": "L", "object_id": "psd_1", "blob_hash": blob,
            }),
        )
    };
    assert_eq!(imported["ok"], json!(true), "{imported}");
    assert_eq!(imported["source"]["format"], json!("psd"), "{imported}");
    // **像素必须对** ✓：渲染整幅 ✓，与源 RGBA 逐像素比 ✓。
    let rendered = workspace
        .document_mut("doc_psd")
        .unwrap()
        .render_region_raw(yanshi_core::Bbox::new(0.0, 0.0, 64.0, 64.0))
        .expect("区域渲染应成功");
    for y in 0..height {
        for x in 0..width {
            let at = ((y * 64 + x) * 4) as usize;
            let source = ((y * width + x) * 4) as usize;
            assert_eq!(
                &rendered.2[at..at + 3],
                &rgba[source..source + 3],
                "({x},{y}) 的颜色应与 PSD 合成图一致 ✓"
            );
        }
    }
    // **合成图确实落了库** ✓（blob 先行的证据 ✓）。
    //
    // **从对象数据里取哈希** ✓，不猜工具返回值的字段路径 ✗ —— 我第一版猜 `imported["bitmap"]` ✗
    // ⇒ 取到空串 ⇒ 断言失败 ✓（同一类错误本会话已经犯过 ✓）。对象数据是**既有且稳定**的来源 ✓。
    let object_hash = {
        let state = workspace.document_mut("doc_psd").unwrap().state().clone();
        state
            .objects
            .get("psd_1")
            .map(|object| object.data.clone())
            .unwrap_or(serde_json::Value::Null)
    };
    let composite: yanshi_core::BlobHash = object_hash["bitmap"]["blob_hash"]
        .as_str()
        .unwrap_or_default()
        .parse()
        .expect("对象数据里应有合成图 blob");
    assert!(workspace.store().exists(&composite), "合成图应在存储里 ✓");
    assert_eq!(
        workspace.store().get(&composite).expect("应能取回").len(),
        (width * height * 4) as usize,
        "存的是原始 RGBA ✓"
    );
}

/// **不支持的一律明确报错** ✗ —— 且**原因要传到工具层的返回值里** ✓。
#[test]
fn unsupported_psd_is_refused_with_a_reason() {
    let mut workspace = workspace();
    setup(&mut workspace);
    let (width, height) = (4u32, 4u32);
    let rgba = pattern(width, height);
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "位深 16",
            {
                let mut psd = yanshi_render::psd::encode_psd(width, height, &rgba, 3, 0);
                psd[22..24].copy_from_slice(&16u16.to_be_bytes());
                psd
            },
            "位深",
        ),
        (
            "CMYK",
            {
                let mut psd = yanshi_render::psd::encode_psd(width, height, &rgba, 4, 0);
                psd[24..26].copy_from_slice(&4u16.to_be_bytes());
                psd
            },
            "色彩模式",
        ),
        (
            "版本 2（PSB）",
            {
                let mut psd = yanshi_render::psd::encode_psd(width, height, &rgba, 3, 0);
                psd[4..6].copy_from_slice(&2u16.to_be_bytes());
                psd
            },
            "版本",
        ),
        (
            "不是 PSD",
            b"definitely not a photoshop file".to_vec(),
            "签名",
        ),
    ];
    for (name, bytes, expected) in cases {
        let blob = workspace
            .store()
            .put(&bytes)
            .expect("入库应成功")
            .to_string();
        let refused = {
            let mut ctx = context(&mut workspace);
            registry().call(
                &mut ctx,
                "import_psd",
                &json!({
                    "layer_id": "L", "object_id": "psd_bad", "blob_hash": blob,
                }),
            )
        };
        assert_eq!(refused["ok"], json!(false), "{name} 应被拒绝：{refused}");
        let detail = refused["context"]["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains(expected),
            "{name} 的原因应提到「{expected}」，实测：{detail}"
        );
    }
}
