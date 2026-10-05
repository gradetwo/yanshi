# 三面对齐表：系统实现 / Web 查看器 / MCP

本文回答一个问题：**同一份能力，系统实现了没有、人在 Web 查看器里够不够得到、Agent 通过 MCP 看不看得见。**
三面各自"实际暴露了什么"都来自**运行命令或读源码**，不来自记忆；每个数字都写了出处。

* **日期**：2026-10-05（Asia/Shanghai）。
* **被测版本**：`main` @ `f5835cb`，工作树干净。
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

| 面 | 数量 | 口径 | 证据 |
|---|---|---|---|
| 系统实现（`ToolRegistry` / HTTP `/api/tools`） | HTTP 默认 **137**；注册表/文档口径 **125**（core **69**） | ① HTTP 口径 = 服务端默认启用的 8 个 profile 并集的工具数；② 注册表口径 = `docs/tools.md` 写死的"全部已实现组"数，它用的 profile 集只有 6 个（**不含 changeset / conflict**） | ① `curl -s http://127.0.0.1:15900/api/tools` ⇒ `profiles` 恰为 `core,history,changeset,retouch,conflict,annotation,collab,structure`（8 个）、`tools` 长度 137；默认集见 `crates/yanshi-http/src/server.rs:97-105`，profile 全集见 `crates/yanshi-server/src/tools.rs:63-72`；② `docs/tools.md:24`「**69 core tools** … **125 tools in total**」＋`crates/yanshi-server/tests/tool_inventory.rs:12-19`（6 组）；实测 `--profiles core,history,retouch,annotation,collab,structure` ⇒ **125** |
| Web 查看器 | 工具条 **22** 个按钮；真正调到的服务端工具 **78** 个 | ① 工具条 = `TOOL_DEFS` 数组条目数（20 个 `{tool:…}` + 2 个 `{id:…}`；`pan` / `eyedropper` / `move_layer` 是纯客户端伪工具）；② 调用面 = 查看器 JS 调用的**去重** `/api/tools` 工具名（直接 `callTool` / `callToolChecked` + 助手 `convertCheckedObjects` 中转的 `convert_to_shape` / `convert_to_path`，以及动态分支 `viewer.rs:3396/3404/6002` 的 10 个） | ① `crates/yanshi-http/src/viewer.rs:6816-6843`，按钮由 `renderToolStrip()`（`:6844`）填充 `#tools`（`:505`）；② 提取命令见文末，其中 **34** 个在 core、**44** 个不在 core |
| MCP | 默认 **69**；全开 **137** | ① 默认 = 不带 `--profile` 启动（`profiles = [Profile::Core]`）；② 全开 = `--profiles all`（= `Profile::IMPLEMENTED`，8 组，**不含 semantic**） | ① `crates/yanshi-mcp/src/lib.rs:86`；实测 `target/debug/yanshi-mcp --list-tools` ⇒ 69；② `crates/yanshi-server/src/tools.rs:100-109`；实测 `--profiles all` ⇒ 137 |

**由表 1 直接推出的两个覆盖率**（来自 `scripts/tool-surface-coverage.mjs` 的打印）：
`MCP 默认 69 / HTTP 137 = 50.4%`；`MCP 默认 ∩ Web 调用 = 34 / 78 = 43.6%`。

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
| 参考图（`set_reference` / `clear_reference`） | 已在（core） | **无**：查看器只用 `get_preferences`@viewer.rs:7344 读出 `reference.*` 并叠一层半透明图（viewer.rs:7336-7372），没有设置/移除入口 | 默认有（core：`set_reference` / `clear_reference` / `get_preferences`） | 系统+MCP | 人只能用 MCP 设参考图，页面只能显示。下一步：加"上传参考图"按钮，或产品决定参考图只由 Agent 设置 |
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

## 需要用户决策

以下不是实现者能单方面决定的（措辞取自仓库自己的待办记录，不是本文作者的取舍）：

1. **MCP 默认只暴露 HTTP 面的一半（69/137）是否加宽。** `docs/design/implementation-notes.md:28142-28148` 把它列为 `待用户决策（我不单方面改 ✓）` 的第一条：`· **MCP 默认只暴露 68/136** ✗ ⇒ 行为变更 ✓`（该行写于 `sample_color` 之前，现为 69/137）。改默认 profile 集是行为变更。
2. **`compare_with_reference` 挪能力，还是新增只读工具。** 同上一节第二条待决策；理由见 `scripts/criteria-known-red.txt`。
3. **笔刷预览的"来源信号"要不要产品提供。** 已知红 `browser-brush-preview-local.mjs` 的条目要求产品给预览加来源信号，**或**该判据改测别的真实信号。本次实测表明该条目的理由已过时（见"本次未测"），所以这条决策的**前提本身**也需要重新确认。
4. **6 条 `semantic` 工具是否实现、以及 provider 契约。** `docs/semantic-tools.md` 明确把模型、端点协议、提示词成形、成本控制、缓存都留给"实现它的那一轮 + owner 输入"。
5. **介质/厚涂（impasto）0.09 个百分点残差是否接受。** 已知红 `tool-impasto-plateau.mjs` 的理由末尾就是"待产品决策"。
6. **`docs/tools.md:24` 的 125 与 `docs/guide.md:7` 的 137 哪个是权威口径。** 两者都自称"全部已实现组"，但 profile 集不同（6 vs 8）；修哪一边取决于"已实现组"是否包含 `changeset` / `conflict`。

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
```
