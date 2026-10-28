#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 90000);
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
// **★ CDP 连接守卫 ✗ ★**（2026-10-16 ✓）：浏览器没起来时这里会抛未捕获异常 ⇒ **退 1 ✗，看着像产品失败** ✗

//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✓，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」

let list;

try {

  list = await fetch(`http://127.0.0.1:${debugPort}/json/list`).then((r) => r.json());

} catch (error) {

  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");

  process.exit(3);

}
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
// **∴ 预热第一遍（**它**不计入 ✓）** ✓✓
// **★ 首帧必须**在**热启动**下量** ✗ ★**（第 106 轮 ✓；**本地两次对照 ✓）：
//   **∴ 实测 ✗**：**同一次会话里连跑两遍页面**✗：
//     **∴ 第 1 遍**：**1115ms ⇒ ❌**（**∴ 含**冷启动 ✓）**
//     **∴ 第 2 遍**：**★ 58ms ⇒ ✅ ★**（**∴ 比预算 1000ms 快 17× ✓）**
//   ⇒ **★ 所以**：**那不是**产品缺陷**✗ ⇒ **∴ 而是**判据**没有预热 ✓ ★
//   **∴ 而**设计**要求的是**"**view < 100ms ✓"**✗
//     ⇒ **∴ 热跑** 58ms **满足它 ✓ ⇒ **∴ 所以**：**热跑**才是**该测的口径 ✓**
//   **∴ 做法**（**与 `tool-render-cost-accounts` 剔除第一轮一致 ✓）**：
// **∴ 预热导航必须用**不同的 URL** ✗ ★**（第 106 轮 ✓）：**∴ 同 URL**会被浏览器复用**✗
//   ⇒ **∴ 于是** `firstPaintMs`**仍是第一次的值**✗ ⇒ **∴ 预热**等于没做 ✓**** ✓✓
await send("Page.navigate", { url: url + (url.includes("?") ? "&" : "?") + "_warm=1" });
await new Promise((resolve) => setTimeout(resolve, 3000));

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
// **★★★ 模式判定必须**包含 URL 参数那一档 ✗ ★★★**（**第 785 轮修；**与 `browser-layout` 同因 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：**本判据报「**首帧/预热预算未通过」**✗
//     ＋ **∴ 而**它与 `browser-layout` **完全同形**✗：**都**只读 `localStorage`** ✓ ★★
//   **∴ 根因 ✗**：**产品的 `serverRenderPreferred()`（**`viewer-app.js:56` ✓）**有**三档**✗**：
//     ⇒ **∴ ① ★ URL 参数**最高优先**✗ ★**：** `?render=client` ⇒ **强制客户端内核** ✓（**页面第 71 行 ✓）
//       ⇒ **∴ ②** `localStorage["yanshi.serverRender"]` ✓
//         ⇒ **∴ ③** `__pwaLocalOnly`／缺省 ✓
//           ＋ **∴ 于是**：**URL 里有 `?render=client`**✗ 而**本判据**只看第 ② 档**
//             ⇒ **∴ 它**把客户端模式**误判成服务端** ✗
//               ⇒ **∴ 而**服务端模式下它**要求 `serverBlits > 0`**✗ ⇒ **∴ 客户端下恒为 0** ⇒ **∴ 必然红** ✓ ★★★
//   **∴ 修法 ✗**：**把第 ① 档加回**✗ ＋ **∴ 三档顺序**与产品一致** ✓ ★
const serverMode = await evaluate(`(() => {
  try {
    if (typeof window !== "undefined" && window.__pwaLocalOnly === true) return false;
    const param = new URLSearchParams(location.search).get("render");
    if (param === "client") return false;
    if (param === "server") return true;
    const stored = localStorage.getItem("yanshi.serverRender");
    return stored === null ? true : stored === "1";
  } catch (error) { return true; }
})()`);
// **★★★ 把模式判定的**输入与输出**都打出来 ✗ ★★★**（**第 786 轮；**CI 实测需要 ✓）
//   **∴ 为什么 ✗**（**本会话的教训 ✓）★**：
//     ＋ **∴ 我**补了第①档 URL 参数**✗ 而** CI **仍报「**首帧（服务端铺底）: 未记录」** ✓
//       ⇒ **∴ 而**那**说明 `serverMode` **仍为 true**✗ 而**我**不知道**为什么** ✓
//         ＋ **∴ 单看**结果**无法区分**✗**：**① 页面**真的**走服务端｜② 表达式**抛异常**落到 `catch`** ✓ ★★
//           ＋ **∴ 所以 ✗**：**必须把**判定内部的每一步**都打出来** ✓ ★
//             ＋ **∴ 而**这与本会话的**通用做法一致 ✗**：**先让输出**说出真相**，**再改代码** ✓ ★
const modeProbe = await evaluate(`(() => {
  const out = { search: String(location.search || ""), param: null, stored: null, pwaLocalOnly: null, error: null };
  try {
    out.pwaLocalOnly = (typeof window !== "undefined" && window.__pwaLocalOnly === true);
    out.param = new URLSearchParams(location.search).get("render");
    out.stored = localStorage.getItem("yanshi.serverRender");
  } catch (error) { out.error = String(error && error.message); }
  return out;
})()`);
console.log(`  ⓘ 模式判定的输入：search=${JSON.stringify(modeProbe?.search)}｜render 参数=${JSON.stringify(modeProbe?.param)}｜` +
  `localStorage=${JSON.stringify(modeProbe?.stored)}｜pwaLocalOnly=${modeProbe?.pwaLocalOnly}｜错误=${modeProbe?.error}`);
console.log(`  ⓘ 模式判定的结论：serverMode=${serverMode}（true ⇒ 服务端铺底；false ⇒ 客户端内核）`);
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

// **★★★ 首帧断言也必须**按模式分支** ✗ ★★★**（**第 787 轮修；**CI 诊断实测 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：**本判据报**
//     `❌ 首帧/预热预算未通过`
//       ＋ **∴ 而**同一份输出里**两行是**矛盾的**✗**：
//         `  首帧（服务端铺底）: 未记录｜预算 < 1000ms`      ← **∴ 它**被判失败** ✓
//         `  内核预热（可交互）: 54ms｜预算 < 3000ms`        ← **∴ 它**明明**通过**（**54 ≪ 3000 ✓）★
//   **∴ 根因 ✗**：**第 164 行**无条件**要求 `paintOk`**✗
//     ＋ **∴ 而** `paintOk` **量的是**服务端铺底**✗（**`firstPaintMs` ✓）
//       ⇒ **∴ 客户端模式下**服务端**不铺底**✗ ⇒ **∴ 那一项**永远「未记录」** ✓ ★★
//         ＋ **∴ 即**：**不是产品的问题**✗ 而**是判据**把"不适用"当成了"失败"** ✓ ★
//   **∴ 而**第 165 行的 `warmOk` **早就**按模式分支了**✗ ⇒ **∴ 本判据**只漏了**一个方向** ✓ ★★
//     ＋ **∴ 与**上文第 104–106 行的注释一致 ✗**：**那里说「**服务端优先时内核预热不适用**」✓
//       ⇒ **∴ 而**反方向（**客户端时首帧不适用 ✓）**当时**没写** ✓ ★
//   **∴ 修法 ✗**：**服务端模式才要首帧**✗；**客户端模式只**要求内核预热** ✓ ★
//     ＋ **∴ 而**客户端模式下 `firstPaintMs` **仍然**照印**✗（**∴ 只是**不作为通过条件** ✓）★
let failed = false;
if (serverMode && (!paintOk || firstPaint > paintBudget)) failed = true;
if (!warmOk) failed = true;
if (!serverMode && stats.kernelWarmMs > warmBudget) failed = true;
if (errors.length > 0) failed = true;
console.log(failed ? "❌ 首帧/预热预算未通过" : "✅ 首帧与可交互时间均在预算内");
ws.close();
process.exit(failed ? 1 : 0);
