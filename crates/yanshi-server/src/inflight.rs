//! **在飞变更操作的登记表 + 协作式取消** ✓（外部测试报告 P1）。
//!
//! **要解决的问题**（报告原话）✗：客户端超时**不会**让服务端停下来 ——
//! 300 s 客户端超时后服务端仍在跑、文档仍被独占 ✓；调用方**分不清"慢"与"挂死"** ✗，
//! 于是重试 ⇒ **重复落笔** ✗（真实事故：seq 477/478 的孤儿原子，最后靠手工删除 ✓）。
//!
//! 本模块只做两件事 ✓（都可观测 ✓）：
//! 1. **在飞登记** ✓：同一文档上已有一个变更操作时，新的变更请求**被拒绝** ✓，
//!    返回 `busy` ＋ "是哪个操作、已经跑了多久" ✓ ⇒ 重试不可能悄悄变成第二次落笔 ✓；
//! 2. **协作式取消** ✓：`InflightOp` 里有一个 `AtomicBool` 取消标志 ✓ ——
//!    长循环在**安全点**轮询它 ✓，置位后操作以 `cancelled` 收尾 ✓、不再提交新的原子 ✓。
//!
//! **为什么是"协作式"而不是"强杀"** ✓：本服务是**单进程同步**实现 ✓（`Workspace` 一把互斥锁 ✓），
//! 强杀线程会留下半更新的渲染缓存与日志 ✗ ⇒ 取消必须在**循环的安全点**由执行者自己看到 ✓
//!（Hokusai 内部的单次 dab 循环改不动 ✗，所以安全点是"每笔 / 每段 / 每次子提交之前"✓）。
//!
//! **为什么登记表可以脱离 `Workspace` 锁读** ✓：这是本模块存在的关键 ✗ ——
//! 长操作**持有** `Workspace` 锁 ✓，若"忙不忙"也要先拿那把锁才能问 ✓，
//! 那么第二个请求会**排队等到长操作结束** ✗（正是报告里的现象 ✓）。
//! ⇒ 登记表用**自己的** `Mutex` ✓，并始终以 `Arc` 持有 ✓，
//! HTTP 层在**取工作区锁之前**就能读它 ✓（`ServerState::inflight` ✓）。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use yanshi_core::{ErrorCode, ErrorContext, YanshiError};

/// 一个正在运行的变更操作。
#[derive(Debug)]
pub struct InflightOp {
    tool: String,
    doc_id: String,
    actor: String,
    session: String,
    started_at: i64,
    cancel: AtomicBool,
}

impl InflightOp {
    /// 工具名（例如 `brush_stroke` / `batch`）。
    pub fn tool(&self) -> &str {
        &self.tool
    }

    /// 目标文档。
    pub fn doc_id(&self) -> &str {
        &self.doc_id
    }

    /// 开始时间（Unix 毫秒）。
    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    /// 是否已被请求取消。
    pub fn cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// 请求取消；返回"此前是否已经请求过"（幂等 ✓）。
    pub fn request_cancel(&self) -> bool {
        self.cancel.swap(true, Ordering::SeqCst)
    }

    /// 已经运行了多久（毫秒）。
    pub fn running_ms(&self, now: i64) -> u64 {
        (now - self.started_at).max(0) as u64
    }

    /// 可序列化的状态快照（`get_inflight` 与 `busy` 错误共用 ✓）。
    pub fn info(&self, now: i64) -> InflightInfo {
        InflightInfo {
            tool: self.tool.clone(),
            doc_id: self.doc_id.clone(),
            actor: self.actor.clone(),
            session: self.session.clone(),
            started_at: self.started_at,
            running_ms: self.running_ms(now),
            cancel_requested: self.cancel_requested(),
        }
    }

    /// **5.7 的 `busy` 错误** ✓：`retryable=false` ＋ 结构化上下文 ✓。
    ///
    /// **为什么把 `running_ms` 放进上下文** ✓：这是"慢"与"挂死"的**唯一**区分依据 ✓ ——
    /// 调用方两次查询之间它**在变大** ⇒ 服务端在动 ✓（报告里最缺的正是这个观察点 ✓）。
    pub fn busy_error(&self, now: i64) -> YanshiError {
        let info = self.info(now);
        YanshiError::new(
            ErrorCode::Busy,
            ErrorContext::detail(format!(
                "文档 {} 上已有在飞的变更操作 {}（已运行 {} ms）⇒ 这次请求被拒绝 ✓。\
                 **不要盲目重试** ✗：你无法知道上一次超时的请求是否已经落笔 ✓\
                 ⇒ 先 `get_inflight` 看它是否还在跑 ✓，或等它结束后核对 `head` 再决定 ✓；\
                 确实要中止它 ⇒ `cancel_operation` ✓",
                self.doc_id, self.tool, info.running_ms
            )),
        )
        .with_extra("status", serde_json::json!("busy"))
        .with_extra("inflight", serde_json::json!(info))
    }

    /// **5.7 的 `cancelled` 错误** ✓：带上"是哪个操作、跑了多久" ✓。
    pub fn cancelled_error(&self, now: i64) -> YanshiError {
        let info = self.info(now);
        YanshiError::new(
            ErrorCode::Cancelled,
            ErrorContext::detail(format!(
                "操作 {} 已被取消（安全点检查，已运行 {} ms）⇒ 不再提交新的原子 ✓。\
                 已经落下的原子不会自动消失 ⇒ 先核对 `head` 与响应里的 `rolled_back` ✓",
                self.tool, info.running_ms
            )),
        )
        .with_extra("status", serde_json::json!("cancelled"))
        .with_extra("inflight", serde_json::json!(info))
    }
}

/// 在飞操作的状态快照（可序列化 ✓）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InflightInfo {
    /// 工具名。
    pub tool: String,
    /// 目标文档。
    pub doc_id: String,
    /// 操作者。
    pub actor: String,
    /// 会话。
    pub session: String,
    /// 开始时间（Unix 毫秒）。
    pub started_at: i64,
    /// 已运行毫秒数。
    pub running_ms: u64,
    /// 是否已被请求取消。
    pub cancel_requested: bool,
}

/// 在飞变更操作的登记表（**自己的锁** ✓，见模块说明 ✓）。
///
/// **必须由 `Arc` 持有** ✓：`begin` 的接收者是 `self: &Arc<Self>` ✓ ——
/// 这样 RAII 句柄才能在 `Drop` 时注销自己 ✓。`Workspace` 正是这样持有的 ✓。
#[derive(Debug, Default)]
pub struct InflightRegistry {
    ops: Mutex<BTreeMap<String, Arc<InflightOp>>>,
    begun: AtomicU64,
    rejected: AtomicU64,
    cancel_requests: AtomicU64,
}

impl InflightRegistry {
    /// 空登记表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个新的变更操作；同一文档已有在飞操作 ⇒ `busy` ✓。
    pub fn begin(
        self: &Arc<Self>,
        doc_id: &str,
        tool: &str,
        actor: &str,
        session: &str,
        now: i64,
    ) -> std::result::Result<InflightGuard, YanshiError> {
        let mut ops = self.lock();
        if let Some(existing) = ops.get(doc_id) {
            self.rejected.fetch_add(1, Ordering::Relaxed);
            return Err(existing.busy_error(now));
        }
        let op = Arc::new(InflightOp {
            tool: tool.to_owned(),
            doc_id: doc_id.to_owned(),
            actor: actor.to_owned(),
            session: session.to_owned(),
            started_at: now,
            cancel: AtomicBool::new(false),
        });
        ops.insert(doc_id.to_owned(), Arc::clone(&op));
        self.begun.fetch_add(1, Ordering::Relaxed);
        Ok(InflightGuard {
            registry: Arc::clone(self),
            op,
        })
    }

    /// 注销（只有仍登记着**同一个** `Arc` 时才移除 ✓ —— 免得误删后来者 ✓）。
    fn finish(&self, op: &Arc<InflightOp>) {
        let mut ops = self.lock();
        let same = ops
            .get(op.doc_id())
            .map(|current| Arc::ptr_eq(current, op))
            .unwrap_or(false);
        if same {
            ops.remove(op.doc_id());
        }
    }

    /// 当前在飞的某个文档操作。
    pub fn op(&self, doc_id: &str) -> Option<Arc<InflightOp>> {
        self.lock().get(doc_id).cloned()
    }

    /// 某个文档的忙状态快照。
    pub fn status(&self, doc_id: &str) -> Option<InflightInfo> {
        let now = yanshi_core::now_ms();
        self.op(doc_id).map(|op| op.info(now))
    }

    /// **忙就给出 5.7 的 `busy` 错误** ✓（入口层在取工作区锁**之前**用它 ✓）。
    pub fn busy_error(&self, doc_id: &str, now: i64) -> Option<YanshiError> {
        self.op(doc_id).map(|op| op.busy_error(now))
    }

    /// 全部在飞操作（按文档 id 排序 ✓，观察用 ✓）。
    pub fn list(&self) -> Vec<InflightInfo> {
        let now = yanshi_core::now_ms();
        self.lock().values().map(|op| op.info(now)).collect()
    }

    /// **请求取消**某个文档上的在飞操作；返回取消后的状态（没有则 `None` ✓）。
    pub fn request_cancel(&self, doc_id: &str) -> Option<InflightInfo> {
        let op = self.op(doc_id)?;
        op.request_cancel();
        self.cancel_requests.fetch_add(1, Ordering::Relaxed);
        Some(op.info(yanshi_core::now_ms()))
    }

    /// 累计登记过的操作数（观察用）。
    pub fn begun(&self) -> u64 {
        self.begun.load(Ordering::Relaxed)
    }

    /// 累计被 `busy` 拒绝的请求数（观察用）。
    pub fn rejected(&self) -> u64 {
        self.rejected.load(Ordering::Relaxed)
    }

    /// 累计取消请求数（观察用）。
    pub fn cancel_requests(&self) -> u64 {
        self.cancel_requests.load(Ordering::Relaxed)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Arc<InflightOp>>> {
        // 毒锁不 panic ✓：登记表是**观察用**的 ✓，一个线程 panic 不该让服务再也接不了活 ✗。
        self.ops.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// RAII 登记句柄：离开作用域即注销 ✓（含错误路径与 panic 路径 ✓）。
#[derive(Debug)]
pub struct InflightGuard {
    registry: Arc<InflightRegistry>,
    op: Arc<InflightOp>,
}

impl InflightGuard {
    /// 本次操作的状态。
    pub fn op(&self) -> &Arc<InflightOp> {
        &self.op
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.registry.finish(&self.op);
    }
}
