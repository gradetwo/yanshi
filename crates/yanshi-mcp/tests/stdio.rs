//! MCP stdio 端到端集成测试：真的把 `yanshi-mcp` 二进制当子进程跑起来。
//!
//! 覆盖：JSON-RPC 握手 → 建文档画一笔 → 区域渲染（内嵌图像）→ 轮询渲染状态 →
//! 落盘后在第二个进程里恢复。

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpProcess {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_yanshi-mcp"))
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("启动 yanshi-mcp");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{message}").expect("写请求");
        self.stdin.flush().expect("flush");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("读响应");
        assert!(!line.is_empty(), "服务器未响应 {method}");
        let response: Value = serde_json::from_str(line.trim()).expect("响应是 JSON");
        assert_eq!(response["id"], json!(id));
        assert!(
            response.get("error").is_none() || response["error"].is_null(),
            "{response}"
        );
        response["result"].clone()
    }

    fn notify(&mut self, method: &str) {
        let message = json!({"jsonrpc": "2.0", "method": method});
        writeln!(self.stdin, "{message}").expect("写通知");
        self.stdin.flush().expect("flush");
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let text = result["content"][0]["text"].as_str().unwrap_or("{}");
        serde_json::from_str(text).expect("工具文本是 JSON")
    }

    fn stop(mut self) {
        drop(self.stdin);
        let _ = self.child.wait();
    }
}

fn temp_root(tag: &str) -> std::path::PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-mcp-it-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

#[test]
fn stdio_handshake_draw_render_and_persist() {
    let root = temp_root("e2e");
    let root_str = root.to_string_lossy().into_owned();
    let mut process = McpProcess::start(&[
        "--root",
        &root_str,
        "--doc",
        "doc_it",
        "--width",
        "48",
        "--height",
        "48",
        "--profile",
        "core,annotation",
    ]);

    // 1) 握手。
    let init = process.request("initialize", json!({}));
    assert_eq!(init["protocolVersion"], "2024-11-05");
    assert_eq!(init["serverInfo"]["name"], "yanshi-mcp");
    process.notify("notifications/initialized");

    // 2) 工具清单按 profile 分层。
    let tools = process.request("tools/list", json!({}));
    let names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(names.contains(&"render_region"));
    assert!(names.contains(&"create_annotation"));
    assert!(!names.contains(&"inpaint_region"), "未实现的语义工具不暴露");

    // 3) 画一笔并拿到区域预览。
    let layer = process.call_tool(
        "create_layer",
        json!({"layer_id": "layer_1", "name": "ink"}),
    );
    assert_eq!(layer["ok"], json!(true));
    let stroke = process.call_tool(
        "draw_stroke",
        json!({
            "layer_id": "layer_1",
            "data": {"points": [[6.0, 6.0], [40.0, 30.0]], "size": 5.0, "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(stroke["dirty_kind"], json!("geometry"));
    assert!(stroke["preview"]["thumb_url"]
        .as_str()
        .unwrap()
        .starts_with("yanshi://blob/"));

    // 4) 区域渲染带内嵌图像（≤512px）。
    let render = process.request(
        "tools/call",
        json!({
            "name": "render_region",
            "arguments": {"region": {"x": 0, "y": 0, "w": 16, "h": 16}, "include_image": true}
        }),
    );
    assert_eq!(render["content"][1]["type"], json!("image"));
    assert_eq!(render["content"][1]["mimeType"], json!("image/png"));
    assert!(
        render["content"][1]["data"].as_str().unwrap().len() > 32,
        "内嵌 PNG 应为 base64"
    );

    // 5) 轮询渲染状态。
    let status = process.call_tool("get_render_status", json!({"atom_id": stroke["atom_id"]}));
    assert_eq!(status["rendered"], json!(true));

    // 6) 标注通道（不进原子日志）。
    let annotation = process.call_tool(
        "create_annotation",
        json!({
            "type": "region",
            "intent": "style",
            "target": {"target": "region", "bbox": {"x": 0, "y": 0, "w": 8, "h": 8}},
            "content": "这里换成暖色",
        }),
    );
    assert!(annotation["annotation_id"].as_str().is_some());
    let log = process.call_tool("get_log", json!({}));
    assert_eq!(log["count"], json!(3), "标注不进入原子日志");

    process.stop();

    // 7) 重启进程：文档从磁盘恢复，状态与渲染缓存可用。
    let mut restarted = McpProcess::start(&[
        "--root", &root_str, "--doc", "doc_it", "--width", "48", "--height", "48",
    ]);
    restarted.request("initialize", json!({}));
    let document = restarted.call_tool("get_document", json!({}));
    assert_eq!(document["doc_id"], json!("doc_it"));
    assert_eq!(document["head_seq"], json!(3));
    assert!(
        document["thumb_url"].as_str().is_some(),
        "打开即图片：渲染缓存可直接用（14.5）"
    );
    let objects = restarted.call_tool("list_objects", json!({}));
    assert_eq!(objects["count"], json!(1));
    restarted.stop();

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn stdio_reports_unknown_tools_and_bad_json() {
    let mut process = McpProcess::start(&["--doc", "doc_err"]);
    process.request("initialize", json!({}));
    // 未知工具走 JSON-RPC 错误（参数层校验）。
    let message =
        json!({"jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": {"name": "nope"}});
    writeln!(process.stdin, "{message}").unwrap();
    process.stdin.flush().unwrap();
    let mut line = String::new();
    process.stdout.read_line(&mut line).unwrap();
    let response: Value = serde_json::from_str(line.trim()).unwrap();
    assert_eq!(response["error"]["code"], json!(-32602));

    // 工具级错误走 5.7 形状 + isError。
    let result = process.request(
        "tools/call",
        json!({"name": "delete_object", "arguments": {"object_id": "obj_missing"}}),
    );
    assert_eq!(result["isError"], json!(true));
    let error: Value =
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["error_code"], json!("reference_not_found"));
    process.stop();
}

#[test]
fn cli_prints_help_and_tool_list() {
    let output = Command::new(env!("CARGO_BIN_EXE_yanshi-mcp"))
        .arg("--help")
        .output()
        .expect("运行 --help");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("yanshi-mcp"));
    assert!(text.contains("--profile"));

    let output = Command::new(env!("CARGO_BIN_EXE_yanshi-mcp"))
        .args(["--list-tools", "--profile", "core,history"])
        .output()
        .expect("运行 --list-tools");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("工具清单是 JSON");
    let tools = value["tools"].as_array().unwrap();
    assert!(
        tools.len() > 27,
        "core + history 应多于 27 个：{}",
        tools.len()
    );
}
