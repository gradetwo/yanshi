//! **★ 单层渲染的区域缓存必须带**图层 scope**✗ ★**（**第 247 轮 ✓）
//!
//! **∴ 为什么要有这个文件 ✗**：`region_cache` 的键原来是 **（区域, 版本）**✗
//!   ＋ **∴ 它**不含图层**✗ ⇒ **∴ 一旦**给"只画一层"那条路接上缓存**✗
//!     ⇒ **∴ 就可能**把别的图层的像素当成这一层的返回** ✓（**只在命中时发作 ✓ 最难查的一类 ✓）
//!       ⇒ **∴ 所以**第 247 轮把 `scope` 加进了 `BlockKey` ✓
//!   ＋ **∴ 而本文件**把三件事钉住**✗（**∴ 每一件**都能红 ✓）★**：
//!     **∴ ① 同一层同一区域连渲两次 ⇒ **字节相同**✗ 且 **命中数自增** ✓
//!       （**语义计数 ✗ 不看墙钟 ✓ —— 与 `tool-below-reuse.mjs` 同一套纪律 ✓）★
//!     **∴ ② 换一层渲同一区域 ⇒ **绝不能**等于第一层的字节** ✓
//!       （**变异：把 `scope` 从键里去掉 ⇒ 命中第一层的缓存 ⇒ 必红 ✓）★★
//!     **∴ ③ 改动该层内容后再渲 ⇒ **字节必须变** ✓
//!       （**变异：把版本从内容指纹换回"永不失效"⇒ 必红 ✓）★

use serde_json::{json, Value};
use yanshi_core::Bbox;
use yanshi_server::{DocumentSettings, NewDocument, Profile, ToolContext, ToolRegistry, Workspace};

/// **跑完即删临时目录** ✓（第 817 轮；`/tmp` 是 1.9 GB tmpfs ✓，夹具留着会写满 ✓）
struct Cleanup(std::path::PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const DOC: &str = "doc_scope";

fn call(workspace: &mut Workspace, tool: &str, args: Value) -> Value {
    let mut ctx = ToolContext::new(workspace, DOC, "human:1", "session:test").with_owner(true);
    ToolRegistry::with_profiles(&Profile::ALL).call(&mut ctx, tool, &args)
}

fn stroke(
    workspace: &mut Workspace,
    object: &str,
    layer: &str,
    colour: Value,
    points: Value,
) -> Value {
    call(
        workspace,
        "brush_stroke",
        json!({
            "layer_id": layer,
            "object_id": object,
            "brush": "100%_Opaque",
            "size": 60,
            "color": colour,
            "points": points,
            "preview": false,
        }),
    )
}

/// **落盘工作区 ＋ 仓库里的 `assets/`**✓（**同 `brush_stroke.rs` 的做法 ✓）**：
/// **∴ 必须指资产目录 ✗** —— 否则**一支笔刷都解析不出来**✗
///   （**∴ 实测**：`找不到 brush「100%_Opaque.myb」⇒ 共 0 支` ✓）★
fn workspace() -> (Workspace, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("yanshi_scope_{}", std::process::id()));
    let _cleanup_1 = Cleanup(root.clone());
    let _ = std::fs::remove_dir_all(&root);
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let mut workspace = Workspace::with_file_store(root.clone(), DocumentSettings::default())
        .expect("落盘工作区应当能建")
        .with_assets_dir(Some(assets));
    workspace
        .create_document(NewDocument::new(DOC, 400, 300), "human:1", "session:test")
        .unwrap();
    (workspace, root)
}

#[test]
fn single_layer_region_cache_is_scoped_by_layer() {
    let (mut workspace, root) = workspace();

    // **两层、两种颜色** ✓ —— **∴ 这样"串层"一眼就能看出来** ✓（**而不必逐像素比 ✓）★
    for (layer, colour) in [
        ("A", json!({ "r": 200, "g": 40, "b": 40, "a": 255 })),
        ("B", json!({ "r": 40, "g": 40, "b": 200, "a": 255 })),
    ] {
        assert_eq!(
            call(&mut workspace, "create_layer", json!({ "layer_id": layer }))["ok"],
            json!(true)
        );
        let made = stroke(
            &mut workspace,
            &format!("o_{layer}_first"),
            layer,
            colour,
            json!([[60.0, 60.0, 1.0], [200.0, 120.0, 1.0]]),
        );
        assert_eq!(made["ok"], json!(true), "画一笔应当成功：{made}");
    }

    let bbox = Bbox::new(0.0, 0.0, 256.0, 200.0);
    let first = workspace
        .render_region_raw_layer(DOC, bbox, "A")
        .expect("A 的区域渲染应成功");
    let after_first = workspace.region_cache_stats(DOC).unwrap();

    let second = workspace
        .render_region_raw_layer(DOC, bbox, "A")
        .expect("A 的第二次区域渲染应成功");
    let after_second = workspace.region_cache_stats(DOC).unwrap();

    // **① 字节相同 ＋ 命中自增** ✓
    assert_eq!(
        first.2, second.2,
        "同一层同一区域连渲两次 ⇒ 字节必须逐字节相同（确定性 ✓）"
    );
    assert!(
        after_second.hits > after_first.hits,
        "第二次必须**命中**区域缓存（hits {} ⇒ {} ✗）",
        after_first.hits,
        after_second.hits
    );

    // **② 换一层 ⇒ 绝不能拿到第一层的字节** ✓（**这条是 scope 的存在理由 ✓）★
    let other = workspace
        .render_region_raw_layer(DOC, bbox, "B")
        .expect("B 的区域渲染应成功");
    assert_ne!(
        first.2, other.2,
        "★ 换一层渲同一区域 ⇒ 绝不能等于第一层的字节 ⇒ 键里必须有 scope ★"
    );

    // **③ 改动该层内容 ⇒ 版本（内容指纹）变 ⇒ 缓存必须失效** ✓
    let again = stroke(
        &mut workspace,
        "o_A_second",
        "A",
        json!({ "r": 10, "g": 220, "b": 10, "a": 255 }),
        json!([[40.0, 160.0, 1.0], [220.0, 160.0, 1.0]]),
    );
    assert_eq!(again["ok"], json!(true), "再画一笔应当成功：{again}");
    let changed = workspace
        .render_region_raw_layer(DOC, bbox, "A")
        .expect("改动后的区域渲染应成功");
    assert_ne!(
        first.2, changed.2,
        "改动该层内容后 ⇒ 缓存必须失效（版本是内容指纹 ✗ 不是「永不失效」 ✓）"
    );
    let _ = std::fs::remove_dir_all(&root);
}
