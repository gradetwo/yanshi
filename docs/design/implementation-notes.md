# 实现说明与设计文档的对应关系

> 本文件记录 **crate `yanshi-core` 相对冻结设计文档 v1.0-draft4 的实现级明确化**。
> 设计文档是权威；这里只记录文档留白处引擎做出的选择、依据与对应测试。
> 任何行为变更都必须先改设计文档，再改本文件与代码（见 [CONTRIBUTING.md](../../CONTRIBUTING.md)）。

## 一、模块与章节对应

### 1.1 `yanshi-core`（历史资产层与折叠引擎）

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

### 1.2 `yanshi-render`（渲染计算内核层，D0 CPU 基线）

| 模块 | 设计文档 | 内容 |
|---|---|---|
| `half` | 6.1 | IEEE-754 binary16 存储（内存 tile 用 f16 线性） |
| `color` | 6.1 | 线性光空间、预乘 alpha、sRGB 传递函数、u8 打包 |
| `blend` | 6.1 | 9 种混合模式的 W3C 合成公式 |
| `prng` | 6.1 / 11.1 | 由 `seed` 驱动的确定性随机源（抖动） |
| `geometry` | 8.1 | 覆盖率光栅化（矩形解析、椭圆/多边形超采样）与 stamp 展开 |
| `tile` | 6.4 / 13.3 | Tile、TileGrid（32–512）、TileCache（字节预算 LRU、视口淘汰） |
| `buffer` | 6.4 / 8.3 | 区域像素缓冲（线性预乘 f32），覆盖率填充、合成、blit、蒙版 |
| `object` | 4.2 | 对象 → 渲染图元解析与效果包围盒 |
| `brush` | 11.1 | 通用光栅笔刷 stamping（间距/硬度/压力/流量/抖动/虚线） |
| `filter` | 4.2 | 调整与滤镜内核（模糊、亮度对比、饱和度、色阶、反相） |
| `dirty` | 6.6 | 几何 / 结构双 dirty 传播与依赖闭包 → tile 失效集 |
| `render` | 6.2 / 8.3 | 文档渲染：图层隔离、蒙版、调整/滤镜作用域、位图补丁、区域裁剪 |
| `thumb` | 7 章 | 缩略图分级尺寸、宽高比适配、32×32 分块增量更新 |
| `png` | 18 章 | 零依赖确定性 PNG 编码器（stored deflate + CRC32/Adler32） |

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

### 9. 渲染内核的实现级约定

- **D0 基线**：像素运算为标量 `f32`，顺序固定；f16 存储往返可复现；
  随机量只来自原子 `seed`；所有合成在线性光空间、预乘 alpha 下完成。
  新增 SIMD/GPU 内核必须保持逐位一致（计算内核层）或通过 D1 容差（合成后端层）。
- **tile 分块不是语义单位**：`tests/render_properties.rs::tile_size_does_not_change_pixels`
  断言 32/64 分块输出逐字节一致；缓存淘汰同样不影响像素。
- **dirty 契约**：`dirty::plan_dirty` 产出的失效 tile 集合必须覆盖所有真正变化的像素
  （属性测试 `dirty_set_covers_every_changed_pixel` 逐像素校验）。
  无法精确判定依赖时宁可放大范围（整层或整文档），因为放大只多渲染、不会留陈旧 tile。
- **调整/滤镜对象的作用域**：作用于**同图层中位于其下方**（更低 `z_index`）的内容；
  滤镜需要邻域，渲染器按文档内最大滤镜半径扩展渲染区域后再裁剪
  （`RenderOptions::expand_for_filters`，上限 `max_filter_padding = 64`）。
- **图层蒙版**用蒙版 `shape` 的覆盖率乘以图层 alpha；剪贴蒙版用下方内容的 alpha 裁剪本层。
- **位图补丁格式**：内核内使用 `image/x-yanshi-raw`（未压缩 RGBA8，尺寸取自
  `data.width/height` 或 `region`）。WebP/AVIF 编解码属传输层，遇到其它 MIME 记为
  `RenderStats::unsupported` 告警而不是失败。
- **未实现的渲染类型**（文本光栅化、retouch/liquify、实例/组引用）同样只记告警；
  缺失 blob 则返回 `reference_not_found`（这是协议级错误，不是降级）。
- **缩略图宽高比**：文档非正方形时按“最长边适配 + 居中留白”排版（7.2 只规定边长）。
- **PNG 输出**是验收载体（无外部依赖、可逐字节复现）；14.4 要求的 WebP/AVIF 与 zstd
  属传输层编码，尚未实现。

### 10. 性能基线（单线程标量实现实测）

`cargo test -p yanshi-render --release --test perf_budget -- --ignored --nocapture`
（4 核容器、release、单线程、标量 f32；数字随机器波动，用于跟踪回归）：

| 场景 | 实测 | 设计目标（8.5 / 14.10） |
|---|---|---|
| 区域渲染（缓存命中，256×256） | ≈ 50 µs | < 10 ms |
| 区域渲染（未命中，256×256，60 笔 + 8 形状，1024×1024 文档） | ≈ 55 ms | < 100 ms |
| 区域渲染（未命中，含高斯模糊 σ=3） | ≈ 65 ms | < 300 ms |
| 区域缩略图更新（64×64 区域 → 64） | ≈ 22 ms | < 15 ms（含一次区域渲染，留 5 倍 CI 余量） |
| 整文档渲染（1024×1024，60 笔 + 8 形状） | ≈ 530 ms | 信息项 |
| 200 笔 overdraw 整文档 | ≈ 1.3 s | 信息项 |

已做的关键优化：对象包围盒裁剪（不渲染区域之外的对象）、预乘表示的
`source-over` 快速路径（省去反预乘除法）、stamping 内层循环的裁剪与直接索引写入。
SIMD/多线程（14.2）与 GPU 合成后端（14.3）属后续阶段；计算内核层的任何
SIMD 版本必须保持 D0 逐位一致。

### 1.3 `yanshi-server` / `yanshi-mcp`（无头服务端与工具层）

| 模块 | 设计文档 | 内容 |
|---|---|---|
| `token` | 12.7 | 文档级 capability token（256 bit）、角色、传输分类与鉴权 |
| `job` | 6.7 | Job 状态机、TTL 兜底、取消、GC、容量上限 |
| `broadcast` | 6.8 / 12.8 | 控制流全局广播 / 数据流视口过滤，含 14.9 统计 |
| `annotations` | 4.6 / 13.4 | 标注独立 append-only 通道与过滤 |
| `document` | 3 / 6.2 / 12.1 | 文档服务：提交校验 → 权威 seq → 增量折叠 → dirty → 广播 → Job → 快照 |
| `persist` | 18 | 原子 JSONL、`meta.json`、渲染缓存、`blobs/` CAS |
| `service` | 3 | 多文档工作区：创建/打开/关闭/列表、提交落盘、令牌发放与恢复 |
| `tools` | 10 章 | 核心 27 工具 + profile 分层 + 10.1/5.7 响应 |
| `base64` | 7.5 | MCP `image` content 的 base64 编码 |
| `yanshi-mcp` | 3 / 6.7 | MCP stdio：`initialize` / `tools/list` / `tools/call` / `ping` |

### 1.4 `yanshi-http`（零依赖 HTTP/1.1 + WebSocket 传输层）

| 模块 | 设计文档 | 内容 |
|---|---|---|
| `sha1` | 握手 | 手写 SHA-1（仅用于 `Sec-WebSocket-Accept`；签名留给 17 章 Ed25519） |
| `ws` | 6.8 / 12.8 | RFC 6455 握手、帧编解码（7/16/64 位长度、掩码强校验、16 MiB 上限）、分片重组、ping/pong/close |
| `http` | 10 章 | 请求解析、查询解码、响应写出、5.7 错误码 → HTTP 状态映射 |
| `server` | 3 / 6.8 / 12.7 | 路由、鉴权、WS 会话与推送线程 |
| `viewer` | 6.2 / 7 章 | 最小 Web 查看器（单页 HTML/JS，零前端依赖） |

路由表（`server::routes()` 同名函数可自省）：

| 方法 | 路径 | 鉴权 | 说明 |
|---|---|---|---|
| GET | `/health` | 无 | 存活与统计 |
| GET | `/` | 无 | 最小 Web 查看器 |
| GET | `/api/documents` | 无 | 文档列表（**仅元数据**，不含像素与日志） |
| POST | `/api/documents` | 无 | 打开/新建文档 → `{doc_id, token, url}`（12.7） |
| GET/DELETE | `/api/documents/{id}` | token | 摘要 / 关闭 |
| POST | `/api/tools/{name}?doc=` | token | 工具调用（10.1 / 5.7） |
| POST | `/api/tools`（body 带 `tool`） | token | 同上 |
| GET | `/api/blob/{hash}?doc=` | token | 取回 CAS 中的 PNG |
| GET | `/ws?doc=&token=` | token | WebSocket 升级与推送 |

WebSocket 消息（JSON 文本帧）：

- 客户端 → 服务端：`{"type":"subscribe","viewport":{x,y,w,h},"zoom"}`、
  `{"type":"tool","request_id":N,"name":"…","arguments":{…}}`、`{"type":"ping"}`。
- 服务端 → 客户端：`{"type":"subscribed",…}`、`{"type":"ack","request_id":N,"result":{10.1}}`、
  `{"type":"error",…}`、`{"type":"pong"}`，
  以及 `{"type":"event","event":{…}}`（`atom` / `tiles` / `thumbnail` / `job_finished` / `annotation`）。

## 一之三、颜色约定（含一次真实事故）

工具层与渲染层共用唯一入口 `yanshi_render::color::parse_spec_color`：

| 写法 | 含义 |
|---|---|
| `[r,g,b]` / `[r,g,b,a]`，所有分量 ≤ 1 | 直通**线性**（alpha 缺省 1.0） |
| `[r,g,b,a]`，任一分量 > 1 | sRGB **字节** `0-255`（与 `{r,g,b,a}` 等价） |
| `{"r":0-255,"g":…,"b":…,"a":…}` | sRGB 字节（alpha 缺省 255） |
| `"#RRGGBB"` / `"#RRGGBBAA"` / `"#RGB"` | sRGB 十六进制 |

**事故**：数组颜色最初无条件按线性浮点直通解析，`[40,120,60,255]` 的 alpha=255
在预乘/合成时直接饱和，笔迹画成了纯白 —— 用户视角是「画了一笔却什么都没看见」。
现在：渲染层把 > 1 的数组当字节；工具层在提交前用 `color_error` 校验，
非法颜色返回 `invalid_argument` 且**不写入日志**。回归测试见
`crates/yanshi-render/src/render.rs::byte_array_colors_paint_the_same_as_srgb_objects`
与 `crates/yanshi-server/tests/service_flow.rs::invalid_colors_are_rejected_and_byte_arrays_paint_correctly`。

## 一之四、缩略图与预览的语义区分

- `Document::latest_preview_url()`：**最近一次**渲染结果，可能只是 dirty 区域（10.1 `preview` 用它）。
- `Document::document_thumbnail_url()`：**覆盖整幅画布**的文档级缩略图（7.3 分级），
  只有全幅 `render_region` 或 doc 级 `thumbnail()` 才会更新，并记录对应的 seq。
- `Workspace::ensure_document_thumbnail()`：缩略图与 HEAD 不一致时重新生成，
  保证 6.2「打开即图片」展示的是**当前**画面而不是历史某一帧。
- 查看器画布渲染**整幅文档**（用 `get_document` 拿到的宽高），不是固定区域——
  早期实现写死 `512×512`，720×480 的文档右边缘被裁掉（真实浏览器截图才发现）。
- 工具 `get_document` / `get_state` 返回文档级缩略图，`preview_size` 可选 `64/128/256/false`。

## 一之二、服务端与工具层的实现级约定

- **鉴权**（12.7）：stdio 与进程内调用豁免鉴权并按 owner 处理；HTTP/WS 必须携带
  文档级 capability token；令牌随 `meta.json` 持久化，重启后仍有效。
- **通道分工**（6.7）：只有 WebSocket 订阅者接收推送；MCP stdio 订阅者
  `PushChannel::Poll` 收到 0 条推送，改用 `get_log` / `get_job` / `get_render_status` 轮询。
- **控制流广播**（6.8）：**所有**原子元数据（含创建类）全局广播、不按视口过滤——
  漏收创建类原子会让客户端折叠状态与权威状态不一致；数据流（tile/缩略图）按视口过滤。
- **冲突处理**（12.3）：`retouch` / `liquify` / `sampling: true` 的 RasterPatch 属采样性替换；
  服务端发现同图层、跨会话、区域相交时**不追加原子**，先以 `system:conflict` actor
  创建 `metadata.conflict = true` 的冲突图层，再返回 `conflict` 错误并附 `conflict_layer_id`；
  客户端换新 ULID 改投冲突图层即可提交。
- **渲染异步**（6.7）：原子提交立即返回；重型原子（retouch/liquify/declare_head 等）创建 Job。
  `wait_for_render`（默认 true，预算 500ms）在进程内实现中直接跑完 Job；
  `--no-wait` 时返回 `job_pending`，由 Agent 用 `get_render_status` 轮询。
  预览默认只渲染 dirty 区域（8.2/8.3），避免每次修改都全图重算。
- **变更集**（5.6）：`batch` 内的原子共享一个 `changeset_id`；单步失败不回滚已提交原子
  （append-only），逐项返回 5.7 错误。
- **标注**（4.6）：独立 append-only 通道，更新/解决/拒绝都是追加新版本；
  不进原子日志、不参与折叠，可用 `list_annotations` 过滤轮询。
- **工具暴露**（10.2）：只注册**已实现**的工具，避免 Agent 看到空壳；
  未实现的语义/修图工具（inpaint_region、semantic_replace、clone_stamp 等）属后续阶段。
- **持久化**（18）：原子按 JSONL 追加（含权威 seq），崩溃残留的半行在加载时忽略；
  渲染缓存写入 `render.png` + `render.seq`，重启后 `mark_rendered` 让「打开即图片」成立。

## 二之二、已实测的性能数据（release，单线程标量，4 核 Haswell 笔记本）

| 场景 | 实测 | 预算 |
|---|---|---|
| view 模式打开（122 / 602 个原子，含日志加载 + 折叠 + 缩略图） | 28.8ms / 33.7ms | < 100ms（Phase 1 出口条件） |
| 无渲染缓存打开（不重放历史） | 65µs | — |
| 文档摘要查询（已打开） | 1.6µs | — |
| 单原子提交（提交校验 + 折叠 + 双 dirty） | 0.43ms / 2.67ms | < 20ms（14.10） |
| 缓存命中区域渲染 256×256 | ≈50µs | < 10ms（8.5） |
| 未命中区域渲染 256×256（60 笔） | ≈55ms | < 100ms（8.5） |
| 未命中区域渲染（含高斯模糊） | ≈65ms | < 300ms（8.5） |
| 64×64 区域缩略图 | ≈22ms | < 15ms（需重渲染场景，当前超出，见下） |
| 全幅 1024×1024 渲染 | ≈527ms | — |

> 64×64 区域缩略图当前 ≈22ms，高于 8.5 表里的 15ms 目标：该路径先做整幅渲染再降采样，
> 属 Phase 2「L3/L4 缓存 + 异步渲染」要解决的问题（缩略图应直接按比例采样目标区域）。

## 二之三、Phase 2 客户端验收（真实 Chromium + CDP）

| 出口条件 | 状态 | 证据 |
|---|---|---|
| 客户端与服务端 CPU 路径 **bit-exact** | ✅ 通过 | 浏览器内「一致性自检」：本地 `render_region_png` 的 SHA-256 == 服务端 `blob_hash`（`window.yanshiStats.bitExact === true`）；宿主回归测试 `yanshi-wasm::tile_composed_render_is_bit_exact_with_whole_region_render` 覆盖 tile 32/64 |
| 首笔呈现延迟 **< 16ms** | ✅ 实测 **1.20ms**，且走在**已证正确**的路径上（tile 失效 + 由权威状态重渲染），不依赖读改写 tile | `window.yanshiStats.firstStrokeMs`；一笔一原子、本地与服务端 HEAD 一致、bit-exact 自检通过、页面零 JS 异常 |

**bit-exact 缺陷复盘**：服务端 `render_region` 从 f32 scratch 缓冲直接转 u8，客户端按 tile 组合时经 f16 量化后转 u8，
两者在舍入边界差 **149/4194304 字节**（如 32 vs 31，首个差异在 (629,213)）。修法是让输出以 **f16 tile 为准**
（14.1：内存 tile 用 f16 线性）——把裁剪后的缓冲就地 `quantize_f16` 再转显示空间。
中途还踩到第二个坑：先写成「从 tile 缓存读回」，小预算下 tile 已被 LRU 淘汰导致像素丢失，
被 `render_properties.rs` 的属性测试抓到（`cache_eviction_does_not_change_pixels`）；改成就地量化即与缓存状态无关。

**首笔延迟的修复路径**（525ms → 143ms → **6.2ms**）：

1. **覆盖率生成与遍历裁剪到目标缓冲**（`shape_coverage_in` / `*_coverage_clipped` / `fill_coverage` 只走交集）：
   一块覆盖全画布的形状原先在每个 tile 渲染时都按自身 1024×1024 bbox 迭代。单块 tile 冷渲染
   **81.7ms → 30.9ms**，512×504 区域 **145ms → 103ms**；缓存命中 39µs。像素不变（属性测试全绿）。
2. **覆盖层增量盖章**（`Renderer::stamp_into_tiles` + `Kernel::extend_preview_stroke`）：
   只把新增笔段 stamp 到已缓存 tile 上，成本 ∝ 笔段长度（最后一帧 999 像素 = 8~9ms），
   而不是整块 tile 面积。
3. **落笔不再整块重绘**（`Kernel::commit_preview`）：像素已由增量盖章画好，
   `plan_dirty` 的整块失效重绘会在抬手时造成 158ms 卡顿。
4. **覆盖层状态更新不失效 tile**（`upsert_preview_object(.., invalidate=false)`）：
   每帧都失效整条增长笔迹的 tile 会把增量盖章打回整块重绘（实测浪费 ~130ms/帧）。

**已修复：局部渲染会清空 tile 其余像素**（影响所有「按 tile 组合」的渲染路径）。

`Renderer::store_tiles` 原先把每次区域渲染的结果**整块**写回缓存，缓冲未覆盖的像素被当作透明，
于是哪怕一次 1×1 的区域渲染都会把该 tile 的其余内容抹掉；客户端 WASM 内核正是按 tile 组合渲染，
因此会出现白块/内容消失。现在 `tile_from_buffer_preserving` 会在已有 tile 的基础上**只覆盖本次
真正渲染到的像素**，回归测试 `partial_region_render_preserves_untouched_tile_pixels` 断言
「局部渲染后整幅组合渲染逐字节不变」，并逐 tile 比对整幅渲染的对应像素。

**已用「按笔段失效 + 小区域直接渲染」达成首笔 1.2ms（正确路径）**：

1. **只失效新增笔段**：笔迹是追加式增长的，`extend_preview_stroke` 在「点列是前缀扩展」时
   只失效新增段的包围盒（点列被替换时才退回整段并集）。旧像素不会被擦除，正确性不依赖任何增量盖章。
2. **小区域走直接渲染**：按 tile 组合时，哪怕 1px 变化也要重算整块 256² tile（≈31ms，
   与测量到的冷 tile 成本吻合）；新增 `render_region_direct`（scratch 路径、不碰缓存）
   让拖动重绘的成本只与**区域面积**成正比。两条路径逐位一致，并有单测断言
   （冷/热缓存、含覆盖层三种情形）。

实测（真实 Chromium，1024² 文档含曲线+噪点+暗角+HSL+辉光，客户端 16 块 tile）：
首笔 **1.20ms**（原 143ms）、一笔一原子、bit-exact 通过、零 JS 异常。

> 备注：早先「独立覆盖层 + canvas 合成」的实验（内核侧已跑通）仍然有价值，
> 但当前路径已经同时满足正确性与 16ms 预算，故不再需要它承担该职责。

**方案备选（未采用）：独立覆盖层（不读改写 tile）**。

思路：待提交笔迹不进 `state`、也不 stamp 进 tile，而由内核单独渲染成**透明底位图**
（`Kernel::render_preview_rgba`，scratch 路径、不写缓存），客户端用「已提交 base 图层 + 覆盖层」
两画布以 source-over 合成。这样拖动路径完全不碰 tile 缓存，从结构上杜绝内容丢失，
且每帧只需重合成新增笔段所在区域。
本轮已把内核侧实现完（覆盖层对象独立于 `state`、`render_preview_rgba`、宿主测试断言
「覆盖层位图透明底且已提交渲染不受影响」「落笔提交后渲染等于直接应用同一原子」，
并在真实浏览器测到**首笔 0.8–3.2ms**），但**客户端合成时序仍有缺陷**：
落笔后本地渲染会落后一颗原子（自检哈希等于上一状态的哈希），
说明提交时的重绘/合成顺序还未理顺。为避免把不完整状态当成成果，已回退到
「失效 + 重绘」这条已证正确的路径；覆盖层方案连同上述具体缺陷留待下一轮收敛。

**已实现：首帧用服务端渲染铺底（6.2「打开即图片」）**。启动顺序改为：先用服务端
`render_region`（命中 HEAD 渲染缓存）出像素并解码，**再在后台预热 WASM 内核对并切换**，
首帧因此不再等待客户端折叠与渲染。实测（真实 Chromium，CDP 读取）：

| 文档 | 首帧（服务端铺底，解码完成） | 内核预热（后台） | 内核接管后画布 | bit-exact |
|---|---|---|---|---|
| `fx-demo` 512²（HEAD 17） | **199ms**（原 ~2.1s） | 56ms | ✅ 已绘制 | ✅ |
| `yanshi` 1024²（HEAD 23） | **434ms** | 41ms | ✅ 已绘制 | ✅ |

`window.yanshiStats` 新增 `firstPaintMs` / `kernelWarmMs`，面板上显示「首帧 / 内核预热」。
后续可再把首帧降到 100ms 以内：用文档级缩略图（更小的 PNG）或 1/2 缩放图先铺底，
再由内核覆盖成清晰像素。

**已修复：区域外扩被静默截断（`clone_stamp` 一度破坏 bit-exact）**。

`RenderOptions.max_filter_padding` 默认 **64**，而 `filter_padding` 会按对象声明外扩
（修图 `|offset| + size/2 + 2`、液化 `size/2 + strength*size`、蒙版羽化 `半径 + 1`）后
再 `.min(64)` —— **超出部分被静默截断**，于是 tile 渲染取不到源/邻域像素：
offset 200 的 `clone_stamp` 在服务端（整幅）正常、在客户端（tile 组合）取不到源，bit-exact 失败；
而辉光（radius 14 → 外扩 28 < 64）恰好没被截断，所以此前一直通过。

修复三件套：
1. `max_filter_padding` 直接等于 `MAX_EFFECT_PADDING`（128），声明多少就外扩多少（不再静默截断）；
2. 工具层按同一常量**拒绝超限参数**（`source_offset`/`smudge_length`/`size`/`strength` 组合的
   可达范围），把「静默不一致」变成显式限制并给出原因；
3. 渲染期对**历史原子**（可能超限）写入 `stats.unsupported` 告警，避免无声无息。

**当前 bit-exact 状态**（每份 1024² 文档只差一个特性，真实 Chromium + CDP）：

| 文档 | 内容 | 结果 |
|---|---|---|
| c1 | 形状 + `clone_stamp`（offset 60） | ✅ |
| c2 | + `smudge` | ✅ |
| c3 | + `liquify_push` | ✅ |
| c4 | + 蒙版（羽化 24） | ✅ |
| fx3 系列 | 曲线 + 白平衡 + HSL + 辉光 + 噪点 + 暗角 | ✅ |
| **phase3b** | **上述全部 + `heal_stamp` + 长笔迹** | ❌ |

后续两轮探针（同样 1024²，真实 Chromium）：

| 文档 | 内容 | 结果 |
|---|---|---|
| h1 | 形状 + **仅 `heal_stamp`** | ✅ |
| p4 | 与 phase3b 完全相同，**但不含那条长笔迹** | ✅ |
| phase3b | p4 + `draw_stroke`（780px 长笔迹，size 26） | ❌ |

结论修正：`heal_stamp` 单独无问题；**触发条件是「`draw_stroke` 笔迹对象 + 上述效果组合」**。
下一轮的定位路径（都很便宜）：

1. 只含「形状 + 笔迹」的文档 → 若通过，说明是笔迹与某个效果的两两交互；
2. 「形状 + 笔迹 + 蒙版/羽化」→ 重点验证蒙版羽化的方框模糊与笔迹抗锯齿边缘的相互作用；
3. 「形状 + 笔迹 + 液化/修图」→ 重点验证跨区域采样在笔迹跨越多个 tile 时的源可见性。

本轮又做了四个组合探针（同样 1024² / 真实 Chromium）：

| 文档 | 内容 | 结果 |
|---|---|---|
| s1 | 形状 + **笔迹** | ✅ |
| s2 | s1 + 蒙版（羽化 30） | ✅ |
| s3 | s1 + `clone_stamp` + `liquify_push`（跨区域采样） | ✅ |
| s4 | s1 + 曲线 + HSL + 辉光 + 噪点 + 暗角 | ✅ |
| **phase3b** | s2 + s3 + s4 + `heal_stamp` + `smudge`（全部同时存在） | ❌ |

即：**这是高阶交互** —— 任意单独一组、以及「笔迹 + 任一其他组」都通过，
只有多组同时存在时才复现。已知排除：每类特性单独、以及「笔迹 + 单一其他组」。

**宿主侧复现测试（已加入）**：`crates/yanshi-wasm/tests/tile_consistency.rs` + 夹具
`tests/data/phase3b_atoms.json`（直接从服务端 `GET /api/atoms` 导出），断言
「按 256² tile 组合 == 整幅直接渲染」。结论**与预期相反**：

- `simple_atoms.json`（形状 + 笔迹）：差异 0 ✅（默认用例，毫秒级）
- `phase3b_atoms.json`（全特性 21 原子）：差异 **0** ✅（`#[ignore]`，约 3 分钟）

也就是说**客户端内核的分块组合本身没有问题**，浏览器端 bit-exact 失败**不是 tile 化导致的**，
而应定位到：

1. **客户端 WASM 与服务端原生的差异**（同一份代码，但浮点超越函数/优化差异可能引入 ulp 级偏差）；
2. 或浏览器渲染路径（自检取的区域/尺寸、PNG 编码路径）。

下一轮的直接验证方式：把夹具原子集喂给**服务端**（`Document::render_region_raw` 整幅）与
**内核**（`render_region` 组合），在宿主上直接对比两者的像素与 PNG 哈希 —— 这能一次性判定
是「WASM vs 原生」还是「浏览器路径」，且仍然是毫秒/秒级。

**决定性进展：原生复现（2 字节）**。新增 `crates/yanshi-server/tests/render_parity.rs`
（在 `yanshi-server` 的 dev-dependencies 里加入 `yanshi-wasm`），把同一份夹具原子集分别喂给
**服务端 `Document`**（整幅原生渲染）与**客户端内核**（按 tile 组合），直接比对像素：

- `simple_atoms.json`（形状 + 笔迹）：差异 **0** ✅（默认用例，5 秒）
- `phase3b_atoms.json`（全特性 21 原子）：差异 **2 / 4,194,304 字节**（`#[ignore]`，release 下 4.4 秒）

结论与意义：

1. 浏览器里的失败**不是 WASM 与原生**的差异，而是**服务端与内核两条渲染路径**的差异；
2. 差异只有 2 字节（相当于 1 个像素的 1 个通道差 1，或两个像素各差 1），且**在宿主上 4 秒可复现**——
   定位成本从「CDP 逐份构造文档」降到「本地跑一个测试」；
3. 已排除 tile 尺寸（服务端与内核都是 256）与分块组合本身（内核内自比 0 差异）。

下一步（都很快）：打印差异像素的坐标与通道差值，看它是否落在 **tile 边界 / 蒙版边缘 / 效果叠加处**；
再按夹具原子二分到最小集合。修复方向可能是「让 f16 量化与缓冲边界无关」或「统一裁剪语义」。

**顺带记录一个性能信号**：1024² 单次全效果渲染约 **182 秒**（外扩上限提到 128 后，
单块缓冲 1280²，叠加辉光/液化/修图的重运算）。Phase 3 的这些效果在大画布上的成本需要
单独优化（例如按对象裁剪重算范围），已记入待办。

**原始记录（上一轮二分）：`clone_stamp` 破坏客户端/服务端 bit-exact**。

全链路文档（形状 + 曲线/白平衡/HSL + 辉光/噪点/暗角 + clone_stamp + 液化 + 蒙版）在真实
Chromium 中 bit-exact 自检**失败**。用「逐个特性叠加」二分（每份 1024² 文档只差一个特性）：

| 文档 | 内容 | bit-exact |
|---|---|---|
| b1 | 形状（底 + 圆） | ✅ |
| b2 | + `glow` | ✅ |
| **b3** | **+ `clone_stamp`** | ❌ |
| b4 | + `liquify_push` | ✅ |

原因分析：滤镜/液化/羽化都是**有限支撑的邻域运算**，只要把邻域计入区域外扩就能保证
「分块 == 整幅」；而 `clone_stamp` 的源是「**当前渲染区域的图层缓冲**」，源的可见范围
取决于区域大小本身。`filter_padding` 已按 `|offset| + size/2 + 2` 申报外扩，
但客户端按 256² tile 渲染时，源仍可能落在「tile + 外扩」之外
（本用例 `source_offset = 200`，外扩 232 → 单块缓冲 720²，理论上应覆盖；
因此更可能是外扩未被真正应用，或外扩在 tile 路径上被裁剪）。

修复方向（下一轮）：让 `patch`/`clone_stamp` 之类的**跨区域采样**改为从**文档级源**取像素
（例如先整幅渲染一份源，或把源区域显式渲染进缓冲），而不是依赖「本区域缓冲里恰好有源」；
在此之前 `clone_stamp` / `heal` / `smudge` 在客户端按 tile 组合时会与服务端有差异。

**已试并否决：笔触采样按 tile 裁剪（负优化）**。思路是在 `dashed_line` 里跳过完全落在
目标缓冲之外的线段（保持相位一致），只对可见笔段采样。A/B 实测（release，同一台机器、
同一测试场景「256×256 含全画布背景 + 60 笔」）：

| | 单块 tile 冷渲染 | 512×504 区域冷渲染（9 块） | 缓存命中 |
|---|---|---|---|
| 关闭裁剪（现状） | **31.0ms** | 112.1ms | 35.8µs |
| 启用裁剪 | 37.7ms | 108.6ms | 35.7µs |

结论：**单块反而慢 6.7ms**，区域差异在噪声内。原因是 `draw_stamp` 对越界采样本来就有
廉价的提前返回，而裁剪版每段都要构造 `Bbox` 并做相交判断，开销超过了省下的盖章。
因此该改动已回退；真正的成本在别处（每个对象仍要对自己的全部采样做一次
`draw_stamp` 调用与覆盖率计算），下一步应做**对象级/笔段级空间索引**而不是采样裁剪。

**未解决：增量盖章（读改写 tile）会让 tile 丢内容**。为拿回首笔延迟而实现的
`Renderer::stamp_into_tiles_incremental`（把新笔段直接 stamp 到缓存 tile 上）在真实路径上会让
tile 丢掉场景内容（症状：`(36,15)` 处场景笔迹变成背景白；第 3 帧首次触及该 tile 时发生）。
已排除的原因：u8 往返量化（改为 f32/f16 直通后差异字节数不变）、tile 缺失时未渲染、
`store_tiles` 局部覆盖（已修）、覆盖层对象 z 序、缓冲原点/尺寸不匹配。
在定位清楚之前，覆盖层走**失效 + 客户端重绘**这条已证正确的路径
（`Kernel::extend_preview_stroke`），因此：
- 采样相位连续的增量能力（`StrokeCursor` / `dashed_line_from` / `stamp_samples_from`）已实现并单测通过
  （逐段采样与一次性整段**逐点相同**），等缺陷修好后即可启用；
- **首笔延迟回到 ~143ms**（此前 6.2ms 的测量是在带此缺陷的路径上得到的，不应作为达标证据）。

**首笔延迟历史拆解**（真实 Chromium，覆盖层单帧）：

| 环节 | 耗时 |
|---|---|
| `putImageData`（512×504） | **1.1ms** |
| WASM 渲染同一区域 | **1558ms**（原生 145ms，WASM 约 ×10） |

原生剖面（同一文档 + 覆盖层，tile 256）：

| 场景 | 耗时 |
|---|---|
| 单块 256×256 tile **冷渲染**（11 对象） | 81.7ms |
| 同一 tile **缓存命中** | 19µs |
| 512×504 区域（9 块）冷渲染 | 145ms |

两个根因（已定位，待修）：

1. **对象光栅化没有裁剪到目标缓冲范围**：一块覆盖全画布的矩形背景，在每个 tile 渲染时都按自己的
   1024×1024 bbox 迭代（约 100 万像素），白烧十几倍工作量。改为与目标缓冲求交后再迭代。
2. **覆盖层每帧失效整块 tile** → 整块重绘（9 块 ≈ 1.5s）。改为**增量盖章**：把新笔段直接
   stamp 到已缓存的 tile 像素上（缓存命中路径本身只要 19µs），只在落笔/切换工具时整块重算。

**首笔延迟待办（下一轮）**：
1. 绘制中用**同一个 atom id** 反复 `apply_atom` 会走幂等分支、笔迹不会增长——正确做法是内核提供
   「本地待提交笔迹」覆盖层（不进原子日志），落笔时才提交一个最终原子（设计 13.3 的乐观渲染本意）。
2. 需要拆分计时（WASM 渲染 vs `putImageData`）定位 525ms 的构成；画布 1024×1024 的整幅重绘路径
   很可能被卷入，应只重绘脏区/视口。
3. 首帧装载（`loadKernel` + 整幅渲染 ≈2.1s）也需要按 13.1 view 模式优化：直接用服务端 HEAD 渲染缓存铺底，
   WASM 只接管后续增量。

## 二之三、Phase 3 调色/滤镜内核扩展（本轮新增）

内核新增 3 种调整 + 4 种滤镜，全部只用参数决定、可复现，并已由工具层放行：

| 类型 | 参数 | 说明 |
|---|---|---|
| 调整 `exposure` | `ev` −10..10 | 线性光下 `×2^ev` |
| 调整 `white_balance` | `temperature`/`tint` −1..1 | 暖/冷 + 品红/绿 |
| 调整 `curves` | `points`[[x,y]...]、`channel` rgb/r/g/b | **单调三次插值**（Fritsch–Carlson），保证不过冲 |
| 调整 `hsl` | `hue` −180..180、`saturation` 0..4、`lightness` −1..1 | 标准 HSL：色相旋转 / 饱和度缩放 / 明度偏移 |
| 滤镜 `motion_blur` | `angle`/`distance`/`samples` | 沿角度平均 |
| 滤镜 `sharpen` | `amount`/`radius` | 非锐化掩模 |
| 滤镜 `noise` | `amount`/`seed` | 逐像素确定性，随机量只来自 `seed` |
| 滤镜 `vignette` | `strength`/`radius`/`softness` | 以**文档**中心与对角线归一化 |
| 滤镜 `glow` | `threshold`/`radius`/`intensity` | 高光阈值 → 两趟方框模糊 → **加性**合成（同时加 alpha，否则光晕出不了笔迹范围） |
| 修图 `clone_stamp`（对象类型 `retouch`） | `points`/`source_offset`/`size`/`hardness`/`opacity` | 从**应用本对象之前**的图层内容按偏移采样后盖回（经典仿制图章） |
| 修图 `heal`（工具 `heal_stamp`） | 同上 | 复制源纹理的同时，按**源/目标局部均值差**把低频颜色对齐到目标处 |
| 修图 `smudge`（工具 `smudge`） | `points`/`size`/`smudge_length`/`hardness`/`opacity` | 每 stamp 沿笔迹方向**后退** `smudge_length` 采样，把后方内容拖到前方 |
| 图章补丁（工具 `patch`，对象类型 `raster_patch`） | `source_region`/`target`/`opacity` | 抓取源区域**原始 RGBA8** → 存 blob → 作为 `raster_patch` 落到目标位置 |
| 基础液化（工具 `liquify_push`，对象类型 `liquify`） | `points`/`direction`/`size`/`strength` | 推力位移场（smoothstep 衰减）+ **反向映射 + 双线性重采样** |

实现时踩到 / 需要守住的约定：

- **预乘不变量**：调整必须像 `brightness_contrast` 一样把直通颜色钳制到 [0,1]，
  否则 `rgb > alpha` 会破坏预乘 tile 的假设。测试逐个效果断言 `rgb ≤ alpha` 且数值有限。
- **滤镜必须申报邻域半径**（`FilterKind::padding`）：`motion_blur` 取 `distance/2`、
  `sharpen` 取 `radius`。若不申报，区域渲染不会外扩，边缘会采到画外产生接缝。
- **暗角必须用文档尺寸而非所在缓冲**：按 tile 渲染时缓冲只是文档的一部分，
  以缓冲自身为中心会让块与块之间出现明显接缝。`apply_filter` 因此新增 `canvas` 参数。
  测试同时断言「分块渲染的像素与整幅渲染对应位置一致」。
- **逐像素随机量必须用文档坐标而非缓冲序号**：`noise` 最初用「缓冲内像素序号」做种子，
  256×256 文档恰好只有一块 tile 因此测试通过，但客户端在 1024×1024 文档上要拼 16 块 tile，
  同一像素在不同分块下会得到不同噪声 → 与服务端不一致。已改为由文档坐标推导序号，
  并加回归测试「偏移区域渲染与整幅渲染逐像素一致」。**验收证据**：1024² 文档
  （曲线 + 噪点 + 暗角，客户端 16 块 tile）在真实 Chromium 中 bit-exact 通过。
- **修图对象的区域渲染必须外扩到源像素**：`clone_stamp` 从 `目标 + source_offset` 采样，
  `filter_padding` 因此把 `|offset| + size/2 + 2` 计入外扩量；否则只渲染笔迹附近时，
  源像素落在区域之外会得到「复制不完整」。测试断言「区域渲染与整幅渲染逐像素一致」。
- **蒙版早已实现，但 `feather` 被静默忽略**：`apply_layer_mask` 只做了形状覆盖率与反选，
  `Selection.feather` 完全没有参与计算（表现为「设了羽化却没有软边」）。现已对蒙版 alpha
  做方框模糊（半径 = `feather/2`）；同时把「引用了不存在的蒙版」从静默返回改为写入
  `stats.unsupported`，避免蒙版失效无人知晓。测试覆盖四种情形：裁剪、反选、羽化中间值、缺失告警。
  工具层补齐 `create_mask`（structure 组），挂载沿用既有的 `set_property`
  （原子层本就支持 `layer_id` 目标，只是此前没有测试覆盖）。
- **液化必须反向映射并申报外扩量**：正向映射会在拉伸区留下空洞，因此改为「目标像素取
  `源 − 位移`」的双线性采样；`filter_padding` 把 `size/2 + strength*size` 计入外扩，
  否则区域渲染会缺掉位移所需的源像素。位移只由原子参数决定，因此仍然确定、可复现。
- **`patch` 走「渲染原始像素 → 存 blob → 建原子」三步**：`render_region` 只返回 PNG blob，
  不带像素，因此新增 `Document::render_region_raw` / `Workspace::render_region_raw`
  返回原始 RGBA8（sRGB 直通，与 `blit_rgba8` 的输入约定一致），且**不写渲染缓存与缩略图状态**。
  提交顺序遵守 6.3：blob 先 `put` 成功再提交原子，否则会被 12.2 的引用校验拒绝。
  内核侧沿用既有 `raster_patch`：`region` 即落点矩形，宽高取自源区域。
- **涂抹做成「方向相关的无状态采样」**：真实涂抹需要跨 stamp 的状态传递，工程量大且易失
  确定性；这里改为「每个 stamp 沿笔迹方向后退 `smudge_length` 采样**应用本对象之前的副本**」，
  视觉上等价于把后方颜色拖到前方，同时保持确定性与无自反馈。工具层据此不需要 `source_offset`，
  但会校验 `smudge_length ∈ [0.5, 512]`。
- **修复画笔的均值要挑「双方都不透明」的样本**：图层缓冲在未绘制处是透明的，
  如果要求源与目标都有像素，落在空白处的目标会把所有样本都判废（实测 `samples=0`，
  heal 静默退化成 clone）。现在只统计双方都不透明的样本，一个都没有时明确退回 clone 语义
  （例如在空图层区域上修复）；目标处已有内容时（真实修复场景）校正正常生效。
- **修图的源是应用本对象之前的图层内容**：渲染时先拷贝一份 layer buffer 再采样，
  避免自反馈（否则笔迹会沿着自身不断累积）。指向历史状态的 `source_state_version`
  采样仍未实现，解析层对其它 `retouch_type` 仍报 `Unsupported`，工具层只放行 `clone_stamp`。
- **工具层校验必须按效果名区分**：`radius` 对模糊是像素（1..128）、对暗角是归一化（0..2）、
  对锐化是 1..8；`amount` 对饱和度 0..8、锐化 0..5、噪点 0..1。
  最初写成一套全局范围，结果**合法的暗角参数被拒**（实测踩到），现已改为按 (kind, name) 查表。
- 未实现类型（如 `hsl`、`glow`）继续在工具层被拒，服务端测试用它们守住「不放空壳能力」。

## 二之四、Phase 3 起步：调色与滤镜工具（retouch 组）

工具层新增 5 个工具（`profile: retouch`，服务端默认已启用）：

| 工具 | 说明 |
|---|---|
| `add_adjustment` | 创建调整对象：`brightness_contrast` / `saturation` / `invert` / `levels` |
| `add_filter` | 创建滤镜对象：`box_blur` / `gaussian_blur` / `brightness_contrast` / `saturation` / `invert` |
| `update_adjustment` / `update_filter` | 参数叠加更新（走 `supersede`） |
| `list_effects` | 列出当前调整/滤镜对象与生效顺序，并回带内核支持的名字清单 |

实现约定与踩到的坑：

- **只暴露内核已实现的子集**：修图（`clone_stamp`/`heal`/`smudge`）与液化在 `parse_object` 里仍是
  `Primitive::Unsupported`，因此**不注册**对应工具（避免空壳工具）。支持的名字清单由
  `yanshi_render::{ADJUSTMENT_NAMES, FILTER_NAMES}` 单一来源导出，工具层据此校验。
- **`CreateObject` 的 dirty 判定曾缺失**：原先落到 `_ => DirtySet::none()`，即用 `CreateObject`
  创建调整/滤镜**不会触发任何重绘**。现按对象类型判定：调整/滤镜 → 整层结构 dirty（6.6），
  实例/组 → 全量，其余按几何包围盒。回归测试
  `dirty::tests::creating_an_adjustment_or_filter_invalidates_the_whole_layer`。
- **调整/滤镜只作用于同层下方内容**（渲染按 `(z_index, id)` 顺序累积），因此 `add_adjustment` /
  `add_filter` 在未显式给 `z_index` 时**默认放到同层最上方**；否则默认 z 序会让效果排在底图之前，
  表现为「加了调整却没有任何变化」（实测踩到过）。
- **参数校验在工具层**（5.7）：`sigma` 0.05–128、`radius` 1–128、`passes` 1–8、`brightness` −1–1、
  `contrast`/`amount` 0–8、`gamma` 0.01–10、`levels` 要求 `black < white`；非法参数与未实现类型
  一律 `invalid_argument` 且不写日志。
- **线性光语义**：内核在线性光里做调整，因此 sRGB 字节 128 反相后是 **229**（不是 127）。
  测试按此断言并加注释，避免后人误判为 bug。
- **默认 profile 回归**：新增工具组必须同时纳入服务端默认 profile，否则会出现「工具存在但线上报
  未知工具」的割裂（`add_adjustment` 实测踩到）。回归测试
  `server::tests::default_http_profiles_cover_every_implemented_group`。
- **验收**：真实服务上对 512×512 场景依次 `saturation 2.4` 与 `gaussian_blur σ=7`，
  `dirty_kind` 均为 `structure`，渲染结果肉眼可见正确；带这两个效果的文档在真实 Chromium 中
  **bit-exact 自检通过**（客户端 WASM == 服务端 PNG 哈希），页面零 JS 异常。

## 三、尚未实现（与 README 路线图一致）

- **Phase 1 其余部分**：WASM 计算内核与 SIMD、GPU 合成；编辑器侧的多选/变形/文字渲染等交互。
  服务端语义层、核心层 27 个工具、capability token、广播边界、MCP stdio、
  零依赖 HTTP/1.1 + WebSocket 传输与最小 Web 查看器均已完成。
- **Web 查看器**：桌面环境就绪后已做**真实浏览器**验收（Chromium 153 / Hyprland 0.56.2）：
  真实窗口截图确认整页渲染正确；用 CDP 发真实鼠标事件画一笔，head 7→8、
  日志面板收到 WS 推送的 `atom seq=8 draw_stroke` 与 `tiles 4 个失效`、画布与缩略图同步更新、
  页面零 JS 异常；服务端 `get_log` 确认原子落在 `session:http`（命令走 HTTP、推送走 WS 的分工）。
  此前无浏览器时采用的三层替代验收：
  ① 仓库内单测做 `node --check` 式的语法与**顶层重复声明**检测
  （`viewer.rs::viewer_script_has_no_duplicate_top_level_declarations`，
  曾捕获 `const preview` 与 `async function preview` 冲突导致的整页 SyntaxError）；
  ② 逐个调用页面用到的 HTTP/WS 端点的集成测试（`crates/yanshi-http/tests/transport.rs`）；
  ③ 本地用桩 DOM/WebSocket 在 node 下真实执行页面脚本，验证
  「建文档 → 身份显示 → WS 连接（含 doc/token）→ subscribe 视口 → 缩略图 → 推送事件」全链路无异常。
  （图层隔离、Tile 分块、几何/结构双 dirty 传播、服务端 CPU 渲染已由 `yanshi-render` 覆盖；
  SIMD/多线程优化与 GPU 合成后端仍属后续阶段。）
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
| D0 bit-exact 渲染（6.1） | `crates/yanshi-render/tests/render_properties.rs::rendering_is_bit_exact`、`tile_size_does_not_change_pixels` |
| O(dirty) 渲染正确性（6.2 / 6.6） | `render_properties.rs::dirty_set_covers_every_changed_pixel`、`cache_eviction_does_not_change_pixels` |
| 缩略图分块增量（7.4） | `render_properties.rs::thumbnail_blocks_match_full_rebuild` |
| 区域渲染 / 缩略图预算（8.5 / 14.10） | `crates/yanshi-render/tests/perf_budget.rs`（`--ignored`） |
| Overdraw 与 tile 命中率（Phase 0 / 14.9） | `crates/yanshi-render/tests/perf_budget.rs::perf_overdraw_and_cache_hit_rate`（`--ignored`） |
| **Phase 1 出口条件：view 模式打开 < 100ms** | `crates/yanshi-server/tests/perf_budget.rs::perf_view_open_under_100ms`（`--ignored`） |
| 打开即图片：无渲染缓存时不重放历史 | `crates/yanshi-server/tests/perf_budget.rs::perf_open_without_render_cache_does_not_replay`（`--ignored`） |
| 冲突处理与冲突图层（12.3） | `crates/yanshi-server/tests/service_flow.rs::sampling_replace_conflict_creates_conflict_layer_and_returns_error` |
| Job 协议 TTL/取消/渲染水位（6.7） | `service_flow.rs::jobs_ttl_cancel_and_render_watermark` |
| 广播边界（6.8） | `service_flow.rs::broadcast_separates_control_and_data_flow`、`src/broadcast.rs` 单元测试 |
| 标注独立通道（4.6） | `service_flow.rs::annotations_stay_out_of_the_atom_log` |
| capability token（12.7） | `service_flow.rs::capability_tokens_gate_http_and_allow_stdio` |
| 核心 27 工具协议（10.1/10.2） | `service_flow.rs::tool_layer_covers_core_workflow`、`crates/yanshi-mcp/src/lib.rs` 单元测试 |
| MCP stdio 线协议 | `crates/yanshi-mcp/tests/stdio.rs`（spawn 真实二进制） |
| 持久化与重启恢复（18 / 14.5） | `crates/yanshi-server/src/persist.rs` 单元测试、`yanshi-mcp/tests/stdio.rs::stdio_handshake_draw_render_and_persist` |
| HTTP 传输与 5.7 状态码映射 | `crates/yanshi-http/tests/transport.rs::http_round_trip_health_viewer_and_errors`、`tool_call_renders_region_and_serves_png_from_cas` |
| capability token 鉴权（HTTP/WS） | `transport.rs::capability_token_gates_http_endpoints`、`websocket_rejects_bad_tokens_and_unknown_messages` |
| WebSocket 握手与 ping/pong | `transport.rs::websocket_handshake_uses_rfc6455_accept_key` |
| 推送边界：控制流全局 / 数据流视口过滤 | `transport.rs::websocket_pushes_control_flow_globally_and_data_flow_by_viewport` |
| 颜色约定与字节数组事故回归 | `crates/yanshi-render/src/color.rs` 单测、`render.rs::byte_array_colors_paint_the_same_as_srgb_objects`、`service_flow.rs::invalid_colors_are_rejected_and_byte_arrays_paint_correctly` |
| 缩略图与 HEAD 一致性 | `service_flow.rs::document_thumbnail_is_cover_whole_canvas_not_the_last_region` |
| 查看器脚本可解析性 | `crates/yanshi-http/src/viewer.rs::viewer_script_has_no_duplicate_top_level_declarations` |
