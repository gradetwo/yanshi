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

## `phase4b-demo.sh` — 建议环路端到端演示

```bash
scripts/phase4b-demo.sh                       # 默认打 127.0.0.1:8110
BASE_URL=http://127.0.0.1:8080 scripts/phase4b-demo.sh
```

覆盖：人类标注 → AI 建议（含 patch）→ 只读步骤被拒 → 接受并重放（**像素指纹必须变化**）→
拒绝并记录原因 → 补丁内先建图层再绘制（**图层数必须增加**）。只创建**新文档**，不动既有数据。

## `browser-pixel-check.mjs` — 内核 vs 服务端逐像素自检（D1 口径）

```bash
node scripts/browser-pixel-check.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
```

读取编辑器内置自检结果：差异像素 ≤16 且最大通道差 ≤1 LSB 记为通过，退出码 0/1 便于接入 CI。

## `browser-drag-perf.mjs` — 拖动笔迹成本

```bash
node scripts/browser-drag-perf.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..." 20
```

输出每段的同步处理耗时（中位数/平均/p95/最大）、首段、`pointerup` 提交耗时与到下一帧的耗时。

> **测量口径提醒**：只测同步处理耗时。若在每段后 `await requestAnimationFrame`，
> 量到的主要是 60fps 帧边界（~16.7ms），真实工作量（~1ms）会被淹没 —— 首版探针就踩了这个坑。

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
