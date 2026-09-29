//! Job 协议（设计文档 6.7）。
//!
//! 统一异步操作模型：语义工具返回 `job_id`，状态机
//! `submitted → running → committed(atom_id) / failed(error_code) / cancelled / expired`。
//! 外部服务挂死由 TTL 兜底，不产生永久悬挂 job。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use yanshi_core::{AtomId, ErrorCode, ErrorContext, Result, SessionId, YanshiError};

/// Job 标识。
pub type JobId = String;

/// Job 默认 TTL（秒）：语义工具外部调用较长（6.7）。
pub const DEFAULT_JOB_TTL_SECONDS: i64 = 300;

/// Job 状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// 已提交。
    Submitted,
    /// 运行中。
    Running,
    /// 已完成并提交原子。
    Committed,
    /// 失败。
    Failed,
    /// 已取消。
    Cancelled,
    /// 超过 TTL。
    Expired,
}

impl JobStatus {
    /// 是否是终态。
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Committed | Self::Failed | Self::Cancelled | Self::Expired
        )
    }

    /// 字符串名。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Running => "running",
            Self::Committed => "committed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }
}

/// Job 对象（6.7）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// 标识。
    pub id: JobId,
    /// 类型（工具名或内部任务名）。
    pub kind: String,
    /// 发起会话。
    pub session: SessionId,
    /// 创建时间（Unix 毫秒）。
    pub created_at: i64,
    /// 状态。
    pub status: JobStatus,
    /// 完成后产出的原子。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub atom_id: Option<AtomId>,
    /// 失败原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    /// TTL（秒）。
    pub ttl: i64,
}

impl Job {
    /// 是否已超时（仅对未结束的 job 有意义）。
    pub fn is_expired(&self, now: i64) -> bool {
        !self.status.is_terminal() && (now - self.created_at) / 1000 >= self.ttl
    }
}

/// Job 管理器。
#[derive(Debug)]
pub struct JobManager {
    jobs: BTreeMap<JobId, Job>,
    /// 插入顺序，用于容量淘汰与 GC。
    order: VecDeque<JobId>,
    default_ttl: i64,
    max_jobs: usize,
    submitted: u64,
    expired: u64,
    cancelled: u64,
    counter: u64,
}

impl Default for JobManager {
    fn default() -> Self {
        Self::new(DEFAULT_JOB_TTL_SECONDS)
    }
}

impl JobManager {
    /// 以默认 TTL 构造。
    pub fn new(default_ttl_seconds: i64) -> Self {
        Self {
            jobs: BTreeMap::new(),
            order: VecDeque::new(),
            default_ttl: default_ttl_seconds.max(1),
            max_jobs: 1024,
            submitted: 0,
            expired: 0,
            cancelled: 0,
            counter: 0,
        }
    }

    /// 容量上限（超出时淘汰最旧的终态 job）。
    pub fn with_capacity(mut self, max_jobs: usize) -> Self {
        self.max_jobs = max_jobs.max(1);
        self
    }

    /// 提交新 job。
    pub fn submit(
        &mut self,
        kind: impl Into<String>,
        session: impl Into<SessionId>,
        now: i64,
    ) -> JobId {
        self.counter += 1;
        let id = format!("job_{:08x}", self.counter);
        let job = Job {
            id: id.clone(),
            kind: kind.into(),
            session: session.into(),
            created_at: now,
            status: JobStatus::Submitted,
            atom_id: None,
            error: None,
            ttl: self.default_ttl,
        };
        self.jobs.insert(id.clone(), job);
        self.order.push_back(id.clone());
        self.submitted += 1;
        self.enforce_capacity();
        id
    }

    /// 以指定 TTL 提交。
    pub fn submit_with_ttl(
        &mut self,
        kind: impl Into<String>,
        session: impl Into<SessionId>,
        now: i64,
        ttl_seconds: i64,
    ) -> JobId {
        let id = self.submit(kind, session, now);
        if let Some(job) = self.jobs.get_mut(&id) {
            job.ttl = ttl_seconds.max(1);
        }
        id
    }

    /// 当前 job 数。
    pub fn len(&self) -> usize {
        self.jobs.len()
    }

    /// 是否没有 job。
    pub fn is_empty(&self) -> bool {
        self.jobs.is_empty()
    }

    /// 累计提交数。
    pub fn submitted(&self) -> u64 {
        self.submitted
    }

    /// 累计过期数。
    pub fn expired(&self) -> u64 {
        self.expired
    }

    /// 累计取消数。
    pub fn cancelled(&self) -> u64 {
        self.cancelled
    }

    /// 查询 job（顺带做 TTL 过期判定）。
    pub fn get(&mut self, id: &str, now: i64) -> Result<Job> {
        self.expire_due(now);
        self.jobs.get(id).cloned().ok_or_else(|| not_found(id))
    }

    /// 进入运行中（`submitted → running`）。
    pub fn begin(&mut self, id: &str, now: i64) -> Result<JobStatus> {
        self.expire_due(now);
        let job = self.jobs.get_mut(id).ok_or_else(|| not_found(id))?;
        match job.status {
            JobStatus::Submitted => {
                job.status = JobStatus::Running;
                Ok(job.status)
            }
            other => Err(invalid_transition(id, other, JobStatus::Running)),
        }
    }

    /// 完成并登记产出的原子。
    pub fn complete(
        &mut self,
        id: &str,
        atom_id: impl Into<AtomId>,
        now: i64,
    ) -> Result<JobStatus> {
        self.expire_due(now);
        let job = self.jobs.get_mut(id).ok_or_else(|| not_found(id))?;
        match job.status {
            JobStatus::Submitted | JobStatus::Running => {
                job.status = JobStatus::Committed;
                job.atom_id = Some(atom_id.into());
                Ok(job.status)
            }
            other => Err(invalid_transition(id, other, JobStatus::Committed)),
        }
    }

    /// 标记失败。
    pub fn fail(&mut self, id: &str, error: &YanshiError, now: i64) -> Result<JobStatus> {
        self.expire_due(now);
        let job = self.jobs.get_mut(id).ok_or_else(|| not_found(id))?;
        if job.status.is_terminal() {
            return Err(invalid_transition(id, job.status, JobStatus::Failed));
        }
        job.status = JobStatus::Failed;
        job.error = Some(error.to_response());
        Ok(job.status)
    }

    /// 取消（只能取消未结束的 job）。
    pub fn cancel(&mut self, id: &str, now: i64) -> Result<JobStatus> {
        self.expire_due(now);
        let job = self.jobs.get_mut(id).ok_or_else(|| not_found(id))?;
        if job.status.is_terminal() {
            return Err(invalid_transition(id, job.status, JobStatus::Cancelled));
        }
        job.status = JobStatus::Cancelled;
        self.cancelled += 1;
        Ok(job.status)
    }

    /// 把所有超时 job 标记为 `expired`，返回本次过期的 id。
    pub fn expire_due(&mut self, now: i64) -> Vec<JobId> {
        let mut expired = Vec::new();
        for job in self.jobs.values_mut() {
            if job.is_expired(now) {
                job.status = JobStatus::Expired;
                expired.push(job.id.clone());
            }
        }
        self.expired += expired.len() as u64;
        expired
    }

    /// 清理已结束且超过 TTL 的 job，返回清理数量。
    pub fn gc(&mut self, now: i64) -> usize {
        self.expire_due(now);
        let doomed: Vec<JobId> = self
            .jobs
            .values()
            .filter(|job| job.status.is_terminal() && (now - job.created_at) / 1000 >= job.ttl)
            .map(|job| job.id.clone())
            .collect();
        for id in &doomed {
            self.jobs.remove(id);
            self.order.retain(|candidate| candidate != id);
        }
        doomed.len()
    }

    /// 未结束的 job 列表（按提交顺序）。
    pub fn pending(&self) -> Vec<Job> {
        self.order
            .iter()
            .filter_map(|id| self.jobs.get(id))
            .filter(|job| !job.status.is_terminal())
            .cloned()
            .collect()
    }

    /// 硬容量上限：超出时淘汰最旧的 job（无论状态）。
    ///
    /// 被淘汰的 job 之后查询返回 `job_not_found`（5.7 明确允许「已被 TTL 清理」这一情形）；
    /// 未完成就被淘汰的计入 `expired`，作为 TTL 兜底的另一种形式。
    fn enforce_capacity(&mut self) {
        while self.jobs.len() > self.max_jobs {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            match self.jobs.remove(&oldest) {
                Some(job) if !job.status.is_terminal() => {
                    self.expired += 1;
                }
                Some(_) => {}
                None => {}
            }
        }
    }
}

fn not_found(id: &str) -> YanshiError {
    YanshiError::new(
        ErrorCode::JobNotFound,
        ErrorContext::detail(format!("job {id} 不存在或已被清理")),
    )
    .with_extra("job_id", serde_json::json!(id))
}

fn invalid_transition(id: &str, from: JobStatus, to: JobStatus) -> YanshiError {
    YanshiError::new(
        ErrorCode::JobNotFound,
        ErrorContext::detail(format!(
            "job {id} 状态 {} 不能转换为 {}",
            from.as_str(),
            to.as_str()
        )),
    )
    .with_extra("job_id", serde_json::json!(id))
    .with_extra("status", serde_json::json!(from.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yanshi_core::ErrorContext;

    #[test]
    fn lifecycle_transitions_follow_the_state_machine() {
        let mut manager = JobManager::new(300);
        let id = manager.submit("inpaint_region", "session:a", 1_000);
        assert_eq!(
            manager.get(&id, 1_000).unwrap().status,
            JobStatus::Submitted
        );

        assert_eq!(manager.begin(&id, 1_100).unwrap(), JobStatus::Running);
        // 不能重复进入 running。
        assert!(manager.begin(&id, 1_200).is_err());
        assert_eq!(
            manager.complete(&id, "atom_1", 1_300).unwrap(),
            JobStatus::Committed
        );
        let job = manager.get(&id, 1_400).unwrap();
        assert_eq!(job.atom_id.as_deref(), Some("atom_1"));
        assert!(job.status.is_terminal());
        // 终态不可再取消。
        assert!(manager.cancel(&id, 1_500).is_err());
        assert_eq!(manager.submitted(), 1);
    }

    #[test]
    fn ttl_expires_hanging_jobs() {
        let mut manager = JobManager::new(300);
        let id = manager.submit("analyze_image", "session:a", 0);
        assert_eq!(manager.expire_due(100_000).len(), 0, "未到 TTL");
        let expired = manager.expire_due(300 * 1000 + 1);
        assert_eq!(expired, vec![id.clone()]);
        assert_eq!(
            manager.get(&id, 999_999_999).unwrap().status,
            JobStatus::Expired
        );
        assert_eq!(manager.expired(), 1);
        // 取消/失败对终态无效。
        assert_eq!(
            manager.fail(&id, &not_found("x"), 1).unwrap_err().code,
            ErrorCode::JobNotFound
        );
    }

    #[test]
    fn fail_records_error_response_shape() {
        let mut manager = JobManager::new(300);
        let id = manager.submit("inpaint_region", "session:a", 0);
        let error = YanshiError::new(
            ErrorCode::Degraded,
            ErrorContext::detail("本地推理端点不可用"),
        );
        manager.fail(&id, &error, 10).unwrap();
        let job = manager.get(&id, 1_000).unwrap();
        let recorded = job.error.unwrap();
        assert_eq!(recorded["ok"], serde_json::json!(false));
        assert_eq!(recorded["error_code"], "degraded");
    }

    #[test]
    fn cancel_and_gc_keep_the_registry_bounded() {
        let mut manager = JobManager::new(10).with_capacity(3);
        let first = manager.submit("a", "s", 0);
        manager.cancel(&first, 1).unwrap();
        assert_eq!(manager.cancelled(), 1);
        for index in 0..5 {
            manager.submit(format!("task_{index}"), "s", 0);
        }
        assert!(manager.len() <= 4, "容量上限生效：{}", manager.len());
        // 过了 TTL 后 GC 清理终态 job。
        let removed = manager.gc(10 * 1000 + 1);
        assert!(removed >= 1);
        assert!(!manager.pending().iter().any(|job| job.id == first));
    }

    #[test]
    fn submit_with_ttl_overrides_default() {
        let mut manager = JobManager::new(300);
        let id = manager.submit_with_ttl("semantic", "s", 0, 5);
        assert_eq!(
            manager.get(&id, 4_000).unwrap().status,
            JobStatus::Submitted
        );
        assert_eq!(manager.get(&id, 6_000).unwrap().status, JobStatus::Expired);
    }
}
