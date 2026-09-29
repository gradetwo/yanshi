# 桌面入口：Omarchy / Hyprland（SDDM + Hyprland 会话）

把偃师的服务端做成 **systemd user 服务**（后台常驻、开机自启、不需要图形环境），
再按 Omarchy 的惯例把它注册成一个 **web app**（`chromium --app`），
最后加一个键位。设计文档里客户端就是「Web 编辑器」，因此这里不引入任何原生 GUI 工具包。

本目录的文件是我在这台机器上实际用过的版本，可直接复用：

| 文件 | 安装位置 | 说明 |
|---|---|---|
| `../systemd/yanshi-serve.service` | `~/.config/systemd/user/yanshi-serve.service` | 回环 8110 上的无头服务端 |
| `../icons/yanshi.png` | `~/.local/share/icons/hicolor/256x256/apps/yanshi.png` | 由引擎自己渲染的图标（见 `make-icon.sh`） |
| `Yanshi.desktop` | `~/.local/share/applications/Yanshi.desktop` | Omarchy web app 条目（`omarchy-launch-webapp`） |
| `make-icon.sh` | — | 重新生成图标（一条 curl 画一枚「偃师印」） |

## 一次性安装

```bash
# 0) 构建服务端二进制（服务会直接跑它）
cd ~/yanshi && cargo build --release -p yanshi-http

# 1) systemd user 服务：回环 8110，工作区在 ~/.local/share/yanshi/workspace
mkdir -p ~/.config/systemd/user
cp deploy/systemd/yanshi-serve.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now yanshi-serve.service
systemctl --user status yanshi-serve.service --no-pager | head -5
curl -s http://127.0.0.1:8110/health

# 2) 图标（可用引擎重新渲染，仓库里已带一份）
deploy/omarchy/make-icon.sh            # 或直接用 deploy/icons/yanshi.png

# 3) web app 条目（Omarchy 自带命令，会写 .desktop 并安装图标）
omarchy-webapp-install "Yanshi" "http://127.0.0.1:8110/?doc=yanshi" deploy/icons/yanshi.png
#  → ~/.local/share/applications/Yanshi.desktop
#    Exec=omarchy-launch-webapp "http://127.0.0.1:8110/?doc=yanshi"

# 4) 键位：在 ~/.config/hypr/bindings.lua 末尾追加（先备份！）
o.bind("SUPER + ALT + Y", "Yanshi", { webapp = "http://127.0.0.1:8110/?doc=yanshi", focus = true })

# 5) 验证（改 Hyprland 配置后必须做）
hyprctl reload && hyprctl configerrors        # 期望：ok / 空
omarchy menu keybindings --print | grep -i yanshi
hyprctl binds -j | grep -i yanshi             # 期望看到 key=Y modmask=72（64=SUPER + 8=ALT）
```

## 用法

- `SUPER + ALT + Y`：打开/聚焦偃师查看器（已有窗口时只聚焦，不重复开窗）。
- 应用启动器（`SUPER + SPACE`）里搜 **Yanshi** 同样可启动。
- 服务端 API 直接可用：`curl -s http://127.0.0.1:8110/health`。

## 卸载

```bash
systemctl --user disable --now yanshi-serve.service
rm ~/.config/systemd/user/yanshi-serve.service
omarchy-webapp-remove "Yanshi"          # 删 .desktop 与图标
# 键位：删掉 ~/.config/hypr/bindings.lua 里那一行，再 hyprctl reload
```

## 实测记录（2026-09-29，Hyprland 0.56.2 / Omarchy 4.0.4 / Chromium 153）

- `systemctl --user is-active yanshi-serve` → `active`；内存 2.2MB，CPU 15ms（冷启）。
- `hyprctl configerrors` → 空；`hyprctl binds -j` → `key=Y modmask=72 desc=Yanshi`。
- 触发绑定命令后窗口出现：`chrome-127.0.0.1__-Default | 偃师 Yanshi 查看器`，
  `grim` 抓屏确认欢迎图、缩略图、`已连接 head 6 rendered 6 dirty 0` 全部正常。
- **注意**：用 `wtype` 注入的合成按键**不会**进入 Hyprland 的绑定层（自检绑定也未被触发），
  所以「物理按键 → 绑定」这一环需要你用真键盘按一次确认；绑定注册与其要执行的命令都已验证。
