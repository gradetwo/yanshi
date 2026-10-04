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
use yanshi_core::{Bbox, ErrorCode, ErrorContext, Seq, YanshiError};
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
    /// 品牌资源目录（`/brand/*` 与 `/favicon.*` 从这里取；`None` 表示不提供）。
    pub brand_dir: Option<PathBuf>,
    /// 介质插件目录（设计 11.1）：`GET /mediums/{file}` ✓。
    pub medium_dir: Option<PathBuf>,
    /// **随发行包发布的资产根目录** ✓（仓库里是 `assets/` ✓；包内是 `share/yanshi` ✓）。
    ///
    /// **为什么与介质同类** ✓：它们都是**发行物的一部分** ✓ ⇒ 由命令行指定 ✓、
    /// 由 `yanshi.sh` 按包内位置传进来 ✓（解包到哪都能跑 ✓）。
    /// 与"工作区缓存"（`<root>/<种类>/` ✓）**并存** ✓：内置的开箱就有 ✓，缓存是用户后导入或抓的 ✓。
    ///
    /// **⚠️ 这段注释原先写的是"内置纹理目录"** ✗ —— 字段改名成"资产根目录"之后注释没跟着改 ✓
    /// ⇒ 后来的人会以为这里**只**管纹理 ✓（实际它同时管 `brushes` / `palettes` ✓）。
    pub assets_dir: Option<PathBuf>,
}

impl Default for HttpOptions {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8080".to_owned(),
            root: None,
            doc_id: "default".to_owned(),
            width: 1024,
            height: 1024,
            // Web 查看器/人类客户端默认启用全部已实现的工具组（10.2：Web 编辑器启用全量）。
            //
            // **这一份列表原本少了两个组** ✗（本轮真机验收抓到的 ✓）：查看器已经有「变更集」面板 ✓，
            // 而 `changeset` 组**不在列表里** ✗ ⇒ 界面上点「开始变更集」只会得到
            // 「**未知工具 begin_changeset（当前 profile 未启用或不存在）**」✗ ——
            // **界面做了、工具没放行** ✓，这正是本项目"内核/工具先行"那条纪律要防的事 ✓
            //（这次是**反着**撞上的 ✓：工具早就实现了 ✓、也放行了测试 ✓，只是**服务端没启用** ✗）。
            // `conflict` 同理 ✓。
            //
            // **`Semantic` 故意不在其中** ✓：语义工具需要外部模型服务 ✓，
            // 按既定裁定"**先预留设计、先不开发**" ✓ ⇒ 这里**不启用** ✓（不是漏了 ✓）。
            profiles: vec![
                Profile::Core,
                Profile::History,
                Profile::Changeset,
                Profile::Annotation,
                Profile::Collab,
                Profile::Structure,
                Profile::Retouch,
                Profile::Conflict,
            ],
            max_connections: 64,
            push_interval_ms: 25,
            rewrite_blob_urls: true,
            // 默认指向仓库内 `wasm-bindgen --target web` 的输出目录（相对当前工作目录）。
            wasm_dir: Some(PathBuf::from("crates/yanshi-wasm/pkg")),
            brand_dir: Some(PathBuf::from("assets/brand")),
            medium_dir: Some(PathBuf::from("assets/mediums")),
            // 门面是**构建产物** ✓（与 `crates/yanshi-wasm/pkg` 同一性质 ✓）⇒ 缺省指向 target 里那份 ✓。
            assets_dir: Some(PathBuf::from("assets")),
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
                "--brand-dir" => options.brand_dir = Some(value_of("--brand-dir")?.into()),
                "--medium-dir" => options.medium_dir = Some(value_of("--medium-dir")?.into()),
                "--assets-dir" => options.assets_dir = Some(value_of("--assets-dir")?.into()),
                "--no-assets" => options.assets_dir = None,
                "--no-mediums" => options.medium_dir = None,
                "--no-brand" => options.brand_dir = None,
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
                        profiles
                            .extend(Profile::parse_list(name).map_err(|error| error.to_string())?);
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
           --profile <list>   启用工具组，逗号分隔，可选：all,core,history,changeset,retouch,semantic,conflict,annotation,collab,structure\n\
                              （all = 全部已实现的组，即除 semantic 外；缺省同 all）\n\
                              （缺省启用除 semantic 外的全部；semantic 组按既定裁定只预留不开发）\n\
           --wasm-dir <dir>   WASM 计算内核产物目录（缺省 crates/yanshi-wasm/pkg）
--medium-dir <dir> 介质插件目录（缺省 assets/mediums；发布包里在 share/yanshi/mediums）\n\
           --no-wasm          不提供浏览器端 WASM 计算内核（查看器退化为服务端渲染）\n\
           --brand-dir <dir>  品牌资源目录（缺省 assets/brand）\n\
           --no-brand         不提供品牌资源（favicon/logo）\n\
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

/// **构建标识** ✓ —— 版本 + commit + 构建时间 ✓（由 `build.rs` 编进来 ✓，取不到就是 `unknown` ✓）。
///
/// **放在这里** ✓：`--version` 与健康接口**共用同一个字符串** ✓ ⇒ 两边永远一致 ✓（否则又会出现
/// "`--version` 说的和接口说的不一样"✗ 这种最让人怀疑自己眼睛的问题 ✓）。
pub fn build_identity() -> String {
    // **报产品名而不是 crate 名** ✓：crate 叫 `yanshi-http` ✓，而用户认的是 **Yanshi** ✓
    //（第一版打出来是 `yanshi-http 0.1.0` ✓ —— 那会让人以为装错了东西 ✗）。
    format!(
        "yanshi {} (commit {}, built {})",
        env!("CARGO_PKG_VERSION"),
        option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
        option_env!("YANSHI_BUILD_TIME").unwrap_or("unknown"),
    )
}

/// **简短标识** ✓（界面/日志里用 ✓）：`0.1.0+abc1234` ✓。
pub fn build_short() -> String {
    format!(
        "{}+{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
    )
}

/// 启动服务（`bind` 支持端口 0，便于测试）。
pub fn serve(options: HttpOptions) -> std::io::Result<ServerHandle> {
    let settings = DocumentSettings::default();
    let workspace = match &options.root {
        Some(root) => Workspace::with_file_store(root.clone(), settings.clone())
            .unwrap_or_else(|_| Workspace::in_memory(settings)),
        None => Workspace::in_memory(settings),
    }
    // **内置纹理目录接进工具层** ✓ ⇒ `list_textures` 会同时报内置与缓存 ✓，
    // 而 MCP 与查看器**都**经工具层 ✓ ⇒ 两边一致 ✓（用户那条硬要求 ✓）。
    // **资产目录要解析成真实存在的那个** ✓（真实用户报告 ✓：换工作目录启动 ⇒ 三类资产全空 ✗）。
    // 解析结果**打印出来** ✓ ⇒ 以后这类问题一眼可查 ✓。
    .with_assets_dir({
        let (resolved, note) =
            yanshi_server::service::resolve_assets_dir(options.assets_dir.clone());
        eprintln!("  {note}");
        resolved
    });
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
    // 空闲 30s 视为断开，避免长期占着线程（keep-alive 连接会复用到客户端关闭为止）。
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok();
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    // HTTP/1.1 keep-alive：一条连接上顺序处理多个请求。
    // 曾经这里只处理一个请求却回了 `Connection: keep-alive`，浏览器复用连接后拿到已关闭的
    // socket，表现为随机的 `TypeError: Failed to fetch`（缩略图刷新时尤其明显）。
    loop {
        let Some(request) = Request::read(&mut reader)? else {
            return Ok(());
        };
        state.requests.fetch_add(1, Ordering::Relaxed);
        if request.is_websocket_upgrade() {
            return handle_websocket(state, request, writer, reader);
        }
        let keep_alive = !request
            .header("connection")
            .map(|value| value.eq_ignore_ascii_case("close"))
            .unwrap_or(false);
        // HEAD 按 GET 路由，但只回头部：此前直接返回 405，连自己的诊断脚本都被误导过。
        let is_head = request.method == yanshi_http_method_head();
        let response = if is_head {
            let mut get_like = request.clone();
            get_like.method = yanshi_http_method_get();
            route(&state, &get_like)
        } else {
            route(&state, &request)
        };
        if is_head {
            response.write_head(&mut writer, keep_alive)?;
        } else {
            response.write(&mut writer, keep_alive)?;
        }
        if !keep_alive {
            return Ok(());
        }
    }
}

/// `Method::Head`（避免在文件顶部再引入一次 `Method` 名称）。
fn yanshi_http_method_head() -> crate::http::Method {
    crate::http::Method::Head
}

/// `Method::Get`。
fn yanshi_http_method_get() -> crate::http::Method {
    crate::http::Method::Get
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
    // 效果目录（只读）：给查看器/客户端列出**内核支持**的调整与滤镜名。
    // 之所以不给参数默认值：默认值由内核在缺参时决定，抄一份到别处必然与内核漂移 ✗；
    // 调用方不传 params 即得到内核默认值，随后用 `list_effects` 读回实际生效的参数 ✓。
    // 设计未规定该端点（10.2 只规定了工具），属于查看器所需的只读支撑接口，已记录在实现说明。
    if path == "/api/effects" {
        if method != "GET" {
            return method_not_allowed(request, "GET");
        }
        return Response::json(
            200,
            &json!({
                "adjustments": yanshi_render::filter::ADJUSTMENT_NAMES,
                "filters": yanshi_render::filter::FILTER_NAMES,
            }),
        )
        .with_header("Cache-Control", "no-cache");
    }
    if path == "/" {
        return match method {
            // 页面与 WASM 内核必须**同版本**：页面改动后若被缓存，会出现
            // 「新页面 + 旧内核」的方法缺失（本次报告的真实缺陷就是这种组合）。
            // 因此页面也必须 no-cache，让普通刷新即可拿到新版本。
            "GET" => Response::html(viewer::page_with_read_tools())
                .with_header("Cache-Control", "no-cache"),
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
            // **`GET /api/tools` = 工具清单** ✓（本轮补 ✓）。
            //
            // **为什么必须有** ✗：外部绘画 agent 实测——HTTP 侧**没有**任何工具清单 ✓、
            // `docs/tools.md` 是设计散文（没有参数表 ✗）⇒ 它只能去读 **11,834 行** `tools.rs` 拿参数名 ✗；
            // 而同一个引擎的 MCP `tools/list` **明明有** 58 个工具 + 完整 `inputSchema` ✓
            // ⇒ 这是"Web 与 MCP 不一致"里最贵的一条 ✓。
            // **清单与 MCP 逐字相同** ✓：都调 `ToolRegistry::tools_list_json` ✓（在 yanshi-server 里 ✓）
            // ⇒ **一处定义、两处使用** ✗（各写一份必然漂移 ✓）。
            "GET" => Response::json(200, &state.registry.tools_list_json()),
            "POST" => tool_route(state, request),
            _ => method_not_allowed(request, "GET, POST"),
        };
    }
    if path == "/api/blob" {
        return match method {
            "GET" => blob_route(state, request),
            // 上传（设计 6.3「blob 先行」）：客户端先传字节，再提交引用它的原子。
            // `import_image` 这类工具要求 blob 已存在（悬空引用会被 12.2 的校验拒绝）。
            "POST" => blob_upload(state, request),
            _ => method_not_allowed(request, "GET, POST"),
        };
    }
    if path == "/api/atoms" {
        return match method {
            // 客户端自带 ULID 的原子提交（12.2 幂等与重试安全）。
            "POST" => atom_submit(state, request),
            // 完整原子读取：WASM 客户端据此本地折叠（`get_log` 只给元数据）。
            "GET" => atom_list(state, request),
            _ => method_not_allowed(request, "GET, POST"),
        };
    }
    if let Some(file) = path.strip_prefix("/wasm/") {
        return match method {
            "GET" => wasm_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/samples/") {
        // **示例画面** ✓（随仓库发布 ✓）：换一台机器也能打开示例看到内容 ✓。
        // 用**白名单**而不是拼路径 ✓（与品牌资源同一考虑 ✓）。
        return match method {
            "GET" => sample_asset(file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/textures/") {
        // **纹理缩略图** ✓（目标 (b) ✓）：界面里要显示预览 ✓，
        // 而浏览器不能直接读服务器上的文件 ✓ ⇒ 得像介质那样**由服务端发** ✓。
        return match method {
            "GET" => texture_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/mediums/") {
        // 笔刷介质插件（设计 11.1）：宿主实例化 wasm 插件时来取 ✓。
        return match method {
            "GET" => medium_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brush-previews/") {
        // **预生成入库的画笔库预览图**（构建期生成、随包发布）—— 面板直接用图片，不再逐支实时渲染。
        return match method {
            "GET" => brush_preview_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brushes/") {
        // **`.myb` 文本** ✓（浏览器拿它喂门面 ✓）—— 与 `/mediums/` **同一条白名单规矩** ✓。
        return match method {
            "GET" => brush_text_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brand/") {
        return match method {
            "GET" => brand_asset(state, file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    // 浏览器默认来取 favicon：SVG 走 icon-light（512 viewBox，缩到 16px 依然清晰），
    // PNG/ICO 走 assets/brand/png 里预先渲染好的尺寸。
    // **离线优先 PWA 的两条路由**（目标 (A)① 第一步）：
    //  `/service-worker.js` ⇒ 缓存外壳 ⇒ **服务端不在时页面仍能打开**；`/manifest.webmanifest` ⇒ 可安装。
    if path == "/service-worker.js" {
        return match method {
            "GET" => {
                // **注入构建标识** ✓（`build.rs` 已把 commit 编进来 ✓ ⇒ 新构建 ⇒ 新缓存名 ✓）。
                let body = SERVICE_WORKER_JS.replace("__BUILD_ID__", BUILD_ID);
                Response::bytes(200, "text/javascript; charset=utf-8", body.into_bytes())
                    .with_header("Cache-Control", "no-cache")
            }
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/manifest.webmanifest" {
        return match method {
            "GET" => Response::bytes(
                200,
                "application/manifest+json",
                WEB_MANIFEST_JSON.as_bytes().to_vec(),
            )
            .with_header("Cache-Control", "public, max-age=3600"),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/favicon.svg" || path == "/favicon.png" || path == "/favicon.ico" {
        let file = match path {
            "/favicon.svg" => "svg/icon-light.svg",
            "/favicon.png" => "png/favicon-32.png",
            _ => "png/favicon.ico",
        };
        return match method {
            "GET" => brand_asset(state, file),
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
            // **"跑的是哪一版"** ✓：版本 + commit + 构建时间 ✓ —— 排查时最先需要的那一行 ✓。
            "version": env!("CARGO_PKG_VERSION"),
            "commit": option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
            "built": option_env!("YANSHI_BUILD_TIME").unwrap_or("unknown"),
            "build": build_identity(),
            "documents": documents,
            "requests": state.requests.load(Ordering::Relaxed),
            "connections": state.connections.load(Ordering::Relaxed),
            "uptime_ms": yanshi_core::now_ms() - state.started_at,
            "profiles": state.registry.profiles().iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            "tools": state.registry.len(),
            "wasm": wasm_available(state),
            "cache": cache_stats(state),
            // **降级要可见** ✓（真实用户报的第 2 条 ✓）：9p/NFS 上 fsync 不被支持 ✓
            // ⇒ blob 仍写得进去 ✓，但**掉电安全没有保证** ✓ ⇒ 这一项就让使用者看得见 ✓。
            "blob_fsync": if state
                .workspace
                .lock()
                .map(|workspace| workspace.store().unsupported_sync())
                .unwrap_or(false)
            {
                "unsupported"
            } else {
                "ok"
            },
            "rss_bytes": rss_bytes(),
        }),
    )
}

/// 渲染缓存与像素缓冲的可观测性（设计 1319 行：可观测性含缓存/生命周期指标）。
fn cache_stats(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    let (tiles, used_bytes, evictions, misses) = workspace.cache_stats();
    json!({
        "tiles": tiles,
        "used_bytes": used_bytes,
        "evictions": evictions,
        "misses": misses,
        "pixel_bytes_estimate": workspace.pixel_bytes_estimate(),
    })
}

/// 进程常驻内存（Linux：/proc/self/statm 的第二个字段 × 页大小）。
///
/// 只做观测，不参与调度；非 Linux 环境返回 `null`（不假装有数据）。
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    // 页大小取 sysconf 的常见默认值 4096；仅用于量级观测。
    Some(pages.saturating_mul(4096))
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
    // 「另存为副本」：`copy_from` + `from_token`（源文档的令牌，用于鉴权 ✓）。
    // 设计未规定文档命名/重命名 ✗ —— 文档以 doc_id 为主键 ✓，用户确认采用可逆的
    // 「另存为副本」（新 id、原文档保留 ✓；真改名会牵动日志/持久化/令牌，不可逆 ✗）。
    if let Some(from) = body.get("copy_from").and_then(Value::as_str) {
        let from = from.to_owned();
        let from_token = body
            .get("from_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let actor = body
            .get("actor")
            .and_then(Value::as_str)
            .unwrap_or("human:web")
            .to_owned();
        // 源文档必须用**它自己的令牌**授权（目标文档的令牌管不到源文档 ✓）。
        if let Err(response) = authorize_with_token(state, &from, from_token) {
            return response;
        }
        let Ok(mut workspace) = state.workspace.lock() else {
            return internal("工作区锁中毒");
        };
        let copied = match workspace.duplicate_document(&from, &doc_id, &actor, "session:web") {
            Ok(copied) => copied,
            Err(error) => return Response::from_error(&error),
        };
        let token = match workspace.issue_token(&doc_id, &actor, Role::Editor) {
            Ok(token) => token,
            Err(error) => return Response::from_error(&error),
        };
        return Response::json(
            200,
            &json!({"ok": true, "doc_id": doc_id, "token": token.as_str(),
                    "copied_atoms": copied, "copy_from": from}),
        );
    }
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
    //
    // **Phase 5：角色权限** ✓ —— 打开时可指定角色，缺省 `editor` ✓。
    // 设计只写了「Phase 5 扩展为 owner/editor/viewer」✓、**没有规定发放策略** ✗
    // ⇒ 这里做一个**最小且可预期**的选择 ✓ 并记档 ✓：**调用方显式声明角色** ✓、缺省 editor ✓。
    // **不擅自发明"谁是 owner"** ✗（那是权限管理策略 ✓，留给后续 ✓）；
    // 但 `owner` 必须**可选** ✓，否则 `can_revert_others` 那条能力**永远用不上** ✗。
    let requested_role = match request.param("role").unwrap_or("editor") {
        "viewer" => Role::Viewer,
        "editor" => Role::Editor,
        "owner" => Role::Owner,
        other => {
            return Response::from_error(&YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!("未知角色 {other}（可用：viewer / editor / owner）")),
            ))
        }
    };
    // **对外绑定时不许匿名发写权限令牌** ✓（第三方代码审计 P0 第 2 条：
    // `/api/documents` 原先任何人无需凭据即可拿到 **Editor** 令牌 ✓，
    // 与已修的"任意路径写"合起来就是"未授权 + 任意写"完整链 ✗）。
    // 策略（最小且可预期 ✓）：**绑回环** ⇒ 维持现状 ✓（本机开发/探针都靠它 ✓）；
    // **绑对外** ⇒ Editor/Owner 必须带密钥 `YANSHI_API_KEY` ✓；没配密钥就**明确拒绝**并给出两条出路 ✓。
    let write_role = matches!(requested_role, Role::Editor | Role::Owner);
    let loopback = bind_is_loopback(&state.options.bind);
    if write_role && !loopback {
        let expected = std::env::var("YANSHI_API_KEY")
            .ok()
            .filter(|value| !value.is_empty());
        let provided = request
            .param("key")
            .map(str::to_owned)
            .or_else(|| request.header("x-yanshi-key").map(str::to_owned));
        match expected {
            None => {
                return Response::from_error(&YanshiError::new(
                    ErrorCode::PreconditionFailed,
                    ErrorContext::detail(
                        "服务端绑在对外地址上，但没配 YANSHI_API_KEY ⇒ 拒绝匿名签发写权限令牌。                         两条出路：① 只在本机用 ⇒ 用 --bind 127.0.0.1:8080 启动；                         ② 确实要对外 ⇒ 设 YANSHI_API_KEY=<密钥>，请求时带上 ?key=<密钥>（或 X-Yanshi-Key 头）"
                            .to_owned(),
                    ),
                ));
            }
            Some(key) => {
                if provided.as_deref() != Some(key.as_str()) {
                    return Response::from_error(&YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail(
                            "密钥不对（或没带）⇒ 对外绑定时签发写权限令牌需要 ?key=<YANSHI_API_KEY>".to_owned(),
                        ),
                    ));
                }
            }
        }
    }
    let token = match workspace.issue_token(&doc_id, &actor, requested_role) {
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
            // **回显尺寸**（真实回归报告）：它请求 800×600、读到的是 1024×1024 ⇒ 报"尺寸被静默忽略" ✗。
            // 真相是：它**量的是别的文档**（会话/命令行那个 1024×1024）✓ —— 但响应不回显尺寸，
            // 就没人能从回执里自证 ✓ ⇒ 补上这两项，让"我建的是多大"**一眼可验** ✓。
            "width": width,
            "height": height,
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

/// 用**显式令牌**授权某个文档（用于「另存为副本」：目标文档的令牌管不到源文档 ✓）。
fn authorize_with_token(state: &ServerState, doc_id: &str, token: &str) -> Result<(), Response> {
    let token = if token.is_empty() {
        None
    } else {
        Some(
            yanshi_server::CapabilityToken::parse(token)
                .map_err(|error| Response::from_error(&error))?,
        )
    };
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| internal("工作区锁中毒"))?;
    workspace
        .authorize(doc_id, token.as_ref(), TransportKind::Http, "local:http")
        .map(|_| ())
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
    // **关闭 ≠ 删除** ✓ —— 这里只把文档从**内存**里放下 ✓，磁盘上的工作区**仍然保留** ✓，
    // 因此它依旧出现在 `GET /api/documents` 里 ✓（子 agent 把这当成 bug 报上来 ✓：
    // "DELETE 返回 closed:true，但文档还在列表里" ✓）。
    // 这是**设计内的持久化行为** ✓（重新打开展示作品正是需求 ✓），所以不改语义 ✗，
    // 而是让**响应说实话** ✓：明确给出 `persisted` ✓。
    // 真删文件是**不可逆**动作 ✗ ⇒ 按纪律不擅自实现 ✓（需要时再单独立项 ✓）。
    let persisted = workspace
        .list_documents()
        .map(|documents| documents.iter().any(|summary| summary.doc_id == doc_id))
        .unwrap_or(false);
    Response::json(
        200,
        &json!({
            "ok": true,
            "closed": closed,
            "persisted": persisted,
            "note": if persisted { "已关闭内存中的文档；磁盘上的工作区仍然保留" } else { "已关闭" },
        }),
    )
}

/// 上传 blob（`POST /api/blob?doc=..&token=..`，请求体即字节）。
///
/// 返回 `{ok, blob_hash, size, mime_type}`，供 `import_image` 等工具引用。
/// 大小上限与设计的画布规模相称（默认 64MiB，可用 `YANSHI_MAX_UPLOAD_BYTES` 覆盖）。
fn blob_upload(state: &ServerState, request: &Request) -> Response {
    let doc_id = match doc_param(request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response,
    };
    let principal = match authorize(state, request, &doc_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    if !principal.role.can_edit() {
        return Response::from_error(&YanshiError::new(
            ErrorCode::PermissionDenied,
            ErrorContext::detail("该 token 不允许上传 blob"),
        ));
    }
    let limit = std::env::var("YANSHI_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(64 * 1024 * 1024);
    if request.body.is_empty() {
        return crate::http::bad_request("上传内容为空");
    }
    if request.body.len() > limit {
        return Response::json(
            413,
            &json!({"ok": false, "error_code": "resource_exhausted", "retryable": false,
                    "context": {"detail": format!("上传 {} 字节超过上限 {limit}", request.body.len())}}),
        );
    }
    let declared_mime = request
        .header("content-type")
        .unwrap_or("application/octet-stream")
        .to_owned();
    // **PNG 在这里解码成原始 RGBA** ✓（设计 791 行：「`import_image`（JPEG/PNG/WebP →
    // 像素图层对象…）」✓）。
    //
    // 为什么放在**上传端点**而不是工具里 ✓：工具接收的是 **blob 描述符** ✓（`{blob_hash, size,
    // mime_type}` ✓），像素已经入库 ✓ ⇒ 在这里归一化 ⇒ **下游一行都不用改** ✓
    //（工具 ✓、介质路径 ✓、缩略图 ✓ 全都照旧吃 raw ✓）。
    //
    // **边界写清楚 ✓**：只认 **PNG** ✓。JPEG/WebP 的解码器不在本仓库自研范围内 ✓
    //（零外部依赖 ✓）⇒ **显式拒绝并说明** ✓。界面不受影响 ✓：它用浏览器的
    // `createImageBitmap` 解码任意格式 ✓ 后上传 raw ✓（见 `importLocalImage` ✓）。
    let mut mime_type = declared_mime.clone();
    let mut body = request.body.clone();
    let mut decoded_from = Value::Null;
    if declared_mime.eq_ignore_ascii_case("image/png") {
        match yanshi_render::png::decode_png(&request.body) {
            Some((width, height, rgba)) => {
                decoded_from = json!({"format": "png", "width": width, "height": height});
                body = rgba;
                mime_type = "image/x-yanshi-raw".to_owned();
            }
            None => {
                return crate::http::bad_request(
                    "PNG 解码失败（本仓库只支持 8 位、非隔行的 RGB/RGBA PNG）",
                );
            }
        }
    } else if declared_mime.eq_ignore_ascii_case("image/jpeg")
        || declared_mime.eq_ignore_ascii_case("image/webp")
    {
        return crate::http::bad_request(
            "本仓库自带的解码器只支持 PNG；JPEG/WebP 请在界面里导入（浏览器会先解码），\
             或先转成 PNG（零外部依赖，见设计 791 行）",
        );
    }
    let Ok(workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    match workspace.store().put(&body) {
        Ok(hash) => Response::json(
            200,
            &json!({"ok": true, "blob_hash": hash.to_string(), "size": body.len(),
                    "mime_type": mime_type, "decoded_from": decoded_from}),
        ),
        Err(error) => Response::from_error(&error),
    }
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
        // **说清两种写法** ✗（外部 agent 实测：它把平铺参数发到 `/api/tools` ✓ ⇒ 只拿到
        // "缺少 tool 字段" ✓ ⇒ 完全猜不到这个服务有两条路 ✓，以为是自己参数写错 ✓）。
        return crate::http::bad_request(
            "缺少 tool 字段。本服务有**两种**调用写法：\
             ① POST /api/tools 的请求体是 {\"tool\": \"<工具名>\", \"arguments\": {…参数…}}；\
             ② POST /api/tools/<工具名> 的请求体**就是参数本身**（平铺）。\
             想知道有哪些工具与参数 ⇒ GET /api/tools（返回与 MCP tools/list 逐字相同的清单）",
        );
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
    // **角色检查已经下沉到工具层** ✓（`ToolRegistry::call` ✓）⇒ 这里**不再重复** ✗。
    //
    // **为什么删掉** ✓：这段原来只堵住 `/api/tools/*` ✓，而 **WebSocket** 是**另一个入口** ✗
    // ⇒ 两处各写一份 ⇒ **必然漏一个** ✓ —— 实测漏的正是 WS ✓：
    // 同一个 viewer 令牌、同一个工具 ✓：`HTTP ⇒ permission_denied` ✓ 而 `WebSocket ⇒ ok:true` ✗。
    // **一处生效、全部受益** ✓ —— 这是本项目一贯的做法 ✓。
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
    // **真实角色必须带进上下文** ✓：工具层的强制检查读的就是它 ✓（见 `ToolRegistry::call` ✓）。
    .with_role(principal.role)
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

/// `GET /mediums/{file}`：提供 `assets/mediums/` 下的介质插件 ✓。
///
/// 只接受 **直接位于该目录、以 `.wasm` 结尾** 的文件名 ✓（拒绝 `..`、`/` 与其它扩展名 ✓），
/// 因此不会把仓库里任意文件暴露出去 ✓（与 `/brand/` 的白名单同思路 ✓，
/// 但这里允许"加一个插件就多一个文件" ✓ —— 正是设计里"介质可扩展"的诉求 ✓）。
/// `GET /samples/{file}`：**随仓库发布的示例画面** ✓。
///
/// **白名单** ✓（与品牌资源同一考虑 ✓）：只发 `assets/samples` 下、名字在清单里的 PNG ✓，
/// 不做路径拼接 ✗ ⇒ 不可能借它读到仓库里的别的文件 ✓。
const SAMPLE_FILES: [&str; 6] = [
    "sample-oil.png",
    "sample-watercolor.png",
    "sample-brush.png",
    "sample-reference.png",
    "sample-lake.png",
    "sample-yanshi.png",
];

// **已退休** ✓（(A)③ ✓）：这里原来是 `GET /brush-module.wasm` 与它的处理函数 ✓ ——
// 它把**第二份实现**（`.myb` 门面 ✓）发给浏览器 ✓，供拖动期的本地笔刷预览用 ✓。
// 预览现在走**共享内核** ✓（`state.wasm.paint_brush` ✓，一份实现 ✓）⇒ 这条路由**没有存在意义**了 ✓
// ⇒ 连同 `--brush-wasm` / `--no-brush-wasm` 与那个选项字段一并删除 ✓
//（**留说明而不是留空白** ✓ —— 空白会让人以为"少了个端点"✗，与过时的描述一样误导 ✓）。

/// `GET /brushes/{file}.myb`：把**笔刷文本**发给浏览器 ✓。
///
/// **只接受直接位于笔刷目录下、以 `.myb` 结尾的名字** ✓（与 `/mediums/` 同一条规矩 ✓：
/// 拒 `..`、`/`、`\\`、其它扩展名 ✓ ⇒ 不可能借它读到仓库里别的文件 ✓）。
fn brush_text_asset(state: &ServerState, file: &str) -> Response {
    if file.contains("..") || file.contains('/') || file.contains('\\') || !file.ends_with(".myb") {
        return crate::http::bad_request("非法笔刷名（只接受笔刷目录下的 *.myb）");
    }
    let Some(root) = state.options.assets_dir.as_ref() else {
        return crate::http::not_found("这一份服务端没有配置资产目录（--assets-dir）");
    };
    let path = root.join("brushes").join(file);

    match std::fs::read(&path) {
        // `.myb` 就是 JSON ✓ ⇒ 报 `application/json` ✓（浏览器按文本读 ✓）。
        Ok(bytes) => {
            // **顺带告诉浏览器"这支笔会不会读画布"** ✓（第 68 轮定 ✓）：会读的**不能**本地预览 ✓
            //（门面没有 base 输入 ⇒ 起点不同 ⇒ 预览会漂 ✗）。判定用**工具层那一个函数** ✓。
            let reads_canvas = std::str::from_utf8(&bytes)
                .map(yanshi_server::tools::myb_text_reads_the_canvas)
                .unwrap_or(false);
            Response::bytes(200, "application/json; charset=utf-8", bytes).with_header(
                "X-Yanshi-Brush-Reads-Canvas",
                if reads_canvas { "1" } else { "0" },
            )
        }
        Err(_) => crate::http::not_found(format!("没有这支笔刷：{}", path.display())),
    }
}

/// **监听地址是不是只在本机** ✓ —— 用来决定"能不能匿名发写权限令牌"（见 `create_document`）。
/// 判据从宽到严 ✓：`127.` / `::1` / `localhost` 都算回环 ✓；`0.0.0.0`、`[::]`、具体外网地址都**不算** ✗。
fn bind_is_loopback(bind: &str) -> bool {
    let host = bind.rsplit_once(':').map(|(host, _)| host).unwrap_or(bind);
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host == "localhost" || host == "::1" || host.starts_with("127.")
}

/// `GET /brush-previews/{file}`：**预生成入库的画笔预览图 / 索引**。
///
/// 白名单（与 `/mediums/`、`/brushes/` 同一条规矩）：只接受该目录下、以 `.png` 或 `.json` 结尾的文件名，
/// 拒 `..`、`/`、`\\` ⇒ 不可能借它读到仓库里别的文件。
fn brush_preview_asset(state: &ServerState, file: &str) -> Response {
    if file.contains("..")
        || file.contains('/')
        || file.contains('\\')
        || !(file.ends_with(".png") || file.ends_with(".json") || file.ends_with(".svg"))
    {
        return crate::http::bad_request("非法预览文件名（只接受该目录下的 *.png / *.json）");
    }
    let Some(root) = state.options.assets_dir.as_ref() else {
        return crate::http::not_found("这一份服务端没有配置资产目录（--assets-dir）");
    };
    let path = root.join("brush-previews").join(file);
    match std::fs::read(&path) {
        Ok(bytes) => Response::bytes(
            200,
            if file.ends_with(".json") {
                "application/json; charset=utf-8"
            } else if file.ends_with(".svg") {
                "image/svg+xml; charset=utf-8"
            } else {
                "image/png"
            },
            bytes,
        ),
        Err(_) => crate::http::not_found(format!("没有这张笔刷预览：{}", path.display())),
    }
}

fn sample_asset(file: &str) -> Response {
    if !SAMPLE_FILES.contains(&file) {
        return crate::http::not_found(format!("没有这个示例画面：{file}"));
    }
    let dir = std::env::var("YANSHI_SAMPLE_DIR").unwrap_or_else(|_| "assets/samples".to_owned());
    let path = std::path::PathBuf::from(dir).join(file);
    match std::fs::read(&path) {
        Ok(bytes) => Response::bytes(200, "image/png", bytes)
            // 画面随版本走 ✓ ⇒ 可以长缓存 ✓。
            .with_header("Cache-Control", "public, max-age=31536000, immutable"),
        Err(_) => crate::http::not_found(format!("示例画面缺失：{}", path.display())),
    }
}

/// **`GET /textures/{file}`：发一张纹理（用于界面预览缩略图 ✓）** ✓。
///
/// **查找顺序与工具层一致** ✓（真实用户要求 ✓）：**工作区缓存优先 ✓、内置其次 ✓** ——
/// 与 `list_assets` / `resolve_asset` 的优先级**必须相同** ✗
///（各判一次必然漂移 ✓：界面显示的是 A ✓、真铺上去的是 B ✗ —— 正是"两边不一样"的那类 bug ✓）。
fn texture_asset(state: &ServerState, file: &str) -> Response {
    // **只接受直接位于目录下的 `.png`** ✓（挡 `..`、`/`、其它扩展名 ✓ ⇒ 不会把仓库任意文件暴露出去 ✓）。
    if file.contains("..") || file.contains('/') || file.contains('\\') || !file.ends_with(".png") {
        return crate::http::bad_request("非法纹理名（只接受纹理目录下的 *.png）");
    }
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    // **缓存优先** ✓。
    if let Some(root) = state.options.root.as_ref() {
        candidates.push(root.join("textures").join(file));
    }
    // **内置其次** ✓。
    if let Some(assets) = state.options.assets_dir.as_ref() {
        candidates.push(assets.join("textures").join(file));
    }
    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Response::bytes(200, "image/png", bytes)
                // **纹理是随版本走的资产** ✓ ⇒ 可以长缓存 ✓。
                .with_header("Cache-Control", "public, max-age=31536000, immutable");
        }
    }
    crate::http::not_found(format!(
        "没有这张纹理：{file} ⇒ 先跑 list_assets 看有哪些（或 scripts/fetch-textures.sh 抓一批 ✓）"
    ))
}

fn medium_asset(state: &ServerState, file: &str) -> Response {
    if file.contains("..") || file.contains('/') || !file.ends_with(".wasm") {
        return crate::http::bad_request("非法介质资源名（只接受 assets/mediums 下的 *.wasm）");
    }
    let dir = state
        .options
        .medium_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("assets/mediums"));
    let path = dir.join(file);
    match std::fs::read(&path) {
        Ok(bytes) => Response::bytes(200, "application/wasm", bytes)
            // 插件产物不可变（按 id + version 引用）⇒ 可长期缓存 ✓。
            .with_header("Cache-Control", "public, max-age=31536000, immutable"),
        Err(_) => crate::http::not_found(format!(
            "介质插件缺失：{}（先运行 make build-medium）",
            path.display()
        )),
    }
}

/// 品牌资源白名单（避免把任意仓库文件暴露出去）。
const BRAND_FILES: [(&str, &str); 14] = [
    ("svg/favicon.svg", "image/svg+xml"),
    ("svg/icon-light.svg", "image/svg+xml"),
    ("svg/icon-dark.svg", "image/svg+xml"),
    ("svg/logo-horizontal.svg", "image/svg+xml"),
    ("svg/logo-horizontal-cn.svg", "image/svg+xml"),
    ("svg/logo-vertical.svg", "image/svg+xml"),
    ("svg/logo-primary.svg", "image/svg+xml"),
    ("svg/logo-ultra-mini.svg", "image/svg+xml"),
    ("svg/logo-mono-dark.svg", "image/svg+xml"),
    ("png/favicon-16.png", "image/png"),
    ("png/favicon-32.png", "image/png"),
    ("png/favicon-180.png", "image/png"),
    ("png/favicon.ico", "image/x-icon"),
    ("png/yanshi-icon-256.png", "image/png"),
];

/// **Service Worker**（离线优先 PWA 第一步）：缓存外壳，服务端不在时页面仍能打开。
/// 上面那个标识的**可读副本** ✓（`viewer.rs` 注入页面时要用 ✓ —— 两边的缓存名必须**一模一样** ✗，否则查不到 ✓）。
pub const BUILD_ID_TEXT: &str = BUILD_ID;

/// **这份构建的标识** ✓（`build.rs` 编进来的短 commit ✓；取不到就退回版本号 ✓）。
/// 它进 SW 的**缓存名** ✓ ⇒ 新构建自动作废旧外壳 ✓。
const BUILD_ID: &str = match option_env!("YANSHI_COMMIT") {
    Some(commit) => commit,
    None => env!("CARGO_PKG_VERSION"),
};

const SERVICE_WORKER_JS: &str = r##""use strict";
// **外壳清单** ✓ —— 第 213 轮补上**共享内核**两件 ✓：
// 退休前是**查看器自己**用 `cache.put("/brush-module.wasm", …)` 把门面塞进来的 ✓，
// 而那行随门面一起被删 ✗ ⇒ 于是**没人**再把内核放进 SW 缓存 ✗ ⇒
// **离线时内核拿不到** ✗（`cachedUrls` 里没有它 ✓ —— 判据当场把这件事量了出来 ✓）。
const SHELL = [
  "/",
  "/favicon.svg",
  "/brand/svg/icon-light.svg",
  "/brush-previews/index.json",
  "/wasm/yanshi_wasm.js",
  "/wasm/yanshi_wasm_bg.wasm",
];
// **缓存名里带上构建标识** ✓（(A)⑥「SW 升级不脏读」的正主 ✓）：
// 名字一变 ⇒ 下面那句"删掉所有名字不同的缓存"✓ 就自动作废**整份旧外壳** ✓
// ⇒ 这正是第 210 轮查到的真因 ✓（旧 js + 新 wasm ⇒ 内核预览失败 ✓）。
const CACHE = "yanshi-shell-__BUILD_ID__";
self.addEventListener("install", (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    await Promise.all(SHELL.map((url) => cache.add(url).catch(() => undefined)));
    await self.skipWaiting();
  })());
});
self.addEventListener("activate", (event) => {
  event.waitUntil((async () => {
    const names = await caches.keys();
    await Promise.all(names.filter((name) => name !== CACHE).map((name) => caches.delete(name)));
    await self.clients.claim();
  })());
});
self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  // **blob 是"按内容哈希命名"的不可变资源** ✓ ⇒ **cache-first** ✓（行业口径 ✓，与 (A)⑥ 同一条原则 ✓）。
  // **为什么放在这里** ✓：查看器里所有 blob 都出自 `const blobUrl = (hash) => api("/api/blob/" + hash)` ✓
  // ⇒ 有的是 `fetch` ✓、有的是 **`<img src>`** ✗（后者**根本不经过我的 `fetchOrLocal`** ✓
  // ⇒ 这就是"离线时那条 blob 一直失败 ✓、而且我加的写失败警告一条都不打"✓ 的原因 ✓）。
  // ⇒ 交给 SW 做，**一处覆盖全部** ✓，不必去追十几个 `<img>` 赋值点 ✗。
  if (url.pathname.startsWith("/api/blob/")) {
    event.respondWith((async () => {
      const cache = await caches.open(CACHE);
      const hit = await cache.match(request, { ignoreSearch: true });
      if (hit) return hit;
      try {
        const response = await fetch(request);
        if (response && response.ok) cache.put(request, response.clone()).catch(() => undefined);
        return response;
      } catch (error) {
        const fallback = await cache.match(request, { ignoreSearch: true });
        if (fallback) return fallback;
        throw error;
      }
    })());
    return;
  }
  if (url.pathname.startsWith("/api/") || url.pathname.startsWith("/ws")) return;
  event.respondWith((async () => {
    try {
      const response = await fetch(request);
      if (response && response.ok && request.mode === "navigate") {
        const cache = await caches.open(CACHE);
        cache.put("/", response.clone()).catch(() => undefined);
      }
      return response;
    } catch (error) {
      const cached = await caches.match(request, { ignoreSearch: true });
      if (cached) return cached;
      const shell = await caches.match("/");
      if (shell) return shell;
      throw error;
    }
  })());
});
"##;

/// **PWA manifest**（可安装）。
const WEB_MANIFEST_JSON: &str = r##"{ "name": "偃师 Yanshi", "short_name": "偃师", "start_url": "/", "display": "standalone", "background_color": "#ffffff", "theme_color": "#ffffff", "icons": [ { "src": "/brand/svg/icon-light.svg", "sizes": "any", "type": "image/svg+xml" } ] }"##;

/// `GET /brand/{file}` 与 favicon：只提供白名单里的品牌资源。
fn brand_asset(state: &ServerState, file: &str) -> Response {
    if file.contains("..") {
        return crate::http::bad_request("非法资源名");
    }
    let Some((_, content_type)) = BRAND_FILES.iter().find(|(name, _)| *name == file) else {
        return crate::http::not_found(format!("未知品牌资源 {file}"));
    };
    let Some(dir) = &state.options.brand_dir else {
        return crate::http::not_found("服务端未启用品牌资源（--no-brand）");
    };
    let path = dir.join(file);
    match std::fs::read(&path) {
        Ok(bytes) => Response::bytes(200, content_type, bytes)
            .with_header("Cache-Control", "public, max-age=3600"),
        Err(_) => crate::http::not_found(format!("品牌资源缺失：{}", path.display())),
    }
}

/// `GET /api/atoms?doc=<id>&since=<seq>`：完整原子（含 payload），供客户端本地折叠。
///
/// 客户端折叠必须拿到完整原子（6.8）；`get_log` 工具只返回元数据用于轮询与展示。
fn atom_list(state: &ServerState, request: &Request) -> Response {
    let doc_id = match doc_param(request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response,
    };
    if let Err(response) = authorize(state, request, &doc_id) {
        return response;
    }
    let since: Seq = request
        .param("since")
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    let limit: usize = request
        .param("limit")
        .and_then(|text| text.parse().ok())
        .unwrap_or(10_000)
        .min(100_000);
    let Ok(workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let Some(document) = workspace.document(&doc_id) else {
        return crate::http::not_found(format!("文档 {doc_id} 未打开"));
    };
    let atoms: Vec<&yanshi_core::Atom> = document
        .log()
        .iter()
        .filter(|atom| atom.seq > since)
        .take(limit)
        .collect();
    let head = document.head_seq();
    Response::json(
        200,
        &json!({
            "ok": true,
            "doc_id": doc_id,
            "head_seq": head,
            "since": since,
            "count": atoms.len(),
            "atoms": atoms,
        }),
    )
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
            let mut value = yanshi_server::tools::commit_response(&result, None, None, Vec::new());
            // 回带**服务端权威原子**（含 seq）：客户端据此把同一原子应用到本地日志，
            // 不必猜测 seq，也不会因为并发原子错位（WASM 乐观渲染的校正依据）。
            if let Some(appended) = workspace
                .document(&doc_id)
                .and_then(|document| document.log().by_seq(result.seq).cloned())
            {
                value["atom"] = json!(appended);
            }
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

/// **这个 `Origin` 能不能接受** ✓（用于 WebSocket 升级的跨站防护 ✓，见 `handle_websocket`）。
///
/// 放行条件（任一 ✓）：① `Origin` 的 host 是**本机**（`localhost` / `127.x` / `::1` ✓）；
/// ② 与本请求 `Host` 头的 host **一致**（同源页面 ✓）。其余一律拒 ✗。
fn origin_is_acceptable(origin: &str, request: &Request) -> bool {
    let host_of = |value: &str| -> Option<String> {
        let after_scheme = value.split("://").nth(1).unwrap_or(value);
        let authority = after_scheme.split('/').next().unwrap_or("");
        let host = authority
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(authority);
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            None
        } else {
            Some(host.to_owned())
        }
    };
    let Some(origin_host) = host_of(origin) else {
        return false;
    };
    let loopback =
        origin_host == "localhost" || origin_host == "::1" || origin_host.starts_with("127.");
    if loopback {
        return true;
    }
    match request.header("host").and_then(host_of) {
        Some(request_host) => request_host == origin_host,
        None => false,
    }
}

fn handle_websocket(
    state: Arc<ServerState>,
    request: Request,
    mut writer: TcpStream,
    mut reader: BufReader<TcpStream>,
) -> std::io::Result<()> {
    // **跨站 WebSocket 必须拒绝** ✓（第三方代码审计 #9：升级处理从不看 `Origin` ⇒ CSWSH）。
    // 严重度如实说明 ✓：下面**已有 token 鉴权**（`authorize` ✓）⇒ 攻击者拿不到令牌仍进不来 ✓
    //（所以审计"静默窃取画布"的说法偏重 ✗）；但令牌是放在 **URL** 里的 capability ✓
    //（会进浏览历史 / 日志 / Referer ✓）⇒ **再加一道 Origin 白名单是划算的** ✓。
    // 规则（够用且可解释 ✓）：**没有 `Origin` ⇒ 放行** ✓（非浏览器客户端：探针 / curl / MCP ✓ 本就没有 ✓）；
    // **有 `Origin` ⇒ 其 host 必须是本机或与本服务自己的 `Host` 一致** ✓，否则 403 并说明原因 ✓。
    if let Some(origin) = request.header("origin") {
        if !origin_is_acceptable(origin, &request) {
            return Response::from_error(&YanshiError::new(
                ErrorCode::PermissionDenied,
                ErrorContext::detail(format!(
                    "拒绝来自 {origin} 的 WebSocket 升级：跨站连接会把画布暴露给别的网页。                     同源（或本机）页面可正常连接；非浏览器客户端不带 Origin 也会放行"
                )),
            ))
            .write(&mut writer, false);
        }
    }
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
            // **角色必须传下去** ✗ —— 这里此前只传了 actor ✓，
            // 于是 WS 分支**无从知道**调用者是谁 ✓ ⇒ 只好**无条件**当 owner ✗（那就是漏洞 ✓）。
            principal.role,
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
    // **调用者角色** ✓：工具层的权限检查要用 ✓（此前这条路径没有它 ✗ ⇒ 那就是漏洞所在 ✓）。
    role: Role,
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
                    // **别再无条件 owner** ✗（**这就是那个漏洞** ✓）：
                    // `owner` 的语义是"**可跨 actor 撤销**" ✓ ⇒ 它只应对 `Owner` 为真 ✓；
                    // 而"**能不能改文档**"由工具层按 `role.can_edit()` 判 ✓ ⇒ 两者**各司其职** ✓。
                    .with_owner(matches!(role, Role::Owner))
                    .with_role(role)
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
        (
            "GET /api/atoms?doc=&since=",
            "完整原子读取（客户端本地折叠，需 token）",
        ),
        ("GET /wasm/{file}", "WASM 计算内核产物（js/wasm）"),
        ("GET /brand/{file}", "品牌资源（SVG 与预渲染 PNG）"),
        ("GET /favicon.svg", "站点图标（SVG / PNG / ICO）"),
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

    /// `HEAD` 必须与 `GET` 同状态码、同 `Content-Length`，但**不含响应体**。
    /// 此前 HEAD 直接返回 405（本仓库自己的诊断脚本就被误导过一次）。
    #[test]
    fn head_matches_get_headers_without_a_body() {
        let state = state();
        let mut get = request("GET", "/");
        get.path = "/".to_owned();
        let get_response = route(&state, &get);
        assert_eq!(get_response.status, 200);

        // 走与连接循环相同的分支：HEAD 按 GET 路由，只写头部。
        let mut head = request("HEAD", "/");
        head.path = "/".to_owned();
        let mut get_like = head.clone();
        get_like.method = crate::http::Method::Get;
        let head_response = route(&state, &get_like);
        assert_eq!(head_response.status, get_response.status);
        assert_eq!(
            head_response.body.len(),
            get_response.body.len(),
            "Content-Length 必须与 GET 一致"
        );

        let mut written = Vec::new();
        head_response.write_head(&mut written, false).unwrap();
        let text = String::from_utf8_lossy(&written);
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.contains("Content-Length: "), "{text}");
        assert!(
            text.ends_with("\r\n\r\n"),
            "HEAD 响应不应包含响应体：{text}"
        );
    }

    fn request(method: &str, target: &str) -> Request {
        let raw = format!("{method} {target} HTTP/1.1\r\nHost: localhost\r\n\r\n");
        Request::read(&mut std::io::Cursor::new(raw.into_bytes()))
            .unwrap()
            .unwrap()
    }

    /// **`GET /api/tools` 必须与 MCP `tools/list` 是同一份** ✓（外部绘画 agent 实测的第一大摩擦 ✗）。
    ///
    /// **判据** ✓：① 这个路由回 200 且不是 `method_not_allowed`（修复前正是它 ✗ —— 只能 POST ✓）；
    /// ② 清单里的工具名**与注册表逐个相同** ✓（两边现在都调 `tools_list_json` ✓ ⇒ "逐字相同"是**结构性**的 ✓，
    /// 这里把它钉住 ✓）；③ 每一条都带 `inputSchema` 与 `description` ✓（否则 agent 还是得读源码 ✗）。
    #[test]
    fn the_tool_catalogue_is_served_over_http_and_matches_the_registry() {
        let state = state();
        let response = route(&state, &request("GET", "/api/tools"));
        assert_eq!(
            response.status, 200,
            "GET /api/tools 必须可用（修复前是 405 ✗）"
        );
        let value = body_json(&response);
        let listed: Vec<String> = value["tools"]
            .as_array()
            .expect("tools 应当是数组")
            .iter()
            .map(|tool| tool["name"].as_str().unwrap_or("").to_owned())
            .collect();
        let expected: Vec<String> = state
            .registry
            .tools()
            .iter()
            .map(|tool| tool.name.to_owned())
            .collect();
        assert_eq!(listed, expected, "HTTP 清单必须与注册表逐个相同 ✓");
        assert!(
            !listed.is_empty(),
            "清单不能是空的（空清单比没有清单更糟 ✗）"
        );
        for tool in value["tools"].as_array().unwrap() {
            let name = tool["name"].as_str().unwrap_or("");
            assert!(
                tool["inputSchema"].is_object(),
                "{name} 必须带 inputSchema（否则 agent 只能去读源码 ✗）"
            );
            assert!(
                tool["description"].as_str().unwrap_or("").len() > 4,
                "{name} 必须有可读的 description ✓"
            );
        }
        // **profiles 也一起报** ✓（调用方才知道"哪些组开着" ✓ —— 与未知工具那条错误同源 ✓）。
        assert!(
            value["profiles"].is_array(),
            "清单里应当带 profiles：{value}"
        );
    }

    /// 建一个文档并返回 Editor token（本模块多个测试需要）。
    fn editor_token(state: &ServerState, doc_id: &str) -> String {
        let mut workspace = state.workspace.lock().unwrap();
        workspace
            .create_document(NewDocument::new(doc_id, 32, 32), "human:1", "session:test")
            .unwrap();
        workspace
            .issue_token(doc_id, "human:1", Role::Editor)
            .unwrap()
            .as_str()
            .to_owned()
    }

    /// 调一次工具并返回响应体。
    fn call_tool(
        state: &ServerState,
        doc_id: &str,
        token: &str,
        tool: &str,
        args: &Value,
    ) -> Value {
        let mut request = request(
            "POST",
            &format!("/api/tools/{tool}?doc={doc_id}&token={token}"),
        );
        request.body = serde_json::to_vec(args).unwrap();
        body_json(&route(state, &request))
    }

    /// 上传字节到 `POST /api/blob`。
    fn upload_blob(state: &ServerState, doc_id: &str, token: &str, bytes: Vec<u8>) -> Value {
        let mut request = request("POST", &format!("/api/blob?doc={doc_id}&token={token}"));
        request.body = bytes;
        body_json(&route(state, &request))
    }

    /// **上传 PNG 会自动归一化成原始 RGBA** ✓（设计 791 行 ✓）—— 这是**工具/API**那条路 ✓；
    /// 界面上传走的是浏览器解码 ✓（`importLocalImage` ✓），所以这个缺口此前被遮住了 ✓。
    #[test]
    fn uploading_a_png_stores_normalised_raw_pixels() {
        let state = state();
        let token = editor_token(&state, "doc_png");
        // 用**真实编码器**产出的夹具 ✓（PIL/zlib ✓，含 dynamic Huffman ✓）。
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../yanshi-render/tests/fixtures/large_rgb.png");
        let png = std::fs::read(&path).unwrap_or_else(|error| panic!("读取夹具 {path:?}：{error}"));
        let mut png_request = request("POST", &format!("/api/blob?doc=doc_png&token={token}"));
        png_request
            .headers
            .insert("content-type".to_owned(), "image/png".to_owned());
        png_request.body = png.clone();
        let uploaded = body_json(&route(&state, &png_request));
        assert_eq!(uploaded["ok"], json!(true), "{uploaded}");
        // **归一化必须可见** ✓：报告来源格式 ✓、尺寸 ✓，并且入库的是 raw ✓。
        assert_eq!(
            uploaded["decoded_from"]["format"],
            json!("png"),
            "{uploaded}"
        );
        assert_eq!(uploaded["decoded_from"]["width"], json!(64), "{uploaded}");
        assert_eq!(uploaded["decoded_from"]["height"], json!(64), "{uploaded}");
        assert_eq!(
            uploaded["mime_type"],
            json!("image/x-yanshi-raw"),
            "{uploaded}"
        );
        assert_eq!(
            uploaded["size"],
            json!(64 * 64 * 4),
            "入库的应是解好的 RGBA：{uploaded}"
        );

        // ② **坏 PNG 必须明确报错** ✓，而不是入库一堆垃圾 ✓。
        let mut broken_request = request("POST", &format!("/api/blob?doc=doc_png&token={token}"));
        broken_request
            .headers
            .insert("content-type".to_owned(), "image/png".to_owned());
        broken_request.body = png[..64].to_vec();
        let broken = route(&state, &broken_request);
        assert_eq!(broken.status, 400, "坏 PNG 应当是 400：{broken:?}");

        // ③ **JPEG/WebP 显式拒绝** ✓（本仓库只自研了 PNG 解码器 ✓，零外部依赖 ✓）。
        let mut jpeg_request = request("POST", &format!("/api/blob?doc=doc_png&token={token}"));
        jpeg_request
            .headers
            .insert("content-type".to_owned(), "image/jpeg".to_owned());
        jpeg_request.body = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        assert_eq!(
            route(&state, &jpeg_request).status,
            400,
            "JPEG 应当被明确拒绝而不是静默入库"
        );
    }

    #[test]
    fn copying_a_document_requires_the_source_token_and_produces_a_listed_copy() {
        let state = state();
        let token = editor_token(&state, "doc_src");
        assert_eq!(
            call_tool(
                &state,
                "doc_src",
                &token,
                "create_layer",
                &json!({"layer_id": "L"})
            )["ok"],
            json!(true)
        );
        assert_eq!(
            call_tool(
                &state,
                "doc_src",
                &token,
                "fill",
                &json!({"layer_id": "L",
                        "data": {"color": {"r": 10, "g": 90, "b": 200, "a": 255},
                                 "region": {"x": 0, "y": 0, "w": 32, "h": 32}}})
            )["ok"],
            json!(true)
        );

        // ① 没有源文档令牌 ⇒ 必须被拒绝（目标文档的令牌管不到源文档）。
        let mut create = request("POST", "/api/documents");
        create.body =
            serde_json::to_vec(&json!({"doc_id": "doc_copy_bad", "copy_from": "doc_src"})).unwrap();
        let denied = body_json(&route(&state, &create));
        assert_eq!(denied["ok"], json!(false), "缺少源令牌应被拒绝：{denied}");

        // ② 带上源令牌 ⇒ 成功，并返回副本自己的令牌。
        let mut create = request("POST", "/api/documents");
        create.body = serde_json::to_vec(&json!({
            "doc_id": "doc_copy_ok", "copy_from": "doc_src", "from_token": token
        }))
        .unwrap();
        let copied = body_json(&route(&state, &create));
        assert_eq!(copied["ok"], json!(true), "另存为副本应成功：{copied}");
        assert!(
            copied["copied_atoms"].as_u64().unwrap_or(0) >= 1,
            "{copied}"
        );
        let copy_token = copied["token"].as_str().expect("应返回副本令牌").to_owned();

        // ③ 副本出现在文档列表里，且能用自己的令牌读写。
        let listed = body_json(&route(&state, &request("GET", "/api/documents")));
        let ids: Vec<String> = listed["documents"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["doc_id"].as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            ids.contains(&"doc_copy_ok".to_owned()),
            "文档列表应含副本：{listed}"
        );
        assert_eq!(
            call_tool(
                &state,
                "doc_copy_ok",
                &copy_token,
                "create_layer",
                &json!({"layer_id": "L2"})
            )["ok"],
            json!(true),
            "副本应可用自己的令牌继续编辑"
        );
    }

    #[test]
    fn uploaded_blob_can_be_imported_and_renders_pixels() {
        let state = state();
        let token = editor_token(&state, "doc_import");

        // 4×4 不透明红色 RGBA8 —— 查看器从本地文件解出的就是这种格式（`image/x-yanshi-raw`）。
        let mut pixels = Vec::new();
        for _ in 0..16 {
            pixels.extend_from_slice(&[220, 30, 40, 255]);
        }
        let uploaded = upload_blob(&state, "doc_import", &token, pixels.clone());
        assert_eq!(uploaded["ok"], json!(true), "{uploaded}");
        assert_eq!(uploaded["size"], json!(pixels.len()), "{uploaded}");
        let hash = uploaded["blob_hash"]
            .as_str()
            .expect("应返回 blob_hash")
            .to_owned();

        assert_eq!(
            call_tool(
                &state,
                "doc_import",
                &token,
                "create_layer",
                &json!({"layer_id": "L"})
            )["ok"],
            json!(true)
        );
        let imported = call_tool(
            &state,
            "doc_import",
            &token,
            "import_image",
            &json!({
                "layer_id": "L",
                "bitmap": {"blob_hash": hash, "size": pixels.len(), "mime_type": "image/x-yanshi-raw"},
                "region": {"x": 0, "y": 0, "w": 4, "h": 4}
            }),
        );
        assert_eq!(imported["ok"], json!(true), "导入应成功：{imported}");

        // 直接问内核/服务端要该区域像素：必须看到导入的红色。
        let rendered = call_tool(
            &state,
            "doc_import",
            &token,
            "render_region",
            &json!({"region": {"x": 0, "y": 0, "w": 4, "h": 4}, "raw": true}),
        );
        assert_eq!(rendered["ok"], json!(true), "{rendered}");
        // 注意：响应里的地址已被改写成可直接 GET 的 `/api/blob/<hash>?doc=..` 形式，
        // 不再是 `yanshi://blob/<hash>`（我第一次就按后者解析，结果拿到非法 hash）。
        let raw_url = rendered["raw_url"]
            .as_str()
            .expect("应返回 raw_url")
            .to_owned();
        let hash = raw_url
            .trim_start_matches("/api/blob/")
            .split('?')
            .next()
            .unwrap_or_default()
            .to_owned();
        let bytes = state
            .workspace
            .lock()
            .unwrap()
            .store()
            .get(&hash.parse().unwrap())
            .expect("应能取回原始像素");
        assert_eq!(bytes.len(), pixels.len(), "渲染字节数应与区域匹配");
        assert!(
            bytes[0] > 150 && bytes[1] < 120,
            "导入后的像素应偏红（实际 r={} g={}）—— 导入路径没有生效",
            bytes[0],
            bytes[1]
        );
    }

    #[test]
    fn blob_upload_rejects_an_empty_body() {
        let state = state();
        let token = editor_token(&state, "doc_import");
        let empty = upload_blob(&state, "doc_import", &token, Vec::new());
        assert_eq!(empty["ok"], json!(false), "空上传应被拒绝：{empty}");
    }

    #[test]
    fn health_and_viewer_do_not_require_tokens() {
        let state = state();
        let health = route(&state, &request("GET", "/health"));
        assert_eq!(health.status, 200);
        assert_eq!(body_json(&health)["ok"], json!(true));
        assert!(body_json(&health)["tools"].as_u64().unwrap() > 27);
        // 可观测性：缓存与内存字段必须存在（设计 1319 行的可观测性要求）。
        let health = body_json(&health);
        assert!(
            health["cache"]["used_bytes"].is_u64(),
            "缓存字段缺失：{health}"
        );
        assert!(
            health["cache"]["evictions"].is_u64(),
            "淘汰计数缺失：{health}"
        );
        // 未打开任何文档时缓存为空，且像素估算为 0。
        assert_eq!(health["cache"]["tiles"], json!(0), "{health}");
        assert_eq!(
            health["cache"]["pixel_bytes_estimate"],
            json!(0),
            "{health}"
        );

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

    /// 回归：新增工具组必须同时纳入服务端默认 profile，否则会出现
    /// 「工具存在但线上报未知工具」的割裂（`add_adjustment` 曾如此）。
    #[test]
    fn default_http_profiles_cover_every_implemented_group() {
        let options = HttpOptions::default();
        let registry = ToolRegistry::with_profiles(&options.profiles);
        let mut groups: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for tool in registry.tools() {
            groups.insert(tool.profile.as_str());
        }
        assert!(groups.contains("core"));
        assert!(groups.contains("history"));
        assert!(groups.contains("annotation"));
        assert!(groups.contains("collab"));
        assert!(groups.contains("structure"));
        assert!(
            groups.contains("retouch"),
            "retouch 组必须默认启用：{groups:?}"
        );
        for required in [
            "add_adjustment",
            "add_filter",
            "update_filter",
            "list_effects",
        ] {
            assert!(
                registry.get(required).is_some(),
                "{required} 应在默认 profile 下可用"
            );
        }
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
