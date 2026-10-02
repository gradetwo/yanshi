//! **工具栏的每个按钮都必须有图标** ✓。
//!
//! **为什么有这条测试** ✓：渲染按钮时用的是
//! `TOOL_ICONS[key] || ""` ✓ ⇒ **某个工具没定义图标时，按钮会渲染成一个空 svg** ✗
//! —— 页面上就是**一个看不见、也点不到的按钮** ✗。
//! **这真的发生过** ✓：两轮前我加「标注」工具时**漏了它的图标** ✗ ⇒ 工具栏里多了一个空位 ✓
//!（我是靠"把工具表与图标表**交叉核对**"才发现的 ✓，不是靠看 ✓）。
//! **所以这条守卫是结构性的** ✓：以后**加工具就必须同时给图标** ✓，
//! 否则**测试直接红** ✓ —— 不靠记性 ✓（本项目自己的纪律 ✓）。

/// 从源码里取出一个以 `const NAME = {` 开头、以 `\n};` 结束的块 ✓。
fn block<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(&format!("const {name} "))
        .unwrap_or_else(|| panic!("找不到 {name} 的定义"));
    let rest = &source[start..];
    // **结束符可能是 `};` 也可能是 `];`** ✓ —— 数组与对象都合法 ✓。
    //
    // **这里踩过一个真坑** ✓：原来的实现只找 `\n};` ✗ ⇒ `TOOL_DEFS`（一个**数组** ✓，以 `];` 结束 ✓）
    // 会被一路读到**下一个 `\n};`** ✗ ⇒ 把后面**毫不相干的一大段**（含别的块、甚至注释里的中文串 ✓）
    // 也当成"工具表"来解析 ✓ ⇒ 于是从一句注释里捡出 `"读"` 当成工具名 ✓ ⇒ 报"工具没有图标" ✗，
    // 而真正的原因是**提取器圈错了范围** ✗。**它此前只是碰巧没出错** ✓。
    let end_object = rest.find("\n};");
    let end_array = rest.find("\n];");
    let end = match (end_object, end_array) {
        (Some(a), Some(b)) => a.min(b),
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => panic!("块应当以 `}};` 或 `];` 结束"),
    };
    &rest[..end]
}

#[test]
fn every_tool_button_has_an_icon() {
    // **直接读查看器源码** ✓（页面就是一段 Rust 原始字符串 ✓ ⇒ `include_str!` 即可 ✓）。
    let source = include_str!("../src/viewer.rs");
    let icons = block(source, "TOOL_ICONS");
    let tools = block(source, "TOOL_DEFS");

    // 图标表里的键 ✓（缩进两格的名字 ✓）。
    let mut have: Vec<String> = Vec::new();
    for line in icons.lines() {
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() != 2 {
            continue;
        }
        if let Some((name, _)) = trimmed.split_once(':') {
            let name = name.trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                have.push(name.to_owned());
            }
        }
    }
    assert!(
        have.len() > 10,
        "图标表解析出来只有 {} 个键 ⇒ 多半是解析写错了 ✗（本轮就栽过一次 ✓）",
        have.len()
    );

    // 工具表里的每一项 ✓：取其 `icon`（或退回到 `tool`/`id` ✓）。
    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for entry in tools.split('{').skip(1) {
        let entry = match entry.split('}').next() {
            Some(text) => text,
            None => continue,
        };
        let pick = |field: &str| -> Option<String> {
            let at = entry.find(&format!("{field}:"))?;
            let rest = &entry[at + field.len() + 1..];
            let start = rest.find('"')? + 1;
            let end = rest[start..].find('"')? + start;
            Some(rest[start..end].to_owned())
        };
        let key = pick("icon").or_else(|| pick("tool")).or_else(|| pick("id"));
        let Some(key) = key else { continue };
        checked += 1;
        if !have.iter().any(|name| name == &key) {
            let label = pick("label").unwrap_or_else(|| key.clone());
            missing.push(format!("{key}（{label}）"));
        }
    }
    assert!(checked > 10, "只检查到 {checked} 个工具 ⇒ 解析多半写错了 ✗");
    assert!(
        missing.is_empty(),
        "以下工具**没有图标** ✗ ⇒ 页面上会是空按钮 ✗：\n{}",
        missing.join("\n")
    );
}

/// **图标内容不能是空的** ✓（定义了键但值是空串，同样会是空按钮 ✗）。
#[test]
fn no_icon_is_empty() {
    let source = include_str!("../src/viewer.rs");
    let icons = block(source, "TOOL_ICONS");
    let mut empty: Vec<String> = Vec::new();
    for line in icons.lines() {
        let trimmed = line.trim_start();
        if line.len() - trimmed.len() != 2 {
            continue;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let value = value.trim().trim_end_matches(',');
        // 允许写 `''` 或 `""` ✓ 之外的任何内容 ✓。
        if value == "''" || value == "\"\"" {
            empty.push(name.to_owned());
        }
    }
    assert!(
        empty.is_empty(),
        "以下图标是空串 ✗ ⇒ 同样是空按钮 ✗：{empty:?}"
    );
}
