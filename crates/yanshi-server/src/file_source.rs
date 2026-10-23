//! **文件来源解析**（设计文档《RAW 导入支持：需求与详细设计》第 4 章的落地，单一来源）。
//!
//! 本模块只做一件事：把调用方给的文件字节弄到手。各工具不再各自读文件、
//! 各自拼错误——三条通道（本地 `path`／http(s) URL／`data_base64`）在这里收口。
//! 规则全部来自设计文档第 4 章（定版），逐条对应：
//!
//! - **平铺二选一**（4.2）：`path` 与 `data_base64` 同给 ⇒ `InvalidArgument`，
//!   不做静默优先级；与工具既有的 blob 来源构成三来源互斥时，判定也在本模块
//!   （[`check_exclusive`]），文案全仓一份。
//! - **`path` 双形态**（4.2）：`http://`／`https://` 开头 ⇒ URL 拉取（[`fetch_url`]）；
//!   其余 scheme 的 `://` 一律拒绝（本地文件走路径形态，不要用 `file://` 绕）；
//!   再其余就是本地路径，沙箱沿用 `put_blob` 已落地的口径（拒 `..` 组件 +
//!   canonicalize 后必须落在工作区 root 内），实现只此一份（[`resolve_local_path`]）。
//! - **体积上限同源**（4.3）：[`max_upload_bytes`] 是全仓唯一出处——缺省 64MiB，
//!   可用环境变量 `YANSHI_MAX_UPLOAD_BYTES` 覆盖；`yanshi-http` 的 `blob_upload`
//!   也调它，不在两处各写一份。三条通道都受它约束。
//! - **URL 拉取细则**（4.3）：协议白名单只有 http/https；连接超时
//!   [`CONNECT_TIMEOUT`]（10s）、总超时 [`TOTAL_TIMEOUT`]（30s）；重定向最多
//!   [`MAX_REDIRECTS`]（5）跳且禁止 https→http 降级；响应体积边读边计数、
//!   超限即断；`Content-Type` 须为 `image/*` 或 `application/octet-stream`，
//!   且入库前以 magic bytes 复核真实格式（CT 与 magic 不符以 magic 为准，
//!   在产物里如实标注）；五类失败（连接／状态码／超时／超限／类型）分开报错，
//!   文案都带 URL 与类别词，不许合并成一句「下载失败」。
//!
//! 信任模型（4.3/4.4）：本地 MCP 场景下调用者即机器所有者，path/URL 都是
//! 调用者对自己资源的引用 ⇒ 不做内网地址封禁（会误伤正当的局域网素材源）；
//! 上面的超时／体积／降级三条是防失控与事故的最小防护，不是防调用者本人。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use yanshi_core::{ErrorCode, ErrorContext, Result, YanshiError};

/// 上传／拉取的体积上限缺省值（64MiB）——与 `yanshi-http` 的 `blob_upload` 同源。
pub const DEFAULT_MAX_UPLOAD_BYTES: usize = 64 * 1024 * 1024;

/// 全仓唯一的体积上限出处：环境变量 `YANSHI_MAX_UPLOAD_BYTES` 覆盖，缺省
/// [`DEFAULT_MAX_UPLOAD_BYTES`]。解析不出的值按缺省处理（与 HTTP 上传端点
/// 原有口径逐字一致——那份实现已改为调本函数，不再各写一份）。
pub fn max_upload_bytes() -> usize {
    std::env::var("YANSHI_MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MAX_UPLOAD_BYTES)
}

/// URL 拉取的连接超时（设计 4.3 定版 10s，常量不许各处另写）。
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// URL 拉取的总超时（设计 4.3 定版 30s，含读完响应体）。
pub const TOTAL_TIMEOUT: Duration = Duration::from_secs(30);

/// URL 拉取最多跟随的重定向跳数（设计 4.3 定版 5 跳）。
pub const MAX_REDIRECTS: u32 = 5;

/// 一条通道的种类（回显 `source` 字段用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// 本地文件路径。
    Path,
    /// http(s) URL 拉取。
    Url,
    /// 内嵌 base64 正文。
    Base64,
}

impl SourceKind {
    /// 回显用的字面量。
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Path => "path",
            SourceKind::Url => "url",
            SourceKind::Base64 => "base64",
        }
    }
}

/// URL 拉取成功后的产物：字节 + 回显要用的事实（最终 URL、声明类型、magic 实测格式）。
#[derive(Debug)]
pub struct FetchedUrl {
    /// 响应体字节（已过体积上限校验）。
    pub bytes: Vec<u8>,
    /// 跟随重定向后的最终 URL。
    pub final_url: String,
    /// 响应声明的 `Content-Type`（原样，可能为 None）。
    pub content_type: Option<String>,
    /// magic bytes 实测到的格式（认不出为 None）。
    pub detected_format: Option<&'static str>,
    /// 声明类型与实测格式不一致（以实测为准，仅标注，不拒绝）。
    pub content_type_mismatch: bool,
}

/// `path` 参数是不是 http(s) URL 形态（设计 4.2：形态由前缀判定）。
pub fn is_http_url(text: &str) -> bool {
    text.starts_with("http://") || text.starts_with("https://")
}

/// 取 `scheme://` 里的 scheme（没有 `://` 时为 None）——用来拒绝 `file://` 等
/// 想绕过本地路径形态的写法（设计 4.3 协议白名单）。
pub fn url_scheme(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once("://")?;
    if !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        Some(scheme)
    } else {
        None
    }
}

/// 三来源互斥判定（设计 4.2 定版）：`given` 是本次实际给了的来源名
/// （如 `["path", "data_base64"]`），多于一个 ⇒ `InvalidArgument`，
/// 文案逐项列出给了哪些、只能给一个。不存在「谁优先」——同给就是错。
pub fn check_exclusive(given: &[&str]) -> Result<()> {
    if given.len() > 1 {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "来源互斥：{} 只能给一个（本次给了 {} 个：{}）",
                given.join(" / "),
                given.len(),
                given.join("、"),
            )),
        ));
    }
    Ok(())
}

/// 体积上限校验（设计 4.3/4.4）：超限 ⇒ `ResourceExhausted`，文案与 HTTP
/// 上传端点的 413 同结构（「…字节超过上限 …」）。
pub fn check_size(what: &str, len: usize, cap: usize) -> Result<()> {
    if len > cap {
        return Err(YanshiError::new(
            ErrorCode::ResourceExhausted,
            ErrorContext::detail(format!("{what} {len} 字节超过上限 {cap}")),
        ));
    }
    Ok(())
}

/// 解析 `data_base64` 参数：纯 base64 正文（设计 4.2：不带 `data:` 前缀，
/// 带了明确报错并指明去掉前缀）；解码失败 ⇒ `InvalidArgument` 并说清原因。
/// 解出的字节同样过体积上限。
pub fn decode_base64_param(args: &Value, key: &str, cap: usize) -> Result<Option<Vec<u8>>> {
    let Some(text) = args.get(key).and_then(Value::as_str) else {
        return Ok(None);
    };
    if text.starts_with("data:") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!(
                "{key} 只收纯 base64 正文：请去掉开头的 `data:…;base64,` 前缀再传"
            )),
        ));
    }
    let bytes = crate::base64::decode(text).ok_or_else(|| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("{key} 不是合法 base64（标准字母表 + `=` 填充）")),
        )
    })?;
    check_size(key, bytes.len(), cap)?;
    Ok(Some(bytes))
}

/// 本地路径解析（设计 4.4：沿用 `put_blob` 第 1230/1232 轮已落地的沙箱，
/// 本函数是那套口径的唯一实现，`put_blob` 与后续工具共用）：
/// 拒 `..` 组件 ⇒ canonicalize ⇒ 必须落在工作区 root 之内 ⇒ 读字节并过上限。
/// 相对路径相对 root 解析（工具文档口径：相对 --root），不相对进程 CWD——
/// 否则 root 内的文件读不到、CWD 下的同名文件会被误读（2026-10-23 实测缺陷）。
/// 无 root 的工作区不支持 path 形态，明确报错（不静默回退到别处读）。
pub fn resolve_local_path(
    raw: &str,
    root: Option<&Path>,
    cap: usize,
) -> Result<(Vec<u8>, PathBuf)> {
    if raw.split(['/', '\\']).any(|seg| seg == "..") {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("path 不许含 `..` 组件（防路径穿越）：{raw}")),
        ));
    }
    // 相对路径先拼 root 再 canonicalize：canonicalize 本身按进程 CWD 解析，
    // 不拼 root 则与文档口径（相对 --root）不符（见函数头注）。
    let candidate = match root {
        Some(root) if !Path::new(raw).is_absolute() => root.join(raw),
        _ => PathBuf::from(raw),
    };
    let resolved = std::fs::canonicalize(&candidate).map_err(|e| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("读不到 {raw}：{e}")),
        )
    })?;
    let inside_root = match root {
        Some(root) => std::fs::canonicalize(root)
            .map(|r| resolved.starts_with(&r))
            .unwrap_or(false),
        None => false,
    };
    if !inside_root {
        // **两种情形说两句不同的话** ✓（外部第 10 轮实测：给了 `--root` 却被告知"无根工作区" ✗，
        // 而且末尾把同一个路径打印两遍 `A ⇒ A` ✗ ⇒ 读起来像"从哪到哪的转换失败" ✓）。
        let detail = match root {
            Some(root) => format!(
                "path 必须落在工作区根目录之内：当前 root={} ⇒ 收不到 {}（把文件放到 root 之下再传）",
                root.display(),
                resolved.display()
            ),
            None => format!(
                "这个工作区没有根目录（以 `--root` 启动才有）⇒ 不接受 path 形态：{raw} \
                 ⇒ 改用 base64 传字节，或让服务端带 --root 启动"
            ),
        };
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(detail),
        ));
    }
    let bytes = std::fs::read(&resolved).map_err(|e| {
        YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("读不到 {raw}：{e}")),
        )
    })?;
    check_size("文件", bytes.len(), cap)?;
    Ok((bytes, resolved))
}

/// magic bytes 认格式（设计 4.3 的复核口径）：只认入库链真能消费的签名，
/// 认不出返回 None（由调用方按「声明类型 vs 实测」规则处置，不在这里报错）。
pub fn detect_format(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        b if b.starts_with(b"\x89PNG\r\n\x1a\n") => Some("png"),
        b if b.starts_with(b"\xff\xd8\xff") => Some("jpeg"),
        b if b.starts_with(b"GIF8") => Some("gif"),
        b if b.starts_with(b"BM") => Some("bmp"),
        b if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" => Some("webp"),
        // TIFF 家族：DNG/CR2/NEF/ORF/RW2 都是它（RAW 的容器签名）。
        b if b.starts_with(b"II*\x00") || b.starts_with(b"MM\x00*") => Some("tiff"),
        b if b.starts_with(b"8BPS") => Some("psd"),
        _ => None,
    }
}

/// `Content-Type` 与 magic 实测是否相符（设计 4.3）：声明 `image/png` 而
/// 实测 jpeg ⇒ 不符（以实测为准）；声明 `application/octet-stream` 不参与比对。
fn content_type_mismatch(content_type: Option<&str>, detected: Option<&str>) -> bool {
    let Some(ct) = content_type else {
        return false;
    };
    let ct = ct
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let Some(detected) = detected else {
        return false;
    };
    let declared = match ct.as_str() {
        "image/png" => Some("png"),
        "image/jpeg" | "image/jpg" => Some("jpeg"),
        "image/gif" => Some("gif"),
        "image/bmp" | "image/x-ms-bmp" => Some("bmp"),
        "image/webp" => Some("webp"),
        "image/tiff" => Some("tiff"),
        "image/vnd.adobe.photoshop" => Some("psd"),
        _ => None,
    };
    matches!(declared, Some(d) if d != detected)
}

/// 把一个重定向的 `Location` 拼成下一个完整 URL（相对引用按 RFC 3986 常见
/// 形态处理：绝对 URL／协议相对／绝对路径／相对路径／仅查询）。拼不出返回 None。
fn resolve_location(current: &str, location: &str) -> Option<String> {
    if is_http_url(location) {
        return Some(location.to_owned());
    }
    let (scheme, rest) = current.split_once("://")?;
    let (authority, _path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if let Some(stripped) = location.strip_prefix("//") {
        return Some(format!("{scheme}://{stripped}"));
    }
    if location.starts_with('/') {
        return Some(format!("{scheme}://{authority}{location}"));
    }
    // 相对路径：以当前路径的目录为基底合并（去掉最后一段后拼接）。
    let base_dir = match _path.rfind('/') {
        Some(idx) => &_path[..=idx],
        None => "/",
    };
    Some(format!("{scheme}://{authority}{base_dir}{location}"))
}

/// https→http 是降级（设计 4.3：不跟随、直接报错）；反向与同级都放行。
fn is_downgrade(from: &str, to: &str) -> bool {
    from.starts_with("https://") && to.starts_with("http://")
}

/// 按设计 4.3 的定版常量拉取一个 URL（见 [`fetch_url_with`]）。
pub fn fetch_url(url: &str) -> Result<FetchedUrl> {
    fetch_url_with(url, max_upload_bytes(), CONNECT_TIMEOUT, TOTAL_TIMEOUT)
}

/// [`fetch_url`] 的可调版本：超时做成参数只是为了让判据能在毫秒级造出
/// 超时现场（定版常量仍是 [`CONNECT_TIMEOUT`]／[`TOTAL_TIMEOUT`]，
/// 工具层只调 [`fetch_url`]）。
pub fn fetch_url_with(
    url: &str,
    cap: usize,
    connect_timeout: Duration,
    total_timeout: Duration,
) -> Result<FetchedUrl> {
    if let Some(scheme) = url_scheme(url) {
        if scheme != "http" && scheme != "https" {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "拉取失败（协议）：只接受 http:// 与 https://，本次是 {scheme}://（本地文件请走 path 的路径形态，不要用 URL 绕）"
                )),
            ));
        }
    } else {
        return Err(YanshiError::new(
            ErrorCode::InvalidArgument,
            ErrorContext::detail(format!("拉取失败（协议）：URL 形态不正确：{url}")),
        ));
    }
    // **手动跟随重定向**：ureq 自带跟随没法逐跳执行「禁 https→http 降级」，
    // 所以关掉自动跟随（max_redirects=0 且不把 3xx 当错误），自己一跳一跳走，
    // 每一跳都重过协议白名单与降级检查。状态码也不让 ureq 翻译成错误——
    // 五类失败要在这里分开报，文案自己写。
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(connect_timeout))
        .timeout_global(Some(total_timeout))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .http_status_as_error(false)
        .build()
        .into();
    let mut current = url.to_owned();
    let mut hops = 0u32;
    loop {
        let mut response = match agent.get(&current).call() {
            Ok(response) => response,
            Err(error) => {
                return Err(map_fetch_error(
                    &current,
                    &error,
                    connect_timeout,
                    total_timeout,
                ))
            }
        };
        let status = response.status().as_u16();
        if (300..400).contains(&status) {
            let location = response
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let Some(location) = location else {
                return Err(YanshiError::new(
                    ErrorCode::Degraded,
                    ErrorContext::detail(format!(
                        "拉取失败（HTTP 状态）：{current} ⇒ HTTP {status}（重定向但没有 Location）"
                    )),
                ));
            };
            hops += 1;
            if hops > MAX_REDIRECTS {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "拉取失败（重定向）：{url} 跟随超过 {MAX_REDIRECTS} 跳"
                    )),
                ));
            }
            let next = resolve_location(&current, &location).ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "拉取失败（重定向）：{current} 的 Location 无法解析：{location}"
                    )),
                )
            })?;
            if !is_http_url(&next) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "拉取失败（重定向）：{current} ⇒ {next} 不是 http(s)，已拒绝跟随"
                    )),
                ));
            }
            if is_downgrade(&current, &next) {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "拉取失败（重定向）：{current} ⇒ {next} 是 https→http 降级，已拒绝跟随"
                    )),
                ));
            }
            current = next;
            continue;
        }
        if !(200..300).contains(&status) {
            return Err(YanshiError::new(
                ErrorCode::Degraded,
                ErrorContext::detail(format!("拉取失败（HTTP 状态）：{current} ⇒ HTTP {status}")),
            ));
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        // **类型第一道筛子**（设计 4.3）：声明类型必须是 image/* 或
        // application/octet-stream，其余直接拒（还没读字节就拒，省流量）。
        if let Some(ct) = content_type.as_deref() {
            let base = ct
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if !base.starts_with("image/") && base != "application/octet-stream" {
                return Err(YanshiError::new(
                    ErrorCode::InvalidArgument,
                    ErrorContext::detail(format!(
                        "拉取失败（类型）：{current} 的 Content-Type 是 {ct}，只接受 image/* 或 application/octet-stream"
                    )),
                ));
            }
        }
        // **体积第二道筛子**：Content-Length 先判（有的话），再边读边计数、
        // 超限即断（设计 4.3：无 Content-Length 时靠计数兜底）。
        if let Some(len) = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<usize>().ok())
        {
            if len > cap {
                return Err(YanshiError::new(
                    ErrorCode::ResourceExhausted,
                    ErrorContext::detail(format!(
                        "拉取失败（超限）：{current} 声明 {len} 字节，超过上限 {cap}"
                    )),
                ));
            }
        }
        let mut bytes = Vec::new();
        let mut reader = response.body_mut().as_reader();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    bytes.extend_from_slice(&chunk[..n]);
                    if bytes.len() > cap {
                        return Err(YanshiError::new(
                            ErrorCode::ResourceExhausted,
                            ErrorContext::detail(format!(
                                "拉取失败（超限）：{current} 的响应超过上限 {cap} 字节（已断开）"
                            )),
                        ));
                    }
                }
                Err(error) => {
                    return Err(YanshiError::new(
                        ErrorCode::Degraded,
                        ErrorContext::detail(format!(
                            "拉取失败（连接）：{current} 读取中断 ⇒ {error}"
                        )),
                    ));
                }
            }
        }
        // **magic 复核**（设计 4.3）：声明 image/* 却认不出任何已知图片签名 ⇒
        // 类型不符拒收；认得出但与声明不符 ⇒ 以实测为准、只标注。
        // application/octet-stream 是「调用方自称不透明字节」，不做硬拒
        // （put_blob 是通用入库端，PSD 等字节也从这条路走，签名表认得出就标注）。
        let detected = detect_format(&bytes);
        let declared_image = content_type
            .as_deref()
            .map(|ct| {
                ct.split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with("image/")
            })
            .unwrap_or(false);
        if declared_image && detected.is_none() {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail(format!(
                    "拉取失败（类型）：{current} 声明了图片类型，但字节的 magic 不是任何已知图片格式"
                )),
            ));
        }
        let mismatch = content_type_mismatch(content_type.as_deref(), detected);
        return Ok(FetchedUrl {
            bytes,
            final_url: current,
            content_type,
            detected_format: detected,
            content_type_mismatch: mismatch,
        });
    }
}

/// ureq 的错误翻成五类里的「连接／超时」两类（其余类在调用点就地判）。
/// 超时优先认 `Error::Timeout`；DNS／连接／协议／TLS 都归连接类，文案带原文。
fn map_fetch_error(
    url: &str,
    error: &ureq::Error,
    connect_timeout: Duration,
    total_timeout: Duration,
) -> YanshiError {
    match error {
        ureq::Error::Timeout(kind) => YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!(
                "拉取失败（超时）：{url} 在 {kind} 阶段超过时限（连接 {}s／总 {}s）",
                connect_timeout.as_secs(),
                total_timeout.as_secs(),
            )),
        ),
        other => YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail(format!("拉取失败（连接）：{url} ⇒ {other}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_gate_only_accepts_http_and_https() {
        assert!(is_http_url("http://a/b.png"));
        assert!(is_http_url("https://a/b.png"));
        assert!(!is_http_url("file:///a/b.png"));
        assert_eq!(url_scheme("file:///a/b.png"), Some("file"));
        assert_eq!(url_scheme("/plain/path.png"), None);
        assert_eq!(url_scheme("C:\\win\\path.png"), None);
    }

    #[test]
    fn relative_path_resolves_against_root_not_cwd() {
        // 回归（2026-10-23 实测）：相对路径必须相对 root 解析，
        // 进程 CWD 下的同名文件不得被误读。
        let root =
            std::env::temp_dir().join(format!("yanshi-file-source-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("pic.bin"), b"from-root").unwrap();
        // **诱饵文件名必须唯一** ✓：写进进程 CWD（本仓根）时不能撞上真实文件 ✓
        let decoy = std::env::current_dir()
            .unwrap()
            .join(format!("yanshi_cwd_decoy_{}.bin", std::process::id()));
        let decoy_preexisting = decoy.exists();
        if !decoy_preexisting {
            std::fs::write(&decoy, b"from-cwd").unwrap();
        }
        let (bytes, resolved) =
            resolve_local_path("pic.bin", Some(&root), 1024).expect("root 内相对路径应可读");
        assert_eq!(bytes, b"from-root");
        assert!(resolved.starts_with(std::fs::canonicalize(&root).unwrap()));
        if !decoy_preexisting {
            let _ = std::fs::remove_file(&decoy);
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn exclusive_sources_error_when_more_than_one() {
        assert!(check_exclusive(&["path"]).is_ok());
        assert!(check_exclusive(&[]).is_ok());
        let err = check_exclusive(&["path", "data_base64"]).unwrap_err();
        assert!(format!("{err}").contains("只能给一个"), "{err}");
    }

    #[test]
    fn redirect_locations_resolve_against_current_url() {
        let cur = "http://h/a/b/img.png";
        assert_eq!(
            resolve_location(cur, "http://x/y.png").as_deref(),
            Some("http://x/y.png")
        );
        assert_eq!(
            resolve_location(cur, "//x/y.png").as_deref(),
            Some("http://x/y.png")
        );
        assert_eq!(
            resolve_location(cur, "/root.png").as_deref(),
            Some("http://h/root.png")
        );
        assert_eq!(
            resolve_location(cur, "next.png").as_deref(),
            Some("http://h/a/b/next.png")
        );
    }

    #[test]
    fn downgrade_is_only_https_to_http() {
        assert!(is_downgrade("https://h/a", "http://h/a"));
        assert!(!is_downgrade("http://h/a", "https://h/a"));
        assert!(!is_downgrade("https://h/a", "https://h/b"));
        assert!(!is_downgrade("http://h/a", "http://h/b"));
    }

    #[test]
    fn magic_detection_covers_the_stored_formats() {
        assert_eq!(detect_format(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(detect_format(b"\xff\xd8\xff\xe0rest"), Some("jpeg"));
        assert_eq!(detect_format(b"II*\x00rest"), Some("tiff"));
        assert_eq!(detect_format(b"MM\x00*rest"), Some("tiff"));
        assert_eq!(detect_format(b"8BPSrest"), Some("psd"));
        assert_eq!(detect_format(b"plain text"), None);
    }

    #[test]
    fn content_type_mismatch_compares_declared_against_magic() {
        assert!(content_type_mismatch(Some("image/png"), Some("jpeg")));
        assert!(!content_type_mismatch(Some("image/png"), Some("png")));
        assert!(!content_type_mismatch(
            Some("application/octet-stream"),
            Some("jpeg")
        ));
        assert!(!content_type_mismatch(None, Some("jpeg")));
        assert!(content_type_mismatch(
            Some("image/jpeg; charset=binary"),
            Some("png")
        ));
    }
}
