# 偃师 Yanshi

**中文** | [English](README.md)

偃师是一个无头（headless）图像编辑引擎：只追加的原子日志、由日志折叠求值出的状态、纯 Rust 渲染
计算内核、零依赖的 HTTP/WebSocket 服务端，以及一个最小 Web 查看器。同一套内核在服务端原生编译、
在浏览器端编译为 WebAssembly，因此本地乐观渲染与服务端渲染是同一份实现。

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) 是权威设计文档，
代码不得与之静默分叉。

## 安装

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh    # Rust stable 1.85+
```

可选：浏览器端内核需要 WebAssembly 目标与 `wasm-bindgen-cli`（版本须与 `wasm-bindgen` crate 匹配）。

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

不装这两项也能运行服务端，查看器退化为服务端渲染。

## 编译与运行

```bash
make run          # 有工具链时构建 WASM 内核，构建服务端并启动
                  # 随后打开 http://127.0.0.1:8110/ （页面自行获取 capability token）
make build        # 只构建服务端
make build-wasm   # 只构建 WASM 内核
```

```bash
make run PORT=9000 ROOT=/tmp/yanshi                 # PORT / ROOT（缺省 ~/.local/share/yanshi/workspace）
cargo run --release -p yanshi-http -- --no-wasm     # 只跑服务端，不用浏览器内核
make help                                           # 全部目标
```

## 测试

```bash
make test         # 工作区测试套件
make ci           # fmt、clippy、测试、WASM 运行时冒烟检查
```

性能预算、10 万原子折叠 fuzz、4K 剖面等长任务带 `#[ignore]`，在 GitHub 上执行：

```bash
gh workflow run heavy.yml && gh run list            # 仅手动触发；日志上传为 artifact
cargo test --release --workspace -- --ignored --nocapture   # 或本地执行
```

## 使用

```bash
cargo run --release -p yanshi-mcp                   # MCP over stdio（供 AI Agent 接入）
cargo run --release -p yanshi-mcp -- --list-tools
```

```bash
# 打开文档，响应里带回 capability token
curl -s -X POST http://127.0.0.1:8110/api/documents -d '{"doc_id":"demo","width":1024,"height":1024}'

# 调用工具（token 可用 Authorization: Bearer 或 ?token=）
curl -s -X POST "http://127.0.0.1:8110/api/tools/create_layer?doc=demo&token=$TOKEN" -d '{"layer_id":"layer_1"}'
```

对象组按设计 9.4 提供：`create_group`、`add_to_group`、`remove_from_group`、`set_group_transform`
可以让一组对象**一起移动**；组变换记录在组上，而成员自身携带合成后的几何，因此渲染、包围盒、命中测试与脏区规划都无需特例即可保持一致。

工具与效果清单见 [docs/tools.md](docs/tools.md)。

## 查看器操作

拖动即画笔；`＋ 图层` 新建图层；`撤销` / `重做`（**可连续多级撤销/重做**）。**滚轮以光标为中心缩放**，
**中键拖动平移**，`+` / `-` / `0` 放大、缩小、适配，`1:1` 一文档像素对一 CSS 像素、`导出 PNG` 下载整幅分辨率的成品；「打开…」对话框列出服务器文档并可导入本地图片（PNG/JPEG/WebP 由浏览器解码后按原始像素上传；走**工具 API** 时 `POST /api/blob` 也能**直接接受 PNG**，由仓库自带的解码器归一化成原始像素——项目不引外部依赖，所以 JPEG/WebP 在那里会被**明确拒绝并说明**，不会原样入库）；历史面板按类型/操作者筛选原子日志并可「回到此处」；调整/滤镜面板可对当前图层应用任意效果（效果名来自内核，参数默认取内核默认值）；笔触支持 `appearance` 块（即设计的 `advanced.appearance`）：`size_curve`/`opacity_curve`/`pressure_curve` 塑造笔形，`dynamics` 配合 `seed` 提供**确定性**的抖动/散布/尺寸与角度变化，程序化 `noise`/`grain` `texture` 调制落墨，`paint_load`/`wetness`/`mixing` 则做出**湿笔**：墨沿笔迹耗尽、连续拖动比点按更耗墨、混色会把笔尖下方的已有颜色带进笔触；**不传 appearance 时渲染与之前逐字节一致**。

工具栏另有像素类工具：画笔与**橡皮**、**填充图层**、**吸管**、仿制/修复（Alt+点击设源点）、涂抹、三种液化模式，以及**矩形/椭圆蒙版**（可设羽化，拖出形状即建好并挂到当前图层）；`选区` 拖出矩形选区后会**逐像素**约束其后的**全部**绘制图元——画笔、橡皮、形状、填充、文本、液化与修图（选区外一个像素都不会被写到），`清除选区` 之后同一批对象按日志重算、恢复为不受约束；`文本` 在点击处放置文本对象，文字始终是日志里的**可编辑对象**（用 `supersede` 改数据即可改变渲染），内核用**内嵌开源字体**光栅化：内置 5×7 ASCII 字体 + 由 Noto Sans CJK（SIL OFL 1.1）生成的 **1-bit 16×16 图集**，覆盖 ASCII、GB2312 一级与二级字库共 6763 个汉字（3755 常用 + 3008 次常用）与 GB2312 符号区；来源、格式与复现命令见 `assets/fonts/`；
`刷新` 从服务端重渲染，`一致性自检` 做内核与服务端的逐像素比对。

## 桌面入口（Linux：Omarchy / Hyprland）

```bash
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user daemon-reload && systemctl --user enable --now yanshi-serve
```

```lua
-- ~/.config/hypr/bindings.lua
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
```

用到 `systemctl --user`，因此仅适用于 Linux；macOS 上服务端用法相同（`make run`）。
细节见 [deploy/omarchy/README.md](deploy/omarchy/README.md)。

## 文档

- [设计文档](docs/design/yanshi-v1.0-draft4.md) —— 权威规范。
- [实现说明](docs/design/implementation-notes.md) —— 模块对应关系、设计未规定处的取舍、
  实测性能数据、已知偏差。
- [docs/tools.md](docs/tools.md) —— 工具、调整、滤镜、修图、蒙版、协作。
- [scripts/README.md](scripts/README.md) —— 验收脚本，含「落笔后画布仍有内容」的真实浏览器 UI 检查。
- 仓库结构：`crates/yanshi-core`（原子、日志、折叠、CAS）、`-render`（计算内核）、
  `-server`（文档服务与工具层）、`-http`（传输、查看器、`yanshi-serve`）、`-mcp`、`-wasm`
  （浏览器内核）；以及 `scripts/`、`deploy/`、`docs/design/`。
- [SECURITY.md](SECURITY.md) —— 威胁模型与报告方式。

## 状态

已实现：原子日志与折叠、CPU 渲染（计算内核逐位一致；模糊族滤镜经批准后允许 ±1 LSB）、
服务端渲染与缩略图/tile、工具层、带 capability token 的 HTTP/WebSocket 传输、标注通道与建议环路、
浏览器 WASM 内核。未实现：GPU 合成（可行性已实测）、插件托管、单服务端之外的协作传输。

## 许可

MIT，见 [LICENSE](LICENSE)。
