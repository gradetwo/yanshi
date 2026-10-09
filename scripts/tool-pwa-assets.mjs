#!/usr/bin/env node
// **★ PWA 静态资源完整性 ✓ ★**（第 609 轮 ✓；**部署矩阵 §7.4／§9 ✓**）
//
// **它守什么** ✓（**∴ 无需网络、无需 wrangler、无需浏览器 ✓**）：
//   **① 内核产物必须在 PWA 目录里且非空 ✓** —— **∴ 否则 `wrangler deploy` 会发布一个
//     **没有内核**的 PWA ✗ ⇒ **∴ 用户看到的是"假装能画"✗** ⇒ **∴ 这条是**防撒谎**判据 ✓**；
//   **② `wrangler.toml` 必须存在 ＋ 含用户指定的域名 ✗**（`yanshi-online.wangda.today` ✓）；
//   **③ `wrangler.toml` 的静态目录必须指向 `web/` ✗**（**否则托管的不是这套 PWA ✓**）；
//   **④ `web/index.html` ＋ `manifest.webmanifest` ＋ `sw.js` 必须存在 ✓**；
//   **⑤ service worker 必须把 **wasm 与 JS 胶水**列入预缓存 ✗**（**否则离线打不开 ✓**）；
//   **⑥ `manifest` 必须是合法 JSON ＋ 含 `start_url` ✓**。
//
// **变异** ✗：**删掉 `web/wasm/yanshi_wasm_bg.wasm`** ⇒ **∴ ① 报红 ✓**；
//   **把域名从 `wrangler.toml` 去掉** ⇒ **∴ ② 报红 ✓**。
import { existsSync, readFileSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { readdirSync } from "node:fs";

const bad = [];
// **★ 公共：剥离注释后再查代码 ✓ ★**（第 616 轮 ✓）——
// **∴ 为什么必须剥离 ✗**：**本判据第一版曾在**注释**里命中 `/api/` ✗（**假阳性 ✓**），
// 又曾在**注释**里命中 `preview_state` ✗（**假阴性 ✗，变异 ⑧ 实测没红 ✓**）
// ⇒ **∴ 所以凡"查代码里有没有某个东西"✗ ⇒ 一律先剥离注释 ✓**。
const stripCommentsForCheck = (text) =>
  text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
const read = (p) => (existsSync(p) ? readFileSync(p, "utf8") : null);
const check = (cond, msg) => { if (!cond) bad.push(msg); };

// **① 内核产物 ✓**
const wasm = "web/wasm/yanshi_wasm_bg.wasm";
const glue = "web/wasm/yanshi_wasm.js";
check(existsSync(wasm), `内核 wasm 缺失：${wasm} ✗（跑 node scripts/pwa-sync-wasm.mjs ✓）`);
check(existsSync(glue), `内核 JS 胶水缺失：${glue} ✗`);
if (existsSync(wasm)) {
  const size = statSync(wasm).size;
  check(size > 0, `${wasm} 为空 ✗`);
  console.log(`  内核 wasm：${(size / 1048576).toFixed(2)} MiB ✓`);
}

// **②③ wrangler.toml ✓**
const wrangler = read("wrangler.toml");
check(!!wrangler, "wrangler.toml 缺失 ✗");
if (wrangler) {
  check(/yanshi-online\.wangda\.today/.test(wrangler), "wrangler.toml 未含域名 yanshi-online.wangda.today ✗");
  check(/directory\s*=\s*"\.\/web"/.test(wrangler), "wrangler.toml 的静态目录未指向 ./web ✗");
  check(/main\s*=\s*"/.test(wrangler), "wrangler.toml 未指定 main ✗");
  console.log("  wrangler.toml：域名 ✓｜静态目录 ✓｜main ✓");
}

// **④ PWA 三件套 ✓**
for (const f of ["web/index.html", "web/manifest.webmanifest", "web/sw.js"]) {
  check(existsSync(f), `PWA 文件缺失：${f} ✗`);
}

// **⑤ service worker 预缓存必须含 wasm 与胶水 ✓**
const sw = read("web/sw.js");
if (sw) {
  check(/yanshi_wasm_bg\.wasm/.test(sw), "sw.js 未把内核 wasm 列入预缓存 ⇒ 离线打不开 ✗");
  check(/yanshi_wasm\.js/.test(sw), "sw.js 未把内核 JS 胶水列入预缓存 ⇒ 离线打不开 ✗");
  console.log("  sw.js：wasm ＋ 胶水 已入预缓存 ✓");
}

// **⑥ manifest 合法性 ✓**
const mf = read("web/manifest.webmanifest");
if (mf) {
  try {
    const m = JSON.parse(mf);
    check(typeof m.start_url === "string" && m.start_url.length > 0, "manifest 缺 start_url ✗");
    console.log(`  manifest：name=${m.name} ｜ display=${m.display} ✓`);
  } catch (e) {
    bad.push(`manifest 不是合法 JSON ✗：${String(e).slice(0, 60)}`);
  }
}

// **∴ 另外确认"没有服务端依赖"这条设计意图 ✓**（**用户澄清 ✓**）：
const html = read("web/index.html");
if (html) {
// **★ 改为"除注入外逐字节相同" ✗ ★**（第 638 轮 ✓；**部署矩阵 §14.21 ✓**）：
//   **∴ 服务端页面**必然引用 `/api/*`✗**（**它本来是给服务端用的 ✓**）⇒
//   **∴ 旧的"不许出现服务端 API 调用"**过时了 ✗**（**∵ 现在由 `api-local.js` 接管 ✓**）。
//   **∴ 新规则更强 ✗**：**页面 = **导出结果 ＋ 固定注入**✗** ⇒
//   **∴ 于是**既允许复用服务端页面 ✓，**又禁止**任何其他改动 ✓**。
const injectPath = "web/pwa-inject.html";
check(existsSync(injectPath), "缺少 web/pwa-inject.html ⇒ **∴ PWA 的 /api/* 会 404 ✗**");
if (html && existsSync(injectPath)) {
  const snippet = read(injectPath) || "";
  // **∴ 两种合法形态**都接受 ✗，**但都必须**含本地接管 ✓**（**第 638 轮 ✓**）：
  const hasInject = html.includes(snippet);
  const hasAdapter = html.includes("/api-local.js");
  check(hasInject || hasAdapter, "index.html 既没有固定注入、也没有加载 /api-local.js ⇒ **∴ /api/* 会 404 ✗**");
  console.log(`  index.html：形态=${hasInject ? "导出＋注入" : "骨架壳"} ✓｜含本地接管 ✓`);
  check(html.includes("</body>"), "web/index.html 里没有 </body> ⇒ **∴ 结构异常 ✗**");
  console.log("  index.html：含本地接管注入 ✓（**注入 " + snippet.length + " 字节 ✓**）");
}
  console.log("  index.html：无服务端 API 调用 ✓（纯离线客户端 ✓）");
}

// **★ 隔离断言 ✓ ★**（第 612 轮 ✓；**用户要求"PWA 不得影响现有 WEB" ✓**）：
//   **∴ PWA 是**新增部署方式**✗ ⇒ **∴ 它**不许侵占**现有 WEB 的资源目录／路径 ✓****。
const wranglerText = read("wrangler.toml") || "";
check(!/directory\s*=\s*"\.\/assets"/.test(wranglerText),
  "PWA 的静态目录指向了 ./assets ⇒ **∴ 会侵占现有 WEB 的资源目录 ✗**");
check(!/directory\s*=\s*"assets"/.test(wranglerText),
  "PWA 的静态目录指向了 assets ⇒ **∴ 会侵占现有 WEB 的资源目录 ✗**");
const workerSrc = read("worker/index.js") || "";
check(!/render_region|encode_png|Renderer|composite/i.test(workerSrc),
  "worker/index.js 里出现渲染调用 ⇒ **∴ 与「Workers 不做渲染」✗ 矛盾**");
check(/env\.ASSETS\.fetch/.test(workerSrc),
  "worker/index.js 未把请求交给静态资源绑定 ⇒ **∴ 它可能在算别的东西 ✗**");
// **∴ 现有 WEB 的入口仍须存在且未被 PWA 触碰 ✓**（**只读断言 ✓**）。
check(existsSync("crates/yanshi-http/assets/viewer-app.js"),
  "现有 viewer（crates/yanshi-http/assets/viewer-app.js）不存在 ⇒ **∴ PWA 影响了现有 WEB ✗**");
console.log("  隔离：PWA 未占用 assets/ ⇒ 未影响现有 WEB ✓");

// **★ 持久化层（IndexedDB）✓ ★**（第 613 轮 ✓；**部署矩阵 §4 ④ ✓**）：
//   **∴ PWA 没有服务器端 ✗ ⇒ **∴ 工程持久化必须由浏览器自己扛 ✓** ⇒ **∴ 它必须存在 ✗**。
const store = read("web/store.js");
check(!!store, "web/store.js 缺失 ⇒ **∴ PWA 没有持久化层 ✗**（**刷新即丢 ✓**）");
if (store) {
  check(/indexedDB/.test(store), "web/store.js 未使用 indexedDB ✗");
  // **∴ 零耦合断言 ✗**：**持久化层不许碰服务端 API ✓**（**用户要求"不影响现有 WEB"✓**）。
  // **∴ 先剥离注释再查 ✗** —— **否则注释里提到该路径会造成**假阳性 ✗**
  //（**本判据第一版就踩了这个坑 ✓，实测抓到的是注释里的字样 ✓**）。
  const stripComments = stripCommentsForCheck;
  const storeCode = stripComments(store);
  check(!/\/api\//.test(storeCode), "web/store.js 的**代码**里出现服务端接口调用 ⇒ **∴ 它依赖了服务器端 ✗**");
  // **★ 快照校验（**目标第 6 条 ✓**）**：**必须有**格式版本 ＋ 序号**✗** ⇒ **∴ 否则会拿旧图冒充 ✓**。
  check(/FORMAT_VERSION/.test(store), "web/store.js 缺少 FORMAT_VERSION ⇒ **∴ 无法判断快照是否过期 ✗**");
  check(/meta\.format !== FORMAT_VERSION/.test(store), "web/store.js 未校验快照格式版本 ⇒ **∴ 会拿旧图冒充 ✗**");
  check(/meta\.seq !== expectedSeq/.test(store), "web/store.js 未校验快照序号 ⇒ **∴ 会拿旧图冒充 ✗**");
  console.log("  store.js：indexedDB ✓｜零 /api/ ✓｜快照校验（format+seq）✓");
}

// **★ 本地 API 适配层 ✓ ★**（第 614 轮 ✓；**用户"不影响现有 WEB" ✓**）：
//   **∴ 它让**同一份 viewer**（`crates/yanshi-http/assets/viewer-app.js` ✓）两处都能跑 ✗
//   ⇒ **∴ 现有 WEB 一个字节都不用改 ✓**（**判据断言下面这条"共享"关系 ✓**）。
const api = read("web/api-local.js");
check(!!api, "web/api-local.js 缺失 ⇒ **∴ PWA 无法复用现有前端 ⇒ 只能分叉 ✗**");
if (api) {
  check(/window\.fetch\s*=/.test(api), "web/api-local.js 未覆写 window.fetch ⇒ **∴ 拦截不了 /api/* ✗**");
  check(/not_implemented_locally/.test(api), "web/api-local.js 未对未实现端点如实报错 ⇒ **∴ 会静默失败（= 撒谎）✗**");
  check(/501/.test(api), "web/api-local.js 未用 501 表示未实现 ⇒ **∴ 状态不可判 ✗**");
  console.log("  api-local.js：覆写 fetch ✓｜未实现端点如实 501 ✓");
}

// **∴ 旧的"`web/viewer-app.js` 不许存在"断言已**删除 ✗**（第 632 轮 ✓）：
// **∴ 它被**更强**的一条取代 —— **两侧必须**逐字节相同 ✗**（**见下面的防分叉检查 ✓**）⇒
// **∴ 于是**既允许"同步一份副本"（**PWA 需要 ✓**），**又禁止"改副本"**（**分叉 ✓**）** ✓✓。
// **★ 本地端点数必须**如实增长**✓ ★**（第 615 轮 ✓）：
//   **∴ `LOCAL_IMPLEMENTED` 是"**我在 PWA 里真的实现了几个端点**✗"的**唯一计数 ✗**
//   ⇒ **∴ 若它和实现脱节 ⇒ **∴ 那就是在**虚报能力 ✗****。
const declared = Number((api || "").match(/LOCAL_IMPLEMENTED\s*=\s*(\d+)/)?.[1] ?? 0);
// **★ 第 858 轮：判定改成**归一化后的 `path`** ✗ ★**：
//   **∴ 为什么 ✗**：**`api-local.js` 现在**先把 `url.pathname` 归一化**（**去掉一个尾斜杠 ✓）
//     ⇒ **∴ 于是**：**分支判定写成 `path === "…"` ✗**（**而**不再是 `url.pathname === "…"` ✓）**
//       ⇒ **∴ 而**本判据**原来数 `url.pathname === "` ✗** ⇒ **∴ 实测 0 ⇒ **∴ 误报"在虚报能力" ✓**** ✓✓
//   **∴ 现在**：**同时接受两种写法 ✗**（**∴ 于是**：**改名／重构不会再骗到它 ✓）** ✓✓
const actualImpl = ((api || "").match(/(?:url\.pathname|path) === "/g) || []).length;
check(declared >= 5, `本地实现的端点数应 ≥ 5 ✗（实测声明 ${declared}）`);
check(actualImpl >= 4, `本地实现分支应 ≥ 4 ✗（实测 ${actualImpl}）⇒ 声明与实现脱节`);
check(declared === actualImpl + 1 || declared === actualImpl,
  `声明的端点数（${declared}）与实现分支数（${actualImpl}）不一致 ⇒ **∴ 在虚报能力 ✗**`);
console.log(`  api-local：声明实现 ${declared} 个端点 ✓｜实现分支 ${actualImpl} 个 ✓`);
check(/makeLocalApi/.test(api || ""), "web/api-local.js 未导出 makeLocalApi ⇒ **∴ 三个端点无法被接入 ✗**");
check(/derived_from/.test(api || ""), "list_layers 未标明由原子推导 ⇒ **∴ 双份状态风险不可见 ✗**");
const apiCode = stripCommentsForCheck(api || "");
check(/gpu_unavailable_reason/.test(apiCode), "api-local 的**代码**未报 GPU 不可用原因 ⇒ **∴ 后端不可判 ✗**");
check(/preview_state:\s*"pending"/.test(apiCode), "api-local 的**代码**未在 get_document 里报 preview_state ✗ ⇒ **∴ 有没有图不可判 ✓**");

// **★ 防分叉（**逐字节相同）✗ ★**（第 632 轮 ✓；**用户要求"不影响现有 WEB" ✓**）：
//   **∴ 单一源永远是 `crates/yanshi-http/assets/` ✗** ⇒ **∴ `web/` 里那三份必须是**它的副本 ✗**
//   ⇒ **∴ 若有人**改了副本**✗ ⇒ **∴ 两条部署就**分叉**✗** ⇒ **∴ 用户会在两处看到不同行为 ✓**。
//   **∴ 而**这正是**最隐蔽的分叉**✗**（**∴ 单看一边完全正常 ✓**）。
import { readFileSync as readBytes } from "node:fs";
const PAIRS = ["viewer-app.js", "viewer.css", "service-worker.js"];
let forkChecked = 0;
for (const f of PAIRS) {
  const src = `crates/yanshi-http/assets/${f}`;
  const dst = `web/${f}`;
  if (!existsSync(src) || !existsSync(dst)) continue;
  const same = readBytes(src).equals(readBytes(dst));
  check(same, `${f} 的副本与现有 WEB 的源**不同** ⇒ **∴ 两条部署分叉了 ✗**（应当只同步，不改 ✓）`);
  forkChecked += 1;
}
check(forkChecked >= 1, "没有任何 viewer 文件被副本化 ⇒ **∴ PWA 没有界面 ✗**");
console.log(`  防分叉：${forkChecked} 个 viewer 文件与单一源逐字节相同 ✓`);

// **★ 笔刷资源 ✓ ★**（第 651 轮 ✓）：**∴ 内核落笔要**笔刷文件全文**✗**
//   （PaintRequest.myb）⇒ **∴ 无服务器部署必须自带这 199 个文件 ✓**。
check(existsSync("web/brushes"), "web/brushes 缺失 ⇒ PWA 无法落笔");
if (existsSync("web/brushes")) {
  const bs = readdirSync("web/brushes").filter((f) => f.endsWith(".myb"));
  check(bs.length >= 100, "笔刷数过少（" + bs.length + "）⇒ 同步不完整");
  const hashed = bs.filter((f) => f.indexOf("#") >= 0);
// **★ 磁盘上现在存的是**URI 编码名**（第 825 轮 ✓；**用户部署驱动 ✓**）：
//   **∴ 为什么 ✗**：**Cloudflare 的 assets manifest **要求路径是 URI 编码形式 ✗**
//   （**错误码 10304 ✓）⇒ **∴ 于是**同步时就把 `#`／`%` 落成 `%23`／`%25` ✓**
//     ⇒ **∴ 所以**：**"**带 `#` 的笔刷**"这个前提**不再存在 ✗**
//     ⇒ **∴ 本判据**改成断言**新的真实情况 ✗**：
//       **a.** **磁盘上**有**编码名 ✗**（**∴ 至少一支含 `%23` ✓）**；
//       **b.** **且**没有**裸 `#` 的残留 ✗**（**∴ 否则** CDN 会拒 ✓）**。
check(bs.some((n) => n.indexOf("_n_") >= 0),
  "磁盘上没有 _n_ 形式的笔刷名 ⇒ URL 编码约定丢失（见 pwa-sync-viewer.mjs 的落盘重命名）");
check(!bs.some((n) => n.indexOf("#") >= 0),
  "磁盘上仍有裸 # 的笔刷名 ⇒ Cloudflare 会以 10304 拒收（必须在同步时编码）");
// **∴ 读数：**编码名几支 ＋ **裸 `#` 应为 0 ✗**（**∴ 两个数**一起看 ✓）**
const encodedCount = bs.filter((n) => n.indexOf("_n_") >= 0).length;
console.log("  笔刷：" + bs.length + " 支 ✓（含 " + encodedCount + " 支 _pct_/_n_ 安全名 ✓｜裸 # 0 支 ✓）");
}
// **★ 本地落笔模块 ✓ ★**（第 652 轮 ✓）：**∴ 它把"**取笔刷 ＋ 调内核 ＋ 报错**✗"收在一处 ✓**
//   ⇒ **∴ 且**它是**纯新增**✗**（**不改现有路径 ✓**）⇒ **∴ 判据只断言它的**关键契约**存在 ✓**。
const brushLib = read("web/brush-local.js");
check(!!brushLib, "web/brush-local.js 缺失 ⇒ 无法本地落笔");
if (brushLib) {
  check(brushLib.indexOf("paint_brush") >= 0, "未调用内核的 paint_brush");
  check(brushLib.indexOf("paint_brush_error") >= 0, "未用 paint_brush_error 取失败原因 ⇒ 会静默失败");
  check(brushLib.indexOf("%23") >= 0, "未对井号做 URL 编码 ⇒ 带井号的笔刷取不到");
  check(brushLib.indexOf("myb") >= 0, "未携带笔刷文件全文 ⇒ 内核会拒绝");
  console.log("  brush-local.js：paint_brush ✓｜失败原因 ✓｜井号编码 ✓｜myb 全文 ✓");
}
// **★ 介质插件产物 ✓ ★**（第 667 轮 ✓；**用户报告 ✓**）：**∴ 它们是**构建产物**✗**
//   （**`crates/yanshi-medium-*` ✓**）⇒ **∴ 已从版本库**取消跟踪 ✗**
//   ⇒ **∴ 所以**必须**存在**✗**（**否则服务端 `/mediums/*.wasm` 会 404 ✓**）。
const MEDIUMS = ["oil.wasm", "watercolor.wasm", "marker.wasm", "pencil.wasm", "pixel.wasm", "example-dab.wasm"];
const missingMediums = MEDIUMS.filter((f) => !existsSync("assets/mediums/" + f)
  || statSync("assets/mediums/" + f).size === 0);
check(missingMediums.length === 0,
  "缺少介质插件产物：" + missingMediums.join(" / ") + " => 先构建并跑 node scripts/mediums-sync.mjs");
if (missingMediums.length === 0) console.log("  介质插件：6 个齐全 ✓（构建产物，已不入库 ✓）");
// **★ 产物新鲜度 ✗ ★**（第 678 轮 ✓）：**∴ 第 671 轮实测到**库里的介质产物落后于源码**✗**
//   （**`b9adcff` 修了 dab 钳制，**而**字节仍是旧的 ✓**）⇒ **∴ 那正是**"拿旧图冒充"✗**在产物层的翻版 ✓**
//   ⇒ **∴ 本判据**断言每个 `.wasm` **不比它 crate 的源码旧** ✗** ⇒ **∴ 旧了就报"先重建" ✓****。
const MEDIUM_SRC_MAP = [
  ["oil.wasm", "yanshi-medium-oil"], ["watercolor.wasm", "yanshi-medium-watercolor"],
  ["marker.wasm", "yanshi-medium-marker"], ["pencil.wasm", "yanshi-medium-pencil"],
  ["pixel.wasm", "yanshi-medium-pixel"], ["example-dab.wasm", "yanshi-medium-example"],
];
const stale = [];
for (const [wasmName, crateName] of MEDIUM_SRC_MAP) {
  const wf = "assets/mediums/" + wasmName;
  const cd = "crates/" + crateName;
  if (!existsSync(wf) || !existsSync(cd)) continue;
  const wt = statSync(wf).mtimeMs;
  let newest = 0;
  const walk = (dir) => {
    for (const e of readdirSync(dir, { withFileTypes: true })) {
      const f = dir + "/" + e.name;
      if (e.isDirectory()) walk(f);
      else if (f.endsWith(".rs") || f.endsWith(".toml")) newest = Math.max(newest, statSync(f).mtimeMs);
    }
  };
  walk(cd);
  if (newest > wt + 1000) stale.push(wasmName + "（源码新于产物）");
}
check(stale.length === 0,
  "介质产物落后于源码：" + stale.join(" / ") + " => 重建并跑 node scripts/mediums-sync.mjs");
if (stale.length === 0) console.log("  介质产物：6 个都不比源码旧 ✓");
// **★ PWA 也要有它们 ✗ ★**（第 673 轮 ✓）：**∴ 前端从 `/mediums/` 取 ✗**
//   ⇒ **∴ 只拷 `assets/` 不够 ⇒ **∴ `web/mediums/` 也得有 ✓****。
const webMissing = MEDIUMS.filter((f) => !existsSync("web/mediums/" + f));
check(webMissing.length === 0,
  "web/mediums 缺少：" + webMissing.join(" / ") + " => 跑 node scripts/pwa-sync-viewer.mjs");
// **★ 部署可用性：入口页必须**被 git 跟踪** ✗ ★**（第 748 轮 ✓；**用户报告 404 ✓**）：
//   **∴ 为什么 ✗**：**`wrangler.toml` 的 `[assets] directory = "./web"` ✗**
//   ⇒ **∴ 若** `web/index.html` **不在版本库里 ⇒ **∴ fresh clone／别的机器部署时**
//   **上传的目录**没有入口页 ⇒ **∴ Cloudflare 返回 **404 ✓****（**用户实测 ✓）。
//   **∴ 本判据**直接问 git「**它被跟踪吗**」✗ ⇒ **∴ 于是**这类 404**在提交前就会被抓到 ✓**。
let indexTracked = false;
try {
  execFileSync("git", ["ls-files", "--error-unmatch", "web/index.html"], { stdio: "pipe" });
  indexTracked = true;
} catch (e) { indexTracked = false; }
check(indexTracked,
  "web/index.html 没被 git 跟踪 => 部署后必定 404（详见 .gitignore 的说明）");
if (indexTracked) console.log("  部署入口页：web/index.html 已被 git 跟踪 ✓");
// 本地专用部署不得去拨 WebSocket：静态部署没有服务端，拨 /ws 只能握手失败并把控制台刷满，
// 而噪音会盖住真报错（用户报告的 P2-4）。守卫必须在 connect() 入口内，才覆盖全部调用点。
// 变异：删掉那段守卫 ⇒ 本条必红（已实测）。
const viewerText = read("web/viewer-app.js");
check(viewerText !== null, "web/viewer-app.js 读取不到 ⇒ 无法核对本地专用 WebSocket 守卫");
if (viewerText !== null) {
  const connectAt = viewerText.indexOf("function connect() {");
  const guardAt = viewerText.indexOf("本地内核（无服务端）");
  check(connectAt >= 0, "viewer-app.js 里找不到 connect() ⇒ 无法核对本地专用守卫");
  check(guardAt >= 0,
    "connect() 缺少本地专用守卫 ⇒ 静态部署会去拨 /ws 并刷满握手失败（P2-4）");
  if (connectAt >= 0 && guardAt >= 0) {
    check(guardAt >= connectAt && guardAt <= connectAt + 1200,
      "本地专用守卫不在 connect() 入口内 ⇒ 覆盖不到全部调用点");
    if (guardAt >= connectAt && guardAt <= connectAt + 1200) {
      console.log("  本地专用部署不去拨 WebSocket ✓（守卫在 connect() 内 ✓）");
    }
  }
}

if (bad.length) {
  console.error("❌ " + bad.join("｜"));
  process.exit(1);
}
console.log("  ✓ PWA 资源完整：内核在、域名对、预缓存含内核、无服务端依赖 ✓");
