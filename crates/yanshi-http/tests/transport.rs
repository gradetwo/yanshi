//! 传输层集成测试：原始 TCP 客户端打真实 HTTP/WebSocket 字节流。
//!
//! 覆盖：HTTP 往返与 404/405、capability token 鉴权（403）、工具调用 →
//! 区域渲染 → 从 CAS 取回 PNG、WebSocket 握手（含 Accept 校验）、
//! 控制流全局推送 vs 数据流视口过滤、WS 级 ping/pong、错误 token 拒绝升级。

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;
use yanshi_http::server::{serve, HttpOptions};
use yanshi_http::ws::{encode_client_frame, read_frame_any, Frame, OpCode};

/// 每个请求用一条新连接（服务端一律 `Connection: close`）。
struct HttpClient {
    addr: std::net::SocketAddr,
}

impl HttpClient {
    fn new(addr: std::net::SocketAddr) -> Self {
        Self { addr }
    }

    fn request(
        &mut self,
        method: &str,
        target: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> (u16, BTreeMap<String, String>, Vec<u8>) {
        let mut stream = TcpStream::connect(self.addr).expect("连接服务端");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let payload = body.map(|value| value.to_string()).unwrap_or_default();
        let mut head =
            format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n");
        if let Some(token) = token {
            head.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        if body.is_some() {
            head.push_str("Content-Type: application/json\r\n");
            head.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).unwrap();
        stream.write_all(payload.as_bytes()).unwrap();
        stream.flush().unwrap();
        read_response(&mut stream)
    }

    fn json(
        &mut self,
        method: &str,
        target: &str,
        token: Option<&str>,
        body: Option<&Value>,
    ) -> (u16, Value) {
        let (status, _, bytes) = self.request(method, target, token, body);
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

fn read_response(stream: &mut TcpStream) -> (u16, BTreeMap<String, String>, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut status_line = String::new();
    reader.read_line(&mut status_line).unwrap();
    let status: u16 = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0);
    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
        }
    }
    let length: usize = headers
        .get("content-length")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    (status, headers, body)
}

fn start_server() -> yanshi_http::ServerHandle {
    let options = HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        width: 64,
        height: 64,
        push_interval_ms: 10,
        ..HttpOptions::default()
    };
    serve(options).expect("启动服务")
}

fn start_server_with_root(root: std::path::PathBuf) -> yanshi_http::ServerHandle {
    serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        root: Some(root),
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .expect("启动服务")
}

/// 建立文档并返回 capability token。
fn create_document(addr: std::net::SocketAddr, doc_id: &str) -> String {
    let mut client = HttpClient::new(addr);
    let (status, body) = client.json(
        "POST",
        "/api/documents",
        None,
        Some(&json!({"doc_id": doc_id, "width": 64, "height": 64})),
    );
    assert_eq!(status, 200, "{body}");
    body["token"].as_str().expect("capability token").to_owned()
}

/// 直接调工具（每次新连接）。
fn tool(
    addr: std::net::SocketAddr,
    doc: &str,
    token: &str,
    name: &str,
    body: Value,
) -> (u16, Value) {
    let mut client = HttpClient::new(addr);
    client.json(
        "POST",
        &format!("/api/tools/{name}?doc={doc}"),
        Some(token),
        Some(&body),
    )
}

#[test]
fn http_round_trip_health_viewer_and_errors() {
    let handle = start_server();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);

    // /health 无需鉴权。
    let (status, body) = client.json("GET", "/health", None, None);
    assert_eq!(status, 200);
    assert_eq!(body["ok"], json!(true));
    assert!(body["tools"].as_u64().unwrap() >= 27);

    // 查看器页面。
    let (status, headers, bytes) = client.request("GET", "/", None, None);
    assert_eq!(status, 200);
    assert!(headers["content-type"].starts_with("text/html"));
    let html = String::from_utf8(bytes).unwrap();
    assert!(html.contains("偃师"));
    assert!(html.contains("/ws?doc="));

    // 未知路由 404，错误方法 405。
    assert_eq!(client.json("GET", "/nope", None, None).0, 404);
    assert_eq!(client.json("PUT", "/api/documents", None, None).0, 405);
    assert_eq!(
        client.json("GET", "/api/tools/draw_stroke", None, None).0,
        405
    );
}

#[test]
fn capability_token_gates_http_endpoints() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_auth");
    assert_eq!(token.len(), 64);

    // 无 token → 403 且是 5.7 错误形状。
    let (status, body) = tool(
        addr,
        "doc_auth",
        "",
        "create_layer",
        json!({"layer_id": "layer_1"}),
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(body["ok"], json!(false));
    assert_eq!(body["error_code"], json!("permission_denied"));

    // 错误 token → 403。
    let bogus = "a".repeat(64);
    let (status, _) = tool(
        addr,
        "doc_auth",
        &bogus,
        "create_layer",
        json!({"layer_id": "layer_1"}),
    );
    assert_eq!(status, 403);

    // 正确 token（Bearer 头）→ 200。
    let (status, body) = tool(
        addr,
        "doc_auth",
        &token,
        "create_layer",
        json!({"layer_id": "layer_1"}),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["seq"], json!(2));

    // token 放查询串亦可（浏览器 <img>/WebSocket 无法带头部）。
    let mut client = HttpClient::new(addr);
    let (status, body) = client.json(
        "POST",
        &format!("/api/tools/create_layer?doc=doc_auth&token={token}"),
        None,
        Some(&json!({"layer_id": "layer_2"})),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["seq"], json!(3));
}

#[test]
fn tool_call_renders_region_and_serves_png_from_cas() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_render");

    let (status, layer) = tool(
        addr,
        "doc_render",
        &token,
        "create_layer",
        json!({"layer_id": "layer_1"}),
    );
    assert_eq!(status, 200, "{layer}");
    let (status, stroke) = tool(
        addr,
        "doc_render",
        &token,
        "draw_stroke",
        json!({
            "layer_id": "layer_1",
            "data": {"points": [[4.0, 4.0], [40.0, 30.0]], "size": 5.0, "color": [0.0, 0.0, 0.0, 1.0]},
        }),
    );
    assert_eq!(status, 200, "{stroke}");
    assert_eq!(stroke["dirty_kind"], json!("geometry"));
    let thumb_url = stroke["preview"]["thumb_url"].as_str().unwrap().to_owned();
    assert!(
        thumb_url.starts_with("/api/blob/"),
        "yanshi://blob/ 已被改写成可直接 GET 的 URL：{thumb_url}"
    );
    assert!(thumb_url.contains("doc=doc_render"));
    assert!(thumb_url.contains("token="));

    // 从 CAS 取回 PNG。
    let mut blob_client = HttpClient::new(addr);
    let (status, headers, bytes) = blob_client.request("GET", &thumb_url, None, None);
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(
        &bytes[0..8],
        &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]
    );
    assert!(bytes.len() > 64);

    // 非法哈希 → 400；不存在 → 404。
    let mut client = HttpClient::new(addr);
    let (status, body) = client.json(
        "GET",
        &format!("/api/blob/not-a-hash?doc=doc_render&token={token}"),
        None,
        None,
    );
    assert_eq!(status, 400, "{body}");
    let missing = format!("sha256:{}", "b".repeat(64));
    let (status, _) = client.json(
        "GET",
        &format!("/api/blob/{missing}?doc=doc_render&token={token}"),
        None,
        None,
    );
    assert_eq!(status, 404);

    // 文档摘要需要 token。
    let (status, body) = client.json(
        "GET",
        &format!("/api/documents/doc_render?token={token}"),
        None,
        None,
    );
    assert_eq!(status, 200);
    assert_eq!(body["document"]["head_seq"], json!(3));
}

struct WsClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl WsClient {
    fn connect(
        addr: std::net::SocketAddr,
        doc: &str,
        token: &str,
    ) -> (Self, u16, BTreeMap<String, String>) {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let key = "dGhlIHNhbXBsZSBub25jZQ==";
        let request = format!(
            "GET /ws?doc={doc}&token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut status_line = String::new();
        reader.read_line(&mut status_line).unwrap();
        let status: u16 = status_line
            .split(' ')
            .nth(1)
            .and_then(|code| code.parse().ok())
            .unwrap_or(0);
        let mut headers = BTreeMap::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let line = line.trim_end().to_string();
            if line.is_empty() {
                break;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        if status != 101 {
            // 失败响应带 JSON body，读掉以便调用方断言。
            let length: usize = headers
                .get("content-length")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            let mut body = vec![0u8; length];
            let _ = reader.read_exact(&mut body);
        }
        (Self { stream, reader }, status, headers)
    }

    fn send_json(&mut self, value: &Value) {
        let payload = value.to_string();
        let frame = encode_client_frame(OpCode::Text, true, payload.as_bytes(), [1, 2, 3, 4]);
        self.stream.write_all(&frame).unwrap();
        self.stream.flush().unwrap();
    }

    fn send_ping(&mut self, payload: &[u8]) {
        let frame = encode_client_frame(OpCode::Ping, true, payload, [9, 9, 9, 9]);
        self.stream.write_all(&frame).unwrap();
        self.stream.flush().unwrap();
    }

    fn read(&mut self) -> Option<Frame> {
        read_frame_any(&mut self.reader).unwrap_or_default()
    }

    /// 一直读到满足条件的 JSON 消息（或超时）。
    fn wait_for_json(&mut self, predicate: impl Fn(&Value) -> bool) -> Option<Value> {
        for _ in 0..64 {
            let frame = self.read()?;
            if frame.opcode != OpCode::Text {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<Value>(&frame.payload) else {
                continue;
            };
            if predicate(&value) {
                return Some(value);
            }
        }
        None
    }
}

#[test]
fn websocket_handshake_uses_rfc6455_accept_key() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_ws");
    let (mut client, status, headers) = WsClient::connect(addr, "doc_ws", &token);
    assert_eq!(status, 101, "升级成功");
    assert_eq!(headers["upgrade"].to_ascii_lowercase(), "websocket");
    assert_eq!(
        headers["sec-websocket-accept"], "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=",
        "RFC 6455 §1.3 示例 key 的 Accept"
    );

    // WS 级 ping → pong（帧层）。
    client.send_ping(b"hb");
    let mut saw_pong = false;
    for _ in 0..16 {
        let Some(frame) = client.read() else { break };
        if frame.opcode == OpCode::Pong {
            assert_eq!(frame.payload, b"hb");
            saw_pong = true;
            break;
        }
    }
    assert!(saw_pong, "应自动回 pong");

    // JSON 层 ping → pong 消息。
    client.send_json(&json!({"type": "ping"}));
    let pong = client
        .wait_for_json(|value| value["type"] == json!("pong"))
        .expect("JSON pong");
    assert_eq!(pong["type"], json!("pong"));

    // 订阅视口。
    client.send_json(&json!({
        "type": "subscribe",
        "doc_id": "doc_ws",
        "viewport": {"x": 0, "y": 0, "w": 32, "h": 32},
        "zoom": 1.0,
    }));
    let subscribed = client
        .wait_for_json(|value| value["type"] == json!("subscribed"))
        .expect("订阅确认");
    assert_eq!(subscribed["applied"], json!(true));
}

#[test]
fn websocket_rejects_bad_tokens_and_unknown_messages() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_ws_auth");
    // 错误 token → 403（不升级）。
    let bogus = "c".repeat(64);
    let (_client, status, _headers) = WsClient::connect(addr, "doc_ws_auth", &bogus);
    assert_eq!(status, 403);
    // 缺少 doc → 400。
    let (_client, status, _headers) = WsClient::connect(addr, "", &token);
    assert_eq!(status, 400);
    // 未知消息类型 → error 消息但不中断。
    let (mut client, status, _) = WsClient::connect(addr, "doc_ws_auth", &token);
    assert_eq!(status, 101);
    client.send_json(&json!({"type": "nonsense"}));
    let error = client
        .wait_for_json(|value| value["type"] == json!("error"))
        .expect("错误消息");
    assert_eq!(error["error"]["code"], json!("invalid_argument"));
}

#[test]
fn websocket_pushes_control_flow_globally_and_data_flow_by_viewport() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_push");
    let (mut ws, status, _) = WsClient::connect(addr, "doc_push", &token);
    assert_eq!(status, 101);
    // 只订阅左上角 32×32（tile 边长 256，因此只有 (0,0) 在视口内）。
    ws.send_json(&json!({
        "type": "subscribe",
        "viewport": {"x": 0, "y": 0, "w": 32, "h": 32},
    }));
    assert!(ws
        .wait_for_json(|value| value["type"] == json!("subscribed"))
        .is_some());

    // 通过 HTTP 触发修改（模拟另一个客户端）。
    let (status, layer) = tool(
        addr,
        "doc_push",
        &token,
        "create_layer",
        json!({"layer_id": "layer_1"}),
    );
    assert_eq!(status, 200, "{layer}");

    // 控制流：创建图层（几何上不在 32×32 视口内）也必须广播。
    let atom_event = ws
        .wait_for_json(|value| {
            value["type"] == json!("event") && value["event"]["event"] == json!("atom")
        })
        .expect("控制流事件");
    assert_eq!(atom_event["event"]["kind"], json!("create_layer"));
    assert_eq!(atom_event["event"]["actor"], json!("human:web"));

    // 数据流：tile 事件只包含视口内的 tile。
    let (status, stroke) = tool(
        addr,
        "doc_push",
        &token,
        "draw_stroke",
        json!({
            "layer_id": "layer_1",
            "data": {"points": [[4.0, 4.0], [40.0, 30.0]], "size": 4.0},
        }),
    );
    assert_eq!(status, 200, "{stroke}");
    let tiles_event = ws
        .wait_for_json(|value| {
            value["type"] == json!("event") && value["event"]["event"] == json!("tiles")
        })
        .expect("数据流事件");
    let keys = tiles_event["event"]["keys"].as_array().unwrap();
    assert!(!keys.is_empty());
    for key in keys {
        assert_eq!(key["x"], json!(0), "视口外（x>0）的 tile 不应推送：{key}");
        assert_eq!(key["y"], json!(0));
    }

    // 通过 WS 调用工具并拿到 ack（WS 走 ack + 推送，stdio 走轮询）。
    ws.send_json(&json!({
        "type": "tool",
        "request_id": 7,
        "name": "get_document",
        "arguments": {},
    }));
    let ack = ws
        .wait_for_json(|value| value["type"] == json!("ack") && value["request_id"] == json!(7))
        .expect("工具 ack");
    assert_eq!(ack["result"]["ok"], json!(true));
    assert_eq!(ack["result"]["doc_id"], json!("doc_push"));
}

#[test]
fn document_persistence_over_http_survives_restart() {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-http-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let token;
    {
        let handle = start_server_with_root(root.clone());
        let addr = handle.addr;
        token = create_document(addr, "doc_persist");
        let (status, body) = tool(
            addr,
            "doc_persist",
            &token,
            "create_layer",
            json!({"layer_id": "layer_1"}),
        );
        assert_eq!(status, 200, "{body}");
        handle.shutdown();
    }

    // 重启：文档从磁盘恢复，且 token 仍然有效（12.7 令牌持久化）。
    let handle = start_server_with_root(root.clone());
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);
    let (status, body) = client.json("GET", "/api/documents", None, None);
    assert_eq!(status, 200);
    let listed = body["documents"].as_array().unwrap();
    assert!(listed
        .iter()
        .any(|doc| doc["doc_id"] == json!("doc_persist")));

    let (status, body) = client.json(
        "GET",
        &format!("/api/documents/doc_persist?token={token}"),
        None,
        None,
    );
    assert_eq!(status, 200, "{body}");
    // 打开时加载日志并标记渲染缓存（打开即图片），head_seq = 2。
    assert_eq!(body["document"]["head_seq"], json!(2));
    assert_eq!(body["document"]["layers"], json!(1));
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
