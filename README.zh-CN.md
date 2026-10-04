<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/svg/logo-horizontal-cn-dark.svg">
    <img src="assets/brand/svg/logo-horizontal-cn.svg" alt="偃师 Yanshi" width="320">
  </picture>
</p>

# 偃师 Yanshi

**中文** | [English](README.md)

**偃师 Yanshi，AI Native 的 headless 绘画引擎，人与 AI 在同一张画布上协作的创作工具。**

- **操作原子化。** 每一次编辑都是一个原子、追加进日志；画面是这份日志折叠求值的结果。
- **一份内核，两个宿主。** 同一套 Rust 内核在服务端原生编译、在浏览器编译为 WebAssembly
  ⇒ 本地渲染与服务端渲染是**同一份实现**，不是两份近似。
- **工程包自包含。** 一个 `.yanshi` 包带着原子日志与它引用的全部 blob；导入到别处就能复现同一幅画。

在这套引擎之上是一个 MCP 服务端与一个 Web 查看器 ⇒ **Agent 与人操作的是同一份文档**。

## 快速开始

**前置要求**

- **Rust** —— [`rust-toolchain.toml`](rust-toolchain.toml) 固定的 stable 工具链，含 `rustfmt` 与 `clippy` 组件；
- **`make`** 与 POSIX shell —— 下面的入口都是 Make 目标，内部调用 `scripts/`；
- 可选，**要用客户端渲染**（浏览器内核）时：`rustup target add wasm32-unknown-unknown`，以及
  `wasm-bindgen-cli` **0.2.129**（`cargo install wasm-bindgen-cli --version 0.2.129 --locked`）。
  它的版本必须与 `wasm-bindgen` crate 匹配，否则生成的胶水加载不了那个 `.wasm`。
  **不装这两项也能启动服务端**，查看器退化为服务端渲染；
- 可选，**只在跑判据时**：**Node** 与 **chromium**（没有 chromium 时运行器会跳过浏览器判据）。

**编译与运行**

```bash
git clone https://github.com/gradetwo/yanshi.git && cd yanshi
make dev        # 有工具链时构建 WASM 内核，构建服务端并启动
                # ⇒ 打开 http://127.0.0.1:8110/ （页面自行获取 capability token）
```

数据缺省落在 `~/.local/share/yanshi/workspace`。要换端口或数据目录，请用**环境变量** ——
`make dev` **不会**把 Make 变量传进它的 recipe，而 `make serve` 是显式传 `--port` 的：

```bash
PORT=9000 make dev                 # 写成 make dev PORT=9000 **不会**传进去
ROOT=/tmp/yanshi PORT=9000 make dev
make serve PORT=9000
```

随后：

```bash
make check      # fmt + clippy + 测试 —— 推送前要跑的三项
make test       # 工作区测试（快的那一批）；make test-heavy 跑很慢的 #[ignore] 任务
make serve      # 重编 -p yanshi-http 并起服务（用 curl 验工具前先跑它）
make release    # 打出发布包
make help       # 全部目标
```

## 致谢


本项目主体是 **MIT**（见 `LICENSE` ✓），但有两处重要的补充与例外 ✓。

`assets/brushes` ✓：**196 支**来自 **mypaint-brushes 2.0.2** 的笔刷 ✓，该包是 **CC0 1.0** ✓；
上游 `COPYING` 一并保留为 `LICENSE-CC0.txt` ✓。

纸张/画布纹理改从 **CC0** 来源取材 ✓，不再用 MyPaint 自带那份 ✗ ——
后者是 **GPL-2.0-or-later** ✓，会把**第二个许可证**带进仓库 ✗ ⇒ 已撤掉 ✓
⇒ 如今仓库里**只有 MIT 与 CC0** ✓。

`assets/palettes` ✓：**Open Colors** ✓（15 个色系 / **132 色** ✓，**MIT** ✓）⇒
这是项目所有者选定用来**替换 MyPaint 自带调色板**的那一套 ✓；
**sK1 的公共领域调色板**尚待定位 ✗（已查的那个仓库里没有 ✓）。

笔刷引擎来自 **Hokusai**（Apache-2.0 或 MIT ✓），见上文 ✓。
- **Rust 库**：`serde`、`serde_json`（MIT / Apache-2.0）、`sha2`（MIT / Apache-2.0）、`thiserror`（MIT / Apache-2.0）、`proptest`（MIT / Apache-2.0），以及 `libm`（MIT）—— 最后这个是为了让服务端与浏览器**共用同一份数学实现**、逐位一致 ✓。
- **人**：这里预留给贡献者与评审者。愿意被列名的话说一声 ✓。

## 名字的由来

「偃师」出自《列子·汤问》里造木偶的工匠。原文：

> 周穆王西巡狩，越昆侖，不至弇山。反還，未及中國，道有獻工人名偃師，穆王薦之，問曰：「若有何能？」偃師曰：「臣唯命所試。然臣已有所造，願王先觀之。」穆王曰：「日以俱來，吾與若俱觀之。」翌日，偃師謁見王。王薦之曰：「若與偕來者何人？」對曰：「臣之所造能倡者。」穆王驚視之，趨步俯仰，信人也。巧夫，顉其頤，則歌合律；捧其手，則舞應節。千變萬化，惟意所適。王以為實人也，與盛姬內御並觀之。技將終，倡者瞬其目而招王之左右侍妾。王大怒，立欲誅偃師。偃師大懾，立剖散倡者以示王，皆傅會革木膠漆白黑丹青之所為。王諦料之，內則肝膽心肺脾腎腸胃，外則筋骨支節皮毛齒髮，皆假物也，而無不畢具者。合會復如初見。王試廢其心，則口不能言；廢其肝，則目不能視；廢其腎，則足不能步。穆王始悅而歎曰：「人之巧乃可與造化者同功乎！」詔貳車載之以歸。夫班輸之雲梯，墨翟之飛鳶，自謂能之極也。弟子東門賈、禽滑釐聞偃師之巧以告二子，二子終身不敢語藝，而時執規矩。

> （据 [中国哲学书电子化计划《列子·汤问》](https://ctext.org/liezi/tang-wen/zhs)；此处按简体转录。）

这个故事值得借来做名字的地方，不在"巧"字，而在**构造与行为是分开的**：那个能唱能舞的东西，剖开看是"革木胶漆、白黑丹青"；把心肝腎拆掉，它就分别不能言、不能视、不能步；**而"合会"之后，又和当初一样**。

这个项目和这个故事对齐的部分，就是这一点：

- **日志即构造**：每次编辑是一条原子，追加进日志；画面是把它折叠求值的结果。
- **拆开再装上，结果不变**：`.yanshi` 工程包把原子日志与它引用到的全部 blob 一起装走，导进另一个工作区后逐字节一致。
- **行为由同一份内核产生**：服务端原生编译与浏览器 WebAssembly 是同一份实现，不是两套各自近似的东西。

原文结尾是穆王的赞叹——"人之巧乃可与造化者同功乎"。**这一句我们不当真**：这个项目只是把一件工具的构造写清楚，能做到的和做不到的都写在文档里，仅此而已。

## Logo

标是一个**弹弓**：Y 形的杈，两根蓝色皮筋，一颗红色弹丸。它的灵感来自电影《谁说我不在乎》里的一句台词：

> 抽出裤衩里的猴皮筋，做成弹弓打你们家玻璃。

> （[片段](https://www.bilibili.com/video/BV1Rw411c7Bx/)，bilibili。影片版权属原权利人。）

借的只是一句玩笑话里那个动作——**用现成的东西做一件能用的东西**。这和项目本身的态度一致：把构造讲清楚，能用就用，不能就说不能。

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

发布打包、查看器操作、笔刷、纹理、调色板、兼容性等**操作性文档**都在 [docs/guide.zh-CN.md](docs/guide.zh-CN.md) ✓。

## 许可

MIT，见 [LICENSE](LICENSE)。
