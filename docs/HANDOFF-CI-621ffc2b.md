# 交接快照（第 968 轮更新，取代此前所有版本）

> 文件名保留是为了不产生两份漂移。**内容以此为准。**

## 代码现状

| 项 | 值 |
|---|---|
| `main` 与 `l4-sync` | **看命令**：`git rev-parse --short HEAD`（两分支应相同；本文件末次更新时是 `0d2a7b9b`）|
| 工作区 | 干净 |
| `cargo clippy --workspace --all-targets` | **0**（`scripts/pre-commit.sh` 保证）|
| 判据（CI 收集口径）| **282** |
| 已知红（`criteria-known-red.txt`）| **30** |

## ⚠ 三条**必须先读**的流程陷阱（都是本场血债）

**① 本机跑判据：用 `scripts/run-criteria-local.sh`，别手搓**

```bash
bash scripts/run-criteria-local.sh scripts/tool-xxx.mjs
```

它替你验二进制新鲜度 → 自动重建 → 起临时服务 → `trap` 清理；服务起不来就 `exit(3)`。

**代价 10 轮**：我用陈旧二进制测出「`[200,…]` 与 `[100,…]` 都给 255」的假现象，
然后花 947–956 去追一个**不存在的 bug**，期间对同一处代码给出过**三个互相矛盾的结论**。

**判别信号**：一个 bug 在本机稳定复现、而 CI 上任何一片都不红 ⇒ 先怀疑本机二进制陈旧。

**② 用 `scripts/watch-ci.sh` 等 CI，别每轮查**

```bash
bash scripts/watch-ci.sh
```

单片 `criteria` 可达 **19 分钟**，而轮次间隔只有 **12–80 秒** ⇒ 每轮查等于白查。
这个脚本只在**某个 job 的结论变化时**打印一行，其余时间静默。

**③ 提交前跑 `bash scripts/pre-commit.sh && git commit …`**
（本场我在 clippy 红着时提交过 3 次，都因为命令链用了 `;` 而非 `&&`。）

## 本场真正落地的交付（全部可复核）

| 项 | 证据 |
|---|---|
| **布尔合并**（`mode=add/subtract/intersect`）| 端到端 **58400**；三 mode 33600/41600/6400；判据 `tool-selection-mode-merge.mjs` |
| **`blend_mode` 9 种** | `normal/multiply/screen/overlay/darken/lighten/difference/addition/subtract` **9/9 ok**；判据 `tool-blend-mode-multiply.mjs`（203→162，sRGB 精确）|
| **`rect` 选区的 `w/h` 真 bug** | `create_selection` 存 `w/h`，`clip` 读 `width/height` ⇒ 任何 rect 选区都不能裁剪。901 修；判据 `tool-clip-to-selection-rect.mjs`（四形状，变异验过）|
| **`brush-params.md` 重新生成** | `check=0`（66 个规范名 / 9 组别名 / medium 13）；手改标题会造成**行序错位**，必须用 `write` 模式 |
| **`browser-pwa-local` 假红** | chromium 未就绪用了 `exit(1)` ⇒ 改 `exit(3)`（按 336 轮那条）|
| **性能断言加固** | `rect_coverage_parity` 的 `new < old` ⇒ `ratio < 1.15`（CI 实测差仅 1.8%）|
| **颜色 alpha 混合域** | `[200,200,200,1]` 的 `a=1` 曾被当字节 1 ⇒ alpha 1/255 ≈ 全透明。修后 **200/100 精确**；判据 `tool-color-array-domain.mjs` |
| `exit(3)` 说明覆盖 | **194/194** |
| `trackTemp` 覆盖 | **62/62** |

## 代码库健康指标（第 992 轮实测，可复核）

```bash
# 遗留标记：应为 0
for k in TODO FIXME XXX HACK; do
  echo "$k = $(grep -rnE "\b$k\b" --include=*.rs --include=*.mjs crates/*/src scripts/ | wc -l)"
done
# 乱码：应为 0
grep -rlP '\x{FFFD}' --include=*.rs --include=*.mjs --include=*.md --include=*.sh . | wc -l
```

| 项 | 实测 |
|---|---|
| `TODO` / `FIXME` / `XXX` / `HACK` | **0 / 0 / 0 / 0** |
| U+FFFD 乱码文件 | **0** |
| `待修` | 1（引用台账的注释，**不是**遗留标记）|
| `未实现` | 90（全是**正式的能力披露**，如「一般仿射与嵌套组尚未实现」）|
| `tools.rs` 的 `unwrap/expect` | 20，**全部带文案且都是不变量**（如 `expect("data 已是对象")`）|
| `trackTemp` 覆盖 | **按口径报**（见下），实测缺 **0** |
| 已知红条数 | **30**，且与 `run-criteria.sh` 的运行时判据**一致**（见下）|
| **能力列表的名字有效性** | `mediums` 5 ✓（1005 修）｜`filters` 14 ✓｜`retouch_kinds` 4 ✓｜`liquify_kinds` 3 ✓｜**`adjustments` 12/12 ✓** |

**能力列表的「名字有效」是什么意思**（第 1010 轮查齐）：

```bash
# 对每个报出来的名字，用它对应的工具调一次；「缺参数」算有效，「名字不认识」算无效
# mediums   ⇒ list_brushes(medium=X) 的支数必须**严格小于**全部（否则没筛选 = 无效）
# filters   ⇒ add_filter(filter_name=X) ⇒ 14/14 通过（dehaze 需 air，另有 filters_needing_params 说清）
# retouch   ⇒ retouch_kind_tools 的映射（heal → heal_stamp）⇒ 4/4 有效
# liquify   ⇒ liquify_<kind> 三个工具 ⇒ 3/3 有效（push 需 direction）
```

**⇒ 唯一真缺陷在 `mediums`**（1005 已修：3 个无效名 + 漏 3 个有效名）。
**⇒ 其余三个列表都准确。**

**已知红条数**（第 997 轮核实，两种口径零差异）：

```bash
# ① 只看『行首是判据名』
grep -cE '^[a-z][a-z0-9._-]+\.mjs' scripts/criteria-known-red.txt              # 30
# ② run-criteria.sh 的**运行时**判据（见 :17 的 is_known_red）
grep -cE '^[a-z][a-z0-9._-]+\.mjs[[:space:]]' scripts/criteria-known-red.txt   # 30
```

**⇒ 两口径都是 30、无差异行 ⇒ 「已知红 30 条」**准确**。**

**⇒ 注意文件有 **28159 行**（大部分是逐条解释），但只有 **30 行**是判据名**
⇒ 所以**别用 `wc -l`**，要用上面两个 `grep`。

**`trackTemp` 的三种口径**（第 994 轮查清，别再混报）：

```bash
grep -lE 'mkdtempSync|mkdtemp\(' scripts/tool-*.mjs | wc -l                    # 58
grep -lE 'mkdtempSync|mkdtemp\(' scripts/tool-*.mjs scripts/mcp-*.mjs | wc -l   # 60
grep -lE 'mkdtempSync|mkdtemp\(' scripts/*.mjs scripts/lib/*.mjs | wc -l        # 65
for f in $(grep -lE 'mkdtempSync|mkdtemp\(' scripts/tool-*.mjs scripts/mcp-*.mjs); do
  grep -q trackTemp "$f" || echo "缺：$f"
done                                                                             # 缺 0
```

**⇒ 本场曾报「62/62」—— **那个数对不上任何一种口径**（58／60／61／65）。**
**⇒ 正确说法是：**60 个判据造临时目录，全部有 `trackTemp`（缺 0）**。**
| `src` 里裸 `panic!`/`todo!` | 0（`panic!` 只在测试模块或 `unwrap_or_else` 里带文案）|

**⇒ 所以「未实现 90」不是债，是**诚实的边界说明** —— 与内核 1「静默降级是唯一对手」一致。**

## 还没做完的

- **`liquify_needing_params` / `retouch_needing_params` 缺失**（第 1012 轮发现，**不是 bug**）：
  `filters_needing_params` 存在（`{"dehaze":["air"]}`），但同类字段**只有这一个**。
  实测还需参数的：
  - `liquify_push` ⇒ 缺 `direction`
  - `clone_stamp` / `heal_stamp` ⇒ 缺 `source_offset`
  **⇒ 但错误信息**说清了**（「缺少必填参数 direction」）⇒ **不是静默降级** ✓
  ⇒ 所以这只是**「提前告知」的可用性缺口**（Agent 要先失败一次才知道），不是缺陷。

- **`fill` / `fill_region` 加 `blend_mode` 参数**（功能已有，走 `set_property`；缺方便入口）
- **`adjust_levels` / `tone_curve` 工具**（外部改用 `add_adjustment(levels)` 自行绕过，优先级降低）
- **等外部回答**：他们 `cv.fill` 的实现（解释 `193/49` vs `226/154` 的差）

## 外部报告状态

| § | 内容 | 我方结论 |
|---|---|---|
| §139 | 「没有 multiply」 | **不成立**——9 种都有，走 `set_property(key="blend_mode")` |
| §142 | `subtract` 不生效 | **他们自己撤回**（用法问题）；但我顺藤找到 `rect` 的 `w/h` 真 bug |
| §144 | 我的 §142 观察救了真 bug | 已答复 |
| §145 | `add_adjustment(levels)` 只在单层生效 | **他们自己找到正确用法**（挂到有内容的那一层）|
| §146 | 颜色混合域 | 已答复；**我的 `color.rs` 修复是真的有效**（完整链路验过）|
