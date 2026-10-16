#!/usr/bin/env node
// **右栏 tab 化 + 整页垂直锁死 + 画布内部滚动** ✓ —— 用户第 5 条 ✓。
//
// **判据（都能红 ✓）**：
//   ① 页面本身**不滚** ✓，而右栏**自己滚** ✓（"垂直方向锁死、面板内部滚"✓）；
//   ② 切 tab：只有那一个窗格的卡可见 ✓，而且**卡片总数不许变** ✓
//      （17 张卡搬完还是 17 张 ✓ —— "搬着搬着丢了一张"是本项目的老病 ✓，所以单独判 ✓）；
//   ③ 放大到 400% ⇒ **舞台内部**出现横向滚动 ✓（不封顶才有这个 ✓）；回到 100% ⇒ 又不滚 ✓；
//   ④ **浮层与 tab 共存** ✓：浮出的调色板/纹理卡，收起后必须回到**素材窗格里** ✓
//      （搬回旧父节点 ⇒ 红 ✓ —— 这就是"搬卡顺序"那条取舍的判据 ✓）；
//   ⑤ 零控制台错误 ✓。
//
// 用法：node scripts/browser-layout.mjs "http://127.0.0.1:<port>/?doc=<id>&token=<token>"
const url = process.argv[2];
const debugPort = process.env.CDP_PORT || "9333";
const shotsDir = process.env.SHOT_DIR || "/tmp/yanshi-layout";
if (!url) {
  console.error("用法: node scripts/browser-layout.mjs <viewer-url>");
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
const viewerBase = url.split("?")[0];
const target =
  list.find((t) => t.type === "page" && t.url.startsWith(viewerBase)) ||
  list.find((t) => t.type === "page");
if (!target) {
  console.error(`未找到调试目标（Chromium 是否以 --remote-debugging-port=${debugPort} 启动？）`);
  process.exit(2);
}
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 1;
const pending = new Map();
const consoleLines = [];
ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method === "Runtime.consoleAPICalled") {
    consoleLines.push((message.params.args || []).map((a) => a.value ?? "").join(" "));
  }
  if (message.method === "Runtime.exceptionThrown") {
    const details = message.params.exceptionDetails || {};
    consoleLines.push("exception: " + (details.exception?.description || details.text || ""));
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
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// **★★★ 等待上限**必须可配置 ✗ ★★★**（**第 777 轮修；**CI 实测 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：**`⏱ 等待超时：WASM 内核就绪`**✗
//     ＋ **∴ 而**后续断言（**如**「设了 200% 但缩放仍在适配档」✓）**只是**后果** ✓ ★
//   **∴ 根因 ✗**：**上限写死**（**本仓 11 处：**6×30000／4×25000／1×20000 ✓）✗
//     ＋ **∴ 而** CI **的 runner **比开发机慢**✗ ⇒ **∴ 那些数字**都没有**产品依据** ✓ ★★
//   **∴ 修法 ✗**：**统一读 `YANSHI_WAIT_MS`**✗ ＋ **∴ 默认 120 秒** ✓ ★
//     ＋ **∴ 收益**：**判据测的是它真正要测的东西**（**产物是否就绪 ✓）而**不是**环境速度** ✓ ★
//       ＋ **∴ 代价**：**真异常时要**多等**✗（**∴ 但**只在**真异常时** ✓）★
const WAIT_MS = Number(process.env.YANSHI_WAIT_MS || 120000);
const waitFor = async (expression, label, timeoutMs = WAIT_MS) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expression)) return true;
    await sleep(200);
  }
  console.error(`  ⏱ 等待超时：${label}`);
  return false;
};
const capture = async (name) => {
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const path = `${shotsDir}/${name}.png`;
  await fs.writeFile(path, Buffer.from(data, "base64"));
  return path;
};

await send("Runtime.enable");
await send("Page.enable");
// **★ 由判据**自己**定视口 ✗★**（2026-10-15 ✓；§49 之补一百零八 ✓）：
//   **∴ 为什么** ✗：`--window-size` 在**本机**有效 ✓、在 **CI 上无效** ✗
//     （实测：`12b1185` 的 CI 仍报 `board.width=320` ✓，而参数**确实在** `run-criteria.sh:64` ✓）
//     ⇒ ⇒ 判据若依赖"窗口多大"✗ ⇒ **它的结果就取决于环境** ✗ ✓；
//   **∴ 改成** ✓：用 CDP 的 `Emulation.setDeviceMetricsOverride` **强制**一个已知视口 ✓
//     ⇒ ⇒ **与窗口无关** ✓、与本机/CI 的差异**无关** ✓ ✓；
//   **∴ 尺寸取 1280×900** ✓：与 `run-criteria.sh` 里给 chromium 的 `--window-size` **一致** ✓
//     （§49 之补三十七 ✓）⇒ 本机行为**不变** ✓，只是**不再依赖它** ✓。
await send("Emulation.setDeviceMetricsOverride", {
  width: 1280,
  height: 900,
  deviceScaleFactor: 1,
  mobile: false,
});
await send("Page.navigate", { url });
await send("Page.reload", { ignoreCache: true });
// **睡眠已删** ✓（第 900 轮 ✓）：下面 waitFor 已在正确地等 ✓，且等不到会 exit ✓。
consoleLines.length = 0;
if (!(await waitFor("typeof window.yanshiRightTabs === 'object' && typeof window.yanshiDock === 'object'", "查看器就绪"))) {
  process.exit(3);
}
// **等画布真的被设过尺寸再量** ✗ —— 第 47 轮就是在这里栽的 ✓：启动瞬间 `<canvas>` 还是默认的
// 300×150 ✓、`style.width` 为空 ✓ ⇒ 我据此得出"尺寸没落到元素上"✗，而真相只是"还没轮到它"✓。
// ⇒ 判据必须**等被测对象稳定** ✓（`board.width` 走出默认值 ✓ + **当前模式的渲染器真的画过一帧** ✓）。
// **★★★ 但「≥ 自己显示宽度的一半」**还是错的** ✗ ★★★**（**第 456 轮修；**CI ＋ 本机受控复现 ✓）
//   **∴ 症状 ✗**（**CI `38817240353` ／ 本机复现 ✓，两边**逐字节同形**✓）★：
//     `❌ 画布尺寸与首帧没就绪 ⇒ board.width=320（应与显示宽度同量级）显示宽度="924px"（rect=844）
//      height=240｜模式=客户端优先｜wasm=true kernelReady=true lastArea=76800 内核 tile 读写=8`
//   **∴ 关键 ✗**：**模式那一边**四项**全真**✓ ⇒ **红的不是"首帧"** ✗ 而**是"尺寸"** ✓ ★★
//   **∴ 根因 ✗**：**`320 >= floor(844 × 0.5) = 422`** ⇒ **假** ✗
//     ＋ **∴ 而**那个 **844** 是**视口宽 1280** 下 `zoom-fit` 把 320px 文档**放大** 2.8875× 的结果 ✓
//       （`style.width = 924px = 320 × 2.8875` ✓）
//     ⇒ ⇒ **∴ 于是**「**位图 ≥ 显示宽度一半**」这个**比值**✗
//       **∴ 它其实**是**窗口宽度**的代理** ✓：**窗口越宽 ⇒ 显示越放大 ⇒ 分母越大 ⇒ 假红** ✗
//         ＋ **∴ 而**我要的恰恰是"**判据不依赖窗口大小**" ✓ ⇒ **∴ 这个比值**把刚删掉的依赖**请回来了** ✗ ★★★
//   **∴ 真正的契约 ✗**（**读产品代码 ✓**，`viewer-app.js:1013` ✓ 原话）★：
//     **"画布内部分辨率 = **视口的文档像素数**（内核 1:1 渲染），CSS 显示尺寸由 `applyDisplaySize` 决定"**
//     ＋ **∴ 本机实测 ✓**（判据文档 = `320×240`，视口 1280×900 ✓）：
//       **`board 320×240｜overlay 320×240｜kernel 320×240`** ✓
//       ＋ **`state().viewport = {x:0,y:0,w:320,h:240}`** ✓ ⇒ **与 backing store 逐值相等** ✓ ★★
//     ⇒ ⇒ **∴ 所以**判据要断的是「**backing store == 视口的文档像素数**」✓
//       ⇒ **∴ 它与**缩放（2.8875 ✓）／窗口宽（1280 ✓）／DPR **全都无关** ✓ ★★★
//   **∴ 强度没有放松 ✗**：**默认的 `300×150`** 仍然**必红** ✓
//     （**∴ 它 ≠ 视口 320×240** ✓）—— 原来那层"不是默认值"的护栏**被相等性蕴含** ✓，
//     而且**不再**把"文档恰好 300 宽"这种**合法**情况误判成红 ✗（那正是**同一类**假红 ✓）。
//
// **"画过一帧"的判据必须按当前渲染模式分开** ✓（本轮 ✓）：本仓缺省是**客户端优先** ✓
//（`yanshi.serverRender` 不在 `localStorage` 里 ⇒ 加载内核 ✓），此时"服务端补过一次像素"**不是**
// 任何一条路径的必然后果 ✗ —— 实测（本机，同一份产物、重载 8 次）：7 次在"内核已经画完"那一刻
// `serverBlits = 0` ✓（补画是**异步排队**的 ✓，而且只有产品在启动竞态里先走了一次服务端预览、
// 把 `needsServerPixels` 立起来才会排 ✓）⇒ 旧判据 `serverBlits > 0` 在这条路径上是**靠竞态碰运气** ✗
//（**过时的基线** ✓，不是被测对象的毛病 ✓：画布尺寸对、内核也真的画了 ✓）。
// ⇒ 分开要求 ✓，**两边都不许恒真** ✓：
//   · 服务端模式（读产品**同一个键** ✓）⇒ **仍然要**一次真的服务端补画 ✓（与今天一致，不放宽 ✓）；
//   · 客户端模式 ⇒ 要**内核真的把像素贴到了画布上** ✓（不是"内核句柄在不在"✗ —— 那只是"已装载" ✓）。
// **客户端那条路的证据是三层** ✓（缺一层都会把"空白画布"放过去 ✓）：
//   ① `yanshiStats.wasm` ✓：内核模块已装载 —— 产品正是用它 gate 客户端路径 ✓（`kernelReady()` ✓）；
//   ② `yanshiKernelReady` ✓：内核实例已建好、原子已装载 ✓（`loadKernel` 里置位 ✓，随后立刻
//      `renderViewport` ✓；`browser-ui-check` 早就拿它当"内核就绪"的判据 ✓ ⇒ 与既有约定一致 ✓）；
//   ③ `yanshiStats.lastArea > 0` ✓ **且** 内核自报的 tile 缓存读写 > 0 ✓（后者的门面是
//      `window.yanshi.kernelStats()` ✓，它原样转发内核的 `stats_json` ✓）：
//      `lastArea` **全仓只有一处写入** ✓ —— `drawKernelRegion` 里**成功 `putImageData` 之后** ✓
//      ⇒ 它证明"画布上真的被内核写过一块像素" ✓；内核计数是**内核自己报的** ✓
//      ⇒ 它证明"内核确实渲染过 tile" ✓。两个一起才排得掉"内核渲染出空缓冲区 ⇒ 早退" ✗
//      与"贴了个 0 面积" ✗ 这两种**现象一样**的空白画布 ✓。
// **判据仍然能红** ✓：内核恒不画（`lastArea`/计数停在 0 ✓）⇒ 这里超时退出 ✓；
// `board.width` 停在默认 300 ✓、或尺寸小于显示宽度的一半 ✓ ⇒ 第一层就退出 ✓。
// **★ 必须与产品**同一套语义** ✗ ★**（第 101 轮 ✓；**本地实测 ✓）：
//   **∴ 我**原来只读 `localStorage === "1"`**✗
//     ⇒ **∴ 而**页面的 `serverRenderPreferred()`（`viewer-app.js:34` ✓）**是**复合的**✗**：
//       **∴ `localOnly ⇒ false`**✗（**∴ 静态 PWA 一律本地 ✓）** ✓✓
//       **∴ `stored === null ? true : stored === "1"`**✗
//         ⇒ **★ 即**"**没有任何偏好 ⇒ **服务端优先**" ✓ ★**** ✓✓
//     ⇒ **∴ 于是**：**全新 profile 下**✗
//       **∴ 页面**走**服务端优先**（`initWasm` 被跳过 ⇒ `wasm=false` ✓，**那是设计 ✓）** ✓✓
//       **∴ 而**判据**按**客户端优先**算**✗ ⇒ **∴ 它**要求**内核** ⇒ **∴ 红 ✓**** ✓✓
//       **∴ 且**它**同时**打印出 `serverBlits=3`**✗（**∴ 服务端**明明铺过底 ✓）
//         ⇒ **★ 那**就是**两个信号矛盾**的来源 ✓ ★**** ✓✓
//   **∴ 修法**：**照抄产品那套判断** ✓**** ✓✓
//
// **★★★ 但上面那次"照抄"漏了**最高优先级**的那一条 ✗ ★★★**（**第 784 轮修；**CI 实测 ✓）
//   **∴ 症状 ✗**（**CI ✓）★**：**本判据报**
//     `❌ 画布尺寸与首帧没就绪 ⇒ board.width=320 … 显示宽度="424px"（rect=344）height=240
//      ｜★模式=服务端★ ⇒ 服务端模式需 serverBlits>0（★实测 0★）
//      ｜客户端模式需内核画过（★wasm=true｜kernelReady=true｜lastArea=76800｜内核 tile 读写=8★，四者都需成立）`
//   **∴ 矛盾 ✗**：**客户端模式的**四个条件**全满足**✗ 而**它**却说模式＝服务端** ✓ ★★
//   **∴ 根因 ✗**：**本判据的 `serverRenderOn` **只读 `localStorage`**✗
//     ＋ **∴ 而**产品的 `serverRenderPreferred()`（**`viewer-app.js:56` ✓）**有**三档**✗：
//       ⇒ **∴ ① ★ URL 参数**最高优先**✗ ★**：** `?render=client` ⇒ **强制客户端内核** ✓（**页面第 71 行 ✓）
//         ⇒ **∴ ②** 否则看 `localStorage.getItem("yanshi.serverRender")` ✓
//           ⇒ **∴ ③** 否则 `pwaLocalOnly`／缺省 ✓
//             ＋ **∴ 于是**：**URL 里有 `?render=client`**✗ 而**本判据**只看了第 ② 档**
//               ⇒ **∴ 它**把客户端模式**误判成服务端** ✗
//                 ⇒ **∴ 而**客户端模式下 `serverBlits` **本来就恒为 0** ✓（**∴ 见上文第 108 行 ✓）
//                   ⇒ **∴ 所以**它**必然超时** ✓ ★★★
//   **∴ 修法 ✗**：**把 URL 参数**加回判定**✗ ＋ **∴ 优先级与产品一致**（**URL ⇒ localStorage ⇒ 缺省 ✓）★
//   **∴ 修法 ✗**：**把 URL 参数**加回判定**✗ ＋ **∴ 优先级与产品一致**（**URL ⇒ localStorage ⇒ 缺省 ✓）★
//     ＋ **∴ 为什么**必须一致 ✗**：**判据**复制产品逻辑**时**漏一条**⇒ **∴ 于是**它**测的不是同一个模式** ✓ ★★
//       ＋ **∴ 三档顺序 ✗**（**与 viewer-app 第 56–75 行一致 ✓）**★**：
//         **∴ ①** `?render=client` ⇒ 强制客户端｜`?render=server` ⇒ 强制服务端 ✓
//           **∴ ②** 否则 `localStorage["yanshi.serverRender"]`（**`"1"` ⇒ 服务端 ✓）
//             **∴ ③** 否则 `__pwaLocalOnly` ⇒ 客户端；再否则 ⇒ 服务端 ✓
const serverRenderOn = `(() => {
  try {
    if (typeof window !== "undefined" && window.__pwaLocalOnly === true) return false;
    const param = new URLSearchParams(location.search).get("render");
    if (param === "client") return false;
    if (param === "server") return true;
    const stored = localStorage.getItem("yanshi.serverRender");
    return stored === null ? true : stored === "1";
  } catch (error) { return true; }
})()`;
// **★★★ 尺寸这一层必须**只有一个出处** ✗ ★★★**（**第 456 轮 ✓）：**判据的等待条件**与
// **失败诊断**都在问"backing store 对不对" ✓ ⇒ **∴ 若各写一遍**✗ ⇒ **∴ 两处会**漂移**✓
// （**实测：**我把期望值变异 ＋8 之后，等待条件**红了** ✓ 而诊断**还印「尺寸这一层成立 ✓」**✗ ——
// **∴ 于是**失败信息**指向了错的一边** ✗）⇒ **∴ 抽成一个串** ✓，**两边都插它** ✓。
const sizeOkExpr = [
  "(() => {",
  "  const b = document.getElementById('board');",
  "  const st = window.yanshi.state();",
  "  const vp = st.viewport || {};",
  "  return !!(vp.w > 0 && vp.h > 0 && b.width === vp.w && b.height === vp.h);",
  "})()",
].join("\n");
if (!(await waitFor(
  `(() => {
     // **契约**（viewer-app.js:1013 ✓）：backing store = **视口的文档像素数**（内核 1:1 ✓）
     // ⇒ 与缩放／窗口宽／DPR **无关** ✓；默认 300×150 不满足相等 ⇒ 仍然必红 ✓。
     if (!(${sizeOkExpr})) return false;
     const st = window.yanshi.state();
     if (${serverRenderOn}) return st.serverBlits > 0;
     const stats = window.yanshiStats || {};
     if (stats.wasm !== true || window.yanshiKernelReady !== true) return false;
     const kernel = window.yanshi.kernelStats ? window.yanshi.kernelStats() : null;
     return (stats.lastArea | 0) > 0
            && !!kernel && ((kernel.cache_misses | 0) + (kernel.cache_hits | 0)) > 0;
   })()`,
  "画布尺寸与首帧",
  20000,
))) {
  // **两边各自的值都打出来** ✓（第 422 轮 ✓）：这条等待是**模式相关的合取** ✗ ——
  // 任一边没成立都会在这里退出 ✓，而只说前者的症状 ✗ ⇒ **看不出是哪一边** ✓（**失败理由无法定位** ✗）。
  const diag = await evaluate(`(() => {
    const b = document.getElementById("board");
    const stats = window.yanshiStats || {};
    const out = { width: b && b.width, height: b && b.height, styleWidth: b && b.style.width,
                  shownWidth: b ? Math.round(b.getBoundingClientRect().width) : null,
                  mode: ${serverRenderOn} ? "服务端" : "客户端优先" };
    try {
      const st = window.yanshi.state();
      out.viewport = st.viewport;
      out.docSize = st.docSize;
      out.zoom = st.displayScale;
      // **与等待条件同一个出处** ✓（sizeOkExpr ✓）⇒ ∴ 不可能漂移 ✓。
      out.sizeOk = ${sizeOkExpr};
      out.serverBlits = st.serverBlits;
    } catch (e) { out.serverBlits = "取不到: " + e; }
    out.wasm = stats.wasm === true;
    out.kernelReady = window.yanshiKernelReady === true;
    out.lastArea = stats.lastArea | 0;
    try {
      const kernel = window.yanshi.kernelStats ? window.yanshi.kernelStats() : null;
      out.kernelTiles = kernel ? ((kernel.cache_misses | 0) + (kernel.cache_hits | 0)) : null;
    } catch (e) { out.kernelTiles = "取不到: " + e; }
    return out;
  })()`);
  console.error(`❌ 画布尺寸与首帧没就绪 ⇒ board=${diag.width}×${diag.height}（应 == 视口的**文档像素数** ✓；默认 300×150 必红 ✓）视口=${JSON.stringify(diag.viewport)}（文档=${JSON.stringify(diag.docSize)} 缩放=${diag.zoom} CSS=${JSON.stringify(diag.styleWidth)}／rect=${diag.shownWidth}）⇒ 尺寸这一层${diag.sizeOk ? "成立 ✓" : "不成立 ✗"}｜模式=${diag.mode} ⇒ 服务端模式需 serverBlits>0（实测 ${diag.serverBlits} ✓）｜客户端模式需内核画过（wasm=${diag.wasm} kernelReady=${diag.kernelReady} lastArea=${diag.lastArea} 内核 tile 读写=${diag.kernelTiles}，四者都需成立 ✓）`);
  process.exit(1);
}

// ① 页面不滚、右栏自己滚 ✓
const scroll = await evaluate(`(() => {
  const aside = document.querySelector("aside");
  const stage = document.querySelector(".stage");
  // **判据是「真去滚一下看动不动」** ✗ —— scrollHeight 即使 overflow:hidden 也会报**内容高度** ✓
  //（第一版拿它当判据 ⇒ 误报"整页还能滚"✗，而 「documentElement」 那一项其实是 0 ✓）。
  const before = window.scrollY;
  window.scrollTo(0, 600);
  const moved = window.scrollY - before;
  window.scrollTo(0, before);
  return {
    pageScrollMoved: moved,
    docOverflow: document.documentElement.scrollHeight - window.innerHeight,
    asideScrolls: aside.scrollHeight - aside.clientHeight,
    asideClipped: aside.getBoundingClientRect().bottom > window.innerHeight + 2,
    asideOverflowY: getComputedStyle(aside).overflowY,
    stageOverflowX: stage ? stage.scrollWidth - stage.clientWidth : null,
  };
})()`);
console.log(
  `  ① 滚动：试着往下滚 ${scroll.pageScrollMoved}px（0 = 锁死 ✓）；文档多出 ${scroll.docOverflow}px；` +
    `右栏内部可滚 ${scroll.asideScrolls}px`,
);
// **右栏内容不许溢出视口 ✗；若溢出则必须可滚 ✓**（用户报告 2026-10-09 ✓）
// 变异：删掉 `aside { overflow-y: auto; max-height: … }` ⇒ 溢出且不可滚 ⇒ 红 ✓
if (scroll.asideClipped && !(scroll.asideScrolls > 0 && ["auto", "scroll"].includes(scroll.asideOverflowY))) {
  console.error(
    "❌ 右栏内容超出视口且不可滚 ⇒ 用户会「看不全，也不能滚动」✗（asideScrolls=" +
      scroll.asideScrolls + "px｜overflowY=" + scroll.asideOverflowY + "）",
  );
  process.exit(1);
}
if (scroll.pageScrollMoved !== 0 || scroll.docOverflow > 2) {
  console.error("❌ 整页还能滚 ⇒「垂直方向锁死」没做到 ✗");
  await capture("layout-page-scrolls");
  process.exit(1);
}

// ② 切 tab：只有那一个窗格可见，且**卡片总数不变** ✓
const tabStates = {};
// **声明必须在循环外** ✓（第 440 轮 ✓）：原先把结果声明在循环体里 ✗
// ⇒ `const` 是块级作用域 ✓ ⇒ 循环一结束它就没了 ✓ ⇒ 下面汇总里再用 ⇒ **ReferenceError** ✗
//（这个错一直没露头 ✓ —— **上面的 `waitFor` 超时会先 `process.exit(1)`** ✗ ⇒ **一处红遮住下一处** ✓）。
let movedOut = null; // 循环内赋值 ✓；循环外仍可见 ✓（取最后一次迭代的值 ✓，与汇总语义一致 ✓）
for (const key of ["history", "assets", "file", "diag", "paint"]) {
  const state = await evaluate(`(() => { window.yanshiRightTabs.show(${JSON.stringify(key)}); return window.yanshiRightTabs.state(); })()`);
  tabStates[key] = state;
  const total = state.counts.reduce((sum, item) => sum + item.cards, 0);
  console.log(
    `  ② tab「${key}」⇒ 可见卡 ${state.visibleCards} 张（该窗格 ${state.counts.find((c) => c.key === key).cards} 张，全部合计 ${total} 张）`,
  );
  if (state.active !== key) {
    console.error(`❌ 切到「${key}」之后活动窗格是「${state.active}」✗`);
    process.exit(1);
  }
  if (state.visibleCards !== state.counts.find((c) => c.key === key).cards) {
    console.error(`❌「${key}」窗格里可见卡数与应有卡数不一致 ⇒ 有卡没挂上/没藏起来 ✗`);
    await capture("layout-tab-mismatch");
    process.exit(1);
  }
  // **17 张** ✓ = **18 张**减去**工程包那张** —— 后者按用户第 6 条被搬进了**顶栏「文件」菜单** ✓
  //（"导入导出不适合放在信息面板" ✓）。少的那张必须**在菜单里找得到** ✓，所以这里同时点名 ✓。
  // **为什么从 16 改成 17** ✓（本轮 ✓）：诊断入口 `#cardDiagnostics`（"下载诊断包" ✓）
  // 作为**一张新的诊断卡**加进了 `aside` ✓（`7083595` ✓）⇒ 面板里的卡片**本来就该是 17 张** ✓。
  // ⇒ 这条**计数**是"搬卡时丢卡"的**绊线** ✓：数量一变就得**显式重新基线** ✓ ——
  // 所以这里改的是**基线**，不是把绊线放松 ✗（判据强度不变 ✓：数量再变一次照样红 ✓）。
  if (total !== 17) {
    console.error(`❌ 面板里的卡片总数变成 ${total} ⇒ 搬卡时丢了卡 ✗（应当是 17 张 ✓）`);
    await capture("layout-lost-card");
    process.exit(1);
  }
  movedOut = await evaluate(`(() => {
    const ids = ["newDoc", "openDoc", "exportPng", "projectExport", "projectImport"];
    const menu = document.getElementById("fileMenuBody");
    return {
      inMenu: ids.filter((id) => menu && menu.contains(document.getElementById(id))),
      inAside: ids.filter((id) => document.querySelector("aside") && document.querySelector("aside").contains(document.getElementById(id))),
      fileTabHidden: (() => { const tab = document.querySelector('#rightTabs button[data-tab="file"]');
        return tab ? tab.hidden : null; })(),
    };
  })()`);
  if (movedOut.inAside.length > 0) {
    console.error(`❌ 信息面板里还留着 ${movedOut.inAside.join(", ")} ⇒ 没搬出去 ✗`);
    process.exit(1);
  }
  if (movedOut.inMenu.length !== 5) {
    console.error(`❌ 只搬进菜单 ${movedOut.inMenu.length}/5 个控件 ⇒ 有控件在搬运中丢了 ✗`);
    await capture("layout-move-lost-control");
    process.exit(1);
  }
}
await evaluate("window.yanshiRightTabs.show('assets')");
const shotTabs = await capture("layout-tabs-assets");

// ③ 放大 ⇒ **画布比舞台宽、且舞台能滚** ✓；回到 100% ⇒ 画布正好塞满 ✓。
//    **量画布本身，别量 `.stage` 的 scrollWidth** ✗ —— 那里恒有 11px 的差（另有子元素 ✓），
//    100% 与 400% 都是 11px ✓ ⇒ 那是我**量错了对象** ✗（连续第三次判据错，教训同型 ✓）。
const zoomTo = async (percent) => {
  await evaluate(`(() => {
    const box = document.getElementById("zoomInput");
    box.value = ${JSON.stringify(String(percent))};
    box.dispatchEvent(new Event("change", { bubbles: true }));
  })()`);
  await sleep(800);
};
const metrics = () =>
  evaluate(`(() => {
    const stage = document.querySelector(".stage");
    const board = document.getElementById("board");
    const width = Math.round(board.getBoundingClientRect().width);
    const client = stage.clientWidth;
    const before = stage.scrollLeft;
    stage.scrollLeft = 60;
    const canScroll = stage.scrollLeft !== before;
    stage.scrollLeft = before;
    return {
      boardWidth: width,
      stageClient: client,
      overflow: width - client,
      canScroll,
      pageStillLocked: document.documentElement.scrollHeight - window.innerHeight <= 2,
    };
  })()`);
await zoomTo(400);
const zoomed = await metrics();
// **"正好塞满"要按「适配」量** ✗ —— 输入框里的 `100%` 意思是 **1:1**（1 文档像素 = 1 CSS 像素 ✓），
// 那时画布 900px 比舞台宽 **正是对的** ✓（连续第四次判据错，教训同型 ✓：先问判据在问什么 ✓）。
await evaluate(`document.getElementById("zoomFit").click()`);
await sleep(800);
const fitted = await metrics();
console.log(
  `  ③ 缩放：400% ⇒ 画布 ${zoomed.boardWidth}px vs 舞台 ${zoomed.stageClient}px（溢出 ${zoomed.overflow}，能滚=${zoomed.canScroll}）` +
    `；适配 ⇒ 画布 ${fitted.boardWidth}px vs 舞台 ${fitted.stageClient}px（溢出 ${fitted.overflow}）`,
);
if (!(zoomed.overflow > 10) || !zoomed.canScroll) {
  console.error("❌ 放大之后画布没有超出舞台 / 舞台滚不动 ⇒「画布内部独立滚动」没做到 ✗");
  await capture("layout-no-stage-scroll");
  process.exit(1);
}
if (!zoomed.pageStillLocked) {
  console.error("❌ 放大把整页顶开了 ⇒ 垂直锁死被破坏 ✗");
  process.exit(1);
}
if (fitted.overflow > 2) {
  console.error(`❌ 回到 100% 之后画布仍比舞台宽 ${fitted.overflow}px ⇒ fit 不再"正好塞满" ✗`);
  process.exit(1);
}
const shotZoom = await capture("layout-zoomed-stage-scrolls");

// ④ 浮层与 tab 共存：收起之后卡必须回到**素材窗格** ✓
const dockFlow = await evaluate(`(() => {
  const where = (id) => {
    const node = document.getElementById(id);
    const parent = node && node.parentNode;
    return parent ? (parent.id || parent.dataset.pane || parent.className || parent.tagName) : null;
  };
  window.yanshiRightTabs.show("assets");
  const before = { palette: where("cardPalette"), texture: where("cardTexture") };
  window.yanshiDock.open();
  const open = { palette: where("cardPalette"), texture: where("cardTexture") };
  window.yanshiDock.close();
  const after = { palette: where("cardPalette"), texture: where("cardTexture") };
  return { before, open, after, pane: document.querySelector('.tab-pane[data-pane="assets"]') ? "assets" : null };
})()`);
console.log(
  `  ④ 浮层：浮出前 ${dockFlow.before.palette}/${dockFlow.before.texture}` +
    ` ⇒ 浮出时 ${dockFlow.open.palette}/${dockFlow.open.texture}` +
    ` ⇒ 收起后 ${dockFlow.after.palette}/${dockFlow.after.texture}`,
);
if (dockFlow.open.palette !== "assetDockBody" || dockFlow.open.texture !== "assetDockBody") {
  console.error("❌ 点了「素材」之后卡没搬进浮层 ✗");
  process.exit(1);
}
if (dockFlow.after.palette !== "assets" || dockFlow.after.texture !== "assets") {
  console.error("❌ 收起之后卡没回到「素材」窗格 ⇒ 它们跑到别处去了 ✗（搬卡顺序那条取舍的判据 ✓）");
  await capture("layout-dock-homes-wrong");
  process.exit(1);
}
const shotDock = await capture("layout-dock-and-tabs");

const errors = consoleLines.filter(
  (line) => /error|uncaught|exception|failed/i.test(line) && !/favicon/i.test(line),
);
console.log(`  ⑤ 控制台错误：${errors.length}`);
if (errors.length > 0) {
  errors.slice(0, 4).forEach((line) => console.error("     " + line.slice(0, 160)));
  process.exit(1);
}
console.log(
  JSON.stringify(
    {
      ok: true,
      scroll,
      tabs: Object.fromEntries(Object.entries(tabStates).map(([key, value]) => [key, value.visibleCards])),
      movedToFileMenu: movedOut,
      stageScroll: { zoomed: zoomed.overflow, fitted: fitted.overflow, canScroll: zoomed.canScroll, pageStillLocked: zoomed.pageStillLocked },
      dockHomes: dockFlow,
      screenshots: [shotTabs, shotZoom, shotDock],
    },
    null,
    2,
  ),
);
ws.close();
