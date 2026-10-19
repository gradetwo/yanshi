# 交接快照（第 844 轮更新，取代旧版）

> 旧文件名保留（`HANDOFF-CI-621ffc2b.md`）是为了不产生两份漂移；内容已全部更新。
> **旧内容提到 `621ffc2b` 的地方已经过期**，以下为准。

## 代码现状

| 项 | 值 |
|---|---|
| `main` 与 `l4-sync` | `ef67e438`（同步）|
| 工作区 | 干净 |
| `cargo clippy --workspace --all-targets` | **0** |
| `cargo test -p yanshi-server --lib` | 82 passed / 0 failed |
| 判据 | 234 条 |
| 已知红有效条目 | 30 |

## 这一批已经做完的事（按轮次）

| 轮 | 提交 | 内容 |
|---|---|---|
| 755–759 | `1f47928b`/`da1d0386` | **PNG 双重展开回归**修复（CI 上 `test` 双绿确认）|
| 765 | `6e05fd30` | smart-select 补丁 + 4 个 TOOL_EXAMPLES 补齐 |
| 769 | `1abe1832` | smart-select 的 3 个 clippy 修复 |
| 775 | `d3b30ea2` | `E1027`：`create_selection` 的 `mode` 死参数（两处检查，`log.rs` 才是生效的）|
| 779 | `63f8c2ed` | `list_selections` 加 `area` / `area_exact` |
| 807 | `63ee1723` | `mode` 合并未实现时**明确报错**（替代静默覆盖）|
| 825 | `a8797d0c` | **位图布尔合井**（三条 mode 同路线）|
| 827 | `cab4ccf9` | bitmap 面积 ⇒ **三条 mode 精确命中** |
| 828 | `a14b0b77` | 变异验过 + 从已知红移除 |
| 830–831 | `798218a1`/`434ad9d5` | **4 条已知红补登记**（此前只写在 AGENTS.md 表格里）|
| 834 | `56d05261` | `scripts/pre-commit.sh`（把「设门」变成可执行）|
| 839 | `6dbc061d` | 端到端验证 `58400` 精确命中 + 告知外部 |

## 布尔合并的验证数字（可复现）

```
create_selection(selection_id="dress", shape=rect(100,60,200,300), mode="new")      ⇒ 60000 px
create_selection(selection_id="dress", mode="subtract", shape=rect(180,100,40,40)) ⇒ 凹口 1600
list_selections() ⇒ kind=bitmap｜area=58400｜area_exact=true      ← 期望 60000−1600
```

判据 `scripts/tool-selection-mode-merge.mjs` 的三条：

| mode | 实测 | 期望 |
|---|---|---|
| `subtract` | 33600 | 40000 − 6400 |
| `add` | 41600 | 40000 + 1600 |
| `intersect` | 6400 | 6400 |

**变异**（把 `subtract => (a-b).max(0.0)` 改成 `a`）⇒ `area=40000` ⇒ 判据退出码 1。

## 还没做完的

- 外部 §139 的建议①：`fill` / `fill_region` 加 `blend_mode` 参数
  （**功能已有**，走 `set_property(key="blend_mode", value="multiply")`；缺的只是 `fill` 上的方便入口）
- 外部 §139 的建议②：`adjust_levels` / `tone_curve` 工具（做提亮、对比度、色阶）

## ⚠ 三条流程规矩（血的教训）

### ③ **不要每轮查 CI** —— 单片要 15+ 分钟，而轮次间隔只有 ~15 秒

**实测**：`criteria shard 3/6` 从 `17:44:09Z` 起跑了 **15.6 分钟**才完成；
而我的轮次间隔约 **12–20 秒** ⇒ **查 50 次它才前进 2 分钟**。

**⇒ 我在 860–882 轮里反复报「未变」，全是无效劳动。**

**正确做法**：
- 一个 run 在跑时，**做别的事**（找文档矛盾、量数字、写规矩）
- 或隔 **20+ 轮**再查一次
- 判据：`gh run list --limit 1 --json status` 若还是 `in_progress`，**不要再看 `jobs`**

**⇒ 这是「用不可靠的方法验证」的第 3 种形态**：
① 取消了自己的 CI（840）② 用旧 run id（836）③ **在它没变化时反复查它（882）**



**① 不要取消 CI**（第 840 轮白等 6 轮）
每轮派发前不要跑「取消所有 pending」的循环 —— 它会把刚派发的也取消。
自检：`gh run list --limit 5 --json conclusion --jq '.[].conclusion' | sort | uniq -c`
⇒ 连续多个 `cancelled` 就先怀疑自己。

**② 提交前跑 `bash scripts/pre-commit.sh && git commit …`**
本场我在 clippy 红着时提交了 3 次，都是因为命令链用了 `;` 而不是 `&&`。
