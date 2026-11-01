//! **import_image 的 bitmap 必须严格校验** ✓（报告问题 3）。
//!
//! 此前 bitmap 只要求是对象：缺 blob_hash 只补空串、全程不拒收 ⇒ 回执 ok:true、
//! blob_decodable:false、导出纯背景，调用方误判"导入底图不可用" ✗。
//! 修后：白名单 {blob_hash, size, mime_type, width, height} 之外的键 ⇒ 拒绝；
//! 缺 blob_hash 或为空 ⇒ 拒绝；put_blob → import_image 的正确流程不受影响 ✓。

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删** ✓（put_blob 要持久化存储 ⇒ 内存工作区写不了 blob，必须用文件工作区）。
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn workspace(root: &std::path::Path) -> Workspace {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    Workspace::with_file_store(root.to_path_buf(), DocumentSettings::default())
        .expect("工作区")
        .with_assets_dir(Some(assets))
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn context<'a>(workspace: &'a mut Workspace) -> ToolContext<'a> {
    ToolContext::new(workspace, "doc_import_val", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 8_000)
}

fn call(ctx: &mut ToolContext<'_>, name: &str, args: &Value) -> Value {
    registry().call(ctx, name, args)
}

fn detail_of(body: &Value) -> String {
    body.get("context")
        .and_then(|context| context.get("detail"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// 1×1 透明 PNG（put_blob 的 data_base64 夹具）。
const PNG_1X1_B64: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

fn setup_doc(name: &str) -> (Cleanup, Workspace) {
    let root =
        std::env::temp_dir().join(format!("yanshi_import_val_{name}_{}", std::process::id()));
    let cleanup = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建根");
    let mut workspace = workspace(&root);
    {
        // new_document 工具建文档并带默认图层（与 zero_ink_advice 同款写法）。
        let mut ctx = context(&mut workspace);
        call(
            &mut ctx,
            "new_document",
            &json!({"doc_id": "doc_import_val", "width": 200, "height": 200}),
        );
    }
    (cleanup, workspace)
}

/// **未知键必须拒绝** ✓：bitmap 里多一个 `path`（报告里误用的形状）⇒
/// 不能静默忽略，必须点名这个键报错。
#[test]
fn bitmap_with_unknown_key_is_rejected() {
    let (_cleanup, mut workspace) = setup_doc("unknown");
    let mut ctx = context(&mut workspace);
    let got = call(
        &mut ctx,
        "import_image",
        &json!({
            "layer_id": "layer_default",
            "bitmap": {"blob_hash": "abc", "size": 1, "mime_type": "image/png", "path": "/tmp/x.png"},
            "region": {"x": 0, "y": 0, "w": 1, "h": 1}
        }),
    );
    assert_eq!(
        got.get("ok").and_then(Value::as_bool),
        Some(false),
        "未知键必须拒绝：{got}"
    );
    assert_eq!(
        got.get("error_code").and_then(Value::as_str),
        Some("invalid_argument"),
        "未知键必须报 invalid_argument：{got}"
    );
    assert!(
        detail_of(&got).contains("path"),
        "报错必须点名未知键 path：{}",
        detail_of(&got)
    );
}

/// **缺 blob_hash 必须拒绝** ✓：只给白名单内的键、但没有 blob_hash ⇒
/// 不能补空串一路 ok:true；空串同样拒绝。
#[test]
fn bitmap_without_blob_hash_is_rejected() {
    let (_cleanup, mut workspace) = setup_doc("nohash");
    let mut ctx = context(&mut workspace);
    for bitmap in [
        json!({"size": 1, "mime_type": "image/png"}),
        json!({"blob_hash": ""}),
    ] {
        let got = call(
            &mut ctx,
            "import_image",
            &json!({
                "layer_id": "layer_default",
                "bitmap": bitmap,
                "region": {"x": 0, "y": 0, "w": 1, "h": 1}
            }),
        );
        assert_eq!(
            got.get("ok").and_then(Value::as_bool),
            Some(false),
            "缺 blob_hash 必须拒绝：{got}"
        );
        assert_eq!(
            got.get("error_code").and_then(Value::as_str),
            Some("invalid_argument"),
            "缺 blob_hash 必须报 invalid_argument：{got}"
        );
        assert!(
            detail_of(&got).contains("blob_hash"),
            "报错必须点名 blob_hash：{}",
            detail_of(&got)
        );
    }
}

/// **正确流程不受影响** ✓：put_blob → import_image（{blob_hash, size, mime_type}）⇒ 成功。
#[test]
fn bitmap_with_valid_put_blob_hash_imports() {
    let (_cleanup, mut workspace) = setup_doc("valid");
    let mut ctx = context(&mut workspace);
    let put = call(
        &mut ctx,
        "put_blob",
        &json!({"data_base64": PNG_1X1_B64, "mime_type": "image/png"}),
    );
    assert_eq!(
        put.get("ok").and_then(Value::as_bool),
        Some(true),
        "put_blob 应成功：{put}"
    );
    let hash = put
        .get("blob_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let size = put.get("size").and_then(Value::as_u64).unwrap_or(0);
    let got = call(
        &mut ctx,
        "import_image",
        &json!({
            "layer_id": "layer_default",
            "bitmap": {"blob_hash": hash, "size": size, "mime_type": "image/png"},
            "region": {"x": 0, "y": 0, "w": 1, "h": 1}
        }),
    );
    assert_eq!(
        got.get("ok").and_then(Value::as_bool),
        Some(true),
        "正确流程必须成功：{got}"
    );
}
