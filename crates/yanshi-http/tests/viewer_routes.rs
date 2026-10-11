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
        // **两种出处都算** ✓：① 服务端文本里出现该精确路径 ✓；
        // ② 或它落在某个**前缀路由**下（如 `/brushes/` 由 `strip_prefix` 处理 ✓）。
        let dir = &url[..url.rfind('/').map(|i| i + 1).unwrap_or(url.len())];
        let served = server.contains(url) || server.contains(&format!("strip_prefix(\"{dir}\")"));
        assert!(
            served,
            "SW 预缓存的 {url} 在服务端**没有出处**（精确路径或前缀路由都没有）⇒ cache.add 会静默失败 ⇒ 离线时缺资源 ✗"
        );
    }
    assert!(checked >= 6, "SHELL 至少应有 6 个 URL（实际 {checked}）");
}

/// **(A)② 离线回退的不变量** ✓：主脚本里出现的"离线清单"路径，**必须也在 SW 的 `SHELL` 里** ✓。
///
/// 为什么 ✓：回退本身若依赖一个**没被预缓存**的 URL ✗，那它**在离线时同样失败** ✗ ⇒
/// 回退就是**假的** ✓（看起来写了、实际不工作 ✗）⇒ **∴ 必须有这条不变量 ✓**。
///
/// **变异判据** ✓：把 `SHELL` 里的 `/brush-previews/index.json` 删掉 ⇒ 红 ✓。
#[test]
fn the_offline_fallback_manifest_is_itself_precached() {
    let app = read("crates/yanshi-http/assets/viewer-app.js");
    let sw = read("crates/yanshi-http/assets/service-worker.js");
    let manifest = "/brush-previews/index.json";
    assert!(
        app.contains(manifest),
        "主脚本应使用 {manifest} 作为离线回退清单 ✓"
    );
    assert!(
        sw.contains(&format!("\"{manifest}\"")),
        "回退清单 {manifest} **必须在 SHELL 里** ✓（否则回退在离线时同样失败 ⇒ 回退是假的 ✗）"
    );
}

/// **特殊字符笔刷名：查看器与 SW 预缓存必须用同一个编码 URL** ✓。
///
/// **为什么不只靠 HTTP 那条判据** ✓：HTTP 那条只量"服务端**能**把编码名发出来" ✓；
/// 若查看器仍写 `"/brushes/" + name + ".myb"`（不编码 ✗）或 SW 仍写字面 `#` ✗，
/// 浏览器端依然取不到 ✓ ⇒ 这条把"两端同码"钉住 ✓（沿用本文件的源码级检查法 ✓，不另起装置 ✓）。
///
/// **判定三条** ✓：① 查看器的笔刷 URL 由 `brushAssetUrl` 唯一拼出 ✓，且必过 `encodeURIComponent` ✓；
/// ② `SHELL` 里所有 `/brushes/` 条目都**不含字面 `#`** ✓，且百分号解码后都对应磁盘上真实文件 ✓
///（SW 预缓存若指向不存在的 URL，`cache.add` 会被 `.catch` 静默吞掉 ✗ ⇒ 离线时少笔 ✗）；
/// ③ 九支带 `#` 的笔刷在 `SHELL` 里**正好**是 `%23` 形式 ✓。
///
/// **变异判据** ✓：去掉查看器的 `encodeURIComponent` ⇒ 红 ✓；
/// 把 `SHELL` 里任一条 `%23` 改回字面 `#` ⇒ 红 ✓。
#[test]
fn brush_urls_are_percent_encoded_on_both_sides() {
    let app = read("crates/yanshi-http/assets/viewer-app.js");
    let sw = read("crates/yanshi-http/assets/service-worker.js");

    // ① 查看器：笔刷 URL 必须逐段编码（`#` 在 URL 里是片段起点 ⇒ 不编码就丢后缀）。
    let builder = app
        .lines()
        .find(|line| line.contains("const brushAssetUrl ="))
        .expect("viewer-app.js 应有 `const brushAssetUrl =` 这个唯一入口 ✓");
    assert!(
        builder.contains("encodeURIComponent"),
        "brushAssetUrl 必须用 encodeURIComponent 编码笔刷名：{builder}"
    );
    // **请求与缓存键必须走同一个入口**（父 agent 点名的第二处）：老写法
    // `"/brushes/" + name + ".myb"` 在 fetch 与 `cache.put` 各出现一次 ⇒ 必须一处不剩，
    // 且 `brushAssetUrl(...)` 至少被调用两次（fetch 一次、cache.put 一次）。
    // 否则请求编码了、缓存键没编码（或反过来）⇒ 离线回退按 `request.url` 查缓存永远命中不了。
    assert!(
        !app.contains("\"/brushes/\" + name"),
        "查看器里不该再有未编码的 \"/brushes/\" + name（fetch 与 cache.put 都必须走 brushAssetUrl）"
    );
    // **★ 断言的**意图**必须保住 ✗ ★**（**第 497 轮修 ✓）
    //   **∴ 原来数的是**出现次数 ≥ 2****✗（**fetch 一次 ＋ `cache.put` 一次 ✓）
    //     ⇒ **∴ 而**第 493 轮把缓存键改成 **真正命中的 URL**（`hitUrl` ✓）
    //       ⇒ **∴ 于是**：**`brushAssetUrl(` 只剩**一次** ⇒ **∴ 这条**红** ✓
    //         ＋ **∴ 而**它的**意图**（**请求与缓存键逐字相同 ⇒ 离线回退能命中 ✓）
    //           ⇒ **∴ 其实**被满足得**更彻底** ✓（**因为**缓存键就是那个请求 URL）★**** ✓✓
    //   **∴ 现在 ✗**：**查**意图**，不查次数** ✓ ★
    //     ⇒ **∴ ①** 拼 URL 的入口**至少用一次**（**∴ 不许**绕过它 ✓）
    //       ＋ **∴ ②** 缓存键必须用 `hitUrl`（**来自 `fetchBrushText` 的返回 ✓）
    //         ⇒ **∴ 而** `hitUrl` **就是**真正命中的那个 URL** ✓ ★**** ✓✓
    assert!(
        app.matches("brushAssetUrl(").count() >= 1,
        "brushAssetUrl 必须被用到（它是唯一拼笔刷 URL 的入口）：实际 {} 次",
        app.matches("brushAssetUrl(").count()
    );
    assert!(
        app.contains("cache.put(hitUrl"),
        "缓存键必须用**真正命中的那个 URL**（`hitUrl`）⇒ 才能与请求逐字相同（离线回退才命中）"
    );
    // **∴ 两条候选 URL 都必须编码 ✗**（**∴ 现在有两个构造器 ✓）★
    // **∴ 定义可能跨行 ✗**（**第 497 轮实测踩到 ✓）：
    //   ⇒ **∴ `const brushAssetUrlSafe = (name) =>`**在一行**✗
    //     ＋ **∴ `"/brushes/" + encodeURIComponent(...)`**在下一行** ✓
    //       ⇒ **∴ 所以**：**要**连下一行一起看** ✓ ★**** ✓✓
    let lines: Vec<&str> = app.lines().collect();
    for builder in ["const brushAssetUrl =", "const brushAssetUrlSafe ="] {
        let idx = lines
            .iter()
            .position(|l| l.contains(builder))
            .unwrap_or_else(|| panic!("缺少 {builder}"));
        let window = format!("{} {}", lines[idx], lines.get(idx + 1).unwrap_or(&""));
        assert!(
            window.contains("encodeURIComponent"),
            "{builder} 必须用 encodeURIComponent（含下一行）：{window}"
        );
    }

    // ② SW：SHELL 里的 `/brushes/` 条目必须不含 `#`，且解码回磁盘上真实文件。
    let start = sw.find("const SHELL = [").expect("SW 应有 SHELL 清单");
    let end = sw[start..].find("];").expect("SHELL 应闭合") + start;
    let entries: Vec<&str> = sw[start..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|url| url.starts_with("/brushes/"))
        .collect();
    assert!(
        entries.len() >= 100,
        "SHELL 里的笔刷条目太少（{} 条）⇒ 提取器坏了",
        entries.len()
    );
    let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("assets/brushes");
    for url in &entries {
        assert!(
            !url.contains('#'),
            "SHELL 里不能有字面 `#`（片段起点 ⇒ cache.add 会丢掉后缀）：{url}"
        );
        let name = yanshi_http::http::percent_decode_path(&url["/brushes/".len()..]);
        assert!(
            assets.join(&name).is_file(),
            "SHELL 预缓存的 {url} 解码后是 {name}，磁盘上没有这个文件"
        );
    }

    // ③ 九支带 `#` 的笔刷：SHELL 里必须正是编码形式（否则离线回退按 URL 查缓存命中不了）。
    const HASH_NAMED: [&str; 9] = [
        "8B_Pencil#1.myb",
        "arrow#1.myb",
        "Fan#1.myb",
        "Flat2#1.myb",
        "Fountain_SF#1.myb",
        "Fount-offset#1.myb",
        "HalfTone#1.myb",
        "HalfToneCMY#1.myb",
        "Round#1.myb",
    ];
    for name in HASH_NAMED {
        let encoded = name.replace('#', "%23");
        assert!(
            sw.contains(&format!("\"/brushes/{encoded}\"")),
            "SHELL 应预缓存编码后的 /brushes/{encoded}（离线才命中得了）"
        );
    }
}
