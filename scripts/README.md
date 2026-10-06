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

## `browser-brush-preview.mjs` — 笔刷预览的浏览器验收（真实 Chromium）

用户原话："201 支笔刷只有一个名字 ⇒ 选笔全凭猜，**很不友好，web 上也是**"。这个脚本走编辑器自己的
真实路径（装载笔刷 → 选笔 → 看图 → 真拖两笔），判据四条：

1. 选中一支笔刷后 `#brushPreview` **真的加载出一张图**（`naturalWidth > 0`）——"src 设了但图是坏的 / 留个空框"会被抓住；
2. **换一支笔刷 ⇒ 图必须换**，且新图也真的加载成功；
3. **换颜色 ⇒ 图也必须换** —— 证明工具条上的颜色真的进了同一个工具（此前查看器传的是 `color: undefined`）；
4. **真拖两笔（红在 y=0.35、蓝在 y=0.65）⇒ 红带里只许有红、蓝带里只许有蓝**；
5. **平滑开关勾上 ⇒ 对象里 `data.smooth === true`**；
6. **平滑开关关掉 ⇒ 对象里 `data.smooth === false`** —— 第 6 条才是"能红"的那一半：
   只查"勾上 ⇒ true"挡不住"把 `smooth: true` 写死"（查看器里真有两条客户端路径是写死的，本轮已统一）；
   最后再断言全程**零控制台错误**。

> **必须强制重新取页面**：调试浏览器里往往已经有同一个 URL 的页面，只 `Page.navigate` 会命中内存缓存
> ⇒ 跑的是旧界面的 JS ⇒ 现象是"服务端明明改了、探针却一直看到旧行为"。脚本现在固定发一次
> `Page.reload({ignoreCache:true})`。

> 第 4 条刻意**不用**"画完有没有红像素"（上一轮的笔迹会骗过它），也**不用**"红像素有没有变多"
> （同一笔重画在同一处 ⇒ 像素逐字节相同）—— 它量的是"两个不同的输入 ⇒ 两个不同的输出"。
> 验证时请**用新文档**跑：老文档里已有上一轮的红蓝笔迹，判据就失去了区分力。

```bash
chromium --remote-debugging-port=9333 --headless=new about:blank
node scripts/browser-brush-preview.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
```

环境变量：`CDP_PORT`（缺省 9333）、`SHOT_DIR`（截图目录，缺省 `/tmp/yanshi-brush-preview`）。

## `browser-canvas-handfeel.mjs` — 画布手感（滚轮/缩放/图层选择/每笔抖动）

真实用户实测报告的四条：滚轮与触摸板**不许误触缩放**（缩放只走显式入口：适配 / 1:1 / **状态栏百分比输入框** / `+ - 0`）、
**新建图层后必须切到它**（同时查 `state.layerId` 与面板 `.layer-row.selected` —— 只查 state 会漏）、
以及**画一笔不许改动用户缩放**（"每笔抖一下"的真凶是 `preview.onload` 里无条件 `state.zoom = 1`）。
判据只用 `userZoom` ✓ —— `displayScale = fit × userZoom`，而 `fit` 会随布局重排自己变，
拿它当判据会把"布局重排"冤判成"缩放了"（实测 board 在 344×311 与 300×150 之间变过）。

```bash
node scripts/browser-canvas-handfeel.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
```

## `browser-asset-dock.mjs` — 素材浮层的浏览器验收（真实 Chromium）

用户要求："画笔区快捷方式、点开浮出来"。判据：① 打开前两张卡（调色板 / 纹理）在**原来的父节点**里；
② 点工具条上的「素材」⇒ 两张卡**搬进浮层**；③ **搬过去还能用** —— 点浮层里的色块，笔刷色真的变
（这条正是"把面板弄空 / 交互失效"的反面：搬的是**节点本身**，不是重建）；④ 点「收起」⇒ 两张卡回到
**原来的父节点、顺序不变**、浮层隐藏；⑤ 零控制台错误。同样**强制 `ignoreCache` 重载**。

```bash
node scripts/browser-asset-dock.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
```

## `browser-ui-check.mjs` — 查看器 UI 回归检查（真实 Chromium）

走用户同一条路径：新建图层 → 画一笔 → 断言**画布确有已绘制像素**、内容画布与覆盖层几何一致、
页面里没有覆盖用的 `#preview`、提交后缩略图自动刷新、**「新建」与「打开」确实切换文档**、
**舞台不宽于画布**（否则右侧是灰色死区、点击无效）、一笔只产生**一条** `draw_stroke` 日志
（防止重复广播）、右侧面板不溢出窗口、**缩放后的坐标映射**（在画布中心滚轮放大再落笔，
必须能在服务端渲染的文档中心找到该笔迹），以及**多级撤销/重做的栈深度转移**
（`{4,0} → {3,1} → {2,2} → {4,0}`；逐像素正确性由 `service_flow` 的确定性测试覆盖）。

> 注意：不要在同一轮里反复取**同一个** `/api/blob/...` 地址来比对像素 ——
> 服务端对 blob 响应带 `Cache-Control: immutable`，重复取会命中缓存、读到陈旧内容。它对应的是一组真实使用缺陷
（操作后画布空白、右边颜色不同且点击无效、缩略图不刷新），因此**每次改查看器都应跑**。

前置与 `browser-pixel-check.mjs` 相同（服务端 + 带远程调试的 Chromium）：

```bash
chromium --remote-debugging-port=9333 --headless=new about:blank
node scripts/browser-ui-check.mjs "http://127.0.0.1:8110/?doc=ui&token=<token>"
UI_DEBUG=1 UI_TRACE=1 node scripts/browser-ui-check.mjs "<url>"   # 额外打印页面日志与绘制打点
```

环境变量：`UI_TIMEOUT_MS`（总体超时，缺省 180s；调试目标无响应时脚本会以退出码 3 结束）。

## 用 `make` 隔离运行（推荐）

浏览器检查会对服务端做真实渲染，这些渲染产物会写进工作区的 CAS（见下节）。用
`make ui-check` / `make pixel-check` 可以在**独立临时工作区**（`mktemp` 目录 + 另一个端口）里跑，
日常实例的文件一个字节都不会变：

```bash
make ui-check       # 真实浏览器 UI 回归（临时工作区）
make pixel-check    # 真实浏览器逐像素自检（临时工作区）
```

实测：运行前后日常工作区的 blob 数量完全不变 ✓；辅助脚本是
`scripts/with-temp-server.sh`（起临时实例 → 把查看器地址作为最后一个参数传给被测命令 → 清理）。

## 验证脚本会写入渲染 blob

这些脚本对**正在运行的服务端**做真实的整幅/区域渲染，因此会在工作区的 CAS 里留下
**不被任何原子引用的渲染产物（孤儿）**。用工具 `collect_garbage` 清理（默认干跑）：

```bash
curl -s -X POST "http://127.0.0.1:8110/api/tools/collect_garbage?doc=<id>&token=<token>" -d '{}'
# 确认后真正回收（根集 = 所有文档的引用闭包 ∪ 活跃 Manifest，引用中的 blob 永不被删）
curl -s -X POST "http://127.0.0.1:8110/api/tools/collect_garbage?doc=<id>&token=<token>" \
     -d '{"confirm":true,"ttl_seconds":0}'
```

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

**它还做一次运行时字节比对**（不只是长度）：两端读**同一份场景文件**
`crates/yanshi-wasm/tests/data/wasm_smoke_scene.json`（脚本把同一个路径同时交给 node 与
原生测试），各算 RGBA 与 PNG 的 sha256，脚本比较两个摘要。
- **期望值不写死**：原生摘要由这一次运行现算（`crates/yanshi-wasm/tests/wasm_native_parity.rs`），
  脚本里没有任何"正确摘要"的常数。
- **不一致 ⇒ 非零退出并打印两个摘要**；**取不到原生摘要也非零退出**（缺一侧不能算通过）。
- 它覆盖"同一份输入下 wasm 与**本宿主**原生字节相同"；**不覆盖**真实浏览器
  （DOM/canvas/`ImageData`/传输）与其它架构的原生（跨平台另有 `parity-arm64` 作业）。

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
