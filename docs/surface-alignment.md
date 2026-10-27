# 三面对齐表：系统实现 / Web 查看器 / MCP

本文回答一个问题：**同一份能力，系统实现了没有、人在 Web 查看器里够不够得到、Agent 通过 MCP 看不看得见。**
三面各自"实际暴露了什么"都来自**运行命令或读源码**，不来自记忆；每个数字都写了出处。

* **日期**：2026-10-05（Asia/Shanghai）；**2026-10-26 追加**一节「追加（2026-10-26）— Web 面计数与逐行更正」，那一节的数字**重测于 `ef2494de` + 本地未提交改动**（`2a86cf7d` 那一轮的读数已被替换），其余各节仍是 10-05 的读数。**同日第二轮**再追加一节「追加（2026-10-26 第二轮）— 范围遮罩 / 渐变蒙版的 Web 入口，与五条登记」✓ 口径是 `0bddba26` + 本地未提交改动 ✓。**同日第三轮**再追加「追加（2026-10-26 第三轮）— 频率分离的 Web 入口…」✓。**同日第四轮**再追加「追加（2026-10-26 第四轮）— 区域填充的 Web 入口、参考图叠加的降级，以及"判据在 CI 里根本没测对对象"」✓ 口径是 `ab0decd0` + 本地未提交改动 ✓（登记 ⑭–⑲ 是本轮的**接线自查**：四条新判据在 CI 的参数形状下原本**恒红/恒 3** ✗ 与本机手工跑的结果相反 ✓；⑰ 的"单次读前提"赛跑**在另外三条里同样存在**（已统一改成有界等待 ✓）；⑲ 是一条**假绿**——"扫中文残留"的断言原本**没扫到任何 `<option>`** 就报了 0 ✓）。**次日第五轮**再追加「追加（2026-10-27 第五轮）— 减淡加深的 Web 入口、柔光那一支的**方向**缺陷，与"复用回执少一格"」✓ 口径是 **`29d94086`**（上游已收走第三轮那两条 ✓）+ 本地未提交改动 ✓（登记 ⑳–㉖：⑳ 是**同名不同内容**的判据副本（上游那份才是超集 ✓ 逐行 diff 过）；㉑ 是 `soft_light` 亮支写错 ⇒ **「减淡」把画面压黑**、而**中性/幂等型测试对方向天然瞎** ✓ 外加一条"对照值抄自实现"的假测试；㉒ 是本引擎的中性字节 **188 不是 128**；㉓ 是曲线臂**静默 `ok:true`**；㉔ ★★ 是**复用回执漏 `mode`** ⇒ 界面念 `method undefined` ✓ 而"页面 == 回执"这种比对**两边一起缺**所以看不见 ✗；㉕ 是本地内核第三条缺端点的工具；㉖ 是**判据把 tmpfs 写满**之后 shell 表现得像"工具坏了" ✓）。**次日第六轮**再追加「追加（2026-10-27 第六轮）— 锁定层那种"按了没反应"、层行 tooltip 的中文残留、以及 curves 那条 ⚠ **被上游推翻**」✓ 口径是 **`e0fd1052`**（上游 9 条新提交里 `302d414d` 把"纯调整层不播种"这件事改了 ⇒ 第五轮 ㉓ 的前提**不再成立** ✓）+ 本地未提交改动 ✓（登记 ㉘–：㉘ 是**服务端一直拒得住而 Web 只剩"按了没反应"**——失败只写进默认收起的 `#log` ✓ 外加客户端模式那笔**幻影墨**；㉙ 是**锁在另一头上** ⇒ 预检的缓存不知道 ⇒ `permission_denied` 后必须重取状态；㉚ 是**属性里的中文**（`title`/`aria-label`）不在 `data-i18n` 节点里 ⇒ 两条 i18n 判据全绿而屏幕上有中文；㉛ 是 ⑲ 的同族复发——那条"扫中文残留"的断言**扫到 0 个点也算通过** ✓ 现在必须自证覆盖面；㉜ ★ 是**断言翻向**的那一次：测试自己红出来 ⇒ 按第五轮写在注释里的约定同时翻断言与撤 `effective` ✓ 并用**两处变异**（只改字段 / 只改渲染）证明字段与画面**两处都被问** ✓）；**同日第七轮**再追加一节「追加（2026-10-27 第七轮）— 效果卡那句「已应用」有几分真」，那一节的数字**重测于 `1231047b` + 本地未提交改动**（㉝ 是**26 个调整在缺省参数下分成三堆：17 个像素空操作 + 7 个默认就动 + 2 个连 `{}` 都建不出来**（`color_lut`／`perspective`）⇒ 卡片那句结论改成由服务端 `render_region` 的 `blob_hash` **比出来** ✓ 参数来自 `list_effects` 回读 ✓ 读数来自 `sample_colors` ✓ 而这三个数是文档核对点数时**改过来的**（初稿写 19 个空操作 ⇒ 名单只有 17 项 ⇒ 重跑取证才分清"建成空操作"与"根本建不成"）✓）；㉞ 是**仪器自己的第二个谎**——均匀网格五点全落在白底上 ⇒ "读数没变"是取样器选错 ✓ 现在外框取自 `list_objects` 的 `bbox` 并集 ✓ 变异一条就红 6 条；㉟ 是**责任归属**——本地那一臂的失败被说成「服务端拒绝（错误码 unknown）」✓ 而码早就在响应的 `error` 格子里 ✓ 这一族**只有 2 条断言在问**（变异 C 恰好红 ⑩b+⑩c）；㊱ 是**刻意不修**的那条：`api-local.js` 写 `adjustment_name` 而服务端认 `adjustment_type` ⇒ 数据形状分叉 ✓ 本机没有介质 wasm ⇒ 无法证明修好 ⇒ 登记为「需要用户决策」第 16 条；㊲ 是**JS 写的 `textContent`**（「更新这个」）又是 i18n 漏网 ✓ 与第六轮 ㉚ 同族；㊳ ★★ 是**本轮唯一只有看真截图才会去查**的一条：那句「指纹没变 ⇒ 空操作」的常设说明在三种状态里有两种都在说假话（「画面确实动了」与「未写入」底下都挂着它）✓ 而我上一刻刚决定「只降亮度、不改一个字」⇒ **那个决定被截图推翻** ✓ 现在缺省 `hidden` ✓ 由服务端那两个哈希决定挂不挂（**可见性与结论同源** ✓ 新增 ①′③′⑥′⑦′ 四条 + 变异 D 恰好红 3 条而 ③′ 照绿）✓ 同一次看图还抓到一次**未定位的偶发** wasm `wasmkernel_set_viewport` memory access out of bounds ✓ 两条对照探针都 0 错 ⇒ 只登记不谎称修好 ✓；㊴ ★★ 是**我把"复跑全绿"抄成了读数**——`ui-text-density` 其实是红的（本轮自己加进界面的那句说明撞了"≥24 汉字且含 `；`/`（`"那条规矩）⇒ 按判据 prescribed 的修法把"为什么"搬进 `title=` ✓ 屏幕只留 17 个汉字的短句 ✓ 并因此踩了第六轮那条规则：**属性走另一本字典**（塞进 `I18N_EN_TEXT` 会被 `ui-i18n-coverage` 当场抓住 ✓ 正确位置是 `I18N_EN_ATTR`）✓ 新增第 37 条 ③″（问"搬家不是删掉"）✓ 变异 E 恰好红 1 条而 ③′ 照绿 ✓；㊵ 是**仪器侧**的一条：`browser-effect-card-truth` 从绿退化成稳定 exit 3 ✓ 真因**不在产品**——CDP 的 profile 目录被删掉而浏览器还在跑 ✓ 外加 `/tmp` 是 tmpfs + `usrquota`（我这个 uid 的额度被打满时 `df` 的 `Avail` 还显示 1.5 G ✓ 骗人）⇒ SW 的 Cache Storage 写不下去 ⇒ **第一个 `Page.navigate` 永不 commit** ⇒ 所有 `browser-*` 一起表现得像"CDP 没起来"✓ 同一额度还把截图 `04` 顶成 `resource_exhausted`「并发连接数已达上限」（其实是 blob 写不进 ✗ 不是并发缺陷 ✓ 修它要动 `server.rs` 的错误映射 ⇒ 进 backlog）✓ 修法随包给（`logs/start-cdp.sh`：profile 落磁盘 ✓ 每次先确认目录存在 ✓ 只留一个标签）✓ 而「SW 缓存写失败不该让导航挂住」这一条**只是指向、尚未在产品侧证明** ⇒ 记第八轮候选 ✓；㊶ 是**第二次看真截图**又抓到的两件事（同一张卡上回执那句说了责任 ✓ 底下 `#effectsList` 那一格只念裸码「读取失败：endpoint_not_local」✗ ⇒ 第三格走 ㉟ 已建好的那一对助手 ✓ 新增 ⑩d 与变异 F 恰好红 1 条；外加相邻两行逐字重复同一句结论 ⇒ 文案缩成「为什么？见鼠标提示」✓ 历史卡那一格**同族但本轮不修**因为没有任何屏幕证据问它）；㊷ 是**收尾那趟回归自己抓出的老漏翻**（英文模式下历史卡空态「（没有匹配的原子）」是运行时 `textContent` ⇒ 静态那条判据按口径**永远看不见** ✓ `browser-i18n` 要走到那个状态才问得到 ⇒ **不是本轮引入的退化 ✓ 是新走到的状态把老洞照出来**）＋量了规模：运行时中文整句 67 处、无词条 22 种 ⇒ **这 22 不能当缺陷数**（要先逐条问落点）⇒ 第八轮那条"枚举运行时文案 + 带理由的豁免名单 + 自查覆盖面"的静态判据；㊸ 是**名单里一条陈旧的已知红**（`browser-reference-overlay` 的"夹具用占位哈希"理由早被 `render_region` 产出真 blob 那版替掉 ✓ 我按 CI 形状跑三次一致 exit 0 ⇒ 移出名单 ✓ 而**陈旧的豁免会藏住将来的真红** ✓ 顺带记我自己的教训：第一次那个 exit 3 是**我没传 `doc`/`token`** ⇒ 差一点抄成"它还在红" ✓）。
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
| ~~`dodge_burn_setup`~~、`export_svg`、`export_timelapse` | 已在缺省面内 | SVG/延时工程包都无入口（`export_timelapse` 的格式文档见 `docs/timelapse-package-format.md`）；**`get_histogram` 本轮已补**（诊断面板的「直方图」卡 ✓，见"本轮之后"下面的✓段落）；**`dodge_burn_setup` 第五轮已补**（「绘制」面板的**减淡加深**卡 ✓ 见「追加（2026-10-27 第五轮）」✓ 本地内核仍缺端点 ✗ 登记 ㉕） |
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

## 追加（2026-10-26 第四轮）— 区域填充的 Web 入口、参考图叠加的降级，以及"判据在 CI 里根本没测对对象"

本轮口径：基线 `ab0decd0`（本轮 `git pull` 后的最新上游 ✓）；服务端 `--root /tmp/yanshi-root`；chromium（CDP 9490）真浏览器 + 真截图；构建 `cargo build --release -p yanshi-http -j 2`（单 crate ✓ 防 OOM ✓）。

### 本轮补掉的"系统已在 + MCP 缺省面内 + Web 零入口"

* **`fill_region` 的 Web 入口** ✓（`viewer.rs` 新增 `#cardFillRegion` ✓ 在「绘制」tab）：图层下拉（取 `list_layers` ✓ 锁定/隐藏层都标出来）、形状 `rect/ellipse/polygon/path` ＋「用选区」、`x/y/宽/高`、颜色、不透明、羽化、「填什么」（纯色 / 位图纹理 / 取画面这块）。落的是**一个形状对象**（不是烘进位图 ✓）⇒ 之后还能选中它改羽化与不透明。
* **回执与页面同源** ✓：`opacity` · 形状 · 脏区 · 脏块数都念服务端给的数；纹理那条念「源块平铺 N 倍（= 目标面积 ÷ 源块面积）· 取纹理图层 …」（`tools.rs:4939` 的 `tiles` 就是面积比 ✓ 不是"块数"✓）。

### 本轮修掉的产品缺陷（每条都有实测，不是读代码猜）

1. **羽化随"这次渲染多大一块"而变** ✗→✓（`render.rs`）：`feather_coverage` 把自己那块缓冲的边界当形状外补零 ⇒ 同一个像素在整幅渲染下 α=255、在 1 像素高的区域里读到 α=194。改成取「scope ∩ 画布」⇒ 网格看到形状的真实范围。判据 `browser-fill-region.mjs` ⑥ 拿**画布像素**与**服务端 `sample_colors` 读数**对表（容差 ±6 ✓ 实测最大单通道差 0 ✓）。
2. **`color` 写成字符串时 `opacity` 被静默丢掉** ✗→✓（`tools.rs`）：实测 `fill_region(color:"#dc2828", opacity=0.5)` 中心 α 仍是 255（1.0／0.5／0.2 三种给法逐字节相同）。与第 1410 轮那条数组形态的缺陷**同型** ✓ 那次只修了三种写法里的两种。
3. **工具描述过时** ✗→✓（`tools_registry.rs`）：`shape` 漏了 `path`、`texture.type:"region"` 写着"尚未实现"（其实第 2680 轮就落地了 ✓）⇒ **过时的描述让人主动放弃可用的能力**。
4. **本地内核（PWA）那条路丢 `opacity`** ✗→✓（`api-local.js`）：关掉「服务器渲染」后同一个滑杆没效果。现在与服务端**同一处落点**（折进颜色的 `a` ⇒ 原子逐字节同形 ✓）；`path` 与位图纹理在本地**如实拒绝并说明是本地缺**（不再报成参数写错 ✓）。
5. **隐藏层的警告被回执行盖掉** ✗→✓：页面先说「这一层当前隐藏 ⇒ 这一笔看不到」✓ 紧接着被「已填充 …」整行覆盖 ✗ ⇒ 变成"点了没反应还说成功"。改成把尾巴并进同一行。
6. **参考图解不开时留一个破图图标盖在画面正中**（`alt` 还是中文）✗→✓：`error` 时收起这一层 + 卡片讲清原因（存储里那份是裸像素 ✓ 重新上传一次就能一直显示 ✓）+ `alt` 进 `I18N_ATTRS` 一起翻。**这条只有看真截图才发现** ✓ 任何判据都没报它。
7. **外部（Agent）`clear_reference` 之后页面还挂着那一层** ✗→✓：偏好改动**不产生原子** ⇒ 没有 head 变化 ⇒ 没有 WS 广播 ⇒ 页面只在 `resize`／`yanshi:document-ready` 重读。现在 `resync()` 也重读一次 ✓（`get_preferences` 显式 `{refresh:false}` ✓ 否则会递归 ✓）。

### 登记 ⑭ ★ CI 给 `browser-*` 一族统一加 `&render=client`，而 URL 参数优先级在 `localStorage` 之上 ★

* 出处：`scripts/run-criteria.sh:313-317`（例外名单）与 `:347`（实参）⇒ 除 5 条例外 ✓ 每条 browser-* 拿到的 viewer URL 都带 `render=client`；`viewer-app.js:83-88` ⇒ 这个参数**直接决定** `serverRenderPreferred()` ✓ 存储擦干净也压不过它。
* 后果：**凡"经服务端 API 写状态、再到页面上读"的判据**在 CI 里读的都是**本地内核那份 IndexedDB 偏好**（`api-local.js:994-999`）⇒ 读不到 ⇒ 恒红；而本机手工跑**不带这个参数** ⇒ 全绿 ✓ **两边都看不见对方** ✗。
* 实测（本轮 ✓）：`browser-reference-overlay.mjs` 带 `render=client` ⇒ `{"found":false,"candidates":11}`（红 ✗）；不带 ⇒ 绿 ✓。**它不是被本轮改坏的** ✓ 是接线形状从来没对过 ✓。
* 修法：判据**自己**在导航 URL 上钉 `render=server`（本轮 4 条新判据 + 上游那 1 条都改了 ✓）。`browser-frequency-separation.mjs` 的「本地内核缺端点」那一臂**故意不钉** ✓ 让它照旧由 `localStorage` 决定 ✓（否则那一臂测不到本地路由 ✓）。
* 同族旧账：第 381 轮给 `ui-control-heights` 分支加过这个参数 ✓ **当时只修了那一个分支** ✗（`run-criteria.sh` 里自己写着这件事 ✓）。

### 登记 ⑮ ★ 新建判据前必须查重：本轮我自己覆盖了上游的一条判据 ★

* `Write scripts/browser-reference-overlay.mjs` 把上游 `0ba66fd9` 引入的**同名 110 行判据整个盖掉** ✗ ⇒ 补丁里等于"删掉一条 CI 判据" ✓ 而它带着本仓约定的 90 s 总超时守卫 ✗。
* 发现方式很便宜：`git status --short` 里它是 `M` 而不是 `??` ✓（新建的文件不可能 M ✓）。
* 处置：恢复上游那份 ✓ 本轮这条改名 **`browser-reference-decode.mjs`** ✓ 并在抬头写明两条的分工 —— overlay 那条用 `render_region` 产的**真 PNG** ⇒ 测"设了就该看得见" ✓ **走不到解不开那条路** ✗；decode 这条用 `POST /api/blob` 的**裸 RGBA**（实测 `application/octet-stream` · 256 字节 ✓）⇒ 专测降级 ✓。**两条合起来才覆盖叠加层的两侧** ✓。
* 教训：**先 `git ls-files scripts/ | grep <名字>`** ✓ 再决定是"扩展"还是"新建"。

### 登记 ⑯ 判据的端口参数：`argv[3]` 是 server-base，不是端口

* `run-criteria.sh:347` 实传 `<viewer-url> <server-base> <token> <cdpPort>` ✓ 端口在 **`CDP_PORT` 环境变量**里 ✓ —— 上游在 `browser-reference-overlay.mjs:20-25` **早就记着这条教训** ✓ 我新写的 4 条**又踩一遍**（`process.argv[3] || "9222"` ⇒ 端口变成一个 URL ⇒ 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ⇒ CI 里连不上 CDP ⇒ **恒 3** ✓）。
* 修法（4 条统一 ✓）：`process.env.CDP_PORT || (argv[3] 是纯数字 ? argv[3] : "9333")` ⇒ CI 形状与本机手工形状**都成立** ✓（本机实测两种调用各跑一遍 ✓）。

### 登记 ⑰ 判据自己也会赛跑：`loadEventFired` 可能属于**上一次**导航

* `browser-reference-decode.mjs` 在导航之后**立刻**读「服务器渲染」勾选 ⇒ 读到 `false` ⇒ 假红（实为上一个判据留下的页面 ✓）。
* 改成**有界等待真状态**：轮询 80×250 ms ✓ 且要求 `location.search` 含 `render=server` ✓ 才认这条前提 ✓。
* **同一处赛跑在另外三条里也存在**（本轮第二遍自查 ✓）：`browser-fill-region.mjs:222`／`browser-blend-modes.mjs:190`／`browser-frequency-separation.mjs:188` 都是**单次读** ⇒ **`browser-blend-modes` 在两条连跑之间一次绿一次 `exit 3`**（真随机 ✓ 不是产品变了 ✓ 顺带把 CDP 上泄漏的 page target 从 4 个清到 1 个再跑 ⇒ 稳）。四处现在都是**同一个形状的有界等待** ✓（`frequency-separation` 的**本地那一臂**也改成"等到 URL 不带 `render=server` 且勾选 = false"✓ 它照旧**故意不钉** ✓ 登记 ⑭）。
* 与 `browser-reference-overlay.mjs:78-81` 那条"只等 `readyState` ✗ **不等被测元素**"是同族纪律：**等前提 ✓ 不等结论** ✓（等结论就等于让断言永不失败 ✓ 第 887 轮犯过 ✓）。


### 登记 ⑱ 超时守卫与"半途而废的偏好"

* 4 条新判据都补了 **180 s 总超时守卫**（外层 browser-* 是 `timeout ${BROWSER_TIMEOUT:-420}` ✓ 内层必须落在它之下 ✓ 否则外层先砍 ⇒ 124 ⇒ 红绿不分 ✗）⇒ 超时报"判据自身失败"退 3 ✓ 不冒充产品失败 ✓。
* `decode` 那条在超时／连不上 CDP 这两条路上**先 `clear_reference` 再退** ✓：参考图记在**工作区级单文件**里（见决策 7 ✓）⇒ 半途挂掉会把"解不开的参考图"留给后面**所有**判据的截图 ✓（本轮那个破图图标就是这么被发现的 ✓）。

### 登记 ⑲ ★ 一条"扫中文残留"的断言可以**什么都没扫到**就报绿 ★

* 症状（本轮自查 ✓）：`browser-fill-region.mjs` 的 ⑧ 报「0 条中文残留（扫了 **58** 条文本与属性 ✓）」—— 而**同一张卡的截图**上图层下拉写着「图层 1 (#layer_default)」✓ ⇒ 扫描**漏看了整类节点** ✗（不是产品没问题 ✗ 是判据没看见 ✓ **假绿** ✓）。
* 根因：那段 `walk()` 在**卡片元素一出现**就读 ⇒ 此时**图层下拉还没被 `list_layers` 填上** ✓ 于是 `<option>` 的文案**一条都不在样本里**（读数 `58` 里没有任何 `option` ✓）。**"扫到 0 条"与"没扫"在断言里长得一模一样** ✗。
* 修法（三件事一起做 ✓）：① **等动态内容就位**才扫（要求 `fillRegionLayer.options.length >= 5` ✓ 等不到 ⇒ `exit(3)` ✓ **不许**在空下拉上判"干净"）；② 扫描时把**界面文案**与**服务端数据**分开收集（`option` 里的文字是**文档数据**——图层名是用户/服务端起的 ✓ 不属翻译面 ✗ 但**照样印出来** ✓ 免得将来又变成"看不见的那一类"）；③ 断言只看界面文案那一半 ✓ 读数改成「50 条界面文本与属性 ＋ **13 个下拉项**」✓。
* 顺带的事实（**只有看真截图才会去查** ✓）：英文界面下那个「图层 1」**不是页面漏翻**——服务端建的默认层名就是中文（`crates/yanshi-core/src/fold.rs:1011` 的 `name: "图层 1"` ✓ `crates/yanshi-server/src/tools.rs:3485` 自动建层用 `format!("图层 {tail}")` ✓）⇒ 属**文档数据**的 i18n（见「需要用户决策」第 12 条 ✗ 本轮不改 ✓）。

### 本轮实测读数（全部用 **CI 的参数形状**复跑：`CDP_PORT=9490 node <判据> "<base>/?doc=…&token=…&render=client" <base> <token> 9490`）

* `browser-fill-region.mjs` ⇒ 12 条断言全绿（⑥ 画布 `[0,0,7,255]` == 服务端 `[0,0,7,255]` ✓ 最大单通道差 0；⑦ 纹理回执「源块平铺 0.75 倍」；⑧ 英文面 **50 条界面文本与属性 ＋ 13 个下拉项** 0 条中文残留 ✓ 0 条 JS 错误 ✓）。
* `browser-reference-decode.mjs` ⇒ 9 条全绿（前提：`/api/blob` 回 `application/octet-stream` · 256 字节 ✓）。
* `browser-reference-overlay.mjs`（上游那条 ✓ 恢复 + 钉 `render=server`）⇒ exit 0 ✓。
* `browser-blend-modes.mjs` ⇒ 全绿；`browser-frequency-separation.mjs` ⇒ 全绿（含「本地内核缺端点」那一臂仍如实走本地 ✓ 说明 ⑭ 的钉法没有把例外臂一起钉死 ✓）。
* **稳定性复跑**（登记 ⑰ 的修法不是"碰巧绿" ✓）：清掉泄漏的 CDP page target（4 → 1 ✓）之后 ✓ `blend-modes` 连跑两次、`fill-region`／`frequency-separation`／`reference-decode` 各再跑一次 ⇒ **全部 exit 0** ✓。
* **变异测试**（证明判据真在测东西 ✓ 三处各击穿一条）：
  · `render.rs` 把羽化那一支的条件改成永不成立（`feather > 1e9` ✓）⇒ Rust 判据 `flat_shape_geometry` **恰好 1 红**（「同一个像素在整幅渲染与 1 行区域渲染下不一样 ⇒ 整幅 `[0,0,0,255]`／窄区域 `[0,0,0,194]`」✓）另两条测试照绿 ✓；
  · `viewer-app.js` 把 `error` 时的 `display = "none"` 改回 `""`（= 修前行为 ✓）⇒ `browser-reference-decode` **恰好 2 红**（① 与 ④ 两条"收起"断言 ✓ 红字里能看见 `display:"block"` 而卡片仍写着"已收起"⇒ 连文案一起说谎 ✓）；
  · 同一次构建里再把 `hiddenTail` 抹成空串 ⇒ `browser-fill-region` **恰好 1 红**（⑧ 隐藏层那条 ✓ 而 `browser-blend-modes` 作为**对照判据**照旧全绿 ⇒ 两条判据没有互相污染 ✓）；
  · 三处变异**全部还原** ✓ 还原后重跑上面那一整批 ⇒ 全绿（`criteria/` 里的读数来自**还原后的构建** ✓）。
* `tool-pwa-assets.mjs` ⇒「防分叉：3 个 viewer 文件与单一源相同」转绿 ✓ 仍红的只剩 `assets/mediums/*.wasm` 本机一个都不存在（与登记 ⑬ 同因 ✓ 属环境前提 ✓ 由 CI 收敛）。镜像 `web/index.html` 按登记 ⑥ 撤回 ✓ 补丁里只留 `web/api-local.js`（纯拷贝 ✓ 无机器戳 ✓）。
* **Web 调用面**（第 8 条复现命令的同一口径 ✓ 只读源码）：基线 `ab0decd0` = **94** ⇒ 本包 = **97**（新增 `fill_region` ✓ `sample_colors` ✓ 加上第三轮那条**尚未进上游**的 `frequency_separation` ✓）。

### 本轮两处**只有看真截图才发现**的界面问题

* 参考图卡片：破图图标盖在画面正中 + 英文模式下 `alt` 仍是中文（已修 ✓ 见上第 6 条）。
* 区域填充卡的数字框**折行不齐**：「宽 60」后面「高 40」掉到下一行 ✓ 而同一行的 `x/y` 不折——**中英两种界面都折**（`en-08-card-english.png` 里 `W 120` 后面 `H 80` 同样掉下去 ✓ 上一版记成"只有中文折"是错的 ✓）⇒ 属排版打磨 ✗ 本轮未改 ✓ 记在这里留住成本与收益。
* 英文界面下图层下拉里的「图层 1」——**不是漏翻**（是服务端起的默认层名 ✓ 见登记 ⑲ 末尾 ✓ 本轮只把它**登记成决策**✗ 没顺手改数据面 ✓）。


---

## 追加（2026-10-27 第五轮）— 减淡加深的 Web 入口、柔光那一支的**方向**缺陷、"复用回执少一格"，与顶栏那个**永远不会自己变**的水位

**口径**：基线 `29d94086`（上游把第三轮那两条收走了：`4790d7d1` 频率分离卡片 + 完整混合模式清单 ✓ `07f675d0` 分带外扩取声明值 ✓ `504b975a`/`29d94086` 文档 ✓）⇒ 本包 = **第四轮全部成果重接新基线** ＋ **第五轮这一条**（`dodge_burn_setup` 的 Web 入口 + 它抓出来的两处产品缺陷）。
工作树对 `29d94086` 的净差 = 18 个已跟踪文件 +5 个新判据（`git diff --stat 29d94086` ⇒ 1640 insertions / 93 deletions ✓ 第三轮那两条**已不在差里**，因为它们在基线里了 ✓）。

### 本轮补掉的"系统已在 + MCP 缺省面内 + Web 零入口"

* `dodge_burn_setup`（减淡加深）此前在页面上**零入口**（`grep -c dodge_burn crates/yanshi-http/` 命中 **0** ✗）⇒ 局部提亮/压暗只能让 Agent 调工具。现在「绘制」面板多一张**减淡加深**卡：源图层下拉（id 序列与 `list_layers` 逐一等）＋ 建组方式（灰层法 / 曲线法）＋ 减淡·加深中点两个区间框 ＋ **按下去之前先把"这次会发生什么"念出来**（组名、放在哪、复用还是新建 ✓ 全按服务端命名规则算）✓ 中英双入口。
* **登记 ㉕ 本地内核（PWA）仍没有 `dodge_burn_setup` 端点**（`grep -c dodge_burn_setup crates/yanshi-http/assets/api-local.js` ⇒ **0** ✗ 与登记 ② `create_range_mask`／登记 ⑨ `frequency_separation` **同一族**）⇒ 本地模式下点「建组」拿到 `endpoint_not_local`。本轮做的是**把这条歧路说实话**：请求**根本没出门** ⇒ 界面念「未建组：本地内核没有 dodge_burn_setup 这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试」✓ 而不是笼统念"服务端拒绝了"（那会把人送去翻服务端日志 ✗ 该改的其实是那个勾选 ✓）。**补端点本身属本地内核工作量**（要 `add_layer` + 蒙版 + 曲线一整套 ✓）⇒ 见「需要用户决策」第 13 条。

### 登记 ⑳ ★ 上游收走我的判据之后又改了它 ⇒ 我本地那份变成"同名但更旧" ★

* 现象（重接基线时 ✓）：`scripts/browser-range-mask.mjs` 在基线里**已存在**（`git cat-file -e 29d94086:scripts/browser-range-mask.mjs` ⇒ yes ✓ 我本地也留了一份同名的 379 行副本）。若照名字判"已经有了"就完事 ⇒ 补丁会**要么冲突、要么用旧覆盖新** ✗。
* 逐行 diff 的结论：**上游那份是超集**（392 行 ✓ 修了两处我那份没修的：① `const port = process.argv[3] || "9222"` ✗ 而 `run-criteria.sh` 的 browser 分支传的是 `<url> <base> <token> <port>` ⇒ **argv[3] 是 base 不是端口**（正是登记 ⑯ 那个坑 ✓ 上游按 `CDP_PORT` 优先 + 只认纯数字修掉了）；② 固定 `sleep(2000)` 读回执 ✗ 本地内核首调要 2.5 s ⇒ 读到空串判红，上游换成**有界轮询** ✓）。
* ∴ 处理：本地副本**退出工作树**（留档 `/tmp/wip-backup-r4/local-duplicate-browser-range-mask.mjs` ✗ 没删 ✓）⇒ 补丁里不再有这一条。
* **可复用的规则**：判断"上游是否已含我的工作"要看**内容**不看文件名 ⇒ `diff <(git show <base>:<file>) <file>` 之后**问哪一份是超集**；同名不同内容 ≠ 重复。

### 登记 ㉑ ★ `soft_light` 的亮支写错 ⇒「减淡」把画面**压黑**（中性测试看不见这一类缺陷）★

* 症状（实测 ✓ `tool-dodge-burn-effective.mjs` 的 B 臂）：灰层法建组后往灰层**涂白**（这就是"减淡"这一步 ✓ 白 = 提亮），底色 128 应该升到 **182** ✗ 旧实现给 **19** ⇒ **越减淡越黑**（差 163 级 ✓）。
* 根因（`crates/yanshi-render/src/blend.rs:182` ✓ W3C 的 `Cb ≤ 0.25` 那一支）：`d = ((16·Cb − 12)·Cb + 4)·Cb` 的最后一个 `× Cb` 被写成 `× √Cb` ⇒ `D(0.2)` 从规范的 `0.448` 变成 `0.01448`。
* **为什么这缺陷能活这么久**：这一族的既有测试全是**中性/幂等**型（"灰层铺上去画面不动"✓ 两端点 ✓ `Cb=0.25` 接缝）⇒ 而 `√Cb` 这个错**恰好不动中性**（顶是中性灰时 `Cb` 怎么算都不影响"不变"这条性质 ✓）⇒ **中性判据对方向天然瞎** ✗。修法是把**方向**钉成断言：同一张灰层，**涂白必须变亮、涂黑必须变暗**，两条各测一支 ✓ 外加一条**与实现无关的结构判据**：两支在 `Cb = 0.25` 上必须给同一个数（`D=0.5=√0.25` ✓ 写错系数时它立刻断）。
* **顺带抓到的测试自身缺陷**：`blend.rs` 那条单测原先的第三个对照值写 `≈0.0877` ✗ 那**不是规范值 ✓ 是照着当时那个 bug 算出来的** ⇒ 一条"对照值来自实现"的测试守不住任何东西 ✗（本轮改成规范闭式 `0.3488` ✓ 并且这条断言在旧实现下**直接红** ✓）。
* 变异证据（证明判据真在测这一支 ✓）：把 `((16.0*cb-12.0)*cb+4.0)*cb` 还原成 `cb.sqrt()` ⇒ ① `cargo test -p yanshi-render --lib` 的 `blend_soft_light_ps_reference_values` 红；② `tool-dodge-burn-effective.mjs` 的 B 臂报 `[19,19,19,255]` 红；其余臂照绿 ✓ 三处还原后全绿。

### 登记 ㉒ 减淡加深的"中性灰"不是 128 而是 **188**（线性缓冲那一层的事实）

* 本引擎在**线性**缓冲上做混合 ⇒ W3C 的 50 % 软光中性（线性 0.5）落到 8-bit 编码是 **188** ✓ 不是显示值的 128。灰层铺 128 的话 ✗ **点一下"建组"这一步自己就把 128 的底色压到 97**（整幅暗 31 级 ✓ 实测）⇒ "建组不该改画面"这条不变量当场破。
* 现在：`DB_NEUTRAL_GRAY = 188` 由服务端**回执**给（`neutral_gray` ✓ 界面**照读**、不写死 ✓ 判据 ③ 逐位对过 ✓），A 臂实测六个底色带**逐点不变**；`sample_colors`（HTTP 路径）残差 0 ✓ 引擎 `render_region_raw` 多一道 f16 tile ⇒ **诚实的界是 ≤1 LSB**（界面文案与判据都按 ≤1 LSB 说话 ✓ 不吹"完全相同"）。

### 登记 ㉓ `mode=curves` 那一臂**改不到画面**，却回 `ok: true`

* 事实：曲线法建出来的调整对象**只作用于自己图层的缓冲** ✓ 对本引擎的合成结果**一个像素都不动**（A/B/D 臂实测：蒙版涂满白之后六个底色带**仍逐字节相同** ✓ ⇒ "改不到画面"不是推测）。
* 旧行为：回执 `ok:true` ✗ 静默 ⇒ 界面念"建好了"，用户以为提亮了。
* 现在：服务端回 `effective:false` ＋ `ineffective_reason`（逐字中文 ✓）✓ 界面卡片上**明写**「⚠ 组建好了，但服务端回执 effective=false ⇒ 这一臂不会改变画面」✓ 完整理由同时留在 `#log`（不复述 ✓ 也不丢 ✓）。

### 登记 ㉔ ★★ **复用分支的回执整格漏了 `mode`** ⇒ 界面念出 `method undefined`；而"页面 == 回执"这种比对**看不见它** ★★

* 这条是**新判据第一次跑红抓出来的**（✗ 不是先验写进判据的 ✓）：`tools_retouch.rs` 的 `already_exists` 那一支构造回执时没带 `mode` ⇒ 英文界面整句念成 `Reusing existing group … · method undefined` ✓ 中文界面念「方法 undefined」。
* 更深一层（顺着这条才查出来的 ✓）：那一支的 `effective` / `ineffective_reason` / `neutral_gray` 是按**本次请求**的 mode 推的 ✗ 不是按**磁盘上那个组**推的 ⇒ 灰组已存在而这次请求 curves 时，回执说它"改不到画面"（**方向反了的谎** ✓ 那个组其实一直在生效）；反过来 curves 组被请求成 gray 时又谎报生效。
* 修法（三层一起 ✓ 缺一层就还有一处说谎）：① 服务端复用分支**从子层反推真实模式**（`soft_light` 子层 ⇒ gray ✓ `减淡 · Dodge` 曲线层 ⇒ curves ✓ 都没有 ⇒ `null` + 说明"这个组里已经没有减淡加深的层了"），`effective`/`neutral_gray`/`note` 一律**照磁盘回答**并在 `note` 里点出"本次请求的是 X、这个组是按 Y 建的"；② 界面读不到 `mode` 时兜一句 `未知` / `unknown`（**不再拼 undefined** ✓）；③ 判据加一条**字段存在性**断言。
* **★ 为什么"页面 == 回执"这种比对不够 ★**：那条断言比的是"页面念的字符串与服务端字段是否同源"✓ 而这一例是**回执自己少一格** ⇒ 页面照抄回执也就一起少一格 ⇒ **两边一致地缺** ⇒ 比对通过 ✗ **假绿**。⇒ 凡"界面复述服务端字段"的判据 ✓ 必须**另外**钉一次"该字段存在且取值在合法域内"（本轮是 `③b` 那条 `mode=gray` + `· 方法 gray ·` 正则 ✓ 变异时它先红 ✓）。

### 登记 ㉖ 判据把 `/tmp`（tmpfs）写满之后 ⇒ **shell 本身不可用**，症状长得像"工具坏了"

* 现象（本轮实测 ✓）：连续跑 13 条判据之后 ✓ **每一条 Bash 调用都只回 `Exit code 1`、正文全空**（连 `echo ok` 也一样 ✗）。看着像工具坏了 ✗ 其实是 **`/tmp` 配额写爆**：命令包装器收尾要往 `/tmp/qoder-<id>-cwd` 写 60 字节 ✗ 写失败 ⇒ 整条命令被判失败且输出被吞。
* 诊断路径（记下来省时间 ✓）：Read `/proc/meminfo` 排除内存（`MemAvailable` 还有 3.8 G ✓）→ Read `/proc/loadavg` 排除负载（0.50 ✓）→ 用 **Write 工具**探一次写（`EDQUOT` ✓ 一击命中）→ 才轮到 `df -h /tmp`。
* 处理（**搬走、不删** ✓ 可逆）：把陈旧 scratch 移到 `/home/polar/stale-tmp-20261027/`（`verify-tarball` 516 M ✓ `patchtest`/`baseline-check`/`yanshi-clean-ab0decd0` 各 ~63 M ✓ 三天的 `pwa-*`/`dbg-prof.*` 各 22–40 M）⇒ `/tmp` 从 80 % 回到 64 % ✓ shell 立刻恢复 ✓ 本轮需要的 `yanshi-root`／`yanshi-web-r5`／`*.good` 备份**一个没动**。
* ∴ 与内存那条同族（用户明确提醒过 OOM ✓）：**多轮之后必须清理自己造的临时目录**；写满 tmpfs 会让后续一切**看起来是工具故障**✗ 别按工具故障去查。

### 登记 ㉗ ★★ 顶栏 `head 25 / rendered 22 / dirty 0 / 已保存` —— 三个数互相打脸，而那个 `22` **永远不会自己变** ★★（看整页截图才查出来的 ✓）

* **症状**（本轮两张整页截图 ✓ 中英文都这样）：`head 25` 而 `rendered 22`、同时 `dirty 0` 且写着「已保存」⇒ 用户读到的是"画布还差 3 个原子没画"✓ 而另外两格说它画完了 ✗。这不是 dodge-burn 那条卡片的错 ✓ 是**顶栏**的错。
* **先排除服务端**（这一步决定改哪边 ✓）：HTTP 探针直调 `get_document` ⇒ 每次真实改动后服务端的 `rendered_seq` **都等于** `head_seq`（`+layer 2/2` ✓ `+fill 3/3` ✓ `+group 6/6` ✓）⇒ **水位本身没算错** ✗ 红在页面显示。唯一合法落后的那一格是"从没落过墨的全新空文档"（`1 / 0` ✓ 服务端确实没出过预览）。
* **两条独立成因**（都在 `viewer-app.js` ✓ 都修了 ✓）：
  A `refreshThumb()` 是 `#rendered` 唯一的**服务端来源**写者 ✓ 而它每次改动**只读一回**（400 ms 去抖 ✓）⇒ 那一回正好读在服务端还没追上的窗口里 ⇒ 之后**无人重读** ✓ 数字搁死；
  B **经 WS 到达的原子**（别的客户端 ✓ 或自动化直打 HTTP ✓）只把 `head` 往上推 ✓ 没有任何人去重读水位 ⇒ 探针实测：开局 `1 / 0` ⇒ 远端连建三层 ⇒ 顶栏停在 `4 / 0 / 0` **六秒不动** ✗ 而同一时刻服务端早就是 `4 / 4`。
* **修法**（两处各一行级别 ✓ 不新增 UI ✓ 不动模板）：读到 `rendered_seq < head_seq` 就**再排一次去抖读**（上限 6 次 ✓ `head` 一变即归零 ⇒ 上限只约束"同一次落后反复追问"✓ 永不落墨的文档也不会把连接打满）；WS 原子事件里补一次 `scheduleThumbRefresh()`。
* **新判据 `browser-status-bar.mjs`**（18 条 ✓ exit 0 ✓ 已加进 `run-criteria.sh` 那条例外名单 ⇒ 不注入 `&render=client` ✓ 本地内核不去读服务端像素 ⇒ 注入了就永远追不平 ✗ 红得没道理）：① 载入后 `rendered` 有界时间内追平 `head` ✓ ② 轨迹全程 `rendered <= head` 且 `head` 不降时不倒退 ✓ ③ 远端连建 3 层后同样追平 ✓ ④ 追平那一刻页面数字与**同一时刻**服务端 `get_document` 逐位等 ✓ ⑤ 英文页重复 ①③④ ✓ ⑥ 全程 0 条 JS 错误 ✓。
* **两次变异各自被独立钉住**（已还原 ✓ 详见「本轮实测读数」那条）：抽掉 B（WS 那次重读）⇒ 只有 ⑤ 红（`head 2 / rendered 1` 停死 9 秒）✓ 抽掉 A（追平循环）⇒ ①⑤ 红（开局 `1 / 0` 停死 9 秒 ✓ **45 次读数一次没变**）。
* **顺带查明的一条事实**（写下来免得下轮又当缺陷查 ✓）：服务端**有时**会广播 `thumbnail` 事件替页面重读 ⇒ 变异 B 下"三层那个用例"碰巧绿、"一层那个用例"红 ⇒ **收敛不能靠它** ✓ 页面自己要保证（这正是那条判据要跑两种文档形状的原因 ✓）。
* **一处刻意没动**（语义归属问题 ⇒ 见「需要用户决策」第 15 条 ✗ 不由一轮补丁顺手定）：`#rendered` 现在有**两个含义不同的写者** —— `refreshPreview()` 乐观写 `rendered = head / dirty = 0`（"客户端画完了"✓）与 `refreshThumb()` 抄服务端水位（"服务端预览到第几"✓）。修完之后三个数**自洽**了 ✓ 但那一格的**含义**仍是二义的；`dirty` 同理（它来自服务端 tile 失效事件 ⇒ 新截图里 `head 25 / rendered 25 / dirty 2` 与「已保存」**并存是合法的**：那 2 是**服务端**的脏块 ✓ 不是"你没保存"✗）。要正名就得同时改 `viewer.rs` 模板 + 中英词条 + 若干判据 ⇒ 成本与收益摆在这里 ✓ 本轮只把"搁死的假数"修成"会自己收敛的数"。

### 本轮实测读数（**CI 的参数形状**：`CDP_PORT=9490 node scripts/<判据> "<base>/?doc=…&token=…" <base> <token> 9490` ✓ 全文在 `criteria/`）

* `browser-status-bar.mjs`（**新** ✓ 18 条全绿 ✓ exit 0 ✓ 登记 ㉗ 的守卫）：`① 载入后追平 1/1` ✓ `②/① 2 次读数全程 rendered<=head 且不倒退` ✓ `③ 远端建 3 层 ⇒ head 跟到 4 ✓ rendered 自己追平 4/4` ✓ `④ 页面 4/4 == 同一时刻服务端 4/4` ✓ `⑤ 英文页 1/1 → 2/2 ✓ 页面 == 服务端` ✓ `⑥ 0 条 JS 错误` ✓。
* `browser-dodge-burn.mjs`（**新** ✓ 22 条全绿 ✓ exit 0）：② 预测句「将新建组 「p2 · 减淡加深」（放在「底」上方）」被服务端**逐字兑现**（组名相同 ✓ 组 `z_index=1` 紧贴 `srcZ=0` 之上 ✓）；③ 回执行整串逐位对上（`已建组 layer_… · 方法 gray · 层 1 个（layer_…） · 对象 1 个（obj_…） · 蒙版 减淡 无 · 加深 无 · 灰层中性字节 188`）；**③b** 复用回执 `mode=gray` 存在（㉔ 的守卫 ✓）；④ 建组后合成读数 `128→128`（drift **0** ≤1 LSB）；⑤ 新组回到下拉（options 2→4 ✓）；⑥ 中点 1.5 **就地拒绝**：说出区间 ✓ **零新原子**（6→6）✓ 零新层；⑦ 曲线臂的 `effective=false` 警告**在卡片上看得见** ✓ 逐字中文理由完整在 `#log`；⑧ 同名二次点击念「复用既有组」✓ 层数/原子数一个没多；⑨ 英文三行动态句子**整句翻译**（`Group name "p2 · Dodge and burn" already exists ⇒ this run reuses it…` ✓ `Reusing existing group … · method gray · … grey layer neutral byte 188` ✓ **用户填的前缀没被翻** ✓）；全程 **0 条 JS 错误** ✓。
* `tool-dodge-burn-effective.mjs`（**新** ✓ 10 条全绿 ✓ exit 0）：A gray 建组后六个底色带逐点不变（中性 188）✓ B 涂白 128→**182**、涂黑 128→**61**（两个方向都钉 ✓ 旧实现给 19 ✗）✓ B 没碰的带逐点不变（局部性 ✓）✓ C 复用字段与新建一致 ✓ C-2 复用回执带 `mode` ✓ **C-3** 同名跨模式调用照磁盘回答（`gray`／`effective=true`／`188` ✓ note 点出这次请求不一致）✓ **C-4** 复用的灰组上涂白仍变亮（128→182 ⇒ 那句 `effective=true` 是真话）✓ D 曲线臂自报 `effective:false` 且**蒙版涂满白后六带逐字节相同**（"改不到画面"落到了像素上 ✓）。
* Rust 判据（单 crate、`-j 2` ✓ 低内存）：`-p yanshi-server --test retouch_tools` ⇒ **7 passed / 0 failed / 0 warnings**（含新增的 `dodge_burn_setup_reuse_reports_the_existing_group_mode` ✓ 它把 ㉔ 钉死：跨模式复用必须照磁盘报 `gray`/`effective=true`/`188`，灰层被删后再调必须报 `mode:null`+`effective:false`）；`-p yanshi-render --lib` ⇒ **553 passed / 0 failed**（含 ㉑ 那条含支接缝的柔光测试 ✓ 规范闭式值 + 白顶必须提亮）；`-p yanshi-render --test flat_shape_geometry` ⇒ 3 passed。
* 其余复跑（同一构建 ✓）：`browser-fill-region` / `browser-reference-decode` / `browser-blend-modes` / `browser-frequency-separation` / `browser-reference-overlay` / `tool-fill-region-opacity` / `check-criteria-have-exit3` / `ui-i18n-coverage` / `ui-text-density` / `check-viewer-unique-ids` ⇒ **全部 exit 0** ✓；`tool-pwa-assets` ⇒ 仍只剩那条**环境前提**红（本机一个 `assets/mediums/*.wasm` 都没有 ✓ 与登记 ⑬ 同因 ✓ 由 CI 收敛）。
* **本轮被自己判据抓到的一次流程遗漏**：改完 `viewer-app.js` **忘了跑镜像同步** ⇒ `tool-pwa-assets` 报「viewer-app.js 的副本与现有 WEB 的源**不同**（除缓存戳外）⇒ 两条部署分叉了 ✗」（`criteria/tool-pwa-assets.txt` 头部记了这条原文 ✓）。按登记 ⑥/⑬ 的流程补跑 `node scripts/pwa-sync-viewer.mjs` + 撤回 `web/index.html` ⇒ 那一行转绿 ✓ **判据替我记住了流程**，这类"忘同步"以后必红。
* **变异测试**（证明本轮新写的判据真在测东西 ✓ 五处击穿各自**恰好**落在对应断言上 ✓ 全部已还原）：
  · `blend.rs` 的柔光亮支还原成 `cb.sqrt()` ⇒ `cargo test -p yanshi-render --lib` 红一条 + `tool-dodge-burn-effective` 的 B 臂红（`[19,19,19,255]`）✓ 其余臂绿；
  · `tools_retouch.rs` 复用分支改成"照**请求**的 mode 回答"（等价于漏掉 `mode`/照请求推 effective ✓ 原写法 `Some(mode.as_str())` 编译不过 ⇒ `E0597 mode dropped while borrowed` ✓ 换成能编译的等价版本）⇒ 新 Rust 测试**恰好**停在 `left "curves" / right "gray"` ✓ 那条断言的文案直接说出后果（「复用回执必须带上模式 ⇒ 界面念 undefined」）；
  · **顶栏那两处（登记 ㉗）各自单独抽掉 ⇒ 重编译 ⇒ 重跑 `browser-status-bar`**：抽掉"读到落后再排一次"那个循环 ⇒ ①⑤ 红（开局 `head 1 / rendered 0` 停死 9 秒 ✓ **45 次读数一次没变**）；抽掉 WS 原子事件里那次 `scheduleThumbRefresh()` ⇒ ⑤ 红（远端建一层后 `head 2 / rendered 1 / dirty 0` 停死 9 秒）⇒ **两条都不是"看起来更 robust"的装饰代码** ✓ 各自都有一条断言指着它；
  · 五处变异**全部还原** ✓ 还原后重跑上面那一整批 ⇒ 全绿（`criteria/` 里的读数来自**还原后的构建** ✓ 构建新鲜度用 mtime 判 ✓ 横幅字符串被 `build.rs` 缓存 ✗ 不可信）。
* **Web 调用面**（本轮把口径写死在这里 ✓ 与第四轮那句"94 ⇒ 97"**不是同一个数**——那条把 `viewer.rs` 的动态分支一起算 ✗ 而本轮改动全在 `viewer-app.js` ✓）：**`viewer-app.js` 里 `callTool("…")` 字面命中的不同工具名** ⇒ 基线 `29d94086` = **87** ✓ 本包 = **90**（净增三个：`fill_region` ✓ `sample_colors` ✓ `dodge_burn_setup` ✓ `comm -13` 实测 ✓ 命令见「复现命令」第 11 条）。
* **真截图**（`screenshots/` ✓ 两种语言各一对：建组前／建组后 ✓ 两张整页 ✓ 外加顶栏特写 `07-zh-顶栏.png`）：灰层法建组后**那道渐变没有被压暗**（对着 ㉑/㉒ 那个历史行为看 ✓ 中性字节一旦铺成 128，"点一下建组"这一步就把画面整体压暗 31 级——这件事**只有在图上才会去看**，数字判据只给"128→128"✓）；英文卡片整卡翻译到位 ✗ 只剩**用户数据**那一处（层名「底」是文档数据 ✓ 见决策 12）；曲线法那张卡的 ⚠ 警告在截图里**看得见**（不是只躺在 `#log` ✓）。

### 本轮三处**只有看真截图才会去查**的界面问题

* **顶栏那三个数字互相打脸**（→ 登记 ㉗ ✓ 本轮唯一"读代码看不出来、判据也不会红"的一类）：卡片与日志全都对 ✓ `criteria/*.txt` 全绿 ✓ 而整页截图上写着 `head 25 / rendered 22 / dirty 0 / 已保存` ✗ 那个 `22` 永远不会自己变 ✓ 修完再看同一张图：`head 25 / rendered 25 / dirty 2` ✓ 顶栏特写那张 `head 14 / rendered 14 / dirty 0` ✓。**只看判据输出会漏掉它**（当时的判据一条都没红 ✓ 因为没有任何一条去读顶栏 ✓）⇒ 新写的 `browser-status-bar.mjs` 就是把这一格纳入判据 ✓。
* **建组这一步本身会不会动画面**——判据给的是"底色仍是 128（drift 0）"这样一个数 ✓ 但**"减淡加深"这个名字承诺的是"不改变画面地准备好两把刷子"** ✓ 只有把建组前后两张截图并排看，才会去查"中性字节到底是不是这个引擎的中性"（→ ㉑/㉒ 那条 128 与 188 的缺陷 ✓ 修前那张图上渐变**整段发暗**）。
* 本轮**自己第一版卡片**把曲线臂的理由只写进 `#log` ⇒ 卡片正文一行"建好了" ✓ 图上完全看不出这一臂不生效 ✗（看截图时发现的 ✓ 现在警告直接进卡片正文 ✓ 判据 ⑦ 钉住"卡片上可见 + `#log` 里逐字完整"两件事）。
* **一处刻意没改**（只登记 ✓ 留住成本与收益）：卡片上"预测句"（将新建组…）与"回执行"（已建组…／复用既有组…）**样式一样** ✓ 用户不容易分清哪句是**许诺**、哪句是**兑现**。这是本仓**全局卡片惯例**（频率分离卡、区域填充卡同形 ✓）⇒ 单独给这一张卡换样式会比不换更乱 ✗ 要改就整套卡一起改 ⇒ 见「需要用户决策」第 14 条。


---

## 追加（2026-10-27 第六轮）— 锁定层那种"按了没反应"、层行 tooltip 的中文残留、以及 curves 那条 ⚠ **被上游推翻**

**口径**：基线 **`e0fd1052`**（本轮 `git pull` 后上游最新 ✓ HEAD == `origin/main` ✓ 本地零提交）⇒ 本包 = **第四、五两轮的全部成果重接这条新基线** ＋ **第六轮这一条**。
**第五轮那几处仍未被上游收走**（实测：`git show e0fd1052:crates/yanshi-server/src/tools_retouch.rs | grep -c DB_NEUTRAL_GRAY` ⇒ **0** ✗ `blend.rs:175` 仍是那条写错的 `((16cb−12)·√cb+4)·cb` ✓）⇒ 柔光亮支与中性 188 那两处**还在本补丁里** ✓ 合并时请按第五轮那一节读 ✓。
工作树对 `e0fd1052` 的净差 = **20 个已跟踪文件 + 8 个新判据 = 28 个路径**（`git diff origin/main` ⇒ 480 KB 补丁 ✓ 在 `git clone --shared` 出来的干净 `e0fd1052` 上 `git apply` 后**逐字节复现**本工作树 ✓ 28/28 路径 `cmp` 全等 ✓）。

### 登记 ㉘ ★ 锁定层上落笔：服务端早就拒了（403），Web 这边却只剩"按了没反应" ★
* 事实（本轮实测 ✗ 不是读代码猜的）：`draw_stroke`／`erase`／`draw_shape`／`draw_text`／`fill_region`／`clone_stamp`／`heal_stamp`／`smudge`／`liquify_*`／`move_object`／`transform_object`／`delete_object` 在锁定层上全部 **HTTP 403 `permission_denied`** ✓ 服务端**一直是对的** ✗ 而界面上那一次失败的**全部**痕迹是 `#log` 里两个字——`#log` 在 `data-panel="history"` 那张**默认收起**的卡片里（实测 `logVisible:false`、`scrollHeight:0` ✓）⇒ 用户得到的是"点了没反应"✓；`render=client` 下更糟：本地乐观像素**回滚不干净** ⇒ 画布上留下一笔服务端根本没有的**幻影墨** ✓。
* 现在：落笔**之前**按 `LOCK_BLOCKED_TOOLS` 预检拦截（请求根本不出门 ✓）＋ 画面正中一块**看得见**的浮层 `#stageNotice` 说明原因 ✓ 文本工具那条分支在 `window.prompt` **之前**就拦 ✓。
* 守卫：`scripts/browser-layer-lock.mjs`（**新** ✓ 32 条断言全绿 ✓ 17 s）—— 钉"服务端事实 + 屏幕事实"两样：原子数一格不多 ✓ **`/api/tools/*` 请求计数为零** ✓ 浮层 `hidden:false` 且在视口内 ✓ 解锁后同一把拖动**真的能画上去**（防"这工具本来就画不上"那种假绿 ✓）✓ 锁着时**矩形蒙版照旧成功**（不许误伤 ✓）✓ 客户端模式下零幻影像素 ✓。

### 登记 ㉙ ★ 锁是"另一头"上的：同事／MCP 改了 `locked` ⇒ 预检的缓存一开始不知道 ★
* 顺着 ㉘ 才暴露的缺口：预检读的是界面自己那份图层表 ✓ 别的客户端改的锁**不在里面** ⇒ 这一笔必然出门并被 403 兜住 ✗ 而界面随即**继续**用旧的缓存劝退后续每一笔（看起来像"这层永远画不了"✓）。
* 现在：收到 `permission_denied` ⇒ **`refreshLayers()` 重取状态** ✓ 于是下一笔的预检与磁盘一致 ✓（收敛在 ⑪ 那条断言里：外部上锁 ⇒ 第一笔被拒 ⇒ 刷新 ⇒ 第二笔**由预检就地拦下** ⇒ 请求计数不再上升 ✓）。

### 登记 ㉚ 层行 tooltip 整段是中文（英文模式下也是）⇒ 属性级 i18n
* 事实：`title`／`aria-label` 这类**属性里的句子**不在 `data-i18n` 节点里 ⇒ 第五轮那条 `ui-i18n-coverage` 只看**文本节点** ✓ 于是**全绿而屏幕上仍有中文**（`browser-i18n` 也扫不到——它扫的是 `textContent` ✓）。
* 现在：`attrEnglish()` + `I18N_EN_ATTR_PATTERNS` 把属性句按模式翻（`localKernelUnusableNow()` 那句降级说明同批）✓ 词条到 `m36`。
* 守卫：`scripts/browser-tooltip-i18n.mjs`（**新** ✓ 英文模式实读 `title` 属性 ✗ 不看文本节点 ✓）。

### 登记 ㉛ ★★ 一条"扫中文残留"的断言，可以**什么都没扫到**就报绿（第五轮 ⑲ 的同族复发）★★
* 事实：`browser-i18n.mjs` 的扫描集合原本**跳过了** `data-i18n="off"` 的豁免区 ✓ 但它同时把"我到底扫了几个标记点"这件事**报成 0 也算通过** ⇒ 变异（把豁免判断写坏）实测读数 `distinct:1, dataMarks:0` **仍然绿** ✓ 也就是"豁免区里的中文"这一族缺陷**在这条判据上是不可见的** ✓。
* 现在：判据必须**自证覆盖面**——`dataMarks ≥ 1` 才允许报绿（红/绿各实测过一次 ✓ `mutation-H-noexempt-raw.txt`）✓。

### 登记 ㉜ ★★★ 上游 `302d414d` 把 ㉓ 那条前提**推翻**了 ⇒ curves 的 `effective:false` 成了新的谎 ⇒ 断言与回执**同时翻向** ★★★
* ㉓ 写的是"曲线臂的调整对象只作用于自己图层的缓冲 ⇒ 一个像素都不动"✓ **当时属实**（15900 实测：蒙版涂满白后六个底色带逐字节相同 ✓）。上游 `302d414d fix(render): seed adjustment-only layers from the content below`（`layer_is_adjustment_only` ⇒ `render.rs:2125/2921`）改成：**纯调整层渲染时先用下方内容播种**再套调整 ✓ 于是 curves **真的改得到画面** ⇒ 而服务端仍回 `effective:false` ＋ 界面卡片仍明写「⚠ 这一臂不会改变画面」✗ **那才是静默的反面：响亮地说假话** ✓。
* 怎么发现的：**不是读代码读出来的** ✓ 是本轮跑判据时 `cargo test -p yanshi-server --test retouch_tools` **红了**（`left: Some("8616 个通道不同｜单通道最大差 31…前 [128,255,3,255] 后 [159,255,5,255]")`）＋ `tool-dodge-burn-effective` 的 D 臂红 ✗ 而第五轮那条测试的注释里**预先写了**"谁把跨层调整修好 ⇒ 这两条会红 ⇒ 那时该同时翻断言与撤 `effective`"✓ **照约定翻** ✓ 不是改口 ✗。
* 重测（15900 ✓ 六个 8 px 底色带 ✓ 读数全部来自服务端 `sample_colors`）：建组（两张蒙版全黑）⇒ 六点**逐字节不变** ✓ 蒙版确实是闸门；dodge 蒙版涂白 ⇒ `60→77`／`128→159`／`200→216`／`(200,30,30)→(216,40,40)` ✓ 而 `0` 与 `255` 两个端点**不动** ✓；burn 蒙版再涂白 ⇒ `128` 从 159 降到 **134** ✓ 两条臂方向相反 ✓。
* 于是：`effective` 的语义在注释里**写清楚是"能力"不是"这份文档此刻的像素"** ✓（唯一还能报 false 的情形是"同名组里那两层已被人删过"的空壳复用 ✓ 那条路径原样保留 ✓）；`viewer.rs` 选项文案与 `tools_registry.rs` 的 summary 不再说 curves 无效 ✓ 改成"两条路都会改变画面 ⇒ 差别只在**留痕方式**"✓；三条 i18n 词条同步（`灰层法（涂就见效）`／`曲线法（曲线可回改）`）✓。
* **变异双向证明**（第六轮新做 ✓ 各编译一次 ✓ 都已还原）：
  · **M1 只改字段**（服务端 curves 的 `effective` 写回 `!is_curves`）⇒ `tool-dodge-burn-effective` 的 D 臂**第一条**红 ✓ 画面那几条仍绿 ⇒ 说明"字段被问"这件事有守卫 ✓（`mutation-M1-noflip-raw.txt`）；
  · **M2 只改渲染**（`layer_is_adjustment_only` 恒 `false` ⇒ 播种退回第五轮之前 ✓ 而 `effective` 保持诚实的 `true`）⇒ D 臂**"涂白必须变亮"与"再开 burn 必须更暗"两条红** ✓ `cargo test -p yanshi-server --test retouch_tools` 也停在 `retouch_tools.rs:395` 那条像素断言 ✓（`mutation-M2-noseed-raw.txt`／`mutation-M2-rust-raw.txt`）⇒ **回执与画面两处都被问**：只谎报字段会被画面抓住 ✓ 只退回渲染会被断言抓住 ✓。
* 第五轮那句「曲线法那张卡的 ⚠ 警告在截图里**看得见**」随本条**作废** ✓（原文保留 ✓ 那是当时的实测；现在 ⑦ 钉的是"页面必须与同一时刻的回执同源"⇒ 该撤的 ⚠ 撤掉、curves 不许念「中性字节」、服务端逐字 `note` 完整留在 `#log` ✓）。

### 本轮判据读数（`e0fd1052` + 本补丁 ✓ 构建新鲜度按 mtime 判 ✓）
* 新增两条：`browser-layer-lock` ⇒ **32 条全绿 / exit 0 / 17 s**；`browser-tooltip-i18n` ⇒ **exit 0 / 3 s**。两条都在自己的导航里**钉死** `render=server` ⇒ 在 CI 给 `browser-*` 的缺省形状（`&render=client`）下也绿 ✓ 因此**不需要**改 `run-criteria.sh` 的例外名单 ✓（第五轮 ⑭ 那条教训的复用）。
* 复跑全绿：`browser-i18n`／`browser-dodge-burn`（22 条 ✓ ⑦ 已翻向）／`browser-status-bar`／`browser-fill-region`／`browser-reference-decode`／`browser-blend-modes`／`browser-frequency-separation`／`browser-reference-overlay`／`tool-fill-region-opacity`／`tool-dodge-burn-effective`（14 条 ✓ D 臂三件事都验）／`check-criteria-have-exit3`（259 个用 `fetch(` 的判据**零**缺 `process.exit(3)` ✓）／`ui-i18n-coverage`／`ui-text-density`／`check-viewer-unique-ids` ⇒ **全部 exit 0** ✓。
* Rust（单 crate、`-j 2`）：`-p yanshi-server --test retouch_tools` ⇒ **7 passed**；`-p yanshi-render --lib` ⇒ **572 passed / 7 ignored**（含 ㉑ 那条含支接缝的柔光测试 ✓）；`-p yanshi-render --test flat_shape_geometry` ⇒ **3 passed** ✓；`cargo fmt --check` ⇒ 0 ✓；`cargo clippy -p yanshi-render -p yanshi-server -p yanshi-http -- -D warnings` ⇒ 0 ✓；`bash scripts/pre-commit.sh` ⇒ **可以提交** ✓（附带一条 `target/debug 已 14G` 的清理建议 ✓ 未动）。
* 唯一红：`tool-pwa-assets` ⇒ 仍是**环境前提**那一条（本机 `web/mediums/*.wasm` 一个都没有 ✓ 与登记 ⑬ 同因 ✓ 由 CI 收敛）✓ 而**防分叉那一行是绿的**（「3 个 viewer 文件与单一源相同」⇒ 本轮改完 `viewer-app.js` 后按流程跑了 `pwa-sync-viewer.mjs` 并撤回 `web/index.html` ✓）。
* **Web 调用面**（同一口径：`viewer-app.js` 里 `callTool("…")` 字面命中的不同工具名 ✓ 命令见「复现命令」第 11 条）：基线 **`e0fd1052` = 87** ⇒ 本包 **= 90**（净增仍是 `fill_region`／`sample_colors`／`dodge_burn_setup` 三个 ✓ `comm -13` 实测 ✓ **第六轮零净增**——本轮全是接线与判据 ✓）。
* **真截图**（`db-shots/` ✓ 1500×1000 视口 ✓ 直接从 CDP 抓）：`01-curves-card-zh.png` 曲线法建组后的中文卡片（念「方法 curves」✓ **没有** ⚠ ✓ 也没念「中性字节」✓）；`02-after-mask-painted.png` 把 dodge 蒙版涂开之后**整页**——画布上那块圆形**肉眼可见地比周围亮**（服务端读数 `128→159` ✓ 这就是 ㉜ 那条翻转的屏幕事实）；`03-curves-card-en.png` 同一张卡的英文模式（整卡翻译到位 ✓ 剩下的汉字只有**用户数据**层名「图层 1（#layer_default）」⇒ 即「需要用户决策」第 12 条那件事）。

### 本轮**只有看真截图才会去查**的东西
* `02` 那张图上顺手看见：英文模式的**图层下拉**里写着「图层 1（#layer_default）」✗ —— 那是**服务端在建文档时生成的默认层名**（不是用户输入 ✓ 也不是 `<option>` 的中文残留误报 ✓）⇒ 英文界面里它长得像漏翻 ✓ 但要在 Web 侧翻就必须**猜**服务端命名模式（脆 ✗）或在服务端引入 locale（行为变更 ✓）⇒ 这一条**第五轮已登记**（「需要用户决策」第 12 条 ✓ 三条正法都动数据模型或既有文档）⇒ 本轮**不改**。
* 同一张图上顶栏是 `head 13 / rendered 13 / dirty 0`（第五轮 ㉗ 修的那条 ✓ 这次并排看没有再打脸）。

---

## 追加（2026-10-27 第七轮）— 效果卡那句「已应用」有几分真：26 个调整里 17 个是空操作、取样器的第二个谎、以及"拒绝这句话该说给谁"

**口径**：基线 **`1231047b`**（本轮 `git pull` 后上游最新 ✓ HEAD == `origin/main` ✓ 本地零提交）。本轮新增的 2 个上游提交（`04c7165e`／`1231047b`）只动 `HANDOFF.md`、`docs/design/brush-capability-gap.md`、`docs/design/implementation-notes.md` ⇒ 与本地 **30 个脏路径零重叠** ✓ 因此**不需要 stash** ✓ 直接 `--ff-only`。
上游**尚未合并第六轮** ⇒ 本包**包含**第六轮那一节的全部成果（合并时请第五、六、七三节连起来读 ✓ 第五轮那两处仍未被上游收走的柔光亮支与中性 188 **还在补丁里**）。
工作树对 `1231047b` 的净差 = **21 个已跟踪文件 + 9 个新判据 = 30 个路径**（`git diff origin/main` ⇒ **381,632 字节** ✓ 在 `git clone --shared` 出来的干净 `1231047b` 上 `git apply` 后**逐字节复现**本工作树 ✓ 30/30 路径 `cmp` 全等 ✓ 新出的那一个已跟踪文件就是 ㊸ 改的 `scripts/criteria-known-red.txt` ✓ 而"字节数"这一项**每次改本文都会变** ⇒ 它的口径是"生成补丁那一刻的实测值"✓ 不是凭记忆抄的（㊴ 那一族 ✓ 复现命令第 15 条给了一行自测））。

### 登记 ㉝ ★★★ 「已应用 X」是界面自己下的结论 ⇒ 改成**服务端整幅合成指纹比出来的** ★★★
* 事实（本轮实测 ✗ 不是读代码猜的 ✓ 逐个建→量→删的取证脚本随包给：`logs/probe-noop.mjs`）：`/api/effects` 供 **26 个调整 + 15 个滤镜** ✓ 而**缺省参数 `{}` 下 26 个调整分成三堆**——**17 个是像素空操作**（`brightness_contrast`／`saturation`／`levels`／`exposure`／`white_balance`／`curves`／`hsl`／`color_balance`／`split_toning`／`channel_mixer`／`calibrate_primaries`／`shadows_highlights`／`selective_color`／`lens`／`dehaze`／`clarity`／`texture` ✓ 整幅合成逐字节不变）✓ **7 个默认就动画面**（`invert`／`posterize`／`vibrance`／`black_and_white`／`gradient_map`／`red_eye`／`denoise`）✓ **2 个连 `{}` 都建不出来**（`color_lut` 要 `path` 或 `size + table` ✓ `perspective` 要 `quad: [[x,y]×4]` ⇒ 服务端 `invalid_argument` 直接拒 ✓ 一个原子都不落）。**同一句「已应用 X」底下可能是"变了"也可能是"一点没动"** ✗ 用户无从分辨 ✓。
* **这条数字本身也是本轮改过来的**（值得记一条 ✓ 因为它是"名单与数字不自洽"那一类）：初稿按早先那份扫描写成「19 个空操作」✓ 落文档时把名单逐个点数 ⇒ **列出来只有 17 个** ✗ 重跑取证才看清那 2 个是**被拒**（不是建成而无效）⇒ 17 + 7 + 2 = 26 才对上 ✓ 也就是说**"建成空操作"与"根本建不成"是两件事** ✓ 混在一堆会把 ⑥ 那条"拒绝时不落原子"的证据底子弄脏 ✓（判据头注释与 `viewer.rs` 卡片注释同批更正 ✓）。
* 第二条：`add_adjustment` **不校验它不认识键** ✓ `{"redhue":50}` ⇒ `ok:true` ✓ `list_effects` 回读原样存着 ✓ 而画面一字未动（内核只对**认识**的键做逐键区间校验 ✓ `tools_effect.rs`）。越界的真键才会被拒：`{"red_hue":150}` ⇒ **HTTP 400 `invalid_argument`** ＋ 中文 `context.detail`（`red_hue 必须在 [-100, 100] 之间，得到 150` ✓ `tools_effect.rs:681`）⇒ **一根汗毛都没动**（原子数、效果数、指纹三项不变 ✓ ⑥ 钉住）。
* 现在：回执行念的是**比出来的结论**——「⇒ 整幅合成的服务端指纹变了 ⇒ 画面确实动了 · 实测 (560,360) = [200,50,50,255] → [200,143,0,255] · 这一层共 1 个效果」或「⇒ 整幅合成逐字节没变 ⇒ 这一笔对画面是空操作」✓ 三个数分别来自三个**服务端**读数：`render_region` 的顶层 `blob_hash`（实测**只读 ✓ 幂等 ✓ 不加原子** ✓）、`list_effects` 的参数回读（不是输入框原样 ✓）、`sample_colors` 的合成读数（界面一个像素都不自己算 ✓）。
* 顺带补上这张卡缺的**出口**：此前只进不出 ⇒ 空操作叠五次也撤不掉 ✓ 实测服务端 `delete_object` 认效果对象 ✓ 删掉后画面复原 ⇒ 现在每行一个「删」✓ 删完同样用服务端指纹说它到底有没有影响画面（「它原本就没参与画面」／「画面确实动了」两种说法都可能出现 ✓ 不许一律写成"删了"两个字）。
* 守卫：`scripts/browser-effect-card-truth.mjs`（**新** ✓ **38 条断言**全绿 / exit 0 / 约 100 s ✓ 其中 4 条是 ㊳ 那组「可见性」✓ 第 37 条是 ㊴ 那条「密度／去向」✓ 第 38 条是 ㊶ 那条「清单那一格也得说责任」）——它**独立**在服务端侧做一次同样的测量（自己建文档、自己 `render_region`、自己 `sample_colors`）✓ 再拿页面上那句去对 ⇒ "页面念什么"与"事实是什么"**不同源**才能判出红 ✓（变异 A1 只把结论硬编码成"动了"⇒ ③ 一条红 ✓ `mut-A1.log`）。

### 登记 ㉞ ★ 均匀网格取样 ⇒ "读数没变"可以是**取样器**的谎（第二轮同族缺陷复发到判据自己头上）★
* 事实：夹具是 1024×1024 白底 ✓ 内容只有 (700,700) 一块 40×40 的红 ✓ **用均匀网格取五点全落在白底上** ✗ 加完真参数后 `200,50,50 → 200,143,0` 这件事**只在内容里发生** ⇒ 拿均匀网格的"没变"当证据 ✓ 就是界面把谎说得漂亮 ✗（同一族缺陷在第五轮是"卡片说的与发生的对不对"✓ 这一轮复发到**判据的仪器**上）。
* 现在：`effectProbeRegion()` 的外框取自服务端 `list_objects` 的 `bbox` **并集**（界面不猜内容在哪 ✓）；效果对象的 bbox 是 `[0,0,0,0]` ⇒ **`w<=0||h<=0` 那条 `continue` 必须留着** ✗ 否则并集被拖到原点；窗口上限 512×512（整幅编码在大文档上太贵 ✓ 超限取外框正中一块并如实带说明）；文档里一个对象都没有 ⇒ 只能钉画布中心 ✓ 而那句话仍然只说"取样在哪"✗ 不说"画面没变"。
* 变异 **A2**（把那条 bbox 有效性守卫删掉）⇒ **6 条红**（①②④⑤⑨⑨ ✓ `mut-A2.log`）⇒ "取样窗口选得对不对"在判据上是**可见的** ✓ 不是靠注释守着。

### 登记 ㉟ ★★ 拒绝这句话把责任说给了错的谁：服务端在 `error_code` ✓ 本地内核在 `error` ✗ ★★
* 事实：`applyEffect`／`deleteEffect` 的失败分支原本硬写「**服务端**拒绝了这一步」✓ 而**这一笔走到哪儿由 `serverRenderPreferred()` 说了算** ✓ 勾掉「服务器渲染」⇒ 请求进的是 `api-local.js` ✓ 本地失败的码放在 **`error`** 那一格（实测 `{ok:false,error:"invalid_argument",reason:…,server:false}`）⇒ 于是卡片念「**服务端**拒绝了这一步（错误码 **unknown**）」✗ **两处都是假的**：把人送去翻服务端的日志 ✓ 而真正的码**早就在响应里** ✓ 而页面的**通用**分支（`错误 <code>：<detail>`）本来就两个键都读 ✓ 只有这张卡没读。
* **为什么"页面 == 回执"那一类断言看不见它**（本轮值得记一条方法论）：这句归属是**界面自己下的结论** ✓ 服务端回执里根本没有"服务端"这个词 ⇒ 逐字段对回执**对不出归属错** ✓ 也只能证明"码不是编的"而证明不了"码读全了"。要抓它必须拿两条**屏幕事实**去问：① 此刻路由到哪一头（读页面自己的偏好）；② 响应里那个键叫什么。
* 现在：`refusalVenue()`（与 `serverRenderPreferred()` 同源 ⇒ 说"服务端"或"本地内核"）＋ `failureCode()`（`error_code || error`）✓ 英文词条 m39 整句跟着走（「the server」／「the local kernel」二选一 ✓ 不是固定 "the server"）。另一条诚实：本地那一臂成功时回的是 **PNG 字节 ✓ 没有 JSON 回执** ⇒ `effectFingerprint` 拿不到 `blob_hash` ✗ 于是那句只许说"没法验证"（`hash:null` ⇒ `code` 也随之为 `null` ✓ 不许瞎报错误码）。
* 变异 **C**（把这两个函数还原成原先那半格）⇒ **恰好 2 条红**（⑩b「这一笔走的是本地内核，页面却把失败说成服务端拒绝」+ ⑩c「卡上的错误码与日志不同源」✓ `mut-C.log`）✓ 其余 30 条不动 ⇒ 这一族缺陷在这条判据上**只有这两条在问** ✓ 也就是"少了它们就是盲区"的正面证据。
* **本轮仍未改**（诚实登记 ✗ 不是遗漏）：`viewer-app.js:4456`／`:4482` 撤销／重做那两句**仍只读 `error_code`** ⇒ 同一族缺陷的第二个现场 ✓ 但它不在这张卡的作用面上 ✓ 改它要同步另外两条 i18n 词条与判据口径 ⇒ 记在 backlog。

### 登记 ㊱ ★★★ 本地内核把调整写成 `adjustment_name` ✓ 服务端认的是 `adjustment_type` ⇒ 这条**本轮刻意不修** ★★★
* 事实：本地内核 `p2cAddEffect` 用的键名是 **`adjustment_name`**（`api-local.js:2119` 那一行三元式 ⇒ 滤镜那一支 `filter_name` **是对的** ✗ 只有调整那一支写错 ✓ 2120 用它校验必填 ✓ 2125 用它写进 `data`）✓ 而服务端 `add_adjustment` 要求的是 `adjustment_type`（`tools.rs:12766` 那张键名表 ✓ `EffectKind::Adjustment => "adjustment_type"` 在 `tools.rs:11221`）✓ `list_effects` 回读时取的也是 `data.adjustment_type`（`tools.rs:12517`）。`grep -rn adjustment_name`（排除 `target/`）**只命中两处**：`crates/yanshi-http/assets/api-local.js` 与它的 PWA 镜像 `web/api-local.js` ⇒ **同一份源码的两个副本** ✓ 不是"两种命名并存"（顺带一条：仓库自己的 `docs/FOR-APPLE2011.md:4707` **早就写明**「参数名是 `adjustment_type`（不是 `adjustment_name`）」✗ 也就是说这条分叉**不是本轮新发现** ✓ 本轮做的是**把它变成屏幕上看得见的一句话**）。后果两条：① 本地模式下这张卡的调整**必被拒**（本地校验 `body[nameKey]` 拿不到页面按服务端契约送出的 `adjustment_type`）✓ ② 真在本地建出的那条原子**服务端读不出来** ⇒ 同一份文档切回「服务器渲染」就"效果凭空消失"（**数据形状分叉** ✓ 比"少一个端点"更贵 ✓ 因为它是静默的）。
* **为什么不顺手修**：要证明修好了 ✓ 需要一条"PWA 建原子 ⇒ 服务端读得到"的双端重放判据 ✓ 而本机 `web/mediums/*.wasm` **一个都没有**（与登记 ⑬ 同因 ✓ `tool-pwa-assets` 那条环境红）⇒ 任何断言都只能对着 mock 报绿 ✓ 而**没有判据能证明的改动不算修好**（仓库自己的纪律）。于是本轮只做**说实话**那一半：㉟ 那句「未写入：**本地内核**拒绝了这一步（错误码 `invalid_argument` …）」正是这一条的屏幕事实 ✓ 不再冒充服务端 ✓ 缺陷本身登记为「需要用户决策」**第 16 条**。

### 登记 ㊲ 「更新这个」那个按钮文案是 i18n 的漏网；那句常设说明一开始只降了亮度（⇒ 被 ㊳ 推翻）
* 编辑态：点效果行 ⇒ 按钮换字「更新这个」✓ 参数框装着**它现在的参数**（从 `list_effects` 回读 ✓ 不必从头敲 JSON）✓ 更新走 `update_adjustment` ⇒ **对象数一个不多**（⑧ 钉死）✓ 回读参数是新值 ✓ 页面念「已更新」。漏的地方是**这一行文案由 JS 写 `textContent`**（不在 `data-i18n` 节点里 ⇒ 第五轮那条 `ui-i18n-coverage` 看不见 ✓ 第六轮 ㉚ 的同族）⇒ 补词条 m40 ✓ 英文模式实测念 "Update this one"（⑨ 断言）。
* `#effectNote` 那句「指纹没变 ⇒ 这一笔对画面是空操作：缺省参数常常本就无效果，参数名拼错也会被照单收下」**文字一个没改**（它说的两件事都实测为真 ✓ 见 ㉝ ✓ 改文案就要跟着动 i18n 词条 ✗ 不值）✓ 但**它出现与否**被 ㊳ 改掉了 ✗ 本轮一开始只给它降了亮度（`opacity:.62` ✓ 以为「层级区分」就够）⇒ **那个决定被真截图推翻** ✓ 见下一条。

### 登记 ㊳ ★★ 「这句不是这一次的结论」当时并不成立：那句常设说明在三种状态里有两种都在说假话 ★★
* 怎么发现的：**就是按流程看真截图** ✓ `02`（画面确实动了）与 `10`（本地那一臂被拒）两张上 ✓ 回执明明写着「⇒ 画面确实动了 · 实测 [200,50,50,255] → [200,143,0,255]」✓ 底下紧跟着**同一张卡**的另一行「指纹没变 ⇒ 这一笔对画面是空操作」✗ 自相矛盾；而 `10` 上那句挂在「未写入：本地内核拒绝了这一步」底下 ⇒ **把「没写成」说成「写成了但没效果」** ✓ 正是这张卡本轮一直在消灭的那类谎 ✓ 而我（㊲ 的原决定）以为**降亮度**就够了 ✗ **不够：它说的仍然是一句与此刻无关的话**。
* 现在：`#effectNote` 缺省 `hidden` ✓ 由 `sayEffect(line, { noOp })` 决定挂不挂 ✓ 而 `noOp` **不是界面自己判断的**——就是 ㉝ 那两个服务端哈希比出来的结果 ⇒ **可见性与结论同源** ✓ 于是它只在「写成了而整幅合成逐字节没变」那一种情况下出现 ✓（那种情况下它补的是**为什么**——"缺省参数本就常无效果 ✓ 拼错的键也会被照单收下"✓ 回执只说结论 ✓ 不说这两条原因 ✓ 但**头半句"指纹没变 ⇒ 空操作"确实与回执重复** ✗ 这是那句文案本身的历史包袱 ✓ 本轮不改文案的决定见 ㊲ ⇒ 记一条：真要清爽，该改的是文案而那是 i18n 词条的活）。
* 守卫：判据新增 **①′③′⑥′⑦′** 四条（⓪~⑩ 之外 ✓ 断言总数 32 ⇒ **36**）✓ 在**四个状态**（动了 / 空操作 / 被拒 / 没出门）各问一次**可见性** ✓ **不问文案**（「该说的时候才说」是一条屏幕事实 ✗ 不是措辞问题 ✓）。
* 变异 **D**（把 `note.hidden = !(verdict && verdict.noOp)` 还原成「常设挂着」）⇒ **恰好 3 条红**（①′⑥′⑦′ ✓ `criteria/mut-D.log`）✓ 而 **③′ 照绿**（那个状态下它本来就该出现）⇒ 证明这四条**各问各的** ✗ 不是一条断言复制四遍 ✓ 也证明「只降亮度」那种修法在判据上**问不出来** ✓。
* 同一次截图复核顺手抓到的**另一件事（本轮未修 ✓ 只登记）**：`10` 那一路（中途勾掉「服务器渲染」⇒ 本地内核接手）出现过**一次** `RuntimeError: memory access out of bounds @ yanshi_wasm.wasm.wasmkernel_set_viewport` ✓ 而服务端那一臂全程 0 错 ✗ 三条读数都不指向服务端。去定位时排掉两条假设：① 纯 Rust 的 `Kernel::set_viewport` 只是存一个 `Bbox`（`kernel.rs:315` ✓ 不碰内存）⇒ 崩点只可能在 glue／句柄生命周期 ✗ 不在页面传参；② 两条对照探针各跑一次（`render=client` 直接开页 ✓ **带**一个 `calibrate_primaries` 对象 vs **不带** ✓ `logs/probe-viewport-trap2.mjs`）⇒ **两臂都 0 错** ✓ 第三次重跑截图脚本也是 0 错 ⇒ **偶发（1/N）✓ 未定位** ✓ 进 backlog（与既有的 wasm `load_atoms_json` `unreachable` 同族 ✗ 那条同样只在本地那一臂出现 ✓ 都需要 wasm 侧句柄／生命周期的取证 ✓ 不是页面能改的）。**登记而不修**的理由与 ㊱ 同：没有能稳定复现的判据 ⇒ 改了也无法证明 ✓ 报「修好了」就是假绿 ✓。

### 登记 ㊴ ★★ 那条"复跑全绿"是我**抄上一轮的记录**抄来的 ⇒ 重跑才看见 `ui-text-density` 是红的（本轮自己加的那句话把它顶红了）★★
* 怎么暴露的：㊳ 那一步做完后 ✓ 我照流程在文档里写下「复跑全绿：…／`ui-text-density`／…」✓ 但**这一条当时并没有在这份树上实跑过**——那句常设说明的文案是本轮**稍早**加进 `viewer.rs` 的 ✓ 而 `ui-text-density` 的规矩是「非 `<script>` 的文本节点 ✓ 汉字 ≥24 **且**含 `；` 或 `（` ⇒ 红」✓ 那句话两条都撞（35 个汉字 ✓ 带 `；` 和 `（）`）⇒ 实跑退出码 **1**。⇒ 也就是说**"判据读数"一旦是抄来的 ✓ 它就不再是读数** ✓ 与第五轮 ㉑／第六轮 ㉛ 那一族（仪器自己说自己在跑）同源 ✗ 这次红在**文档**上而不是代码上 ✓ 修法一样：**重跑**而不是相信记录 ✓（本轮此后所有「全绿」都改了当场实跑 ✓ 原始输出随包给）。
* 修法用的是**判据自己 prescribed 的那一条**（「进 `title=` 或只在 MCP 侧说清」）✓ 不是我自己另发明一套：屏幕上只留一句短的「这一笔对画面是空操作 ⇒ 原因见鼠标提示」（17 个汉字 ✓ 无 `；` ✓ 合格）✓ 那两条"为什么"挪进 `#effectNote` 的 `title=` ⇒ **搬家不是删掉**（㊲ 那个"不改一个字"的决定在**内容**层面仍然成立 ✓ 变的是它在屏幕上的**位置**）。
* **一本字典不够**（第六轮 ㉚ 那条规则的直接推论 ✓ 本轮踩了一次）：`title` 与文本节点走的是**不同的表**——`I18N_EN_TEXT` 管文本节点 ✓ `I18N_EN_ATTR`（`viewer-app.js:13673`）管 `title`/`placeholder`/`aria-label`/`alt` ✓ 我先把那句长说明塞进 `I18N_EN_TEXT` ⇒ `ui-i18n-coverage` 当场红「属性漏词条：viewer.rs:507」✓ 挪进 `I18N_EN_ATTR` 才绿（那条判据第六轮就加上了**属性覆盖**✓ 还会自查"至少真提到 20 条属性字符串"✓ 否则退出码 2 ⇒ 它这次抓得正是 ✓）。⇒ 短的那句与 `title` 那长的**两个键都要词条** ✗ 少一个就是英文模式下的中文残留。
* 守卫：判据新增 **③″**（断言总数 36 ⇒ **37**）✓ 它**不问文案对不对** ✓ 只问两件屏幕事实：**可见字数是否已降到密度合格**（汉字 <24）＋**"为什么"是否还在**（`title` 里仍含"内核只校验它认识的键"或英文那半句）⇒ 这样"为了过判据把原因删掉"和"界面又变回大段解释"两种走偏都会红 ✓（`checkNoteWhy()` ✓ 读数带 `noteText`/`cjk`/`title` 三项 ✓）。
* 变异 **E**（把 `#effectNote` 的 `title=` 整个摘掉 ✓ 文案一字不动）⇒ **恰好 1 条红**（③″ ✓ `criteria/mut-E.log` ✓ 其余 36 条绿）而 **③′ 照绿** ⇒ 证明 ③′（可见性）与 ③″（密度／去向）**各问各的** ✓ 也就证明 ③″ 不是又一条"看着像测试的注释" ✓。

### 登记 ㊵ ★ 环境那一侧的真相：`/tmp` 是 **tmpfs + `usrquota`** ⇒ 我这个 uid 的写额度被打满时 ✓ 缺陷会**伪装成产品红**（`resource_exhausted`）也**伪装成判据坏**（`Page.navigate` 永不回包）★
* 症状一（被拍进截图）：`04-zh-noop-typo` 那一张上回执是 `resource_exhausted`「并发连接数已达上限」✓ 看上去像**产品的并发缺陷**（`server.rs:533-541` 那条拒绝路径 ✓ `max_connections = 64`）⇒ 实际根因是**服务端往 `/tmp/yanshi-root` 写 blob 时 `Disk quota exceeded (os error 122)`** ✓ 错误码从 IO 失败**升格**成了"并发已达上限"那一句 ✓（⇒ 这是**产品可以更好**的一处：拒绝原因把配额／磁盘问题说成连接数 ✓ 但本轮不改 ✓ 因为它是**环境问题**触发的 ✓ 而改它要动 `server.rs` 的错误映射 ✓ 记进 backlog）。
* 症状二（被判据当成自己坏）：`browser-effect-card-truth` 从绿（08:37 ✓ 38 行 ✓ exit 0）**退化**成稳定 exit 3 ✓ 卡死在**第一个 `Page.navigate`**（用 `judge-debug.mjs` 加"每次 send 包超时"才定位到 ✓ `probe-cdp-hang.mjs` 那两份探针随包给）。排掉的假设：① 服务端没坏——同一个 URL 用 `curl` **3 ms** 返回 200 ✓ 1,017,964 字节 ✓ `/health` 显示 `connections 4`／`documents 13`；② 不是标签页泄漏——关掉 7 个残留 tab 后照旧；③ 不是 SW 注册本身——`probe-sw.mjs` 反注册 + 清缓存后 ✓ 页面一加载又被 `viewer-app.js:11530` 注册回来 ✓ 而第二次导航**会提交** ✓ 只有**第一次**不提交（`probe-sw-stall.mjs`：首航 20 s 不提交 ✓ `caches.open` 计时 1 ms）。
* **定位（已证实 ✓ 不是推测）**：CDP 那个 profile 目录 `/tmp/cdp-profile-i18n` **已经不存在了**（`ls` ⇒ No such file or directory）✓ 而 chromium 进程（pid 935776）**还活着** ✓ `/proc/<pid>/fd` 里只剩 5 个 `.org.chromium.Chromium.*` 以"已删除"状态被持着 ⇒ 浏览器的 Cache Storage **无处可写** ⇒ SW 那一次导航的缓存写永不完成 ⇒ **第一个 `Page.navigate` 永不回包**。换一个新 profile（`/home/polar/cdp-profile-9490` ✓ **落磁盘**）重启浏览器 ⇒ 判据**当场恢复全绿**（exit 0 ✓ 39 行 ✓ 38 条断言）⇒ **产品侧无缺陷** ✓ ㊵ 这一整条都是**仪器侧**的。
* **更正我先前在这条里写下的推测**（记下来 ✓ 因为它错得典型）：我一度把"额度打满 ⇒ SW 的 `cache.put` 反压 ⇒ 导航挂住"当成"立住的解释"✗ 而**实测顺序把它否掉了**：先把 `/tmp` 的写额度清出来（`dd bs=1M count=250` 从报错变成整块成功 ✓ 服务端 `/health` 正常 ✓ `curl` 同一个 URL 3 ms 返回 1,017,964 字节）⇒ **判据照旧卡死**（`judge-rerun-1.log` ✓ 仍然 exit 3 ✓ 仍然停在第一个 `Page.navigate`）✓ 之后换 profile 才绿 ⇒ 所以额度是**另一件真事**（它确实造成了截图 `04` 那个 `resource_exhausted` ✓ 症状一）✗ 但**不是**导航挂住的原因。⇒ 那条「SW 缓存写失败不该让导航挂住」现在**降级为未证实的候选**（既没被证明 ✗ 也没被否证 ✓ 要证它得**故意**让缓存写失败再看导航提不提交 ⇒ 第八轮 ✓ 不能拿这次的环境事故倒推）。
* 教训一条（与第五轮 ㉑／第六轮 ㉛／本轮 ㊴ 是同一族 ✓ 只是这次红在**我读日志的方式**上）：**两个坑同时在场时 ✓ 修掉一个之后剩下的症状必须重测**再归因 ✗ 我差点把"清完额度还红"解释成"产品有 bug"✓ 又差点把"清完额度就绿"（**其实没绿**）写进文档 ✓ 两份日志（`logs/judge-rerun-1.log` 红 ✓ `logs/judge-rerun-2.log` 绿）都随包给 ⇒ 顺序可查 ✓ 谁先谁后不必信我的叙述。
* 修法随包给：`logs/start-cdp.sh`（profile 落磁盘 ✓ 每次先确认目录存在 ✓ 已在跑就不起第二个 ✓ 起来后**自报 page 标签数**⇒ 零标签就是下一次卡死的预兆 ✓ 本轮起固定先跑它）。
* 自查清单（本轮起固定执行 ✓ 别再花一轮重新猜）：`browser-*` 判据 exit 3 ⇒ **先查仪器**：① `ls` 那个 `--user-data-dir` 还在不在（**本轮真因**✓ 目录没了而进程还活着 ⇒ 症状就是"第一个导航永不回包"）✓ ② `curl /json/version` 通不通 ✓ ③ 数 `page` 标签（0 个 ⇒ 下次导航必卡 ✓ 这是本仓库既有的坑）✓ ④ 才轮到 `dd bs=1M count=250` 测**真实写额度**（`df` 的 `Avail` 是整个挂载点的 ✓ **不是**我这个 uid 的额度 ⇒ 骗人）✓ ⑤ 最后才怀疑产品；截图里出现 `resource_exhausted` ⇒ 先查 `/tmp/yanshi-serve-15900.log` 有没有 `os error 122` ✓ 别先接受"并发上限"那句字面。

### 登记 ㊶ ★★ 第二次看真截图又抓到两件事：同一张卡上"一句说责任、一句只念裸码"，以及相邻两行逐字重复同一句结论 ★★
* **⑩d 那一族（真缺陷 ✓ 本轮已修）**：`10`（本地那一臂）那张上 ✓ 回执行已经说实话了（「未写入：**本地内核**拒绝了这一步」✓ 那是 ㉟/⑩b 的成果）✓ 可**紧接底下那一格**（`#effectsList`）念的是「读取失败：endpoint_not_local」⇒ **一个裸码** ✗ 同一张卡上同一件事 ✓ 一句说责任一句不说 ⇒ 读的人只能猜是"服务端读不到我的效果"还是"本机没有这个端点"（后者才是事实 ✓ 登记 ⑬/㉟）✓ 而这一格在 ⑩b 那一轮**根本没被问**（那条断言读的是 `#effectInfo` ✓ 与 ㊳ 的教训一模一样：**"旁边那一格"不在被问的集合里 ✓ 判据就看不见** ✓）。
* 修法**不另发明第二套**：那一句改走 ㉟ 已经建好的 `refusalVenue()` + `failureCode()` 一对助手 ⇒ 「读取失败（本地内核）：endpoint_not_local」✓ 责任与码同源 ✓ 英文那侧**必须补一条模式词条**（句式变了 ⇒ m19 那条 `^读取失败：(\S+)$` 的 `$` 锚匹配不到 ✗ 漏了就是英文模式留中文 ✓ 新增 **m40** ✓ venue 两个词的映射与 m39 逐字同一本 ✗ 不复制第二份）。
* 守卫：**⑩d**（断言总数 37 ⇒ **38**）✓ 三种屏幕状态各有说法 ⇒ **不留"什么都没发生也算通过"的空档**（第六轮 ㉛ 那条同族规矩）：那句是「读取失败…」⇒ 必须同时带 venue 与具体码 ✓ 否则产品红；那一格若**念出了效果清单**（= 本地内核答得出 `list_effects` ⇒ 与「本地没有这个端点」矛盾）或**空着** ⇒ 那是**判据前提变了** ⇒ 按本文件口径 `process.exit(3)` ✓ 既不判成产品红也不判成绿 ✓。
* 变异 **F**（把那一句还原成裸码版 ✓ 只读 `error_code`）⇒ **恰好 1 条红**（⑩d ✓ `criteria/mut-F.log` ✓ 其余 37 条绿）而 **⑩b／⑩c 照绿** ⇒ 证明"同一族的第三格"是**独立**在被问的 ✗ 不是一条断言复制三遍 ✓ 也说明 ㉟ 那两条修的是**回执那一格**、够不到清单这一格 ✓。
* **顺手那件小的（文案 ✓ 不算缺陷）**：同一张 `04` 上 ✓ 回执那一行末尾说「… ⇒ 这一笔对画面是空操作」✓ 底下那句说明的**头半句又是一模一样的「这一笔对画面是空操作」**⇒ 相邻两行逐字重复一遍 ✓ 判据不会红（它只问可见性与密度 ✗ 不问措辞）⇒ 既然那句现在只负责"指一下鼠标提示" ✓ 就把它缩成「为什么？见鼠标提示」（英文 "Why? See the hover hint" ✓ 文本词条同批改 ✓ `title=` 那条长的不动 ⇒ ③″ 两个条件都照旧成立 ✓）。**代价记一条**：改文案就要跟着改 `I18N_EN_TEXT` 的键 ✓ 这正是 ㊲ 当初"不改一个字"那个决定想避开的东西 ✗ 但 ㊴ 的密度修法已经把那句话改过一次了 ⇒ 再改一次的成本已经付过 ✓ 不是新账。
* **同族但本轮不动**（诚实划一条界 ✗ 免得下轮以为漏了）：`get_log` 那一格（历史卡 ✓ `viewer-app.js:5036`）用的还是**旧形状**「读取失败：<code>」⇒ 同一个缺陷在另一张卡上仍在 ✓ 但本轮**没有任何屏幕证据**说它在本地那一臂会真的走到那条分支 ✗ 也没有判据问它 ⇒ 按"改了也无法证明就不谎称修好"（与 ㊳/㉟ 同一条规矩）留作第八轮：先给历史卡补一条同样的断言 ✓ 再改那一句。

### 登记 ㊷ ★ 收尾那趟回归自己抓出一条**老漏翻**：历史卡的空态「（没有匹配的原子）」在英文模式下是中文 ✓ 而静态那条判据**永远看不见它** ★
* 怎么抓到的：就是「收尾把整套判据在**最终构建**上重跑一遍」这一步（本轮第三次跑红 ✓ 前两次都红在仪器上 ✗ 这一次红在**产品**上）⇒ `browser-i18n` 报 `distinct:1, where:["<div#history>"]` ✓ 英文模式下整页只剩那一句中文 ✓。
* 根因一句话：那句是**运行时**赋值的 `textContent`（`viewer-app.js:5143` ✓ "筛选后一条原子都不剩"那个分支）⇒ **不在 `viewer.rs` 模板里** ⇒ `ui-i18n-coverage` 按口径**看不见**（它只扫模板的文本节点与属性 ✓ 自己那行说明就写着"不看 viewer-app.js 运行时拼的文案"✓ 与 ㉚／㊲ 完全同族 ✓）✓ 而 `browser-i18n` 要**走到那个状态**才问得到 ⇒ 本轮之前从没走到过 ⇒ **不是新代码引入的退化 ✓ 是新走到的状态把老洞照出来**（这一点必须写清 ✗ 否则下轮会去 diff 本轮的改动找"是谁弄坏的"✓ 找不到 ✓ 因为没人弄坏 ✓ 洞一直在）。
* 修法：补一条 `I18N_EN_TEXT` 词条「（没有匹配的原子）」⇒ "(no matching atoms)"（整句固定 ✓ 不拼数据 ⇒ 走文本词典而不是 m-模式词条 ✓ 与 ㊲ 那颗「更新这个」同一条路）。
* **顺手量了一条规模**（不修 ✓ 只登记 ✗ 因为"哪些算缺陷"要先问清落点）：用一段临时脚本扫 `viewer-app.js` 里**整句赋值**的中文 `textContent`（67 处 ✓ 有词条 45 处 ✓ **无词条 22 种**）⇒ 名单如 `L3024 宽度与高度都要是 1..8192 之间的整数`／`L3035 请输入名称`／`L3050 已存在同名文档…`（这三条看着像**校验提示**⇒ 可能走 `setCustomValidity` 那类**不受文本节点 i18n 管**的通道 ✓ 补词条也没用）✓ `L5312 拒绝`／`L5355 回到这里`／`L6079 接受`（短标签 ✓ 大概率和 ㊲ 一样是按钮切字 ✓ 补一条就够）✓ `L2893/L2925/L2935`（内核对照那一族 ✓ 可能只落 `#log` ⇒ **按设计豁免**✓ 见 ㉛ 的 `data-i18n=off`）。⇒ 也就是说"22"这个数**不能直接当缺陷数**用 ✗ 要先逐条问落点 ⇒ 第八轮的活：**新写一条静态判据**（枚举运行时中文 `textContent` ⇒ 查词条 ⇒ 带一份**写明理由的豁免名单**（`#log`／`setCustomValidity`／只进 `title=` 的）✓ 并且**自查覆盖面**（照 ㉛ 那条规矩：真提到几条必须报出来 ✓ 提到 0 条也算通过的那种绿不要再写）⇒ 不然这 22 种里下一次又会"走到才抓到"）。
* 本轮**只修被实跑到的那一条**（`browser-i18n` 抓的这条 ✓ 修完它照绿 ⇒ 证明修的就是它问的那件事 ✓），其余 21 种**不顺手改**：没有判据问它们 ⇒ 改了也无法证明 ✗ 与 ㊳/ 那条"改了也无法证明就不谎称修好"是同一条规矩 ✓（另：本轮同批还看到 `browser-layer-lock` 又红一次 wasm `wasmkernel_set_viewport` memory access out of bounds ✓ 与 ㊳ 末登记的是**同一条偶发**✓ 本轮累计 2 次命中 ⇒ 只加一条计数 ✓ 不改结论）。

### 登记 ㊸ ★ 名单里一条**陈旧的已知红**：判据早就绿了而豁免还挂着 ⇒ 它会藏住将来的真红 ★
* 怎么撞上的：就是上面那条 exit 3 的复核 ✓ 我把 `browser-reference-overlay` 按 CI 的形状跑对之后它**绿了** ⇒ 而 `scripts/criteria-known-red.txt` 仍把它列成"已知红 ⇒ 不阻塞"✓ 理由写的是「**夹具用占位哈希 `sha256:aaaa…`** ⇒ 被存在性校验拒 ⇒ 每次停在 exit 3」（第 46 轮的读数）。
* 事实核对：那条理由**已经不再成立** ✗ —— 判据的夹具**自己就改了**：先用**只读**的 `render_region` 把当前文档渲成一张**真 blob** ✓ 再交给 `set_reference`（`scripts/browser-reference-overlay.mjs:28、54-61` ✓ 那几行注释还留着「原来这里写死一个假 hash」的自白）⇒ 也就是**当年写下的"转绿条件 ②"后来正是照它修的** ✓。
* 复验（不是"跑过一次绿"就下结论）：`127.0.0.1:15900` + CDP 9490 ✓ 服务端渲染臂 ✓ **320×240 一轮 + 640×400 两轮** ⇒ **三次一致 exit 0** ✓ 逐条印「设了参考图之后 `{"found":true,…,"opacity":0.5}`／清掉之后 `{"found":false}`／✓」✓ 原始输出随包给（`regression/browser-reference-overlay*.txt`）。
* **为什么必须处理**（不是打扫干净好看 ✗）：豁免名单的作用是让 CI **容忍**这些条目 ⇒ **陈旧条目 = 那条判据将来真回归时也会被当"已知红"放过去** ✓ 这个风险不是我这轮发明的：同一个仓 `run-criteria.sh:559-563` 早就写下同一认识（「名单 32 条而 CI 实测只有 20 条真失败 ⇒ 12 条陈旧」）✓ 并**每次运行都印**「名单里有、但本片没失败（可能已修好 ⇒ 考虑移除）」的候选 ✓ 只是**候选印了没人处理** ✗ 我这轮正好撞上其中一条 ⇒ 处理掉。
* 处置：把 `browser-reference-overlay.mjs` 那一行**从名单移出**（改成 `#` 注释行 ⇒ `is_known_red()` 用的是 `grep -qE "^$1[[:space:]]"` ✓ 只认行首名字 ⇒ 注释掉等于移出而**文字一字不删** ✓ 后来的读得到缘由）✓ 并在原位补上"为什么移 + 复验数据 + 我自己那次调用错误"的注释块 ✓ **同批的 `tool-example-acceptance.mjs` 不移**：它的红**另有原因**（`set_reference` 的**示例**是全零占位 ⇒ 静态不可满足 ✓ 名单里紧邻那段已写明）✓ 我没复验过它 ⇒ 按"没证据不动"留着。
* **一条要传给下一轮的教训**：exit 3 与"产品红"是两种东西 ✓ 但**都可能是"仪器/调用方的问题"** ✗ ⇒ 先回去读**判据自己的用法行**（`if (!url) 用法: … <viewer-url> [cdpPort]`）✓ 再决定它是红还是我没喂对 ✓ 我这次差一点把"我没传 token"抄成"它还在已知红里"⇒ 那样这条陈旧豁免就能一直活到下一次真回归。

### 本轮判据读数（`1231047b` + 本补丁 ✓ 构建新鲜度按 mtime 判 ✓）
* **新增 1 条**：`browser-effect-card-truth` ⇒ **38 条断言全绿 / exit 0 / 约 100 s**（⓪~⑩ 各钉一件事 ✓ 加 ①′③′⑥′⑦′ 那组「可见性」✓ 加 ㊴ 的 ③″「密度／去向」✓ 加 ㊶ 的 ⑩d「清单那一格也得说责任」✓）。**日志行数比断言数多一行**（39 行 ✓ 多的是收尾那句总结 ✓ 不是第 39 条断言）⇒ 各份变异日志按「红 + 绿 = 当轮断言数」点数时 ✓ 失败那一轮不打印收尾那句 ⇒ 所以那些日志里 红+绿 恰好等于断言数 ✓ 两边口径一致。它已登进 `scripts/run-criteria.sh` 给 `browser-*` 的例外名单（原因写在名单旁的注释里 ✓ 它自己两条路都走：`render=server` 与本地那一臂 ⇒ 不依赖 CI 的缺省形状 ✓ 因此不需要 CI 改导航）。
* 复跑全绿：`check-criteria-have-exit3`（**259** 个用 `fetch(` 的判据**零**缺 `process.exit(3)` ✓ 含本轮新那条）／`check-viewer-unique-ids`／`ui-i18n-coverage`／`ui-text-density`／`browser-tooltip-i18n`／`browser-status-bar`／`browser-dodge-burn`／`browser-fill-region`／`browser-reference-decode`／`browser-blend-modes`／`browser-frequency-separation`／`tool-dodge-burn-effective`／`tool-fill-region-opacity` ⇒ **exit 0** ✓（原始输出在 `criteria/` 与 `regression/` ✓）。
  · **但这一句最初是我抄上去的** ✗ ⇒ 收尾那趟**在同一份最终构建上当场重跑**才把它拆开 ✓ 四条各自的处理记在这里（✗ 不许再把非绿混进"全部 exit 0"）：
  · **`browser-i18n` ⇒ 先前红过一次 ⇒ 是真产品缺陷** ✓ 见 ㊷（英文模式下历史卡那句「（没有匹配的原子）」是运行时 `textContent` ✓ 静态那本字典看不见）⇒ 补一条 `I18N_EN_TEXT` 词条后**当场复跑** ⇒ exit 0 且**逐页 distinct 0** ✓（这条"先红后绿"才是"修的就是它问的事"的证明 ✓ 光看最终绿不算）。
  · **`browser-layer-lock` ⇒ exit 3** ✓ 不是产品红而是**判据前提空转**：那一趟本地内核没把文档装进来（12 个原子在本地是 0）⇒ 「幻影」那三条断言成了 0⇒0 的空判 ⇒ 它按约定自判 exit 3 ✓。**同一份二进制、相隔一分钟再跑 ⇒ 全绿 exit 0** ✓ ⇒ **间歇性**这件事由此**第一次拿到"同构建、背靠背"的对照证据**（先前只有"跨轮偶发"）✓ 触发点是 ㊳ 记过的那个未定位 wasm `wasmkernel_set_viewport` memory access out of bounds ✓ **第三次命中** ⇒ 仍**只登记不谎称修好**（第八轮候选：给那条 wasm 入口加有界重试 + 抓最小复现）。
  · **`browser-reference-overlay` ⇒ 第一次 exit 3 的原因是"我调用方式错了"** ✗：我只传了 `http://127.0.0.1:15900` ✓ 而它要的是 `<viewer-url>` 且 URL 里带 `doc`/`token`（`:26、34`）⇒ 被自己的前提校验拒掉（`capability token 必须是 64 位小写十六进制`）✓ 换成 CI 的形状（`$BASE/?doc=…&token=…`）后**两个文档尺寸、三次一致 exit 0** ⇒ 由此查出**名单里那条陈旧登记** ✓ 见 ㊸。
  · 跑 `browser-i18n` 时要 `export CDP_PORT=9490`：那个脚本**只读环境变量**✓ 不接 argv[3] ✗（既有参数约定不一致 ✓ 记在 backlog ✓ 不是本轮引入）。
* Rust 侧本轮**零改动**（改动全在 `viewer-app.js`／`viewer.rs`／`run-criteria.sh`／`criteria-known-red.txt`／本文 ✓ 其余 rust 文件是第四～六轮的成果随包走）：`cargo fmt --check` ⇒ **0** ✓ `cargo clippy -j 2 -p yanshi-http -p yanshi-server -p yanshi-render --all-targets -- -D warnings` ⇒ **0** ✓（坑记一条：`-j 2` 必须放在 **`cargo` 一级**✓ 放进 `-- -D warnings` 之后会被 rustc 当选项 ⇒ `error: Unrecognized option: 'j'` ✓ 并连带报出无关 crate 的"could not compile"✓ 本轮踩过）。
* **`bash scripts/pre-commit.sh` 我"没有整条跑"——如实拆开**（㊴ 那条规矩：读数不许抄 ✓ 所以逐条列实际跑过什么）：
  · ① `cargo fmt --all -- --check` ⇒ **0** ✓ ｜ ③ `check-ledger-section-numbering` ⇒ **0**（棘轮 0／实测 0 ✓ 悬空引用 0）✓ ｜ ④ 四条 `--self-test`（`check-tests-clean-temp`／`check-tests-have-asserts`／`check-release-workflow-gh-repo`／`check-blank-overwrite-guard`）⇒ **全 ok** ✓ ｜ ⑤ `check-release-workflow-gh-repo` 真跑 ⇒ **0** ✓
  · 两个**条件门**这轮**都触发了**（`git diff --name-only HEAD` 里同时有 `scripts/run-criteria.sh` 与 `crates/yanshi-server/src/tools_registry.rs`）⇒ 照脚本原样跑：`cargo test -j 1 -p yanshi-server --test shell_scripts --test script_portability` ⇒ **3 + 1 passed / 0 failed** ✓ ｜ `--lib every_registered_tool_has_a_documented_example` ⇒ **1 passed**（227 filtered）✓ ｜ `--test tool_inventory` ⇒ **2 passed** ✓
  · **② `cargo clippy --workspace --all-targets` ⇒ 我换成了"只跑我碰过的三个 crate"** ✗ 依据是**这台机器的两条硬约束**而不是"懒得等"：`free` 显示 **11 G 总 / 3 G 可用**（本场**被 OOM kill 过一次** ✓ 用户明确要求注意内存）＋ `df -h /` 显示 **28 G 已用 / 只剩 2.1 G（94%）** ✓ 而 `--workspace --all-targets` 走的是 **debug** 档 ✓ `target/debug` 已经是 **14 G** ⇒ 再往里写产物有**把根盘写满**的风险（那会连带炸掉所有判据 ✓ 不只是这一条红）。
  · **∴ 因此**：这道门**在 CI 上仍然要整条过** ✓ 我这边是"三个受影响 crate 的 release clippy 干净 + 其余各条干净"✓ 不是"pre-commit 全绿"✗ 这两个说法**不许混用**（本轮之前我写过一句 `bash scripts/pre-commit.sh ⇒ 可以提交` ✓ 那是在**没有整条跑**的情况下写的 ⇒ 已删 ✓ 与 ㊴ 同一件事的第二次）。
  · 顺带一条**给下一轮或 CI 维护者的观察**（不改 ✗ 只登记）：`pre-commit.sh:23` 那条 `--workspace --all-targets` **没有 `-j`** ✓ 而它被设计成"不可绕过"的提交前门 ⇒ **在小内存／小磁盘的机器上它自己就是那次 OOM 的来源** ✓ 本仓 `AGENTS.md` 另有一条"build 后清 `target/debug/{deps,build,incremental}`"（脚本 `:108-119` 也提醒了 ✓ 实测 `target/debug` 14 G ⇒ 提醒已触发）⇒ 两件事合起来是"**门是对的但没有按机器的预算调过**"✓ 要不要在脚本里加 `CARGO_BUILD_JOBS`／改跑 scoped clippy ⇒ **属于改别人的门 ⇒ 记进「需要用户决策」**。
* 唯一红：`tool-pwa-assets` ⇒ 仍是**环境前提**那一条（本机 `web/mediums/*.wasm` 一个都没有 ✓ 与登记 ⑬／㊱ 同因 ✓ 由 CI 收敛）✓ 而**防分叉那一行是绿的**（改完 `viewer-app.js` 后按流程跑了 `pwa-sync-viewer.mjs` 并 `git checkout -- web/index.html` ✓）。
* **Web 调用面**（同一口径：`viewer-app.js` 里 `callTool("…")` 字面命中的不同工具名 ✓ 命令见「复现命令」第 11 条）：基线 **`1231047b` = 87** ⇒ 本包 **= 90**（净增仍是 `fill_region`／`sample_colors`／`dodge_burn_setup` 三个 ✓ `comm -13` 实测 ✓ **第七轮零净增**——本轮全是把已有端点的话说实 ✓ 新用到的 `render_region`／`list_effects`／`list_objects` 早就在集合里）。

### 变异七条（各编译一次 ✓ 全部已还原 ✓ 还原后重验「MUTATION 残留（页面）」= **0**）
| 变异 | 改的是哪一处 | 红了什么 | 结论 |
| --- | --- | --- | --- |
| **A1** | 把回执行的结论硬编码成"动了" | **1 条**（③ 拼错参数名那句） | "页面念的是比出来的 ✗ 不是写死的"这条**有人问** |
| **A2** | 删掉 `effectProbeRegion` 的 bbox 有效性守卫 | **6 条**（①②④⑤⑨⑨） | 取样窗口错 ⇒ 整族的"实测读数"都失去意义 ✓ 会被抓住 |
| **B** | `deleteEffect` 里把回执当成功（`{ok:true,server:false}`） | **5 条**（④ + 「删完服务端还留着效果对象」+ ⑤清场 + ⑨删除句 + 收尾基线） | 删除出口不兑现 ⇒ 画面回不到基线 ✓ 会被抓住 |
| **C** | 还原 `refusalVenue()`／`failureCode()` | **恰好 2 条**（⑩b、⑩c） | 归属与错误码这一族**只有这两条在问**（同批其余 30 条看不见） |
| **D** | 把 `#effectNote` 恢复成**常设挂着**（`note.hidden = false`） | **恰好 3 条**（①′⑥′⑦′ ✓ 而 ③′ 照绿） | 「该说的时候才说」是**屏幕事实** ✓ 只降亮度那种修法问不出来 |
| **E** | 把 `#effectNote` 的 `title=` 整个摘掉（可见文案一字不动） | **恰好 1 条**（③″ ✓ 而 ③′ 照绿） | 「原因只是搬家、不是被删掉」与「屏幕上不许再变回大段解释」这两件事**有人在问** ✓ 不是靠注释守着 |
| **F** | 把 `#effectsList` 那句读取失败还原成**裸码**（不报 venue ✓ 只读 `error_code`） | **恰好 1 条**（⑩d ✓ 而 ⑩b／c 照绿） | ㉟ 那一族修的是**回执那一格** ✗ 够不到**清单那一格** ✓ 第三格是独立在被问的 |
* **各条变异跑的是哪一版断言集**（诚实记一条 ✓ 因为数字对不上会误导）：**A1/A2/B/C 跑在 ㊳ 之前那 32 条**（31+1 ✓ 26+6 ✓ 27+5 ✓ 30+2 ✓ 四份日志的红绿相加**都恰好等于 32** ⇒ 它们问的那几条断言在 ㊳ 之后**一字未动** ✓ 结论照旧成立）✓ **D 跑在 ㊳ 之后那 36 条**（33+3）✓ **E 跑在 ㊴ 之后那 37 条**（36+1）✓ **F 跑在 ㊶ 之后那 38 条**（37+1 ⇒ F 是最后一条 ✓ 它是在**最终形态**上证的 ✓ 而 E 证的 ③″ 与 D 证的那三条在 ㊶ 之后**一字未动** ✓ 结论照旧成立）。最后一版判据（38 条 ✓ 无任何变异）另跑一次全绿 ⇒ `criteria/browser-effect-card-truth.txt`。
原始日志：`criteria/mut-A1.log`／`mut-A2.log`／`mut-B.log`／`mut-C.log`／`mut-D.log`／`mut-E.log`／`mut-F.log` ✓ 驱动脚本随包给（`logs/mutate.py` ✓ 锚点写死 ✓ `finally` 里必还原 ✓ 每条变异都要**重编译 + 先杀旧进程再起 + 重跑判据** ✓ 还原后**再重建一次**再看页面「MUTATION 残留」= 0 ✓）。⇒ 驱动脚本本轮也被改过一处**流程缺陷**（记一条 ✓ 因为这类"备份"最容易在下一轮咬人）：原先是"`BAK` 不存在才写备份"✓ 而 D 之后我又改了 `viewer-app.js` ⇒ 那份 `BAK` 已**过时** ✗ 用它还原会把新改动一起抹掉 ✓ 现在改成**每次运行先快照当前干净文件**（`snapshot/<key>.<ext>`）✓ `finally` 从快照还原并**逐字节核对**（报"已恢复原状 ✓ 字节一致"）✓ 快照目录随包给 ✓ 替代旧的 `viewer-app.js.bak`。

### 真截图（`screenshots/effect-card/01…10.png` ✓ 1680×1050 ✓ `deviceScaleFactor:2` ✓ 直接从 CDP 抓 ✓ 每张都实际看过）
`01-zh-card-idle`（卡片展开的缺省态：参数框 `{}` ✓ 列表「(当前文档没有调整/滤镜)」✓ **㊳ 之后那句常设说明这一态不挂** ✓）；`02-zh-moved`（真参数后的回执行：「画面确实动了」＋实测 `[200,50,50,255] → [200,143,0,255]` ✓ **那张上再没有"指纹没变 ⇒ 空操作"那一行** ✗ 这就是 ㊳ 的前后对照）；`03-zh-fullpage-canvas`（**整页**：那块方形在画布上**肉眼可见地变成橙色** ⇒ ㉝ 那句结论有屏幕事实兜着 ✓ 不是哈希自证）；`04-zh-noop-typo`（`{"redhue":50}` ⇒ 回执那一行「整幅合成逐字节没变 ⇒ 这一笔对画面是空操作 · 这一层共 2 个效果」而参数照样被收下并念回来 ✓ **这一态那句说明才挂着** ✓ 且屏幕上实际是**改短后的那一句「为什么？见鼠标提示」**（暗色 ✓ 长原因在 `title=` 里 ⇒ 这就是 ㊴ 的密度修法与 ㊶ 的"相邻两行不再逐字重复"落在屏幕上的样子 ✓ 这张是**改完之后重拍的** ⇒ 不是拿文案描述冒充像素））；`05-zh-refused`（越界真参数被拒：「未写入：服务端拒绝了这一步（错误码 invalid_argument …）」）；`06-zh-editing-row`（编辑态：按钮换字「更新这个」＋参数框装着现有参数）；`07-zh-after-delete`（删完那一句与"这一层还剩 0 个效果"）；`08-en-moved`／`09-en-refused`（英文模式整句翻译 ✓ 数字仍是服务端那几个数 ✓ 无汉字残留）；`10-zh-local-arm`（★ 关掉「服务器渲染」后那句「未写入：**本地内核**拒绝了这一步（错误码 `invalid_argument` · 完整原因见下方日志）。拒绝时不落原子 ⇒ 画面不会动。」⇒ 这就是 ㉟／㊱ 的屏幕事实 ✓ **同张底下那一格现在是「读取失败（本地内核）：endpoint_not_local」**（㊶ 那条 ⑩d 修的就是这一格 ✓ 先前它只有裸的「读取失败：endpoint_not_local」⇒ 责任要读的人自己猜 ✗ 这张同样是修完之后重拍的 ✓ 两张卡同一件事现在说同一种话））。

### 本轮**只有看真截图才会去查**的东西
* ★ **头号就是 ㊳**：`02` 与 `10` 两张上 ✓ 回执写着「画面确实动了 · 实测 [200,50,50,255] → [200,143,0,255]」✓ 而**同一张卡底下那一行**仍挂着「指纹没变 ⇒ 这一笔对画面是空操作」✗ 判据跑到那一步**全绿**（它比的是 `#effectInfo` 那一行 ✓ 而这句话在**另一格** `#effectNote` 里）⇒ 也就是说**「页面 == 回执」这类断言看不见「旁边那行常设文案在打脸」** ✓ 只有把整张卡截下来一起看才会去查 ✗ 本轮因此补了 ①′③′⑥′⑦′ 那组**可见性**断言（把「旁边那行」也纳入被问的对象 ✓）。
* `10` 那张是**特意去拍**的：判据跑到本地那一臂时 ✓ 屏幕上的句子已经对了 ✓ 但只有看图才会顺带去查"为什么本地一定被拒"⇒ 顺着 `api-local.js:2119` 那一处 `adjustment_name` 才把 `docs/FOR-APPLE2011.md:4707` **早就写明**的那条命名分叉**落到屏幕上**（✗ 不是本轮新发现的缺陷 ✓ 是"文档知道而界面在骗人"的缺陷）。也就是说：**一条说实话的消息本身把缺陷指了出来** ✓ 如果本轮选择让它继续冒充"服务端拒绝"，这条缺陷会一直藏在错误的责任归属底下。
* ★ **同一张卡看了第二遍又掉出两件事**（㊶ ⇒ 也就是说"看过一次截图"不是一种可以勾掉的完成状态 ✓ 修完之后**再看一遍**才有意义）：`10` 那张第二遍才看见回执说责任、**底下那一格只念裸码**（判据当时读的是 `#effectInfo` ✓ 那一格不在被问集合里 ⇒ 与 ㊳ 一模一样的盲区）；`04` 那张第二遍才看见相邻两行**逐字重复同一句结论**（这种缺陷**永远不会让任何判据红** ✓ 它不影响可见性、不影响密度、不影响翻译 ⇒ 只有人眼会觉得"这卡片在跟自己复读"）。
* `04` 与 `10` 这两张**是修完之后重拍的** ✓ 所以文档里那两句文案描述（「为什么？见鼠标提示」／「读取失败（本地内核）：endpoint_not_local」）是**屏幕上真实存在的字** ✗ 不是"我打算改成这样"的预告 ✓（这条区分很重要：本轮一开始我把旧文案的描述留在了新截图的说明里 ⇒ 那等于用文字冒充像素 ✓ 与 ㊴ 抄读数同族）。
* `03`（**整页**那张）上顺手复核了两件事 ✓ 一是右栏「调整 / 滤镜」→「锚点」→「范围遮罩」三块**依次排开、没有互相遮挡也没有把画布挤没**（第六轮 ㉘ 那块浮层与本轮这张卡同栏共存 ✓ 看图才知道"两张卡同时展开"是常态而不是极端情况）✓ 二是顶栏水位 `head 4 rendered 4 dirty 0 已保存` —— 效果卡反复建／删之后**仍是 `dirty 0`**（第五轮 ㉗ 那条"水位不许骗人"没有再打脸 ✓ 底部「可撤销 2 笔 / 可重做 一 笔」也与建了两笔调整的事实对得上）。

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
11. **偏好有两份存储 ⇒ 跨模式看不见，该不该收敛。** 实测（第四轮 ✓ 登记 ⑭）：服务端那份是 `<工作区根>/preferences.json` ✓ 本地内核那份是 IndexedDB 的 `prefs` 表（`api-local.js:994-999` ✓）⇒ **同一个键两份真相**。后果不只是判据：用户勾掉「服务器渲染」之后 ✓ Agent／另一台设备经服务端设的参考图、以及任何"经服务端写的视图类偏好"在页面上**全都读不到**（页面只是安静地不画 ✓ `refreshReferenceOverlay` 的 `catch` 是空的 ✓）。可选解法：① 在线时视图类偏好**一律以服务端为准**（离线才回落本地）；② 本地 `get_preferences` 合并服务端那一份（联网时）；③ 保持现状但在界面上说明"本地模式下这些设置来自本机"。三条都改行为契约 ✗ 不由本轮顺手改 ✓。
12. **服务端生成的文档数据（默认图层名）要不要随界面语言。** 英文界面下图层下拉里出现「图层 1 (#layer_default)」（真截图 ✓ 见登记 ⑲ 末）——**不是漏翻**：`crates/yanshi-core/src/fold.rs:1011` 与 `crates/yanshi-server/src/tools.rs:3485` 在**建文档/自动建层**那一刻就把名字**写进原子**（`"图层 1"` / `format!("图层 {tail}")` ✓）。要"跟着界面走"只有三条路：① 服务端按**请求语言**生成名字（⇒ 同一个文档在不同语言下建出的层名不同 ✗ 且**改不掉已存的** ✓）；② 存一个"未命名"哨兵 ⇒ 由界面按当前语言**显示**（⇒ 层名不再是单一真相 ✓ 导出/CLI 看到的是哨兵 ✗）； 接受现状（名字是**用户可改的数据** ✓ 英文用户可自行改名 ✓）。三条都动**数据模型或既有文档** ✗ 不由本轮顺手改 ✓。
13. **本地内核（PWA）要不要补这三条端点。** 现在有三张卡的服务端工具在本地内核**根本没有端点**：`create_range_mask`（登记 ②）✓ `frequency_separation`（登记 ⑨）✓ `dodge_burn_setup`（登记 ㉕ ✓ 本轮实测 `grep -c … api-local.js` = **0**）。本轮统一做了"**说实话**"那一半：请求不出门 ⇒ 界面念「本地内核没有 X 这个端点（这次请求没到服务端）⇒ 勾回「服务器渲染」再试」✓ 不冒充"服务端拒绝" ✗。补端点本身是**本地内核工作量**（`dodge_burn_setup` 要 add_layer + 蒙版 + 曲线一整套 ✓ `frequency_separation` 要高频/低频两层 + 线性混合 ✓），且每条都要与 `yanshi-server` 逐字段同语义 ⇒ 属"要不要投这份工"的决策 ✗ 不由一轮功能补丁顺手决定 ✓。
14. **卡片上"预测句"与"回执行"要不要在样式上分开。** 本轮的减淡加深卡延续全局惯例：`#dodgeBurnCurrent` 念"这次会新建/复用哪个组"（**按下去之前** ✓）✓ `#dodgeBurnInfo` 念"已建组 … ／复用既有组 …"（**按下去之后** ✓ 由服务端回执拼）✓ 两行**同形**（同一 `.hint` 两行式 ✓ 频率分离卡、区域填充卡都是这形状）⇒ 用户不容易分清哪句是许诺、哪句是兑现（真截图复核时看到的 ✓ 本轮**刻意没单独改这一张** ✗ 单独改会让这一张与其余卡更不像 ✓）。要改就整套卡片一起改（给"已兑现"那一行加边框/图标 ✓ 或加"预告/结果"两个小标签）⇒ 属界面语言的一致性决策 ✓ 且要同步 `ui-text-density` 与 `ui-i18n-coverage` 两条判据的口径 ✓。
15. **顶栏那一格 `rendered` 到底该说谁的真相。** 本轮把"数字搁死"修成"数字会自己收敛"（登记 ㉗ ✓ 新判据 `browser-status-bar.mjs` 钉住 ✓），但**含义仍是二义的**：`refreshPreview()` 乐观写的是"**客户端**画到第几"✓ 而 `refreshThumb()` 抄的是"**服务端预览/缩略图**到第几"（`rendered_seq` ✓ `tools_io.rs:635-672` 上游自己注明"拿它当画布状态是误报"✓）。同名的两件事挤在一格 ⇒ 用户只能猜。三种正法：① 拆成两格（「画布 25」「预览 22」✓ 要改 `viewer.rs` 模板 + 中英词条 + `ui-*` 判据口径）；② 只留客户端语义 ⇒ 页面自己记账（`dirty`/`rendered` 全部由本地渲染完成事件写 ✓ 服务端那份只在"预览"卡上出现）；③ 只留服务端语义 ⇒ 删掉 `refreshPreview()` 那句乐观写（⇒ 本地刚画完也要等一次 `get_document` 才更新 ✓ 会有可见延迟）。本轮选的是**不改语义、只修搁死**（最小、可回退 ✓ 且新判据在三种正法下都不用重写 ✓）⇒ 选哪条属界面信息架构决策 ✗ 不由一轮补丁顺手定 ✓。
16. **本地内核（PWA）的调整对象键名 `adjustment_name` 要不要改成服务端的 `adjustment_type`。** 实测（第七轮 ✓ 登记 ㊱）：本地 `p2cAddEffect` 用的键名是 `adjustment_name`（`api-local.js:2119` ✓ 滤镜那一支的 `filter_name` 是对的 ✓ 只有调整这一支错）✓ 而服务端建原子与 `list_effects` 用的都是 `adjustment_type`（`tools.rs:12766`／`:11221`／`:12517`）✓ 仓库自己的 `docs/FOR-APPLE2011.md:4707` **早已写明正确名是 `adjustment_type`** ⇒ ① 本地模式下效果卡的调整**必被拒** ② 本地建出的原子**服务端读不出来**（切回「服务器渲染」就"效果凭空消失"✓ 静默的数据形状分叉）。改法本身只有一行（把 `nameKey` 那一支换成 `adjustment_type` ✓ 两份镜像按流程同步），但**本轮无法证明它修好了**：需要一条"PWA 建原子 ⇒ 服务端读得到"的双端重放判据 ✓ 而本机 `web/mediums/*.wasm` 一个都没有（登记 ⑬ 同因 ✓ `tool-pwa-assets` 环境红）⇒ 对着 mock 报绿不算修好。要投这份工需要：① 介质 wasm 到位（或只在 CI 里跑）✓ ② 决定既有本地文档里那条 `adjustment_name` 原子是**迁移**还是**读时兼容两个键**（⇒ 数据模型决策 ✓ 会动已存的用户数据）✗ 不由一轮补丁顺手定 ✓。
17. **提交前那道 `cargo clippy --workspace --all-targets` 要不要按机器预算调档。** `scripts/pre-commit.sh:23` 把它设成**不可绕过**的门 ✓ 设计意图是对的（第 834/998/1024/1051/1059/1562 轮各加一道 ⇒ 全是"CI 先抓到 ⇒ 挪到提交前"）✓ 但它**没带 `-j`** ✓ 跑的是 **debug 档全工作区全 target** ⇒ 实测本机：**11 G 内存 / 3 G 可用**（本场曾被 OOM kill ✓ 用户明确要求控制内存）＋ **根盘只剩 2.1 G（94%）** ✓ 而 `target/debug` 已 **14 G**（脚本 `:114-119` 那条"磁盘卫生提醒"自己也触发了）⇒ **我这一轮没能整条跑它** ✗ 只能用"三个受影响 crate 的 release clippy"替（exit 0 ✓ 其余各条 fmt／台账引用／四条 self-test／两个条件门**都按脚本原样跑过并全绿** ✓ 逐条读数见「本轮判据读数」）。三个选项：① 脚本读 `CARGO_BUILD_JOBS`／按 `nproc` 与内存自动降档（**改的是别人的门** ⇒ 且各机器强度不一致 ✓ 一致性反而变差）；② 把 ② 拆成"scoped 必过 + workspace 交给 CI"（**弱化提交前覆盖** ✗ 与它当初被加进来的理由冲突）；③ 什么都不改 ✓ 由实现者保证在够大的机器上跑（⇒ 小机器上**每次都要如实声明"没整条跑"** ✓ 本轮就是这个状态）。⇒ 属仓库门禁策略决策 ✗ 不由一轮 web 补丁顺手定 ✓（连带一条：`target/debug` 那 14 G 要不要清 ⇒ 同样待用户点头 ✓ 清了下一次 `cargo test` 要重编很久 ✗ 本轮没动它）。

---

## 本次未测

* **`browser-brush-preview-local.mjs` 的真实结论**：本次用 chromium（`Chromium 153.0.8010.52`）连上运行中的服务端实跑了它，得到 `{"options":200,"rows":199,"local":0,"dataUrls":199}`、退出码 1。**但红的原因不是产品缺接口**：同一页面用 CDP 读出 `img.getAttribute("src") = /brush-previews/classic-brush.myb.png`、`img.src = http://127.0.0.1:15900/brush-previews/classic-brush.myb.png`，而判据用 `.src`（绝对 URL）去 `startsWith("/brush-previews/")`（相对前缀）⇒ 恒不成立。产品侧计数 `brushPreviewsFromFiles = 199`，即 **199 行全部走入库预览**。所以：已知红条目写的理由（"测的是不存在的接口 / viewer.rs 零命中"）已与判据正文（`scripts/browser-brush-preview-local.mjs:60-70` 注释说已改测 `/brush-previews/`）矛盾。**要判定"产品是否缺该接口"，需要先修判据的属性/属性前缀比较**；本文不替产品下结论。
* **`tool-paint-memory.mjs` 的成本比**：本次按 `node scripts/tool-paint-memory.mjs http://127.0.0.1:15900 12` 复跑，判据只记录到 5 笔、打印"**笔数 < 6 ⇒ 成本比不判**"、退出码 0，**没有量到**已知红里那个 `1.66×`。故本文引用该数字时标注为"已知红文件记录"，不是本次实测；要复现需先查明该判据为何只数到 5 笔。
* **`tool-archive-bloat.mjs` / `tool-impasto-plateau.mjs` / `kernel-brush-parity.mjs` / `browser-ui-check.mjs`**：本次未逐条运行，理由/数字均逐字引自 `scripts/criteria-known-red.txt`，未独立复测。
* **`tool-surface-coverage.mjs` 的当前红灯**：本次实跑退出码 1，报 2 处不一致——`crates/yanshi-server/src/tools.rs:33` 与 `:358` 仍写「核心层 … 68 个」，实测 core = 69（`:4` 与 `:943` 已改成 69）。该脚本**不在** `criteria-known-red.txt` 的 8 条名单里，而 `scripts/run-criteria.sh` 的枚举是 `scripts/tool-*.mjs` ⇒ 按现有接线它会让 CI 变红。要判定"CI 当前是否红"，需要在 CI 配置里确认分片与超时后实跑一次，本文未做。
* **介质的两条实现是否逐像素一致**：浏览器 WASM 路径与服务端 `medium_stroke` 未做逐像素比对；需要写一条类似 `kernel-brush-parity.mjs` 的判据。
* **`docs/design/tool-examples.md` 的完整差异**：本次只量到"文档 47 个 `##` 小节 vs 运行目录里 109 个带 `example` 的工具"，判据 `tool-examples-doc-check.mjs` 退出码 1（已知红）。具体缺哪些小节需运行生成器后逐条 diff，本文未做。
* **（第六轮）本地内核 `load_atoms_json` 里那个 wasm `unreachable` trap 的**内核侧根因**：本轮只做了 Web 侧的**缓解**（`localKernelUnusableNow()` ⇒ 检测到内核不可用就不拨请求、并把原因说清 ✓ 登记 ㉘ 的 ⑫ 那条断言钉住 ✓）。**为什么** wasm 在装载这份文档时走到 `unreachable`（是版本不匹配的 `atoms.json`？还是某类原子未实现？）✗ 未查 ⇒ 需要一条直接跑 wasm 的内核判据 ✗ 不在 Web 这轮的范围内 ✓。
* **（第六轮）英文的**单复数**：`i18nText()` 生成的句子按**数字**填格 ✓ 但模板是固定的 `conflict(s)`／`object(s)` 式写法（m3/m4/m15/m16/m20 ✓ 真截图 `08-kernel-fallback-en.png` 上能读到 `0 strokes undoable` 这种"零也用复数"的句子 ✓）。中文没有这个范畴 ⇒ 判据不会红 ✓ 本轮**未改**：要改得先定"英文词条按量词分叉"的口径（词条表会长一倍 ✓）并给 `ui-i18n-coverage` 加一条"数字句必须按单复数取词"的断言 ✓ 属界面语言决策 ⇒ 见「需要用户决策」同族（本轮登记为**已知瑕疵** ✗ 不当缺陷修）。

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
#    第四轮起：端口**优先读 `CDP_PORT` 环境变量** ✓ argv[3] 只有在"是纯数字"时才当端口用
#    （CI 的 argv[3] 是 server-base ✗ 见登记 ⑯ ✓）⇒ 下面两种调用形状都成立 ✓
CDP_PORT=9490 node scripts/browser-frequency-separation.mjs "http://127.0.0.1:15900/?doc=boot&token=$tok"
CDP_PORT=9490 node scripts/browser-blend-modes.mjs "http://127.0.0.1:15900/?doc=boot&token=$tok"

# 9) 第四轮的三条（同一族 ✓ 用 **CI 的参数形状**跑才算验到 CI 那条路 ✓ 见登记 ⑭⑯）
CDP_PORT=9490 node scripts/browser-fill-region.mjs \
  "http://127.0.0.1:15900/?doc=boot&token=$tok&render=client" "http://127.0.0.1:15900" "$tok" 9490
CDP_PORT=9490 node scripts/browser-reference-decode.mjs \
  "http://127.0.0.1:15900/?doc=boot&token=$tok&render=client" "http://127.0.0.1:15900" "$tok" 9490
CDP_PORT=9490 node scripts/browser-reference-overlay.mjs \
  "http://127.0.0.1:15900/?doc=boot&token=$tok&render=client" "http://127.0.0.1:15900" "$tok" 9490
#    前两条把截图写到 `SHOTS_DIR`（缺省 /tmp/yanshi-{fill-region,reference-overlay}-shots）✓
#    **界面一律看真截图** ✓ 判据绿不等于界面没问题（本轮的破图图标只有截图里有 ✓）
# 10) 变异测试（证明判据真在测东西 ✓ 三处各击穿一条 ⇒ 红点**恰好**落在对应断言上 ✓）
#     a. 关掉羽化的区域不变量 ⇒ Rust 判据红一条（另两条照绿）
sed -i 's/} else if feather > 0.0 {/} else if feather > 1e9 {/' crates/yanshi-render/src/render.rs
cargo test -p yanshi-render --test flat_shape_geometry -j 2      # ⇒ 1 failed（整幅 [0,0,0,255] vs 窄区域 [0,0,0,194]）
git diff crates/yanshi-render/src/render.rs | patch -R -p1       # 或按备份还原 ✗ 别用 git checkout --（会连本轮改动一起丢）
#     b. 把 `error` 时的收起改回"留着" ⇒ browser-reference-decode 红两条（①④）
#     c. 再把 `hiddenTail` 抹成空串 ⇒ browser-fill-region 红一条（⑧）✓ 而 browser-blend-modes 照绿（对照 ✓ 无污染）
#     b/c 都在 `crates/yanshi-http/assets/viewer-app.js` ✓ 改完**必须重建**（`include_str!` ✓）：
#       cargo build --release -p yanshi-http -j 2 ＋ **先杀掉旧进程再起**（否则 `Address already in use` ✗ 本轮踩过 ✓）
# 改完查看器必须同步镜像 ✗ 否则 tool-pwa-assets 报分叉（见登记 ⑥ / 登记 ⑬）
#   **顺序**：改 `crates/yanshi-http/assets/` ⇒ `node scripts/pwa-sync-viewer.mjs` ⇒ **`git checkout -- web/index.html`**
#   （那份带本机缓存唯一戳 ✗ 不进补丁 ✓ 登记 ⑥）⇒ `node scripts/tool-pwa-assets.mjs` 复核"分叉"那一行**转绿**
node scripts/tool-pwa-assets.mjs                       # 本机仍红一条：assets/mediums/*.wasm 未构建
node scripts/pwa-sync-viewer.mjs && git checkout -- web/index.html

# 11) 第五轮：减淡加深（服务端工具 ⇒ CI 给这条**不加** `render=client` ✓ 判据自己也钉一次）
CDP_PORT=9490 node scripts/browser-dodge-burn.mjs \
  "http://127.0.0.1:15900/?doc=boot&token=$tok" "http://127.0.0.1:15900" "$tok" 9490
node scripts/tool-dodge-burn-effective.mjs "http://127.0.0.1:15900"   # 只取 base ✓ 自己建文档与六色带
cargo test -p yanshi-server --test retouch_tools -j 2                 # ⇒ 7 passed（含复用回执那条）
cargo test -p yanshi-render --lib -j 2                                # ⇒ 553 passed（含柔光支接缝那条）
# Web 调用面（本轮口径：viewer-app.js 里 callTool("…") 字面命中的**不同工具名**）
names() { cat "$@" | grep -ohE 'callTool(Checked)?\(\s*["'"'"'][a-z_]+["'"'"']' \
  | sed -E 's/.*["'"'"']([a-z_]+)["'"'"'].*/\1/' | sort -u; }
names <(git show 29d94086:crates/yanshi-http/assets/viewer-app.js) | wc -l   # ⇒ 87（基线）
names crates/yanshi-http/assets/viewer-app.js | wc -l                         # ⇒ 90（本包）
comm -13 <(names <(git show 29d94086:crates/yanshi-http/assets/viewer-app.js)) \
         <(names crates/yanshi-http/assets/viewer-app.js)                     # ⇒ fill_region / sample_colors / dodge_burn_setup
# 变异（证明 ㉑/㉔ 真被钉住 ✓ 做完**按备份还原**✗ 别用 git checkout --）
#   a. blend.rs:182 把 ((16.0*cb-12.0)*cb+4.0)*cb 改回 cb.sqrt()
#      ⇒ cargo test -p yanshi-render --lib 红一条 ✓ tool-dodge-burn-effective 的 B 臂报 [19,19,19,255] 红
#   b. tools_retouch.rs 复用分支改成"照**请求**的 mode 回答"
#      ⇒ cargo test -p yanshi-server --test retouch_tools 停在 left "curves" / right "gray"
#   备份：/tmp/blend.rs.good ✓ /tmp/tools_retouch.good.rs ✓（cp 回去后**必须重建** `cargo build --release -p yanshi-http -j 2`
#         并**先杀旧进程再起** ✓ 见 /tmp/yanshi-web-r5/restart-server.sh）

# 12) 第五轮：顶栏水位（登记 ㉗ ✓ 测的是服务端水位与显示 ⇒ 同样**不加** `render=client`）
CDP_PORT=9490 node scripts/browser-status-bar.mjs \
  "http://127.0.0.1:15900?doc=<doc>&token=<tok>&render=server" http://127.0.0.1:15900 <tok> 9490
#   ⇒ 18 条全绿 ✓ 全文 criteria/browser-status-bar.txt
# 先证明"服务端没算错"（决定改页面还是改服务端 ✓ 这一步不能省）：
curl -s -X POST "http://127.0.0.1:15900/api/tools/get_document?doc=<doc>&token=<tok>" -H 'content-type: application/json' -d '{}' \
  | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d["head_seq"],d["rendered_seq"])'   # ⇒ 每次真实改动后两数相等 ✓ 只有全新空文档给 1 0
# 变异（两处各自单独抽 ✓ 各自都要**重编译 + 重启 + 重跑** ✓ 做完按备份还原）
#   a. 抽掉 refreshThumb 里"读到 rendered_seq < head_seq 就再排一次"的那个上限 6 次循环
#      ⇒ ①⑤ 红（开局 `1 / 0` 停死 9 秒 ✓ 45 次读数不变）
#   b. 抽掉 WS `atom` 事件里的 scheduleThumbRefresh()
#      ⇒ ⑤ 红（远端建一层后 `head 2 / rendered 1 / dirty 0` 停死 9 秒）
#   备份：/tmp/viewer-app.statusbar-good.js ✓（还原后**必须** `cargo build --release -p yanshi-http -j 2`
#         ⇒ 重启 ⇒ 再跑 `node scripts/pwa-sync-viewer.mjs` 复核「防分叉」那一行 ✓ 然后 `git checkout -- web/index.html`）

# 13) 第七轮：效果卡那句「动没动」（登记 ㉝~㊸ ✓ 它自己两条路都走 ⇒ **不需要**在 CI 侧改导航形状）
node scripts/browser-effect-card-truth.mjs "http://127.0.0.1:15900" 9490
#   ⇒ **38 条断言全绿**（打印 39 行 ✓ 多一行是收尾那句）✓ exit 0 ✓ 约 100 s ✓ 全文 criteria/browser-effect-card-truth.txt
#   ★ **这条命令先前在这里写错了** ✗：原样是 `… <viewer-url> http://127.0.0.1:15900 <tok> 9490` ✓
#     而判据只接 `<viewer-url> [cdp-port]`（`:71、80-81` ✓ 它**自己建文档、自己拿 token**）⇒
#     argv[3] 若不是纯数字就**回落到 9333** ✓ 照抄那行的人会在一个**没在监听的端口**上失败 ✗
#     （与 `browser-reference-overlay.mjs:20-25` 记的是**同一处坑** ✓ 而它这次坑的是**我自己写的复现命令**）
#     ⇒ 文档里的命令**必须照抄源码的用法行** ✓ 不能照抄"别的判据的形状"✓
#     另外 `CDP_PORT=9490` 这个环境变量比 argv 更稳（那条 `||` 顺序 ✓ 见 `:81`）。
# 先证明"render_region 真的只读且幂等"（决定这话能不能当真 ✓ 这一步不能省）：
curl -s -X POST "http://127.0.0.1:15900/api/tools/render_region?doc=<doc>&token=<tok>" \
     -H 'content-type: application/json' -d '{"region":[540,340,40,40]}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["blob_hash"])'   # ⇒ 连调两次同一个串 ✓ 且原子数不变
# 变异五条（驱动脚本 /tmp/yanshi-web-r7/mutate.py ✓ 锚点写死 ✓ `finally` 必还原 ✓ 每条都要**重编译 + 先杀旧进程再起 + 重跑**）
#   A1. 把回执行那句结论硬编码成"画面确实动了"          ⇒ ③ 红 1 条（mut-A1.log）
#   A2. 删掉 effectProbeRegion 的 bbox 有效性 `continue` ⇒ ①②④⑤⑨⑨ 红 6 条（mut-A2.log ⇒ 取样器错会被整族抓住）
#   B.  deleteEffect 把回执当成功（`{ok:true,server:false}`）⇒ ④＋「删完服务端还留着效果对象」＋⑤清场＋⑨删除句＋收尾基线 红 5 条（mut-B.log）
#   C.  还原 refusalVenue()／failureCode()              ⇒ **恰好** ⑩b＋⑩c 红 2 条（mut-C.log ⇒ 这一族只有这两条在问）
#   D.  把 #effectNote 恢复成常设挂着（note.hidden = false）⇒ **恰好** ①′＋⑥′＋⑦′ 红 3 条 ✓ ③′ 照绿（mut-D.log ⇒ 四条各问各的）
#   E.  把 #effectNote 的 title= 整个摘掉（可见文案一字不动）⇒ **恰好** ③″ 红 1 条 ✓ ③′ 照绿（mut-E.log ⇒ 问的是"搬家之后信息还在不在"✓ 删掉不算修好）
#   F.  把 #effectsList 那句读取失败还原成裸码（不报 venue ✓ 只读 error_code）⇒ **恰好** ⑩d 红 1 条 ✓ ⑩b／⑩c 照绿（mut-F.log ⇒ 第三格独立在被问的）
#   备份：**每次运行先快照当前干净文件**（`/tmp/yanshi-web-r7/snapshot/<key><ext>` ✓ 替代原先那份"只在第一次写盘"的
#         `viewer-app.js.bak` ⇒ 那份在 D 之后已经**过时** ✗ 用它还原会把新改动抹掉 ✓ 这坑是我自己埋的）
#         ⇒ `finally` 从快照还原并**逐字节核对** ✓ 还原后 `cargo build --release -p yanshi-http -j 2` ⇒ 重启
#         ⇒ 自检「MUTATION 残留（页面）」必须回到 **0** ✓ 再跑 /tmp/yanshi-web-r7/shot-effect-card.mjs 重拍十张
# ㊱ 的取证（只查不改 ✓ 本机没有介质 wasm ⇒ 无法证明修好 ⇒ 见「需要用户决策」第 16 条）：
grep -rn --include='*.js' adjustment_name . | grep -v target/   # ⇒ 只两处：单一源 + PWA 镜像（同一份源码）
grep -n 'adjustment_type' crates/yanshi-server/src/tools.rs | head  # ⇒ 键名表 12766/11221 ✓ list_effects 读的是 12517
grep -n 'adjustment_name' docs/FOR-APPLE2011.md                   # ⇒ 4707 行**早已写明**正确名是 adjustment_type

# 14) ㊸ 的复验（"名单里这条还算不算红" ⇒ 必须**照 CI 的形状**喂它 ✓ 别只传裸 base）
doc=$(python3 -c 'import time;print("crit_r7refoverlay%d" % (time.time() % 1000))')
tok=$(curl -s -X POST http://127.0.0.1:15900/api/documents -H 'content-type: application/json' \
        -d "{\"doc_id\":\"$doc\",\"width\":640,\"height\":400}" \
      | python3 -c 'import sys,json;print(json.load(sys.stdin)["token"])')          # ⇒ 64 位小写十六进制
CDP_PORT=9490 node scripts/browser-reference-overlay.mjs "http://127.0.0.1:15900/?doc=$doc&token=$tok" 9490
#   ⇒ exit 0 ✓ 印「设了参考图之后 {"found":true,…,"opacity":0.5}／清掉之后 {"found":false}／✓」
#   ⇒ 只传裸 base 会得到 exit 3 + `capability token 必须是 64 位小写十六进制` ✓ 那是**调用方缺参数**✗ 不是产品红
grep -cE '^browser-reference-overlay\.mjs[[:space:]]' scripts/criteria-known-red.txt   # ⇒ 0（已移出名单 ✓ 文字留在原位）

# 15) 补丁自证（"字节数"这一项**不许抄** ⇒ 每次生成后当场量 ✓ 并在干净克隆上比回来）
git diff origin/main > /tmp/yanshi-web-r7/pkg/patches/01-tracked.patch
NEW="scripts/browser-dodge-burn.mjs scripts/browser-effect-card-truth.mjs scripts/browser-fill-region.mjs scripts/browser-layer-lock.mjs scripts/browser-reference-decode.mjs scripts/browser-status-bar.mjs scripts/browser-tooltip-i18n.mjs scripts/check-viewer-unique-ids.mjs scripts/tool-dodge-burn-effective.mjs"
git add -N -- $NEW
#   ★ 02 必须**只列这 9 个新文件**（✗ 不要用 `scripts/browser-*.mjs` 那种 glob ✓
#     那会把 4 个**已跟踪且已改过**的判据一起抓进来 ⇒ 与 01 重复 ⇒ `git apply` 第二份必然冲突 ✗）
git diff origin/main -- $NEW > /tmp/yanshi-web-r7/pkg/patches/02-new-criteria.patch
git reset -q -- scripts/            # ⇒ **只退索引不动工作树**（`git add -N` 是"意图添加"✓ 不 `git add` 别的东西 ✓）
wc -c /tmp/yanshi-web-r7/pkg/patches/01-tracked.patch    # ⇒ 这一行的输出就是文档里那个字节数 ✓ 抄它 ✗ 记它
rm -rf /tmp/verify-r7 && git clone --shared -q /home/polar/yanshi /tmp/verify-r7 && cd /tmp/verify-r7
git rev-parse --short HEAD                               # ⇒ 1231047b（补丁的 base ✓ 不是本地 HEAD）
git apply /tmp/yanshi-web-r7/pkg/patches/00-combined-1231047b.patch && echo "apply OK"
# 逐字节核对 30 个路径（✗ 不是"看起来一样"）：
cd /home/polar/yanshi && git status --porcelain | awk '{print $2}' | while read -r f; do
  cmp -s "/home/polar/yanshi/$f" "/tmp/verify-r7/$f" && echo "= $f" || echo "✗ $f"
done | grep -c '^='                                      # ⇒ 30（少一个就是有路径没进补丁 ✓ 例如新文件忘了 -N）
```
