//! 统一错误协议（设计文档 5.7）。
//!
//! 所有 API 返回同一 schema：
//!
//! ```json
//! {
//!   "ok": false,
//!   "error_code": "reference_not_found",
//!   "retryable": true,
//!   "context": {
//!     "atom_id": "sha256:...",
//!     "object_id": "...",
//!     "layer_id": "...",
//!     "blob_hash": "sha256:...",
//!     "detail": "对象 S 在当前 HEAD 中不存在"
//!   }
//! }
//! ```
//!
//! `conflict` 仅用于采样性替换操作（retouch、inpaint、heal、patch）的并发互踩；
//! 生成性叠加（draw_stroke 等）默认 LWW 自然叠加，不报 `conflict`。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// 错误码枚举（设计文档 5.7）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// 引用的原子 / 对象 / 图层 / blob 不存在。
    ReferenceNotFound,
    /// 参数非法。
    InvalidArgument,
    /// 提交时 precondition 校验失败，或引用已被 tombstone。
    PreconditionFailed,
    /// 采样性替换并发互踩（仅 retouch / inpaint / heal / patch）。
    Conflict,
    /// 权限不足（含跨 actor revert）。
    PermissionDenied,
    /// 资源超限。
    ResourceExhausted,
    /// 服务降级。
    Degraded,
    /// 异步 job 尚未完成。
    JobPending,
    /// 异步 job 不存在或已被 TTL 清理。
    JobNotFound,
}

impl ErrorCode {
    /// 返回 schema 中使用的 snake_case 字面量。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReferenceNotFound => "reference_not_found",
            Self::InvalidArgument => "invalid_argument",
            Self::PreconditionFailed => "precondition_failed",
            Self::Conflict => "conflict",
            Self::PermissionDenied => "permission_denied",
            Self::ResourceExhausted => "resource_exhausted",
            Self::Degraded => "degraded",
            Self::JobPending => "job_pending",
            Self::JobNotFound => "job_not_found",
        }
    }

    /// 该错误码的默认可重试性。
    ///
    /// 可重试：冲突（客户端换 ULID 重提交）、资源超限、降级、job 未完成。
    /// 不可重试：引用缺失、参数非法、precondition 失败、权限不足、job 不存在。
    pub const fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::Conflict | Self::ResourceExhausted | Self::Degraded | Self::JobPending
        )
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 错误上下文，字段与设计文档 5.7 一一对应。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorContext {
    /// 引发错误的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_id: Option<String>,
    /// 引发错误的对象。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    /// 引发错误的图层。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    /// 引发错误的 blob。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_hash: Option<String>,
    /// 人类可读细节。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 其他结构化补充信息（如冲突对方原子、冲突图层）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl ErrorContext {
    /// 以 detail 构造上下文。
    pub fn detail(detail: impl Into<String>) -> Self {
        Self {
            detail: Some(detail.into()),
            ..Self::default()
        }
    }
}

/// 引擎统一错误类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {}", .context.detail.as_deref().unwrap_or(""))]
pub struct YanshiError {
    /// 错误码。
    pub code: ErrorCode,
    /// 是否可重试。
    pub retryable: bool,
    /// 错误上下文。
    pub context: ErrorContext,
}

impl YanshiError {
    /// 用错误码与上下文构造，`retryable` 取该错误码的默认值。
    pub fn new(code: ErrorCode, context: ErrorContext) -> Self {
        Self {
            code,
            retryable: code.default_retryable(),
            context,
        }
    }

    /// 显式覆盖 `retryable`。
    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// 附加对象 id。
    pub fn with_object(mut self, object_id: impl Into<String>) -> Self {
        self.context.object_id = Some(object_id.into());
        self
    }

    /// 附加图层 id。
    pub fn with_layer(mut self, layer_id: impl Into<String>) -> Self {
        self.context.layer_id = Some(layer_id.into());
        self
    }

    /// 附加原子 id。
    pub fn with_atom(mut self, atom_id: impl Into<String>) -> Self {
        self.context.atom_id = Some(atom_id.into());
        self
    }

    /// 附加 blob hash。
    pub fn with_blob(mut self, blob_hash: impl Into<String>) -> Self {
        self.context.blob_hash = Some(blob_hash.into());
        self
    }

    /// 附加结构化补充信息。
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.context.extra.insert(key.into(), value);
        self
    }

    /// 渲染为设计文档 5.7 定义的响应 JSON。
    pub fn to_response(&self) -> serde_json::Value {
        serde_json::json!({
            "ok": false,
            "error_code": self.code.as_str(),
            "retryable": self.retryable,
            "context": self.context,
        })
    }
}

/// 便捷构造宏：`err!(PreconditionFailed, "detail", object_id = id)`。
#[macro_export]
macro_rules! yanshi_err {
    ($code:ident, $detail:expr $(, $field:ident = $value:expr)* $(,)?) => {{
        #[allow(unused_mut)]
        let mut e = $crate::error::YanshiError::new(
            $crate::error::ErrorCode::$code,
            $crate::error::ErrorContext::detail($detail),
        );
        $(
            e = e.$field($value);
        )*
        e
    }};
}

/// 引擎统一 `Result`。
pub type Result<T> = std::result::Result<T, YanshiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_literals_match_schema() {
        assert_eq!(ErrorCode::ReferenceNotFound.as_str(), "reference_not_found");
        assert_eq!(ErrorCode::InvalidArgument.as_str(), "invalid_argument");
        assert_eq!(ErrorCode::PreconditionFailed.as_str(), "precondition_failed");
        assert_eq!(ErrorCode::Conflict.as_str(), "conflict");
        assert_eq!(ErrorCode::PermissionDenied.as_str(), "permission_denied");
        assert_eq!(ErrorCode::ResourceExhausted.as_str(), "resource_exhausted");
        assert_eq!(ErrorCode::Degraded.as_str(), "degraded");
        assert_eq!(ErrorCode::JobPending.as_str(), "job_pending");
        assert_eq!(ErrorCode::JobNotFound.as_str(), "job_not_found");
    }

    #[test]
    fn error_response_matches_document_schema() {
        let err = YanshiError::new(
            ErrorCode::PreconditionFailed,
            ErrorContext::detail("对象 S 在当前 HEAD 中不存在"),
        )
        .with_object("obj_1")
        .with_atom("01ARZ3NDEKTSV4RRFFQ69G5FAV");

        let json = err.to_response();
        assert_eq!(json["ok"], serde_json::json!(false));
        assert_eq!(json["error_code"], "precondition_failed");
        assert_eq!(json["retryable"], false);
        assert_eq!(json["context"]["object_id"], "obj_1");
        assert_eq!(
            json["context"]["atom_id"],
            "01ARZ3NDEKTSV4RRFFQ69G5FAV"
        );
        // 未设置的字段不出现。
        assert!(json["context"].get("layer_id").is_none());
        assert!(json["context"].get("blob_hash").is_none());
    }

    #[test]
    fn retryable_defaults_follow_document() {
        assert!(ErrorCode::Conflict.default_retryable());
        assert!(ErrorCode::JobPending.default_retryable());
        assert!(!ErrorCode::PreconditionFailed.default_retryable());
        assert!(!ErrorCode::ReferenceNotFound.default_retryable());
        assert!(YanshiError::new(ErrorCode::Conflict, ErrorContext::default()).retryable);
    }

    #[test]
    fn error_code_serde_roundtrip() {
        for code in [
            ErrorCode::ReferenceNotFound,
            ErrorCode::InvalidArgument,
            ErrorCode::PreconditionFailed,
            ErrorCode::Conflict,
            ErrorCode::PermissionDenied,
            ErrorCode::ResourceExhausted,
            ErrorCode::Degraded,
            ErrorCode::JobPending,
            ErrorCode::JobNotFound,
        ] {
            let json = serde_json::to_string(&code).unwrap();
            assert_eq!(json, format!("\"{}\"", code.as_str()));
            let back: ErrorCode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, code);
        }
    }
}
