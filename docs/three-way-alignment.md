# 三方功能对齐：可复现缺口清单与棘轮

> **边界**：本文是**工具级**账本 —— 一行一个工具，只回答「服务端有没有、MCP 有没有、PWA 本地有没有、本地读没读全部声明参数」。
> 数字**由棘轮现算**（`node scripts/check-three-way-alignment.mjs`）；下面的表是 2026-11-04 的快照，会随工具增减而变 ⇒ **引用数字请重跑判据，不要抄表**。
> **能力级**的结论 ——「同一份能力，人 / Agent / 页面三个面够不够得到」——在 `docs/surface-alignment.md`，那份文档不列逐工具表。
> 两边**不互相抄数字**：本文只留可现算的工具级计数，能力级判断与缺口登记留在那边（它指向本文，不复制）。

> **日期**：2026-11-04。**所有计数以 `node scripts/check-three-way-alignment.mjs` 现算为准**；
> 本文的表格是这一天的快照（行数会随工具增减而变），**不要把表里的数字当成新的权威**。
>
> **一句话**：服务端注册表 224 个工具，MCP 缺省可见 223 个（Study 组要显式 `--profile study`），
> PWA 本地有 165 个路由、缺 59 个；本地反向多出已清零；已声明参数里有 66 个本地从不读。
> 两个缺口各有一把**只许降**的棘轮：缺路由 ≤ 59、参数未读 ≤ 66。

---

## 1. 三方各自的权威源头

| 方 | 权威文件:行 | 说明 |
|---|---|---|
| **服务端（HTTP）** | `crates/yanshi-server/src/tools_registry.rs:78` | `pub const ALL_TOOLS: &[ToolSpec] = &[` —— **唯一工具表**。每个条目有 `name`／`profile`／`params`。`tools.rs:2270` 的 `dispatch` 只是按这张表分派。 |
| **服务端（profile）** | `crates/yanshi-server/src/tools.rs:36` | `pub enum Profile`。`tools.rs:208` 是 `Profile::DEFAULT`（MCP 缺省会话的组成）。`tools.rs:779` 的 `expand_profiles` 决定 workflow 预设展开。 |
| **MCP** | `crates/yanshi-mcp/src/lib.rs:307` | `let registry = ToolRegistry::with_profiles(&options.profiles);` ⇒ **MCP 没有第二份名单**，它就是同一张表按 profile 过滤。`lib.rs:341-342` 的 `tools_list_for` 与 `lib.rs:358` 的 `tools_list_json` 都走这张表。 |
| **PWA 本地** | `crates/yanshi-http/assets/api-local.js:3480` | `async function localDispatch(req)` —— 本地路由的唯一入口。 |
| **PWA 参数镜像** | `crates/yanshi-http/assets/api-local.js:595` | `const P2_TOOL_SPECS = {` 的 `<gen:tool-specs>` 生成块（223 行；少 `reference_trace_region`）。 |
| **PWA 声明计数** | `crates/yanshi-http/assets/api-local.js:95` | `const LOCAL_IMPLEMENTED = 177;`（与 `path ===` 分支机械数同口径）。 |
| **PWA 镜像** | `web/api-local.js` | 必须与 `crates/yanshi-http/assets/api-local.js` **逐字节相同**。 |

**为什么 MCP 一列几乎全是「✓」**：MCP 与 HTTP 共用 `ALL_TOOLS`。唯一差别是缺省 profile：
`Profile::DEFAULT` 不含 `Study`，所以 `reference_trace_region` 默认不可见，**显式
`--profile study` 时仍可用**。这不是缺口，是分组策略。

---

## 2. 怎么复现

```bash
# 四个数 + 全部断言（绿 exit 0；红 exit 1；判不了 exit 3）
node scripts/check-three-way-alignment.mjs; echo "EXIT=$?"

# 两份本地层必须逐字节相同
cmp crates/yanshi-http/assets/api-local.js web/api-local.js; echo "CMP=$?"
```

2026-11-04 实测输出（节选）：

```
注册表工具 224｜MCP 缺省可见 223（Study 专属 1 只需 --profile study）
本地镜像规格 223 行｜有本地路由 165｜缺本地路由 59
声明参数本地未读 66（22 个工具）｜本地反向多出 0
```

**棘轮常量**在 `scripts/check-three-way-alignment.mjs:52-53`：

```js
const MISSING_CEILING = 59;
const DEAD_PARAMS_CEILING = 66;
```

**只许降**。要涨必须像 `check-index-html-byte-budget.mjs` 那样抬常量，并在提交信息里写清
「谁涨了多少、为什么」——让缺口**必须被看见**，这是这两个常量的全部目的。

---

## 3. 逐工具表

图例：
- **服务端有** = `ALL_TOOLS` 里有；括号是 profile。
- **MCP 有** = `Profile::DEFAULT` 可见；`✗` 只有 `reference_trace_region`（Study）。
- **PWA 本地有** = `localDispatch` 里有 `path === "/api/tools/<name>"` 分支（或登记的别名）。
- **声明参数本地全读** = 生成块里该工具的参数名，在生成块之外**非 0 次出现**（单向判据：
  0 次 ⇒ 一定没读；> 0 次 **不代表**读了）。`—` = 没有本地路由，参数无从读起。

### A. 缺本地路由（59 个，优先处理）

| 工具 | 层 | 归因 | 下一步 |
|---|---|---|---|
| `apply_curve_preset` | Core | K | 不做：需 `.myb` 动力学解析，内核未导出 |
| `bake_document` | Core | K | 不做：需内核 bake |
| `bake_layer` | Core | K | 不做：需内核 bake |
| `calibrate` | Core | K | 不做：要真渲染 + 像素量回 |
| `combine_shapes` | Core | K | 不做：服务端是**位图路线**（像素场布尔） |
| `commit_batch` | Core | N | 排期：需本地试画表（IndexedDB） |
| `commit_stroke` | Core | N | 排期：同上 |
| `crop_canvas` | Core | F | 排期：先补 JS 折叠，再加路由 |
| `discard_stroke` | Core | N | 排期：需本地试画表 |
| `explain_tool` | Core | G | 排期：生成器要先镜像 summary/doc |
| `export_book_pngs` | Core | K | 不做：多文档渲染 + 编码 |
| `export_psd_layers` | Core | P | 排期：移植 PSD 树编码（本地已有单层 PSD 的 JS 解码） |
| `export_svg` | Core | K | 不做：非矢量笔触要嵌 PNG，必须有渲染 |
| `export_timelapse` | Core | K | 不做：服务端走 ffmpeg，浏览器没有等价编码器 |
| `extend_canvas` | Core | F | 排期：先补 JS 折叠 |
| `fit_background_field` | Core | K | 不做：多项式拟合 + 像素铺底 |
| `flip_canvas` | Core | F | 排期：先补 JS 折叠 |
| `get_bake_status` | Core | K | 不做：状态来自 bake 机制 |
| `get_brush_dynamics` | Core | K | 不做：需 `.myb` 解析 + 内核导出 |
| `get_brush_settings` | Core | K | 不做：同上 |
| `import_psd_layers` | Core | P | 排期：移植 PSD 图层树（本地已有单层） |
| `list_attempts` | Core | N | 排期：需本地试画表 |
| `list_fonts` | Core | M | 不做：浏览器无法枚举系统字体；服务端清单是**服务端 rasterizer** 的，镜像会让用户选到画不出的族名。前端已在该调用失败时回退（`web/viewer-app.js:12872`） |
| `memory_status` | Core | M | 不做：服务端进程 RSS/会话量度，PWA 没有对应进程 |
| `rebackground` | Core | K | 不做：改底色要重渲染 |
| `recover_baked_blobs` | Core | K | 不做：需 bake 重放 |
| `release_document` | Core | M | 不做：PWA 没有「内存里放下的文档」这层；文档就在 IndexedDB |
| `reopen_document` | Core | M | 不做：同上；`list_documents` 已是全量视图 |
| `resize_canvas_bounds` | Core | F | 排期：先补 JS 折叠 |
| `revoke_token` | Core | M | 不做：PWA 的 token 是占位 `"local"`，没有真实令牌库 |
| `rotate_canvas` | Core | F | 排期：先补 JS 折叠 |
| `scale_canvas` | Core | F | 排期：先补 JS 折叠 |
| `set_brush_curve` | Core | K | 不做：需 `.myb` 解析 + 内核导出 |
| `set_brush_setting` | Core | K | 不做：同上 |
| `smart_blend` | Core | K | 不做：内核笔触生成 |
| `smart_edge_soften` | Core | K | 不做：内核笔触生成 |
| `smart_erase_shape` | Core | K | 不做：像素擦除 |
| `smart_fill` | Core | K | 不做：内核笔触生成 |
| `smart_glaze` | Core | K | 不做：内核笔触生成 |
| `smart_hatch_follow` | Core | K | 不做：内核笔触生成 |
| `smart_sample` | Core | K | 不做：内核笔触生成 |
| `smart_select` | Core | K | 不做：像素泛洪 |
| `smart_texture` | Core | K | 不做：内核笔触生成 |
| `stroke_path` | Core | K | 不做：真笔刷沿路径烘焙 |
| `swatch_brush_strokes` | Core | K | 不做：内核试笔 |
| `toned_ground` | Core | K | 不做：内核笔触生成 |
| `try_batch` | Core | N | 排期：需本地试画表 |
| `try_stroke` | Core | N | 排期：需本地试画表 |
| `vector_erase` | Core | K | 不做：矢量脊线截断 + 渲染 |
| `verify_package` | Core | P | 排期：纯 JS 的 tar 解析；收益低，暂缓 |
| `stroke_heatmap` | History | K | 不做：墨量要读烘焙 tile 的 alpha |
| `apply_adjustments` | Retouch | K | 不做：调整对象 + 渲染 |
| `auto_retouch` | Retouch | K | 不做：图像统计 + 渲染 |
| `dodge_burn_setup` | Retouch | K | 不做：烘焙 + 内核笔触 |
| `frequency_separation` | Retouch | K | 不做：像素分解 |
| `paint_mask` | Structure | K | 不做：蒙版像素沉积 |
| `select_opaque_pixels` | Structure | K | 不做：读像素 alpha |
| `transform_selection` | Structure | P | 排期：矢量选区可 JS；位图重采样仍需内核 |
| `reference_trace_region` | Study | K | 不做：参考图分析；Study 组本身不进缺省 MCP |

**归因缩写**：
- **K** = 必须内核渲染/像素/编解码（PWA 没有等价入口，或移植成本远大于收益）
- **F** = 需先补本地**状态折叠 parity**（canvas 原子在 `yanshi-core` 有，但本地
  `p2cDocSize`／`p2cState` 只读 `create_document`，直接写原子会让面板尺寸错）
- **N** = 需新增**试画状态机**（`try_stroke` 族；不是别名，是新的 IndexedDB 表）
- **P** = 纯前端可补，但要移植一块算法（不是「只差一个路由别名」）
- **G** = 可补，但需先扩规格生成器（`explain_tool` 要 summary/doc，生成块目前只有参数名/类型/必填）
- **M** = 明确不做：镜像会撒谎（无对应进程/浏览器能力）

### B. 有本地路由（165 个）

| 工具 | 服务端有 | MCP 有 | PWA 本地有 | 声明参数本地全读 |
|---|---|---|---|---|
| `create_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `delete_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `get_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `list_annotations` | ✓ Annotation | ✓ | ✓ | 是 |
| `reject_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `resolve_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `update_annotation` | ✓ Annotation | ✓ | ✓ | 是 |
| `abort_changeset` | ✓ Changeset | ✓ | ✓ | 是 |
| `apply_stash` | ✓ Changeset | ✓ | ✓（缺 1） | 否 |
| `begin_changeset` | ✓ Changeset | ✓ | ✓ | 是 |
| `begin_transaction` | ✓ Changeset | ✓ | ✓ | 是 |
| `commit_changeset` | ✓ Changeset | ✓ | ✓ | 是 |
| `commit_transaction` | ✓ Changeset | ✓ | ✓ | 是 |
| `discard_stash` | ✓ Changeset | ✓ | ✓（缺 1） | 否 |
| `get_changesets` | ✓ Changeset | ✓ | ✓ | 是 |
| `list_stashes` | ✓ Changeset | ✓ | ✓ | 是 |
| `revert_changeset` | ✓ Changeset | ✓ | ✓ | 是 |
| `submit_offline` | ✓ Changeset | ✓ | ✓ | 是 |
| `accept_suggestion` | ✓ Collab | ✓ | ✓ | 是 |
| `accept_suggestions` | ✓ Collab | ✓ | ✓ | 是 |
| `comment` | ✓ Collab | ✓ | ✓ | 是 |
| `list_comments` | ✓ Collab | ✓ | ✓ | 是 |
| `list_suggestions` | ✓ Collab | ✓ | ✓ | 是 |
| `preview_suggestion` | ✓ Collab | ✓ | ✓ | 是 |
| `reject_suggestion` | ✓ Collab | ✓ | ✓ | 是 |
| `reject_suggestions` | ✓ Collab | ✓ | ✓ | 是 |
| `suggest` | ✓ Collab | ✓ | ✓ | 是 |
| `resolve_conflict` | ✓ Conflict | ✓ | ✓（缺 4） | 否 |
| `add_to_group` | ✓ Core | ✓ | ✓ | 是 |
| `analyze_region` | ✓ Core | ✓ | ✓ | 是 |
| `batch` | ✓ Core | ✓ | ✓（缺 5） | 否 |
| `brush_preview` | ✓ Core | ✓ | ✓（缺 5） | 否 |
| `brush_stroke` | ✓ Core | ✓ | ✓（缺 22） | 否 |
| `cancel_job` | ✓ Core | ✓ | ✓ | 是 |
| `cancel_operation` | ✓ Core | ✓ | ✓ | 是 |
| `clear_reference` | ✓ Core | ✓ | ✓ | 是 |
| `collect_diagnostics` | ✓ Core | ✓ | ✓（缺 2） | 否 |
| `create_group` | ✓ Core | ✓ | ✓ | 是 |
| `create_instance` | ✓ Core | ✓ | ✓ | 是 |
| `create_layer` | ✓ Core | ✓ | ✓ | 是 |
| `delete_anchor` | ✓ Core | ✓ | ✓ | 是 |
| `delete_document` | ✓ Core | ✓ | ✓ | 是 |
| `delete_layer` | ✓ Core | ✓ | ✓ | 是 |
| `delete_object` | ✓ Core | ✓ | ✓ | 是 |
| `describe_canvas` | ✓ Core | ✓ | ✓ | 是 |
| `detach_instance` | ✓ Core | ✓ | ✓ | 是 |
| `draw_shape` | ✓ Core | ✓ | ✓ | 是 |
| `draw_stroke` | ✓ Core | ✓ | ✓ | 是 |
| `draw_text` | ✓ Core | ✓ | ✓ | 是 |
| `edit_path` | ✓ Core | ✓ | ✓ | 是 |
| `erase` | ✓ Core | ✓ | ✓ | 是 |
| `export_png` | ✓ Core | ✓ | ✓（缺 4） | 否 |
| `export_project` | ✓ Core | ✓ | ✓（缺 2） | 否 |
| `fill` | ✓ Core | ✓ | ✓ | 是 |
| `fill_region` | ✓ Core | ✓ | ✓ | 是 |
| `get_anchors` | ✓ Core | ✓ | ✓ | 是 |
| `get_atom` | ✓ Core | ✓ | ✓ | 是 |
| `get_dependency_graph` | ✓ Core | ✓ | ✓ | 是 |
| `get_document` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `get_histogram` | ✓ Core | ✓ | ✓ | 是 |
| `get_inflight` | ✓ Core | ✓ | ✓ | 是 |
| `get_job` | ✓ Core | ✓ | ✓ | 是 |
| `get_log` | ✓ Core | ✓ | ✓ | 是 |
| `get_object` | ✓ Core | ✓ | ✓ | 是 |
| `get_path` | ✓ Core | ✓ | ✓ | 是 |
| `get_preferences` | ✓ Core | ✓ | ✓ | 是 |
| `get_render_status` | ✓ Core | ✓ | ✓ | 是 |
| `get_resolved_state` | ✓ Core | ✓ | ✓ | 是 |
| `get_state` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `get_undo_status` | ✓ Core | ✓ | ✓ | 是 |
| `gradient_blend` | ✓ Core | ✓ | ✓ | 是 |
| `gradient_fill` | ✓ Core | ✓ | ✓ | 是 |
| `import_asset` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `import_image` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `import_project` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `import_psd` | ✓ Core | ✓ | ✓ | 是 |
| `link_to_master` | ✓ Core | ✓ | ✓ | 是 |
| `list_assets` | ✓ Core | ✓ | ✓ | 是 |
| `list_atoms` | ✓ Core | ✓ | ✓ | 是 |
| `list_brushes` | ✓ Core | ✓ | ✓ | 是 |
| `list_documents` | ✓ Core | ✓ | ✓ | 是 |
| `list_engine_capabilities` | ✓ Core | ✓ | ✓ | 是 |
| `list_layers` | ✓ Core | ✓ | ✓ | 是 |
| `list_objects` | ✓ Core | ✓ | ✓ | 是 |
| `list_palette_colors` | ✓ Core | ✓ | ✓ | 是 |
| `list_textures` | ✓ Core | ✓ | ✓ | 是 |
| `medium_stroke` | ✓ Core | ✓ | ✓ | 是 |
| `move_object` | ✓ Core | ✓ | ✓ | 是 |
| `new_document` | ✓ Core | ✓ | ✓ | 是 |
| `put_blob` | ✓ Core | ✓ | ✓ | 是 |
| `reapply` | ✓ Core | ✓ | ✓ | 是 |
| `redo_last` | ✓ Core | ✓ | ✓ | 是 |
| `remove_from_group` | ✓ Core | ✓ | ✓ | 是 |
| `render_region` | ✓ Core | ✓ | ✓（缺 6） | 否 |
| `reorder_layers` | ✓ Core | ✓ | ✓ | 是 |
| `reorder_objects` | ✓ Core | ✓ | ✓ | 是 |
| `replace_object_data` | ✓ Core | ✓ | ✓ | 是 |
| `revert` | ✓ Core | ✓ | ✓ | 是 |
| `revert_atoms` | ✓ Core | ✓ | ✓ | 是 |
| `revert_group` | ✓ Core | ✓ | ✓ | 是 |
| `revert_to_seq` | ✓ Core | ✓ | ✓ | 是 |
| `sample_color` | ✓ Core | ✓ | ✓ | 是 |
| `sample_colors` | ✓ Core | ✓ | ✓ | 是 |
| `save_palette` | ✓ Core | ✓ | ✓ | 是 |
| `scatter_strokes` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `set_anchor` | ✓ Core | ✓ | ✓ | 是 |
| `set_brush_dynamics` | ✓ Core | ✓ | ✓（缺 1） | 否 |
| `set_group_transform` | ✓ Core | ✓ | ✓ | 是 |
| `set_layer_blend` | ✓ Core | ✓ | ✓ | 是 |
| `set_preferences` | ✓ Core | ✓ | ✓ | 是 |
| `set_reference` | ✓ Core | ✓ | ✓ | 是 |
| `texture_background` | ✓ Core | ✓ | ✓ | 是 |
| `undo_last` | ✓ Core | ✓ | ✓ | 是 |
| `update_layer` | ✓ Core | ✓ | ✓ | 是 |
| `update_object` | ✓ Core | ✓ | ✓ | 是 |
| `update_override` | ✓ Core | ✓ | ✓ | 是 |
| `update_stroke` | ✓ Core | ✓ | ✓ | 是 |
| `update_sync_policy` | ✓ Core | ✓ | ✓ | 是 |
| `vector_hit_test` | ✓ Core | ✓ | ✓ | 是 |
| `blob_gc` | ✓ History | ✓ | ✓ | 是 |
| `checkpoint` | ✓ History | ✓ | ✓ | 是 |
| `declare_head` | ✓ History | ✓ | ✓ | 是 |
| `find_atom` | ✓ History | ✓ | ✓ | 是 |
| `get_ancestors` | ✓ History | ✓ | ✓ | 是 |
| `get_checkpoints` | ✓ History | ✓ | ✓ | 是 |
| `get_descendants` | ✓ History | ✓ | ✓ | 是 |
| `get_diff` | ✓ History | ✓ | ✓ | 是 |
| `get_object_history` | ✓ History | ✓ | ✓ | 是 |
| `get_stroke_log` | ✓ History | ✓ | ✓ | 是 |
| `restore_checkpoint` | ✓ History | ✓ | ✓ | 是 |
| `revert_to` | ✓ History | ✓ | ✓ | 是 |
| `add_adjustment` | ✓ Retouch | ✓ | ✓ | 是 |
| `add_filter` | ✓ Retouch | ✓ | ✓ | 是 |
| `apply_preset` | ✓ Retouch | ✓ | ✓ | 是 |
| `clone_stamp` | ✓ Retouch | ✓ | ✓ | 是 |
| `estimate_dehaze` | ✓ Retouch | ✓ | ✓ | 是 |
| `get_preset` | ✓ Retouch | ✓ | ✓ | 是 |
| `heal_stamp` | ✓ Retouch | ✓ | ✓ | 是 |
| `liquify_pinch` | ✓ Retouch | ✓ | ✓ | 是 |
| `liquify_push` | ✓ Retouch | ✓ | ✓ | 是 |
| `liquify_twirl` | ✓ Retouch | ✓ | ✓ | 是 |
| `list_effects` | ✓ Retouch | ✓ | ✓ | 是 |
| `list_presets` | ✓ Retouch | ✓ | ✓ | 是 |
| `patch` | ✓ Retouch | ✓ | ✓ | 是 |
| `preview_preset` | ✓ Retouch | ✓ | ✓ | 是 |
| `resample` | ✓ Retouch | ✓ | ✓ | 是 |
| `smudge` | ✓ Retouch | ✓ | ✓（缺 1） | 否 |
| `update_adjustment` | ✓ Retouch | ✓ | ✓ | 是 |
| `update_filter` | ✓ Retouch | ✓ | ✓ | 是 |
| `collect_garbage` | ✓ Structure | ✓ | ✓（缺 1） | 否 |
| `convert_to_path` | ✓ Structure | ✓ | ✓ | 是 |
| `convert_to_shape` | ✓ Structure | ✓ | ✓ | 是 |
| `create_mask` | ✓ Structure | ✓ | ✓（缺 1） | 否 |
| `create_range_mask` | ✓ Structure | ✓ | ✓ | 是 |
| `create_selection` | ✓ Structure | ✓ | ✓（缺 1） | 否 |
| `delete_mask` | ✓ Structure | ✓ | ✓ | 是 |
| `delete_selection` | ✓ Structure | ✓ | ✓ | 是 |
| `duplicate_layer` | ✓ Structure | ✓ | ✓ | 是 |
| `list_selections` | ✓ Structure | ✓ | ✓ | 是 |
| `lock_layer` | ✓ Structure | ✓ | ✓ | 是 |
| `path_edit` | ✓ Structure | ✓ | ✓（缺 3） | 否 |
| `restore_object` | ✓ Structure | ✓ | ✓ | 是 |
| `set_property` | ✓ Structure | ✓ | ✓ | 是 |
| `transform_object` | ✓ Structure | ✓ | ✓ | 是 |
| `unlock_layer` | ✓ Structure | ✓ | ✓ | 是 |

> **注 1**：`list_brushes` 本地分支返回**空表**（`api-local.js` 的 `/list_brushes`），
> 参数名（`filter`/`medium`/`limit`/`offset`）被别处的同名变量掩盖，所以表里仍是「是」。
> 这是**已知静默降级**（空表冒充「没有笔刷」），下一步是补真实清单或改成 501 —— 见 §5。
>
> **注 2**：`apply_stash`／`discard_stash`／`list_stashes`／`medium_stroke`／
> `set_brush_dynamics`／`resolve_conflict` 的本地分支是**明确 501**（不是假装成功）；
> `import_asset` 只支持 texture 一条路径。它们算「有路由」，但能力受限。

---

## 4. 三类缺口与归因

### 4.1 本地反向多出（本次已删 6 条）

审计时本地有 6 条 `/api/tools/<name>` 分支，而服务端注册表里**没有**这个名字：

| 工具 | 本地原行为 | 前端调用者 | 处理 |
|---|---|---|---|
| `update_selection` | `p2NoServerEndpoint` 501 | 无 | **删除**（通用兜底已回 501） |
| `create_object` | `p2NoServerEndpoint` 501 | 无 | **删除**（对象由 draw_*/import_image 创建） |
| `delete_group` | `p2NoServerEndpoint` 501 | 无 | **删除**（组是对象，用 `delete_object`） |
| `heal` | `heal_stamp` 的别名 | 无 | **删除**（服务端没有 `heal` 这个名字） |
| `cache_document_preview` | 真实现：整幅渲染 + 写快照 | 无 | **删除**（服务端只在**内部**调 `workspace.cache_document_preview`，不是注册工具；见 `tools_io.rs` 的调用） |
| `push` | `p2NoServerEndpoint` 501 | 无 | **删除**（dispatch 表没有这一项） |

**依据**：`grep -n "callTool(\"<name>\"\|\"<name>\"" web/viewer-app.js` 对这 6 个全是 0。
**规则**：本地反向多出必须为空；真要保留，必须进 `scripts/three-way-alignment-allowlist.txt`
并在同一行写原因（判据会拒绝「只有名字没有原因」）。

`undo`／`redo`／`list_brushes` **不算多出**：它们是同一实现的**另一个请求名**
（`localDispatch` 开头把 `/api/tools/undo` 归一成 `undo_last`，`list_brushes` 归一成
`/list_brushes`），判据把它们记作别名，不记作新工具。

### 4.2 缺本地路由（59 条）

**结论：只有 1 条是「本地已有等价能力、只差一个入口」——已经补掉；其余 58 条都不是别名。**
完整归因见 §3 表 A 的「归因 / 下一步」两列。按大类汇总：

| 类 | 数量 | 代表 | 为什么不能只加一个路由 |
|---|---|---|---|
| **已补（纯前端）** | 1 | `put_blob` | 本地已有 `/api/blob` 的同一条 `blob_put` + `store.putBlob` 链路；本次把它接到 `/api/tools/put_blob` |
| **K：必须内核渲染/像素/编解码** | 37 | `smart_*`、`bake_*`、`swatch_brush_strokes`、`export_timelapse` | PWA 没有对应内核入口（或服务端用 ffmpeg/PSD 编码器）；在 JS 重写等于第二套实现 |
| **F：先补折叠 parity** | 6 | `scale_canvas`、`rotate_canvas`、`flip_canvas`、`crop_canvas`、`extend_canvas`、`resize_canvas_bounds` | `yanshi-core` 有这些原子，但本地 `p2cDocSize`／`p2cState` 只读 `create_document`；直接写原子会让面板尺寸错 |
| **N：需试画状态机** | 6 | `try_stroke`／`commit_stroke`／`discard_stroke`／`try_batch`／`commit_batch`／`list_attempts` | 本地没有 attempt 表；不是别名，是新持久化机制 |
| **P：可移植一块纯 JS** | 4 | `import_psd_layers`／`export_psd_layers`、`transform_selection`、`verify_package` | 收益低于成本或依赖大块算法；已列入排期而不是「差一个别名」 |
| **G：需扩生成器** | 1 | `explain_tool` | 生成块只有参数名/类型/必填，没有 summary/doc；半份镜像会误导 |
| **M：明确不做** | 5 | `list_fonts`、`memory_status`、`release_document`、`reopen_document`、`revoke_token` | 无对应进程/浏览器能力；镜像会撒谎 |

> **计数**：37 K + 6 F + 6 N + 4 P + 1 G + 5 M = **59**（与 §3 表 A 的 59 行一一对应；
> `put_blob` 已不在缺路由里，所以不计入这 59）。

**本次实际补掉的工具**：

- `put_blob`：在 `localDispatch` 里加 `/api/tools/put_blob`，复用已有的内核
  `blob_put`（内容寻址）+ `store.putBlob`（IndexedDB 持久化）。
  接受 `data_base64` 或 http(s) 的 `path`；`path` 与 `data_base64` 互斥（照服务端）；
  浏览器读不到的裸路径／`file://` **明确 400 说清**，不静默降级。
  返回字段照服务端：`{ok, blob_hash, size, mime_type, source}`。

### 4.3 声明参数本地未读（66 条 / 22 个工具）

判据口径：生成块里声明了某参数，而该名字在生成块之外**一次都不出现** ⇒ 本地一定没读
（该文件没有 `body[k]` 反射循环、没有 `...body` 转发）。**这是单向判据**：出现 > 0 次不代表读了
（通用名会被别处掩盖，例如 `width`/`size`）。

| 工具 | 缺 | 未读参数 | 处理 |
|---|---|---|---|
| `brush_stroke` | 22 | `brush_params`、`hand_tremor`、`color_breath`、`paint_push`、`dry_brush`、`edge_soft`、`tilt_shade`、`scatter_texture`、`edge`、`path_guide`、`wait_for_render`、`sensors`、`anchor_mode`、`grid_spacing`、`humanize`、`pressure_curve`、`dwell`、`tremor`、`tremor_seed`、`glaze`、`glaze_passes`、`clip_to_selection` | **已知缺口**：本地落笔走内核默认路径，不转发这些笔触参数；不是「服务端已读」的证明 |
| `render_region` | 6 | `output_path`、`show_coords`、`include_image`、`max_px`、`active_layer`、`return_image` | 服务端落盘/标注选项；本地只接 bbox |
| `batch` | 5 | `preview_every_n_strokes`、`preview_interval_ms`、`silent`、`slim_receipt`、`wait_for_render` | 服务端批处理选项；本地 `batch` 只逐条重放 |
| `brush_preview` | 5 | `brush_params`、`include_image`、`output_path`、`return_image`、`max_px` | 同上 |
| `resolve_conflict` | 4 | `resolution`、`conflict_layer_id`、`formal_layer_id`、`opponent_atom_id` | 本地是明确 501，参数无从读起 |
| `export_png` | 4 | `lod`、`deterministic`、`return_image`、`max_px` | 本地只有内核 `render_region_png`（bbox）；缩放/确定性由服务端链路做 |
| `path_edit` | 3 | `other_id`、`right_id`、`result_id` | 本地只按 `path_id` 改，不生成新 id |
| `collect_diagnostics` | 2 | `include_thumbnail`、`max_bytes` | 服务端诊断选项；本地诊断不产缩略图 |
| `export_project` | 2 | `include_bitmaps`、`blob_bundle_mode` | 本地打 JSON 包，不嵌服务端 blob bundle |
| `apply_stash` | 1 | `stash_id` | 本地 501（没有 stash 机制） |
| `discard_stash` | 1 | `stash_id` | 同上 |
| `collect_garbage` | 1 | `ttl_seconds` | 服务端 GC 语义；本地按孤儿扫描 |
| `create_mask` | 1 | `from_selection` | 本地只接 shape/from_* 的另一条 |
| `create_selection` | 1 | `from_mask` | 同上 |
| `get_document` | 1 | `preview_size` | 本地返回固定预览；档位未接 |
| `get_state` | 1 | `preview_size` | 同上 |
| `import_asset` | 1 | `overwrite` | 本地只支持 URL/base64 → blob；overwrite 是服务端资产库语义 |
| `import_image` | 1 | `text_provenance` | 本地 `import_image` 不写文本溯源对象 |
| `import_project` | 1 | `missing_blob_policy` | 本地只入库 blob/原子 |
| `scatter_strokes` | 1 | `clip_to_selection` | 本地未接选区裁剪 |
| `set_brush_dynamics` | 1 | `curve` | 本地 501 |
| `smudge` | 1 | `deposit` | 本地涂抹参数未接 |

**下一步**：
1. **不为了降数而假读**。这些参数要么真的接进本地语义，要么保持登记（`scripts/pwa-local-ignored-params.txt`）。
2. **优先做便宜的三条**（如果后续有需求）：`batch.silent`／`slim_receipt`（本地可轻松照做）、
   `get_document.preview_size`／`get_state.preview_size`（本地可返回不同档位）、
   `export_project.include_bitmaps`（本地包已能带 blob）。
3. `brush_stroke` 的 22 个是**最大的一块**，但它属于「本地内核默认路径」的能力边界，
   不是漏读 —— 真正接它们要改本地落笔链路，不能靠改判据解决。

---

## 5. 下一步动作（按优先级）

| 优先级 | 动作 | 对象 | 验收 |
|---|---|---|---|
| **P0** | 已做：补 `put_blob`；删 6 条反向多出 | `api-local.js` | `node scripts/check-three-way-alignment.mjs` 绿；`cmp` = 0 |
| **P1** | 修 `list_brushes` 的空表：从 `/assets-index.json` 取真实笔刷清单，或明确 501 | `api-local.js` | 前端不再把「空表」当「没有笔刷」 |
| **P1** | 把 §3 的 59 条逐行归因落成 issue/任务；K 类默认「不做，走服务端」，F/N/P/G 类排期 | 本文 §3 | 每条有 owner 或「不做」理由 |
| **P2** | canvas 6 条（F）：先补 `p2cDocSize`／`p2cState` 对 canvas 原子的折叠，再加路由 | `api-local.js` | 新判据：本地 `scale_canvas` 后 `/api/tools/get_document` 的宽高 == 服务端同输入 |
| **P2** | 试画族 6 条（N）：IndexedDB 加 attempt 表，`try_*` 只写表不写文档 | `api-local.js` | 试画不入原子日志；`commit_*` 后才可见 |
| **P2** | `explain_tool`（G）：生成器加 `summary`/`doc` 字段，或明确「本地只答参数，不答说明」 | `scripts/gen-pwa-tool-specs.mjs` + `api-local.js` | 本地响应字段与服务端同名同义，缺的字段显式说缺 |
| **P3** | 参数未读 66 条：按 §4.3 的「便宜三条」先做，其余保持白名单 | `api-local.js` + `pwa-local-ignored-params.txt` | 棘轮只降不涨 |

---

## 6. 验证记录（2026-11-04 实跑）

```text
$ node scripts/check-three-way-alignment.mjs; echo "EXIT=$?"
...
✓ 三方对齐在棘轮内（缺口只许降）
EXIT=0

$ cmp crates/yanshi-http/assets/api-local.js web/api-local.js; echo "CMP=$?"
CMP=0
```

**变异验证（证明尺子能红）**：把 `MISSING_CEILING` 从 59 临时改成 58：

```text
$ node scripts/check-three-way-alignment.mjs; echo "EXIT=$?"
✗ 缺本地路由 59 ≤ 棘轮 58（涨了 1 个 ⇒ 要么补路由，要么抬常量并写理由）
✗ 三方对齐判据红 1 条
EXIT=1
```

还原为 59 后重跑 ⇒ `EXIT=0`（见上）。两次退出码：**变异 1 / 还原 0**。

---

## 7. 维护规则

1. **不要往 `scripts/criteria-known-red.txt` 加条目来逃避这两个棘轮**。红就是红：
   要么补，要么抬常量并写理由。
2. **改一份 `api-local.js` 必须镜像另一份**；判据会逐字节比较，分叉直接红。
3. **新增工具**：注册表加条目后，生成器会更新 `P2_TOOL_SPECS`；如果本地不接，
   缺路由计数会 +1 ⇒ 棘轮红 ⇒ 必须在本文 §3 登记归因，或补路由。
4. **新增本地专属路由**（服务端没有的工具）：必须进
   `scripts/three-way-alignment-allowlist.txt` 并写原因；否则判据红。
5. **本文的计数是快照**。报数用命令，不要抄表；表只用来回答「哪个工具、什么归因、下一步是什么」。
