//! C1-b 判据（期 2）：预览档与全档的渲染缓存必须分键——
//! `preview_and_full_have_distinct_cache_keys`。同一份 DNG 字节以
//! 全档与预览档（raw_options.preview_max_edge）各导入一个对象，
//! 先后渲染：misses 恰为 2、两条目的尺寸不同；再渲全档应命中
//! （键稳定，不是靠永远 miss 装分开）。feature `raw` 开时编译。
#![cfg(feature = "raw")]

use serde_json::{json, Value};
use std::path::PathBuf;
use yanshi_core::Bbox;
use yanshi_raw::dng_fixture::{write_synthetic_dng, SyntheticDng};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("yanshi_c1b_{}_{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, "doc_c1b", "human:1", "session:test")
        .with_owner(true)
        .with_wait_for_render(true, 4_000);
    registry().call(&mut ctx, tool, &args)
}

fn object_data(workspace: &Workspace, object_id: &str) -> Value {
    let state = workspace.document("doc_c1b").unwrap().state().clone();
    state
        .objects
        .get(object_id)
        .map(|object| object.data.clone())
        .unwrap_or(Value::Null)
}

#[test]
fn preview_and_full_have_distinct_cache_keys() {
    let root = temp_dir("keys");
    let mosaic: Vec<u16> = (0..240 * 160)
        .map(|i| 600 + ((i * 37) % 14000) as u16)
        .collect();
    let bytes = write_synthetic_dng(&SyntheticDng {
        width: 240,
        height: 160,
        mosaic,
        ..SyntheticDng::default()
    });
    std::fs::write(root.join("shot.dng"), &bytes).unwrap();

    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建");
    workspace
        .create_document(
            NewDocument::new("doc_c1b", 480, 160),
            "human:1",
            "session:test",
        )
        .unwrap();
    let created = call(&mut workspace, "create_layer", json!({"layer_id": "L"}));
    assert_eq!(created["ok"], json!(true), "{created}");

    let absolute = root.join("shot.dng").to_string_lossy().into_owned();
    let put = call(
        &mut workspace,
        "put_blob",
        json!({"path": absolute, "mime_type": "image/x-dcraw"}),
    );
    assert_eq!(put["ok"], json!(true), "{put}");
    let blob_hash = put["blob_hash"].as_str().unwrap().to_owned();

    // 全档对象（左）与预览档对象（右，cap 64）各一份。
    let full = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_full",
            "bitmap": {"blob_hash": blob_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 240.0, "h": 160.0},
        }),
    );
    assert_eq!(full["ok"], json!(true), "{full}");
    let preview = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_preview",
            "bitmap": {"blob_hash": blob_hash, "size": bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 240.0, "y": 0.0, "w": 240.0, "h": 160.0},
            "raw_options": {"preview_max_edge": 64},
        }),
    );
    assert_eq!(preview["ok"], json!(true), "{preview}");

    // 落账档位：全档记录无档位、预览记录记 64（分键的事实来源）。
    let full_record = &object_data(&workspace, "raw_full")["raw_develop"];
    assert_eq!(
        full_record["developed_max_edge"],
        Value::Null,
        "{full_record}"
    );
    let preview_record = &object_data(&workspace, "raw_preview")["raw_develop"];
    assert_eq!(
        preview_record["developed_max_edge"],
        json!(64),
        "{preview_record}"
    );

    let stats = |workspace: &Workspace| workspace.document("doc_c1b").unwrap().bitmap_cache_stats();

    // 导入的同步渲染（wait_for_render）已把两个对象各取过一次：
    // 键若不分开，第二个对象的取图会命中第一个的条目 ⇒ misses=1、
    // entries=1、字节只有一份。分键成立时三者恰为两份各自的账。
    let settled = stats(&workspace);
    assert_eq!(
        settled.misses, 2,
        "两对象各 miss 一次（键相撞时第二个会命中 ⇒ misses=1）：{settled:?}"
    );
    assert_eq!(settled.entries, 2, "应有两条独立条目：{settled:?}");
    assert_eq!(
        settled.bytes,
        240 * 160 * 12 + 64 * 42 * 12,
        "两条目字节 = 全档整幅 + 预览 64×42 线性载荷：{settled:?}"
    );

    // 再渲两区：都应命中既有条目（键稳定，不是靠永远 miss 装分开）。
    workspace
        .render_region_raw("doc_c1b", Bbox::new(0.0, 0.0, 240.0, 160.0))
        .unwrap();
    workspace
        .render_region_raw("doc_c1b", Bbox::new(240.0, 0.0, 240.0, 160.0))
        .unwrap();
    let after_again = stats(&workspace);
    assert_eq!(after_again.misses, settled.misses, "再渲不许新增 miss");
    assert!(
        after_again.hits >= settled.hits + 2,
        "两区再渲应各命中一次：{after_again:?} vs {settled:?}"
    );
}
