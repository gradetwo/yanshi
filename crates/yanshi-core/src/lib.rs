//! # 偃师 Yanshi 核心引擎
//!
//! 本 crate 实现设计文档 v1.0-draft4（`docs/design/yanshi-v1.0-draft4.md`）中的
//! 计算内核层与历史资产层，不依赖 GPU、GUI 与网络：
//!
//! | 模块 | 设计文档 | 职责 |
//! |---|---|---|
//! | [`error`] | 5.7 | 统一错误协议 |
//! | [`ids`] | 5.1 | 客户端 ULID、序号与各类标识 |
//! | [`atom`] | 5.1 / 5.2 | 原子结构与原子类型 |
//! | [`state`] | 4.1 – 4.5 | 文档状态模型（图层、对象、选区、检查点、头部指针） |
//! | [`fold`] | 5.3 / 5.4 | 折叠求值、有效集、级联失效、reapply、LWW |
//! | [`log`] | 5.1 / 12.2 | Append-Only 日志、ULID 幂等、提交时 precondition 校验 |
//! | [`blob`] | 6.3 | Blob CAS、三级生命周期、GC 根集与活跃 Manifest |
//! | [`seq`] | 5.5 | `state@seq` 求值公式、declare_head、增量折叠 |
//! | [`snapshot`] | 4.5 / 6.5 | 逻辑快照、CRC、可清理策略（旁路缓存，非原子） |
//! | [`changeset`] | 5.6 | 变更集分组与整体撤销规划 |
//! | [`conflict`] | 12.3 | 采样性替换冲突检测（生成性叠加走 LWW） |
//! | [`testkit`] | 5.3 / 19 | 确定性场景生成器：属性测试、fuzz 与基准共用 |
//!
//! 折叠代数的五条不变量（设计文档 5.3）由 `tests/` 下的属性测试与 fuzz 用例保证：
//! 幂等性、收敛性、无孤儿引用、`revert`-`reapply` 往返、历史可重放。
//!
//! ```
//! use yanshi_core::{Atom, AtomKind, AtomLog, FoldEngine};
//!
//! let mut log = AtomLog::new();
//! let atom = Atom::new(
//!     AtomKind::CreateDocument,
//!     "human:1",
//!     "session:a",
//!     serde_json::json!({"doc_id": "doc_1", "width": 64, "height": 64}),
//! );
//! let id = atom.id.clone();
//! log.append(atom).unwrap();
//!
//! let state = FoldEngine::new().fold(&log).unwrap().state;
//! assert_eq!(state.head_atom.as_deref(), Some(id.as_str()));
//! assert_eq!(state.width, 64);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod atom;
pub mod blob;
pub mod changeset;
pub mod conflict;
pub mod error;
pub mod fold;
pub mod ids;
pub mod log;
pub mod resample;
pub mod seq;
pub mod snapshot;
pub mod state;
pub mod testkit;

pub use atom::{
    payload_bool, payload_f64, payload_str, payload_u64, Atom, AtomId, AtomKind, BlobHash, BlobRef,
    PluginRef, Refs, CAS_THRESHOLD_BYTES, SCHEMA_VERSION, SEQ_UNASSIGNED,
};
pub use blob::{
    plan_gc, run_gc, stage_blob, BlobEntry, BlobLifecycleMetrics, BlobStore, BlobTier, FsBlobStore,
    GcPlan, GcReport, MemoryBlobStore, DEFAULT_ORPHAN_TTL_SECONDS,
};
pub use changeset::{Changeset, ChangesetBuilder};
pub use conflict::{Bbox, ConflictInfo, CONFLICT_ACTOR};
pub use error::{ErrorCode, ErrorContext, Result, YanshiError};
pub use fold::{
    apply, compute_suppressed, fold_atoms, FoldEngine, FoldResult, FoldWarning, WarningKind,
};
pub use ids::{
    now_ms, ActorId, ChangesetId, CheckpointId, DocId, LayerId, MaskId, ObjectId, Seq, SessionId,
    SnapshotId, StyleId, Ulid, UlidGen,
};
pub use log::{AppendOutcome, AtomLog, CommitContext};
pub use seq::{state_at, IncrementalFolder, StateAt, StateAtCache};
pub use snapshot::{
    crc32, snapshot_decision, Snapshot, SnapshotBase, SnapshotDecision, SnapshotStore,
    SnapshotTrigger,
};
pub use state::{
    Checkpoint, DeclareHead, DocumentState, HeadBase, Layer, LayerType, Mask, Object, ObjectRef,
    ObjectType, Selection, Style, Transform, Violation,
};
