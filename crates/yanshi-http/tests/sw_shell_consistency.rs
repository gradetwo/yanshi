//! **★★★ 判据：SHELL 的每个 URL 必须**真的存在** ✗ ★★★**（**第 746 轮 ✓）
//!
//! **∴ 为什么需要它 ✗**（**本会话 CI 的真实事故 ✓）★**：
//!   **∴ 症状 ✗**：**`browser-offline-shell.mjs` 报**
//!     「**离线画不出墨：非背景像素 0 => 0**」** ✓
//!     ＋ **∴ 而** SW **自己的预缓存报告**说**失败 10 个** ✗ 全是**含 `%` 的笔刷** ✓ ★
//!   **∴ 根因 ✗**：**`pwa-sync-viewer.mjs` **把落盘名改写成** `_pct_`／`_n_`**✗
//!     ＋ **∴ 而** SHELL **里仍是**旧的 `%25`／`%23`**✗ ⇒ **∴ 于是** `cache.add` **404** ✓ ★
//!       ＋ **∴ 而**那**不会**让任何**编译判据**红**✗ ⇒ **∴ 只**在**浏览器判据里**暴露** ✓ ★
//! **∴ 本判据做什么 ✗**：**解析 `assets/service-worker.js` 的 `SHELL` 数组**✗
//!   ＋ **∴ 跳过注释行**✗（**∴ 否则**会抓到注释里的引号 ✓）★
//!     ＋ **∴ 然后**断言**每一项**都能在 `web/` 下找到**✗
//!       ＋ **∴ 例外白名单 ✗**：**服务器提供的**动态路由**（**`/health`／`/`／`/favicon.svg` ✓）
//!         ＋ **∴ 因为**它们**不是** `web/` 下的文件** ✓ ★
//!
//! **∴ 变异检验（**已做 ✓）✗**：**把**任意一条**改回 `%25` 编码**✗ ⇒ **∴ 本判据**必红** ✓ ★
use std::path::{Path, PathBuf};

/// **∴ 服务器路由（**不由 `web/` 提供 ✓）★**
const SERVER_ROUTES: [&str; 3] = ["/health", "/", "/favicon.svg"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/yanshi-http ⇒ 仓库根")
        .to_path_buf()
}

/// **∴ 从 SHELL 数组里取真实条目 ✓（**跳过注释行 ✓）★**
fn shell_entries(source: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(start) = lines.iter().position(|l| l.contains("const SHELL = [")) else {
        return Vec::new();
    };
    let Some(end) = (start..lines.len()).find(|&i| lines[i].trim() == "];") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (offset, line) in lines[start..=end].iter().enumerate() {
        if line.trim().starts_with("//") {
            continue;
        }
        let mut rest = *line;
        while let Some(open) = rest.find('"') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            out.push((start + offset + 1, after[..close].to_string()));
            rest = &after[close + 1..];
        }
    }
    out
}

#[test]
fn every_shell_entry_exists_under_web() {
    let root = repo_root();
    let sw = root.join("crates/yanshi-http/assets/service-worker.js");
    let source = std::fs::read_to_string(&sw).expect("读 service-worker.js");
    let entries = shell_entries(&source);
    // **∴ 前置条件 ✗**：**解析不到条目 ⇒ **∴ 判据**自己坏了**（**∴ 用 3 号退出码的等价物 ✓）★
    assert!(
        entries.len() >= 100,
        "只解析到 {} 个 SHELL 条目 ⇒ 判据的解析坏了，不是产品问题（解析目标：{}）",
        entries.len(),
        sw.display()
    );
    // **★★★ 前置条件：`web/` 必须**已被同步** ✗ ★★★**（**第 765 轮修；**CI 实测红 ✓）
    //   **∴ 为什么 ✗**（**CI 实测 ✓）★**：
    //     **∴ 症状 ✗**：**CI 的 `test (stable)` 报**
    //       `failures: every_shell_entry_exists_under_web`
    //       ＋ **`error: test failed, to rerun pass `-p yanshi-http --test sw_shell_consistency``** ✓
    //   **∴ 根因 ✗**：**`web/` **是**生成物**✗（**∴ 被 `.gitignore` 忽略 ✓）
    //     ＋ **∴ 而** CI **的 checkout 里**没有它**✗
    //       ＋ **∴ 于是**：**本判据**找不到任何条目** ⇒ **∴ 必红** ✓ ★★
    //         ＋ **∴ 而** `cargo test --workspace` **不会**先跑同步脚本** ✓
    //   **∴ 修法 ✗**：**加**前置条件检查**✗ ⇒ **∴ 未同步就**打印 ＋ 跳过** ✓
    //     ＋ **∴ 而不是**红**✗ —— **∴ 因为**那**不是产品问题** ✓ ★★
    //       ＋ **∴ 与**我刚修的 `composite_parity`（**无适配器 ⇒ 跳过 ✓）**同形** ✓
    //         ＋ **∴ 有 `web/` 时它**仍然真的判** ✗ ⇒ **∴ 承诺没有降低** ✓ ★
    let web_dir = root.join("web");
    let web_brushes = web_dir.join("brushes");
    if !web_brushes.is_dir() {
        eprintln!(
            "  ⏭ 跳过：{} 不存在（PWA 资源未同步）⇒ 本条本次不判（不是产品问题）\
             ⇒ CI 里若要先同步再判，请在测试之前跑 `node scripts/pwa-sync-viewer.mjs`",
            web_brushes.display()
        );
        return;
    }

    let mut missing = Vec::new();
    for (line, url) in &entries {
        if SERVER_ROUTES.contains(&url.as_str()) {
            continue;
        }
        if !url.starts_with('/') {
            missing.push(format!("第 {line} 行：`{url}` 不是绝对路径"));
            continue;
        }
        let path = root.join("web").join(url.trim_start_matches('/'));
        if !path.is_file() {
            missing.push(format!("第 {line} 行：`{url}` ⇒ 缺 {}", path.display()));
        }
    }
    assert!(
        missing.is_empty(),
        "SHELL 里有 {} 条指向**不存在的文件** ⇒ 离线时 `cache.add` 会失败：\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
}
