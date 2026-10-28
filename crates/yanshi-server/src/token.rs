//! 文档级 capability token 与会话（设计文档 12.7）。
//!
//! Phase 1 即实现文档级 capability token（随机不可猜 URL）。原子 `actor` 字段与会话认证绑定：
//!
//! - **MCP stdio**：本地进程豁免鉴权。
//! - **HTTP / WebSocket**：走 capability token。
//! - **Token 发放**：打开文档返回内嵌 token 的 URL；后续请求携带 token。
//! - Phase 5 扩展为 owner/editor/viewer 角色权限；这里已预留 [`Role`]。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use yanshi_core::{ActorId, ErrorCode, ErrorContext, Result, SessionId, YanshiError};

/// 令牌长度（十六进制字符数）：64 字符 = 256 bit。
pub const TOKEN_HEX_LEN: usize = 64;

/// 文档级 capability token。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityToken(String);

impl CapabilityToken {
    /// 生成新令牌：**直接读 OS 熵**（32 字节 ⇒ 64 位小写十六进制）✓。
    ///
    /// **为什么不用 `RandomState`**：它把安全性押在 SipHash 的 PRF 性质上 ✗（审计 L-1）⇒
    /// 令牌是能力凭证 ⇒ 必须用标准 CSPRNG ✓（`/dev/urandom` 在 Linux/macOS 都是内核熵池 ✓）。
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_HEX_LEN / 2];
        if !fill_os_entropy(&mut bytes) {
            // **不许静默降级** ✓：熵源不可用时留痕，然后退回旧方案（仍非可预测明文 ✓）。
            eprintln!("偃师：OS 熵源不可用 ⇒ capability token 退回 RandomState 熵；请检查系统熵源");
            return Self::generate_from_random_state();
        }
        let mut out = String::with_capacity(TOKEN_HEX_LEN);
        for byte in bytes {
            out.push_str(&format!("{byte:02x}"));
        }
        Self(out)
    }

    /// 旧方案（`RandomState` 逐段哈希计数器）—— 只在 OS 熵源不可用时兜底 ✓。
    fn generate_from_random_state() -> Self {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hasher};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let mut out = String::with_capacity(TOKEN_HEX_LEN);
        while out.len() < TOKEN_HEX_LEN {
            let state = RandomState::new();
            let mut hasher = state.build_hasher();
            hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
            hasher.write_u64(yanshi_core::now_ms() as u64);
            let chunk = hasher.finish();
            out.push_str(&format!("{chunk:016x}"));
        }
        out.truncate(TOKEN_HEX_LEN);
        Self(out)
    }

    /// 字符串形式。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 从字符串解析（长度与字符集校验）。
    pub fn parse(text: &str) -> Result<Self> {
        let valid = text.len() == TOKEN_HEX_LEN
            && text
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if !valid {
            return Err(YanshiError::new(
                ErrorCode::InvalidArgument,
                ErrorContext::detail("capability token 必须是 64 位小写十六进制"),
            ));
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for CapabilityToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 读 OS 熵源 ✓。失败返回 `false`（调用方退回旧方案并留痕 ✓，不冒充成功 ✗）。
#[cfg(unix)]
fn fill_os_entropy(out: &mut [u8]) -> bool {
    use std::io::Read as _;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(out))
        .is_ok()
}

/// 非 unix 目标没有 `/dev/urandom` ⇒ 走兜底（Windows 若要发版，应改接 `BCryptGenRandom`）。
#[cfg(not(unix))]
fn fill_os_entropy(_out: &mut [u8]) -> bool {
    false
}

/// 角色（Phase 5 细粒度权限；Phase 1 只区分“能改/只读”）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// 只读。
    Viewer,
    /// 可编辑。
    Editor,
    /// 拥有者：可跨 actor 撤销、管理令牌。
    Owner,
}

impl Role {
    /// 是否允许修改文档。
    pub const fn can_edit(self) -> bool {
        matches!(self, Self::Editor | Self::Owner)
    }

    /// 是否允许跨 actor revert（12.5）。
    pub const fn can_revert_others(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
            Self::Owner => "owner",
        }
    }
}

/// **每文档令牌不许越界到别的文档** ✓（第 1440 轮审计 · **单一实现** ✓ ——
/// 工具层 `get_inflight`/`cancel_operation` 与控制面 `control_plane` **都调这一份** ✓，
/// 各写一份必然漂移 ✗ 是本仓反复踩过的老坑 ✓）。
///
/// **洞的形状** ✗：`doc_id` 是这些工具的**入参** ✓ ⇒ 拿着文档 A 的一个 editor 令牌
/// 就能**枚举／打断任意文档**的在飞操作 ✗ —— 12.x 的按文档授权模型在这里被入参绕开 ✗。
/// **规矩不是新发明的** ✓：跨边界（跨 actor ✓、跨文档 ✓）只有 **Owner** ✓ ——
/// 与 [`Role::can_revert_others`]（12.5）同形 ✓。
/// 本地传输（MCP ✓／嵌入式 ✓）的角色缺省就是 `Owner` ⇒ **行为不变** ✓。
pub fn ensure_doc_scope(authorized: &str, role: Role, wanted: &str) -> Result<()> {
    if wanted == authorized || role == Role::Owner {
        return Ok(());
    }
    Err(YanshiError::new(
        ErrorCode::PermissionDenied,
        ErrorContext::detail(format!(
            "本 token 只授权文档「{authorized}」⇒ 跨文档查询／取消在飞操作需要 owner token（请求的是「{wanted}」，当前角色是 {}）",
            role.as_str()
        )),
    ))
}

/// 传输类型：决定鉴权豁免与推送通道分工（6.7 / 12.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportKind {
    /// MCP stdio：本地进程，豁免鉴权，不做推送。
    Stdio,
    /// HTTP：无状态查询与提交，走 token。
    Http,
    /// WebSocket：实时推送，走 token。
    WebSocket,
    /// 进程内调用（测试、嵌入式使用）。
    InProcess,
}

impl TransportKind {
    /// stdio 与进程内调用豁免鉴权（12.7）。
    pub const fn is_local(self) -> bool {
        matches!(self, Self::Stdio | Self::InProcess)
    }

    /// 是否支持服务端推送（仅 WebSocket 服务 Web 客户端）。
    pub const fn supports_push(self) -> bool {
        matches!(self, Self::WebSocket)
    }
}

/// 认证主体。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    /// 绑定的 actor（原子 `actor` 字段）。
    pub actor: ActorId,
    /// 角色。
    pub role: Role,
}

/// 已建立的会话。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// 会话 id。
    pub id: SessionId,
    /// 认证主体。
    pub principal: Principal,
    /// 传输类型。
    pub transport: TransportKind,
}

impl Session {
    /// 新建会话。
    pub fn new(id: impl Into<SessionId>, principal: Principal, transport: TransportKind) -> Self {
        Self {
            id: id.into(),
            principal,
            transport,
        }
    }

    /// actor。
    pub fn actor(&self) -> &str {
        &self.principal.actor
    }

    /// 是否允许修改。
    pub fn can_edit(&self) -> bool {
        self.principal.role.can_edit()
    }
}

/// 令牌表的键：`SHA-256(token)`（64 位小写十六进制）✓（审计 M-2）。
///
/// **为什么键不存令牌原文** ✗：`BTreeMap` 按 `Ord` 查表 ⇒ 比较在**第一个不同的字节**
/// 就返回 ✗ ⇒ 单次查表的耗时随「与真实键共享的前缀长度」变化 ⇒ 逐字节可测的旁路 ✓；
/// 而且明文凭证本不该在服务端内存里多躺一份 ✓。摘要定长 32 字节 ⇒ 一次比较 ✓，
/// 像空间仍是 2^256 ✓（令牌本身就是 OS 熵 ⇒ L-1 ✓）。
fn token_key(token: &str) -> String {
    let mut key = String::with_capacity(TOKEN_HEX_LEN);
    for byte in Sha256::digest(token.as_bytes()) {
        let _ = write!(key, "{byte:02x}");
    }
    key
}

/// 令牌管理器：一个文档一份 capability token 集合。
#[derive(Debug, Default)]
pub struct TokenAuthority {
    tokens: BTreeMap<String, Principal>,
}

impl TokenAuthority {
    /// 空管理器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 发放令牌。
    pub fn issue(&mut self, actor: impl Into<ActorId>, role: Role) -> CapabilityToken {
        let token = CapabilityToken::generate();
        self.tokens.insert(
            token_key(token.as_str()),
            Principal {
                actor: actor.into(),
                role,
            },
        );
        token
    }

    /// 吊销令牌。
    pub fn revoke(&mut self, token: &CapabilityToken) -> bool {
        let key = token_key(token.as_str());
        self.tokens.remove(&key).is_some()
    }

    /// 恢复一个已持久化的令牌（服务端重启，12.7）。
    pub fn restore(&mut self, token: &str, actor: &str, role: Role) -> Result<()> {
        let token = CapabilityToken::parse(token)?;
        self.tokens.insert(
            token_key(token.as_str()),
            Principal {
                actor: actor.to_owned(),
                role,
            },
        );
        Ok(())
    }

    /// 已发放令牌数。
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// 是否没有令牌。
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// 解析令牌对应的主体（不校验传输）。
    pub fn principal_of(&self, token: &CapabilityToken) -> Option<&Principal> {
        self.tokens.get(&token_key(token.as_str()))
    }

    /// 鉴权：本地传输豁免；其他传输必须提供有效令牌。
    pub fn authorize(
        &self,
        token: Option<&CapabilityToken>,
        transport: TransportKind,
        fallback_actor: &str,
    ) -> Result<Principal> {
        if transport.is_local() {
            return Ok(Principal {
                actor: fallback_actor.to_owned(),
                role: Role::Owner,
            });
        }
        let Some(token) = token else {
            return Err(YanshiError::new(
                ErrorCode::PermissionDenied,
                ErrorContext::detail("缺少 capability token"),
            ));
        };
        self.tokens
            .get(&token_key(token.as_str()))
            .cloned()
            .ok_or_else(|| {
                YanshiError::new(
                    ErrorCode::PermissionDenied,
                    ErrorContext::detail("capability token 无效或已吊销"),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn os_entropy_really_reads_the_kernel_pool() {
        // 变异：让 fill_os_entropy 写全 0 ⇒ 本测试红（两次相同），令牌唯一性测试也红 ✓。
        let mut first = [0u8; TOKEN_HEX_LEN / 2];
        let mut second = [0u8; TOKEN_HEX_LEN / 2];
        assert!(fill_os_entropy(&mut first), "/dev/urandom 必须可读");
        assert!(fill_os_entropy(&mut second));
        assert_ne!(first, second, "两次读取必须不同 ⇒ 熵源真的在读内核");
        assert!(first.iter().any(|byte| *byte != 0), "不许是全 0 占位");
    }

    #[test]
    fn generated_tokens_are_unique_and_well_formed() {
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..64 {
            let token = CapabilityToken::generate();
            assert_eq!(token.as_str().len(), TOKEN_HEX_LEN);
            assert!(token
                .as_str()
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
            assert!(seen.insert(token.as_str().to_owned()), "令牌必须唯一");
            assert!(CapabilityToken::parse(token.as_str()).is_ok());
        }
        assert!(CapabilityToken::parse("short").is_err());
        assert!(
            CapabilityToken::parse(&"A".repeat(TOKEN_HEX_LEN)).is_err(),
            "必须小写"
        );
    }

    #[test]
    fn local_transports_are_exempt_and_remote_require_tokens() {
        let mut authority = TokenAuthority::new();
        let token = authority.issue("human:1", Role::Editor);

        // stdio / 进程内：豁免鉴权，按 owner 处理。
        let principal = authority
            .authorize(None, TransportKind::Stdio, "local:stdio")
            .unwrap();
        assert_eq!(principal.actor, "local:stdio");
        assert_eq!(principal.role, Role::Owner);
        assert_eq!(
            authority
                .authorize(None, TransportKind::InProcess, "embedded")
                .unwrap()
                .actor,
            "embedded"
        );

        // HTTP / WS：必须带有效令牌。
        let error = authority
            .authorize(None, TransportKind::Http, "x")
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        let principal = authority
            .authorize(Some(&token), TransportKind::WebSocket, "x")
            .unwrap();
        assert_eq!(principal.actor, "human:1");
        assert!(principal.role.can_edit());

        // 无效令牌。
        let bogus = CapabilityToken::generate();
        assert_eq!(
            authority
                .authorize(Some(&bogus), TransportKind::Http, "x")
                .unwrap_err()
                .code,
            ErrorCode::PermissionDenied
        );

        // 吊销后失效。
        assert!(authority.revoke(&token));
        assert_eq!(authority.len(), 0);
        assert!(authority
            .authorize(Some(&token), TransportKind::Http, "x")
            .is_err());
    }

    #[test]
    fn token_table_is_keyed_by_digest_not_by_plaintext() {
        // 变异：让 `token_key` 直接返回令牌原文 ⇒ 第一条断言红 ✓（审计 M-2）。
        let mut authority = TokenAuthority::new();
        let token = authority.issue("human:2", Role::Editor);
        assert_eq!(authority.len(), 1);
        assert!(
            !authority.tokens.contains_key(token.as_str()),
            "表里不许出现令牌原文（泄漏内存即泄漏凭证）"
        );
        let key = authority.tokens.keys().next().unwrap().clone();
        assert_eq!(key.len(), TOKEN_HEX_LEN);
        assert!(key
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
        assert_eq!(
            key,
            token_key(token.as_str()),
            "键必须是 SHA-256 的十六进制"
        );

        // 查表／吊销／恢复仍按令牌原文工作 ✓（键换了，语义不变 ✓）。
        assert_eq!(authority.principal_of(&token).unwrap().actor, "human:2");
        assert_eq!(
            authority
                .authorize(Some(&token), TransportKind::Http, "x")
                .unwrap()
                .role,
            Role::Editor
        );
        assert!(authority.revoke(&token));
        assert!(authority.principal_of(&token).is_none());
        assert!(authority
            .authorize(Some(&token), TransportKind::Http, "x")
            .is_err());
        authority
            .restore(token.as_str(), "human:2", Role::Viewer)
            .unwrap();
        assert!(!authority.tokens.contains_key(token.as_str()));
        assert_eq!(authority.principal_of(&token).unwrap().role, Role::Viewer);
    }

    #[test]
    fn roles_shape_permissions() {
        assert!(!Role::Viewer.can_edit());
        assert!(Role::Editor.can_edit());
        assert!(!Role::Editor.can_revert_others());
        assert!(Role::Owner.can_revert_others());
        assert_eq!(Role::Owner.as_str(), "owner");
    }

    #[test]
    fn push_is_only_for_websocket() {
        assert!(TransportKind::WebSocket.supports_push());
        assert!(!TransportKind::Stdio.supports_push());
        assert!(!TransportKind::Http.supports_push());
        assert!(TransportKind::Stdio.is_local());
        assert!(!TransportKind::WebSocket.is_local());
    }
}
