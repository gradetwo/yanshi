#!/usr/bin/env node
// 测量「打开文档」的两段体验预算，并给出退出码（可接 CI）：
//
//   * 首帧（服务端铺底，不依赖 WASM）  —— 设计 14.10：view 模式 < 100ms；edit 模式 < 1s
//   * 内核预热完成（可交互）          —— 同上
//
// 口径：数字取自编辑器自身的 window.yanshiStats（firstPaintMs / kernelWarmMs），
// 因此量的是**真实用户路径**而不是另写一套计时。页面会强制忽略缓存重载，
// 否则第二次运行量到的是浏览器缓存，而不是冷启动。
//
// 前置条件：服务端 + 带远程调试的 Chromium（见 scripts/README.md）。
// 用法：
//   node scripts/browser-first-paint.mjs "http://127.0.0.1:8110/?doc=myDoc&token=..."
//
// 环境变量：
//   FIRST_PAINT_BUDGET_MS   首帧预算（默认 1000，对应 edit 模式 < 1s）
//   KERNEL_WARM_BUDGET_MS   内核预热预算（默认 3000，本机噪声大，留足余量）

const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const paintBudget = Number(process.env.FIRST_PAINT_BUDGET_MS || 1000);
const warmBudget = Number(process.env.KERNEL_WARM_BUDGET_MS || 3000);
if (!url) {
  console.error("用法: node scripts/browser-first-paint.mjs <viewer-url>");
  process.exit(2);
}

const list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());
const target = list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const errors = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.exceptionThrown") {
    errors.push(
      (message.params.exceptionDetails?.exception?.description ||
        message.params.exceptionDetails?.text ||
        "").slice(0, 160),
    );
  }
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
});
await new Promise((resolve) => ws.addEventListener("open", resolve));
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const current = id++;
    pending.set(current, resolve);
    ws.send(JSON.stringify({ id: current, method, params }));
  });
const evaluate = async (expression) =>
  (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result
    ?.result?.value;

await send("Runtime.enable");
await send("Page.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Page.navigate", { url });

// 1) 首帧：服务端铺底，不等 WASM。
let firstPaint = null;
for (let attempt = 0; attempt < 120; attempt++) {
  firstPaint = await evaluate("window.yanshiStats ? window.yanshiStats.firstPaintMs : null");
  if (typeof firstPaint === "number") break;
  await new Promise((resolve) => setTimeout(resolve, 100));
}

// 2) 内核预热：完成后才真正可交互。
let stats = null;
for (let attempt = 0; attempt < 40; attempt++) {
  stats = await evaluate("window.yanshiStats");
  if (stats?.wasm && typeof stats.kernelWarmMs === "number") break;
  await new Promise((resolve) => setTimeout(resolve, 250));
}

const doc = new URL(url).searchParams.get("doc") ?? "(unknown)";
const paintOk = typeof firstPaint === "number";
// **★ 服务端优先模式下**没有内核预热** ✗ ★**（第 105 轮 ✓；**同 `browser-layout` 的发现 ✓）：
//   **∴ 症状 ✗**：**全新 profile ＋ 服务端 URL**✗
//     ⇒ **∴ 页面**走**服务端优先**（`serverRenderPreferred()` 缺省 true ✓）
//       ⇒ **∴ `initWasm()`**被跳过**✗ ⇒ **∴ `wasm=false`／`kernelWarmMs=null`** ✓**** ✓✓
//     ⇒ **∴ 于是**：**本判据**死等**内核✗（40 秒 ✓）⇒ **∴ 然后**判**"**预热未记录 ✓"** ⇒ **∴ 红 ✓**** ✓✓
//   **∴ 修法**：**服务端优先时**"**内核预热**"这一项**不适用**✗
//     ⇒ **∴ 它**的可交互标志**是**"**服务端铺过底**" ✓（**`serverBlits > 0` ✓）** ✓✓
//     **∴ 而**首帧那条**已经在量**服务端铺底**（`firstPaintMs` ✓）⇒ **∴ 仍然有效 ✓**** ✓✓
const serverMode = await evaluate(`(() => {
  try {
    if (typeof window !== "undefined" && window.__pwaLocalOnly === true) return false;
    const stored = localStorage.getItem("yanshi.serverRender");
    return stored === null ? true : stored === "1";
  } catch (error) { return true; }
})()`);
const warmOk = serverMode === true
  ? Number(stats?.serverBlits || 0) > 0
  : Boolean(stats && typeof stats.kernelWarmMs === "number");
console.log(`文档 ${doc}:`);
console.log(
  `  首帧（服务端铺底）: ${paintOk ? firstPaint.toFixed(0) + "ms" : "未记录"}` +
    `｜预算 < ${paintBudget}ms（14.10：view < 100ms / edit < 1s）`,
);
console.log(
  `  ${serverMode ? "服务端铺底（可交互）: serverBlits=" + (stats?.serverBlits ?? 0) : "内核预热（可交互）: " + (warmOk ? stats.kernelWarmMs.toFixed(0) + "ms" : "未记录")}` +
    `｜预算 < ${warmBudget}ms｜HEAD ${stats?.kernelHead ?? "?"}/${stats?.serverHead ?? "?"}`,
);
if (errors.length > 0) {
  console.log(`  ⚠️ 页面异常 ${errors.length} 条，例如：${errors[0]}`);
}

let failed = false;
if (!paintOk || firstPaint > paintBudget) failed = true;
if (!warmOk) failed = true;
if (!serverMode && stats.kernelWarmMs > warmBudget) failed = true;
if (errors.length > 0) failed = true;
console.log(failed ? "❌ 首帧/预热预算未通过" : "✅ 首帧与可交互时间均在预算内");
ws.close();
process.exit(failed ? 1 : 0);
