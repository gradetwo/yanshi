# 第三方深度测试报告（2026-10-14 重跑版）逐条排查与处置

**被测报告**：`yanshi-deep-test-deliverables.zip` → `reports/yanshi-deep-test-report.md`
（被测提交 `e779ef5`，2026-10-13；而处置时 main 已是 `2145202c`，新了很多）
＋ 交付里自带一份补丁：`patches/yanshi-fixes.patch`（DEF-04，+21/-2）
＋ 用户另外给了一份 UI 补丁：`yanshi-ui-fixes-f214e63b.patch`（3 处）

**本文件的性质**：每一条都在**当前代码**上重新复验过（报告测的是旧提交 ⇒ 不能凭报告直接改），
并写明**处置**与**证据**；没做的如实列为"未做"，不写成"已解决"。

---

## 一、两份补丁的合并

| 补丁 | 内容 | 结果 |
| :--- | :--- | :--- |
| `yanshi-ui-fixes-f214e63b.patch` | ① 历史面板把带 `medium` 的 `import_image` 显示成「喷枪笔触(…)」；② 删掉顺序写反的 `#addLayer` 处理器；③ 删掉重复的「＋ 图层」按钮 | **已合**（干净应用） |
| `patches/yanshi-fixes.patch`（报告自带） | DEF-04：零 dab 兜底消息根因化 | **已合**（偏移 564 行） |

**UI 补丁的连带影响**（报告没提，但不处理就会打断判据）：
- **5 条判据脚本在点 `#addLayer`**，按钮一删它们就抛：`browser-stroke-refresh`、
  `browser-canvas-handfeel`、`browser-ui-check`，以及 `acceptance/path-operators.js`、
  `path-arity-guard.js` ⇒ **已全部改指仍然存在的 `#layerAdd`**（图层面板那颗 ＋）。
- 为什么收敛到 `#layerAdd` 而不是"修好旧的"：两条路功能完全重复，而 `#layerAdd` 与
  `#layerDuplicate`／`#layerDelete` 同处一个面板，且它**先刷后选**、与 `duplicate` 一致。
- `web/` 是 assets 的**同步副本** ⇒ 已跑 `pwa-sync-viewer.mjs`（同步后 `web/` 里 `addLayer`
  **零残留**）。注意：`web/index.html`、`web/sw.js` 在版本控制里，而 `web/viewer-app.js`、
  `web/.index.export.html` 是 gitignore 的生成物。

**判据实测（本机）**：`browser-stroke-refresh` **退出 0**（它走的正是"点新建图层 → 等切过去 →
落笔 → 断言画布更新"那条链）；`browser-canvas-handfeel` **退出 0**；`browser-ui-check` 仍在
**台账**里（既有缺陷，与本线无关）。

---

## 二、逐条复验与处置

### DEF-04（P2）零 dab 消息误导 —— **已修 ＋ 补判据**

复验（当前代码）：报告那条仍然存在（`size=400` 单点 ⇒ 消息说"试试给 size"）。
应用报告补丁后实测：

```
① size=400 单点 ：「size 已经是 400px ⇒ 问题不在缺参数，而在路径撑不满 dab 间距：本笔 1 个点、路径总长 0px…」
② size=300 两点 ：「……本笔 2 个点、路径总长 21px……」
③ 不给 size     ：「试试给 size，或换一支笔刷 ✓」（此时这条建议是对的 ⇒ 保留）
④ 正常笔画      ：ok=true（补丁没有把好路径弄坏）
```

处置：补丁合入 ＋ **新增判据 `scripts/tool-zero-dab-hint.mjs`**（报告只有补丁、没有判据 ⇒
本仓把它变成能红能绿的一条）。**变异**：把兜底改回旧文案 ⇒ ① 立刻红。

### UI-1（P3）适配按钮不联动缩放读数 —— **真 bug，已修 ＋ 补判据**

复验（当前代码）：**比报告说的更严重** —— 初始 `displayScale=0.7002`（70%）而读数与标签都写
`100%`（HTML 里的静态默认值）；点「适配」后**视图确实归位**，读数**仍是旧值**。

根因（读代码 ＋ 探针）：回填那两行在 `renderViewport()` 里，而那条函数在**服务端渲染模式**
（`kernelReady()=false`）下**提前 return**（要排队服务端补画）⇒ 只有内核模式会更新读数，
而报告与常规访问正是服务端渲染模式。

处置：抽成 `syncZoomReadout()` 一处，由**写 `displayScale` 的地方**调用（`clampViewport()` ＋
`renderViewport()`）⇒ 两条路、首次加载、适配、数值框全覆盖。

判据：**新增 `scripts/browser-zoom-readout.mjs`**，断的是**不变量**（`#zoomInput` 与 `#zoom`
必须等于 `round(displayScale×100)`），两种模式都测。实测：修前服务端渲染 70% vs 100% ⇒ 红；
修后两种模式全绿（内核模式按 `+` 键 0.8752 ⇒ 读数 88%，说明这条不变量真能抓漂移）。

### UI-2（P2）滚轮缩放无效 —— **刻意行为，不是缺陷**

复验：代码里有大段注释引用了**用户实测报告**（"web 画布很容易不小心就被放大缩小"／"适合只有
点击放大缩小键或输入具体数值才变化，避免手势或者触摸板误触放大缩小"）⇒ 滚轮**只吃掉事件**
（既不缩放，也不让页面滚）。更有力的证据：本仓自己的判据 `browser-ui-check.mjs` 里写着
**「滚轮缩放 0.73 → 0.73（必须不变）」**。

处置：**不改**（改了就是回退用户要求），只在文档里写明这是有意为之。缩放的显式入口是
适配／1:1／数值框／± 键 —— 本机实测数值框有效：设 150% ⇒ `displayScale` 0.7002 → 1.5，
读数与标签同步 150%。

### UI-4a（新变体）重载后撤销按钮即禁用 —— **已修**

复验：`state.remainingUndo` 只在**落笔之后**（`afterMutation`）与 `undo_last`／`redo_last` 的
响应里被写 ⇒ 新开的页面不知道服务端还有可撤销的笔；而 `updateUndoStatus()` 的口径是
"计数未知也算不可用"（保守）⇒ 按钮一直禁用。

处置：启动后调一次**已有的只读工具** `get_undo_status`（`refreshUndoStatusFromServer()` ——
不新造轮子）。实测（本机）：服务端 `remaining=2` ⇒ 页面加载后标签"可撤销 2 笔"、
两个撤销按钮 `disabled:false`（修前是禁用的）。

### DEF-05（P2）参数契约割裂 —— **部分根除 ＋ 其余记档**

复验：`delete_document` 只认 `document_id`（`tools.rs` 里 `require_str(args,"document_id")`），
而同一个面上的 `get_document` 等用 `doc_id` ⇒ Agent 按同一套命名推理必然撞一次。

**处置（含一次自我纠偏）**：我第一版**顺手给 `doc_id` 加了别名** ✗ —— 然后读 spec 才发现
**那个区分是有意的**：spec 原文写着"**注意它跟 `doc_id` 不是一回事**：`doc_id` 是**本次调用的
会话文档**（框架会按需创建），`document_id` 才是**要被删掉的那一份**" ⇒ 加别名会把
"删会话文档"与"删另一份"混起来 ✗ ⇒ **已撤回** ✓。

**∴ 那么报告这条的最终处置 ✗**：**它不是缺陷 ✗ 是**有理由的设计** ✓ —— 但**报错确实可以更好** ✓：
原来缺参只说"缺少必填参数 document_id" ✗ 猜了 `doc_id` 的人看不出为什么 ✓
⇒ **现在把 `document_id` 在 spec 里标成非必填 ✗ 由处理器出面拒绝 ✗ 错误里直接讲清两者区别
与怎么改** ✓。实测：
```
① 猜 doc_id      ⇒ 「**这把工具要的是 `document_id` ✗ 不是 `doc_id`** ⇒ ……要删当前会话这份，
                     就把同一个值填进 `document_id` ✓」
② 正确用 document_id ⇒ ok=True、deleted=True
③ 都不给          ⇒ 同上那条可照着改的消息
```
（**∴ 顺带一条实测 ✗**：**框架会把框架级参数（`doc_id`／`actor`／`session`）**合并进 `args`**✗
⇒ **∴ "调用方给没给 doc_id"在 `args` 里**分不出来** ✓ ⇒ **∴ 所以**错误消息**不要按它分支** ✓。）

**未做**：`draw_shape` 的**扁平 points** 仍不接受（它的契约是 `data.geometry.bbox` 嵌套）——
只记录、没改：改它要动几何参数的入口契约，影响面比这一轮的目的大。

### DEF-06（P2）缺省构建缺 wasm32 介质插件 —— **当前构建实测不缺 ＋ 记档**

复验（本机实测）：用**空 assets 目录**起服务（`--assets-dir /tmp/noassets`），
`medium_stroke(medium=oil)` 照样成功、`dabs=2`、对象里记 `medium:{id:oil,version:2}`
⇒ 服务端侧介质是**链进二进制**的，与报告说的"需要预置 assets/mediums"不同。
而 PWA／浏览器侧确实需要那些 `.wasm` —— 本仓已有 `scripts/mediums-sync.mjs`（生成器）
＋ `tool-pwa-assets.mjs`（判据，断言它们在）。

处置：不改代码，记为"构建文档事项"。报告建议的"运行时缺少介质时给显式诊断"**本轮没做**：
当前默认构建根本不缺，盲加诊断会是无的放矢。

### DEF-03（P1）8K 三种 OOM 形态 —— **未做，如实记档**

报告内容：形态一（同步整幅写）RSS 3.51GB 被 SIGKILL；形态二（预览过期时 `get_document`
触发整幅重渲）；形态三（浏览器与服务端 8K 共存）。报告给的修复方向：整幅操作前**内存预估** ＋
**结构化失败**，并把 `get_document` 的隐式整幅渲染与显式读数**解耦**。

处置：**本轮未做** —— 这是独立工程主题（预估、结构化失败、惰性加载），不是一份补丁的规模，
而本轮的目标是合补丁 ＋ 排查报告。

现状（已存在的缓解）：只读的 `get_log`／`get_render_status` 不触发渲染（报告自己也是改用它们
规避的）；MCP 的 `--no-wait` 异步路径峰值仅 1.04GB（**报告实测 —— 我本轮没复验，因此不写成
我的结论**）。

### 其余条目（报告已判定或与现状一致）

| 编号 | 报告状态 | 本轮复核 |
| :--- | :--- | :--- |
| WEB-1（P0 批量落笔丢失） | 上游已修 | **未复验**（报告已给五步证据链；本轮没有独立器材复现） |
| DEF-02（导出陈旧缓存） | 上游已修 | **未复验**（同上） |
| DEF-07（只读工具 GET） | 上游统一 405 | **未复验**（同上） |
| UI-3（每页 4xx） | 上游已修 | **一致**：`browser-ui-check` 实测 `requests_4xx: []`、`console_errors: []` |
| UI-5（F5 一致性） | PASS 8/8 | **一致**：`browser-stroke-refresh` 退出 0 |
| UI-6（图层操作无过渡反馈） | 不变 | **未改**（属体验打磨，报告自己也只标"不变"） |
| 残留观察：批量导入后 `thumbnail_current=false` | 良性陈旧 | **已是文档化行为**：`get_atom_render_status` 的工具描述写明"它是缓存，要新鲜请再调 `get_document`" ⇒ 观察者误判的风险已由这句话兜住 |

### 报告的建议里**本轮未做**的（如实列出）

- `diff_thumbnail` 官方工具（低分辨率缩略图 ＋ 参考图网格差值）—— 每轮独立测试都在重写这套
  拼装，确实值得内建，但本轮没做。
- `ensure_reference`（把参考图预置的 CAS 封装约定从"外部知识"变成工具）—— 没做。
- `job` 语义推广 ＋ 在工具描述里标注"哪些调用可能超预算转后台" —— 没做。
- 大文档加载惰性化 —— 没做（与 DEF-03 同一主题）。
