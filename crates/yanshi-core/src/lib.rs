//! # 偃师 Yanshi 核心引擎
//!
//! 本 crate 实现设计文档 v1.0-draft4（`docs/design/yanshi-v1.0-draft4.md`）中的
//! 计算内核层与历史资产层，不依赖 GPU、GUI 与网络：
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`error`] | 5.7 | 统一错误协议 |
//! | [`ids`] | 5.1 | 客户端 ULID、序号与各类标识 |
//!
//! 折叠代数的五条不变量（设计文档 5.3）由 `tests/` 下的属性测试与 fuzz 用例保证：
//! 幂等性、收敛性、无孤儿引用、`revert`-`reapply` 往返、历史可重放。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod ids;

pub use error::{ErrorCode, Result, YanshiError};
pub use ids::{
    now_ms, ActorId, ChangesetId, CheckpointId, DocId, LayerId, MaskId, ObjectId, Seq, SessionId,
    SnapshotId, StyleId, Ulid, UlidGen,
};
