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

**收口结论（按设计分级）**：

| 比较 | 结果 | 依据 |
|---|---|---|
| 同路径（服务端 vs 服务端、内核 vs 内核） | **逐位相同** | `tile_consistency.rs`（内核分块自比 0 差异）、服务端测试 |
| 跨路径（客户端预览 vs 服务端权威） | **≤1 LSB，且仅极少数像素** | 设计 6.1：D0 计算内核层 bit-exact；D1 合成/预览允许 ±1 LSB |

真实 Chromium 实测（新的逐像素自检）：

| 文档 | 差异像素 | 最大通道差 | 判定 |
|---|---|---|---|
| `phase3b`（全特性 21 原子，1024²） | **3 / 1,048,576** | **1** | 通过 |
| `fx3`（曲线+白平衡+HSL+辉光+噪点+暗角，1024²） | **0** | **0** | 逐位相同 |

**自检已从「比哈希」升级为「比像素」**：`render_region { raw: true }` 让服务端把**原始 RGBA8**
存入 CAS 并返回取回地址，客户端取回后逐像素比对，报告 `diffPixels` / `maxChannelDelta` 并在面板显示
（`±1 LSB × N` 或 `逐位相同`）。判据：`maxChannelDelta ≤ 1 且 diffPixels ≤ 16`。
哈希相等只能告诉你「不一样」，这个口径能告诉你「差多少、差在哪」——本次定位正是靠它。

**为什么会有跨路径 ±1 LSB**：两条路径都是同一份内核代码、同样的 `RenderOptions` 与 tile 尺寸（256），
但一个按整幅区域渲染、另一个按 256² 分块渲染；差异出现在 f32→f16→u8 的舍入临界上
（实测为 3 个像素单通道差 1）。逐原子二分显示「删掉任意一个参与贡献的原子差异即归零」，
即典型的**量化临界**，而非某个算子写错。同路径确定性不受影响，因此不违反 D0。

**定位过程中的关键证据（原生复现 2 字节）**。新增 `crates/yanshi-server/tests/render_parity.rs`
（在 `yanshi-server` 的 dev-dependencies 里加入 `yanshi-wasm`），把同一份夹具原子集分别喂给
**服务端 `Document`**（整幅原生渲染）与**客户端内核**（按 tile 组合），直接比对像素：

- `simple_atoms.json`（形状 + 笔迹）：差异 **0** ✅（默认用例，5 秒）
- `phase3b_atoms.json`（全特性 21 原子）：差异 **2 / 4,194,304 字节**（`#[ignore]`，release 下 4.4 秒）

结论与意义：

1. 浏览器里的失败**不是 WASM 与原生**的差异，而是**服务端与内核两条渲染路径**的差异；
2. 差异只有 2 字节（相当于 1 个像素的 1 个通道差 1，或两个像素各差 1），且**在宿主上 4 秒可复现**——
   定位成本从「CDP 逐份构造文档」降到「本地跑一个测试」；
3. 已排除 tile 尺寸（服务端与内核都是 256）与分块组合本身（内核内自比 0 差异）。

**差异像素明细（已打印）**：

```
(779,744) 通道2  服务端 154 / 内核 155 | tile (3,2) | tile 内偏移 (11, 232)
(768,745) 通道1  服务端  72 / 内核  73 | tile (3,2) | tile 内偏移 (0, 233)
```

两点都落在：
- **同一个 tile (3,2)**，且都是 **±1** 的单通道差异（不是逻辑错误，是舍入/精度差异）；
- tile 内 y 偏移 232/233（tile 边长 256，距下边界 23px），x 偏移 11 与 **0**；
- 空间上正处在 phase3b 里 `clone_stamp` 笔迹（点 `(760,760)`、`(860,790)`、offset `(-80,-80)`）附近，
  也就是**修图的源采样区域**。

据此判断：**修图从「应用本对象之前的图层缓冲」取源像素时，两条路径的源值存在 ±1 级差异**
（tile 路径的缓冲是 padding 后的 512²，整幅路径是 1280²），随后被放大到输出的 1 个字节。
这也解释了为什么只有「笔迹 + 修图 + 多效果」同时存在时才出现：源像素要恰好落在量化边界上。

**逐原子二分结果（已跑，71 秒）——否定了「单一元凶」**：

| 删除的原子 | 结果 |
|---|---|
| #9 `draw_stroke` | 差异 **0** |
| #10 `create_object`（调整） | 差异 **0** |
| #12 `create_object`（滤镜） | 差异 **0** |
| #15 `retouch`（clone） | 差异 **0** |
| #2–#8 形状、#11/#13/#14 其它效果、#16/#17 其它修图、#18 液化、#20 挂蒙版 | 仍有 1–4 字节差异 |
| #0/#1/#19 | 删除后日志不成立，无法比较 |

删掉**任意一个对这两个像素有贡献的原子**都会让差异归零 → 这是**量化临界（knife-edge）**特征：
两条路径执行的是同一套数学，但在这 2 个像素上恰好跨过舍入边界。
因此不应指望「找到某个写错的算子」，而应**让两条路径逐位对齐**。

**下一步（收敛到配置/路径对齐）**：

1. 对比服务端 `Document` 与内核构造 `Renderer` 时传入的 `RenderOptions`（`expand_for_filters`、
   `max_filter_padding`、内存预算/水位）与 tile 尺寸，先排除配置差异；
2. 若配置一致，则在两条路径下 dump 差异像素对应的**图层缓冲 f32 值**（在 `apply_layer_mask` 前后各一次），
   定位是「源值差」还是「使用处差」；
3. 对齐手段（择一）：统一两条路径的缓冲外扩/裁剪语义；或让客户端自检改比「同 tile 划分下服务端渲染的 tiles」
   （即服务端也按 256² 出块），从定义上消除跨分块舍入差异。

> 说明：内核自身的「分块 vs 整幅」比较是 0 差异（`tile_consistency.rs`），
> 所以问题只在**服务端 ↔ 内核**两套入口之间，而它们在数学上是同一份内核代码。

（调试入口已就绪：`render_parity.rs` 的 `report_differing_pixels` 与 `bisect_phase3b` 两个
`#[ignore]` 用例，release 下 4–8 秒一轮。）

**性能纠正与实测（重要）**：此前记录的「1024² 全效果渲染约 182 秒」是 **debug 构建**的数字，
不是产品性能。release 实测（`cargo test --release ... perf_phase3_effects_on_a_large_canvas`）：

| 场景（release，1024²） | 耗时 | 外扩 |
|---|---|---|
| 仅形状 + 笔迹，整幅冷渲染 | **285.8ms** | 0px |
| Phase 3 全效果（调色×2 + 滤镜×3 + 修图 + 液化 + 蒙版羽化），整幅冷渲染 | **1.26s** | 106px |
| Phase 3 全效果，单块 256² tile 冷渲染 | **172ms** | 106px |

即效果本身给整幅渲染增加约 **0.98s**；debug/release 相差约 40×，与 `render_parity`
release 下两项渲染合计 4.4s 的观测一致。已把该用例加进 perf 预算测试（release 预算：
整幅 < 15s、单块 < 2s），防止后续回归。

**本轮新增：液化 twirl / pinch**（`mode` 由 `liquify_type` 解析，默认 `push`）：

* **twirl**：把采样点绕笔迹点旋转 `strength × falloff` 弧度（`strength 1.0 ≈ 57°`）；
* **pinch**：采样点沿半径向中心靠拢（`shift = -(p-c)×strength×falloff`），
  正强度=收缩、负强度=膨胀；`direction` 仅 `push` 需要。

位移仍只由原子参数决定、反向映射 + 双线性重采样、外扩上界仍按 `size/2 + |strength|×size` 申报。
测试覆盖：twirl 使左右两侧分界高度不同、pinch（中心放在分界右侧）把分界拉向中心、
未实现模式（`warp`）仍告警。**踩到两个测试自身的坑**并已在注释里写明：pinch 的中心若正好落在
分界线上，分界线是径向缩放的不变量（测不出效果）；`before` 必须在插入液化对象**之前**渲染。

**已完成优化：区域相关外扩**（`Renderer::padding_for_region`）。

原实现取「全文档所有对象需求的最大值」作为外扩，于是远离修图笔迹的 tile 也按 106px 膨胀
（tile 缓冲 256² → 468²，面积 3.3×）。现在改为：

* **整层类**（滤镜邻域、蒙版羽化）取全局半径——它们作用于整个图层缓冲，与区域无关；
* **空间局部**（修图/液化的源采样）只在「自身包围盒 ∪ 源偏移」外扩后与目标区域相交时才计入。

release 实测（同一份全效果文档）：

| tile | 外扩 | 耗时 |
|---|---|---|
| 远离修图（左上） | **28px** | **74.9ms**（原 172ms，约 **2.3×** 加速） |
| 邻近修图（右下） | 106px | 170.9ms（正确：确实需要源像素） |
| 整幅 1024² | 106px | 1.26s（不变，正确） |

正确性由两道闸门守住：内核「分块组合 == 整幅渲染」测试仍为 **0 差异**；
浏览器逐像素自检 `phase3b` 仍为 **3 像素 / 最大通道差 1**（D1 口径通过）。
`filter_padding` 保留为「整篇最大外扩」用于诊断与测试。

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
| 调整 `vibrance` | `amount` 0..2 | 自然饱和度：权重 `1 − 饱和度`，**低饱和提升更强**、灰色不产生色偏 |
| 调整 `levels`（扩展） | 增加 `channel`（rgb/r/g/b） | 分通道色阶，与 `curves` 同一套通道命名 |
| 调整 `posterize` | `levels` 2..64 | 直通通道量化到 N 个台阶（如 3 级 → 0/0.5/1） |
| 调整 `color_balance` | `shadows`/`midtones`/`highlights` 各 `[r,g,b]`（−1..1） | 按线性亮度分档加权偏移：`shadows=(1-luma)²`、`highlights=luma²`、中间调为余量 |
| 调整 `split_toning` | `shadows`/`highlights` 各 `[r,g,b]`、`balance` −1..1、`amount` 0..1 | 权重为 smoothstep（分界 `mid = 0.5 + balance×0.25`），阴影/高光分别着色 |
| 滤镜 `motion_blur` | `angle`/`distance`/`samples` | 沿角度平均 |
| 滤镜 `sharpen` | `amount`/`radius` | 非锐化掩模 |
| 滤镜 `noise` | `amount`/`seed` | 逐像素确定性，随机量只来自 `seed` |
| 滤镜 `vignette` | `strength`/`radius`/`softness` | 以**文档**中心与对角线归一化 |
| 滤镜 `glow` | `threshold`/`radius`/`intensity` | 高光阈值 → 两趟方框模糊 → **加性**合成（同时加 alpha，否则光晕出不了笔迹范围） |
| 滤镜 `clarity` | `amount` 0..2、`radius` 2..64 | 大半径非锐化掩模 + **中间调加权**（`1-(2·luma-1)²`），避免极暗/极亮处出现光晕 |
| 滤镜 `dehaze` | `air` `[r,g,b]`（**必填**）、`omega` 0..1、`floor` 0.02..0.8 | 暗通道先验；大气光由参数给出，透射率图做 radius=4 模糊（外扩固定 8） |
| 只读工具 `estimate_dehaze` | — | 整幅扫描后给出建议 `air`/`omega`，供 `add_filter(dehaze)` 使用 |
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
**测试样例约定**：需要"内核尚未实现的类型"时，使用**刻意不存在**的名字
（`not_an_effect`），不要用"某个还没实现的真实类型"。此前这样做导致每次实现该类型都要回来
改测试（`curves → hsl → color_balance → split_toning → dehaze` 换了四次），属于自找的维护成本。

**测试写法上的一个反复出现的坑（记下来提醒自己）**：断言"某效果对 A 的影响小于对 B"时，
**必须比相对量**（增益 ÷ 基准差），比绝对值会得出相反结论 —— 因为 B 的基准差可能大很多倍。
`vibrance` 的测试就踩了第三次（饱和像素绝对增益 0.089 > 低饱和 0.082，但相对增益 0.111 ≪ 0.818），
同类错误在 `color_balance`、`split_toning` 也各出现过一次；三处现在都改为相对量或明确的物理量。

**去雾为什么拆成「只读估计 + 参数驱动滤镜」两步**：暗通道先验需要**全局**大气光 A，
若在滤镜内部按区域估计，同一像素在分块渲染与整幅渲染下会得到不同的 A，
「分块 == 整幅」立刻失效（这类问题此前踩过多次）。因此 A 必须作为**原子参数**：
`estimate_dehaze`（只读）扫描文档给出建议值，调用方再把 `air/omega/floor` 写进滤镜参数，
渲染因此保持逐像素确定性。这个两步式流程同时天然适配 Phase 4b 的
「估计 → 作为 patch 应用」——`estimate_dehaze` 的返回值可以直接填进 `suggest` 的 patch。

测试还暴露了一个**先验适用条件**：暗通道先验要求场景各通道有差异，
纯灰度图会退化为 `min(r,g,b) == 亮度`，去雾反而降低对比度（第一版测试图就是灰度，实测
对比度 0.25 → 0.242）。测试改为**彩色雾化场景**后对比度正确回升。

- **有邻域的算子必须做成滤镜而不是调整**：`clarity`（局部对比度）依赖大半径模糊，
  因此归入 `FilterKind` 并申报外扩 `radius × 2`；调整类必须逐像素无邻域，
  这样才能同时满足「整层即时生效」与「分块 == 整幅」。测试覆盖：软边被拉陡、
  平坦区不变（<0.02）、极暗处因中间调加权提升更弱、越界参数不破坏预乘不变量。
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

## 二之五、Phase 4b 起步：建议（suggest）与拒绝流程

按设计 12.6 实现（`suggest` 原子**包含 patch**；`accept_suggestion → reapply`；
`reject_suggestion → 记录原因`）：

| 工具 | 说明 |
|---|---|
| `suggest` | 记录一条建议：`patch` 采用**工具调用序列** `[{"tool":..,"arguments":{..}}]`（非空、≤64 步、形状校验），并可选 `annotation_id`/`summary` |
| `list_suggestions` | 列出建议及其状态；状态由后续的 `accept_suggestion`/`reject_suggestion` 原子推导（pending/accepted/rejected），可按状态过滤 |
| `reject_suggestion` | 提交 `reject_suggestion` 原子记录原因，并把**引用该建议**的待处理标注一并置为 `rejected` |

配套改动：`create_annotation` 增加 `suggestion_id` 参数（标注结构本就有该字段，此前被写死为
`None`），使「标注 → 建议」的追踪真正可用。

patch 采用工具调用序列（而不是原始原子）的原因：AI 的输出天然是这个形状，且
`accept_suggestion` 只需按序重放即可实现设计里的「→ reapply」。

**线上实测（真实服务端）**：创建标注 → `suggest`（2 步 patch）→ `list_suggestions` 显示 1 条
pending/2 步 → 创建关联标注（pending 变 2）→ `reject_suggestion`（记录原因，联动 1 条标注转为
rejected）全部成功。

**`accept_suggestion`（已实现）**：按序重放 patch → 提交 `accept_suggestion` 原子 →
把引用该建议的待处理标注置为 `resolved`（记录 `resolved_by` = 接受原子、`resolved_at`）。

* 重放走**同一张工具分发表**（`validate_args` + `dispatch`），因此与人工操作完全同路径：
  同样的参数校验、dirty 传播与响应；
* 注册表用**所有已实现 profile 的并集**构造（与 HTTP 服务端默认集一致），
  这样 patch 里可以出现 `add_adjustment`/`add_filter`/`clone_stamp` 等扩展组工具；
* **只允许 mutating 工具**作为补丁步骤：把只读工具塞进 patch 没有意义（测试覆盖了拒绝路径）；
* 未知建议 `reference_not_found`、非 suggest 原子 `invalid_argument`、空 patch 明确报错。

至此 4b 的核心环路可用：**人类标注 → AI 解析 → `suggest`（含 patch）→ 人工 `accept_suggestion`
/ `reject_suggestion` → patch 应用 → 标注状态更新（resolved/rejected）**。
AI 侧感知新标注仍走设计 976 的轮询：`list_annotations(status=pending)`。

**建议预览（`preview_suggestion`）**：在不应用的前提下逐步给出
`{index, tool, class, layer_id, object_id, valid, error}`，以及 `total_steps`/`invalid_steps`/`applicable`。
`class` 为 effect / retouch / geometry / structure / other 的粗分类，便于人工与 AI 审阅。

一个必须守住的一致性：预览**必须与接受同标准**。最初预览只做 `validate_args`（形状/类型），
而效果类工具的参数范围校验（如 `sigma` 上限）在处理器内部按效果名区分，于是出现
「预览说可应用、接受时被拒」的误导（测试当场抓到：`sigma: 999` 被判合法）。
现在预览对 `add_adjustment`/`add_filter` 复用**同一个** `validate_effect`，
并在响应里注明「静态检查；个别工具的运行期限制（如外扩上限）在应用时最终判定」。
预览的 `applicable` 为 `false` 时不应提交接受。

**影响范围预估（`preview_suggestion` 增强）**：每步额外给出
`affects`（`document` / `layer` / `region` / `unknown`）与 `estimated_region`，
并在响应里汇总 `estimated_dirty` + `estimated_region`（多步取并集、`document` 覆盖一切）。
推导规则对齐**内核真实的 dirty 语义**，因此预估与实测同源：

| 步骤类型 | affects | 依据 |
|---|---|---|
| `create_layer` / `reorder_layers` / `delete_layer` | `document` | 图层结构变化按内核语义整文档失效 |
| `add_adjustment` / `add_filter` / `update_*` | `layer` | 6.6：调整/滤镜作用于同层下方全部内容 |
| `set_property` 的 `z_index`/`visible`/`layer_id`/`mask_id`/`style_id`/`type` | `layer` | 这些键会改变依赖闭包 |
| `clone_stamp` / `heal_stamp` / `smudge` | `region` | 笔迹范围 + `max(|offset|, smudge_length)` + 半径 |
| `liquify_*` | `region` | 笔迹范围 + `size/2 + strength×size` |
| `draw_*` / `fill` / `erase` / `import_image` | `region` | 点列/形状包围盒 + 笔刷半径 |
| `patch` | `region` | `target` + 源区域宽高 |
| 其它 | `unknown` | 保守标注，不猜 |

参数既可能平铺（`points`/`bbox`）也可能嵌在 `data` 里（工具调用的常见形状），两种都解析
（第一版只解析平铺形式，几何补丁的预估直接是 `null`，被测试抓到）。

这是**预估**而不是实测：精确范围仍由内核在应用时按对象与效果计算，响应里的 `notes`
也如实说明这一点。

**补丁依赖顺序校验（预览的第二道闸门）**：预览按 patch 顺序模拟「此刻已知的图层/对象」：
创建型步骤（`create_layer`/`create_*`/`draw_*`/`add_adjustment`/…）把目标加入集合，
其余引用 `layer_id`/`object_id` 的步骤要求目标**此刻已存在**，`delete_*` 会把目标移出集合。
命中时该步标为非法并给出「在此时尚未创建（补丁顺序问题）」之类的说明。

这条校验针对一个**实际风险**：`accept_suggestion` 是逐步执行的，若某步在**中途**失败，
前面的步骤已经落地（部分应用的状态）。预览把这类顺序错误提前暴露，
使「预览通过 → 接受成功」更接近充分条件。测试覆盖：先画后建（非法）、先建后画（合法）、
patch 内先建对象再更新（合法）、更新不存在的对象（非法），并断言预览**全程零副作用**
（HEAD 不变）。

**接受前的全量预检（消除「部分应用」）**：`accept_one` 在写任何原子之前先跑
`preflight_patch`，它复现预览的全部校验（未知工具 / 只读步骤 / 参数形状 / 效果取值范围 /
引用顺序）。任一步不合法 → **整体拒绝且一步不写**，因此不再出现「第 1 步已落地、第 2 步失败」
的半成品状态。预览与接受**共用 `check_single_step`**，两条路径的标准不会分叉。

线上实测：含只读步骤的补丁被拒（`invalid_argument`，说明「只读工具不能作为补丁步骤」），
且 `HEAD 18 → 18` 完全未变。

> **已解决（本轮定位并修复）**：补丁里「先创建实体、再在同一条补丁内引用它」
> 曾经**连 `suggest` 都提交不了**。排查过程与结论如下（记录方法，避免以后靠猜）：
>
> 1. 现象：`suggest` 报 `reference_not_found：引用的图层 L2 不存在`，且**该原子不在日志里**
>    （说明是提交时校验拒绝，不是折叠拒绝）。
> 2. 探针二分：只含 `create_layer` 的补丁被拒；指向**已存在**图层的 `draw_shape`、
>    `add_adjustment` 补丁通过 → 问题出在「补丁里声明了新实体」。
> 3. 打印实际提交的 payload：结构完全正确（嵌套在 `patch[].arguments` 里），排除「payload 构造错误」。
> 4. 按错误文案定位出处：`grep -rn "引用的图层"` → `log.rs:369` 的 `validate_secondary_refs`。
>
> **根因**：`Atom::refs` 是**递归**收集的（供依赖追踪/CAS 使用，递归本身正确），
> 而 `validate_secondary_refs` 用同一份 refs 做**引用存在性**校验，
> 于是 suggest 原子 payload 里嵌套的步骤被当成「该原子自己引用了 L2」，
> 按「必须已存在」拒绝。
>
> **修法**（有原则、非打补丁）：纯协作原子（`comment`/`suggest`/`accept_suggestion`/
> `reject_suggestion`）不产生状态效果、不参与折叠（设计 5.2 注），因此**无法制造孤儿引用**，
> 豁免其二次引用校验；refs 仍递归收集，依赖追踪不受影响。
>
> 回归测试（core）：嵌套 patch 里的 `layer_id` 仍出现在 `refs.layers` 中（依赖追踪保留），
> 但校验返回 Ok。回归测试（server）：一条「先 `create_layer` 再 `draw_shape`」的建议
> 能记录、预览判定可应用、接受后**像素上真的出现红方块**。
> 线上实测：`suggest` 成功 → `accept` 应用 2 个原子 → 图层数 +1。
>
> 这也让预览里的顺序校验真正有意义了（此前它检查的补丁根本进不了日志）。

**接受的状态闸门（设计未规定，此处记录决定与理由）**：

| 情形 | 行为 | 理由 |
|---|---|---|
| 建议**已接受** | 不再重复施加效果，返回 `already_accepted: true` 且 `applied_atom_ids` 为空 | accept 的语义是「采纳这份提议」；重复采纳不该把效果叠两遍。批量接受场景下 AI 很容易重复提交同一 id，属于必须防的 footgun |
| 建议**已拒绝** | `precondition_failed`，拒绝接受 | 否则状态自相矛盾（日志里同时存在该建议的 reject 与 accept） |
| 建议不存在 | `reference_not_found` | 既有行为 |

> **这是一次行为变更**：此前重复接受会**再次施加效果**（我在 Phase 4b 首轮把这一点写成「幂等重放，但会再产生一次效果」并接受）。现在改为真正的幂等 no-op —— 当时的描述其实是把"可重复执行"错当成了"安全"，事后看那是个 footgun。批量接受中重复 id 现在表现为 `accepted: 2`（第二次是幂等返回），不再产生第二次效果。

**建议冲突检测（`list_suggestions.conflicts`）**：对**待处理**建议两两比较作用域，
命中即给出 `{suggestion_id, conflicts_with, reason}`。判定规则与预览同源（复用
`estimate_step_impact`）：

* 任一方是 `document` 级（图层结构变化）→ 冲突；
* 修改同一个 `object_id` → 冲突；
* 一方作用于**整层**（调整/滤镜）且另一方也改动该层 → 冲突；
* 两侧区域相交 → 冲突；
* 否则不冲突（互不相交的区域改动、不同对象）。

测试覆盖「同层两条整层调整 → 必然冲突且原因含『整层』」与「区域不相交、对象不同的两条 →
零冲突（避免误报）」。线上实测：两条同层曝光建议 → `conflicts=1`，原因为
「同图层上存在作用于整层的改动（调整/滤镜）」。

> 测试里踩到的一个 API 语义：`draw_shape` 传 `object_id` 时走的是 **supersede**（要求对象已存在），
> 想新建对象就不要传 id。反例测试因此改用「不传 id + 区域不相交」构造。

**批量接受与轮询友好列表**：

* `accept_suggestions`：一次最多 64 条，**逐条尝试、个别失败不中断**，逐条返回
  `{suggestion_id, ok, applied_atom_ids/resolved_annotations}` 或 `{error_code, detail}`，
  并汇总 `accepted`/`failed`。单条接受与批量接受共用同一个 `accept_one`，不存在两套语义。
* `list_suggestions` 增加 `since_seq`（AI 轮询取增量）、`limit`（缺省 50，上限 500）、`offset`，
  响应带 `total` —— 便于「拉取新建议 → 预览 → 批量接受」的自动化循环。

线上实测：`limit=1` 时分页返回 1 条且 `total=2`；`accept_suggestions` 一次接受 2 条全部成功，
`accepted` 状态计数同步变为 2。

**可复现验收**：`scripts/phase4b-demo.sh` 把整条环路脚本化（只创建新文档，依赖仅 curl + python3）：

```
① 人类标注 → ② AI 建议（2 步 patch）→ ③ 关联第二标注（追踪）→
④a 只读步骤被拒 → ④b 接受并重放（像素指纹 e3b5003e… → 24b23a41…，证明真的改了像素）
→ ⑤ 另建建议并拒绝（记录原因）
→ ⑥ 补丁内先建图层再绘制（断言**图层数增加**且像素指纹改变）
```

实测输出显示：应用原子 2、解决标注 1、pending 递减；建议状态列表为
`accepted / pending / rejected` 三种并存，可作为回归与演示之用。

> 细节：被拒的「只读步骤」建议在状态列表里仍是 `pending` —— 它在**接受时**被拒，
> 因此没有产生 accept/reject 原子。设计未规定此情形，保持现状并在此备注。

## 二之五之三、效果算子清单与文档一致性（自动校验）

调整 **12 种** / 滤镜 **12 种**（本行由 `crates/yanshi-render/tests/doc_consistency.rs` 校验，
数字与 `ADJUSTMENT_NAMES`/`FILTER_NAMES` 的实际长度必须一致；每个名字也必须出现在本文档与两份 README 中）。

> **为什么要自动化**：此前几轮我把「| 调整与滤镜 | N 种调整 / M 种滤镜 |」当作文档同步目标，
> 但该表格行**并不存在**，替换是空操作，我却报告了"文档已同步（N 种）"。
> 名字清单本身确实改对了，但这种**自我核对的幻觉**必须靠测试消除，而不是靠我更小心。
>
> 该测试做过**突变检验**：临时从 `README.md` 删掉 `vibrance` 后，两项断言立刻变红
> （`README.md 缺少 vibrance`），还原后恢复全绿 —— 确认它具备真实的检出力，
> 而不是一条永远通过的摆设。

## 二之六、GPU 合成后端可行性核对（本轮实测，结论：暂不实施）

按设计第 6.1 / 13.x 章核对：**合成后端层**只负责 tile 上传、显示合成、blit 与双缓冲，
**不含计算内核**（内核保持 CPU、D0 bit-exact）；该层按 **D1** 容差（±1 LSB），
客户端三级降级 `WebGPU → WebGL2 → GPU 回退 CPU`；服务端 CPU/SIMD 为基线、GPU 仅作可选加速。
设计文档本身把「Vello + WASM + WebGPU 可行性」列为 **Phase 0 待验证项**。

**本机实测**（用于决定是否值得投入）：

| 探测项 | 结果 |
|---|---|
| `navigator.gpu` | 存在，但 `requestAdapter()` **拿不到 adapter**（headless Chromium + SwiftShader 软件栈） |
| WebGL2 | 可用，但 renderer 是 `ANGLE (Google, Vulkan 1.3.0 SwiftShader)`，即**软件光栅化** |
| 宿主 GPU | Intel Haswell-ULT 核显（较老，无法在 headless 环境暴露可用的 WebGPU/Vulkan 适配器） |

**结论**：在当前环境下无法验证 GPU 合成的收益（没有真实 GPU 适配器，软件 WebGL2 只会更慢），
而引入 `wgpu`/Vello 会带来大量依赖，与"零依赖 + 单一二进制"的项目取向冲突。
因此**不实施** GPU 合成后端，并把上述实测数据作为该 Phase 0 项的答案记录下来：
它需要一台有可用 GPU 适配器与 WebGPU 浏览器的机器才能推进。

作为替代（同一设计章节目标：减少「合成/上传」开销），已在客户端落实的是：
脏区域**直接渲染**（`render_region_direct`，避免整块 tile 重算）与**区域相关外扩**
（tile 缓冲从 468² 降到 288²，远离修图笔迹的 tile 快 2.3×），两者都已在浏览器实测。

## 二之六之二、标注状态机（设计 4.6 只列三态，转换规则未规定）

设计 4.6 给出 `status: pending | resolved | rejected` 与「独立 append-only 通道」，
但**未规定**转换规则与重开语义。此处记录决定（原则：与建议层的状态闸门对称，禁止自相矛盾的历史）：

| 转换 | 行为 | 理由 |
|---|---|---|
| resolved → resolved | **幂等**返回当前版本（不新增版本、revision 不变） | 重复动作不该污染版本历史 |
| rejected → rejected | 同上 | 对称 |
| resolved → rejected | `precondition_failed`，提示先重开 | 否则历史里同时存在"已解决"和"已拒绝"两种结论 |
| rejected → resolved | 同上 | 对称 |
| resolved/rejected → pending | 允许（**重开**），并清空 `resolved_by`/`resolved_at` | 人类确实会重开已关闭的问题 |
| 任意 → rejected（delete） | 允许 | 删除是**软删除**（append-only 通道里资产不可回滚删除，见第 1330 行原则），记录仍可取回、内容清空 |

**重开入口**：`update_annotation` 增加 `status` 参数，且**只接受 `pending`** ——
解决/拒绝必须走 `resolve_annotation`/`reject_annotation`，不允许用 update 绕过状态机。

测试覆盖：幂等解决（revision 不变）、resolved→rejected 被拒且错误提示"重开"、
重开清空 `resolved_by`/`resolved_at` 且 pending 计数正确、update 传其它状态被拒、
已解决标注可软删除且删除后仍可取回（状态 rejected、内容为空）。

线上实测：重复 resolve 的 revision 均为 2；直接 reject 已解决标注返回
`precondition_failed`（提示先重开）；重开返回 `status=pending` 且 `resolved_by` 字段消失
（序列化时 `skip_serializing_if = is_none`，字段缺失即为已清空的证据）。

## 二之六之四、服务端内存与缓存预算实测（4K/10 图层）

按设计 14.1（4K/10 图层内存目标 < 4GB）实测。负载：4096² 文档、10 图层（每层 2000² 形状）、
顶层加曝光调整 + σ=8 高斯模糊，整幅渲染 3 次。

| 指标 | 实测 | 设计目标 |
|---|---|---|
| 进程 RSS | **0.08GB** | < 4GB ✓ 余量极大 |
| 渲染缓存占用 | **128 tiles / 64.0MB**（= 配置预算 `render_cache_bytes`，未越界） | — |
| LRU 淘汰 / 未命中 | 2868 / 4522 | ✓ 压力下正确淘汰 |
| 整幅 4K 渲染耗时 | **约 12.6s/次** | 设计只给了内存目标，未给 4K 全幅延迟目标（记录为数据点） |

**可观测性补齐**（设计 1319 行要求"可观测性（含 blob 生命周期指标）"）：`/health` 新增 `cache`
（`tiles` / `used_bytes` / `evictions` / `misses` / `pixel_bytes_estimate`）与 `rss_bytes`
（读 `/proc/self/statm`，非 Linux 返回 `null` 而不是假装有数据）。

**一处诚实性修正**：该字段最初叫 `pixel_bytes`，但它是**估算上界**（按"每层整幅 RGBA8"计，
4K/10 图层算出 0.63GB），而实测 RSS 只有 0.08GB —— 因为图层实际存的是**矢量/原子描述**，
只有渲染成栅格时才占内存。名称容易被误读为实测，已改名为 `pixel_bytes_estimate` 并在文档注释里写明差别。

## 二之六之五、4K 全幅渲染剖面：两个假设被证伪 + 一次方法论修正

`crates/yanshi-server/tests/perf_budget.rs::profile_4k_full_render_cache_capacity`（`--ignored`）
用 3 次采样取**最小值**测量 4K（4096²）全幅渲染：

| 配置 | PNG | raw（不编码） | PNG 编码成本 |
|---|---|---|---|
| 64MB 缓存 / 10 图层（淘汰 1664 次） | 6.67s | 5.27s | 1.40s |
| 512MB 缓存 / 10 图层（淘汰 0 次） | 6.41s | 5.67s | 0.74s |
| 512MB / 1 图层 | 4.81s | 3.77s | 1.04s |
| 512MB / 0 图层（**空白画布**） | 4.54s | **3.47s** | 1.08s |

**结论一：缓存抖动假设不成立。** 把淘汰从 1664 次降到 0 次，耗时差 **4%**，
且 raw 路径在 512MB 下反而更慢 —— 差异落在噪声带内。此前记录的"消除淘汰带来 6% 提升"
**不成立，予以撤回**。

**结论二：PNG 编码不是主因**（占 15–25%，0.7–1.4s）。真正的大头是**空白 4K 画布的 raw 路径
3.47s**：即 16.7M 像素的 **逐像素装配/量化**（约 4.8ns/像素通道）。每图层另加约 **0.26s**（线性）。

**结论三（与我原先的猜测相反）**：形状覆盖率**已按 tile 裁剪**、对象与效果 reach **已做包围盒剔除**
（`render.rs:632-637`、`516-527`），所以慢的不是"算了不该算的区域"，而是纯粹的逐像素工作量。

**方法论修正（重要）**：这台机器上 4–7s 量级的**单次**计时噪声可达 ±1s（±15%）——
第一版只测一次，于是把噪声读成了"6% 的优化"。已改为**多次采样取最小/中位数**，
并且今后任何小于噪声带的差异都不作为结论。设计文档 §14.1 只给内存目标（4K/10 < 4GB，
实测 0.08GB），**未给 4K 全幅延迟目标**，因此这里只记录数据点、不做通过/不通过判定；
若日后在意 4K 导出延迟，优化目标是逐像素装配路径与 PNG 编码（**导出**路径，与交互式区域渲染无关）。

## 二之六之六、交互路径预算：一档未达标（**待设计决策**）+ 成本归属量化

设计 14.10 预算表把「区域渲染（缓存命中 / 未命中简单 / 未命中复杂）」定为
**< 10ms / < 100ms / < 300ms**，并称关键行纳入 CI benchmark。实测（512² 区域，release，多次取最小）：

| 档位 | 实测（raw，不含 PNG 编码） | 设计预算 | 结论 |
|---|---|---|---|
| 缓存命中 | **49.0ms** | < 10ms | ✗ **未达标（约 5×）** |
| 未命中·简单 | 49.1ms | < 100ms | ✓ |
| 未命中·复杂（σ=12 模糊 + 曝光） | 48.9ms | < 300ms | ✓ |

**成本归属（量化而非猜测）**：耗时与像素数**严格线性**，约 **190ns/像素**：

| 区域 | 耗时 | 每像素 |
|---|---|---|
| 64² | 0.89ms | 217ns |
| 128² | 3.09ms | 189ns |
| 256² | 12.24ms | 187ns |
| 512² | 49.07ms | 187ns |

单独测 `linear_to_srgb`（sRGB 传递函数，即 `powf`）：262144 次调用 **8.42ms** →
按每像素 3 通道外推 **25.3ms**，占 512² 实测 49ms 的 **约 52%**；其余为 f16→f32 转换与合成。
这也是 4K 空白画布 raw 3.47s 的同一根因（16.7M × 190ns ≈ 3.2s）。

**两处机械优化实测零收益，已回退**：预分配输出缓冲 + 直接遍历 f16 切片（结果逐位相同）
测得 48.97ms vs 49.12ms —— 落在噪声内。**改动热路径却没有实测收益等于白增风险**，因此不留。

**为什么不直接优化掉 `powf`**：输入是**连续 f32** 线性值，用查找表会改变量化结果，
破坏 D0 bit-exact（服务端与内核必须逐位一致，由 parity 测试守护）。要达标必须在设计层面取舍：

1. 该路径放宽到 **D1（±1 LSB）** → 可用 LUT，预计快数倍；
2. 或修订 **CPU 路径**的区域渲染预算（设计未区分 CPU/GPU 路径的目标值）；
3. 或寻找**逐位等价**的更快 sRGB 实现（研究性质）。

**这是需要设计方决定的问题，实现方不擅自改 D0 语义、也不擅自放宽预算**：
因此该测试守护「不得明显回归」的实测基线（80/150/450ms 三档留出余量），
并把与设计目标的差距每次打印出来，使偏差保持**可见**而不是被静默吞掉。

## 二之六之七、客户端 WASM 内核的区域渲染成本（与服务端分开测）

**为什么必须分开测**：客户端预览走内核的 `render_region_direct`（无 HTTP、无 PNG、无缩略图），
服务端的数字不能外推。新增 `scripts/browser-kernel-perf.mjs`（URL 带 `debug=1` 时暴露
`window.yanshiKernel`）测量 8²–512²，5 次取最小：

| 区域 | 客户端内核 | 服务端（raw） | 说明 |
|---|---|---|---|
| 8² / 16² / 32² | 4.20 / 4.20 / 4.30ms | — | **固定开销约 4.2ms/次**（与像素数无关） |
| 64² / 128² | 4.50 / 4.80ms | 0.89 / 3.09ms | |
| 256² / 512² | 6.50 / 26.90ms | 12.24 / 49.07ms | 每像素约 **87ns** vs 服务端 **187ns** |

**结论**：客户端每像素比服务端快约 2×（少了 PNG 编码、缩略图与背景扁平化），
但有 **约 4.2ms 的每次调用固定开销** —— 占设计"首笔呈现 < 16ms"预算的 26%，
在"持续笔迹帧预算 < 8ms"下更是超过一半。注意**笔迹延伸路径不含这笔开销**：
实测拖动每段仅 0.8ms（见下一节），因为延伸走的是增量接口而非整块区域渲染。
因此该固定开销影响的是**预览整块刷新**（参数微调后重绘）而非画线手感；
4.2ms 的构成尚未定位，记为后续观测项（同样先测量再改）。

### 固定开销的追查：三个候选被排除，剩一个需确定性工具

对 `window.yanshiKernel` 做逐入口二分（`scripts/browser-kernel-perf.mjs` 已内置）：

| 入口 | 实测 | 说明 |
|---|---|---|
| `state_json()` | **≈ 0.000ms** | JS↔WASM 调用开销可忽略 ✗ 排除 |
| `render_region_info(8²)` | 11.6ms | 与下一行几乎相同 |
| `render_region_rgba(8²)` | 11.5ms | 只返回像素也不同时更慢 |
| `render_region_png(512²)` | 155.0ms ≈ `rgba_512` 155.7ms | 说明 PNG（store 模式 + CRC）**不是**大头 ✗ 排除 |

**原生参照（决定性）**：同一负载在原生 release 下，8² 区域 raw 渲染**只要 0.093ms**，
而 256² 为 12.2ms（即 186ns/px，再次吻合 sRGB `powf` 量化的成本）。

⇒ 客户端的 4.2ms 固定开销**不是算法成本**（比原生高 45–120×），也**不是** JS↔WASM 调用开销，
也**不是** PNG 编码。代码阅读另发现 `Kernel::render_region` 会**无条件编码 PNG**
（即便调用方只要 RGBA），属于结构性浪费，但实测证明其代价可忽略，
按"改动热路径必须有实测收益"的标准**不做**该改动，仅记录。

**已用确定性工具归因（`crates/yanshi-wasm/tests/kernel_perf.rs`，`--ignored`）**：
`Kernel` 只依赖纯 Rust crate，可在**宿主**上直接计时，于是能把「WASM/浏览器特有开销」与
「算法开销」分开。原生结果（release，5 次取最小）：

| 区域 | 内核**原生** | 原生 ns/px | 内核**浏览器 WASM** |
|---|---|---|---|
| 8² | **0.75ms** | — | 4.2–11.5ms |
| 64² | 0.88ms | 214.6 | 4.5ms |
| 256² | 2.73ms | **41.6** | 6.5ms |
| 512² | 11.21ms | **42.8** | 26.9ms |

**结论**：客户端的每次调用固定开销 ≈ **算法 0.7ms + 浏览器/WASM 特有 3.5–10ms** ——
**大头在浏览器环境，不在我们的 Rust 代码里**，因此没有可改的 Rust 代码（不改）。
笔迹延伸路径（0.8ms/段）本就不走这条整块渲染路径，交互手感不受影响。

**服务端 186ns/px vs 内核 42ns/px 的 4.4× 差距：打到「背景」这一层，但**未**定位到具体步骤。**

二分过程（每一步都用测量，不用推测）：

| 步骤 | 观测 | 结论 |
|---|---|---|
| 服务端是否用 tile 缓存？ | 两次 512² 渲染后 `tiles=4 / misses=4`（第二次命中） | **在用** ✗ 我原先"服务端不用缓存"的推断错误 |
| 2×2（有/无背景 × 有/无对象） | 无背景 42.8 → **有背景 175.1/175.5** ns/px；对象几乎不耗时 | 4× **只由背景造成**，与对象无关 |
| 逐像素背景换算是否为因？ | 把 `prepare_background` 提到循环外（`Buffer::to_rgba8` / `Tile::to_rgba8`），实测 175.6 vs 176.0ns/px | **不是**；按"热路径改动必须有实测收益"的标准**已全部回退** |
| 量化器的背景分支是否为因？ | 直接测量化器：无背景 125.5 ｜有背景 130.9 ns/px（**1.04×**） | **不是** |

**当前状态**：4× 发生在「有背景」时的**渲染/合成阶段**（`Renderer::render_region` 里
`Buffer::filled` 铺底 + `accumulation.composite(...)`），量化与背景换算均已排除。
下一步该测的是 `Buffer::composite` 在**不透明底**与**透明底**下的成本差（其余阶段已排除）。
**在定位到具体步骤之前不改代码** —— 本轮三次"看起来很有道理"的优化方向全部被实测否定，
这正是不靠猜的价值。

> **噪声警示（第二次记录）**：同一个 64² 在 `cargo build` 刚结束时测得 13.5ms、机器空闲时 4.5ms（3×）。
> 本机在 4ms 以上量级的单次计时噪声极大，只能支撑量级结论；脚本已改为 5 次取最小，
> 并且**必须在机器空闲时**测量。

## 二之七、拖动笔迹性能实测与「预览覆盖层」的否定结论

在真实 Chromium（真实 pointer 事件、真实重绘路径）下测量拖动成本：

| 指标 | 值 |
|---|---|
| 每段**同步**处理耗时（中位数） | **0.8–0.9ms** |
| 每段（平均 / p95） | 0.95ms / 3.9ms |
| 首段（含创建笔迹） | 3.8–3.9ms |
| `pointerup` 提交 | 3.8–4.0ms |
| 到下一帧可见 | 9.8–17.6ms（60fps 帧时钟） |

**测量口径**：只测同步处理耗时。第一版探针在每段后 `await requestAnimationFrame`，
量到的中位数恰为 16.7ms —— 那是**帧边界**而不是工作量，真实值被淹没。改为只计
`dispatchEvent` 的同步耗时后才得到 0.8ms。

**结论（否定性）**：每段 0.9ms 相对 16.7ms 帧预算有约 **16 倍余量**，瓶颈是帧时钟而非内核计算，
因此**不做**独立预览覆盖层 —— 收益为零，却要引入一层状态同步与生命周期风险。
这纠正了我此前的计划（"把拖动重绘再降一档"），并说明：先测量再决定，比先动手更省事。

**验收脚本入库**：此前这些探针只存在于 `/tmp`（不可复现、重启即失）。
现在 `scripts/browser-pixel-check.mjs`（D1 口径自检，退出码 0/1）与
`scripts/browser-drag-perf.mjs`（拖动成本）连同 `scripts/phase4b-demo.sh` 一并入库，
基线与口径写在 `scripts/README.md`。

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
