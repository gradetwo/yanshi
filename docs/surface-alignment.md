# 三面对齐表：系统实现 / Web 查看器 / MCP

本文回答一个问题：**同一份能力，系统实现了没有、人在 Web 查看器里够不够得到、Agent 通过 MCP 看不看得见。**
三面各自"实际暴露了什么"都来自**运行命令或读源码**，不来自记忆；每个数字都写了出处。

* **日期**：2026-10-05（Asia/Shanghai）；**2026-10-26 追加**一节「追加（2026-10-26）— Web 面计数与逐行更正」，那一节的数字**重测于 `ef2494de` + 本地未提交改动**（`2a86cf7d` 那一轮的读数已被替换），其余各节仍是 10-05 的读数。**同日第二轮**再追加一节「追加（2026-10-26 第二轮）— 范围遮罩 / 渐变蒙版的 Web 入口，与五条登记」✓ 口径是 `0bddba26` + 本地未提交改动 ✓。
* **被测版本**：工具面在 `f5835cb`（`main`，工作树干净）上实测；截至 `3b71267`，`crates/yanshi-server/src/tools.rs`、`crates/yanshi-http/src/viewer.rs`、`crates/yanshi-mcp/src/lib.rs`、`docs/tools.md`、`docs/guide*.md`、`scripts/criteria-known-red.txt` 自 `f5835cb` 起**无改动**（`git diff --stat f5835cb..HEAD -- <上述文件>` 为空），并在 `3b71267` 的工作树上复核过全部计数，读数一致。
* **口径定义**（本文三张表都按它）：
  * `系统实现` = `ToolRegistry`（`crates/yanshi-server/src/tools.rs`）里注册的工具；`已在` / `计划中` / `无` 三态。
  * `Web 暴露` = **查看器页面的 JS 真的会调到**的 `/api/tools` 工具名（`callTool` / `callToolChecked` 的直接实参，以及经 `convertCheckedObjects(...)` 这类助手中转的工具名）。**不含**只在注释或 Rust 测试字符串里出现的名字。
  * `MCP 暴露` = `yanshi-mcp` 在**该 profile 启用**时是否列出该工具；`默认` 一栏特指**不带 `--profile` 启动**（= 只有 core，见 `crates/yanshi-mcp/src/lib.rs:86`）。
  * `状态判定` 取值与含义：
    * `三方齐` = 系统已在 **且** Web 能调 **且** MCP **默认（core）**能调；
    * `系统+Web` = 系统已在、Web 能调，但 **MCP 默认看不到**（需要显式 `--profile`）；
    * `系统+MCP` = 系统已在、MCP 默认能调，但 Web 没有入口；
    * `仅系统` = 系统已在，两个面都没有入口；
    * `计划中` = 系统尚未实现。
  * **注意**："MCP 默认无" ≠ "MCP 不可达"：除 `semantic` 外，HTTP 面的每个工具都能靠 `--profile <组>` 打开。本文把"默认口径"单列，因为那才是 Agent 开箱看到的东西。

---

## 表 1 — 汇总计数

> ⚠️ **本表的 MCP 与 Web 两行已过期**（缺省 profile 集在 2026-10-24 被改成 `Profile::DEFAULT`，本轮又给查看器加了入口）。当前实测数字见文末「追加（2026-10-26）」一节；本表保留原样，是为了留下"哪个版本读到什么"的证据，不是为了给现状。

| 面 | 数量 | 口径 | 证据 |
|---|---|---|---|
| 系统实现（`ToolRegistry` / HTTP `/api/tools`） | HTTP 默认 **181**；注册表/文档口径 **169**（core **109**） | ① HTTP 口径 = 服务端默认启用的 8 个 profile 并集的工具数；② 注册表口径 = `docs/tools.md` 写死的"全部已实现组"数，它用的 profile 集只有 6 个（**不含 changeset / conflict**） | ① `curl -s http://127.0.0.1:15900/api/tools` ⇒ `profiles` 恰为 `core,history,changeset,retouch,conflict,annotation,collab,structure`（8 个）、`tools` 长度 181；默认集见 `crates/yanshi-http/src/server.rs:97-105`，profile 全集见 `crates/yanshi-server/src/tools.rs:63-72`；② `docs/tools.md:24`「**72 core tools** … **128 tools in total**」＋`crates/yanshi-server/tests/tool_inventory.rs:12-19`（6 组）；实测 `--profiles core,history,retouch,annotation,collab,structure` ⇒ **128** |
| Web 查看器 | 工具条 **22** 个按钮；真正调到的服务端工具 **78** 个 | ① 工具条 = `TOOL_DEFS` 数组条目数（20 个 `{tool:…}` + 2 个 `{id:…}`；`pan` / `eyedropper` / `move_layer` 是纯客户端伪工具）；② 调用面 = 查看器 JS 调用的**去重** `/api/tools` 工具名（直接 `callTool` / `callToolChecked` + 助手 `convertCheckedObjects` 中转的 `convert_to_shape` / `convert_to_path`，以及动态分支 `viewer.rs:3396/3404/6002` 的 10 个） | ① `crates/yanshi-http/src/viewer.rs:6816-6843`，按钮由 `renderToolStrip()`（`:6844`）填充 `#tools`（`:505`）；② 提取命令见文末，其中 **34** 个在 core、**44** 个不在 core |
| MCP | 默认 **109**；全开 **181** | ① 默认 = 不带 `--profile` 启动（`profiles = [Profile::Core]`）；② 全开 = `--profiles all`（= `Profile::IMPLEMENTED`，8 组，**不含 semantic**） | ① `crates/yanshi-mcp/src/lib.rs:86`；实测 `target/debug/yanshi-mcp --list-tools` ⇒ 109；② `crates/yanshi-server/src/tools.rs:100-109`；实测 `--profiles all` ⇒ 181 |

**由表 1 直接推出的两个覆盖率**（来自 `scripts/tool-surface-coverage.mjs` 的打印）：
`MCP 默认 109 / HTTP 181 = 60.2%`；`MCP 默认 ∩ Web 调用 = 34 / 78 = 43.6%`。

**两个口径不一致（本次实测，非引用）**：
`docs/tools.md:24` 的"every implemented group = 125"用的是 6 组口径，而服务端默认启用了 8 组（多 `changeset`、`conflict`）= 137；
`docs/guide.md:7` 与 `docs/guide.zh-CN.md:7` 写的是 137（与 HTTP 面一致）。同一句"全部已实现组"在两份文档里是 125 与 137。
`tool_inventory.rs:12-19` 的注释自称"与 `yanshi-http` 的默认一致"，但 `server.rs:97-105` 是 8 组。

---

## 表 2 — 对齐表

> `Web 暴露` 的 `viewer.rs:NNNN` 是该工具在查看器源码里**被调用**的行；`MCP 暴露` 的组名见 `tools.rs` 的 `Profile`。
> `缺口原因` 里点名 `已知红` 的，理由逐字取自 `scripts/criteria-known-red.txt`（8 条）。

| 能力/功能 | 系统实现（已在/计划中/无） | Web 暴露 | MCP 暴露 | 状态判定 | 缺口原因 / 计划或下一步 |
|---|---|---|---|---|---|
| 文档与图层/对象基础读写 | 已在 | 有：`create_layer`@viewer.rs:2823、`list_layers`@2961、`list_objects`@1591、`update_layer`@5257、`delete_layer`@5211、`delete_object`@3947、`reorder_layers`@5278、`duplicate_layer`@5305 | 默认有（core：前 7 个）；`duplicate_layer` 需 `--profile structure` | 三方齐 | `duplicate_layer` 默认 MCP 无；`get_object` / `update_object` / `new_document` / `list_documents` / `get_document` / `get_state` 在 core 但 Web 未调用（页面自建状态）。下一步：把 `get_object` / `update_object` 接进对象面板 |
| 撤销 / 重做 / 历史查询 | 已在 | 有：`undo_last`@viewer.rs:3285、`redo_last`@3307、`revert_to`@3622、`get_log`@3587、`get_atom`@3567、`batch`@6538 | 默认有（core：以上全部 + `revert` / `reapply`）；`find_atom` / `get_diff` / `get_object_history` / `get_ancestors` / `get_descendants` / `declare_head` 需 `--profile history` | 三方齐 | Web 不调 `revert` / `reapply`（`viewer.rs:1443` 只在注释里、`:8600-8601` 只在 Rust 测试断言的字符串里）；history 的 5 个检索/图遍历工具既无 UI、默认 MCP 也无。原因：MCP 默认只 core（`lib.rs:86`）。下一步：先由用户决定是否加宽 MCP 默认 |
| 检查点（history） | 已在 | 有：`checkpoint`@viewer.rs:3888、`get_checkpoints`@3878、`restore_checkpoint`@3859 | 有，但需 `--profile history`（默认 core 无） | 系统+Web | 原因：`history` 不在默认 profile（`lib.rs:86`）；`tool-surface-coverage.mjs` 实测 "MCP 默认（core）：69 个 ⇒ 覆盖 HTTP 的 50.4%"。下一步：并入默认与否是用户决策 |
| 变更集 / 事务 / 离线暂存（changeset） | 已在 | 有：`begin_changeset`@viewer.rs:3536、`commit_changeset`@3542、`abort_changeset`@3548、`get_changesets`@3527、`revert_changeset`@3511 | 有，但需 `--profile changeset`（默认无）；`begin_transaction` / `commit_transaction` / `apply_stash` / `discard_stash` / `list_stashes` / `submit_offline` 无 UI | 系统+Web | 原因同上一行；此外事务与悬空变更集（设计 12.4）在 Web 完全没有入口。下一步：至少给事务与 stash 一个面板入口，并等 MCP 默认决策 |
| 标注（annotation，独立 append-only 通道） | 已在 | 有：`create_annotation`@viewer.rs:4482、`list_annotations`@4467、`update_annotation`@3782、`delete_annotation`@3806、`reject_annotation`@3816、`resolve_annotation`@3798 | 有，但需 `--profile annotation`（默认无）；`get_annotation` 无 UI | 系统+Web | 原因同上。下一步：等 MCP 默认决策 |
| 协作建议与评论（collab） | 已在 | 有：`list_suggestions`@viewer.rs:4184、`preview_suggestion`@4134、`accept_suggestion`@4153、`reject_suggestion`@4168、`comment`@4070、`list_comments`@4039 | 有，但需 `--profile collab`（默认无）；`suggest` / `accept_suggestions` / `reject_suggestions` 无 UI | 系统+Web | 原因同上。"提出建议（`suggest`）"与批量接受/拒绝只在工具层有；查看器只做单条预览/接受/拒绝。下一步：等 MCP 默认决策 |
| 对象分组与实例（设计 9.4） | 已在 | 部分：`create_group`@viewer.rs:3993、`create_instance`@3977；`add_to_group` / `remove_from_group` / `set_group_transform` / `detach_instance` / `link_to_master` / `get_resolved_state` / `update_sync_policy` / `update_override` / `get_dependency_graph` 无 | 默认有（core：以上全部，即整组都在 core） | 三方齐 | 9.4 的依赖图、实例同步策略与覆盖变换在 MCP 有、Web 无。下一步：实例面板补 `get_resolved_state` / `update_sync_policy` |
| 选区（设计 4.4） | 已在 | 有：`create_selection`@viewer.rs:5800、`list_selections`@5747、`delete_selection`@5827 | 有，但需 `--profile structure`（默认无） | 系统+Web | 原因：`structure` 非默认。下一步：等 MCP 默认决策 |
| 蒙版 | 已在 | 有：`create_mask`@viewer.rs:5904 + `set_property`@5914 挂载 | 有，但需 `--profile structure`（默认无） | 系统+Web | 原因同上 |
| 矢量路径/形状互转（设计 792） | 已在 | 有：`path_edit`@viewer.rs:4329、`convert_to_shape`@4435、`convert_to_path`@4447（经 `convertCheckedObjects` 中转） | 有，但需 `--profile structure`（默认无） | 系统+Web | 原因同上。附带实测：`path_edit` 的 summary 写「本片只实现 reverse/close/join」（`tools.rs:1513`），而参数已含 `merge/split`（`tools.rs:1520`）、处理器也含 `merge/split/boolean`（`tools.rs:7418-7430`）⇒ **工具描述过时**；现有判据没覆盖这一句。下一步：同步 summary |
| 修图：仿制 / 修复 / 涂抹 / 补丁（retouch） | 已在 | 有：`clone_stamp` / `heal_stamp` / `smudge`@viewer.rs:6002、`resample`@4387；`patch` / `estimate_dehaze` 无 UI | 有，但需 `--profile retouch`（默认无） | 系统+Web | 原因：`retouch` 非默认。下一步：等 MCP 默认决策；`patch` / `estimate_dehaze` 是否要 UI 属产品决策 |
| 液化（retouch） | 已在 | 有：`liquify_push` / `liquify_twirl` / `liquify_pinch`@viewer.rs:6002 | 有，但需 `--profile retouch`（默认无） | 系统+Web | 原因同上 |
| 调整与滤镜（retouch） | 已在 | 有：`add_adjustment` / `add_filter`@viewer.rs:3404、`update_adjustment` / `update_filter`@3396、`list_effects`@3428 | 有，但需 `--profile retouch`（默认无） | 系统+Web | 原因同上 |
| 参考图（`set_reference` / `clear_reference`） | 已在（core） | **有**（本行原文"无"已过期 ⇒ 见"追加"一节 ✓）：上传 / 清除按钮 + 不透明度 + `x/y/w/h` 位置框，改完即生效 | 默认有（core：`set_reference` / `clear_reference` / `get_preferences`） | 三方齐 | 位置与不透明度以服务端偏好为唯一真源 ⇒ 页面与 MCP 读同一份数据 ✓（实测几何 ±2px 内相符 ✓）。剩余限制：裸像素 blob 换浏览器解不开（页面会说明并提示重传 ✓）、偏好是工作区级不是文档级 ⇒ 两条均入"需要用户决策"第 7 / 8 条 |
| 画布采样与区域分析（`sample_color` / `analyze_region`） | 已在（core） | **无**：吸管走客户端 WASM 内核 `state.kernel.render_region_rgba`（viewer.rs:5642-5664），不调 `sample_color` / `analyze_region` | 默认有（core：`sample_color`、`analyze_region`） | 系统+MCP | 两条取色路径并存（客户端内核 vs 服务端 `sample_color`，`tools.rs:963` 规格、`:2642` 调度、`:2795` 实现）⇒ 可能漂移。下一步：吸管改调 `sample_color`，或加一条"两条路径读数一致"的判据 |
| 笔刷库与预览 | 已在 | 有：`brush_preview`@viewer.rs:7264（入库图缺失时的回退）、`list_assets`@4956、`list_palette_colors`@5022；入库预览 `/brush-previews/`@7137,7254 | 默认有（core：`brush_preview` / `list_assets` / `list_palette_colors` / `save_palette` / `set_brush_dynamics`）；`list_brushes` 需 `--profile structure` | 三方齐 | `list_brushes`（内置光栅笔刷参数）Web 无、默认 MCP 也无；`save_palette` / `set_brush_dynamics` 无 UI。另一条实测见"本次未测"：`browser-brush-preview-local.mjs` 的已知红理由已过时 |
| 介质插件模拟（油画/水彩/马克笔/铅笔/像素） | 已在（`medium_stroke` core；`assets/mediums` 6 个 wasm） | 有，但**走另一条实现**：浏览器 WASM 插件出 RGBA，再 `import_image` 入库（viewer.rs:2945-2980）；**未调用** `medium_stroke` | 默认有（core：`medium_stroke`） | 三方齐 | 同一能力有两份实现（浏览器 WASM 与服务端插件），Web 与 MCP 走的不是同一条路；两者是否逐字节一致**未测**。相关已知红：`tool-impasto-plateau.mjs`（报告 2.3：修法已把偏差从 34.85% 降到 0.59%，差 0.09 个百分点，待产品决策）。下一步：先测两条路的中等笔迹是否同像素 |
| 存储维护与工程包 | 已在 | 有：`blob_gc`@viewer.rs:4229、`export_project`@5098、`import_project`@5124；`collect_garbage` 无 UI | 默认有（core：`export_project` / `import_project`）；`blob_gc` 需 `history`、`collect_garbage` 需 `structure` | 三方齐 | 已知红 `tool-archive-bloat.mjs`：「报告 2.6：覆盖 12 次后归档 1.69×，需日志压缩/快照（架构级）」。下一步：`collect_garbage` 面板入口与归档压缩同属待办 |
| 作业与批处理 | 已在（core） | 部分：`batch`@viewer.rs:6538；`cancel_job` / `get_job` 无 UI | 默认有（core：`batch` / `cancel_job` / `get_job`） | 三方齐 | 长作业的进度与取消在页面无处可见。下一步：状态栏接入 `get_job` / `cancel_job` |
| 导入导出 | 已在（core） | 有：`import_image`@viewer.rs:2980、`import_psd`@3094、`import_project`@5124、`export_project`@5098；**无** `export_png`（页面用本地 canvas `toBlob` 导出，失败才退回 `render_region`，viewer.rs:8047-8070） | 默认有（core：`export_png` / `import_image` / `import_psd` / `import_project` / `import_asset`） | 三方齐 | `export_png`（任意尺寸、直接落盘、不受 512px 限制）在 Web 无入口；`import_asset`（.myb/.png/.json 资产入库）也无 UI。下一步：导出对话框增加"服务端导出"选项 |
| 对象内容 / 笔触重编辑 | 已在（core） | 部分：`update_stroke`@viewer.rs:4360；`replace_object_data` / `update_object` / `set_layer_blend` 无 UI | 默认有（core：`update_stroke` / `replace_object_data` / `update_object` / `set_layer_blend`） | 三方齐 | "改内容不新建对象"的 `supersede` 入口（`replace_object_data`，`docs/design/implementation-notes.md:2338`）与混合模式（`set_layer_blend`，罩染用 multiply）在 Web 无入口。下一步：对象面板接入 |
| 冲突解决（设计 12.3） | 已在（`resolve_conflict`，归 `conflict` 组） | 无 | 有，但需 `--profile conflict`（默认无） | 仅系统 | `conflict` 是 8 组里最小的一组（1 个工具），既不在默认 MCP、也无 UI。下一步：等 MCP 默认决策；是否需要人工冲突面板属产品决策 |
| 语义工具（6 个，`semantic` 组） | **计划中**（未注册） | 无 | 无（`--profile semantic` 也只列出 core 的 69 个） | 计划中 | `docs/semantic-tools.md`：「Status: reserved, deliberately not developed … no code, no tool registration and no network path exist」；六个名字是 `analyze_image` / `inpaint_region` / `generate_mask_from_prompt` / `semantic_replace` / `vectorize_stroke` / `apply_style_transfer`。实测 `/api/tools` 与 `--profile semantic` 均无这些名字。下一步：是否实现、以及 provider 契约属用户决策 |
| 离线绘制 / 客户端内核（WASM） | 已在（`crates/yanshi-wasm`） | 有：查看器用 `window.yanshiKernel`，离线时本地出图；判据 `browser-offline-draw.mjs` / `browser-offline-export.mjs` / `browser-offline-shell.mjs` | 无（不是 `/api/tools` 工具；离线提交对应 `submit_offline`，属 `changeset`，默认 MCP 无） | 系统+Web | 服务端工具面没有"客户端内核"这一项，属结构差异而非缺漏。下一步：无需动作，除非要让 Agent 也能复现离线重放 |
| 与参考图比 ΔE（`compare_with_reference`） | 已在但**挂错位置** | 无 | 默认 core 的 `gradient_fill` 声明了该参数，但默认 core 的 `analyze_region` 才消费它 | 系统+MCP | 已知红 `tool-reference-delta-e.mjs`：「能力挂错位置：`compare_with_reference` 在 `gradient_fill` 上而不是 `analyze_region` 上；而 `gradient_fill` 会画画 ⇒ 无法在不改画面的前提下问 ΔE ⇒ 需产品决策（挪能力或加只读工具）。**已验证**：产品报错 `analyze_region 不接受参数 compare_with_reference；可用参数：region`」。本次复跑该判据仍红（`自比 ⇒ undefined`）。代码位置：参数声明 `tools.rs:2332`（属 `gradient_fill`，规格起于 `:2317`），实现读取 `read_analyze_region`（`tools.rs:3127`，用点 `:3178-3181`）。下一步：用户决定"挪参数"还是"加只读工具" |

---

## 追加（2026-10-26）— Web 面计数与逐行更正

上面两张表的数字测于 `f5835cb`（2026-10-05）。此后上游把 HTTP/MCP 的缺省 profile 集从"core 一组"改成 `Profile::DEFAULT`（`crates/yanshi-server/src/tools.rs:100-155`、`crates/yanshi-http/src/server.rs:152-161`，并由 `server.rs:4364-4382` 的断言钉死「http 缺省组成必须是 `Profile::DEFAULT`」）；本轮又给查看器补了锚点、风格预设、蒙版涂抹三块入口。**所以表 1 / 表 2 里的 MCP 计数与 Web 计数都已过期**，本节是**当前基线 `ef2494de` + 本地未提交改动**的实测（`2a86cf7d` 那一轮的读数已被下面的数字取代），口径与上面一致（「JS 真的会调到」= `callTool` / `callToolChecked` / `callToolQuiet` / `window.yanshiCallTool` 的字符串实参去重）。

| 项 | 2026-10-05 | 2026-10-26 实测 | 出处 |
|---|---|---|---|
| MCP 缺省工具数 | 109（= core） | **205**（缺省 9 组：core,history,changeset,retouch,conflict,annotation,collab,structure,photo） | `target/release/yanshi-mcp --list-tools \| python3 -c 'import sys,json;print(len(json.load(sys.stdin)["tools"]))'`（测于 `ef2494de` + 本地改动，二进制重新构建 ✓） |
| 系统实现（全开） | 181 | **205**（`--profile all` = `Profile::IMPLEMENTED` 15 组） | 实测 `curl -s http://127.0.0.1:15900/api/tools`（`--profile all`）⇒ `profiles` 15 组、`tools` 205；`core` 单组 = **125**。**HTTP 缺省面与全开同数**（另起一台不带 `--profile` 的实例实测 = 9 组 / 205 ✓）⇒ 介质与 svg 那 6 组只放行资产/预设，不放行新工具 |
| Web 工具条条目 | 22（20 真工具） | **25**（23 真工具；新增 `anchor` / `mask_paint` / `mask_erase`） | `const TOOL_DEFS`（`crates/yanshi-http/assets/viewer-app.js:9551`）；注意 JS 已从 `viewer.rs` 拆到 `assets/viewer-app.js` ⇒ 本文早先"行号在 viewer.rs"的出处整体平移 |
| Web 真正调到的工具 | 78（另一算法 83） | **93**（补 `get_histogram` / `revert_group` 前 90，再补 `reorder_objects` 前 92） | 提取命令见文末第 8 条 |
| Web 调用 ∩ MCP 缺省 | 34 / 78 | **93 / 93**（Web 用到的每一个工具都在缺省面内）| 与 `/api/tools` 返回的名字求交，`web - http = ∅` ✓ |
| 缺面覆盖率（Web / MCP 缺省） | 43.6%（另一口径 31%） | **45.4%**（93 / 205） | 由上两行相除 |

**表 2 里本轮失效的行**（只列结论，逐行重测留待下一轮三面对账）：

* 「参考图」行：原文写 Web **无** `set_reference` / `clear_reference` 入口 ⇒ **已过期**（上传/清除两个按钮都在）；**2026-10-26 第二轮把这一行整体推到"三方齐"** ✓：
  * **位置**：卡片新增 `x / y / w / h` 四个框（`viewer.rs` 的 `#cardReference`）⇒ 真的发 `set_reference.position` ✓，叠加层按它摆（`applyReferenceGeometry()`，与选区/参考线同一时刻重算 ✓）。此前服务端收 position、页面只能铺满 ⇒ **同一份数据两种读法** ✗ 已消除。
  * **改数字即生效**：五个输入框挂 `change` ⇒ 重发一次 `set_reference`（同一条工具路 ✓）。此前它们只在"下一次上传"被读到 ⇒ 控件存在但不反应 ✗。
  * **跨会话可见**：`<img>` 解不开存储里的裸 RGBA（`/api/blob` 的 `sniff_blob_mime` 对裸像素只给 octet-stream ✓）⇒ 上传时把原始 dataURL 另存进 `localStorage`（上限 1.5MB ✓，超了就在信息条说明），F5 之后参考图不再消失 ✓。
  * **MCP/HTTP 设的参考图，页面看得见**：位置/不透明度一律以服务端偏好为准（实测 `set_reference` 后重开页面 ⇒ 叠加层几何与 `reference.position` 相符 ✓）。
  * 仍缺：参考图的**混合模式 / 可见性开关 / 锁定**（服务端也没有这三样 ⇒ 属"计划中"而非"web 缺"）；以及**裸像素无法跨浏览器显示**这条结构性限制（要彻底解决需服务端存原图或给出尺寸 ⇒ 见「需要用户决策」）。
* **判据自身的假红（本轮发现并修）** ✓：`scripts/browser-reference-overlay.mjs` 写死 `blob_hash = sha256:aaaa…`，而 `set_reference` 自上游 `84d15439` 起会校验"blob 必须真在存储里" ⇒ 该判据**每次都停在 exit 3**（"我自己坏了"）✓ —— 也就是说**"参考图看得见"那一半其实已经六天没被真正判过** ✗。改法：先用**只读**的 `render_region` 产出当前文档的真 blob 再设 ⇒ 判据恢复真判（本轮实测 exit 0 ✓）。同类写死假 hash 的判据已排查：`tool-pwa-gc-references` / `tool-gpu-pure-parity` 的假 hash 属"引用计数/参数形状"用途 ⇒ 不受该校验影响 ✓（两者本轮复跑 exit 0）。
* **参考图偏好是工作区级、不是文档级** ✓（实测：给文档 A 设 `reference.position` 后，新建文档 B 的 `get_preferences` 回执里 `where = <工作区根>/preferences.json`，B 直接读到同一份 ⇒ 换文档时参考图与它的矩形会跟过去 ✗）。这是服务端数据模型如此（`clear_reference` 才能清 ⇒ 页面「清除参考图」按钮因此是**全局**清除）。要不要改成按文档存 ⇒ 见「需要用户决策」。
* **本轮又补掉两个"系统 + MCP 缺省有、人够不到"的缺口** ✓（验证一律以**服务端返回**为准，不比"看起来差不多"✗）：
  * `get_histogram` ⇒ 诊断面板新增「直方图」卡：作用域与工具规格一致（整文档 / 当前图层 / 只统计选区 ✓，选区来自 `list_selections` 写进 `state.selectionShape` 的服务端事实 ✓）；**读数行显示的 `scope` 取服务端回的那个**（勾了选区又切图层 ⇒ 回 `layer_region`，页面照写"图层×选区"✓，不拿界面控件状态猜 ✗）。实测页面读数与直接调 `get_histogram` **逐字段相等**（`pixels` / `scope` / `summary.luma` 的均值·中位·p5·p95 ✓）。
  * `revert_group` ⇒ 对象工具条新增「整组撤销」：只要 `group_id`，不需要先勾选对象 ✓（提示里写清了这点，否则会以为要勾 ✓）。**`reverted_count` 必须出口报数** ✓：该工具对"这组没有可撤的原子"回的是 `ok:true` + `note`（幂等，不算错误 ✓）⇒ 页面只看 `ok` 就成了"点了没反应还显示成功"✗，故撤了几笔/一笔没撤都要说明 ✓。
  * `reorder_objects` ⇒ 对象工具条新增「对象上移 / 对象下移」（`ef2494de` 这一轮补 ✓）。三条硬约束都必须在页面侧成立，否则服务端直接打回 ✗：
    * **一次只动一个对象**：`reorder_objects` 要求提交**完整**的图层 z 序（`fold.rs` 校验"期望 N 个对象，实际 M 个；缺失：…"✓）⇒ 勾选数 ≠ 1 时页面出声拒绝，不去猜该动哪个 ✓。
    * **必须带 `include_hidden: true` 去读对象**：`list_objects` 缺省**不返回隐藏对象**（`tools.rs:3125` ✓）⇒ 少读一根就拼出不完整的顺序，重排直接被判"顺序不完整"✗。真机验证特意在图层里留了一根隐藏笔迹 ✓，页面仍然排得动 ✓。
    * **幂等也要说**：已经在最上/最下再点 ⇒ 明说"画面不变"，不静默 ✓。
    * 验证不只看数据：探针在交叠处取像素，**红蓝互换**才算 z 序真的进了渲染（`render_region raw:true` ⇒ 逐像素数红蓝 ✓）；重排原子可用 `get_log` + `revert` 撤回来（`undo_last` 按设计只管笔迹类原子 ⇒ 拿它验结构变更是探针的前提错 ✗，不是产品的错 ✓）。
* **本轮修掉的两个真实启动 bug（都不是"缺口"，是坏在脸上的东西）** ✗→✓：
  * **打开文档后「对象」「标注」面板是空的**：两条启动路（`loadDocumentData()` 与带 token 的内联分支）都没调 `refreshObjects()` / `refreshAnnotations()` ⇒ 页面明明有 2 个对象却写着「当前图层还没有对象 ✓」（**一句假话** ✓），用户只能自己去点「刷新」。修法：两条路都补上，并按 `__appMarks` 纪律打点（`boot:before-refreshObjects` / `boot:refreshObjects-ok` / `…!err`），失败要留下可离线诊断的痕迹 ✓。
  * **面板按钮要等 WASM 内核加载完才接线**：`warmKernel()` 里 `initWasm()` 排在面板接线之前 ⇒ 内核加载失败/慢时，图层、对象、锚点这些**与服务端工具对话、跟内核无关**的面板整片是死的 ✗（表现为"按钮点了没反应"，而页面看起来一切正常）。修法：接线提到 `initWasm()` 之前 ✓。
  * 新增仓库判据 `scripts/browser-boot-panels.mjs` ✓：建一份"有对象、有标注"的一次性文档 ⇒ 打开页面**不点刷新** ⇒ 面板条数必须等于服务端条数、且不得出现"还没有对象"这类自相矛盾的提示。已做**变异测试**：把两处启动调用换成空表达式 ⇒ 判据红（面板 0 vs 服务端 2 ✓），恢复 ⇒ 绿 ✓。
* **新入口必须连带过英文面**✓（`browser-i18n.mjs` 在真浏览器里逐条抓到，不是补完就自以为干净 ✓）：本轮补的词条分三类 —— ① 卡片静态名与 tooltip（「对象上移 / 对象下移」）；② **面板空态提示**（「还没有标注 ✓」「当前图层还没有对象 ✓」「还没有存档点 ✓」…）—— 这类以前只在中文里写过 ⇒ 启动 bug 一修好、它们第一次真的显示出来，英文面立刻露红 ✗→✓；③ **动态拼句**要加模式词条：`读取失败：reference_not_found` 这种"整句里带服务端错误码"的串，字典按整句匹配不到 ⇒ 新增 `i18nText()` 的 `m19` 模式（`/^读取失败：(\S+)$/`）✓。
* 「笔刷库与预览」行里"`save_palette` / `set_brush_dynamics` 无 UI"⇒ **已过期**：两者都在 Web 调用集内（调色板保存、笔刷动态）。
* 「导入导出」行里"**无** `export_png`"⇒ **已过期**：`export_png` 已在调用集内（服务端导出选项）。
* 「画布采样」行的两条路径并存问题**未动**：吸管仍走客户端 WASM（`state.kernel.render_region_rgba`），`sample_color` / `sample_colors` / `analyze_region` 在 Web 零调用 ⇒ "读数可能漂移"的判断继续成立。
* 「作业与批处理」行**未变**：`batch` 有、`cancel_job` / `get_job` 无；本轮另发现 `commit_batch` / `try_batch` 也无入口。

**本轮之后仍然成立的 Web 缺口**（逐个 `grep` 过 `assets/viewer-app.js` + `src/viewer.rs`，命中数 0；这些是"系统已在、MCP 缺省也能调、人却够不到"的一类，按价值排序，逐条要不要补属产品决策，本文只登记）：

| 缺口 | 服务端状态 | 为什么算缺口 |
|---|---|---|
| `import_psd_layers` | core | Web 只有 `import_psd`（整包栅格化）；按图层入库（PSD 分层进画布）无入口 |
| ~~`frequency_separation`~~ | retouch（已在缺省面内） | **第三轮已补** ✓（「绘制」面板的**频率分离**卡 ⇒ 见"追加（2026-10-26 第三轮）"✓）；同一轮还修了**混合模式清单与内核漂移**那条显示缺陷 ✓ |
| `smart_*` 9 个（`smart_fill` / `smart_select` / `smart_sample` / `smart_edge_soften` / `smart_erase_shape` / `smart_blend` / `smart_glaze` / `smart_hatch_follow` / `smart_texture`） | 已在缺省面内 | 一整套"以选区/内容驱动"的能力只有 Agent 用得到 |
| stash 三件套（`list_stashes` / `apply_stash` / `discard_stash`） | 已在缺省面内 | 暂存区完全没有 UI |
| `commit_batch` / `try_batch` | 已在缺省面内 | 见"作业与批处理"行 |
| `fill_region`（羽化 + 位图纹理填充） | core | Web 只有 `fill`（纯色矩形）与 `gradient_fill`；`fill_region` 的 `feather` 与 `texture:{type:"bitmap",…}` 两条能力页面无入口（`grep fill_region` 在 `assets/viewer-app.js` 与 `src/viewer.rs` 里 **0 命中**） |
| `dodge_burn_setup`、`export_svg`、`export_timelapse` | 已在缺省面内 | 曲线/SVG/延时工程包都无入口（`export_timelapse` 的格式文档见 `docs/timelapse-package-format.md`）；**`get_histogram` 本轮已补**（诊断面板的「直方图」卡 ✓，见"本轮之后"下面的✓段落） |
| 预设库的**内容**只有中文 | — | `list_presets` 返回 `name_en`（本轮已接进面板 ✓），但 `tags` / `suitable` / `caution` 与旋钮标签**服务端只有中文** ⇒ 英文界面下这三段仍必是中文。不是 i18n 判据能修的，属 `crates/yanshi-server/src/presets.rs` 的内容缺口 |
| `delete_mask` | structure（已在缺省面内） | 页面无入口 ✓ 而且**它根本不该做"取消蒙版"那件事** ✗：它只删本体 ✓ 层上的 `mask_id` 引用还留着 ✓（见"追加（2026-10-26 第二轮）"登记 ① ✓）⇒ 真正的"取消"是 `set_property mask_id = null` ✓ 本轮已做成「取下蒙版」按钮 ✓ |
| `window.yanshiCallTool` 强制 `refresh: false` | — | 外部脚本（检查脚本 / 自动化）经它调工具后**页面不重读状态** ⇒ 验证者以为没生效。位置 `assets/viewer-app.js:2914`；下一轮要么加可选参数，要么在文档里写清"调用后需手动刷新" |

**复现第 8 条**（Web 面计数，本轮口径，只读源码、不需要服务端）：

```bash
node -e '
const fs=require("fs");
const src=["crates/yanshi-http/assets/viewer-app.js","crates/yanshi-http/src/viewer.rs"].map((f)=>fs.readFileSync(f,"utf8")).join("");
const called=new Set([...src.matchAll(/call(?:Tool|ToolChecked|ToolQuiet)\(\s*["\x27]([a-z_0-9]+)["\x27]/g)].map((m)=>m[1])
  .concat([...src.matchAll(/yanshiCallTool\(\s*["\x27]([a-z_0-9]+)["\x27]/g)].map((m)=>m[1])));
console.log(called.size);'                                   # ⇒ 93（补 `get_histogram` / `revert_group` 之前 90，再补 `reorder_objects` 之前 92）
target/release/yanshi-mcp --list-tools \
  | python3 -c 'import sys,json;print(len(json.load(sys.stdin)["tools"]))'   # ⇒ 205
```

上面那张缺口表在 `ef2494de` 上**逐条重新 `grep` 过**（每个名字在 `assets/viewer-app.js` + `src/viewer.rs` 里命中数仍为 0 ✓）；`reorder_objects` 本轮补了入口 ⇒ 命中 1 ✓，不再列入缺口。

---

## 追加（2026-10-26 第二轮）— 范围遮罩 / 渐变蒙版的 Web 入口，与五条登记

本轮口径：基线 `0bddba26`，服务端 `yanshi 0.3.0 (commit 0bddba26-dirty-…)`，`--root /tmp/yanshi-root`（**必须给 `--root`** ✗ 纯内存部署下 `create_range_mask` 写不了 blob ⇒ `precondition_failed` ✓ 实测踩过 ✓）。

### 本轮补掉的"系统已在 + MCP 缺省面内 + Web 零入口"

| 能力 | 页面入口 | 关键行为 |
|---|---|---|
| `create_range_mask`（D1-a ✓ `Profile::Structure`） | 「绘制」面板的**范围遮罩**卡：采样层 / 挂到层两个下拉（都由 `list_layers` 填 ✓）＋亮度带 / 色相带两套控件＋「烘焙并挂载」＋**「取下蒙版」**＋「该层当前蒙版」读数 | **覆盖为 0 时拒绝挂载** ✓（工具本身回 `ok:true` ✓ 挂上去就是"改了参数、画面纹丝不动"✓）；下限 > 上限就地拒绝 ⇒ 不产生废原子 ✓；覆盖数 / 占比 / 均值**全部取服务端回执** ✓ |
| `create_mask` 的 `linear_gradient` / `radial_gradient`（D1-b） | 工具栏两支新笔（线性 / 径向渐变蒙版）＋**渐变蒙版**卡（径向衰减、反选） | 拖动中只画**矢量预览**（虚线 / 内切椭圆 + 覆盖为 1 那一端的实心点 ✓）⇒ 提交的是文档坐标 ✓；**羽化对这两种 kind 无效 ⇒ 忽略并出声** ✓（服务端对非零羽化直接报错 ✓）；起收笔重合就地拒绝 ✓ |
| 挂上去的蒙版怎么断开的口径 | 同上「取下蒙版」按钮 | 见登记 ① ✓ 只有 JSON `null` 这一条路 ✓ |

Web 调用面（第 8 条复现命令的同一口径）：**93 → 94**（新增 `create_range_mask` ✓ `create_mask` 本来就有 ✓）。
判据：`scripts/browser-range-mask.mjs`（九项 ✓ 每条前提都有 `exit(3)` ✓ 变异测试见下）。
截图：`screenshots/rangemask-zh/`、`screenshots/rangemask-en/`（真浏览器 ✓ 含挂上前后同一画布的对照 ✓）。

### 登记 ① 取下蒙版：**能取，但只有 JSON `null` 这一条路**（三条都被实测过）

| 试的形状 | 结果 |
|---|---|
| `set_property {layer_id, key:"mask_id", value: null}` | **成功** ✓ 层上 `mask_id` 变回 `null` ✓ |
| `set_property {layer_id, key:"mask_id", value: ""}`（空串） | `reference_not_found` ✓ detail 是 `set_property mask_id 引用的实体  不存在或已删除` —— 中间那两个空格就是空串被插进模板的证据 ✓ |
| `update_layer {layer_id, patch:{mask_id: null}}` | 成功 ✓（同一条原子语义的另一种形状 ✓） |
| `delete_mask {mask_id}` | 回 `ok:true` ✓ **但 `list_layers[].mask_id` 仍指着那个已删的 id** ✓ 悬空引用 |

⇒ 界面因此送 `null` ✓ 且把"这一层现在挂着什么"照服务端事实显示出来 ✓（不显示的话 ✗ 取下与"本来就没有"在界面上长得一样 ✓）。
⇒ **待定位（服务端）**：`delete_mask` 删本体时应否一并清掉引用 ✗ 或在"仍被引用"时拒绝删除 ✓。渲染结果与"没挂"相同（blob 比对已证 ✓）⇒ 只有引用在说谎 ⇒ 本轮没有据此做按钮 ✓。

### 登记 ② `create_range_mask` 在本地内核（PWA）里**没有端点**

- 症状 ✓：勾掉「服务器渲染」⇒ 请求由 `api-local.js` 就地接管 ✓ 而它没有这个端点 ⇒ `api-local.js:4810` 如实抛 `endpoint_not_local` ✓（`viewer-app.js:1415` 把它降级成一次工具失败 ✓ 不再打断整页初始化 ✓）。
- 本轮修的是**归因** ✗：卡片原来一律写「服务端拒绝了这次烘焙」✓ 而**请求根本没出门** ⇒ 人会去翻服务端日志 ✗。现在分两句说 ✓ 并给出下一步（勾回开关 ✓）。
- 缺口本身（在本地实现这个端点）留给 PWA 那一轮 ✓ 不在本轮范围。
- 同一模式下 `mask_rect` 拖动还会刷一屏「错误 needs_render：内核未产出字节」✓ —— 与本轮改动无关 ✓ 未据此写判据。

### 登记 ③ 判据环境：`localStorage` 是**按源共享**的

`yanshi.serverRender` 与 `yanshi.lang` 会跨脚本留在同一个 chromium profile 里 ✓ 于是"上一个脚本改过勾选"会让下一个判据在没人预期的模式下跑 ✗（本轮实测：残留 `"0"` ⇒ 范围遮罩整批判据红 ✗ 而产品没变 ✓）。
`browser-range-mask.mjs` 的写法 ✓：先开一次同源页面擦掉这两个 key ⇒ 再进被测文档 ⇒ 并**核实**「服务器渲染」确实生效 ✓ 擦不动就是前提失败 ⇒ `exit(3)` ✓ 不拿环境毛病判产品。
建议：凡依赖渲染模式或语言的判据都照这个写法 ✓。

### 登记 ④ `browser-ui-check.mjs` 自上游 `dab8a1c5` 起恒红（**判据腐烂** ✗ 不是产品缺陷）

- 它等的是 `window.yanshiKernelReady` ✓ 而 `dab8a1c5 fix(web): store an explicit off value so the server render switch stays effective` 把缺省设成"服务器渲染" ⇒ 内核**按设置故意不加载** ✓ ⇒ 等到 `YANSHI_WAIT_MS`（缺省 120 s）超时 ✗（`scripts/browser-ui-check.mjs:340` 与 `:347`）。
- 实测对照 ✓：勾选在 ⇒ `kernelReady=undefined`、`kernelHead=0`、wasm 状态「已按设置跳过（用服务端渲染）」✓；把勾去掉 ⇒ `ready=true`、`head=4`、「已加载」✓。
- 该条**不在** `scripts/criteria-known-red.txt` ⇒ 要么改判据（自己取消勾选 ✓ 或接受服务端路径 ✓），要么登记已知红 ✓。本轮**未修**（不是 web 功能缺口 ✓ 且改它要连带决定"这条判据到底测哪条渲染路径"✓）。

### 登记 ⑤ 改动选择器漏报查看器改动（本轮已修）

- `scripts/criteria-for-changes.sh` 原来只有 `web/*|assets/*` ⇒ 那匹配的是**仓库根**目录 ✗ 而查看器的本体在 `crates/yanshi-http/assets/viewer-app.js` 与 `crates/yanshi-http/src/viewer.rs` ✓ ⇒ 改页面却报「受影响的判据：0 个 ⇒ 直接推 CI」✓（正是该脚本自己在第 1299 / 1352 轮批评的那类漏报 ✓）。
- 实测（同一份工作区改动 `--since HEAD`）✓：修之前报 **39** 条其中 `browser-*` / `tool-pwa-*` **各 0 条** ✗ 修之后报 **91** 条（`browser-*` 44 + `tool-pwa-*` 8 ⇒ 新覆盖 **52** 条 ✓）。
- 顺带确认的既有纪律 ✓：改完查看器**必须**跑 `node scripts/pwa-sync-viewer.mjs` ✗ 否则 `tool-pwa-assets.mjs` 报"两条部署分叉"✓。
- 该判据在本机仍剩一条与本轮无关的红：`web/mediums/*.wasm` 是 gitignore 的构建产物 ✓ 本机没建 ⇒ 需先跑介质构建 + `mediums-sync.mjs`（属环境前提 ✗ 不是缺陷 ✓）。

### 登记 ⑥ `web/index.html` 这面镜像**不属于功能补丁**（本轮实测后撤回）

- 上游自己已经分叉：`git log -1 -- web/index.html` = `04d6aab6`（布局卡那轮）✗ 而 `git log -1 -- crates/yanshi-http/assets/viewer-app.js` = `0ba66fd9` ⇒ 即 **HEAD 上的镜像比查看器本体旧一个提交** ✓（`0ba66fd9` 提交时没跑 `pwa-sync-viewer.mjs`）。
- 本机跑同步**能**消掉"两条部署分叉"这条红 ✓ **但**它把带机器状态的唯一戳写进了受版本控制的文件：`caches.open("yanshi-shell-0bddba26-dirty-de862dcb")` ✗（`-dirty-` 与本机 worktree 的哈希都跟着进 `web/index.html`）⇒ 一个功能补丁里出现这种字节 = 无法复核、也无法在别人机器上复现 ✗。
- CI 不会因此红：`.github/workflows/ci.yml:509-510` 在 PWA 作业里**重新生成**这面镜像 ✓ 所以"镜像落后"在上游是**被容忍的状态** ✓ 由发布/PWA 那一步收敛 ✗ 不是每轮功能改动该带的东西 ✓。
- 本轮结论：改查看器时**跑**同步、验证 `tool-pwa-assets.mjs` 的分叉条目确实转绿 ✓ 然后 `git checkout -- web/index.html` 撤回镜像 ✗ 并把这件事登记在这里 ✓。**要用户决策的**：镜像的唯一戳该不该由"内容哈希 + 提交号"构成（去掉 `-dirty-` 与本机因素）⇒ 那属于 PWA 缓存策略，不是本轮能顺手改的。

---

## 追加（2026-10-26 第三轮）— 频率分离的 Web 入口，与"界面混合模式清单只有一半"那条显示缺陷

本轮口径：基线 `0bddba26`，服务端 `--root /tmp/yanshi-root`，chromium（CDP 9490）真浏览器 + 真截图；构建 `cargo build --release -p yanshi-http -j 2`（单 crate ✓ 防 OOM ✓）。

### 本轮补掉的两件事

| 能力 | 页面入口 | 关键行为（全部以服务端回执为源 ✓） |
|---|---|---|
| `frequency_separation`（`Profile::Retouch` ✓ 缺省面内） | 「绘制」面板的**频率分离**卡：源图层下拉（由 `list_layers` 填 ✓）＋半径（1–64 ✓ 高斯 sigma）＋图层名（留空用源层名 ✓）＋「分离」按钮 | 建出 5 层（组「X · 频率分离」＋低频 / 低频绘画 / 高频（`linear_light`）/ 高频绘画）✓；回执里的 `radius` / `low_layer_id` / `high_layer_id` / 三项重建误差**原样打印** ✓ 页面不自己算 ✗。**四条拒绝路径都不静默** ✓：空层（`precondition_failed` ✓ 就地拦下、请求都不发 ✓）、半径越界（`invalid_argument` ✓ 就地拦下）、源层不在服务端清单里（刚被删 ✓ 拦下）、**服务端自检失败**（`invalid_argument` ✓ 发得出 ✗ 它拒绝时不建组 ⇒ 不留半套层 ✓）；外加**本地内核没有这个端点**单独一句 ✓（见登记 ⑨） |
| 图层混合模式的**显示清单** | 图层面板每行的混合模式下拉 | 原来**只有 7 个选项** ✗ 而内核有 **13 个** ⇒ 用 `set_layer_blend` 成功写下 `soft_light` 之后**下拉倒回「正常」** ✓ 人以为没生效 ✗（详见登记 ⑦）。本轮改成与内核同源的一张表 ✓ 并且**当前值不在清单里也照原名显示** ✓（内核将来加模式 ✗ 界面不许倒退） |

Web 调用面（第 8 条复现命令的同一口径）：**94 → 95**（新增 `frequency_separation` ✓ 实测该命令输出 95 ✓）。
判据：`scripts/browser-frequency-separation.mjs`（八项 ✓）、`scripts/browser-blend-modes.mjs`（六项 ✓）；两条的前提失败都走 `exit(3)` ✓ 用法错误 `exit(2)` ✓；调用口径与同级脚本一致 ⇒ `node scripts/<name>.mjs <viewer-url> [cdp-port]`。
变异测试 ✓：频率分离注入 4 处产品改动 ⇒ 恰好 5 条预期红（多一条是"拒绝时不许留层"被连带击穿 ✓ 已核对是同一处产品）；混合模式回退到 7 项清单 ⇒ 恰好 5 条预期红 ✗ 其中英文残留那条**正确地保持绿**（回退清单不含新英文串 ✓ 说明它不是被顺带红出来的）。两条测完都已恢复全绿。
截图：`screenshots/freqsep-zh/`、`screenshots/freqsep-en/`（各 14 张 ✓ 含分离前后同一文档的图层列表对照、四条拒绝路径、内核模式那一句）✓ `screenshots/blend-before/`、`screenshots/blend-after/`（同一层同一模式的对照 ✓ 修前那张下拉是「正常」）。

### 登记 ⑦ 界面那份混合模式清单与内核**漂移**（本轮修 ✓ 但它暴露的是结构性问题）

- 权威清单：`list_engine_capabilities.blend_modes` ⇐ `crates/yanshi-render/src/blend.rs:72-86` 的 `BlendMode::NAMES` ⇒ **13** 个（`normal, multiply, screen, overlay, darken, lighten, add, linear_dodge, subtract, color_dodge, difference, linear_light, soft_light`）。
- 本轮实测到的缺陷面 ✓：`set_layer_blend` **写成功**（`ok:true` ✓ 服务端 `list_layers` 也存着新值 ✓）⇒ **只有显示在说谎** ✗。误报的 6 种是 `subtract` / `color_dodge` / `difference` / `linear_light` / `soft_light` / `linear_dodge`。
- **最刺眼的一例**：`linear_light` 正是**频率分离建出来的高频层**所用的模式 ⇒ 人用同一面板做频率分离 ✗ 看到的却是「正常」✓（本轮截图里修前就是这幅样子）。
- 两条写入语义不同 ✓ 值得记下来：工具层 `BlendMode::parse` **严格**（未知值 ⇒ `invalid_argument` ✓）而渲染层 `from_name` **宽松**（未知值 ⇒ `Normal` ✓）⇒ 所以"界面显示成 normal"这类降级**只可能来自读到一份外部/旧文档** ✗ 不可能由 HTTP 写出来 ✓（三条写路径 `create_layer` / `update_layer` / `set_property` 都校验 ✓）。这也是 `browser-blend-modes.mjs` 自己承认测不到该回退分支的原因 ✓。
- 顺带确认 ✓：`linear_dodge` 是 `add` 的**别名** ✗ 但**原样存**（不归一）⇒ 界面把它单列一项 ✓ 标签写明是别名 ✓ 不冒充第 14 种模式。

### 登记 ⑧ `revert_group` **撤销不了**频率分离建出来的层

- 实测：对刚建好的分离组调 `revert_group` ⇒ `reverted_count: 0` ✓ 五个层一个没掉 ✗。
- 原因口径：那五个层不是"原子组"（`frequency_separation` 自己逐条建 ✗ 没有 `group` 语义把它们的原子收在一起 ✓）。
- 本轮因此**没有**给"一键撤销分离"做按钮 ✗（做了就是假承诺 ✓）；页面文案只说"服务端拒绝时不建组 ⇒ 不留半套层"✓。要真正可撤销需要服务端把这次工具调用整体收成一个可回退批次 ⇒ 属工具层改动 ✗ 不在本轮。

### 登记 ⑨ `frequency_separation` 在本地内核（PWA）里**没有端点**

勾掉「服务器渲染」⇒ `api-local.js` 就地接管并如实抛 `endpoint_not_local` ⇒ 页面单独一句「本地内核没有这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试」✓。与登记 ② 同一族（`create_range_mask` 也缺）✓ 缺口本身留给 PWA 那一轮 ✓。

### 登记 ⑩ 分离后的合成结果**不逐像素等于**原图

实测（平色 `#3f7fbf` ✓ 半径 8 ✓）：工具自检回执 `reconstruction_mean_error 0.0092` / `max 0.0157` / `bright max 0.0039` ✓ 而它**在限内**（工具的限是均值 0.01176 / 最大 0.07843 ✓）。另一次对照里页面合成与原图有 **124 个通道差**、最大 **10/255** ⇒ 与工具的自检限不矛盾 ✗ 但**本轮没有据此写"必须逐像素相同"的判据** ✓（那要先把两处的容差对齐 ✗ 属服务端口径）。

### 登记 ⑪ 判据环境（延伸登记 ③）：**改过 `localStorage` 的判据必须自己还原**

- 实测代价 ✓：`browser-render-switch.mjs` 收尾时把偏好停在 `"0"`（客户端渲染）✗ 且从不还原 ⇒ 下一个共用同一 profile 的脚本 `browser-canvas-handfeel.mjs` 报 `endpoint_not_local: /api/tools/list_effects` ＋「新建之后没有切到新图层」✗ **一条产品没变的判据假红** ✓。
- 本轮修法 ✓：两条退出路径（红 / 绿）都先 `setPref(null)` 回到缺省 ⇒ 复现链已证：改前 handfeel 红 ✓ render-switch 加还原后 rc=0 ✓ profile 里 `sr:null` ✓ handfeel rc=0（`consoleErrors: 0`）。
- 与登记 ③ 的分工 ✓：③说"每条判据进来先擦"✗ 本条说"**出去也要擦**"✗ 两头都做到 ✗ profile 才是可预期的。

### 登记 ⑫ 判据环境：泄漏的 CDP **page target** 会让判据假红

`browser-frequency-separation.mjs` 第 ⑧ 项一度报 `page:""`（点「分离」什么都没发生）✗ 而产品未变 ✓。排查：共用 chromium 上积了 **18 个**没关的 page target（`http://127.0.0.1:9490/json/list` 数出 21 个目标）⇒ 逐个 `json/close/{id}` 关掉（21 → 3）后同一条判据 rc=0 ✓。
本轮动作 ✓：在该项的失败详情里带上 `jsErrorsSoFar`（头两条页面错误）⇒ 下次一眼能分清"产品坏了"还是"浏览器被用坏了" ✗ 不再靠人猜。
建议：凡自己 `Target.createTarget` 的判据 ⇒ `Target.closeTarget` + `ws.close()` + 显式 `process.exit` 三件都要有 ✓。

### 登记 ⑬ 本轮自己触发的那条分叉红（照登记 ⑥ 的流程走完）

改 `assets/viewer-app.js` 后 `tool-pwa-assets.mjs` 报「防分叉：viewer-app.js 与单一源不同（除戳外）」✗ —— **这条红是真的** ✓（我改了单一源 ✗ 没同步镜像）。跑 `node scripts/pwa-sync-viewer.mjs` ⇒ 该条目转绿 ✓ 然后按登记 ⑥ 撤回带机器戳的 `web/index.html`（`git checkout -- web/index.html` ✓ `git status` 已干净 ✓）。
该判据本机仍剩的那条红与本轮无关：`assets/mediums/*.wasm` 在本机**一个都不存在**（`ls` 实证）⇒ 介质产物缺失属环境前提 ✗ 与登记 ⑤ 末尾那条同因 ✓ 由 CI 收敛。

### 本轮两处**只有看真截图才发现**的界面问题

- 「源图层」这个标签在卡片里**折成两行**（中间断词）⇒ `viewer.rs:637` 加 `white-space:nowrap` ✓。
- 英文界面下图层名输入框的 placeholder 被 170px 宽的框**裁掉尾字** ⇒ 输入框定宽 170px ＋ 把译文从 "Defaults to the source layer name" 收成 "Layer name by default" ✓（`I18N_EN_ATTR` 里留了原因注释 ✓）。
两条都不是任何判据能报的 ✗ 记在这里是为了留住"截图检查"这件事的成本与收益 ✓。

---

## 需要用户决策

以下不是实现者能单方面决定的（措辞取自仓库自己的待办记录，不是本文作者的取舍）：

1. **MCP 默认只暴露 HTTP 面的一半（69/137）是否加宽。** `docs/design/implementation-notes.md:28142-28148` 把它列为 `待用户决策（我不单方面改 ✓）` 的第一条：`· **MCP 默认只暴露 68/136** ✗ ⇒ 行为变更 ✓`（该行写于 `sample_color` 之前，现为 69/137）。改默认 profile 集是行为变更。
2. **`compare_with_reference` 挪能力，还是新增只读工具。** 同上一节第二条待决策；理由见 `scripts/criteria-known-red.txt`。
3. **笔刷预览的"来源信号"要不要产品提供。** 已知红 `browser-brush-preview-local.mjs` 的条目要求产品给预览加来源信号，**或**该判据改测别的真实信号。本次实测表明该条目的理由已过时（见"本次未测"），所以这条决策的**前提本身**也需要重新确认。
4. **6 条 `semantic` 工具是否实现、以及 provider 契约。** `docs/semantic-tools.md` 明确把模型、端点协议、提示词成形、成本控制、缓存都留给"实现它的那一轮 + owner 输入"。
5. **介质/厚涂（impasto）0.09 个百分点残差是否接受。** 已知红 `tool-impasto-plateau.mjs` 的理由末尾就是"待产品决策"。
6. **`docs/tools.md:24` 的 125 与 `docs/guide.md:7` 的 137 哪个是权威口径。** 两者都自称"全部已实现组"，但 profile 集不同（6 vs 8）；修哪一边取决于"已实现组"是否包含 `changeset` / `conflict`。
7. **参考图偏好是"工作区级"还是"文档级"。** 实测：`set_reference` 写进 `<工作区根>/preferences.json` ⇒ 给文档 A 设的位置/图，切到新建文档 B 仍在（`get_preferences` 的 `where` 字段自证）。作为"临摹底图"跨文档常驻可能是有意的 ✗，但**位置是某个文档的像素矩形** ⇒ 换文档后那块矩形落在别处；页面能做的只有"照实显示 + 提供清除"（本轮已做 ✓）。要不要改成按文档存（`reference.<doc>.position`）属服务端数据模型决策。
8. **参考图原图要不要以可显示的形式留一份。** 存储是内容寻址的**裸 RGBA**（`POST /api/blob` 先把 PNG 解码成 RGBA ✓），`GET /api/blob/{hash}` 的 MIME 嗅探对裸像素只给 `application/octet-stream` ✓ ⇒ `<img>` 永远解不开 ✗。本轮的对策是"浏览器自己缓存上传时的 dataURL"（超过 1.5MB 或配额不足就没辙 ✓，信息条会说明 ✓）。彻底解法要么在服务端留原图/记录宽高（`reference.width/height` ⇒ 前端可用 `ImageData` 重建成 PNG ✓），要么产品接受"换浏览器需重传"。
9. **`web/index.html` 的缓存唯一戳该不该去掉本机状态。** 见「登记 ⑥」：`pwa-sync-viewer.mjs` 把 `0bddba26-dirty-de862dcb`（含 `-dirty-` 与本机 worktree 哈希）写进受版本控制的镜像 ⇒ 同一份改动在别人机器上生成不同字节。改成"提交号 + 内容哈希"是 PWA 缓存失效策略的决策 ✗ 不属于一轮功能补丁顺手改的范围 ✓。
10. **界面的混合模式清单该由谁供。** 本轮的修法是**把 13 个模式与中文名内置成一张与 `BlendMode::NAMES` 同源的表**（`viewer-app.js` 的 `BLEND_MODE_LABELS` ✓）——它当场止住了"6 种模式显示成「正常」"✓ 但**下一次内核再加模式仍会漂移** ✗（判据 `browser-blend-modes.mjs` 会把这种漂移报红 ✓ 也就是"靠判据追"）。彻底解法是页面下拉直接取 `list_engine_capabilities.blend_modes` ＋**服务端供中文名（含英文）** ⇒ 那是 HTTP 返回结构与 i18n 数据模型的改动（模式名与标签同源在一处 ✓ 就不会再有第三份）✗ 属服务端契约决策 ✗ 不由本轮顺手改。

---

## 本次未测

* **`browser-brush-preview-local.mjs` 的真实结论**：本次用 chromium（`Chromium 153.0.8010.52`）连上运行中的服务端实跑了它，得到 `{"options":200,"rows":199,"local":0,"dataUrls":199}`、退出码 1。**但红的原因不是产品缺接口**：同一页面用 CDP 读出 `img.getAttribute("src") = /brush-previews/classic-brush.myb.png`、`img.src = http://127.0.0.1:15900/brush-previews/classic-brush.myb.png`，而判据用 `.src`（绝对 URL）去 `startsWith("/brush-previews/")`（相对前缀）⇒ 恒不成立。产品侧计数 `brushPreviewsFromFiles = 199`，即 **199 行全部走入库预览**。所以：已知红条目写的理由（"测的是不存在的接口 / viewer.rs 零命中"）已与判据正文（`scripts/browser-brush-preview-local.mjs:60-70` 注释说已改测 `/brush-previews/`）矛盾。**要判定"产品是否缺该接口"，需要先修判据的属性/属性前缀比较**；本文不替产品下结论。
* **`tool-paint-memory.mjs` 的成本比**：本次按 `node scripts/tool-paint-memory.mjs http://127.0.0.1:15900 12` 复跑，判据只记录到 5 笔、打印"**笔数 < 6 ⇒ 成本比不判**"、退出码 0，**没有量到**已知红里那个 `1.66×`。故本文引用该数字时标注为"已知红文件记录"，不是本次实测；要复现需先查明该判据为何只数到 5 笔。
* **`tool-archive-bloat.mjs` / `tool-impasto-plateau.mjs` / `kernel-brush-parity.mjs` / `browser-ui-check.mjs`**：本次未逐条运行，理由/数字均逐字引自 `scripts/criteria-known-red.txt`，未独立复测。
* **`tool-surface-coverage.mjs` 的当前红灯**：本次实跑退出码 1，报 2 处不一致——`crates/yanshi-server/src/tools.rs:33` 与 `:358` 仍写「核心层 … 68 个」，实测 core = 69（`:4` 与 `:943` 已改成 69）。该脚本**不在** `criteria-known-red.txt` 的 8 条名单里，而 `scripts/run-criteria.sh` 的枚举是 `scripts/tool-*.mjs` ⇒ 按现有接线它会让 CI 变红。要判定"CI 当前是否红"，需要在 CI 配置里确认分片与超时后实跑一次，本文未做。
* **介质的两条实现是否逐像素一致**：浏览器 WASM 路径与服务端 `medium_stroke` 未做逐像素比对；需要写一条类似 `kernel-brush-parity.mjs` 的判据。
* **`docs/design/tool-examples.md` 的完整差异**：本次只量到"文档 47 个 `##` 小节 vs 运行目录里 109 个带 `example` 的工具"，判据 `tool-examples-doc-check.mjs` 退出码 1（已知红）。具体缺哪些小节需运行生成器后逐条 diff，本文未做。

---

## 复现命令

```bash
# 0) 构建
cargo build --workspace

# 1) 起服务端（全 profile），并用权威清单取 HTTP 面
target/debug/yanshi-serve --bind 127.0.0.1:15900 --root /tmp/align-root --doc boot \
  --width 512 --height 512 --assets-dir assets --profile all &
curl -s http://127.0.0.1:15900/api/tools \
  | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len(d["tools"]), d["profiles"])'
# ⇒ 137 ['core','history','changeset','retouch','conflict','annotation','collab','structure']

# 2) MCP 默认与逐 profile
target/debug/yanshi-mcp --list-tools \
  | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["tools"]))'          # ⇒ 69
for p in core history changeset retouch conflict annotation collab structure semantic all; do
  n=$(target/debug/yanshi-mcp --list-tools --profiles "$p" \
      | python3 -c 'import sys,json; print(len(json.load(sys.stdin)["tools"]))')
  echo "$p = $n"
done
# 6 组（tool_inventory 的口径）⇒ 125；8 组（服务端默认）⇒ 137

# 3) 三面对账判据（它同时核对 guide*.md 的数字与 tools.rs 的注释）
node scripts/tool-surface-coverage.mjs http://127.0.0.1:15900

# 4) Web 面：工具条条目数
sed -n '6816,6843p' crates/yanshi-http/src/viewer.rs | grep -cE '\{ (tool|id):'   # ⇒ 22

# 5) Web 面：查看器真正调到的工具名（直接调用 + 助手转交）
grep -oE 'callTool(Checked)?\(\s*["'"'"'][a-z_]+["'"'"']' crates/yanshi-http/src/viewer.rs \
  | sed -E 's/.*["'"'"']([a-z_]+)["'"'"']/\1/' | sort -u
grep -oE '(convert_to_shape|convert_to_path)' crates/yanshi-http/src/viewer.rs | sort -u
# 再加上动态分支里出现的 10 个（viewer.rs:3396/3404/6002）：
#   update_adjustment update_filter add_adjustment add_filter
#   clone_stamp heal_stamp smudge liquify_push liquify_twirl liquify_pinch

# 6) 已知红与文档陈旧
cat scripts/criteria-known-red.txt
node scripts/tool-examples-doc-check.mjs http://127.0.0.1:15900   # ⇒ 退出码 1（文档过期）
node scripts/tool-sample-color.mjs http://127.0.0.1:15900         # ⇒ 退出码 0（本次绿）
node scripts/tool-reference-delta-e.mjs http://127.0.0.1:15900    # ⇒ 退出码 1（能力挂错位置）

# 7) 笔刷本地预览的实测（需 chromium）
chromium --headless=new --no-sandbox --disable-gpu --remote-debugging-port=9490 \
  --user-data-dir=/tmp/align-chrome about:blank &
tok=$(curl -s -X POST http://127.0.0.1:15900/api/documents -H 'content-type: application/json' \
  -d '{"doc_id":"boot","width":512,"height":512}' | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')
CDP_PORT=9490 node scripts/browser-brush-preview-local.mjs \
  "http://127.0.0.1:15900/?doc=boot&token=$tok"

# 8) 第三轮的两条新判据（需 chromium 在 9490 ＋ 服务端带 --root）
#    注意调用口径：这两条取 argv ✗ 不读 CDP_PORT 环境变量 ✓（与同级 browser-* 一致）
node scripts/browser-frequency-separation.mjs "http://127.0.0.1:15900/?doc=boot&token=$tok" 9490
node scripts/browser-blend-modes.mjs "http://127.0.0.1:15900/?doc=boot&token=$tok" 9490
# 改完查看器必须同步镜像 ✗ 否则 tool-pwa-assets 报分叉（见登记 ⑬ / 登记 ⑥）
node scripts/tool-pwa-assets.mjs                       # 本机仍红一条：assets/mediums/*.wasm 未构建
node scripts/pwa-sync-viewer.mjs && git checkout -- web/index.html
```
