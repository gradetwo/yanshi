//! HTTP 路由与 WebSocket 会话（设计文档 3 章协议、6.8 广播、10 章工具、12.7 鉴权）。
//!
//! 线程模型：一个 accept 循环 + 每连接一个线程（上限可配）；WebSocket 连接额外起一个
//! 推送线程，按 6.8 的边界把控制流（全部原子元数据）与视口内的数据流（tile/缩略图）
//! 发给浏览器。
//!
//! 路由：
//!
//! | 方法 | 路径 | 鉴权 | 说明 |
//! |---|---|---|---|
//! | GET | `/health` | 无 | 存活与统计 |
//! | GET | `/` | 无 | 最小 Web 查看器 |
//! | GET | `/api/documents` | 无 | 文档列表（仅元数据） |
//! | POST | `/api/documents` | 无 | 打开/新建文档，返回 capability token 与 URL |
//! | GET | `/api/documents/{id}` | token | 文档摘要 |
//! | POST | `/api/tools/{name}?doc=..` | token | 工具调用（10.1 / 5.7） |
//! | GET | `/api/blob/{hash}?doc=..` | token | 取回 CAS 中的 PNG |
//! | GET | `/ws?doc=..` | token | WebSocket 升级与推送 |

use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};
use yanshi_core::{Bbox, ErrorCode, ErrorContext, YanshiError};
use yanshi_server::broadcast::PushChannel;
use yanshi_server::token::{Role, TransportKind};
use yanshi_server::tools::{Profile, ToolContext, ToolRegistry};
use yanshi_server::{DocumentSettings, NewDocument, Workspace};

use crate::http::{Request, Response};
use crate::viewer;
use crate::ws::{accept_key, is_valid_client_key, read_message, write_frame, Frame, OpCode};

/// HTTP 服务选项。
#[derive(Debug, Clone)]
pub struct HttpOptions {
    /// 监听地址（`127.0.0.1:0` 表示随机端口，测试用）。
    pub bind: String,
    /// 持久化根目录。
    pub root: Option<PathBuf>,
    /// 默认文档 id。
    pub doc_id: String,
    /// 自动创建文档的尺寸。
    pub width: u32,
    /// 高。
    pub height: u32,
    /// 启用的工具组。
    pub profiles: Vec<Profile>,
    /// 并发连接上限。
    pub max_connections: usize,
    /// WebSocket 推送轮询间隔（毫秒）。
    pub push_interval_ms: u64,
    /// 是否在工具响应里把 `yanshi://blob/<hash>` 改写成可直接访问的 URL。
    pub rewrite_blob_urls: bool,
    /// WASM 计算内核产物目录（`/wasm/*` 从这里取；`None` 表示不提供）。
    pub wasm_dir: Option<PathBuf>,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".to_owned(),
            root: None,
            doc_id: "default".to_owned(),
            width: 1024,
            height: 1024,
            profiles: vec![
                Profile::Core,
                Profile::History,
                Profile::Annotation,
                Profile::Collab,
                Profile::Structure,
            ],
            max_connections: 64,
            push_interval_ms: 25,
            rewrite_blob_urls: true,
            // 默认指向仓库内 `wasm-bindgen --target web` 的输出目录（相对当前工作目录）。
            wasm_dir: Some(PathBuf::from("crates/yanshi-wasm/pkg")),
        }
    }
}

impl HttpOptions {
    /// 解析命令行参数。
    pub fn parse_args<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut options = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            let arg = arg.as_ref();
            let mut value_of = |name: &str| -> Result<String, String> {
                iter.next()
                    .map(|value| value.as_ref().to_owned())
                    .ok_or_else(|| format!("{name} 需要一个取值"))
            };
            match arg {
                "--bind" => options.bind = value_of("--bind")?,
                "--wasm-dir" => options.wasm_dir = Some(value_of("--wasm-dir")?.into()),
                "--no-wasm" => options.wasm_dir = None,
                "--root" => options.root = Some(value_of("--root")?.into()),
                "--doc" => options.doc_id = value_of("--doc")?,
                "--width" => {
                    options.width = value_of("--width")?
                        .parse()
                        .map_err(|_| "--width 必须是整数".to_owned())?
                }
                "--height" => {
                    options.height = value_of("--height")?
                        .parse()
                        .map_err(|_| "--height 必须是整数".to_owned())?
                }
                "--profile" | "--profiles" => {
                    let list = value_of("--profile")?;
                    let mut profiles = Vec::new();
                    for name in list
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                    {
                        profiles.push(Profile::parse(name).map_err(|error| error.to_string())?);
                    }
                    if !profiles.is_empty() {
                        options.profiles = profiles;
                    }
                }
                other => return Err(format!("未知参数 {other}")),
            }
        }
        Ok(options)
    }

    /// 帮助文本。
    pub const fn help() -> &'static str {
        "yanshi-serve —— 偃师 Yanshi 零依赖 HTTP/WebSocket 服务端\n\
         \n\
         用法：yanshi-serve [选项]\n\
         \n\
         选项：\n\
           --bind <addr>      监听地址（缺省 127.0.0.1:8080）\n\
           --root <dir>       持久化根目录（缺省纯内存）\n\
           --doc <id>         默认文档 id（缺省 default）\n\
           --width <n>        自动创建文档的宽（缺省 1024）\n\
           --height <n>       自动创建文档的高（缺省 1024）\n\
           --profile <list>   启用工具组，逗号分隔\n\
           --wasm-dir <dir>   WASM 计算内核产物目录（缺省 crates/yanshi-wasm/pkg）\n\
           --no-wasm          不提供浏览器端 WASM 计算内核（查看器退化为服务端渲染）\n\
           --help             显示帮助\n\
         \n\
         打开 http://127.0.0.1:8080/ 使用最小 Web 查看器（URL 中的 token 即文档 capability）。\n"
    }
}

/// 共享服务状态。
pub struct ServerState {
    /// 工作区。
    pub workspace: Mutex<Workspace>,
    /// 工具注册表。
    pub registry: ToolRegistry,
    /// 选项。
    pub options: HttpOptions,
    /// 启动时间。
    pub started_at: i64,
    /// 已处理请求数。
    pub requests: AtomicU64,
    /// 当前连接数。
    pub connections: AtomicUsize,
    /// 关闭标志。
    pub shutdown: AtomicBool,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerState")
            .field(
                "documents",
                &self.workspace.lock().map(|w| w.len()).unwrap_or(0),
            )
            .field("requests", &self.requests.load(Ordering::Relaxed))
            .finish()
    }
}

/// 已启动的服务句柄。
pub struct ServerHandle {
    /// 实际监听地址。
    pub addr: SocketAddr,
    state: Arc<ServerState>,
    accept_thread: Option<thread::JoinHandle<()>>,
}

impl ServerHandle {
    /// 基址（如 `http://127.0.0.1:8080`）。
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// 共享状态（测试可直接读写工作区）。
    pub fn state(&self) -> Arc<ServerState> {
        Arc::clone(&self.state)
    }

    /// 已处理请求数。
    pub fn requests(&self) -> u64 {
        self.state.requests.load(Ordering::Relaxed)
    }

    /// 关闭：置标志并唤醒 accept 循环。
    pub fn shutdown(mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);
        // 触发一次连接以唤醒 accept。
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.state.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(handle) = self.accept_thread.take() {
            let _ = handle.join();
        }
    }
}

/// 启动服务（`bind` 支持端口 0，便于测试）。
pub fn serve(options: HttpOptions) -> std::io::Result<ServerHandle> {
    let settings = DocumentSettings::default();
    let workspace = match &options.root {
        Some(root) => Workspace::with_file_store(root.clone(), settings.clone())
            .unwrap_or_else(|_| Workspace::in_memory(settings)),
        None => Workspace::in_memory(settings),
    };
    let registry = ToolRegistry::with_profiles(&options.profiles);
    let state = Arc::new(ServerState {
        workspace: Mutex::new(workspace),
        registry,
        options: options.clone(),
        started_at: yanshi_core::now_ms(),
        requests: AtomicU64::new(0),
        connections: AtomicUsize::new(0),
        shutdown: AtomicBool::new(false),
    });

    let listener = TcpListener::bind(&options.bind)?;
    let addr = listener.local_addr()?;
    listener.set_nonblocking(false)?;

    let accept_state = Arc::clone(&state);
    let accept_thread = thread::spawn(move || {
        for incoming in listener.incoming() {
            if accept_state.shutdown.load(Ordering::SeqCst) {
                break;
            }
            let Ok(stream) = incoming else { continue };
            if accept_state.connections.load(Ordering::SeqCst)
                >= accept_state.options.max_connections
            {
                let mut stream = stream;
                let response = Response::from_error(&YanshiError::new(
                    ErrorCode::ResourceExhausted,
                    ErrorContext::detail("并发连接数已达上限"),
                ));
                let _ = response.write(&mut stream, false);
                continue;
            }
            let state = Arc::clone(&accept_state);
            accept_state.connections.fetch_add(1, Ordering::SeqCst);
            thread::spawn(move || {
                let _ = handle_connection(state.clone(), stream);
                state.connections.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });

    Ok(ServerHandle {
        addr,
        state,
        accept_thread: Some(accept_thread),
    })
}

fn handle_connection(state: Arc<ServerState>, stream: TcpStream) -> std::io::Result<()> {
    stream.set_nodelay(true).ok();
    let mut reader = BufReader::new(stream.try_clone()?);
    let Some(request) = Request::read(&mut reader)? else {
        return Ok(());
    };
    state.requests.fetch_add(1, Ordering::Relaxed);
    let mut writer = stream;
    if request.is_websocket_upgrade() {
        return handle_websocket(state, request, writer, reader);
    }
    let keep_alive = request
        .header("connection")
        .map(|value| value.eq_ignore_ascii_case("keep-alive"))
        .unwrap_or(false);
    let response = route(&state, &request);
    response.write(&mut writer, keep_alive)
}

/// 路由（`Request` → `Response`）。
pub fn route(state: &ServerState, request: &Request) -> Response {
    let trimmed = request.path.trim_end_matches('/');
    let path = if trimmed.is_empty() { "/" } else { trimmed };
    let method = request.method.as_str();

    if path == "/health" {
        return match method {
            "GET" => health(state),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/" {
        return match method {
            "GET" => Response::html(viewer::PAGE),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/api/documents" {
        return match method {
            "GET" => list_documents(state),
            "POST" => create_document(state, request),
            _ => method_not_allowed(request, "GET, POST"),
        };
    }
    if path == "/api/tools" {
        return match method {
            "POST" => tool_route(state, request),
            _ => method_not_allowed(request, "POST"),
        };
    }
    if path == "/api/blob" {
        return match method {
            "GET" => blob_route(state, request),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/api/atoms" {
        return match method {
            // 客户端自带 ULID 的原子提交（12.2 幂等与重试安全）。
            "POST" => atom_submit(state, request),
            _ => method_not_allowed(request, "POST"),
        };
    }
    if let Some(file) = path.strip_prefix("/wasm/") {
        return match method {
            "GET" => wasm_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(id) = path.strip_prefix("/api/documents/") {
        return match method {
            "GET" => document_summary(state, request, id),
            "DELETE" => close_document(state, request, id),
            _ => method_not_allowed(request, "GET, DELETE"),
        };
    }
    if let Some(hash) = path.strip_prefix("/api/blob/") {
        return match method {
            "GET" => blob_by_hash(state, request, hash),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(name) = path.strip_prefix("/api/tools/") {
        return match method {
            "POST" => tool_call(state, request, name),
            _ => method_not_allowed(request, "POST"),
        };
    }
    if path == "/ws" {
        return Response::from_error(&YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("请使用 ws:// 与 Upgrade: websocket 连接 /ws"),
        ));
    }
    crate::http::not_found(format!("未知路由 {} {path}", method))
}

fn health(state: &ServerState) -> Response {
    let documents = state
        .workspace
        .lock()
        .map(|workspace| workspace.len())
        .unwrap_or(0);
    Response::json(
        200,
        &json!({
            "ok": true,
            "documents": documents,
            "requests": state.requests.load(Ordering::Relaxed),
            "connections": state.connections.load(Ordering::Relaxed),
            "uptime_ms": yanshi_core::now_ms() - state.started_at,
            "profiles": state.registry.profiles().iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "tools": state.registry.len(),
            "wasm": wasm_available(state),
        }),
    )
}

fn list_documents(state: &ServerState) -> Response {
    let Ok(workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    match workspace.list_documents() {
        Ok(documents) => Response::json(200, &json!({"ok": true, "documents": documents})),
        Err(error) => Response::from_error(&error),
    }
}

fn create_document(state: &ServerState, request: &Request) -> Response {
    let body = match request.json() {
        Ok(body) => body,
        Err(error) => return crate::http::bad_request(format!("请求体不是合法 JSON：{error}")),
    };
    let doc_id = body
        .get("doc_id")
        .and_then(Value::as_str)
        .unwrap_or(&state.options.doc_id)
        .to_owned();
    let width = body
        .get("width")
        .and_then(Value::as_u64)
        .unwrap_or(state.options.width as u64) as u32;
    let height = body
        .get("height")
        .and_then(Value::as_u64)
        .unwrap_or(state.options.height as u64) as u32;
    let actor = body
        .get("actor")
        .and_then(Value::as_str)
        .unwrap_or("human:web")
        .to_owned();

    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let spec = NewDocument::new(doc_id.clone(), width, height);
    if let Err(error) = workspace.open_or_create(spec, &actor, "session:web") {
        return Response::from_error(&error);
    }
    // 12.7：打开文档返回内嵌 token 的 URL。
    let token = match workspace.issue_token(&doc_id, &actor, Role::Editor) {
        Ok(token) => token,
        Err(error) => return Response::from_error(&error),
    };
    let summary = workspace
        .summary_json(&doc_id)
        .unwrap_or_else(|_| json!({"doc_id": doc_id}));
    Response::json(
        200,
        &json!({
            "ok": true,
            "doc_id": doc_id,
            "token": token.as_str(),
            "url": format!("/?doc={doc_id}&token={token}"),
            "document": summary,
        }),
    )
}

fn authorize(
    state: &ServerState,
    request: &Request,
    doc_id: &str,
) -> Result<yanshi_server::Principal, Response> {
    let token = request
        .token()
        .map(|text| yanshi_server::CapabilityToken::parse(&text))
        .transpose()
        .map_err(|error| Response::from_error(&error))?;
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| internal("工作区锁中毒"))?;
    workspace
        .authorize(doc_id, token.as_ref(), TransportKind::Http, "local:http")
        .map_err(|error| Response::from_error(&error))
}

fn document_summary(state: &ServerState, request: &Request, doc_id: &str) -> Response {
    if let Err(response) = authorize(state, request, doc_id) {
        return response;
    }
    let Ok(workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    match workspace.summary_json(doc_id) {
        Ok(summary) => Response::json(200, &json!({"ok": true, "document": summary})),
        Err(error) => Response::from_error(&error),
    }
}

fn close_document(state: &ServerState, request: &Request, doc_id: &str) -> Response {
    if let Err(response) = authorize(state, request, doc_id) {
        return response;
    }
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let closed = workspace.close_document(doc_id);
    Response::json(200, &json!({"ok": true, "closed": closed}))
}

fn blob_route(state: &ServerState, request: &Request) -> Response {
    match request.param("hash") {
        Some(hash) => blob_by_hash(state, request, hash),
        None => crate::http::bad_request("缺少 blob hash"),
    }
}

fn blob_by_hash(state: &ServerState, request: &Request, hash: &str) -> Response {
    let doc_id = match doc_param(request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response,
    };
    if let Err(response) = authorize(state, request, &doc_id) {
        return response;
    }
    let Ok(hash) = hash.parse::<yanshi_core::BlobHash>() else {
        return crate::http::bad_request("非法 blob hash");
    };
    let Ok(workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let store = workspace.store();
    if !store.exists(&hash) {
        return crate::http::not_found(format!("blob {hash} 不存在"));
    }
    match store.get(&hash) {
        Ok(bytes) => Response::bytes(200, "image/png", bytes)
            .with_header("Cache-Control", "public, max-age=31536000, immutable"),
        Err(error) => Response::from_error(&error),
    }
}

fn tool_route(state: &ServerState, request: &Request) -> Response {
    // `POST /api/tools` 用请求体指定工具名（等价于 /api/tools/{name}）。
    let Ok(body) = request.json() else {
        return crate::http::bad_request("请求体不是合法 JSON");
    };
    let Some(name) = body.get("tool").and_then(Value::as_str) else {
        return crate::http::bad_request("缺少 tool 字段");
    };
    let name = name.to_owned();
    let arguments = body
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| body.clone());
    tool_call_with(state, request, &name, arguments)
}

fn tool_call(state: &ServerState, request: &Request, name: &str) -> Response {
    let arguments = match request.json() {
        Ok(value) => value,
        Err(error) => return crate::http::bad_request(format!("请求体不是合法 JSON：{error}")),
    };
    tool_call_with(state, request, name, arguments)
}

fn tool_call_with(
    state: &ServerState,
    request: &Request,
    name: &str,
    mut arguments: Value,
) -> Response {
    if !arguments.is_object() {
        return crate::http::bad_request("arguments 必须是 JSON 对象");
    }
    let doc_id = request
        .param("doc")
        .map(str::to_owned)
        .or_else(|| {
            arguments
                .get("doc_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| state.options.doc_id.clone());
    let principal = match authorize(state, request, &doc_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if arguments.get("doc_id").is_none() {
        arguments["doc_id"] = json!(doc_id);
    }

    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    // 文档尚未打开时按需创建（与 MCP 一致）。
    if workspace.document(&doc_id).is_none() {
        let spec = NewDocument::new(doc_id.clone(), state.options.width, state.options.height);
        if let Err(error) = workspace.open_or_create(spec, &principal.actor, "session:http") {
            return Response::from_error(&error);
        }
    }
    let owner = principal.role.can_revert_others();
    let mut context = ToolContext::new(
        &mut workspace,
        doc_id.clone(),
        principal.actor.clone(),
        "session:http",
    )
    .with_owner(owner)
    .with_wait_for_render(true, 500);
    let value = state.registry.call(&mut context, name, &arguments);
    let value = if state.options.rewrite_blob_urls {
        rewrite_blob_urls(value, &doc_id, request.token().as_deref())
    } else {
        value
    };
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        // 工具级错误：HTTP 状态按 5.7 错误码映射，响应体保持 10.1/5.7 形状。
        let status = match value.get("error_code").and_then(Value::as_str) {
            Some("invalid_argument") => 400,
            Some("permission_denied") => 403,
            Some("reference_not_found") | Some("job_not_found") => 404,
            Some("precondition_failed") | Some("conflict") => 409,
            Some("resource_exhausted") | Some("degraded") => 503,
            Some("job_pending") => 202,
            _ => 400,
        };
        return Response::json(status, &value);
    }
    Response::json(200, &value)
}

/// 把 `yanshi://blob/<hash>` 改写成可直接 GET 的 URL（带 doc 与 token，12.7）。
fn rewrite_blob_urls(value: Value, doc_id: &str, token: Option<&str>) -> Value {
    fn rewrite_string(text: &str, doc_id: &str, token: Option<&str>) -> Option<String> {
        let hash = text.strip_prefix("yanshi://blob/")?;
        let mut url = format!("/api/blob/{hash}?doc={doc_id}");
        if let Some(token) = token {
            url.push_str(&format!("&token={token}"));
        }
        Some(url)
    }
    fn walk(value: Value, doc_id: &str, token: Option<&str>) -> Value {
        match value {
            Value::String(text) => match rewrite_string(&text, doc_id, token) {
                Some(url) => Value::String(url),
                None => Value::String(text),
            },
            Value::Array(items) => Value::Array(
                items
                    .into_iter()
                    .map(|item| walk(item, doc_id, token))
                    .collect(),
            ),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(key, item)| (key, walk(item, doc_id, token)))
                    .collect(),
            ),
            other => other,
        }
    }
    walk(value, doc_id, token)
}

/// 取 `doc` 参数（缺失或为空都算参数错误）。
fn doc_param(request: &Request) -> Result<String, Response> {
    match request.param("doc") {
        Some(doc) if !doc.trim().is_empty() => Ok(doc.to_owned()),
        _ => Err(crate::http::bad_request("缺少 doc 参数")),
    }
}

/// WASM 产物是否可用（`/health` 与查看器据此决定是否启用本地乐观渲染）。
fn wasm_available(state: &ServerState) -> bool {
    state
        .options
        .wasm_dir
        .as_ref()
        .map(|dir| {
            dir.join("yanshi_wasm.js").is_file() && dir.join("yanshi_wasm_bg.wasm").is_file()
        })
        .unwrap_or(false)
}

/// 只允许取 `wasm-bindgen --target web` 生成的白名单文件，避免路径穿越。
fn wasm_asset(state: &ServerState, file: &str) -> Response {
    const ALLOWED: [(&str, &str); 4] = [
        ("yanshi_wasm.js", "text/javascript; charset=utf-8"),
        ("yanshi_wasm_bg.wasm", "application/wasm"),
        ("yanshi_wasm.d.ts", "text/plain; charset=utf-8"),
        ("yanshi_wasm_bg.wasm.d.ts", "text/plain; charset=utf-8"),
    ];
    if file.contains("..") || file.contains('/') {
        return crate::http::bad_request("非法资源名");
    }
    let Some((_, content_type)) = ALLOWED.iter().find(|(name, _)| *name == file) else {
        return crate::http::not_found(format!("未知 WASM 资源 {file}"));
    };
    let Some(dir) = &state.options.wasm_dir else {
        return crate::http::not_found("服务端未启用 WASM 计算内核（--no-wasm）");
    };
    let path = dir.join(file);
    match std::fs::read(&path) {
        Ok(bytes) => Response::bytes(200, content_type, bytes)
            .with_header("Cache-Control", "no-cache"),
        Err(_) => crate::http::not_found(format!(
            "WASM 产物缺失：{}（先运行 cargo build -p yanshi-wasm --target wasm32-unknown-unknown --release 与 wasm-bindgen）",
            path.display()
        )),
    }
}

/// `POST /api/atoms?doc=<id>`：客户端构造的原子（自带 ULID）直接提交。
///
/// 与工具路径的区别：**id 由客户端生成**，重复提交同一 id 幂等命中（12.2），
/// 客户端因此可以安全重试；这是 WASM 侧本地乐观渲染的服务端入口。
fn atom_submit(state: &ServerState, request: &Request) -> Response {
    let doc_id = match doc_param(request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response,
    };
    let principal = match authorize(state, request, &doc_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let atom: yanshi_core::Atom = match serde_json::from_slice(&request.body) {
        Ok(atom) => atom,
        Err(error) => {
            return crate::http::bad_request(format!("原子不是合法 JSON（5.1 形状）：{error}"))
        }
    };
    if atom.id.is_empty() {
        return crate::http::bad_request("原子缺少客户端生成的 id（ULID）");
    }
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    if workspace.document(&doc_id).is_none() {
        let spec = NewDocument::new(doc_id.clone(), state.options.width, state.options.height);
        if let Err(error) = workspace.open_or_create(spec, &principal.actor, "session:http") {
            return Response::from_error(&error);
        }
    }
    let owner = principal.role.can_revert_others();
    match workspace.commit(&doc_id, atom, &principal.actor, owner) {
        Ok(result) => {
            let value = yanshi_server::tools::commit_response(&result, None, None, Vec::new());
            let value = rewrite_blob_urls(value, &doc_id, request.token().as_deref());
            Response::json(200, &value)
        }
        Err(error) => Response::from_error(&error),
    }
}

fn method_not_allowed(request: &Request, allow: &str) -> Response {
    Response::json(
        405,
        &json!({
            "ok": false,
            "error_code": "invalid_argument",
            "retryable": false,
            "context": {"detail": format!("{} 不允许 {}", request.path, request.method.as_str())},
        }),
    )
    .with_header("Allow", allow)
}

fn internal(detail: &str) -> Response {
    Response::from_error(&YanshiError::new(
        ErrorCode::ResourceExhausted,
        ErrorContext::detail(detail),
    ))
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

fn handle_websocket(
    state: Arc<ServerState>,
    request: Request,
    mut writer: TcpStream,
    mut reader: BufReader<TcpStream>,
) -> std::io::Result<()> {
    let key = request
        .header("sec-websocket-key")
        .unwrap_or_default()
        .to_owned();
    if !is_valid_client_key(&key) {
        return Response::from_error(&YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail("非法 Sec-WebSocket-Key"),
        ))
        .write(&mut writer, false);
    }
    let doc_id = match doc_param(&request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response.write(&mut writer, false),
    };
    let principal = match authorize(&state, &request, &doc_id) {
        Ok(principal) => principal,
        Err(response) => return response.write(&mut writer, false),
    };

    let accept = accept_key(&key);
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    writer.write_all(handshake.as_bytes())?;
    writer.flush()?;
    writer.set_nodelay(true).ok();

    // 订阅推送（6.8）：控制流全局、数据流按视口过滤。
    let session = format!("session:ws:{}", yanshi_core::Ulid::new().encode());
    let subscriber = {
        let Ok(mut workspace) = state.workspace.lock() else {
            return Ok(());
        };
        if workspace.document(&doc_id).is_none() {
            let spec = NewDocument::new(doc_id.clone(), state.options.width, state.options.height);
            if workspace
                .open_or_create(spec, &principal.actor, &session)
                .is_err()
            {
                return Ok(());
            }
        }
        match workspace.document_mut(&doc_id) {
            Ok(document) => document.subscribe(&session, PushChannel::WebSocket),
            Err(_) => return Ok(()),
        }
    };

    let closed = Arc::new(AtomicBool::new(false));
    let write_stream = Arc::new(Mutex::new(writer.try_clone()?));

    // 推送线程：把广播器里排队的事件写给客户端（含其他客户端触发的变更）。
    let push_state = Arc::clone(&state);
    let push_closed = Arc::clone(&closed);
    let push_writer = Arc::clone(&write_stream);
    let push_doc = doc_id.clone();
    let interval = Duration::from_millis(state.options.push_interval_ms.max(5));
    let pusher = thread::spawn(move || {
        while !push_closed.load(Ordering::SeqCst) && !push_state.shutdown.load(Ordering::SeqCst) {
            thread::sleep(interval);
            let events = {
                let Ok(mut workspace) = push_state.workspace.lock() else {
                    break;
                };
                let Ok(document) = workspace.document_mut(&push_doc) else {
                    break;
                };
                document.broadcaster_mut().drain(subscriber)
            };
            if events.is_empty() {
                continue;
            }
            let Ok(mut stream) = push_writer.lock() else {
                break;
            };
            for event in events {
                let payload = json!({"type": "event", "event": event}).to_string();
                if write_frame(&mut *stream, &Frame::text(payload)).is_err() {
                    push_closed.store(true, Ordering::SeqCst);
                    break;
                }
            }
        }
    });

    // 读循环：处理客户端消息（工具调用、订阅、ping）。
    loop {
        if state.shutdown.load(Ordering::SeqCst) {
            break;
        }
        let message = match read_message(&mut reader, &mut writer) {
            Ok(message) => message,
            Err(_) => break,
        };
        let Some((opcode, payload)) = message else {
            break;
        };
        if opcode == OpCode::Close {
            break;
        }
        let Ok(text) = String::from_utf8(payload) else {
            break;
        };
        let Ok(request_value) = serde_json::from_str::<Value>(&text) else {
            let _ = send_json(
                &write_stream,
                &json!({"type": "error", "error": {"code": "invalid_argument", "message": "消息不是 JSON"}}),
            );
            continue;
        };
        handle_ws_message(
            &state,
            &write_stream,
            &doc_id,
            &principal.actor,
            subscriber,
            &request_value,
        );
    }

    closed.store(true, Ordering::SeqCst);
    let _ = pusher.join();
    if let Ok(mut workspace) = state.workspace.lock() {
        if let Ok(document) = workspace.document_mut(&doc_id) {
            document.broadcaster_mut().unsubscribe(subscriber);
        }
    }
    Ok(())
}

fn handle_ws_message(
    state: &Arc<ServerState>,
    write_stream: &Arc<Mutex<TcpStream>>,
    doc_id: &str,
    actor: &str,
    subscriber: u64,
    message: &Value,
) {
    let kind = message.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "ping" => {
            let _ = send_json(write_stream, &json!({"type": "pong"}));
        }
        "subscribe" => {
            let viewport = message.get("viewport").and_then(Bbox::from_value);
            let zoom = message.get("zoom").and_then(Value::as_f64).unwrap_or(1.0);
            let applied = state
                .workspace
                .lock()
                .ok()
                .and_then(|mut workspace| {
                    workspace.document_mut(doc_id).ok().map(|document| {
                        document
                            .broadcaster_mut()
                            .set_viewport(subscriber, viewport, zoom)
                    })
                })
                .unwrap_or(false);
            let _ = send_json(
                write_stream,
                &json!({
                    "type": "subscribed",
                    "doc_id": doc_id,
                    "viewport": message.get("viewport"),
                    "zoom": zoom,
                    "applied": applied,
                }),
            );
        }
        "tool" | "call" => {
            let request_id = message.get("request_id").cloned().unwrap_or(Value::Null);
            let name = message.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = message
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let result = {
                let Ok(mut workspace) = state.workspace.lock() else {
                    let _ = send_json(
                        write_stream,
                        &json!({"type": "error", "request_id": request_id,
                                "error": {"code": "resource_exhausted", "message": "工作区锁中毒"}}),
                    );
                    return;
                };
                let mut context = ToolContext::new(&mut workspace, doc_id, actor, "session:ws")
                    .with_owner(true)
                    .with_wait_for_render(true, 500);
                state.registry.call(&mut context, name, &arguments)
            };
            let result = rewrite_blob_urls_value(result, doc_id, state.options.rewrite_blob_urls);
            let _ = send_json(
                write_stream,
                &json!({"type": "ack", "request_id": request_id, "result": result}),
            );
        }
        other => {
            let _ = send_json(
                write_stream,
                &json!({"type": "error", "error": {"code": "invalid_argument", "message": format!("未知消息类型 {other}")}}),
            );
        }
    }
}

fn rewrite_blob_urls_value(value: Value, doc_id: &str, rewrite: bool) -> Value {
    if rewrite {
        rewrite_blob_urls(value, doc_id, None)
    } else {
        value
    }
}

fn send_json(stream: &Arc<Mutex<TcpStream>>, value: &Value) -> std::io::Result<()> {
    let Ok(mut stream) = stream.lock() else {
        return Ok(());
    };
    write_frame(&mut *stream, &Frame::text(value.to_string()))
}

/// 事件类型名（测试与日志用）。
pub fn event_kind(event: &yanshi_server::BroadcastEvent) -> &'static str {
    match event {
        yanshi_server::BroadcastEvent::Atom { .. } => "atom",
        yanshi_server::BroadcastEvent::Tiles { .. } => "tiles",
        yanshi_server::BroadcastEvent::Thumbnail { .. } => "thumbnail",
        yanshi_server::BroadcastEvent::JobFinished { .. } => "job_finished",
        yanshi_server::BroadcastEvent::Annotation { .. } => "annotation",
    }
}

/// 便于测试：把响应体解析成 JSON。
pub fn body_json(response: &Response) -> Value {
    serde_json::from_slice(&response.body).unwrap_or(Value::Null)
}

/// 便于测试与文档：路由表。
pub fn routes() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("GET /health", "存活与统计"),
        ("GET /", "最小 Web 查看器"),
        ("GET /api/documents", "文档列表（仅元数据）"),
        (
            "POST /api/documents",
            "打开/新建文档并返回 capability token",
        ),
        ("GET /api/documents/{id}", "文档摘要（需 token）"),
        ("DELETE /api/documents/{id}", "关闭文档（需 token）"),
        ("POST /api/tools/{name}?doc=", "工具调用（需 token）"),
        ("POST /api/tools", "工具调用（body 里带 tool）"),
        ("GET /api/blob/{hash}?doc=", "取回 CAS 中的 PNG（需 token）"),
        (
            "POST /api/atoms?doc=",
            "客户端自带 ULID 的原子提交（12.2 幂等，需 token）",
        ),
        ("GET /wasm/{file}", "WASM 计算内核产物（js/wasm）"),
        ("GET /ws?doc=&token=", "WebSocket 升级与推送（需 token）"),
    ])
}

/// 便于工具层：`Role` 是否允许跨 actor revert（12.5）。
pub fn owner_from_role(role: Role) -> bool {
    role.can_revert_others()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ServerState {
        let settings = DocumentSettings::default();
        ServerState {
            workspace: Mutex::new(Workspace::in_memory(settings)),
            registry: ToolRegistry::full(),
            options: HttpOptions {
                width: 32,
                height: 32,
                ..HttpOptions::default()
            },
            started_at: yanshi_core::now_ms(),
            requests: AtomicU64::new(0),
            connections: AtomicUsize::new(0),
            shutdown: AtomicBool::new(false),
        }
    }

    fn request(method: &str, target: &str) -> Request {
        let raw = format!("{method} {target} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        Request::read(&mut std::io::Cursor::new(raw.into_bytes()))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn health_and_viewer_do_not_require_tokens() {
        let state = state();
        let health = route(&state, &request("GET", "/health"));
        assert_eq!(health.status, 200);
        assert_eq!(body_json(&health)["ok"], json!(true));
        assert!(body_json(&health)["tools"].as_u64().unwrap() > 27);

        let viewer = route(&state, &request("GET", "/"));
        assert_eq!(viewer.status, 200);
        assert!(viewer.content_type.starts_with("text/html"));
        assert!(String::from_utf8_lossy(&viewer.body).contains("偃师"));
    }

    #[test]
    fn document_creation_returns_token_and_url() {
        let state = state();
        let mut create = request("POST", "/api/documents");
        create.body = br#"{"doc_id":"doc_http","width":48,"height":48}"#.to_vec();
        create
            .headers
            .insert("content-length".to_owned(), create.body.len().to_string());
        let response = route(&state, &create);
        assert_eq!(response.status, 200);
        let body = body_json(&response);
        assert_eq!(body["doc_id"], json!("doc_http"));
        let token = body["token"].as_str().unwrap().to_owned();
        assert_eq!(token.len(), 64);
        assert!(body["url"].as_str().unwrap().contains("token="));
        assert_eq!(body["document"]["width"], json!(48));

        // 再次打开同一文档同样返回 token（幂等）。
        let again = route(&state, &create);
        assert_eq!(body_json(&again)["doc_id"], json!("doc_http"));
    }

    #[test]
    fn tools_require_valid_tokens_and_reject_unknown_ones() {
        let state = state();
        let mut create = request("POST", "/api/documents");
        create.body = br#"{"doc_id":"doc_auth"}"#.to_vec();
        let body = body_json(&route(&state, &create));
        let token = body["token"].as_str().unwrap().to_owned();

        // 无 token。
        let mut no_token = request("POST", "/api/tools/create_layer?doc=doc_auth");
        no_token.body = br#"{"layer_id":"layer_1"}"#.to_vec();
        let response = route(&state, &no_token);
        assert_eq!(response.status, 403);
        assert_eq!(
            body_json(&response)["error_code"],
            json!("permission_denied")
        );

        // 错误 token。
        let bogus = "0".repeat(64);
        let mut bad = request(
            "POST",
            &format!("/api/tools/create_layer?doc=doc_auth&token={bogus}"),
        );
        bad.body = br#"{"layer_id":"layer_1"}"#.to_vec();
        assert_eq!(route(&state, &bad).status, 403);

        // 正确 token。
        let mut good = request(
            "POST",
            &format!("/api/tools/create_layer?doc=doc_auth&token={token}"),
        );
        good.body = br#"{"layer_id":"layer_1"}"#.to_vec();
        let response = route(&state, &good);
        assert_eq!(response.status, 200, "{:?}", body_json(&response));
        assert_eq!(body_json(&response)["ok"], json!(true));
    }

    #[test]
    fn tool_errors_map_to_5_7_body_and_http_status() {
        let state = state();
        let mut create = request("POST", "/api/documents");
        create.body = br#"{"doc_id":"doc_err"}"#.to_vec();
        let token = body_json(&route(&state, &create))["token"]
            .as_str()
            .unwrap()
            .to_owned();

        let mut call = request(
            "POST",
            &format!("/api/tools/delete_object?doc=doc_err&token={token}"),
        );
        call.body = br#"{"object_id":"obj_missing"}"#.to_vec();
        let response = route(&state, &call);
        assert_eq!(response.status, 404);
        let body = body_json(&response);
        assert_eq!(body["error_code"], json!("reference_not_found"));

        // 缺少必填参数 → 400。
        let mut missing = request(
            "POST",
            &format!("/api/tools/delete_object?doc=doc_err&token={token}"),
        );
        missing.body = br#"{}"#.to_vec();
        assert_eq!(route(&state, &missing).status, 400);

        // 未知工具 → 400 invalid_argument。
        let mut unknown = request(
            "POST",
            &format!("/api/tools/inpaint_region?doc=doc_err&token={token}"),
        );
        unknown.body = br#"{}"#.to_vec();
        let response = route(&state, &unknown);
        assert_eq!(response.status, 400);
        assert!(body_json(&response)["context"]["detail"]
            .as_str()
            .unwrap()
            .contains("未知工具"));
    }

    #[test]
    fn unknown_routes_and_methods_are_reported() {
        let state = state();
        assert_eq!(route(&state, &request("GET", "/nope")).status, 404);
        let response = route(&state, &request("PUT", "/api/documents"));
        assert_eq!(response.status, 405);
        assert!(response.headers.iter().any(|(name, _)| name == "Allow"));
    }

    #[test]
    fn blob_urls_are_rewritten_with_doc_and_token() {
        let value = json!({
            "ok": true,
            "preview": {"thumb_url": "yanshi://blob/sha256:abc"},
            "list": ["yanshi://blob/sha256:def"],
            "other": "yanshi://doc/1",
        });
        let rewritten = rewrite_blob_urls(value, "doc_1", Some("tok"));
        assert_eq!(
            rewritten["preview"]["thumb_url"],
            json!("/api/blob/sha256:abc?doc=doc_1&token=tok")
        );
        assert_eq!(
            rewritten["list"][0],
            json!("/api/blob/sha256:def?doc=doc_1&token=tok")
        );
        assert_eq!(
            rewritten["other"],
            json!("yanshi://doc/1"),
            "非 blob URL 不改写"
        );
    }

    #[test]
    fn routes_table_and_helpers() {
        let table = routes();
        assert!(table.contains_key("GET /ws?doc=&token="));
        assert!(owner_from_role(Role::Owner));
        assert!(!owner_from_role(Role::Editor));
        assert!(HttpOptions::help().contains("yanshi-serve"));
        let options = HttpOptions::parse_args([
            "--bind",
            "0.0.0.0:9000",
            "--doc",
            "d1",
            "--profile",
            "core,collab",
        ])
        .unwrap();
        assert_eq!(options.bind, "0.0.0.0:9000");
        assert_eq!(options.doc_id, "d1");
        assert!(options.profiles.contains(&Profile::Collab));
        assert!(HttpOptions::parse_args(["--bogus"]).is_err());
    }
}
