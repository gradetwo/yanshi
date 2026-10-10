#!/usr/bin/env node
// **离线落笔判据**（目标 (A)⑥ 的最后一块）：断网时**普通 .myb 画笔**必须能画出墨 ✓，
// 而且画出来的东西必须与**在线那一笔逐字节相同** ✓。
//
// 为什么需要它 ✗：产品一直把普通画笔的墨交给**服务端** ✓（`viewer-app.js` 的
// `commitShape` 里那条"画笔不能走本地内核"的说明 ✓）⇒ 断网时 `brush_stroke` 只进离线队列 ✓、
// 画布上**一个像素都没有** ✗（旧判据量到的一直是 `76800 => 76800` = 只有底色 ✓）。
//
// 三条断言（每一条都能单独变红 ✓）：
//   ① **离线出墨**：断网后同一笔让画布的**非背景像素数 > 0** ✓
//      （旧判据数的是"不透明像素"✗ —— 底色本来就是不透明的 ✓ ⇒ 那条阈值永远不可能满足 ✓）。
//   ② **两种模式逐字节相同**：在**两份同样全新的文档**上画**同一串坐标** ✓ ——
//      在线那一笔走服务端 Hokusai ✓、离线那一笔走共享内核 ✓ ⇒ 整块 `#board` 的 RGBA 必须逐字节相同 ✓。
//   ③ **真的是内核画的，且没有服务端往返**：内核自己的计数器必须涨 ✓
//      （`blobs` = 本地 CAS 条数 ✓、`head_seq` ✓），且**离线期间一个 `brush_stroke` 请求都没发** ✓
//      （用 CDP 的 `Network.requestWillBeSent` 数 ✓，不是靠像素猜 ✓）。
//
// 用法：node scripts/browser-offline-brush.mjs <viewer-url> <base> <token> [cdpPort]
// **★ 离线判据**必须**用查看器（**静态 PWA ✓）✗ ★**（第 111 轮 ✓；**有诊断证据 ✓）：
//   **∴ 证据 ✗**：`browser-offline-shell` 报**✗**：
//     **"**断网后本地内核没有就绪（`kernelStats()` 为 null ✓）"**✗
//       ＋ **"**断网前内核 = **null**（**在线都没就绪 ✓）"** ✓**** ✓✓
//     **∴ 而** `browser-offline-journal` 报**✗**：
//       **"**WASM 内核必须就绪（**指针笔迹这条路要用它 ✓）"** ✓**** ✓✓
//   **∴ 根因 ✗**：**离线落笔**要**本地内核**✗ ⇒ **∴ 而**内核**只在
//     **`__pwaLocalOnly === true`**（**静态 PWA ✓）时**才加载 ✓**** ✓✓
//     ⇒ **∴ 在**服务端 URL 下**✗ ⇒ **∴ 页面**走**服务端优先 ⇒ **内核**不加载 ✓**** ✓✓
//   **∴ 所以 ✗**：**"**离线 ✓"**这件事**在**静态 PWA** 上**才有意义 ✓**** ✓✓
//     （**∴ 因为**静态部署**本来**就没有服务端 ✓）** ✓✓
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
const base = process.argv[3];
const token = process.argv[4];
const port = process.env.CDP_PORT || process.argv[5] || "9333";
if (!url || !base) {
  console.error("用法: node scripts/browser-offline-brush.mjs <viewer-url> <base> <token> [cdpPort]");
  process.exit(2);
}
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const BRUSH = "100%_Opaque";
const SIZE = 20;
const COLOUR = "#101820";
// **固定的合成笔迹** ✓ —— 两个模式传的是**同一串坐标** ✓（判据的前提 ✓）。
const STROKE = [[80, 70], [110, 95], [140, 80], [170, 110], [200, 95]];

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((target) => target.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效（不是通过 ✗）"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
// **服务端往返计数** ✓（断言 ③ 的另一半 ✓）：离线期间发往 `/api/tools/brush_stroke` 的请求数。
let brushStrokeRequests = 0;
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Network.requestWillBeSent") {
    const request = (message.params && message.params.request) || {};
    if (String(request.url || "").includes("/api/tools/brush_stroke")) brushStrokeRequests += 1;
  }
  if (message.id && pending.has(message.id)) { pending.get(message.id)(message); pending.delete(message.id); }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  pending.set(id, resolve);
  socket.send(JSON.stringify({ id, method, params: params || {} }));
});
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (result.result && result.result.exceptionDetails) {
    return { __error: String((result.result.exceptionDetails.exception || {}).description ||
      JSON.stringify(result.result.exceptionDetails)).slice(0, 300) };
  }
  return result.result?.result?.value;
};
// **另开一条连到 service worker 自己的调试目标** ✓（本次审计加的 ✓）：下面要在 SW 上下文里
// 把 `self.fetch` 换成必然失败的桩 ✓（与 `browser-offline-assets.mjs` 同一招 ✓）。
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
await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
// **关掉浏览器的 HTTP 缓存** ✗ —— 否则判定"离线能画"时，页面的 JS/WASM 可能来自 HTTP 缓存 ✓
// 而不是**我们自己的 SW + 本地缓存** ✓（第 60 轮实测过这个"侥幸" ✓，判据自己也写着那句 ✓）。
// 关掉之后，"离线那一笔出自本地内核"才是真的 ✓；顺带让本机反复改 JS 后必跑新代码 ✓
//（构建标识只到分钟 ⇒ 一分钟内连改两次会被 SW/HTTP 缓存喂回旧 JS ✓，本机变异测试当场被骗过一次 ✗）。
await send("Network.setCacheDisabled", { cacheDisabled: true });

const failures = [];
const waitComplete = async () => {
  for (let i = 0; i < 60; i += 1) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) break; } catch (_) { /* 还没就绪 */ }
  }
  await sleep(1000);
};
// **读画布**：整块 `#board` 的 RGBA ⇒ base64 ✓（307200 字节 ✓，直接带回来才能算"差了几个字节" ✓）。
const BOARD_B64 = `(() => {
  const canvas = document.getElementById("board");
  const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < data.length; i += chunk) {
    binary += String.fromCharCode.apply(null, data.subarray(i, i + chunk));
  }
  return btoa(binary);
})()`;
const INK = `(() => {
  const canvas = document.getElementById("board");
  const data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data;
  let nonBackground = 0;
  let dark = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] > 8) {
      if (!(data[i] === 255 && data[i + 1] === 255 && data[i + 2] === 255)) nonBackground += 1;
      if (data[i] + data[i + 1] + data[i + 2] < 240) dark += 1;
    }
  }
  return { nonBackground, dark };
})()`;
/// **有界等出墨** ✓（等待条件与断言条件对齐 ✓ —— 第 1116 轮定的规矩 ✓）。
const waitForInk = async (label, limit) => {
  let last = null;
  let stable = 0;
  for (let i = 0; i < limit; i += 1) {
    await sleep(250);
    last = await evaluate(INK);
    if (last && last.nonBackground > 0) {
      stable += 1;
      if (stable >= 2) { console.log(`  · ${label}：第 ${i + 1} 次采样连续出墨（非背景 ${last.nonBackground}）`); return last; }
    } else {
      stable = 0;
    }
  }
  console.log(`  · ${label}：轮询 ${limit} 次仍没有稳定出墨（最后一次 ${JSON.stringify(last)}）`);
  return last;
};
const selectBrush = async () => {
  // **有界重试** ✓：面板是懒加载的 ✓（`refreshBrushOptions` 要等一次网络/离线回退 ✓）
  // ⇒ 点一次就读，会把"还没填完"当成"没有这支笔" ✗（第 1090 轮的老教训 ✓）。
  let last = null;
  for (let attempt = 0; attempt < 20; attempt += 1) {
    await evaluate(`(() => { const button = document.getElementById("brushLibraryOpen"); if (button) button.click(); return true; })()`);
    await sleep(400);
    last = await evaluate(`(() => {
      const select = document.getElementById("brush");
      if (!select) return { found: false, reason: "no-select" };
      const option = [...select.options].find((item) => item.value === ${JSON.stringify(BRUSH)});
      if (!option) return { found: false, reason: "no-option", options: select.options.length,
        sample: [...select.options].slice(0, 4).map((item) => item.value) };
      select.value = option.value;
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return { found: true, value: option.value, options: select.options.length };
    })()`);
    if (last && last.found) break;
  }
  await sleep(300);
  return last;
};
// **一笔真实指针笔迹** ✓ —— 走页面自己的 `pointerdown/move/up` 处理器 ✓（不是直接调提交函数 ✓）。
const PAINT = `(async () => {
  const canvas = document.getElementById("board");
  const rect = canvas.getBoundingClientRect();
  const scaleX = canvas.width / rect.width;
  const scaleY = canvas.height / rect.height;
  const event = (x, y, extra) => Object.assign({
    bubbles: true, cancelable: true, pointerId: 1, pointerType: "mouse", isPrimary: true,
    button: 0, buttons: 1, clientX: rect.left + x / scaleX, clientY: rect.top + y / scaleY,
  }, extra || {});
  const points = ${JSON.stringify(STROKE)};
  canvas.dispatchEvent(new PointerEvent("pointerdown", event(points[0][0], points[0][1])));
  await new Promise((resolve) => setTimeout(resolve, 30));
  for (let index = 1; index < points.length; index += 1) {
    for (let step = 1; step <= 4; step += 1) {
      const t = step / 4;
      const x = points[index - 1][0] + (points[index][0] - points[index - 1][0]) * t;
      const y = points[index - 1][1] + (points[index][1] - points[index - 1][1]) * t;
      canvas.dispatchEvent(new PointerEvent("pointermove", event(x, y)));
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
  }
  canvas.dispatchEvent(new PointerEvent("pointerup",
    event(points[points.length - 1][0], points[points.length - 1][1], { buttons: 0 })));
  return "dispatched";
})()`;
const preparePage = async (docId, docToken) => {
  await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${docToken}` });
  await waitComplete();
  // **内核必须真的就绪** ✗ —— 判据后面所有断言都以它为前提 ✓。
  // 实测（本机高负载时）：`readyState` 完成 ≠ 内核装载完成 ✓ ⇒ 预热那一趟若提前离开 ✓，
  // `/api/atoms` 就没进本地缓存 ✓ ⇒ 离线重载时内核是 `null` ✓ ⇒ 报出来的却是"离线画不出墨" ✗
  //（**量错对象**的老毛病 ✓）⇒ 这里**有界等到内核就绪** ✓，落笔前也断言它不是 null ✓。
  const chosen = await selectBrush();
  await evaluate(`(() => { window.yanshi.setColor(${JSON.stringify(COLOUR)}); window.yanshi.setSize(${SIZE}); return true; })()`);
  await sleep(200);
  let kernel = null;
  for (let i = 0; i < 40; i += 1) {
    kernel = await evaluate("window.yanshi.kernelStats()");
    if (kernel) break;
    await sleep(250);
  }
  if (!kernel) console.log(`  · 注意：${docId} 的内核仍未就绪（kernelStats() = null）`);
  return chosen;
};

// ① **只等页面与 Service Worker 就绪，不清理缓存** ✗。
//
// 我第一版在这里 `unregister()` + `caches.delete()` 全部缓存 —— 本机单跑没问题 ✓，
// 但官方 runner 里**所有 browser-* 判据共用同一个浏览器实例** ✓ ⇒
// 这一下会把**别的判据要用的外壳缓存**删掉 ✓：
// 实测 `browser-offline-draw` 报「SW 缓存里没有共享内核」✗、
// `browser-render-switch` 报「内核句柄 = false」✗ —— 两条都不是它们自己的问题 ✓。
// ⇒ **判据不许改动共享环境** ✓（要保证"跑的是这一版 JS"，用 `Network.setCacheDisabled` 就够 ✓；
// 官方 runner 每次都是全新 profile ✓，本来就没有旧外壳 ✗）。
await send("Page.navigate", { url });
await waitComplete();
const swState = await evaluate(`(async () => {
  if (!("serviceWorker" in navigator)) return "no-api";
  try {
    const registration = await Promise.race([
      navigator.serviceWorker.ready,
      new Promise((resolve) => setTimeout(resolve, 12000)),
    ]);
    return registration ? (registration.active ? "active" : "installed") : "timeout";
  } catch (error) { return "error:" + error; }
})()`);
console.log(`  · Service Worker = ${swState}`);
if (swState !== "active") failures.push(`Service Worker 未激活（${swState}）⇒ 离线那一段无法成立`);

// ② **两份同样全新的文档** ✓（同一份不行 ✗：在线那一笔会留在服务端 ⇒ 两张画布起点就不同 ✓）。
const suffix = Date.now().toString(36);
const onlineDoc = { id: `crit_obrush_on_${suffix}` };
const offlineDoc = { id: `crit_obrush_off_${suffix}` };
for (const doc of [onlineDoc, offlineDoc]) {
  const created = await fetch(`${base}/api/documents`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: doc.id, width: 320, height: 240 }),
  }).then((response) => response.json()).catch((error) => ({ error: String(error) }));
  if (!created || !created.token) {
    console.log(`  ✗ 无法新建对照文档 ${doc.id} ⇒ 判据无效：${JSON.stringify(created).slice(0, 160)}`);
    socket.close();
    process.exit(1);
  }
  doc.token = created.token;
}
// **在线预热两份文档** ✗：离线时 `/api/atoms` 走浏览器本地缓存 ✓（`fetchOrLocal` ✓）
// ⇒ 不预热就根本没有可折叠的日志 ✓。
for (const doc of [onlineDoc, offlineDoc]) {
  await send("Page.navigate", { url: `${base}/?doc=${doc.id}&token=${doc.token}` });
  await waitComplete();
  // **预热必须等到内核真的装载完** ✗（见 `preparePage` 的说明 ✓）。
  for (let i = 0; i < 40; i += 1) {
    if (await evaluate("window.yanshi.kernelStats()")) break;
    await sleep(250);
  }
}

// ③ 在线那一笔
const onlineBrush = await preparePage(onlineDoc.id, onlineDoc.token);
console.log(`  · 在线选笔 = ${JSON.stringify(onlineBrush)}`);
if (!onlineBrush || !onlineBrush.found) failures.push(`在线选不到笔刷 ${BRUSH} ⇒ 判据无效（${JSON.stringify(onlineBrush)}）`);
const onlineBefore = await evaluate(INK);
console.log(`  · 在线落笔前：${JSON.stringify(onlineBefore)}`);
console.log(`  · 在线落笔 = ${await evaluate(PAINT)}`);
let onlineStats = null;
for (let i = 0; i < 40; i += 1) {
  await sleep(250);
  onlineStats = await evaluate(`(() => {
    const stats = window.yanshiStats;
    return { serverBlits: stats.serverBlits || 0, serverInk: stats.lastBlitServerInk || 0,
             kernelHead: stats.kernelHead || 0, kernel: window.yanshi.kernelStats() };
  })()`);
  if (onlineStats && onlineStats.serverBlits > 0 && onlineStats.serverInk > 0) break;
}
const onlineInk = await waitForInk("在线", 24);
const onlineBoard = await evaluate(BOARD_B64);
console.log(`  · 在线落笔后：${JSON.stringify(onlineInk)}｜补画 ${onlineStats && onlineStats.serverBlits} 次｜服务端墨 ${onlineStats && onlineStats.serverInk}`);
if (!onlineInk || onlineInk.nonBackground <= 0) failures.push(`在线落笔后画布上没有墨 ⇒ 判据前提不成立（${JSON.stringify(onlineInk)}）`);

// ④ 断网 + 在**另一份同样全新的文档**上画同一笔
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
// ⚠️ **CDP 的网络模拟到不了 service worker 自己的 `fetch()`** ✗（本次审计复核 ✓）：
// 页面 `navigator.onLine === false` 时，SW 里 `fetch(...)` 照样 200 ✓，
// `Network.setCacheDisabled` 与 `clearBrowserCache` 都管不住 ✓ ⇒ **只模拟页面网络时，
// 本判据可以在 SW 仍走活网（离线导航与 .myb 都现取）的情况下通过** ✗ ⇒ 那样它就没有证明 SW 缓存 ✓。
// ⇒ 在 SW 上下文里把 `self.fetch` 换成必然失败的桩 ✓ —— 这正是"网络没了"时 SW 看到的那件事 ✓
// ⇒ 之后外壳/内核/笔刷只能来自 **SW 自己的 Cache Storage** ✓。
const swTarget = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())
  .find((target) => target.type === "service_worker");
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
// **负对照**（本次审计加的 ✓）：断网状态下，页面自己发一个**同一来源、已知不会被缓存**的请求 ⇒
// 它**必须失败** ✓。为什么它不可能被缓存 ✓：`service-worker.js` 对 `/api/` 前缀直接 `return`
//（不 respondWith ✓）⇒ **永远进不了 SW 的 Cache Storage** ✓；路径不存在（服务端 404 ✓）
// ＋ `cache: "no-store"` ＋每次唯一 nonce ⇒ **也进不了浏览器 HTTP 缓存** ✓。
// 它若居然成功 ⇒ 说明"断网"没真的生效 ⇒ **本判据作废（VOID）**，不是产品通过 ✗。
const NEGATIVE_CONTROL = "/api/__offline_negative_control__?nonce=" + Date.now();
const controlProbe = await evaluate(`(async () => {
  try {
    const response = await fetch(${JSON.stringify(NEGATIVE_CONTROL)}, { cache: "no-store" });
    return { failed: false, status: response.status, bytes: (await response.arrayBuffer()).byteLength };
  } catch (error) { return { failed: true, error: String(error) }; }
})()`);
console.log("  · 负对照（未缓存接口必须失败）= " + JSON.stringify(controlProbe));
// **负对照不成立 ⇒ 立刻作废（VOID）** ✗ —— 不要带着"断网是假的"这个前提继续跑几十秒。
if (!(controlProbe && controlProbe.failed)) {
  console.error(`  ⊘ 判据作废（VOID）：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 本跑没有结论`);
  await setServiceWorkerFetch(false);
  if (swControl) swControl.socket.close();
  await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  socket.close();
  process.exit(1);
}
brushStrokeRequests = 0; // **只数离线之后的** ✓
const offlineBrush = await preparePage(offlineDoc.id, offlineDoc.token);
const onlineFlag = await evaluate("navigator.onLine");
console.log(`  · 离线 onLine=${onlineFlag}｜选笔 = ${JSON.stringify(offlineBrush)}`);
if (!offlineBrush || !offlineBrush.found) failures.push(`离线选不到笔刷 ${BRUSH} ⇒ 用户离线根本没法用这支笔（${JSON.stringify(offlineBrush)}）`);
const offlineBefore = await evaluate(INK);
let kernelBefore = await evaluate("window.yanshi.kernelStats()");
for (let i = 0; i < 40 && !kernelBefore; i += 1) {
  await sleep(250);
  kernelBefore = await evaluate("window.yanshi.kernelStats()");
}
if (!kernelBefore) failures.push("离线时内核没有就绪（kernelStats() 为 null）⇒ 本地根本画不了");
console.log(`  · 离线落笔前：${JSON.stringify(offlineBefore)}｜内核 ${JSON.stringify(kernelBefore)}`);
console.log(`  · 离线落笔 = ${await evaluate(PAINT)}`);
let offlineStats = null;
for (let i = 0; i < 60; i += 1) {
  await sleep(250);
  offlineStats = await evaluate(`(() => {
    const stats = window.yanshiStats;
    // **内核自己渲染出来的那一块** ✓ —— 断言 ③ 的最强形式 ✓：
    // 只看画布分不清"墨来自内核落笔"与"墨是拖动期留在画布上的预览" ✗
    //（本机变异实测：把本地落笔掐掉之后，画布上仍有预览留下的墨 ✓，而内核渲染里是空的 ✓）。
    const kernelInk = (() => {
      const kernel = state.kernel;
      if (!kernel || typeof kernel.render_region_rgba !== "function") return -1;
      const rgba = kernel.render_region_rgba(0, 0, 320, 240);
      let ink = 0;
      for (let i = 0; i + 3 < rgba.length; i += 4) {
        if (rgba[i + 3] > 8 && !(rgba[i] === 255 && rgba[i + 1] === 255 && rgba[i + 2] === 255)) ink += 1;
      }
      return ink;
    })();
    return { serverBlits: stats.serverBlits || 0, offlineBrushPaints: stats.offlineBrushPaints || 0,
             kernelInk: kernelInk,
             kernelHead: stats.kernelHead || 0, outbox: window.yanshi.outbox() };
  })()`);
  if (offlineStats && offlineStats.offlineBrushPaints > 0) break;
}
const offlineInk = await waitForInk("离线", 24);
const kernelAfter = await evaluate("window.yanshi.kernelStats()");
const offlineBoard = await evaluate(BOARD_B64);
const outbox = await evaluate("window.yanshi.outbox()");
console.log(`  · 离线落笔后：${JSON.stringify(offlineInk)}｜内核 ${JSON.stringify(kernelAfter)}`);
console.log(`  · 离线本地落笔次数 = ${offlineStats && offlineStats.offlineBrushPaints}｜内核渲染墨 = ${offlineStats && offlineStats.kernelInk}｜离线期间 brush_stroke 请求数 = ${brushStrokeRequests}｜离线队列 = ${JSON.stringify(outbox && { pending: outbox.pending, state: outbox.state })}`);

// ④b **断网重载**：那一笔必须**还在** ✓ —— 离线时它只进了内核日志与离线队列 ✓，
// 内核日志**不持久** ✗ ⇒ 不重放的话一刷新就没了 ✓（这就是"本地队列重放"那条产品改动 ✓）。
await send("Page.reload", { ignoreCache: false });
await waitComplete();
let reloadKernel = await evaluate("window.yanshi.kernelStats()");
for (let i = 0; i < 60 && !reloadKernel; i += 1) {
  await sleep(250);
  reloadKernel = await evaluate("window.yanshi.kernelStats()");
}
const reloadInk = await waitForInk("断网重载后", 40);
const reloadBoard = await evaluate(BOARD_B64);
const reloadReplayed = await evaluate("window.yanshiStats.offlineBrushReplayed || 0");
const reloadOutbox = await evaluate("window.yanshi.outbox()");
console.log(`  · 断网重载后：${JSON.stringify(reloadInk)}｜内核 ${JSON.stringify(reloadKernel && { head_seq: reloadKernel.head_seq, blobs: reloadKernel.blobs })}｜本地重放 ${reloadReplayed} 笔｜队列 ${JSON.stringify({ pending: reloadOutbox && reloadOutbox.pending })}`);

// ⑤ 三条断言
if (!offlineInk || offlineInk.nonBackground <= offlineBefore.nonBackground) {
  failures.push(`离线画不出墨：非背景像素 ${offlineBefore && offlineBefore.nonBackground} => ${offlineInk && offlineInk.nonBackground}`);
}
const blobsBefore = (kernelBefore && kernelBefore.blobs) || 0;
const blobsAfter = (kernelAfter && kernelAfter.blobs) || 0;
const headAfter = (kernelAfter && kernelAfter.head_seq) || 0;
if (!(offlineStats && offlineStats.offlineBrushPaints >= 1)) {
  failures.push("离线落笔没有走本地内核那条路（offlineBrushPaints 没涨）⇒ 墨不是内核给的");
}
if (!(offlineStats && offlineStats.kernelInk > 0)) {
  failures.push(`内核自己渲染出来的画布里没有这一笔（kernelInk=${offlineStats && offlineStats.kernelInk}）` +
    "⇒ 画布上的墨不是内核落笔的结果（很可能是拖动期预览的残留）");
}
if (!(blobsAfter > blobsBefore)) {
  failures.push(`内核 CAS 里没有新增笔触位图（blobs ${blobsBefore} => ${blobsAfter}）⇒ 内核没有真的画`);
}
if (!(headAfter >= 2 && offlineStats && offlineStats.kernelHead >= 2)) {
  failures.push(`内核日志 HEAD 没有增长（kernel.stats.head_seq=${headAfter}，页面 kernelHead=${offlineStats && offlineStats.kernelHead}）`);
}
if (brushStrokeRequests !== 0) {
  failures.push(`离线期间仍然把 brush_stroke 发了出去（${brushStrokeRequests} 次）⇒ 出现服务端往返`);
}
if (onlineFlag !== false) {
  failures.push(`网络并没有真的被切断（navigator.onLine=${onlineFlag}）⇒ 判据前提不成立`);
}
// **负对照必须成立** ✗：未缓存的同源请求断网后仍成功 ⇒ 断网模拟没生效 ⇒ 这一跑没有结论。
if (!(controlProbe && controlProbe.failed)) {
  failures.push(`负对照失败：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 判据作废（VOID）`);
}
if (!(reloadInk && reloadInk.nonBackground > 0)) {
  failures.push(`断网重载后那一笔没了（非背景像素 ${reloadInk && reloadInk.nonBackground}）⇒ 离线落笔没有挺过刷新`);
}
if (!(reloadReplayed >= 1)) {
  failures.push(`断网重载后没有从离线队列重放（offlineBrushReplayed=${reloadReplayed}）⇒ 笔触没有本地持久来源`);
}
if (!(reloadKernel && reloadKernel.blobs >= 1)) {
  failures.push(`断网重载后的内核里没有那张位图（blobs=${reloadKernel && reloadKernel.blobs}）`);
}
if (!reloadBoard || typeof reloadBoard !== "string") {
  failures.push("断网重载后读画布失败 ⇒ 逐字节比对无法进行");
} else if (offlineBoard && typeof offlineBoard === "string" && reloadBoard !== offlineBoard) {
  const leftReload = Buffer.from(offlineBoard, "base64");
  const rightReload = Buffer.from(reloadBoard, "base64");
  let differingReload = 0;
  for (let index = 0; index < Math.min(leftReload.length, rightReload.length); index += 1) {
    if (leftReload[index] !== rightReload[index]) differingReload += 1;
  }
  failures.push(`断网重载后的画布与刷新前不同（${differingReload} 个字节不同）⇒ 重放不是同一笔`);
}
if (!offlineBoard || typeof offlineBoard !== "string" || !onlineBoard || typeof onlineBoard !== "string") {
  failures.push("读画布失败 ⇒ 逐字节比对无法进行");
} else {
  const left = Buffer.from(onlineBoard, "base64");
  const right = Buffer.from(offlineBoard, "base64");
  let differing = 0;
  let maxDelta = 0;
  let firstDiff = -1;
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const delta = Math.abs(left[index] - right[index]);
    if (delta !== 0) {
      differing += 1;
      if (firstDiff < 0) firstDiff = index;
      if (delta > maxDelta) maxDelta = delta;
    }
  }
  const extra = left.length !== right.length ? Math.abs(left.length - right.length) : 0;
  console.log(`  · 逐字节比对：在线 ${left.length} 字节 vs 离线 ${right.length} 字节｜不同 ${differing + extra}` +
    (firstDiff >= 0 ? `｜首个 @${firstDiff}（像素 ${(firstDiff / 4) | 0}，通道 ${firstDiff % 4}）｜最大通道差 ${maxDelta}` : "") +
    `｜${differing + extra === 0 ? "**逐字节相同** ✓" : "有差异 ✗"}`);
  if (differing + extra !== 0) {
    failures.push(`离线与在线不是逐字节相同：${differing + extra} 个字节不同（首个 @${firstDiff}，最大通道差 ${maxDelta}）`);
  }
  if (reloadBoard && typeof reloadBoard === "string") {
    const reloadBytes = Buffer.from(reloadBoard, "base64");
    let reloadDiff = 0;
    for (let index = 0; index < Math.min(left.length, reloadBytes.length); index += 1) {
      if (left[index] !== reloadBytes[index]) reloadDiff += 1;
    }
    console.log(`  · 断网重载后的画布 vs 在线：不同 ${reloadDiff} 字节｜${reloadDiff === 0 ? "**逐字节相同** ✓" : "有差异 ✗"}`);
    if (reloadDiff !== 0) failures.push(`断网重载后的画布与在线不同（${reloadDiff} 个字节）`);
  }
}

await setServiceWorkerFetch(false);
if (swControl) swControl.socket.close();
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
socket.close();
if (failures.length) {
  console.log(`  ✗ 离线落笔未达成：${failures.join("；")}`);
  process.exit(1);
}
console.log("  ✓ 离线落笔达成：断网能画、与在线逐字节相同、且墨出自本地内核");
process.exit(0);
