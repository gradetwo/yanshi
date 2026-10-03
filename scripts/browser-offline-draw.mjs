#!/usr/bin/env node
// **离线画一笔**判据（目标 (A)③⑥）：断网后仍必须能在画布上画出墨。
// 为什么它是"离线优先"的成功定义：能"打开页面"只是外壳（已达成 ✓），
// 真正的要求是**离线下还能画**（本地渲染 + 本地状态，服务端不在也算数）。
// 用法：node scripts/browser-offline-draw.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
const port = process.argv[3] || process.env.CDP_PORT || "9333";
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
socket.addEventListener("message", (event) => {
  let m = null;
  try { m = JSON.parse(event.data); } catch (error) { return; }
  if (m.method === "Runtime.consoleAPICalled") {
    const text = (m.params.args || []).map((a) => a.value === undefined ? (a.description || a.type) : String(a.value)).join(" ");
    evidence.push("console." + m.params.type + ": " + text.slice(0, 200));
  } else if (m.method === "Log.entryAdded") {
    evidence.push("log." + m.params.entry.level + ": " + String(m.params.entry.text).slice(0, 200));
  } else if (m.method === "Network.loadingFailed") {
    evidence.push("request-failed: " + String(m.params.errorText) + " (type " + m.params.type + ")");
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
await sleep(3000);
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
const STROKE = `(async () => {
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
  board.dispatchEvent(new PointerEvent("pointerdown", at(0.3, 0.4)));
  // **每步之间真的等一会儿** —— 本地预览有 35ms 节流,同一个 tick 里连发会被丢掉大半
  // (实测:那样只画出 2 帧、增量 6 像素,看起来像"离线画不了",其实是判据的笔画不真实)。
  for (let step = 1; step <= 8; step += 1) {
    await new Promise((resolve) => setTimeout(resolve, 60));
    board.dispatchEvent(new PointerEvent("pointermove", at(0.3 + 0.04 * step, 0.4 + 0.025 * step)));
  }
  await new Promise((resolve) => setTimeout(resolve, 60));
  board.dispatchEvent(new PointerEvent("pointerup", at(0.62, 0.6, { buttons: 0 })));
  return { ok: true, steps: 8, brushAtPointerDown: brushAtPointerDown };
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
const online = await evaluate(STROKE);
await sleep(1500);
const after1 = await evaluate(INK);
const deltaOnline = after1.ink - before1.ink;
await report("在线");
// ② 断网 ⇒ 再画一笔 ⇒ 记录**它自己的增量**（这才是"离线能不能画"的直接量）
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
const before2 = await evaluate(INK);
const offlineStroke = await evaluate(STROKE);
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
    out.push({ name: name, count: requests.length, entries: entries });
  }
  return { supported: true, caches: out };
})()`;

const cachesAfter = await evaluate(CACHES);
console.log("  缓存（离线后）：" + JSON.stringify(cachesAfter));
// **PWA 的实质断言** ✓：离线能用，必须靠**我们自己的 SW 缓存** ✓，
// 而不是靠浏览器 HTTP 缓存的"侥幸" ✗（实测：CDP 离线模拟下 HTTP 缓存仍供得上 ✓）。
// 这条**今天是红的** ✗ ⇒ 它就是"离线优先"还差的那一块 ✓。
const cacheNames = (cachesAfter && cachesAfter.caches) || [];
const cachedUrls = cacheNames.flatMap((entry) => entry.entries || []);
const hasModuleInWorkerCache = cachedUrls.some((url) => url.startsWith("/brush-module.wasm"));
console.log("  SW 缓存里有门面吗：" + hasModuleInWorkerCache + "（条目 " + cachedUrls.length + " 条）");
// **完整打印离线阶段的取证** ✓（这次不 grep、不截断 ✗ —— 上一轮我就是把它滤掉才看不出原因 ✓）
console.log("  离线阶段取证（共 " + evidence.length + " 条）：");
for (const line of evidence) console.log("    · " + line);
await report("离线");
const failures = [];
if (!brushOptions || !(brushOptions.nonEmpty >= 1)) {
  // **判据无效**：没选上真 `.myb` ⇒ 这一跑测的还是内置画笔 ⇒ 结论没有意义 ✗
  failures.push("#brush 里没有非空选项（" + JSON.stringify(brushOptions) + "）⇒ 判据无效");
}
if (!online.ok) failures.push("在线都没画上：" + online.why);
if (after1.canvases === 0) failures.push("页面里没有画布 ⇒ 判据无效");
// **不能只看" > 0"** ✗ —— 实测出现过离线只加 57 像素（噪声级 ✓，而在线是 1666+ ✓）就"通过"的情况 ✓
// ⇒ 这里要求离线那一笔**与在线同一笔的量级相当** ✓（至少 1/4 ✓）：真画出笔触才可能达到 ✓。
const floor = Math.max(200, Math.floor(Math.abs(deltaOnline) / 4));
if (!hasModuleInWorkerCache) {
  failures.push("SW 缓存里没有 /brush-module.wasm ⇒ 离线能力其实依赖浏览器 HTTP 缓存");
}
if (!(deltaOffline >= floor)) {
  failures.push("离线这一笔的增量 " + deltaOffline + " 达不到同一笔的量级门槛 " + floor + " ⇒ 不像真的画上了一笔");
}
socket.close();
if (failures.length) { console.log(`  ✗ 离线还画不了：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线也能画：断网后一笔仍然上墨");
process.exit(0);
