# 交接快照：CI `621ffc2b`（第 818 轮记录）

## 现状

| 项 | 值 |
|---|---|
| 提交 | `621ffc2b`（HEAD 已到 `45bfc830`）|
| 绿 | **10** |
| 跑 | **3**：`brush parity`、`criteria shard 3/6`、`criteria shard 5/6` |
| 红 | **0** |

已绿的 10 个：`rustfmt`、`clippy`、`test (stable)`、`test (beta)`、`wasm smoke`、
`GPU feature`、`criteria shard 1/6`、`2/6`、`4/6`、`6/6`。

## 这一批验证了什么（4 个真红）

| 判据 | 修于 | CI 证据 |
|---|---|---|
| `tool-surface-coverage` | `60170522` | shard 6 绿 |
| `tool-examples-doc-check` | `621ffc2b` | shard 6 绿 |
| `tool-export-cold-identical` | `a8ef177f` | shard 4 从「意外失败 1」转绿 |
| `tool-brush-params-doc` | `5ac6ffc7` + `f4aa5c6e` | shard 6 里是 ⊘（跳过，非失败）；第二处硬编码 `f4aa5c6e` **还没被 CI 验证** |

## 还没验的

- `f4aa5c6e`（`brush-params-doc` 的第二处硬编码 `!== 4` → 下界）
- `63ee1723`（`mode` 合并未实现时**明确报错**，替代静默覆盖）
- `f4aa5c6e` 之后的全部提交

## 已知红（不阻塞）

- `browser-layout`（视口依赖）、`browser-live-brush`、`browser-stroke-refresh`（CDP 环境）
- `tool-selection-mode-merge`（真缺陷，但**现在是「明确报错」不是「静默错值」**）
- `tool-taper-shrinks-ink`（引擎真缺陷）
- `brush parity`（`continue-on-error`）

## 下一步

1. 等 `shard 3/6`、`5/6` 的结果（预期绿）
2. 派发新 CI 验 `f4aa5c6e` 及之后
3. **布尔合并第 2–3 步**（`docs/design/brush-capability-gap.md` 三百二十七节）

## 验证数字（供布尔合并用）

- `mode=add` 现 `area=1600`，**应 `41600`**
- `mode=subtract` 现 `6400`，**应 `33600`**
- `mode=intersect` 现 `6400`（恰好对）
