# 部署方式矩阵（**含 GPU 优先与 fallback ✓**）

**用户要求（第 601 轮 ✓）**：
> 1. **再增加一个轻量级部署：可部署到 Cloudflare Workers，并作为 PWA 使用，其后端不再连服务器 ✗**；
> 2. **所有部署方式都要**GPU 优先**✗，**GPU 没有或人为强制关闭时才用 CPU ✓****。

## 一、现状核验（**实测 ✓**）

| 项 | 事实 |
|---|---|
| **`yanshi-wasm` 依赖** | **`yanshi-core` ✓／`yanshi-render` ✓／`hokusai` ✓／`libm` ✓／`serde` ＋ `serde_json` ✓／`wasm-bindgen 0.2` ✓** |
| **★ DOM 依赖** | **`web_sys`／`js_sys`／`window()`／`document()`／`HtmlCanvas`／`getContext` ⇒ **全部零命中 ✓** |
| **∴ 结论** | **内核是一个**纯计算**wasm 模块 ✗** ⇒ **∴ 它不依赖 DOM ⇒ **∴ 可在 Cloudflare Workers 的 V8 isolate 里运行 ✓**** |
| **已有部署线索** | **仓库里**没有** `wrangler`／`manifest.json`／service worker／Docker／CI 部署配置 ✗** ⇒ **∴ 三种新部署都要新建 ✓** |

## 二、部署矩阵（**四种 ✓**）

| # | 部署 | 后端 | 计算在哪 | 状态 |
|---|---|---|---|---|
| **A ★ 现有 ★** | **`yanshi-serve`（**Rust 服务端 ✓ ＋ 浏览器 viewer ✓**）** | **有（**本地／自托管 ✓**）** | **服务端 ＋ 可选内核 ✓** | **★ 已发布 ✓** |
| **B ★ 新 ★** | **Cloudflare Workers ＋ PWA** | **无 ✗**（**不连服务器 ✓**） | **Workers 里的 wasm（**纯计算 ✓**）＋ 浏览器内核 ✓** | **待建** |
| **C ★ 新（**由 B 派生 ✓**）** | **纯静态 PWA（**无 Workers ✓**）** | **无 ✗** | **仅浏览器内核 ✓** | **待建** |
| **D** | **桌面（**若将来 ✓**）** | **无 ✗** | **本地进程 ✓** | **未规划** |

**∴ B 与 C 的区别** ✓：**B 用 Workers 兜住"**需要一台可信机器**"的少量职责 ✗**
（**如同步、分享、blob 托管 ✓**）；**C 完全离线 ✓（**纯 PWA ✓**）**。**∴ 二者共用同一个前端 ✓。**

## 三、★ GPU 能力矩阵（**这里必须如实 ✓**）★

| 部署 | GPU 可用？ | 结论 |
|---|---|---|
| **A 服务端** | **取决于机器 ✗**（**有 CUDA／Vulkan／Metal 则可用 ✓**） | **★ GPU 优先 ＋ CPU fallback ＋ 参数可关 ✓**（**第 591 轮已入档 ✓**）** |
| **B Cloudflare Workers** | **★ 没有 ✗ ★** —— **V8 isolate **不提供 GPU**✗（**无 WebGPU／无 CUDA ✓**）** | **★ 只能 CPU ✗ ★**（**∴ 用户要求的"Workers 上 GPU 优先"**在技术上无法满足 ✗** ⇒ **∴ 必须如实告知 ✓**）** |
| **C 纯 PWA（**浏览器 ✓**）** | **★ 有 ✓ ★**（**WebGPU ✓，若浏览器支持 ✓**） | **★ GPU 优先 ＋ 不支持时 CPU fallback ＋ 设置项可关 ✓**（**第 591 轮已入档 ✓**）** |
| **D 桌面** | **有 ✓**（**若用 wgpu ✓**） | **★ 同上 ✓** |

**∴ 所以"所有部署 GPU 优先"**在四种里满足三种 ✗**，**Workers 是**宿主限制**✗（**不是我们的取舍 ✓**）**
⇒ **∴ 处置** ✓：**Workers 上**必须显式报出**它用的是 CPU**✗**（**如 `render_backend: "cpu"` ＋
`gpu_unavailable_reason: "host_has_no_gpu"` ✓**）⇒ **∴ 于是"GPU 优先"这条在每条部署上**都可判定 ✓**** ✓✓

## 四、B 的实现路径（**基于现状 ✓**）

| 步 | 做什么 | 依据 |
|---|---|---|
| **①** | **把 `yanshi-wasm` 编成 `wasm32-unknown-unknown` 的**纯计算模块**✗**（**不引入 `web-sys` ✓；`wasm-bindgen` 用 `--target bundler`／`web` ✓**） | **∴ 现状已满足（**零 DOM ✓**）** |
| **②** | **加一个 `worker/` 目录**：**Workers 入口（**JS／TS ✓**）＋ `wrangler.toml` ✓** | **新建 ✓** |
| **③** | **前端做成 PWA**：**`manifest.webmanifest` ＋ service worker（**离线缓存 ✓**）＋ 把现有 `viewer-app.js` 静态化 ✓** | **新建 ✓**（**现有 viewer 是服务端注入的 ✗ ⇒ 要抽出静态壳 ✓**） |
| **④** | **持久化**：**IndexedDB（**替换服务端的 blob ／ 原子日志 ✓**）** | **新建 ✓**（**∴ 这是 B／C 的主要工作量 ✗**） |
| **⑤** | **后端一致性地基**：**内核已有的 `load_atoms_json` ✓／`paint_brush` ✓ 直接复用 ✓** | **现状 ✓** |

**∴ 代价（**两面 ✓**）**：
* **收益 ✓**：**零服务器成本 ✓｜离线可用 ✓｜冷启动只受静态资源影响 ✓**；
* **代价 ✗**：**① Workers **无 GPU**✗（**只能用 CPU ✓**）｜**② 需要**IndexedDB 层的完整性设计 ✗**
  （**原子日志 ＋ 冲突处理 ＋ 存储配额 ✓**）｜**③ 前端要从"服务端注入壳"改成"静态 PWA 壳"✗**
  （**∴ 工作量在⑶⑷ ✗**）｜**④ Workers 的 CPU 时间有限额 ✗**（**∴ 重渲染要放浏览器内核 ✓**）** ✓✓

## 五、可红判据（**每条部署都要能被判定 ✓**）

| # | 判据 | 期望 |
|---|---|---|
| **①** | **每种部署都在响应／诊断里报 `render_backend`** | **`"gpu"` 或 `"cpu"` ✓**（**不许缺 ✗**） |
| **② 服务端** | **`--gpu=off`（**或等价参数 ✓**）⇒ 走 CPU ⇒ 与基线**逐字节相同** | **`diff == 0` ✓** |
| **③ 服务端** | **GPU 可用时 ⇒ **默认**走 GPU ✗** | **`render_backend == "gpu"` ✓** |
| **④ Workers** | **必须报 `render_backend == "cpu"` ＋ 原因 `host_has_no_gpu`** | **不许假装有 GPU ✗** |
| **⑤ PWA** | **WebGPU 可用 ⇒ GPU；不可用 ⇒ CPU fallback；设置里可关** | **三态都可观测 ✓** |
| **⑥ B／C** | **断网后（**service worker 生效 ✓**）仍能打开并绘制 ✓** | **离线可用 ✓** |

## 六、∴ 落地顺序（**先做能验证的 ✓**）

1. **① 判据先立**：**在服务端把 `render_backend` 报出来 ＋ `--gpu=off`（**此时 `cpu` ✓**）** ⇒
   **∴ 无需 GPU 即可验证判据框架 ✓**；
2. **② 服务端 GPU 后端**（**GPU 优先 ＋ fallback ＋ 可关 ✓**）；
3. **③ PWA 壳（**静态化 ＋ manifest ＋ service worker ✓**）⇒ 部署到 Workers ✓**；
4. **④ IndexedDB 持久化层**（**∴ B／C 的主要工作量 ✗**）；
5. **⑤ 内核 WebGPU**（**WebGPU 可用则用 ✓；**∴ 体积代价已由用户接受 ✓**）**。

---

## 七、B（Workers ＋ PWA）的**具体部署方式**（**用户指定 ✓，第 605 轮 ✓**）

用户指示：
> 这个 PWA 到时候 **wrangler 部署到 workers 上**，然后**加到 yanshi 的网站上去**，
> 例如 **`yanshi-online.wangda.today`**。

### 7.1 落地形态

| 项 | 内容 |
|---|---|
| **工具** | **`wrangler`（**Cloudflare 官方 CLI ✓**）** ⇒ **`wrangler deploy` ✓** |
| **产物** | **静态 PWA 资源（**HTML／JS／WASM／manifest／service worker ✓**）＋ Workers 入口脚本 ✓** |
| **域名** | **`yanshi-online.wangda.today`** ✓（**Workers custom domain ✓，或在 yanshi 站点里加一条子域路由 ✓**） |
| **与主站关系** | **"加到 yanshi 的网站上去"** ⇒ **∴ 主站（**`wangda.today` ✓**）上应有入口链接／子域指向 ✓** |
| **服务器依赖** | **无 ✗**（**不连 `yanshi-serve` ✓**）⇒ **∴ 它必须自带持久化（**IndexedDB ✓**）** |

### 7.2 需要新增的仓库内容

| # | 文件／目录 | 作用 |
|---|---|---|
| **①** | **`worker/`** | **Workers 入口（**JS／TS ✓**）＋ 静态资源绑定（**`assets` ✓**）** |
| **②** | **`wrangler.toml`** | **`name`／`main`／`assets`／`compatibility_date`／**custom domain（**`routes` ✓**）** |
| **③** | **`web/`（**或复用现有 viewer 的静态壳 ✓**）** | **PWA：**`index.html` ＋ `manifest.webmanifest` ＋ `sw.js` ✓** |
| **④** | **`wasm` 构建产物** | **`yanshi-wasm` 的 `wasm32-unknown-unknown` 输出（**现状零 DOM ⇒ 可直接上 ✓**）** |
| **⑤** | **IndexedDB 层** | **替换服务端的 blob／原子日志（**B／C 的主要工作量 ✓**）** |
| **⑥** | **CI 工作流** | **`wrangler deploy` 交给 GitHub Actions（**纪律：重活给 CI/CD ✓**；**密钥用仓库 secret ✓**）** |

### 7.3 与 GPU 决定的接口（**重申 ✓**）

**Workers 无 GPU ✗** ⇒ **∴ 该部署会**如实报 `render_backend: "cpu"` ＋ `gpu_unavailable_reason: "host_has_no_gpu"` ✓**
⇒ **∴ 而**"GPU 优先"这条在它上面由**"**报出后端 ＋ 报出原因**"**来满足**✓（**∴ 不许假装有 GPU ✗**）；
**∴ 若将来 Cloudflare 提供 GPU（**如 Workers AI 上的加速 ✓**）⇒ **∴ 那时才把这两行改成**从实际后端读**✓****。

### 7.4 验收（**新增 ✓**）

| # | 判据 | 期望 |
|---|---|---|
| **①** | **`wrangler deploy` 后访问 `https://yanshi-online.wangda.today`** | **PWA 可打开 ✓** |
| **②** | **首次打开** | **`manifest` 被识别 ＋ service worker 注册成功 ✓** |
| **③** | **断网后刷新** | **仍可打开并绘制 ✓** |
| **④** | **其 `/health`（**若保留 ✓**）或诊断** | **`render_backend == "cpu"` ＋ 原因 ✓** |
| **⑤** | **持久化** | **刷新后文档仍在（**IndexedDB ✓**）** |

---

## 八、★ **修正（**用户澄清 ✓，第 606 轮 ✓**）**：PWA **没有服务端计算** ★

用户指示：
> **PWA 只有离线客户端渲染功能，没有服务器端 ✗**，所以 GPU 也只是**离线渲染的 wasm 里头的 GPU ✗**，
> **都是跑在本地浏览器 ✓**。

⇒ **∴ 这**修正**了 §3 里我的一条**错误论断 ✗**：**我曾把"Workers 无 GPU"当作"GPU 优先"的例外 ✗**
⇒ **∴ 而**真实情况是**：**PWA 的渲染**根本不在 Workers 上跑 ✗** ⇒ **∴ 所以 Workers 有没有 GPU**无关 ✓**** ✓✓

### 8.1 修正后的职责划分

| 层 | 职责 | 是否有计算 |
|---|---|---|
| **Cloudflare Workers** | **仅**静态资源托管**（**HTML／JS／WASM／manifest／sw.js ✓**）＋ 可选路由／缓存头 ✓** | **★ 无 ✗ ★**（**不跑 wasm 内核 ✓**） |
| **浏览器（**PWA 前台 ✓**）** | **★ 全部渲染与编辑 ✗**：**wasm 内核（**`yanshi-wasm` ✓**）＋ WebGPU（**若可用 ✓**）＋ IndexedDB 持久化 ✓** | **★ 有 ✓ ★** |
| **GPU** | **＝ 浏览器里的 WebGPU ✗**（**由 wasm 内核调用 ✓**） | **∴ 与 Workers 无关 ✓** |

### 8.2 于是"所有部署 GPU 优先"**全部成立 ✓**

| 部署 | 计算位置 | GPU 来源 | 结论 |
|---|---|---|---|
| **A 服务端** | **服务端进程** | **机器的 GPU（**CUDA／Vulkan／Metal ✓**）** | **★ GPU 优先 ＋ CPU fallback ＋ `--gpu off` ✓** |
| **B Workers ＋ PWA** | **★ 只在浏览器 ✗ ★** | **★ 浏览器 WebGPU ✓ ★** | **★ GPU 优先（**WebGPU ✓**）＋ 不支持则 CPU fallback ✓＋ 设置可关 ✓** |
| **C 纯静态 PWA** | **只在浏览器** | **浏览器 WebGPU** | **同上 ✓** |
| **D 桌面** | **本地进程** | **本地 GPU（**wgpu ✓**）** | **同上 ✓** |

**∴ 所以** ✓：**§3 里那条"Workers 是唯一例外 ✗"**作废 ✗** ⇒ **∴ 正确的说法是**：
**Workers **不参与渲染**✗，**因此它的宿主能力**不构成约束 ✓**；**∴ 而**每一条**真正**跑渲染的路径
（**服务端进程 ✓／浏览器内核 ✓／桌面进程 ✓**）**都执行**GPU 优先 ＋ CPU fallback ＋ 可强制关闭 ✗**** ✓✓

### 8.3 对实现的影响（**收紧 ✓**）

* **§4 的第①步**（**"把 `yanshi-wasm` 编成纯计算模块 ✓"**）**仍然成立 ✓**，
  但**目的变了 ✗**：**不是为了在 Workers 里跑 ✗**，而是为了**在浏览器里以最小体积加载 ✓**
  （**∴ 零 DOM 依赖 ⇒ 可被 Worker 线程／OffscreenCanvas 使用 ✓**）。
* **`/health` 里的 `render_backend`** ✓：**PWA 侧应由**前端**报出（**如诊断面板 ✓**），
  **∴ 而服务端那份仍是**服务端进程**的后端 ✓** —— **∴ 两者**不要混为一谈 ✓**。
* **验收 ④**（**"其 `/health` 报 `cpu`"**）**改为 ✗**：
  **PWA 的诊断应报**浏览器侧后端**✗**（**`webgpu` 或 `cpu` ✓**）＋ **不可用原因 ✓**。

---

## 九、前置条件核验（**实测 ✓，第 607 轮 ✓**）

| 项 | 事实 | 结论 |
|---|---|---|
| **wasm target** | **`wasm32-unknown-unknown` **已安装** ✓** | **★ 具备 ✓ ★** |
| **workspace** | **`crates/yanshi-wasm` 是成员 ✓** | **✓** |
| **★ 产物** | **`crates/yanshi-wasm/pkg/` **已存在** ✓ ★** | **★ 已有 wasm-bindgen 产物 ✓ ★** |
| **浏览器加载** | **`await import("/wasm/yanshi_wasm.js")`** ✓ ⇒ **服务端把 `pkg/` 挂在 `/wasm/` 下** | **✓ 路径已知** |
| **开关** | **`health.wasm`** ⇒ **`--no-wasm` 可关 ✓** | **✓** |
| **DOM 依赖** | **零命中 ✓**（**第 601 轮 ✓**） | **★ 可在 Worker 线程／OffscreenCanvas 里跑 ✓ ★** |

**∴ 所以 PWA 的前置条件**几乎全具备 ✓**：
**① target ✓｜② 产物 ✓｜③ 加载方式已知 ✓｜④ 零 DOM ✓**
⇒ **∴ 剩下的是**把它们**静态化**（**不经服务端注入 ✓**）＋ `wrangler.toml` ＋ `worker/` ✓
⇒ **∴ 而计算**全在浏览器 ✗**（**与 §8 的澄清一致 ✓**）⇒ **∴ Workers 只**托管**这几个文件 ✓** ✓✓

### 9.1 下一步的确切清单（**可执行 ✓**）

| # | 动作 | 依据 |
|---|---|---|
| **①** | **把 `crates/yanshi-wasm/pkg/` 的产物纳入 PWA 资源**（**WASM ＋ JS 胶水 ✓**） | **现状已有 ✓** |
| **②** | **写 `wrangler.toml`**：**`[assets] directory = "web"` ✓ ＋ `name` / `compatibility_date` / `routes`（**`yanshi-online.wangda.today` ✓**）** | **§7 ✓** |
| **③** | **写 `worker/`**：**仅静态托管 ＋（**可选**）缓存头／SPA 回退 ✓** | **§8 ✓（**不做计算 ✓**）** |
| **④** | **把 viewer 抽成**静态壳**（**manifest ＋ service worker ＋ 不依赖服务端注入 ✓**）** | **§4 ③ ✓** |
| **⑤** | **IndexedDB 持久化层** | **§4 ④ ✓** |
| **⑥** | **内核里的 WebGPU 后端 ＋ 报出 `webgpu`／`cpu`** | **§8.2 ✓（**GPU 的真实落点 ✓**）** |

---

## 十、CI 部署工作流（**第 611 轮 ✓**）

**新增** [`.github/workflows/pwa.yml`](../../.github/workflows/pwa.yml) ✓，**8 步** ✓：
**① checkout ｜② Rust ＋ `wasm32-unknown-unknown` ｜③ 构建 `yanshi-wasm` ｜④ 装匹配版本的 `wasm-bindgen-cli`
｜⑤ `wasm-bindgen --target web` 生成 `pkg/` ｜⑥ `node scripts/pwa-sync-wasm.mjs`（**同步到 `web/wasm/` ✓**）
｜⑦ **`node scripts/tool-pwa-assets.mjs`（**★ 判据：产物不完整就**失败即停**✗ ★**）｜⑧ `wrangler deploy` ✓**

**触发时机** ✓（**按仓库约定：部署是重活 ⇒ 不阻塞每次 push ✓**）：
**① 手动 `workflow_dispatch` ✓（**含 `dry_run` 选项 ✓**）｜② 打 `v*` tag 时 ✓（**发版顺带部署 ✓**）**

**∴ 缺 secret 时的行为（**关键 ✓**）**：**未配置 `CLOUDFLARE_API_TOKEN` ⇒ **∴ 不失败 ✗，
而是**跳过部署 ＋ 在 Step Summary 里**如实写明"未部署 ＋ 原因"**✗**（**∴ 不许假装部署成功 ✓**）** ✓✓

### 10.1 代价与诚实说明（**两面 ✓**）

| 面 | 内容 |
|---|---|
| **收益 ✓** | **部署**不需要本地装 wrangler ✗**（**本地实测也确认它未安装 ✓**）｜**发版即部署 ✓**｜**判据在部署前拦截"没有内核的 PWA"✗** |
| **代价 ✗** | **① 我**没有**在真实 CI 上跑过它 ✗**（**∴ Step 的**每一步都在本地单独验证过 ✓，但**整条流水线未跑 ✓**）；**② 需要仓库 secret ✗**（**未配 ⇒ 只校验不部署 ✓**）；**③ `web/wasm/*.wasm`（**1.38 MiB ✓**）已入库 ✗ ⇒ **∴ 每次内核更新都要**提交一个二进制 ✗**（**∴ 好处是判据可离线验证 ＋ 新人 clone 即可跑 ✓**）** |

---

## 十一、★ **隔离边界**（**用户要求 ✓，第 612 轮 ✓**）：PWA **不得影响现有 WEB** ★

用户指示：
> **PWA 是新增的部署方式，不要影响现有的这套 WEB。**

### 11.1 核验结果（**实测 ✓**）

| 检查 | 结果 |
|---|---|
| **`web/` 是否与现有路径撞名** | **★ 没有 ✓ ★** —— 唯一命中是**无关字符串** `surface: "web"`（**诊断面的名字 ✓**） |
| **现有 WEB 的资源目录** | **仍是 `assets/` ✓**（`server.rs:121` `assets_dir: Some(PathBuf::from("assets"))`）⇒ **未被 PWA 改 ✗** |
| **现有内核 URL** | **`/wasm/yanshi_wasm.js`（`server.rs:591` 提供 ✓）** ⇒ **∴ 与 PWA 的 `/wasm/` **同名但不同宿主**✗**（**一个由 `yanshi-serve` 提供 ✓；一个由 Workers assets 提供 ✓**）⇒ **∴ 不冲突 ✓** |
| **PWA 引入的改动** | **全部是**新增文件 ✗**：`wrangler.toml`／`worker/`／`web/`／`scripts/pwa-sync-wasm.mjs`／`scripts/tool-pwa-assets.mjs`／`.github/workflows/pwa.yml` ✓ |

### 11.2 我碰过的**现有文件**（**4 个，全部是增量 ✓**）

| 文件 | 改了什么 | 是否与 PWA 有关 |
|---|---|---|
| **`crates/yanshi-render/src/render.rs`** | **修 `blit` 尺寸缺陷（**增量盖章丢内容**）** | **无关 ✓（**目标本身的缺陷 ✓**）** |
| **`crates/yanshi-http/src/server.rs`** | **加 `render_backend` 字段 ＋ `--gpu` 参数** | **无关 ✓（**GPU 决定 ✓**）** |
| **`scripts/run-criteria.sh`** | **加一条 `tool-pwa-assets.mjs)` 分支** | **有关，但**只增不减 ✓** |
| **`scripts/tool-criteria-coverage.mjs`** | **生成器列表加 `pwa-sync-wasm.mjs`** | **有关，但**只增不减 ✓** |

### 11.3 固化（**可红判据 ✓**）

`tool-pwa-assets.mjs` 新增**四条隔离断言** ✓：
* **PWA 的静态目录**不许**是 `./assets` 或 `assets` ✗**（**∴ 否则会侵占现有 WEB 的资源目录 ✓**）；
* **`worker/index.js` 不许出现渲染调用 ✗**（**`render_region`／`encode_png`／`Renderer`／`composite` ✓**）；
* **`worker/index.js` 必须把请求交给静态资源绑定 ✗**（`env.ASSETS.fetch` ✓）；
* **现有 viewer（`crates/yanshi-http/assets/viewer-app.js`）必须仍然存在 ✓**。

**变异** ✗：**把 PWA 静态目录改成 `./assets` ⇒ **判据报红 ✓**（**已实测 ✓**）。

---

## 十二、⑥ **viewer 静态化**的真实内容（**实测后修正 ✓，第 614 轮 ✓**）

### 12.1 两个发现（**实测 ✓**）

| 发现 | 事实 | 影响 |
|---|---|---|
| **★ 现有 WEB **已经是 PWA**✗ ★** | **`crates/yanshi-http/assets/service-worker.js`（**16 KB ✓**）已存在**，含 `CACHE = "yanshi-shell-__BUILD_ID__"` 与预缓存逻辑 ✓ | **∴ 外壳缓存这件事**已做过 ✓ ⇒ **∴ 不必重做 ✓** |
| **★ 前端**强依赖 15 个服务端端点**✗ ★** | **`viewer-app.js`（**510 KB ✓**）引用 `/api/tools/get_document` ✓／`render_region` ✓／`list_layers` ✓／`/api/documents` ✓／`/api/atoms` ✓／`/api/blob/…` ✓／`/api/effects` ✓／`/api/diagnostics` ✓ 等 ✓ | **∴ 直接搬进 PWA ⇒ 它会**访问不存在的服务端 ✗** |

### 12.2 ★ 方案：**拦截，而不是复制**（**零分叉 ✓**）

**新增** [`web/api-local.js`](../../web/api-local.js) ✓：**在 PWA 里**覆写 `window.fetch`✗**
⇒ **∴ 把 `/api/*` 转到**本地实现**✗**（**内核 wasm ＋ IndexedDB ✓**），**其余（**静态资源 ✓**）走原路 ✓**
⇒ **∴ 于是**：
* **同一份 `viewer-app.js` 两处都能跑 ✓**；
* **`crates/yanshi-http/assets/` **一个字节都不用改 ✓****（**⇒ 满足用户"不要影响现有的这套 WEB"✓**）；
* **`/health` 由本层**如实报"无服务器端"✗**（**不许谎报 ✓**）。

### 12.3 现状与诚实边界（**两面 ✓**）

| 面 | 内容 |
|---|---|
| **已实现 ✓** | **拦截安装／卸载 ✓｜`/health` 如实报无服务器 ✓｜浏览器后端探测（`webgpu`／`cpu` ✓）｜未实现端点**如实 501 ＋ 原因**✗**（**不静默失败 ✓**）** |
| **未实现 ✗** | **15 个端点的真实本地映射 ✗**（**需按端点逐个接内核／IndexedDB ✓**，是后续轮次的工作 ✓）⇒ **∴ 在此之前 PWA 会**明确报"该端点尚未本地实现"✗**，**绝不会假装成功 ✓** |

### 12.4 判据（**已加，含变异 ✓**）

`tool-pwa-assets.mjs` 新增：
* **`web/api-local.js` 必须存在 ＋ 覆写 `window.fetch` ＋ 对未实现端点用 501 ✗**；
* **★ 防分叉 ★**：**`web/viewer-app.js` **不许存在**✗** —— **∴ 前端只能**同步**✗，**不许复制成第二份 ✓**。

**变异实测** ✓：**造一份 `web/viewer-app.js` ⇒ 判据**报红 ✓**；删除 ⇒ 恢复绿 ✓**
（**∴ 于是"不影响现有 WEB"从**口头承诺**变成**可红判据**✗**）

---

## 十三、真实浏览器验证的**三要素**（**骨架已取得 ✓，第 619 轮 ✓**）

**∴ 现状** ✗**：**PWA 有两层证据（**静态 ✓ ＋ node 行为级 ✓**），但**没有真实浏览器证据 ✗****。
**∴ 已确认可行 ✗**：**仓库有 14 个 `browser-*.mjs` 判据 ✗**，**用 **CDP**（**零依赖 ✓**）⇒
**∴ 且本机有 `chromium`（`/usr/bin/chromium` ✓）⇒ **∴ 本地就能验 ✓**** ✓✓

**∴ 骨架** ✓（取自 `browser-undo-disabled.mjs`，**64 行 ✓**）：
```js
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
const socket = new WebSocket(page.webSocketDebuggerUrl);   // **node 有全局 WebSocket ✓**
const send = (method, params) => …;                        // **id ＋ pending map ✓**
const evaluate = async (expr) => (await send("Runtime.evaluate",
  { expression: expr, returnByValue: true, awaitPromise: true })).result.result.value;
await send("Page.navigate", { url });
for (let i = 0; i < 40; i++) { await sleep(300); if (await evaluate('document.readyState === "complete"')) break; }
```

### 13.1 写它所需要的**三件事**（**下一轮 ✓**）

| # | 要素 | 为什么 |
|---|---|---|
| **①** | **自起静态服务器**（`node:http` 提供 `web/` ✓） | **∴ `file://` 下 ES module 与 `import()` 会被 CORS 拒 ✗** ⇒ **∴ 必须走 http ✓** |
| **②** | **自起 chromium**（`--headless=new --remote-debugging-port=…` ✓） | **∴ 现有 `browser-*.mjs` 假定调试端口**已被别处启动**✗**（**`CDP_PORT` 由外部传入 ✓**）⇒ **∴ PWA 判据要自己起 ✓** |
| **③** | **四条断言** | **① `/health` 报 `server:false` ＋ `render_backend` ✗**；**② 内核加载成功（`window.yanshiKernel` 存在 ✓）**；**③ 未实现端点 ⇒ 501 ＋ `endpoint` ＋ `reason` ✓**；**④ 快照缺失的 `render_region` ⇒ `needs_render` ✓** |

**∴ 变异** ✗：**把 `api-local.js` 的 501 分支去掉 ⇒ ③ 必红 ✓**（**与 node 行为级判据同源的变异 ✓**）。

**⚠️ 诚实** ✓：**本轮**没有**产出新判据 ✗**（**只取到骨架并确认其可复用 ✓**）⇒
**∴ 而这一步有效的理由 ✗**：**它把"能不能做"变成"知道怎么写"✗**（**64 行、零依赖、本机可跑 ✓**）** ✓✓

---

## 十四、PWA「真能画图」只差**接线**（**内核已备 ✓，第 623 轮 ✓**）

### 14.1 内核已暴露的能力（**实测 ✓**）

| 内核方法 | 位置 | 用途 |
|---|---|---|
| **★ `render_region_png(x, y, w, h) -> Vec<u8>` ★** | `lib.rs:128` | **★ 直接返回 **PNG** ★** ⇒ **∴ `render_region` 的本地实现**几乎不用写 ✗** |
| **`render_region_rgba`** | `:120` | **原始 RGBA（**客户端自己上屏 ✓**）** |
| **`render_region_direct_rgba`** | `:159` | **直通（**不经缓存 ✓**）** |
| **`load_atoms_json`** | `:110` | **重放整份原子日志 ✓** |
| **`apply_atom_json`** | `:115` | **应用单条原子 ✓** |
| **`new(...)`／`version`／`set_viewport`／`memory_usage`／`evict_outside_viewport`** | `:87` 起 | **生命周期 ＋ 内存预算 ✓** |
| **`set_preview_object`／`extend_preview_stroke`／`commit_preview`／`clear_preview`／`has_preview`** | `:173` 起 | **乐观预览（**笔触实时 ✓**）** |

### 14.2 `render_region` 的本地实现（**下一轮，约 30 行 ✓**）

**∴ 步骤** ✓：
1. **从 IndexedDB 取该文档的原子**（`atomsOf` ✓）⇒ **`kernel.load_atoms_json(JSON.stringify(atoms))` ✓**；
2. **`const png = kernel.render_region_png(x, y, w, h)` ✓**；
3. **`return new Response(png, { headers: { "content-type": "image/png" } })` ✓**；
4. **★ 并把结果写进快照（**带当前 `seq` ＋ `FORMAT_VERSION` ✓**）★**
   ⇒ **∴ 于是**下一次同区域请求**直接命中快照分支 ✓**（**第 617 轮已写 ✓**）；
5. **∴ 内核实例要**缓存 ✗**（**同一文档一个 ✓**）⇒ **∴ 否则每次重建 ⇒ **`load_atoms_json` 重放全部原子 ✗****。

### 14.3 代价与两面（**必须写明 ✓**）

| 面 | 内容 |
|---|---|
| **收益 ✓** | **PWA **真的能渲图**✗（**当前只有快照分支 ⇒ 冷启动给不出图 ✓**）｜**∴ 且**它**复用同一份内核 ✗**（**与服务端同源 ✓ ⇒ **∴ 像素级一致的前提还在 ✓**） |
| **代价 ✗** | **① 首次渲染要**重放全部原子**✗**（**与服务端冷启动同性质 ⇒ **∴ 可接受 ✓**）；**② 内核实例常驻内存 ✗**（**4K 一份 ≈33 MB ⇒ **∴ 必须配 `set_memory_limit` ＋ `evict_outside_viewport` ✓**）；**③ 内核在**主线程**跑会阻塞 UI ✗** ⇒ **∴ 应放 **Web Worker**（**∴ 零 DOM 依赖正好支持 ✓**）** |

### 14.4 精确签名（**实测 ✓，第 624 轮 ✓**）

```rust
#[wasm_bindgen(constructor)]
pub fn new(doc_id: &str, tile_size: u32, width: u32, height: u32, memory_limit: f64)
    -> Result<WasmKernel, JsValue>;                    // lib.rs:87

/// view 模式批量装载：`json_array` 是**服务端 `get_log` 给出的原子数组** ✓。
pub fn load_atoms_json(&mut self, json_array: &str) -> String;      // :110

/// 渲染区域，返回 PNG 字节（**与服务端同一编码器，可直接比对哈希** ✓）。
pub fn render_region_png(&mut self, x: f64, y: f64, w: f64, h: f64) -> Vec<u8>;  // :128

/// 渲染区域并返回**完整元信息**（bbox／宽高／padding／**警告**／tile 数 ✓）。
pub fn render_region_info(&mut self, …) -> String;                 // :136
```

### 14.5 接线配方（**下一轮照此写 ✓**）

```js
// **∴ 内核实例**按文档缓存 ✗**（**否则每次重放全部原子 ✓**）
const k = new mod.WasmKernel(docId, 256 /* tile_size ✓ */, width, height, 256 * 1024 * 1024);
// **∴ `load_atoms_json` 要的是**服务端 `get_log` 形态的数组**✗**
// ⇒ **∴ 而 `store.js` 的 `atomsOf` 返回 `{ id, doc, seq, atom }` ✗ ⇒ **∴ 必须映射成 `r.atom` ✓**
k.load_atoms_json(JSON.stringify((await atomsOf(db, docId)).map((r) => r.atom)));
const png = k.render_region_png(x, y, w, h);            // **Uint8Array ✓**
await writeSnapshot(db, docId, seq, { bytes: png });    // **∴ 带 seq ⇒ 下次命中快照分支 ✓**
return new Response(png, { headers: { "content-type": "image/png" } });
```

**★ 同源证据（**重要 ✓**）**：**`render_region_png` 的注释写明"**与服务端同一编码器，
可直接比对哈希**✗"** ⇒ **∴ 于是**PWA 与服务端的**像素一致性**有据可依 ✓
（**∴ 这也是"**GPU 上不做真值**✗"这一分层原则的基石 ✓：**CPU 路径两处同源 ✓**）。

### 14.6 接线进度（**第 625 轮 ✓，如实 ✓**）

**已打通 ✓**（**真实浏览器实测 ✓**）：
* **`await mod.default()`** ⇒ **∴ wasm 初始化 ✓**（**∴ 否则 `new WasmKernel` 报
  `Cannot read properties of undefined (reading '__wbindgen_malloc')` ✗ —— **实测踩过 ✓**）；
* **`new WasmKernel(doc, 256, w, h, 256 MiB)`** ✓ ⇒ **∴ 构造成功 ✓**；
* **`load_atoms_json(原子数组)`** ✓ ⇒ **∴ 装载成功 ✓**；
* **区域裁剪到画布内** ✓（**∴ 越界会返回空字节 ✗ —— **实测踩过 ✓**）。

**未打通 ✗**：**`render_region_png` 返回**空字节**✗** ——
**∴ 而它的实现是 `… .map(|r| r.png).unwrap_or_default()`（`lib.rs:44-47` ✓）
⇒ **∴ 即**渲染返回了 `Err` ✗，**而错误被 `unwrap_or_default()` **吞掉**✗**
⇒ **∴ 于是**前端只会看到"内核未产出字节"✓，**看不到真正原因 ✗**** ✓✓

**∴ 下一步（**明确 ✓**）**：
1. **先调 `render_region_info(x, y, w, h)`（`lib.rs:136` ✓）** ⇒ **∴ 它返回 JSON（**含
   **bbox／宽高／padding／**警告**／tile 数 ✓**）⇒ **∴ 警告里应当写着失败原因 ✓**；
2. **∴ 据原因修 ✗**（**∴ 常见嫌疑**：**原子形态 ／ 层未创建 ／ 画布尺寸不匹配 ✓**）；
3. **∴ 并把这条**如实报错**保留 ✗**：**∴ 内核失败时**绝不返回空 `image/png` ✓**
   （**∴ 而是 `needs_render` ＋ 原因 ✓** —— **∴ 现状已如此 ✓**）。

### 14.7 失败原因**已能取到**（**第 626 轮 ✓**）

**`render_region_info` 给出的 `kernel_info`**（**实测 ✓**）：
```
{"context":{"detail":"invalid_argument: 渲染区域 B…
```
⇒ **∴ 即**内核**拒绝了这个区域参数**✗**（**`invalid_argument` ✓**）——
**∴ 而**这正是 `render_region_png` 的 `Err` 被 `unwrap_or_default()` 吞掉的那一条 ✓**
⇒ **∴ 于是**我们**不再靠猜 ✗**：**失败原因**可以从元信息里读出来 ✓**** ✓✓

**∴ 已做的处置** ✓：**把 `kernel_info` 放进 `needs_render` 的响应里 ✗**（**截断 600 字符 ✓**）
⇒ **∴ 于是**前端／诊断**能看到原因 ✗**（**∴ 而**不是只看到"未产出字节"✗**）** ✓✓

**∴ 下一步（**一步 ✓**）** ✓：**在页面里**解析** `kernel_info` ✗**（**它是 JSON 字符串 ✓**）
⇒ **∴ 拿到 `context.detail` 的**完整文本 ✗** ⇒ **∴ 它会写明区域的**合法范围／限制**✗**
⇒ **∴ 据此**修参数 ✓**（**∴ 嫌疑**：**bbox 必须是 tile 对齐 ✓／或 w·h 有上限 ✓／或坐标须为整数 ✓**）** ✓✓

### 14.8 为什么内核说「文档 0×0」（**第 627 轮 ✓，真因已锁定 ✓**）

**内核的完整回复** ✓（**解析后读出 ✓**）：
```json
{"context":{"detail":"invalid_argument: 渲染区域 Bbox { x: 0.0, y: 0.0, w: 256.0, h: 256.0 } 与文档 0×0 不相交"},
 "error_code":"invalid_argument","ok":false,"retryable":false}
```

**∴ 这不是内核缺陷 ✗** —— **∴ 内核**正确地**拒绝了一个"空文档上的渲染"✓**。
**∴ 真因** ✓：**本判据造的原子是**最小形态**✗**
（**只有 `kind: "create_document"` ＋ `width`／`height` ✓**），
**∴ 而**内核折叠要求**与服务端 `get_log` 同形的完整原子 ✗**
（**含 `id`／`seq`／`session`／`precondition` 等 ✓）⇒ **∴ 于是**折叠**未生效**✗
⇒ **∴ 文档保持 0×0 ✓**** ✓✓

**∴ 且**已确认 ✓：**`create_document` **就是**正确的 `kind`**（`yanshi-core/src/atom.rs:344` ✓）
⇒ **∴ 所以**问题**不在名字 ✗，而在**字段完整性**✗**** ✓✓

### 14.9 下一步（**一步 ✓**）

**用**真正的原子构造器**造原子 ✗**（**而不是手写最小 JSON ✓**）——
**∴ 如**复用 `yanshi-core` 的 `Atom` 构造 ＋ 序列化 ✗**，**或**把服务端 `get_log` 的样例形态抄成判据夹具 ✓**
⇒ **∴ 一旦原子合法 ⇒ **∴ 内核文档会有正确尺寸 ⇒ **∴ 渲染应当产出真 PNG ✗****
⇒ **∴ 那时判据自动升级 ✗**（**本判据已写好那条分支 ✓**：**`img.type === "image/png"` 时
断言 `local-kernel` ＋ 二次命中 `local-snapshot` ✓**）** ✓✓

### 14.10 ★ **权威原子形态**（**第 628 轮 ✓，一条就够 ✓**）★

**来源** ✓：**`crates/yanshi-server/src/tools.rs:1736`** ✓ 的参数声明 ——
```rust
params: &[param!("atoms", Array, true, "[{kind, payload, actor?, session?}] 离线期间追加的原子")],
```

⇒ **∴ 正确形态** ✓：**`{ kind, payload, actor?, session? }`** ——
**∴ 而**我此前手写的是**字段铺平**✗**（`{ kind: "create_document", width, height }` ✓）
⇒ **∴ 所以**内核**折叠不出来**✗ ⇒ **∴ 文档保持 0×0 ✓**（**∴ 与 §14.8 的观测一致 ✓**）** ✓✓

**∴ 另外一条** ✓：**`/api/atoms` 的 POST 要 `{ atoms: [ … ] }` ✗**
（**∴ 我实测直接 POST `{}` ⇒ **400 Bad Request**✓ —— **∴ 因为 `atoms` 是**必填**✓**）** ✓✓

**∴ 前端也是这么用的** ✓：**`viewer-app.js:1313`** ✓
```js
const loaded = JSON.parse(state.kernel.load_atoms_json(JSON.stringify(atoms.atoms)));
```
⇒ **∴ 它把服务端 `/api/atoms` 返回的数组**原样喂给内核 ✗** ⇒ **∴ 所以**那份数组**就是**内核真值形态**✗** ✓✓

### 14.11 下一步（**一步 ✓**）

**把 PWA 侧的原子改成**嵌套 `payload`**✗**：
* **`api-local.js` 的 `/api/atoms`（**POST ✓**）**：**改为接受 `{ atoms: [ … ] }` ✗，
  并把每条原子**原样存下**（**含 `payload` ✓**）；**
* **`render_region` 的内核分支**：**`k.load_atoms_json(JSON.stringify(记录的原子))` ✗**
  （**∴ 即**直接传原子 ✗，**不再包装成 `r.atom` 的自定义形状 ✓**）**；
* **浏览器判据**：**造 `{ kind: "create_document", payload: { width: 256, height: 256, … } }` ✗**
  ⇒ **∴ 于是**内核文档有正确尺寸 ⇒ **∴ 渲染应当产出真 PNG ✗** ⇒ **∴ 判据自动升级 ✓**（**§14.9 ✓**）** ✓✓

### 14.12 ★★ **权威原子样本**（**第 629 轮 ✓，实测自 `/api/atoms` ✓**）★★

**请求** ✓：**`GET /api/atoms?doc=…&token=…`** ⇒ **键 ＝ `{ atoms, count, doc_id, head_seq, ok, since }`** ✓
（**∴ 而 `/api/tools/get_log` 是 **POST**（**GET ⇒ 405** ✓）**）** ✓✓

**`atoms[0]` 的全部字段** ✓（**8 个 ✓**）：
```json
{
  "actor": "human:web",
  "id": "01M4FJ06KTR82YTZ6Y8DD90000",
  "kind": "create_document",
  "payload": {
    "background": { "a": 255, "b": 255, "g": 255, "r": 255 },
    "color_space": "srgb",
    "doc_id": "s_1791523560",
    "height": 256,
    "width": 256
  },
  "schema_version": 1,
  "seq": 1,
  "session": "session:web",
  "timestamp": 1791523560058
}
```

**∴ 所以内核要求的**最小合法原子**✗ ＝ **`id` ＋ `seq` ＋ `kind` ＋ `payload` ＋
`schema_version` ＋ `actor` ＋ `session` ＋ `timestamp`** ✓
—— **∴ 而**`payload` 里**创建文档**还需要 **`doc_id` ＋ `width` ＋ `height` ＋ `color_space` ＋ `background` ✓**** ✓✓

**∴ 这解释了**§14.10 之后仍然 0×0 的原因 ✗**：
**∴ 我补齐了 `payload` 这一层 ✓，**但**仍缺 `id`／`seq`／`schema_version`／`actor`／`session`／`timestamp` ✗**
（**∴ 且**`payload` 里缺 `doc_id`／`color_space`／`background` ✓）⇒ **∴ 折叠仍被拒 ✗** ✓✓

### 14.13 下一步（**最后一步 ✓**）

**把 §14.12 的**完整样本**抄进浏览器判据的夹具 ✗**（**8 个字段 ＋ 完整 `payload` ✓**）
⇒ **∴ 于是**内核文档尺寸正确 ⇒ **∴ `render_region` 应产出真 PNG ✗**
⇒ **∴ 判据**自动升级 ✓**（**§14.9 ✓**：**断言 `local-kernel` ＋ 二次命中 `local-snapshot` ✓**）** ✓✓

**∴ 且**PWA 侧也应存**同样完整**的原子 ✗**（**`api-local.js` 的 `/api/atoms` POST ✓**）
⇒ **∴ 否则**"本地能画"只在判据夹具上成立 ✗** ⇒ **∴ 而那会是**假证据 ✗** ⇒ **∴ 必须两边一起改 ✓**。

### 14.14 🎉🎉🎉 **PWA 真的画出了图**（**第 630 轮 ✓，真实浏览器实测 ✓**）

```
render_region（首次）⇒ status 200 ｜ type image/png ｜ source **local-kernel** ｜ **850 字节** ✓
render_region（第二次）⇒ status 200 ｜ source **local-snapshot** ✓
fold_result ⇒ {"atoms":3,"head_seq":3,"ok":true} ✓
✓ PWA 在真实浏览器里成立 ｜ EXIT = 0 ✓
```

**∴ 为了走到这一步，一共澄清了**五层**✗**（**每层都由**实测**而非推理得出 ✓**）：

| # | 层 | 关键事实 |
|---|---|---|
| **①** | **能力** | **内核有 `render_region_png` ✓**（**与服务端同一编码器 ⇒ 可直接比对哈希 ✓**） |
| **②** | **初始化** | **`--target web` 必须先 `await mod.default()` ✗**（**否则 `__wbindgen_malloc` undefined ✓**） |
| **③** | **构造** | **`new WasmKernel(doc, 256, w, h, limit)` ✓** |
| **④** | **原子形态** | **完整 **8 字段封套**✗**：`id`／`seq`／`kind`／`payload`／`schema_version`／`actor`／`session`／`timestamp`；**创建文档的 `payload` 还要 `doc_id`／`width`／`height`／`color_space`／`background` ✓** |
| **⑤** | **序号** | **`seq` 必须**由本地层自增**✗**（**两处都写 1 ⇒ `seq 1 重复` ✓**）｜**快照的 `expected` 默认应为**当前 head**✗**（**默认 0 ⇒ 永不命中 ✓**） |

**★ 而每一次突破都靠**把量打印出来**✗，**而不是靠推理 ✗**：
`render_region_info`（**拿警告 ✓**）⇒ **解析 JSON（**拿 `与文档 0×0 不相交` ✓**）⇒
`load_atoms_json` 的返回值（**拿 `missing field id` ＋ `seq 1 重复` ✓**）** ✓✓

### 14.15 计数与状态（**第 631 轮 ✓**）

**`LOCAL_IMPLEMENTED` ＝ 6** ✓（**实现分支 5 ＋ `/health` ✓**）——
**∴ `render_region` 已从"部分实现"升级为**完整实现**✗**
（**∴ 它现在真的调用本地内核渲出 PNG ✗**：**真实浏览器实测 850 字节 ✓**）。

**∴ 判据会**核对声明与实现是否一致 ✗**（`declared === actualImpl + 1` ✓）
⇒ **∴ 若哪天有人**虚报**✗ ⇒ **∴ 判据当场报红 ✓**** ✓✓

**∴ 三条 PWA 判据当前状态** ✓：**`ASSETS=ok` ✓｜`BEH=ok` ✓｜`BROWSER=ok` ✓**

### 14.16 ★ **真界面的关键在 Rust 里的 HTML 模板**（**第 633 轮 ✓，实测 ✓**）★

**实测** ✓（`crates/yanshi-http/src/viewer.rs` ✓）：
* **HTML 不是静态文件 ✗** —— **它是**Rust 里拼出来的**✗**（**`viewer.rs:32` 起 `<html lang="zh-CN">` ✓**）；
* **CSS 与 JS 被**内联**✗**（**`include_str!("../assets/viewer.css")` ✓／
  **`include_str!("../assets/viewer-app.js")` ✓** ⇒ **∴ 它们被嵌进那份 HTML ✓**）；
* **页面只引用少量静态资源** ✓（**`/favicon.svg` ✓／`/brand/svg/icon-light.svg` ✓ 等 ✓**）。

**∴ 所以** ✓：**只同步 `viewer-app.js` ＋ `viewer.css` **不够 ✗**** ——
**∴ `viewer-app.js` 依赖一份**特定的 DOM 结构**✗**（**由那份 Rust 模板提供 ✓**）
⇒ **∴ 于是**要做出"**真界面**"的 PWA ✗ ⇒ **∴ 必须有一条**导出静态 HTML**的路 ✗**** ✓✓

### 14.17 下一步（**明确 ✓**）

| # | 做法 | 代价 |
|---|---|---|
| **① ★ 推荐 ★** | **让 `yanshi-serve`（**或一个小 bin ✓**）把 `viewer.rs` 生成的那份 HTML **写成一个静态文件**✗**（**如 `assets/viewer.html` ✓**）⇒ **∴ 而后**PWA 同步脚本把它拷进 `web/index.html` ✗** | **小 ✓（**一条导出路径 ✓**）｜**∴ 且**与服务端**同一份模板**✗ ⇒ **∴ 天然同源 ✓**** |
| **②** | **手写一份 PWA 专用 HTML ✗** | **∴ 会**分叉**✗**（**违背用户要求 ✓**）⇒ **∴ 不采用 ✓** |

**∴ 判据** ✓：**导出后 ⇒ **`web/index.html` 与服务端生成的那份**逐字节相同**✗**
（**∴ 沿用第 632 轮"**同步而非分叉**"的同一条规则 ✓**）** ✓✓

### 14.18 导出静态页：**只差一个 CLI 参数**（**第 634 轮 ✓，实测 ✓**）

**已确认** ✓：
* **`pub fn page_with_read_tools() -> String`**（`crates/yanshi-http/src/viewer.rs:16` ✓）——
  **∴ 它就是**那一份完整的 HTML**✗**（**`server.rs:522` 用它回应 `GET /` ✓**）；
* **它是**纯函数**✗**（**只需 `yanshi_server::tools::ALL_TOOLS` ✓ ⇒ **∴ 无副作用 ✓**）
  ⇒ **∴ 所以**把它**写进文件**只是几行 ✓；
* **参数解析在 `HttpOptions`（`:48` ✓）的 `impl`（`:126` ✓）里 ✗**
  ⇒ **∴ 加一个 `--export-viewer-html <path>` ＋ 一个**早退**✗** 即可 ✓。

### 14.19 下一轮（**一步 ✓**）

1. **在 `HttpOptions` 里加 `export_viewer_html: Option<PathBuf>` ✓**；
2. **解析 `--export-viewer-html <path>` ⇒ 设字段 ⇒ 在参数解析结束后**写文件 ＋ `return Ok(())` ✗**；
3. **`pwa-sync-viewer.mjs` 增一步**：**若服务端可用 ⇒ 调它导出 ⇒ 拷进 `web/index.html` ✗**
   （**∴ 或**由 CI 在构建后跑一次 ✓**）；
4. **判据（**沿用第 632 轮的规则 ✓**）**：**`web/index.html` 与服务端生成的页面**逐字节相同 ✗**。

**∴ 收益（**两面 ✓**）**：
* **收益 ✓**：**PWA 有了**真界面**✗，**且与服务端**同一份模板**✗（**天然同源 ✓**）；
* **代价 ✗**：**多一个 CLI 参数 ✗**（**∴ 而**它与现有参数同构 ✓）＋ **∴ 导出要在**构建时**跑一次 ✗**（**∴ 因此 `web/index.html` 会入库 ✗ ⇒ **∴ 与 wasm 同样处理 ✓**）** ✓✓

### 14.20 插入点已全部定位（**第 635 轮 ✓，四处 ✓**）

| # | 位置 | 改什么 |
|---|---|---|
| **①** | **`pub struct HttpOptions {`（`:48` ✓）的字段区末尾** | **加 `pub export_viewer_html: Option<std::path::PathBuf>,` ✓** |
| **②** | **`impl Default for HttpOptions`（`:84` ✓）**，**`assets_dir: Some(PathBuf::from("assets")),` 那一行之后** | **加 `export_viewer_html: None,` ✓** |
| **③** | **`parse_args`（`:128` ✓）里，`"--gpu" => { … }` 分支**之前** | **加 `"--export-viewer-html" => { options.export_viewer_html = Some(value_of("--export-viewer-html")?.into()); }` ✓** |
| **④** | **`Ok(options)`（`:193` ✓）之前** | **加早退：**若 `export_viewer_html` 有值 ⇒ `std::fs::write(path, viewer::page_with_read_tools())` ＋ **`return Ok(options)`／或直接退出 ✓** |

**⚠️ 本轮我又踩了自己的坑 ✗**：**在 python 三引号里嵌了多行 Rust 字符串 ✗**
⇒ **∴ `SyntaxError: unterminated triple-quoted string` ⇒ **∴ 脚本在写入前中止 ⇒ **树干净 ✓****
⇒ **∴ 这已是本会话第 4 次同类 ✗** ⇒ **∴ 硬规则重申** ✓：
**在 python 里给 Rust 插**多行内容**✗ ⇒ **只能**用单行字符串列表拼 `\n`**✗**，
**绝不把多行内容写进三引号 ✗**** ✓✓

### 14.21 ⚠️ 直接替换 `web/index.html` **会破两处**（**第 637 轮 ✓，实测 ✓**）

**我做了什么** ✓：**让同步脚本调 `--export-viewer-html` 并把结果**直接写成 `web/index.html`**✗**
⇒ **∴ 逐字节相同 ✓（**573 KiB ✓**）** ⇒ **∴ 但**两处判据立刻破了 ✗**：

| # | 破了什么 | 为什么 |
|---|---|---|
| **①** | **`ASSETS=bad`** ✗ | **∴ 服务端页面**必然引用 `/api/*`✗**（**它本来是给服务端用的 ✓**）⇒ **∴ 而我第 614 轮那条"`index.html` 不许出现服务端 API 调用"✗ ⇒ **∴ 它**过时了 ✓**（**∵ 现在 `/api/*` 由 `api-local.js` 接管 ✓**） |
| **②** | **`BROWSER=bad`（**内核加载 false ✓**）** ✗ | **∴ 服务端页面里**没有我那段"安装本地 API"✗**（**那是骨架里的 ✓**）⇒ **∴ 于是 `api-local.js` 没被加载 ⇒ `/api/*` 打到静态服务器 ⇒ 404 ✓** |

**∴ 正确的做法（**下一步 ✓**）** ✓：**导出 ＋ **注入**✗** ——
**在导出的页面 `</body>` 之前**追加一段固定的 `<script type="module">`✗**
（**内容 ＝ 安装 `api-local.js` ＋ `store.js` ✓**）⇒ **∴ 而**判据相应改为
**"**除那段固定注入外**，其余逐字节与导出结果相同**✗**（**∴ 注入是**唯一允许的差异**✗，**且它自身也受断言约束 ✓**）** ✓✓

**∴ 处置** ✓：**本轮**回退**✗（**`web/index.html` 恢复骨架 ⇒ 三判据回绿 ✓**）
⇒ **∴ 而**同步脚本里的导出步骤**保留 ✓**（**它本身正确 ✓**）⇒ **∴ 只是**暂不写入 `index.html` ✗** ✓✓

### 14.22 换成服务端页面后，**两处需要改**（**第 638 轮 ✓，实测 ✓**）

**已成功** ✓：**导出 ＋ 注入** ⇒ **`web/index.html` ＝ 导出结果 ＋ 固定注入（**939 字节 ✓**）**
⇒ **∴ 实测"除注入外逐字节相同 ＝ 是 ✓"** ⇒ **∴ 判据也已改为该规则 ⇒ `ASSETS=ok` ✓**。

**∴ 但**浏览器判据变红 ✗，**两处原因**（**都已定位 ✓，尚未修 ✗**）：

| # | 现象 | 分析 |
|---|---|---|
| **①** | **`内核加载 window.yanshiKernel = false`** ✗ | **∴ 那是**骨架**里的探测 ✗** —— **∴ 服务端页面**不用那个全局 ✗**（**它有自己的一套初始化 ✓**）⇒ **∴ 所以**判据不能再等它 ✓ |
| **②** | **渲染返回 9 字节（**＝ `"not found"` ✓）** ✗ | **∴ 说明注入的脚本**没有生效 ✗**（**`/api/*` 打到了静态服务器 ✓**）⇒ **∴ 待查**：**注入位置／`await` 在 module 顶层 ✓／或 `api-local.js` 在服务端页面语境下抛错 ✓** |

**∴ 处置** ✓：**本轮**回退 `web/index.html`（**恢复骨架 ⇒ 三判据回绿 ✓**）**，
**∴ 而**保留**（**它们都正确 ✓**）：**导出能力 ✓／注入片段 `web/pwa-inject.html` ✓／
判据的"除注入外逐字节相同"规则 ✓／同步脚本的注入逻辑 ✓**** ✓✓

**∴ 下一步** ✓：**让判据改用**与页面无关的就绪信号**✗**
（**如**等 `window.__pwaInstalled` ✓／**或**直接探测 `fetch("/health")` 的返回值 ✓**）
⇒ **∴ 并**查清注入为何未生效 ✗**（**∴ 用**把 `__pwaInstallError` 打出来**的办法 ✓**）** ✓✓

### 14.23 ★★ **导出形态下：注入生效 ＋ 真的渲出图**（**第 639 轮 ✓，实测 ✓**）★★

**把 `web/index.html` 换成"导出 ＋ 注入"后，浏览器判据给出**决定性的**读数 ✗**：

```
内核加载 window.yanshiKernel = false          ← **唯一的红 ✓**（**它只是调试全局 ✓**）
★ 渲染已产出真图 ✗：**bytes=850｜source=local-kernel** ★   ← **★ 注入生效 ＋ 内核真的工作 ★**
❌ 只剩一条：内核未在浏览器里加载成功（window.yanshiKernel = false）
```

⇒ **∴ 结论（**重要 ✓**）** ✓：
* **★ 注入**确实生效了 ✗** —— **∴ 服务端页面里的 `/api/*` 被 `api-local.js` 接管 ✓**；
* **★ 内核**真的渲染出了 850 字节 PNG ✗**（**`local-kernel` ✓**）
  ⇒ **∴ 也就是说：换成导出形态后，PWA **已经是一个真的能画的工作台**✗（**内核 ✓ ＋ 接管 ✓ ＋ PWA 壳 ✓**）**；
* **∴ 唯一的红是**判据自己的问题 ✗**：**它把 `window.yanshiKernel` 当**必要条件**✗**
  ⇒ **∴ 而**那个全局**只在 `?debug=1` 且服务端 viewer 初始化完成后才挂 ✗**
  ⇒ **∴ 所以**在导出形态下它可能不出现 ✓（**∴ 而**内核**照样工作 ✓**）** ✓✓

### 14.24 下一步（**两小步 ✓**）

1. **判据**：**把"等 `window.yanshiKernel`"改成**以"渲染能出图"为内核证据**✗**
   （**∴ 即**：**`render_region` 返回 200 ＋ `image/png` ＋ 非空字节 ⇒ **∴ 内核已被加载并工作 ✓****）
   ⇒ **∴ 这才是**本质证据 ✗**，**而**调试全局只是**便利 ✓**；
2. **然后**再把 `web/index.html` 切到导出形态 ✓ ⇒ **∴ 预期全绿 ✓**。

**⚠️ 本轮**回退**的原因 ✗**：**我在改判据时**一次改了三处 ✗**（**替换 ＋ 插入声明 ＋ 插入置位 ✓**）
⇒ **∴ 而**python 的小错让它们**不一致 ⇒ **∴ 判据语法自相矛盾 ⇒ 红 ✓**
⇒ **∴ 教训**：**同一次改动里**不要同时换断言 ＋ 引入新变量 ✗** ⇒ **∴ 分两步 ✓**** ✓✓

### 14.25 快照**过期**的可红判据（**第 641 轮 ✓；目标第 6 条 ✓**）

**在行为级判据里新增** ✓（**`tool-pwa-api-behaviour.mjs` ✓**）：
```js
await writeSnapshot(db, "d1", 3, { bytes: snapBytes });
check(!!(await readSnapshot(db, "d1", 3)), "序号匹配时应能读到快照 ✗");
check((await readSnapshot(db, "d1", 4)) === null,
      "序号**不**匹配时**必须**返回 null ✗（**∴ 否则就是拿旧图冒充 ✗**）");
```

**∴ 实测** ✓：**`快照：序号匹配 ⇒ 命中 ✓｜序号不匹配 ⇒ null ✓（format=1）`** ✓
**∴ 变异 ✗**：**去掉 `if (meta.seq !== expectedSeq) return null;` ⇒ **判据**报红（**EXIT 1** ✓）****；**恢复 ⇒ 绿 ✓**

**∴ 意义** ✓：**目标第 6 条那句「**快照必须带序号 ＋ 格式版本校验；过期／缺失就重算；
绝不许拿旧图冒充**✗」现在有**双重**可红判据 ✗**：
**① 静态**（`tool-pwa-assets.mjs` ✓：**断言两处校验**存在 ✓**）｜
**② 行为**（**本节 ✓：**真的调用 ＋ 序号不匹配必须返回 null ✓**）** ✓✓

### 14.26 持久化断言：**插入点必须在清理之前**（**第 642 轮 ✓，两次试错 ✓**）

**目标** ✓：**加一条**刷新后文档仍在**的断言 ✗**（**`/api/atoms` 的 `count` 不减少 ✓**）
⇒ **∴ 那是**"无服务器部署的最后一道保证"**✗**（**IndexedDB 真的持久化 ✓**）** ✓✓

**∴ 两次都失败 ✗，原因都很具体 ✓**：

| # | 失败 | 原因 |
|---|---|---|
| **①** | **`EXIT=13` ＋ `Warning: Detected unsettled top-level await`（`:193` ✓）** | **∴ 还有一条**旧的 `render_region` 断言**✗** 在 `await r.json()` ✗** ⇒ **∴ 而响应是 PNG ✗** ⇒ **∴ 抛未捕获错误 ⇒ **∴ 后面的代码**根本没跑 ✓**（**∴ 该旧断言已删 ✓**） |
| **②** | **`EXIT=1` ＋ 无输出** ✗ | **∴ 我用"找第一个 `chrome.kill(); server.close();`"定位 ✗** ⇒ **∴ 而**文件里它**出现在**早期**✗**（**∴ 于是**持久化块被插到了**变量尚未定义**处 ⇒ 崩 ✓**）** ✓✓ |

**∴ 正确插入点** ✓（**下一轮 ✓**）：**必须在**清理之前 ✗**，**且在**所有前置变量定义之后 ✗** ——
**∴ 具体**：**在 `render_region` 的第二次断言之后 ✗、**在 `if (failures.length)` 之前 ✗**** ✓✓
（**∴ 而**不是"第一处 `chrome.kill`" ✗ —— **∴ 那个位置在文件早期 ✓**）** ✓✓

**⚠️ 教训** ✓：**用文本匹配"找第一个"✗ ⇒ **在文件里可能命中**注释或早期分支**✗**
⇒ **∴ 应当**先打印命中行号 ＋ 上下文再改 ✗**（**∴ 我这次没先看 ✓**）** ✓✓
