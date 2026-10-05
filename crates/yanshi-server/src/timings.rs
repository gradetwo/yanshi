//! **写路径的阶段耗时** ✓（外部测试报告 P2）。
//!
//! **报告原话** ✗：响应里**没有任何服务端阶段耗时** ⇒ 没人知道一次慢的 `brush_stroke`
//! 把时间花在哪（dab 生成 ✓ 合成 ✓ 还是日志持久化 ✓）⇒ 测试方只能从外部做实验、
//! 反推出一个"固定 0.5 s ＋ 光栅 41 µs/px² ＋ 历史 0.005 s/atom"的模型 ✓。
//!
//! 本模块把这些阶段**在服务端如实量出来** ✓，并作为 `timings` 字段附在**工具结果 JSON** 里 ✓
//! ⇒ MCP 与 HTTP 调用方都能看到 ✓（两者共用 `ToolRegistry::call` ✓）。
//!
//! **代价** ✓：每次调用多几次 `Instant::now()`（纳秒量级 ✓）＋ 一个 7 字段的对象 ✓ ——
//! 相对一次落笔（毫秒~秒 ✓）可以忽略 ✓，所以**总是开着** ✓（不需要开关 ✓）。
//!
//! **各阶段的定义** ✓（都在本文件与调用点写明 ✓）：
//! * `prep_ms`：工具查找、角色检查、**参数 schema 校验**（顶层调用）✓；
//!   **`batch` 的子调用这里是 0** ✓ —— 参数面校验发生在**顶层那次调用**里 ✓，
//!   子调用自己的准备时间落在 `other_ms` 里 ✓（光栅/折叠/日志三相在子调用上照常准 ✓）；
//! * `raster_ms`：**光栅化/合成** —— 笔刷 dab 生成、覆盖度裁剪、blob 落库 ✓；
//! * `dirty_ms`：**脏区/tile 计算**（`plan_dirty_with_log` ＋ `apply_dirty` ✓）；
//! * `fold_ms`：**增量折叠**（状态推进 ＋ 渲染网格重建 ✓）；
//! * `log_ms`：**日志追加与落盘**（内存日志 append ＋ `FileStore` 持久化 ✓；纯内存工作区不落盘 ✓）；
//! * `other_ms`：**残差** —— 总时长减去上面已量的部分 ✓（预览渲染、快照、响应组装……✓）。
//!
//! `other_ms` 是残差而不是"又一件事" ✓：这样**五项之和恒等于 `total_ms`** ✓ ——
//! 判据因此可以断言"账目对得上" ✓（阶段若被重复累计，和会超过总时长 ⇒ 判据变红 ✓）。

use serde_json::{json, Value};
use std::time::Duration;

/// 可单独计时的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 参数解析/准备（顶层调用在 `ToolRegistry::call` 里量 ✓）。
    Prep,
    /// 光栅化/合成。
    Raster,
    /// 脏区/tile 计算。
    Dirty,
    /// 增量折叠。
    Fold,
    /// 日志追加与落盘。
    Log,
}

/// 一次工具调用的阶段累计（微秒 ✓）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ToolTimings {
    prep_us: u64,
    raster_us: u64,
    dirty_us: u64,
    fold_us: u64,
    log_us: u64,
}

impl ToolTimings {
    /// 清零（每次顶层调用开始时调用 ✓）。
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 累加一个阶段的耗时。
    pub fn add(&mut self, phase: Phase, elapsed: Duration) {
        self.add_micros(phase, elapsed.as_micros() as u64);
    }

    /// 累加一个阶段的耗时（微秒 ✓，给底层已经按微秒量的路径用 ✓）。
    pub fn add_micros(&mut self, phase: Phase, micros: u64) {
        let slot = match phase {
            Phase::Prep => &mut self.prep_us,
            Phase::Raster => &mut self.raster_us,
            Phase::Dirty => &mut self.dirty_us,
            Phase::Fold => &mut self.fold_us,
            Phase::Log => &mut self.log_us,
        };
        *slot = slot.saturating_add(micros);
    }

    /// 把提交时的分项耗时并进来 ✓（`Document`/`Workspace` 量的 ✓）。
    pub fn absorb_commit(&mut self, phases: &CommitPhases) {
        self.add_micros(Phase::Dirty, phases.dirty_us);
        self.add_micros(Phase::Fold, phases.fold_us);
        self.add_micros(Phase::Log, phases.log_us);
    }

    /// **相对某个基准点的差** ✓（`batch` 里的子调用用 ✓：每个子调用只报自己那一段 ✓）。
    pub fn since(&self, base: &ToolTimings) -> ToolTimings {
        ToolTimings {
            prep_us: self.prep_us.saturating_sub(base.prep_us),
            raster_us: self.raster_us.saturating_sub(base.raster_us),
            dirty_us: self.dirty_us.saturating_sub(base.dirty_us),
            fold_us: self.fold_us.saturating_sub(base.fold_us),
            log_us: self.log_us.saturating_sub(base.log_us),
        }
    }

    /// 已量到的阶段之和（微秒 ✓）。
    pub fn measured_us(&self) -> u64 {
        self.prep_us
            .saturating_add(self.raster_us)
            .saturating_add(self.dirty_us)
            .saturating_add(self.fold_us)
            .saturating_add(self.log_us)
    }

    /// **10.1 的 `timings` 对象** ✓（毫秒 ✓，3 位小数 ✓）。
    pub fn report(&self, total: Duration) -> Value {
        let total_us = total.as_micros() as u64;
        let other_us = total_us.saturating_sub(self.measured_us());
        json!({
            "total_ms": millis(total_us),
            "prep_ms": millis(self.prep_us),
            "raster_ms": millis(self.raster_us),
            "dirty_ms": millis(self.dirty_us),
            "fold_ms": millis(self.fold_us),
            "log_ms": millis(self.log_us),
            "other_ms": millis(other_us),
        })
    }
}

/// 毫秒（3 位小数 ✓）——避免 JSON 里出现 `0.12300000000000001` 这种二进制尾巴 ✓。
fn millis(micros: u64) -> f64 {
    (micros as f64 / 1000.0 * 1000.0).round() / 1000.0
}

/// **提交路径的分项耗时** ✓（由 `Document::commit_as_timed` 与 `Workspace::journal` 填写 ✓）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CommitPhases {
    /// 脏区/tile 计算（微秒 ✓）。
    pub dirty_us: u64,
    /// 增量折叠（微秒 ✓）。
    pub fold_us: u64,
    /// 日志追加与落盘（微秒 ✓）。
    pub log_us: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_is_internally_consistent() {
        let mut timings = ToolTimings::default();
        timings.add(Phase::Raster, Duration::from_millis(3));
        timings.add(Phase::Fold, Duration::from_micros(500));
        let report = timings.report(Duration::from_millis(10));
        let sum = report["prep_ms"].as_f64().unwrap()
            + report["raster_ms"].as_f64().unwrap()
            + report["dirty_ms"].as_f64().unwrap()
            + report["fold_ms"].as_f64().unwrap()
            + report["log_ms"].as_f64().unwrap()
            + report["other_ms"].as_f64().unwrap();
        assert!(
            (sum - report["total_ms"].as_f64().unwrap()).abs() < 1e-9,
            "五项（含残差）之和必须等于总时长：{report}"
        );
        assert_eq!(report["raster_ms"], json!(3.0));
        assert_eq!(report["other_ms"], json!(6.5));
    }

    #[test]
    fn phases_never_exceed_the_total() {
        // 阶段被重复累计（> 总时长）时，残差归零而不是变成负数 ✓ —— 判据据此发现账目不对 ✓。
        let mut timings = ToolTimings::default();
        timings.add(Phase::Raster, Duration::from_millis(20));
        let report = timings.report(Duration::from_millis(10));
        assert_eq!(report["other_ms"], json!(0.0));
        assert!(report["raster_ms"].as_f64().unwrap() > report["total_ms"].as_f64().unwrap());
    }

    #[test]
    fn since_reports_only_the_delta() {
        let mut timings = ToolTimings::default();
        timings.add(Phase::Raster, Duration::from_millis(1));
        let base = timings;
        timings.add(Phase::Raster, Duration::from_millis(2));
        let delta = timings.since(&base);
        assert_eq!(delta.raster_us, 2_000);
    }
}
