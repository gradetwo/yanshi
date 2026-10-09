#!/usr/bin/env node
// **★ 线上 UI 级实测 ✓ ★**（用户要求：「**你自己发布然后实际测试，画画和加载都没问题算修好**」✓）
//
// **∴ 为什么必须有它 ✗**：**已有的判据都停在**接口层**✗**：
//   * `browser-pwa-local.mjs` 用 `fetch` 直接打 `/api/tools/...` ✓ ⇒ **∴ 它**绕过**UI ✓**；
//   * 于是**"**点了画笔、在画布上拖一下，**到底**发生了什么**✗**"**从未被测 ✓**
//     ⇒ **∴ 用户**四次真机复验**都画不了画**✗，**而**判据**全绿 ✓**** ✓✓
//   ⇒ **∴ 本脚本**用**真鼠标事件**（CDP `Input.dispatchMouseEvent` ✓）在 `#board` 上**画一笔** ✓，
//     并**同时**收**控制台错误／未处理拒绝／4xx-5xx** ✓ ⇒ **∴ 一次**给出"**能不能用 ✓**"的答案 ✓**** ✓✓
//
// **∴ 用法 ✗**：`node scripts/browser-live-ui.mjs [url]`（**默认线上 ✓**）
//
// **∴ 断言（**六条 ✓）★**：
//   **① 页面加载无未捕获异常 ✗**（`Runtime.exceptionThrown` ✓，**含**未处理的 promise 拒绝 ✓）**；
//   **② 无控制台 error ✗**（`Runtime.consoleAPICalled` 的 `error` ✓）**；
//   **③ 无 4xx／5xx ✗**（`Network.responseReceived` ✓）——
//      **∴ 它**顺带**抓 `not_implemented_locally` ✗**（**501 ⇒ **∴ 并**打印**是哪条端点 ✓）**；
//   **④ 内核预热 ✗**（`window.yanshiStats.wasm === true` ✓）**；
//   **⑤ 真鼠标拖拽后画布像素**必须变化 ✗**（**∴ 这是"**画得上**"的定义 ✓）**；
//   **⑥ 无**服务端渲染**误选 ✗**（**`__pwaLocalOnly` ⇒ **∴ 内核**不得被跳过 ✓）** ✓✓
import { spawn } from "node:child_process";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const base = String(process.argv[2] || "https://yanshi-online.wangda.today").replace(/\/+$/, "");
const cdpPort = 9500 + Math.floor(Math.random() * 200);
const profile = `/tmp/live-ui-${cdpPort}`;

const chrome = spawn(process.env.CHROME_BIN || "chromium",
  ["--headless=new", `--remote-debugging-port=${cdpPort}`, "--no-sandbox", "--disable-gpu",
   "--disable-dev-shm-usage", "--window-size=1280,900", `--user-data-dir=${profile}`, "about:blank"],
  { stdio: "ignore" });

let targets = null;
for (let i = 0; i < 80; i++) {
  await sleep(300);
  try {
    targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
    if (targets && targets.length) break;
  } catch {}
}
if (!targets || !targets.length) {
  console.error("❌ chromium 未就绪 ⇒ 本次实测无效");
  chrome.kill();
  process.exit(1);
}
const page = targets.find((t) => t.type === "page") || targets[0];
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();

// **∴ 证据收集器（**全部**只记录，判定在最后 ✓）**
const consoleErrors = [];
const exceptions = [];
const badResponses = [];
const notImplemented = [];
const logEntries = [];

socket.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
    return;
  }
  const p = m.params || {};
  if (m.method === "Runtime.consoleAPICalled" && p.type === "error") {
    const text = (p.args || []).map((a) => String(a.value ?? a.description ?? a.type)).join(" ");
    consoleErrors.push(text);
  }
  if (m.method === "Runtime.exceptionThrown") {
    const d = (p.exceptionDetails || {}).exception || {};
    exceptions.push(String(d.description || (p.exceptionDetails || {}).text || "unknown"));
  }
  if (m.method === "Log.entryAdded") {
    logEntries.push(`${(p.entry || {}).level}: ${(p.entry || {}).text}`);
  }
  if (m.method === "Network.responseReceived") {
    const r = p.response || {};
    const status = r.status || 0;
    if (status >= 400) {
      badResponses.push(`${status} ${r.url}`);
      if (status === 501 || /not_implemented/i.test(r.url)) notImplemented.push(r.url);
    }
  }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((res) => {
  const id = nextId++;
  pending.set(id, res);
  socket.send(JSON.stringify({ id, method, params }));
});
const evaluate = async (expr) =>
  (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true }))
    ?.result?.result?.value;

await send("Runtime.enable");
await send("Log.enable");
await send("Page.enable");
await send("Network.enable");
await send("Page.navigate", { url: base + "/" });

// **∴ 等加载完成 ✗**（**最多 30 秒 ✓**）。
for (let i = 0; i < 100; i++) {
  await sleep(300);
  if (await evaluate('document.readyState === "complete"')) break;
}
console.log("  ① 页面加载完成 ✓ " + base);

// **∴ 等内核预热 ✗**（**用户报的"卡片全 —"正是这里 ✗**）。
let warm = false;
for (let i = 0; i < 120; i++) {
  warm = (await evaluate("!!(window.yanshiStats && window.yanshiStats.wasm === true)")) === true;
  if (warm) break;
  await sleep(500);
}
const localOnly = await evaluate("window.__pwaLocalOnly === true");
const docId = await evaluate("window.yanshiStats && window.yanshiStats.docId");
console.log(`  ② 内核预热 wasm=${warm}｜__pwaLocalOnly=${localOnly}｜doc=${docId}`);

// **∴ 画布像素采样 ✗**（**∴ 不依赖**`getImageData` 是否可用 ⇒ **∴ `toDataURL` 兜底 ✓**）。
const boardSig = () => evaluate(`(() => {
  const c = document.getElementById("board");
  if (!c) return "no-board";
  try {
    const g = c.getContext("2d");
    if (g) {
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let s = 0;
      for (let i = 0; i < d.length; i += 61) s = (s * 31 + d[i]) % 2147483647;
      return c.width + "x" + c.height + ":" + s;
    }
  } catch (e) { /* 跨源／无 2d ⇒ 走兜底 */ }
  try { const u = c.toDataURL("image/png"); return c.width + "x" + c.height + ":u" + u.length; }
  catch (e) { return "unreadable:" + String(e).slice(0, 40); }
})()`);

// **∴ 拖拽前先读一次面板 ✗** ⇒ **∴ 于是**能把"**加载阶段的未映射**"与"**落笔时的未映射**"分开 ✓。
const panelReader = `(() => { const el = document.getElementById("log"); return el ? String(el.innerText || "") : ""; })()`;
const panelBefore = String(await evaluate(panelReader) || "");
const before = await boardSig();
const boardBox = await evaluate(`(() => {
  const c = document.getElementById("board");
  if (!c) return null;
  const r = c.getBoundingClientRect();
  return { x: r.x, y: r.y, w: r.width, h: r.height };
})()`);
console.log(`  ③ 画布签名（画前）${before}｜尺寸 ${boardBox ? Math.round(boardBox.w) + "x" + Math.round(boardBox.h) : "?"}`);

// **★ 谁在那个坐标的最上层 ✗ ★**（**∴ 遮罩吃掉指针事件**是最可能的原因 ✓）：
const hitTest = await evaluate(`(() => {
  const c = document.getElementById("board");
  if (!c) return "no-board";
  const r = c.getBoundingClientRect();
  const x = Math.round(r.x + r.width * 0.5), y = Math.round(r.y + r.height * 0.5);
  const el = document.elementFromPoint(x, y);
  const stack = document.elementsFromPoint(x, y).slice(0, 6).map((n) => n.id || n.tagName);
  return JSON.stringify({ at: [x, y], top: el ? (el.id || el.tagName) : null, stack });
})()`);
console.log("  ⑧ 命中测试 " + String(hitTest));

// **★ 真鼠标拖拽 ✗ ★**（**∴ 不是**直接 fetch 接口 ✓ —— **∴ 那**正是之前漏掉的一层 ✓）。
let drew = false;
if (boardBox && boardBox.w > 4 && boardBox.h > 4) {
  const x0 = Math.round(boardBox.x + boardBox.w * 0.3);
  const y0 = Math.round(boardBox.y + boardBox.h * 0.3);
  const x1 = Math.round(boardBox.x + boardBox.w * 0.7);
  const y1 = Math.round(boardBox.y + boardBox.h * 0.7);
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x0, y: y0, button: "none", buttons: 0 });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: x0, y: y0, button: "left", buttons: 1, clickCount: 1 });
  for (let i = 1; i <= 12; i++) {
    const t = i / 12;
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: Math.round(x0 + (x1 - x0) * t),
      y: Math.round(y0 + (y1 - y0) * t),
      button: "left",
      buttons: 1,
    });
    await sleep(25);
  }
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x1, y: y1, button: "left", buttons: 0, clickCount: 1 });
  await sleep(1200);
  drew = true;
}
const after = await boardSig();
console.log(`  ④ 画布签名（画后）${after}｜已派发鼠标拖拽 ${drew}`);

// **∴ 统计 ✗**
const stats = await evaluate("JSON.stringify({applies: window.yanshiStats && window.yanshiStats.applies, lastArea: window.yanshiStats && window.yanshiStats.lastArea, warm: window.yanshiStats && window.yanshiStats.kernelWarmMs})");
console.log("  ⑤ 统计 " + String(stats));

// **★ 让应用自己的日志说话 ✗ ★**（**∴ 它**是唯一知道"**为什么没落上**"的地方 ✓）：
//   **∴ 用户的报错**（`落笔失败：unknown`／`not_implemented_locally` ✓）**正是**打在这里 ✓。
const panelAfter = String((await evaluate(panelReader)) || "");
const panelNew = panelAfter.startsWith(panelBefore) ? panelAfter.slice(panelBefore.length) : panelAfter;
console.log("  ⑥ 加载阶段日志（长度 " + panelBefore.length + "）：\n-----\n" + panelBefore.slice(-400) + "\n-----");
console.log("  ★ 拖拽之后**新增**的日志（**∴ 这条**才可能挡住画画 ✓）：\n-----\n" + String(panelNew).slice(-600) + "\n-----");
const ui = await evaluate(`JSON.stringify({
  tool: (window.state && window.state.tool) || null,
  layerId: (window.state && window.state.layerId) || null,
  layerRows: document.querySelectorAll("[data-layer-id], .layer-row, #layers li").length,
  boardSize: (() => { const c = document.getElementById("board"); return c ? c.width + "x" + c.height : null; })(),
})`);
console.log("  ⑦ 界面状态 " + String(ui));

const failures = [];
const check = (cond, msg) => { if (!cond) failures.push(msg); };
check(exceptions.length === 0, `有未捕获异常 ${exceptions.length} 条：` + exceptions.slice(0, 3).join(" ｜ "));
check(consoleErrors.length === 0, `有控制台 error ${consoleErrors.length} 条：` + consoleErrors.slice(0, 3).join(" ｜ "));
check(badResponses.length === 0, `有 4xx／5xx ${badResponses.length} 条：` + badResponses.slice(0, 4).join(" ｜ "));
check(warm === true, "内核没有预热（window.yanshiStats.wasm !== true）⇒ **∴ 画不了画**");
check(drew, "画布不可见或太小 ⇒ 没能派发鼠标拖拽");
check(before !== after, `鼠标拖拽之后画布像素**没有变化**（${before} ⇒ ${after}）⇒ **∴ 画不上**`);
check(localOnly === true, "页面没有 __pwaLocalOnly ⇒ **∴ 它可能仍在走服务端渲染**");

if (notImplemented.length) {
  console.log("  ⚠️ 未实现端点（501）：" + notImplemented.slice(0, 6).join(" ｜ "));
}
if (logEntries.length) {
  console.log("  ℹ️ 浏览器日志 " + logEntries.length + " 条，前 3 条：" + logEntries.slice(0, 3).join(" ｜ "));
}

try { socket.close(); } catch {}
chrome.kill();

if (failures.length) {
  console.error("❌ 线上 UI 实测不通过：\n  - " + failures.join("\n  - "));
  process.exit(1);
}
console.log("  ✓ 线上 UI 实测通过：加载 ✓｜内核预热 ✓｜真鼠标画得出像素 ✓｜无报错 ✓｜无 4xx/5xx ✓");
