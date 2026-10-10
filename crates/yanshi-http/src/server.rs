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
//! | GET | `/api/diagnostics?doc=..` | token | 诊断包（zip，两面的采集实现同一份）|
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

/// **★ 本次进程的 GPU 模式 ✓ ★**（第 606 轮 ✓）：**auto（默认）｜on｜off**。
/// **∴ 它只记录请求的模式 ✗**；**实际后端见 `/health` 的 `render_backend`** ✓。
/// **∴ 二者分离 ⇒ "要求 GPU 但实际用 CPU" 一眼可见 ✗，不会被掩盖 ✓**。
static GPU_MODE: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// **★ `--cache-bust` 的运行时后缀 ✗ ★**（第 408 轮 ✓；**用户提的测试手段 ✓）。
///
/// **∴ 为什么用 `OnceLock` ✗**：**SW 路由**拿不到 `HttpOptions`**✗
///   ⇒ **∴ 所以**在 `serve()` 里**存一次**✗ ⇒ **∴ 路由**读它** ✓
///     （**∴ 与** `GPU_MODE` **同一套写法** ✓）
static CACHE_BUST: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// **算 SW 缓存名里的后缀** ✓（**空 = 不加后缀** ✓）。
///
/// **∴ 空串的含义 ✗**：**用户传了 `--cache-bust` **但没给值**✗
///   ⇒ **∴ 于是**取**当前时间**当后缀**✗ ⇒ **∴ 每次启动**都不同** ✓ ★**** ✓✓
fn cache_bust_suffix() -> String {
    match CACHE_BUST.get() {
        None => String::new(),
        Some(token) if token.is_empty() => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            format!("-bust{now}")
        }
        Some(token) => format!("-bust{token}"),
    }
}

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
    /// **★ 导出静态页 ✓ ★**（第 636 轮 ✓；**部署矩阵 §14.18 ✓**）：**给了路径 ⇒ 把
    /// `viewer::page_with_read_tools()` 生成的那一份 HTML 写进文件并退出 ✗**
    /// ⇒ **∴ 于是**PWA 可以**同步同一份页面 ✗**（**不手写第二份 ⇒ 不分叉 ✓**）。
    /// **★ 一传就让所有缓存失效 ✗ ★**（第 408 轮 ✓；**用户提的测试手段 ✓）：
    ///
    /// **∴ 用途 ✗**：**定位「**改了代码却看不出变化**」这类问题** ✓ ——
    ///   **∴ 传 `--cache-bust`（**不带值 ✓）⇒ 服务端**每次启动**生成一个**唯一后缀** ✓
    ///     ⇒ **∴ 于是** SW 的缓存名 `yanshi-shell-<BUILD_ID>-<后缀>` **每次都不同** ✓
    ///       ⇒ **∴ 浏览器**必然**重新预缓存 ⇒ **看到的一定是新代码** ✓ ★**** ✓✓
    ///   **∴ 传 `--cache-bust=<token>` ✗** ⇒ **用**你给的那个 token** ✓
    ///     （**∴ 便于**多次运行**复用同一个** ✓）
    ///
    /// **∴ 代价（**两面 ✓）★**：**每次启动**都重装外壳**✗ ⇒ **∴ 首次加载**变慢** ✓
    ///   ⇒ **∴ 所以**它**只该用于**开发／排查**✗，**不该**进生产** ✓
    pub cache_bust: Option<String>,
    /// **把 `viewer::page_with_read_tools()` 生成的那一份 HTML 写进文件并退出** ✓
    /// ⇒ **∴ 于是** PWA 可以**同步同一份页面**（**不手写第二份 ⇒ 不分叉 ✓**）。
    pub export_viewer_html: Option<std::path::PathBuf>,
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
            cache_bust: None,
            export_viewer_html: None,
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
                // **★ GPU 模式 ✓ ★**（第 606 轮 ✓；**部署矩阵 ＋ GPU 优先决策 ✓**）：
                // auto（默认）⇒ GPU 优先 ＋ 不可用则 CPU ✓；on ⇒ 要求 GPU ✓；off ⇒ 人为强制 CPU ✓。
                // **⚠️ 现状 ✗**：**还没有 GPU 后端 ⇒ 任何模式的实际后端都是 cpu ✓**；
                // **∴ `/health` 会如实报出 `gpu_mode` 与 `render_backend` ✗ ⇒ 不撒谎 ✓**。
                "--export-viewer-html" => {
                    options.export_viewer_html = Some(value_of("--export-viewer-html")?.into());
                }
                // **★ `--cache-bust` ✗ ★**（第 408 轮 ✓）：**一传 ⇒ 全部缓存失效** ✓
                //   **∴ 两种写法都支持 ✗**：`--cache-bust`（**自动用时间戳 ✓）
                //     ＋ `--cache-bust=<token>`（**用你给的 ✓）
                "--cache-bust" => {
                    options.cache_bust = Some(String::new()); // 空串 ⇒ 启动时补时间戳
                }
                other if other.starts_with("--cache-bust=") => {
                    options.cache_bust = Some(other["--cache-bust=".len()..].to_owned());
                }
                "--gpu" => {
                    let mode = value_of("--gpu")?;
                    if !matches!(mode.as_str(), "auto" | "on" | "off") {
                        return Err(format!("--gpu 只接受 auto／on／off（收到 {mode}）"));
                    }
                    // **★ 同步告知渲染层 ✗ ★**（**第 461 轮 ✓；**目标第 7 条 ✓）：
                    //   **∴ 为什么 ✗**：**`GPU_MODE` **只在本 crate 内被读**✗
                    //     ⇒ **∴ 而**渲染在 `yanshi-render`** ✓
                    //       ⇒ **∴ 所以**：**`--gpu off` **必须**同时告诉它** ✓ ★**** ✓✓
                    yanshi_render::set_gpu_disabled(mode == "off");
                    let _ = GPU_MODE.set(mode);
                }
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
        // **★ 构建期导出路径 ✓ ★**（第 636 轮 ✓）：**∴ 它**不改服务端行为 ✗**
        //（**∴ 只有显式给参数才走 ✓**）⇒ **∴ 写完即退出 ✓**，**不启动监听 ✓**。
        if let Some(path) = options.export_viewer_html.as_ref() {
            std::fs::write(path, crate::viewer::page_with_read_tools())
                .map_err(|error| format!("写静态页失败 {path:?}：{error}"))?;
            std::process::exit(0);
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
           ★ --cache-bust [<token>] ★ 附一个唯一后缀到 SW 缓存名 ⇒ 客户端必然重装外壳 ⇒
                              「改了代码却看不出变化」不再可能（不给 token ⇒ 用启动时间；测试用）
           --help             显示帮助\n\
         \n\
         打开 http://127.0.0.1:8080/ 使用最小 Web 查看器（URL 中的 token 即文档 capability）。\n"
    }
}

/// 共享服务状态。
pub struct ServerState {
    /// 工作区。
    pub workspace: Mutex<Workspace>,
    /// **在飞变更操作登记表** ✓（外部测试报告 P1）。
    ///
    /// **与 `Workspace` 里那份是同一张表** ✓（`Arc` 克隆 ✓），但它**不经过工作区锁** ✓ ——
    /// 长操作正持着那把锁 ✓，所以"忙不忙 / 取消 / 观察"必须在**取锁之前**回答 ✓，
    /// 否则第二个请求会排队等到长操作结束 ✗（正是报告里的现象 ✓）。
    pub inflight: Arc<yanshi_server::InflightRegistry>,
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
    // **★ 存下 `--cache-bust` ✗ ★**（第 408 轮 ✓）：**SW 路由**拿不到 options**✗
    //   ⇒ **∴ 所以**在这里**存一次**✗ ⇒ **∴ 于是**路由能算缓存名后缀** ✓
    //     **∴ 空串 ⇒ **启动时**取时间戳** ✗ ⇒ **∴ 每次运行**都换缓存名** ✓ ★**** ✓✓
    // **∴ 只有**真的传了**才设置 ✗**（第 409 轮 ✓ 实测修 ✓）：
    //   **∴ 原来的写法 ✗**：`unwrap_or_default()` ⇒ **∴ `None` **也变成**空串**✗
    //     ⇒ **∴ 于是** `cache_bust_suffix()` **以为**用户传了 `--cache-bust`** ✓
    //       ⇒ **∴ 于是**「**不带参数**」也带上时间戳**✗ ⇒ **∴ 默认行为**被改变** ✓ ★**** ✓✓
    //   **∴ 修法 ✗**：**只在 `Some` 时设置**✗ ⇒ **∴ 于是** `CACHE_BUST.get()` **返回 `None`** ✓
    //     ⇒ **∴ 默认（**不传 ✓）**缓存名**不加后缀** ✓ ★**** ✓✓
    if let Some(token) = options.cache_bust.clone() {
        let _ = CACHE_BUST.set(token);
    }
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
        yanshi_server::diagnostics::log_line(format!("  {note}"));
        resolved
    });
    // **启动即记一行** ✓：`--bind 0.0.0.0` 这种部署细节、
    // 以及"这是哪个 commit"——都从这一刻起进环形缓冲 ✓，随诊断包一起交出去 ✓。
    yanshi_server::diagnostics::log_line(format!(
        "yanshi-serve 启动：{}｜bind={}｜root={}｜profiles={:?}｜assets_dir={:?}",
        build_identity(),
        options.bind,
        options
            .root
            .as_ref()
            .map(|root| root.display().to_string())
            .unwrap_or_else(|| "<内存>".to_owned()),
        options
            .profiles
            .iter()
            .map(|profile| profile.as_str())
            .collect::<Vec<_>>(),
        options.assets_dir,
    ));
    let registry = ToolRegistry::with_profiles(&options.profiles);
    // **登记表的共享句柄要在工作区进锁之前拿到** ✓（同一张表 ✓，见 `ServerState::inflight` ✓）。
    let inflight = Arc::clone(workspace.inflight());
    let state = Arc::new(ServerState {
        workspace: Mutex::new(workspace),
        inflight,
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

/// 把 `strip_prefix` 取到的**资源路径段**做**百分号解码**（路径语义，见 [`crate::http::percent_decode_path`]）。
///
/// **为什么必须解码**（真实 bug）：九支随仓库发布的笔刷名字里带 `#`
///（`8B_Pencil#1.myb` / `arrow#1.myb` / `Fan#1.myb` / `Flat2#1.myb` / `Fountain_SF#1.myb` /
/// `Fount-offset#1.myb` / `HalfTone#1.myb` / `HalfToneCMY#1.myb` / `Round#1.myb`）。
/// `#` 在 URL 里是**片段起点** ⇒ 浏览器根本不会把它发出去（查看器已改成编成 `%23`），
/// 服务端若不解码，就会去找一个字面名为 `8B_Pencil%231.myb` 的文件 ⇒ 九支笔刷**全都取不到**。
/// `%`（`100%_Opaque.myb`）与 `+`（`blend+paint.myb`）是同一类字符。
///
/// **解码在守卫之前，且不放宽任何一条**：各资源处理器拿到的是**解码后**的名字，
/// 它自己的守卫（拒 `..`、`/`、`\\`、扩展名不符）**一字未改**地作用其上
/// ⇒ `%2e%2e` 会被还原成 `..` 并被同一条规则拒掉，即**解码只会让守卫更严**
///（以前 `%2e%2e` 只是个碰不到任何文件的字面名）。反过来，任何能经解码得到的名字，
/// 本来就能用**原始字节**直接写进请求行（HTTP 请求目标是原样字符串，服务端不做路径归一化）
/// ⇒ **解码不会引入新的穿越路径**。
/// 用**路径版**解码器（`+` 保持字面），否则 `blend%2Bpaint.myb` 会被解成 `blend paint.myb`。
fn decode_asset_segment(file: &str) -> String {
    crate::http::percent_decode_path(file)
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
    // **工程包导入** ✓：浏览器把用户电脑上的 `.yanshi` **字节**交给服务端 ✓。
    //
    // **为什么必须在 `strip_prefix("/api/documents/")` 之前** ✗：那条前缀路由会把
    // `/api/documents/import` 当成"文档 id 叫 import" ✓ ⇒ 走到 `document_summary` ✗
    //（GET 200/404、POST 405），表现为"导入路由不存在" ✓。
    // 这不是假想的坑 ✓：本项目已经因为"两条路由前缀互相遮蔽"翻过一次车 ✓
    // ⇒ 这里同时用注释与判据把它钉住 ✓（判据里有一条正对着它 ✓）。
    if path == "/api/documents/import" {
        return match method {
            "POST" => import_document(state, request),
            _ => method_not_allowed(request, "POST"),
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
    // **诊断包** ✓（P0 事故复盘：事发时柜台是空的 ✗）—— 采集实现与 MCP 工具**同一份** ✓
    //（`yanshi_server::diagnostics::collect` ✓）⇒ 包内条目不会漂移 ✓。
    // **只读** ✓：viewer 令牌也能取 ✓（出事了却只有 owner 能取证 = 没有取证能力 ✓）。
    if path == "/api/diagnostics" {
        return match method {
            "GET" => diagnostics_route(state, request),
            _ => method_not_allowed(request, "GET"),
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
    // **下列资源路由都先做路径段百分号解码**（`decode_asset_segment`）：
    // 它必须在各处理器自己的白名单/穿越守卫**之前**，且只做"还原"、不改守卫。
    if let Some(file) = path.strip_prefix("/wasm/") {
        let file = decode_asset_segment(file);
        return match method {
            "GET" => wasm_asset(state, &file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/samples/") {
        // **示例画面** ✓（随仓库发布 ✓）：换一台机器也能打开示例看到内容 ✓。
        // 用**白名单**而不是拼路径 ✓（与品牌资源同一考虑 ✓）。
        let file = decode_asset_segment(file);
        return match method {
            "GET" => sample_asset(&file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/textures/") {
        // **纹理缩略图** ✓（目标 (b) ✓）：界面里要显示预览 ✓，
        // 而浏览器不能直接读服务器上的文件 ✓ ⇒ 得像介质那样**由服务端发** ✓。
        let file = decode_asset_segment(file);
        return match method {
            "GET" => texture_asset(state, &file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/mediums/") {
        // 笔刷介质插件（设计 11.1）：宿主实例化 wasm 插件时来取 ✓。
        let file = decode_asset_segment(file);
        return match method {
            "GET" => medium_asset(state, &file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brush-previews/") {
        // **预生成入库的画笔库预览图**（构建期生成、随包发布）—— 面板直接用图片，不再逐支实时渲染。
        let file = decode_asset_segment(file);
        return match method {
            "GET" => brush_preview_asset(state, &file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brushes/") {
        // **`.myb` 文本** ✓（浏览器拿它喂门面 ✓）—— 与 `/mediums/` **同一条白名单规矩** ✓。
        let file = decode_asset_segment(file);
        return match method {
            "GET" => brush_text_asset(state, &file),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if let Some(file) = path.strip_prefix("/brand/") {
        let file = decode_asset_segment(file);
        return match method {
            "GET" => brand_asset(state, &file),
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
                // **★ 后缀来自 `--cache-bust` ✗ ★**（第 408 轮 ✓）：
                //   **∴ 传了它 ✗** ⇒ **∴ 缓存名**带唯一后缀** ⇒ **∴ 浏览器**必然重装外壳** ✓
                //     ⇒ **∴ 于是**「**改了代码却看不出变化**」**不再可能** ✓ ★**** ✓✓
                let build_id = format!("{BUILD_ID}{}", cache_bust_suffix());
                let body = SERVICE_WORKER_JS.replace("__BUILD_ID__", &build_id);
                Response::bytes(200, "text/javascript; charset=utf-8", body.into_bytes())
                    .with_header("Cache-Control", "no-cache")
            }
            _ => method_not_allowed(request, "GET"),
        };
    }
    // **(A)①：拆出来的两个资产必须有**可路由的 URL** ✓** —— 否则它们只是"编译进二进制"✗，
    // SW 也就**无法把它们作为独立资源预缓存** ✗（**离线外壳的完整性靠这个** ✓）。
    if path == "/viewer.css" {
        return match method {
            "GET" => Response::bytes(
                200,
                "text/css; charset=utf-8",
                viewer::stylesheet().as_bytes().to_vec(),
            ),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/viewer-app.js" {
        return match method {
            "GET" => Response::bytes(
                200,
                "text/javascript; charset=utf-8",
                viewer::app_script().as_bytes().to_vec(),
            ),
            _ => method_not_allowed(request, "GET"),
        };
    }
    // **★ C/S 本地渲染模式**：`api-local.js` + `store.js` 是 PWA 本地 API 的实现，
    //   C/S 前端在"关闭服务器渲染"时复用它们（`callTool` 走本地 WASM）。
    //   与 `viewer-app.js` 同样内嵌，保證单二进制部署。
    if path == "/api-local.js" {
        return match method {
            "GET" => Response::bytes(
                200,
                "text/javascript; charset=utf-8",
                include_str!("../assets/api-local.js").as_bytes().to_vec(),
            ),
            _ => method_not_allowed(request, "GET"),
        };
    }
    if path == "/store.js" {
        return match method {
            "GET" => Response::bytes(
                200,
                "text/javascript; charset=utf-8",
                include_str!("../assets/store.js").as_bytes().to_vec(),
            ),
            _ => method_not_allowed(request, "GET"),
        };
    }
    // **★ `brush-local.js` 也必须在这里**✗ ★**（第 384 轮 ✓；**实测根因 ✓）：
    //   **∴ 症状 ✗**：**判据**落笔**永远画不出墨**✗（**在线也一样 ✓）**：
    //     **∴ `brush_tool` 返回 `stroke_failed`**✗
    //       ⇒ **∴ `reason = "Failed to fetch dynamically imported module: /brush-local.js"`** ✓
    //         ⇒ **∴ 于是** `import("/brush-local.js")` **404** ✓
    //           ⇒ **∴ 落笔链路**第一步就断** ✓ ★**** ✓✓
    //   **∴ 为什么以前没暴露 ✗**：**它**走的是**磁盘 `--assets-dir assets`**✗
    //     ⇒ **∴ 而** `assets/brush-local.js` **不存在** ✓
    //       ⇒ **∴ 于是** 404** ✓
    //         ⇒ **∴ 而** PWA（**Cloudflare ✓）**有它**✗ ⇒ **∴ 所以**只有**服务端模式**中招** ✓ ★**** ✓✓
    //   **∴ 修法 ✗**：**与 `api-local.js`／`store.js` **同样内嵌**✗
    //     ⇒ **∴ 于是**单二进制部署**也带上它** ✓ ★**** ✓✓
    //   **∴ 两面（**AGENTS.md 第 3 条 ✓）★**：
    //     **∴ 收益 ✗**：**服务端模式**终于能落笔**✗ ⇒ **∴ 内核路径**可用** ✓**** ✓✓
    //     **∴ 代价 ✗**：**二进制**多约 6 KB**✗（**∴ 可忽略 ✓）
    //       ＋ **∴ 且**：**`crates/yanshi-http/assets/brush-local.js` **必须与 `web/` 同步** ✗
    //         ⇒ **∴ 否则**两边漂移** ✓ ⇒ **∴ 所以**要**加一条判据守住** ✓ ★**** ✓✓
    if path == "/brush-local.js" {
        return match method {
            "GET" => Response::bytes(
                200,
                "text/javascript; charset=utf-8",
                include_str!("../assets/brush-local.js").as_bytes().to_vec(),
            ),
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
        // **关闭与删除是两件事，所以是两条路** ✗（产品负责人要的是**真删** ✓）：
        // `POST /api/documents/<id>/close` ⇒ 只放下内存 ✓（磁盘上还在 ✓，再打开就回来 ✓）；
        // `DELETE /api/documents/<id>?confirm=<id>` ⇒ **不可逆地删掉磁盘上的文档目录** ✓。
        // 以前 `DELETE` 走的是**关闭** ✓ —— 那正是"子 agent 报成 bug"的那条 ✓：
        // 一个方法名叫 DELETE 却只关内存 ✓，谁都读不出这个语义 ✓ ⇒ 现在各归各位 ✓。
        if let Some(id) = id.strip_suffix("/close") {
            return match method {
                "POST" => close_document(state, request, id),
                _ => method_not_allowed(request, "POST"),
            };
        }
        return match method {
            "GET" => document_summary(state, request, id),
            "DELETE" => delete_document(state, request, id),
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
            // **位图缓存的命中率** ✓（第 93 轮 ✓，纯观测 ✓）—— tile 缓存与它**是两套** ✓
            //（前者缓存"渲染出的 tile"✓，后者缓存"解码后的位图"✓）⇒ **∴ 要分开看** ✓。
            // ⚠️ 教训 ✓：**`json!` 里不能写语句块** ✗（`unexpected end of macro invocation` ✓）⇒ **∴ 照 `cache_stats` 写成函数调用** ✓。
            "bitmap_cache": bitmap_cache_stats(state),
            "below_reuse": below_reuse_count(state),
            // below 的另外两个累计读数（第 342 轮，纯观测）：只有 below_reuse 时，
            // 判据分不清「真的没命中」与「没走 below 路径」。
            "below_missing": below_missing_count(state),
            "below_wanted": below_wanted_count(state),
            // **★ 渲染后端必须**如实报出** ✓ ★**（第 602 轮 ✓；**部署矩阵 ＋ GPU 优先决策 ✓**）：
            // **∴ 现在**没有 GPU 后端 ✗**（**∴ 全部走 CPU ✓**）⇒ **∴ 这里**永远**如实写 `cpu` ✓**
            // **★ 而**"**环境有没有 GPU**"**是另一个问题**✗ ⇒ **∴ 见下面的 `gpu_probe` ✓ ★**
            //（**∴ 不许因为"用户要求 GPU 优先"就写成 `gpu` ✗** —— **∴ 那是撒谎 ✓**）。
            // **∴ 将来加 GPU 后端时 ✓**：**把这两行改成**从实际后端读**✗**（**如 `renderer.backend()` ✓**），
            // **并**在 `--gpu=off` 时仍报 `cpu` ✓****。
            // **★★ 第 258 轮我犯的错（**如实记 ✓）★★**：
            //   **∴ 我**把这两行**改成**由 `gpu_probe()` 决定**✗
            //     ⇒ **∴ 于是**：`render_backend` **报了 `gpu`** ✓
            //       ⇒ **★ 而**渲染**实际仍然**是纯 CPU** ✗ ⇒ **∴ 那**是**一个新的撒谎** ✓ ★**** ✓✓
            //         （**∴ 目标第 7 条：**不许假装用了 GPU** ✓）
            //   ⇒ **★ 所以**：**我**把「**假的没有**」换成了「**假的有**」**✗
            //     ⇒ **∴ 已**改回**：**`render_backend` **只报**真实后端** ✓ ★**** ✓✓
            //
            // **∴ 正确的分工 ✗ ★**：
            //   **∴ `render_backend` ✗**：**渲染**真的用了什么**✗
            //     ⇒ **∴ 现在**只有 CPU** ⇒ **∴ 必须**恒为 `cpu`** ✓（**∴ 直到**GPU 后端真的接上 ✓）** ✓✓
            //   **∴ `gpu_probe` ✗**：**环境**有没有 GPU**✗（**∴ 那**是**探测结果** ✓）
            //     ⇒ **∴ 与 `render_backend` **分开报** ✓**** ✓✓
            // **★ 第 323 轮：**改成**动态读取** ✗ ★**（**目标第 7 条 ✓）：
            //   **∴ 它**读的是 `yanshi-render` 的**进程级记录点**✗
            //     ⇒ **∴ 而**那个记录点**只在**真的走完 GPU 路 ＋ 逐位核对通过**后才被设成 `gpu`** ✓
            //       ⇒ **★ 所以**：**`render_backend` **不可能**假装** ✓ ★**** ✓✓
            //   **∴ 没有 `gpu` feature 时 ✗**：**记录点**恒为 `cpu`** ✓
            //     ⇒ **∴ 于是**：**默认构建**仍报 `cpu`** ✓（**∴ 与**引入 GPU 前**一致 ✓）** ✓✓
            //   **∴ 小输入时 ✗**：**规模不够 ⇒ 走 CPU**✗ ⇒ **∴ 记录点**也是 `cpu`** ✓
            //     ⇒ **∴ 于是**：**它**如实反映**这一次**实际用了什么** ✓**** ✓✓
            "render_backend": yanshi_render::last_backend().as_str(),
            "gpu_probe": {
                "available": gpu_probe().0,
                "reason": gpu_probe().1,
            },
            // **★ 名字要说实话 ✗ ★**（**第 463 轮 ✓；**第 446 轮发现 ✓）：
            //   **∴ 原来的错 ✗**：**字段名叫 `gpu_unavailable_reason`**✗
            //     ⇒ **∴ 而**它在**GPU **可用时**也写内容** ✓
            //       （**∴ 如** `adapter:Gl:IntegratedGpu:…:device=true` ✓）
            //         ⇒ **∴ 于是**：**读者**会**误以为 GPU 不可用** ✓ ★**** ✓✓
            //   **∴ 现在 ✗**：**新增 `gpu_adapter_note`**✗（**它**是**能力描述** ✓）
            //     ＋ **∴ 并**保留 `gpu_unavailable_reason` **作为**同值别名**✗
            //       ⇒ **∴ 因为**本仓库有 **6 个脚本**在读旧名** ✓
            //         ⇒ **∴ 于是**：**判据**全绿 ＋ **新读者**用新名** ✓ ★**** ✓✓
            //     ＋ **∴ 两面 ✗**：**收益**：**语义清楚**（**能力 vs 原因 ✓）
            //       ＋ **∴ 代价 ✗**：**字段**暂时重复**✗
            //         ⇒ **∴ 应**在**所有脚本迁完后**删旧名** ✓ ★**** ✓✓
            "gpu_adapter_note": gpu_probe().1,
            "gpu_unavailable_reason": gpu_probe().1, // **∴ 已弃用别名（**兼容 6 个脚本 ✓）
            // **★ §6.3 的 ④ ✗ ★**（第 35 轮 ✓）：**必须报出**后端 ＋ 最大通道差**✗
            //   **∴ 而**这里**没有 GPU** ✗ ⇒ **∴ 没有比较发生过 ✓**
            //     ⇒ **★ 所以 `max_channel_delta` 报 `null` ✗**（**不是 0 ✓）**★**
            //       **∴ 用 0**会**撒谎**✗（**它**读起来像"**比过且一致 ✓"）** ✓✓
            // **★ §6.3 的 ④：**报出后端 ＋ `max_channel_delta`** ✗ ★**（第 292 轮 ✓）：
            //   **∴ 以前**恒为 `null`**✗（**∴ 对 —— **因为**从没比过** ✓）
            //     ⇒ **∴ 现在**：**`--features gpu` ＋ 有适配器时**真的跑一次自检**✓
            //       ⇒ **∴ 于是**：**报出**真实的通道差**（**∴ 0 表示逐位相同 ✓）** ✓✓
            //   **∴ 而**没有 feature 时 ⇒ **∴ 仍然 `null`** ✗
            //     ⇒ **∴ 那**是**诚实的**（**∴ 因为**确实没比过 ✓）** ✓✓
            // **★ `max_channel_delta` **只**反映**渲染路** ✗ ★**（**第 451 轮 ✓；**目标第 4 条 ✓）：
            //   **∴ 原来 ✗**：**它**永远来自**自检**✗（**`gpu_selfcheck_delta()` ✓）
            //     ⇒ **∴ 于是**：**没渲染时**也报 `0`**✗
            //       ⇒ **∴ 判据**读成**「**恒 0 冒充**」** ✓
            //         ⇒ **∴ 而**那**正是**目标第 4 条**禁止的** ✓
            //   **∴ 现在 ✗**：**它**只反映**渲染路**真的比过**的差值**✗
            //     ⇒ **∴ 没比过 ⇒ `null`** ✓（**∴ 那就是**判据要的** ✓）
            //       ＋ **∴ 自检的差值**另立字段**（**`selfcheck_max_channel_delta` ✓）★**** ✓✓
            "max_channel_delta": render_delta_json(),
            "selfcheck_max_channel_delta": gpu_selfcheck_delta(),
            "max_channel_delta_note": gpu_selfcheck_note(),
            // **∴ `gpu_mode` ＝ 请求的模式 ✗；`render_backend` ＝ 实际后端 ✓**（分开报 ✓）。
            "gpu_mode": GPU_MODE.get().cloned().unwrap_or_else(|| "auto".to_owned()),
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
/// **位图缓存的命中／未命中** ✓（第 93 轮 ✓）—— **纯观测** ✓，不参与任何渲染决策 ✓。
///
/// **为什么单列一段** ✗：它与 `cache`（tile 缓存 ✓）**是两套东西** ✓ ——
/// 前者缓存"**渲染出的 tile**" ✓，后者缓存"**解码后的位图**" ✓ ⇒ **∴ 混在一起就看不出是谁的问题** ✓。
/// **below 复用次数** ✓（第 95 轮 ✓，纯观测 ✓）—— 目标第 4 条的判据用它问
/// "**只改当前层时，下方合成有没有被复用**" ✓（现在还没有缓存 ⇒ **恒 0** ✓）。
fn below_reuse_count(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    json!(workspace.below_reuse_count())
}

/// **below 缓存累计缺了几格**（第 342 轮，纯观测）—— 照 `below_reuse_count` 的写法。
fn below_missing_count(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    json!(workspace.below_missing_count())
}

/// **below 缓存累计想要几格**（第 342 轮，纯观测）—— 照 `below_reuse_count` 的写法。
fn below_wanted_count(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    json!(workspace.below_wanted_count())
}

/// **★ GPU 探测：**真的去看**，而不是写死** ✗ ★**（第 258 轮 ✓；**目标第 7 条 ✓**）。
///
/// **∴ 为什么 ✗**：**实测**（**第 256／257 轮 ✓）**✗**：
///   **∴ 本机有**渲染设备**✗**：`/dev/dri/card1` ＋ **`/dev/dri/renderD128`** ✓
///   **∴ 而**服务端**写死** `gpu_unavailable_reason = "host_has_no_gpu"`** ✓
///     ⇒ **∴ 于是**：**判据** `tool-gpu-probe-honesty.mjs` **当场抓住它**（**退出码 1 ✓）** ✓✓
///   **⇒ ★ 所以**：**硬编码的"没有"**与**硬编码的"有"**一样是**撒谎** ✓ ★**** ✓✓
///
/// **∴ 两面（**AGENTS.md 第 5 条 ✓）★**：
///   **∴ 收益 ✗**：**不引入任何新依赖**✗（**∴ 因此**体积 6.6 MiB、89 包、构建时间**都不变** ✓）
///     ⇒ **∴ 且**：**报出的原因**第一次**来自环境** ✓**** ✓✓
///   **∴ 代价 ✗**：**设备节点**不等于**可用驱动**✗
///     ⇒ **∴ 所以**：**本探测**只能说**"看起来有"**✗ ⇒ **∴ 不能**承诺**能用** ✓
///       ⇒ **∴ 因此**：**真正接入 GPU 计算时**必须**再用 `wgpu` 的 `request_adapter()` 复核** ✓
///         （**∴ 那**是**候选 ②**✗，**见第 256 轮评估 ✓）** ✓✓
/// **★ 适配器级探测（**`--features gpu` 时可用 ✓）✗ ★**（第 266 轮 ✓；**目标第 7 条 ✓）。
///
/// **∴ 两面（**AGENTS.md 第 5 条 ✓）★**：
///   **∴ 收益 ✗**：**结论**来自**真实枚举**✗ ⇒ **∴ 不再**靠设备节点推断** ✓
///   **∴ 代价 ✗**：**要起 `wgpu::Instance`**（**∴ 首次**可能几百 ms ✓）
///     ＋ **∴ 它**只在 `--features gpu` 下编译** ⇒ **∴ 默认构建**不受影响** ✓（**45 包／6.62 MiB ✓）** ✓✓
/// **∴ 不新增依赖 ✗**：**future**用**手写 `block_on`** ✓（**∴ `pollster`**不值一个包** ✓）** ✓✓
#[cfg(feature = "gpu")]
fn gpu_adapter_probe() -> (bool, String) {
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        use std::task::{Context, Poll};
        // **∴ 用标准库的 no-op waker ✗**（**第 266 轮 ✓；**clippy `manual_noop_waker` ✓）：
        //   **∴ 我**原来手写了一个 `Wake` 实现**✗ ⇒ **∴ 而** `Waker::noop()` **已足够** ✓**** ✓✓
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut fut = Box::pin(fut);
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::yield_now();
        }
    }
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    match block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) {
        Ok(a) => {
            let info = a.get_info();
            // **★ 再要一个**设备** ✗ ★**（第 270 轮 ✓）：
            //   **∴ 为什么 ✗**：**adapter** 只说明**有可用 GPU** ✗
            //     ⇒ **∴ 而**要**真的能算**✗ ⇒ **∴ 必须**拿到 `Device` ＋ `Queue`** ✓**** ✓✓
            //   **∴ 代价 ✗**：**多一次**初始化**（**∴ 首次**几十 ms ✓）
            //     ＋ **∴ 它**同样只在 `--features gpu` 下编译** ✓**** ✓✓
            let device_ok = block_on(a.request_device(&wgpu::DeviceDescriptor::default()))
                .map(|(_, _)| true)
                .unwrap_or(false);
            let detail = format!(
                "adapter:{:?}:{:?}:{}:device={}",
                info.backend, info.device_type, info.name, device_ok
            );
            (true, detail)
        }
        Err(e) => (false, format!("no_adapter:{e:?}")),
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_adapter_probe() -> (bool, String) {
    (false, "feature_gpu_not_enabled".to_owned())
}

/// **★ 自检的通道差（**`--features gpu` 时才有实数 ✓）✗ ★**（第 292 轮 ✓；**§6.3 ④ ✓）。
///
/// **∴ 为什么返回 `Value` 而不是数字 ✗**：**没有 feature ⇒ **没有比过**✗
///   ⇒ **∴ 那时**必须报 `null`**✗ ⇒ **∴ 不许**用 `0` 冒充** ✓（**∴ 用 0**会读成「**比过且一致**」✓）** ✓✓
/// **∴ 渲染路的比对差值 ✗**（**`None` ⇒ `null`** ✓；**第 451 轮 ✓）
///
/// **∴ 为什么单列 ✗**：**它**与**自检的差值**是**两条路**✗
///   ⇒ **∴ 混用**会让「**没比过**」看起来像「**比过且一致**」** ✓
///     ⇒ **∴ 而**那**正是**目标第 4 条**禁止的** ✓ **** ✓✓
fn render_delta_json() -> serde_json::Value {
    match yanshi_render::render_delta() {
        Some(v) => serde_json::json!(v),
        None => serde_json::Value::Null,
    }
}

#[cfg(feature = "gpu")]
fn gpu_selfcheck_delta() -> serde_json::Value {
    match crate::gpu_policy::selfcheck::run() {
        Some(r) => serde_json::json!(r.max_channel_delta),
        None => serde_json::Value::Null,
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_selfcheck_delta() -> serde_json::Value {
    serde_json::Value::Null
}

/// **∴ 与差值配套的说明 ✗**（**∴ 让读数**自解释** ✓）
#[cfg(feature = "gpu")]
fn gpu_selfcheck_note() -> String {
    match crate::gpu_policy::selfcheck::run() {
        Some(r) => format!(
            // **★ 文案要说清**哪本账** ✗ ★**（**第 453 轮 ✓；**目标第 4／8 条 ✓）：
            //   **∴ 因为** `max_channel_delta` **现在**只反映**渲染路**✗
            //     ⇒ **∴ 而**自检另立 `selfcheck_max_channel_delta`** ✓
            //       ⇒ **∴ 所以**：**本字段**必须**明说**它是**自检的** ✓ **** ✓✓
            "本条是**自检**的差值（{} 像素与 CPU 真值逐位对比）；\
             **渲染路**的差值在 `max_channel_delta`；**渲染路未做比对时为 null**（§6.3）",
            r.pixels
        ),
        None => "本机没有可用 GPU 适配器 ⇒ 未做 GPU／CPU 比对（§6.3）".to_owned(),
    }
}

#[cfg(not(feature = "gpu"))]
fn gpu_selfcheck_note() -> String {
    "构建未开启 gpu feature ⇒ 未做 GPU／CPU 比对（§6.3）".to_owned()
}

fn gpu_probe() -> (bool, String) {
    // **∴ ① `--gpu off` ✗**：**用户明确关掉** ⇒ **∴ `cpu` 是**正确的**✗，**而**原因**必须**说明是关掉的** ✓
    let mode = GPU_MODE.get().map(String::as_str).unwrap_or("auto");
    if mode == "off" {
        return (false, "disabled_by_flag".to_owned());
    }
    // **∴ ② 找渲染节点 ✗**：`/dev/dri/renderD*` ✓
    let mut has_render_node = false;
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with("renderD") {
                has_render_node = true;
                break;
            }
        }
    }
    // **∴ ③ 找已绑驱动的卡 ✗**：`/sys/class/drm/card*/device/driver` ✓
    let mut has_bound_driver = false;
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("card")
                && !name.contains('-')
                && std::fs::metadata(entry.path().join("device/driver")).is_ok()
            {
                has_bound_driver = true;
                break;
            }
        }
    }
    let (adapter_ok, adapter_reason) = gpu_adapter_probe();
    if adapter_reason != "feature_gpu_not_enabled" {
        return (adapter_ok, adapter_reason);
    }
    if has_render_node {
        let reason = if has_bound_driver {
            "render_node_present".to_owned()
        } else {
            "render_node_without_bound_driver".to_owned()
        };
        (true, reason)
    } else {
        (false, "host_has_no_render_node".to_owned())
    }
}

fn bitmap_cache_stats(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    let (hits, misses) = workspace.bitmap_cache_hits_misses();
    // **未命中字节** ✓（第 133 轮 ✓）：判"小区域有没有整幅解码"看**字节** ✓，不是次数 ✓。
    let missed_bytes = workspace.bitmap_cache_missed_bytes();
    json!({"hits": hits, "misses": misses, "missed_bytes": missed_bytes})
}

fn cache_stats(state: &ServerState) -> serde_json::Value {
    let Ok(workspace) = state.workspace.lock() else {
        return json!({"error": "工作区锁中毒"});
    };
    let (tiles, used_bytes, evictions, misses, hits) = workspace.cache_stats();
    json!({
        "tiles": tiles,
        "used_bytes": used_bytes,
        "evictions": evictions,
        "misses": misses,
        // **命中数** ✓（第 25 轮 ✓）：与 `misses` 一起才能算**命中率** ✓
        //（此前只有 misses ✗ ⇒ 缓存有效性不可观测 ✓）。
        "hits": hits,
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
    if write_role {
        if let Some(response) = require_write_access(state, request) {
            return response;
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

/// **写权限的入口守卫** ✓（第三方代码审计 P0 第 2 条）。
///
/// `/api/documents` 原先任何人无需凭据即可拿到 **Editor** 令牌 ✓，
/// 与"任意路径写"合起来就是"未授权 + 任意写"完整链 ✗。策略（最小且可预期 ✓）：
/// **绑回环** ⇒ 放行 ✓（本机开发/探针都靠它 ✓）；**绑对外** ⇒ 必须带密钥 `YANSHI_API_KEY` ✓；
/// 没配密钥就**明确拒绝**并给出两条出路 ✓。
///
/// **为什么抽成函数** ✓：工程包导入与文档删除都会**新建/销毁落盘文档** ✓，
/// 与"开一个文档"是同一级写操作 ✓ ⇒ **必须用同一条策略** ✓。
/// 各写一份必然漂移 ✗（本项目在这上面栽过不止一次 ✓）。
fn require_write_access(state: &ServerState, request: &Request) -> Option<Response> {
    if bind_is_loopback(&state.options.bind) {
        return None;
    }
    let expected = std::env::var("YANSHI_API_KEY")
        .ok()
        .filter(|value| !value.is_empty());
    let provided = request
        .param("key")
        .map(str::to_owned)
        .or_else(|| request.header("x-yanshi-key").map(str::to_owned));
    match expected {
        None => Some(Response::from_error(&YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "服务端绑在对外地址上，但没配 YANSHI_API_KEY ⇒ 拒绝匿名签发写权限令牌。                                 两条出路：① 只在本机用 ⇒ 用 --bind 127.0.0.1:8080 启动；                                 ② 确实要对外 ⇒ 设 YANSHI_API_KEY=<密钥>，请求时带上 ?key=<密钥>（或 X-Yanshi-Key 头）"
                    .to_owned(),
            ),
        ))),
        Some(key) => {
            if provided.as_deref() == Some(key.as_str()) {
                None
            } else {
                Some(Response::from_error(&YanshiError::new(
                    ErrorCode::PermissionDenied,
                    ErrorContext::detail(
                        "密钥不对（或没带）⇒ 对外绑定时签发写权限令牌需要 ?key=<YANSHI_API_KEY>".to_owned(),
                    ),
                )))
            }
        }
    }
}

/// 导入用的临时目录（`<root>/.imports`）。
///
/// **为什么落在工作区而不是系统临时目录** ✓：同一次导入最终要把整个包读进内存并交给
/// 落盘工作区 ✓ ⇒ 放在**同一个文件系统**上更可控 ✓，也不会在小分区上撑爆 `/tmp` ✗。
fn import_dir(root: &std::path::Path) -> PathBuf {
    root.join(".imports")
}

/// 分片上传 id 的合法形状 ✓（它会被拼进文件名 ✓ ⇒ 必须挡住路径穿越 ✗）。
fn is_safe_upload_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// **`POST /api/documents/import`：把 `.yanshi` 工程包导入成一份新文档** ✓。
///
/// **为什么要有这条路由** ✗：工具层早就有 `import_project` ✓，查看器也早有一个按钮 ✓，
/// 但那条路收的是**服务器上的路径** ✗ —— 浏览器里的用户拿不到自己的文件 ✓
/// ⇒ "导入"在 Web 上**形同虚设** ✓（这正是产品负责人报的缺口 ✓）。
/// 这里补的是**传输**：把**字节**从浏览器送进服务端 ✓，解析与落盘仍然只有一份实现 ✓
///（`Workspace::import_project` ✓ —— 与 MCP 的 `import_project` 工具**同一条** ✓）。
///
/// **为什么是分片协议而不是一次 POST** ✗：HTTP 请求体上限是 **32 MiB** ✓
///（`http::MAX_BODY_BYTES` ✓，且本服务**不支持 chunked** ✓），
/// 而真实工程包是**几百 MB** ✓（产品负责人给的夹具：492 atom / 约 349 MB blob ✓）
/// ⇒ 一次 POST 根本传不进来 ✓。协议三步（全部落在这一条路由上 ✓）：
///
/// 1. `?begin=1` ⇒ 建一个上传会话，回 `upload_id` 与单片上限；
/// 2. `?upload=<id>&offset=<n>` ⇒ 把这一段追加进临时文件，回**服务端实际收到的字节数**；
///    `offset` 与服务端已经收到的字节数不符 ⇒ **409 并告诉客户端该从哪续** ✓
///    （"静默丢弃一段"会让导入出来的文档**悄悄缺内容** ✗ —— 那是最坏的一种成功 ✓）；
/// 3. `?upload=<id>&finish=1[&doc_id=<新 id>]` ⇒ 走真实导入，回**新文档的令牌** ✓。
///
/// **令牌随导入一起回** ✓：令牌是**按文档签发**的 ✗ ⇒ 导入完不给令牌，用户面对的就是
/// "导入成功了但打不开" ✓（旧按钮正是这样，只好让人去敲 curl ✓）⇒ 这里一次给全 ✓。
///
/// **已知的代价**（如实记下 ✗）：`Workspace::import_project` 收 `&[u8]` ✓
/// ⇒ 收尾那一步仍然要把整包读进内存（外加 tar 解析的副本 ✓）
/// ⇒ 分片只解决了"传得进来" ✓，没解决"解析时的内存峰值" ✗（值得单独立项 ✓）。
fn import_document(state: &ServerState, request: &Request) -> Response {
    if let Some(response) = require_write_access(state, request) {
        return response;
    }
    let Some(root) = state.options.root.clone() else {
        return Response::from_error(&YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(
                "导入工程包需要落盘工作区（启动时给 --root）⇒ 纯内存模式没有地方放它".to_owned(),
            ),
        ));
    };
    if request.param("begin").is_some() {
        return import_begin(&root);
    }
    let Some(upload_id) = request.param("upload").map(str::to_owned) else {
        return crate::http::bad_request(
            "导入工程包要分三步：① ?begin=1 建会话；② ?upload=<id>&offset=<已上传字节数> 传分片；\
             ③ ?upload=<id>&finish=1 落盘并签发令牌",
        );
    };
    if !is_safe_upload_id(&upload_id) {
        return crate::http::bad_request("upload id 形状不合法（只允许字母数字与 -_）");
    }
    if request.param("finish").is_some() {
        return import_finish(state, request, &upload_id);
    }
    import_chunk(request, &root, &upload_id)
}

/// 第 ① 步：开一个上传会话（`?begin=1`）。
///
/// 顺手清掉**过期的**半截上传 ✓（24 小时前开始的 ✓）：放弃的会话不该在磁盘上留一辈子 ✗。
/// 只清过期的 ✓ —— 正在传的那个**绝不碰** ✗（时间窗就是这条不变量 ✓）。
fn import_begin(root: &std::path::Path) -> Response {
    let dir = import_dir(root);
    if let Err(error) = std::fs::create_dir_all(&dir) {
        return Response::from_error(&YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("建不了导入临时目录 {}：{error}", dir.display())),
        ));
    }
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 3600);
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let stale = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .map(|modified| modified < cutoff)
                .unwrap_or(false);
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let upload_id = yanshi_core::Ulid::new().encode();
    let path = dir.join(format!("{upload_id}.part"));
    if let Err(error) = std::fs::write(&path, []) {
        return Response::from_error(&YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("建不了上传文件 {}：{error}", path.display())),
        ));
    }
    Response::json(
        200,
        &json!({
            "ok": true,
            "upload_id": upload_id,
            "received": 0,
            // **单片上限照实说** ✓：客户端据此切片 ✓（猜一个数就会撞上 32 MiB 的墙 ✗）。
            "max_chunk_bytes": crate::http::MAX_BODY_BYTES,
        }),
    )
}

/// 第 ② 步：追加一片（`?upload=<id>&offset=<n>`）。
///
/// **同一个会话假定单写者** ✓（查看器就是串行上传 ✓）。真并发同偏移的两片会被偏移检查挡住 ✓
/// —— 也就是**响亮地失败** ✓，而不是两段交错拼出一个坏包 ✗。
fn import_chunk(request: &Request, root: &std::path::Path, upload_id: &str) -> Response {
    let path = import_dir(root).join(format!("{upload_id}.part"));
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Response::from_error(&YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!(
                "上传会话 {upload_id} 不存在（可能已过期被清理）⇒ 请从 ?begin=1 重新开始"
            )),
        ));
    };
    let received = metadata.len();
    let Some(offset) = request
        .param("offset")
        .and_then(|text| text.parse::<u64>().ok())
    else {
        return crate::http::bad_request(
            "分片要带 ?offset=<服务端已收到的字节数>（begin/finish 两回复里都有）",
        );
    };
    if offset != received {
        return Response::from_error(&YanshiError::new(
            ErrorCode::Conflict,
            ErrorContext::detail(format!(
                "分片偏移不符：服务端已经有 {received} 字节，而这一片从 {offset} 开始 ⇒ 请从 {received} 续传（少一段会让导入出来的文档悄悄缺内容）"
            )),
        ));
    }
    let appended = (|| -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
        file.write_all(&request.body)?;
        file.flush()
    })();
    if let Err(error) = appended {
        return Response::from_error(&YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail(format!("写不进上传文件 {}：{error}", path.display())),
        ));
    }
    Response::json(
        200,
        &json!({
            "ok": true,
            "upload_id": upload_id,
            "received": received + request.body.len() as u64,
        }),
    )
}

/// 第 ③ 步：收尾（`?upload=<id>&finish=1`）—— 走真实导入并**把令牌一起给** ✓。
fn import_finish(state: &ServerState, request: &Request, upload_id: &str) -> Response {
    let Some(root) = state.options.root.as_ref() else {
        return internal("导入收尾时工作区根目录消失了");
    };
    let path = import_dir(root).join(format!("{upload_id}.part"));
    let Ok(bytes) = std::fs::read(&path) else {
        return Response::from_error(&YanshiError::new(
            ErrorCode::ReferenceNotFound,
            ErrorContext::detail(format!(
                "上传会话 {upload_id} 不存在（可能已过期被清理）⇒ 请重新上传"
            )),
        ));
    };
    // **收尾是终态** ✓：无论成不成，临时文件都删掉 ✓（留着只会变成一个"看起来还在传"的僵尸 ✗）。
    let _ = std::fs::remove_file(&path);
    if bytes.is_empty() {
        return crate::http::bad_request("上传内容为空 ⇒ 这不是一个 .yanshi 工程包");
    }
    let doc_id = request.param("doc_id").map(str::to_owned);
    let actor = "human:web";
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let imported = match workspace.import_project(&bytes, doc_id.as_deref()) {
        Ok(value) => value,
        Err(error) => return Response::from_error(&error),
    };
    let Some(doc_id) = imported.get("doc_id").and_then(Value::as_str) else {
        return internal("导入成功却没有回 doc_id");
    };
    let token = match workspace.issue_token(doc_id, actor, Role::Editor) {
        Ok(token) => token,
        Err(error) => return Response::from_error(&error),
    };
    Response::json(
        200,
        &json!({
            "ok": true,
            "doc_id": doc_id,
            "token": token.as_str(),
            "url": format!("/?doc={doc_id}&token={token}"),
            "atoms": imported.get("atoms").cloned().unwrap_or(Value::Null),
            "blobs": imported.get("blobs").cloned().unwrap_or(Value::Null),
            "entries": imported.get("entries").cloned().unwrap_or(Value::Null),
            "had_render": imported.get("had_render").cloned().unwrap_or(Value::Null),
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

/// **`GET /api/diagnostics`：把诊断包（zip）直接交给浏览器** ✓。
///
/// **为什么是独立的一条路由、而不是让查看器去调工具** ✓：工具把 zip 放进 JSON（base64 ✓），
/// 而浏览器要的是**一个可下载的压缩包** ✓ —— 走这条路由就带上
/// `Content-Type: application/zip` 与 `Content-Disposition: attachment` ✓，点一下即下载 ✓。
/// **采集实现只有一份** ✓：与 MCP 的 `collect_diagnostics` 工具**同一个**
/// [`yanshi_server::diagnostics::collect`] ✓ ⇒ 包内条目不会漂移 ✓
///（`scripts/tool-collect-diagnostics.mjs` 断言两面的条目名逐字相同 ✓）。
///
/// **谁能取** ✓：能通过该文档鉴权的**任何角色** ✓ —— 包括 `viewer` ✓。
/// "出事了却只有 owner 能取证"等于没有取证能力 ✓。
///
/// **它不改文档** ✓：采集只读 ✓（`GET` 也表达了这一点 ✓）。
fn diagnostics_route(state: &ServerState, request: &Request) -> Response {
    let doc_id = match doc_param(request) {
        Ok(doc_id) => doc_id,
        Err(response) => return response,
    };
    let principal = match authorize(state, request, &doc_id) {
        Ok(principal) => principal,
        Err(response) => return response,
    };
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    // **与工具面同一条规矩：文档没打开就按需创建** ✓ ——
    // 否则"新文档"在 Web 面会 404、在 MCP 面却能出包 ✓ ⇒ 两面的条目对不上 ✗。
    if workspace.document(&doc_id).is_none() {
        let spec = NewDocument::new(doc_id.clone(), state.options.width, state.options.height);
        if let Err(error) = workspace.open_or_create(spec, "local:http", "session:http") {
            return Response::from_error(&error);
        }
    }
    let facts = web_diagnostics_facts(state, request, &principal, workspace.len());
    let bundle = match yanshi_server::diagnostics::collect(
        &workspace,
        &yanshi_server::DiagnosticsRequest {
            doc_id: doc_id.clone(),
            facts,
            limits: yanshi_server::Limits::default(),
        },
    ) {
        Ok(bundle) => bundle,
        Err(error) => return Response::from_error(&error),
    };
    let zip = match bundle.zip() {
        Ok(zip) => zip,
        Err(error) => return Response::from_error(&error),
    };
    Response::bytes(200, "application/zip", zip)
        .with_header(
            "Content-Disposition",
            format!("attachment; filename=\"yanshi-diagnostics-{doc_id}.zip\""),
        )
        .with_header("Cache-Control", "no-store")
        // **把条目名也放在头里** ✓：不看包体也能一眼对账两个面的条目是否一致 ✓。
        .with_header(
            "X-Yanshi-Diagnostics-Entries",
            bundle.entry_names().join(","),
        )
}

/// 采集诊断时要抹掉的机密值（当前令牌 / API key / 环境里的模型密钥 ✓）。
fn diagnostics_secrets(request: &Request) -> Vec<String> {
    let mut secrets = Vec::new();
    for value in [request.token(), request.param("key").map(str::to_owned)]
        .into_iter()
        .flatten()
    {
        if value.len() >= 4 {
            secrets.push(value);
        }
    }
    for key in [
        "YANSHI_API_KEY",
        "YANSHI_TOKEN",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
    ] {
        if let Ok(value) = std::env::var(key) {
            if value.len() >= 4 {
                secrets.push(value);
            }
        }
    }
    secrets
}

/// **Web 面的诊断事实** ✓ —— 独立路由与 `/api/tools/collect_diagnostics` **共用这一份** ✓。
///
/// **为什么要抽出来** ✗：同一条事实若在两条路上各写一遍 ✓，两边必然慢慢分叉 ✓
///（本项目的老毛病 ✓）⇒ 一处定义、两处使用 ✓。
fn web_diagnostics_facts(
    state: &ServerState,
    request: &Request,
    principal: &yanshi_server::Principal,
    open_documents: usize,
) -> yanshi_server::SurfaceFacts {
    yanshi_server::SurfaceFacts {
        surface: "web".to_owned(),
        build: json!({
            "name": "yanshi-serve",
            "version": env!("CARGO_PKG_VERSION"),
            "commit": option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
            "built": option_env!("YANSHI_BUILD_TIME").unwrap_or("unknown"),
            "transport": "http",
        }),
        config: json!({
            "bind": state.options.bind,
            "root": state.options.root.as_ref().map(|root| root.display().to_string()),
            "doc_id": state.options.doc_id,
            "width": state.options.width,
            "height": state.options.height,
            "profiles": state.options.profiles.iter().map(|profile| profile.as_str()).collect::<Vec<_>>(),
            "max_connections": state.options.max_connections,
            "push_interval_ms": state.options.push_interval_ms,
            "rewrite_blob_urls": state.options.rewrite_blob_urls,
            "wasm_dir": state.options.wasm_dir.as_ref().map(|dir| dir.display().to_string()),
            "brand_dir": state.options.brand_dir.as_ref().map(|dir| dir.display().to_string()),
            "medium_dir": state.options.medium_dir.as_ref().map(|dir| dir.display().to_string()),
            "assets_dir": state.options.assets_dir.as_ref().map(|dir| dir.display().to_string()),
            "log_ring_capacity": yanshi_server::diagnostics::LOG_RING_CAPACITY,
        }),
        extra: json!({
            "requests": state.requests.load(Ordering::Relaxed),
            "connections": state.connections.load(Ordering::Relaxed),
            "role": principal.role.as_str(),
            "actor": principal.actor.clone(),
            "open_documents": open_documents,
            "uptime_ms": yanshi_core::now_ms().saturating_sub(state.started_at),
        }),
        secrets: diagnostics_secrets(request),
    }
}

/// **`POST /api/documents/<id>/close`：把文档从内存里放下** ✓（磁盘上的内容**一点不动** ✓）。
///
/// **关闭 ≠ 删除** ✓，所以现在**各有各的路** ✓：
/// 关闭 ⇒ 这里 ✓（`closed` ✓，磁盘上还在 ✓，下次打开就回来 ✓）；
/// 删除 ⇒ `DELETE /api/documents/<id>?confirm=<id>` ✓（不可逆 ✓）。
/// 以前 `DELETE` 走的**就是关闭** ✗ —— 子 agent 把这当成 bug 报上来 ✓
///（"DELETE 返回 closed:true，但文档还在列表里" ✓）：那不是它的错 ✓，
/// 是**方法名与语义对不上** ✓ ⇒ 本次把两者分开 ✓，并让关闭这条**只**回答关闭这件事 ✓。
fn close_document(state: &ServerState, request: &Request, doc_id: &str) -> Response {
    if let Err(response) = authorize(state, request, doc_id) {
        return response;
    }
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    let closed = workspace.close_document(doc_id);
    // 让**响应说实话** ✓：明确给出 `persisted` ✓（它依旧出现在 `GET /api/documents` 里 ✓，
    // 那是**设计内的持久化行为** ✓ —— 重新打开展示作品正是需求 ✓）。
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
            "deleted": false,
            "note": if persisted { "已关闭内存中的文档；磁盘上的工作区仍然保留（要真删请用 DELETE /api/documents/<id>?confirm=<id>）" } else { "已关闭" },
        }),
    )
}

/// **`DELETE /api/documents/<id>?confirm=<id>`：真正把文档从磁盘上删掉** ✓。
///
/// **为什么带 `confirm`** ✓：删除**不可逆** ✗ ⇒ 要求调用方把文档 id **再写一遍** ✓
/// （"手滑点到删除"与"确定要删这一份"必须能区分开 ✓）。缺了或对不上 ⇒ 400 并说清 ✓。
///
/// **为什么走写权限策略而不是文档令牌** ✓：查看器要能删**别的**作品 ✓，
/// 而令牌是**按文档签发**的 ✗ ⇒ 它手里根本没有那些文档的令牌 ✓。
/// 于是用与"新建文档"**同一条**策略 ✓（`require_write_access` ✓：绑回环放行 ✓，
/// 绑对外必须带 `YANSHI_API_KEY` ✓）—— 删一份落盘文档与新建一份落盘文档是同一级动作 ✓。
///
/// **实现只有一份** ✓：这里只做**传输与守卫** ✓，真正动手的是 `Workspace::delete_document` ✓
/// —— MCP 的 `delete_document` 工具调的是**同一个函数** ✓ ⇒ 两个面不可能漂移 ✓。
fn delete_document(state: &ServerState, request: &Request, doc_id: &str) -> Response {
    if let Some(response) = require_write_access(state, request) {
        return response;
    }
    match request.param("confirm") {
        Some(confirm) if confirm == doc_id => {}
        Some(confirm) => {
            // **先建字符串再返回** ✓：直接写在 `return` 里会让 rustfmt 在这两种写法之间**来回摆** ✗
            //（`cargo fmt --check` 因此永远不绿 ✓ —— 那不是风格问题，是它真的不收敛 ✓）。
            let detail = format!(
                "confirm={confirm} 与要删的文档 {doc_id} 不一致 ⇒ 删除不可逆，请把文档 id 原样再写一遍"
            );
            return crate::http::bad_request(detail);
        }
        None => {
            let detail =
                format!("删除不可逆 ⇒ 必须带 ?confirm={doc_id}（把要删的文档 id 原样写一遍）");
            return crate::http::bad_request(detail);
        }
    }
    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    match workspace.delete_document(doc_id) {
        Ok(value) => Response::json(200, &value),
        Err(error) => Response::from_error(&error),
    }
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

    // **控制面：三件事必须在取工作区锁之前办** ✓（外部测试报告 P1）。
    //
    // 报告的事故链是：客户端 300 s 超时 ⇒ 服务端仍在跑、**文档仍被独占** ⇒ 用户分不清
    // "慢"与"挂死" ⇒ 重试 ⇒ **重复落笔**（真实事故：seq 477/478 的孤儿原子 ✗）。
    // 若"忙不忙"也要先拿工作区锁才能问 ✓，第二个请求就只会**排队等** ✗ —— 那正是原现象 ✓。
    // ⇒ `busy` 拒绝 / `cancel_operation` / `get_inflight` 都在这里回答 ✓（用的是同一张登记表 ✓）。
    if let Some((status, value)) = control_plane(state, principal.role, &doc_id, name, &arguments) {
        return Response::json(status, &value);
    }

    let Ok(mut workspace) = state.workspace.lock() else {
        return internal("工作区锁中毒");
    };
    // 文档尚未打开时按需创建（与 MCP 一致）。
    // **自己建文档的工具不替它预建** ✓（与 MCP 共用同一条规矩 ✓，判据见
    // `ToolSpec::creates_own_document` ✓）：HTTP 这边通常已经过 `authorize` ⇒ 目标多在内存里 ✓、
    // 这一句多半不生效 ✓，但**两个入口必须同一套规矩** ✓ —— 各写一套必然漂移 ✗。
    let pre_create = !state
        .registry
        .get(name)
        .is_some_and(|spec| spec.creates_own_document());
    if pre_create && workspace.document(&doc_id).is_none() {
        let spec = NewDocument::new(doc_id.clone(), state.options.width, state.options.height);
        if let Err(error) = workspace.open_or_create(spec, &principal.actor, "session:http") {
            return Response::from_error(&error);
        }
    }
    let owner = principal.role.can_revert_others();
    // **诊断事实在这里也挂上** ✓：`POST /api/tools/collect_diagnostics` 与
    // `GET /api/diagnostics` **必须给出同一份包** ✓ ⇒ 两条路共用 `web_diagnostics_facts` ✓。
    let diagnostics_facts = web_diagnostics_facts(state, request, &principal, workspace.len());
    let mut context = ToolContext::new(
        &mut workspace,
        doc_id.clone(),
        principal.actor.clone(),
        "session:http",
    )
    .with_owner(owner)
    // **真实角色必须带进上下文** ✓：工具层的强制检查读的就是它 ✓（见 `ToolRegistry::call` ✓）。
    .with_role(principal.role)
    .with_wait_for_render(true, 500)
    .with_diagnostics_facts(diagnostics_facts);
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
            // 与 `http.rs::from_error` 保持同一套映射 ✓（busy/cancelled ⇒ 409 ✓）。
            Some("busy") | Some("cancelled") => 409,
            Some("resource_exhausted") | Some("degraded") => 503,
            Some("job_pending") => 202,
            _ => 400,
        };
        return Response::json(status, &value);
    }
    Response::json(200, &value)
}

/// **控制面：在取工作区锁之前就能回答的三件事** ✓（外部测试报告 P1）。
///
/// | 工具 | 为什么不能等锁 |
/// |---|---|
/// | 任意**变更**工具（`busy` 拒绝 ✓） | 长操作持锁 ⇒ 先取锁就变成"排队等它跑完" ✗（原现象 ✓） |
/// | `cancel_operation` | 等锁 ⇒ 要等到被取消的那个操作**自己结束**才生效 ✗（等于没有取消 ✓） |
/// | `get_inflight` | 等锁 ⇒ 恰恰在"卡住"时查不到东西 ✗ |
///
/// 三件事都只读/写 `state.inflight` ✓（**自己的锁** ✓，与工作区无关 ✓）。
/// 工具层仍有同样的实现 ✓（MCP／嵌入式走那边 ✓）—— 这里只是为了**不排队** ✓。
///
/// **返回 `(HTTP 状态, JSON 体)`** ✓：HTTP 用它组响应 ✓，WebSocket 直接把 JSON 体塞进 `ack` ✓
/// （两个入口共用这一份判定 ✓ —— 免得又出现"只堵住了一个入口"✗ 的老问题 ✓）。
fn control_plane(
    state: &ServerState,
    role: Role,
    doc_id: &str,
    name: &str,
    arguments: &Value,
) -> Option<(u16, Value)> {
    let wanted = arguments
        .get("doc_id")
        .and_then(Value::as_str)
        .unwrap_or(doc_id);
    match name {
        "get_inflight" => {
            let inflight = if wanted == "*" {
                state.inflight.list()
            } else {
                state.inflight.status(wanted).into_iter().collect()
            };
            Some((
                200,
                json!({
                    "ok": true,
                    "doc_id": wanted,
                    "inflight": inflight,
                    "count": inflight.len(),
                    "begun": state.inflight.begun(),
                    "rejected": state.inflight.rejected(),
                    "cancel_requests": state.inflight.cancel_requests(),
                }),
            ))
        }
        "cancel_operation" => {
            if !role.can_edit() {
                return Some((
                    403,
                    YanshiError::new(
                        ErrorCode::PermissionDenied,
                        ErrorContext::detail(format!(
                            "该 token 的角色是 {} ⇒ 不允许取消别人的操作（读类查询不受限）",
                            role.as_str()
                        )),
                    )
                    .to_response(),
                ));
            }
            let info = state.inflight.request_cancel(wanted);
            Some((
                200,
                json!({
                    "ok": true,
                    "doc_id": wanted,
                    // **协作式**：`cancelled: true` 只表示"请求已受理" ✓（不是"已经停了" ✗）。
                    "cancelled": info.is_some(),
                    "inflight": info,
                }),
            ))
        }
        _ => {
            // 只读工具不受影响 ✓（读文档本来就要锁 ✓，而且它们本来就快 ✓）。
            if state
                .registry
                .get(name)
                .map(|spec| spec.mutating)
                .unwrap_or(false)
            {
                if let Some(busy) = state.inflight.busy_error(doc_id, yanshi_core::now_ms()) {
                    return Some((409, busy.to_response()));
                }
            }
            None
        }
    }
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
    // **★ 磁盘名兼容 ✗ ★**（第 803 轮 ✓；**用户部署驱动 ✓**）：
    //   **∴ 背景 ✗**：**Cloudflare 的 assets manifest **要求路径是 URI 编码形式 ✗**
    //   （**错误码 10304 ✓）⇒ **∴ 于是**同步脚本把**带 `#`／`%` 的笔刷**落盘成
    //   **编码名（**`8B_Pencil%231.myb` ✓）⇒ **∴ 而**本函数**先做了**百分号解码 ✗**
    //     ⇒ **∴ 于是**它**去找解码名（**`8B_Pencil#1.myb` ✓）⇒ **∴ 磁盘上没有 ⇒ **∴ 404 ✓****
    //   ⇒ **∴ 现在**：**解码名不存在时，**按**同一规则**再编码一次**重试 ✗**
    //     ⇒ **∴ 于是**：**本地（**磁盘编码名 ✓）与 CDN **同时可用 ✓**
    //       ＋ **∴ 若**将来**磁盘还原成原名 ✗**，**第一条路径**又会命中 ✓**** ✓✓
    let mut path = root.join("brushes").join(file);
    if !path.exists() {
        let encoded = file.replace('%', "%25").replace('#', "%23");
        let alt = root.join("brushes").join(&encoded);
        if alt.exists() {
            path = alt;
        }
    }

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

/// **Service Worker 脚本** ✓（离线优先 PWA 的第一步 ／ (A)① 的**第一个可打包静态产物** ✓）。
///
/// 它从 `viewer.rs` 的字符串里**拆成独立文件** ✓，理由有两条 ✓：
///   ① **SW 必须是独立可寻址资源** ✓ —— 内嵌在页面里的脚本**无法注册为 SW** ✗；
///   ② **离线预缓存需要一份可枚举、可打包的清单** ✓ —— 独立文件才能被 `package-release.sh` 复制、
///      被校验、也被 SW 自己 `cache.add` ✓。
///
/// 内容里的 `__BUILD_ID__` 是**构建标识占位** ✓，由用法处替换 ✓（**两侧必须一致 ✗**）。
const SERVICE_WORKER_JS: &str = include_str!("../assets/service-worker.js");

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
                // **控制面与 busy 检查先于工作区锁** ✓（P1）：与 HTTP 入口**同一份判定** ✓ ——
                // WebSocket 曾经绕过了角色检查 ✗（见 `ToolRegistry::call` 的说明 ✓），
                // 所以这里不再各写一份 ✓。
                match control_plane(state, role, doc_id, name, &arguments) {
                    Some((_, value)) => value,
                    None => {
                        let Ok(mut workspace) = state.workspace.lock() else {
                            let _ = send_json(
                                write_stream,
                                &json!({"type": "error", "request_id": request_id,
                                        "error": {"code": "resource_exhausted", "message": "工作区锁中毒"}}),
                            );
                            return;
                        };
                        let mut context =
                            ToolContext::new(&mut workspace, doc_id, actor, "session:ws")
                                // **别再无条件 owner** ✗（**这就是那个漏洞** ✓）：
                                // `owner` 的语义是"**可跨 actor 撤销**" ✓ ⇒ 它只应对 `Owner` 为真 ✓；
                                // 而"**能不能改文档**"由工具层按 `role.can_edit()` 判 ✓ ⇒ 两者**各司其职** ✓。
                                .with_owner(matches!(role, Role::Owner))
                                .with_role(role)
                                .with_wait_for_render(true, 500);
                        state.registry.call(&mut context, name, &arguments)
                    }
                }
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
        (
            "GET /api/diagnostics?doc=",
            "诊断包（zip：构建/配置/文档元数据/原子尾部/stderr 环形缓冲/渲染告警/耗时/缩略图，需 token）",
        ),
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

    /// **页面与 SW 的缓存名必须逐字一致** ✓ —— 这是 (A)⑥「SW 升级不脏读」的核心不变量 ✓。
    ///
    /// 页面按缓存名开缓存、SW 按缓存名安装与清理 ✗ ⇒ **名字一分岔 ⇒ 就会读到旧外壳** ✗
    /// （第 210 轮查到的真因：**旧 js 配新 wasm ⇒ 内核预览失败** ✓）。
    ///
    /// **变异判据** ✓：把**任一侧**的 `yanshi-shell-` 名字改掉（或去掉 `__BUILD_ID__` 替换）⇒ 红 ✓。
    #[test]
    fn the_page_and_the_service_worker_agree_on_the_shell_cache_name() {
        let sw = SERVICE_WORKER_JS.replace("__BUILD_ID__", BUILD_ID_TEXT);
        let page = crate::viewer::page_with_read_tools();
        let name_in = |text: &str| -> String {
            let prefix = "yanshi-shell-";
            let start = text
                .find(prefix)
                .unwrap_or_else(|| panic!("应含外壳缓存名前缀 {prefix} ✓"))
                + prefix.len();
            text[start..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
                .collect()
        };
        assert_eq!(
            name_in(&sw),
            name_in(&page),
            "页面与 SW 的缓存名必须一致 ✓（否则 SW 升级后会读到旧外壳 ⇒ 脏读 ✗）"
        );
        assert!(
            name_in(&sw).contains(BUILD_ID_TEXT),
            "缓存名里必须带构建标识 ✓ ⇒ 新构建自动作废旧外壳 ✓"
        );
    }

    /// **(A)① 的第一个可打包静态产物** ✓：Service Worker 必须是**独立文件** ✓、
    /// 保留构建标识占位 ✓、且外壳清单里含**共享内核两件** ✓。
    ///
    /// **变异判据** ✓：① 把资产文件删掉 ⇒ 红 ✓；② 去掉 `__BUILD_ID__` 占位 ⇒ 红 ✓；
    /// ③ 从外壳清单里删掉任一内核条目 ⇒ 红 ✓（**内核缺席会导致离线时内核拿不到** ✗ —— 第 213 轮的教训 ✓）。
    #[test]
    fn the_service_worker_is_a_separate_packable_asset() {
        let path = std::path::Path::new("assets/service-worker.js");
        assert!(
            path.is_file(),
            "(A)①：Service Worker 应是**独立资产文件**（可打包/可预缓存/可校验）✓"
        );
        let on_disk = std::fs::read_to_string(path).expect("应能读取资产文件");
        assert_eq!(
            on_disk, SERVICE_WORKER_JS,
            "二进制内嵌的内容必须与磁盘资产**逐字节一致** ✓"
        );
        assert!(
            on_disk.contains("__BUILD_ID__"),
            "SW 必须保留 __BUILD_ID__ 占位 ✓（它进缓存名 ⇒ 新构建自动作废旧外壳 ✓，即 (A)⑥ ✓）"
        );
        for url in [
            "/wasm/yanshi_wasm.js",
            "/wasm/yanshi_wasm_bg.wasm",
            "/brush-previews/index.json",
        ] {
            assert!(on_disk.contains(url), "SW 外壳清单应含 {url} ✓");
        }
    }

    use super::*;

    fn state() -> ServerState {
        let settings = DocumentSettings::default();
        let workspace = Workspace::in_memory(settings);
        let inflight = Arc::clone(workspace.inflight());
        ServerState {
            workspace: Mutex::new(workspace),
            inflight,
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
