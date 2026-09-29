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
