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
// **只等"页面就绪"** ✓（第 892 轮 ✓）：原来固定睡 3 秒 ✗ ⇒ 纯猜测 ✗ ——
// 而紧接着的 `navigator.serviceWorker.ready` **自己就有 12 秒超时的等待** ✓
// ⇒ 这里只需等 `readyState` 完成 ✓（**不与任何断言字段重合** ✓）。
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate('document.readyState === "complete"')) break; } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
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
// **断网前先取证**：SW 是否接管、缓存里到底有什么（页面的"新旧"由这里判定）。
const cacheReport = await evaluate(`(async () => {
  const reg = await navigator.serviceWorker.getRegistration();
  const keys = await caches.keys();
  const out = { controller: !!navigator.serviceWorker.controller, keys, caches: {} };
  for (const k of keys) {
    const c = await caches.open(k);
    const reqs = await c.keys();
    const urls = reqs.map((r) => new URL(r.url).pathname);
    let rootLen = -1;
    const root = await c.match("/");
    if (root) rootLen = (await root.text()).length;
    out.caches[k] = { count: urls.length, hasAppJs: urls.includes("/viewer-app.js"),
                      hasCss: urls.includes("/viewer.css"), rootBytes: rootLen,
                      scope: reg ? reg.scope : null };
  }
  return out;
})()`);
console.log(`  · 断网前缓存报告 = ${JSON.stringify(cacheReport)}`);

await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
await send("Page.reload", { ignoreCache: false });
// ⚠️ **这一处只等 `readyState` ＋ 有界沉降** ✗（第 892 轮 ✓，**不要"顺手改成等 board"** ✗）：
// 下面断言读的正是 `board` / `title` / `innerText` ✗ ⇒ **等它们就等于让断言永不失败** ✗
//（**第 887 轮我犯过这个错** ✓）⇒ 所以只等"文档完成" ✓ ＋ 800ms 让 SW 接管 ✓，
// **"离线时页面能不能渲染"始终只由断言判** ✓ ✓。
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate('document.readyState === "complete"')) { await sleep(800); break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
const rendered = await evaluate(`(() => ({
  board: !!document.getElementById("board"),
  title: document.title || "",
  hasShellText: document.body ? document.body.innerText.length > 20 : false,
}))()`);
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
console.log(`  ② 离线重载后：board=${rendered.board}｜标题=${rendered.title}｜正文长度>20=${rendered.hasShellText}`);
// (A)② 的端到端判据 —— 断网后笔刷面板必须仍有选项。
// 面板清单走服务端工具 list_assets ⇒ 若没有离线回退，下拉是空的 ⇒ 选不到笔 ⇒ 画不出来。
const brushOptions = await evaluate(`(() => {
  const select = document.getElementById("brush");
  if (!select) return -1;
  return select.options.length;
})()`);

const refreshCount = await evaluate(`(() => {
  const c = window.__brushRefresh;
  return c ? { entered: c.entered, exited: c.exited } : null;
})()`);
console.log(`  · 断网后笔刷面板选项数 = ${brushOptions}`);
console.log(`  · refreshBrushOptions 计数 = ${JSON.stringify(refreshCount)}`);
// **段是否执行**：`setupBrushLibrary` 是主脚本里的顶层函数声明 ⇒
// 若页面里 `typeof` 是 "undefined" ⇒ **那一段整体没执行**（上游抛了）；
// 若是 "function" ⇒ 段执行了 ⇒ 问题在函数内部。
const scopeProbe = await evaluate(`(() => ({
  setup: typeof setupBrushLibrary,
  refresh: typeof refreshBrushOptions,
  counter: typeof window.__brushRefresh,
  wasm: typeof window.yanshi,
}))()`);
console.log(`  · 作用域探测 = ${JSON.stringify(scopeProbe)}`);
const marks = await evaluate(`(() => window.__appMarks || null)()`);
console.log(`  · 脚本执行标记 = ${JSON.stringify(marks)}`);
// **先打开笔刷库面板再断言**（第 1076 轮）：`refreshBrushOptions()` 可能只在面板打开时调用，
// 此前判据从未打开面板 ⇒ 选项停在"内置 1 项" ⇒ 可能把**设计**当成了缺陷。
await evaluate(`(() => {
  const btn = document.getElementById("brushLibraryOpen");
  if (btn) btn.click();
  return true;
})()`);
await sleep(1500);
const afterOpen = await evaluate(`(() => {
  const select = document.getElementById("brush");
  const c = window.__brushRefresh;
  return { options: select ? select.options.length : -1, counters: c ? { entered: c.entered, exited: c.exited } : null };
})()`);
console.log(`  · 点开面板后 = ${JSON.stringify(afterOpen)}`);
// 判据对象必须是"打开面板之后"的选项数：面板**懒加载**（不点开就不填充）**是设计**。
const panelOptions = afterOpen ? afterOpen.options : -1;
// 三个分支：0 次进入 ⇒ 上游抛了；进过但没出来 ⇒ 中途抛了；进出一致却没选项 ⇒ 渲染环节的问题。
if (panelOptions < 2) {
  const c = refreshCount || { entered: 0, exited: 0 };
  if (c.entered === 0) console.log("  ↳ 诊断：refreshBrushOptions 从未被进入 ⇒ 上游（setupBrushLibrary）失败");
  else if (c.exited < c.entered) console.log("  ↳ 诊断：进入了但没返回 ⇒ 清空选项后、写入前失败");
  else console.log("  ↳ 诊断：进出正常但选项没进 DOM ⇒ 查 renderBrushLibrary/分组逻辑");
}

const failures = [];
if (ready !== "active") failures.push(`Service Worker 未激活（${ready}）`);
if (!rendered.board) failures.push("离线重载后画布不存在");
if (!rendered.hasShellText && !rendered.title) failures.push("离线重载后页面没有内容");
// **必须显式退出** ✗ —— 第一版成功时"自然走到结尾" ✗，而 WebSocket 让事件循环不退出 ✓
// ⇒ 外层 `timeout` 把它当超时（exit 124 ✓）⇒ **红绿分不开** ✗（这是判据的致命问题 ✓）。
socket.close();
// **阈值必须 > 1** —— 面板**永远保留 1 个内置笔选项**（主脚本里"只留第一个内置画笔选项"），
// 所以 `< 1` 这条**永远不会触发**（第一次实跑就暴露了：它是"永远绿"的判据）。
// 有离线回退时选项来自随包清单（约 24 个）⇒ 阈值取 2 即可分辨"只有内置那一项"与"清单回来了"。
if (panelOptions < 2) {
  failures.push(`断网后笔刷面板没有选项（options=${panelOptions}）⇒ 选不到笔 ⇒ 离线画不了`);
}

if (failures.length) { console.log(`  ✗ 离线外壳未达成：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线外壳达成：断网后页面仍能打开并渲染");
process.exit(0);
