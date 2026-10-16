#!/usr/bin/env node
// **离线外壳**判据（目标 (A)① ✓）：在线加载一次 ⇒ 等 Service Worker 就绪 ⇒ **切离线** ⇒ 重载页面
// ⇒ 页面**仍必须能渲染**（否则"离线优先"就是空话 ✗）。
// 用 CDP 的 `Network.emulateNetworkConditions {offline:true}` 而不是杀服务端 ✓：
// 这样测的是**页面的离线能力**（SW 缓存）✓，而不是"服务端在不在" ✓。
// 用法：node scripts/browser-offline-shell.mjs <viewer-url> [cdpPort]
// **★ 离线判据**必须**用查看器（**静态 PWA ✓）✗ ★**（第 111 轮 ✓；**有诊断证据 ✓）：
//   **∴ 证据 ✗**：`browser-offline-shell` 报**✗**：
//     **"**断网后本地内核没有就绪（`kernelStats()` 为 null ✓）"**✗
//       ＋ **"**断网前内核 = **null**（**在线都没就绪 ✓）"** ✓**** ✓✓
//     **∴ 而** `browser-offline-journal` 报**✗**：
//       **"**WASM 内核必须就绪（**指针笔迹这条路要用它 ✓）"** ✓**** ✓✓
//   **∴ 根因 ✗**：**离线落笔**要**本地内核**✗ ⇒ **∴ 而**内核**只在
//     **`__pwaLocalOnly === true`**（**静态 PWA ✓）时**才加载 ✓**** ✓✓
//     ⇒ **∴ 在**服务端 URL 下**✗ ⇒ **∴ 页面**走**服务端优先 ⇒ **内核**不加载 ✓**** ✓✓
//   **∴ 所以 ✗**：**"**离线 ✓"**这件事**在**静态 PWA** 上**才有意义 ✓**** ✓✓
//     （**∴ 因为**静态部署**本来**就没有服务端 ✓）** ✓✓
const url = process.env.YANSHI_VIEWER_URL || process.argv[2];
// **端口从环境变量取** ✓（第 409 轮 ✓）：`run-criteria.sh` 给 browser-* 传的是
// `<viewer-url> <base> <token> <cdp-port>` ✓ ⇒ **argv[3] 是 BASE（完整 URL）** ✗ ⇒
// 原先的 `process.argv[3] || process.env.CDP_PORT` ✗ 让端口变成一个 URL ✓ ⇒
// 拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗ ⇒ **取不到调试目标** ✗。
//（全仓共 8 条这样写 ✓ —— **含"离线"全家** ✓ ⇒ 影响 A⑥ 的证据 ✓。）
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-shell.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// **∴ CDP 命令的**统一超时** ✗**（第 115 轮 ✓）
const CDP_TIMEOUT_MS = 20000;
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据无效（不是通过 ✗）"); process.exit(1); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => {
  const m = JSON.parse(event.data);
  // **★ 带 `sessionId` 的消息必须**可见** ✗ ★**（第 131 轮 ✓）
  if (m.sessionId && m.id) console.log("  · 会话回包 id=" + m.id + "（sessionId " + String(m.sessionId).slice(0, 8) + "…）");
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((open) => { socket.onopen = open; });
// **★ CDP 命令必须**有超时** ✗ ★**（第 115 轮 ✓；**本判据**挂死换来的 ✓）：
//   **∴ 原来**没有超时 ✗ ⇒ **∴ 若**一条命令**没有响应**
//     ⇒ **∴ Promise**永不解决 ⇒ **∴ 判据**挂死 ✓（实测：只打印 2 行就没有第三行 ✓）
//   **∴ 修法**：**超时**按「**无法作出结论 ✓」退出（**EXIT=2 ✓）
//     ⇒ **∴ 而**不是**挂到外层 `timeout` 才死 ✓ ⇒ **∴ 红绿**才分得开 ✓
const send = (method, params) => new Promise((resolve) => {
  const id = nextId++;
  // **★ 主连接的悬空请求**不许杀掉整个判据** ✗ ★**（第 132 轮 ✓；**id 跳号的铁证 ✓）：
  //   **∴ 证据 ✗**：**会话请求的 id 是 10 与 12**✗ ⇒ **∴ 中间**有一个 `id=11`** ✓**** ✓✓
  //     **∴ 而**12 **回了**✗ ＋ **11 **永远没回** ✓**** ✓✓
  //     **∴ 于是**：**11 **的定时器**在 20 秒后**触发**✗ ⇒ **∴ `process.exit(2)`** ✓**** ✓✓
  //     **∴ 而**报告的 `method` **是** `Runtime.evaluate`**✗
  //       ⇒ **∴ 但**它**不是**会话那条 ✓（**∴ 会话那条是 12 ✓）** ✓✓
  //   **∴ 所以 ✗**：**11** 是**页面**的一次 `Runtime.evaluate`**✗
  //     ⇒ **∴ 而**它**在**导航**之后**悬空** ✓（**∴ 旧执行上下文**被销毁 ✓）** ✓✓
  //     ⇒ **★ 那**与**判据的结论**无关**✗ ⇒ **∴ 不**该**杀掉判据 ✓ ★**** ✓✓
  //   **∴ 修法**：**主连接**超时**只**警告**✗（**∴ 并**把该请求从 `pending` **移除**✓）
  //     ⇒ **∴ 而**会话的超时**仍然**退出 ✓（**∴ 那条**真的**影响结论 ✓）** ✓✓
  const timer = setTimeout(() => {
    if (!pending.has(id)) return;
    pending.delete(id);
    console.warn("  ⚠️ CDP 命令无应答（" + CDP_TIMEOUT_MS + "ms）：" + method
      + "（很可能是导航后旧执行上下文悬空 ⇒ 与结论无关）⇒ **忽略这条** ✓");
    resolve({ id, result: undefined, __timeout: true });
  }, CDP_TIMEOUT_MS);
  pending.set(id, (value) => { clearTimeout(timer); resolve(value); });
  socket.send(JSON.stringify({ id, method, params: params || {} }));
});
// **★ `evaluate` 必须**重试** ✗ ★**（第 133 轮 ✓；**悬空请求换来的 ✓）：
//   **∴ 为什么 ✗**：**导航**（**如 `Page.reload` ✓）会**销毁旧执行上下文** ✗
//     ⇒ **∴ 于是在**那一刻发的 `Runtime.evaluate` **永远**没有回包 ✓**** ✓✓
//       ⇒ **∴ 而**本判据**在**断网重载**后**马上**要读页面状态 ✓**** ✓✓
//         ⇒ **∴ 第一次**必然**悬空 ✓**** ✓✓
//   **∴ 修法**：**`__timeout` 时**重发**✗（**最多 3 次 ✓）** ✓✓
//     ⇒ **∴ 于是**：**第二次**落在**新的**执行上下文上**✗
//       ⇒ **∴ 就能**拿到真的答案 ✓**** ✓✓
//   **∴ 且 ✗**：**三次都拿不到**时**返回** `undefined`**✗
//     ⇒ **∴ 而**调用方**必须**自己**判**"**没有答案 ✓"**✗（**∴ 见负对照那一段 ✓）** ✓✓
// **★ `evaluate` 必须**重试** ✗ ★**（第 133 轮 ✓；**悬空请求换来的 ✓）：
//   **∴ 为什么 ✗**：**导航**（**如 `Page.reload` ✓）会**销毁旧执行上下文** ✗
//     ⇒ **∴ 于是在**那一刻发的 `Runtime.evaluate` **永远**没有回包 ✓**** ✓✓
//   **∴ 修法**：**`__timeout` 时**重发**（**最多 3 次 ✓）** ✓✓
// **★★ 而**异步表达式**必须走**两段式** ✗ ★★**（第 139 轮 ✓；**探针的铁证 ✓）：
//   **∴ 陷阱 ✗**：**不带 `awaitPromise`** 时**✗
//     **∴ 一个**返回 Promise** 的表达式**✗
//       ⇒ **∴ 返回**一个**未等待的 Promise 对象** ✗**** ✓✓
//         ⇒ **∴ 而 CDP **把它**序列化成 `{}`** ✗**** ✓✓
//         ⇒ **★ 于是**：**`{}` **不是**"**空对象 ✓"**✗，**而是**"**没有等到 ✓" ✓ ★**** ✓✓
//   **∴ 而**带 `awaitPromise: true`**✗ ⇒ **∴ 在**这个目标上**挂死** ✓（**实测 20 秒**超时 ✓）** ✓✓
//   **∴ 所以**：**两者**都**不能用**✗ ⇒ **∴ 必须**两段式 ✓**** ✓✓
//     **∴ ①** **发起**：一个**同步**表达式**✗
//       ⇒ **∴ 它**把**异步结果**写到 `window.__e…`** ✓**** ✓✓
//     **∴ ②** **轮询**：**同步**读它**✗ ⇒ **∴ 直到** `done` ✓**** ✓✓
//     ⇒ **∴ 于是**：**每一步**都是**同步**✗
//       ⇒ **∴ 不**需要 `awaitPromise`**✗ ⇒ **∴ 也**不会**挂死 ✓ ★**** ✓✓
const evaluate = async (expression) => {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    // **★ 必须 `awaitPromise: true` ✗ ★**（第 402 轮 ✓；**实测根因 ✓）：
    //   **∴ 症状 ✗**：**判据的落笔求值**返回一个**真正的空对象** `{}`**✗
    //     （**∴ 调试实测：`typeof = object`｜`keys = []`｜`raw = {}` ✓）
    //       ⇒ **∴ 于是**判据**误判「**事件没进状态机**」 ✓ ★**** ✓✓
    //   **∴ 根因 ✗**：**原来的调用**只有 `returnByValue: true`**✗
    //     ⇒ **∴ 而**落笔表达式是一个 **async 函数**✗ ⇒ **∴ 它**返回 **Promise** ✓
    //       ⇒ **∴ CDP** 无法把 Promise 序列化成值**✗ ⇒ **∴ 于是**给出 `{}`** ✓ ★**** ✓✓
    //   **∴ 对照 ✗**：**我的诊断脚本**传了** `awaitPromise: true`**✗ ⇒ **∴ 全部成功** ✓
    //     （**∴ 11 步全通 ＋ 整条提交路径全通 ＋ 三连胜 ✓）** ⇒ **∴ 差别就在这一个开关** ✓ ★**** ✓✓
    //   **∴ 且 ✗**：**它对**非 Promise 的表达式**无害**✗
    //     ⇒ **∴ 所以**同一个 helper 可以两用** ✓ ★**** ✓✓
    const message = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    if (!message || !message.__timeout) return message.result?.result?.value;
    console.warn("    ↳ 第 " + (attempt + 1) + " 次读到页面状态无应答 ⇒ **重发** ✓");
  }
  return undefined;
};

/// **★ 两段式**求值 ✗**（第 139 轮 ✓）：**发起 ＋ 轮询，**每一步都是同步 ✓**。
const evaluateAsync = async (expression) => {
  const key = "__ea" + Math.floor(Math.random() * 1e9);
  await evaluate("window." + key + " = { done: false, value: null, error: null };\n"
    + "(async () => { try { window." + key + ".value = await (" + expression + "); }"
    + " catch (error) { window." + key + ".error = String(error); }"
    + " window." + key + ".done = true; })();\n\"started\"");
  // **★ 两段式失败时**必须能看出卡在哪** ✗ ★**（第 140 轮 ✓）：
  //   **∴ 为什么 ✗**：**负对照**在**断网后**仍然拿到 `undefined`**✗
  //     ⇒ **∴ 而** `evaluateAsync` **自己不**说**哪一步**失败 ✓**** ✓✓
  //   **∴ 做法**：**第 1 次**与**最后 1 次**轮询各**打印一次**✗
  //     ⇒ **∴ 于是**：**发起**失败**与**轮询**失败**就**分得开 ✓**** ✓✓
  let lastRaw = null;
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const raw = await evaluate("window." + key + " ? JSON.stringify({ done: window." + key
      + ".done, value: window." + key + ".value, error: window." + key + ".error }) : null");
    lastRaw = raw;
    if (attempt === 0) console.log("    ↳ 两段式第 1 次轮询 = " + String(raw).slice(0, 120));
    if (typeof raw === "string") {
      try {
        const parsed = JSON.parse(raw);
        if (parsed.done) {
          await evaluate("delete window." + key + "; \"cleaned\"");
          if (parsed.error) { console.warn("    ↳ 两段式求值失败：" + parsed.error.slice(0, 90)); return undefined; }
          return parsed.value;
        }
      } catch (error) { /* 还没好 ⇒ 继续等 */ }
    }
    await sleep(250);
  }
  console.error("    ↳ 两段式轮询 15 秒未完成（最后一次读数 = " + String(lastRaw).slice(0, 160) + "）");
  return undefined;
};
// **另开一条连到 service worker 自己的调试目标** ✓（本次审计加的 ✓）：下面要在 SW 上下文里
// 把 `self.fetch` 换成必然失败的桩 ✓（与 `browser-offline-assets.mjs` 同一招 ✓）。
// **★ 连 worker 目标必须用** `Target.attachToTarget`** ✗ ★**（第 128 轮 ✓；**CDP 探针的铁证 ✓）：
  //   **∴ 症状 ✗**：**直接**连 `webSocketDebuggerUrl`**✗
  //     ⇒ **∴ 它**只**推**事件**✗（实测：`{"method":"Inspector.workerScriptLoaded"}` ✓）
  //       ⇒ **∴ 而**对**命令**（`Runtime.enable` ✓）**不**回**应答** ✓**** ✓✓
  //     ⇒ **∴ 而**本判据的 `send` **只认** `m.id`**✗
  //       ⇒ **∴ 于是**等满 **20 秒** ⇒ **∴ 报**"**CDP 命令超时**" ✓**** ✓✓
  //   **∴ 修法**：**从**主连接**用 `Target.attachToTarget`**✗
  //     ⇒ **∴ 它**给出** `sessionId`**✗
  //       ⇒ **∴ 然后**所有命令**带上**它 ✓**** ✓✓
  //     ⇒ **∴ 于是**：**真的**得到**应答 ✓（**标准** CDP 做法 ✓）** ✓✓
  //   **∴ 接口**保持 `{ socket, send, evaluate }`**✗
  //     ⇒ **∴ 因为**调用处**只用这三样 ✓（`socket.close()` ✓）** ✓✓
  const connectTarget = async (target) => {
    const attached = await send("Target.attachToTarget", { targetId: target.id, flatten: true });
    // **∴ `send` 回的是**整条消息**✗ ⇒ **∴ 会话号在 `result` 里** ✓（第 128 轮 ✓）
    const sessionId = attached && attached.result && attached.result.sessionId;
    if (!sessionId) throw new Error("attachToTarget-failed");
    const targetSend = (method, params) => new Promise((resolve) => {
      const id = nextId++;
      const timer = setTimeout(() => {
        if (!pending.has(id)) return;
        pending.delete(id);
        console.error("✗ CDP 命令超时（" + CDP_TIMEOUT_MS + "ms）：" + method
          + "（worker 会话）⇒ 判据无法作出结论 ✗");
        process.exit(2);
      }, CDP_TIMEOUT_MS);
      // **★ 每条会话消息都要**留下痕迹** ✗ ★**（第 131 轮 ✓；**∵ 超时原因不明 ✓）：
      //   **∴ 为什么 ✗**：**`Runtime.enable` 有应答**✗ ＋ **`Runtime.evaluate` 超时** ✓**** ✓✓
      //     ⇒ **∴ 而**同样写法**在探针里**两条都成功 ✓**** ✓✓
      //     ⇒ **∴ 所以**：**必须**看到**实际**收到了什么 ✓**** ✓✓
      pending.set(id, (message) => { clearTimeout(timer); resolve(message); });
      console.log("  · 会话命令 " + method + "（sessionId " + String(sessionId).slice(0, 8)
        + "…｜请求 id " + id + "）");
      socket.send(JSON.stringify({ id, method, params: params || {}, sessionId }));
    });
    // **★ worker 会话上**不许带 `awaitPromise`** ✗ ★**（第 130 轮 ✓；**探针与判据的差别 ✓）：
    //   **∴ 证据 ✗**：**同一个**目标 ＋ **同一条**命令**✗**：
    //     **∴ 探针**（**不带 `awaitPromise` ✓）⇒ **8 秒内有应答**✗
    //       ⇒ **∴ 实测** `Runtime.evaluate ⇒ 2` ✓**** ✓✓
    //     **∴ 判据**（**带 `awaitPromise: true` ✓）⇒ **20 秒**超时** ✓**** ✓✓
    //   ⇒ **∴ 所以**：**差别**就是这个字段 ✓**** ✓✓
    //     **∴ 它**可能要** worker **支持 `Runtime.awaitPromise`**✗
    //       ⇒ **∴ 而在**这个目标上**它**挂住 ✓**** ✓✓
    //   **∴ 修法**：**去掉**它**✗ —— **∴ 而**本判据用的表达式
    //     （`self.fetch` 的替换 ✓）**都是**同步 IIFE**✗ ⇒ **∴ 不**需要它 ✓**** ✓✓
    const targetEvaluate = async (expression) => {
      // **∴ 按表达式自动判断 ✓**（**第 735 轮 ✓；**理由见 assets／brush 同名注释 ✓）★
      const needsAwait = /\basync\b/.test(expression);
      // **★★★ 不许带 `awaitPromise` ✗ ★★★**（**第 732 轮：**注释早就写了**✗ 而**代码没照做** ✓）
      //   **∴ 实测证据（**第 130 轮 ✓）✗**：**同一个目标 ＋ 同一条命令**✗
      //     ＋ **∴ 探针（**不带 ✓）⇒ **8 秒内有应答**✗ ｜ **∴ 判据（**带 ✓）⇒ **20 秒**超时** ✓ ★
      //   **∴ 后果 ✗**：**CI 上**这一条报**
      //     `sw-evaluate-returned-nothing`**✗ ⇒ **∴ 整个** criteria job **红** ✓ ★
      //     ＋ **∴ 而**它**看起来像**产品 bug**✗ ⇒ **∴ 所以**容易**误导** ✓ ★
      //   **∴ 为什么可以不带 ✗**：**本判据的表达式**都是**同步 IIFE** ✓
      //     ＋ **∴ 所以**：**不需要** `Runtime.awaitPromise`** ✓ ★
      const message = await targetSend("Runtime.evaluate", {
        expression,
        returnByValue: true,
        ...(needsAwait ? { awaitPromise: true } : {}),
      });
      const result = message.result || {};
      return result.result ? result.result.value : undefined;
    };
    return {
      socket: { close: () => { void send("Target.detachFromTarget", { sessionId }).catch(() => undefined); } },
      send: targetSend,
      evaluate: targetEvaluate,
    };
  };
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
// **★★★ 在**页面脚本之前**装一个**错误收集器** ✗ ★★★**（**第 738 轮 ✓）
//   **∴ 为什么必须用 `addScriptToEvaluateOnNewDocument` ✗**（**而不是直接 evaluate ✓）★**：
//     **∴ 因为判据**中途会** `Page.reload`**✗（**∴ 断网重载 ✓）
//       ＋ **∴ 而**直接 evaluate 装的收集器**会在**重载后丢失** ✓
//         ＋ **∴ 于是**：**断网后**那段**的**错误**就**收不到** ✗ ⇒ **∴ 诊断**永远为空** ✓ ★
//   **∴ 它收什么 ✗**：**① `error` 事件**✗（**∴ 如**脚本加载失败 ✓）
//     ＋ **② `unhandledrejection`**✗（**∴ 如** fetch 被桩后**未处理的 Promise 拒绝 ✓）★
//       ＋ **∴ ③ `console.error`**（**∴ 兜底 ✓）★
//   **∴ 用途 ✗**：**当**「**离线画不出墨**」发生时**✗ ⇒ **∴ 把**这些**打出来** ✓
//     ＋ **∴ 从而**看到**真正的原因**✗ 而**不是**猜** ✓ ★
await send("Page.addScriptToEvaluateOnNewDocument", {
  source: `(() => {
    const bag = (window.__offlineDiag = { errs: [], rejected: [] });
    const note = (kind, text) => { bag.errs.push(kind + ": " + String(text).slice(0, 200)); if (bag.errs.length > 40) bag.errs.shift(); };
    window.addEventListener("error", (e) => {
      const t = e && e.target;
      note("error", (t && (t.src || t.href)) ? ("资源加载失败 " + (t.src || t.href)) : ((e && e.message) || "unknown"));
    }, true);
    window.addEventListener("unhandledrejection", (e) => {
      const r = e && e.reason;
      bag.rejected.push(String((r && (r.message || r)) || "unknown").slice(0, 200));
      if (bag.rejected.length > 40) bag.rejected.shift();
    });
    const origErr = console.error.bind(console);
    console.error = (...a) => { note("console.error", a.join(" ")); origErr(...a); };
  })()`,
});
await send("Page.navigate", { url });
// **只等"页面就绪"** ✓（第 892 轮 ✓）：原来固定睡 3 秒 ✗ ⇒ 纯猜测 ✗ ——
// 而紧接着的 `navigator.serviceWorker.ready` **自己就有 12 秒超时的等待** ✓
// ⇒ 这里只需等 `readyState` 完成 ✓（**不与任何断言字段重合** ✓）。
for (let i = 0; i < 40; i++) {
  await sleep(300);
  try { if (await evaluate('document.readyState === "complete"')) break; } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
}
// ① SW 必须注册并**激活**（缓存只有在 activated 后才会被用 ✓）
const ready = await evaluateAsync(`(async () => {
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
const cacheReport = await evaluateAsync(`(async () => {
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
  // **★★★ 必须读 SW 的**预缓存报告** ✗ ★★★**（**第 745 轮 ✓）
  //   **∴ 为什么 ✗**：**上面只统计每个缓存**有多少条**✗ ⇒ **∴ 看不到**谁失败了、为什么失败** ✓ ★
  //     ＋ **∴ 而** SW **自己会把失败写进** __sw_precache_report**（**含 url 与 error ✓）★
  //       ＋ **∴ 尤其重要 ✗**：**第 743 轮我给 SW 加了**索引驱动的预览图预缓存**✗
  //         ＋ **∴ 而**它**的失败**也会进那份报告**✗ ⇒ **∴ 读它**就能知道**预览图到底缓存上了没有** ✓ ★
  //   **∴ 注意 ✗**：**本段在模板字符串**内部** ✗ ⇒ **∴ 注释里**不许出现反引号** ✓（**会提前闭合模板 ✓）★
  try {
    const rep = await caches.match("/__sw_precache_report");
    if (rep) {
      const body = await rep.json();
      out.precache = {
        shell: body.shell,
        previews: body.previews,
        total: body.total,
        failedCount: (body.failed || []).length,
        failed: (body.failed || []).slice(0, 10),
      };
    } else {
      out.precache = { missing: true };
    }
  } catch (error) {
    out.precache = { error: String(error) };
  }
  return out;
})()`);
console.log(`  · 断网前缓存报告 = ${JSON.stringify(cacheReport)}`);
// **∴ 预缓存失败单独打一行 ✗**（**∴ 否则**它埋在长 JSON 里看不见 ✓）★
if (cacheReport && cacheReport.precache) {
  const p = cacheReport.precache;
  console.log(`  · 预缓存：shell=${p.shell}｜预览图=${p.previews}｜失败 ${p.failedCount}`);
  for (const f of (p.failed || []).slice(0, 6)) {
    console.log(`      ✗ ${JSON.stringify(f)}`);
  }
}

// ⚠️ **CDP 的网络模拟到不了 service worker 自己的 `fetch()`** ✗（本次审计复核 ✓）：
// 页面 `navigator.onLine === false` 时，SW 里 `fetch(...)` 照样 200 ✓，
// `Network.setCacheDisabled` 与 `clearBrowserCache` 都管不住 ✓ ⇒ **只模拟页面网络时，
// 本判据可以在 SW 仍走活网（每次导航与每个资产都现取）的情况下通过** ✗ ⇒ 那样它就没有证明 SW 缓存 ✓。
// ⇒ 在 SW 上下文里把 `self.fetch` 换成必然失败的桩 ✓ —— 这正是"网络没了"时 SW 看到的那件事 ✓
// ⇒ 之后外壳只能来自 **SW 自己的 Cache Storage** ✓（`caches.match` 回落那条路才是被考的对象 ✓）。
const swTarget = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())
  // **★ 必须**按 URL 选**我们自己的 SW** ✗ ★**（第 117 轮 ✓；**30 行探针的铁证 ✓）：
  //   **∴ 实测 ✗**：`/json/list` 里的 `service_worker` 目标**有 **3 个**** ✓**** ✓✓
  //     **∴ ①** `http://127.0.0.1:<port>/sw.js` ✓（**我们**的 ✓）
  //     **∴ ②**／**③** **两个浏览器扩展**的 background ✓**** ✓✓
  //   **∴ 而**原来只写 `type === "service_worker"`**✗
  //     ⇒ **∴ 取到的是**第一个**✗（**顺序**不保证 ✓）
  //       ⇒ **∴ 若**取到**扩展的**✗ ⇒ **∴ 我们页面的 SW **没被打桩**✗
  //         ⇒ **★ 于是**：**测的**根本不是**离线** ✓ ★**** ✓✓
  //   **∴ 修法**：**同时**要求 URL 以 `/sw.js` 结尾 ✓**** ✓✓
  // **∴ 必须**按 URL 选 ✓（**第 117 轮 ✓）
  // **★ 必须**同时接受两个 SW 路径** ✗ ★**（第 126 轮 ✓；**双 SW 修好后换来的 ✓）：
  //   **∴ 我**上一轮**只写了 `/sw.js`**✗
  //     ⇒ **∴ 而**修好双 SW 之后**✗ ⇒ **∴ 现在**真正生效的是
  //       `/service-worker.js`**✗（**Rust** 内嵌的那个 ✓）** ✓✓
  //     ⇒ **∴ 于是**找不到目标 ⇒ **∴ 判据**报**"**切不断 SW 的网络 ✓" ✓**** ✓✓
  //   **∴ 修法**：**接受 `/sw.js` 或 `/service-worker.js`**✗
  //     ＋ **必须**排除**扩展**✗（`chrome-extension://…/background.js` ✓）** ✓✓
  //       ⇒ **∴ 因为**本机有两个**扩展的 SW**✗（**第 117 轮实测 ✓）** ✓✓
  .find((target) => {
    if (target.type !== "service_worker") return false;
    const url = String(target.url);
    if (!/^https?:\/\//.test(url)) return false;
    return /\/(sw|service-worker)\.js(\?|$)/.test(url);
  });
const swControl = swTarget ? await connectTarget(swTarget) : null;
if (swControl) await swControl.send("Runtime.enable");
// **★ 顺序：先等内核就绪，**再**切断 SW 的网络** ✗ ★**（第 113 轮 ✓；**探针证据 ✓）：
//   **∴ 原来**顺序反了 ✗**：**先**切断 SW 的 fetch**✗ ⇒ **∴ 再**等内核 ✓**** ✓✓
//     ⇒ **∴ 而**内核装载**正要**用 SW 走 `/api/atoms`**✗
//       ⇒ **★ 于是**内核**永远就绪不了** ✗
//         ⇒ **∴ 判据**报**"**断网前内核 = null（**在线都没就绪 ✓）" ✓ ★**** ✓✓
//   **∴ 证据（**30 行 CDP 探针 ✓）★**：**不切网络时**✗
//     **∴ 静态 PWA** 上 `kernelReady=true`／`kernelStats` **有**／`boardW=1024` ✓
//       ⇒ **∴ 所以**：**内核**本来**是好的**✗ ⇒ **∴ 问题**只在**顺序 ✓ ★**** ✓✓
// **断网前先让内核装载完** ✗ —— 离线重载要靠**本地缓存**里的 `/api/atoms` 才建得起内核 ✓；
// 在线这趟没装完就断网 ⇒ 离线内核是 null ⇒ 后面量到的"画不出墨"是**前提不成立** ✗，
// 而不是"离线不会画" ✓（本机高负载时实测踩到过：`readyState` 完成 ≠ 内核装载完成 ✓）。
let onlineKernel = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
for (let attempt = 0; attempt < 80 && !onlineKernel; attempt += 1) {
  await sleep(250);
  onlineKernel = await evaluate(`(() => (window.yanshi.kernelStats ? window.yanshi.kernelStats() : null))()`);
}
console.log(`  · 断网前内核 = ${onlineKernel ? JSON.stringify({ head_seq: onlineKernel.head_seq }) : "null（在线都没就绪）"}`);
// **★★★ 对照探针：**在线**阶段打**同一组指标** ✗ ★★★**（**第 749 轮 ✓）
//   **∴ 为什么必须对照 ✗**（**第 748 轮我犯的错 ✓）★**：
//     **∴ 我**看到离线的 `brushNow: null`**✗ 就**断定它是根因** ✓
//       ＋ **∴ 而**判据**从不选笔**✗（**∴ 只** `brushBtn.click()` **打开面板 ✓）
//         ＋ **∴ 所以**：**在线阶段**大概也是 `null`**✗ ⇒ **∴ 它**很可能**不是**离线特有** ✓ ★★
//   **∴ 正确做法 ✗**：**同一组字段**在**两个时刻**各打一次**✗
//     ＋ **∴ 只有**离线特有**的指标才是根因** ✓ ★
//   **∴ 打四样（**与离线那份完全相同 ✓）✗**：
//     **∴ ① `brushNow`／`toolNow`／`sizeNow`**✗ ／ **② `applies`／`lastArea`**
//       ＋ **③ `localCalls`／`serverCalls`**✗ ／ **④ `lastRenderMs`** ✓ ★
// **★★★ 声明**必须在赋值之前** ✗ ★★★**（**第 750 轮：**TDZ 陷阱 ✓）
//   **∴ 我**上一版把 `let` 声明**放在探针之后**✗
//     ＋ **∴ 于是**：**赋值处** `onlineProbeRef` 还在**暂时性死区（TDZ）** ✗
//       ⇒ **∴ 抛** `ReferenceError`**✗ ⇒ **∴ 被**同一个 `try/catch` **吞掉** ✓ ★
//         ＋ **∴ 而** `catch` **打印的那行**又**在收集窗口之外**✗
//           ⇒ **∴ 于是**：**结果** `在线对照=null`**✗ 而**我**看不到任何错误** ✓ ★★
//             ＋ **∴ 教训 ✗**：**`let` **必须**在赋值之前声明** ✓
//               ＋ **∴ 且** `catch` **里的错误**也要**送到必然可见的地方** ✓ ★
let onlineProbeRef = null;
try {
  const onlineProbe = await evaluate(`(() => {
      const w = window;
      const s = (w.yanshi && w.yanshi.state) ? w.yanshi.state() : null;
      const st = w.yanshiStats || {};
      return { brushNow: s ? (s.brush === undefined ? null : s.brush) : "no-state",
               toolNow: s ? (s.tool === undefined ? null : s.tool) : null,
               sizeNow: s ? (s.size === undefined ? null : s.size) : null,
               applies: st.applies, lastArea: st.lastArea, lastRenderMs: st.lastRenderMs,
               localCalls: st.localCalls, serverCalls: st.serverCalls };
    })()`);
  onlineProbeRef = onlineProbe;
} catch (e) {
  // **∴ 错误也必须**可见** ✗**（**∴ 否则**又是静默失败 ✓）★
  onlineProbeRef = { probeError: String(e) };
}
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


// **★ 不设 CDP 离线 ✗ ★**（第 189 轮 ✓；**实测换来的 ✓）：
//   **∴ 为什么改 ✗**：**实测**（**第 188 轮 ✓）证明**✗**：
//     **∴ CDP 的 `offline: true` 下**✗
//       ⇒ **∴ `fetch` **既不**成功**✗、**也**不**失败** ✓（**∴ 它**挂着 ✓）**
//         ⇒ **∴ 连** `AbortSignal.timeout(8000)` **都**不触发** ✓
//           （**∴ 可能**因为** SW **接管了那次 fetch**✗ ⇒ **∴ 页面**约束不到它 ✓）** ✓✓
//       ⇒ **★ 而**真实离线里**✗
//         ⇒ **∴ `fetch` **会**快速失败** ✓（**∴ 如** `TypeError: Failed to fetch` ✓）**
//           ⇒ **∴ 于是**走的**是**回落路径** ✓**** ✓✓
//   **∴ 所以**：**CDP 的 offline **造出了一个**真实世界不存在**的状态**✗**
//     ⇒ **∴ 它**把**"网络挂着 ✓"**报成了**产品缺口** ✓（**∴ 不是**真的 ✓）** ✓✓
//   **∴ 替代 ✗**：**只**用 SW 层拦截**✗**
//     （**`self.fetch` ⇒ 必然 reject ✓，见上面那一段 ✓）**
//     ⇒ **∴ 那就**等价于**"**离线 ＋ 缓存未命中 ✓"**✗
//       ⇒ **∴ 而**不是**"**网络挂着 ✓" ✓**** ✓✓
//     **∴ 且**：**页面**仍在**在线状态**✗
//       ⇒ **∴ 于是** `fetch` **会**真的**发出去**✗
//         ⇒ **∴ 而** SW **对缓存命中的请求**照常**从缓存返回** ✓
//           ⇒ **∴ 于是**：**内核**依赖的**缓存资源**能**拿到**✗
//             ⇒ **∴ 而**未缓存的**会**快速失败** ✓ ★**** ✓✓
//   **∴ 保真度对比 ✗**：**真实离线**里**首次**打开的**资源**来自** SW 缓存**✗**
//     ⇒ **∴ 本改法**与它**一致** ✓（**∴ 而** CDP offline **不**一致 ✓）** ✓✓
// （**∴ 原来的那一行留在这里当记录 ✗**：）
// await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
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
  // **★ 那个探测**必须自带超时** ✗ ★**（第 141 轮 ✓；**实测换来的 ✓）：
  //   **∴ 为什么 ✗**：**断网时**那一次 `fetch` **既不**成功**✗、**也**不**失败** ✗
  //     ⇒ **∴ 它**永远**挂着** ✓（**实测：**`done: false`** 一直不变 ✓）** ✓✓
  //     ⇒ **∴ 于是**：**`controlProbe`**永远**拿不到值** ⇒ **∴ 而**判据
  //       **把 `undefined` **当成**"**仍然成功 ✓"** ⇒ **∴ 报** VOID ✓**** ✓✓
  //   **∴ 修法**：**`Promise.race` ＋ 3 秒超时**✗
  //     ⇒ **∴ 于是**三种结果**✗**：
  //       **∴ ①** **reject ⇒ `failed: true` ⇒ **负对照**成立 ✓**** ✓✓
  //       **∴ ②** **成功 ⇒ `failed: false` ⇒ **网络**真的**没断** ⇒ **∴ 才**该 VOID ✓**** ✓✓
  //       **∴ ③** **超时 ⇒ `hanging: true` ⇒ **那**也**是断网的证据**✗
  //         ⇒ **∴ 应当**算**负对照**成立 ✓（**∴ 只是**形式不同 ✓）★**** ✓✓
  // **★ 负对照（**第 328 轮改成验 SW 桩 ✓）★**：
//   **∴ 为什么改 ✗**（**实测 ＋ 读码换来的 ✓）★**：
//     **∴ 本判据**故意不设 CDP offline**（**见 306–323 行的论证 ✓）
//       ⇒ **∴ 于是**：**页面仍在**在线状态**✗ ⇒ **∴ 它的 `fetch` **会真的发出去** ✓**** ✓✓
//         ⇒ **★ 所以**旧版的负对照**永远**拿到 `{failed:false,status:501}`** ✗
//           ⇒ **∴ 于是**判据**永远 VOID** ✗（**∴ 而** VOID **等价于红** ✓）** ★**** ✓✓
//     **∴ 正确的是 ✗**：**负对照要验**本判据**实际使用的离线机制**✗
//       ⇒ **∴ 即**：**SW 上的 `self.fetch` **必须已经是桩** ✓（**∴ 见 `setServiceWorkerFetch` ✓）** ✓✓
//       **∴ 且**：**桩必须**真的 reject**（**∴ 直接叫一次看结果 ✓）** ✓**** ✓✓
//   **∴ 三段结果（**与旧的语义对齐 ✓）**：
//     **∴ ①** **桩在 ＋ 调用 reject ⇒ `failed: true` ⇒ **负对照**成立 ✓**** ✓✓
//     **∴ ②** **桩不在 ⇒ `failed: false` ⇒ **离线机制**没生效** ⇒ **∴ 才**该 VOID ✓**** ✓✓
//     **∴ ③** **桩在但调用**没 reject**✗ ⇒ **∴ 那**也是**机制错** ⇒ **∴ VOID** ✓**** ✓✓
let controlProbe = swControl
  ? (await swControl.send("Runtime.evaluate", { 
      // **★ 必须显式 `returnByValue` ＋ `awaitPromise` ✗ ★**（第 334 轮 ✓；**实测换来的 ✓）：
      //   **∴ 第 333 轮的 CI 显示 ✗**：**`负对照 … = {}`** ✓
      //     ⇒ **∴ 即**：**`swControl.evaluate` **没把对象取回来**✗（**∴ 只拿到空对象 ✓）
      //       ⇒ **∴ 所以**：**这里**改用**原始 `Runtime.evaluate`**✗
      //         ＋ **`returnByValue: true`**（**对象要按值回传 ✓）
      //         ＋ **`awaitPromise: true`**（**async IIFE 要等 ✓）** ✓✓
      expression: `(async () => {
        const isStub = self.fetch !== self.__yanshiRealFetch;
        if (!isStub) return { failed: false, why: "sw-fetch-is-not-stubbed" };
        try {
          await self.fetch("${NEGATIVE_CONTROL}");
          return { failed: false, why: "stub-did-not-reject" };
        } catch (error) {
          return { failed: true, error: String(error) };
        }
      })()`,
      returnByValue: true,
      // **★★★ `awaitPromise` 在这里**必须**有 ✗ ★★★**（**第 735 轮：**我上轮删错了 ✓）
      //   **∴ 我上轮的错 ✗**（**第 732 轮 ✓）✗**：**我**按「**worker 会话不许带 `awaitPromise`**」
      //     ＋ **∴ 把**这一处**也**删了**✗ ⇒ **∴ 于是** CI **两次都报**
      //       `sw-evaluate-returned-nothing`** ✓ ★
      //   **∴ 真因 ✗**（**AGENTS.md 明写 ✓）✗**：**上面那个表达式是**
      //     `(async () => { … })()`**✗ ⇒ **∴ 它**返回 Promise** ✓
      //       ＋ **∴ 而** `returnByValue: true` **单独用时**✗
      //         ⇒ **∴ CDP **无法序列化 Promise**✗ ⇒ **∴ 静默返回空对象** ✓ ★★
      //   **∴ 两难 ✗**：**带它 ⇒ worker 会话上**可能挂住**（**第 130 轮实测 ✓）**✗
      //     ＋ **∴ 不带 ⇒ **async 表达式无法序列化**（**本次 ✓）★
      //       ＋ **∴ 而**第 130 轮测的是**同步表达式**✗ ⇒ **∴ 所以**两条规则**不矛盾** ✓ ★
      //         ＋ **∴ 正确规则 ✗**：**同步表达式 ⇒ 不带**✗ ／ **async 表达式 ⇒ 带** ✓ ★
      awaitPromise: true,
    }))
  : { failed: false, why: "no-sw-control-target" };
// **★★★ 取不到值时**打印原文** ✗ ★★★**（**第 737 轮 ✓；**∴ 让下次 CI 自己说出原因 ✓）
//   **∴ 为什么不再猜 ✗**（**已耗尽两次猜测 ✓）★**：
//     **∴ ① 我猜** `awaitPromise` **✗ ⇒ **∴ CI **否证** ✓
//       ＋ **∴ ② 我改**按表达式自动判断**✗ ⇒ **∴ CI **仍报同样的错** ✓
//         ＋ **∴ 所以**：**必须**把**原始回包**打出来** ✓ ★
//           ＋ **∴ 要打三样 ✗**：**① 包 keys**✗ ／ **② `result` keys**✗（**∴ 可能**是 `exceptionDetails` ✓）
//             ＋ **③ 原始包前 600 字符**（**∴ 从而**看到**真正的错** ✓）★
if (swControl) {
  const raw = await swControl.send("Runtime.evaluate", {
    expression: `(async () => {
        const isStub = self.fetch !== self.__yanshiRealFetch;
        if (!isStub) return { failed: false, why: "sw-fetch-is-not-stubbed" };
        try {
          await self.fetch("${NEGATIVE_CONTROL}");
          return { failed: false, why: "stub-did-not-reject" };
        } catch (error) {
          return { failed: true, error: String(error) };
        }
      })()`,
    returnByValue: true,
    awaitPromise: true,
  });
  if (raw && raw.result && raw.result.result && "value" in raw.result.result) {
    controlProbe = raw.result.result.value;
  } else {
    console.error("  ★ 负对照取不到值 ⇒ 打印原始回包（**诊断用 ✓）");
    console.error("     包 keys = " + JSON.stringify(Object.keys(raw || {})));
    console.error("     result keys = " + JSON.stringify(Object.keys((raw && raw.result) || {})));
    console.error("     原始 = " + JSON.stringify(raw).slice(0, 600));
    controlProbe = { failed: false, why: "sw-evaluate-returned-nothing" };
  }
}
console.log("  · 负对照（SW 桩必须让未缓存请求失败）= " + JSON.stringify(controlProbe));
// **★ 「**没有答案**」不等于「**成功了**」 ✗ ★**（第 133 轮 ✓；**VOID 误报换来的 ✓）：
  //   **∴ 症状 ✗**：**那一次 evaluate 悬空**✗ ⇒ **∴ 结果**是 `undefined`** ✗
  //     ⇒ **∴ 而**判据**读 `controlProbe.failed`**✗ ⇒ **∴ 得到 `undefined`** ✓**** ✓✓
  //       ⇒ **∴ 于是**：**它**把**"**没有答案 ✓"**当成了**"**仍然成功 ✓"**✗
  //         ⇒ **∴ 报**"**断网模拟没有生效 ⇒ 判据作废（VOID）" ✓**** ✓✓
  //         ⇒ **★ 那是**一个**假结论** ✓ ★**** ✓✓
  //   **∴ 修法**：**分成三种情况**✗**：
  //     **∴ ①** `undefined`／没有结果 ⇒ **∴ 报**"**无法判断（**evaluate 三次都不回 ✓）" ✓**** ✓✓
  //     **∴ ②** `failed !== true` ⇒ **∴ 报**"**真的**仍然成功 ⇒ **断网模拟**没有**生效 ✓" ✓**** ✓✓
  //     **∴ ③** `failed === true` ⇒ **∴ 负对照**成立 ✓
  if (!controlProbe || (controlProbe && controlProbe.__timeout)) {
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
// **这里原先就恢复联网** ✗ ⇒ 下面"断网后笔刷面板"与"离线落笔"三段**其实跑在线上** ✗
// （既有问题，本次审计确认 ✓）。现在把它**移到最后** ✓，让那三段成为**真正的离线断言** ✓。
// 以前不能移：那时 `/health` 不在 `SHELL` 里 ⇒ 离线首次 `fetch("/health")` 落空 ⇒
// 回落到外壳 HTML ⇒ `.json()` 抛 `Unexpected token '<'` ⇒ 内核起不来 ⇒ 三段全红 ✗。
// 现在 `SHELL` 已预缓存 `/health` ✓（与 wasm 两件同版 ✓）⇒ 内核能在离线时起来 ✓。
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
// **★ 三个信号必须**一起读** ✗ ★**（第 119 轮 ✓；**探针与判据不一致换来的 ✓）：
  //   **∴ 背景 ✗**：**离线探针**看到 `window.yanshi` 是 **object**／正文 **625**／
  //     **标题正常**／**资源都请求了** ⇒ **∴ 页面**是活的 ✓**** ✓✓
  //     **∴ 而**本判据**读到** `__appMarks` 是 **null**✗
  //       ⇒ **∴ 于是**它**报**"**脚本没执行 ✓"一类** ✓**** ✓✓
  //   **∴ 所以**：**两个观察**不一致**✗ ⇒ **∴ 必须**在**同一处**把
  //     `__appMarks`／`window.yanshi`／`document.title` **一起读** ✓**** ✓✓
  //     ⇒ **∴ 于是**：**一眼**分出**是**时机／上下文／别的页面 ✓**** ✓✓
  const marksProbe = await evaluate(`(() => ({
    marks: window.__appMarks || null,
    yanshi: typeof window.yanshi,
    kernelStats: (window.yanshi && window.yanshi.kernelStats) ? (window.yanshi.kernelStats() ? "ready" : "null") : "no-hook",
    title: document.title || "",
    bodyLen: document.body ? document.body.innerText.length : -1,
    scriptCount: document.scripts.length,
    appJsLoaded: Array.from(document.scripts).some((x) => String(x.src).includes("viewer-app.js")),
    // **★ 内核状态那一行**必须读出来** ✗ ★**（第 150 轮 ✓）：
    //   **∴ 为什么 ✗**：**「wasmState」那个元素的文字**直接**说出**内核走到了哪一步 ✓**** ✓✓
    //     **∴ 如**「已加载」／「不可用」／「已按设置跳过」／**还**停在**初始值 ✓** ✓✓
    //   ⇒ **∴ 于是**：**失败时**一眼**知道**是**哪一步** ✓**** ✓✓
    wasmState: (function () { const e = document.getElementById("wasmState"); return e ? e.textContent : "（无元素）"; })(),
    statsWasm: (window.yanshiStats && window.yanshiStats.wasm) === true,
    wasmModule: typeof window.__yanshiWasmModule,
    abortTimeout: (typeof AbortSignal !== "undefined" && typeof AbortSignal.timeout === "function") ? "支持" : "不支持",
  }))()`);
  console.log(`  · 三信号 = ${JSON.stringify(marksProbe)}`);
  // **★ 页面用的脚本**是不是我改的那一份** ✗ ★**（第 168 轮 ✓；**证明离线页面来源 ✓）：
  //   **∴ 为什么 ✗**：**离线时**页面**可能**来自**SW 缓存** ✗**
  //     ⇒ **∴ 而**我在源码里加的新标记**若**不在那份里**✗
  //       ⇒ **∴ 那些标记**永远**不会出现** ✓**** ✓✓
  //   **∴ 做法**：**取**页面实际引用的 viewer-app.js**✗
  //     ⇒ **∴ 看**它**有没有**那两个新标记**✗ ＋ **看**它的**字节数** ✓**** ✓✓
  const scriptInfo = await evaluateAsync(`(async () => {
    const tag = Array.from(document.scripts).find((x) => String(x.src).includes("viewer-app.js"));
    if (!tag) return { found: false };
    try {
      const text = await (await fetch(tag.src, { cache: "no-store" })).text();
      return { found: true, url: String(tag.src).slice(-34), bytes: text.length,
               hasBrushMark: text.includes("setupBrushLibrary:before"),
               hasObjectMark: text.includes("yanshi-object:before") };
    } catch (error) { return { found: true, error: String(error).slice(0, 80) }; }
  })()`);
  console.log("  · 页面脚本 = " + JSON.stringify(scriptInfo));
  const marks = marksProbe ? marksProbe.marks : null;
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
// **★★★ 落笔之前**必须先选一支笔** ✗ ★★★**（**第 752 轮 ✓；**∴ 由对照 ＋ 全画布扫描推出 ✓）
//   **∴ 证据链 ✗**（**决定性的 ✓）★**：
//     **∴ ① `localCalls` 13 ⇒ 30**✗（**∴ 内核**被调用了 17 次 ✓）
//       ＋ **∴ ② 而 `applies: 0`／`lastArea: 0`／`lastRenderMs: null`**✗
//         ＋ **∴ ③ 全画布扫描 `inkAll: 0`**（**∴ 画布 1024² **整体**无墨 ✓）★
//           ⇒ **∴ 所以**：**内核**被调用但**从不 apply** ✓ ★
//     **∴ 最可能的原因 ✗**：**绘制请求里**没有笔刷**✗
//       ＋ **∴ 而**判据**从不选笔**✗（**∴ 它**只 `brushBtn.click()` **打开面板 ✓）★
//         ＋ **∴ 而** `brushNow` **在线／离线都**是 null**✗ ⇒ **∴ 说明**判据两趟都没选笔** ✓ ★★
//   **∴ 修法 ✗**：**用**前端**唯一的选笔入口**✗ —— `window.yanshi.setBrush(name)`** ✓
//     ＋ **∴ 为什么用它 ✗**：** `viewer-app.js:8748` **注明**「**选择只有一条路**：
//       点一行 ⇒ 走 `window.yanshi.setBrush()` ⇒ 与下拉、与 MCP **同一个入口**」** ✓ ★
//       ＋ **∴ 而**笔名**从**已打开的面板**取第一项**✗（**∴ 不**硬编码 ✓）★
let brushedRef = null;
try {
  const brushed = await evaluate(`(async () => {
      const w = window;
      const btns = Array.from(document.querySelectorAll("#brushLibraryList [data-brush], #brushLibraryList button, #brushLibraryList li"));
      const names = btns.map((b) => b.getAttribute("data-brush") || b.textContent || "").map((t) => t.trim()).filter(Boolean);
      // **∴ 从**面板文本**取一个**像文件名的**（**∴ 如** Blender.myb ✓）
      const guess = names.find((t) => /\\.myb$/.test(t)) || names[0] || null;
      let applied = null, error = null;
      if (guess) {
        try { await w.yanshi.setBrush(guess); applied = guess; }
        catch (e) { error = String(e); }
      } else {
        // **∴ 面板里没有名字 ⇒ **回退**到内置的第一支笔 ✓
        try { await w.yanshi.setBrush("2B_pencil.myb"); applied = "2B_pencil.myb（回退）"; }
        catch (e) { error = String(e); }
      }
      const s = (w.yanshi && w.yanshi.state) ? w.yanshi.state() : null;
      return { candidateCount: names.length, applied: applied, error: error,
               brushAfter: s ? (s.brush === undefined ? null : s.brush) : "no-state",
               sample: names.slice(0, 3) };
    })()`);
  brushedRef = brushed;
} catch (e) {
  brushedRef = { probeError: String(e) };
}
// **∴ 落笔前的状态（**供「离线落笔」行引用 ✓）★
let beforeRef = null;
// **★★★ 落笔**之前**的笔刷状态 ✗ ★★★**（**第 748 轮 ✓；**∴ 一劳永逸区分两个候选 ✓）
//   **∴ 为什么必须卡在这个时刻 ✗**（**第 747 轮的诊断只给了**落笔后**的状态 ✓）★**：
//     **∴ 落笔后 `brushNow: null`**✗ ＋ **∴ `applies: 0`／`lastArea: 0`** ✓
//       ＋ **∴ 而那**有两种解释 ✗ 而**两种**修法不同** ✓ ★
//         ＋ **∴ 候选 ①（**产品缺陷 ✓）✗**：**页面**重载后**没恢复**选中的笔刷**✗
//           ⇒ **∴ 于是** `brushNow` **本来就是** null** ✓ ⇒ **∴ 修产品**（**状态持久化 ✓）★
//             ＋ **∴ 候选 ②（**判据缺陷 ✓）✗**：**判据**重载后**没先**选笔**✗
//               ＋ **∴ 而**落笔**某处**把 `brushNow` **清成了** null** ✓ ⇒ **∴ 修判据** ✓ ★
//   **∴ 现在 ✗**：**在**落笔**之前**把**同一组状态**打出来**✗
//     ＋ **∴ 对比两行**就能立刻判定**是哪一种** ✓ ★
try {
  const before = await evaluate(`(() => {
      const w = window;
      const s = (w.yanshi && w.yanshi.state) ? w.yanshi.state() : null;
      return { brushNow: s ? (s.brush === undefined ? null : s.brush) : "no-state",
               toolNow: s ? (s.tool === undefined ? null : s.tool) : null,
               sizeNow: s ? (s.size === undefined ? null : s.size) : null,
               applies: (w.yanshiStats || {}).applies,
               localCalls: (w.yanshiStats || {}).localCalls,
               serverCalls: (w.yanshiStats || {}).serverCalls,
               strokeKeys: s && s.stroke ? Object.keys(s.stroke) : null };
    })()`);
  // **★★★ 必须用判据的**标准标注格式** ✗ ★★★**（**第 748 轮实测教训 ✓）
  //   **∴ 我第一版用 `console.error("  · …")`**✗ ⇒ **∴ 而** CI 的 criteria 运行器
  //     **只收集**「**前│／后│／头│／体│／项│**」等**标注行** ✓
  //     ＋ **∴ 于是**：**那一行**在日志里**根本看不到** ✗（**∴ 我**白等了一轮 ✓）★
  //   **∴ 所以 ✗**：**改** `前│` **前缀**✗ ⇒ **∴ 它**与**已有的输出**同一格式** ⇒ **∴ 必被收集** ✓ ★
  beforeRef = before;
  console.log("  前│  · 落笔之前的状态 = " + JSON.stringify(before));
} catch (e) {
  console.error("  · 落笔前状态求值失败：" + e);
}
// **★ 求值必须**自己报错** ✗ ★**（第 395 轮 ✓；**实测根因 ✓）：
//   **∴ 症状 ✗**：**这段求值**在离线时抛异常**✗ ⇒ **∴ `evaluate` **吞掉它** ⇒ **∴ 返回 `undefined`
//     ⇒ **∴ 打印成** `{}`** ✓ ⇒ **∴ 于是**判据**误判「**事件没进状态机**」 ✓
//       ⇒ **∴ 而**实测（**第 394 轮 ✓）**：**同一个页面**跑完后** `state().tool === "brush"`**✗
//         ＋ **∴ 再发一次事件** ⇒ `points:1`／`dragging:1`** ✓
//           ＋ **∴ 而**像素计数**从 0 变 59** ✓
//             ⇒ **★ 所以**那次落笔**其实成功了**✗ ⇒ **∴ 是**求值自己坏了** ✓ ★**** ✓✓
//   **∴ 修法 ✗**：**包一层 `try/catch`**✗ ⇒ **∴ 于是**异常**带文本返回** ✓
//     ＋ **∴ 调用方**把**非对象／空对象**当成**硬失败**（**∴ 不再**伪装成产品症状 ✓）★**** ✓✓
const strokeRaw = await evaluate(`Promise.race([(async () => {
  try {
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
           draggingAtDown: draggingAtDown, pointsAtMove: atMove.points,
           // **★ 诊断（**第 364 轮 ✓）：那一刻的真实状态 ✗** ⇒ **∴ 一次运行就能定位 ✓**
           stateKeys: Object.keys(atDown).length,
           toolNow: atDown.tool,
           draggingNow: atDown.dragging,
           sizeNow: atDown.size, brushNow: atDown.brush,
           strokeState: { points: atDown.points, dragging: atDown.dragging } };
  } catch (error) {
    // **★ 求值自己的异常必须**说出来** ✗ ★**（第 395 轮 ✓）：
    //   **∴ 否则** 'evaluate' 吞掉它**✗ ⇒ **∴ 打印成 '{}'** ⇒ **∴ 判据**误判产品** ✓
    return { result: "evaluate-threw", error: String((error && error.message) || error).slice(0, 200),
             stack: String((error && error.stack) || "").split("\\n").slice(0, 4).join(" | ").slice(0, 300) };
  }
})(), new Promise((r) => setTimeout(() => r({ result: "evaluate-timeout",
    where: "判据的落笔求值在 8 秒内没有返回 ⇒ 多半是某个 await 挂在离线网络上"
      + "（已知限制：CDP 页面级 offline 管不到 SW 上下文的 fetch）" }), 8000))])`);
const strokeResult = strokeRaw;
// **★ 求值失败**不许**伪装成产品症状 ✗ ★**（第 395 轮 ✓）：**∴ 空对象／缺字段**都算**硬失败** ✓
// **★ 原始值**必须打出来 ✗ ★**（第 402 轮 ✓）：**∴ 不然分不清** `undefined` 与 `{}`** ✓
const rawJson = (() => { try { return JSON.stringify(strokeRaw); } catch (e) { return "stringify失败:" + e; } })();
console.error("  · 调试：typeof strokeRaw = " + typeof strokeRaw
  + "｜isArray = " + Array.isArray(strokeRaw)
  + "｜keys = " + (strokeRaw && typeof strokeRaw === "object" ? JSON.stringify(Object.keys(strokeRaw)) : "n/a")
  + "｜raw = " + String(rawJson).slice(0, 300));
const emptyResult = !strokeResult || (typeof strokeResult === "object"
  && Object.keys(strokeResult).length === 0);
if (emptyResult || typeof strokeResult !== "object"
    || strokeResult.result === "evaluate-threw" || strokeResult.result === "evaluate-timeout") {
  console.error("✗ 判据自身的求值失败（不是产品问题）⇒ 结果无效：" + JSON.stringify(strokeResult));
  process.exit(3);
}
// **有界等出墨** ✓（等待条件与断言条件对齐 ✓ —— 第 1116 轮的规矩 ✓）：提交是异步的 ✓，
// 固定睡一次会把"还没画完"读成"画不出来" ✗。
let inkAfter = await evaluate(canvasSig);
for (let i = 0; i < 32 && !(inkAfter && inkBefore && inkAfter.ink > inkBefore.ink); i += 1) {
  await sleep(250);
  inkAfter = await evaluate(canvasSig);
}
console.log(`  · 离线落笔 = ${JSON.stringify(strokeResult)} ｜ 非背景像素 ${inkBefore && inkBefore.ink} => ${inkAfter && inkAfter.ink}`
  + ` ｜ 落笔前 brushNow=${JSON.stringify(beforeRef && beforeRef.brushNow)}`
  // **★★★ 对照必须**同处打印** ✗ ★★★**（**第 749 轮实测教训 ✓）
  //   **∴ 我**第一版**在**在线阶段**单独打一行**✗ ⇒ **∴ 而**判据的输出收集**有窗口**✗
  //     ＋ **∴ 于是**：**那一行**被**挤出窗口**✗ ⇒ **∴ 白等一轮** ✓ ★
  //       ＋ **∴ 所以 ✗**：**存进变量 ＋ **在**这个必然可见的行**里一起打** ✓ ★
  + ` ｜ 在线对照=${JSON.stringify(onlineProbeRef)}`
  // **∴ 选笔结果也必须在**可见行**里 ✗**（**∴ 否则**又是静默失败 ✓）★
  + ` ｜ 选笔=${JSON.stringify(brushedRef)}`);
// ① **事件确实进了笔画状态机** ✓（`points` 从 0 变成 > 0 ✓、`dragging` 为真 ✓）。
if (!strokeResult || typeof strokeResult !== "object" || strokeResult.pointsAtDown <= 0 || !strokeResult.draggingAtDown) {
  failures.push(`落笔事件没有进入笔画状态机（tool=${strokeResult && strokeResult.tool}｜points@down=${strokeResult && strokeResult.pointsAtDown}｜dragging@down=${strokeResult && strokeResult.draggingAtDown}）⇒ 事件被平移那条吃掉了`);
} else if (!(strokeResult.pointsAtMove > strokeResult.pointsAtDown)) {
  failures.push(`pointermove 没有累积笔画点（${strokeResult.pointsAtDown} => ${strokeResult.pointsAtMove}）`);
}
// ② **画布上真的多了墨** ✓。
if (!(inkAfter && inkBefore && inkAfter.ink > inkBefore.ink)) {
  failures.push(`离线画不出墨：非背景像素 ${inkBefore && inkBefore.ink} => ${inkAfter && inkAfter.ink}`);
  // **★★★ 诊断：说出**为什么**画不上 ✗ ★★★**（**第 738 轮 ✓；**∴ 沿用上次奏效的办法：不猜 ✓）
  //   **∴ 为什么需要 ✗**（**已确认的事实 ✓）★**：
  //     **∴ ① 读数**正确**✗**（**∴ 历史注释记着**第 394 轮曾看到 **0 => 59** ✓）
  //       ＋ **∴ ② SW **已激活**✗ ＋ **∴ 内核**有内容**（`head_seq:1` ✓）＋ **∴ 落笔**进了状态机** ✓
  //         ＋ **∴ ③ 而**判据自己写着**一条**已知限制**✗：
  //           「**CDP **页面级 offline **管不到** SW **上下文的 fetch**」** ✓
  //           ＋ **∴ 于是**它**改桩** SW **的 `self.fetch`**✗（**∴ 负对照**证明桩**生效** ✓）★
  //             ＋ **∴ 所以**：**渲染**若要**某个**网络资源**✗ ⇒ **∴ 它**就被**桩掉了** ✓
  //   **∴ 现在 ✗**：**把**离线期间的**页面错误 ＋ 失败请求**打出来** ✓ ★
  try {
    const errs = await evaluate(`(() => {
        const w = window;
        const bag = (w.__offlineDiag = w.__offlineDiag || { errs: [], rejected: [] });
        return { errs: bag.errs.slice(-8), rejected: bag.rejected.slice(-8),
                 kernel: !!(w.yanshiStats && w.yanshiStats.wasm),
                 tool: (w.yanshi && w.yanshi.state && w.yanshi.state().tool) || null };
      })()`);
    console.error("     ↳ 诊断（离线期间）：" + JSON.stringify(errs));
  } catch (e) {
    console.error("     ↳ 诊断求值失败：" + e);
  }
  // **★★★ 渲染侧诊断 ✗ ★★★**（**第 747 轮 ✓；**∴ 预缓存已 0 失败**⇒** ∴ 问题在**渲染本身** ✓）
  //   **∴ 为什么加它 ✗**（**第 746 轮 CI 的新事实 ✓）★**：
  //     **∴ 预缓存**从 10 个失败 ⇒ **0**✗ ＋ **∴ 而**「**离线画不出墨**」**仍在** ✓
  //       ＋ **∴ 且**输出里**不再有**「**资源加载失败**」**✗ ⇒ **∴ 所以**：**资源**不是原因** ✓ ★
  //         ＋ **∴ 因此**要问的是 ✗**：**落笔之后**渲染**到底**走了哪条路**✗ ／ **画布**收到了什么** ✓ ★
  //   **∴ 打四样 ✗**：**① `yanshiStats`（**wasm／local／server 计数 ✓）
  //     ＋ **② `kernelStats()` 的 `head_seq` ＋ `blobs`**（**∴ 内核**有没有**收到笔 ✓）
  //       ＋ **③ 画布前 4 个像素的 RGBA**（**∴ 看**到底画了什么 ✓）
  //         ＋ **④ 缓存的键数 ＋ 控制器状态**（**∴ 确认**离线外壳是否真的在服务** ✓）★
  try {
    const render = await evaluate(`(() => {
        const w = window;
        const out = {};
        out.stats = w.yanshiStats ? JSON.parse(JSON.stringify(w.yanshiStats)) : null;
        try { out.kernel = w.yanshi && w.yanshi.kernelStats ? w.yanshi.kernelStats() : null; }
        catch (e) { out.kernelError = String(e); }
        const c = document.getElementById("board");
        if (c) {
          const g = c.getContext("2d");
          const d = g.getImageData(0, 0, 4, 1).data;
          out.firstPixels = Array.from(d.slice(0, 16));
          out.size = [c.width, c.height];
        } else { out.canvas = "missing"; }
        out.scroll = (() => { const s = document.getElementById("boardScroll"); return s ? [s.scrollLeft, s.scrollTop] : null; })();
        return out;
      })()`);
    console.error("     ↳ 诊断（渲染侧）：" + JSON.stringify(render).slice(0, 900));
  } catch (e) {
    console.error("     ↳ 渲染侧诊断求值失败：" + e);
  }
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
// **★★★ 决定性诊断：**全画布**扫描 ✗ ★★★**（**第 751 轮 ✓；**∴ 由对照实验推出 ✓）
//   **∴ 我的新怀疑 ✗**（**∴ 由**在线 vs 离线**的对照推出 ✓）★**：
//     **∴ `canvasSig` **只读画布**左上角 4 个像素**✗（`getImageData(0, 0, 4, 1)` ✓）
//       ＋ **∴ 而**笔**画在**落笔位置**✗（**∴ 可能**在画布中部 ✓）
//         ＋ **∴ 若**画布**有滚动／缩放**✗ ⇒ **∴ 左上角**可能**永远是背景 ⇒ **∴ 计数**恒 0** ✓ ★★
//           ＋ **∴ 于是**：**判据**报「**画不出墨**」**✗ 而**渲染**其实**发生了** ✓ ★
//   **∴ 为什么先测不改 ✗**（**先测量再改 ✓）★**：
//     **∴ 直接改 `canvasSig`** 会**改变判据语义** ✗ ⇒ **∴ 必须先**知道**画布上**到底有没有墨** ✓ ★
//       ＋ **∴ 现在 ✗**：**全画布扫描**✗（**∴ 与**左上角**对照 ✓）＋ **∴ 并**打**本地调用增量**
//         （**∴ 在线 13 ⇒ 落笔后**是否增长** ⇒ **∴ 那**直接回答「**渲染**有没有发生」 ✓）★
try {
  const full = await evaluate(`(() => {
      const w = window;
      const c = document.getElementById("board");
      if (!c) return { canvas: "missing" };
      const g = c.getContext("2d");
      const d = g.getImageData(0, 0, c.width, c.height).data;
      let inkAll = 0, inkTop = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (d[i + 3] > 8 && !(d[i] === 255 && d[i + 1] === 255 && d[i + 2] === 255)) inkAll += 1;
      }
      const top = g.getImageData(0, 0, 4, 1).data;
      for (let i = 0; i < top.length; i += 4) {
        if (top[i + 3] > 8 && !(top[i] === 255 && top[i + 1] === 255 && top[i + 2] === 255)) inkTop += 1;
      }
      const st = w.yanshiStats || {};
      const sc = document.getElementById("boardScroll");
      return { inkAll: inkAll, inkTop: inkTop, size: [c.width, c.height],
               cssSize: [c.clientWidth, c.clientHeight],
               scroll: sc ? [sc.scrollLeft, sc.scrollTop, sc.scrollWidth, sc.scrollHeight] : null,
               localCalls: st.localCalls, serverCalls: st.serverCalls,
               applies: st.applies, lastArea: st.lastArea, lastRenderMs: st.lastRenderMs };
    })()`);
  console.log("  后│  · 全画布扫描 = " + JSON.stringify(full));
} catch (e) {
  console.log("  后│  · 全画布扫描失败：" + e);
}
// **负对照必须成立** ✗：未缓存的同源请求断网后仍成功 ⇒ 断网模拟没生效 ⇒ 这一跑没有结论。
if (!(controlProbe && (controlProbe.failed || controlProbe.hanging))) {
  failures.push(`负对照失败：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 判据作废（VOID）`);
}
// **收尾：把 SW 的网络恢复** ✓（页面网络已在上面恢复 ✓；别把这个浏览器实例弄成半残 ✓）。
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
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
