#!/usr/bin/env node
// **离线资产端到端判据**（目标 (A)② 的六类资产 ✓）—— 在**真 Chromium** 里证明：
// 在线用过一次的纹理 / 预览图 / 示例 / 介质插件 / 品牌图 / 内核，**断网后仍拿得到**，
// 而且拿到的是**它自己的字节**，不是被外壳 `/` 的 HTML 顶包 ✓。
//
// **为什么必须端到端** ✗：`tool-offline-asset-coverage.mjs` 只用内存版 Cache Storage 跑 SW 的
// fetch 处理器 ✓（证明**缓存策略**对 ✓）；本条证明**真浏览器的 Cache Storage / PWA 路径**也成立 ✓。
//
// **两条断言各自能红 ✓**：
//   ① **产品自己的路**：断网重载后，纹理缩略图 / 画笔库预览图 / 调色板色块必须**真的画出来**
//      （`naturalWidth > 0` / 色块数 > 0 ✓）。今天断网时 SW 对 `<img>` 请求回落到**外壳 HTML** ✗
//      ⇒ 图 `naturalWidth === 0` ⇒ **当场红** ✓。
//   ② **字节级**：断网后 `fetch()` 取介质插件（`.wasm` ✓）/ 示例 PNG / 品牌 PNG ⇒ 必须
//      **HTTP 200 ＋ 正确的魔数 ＋ 长度与在线那一趟相同** ✓。今天回落也是 **200** ✗（给的是 HTML ✓）
//      ⇒ 只看 `response.ok` 会被骗过 ✓ ⇒ 所以这里查**魔数**（`\0asm` / `\x89PNG` ✓）。
//
// 前提（与其它 browser-* 一致 ✓）：`<viewer-url> <base> <token> [cdp-port]`，端口读 `CDP_PORT` ✓。
// 用法：CDP_PORT=9490 node scripts/browser-offline-assets.mjs "http://127.0.0.1:13990/?doc=d&token=t"
import { readdirSync, statSync } from "node:fs";

const url = process.argv[2];
const base = process.argv[3];
const port = process.env.CDP_PORT || process.argv[5] || "9333";
if (!url) {
  console.error("用法: node scripts/browser-offline-assets.mjs <viewer-url> <base> <token> [cdpPort]");
  process.exit(2);
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// **代表文件从磁盘上挑** ✓（不写死名字 ✗ —— 免得起重命名后判据变成谎话 ✓）。
const pick = (dir, test) => {
  const names = readdirSync(dir).filter(test).sort();
  if (names.length === 0) {
    console.error(`  ✗ ${dir} 里挑不出代表文件 ⇒ 判据无法运行`);
    process.exit(1);
  }
  return `${dir}/${names[0]}`;
};
const TEXTURE = pick("assets/textures", (name) => name.endsWith(".png"));
const MEDIUM = pick("assets/mediums", (name) => name.endsWith(".wasm"));
const SAMPLE = pick("assets/samples", (name) => name.endsWith(".png") && name.startsWith("sample-"));
const BRAND = "assets/brand/png/yanshi-icon-256.png";
// 浏览器请求的 URL（**去掉 `assets/` 前缀** ✓：路由是 `/textures/…` 而不是 `/assets/textures/…` ✓）。
const URLS = {
  texture: "/" + TEXTURE.replace(/^assets\//, ""),
  medium: "/" + MEDIUM.replace(/^assets\//, ""),
  sample: "/" + SAMPLE.replace(/^assets\//, ""),
  brand: "/" + BRAND.replace(/^assets\//, ""),
};

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((target) => target.type === "page");
if (!page) {
  console.error("没有页面目标 ⇒ 判据无效（不是通过 ✗）");
  process.exit(1);
}
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params: params || {} }));
  });
const evaluate = async (expression) => {
  const message = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const result = message.result || {};
  if (result.exceptionDetails) {
    const details = result.exceptionDetails;
    return { __error: String((details.exception && details.exception.description) || details.text || "eval 失败") };
  }
  return result.result ? result.result.value : undefined;
};
// **另开一条连到 service worker 自己的调试目标** ✓（下面要用它单独切断 SW 的网络 ✓）。
const connectTarget = async (target) => {
  const targetSocket = new WebSocket(target.webSocketDebuggerUrl);
  let targetNextId = 1;
  const targetPending = new Map();
  targetSocket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && targetPending.has(message.id)) {
      targetPending.get(message.id)(message);
      targetPending.delete(message.id);
    }
  };
  await new Promise((open) => { targetSocket.onopen = open; });
  const targetSend = (method, params) =>
    new Promise((resolve) => {
      const id = targetNextId++;
      targetPending.set(id, resolve);
      targetSocket.send(JSON.stringify({ id, method, params: params || {} }));
    });
  const targetEvaluate = async (expression) => {
    const message = await targetSend("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    const result = message.result || {};
    return result.result ? result.result.value : undefined;
  };
  return { socket: targetSocket, send: targetSend, evaluate: targetEvaluate };
};
// **等一个与断言不同的信号** ✗（第 887 轮的教训 ✓）：断言读的是图 / 字节 ✓，
// 所以这里只等 `readyState` ＋ 有界沉降 ✓ —— 绝不把"图已加载"当等待条件 ✓。
const waitReady = async () => {
  for (let i = 0; i < 60; i += 1) {
    await sleep(300);
    try {
      if (await evaluate('document.readyState === "complete"')) { await sleep(1200); return; }
    } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
};
const waitFor = async (expression, label, timeoutMs) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const value = await evaluate(expression);
    if (value) return true;
    await sleep(300);
  }
  console.log(`  ↳ 等待超时：${label}`);
  return false;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
// ⚠️ **必须关掉 HTTP 缓存** ✗ —— 这是本判据最容易假绿的地方 ✓：
// `/textures//mediums//samples/` 的响应都带 `Cache-Control: public, max-age=31536000, immutable` ✓
// ⇒ `Network.emulateNetworkConditions {offline:true}` **并不清空 HTTP 缓存** ✗
// ⇒ 只查"断网后 fetch 到了吗"，今天（SW 不写静态缓存 ✓）**照样是绿的** ✗ —— 我实测踩到过：
//    SW 缓存里一条纹理都没有 ✓，而断网 fetch 却回了 200 ＋ 正确 PNG 魔数 ✓（是 HTTP 磁盘缓存给的 ✓）。
// ⇒ 关掉它之后，断网那一趟**只有 SW 的 Cache Storage 能供上字节** ✓ ⇒ 断言才真的在判 SW ✓。
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Page.navigate", { url });
await waitReady();

// ① SW 必须激活（缓存只有在 activated 后才被用 ✓）。
const ready = await evaluate(`(async () => {
  if (!("serviceWorker" in navigator)) return "no-api";
  try {
    const registration = await Promise.race([
      navigator.serviceWorker.ready,
      new Promise((resolve) => setTimeout(() => resolve(null), 12000)),
    ]);
    return registration ? (registration.active ? "active" : "installed") : "timeout";
  } catch (error) { return "error:" + error; }
})()`);
console.log(`  ① Service Worker 状态 = ${ready}`);

// ② 在线这一趟：让**产品自己的路**先把资产用起来（纹理缩略图 / 画笔库预览 / 调色板 ✓）。
await waitFor("document.querySelectorAll('#textureThumbs img').length > 0", "纹理所略图入 DOM", 20000);
// **打开画笔库** ✓（面板懒加载 ✓：不点开就不画预览 ✓）。
await evaluate("(() => { const button = document.getElementById('brushLibraryOpen'); if (button) button.click(); return true; })()");
await waitFor("document.querySelectorAll('#brushLibraryList img[data-brush]').length > 0", "画笔库行入 DOM", 20000);

// ③ 在线把"没有 UI 入口"的那几类也各取一次（模拟用户用过 ✓ ⇒ SW 才有机会 acquire ✓）。
const probe = async (path) => evaluate(`(async () => {
  try {
    const response = await fetch(${JSON.stringify(path)});
    const bytes = new Uint8Array(await response.arrayBuffer());
    return { ok: response.ok, status: response.status, type: response.headers.get("content-type") || "",
             len: bytes.length, head: Array.from(bytes.slice(0, 4)) };
  } catch (error) { return { error: String(error) }; }
})()`);
const online = {};
for (const [kind, path] of Object.entries(URLS)) {
  online[kind] = await probe(path);
  console.log(`  · 在线 ${kind} = ${path} ⇒ ${JSON.stringify(online[kind])}`);
}
const onlineTextureThumbs = await evaluate(`(() => {
  const images = [...document.querySelectorAll("#textureThumbs img")];
  return { count: images.length, loaded: images.filter((image) => image.naturalWidth > 0).length };
})()`);
const onlinePreviews = await evaluate(`(() => {
  const images = [...document.querySelectorAll("#brushLibraryList img[data-brush]")];
  return { count: images.length, loaded: images.filter((image) => image.naturalWidth > 0).length };
})()`);
const onlinePalettes = await evaluate("document.querySelectorAll('#paletteSwatches button').length");
console.log(`  · 在线纹理缩略图 = ${JSON.stringify(onlineTextureThumbs)}｜画笔预览 = ${JSON.stringify(onlinePreviews)}｜调色板色块 = ${onlinePalettes}`);

// ④ 断网前的缓存取证（SW 到底攒了什么 ✓ —— 红的时候这是唯一能回答"为什么"的东西 ✓）。
// **有界沉降** ✓：SW 里的 `cache.put(...)` 是**点火即忘**（`.catch()` 挂 promise ✓）⇒
// 刚 fetch 完立刻列缓存可能还没落盘 ✓（本机实测踩到过：条目数正好等于 SHELL ✓、一条纹理都没有 ✓）。
await sleep(1500);
const cacheReport = await evaluate(`(async () => {
  const registration = await navigator.serviceWorker.getRegistration();
  const names = await caches.keys();
  const out = { controller: !!navigator.serviceWorker.controller, names, urls: {} };
  for (const name of names) {
    const cache = await caches.open(name);
    for (const request of await cache.keys()) out.urls[new URL(request.url).pathname] = name;
  }
  out.count = Object.keys(out.urls).length;
  out.scope = registration ? registration.scope : null;
  return out;
})()`);
const cachedPaths = Object.keys((cacheReport && cacheReport.urls) || {});
console.log(`  · 断网前：controller=${cacheReport.controller}｜缓存名=${JSON.stringify(cacheReport.names)}｜条目=${cacheReport.count}`);
for (const [kind, path] of Object.entries(URLS)) {
  console.log(`      ${cachedPaths.includes(path) ? "✓" : "✗"} 缓存里有 ${kind}：${path}`);
}

// ⑤ 断网 ⇒ 重载 ⇒ 断言。
//
// ⚠️ **两层"离线"都要做** ✗ —— 这一条是本判据最容易假绿的地方 ✓，实测过两次：
//   ① `Network.emulateNetworkConditions {offline:true}` 只作用于**页面** ✓：
//      本机实测（连到 service_worker 目标再看 ✓）—— 页面 `navigator.onLine === false` ✓，
//      **而 SW 里 `fetch("/mediums/marker.wasm")` 照样 200 ＋ 30281 字节** ✗
//      （在 SW target 上再设一次 offline 也没用 ✓，同样 200 ✗）。
//   ② `Network.setCacheDisabled` ＋ `Network.clearBrowserCache` 也**管不住 SW 的 `fetch`** ✗：
//      变异跑（把 `cache.put` 改回"只存导航"✓）时，断网 fetch **照样** 200 ＋ 正确魔数 ✓
//      ⇒ 只靠"断网后 fetch 成功"来判 ⇒ **今天也是绿的** ✗（假绿 ✓）。
// ⇒ 真办法 ✓：**在 SW 上下文里把 `self.fetch` 换成必然失败的桩** ✓ —— 这正是"网络没了"时
//   SW 看到的那件事 ✓（`caches.match` 回落那条路才是被考的对象 ✓）。并加一条**负对照** ✓：
//   一个**没被在线取过**的白名单品牌图，断网后必须**拿不到它自己的字节** ✓
//   —— 否则说明桩没生效 ✓ ⇒ **判据自己作废** ✓（而不是"产品通过"✓）。
const swTarget = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()).find(
  (target) => target.type === "service_worker",
);
const swControl = swTarget ? await connectTarget(swTarget) : null;
if (swControl) await swControl.send("Runtime.enable");
const setServiceWorkerFetch = async (broken) => {
  if (!swControl) return false;
  const value = await swControl.evaluate(
    broken
      ? `(() => { if (!self.__yanshiRealFetch) self.__yanshiRealFetch = self.fetch;
           self.fetch = () => Promise.reject(new TypeError("yanshi-offline-stub")); return typeof self.fetch; })()`
      : `(() => { if (self.__yanshiRealFetch) self.fetch = self.__yanshiRealFetch; return typeof self.fetch; })()`,
  );
  return value === "function";
};
if (!(await setServiceWorkerFetch(true))) {
  console.error("  ✗ 切不断 service worker 的网络（拿不到 SW 调试目标 / 桩没装上）⇒ 判据无法作出离线结论");
  if (swControl) swControl.socket.close();
  socket.close();
  process.exit(1);
}
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
await send("Network.clearBrowserCache");
await send("Page.reload", { ignoreCache: false });
await waitReady();

const failures = [];
const after = await evaluate(`(() => ({
  board: !!document.getElementById("board"),
  textureThumbs: (() => {
    const images = [...document.querySelectorAll("#textureThumbs img")];
    return { count: images.length, loaded: images.filter((image) => image.naturalWidth > 0).length };
  })(),
  palettes: document.querySelectorAll("#paletteSwatches button").length,
}))()`);
// 画笔库在重载后要再点开一次（面板懒加载 ✓，与在线那趟同一入口 ✓）。
await evaluate("(() => { const button = document.getElementById('brushLibraryOpen'); if (button) button.click(); return true; })()");
await waitFor("document.querySelectorAll('#brushLibraryList img[data-brush]').length > 0", "离线重载后画笔库行入 DOM", 25000);
const offlinePreviews = await evaluate(`(() => {
  const images = [...document.querySelectorAll("#brushLibraryList img[data-brush]")];
  return { count: images.length, loaded: images.filter((image) => image.naturalWidth > 0).length };
})()`);
const offline = {};
for (const [kind, path] of Object.entries(URLS)) {
  const result = await probe(path);
  offline[kind] = result;
  if (kind === "texture") offline.textureJson = result; // 纹理也会经 <img> 走一遍 ✓
}
// **负对照** ✓：白名单里的一张**在线这趟没取过**的品牌图 ✓（`/brand/svg/icon-light.svg` 在 SHELL 里 ✓，
// 这里挑一个不在 SHELL、也没被产品取过的 ✓）⇒ 断网后它**必须**落到外壳 HTML ✓，
// 否则说明 SW 的网络桩没生效 ✓ ⇒ 上面那些"拿到了"就一文不值 ✓。
const CONTROL_PATH = "/brand/svg/logo-mono-dark.svg";
const control = await probe(CONTROL_PATH);
// **把 SW 的 fetch 恢复回去** ✓（别把这个浏览器实例弄成半残 ✓）。
await setServiceWorkerFetch(false);
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
if (swControl) swControl.socket.close();

console.log(`  ② 断网重载后：画布=${after.board}｜纹理缩略图=${JSON.stringify(after.textureThumbs)}｜调色板色块=${after.palettes}｜画笔预览=${JSON.stringify(offlinePreviews)}`);
for (const [kind, path] of Object.entries(URLS)) {
  console.log(`  · 断网 ${kind} = ${path} ⇒ ${JSON.stringify(offline[kind])}`);
}
console.log(`  · 负对照（没缓存过）= ${CONTROL_PATH} ⇒ ${JSON.stringify(control)}`);

// **魔数检查** ✓ —— 只看 `ok` 会被"回落成外壳 HTML ✓、状态仍是 200 ✗"骗过 ✓。
const MAGIC = { wasm: "0,97,115,109", png: "137,80,78,71" };
const magicOf = (result) => (result && Array.isArray(result.head) ? result.head.join(",") : "");

if (ready !== "active") failures.push(`Service Worker 未激活（${ready}）⇒ 离线无从谈起`);
if (!after.board) failures.push("断网重载后画布不存在 ⇒ 断网重载本身没成功（先修外壳）");
// ① 产品自己的路（<img>）—— 今天会回落到外壳 HTML ⇒ naturalWidth 为 0 ⇒ 红。
if (!(after.textureThumbs && after.textureThumbs.loaded > 0)) {
  failures.push(`断网后纹理缩略图一张都没画出来（${JSON.stringify(after.textureThumbs)}）⇒ /textures/ 没进缓存（拿到的是外壳 HTML）`);
}
if (!(offlinePreviews && offlinePreviews.loaded > 0)) {
  failures.push(`断网后画笔库预览图一张都没画出来（${JSON.stringify(offlinePreviews)}）⇒ /brush-previews/ 没进缓存`);
}
// 调色板走查看器的本地只读缓存（没有静态路由 ✓）—— 这是它离线可用的唯一机制 ✓。
if (!(after.palettes > 0)) {
  failures.push(`断网后调色板色块为 0 ⇒ 查看器没有把只读工具（list_palette_colors / list_assets）的响应缓存下来`);
}
// ② 字节级（魔数 ＋ 长度与在线那一趟一致 ✓）。
const byteChecks = [
  ["medium", "wasm", MAGIC.wasm],
  ["sample", "png", MAGIC.png],
  ["brand", "png", MAGIC.png],
  ["texture", "png", MAGIC.png],
];
for (const [kind, , magic] of byteChecks) {
  const got = offline[kind];
  const want = online[kind];
  if (!got || got.error) {
    failures.push(`断网后取不到 ${kind}（${URLS[kind]}）：${got && got.error} ⇒ SW 没 acquire 它`);
    continue;
  }
  if (!got.ok) {
    failures.push(`断网后 ${kind} 返回 HTTP ${got.status}（期望 200）`);
    continue;
  }
  if (magicOf(got) !== magic) {
    failures.push(
      `断网后 ${kind} 的魔数是 [${magicOf(got) || "空"}]（期望 [${magic}]）⇒ 拿到的是外壳 HTML 而不是资产字节`,
    );
    continue;
  }
  if (want && want.ok && want.len > 0 && got.len !== want.len) {
    failures.push(`断网后 ${kind} 的长度 ${got.len} 与在线那趟 ${want.len} 不同 ⇒ 拿到的不是同一份字节`);
  }
  if (!(cachedPaths.includes(URLS[kind]))) {
    // 缓存报告是在**断网前**取的 ✓ —— 它得能证明"SW 真的把这类资产收进来了" ✓。
    failures.push(`断网前的缓存报告里没有 ${kind}（${URLS[kind]}）⇒ SW 从未 acquire 它`);
  }
}
// 断网前那几类必须真的在线成功过 ✓（否则"离线拿到"无从谈起 ✓）。
for (const [kind, result] of Object.entries(online)) {
  if (!result || result.error || !result.ok || result.len === 0) {
    failures.push(`在线这一趟就没取到 ${kind}（${JSON.stringify(result)}）⇒ 判据前置不成立`);
  }
}

// ③ **负对照必须成立** ✗：没缓存过的控制文件断网后**不许**拿到它自己的字节 ✓。
const CONTROL_FILE_SIZE = statSync("assets" + CONTROL_PATH).size;
if (control && !control.error && control.ok && control.len === CONTROL_FILE_SIZE) {
  failures.push(
    `负对照失败：没被缓存过的 ${CONTROL_PATH} 断网后仍拿到了它自己的 ${control.len} 字节 ` +
      "⇒ SW 的 fetch 桩没生效 ⇒ 本判据的断网是假的（**判据作废**，不是产品通过）",
  );
}

socket.close();
if (failures.length) {
  console.log(`  ✗ 离线资产端到端不成立：${failures.join("；")}`);
  process.exit(1);
}
console.log("  ✓ 离线资产端到端成立：断网后纹理/预览/调色板照常显示，介质/示例/品牌/纹理的字节与在线一致");
process.exit(0);
