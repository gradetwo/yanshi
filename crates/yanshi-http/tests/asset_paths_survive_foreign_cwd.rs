//! **在仓库外启动 ⇒ 查看器的三类资产必须仍然能拿到** ✓（缺陷 F ✓）。
//!
//! **现象**：从 `~/` 之类的位置起 `yanshi-serve`，查看器的品牌图、**全部**笔刷缩略图、
//! **全部**纸纹**静默 404** ✗ —— 浏览器只留一行 console ⇒ 创作者看到的是「素材区是空的」，
//! 而**没有任何地方告诉他为什么** ✗（静默降级）。
//!
//! **根因**：同一个「资产目录」被两处读，而只有一处解析过 ✓：
//! 工具层走 `resolve_assets_dir`（会试 `current_exe` 旁边那几级布局 ✓）；
//! 而 `GET /brand/*`、`/brush-previews/*`、`/textures/*`、`/fonts/*` **直接读**
//! `options.assets_dir` / `options.brand_dir` ⇒ 那还是**相对当前工作目录**的缺省值 ✗。
//!
//! **为什么这条断言能红**（不是安慰剂 ✓）：集成测试的工作目录是 `crates/yanshi-http/` ✓，
//! 那里**有** `assets/`（放查看器的 js/css ✓）但**没有** `assets/brand` ✗ ⇒
//! 「相对缺省」在这个目录里**必然**找不到 ⇒ 未修的代码只能返回 404 ✓。
//! 下面第 ④ 段把这件事**断言下来** ✓：一旦有人把测试挪到仓库根跑 ⇒ ④ 先红 ⇒
//! 提醒他「①②③ 现在没有判别力了」，而不是让守卫**悄悄地变成永真** ✗。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;
use yanshi_http::server::{serve, HttpOptions};

fn get(addr: std::net::SocketAddr, target: &str) -> u16 {
    let mut stream = TcpStream::connect(addr).expect("连接服务端");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).expect("读完响应");
    // **只取状态行**：`HTTP/1.1 200 OK` 的第 2 个 token 才是码 ✓
    // （曾经误取第 3 个 ⇒ 读到 `OK` ⇒ 每个响应都 panic ✗ —— 工具自己也要能红 ✓）。
    let first_line = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    first_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or_else(|| panic!("响应头读不出状态码：{first_line:?}"))
}

#[test]
fn asset_routes_still_serve_when_the_process_starts_elsewhere() {
    // 缺省 options（`assets_dir = "assets"`、`brand_dir = "assets/brand"`）**故意不改** ✓：
    // 这条测试判的就是「按缺省启动」的那份行为 ✓。
    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        ..HttpOptions::default()
    })
    .expect("启动服务");
    let addr = handle.addr;

    // ① 品牌资源（查看器标题那个 mark 与 manifest 的 icon ✓）
    assert_eq!(
        200,
        get(addr, "/brand/svg/icon-light.svg"),
        "品牌图拿不到 ⇒ 查看器顶栏是空的"
    );
    // ② 笔刷缩略图索引（画笔下拉里每一行的缩略图 ✓）
    assert_eq!(
        200,
        get(addr, "/brush-previews/index.json"),
        "笔刷预览索引 404 ⇒ 全部笔刷没有缩略图"
    );
    // ③ 内置纸纹（素材浮窗「铺成背景」那一排 ✓）
    assert_eq!(
        200,
        get(addr, "/textures/Paper003.png"),
        "内置纹理 404 ⇒ 纸纹一排全空"
    );

    // ④ **负对照** ✓：证明上面三条不是「本来就通」——
    // 相对缺省在当前工作目录里**确实**不存在 ⇒ 未解析的代码只能读到不存在的路径 ✗。
    for relative in ["assets/brand", "assets/brush-previews", "assets/textures"] {
        assert!(
            !Path::new(relative).is_dir(),
            "{relative} 在当前工作目录里存在 ⇒ ①②③ 不再能区分「解析过」与「没解析」⇒ 这条守卫失去判别力"
        );
    }
}
