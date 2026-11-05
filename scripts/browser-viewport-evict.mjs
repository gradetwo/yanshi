// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **★ 总超时守卫 ✗ ★**（2026-10-31）：**浏览器/CDP 起不来时本判据会**一直等**✗ ⇒
//   **∴ 超时**报「自己跑不了」并退 3** ✓（**与产品失败区分 ✓**）；`unref()` ⇒ **跑完不被它拖住** ✓。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（浏览器/CDP 可能没起来）⇒ 退出码 3");
  process.exit(3);
}, Number(process.env.YANSHI_JUDGE_TIMEOUT_MS || 300000));
__judgeTimeout.unref();

// **★ 视口淘汰接线判据（乙(iii) ✓，2026-10-15 ✓）★**
//
// **守的是什么** ✗：内核**早就有** `evict_outside_viewport` ✓（`yanshi-wasm/src/kernel.rs` ✓
// 还有一条测试叫 "**is driven by JS** and reports counts" ✓），绑定也在 `web/wasm/yanshi_wasm.js` 里 ✓
// —— 但**没有任何手写代码调用它** ✗ ⇒ 视口外的 tile **永远留着** ✓（设计文档 13.3 那条没接上 ✓）。
// 本判据守的就是**那条线接上了、而且接得对** ✓。
//
// **两条断言（缺第二件不算完成 ✗）**：
//   ① **确实逐出** ✓：在本地渲染模式下**平移**（视口换到别处 ✓）之后，
//      `window.yanshiStats.viewportEvictions` **必须 > 0** ✓；
//   ② **可见区仍然正确、且没有抖动** ✓：平移出去**再移回来** ⇒ 画布的**像素和必须回到原值** ✓
//      （只差一个"回来"的动作 ⇒ 若逐出把**还要用的** tile 扔了 ✓ 画面就会变 ✗
//       —— 那正是"**CPU↑ 而内存没省**"那种失败形态 ✓，§35 记着 prior ✓）。
//
// **为什么必须用本地渲染模式** ✓：服务端渲染时**没有内核** ✓ ⇒ `setViewport` 是空操作 ✓
//   ⇒ 那样测的就不是这条线了 ✗。杠杆是 `window.__pwaLocalOnly === true`
//   （`viewer-app.js`：localOnly 时一律本地 ✓）。
//
// **为什么能在本机跑** ✓：`/usr/bin/chromium` 在 ✓；**必须由 harness 起 chromium 并传 URL** ✓
//   —— `run-criteria.sh` 自己起（`--remote-debugging-port=$CDP_PORT` ✓），且 **URL 上带 `render=client`** ✓
//   （`run-criteria.sh:329` ✓，第 780 轮修 ✓）。**别即席跑** ✗（§43 ✓）。
//
// **退出码约定**（`run-criteria.sh:376` 第 766 轮 ✓）：
//   `0` 通过｜`1` 产品失败｜`2` 用法错｜**`3` 判据自己跑不了**（没有 CDP／求值失败 ✓）。
//   **AGENTS.md 的硬规则** ✓：`Runtime.evaluate` **必须同时**给 `returnByValue: true`
//   **和** `awaitPromise: true` ✗ —— 只给前者时 async 表达式会**静默返回 `{}`** ✓
//   ⇒ 那看起来像"页面返回空对象" ✓ ⇒ **判据会误判成产品 bug** ✗（第 394–402 轮六轮的血 ✓）。
//   ⇒ 所以下面**两处**都查：给两个开关 ✓ ＋ 求值结果是空对象时**用与产品失败不同的退出码** ✓。
//
// 用法：node scripts/browser-viewport-evict.mjs <viewer-url> [cdpPort]
//   （与 `browser-*` 那一族一致：端口也接受 `CDP_PORT` 环境变量 ✓）

let url = process.argv[2] || "";
if (!url) {
  console.error("用法: node scripts/browser-viewport-evict.mjs <viewer-url> [cdpPort]");
  process.exit(2);
}
const port = process.env.CDP_PORT || "9333";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// **★ 自己建一份够大的文档 ✗ 不要用 runner 的 boot 文档 ★**（2026-11-05 ✓）
//   **∴ 为什么** ✗：runner 给的是 **320×240** ✓ ⇒ 400% 之后视口几乎铺满 ✓
//     ⇒ 平移**动不了**、逐出 **0** ✓（CI 实测：`viewportEvictions = 0`、视口变更 0 次 ✗）
//     ⇒ 判据不是在测逐出，而是在测"文档太小" ✗。
//   **∴ 做法** ✓：用 API 建 **1024×1024** 并落两笔 ✓ ⇒ 视口有余量、逐出可见 ✓。
{
  const parsedUrl = new URL(url);
  const docId = "ve-" + Date.now().toString(36);
  const created = await fetch(`${parsedUrl.origin}/api/documents`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: docId, width: 1024, height: 1024 }),
  }).then((r) => r.json()).catch(() => ({}));
  if (!created || !created.token) voidRun("建不了自己的 1024×1024 文档（拿不到 token）");
  url = `${parsedUrl.origin}/?doc=${docId}&token=${created.token}&render=client`;
  const call = (tool, args) => fetch(`${parsedUrl.origin}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(created.token)}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
  }).then((r) => r.json()).catch(() => ({}));
  await call("draw_stroke", { layer_id: "layer_default", data: { points: [[80, 80], [944, 944]], size: 56, color: { r: 200, g: 30, b: 60, a: 255 } } });
  await call("draw_stroke", { layer_id: "layer_default", data: { points: [[80, 944], [944, 80]], size: 28, color: { r: 30, g: 90, b: 200, a: 255 } } });
}



// **判据自己的坏 ⇒ 退 3**（与"产品红"分开 ✓）
function voidRun(reason) {
  console.error(`❌ 判不了：${reason}（这不是产品问题）⇒ 退出码 3`);
  process.exit(3);
}

// **★ 新建一个**自己的**标签页并连它（2026-10-15 实测修 ✓）★**
//   **为什么** ✗：原来从 `/json/list` 里挑"第一个 page" ✓ ⇒ 本机实测**同一个交付**
//   在独立脚本里 3 秒内就收到 CDP 回复 ✓，而在这条判据里**一条回复都收不到** ✗
//   （socket 明明 `readyState === 1` ✓）—— 当时 CDP 里有 **3 个**目标 ✓
//   ⇒ ⇒ **不再猜是哪一个** ✓：**自己开一个干净的标签页** ✓，连它 ✓。
//   `/json/new` **只接受 PUT** ✗（GET 会报 "Using unsafe HTTP verb" ✓ —— 本轮踩过 ✓）。
let page = null;
try {
  const created = await fetch(`http://127.0.0.1:${port}/json/new?about:blank`, { method: "PUT" });
  if (created.ok) page = await created.json();
} catch (_) {
  /* 老版本 Chrome 不支持 /json/new ⇒ 落回挑一个 ✓ */
}
if (!page || !page.webSocketDebuggerUrl) {
  let targets;
  try {
    targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  } catch (e) {
    voidRun(`连不上 CDP（http://127.0.0.1:${port}/json/list）：${e && e.message}`);
  }
  page = (targets || []).find((t) => t.type === "page");
}
if (!page || !page.webSocketDebuggerUrl) voidRun("CDP 里没有可用的页面目标");

console.log(`  调试目标：${page.url}｜ws = ${String(page.webSocketDebuggerUrl).slice(0, 60)}`);
// **★ `localhost` → `127.0.0.1`（2026-10-15 ✓）★**：CDP 有时给 `ws://localhost:PORT/…` ✓，
//   而本机某些解析路径下 `localhost` 连不上 ✗ ⇒ `new WebSocket` **一直不 open** ✓
//   ⇒ 判据**零输出地挂住** ✗（本轮实测踩到 ✓）。
const wsUrl = String(page.webSocketDebuggerUrl).replace("://localhost:", "://127.0.0.1:");
const socket = new WebSocket(wsUrl);
const pending = new Map();
// **★ `nextId` 必须从 **1** 开始（2026-10-15 实测修 ✓）★**
//   **症状** ✗：socket 连上了 ✓、消息**也确实到了** ✓（诊断打印证明 ✓），
//   但 `Runtime.enable` **永远没有回复** ✗ ⇒ 判据超时被砍 ✓（我此前误判成"连不上" ✗）。
//   **根因** ✗：`nextId = 0` ⇒ 第一条请求的 **`msg.id === 0`** ✓，
//   而监听器写的是 `if (msg.id && pending.has(msg.id))` ✗ ⇒ **0 是 falsy** ✓
//   ⇒ **第一条回复被丢掉** ✗ ⇒ 之后每一条都等一条永远不来的回复 ✓。
//   **旁证** ✓：`browser-offline-draw.mjs` 用的是 **`let nextId = 1`** ✓
//   —— 这正是那个"我早先注意到却略过"的差异 ✗（教训：**差异就是线索** ✓）。
let nextId = 1;
// **★ `ev.data` 可能是 `Blob` 而不是字符串（2026-10-15 实测修 ✓）★**
//   **症状** ✗：socket **连上了** ✓（`readyState === 1` ✓），但**任何 `send` 都没有回复** ✗
//   ⇒ 判据看起来"卡在 Runtime.enable" ✗。
//   **根因** ✗：Node 的全局 `WebSocket`（undici）按**二进制**投递 ✓ ⇒ `ev.data` 是 `Blob` ✓
//   ⇒ `JSON.parse(Blob)` **抛错** ✗ ⇒ **事件处理器每次都在第一行死掉** ✓
//   ⇒ ⇒ 于是**一条回复都处理不了** ✗（而 `open` 事件是另一条路 ✓，所以它照常触发 ✓ ——
//     这正是"连上了却没回复"这个组合的来源 ✓）。
//   **修法** ✓：`string` 直接用 ✓，`Blob` 用 `.text()` ✓，`ArrayBuffer` 用 `Buffer` ✓。
socket.addEventListener("message", async (ev) => {
  let raw = ev.data;
  if (typeof raw !== "string") {
    if (raw && typeof raw.text === "function") raw = await raw.text();
    else raw = Buffer.from(raw).toString("utf8");
  }
  const msg = JSON.parse(raw);
  if (msg.id !== undefined && msg.id !== null && pending.has(msg.id)) {
    pending.get(msg.id)(msg);
    pending.delete(msg.id);
  }
});
// **★ 等它连上：**轮询** `readyState`（2026-10-15 实测修 ✓）★**
//   **为什么不用 `open` 事件** ✗：本机实测——**同一个** WebSocket，独立脚本里
//   `addEventListener("open")` 与 `onopen` **都能连** ✓，但在这条判据里**事件没等到** ✗
//   （零输出／超时 ✓）⇒ ⇒ **不去猜事件时序** ✓，**直接看状态** ✓：`1` = OPEN ✓。
let connected = false;
for (let i = 0; i < 40; i += 1) {
  if (socket.readyState === 1) { connected = true; break; }
  if (socket.readyState === 3) voidRun("WebSocket 已被关闭（readyState = CLOSED）");
  await sleep(250);
}
if (!connected) voidRun(`WebSocket 10 秒内没到 OPEN（readyState = ${socket.readyState}）`);
// **★ `send` 必须带**超时**（2026-10-15 实测修 ✓）★**：原来只等回复 ⇒
//   一旦某一步的回复没来（本机实测踩到 ✓）就**无声挂死** ✗ ⇒ 判据超时被砍 ✓
//   而**看不出卡在哪** ✗ ⇒ ⇒ 现在**每一步都记名 ＋ 10 秒超时** ✓（超时 ⇒ 退 3 ✓，
//   因为那是"判据自己跑不了" ✗，**不是**产品红 ✓）。
const send = (method, params) =>
  Promise.race([
    new Promise((resolve) => {
      const id = nextId++;
      pending.set(id, resolve);
      socket.send(JSON.stringify({ id, method, params: params || {} }));
    }),
    sleep(10000).then(() => {
      voidRun(`CDP \`${method}\` 10 秒内没有回复 ⇒ 判据自身跑不了（已发出 ${nextId} 条）`);
    }),
  ]);

// **★ AGENTS.md 的硬规则：两个开关都要给 ✓ ★**
const evaluate = async (expression) => {
  const r = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (r.error) voidRun(`CDP 报错：${JSON.stringify(r.error).slice(0, 200)}`);
  const res = r.result || {};
  if (res.exceptionDetails) {
    voidRun(`页面里抛异常：${JSON.stringify(res.exceptionDetails).slice(0, 200)}`);
  }
  return res.result ? res.result.value : undefined;
};

console.log("  连上了 ✓ ⇒ 开 Runtime/Page 并导航");
await send("Runtime.enable");
console.log("    Runtime.enable ✓");
await send("Page.enable");
console.log("    Page.enable ✓");
await send("Page.navigate", { url });
console.log("    Page.navigate ✓");
for (let i = 0; i < 60; i += 1) {
  try {
    if (await evaluate('document.readyState === "complete"')) {
      await sleep(900);
      break;
    }
  } catch (_) {
    /* 还没就绪 */
  }
  await sleep(300);
}

// ① 记录渲染模式 ✓（**只作参考，不作硬条件** ✗）：
//   **为什么** ✗：真正的先决条件是"**内核被用起来了**" ✓ —— 那由下面的
//   `__kernelStats.calls > 0` 直接验 ✓。而进本地内核有**两条**路 ✓：
//   `window.__pwaLocalOnly === true`（静态 PWA ✓）或 URL 上的 **`?render=client`** ✓
//   （`viewer-app.js`：`render=client` ⇒ 强制客户端内核 ✓ ⇒ `serverRenderPreferred()` 返回 false ✓）。
const localOnly = await evaluate("window.__pwaLocalOnly === true");
console.log(`  渲染模式：__pwaLocalOnly = ${localOnly}`);

// 等内核就绪 ✓（本地模式下它必须就绪，否则这条判据无意义 ✓）
let stats = null;
for (let i = 0; i < 40; i += 1) {
  stats = await evaluate(
    "(() => { const s = window.yanshiStats || {}; return { calls: (window.__kernelStats||{}).calls || 0, ev: s.viewportEvictions || 0, ch: s.viewportChanges || 0 }; })()",
  );
  if (stats && stats.calls > 0) break;
  await sleep(300);
}
if (!stats || !stats.calls) voidRun("本地内核没有起来（__kernelStats.calls 为 0）⇒ 这条线测不到");
console.log(`  内核已就绪：__kernelStats.calls = ${stats.calls}`);

// **★ 记下接线为何没触发的现场（2026-10-15 ✓）★**
//   `clampViewport` 里的门是 `if (__csLocalApiPromise && state.docId)` ✓
//   ⇒ 哪一个为假就**一个都不会调用** ✓ ⇒ 直接把两个值读出来 ✓。
const wiring = await evaluate(
  "(() => { const st = (window.yanshi || {}).state ? window.yanshi.state() : {}; return { docId: st.docId || null, kernelCalls: ((window.__kernelStats||{}).calls)||0 }; })()",
);
console.log(`  接线前置：state.docId = ${JSON.stringify(wiring.docId)}｜内核调用 ${wiring.kernelCalls}`);

// ② 取画布像素和 ✓（查看器里那个 `kernel` 是 **DOM canvas** ✓ ⇒ 可以直接读像素 ✓）
const PIXEL_SUM = `(() => {
  const canvases = Array.from(document.querySelectorAll("canvas"));
  const c = canvases.sort((a, b) => b.width * b.height - a.width * a.height)[0];
  if (!c || !c.width || !c.height) return null;
  const g = c.getContext("2d");
  if (!g) return null;
  const d = g.getImageData(0, 0, c.width, c.height).data;
  // **按像素算**（每 4 字节一次）✗ 不要用 401 的步长：那样「非零」数的是**白色字节**
  // ⇒ 空画布也有 295 个"非零" ✓，尺寸变一行就少/多几个采样 ✓（本轮实测就是这么误报的）。
  let sum = 0, ink = 0;
  for (let i = 0; i < d.length; i += 4) {
    sum += d[i] + d[i + 1] + d[i + 2];
    if (d[i + 3] > 8 && (d[i] < 245 || d[i + 1] < 245 || d[i + 2] < 245)) ink += 1;
  }
  return { sum, ink, w: c.width, h: c.height };
})()`;

const before = await evaluate(PIXEL_SUM);
if (!before) voidRun("读不到画布像素（canvas 或 2d 上下文缺失）");
console.log(`  平移前：画布 ${before.w}×${before.h}｜采样像素和 ${before.sum}｜墨 ${before.ink}`);

// **★★ 先放大到 400%（2026-10-15 实测修 ✓ —— 这是我漏掉的关键一步 ✗）★★**
//
// **为什么必须** ✗：默认是"适配窗口"的缩放 ✓ ⇒ `viewW == docW` ✓
//   ⇒ `clampViewport` 把 `x` 钳在 `[0, docW − viewW] = [0, 0]` ✓
//   ⇒ **视口根本动不了** ✗ ⇒ 平移后"视图没变"是**正常**的 ✓（不是产品问题 ✓）。
//   `browser-pan-vs-paint.mjs` 的注释就写着这句 ✓：
//   「**先放大到 400%** —— 让画布**超出**窗口，平移才有余量（**否则"视图没变"是正常的** ✓）」。
//   **我照抄了它的拖拽形状 ✗，却漏了这一步** ✓ ⇒ 白花好几轮 ✓（同族教训：**逐字对照** ✓）。
await evaluate(`(() => { const input = document.getElementById("zoomInput");
  if (!input) return false; input.value = "400"; input.dispatchEvent(new Event("change", { bubbles: true }));
  input.blur(); return true; })()`);
await sleep(900);

// **读视口** ✓（`window.yanshi.state().viewport` ✓）—— 比像素和**更能说明"视图动没动"** ✓
const viewBefore = await evaluate("((window.yanshi || {}).state ? window.yanshi.state().viewport : null)");
if (!viewBefore) voidRun("读不到 window.yanshi.state().viewport");
console.log(`  放大后视口 = (${viewBefore.x}, ${viewBefore.y}) ${viewBefore.w}×${viewBefore.h}`);
// **前置条件** ✓：放大之后视口必须**有余量** ✓，否则下面的平移断言没有意义 ✗
// **★ 前置条件别写死 `4096` ✗★**（2026-10-15 修 ✓）：
//   **∴ 为什么** ✗：`4096` 是**本仓默认文档宽** ✓ ⇒ 换成 **1024 宽的文档**时
//     `viewBefore.w >= 4096` **永远为假** ✓ ⇒ 这条"没有平移余量 ⇒ 此刻测不了"的守卫
//     **静默失效** ✗ ✓ ⇒ 判据会去测一个**根本没有余量**的场景 ✓ ⇒ 手势动不了 ⇒
//     报"驱动问题" ✗（§49 之补四十 ✓）。**∴ 改成**按**实际文档尺寸**判断 ✓。
const docW =
  (await evaluate("((window.yanshi || {}).state ? window.yanshi.state().docSize.w : null)")) || 4096;
if (viewBefore.w >= docW && viewBefore.x === 0 && viewBefore.y === 0) {
  voidRun("放大之后视口仍然铺满文档（没有平移余量）⇒ 这条判据此刻测不了 ✗");
}

// **★ ② 的基准必须**在放大之后**取（2026-10-15 修 ✓）★**
//   **为什么** ✗：上面那个 `before.sum`（`:196` 一带 ✓）是**放大之前**的像素和 ✓ ⇒
//   **放大本身就会改变画面** ✗ ⇒ 拿它当基准 ⇒ 断言②**必然误报**"画面变了" ✗
//   （台账里那条「**[另一处待修]** …要在**放大之后**再取基准 ✓」说的就是这件事 ✓）。
//   **正确基准** ✓：**同一缩放状态下、平移之前**的像素和 ✓。
// **等画面稳定再取基准** ✗（2026-11-05 ✓）：放大后内核/瓦片是**异步**上屏的，
// 900ms 固定等待在慢机器上会**在渲染中途取基准** ⇒ 之后"移回来"看到的是**更完整**的画面
// ⇒ 差值被误判成"逐出把还要用的 tile 扔了" ✓（本机实测 295→298、75225→75990）。
// 判据：连续 3 次采样相同才算稳定（有界：最多 ~12 秒）。
const settleSum = async () => {
  let prev = null;
  let same = 0;
  for (let i = 0; i < 60; i += 1) {
    const current = await evaluate(PIXEL_SUM);
    if (prev && current && current.sum === prev.sum && current.ink === prev.ink) same += 1;
    else same = 0;
    prev = current;
    if (same >= 6 && i >= 6) return current;
    await sleep(200);
  }
  return prev;
};
const baseSum = await settleSum();
if (!baseSum) voidRun("放大之后读不到画布像素和");
console.log(
  `  平移前基准（**放大后** ✓）：采样像素和 ${baseSum.sum}｜墨 ${baseSum.ink}`,
);

// ③ 选中手形工具并平移 ✓
const picked = await evaluate(
  `(() => { const b = document.querySelector('button[data-tool="pan"]'); if (!b) return null; b.click(); return b.dataset.tool; })()`,
);
if (picked !== "pan") voidRun(`选不到手形工具（实测 ${JSON.stringify(picked)}）`);

// **★ 必须拖**那块大画布**（2026-10-15 实测修 ✓）★**
//   **症状** ✗：工具选中了 ✓（`data-tool="pan"` ✓），但拖完**视口变更 0 次** ✓、
//   画面像素和**一丝不变** ✗ ⇒ 判据报"没有逐出" ✗。
//   **根因** ✗：我取位置时用 `document.querySelector("canvas")`（**第一个**画布 ✗），
//   而取像素和时用**最大的**那个 ✓ ⇒ ⇒ 若第一个是**小预览图** ✓ 拖它当然不动视口 ✗。
//   **修法** ✓：**两处用同一块画布**（最大的那个 ✓）。
const box = await evaluate(
  `(() => {
     const cs = Array.from(document.querySelectorAll("canvas"));
     const c = cs.sort((a, b) => b.width * b.height - a.width * a.height)[0];
     if (!c) return null;
     const r = c.getBoundingClientRect();
     return {
       x: Math.round(r.left + r.width / 2),
       y: Math.round(r.top + r.height / 2),
       // **★ 把画布尺寸也带回来 ✗★**（2026-10-15 ✓）：拖拽距离要**按它缩放** ✓
       //   ⇒ 否则写死的 420×300 会让指针**跑出窗口** ✗ ⇒ 手势丢失 ✗（§49 之补四十一 ✓）。
       w: Math.round(r.width),
       h: Math.round(r.height),
     };
   })()`,
);
if (!box) voidRun("拿不到画布的位置");

// **平移很远** ✓：视口换到别处 ⇒ 旧位置的 tile 应当落在视口外 ✓
// **★ 拖拽距离按**画布尺寸**缩放 ✗★**（2026-10-15 修 ✓）：
//   **∴ 为什么** ✗：原来写死 `-420/-300` ✓ ⇒ 而视口常常只有 **231×179** 或 **329×256** ✓
//     ⇒ 从**中心**拖 420×300 ⇒ 终点落在 **(-256, -172)** 一带 ✗ ⇒ **指针跑出浏览器窗口** ✗
//     ⇒ 窗外的事件**丢失** ✗ ⇒ 视口**纹丝不动** ⇒ 判据报"① 视口压根没动"✗
//       （**它报得对** ✓：那是**驱动**问题，不是产品 ✗ —— 见 §49 之补三十九／四十一 ✓）。
//   **∴ 改成** ✓：**0.6 倍画布** ⇒ 一定落在窗口内 ✓，又足够远（0.6 倍视口 ⇒ 必然移动视口 ✓）。
const DX = -Math.max(24, Math.round((box.w || 400) * 0.6));
const DY = -Math.max(24, Math.round((box.h || 300) * 0.6));

// **★ 一次手势里「拖出去再拖回来」✗ 不要两次按下 ★**（2026-11-05 ✓）
//   **∴ 实证 ✗**：两次按下时第二次 pointerdown 会被吞（同一坐标刚释放 ⇒ 双击/捕获状态）
//     ⇒ 视口**没回来**（本机实测 viewBefore=(407,442) vs viewAfter=(534,526) ✓）
//     ⇒ ② 比的是两块**不同视口** ✗ 红得没有意义 ✓。
//   **∴ 做法** ✓：按住 → 拖远（读 viewFar＋逐出计数）→ **同一手势**拖回原点 → 松开
//     ⇒ 视口必须回到 viewBefore ✓（回不去 = 产品累计取整有问题 ⇒ 判红 ✓）。
await send("Input.dispatchMouseEvent", { type: "mousePressed", x: box.x, y: box.y, button: "left", buttons: 1, clickCount: 1 });
for (let i = 1; i <= 8; i += 1) {
  await send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: box.x + Math.round((DX * i) / 8),
    y: box.y + Math.round((DY * i) / 8),
    button: "left",
    buttons: 1,
  });
  await sleep(60);
}
const viewFar = await evaluate("((window.yanshi || {}).state ? window.yanshi.state().viewport : null)");
const afterPan = await evaluate(
  "(() => { const s = window.yanshiStats || {}; return { ev: s.viewportEvictions || 0, ch: s.viewportChanges || 0 }; })()",
);
console.log(`  拖到远处：视口 = (${viewFar ? viewFar.x : "?"}, ${viewFar ? viewFar.y : "?"}) ${viewFar ? viewFar.w : "?"}×${viewFar ? viewFar.h : "?"}｜视口变更 ${afterPan.ch} 次｜累计逐出 ${afterPan.ev} 块`);

for (let i = 7; i >= 0; i -= 1) {
  await send("Input.dispatchMouseEvent", {
    type: "mouseMoved",
    x: box.x + Math.round((DX * i) / 8),
    y: box.y + Math.round((DY * i) / 8),
    button: "left",
    buttons: 1,
  });
  await sleep(60);
}
await send("Input.dispatchMouseEvent", {
  type: "mouseReleased",
  x: box.x,
  y: box.y,
  button: "left",
  buttons: 0,
  clickCount: 1,
});
await sleep(600);

// 移回来之后**等稳定**再比（异步渲染可能还在上屏）。
const backSum = await settleSum();
if (!backSum) voidRun("移回来之后读不到画布像素");
console.log(`  移回后：采样像素和 ${backSum.sum}｜墨 ${backSum.ink}`);

const fails = [];
// **★ 视口是否真的动了（读状态，最直接 ✓）★**
const viewAfter = await evaluate("((window.yanshi || {}).state ? window.yanshi.state().viewport : null)");
// **「动了」看拖远那一刻**；「回到原位」看松手后 ✓（两件事，别混）。
const moved = !!viewFar && (viewFar.x !== viewBefore.x || viewFar.y !== viewBefore.y);
const restored = !!viewAfter && viewAfter.x === viewBefore.x && viewAfter.y === viewBefore.y
  && viewAfter.w === viewBefore.w && viewAfter.h === viewBefore.h;
console.log(`  松手后视口 = (${viewAfter ? viewAfter.x : "?"}, ${viewAfter ? viewAfter.y : "?"}) ${viewAfter ? viewAfter.w : "?"}×${viewAfter ? viewAfter.h : "?"}｜拖远时动了 = ${moved}｜回到原位（含尺寸）= ${restored}`);

// **⓪ 手势必须真的往返** ✓：回不去 ⇒ 后面的像素比对没有意义（先修驱动/或判产品累计取整）。
if (!restored) {
  fails.push(`拖回后视口没有回到原位（before=(${viewBefore.x},${viewBefore.y}) ${viewBefore.w}×${viewBefore.h}`
    + ` after=(${viewAfter ? viewAfter.x : "?"},${viewAfter ? viewAfter.y : "?"}) ${viewAfter ? viewAfter.w : "?"}×${viewAfter ? viewAfter.h : "?"}）`
    + `⇒ ② 的像素比对无效（平移不应改变视口尺寸/位置）`);
}

// **① 确实逐出** ✓
if (!moved) {
  fails.push(
    `① **视口压根没动** ✗：(${viewBefore.x}, ${viewBefore.y}) ⇒ (${viewAfter ? viewAfter.x : "?"}, ${viewAfter ? viewAfter.y : "?"})` +
      ` ⇒ 这是**判据的驱动**问题（不是产品）✗：平移手势没走到 \`clampViewport\` ✓`,
  );
} else if (!(afterPan.ev > 0)) {
  fails.push(
    `① **没有发生任何逐出** ✗：平移后 \`yanshiStats.viewportEvictions\` = ${afterPan.ev}` +
      `（视口变更 ${afterPan.ch} 次）⇒ \`evict_outside_viewport\` 仍然**没被调用** ✗`,
  );
}
// **② 回来之后画面一致** ✓
if (baseSum.w !== backSum.w || baseSum.h !== backSum.h) {
  fails.push(`平移前后画布尺寸不同（${baseSum.w}×${baseSum.h} → ${backSum.w}×${backSum.h}）⇒ 像素比对无效（判据必须同尺寸）`);
} else if (baseSum.sum !== backSum.sum) {
  fails.push(
    `② **平移回来之后画面变了** ✗：像素和 ${baseSum.sum} → ${backSum.sum}` +
      `（墨 ${baseSum.ink} → ${backSum.ink}）⇒ 逐出把**还要用的** tile 扔了 ✗（抖动）` +
      `（基准取在**放大之后、平移之前** ✓ —— 放大本身会改变像素和 ✗）`,
  );
}

if (fails.length) {
  console.error("❌ 没通过：");
  for (const f of fails) console.error(`   ${f}`);
  process.exit(1);
}
console.log(
  `✓ 视口淘汰接线成立：平移触发了 ${afterPan.ev} 块逐出 ✓，且移回后画面逐像素一致（${baseSum.sum}）✓`,
);
process.exit(0);
