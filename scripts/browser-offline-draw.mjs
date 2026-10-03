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
const STROKE = `(() => {
  const board = document.getElementById("board");
  if (!board) return { ok: false, why: "没有 #board" };
  const rect = board.getBoundingClientRect();
  const at = (fx, fy, extra) => Object.assign({
    clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy,
    bubbles: true, pointerId: 1, isPrimary: true, button: 0, buttons: 1, pressure: 0.7, pointerType: "pen",
  }, extra || {});
  board.dispatchEvent(new PointerEvent("pointerdown", at(0.3, 0.4)));
  for (let step = 1; step <= 6; step += 1) {
    board.dispatchEvent(new PointerEvent("pointermove", at(0.3 + 0.05 * step, 0.4 + 0.03 * step)));
  }
  board.dispatchEvent(new PointerEvent("pointerup", at(0.6, 0.58, { buttons: 0 })));
  return { ok: true, brush: (document.getElementById("brush") || {}).value || "" };
})()`;
// **显式选定一支真笔刷**（消除"首访用默认笔、复访恢复上次笔刷"这个混淆变量 ✓）：
// 从 select 里挑**第一个非空值**（= 一支真 `.myb`）✓，并派发 change ✓ —— 不写死任何具体笔名 ✗。
const PICK = `(() => {
  const select = document.getElementById("brush");
  if (!select) return { picked: null, why: "没有 #brush" };
  const option = Array.from(select.options).find((item) => item.value);
  if (!option) return { picked: null, why: "没有非空笔刷选项", count: select.options.length };
  select.value = option.value;
  select.dispatchEvent(new Event("change", { bubbles: true }));
  return { picked: select.value, count: select.options.length };
})()`;
const picked = await evaluate(PICK);
console.log("  选定笔刷：" + JSON.stringify(picked));
// **界面状态快照**（证据 ✓）：localStorage 里存了什么（首访/复访的差别最可能在这里）
const STATE = `(() => {
  const out = {};
  try { for (let i = 0; i < localStorage.length; i += 1) { const k = localStorage.key(i); out[k] = String(localStorage.getItem(k)).slice(0, 60); } } catch (error) { out.error = String(error); }
  return { tool: (document.getElementById("tool") || {}).value || null, stored: out };
})()`;
console.log("  界面状态：" + JSON.stringify(await evaluate(STATE)));
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
await send("Runtime.enable"); await send("Log.enable");
const cachesBefore = await evaluate(CACHES);
console.log("  缓存（画前）：" + JSON.stringify(cachesBefore));
// ① 在线：画一笔，记录**这一笔自己的增量**
const before1 = await evaluate(INK);
const online = await evaluate(STROKE);
await sleep(1500);
const after1 = await evaluate(INK);
const deltaOnline = after1.ink - before1.ink;
// ② 断网 ⇒ 再画一笔 ⇒ 记录**它自己的增量**（这才是"离线能不能画"的直接量）
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
const before2 = await evaluate(INK);
const offlineStroke = await evaluate(STROKE);
await sleep(1800);
const after2 = await evaluate(INK);
const deltaOffline = after2.ink - before2.ink;
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
console.log("  在线一笔：画前 " + before1.ink + " ⇒ 画后 " + after1.ink + "（增量 " + deltaOnline + "）｜分画布 " + JSON.stringify(after1.parts));
console.log("  离线一笔：画前 " + before2.ink + " ⇒ 画后 " + after2.ink + "（增量 " + deltaOffline + "）｜分画布 " + JSON.stringify(after2.parts));
const cachesAfter = await evaluate(CACHES);
console.log("  缓存（离线后）：" + JSON.stringify(cachesAfter));
if (evidence.length) { console.log("  取证（console/日志/失败请求）："); for (const line of evidence.slice(-14)) console.log("    · " + line); }
const failures = [];
if (!online.ok) failures.push("在线都没画上：" + online.why);
if (after1.canvases === 0) failures.push("页面里没有画布 ⇒ 判据无效");
// **不能只看" > 0"** ✗ —— 实测出现过离线只加 57 像素（噪声级 ✓，而在线是 1666+ ✓）就"通过"的情况 ✓
// ⇒ 这里要求离线那一笔**与在线同一笔的量级相当** ✓（至少 1/4 ✓）：真画出笔触才可能达到 ✓。
const floor = Math.max(200, Math.floor(Math.abs(deltaOnline) / 4));
if (!(deltaOffline >= floor)) {
  failures.push("离线这一笔的增量 " + deltaOffline + " 达不到同一笔的量级门槛 " + floor + " ⇒ 不像真的画上了一笔");
}
socket.close();
if (failures.length) { console.log(`  ✗ 离线还画不了：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线也能画：断网后一笔仍然上墨");
process.exit(0);
