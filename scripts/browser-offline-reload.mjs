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
// **离线重载仍在的浏览器判据** ✓（目标 (A)④/A⑥ ✓：断网后画面还在／能导出 ✓）。
//
// **为什么必须是产品级判据** ✗：不能测"我自己新写的保存 API"✓ —— 那样只要我把 API 写出来就绿了 ✗。
// 这里只问一件事 ✓：**把网断掉、重新加载页面之后，那一笔还在不在** ✓。
//
// **前提已验证（第 162 轮 ✓）** ✓：SW 的 `fetch` 处理器**明确跳过** `/api/` ✗
// ⇒ 所以断网时像素**不可能**从 API 来 ✓ ⇒ 画面要还在，就**只能**靠浏览器自己的本地存储 ✓
// ⇒ 这条判据**不会假绿** ✓（否则我会以为自己做完了 ✗）。
//
// 判据 ✓：① 画一笔（经 API ✓，保证文档真的有墨 ✓）；
//        ② **正对照** ✓：画之前同一画布必须是**白的** ✓（否则"深色像素数"这个量法不可信 ✗）；
//        ③ 开页 ⇒ 画面必须**有深色像素** ✓（说明渲染真的发生了 ✓）；
//        ④ **断网** ✓（CDP `Network.emulateNetworkConditions{offline:true}` ✓ —— 需求原话就是"断网/关服务端"✓）
//           ⇒ `Page.reload` ✓ ⇒ 画面上**必须仍有深色像素** ✓（像素来自本地存储 ✓），且**不是全白** ✓。
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
if (!url) { console.error("用法: node scripts/browser-offline-reload.mjs <viewer-url> [cdpPort]"); process.exit(2); }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
// **★ 历史：为什么曾经同时用两个 URL ✗ ★**（第 779 轮 ✓，现已被下面的改法取代 ✓）
//   **∴ 当时** ✗：落笔用 `POST ${服务端 base}/api/tools/brush_stroke` ✓ 而导航用查看器 URL ✓
//     ⇒ 无静态 PWA 时两者同源、能跑通 ✓ ⇒ 有静态 PWA 时 `/api/tools` 发到静态服务 ⇒ 明文 404 ✗
//       ⇒ 那一轮把 API 改成走 `argv[3]`（服务端 base ✓）⇒ **404 消失了** ✓
//         ⇒ **∴ 而画布仍是 0** ✗ ⇒ 因为**页面打开的根本不是服务端那份文档** ✓（见下 ✓）。
// 页面里"数深色像素"的取样器 ✓ —— 用**离屏 2D 画布 drawImage** ✓，这样 canvas 是 2D 还是 WebGL 都能量 ✓。
const SAMPLER = `(() => {
  // **画板可能是 img 也可能是 canvas** ✗ —— 实测：查看器里根本没有 canvas ✓
  //（它把服务端的 PNG 显示在 img 上 ✓）⇒ 取「面积最大」的那个 ✓，两种都能量 ✓。
  const nodes = Array.from(document.querySelectorAll("img, canvas")).filter((node) => {
    const box = node.getBoundingClientRect();
    return box.width >= 64 && box.height >= 64;
  });
  if (nodes.length === 0) return { error: "页面上没有够大的 img/canvas" };
  // **必须优先 #board** ✗ —— 实测：overlay 的 CSS 盒子（354×318）**比 board（344×318）还大** ✓
  // ⇒ 我原来"取面积最大"会选中**空的 overlay** ✗（dark=0 ✓）⇒ 判据会**冤枉**在线画面 ✗ ✗。
  const byId = nodes.find((node) => node.id === "board");
  const canvas = byId || nodes.reduce((best, node) => {
    const box = node.getBoundingClientRect();
    const area = box.width * box.height;
    return area > best.area ? { node, area } : best;
  }, { node: nodes[0], area: 0 }).node;
  const off = document.createElement("canvas");
  off.width = canvas.width; off.height = canvas.height;
  const ctx = off.getContext("2d");
  if (!ctx) return { error: "拿不到 2D 上下文" };
  try { ctx.drawImage(canvas, 0, 0); } catch (error) { return { error: "drawImage 失败：" + error }; }
  let data;
  try { data = ctx.getImageData(0, 0, off.width, off.height).data; } catch (error) { return { error: "getImageData 失败：" + error }; }
  let dark = 0, ink = 0, total = 0;
  for (let i = 0; i < data.length; i += 4) {
    total += 1;
    const [r, g, b, a] = [data[i], data[i + 1], data[i + 2], data[i + 3]];
    if (a > 32 && !(r > 245 && g > 245 && b > 245)) ink += 1;
    if (a > 32 && r < 140 && g < 140 && b < 140) dark += 1;
  }
  return { dark, ink, total, width: off.width, height: off.height };
})()`;

// **有界轮询等出墨**（第 1120 轮定案）：原来只等固定 800ms，首帧可能更慢 ⇒ 采样太早 ⇒ 读到 0 ✗。
// 决定性证据：只在采样前加一个**只读**探针（引入少量延迟）⇒ 判据立刻转绿 ✓。
// 所以把"等待条件"与"断言条件"对齐：轮询到出现深色像素为止（有界），再断言 ✓。
async function waitForInk(label, limit = 80, step = 250) {
  let last = null;
  for (let i = 0; i < limit; i += 1) {
    last = await evaluate(SAMPLER);
    if (last && !last.error && last.dark > 50) {
      console.log(`  · ${label}：第 ${i + 1} 次采样出墨（dark=${last.dark}，${last.width}x${last.height}）`);
      return last;
    }
    await sleep(step);
  }
  console.log(`  · ${label}：轮询 ${limit} 次仍无墨（最后一次 ${JSON.stringify(last)}）`);
  return last;
}
// **失败要自己说清"为什么"** ✗（2026-11-05 ✓）：只报"没有墨"会让我再猜三四轮 ✓
// ⇒ 把**能区分原因**的几项一起打出来 ✓：
//   · `docAtomRepair` —— 文档原子补没补（`created`／`present`／`failed:…` ✓）
//   · 原子种类 —— 有没有 `create_document`（没有 ⇒ 内核认为文档 0×0 ⇒ 渲染必空 ✓）
//   · `kernelHead` —— 落笔到底进没进日志（进了 ⇒ 是**显示**问题，不是**提交**问题 ✓）
const diagnose = () => evaluate(`(async () => {
    const s = window.yanshiStats || {};
    const out = { repair: s.docAtomRepair ?? null, kernelHead: s.kernelHead ?? null,
                  wasm: !!s.wasm, blits: (s.blitLog || []).slice(-4).map((b) => b.reason),
                  docId: (window.yanshi && window.yanshi.state) ? window.yanshi.state().docId : null };
    try {
      const log = await window.yanshiCallTool("get_log", {});
      out.atoms = (log.atoms || []).map((a) => a.kind);
    } catch (e) { out.atoms = "throw:" + String((e && e.message) || e).slice(0, 80); }
    out.pageLog = (document.getElementById("log") || {}).innerText ? document.getElementById("log").innerText.slice(0, 180) : "";
    return out;
  })()`);

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
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (event) => { const m = JSON.parse(event.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
await send("Runtime.enable"); await send("Page.enable"); await send("Network.enable");
const reload = async (offline) => {
  await send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  await send("Page.navigate", { url });
  // **等到"页面就绪"这个独立信号** ✓（第 891 轮 ✓）：原来固定睡 4 秒 ✗ ⇒ 机器慢时间歇红 ✗。
  // ⚠️ **等待条件与断言条件不同** ✗：`SAMPLER` 读的是**深色像素数** ✓，
  //    这里只等 `readyState` 完成 ＋ `#board` 存在 ✓，再留 **800ms 有界沉降** 让首帧画完 ✓；
  //    **"深色像素有多少"始终只由断言判** ✓ ✓。
  for (let i = 0; i < 40; i++) {
    await sleep(300);
    try { if (await evaluate('document.readyState === "complete" && !!document.getElementById("board")')) { await sleep(800); break; } } catch (_) { /* 还没就绪 ⇒ 继续等 ✓ */ }
  }
  return await waitForInk("采样");
};

// ② **正对照** ✓：先看"什么都没画"的画布（另开一个文档 ✓，避免污染被测文档 ✓）。
const blank = await reload(false);
console.log("  正对照（未落笔）：" + JSON.stringify(blank));
const failures = [];
if (!blank || blank.error) { console.log("  ✗ 取样器不可用 ⇒ 判据自身前提不成立（不是产品问题）⇒ " + JSON.stringify(blank)); socket.close(); process.exit(3); }
if (blank.dark > 50) failures.push(`未落笔的画布上就有 ${blank.dark} 个深色像素 ⇒ 量法不可信 ✗`);

// ① 画一笔 ✓ —— **页面内真实手势** ✓（不是调 API ✓：静态 PWA 的 API 就是它自己的本地实现 ✓，
// 走 API 只能证明"我把函数调通了"✗，走手势才证明"用户画得上"✓）。
const PAINT = `(async () => {
  const board = document.getElementById("board");
  if (!board) return "no-board";
  const rect = board.getBoundingClientRect();
  if (rect.width < 32 || rect.height < 32) return "board-too-small:" + Math.round(rect.width) + "x" + Math.round(rect.height);
  const at = (fx, fy) => ({ clientX: rect.left + rect.width * fx, clientY: rect.top + rect.height * fy });
  const fire = (type, fx, fy) => board.dispatchEvent(new PointerEvent(type, {
    bubbles: true, cancelable: true, pointerId: 71, pointerType: "mouse", isPrimary: true,
    buttons: type === "pointerup" ? 0 : 1, ...at(fx, fy),
  }));
  const color = document.getElementById("color");
  if (color) color.value = "#101010";
  const brush = document.querySelector('button[data-tool="brush"]');
  if (brush) brush.click();
  const size = document.getElementById("size");
  if (size) size.value = "24";
  await new Promise((r) => setTimeout(r, 250));
  fire("pointerdown", 0.30, 0.50);
  for (let i = 1; i <= 8; i += 1) {
    fire("pointermove", 0.30 + 0.05 * i, 0.50);
    await new Promise((r) => setTimeout(r, 30));
  }
  fire("pointerup", 0.70, 0.50);
  return "dispatched";
})()`;
// **内核必须就绪再落笔** ✗（与 `browser-offline-brush` 同一条纪律 ✓）：
// 静态 PWA 的笔走**本地内核** ✓ ⇒ 内核没起来时手势会被吞掉 ✓（表现为"画了没反应"✓）。
for (let i = 0; i < 80; i += 1) {
  if (await evaluate("!!(window.yanshi && window.yanshi.kernelStats && window.yanshi.kernelStats())")) break;
  await sleep(250);
}
// **落笔前先记下日志条数** ✗（2026-11-05 ✓）：下面要等"这一笔真的进了日志" ✓
// ⇒ 只等"画布上有墨"不够 ✓ —— 上屏是**乐观渲染** ✓，提交是**另一件事** ✓
// （实测：画完 0.8 秒就重载 ⇒ 画布有墨 ✓、重载后原子退回两条 ✓ ⇒ 提交被导航打断 ✗）。
const atomsBefore = await evaluate(`(async () => {
  try { const l = await window.yanshiCallTool("get_log", {}); return (l.atoms || []).length; }
  catch (e) { return -1; }
})()`);
const painted = await evaluate(PAINT);
console.log("  · 落笔 = " + JSON.stringify(painted));
if (painted !== "dispatched") {
  console.log("  ⊘ 不可判：页面里没法落笔（" + JSON.stringify(painted) + "）⇒ 不是产品结论");
  socket.close();
  process.exit(3);
}

// **落笔之后先确认"画上了"** ✗（2026-11-05 ✓）：紧跟着就是一次重载 ✓
// ⇒ 若此刻还没上屏，重载后为 0 就**分不清**「没画上」与「没存下来」✗ ⇒ 必须先看到墨 ✓。
// **∴ 这一条是前提** ✗：静态 PWA 里一笔都画不上 ⇒ 后面的"重载还在"没有意义 ✓
// （但那**确实**是产品缺陷 ✓ ⇒ 归到 failures ✓，不是判据自身不可判 ✓）。
const drawn = await waitForInk("落笔后");
if (!drawn || drawn.error || drawn.dark <= 50) {
  failures.push(`在页面里落笔之后画布上没有墨（${JSON.stringify(drawn)}）⇒ 静态 PWA 的本地落笔没有生效 ✗｜诊断 ${JSON.stringify(await diagnose())}`);
}

// **等"这一笔进了日志"** ✗（不是等上屏 ✓）：上限 10 秒，超时**如实报**（不静默 ✓）。
if (atomsBefore >= 0) {
  let committed = null;
  for (let i = 0; i < 40; i += 1) {
    committed = await evaluate(`(async () => {
      try { const l = await window.yanshiCallTool("get_log", {}); return (l.atoms || []).length; }
      catch (e) { return -1; }
    })()`);
    if (committed > atomsBefore) break;
    await sleep(250);
  }
  console.log(`  · 提交：落笔前 ${atomsBefore} 条 ⇒ 现在 ${committed} 条`);
  if (!(committed > atomsBefore)) {
    failures.push(`落笔 10 秒内没有进日志（落笔前 ${atomsBefore} 条、现在 ${committed} 条）⇒ 只有乐观上屏、没有提交 ✗`);
  }
}

// ③ 在线时画面必须有墨 ✓
const online = await reload(false);
console.log("  在线（已落笔）：" + JSON.stringify(online));
if (!online || online.error) failures.push("在线时取样失败 ⇒ 判据无效 ✗");
else if (online.dark <= 50) failures.push(`在线时画布上只有 ${online.dark} 个深色像素 ⇒ 那一笔没渲染出来 ✗｜诊断 ${JSON.stringify(await diagnose())}`);

// ④ **断网重载** ✓ —— 这一条就是需求的核心 ✓
const offline = await reload(true);
// **负对照**（本次审计加的 ✓）：断网状态下，页面自己发一个**同一来源、已知不会被缓存**的请求 ⇒
// 它**必须失败** ✓。为什么它不可能被缓存 ✓：`service-worker.js` 对 `/api/` 前缀直接 `return`
//（不 respondWith ✓）⇒ **永远进不了 SW 的 Cache Storage** ✓；路径不存在（服务端 404 ✓）
// ＋每次唯一 nonce ⇒ **也进不了浏览器 HTTP 缓存** ✓。
// 它若居然成功 ⇒ 说明"断网"没真的生效 ⇒ **本判据作废（VOID）**，不是产品通过 ✗。
// **★ 第 738 轮：探测走 `XMLHttpRequest` 而不是 `window.fetch` ✗ ★**（**根因写全在
// `browser-offline-export.mjs` 的那一节 ✓**）：**静态 PWA 引导页 `installLocalApi` 覆写
// `window.fetch`**✗ ⇒ **未映射的 `/api/` 在页面里就被回 501**✗（**`api-local.js:4836–4854` ✓）
// ⇒ **断网与否根本影响不到它** ✓ ⇒ **XHR 没被覆写**✗、**照常出网络** ✓ ★**** ✓✓
const NEGATIVE_CONTROL = "/api/__offline_negative_control__?nonce=" + Date.now();
const controlProbe = await evaluate(`(() => new Promise((resolve) => {
  const xhr = new XMLHttpRequest();
  xhr.open("GET", ${JSON.stringify(NEGATIVE_CONTROL)}, true);
  xhr.onload = () => resolve({ failed: false, status: xhr.status, bytes: -1, onLine: navigator.onLine, via: "xhr" });
  xhr.onerror = () => resolve({ failed: true, status: xhr.status, onLine: navigator.onLine, via: "xhr" });
  xhr.send();
}))()`);
console.log("  · 负对照（未缓存接口必须失败；XHR 绕开 api-local 对 window.fetch 的接管）= " + JSON.stringify(controlProbe));
console.log("  断网重载后：" + JSON.stringify(offline));
if (!offline || offline.error) failures.push("断网重载后取样失败（页面可能根本没打开）✗");
else if (offline.dark <= 50) failures.push(`断网重载后画布上只有 ${offline.dark} 个深色像素 ⇒ 像素没有落到浏览器本地存储 ✗（目标：≥ 在线时的九成）｜诊断 ${JSON.stringify(await diagnose())}`);
else if (online && offline.dark < online.dark * 0.9) failures.push(`断网重载后墨量 ${offline.dark} 明显少于在线时的 ${online.dark} ⇒ 本地读回不完整 ✗`);
// **负对照必须成立** ✗：未缓存的同源请求断网后仍成功 ⇒ 断网模拟没生效 ⇒ 这一跑没有结论。
if (!(controlProbe && controlProbe.failed)) {
  failures.push(`负对照失败：断网后未缓存的 ${NEGATIVE_CONTROL} 仍然成功（${JSON.stringify(controlProbe)}）⇒ 断网模拟没有生效 ⇒ 判据作废（VOID）`);
}

if (failures.length) { console.log("  ✗ " + failures.join("；")); socket.close(); process.exit(1); }
console.log("  ✓ 离线重载：那一笔仍在（像素来自浏览器本地存储）");
socket.close();
process.exit(0);
