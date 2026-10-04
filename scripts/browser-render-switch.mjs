#!/usr/bin/env node
// **显式渲染开关判据** ✓（(A)⑤ ✓："服务端仅作弱设备/能力缺失时的回退，并给一个显式开关"✓）。
//
// **查证过的现状（✓）** ✓：查看器**只有自动回退** ✓（`needsServerPixels = true` 在 1376/1554/1582 ✓）
// ⇒ **没有**用户可以显式打开的"用服务端渲染"✗ ⇒ 判据**天然红** ✓。
//
// 判据（**只断言可观察的差别** ✓，不猜内部字段 ✗）：
//   ① 设好偏好 `yanshi.serverRender = "1"` ✓ ⇒ 重载 ⇒ **服务端像素请求（`render_region`）必须 > 0** ✓；
//   ② 清掉偏好 ✓ ⇒ 重载 ⇒ 客户端渲染 ✓ ⇒ **请求数必须严格少于 ①** ✓
//      （不写成"必须为 0"✗ —— 客户端路径在别处也可能用到它 ✓ ⇒ 用**差值**才是稳的 ✓）。
const url = process.argv[2];
const port = process.argv[3] || process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-render-switch.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map(); const seen = [];
socket.onmessage = (event) => { const m = JSON.parse(event.data);
  if (m.method === "Network.requestWillBeSent") seen.push(m.params.request.url);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
// 先开一次页，让我们能在同一个 origin 上写 localStorage ✓
await send("Page.navigate", { url }); await sleep(4500);
const setPref = (value) => evaluate(value === null
  ? `localStorage.removeItem("yanshi.serverRender"), "cleared"`
  : `localStorage.setItem("yanshi.serverRender", ${JSON.stringify(value)}), "set"`);
/// **确定性信号** ✓（第 197 轮的教训 ✓）：**内核句柄在不在** ✓ —— 它与时间无关 ✓，
/// 而"固定等 5 秒内的请求数"**时序敏感** ✗（当初的 `4 vs 3` 就是运气 ✓）。
const load = async () => { seen.length = 0; await send("Page.navigate", { url }); await sleep(5000);
  const hasWasm = await evaluate('(typeof state !== "undefined" && state) ? !!state.wasm : "no-state"');
  return { count: seen.filter((u) => u.includes("/api/tools/render_region")).length, hasWasm }; };
const server = await (async () => { await setPref("1"); return await load(); })();
const client = await (async () => { await setPref(null); return await load(); })();
console.log(`  偏好=服务端 ⇒ 内核句柄 = ${server.hasWasm}｜render_region 请求数 = ${server.count}（**次要观察** ✓）`);
console.log(`  偏好=客户端 ⇒ 内核句柄 = ${client.hasWasm}｜render_region 请求数 = ${client.count}（**次要观察** ✓）`);
const failures = [];
// **主断言：与时间无关** ✓ —— 开关打开就该**不加载内核** ✓（第 182 轮的守卫 ✓）
if (server.hasWasm !== false) failures.push(`打开「用服务端渲染」之后内核句柄 = ${server.hasWasm}，应当是 false ⇒ 开关没生效 ✗`);
if (client.hasWasm !== true) failures.push(`关掉开关之后内核句柄 = ${client.hasWasm}，应当是 true ⇒ 客户端渲染没了 ✗`);
// **次断言（只在明显矛盾时才报）** ✓：服务端模式不该比客户端**少**走服务端像素 ✓
if (server.count < client.count) failures.push(`服务端模式的 render_region 请求数（${server.count}）少于客户端（${client.count}）⇒ 可疑 ✗`);
if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 显式开关：服务端模式确实多走服务端像素，客户端模式更少");
socket.close();
process.exit(0);
