//! 文档级 capability token 与会话（设计文档 12.7）。
//!
//! Phase 1 即实现文档级 capability token（随机不可猜 URL）。原子 `actor` 字段与会话认证绑定：
//!
//! - **MCP stdio**：本地进程豁免鉴权。
//! - **HTTP / WebSocket**：走 capability token。
//! - **Token 发放**：打开文档返回内嵌 token 的 URL；后续请求携带 token。
//! - Phase 5 扩展为 owner/editor/viewer 角色权限；这里已预留 [`Role`]。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use yanshi_core::{ActorId, ErrorCode, ErrorContext, Result, SessionId, YanshiError};

/// 令牌长度（十六进制字符数）：64 字符 = 256 bit。
pub const TOKEN_HEX_LEN: usize = 64;

/// 文档级 capability token。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilityToken(String);

impl CapabilityToken {
    /// 生成新令牌。
    ///
    /// 用 OS 熵播种的 `RandomState` 逐段哈希计数器，拼出 256 bit 十六进制串。
    /// Phase 5 会换成 Ed25519 签名令牌（17 章远期项），届时本类型保留为不透明载体。
    pub fn generate() -> Self {
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
            token.as_str().to_owned(),
            Principal {
                actor: actor.into(),
                role,
            },
        );
        token
    }

    /// 吊销令牌。
    pub fn revoke(&mut self, token: &CapabilityToken) -> bool {
        self.tokens.remove(token.as_str()).is_some()
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
        self.tokens.get(token.as_str())
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
        self.tokens.get(token.as_str()).cloned().ok_or_else(|| {
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
