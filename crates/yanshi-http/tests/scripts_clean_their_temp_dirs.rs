//! **★★★ 判据：用 `mkdtempSync` 的脚本必须导入**共享清理模块** ✗ ★★★**（**第 763 轮 ✓）
//!
//! **∴ 为什么需要它 ✗**（**实测的系统性泄漏 ✓）★**：
//!   **∴ 证据 ✗**：**用户定期清理 `/tmp` 时**实测到**：
//!     ＋ **131 个 `gpuprobe-`／`gfp-` 目录**（**最老到 10 月 11 日 ✓）★★
//!       ＋ **∴ 而**普查后发现 ✗**：**全仓库 26 个脚本用 `mkdtempSync`**✗
//!         ＋ **∴ 其中**15 个**没有**任何清理** ✓（**≈33 处创建点 ✓）★★
//!   **∴ 根因 ✗**：**清理模板以前是**逐脚本复制的**✗
//!     ＋ **∴ 于是**：**新脚本**只能**靠作者记得** ✓ ★
//!       ＋ **∴ 所以 ✗**：**抽成 `scripts/lib/temp-dirs.mjs`**✗
//!         ＋ **∴ 并由**本判据**守住**：**导入它 ＝ 清理** ✓ ★★
//!
//! **∴ 判据做什么 ✗**：**扫 `scripts/**/*.mjs`**✗
//!   ＋ **∴ 找出**用了 `mkdtempSync` **的**✗
//!     ＋ **∴ 断言**它**同时**引用了 `temp-dirs.mjs`** ✓ ★
//!
//! **∴ 例外白名单 ✗**：**本模块自己**（**`scripts/lib/temp-dirs.mjs` ✓）
//!   ＋ **∴ 因为**它**不创建**目录**✗ 只**删除** ✓ ★
//!
//! **∴ 变异检验（**已做 ✓）✗**：**把任一已修脚本的 import 去掉**✗ ⇒ **∴ 必红** ✓ ★
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/yanshi-http ⇒ 仓库根")
        .to_path_buf()
}

fn collect_mjs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_mjs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("mjs") {
            out.push(path);
        }
    }
}

#[test]
fn every_script_that_makes_a_temp_dir_imports_the_cleanup_module() {
    let root = repo_root();
    let scripts = root.join("scripts");
    let mut files = Vec::new();
    collect_mjs(&scripts, &mut files);
    // **∴ 前置条件 ✗**：**扫到的文件太少 ⇒ **∴ 判据**自己坏了** ✓ ★
    assert!(
        files.len() >= 20,
        "只扫到 {} 个 .mjs ⇒ 判据的路径或遍历坏了，不是产品问题",
        files.len()
    );

    let mut offenders = Vec::new();
    let mut checked = 0usize;
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if !text.contains("mkdtempSync") {
            continue;
        }
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        // **∴ 白名单 ✗**：**模块自己**不创建**目录** ✓ ★
        if name.ends_with("scripts/lib/temp-dirs.mjs") {
            continue;
        }
        checked += 1;
        // **★★★ 两种清理机制**都算数 ✗ ★★★**（**第 763 轮修正 ✓）
        //   **∴ 为什么放宽 ✗**（**实测：我第一版太严 ✓）★**：
        //     **∴ 第一版 ✗**：**要求**必须导入 `scripts/lib/temp-dirs.mjs`** ✓
        //       ＋ **∴ 而**现实 ✗**：**有 **9 个脚本**早已有**自己的内联模板** ✓
        //         ＋ **∴ 即**：**它们**有 `__tempPaths` ＋ `trackTemp` ＋ `__cleanupTemp`
        //           ＋ **∴ 退出与信号钩子**✗ ⇒ **∴ 它们**确实在清理** ✓ ★★
        //             ＋ **∴ 所以**：**第一版**误报 9 个**✗ ⇒ **∴ 那**不是产品的缺陷** ✓
        //   **∴ 规则 ✗**：**有**任一**清理机制**即可** ✓
        //     ＋ **∴ ①** 导入共享模块**✗ ／ **∴ ②** 内联实现（`__cleanupTemp` 或 `__tempPaths`）★
        //       ＋ **∴ 而**共享模块**是**推荐路径**✗（**∴ 新脚本**用它 ⇒ **∴ 不会漏** ✓）★
        let imports_shared = text.contains("temp-dirs.mjs");
        let inline_cleanup = text.contains("__cleanupTemp") || text.contains("__tempPaths");
        if !imports_shared && !inline_cleanup {
            offenders.push(name);
        }
    }
    assert!(
        checked >= 10,
        "只有 {checked} 个脚本用了 mkdtempSync ⇒ 扫描逻辑可能错了"
    );
    assert!(
        offenders.is_empty(),
        "有 {} 个脚本用 mkdtempSync 却没导入 `scripts/lib/temp-dirs.mjs` ⇒ \
         它们的临时目录**永不清理**（实测曾累积 131 个）：\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}
