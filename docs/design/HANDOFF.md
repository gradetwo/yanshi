# YANSHI 交接摘要（第 244 轮）

> 上下文已极紧。这份文件是**下一轮接上所需的全部状态**，每条都带可核对的文件或数字。

## 一、当前状态

```
HEAD              = b39c341f（main == l4-sync）
外部复测 8 项      = 7 个四件齐 ＋ G02 5/7（第 3 项已自陈且被验）
判据              = tool-preview-read-canvas 10/10、tool-swatch-include-image 8/8
AGENTS.md         = 9 条行为规矩
CI 已知红         = criteria shard 4/6（tool-dwell，归引擎，AGENTS.md 有记录）
```

## 二、外部复测 8 项的定案（都在 `docs/design/brush-capability-gap.md`）

| 项 | 状态 | 关键数字 |
|---|---|---|
| D01 `dab_shape` | ✓ | `supported: ["circle","ellipse"]` |
| D02 细笔画文案 | ✓ | 「这一笔没落下任何像素」＋ 排除列表 |
| D03 `max_channel_delta` | ✓ | 四色 **0/1/31/205**（曾 255×4）；变异 ⇒ 254 |
| D04 画布高度 | ✓ | `innerHeight-96` = 481 vs 实际 312，**差 169 px** |
| D05 WS 空闲 | ✓ | 升级后清读超时 |
| D06 `--cache-bust` | ✓ | 只接受等号语法 |
| G01 `include_image` | ✓ | 9 支 ⇒ 8 张 ＋ `image_skipped_count=1` |
| G02 预览 | **5/7** | 见下 |

## 三、G02 的 7 个子项

| # | 子项 | 状态 |
|---|---|---|
| 1 | 公共参数定义 | ✓ `brush_params` |
| 2 | `background` | ✓ 纯色底 |
| 3 | `target-layer` | ✗ **未实现，但 `target_layer_supported=false` ＋ note 指向 `export_png` 的 `layer_id`，且被判据 ⑨⑩ 钉住** |
| 4 | `context` | ✗ 与 3 同源（渲染层） |
| 5 | 是否读取底图 | ✓ `read_canvas_effective`（现在恒 `false`） |
| 6 | 颜色空间 | ✓ `color_space="srgb"`（有源码证据） |
| 7 | 预览实际参数 ＋ 不落 atom | ✓ |

**渲染层边界（不许修，只定位）**：预览走 `render_region_raw`，读的是文档合成，读不到底图；
`export_png` 走文档合成，能看到（落盘 3235 字节，要 `path` 参数）。两者不是同一个读法。

## 四、9 条规矩（`AGENTS.md`，全从失败里来）

1. CDP `Runtime.evaluate` 必须双开关
2. 判据优先断**结构量**，不要断像素数
3. 断言必须**证明自己能红**，否则是安慰剂
4. 给自己写判据时**也读对方的示例代码**
5. 变异的粒度是**断言**，不是**判据**
6. 调不熟的接口**先打印完整响应**，不要猜参数
7. 改代码用字符串替换时**必须**带唯一性断言（`assert s.count(old) == N`）
8. **追加新节 ≠ 更新旧表**（台账表格要回头改）
9. 对方也有断言时，**核对断的是同一个取值集合**

## 五、下一轮该做什么

```
1. 先跑 bash /tmp/dg/check-report.sh 看 apple2011 的报告（变了就立刻逐条处理）
2. 收 CI：gh api .../actions/runs/$(cat /tmp/dg/ci_rid)/jobs
3. 若都没事 ⇒ 报"没有新数字"，不造活
```

## 六、环境红线（别追）

- `tool-dwell`（本机 +1／CI −5，方向相反，归引擎）
- `brush parity`（`continue-on-error`）
- `background_stroke_cost::other_ms`（CI 486 vs 本机 7.3）
- `browser-*` 判据在 CI 无 chromium ⇒ `exit(3)` 计为 skipped

## 七、这份文件**自己会过期**（第 8 条规矩，§70 提醒）

`§70` 里对方说过一句更准的话：过期的表格会变成「**看起来权威的过期数据**」——
它不会让人忽略它，而会让人**信它**。

所以这份文件里的**四张表**（状态、8 项定案、G02 的 7 项、9 条规矩）都有同一个义务：

```
**∴ 任何一项变了 ⇒ 必须**回头改这张表**✗**
   ⇒ **∴ 而**不是**在后面**追加一句"更新：…"**✗**（**∴ 那**会让表与更新**打架**✗**）
```

**触发条件**（照做即可，不需要判断）：

| 我改了什么 | 必须改哪张表 |
|---|---|
| 判据的断言数 | **一、状态** 里的「10/10 / 8/8」 |
| `AGENTS.md` 加/改规矩 | **四、9 条规矩** ＋ **一、状态** 里的「9 条」 |
| 外部复测某一项有进展 | **二、8 项定案** 对应行 |
| G02 某个子项变了 | **三、G02 的 7 项** 对应行 |
| `HEAD` 前进 | **一、状态** 里的 `HEAD` |

**自检**（一条命令，能在几秒内看出表是否过期）：

```bash
cd /home/crow/yanshi
echo "HEAD=$(git rev-parse --short HEAD)"
grep -c 'check(' scripts/tool-preview-read-canvas.mjs   # 应为 10
grep -c 'check(' scripts/tool-swatch-include-image.mjs   # 应为 8
grep -c '^## [^判]' AGENTS.md                            # 规矩段数
```
