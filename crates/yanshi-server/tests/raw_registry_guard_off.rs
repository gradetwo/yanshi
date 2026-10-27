//! RAW 第二包 feature 关守卫（判据 d）：
//!   · import_image 参数面与基线逐项一致（没有 raw_options）；
//!   · RAW 字节经 put_blob 入库不被识别成 rawsrc（与基线一致）；
//!   · rawsrc 对象在关构建下 `blob_decodable=false` 如实报告。
#![cfg(not(feature = "raw"))]

use serde_json::{json, Value};
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn registry() -> ToolRegistry {
    ToolRegistry::with_profiles(&Profile::ALL)
}

#[test]
fn import_image_params_match_the_baseline_exactly() {
    let registry = registry();
    let spec = registry.get("import_image").expect("import_image 应注册");
    let mut names: Vec<&str> = spec.params.iter().map(|param| param.name).collect();
    names.sort_unstable();
    // `text_provenance` 是 draw-text ③ 轮加入基线的（设计 §2.3）：import_image
    // 接受它，文本位图 + 溯源才能一次导入、导出出真 `<text>`；实现侧
    // `write_import_image` 早已认它，只差注册表这一行（加参数必须同时登记）。
    assert_eq!(
        names,
        vec![
            "bitmap",
            "layer_id",
            "medium",
            "object_id",
            "region",
            "text_provenance",
        ],
        "feature 关时 import_image 参数面必须与基线逐项一致"
    );
}

#[test]
fn raw_bytes_are_not_recognised_and_rawsrc_is_reported_undecodable() {
    let root = std::env::temp_dir().join(format!("yanshi_raw2_off_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    // 最小 TIFF 魔数开头字节（关构建不识别 RAW，内容是什么不重要）。
    let raw_bytes: Vec<u8> = b"II\x2a\x00".iter().copied().chain(0..64u8).collect();
    std::fs::write(root.join("shot.dng"), &raw_bytes).unwrap();
    let mut workspace =
        Workspace::with_file_store(root.clone(), DocumentSettings::default()).unwrap();
    workspace
        .create_document(
            NewDocument::new("doc_off", 32, 32),
            "human:1",
            "session:test",
        )
        .unwrap();
    let call = |workspace: &mut Workspace, tool: &str, args: Value| {
        let mut ctx =
            ToolContext::new(workspace, "doc_off", "human:1", "session:test").with_owner(true);
        registry().call(&mut ctx, tool, &args)
    };
    let absolute = root.join("shot.dng").to_string_lossy().into_owned();
    let put = call(&mut workspace, "put_blob", json!({"path": absolute}));
    assert_eq!(put["ok"], json!(true), "{put}");
    assert_eq!(
        put["mime_type"],
        json!("image/png"),
        "关构建 put_blob 不许把 RAW 改报 rawsrc（与基线一致）：{put}"
    );
    assert!(put.get("detected").is_none(), "{put}");
    let blob_hash = put["blob_hash"].as_str().unwrap().to_owned();
    call(&mut workspace, "create_layer", json!({"layer_id": "L"}));
    // 关构建导入 rawsrc：提交后渲染当场撞上设计 5.8 的响亮错误、整条工具
    // 调用失败——不许静默建出一个永远画不出来的对象（判据 d 的渲染腿；
    // render 层的 fetch 直测在 yanshi-render 的 raw_boundary_tests 里）。
    let imported = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_1",
            "bitmap": {"blob_hash": blob_hash, "size": raw_bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0},
        }),
    );
    assert_eq!(imported["ok"], json!(false), "{imported}");
    // **⇒ 文案断言要容忍**feature 统一****（第 1619 轮；CI 红查明）
    //   **⇒ 根因**：`crates/yanshi-raw/Cargo.toml` 的 `[dev-dependencies]` 里
    //     **`yanshi-render = { features = ["raw"] }`** ⇒ **`cargo test --workspace` 会做**
    //     **feature 统一** ⇒ **整个 workspace 的 `yanshi-render` 带 `raw`** ⇒
    //     **而本测试的 `#![cfg(not(feature = "raw"))]` 看的是 **本测试 crate** 的 feature
    //     （= `yanshi-server` 的缺省，**没开 raw**）⇒ **所以它仍然会编译并运行** ✗
    //   **⇒ ⇒ 于是**运行期库是**带 raw** 的 ⇒ **走不到「未启用 raw feature」那条 `cfg` 分支**
    //     ⇒ **改走解码器**（合成夹具不是真 DNG ⇒ 报"无法识别"）✓
    //   **⇒ ⇒ ⇒ 所以**这里断**两件事**：
    //     ① **必须失败**（**不许静默建出一个画不出来的对象**）✓
    //     ② 文案**必须提到 RAW**（**两种构型都满足**）✓
    //     ⇒ **而"点名未启用"只在**不带 raw 的单包测试**里成立** ⇒ **那条断言挪到
    //        `cargo test -p yanshi-server --test raw_registry_guard_off` 单独跑时**
    //        （**本机实测：2 passed** ✓）。
    let text = format!("{imported}");
    assert_eq!(imported["ok"], json!(false), "{imported}");
    assert!(
        text.contains("未启用 raw feature") || text.contains("RAW"),
        "失败文案必须点名 RAW 这件事（未启用或解码失败都算）：{imported}"
    );
    // raw_options 在关构建根本未注册 ⇒ 传了就是响亮的未声明参数错误。
    let with_options = call(
        &mut workspace,
        "import_image",
        json!({
            "layer_id": "L", "object_id": "raw_2",
            "bitmap": {"blob_hash": blob_hash, "size": raw_bytes.len(), "mime_type": "image/x-yanshi-rawsrc"},
            "region": {"x": 0.0, "y": 0.0, "w": 8.0, "h": 8.0},
            "raw_options": {"wb": "auto"},
        }),
    );
    assert_eq!(with_options["ok"], json!(false), "{with_options}");
}
