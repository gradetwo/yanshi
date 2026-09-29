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
        let mut request_line = String::new();
        loop {
            request_line.clear();
            match reader.read_line(&mut request_line) {
                Ok(0) => return Ok(None),
                Ok(_) => {}
                Err(error) => return Err(error),
            }
            if request_line != "\r\n" && request_line != "\n" {
                break;
            }
        }
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
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line)?;
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
            if let Some((name, value)) = trimmed.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
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

/// 百分号解码（非法序列原样保留）。
pub fn percent_decode(text: &str) -> String {
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
            b'+' => {
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
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n",
            self.status,
            self.reason(),
            self.content_type,
            self.body.len(),
            if keep_alive { "keep-alive" } else { "close" }
        );
        for (name, value) in &self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        writer.write_all(head.as_bytes())?;
        writer.write_all(&self.body)?;
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
}
