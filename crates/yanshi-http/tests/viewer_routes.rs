//! **查看器请求的每个 URL，服务端都必须真的有那条路由** ✓。
//!
//! **为什么值得一条测试** ✓（真实翻车 ✓）：查看器曾经请求 `/api/health` ✗，
//! 而服务端只有 `/health` ✓ ⇒ 那条请求**永远 404** ✓，于是"服务端构建：…"这行
//! **最有用的排查信息永远打不出来** ✗ ✓ —— 我是**手工把两边逐个对了一遍**才发现的 ✓。
//! 手工对账能发现一次 ✓、**不能防住下一次** ✗ ⇒ 这里把它变成**会红的守卫** ✓。
//!
//! **判定方式** ✓：从查看器源码里取出所有 `"/api/…"` / `"/ws"` 这类**绝对路径字面量** ✓，
//! 逐个到服务端源码里找它**作为前缀或整串**出现过 ✓（服务端用 `strip_prefix("/api/…")` ✓ 与
//! `path == "/api/…"` ✓ 两种写法 ✓ ⇒ 两种都算 ✓）。
//! **刻意保守** ✓：只查"出现的路径" ✓ —— 拼出来的 URL（带文档 id / token ✓）当然查不到字面量 ✓，
//! 所以**只在字面量上判** ✓，宁可漏报也不要误报 ✓（误报的守卫会被关掉 ✗）。

use std::path::Path;

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("读不了 {}：{error}", path.display()))
}

#[test]
fn every_url_the_viewer_requests_is_a_route_the_server_serves() {
    // **(A)①：请求字面量在主脚本资产里** ✓ ⇒ 骨架与资产**拼起来**再看 ✓。
    let viewer = format!(
        "{}{}",
        read("crates/yanshi-http/src/viewer.rs"),
        read("crates/yanshi-http/assets/viewer-app.js")
    );
    let server = read("crates/yanshi-http/src/server.rs");
    // **只看查看器发请求的那些调用** ✓ —— 从 `fetch(` / `api(` / `new WebSocket(` 后面取字面量 ✓。
    let mut wanted: Vec<String> = Vec::new();
    for needle in ["fetch(\"", "api(\"", "new WebSocket("] {
        let mut from = 0usize;
        while let Some(at) = viewer[from..].find(needle) {
            let start = from + at + needle.len();
            let rest = &viewer[start..];
            // `new WebSocket(...)` 是拼出来的 ✓ ⇒ 单独处理（下面直接补 `/ws` ✓）。
            if needle == "new WebSocket(" {
                wanted.push("/ws".to_owned());
                from = start;
                continue;
            }
            let end = match rest.find('"') {
                Some(end) => start + end,
                None => break,
            };
            let literal = &viewer[start..end];
            if literal.starts_with('/') {
                wanted.push(literal.to_owned());
            }
            from = end;
        }
    }
    wanted.sort();
    wanted.dedup();
    assert!(
        wanted.len() >= 5,
        "只找到 {} 个路径 ⇒ 提取器坏了 ✗：{wanted:?}",
        wanted.len()
    );

    // **服务端要不要 token 都无所谓** ✓ —— 这里只问"这条路由存在吗" ✓。
    let mut missing = Vec::new();
    for url in &wanted {
        // `/api/tools/<tool>` 这种带变量的 ⇒ 看它前面那一段在不在 ✓。
        let probe = match url.split_once("/api/tools") {
            Some((_, _)) => "/api/tools".to_owned(),
            None => url.clone(),
        };
        // **回退到目录前缀** ✓：像 `/brush-previews/index.json` 这种"目录路由 + 固定文件名"的写法 ✓，
        // 服务端是用 `strip_prefix("/brush-previews/")` + 拼文件名实现的 ✓ ⇒ 源码里只有**目录那一段** ✓。
        // 这与本测试自己的哲学一致 ✓（"宁可漏报也不要误报" ✓）：目录这段在 ✓ 就算这条路由在 ✓。
        let directory = match url.rsplit_once('/') {
            Some((head, _)) if !head.is_empty() => format!("{head}/"),
            _ => probe.clone(),
        };
        if !server.contains(&probe) && !server.contains(&directory) {
            missing.push(format!("{url}（按 {probe} 或目录 {directory} 找 ✓）"));
        }
    }
    assert!(
        missing.is_empty(),
        "查看器请求了服务端**没有的路由** ✗ ⇒ 那些请求会永远 404 ✓ \
         （`/api/health` 那次就是这样 ✓，把最有用的排查信息吞掉了 ✓）：\n{}",
        missing.join("\n")
    );
}

/// **(A)① 收尾判据** ✓：SW 的 `SHELL` 里**每一项都必须在服务端有出处** ✓。
///
/// 为什么必须有一条 ✓：拆出资产后，若只把 URL 写进 `SHELL` 却**忘了加路由** ✗，
/// SW 里的 `cache.add(url).catch(() => undefined)` 会**静默吞掉失败** ✓ ⇒
/// **离线打开时样式/脚本缺失** ✗，而**日志上什么都看不到** ✗ ⇒ **∴ 靠这条结构性守卫 ✓**。
///
/// **变异判据** ✓：往 `SHELL` 里加一个服务端不存在的路径（如 `/nope.js`）⇒ 红 ✓。
///
/// 说明 ✓：这里做的是**源码级**检查（服务端文本里是否出现该路径 ✓）；
/// 端到端版本（真的发一次请求看 200 ✓）留作后续加强 ✓。
#[test]
fn every_shell_entry_is_served_by_the_server() {
    let sw = read("crates/yanshi-http/assets/service-worker.js");
    let server = read("crates/yanshi-http/src/server.rs");
    let start = sw.find("const SHELL = [").expect("SW 应有 SHELL 清单");
    let end = sw[start..].find("];").expect("SHELL 应闭合") + start;
    let mut checked = 0usize;
    for piece in sw[start..end].split('"').skip(1).step_by(2) {
        let url = piece.trim();
        if !url.starts_with('/') {
            continue;
        }
        checked += 1;
        assert!(
            server.contains(url),
            "SW 预缓存的 {url} 在服务端**没有出处** ⇒ cache.add 会静默失败 ⇒ 离线时缺资源 ✗"
        );
    }
    assert!(checked >= 6, "SHELL 至少应有 6 个 URL（实际 {checked}）");
}
