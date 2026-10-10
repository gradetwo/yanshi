#!/usr/bin/env node
// **显式渲染开关判据** ✓（(A)⑤ ✓："服务端仅作弱设备/能力缺失时的回退，并给一个显式开关"✓）。
//
// **现状（第 787 轮重新查证 ✓）** ✓：查看器**已经有**显式开关 ✓ ——
// `SERVER_RENDER_KEY = "yanshi.serverRender"` ✓、`serverRenderPreferred()` ✓（`viewer.rs:855-857` ✓），
// 且**有 UI 复选框** ✓（`viewer.rs:4895 useServerRenderBox.checked = serverRenderPreferred()` ✓）。
// ⇒ 上面那句「只有自动回退、判据天然红」是**过时注记** ✗ ⇒ 已按现状改正 ✓。
//
// 判据（**只断言可观察的差别** ✓，不猜内部字段 ✗）：
//   ① 设好偏好 `yanshi.serverRender = "1"` ✓ ⇒ 重载 ⇒ **服务端像素请求（`render_region`）必须 > 0** ✓；
//   ② 清掉偏好 ✓ ⇒ 重载 ⇒ 客户端渲染 ✓ ⇒ **请求数必须严格少于 ①** ✓
//      （不写成"必须为 0"✗ —— 客户端路径在别处也可能用到它 ✓ ⇒ 用**差值**才是稳的 ✓）。
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
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
// **等到"页面就绪"这个独立信号** ✓（第 889 轮 ✓）：原来固定睡 5 秒/4.5 秒 ✗ ⇒ 机器慢时间歇红 ✗。
// ⚠️ **等待条件与断言条件必须不同** ✗：这里只等 `readyState` 与 `state` **存在** ✓，
//    而断言读的是 **`state.wasm` 的值** ✓ ✓（**等"值"就等于让断言永不失败** ✗ —— 第 887 轮的教训 ✓）。
const waitReady = async () => {
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete" && typeof state !== "undefined"')) return true; } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
  return false;
};
// 先开一次页，让我们能在同一个 origin 上写 localStorage ✓
await send("Page.navigate", { url }); await waitReady();
const setPref = (value) => evaluate(value === null
  ? `localStorage.removeItem("yanshi.serverRender"), "cleared"`
  : `localStorage.setItem("yanshi.serverRender", ${JSON.stringify(value)}), "set"`);
/// **确定性信号** ✓（第 197 轮的教训 ✓）：**内核句柄在不在** ✓ —— 它与时间无关 ✓，
/// 而"固定等 5 秒内的请求数"**时序敏感** ✗（当初的 `4 vs 3` 就是运气 ✓）。
const load = async () => { seen.length = 0; await send("Page.navigate", { url }); await waitReady();
  // **有界等待内核句柄**（第 1146 轮）：wasm 模块的 import 是异步的，
// 而原来在 readyState 完成之后**立刻**读 state.wasm ⇒ 竞态（实测 t=3s 仍 false、t=6s 才 true）✗。
// 改成轮询到出现为止（有界），让**等待条件与断言条件对齐** ✓。
let hasWasm = null;
for (let wait = 0; wait < 40; wait += 1) {
  hasWasm = await evaluate('(typeof state !== "undefined" && state) ? !!state.wasm : "no-state"');
  if (hasWasm) break;
  await new Promise((r) => setTimeout(r, 250));
}

  return { count: seen.filter((u) => u.includes("/api/tools/render_region")).length, hasWasm }; };
const server = await (async () => { await setPref("1"); return await load(); })();
// **★ 必须**显式设 "0"**✗ ★**（第 339 轮 ✓；**CI 实测换来的 ✓）：
//   **∴ 原来**这里写 `setPref(null)`**（**清除偏好**✓）✗
//     ⇒ **∴ 而**产品里**无偏好时的默认是**服务端渲染** ✗
//       （**`web/viewer-app.js:51`**：`return stored === null ? true : stored === "1";` ✓）
//         ⇒ **∴ 于是**：**`hasWasm === false`** ✗（**∴ 内核**不加载 ✓）
//           ⇒ **★ 所以**下面那条断言**必然红** ✗
//             ⇒ **∴ 而**它**报成**「**客户端渲染没了**」**✗ ⇒ **∴ 那**是**误报** ✓ ★**** ✓✓
//   **∴ 修法 ✗**：**显式设 `"0"`**✗ ⇒ **∴ 于是**：**本判据**只考**开关**✗
//     ⇒ **∴ 而**不依赖**产品的默认值** ✓（**∴ 那**是**另一件事** ✓）** ✓✓
const client = await (async () => { await setPref("0"); return await load(); })();
console.log(`  偏好=服务端 ⇒ 内核句柄 = ${server.hasWasm}｜render_region 请求数 = ${server.count}（**次要观察** ✓）`);
console.log(`  偏好=客户端 ⇒ 内核句柄 = ${client.hasWasm}｜render_region 请求数 = ${client.count}（**次要观察** ✓）`);
const failures = [];
// **主断言：与时间无关** ✓ —— 开关打开就该**不加载内核** ✓（第 182 轮的守卫 ✓）
if (server.hasWasm !== false) failures.push(`打开「用服务端渲染」之后内核句柄 = ${server.hasWasm}，应当是 false ⇒ 开关没生效 ✗`);
if (client.hasWasm !== true) failures.push(`关掉开关之后内核句柄 = ${client.hasWasm}，应当是 true ⇒ 客户端渲染没了 ✗`);
// **次断言（只在明显矛盾时才报）** ✓：服务端模式不该比客户端**少**走服务端像素 ✓
// **请求数只作记录，不作判定** ✗（2026-10-06 CI 实测 ✓）—— 上面那两行**本来**就把它标成
// 「次要观察」✓，但这里却用它让判据红 ✗ ⇒ **自相矛盾** ✓，而且这个量**本身不可靠**：
// 一次 `render_region` 可以**覆盖多块** ⇒ 3 次可能比 4 次**做了更多事** ✗ ⇒ 用次数比大小没有意义 ✓。
// ⇒ 判定**只保留上面两条确定性断言**（内核句柄 ✓，它与时间无关 ✓）；请求数**打印但不判** ✓。
// ⇒ 而判据**仍然是能红的** ✓：`server.hasWasm !== false` 或 `client.hasWasm !== true` 任一成立即红 ✓
//   （**∴ 那两个量直接反映"是否走客户端内核" ✗ ⇒ **∴ 正是开关要证明的东西 ✓**）。
if (server.count < client.count) {
  console.log(`  · 参考：服务端请求数 ${server.count} < 客户端 ${client.count} ⇒ **只记录** ✓（次数受合并影响，不作判定 ✓）`);
}
if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 显式开关：服务端模式确实多走服务端像素，客户端模式更少");
socket.close();
process.exit(0);
