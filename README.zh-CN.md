# 偃师 Yanshi

**中文** | [English](README.md)

<img src="assets/brand/svg/logo-horizontal-cn.svg" alt="偃师 Yanshi" width="320" />

> AI 原生协作绘画与设计引擎

偃师用文档状态、语义命令、可编辑对象、多级预览、版本日志和协作机制，替代传统图形界面的面板加鼠标轨迹。人类与 AI 在同一套对象模型和原子类型上协作：人类使用微观工具，AI 使用宏观语义工具。

当前仓库已完成 **Phase 0 / Phase 1 / Phase 2 的无头部分**：核心引擎、渲染计算内核、无头服务端与工具协议、
零依赖 HTTP/WebSocket 传输层、最小 Web 查看器，以及浏览器端 **WASM 计算内核与本地乐观渲染**。

```bash
cargo test --workspace                 # 322 个测试
cargo run -p yanshi-http --bin yanshi-serve -- --root ./workspace
# 浏览器打开 http://127.0.0.1:8080/ ，页面会用 POST /api/documents 自动取得 capability token
```

## 设计文档

- [docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) —— 冻结设计文档 v1.0-draft4（中文）。这是全部语义与协议的权威定义；代码不得与设计文档静默分叉，行为变更必须同时更新设计文档。
- [docs/design/implementation-notes.md](docs/design/implementation-notes.md) —— 实现说明：模块与章节对应表、文档留白处的实现级明确化、实测性能数据、已知限制、尚未实现清单、验收载体（测试 ↔ 文档要求）。

## 当前状态

| 层 | 状态 | 说明 |
|---|---|---|
| `yanshi-core` 核心引擎 | ✅ | append-only 原子日志（客户端 ULID 幂等 + 服务端权威 `seq`）、折叠求值与级联失效、`state@seq` 与 `declare_head`、逻辑快照、Blob CAS 三级生命周期与 GC |
| `yanshi-render` 渲染计算内核 | ✅ | D0 CPU bit-exact：f16 线性 tile、预乘混合、笔触 stamping、形状、调整/滤镜、双 dirty 传播、分级缩略图、零依赖确定性 PNG |
| `yanshi-server` 服务端语义层 | ✅ | 文档服务、Job 协议（TTL/取消/轮询）、capability token、控制流/数据流广播边界、标注独立通道、文件持久化、核心 27 工具 + 扩展组（启用全部已实现工具组共 49 个） |
| `yanshi-http` 传输层与查看器 | ✅ | 零依赖 HTTP/1.1 + RFC 6455（手写 SHA-1 握手、帧编解码、分片、ping/pong）、连接复用、最小 Web 查看器 |
| `yanshi-mcp` | ✅ | MCP stdio（`initialize` / `tools/list` / `tools/call` / `ping`），profile 分层，提交 + 轮询 |
| `yanshi-wasm` 浏览器计算内核 | ✅ | wasm32 构建，本地增量折叠 + 乐观渲染、待提交覆盖层、LRU tile 内存池与 90% 水位兜底 |
| 桌面入口 | ✅ | systemd user 服务 + Omarchy web app + `SUPER + ALT + Y`（见 [deploy/omarchy/README.md](deploy/omarchy/README.md)） |

**Phase 2 出口条件已在真实 Chromium 中验收**：

| 出口条件 | 结果 |
|---|---|
| 客户端与服务端 CPU 路径 bit-exact | ✅ 本地 WASM 渲染 PNG 的 SHA-256 == 服务端 `blob_hash` |
| 首笔呈现延迟 < 16ms | ✅ 实测 **6.2ms**（增量盖章路径），且一笔只产生一个原子 |

尚未实现、属于路线图规划中的部分见下方路线图；README 中不把未实现能力描述为可用。

## 仓库结构

```
yanshi/
├── assets/brand/             # 品牌资源：9 个源 SVG + render.sh（派生图标/favicon/logo 各尺寸）
├── deploy/                   # 部署产物：systemd user 服务 + Omarchy/Hyprland 桌面入口
├── crates/
│   ├── yanshi-core/          # 核心引擎：原子日志、折叠求值、状态、Blob CAS
│   ├── yanshi-render/        # 渲染计算内核层：D0 CPU 基线、tile、dirty、缩略图、PNG
│   ├── yanshi-server/        # 服务端语义层：文档服务、Job、token、广播、标注、27 工具
│   ├── yanshi-http/          # 零依赖 HTTP/1.1 + WebSocket 传输层与最小 Web 查看器
│   ├── yanshi-wasm/          # 计算内核的 WASM 绑定（浏览器端本地乐观渲染）
│   └── yanshi-mcp/           # MCP stdio 服务器（JSON-RPC over stdio）
├── docs/design/              # 冻结设计文档、实现说明与修订历史
├── .github/workflows/ci.yml  # 持续集成：fmt / clippy / test / 长时 fuzz
├── CONTRIBUTING.md
└── LICENSE                   # MIT
```

## 快速开始

需要 Rust stable 1.85 或更高版本。

```bash
cargo test --workspace                  # 单元测试 + 属性测试 + Phase 0 出口用例
cargo test --workspace --release -- --ignored   # 10 万原子折叠 fuzz + 渲染/服务端性能预算
cargo run -p yanshi-core --example quickstart   # 端到端示例：提交 → 撤销 → 时间旅行 → GC
cargo run -p yanshi-render --example render_demo  # 渲染示例：输出 PNG 到 target/render-demo/
cargo run -p yanshi-mcp -- --list-tools         # 列出 MCP 工具清单（JSON）
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

> 长时 fuzz 默认被 `#[ignore]` 标记，CI 以 `--ignored` 单独执行（`.github/workflows/ci.yml` 的 `fuzz` 作业）。

### 零依赖 HTTP/WebSocket 服务端 + 最小 Web 查看器

```bash
# 启动服务端（回环 8080；加 --no-wasm / --no-brand 可关掉 WASM 内核与品牌资源）
cargo run -p yanshi-http --bin yanshi-serve -- --bind 127.0.0.1:8080 --root ./workspace
# 浏览器打开 http://127.0.0.1:8080/ ：页面自动取得 capability token 并连上 WebSocket
```

* **本地乐观渲染**：浏览器加载 `yanshi_wasm.wasm`（与服务端同一份 Rust 计算内核），
  拖动时只把**新增笔段**增量盖章到已缓存 tile，落笔才异步提交一个原子，随后按服务端权威状态校正。
* **一致性自检**：界面上的「一致性自检」按钮会比对本地渲染 PNG 与服务端 blob 的 SHA-256。
* WASM 产物构建：`cargo build -p yanshi-wasm --target wasm32-unknown-unknown --release` +
  `wasm-bindgen --target web --out-dir crates/yanshi-wasm/pkg --no-typescript <wasm>`；
  产物缺失时查看器自动降级为纯服务端渲染。

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

### 通过 HTTP / WebSocket 使用

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

# 4) 客户端自带 ULID 的原子提交（幂等重试安全；客户端乐观渲染的入口）
curl -s -X POST "http://127.0.0.1:8080/api/atoms?doc=demo&token=$TOKEN" \
     -d '{"id":"01J...","kind":"create_layer","actor":"human:web","session":"s",
          "timestamp":1,"payload":{"layer_id":"layer_1"}}'
```

WebSocket（`ws://127.0.0.1:8080/ws?doc=demo&token=$TOKEN`）按 6.8 的边界推送：
**控制流**（完整原子）全局广播，**数据流**（tile/缩略图）按订阅视口过滤。
MCP stdio 不做推送，改用 `get_log` / `get_job` / `get_render_status` 轮询。

#### 颜色写法（工具层统一约定）

| 写法 | 含义 |
|---|---|
| `[r,g,b]` / `[r,g,b,a]`，分量 ≤ 1 | 直通线性 |
| `[r,g,b,a]`，任一分量 > 1 | sRGB 字节 0-255（与 `{"r":…}` 等价） |
| `{"r":0-255,"g":…,"b":…,"a":…}` | sRGB 字节（`a` 缺省 255） |
| `"#RRGGBB"` / `"#RRGGBBAA"` | sRGB 十六进制 |

非法颜色在工具层即被拒绝（`invalid_argument`），不会写入原子日志。

#### 调色与滤镜（`retouch` 组）

```bash
# 调整：brightness_contrast / saturation / invert / levels
curl -s -X POST "http://127.0.0.1:8080/api/tools/add_adjustment?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","adjustment_type":"saturation","params":{"amount":1.6}}'
# 滤镜：box_blur / gaussian_blur / brightness_contrast / saturation / invert
curl -s -X POST "http://127.0.0.1:8080/api/tools/add_filter?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1","filter_name":"gaussian_blur","params":{"sigma":4.0}}'
```

效果只作用于**同层下方**内容，因此两个工具缺省把新对象放到该层最上方（也可显式给 `z_index`）。
调整在线性光里计算（sRGB 字节 128 反相得 229，不是 127）。未实现的类型与越界参数一律
`invalid_argument`，`list_effects` 会回带内核支持的名字清单。

### 桌面入口（Omarchy / Hyprland）

客户端就是 Web 编辑器，所以桌面侧不需要任何原生 GUI 工具包：把服务端做成 systemd user
服务，再用 Omarchy 自带的 `omarchy-webapp-install` 注册成 web app（`chromium --app`），
最后加一个键位。完整步骤与实测记录见 **[deploy/omarchy/README.md](deploy/omarchy/README.md)**。

```bash
cargo build --release -p yanshi-http
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user enable --now yanshi-serve     # 回环 8110，开机自启
assets/brand/render.sh                         # 从品牌 SVG 生成图标/favicon 各尺寸
omarchy-webapp-install "Yanshi" "http://127.0.0.1:8110/?doc=yanshi" deploy/icons/yanshi.png
# 在 ~/.config/hypr/bindings.lua 追加：
#   o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
hyprctl reload && hyprctl configerrors         # 期望：ok / 空
```

## 品牌资源

`assets/brand/svg/` 是九个源 SVG（横版/竖版/主标 logo、深浅图标、favicon、极简版、单色版），
是品牌资产的唯一来源；`assets/brand/render.sh` 用 `rsvg-convert` 生成产品实际使用的尺寸
（桌面图标 256/512、favicon 16/32/180 与 `.ico`、README 与文档用 logo）。
服务端把它们托管在 `/favicon.svg`、`/favicon.png`、`/favicon.ico` 与 `/brand/{file}`（白名单）。

## 设计要点

- **原子日志**：append-only，不删除、不修改；客户端 ULID 幂等，服务端 `seq` 为唯一权威全序。`parents` 只用于因果审计，不参与排序。
- **折叠求值**：原子按 `seq` 线性扫描，维护每个对象的有效原子链；`revert` 级联失效，依赖被撤销原子的后续原子记录 `cascade_invalidation` 警告后跳过；`reapply` 只恢复目标原子，级联链需显式重新提交。
- **state@seq 与 declare_head**：`state@seq_n := fold(A_n, base_state(H_n))`，`H_n` 为 `seq ≤ n` 中最近一次 `declare_head`。`declare_head` 是重型原子，统一实现时间旅行、`revert_to`、`restore_checkpoint`，提交后触发快照与全量 tile 失效，日志始终保持 append-only。
- **Blob CAS 三级生命周期**：活跃（当前 HEAD 折叠状态引用，热存储）、历史（被日志任意原子引用但不在当前状态，冷归档 + zstd，保留可回取）、孤儿（上传成功但从未被引用，TTL 7 天后清理）。GC 根集 = 全日志引用闭包，快照产出的活跃 Manifest 只用于冷热迁移标记，不是 GC 根集。
- **确定性分级**：D0 计算内核层 CPU bit-exact 基线；D1 合成后端层、预览、缩略图允许 ±1 LSB；D2 插件与外部服务允许差异。计算内核层是唯一权威来源。
- **广播边界**：控制流（完整原子）全局广播，客户端折叠需要全部原子；数据流（tile 位图、缩略图、blob 二进制）按视口订阅过滤或按需拉取。
- **工具暴露分层**：核心层 27 个工具默认注册，扩展组按 `profile` 参数启用，以控制 Agent 的函数选择负担与 token 开销。
- **渲染计算内核层（D0）**：`yanshi-render` 用纯 CPU 标量 `f32` 实现合成、笔触 stamping、覆盖率光栅化、
  调整与滤镜、位图补丁；内存 tile 为 f16 线性预乘，输出像素以 f16 tile 为准；随机量只来自原子 `seed`，
  因此逐位可复现。tile 分块、缓存淘汰与覆盖率裁剪不影响像素（属性测试逐字节校验）。
- **双 dirty 传播**：几何 dirty 取对象包围盒并集，结构 dirty 通过依赖图闭包（同层上方对象、
  实例 master、组成员）计算；失效 tile 集合必须覆盖所有变化像素，判定不了时宁可放大范围。
- **缩略图与导出**：doc/layer/object/history/selection 分级尺寸、32×32 分块增量更新、
  宽高比适配；PNG 使用零依赖确定性编码器（WebP/AVIF 属传输层，尚未实现）。
  文档级缩略图与 HEAD 严格对应（落后即重算），保证「打开即图片」显示当前画面。
- **零依赖传输层与最小 Web 查看器**：`std::net` 上手写 HTTP/1.1 与 RFC 6455（自实现 SHA-1 握手、
  帧编解码/掩码校验/分片/ping-pong/16 MiB 上限），连接复用（keep-alive），token 走 `Bearer` 头或查询串，
  `yanshi://blob/<hash>` 自动改写为带 token 的可 GET 地址；单页查看器支持画笔/矩形/椭圆/橡皮、
  撤销重做、区域预览、缩略图、控制流日志面板与一致性自检。
- **无头服务端与工具层**：提交校验 → 权威 seq → 增量折叠 → 双 dirty → 控制流广播 → Job → 快照；
  核心层 27 个工具默认注册、扩展组按 `profile` 启用；`batch` 内共享变更集；
  采样性替换冲突自动创建冲突图层并返回 `conflict_layer_id`；
  标注走独立 append-only 通道；capability token 保护 HTTP/WS 而豁免 stdio；
  MCP stdio 走「提交 + 轮询」，原子 JSONL + CAS + 渲染缓存持久化，重启即恢复。
- **WASM 计算内核与本地乐观渲染**：与服务端共享同一份计算内核（D0 bit-exact）；
  本地增量折叠 + tile 失效 + 视口联动；LRU tile 内存池有硬上限，达到 90% 水位自动淘汰（不依赖 JS 调用），
  另暴露 `evict_outside_viewport` 供 JS 主动淘汰；拖动中的笔迹走**本地待提交覆盖层**（不进原子日志），
  落笔才提交并接受服务端权威状态校正。
- **许可证**：MIT。

## 路线图

| 阶段 | 内容摘要 | 状态 |
|---|---|---|
| Phase 0：技术验证 | 折叠引擎原型、代数属性测试与 fuzz、CPU D0 基线渲染器、Blob CAS 竞态与三级生命周期 / GC 原型、液化方案与 WebGPU 可行性验证 | ✅ 已完成 |
| Phase 1：原子核心 + 折叠 + 服务端渲染 | 原子模型与 append-only 日志、ULID 幂等、权威 seq、折叠求值与级联失效、state@seq、Blob CAS 提交顺序协议、图层隔离与 Tile 分块、服务端 CPU 渲染、核心层 27 工具、Job 协议、capability token、广播边界、HTTP/WS 传输、最小 Web 查看器 | ✅ 已完成 |
| Phase 2：WASM 核心 + WS 协作 + 本地乐观渲染 | WASM 计算内核层、控制流/数据流分离的 WS 广播、本地乐观渲染、WASM LRU 内存池与视口联动、Job 协议、import_image | ✅ 已完成（L3/L4 缓存与 cross-fade 校正待补） |
| Phase 3：基础修图 + GPU 合成 + 通用笔刷 | GPU 合成后端、通用光栅笔刷与风格系统、clone/heal/patch 与基础液化调色、检查点与历史浏览、冲突处理与 resolve_conflict 组合宏、AI 语义工具 | 进行中 —— 调色与滤镜已完成（`retouch` 组的 `add_adjustment` / `add_filter` / `update_*` / `list_effects`）；修图与液化在内核里仍是 `Primitive::Unsupported`，因此不注册空壳工具 |
| Phase 4a / 4b：标注基础 / 标注 AI 解析与建议 | 标注独立通道与 CRUD、标注可视化；AI 解析标注、生成建议、接受/拒绝流程 | 4a 通道与 CRUD 已完成，4b 规划中 |
| Phase 5：插件 + 高级功能 | WASM 插件沙箱与能力模型、实例与组引用、高级路径编辑、owner/editor/viewer 权限 | 规划中 |

## 测试与验收

| 命令 | 覆盖 |
|---|---|
| `cargo test --workspace` | 322 个测试：核心引擎（原子/日志/折叠/state@seq/快照/Blob CAS/冲突）、渲染内核（笔触/形状/调整滤镜/dirty/缩略图/PNG/颜色/覆盖率裁剪/增量盖章）、服务端（文档服务/Job/token/广播/标注/持久化/工具层）、传输层（HTTP 路由/keep-alive/鉴权/WASM 与品牌资源托管）、MCP stdio、WASM 内核（增量折叠/覆盖层/水位兜底），以及 5.3 五条不变量与 D0 确定性的属性测试 |
| `cargo test --workspace --release -- --ignored` | 10 万原子折叠 fuzz（Phase 0 出口条件）、渲染性能预算（8.5 / 14.10）、overdraw / tile 命中率、服务端 view 模式打开 < 100ms、覆盖率裁剪预算 |
| `cargo doc --workspace --no-deps` | 无 rustdoc 警告（`missing_docs` 已开启） |

设计文档与测试的对应关系、实测性能数据与已知限制见
[docs/design/implementation-notes.md](docs/design/implementation-notes.md)。

## 贡献

欢迎贡献，请先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。提交前必须保证：

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

三者全部通过，且新增折叠或 GC 语义必须附带属性测试。提交信息使用英文。

## 许可证

本项目以 [MIT 许可证](LICENSE) 发布。

Copyright (c) 2026 The Yanshi Authors
