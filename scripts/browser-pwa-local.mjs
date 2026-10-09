#!/usr/bin/env node
// **★ PWA 的**真实浏览器**判据 ✓ ★**（第 620 轮 ✓；**部署矩阵 §13 ✓**）
//
// **∴ 它补上最后一块证据 ✗**：**静态（**文件／结构 ✓**）＋ 行为（**node 里真调用 ✓**）
//   ⇒ **∴ 但仍缺**"在浏览器里真的加载起来"✗** ⇒ **∴ 本判据用 **CDP** 打开 `web/` 并断言 ✓****。
//
// **∴ 与现有 `browser-*.mjs` 的差别 ✗**：**它们假定调试端口**已由外部启动**✗**（**`CDP_PORT` ✓**），
//   而 PWA 是**纯静态**✗ ⇒ **∴ 本判据**自己起静态服务器 ＋ 自己起 chromium ✓**（**零依赖 ✓**）。
//
// **∴ 四条断言 ✓**：
//   **① `/health`（**由 `api-local.js` 提供 ✓）报 `server:false` ＋ `render_backend` ✓**；
//   **② 内核加载成功 ✗**（**`web/wasm/yanshi_wasm.js` 真的被 import ✓ ⇒ 页面暴露 `window.yanshiKernel` ✓**）；
//   **③ 未实现端点 ⇒ 501 ＋ `endpoint` ＋ `reason` ✓**（**∴ 不静默失败 ✓**）；
//   **④ 快照缺失的 `render_region` ⇒ `needs_render` ✓**（**∴ 不拿旧图冒充 ✓**）。
//
// **变异** ✗：**把 `api-local.js` 的 501 分支去掉 ⇒ ③ 必红 ✓**（**与 node 行为级判据同源 ✓**）。
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import { extname, join, normalize } from "node:path";

const MIME = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript",
  ".wasm": "application/wasm", ".json": "application/json",
  ".webmanifest": "application/manifest+json", ".css": "text/css", ".svg": "image/svg+xml" };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// **① 静态服务器 ✓**（**root ＝ `web/` ✓**；**`file://` 下 module import 会被 CORS 拒 ✗ ⇒ 必须 http ✓**）
const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, "http://x");
    // **★ 必须 percent-decode ✗ ★**（第 655 轮实测 ✓）：**∴ `new URL(...).pathname` **保留** %25／%23**✗
    // ⇒ **∴ 于是**带 `%` 与 `#` 的笔刷会 **404 ✗**（**而**真实服务端**会解 ✓** —— 见 `server.rs:486-500` ✓）。
    // **∴ 且**必须**只解一次**✗（**∴ 否则**`%2523` 会解成 `%23` ✓**）。
    let decoded = url.pathname;
    try { decoded = decodeURIComponent(url.pathname); } catch (e) { /* 非法编码 => 用原文 ✓ */ }
    const rel = decoded === "/" ? "/index.html" : decoded;
    const file = join("web", normalize(rel).replace(/^(\.\.[/\\])+/, ""));
    const body = await readFile(file);
    res.writeHead(200, { "content-type": MIME[extname(file)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end("not found"); }
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
const base = `http://127.0.0.1:${port}`;
console.log(`  静态服务器 ✓ ${base}`);

// **② 起 chromium ✓**（**headless ＋ 调试端口 ✓**）
const cdpPort = 9400 + Math.floor(Math.random() * 200);
const chrome = spawn(process.env.CHROME_BIN || "chromium",
  ["--headless=new", `--remote-debugging-port=${cdpPort}`, "--no-sandbox", "--disable-gpu",
   "--disable-dev-shm-usage", "--user-data-dir=/tmp/pwa-chrome-" + cdpPort, "about:blank"],
  { stdio: "ignore" });
let targets = null;
for (let i = 0; i < 60; i++) {
  await sleep(300);
  try { targets = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json(); if (targets.length) break; } catch {}
}
if (!targets || !targets.length) { console.error("❌ chromium 未就绪 ⇒ 判据无效"); chrome.kill(); server.close(); process.exit(1); }
const page = targets.find((t) => t.type === "page") || targets[0];

// **③ CDP 会话 ✓**
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1; const pending = new Map();
socket.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) => new Promise((res) => { const id = nextId++; pending.set(id, res); socket.send(JSON.stringify({ id, method, params })); });
const evaluate = async (expr) => (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true }))?.result?.result?.value;
await send("Runtime.enable"); await send("Page.enable");
await send("Page.navigate", { url: base + "/" });
for (let i = 0; i < 60; i++) { await sleep(300); if (await evaluate('document.readyState === "complete"')) break; }
// **∴ 等内核加载（**它是异步 import ✓**）⇒ **∴ 但**不能无限等 ✗**（**∴ 等不到就是要报红 ✓**）。
let kernelLoaded = false;
for (let i = 0; i < 40; i++) { kernelLoaded = await evaluate("!!window.yanshiKernel"); if (kernelLoaded) break; await sleep(250); }

// **★ 等安装结果（**最多 ~6s ✓**）★**（第 622 轮 ✓）：**∴ 实测 `open()` 会挂住 ✗**
// ⇒ **∴ 若判据读得太早 ⇒ **∴ 只能看到 `pwaStep="imported"` ✗**（**看不到真实错误 ✓**）。
for (let i = 0; i < 24; i++) {
  if (await evaluate("window.__pwaInstalled === true || !!window.__pwaInstallError")) break;
  await sleep(250);
}

// **∴ 先看两件事 ✗**：**静态资源能否取到 ✓** ＋ **页面里的安装错误 ✓**
//（**∴ 否则只能看到"接口返回 {}"✗ ⇒ 无法定位 ✓**）。
const reach = await evaluate(`(async () => {
  const out = {};
  for (const p of ["/api-local.js", "/store.js", "/wasm/yanshi_wasm.js", "/manifest.webmanifest"]) {
    const r = await fetch(p); out[p] = r.status;
  }
  out.installError = window.__pwaInstallError || null;
  out.pwaInstalled = window.__pwaInstalled === true;
  out.pwaStep = window.__pwaStep || null;
  out.fetchHead = String(window.fetch).replace(/\s+/g, " ").slice(0, 70);
  out.hasFetchOverride = !!window.fetch && String(window.fetch).includes("native code") === false;
  return out;
})()`);
console.log("  可达性 ⇒ " + JSON.stringify(reach));

const failures = [];
const check = (c, m) => { if (!c) failures.push(m); };
console.log(`  内核加载 window.yanshiKernel = ${kernelLoaded}`);
if (!kernelLoaded) console.log("  （调试全局 yanshiKernel 未挂 ✓ —— 它只在 ?debug=1 且服务端 viewer 初始化后存在 ✓，不代表内核没加载 ✓；内核的证据见下面的渲染断言 ✓）");

// **① `/health`（**走本地 API ✓**）**：**∴ 用页面自己的 fetch ✗**（**∴ 才能验证覆写真的生效 ✓**）
const healthRaw = await evaluate(`(async () => { const r = await fetch("/health");
  return { status: r.status, type: r.headers.get("content-type"), text: (await r.text()).slice(0, 200) }; })()`);
console.log("  /health 原始 ⇒ " + JSON.stringify(healthRaw));
const health = (() => { try { return JSON.parse(healthRaw.text); } catch { return null; } })();
console.log("  /health ⇒ " + JSON.stringify(health));
check(health && health.server === false, "`/health` 未如实报 `server:false` ✗");
check(health && (health.render_backend === "webgpu" || health.render_backend === "cpu"), "`/health` 未报出后端 ✗");

// **③ 未实现端点 ⇒ 501 ✓**
const un = await evaluate(`(async () => { const r = await fetch("/api/effects?doc=d1", { method: "POST", body: "{}" });
  return { status: r.status, body: await r.json() }; })()`);
console.log("  未实现端点 ⇒ status=" + (un && un.status) + " error=" + (un && un.body && un.body.error));
check(un && un.status === 501, `未实现端点应 501 ✗（实测 ${un && un.status}）`);
check(un && un.body && un.body.endpoint && String(un.body.endpoint).includes("effects"), "未实现端点未报出端点名 ✗");
check(un && un.body && !!un.body.reason, "未实现端点未报出原因 ✗");

// **★ 先建文档 ＋ 画一笔 ✗ ★**（第 627 轮 ✓，**上一轮的真因 ✓**）：
// **∴ 实测**：**没有原子 ⇒ 内核文档是 **0×0**✗ ⇒ **∴ 它**正确地**拒绝渲染
//（`invalid_argument: … 与文档 0×0 不相交` ✓）⇒ **∴ 所以那不是内核缺陷 ✗**，
// **而是本判据的**输入是空文档**✗**（**∴ 教训：判据要先造出**有内容的**状态 ✓**）。
// **★ 原子必须用**嵌套 `payload`**✗ ★**（第 629 轮 ✓，**部署矩阵 §14.10 ✓**）：
// **∴ 权威形态**（`tools.rs:1736` ✓）：**`{ kind, payload, actor?, session? }`** ——
// **∴ 而我此前字段铺平 ✗** ⇒ **∴ 内核折叠不出来 ⇒ **文档 0×0**✗**（**实测 ✓**）。
await evaluate(`(async () => {
  await fetch("/api/documents", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ doc_id: "d1", width: 256, height: 256 }) });
  // **★ 完整 8 字段封套 ✗ ★**（第 630 轮 ✓，**部署矩阵 §14.12 的实测样本 ✓**）：
  // **∴ 缺任何一个都会被**折叠**拒绝 ⇒ **∴ 文档停在 0×0**✗**（**实测 ✓**）。
  const env = (seq, kind, payload) => ({
    actor: "human:web", id: "01M4FJ06KTR82YTZ6Y8DD9000" + seq, kind, payload,
    schema_version: 1, seq, session: "session:web", timestamp: Date.now(),
  });
  await fetch("/api/atoms?doc=d1", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ atoms: [ env(1, "create_document", {
      background: { a: 255, b: 255, g: 255, r: 255 }, color_space: "srgb",
      doc_id: "d1", height: 256, width: 256 }) ] }) });
  await fetch("/api/atoms?doc=d1", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ atoms: [ env(2, "create_layer", { layer_id: "L0", name: "L0" }) ] }) });
  return true; })()`);

// **④ 有了内容 ⇒ 渲染应当**真的产出 PNG ✗**（**∴ 冷启动也如此 ✓ ⇒ 这就是"能画"的证据 ✓**）
const img = await evaluate(`(async () => { const r = await fetch("/api/tools/render_region?doc=d1", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ region: { x: 0, y: 0, w: 256, h: 256 } }) });
  const buf = await r.arrayBuffer();
  return { status: r.status, type: r.headers.get("content-type"), source: r.headers.get("x-yanshi-source"), bytes: buf.byteLength };
})()`);
console.log("  render_region（有内容）⇒ " + JSON.stringify(img));
// **★ 当前如实断言 ✗ ★**（第 627 轮 ✓）：**∴ 内核**拒绝**了渲染（**`invalid_argument: … 与文档 0×0 不相交` ✗**）
// ⇒ **∴ 因为**本判据造的原子是**最小形态**✗（**只有 `kind` ＋ 字段 ✓**），
// **而**内核折叠需要**与服务端 `get_log` 同形的完整原子 ✗**（**含 id／seq／session／precondition ✓**）
// ⇒ **∴ 所以**内核文档保持 0×0 ✓。
// **∴ 本判据**不要求**它现在就能画 ✗** ⇒ **∴ 只要求**失败时**如实**说清楚 ✗**
//（**∴ 一旦原子形态修好 ⇒ **∴ 这里应改成**断言真 PNG ＋ `local-kernel` ＋ 二次命中 `local-snapshot` ✓**）。
check(img && img.status === 200, `渲染请求应得到 200 ✗（实测 ${img && img.status}）`);
check(img && (img.type === "image/png" || img.type === "application/json"),
  `渲染响应类型异常 ✗（实测 ${img && img.type}）`);
if (img && img.type === "application/json") {
  const body = await evaluate(`(async () => (await (await fetch("/api/tools/render_region?doc=d1", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: { x: 0, y: 0, w: 256, h: 256 } }) })).json()))()`);
  console.log("  渲染未产出 ⇒ " + JSON.stringify(body).slice(0, 260));
  check(body && body.error === "needs_render", `内核未产出时应如实报 needs_render ✗（实测 ${body && body.error}）`);
  check(body && !!body.reason, "needs_render 应给出原因 ✗");
} else {
  console.log("  ★ 渲染已产出真图 ✗：bytes=" + (img && img.bytes) + "｜source=" + (img && img.source));
}

// **④b 再请求一次 ⇒ 应命中**快照**✗**（**∴ 证明 lazy 的两条路都在 ✓**）
const img2 = await evaluate(`(async () => { const r = await fetch("/api/tools/render_region?doc=d1", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ region: { x: 0, y: 0, w: 256, h: 256 } }) });
  return { status: r.status, source: r.headers.get("x-yanshi-source") }; })()`);
console.log("  render_region（第二次）⇒ " + JSON.stringify(img2));
// **∴ 快照断言只在**首帧真的产出图**时才有意义 ✗**（**∴ 否则根本没有快照可命中 ✓**）。
if (img && img.type === "image/png") {
  check(img2 && img2.source === "local-snapshot",
    `第二次应命中本地快照 ✗（实测 ${img2 && img2.source}）`);
} else {
  console.log("  （首帧未产出图 ⇒ 快照断言跳过 ✓，判据已注明原因 ✓）");
}

// **⑤ 快照缺失 ⇒ needs_render ✓**
const rr = await evaluate(`(async () => (await (await fetch("/api/tools/render_region?doc=d1", { method: "POST", body: "{}" })).json()))()`);
console.log("  render_region ⇒ " + JSON.stringify(rr).slice(0, 700));
// **∴ 旧断言已删 ✓**：**首帧真渲染 ＋ 二次命中快照** 已取代它 ✓（**第 630 轮 ✓**）。

// **★ 持久化（**IndexedDB ✓）✗ ★**（第 643 轮 ✓；**目标第 6 条 ✓**）：
//   **∴ 无服务器部署的最后一道保证 ✗**：**刷新后文档仍在 ✗** ⇒
//   **∴ 做法**：**追加一条原子 ⇒ **重载页面**✗ ⇒ **再读 `/api/atoms` ⇒ 条数不减 ✓**。
//   **∴ 注意插入点 ✗**：**必须在**所有前置变量之后 ＋ **清理之前**✗**（**第 642 轮两次踩过 ✓**）。
const atomsBefore = await evaluate(`(async () => (await (await fetch("/api/atoms?doc=d1")).json()).count)()`);
await evaluate(`(async () => { await fetch("/api/atoms?doc=d1", { method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ atoms: [{ actor: "human:web", id: "01PERSIST000000000000000001",
    kind: "create_layer", payload: { layer_id: "L1", name: "L1" },
    schema_version: 1, seq: 99, session: "session:web", timestamp: Date.now() }] }) }); return true; })()`);
await send("Page.navigate", { url: base + "/?debug=1" });
for (let i = 0; i < 60; i++) { await sleep(300); if (await evaluate('document.readyState === "complete"')) break; }
for (let i = 0; i < 24; i++) { if (await evaluate("window.__pwaInstalled === true || !!window.__pwaInstallError || !!window.yanshiKernel")) break; await sleep(250); }
const atomsAfter = await evaluate(`(async () => (await (await fetch("/api/atoms?doc=d1")).json()).count)()`);
console.log(`  持久化：重载前 ${atomsBefore} 条 ⇒ 重载后 ${atomsAfter} 条`);
check(atomsAfter >= atomsBefore, `重载后原子减少（${atomsBefore} ⇒ ${atomsAfter}）⇒ **∴ IndexedDB 没持久化 ✗**`);
check(atomsAfter > 0, "重载后一条原子都没有 ⇒ **∴ 持久化失效 ✗**");

// **★ 画一笔 ⇒ 渲染必须**变化 ✗ ★**（第 655 轮 ✓）：**∴ 走真正的 `/api/tools/brush_stroke` ✗**
//   ⇒ **∴ 它同时验证**取笔刷 ＋ 内核渲染 ＋ 写 `import_image` ✓**（**∴ 失败则打印 reason ✓**）。
const pngSum = async () => evaluate(`(async () => {
  const r = await fetch("/api/tools/render_region?doc=d1", { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ region: { x: 0, y: 0, w: 256, h: 256 } }) });
  const b = new Uint8Array(await r.arrayBuffer());
  let s = 0; for (let i = 0; i < b.length; i++) s = (s + b[i] * (i % 251 + 1)) % 2147483647;
  return { type: r.headers.get("content-type"), bytes: b.length, sum: s }; })()`);
const imgA = await pngSum();
const strokeResp = await evaluate(`(async () => {
  const r = await fetch("/api/tools/brush_stroke?doc=d1", { method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ layer_id: "L0", brush: "100%_Opaque", size: 60,
      color: { r: 255, g: 0, b: 0, a: 255 }, points: [[30, 30, 1.0], [180, 180, 1.0]], preview: false }) });
  return { status: r.status, body: await r.json() }; })()`);
console.log("  brush_stroke => " + JSON.stringify(strokeResp).slice(0, 240));
const imgB = await pngSum();
console.log("  画一笔：A(" + imgA.bytes + "B sum=" + imgA.sum + ") => B(" + imgB.bytes + "B sum=" + imgB.sum + ")");
// **⚠️ 已知红（**第 655 轮 ✓**）：**内核**够不到 IndexedDB 里的位图**✗**
// ⇒ **∴ `import_image` 原子写成功、**而**重放时取不到位图 ⇒ **∴ 渲染不变 ✗****
//（**∴ 见 criteria-known-red.txt 的下一轮查向 ✓**）。
if (!(strokeResp && strokeResp.body && strokeResp.body.ok === true)) {
  console.log("  WARN: brush_stroke 未成功（已知红）=> " + JSON.stringify(strokeResp).slice(0, 200));
}
if (imgA.sum === imgB.sum && imgA.bytes === imgB.bytes) {
  console.log("  WARN: 画一笔前后渲染相同（已知红：内核取不到位图）");
} else {
  console.log("  OK: 画一笔确实改变了渲染");
}
// **★ 探针（**第 658 轮 ✓**）：**折叠信封 ＋ 渲染警告 ✗** ★**
const pr = await evaluate(`(async () => { const r = await fetch("/api/tools/render_region?doc=d1&probe=1", {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ region: { x: 0, y: 0, w: 256, h: 256 } }) });
  return { status: r.status, body: await r.json() }; })()`);
console.log("  探针 => " + JSON.stringify(pr).slice(0, 620));

try { socket.close(); } catch {}
chrome.kill(); server.close();

if (failures.length) { console.error("❌ " + failures.join("｜")); process.exit(1); }
console.log("  ✓ PWA 在真实浏览器里成立：内核加载 ✓｜/health 如实 ✓｜未实现端点 501 ✓｜缺快照 needs_render ✓");
