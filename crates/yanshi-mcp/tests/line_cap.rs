//! **MCP 的 stdio 循环必须给每行设上限**（第 739 轮；外部审计 S-08）
//!
//! **⇒ 缺陷**：`reader.lines()` **无上限** ✗ ⇒ 一个**不带换行**的超大输入就能让 MCP 进程
//!   把内存吃光 ✓（HTTP 层早有 64 KiB 的 `read_line_bounded` ✓ —— **一道门有锁、一道没有** ✓）。
//!
//! **⇒ 判据**：喂一段**超过上限、且没有换行**的输入 ⇒ `serve` 必须**返回 Err**，
//!   且**错误文案说清上限** ✓（不是静默截断、也不是读到内存耗尽 ✓）。
//! **变异**：改回 `reader.lines()` ⇒ 它会把整段读进内存并**正常返回** ⇒ 第一条红 ✓。
use std::io::Cursor;
use yanshi_mcp::{serve, McpOptions};

#[test]
fn a_line_beyond_the_cap_is_refused_with_the_limit_named() {
    // 上限是 64 KiB ⇒ 给 256 KiB 且**不带换行** ✓
    let mut input = vec![b'x'; 256 * 1024];
    input.push(b'\n');
    let mut output: Vec<u8> = Vec::new();
    let result = serve(Cursor::new(input), &mut output, McpOptions::default());
    let error = result.expect_err("超长行必须被拒绝（否则一行就能把内存吃光）");
    let text = error.to_string();
    assert!(
        text.contains("上限") && text.contains("65536"),
        "错误文案要说清上限（否则用户不知道能发多大）：{text}"
    );
}

#[test]
fn normal_lines_still_work() {
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}\n";
    let mut output: Vec<u8> = Vec::new();
    serve(
        Cursor::new(input.to_vec()),
        &mut output,
        McpOptions::default(),
    )
    .expect("正常的一行必须照常处理");
    let text = String::from_utf8_lossy(&output);
    assert!(text.contains("jsonrpc"), "没有回包：{text}");
}
