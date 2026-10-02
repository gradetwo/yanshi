# 偃师 Yanshi

**中文** | [English](README.md)

偃师是一个无头（headless）图像编辑引擎：只追加的原子日志、由日志折叠求值出的状态、纯 Rust 渲染
计算内核、零依赖的 HTTP/WebSocket 服务端，以及一个最小 Web 查看器。同一套内核在服务端原生编译、
在浏览器端编译为 WebAssembly，因此本地乐观渲染与服务端渲染是同一份实现。

[docs/design/yanshi-v1.0-draft4.md](docs/design/yanshi-v1.0-draft4.md) 是权威设计文档，
代码不得与之静默分叉。

## 一句话开启全部工具组

**`--profile all`** ✓ 一个词开启**全部已实现**的组 ✓：**113 个工具**（只开 core 是 45 个 ✓）；
它**不含 semantic** ✓（按既定裁定只预留 ✓）。两个可执行文件的 `--help` 都列全了可选值 ✓。
**图层混合模式会校验** ✓（用渲染层那份权威清单 ✓）⇒ 不认识的名字**明确拒绝并列出可用值** ✓，
**不会**写进日志之后被静默忽略 ✗。

## 纸纹、描边，以及一句实话

`appearance.texture` 给笔触加**纸纹般的颗粒** ✓：字符串 `"noise"` / `"grain"` ✓，
或对象 `{"kind", "scale", "strength", "seed"}` ✓。它们**近似**水彩纸与画布 ✓ 且可调 ✓；
但**没有**专门的"粗纹水彩纸 / 帆布纹理"预设 ✗ —— **如实说明** ✓，好过让人以为有 ✓。

形状支持 **`stroke_width` + `stroke_color`**（与填充色并列 ✓，都写在对象数据顶层 ✓）：

    {"layer_id": "L", "data": {
      "geometry": {"kind": "rect", "x": 20, "y": 20, "w": 120, "h": 90},
      "color": {"r": 200, "g": 180, "b": 140, "a": 255},
      "stroke_width": 4, "stroke_color": {"r": 40, "g": 40, "b": 40, "a": 255}}}

## 批量静默提交

`batch` 支持 **`silent: true`** ✓：不再为**每个子调用**生成预览 ✓ ——
大面积排线几百笔时 ✓，那些**中间预览没人会看** ✓。
**缺省 false** ✓，且**只影响预览** ✓：原子照落 ✓、画照画 ✓。

## 破色（颜色抖动）

`appearance.dynamics.color_jitter`（0..1）让**笔尖色在给定色附近逐印章轻微游走** ✓ —— 这就是油画说的**破色** ✓。
**缺省关闭** ✓，且 `0` 时**整段扰动不执行** ✓ ⇒ **既有文档逐字节不变** ✓。

## 工程包

`export_project` 打出 **`.yanshi` 工程包** ✓：一个**未压缩 tar** ✓，内含**不可变原子日志** ✓、文档元数据 ✓、
日志引用到的**全部 CAS blob** ✓、以及**导出时当场渲染的 HEAD 预览** ✓。任何系统的 `tar` 都能列出与解开 ✓。
**请用它，不要手工拷贝文档目录** ✗ —— 磁盘上那份 `render.png` 缓存**可能是过期的** ✓。

## 介质纹理与混色

`medium_stroke` 新增 **`texture`（0..1）** ✓：把油画插件的**鬃毛与颗粒**压平 ✓
⇒ 大面积铺色也能得到**平滑底色** ✓（你报的"草席感" ✓）。
**缺省 0** ✓，且与改动前**逐字节等同** ✓（用**改动前代码的金标哈希**证明过 ✓）⇒ **已画的画一个像素都不会变** ✓。
**服务端现在也会回读画布** ✓：落笔前取笔下那一点的颜色交给插件 ✓ ⇒ 颜料会与**已有底色混色** ✓
（浏览器端一直如此 ✓，每枚 dab 读 1×1 像素 ✓）。

## 静态构建与存储兼容性

`scripts/package-release.sh --static` 打出**完全静态**的二进制 ✓ —— **不引用任何 GLIBC 版本** ✓。
这件事很重要 ✓：动态版会**继承构建机的 glibc** ✗（实测要求 **2.43** ✓，只因 `atan2f` 一个符号 ✓），
而 Debian 12 只有 2.36 ✓ ⇒ **一运行就崩** ✗。打包脚本现在**每次都打印这个包要求多新的 glibc** ✓，
并且**拒绝**一个"声明了静态却仍引用 GLIBC"的包 ✓ ⇒ 不会再**静默**回归 ✓。

**文件系统不支持 `fsync` 时**（例如 9p / 部分网络挂载 ✓，会返回 `ENOTSUP` ✓），
CAS 写入改为**降级继续** ✓（原来会让**任何写操作都失败** ✗）；
**只有**这一种错误被降级 ✓，磁盘满 / 权限 / IO 错**照旧失败** ✗，
且降级会在 `/health` 的 **`blob_fsync: unsupported`** 里**报出来** ✓ ——
**降级可以接受，静默降级不可以** ✗。

**混合模式本来就齐** ✓：`normal / multiply / screen / overlay / darken / lighten / add / subtract / difference` ✓
（在图层 patch 里设置 ✓）⇒ 这里缺的是**文档** ✗，不是能力 ✓。

## 自动化与空白画布

`window.yanshi` 给脚本一个**稳定入口** ✓：`state()` 报出当前工具 / 颜色 / 粗细 / 不透明度 / 介质 / 图层 / 文档 ✓；
`setColor` / `setSize` / `setOpacity` / `setMedium` / `setTool` 走**界面同一条**控件与事件 ✓
⇒ **不复制逻辑** ✓，UI 改了只需跟着改选择器 ✓。
**注意** ✓：`setColor` 设的是**工具栏那个颜色** ✓（笔刷真正用的那个 ✓），不是对象面板的"重设颜色" ✓ ——
两者**用途不同** ✓，我第一版设错了对象 ✓ ⇒ 表现成"设了没反应" ✓，**只有打印像素才看得出来** ✓。

`new_document` 一键给出**空白画布** ✓。**文档 id 就是持久单元** ✓ ⇒ **新画布 = 新 id** ✓；
对**已打开**的 id 再建会**明确拒绝并告诉你换个 id** ✓，**不会**悄悄清空 ✓。

`--profile` 可用值：core / history / changeset / retouch / semantic / conflict / annotation / collab / structure ✓
（`--help` 里**列全** ✓；semantic 按既定裁定**只预留不开发** ✓）。

## 能照着改的错误信息

图层 / 对象 / 选区 / 蒙版 / 风格**不存在**时 ✓，报错会**列出已有的 id** ✓（**上限 8 个 + 省略号** ✓）；
**一个都没有时单独说明** ✓ ⇒ 调用方**不必再调一次列表接口** ✓ 就能自纠 ✓。

**快捷键** ✓：`i` 切**吸管** ✓（想从已画区域取色继续画时用 ✓）、`Tab` 全屏 ✓、工具提示里都写着对应键 ✓。

## 撤销与重做

`Ctrl/Cmd + Z` 撤销 ✓、`Ctrl/Cmd + Shift + Z` 重做 ✓（右栏的按钮同样可用 ✓）。

## 我跑的是哪一版

`yanshi-serve --version` 与 `yanshi-mcp --version` 会打印**版本 / 短 commit / 构建时间** ✓，
`GET /health` 报**同样的值** ✓ ⇒ 拿到一个正在跑的服务**不用问人就知道是哪一版** ✓。
commit 带 **`-dirty`** 后缀表示构建时工作区**有未提交改动** ✓。
发布包的**文件名里就带 commit** ✓，包内还有 **`BUILD-INFO`**（版本 / commit / 目标平台 / 构建时间 / 编译器 ✓）。

## 导出 PNG

`export_png` 把**整幅或指定区域**渲染并**写成 PNG 文件** ✓（可先缩放 ✓）——
agent 拿到的是**文件** ✓，不是 base64 ✓，也不再受 `include_image` 那个 **512px** 上限的限制 ✓。

    {"path": "/tmp/canvas.png", "max_edge": 2048}

也可以改用 `width`+`height` 一起给 ✓；`filter` 选 `nearest` / `bilinear` ✓。

## 笔触压力

`draw_stroke` 的 `points` 支持**可选的第三个分量**：压力（0..1 ✓）——
`[[x, y, 0.2], [x, y, 0.9]]` 画出来的线就有**提按顿挫** ✓；不写就是满压力 ✓，与从前**逐位一致** ✓。
内核**一直支持**这件事 ✓（连 `{"x":…, "y":…, "pressure":…}` 的对象写法也认 ✓），
只是**工具的说明里没写** ✗ ⇒ 生成的画面才总是"均匀的面条线" ✗。

    {"layer_id": "L", "data": {
      "points": [[20, 40, 0.15], [60, 44, 0.9], [100, 48, 0.35]],
      "size": 24, "color": [0.8, 0.2, 0.1, 1.0]}}

## 形状几何

形状的几何是 `{"kind": "rect" | "ellipse" | "polygon", …}` ✓，尺寸字段用 `bbox`（`x`/`y`/`w`/`h` ✓）或 `points` ✓。
**其它写法会明确报错并说明该写什么** ✓ —— **不会**返回成功却落一个**渲染不出来**的对象 ✗。
`bbox` 可以写成对象 ✓、四个数的数组 ✓、**或者干脆不写 `bbox` 而把 `x`/`y`/`w`/`h` 直接写在 geometry 上** ✓ ——
**三种写法都会真的画出来** ✓（归一化在**校验之前**完成 ✓ ⇒ "能通过校验的写法一定能画" ✓）。
`create_layer` 会**直接返回它创建的 `layer_id`** ✓。

## 渲染与确定性

计算内核是客户端与服务端**共享的 CPU 代码** ✓、**逐位一致** ✓ —— 设计把这条叫作 **D0 基线** ✓、并视其为**唯一权威** ✓；
**合成层、预览与缩略图**属 **D1** ✓、允许 **1 个最低有效位**的差异 ✓。
**没有 GPU 后端** ✓：设计把 GPU 加速列为**可选** ✓、且**只作用于合成层** ✓，
而引入它意味着**引入依赖** ✓ —— 本项目不引依赖 ✓。
目前 **CPU 路径同时满足两层** ✓（预算测试都在跑 ✓），日后若需要 ✓，
可以在**合成层之后**加 GPU 后端 ✓，**架构不用改** ✓。

## 路径算子

对象面板可执行设计 §792 的路径算子 ✓：**reverse 反向 / close 闭合 / join 连接 / merge 合并 / split 切开 / boolean 布尔** ✓，
布尔另有 **union / intersect / subtract / xor** 四种模式 ✓。
**一元算子作用于勾选的第一个对象** ✓；**join / merge / boolean 需要两个** ✓（第一个当目标 ✓、第二个当 `other_id` ✓）；
**不足两个时界面直接拒绝并说明需要几个** ✓（而不是发一个必然失败的请求 ✗）。
**布尔会用合并结果替换掉两个输入** ✓ ⇒ 所以对象数是**变少**而不是变多 ✓。

## 角色

打开文档时可以给一个角色 ✓：`POST /api/documents?role=viewer` 签发只读能力令牌 ✓，`editor`（缺省）可改文档 ✓，
`owner` 另可撤销别人的原子 ✓。**强制发生在工具层** ✓（用工具自己声明的 `mutating` 标记 ✓）——
之所以不挂在某一个入口上 ✓，是因为**曾经**只在 HTTP 工具接口上查过 ✓，
而 **WebSocket** 那条路径**自建上下文** ✓ ⇒ **整个绕过了检查** ✗。
未知角色会被拒绝 ✓，而不是悄悄当成 editor ✓（那会把一条只读链接变成可写链接 ✗）。


## 发布包

**`make release`** ✓ 会构建 release 二进制 ✓，并把它们**运行期真正需要的资产**一起打包 ✓：
两个可执行文件 ✓、浏览器端 WASM 计算内核 ✓、以及各**介质插件** ✓。产物是 `dist/` 下一个带版本号的
tar.gz 与一份校验和 ✓。

    make release                          # 构建并打包到 dist/
    tar -xzf dist/yanshi-*-x86_64-unknown-linux-gnu.tar.gz
    cd yanshi-*-x86_64-unknown-linux-gnu
    ./yanshi.sh --root ./workspace --bind 127.0.0.1:8110

然后打开 <http://127.0.0.1:8110/> ✓。**不给 `--root` 就是纯内存** ✓：不落地、重启即空 ✓。

包内布局是 `bin/`（可执行文件 ✓）＋ `share/yanshi/`（WASM 内核 ✓、介质插件 ✓、品牌资源 ✓），
外加一个薄包装 `yanshi.sh` ✓。**包装脚本不是多余的** ✓：服务端对这些资产目录的缺省值是**相对当前目录**的 ✓
⇒ 直接跑 `bin/yanshi-serve` 而当前目录不对时 ✓，**内核与插件都取不到** ✗ ——
服务**照样能起来** ✗，但查看器会退化 ✓、`/mediums/*.wasm` 会 **404** ✓（**没有人会替你报错** ✗）。
包装脚本改为**按自身位置**推算这些路径 ✓ ⇒ 整棵树**放到哪都能跑** ✓。

## 依赖

本项目此前**只有一个依赖 `wasm-bindgen`** ✓。**按决定** ✓，现在多了 **Hokusai** ✓ ——
一个受 libmypaint 启发的**纯 Rust 笔刷引擎** ✓（`hokusai` ✓，用缺省 feature `myb-json` 与 `tile-mem` ✓，
连带两个 `thiserror` crate ✓）。**理由很具体** ✓：Hokusai 能读 libmypaint 的 **`.myb` 笔刷** ✓
⇒ 仓库里 vendor 的那包 **CC0 笔刷**（`assets/brushes/` ✓）可**照原样使用** ✓ 而不必做近似 ✓；
它与 libmypaint 在 **196 支 stock 笔刷里 188 支像素对齐** ✓。许可证 **Apache-2.0 或 MIT** ✓、
**无 `unsafe`** ✓、**可为 wasm32 构建** ✓；**有意不开** `tiny-skia` ✓。
**其余部分依然零依赖** ✓：HTTP 栈 ✓、渲染 ✓、存储 ✓、介质插件 ✓ 都是手写的 ✓。

## 致谢与许可

本项目主体是 **MIT**（见 `LICENSE` ✓），但有两处重要的补充与例外 ✓。

`assets/brushes` ✓：**196 支**来自 **mypaint-brushes 2.0.2** 的笔刷 ✓，该包是 **CC0 1.0** ✓；
上游 `COPYING` 一并保留为 `LICENSE-CC0.txt` ✓。

`assets/backgrounds` ✓：**69 张**纸张/画布纹理 ✓，来自 **MyPaint 2.0.1** 资源包 ✓ ⇒ 该部分是 **GPL-2.0-or-later** ✗
⇒ **不受本仓库 MIT 许可覆盖** ✗，细节见 `assets/backgrounds/NOTICE.md` ✓。
**项目所有者裁定** ✓：**先用起来并在 README 署名** ✓，**是否整体转成 GPL 兼容许可留待后续考虑** ✓。
下游再分发前请先读那份说明 ✓。

笔刷引擎来自 **Hokusai**（Apache-2.0 或 MIT ✓），见上文 ✓。

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
## 查看器操作

**东西在哪** ✓（有几个确实不好找 ✓）：左侧**工具栏**是画笔、形状、文字等 ✓，每个都有图标与快捷键 ✓；
画布上方的**选项行**是该工具的参数 ✓，其中就有 **`介质`** 下拉（示范点 / 油画 / 水彩 / 马克笔 / 铅笔 / 像素 ✓）
—— **要用插件介质而不是内置笔刷，就改它** ✓；
右侧**图层列表**每行有一个**眼睛**和一个**锁** ✓；画布上方还有 **`导出 PNG`** ✓。
**隐藏最后一个可见图层 ⇒ 画布变白** ✓ —— 那是"隐藏"本来的意思 ✓，不是故障 ✓
（**眼睛**管可见性 ✓，**锁**只管"不能改内容" ✓，两者互不影响 ✓）。
**导出工程包**（不是 PNG ✓）目前在工具接口上叫 `export_project` ✓，**查看器里还没有按钮** ✗（已排期 ✓）。


拖动即画笔；`＋ 图层` 新建图层；`撤销` / `重做`（**可连续多级撤销/重做**）。**滚轮以光标为中心缩放**，
**中键拖动平移**，`+` / `-` / `0` 放大、缩小、适配，`1:1` 一文档像素对一 CSS 像素、`导出 PNG` 下载整幅分辨率的成品；「打开…」对话框列出服务器文档并可导入本地图片（PNG/JPEG/WebP 由浏览器解码后按原始像素上传；走**工具 API** 时 `POST /api/blob` 也能**直接接受 PNG**，由仓库自带的解码器归一化成原始像素——项目不引外部依赖，所以 JPEG/WebP 在那里会被**明确拒绝并说明**，不会原样入库）；历史面板按类型/操作者筛选原子日志并可「回到此处」；调整/滤镜面板可对当前图层应用任意效果（效果名来自内核，参数默认取内核默认值）；调整/滤镜**加完还能改**——点列表里的那一条即载入它的参数并**更新同一个对象**，而不是再叠一个；最近几块面板补的是同一类缺口（**工具在、单测也在、就是没有入口**）：导入对话框里选中 **PSD** 会**转交服务端**，只读它的**合成图**并在日志里说明，**图层结构与蒙版不导入**、契约是**只读**；`标注` 面板可新建/编辑/解决/删除标注，并在画布上画出**可点的图钉**；`对象` 面板列出当前图层的对象，可把一个变成**联动实例**、把若干**编组**，可按角度/百分比/位移**旋转缩放平移**，任何对象都能**转成形状或路径**以便当矢量继续编辑；历史面板可**打检查点并回到它**（日志 append-only ✓，什么都不会丢 ✓，之后还能再前进 ✓）；历史面板现在可以**点开任意一条原子**看它**到底改了什么** ✓（净荷 ✓）—— 这需要新工具 **`get_atom`** ✓（`get_log`/`find_atom` 按设计只给元数据 ✗）；`评论` 面板可发表评论并列出（含作者与正文 ✓）—— 它**顺带补上了一个真缺口**：评论一直**写得进、读不回** ✗（`get_log`/`find_atom` 按设计**只返回元数据** ✓）⇒ 于是先补工具 **`list_comments`** ✓、再让界面用它 ✓；标注除了「解决」还能「**拒绝**」✓（状态真的变成 rejected ✓）；建议可以先「**预览**」✓ —— 预览会**逐步校验补丁**并报告 ✓、但**不应用任何东西** ✓（验收里效果数在预览后仍是 0 ✓、按下接受才变 1 ✓）；工具栏每个按钮都带内联 SVG 图标 ✓、提示里带快捷键 ✓，并有**守卫测试**保证这一点 ✓ —— 因为「标注」工具当初就是**漏了图标** ✗ ⇒ 渲染成一个**看不见的按钮** ✗；`medium_stroke` 让**服务端 / MCP 会话**也能用介质插件作画 ✓（不再只有浏览器能画 ✓）：

    {"layer_id": "L", "medium": "oil", "points": [[20, 30, 0.2], [60, 36, 0.9], [100, 42, 0.5]],
     "size": 24, "color": {"r": 210, "g": 80, "b": 40, "a": 255}}

点支持**可选的第三个分量（压力 ✓）**，对象上记着**插件 id 与 version** ✓。仓库自带**六个介质插件** ✓（`assets/mediums/` 下的零依赖 wasm ✓：example / **oil** / watercolor / marker / pencil / pixel ✓），并且**对象里记着介质的 id 与 version** ✓ ⇒ **升级插件不会悄悄改变旧文档的渲染** ✓；对象面板还能**改笔触** ✓（颜色 / 粗细 / 不透明度 ✓，走 `update_stroke` ✓ —— 它此前**零测试覆盖** ✗，本轮补了两条 ✓）；历史面板里的「**变更集**」一节可以把**一串动作打包、整体撤销** ✓ —— 顺带修掉一个真缺口 ✓：对外服务的**工具组列表漏了 changeset 与 conflict** ✗（而它自己的注释写着「**启用全量**」✓）⇒ 界面上点按钮只会得到「未知工具」✗；**Semantic 仍故意不启用** ✓（需外部模型服务 ✓，按既定裁定只预留不开发 ✓）；`建议` 面板列出设计 §12.6 的建议（含状态与优先级 ✓），**接受**时会**按序重放它的补丁** ✓ 并把关联标注置为 resolved ✓ —— 与「标注」合起来才是完整的评审闭环 ✓；`存储 / 维护` 面板按设计 §6.3 报出 blob 的**三级生命周期**（活跃/历史/孤儿 ✓）并可**回收过 TTL 的孤儿**、**把历史级降冷** ✓，两者都必须**先勾确认** ✓（删除不可逆 ✓）；只统计则永远安全 ✓、绝不删东西 ✓；笔触支持 `appearance` 块（即设计的 `advanced.appearance`）：`size_curve`/`opacity_curve`/`pressure_curve` 塑造笔形，`dynamics` 配合 `seed` 提供**确定性**的抖动/散布/尺寸与角度变化，程序化 `noise`/`grain` `texture` 调制落墨，`paint_load`/`wetness`/`mixing` 则做出**湿笔**：墨沿笔迹耗尽、连续拖动比点按更耗墨、混色会把笔尖下方的已有颜色带进笔触；**不传 appearance 时渲染与之前逐字节一致**。

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
