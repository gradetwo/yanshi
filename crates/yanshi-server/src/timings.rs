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
//! * `preview_ms`：**预览/缩略图渲染** —— `finish_mutation` 里"文档级预览"
//!   （`run_pending_jobs` ⇒ `render_document_preview` ✓）与"响应里的区域预览"
//!   （`render_region` ✓）两段墙钟之和 ✓（本轮新增 ✓，见 [`Phase::Preview`] ✓）；
//! * `render_ms`：**导出路径的像素产出** —— `export_png` 从取像素
//!   （`render_region_raw` / `render_region_raw_layer` ✓）到缩放
//!   （`resample_rgba` ✓）为止 ✓（**只有导出路径会填它** ✓，其余工具恒为 0 ✓）；
//! * `png_ms`：**导出路径的 PNG 编码** —— `encode_png` 那一段 ✓
//!   （**只有导出路径会填它** ✓，其余工具恒为 0 ✓）；
//! * `other_ms`：**残差** —— 总时长减去上面已量的部分 ✓（预览渲染、快照、响应组装……✓）。
//!
//! `other_ms` 是残差而不是"又一件事" ✓：这样**各相之和恒等于 `total_ms`** ✓ ——
//! 判据因此可以断言"账目对得上" ✓（阶段若被重复累计，和会超过总时长 ⇒ 判据变红 ✓）。
//!
//! **为什么单列 `render_ms` / `png_ms`** ✗（本轮实测的缺口 ✓）：4K 导出（debug ✓）的读数
//! 曾是 `total_ms=246176.9` 而 **`other_ms=246176.9`** ✓ —— 渲染与编码**全被残差吞掉** ✗
//! ⇒ 一条读数分不清"慢在渲染"还是"慢在编码" ✗。单列之后两者**直接可读** ✓，
//! 而残差按定义自动缩小 ✓ ⇒ 既有的"各相之和 = 总时长"判据不受影响 ✓。

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
    /// **预览/缩略图渲染**（本轮新增 ✓）。
    ///
    /// **为什么必须有** ✗：外部性能报告的观测是"每一笔 `brush_stroke` 的 `other_ms`
    /// 恒定 ~11 s、与笔刷大小和 atom 数都无关" ✓ —— 而落笔路径里**唯一**具有
    /// "固定分辨率、与这一笔多大无关"这个特征的工作就是**预览渲染**（连同它内部的
    /// 位图补丁解码 ✓）。此前这一段**一个阶段都没记** ✗ ⇒ 11 s 全落进残差 ✓，
    /// 报告方只能从外部反推 ✓。
    ///
    /// 覆盖范围 ✓：`finish_mutation` 里"文档级预览渲染"（`run_pending_jobs` ⇒
    /// `render_document_preview` ✓）与"响应里的区域预览渲染"（`render_region` ✓）
    /// 两段**互不重叠**的墙钟 ✓（两段都记进同一个阶段 ✓）。
    Preview,
    /// **导出路径的像素产出**（取像素 ＋ 缩放 ✓；只有 `export_png` 会填 ✓）。
    Render,
    /// **导出路径的 PNG 编码**（只有 `export_png` 会填 ✓）。
    Png,
}

/// 一次工具调用的阶段累计（微秒 ✓）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ToolTimings {
    prep_us: u64,
    raster_us: u64,
    dirty_us: u64,
    fold_us: u64,
    log_us: u64,
    preview_us: u64,
    render_us: u64,
    png_us: u64,
    /// **★ 预览阶段被**调用**了几次 ✗ ★**（**第 253 轮 ✓）
    ///
    /// **∴ 为什么要一个**计数**✗ 而不只看 `preview_us`** ✓ ★★★**：
    ///   ＋ **∴ 实测（**CI 偶发红 ✓）✗**：`background_stroke_cost.rs` 里
    ///     `assert!(preview > 0.0)` **偶发失败** ✗ —— **∴ 而**本机连跑 3 次全过 ✓
    ///   ＋ **∴ 根因 ✗**：**那两处**记录**都在**条件分支里**✗ ⇒ **∴ 某些执行路径**根本不进**
    ///     ⇒ **∴ 于是**`preview_ms == 0`** ✓ ⇒ **∴ 拿"**墙钟 > 0**"当断言 ⇒ **随机红 ✓
    ///   ＋ **∴ 本仓的既有原则正是 ✗**：**"判据用**语义计数**✗ 不看墙钟** ✓"
    ///     （**见 `document_preview_render_count` 那条注释 ✓）⇒ **∴ 所以**：**补上这个计数 ✓
    ///       ＋ **∴ 判据改成断"**这一阶段确实被调用过**"** ✓ ⇒ **∴ 确定性的 ✓ ★★★
    preview_calls: usize,
    /// **数"向渲染层要了一块区域"的次数** ✓（**∴ 与墙钟无关 ✓**）。
    ///
    /// **∴ 为什么要有它 ✗**：`sample_colors` 的判据原来比**净每点成本**✗（时间），
    /// 而 CI 的 CPU 让净比从本机 0.33 变成 0.89 ✗ ⇒ **∴ 那条判据在 CI 上一直红 ✓**
    /// （**∴ 已登记 known-red ✓，见 `scripts/criteria-known-red.txt` ✓**）。
    /// ⇒ **∴ 而真正该断的是**语义**✗**：**一次 `sample_colors` 只应**要一次区域**✗**
    ///   ⇒ **∴ 逐点渲染会让它变成 N ✗ ⇒ **∴ 而这个计数**与机器快慢无关 ✓**
    region_render_calls: usize,
}

impl ToolTimings {
    /// 清零（每次顶层调用开始时调用 ✓）。
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// 记一次"向渲染层要区域" ✓（**∴ 计数是确定性的 ✗ 墙钟不是 ✓**）。
    pub fn count_region_render(&mut self) {
        self.region_render_calls = self.region_render_calls.saturating_add(1);
    }

    /// 累加一个阶段的耗时。
    pub fn add(&mut self, phase: Phase, elapsed: Duration) {
        self.add_micros(phase, elapsed.as_micros() as u64);
    }

    /// 累加一个阶段的耗时（微秒 ✓，给底层已经按微秒量的路径用 ✓）。
    pub fn add_micros(&mut self, phase: Phase, micros: u64) {
        // **∴ 计数与耗时**分开记 ✗**（**∴ 计数是确定性的 ✗ 耗时不是 ✓）★
        if phase == Phase::Preview {
            self.preview_calls = self.preview_calls.saturating_add(1);
        }
        let slot = match phase {
            Phase::Prep => &mut self.prep_us,
            Phase::Raster => &mut self.raster_us,
            Phase::Dirty => &mut self.dirty_us,
            Phase::Fold => &mut self.fold_us,
            Phase::Log => &mut self.log_us,
            Phase::Preview => &mut self.preview_us,
            Phase::Render => &mut self.render_us,
            Phase::Png => &mut self.png_us,
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
            preview_us: self.preview_us.saturating_sub(base.preview_us),
            render_us: self.render_us.saturating_sub(base.render_us),
            png_us: self.png_us.saturating_sub(base.png_us),
            // **∴ 计数也要**相对**✗（**子调用只报自己那段 ✓）★
            preview_calls: self.preview_calls.saturating_sub(base.preview_calls),
            region_render_calls: self
                .region_render_calls
                .saturating_sub(base.region_render_calls),
        }
    }

    /// 已量到的阶段之和（微秒 ✓）。
    pub fn measured_us(&self) -> u64 {
        self.prep_us
            .saturating_add(self.raster_us)
            .saturating_add(self.dirty_us)
            .saturating_add(self.fold_us)
            .saturating_add(self.log_us)
            .saturating_add(self.preview_us)
            .saturating_add(self.render_us)
            .saturating_add(self.png_us)
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
            "preview_ms": millis(self.preview_us),
            // **∴ 语义计数 ✗**（**与 `preview_ms` 并排 ⇒ 判据可以不看墙钟 ✓）★
            "preview_calls": self.preview_calls,
            "region_render_calls": self.region_render_calls,
            "render_ms": millis(self.render_us),
            "png_ms": millis(self.png_us),
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
        // **预览阶段也要进这份账目** ✗：残差是按"总时长 − 全部已量阶段"算的 ✓
        // ⇒ 若这里漏掉 `preview_ms` ✓，残差就会把预览那段时间**再扣一次** ✗
        // ⇒ 这条"各相之和 = 总时长"的判据会在有预览时红 ✓（正是它该红的 ✓）。
        timings.add(Phase::Preview, Duration::from_millis(1));
        let report = timings.report(Duration::from_millis(10));
        let sum = report["prep_ms"].as_f64().unwrap()
            + report["raster_ms"].as_f64().unwrap()
            + report["dirty_ms"].as_f64().unwrap()
            + report["fold_ms"].as_f64().unwrap()
            + report["log_ms"].as_f64().unwrap()
            + report["preview_ms"].as_f64().unwrap()
            + report["render_ms"].as_f64().unwrap()
            + report["png_ms"].as_f64().unwrap()
            + report["other_ms"].as_f64().unwrap();
        assert!(
            (sum - report["total_ms"].as_f64().unwrap()).abs() < 1e-9,
            "各相（含残差）之和必须等于总时长：{report}"
        );
        assert_eq!(report["raster_ms"], json!(3.0));
        assert_eq!(report["preview_ms"], json!(1.0));
        assert_eq!(report["other_ms"], json!(5.5));
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

    /// **导出两相进账目** ✓：`render_ms` / `png_ms` 被算进 `measured_us()` ✓
    /// ⇒ 残差相应变小 ✓，而各相之和仍等于总时长 ✓。
    #[test]
    fn export_phases_shrink_the_residual_instead_of_breaking_the_sum() {
        let mut timings = ToolTimings::default();
        timings.add(Phase::Render, Duration::from_millis(4));
        timings.add(Phase::Png, Duration::from_millis(1));
        let report = timings.report(Duration::from_millis(10));
        assert_eq!(report["render_ms"], json!(4.0));
        assert_eq!(report["png_ms"], json!(1.0));
        // 残差 = 10 − 4 − 1 = 5 ✓（不是 10 ✗ —— 那正是"两相被吞掉"的旧读数 ✓）。
        assert_eq!(report["other_ms"], json!(5.0));
        assert_eq!(timings.measured_us(), 5_000);
    }
}
