> 仓库内冻结副本，源文档：`yanshi.md`

# 偃师 Yanshi 设计文档

**AI 原生协作绘画与设计引擎**

**版本：v1.0-draft4（冻结候选版）**

> 版本记录：
> - draft1：初版愿景文档。
> - draft2：折叠语义形式化、declare_head、Blob CAS、错误协议、ULID 幂等、D0/D1/D2、view/edit 双模式、Phase 0。
> - draft3：冲突方案 d、Job 协议、渲染两层拆分、结构 dirty、控制流/数据流分离、WASM 内存管理、Stash、import_image、CI benchmark。
> - draft4（本版）：Blob 三级生命周期与提交顺序协议（修复 GC 与可回放性矛盾）、广播边界精确化、resolve_conflict 组合宏化、state@seq 公式定义、工具暴露分层、外部服务默认部署形态、快照移出原子日志、Job TTL/取消、GPU 章节同步两层架构、风险表清理。**本版为冻结候选，下一步为规范→属性测试代码翻译 + Phase 0。**

## 目录

1. 概述与定位
2. 设计原则
3. 总体架构
4. 核心数据模型
5. 原子日志与折叠求值
6. 渲染与性能架构
7. 缩略图
8. 局部渲染与局部缩略图
9. 对象复制、实例化与组引用
10. 工具协议与 API
11. 笔刷与风格
12. 协作与并发控制
13. Web 编辑器
14. 性能与资源
15. 插件与外部服务
16. 选型参考
17. 远期规划与可能
18. 部署
19. MVP 路线图
20. 风险与对策
21. 总结

---

## 1. 概述与定位

偃师是一个 AI 原生、人机协作的绘画与设计引擎。它用文档状态、语义命令、可编辑对象、多级预览、版本日志和协作机制，替代传统 GUI 的面板加鼠标轨迹。

核心目标：

- AI 用起来简单、自然，符合 LLM 特点。
- 人类可实时介入、修改、评论。
- 支持光栅绘画和基础矢量设计。
- 支持基础修图。
- 所有操作原子化、可追溯、可撤销。
- 渲染性能与历史长度解耦。
- 复制、实例化、组引用建立关系网络，数据共享。
- 人类和 AI 功能对等、**语义分层**：人类使用微观工具（`draw_stroke`、`liquify`），AI 使用宏观语义工具（`inpaint_region`、`semantic_replace`），底层对象模型和原子类型统一。
- **暴露分层**：工具集按核心/扩展分组注册，控制 Agent 的工具选择负担与 token 开销（见第 10.2 节）。

---

## 2. 设计原则

1. **AI-first**：工具少、参数扁平、默认合理、返回结构化状态和预览。工具暴露按核心/扩展分层（第 10.2 节）。
2. **Append-Only 权威**：原子只追加，不删除、不修改。服务端原子日志是唯一权威排序与状态来源。
3. **原子不可变**：原子内容哈希固定。
4. **状态可折叠**：当前状态 = 折叠原子日志的结果。折叠规则形式化定义（第 5 章）。
5. **打开即图片**：打开文档直接加载最新渲染输出，不重放历史。
6. **非破坏性**：笔触、滤镜、变换、文本、调整、修图都是可编辑对象。
7. **确定性**：随机纹理、抖动、物理模拟由 seed 控制，可重放。确定性等级 D0/D1/D2（第 6.1 节）。
8. **有状态**：文档在服务端维护，AI 不需要反复上传整张图。
9. **批处理**：batch 是逻辑分组，支持整体撤销，也支持对 batch 内单个或部分原子单独撤销或修改。
10. **可观察**：操作返回缩略图 URL、区域预览、结构化错误。
11. **可协作**：多会话可同时工作，服务端串行化排序。
12. **无头优先**：不依赖 GUI。
13. **可扩展**：笔刷、滤镜通过 WASM 插件扩展。
14. **修图与绘画统一**：修图是同一套对象模型上的操作。
15. **关系优先于拷贝**：复制是建立引用。
16. **人和 AI 功能对等、语义分层**：底层对象模型和原子类型统一；人类用微观工具，AI 用宏观语义工具，也可调用微观工具。
17. **回放是历史的可视化**：原子日志本身就是回放数据源。
18. **标注是协作的入口**：人类用画笔、框选、批注告诉 AI 改哪里、怎么改。
19. **渲染计算内核单一来源**：客户端与服务端共享同一套 Rust 计算内核代码，D0 bit-exact。GPU 加速仅作用于合成后端层（第 6.1 节）。
20. **大二进制外置**：原子只存元数据和引用，位图数据存入内容寻址 blob 存储。
21. **历史完整性**：原子与其引用的 blob 同属历史资产。GC 根集 = 全日志引用闭包，只清理从未进入日志的孤儿。**状态可达性不等于历史可达性，GC 决不以后者为代价**（第 6.3 节）。
22. **冲突不自动合并**：引擎只报告冲突，不负责解决语义冲突。自动合并仅限生成性叠加（LWW），采样性替换冲突走拒绝 + 重提交（第 12.3 节）。

---

## 3. 总体架构

```
┌─────────────────────────────────────────────┐
│ 客户端：MCP Agent / CLI / Web 编辑器 / SDK   │
│  ├─ WASM 计算内核层（与服务端共享 Rust 代码） │
│  ├─ 合成后端层（WebGPU / WebGL2 / CPU）      │
│  ├─ 本地乐观渲染                              │
│  └─ 原子异步提交（blob 先行，原子后行）       │
└──────────────────┬──────────────────────────┘
                   │ MCP stdio / HTTP / WebSocket
┌──────────────────▼──────────────────────────┐
│ 偃师 Server                                 │
│  ├─ 工具协议层：语义命令、batch、job、分层   │
│  ├─ 文档服务：图层、对象、选区、蒙版          │
│  ├─ 修图服务：调整、基础修补、基础液化        │
│  ├─ 协作服务：原子排序、评论、推送            │
│  ├─ 渲染服务：Tile 渲染、预览、导出           │
│  ├─ 标注服务：区域、对象、批注（独立通道）    │
│  ├─ Blob 存储：CAS，三级生命周期，惰性 GC     │
│  └─ 插件层：WASM 笔刷、滤镜                  │
└──────────────────┬──────────────────────────┘
                   │
┌──────────────────▼──────────────────────────┐
│ 核心引擎：Rust，无 GUI，确定性渲染            │
│ 输出 PNG / WebP / 精灵图                     │
└─────────────────────────────────────────────┘
```

**渲染核心两层架构**：

| 层级 | 职责 | 确定性 | 代码来源 |
|---|---|---|---|
| **计算内核层** | 笔触 stamping、混合、滤镜、液化求解、折叠求值 | D0 bit-exact | 纯 Rust，无 GPU 依赖，客户端/服务端共享编译 |
| **合成后端层** | tile 上传、显示合成、blit | D1 容差 | 服务端可选 GPU，客户端 WebGPU→WebGL2→CPU 三级降级 |

“单一来源”精确指代**计算内核层**。GPU 加速初期只做缓存上传与显示合成；任何计算内核的 GPU 版本属于远期项，且必须先通过 D1 容差测试才可启用。

**客户端本地乐观渲染**：

- 人类操作在客户端 WASM 计算内核层上立即绘制，不等待服务端。
- 同时异步提交原子（blob 先传，原子后提交，见 6.3）。
- 服务端确认后，以服务端状态为准做校正，校正使用 tile cross-fade 平滑过渡。
- 其他客户端的原子通过 WebSocket 全局广播（元数据），客户端增量折叠并渲染。

**协议**：

- MCP stdio：单 Agent 本地或管道调用。本地 stdio 豁免鉴权。
- HTTP：无状态查询、批量提交、图像下载。走 capability token。
- WebSocket：实时协作推送、原子广播、视口订阅。走 capability token。

---

## 4. 核心数据模型

### 4.1 文档与图层

```
Document
  id, width, height, color_space, background
  layers: [Layer]
  selection, masks
  history: OperationLog
  head: atom_id
  medium, style

Layer
  id, name, type: raster | vector | layer_group | mask
  parent_id, z_index
  blend_mode, opacity, visible, locked, alpha_lock, clipping_mask
  mask_id, transform, thumbnail
  medium, style
```

**图层只做容器**。调整、滤镜、文本、修图全部对象化（4.2）。Photoshop 式“调整层”心智在 UI 层用“对象 + 独占显示行”的视图映射实现，不进数据模型。

`Layer.objects` 为派生索引，权威引用是 `Object.layer_id`。

### 4.2 对象基类

```
Object
  id, layer_id, type, z_index, visible, locked, metadata
  transform: {matrix, pivot}
  created_by: atom_id
  deleted_by: atom_id?
  versions: [atom_id]
  current_version: atom_id
  style: StyleRef?
```

对象类型：

- **Stroke**：笔触，含 points、pressure_curve、dynamics、seed。points 超过 4KB 走 Blob CAS。
- **Shape**：形状，含 geometry、style。
- **Text**：文本，含 text、font、size、color、align。
- **Adjustment**：调整，含 adjustment_type、params、mask_id。
- **Filter**：滤镜，含 filter_name、params、mask_id。
- **RasterPatch**：AI 语义工具输出，含 blob_ref、生成参数、prompt 记录。重跑 = supersede（新 blob）。
- **Retouch**：修图对象，含 retouch_type、source_region、source_state_version、target_region。
- **Liquify**：液化，含 mesh、strength、mode、source_state_version。
- **Instance**：实例，含 master_ref、override、sync_policy。
- **Group**：对象组，含 members、group_transform、override、sync_policy。

**AI 宏观语义工具的输出强制封装为独立对象**（RasterPatch 或带 Clipping Mask 的对象组），确保非破坏性。人类后续修改作用于该独立对象，AI 原始生成物保持纯净，可通过隐藏/删除该对象撤销 AI 的介入。

### 4.3 组与引用

```
Group
  id, name
  members: [ObjectRef]
  group_transform: matrix
  override: Override?
  sync_policy: SyncPolicy?
  parent_group_id: string?
  metadata

ObjectRef
  object_id, layer_id
  local_transform: matrix
  override: Override?
  sync_policy: SyncPolicy?

Instance < Object
  master_ref: ObjectRef | GroupRef
  override: Override
  sync_policy: SyncPolicy

GroupRef
  group_id: string

Override
  transform, color, opacity, visible, blend_mode, size
  properties: {key: value}
  members: {object_id: Override}

SyncPolicy
  mode: "all" | "none" | "custom"
  sync_geometry, sync_appearance, sync_transform, sync_effects, sync_metadata
  custom_fields: [string]

StyleRef
  style_id: string
  overrides: {key: value}?
```

`Layer.type = layer_group` 是图层层级结构，`Group` 是对象级引用组，两者语义不同，独立管理。

### 4.4 选区与蒙版

```
Selection / Mask
  id, shape, feather, mode, invert, linked_layer, refined_edges
```

基础选区类型：矩形、椭圆、套索、多边形、魔棒、按颜色、画笔蒙版。语义选区（`select_subject`、`select_sky`、`select_face`）通过外部模型服务实现。

### 4.5 检查点、快照与 HEAD 指针

```
Checkpoint
  id, name, created_by, message
  anchor_seq: int          // 创建时的 head seq
  snapshot_id: string?     // 可选关联快照，保证 O(1) 恢复

Snapshot
  id: string
  base_ref: {type: declare_head | snapshot, id: string}   // 统一引用类型
  seq_at: int              // 锚定 seq（按 5.5 公式求值）
  state: object            // 逻辑状态
  manifest: [blob_hash]    // 活跃 blob 清单（GC 用，见 6.3）
  crc: string

declare_head
  base: atom_id | checkpoint_id
  reason: string?

Changeset
  id, name, actor, timestamp
  atoms: [atom_id]
  message
```

三概念关系：

- **Snapshot**：系统性能机制，旁路缓存，**不是原子**，不进入日志（见 5.2 注）。锚定 `state@seq_n`（求值公式见 5.5），带 CRC。
- **Checkpoint**：用户语义，轻量元数据原子（`create_checkpoint`），锚定 `anchor_seq`，可选关联快照。**用户显式创建的 checkpoint 其关联快照不参与 LRU 清理**，保证时间旅行可用性。
- **declare_head**：求值起点跳变原子，base 引用 atom_id 或 checkpoint_id，是 `revert_to` 和 `restore_checkpoint` 的统一实现。

快照链（`base_ref`）最终追溯到某个 declare_head；快照可以基于前一个快照增量存储。

### 4.6 标注

标注走独立 append-only 通道，不进入主原子日志，不参与折叠。

```
Annotation
  id, doc_id, actor, head_seq
  type: region | object | arrow | text | doodle | highlight
  target: Region | ObjectRef
  content: string | path
  intent: modify | add | remove | replace | style | move | resize | color
  status: pending | resolved | rejected
  created_at, updated_at
  suggestion_id: string?
  resolved_by: atom_id?
  resolved_at: int?
```

---

## 5. 原子日志与折叠求值

### 5.1 原子结构

```
Atom
  id: string              // 客户端生成 ULID，服务端按 id 去重
  seq: int                // 服务端权威序号，全序排序依据
  parents: [atom_id]      // 因果记录，非排序依据
  kind: string
  actor: string
  session: string
  timestamp: int
  message: string?
  changeset_id: string?
  schema_version: int
  plugin_id: string?
  plugin_version: string?
  payload: object         // 大二进制外置，只存 blob 引用
  refs: object
```

**seq 与 parents**：

- `seq` 是折叠的唯一排序依据（全序）。
- `parents` 记录因果信息，用于审计和冲突检测。离线重连时，客户端原子的 `parents` 指向其所见 head，与服务端实际插入位置可能不同。
- 原子 `id` 由客户端生成（ULID），服务端按 id 去重，获得幂等与重试安全。

**插件版本锁定**：插件笔触进入日志后，`plugin_id + plugin_version` 随原子记录。插件升级不自动改变旧文档渲染，需显式迁移原子。

### 5.2 原子类型

- **创建类**：create_document, create_layer, create_object, create_selection, create_mask, create_style, create_checkpoint
- **绘制类**：draw_stroke, fill, draw_shape, draw_text, erase, retouch, liquify
- **修改类**：supersede, move, transform, set_property
- **删除类**：tombstone
- **历史类**：revert, reapply, checkpoint, tag, declare_head
- **协作类**：comment, suggest, accept_suggestion, reject_suggestion

> **注：快照不是原子。** 快照是 L1 旁路缓存（6.4/6.5），不进入日志、不参与折叠。日志中不记录快照事件；快照通过 seq 锚定与 CRC 自校验。`resolve_conflict` 不是新原子类型，是已有原子的组合宏（12.3）。

### 5.3 折叠求值模型

折叠是将原子日志按 `seq` 线性扫描，维护每个对象的有效原子链，生成当前状态的过程。

**求值规则**：

```
fold(atoms, base_state):
  state = base_state
  for atom in atoms sorted by seq:
    if ∃ 有效 revert 原子 r: r.target == atom.id and r.seq > atom.seq:
      continue
    if atom.precondition not satisfied(state):
      record_warning(atom, "cascade_invalidation")
      continue
    state = apply(state, atom)
  return state
```

> 说明：进入日志的原子均已通过提交时校验（12.2），因此**折叠期 precondition 失败一律来自级联失效**，记录警告后跳过，不产生状态效果。折叠器不需要、也不持有“提交期校验状态”信息（客户端折叠器尤其如此）。

**有效集与失效传播**：

- 每个对象有一个**有效原子链**：从 `create_object` 或 `create_layer` 开始，经过所有未被 revert 的 `supersede`、`move`、`transform`、`set_property`，直到被 `tombstone` 终止。
- `revert` 原子将目标原子移出有效集。
- **级联失效**：若被撤销的原子被后续原子的 precondition 依赖，所有依赖它的后续原子也失效。折叠时跳过并记录警告。
- **reapply 决策**：reapply 只恢复目标原子，**不恢复级联链**。级联链中未被独立 revert 的原子需显式重新提交。UI 在 reapply 时提示“以下 N 个编辑不会被恢复”。

**并发求值规则（LWW）**：

- 同一对象的并发 `supersede`：按 seq 后者覆盖前者，折叠取最后有效版本。
- 并发 `reorder_layers`：`reorder_layers` 原子携带**完整目标 z 序**（绝对序快照）；并发时按 seq 取最后有效原子为准。
- `z_index`、`visible`、图层归属等属性冲突：同样按 seq LWW，最后有效原子胜出。

**折叠代数不变量**（属性测试目标）：

- **幂等性**：`fold(atoms) == fold(atoms)`。
- **收敛性**：不同起点到达相同 head 的结果一致。
- **无孤儿引用**：折叠结果中不存在指向已失效原子的引用。
- **revert-reapply 往返**：`revert(x)` 后 `reapply(x)` 恢复 `x` 的效果（不恢复级联链）。
- **历史可重放**：任意 `state@seq_n`（5.5 公式）在日志与 blob 完整时均可重放，GC 不破坏此性质（原则 21）。

### 5.4 删除、撤销、修改

- 删除 = tombstone 原子，不物理移除。
- 撤销 = revert 原子，不删除原原子。跨 actor revert 默认不允许，需 owner 权限。
- 重做 = reapply 原子。
- 修改 = supersede 原子，产生对象新版本。

**revert 合法目标**：限定为有状态效果的原子类别（create_\*, draw_\*, fill, erase, retouch, liquify, supersede, move, transform, set_property, tombstone）。不能 revert comment、suggest 等协作原子。

`revert(revert(x))` 定义为等价 `reapply(x)`，但保留独立记录。

**两类 precondition 失败**：

| 类型 | 时机 | 处理 |
|---|---|---|
| 提交时校验失败 | 原子提交到服务端时 | 拒绝，不进日志，返回 `precondition_failed` |
| 级联失效失败 | 折叠时发现依赖已被 revert | 原子已在日志中；折叠时跳过，记录警告，不产生状态效果 |

### 5.5 HEAD 指针与 state@seq 定义

`declare_head` 将文档当前求值起点设置为指定 checkpoint 或原子。日志保持 append-only，历史不重写。

**state@seq_n 求值公式**：

```
state@seq_n := fold(A_n, base_state(H_n))

  H_n   := seq ≤ n 中最近一次 declare_head 原子（若存在）
  A_n   := 满足 H_n.seq < seq ≤ n 的全部原子
  base_state(H) := H.base 指向的状态：
      - atom_id      → 该原子时刻的 state@seq（递归定义，seq 严格递减，必然终止）
      - checkpoint_id → checkpoint.anchor_seq 对应的 state@seq（或其关联快照）
  若 H_n 不存在：base_state := 空白初始文档，A_n := seq ≤ n 的全部原子
```

- 当前 HEAD = `state@seq_head`。
- 快照生成时按此公式求值并锚定 `seq_at`。
- **时间旅行到任意原子 a**：显示 `state@seq_a`。求值起点是 seq ≤ a 之前最近的 declare_head。
- **declare_head 是重型原子**：提交后立即触发快照 + 全量 tile 失效（6.5）。求值起点跳变意味着增量折叠的 HEAD 与 base 状态可能全图不同。

`revert_to` 和 `restore_checkpoint` 统一通过 `declare_head` 实现。

### 5.6 变更集

- 变更集是一组原子的逻辑分组。
- 撤销默认按变更集粒度，也支持按单个原子粒度。
- 一个 batch 通常对应一个变更集，batch 内的原子仍可独立操作。

### 5.7 错误协议

所有 API 返回统一错误 schema：

```json
{
  "ok": false,
  "error_code": "reference_not_found | invalid_argument | precondition_failed | conflict | permission_denied | resource_exhausted | degraded | job_pending | job_not_found",
  "retryable": true,
  "context": {
    "atom_id": "sha256:...",
    "object_id": "...",
    "layer_id": "...",
    "blob_hash": "sha256:...",
    "detail": "对象 S 在当前 HEAD 中不存在"
  }
}
```

`conflict` 错误仅用于采样性替换操作（retouch、inpaint、heal、patch）的并发互踩。生成性叠加（draw_stroke 等）默认 LWW 自然叠加，不报 `conflict`。

---

## 6. 渲染与性能架构

### 6.1 确定性等级

跨平台 bit-exact 渲染在 GPU 浮点下不可行。定义三级确定性：

| 等级 | 范围 | 要求 |
|---|---|---|
| **D0** | 计算内核层 CPU 软实现：折叠求值、混合、滤镜、液化求解、笔触 stamping | bit-exact，唯一权威基线 |
| **D1** | 合成后端层、预览、缩略图 | 视觉一致，允许 ±1 LSB |
| **D2** | 插件、外部服务 | 允许差异 |

黄金测试策略：CPU 路径 bit-exact 基线 + GPU 路径容差比对，比较最终输出。

**色彩空间**：合成在线性光空间进行，输出转换到目标色彩空间。alpha 使用预乘。此定义进入 D0 基线。

**Tile 存储格式**：内存 tile 用 f16 线性（合成正确性），持久缓存与网络传输用 u8/WebP（显示空间）。黄金测试比较最终输出。

### 6.2 核心原则

历史是磁盘上的资产，状态是内存里的缓存。渲染只看状态，不看历史。打开文档是读取渲染状态，不是重放历史。

- O(dirty) 渲染。
- O(state) 折叠，从最近快照或 `declare_head` 开始，普通原子重放上限 1000，重型原子强制快照。
- 服务端维护对象索引树和活跃图层的 Tile 缓存，不要求全图 HEAD 常驻。
- 客户端 WASM 计算内核层维护本地副本。
- 快照隔离、缓存分层、异步渲染、冷热分离、压缩不删原子。

### 6.3 Blob 存储、提交协议与生命周期

大二进制数据（画笔蒙版、纹理、retouch 采样 patch、Stroke points、RasterPatch 位图）存入**内容寻址存储（CAS）**，原子只存 `{blob_hash, size, mime_type}` 引用。

- 超过 4KB 的 payload 走 CAS。
- 同哈希天然去重。
- 原子保持小而可 JSON 索引。
- 存储后端：本地文件系统 CAS，hash 分桶路径。
- 并发写入安全：先写 `.tmp` 再 `rename`。

**Blob 提交顺序协议**（保证日志无悬空引用）：

```
1. 客户端 PUT blob（CAS 去重；.tmp + rename 原子写入）
2. 客户端提交原子（payload 引用 blob_hash）
3. 服务端校验原子引用的所有 blob 已存在；缺失 → reference_not_found 拒绝
4. 原子追加日志，WS 全局广播原子元数据
5. 其他客户端按需拉取 blob（此刻必然已存在）
```

离线队列保持相同顺序；Stash 打包原子及其 blob 引用一起保存（12.4）。

**Blob 三级生命周期**（原则 21 的落地）：

| 级别 | 定义 | 存储 | GC |
|---|---|---|---|
| **活跃** | 当前 HEAD 折叠状态引用的 blob | 热存储（内存/SSD） | 保留 |
| **历史** | 被日志中任何原子引用、但不在当前折叠状态（revert 中、被 declare_head 甩出求值范围、Stash 中、旧分支） | 冷归档 + zstd | 保留，可按需取回 |
| **孤儿** | 上传成功但从未被任何原子引用（提交失败/中断） | 临时区 | TTL 7 天后清理 |

- **GC 根集 = 全日志原子引用闭包**。GC 永不删除被任何日志原子引用的 blob——删 blob 等于部分删除原子，违反原则 2/21。
- 快照生成时顺带产出**活跃 Manifest**（当前折叠状态的 blob 清单，存入 Snapshot.manifest）。Manifest 的角色是**活跃集标记与冷热迁移依据**，不是 GC 根集。
- 后台任务：活跃→历史降冷迁移（不在最新 Manifest 但被日志引用）；孤儿超 TTL 清理。
- reapply / 时间旅行 / Stash 重放需要历史 blob 时从归档层取回（慢路径，延迟预算另计，14.5）。
- **可观测性**：记录每级 blob 数量与体积、降冷迁移量、孤儿清理量。

### 6.4 分层缓存

| 层级 | 内容 | 存储 |
|---|---|---|
| L0 | 原子日志 | 磁盘，append-only |
| L1 | 快照（旁路缓存，非原子） | 磁盘/内存，锚定 `state@seq_n`，带 CRC |
| L2 | 文档状态 | 内存，对象索引 + 活跃图层元数据 |
| L3 | Tile 缓存 | 内存/GPU，256×256 或 512×512，**唯一物化形式** |
| L4 | 缩略图缓存 | 磁盘/内存 |

图层级操作从 tiles 拼装，不存在独立的图层全量位图缓存。

### 6.5 快照策略

- 条件触发：落后 ≥ 1000 原子，或 ≥ 30s 且有新原子，或重型原子提交。
- 重型原子清单：retouch、liquify、declare_head、大尺寸滤镜、RasterPatch、任何导致结构 dirty 的原子。
- 快照按 5.5 公式求值并锚定 `seq_at`，写入带 CRC 校验头。
- 若最近快照不可用（CRC 失败），回退更早快照，重放不设 1000 上限但记录恢复时间。
- **快照缓存可 LRU 清理，例外**：用户显式 checkpoint 关联的快照（4.5）不清理；最近 N 个 declare_head 的 base 状态快照保留。
- 打开文档时，服务端直接返回 HEAD 渲染缓存。

### 6.6 增量折叠与失效传播

定义两类失效传播：

| 类型 | 触发 | 传播范围 |
|---|---|---|
| **几何 dirty** | draw_stroke、fill、move 等 | 受影响对象的 bbox 并集 |
| **结构 dirty** | revert、supersede、tombstone、declare_head、z_index、visible、图层归属变更 | 依赖图传播闭包中所有对象的效果 bbox 并集 |

结构 dirty 的传播闭包包括实例引用 master 的传播。属性变更（z_index、visible、归属）不对应几何 bbox，必须通过依赖图计算受影响对象集，再取其 bbox 并集标记 tile 失效。

- 新原子到达只更新受影响部分。
- 计算 dirty_bbox 和结构 dirty 集，标记 Tile 失效。
- 异步触发渲染。
- 打开文档不触发折叠。
- 客户端 WASM 计算内核层执行同样的增量折叠逻辑。

### 6.7 异步渲染与 Job 协议

- 原子提交立即返回，渲染异步。
- 渲染队列：高（当前可见）、中（AI 请求）、低（预计算）。
- 批量合并，去重，取消过期任务。
- 人类交互走客户端 WASM 本地乐观渲染。
- 打开文档不触发渲染。

**Job 协议**（统一异步操作模型）：

微观工具的 `wait_for_render` 参数：

- 默认 `true`，短超时 500ms。
- 超时返回 `job_pending` + `job_id`。
- Agent 通过 `get_job(job_id)` 轮询。
- **它等的是哪一次渲染**（性能专题澄清）：**这一笔改动的脏区**那一次。
  返回时保证 `render_status.rendered == true`、`job_status == committed`，
  响应里的 `preview` 就是该脏区的真实渲染。
- **它不等文档级缩略图**：那张 256² 图是一份**缓存**，改为**按需**产生 ——
  落后时 `get_document` / `GET /api/documents/<id>/preview` 会**当场重建**（绝不拿旧图冒充 HEAD），
  而在被请求之前 `get_render_status.thumbnail_current` 为 `false`（滞后可见）。
  8K 实测：默认单笔 39.5 s（那次重算全在提交收尾）vs `wait_for_render=false` 95.1 ms，
  两者**逐像素一致** —— 那一次渲染本来就不是调用方要的东西。

Job 对象：

```
Job
  id, kind, session, created_at
  status: submitted | running | committed | failed | cancelled | expired
  atom_id?, error?
  ttl: 默认 300s（语义工具外部调用较长），超时 → expired
```

轮询工具：

- `get_job(job_id)` → 状态机 + 结果
- `get_render_status(atom_id)` → `{rendered: bool, thumb_url?}`
- `cancel_job(job_id)` → 取消运行中的 job

语义工具（inpaint_region 等）返回 `job_id`。job 状态机：`submitted → running → committed(atom_id) / failed(error_code) / cancelled / expired`。外部服务挂死由 TTL 兜底，不产生永久悬挂 job。

**通道分工**：MCP stdio 统一走“提交 + 轮询”，不做推送。WebSocket 推送（含 `thumb_updated`、job 完成事件）只服务 Web 客户端。

### 6.8 广播策略：控制流与数据流

| 类型 | 内容 | 广播策略 |
|---|---|---|
| **控制流** | **所有原子元数据**（全部 kind，含 draw_stroke 等创建类原子） | **全局广播** |
| **数据流** | Tile 位图、缩略图位图、blob 二进制 | 视口订阅过滤 / 按需拉取 |

划界依据：客户端折叠状态需要**全部**原子元数据——创建类原子同样影响对象树，漏收即状态不一致。原子元数据很小（大二进制已外置至 CAS），全量广播成本可忽略——这是 CAS 外置换来的红利。只有渲染位图输出按视口过滤。

WS 支持 viewport/zoom 订阅消息；服务端只向会话推送其视口范围内的 Tile 更新。

---

## 7. 缩略图

### 7.1 独立管线

原子 → L2 状态 → ThumbState → ThumbBlock → 缩略图输出

### 7.2 缩略图分类

| 类型 | 尺寸 | 用途 |
|---|---|---|
| doc_thumb | 64/128/256 | get_state、文档列表 |
| layer_thumb | 32/64 | 图层面板 |
| object_thumb | 16/32 | 对象列表 |
| history_thumb | 32/64 | 检查点、变更集 |
| selection_thumb | 32 | 选区预览 |

### 7.3 渲染方式

1. **对象重投影**：参数化对象直接缩放重绘。
2. **低分辨率直接渲染**：在缩略图尺寸上直接渲染。
3. **全分辨率降采样**：仅在必要时用。

### 7.4 增量更新

- 缩略图分块（如 32×32），只重渲染 dirty 块。
- 异步降级生成：先返回上一版本，后台完成后 Web 客户端收到 `thumb_updated` WS 事件；MCP 客户端通过 `get_render_status` 轮询。

### 7.5 Token 优化

- 默认返回 URL，不返回 base64。
- MCP 响应可选内嵌小尺寸 image content（≤512px），同时给 URL（MCP 协议支持 image content；不保证 Agent 能 fetch 任意 URL）。
- HTTP 层返回 URL + 二进制端点。
- 差量传输：只传 dirty 块。
- 格式：WebP/AVIF。

---

## 8. 局部渲染与局部缩略图

### 8.1 Region 定义

```
Region
  doc_id
  type: bbox | object | layer | selection | path | polygon | mask
  bbox: {x, y, w, h}
  padding: number
  layers: [layer_id]?
  objects: [object_id]?
  include_below: bool
  zoom: number
  min_resolution, max_resolution
  format, quality
```

### 8.2 局部缩略图

- region_thumb、object_region_thumb、layer_region_thumb、selection_region_thumb。
- 默认 padding 5%，最小 16px，最大 256px。

### 8.3 局部渲染

- 高分辨率、精确、用于编辑和导出。
- 图层隔离渲染、对象隔离渲染、上下文合成。
- 局部调整与滤镜。

### 8.4 RegionBlock 缓存

```
RegionBlock
  coord: (bx, by)
  size: 32 | 64 | 128
  data: bytes
  hash
  version_atom
  layers, objects
```

### 8.5 性能指标

| 操作 | 目标 |
|---|---|
| 区域缩略图更新（缓存命中） | < 5ms |
| 区域缩略图更新（需重渲染） | < 15ms |
| 区域渲染（缓存命中） | < 10ms |
| 区域渲染（未命中，简单 Region） | < 100ms |
| 区域渲染（未命中，含滤镜/调整） | < 300ms |

---

## 9. 对象复制、实例化与组引用

### 9.1 四种复制模式

| 模式 | 引用关系 | 同步 | 覆盖 |
|---|---|---|---|
| copy | 无 | 无 | 无 |
| instance | 有 | all | 无 |
| instance_override | 有 | 部分 | 有 |
| linked_group | 有 | 组级 | 组级+成员级 |

### 9.2 折叠规则

```
resolve_object(object_id, head):
  obj = get_object(object_id, head)
  if obj.type == "instance":
    master = resolve_object(obj.master_ref.object_id, head)
    resolved = apply_override(master, obj.override)
    resolved = apply_sync_policy(resolved, master, obj.sync_policy)
    resolved = apply_transform(resolved, obj.master_ref.local_transform)
    return resolved
  else:
    return obj
```

### 9.3 缓存与失效

- 实例共享 Master 渲染缓存。
- override 只含 transform 时，直接变换 Master 位图。
- Master 修改时，依赖图传播失效（结构 dirty，6.6）。
- 循环引用检测：拒绝创建并返回错误。

### 9.4 工具 API

- copy_object, copy_objects, copy_group, copy_region
- create_instance, detach_instance, link_to_master
- update_override, update_sync_policy
- create_group, add_to_group, remove_from_group, set_group_transform
- get_dependency_graph, get_resolved_state

---

## 10. 工具协议与 API

### 10.1 成功返回格式

```json
{
  "ok": true,
  "atom_id": "sha256:...",
  "seq": 12345,
  "changeset_id": "cs_42",
  "head": "sha256:...",
  "dirty_bbox": [0.1, 0.2, 0.5, 0.4],
  "preview": {
    "thumb_url": "https://.../thumb.webp"
  },
  "job_id": "job_123",
  "warnings": [],
  "suggestions": []
}
```

### 10.2 工具集与暴露分层

工具总量约 70 个。**Agent 的函数选择准确率随工具数量下降，且工具 schema 本身是每次会话的 token 开销**。因此按两层暴露：

**核心层（默认注册，27 个）**：

- 查询：get_document, get_state, list_layers, list_objects, get_object, render_region
- 图层：create_layer, update_layer, delete_layer, reorder_layers
- 导入：import_image
- 绘制：draw_stroke, draw_shape, draw_text, fill, erase
- 修改：update_object, update_stroke, move_object
- 删除：delete_object
- 撤销：revert, reapply
- 历史：get_log
- Job：get_job, get_render_status, cancel_job
- 批量：batch

**扩展组（按 profile 参数启用，命名前缀分组）**：

| 组 | 工具 |
|---|---|
| history | get_object_history, get_diff, get_changesets, get_checkpoints, get_ancestors, get_descendants, find_atom, revert_to, checkpoint, restore_checkpoint, declare_head |
| changeset | begin_changeset, commit_changeset, abort_changeset, revert_changeset, begin_transaction, commit_transaction |
| retouch | retouch, liquify, resample |
| semantic | analyze_image, inpaint_region, generate_mask_from_prompt, semantic_replace, vectorize_stroke, apply_style_transfer |
| conflict | resolve_conflict |
| annotation | create_annotation, update_annotation, delete_annotation, list_annotations, get_annotation, resolve_annotation, reject_annotation |
| collab | comment, suggest, accept_suggestion, reject_suggestion |
| structure | lock_layer, unlock_layer, transform_object, set_property, path_edit, restore_object, list_brushes |

**启用机制**：MCP initialize / HTTP / WS 会话建立时传 `profile: ["core", "semantic", ...]`，默认 `["core"]`。Web 编辑器启用全量。工具命名统一前缀，帮助模型分类检索。

**语义说明**：

- `analyze_image` 调用外部多模态服务，核心只传图和接收结构化结果。
- AI 语义工具内部调用外部模型服务，输出强制封装为独立对象（RasterPatch 或带 Clipping Mask 的对象组），生成结果作为标准原子提交（blob 先行）。AI 也可直接调用微观工具。
- `import_image`（JPEG/PNG/WebP → 像素图层对象，色彩配置解析，位图入 Blob CAS）。PSD 只读导入列为后续。
- `path_edit`（split, merge, join, close, reverse, boolean, convert_to_shape, convert_to_path）。
- batch 内原子可独立 revert 或 supersede；revert_changeset 整体撤销一个变更集。

### 10.3 update_stroke 参数

参数分三层结构：

**核心参数（6 个）**：points / points_patch、brush、color、size、opacity、blend_mode。

**Preset 参数**：`preset` 字段引用命名预设，覆盖核心参数默认值。

**Advanced 嵌套对象**：

```
advanced:
  geometry: {smooth, simplify, resample, offset, scale, rotate, flip, transform}
  appearance: {size_curve, opacity_curve, pressure_curve, dynamics, seed, texture, wetness, paint_load, mixing}
  structure: {layer_id, z_index, group_id, visible, locked, alpha_lock, clipping_mask, mask_id}
  metadata: {name, tags}
```

---

## 11. 笔刷与风格

### 11.1 笔刷

MVP 只做通用光栅笔刷，可调大小、流量、不透明度、混合模式、纹理、湿度和载墨量。其他介质（油画、水彩、马克笔、铅笔、像素、矢量）通过 WASM 插件扩展。

**WASM 插件边界约束**：

- 无网络访问、无系统时钟。
- 确定性 PRNG 注入，不允许自主随机。
- 内存和 CPU 时间受配额限制。
- 插件浮点结果属于 D2 等级。
- 插件 `id + version` 随原子记录，升级不自动改变旧文档渲染。

### 11.2 风格系统

```
Style
  id, name, category, parent_style
  brushes, palette, textures
  render_params, seed_policy
```

支持继承、覆盖、自定义。内置少量基础风格。

---

## 12. 协作与并发控制

### 12.1 并发模型

- Append-Only 原子日志是唯一权威排序。服务端按接收顺序分配 seq。
- 每个 Agent / 人类有独立会话。
- 客户端 WASM 计算内核层执行本地乐观渲染，异步提交原子。
- 服务端收到原子后按 seq 追加，WS 全局广播原子元数据（6.8）。
- 像素级操作在服务端串行化。

### 12.2 提交时 precondition 校验

原子提交时，服务端在**当前状态**上校验：

- 引用的对象不存在 → 拒绝，`reference_not_found`。
- 引用的对象已被 tombstone → 拒绝，`precondition_failed`。
- 引用的图层已删除 → 拒绝，`precondition_failed`。
- 引用的 blob 不存在（违反提交顺序协议）→ 拒绝，`reference_not_found`。

客户端收到拒绝后，回滚本地乐观渲染的对应操作，并提示用户。

### 12.3 冲突处理：拒绝 + 客户端重提交 + 组合宏

**两类操作语义**：

| 类型 | 操作 | 并发策略 |
|---|---|---|
| **生成性叠加** | draw_stroke、fill、draw_shape、draw_text、erase | 默认 LWW 自然叠加，不走冲突层 |
| **采样性替换** | retouch、inpaint、heal、patch | 基于源像素采样，并发互踩才升级冲突处理 |

**采样性替换的冲突处理流程**：

1. 服务端在提交校验阶段检测同一 Region 的并发采样性替换冲突。
2. **不追加原子，直接返回 `conflict` 错误** + 冲突图层 id。若冲突图层不存在，服务端先以系统 actor（`actor: "system:conflict"`）追加 `create_layer` 原子（`type: raster`，`metadata.conflict = true`）。
3. 客户端生成**新 ULID** 的原子，改投冲突图层，重新提交。
4. 用户/AI 通过 `resolve_conflict` 工具解决。

**resolve_conflict 是组合宏，不是新原子类型——折叠器零改动**。工具层展开为已有原子序列：

| resolution | 展开为 |
|---|---|
| keep_ours | tombstone(对方原子) + move(我方原子 → 正式图层) |
| keep_theirs | tombstone(我方原子) |
| discard | tombstone(双方原子) |
| merge | 上层（人类/AI）手动编辑后自行提交；工具仅关闭冲突标记 |

冲突图层在 resolve 后 tombstone，`metadata.conflict` 标记保留供审计。

**优点**：协议完全纯净（原子不可变、append-only、客户端可折叠一致全部保持），折叠器零改动。代价是一次往返（服务端处理 20ms 级）。

### 12.4 离线编辑与悬空变更集

- 离线期间本地追加原子，`parents` 指向离线前所见 head。
- 重连时，服务端校验原子 precondition。
- 校验通过 → 按 seq 追加。
- 校验失败 → **不自动 Rebase**。原子及其引用的 blob 一起打包存入独立的“悬空变更集（Stash）”。
- Stash 在 Web 编辑器 UI 以“分支对比”形式呈现，由上层逻辑决定：丢弃 / 强制应用到当前 HEAD（可能产生视觉错误）/ 基于当前 HEAD 重新生成。
- Stash 中的 blob 归入**历史级**保留（6.3），不被 GC。
- Stash 原子无 seq（未进日志）；重新提交时获得新 seq。
- **引擎只报告冲突，不解决语义冲突。**

### 12.5 跨 actor revert 权限

- 默认只能 revert 自己的原子或自己创建的 changeset。
- 跨 actor revert 需要 owner 权限。
- 权限检查在服务端执行。

### 12.6 评论与建议

- comment 原子，引用目标。
- suggest 原子，包含 patch。
- accept_suggestion → reapply。reject_suggestion → 记录原因。

### 12.7 鉴权

Phase 1 即实现文档级 capability token（随机不可猜 URL）。原子 `actor` 字段与会话认证绑定。

- **MCP stdio**：本地进程豁免鉴权。
- **HTTP / WebSocket**：走 capability token。
- **Token 发放**：打开文档返回内嵌 token 的 URL；后续请求携带 token。
- Phase 5 扩展为 owner/editor/viewer 角色权限。

### 12.8 实时协作

- WS 全局广播新原子元数据（控制流，6.8）。
- 客户端 WASM 计算内核层增量折叠并渲染。
- blob 与 tile 位图按需拉取 / 视口过滤（数据流）。

---

## 13. Web 编辑器

### 13.1 打开模式

| 模式 | 返回内容 | 预算 |
|---|---|---|
| **view** | HEAD 渲染缓存 + 状态摘要 | < 100ms |
| **edit** | 图层树 + 对象摘要（id/type/bbox）先行 + tiles 流式加载 | 1s 内可交互 |

对象详情按需 `get_object` 获取。

### 13.2 历史浏览

数据源：原子日志。支持按原子步进、按 actor 筛选、按类型筛选。不做视频导出。

### 13.3 人类编辑工具集

**客户端渲染**：Web 编辑器加载 WASM 计算内核层（与服务端共享 Rust 代码），本地执行乐观渲染。渲染降级：WebGPU → WebGL2 → WASM CPU 回退。

**WASM 内存管理**：

- WASM 内部实现基于 LRU 的 Tile 内存池，硬性上限（如 500MB）。
- JS 端通过 `IntersectionObserver` / 视口矩阵，主动调用 WASM 暴露的 `evict_outside_viewport(bbox)`。
- **兜底**：内存水位达到硬上限 90% 时，WASM 内部自动触发 LRU evict，不依赖 JS 调用（防 iframe 隐藏等场景下 observer 不触发）。
- 不依赖 Rust 自动 Drop（WASM 线性内存与 JS GC 隔离）。

**基础工具**：画笔、橡皮、形状、文本、填充、吸管、选区、移动、变换、裁剪、抓手、缩放。

**高级工具**：图层管理、蒙版编辑、调整、滤镜、基础修图、基础液化、选区高级、路径、渐变、图案、历史、实例、组。

**人类操作产生原子**：画笔 = draw_stroke，改颜色 = supersede，删除 = tombstone，撤销 = revert。

**人类交互优化**：本地 WASM 乐观渲染，操作立即显示，原子异步提交（blob 先行）。服务端确认后以服务端状态为准，tile cross-fade 平滑校正。

**输入方式**：鼠标、触控笔、触屏、键盘、数位板。

**性能指标**：工具切换 < 16ms，首笔呈现延迟 < 16ms，持续笔迹帧预算 < 8ms，选区响应 < 50ms，图层操作 < 100ms，历史跳转 < 200ms。

### 13.4 标注

**标注类型**：区域标注、对象标注、箭头、文字批注、涂鸦、高亮。

**标注存储**：独立 append-only 通道，不进入主原子日志，不参与折叠。

**标注工作流**：人类标注 → AI 解析 → AI 建议 → 人类预览 → 接受/拒绝 → 应用 → 状态更新。AI 侧通过轮询 `list_annotations(status=pending)` 或 WS 事件感知新标注。

**API**：create_annotation, update_annotation, delete_annotation, list_annotations, get_annotation, resolve_annotation, reject_annotation。

**性能指标**：标注创建 < 16ms，标注渲染 < 16ms。AI 解析与建议延迟取决于外部服务，典型解析 1–3s，建议 3–10s。

---

## 14. 性能与资源

### 14.1 内存分级

| 画布 | 图层 | 内存目标 | 测量条件 |
|---|---|---|---|
| 4K | 10 | < 4GB | 100% 缩放，混合光栅/矢量内容 |
| 8K | 5 | < 8GB | 100% 缩放 |
| 16K | 按需 | 不承诺统一上限，强制冷热分离 | 仅活跃 tile 常驻 |

16K 画布单张 RGBA 约 1GB，必须按需加载 tile。服务端不要求全图 HEAD 常驻，只维护对象索引树和活跃 Tile。

其他内存管理：内存池（对象/缓冲区/字符串/矩阵）、零拷贝、Arc/Rc + 弱引用、压力降级。

### 14.2 CPU

- 线程模型：主线程、渲染、IO、编解码、AI、GC、监控。
- SIMD：AVX2/AVX-512/NEON。
- 无锁数据结构：跳表、LRU、队列、图。
- 任务调度：优先级、任务窃取、批量合并、去重、取消。
- 缓存友好：紧凑排列、数组索引、缓存行对齐。

### 14.3 GPU

**GPU 的职责边界与两层架构一致（原则 19）**：

- **合成后端层**：tile 上传、显示合成、blit、双缓冲。服务端可选 GPU，客户端 WebGPU → WebGL2 → CPU。
- **缓存**：显存内的 tile 缓存与纹理流式（分块上传，只传可见）。
- 显存预算：4K 画布、10 图层，目标 < 6GB。
- **计算内核层（混合、滤镜、液化求解、笔触 stamping）不依赖 GPU**：CPU/SIMD + 多线程是基线（D0 权威）。计算内核的 GPU 版本是远期项，且任何内核 GPU 化必须先通过 D1 容差测试才可启用（17 章）。

### 14.4 网络

- 增量同步：只传新原子元数据，blob 与位图按需拉取。
- 延迟预算：原子提交（批量）< 10ms，单原子 < 20ms（服务端处理延迟）；公网同步 < 200ms。
- 断线重连：从最后原子拉取。
- 离线编辑：本地追加，重连同步（12.4）。

**压缩**：小于 256 字节的原子不压缩；大于 256 字节用 zstd；HTTP 层 Content-Encoding: zstd；图像用 WebP/AVIF；历史级 blob 归档用 zstd。

### 14.5 冷启动与打开文档

- view 模式：直接加载 HEAD 渲染缓存，< 100ms。
- edit 模式：图层树 + 对象摘要先行 + tiles 流式加载，1s 内可交互。
- 服务端重启恢复：从最近快照 + 重放恢复，CRC 校验。
- **时间旅行预算分级**：
  - 近期历史（目标 seq 附近存在可用快照）：< 300ms。
  - 关键 checkpoint（用户显式创建，快照不清理）：O(快照加载)，< 300ms。
  - 老历史（附近快照已被 LRU 清理）：需从更早快照或空白折叠 + 从 blob 归档层取回，秒级，UI 显示进度提示。

### 14.6 大画布

- 支持 1K–16K。分块 256/512。金字塔多级分辨率。mmap 按需分页。流式渲染。

### 14.7 长时间会话

- 泄漏检测、缓存增长控制、原子日志增长、句柄泄漏。
- 长会话降级：< 1h 正常，1–4h 清理，4–8h 降低预计算，> 8h 建议重建（重建 = 客户端重连刷新，文档状态在服务端不丢失）。
- 指标：8h 内性能衰减 < 20%。

### 14.8 降级与容错

- Level 0: 全质量
- Level 1: 降低预览频率
- Level 2: 降低渲染分辨率
- Level 3: 禁用物理模拟
- Level 4: 禁用区域渲染，用全局缩略图
- Level 5: 只读模式

### 14.9 可观测性

核心指标：

- 折叠延迟、快照落后量、Tile 命中率、原子队列深度、内存水位、渲染队列延迟。
- Blob 生命周期：三级数量与体积、活跃→历史降冷迁移量、孤儿清理量、归档取回延迟。

### 14.10 性能预算总表

| 维度 | 预算 |
|---|---|
| 打开文档 view 模式 | < 100ms |
| 打开文档 edit 模式（可交互） | < 1s |
| 首笔呈现延迟（本地） | < 16ms |
| 持续笔迹帧预算 | < 8ms |
| 原子提交（批量，服务端处理） | < 10ms |
| 原子提交（单原子，服务端处理） | < 20ms |
| 区域渲染（缓存命中 / 未命中简单 / 未命中复杂） | < 10ms / < 100ms / < 300ms |
| 时间旅行（近期历史 / 关键 checkpoint） | < 300ms |
| 时间旅行（老历史，含归档取回） | 秒级，UI 提示 |
| 内存（4K/10 图层） | < 4GB |
| 内存（8K/5 图层） | < 8GB |
| 显存（4K/10 图层，合成后端） | < 6GB |
| 网络延迟（公网） | < 200ms |
| 8h 会话性能衰减 | < 20% |

**CI benchmark**：本表关键行纳入 CI benchmark 套件，自动回归。出口条件不只是 fuzz 测试，还包括预算表的自动验证。

---

## 15. 插件与外部服务

| 功能 | 提供方式 |
|---|---|
| 油画、水彩、马克笔、铅笔、像素、矢量介质 | WASM 插件 |
| 漫画分格、对白、网点 | WASM 插件 |
| 海报版式、品牌套件、模板 | 独立设计工具或插件 |
| 语义选区（select_subject, select_sky, select_face） | 外部模型服务（SAM/SAM2） |
| 人像精修 | 外部模型服务 |
| 超分 | 外部模型服务 |
| 去噪、去模糊、HDR | 外部模型服务 |
| 风格迁移 | 外部模型服务 |
| 内容感知修补、内容感知移动 | 外部模型服务 |
| 抠图、人脸检测 | 外部模型服务 |
| 图像分析 | 外部多模态服务 |
| 视频导出 | 外部工具 |
| 动画时间线 | 独立动画工具 |
| ICC 色彩管理 | 专业印刷场景按需接入 |

**外部服务部署形态与隐私**：

- **默认部署形态：本地推理端点**。发行版默认对接本地部署的模型服务（如本地 SAM、Real-ESRGAN via ONNX/本地推理服务）。本地处理不涉第三方传输，**默认启用**——否则 AI 原生工作回路（看图、修图）开箱即残。
- **第三方云服务**：首次使用时一次性授权向导，明确告知数据流向与接收方，未授权不发送。
- `analyze_image` 等默认指向本地端点；无本地端点时提示配置或授权云服务。

**核心保留的基础选区**：矩形、椭圆、套索、多边形、魔棒、按颜色、画笔蒙版。不依赖外部服务。

---

## 16. 选型参考

以下项目作为技术选型参考，分进程内库与外部服务两类。实际集成前经过 Phase 0 技术验证。

**进程内库（候选）**：

| 模块 | 项目 | 备注 |
|---|---|---|
| GPU 合成后端 | Vello (Rust) | 依赖 WebGPU，wasm32 一致性需验证；可能只覆盖合成后端一部分 |
| ICC 色彩管理 | Little CMS (C) | 生产验证充分 |

**外部服务（候选）**：

| 模块 | 项目 | 备注 |
|---|---|---|
| 超分 | Real-ESRGAN | Python 生态，进程内嵌入不现实；本地端点部署 |
| 语义选区 | SAM / SAM2 | 外部服务，按需调用 |

**Phase 0 验证项**：

- Vello + WASM + WebGPU 可行性，及 WebGL2 回退路径。
- 跨平台渲染一致性（CPU D0 基线 + GPU D1 容差）。
- 折叠引擎代数正确性 + Fuzz 测试。
- 液化求解方案（CPU 基线）。
- Blob CAS 并发读写竞态（10 客户端同时上传相同 hash）。
- 大量半透明笔触叠加时 CPU 软渲染性能。
- 极端原子序列：`create → supersede → revert(supersede) → revert(create) → reapply(create)`，验证无孤儿对象与悬挂指针；`revert → GC 周期 → reapply`，验证历史级 blob 取回正确。

服务端渲染以 CPU/SIMD 为基线路径，GPU 为可选加速（仅合成后端）。

---

## 17. 远期规划与可能

以下功能可能在未来版本支持，当前不做：

- 视频录制与回放视频导出
- 动画、时间线、关键帧系统
- 完整 Git 式分支、合并、rebase、diff、blame
- 多 Agent 公平调度、自动分支、锁 TTL 自动释放
- 多光标、多选区可视化
- CRDT 草稿同步
- WASM 第三方插件沙箱
- 本地 AI 模型内嵌（进程内推理）
- **计算内核 GPU 化**（须逐内核通过 D1 容差门控）
- 移动端
- 对象存储、远程冷热分离
- 签名 Ed25519
- 细粒度权限角色
- 复杂文字排版（CTL）
- PSD 只读导入

---

## 18. 部署

- 单二进制：Rust 编译，无复杂依赖（GPU 为可选加速，仅合成后端；CPU/SIMD 为基线）。
- 协议：MCP stdio、HTTP、WebSocket。
- 存储后端：本地文件、SQLite、Blob CAS（文件系统，含冷归档层）。
- 原子格式：JSON + 二进制可选。
- 插件：WASM 笔刷、滤镜。
- Web UI：可选，加载 WASM 计算内核层。
- 压缩：zstd（> 256 字节的原子和 HTTP 响应）。
- 字体：内嵌开源字体子集，禁用平台相关 hinting，首版仅 Latin/CJK 基础。
- 鉴权：Phase 1 用文档级 capability token，Phase 5 扩展为角色权限。
- **本地推理端点**：发行版默认配置本地模型服务连接（可执行文件或 sidecar 容器），作为 AI 语义工具的默认后端。

---

## 19. MVP 路线图

### Phase 0：技术验证

- Vello + WASM + WebGPU 可行性验证，WebGL2 回退路径验证。
- CPU D0 基线渲染器实现。
- 折叠引擎原型 + 代数属性测试 + Fuzz 测试（含 5.3 全部不变量与 Phase 0 极端序列）。
- 液化求解方案验证。
- Blob CAS 并发读写竞态验证 + 三级生命周期/GC 原型。
- Overdraw 性能验证。

**出口条件**：上述六项全部有可运行原型或明确替代方案。折叠 fuzz 10 万原子无不变量违反（无孤儿对象、无悬挂指针）；`revert → GC → reapply` 用例通过。

### Phase 1：原子核心 + 折叠 + 服务端渲染

- 原子模型、Append-Only 日志、客户端 ULID 幂等、服务端权威 seq。
- 折叠求值模型、有效集、级联失效、reapply 决策、HEAD 指针、state@seq 公式实现。
- 逻辑快照、Blob CAS + 三级生命周期 + 提交顺序协议、错误协议。
- 图层隔离、Tile 分块、几何/结构双 dirty 传播。
- 服务端 CPU/SIMD 渲染。
- 文档级 capability token。
- 最小 Web 查看器 + WebSocket 全局广播。
- 工具：核心层 27 个（10.2）。

**出口条件**：折叠 fuzz 10 万原子无不变量违反；view 模式打开 < 100ms；CI benchmark 覆盖预算表关键行。

### Phase 2：WASM 核心 + WS 协作 + 本地乐观渲染

- WASM 计算内核层编译与集成。
- WS 原子元数据全局广播 + tile 视口过滤（控制流/数据流分离）。
- 客户端本地乐观渲染 + tile cross-fade 校正。
- 最小画笔子集接入 Web 编辑器。
- WASM LRU 内存池 + JS-WASM 视口联动 + 水位兜底。
- L3/L4 缓存、异步渲染。
- import_image。
- Job 协议：wait_for_render + get_job / get_render_status / cancel_job。

**出口条件**：客户端与服务端 CPU 路径 bit-exact；首笔呈现延迟 < 16ms。

### Phase 3：基础修图 + GPU 合成 + 通用笔刷

- GPU 合成后端与缓存（计算内核保持 CPU）。
- 通用光栅笔刷、风格系统。
- 基础修图：clone/heal/patch、基础液化、调色。
- retouch 源像素版本化 + resample。
- 冲突处理：采样性替换 conflict + resolve_conflict 组合宏。
- 检查点、历史浏览。
- 人类基础工具完整接入。
- AI 语义工具：inpaint_region, generate_mask_from_prompt, semantic_replace。

**出口条件**：4K/10 图层内存 < 4GB；渲染一致性测试通过率 100%（D0）。

### Phase 4a：标注基础

- 标注独立通道、标注 CRUD、标注可视化。

### Phase 4b：标注 AI 解析与建议

- AI 解析标注、AI 生成建议、接受/拒绝流程。
- 依赖外部多模态服务（默认本地端点）。

### Phase 5：插件 + 高级功能

- WASM 插件沙箱 + 能力模型 + 版本锁定。
- 实例、组引用。
- 高级路径编辑。
- owner/editor/viewer 权限。

---

## 20. 风险与对策

> 设计决策已入正文（并发 LWW 求值规则见 5.3，冲突处理见 12.3，GC 策略见 6.3）。本表仅保留风险与工程对策。

| 风险 | 对策 |
|---|---|
| 折叠语义实现错误 | 5.3 形式化规范 + 代数属性测试 + Fuzz（含极端序列） |
| 折叠性能下降 | 快照 + 增量 + 重放上限 1000，重型原子强制快照 |
| GC 误删历史 blob 破坏可回放性 | GC 根集 = 全日志引用闭包；三级生命周期；Phase 0 专项用例 |
| Blob 悬空引用 | 提交顺序协议：blob 先行 + 提交时校验（12.2） |
| 原子日志无限增长 | 快照 + 压缩 + 归档，不删原子 |
| 打开文档慢 | view/edit 双模式，view 读取 HEAD 渲染缓存 |
| 渲染性能下降 | O(dirty) + 图层隔离 + Tile 分块 + 结构 dirty 传播 |
| 跨平台渲染不一致 | D0/D1/D2 分级，计算内核层 CPU 为权威基线 |
| 计算内核 GPU 化引入漂移 | GPU 仅合成后端；内核 GPU 化为远期且逐内核 D1 门控 |
| 缩略图更新阻塞 | 异步、独立队列、降级生成 |
| 区域缓存膨胀 | RegionBlock 分块 + LRU + 内容哈希共享 |
| 实例解析递归过深 | 缓存 + 依赖图 + 深度限制 |
| 循环引用 | 检测 + 拒绝 + 错误提示 |
| 内存/显存不足 | 分级预算 + LRU + 降级 |
| 16K 画布内存超限 | 按需加载 tile + 不承诺统一上限 |
| WASM Tab OOM | 显式 LRU 内存池 + JS-WASM 视口联动 + 水位自动 evict 兜底 |
| 采样性替换并发冲突 | 拒绝 + 客户端重提交 + resolve_conflict 组合宏 |
| 离线协作 Rebase 噩梦 | 不做自动 Rebase，Stash + UI 层决定 |
| 乐观渲染被拒绝 | 回滚 + 提示 + tile cross-fade |
| 跨 actor revert 滥用 | 默认仅限自己的原子，跨 actor 需 owner 权限 |
| Token 消耗高 | 多级预览 + 区域渲染 + 默认返回 URL + 工具暴露分层 |
| 工具数量降低 Agent 选择准确率 | 核心 27 个默认注册 + profile 分组按需启用 + 命名前缀 |
| MCP stdio 异步回路断裂 | Job 协议 + 轮询 + TTL + cancel；WS 只服务 Web 客户端 |
| 视口过滤漏收原子 | 控制流（全部原子元数据）全局广播，数据流视口过滤 |
| 绘画能力弱 | 通用笔刷 + WASM 插件 + 外部服务 |
| 修图效果差 | 基础功能自研，高级功能外部服务 |
| retouch 源像素版本漂移 | source_state_version + resample |
| 人类工具复杂 | 渐进式复杂度 + 默认参数 + 高级展开 |
| 标注解析不准 | 多模态 AI + 上下文 + 多轮对话 |
| 标注污染主日志 | 独立 append-only 通道，不参与折叠 |
| 字体版权 / 跨端一致性 | 内置开源字体子集 + 禁用平台 hinting |
| 色彩语义未定义 | 线性光合成 + 预乘 alpha，进入 D0 基线 |
| 外部服务隐私 | 默认本地推理端点；云服务首次使用授权向导 |
| 本地推理端点性能不足 | 端点可替换（更强本地硬件或授权云）；工具超时由 job TTL 兜底 |
| WASM 插件不确定性 | 能力模型：无网络、无时钟、确定性 PRNG；版本锁定 |
| 液化方案不成熟 | Phase 0 spike，CPU 基线 |
| WebGPU 不可用 | WebGL2 回退 + WASM CPU 回退 |
| 性能预算无验证载体 | CI benchmark 套件，纳入出口条件 |

---

## 21. 总结

偃师的核心是：

把 GUI 的面板加鼠标轨迹换成文档状态、语义命令、可编辑对象、多级预览、版本日志和协作机制。

- **折叠求值模型**：原子按 seq 线性扫描，维护有效原子链。revert 级联失效，reapply 只恢复目标原子。`state@seq_n` 有公式定义（5.5）。折叠代数不变量由属性测试与 Fuzz 保证。
- **原子日志**：只追加、不删除、不可变；服务端 Append-Only 日志是唯一权威排序。客户端生成 ULID，服务端按 id 去重。
- **HEAD 指针**：`declare_head` 统一实现时间旅行、revert_to、restore_checkpoint，日志保持 append-only。declare_head 是重型原子，强制快照 + 全量 tile 失效。
- **打开即图片**：view 模式 < 100ms；edit 模式状态先行 + tiles 流式加载 < 1s。
- **Blob CAS 与历史完整性**：原子只存引用，blob 先行提交。**blob 与原子同属历史**：GC 根集 = 全日志引用闭包，三级生命周期（活跃/历史/孤儿），只清理孤儿。revert、时间旅行、Stash 永不因 GC 丢失数据。
- **确定性等级**：D0（计算内核 CPU bit-exact 基线）、D1（合成后端 ±1 LSB）、D2（插件/外部服务）。
- **渲染两层**：计算内核层纯 Rust 共享编译、D0 bit-exact；合成后端层 GPU 可选、三级降级。“单一来源”精确指代计算内核层。
- **广播边界**：所有原子元数据 = 控制流，全局广播；tile 位图/blob = 数据流，视口过滤/按需拉取。
- **人和 AI 功能对等、语义分层**：底层对象模型和原子类型统一；人类用微观工具，AI 用宏观语义工具（输出强制封装为独立对象）。
- **工具暴露分层**：核心 27 个默认注册，扩展组按 profile 启用，控制 Agent 选择负担与 token 开销。
- **冲突处理**：生成性叠加 LWW；采样性替换冲突 → 拒绝 + 客户端重提交 + resolve_conflict 组合宏（折叠器零改动）。不做自动 Rebase，离线走 Stash。
- **Job 协议**：wait_for_render + 轮询 + TTL + cancel。MCP stdio 走“提交 + 轮询”，WS 推送只服务 Web 客户端。
- **retouch 非破坏性**：source_state_version + resample。
- **标注独立通道**：不进主日志，不参与折叠。
- **外部服务默认本地推理端点**：AI 工作回路开箱可用；云服务显式授权。
- **性能**：内存分级、双 dirty 传播、时间旅行预算分级、CI benchmark、可观测性（含 blob 生命周期指标）。
- **部署**：Rust 单二进制，MCP stdio + HTTP + WebSocket，无头优先，CPU/SIMD 基线，本地推理端点默认。

对 AI 来说，它最自然：

看状态 → 分析图像 → 调用语义工具批量画/修 → 轮询 job → 看预览 → 改对象 → 继续。

对人类来说，它可介入：

实时看、直接改、评论、接受建议、时间旅行、标注修改意见。

文档不是状态，文档是历史；状态只是历史在当前 HEAD 上的折叠结果。渲染只看状态，不看历史。**历史是资产——原子和 blob 都是不可回滚删除的资产；状态是缓存，只有缓存可以被回收。**打开文档是读取渲染状态，不是重放历史。关系优先于拷贝。回放是历史的可视化，标注是协作的入口，工具是分层的体现。冲突不自动合并，引擎只报告。折叠语义有公式、有不变量、有 Fuzz。人和 AI 一起创作、一起修改、一起成长。

---

> **冻结声明**：本版（draft4）为 v1.0 冻结候选。剩余已知问题均为实现级（在 PR 评审中修复），不再进行下一轮全量纸面评审。下一步行动：
> 1. 将 5.3 求值规范与 5.5 state@seq 公式翻译为属性测试代码；
> 2. 用测试代替第四轮评审（折叠器、GC 可回放性、广播边界三处由 fuzzer 检验）；
> 3. 启动 Phase 0。
