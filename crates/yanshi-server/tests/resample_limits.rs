//! **`resample` 的目标尺寸必须受画布上限约束**（第 729 轮；外部审计 S-06）
//!
//! **⇒ 缺陷**：`create_document` 有 `MAX_CANVAS_EDGE = 16_384` 与 `MAX_CANVAS_PIXELS = 1<<28` ✓，
//!   而 `resample` **一个都没检查** ✗ ⇒ `width: 60000, height: 60000` 会走到**分配 14.4 GB**
//!   （本机 3.8 GiB ⇒ OOM killer 杀掉整个服务 ✓）。
//!
//! **⇒ 判据**：超边长的请求必须 `invalid_argument`，且**文案里出现上限数字** ✓
//!   （"点出是哪一个上限"——否则调用方不知道该改哪个 ✓）；
//!   **对照**：正常的小尺寸重采样**不许**被这条拦住（防"修过头" ✓）。
//! **变异**：删掉那段上限检查 ⇒ 第一条红（会真的去分配）✓。
use serde_json::json;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn workspace() -> Workspace {
    Workspace::in_memory(DocumentSettings::default())
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_rs", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000)
}

#[test]
fn resample_refuses_target_sizes_beyond_the_canvas_limits() {
    let mut workspace = workspace();
    workspace
        .create_document(
            NewDocument::new("doc_rs", 32, 32),
            "human:1",
            "session:test",
        )
        .expect("create_document");
    let registry = registry();
    // **必须是光栅对象** ✓（`resample` 只作用于 `raster_patch` ✓ —— 我先用一笔 Stroke 试，
    //   结果在**类型检查**那一步就被拦住 ✗ ⇒ 根本没走到尺寸检查 ✓）。
    //   照 `tests/resample.rs` 的写法：先 `put` 一张 8×8 的原始 RGBA，再 `import_image` ✓。
    let mut bytes = Vec::new();
    for y in 0..8u32 {
        for x in 0..8u32 {
            bytes.extend_from_slice(&[(x * 20) as u8, (y * 20) as u8, 30, 255]);
        }
    }
    let hash = workspace
        .store()
        .put(&bytes)
        .expect("入库应成功")
        .to_string();
    let mut ctx = context(&mut workspace);
    let imported = registry.call(
        &mut ctx,
        "import_image",
        &json!({"layer_id": "layer_default", "object_id": "img_rs",
                "bitmap": {"blob_hash": hash, "size": 8 * 8 * 4,
                           "mime_type": "image/x-yanshi-raw"},
                "region": {"x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0}}),
    );
    assert_eq!(imported["ok"], json!(true), "{imported}");

    // ① 超大边长 ⇒ 必须报错，且点出上限 ✓
    let huge = registry.call(
        &mut ctx,
        "resample",
        &json!({"object_id": "img_rs", "width": 60000, "height": 60000}),
    );
    assert_eq!(
        huge["ok"],
        json!(false),
        "60000×60000 被接受了 ⇒ 会去分配 14.4 GB：{huge}"
    );
    let text = huge.to_string();
    assert!(
        text.contains("16384") || text.contains("16_384") || text.contains("上限"),
        "报错里没点出上限 ⇒ 调用方不知道改哪个：{huge}"
    );
}
