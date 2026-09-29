//! 工具清单防漂移：README/实现说明里声明的工具数量必须与注册表实际一致。
//!
//! 背景：README 曾长期写着「27 core tools / 49 tools in total」，而 Phase 4b 陆续新增的
//! 建议类工具从未回填文档 —— 实际已达 64 个。数字写进文档却不被校验，就会变成谎言。
//! 本测试把「文档声明的数字」与「注册表的真实数字」绑在一起。

use std::path::PathBuf;

use yanshi_server::{Profile, ToolRegistry};

/// HTTP 服务端默认启用的 profile 集（与 `yanshi-http` 的默认一致）。
const DEFAULT_PROFILES: [Profile; 6] = [
    Profile::Core,
    Profile::History,
    Profile::Retouch,
    Profile::Annotation,
    Profile::Collab,
    Profile::Structure,
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crate 应位于 <repo>/crates/yanshi-server")
        .to_path_buf()
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("读取 {path:?} 失败：{error}"))
}

/// 文档里声明的工具数量（`N core tools` / `N tools in total`，以及中文版）。
/// 文档必须用锚点声明数量（数字在锚点**之前**，便于解析）：
/// * 核心：`27 core tools`
/// * 总数：`65 tools in total`
fn declared_counts(text: &str) -> Option<(usize, usize)> {
    let core = find_first(text, &["core tools", "个核心工具"])?;
    let total = find_first(text, &["tools in total", "个工具。"])?;
    Some((core, total))
}

fn find_first(text: &str, needles: &[&str]) -> Option<usize> {
    for needle in needles {
        if let Some(index) = text.find(needle) {
            // 向前扫描最近的数字（允许中间夹空格/括号等非数字字符，如 "(49 tools in total"）。
            let prefix = &text[..index];
            let digits: String = prefix
                .chars()
                .rev()
                .skip_while(|character| !character.is_ascii_digit())
                .take_while(|character| character.is_ascii_digit())
                .collect();
            if let Ok(value) = digits.chars().rev().collect::<String>().parse() {
                return Some(value);
            }
        }
    }
    None
}

#[test]
fn registry_has_no_duplicate_or_undocumented_tools() {
    let registry = ToolRegistry::with_profiles(&DEFAULT_PROFILES);
    let mut names: Vec<String> = registry
        .tools()
        .iter()
        .map(|spec| spec.name.to_owned())
        .collect();
    let total = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), total, "工具名重复注册");
    for spec in registry.tools() {
        assert!(
            spec.summary.len() >= 8,
            "工具 {} 的 summary 过短，不利于 Agent 选择：{:?}",
            spec.name,
            spec.summary
        );
        for param in spec.params {
            assert!(
                param.description.len() >= 4,
                "工具 {} 的参数 {} 缺少说明",
                spec.name,
                param.name
            );
        }
    }
    println!("注册表：{total} 个工具");
    for profile in DEFAULT_PROFILES {
        let count = ToolRegistry::with_profiles(&[profile]).tools().len();
        println!("  profile {}: {count} 个", profile.as_str());
    }
}

#[test]
fn documented_tool_counts_match_the_registry() {
    let registry = ToolRegistry::with_profiles(&DEFAULT_PROFILES);
    let actual_total = registry.tools().len();
    let actual_core = ToolRegistry::with_profiles(&[Profile::Core]).tools().len();
    println!("实际：core {actual_core} 个 / 全部 {actual_total} 个");

    // 数量声明放在 docs/tools.md（README 保持精简）。
    // 注意：不要写成 `for relative in ["docs/tools.md"]` —— 新版 clippy 的
    // `single_element_loop` 会把它判为错误（CI 的 stable 比本地新时才会出现）。
    let relative = "docs/tools.md";
    let text = read(relative);
    let (core, total) = declared_counts(&text)
        .unwrap_or_else(|| panic!("{relative} 未声明工具数量（测试因此失去意义）"));
    assert_eq!(
        (core, total),
        (actual_core, actual_total),
        "{relative} 声明的工具数量与注册表不符（core {core} vs {actual_core}，\
         全部 {total} vs {actual_total}）"
    );
}
