//! # 偃师 Yanshi MCP stdio 服务器
//!
//! 把 [`yanshi_server`] 的工具协议层暴露给 Agent（设计文档 3 章协议、10 章工具、12.7 鉴权）。
//!
//! - **传输**：MCP stdio —— 每行一个 JSON-RPC 2.0 消息（本地进程，豁免鉴权）。
//! - **方法**：`initialize`、`tools/list`、`tools/call`、`ping`；
//!   `notifications/initialized` 等通知不回复。
//! - **通道分工**（6.7）：stdio 统一走「提交 + 轮询」，不做推送；Agent 用
//!   `get_job` / `get_render_status` / `get_log` 轮询。
//! - **profile**（10.2）：`--profile core,annotation,semantic` 控制注册的工具集。
//!
//! ```
//! use std::io::Cursor;
//! use yanshi_mcp::{serve, McpOptions};
//!
//! let mut input = Vec::new();
//! input.extend_from_slice(
//!     br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#
//! );
//! input.push(b'\n');
//! input.extend_from_slice(br#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#);
//! input.push(b'\n');
//!
//! let mut output = Vec::new();
//! serve(Cursor::new(input), &mut output, McpOptions::default()).unwrap();
//! let text = String::from_utf8(output).unwrap();
//! assert!(text.contains("\"protocolVersion\""));
//! assert!(text.contains("\"tools\""));
//! ```

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use yanshi_server::DocumentSettings;
use yanshi_server::Workspace;
use yanshi_server::{Profile, ToolContext, ToolRegistry};

/// MCP 协议版本（与 MCP 规范 2024-11-05 对齐）。
pub const PROTOCOL_VERSION: &str = "2024-11-05";
/// 服务器名。
pub const SERVER_NAME: &str = "yanshi-mcp";
/// 服务器版本。
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// JSON-RPC 错误码。
pub mod error_code {
    /// 解析错误。
    pub const PARSE_ERROR: i64 = -32700;
    /// 请求非法。
    pub const INVALID_REQUEST: i64 = -32600;
    /// 方法不存在。
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// 参数非法。
    pub const INVALID_PARAMS: i64 = -32602;
    /// 内部错误。
    pub const INTERNAL_ERROR: i64 = -32603;
}

/// 服务器选项。
#[derive(Debug, Clone)]
pub struct McpOptions {
    /// 持久化根目录（`None` 表示纯内存）。
    pub root: Option<std::path::PathBuf>,
    /// 默认文档 id（工具参数里的 `doc_id` 优先）。
    pub doc_id: String,
    /// 自动创建文档时的画布尺寸。
    pub width: u32,
    /// 高度。
    pub height: u32,
    /// 启用的 profile。
    pub profiles: Vec<Profile>,
    /// 是否等待渲染（`wait_for_render`，6.7）。
    ///
    /// **等的是"这一笔的脏区"那一次渲染** ✓（性能专题本轮澄清 ✓）——
    /// 它保证返回时该原子 `rendered == true` 且 `job_status == "committed"` ✓。
    /// **不等那张 256² 文档级缩略图** ✗：它是缓存，按需产生 ✓
    ///（`get_document` 会发现它落后并当场重建 ✓；在那之前
    /// `get_render_status.thumbnail_current` 为 `false` ✓ —— 调用方看得见 ✓）。
    /// 8K 实测：默认（同步）单笔 39.5 s vs `--no-wait` 95.1 ms，**逐像素一致** ✓。
    pub wait_for_render: bool,
    /// 等待预算（毫秒）。
    pub wait_budget_ms: u64,
    /// 是否在 `tools/call` 结果里内嵌图像（≤512px）。
    pub inline_images: bool,
}

impl Default for McpOptions {
    fn default() -> Self {
        Self {
            root: None,
            doc_id: "default".to_owned(),
            width: 1024,
            height: 1024,
            profiles: vec![Profile::Core],
            wait_for_render: true,
            wait_budget_ms: 500,
            inline_images: true,
        }
    }
}

impl McpOptions {
    /// 解析命令行参数（`--help` / `--version` 由调用方先行处理）。
    pub fn parse_args<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut options = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            let arg = arg.as_ref();
            let mut take_value = |name: &str| -> Result<String, String> {
                iter.next()
                    .map(|value| value.as_ref().to_owned())
                    .ok_or_else(|| format!("{name} 需要一个取值"))
            };
            match arg {
                "--root" => options.root = Some(take_value("--root")?.into()),
                "--doc" => options.doc_id = take_value("--doc")?,
                "--width" => {
                    options.width = take_value("--width")?
                        .parse()
                        .map_err(|_| "--width 必须是整数".to_owned())?
                }
                "--height" => {
                    options.height = take_value("--height")?
                        .parse()
                        .map_err(|_| "--height 必须是整数".to_owned())?
                }
                "--profile" | "--profiles" => {
                    let list = take_value("--profile")?;
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
                "--no-wait" => options.wait_for_render = false,
                "--wait-budget" => {
                    options.wait_budget_ms = take_value("--wait-budget")?
                        .parse()
                        .map_err(|_| "--wait-budget 必须是整数".to_owned())?
                }
                "--no-inline-images" => options.inline_images = false,
                // `--list-tools` 由 main 处理；这里接受以免被当成未知参数。
                "--list-tools" => {}
                other => return Err(format!("未知参数 {other}")),
            }
        }
        Ok(options)
    }

    /// 帮助文本。
    pub const fn help() -> &'static str {
        "yanshi-mcp —— 偃师 Yanshi MCP stdio 服务器\n\
         \n\
         用法：yanshi-mcp [选项]\n\
         \n\
         选项：\n\
           --root <dir>          持久化根目录（缺省纯内存）\n\
           --doc <id>            默认文档 id（缺省 default）\n\
           --width <n>           自动创建文档的宽（缺省 1024）\n\
           --height <n>          自动创建文档的高（缺省 1024）\n\
           --profile <list>      启用工具组，逗号分隔，可选：all,core,history,changeset,retouch,semantic,conflict,annotation,collab,structure\n\
                                 （all = 全部已实现的组，即除 semantic 外；缺省只开 core）\n\
           --no-wait             不等待渲染（立即返回 job_pending，由 Agent 轮询）\n\
           --wait-budget <ms>    wait_for_render 的等待预算（缺省 500）\n\
                                 **注意**：默认的等待只覆盖「这一笔的渲染」（rendered/job_status）\n\
                                 —— 它**不**等那张 256² 文档缩略图（缓存，按需产生；\n\
                                 要新鲜就调 get_document，它会当场重建；\n\
                                 get_render_status.thumbnail_current 会告诉你它是否落后）\n\
           --no-inline-images    关掉内嵌图像；**缺省是内嵌**（图片在 MCP 的 image 内容块里）\n\
           --list-tools          打印工具清单后退出\n\
           --help                显示帮助\n\
           --version             显示版本\n"
    }
}

/// MCP 服务器状态。
pub struct Server {
    options: McpOptions,
    registry: ToolRegistry,
    workspace: Workspace,
    initialized: bool,
    requests: u64,
}

impl Server {
    /// 以选项构造（文件存储失败时回退内存并给出警告）。
    pub fn new(options: McpOptions) -> Self {
        let settings = DocumentSettings::default();
        let workspace = match &options.root {
            Some(root) => Workspace::with_file_store(root.clone(), settings.clone())
                .unwrap_or_else(|_| Workspace::in_memory(settings)),
            None => Workspace::in_memory(settings),
        }
        // **与 HTTP 用同一个默认** ✓：内置纹理随仓库与发行包走 ✓。
        // **这条正是"MCP 与 Web 都要能用"的落法** ✓ —— 两边都经**工具层** ✓ ⇒ 结果一致 ✓。
        // **与 HTTP 用同一个解析器** ✓（真实用户报告 ✓：相对路径 `assets` 换个工作目录就找不到 ✓）。
        .with_assets_dir({
            let (resolved, note) = yanshi_server::service::resolve_assets_dir(Some(
                std::path::PathBuf::from("assets"),
            ));
            yanshi_server::diagnostics::log_line(&note);
            resolved
        });
        // **启动即记一行** ✓：MCP stdio 模式的 stderr 会被客户端丢掉 ✗ ⇒
        // 这一行是"这个进程是什么时候、以什么身份起来的"的**唯一**证据 ✓，
        // 会随下一次 `collect_diagnostics` 一起交出去 ✓。
        yanshi_server::diagnostics::log_line(format!(
            "{SERVER_NAME} {SERVER_VERSION} 启动（commit {}，root={}，doc={}，profiles={:?}，wait_for_render={}）",
            option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
            options
                .root
                .as_ref()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "<内存>".to_owned()),
            options.doc_id,
            options.profiles.iter().map(|profile| profile.as_str()).collect::<Vec<_>>(),
            options.wait_for_render,
        ));
        let registry = ToolRegistry::with_profiles(&options.profiles);
        Self {
            options,
            registry,
            workspace,
            initialized: false,
            requests: 0,
        }
    }

    /// 注册表。
    pub const fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// 工作区。
    pub const fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    /// 工作区（可变）。
    pub fn workspace_mut(&mut self) -> &mut Workspace {
        &mut self.workspace
    }

    /// 已处理请求数。
    pub const fn requests(&self) -> u64 {
        self.requests
    }

    /// 工具清单（`tools/list`）。
    ///
    /// **与 HTTP 的 `GET /api/tools` 是同一份** ✓ —— 两边都调注册表那一个函数 ✗
    ///（各写一份必然漂移 ✓；外部 agent 实测就是靠 MCP 这份才发现 HTTP 侧缺清单 ✓）。
    pub fn tools_list(&self) -> Value {
        self.registry.tools_list_json()
    }

    /// 处理一条 JSON-RPC 消息；通知返回 `None`。
    pub fn handle_message(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));

        // 通知（无 id）不回复。
        if id.is_none() || method.starts_with("notifications/") {
            return None;
        }
        let id = id.unwrap_or(Value::Null);
        self.requests += 1;

        let response = match method {
            "initialize" => {
                self.initialized = true;
                Ok(json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                    "instructions": "偃师 Yanshi 无头绘画引擎。用 get_document/get_state 看状态，\
                                     用 draw_*/fill/erase 画，用 render_region 看预览，\
                                     重型操作用 get_job 轮询。"
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(self.tools_list()),
            "tools/call" => self.tools_call(&params),
            other => Err((error_code::METHOD_NOT_FOUND, format!("未知方法 {other}"))),
        };

        Some(match response {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err((code, message)) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": code, "message": message},
            }),
        })
    }

    fn tools_call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let name = params.get("name").and_then(Value::as_str).ok_or((
            error_code::INVALID_PARAMS,
            "tools/call 缺少 name".to_owned(),
        ))?;
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !arguments.is_object() {
            return Err((
                error_code::INVALID_PARAMS,
                "tools/call 的 arguments 必须是对象".to_owned(),
            ));
        }
        if self.registry.get(name).is_none() {
            return Err((
                error_code::INVALID_PARAMS,
                format!(
                    "未知工具 {name}（当前 profile：{}）",
                    self.registry
                        .profiles()
                        .iter()
                        .map(|profile| profile.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ));
        }

        // 文档：参数 doc_id 优先，其次 CLI 默认值；不存在则按需创建。
        let doc_id = arguments
            .get("doc_id")
            .and_then(Value::as_str)
            .unwrap_or(&self.options.doc_id)
            .to_owned();
        let spec = yanshi_server::NewDocument::new(
            doc_id.clone(),
            self.options.width,
            self.options.height,
        );
        if let Err(error) =
            self.workspace
                .open_or_create(spec, "agent:mcp", &format!("session:{SERVER_NAME}"))
        {
            return Ok(tool_error(&error.to_response()));
        }

        // **让"显式点名的文档"成为会话活跃文档** ✓（用户报告 2.2 ✓ 会话级上下文锁定 ✗）：
        // 老行为是"每次都回落到 CLI 的 `--doc`"✗ ⇒ `new_document(second)` 之后 **仍画在 first** ✗
        // （判据 `scripts/mcp-document-switch.mjs` 用**磁盘**复现过 ✓：图层落在 `first/atoms.jsonl` ✗）
        // ⇒ 客户端画第二幅只能**重启进程** ✗，与"MCP 是常驻服务"的设计初衷相悖 ✓。
        // 现在：只要这次调用**显式**给了 `doc_id` ✓（`new_document` ✓、或任何带该参数的工具 ✓），
        // 它就**粘住**成为后续调用的缺省 ✓ —— 这同时给了报告建议③一个可用的"切换"手段 ✓。
        if arguments.get("doc_id").and_then(Value::as_str).is_some() {
            self.options.doc_id = doc_id.clone();
        }

        // **事实必须在借出 `&mut self.workspace` 之前收齐** ✓（否则 `self.workspace.len()`
        // 会与那次可变借用冲突 ✓ —— 编译器替我们挡住了一次"边借边读" ✓）。
        let open_documents = self.workspace.len();
        let facts = yanshi_server::SurfaceFacts {
            surface: "mcp".to_owned(),
            build: json!({
                "name": SERVER_NAME,
                "version": SERVER_VERSION,
                "commit": option_env!("YANSHI_COMMIT").unwrap_or("unknown"),
                "built": option_env!("YANSHI_BUILD_TIME").unwrap_or("unknown"),
                "transport": "stdio",
            }),
            config: json!({
                "transport": "stdio (JSON-RPC，每行一条)",
                "root": self.options.root.as_ref().map(|root| root.display().to_string()),
                "doc_id": self.options.doc_id,
                "width": self.options.width,
                "height": self.options.height,
                "profiles": self.options.profiles.iter().map(|profile| profile.as_str()).collect::<Vec<_>>(),
                "wait_for_render": self.options.wait_for_render,
                "wait_budget_ms": self.options.wait_budget_ms,
                "inline_images": self.options.inline_images,
                "log_ring_capacity": yanshi_server::diagnostics::LOG_RING_CAPACITY,
            }),
            extra: json!({
                "requests": self.requests,
                "open_documents": open_documents,
            }),
            secrets: diagnostics_secrets(),
        };
        let mut context = ToolContext::new(
            &mut self.workspace,
            doc_id,
            "agent:mcp",
            format!("session:{SERVER_NAME}"),
        )
        .with_owner(true)
        .with_wait_for_render(self.options.wait_for_render, self.options.wait_budget_ms)
        // **告诉工具层"这个面是什么"** ✓：`collect_diagnostics` 的 build/config/extra 全从这里来 ✓
        // ⇒ 包内条目与 HTTP 面**不会漂移** ✓（各写一份必然漂移 ✗）。
        .with_diagnostics_facts(facts);

        let result = self.registry.call(&mut context, name, &arguments);
        Ok(tool_result(result, self.options.inline_images))
    }

    /// 处理一行输入，返回要写出的行（通知返回 `None`）。
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(error) => {
                return Some(
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": {"code": error_code::PARSE_ERROR, "message": format!("JSON 解析失败: {error}")},
                    })
                    .to_string(),
                )
            }
        };
        self.handle_message(&message).map(|value| value.to_string())
    }
}

fn tool_error(response: &Value) -> Value {
    json!({
        "content": [{"type": "text", "text": response.to_string()}],
        "isError": true,
    })
}

/// 采集诊断时要抹掉的机密值（环境里的密钥都要洗掉 ✓）。
///
/// **为什么列一串环境变量名** ✓：诊断包会带上"生效配置" ✓，
/// 而这些值是**部署时**从环境注入的 ✓ ⇒ 采集时按同一批名字读出来当机密 ✓，
/// 去密器会把它们在**所有条目**里替换成占位串 ✓（见 `diagnostics::redact_text` ✓）。
fn diagnostics_secrets() -> Vec<String> {
    let mut secrets = Vec::new();
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

/// 把工具返回的 JSON 包装成 MCP `tools/call` 结果。
///
/// 7.5 的工具返回默认是 URL；当结果里带 `image`（小尺寸预览）时额外发一条
/// MCP `image` content，避免 Agent 无法 fetch 任意 URL。
pub fn tool_result(value: Value, inline_images: bool) -> Value {
    let is_error = value.get("ok").and_then(Value::as_bool) == Some(false);
    let mut content = Vec::new();
    let mut text_value = value.clone();
    if inline_images {
        let image = text_value.get("image").cloned().or_else(|| {
            text_value
                .get("preview")
                .and_then(|preview| preview.get("image"))
                .cloned()
        });
        if let Some(image) = image {
            let data = image
                .get("data")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let mime = image
                .get("mime_type")
                .and_then(Value::as_str)
                .unwrap_or("image/png");
            if !data.is_empty() {
                content.push(json!({"type": "image", "data": data, "mimeType": mime}));
            }
            if let Value::Object(map) = &mut text_value {
                map.remove("image");
            }
        }
    }
    // **诊断包要作为 MCP 的二进制内容块交出去** ✓（与 `image` 同一条规矩 ✓）：
    // `archive_base64` 是 zip 的全部字节 ✓ ⇒ 转成 `resource` 内容块 ✓（`blob` 就是 base64 ✓），
    // 并从 text 里**移走** ✓ —— 不然同一条消息里它要出现两遍 ✓（响应体积翻倍 ✗）。
    if let Some(archive) = text_value
        .get("archive_base64")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        if !archive.is_empty() {
            content.push(json!({
                "type": "resource",
                "resource": {
                    "uri": "yanshi://diagnostics/latest.zip",
                    "mimeType": "application/zip",
                    "blob": archive,
                },
            }));
            if let Value::Object(map) = &mut text_value {
                map.remove("archive_base64");
                map.insert(
                    "archive_transport".to_owned(),
                    json!("MCP resource 内容块（mimeType=application/zip，blob=base64 zip 的全部字节）"),
                );
            }
        }
    }
    content.insert(0, json!({"type": "text", "text": text_value.to_string()}));
    json!({"content": content, "isError": is_error})
}

/// 跑 stdio 循环：逐行读 JSON-RPC，逐行写响应。
pub fn serve<R: Read, W: Write>(
    reader: R,
    mut writer: W,
    options: McpOptions,
) -> std::io::Result<()> {
    let mut server = Server::new(options);
    let reader = BufReader::new(reader);
    for line in reader.lines() {
        let line = line?;
        if let Some(response) = server.handle_line(&line) {
            writeln!(writer, "{response}")?;
            writer.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::new(McpOptions {
            doc_id: "doc_test".to_owned(),
            width: 32,
            height: 32,
            ..McpOptions::default()
        })
    }

    fn call(server: &mut Server, id: u64, method: &str, params: Value) -> Value {
        let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        server.handle_message(&message).expect("请求应有响应")
    }

    fn tool(server: &mut Server, name: &str, arguments: Value) -> Value {
        let response = call(
            server,
            1,
            "tools/call",
            json!({"name": name, "arguments": arguments}),
        );
        assert_eq!(response["error"], Value::Null, "{response}");
        response["result"].clone()
    }

    fn text_of(result: &Value) -> Value {
        let text = result["content"][0]["text"].as_str().unwrap_or("{}");
        serde_json::from_str(text).expect("工具文本是 JSON")
    }

    #[test]
    fn initialize_and_tools_list_follow_mcp_shape() {
        let mut server = server();
        let response = call(&mut server, 1, "initialize", json!({}));
        assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(response["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(response["result"]["capabilities"]["tools"].is_object());

        let response = call(&mut server, 2, "tools/list", json!({}));
        let tools = response["result"]["tools"].as_array().unwrap();
        // **不要硬编码工具数量** ✗ —— 本轮新增 `replace_object_data` 时，
        // 这里写死的 27 直接让测试变红 ✓，而它真正想守的是"MCP 暴露的工具与注册表一致" ✓。
        // 因此改成两条**真正的**断言：与注册表逐一对应 ✓，以及一个宽松的下界（防止空列表假过 ✓）。
        let registry: Vec<String> = yanshi_server::ToolRegistry::core()
            .tools()
            .iter()
            .map(|tool| tool.name.to_owned())
            .collect();
        assert_eq!(
            tools.len(),
            registry.len(),
            "MCP 暴露的工具数应与核心注册表一致（注册表 {} 个）",
            registry.len()
        );
        assert!(
            tools.len() >= 20,
            "核心工具数明显偏少（{}），列表可能没生成出来",
            tools.len()
        );
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool["name"].as_str())
            .collect();
        for expected in [
            "get_document",
            "get_state",
            "render_region",
            "draw_stroke",
            "draw_shape",
            "fill",
            "erase",
            "update_stroke",
            "batch",
            "get_job",
            "get_render_status",
            "cancel_job",
        ] {
            assert!(names.contains(&expected), "缺少 {expected}: {names:?}");
        }
        // 输入 schema 是合法 JSON Schema。
        let schema = &tools[0]["inputSchema"];
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"].is_object());
    }

    #[test]
    fn notifications_and_pings_are_handled() {
        let mut server = server();
        assert!(server
            .handle_message(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .is_none());
        let response = call(&mut server, 3, "ping", json!({}));
        assert_eq!(response["result"], json!({}));
        let response = call(&mut server, 4, "no/such", json!({}));
        assert_eq!(response["error"]["code"], error_code::METHOD_NOT_FOUND);
        let response = call(&mut server, 5, "tools/call", json!({"name": "nope"}));
        assert_eq!(response["error"]["code"], error_code::INVALID_PARAMS);
    }

    #[test]
    fn end_to_end_draw_render_and_poll() {
        let mut server = server();
        // 文档按需创建。
        let document = text_of(&tool(&mut server, "get_document", json!({})));
        assert_eq!(document["width"], json!(32));
        assert_eq!(document["head_seq"], json!(1));

        let layer = text_of(&tool(
            &mut server,
            "create_layer",
            json!({"layer_id": "layer_1", "name": "ink"}),
        ));
        assert_eq!(layer["ok"], json!(true));
        assert_eq!(layer["seq"], json!(2));
        assert!(layer["preview"]["thumb_url"]
            .as_str()
            .unwrap()
            .starts_with("yanshi://blob/"));

        let stroke = text_of(&tool(
            &mut server,
            "draw_stroke",
            json!({
                "layer_id": "layer_1",
                "data": {"points": [[4.0, 4.0], [20.0, 16.0]], "size": 4.0, "color": [0.0, 0.0, 0.0, 1.0]},
            }),
        ));
        assert_eq!(stroke["dirty_kind"], json!("geometry"));

        // 显式区域渲染（含内嵌图像）。
        let render = tool(
            &mut server,
            "render_region",
            json!({"region": {"x": 0, "y": 0, "w": 8, "h": 8}, "include_image": true}),
        );
        let render_json = text_of(&render);
        assert_eq!(render_json["width"], json!(8));
        assert!(render_json.get("image").is_none(), "内嵌图像从文本里移除");
        assert_eq!(render["content"][1]["type"], json!("image"));
        assert_eq!(render["content"][1]["mimeType"], json!("image/png"));

        // 渲染状态与日志轮询（stdio 不做推送，6.7）。
        let status = text_of(&tool(
            &mut server,
            "get_render_status",
            json!({"atom_id": stroke["atom_id"]}),
        ));
        assert_eq!(status["rendered"], json!(true));
        let log = text_of(&tool(&mut server, "get_log", json!({"since_seq": 0})));
        assert_eq!(
            log["count"],
            json!(3),
            "create_document + create_layer + draw_stroke"
        );
    }

    #[test]
    fn unknown_objects_and_missing_params_return_5_7_errors() {
        let mut server = server();
        let result = tool(
            &mut server,
            "delete_object",
            json!({"object_id": "obj_missing"}),
        );
        let error = text_of(&result);
        assert_eq!(result["isError"], json!(true));
        assert_eq!(error["ok"], json!(false));
        assert_eq!(error["error_code"], json!("reference_not_found"));
        assert!(error["retryable"].is_boolean());

        let response = call(
            &mut server,
            9,
            "tools/call",
            json!({"name": "delete_object", "arguments": {}}),
        );
        let error = text_of(&response["result"]);
        assert_eq!(error["error_code"], json!("invalid_argument"));
    }

    #[test]
    fn profiles_gate_tool_visibility() {
        let mut server = Server::new(McpOptions {
            profiles: vec![Profile::Core, Profile::Annotation, Profile::History],
            ..McpOptions::default()
        });
        let response = call(&mut server, 1, "tools/list", json!({}));
        let tools = response["result"]["tools"].as_array().unwrap().len();
        assert!(tools > 27, "启用扩展组后工具应更多：{tools}");
        let names: Vec<String> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
            .collect();
        assert!(names.contains(&"create_annotation".to_owned()));
        assert!(names.contains(&"checkpoint".to_owned()));
        assert!(
            !names.contains(&"inpaint_region".to_owned()),
            "未实现的语义工具不注册"
        );

        // 标注流：创建 → 列表 → 解决。
        let created = text_of(&tool(
            &mut server,
            "create_annotation",
            json!({
                "type": "region",
                "intent": "modify",
                "target": {"target": "region", "bbox": {"x": 0, "y": 0, "w": 4, "h": 4}},
                "content": "这里太亮",
            }),
        ));
        let annotation_id = created["annotation_id"].as_str().unwrap().to_owned();
        let listed = text_of(&tool(
            &mut server,
            "list_annotations",
            json!({"status": "pending"}),
        ));
        assert_eq!(listed["count"], json!(1));
        let resolved = text_of(&tool(
            &mut server,
            "resolve_annotation",
            json!({"annotation_id": annotation_id}),
        ));
        assert_eq!(resolved["annotation"]["status"], json!("resolved"));
        assert_eq!(resolved["pending"], json!(0));
    }

    #[test]
    fn batch_shares_one_changeset() {
        let mut server = server();
        let batch = text_of(&tool(
            &mut server,
            "batch",
            json!({
                "calls": [
                    {"tool": "create_layer", "arguments": {"layer_id": "layer_1"}},
                    {"tool": "draw_stroke", "arguments": {"layer_id": "layer_1", "data": {"points": [[1.0, 1.0]], "size": 2.0}}},
                ]
            }),
        ));
        assert_eq!(batch["count"], json!(2));
        let changeset = batch["changeset_id"].as_str().unwrap().to_owned();
        let log = text_of(&tool(&mut server, "get_log", json!({})));
        let matching: Vec<&Value> = log["atoms"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|atom| atom["changeset_id"].as_str() == Some(changeset.as_str()))
            .collect();
        assert_eq!(matching.len(), 2, "batch 内原子共享变更集");

        // 嵌套 batch 被拒绝。
        let nested = tool(
            &mut server,
            "batch",
            json!({"calls": [{"tool": "batch", "arguments": {"calls": []}}]}),
        );
        assert_eq!(nested["isError"], json!(true));
    }

    #[test]
    fn wait_for_render_false_defers_jobs() {
        let mut server = Server::new(McpOptions {
            doc_id: "doc_jobs".to_owned(),
            width: 32,
            height: 32,
            profiles: vec![Profile::Core, Profile::History],
            wait_for_render: false,
            ..McpOptions::default()
        });
        tool(&mut server, "create_layer", json!({"layer_id": "layer_1"}));
        tool(
            &mut server,
            "draw_stroke",
            json!({"layer_id": "layer_1", "data": {"points": [[2.0, 2.0], [8.0, 8.0]], "size": 3.0}}),
        );
        // 重型原子：declare_head（6.5）——`--no-wait` 下立即返回 job_pending。
        let log = text_of(&tool(&mut server, "get_log", json!({})));
        let first_atom = log["atoms"][0]["atom_id"].as_str().unwrap().to_owned();
        let head = text_of(&tool(
            &mut server,
            "revert_to",
            json!({"atom_id": first_atom}),
        ));
        let job_id = head["job_id"]
            .as_str()
            .expect("重型原子应产生 job")
            .to_owned();
        assert_eq!(head["job_status"], json!("submitted"));

        let job = text_of(&tool(&mut server, "get_job", json!({"job_id": job_id})));
        assert_eq!(job["status"], json!("submitted"));
        assert_eq!(job["terminal"], json!(false));

        // 渲染水位不推进，Agent 用 get_render_status 轮询。
        let status = text_of(&tool(
            &mut server,
            "get_render_status",
            json!({"atom_id": head["atom_id"]}),
        ));
        assert_eq!(status["rendered"], json!(false));

        // 显式渲染后 job 完成、水位推进。
        let render = tool(
            &mut server,
            "render_region",
            json!({"region": {"x": 0, "y": 0, "w": 4, "h": 4}}),
        );
        assert_eq!(text_of(&render)["width"], json!(4));
        let status = text_of(&tool(
            &mut server,
            "get_render_status",
            json!({"atom_id": head["atom_id"]}),
        ));
        assert_eq!(status["rendered"], json!(true), "渲染后水位推进");
    }

    #[test]
    fn parse_errors_and_option_parsing() {
        let mut server = server();
        let response = server.handle_line("{not json").unwrap();
        assert!(response.contains("-32700"));
        assert!(server.handle_line("").is_none());
        assert!(server.handle_line("   ").is_none());

        let options = McpOptions::parse_args([
            "--root",
            "/tmp/yanshi-mcp",
            "--doc",
            "doc_x",
            "--width",
            "64",
            "--height",
            "48",
            "--profile",
            "core,annotation",
            "--no-wait",
            "--wait-budget",
            "1200",
        ])
        .unwrap();
        assert_eq!(options.doc_id, "doc_x");
        assert_eq!((options.width, options.height), (64, 48));
        assert!(options.profiles.contains(&Profile::Annotation));
        assert!(!options.wait_for_render);
        assert_eq!(options.wait_budget_ms, 1200);
        assert!(options.root.is_some());
        assert!(McpOptions::parse_args(["--bogus"]).is_err());
        assert!(McpOptions::parse_args(["--width"]).is_err());
        assert!(McpOptions::help().contains("yanshi-mcp"));
    }

    #[test]
    fn serve_loop_reads_and_writes_lines() {
        let input = concat!(
            "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n",
            "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n",
            "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"get_document\",\"arguments\":{}}}\n"
        );
        let mut output = Vec::new();
        serve(
            std::io::Cursor::new(input.as_bytes()),
            &mut output,
            McpOptions {
                doc_id: "doc_stdio".to_owned(),
                width: 16,
                height: 16,
                ..McpOptions::default()
            },
        )
        .unwrap();
        let text = String::from_utf8(output).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "通知不产生响应行");
        assert!(lines[0].contains("protocolVersion"));
        assert!(lines[1].contains("doc_stdio"));
    }
}
