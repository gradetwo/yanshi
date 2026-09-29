//! 文档与代码一致性：效果算子清单不得与内核常量静默分叉。
//!
//! 背景：此前多轮手工同步文档时，有一次"同步"其实是**空操作**（目标文本并不存在），
//! 但结论被当成已完成。用测试固定这类断言，比依赖人工仔细更可靠。

use std::path::PathBuf;

use yanshi_render::{ADJUSTMENT_NAMES, FILTER_NAMES};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate 应位于 <repo>/crates/yanshi-render")
        .to_path_buf()
}

fn read(name: &str) -> String {
    let path = repo_root().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("读取 {path:?} 失败：{error}"))
}

/// 每个调整/滤镜名字都要出现在两份 README 与设计实现说明里。
#[test]
fn every_effect_name_is_documented() {
    let docs = [
        ("README.md", read("README.md")),
        ("README.zh-CN.md", read("README.zh-CN.md")),
        (
            "docs/design/implementation-notes.md",
            read("docs/design/implementation-notes.md"),
        ),
    ];
    let mut missing = Vec::new();
    for name in ADJUSTMENT_NAMES.iter().chain(FILTER_NAMES.iter()) {
        for (label, text) in &docs {
            if !text.contains(name) {
                missing.push(format!("{label} 缺少 {name}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "文档与内核常量不一致（共 {} 处）：\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// 实现说明里的总数行必须与常量实际长度一致。
#[test]
fn documented_effect_counts_match_the_kernel() {
    let notes = read("docs/design/implementation-notes.md");
    let line = notes
        .lines()
        .find(|line| line.contains("调整 **") && line.contains("种** / 滤镜 **"))
        .unwrap_or_else(|| {
            panic!(
                "未找到总数行（应为 `调整 **N 种** / 滤镜 **M 种**` 形式）：\
                 数字若不再被文档声明，本测试就失去了意义"
            )
        });
    let numbers: Vec<usize> = line
        .split(|character: char| !character.is_ascii_digit())
        .filter(|chunk| !chunk.is_empty())
        .filter_map(|chunk| chunk.parse().ok())
        .collect();
    assert_eq!(
        numbers.len(),
        2,
        "总数行应恰有两个数字（调整、滤镜）：{line}"
    );
    assert_eq!(
        numbers[0],
        ADJUSTMENT_NAMES.len(),
        "文档声明的调整数（{}）与内核不符（{}）：{line}",
        numbers[0],
        ADJUSTMENT_NAMES.len()
    );
    assert_eq!(
        numbers[1],
        FILTER_NAMES.len(),
        "文档声明的滤镜数（{}）与内核不符（{}）：{line}",
        numbers[1],
        FILTER_NAMES.len()
    );
}

/// 两份 README 的算子清单应逐项对应：README 里出现的每个内核名字都要在中文版里也出现。
#[test]
fn readmes_agree_on_the_effect_inventory() {
    let english = read("README.md");
    let chinese = read("README.zh-CN.md");
    let mut missing = Vec::new();
    for name in ADJUSTMENT_NAMES.iter().chain(FILTER_NAMES.iter()) {
        if english.contains(name) && !chinese.contains(name) {
            missing.push(format!("README.zh-CN.md 缺少 {name}"));
        }
        if chinese.contains(name) && !english.contains(name) {
            missing.push(format!("README.md 缺少 {name}"));
        }
    }
    assert!(
        missing.is_empty(),
        "两份 README 不一致：\n{}",
        missing.join("\n")
    );
}
