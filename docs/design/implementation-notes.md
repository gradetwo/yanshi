# 实现说明与设计文档的对应关系

> 本文件记录 **crate `yanshi-core` 相对冻结设计文档 v1.0-draft4 的实现级明确化**。
> 设计文档是权威；这里只记录文档留白处引擎做出的选择、依据与对应测试。
> 任何行为变更都必须先改设计文档，再改本文件与代码（见 [CONTRIBUTING.md](../../CONTRIBUTING.md)）。

## 一、模块与章节对应

| 模块 | 设计文档 | 内容 |
|---|---|---|
| `error` | 5.7 | 错误码、上下文、`ok/error_code/retryable/context` 响应 schema |
| `ids` | 5.1 | ULID 客户端标识、`Seq` 与各类 id 别名 |
| `atom` | 5.1 / 5.2 / 6.3 | 原子结构、30 类原子与分类谓词、`Refs` 引用收集、`BlobHash` |
| `state` | 4.1 – 4.5 | 文档状态、图层/对象/选区/蒙版/风格/检查点、活跃 Manifest、一致性检查 |
| `log` | 5.1 / 12.1 / 12.2 | Append-Only 日志、ULID 幂等、权威 `seq`、提交时校验 |
| `fold` | 5.3 / 5.4 | 折叠求值、有效集、级联失效、`reapply`、LWW、双 dirty 判定谓词 |
| `seq` | 5.5 | `state@seq` 公式、`declare_head`、`revert_to` / `restore_checkpoint`、增量折叠 |
| `snapshot` | 4.5 / 6.5 | 旁路快照、CRC、触发条件、LRU 与清理例外 |
| `blob` | 6.3 | CAS、提交顺序协议、三级生命周期、GC 根集与报告 |
| `changeset` | 5.6 | 变更集聚合、整体撤销计划 |
| `conflict` | 12.3 | 采样性替换冲突检测、冲突图层原子 |
| `testkit` | 5.3 / 19 | 确定性场景生成器（属性测试、fuzz 共用） |

## 二、实现级明确化

### 1. 跨 `declare_head` 的 `revert` / `reapply` 被拒绝

- **文档位置**：5.4（撤销合法目标）、5.5（`state@seq` 公式）。
- **问题**：5.5 中 `A_n = (H_n.seq, n]`，`base_state(H_n)` 已是**折叠后的状态**。若允许
  `revert` 指向 `H_n` 之前的原子，该原子的效果已烘进 `base_state`，仅靠向前折叠无法撤销。
- **引擎选择**：提交期要求 `revert` / `reapply` 的目标 `seq` 大于当前求值起点
  （`DocumentState::eval_origin_seq()`），否则返回 `invalid_argument`。
- **结果**：`A_n` 的有效集永远与全局一致，区间内不会出现 `declare_head`，
  `state@seq_n` 在任意 `n` 上可复现。
- **测试**：`src/seq.rs::tests::cross_origin_revert_is_rejected_at_commit`。

### 2. head 指针 = 已折叠区间内 seq 最大的原子

- **文档位置**：4.1（`Document.head`）、5.1（`seq` 是唯一排序依据）。
- **引擎选择**：`fold` 完成后把 `head_seq` / `head_atom` 置为**本次折叠区间内 seq 最大的原子**，
  无论它是产生状态效果的原子、`revert`、还是 `comment`。`applied` 单独记录“实际产生状态效果的原子”。
- **理由**：head 表示历史位置而不是“最后一次有效修改”；这样增量折叠与完整折叠逐字段收敛，
  幂等性不变量才逐字段成立。
- **测试**：`tests/fold_properties.rs::incremental_fold_converges_with_full_fold`。

### 3. 图层 tombstone 级联到后代图层与其中对象

- **文档位置**：5.3（无孤儿引用）、12.2（引用已删除图层即拒绝）。
- **引擎选择**：`tombstone(layer)` 递归标记后代图层及其中的对象；因为折叠是纯函数，
  撤销该 tombstone 会整体重算，不会留下“被级联删除”的痕迹。
- **测试**：`src/fold.rs::tests::layer_tombstone_cascades_to_children_and_objects`。

### 4. `reorder_layers` 必须携带完整 z 序

- **文档位置**：5.3（“并发 `reorder_layers` 携带完整目标 z 序，绝对序快照”）。
- **引擎选择**：`order` 必须与“当时的存活图层集合”完全一致，否则 precondition 失败并级联失效。
  未列出的存活图层不会被隐式移动到列表末尾。
- **测试**：`src/fold.rs::tests::reorder_layers_is_absolute_and_last_writer_wins`。

### 5. 检查点 base 从日志解析，而非当前状态

- **文档位置**：4.5（检查点是轻量元数据原子）、5.5（`base_state` 可引用 checkpoint）。
- **引擎选择**：`HeadBase::Checkpoint` 的 `anchor_seq` 通过 `seq::checkpoint_anchor_seq`
  在日志中解析。若某检查点原子位于跳变区间内，它不会出现在跳变后的状态里，但依然可作为求值起点。
- **测试**：`src/seq.rs::tests::checkpoint_restore_jumps_to_anchor_seq`。

### 6. 辅助引用参与折叠期 precondition 与提交期校验

- **文档位置**：5.3（无孤儿引用）、12.2（引用不存在即拒绝）。
- **引擎选择**：`style_id` / `mask_id` / `parent_id` 以及 `set_property` 的取值引用
  都必须指向当时存活的实体；撤销 `create_mask` / `create_style` 时，依赖它们的绑定原子级联失效，
  而不是在状态里留下悬空引用。提交期校验同样覆盖 `set_property` 的取值引用，
  使这类原子在入口处即被拒绝（`reference_not_found`）。
- **测试**：`src/fold.rs::tests::reverting_mask_creation_cascades_to_dependent_layer_property`、
  `object_with_style_reference_keeps_no_orphans`。

### 7. 采样性替换冲突窗口截断为最近 1000 原子

- **文档位置**：6.2（普通原子重放上限 1000）、12.3（采样性替换冲突处理）。
- **引擎选择**：冲突检测只在 `parents` 之后的最近 `CONFLICT_WINDOW_ATOMS = 1000` 个原子中判定；
  超出窗口的并发互踩不再由服务端自动判定，交由 `resolve_conflict` 组合宏人工处理。
- **测试**：`src/conflict.rs` 单元测试；`src/log.rs::tests` 覆盖冲突拒绝路径。

### 8. 无 `declare_head` 文档的快照 base_ref 使用哨兵

- **文档位置**：4.5（`base_ref: {type: declare_head | snapshot, id}`）。
- **引擎选择**：尚无任何 `declare_head` 时，快照的 `base_ref` 记为
  `{"type": "snapshot", "id": "root"}`，表示“空白初始状态”这一逻辑根，`root` 不是真实快照。
  一旦出现 `declare_head`，后续快照即以该原子为 base。
- **测试**：`src/snapshot.rs::tests`。

## 三、尚未实现（与 README 路线图一致）

- **Phase 1 其余部分**：服务端进程与 HTTP/WebSocket/MCP 传输层、图层隔离与 Tile 分块渲染、
  服务端 CPU/SIMD 渲染、Web 查看器、核心层 27 个工具的协议封装。
- **Phase 2 起**：WASM 计算内核、控制流/数据流分离的广播、本地乐观渲染、Job 协议、
  `import_image`、Job TTL 与取消。
- **Phase 3 起**：GPU 合成后端、修图与液化、`resolve_conflict` 工具展开、
  `accept_suggestion → reapply` 的展开（当前协作原子在折叠中无状态效果，符合 5.2 注）、
  AI 语义工具与本地推理端点。
- 实例/组引用（9 章）当前只有数据模型（`ObjectRef`、`Instance` / `Group` 对象类型），
  折叠求值中的覆盖与同步策略解析属 Phase 5。

## 四、验收载体

| 设计文档要求 | 载体 |
|---|---|
| 5.3 五条折叠代数不变量 | `tests/fold_properties.rs`（proptest，随机场景） |
| Phase 0 极端序列与 GC 可回放性 | `tests/phase0_exit.rs` |
| 10 万原子折叠 fuzz（Phase 0 出口） | `tests/phase0_exit.rs::fuzz_100k_atoms_keeps_all_invariants`（`#[ignore]`，CI 以 `--ignored` 执行） |
| Blob CAS 并发读写竞态（16 章 Phase 0 验证项） | `src/blob.rs::tests::ten_concurrent_writers_of_same_hash_produce_one_file` |
| 错误协议 schema（5.7） | `src/error.rs::tests::error_response_matches_document_schema` |
| 提交顺序协议（6.3） | `src/log.rs::tests::commit_validation_rejects_missing_blob`、`tests/fold_properties.rs::atom_metadata_stays_small_and_references_existing_blobs` |
