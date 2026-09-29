# 偃师 Yanshi

**中文** | [English](README.md)

偃师是一个无头（headless）图像编辑引擎：文档以只追加的原子日志保存，状态由折叠日志求值得出，
渲染走一套纯 Rust 计算内核，对外通过零依赖的 HTTP/WebSocket 传输层与一个最小 Web 查看器提供服务。

同一套内核既在服务端原生编译，也编译成 WebAssembly 供浏览器使用，因此浏览器的本地乐观渲染
与服务端渲染是同一份实现，结果一致。

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) 是权威设计文档。
代码不得与之静默分叉。

## 安装

需要 Rust stable（1.85 或更高）与 `make`：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

若需要浏览器端内核，再装 WebAssembly 目标与 `wasm-bindgen-cli`（版本须与 `wasm-bindgen` crate 匹配）：

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

这两项是可选的：不装也能运行服务端，查看器退化为服务端渲染。

## 编译与运行

```bash
make run          # 有工具链时构建 WASM 内核，构建服务端并启动
                  # 随后打开 http://127.0.0.1:8110/ （页面自行获取 capability token）
make build        # 只构建服务端
make build-wasm   # 只构建 WASM 内核
```

`make run` 支持 `PORT` 与 `ROOT`（缺省 `~/.local/share/yanshi/workspace`）：

```bash
make run PORT=9000 ROOT=/tmp/yanshi
```

不需要浏览器内核时，直接运行服务端：

```bash
cargo run --release -p yanshi-http -- --root ./workspace --no-wasm
```

全部目标见 `make help`。

## 测试

```bash
make test         # 工作区测试套件，秒级到两分钟
make ci           # fmt、clippy、测试、WASM 运行时冒烟检查
```

性能预算、10 万原子折叠 fuzz、4K 剖面等长任务带 `#[ignore]`，在 GitHub 上跑，不占用本地：

```bash
gh workflow run heavy.yml
gh run list
```

`.github/workflows/ci.yml` 在每次 push 时执行快速检查；`.github/workflows/heavy.yml` 每夜与手动
触发执行长任务，并把日志上传为 artifact。确需本地执行长任务时：

```bash
cargo test --release --workspace -- --ignored --nocapture
```

## 工具与效果

工具层默认注册 27 个核心工具；启用全部已实现分组时共 65 个工具。
分组为 core、history、retouch、annotation、collab、structure（用 `--profile` 选择）。

调整（12 种）：brightness_contrast、saturation、invert、levels、exposure、white_balance、curves、
hsl、posterize、color_balance、split_toning、vibrance。其中 `levels` 支持分通道 `channel`。

滤镜（13 种）：box_blur、gaussian_blur、motion_blur、sharpen、clarity、dehaze、film_grain、
noise、vignette、glow、brightness_contrast、saturation、invert。

修图与液化：clone_stamp、heal_stamp、smudge、patch、liquify_push、liquify_twirl、liquify_pinch。
蒙版支持矩形、椭圆、多边形，可设羽化。

### MCP（供 AI Agent 接入）

```bash
cargo run --release -p yanshi-mcp            # stdio，纯内存，缺省文档
cargo run --release -p yanshi-mcp -- --list-tools
```

### HTTP / WebSocket

```bash
# 打开或创建文档，响应里带回 capability token
curl -s -X POST http://127.0.0.1:8110/api/documents \
     -d '{"doc_id":"demo","width":1024,"height":1024}'

# 调用工具（token 可用 Authorization: Bearer 或 ?token=）
curl -s -X POST "http://127.0.0.1:8110/api/tools/create_layer?doc=demo&token=$TOKEN" \
     -d '{"layer_id":"layer_1"}'

# 渲染区域，thumb_url 指向 CAS 中的 blob
curl -s -X POST "http://127.0.0.1:8110/api/tools/render_region?doc=demo&token=$TOKEN" \
     -d '{"region":{"x":0,"y":0,"w":512,"h":512}}'
```

## 桌面入口（Omarchy / Hyprland）

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/yanshi-serve.service <<'EOF'
[Unit]
Description=Yanshi server
After=network.target

[Service]
ExecStart=%h/yanshi/target/release/yanshi-serve --bind 127.0.0.1:8110 --root %h/.local/share/yanshi/workspace --doc yanshi
WorkingDirectory=%h/yanshi
Restart=on-failure
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=default.target
EOF
systemctl --user daemon-reload
systemctl --user enable --now yanshi-serve
```

```lua
-- ~/.config/hypr/bindings.lua
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })
```

## 仓库结构

```
crates/yanshi-core/     原子、日志、折叠、state@seq、快照、Blob CAS、冲突、标注
crates/yanshi-render/   渲染内核：笔触、形状、调整、滤镜、dirty、tile、PNG
crates/yanshi-server/   文档服务、Job、token、广播、工具层
crates/yanshi-http/     HTTP/WebSocket 传输、查看器、yanshi-serve 可执行文件
crates/yanshi-mcp/      MCP stdio 服务
crates/yanshi-wasm/     浏览器内核（wasm-bindgen）
scripts/                验收脚本（演示、逐像素自检、wasm 冒烟、性能探针）
docs/design/            设计文档与实现说明
```

## 文档

- [设计文档](docs/design/yanshi-v1.0-draft4.md) —— 权威规范。
- [实现说明](docs/design/implementation-notes.md) —— 模块对应关系、设计未规定处的取舍、
  实测性能数据、已知偏差。
- [scripts/README.md](scripts/README.md) —— 验收脚本的运行方式。
- [SECURITY.md](SECURITY.md) —— 威胁模型与报告方式。

## 状态

已实现：原子日志与折叠（五条不变量有属性测试覆盖）、确定性 CPU 渲染（计算内核逐位一致；
模糊族滤镜经批准后允许 ±1 LSB）、服务端渲染与缩略图/tile、工具层与分组、带 capability token 的
HTTP/WebSocket 传输、标注通道与 AI 建议环路、浏览器 WASM 内核。

未实现：GPU 合成（可行性已实测并记录在实现说明里）、插件托管、单服务端之外的协作传输。

## 许可

MIT，见 [LICENSE](LICENSE)。
