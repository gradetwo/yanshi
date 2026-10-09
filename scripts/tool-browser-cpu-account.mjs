//! **★ 浏览器内核的 **CPU 占用账** ✗ ★**（第 48 轮 ✓；**用户第 594 轮点名的方法 ✓**）。
//!
//! **∴ 依据（**用户第 594 轮 ✓）★**：
//!   **∴ ② CPU 占用账 ✗**：**（浏览器内核 ✓）**用 CDP `Performance.getMetrics` 的
//!     **`TaskDuration`** 当**代理** ✓ ⇒ **∴ 与**阶段一第 5 条记下的基线**逐项对比 ✓**** ✓✓
//!
//! **∴ 为什么需要它 ✗ ★**：**服务端的 CPU 账**可以用 `/proc` 量**✗（**见
//!   `tool-render-cost-accounts.mjs` ✓）⇒ **∴ 而**离线内核跑在**浏览器**里 ✗
//!     ⇒ **∴ 那里没有 `/proc`** ✗ ⇒ **∴ 唯一的官方代理**就是 **CDP 的 `TaskDuration`** ✓**** ✓✓
//!
//! **∴ 它量什么 ✗**：**同一段场景**（**导航 ⇒ 内核预热 ⇒ 真鼠标画一笔 ⇒ 等渲染 ✓）
//!   ⇒ **∴ 在每个阶段前后各取一次** `Performance.getMetrics`**✗
//!     ⇒ **∴ 差值**＝**该阶段的 TaskDuration（**秒 ✓）⇒ **∴ 报成毫秒 ✓**** ✓✓
//!
//! **∴ 且**（**阶段二的前置事实 ✓）★**：**如实报出**这个浏览器有没有 WebGPU**✗**：
//!   `navigator.gpu` 是否存在 ＋ `requestAdapter()` 是否给出适配器 ✓
//!   ⇒ **∴ 本机实测**是**没有**（**headless ＋ `--disable-gpu` ⇒ "No available adapters ✓）
//!     ⇒ **∴ 所以**：**本脚本**默认**去掉 `--disable-gpu`**✗，**并**把结果**如实打印** ✓**** ✓✓
//!
//! **∴ 用法 ✗**：`node scripts/tool-browser-cpu-account.mjs [base-url] [--gpu]`
//!   **∴ `--gpu`**✗：**加上 `--enable-unsafe-webgpu` 等标志再试一次 ✓**

import { spawn } from "node:child_process";

const argv = process.argv.slice(2);
const wantGpu = argv.includes("--gpu");
const base = String(argv.find((a) => !a.startsWith("--")) || "https://yanshi-online.wangda.today")
  .replace(/\/+$/, "");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const cdpPort = 9700 + Math.floor(Math.random() * 200);
const profile = `/tmp/browser-cpu-${cdpPort}`;

const flags = [
  "--headless=new",
  `--remote-debugging-port=${cdpPort}`,
  "--no-sandbox",
  "--disable-dev-shm-usage",
  "--window-size=1280,900",
  `--user-data-dir=${profile}`,
];
if (wantGpu) {
  // **∴ 请求 GPU 的一组标志 ✗**（**∴ 不保证**有适配器 ✓）
  flags.push("--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=vulkan");
} else {
  // **∴ 默认**不加 `--disable-gpu`**✗：**∴ 那**会把 WebGPU 一起关掉**✗
  //   ⇒ **∴ 于是**测不到真实情况 ✓**** ✓✓
  flags.push("--ignore-gpu-blocklist");
}
flags.push("about:blank");

const chrome = spawn(process.env.CHROME_BIN || "chromium", flags, { stdio: "ignore" });

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
  chrome.kill();
  process.exit(1);
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

/** **∴ 取一次 `Performance.getMetrics` ✗**（**∴ 返回**名字 ⇒ 秒**的表 ✓）** ✓✓ */
async function metrics() {
  const result = await send("Performance.getMetrics");
  const table = {};
  for (const item of result.metrics || []) table[item.name] = item.value;
  return table;
}

const diffMs = (before, after, name) =>
  ((after[name] ?? 0) - (before[name] ?? 0)) * 1000;

await send("Runtime.enable");
await send("Page.enable");
await send("Performance.enable");

console.log(`  目标：${base}`);
console.log(`  chromium 标志：${wantGpu ? "**请求 GPU**" : "**默认（不去掉 GPU ✓）**"}`);

// **★ 阶段 ①：导航 ＋ 加载 ✗ ★**
let before = await metrics();
await send("Page.navigate", { url: `${base}/` });
for (let i = 0; i < 120; i += 1) {
  await sleep(250);
  const ready = await send("Runtime.evaluate", {
    expression: "document.readyState === 'complete' && !!document.getElementById('board')",
    returnByValue: true,
  });
  if (ready.result && ready.result.value === true) break;
}
let after = await metrics();
const loadMs = diffMs(before, after, "TaskDuration");

// **★ 阶段 ②：等内核预热 ✗ ★**（**∴ 与阶段一第 5 条的同一场景口径一致 ✓）**
before = await metrics();
let warm = null;
for (let i = 0; i < 120; i += 1) {
  const stats = await send("Runtime.evaluate", {
    expression: "JSON.stringify(window.yanshiStats || null)",
    returnByValue: true,
  });
  try {
    warm = JSON.parse(stats.result.value);
    if (warm && warm.wasm === true) break;
  } catch { /* 还没就绪 */ }
  await sleep(250);
}
after = await metrics();
const warmMs = diffMs(before, after, "TaskDuration");

// **★ 阶段 ③：真鼠标画一笔 ✗ ★**
before = await metrics();
const box = await send("Runtime.evaluate", {
  expression: `(() => { const b = document.getElementById("board");
    if (!b) return null; const r = b.getBoundingClientRect();
    return JSON.stringify({x: r.x, y: r.y, w: r.width, h: r.height}); })()`,
  returnByValue: true,
});
let strokeMs = null;
let boxValue = null;
try { boxValue = JSON.parse(box.result.value); } catch { /* 没有画布 */ }
if (boxValue) {
  const x0 = boxValue.x + boxValue.w * 0.3;
  const y0 = boxValue.y + boxValue.h * 0.4;
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: x0, y: y0, button: "none", buttons: 0 });
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: x0, y: y0, button: "left", buttons: 1, clickCount: 1 });
  for (let i = 1; i <= 8; i += 1) {
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved", x: x0 + i * 12, y: y0 + i * 6, button: "left", buttons: 1,
    });
    await sleep(60);
  }
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: x0 + 96, y: y0 + 48, button: "left", buttons: 0, clickCount: 1 });
  await sleep(800);
}
after = await metrics();
strokeMs = diffMs(before, after, "TaskDuration");

// **★ WebGPU 前置事实（**如实 ✓）✗ ★**
const gpu = await send("Runtime.evaluate", {
  expression: `(async () => {
    const has = !!navigator.gpu;
    let adapter = null, reason = null;
    if (has) {
      try {
        const a = await navigator.gpu.requestAdapter();
        adapter = !!a;
        if (!a) reason = "requestAdapter() 返回 null";
      } catch (e) { reason = String(e && e.message || e); }
    } else { reason = "navigator.gpu 不存在"; }
    return JSON.stringify({ has, adapter, reason });
  })()`,
  returnByValue: true,
  awaitPromise: true,
});
let gpuFact = null;
try { gpuFact = JSON.parse(gpu.result.value); } catch { /* 拿不到 */ }

console.log("");
console.log("  ★ 浏览器内核的 CPU 账（**CDP `Performance.getMetrics` 的 `TaskDuration`**）★");
console.log(`  | 阶段 | TaskDuration ms | ScriptDuration ms |`);
console.log(`  |---|---|---|`);
console.log(`  | ① 导航 ＋ 加载 | ${loadMs.toFixed(1)} | ${diffMs(before, after, "ScriptDuration").toFixed(1)} |`);
console.log(`  | ② 内核预热 | ${warmMs.toFixed(1)} | — |`);
console.log(`  | ③ 真鼠标一笔 ＋ 渲染 | ${strokeMs === null ? "（没有画布）" : strokeMs.toFixed(1)} | — |`);
console.log("");
console.log(`  内在核预热状态：${warm ? JSON.stringify(warm) : "（拿不到）"}`);
console.log(`  WebGPU：navigator.gpu=${gpuFact ? gpuFact.has : "?"}｜`
  + `adapter=${gpuFact ? gpuFact.adapter : "?"}`
  + `${gpuFact && gpuFact.reason ? `｜原因=${gpuFact.reason}` : ""}`);

// **★ 如实结论 ✗ ★**：**没有适配器时**不许**说得像"**GPU 已就绪 ✓"** ✓✓
if (!gpuFact || !gpuFact.adapter) {
  console.log("  ℹ️ 这台浏览器**没有可用的 WebGPU 适配器** ⇒ **∴ 本次**只能给出**CPU 路径的账 ✓");
  console.log("     ⇒ **∴ 一旦**有适配器 ✗，**同一条命令**就会给出**可比的两本账 ✓**");
}

socket.close();
chrome.kill();
// **∴ 必须**显式退出 ✗（**∴ 第 46 轮的教训 ✓）** ✓✓
process.exit(0);
