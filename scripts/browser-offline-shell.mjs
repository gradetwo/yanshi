#!/usr/bin/env node
// **离线外壳**判据（目标 (A)① ✓）：在线加载一次 ⇒ 等 Service Worker 就绪 ⇒ **切离线** ⇒ 重载页面
// ⇒ 页面**仍必须能渲染**（否则"离线优先"就是空话 ✗）。
// 用 CDP 的 `Network.emulateNetworkConditions {offline:true}` 而不是杀服务端 ✓：
// 这样测的是**页面的离线能力**（SW 缓存）✓，而不是"服务端在不在" ✓。
// 用法：node scripts/browser-offline-shell.mjs <viewer-url> [cdpPort]
const url = process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-shell.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效（不是通过 ✗）"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expression) => (await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
await send("Page.navigate", { url });
await sleep(3000);
// ① SW 必须注册并**激活**（缓存只有在 activated 后才会被用 ✓）
const ready = await evaluate(`(async () => {
  if (!("serviceWorker" in navigator)) return "no-api";
  try {
    const registration = await Promise.race([
      navigator.serviceWorker.ready,
      new Promise((r) => setTimeout(() => r(null), 12000)),
    ]);
    return registration ? (registration.active ? "active" : "installed") : "timeout";
  } catch (error) { return "error:" + error; }
})()`);
console.log(`  ① Service Worker 状态：${ready}`);
// ② 切离线 ⇒ 重载 ⇒ 页面必须仍能渲染
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
await send("Page.reload", { ignoreCache: false });
await sleep(3500);
const rendered = await evaluate(`(() => ({
  board: !!document.getElementById("board"),
  title: document.title || "",
  hasShellText: document.body ? document.body.innerText.length > 20 : false,
}))()`);
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
console.log(`  ② 离线重载后：board=${rendered.board}｜标题=${rendered.title}｜正文长度>20=${rendered.hasShellText}`);
const failures = [];
if (ready !== "active") failures.push(`Service Worker 未激活（${ready}）`);
if (!rendered.board) failures.push("离线重载后画布不存在");
if (!rendered.hasShellText && !rendered.title) failures.push("离线重载后页面没有内容");
// **必须显式退出** ✗ —— 第一版成功时"自然走到结尾" ✗，而 WebSocket 让事件循环不退出 ✓
// ⇒ 外层 `timeout` 把它当超时（exit 124 ✓）⇒ **红绿分不开** ✗（这是判据的致命问题 ✓）。
socket.close();
if (failures.length) { console.log(`  ✗ 离线外壳未达成：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线外壳达成：断网后页面仍能打开并渲染");
process.exit(0);
