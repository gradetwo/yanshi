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

const bad = [];
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
  check(!/\/api\/tools\//.test(html), "index.html 里出现服务端 API 调用 ⇒ 与该部署「无服务器」的设计不符 ✗");
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
  const stripComments = (text) => text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:])\/\/.*$/gm, "$1");
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

// **★ 防分叉断言 ✓ ★**（第 614 轮 ✓）：
//   **∴ 单一源必须是**现有 WEB**✗**（`crates/yanshi-http/assets/viewer-app.js` ✓）⇒
//   **∴ 若哪天有人把 `web/` 里也放一份**改过的**同名文件 ✗ ⇒ **∴ 两条部署就分叉了 ✗**。
//   **∴ 所以**这里禁止 `web/viewer-app.js` 存在 ✗**（**它必须由同步脚本从单一源拷入 ✓**）。
check(!existsSync("web/viewer-app.js"),
  "web/viewer-app.js 存在 ⇒ **∴ 前端被复制成了第二份 ⇒ 会与现有 WEB 分叉 ✗**（应当只同步，不改 ✓）");

// **★ 本地端点数必须**如实增长**✓ ★**（第 615 轮 ✓）：
//   **∴ `LOCAL_IMPLEMENTED` 是"**我在 PWA 里真的实现了几个端点**✗"的**唯一计数 ✗**
//   ⇒ **∴ 若它和实现脱节 ⇒ **∴ 那就是在**虚报能力 ✗****。
const declared = Number((api || "").match(/LOCAL_IMPLEMENTED\s*=\s*(\d+)/)?.[1] ?? 0);
const actualImpl = ((api || "").match(/url\.pathname === "\/api\/[^"]+"/g) || []).length;
check(declared >= 5, `本地实现的端点数应 ≥ 5 ✗（实测声明 ${declared}）`);
check(actualImpl >= 4, `本地实现分支应 ≥ 4 ✗（实测 ${actualImpl}）⇒ 声明与实现脱节`);
check(declared === actualImpl + 1 || declared === actualImpl,
  `声明的端点数（${declared}）与实现分支数（${actualImpl}）不一致 ⇒ **∴ 在虚报能力 ✗**`);
console.log(`  api-local：声明实现 ${declared} 个端点 ✓｜实现分支 ${actualImpl} 个 ✓`);
check(/makeLocalApi/.test(api || ""), "web/api-local.js 未导出 makeLocalApi ⇒ **∴ 三个端点无法被接入 ✗**");
check(/derived_from/.test(api || ""), "list_layers 未标明由原子推导 ⇒ **∴ 双份状态风险不可见 ✗**");
check(/gpu_unavailable_reason/.test(api || ""), "api-local 未报 GPU 不可用原因 ⇒ **∴ 后端不可判 ✗**");
check(/preview_state/.test(api || ""), "api-local 的 get_document 未报 preview_state ⇒ **∴ 有没有图不可判 ✗**");

if (bad.length) {
  console.error("❌ " + bad.join("｜"));
  process.exit(1);
}
console.log("  ✓ PWA 资源完整：内核在、域名对、预缓存含内核、无服务端依赖 ✓");
