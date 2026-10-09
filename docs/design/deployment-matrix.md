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
