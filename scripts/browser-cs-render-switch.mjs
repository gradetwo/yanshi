//! **★ C/S 渲染开关的**真浏览器**判据 ✗ ★**（第 68 轮 ✓；
//! **`yanshi-cs-local-render-修复报告` 的**未覆盖项**换来的 ✓**）。
//!
//! **∴ 报告自己写的未覆盖项 ✗ ★**：
//!   **"**未覆盖（需真实浏览器 ✓）：**本地模式下落一笔的**像素验证**、
//!    **IndexedDB 与服务端的**状态分叉**行为 ✓
//!    ⇒ **∴ 建议**在合入后用**真实浏览器走一遍开关两种模式 ✓"**
//!   ⇒ **∴ 所以**：**这一条判据**就是**那个建议的落地 ✓**** ✓✓
//!
//! **∴ 它验什么（**四条 ✓）★**：
//!   **∴ ①** **默认（**服务器渲染 ✓）**✗：**真鼠标画一笔**
//!     ⇒ **∴ 服务端的原子数**必须**增加 ✓（**∴ 证明**确实走服务端 ✓）** ✓✓
//!   **∴ ②** **关掉开关（**`localStorage` ✓）**✗ ⇒ **重载** ⇒ **再画一笔**
//!     ⇒ **∴ 服务端的原子数**必须**不增 ✓（**∴ 证明**确实走本地 ✓）** ✓✓
//!   **∴ ③** **两种模式下**画布签名**都必须变 ✓**（**∴ 否则**"**画得上 ✓"就没了 ✓）** ✓✓
//!   **∴ ④** **两本账 ✗**（**用户第 8 条 ✓）**：**每笔**记**墙钟 ＋ **主线程 `TaskDuration`**
//!     ⇒ **∴ 两种模式**都报出来 ⇒ **∴ 且**如实说明**哪一本更好 ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/browser-cs-render-switch.mjs [--keep]`

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const __tempPaths = [];
function trackTemp(path) { __tempPaths.push(path); return path; }
function __cleanupTemp() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let left = 0;
    for (const path of __tempPaths) {
      try { rmSync(path, { recursive: true, force: true }); } catch { left += 1; }
    }
    if (left === 0) return;
    try { Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 200); } catch { /* 忽略 */ }
  }
}
process.on("exit", __cleanupTemp);
for (const __signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.on(__signal, () => { __cleanupTemp(); process.exit(1); });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const binary = ["target/debug/yanshi-serve", "target/release/yanshi-serve"]
  .map((path) => { try { return { path, m: statSync(path).mtimeMs }; } catch { return null; } })
  .filter(Boolean).sort((a, b) => b.m - a.m).map((e) => e.path)[0];
if (!binary) {
  console.error("✗ 找不到 yanshi-serve ⇒ 先构建");
  process.exit(2);
}

const failures = [];
const check = (ok, label, detail) => {
  console.log("  " + (ok ? "✓" : "✗") + " " + label + (detail ? "（" + detail + "）" : ""));
  if (!ok) failures.push(label + (detail ? "：" + detail : ""));
};

const SERVER_PORT = 8771;
const root = trackTemp(mkdtempSync(join(tmpdir(), "cs-switch-")));
const server = spawn(binary, ["--root", root, "--bind", `127.0.0.1:${SERVER_PORT}`], { stdio: "ignore" });
const base = `http://127.0.0.1:${SERVER_PORT}`;
for (let i = 0; i < 60; i += 1) {
  try { if ((await fetch(`${base}/health`)).ok) break; } catch { /* 还没起来 */ }
  await sleep(250);
}

const cdpPort = 9400 + Math.floor(Math.random() * 90);
const profile = trackTemp(`/tmp/cs-switch-chrome-${cdpPort}`);
const chrome = spawn(process.env.CHROME_BIN || "chromium", [
  "--headless=new", `--remote-debugging-port=${cdpPort}`, "--no-sandbox",
  "--disable-dev-shm-usage", "--window-size=1280,900", `--user-data-dir=${profile}`, "about:blank",
], { stdio: "ignore" });

let targets = null;
for (let i = 0; i < 80; i += 1) {
  await sleep(300);
  try {
    targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json();
    if (targets && targets.length) break;
  } catch { /* 还没起来 */ }
}
if (!targets || !targets.length) {
  console.error("❌ chromium 未就绪 ⇒ 本次实测无效");
  chrome.kill(); server.kill();
  process.exit(2);
}
const page = targets.find((t) => t.type === "page") || targets[0];
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    const resolve = pending.get(message.id);
    pending.delete(message.id);
    resolve(message.result || message.error || {});
  }
});
await new Promise((resolve) => socket.addEventListener("open", resolve));
const send = (method, params) => {
  const id = nextId++;
  socket.send(JSON.stringify({ id, method, params: params || {} }));
  return new Promise((resolve) => pending.set(id, resolve));
};
const evaluate = async (expression) => {
  const result = await send("Runtime.evaluate", {
    expression, returnByValue: true, awaitPromise: true,
  });
  if (result.exceptionDetails) {
    return `THREW:${result.exceptionDetails.exception?.description || result.exceptionDetails.text}`;
  }
  return result.result ? result.result.value : undefined;
};
const taskDuration = async () => {
  const metrics = await send("Performance.getMetrics");
  for (const item of metrics.metrics || []) if (item.name === "TaskDuration") return item.value;
  return null;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Performance.enable");

// **★ C/S 前端必须**带 doc ＋ token**打开 ✗ ★**（第 68 轮 ✓；**∴ 我**踩到了 ✓）：
//   **∴ 我**原来用 `base + "/"`**✗ ⇒ **∴ 于是** `window.yanshiStats.docId/token` **是 null** ✗
//     ⇒ **∴ 任何**带凭证的接口**都失败**✗（**∴ `get_log`**返回 not ok ⇒ **∴ 我**的计数**恒 −2** ✓）
//       ⇒ **★ 于是**：**"**服务端原子是否增加 ✓"**这条断言**完全没有在测东西** ✗ ★**** ✓✓
//   **∴ 修法**：**先**建文档**✗ ⇒ **∴ 取** token**✗ ⇒ **∴ 用**带参数的 URL 打开 ✓**** ✓✓
const DOC = "crit_cs_switch";
const created = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: 512, height: 512 }),
})).json();
const TOKEN = created.token;
if (!TOKEN) {
  console.error("✗ 建文档失败 ⇒ " + JSON.stringify(created).slice(0, 160));
  chrome.kill(); server.kill();
  process.exit(2);
}
const PAGE_URL = `${base}/?doc=${encodeURIComponent(DOC)}&token=${encodeURIComponent(TOKEN)}`;
console.log(`  页面 URL：${PAGE_URL.replace(TOKEN, "<token>")}`);

/** **∴ 载入页面并等它就绪 ✗**（**∴ 并**绕开 SW 缓存 ✓）** ✓✓ */
async function load() {
  await send("Page.navigate", { url: "about:blank" });
  await evaluate(`(async () => {
    if (navigator.serviceWorker) {
      for (const reg of await navigator.serviceWorker.getRegistrations()) await reg.unregister();
    }
    if (window.caches) { for (const key of await caches.keys()) await caches.delete(key); }
  })()`);
  await send("Page.navigate", { url: PAGE_URL });
  for (let i = 0; i < 120; i += 1) {
    await sleep(250);
    const ready = await evaluate(
      "document.readyState === 'complete' && !!document.getElementById('board')");
    if (ready === true) break;
  }
  // **∴ 等内核 ✗**（**∴ 服务器渲染模式下**它是可选 ✓ ⇒ **∴ 不**强求 ✓）** ✓✓
  await sleep(1200);
}

/** **∴ 服务端的原子数 ✗**（**∴ 用**权威接口 ✓）** ✓✓ */
async function serverAtomCount() {
  // **★ 必须走**页面自己的 `get_log`** ✗ ★**（第 68 轮 ✓；**∴ 我**第一版踩到了 ✓）：
  //   **∴ 我**原来直接 `fetch("/api/atoms")`**✗** ⇒ **∴ 它**需要 `doc` 与 `token`**✗
  //     ⇒ **∴ 于是**返回 **-1** ⇒ **∴ 两条断言**都废了 ✓（**∴ 一条**假红 ＋ **一条**假绿 ✓）** ✓✓
  //   **∴ 修法**：**用**页面里已注入的**凭证**✗（**`window.yanshiStats` ✓）
  //     ＋ **调**权威的 `get_log`**✗ ⇒ **∴ 于是**数的是**服务端真实状态** ✓**** ✓✓
  const raw = await evaluate(`(async () => {
    const stats = window.yanshiStats || {};
    const url = "/api/tools/get_log?doc=" + encodeURIComponent(stats.docId || "")
      + "&token=" + encodeURIComponent(stats.token || "");
    try {
      const res = await fetch(url, { method: "POST",
        headers: { "content-type": "application/json" }, body: JSON.stringify({}) });
      const body = await res.json();
      if (!body || body.ok !== true) return -2;
      return Array.isArray(body.atoms) ? body.atoms.length : -3;
    } catch (error) { return -4; }
  })()`);
  return typeof raw === "number" ? raw : -5;
}

/** **∴ 真鼠标画一笔 ✗**（**∴ 并**量**墙钟 ＋ TaskDuration ✓）** ✓✓ */
async function drawStroke() {
  const boxRaw = await evaluate(`(() => { const b = document.getElementById("board");
    if (!b) return null; const r = b.getBoundingClientRect();
    return JSON.stringify({ x: r.x, y: r.y, w: r.width, h: r.height }); })()`);
  let box = null;
  try { box = JSON.parse(boxRaw); } catch { /* 没有画布 */ }
  if (!box) return null;
  const before = await evaluate(`(() => { const b = document.getElementById("board");
    const c = document.createElement("canvas"); c.width = b.width; c.height = b.height;
    return 0; })()`);
  const signature = async () => {
    const raw = await evaluate(`(() => { const b = document.getElementById("board");
      const c = document.createElement("canvas"); c.width = b.width; c.height = b.height;
      const g = c.getContext("2d"); g.drawImage(b, 0, 0);
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let h = 2166136261; for (let i = 0; i < d.length; i += 97) { h ^= d[i]; h = Math.imul(h, 16777619) >>> 0; }
      return c.width + "x" + c.height + ":" + h; })()`);
    return raw;
  };
  const beforeSig = await signature();
  const t0 = Date.now();
  const task0 = await taskDuration();
  const x0 = box.x + box.w * 0.3;
  const y0 = box.y + box.h * 0.4;
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x0, y: y0, button: "none", buttons: 0 });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: x0, y: y0, button: "left", buttons: 1, clickCount: 1 });
  for (let i = 1; i <= 8; i += 1) {
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved", x: x0 + i * 12, y: y0 + i * 6, button: "left", buttons: 1,
    });
    await sleep(60);
  }
  await send("Input.dispatchMouseEvent", {
    type: "mouseReleased", x: x0 + 96, y: y0 + 48, button: "left", buttons: 0, clickCount: 1,
  });
  await sleep(1500);
  const wallMs = Date.now() - t0;
  const task1 = await taskDuration();
  const afterSig = await signature();
  return {
    wallMs,
    taskMs: task0 !== null && task1 !== null ? (task1 - task0) * 1000 : null,
    changed: beforeSig !== afterSig,
    beforeSig, afterSig, ignored: before,
  };
}

// **★ ① 服务器渲染模式（**默认 ✓）✗ ★**
await load();
const modeA = await evaluate("localStorage.getItem('yanshi.serverRender')");
console.log(`  模式 A：localStorage 偏好 = ${modeA === null ? "（未设 ⇒ 默认服务器渲染 ✓）" : modeA}`);
const atomsBeforeA = await serverAtomCount();
const strokeA = await drawStroke();
const atomsAfterA = await serverAtomCount();
console.log(`  A 服务端原子：${atomsBeforeA} ⇒ ${atomsAfterA}｜墙钟 ${strokeA?.wallMs} ms`
  + `｜TaskDuration ${strokeA?.taskMs?.toFixed(1)} ms｜画布变化 ${strokeA?.changed}`);
check(strokeA !== null && strokeA.changed === true,
  "**服务器渲染模式下**画布必须有变化（**真鼠标画得上 ✓）", String(strokeA?.changed));
check(atomsAfterA > atomsBeforeA,
  "**服务器渲染模式下**服务端原子必须增加（**证明走服务端 ✓）",
  `${atomsBeforeA} ⇒ ${atomsAfterA}`);

// **★ ② 本地渲染模式（**关掉开关 ✓）✗ ★**
await evaluate("localStorage.setItem('yanshi.serverRender', '0')");
await load();
// **∴ 本地模式**先看清内核状态 ✗（**∴ 它**是本地渲染的前提 ✓）** ✓✓
const localStats = await evaluate("JSON.stringify(window.yanshiStats || {})");
console.log(`  模式 B 内核状态：${String(localStats).slice(0, 200)}`);
// **∴ 再**等一会儿 ✗（**∴ 内核**可能在**页面就绪之后**才完成预热 ✓）** ✓✓
await sleep(2500);
const atomsBeforeB = await serverAtomCount();
const strokeB = await drawStroke();
const atomsAfterB = await serverAtomCount();
console.log(`  B 服务端原子：${atomsBeforeB} ⇒ ${atomsAfterB}｜墙钟 ${strokeB?.wallMs} ms`
  + `｜TaskDuration ${strokeB?.taskMs?.toFixed(1)} ms｜画布变化 ${strokeB?.changed}`);
check(strokeB !== null && strokeB.changed === true,
  "**本地渲染模式下**画布必须有变化（**关掉服务端也必须画得上 ✓）", String(strokeB?.changed));
check(atomsAfterB === atomsBeforeB,
  "**本地渲染模式下**服务端原子**不许增加**（**证明走本地 ✓）",
  `${atomsBeforeB} ⇒ ${atomsAfterB}`);

// **★ ④ 两本账（**用户第 8 条 ✓）✗ ★**
console.log("");
console.log("  ★ 两本账（**同一条笔画场景 ✓）★");
console.log("  | 模式 | ① 墙钟 ms | ② 主线程 TaskDuration ms | 画布变化 |");
console.log("  |---|---|---|---|");
console.log(`  | 服务器渲染 | ${strokeA?.wallMs} | ${strokeA?.taskMs?.toFixed(1)} | ${strokeA?.changed} |`);
console.log(`  | 本地渲染 | ${strokeB?.wallMs} | ${strokeB?.taskMs?.toFixed(1)} | ${strokeB?.changed} |`);
if (strokeA && strokeB && strokeA.taskMs && strokeB.taskMs) {
  const d = (1 - strokeB.taskMs / strokeA.taskMs) * 100;
  console.log(`  ∴ CPU 占用账：本地渲染的主线程时间 ${d > 0 ? "少" : "多"} ${Math.abs(d).toFixed(1)}%`);
  const w = (1 - strokeB.wallMs / strokeA.wallMs) * 100;
  console.log(`  ∴ 时间账：本地渲染 ${w > 0 ? "快" : "慢"} ${Math.abs(w).toFixed(1)}%`);
  if (d < 0 && w > 0) console.log("  ⚠️ **只有墙钟变好而 CPU 占用更差** ⇒ **∴ 按第 8 条必须明说** ✓");
}

socket.close();
chrome.kill();
server.kill();
await sleep(400);

console.log("");
if (failures.length) {
  console.error(`  结论：C/S 渲染开关**未达预期** ✗（${failures.length} 条）`);
  for (const line of failures) console.error("   - " + line);
  process.exit(1);
}
console.log("  结论：✓ 开关两种模式都成立（**默认走服务端 ✓；**关掉走本地 ✓；**两种都画得上 ✓）");
process.exit(0);
