//! 守卫：**仓库里调用到的工具名必须都真实存在于注册表**。
//!
//! 起因：我在一天之内因为**凭印象写工具名**而浪费了四次排查 ——
//! `atom.id`（实际 `atom_id`）、`effect.name`（实际 `adjustment_type`/`filter_name`）、
//! `yanshi://blob/<hash>`（实际响应里已改写成 `/api/blob/<hash>`）、
//! `apply_adjustment`（实际不存在，应为 `add_adjustment`）✗。
//! 前三次是"猜响应字段"，第四次是"猜工具名" ✓ —— 后者可以在仓库内**自动守住** ✓。
//!
//! 因此本测试扫描查看器与浏览器检查脚本里的工具名调用，逐一核对注册表 ✓。
//! 名字写错会在 `cargo test` 阶段就被拦住，而不是等到线上探针返回
//! 「未知工具 xxx」才被发现 ✓。

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crates/yanshi-http 应位于仓库内")
        .to_path_buf()
}

/// 从源码文本里提取工具名调用（`callTool("x"` / `yanshiCallTool("x"` / `"tool": "x"`）。
fn tool_names_in(source: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for marker in ["callTool(\"", "yanshiCallTool(\"", "\"tool\": \""] {
        let mut rest = source;
        while let Some(index) = rest.find(marker) {
            let after = &rest[index + marker.len()..];
            let name: String = after
                .chars()
                .take_while(|ch| ch.is_ascii_lowercase() || *ch == '_')
                .collect();
            if !name.is_empty() {
                names.insert(name);
            }
            rest = after;
        }
    }
    names
}

#[test]
fn every_tool_name_used_in_the_repo_exists_in_the_registry() {
    let root = repo_root();
    let registered: BTreeSet<String> = yanshi_server::ToolRegistry::full()
        .tools()
        .iter()
        .map(|tool| tool.name.to_owned())
        .collect();
    assert!(registered.len() > 27, "注册表应包含核心层之外的扩展工具");

    // **来源就是"页面源代码"** ✓ —— (A)① 之后**主脚本是独立资产** ✓，而 `viewer.rs` 只剩骨架 ✗
    // ⇒ **∴ 骨架里已无任何工具调用 ✓** ⇒ 把它留在列表里会让"每个来源都应有工具调用"这条断言误报 ✗
    //（本轮实测：`viewer.rs 里应至少调用一个工具` ✓）⇒ **∴ 只列真正的页面源 ✓**。
    let sources = [
        root.join("crates/yanshi-http/assets/viewer-app.js"),
        root.join("scripts/browser-ui-check.mjs"),
    ];
    let mut checked = 0usize;
    for path in sources {
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("读取 {} 失败：{error}", path.display()));
        let names = tool_names_in(&text);
        assert!(!names.is_empty(), "{} 里应至少调用一个工具", path.display());
        for name in names {
            checked += 1;
            assert!(
                registered.contains(&name),
                "{} 调用了未注册的工具 `{name}` —— 工具名必须与注册表一致（凭印象写名字是已犯过四次的错误）",
                path.display()
            );
        }
    }
    assert!(checked >= 10, "应检查到足够多的工具名（实际 {checked}）");
}

/// 反向守卫：查看器里**以字面量出现**的工具名必须仍然存在（避免名字被改乱后测试空转）。
///
/// 注意：多数工具（`fill`、`create_mask`、`move_object` 等）是由 `state.tool` **变量**传入的，
/// 因此这里只断言确实以字面量出现的那些 ✓；动态名字来自内核工具目录与 `state.tool`，
/// 由 `every_tool_name_used_in_the_repo_exists_in_the_registry` 覆盖字面量部分 ✓。
#[test]
fn the_viewer_still_calls_its_literal_tools() {
    let root = repo_root();
    let mut text = std::fs::read_to_string(root.join("crates/yanshi-http/src/viewer.rs"))
        .expect("应能读取查看器骨架");
    // **(A)①：主脚本已是独立资产** ✓ ⇒ 字面量工具名要连它一起看 ✓（**只看骨架会漏掉全部工具调用** ✗）。
    text.push_str(
        &std::fs::read_to_string(root.join("crates/yanshi-http/assets/viewer-app.js"))
            .expect("应能读取查看器主脚本资产"),
    );
    let names = tool_names_in(&text);
    for expected in [
        "draw_stroke",
        "create_layer",
        "import_image",
        "list_objects",
        "get_log",
    ] {
        assert!(
            names.contains(expected),
            "查看器应以字面量调用 `{expected}`（实际调用集合：{names:?}）"
        );
    }
}
