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

/// **`new_document` 认尺寸、新 id 报 created** ✓ —— 报告的原始场景（2026-10-06 ✓）。
///
/// **为什么必须走真进程** ✗：缺陷**不在工具里** ✓。工具的存在判据是"内存里有没有" ✓，
/// 而 MCP 入口在调工具**之前**就用 CLI 缺省尺寸（`--width/--height` ✓）`open_or_create`
/// 了参数里的 `doc_id` ✓ ⇒ 新 id 到工具里时**已经存在** ✗ ⇒ 走"打开"分支 ✓、
/// 尺寸丢掉（实测恒 1024×1024 ✗）、新 id 被报成 `opened:true` ✗。
/// 直接用 `ToolRegistry` 调（`crates/yanshi-server/tests/new_document.rs` ✓）**绕过了入口** ✓
/// ⇒ 那种判据今天是绿的 ✓ 而真进程是红的 ✗ —— 所以这条必须走 stdio ✓。
///
/// 判据分四段 ✓：① 报告的三个尺寸各用**全新 doc_id** ✓ ⇒ 回执、内存、磁盘 `meta.json`
/// 三处都必须等于请求值 ✓；② 同 id 再建 ⇒ `opened:true`/`created:false` 且**尺寸不变** ✓；
/// ③ 画过的东西**还在** ✓（内容没被清空 ✓）；④ 给一个**不同**的尺寸 ⇒ 老文档
/// **不许被改尺寸** ✓（安全约束 ✓）。
#[test]
fn new_document_honours_size_and_reports_created_for_fresh_ids() {
    let root = temp_root("newdoc");
    let root_str = root.to_string_lossy().into_owned();
    let mut process = McpProcess::start(&[
        "--root",
        &root_str,
        "--doc",
        "seed",
        "--width",
        "1024",
        "--height",
        "1024",
        "--profile",
        "core",
    ]);
    process.request("initialize", json!({}));
    process.notify("notifications/initialized");

    // ① 报告的三个尺寸：每个都用**全新 id** ⇒ 必须是"新建"，且三处尺寸一致。
    for (doc_id, width, height) in [
        ("fresh_3840", 3840u64, 2160u64),
        ("fresh_800", 800, 600),
        ("fresh_1920", 1920, 1080),
    ] {
        let made = process.call_tool(
            "new_document",
            json!({"doc_id": doc_id, "width": width, "height": height}),
        );
        assert_eq!(made["ok"], json!(true), "{made}");
        assert_eq!(
            made["created"],
            json!(true),
            "新 id 必须报 created:true：{made}"
        );
        assert_eq!(made["opened"], json!(false), "新 id 不是打开：{made}");
        assert_eq!(
            (made["width"].as_u64(), made["height"].as_u64()),
            (Some(width), Some(height)),
            "{made}"
        );
        // 内存里的**真实尺寸** ✓（回执可能是照抄请求 ✓ ⇒ 要独立量一次 ✓）。
        let state = process.call_tool(
            "get_state",
            json!({"doc_id": doc_id, "preview_size": false, "include_objects": true}),
        );
        assert_eq!(
            (state["width"].as_u64(), state["height"].as_u64()),
            (Some(width), Some(height)),
            "{state}"
        );
        // 磁盘 `meta.json` ✓。
        let meta_path = root.join("docs").join(doc_id).join("meta.json");
        let meta: Value = serde_json::from_str(
            &std::fs::read_to_string(&meta_path)
                .unwrap_or_else(|error| panic!("{}：{error}", meta_path.display())),
        )
        .expect("meta.json 是 JSON");
        assert_eq!(
            (meta["width"].as_u64(), meta["height"].as_u64()),
            (Some(width), Some(height)),
            "{meta}"
        );
    }

    // ②③④：在**同一份**文档上再建 —— 尺寸不变、内容还在、给别的尺寸也不改。
    let made = process.call_tool(
        "new_document",
        json!({"doc_id": "reuse", "width": 96, "height": 96}),
    );
    assert_eq!(made["created"], json!(true), "{made}");
    let layer = made["default_layer"]
        .as_str()
        .expect("新文档自带默认图层")
        .to_owned();
    let drawn = process.call_tool(
        "draw_shape",
        json!({
            "doc_id": "reuse", "layer_id": layer, "object_id": "s1",
            "data": {"geometry": {"kind": "rect", "bbox": {"x": 8, "y": 8, "w": 40, "h": 30}},
                     "color": {"r": 10, "g": 10, "b": 10, "a": 255}}
        }),
    );
    assert_eq!(drawn["ok"], json!(true), "{drawn}");

    // ② 同 id、同样的尺寸 ⇒ 打开 ✓。
    let reopened = process.call_tool(
        "new_document",
        json!({"doc_id": "reuse", "width": 96, "height": 96}),
    );
    assert_eq!(reopened["ok"], json!(true), "{reopened}");
    assert_eq!(reopened["opened"], json!(true), "{reopened}");
    assert_eq!(reopened["created"], json!(false), "{reopened}");
    assert_eq!(
        (reopened["width"].as_u64(), reopened["height"].as_u64()),
        (Some(96), Some(96)),
        "{reopened}"
    );

    // ④ **安全**：同 id、**不同的尺寸** ⇒ 不许被改尺寸 ✓、不许清空 ✓。
    let safe = process.call_tool(
        "new_document",
        json!({"doc_id": "reuse", "width": 640, "height": 480}),
    );
    assert_eq!(safe["opened"], json!(true), "{safe}");
    assert_eq!(safe["created"], json!(false), "{safe}");
    assert_eq!(
        (safe["width"].as_u64(), safe["height"].as_u64()),
        (Some(96), Some(96)),
        "已存在的文档不许被改成 640×480：{safe}"
    );
    // ③ 画过的东西还在 ✓ —— 用对象数说话 ✓。
    let state = process.call_tool(
        "get_state",
        json!({"doc_id": "reuse", "preview_size": false, "include_objects": true}),
    );
    assert_eq!(
        (state["width"].as_u64(), state["height"].as_u64()),
        (Some(96), Some(96)),
        "{state}"
    );
    let kept = state["objects"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter(|object| object["object_id"] == json!("s1"))
        .count();
    assert_eq!(kept, 1, "打开之后画过的东西必须还在：{state}");

    process.stop();
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
