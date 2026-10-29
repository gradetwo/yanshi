# 导出包两层口径：存储侧 vs 包内（第 4 轮第 3 期口径簇）

> 缺省导出的包**不是逐字节自含**，也不是数据丢失。判一个包能不能用，
> 看 `verify_package`，不要看导出回执的 `missing_body`，也不要数包里
> `blobs/` 的文件个数。
>
> 本文件里的实测数是实验结论（不是可算的命令），按仓库约定带时间戳：
> 截至 2026-10-29（第 4 轮第 3 期盒上探针与回归测试）。

## §0 两处纠错（后续引用以此为准，不许走回头路）

### 纠错一：水墨 2784 对 4654 不是口径差，是跨文档误比

- 2784 是另一文档 `ink-r3` 的**导出回执**，终版同文档 `ink-r3-final`
  两侧（导出回执与 check 计数）同为 4654。
- 同文档上导出与 check 口径一致。核对前须先对齐三项：
  `doc_id`、`blob_hash`、`bytes` —— 文档不同，一切数字都不可比。

### 纠错二：包络 `kind` 改名方向否决，改消费侧与文档

- `import_image` 包络是重放链的承重结构：缺省包导入时
  `blobs_replayed=120` 正是对 `import_image` 原子重建。
- 改名是跨导入、重放、核包、反作弊的迁移，风险大于收益。
- **不改存储侧**：`atoms.jsonl` 与 `list_atoms` 的笔触包络 `kind`
  恒为 `import_image`，`get_stroke_log` 的笔触 `kind` 恒为 `brush`，
  `draw_stroke` 保持自有 `kind`（只有 `brush_stroke` 走 `import_image`
  包络）。**改消费侧**：反作弊与核包一律按 `payload.source.kind` +
  `payload.origin` 判定，永不按包络 `kind` 名。

## §1 缺省导出语义

缺省（`include_bitmaps: false`）导出时，导出器对每个本体 blob 只做
三种处置之一：

| 处置 | 条件 | 包里有没有 |
| --- | --- | --- |
| 省掉（`omitted`） | 笔触本体，且配方可重放（`source.kind == brush` 且笔刷不读画布，且 `origin == drawn`，老日志无 `origin` 时按 source 带笔刷名派生） | 没有，import 时按配方重跑回来 |
| 恒装（`kept_no_recipe`） | 笔触本体但配方不可重放：读画布笔刷、媒体笔触（无 `source`，`origin == imported`） | 有 |
| 恒装（`kept_mismatch` / `kept_missing`） | 重放出来的哈希与日志不符，或本体在存储里就缺失 | 有（mismatch 时）/ 无（missing 时，导出失败） |

关键推论：

- 只有**可重放的笔触本体**会被省掉；照片与媒体本体缺省下恒装。
- 缺省包 + `import_project`（或解包到 `--root/` 后 `open_document`）
  恢复后与全量包逐位相同（水彩 240 笔：`blobs_replayed=120`、
  `unreplayable=0`；素描两支铅笔：0/262,144 像素不同）。
- 不走 import 重放、直接读包内 `blobs/` 的消费方，**仍见 omitted
  本体缺失** —— 那是缺省语义，不是损坏（见 §4 边界 P-C）。

## §2 两层口径对照表

### 存储侧口径（导出回执）vs 包内口径（verify_package）

| 栏位 | 存储侧（导出回执 `export_project`） | 包内（`verify_package`） |
| --- | --- | --- |
| `missing_body` | **成功时恒 0**：本体在存储里缺失会直接让导出失败，不会进回执 | 包内本体缺失数（去重后的哈希数）；照片类缺失 ⇒ `complete=false` |
| `omitted` | 导出器省掉的可重放本体数（去重后的哈希数） | 同口径：可重建缺失数；`brush + drawn` 的缺失记这里，**不判不合格** |
| `missing_derived` | 派生 tile 缺失（另计，不并入本体数） | 同口径 |
| `package_self_contained` | 导出回执新增字段：`omitted == 0` 才为真（逐字节自含） | —— |
| `caliber_note` | 导出回执新增字段：明说 `missing_body` 是存储侧口径，不能判包自含性，判包用 `verify_package` | —— |

**判包三句话**：

1. `verify_package.complete == true` ⇒ 包可用（omitted 只是可重建缺失）。
2. `verify_package.complete == false` 且 `missing_body > 0` ⇒ 真缺失
   （照片类无生成参数本体丢失，无法重建）。
3. 导出回执 `missing_body == 0` 说明不了任何包内结论 —— 它是存储侧
   口径，成功时恒 0。

### 计数口径（不要数错）

- `missing_body` / `omitted` 计**去重后的本体 blob 哈希数**，不是笔数：
  水彩 240 笔只对应 17 个哈希；12284 笔去重后为 7165 量级。
- 派生 tile 另计（`missing_derived`），不并入本体数。
- 导入回执是另一口径：`blobs_replayed` / `blobs_unreplayable` 按
  **引用原子数**计（水彩缺省包 `blobs_replayed=120`，对应 120 笔），
  与 `verify_package` 的去重哈希数不同，两边数字对不上是正常的。

## §3 kind 两层对照表（消费侧判定准则）

| 视角 | 笔触 `kind` | 说明 |
| --- | --- | --- |
| `atoms.jsonl`（包内日志） | `import_image`（brush_stroke） | 包络 kind，存储侧，**不作判定依据** |
| `list_atoms` | `import_image`（brush_stroke） | 同上；另回 `origin` / `stored_origin` / `brush` |
| `get_stroke_log` | `brush`（全部笔触，含媒体笔之外的笔） | 日志视角；媒体笔触不在此日志内（既有语义） |
| `draw_stroke`（矢量线） | 自有 `kind`（不走 `import_image` 包络） | 无 bitmap 引用，不参与本体口径 |

判定准则（反作弊与核包统一）：

- 只看 `payload.source.kind` + `payload.origin`：
  `source.kind == brush` 且 `origin == drawn` ⇒ 可重建缺失；
  `origin == imported`（照片 / 媒体）⇒ 真缺失。
- 老日志无 `payload.origin` 时，读时按 `origin_of` 派生兜底
  （source 带笔刷名 ⇒ drawn）。
- `get_stroke_log` 缺省只返 50 条（上限 500）：计数须读 `total`
  或传 `limit`，不要数 `returned`。

## §4 验收边界结论（第 4 轮第 3 期实测）

### P-A：读画布笔刷与媒体笔触在缺省下恒装（已实测）

512×512 文档：2B 铅笔（可重放）+ oil-03-paint.myb（smudge，读画布）
+ medium_stroke oil（媒体笔）各落一笔，缺省导出：

- `omitted = 1`（只有可重放铅笔被省掉），`kept_no_recipe = 2`
  （读画布笔与媒体笔本体均在包内），`kept_mismatch = 0`，
  `package_self_contained = false`。
- README「恒保留」的说法实测成立。

### P-C：直接读 blob 的消费方仍见缺失（已实测）

同一缺省包：omitted 哈希不在包内 `blobs/` 中；不走 import 重放的
消费方读包内 blob 仍得空白。缺省包不是逐字节自含 —— 这是语义，
不是损坏；判包用 `verify_package`，不要直接数包内文件。

### P-D：照片本体缺失是真实丢失（已实测，两侧都出声）

64×64 照片（`import_image` 工具导入，用户照片）经全量包抠掉本体后：

- `verify_package`：`complete = false`、`missing_body = 1`、
  `omitted = 0` —— 照片类无生成参数，缺失判不合格，不记可重建。
- 导入侧：全新工作区导入同一抠包 ⇒ `blobs_unreplayable = 1`、
  `blobs_replayed = 0`。此前导入侧是静默的（`materialize` 只对有
  `source` 的原子计数，无 `source` 的照片 / 媒体直接跳过，报 0）；
  本期已补：无重跑路径的本体缺失直接计入 `unreplayable`（只变回执
  计数，不写错字节、不改渲染）。
- 缺省包可重放结论**只对笔触成立**；真正的 `import_image` 工具导入
  （用户照片）本体缺失无法重建，那种 `missing_body` 是真实丢失。

### 16,000 笔大文档 import 重放（见探针报告）

> 本节数字以 `~/workspace/yanshi-package/paint-r4-p3/` 内探针报告为准：
> 100 笔/批 × 160 批 2B 铅笔（颜色逐笔微变，迫使本体哈希互不相同），
> 缺省导出 → 全新 root import → 两边 `export_png` sha 比对。

## §5 遗留项：独立核包脚本 `check_package.py` 不在仓库内

- 探针实际用来判「不合格」的 `check_package.py` 只存在于 release 包
  目录（如 `~/workspace/tools/yanshi-mcp-release-9045faeb/check_package.py`），
  其 verdict 为 `missing_body or missing_other` 即 exit 1，**完全没有
  omitted 概念** —— 它会把可重建的缺省包误判为不合格。
- 本期仓库补丁无法改它（不在仓库内），不要擅自把它 vendor 进仓库。
- 仓库内 canonical 核包工具是服务端 `verify_package`（已按本文件口径
  分流）。独立脚本的口径同步另案处理。
