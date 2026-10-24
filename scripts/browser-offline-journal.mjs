#!/usr/bin/env node

// **★ 总超时守卫 ✗ ★**（2026-10-16 ✓）：**本判据要连 CDP 浏览器** ✓ ⇒ 浏览器起不来时它可能**一直等**
//   （实测：CI 的判据分片被这种判据**拖到跑不完** ✗ ⇒ 我这边十几轮拿不到判定 ✓）。
//   **∴ 本仓约定**（AGENTS.md）：「判据必须能说**我自己坏了** ✗，用**与产品失败不同的退出码**报出来 ✓（现用 exit(3) ✓）」
//   ⇒ **∴ 超时就报「无法判断」并退 3** ✓ —— **不冒充产品失败** ✗、**也不永远挂着** ✗ ★
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 90000);
// **离线写队列（journal / outbox）判据** ✓ —— 离线优先里"**写**"的那一半 ✓。
//
// **为什么必须有这一条** ✗：`browser-offline-shell.mjs` 只证明了"断网还能画" ✓
//（像素走本地内核 ✓、笔刷面板走随包清单 ✓），但那一笔**到了服务端没有** ✗ ——
// 断网期间的操作全靠**本地内核的一次预览** ✓；刷新一下、换台机器，它就没了 ✗。
// 这一条测的正是"**断网时的改动不会丢，联网后按顺序补交**" ✓。
//
// 判据分七段（每段都能红 ✓ —— 见各段里的断言 ✓）：
//   ① 断网后一次**改文档的工具调用**必须进队列 ✓，载荷**逐字段对得上** ✓，界面必须说"排队"而非"已保存" ✓；
//   ② 再排一条 ✓，顺序必须是**入队顺序** ✓（自增主键 ✓，不是墙钟 ✓）；
//   ③ 联网后补交 ✓：**服务端最终状态 == 同样的动作在线做一遍** ✓（拿一份参照文档逐原子比对 ✓），
//      队列被清空 ✓，**再补交一次不会重复应用** ✓；
//   ④ **真实指针笔迹**（产品路径 ✓）断网时也进队列 ✓，联网后服务端里**恰好一条**那个 id ✓；
//   ⑤ 队列**有界** ✓：到上限就**拒绝并说出来** ✗（不静默丢 ✓、不悄悄丢最旧的 ✓）；
//   ⑥ 服务端**拒绝**某一条时：报告出来 ✓、**仍留在队列里** ✗（不消失 ✓）；
//   ⑦ 服务端这份文档**已经变了**时：**暂停补交并告诉用户** ✗（不静默覆盖 ✓，也不做合并 ✓）。
//
// 用法（与其它 browser-* 一致）：`node scripts/browser-offline-journal.mjs <viewer-url> <base> <token> <cdp-port>`
//   `argv[3]` 是 BASE ✓（不是端口 ✗ —— 那是我在 8 条判据里踩过的坑 ✓，端口只从 `CDP_PORT` 取 ✓）。
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
const base = process.argv[3] || (url ? new URL(url).origin : "");
const port = process.env.CDP_PORT || "9333";
if (!url) { console.error("用法: node scripts/browser-offline-journal.mjs <viewer-url> <base> [token] [cdpPort]"); process.exit(2); }
const parsed = new URL(url);
const doc = parsed.searchParams.get("doc");
const token = process.argv[4] || parsed.searchParams.get("token");
if (!doc || !token) { console.error("判据需要 ?doc=..&token=..（前置不成立 ⇒ 不当作通过 ✗）"); process.exit(1); }

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const check = (ok, message) => { if (ok) console.log("     ✓ " + message); else { console.log("     ✗ " + message); failures.push(message); } };
const section = (name) => console.log("  " + name);

// ——— Node 侧直连服务端（用来造"在线参照"与"另一个人改了文档"✓）———
const httpJson = async (path, body) => (await fetch(base + path, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body || {}),
})).json();
const callToolHttp = (d, t, tool, args) => httpJson(`/api/tools/${tool}?doc=${d}&token=${t}`, args);
const atomsOf = async (d, t) => {
  const response = await fetch(`${base}/api/atoms?doc=${d}&token=${t}&since=0&limit=1000`);
  return ((await response.json()).atoms) || [];
};
/// **投影**：只留"我控制得了的"字段 ✓（`id`/`seq`/`timestamp`/`actor`/`session` 是每次不同的 ✓）。
/// **剔除 `create_document`** ✗ —— 它的载荷里带**文档 id 与时间** ✓，
/// 两份文档天生不同 ✓（第一版就是栽在这里 ✓：kind 序列一模一样而 payload 不等 ✓）。
const project = (atoms) => atoms.filter((atom) => atom.kind !== "create_document")
  .map((atom) => ({ kind: atom.kind, payload: atom.payload }));
/// **规范化**：递归按键名排序 ✓ —— **服务端的 `serde_json` 键是排过序的** ✓（BTreeMap ✓），
/// 而页面里 `JSON.stringify` 用的是**插入顺序** ✓ ⇒ 直接比字符串会把"同一份载荷"判成不等 ✗
///（第一版就是这样：`{"data":…,"layer_id":…}` vs `{"object_id":…,"layer_id":…}` ✓）。
const canonical = (value) => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = canonical(value[key]);
    return out;
  }
  return value;
};
const sameJson = (left, right) => JSON.stringify(canonical(left)) === JSON.stringify(canonical(right));
/// 第一条不同在哪（诊断用 ✓ —— 只印"不等"是不够的 ✗，得指出**差在第几条、差在哪** ✓）。
const firstDiff = (left, right) => {
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const a = JSON.stringify(canonical(left[index])), b = JSON.stringify(canonical(right[index]));
    if (a !== b) return { index: index, page: a, ref: b };
  }
  return null;
};

(async () => {
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
  if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题，也不是通过）"); process.exit(3); }
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  let nextId = 1; const pending = new Map();
  socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
  await new Promise((open) => { socket.onopen = open; });
  const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
    // **判据自身抛错时必须看得见原因** ✗（第 1110 轮）：原来只抛 "页内求值抛错" ✗，
    // 栈被 runner 的回显截断 ⇒ 无从定位 ✓。这里把 exceptionDetails 全文打出来 ✓。
    if (result && result.exceptionDetails) {
      console.log("  ‼️ 页内异常全文 = " + JSON.stringify(result.exceptionDetails));
    }
    if (result.result && result.result.exceptionDetails) {
      const __d = ((result.result.exceptionDetails || {}).exception || {}).description || "无描述";
      console.log("  ‼️ 页内 TypeError 全文 = " + __d);
      throw new Error("页内求值抛错：" + __d);
    }
    return result.result && result.result.result ? result.result.result.value : undefined;
  };
  const offline = () => send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
  const online = () => send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  // **★ 等一个离线资产进 SW 缓存 ✗ ★**（第 407 轮 ✓）：
  //   **∴ 为什么 ✗**：**SW 的预缓存是**异步**的**✗
  //     ⇒ **∴ 若**页面刚加载就断网**✗ ⇒ **∴ `import` **可能失败** ✓
  //       ⇒ **∴ 于是**判据**误判产品** ✓
  //         **∴ 实测（**第 406 轮 ✓）**：**缓存里**确实有 `brush-local.js`**（**200／4491 ✓）** ✓
  //           ⇒ **★ 所以**：**断网前**要**显式等它**✗（**最多 20 秒 ✓）** ✓
  // **★ 必须同时等「SW 已接管页面」✗ ★**（第 410 轮 ✓；**实测根因 ✓）：
  //   **∴ 实测（**决定性 ✓）**✗**：
  //     ```
  //     ① 缓存命中 = true
  //     ★ ② controller = false｜SW scope = 无 ★     ← ★ SW 还没接管页面 ★
  //     ③ 在线 fetch = {ok:true, status:200, len:5454}
  //     ★ ④ 离线 fetch(/brush-local.js) = {ok:true, status:200, len:5454} ★
  //     ★ ⑤ 离线 import(/brush-local.js) = "OK:baseFromKernel,fetchBrushText,paintWithKernel" ★
  //     ```
  //     **⇒ ★ 所以 ✗ ★**：**缓存**命中**✗ ＋ **离线 `import` **也能成功**** ✓
  //       ⇒ **∴ 而**那一刻 `controller === false`**✗
  //         ⇒ **∴ 所以**：**请求**没有经过 SW**✗ ⇒ **∴ 走的是**网络 ＋ HTTP 缓存** ✓
  //           ⇒ **∴ 于是**：**HTTP 缓存命中**就成功**✗、**没命中**就失败** ✓
  //             ⇒ **∴ 那**正是「**有时成功有时失败**」的来源** ✓ ★**** ✓✓
  //   **∴ 修法 ✗**：**等到** `navigator.serviceWorker.controller` **为真**✗
  //     ⇒ **∴ 于是**判据**在**真正受控**之后**才断网** ✓ ★**** ✓✓
  //   **∴ 两面 ✗**：**收益**：**判据测的是**真正的 SW 离线能力** ✓
  //     ＋ **∴ 代价 ✗**：**多等一点时间**（**上限 20 秒 ✓）★**** ✓✓
  // **∴ 同上：等待预算可配置 ✓**（**第 777 轮 ✓）
const waitForPrecache = async (path, timeoutMs = Number(process.env.YANSHI_WAIT_MS || 120000)) => {
    const probe = `(async () => { try { const r = await caches.match(${JSON.stringify(path)}); return { hit: !!r, controlled: !!navigator.serviceWorker.controller }; } catch (e) { return { hit: false, controlled: false }; } })()`;
    const started = Date.now();
    for (;;) {
      const state = await evaluate(probe);
      const hit = state && state.hit === true;
      const controlled = state && state.controlled === true;
      if (hit && controlled) {
        console.log("     · 离线资产已进缓存 ＋ SW 已接管页面：" + path);
        return true;
      }
      if (hit && !controlled && Date.now() - started > timeoutMs) {
        console.warn("     ⚠️ 缓存已命中但 SW 一直没接管页面（controller 仍为假）⇒ 离线结论可能不成立");
        return false;
      }
      if (Date.now() - started > timeoutMs) {
        console.warn("     ⚠️ 等 " + path + " 进缓存超时（" + timeoutMs + "ms）⇒ 离线结论可能不成立");
        return false;
      }
      await sleep(250);
    }
  };

  // **★ 第 739 轮：分段导航——这条判据**必须跨两种渲染模式**✗ ★**（**台账第 449 节 ✓**）：
  //   **∴ 为什么 ✗**：**产品把「写队列」只留在了服务端渲染一侧**✗
  //     （**`callTool → !serverRenderPreferred() ⇒ callToolLocal`✓：直接进 IndexedDB ✗**
  //      **永不入队 ✗**），**而指针笔迹的 ULID 原子**需要 WASM 内核**✗
  //     （**`initWasm` 第一行：服务端渲染模式直接 return ✗**）
  //       ⇒ **∴ 于是**：**单一导航永远过不了七段**✗（**客户端：①②③⑤⑥⑦ 结构性 0 条 ✓；
  //         服务端：④ 没内核 ✓）** ★**** ✓✓
  //   **∴ 所以 ✗**：**工具路径段（①②③⑤⑥⑦）在 `render=server` 的页面跑**✓，
  //     **指针原子段（④）在 `render=client` 的页面跑**✓（**层由判据经服务端 API 先建好 ✓，
  //     否则补交会被拒——那测的就成了⑥ ✗**）。
  //   **⚠ 静态 PWA 页面没有这个开关**✗：**`__pwaLocalOnly` 在 URL 参数之前就把模式钉死为本地**
  //     （**viewer-app.js:59–60 ✓**）⇒ **分段必须在**服务端托管的页面**上做** ✓（**编排给 argv[2] ✓）。
  const serverUrl = url.includes("render=client") ? url.replace("render=client", "render=server")
    : (url.includes("?") ? url + "&render=server" : url + "?render=server");
  const gotoUrl = async (target) => {
    await send("Page.navigate", { url: target });
    for (let i = 0; i < 40; i++) {
      await sleep(300);
      try { if (await evaluate('document.readyState === "complete"')) break; } catch (_) { /* 继续等 ✓ */ }
    }
  };
  const waitKernelReady = async () => {
    for (let i = 0; i < 40; i++) {
      if (await evaluate('window.yanshiKernelReady === true')) return true;
      await sleep(500);
    }
    return false;
  };
  // **把整段"启动＋清旧外壳＋等接口"做成函数**✓（**第 739 轮 ✓：**分段导航后每一页都要走一遍**✗，
  // **原来内联一次 ✗ 复制两份一定漂移 ✓——内核 ② ✓**）。
  const bootViewer = async (navUrl) => {
    await send("Network.setCacheDisabled", { cacheDisabled: true });
    await gotoUrl(navUrl);
    const ready = await evaluate(`(async () => {
      try {
        const registration = await Promise.race([
          navigator.serviceWorker.ready,
          new Promise((r) => setTimeout(() => r(null), 12000)),
        ]);
        return registration ? (registration.active ? "active" : "installed") : "timeout";
      } catch (error) { return "error:" + error; }
    })()`);
    console.log("  前置：Service Worker = " + ready + "（" + navUrl.replace(/token=[0-9a-f]+/, "token=…") + "）");
    check(ready === "active", "Service Worker 必须已激活（否则断网判据不成立）");
    // **清掉可能残留的队列** ✓（同一浏览器 profile 复用时 ✓ —— 否则计数会从中间开始 ✓）。
    // **先清掉旧外壳**（第 1115 轮定案）：SW 的 SHELL 含 /viewer-app.js 与 /viewer.css，
    // 而复用的浏览器 profile 会留下旧缓存 ⇒ 浏览器里的 window.yanshi 只有 18 个旧键
    // （没有 outbox 那五个），而服务端发出的资产是新的（curl 已确认）✗。
    // 所以：注销所有 Service Worker、删掉所有 caches，然后重载页面，再等就绪 ✓。
    const purged = await evaluate(`(async () => {
      const out = { workers: 0, caches: 0 };
      try {
        const regs = await navigator.serviceWorker.getRegistrations();
        for (const r of regs) { await r.unregister(); out.workers += 1; }
      } catch (e) { out.workerError = String(e); }
      try {
        const keys = await caches.keys();
        for (const k of keys) { await caches.delete(k); out.caches += 1; }
      } catch (e) { out.cacheError = String(e); }
      return out;
    })()`);
    console.log("  清旧外壳 = " + JSON.stringify(purged));
    await send("Page.reload", { ignoreCache: true });
    for (let i = 0; i < 60; i++) {
      await sleep(250);
      try { if (await evaluate('document.readyState === "complete"')) break; } catch (_) { /* 继续等 ✓ */ }
    }
    // **等 window.yanshi 挂上再调用**（第 1112 轮：真错是 `discardOutbox is not a function`，
    // 而方法确实存在 ⇒ 是**调用太早** ✗）。
    let ifaceReady = false;
    for (let i = 0; i < 60; i++) {
      let ok = false;
      try {
        ok = await evaluate('typeof (window.yanshi && window.yanshi.discardOutbox) === "function"');
      } catch (_) { /* 继续等 ✓ */ }
      if (ok) { ifaceReady = true; break; }
      await sleep(250);
    }
    console.log("  前置：window.yanshi.discardOutbox 就绪 = " + ifaceReady);
    if (!ifaceReady) {
      // **诊断阶梯**（第 1113/1114 轮）：脚本若中途抛错，后半段 `window.yanshi = {...}` 永不执行 ⇒
      // 按定义行号递增的全局能夹出出错行段 ✓。
      const ladder = await evaluate(`(() => {
        const names = ["yanshiStats","yanshiKernel","yanshiKernelReady","yanshiDebugBlit","yanshiCallTool",
                       "yanshiMediumBatch","yanshiApplyScore","yanshiListLayers","yanshiBrushArea",
                       "yanshiDockFocus","yanshiDock","yanshiRightTabs","yanshi"];
        const out = {};
        for (const n of names) { try { out[n] = typeof window[n]; } catch (e) { out[n] = "throws"; } }
        out.__marks = window.__appMarks || null;
        return out;
      })()`);
      console.log("  全局阶梯 = " + JSON.stringify(ladder));
      throw new Error("window.yanshi.discardOutbox 迟迟未挂上 ⇒ 判据无法进行");
    }
    await evaluate("window.yanshi.discardOutbox()");
    await evaluate("window.yanshi.refreshOutbox()");
  };

  await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
  // **第 739 轮：首屏直接起在**服务端渲染**页**✗（**①②③⑤⑥⑦ 的主战场 ✓**）；
  // **④ 在 ③ 之后单独切到 `render=client`**✓（**见下 ✓）**。
  await bootViewer(serverUrl);

  // ——— 参照文档：**同样的动作在线做一遍** ✓（③要拿它做等价比对 ✓）———
  const refDoc = doc + "_ref";
  const refToken = (await httpJson("/api/documents", { doc_id: refDoc, width: 320, height: 240 })).token;
  const LAYER = "eq_layer", LAYER2 = "eq_layer2", OBJECT = "obj_eq";
  const DRAW_ARGS = {
    layer_id: LAYER,
    object_id: OBJECT,
    // **★ 每个点**必须 3 个数** ✗ ★**（第 411 轮 ✓；**实测根因 ✓）：
    //   **∴ 症状 ✗**：**入队返回**报**
    //     `内核未产出像素：请求不是合法 JSON：invalid length 2, expected an array of length 3`** ✓
    //     ⇒ **∴ 而**那不是**产品缺陷**✗ ⇒ **∴ 而是**本判据**少给了一个分量** ✓ ★**** ✓✓
    //   **∴ 形状 ✗**：`[x, y, pressure]`**✗（**第三个数是**压力 ✓）
    //     ⇒ **∴ 权威样例**见 `scripts/browser-export-full-document.mjs`** ✓ ★**** ✓✓
    //   **∴ 教训 ✗**：**同一个参数形状**在**多处**出现**✗ ⇒ **∴ 一处改对**不代表**别处也对** ✓
    //     ⇒ **∴ 所以**：**新增判据**要**抄权威样例**✗（**不凭记忆 ✓）** ★**** ✓✓
    data: { points: [[12, 16, 0.5], [48, 40, 0.8], [92, 84, 0.9]], size: 7, color: { r: 10, g: 20, b: 30, a: 255 }, hardness: 0.7, smooth: true },
  };
  const createLayerArgs = (id, name) => ({ layer_id: id, name: name });

  // ①-a 在线：两边各建同一层（离线时 `draw_stroke` 需要一个已存在的图层 ✓）。
  await evaluate(`window.yanshiCallTool("create_layer", ${JSON.stringify(createLayerArgs(LAYER, "eq"))})`);
  const refCreate = await callToolHttp(refDoc, refToken, "create_layer", createLayerArgs(LAYER, "eq"));
  check(refCreate.ok === true, "参照文档：在线 create_layer 成功（前置 ✓）");

  // ① 断网 ⇒ 一次改文档的工具调用必须进队列
  section("① 断网：改文档的调用进队列，界面显示「排队」而不是「已保存」");
  // **★ 断网前必须**确认离线资产已进缓存** ✗ ★**（第 407 轮 ✓；**实测根因 ✓）：
  //   **∴ 为什么 ✗**：**离线落笔要 `import("/brush-local.js")`** ✗
  //     ＋ **∴ 它**由 SW **预缓存** ✓
  //       ⇒ **∴ 若**在**预缓存完成之前**就断网**✗
  //         ⇒ **∴ 于是** import **必然失败**✗（**`Failed to fetch dynamically imported module` ✓）
  //           ⇒ **∴ 于是**判据**误报「**产品把笔丢了**」 ✓
  //             **∴ 而**实测（**第 406 轮 ✓）**：**缓存里**确实有它**（**命中 200／4491 字节 ✓）** ✓
  //               ⇒ **★ 所以**：**先等它进缓存**✗ ⇒ **∴ 于是**判据测的是**真正的离线能力** ✓ ★**** ✓✓
  await waitForPrecache("/brush-local.js");
  await offline();
  await sleep(300);
  // **负对照**（本次审计加的 ✓）：断网后页面自己发一个**同一来源、已知不会被缓存**的请求 ⇒
  // 它**必须失败** ✓。为什么它不可能被缓存 ✓：`service-worker.js` 对 `/api/` 前缀直接 `return`
  //（不 respondWith ✓）⇒ **永远进不了 SW 的 Cache Storage** ✓；路径不存在（服务端 404 ✓）
  // ＋ `cache: "no-store"` ＋每次唯一 nonce ⇒ **也进不了浏览器 HTTP 缓存** ✓。
  // 它若居然成功 ⇒ 说明"断网"没真的生效 ⇒ **本判据作废（VOID）**，不是产品通过 ✗。
  const NEGATIVE_CONTROL = "/api/__offline_negative_control__?nonce=" + Date.now();
  // **不 `await` 这一次求值** ✓ —— 本判据对时序敏感（入队时读 `window.yanshiStats.serverHead`
  // 当基线，多一次 CDP 往返会改变它何时被后台更新），所以让这个必然失败的请求**与入队那一步并发**，
  // 结论稍后再取 ✓（请求仍然真的由页面发出 ✓）。
  // **★ 第 738 轮：探测改走 `XMLHttpRequest` ✗ ★**（**根因写全在台账第 450 节 ✓**）：
  //   **∴ 旧注释把 501 归因成「**SW 绕过 ✓」✗ —— 那次 `fetch` **根本没出页面** ✓：
  //     **静态 PWA** 的引导页 `installLocalApi` **覆写 `window.fetch`**✗
  //     （**`web/index.html:74–76` ✓＋`api-local.js:4836–4854` ✓**——未映射的 `/api/` 本地回 501 ✓）
  //       ⇒ **∴ 于是**：`knownCdpLimit` 那条「**不算失败**」的分支**掩盖的是接管**✗
  //         **不是** CDP 限制 ✓ ⇒ **∴ 归因错了** ✗
  //   **∴ XHR 没有被覆写**（**`grep XMLHttpRequest api-local.js` = 0 ✓）⇒ **∴ 真实经过**
  //     **SW＋网络栈**✗ ⇒ **∴ 断网必失败**✓；**若仍成功**✗ 才是**真的没断下去**（**offline 没生效／
  //     SW 上下文绕过 ✓**）⇒ **∴ `knownCdpLimit` 分支保留**✗ 但含义**变干净了** ✓ ★**** ✓✓
  const controlPending = send("Runtime.evaluate", {
    awaitPromise: true, expression: `(() => {
      const onLine = navigator.onLine;
      const swControlled = !!(navigator.serviceWorker && navigator.serviceWorker.controller);
      return new Promise((resolve) => {
        const xhr = new XMLHttpRequest();
        xhr.open("GET", ${JSON.stringify(NEGATIVE_CONTROL)}, true);
        xhr.onload = () => resolve({ failed: false, status: xhr.status, bytes: -1, onLine, swControlled, via: "xhr" });
        xhr.onerror = () => resolve({ failed: true, status: xhr.status, onLine, swControlled, via: "xhr" });
        xhr.send();
      });
    })()`,
    returnByValue: true, awaitPromise: true,
  });
  const queued = await evaluate(`window.yanshiCallTool("draw_stroke", ${JSON.stringify(DRAW_ARGS)})`);
  const entries1 = await evaluate("window.yanshi.outboxEntries()");
  const ui1 = await evaluate(`(() => {
    const n = document.getElementById("outbox");
    const b = document.getElementById("outboxBar");
    return { state: n.dataset.state, text: n.textContent,
             banner: b && !b.hidden && document.getElementById("outboxText") ? document.getElementById("outboxText").textContent : null };
  })()`);
  console.log("     入队返回 = " + JSON.stringify(queued).slice(0, 160));
  console.log("     界面 = " + JSON.stringify(ui1));
  const controlProbe = (await controlPending)?.result?.result?.value;
  console.log("     负对照（未缓存接口必须失败）= " + JSON.stringify(controlProbe));
  // **★ 分清两种「不成立」✗ ★**（第 335 轮立 ✓；**第 738 轮重新归因 ✓——台账第 450 节 ✓**）：
  //   **∴ 第 333 轮的 CI 实测（**本判据当时打印的 ✓）★**：
  //     `{"failed":false,"status":501,"bytes":307,"onLine":false,"swControlled":true}`
  //     **∴ 当时的读法 ✗**：offline 已生效＋请求由 SW 上下文发出 ⇒ **已知 CDP 限制** ✓
  //     **∴ 现在的读法 ✓**：**那是页面里的 `fetch` 被 `installLocalApi` 接管**✗
  //       ⇒ **∴ 501 是本地回的**✗（**请求没出页面**✓）**⇒ 与 CDP／SW 无关** ✓
  //         ⇒ **∴ 所以**：那条「不算失败」的分支**当年掩盖的是接管**✗（**∴ 归因错 ✓）** ✓✓
  //   **∴ 探针换成 XHR 之后 ✗**：`failed === false` **只能**是**真的没断下去** ✓
  //     ⇒ **∴ `knownCdpLimit` 保留**✗（**SW 绕过这条 CDP 限制依然存在 ✓——见
  //       `browser-offline-shell.mjs` 用 `self.fetch` 的那一节 ✓）**✗
  //       但它**不再顺手豁免本地接管的假象** ✓ ★**** ✓✓
  const knownCdpLimit =
    !!(controlProbe && controlProbe.failed !== true
       && controlProbe.swControlled === true && controlProbe.onLine === false);
  const controlBlocked = !!(controlProbe && controlProbe.failed) || knownCdpLimit;
  if (knownCdpLimit) {
    console.log("     · ⊘ 已知 CDP 限制（**不算失败** ✗）：offline 已生效但请求由 SW 上下文发出 ⇒ 负对照不可成立");
  }
  check(controlBlocked, `负对照：断网后未缓存的 ${NEGATIVE_CONTROL} 必须失败（拿到了 status=${controlProbe && controlProbe.status} ⇒ 断网模拟没生效 ⇒ 判据作废 VOID）`);
  if (!controlBlocked) {
    // **负对照不成立 ⇒ 立刻作废（VOID）** ✗ —— 不要带着"断网是假的"这个前提继续跑。
    // **∴ 退出码必须是 3 ✗ ★**（**第 738 轮改 ✓**）：这是「**判据自身前提不成立**」✗
    //   ⇒ **∴ 不是**产品失败**✗（**仓内统一约定 ✓；**旧代码在这里退 1 ✗ 会把环境问题算成产品红 ✓）**）✓
    console.error("  ⊘ 判据作废（VOID）：断网模拟没有生效 ⇒ 后续离线结论无效 ⇒ 退出码 3");
    await online();
    socket.close();
    process.exit(3);
  }
  check(Array.isArray(entries1) && entries1.length === 1, "队列里恰好 1 条（拿到了 " + (entries1 ? entries1.length : "?") + "）");
  const first = entries1 && entries1[0];
  check(!!first && first.kind === "tool" && first.tool === "draw_stroke", "第一条是 draw_stroke 的工具调用");
  check(!!first && JSON.stringify(first.args) === JSON.stringify(DRAW_ARGS), "载荷逐字段等于传入的参数（这才是「没丢」）");
  check(!!first && Number(first.base_head) > 0, "入队时记下了服务端 HEAD 作为基线（冲突判定要用）");
  check(ui1.state === "queued", "界面 data-state = queued（实际 " + ui1.state + "）");
  check(ui1.text !== "已保存", "界面不谎称「已保存」（实际「" + ui1.text + "」）");
  check(!!ui1.banner && ui1.banner.indexOf("补交") >= 0, "有可见的操作条，且说清会自动补交（实际「" + ui1.banner + "」）");
  check(queued && queued.ok === false && queued.queued === true, "调用方拿到的是「已排队」而不是「已落库」（ok=false, queued=true）");

  // ② 顺序 = 入队顺序
  section("② 再排一条：顺序必须是入队顺序");
  await evaluate(`window.yanshiCallTool("create_layer", ${JSON.stringify(createLayerArgs(LAYER2, "eq2"))})`);
  await evaluate(`window.yanshiCallTool("draw_stroke", ${JSON.stringify(DRAW_ARGS)})`);
  const entries2 = await evaluate("window.yanshi.outboxEntries()");
  const seqs = (entries2 || []).map((row) => row.seq);
  const ordered = seqs.every((value, index) => index === 0 || value > seqs[index - 1]);
  check((entries2 || []).length === 3, "队列 3 条（实际 " + (entries2 ? entries2.length : "?") + "）");
  check(ordered, "主键严格递增（" + JSON.stringify(seqs) + "）⇒ 补交顺序有唯一依据");
  check((entries2 || [])[1] && entries2[1].tool === "create_layer", "第 2 条是 create_layer（入队顺序被保住）");

  // ③ 联网 ⇒ 按序补交；最终状态 == 在线做一遍；再补交一次不重复应用
  section("③ 联网：按序补交，最终服务端状态 == 在线做一遍，且补交两次不重复应用");
  await online();
  await sleep(500);
  await evaluate("window.yanshi.flushOutbox()");
  await evaluate("window.yanshi.flushOutbox()");
  const leftovers = await evaluate("window.yanshi.outboxEntries()");
  const pageAtoms = await atomsOf(doc, token);
  // 参照：同样的三条 —— 第 2 条 draw_stroke 在**同一图层**上；页面这边是入队顺序 [draw, create_layer2, draw]。
  await callToolHttp(refDoc, refToken, "draw_stroke", DRAW_ARGS);
  await callToolHttp(refDoc, refToken, "create_layer", createLayerArgs(LAYER2, "eq2"));
  await callToolHttp(refDoc, refToken, "draw_stroke", DRAW_ARGS);
  const refFinal = await atomsOf(refDoc, refToken);
  const pageProject = project(pageAtoms), refProject = project(refFinal);
  console.log("     页面原子 kind = " + JSON.stringify(pageAtoms.map((a) => a.kind)));
  console.log("     参照原子 kind = " + JSON.stringify(refFinal.map((a) => a.kind)));
  const difference = firstDiff(pageProject, refProject);
  if (difference) console.log("     第一处不同 @" + difference.index + "\n       页面 = " + String(difference.page).slice(0, 300) + "\n       参照 = " + String(difference.ref).slice(0, 300));
  check(leftovers.length === 0, "补交后队列清空（实际剩 " + leftovers.length + " 条）⇒ 被确认的条目**各删一次**");
  check(!difference, "服务端最终状态 == 在线做一遍（逐原子的 kind + payload 完全相同）");
  const ids = pageAtoms.map((a) => a.id);
  check(new Set(ids).size === ids.length, "原子 id 无重复（" + ids.length + " 条）");
  const headAfter = pageAtoms.length;
  const flushTwice = await evaluate("window.yanshi.flushOutbox()");
  const pageAtoms2 = await atomsOf(doc, token);
  check(pageAtoms2.length === headAfter, "队列空时再补交一次是**空操作**（原子数 " + headAfter + " ⇒ " + pageAtoms2.length + "）");
  check(flushTwice && flushTwice.replayed === 0 && flushTwice.ok === true, "第二次补交报告 replayed=0（实际 " + JSON.stringify(flushTwice) + "）");

  // ④ 真实指针笔迹（产品路径）也进队列，联网后服务端里恰好一条
  section("④ 真实指针笔迹：断网时进队列，联网后服务端**恰好一条**那个 id");
  // **★ 第 739 轮：本段切到 `render=client` 的页面**✗ ★**（台账第 449 节 ✓）**：
  //   **∴ 为什么 ✗**：`initWasm` 第一行在**服务端渲染模式**直接 return ✗
  //     ⇒ 指针笔迹的**客户端 ULID 原子**需要 WASM 内核 ✓（**①②③ 用的**工具路径 ✗
  //       在**服务端页**跑 ⇒ 那一侧的队列才是被测对象 ✓）** ★
  //   **∴ 于是 ✗**：**④ 之后立刻切回**服务端页**✓（**⑤⑥⑦ 的队列语义只在服务端页存在 ✓**）✓
  await bootViewer(url);
  // **内核等待放在段内**✓（**原来挂在全局 ✗ —— 分段之后它只是这一段的先决条件 ✓**）。
  const kernelReady = await waitKernelReady();
  // **这一步是硬断言** ✗：没有内核时普通画笔走 `commitShape`（工具路径 ✓），
  // 那条**不产生客户端 ULID 原子** ✓ ⇒ 这一段就没被验到 ✓ ⇒ **不当作通过** ✗。
  // CI 在跑判据前会构建 `crates/yanshi-wasm/pkg`（见 .github/workflows/ci.yml ✓）。
  check(kernelReady, "WASM 内核必须就绪（指针笔迹这条路要用它；CI 会先构建 wasm 产物）");
  if (kernelReady) {
    // **保证"当前图层"真的存在** ✗ —— 原始原子（`/api/atoms`）带的是 `state.layerId` ✓，
    // 所以**先读页面自己会用的那个 id**✗（**∴ 不是判据替页面挑一个 ✓**），再把它建到**服务端**✓：
    //   **∴ 若图层不存在 ✗** ⇒ 补交会被服务端拒 ✓ ⇒ 那测的就成了⑥而不是④ ✓。
    // **∴ 为什么经服务端 API 建 ✗**：客户端页的 callTool 只进本地 IndexedDB ✗
    //   ⇒ **journal 里没有这条层 ⇒ 原子补交回来时服务端不认 ✗**（**上一版正是这样红 ✗**）。
    const activeLayer = await evaluate("window.yanshi.state().layerId");
    check(!!activeLayer, "客户端页给出了当前图层 id（实际 " + JSON.stringify(activeLayer) + "）");
    // **`if_exists: "reuse"`**✓（**产品自己的复用口径 ✓，tools.rs:3363 ✓**）：
    //   **∴ 实测 ✗**：打开文档时服务端**早就建好** `layer_default` ✓ ⇒  plain create 被拒
    //     （**"图层 layer_default 已存在" ✓——**服务端如实拒绝 ✗ 不是缺陷 ✓**）
    //     ⇒ **∴ 判据原来的写法**（页面本地 callTool ✓）在**客户端页**上**根本不进服务端**✗
    //       ⇒ **∴ 复用**是**唯一**既**不改文档语义**（**已存在就**不 commit ✓）**又**说清结果**（`reused: true` ✓）的写法 ✓
    const layerSeed = await callToolHttp(doc, token, "create_layer",
      { layer_id: String(activeLayer), name: "active", if_exists: "reuse" });
    check(layerSeed && layerSeed.ok === true, "当前图层已在服务端日志里可用（新建或按 if_exists=reuse 复用，实际 " + JSON.stringify(layerSeed).slice(0, 120) + "）");
    console.log("     当前图层 = " + activeLayer);
    // **断网前先确认离线资产进了缓存**✓（第 407 轮 ✓：`import("/brush-local.js")` 由 SW 服务 ✓）。
    await waitForPrecache("/brush-local.js");
    await offline();
    await sleep(300);
    const stroke = await evaluate(`(async () => {
      const canvas = document.getElementById("board");
      if (!canvas) return "no-canvas";
      const r = canvas.getBoundingClientRect();
      const o = (x, y) => ({ bubbles: true, cancelable: true, pointerId: 7, pointerType: "mouse",
                             isPrimary: true, button: 0, buttons: 1, clientX: x, clientY: y });
      const x0 = r.left + r.width * 0.3, y0 = r.top + r.height * 0.3;
      canvas.dispatchEvent(new PointerEvent("pointerdown", o(x0, y0)));
      for (let i = 1; i <= 8; i += 1) {
        canvas.dispatchEvent(new PointerEvent("pointermove", o(x0 + i * 5, y0 + i * 3)));
        await new Promise((res) => setTimeout(res, 25));
      }
      canvas.dispatchEvent(new PointerEvent("pointerup", Object.assign(o(x0 + 40, y0 + 24), { buttons: 0 })));
      await new Promise((res) => setTimeout(res, 600));
      return "dispatched";
    })()`);
    const atomEntries = (await evaluate("window.yanshi.outboxEntries()")).filter((row) => row.kind === "atom");
    const uiStroke = await evaluate('document.getElementById("outbox").dataset.state');
    const entry = atomEntries[0];
    console.log("     落笔 = " + stroke + "｜队列里的原子条目 = " + atomEntries.length);
    check(atomEntries.length === 1, "指针笔迹进队列 1 条（实际 " + atomEntries.length + "）");
    check(!!entry && entry.atom && entry.atom.kind === "draw_stroke", "它是 draw_stroke 原子");
    check(!!entry && Array.isArray(entry.atom.payload.data.points) && entry.atom.payload.data.points.length >= 2,
      "载荷里带着真实点列（" + (entry && entry.atom.payload.data.points.length) + " 个点）");
    check(uiStroke === "queued", "界面仍然是 queued（实际 " + uiStroke + "）");
    await online();
    await sleep(500);
    await evaluate("window.yanshi.flushOutbox()");
    const served = (await atomsOf(doc, token)).filter((a) => entry && a.id === entry.atom.id);
    check(served.length === 1, "服务端里恰好一条那个客户端 ULID（" + served.length + " 条）⇒ 原子路径幂等");
    const payloadSame = served.length === 1 && sameJson(served[0].payload, entry.atom.payload);
    if (!payloadSame && served.length === 1) {
      console.log("     服务端载荷 = " + JSON.stringify(served[0].payload).slice(0, 300));
      console.log("     排队载荷   = " + JSON.stringify(entry.atom.payload).slice(0, 300));
    }
    check(payloadSame, "服务端存下的载荷与排队的那一份逐字段相同");
    await evaluate("window.yanshi.flushOutbox()");
    const served2 = (await atomsOf(doc, token)).filter((a) => entry && a.id === entry.atom.id);
    check(served2.length === 1, "再补交一次仍然只有一条（不重复应用）");
    // **★ 切回服务端渲染页 ✗ ★**（**第 739 轮 ✓**）：⑤⑥⑦ 断的是「**写队列**」那一半 ✗
    //   ⇒ 它**只在** `render=server` 一侧存在 ✓（**客户端页的调用直接进 IndexedDB ✗ 永不入队 ✓**）。
    await bootViewer(serverUrl);
  } else {
    // **内核没起来也必须切回服务端页**✗ —— 否则 ⑤⑥⑦ 会在**客户端页**上结构性全红 ✓，
    //   而那是**判据自己的导航**坏了的样子 ✗，不是那条「内核未就绪」已经如实报出的红 ✓。
    await bootViewer(serverUrl);
  }

  // ⑤ 有界
  section("⑤ 队列有界：到上限**拒绝并说出来**（不静默丢）");
  // **必须先断网** ✗ —— 第一版忘了这一步 ✓ ⇒ 200 次 create_layer 全在线成功 ✓、
  // 队列永远是空的 ✓ ⇒ 四个断言全红 ✓（判据自己把"没断网"这件事量了出来 ✓）。
  await offline();
  await sleep(300);
  const bound = await evaluate(`(async () => {
    await window.yanshi.discardOutbox();
    const max = window.yanshi.outbox().max_entries;
    let refusedAt = null;
    for (let i = 0; i < max + 20; i += 1) {
      await window.yanshiCallTool("create_layer", { layer_id: "bound_" + i, name: "b" + i });
      if (window.yanshi.outbox().refused > 0) { refusedAt = i; break; }
    }
    // **徽标是异步刷的** ✗（refreshOutboxBadge 要 await IndexedDB ✓）⇒
    // 不等它一下就读 DOM，会读到上一次的状态 ✓（第一版就是这样：state 还是 queued ✗）。
    await window.yanshi.refreshOutbox();
    return { max, refusedAt, pending: window.yanshi.outbox().pending, state: window.yanshi.outbox().state,
             label: window.yanshi.outbox().label, banner: window.yanshi.outbox().banner };
  })()`);
  console.log("     上限 = " + bound.max + "｜第 " + bound.refusedAt + " 次被拒｜排队 = " + bound.pending + "｜界面 = " + bound.state);
  check(bound.max === 200, "上限是写死的 200 条（实际 " + bound.max + "）");
  check(bound.refusedAt === bound.max, "第 " + bound.max + " 条之后开始拒绝（实际 refusedAt=" + bound.refusedAt + "）");
  check(bound.pending === bound.max, "被拒时队列**没有变短**（" + bound.pending + " 条）⇒ 没有悄悄丢最旧的");
  check(bound.state === "full", "界面 data-state = full（实际 " + bound.state + "）");
  check(!!bound.banner && bound.banner.indexOf("没有记录") >= 0, "界面明说「刚才那一步没有记录」（实际「" + bound.banner + "」）");
  // **清理必须在**断网时**做** ✗ —— 第一版先联网再清 ✓ ⇒ 4 秒轮询/WS 重连的**自动补交**
  // 抢在清理之前把 200 条第几条补交上去了 ✓ ⇒ HEAD 前进 ✓ ⇒ ⑥ 的入队基线失效 ✓
  // ⇒ ⑥ 被误判成"冲突"而不是"被拒" ✓（判据自己把这条时序暴露了出来 ✓）。
  await evaluate("window.yanshi.discardOutbox()");
  await evaluate("window.yanshi.refreshOutbox()");
  await online();
  await sleep(300);
  // **★ 第 739 轮：⑥ 之前必须把入队基线刷成**当前** HEAD ✗ ★**（**实测证据 ✓**）：
  //   **∴ 上一轮实测 ✗**：⑥ 拿到的是 `conflict`（**"server head 5 != 2"**✓）而不是"被服务端拒绝"✗
  //     ⇒ **∴ 根因 ✗**：条目入队时记的 `base_head` 是**最后一次成功在线调用**给的 HEAD ✓（viewer-app.js:1442 ✓）
  //       —— ⑤ 的 200 条**全被拒在门外 ⇒ 没有任何一次成功调用把基线抬到 5** ✗
  //         ⇒ **∴ 于是**：flush 的冲突门（**发第一条之前先探 HEAD ✓**）先撞上 ✗ ⇒ 测不到"内容被拒" ✓
  //   **∴ 修法 ✗**：**用产品自己的探针端点**（`outboxProbeHead` 同一条 URL ✓：
  //     `since=999999999&limit=1` ⇒ 只回 `head_seq` ✗ 不改文档 ✓）**在线读一次**✗
  //     写回 `window.yanshiStats.serverHead` ✓ ⇒ **∴ 于是** ⑥ 的基线 == 当前 HEAD ✓
  //     ⇒ **拒绝测试**（draw_stroke 指向不存在的图层 ✓）测的才是**内容被拒** ✗ 不是基线过期 ✓
  //   **∴ 为什么这不算替页面伪造 ✗**：产品本来就在**每次成功在线调用后**刷新这个值 ✓（:1442 ✓）
  //     ⇒ 这里只是**在"没有成功调用可跑"的时刻走同一条读**✓ —— ⑦ 的冲突**不刷**✓
  //       ⇒ 它拿基线 = ⑥ 之后的当前 HEAD ✓，别人再改一次 ⇒ 冲突照旧成立 ✓（**实测 5 ⇒ 6 ✓**）。
  const headBeforeBad = await evaluate(`(async () => {
    const response = await fetch("/api/atoms?doc=" + encodeURIComponent(${JSON.stringify(doc)})
      + "&token=" + encodeURIComponent(${JSON.stringify(token)}) + "&since=999999999&limit=1");
    const value = await response.json();
    if (value && value.ok) window.yanshiStats.serverHead = Number(value.head_seq || 0);
    return value && value.ok ? Number(value.head_seq || 0) : null;
  })()`);
  check(headBeforeBad !== null, "⑥ 前在线刷新入队基线成功（当前 HEAD = " + headBeforeBad + "）");

  // ⑥ 服务端拒绝：报告 + 仍留在队列
  section("⑥ 服务端拒绝某条：报告出来，且**仍留在队列**");
  await offline();
  await sleep(300);
  const BAD_ARGS = { layer_id: "no_such_layer_offline_journal", object_id: "obj_bad", data: { points: [[1, 1], [5, 5]], size: 3, color: { r: 0, g: 0, b: 0, a: 255 }, hardness: 0.7, smooth: false } };
  await evaluate(`window.yanshiCallTool("draw_stroke", ${JSON.stringify(BAD_ARGS)})`);
  const badPreview = (await evaluate("window.yanshi.outboxEntries()")).length;
  check(badPreview === 1, "坏操作先正常入队（1 条，实际 " + badPreview + "）");
  await online();
  await sleep(500);
  const flushBad = await evaluate("window.yanshi.flushOutbox()");
  const badEntries = await evaluate("window.yanshi.outboxEntries()");
  const badUi = await evaluate(`(() => {
    const n = document.getElementById("outbox");
    const b = document.getElementById("outboxBar");
    return { state: n.dataset.state, banner: b && !b.hidden ? document.getElementById("outboxText").textContent : null };
  })()`);
  console.log("     补交返回 = " + JSON.stringify(flushBad));
  console.log("     队列 = " + JSON.stringify((badEntries || []).map((r) => ({ tool: r.tool, state: r.state, error: String(r.error).slice(0, 60) }))));
  check(flushBad && flushBad.ok === false && flushBad.rejected === true, "补交报告被拒（ok=false, rejected=true）");
  check((badEntries || []).length === 1, "被拒的条目**还在队列里**（实际 " + (badEntries || []).length + " 条）");
  check(!!badEntries[0] && badEntries[0].state === "failed", "它的 state = failed（实际 " + (badEntries[0] && badEntries[0].state) + "）");
  check(badUi.state === "failed", "界面 data-state = failed（实际 " + badUi.state + "）");
  check(!!badUi.banner && badUi.banner.indexOf("被服务端拒绝") >= 0, "界面上写着被拒、且仍在队列（实际「" + badUi.banner + "」）");
  await evaluate("window.yanshi.discardOutbox()");
  await evaluate("window.yanshi.refreshOutbox()");

  // ⑦ 冲突：服务端这份文档已经变了 ⇒ 暂停补交、不覆盖
  section("⑦ 冲突：服务端这份文档已变化 ⇒ 暂停补交、不覆盖别人");
  await offline();
  await sleep(300);
  const CONFLICT_ARGS = { layer_id: LAYER, object_id: "obj_conflict", data: { points: [[3, 3], [30, 30]], size: 5, color: { r: 200, g: 10, b: 10, a: 255 }, hardness: 0.7, smooth: false } };
  await evaluate(`window.yanshiCallTool("draw_stroke", ${JSON.stringify(CONFLICT_ARGS)})`);
  const beforeConflict = (await atomsOf(doc, token)).length;
  // **别的人/别的会话改了同一份文档** ✓（Node 直连 ⇒ 页面此刻离线看不见它 ✓）。
  const other = await callToolHttp(doc, token, "create_layer", { layer_id: "other_actor_layer", name: "other" });
  check(other.ok === true, "离线的同时，服务端上这份文档被别人改了一次（前置 ✓）");
  await online();
  await sleep(500);
  const flushConflict = await evaluate("window.yanshi.flushOutbox()");
  const conflictUi = await evaluate(`(() => {
    const n = document.getElementById("outbox");
    const b = document.getElementById("outboxBar");
    return { state: n.dataset.state, banner: b && !b.hidden ? document.getElementById("outboxText").textContent : null };
  })()`);
  const afterConflict = await atomsOf(doc, token);
  const applied = afterConflict.filter((a) => a.payload && a.payload.object_id === "obj_conflict");
  console.log("     补交返回 = " + JSON.stringify(flushConflict));
  check(flushConflict && flushConflict.ok === false && flushConflict.conflict === true, "补交报告冲突（ok=false, conflict=true）");
  check(applied.length === 0, "**没有**把离线那一笔覆盖上去（obj_conflict 原子 " + applied.length + " 条）");
  check(conflictUi.state === "conflict", "界面 data-state = conflict（实际 " + conflictUi.state + "）");
  check(!!conflictUi.banner && conflictUi.banner.indexOf("暂停补交") >= 0, "界面说清「已暂停补交」（实际「" + conflictUi.banner + "」）");
  const conflictEntries = await evaluate("window.yanshi.outboxEntries()");
  check((conflictEntries || []).length === 1 && conflictEntries[0].state === "conflict", "冲突的条目仍然留在队列里（可重试 / 可放弃）");
  check(afterConflict.length > beforeConflict, "服务端那边的改动仍在（HEAD 前进：" + beforeConflict + " ⇒ " + afterConflict.length + "）——我们没回滚别人");
  await evaluate("window.yanshi.discardOutbox()");
  await evaluate("window.yanshi.refreshOutbox()");
  const finalUi = await evaluate('document.getElementById("outbox").dataset.state');
  check(finalUi === "saved", "放弃之后界面回到 saved（实际 " + finalUi + "）");

  socket.close();
  if (failures.length) { console.log("  ✗ 离线写队列未达成：" + failures.join("；")); process.exit(1); }
  console.log("  ✓ 离线写队列达成：断网不丢、按序补交、有界、被拒/冲突都说得清");
  process.exit(0);
})().catch((error) => {
  console.error("判据自己抛错（不当作通过 ✗）：" + (error && error.stack ? error.stack : error));
  process.exit(1);
});
