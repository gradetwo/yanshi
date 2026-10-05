//! # 偃师 Yanshi 无头服务端
//!
//! 本 crate 实现设计文档 v1.0-draft4 的服务端语义层（不含传输实现）：
//! 文档服务、capability token 鉴权、Job 协议、广播边界、标注通道与工具协议层。
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`token`] | 12.7 | 文档级 capability token、角色与会话 |
//! | [`job`] | 6.7 | Job 状态机、TTL、取消与轮询 |
//! | [`broadcast`] | 6.8 / 12.8 | 控制流全局广播 / 数据流视口过滤 |
//! | [`annotations`] | 4.6 / 13.4 | 标注独立 append-only 通道 |
//! | [`document`] | 3 / 6.2 / 12.1 | 文档服务：提交、增量折叠、dirty、渲染、Job、广播 |
//! | [`persist`] | 18 | 文件持久化：原子 JSONL、CAS、渲染缓存、令牌 |
//! | [`tools`] | 10 章 | 工具协议层：核心 73 工具、profile 分层、10.1/5.7 响应 |
//! | [`diagnostics`] | — | 诊断包（zip）：stderr 环形缓冲、去密、采集与体积上限 |
//! | [`timings`] | — | 写路径阶段耗时（`timings`，外部报告 P2）|
//! | [`inflight`] | — | 在飞变更操作登记与协作式取消（`busy`/`cancelled`，外部报告 P1）|
//! | [`base64`] | 7.5 | MCP image content 需要的 base64 编码 |

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod blob_codec;

pub mod annotations;
pub mod archive;
pub mod base64;
pub mod broadcast;
pub mod diagnostics;
pub mod document;
pub mod inflight;
pub mod job;
pub mod persist;
pub mod service;
pub mod timings;
pub mod token;
pub mod tools;

pub use annotations::{
    Annotation, AnnotationFilter, AnnotationId, AnnotationIntent, AnnotationStatus,
    AnnotationStore, AnnotationTarget, AnnotationType, NewAnnotation,
};
pub use broadcast::{
    tile_bounds, BroadcastEvent, BroadcastStats, Broadcaster, Delivery, PushChannel, Subscriber,
    SubscriberId,
};
pub use diagnostics::{
    collect, DiagnosticsBundle, DiagnosticsRequest, Limits, LogStats, SurfaceFacts,
};
pub use document::{
    CommitResult, Document, DocumentSettings, NewDocument, RenderStatus, RenderedPreview,
};
pub use inflight::{InflightGuard, InflightInfo, InflightOp, InflightRegistry};
pub use job::{Job, JobId, JobManager, JobStatus, DEFAULT_JOB_TTL_SECONDS};
pub use persist::{DocumentMeta, FileStore, TokenRecord};
pub use service::{DocThumbSize, DocumentSummary, Workspace};
pub use timings::{CommitPhases, Phase, ToolTimings};
pub use token::{
    CapabilityToken, Principal, Role, Session, TokenAuthority, TransportKind, TOKEN_HEX_LEN,
};
pub use tools::{
    commit_response, error_response, ok_response, profile_summary, validate_args, ParamKind,
    ParamSpec, PreviewInfo, Profile, ToolContext, ToolRegistry, ToolSpec,
};
