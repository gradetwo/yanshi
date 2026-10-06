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
// **另开一条连到 service worker 自己的调试目标** ✓（本次审计加的 ✓）：下面要在 SW 上下文里
// 把 `self.fetch` 换成必然失败的桩 ✓（与 `browser-offline-assets.mjs` 同一招 ✓）。
const connectTarget = async (target) => {
  const targetSocket = new WebSocket(target.webSocketDebuggerUrl);
  let targetNextId = 1;
  const targetPending = new Map();
  targetSocket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && targetPending.has(message.id)) {
      targetPending.get(message.id)(message);
      targetPending.delete(message.id);
    }
  };
  await new Promise((open) => { targetSocket.onopen = open; });
  const targetSend = (method, params) =>
    new Promise((resolve) => {
      const id = targetNextId++;
      targetPending.set(id, resolve);
      targetSocket.send(JSON.stringify({ id, method, params: params || {} }));
    });
  const targetEvaluate = async (expression) => {
    const message = await targetSend("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    const result = message.result || {};
    return result.result ? result.result.value : undefined;
  };
  return { socket: targetSocket, send: targetSend, evaluate: targetEvaluate };
};
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

// ⚠️ **CDP 的网络模拟到不了 service worker 自己的 `fetch()`** ✗（本次审计复核 ✓）：
// 页面 `navigator.onLine === false` 时，SW 里 `fetch(...)` 照样 200 ✓，
// `Network.setCacheDisabled` 与 `clearBrowserCache` 都管不住 ✓ ⇒ **只模拟页面网络时，
// 本判据可以在 SW 仍走活网（每次导航与每个资产都现取）的情况下通过** ✗ ⇒ 那样它就没有证明 SW 缓存 ✓。
// ⇒ 在 SW 上下文里把 `self.fetch` 换成必然失败的桩 ✓ —— 这正是"网络没了"时 SW 看到的那件事 ✓
// ⇒ 之后外壳只能来自 **SW 自己的 Cache Storage** ✓（`caches.match` 回落那条路才是被考的对象 ✓）。
const swTarget = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())
  .find((target) => target.type === "service_worker");
const swControl = swTarget ? await connectTarget(swTarget) : null;
if (swControl) await swControl.send("Runtime.enable");
const setServiceWorkerFetch = async (broken) => {
  if (!swControl) return false;
  const value = await swControl.evaluate(
    broken
      ? `(() => { if (!self.__yanshiRealFetch) self.__yanshiRealFetch = self.fetch;
           self.fetch = () => Promise.reject(new TypeError("yanshi-offline-stub")); return typeof self.fetch; })()`
      : `(() => { if (self.__yanshiRealFetch) self.fetch = self.__yanshiRealFetch; return typeof self.fetch; })()`,
  );
  return value === "function";
};
if (!(await setServiceWorkerFetch(true))) {
  console.error("  ✗ 切不断 service worker 的网络（拿不到 SW 调试目标 / 桩没装上）⇒ 判据无法作出离线结论");
  if (swControl) swControl.socket.close();
  socket.close();
  process.exit(1);
}

// **断网前先让内核装载完** ✗ —— 离线重载要靠**本地缓存**里的 `/api/atoms` 才建得起内核 ✓；
// 在线这趟没装完就断网 ⇒ 离线内核是 null ⇒ 后面量到的"画不出墨"是**前提不成立** ✗，
// 而不是"离线不会画" ✓（本机高负载时实测踩到过：`readyState` 完成 ≠ 内核装载完成 ✓）。
let onlineKernel = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
for (let attempt = 0; attempt < 80 && !onlineKernel; attempt += 1) {
  await sleep(250);
  onlineKernel = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
}
console.log(`  · 断网前内核 = ${onlineKernel ? JSON.stringify({ head_seq: onlineKernel.head_seq }) : "null（在线都没就绪）"}`);

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
// **负对照**（本次审计加的 ✓）：断网状态下，页面自己发一个**同一来源、已知不会被缓存**的请求 ⇒
// 它**必须失败** ✓。为什么它不可能被缓存 ✓：`service-worker.js` 对 `/api/` 前缀直接 `return`
//（不 respondWith ✓）⇒ **永远进不了 SW 的 Cache Storage** ✓；路径不存在（服务端 404 ✓）
// ＋ `cache: "no-store"` ＋每次唯一 nonce ⇒ **也进不了浏览器 HTTP 缓存** ✓。
// 它若居然成功 ⇒ 说明"断网"没真的生效 ⇒ **本判据作废（VOID）**，不是产品通过 ✗。
const NEGATIVE_CONTROL = "/api/__offline_negative_control__?nonce=" + Date.now();
const controlProbe = await evaluate(`(async () => {
  try {
    const response = await fetch(${JSON.stringify(NEGATIVE_CONTROL)}, { cache: "no-store" });
    return { failed: false, status: response.status, bytes: (await response.arrayBuffer()).byteLength };
  } catch (error) { return { failed: true, error: String(error) }; }
})()`);
console.log("  · 负对照（未缓存接口必须失败）= " + JSON.stringify(controlProbe));
// **负对照不成立 ⇒ 立刻作废（VOID）** ✗ —— 不要带着"断网是假的"这个前提继续跑。
if (!(controlProbe && controlProbe.failed)) {
  console.error(`  ⊘ 判据作废（VOID）：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 本跑没有结论`);
  await setServiceWorkerFetch(false);
  if (swControl) swControl.socket.close();
  await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  socket.close();
  process.exit(1);
}
// ⚠️ **恢复联网**（原样保留 ✓）：下面的"断网后笔刷面板"与"离线落笔"三段其实运行在**在线**网络上 ✗
// ⇒ 它们**从来不是离线断言** ✓（本次审计确认的既有问题 ✓，见报告 ✓）。
// 我实验过把这一句挪到末尾（让那三段真正离线 ✓）⇒ **判据转红** ✗：离线重载后内核起不来
// （`window.yanshi.kernelStats()` 为 `null` ✓；SW 缓存里 `/api/blob/` 条数为 **0** ✓，
//  而离线初始化需要它 ⇒ 只能靠 SW **现取活网** ✓ —— 这正是"网络其实没断"的证据 ✓）。
// ⇒ 这是**真发现**，不在这里掩盖 ✓：本次审计**只**让 SW 缓存那条断言（离线重载）真的离线 ✓。
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
// `failures` 必须在**用到它的检查之前**声明（第 1081 轮：原来在后面 ⇒ 失败分支抛
// `ReferenceError: Cannot access 'failures' before initialization` ⇒ 既没消息也没数字 ✗）。
const failures = [];

console.log(`  · 点开面板后 = ${JSON.stringify(afterOpen)}`);

// **离线这一趟必须先把内核等出来** ✗ —— 落笔那一段的本地渲染／本地落笔**都以内核为前提** ✓
//（内置画笔走 `pendingStroke` ✓、`.myb` 画笔走 `paintBrushOffline` ✓）。
// 实测（本机高负载时）：`readyState` 完成 ≠ 内核装载完成 ✓ ⇒ 断网后内核还是 `null` ✓
// ⇒ 报出来的是"离线画不出墨" ✗ —— 那是**量错对象** ✓，不是产品不会画 ✗。
// ⇒ 有界等内核就绪 ✓；等不到就**如实报"离线没有内核"** ✓（那才是真的缺口 ✓）。
let kernelState = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
for (let attempt = 0; attempt < 80 && !kernelState; attempt += 1) {
  await sleep(250);
  kernelState = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
}
console.log(`  · 断网后内核 = ${kernelState ? JSON.stringify({ head_seq: kernelState.head_seq, blobs: kernelState.blobs }) : "null（没就绪）"}`);
if (!kernelState) failures.push("断网后本地内核没有就绪（kernelStats() 为 null）⇒ 离线落笔没有内核可用");

// (A)⑥ 第二层：离线画一笔要有墨。用真实指针事件驱动页面自己的落笔路径，前后比画布**非背景**像素数。
//
// ⚠️ **原来数的是"不透明像素"（`alpha > 8`）** ✗ —— 画布底色本来就整块不透明 ✓
// ⇒ 实测**永远是 76800 => 76800** ✓ ⇒ 那条阈值**永远不可能满足** ✗（第 1125 轮记的 76800 就是这个 ✓）。
// 现在数**非背景（非纯白）像素** ✓：底色是白的 ✓ ⇒ 一笔黑墨必然让它上涨 ✓，阈值才有意义 ✓。
const canvasSig = `(() => {
  const c = document.getElementById("board");
  if (!c) return { error: "no-canvas" };
  const g = c.getContext("2d");
  if (!g) return { error: "no-2d-context" };
  const d = g.getImageData(0, 0, c.width, c.height).data;
  let ink = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (d[i + 3] > 8 && !(d[i] === 255 && d[i + 1] === 255 && d[i + 2] === 255)) ink += 1;
  }
  return { ink: ink, w: c.width, h: c.height };
})()`;
const inkBefore = await evaluate(canvasSig);
const strokeResult = await evaluate(`(async () => {
  // 先选笔刷工具（页面有多个 pointerdown 监听；工具不是 brush 时落笔会被当成别的操作）
  const brushBtn = document.querySelector('[data-tool="brush"]');
  if (brushBtn) { brushBtn.click(); await new Promise((r) => setTimeout(r, 300)); }
  // 先把"按住空格 = 临时手形"这个状态清掉（第 1125 轮定的判据侧根因之一）：
  // wantsPanEvent 是 "event.button === 1 || state.tool === \"pan\" || spaceHeld"，
  // 而判据发的是 button: 0、工具是 brush ⇒ 只剩 spaceHeld。
  // 它若为 true：落笔处理器第一行就 return ⇒ points 不启动、dragging 不变
  // ⇒ 事件被平移那条吃掉（与当时"points 仍 0"的读数吻合）。
  // 发一个 keyup(Space) 把状态显式归零（不靠"之前没人按过空格"这种巧合）。
  window.dispatchEvent(new KeyboardEvent("keyup", { code: "Space", key: " ", bubbles: true }));
  await new Promise((r) => setTimeout(r, 50));
  const c = document.getElementById("board");
  if (!c) return "no-canvas";
  const r = c.getBoundingClientRect();
  const o = (x, y) => ({ bubbles: true, cancelable: true, pointerId: 1, pointerType: "mouse",
                         isPrimary: true, button: 0, buttons: 1, clientX: x, clientY: y });
  const x0 = r.left + r.width * 0.35, y0 = r.top + r.height * 0.35;
  c.dispatchEvent(new PointerEvent("pointerdown", o(x0, y0)));
  await new Promise((res) => setTimeout(res, 30));
  // 落笔必须真的进了笔画状态机 —— 只看像素分不清"事件被吞"与"墨来自别处"
  //（判据用 window.yanshi.state()：它是函数，读属性会永远拿到 null —— 第 1121 轮的坑）。
  const atDown = window.yanshi.state();
  const pointsAtDown = atDown.points;
  const draggingAtDown = !!atDown.dragging;
  for (let i = 1; i <= 12; i += 1) {
    c.dispatchEvent(new PointerEvent("pointermove", o(x0 + i * 6, y0 + i * 4)));
    await new Promise((res) => setTimeout(res, 25));
  }
  const atMove = window.yanshi.state();
  c.dispatchEvent(new PointerEvent("pointerup", Object.assign(o(x0 + 72, y0 + 48), { buttons: 0 })));
  await new Promise((res) => setTimeout(res, 1200));
  return { result: "dispatched", tool: atDown.tool, pointsAtDown: pointsAtDown,
           draggingAtDown: draggingAtDown, pointsAtMove: atMove.points };
})()`);
// **有界等出墨** ✓（等待条件与断言条件对齐 ✓ —— 第 1116 轮的规矩 ✓）：提交是异步的 ✓，
// 固定睡一次会把"还没画完"读成"画不出来" ✗。
let inkAfter = await evaluate(canvasSig);
for (let i = 0; i < 32 && !(inkAfter && inkBefore && inkAfter.ink > inkBefore.ink); i += 1) {
  await sleep(250);
  inkAfter = await evaluate(canvasSig);
}
console.log(`  · 离线落笔 = ${JSON.stringify(strokeResult)} ｜ 非背景像素 ${inkBefore && inkBefore.ink} => ${inkAfter && inkAfter.ink}`);
// ① **事件确实进了笔画状态机** ✓（`points` 从 0 变成 > 0 ✓、`dragging` 为真 ✓）。
if (!strokeResult || typeof strokeResult !== "object" || strokeResult.pointsAtDown <= 0 || !strokeResult.draggingAtDown) {
  failures.push(`落笔事件没有进入笔画状态机（tool=${strokeResult && strokeResult.tool}｜points@down=${strokeResult && strokeResult.pointsAtDown}｜dragging@down=${strokeResult && strokeResult.draggingAtDown}）⇒ 事件被平移那条吃掉了`);
} else if (!(strokeResult.pointsAtMove > strokeResult.pointsAtDown)) {
  failures.push(`pointermove 没有累积笔画点（${strokeResult.pointsAtDown} => ${strokeResult.pointsAtMove}）`);
}
// ② **画布上真的多了墨** ✓。
if (!(inkAfter && inkBefore && inkAfter.ink > inkBefore.ink)) {
  failures.push(`离线画不出墨：非背景像素 ${inkBefore && inkBefore.ink} => ${inkAfter && inkAfter.ink}`);
}
// 判据对象必须是"打开面板之后"的选项数：面板**懒加载**（不点开就不填充）**是设计**。
const panelOptions = afterOpen ? afterOpen.options : -1;
// 三个分支：0 次进入 ⇒ 上游抛了；进过但没出来 ⇒ 中途抛了；进出一致却没选项 ⇒ 渲染环节的问题。
if (panelOptions < 2) {
  const c = refreshCount || { entered: 0, exited: 0 };
  if (c.entered === 0) console.log("  ↳ 诊断：refreshBrushOptions 从未被进入 ⇒ 上游（setupBrushLibrary）失败");
  else if (c.exited < c.entered) console.log("  ↳ 诊断：进入了但没返回 ⇒ 清空选项后、写入前失败");
  else console.log("  ↳ 诊断：进出正常但选项没进 DOM ⇒ 查 renderBrushLibrary/分组逻辑");
  failures.push(`断网后笔刷面板没有选项（options=${panelOptions}）⇒ 选不到笔 ⇒ 离线画不了`);
}

if (ready !== "active") failures.push(`Service Worker 未激活（${ready}）`);
if (!rendered.board) failures.push("离线重载后画布不存在");
if (!rendered.hasShellText && !rendered.title) failures.push("离线重载后页面没有内容");
// **负对照必须成立** ✗：未缓存的同源请求断网后仍成功 ⇒ 断网模拟没生效 ⇒ 这一跑没有结论。
if (!(controlProbe && controlProbe.failed)) {
  failures.push(`负对照失败：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 判据作废（VOID）`);
}
// **收尾：把 SW 的网络恢复** ✓（页面网络已在上面恢复 ✓；别把这个浏览器实例弄成半残 ✓）。
await setServiceWorkerFetch(false);
if (swControl) swControl.socket.close();
// **必须显式退出** ✗ —— 第一版成功时"自然走到结尾" ✗，而 WebSocket 让事件循环不退出 ✓
// ⇒ 外层 `timeout` 把它当超时（exit 124 ✓）⇒ **红绿分不开** ✗（这是判据的致命问题 ✓）。
socket.close();
// **阈值必须 > 1** —— 面板**永远保留 1 个内置笔选项**（主脚本里"只留第一个内置画笔选项"），
// 所以 `< 1` 这条**永远不会触发**（第一次实跑就暴露了：它是"永远绿"的判据）。
// 有离线回退时选项来自随包清单（约 24 个）⇒ 阈值取 2 即可分辨"只有内置那一项"与"清单回来了"。

if (failures.length) { console.log(`  ✗ 离线外壳未达成：${failures.join("；")}`); process.exit(1); }
console.log("  ✓ 离线外壳达成：断网后页面仍能打开并渲染");
process.exit(0);
