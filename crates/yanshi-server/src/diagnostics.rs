//! **诊断包：把排查一次事故所需的信息与文件打成*一个*压缩包（zip）** ✓。
//!
//! ## 为什么会有这个模块
//!
//! P0 报告说"一份文档丢了几小时的工作" ✓，而复盘时**柜台是空的** ✗：
//! 那份最终拿到的复现材料**并没有复现丢失** ✓ ⇒ 谁也无法回答
//! "当时磁盘上的原子日志是不是本来就缺一截" ✗。更糟的是 MCP stdio 模式
//! **没有日志文件** ✗ —— 它的 stderr 由客户端丢弃 ✓ ⇒ 事发之后**没有任何东西可查** ✗。
//!
//! 所以这个模块只做一件事：**在事故发生时，把能拿到的证据一次性收齐、压成 zip 交出来** ✓。
//! 两个面（MCP/工具 与 Web 查看器）**共用本模块的 [`collect`]** ✓
//! ⇒ 包内 **条目不会漂移** ✓（两个面各自实现一份必然漂移 ✗ —— 本项目的老毛病 ✓）。
//!
//! ## 归档格式与取舍（依赖决策，两面都写 ✓）
//!
//! **格式：zip（DEFLATE，纯 Rust 的 miniz_oxide 后端）** ✓。
//!
//! * **收益** ✓：任何维护者手上都有能打开 `.zip` 的东西 ✓（`unzip` / 资源管理器 /
//!   Python 的 `zipfile` / 本仓库的测试 ✓）—— 排查材料最怕"打不开" ✗；
//!   `zip` crate 处理了 CRC、中央目录与真实读取器兼容这些**容易写错**的部分 ✓
//!   （本项目 `archive.rs` 的手写 tar 之所以可行，是因为 tar **只是一层 512 字节头** ✓；
//!   zip 不是 ✓ ⇒ 按 AGENTS.md 的判据，**成熟 crate 是更好的取舍** ✓）。
//! * **代价** ✓：新增 7 个包（`zip` / `flate2` / `miniz_oxide` / `crc32fast` /
//!   `indexmap` / `hashbrown` / `equivalent` / `typed-path` 中的直接与传递项 ✓），
//!   实测 `cargo fetch` 多下载约 200 KB、**构建时间多几秒** ✓；
//!   `--offline` 环境下需要这些包已在缓存里 ✓。默认特性**没有**启用 ✓ ——
//!   只开 `deflate-flate2`（miniz_oxide ✓），**不引** zopfli / bzip2 / zstd / lzma / aes ✓
//!   （默认特性会把它们与七八种压缩后端全拉进来 ✗，体积与供应链面都不划算 ✓）。
//!
//! ## 隐私与体积（硬要求）
//!
//! * 归档里**不许**出现认证令牌、会话机密或绝对家目录路径 ✓ ——
//!   所有文本条目都过 [`redact_text`] / [`redact_value`] ✓，并写出 `privacy.json` 如实报告洗了多少处 ✓；
//! * 归档**有上限** ✓：未压缩内容 ≤ [`MAX_CONTENT_BYTES`]（3 MiB）✓，
//!   zip 本身 ≤ [`MAX_ARCHIVE_BYTES`]（4 MiB）✓ ⇒ 超了就**按固定顺序裁剪并记录** ✓，绝不无限增长 ✓；
//! * stderr 走**环形缓冲** ✓（[`LOG_RING_CAPACITY`] 行）✓ —— 只保留最近若干行 ✓，
//!   容量固定 ✓，内存不会随运行时长增长 ✓。
//!
//! ## 明确**采集不到**的东西（写在包里也写在这里）
//!
//! * **进程 fd 2 上由第三方/panic 直接写出的字节** ✗：本 crate 有 `#![forbid(unsafe_code)]` ✓，
//!   无法 `dup2` 接管 stderr ✓；我们只捕获**经本模块 [`log_line`] 走出的行** ✓
//!   （服务端自己的启动信息、工具失败、以及 panic hook 转进来的 panic 摘要 ✓）。
//! * **已经翻页掉的原子** ✗：只带日志尾部（[`MAX_ATOM_TAIL`] 条 ✓），更早的只给计数与首个 seq ✓。
//! * **二进制像素全量** ✗：只带当前缩略图（若已在 CAS 里且 ≤ [`MAX_THUMBNAIL_BYTES`] ✓）。
//! * **历史 stderr** ✗：环形缓冲被覆盖掉的行只报"丢了多少行" ✓。

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Map, Value};
use yanshi_core::{ErrorCode, ErrorContext, Result, YanshiError};

use crate::archive::TarEntry;
use crate::service::Workspace;

/// stderr 环形缓冲的容量（**行**）✓ —— 固定值，缓冲不会随运行时长增长 ✓。
pub const LOG_RING_CAPACITY: usize = 512;
/// 归档内**未压缩**内容的上限（3 MiB）✓；超出按固定顺序裁剪 ✓。
pub const MAX_CONTENT_BYTES: usize = 3 * 1024 * 1024;
/// zip 归档本身的硬上限（4 MiB）✓ —— 由 [`DiagnosticsBundle::zip`] 断言 ✓。
pub const MAX_ARCHIVE_BYTES: usize = 4 * 1024 * 1024;
/// 原子日志最多带多少条（**尾部**）✓。
pub const MAX_ATOM_TAIL: usize = 1000;
/// 单条原子的 JSON 行上限 ✓；超长原子退化成摘要对象 ✓（不写半截 JSON ✗）。
pub const MAX_ATOM_LINE_BYTES: usize = 8 * 1024;
/// `stderr.log` 的总字节上限 ✓。
pub const MAX_STDERR_BYTES: usize = 128 * 1024;
/// 单条 stderr 行上限 ✓。
pub const MAX_STDERR_LINE_BYTES: usize = 4 * 1024;
/// 缩略图字节上限 ✓（超过就只留 `thumbnail.json` 里的说明 ✓）。
pub const MAX_THUMBNAIL_BYTES: usize = 1024 * 1024;
/// 通用文本条目（README / config / build）上限 ✓。
pub const MAX_TEXT_BYTES: usize = 256 * 1024;
/// 记录最近多少次工具调用的耗时 + 告警 ✓。
pub const MAX_TIMING_RECORDS: usize = 32;

// ---------------------------------------------------------------------------
// ① stderr 环形缓冲
// ---------------------------------------------------------------------------

/// **固定容量的 stderr 环形缓冲** ✓。
///
/// **为什么必须是环** ✗：MCP stdio 模式的 stderr 会被客户端丢掉 ✓ ⇒
/// 若不自己留一份，事发之后**一个字都没有** ✗；而"留一份"若不做上限 ✓，
/// 一个跑几天的常驻服务就会把内存吃光 ✗ ⇒ 固定容量 + 覆盖最旧 ✓。
#[derive(Debug)]
struct LogRing {
    lines: VecDeque<String>,
    capacity: usize,
    dropped: u64,
}

impl LogRing {
    /// 以给定容量新建。
    fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    /// 追加一行；满了就丢最旧的（`dropped` 只增不减 ✓）。
    fn push(&mut self, line: String) {
        if self.capacity == 0 {
            self.dropped += 1;
            return;
        }
        while self.lines.len() >= self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line);
    }

    /// 当前内容的快照（最旧 → 最新 ✓）。
    fn snapshot(&self) -> Vec<String> {
        self.lines.iter().cloned().collect()
    }

    /// 清空（测试用 ✓）。
    fn clear(&mut self) {
        self.lines.clear();
        self.dropped = 0;
    }
}

/// 环形缓冲的统计（可断言 ✓）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogStats {
    /// 容量（行）。
    pub capacity: usize,
    /// 当前行数（**恒 ≤ 容量** ✓）。
    pub len: usize,
    /// 因容量上限被丢弃的行数。
    pub dropped: u64,
}

fn log_ring() -> &'static Mutex<LogRing> {
    static RING: OnceLock<Mutex<LogRing>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(LogRing::new(LOG_RING_CAPACITY)))
}

/// **写一行服务端日志** ✓ —— 同时进环形缓冲 ✓ 与真实 stderr ✓。
///
/// 服务端自己的诊断输出都应该走这里 ✓（启动信息 ✓、工具失败 ✓、panic 摘要 ✓）；
/// 直接 `eprintln!` 的行**进不了归档** ✗（见模块头"采集不到的东西" ✓）。
pub fn log_line(text: impl AsRef<str>) {
    let text = text.as_ref();
    if let Ok(mut ring) = log_ring().lock() {
        ring.push(text.to_owned());
    }
    // **照旧写 stderr** ✓：交互式启动时人还是要在终端里看到它 ✓。
    eprintln!("{text}");
}

/// 环形缓冲的统计 ✓（判据用它断言"不会无界增长" ✓）。
pub fn log_stats() -> LogStats {
    match log_ring().lock() {
        Ok(ring) => LogStats {
            capacity: ring.capacity,
            len: ring.lines.len(),
            dropped: ring.dropped,
        },
        Err(_) => LogStats {
            capacity: LOG_RING_CAPACITY,
            len: 0,
            dropped: 0,
        },
    }
}

/// 环形缓冲当前内容的快照 ✓（最旧 → 最新 ✓）。
pub fn log_lines() -> Vec<String> {
    log_ring()
        .lock()
        .map(|ring| ring.snapshot())
        .unwrap_or_default()
}

/// 清空环形缓冲 ✓（**只给测试用** ✓ —— 生产路径没有理由清掉证据 ✗）。
pub fn clear_logs() {
    if let Ok(mut ring) = log_ring().lock() {
        ring.clear();
    }
}

/// 安装 panic hook：把 panic 摘要转进环形缓冲，再交给原来的 hook ✓。
///
/// **为什么要装** ✓：panic 是"进程没了、什么都没留下"的最坏情形 ✓；
/// 转一份摘要进环 ✓ ⇒ 下一次采集至少能看到"它为什么倒的" ✓。
/// **幂等** ✓：重复调用只装一次 ✓。
pub fn install_panic_hook() {
    static INSTALLED: std::sync::Once = std::sync::Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log_line(format!("PANIC: {info}"));
            previous(info);
        }));
    });
}

// ---------------------------------------------------------------------------
// ② 最近的工具结果（阶段耗时 + 告警）—— "工具结果里已经有的那份" ✓
// ---------------------------------------------------------------------------

/// 一条工具结果的观察记录 ✓。
#[derive(Debug, Clone)]
struct ToolRecord {
    tool: String,
    at_ms: i64,
    timings: Value,
    warnings: Value,
}

fn tool_ring() -> &'static Mutex<VecDeque<ToolRecord>> {
    static RING: OnceLock<Mutex<VecDeque<ToolRecord>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_TIMING_RECORDS)))
}

/// **记下一条工具结果** ✓（由 [`crate::tools::ToolRegistry::call`] 在收口处调用 ✓）。
///
/// 只留最近 [`MAX_TIMING_RECORDS`] 条 ✓ —— 与 stderr 同样的道理：**固定容量、覆盖最旧** ✓。
pub fn record_tool_result(tool: &str, response: &Value) {
    let record = ToolRecord {
        tool: tool.to_owned(),
        at_ms: yanshi_core::now_ms(),
        timings: response.get("timings").cloned().unwrap_or(Value::Null),
        warnings: response.get("warnings").cloned().unwrap_or(Value::Null),
    };
    if let Ok(mut ring) = tool_ring().lock() {
        while ring.len() >= MAX_TIMING_RECORDS {
            ring.pop_front();
        }
        ring.push_back(record);
    }
}

/// 最近若干条工具结果的 `{tool, at_ms, timings, warnings}` ✓（最新在最后 ✓）。
pub fn recent_tool_results() -> Vec<Value> {
    tool_ring()
        .lock()
        .map(|ring| {
            ring.iter()
                .map(|record| {
                    json!({
                        "tool": record.tool,
                        "at_ms": record.at_ms,
                        "timings": record.timings,
                        "warnings": record.warnings,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 清空工具结果环 ✓（**只给测试用** ✓）。
pub fn clear_tool_results() {
    if let Ok(mut ring) = tool_ring().lock() {
        ring.clear();
    }
}

// ---------------------------------------------------------------------------
// ③ 去密（隐私硬要求）
// ---------------------------------------------------------------------------

/// 抹除后的占位串 ✓（判据可以据此确认"这里被洗过" ✓）。
pub const REDACTED: &str = "[已抹除]";

/// JSON 里"名字一看就是机密"的键 ✓（**精确匹配** ✓，不做前缀匹配 ✗ —— 免得误伤 `keys` 之类 ✓）。
const SECRET_KEYS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "api_token",
    "access_token",
    "refresh_token",
    "authorization",
    "auth",
    "key",
    "capability",
    "capability_token",
];

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SECRET_KEYS.contains(&lower.as_str())
}

/// 去密器：一次采集里累计洗掉了多少处 ✓。
#[derive(Debug)]
struct Redactor<'a> {
    secrets: Vec<&'a String>,
    secret_hits: usize,
    home_hits: usize,
}

impl<'a> Redactor<'a> {
    fn new(secrets: &'a [String]) -> Self {
        Self {
            // **过短的"机密"不参与全局替换** ✗：1~3 个字符的串几乎必然误伤正文 ✓。
            secrets: secrets.iter().filter(|value| value.len() >= 4).collect(),
            secret_hits: 0,
            home_hits: 0,
        }
    }

    /// 用占位串替换所有出现 ✓（调用方自己数命中 ✓）。
    fn replace_all(text: &str, needle: &str, label: &str) -> (String, usize) {
        if needle.is_empty() {
            return (text.to_owned(), 0);
        }
        let mut hits = 0;
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(index) = rest.find(needle) {
            out.push_str(&rest[..index]);
            out.push_str(label);
            rest = &rest[index + needle.len()..];
            hits += 1;
        }
        out.push_str(rest);
        (out, hits)
    }

    /// 抹掉查询串 / 头里的机密值（`token=…` / `key=…` / `Bearer …` ✓）。
    fn replace_named_values(text: &str) -> (String, usize) {
        const NAMES: &[&str] = &[
            "token=",
            "key=",
            "api_key=",
            "apikey=",
            "secret=",
            "password=",
            "access_token=",
        ];
        let mut hits = 0;
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        'outer: loop {
            // 找**最靠前**的那个名字（保证从左到右不再回头 ✓）。
            let mut best: Option<(usize, &str)> = None;
            for name in NAMES {
                if let Some(index) = rest.find(name) {
                    if best.map(|(at, _)| index < at).unwrap_or(true) {
                        best = Some((index, name));
                    }
                }
            }
            let Some((index, name)) = best else {
                out.push_str(rest);
                break 'outer;
            };
            out.push_str(&rest[..index]);
            out.push_str(name);
            out.push_str(REDACTED);
            let after = index + name.len();
            // 值的边界：`&` `"` `'` 空白 `?` `#` 与行尾 ✓。
            let tail = &rest[after..];
            let end = tail
                .find(|ch: char| {
                    ch == '&'
                        || ch == '"'
                        || ch == '\''
                        || ch == '?'
                        || ch == '#'
                        || ch.is_whitespace()
                })
                .unwrap_or(tail.len());
            // **空值也算命中** ✓（`token=` 本身就是"这里有令牌"的证据 ✓）。
            hits += 1;
            rest = &tail[end..];
        }
        // `Authorization: Bearer <令牌>` 里的令牌也要洗 ✓（它不带 `=` 前缀 ✓）。
        let (out, bearer) = Self::replace_bearer(&out);
        (out, hits + bearer)
    }

    /// 抹掉 `Bearer <值>` 里的值 ✓。
    fn replace_bearer(text: &str) -> (String, usize) {
        const BEARER: &str = "Bearer ";
        let mut hits = 0;
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(index) = rest.find(BEARER) {
            out.push_str(&rest[..index]);
            out.push_str(BEARER);
            out.push_str(REDACTED);
            let after = index + BEARER.len();
            let tail = &rest[after..];
            let end = tail
                .find(|ch: char| ch == '"' || ch == '\'' || ch.is_whitespace())
                .unwrap_or(tail.len());
            hits += 1;
            rest = &tail[end..];
        }
        out.push_str(rest);
        (out, hits)
    }

    /// 抹掉绝对家目录路径 ✓（`$HOME` 与通用 `/home/<user>`、`/Users/<user>`、`C:\Users\<user>` ✓）。
    fn replace_home(text: &str) -> (String, usize) {
        let mut out = text.to_owned();
        let mut hits = 0;
        // ① 显式 HOME（最可靠 ✓）。
        for key in ["HOME", "USERPROFILE"] {
            if let Ok(home) = std::env::var(key) {
                if home.len() > 1 {
                    let (next, count) = Self::replace_all(&out, &home, "~");
                    out = next;
                    hits += count;
                }
            }
        }
        // ② 通用前缀（HOME 没设 / 路径来自别的机器时兜底 ✓）。
        for prefix in ["/home/", "/Users/", "C:\\Users\\"] {
            while let Some(start) = out.find(prefix) {
                let after = start + prefix.len();
                let sep = if prefix.starts_with('C') { '\\' } else { '/' };
                let end = out[after..]
                    .find(sep)
                    .map(|index| after + index)
                    .unwrap_or(out.len());
                if end <= after {
                    // 空用户名（如 `/home//x`）⇒ 只洗前缀，避免原地打转 ✓。
                    out.replace_range(start..after, "~");
                } else {
                    out.replace_range(start..end, "~");
                }
                hits += 1;
            }
        }
        (out, hits)
    }

    /// 洗一段文本 ✓。
    fn text(&mut self, text: &str) -> String {
        let (mut out, named) = Self::replace_named_values(text);
        self.secret_hits += named;
        for secret in &self.secrets {
            let (next, count) = Self::replace_all(&out, secret.as_str(), REDACTED);
            out = next;
            self.secret_hits += count;
        }
        let (next, home) = Self::replace_home(&out);
        self.home_hits += home;
        next
    }

    /// 洗一个 JSON 值 ✓（机密键一律置为占位串 ✓，字符串走文本去密 ✓）。
    fn value(&mut self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.text(text)),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.value(item)).collect())
            }
            Value::Object(map) => {
                let mut out = Map::new();
                for (key, item) in map {
                    if is_secret_key(key) {
                        // **键名本身就是机密** ⇒ 连值都不看，直接占位 ✓。
                        out.insert(key.clone(), Value::String(REDACTED.to_owned()));
                        self.secret_hits += 1;
                    } else {
                        out.insert(key.clone(), self.value(item));
                    }
                }
                Value::Object(out)
            }
            other => other.clone(),
        }
    }
}

/// 洗一段文本 ✓（纯函数形式，方便单测 ✓）：抹掉命名机密值、给定机密、以及绝对家目录路径 ✓。
pub fn redact_text(text: &str, secrets: &[String]) -> String {
    Redactor::new(secrets).text(text)
}

/// 洗一个 JSON 值 ✓（递归 ✓）。
pub fn redact_value(value: &Value, secrets: &[String]) -> Value {
    Redactor::new(secrets).value(value)
}

// ---------------------------------------------------------------------------
// ④ 采集请求 / 上限 / 结果
// ---------------------------------------------------------------------------

/// 归档的大小与条数上限 ✓ —— **每个都有默认值，且永远有上限** ✓。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// 未压缩内容总上限（字节）✓。
    pub max_content_bytes: usize,
    /// zip 归档上限（字节）✓。
    pub max_archive_bytes: usize,
    /// 原子日志尾部条数上限 ✓。
    pub max_atoms: usize,
    /// `stderr.log` 字节上限 ✓。
    pub max_stderr_bytes: usize,
    /// 缩略图字节上限 ✓。
    pub max_thumbnail_bytes: usize,
    /// 是否尝试带上缩略图 ✓。
    pub include_thumbnail: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_content_bytes: MAX_CONTENT_BYTES,
            max_archive_bytes: MAX_ARCHIVE_BYTES,
            max_atoms: MAX_ATOM_TAIL,
            max_stderr_bytes: MAX_STDERR_BYTES,
            max_thumbnail_bytes: MAX_THUMBNAIL_BYTES,
            include_thumbnail: true,
        }
    }
}

/// **某一个面在采集时提供的、只有它自己知道的事实** ✓。
///
/// **为什么把它做成数据而不是让采集函数去问环境** ✗：两个面拿不到同一份东西 ✓
///（MCP 有 `McpOptions` ✓、Web 有 `HttpOptions` 与 `ServerState` ✓）⇒
/// 由面自己填好 ✓、采集函数只负责**统一组织与去密** ✓ —— 这样包内条目才会一致 ✓。
#[derive(Debug, Clone, Default)]
pub struct SurfaceFacts {
    /// 哪个面发起的（`mcp` / `web` / `tool` / `embed` ✓）。
    pub surface: String,
    /// 构建标识（版本 / commit / 构建时间 ✓）。
    pub build: Value,
    /// 生效配置（**本模块还会再洗一遍** ✓）。
    pub config: Value,
    /// 该面特有的附加事实（连接数 / 请求数 / profiles…… ✓）。
    pub extra: Value,
    /// 必须从所有输出里抹掉的机密值（如 `YANSHI_API_KEY`、当前 capability token ✓）。
    pub secrets: Vec<String>,
}

/// 一次采集请求 ✓。
#[derive(Debug, Clone)]
pub struct DiagnosticsRequest {
    /// 目标文档 id ✓。
    pub doc_id: String,
    /// 面提供的事实 ✓。
    pub facts: SurfaceFacts,
    /// 上限 ✓。
    pub limits: Limits,
}

/// 采集结果：包内文件（**尚未压缩** ✓）+ 人读说明 ✓。
#[derive(Debug, Clone)]
pub struct DiagnosticsBundle {
    /// 包内文件（`path` 用 `/` 分隔 ✓，顺序固定 ✓）。
    pub entries: Vec<TarEntry>,
    /// 人读的说明（截断、缺什么、采到了什么 ✓）。
    pub notes: Vec<String>,
    /// 未压缩内容总字节 ✓。
    pub content_bytes: usize,
    /// 本次采集允许的 zip 上限（字节）✓。
    pub max_archive_bytes: usize,
    /// 去密报告 ✓。
    pub privacy: Value,
}

impl DiagnosticsBundle {
    /// 包内条目名（**稳定顺序** ✓ —— 判据断言的就是它 ✓）。
    pub fn entry_names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect()
    }

    /// 每个条目名 → 字节数 ✓。
    pub fn entry_sizes(&self) -> Value {
        let mut map = Map::new();
        for entry in &self.entries {
            map.insert(entry.path.clone(), json!(entry.bytes.len()));
        }
        Value::Object(map)
    }

    /// 找一条条目 ✓。
    pub fn entry(&self, path: &str) -> Option<&TarEntry> {
        self.entries.iter().find(|entry| entry.path == path)
    }

    /// **打成 zip** ✓（DEFLATE ✓）；若超过上限则**如实报错**而不是交出一个无界归档 ✓。
    pub fn zip(&self) -> Result<Vec<u8>> {
        self.zip_with_limit(self.max_archive_bytes)
    }

    /// 打成 zip，并断言不超过 `max_archive_bytes` ✓。
    pub fn zip_with_limit(&self, max_archive_bytes: usize) -> Result<Vec<u8>> {
        let bytes = write_zip(&self.entries)?;
        if bytes.len() > max_archive_bytes {
            return Err(YanshiError::new(
                ErrorCode::ResourceExhausted,
                ErrorContext::detail(format!(
                    "诊断包 {:#} 字节超过上限 {} 字节 ⇒ 拒绝交出无界归档（请报告：裁剪逻辑没生效）",
                    bytes.len(),
                    max_archive_bytes
                )),
            ));
        }
        Ok(bytes)
    }
}

/// **打包成 zip** ✓（DEFLATE ✓，固定权限 ✓）。
///
/// 取舍见模块头：用成熟 crate 而不是手写 zip 容器 ✓ —— zip 的中央目录、CRC 与
/// 读取器兼容是**容易写错且错了很难发现**的地方 ✓。
fn write_zip(entries: &[TarEntry]) -> Result<Vec<u8>> {
    use zip::write::SimpleFileOptions;
    use zip::CompressionMethod;

    let to_error = |error: zip::result::ZipError| {
        YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!("打 zip 失败：{error}")),
        )
    };
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for entry in entries {
        writer
            .start_file(entry.path.clone(), options)
            .map_err(to_error)?;
        writer.write_all(&entry.bytes).map_err(|error| {
            YanshiError::new(
                ErrorCode::Degraded,
                ErrorContext::detail(format!("写 zip 内容失败：{error}")),
            )
        })?;
    }
    let cursor = writer.finish().map_err(to_error)?;
    Ok(cursor.into_inner())
}

// ---------------------------------------------------------------------------
// ⑤ 采集
// ---------------------------------------------------------------------------

/// 包内条目的固定清单 ✓（**两个面逐字相同** ✓ —— 这就是"条目不会漂移"的落点 ✓）。
///
/// 顺序固定 ✓；两个面产出的 `entry_names()` 必须与它一致 ✓（判据断言 ✓）。
pub const ENTRY_NAMES: &[&str] = &[
    "README.txt",
    "build.json",
    "config.json",
    "surface.json",
    "document.json",
    "atoms.jsonl",
    "atoms.meta.json",
    "warnings.json",
    "timings.json",
    "stderr.log",
    "stderr.meta.json",
    "thumbnail.json",
    "thumbnail.bin",
    "privacy.json",
];

/// **采集**：把两个面共用的那一份证据收齐 ✓（**唯一的实现** ✓）。
///
/// 只读 ✓：不改文档、不写原子 ✓ —— 只读调用方（viewer 令牌）也能用 ✓。
pub fn collect(workspace: &Workspace, request: &DiagnosticsRequest) -> Result<DiagnosticsBundle> {
    let doc_id = request.doc_id.as_str();
    let limits = request.limits;
    let mut notes: Vec<String> = Vec::new();

    // ---- 文档元数据 -------------------------------------------------------
    let summary = workspace.summary_json(doc_id)?;
    let (atom_lines_all, atom_total, log_head_seq) = atom_lines(workspace, doc_id);
    let open_documents: Vec<Value> = workspace
        .list_documents()?
        .into_iter()
        .map(|summary| {
            json!({
                "doc_id": summary.doc_id,
                "width": summary.width,
                "height": summary.height,
                "atoms": summary.atoms,
                "head_seq": summary.head_seq,
                "layers": summary.layers,
                "objects": summary.objects,
                "persisted": summary.persisted,
            })
        })
        .collect();
    let mut document = summary;
    if let Value::Object(map) = &mut document {
        map.insert("atoms_in_log".to_owned(), json!(atom_total));
        map.insert("log_head_seq".to_owned(), json!(log_head_seq));
        map.insert(
            "open_documents".to_owned(),
            Value::Array(open_documents.clone()),
        );
        map.insert(
            "open_document_count".to_owned(),
            json!(open_documents.len()),
        );
    }
    notes.push(format!(
        "文档 {doc_id}：原子 {atom_total} 条、head seq {log_head_seq}、打开中的文档 {} 份。",
        open_documents.len()
    ));

    // ---- 原子日志尾部 -----------------------------------------------------
    let atom_cap = limits.max_atoms.min(atom_lines_all.len());
    let start = atom_lines_all.len().saturating_sub(atom_cap);
    let mut atom_lines: Vec<String> = atom_lines_all[start..].to_vec();
    let atoms_truncated = start > 0;
    let first_seq = first_seq_of(&atom_lines_all, start);
    let last_seq = log_head_seq;
    if atoms_truncated {
        notes.push(format!(
            "原子日志**被截断**：只带最后 {atom_cap} 条（共 {atom_total} 条）⇒ 更早的从 seq {} 之前开始缺。",
            first_seq.unwrap_or(0)
        ));
    }

    // ---- 渲染告警（含被跳过的补丁）---------------------------------------
    let last_render_warnings = workspace.last_render_warnings(doc_id);
    let recent = recent_tool_results();
    let recent_warnings: Vec<Value> = recent
        .iter()
        .filter(|record| {
            record
                .get("warnings")
                .map(|value| !value.is_null())
                .unwrap_or(false)
        })
        .map(|record| {
            json!({
                "tool": record.get("tool").cloned().unwrap_or(Value::Null),
                "at_ms": record.get("at_ms").cloned().unwrap_or(Value::Null),
                "warnings": record.get("warnings").cloned().unwrap_or(Value::Null),
            })
        })
        .collect();
    let warnings = json!({
        "last_render_warnings": last_render_warnings,
        "skipped_patch_count": last_render_warnings.len(),
        "recent_tool_warnings": recent_warnings,
        "note": "last_render_warnings 就是「被跳过的补丁 / 缺失 blob」的原文。\
                 **一张缺了补丁的画与「数据丢了」长得一模一样** —— 这里若有内容，先看它。",
    });
    if !last_render_warnings.is_empty() {
        notes.push(format!(
            "**渲染告警 {} 条**（有补丁被跳过）：{}",
            last_render_warnings.len(),
            last_render_warnings.join(" ｜ ")
        ));
    }

    // ---- 缩略图 -----------------------------------------------------------
    let (thumbnail_bytes, thumbnail_meta) = read_thumbnail(workspace, doc_id, limits, &mut notes);

    // ---- stderr -----------------------------------------------------------
    let all_log_lines = log_lines();
    let ring = log_stats();
    let mut stderr_text = String::new();
    let mut kept_from = all_log_lines.len();
    let mut per_line_truncated = false;
    for (index, line) in all_log_lines.iter().enumerate().rev() {
        let mut line = line.clone();
        if line.len() > MAX_STDERR_LINE_BYTES {
            per_line_truncated = true;
        }
        truncate_chars(&mut line, MAX_STDERR_LINE_BYTES, "…[本行截断]");
        if stderr_text.len() + line.len() + 1 > limits.max_stderr_bytes {
            break;
        }
        stderr_text.insert_str(0, &format!("{line}\n"));
        kept_from = index;
    }
    let stderr_truncated = kept_from > 0 || per_line_truncated;
    if ring.dropped > 0 {
        notes.push(format!(
            "stderr 环形缓冲已覆盖 {} 行（容量 {} 行）⇒ 更早的输出**已经丢了**。",
            ring.dropped, ring.capacity
        ));
    }
    if kept_from > 0 {
        notes.push(format!(
            "stderr 按字节上限裁剪：保留最后 {} 行中的从第 {} 行起。",
            all_log_lines.len(),
            kept_from + 1
        ));
    }

    // ---- 组装（顺序固定 = ENTRY_NAMES ✓）---------------------------------
    let build = json!({
        "platform": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "family": std::env::consts::FAMILY,
        },
        "server_crate_version": env!("CARGO_PKG_VERSION"),
        "reported_by_surface": request.facts.build,
    });
    let surface = json!({
        "surface": if request.facts.surface.is_empty() { "unknown" } else { request.facts.surface.as_str() },
        "produced_at_ms": yanshi_core::now_ms(),
        "extra": request.facts.extra,
    });

    let mut entries: Vec<TarEntry> = Vec::new();
    let mut push = |path: &str, bytes: Vec<u8>| {
        entries.push(TarEntry {
            path: path.to_owned(),
            bytes,
        });
    };
    push("README.txt", Vec::new()); // 占位：内容最后生成 ✓
    push(
        "build.json",
        to_json_bytes(&build, MAX_TEXT_BYTES, &mut notes, "build.json"),
    );
    push(
        "config.json",
        to_json_bytes(
            &request.facts.config,
            MAX_TEXT_BYTES,
            &mut notes,
            "config.json",
        ),
    );
    push(
        "surface.json",
        to_json_bytes(&surface, MAX_TEXT_BYTES, &mut notes, "surface.json"),
    );
    push(
        "document.json",
        to_json_bytes(&document, MAX_TEXT_BYTES, &mut notes, "document.json"),
    );
    push("atoms.jsonl", atom_lines.join("\n").into_bytes());
    push(
        "atoms.meta.json",
        to_json_bytes(
            &json!({
                "total": atom_total,
                "included": atom_lines.len(),
                "first_included_seq": first_seq,
                "last_included_seq": last_seq,
                "truncated": atoms_truncated,
                "max_atoms": limits.max_atoms,
                "per_line_limit_bytes": MAX_ATOM_LINE_BYTES,
            }),
            MAX_TEXT_BYTES,
            &mut notes,
            "atoms.meta.json",
        ),
    );
    push(
        "warnings.json",
        to_json_bytes(&warnings, MAX_TEXT_BYTES, &mut notes, "warnings.json"),
    );
    push(
        "timings.json",
        to_json_bytes(
            &json!({
                "recent_tool_results": recent,
                "max_records": MAX_TIMING_RECORDS,
                "note": "阶段耗时的字段解释见 timings.rs：prep/raster/dirty/fold/log + other（残差），六项之和 = total_ms。",
            }),
            MAX_TEXT_BYTES,
            &mut notes,
            "timings.json",
        ),
    );
    push("stderr.log", stderr_text.into_bytes());
    push(
        "stderr.meta.json",
        to_json_bytes(
            &json!({
                "ring_capacity": ring.capacity,
                "buffered_lines": ring.len,
                "dropped_lines": ring.dropped,
                "included_from_line": kept_from + 1,
                "total_lines": all_log_lines.len(),
                "truncated": stderr_truncated,
                "max_bytes": limits.max_stderr_bytes,
                "per_line_limit_bytes": MAX_STDERR_LINE_BYTES,
                "note": "这是服务端自己的 stderr 环形缓冲（MCP stdio 模式下客户端会丢弃 stderr ⇒ 这份是唯一留痕）。fd 2 上由第三方直接写出的字节不在其中。",
            }),
            MAX_TEXT_BYTES,
            &mut notes,
            "stderr.meta.json",
        ),
    );
    push(
        "thumbnail.json",
        to_json_bytes(
            &thumbnail_meta,
            MAX_TEXT_BYTES,
            &mut notes,
            "thumbnail.json",
        ),
    );
    push("thumbnail.bin", thumbnail_bytes);
    // **去密报告占位** ✓：内容要等所有文本洗完之后才有数字 ✓（见下面 ✓）。
    push("privacy.json", Vec::new());

    // ---- 去密（所有文本条目一起洗 ✓）-------------------------------------
    let mut redactor = Redactor::new(&request.facts.secrets);
    for entry in &mut entries {
        if entry.path == "thumbnail.bin" {
            continue; // PNG 是像素，没有可洗的文本 ✓
        }
        let text = String::from_utf8_lossy(&entry.bytes).into_owned();
        entry.bytes = redactor.text(&text).into_bytes();
    }

    // ---- 体积裁剪（**先裁到给 README 留出余量** ✓）-----------------------
    // **余量按上限取比例** ✓：写死 64 KiB 会让很小的自定义上限把 target 压成 0 ✓
    //（于是所有条目都被砍光 ✗）；取 `min(64 KiB, 上限/4)` ✓ ⇒ 小上限也有意义 ✓。
    let readme_reserve = (64 * 1024).min(limits.max_content_bytes / 4);
    let content_target = limits.max_content_bytes.saturating_sub(readme_reserve);
    let mut trimmed = Vec::new();
    trim_to_budget(
        &mut entries,
        &mut atom_lines,
        &mut notes,
        &mut trimmed,
        content_target,
    );

    // ---- README（最后生成 ✓，因为要写清裁掉了什么 ✓）--------------------
    let readme = build_readme(&request.doc_id, &notes, &trimmed);
    if let Some(entry) = entries.iter_mut().find(|entry| entry.path == "README.txt") {
        entry.bytes = readme.into_bytes();
    }
    // README 也可能带进机密（notes 里有告警原文 ✓）⇒ 单独再洗一遍 ✓。
    if let Some(entry) = entries.iter_mut().find(|entry| entry.path == "README.txt") {
        let text = String::from_utf8_lossy(&entry.bytes).into_owned();
        entry.bytes = redactor.text(&text).into_bytes();
    }

    // ---- 最终上限断言 -----------------------------------------------------
    let mut content_bytes: usize = entries.iter().map(|entry| entry.bytes.len()).sum();
    if content_bytes > limits.max_content_bytes {
        // 兜底：把原子与 stderr 再砍一半，直到进线 ✓（README 本身很小 ✓）。
        while content_bytes > limits.max_content_bytes {
            let before = content_bytes;
            halve_entry(&mut entries, "atoms.jsonl");
            content_bytes = entries.iter().map(|entry| entry.bytes.len()).sum();
            if content_bytes >= before {
                halve_entry(&mut entries, "stderr.log");
                content_bytes = entries.iter().map(|entry| entry.bytes.len()).sum();
            }
            if content_bytes >= before {
                break;
            }
        }
        notes.push(format!(
            "为满足未压缩上限 {} 字节，最后又做了一轮兜底裁剪。",
            limits.max_content_bytes
        ));
    }

    let privacy = json!({
        "secrets_replaced": redactor.secret_hits,
        "home_paths_replaced": redactor.home_hits,
        "secret_key_values": "token/secret/password/api_key/key/authorization 等键的值一律置为占位串",
        "home_prefixes": ["$HOME", "/home/<user>", "/Users/<user>", "C:\\\\Users\\\\<user>"],
        "note": "归档里不许出现令牌、会话机密与绝对家目录路径。若仍有可疑内容，请把本文件与 README 一起报告。",
    });
    if let Some(entry) = entries
        .iter_mut()
        .find(|entry| entry.path == "privacy.json")
    {
        entry.bytes = serde_json::to_vec_pretty(&privacy).unwrap_or_default();
    }
    content_bytes = entries.iter().map(|entry| entry.bytes.len()).sum();

    // 顺序与 ENTRY_NAMES 对齐 ✓（README 占位时就已经按下标放好 ✓）。
    let mut by_name: std::collections::BTreeMap<String, Vec<u8>> = entries
        .into_iter()
        .map(|entry| (entry.path, entry.bytes))
        .collect();
    let ordered: Vec<TarEntry> = ENTRY_NAMES
        .iter()
        .map(|name| TarEntry {
            path: (*name).to_owned(),
            bytes: by_name.remove(*name).unwrap_or_default(),
        })
        .collect();

    Ok(DiagnosticsBundle {
        entries: ordered,
        notes,
        content_bytes,
        max_archive_bytes: limits.max_archive_bytes,
        privacy,
    })
}

/// 读当前缩略图（**不新渲染** ✓ —— 只读 CAS 里已有的那份 ✓）。
fn read_thumbnail(
    workspace: &Workspace,
    doc_id: &str,
    limits: Limits,
    notes: &mut Vec<String>,
) -> (Vec<u8>, Value) {
    if !limits.include_thumbnail {
        return (
            Vec::new(),
            json!({"included": false, "reason": "调用方用 include_thumbnail=false 明确要求跳过"}),
        );
    }
    let Some(document) = workspace.document(doc_id) else {
        return (
            Vec::new(),
            json!({"included": false, "reason": format!("文档 {doc_id} 未打开")}),
        );
    };
    let Some(hash) = document.document_thumbnail_blob().cloned() else {
        return (
            Vec::new(),
            json!({"included": false, "reason": "当前没有缓存的文档缩略图（诊断包**不**为了它现场渲染整幅 ✓）"}),
        );
    };
    match workspace.store().get(&hash) {
        Ok(bytes) if bytes.len() <= limits.max_thumbnail_bytes => {
            let meta = json!({
                "included": true,
                "blob": hash.as_str(),
                "bytes": bytes.len(),
                "mime_type": "image/png",
            });
            (bytes, meta)
        }
        Ok(bytes) => {
            notes.push(format!(
                "缩略图 {} 字节超过上限 {} ⇒ 只留说明，不带像素。",
                bytes.len(),
                limits.max_thumbnail_bytes
            ));
            (
                Vec::new(),
                json!({
                    "included": false,
                    "blob": hash.as_str(),
                    "bytes": bytes.len(),
                    "reason": format!("缩略图 {} 字节超过上限 {}", bytes.len(), limits.max_thumbnail_bytes),
                }),
            )
        }
        Err(error) => (
            Vec::new(),
            json!({"included": false, "blob": hash.as_str(), "reason": format!("读缩略图失败：{error}")}),
        ),
    }
}

/// 取原子日志的 JSONL 行 + 总数 + head seq。
fn atom_lines(workspace: &Workspace, doc_id: &str) -> (Vec<String>, usize, u64) {
    let Some(document) = workspace.document(doc_id) else {
        return (Vec::new(), 0, 0);
    };
    let log = document.log();
    let lines: Vec<String> = log
        .iter()
        .map(|atom| match serde_json::to_string(atom) {
            Ok(text) if text.len() <= MAX_ATOM_LINE_BYTES => text,
            // **超长原子退化成摘要对象** ✓：截断原始 JSON 会写出半截行 ✗（JSONL 就不能逐行解析了 ✓）。
            Ok(text) => json!({
                "seq": atom.seq,
                "id": atom.id,
                "kind": atom.kind.as_str(),
                "actor": atom.actor,
                "timestamp": atom.timestamp,
                "truncated": true,
                "approx_bytes": text.len(),
            })
            .to_string(),
            Err(error) => json!({
                "seq": atom.seq,
                "id": atom.id,
                "kind": atom.kind.as_str(),
                "serialize_error": error.to_string(),
            })
            .to_string(),
        })
        .collect();
    (lines, log.len(), log.head_seq())
}

/// 原子行数组里第 `start` 条对应的 seq（拿不到就 `None` ✓）。
fn first_seq_of(lines: &[String], start: usize) -> Option<u64> {
    let text = lines.get(start)?;
    let value: Value = serde_json::from_str(text).ok()?;
    value.get("seq").and_then(Value::as_u64)
}

/// 序列化 JSON 并套文本上限 ✓。
fn to_json_bytes(value: &Value, max: usize, notes: &mut Vec<String>, name: &str) -> Vec<u8> {
    let bytes = serde_json::to_vec_pretty(value).unwrap_or_default();
    if bytes.len() <= max {
        return bytes;
    }
    // **超限就只剩一个说明对象** ✓ —— 宁可条目小、也不交出无界归档 ✓。
    notes.push(format!(
        "{name} {} 字节超过条目上限 {} ⇒ 已降级为说明对象。",
        bytes.len(),
        max
    ));
    serde_json::to_vec_pretty(&json!({
        "truncated": true,
        "original_bytes": bytes.len(),
        "max_bytes": max,
        "note": "本条目过大，已降级；具体数值见同目录其它条目或 README。",
    }))
    .unwrap_or_default()
}

/// 把字符数（不是字节数）截到上限 ✓ —— UTF-8 边界安全 ✓。
fn truncate_chars(text: &mut String, max_bytes: usize, suffix: &str) {
    if text.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes.saturating_sub(suffix.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(suffix);
}

/// 把 `atoms.jsonl` 再砍一半 ✓。
fn halve_entry(entries: &mut [TarEntry], path: &str) {
    if let Some(entry) = entries.iter_mut().find(|entry| entry.path == path) {
        let text = String::from_utf8_lossy(&entry.bytes).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() > 1 {
            let keep = lines.len() / 2;
            let start = lines.len() - keep;
            entry.bytes = lines[start..].join("\n").into_bytes();
        } else {
            entry.bytes.clear();
        }
    }
}

/// **按固定顺序裁剪**：缩略图 → 原子 → stderr ✓，每一步都记进说明 ✓。
///
/// **为什么顺序固定** ✗：不固定就不可复现 ✓，而"同样的输入裁出不同的包"会让复盘更难 ✓。
fn trim_to_budget(
    entries: &mut [TarEntry],
    atom_lines: &mut Vec<String>,
    notes: &mut Vec<String>,
    trimmed: &mut Vec<String>,
    target: usize,
) {
    let total = |entries: &[TarEntry]| -> usize { entries.iter().map(|e| e.bytes.len()).sum() };
    // ① 缩略图（最大、最不关键 ✓）。
    if total(entries) > target {
        if let Some(entry) = entries
            .iter_mut()
            .find(|entry| entry.path == "thumbnail.bin")
        {
            if !entry.bytes.is_empty() {
                entry.bytes.clear();
                trimmed.push("thumbnail.bin".to_owned());
                notes.push(
                    "为满足体积上限，缩略图**未包含**（thumbnail.json 仍说明它原本存在）。"
                        .to_owned(),
                );
                if let Some(meta) = entries
                    .iter_mut()
                    .find(|entry| entry.path == "thumbnail.json")
                {
                    if let Ok(mut value) = serde_json::from_slice::<Value>(&meta.bytes) {
                        value["included"] = json!(false);
                        value["reason"] = json!("被体积上限裁掉");
                        meta.bytes = serde_json::to_vec_pretty(&value).unwrap_or_default();
                    }
                }
            }
        }
    }
    // ② 原子（从最早的一半开始丢 ✓）。
    let mut guard = 0;
    while total(entries) > target && atom_lines.len() > 1 && guard < 32 {
        guard += 1;
        let keep = atom_lines.len() / 2;
        let dropped = atom_lines.len() - keep;
        atom_lines.drain(..dropped);
        if let Some(entry) = entries.iter_mut().find(|entry| entry.path == "atoms.jsonl") {
            entry.bytes = atom_lines.join("\n").into_bytes();
        }
        // **元数据必须跟着改** ✗：否则 `atoms.meta.json` 会写"包含 1000 条"，
        // 而 `atoms.jsonl` 只剩 400 条 ✓ —— **自述与实物不符**正是这个包要消灭的东西 ✓。
        if let Some(meta) = entries
            .iter_mut()
            .find(|entry| entry.path == "atoms.meta.json")
        {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&meta.bytes) {
                value["included"] = json!(atom_lines.len());
                value["truncated"] = json!(true);
                value["trimmed_by_budget"] = json!(true);
                meta.bytes = serde_json::to_vec_pretty(&value).unwrap_or_default();
            }
        }
        if !trimmed.iter().any(|name| name == "atoms.jsonl") {
            trimmed.push("atoms.jsonl".to_owned());
        }
        notes.push(format!(
            "为满足体积上限，原子日志再丢 {} 条 ⇒ 只剩 {} 条（更早的只能看磁盘上的 atoms.jsonl）。",
            dropped,
            atom_lines.len()
        ));
    }
    // ③ stderr（保留最新的 ✓）。
    guard = 0;
    while total(entries) > target && guard < 32 {
        guard += 1;
        let Some(entry) = entries.iter_mut().find(|entry| entry.path == "stderr.log") else {
            break;
        };
        if entry.bytes.is_empty() {
            break;
        }
        let text = String::from_utf8_lossy(&entry.bytes).into_owned();
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= 1 {
            entry.bytes.clear();
        } else {
            let keep = lines.len() / 2;
            entry.bytes = lines[lines.len() - keep..].join("\n").into_bytes();
        }
        if !trimmed.iter().any(|name| name == "stderr.log") {
            trimmed.push("stderr.log".to_owned());
        }
        // 同样要让 `stderr.meta.json` 承认这次裁剪 ✓（自述与实物必须一致 ✓）。
        if let Some(meta) = entries
            .iter_mut()
            .find(|entry| entry.path == "stderr.meta.json")
        {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&meta.bytes) {
                value["truncated"] = json!(true);
                value["trimmed_by_budget"] = json!(true);
                meta.bytes = serde_json::to_vec_pretty(&value).unwrap_or_default();
            }
        }
        notes.push("为满足体积上限，stderr 只保留最新的一半。".to_owned());
    }
}

/// 生成包内 `README.txt` ✓ —— **一个不在场的人靠它就能看懂这次事故** ✓。
fn build_readme(doc_id: &str, notes: &[String], trimmed: &[String]) -> String {
    let mut out = String::new();
    out.push_str("偃师 Yanshi —— 诊断包（collect_diagnostics）\n");
    out.push_str("================================================\n\n");
    out.push_str(&format!("目标文档：{doc_id}\n"));
    out.push_str(&format!(
        "生成时刻：{} ms（Unix 毫秒）\n\n",
        yanshi_core::now_ms()
    ));
    out.push_str("这个包是什么\n------------\n");
    out.push_str(
        "一次事故的排查材料：构建/平台/配置、文档元数据、原子日志尾部、服务端 stderr 环形缓冲、\n\
         渲染告警（含被跳过的补丁）、阶段耗时、缩略图。**MCP stdio 模式下 stderr 会被客户端丢弃**，\n\
         所以 stderr.log 是那次运行唯一留下的痕迹。\n\n",
    );
    out.push_str("包内每个条目是什么\n------------------\n");
    for (name, description) in ENTRY_DESCRIPTIONS {
        out.push_str(&format!("  {name:<20} {description}\n"));
    }
    out.push('\n');
    out.push_str("上限与裁剪（**不是无界的**）\n----------------------------\n");
    out.push_str(&format!(
        "  未压缩内容上限 {} 字节；zip 上限 {} 字节。\n\
         \x20 stderr 环形缓冲容量 {} 行；原子尾部最多 {} 条；单条原子 {} 字节；缩略图 {} 字节。\n",
        MAX_CONTENT_BYTES,
        MAX_ARCHIVE_BYTES,
        LOG_RING_CAPACITY,
        MAX_ATOM_TAIL,
        MAX_ATOM_LINE_BYTES,
        MAX_THUMBNAIL_BYTES
    ));
    if trimmed.is_empty() {
        out.push_str("  本次没有因为体积上限裁掉任何条目。\n");
    } else {
        out.push_str(&format!(
            "  本次被体积上限裁过的条目：{}\n",
            trimmed.join(", ")
        ));
    }
    out.push('\n');
    out.push_str("本次采集的观察\n--------------\n");
    if notes.is_empty() {
        out.push_str("  （无）\n");
    } else {
        for note in notes {
            for line in note.lines() {
                out.push_str("  * ");
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    out.push('\n');
    out.push_str("采集**到**了什么 / **没**采到什么\n--------------------------------\n");
    out.push_str("  [有] 构建标识、平台、生效配置（已去密）。\n");
    out.push_str(
        "  [有] 文档元数据：doc id、画布尺寸、head seq、原子数、图层/对象计数、渲染水位。\n",
    );
    out.push_str("  [有] 原子日志尾部（超长会截断，见 atoms.meta.json 的 truncated / first_included_seq）。\n");
    out.push_str("  [有] stderr 环形缓冲（固定容量，覆盖掉的行只报数量）。\n");
    out.push_str(
        "  [有] 渲染告警与被跳过的补丁（warnings.json）—— 静默的不完整画面就在这里现形。\n",
    );
    out.push_str("  [有] 最近若干次工具调用的阶段耗时（timings.json）。\n");
    out.push_str("  [有] 当前缩略图（若已在缓存里且不大；否则 thumbnail.json 说明原因）。\n");
    out.push_str("  [无] 进程 fd 2 上第三方/panic 直接写出的原始字节（本 crate 禁止 unsafe，无法接管 stderr）。\n");
    out.push_str("  [无] 已被环形缓冲覆盖掉的更早 stderr；已被截断掉的更早原子。\n");
    out.push_str("  [无] 全量像素/全部 blob —— 需要时用 export_project 单独导出。\n\n");
    out.push_str("隐私\n----\n");
    out.push_str("  所有文本条目都经过去密：命名机密值（token=/key=/Bearer …）、\n\
                  调用方声明的机密串、以及 $HOME / /home/<user> / /Users/<user> 这类绝对家目录路径\n\
                  都会被替换成占位串。洗了多少处见 privacy.json。\n");
    out
}

/// 每个条目的说明（README 与测试共用一份 ✓）。
const ENTRY_DESCRIPTIONS: &[(&str, &str)] = &[
    (
        "README.txt",
        "本文件：包内说明、上限、采到了什么/没采到什么",
    ),
    (
        "build.json",
        "构建标识（版本/commit/构建时间）与平台（os/arch/family）",
    ),
    ("config.json", "生效配置（已去密）"),
    (
        "surface.json",
        "哪个面生成的（mcp/web/tool）与该面的附加事实",
    ),
    (
        "document.json",
        "文档元数据：尺寸、head seq、原子数、图层/对象计数、渲染水位、打开中的文档",
    ),
    (
        "atoms.jsonl",
        "原子日志（JSONL，尾部；超长原子退化为摘要对象）",
    ),
    (
        "atoms.meta.json",
        "原子日志的截断信息（总数、包含数、首个包含的 seq）",
    ),
    (
        "warnings.json",
        "渲染告警与**被跳过的补丁**、以及最近工具结果里的告警",
    ),
    (
        "timings.json",
        "最近若干次工具调用的阶段耗时（prep/raster/dirty/fold/log/other）",
    ),
    (
        "stderr.log",
        "服务端 stderr 的环形缓冲内容（MCP stdio 下唯一的留痕）",
    ),
    (
        "stderr.meta.json",
        "环形缓冲统计（容量、当前行数、被覆盖的行数、截断）",
    ),
    (
        "thumbnail.json",
        "缩略图说明（是否包含、原因、字节数、blob 哈希）",
    ),
    (
        "thumbnail.bin",
        "当前缩略图的 PNG 字节；未包含时为空文件（原因见 thumbnail.json）",
    ),
    ("privacy.json", "去密报告：洗掉了多少处机密与家目录路径"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// **能红**：把 `LogRing::push` 里的 `while … pop_front` 去掉 ⇒ 行数会超过容量 ⇒ 红 ✓。
    #[test]
    fn log_ring_is_bounded_and_keeps_the_newest() {
        let mut ring = LogRing::new(8);
        for index in 0..100 {
            ring.push(format!("line-{index}"));
        }
        assert_eq!(ring.lines.len(), 8, "环形缓冲不得超过容量");
        assert_eq!(ring.dropped, 92, "被覆盖的行数要如实记账");
        let snapshot = ring.snapshot();
        assert_eq!(snapshot.first().map(String::as_str), Some("line-92"));
        assert_eq!(snapshot.last().map(String::as_str), Some("line-99"));
    }

    /// **能红**：把 `Redactor::replace_named_values` 去掉 ⇒ 令牌留在文本里 ⇒ 红 ✓。
    #[test]
    fn redaction_removes_tokens_keys_and_home_paths() {
        let secrets = vec!["S3CRET-TOKEN-VALUE".to_owned()];
        let text = "GET /api/blob/x?doc=d&token=abc123&key=XYZ HTTP/1.1\n\
                    Authorization: Bearer abc123\n\
                    YANSHI_API_KEY=S3CRET-TOKEN-VALUE at /home/alice/private/yanshi";
        let washed = redact_text(text, &secrets);
        assert!(!washed.contains("abc123"), "{washed}");
        assert!(!washed.contains("S3CRET-TOKEN-VALUE"), "{washed}");
        assert!(!washed.contains("/home/alice"), "{washed}");
        assert!(washed.contains(REDACTED), "{washed}");
        assert!(washed.contains('~'), "{washed}");
    }

    /// **能红**：把 `Redactor::value` 里对 `is_secret_key` 的判断去掉 ⇒ 值原样留下 ⇒ 红 ✓。
    #[test]
    fn redaction_replaces_secret_valued_keys_in_json() {
        let secrets: Vec<String> = Vec::new();
        let value = json!({"token": "abc123", "api_key": "k-999", "nested": {"password": "pw"}});
        let washed = redact_value(&value, &secrets);
        assert_eq!(washed["token"], json!(REDACTED));
        assert_eq!(washed["api_key"], json!(REDACTED));
        assert_eq!(washed["nested"]["password"], json!(REDACTED));
    }

    /// **能红**：把 `truncate_chars` 的 `is_char_boundary` 循环去掉 ⇒ 多字节字符会被切断 ⇒ 红 ✓。
    #[test]
    fn truncation_never_splits_utf8() {
        let mut text = "中".repeat(100);
        truncate_chars(&mut text, 31, "…");
        assert!(text.len() <= 31 + "…".len());
        assert!(text.ends_with('…'));
        // 还能当字符串用（没有断裂的 UTF-8 ✓）。
        assert_eq!(text.chars().count(), 10);
    }
}
