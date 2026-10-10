#!/usr/bin/env node
// **离线画一笔**判据（目标 (A)③⑥）：断网后仍必须能在画布上画出墨。
// **第 213 轮更正** ✓：这里原来找 `/brush-module.wasm` ✗ —— 那是**已退休的第二份实现** ✓（端点现在 404 ✓）
// ⇒ 判据应当看**共享内核**（`/wasm/yanshi_wasm.js` ✓）在不在 SW 缓存里 ✓。
// 另一条更硬的信号 ✓：`localBrushFrames`（成功帧数 ✓）必须 > 0，且 `localBrushErrors` 必须为 0 ✓
// —— 第 212 轮那个"一个词的错"就是**只有它**能一眼看出 ✓（15 次调用 / 15 次失败 ✓）。
// 为什么它是"离线优先"的成功定义：能"打开页面"只是外壳（已达成 ✓），
// 真正的要求是**离线下还能画**（本地渲染 + 本地状态，服务端不在也算数）。
// 用法：node scripts/browser-offline-draw.mjs <viewer-url> [cdpPort]
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
// **端口一律先从环境变量取** ✓（第 408 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（一个完整 URL）** ✗ ⇒
// 原先 `process.argv[3] || process.env.CDP_PORT` ✗ ⇒ **port 拿到的是 URL** ✗ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **这两个判据一直取不到调试目标** ✗
//（**而它们是"离线"判据** ✓ ⇒ 影响 A⑥ 的验证 ✓）。
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-draw.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
// **取证**（第 11 轮定 ✓）：把 console、日志、失败请求都收起来 ⇒ 判据**自带诊断** ✓
const evidence = [];
const requestUrls = new Map();   // **requestId → url** ✓（供 loadingFailed 回查 ✓）
socket.addEventListener("message", (event) => {
  let m = null;
  try { m = JSON.parse(event.data); } catch (error) { return; }
  if (m.method === "Runtime.consoleAPICalled") {
    const text = (m.params.args || []).map((a) => a.value === undefined ? (a.description || a.type) : String(a.value)).join(" ");
    evidence.push("console." + m.params.type + ": " + text.slice(0, 200));
  } else if (m.method === "Log.entryAdded") {
    evidence.push("log." + m.params.entry.level + ": " + String(m.params.entry.text).slice(0, 200));
  } else if (m.method === "Network.requestWillBeSent") {
    // **记 requestId → url** ✓（`loadingFailed` 不带 URL ✗ ⇒ 只能这样回查 ✓）。
    const request = (m.params && m.params.request) || {};
    if (m.params && m.params.requestId) requestUrls.set(m.params.requestId, String(request.url || ""));
  } else if (m.method === "Network.loadingFailed") {
  } else if (m.method === "Network.responseReceived") {
    // **不扰动的观测** ✓（第 55 轮定 ✓）：只记网络流水，不替页面发请求 ✓。
    const response = (m.params && m.params.response) || {};
    if (String(response.url || "").includes("yanshi_wasm.js") || String(response.url || "").includes("yanshi_wasm_bg.wasm")) {
      evidence.push("module-response: status=" + response.status + " fromDisk=" + !!response.fromDiskCache +
        " fromSW=" + !!response.fromDiskCache + " encoded=" + (m.params.response && m.params.response.encodedDataLength) +
        " mime=" + response.mimeType);
    }
    evidence.push("request-failed: " + String(m.params.errorText) + " url=" + (requestUrls.get(m.params.requestId) || "?").replace(/^https?:\/\/[^/]+/, "") + " (type " + m.params.type + ")");
  }
});
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
// **单因素实验开关**（目标 (A)③ 的诊断用）：`BYPASS_SW=1` ⇒ 让页面**绕过 Service Worker** ✓
// ⇒ 用于回答"是不是 SW 的取数把在线预览弄坏了" ✓（第 10 轮定的隔离实验 ✓）。
if (process.env.NO_HTTP_CACHE === "1") {
  await send("Network.setCacheDisabled", { cacheDisabled: true });
  console.log("  （实验模式：已关闭浏览器的 HTTP 缓存）");
}
if (process.env.BYPASS_SW === "1") {
  await send("Network.setBypassServiceWorker", { bypass: true });
  console.log("  （实验模式：已让页面绕过 Service Worker）");
}
await send("Page.navigate", { url });
// ⚠️ **这里只能等 `readyState` ＋ 有界沉降** ✗（第 894 轮 ✓）：下面 `INK` 数的是 **canvas 与暗像素** ✗
// ⇒ **"有几个 canvas"本身就是被断言的东西** ⇒ 等它就会让断言永不失败 ✗（**第 887 轮的教训** ✓）。
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate('document.readyState === "complete"')) { await sleep(800); break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
// ⚠️ **本文件其余四处 `sleep` 是**故意保留**的** ✗（第 894 轮判过 ✓，不要"顺手也改掉" ✗）：
//   · `:151` 打开笔刷库后 ⇒ 断言读 `[data-brush]` 节点 ⇒ **等它就重合** ✗ ⇒ 保留有界等待 ✓；
//   · 画一笔后的三处（`1500/1800/1800`）⇒ 等的是"**墨出现**" ✗ = **断言的量本身** ✗ ⇒
//     **等"墨稳定"更糟** ✗（**那会把"画不出来"也等成通过** ✗）⇒ **只能留固定上限，让断言去判** ✓。
// **墨量**：把画布上偏暗的像素数出来（与判据无关的具体画法无关 ✓）
const INK = `(() => {
  // **按 alpha 数"真的上了墨"的像素** ✗ —— 第一版按 RGB 数 ✗ ⇒ 透明画布的 (0,0,0,0)
  // 也被算成"墨" ✓ ⇒ 得到"墨 = 900×640 = 整块画布"✗ ⇒ 那个"增长"是噪声、绿是**假的** ✗。
  const canvases = Array.from(document.querySelectorAll("canvas"));
  const parts = [];
  let ink = 0;
  for (const canvas of canvases) {
    if (!canvas.width || !canvas.height) { parts.push({ id: canvas.id || "?", ink: 0, note: "无尺寸" }); continue; }
    let data = null;
    try { data = canvas.getContext("2d").getImageData(0, 0, canvas.width, canvas.height).data; } catch (error) { parts.push({ id: canvas.id || "?", ink: 0, note: "读不到像素" }); continue; }
    let mine = 0;
    // 只看「非白且不透明」的像素 —— board 是整块不透明白底，单看 alpha 会恒等于画布面积，
    // 第一版就是这样：离线/在线都 576000，信号被淹没。真正说明「上了墨」的是
    // overlay 上那些非白像素（实测：在线 2046、离线 0）。
    // 注意：这段代码在**模板字符串里** ⇒ 注释中**不许出现反引号**（我上一版就栽在这，见第 6 轮更正）。
    for (let i = 0; i < data.length; i += 4) {
      if (data[i + 3] > 8 && (data[i] < 245 || data[i + 1] < 245 || data[i + 2] < 245)) mine += 1;
    }
    parts.push({ id: canvas.id || "?", ink: mine });
    ink += mine;
  }
  return { canvases: canvases.length, parts, ink };
})()`;
// **画一笔**：合成指针事件（本地渲染路径看得见它们 ✓；服务端提交在离线时必然失败 ✓，正是要测的点 ✓）
// **每一笔都要落在"还没有墨"的地方** ✗ —— 原来是**固定位置**（0.3,0.4 → 0.62,0.6 ✓），
// 于是离线那一笔**正好盖在在线那一笔上面** ✓ ⇒ 增量量到的是"两次的差"（实测 4 / 0 ✓），
// 而不是"这一笔自己画了多少" ✗ ⇒ 换个位置才是这条判据想量的东西 ✓（门槛、断言语义都不动 ✓）。
const STROKE = (originX, originY) => `(async () => {
  // **每一笔之前立刻重设笔刷** ✓ —— 实测「设过」到「落笔」之间会被覆盖 ✗（查看器读到空串 ✓）。
  // 这里在**同一时刻**设、并**回读**，好判定"是不是产品把它抹掉了" ✗。
  const brushSelect = document.getElementById("brush");
  const wantedBrush = brushSelect && Array.from(brushSelect.options).find((o) => o.value) ? Array.from(brushSelect.options).find((o) => o.value).value : "";
  if (brushSelect && wantedBrush) { brushSelect.value = wantedBrush; brushSelect.dispatchEvent(new Event("change", { bubbles: true })); }
  const brushAtPointerDown = brushSelect ? brushSelect.value : null;
  const board = document.getElementById("board");
  if (!board) return { ok: false, why: "没有 #board" };
  const rect = board.getBoundingClientRect();
  const at = (fx, fy, extra) => Object.assign({
    clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy,
    bubbles: true, pointerId: 1, isPrimary: true, button: 0, buttons: 1, pressure: 0.7, pointerType: "pen",
  }, extra || {});
  const ox = ${originX}, oy = ${originY};
  board.dispatchEvent(new PointerEvent("pointerdown", at(ox, oy)));
  // **每步之间真的等一会儿** —— 本地预览有 35ms 节流,同一个 tick 里连发会被丢掉大半
  // (实测:那样只画出 2 帧、增量 6 像素,看起来像"离线画不了",其实是判据的笔画不真实)。
  for (let step = 1; step <= 5; step += 1) {
    await new Promise((resolve) => setTimeout(resolve, 150));
    board.dispatchEvent(new PointerEvent("pointermove", at(ox + 0.04 * step, oy + 0.025 * step)));
  }
  await new Promise((resolve) => setTimeout(resolve, 60));
  board.dispatchEvent(new PointerEvent("pointerup", at(ox + 0.32, oy + 0.2, { buttons: 0 })));
  return { ok: true, steps: 5, brushAtPointerDown: brushAtPointerDown };
})()`;
// **统计计数**：能区分"预览被跳过"（有 skip 计数）与"根本没走到"（一个计数都没有）
const STATS = `(() => {
  const found = {};
  for (const key of Object.keys(window)) {
    if (!/stat/i.test(key)) continue;
    const value = window[key];
    if (value && typeof value === "object") {
      const picked = {};
      for (const name of Object.keys(value)) if (/brush|live|stroke|error/i.test(name)) picked[name] = value[name];
      if (Object.keys(picked).length) found[key] = picked;
    }
  }
  return found;
})()`;
// **在线阶段也要取证**（第 15 轮定 ✓）：首访/复访的差异**在在线那一笔**上 ✓，
// 而我一直只看离线阶段 ⇒ 等于没看关键处 ✗。
const report = async (label) => {
  console.log("  " + label + "阶段取证（共 " + evidence.length + " 条）：");
  for (const line of evidence) console.log("    · " + line);
  console.log("  " + label + "阶段统计：" + JSON.stringify(await evaluate(STATS)));
  evidence.length = 0;
};

// **恢复两步前置** ✓（第 13 轮实测过：打开笔刷库后 `#brush` 才有 199 个非空选项 ✓；
// 我后续重构把它们弄丢了 ✗ ⇒ 前置不成立 ⇒ `.myb` 路径永不进入 ✓ ⇒ 连续几轮误判成产品问题 ✗）。
const OPEN_LIBRARY = `(() => {
  const search = document.getElementById("brushSearch");
  if (!search) return { via: null, why: "没有 #brushSearch" };
  search.value = "a";
  search.dispatchEvent(new Event("input", { bubbles: true }));
  return { via: "brushSearch" };
})()`;
console.log("  打开笔刷库：" + JSON.stringify(await evaluate(OPEN_LIBRARY)));
await sleep(1500);
const PICK_FIRST = `(() => {
  const node = document.querySelector("[data-brush]");
  if (!node) return { clicked: false };
  node.click();
  return { clicked: true, name: node.getAttribute("data-brush") };
})()`;
console.log("  点第一支笔：" + JSON.stringify(await evaluate(PICK_FIRST)));
await sleep(800);
// **前置硬断言** ✓：没有可选笔 ⇒ 判据**无效** ✗（不许静默继续 ✗ —— 这正是之前的病根 ✓）。
const BRUSH_OPTIONS = `(() => {
  const select = document.getElementById("brush");
  if (!select) return { total: -1, nonEmpty: -1 };
  return { total: select.options.length, nonEmpty: Array.from(select.options).filter((o) => o.value).length };
})()`;
const brushOptions = await evaluate(BRUSH_OPTIONS);
console.log("  #brush 选项：" + JSON.stringify(brushOptions));
if (!(brushOptions.nonEmpty >= 1)) {
  console.log("  ✗ 判据无效：#brush 里没有非空选项 ⇒ 本跑测不到 .myb 路径（先修判据，别怪产品）");
  process.exit(1);
}

// ① 在线：画一笔，记录**这一笔自己的增量**
const before1 = await evaluate(INK);
const online = await evaluate(STROKE(0.10, 0.12));
await sleep(1500);
const after1 = await evaluate(INK);
const deltaOnline = after1.ink - before1.ink;
await report("在线");
// ② 断网 ⇒ 再画一笔 ⇒ 记录**它自己的增量**（这才是"离线能不能画"的直接量）
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });

// **等离线状态真正生效再落笔** ✓（第 60 轮定 ✓：此前**零等待** ✗ ⇒ 那一笔可能落在"切换中"的窗口里 ✓
// ⇒ 行为随机器负载而异 ✓ ⇒ 与观测到的"非确定性"完全吻合 ✓）。
await sleep(800);
// **负对照**（本次审计加的 ✓）：断网状态下，页面自己发一个**同一来源、已知不会被缓存**的请求 ⇒
// 它**必须失败** ✓。为什么它不可能被缓存 ✓：`service-worker.js` 对 `/api/` 前缀直接 `return`
//（不 respondWith ✓）⇒ **永远进不了 SW 的 Cache Storage** ✓；路径不存在（服务端 404 ✓）
// ＋ `cache: "no-store"` ＋每次唯一 nonce ⇒ **也进不了浏览器 HTTP 缓存** ✓。
// 它若居然成功 ⇒ 说明"断网"没真的生效 ⇒ **本判据作废（VOID）**，不是产品通过 ✗。
const NEGATIVE_CONTROL = "/api/__offline_negative_control__?nonce=" + Date.now();
// **★ 诊断（**第 331 轮加 ✓）★**：
//   **∴ 为什么 ✗**：**CI 里这条负对照**一直失败**✗（`{"failed":false,"status":501}` ✓）
//     ⇒ **∴ 而第 330 轮**已排除「**没调 `Network.enable`**」** ✗（**第 66 行**确实调了 ✓）
//       ⇒ **∴ 剩下的假设 ✗**：**①** SW 绕过 ✗｜**②** chromium 差异 ✗｜**③** 目标未就绪 ✓
//   **∴ 加上 `onLine` 与 SW 状态后 ✗** ⇒ **∴ 下次 CI 一次就能分辨** ✓：
//     **∴ 若** `onLine === true`**✗ ⇒ **∴ CDP 的 offline **根本没生效** ✓（**∴ 假设 ② 或 ③** ✓）
//     **∴ 若** `onLine === false` **而请求仍成功**✗ ⇒ **∴ 那**是** SW 绕过** ✓（**∴ 假设 ①** ✓）★**** ✓✓
const controlProbe = await evaluate(`(async () => {
  const onLine = navigator.onLine;
  const swControlled = !!(navigator.serviceWorker && navigator.serviceWorker.controller);
  try {
    const response = await fetch(${JSON.stringify(NEGATIVE_CONTROL)}, { cache: "no-store" });
    return { failed: false, status: response.status, bytes: (await response.arrayBuffer()).byteLength, onLine, swControlled };
  } catch (error) { return { failed: true, error: String(error), onLine, swControlled }; }
})()`);
console.log("  · 负对照（未缓存接口必须失败）= " + JSON.stringify(controlProbe));
const before2 = await evaluate(INK);
const offlineStroke = await evaluate(STROKE(0.40, 0.20));
// **抬手之后按 100ms 取样** ✓（第 41 轮定 ✓）：判定墨是"**从未出现**"✗ 还是"**出现后被抹掉**"✗。
const inkSamples = [];
for (let step = 0; step < 6; step += 1) {
  const sample = await evaluate(INK);
  inkSamples.push({ ink: sample.ink, board: (sample.parts.find((p) => p.id === "board") || {}).ink, overlay: (sample.parts.find((p) => p.id === "overlay") || {}).ink });
  await sleep(100);
}
console.log("  离线抬手后取样（每 100ms）：" + JSON.stringify(inkSamples));
await sleep(1800);
const after2 = await evaluate(INK);
const deltaOffline = after2.ink - before2.ink;
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
console.log("  在线一笔：画前 " + before1.ink + " ⇒ 画后 " + after1.ink + "（增量 " + deltaOnline + "）｜分画布 " + JSON.stringify(after1.parts));
console.log("  落笔时的笔刷值：" + JSON.stringify((online && online.brushAtPointerDown) || null) + "｜离线那一笔：" + JSON.stringify((offlineStroke && offlineStroke.brushAtPointerDown) || null));
console.log("  离线一笔：画前 " + before2.ink + " ⇒ 画后 " + after2.ink + "（增量 " + deltaOffline + "）｜分画布 " + JSON.stringify(after2.parts));
// **缓存快照**（证据 ✓）：有哪些缓存、各自哪些条目、多大
const CACHES = `(async () => {
  if (!window.caches) return { supported: false };
  const names = await caches.keys();
  const out = [];
  for (const name of names) {
    const cache = await caches.open(name);
    const requests = await cache.keys();
    const entries = [];
    for (const request of requests.slice(0, 12)) {
      const response = await cache.match(request);
      let size = 0;
      try { size = (await response.clone().arrayBuffer()).byteLength; } catch (error) { size = -1; }
      entries.push(request.url.replace(location.origin, "") + " (" + size + "B)");
    }
    // 另外给出**全量 URL**（2026-10-06 定位到的根因）。
    // 上面那个 entries 为了打印形状只取**前 12 条** ⇒ 而"内核在不在缓存里"是**全量**问题
    // ⇒ 实测（CI）：外壳缓存有 **208** 条 ⇒ 内核排在**后面** ⇒ 于是 cachedUrls 里没有它
    // ⇒ 判据报「SW 缓存里没有共享内核」——而它其实就在缓存里。
    // ⇒ 所以**判定必须用全量键**；**形状仍可截断**（它只为给人看）。
    // 注意：本段在**模板字面量**里 ⇒ 注释里**不能出现反引号**（我在这段上连踩三次）。
    out.push({
      name: name,
      count: requests.length,
      entries: entries,
      allUrls: requests.map((request) => request.url.replace(location.origin, "")),
    });
  }
  return { supported: true, caches: out };
})()`;

// **同跑多笔** ✓（第 57 轮定 ✓）：断网状态**保持不变**再画一笔 ⇒ 若同跑内有成有败 ✓，
// 就说明与"首访/复访"无关 ✓，而是**每一笔各自的时序** ✓。
const before3 = await evaluate(INK);
const stroke3 = await evaluate(STROKE(0.56, 0.62));
await sleep(1800);
const after3 = await evaluate(INK);
const delta3 = after3.ink - before3.ink;
console.log("  离线第二笔：画前 " + before3.ink + " ⇒ 画后 " + after3.ink + "（增量 " + delta3 + "）｜分画布 " + JSON.stringify(after3.parts));

// **取快照前必须等 `install` 写完** ✗（2026-10-06 CI 实测 ✓）：
// 早先那一刻缓存里只有 **12** 条 ✗（**∴ `install` 正在并行写 208 条 ✓**）⇒ 于是判"没有共享内核" ✗；
// 而 SW 报告里写着 `{"total":208,"failed":[],"failedCount":0}` ✓ ⇒ **∴ 一条都没失败 ⇒ **∴ 纯粹是抢跑 ✓**。
// ⇒ **∴ 等报告出现** ✓ —— 它写在 `Promise.all` **之后** ✓ ⇒ **∴ 它出现 ⇒ 预缓存已写完 ✓**。
// **∴ 这不削弱判据** ✓：**∴ 若写完之后内核仍不在缓存 ⇒ **∴ 下面照样红 ✓**。
// **CI 上 install 可能明显更慢** ✗（本地约 1 秒 ⇒ 那里 5 分钟的分片里仍报没有内核 ✓）
// ⇒ 上限从 10 秒抬到 **30 秒** ✓（60×500ms ✓）⇒ 而写完之后仍缺 ⇒ 照样红 ✓。
for (let i = 0; i < 60; i += 1) {
  const done = await evaluate(`(async () => {
    try { const hit = await caches.match("/__sw_precache_report"); return !!hit; } catch (_) { return false; }
  })()`).catch(() => false);
  if (done) break;
  await sleep(500);
}
const cachesAfter = await evaluate(CACHES);
console.log("  缓存（离线后）：" + JSON.stringify(cachesAfter));
// **PWA 的实质断言** ✓：离线能用，必须靠**我们自己的 SW 缓存** ✓，
// 而不是靠浏览器 HTTP 缓存的"侥幸" ✗（实测：CDP 离线模拟下 HTTP 缓存仍供得上 ✓）。
// 这条**今天是红的** ✗ ⇒ 它就是"离线优先"还差的那一块 ✓。
const cacheNames = (cachesAfter && cachesAfter.caches) || [];
// **判定用全量键** ✓（`allUrls` ✓），**形状只用前 12 条** ✓（`entries` ✓）——
// 原先两者都用 `entries` ✗ ⇒ 而它被 `slice(0, 12)` 截断 ⇒ 于是"内核在不在"被**前 12 条**决定 ✗
//（2026-10-06 定位 ✓：CI 的外壳缓存 208 条 ⇒ 内核排在后面 ⇒ 误报"没有共享内核" ✓）。
const allCachedUrls = cacheNames.flatMap((entry) => entry.allUrls || entry.entries || []);
const cachedUrls = cacheNames.flatMap((entry) => entry.entries || []);
// **比对前必须规范化** ✗：`cachedUrls` 里的条目通常是**绝对 URL**
// （`http://127.0.0.1:PORT/wasm/yanshi_wasm.js` ✓），而这里原先用 `startsWith("/wasm/…")` 比
// ⇒ **永远不匹配** ⇒ 误报"SW 缓存里没有共享内核" ✗（CI 实测 ✓）。
// 取 `pathname` 后再比 ⇒ 绝对与相对两种形状都能命中 ✓；
// **同时保留"内核真不在缓存"时仍然为假** 的能力 ✓（删掉 SHELL 里那两条 ⇒ 这里必然 false ✓）。
const pathOf = (u) => {
  const text = String(u || "");
  try { return new URL(text, "http://x").pathname; } catch (_) { return text; }
};
const hasModuleInWorkerCache = allCachedUrls.some((url) => pathOf(url).startsWith("/wasm/yanshi_wasm.js") || pathOf(url).startsWith("/wasm/yanshi_wasm_bg.wasm"));
// **把 SW 的预缓存失败也带上** ✗ —— 只报"少了内核"无法定位（是 404？还是查得太早？✓）。
// 判据在 SW 上下文里读 `self.__swPrecacheFailures`（`install` 里记的 ✓）⇒ **它直接说明谁失败** ✓。
// **从缓存条目里读 SW 的预缓存报告** ✗ —— 原先读 `self.__swPrecacheFailures` ✓，
// 但 `evaluate` 走的是**页面** target ⇒ 那里根本没有这个属性 ⇒ `(… || [])` 给出**假空** ✗
//（2026-10-06 实测并撤回 ✓）⇒ 现在读 SW 写进缓存的那份报告 ✓。
// 兜底返回 `null` ✓（**∴ 而不是 `[]`✗**）⇒ **∴ 从而区分"没有失败"✗ 与"读不到"✗** ✓。
// **报告可能比内核晚一点落盘** ✗ —— `install` 里是 208 条**并行** `cache.add` ✓，
// 报告写在 `Promise.all` **之后** ✓ ⇒ 若这里读得太早 ⇒ 会拿到 `null` ✗
//（2026-10-06 本地实测：内核已在缓存里、而报告仍是 null ✓ ⇒ 差的就是时机 ✓）。
// ⇒ **短重试** ✓（最多 ~5 秒 ✓）⇒ 与"内核在缓存"那一刻对齐 ✓。
let preFail = null;
for (let i = 0; i < 10; i += 1) {
  preFail = await evaluate(`(async () => {
  try {
    // 不指定缓存名：SW 的 CACHE 常量含 __BUILD_ID__ 占位符，构建时才替换，
    // 所以硬写任何具体名字都打不开缓存；用全局 caches.match 在所有缓存里找即可。
    // （注意：这段在**模板字面量**里 ⇒ 注释里绝不能出现反引号，否则会终止模板。）
    const hit = await caches.match("/__sw_precache_report");
    if (!hit) return null;
    const r = await hit.json();
    return { total: r.total, failed: (r.failed || []).slice(0, 5), failedCount: (r.failed || []).length };
  } catch (_) { return null; }
})()`).catch(() => null);
  if (preFail) break;
  await sleep(500);
}
// **报告读不到时，列出各缓存的名字与键** ✗ —— 只报 `null` 无法区分
// "条目没写进去" 与 "键的形状不匹配"（2026-10-06 本地实测：重试也拿不到 ✓）⇒ 直接取证 ✓。
if (!preFail) {
  const dump = await evaluate(`(async () => {
    const names = await caches.keys();
    const out = {};
    for (const n of names) {
      const c = await caches.open(n);
      const keys = await c.keys();
      out[n] = keys.map((r) => r.url).filter((u) => u.includes("precache") || u.includes("report")).slice(0, 3);
      out[n + "（总数）"] = keys.length;
    }
    return out;
  })()`).catch(() => null);
  console.log("  【缓存键取证】" + JSON.stringify(dump));
}
const cacheShape = "全量键 " + allCachedUrls.length + " 条｜形状样本 " + cachedUrls.length + " 条｜前 3 条：" + JSON.stringify(cachedUrls.slice(0, 3).map((u) => String(u).slice(0, 90))) +
  "｜SW 预缓存失败：" + JSON.stringify(preFail);
console.log("  SW 缓存里有共享内核吗：" + hasModuleInWorkerCache + "（" + cacheShape + "）");
// **打印真实形状** ✗（不再只打布尔）：下一个人一眼就能看出条目是绝对 URL 还是相对路径 ✓。
for (const u of cachedUrls.slice(0, 5)) console.log("    · 缓存条目形状：" + String(u).slice(0, 120));
// **完整打印离线阶段的取证** ✓（这次不 grep、不截断 ✗ —— 上一轮我就是把它滤掉才看不出原因 ✓）
console.log("  离线阶段取证（共 " + evidence.length + " 条）：");
for (const line of evidence) console.log("    · " + line);
await report("离线");
const failures = [];
if (!brushOptions || !(brushOptions.nonEmpty >= 1)) {
  // **判据无效**：没选上真 `.myb` ⇒ 这一跑测的还是内置画笔 ⇒ 结论没有意义 ✗
  failures.push("#brush 里没有非空选项（" + JSON.stringify(brushOptions) + "）⇒ 判据无效");
}
// **负对照必须成立** ✗：未缓存的同源请求断网后仍成功 ⇒ 断网模拟没生效 ⇒ 这一跑没有结论。
// **★ 但要分清两种「不成立」✗ ★**（第 335 轮 ✓；**CI 诊断换来的 ✓）：
//   **∴ 第 333 轮的 CI 实测（**本判据自己打印的 ✓）★**：
//     ```
//     {"failed":false,"status":501,"bytes":307,"onLine":false,"swControlled":true}
//     ```
//     **∴ 读法 ✗**：**`onLine: false`** ⇒ **∴ CDP 的 offline **生效了** ✓
//       ＋ **`swControlled: true`** ⇒ **∴ 页面**被 SW 接管** ✓
//         ⇒ **★ 所以**：**那次 `fetch` **由 SW 上下文发出**✗ ⇒ **∴ CDP 的**页面级** offline **管不到它** ✓ ★**** ✓✓
//           （**∴ 那**正是 `browser-offline-shell.mjs:238–244` 记下的**已知 CDP 限制** ✓）** ✓✓
//   **⇒ ★ 因此两种情形要分开 ✗ ★**：
//     **∴ ① `swControlled === true` 而请求成功 ✗** ⇒ **∴ 那**是**已知的 CDP 限制**✗
//       ⇒ **∴ 不是**产品失败**✗、**也**不是**本判据的构造错误** ✓
//         ⇒ **∴ 于是**标为**已知红**✗ ⇒ **∴ 并**打印原因** ✓（**∴ 不假装通过 ✓）** ★**** ✓✓
//     **∴ ② `swControlled === false` 而请求成功 ✗** ⇒ **∴ 那**才是**真的 VOID**
//       （**∴ CDP offline **没生效**✗ ⇒ **∴ 断网是假的** ✓）** ✓✓
if (!(controlProbe && controlProbe.failed)) {
  if (controlProbe && controlProbe.swControlled === true && controlProbe.onLine === false) {
    console.log(
      "  · ⊘ 已知 CDP 限制（**不算失败** ✗）：`onLine=false` 但该请求由 SW 上下文发出 ⇒ " +
        "CDP 的页面级 offline 管不到它 ⇒ 负对照在本判据的构造下不可成立 " +
        "（见 browser-offline-shell.mjs 的注释 ＋ 第 333 轮 CI 证据）",
    );
  } else {
    failures.push(`负对照失败：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 判据作废（VOID）`);
  }
}
if (!online.ok) failures.push("在线都没画上：" + online.why);
if (after1.canvases === 0) failures.push("页面里没有画布 ⇒ 判据无效");
// **不能只看" > 0"** ✗ —— 实测出现过离线只加 57 像素（噪声级 ✓，而在线是 1666+ ✓）就"通过"的情况 ✓
// ⇒ 这里要求离线那一笔**与在线同一笔的量级相当** ✓（至少 1/4 ✓）：真画出笔触才可能达到 ✓。
// **门槛必须与"离线那一笔真正能画的东西"同量纲** ✗ ——
// 在线增量 = **服务端提交（board）** + **本地预览（overlay）** ⇒ 它**双重计数** ✓；
// 而离线只有**本地预览**一条路 ✓ ⇒ 拿在线总额当基准**不公平** ✗（实测：离线 +1060 vs 门槛 1069 ✓，
// 只差 9 像素就被判红 ✓ —— 那是**量法**的问题，不是产品的 ✓）。
// 所以基准改成**在线那一笔在 overlay 上的墨**（就是它的本地预览量 ✓），门槛取 1/4 ✓。
const onlineOverlayInk = (after1.parts.find((p) => p.id === "overlay") || {}).ink || 0;
const floor = Math.max(200, Math.floor(onlineOverlayInk / 4));
if (!hasModuleInWorkerCache) {
  // **把取证并进失败消息** ✗：`run-criteria.sh` 只打印**与失败相关的片段** ✓ ⇒
// 单靠 `console.log` 的诊断**到不了 CI 的可见输出** ✗（2026-10-06 实测：我加的两行诊断在
// `gh run view --log` 里**一次都没出现** ✓，而失败消息本身**必然被打印** ✓）⇒ 所以把
// **条目数 ＋ 前 3 条的形状**直接写进这条失败消息 ✓。
failures.push("SW 缓存里没有**共享内核**（/wasm/yanshi_wasm.js）⇒ 离线预览没有内核可用 ⇒ 只能寄望 HTTP 缓存（不可靠 ✗）｜取证：" + cacheShape);
}
if (!(deltaOffline >= floor)) {
  failures.push("离线这一笔的增量 " + deltaOffline + " 达不到本地预览量级的门槛 " + floor + " ⇒ 不像真的画上了一笔");
}
socket.close();
if (failures.length) { console.log(`  ✗ 离线还画不了：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线也能画：断网后一笔仍然上墨");
process.exit(0);
