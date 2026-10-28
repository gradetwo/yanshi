//! HTTP/1.1 请求解析与响应写出（零依赖）。
//!
//! 只实现本服务需要的子集：请求行、头部、`Content-Length` 请求体、
//! 查询参数解析与百分号解码、JSON 响应、静态 HTML、文件下载（PNG），
//! 以及 WebSocket 升级所需的原始流交接。

use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};

/// 请求体上限（32 MiB）——原子里的位图走 CAS，不从这里过。
pub const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;
/// 单行头部上限。
pub const MAX_LINE_BYTES: usize = 16 * 1024;
/// **头部总量上限** ✓（审计 L-5）：逐行 16 KiB 帽不够 —— 单连接可无限喂头部 ⇒ 内存无界 ✗。
pub const MAX_HEADER_TOTAL_BYTES: usize = 64 * 1024;
/// **头部条数上限** ✓（同上；`BTreeMap` 还会把同名头合并 ⇒ 只算行数 ✓）。
pub const MAX_HEADER_COUNT: usize = 100;

/// HTTP 方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// GET。
    Get,
    /// POST。
    Post,
    /// PUT。
    Put,
    /// DELETE。
    Delete,
    /// OPTIONS。
    Options,
    /// HEAD。
    Head,
}

impl Method {
    /// 解析请求行方法。
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "GET" => Self::Get,
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "DELETE" => Self::Delete,
            "OPTIONS" => Self::Options,
            "HEAD" => Self::Head,
            _ => return None,
        })
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
            Self::Head => "HEAD",
        }
    }
}

/// 已解析的请求。
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// 方法。
    pub method: Method,
    /// 原始路径（未解码）。
    pub path: String,
    /// 查询参数（已百分号解码）。
    pub query: BTreeMap<String, String>,
    /// 头部（键统一小写）。
    pub headers: BTreeMap<String, String>,
    /// 请求体。
    pub body: Vec<u8>,
}

impl Request {
    /// 解析请求（读取请求行、头部与请求体）。
    pub fn read<R: BufRead + std::io::Read>(reader: &mut R) -> io::Result<Option<Self>> {
        // **有上限读行** ✓（第 1440 轮）：慢滴不换行 ⇒ 不再无限攒 ✗。
        // `loop` 直接产出请求行 ✓ —— 旧写法每轮先赋值再判断，编译器报「上一轮的赋值没读就覆盖」✗。
        let request_line = loop {
            let (read, line) = read_line_bounded(reader, MAX_LINE_BYTES)?;
            if read == 0 {
                return Ok(None);
            }
            if line != "\r\n" && line != "\n" {
                break line;
            }
        };
        if request_line.len() > MAX_LINE_BYTES {
            return Err(invalid("请求行过长"));
        }
        let mut parts = request_line.trim_end().split(' ');
        let method_text = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or_default();
        let version = parts.next().unwrap_or_default();
        let method = Method::parse(method_text).ok_or_else(|| invalid("不支持的 HTTP 方法"))?;
        if !version.starts_with("HTTP/1.") {
            return Err(invalid("只支持 HTTP/1.1"));
        }
        let (path, query_text) = match target.split_once('?') {
            Some((path, query)) => (path.to_owned(), query),
            None => (target.to_owned(), ""),
        };
        let query = parse_query(query_text);

        let mut headers = BTreeMap::new();
        // **重复 Content-Length 计数** ✓（审计 P2-N2）：头部存进 BTreeMap 会静默覆盖前值 ✓
        // ⇒ 必须在覆盖**之前**数清出现了几次 ✓。
        let mut content_length_seen = 0usize;
        let mut header_bytes = 0usize;
        let mut header_count = 0usize;
        loop {
            // **有上限读行** ✓（同上：超长头部行在 `cap+1` 处止损，不再读完整行 ✗→✓）。
            let (read, line) = read_line_bounded(reader, MAX_LINE_BYTES)?;
            if read == 0 {
                break;
            }
            if line.len() > MAX_LINE_BYTES {
                return Err(invalid("头部过长"));
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            header_bytes = header_bytes.saturating_add(line.len());
            if header_bytes > MAX_HEADER_TOTAL_BYTES {
                return Err(invalid("头部总量过大"));
            }
            header_count += 1;
            if header_count > MAX_HEADER_COUNT {
                return Err(invalid("头部条数过多"));
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                let name = name.trim().to_ascii_lowercase();
                if name == "content-length" {
                    content_length_seen += 1;
                }
                headers.insert(name, value.trim().to_owned());
            }
        }

        // **走私防线** ✓（审计 P2-N2；RFC 7230 §3.3.3）：
        //
        // ① **CL 与 TE 互斥** ✓ —— 此前 Content-Length 命中即用、Transfer-Encoding 被无视 ✗
        //    ⇒ CL+TE 同现按 CL 处理正是 CL.TE 走私的经典形状 ✗。有 TE 就拒 ✓，
        //    与常见代理取首 CL 的语义不可兼容 ✓ ⇒ 宁可断连 ✓。
        // ② **重复 Content-Length 一律拒绝** ✓ —— 相等值也拒 ✓：前置代理（如 nginx）取
        //    **首个** CL ✓，本解析器存 BTreeMap **静默取后值** ✗ ⇒ 两端对 body 边界判断分歧
        //    ⇒ 第二个请求就会在 keep-alive 连接上被当新请求解析，可绕过代理层鉴权/审计头 ✗。
        //    取首与取尾永选一，这里选择**都不取** ✓。
        if content_length_seen > 1 {
            return Err(invalid("重复的 content-length"));
        }
        if headers.contains_key("content-length") && headers.contains_key("transfer-encoding") {
            return Err(invalid("content-length 与 transfer-encoding 同时出现"));
        }

        let mut body = Vec::new();
        if let Some(length) = headers.get("content-length") {
            let length: usize = length.parse().map_err(|_| invalid("Content-Length 非法"))?;
            if length > MAX_BODY_BYTES {
                return Err(invalid("请求体过大"));
            }
            body = vec![0u8; length];
            reader.read_exact(&mut body)?;
        } else if headers
            .get("transfer-encoding")
            .map(|value| value.contains("chunked"))
            .unwrap_or(false)
        {
            return Err(invalid("暂不支持 chunked 请求体"));
        }
        Ok(Some(Self {
            method,
            path,
            query,
            headers,
            body,
        }))
    }

    /// 查询参数。
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(String::as_str)
    }

    /// 头部（键大小写不敏感）。
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// 请求体按 JSON 解析。
    pub fn json(&self) -> serde_json::Result<serde_json::Value> {
        if self.body.is_empty() {
            return Ok(serde_json::json!({}));
        }
        serde_json::from_slice(&self.body)
    }

    /// 是否请求 WebSocket 升级。
    pub fn is_websocket_upgrade(&self) -> bool {
        self.header("upgrade")
            .map(|value| value.eq_ignore_ascii_case("websocket"))
            .unwrap_or(false)
            && self
                .header("connection")
                .map(|value| value.to_ascii_lowercase().contains("upgrade"))
                .unwrap_or(false)
    }

    /// 从 `Authorization: Bearer` 或 `?token=` 取令牌（12.7）。
    pub fn token(&self) -> Option<String> {
        if let Some(header) = self.header("authorization") {
            if let Some(token) = header.strip_prefix("Bearer ") {
                return Some(token.trim().to_owned());
            }
        }
        self.param("token").map(str::to_owned)
    }
}

/// 百分号解码 + `+` 视作空格的查询串解析。
pub fn parse_query(text: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in text.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(percent_decode(name), percent_decode(value));
    }
    map
}

/// 百分号解码（非法序列原样保留）；`+` 视作空格 ⇒ **查询串**语义。
///
/// 路径段请用 [`percent_decode_path`] —— 路径里的 `+` 是个**字面加号**
///（`blend+paint.myb` 这种文件名必须原样），当成空格会把它解成另一个名字。
pub fn percent_decode(text: &str) -> String {
    percent_decode_with(text, true)
}

/// 百分号解码**路径段**：与 [`percent_decode`] 的唯一区别是 `+` **保持字面**。
///
/// **为什么必须有它**：浏览器把 URL 路径里会改变含义的字符编成 `%XX`
///（`#` ⇒ `%23`、`%` ⇒ `%25`、`+` ⇒ `%2B`），服务端要在**文件名白名单/穿越守卫之前**
/// 把路径段还原，且必须按**路径**的规矩还原 —— 若沿用查询串那套（`+` ⇒ 空格），
/// `blend%2Bpaint.myb` 会先被还原成 `blend+paint.myb` 再被当成 `blend paint.myb`。
/// 非法序列（如 `100%_Opaque` 里的 `%_O`）原样保留，与查询串同一套宽容规则。
pub fn percent_decode_path(text: &str) -> String {
    percent_decode_with(text, false)
}

/// 两者的共同实现：`plus_is_space` 决定 `+` 是空格（查询串）还是字面加号（路径段）。
fn percent_decode_with(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' if plus_is_space => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 响应。
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    /// 状态码。
    pub status: u16,
    /// Content-Type。
    pub content_type: String,
    /// 头部（额外）。
    pub headers: Vec<(String, String)>,
    /// 响应体。
    pub body: Vec<u8>,
}

impl Response {
    /// JSON 响应（5.7 错误与 10.1 成功都走这里）。
    pub fn json(status: u16, value: &serde_json::Value) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8".to_owned(),
            headers: Vec::new(),
            body: value.to_string().into_bytes(),
        }
    }

    /// 文本响应。
    pub fn text(status: u16, content_type: &str, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: content_type.to_owned(),
            headers: Vec::new(),
            body: body.into().into_bytes(),
        }
    }

    /// HTML 响应。
    pub fn html(body: impl Into<String>) -> Self {
        Self::text(200, "text/html; charset=utf-8", body)
    }

    /// 二进制响应。
    pub fn bytes(status: u16, content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type: content_type.to_owned(),
            headers: Vec::new(),
            body,
        }
    }

    /// 追加头部。
    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_owned(), value.into()));
        self
    }

    /// 状态码原因短语。
    pub const fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            201 => "Created",
            204 => "No Content",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            426 => "Upgrade Required",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "Unknown",
        }
    }

    /// 写入响应（始终 `Connection: close`，一个连接一个请求循环）。
    pub fn write<W: Write>(&self, writer: &mut W, keep_alive: bool) -> io::Result<()> {
        self.write_inner(writer, keep_alive, true)
    }

    /// 响应 `HEAD` 请求：头部（含 `Content-Length`）与 `GET` 完全一致，但**不写响应体**。
    pub fn write_head<W: Write>(&self, writer: &mut W, keep_alive: bool) -> io::Result<()> {
        self.write_inner(writer, keep_alive, false)
    }

    fn write_inner<W: Write>(
        &self,
        writer: &mut W,
        keep_alive: bool,
        body: bool,
    ) -> io::Result<()> {
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
            if keep_alive { "keep-alive" } else { "close" }
        );
        // **安全头** ✓（审计 M-5）：nosniff 挡 MIME 嗅探 ✓、no-referrer 防 `?token=` 经
        // Referer 外泄 ✓、frame-ancestors/X-Frame-Options 挡点击劫持 ✓（只需 frame 指令，
        // 不限制脚本 ⇒ 不会把内联脚本的查看器打坏 ✓）。
        head.push_str("X-Content-Type-Options: nosniff\r\n");
        head.push_str("Referrer-Policy: no-referrer\r\n");
        head.push_str("X-Frame-Options: DENY\r\n");
        head.push_str("Content-Security-Policy: frame-ancestors 'none'\r\n");
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        writer.write_all(head.as_bytes())?;
        if body {
            writer.write_all(&self.body)?;
        }
        writer.flush()
    }

    /// 5.7 错误响应。
    pub fn from_error(error: &yanshi_core::YanshiError) -> Self {
        let status = match error.code {
            yanshi_core::ErrorCode::InvalidArgument => 400,
            yanshi_core::ErrorCode::PermissionDenied => 403,
            yanshi_core::ErrorCode::ReferenceNotFound => 404,
            yanshi_core::ErrorCode::PreconditionFailed => 409,
            yanshi_core::ErrorCode::Conflict => 409,
            yanshi_core::ErrorCode::ResourceExhausted => 503,
            yanshi_core::ErrorCode::Degraded => 503,
            yanshi_core::ErrorCode::JobPending => 202,
            yanshi_core::ErrorCode::JobNotFound => 404,
            // **在飞冲突 / 已取消** ✓：两者都表示"这次请求与文档上正在发生的事冲突" ✓
            // ⇒ 409（不是 4xx 参数错，也不是 5xx 服务错 ✓）。
            yanshi_core::ErrorCode::Busy => 409,
            yanshi_core::ErrorCode::Cancelled => 409,
        };
        Self::json(status, &error.to_response())
    }
}

/// 便捷：构造 `invalid_argument` 错误响应。
pub fn bad_request(detail: impl Into<String>) -> Response {
    Response::from_error(&yanshi_core::YanshiError::new(
        yanshi_core::ErrorCode::InvalidArgument,
        yanshi_core::ErrorContext::detail(detail),
    ))
}

/// 便捷：`permission_denied`。
pub fn forbidden(detail: impl Into<String>) -> Response {
    Response::from_error(&yanshi_core::YanshiError::new(
        yanshi_core::ErrorCode::PermissionDenied,
        yanshi_core::ErrorContext::detail(detail),
    ))
}

/// 便捷：`reference_not_found`。
pub fn not_found(detail: impl Into<String>) -> Response {
    Response::from_error(&yanshi_core::YanshiError::new(
        yanshi_core::ErrorCode::ReferenceNotFound,
        yanshi_core::ErrorContext::detail(detail),
    ))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

/// **有上限地读一行**（第 1440 轮审计）。
///
/// `BufRead::read_line` 会把一行**无限**攒进内存 ⇒ 对**永远不发换行**的慢滴客户端，
/// "读完才检查长度"✗ 检查的是已经涨爆的缓冲 ✓。这里读到 `cap + 1` 字节就**止损返回**
/// （调用方原有的 `len > MAX_LINE_BYTES` 判据照常生效 ✓）。
/// 返回值 =（本次消费的字节数，行文本）；0 表示对端在空行前关闭（与 `read_line` 同义 ✓）。
/// 非 UTF-8 按替换字符处理（头部键值随后只做 ASCII 小写比较 ✓，不该因脏字节断连 ✗）。
fn read_line_bounded<R: BufRead>(reader: &mut R, cap: usize) -> io::Result<(usize, String)> {
    let mut bytes: Vec<u8> = Vec::new();
    let mut one = [0u8; 1];
    loop {
        let n = reader.read(&mut one)?;
        if n == 0 {
            return Ok((bytes.len(), String::from_utf8_lossy(&bytes).into_owned()));
        }
        bytes.push(one[0]);
        if one[0] == b'\n' || bytes.len() > cap {
            return Ok((bytes.len(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn parse(raw: &str) -> Request {
        Request::read(&mut Cursor::new(raw.as_bytes()))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn parses_request_line_headers_and_body() {
        let request = parse(
            "POST /api/tools/draw_stroke?doc=d1&token=abc%2Bd HTTP/1.1\r\n\
             Host: 127.0.0.1:8080\r\n\
             Content-Type: application/json\r\n\
             Content-Length: 13\r\n\
             \r\n\
             {\"size\": 4.0}",
        );
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.path, "/api/tools/draw_stroke");
        assert_eq!(request.param("doc"), Some("d1"));
        assert_eq!(request.param("token"), Some("abc+d"), "百分号解码");
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.json().unwrap()["size"], serde_json::json!(4.0));
    }

    #[test]
    fn token_comes_from_header_or_query() {
        let header = parse("GET /x?doc=d1 HTTP/1.1\r\nAuthorization: Bearer tok123\r\n\r\n");
        assert_eq!(header.token().as_deref(), Some("tok123"));
        let query = parse("GET /x?doc=d1&token=tok456 HTTP/1.1\r\n\r\n");
        assert_eq!(query.token().as_deref(), Some("tok456"));
        let none = parse("GET /x HTTP/1.1\r\n\r\n");
        assert!(none.token().is_none());
    }

    #[test]
    fn malformed_requests_are_rejected() {
        assert!(Request::read(&mut Cursor::new(b"BREW / HTTP/1.1\r\n\r\n".to_vec())).is_err());
        assert!(Request::read(&mut Cursor::new(b"GET / HTTP/2.0\r\n\r\n".to_vec())).is_err());
        assert!(Request::read(&mut Cursor::new(
            b"GET / HTTP/1.1\r\nContent-Length: abc\r\n\r\n".to_vec()
        ))
        .is_err());
        assert!(Request::read(&mut Cursor::new(Vec::new()))
            .unwrap()
            .is_none());
    }

    #[test]
    fn header_total_bytes_are_capped() {
        // 每行 15 KiB（< MAX_LINE_BYTES）⇒ 5 行就超过 64 KiB 总量 ⇒ 必须报错 ✓。
        let long = "x".repeat(15 * 1024);
        let mut raw = String::from("GET / HTTP/1.1\r\n");
        for i in 0..5 {
            raw.push_str(&format!("X-Pad-{i}: {long}\r\n"));
        }
        raw.push_str("\r\n");
        let err = Request::read(&mut Cursor::new(raw.into_bytes())).unwrap_err();
        assert!(err.to_string().contains("头部总量过大"), "{err}");

        // 边界内（4 行 ≈ 60 KiB）必须照常解析 ✓ —— 证明上限不是「一律拒绝」✗。
        let mut ok = String::from("GET / HTTP/1.1\r\n");
        for i in 0..4 {
            ok.push_str(&format!("X-Pad-{i}: {long}\r\n"));
        }
        ok.push_str("\r\n");
        assert!(Request::read(&mut Cursor::new(ok.into_bytes()))
            .unwrap()
            .is_some());
    }

    #[test]
    fn header_count_is_capped() {
        let mut raw = String::from("GET / HTTP/1.1\r\n");
        for i in 0..=MAX_HEADER_COUNT {
            raw.push_str(&format!("X-H-{i}: v\r\n"));
        }
        raw.push_str("\r\n");
        let err = Request::read(&mut Cursor::new(raw.into_bytes())).unwrap_err();
        assert!(err.to_string().contains("头部条数过多"), "{err}");

        let mut ok = String::from("GET / HTTP/1.1\r\n");
        for i in 0..MAX_HEADER_COUNT {
            ok.push_str(&format!("X-H-{i}: v\r\n"));
        }
        ok.push_str("\r\n");
        assert!(Request::read(&mut Cursor::new(ok.into_bytes()))
            .unwrap()
            .is_some());
    }

    #[test]
    fn websocket_upgrade_detection() {
        let upgrade = parse(
            "GET /ws?doc=d1 HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
        );
        assert!(upgrade.is_websocket_upgrade());
        let plain = parse("GET /ws HTTP/1.1\r\n\r\n");
        assert!(!plain.is_websocket_upgrade());
    }

    #[test]
    fn query_and_percent_decoding_edge_cases() {
        let query = parse_query("a=1&b=hello+world&c=%E4%B8%AD%E6%96%87&d=&e&&f=%ZZ");
        assert_eq!(query["a"], "1");
        assert_eq!(query["b"], "hello world");
        assert_eq!(query["c"], "中文");
        assert_eq!(query["d"], "");
        assert_eq!(query["e"], "");
        assert_eq!(query["f"], "%ZZ", "非法转义原样保留");
        assert_eq!(percent_decode("100%25"), "100%");
    }

    #[test]
    fn response_serialization_has_required_headers() {
        let response = Response::json(200, &serde_json::json!({"ok": true}));
        let mut out = Vec::new();
        response.write(&mut out, false).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(text.contains("Content-Length: 11\r\n"), "{text}");
        assert!(text.contains("Connection: close\r\n"));
        // 审计 M-5：安全头必须每张响应都有（不只是某些路由）✓。
        assert!(
            text.contains("X-Content-Type-Options: nosniff\r\n"),
            "{text}"
        );
        assert!(text.contains("Referrer-Policy: no-referrer\r\n"), "{text}");
        assert!(text.contains("X-Frame-Options: DENY\r\n"), "{text}");
        assert!(
            text.contains("Content-Security-Policy: frame-ancestors 'none'\r\n"),
            "{text}"
        );
        assert!(text.ends_with("{\"ok\":true}"));

        let forbidden = Response::from_error(&yanshi_core::YanshiError::new(
            yanshi_core::ErrorCode::PermissionDenied,
            yanshi_core::ErrorContext::detail("缺少 capability token"),
        ));
        assert_eq!(forbidden.status, 403);
        let body: serde_json::Value = serde_json::from_slice(&forbidden.body).unwrap();
        assert_eq!(body["error_code"], "permission_denied");
    }

    #[test]
    fn error_codes_map_to_http_status() {
        let cases = [
            (yanshi_core::ErrorCode::InvalidArgument, 400),
            (yanshi_core::ErrorCode::PermissionDenied, 403),
            (yanshi_core::ErrorCode::ReferenceNotFound, 404),
            (yanshi_core::ErrorCode::Conflict, 409),
            (yanshi_core::ErrorCode::ResourceExhausted, 503),
        ];
        for (code, expected) in cases {
            let response = Response::from_error(&yanshi_core::YanshiError::new(
                code,
                yanshi_core::ErrorContext::default(),
            ));
            assert_eq!(response.status, expected, "{code:?}");
        }
        assert_eq!(bad_request("x").status, 400);
        assert_eq!(forbidden("x").status, 403);
        assert_eq!(not_found("x").status, 404);
    }

    /// 第 1440 轮审计：**慢滴超长行不再把整段读进内存** ✓。
    /// 判据：请求行是 1 MiB 个 `A`、**永不换行** ⇒ 解析必须报错，
    /// 而且**消费掉的字节数**必须停在 `MAX_LINE_BYTES + 1` 附近 ✓。
    /// 变异（退回 `read_line`）⇒ 它会把 1 MiB 全部读完才报同一个错 ⇒ position 红 ✓。
    #[test]
    fn an_endless_slow_drip_line_is_stopped_at_the_cap() {
        let flood = vec![b'A'; 1024 * 1024];
        let mut cursor = std::io::Cursor::new(flood);
        let outcome = Request::read(&mut cursor);
        assert!(outcome.is_err(), "超长请求行必须报错");
        assert!(
            (cursor.position() as usize) <= MAX_LINE_BYTES + 2,
            "有上限读 ⇒ 只该消费 cap+1 字节，实测 {}",
            cursor.position()
        );
    }

    /// 头部行同口径 ✓：正常请求行 + 永不换行的 1 MiB 头部 ⇒ 在帽处止损 ✓。
    #[test]
    fn an_endless_header_line_is_stopped_at_the_cap() {
        let mut data = b"GET / HTTP/1.1\r\n".to_vec();
        data.extend_from_slice(b"X-Long: ");
        data.extend_from_slice(&vec![b'B'; 1024 * 1024]);
        let mut cursor = std::io::Cursor::new(data);
        let outcome = Request::read(&mut cursor);
        assert!(outcome.is_err(), "超长头部行必须报错");
        assert!(
            cursor.position() as usize <= 16 + MAX_LINE_BYTES + 64,
            "消费字节数应停在帽附近，实测 {}",
            cursor.position()
        );
    }

    /// 哨兵：正常请求照常解析（换行、空行、头部、EOF 语义与 `read_line` 一致 ✓）。
    /// 变异（`read_line_bounded` 提前截断正常行）⇒ 这条红 ✓。
    #[test]
    fn ordinary_requests_still_parse() {
        let mut cursor = std::io::Cursor::new(b"GET /a?b=c HTTP/1.1\r\nHost: x\r\n\r\n".to_vec());
        let request = Request::read(&mut cursor).unwrap().expect("应当解析成功");
        assert_eq!(request.path, "/a");
        assert_eq!(request.param("b"), Some("c"));
        assert_eq!(request.headers.get("host").map(String::as_str), Some("x"));
    }

    /// **CL 与 TE 同时出现必须被拒** ✓（审计 P2-N2 回归判据；RFC 7230 §3.3.3）。
    /// 变异（删掉互斥闸、回退成「CL 命中即用」）⇒ 这条红 ✓。
    #[test]
    fn cl_plus_te_is_rejected_as_a_smuggling_shape() {
        let raw = b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 13\r\n\
             Transfer-Encoding: chunked\r\n\r\n{\"size\": 4.0}"
            .to_vec();
        let error = Request::read(&mut std::io::Cursor::new(raw)).expect_err("CL+TE 必须报错");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("同时出现"), "{error}");
    }

    /// **重复 Content-Length 一律拒绝** ✓（审计 P2-N2 回归判据）。
    /// 相等值也拒 ✓：取首（nginx 形状）与本解析器旧取尾（BTreeMap 覆盖形状）
    /// 对 body 边界的判断分歧正是走私面 ⇒ 唯一安全答案是**都不取** ✓。
    /// 变异（回退成 BTreeMap 静默取后值）⇒ 这条红 ✓。
    #[test]
    fn duplicate_content_length_is_rejected_even_when_equal() {
        let equal = b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\
             Content-Length: 5\r\n\r\nhello"
            .to_vec();
        let error =
            Request::read(&mut std::io::Cursor::new(equal)).expect_err("重复 CL（相等）必须报错");
        assert!(
            error.to_string().contains("重复的 content-length"),
            "{error}"
        );

        let conflicting = b"POST / HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\n\
             Content-Length: 13\r\n\r\nhello"
            .to_vec();
        let error = Request::read(&mut std::io::Cursor::new(conflicting))
            .expect_err("重复 CL（冲突）必须报错");
        assert!(
            error.to_string().contains("重复的 content-length"),
            "{error}"
        );
    }

    /// 单个 CL 的哨兵 ⇒ 照旧按声明长度读请求体 ✓（互斥闸只拦「同现」✓）。
    #[test]
    fn single_content_length_still_parses() {
        let request = parse(
            "POST /api/tools/draw_stroke HTTP/1.1\r\nContent-Length: 13\r\n\r\n{\"size\": 4.0}",
        );
        assert_eq!(request.body.len(), 13);
    }
}
