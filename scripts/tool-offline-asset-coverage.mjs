#!/usr/bin/env node
// **离线资产覆盖判据**（目标 (A)② 的"六类资产"那一半 ✓）。
//
// **为什么需要它** ✗：`assets/` 下共 **8 类**（brand / brushes / brush-previews / fonts /
// mediums / palettes / samples / textures ✓），而 SW 的 `SHELL` 预缓存只列了 207 条
// （`/`、`/favicon.svg`、一张品牌图、`/brush-previews/index.json`、`/viewer.css`、
// `/viewer-app.js`、两件内核、199 支笔刷 ✓）—— 其余**一类都没进** ✗。
// 更糟的是：普通静态资产**从不写进任何缓存** ✗（fetch 分支只在 `request.mode === "navigate"`
// 时 `cache.put("/", …)` ✓）⇒ 断网时 `caches.match(request)` 与 `caches.match("/")`
// 两条回落**都不可能命中** ✗ ⇒ 纹理 / 预览图 / 示例 / 介质插件离线都拿不到 ✓。
//
// **本判据怎么量** ✓（不是读字符串猜 ✓ —— 它**真的跑 SW 的 fetch 处理器** ✓）：
//   用 `node:vm` 把 `crates/yanshi-http/assets/service-worker.js` 装进一个沙箱 ✓，
//   配上**内存版 Cache Storage** 与一个可切"在线/断网"的 `fetch` ✓，然后：
//     ① 跑 `install` ＋ `activate`（预缓存真的发生 ✓，换构建真的清旧缓存 ✓）；
//     ② 对**每个种类的代表文件**：先在线取一次（模拟"用到了"✓），再断网重取一次 ✓
//        ⇒ 断言拿到的是**那个文件自己的字节** ✓ —— 今天断网会落到外壳 `/` 的 HTML ✗
//        （字节不同 ⇒ 当场红 ✓），或直接抛网络错 ✓；
//     ③ `SHELL` 里已有的（如笔刷 ✓）**不先在线取**也要能离线拿到 ✓（预缓存那条路 ✓）；
//     ④ `/api/` 与 `/ws` 必须**不被接管** ✓（可变数据不能走缓存 ✓，见 `browser-no-stale-read` ✓）；
//     ⑤ `/service-worker.js` 在线取过后**不得进缓存** ✓（否则旧 SW 会被钉住 ✓ —— (A)⑥）；
//     ⑥ 换构建（`__BUILD_ID__` 变了 ✓）⇒ `activate` 必须把旧缓存整份删掉 ✓，
//        且旧构建里缓存的资产在新构建**不得**被离线读到 ✓（不脏读 ✓）。
//
// **四种离线机制** ✓（每个种类按自己那条 ✓）：
//   · `precache`     ：URL 在 `SHELL` 里 ⇒ install 时就进缓存 ✓（笔刷就是这种 ✓）；
//   · `runtime`      ：URL 可路由 ✓ ＋ 静态分支对**非导航**请求也写缓存 ✓（"用到就存"✓）；
//   · `embedded`     ：字节 `include_bytes!` 进**已预缓存的内核 wasm** ✓（字体就是这种 ✓）；
//   · `viewer-cache` ：字节经**只读工具**下发 ✓ ＋ 查看器把只读工具响应写本地缓存 ✓
//                      （调色板没有静态路由 ✓ ⇒ 走这条 ✓）。
//
// **它不证明什么** ✗：沙箱是**行为等价**而不是同一个浏览器 ✓ —— 它证明 SW 的缓存策略正确 ✓，
// 但**不**证明真实 Chromium 的 Cache Storage 实现 / PWA 安装 / 配额行为 ✓；
// 那一段由 `scripts/browser-offline-assets.mjs` 在真浏览器里端到端证明 ✓。
//
// 用法：node scripts/tool-offline-asset-coverage.mjs
//      （不消费 `<base-url>` ✓ —— 只为与 `tool-*.mjs` 的调用约定一致 ✓）
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { createContext, runInNewContext } from "node:vm";

const SW_PATH = "crates/yanshi-http/assets/service-worker.js";
const SERVER_RS = "crates/yanshi-http/src/server.rs";
const TOOLS_RS = "crates/yanshi-server/src/tools.rs";
const VIEWER_RS = "crates/yanshi-http/src/viewer.rs";
const VIEWER_APP = "crates/yanshi-http/assets/viewer-app.js";
const FONT_ATLAS = "crates/yanshi-render/src/font_atlas.rs";
const ASSETS_DIR = "assets";
const ORIGIN = "http://127.0.0.1:13990";

const failures = [];
const notes = [];
const fail = (message) => failures.push(message);

const read = (path) => {
  try {
    return readFileSync(path, "utf8");
  } catch (error) {
    fail(`读不到 ${path}（${error.message}）⇒ 判据无法运行`);
    return "";
  }
};

const sw = read(SW_PATH);
const server = read(SERVER_RS);
const tools = read(TOOLS_RS);
const viewerRs = read(VIEWER_RS);
const viewerApp = read(VIEWER_APP);
const fontAtlas = read(FONT_ATLAS);
if (failures.length) {
  console.log(`  ✗ ${failures.join("；")}`);
  process.exit(1);
}

// ── ① 解析 `SHELL` 预缓存清单 ───────────────────────────────────────────────
const shellStart = sw.indexOf("const SHELL = [");
const shellEnd = shellStart >= 0 ? sw.indexOf("];", shellStart) : -1;
if (shellStart < 0 || shellEnd < 0) {
  console.log("  ✗ 在 service-worker.js 里找不到 `const SHELL = [` 清单 ⇒ 判据无法运行");
  process.exit(1);
}
const SHELL = [...sw.slice(shellStart, shellEnd).matchAll(/"([^"]+)"/g)].map((m) => m[1]);
const shellSet = new Set(SHELL);
const duplicate = SHELL.find((url, index) => SHELL.indexOf(url) !== index);
if (duplicate) fail(`SHELL 里有重复条目：${duplicate} ⇒ 清单在腐化`);
if (shellSet.has("/service-worker.js")) fail("SHELL 里出现了 `/service-worker.js` ⇒ 缓存它可能把旧版本钉住（(A)⑥）");
// **`#` 会被 URL 当片段丢掉** ✗ ⇒ `cache.add("/brushes/Fan#1.myb")` 的键其实是 `/brushes/Fan` ✓。
// 这是**已登记的独立缺口** ✓（不在本次六类资产里 ✓）：要修得同时改查看器的 URL 编码与服务端的
// 路径解码 ✓ ⇒ 本判据只**如实计数并打印** ✓，不把它混进本轮的判定 ✓。
const fragmentEntries = SHELL.filter((url) => url.includes("#"));
if (fragmentEntries.length) {
  notes.push(
    `SHELL 里有 ${fragmentEntries.length} 条含 \`#\` 的键（例：\`${fragmentEntries[0]}\`）⇒ ` +
      "浏览器把 `#` 之后当片段丢掉 ⇒ 这些预缓存的键与真实请求对不上（**已登记的独立缺口**，需同时改查看器编码与服务端解码）",
  );
}

// ── ② 解析路由白名单 / 只读工具 ─────────────────────────────────────────────
const arrayItems = (source, decl) => {
  const start = source.indexOf(decl);
  if (start < 0) return null;
  const end = source.indexOf("];", start);
  return end < 0 ? null : source.slice(start, end);
};
const brandBlock = arrayItems(server, "const BRAND_FILES:");
const brandFiles = brandBlock ? [...brandBlock.matchAll(/\("([^"]+)",\s*"[^"]*"\)/g)].map((m) => m[1]) : null;
const sampleBlock = arrayItems(server, "const SAMPLE_FILES:");
const sampleFiles = sampleBlock ? [...sampleBlock.matchAll(/"([^"]+)"/g)].map((m) => m[1]) : null;
if (!brandFiles || brandFiles.length === 0) fail("解析不出 `BRAND_FILES` 白名单（或它是空的）⇒ 品牌资源发不出来");
if (!sampleFiles || sampleFiles.length === 0) fail("解析不出 `SAMPLE_FILES` 白名单（或它是空的）⇒ 示例画面发不出来");

const routeDispatched = (prefix) => server.includes(`strip_prefix("${prefix}")`);
const toolIsReadOnly = (name) => {
  const at = tools.indexOf(`name: "${name}"`);
  if (at < 0) return false;
  return /mutating:\s*false/.test(tools.slice(at, at + 600));
};

// ── ③ 目录工具 ─────────────────────────────────────────────────────────────
const walk = (dir, base) => {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...walk(path, base));
    else if (entry.isFile()) out.push(path.slice(base.length + 1));
  }
  return out;
};
const kindsOnDisk = readdirSync(ASSETS_DIR, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map((entry) => entry.name)
  .sort();
// **种类内的相对路径** ✓（`brand/svg/icon-dark.svg` ✓ —— 白名单就是按相对路径写的 ✓）。
const filesOf = (kind) => walk(join(ASSETS_DIR, kind), join(ASSETS_DIR, kind)).sort();
const bytesOfFile = (kind, file) => {
  try {
    return statSync(join(ASSETS_DIR, kind, file)).size;
  } catch (error) {
    return 0;
  }
};

// ── ④ 每个种类的离线机制表 ─────────────────────────────────────────────────
const KINDS = {
  brand: {
    mechanism: "runtime",
    route: "/brand/",
    served: (file) => (brandFiles || []).includes(file),
    url: (file) => "/brand/" + file,
  },
  brushes: {
    mechanism: "runtime", // 199 支已在 SHELL 里 ✓；其余靠"用到就存"兜底 ✓。
    route: "/brushes/",
    served: (file) => file.endsWith(".myb") && !file.includes("/"),
    url: (file) => "/brushes/" + file,
  },
  "brush-previews": {
    mechanism: "runtime",
    route: "/brush-previews/",
    served: (file) => !file.includes("/") && /\.(png|svg|json)$/.test(file),
    url: (file) => "/brush-previews/" + file,
  },
  fonts: {
    // **字体没有路由** ✓ —— 它经 `include_bytes!` 编进内核 ✓，而内核在 SHELL 里 ✓。
    mechanism: "embedded",
    route: null,
    served: (file) => file.endsWith(".bin"),
    url: () => null,
  },
  mediums: {
    mechanism: "runtime",
    route: "/mediums/",
    served: (file) => !file.includes("/") && file.endsWith(".wasm"),
    url: (file) => "/mediums/" + file,
  },
  palettes: {
    // **调色板没有静态路由** ✗ ⇒ 字节只经只读工具 `list_palette_colors` 下发 ✓，
    // 查看器把**只读工具**的响应写本地缓存（IndexedDB ✓）⇒ 用过一次就离线可用 ✓。
    mechanism: "viewer-cache",
    route: null,
    served: (file) => !file.includes("/") && /\.(gpl|json)$/.test(file),
    url: () => null,
  },
  samples: {
    mechanism: "runtime",
    route: "/samples/",
    served: (file) => (sampleFiles || []).includes(file),
    url: (file) => "/samples/" + file,
  },
  textures: {
    mechanism: "runtime",
    route: "/textures/",
    served: (file) => !file.includes("/") && file.endsWith(".png"),
    url: (file) => "/textures/" + file,
  },
};

// ── ⑤ SW 沙箱：真的跑 fetch 处理器 ──────────────────────────────────────────
/// 内存版 Cache Storage ＋ 可切在线/断网的 `fetch` ✓ —— 行为与浏览器那三条分支一一对应 ✓。
function makeServiceWorker(source, buildId, sharedStore = new Map()) {
  const handlers = new Map();
  let offline = false;
  // **每个 URL 一个可辨认的字节串** ✓ ⇒ 外壳回落（`/` 的正文）与资产正文**必然不同** ✓
  // ⇒ "断网拿到的是不是**它自己**"这件事可以被逐字节判 ✓。
  const onlineBody = (path) => "YANSHI-ASSET-BODY " + path;
  const cacheKey = (request, ignoreSearch) => {
    const url = new URL(typeof request === "string" ? request : request.url, ORIGIN);
    return url.pathname + (ignoreSearch ? "" : url.search);
  };
  const networkFetch = async (request) => {
    if (offline) throw new TypeError("Failed to fetch");
    const key = cacheKey(request, false);
    return new Response(onlineBody(key), { status: 200, headers: { "content-type": "application/octet-stream" } });
  };
  class MemoryCache {
    constructor() {
      this.entries = new Map();
    }
    async add(request) {
      const response = await networkFetch(request);
      this.entries.set(cacheKey(request, false), response);
    }
    async put(request, response) {
      this.entries.set(cacheKey(request, false), response);
    }
    async match(request, options) {
      const key = cacheKey(request, !!(options && options.ignoreSearch));
      if (this.entries.has(key)) return this.entries.get(key).clone();
      if (options && options.ignoreSearch) {
        for (const [candidate, response] of this.entries) {
          if (candidate.split("?")[0] === key) return response.clone();
        }
      }
      return undefined;
    }
    async keys() {
      return [...this.entries.keys()].map((key) => ({ url: ORIGIN + key }));
    }
  }
  const caches = {
    async open(name) {
      if (!sharedStore.has(name)) sharedStore.set(name, new MemoryCache());
      return sharedStore.get(name);
    },
    async keys() {
      return [...sharedStore.keys()];
    },
    async delete(name) {
      return sharedStore.delete(name);
    },
    async match(request, options) {
      for (const cache of sharedStore.values()) {
        const hit = await cache.match(request, options);
        if (hit) return hit;
      }
      return undefined;
    },
  };
  const sandbox = { URL, Request, Response, Headers, TextEncoder, caches, fetch: networkFetch, console };
  sandbox.self = sandbox;
  sandbox.location = { origin: ORIGIN };
  sandbox.addEventListener = (type, handler) => handlers.set(type, handler);
  sandbox.skipWaiting = async () => undefined;
  sandbox.clients = { claim: async () => undefined };
  createContext(sandbox);
  runInNewContext(source.replaceAll("__BUILD_ID__", buildId), sandbox);

  const lifecycle = async (type) => {
    const tasks = [];
    handlers.get(type)({ waitUntil: (promise) => tasks.push(promise) });
    await Promise.all(tasks);
  };
  // **派发一次同源 GET** ✓（`mode` 不给 ⇒ 非导航 ✓，正是普通静态资产那种 ✓）。
  const dispatch = async (path) => {
    const request = new Request(ORIGIN + path);
    let captured = null;
    handlers.get("fetch")({ request, respondWith: (promise) => { captured = promise; }, waitUntil: () => undefined });
    if (!captured) return { intercepted: false, response: null };
    return { intercepted: true, response: await captured };
  };
  return {
    install: () => lifecycle("install"),
    activate: () => lifecycle("activate"),
    dispatch,
    setOffline: (value) => { offline = value; },
    cacheNames: () => [...sharedStore.keys()],
    allCachedUrls: () => {
      const urls = new Set();
      for (const cache of sharedStore.values()) for (const key of cache.entries.keys()) urls.add(key);
      return urls;
    },
    store: sharedStore,
  };
}

const SW_TEMPLATE = sw;
const swFor = (buildId, store) => makeServiceWorker(SW_TEMPLATE, buildId, store);

// ── ⑥ 代表文件的离线判定（先在线取一次 ⇒ 再断网取一次） ─────────────────────
const rows = [];
const coverageFor = async (worker, kind, representative, url) => {
  // `install` ＋ `activate` 已经在外面跑过 ✓ ⇒ SHELL 里的东西此刻**已经**在缓存里 ✓。
  const key = new URL(url, ORIGIN).pathname + new URL(url, ORIGIN).search;
  const expected = "YANSHI-ASSET-BODY " + key;
  worker.setOffline(false);
  const online = await worker.dispatch(url);
  if (!online.intercepted) return { ok: false, why: "请求没有被 SW 接管（落进了 `/api/` 或 `/ws` 那条早退？）" };
  worker.setOffline(true);
  const offline = await worker.dispatch(url);
  worker.setOffline(false);
  if (!offline.intercepted) return { ok: false, why: "断网后请求没有被 SW 接管" };
  let text = "";
  try {
    text = await offline.response.text();
  } catch (error) {
    text = `<读不出正文：${error.message}>`;
  }
  if (text === expected) return { ok: true, why: "" };
  const where = text === "YANSHI-ASSET-BODY /" ? "落到了外壳 `/`（HTML）" : "拿到的不是它自己的字节";
  return { ok: false, why: `断网后 ${where}` };
};

// ── ⑦ 主流程 ───────────────────────────────────────────────────────────────
const store = new Map();
const worker = swFor("buildA", store);
await worker.install();
await worker.activate();

// ① `/api/`（blob 除外）与 `/ws` 必须不被接管（可变数据不走缓存 ✓）。
//    **`/api/blob/` 是例外** ✓：它是**按内容哈希命名**的不可变资源 ✓ ⇒ SW 故意 cache-first ✓
//    （`<img src>` 也走它 ✓，见 SW 里那段注释 ✓）⇒ 这里对它做**正向**断言 ✓。
for (const path of ["/api/tools/get_document", "/api/atoms", "/ws"]) {
  const probe = await worker.dispatch(path);
  if (probe.intercepted) fail(`\`${path}\` 被 SW 接管了 ⇒ 可变数据可能被缓存 ⇒ 脏读风险（见 browser-no-stale-read）`);
}
const blobProbe = await worker.dispatch("/api/blob/abc123");
if (!blobProbe.intercepted) fail("/api/blob/ 不再被 SW 接管 ⇒ 内容寻址的 blob 离线拿不到（`<img src>` 那条路失守）");
// ② `/service-worker.js` 在线取过后不得进缓存（否则旧 SW 被钉住 ✓）。
await worker.dispatch("/service-worker.js");
if (worker.allCachedUrls().has("/service-worker.js")) {
  fail("`/service-worker.js` 被写进了缓存 ⇒ 浏览器检查更新时可能拿到旧脚本 ⇒ (A)⑥ 的升级机制失效");
}
// ③ 构建标识必须进缓存名。
if (!worker.cacheNames().includes("yanshi-shell-buildA")) {
  fail(`SW 没有用带构建标识的缓存名（现有：${worker.cacheNames().join(", ")}）⇒ 换构建不会作废旧资产`);
}
const initialCacheNames = worker.cacheNames();

// ④ 逐种类核对（拉上机制表的兜底 ✓）。
for (const kind of kindsOnDisk) {
  const spec = KINDS[kind];
  if (!spec) {
    fail(`assets/${kind} 是一个**没有登记离线机制**的种类 ⇒ 判据不知道它怎么离线可用`);
    continue;
  }
  const files = filesOf(kind).filter(spec.served);
  if (files.length === 0) {
    fail(`assets/${kind} 里一个可下发的文件都没有（白名单/扩展名对不上？）⇒ 该种类没有离线路径`);
    continue;
  }
  if (spec.route && !routeDispatched(spec.route)) {
    fail(`assets/${kind} 声明的路由 ${spec.route} 在 server.rs 里没有分发 ⇒ 浏览器根本取不到`);
  }
  // **代表文件优先挑"不在 SHELL 里的"** ✓ —— 否则会被"预缓存那几件"遮住 ✓。
  // 同时避开含 `#` 的名字 ✓（那是已登记的独立缺口 ✓，见上面的 note ✓）。
  const urlOf = (file) => spec.url(file);
  const candidates = files.filter((file) => {
    const url = urlOf(file);
    return !url || (!shellSet.has(url) && !url.includes("#"));
  });
  const representative = candidates[0] || files.find((file) => (urlOf(file) || "").indexOf("#") < 0) || files[0];
  const url = urlOf(representative);
  if (url) {
    const result = await coverageFor(worker, kind, representative, url);
    rows.push({ kind, representative: url, mechanism: shellSet.has(url) ? "precache" : spec.mechanism, ready: result.ok });
    if (!result.ok) {
      fail(
        `assets/${kind} 的代表文件 \`${representative}\`（URL \`${url}\`）**离线拿不到** ✗ —— ` +
          `既不在 SHELL 里，兜底机制 \`${spec.mechanism}\` 也没接线：${result.why}`,
      );
    }
  } else {
    // 没有静态路由的种类（字体 / 调色板 ✓）：机制必须真的接线 ✓。
    rows.push({ kind, representative: `assets/${kind}/${representative}`, mechanism: spec.mechanism, ready: false });
  }
}

// ⑤ `embedded` / `viewer-cache` 两条机制的接线检查。
const mechanismReady = {
  embedded: () => {
    const included = /include_bytes!\([^)]*assets\/fonts\/yanshi-bitmap-16\.bin/.test(fontAtlas);
    const kernelPrecached = shellSet.has("/wasm/yanshi_wasm.js") && shellSet.has("/wasm/yanshi_wasm_bg.wasm");
    if (!included) fail("字体图集没有 `include_bytes!` 进内核 ⇒ 它既没路由也没内嵌 ⇒ 离线拿不到");
    if (!kernelPrecached) fail("内核两件没有同时在 SHELL 里 ⇒ 内嵌的字体也离线拿不到");
    return included && kernelPrecached;
  },
  "viewer-cache": () => {
    const readOnly = toolIsReadOnly("list_palette_colors") && toolIsReadOnly("list_assets");
    const readsGenerated = viewerRs.includes("!spec.mutating") && viewerRs.includes('"__READ_TOOLS__"');
    const viewerCaches =
      viewerApp.includes("const LOCAL_READ_TOOLS = [__READ_TOOLS__];") &&
      viewerApp.includes("LOCAL_READ_TOOLS.includes(toolName)") &&
      viewerApp.includes("localJsonPut") &&
      viewerApp.includes("localJsonGet");
    if (!readOnly) fail("`list_palette_colors` / `list_assets` 不再是只读工具 ⇒ 查看器不会缓存它们 ⇒ 调色板离线拿不到");
    if (!readsGenerated) fail("查看器的 `__READ_TOOLS__` 不再由 `mutating: false` 生成 ⇒ 只读清单可能漂移");
    if (!viewerCaches) fail("查看器不再把只读工具的响应写本地缓存 / 不再回落 ⇒ 调色板离线拿不到");
    return readOnly && readsGenerated && viewerCaches;
  },
};
for (const row of rows) {
  if (row.mechanism === "precache") {
    row.ready = row.ready || shellSet.has(row.representative);
  } else if (row.mechanism === "embedded" || row.mechanism === "viewer-cache") {
    row.ready = mechanismReady[row.mechanism]();
  }
}
for (const kind of Object.keys(KINDS)) {
  if (!kindsOnDisk.includes(kind)) fail(`机制表里登记了 assets/${kind}，但磁盘上已经没有这个目录 ⇒ 表在腐化`);
}

// ⑥ 换构建 ⇒ 旧缓存整份作废、旧资产不得被新构建离线读到（不脏读 ✓）。
// **先用 buildA 把一个纹理"用到就存"** ✓（它不在 SHELL 里 ✓）。
const carried = "/textures/Paper001.png";
worker.setOffline(false);
await worker.dispatch(carried);
const carriedBody = "YANSHI-ASSET-BODY " + carried;
worker.setOffline(true);
const beforeUpgrade = await worker.dispatch(carried);
worker.setOffline(false);
const beforeText = beforeUpgrade.intercepted ? await beforeUpgrade.response.text() : "";
if (beforeText !== carriedBody) fail(`升级前 ` + `${carried} 就没能离线读到 ⇒ 运行时缓存这条根本没接上`);

const upgraded = swFor("buildB", store);
await upgraded.install();
await upgraded.activate();
if (upgraded.cacheNames().includes("yanshi-shell-buildA")) {
  fail("换构建后旧缓存 `yanshi-shell-buildA` 还在 ⇒ 旧资产会跨版本留下来（脏读）");
}
upgraded.setOffline(true);
const afterUpgrade = await upgraded.dispatch(carried);
const afterText = afterUpgrade.intercepted ? await afterUpgrade.response.text() : "";
upgraded.setOffline(false);
if (afterText === carriedBody) {
  fail(`换构建后**仍然**离线读到了旧构建缓存的 ${carried} ⇒ 脏读 ✓（(A)⑥ 在资产上的那一半不成立）`);
}

// ── ⑧ 报告"按需缓存"的体量（不进 install ⇒ 只报告，不设阈值 ✓） ─────────────
// **逐种类实测** ✓（不写死数字 ✗ —— 本会话已四次栽在"写死快照"上 ✓）。
const onDemandByKind = kindsOnDisk
  .filter((kind) => KINDS[kind])
  .map((kind) => {
    const spec = KINDS[kind];
    let bytes = 0;
    for (const file of filesOf(kind)) {
      if (!spec.served(file)) continue;
      const url = spec.url(file);
      // **只统计 SW 会按需缓存的那些** ✓（字体内嵌 ✓、调色板走查看器本地缓存 ✓ ⇒ 都不算 ✓）；
      // 含 `#` 的名字单独登记 ✓（它们是另一个缺口 ✓，不能算进"按需能拿到"✓）。
      if (!url || shellSet.has(url) || url.includes("#")) continue;
      bytes += bytesOfFile(kind, file);
    }
    return { kind, bytes };
  })
  .filter((entry) => entry.bytes > 0);
const total = onDemandByKind.reduce((sum, entry) => sum + entry.bytes, 0);
notes.push(
  `未预缓存、由 SW **按需缓存**的静态资产体量 = ${(total / 1048576).toFixed(1)} MiB（` +
    onDemandByKind.map((entry) => `${entry.kind} ${(entry.bytes / 1048576).toFixed(1)}`).join(" ＋ ") +
    "）—— 不进 install ⇒ 只有真正用到的才落缓存",
);

// ── ⑨ 结论 ─────────────────────────────────────────────────────────────────
console.log(`  · SHELL 预缓存条目 = ${SHELL.length}（含 ${fragmentEntries.length} 条被 '#' 吞掉片段的键 ✓）`);
console.log(`  · 构建 A 的缓存名 = ${initialCacheNames.join(", ")}｜构建 B 的缓存名 = ${upgraded.cacheNames().join(", ")}`);
console.log("  · 各类资产的离线路径（沙箱里真跑 fetch 处理器 ✓）：");
for (const row of rows) {
  console.log(`      ${row.ready ? "✓" : "✗"} ${row.kind.padEnd(15)} ${row.mechanism.padEnd(12)} ${row.representative}`);
}
for (const note of notes) console.log(`  · ${note}`);
if (failures.length) {
  console.log(`  ✗ 离线资产覆盖不成立：${failures.join("；")}`);
  process.exit(1);
}
console.log("  ✓ 离线资产覆盖成立：每一类资产都有已接线的离线路径（预缓存 / 用到就存 / 内嵌 / 查看器本地缓存）");
process.exit(0);
