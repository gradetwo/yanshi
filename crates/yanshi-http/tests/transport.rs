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

/// 12.2：客户端自带 ULID 的原子提交必须幂等（重试安全）；这也是 WASM 乐观渲染的服务端入口。
#[test]
fn client_supplied_atom_ids_are_idempotent() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_atoms");

    let mut client = HttpClient::new(addr);
    // 客户端构造的原子（没有 seq；服务端授予权威 seq）。
    let atom = json!({
        "id": "01CLIENT0000000000000000AB",
        "kind": "create_layer",
        "actor": "human:wasm",
        "session": "session:wasm",
        "timestamp": 1_700_000_000_000i64,
        "payload": {"layer_id": "layer_wasm", "name": "wasm"}
    });
    let (status, body) = client.json(
        "POST",
        &format!("/api/atoms?doc=doc_atoms&token={token}"),
        None,
        Some(&atom),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ok"], json!(true));
    assert_eq!(body["atom_id"], json!("01CLIENT0000000000000000AB"));
    assert_eq!(body["seq"], json!(2), "create_document 占 seq 1");
    assert_eq!(body["duplicate"], json!(false));
    // 回带权威原子（含 seq），客户端据此本地应用而不必猜 seq。
    assert_eq!(body["atom"]["id"], json!("01CLIENT0000000000000000AB"));
    assert_eq!(body["atom"]["seq"], json!(2));

    // 同一 id 重试：幂等命中，seq 不变、不产生新原子。
    let (status, retry) = client.json(
        "POST",
        &format!("/api/atoms?doc=doc_atoms&token={token}"),
        None,
        Some(&atom),
    );
    assert_eq!(status, 200, "{retry}");
    assert_eq!(retry["duplicate"], json!(true));
    assert_eq!(retry["seq"], json!(2));

    // 下一个客户端原子拿到连续 seq；客户端据此做本地乐观预测与校正。
    let mut second = atom.clone();
    second["id"] = json!("01CLIENT0000000000000000AC");
    second["kind"] = json!("draw_stroke");
    second["payload"] = json!({
        "object_id": "obj_wasm",
        "layer_id": "layer_wasm",
        "data": {"points": [[4.0, 4.0], [40.0, 30.0]], "size": 5.0, "color": [20, 20, 30, 255]}
    });
    let (status, body) = client.json(
        "POST",
        &format!("/api/atoms?doc=doc_atoms&token={token}"),
        None,
        Some(&second),
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["seq"], json!(3));
    assert_eq!(body["dirty_kind"], json!("geometry"));

    // 原子确实进了权威日志，且 session 保持客户端给的会话。
    let log = tool(addr, "doc_atoms", &token, "get_log", json!({})).1;
    let atoms = log["atoms"].as_array().unwrap();
    assert_eq!(atoms.len(), 3);
    assert_eq!(atoms[2]["session"], json!("session:wasm"));
    assert_eq!(atoms[2]["seq"], json!(3));

    // 缺 id / 非法 JSON → 400，且不写入日志。
    let (status, body) = client.json(
        "POST",
        &format!("/api/atoms?doc=doc_atoms&token={token}"),
        None,
        Some(&json!({"kind": "comment", "actor": "human:1", "session": "s", "timestamp": 1})),
    );
    assert_eq!(status, 400, "{body}");
    let (status, _) = client.json(
        "POST",
        &format!("/api/atoms?doc=doc_atoms&token={token}"),
        None,
        Some(&json!({"nonsense": true})),
    );
    assert_eq!(status, 400);
    let log = tool(addr, "doc_atoms", &token, "get_log", json!({})).1;
    assert_eq!(log["count"], json!(3), "被拒绝的提交不进日志");

    // GET /api/atoms 返回**完整**原子（含 payload），供客户端本地折叠。
    let (status, body) = client.json(
        "GET",
        &format!("/api/atoms?doc=doc_atoms&since=1&token={token}"),
        None,
        None,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["count"], json!(2), "since=1 之后还有 2 个原子");
    assert_eq!(body["head_seq"], json!(3));
    let atoms = body["atoms"].as_array().unwrap();
    assert_eq!(atoms[0]["id"], json!("01CLIENT0000000000000000AB"));
    assert_eq!(
        atoms[0]["payload"]["layer_id"],
        json!("layer_wasm"),
        "客户端折叠需要完整 payload，而不是只有元数据"
    );
    assert_eq!(atoms[1]["payload"]["data"]["size"], json!(5.0));
    assert_eq!(
        client
            .json(
                "GET",
                &format!("/api/atoms?doc=doc_atoms&token={token}"),
                None,
                None
            )
            .1["count"],
        json!(3),
        "不带 since 时返回全部原子"
    );

    // 无 token → 403（与工具路径一致）。
    let (status, _) = client.json("POST", "/api/atoms?doc=doc_atoms", None, Some(&second));
    assert_eq!(status, 403);
}

/// `/wasm/*` 只提供白名单产物，且目录缺失时给出可操作的 404。
#[test]
fn wasm_assets_are_served_from_the_configured_directory() {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-wasm-assets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("yanshi_wasm.js"),
        b"export default async function init() {}",
    )
    .unwrap();
    std::fs::write(root.join("yanshi_wasm_bg.wasm"), b"\0asm\x01\0\0\0").unwrap();
    std::fs::write(root.join("secret.txt"), b"nope").unwrap();

    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        wasm_dir: Some(root.clone()),
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);

    let health = client.json("GET", "/health", None, None).1;
    assert_eq!(health["wasm"], json!(true), "产物齐全时 /health 报告可用");

    let (status, headers, bytes) = client.request("GET", "/wasm/yanshi_wasm.js", None, None);
    assert_eq!(status, 200);
    assert!(headers["content-type"].starts_with("text/javascript"));
    assert!(String::from_utf8(bytes).unwrap().contains("export default"));

    let (status, headers, bytes) = client.request("GET", "/wasm/yanshi_wasm_bg.wasm", None, None);
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "application/wasm");
    assert_eq!(&bytes[0..4], b"\0asm");

    // 白名单之外 / 路径穿越 / 缺失文件。
    assert_eq!(client.request("GET", "/wasm/secret.txt", None, None).0, 404);
    assert_eq!(
        client.request("GET", "/wasm/../Cargo.toml", None, None).0,
        400
    );
    assert_eq!(client.request("GET", "/wasm/missing.js", None, None).0, 404);
    assert_eq!(
        client.request("POST", "/wasm/yanshi_wasm.js", None, None).0,
        405
    );
    handle.shutdown();

    // 目录缺失：/health 报不可用，资源 404 且提示如何构建。
    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        wasm_dir: Some(root.join("missing")),
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);
    let health = client.json("GET", "/health", None, None).1;
    assert_eq!(health["wasm"], json!(false));
    let (status, body) = client.json("GET", "/wasm/yanshi_wasm.js", None, None);
    assert_eq!(status, 404);
    assert!(body["context"]["detail"]
        .as_str()
        .unwrap()
        .contains("wasm-bindgen"));
    handle.shutdown();

    // --no-wasm：明确不提供。
    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        wasm_dir: None,
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);
    assert_eq!(
        client.json("GET", "/health", None, None).1["wasm"],
        json!(false)
    );
    let (status, body) = client.json("GET", "/wasm/yanshi_wasm.js", None, None);
    assert_eq!(status, 404);
    assert!(body["context"]["detail"]
        .as_str()
        .unwrap()
        .contains("--no-wasm"));
    handle.shutdown();

    let _ = std::fs::remove_dir_all(&root);
}

/// 回归：服务端声称 keep-alive 就必须真的复用连接。
///
/// 曾经的缺陷是一条连接只处理一个请求却回 `Connection: keep-alive`，
/// 浏览器复用该连接时拿到已关闭的 socket，表现为随机的 `Failed to fetch`。
#[test]
fn one_connection_serves_multiple_requests() {
    let handle = start_server();
    let addr = handle.addr;
    let token = create_document(addr, "doc_keepalive");

    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let listing = "GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n";
    stream.write_all(listing.as_bytes()).unwrap();
    stream.flush().unwrap();
    let (status, headers, body) = read_response(&mut stream);
    assert_eq!(status, 200);
    assert_eq!(
        headers.get("connection").map(String::as_str),
        Some("keep-alive")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["ok"],
        json!(true)
    );

    // 同一条连接上继续发两个请求（工具调用 + 文档摘要）。
    let body = json!({"layer_id": "layer_1"}).to_string();
    let call = format!(
        "POST /api/tools/create_layer?doc=doc_keepalive&token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
         Connection: keep-alive\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(call.as_bytes()).unwrap();
    stream.flush().unwrap();
    let (status, _, body) = read_response(&mut stream);
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["ok"],
        json!(true)
    );

    let summary = format!(
        "GET /api/documents/doc_keepalive?token={token} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(summary.as_bytes()).unwrap();
    stream.flush().unwrap();
    let (status, headers, body) = read_response(&mut stream);
    assert_eq!(status, 200);
    assert_eq!(headers.get("connection").map(String::as_str), Some("close"));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["document"]["head_seq"],
        json!(2)
    );
    handle.shutdown();
}

/// 联系方式：页面内置反馈邮箱（邮件预填文档 id 与 HEAD 便于排查）。
#[test]
fn viewer_exposes_the_contact_mailbox() {
    // 用 0 端口避免与正在运行的服务（8110）冲突。
    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);
    let (status, _, page) = client.request("GET", "/", None, None);
    assert_eq!(status, 200);
    let page = String::from_utf8_lossy(&page);
    assert!(
        page.contains("mailto:yanshi@wangda.today"),
        "页面应提供反馈邮箱链接"
    );
    assert!(page.contains("yanshi@wangda.today</a>"), "邮箱应可见");
    assert!(
        page.contains("refreshContactLink"),
        "邮件正文应带上文档与 HEAD 上下文"
    );
    assert!(page.contains("安全漏洞"), "应提示安全漏洞不要开公开 issue");
    handle.shutdown();
}

/// 品牌资源：favicon 与 /brand/* 白名单，路径穿越与未知文件被拒。
#[test]
fn brand_assets_are_served_from_the_configured_directory() {
    let mut root = std::env::temp_dir();
    root.push(format!("yanshi-brand-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("svg")).unwrap();
    std::fs::create_dir_all(root.join("png")).unwrap();
    std::fs::write(
        root.join("svg/icon-light.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    std::fs::write(root.join("png/favicon-32.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    std::fs::write(root.join("secret.txt"), b"nope").unwrap();

    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        brand_dir: Some(root.clone()),
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);

    let (status, headers, bytes) = client.request("GET", "/favicon.svg", None, None);
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/svg+xml");
    assert!(String::from_utf8_lossy(&bytes).contains("<svg"));

    let (status, headers, bytes) = client.request("GET", "/favicon.png", None, None);
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(&bytes[0..4], b"\x89PNG");

    let (status, headers, _) = client.request("GET", "/brand/svg/icon-light.svg", None, None);
    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/svg+xml");

    assert_eq!(
        client.request("GET", "/brand/secret.txt", None, None).0,
        404
    );
    assert_eq!(
        client.request("GET", "/brand/../Cargo.toml", None, None).0,
        400
    );
    // 页面引用了 favicon 与页头 logo。
    let (_, _, page) = client.request("GET", "/", None, None);
    let page = String::from_utf8_lossy(&page);
    assert!(page.contains("rel=\"icon\""), "页面应声明 favicon");
    assert!(
        page.contains("brand/svg/icon-light.svg"),
        "页头应使用品牌图标"
    );
    handle.shutdown();

    // --no-brand：明确不提供。
    let handle = serve(HttpOptions {
        bind: "127.0.0.1:0".to_owned(),
        brand_dir: None,
        width: 32,
        height: 32,
        ..HttpOptions::default()
    })
    .unwrap();
    let addr = handle.addr;
    let mut client = HttpClient::new(addr);
    let (status, body) = client.json("GET", "/favicon.svg", None, None);
    assert_eq!(status, 404);
    assert!(body["context"]["detail"]
        .as_str()
        .unwrap()
        .contains("--no-brand"));
    handle.shutdown();
    let _ = std::fs::remove_dir_all(&root);
}
