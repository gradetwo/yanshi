//! **仓库里任意两个路径都不许只差大小写** ✗（真实用户报告 ✓）。
//!
//! **为什么这是硬要求** ✓：**macOS 与 Windows 的文件系统大小写不敏感** ✓
//! ⇒ 两个只差大小写的路径在那些系统上会**互相覆盖** ✓ ⇒ 检出之后**静默少文件** ✗。
//! **用户点出的例子** ✓：`Pen.myb` / `pen.myb`、`Knife.myb` / `knife.myb` ✓
//!（在 `assets/brushes/` 里 ✓ —— 上游 mypaint-brushes 自己就不一致 ✓，四支是**不同的笔刷** ✓）。
//!
//! **为什么要有守卫** ✓：这种错**在 Linux 上完全看不出来** ✗ ——
//! 本地测试全绿 ✓、CI 全绿 ✓，直到有人用 macOS 检出才开始丢东西 ✓
//! ⇒ 只有**扫一遍**才能挡住 ✓（与"资产必须真的可用"那条守卫同理 ✓）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// **递归收集** ✓，跳过 `.git` 与 `target` ✓（那是构建产物 ✓，不属于仓库内容 ✓）。
fn collect(dir: &Path, taken: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "target" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect(&path, taken);
        } else {
            taken.push(path);
        }
    }
}

/// **同一目录下，不许有两个文件名只差大小写** ✓。
///
/// **只看同一目录** ✓ 是有意的：`a/Pen.myb` 与 `b/pen.myb` **不冲突** ✓
///（路径整体不同 ✓），而 `a/Pen.myb` 与 `a/pen.myb` **在 macOS 上就是同一个文件** ✗。
#[test]
fn no_directory_holds_two_names_differing_only_by_case() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        repo.join("Cargo.toml").is_file(),
        "仓库根找不到：{}",
        repo.display()
    );
    let mut files = Vec::new();
    collect(&repo, &mut files);
    assert!(
        files.len() > 100,
        "只扫到 {} 个文件 ⇒ 守卫没测到东西 ✗",
        files.len()
    );

    // **按（所在目录的小写形式 + 文件名的小写形式）分组** ✓ —— 键相同的组就是冲突 ✓。
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for path in &files {
        let dir = path
            .parent()
            .map(|parent| parent.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        groups
            .entry((dir, name))
            .or_default()
            .push(path.to_string_lossy().into_owned());
    }
    let collisions: Vec<&Vec<String>> = groups.values().filter(|paths| paths.len() > 1).collect();
    assert!(
        collisions.is_empty(),
        "同一目录下有文件名只差大小写 ⇒ macOS / Windows 上会**互相覆盖** ✗：{collisions:#?}\n\
         （处理办法：改名 ✓ —— 见 assets/brushes/CASE-NOTES.md ✓）"
    );
}
