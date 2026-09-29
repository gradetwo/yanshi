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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod annotations;
pub mod broadcast;
pub mod job;
pub mod token;

pub use annotations::{
    Annotation, AnnotationFilter, AnnotationId, AnnotationIntent, AnnotationStatus,
    AnnotationStore, AnnotationTarget, AnnotationType, NewAnnotation,
};
pub use broadcast::{
    tile_bounds, BroadcastEvent, BroadcastStats, Broadcaster, Delivery, PushChannel, Subscriber,
    SubscriberId,
};
pub use job::{Job, JobId, JobManager, JobStatus, DEFAULT_JOB_TTL_SECONDS};
pub use token::{
    CapabilityToken, Principal, Role, Session, TokenAuthority, TransportKind, TOKEN_HEX_LEN,
};
