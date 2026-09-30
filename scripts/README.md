# 验收脚本

这些脚本是**可复现的验收证据**，不是一次性玩具：每一条性能与正确性结论都应该能用它们复跑出来。

共同前置条件：

```bash
# 1) 服务端（release 构建；开发时用 cargo run 亦可）
target/release/yanshi-serve --workspace ~/.local/share/yanshi/workspace --port 8110
# 2) 带远程调试的 Chromium（浏览器类脚本需要）
chromium --remote-debugging-port=9333 --headless=new about:blank
```

依赖仅 `curl`、`python3`、`node`（≥18，用到内置 `fetch`/`WebSocket`）。浏览器类脚本通过 CDP 驱动，
驱动的是**编辑器自身的真实路径**，而不是另写一套比对逻辑。

## `browser-ui-check.mjs` — 查看器 UI 回归检查（真实 Chromium）

走用户同一条路径：新建图层 → 画一笔 → 断言**画布确有已绘制像素**、内容画布与覆盖层几何一致、
页面里没有覆盖用的 `#preview`、提交后缩略图自动刷新、**「新建」与「打开」确实切换文档**、
**舞台不宽于画布**（否则右侧是灰色死区、点击无效）、一笔只产生**一条** `draw_stroke` 日志
（防止重复广播）、右侧面板不溢出窗口。它对应的是一组真实使用缺陷
（操作后画布空白、右边颜色不同且点击无效、缩略图不刷新），因此**每次改查看器都应跑**。

前置与 `browser-pixel-check.mjs` 相同（服务端 + 带远程调试的 Chromium）：

```bash
chromium --remote-debugging-port=9333 --headless=new about:blank
node scripts/browser-ui-check.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
UI_DEBUG=1 UI_TRACE=1 node scripts/browser-ui-check.mjs "<url>"   # 额外打印页面日志与绘制打点
```

环境变量：`UI_TIMEOUT_MS`（总体超时，缺省 180s；调试目标无响应时脚本会以退出码 3 结束）。

## 重活交给 GitHub（CI/CD）

* **快反馈**（`.github/workflows/ci.yml`，push/PR）：rustfmt、clippy、workspace 测试（stable/beta）、
  **wasm 冒烟**（唯一能抓"原生全绿、浏览器全崩"那类回归的检查）；
* **重活**（`.github/workflows/heavy.yml`，每夜定时 + 手动）：整仓 `--ignored` 长任务
  （10 万原子 fuzz、性能预算、4K 剖面、单效果成本、内核原生计时），日志上传为 artifact。

```bash
gh workflow run heavy.yml          # 手动触发
gh run list --limit 5              # 查看状态
gh run view <id> --log | tail -50  # 读日志
gh run download <id> -n ignored-suite-log   # 下载 artifact
```

**本机只跑快测试**（`cargo test --workspace` + `scripts/wasm-smoke.sh`），
长任务交给 CI 并行执行，本地继续其他开发，**定时查看**结果即可 —— 这也是本仓库的分工约定。

## `phase4b-demo.sh` — 建议环路端到端演示

```bash
scripts/phase4b-demo.sh                       # 默认打 127.0.0.1:8110
BASE_URL=http://127.0.0.1:8080 scripts/phase4b-demo.sh
```

覆盖：人类标注 → AI 建议（含 patch）→ 只读步骤被拒 → 接受并重放（**像素指纹必须变化**）→
拒绝并记录原因 → 补丁内先建图层再绘制（**图层数必须增加**）。只创建**新文档**，不动既有数据。

## `wasm-smoke.sh` — wasm 运行时冒烟门禁（**不需要浏览器**）

```bash
scripts/wasm-smoke.sh
```

把内核编译为 wasm32，在 node 里真正构造内核、渲染并走一次写入路径（预览 + 提交）。
存在的理由：`cargo test` 在宿主上跑，**原生全绿不代表浏览器可用** —— 曾经一个诊断探针在
wasm32 上调用了 `std::time::Instant::now()`（该目标不支持，直接 panic），原生毫无反应、
浏览器端每次渲染都崩。该脚本就是为拦住这类回归。

已做**反向验证**：把那个 `Instant::now()` 放回去，脚本立刻以 wasm 栈报错并非零退出 ✓

配套还有 `crates/yanshi-render/tests/wasm_target_guard.rs`（普通测试套件里跑）：
用「带理由的白名单绊线」统计 `Instant::now` / `SystemTime::now` 的出现次数，
任何新增用法都会失败，迫使作者显式确认是否已按目标平台门控。

## `browser-pixel-check.mjs` — 内核 vs 服务端逐像素自检（D1 口径）

```bash
node scripts/browser-pixel-check.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
```

读取编辑器内置自检结果：差异像素 ≤16 且最大通道差 ≤1 LSB 记为通过，退出码 0/1 便于接入 CI。

> 预算覆盖情况（设计 14.10 的每一行是否有测试/脚本证据）记录在
> `docs/design/implementation-notes.md` 的「14.10 预算表覆盖情况」节，
> 由 `crates/yanshi-server/tests/budget_coverage.rs` 校验：声称「已覆盖」的证据必须真实存在。

## `browser-drag-perf.mjs` — 拖动笔迹成本

```bash
node scripts/browser-drag-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..." 20
```

输出每段的同步处理耗时（中位数/平均/p95/最大）、首段、`pointerup` 提交耗时与到下一帧的耗时。

> **测量口径提醒**：只测同步处理耗时。若在每段后 `await requestAnimationFrame`，
> 量到的主要是 60fps 帧边界（~16.7ms），真实工作量（~1ms）会被淹没 —— 首版探针就踩了这个坑。

## `browser-first-paint.mjs` — 打开文档的首帧与可交互时间

```bash
node scripts/browser-first-paint.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
# 可用 FIRST_PAINT_BUDGET_MS / KERNEL_WARM_BUDGET_MS 调整预算，退出码 0/1 可接 CI
```

默认**禁用缓存**重载（否则第二次运行量到的是浏览器缓存而不是冷启动），数字取自编辑器自身的
`window.yanshiStats.firstPaintMs` / `kernelWarmMs`，因此量的是真实用户路径。
本机基线：简单文档首帧 101ms、内核预热 48ms；含蒙版与液化的重文档 458ms / 60ms。

## `browser-kernel-perf.mjs` — 客户端 WASM 内核区域渲染成本

```bash
node scripts/browser-kernel-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=...&debug=1"
```

需要 URL 带 `debug=1`（内核句柄 `window.yanshiKernel` 仅在调试模式暴露）。输出 8²–512² 的
5 次取最小时耗与 ns/px，可与服务端口径横向对比。

| 区域 | 客户端内核 | 服务端（raw） |
|---|---|---|
| 64² | 4.5ms | 0.89ms |
| 128² | 4.8ms | 3.09ms |
| 256² | 6.5ms | 12.24ms |
| 512² | 26.9ms | 49.07ms |

客户端约 **4.2ms 固定开销 + 约 87ns/px**（512²），每像素比服务端快约 2×（少了 PNG、缩略图与
背景扁平化）；服务端约 187ns/px。

> **必须让机器空闲再测**：同一次 64² 测量在 `cargo build` 刚结束时得到 13.5ms、空闲时 4.5ms（3×）。
> 本机噪声极大，只能支撑量级结论；脚本已内置 5 次取最小。

### 参考基线（本机，1024²，release 服务端）

| 指标 | 值 |
|---|---|
| 拖动每段同步耗时（中位数） | 0.8–0.9ms |
| 拖动每段（平均 / p95） | 0.95ms / 3.9ms |
| 首段（含创建笔迹） | 3.8–3.9ms |
| `pointerup` 提交 | 3.8–4.0ms |
| 到下一帧可见 | 9.8–17.6ms（受 60fps 帧时钟限制） |

每段约 0.9ms 相对于 16.7ms 的帧预算有约 **16 倍余量**，瓶颈在帧时钟而不是内核计算；
因此**不做**独立预览覆盖层（收益为 0，却要引入一层状态同步风险）。
