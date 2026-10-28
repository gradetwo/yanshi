//! **MCP 的 stdio 循环必须给每行设上限**（第 739 轮；外部审计 S-08）
//!
//! **⇒ 缺陷**：`reader.lines()` **无上限** ✗ ⇒ 一个**不带换行**的超大输入就能让 MCP 进程
//!   把内存吃光 ✓（HTTP 层早有 64 KiB 的 `read_line_bounded` ✓ —— **一道门有锁、一道没有** ✓）。
//!
//! **⇒ 判据（P-1，paint-fix-p3 改）**：喂一段**超过上限**的输入 ⇒ `serve` 必须**返回 Ok**，
//!   stdout 有一条**点名 65536 上限**的 JSON-RPC 错误回执（码 -32600，id 是 null），
//!   下一行 ping 仍有响应（进程存活 ✓）；上限本身保持 64 KiB 不变 ✓。
//!   排空本身另设 1 MiB 丢弃上限：行尾一直不来、丢弃量越过该上限时才返回 Err。
//! **变异**：改回 `reader.lines()` ⇒ 它会把整段读进内存并**正常返回** ⇒ 第一条红 ✓；
//!   把超限改回 `return Err` ⇒ 第一条红（serve 返回 Err）✓；
//!   删掉错误回执 ⇒ 第一条红（没有错误行）✓；
//!   把丢弃上限改大 ⇒ 第三条红（不再返回 Err）✓。
use std::io::Cursor;
use yanshi_mcp::{serve, McpOptions};

#[test]
fn a_line_beyond_the_cap_is_answered_with_an_error_and_the_server_survives() {
    // 上限是 64 KiB ⇒ 给 256 KiB 且**带换行**，之后再发一行 ping ✓
    let mut input = vec![b'x'; 256 * 1024];
    input.push(b'\n');
    input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n");
    let mut output: Vec<u8> = Vec::new();
    let result = serve(Cursor::new(input), &mut output, McpOptions::default());
    assert!(
        result.is_ok(),
        "超长行之后 serve 应返回 Ok 而不是杀进程：{result:?}"
    );
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "超长行一条错误回执 + ping 一条响应：{text}");
    let error: serde_json::Value =
        serde_json::from_str(lines[0]).expect("第一行是 JSON-RPC 错误回执");
    assert_eq!(error["jsonrpc"], "2.0");
    assert_eq!(
        error["id"],
        serde_json::Value::Null,
        "超限行的 id 读不到，回执 id 只能是 null"
    );
    assert_eq!(error["error"]["code"], -32600, "超限行是非法请求：{error}");
    let message = error["error"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("65536") && message.contains("拆成更小的 batch"),
        "错误文案要说清 65536 上限并指路拆成更小的 batch：{message}"
    );
    let ping: serde_json::Value = serde_json::from_str(lines[1]).expect("第二行是 ping 响应");
    assert_eq!(ping["id"], 9, "超长行之后 ping 仍有响应：{ping}");
    assert_eq!(ping["result"], serde_json::json!({}));
}

#[test]
fn a_line_beyond_the_cap_without_newline_is_discarded_too() {
    // 100 KiB 且**不带换行**、在 1 MiB 丢弃上限内 ⇒ 到 EOF 时同样只丢该行，不杀进程。
    let input = vec![b'x'; 100 * 1024];
    let mut output: Vec<u8> = Vec::new();
    let result = serve(Cursor::new(input), &mut output, McpOptions::default());
    assert!(
        result.is_ok(),
        "无行尾的超长行也应只丢行不杀进程：{result:?}"
    );
    let text = String::from_utf8(output).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "只应有一条错误回执：{text}");
    assert!(
        lines[0].contains("65536") && lines[0].contains("拆成更小的 batch"),
        "错误回执要点名上限并指路拆分：{text}"
    );
}

#[test]
fn a_line_without_newline_past_the_discard_cap_still_fails() {
    // 64 KiB 上限 + 1 MiB 丢弃上限都越过、仍无行尾 ⇒ 返回 Err（旧行为只留给这种病态输入）。
    // 长度算法：65,537 字节越过行上限（第 65,537 个字节让 bytes.len() > 65536），
    // 再 1,048,577 字节越过丢弃上限 ⇒ 输入共 64 KiB + 1 MiB + 2 字节。
    let input = vec![b'x'; 64 * 1024 + 1024 * 1024 + 2];
    let mut output: Vec<u8> = Vec::new();
    let result = serve(Cursor::new(input), &mut output, McpOptions::default());
    let error = result.expect_err("丢弃量越过 1 MiB 仍无行尾时必须返回 Err");
    let text = error.to_string();
    assert!(
        text.contains("上限") && text.contains("65536"),
        "错误文案要说清上限：{text}"
    );
    assert!(output.is_empty(), "丢弃上限越过时不应已写出回执");
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
