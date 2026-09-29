# 偃师 Yanshi

> AI 原生协作绘画与设计引擎

偃师用文档状态、语义命令、可编辑对象、多级预览、版本日志和协作机制，替代传统图形界面的面板加鼠标轨迹。人类与 AI 在同一套对象模型和原子类型上协作：人类使用微观工具，AI 使用宏观语义工具。

当前仓库处于 **Phase 0 / Phase 1：核心引擎、渲染内核与无头服务端开发中**，实现范围仅限无 GUI 的 Rust 无头引擎
（`yanshi-core` 历史资产层与折叠引擎 + `yanshi-render` 渲染计算内核层 +
`yanshi-server` 服务端语义层与工具协议 + `yanshi-mcp` MCP stdio 服务器）。

## 设计文档

- [docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) —— 冻结设计文档 v1.0-draft4（中文）。这是全部语义与协议的权威定义；代码不得与设计文档静默分叉，行为变更必须同时更新设计文档。
- [docs/design/implementation-notes.md](docs/design/implementation-notes.md) —— 实现说明：模块与章节对应表、文档留白处的实现级明确化、尚未实现清单、验收载体（测试 ↔ 文档要求）。

## 当前状态

本仓库当前只交付 **核心引擎的 Phase 0 / Phase 1 内容**，正在开发中：

- **原子日志**：append-only，客户端生成 ULID 保证幂等与重试安全，服务端分配权威 `seq`。
- **折叠求值**：按 `seq` 线性扫描维护有效原子链，`revert` 级联失效，`reapply` 只恢复目标原子（不恢复级联链）。
- **state@seq**：按公式 `state@seq_n := fold(A_n, base_state(H_n))` 求值任意历史状态，`declare_head` 统一切换求值起点。
- **Blob CAS 三级生命周期**：活跃 / 历史 / 孤儿三级；GC 根集 = 全日志引用闭包，只清理从未被日志引用的孤儿，历史 blob 永不因 GC 丢失。
- **属性测试**：折叠代数不变量（幂等性、收敛性、无孤儿引用、revert-reapply 往返、历史可重放）以属性测试和 fuzz 覆盖。

尚未实现、属于路线图规划中的部分包括：渲染器与合成后端、服务端与 HTTP/WebSocket/MCP 传输层、WASM 计算内核、Web 编辑器、修图与语义工具、插件系统、标注通道。README 中不描述其为可用能力。

## 仓库结构

```
yanshi/
├── deploy/                   # 部署产物：systemd user 服务 + Omarchy/Hyprland 桌面入口
├── crates/
│   ├── yanshi-core/          # 核心引擎：原子日志、折叠求值、状态、Blob CAS
│   ├── yanshi-render/        # 渲染计算内核层：D0 CPU 基线、tile、dirty、缩略图、PNG
│   ├── yanshi-server/        # 服务端语义层：文档服务、Job、capability token、广播、标注、27 工具
│   ├── yanshi-http/          # 零依赖 HTTP/1.1 + WebSocket 传输层与最小 Web 查看器
│   └── yanshi-mcp/           # MCP stdio 服务器（JSON-RPC over stdio）
├── docs/
│   └── design/               # 冻结设计文档、实现说明与修订历史
├── .github/workflows/ci.yml  # 持续集成：fmt / clippy / test / 长时 fuzz
├── CONTRIBUTING.md
└── LICENSE                   # MIT
```

- `crates/yanshi-core`：无 GUI 依赖的历史资产层与折叠引擎。
- `crates/yanshi-render`：设计文档 6.1 的**计算内核层**参考实现（纯 CPU、D0 bit-exact），
  客户端与服务端共享同一份代码；合成后端层（GPU/tile 上传）尚未实现。

## 快速开始

需要 Rust stable 1.85 或更高版本。

```bash
cargo test --workspace                  # 单元测试 + 属性测试 + Phase 0 出口用例
cargo test --workspace --release -- --ignored   # 10 万原子折叠 fuzz + 渲染性能预算
cargo run -p yanshi-core --example quickstart   # 端到端示例：提交 → 撤销 → 时间旅行 → GC
cargo run -p yanshi-render --example render_demo  # 渲染示例：输出 PNG 到 target/render-demo/
cargo run -p yanshi-mcp -- --list-tools         # 列出 MCP 工具清单（JSON）

# 启动零依赖 HTTP/WebSocket 服务端 + 最小 Web 查看器
cargo run -p yanshi-http --bin yanshi-serve -- --bind 127.0.0.1:8080 --root ./workspace
# 浏览器打开 http://127.0.0.1:8080/ ，页面会用 POST /api/documents 自动取得 capability token
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

> 长时 fuzz 默认被 `#[ignore]` 标记，CI 以 `--ignored` 单独执行（`.github/workflows/ci.yml` 的 `fuzz` 作业）。

### 通过 MCP 使用（Agent 接入）

`yanshi-mcp` 是一个 MCP stdio 服务器：每行一个 JSON-RPC 消息，本地进程豁免鉴权（12.7）。

```bash
# 内存模式，默认文档 default（1024×1024）
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' | cargo run -q -p yanshi-mcp

# 持久化到 ./workspace，暴露标注与历史工具组
cargo run -q -p yanshi-mcp -- --root ./workspace --doc demo --profile core,annotation,history
```

典型调用序列（10.1 返回 `atom_id`/`seq`/`preview.thumb_url`）：

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"create_layer","arguments":{"layer_id":"layer_1"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"draw_stroke","arguments":{
  "layer_id":"layer_1","data":{"points":[[40,40],[400,300]],"size":12,"color":{"r":40,"g":40,"b":60,"a":255}}}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"render_region","arguments":{
  "region":{"x":0,"y":0,"w":256,"h":256},"include_image":true}}}
```

### 桌面入口（Omarchy / Hyprland）

客户端就是 Web 编辑器，所以桌面侧不需要任何原生 GUI 工具包：把服务端做成 systemd user
服务，再用 Omarchy 自带的 `omarchy-webapp-install` 注册成 web app（`chromium --app`），
最后加一个键位。完整步骤与实测记录见 **[deploy/omarchy/README.md](deploy/omarchy/README.md)**。

```bash
cargo build --release -p yanshi-http
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user enable --now yanshi-serve     # 回环 8110，开机自启
deploy/omarchy/make-icon.sh                    # 图标也由引擎自己渲染
omarchy-webapp-install "Yanshi" "http://127.0.0.1:8110/?doc=yanshi" deploy/icons/yanshi.png
# 在 ~/.config/hypr/bindings.lua 追加：
#   o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
hyprctl reload && hyprctl configerrors         # 期望：ok / 空
```

### 通过 HTTP / WebSocket 使用

`yanshi-serve` 只用 `std::net` 实现 HTTP/1.1 与 RFC 6455 WebSocket（含手写 SHA-1 握手、
帧编解码/分片/ping-pong），没有网络依赖：

```bash
# 1) 打开/新建文档 → 拿到 capability token 与 URL（12.7）
curl -s -X POST http://127.0.0.1:8080/api/documents \
     -d '{"doc_id":"demo","width":1024,"height":1024}'
# {"ok":true,"doc_id":"demo","token":"<64 hex>","url":"/?doc=demo&token=..."}

# 2) 调工具（token 走 Authorization: Bearer 或 ?token=）
curl -s -X POST "http://127.0.0.1:8080/api/tools/draw_stroke?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","data":{"points":[[40,40],[400,300]],"size":12,
          "color":{"r":40,"g":40,"b":60,"a":255}}}'

# 3) 渲染区域并从 CAS 取回 PNG（响应里的 thumb_url 已改写成可直接 GET 的地址）
curl -s -X POST "http://127.0.0.1:8080/api/tools/render_region?doc=demo&token=$TOKEN" \
     -d '{"region":{"x":0,"y":0,"w":256,"h":256}}'
```

WebSocket（`ws://127.0.0.1:8080/ws?doc=demo&token=$TOKEN`）按 6.8 的边界推送：
**控制流**（全部原子元数据）全局广播，**数据流**（tile/缩略图）按订阅视口过滤。
MCP stdio 不做推送，改用 `get_log` / `get_job` / `get_render_status` 轮询。

#### 颜色写法（工具层统一约定）

| 写法 | 含义 |
|---|---|
| `[r,g,b]` / `[r,g,b,a]`，分量 ≤ 1 | 直通线性 |
| `[r,g,b,a]`，任一分量 > 1 | sRGB 字节 0-255（与 `{"r":…}` 等价） |
| `{"r":0-255,"g":…,"b":…,"a":…}` | sRGB 字节（`a` 缺省 255） |
| `"#RRGGBB"` / `"#RRGGBBAA"` | sRGB 十六进制 |

非法颜色在工具层即被拒绝（`invalid_argument`），不会写入原子日志。

## 设计要点

- **原子日志**：append-only，不删除、不修改；客户端 ULID 幂等，服务端 `seq` 为唯一权威全序。`parents` 只用于因果审计，不参与排序。
- **折叠求值**：原子按 `seq` 线性扫描，维护每个对象的有效原子链；`revert` 级联失效，依赖被撤销原子的后续原子记录 `cascade_invalidation` 警告后跳过；`reapply` 只恢复目标原子，级联链需显式重新提交。
- **state@seq 与 declare_head**：`state@seq_n := fold(A_n, base_state(H_n))`，`H_n` 为 `seq ≤ n` 中最近一次 `declare_head`。`declare_head` 是重型原子，统一实现时间旅行、`revert_to`、`restore_checkpoint`，提交后触发快照与全量 tile 失效，日志始终保持 append-only。
- **Blob CAS 三级生命周期**：活跃（当前 HEAD 折叠状态引用，热存储）、历史（被日志任意原子引用但不在当前状态，冷归档 + zstd，保留可回取）、孤儿（上传成功但从未被引用，TTL 7 天后清理）。GC 根集 = 全日志引用闭包，快照产出的活跃 Manifest 只用于冷热迁移标记，不是 GC 根集。
- **确定性分级**：D0 计算内核层 CPU bit-exact 基线；D1 合成后端层、预览、缩略图允许 ±1 LSB；D2 插件与外部服务允许差异。计算内核层是唯一权威来源。
- **广播边界**：控制流（全部原子元数据）全局广播，客户端折叠需要完整原子元数据；数据流（tile 位图、缩略图、blob 二进制）按视口订阅过滤或按需拉取。
- **工具暴露分层**：核心层 27 个工具默认注册，扩展组按 `profile` 参数启用，以控制 Agent 的函数选择负担与 token 开销。
- **渲染计算内核层（D0）**：`yanshi-render` 用纯 CPU 标量 `f32` 实现合成、笔触 stamping、覆盖率光栅化、
  调整与滤镜、位图补丁；内存 tile 为 f16 线性预乘；随机量只来自原子 `seed`，因此逐位可复现。
  tile 分块与缓存淘汰不影响像素（属性测试逐字节校验）。
- **双 dirty 传播**：几何 dirty 取对象包围盒并集，结构 dirty 通过依赖图闭包（同层上方对象、
  实例 master、组成员）计算；失效 tile 集合必须覆盖所有变化像素，判定不了时宁可放大范围。
- **缩略图与导出**：doc/layer/object/history/selection 分级尺寸、32×32 分块增量更新、
  宽高比适配；PNG 使用零依赖确定性编码器（WebP/AVIF 属传输层，尚未实现）。
- **零依赖传输层与最小 Web 查看器**：`std::net` 上手写 HTTP/1.1 与 RFC 6455（自实现 SHA-1 握手、
  帧编解码/掩码校验/分片/ping-pong/16 MiB 上限），token 走 `Bearer` 头或查询串，
  `yanshi://blob/<hash>` 自动改写为带 token 的可 GET 地址；单页查看器支持画笔/矩形/椭圆/橡皮、
  撤销重做、区域预览、缩略图与控制流日志面板。
- **无头服务端与工具层**：提交校验 → 权威 seq → 增量折叠 → 双 dirty → 控制流广播 → Job → 快照；
  核心层 27 个工具默认注册、扩展组按 `profile` 启用；`batch` 内共享变更集；
  采样性替换冲突自动创建冲突图层并返回 `conflict_layer_id`；
  标注走独立 append-only 通道；capability token 保护 HTTP/WS 而豁免 stdio；
  MCP stdio 走「提交 + 轮询」，原子 JSONL + CAS + 渲染缓存持久化，重启即恢复。
- **许可证**：MIT。

## 路线图

| 阶段 | 内容摘要 | 状态 |
|---|---|---|
| Phase 0：技术验证 | 折叠引擎原型、代数属性测试与 fuzz、CPU D0 基线渲染器、Blob CAS 竞态与三级生命周期 / GC 原型、液化方案与 WebGPU 可行性验证 | 进行中 |
| Phase 1：原子核心 + 折叠 + 服务端渲染 | 原子模型与 append-only 日志、ULID 幂等、权威 seq、折叠求值与级联失效、state@seq、Blob CAS 提交顺序协议、图层隔离与 Tile 分块、服务端 CPU 渲染、核心层 27 工具、Job 协议、capability token、广播边界、HTTP/WS 传输、最小 Web 查看器 | 已完成（WASM/SIMD 内核与 GPU 合成属 Phase 2+） |
| Phase 2：WASM 核心 + WS 协作 + 本地乐观渲染 | WASM 计算内核层、控制流/数据流分离的 WS 广播、本地乐观渲染与 tile cross-fade 校正、L3/L4 缓存、异步渲染、import_image、Job 协议 | 规划中 |
| Phase 3：基础修图 + GPU 合成 + 通用笔刷 | GPU 合成后端、通用光栅笔刷与风格系统、clone/heal/patch 与基础液化调色、检查点与历史浏览、冲突处理与 resolve_conflict 组合宏、AI 语义工具 | 规划中 |
| Phase 4a / 4b：标注基础 / 标注 AI 解析与建议 | 标注独立通道与 CRUD、标注可视化；AI 解析标注、生成建议、接受/拒绝流程 | 规划中 |
| Phase 5：插件 + 高级功能 | WASM 插件沙箱与能力模型、实例与组引用、高级路径编辑、owner/editor/viewer 权限 | 规划中 |

## 测试与验收

| 命令 | 覆盖 |
|---|---|
| `cargo test --workspace` | 单元测试（原子/日志/折叠/state@seq/快照/Blob CAS/冲突/渲染内核/服务端/工具层/SHA-1 与 WS 帧/HTTP 路由/查看器）+ 5.3 五条不变量属性测试 + Phase 0 极端序列与 GC 可回放性 + 渲染的 D0 确定性、分块无关性、dirty 覆盖、缩略图增量属性测试 + 服务端端到端（冲突图层、Job、广播、标注、时间旅行、鉴权、GC、颜色与缩略图回归）+ MCP stdio 子进程往返与重启恢复 + 原始 TCP 的 HTTP/WS 集成（含握手 Accept、鉴权 403、视口过滤、ping/pong、404/405） |
| `cargo test --workspace --release -- --ignored` | 10 万原子折叠 fuzz（Phase 0 出口条件）：9.8 万原子、约 930 次 `declare_head` 跳变、回收窗口重放；渲染性能预算（8.5 / 14.10）与 overdraw / tile 命中率统计 |
| `cargo doc --workspace --no-deps` | 无 rustdoc 警告（`missing_docs` 已开启） |

设计文档与测试的对应关系见 [docs/design/implementation-notes.md](docs/design/implementation-notes.md) 的“验收载体”一节。

## 贡献

欢迎贡献，请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。提交前必须保证：

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

三者全部通过，且新增折叠或 GC 语义必须附带属性测试。

## 许可证

本项目以 [MIT 许可证](LICENSE) 发布。

Copyright (c) 2026 The Yanshi Authors

## English Abstract

Yanshi is an AI-native collaborative painting and design engine. It replaces the traditional panel-and-mouse-trajectory GUI with document state, semantic commands, editable objects, multi-level previews, a versioned append-only atom log, and collaboration primitives, so that humans and AI agents work on one shared object model — humans through fine-grained tools, AI through high-level semantic tools.

The repository is currently in **Phase 0 / Phase 1**. Implemented: the headless core engine (`crates/yanshi-core`) with the append-only atom log (client ULIDs, server-authoritative seq), fold evaluation with cascading revert invalidation and reapply, the `state@seq` formula with `declare_head`, the three-tier Blob CAS lifecycle with a GC root set equal to the full-log reference closure, and property/fuzz tests; the render compute kernel (`crates/yanshi-render`) with f16 linear tiles, premultiplied blending, brush stamping, shapes, adjustments/filters, geometry/structure dirty propagation, thumbnails and a dependency-free PNG encoder; the headless server (`crates/yanshi-server`) with document service, Job protocol, capability tokens, control/data-flow broadcast boundaries, an append-only annotation channel, file persistence and the 27 core tools with profile-based exposure; an MCP stdio server (`crates/yanshi-mcp`); and a dependency-free HTTP/1.1 + RFC 6455 WebSocket transport with a minimal single-page web viewer (`crates/yanshi-http`, hand-written SHA-1 handshake, frame codec with fragmentation and ping/pong, capability tokens over `Bearer`/query string, control-flow-global vs viewport-filtered data-flow push).

Not implemented yet (roadmap): the WASM compute kernel and SIMD/GPU paths, the full web editor, retouch and semantic tools, the plugin sandbox, and desktop GUI work. The frozen design document is `docs/design/yanshi-v1.0-draft4.md` (Chinese). Licensed under MIT.
