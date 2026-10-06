#!/usr/bin/env node
// **「默认导出 = 整幅文档」判据** —— 外部审计的 P0 交付正确性缺陷：
// 1024×1024 的文档、显示缩放到 200%（远离"适配"）、点导出 ⇒ 落盘的文件是 **视口**（约 46x×31x），
// 不是文档。真因在 `viewer-app.js` 的点击处理器：本地优先那条直接
// `board.toBlob(...)`，而 `board.width/height` 是**视口的文档像素数**（见 `sizeBoards` 的注释）。
//
// 判据（都能红 ✓）：
//   ① **默认导出**（`#exportPng`）在显示缩放 ≠ 适配时必须落下一个 **文档尺寸** 的 PNG。
//      测量口径是**文件里的 IHDR**（不是日志文字 ✗ —— 文字可以撒谎，字节不会）。
//   ② **显式"导出当前视图"**（`#exportPngView`）必须存在、且落下的 PNG 恰好是
//      当时视口的文档像素尺寸 —— 证明"导出视口"这条路**被保留下来了**、
//      但它是**显式的、被标明的**，而不是默认偷偷裁一刀。
//   ③ 两条导出都必须真的写文件（"点得到按钮"不算 ✓）+ 日志里要有对应的结果行。
//
// 用法（经 `scripts/run-criteria.sh` 时它传 `<viewer-url> <base> <token> <cdp-port>`）：
//   node scripts/browser-export-full-document.mjs "<viewer-url>" [base] [token] [cdpPort]
// 直接跑（`scripts/with-temp-server.sh node scripts/browser-export-full-document.mjs`）时
// 只有 `<viewer-url>`：base/token 从它推导、CDP 端口从 `CDP_PORT`（缺省 9333）取。
//
// **本判据自己造文档**（1024×1024）✓ —— 不依赖 runner 给的那份 320×240 ✓：
// 缺陷只有在"文档尺寸 ≠ 视口尺寸"时才显形 ✓，而 runner 的小文档 + 小窗口会让两者撞在一起 ✗。
const url = process.argv[2];
if (!url) {
  console.error("用法: node scripts/browser-export-full-document.mjs <viewer-url> [base] [token] [cdpPort]");
  process.exit(2);
}
const fs = await import("node:fs");
const parsed = new URL(url);
const base = (process.argv[3] && process.argv[3].startsWith("http") ? process.argv[3] : parsed.origin).replace(/\/$/, "");
const port = process.env.CDP_PORT || process.argv[5] || "9333";
const dir = process.env.EXPORT_DL_DIR || "/var/tmp/yanshi-export-full-document";
const DOC = "crit_export_full";
// 缺省按审计的 1024² ✓；`EXPORT_DOC_SIZE=4096` 可复跑"4K 的耗时/内存"那一段 ✓（判据本身不变 ✓）。
const W = Number(process.env.EXPORT_DOC_SIZE) || 1024;
const H = W;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const failures = [];
const notes = [];

function pngSize(bytes) {
  if (bytes.length < 24) return null;
  if (bytes.readUInt32BE(0) !== 0x89504e47) return null;
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

/// **把 8 位 RGB/RGBA PNG 真的解开** ✓ —— 判据不能只看 IHDR 的宽高 ✓：
/// 一个"尺寸对、内容空"的整幅导出会骗过尺寸断言 ✗（内核折叠不了的重内容正是这种）。
/// 编码器有两种：本地内核是 `encode_png`（filter 0 ✓）、浏览器 `toBlob` 可能用 1–4 ✓
/// ⇒ 这里把五种 filter 都实现 ✓（只支持 8 位、非隔行；这两种之外返回 null ⇒ 判据明说不会去猜 ✗）。
async function pngDecode(bytes) {
  const zlib = await import("node:zlib");
  if (bytes.length < 8 || bytes.readUInt32BE(0) !== 0x89504e47) return null;
  let off = 8;
  let width = 0, height = 0, bitDepth = 0, colorType = 0, interlace = 0;
  const idat = [];
  while (off + 8 <= bytes.length) {
    const len = bytes.readUInt32BE(off);
    const type = bytes.toString("ascii", off + 4, off + 8);
    const data = bytes.subarray(off + 8, off + 8 + len);
    if (type === "IHDR") {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      bitDepth = data[8];
      colorType = data[9];
      interlace = data[12];
    } else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
    off += 12 + len;
  }
  if (bitDepth !== 8 || interlace !== 0 || (colorType !== 6 && colorType !== 2)) return null;
  const channels = colorType === 6 ? 4 : 3;
  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(height * stride);
  const paeth = (a, b, c) => {
    const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
    return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
  };
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const src = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    const dst = out.subarray(y * stride, (y + 1) * stride);
    const up = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? dst[i - channels] : 0;
      const b = up ? up[i] : 0;
      const c = up && i >= channels ? up[i - channels] : 0;
      let value = src[i];
      if (filter === 1) value += a;
      else if (filter === 2) value += b;
      else if (filter === 3) value += (a + b) >> 1;
      else if (filter === 4) value += paeth(a, b, c);
      dst[i] = value & 0xff;
    }
  }
  return { width, height, channels, pixels: out };
}
/// **墨量** ✓：文档背景是白的 ✓、画的那笔是深灰（10,10,10）✓ ⇒ 数"明显不是背景"的像素 ✓。
function inkCount(image) {
  if (!image) return null;
  let ink = 0;
  const { pixels, channels } = image;
  for (let i = 0; i + channels - 1 < pixels.length; i += channels) {
    if (pixels[i] < 200 || pixels[i + 1] < 200 || pixels[i + 2] < 200) ink++;
  }
  return ink;
}

async function waitForFile(listDir, timeoutMs) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const files = fs.existsSync(listDir) ? fs.readdirSync(listDir).filter((n) => !n.endsWith(".crdownload")) : [];
    if (files.length > 0) return files;
    // **有界等待** ✓：不把"文件出现"当成继续条件的一部分去断言 ✓（那会让断言恒真 ✗，见第 887 轮）。
    await sleep(200);
  }
  return fs.existsSync(listDir) ? fs.readdirSync(listDir).filter((n) => !n.endsWith(".crdownload")) : [];
}

// ① 造一份 1024×1024 的文档并画一笔（空文档导出来全是背景，分不清"整幅"与"空白视口"）。
const created = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC, width: W, height: H }),
}).then((r) => r.json());
if (!created || !created.token) {
  console.error("❌ 无法创建 " + W + "×" + H + " 文档（服务端没给 token）⇒ 判据前置不成立：" + JSON.stringify(created));
  process.exit(1);
}
const token = created.token;
const post = async (tool, args) =>
  (await fetch(`${base}/api/tools?doc=${DOC}&token=${token}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  })).json();
// 新建文档通常**自带的就有一个图层** ✓ ⇒ `create_layer` 会以 `precondition_failed` 被拒 ✓
//（"已存在"不是失败 ✗ —— 只要下面能真的画出笔迹 ✓）。真正的前置是"有图层可画" ✓。
const layer = await post("create_layer", { layer_id: "layer_default", name: "l" });
const layers = await post("list_layers", {});
const layerId = (layer && layer.ok && layer.layer_id) ||
  ((layers && layers.layers) || []).map((l) => l.layer_id || l.id).filter(Boolean)[0];
if (!layerId) {
  console.error("❌ 文档里没有可用图层 ⇒ 判据前置不成立：" + JSON.stringify({ layer, layers }).slice(0, 300));
  process.exit(1);
}
const stroke = await post("brush_stroke", {
  layer_id: layerId,
  brush: "classic-brush",
  size: 32,
  color: { r: 10, g: 10, b: 10, a: 255 },
  points: [[80, 120, 0.9], [300, 120, 0.9], [520, 300, 0.9], [900, 900, 0.9]],
});
notes.push("造文档：" + W + "×" + H + "｜layer=" + layerId + "｜create_layer.ok=" + (layer && layer.ok) + "｜brush_stroke.ok=" + (stroke && stroke.ok));
if (!stroke || stroke.ok === false) {
  console.error("❌ 造文档失败（画不出笔迹）⇒ 判据前置不成立：" + JSON.stringify(stroke).slice(0, 300));
  process.exit(1);
}

// ② 连 CDP（页面由 runner 打开；这里自己导航到 1024×1024 那份文档）。
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((t) => t.type === "page" && t.url.startsWith(parsed.origin)) || targets.find((t) => t.type === "page");
if (!page) {
  console.error("❌ 没有页面调试目标（Chromium 是否以 --remote-debugging-port=" + port + " 启动？）⇒ 判据无效");
  process.exit(1);
}
const socket = new WebSocket(page.webSocketDebuggerUrl);
let nextId = 1;
const pending = new Map();
const consoleLines = [];
// **请求级断网钩子** ✓（见离线那段 ✓）：`Fetch.requestPaused` 事件在这里被接走 ✓。
let fetchCutHandler = null;
socket.onmessage = (event) => {
  const m = JSON.parse(event.data);
  if (fetchCutHandler && m.method === "Fetch.requestPaused") { void fetchCutHandler(m); return; }
  if (m.method === "Runtime.consoleAPICalled") consoleLines.push((m.params.args || []).map((a) => String(a.value ?? a.description ?? "")).join(" "));
  if (m.method === "Runtime.exceptionThrown") consoleLines.push("exception: " + (m.params.exceptionDetails?.exception?.description || m.params.exceptionDetails?.text || ""));
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
};
await new Promise((open) => { socket.onopen = open; });
const send = (method, params) =>
  new Promise((resolve) => { const id = nextId++; pending.set(id, resolve); socket.send(JSON.stringify({ id, method, params: params || {} })); });
const evaluate = async (expr) =>
  (await send("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true })).result?.result?.value;
const waitFor = async (expr, label, timeoutMs = 30000) => {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    if (await evaluate(expr)) return true;
    await sleep(250);
  }
  console.error("  ⏱ 等待超时：" + label);
  return false;
};

await send("Runtime.enable");
await send("Page.enable");
await send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: dir }).catch(() => undefined);
await send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: dir, eventsEnabled: true }).catch(() => undefined);
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir, { recursive: true });

const viewerUrl = `${base}/?doc=${DOC}&token=${token}`;
await send("Page.navigate", { url: viewerUrl });
if (!(await waitFor(`document.readyState === "complete" && !!document.getElementById("exportPng") && !!window.yanshi`, "查看器就绪", 40000))) {
  console.error("❌ 查看器没就绪 ⇒ 判据无效");
  process.exit(3);
}
// 内核就绪 = 本地导出那条路可用（离线优先的出口）。等不到也继续：在线还有服务端那条。
await waitFor(`window.yanshiStats.wasm === true`, "WASM 内核就绪", 30000);
// **还要等"文档尺寸就绪"** ✗ —— `applyZoomInput` 的第一句是
// `if (!box || !state.docSize) return;`（早退 ✓）⇒ 若在 `state.docSize` 有值之前设值 ✓，
// 那次 `change` 会被**静默丢掉** ✗ ⇒ 后面看到 `zoomPercent=39`（**∴ 适配档 ✓**）
// ⇒ 判据自报"失去区分力" ✗（**∴ CI 实测：2026-10-06 ✓**）。
await waitFor(
  `(() => { const s = window.yanshi.state && window.yanshi.state(); return !!(s && s.docSize && s.docSize.w > 0); })()`,
  "文档尺寸就绪", 30000
);

// ③ 显示缩放设为 200%（**远离"适配"** ⇒ 视口的文档像素数必然远小于文档）。
const zoomSet = await evaluate(`(() => {
  const box = document.getElementById("zoomInput");
  if (!box) return "no-input";
  box.value = "200";
  box.dispatchEvent(new Event("change", { bubbles: true }));
  return "set";
})()`);
// **设完要确认它真的生效** ✗ —— 只发事件而不核对 ⇒ **无法区分"设置了"与"被早退丢掉"** ✓。
// 等 `#zoom` 标签离开适配档（**这是页面上真实显示的缩放 ✓**）⇒ 拿不到就说明设置没生效 ✓。
let zoomTook = false;
for (let i = 0; i < 20; i += 1) {
  await sleep(200);
  const shown = await evaluate(`(() => {
    const label = document.getElementById("zoom");
    return label ? parseInt(String(label.textContent).replace("%", ""), 10) : NaN;
  })()`);
  if (Number.isFinite(shown) && shown > 105) { zoomTook = true; break; }
}
if (!zoomTook) {
  console.error("❌ 设了 200% 但页面显示的缩放仍在适配档 ⇒ 设置那一步没生效（判据前置不成立）");
  process.exit(1);
}
await sleep(300);
// **前置条件要用"页面上真实的缩放"判定** ✗（CI 实测教训 ✓）：
// 原先读 `window.yanshi.state().zoom` 并当成 `displayScale` ⇒ 得到的值像**适配档的 scale**
// （CI 上 0.3877）⇒ 判据自报"失去区分力"而退出 ✗。
// ⇒ 改成三条**独立**证据，任一成立即算"离开适配档"：
//   ① `#zoom` 标签的百分比（`syncZoomLabel` 回填的就是它 ✓）> 105%；
//   ② `viewport` 的文档像素数**明显小于**文档尺寸（适配档下两者接近 ✓）；
//   ③ 兜底：`state().zoom`（用户缩放）> 1.05。
const view = await evaluate(`(() => {
  const s = window.yanshi.state();
  const label = document.getElementById("zoom");
  const percent = label ? parseInt(String(label.textContent).replace("%", ""), 10) : NaN;
  const inputVal = document.getElementById("zoomInput") ? document.getElementById("zoomInput").value : null;
  return { docSize: s.docSize, viewport: s.viewport, userZoom: s.userZoom,
           zoomPercent: percent, zoomInputValue: inputVal };
})()`);
const vp = view && view.viewport;
const vpArea = vp ? vp.w * vp.h : 0;
const docArea = (view && view.docSize) ? view.docSize.w * view.docSize.h : 0;
const viewportSmaller = docArea > 0 && vpArea > 0 && vpArea < docArea * 0.8;
const percentBig = Number.isFinite(view.zoomPercent) && view.zoomPercent > 105;
const userZoomBig = view && typeof view.userZoom === "number" && view.userZoom > 1.05;
const leftFit = percentBig || viewportSmaller || userZoomBig;
notes.push("显示缩放：" + zoomSet + "｜" + JSON.stringify(view));
if (zoomSet !== "set" || !view || !view.docSize || view.docSize.w !== W || view.docSize.h !== H) {
  console.error("❌ 没拿到 1024×1024 的文档尺寸（拿到 " + JSON.stringify(view) + "）⇒ 判据前置不成立");
  process.exit(1);
}
if (!leftFit) {
  console.error("❌ 显示缩放没有离开适配档（三条证据都不成立：zoomPercent=" + view.zoomPercent +
    " vpArea=" + vpArea + " docArea=" + docArea + " userZoom=" + view.userZoom + "）⇒ 判据失去区分力");
  process.exit(1);
}
const viewportW = Math.round(view.viewport.w);
const viewportH = Math.round(view.viewport.h);
notes.push("此刻视口的文档像素：" + viewportW + "×" + viewportH +
  "（文档 " + W + "×" + H + " ⇒ 若导出等于它，就是「视口裁剪」✗）");
if (viewportW === W && viewportH === H) {
  console.error("❌ 视口恰好等于文档 ⇒ 本判据无法区分（窗口太大或缩放没生效）✗");
  process.exit(1);
}

// ④ 默认导出 ⇒ 文件必须是 **文档尺寸**。
fs.rmSync(dir, { recursive: true, force: true });
fs.mkdirSync(dir, { recursive: true });
const clickedFull = await evaluate(`(() => { const b = document.getElementById("exportPng"); if (!b) return "no-button"; b.click(); return "clicked"; })()`);
const fullFiles = await waitForFile(dir, 12000);
let fullSize = null;
let fullInk = null;
if (fullFiles.length > 0) {
  const bytes = fs.readFileSync(`${dir}/${fullFiles[0]}`);
  fullSize = pngSize(bytes);
  fullInk = inkCount(await pngDecode(bytes));
  notes.push("默认导出：文件 " + fullFiles[0] + "｜" + bytes.length + " 字节｜像素 " +
    (fullSize ? fullSize.width + "×" + fullSize.height : "（不是 PNG）") + "｜墨量 " + fullInk);
} else {
  notes.push("默认导出：**下载目录里没有任何文件**");
}
const fullLog = await evaluate(`(() => { const t = document.getElementById("log").textContent || ""; return (t.split("\\n").filter((l) => l.includes("已导出 PNG")).pop() || "").trim().slice(0, 140); })()`);
notes.push("默认导出日志：" + JSON.stringify(fullLog));
const fullStats = await evaluate(`window.yanshiStats.lastExport || null`);
notes.push("默认导出 lastExport：" + JSON.stringify(fullStats));
if (clickedFull !== "clicked") failures.push("找不到默认导出按钮 #exportPng ⇒ 判据无效 ✗");
if (fullFiles.length === 0) failures.push("默认导出没有落下任何文件 ✗");
else if (!fullSize) failures.push("默认导出的文件不是 PNG（读不出 IHDR）✗");
else if (fullSize.width !== W || fullSize.height !== H) {
  failures.push("默认导出的 PNG 是 " + fullSize.width + "×" + fullSize.height +
    "，而文档是 " + W + "×" + H + "（视口是 " + viewportW + "×" + viewportH + "）⇒ **默认导出偷偷裁成了视口** ✗");
} else if (fullInk !== null && fullInk < 500) {
  // **尺寸对、内容空**同样是坏导出 ✗（内核折叠不了的重内容会这样 ✓）⇒ 判据要看像素本身 ✓。
  failures.push("默认导出的整幅 PNG 尺寸对了，但**几乎没有笔迹**（墨量 " + fullInk +
    "）⇒ 导出的是一张空文档 ✗");
}

// ⑤ 显式"导出当前视图" ⇒ 必须存在、且恰好是当时的视口尺寸（保留视口导出，但不许当默认）。
const viewButtonExists = await evaluate(`!!document.getElementById("exportPngView")`);
let viewSize = null;
let viewLog = "";
if (viewButtonExists) {
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  const viewportNow = await evaluate(`(() => { const v = window.yanshi.state().viewport; return { w: Math.round(v.w), h: Math.round(v.h) }; })()`);
  const clickedView = await evaluate(`(() => { const b = document.getElementById("exportPngView"); if (!b) return "no-button"; b.click(); return "clicked"; })()`);
  const viewFiles = await waitForFile(dir, 12000);
  if (viewFiles.length > 0) {
    const bytes = fs.readFileSync(`${dir}/${viewFiles[0]}`);
    viewSize = pngSize(bytes);
    notes.push("显式视口导出：" + viewFiles[0] + "｜像素 " + (viewSize ? viewSize.width + "×" + viewSize.height : "（不是 PNG）") +
      "｜当时视口 " + viewportNow.w + "×" + viewportNow.h);
  } else {
    notes.push("显式视口导出：**没有文件**");
  }
  viewLog = await evaluate(`(() => { const t = document.getElementById("log").textContent || ""; return (t.split("\\n").filter((l) => l.includes("当前视图") || l.includes("视口")).pop() || "").trim().slice(0, 140); })()`);
  notes.push("显式视口导出日志：" + JSON.stringify(viewLog));
  if (clickedView !== "clicked") failures.push("视口导出按钮点不到 ✗");
  else if (viewFiles.length === 0) failures.push("显式「导出当前视图」没有落下任何文件 ✗");
  else if (!viewSize) failures.push("显式「导出当前视图」的文件不是 PNG ✗");
  else if (viewSize.width !== viewportNow.w || viewSize.height !== viewportNow.h) {
    failures.push("显式「导出当前视图」的 PNG 是 " + viewSize.width + "×" + viewSize.height +
      "，当时视口是 " + viewportNow.w + "×" + viewportNow.h + " ✗");
  } else if (!/当前视图|视口/.test(viewLog)) {
    failures.push("显式视口导出没有在日志里**明确标注**它是当前视图 ⇒ 用户分不清两条路 ✗");
  }
} else {
  failures.push("没有显式的「导出当前视图」入口 #exportPngView ⇒ 视口导出被删掉了，用户再也没法导当前视图 ✗");
}

// ⑥ **离线**：默认导出必须**仍然是文档尺寸**（不是又变回视口裁剪 ✗）。
//
// **什么叫"离线"要说清** ✓（照抄 `browser-offline-assets.mjs` 的教训 ✓，它实测过两次假绿 ✓）：
//   ① `Network.emulateNetworkConditions{offline:true}` **只作用于页面** ✗ ——
//      SW 里的 `fetch` 照样能出去 ✓ ⇒ 单靠它得到的"离线"是**可疑的** ✓。
//   ② 真办法 ✓：**在 SW 上下文里把 `self.fetch` 换成必然失败的桩** ✓ ——
//      那正是"网络没了"时 SW 看到的那件事 ✓。两件一起做 ⇒ 这条离线结论才站得住 ✓。
//   ③ **但断网还不足以证明"走的是本地整幅合成"** ✗ —— 本判据第一次跑就抓到了这个假象 ✓：
//      `render_region` 是**只读工具** ✓ ⇒ `fetchOrLocal` 会把它的 JSON **按 URL+参数缓存** ✓
//      ⇒ 上面第 ④ 段刚在线导过一次**同一个整幅** ✓ ⇒ 断网后这次请求**命中缓存** ✓
//      ⇒ `source` 仍然是 `server` ✓（**尺寸对，但它证明不了本地合成那条路** ✗）。
//   ⇒ 所以这里**换一份从没在线导出过的文档**（`…_off` ✓）⇒ 整幅没有缓存 ✓
//      ⇒ 才真的落到本地合成 ✓，判据也才能断言 `source === "local-full"` ✓。
// **它证明什么** ✓：页面断网 + SW 出不去时，默认导出仍落下一个**文档尺寸**的文件 ✓，
//   且走的确实是本地合成那条 ✓（`source` ✓）、日志明确写着"离线本地合成" ✓（不静默 ✓）。
// **它不证明什么** ✗：① 不证明"像素完整" ✗ —— 离线合成只有视口那块有真实像素 ✓
//（判据只断言尺寸 ✓ 与那条显式说明 ✓）；② 不证明"服务端真的死了" ✗
//（是"这个标签页与它的 SW 出不去"✓）；③ 不证明"缓存命中那条路" ✗ —— 那是另一条路 ✓，
//   它同样会给出文档尺寸 ✓，但不在本段的裁断范围内 ✓。
const DOC_OFF = DOC + "_off";
const createdOff = await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: DOC_OFF, width: W, height: H }),
}).then((r) => r.json());
if (!createdOff || !createdOff.token) {
  console.error("❌ 无法创建离线阶段的文档 ⇒ 判据前置不成立");
  process.exit(1);
}
const postOff = async (tool, args) =>
  (await fetch(`${base}/api/tools?doc=${DOC_OFF}&token=${createdOff.token}`, {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ tool, arguments: args }),
  })).json();
const strokeOff = await postOff("brush_stroke", {
  layer_id: "layer_default", brush: "classic-brush", size: 32,
  color: { r: 10, g: 10, b: 10, a: 255 },
  points: [[80, 120, 0.9], [300, 120, 0.9], [520, 300, 0.9], [900, 900, 0.9]],
});
if (!strokeOff || strokeOff.ok === false) {
  console.error("❌ 离线阶段的文档画不出笔迹 ⇒ 判据前置不成立：" + JSON.stringify(strokeOff).slice(0, 200));
  process.exit(1);
}
await send("Page.navigate", { url: `${base}/?doc=${DOC_OFF}&token=${createdOff.token}` });
if (!(await waitFor(`document.readyState === "complete" && !!document.getElementById("exportPng") && window.yanshiStats.docId === "${DOC_OFF}"`, "离线阶段文档就绪", 40000))) {
  console.error("❌ 离线阶段文档没就绪 ⇒ 判据无效");
  process.exit(3);
}
await waitFor(`window.yanshiStats.wasm === true`, "离线阶段内核就绪", 30000);
// **先在线重载一次** ✗（必须 ✓）：`Network.emulateNetworkConditions{offline}` 对
// **被 SW 接管的页面**是否生效，取决于这个页面**在断网那一刻是否已经由 SW 接管** ✓。
// 实测（`/tmp/probe-offline-cut2.mjs` ✓）：单次导航后断网，POST 仍能出去（status=200 ✗）；
// **重载一次之后再断网，POST 就真的失败** ✓。所以这条重载是"断网是真的"的前提 ✓，不是摆设 ✗。
// （重载会让启动期再写一次整幅渲染缓存 ✓ —— 下面**毒化**它 ✓。）
await send("Page.reload", { ignoreCache: false });
if (!(await waitFor(`document.readyState === "complete" && !!document.getElementById("exportPng") && window.yanshiStats.docId === "${DOC_OFF}"`, "离线阶段重载后就绪", 40000))) {
  console.error("❌ 离线阶段重载后没就绪 ⇒ 判据无效");
  process.exit(3);
}
const controlled = await evaluate(`!!navigator.serviceWorker.controller`);
// **断网前要让"在线那一趟"真的跑完** ✗ —— 画布的服务端补画（`render_region raw` + blob ✓）
// 是异步的 ✓；在它落地**之前**就断网 ⇒ 画布还是内核那份**空白** ✓ ⇒ 本地合成当然是空的 ✓。
// 这是**前置**，不是断言量 ✗（断言的是导出文件的尺寸 ✓ —— 与第 887 轮的教训不冲突 ✓）。
const boardInked = await waitFor(`(() => {
  const b = document.getElementById("board");
  if (!b || !b.width) return false;
  const d = b.getContext("2d").getImageData(0, 0, b.width, b.height).data;
  let ink = 0;
  for (let i = 0; i + 3 < d.length; i += 4) if (d[i] < 200 || d[i + 1] < 200 || d[i + 2] < 200) ink++;
  window.__offlineBoardInk = ink;
  return ink > 500;
})()`, "离线前画布已有墨（在线补画完成）", 20000);
const boardInk = await evaluate(`window.__offlineBoardInk || 0`);
const viewOffline = await evaluate(`(() => { const s = window.yanshi.state(); return { viewport: s.viewport, needsServerPixels: s.needsServerPixels }; })()`);
notes.push("离线前：画布有墨=" + boardInked + "（墨量 " + boardInk + "）｜" + JSON.stringify(viewOffline));
const onlineConsoleCount = consoleLines.length;

const offlineDir = dir;
const swTarget = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json())
  .find((t) => t.type === "service_worker");
let swStubbed = false;
let swSocket = null;
if (swTarget) {
  swSocket = new WebSocket(swTarget.webSocketDebuggerUrl);
  let swId = 1;
  const swPending = new Map();
  swSocket.onmessage = (event) => {
    const m = JSON.parse(event.data);
    if (m.id && swPending.has(m.id)) { swPending.get(m.id)(m); swPending.delete(m.id); }
  };
  await new Promise((open) => { swSocket.onopen = open; });
  const swSend = (method, params) => new Promise((resolve) => {
    const id = swId++; swPending.set(id, resolve); swSocket.send(JSON.stringify({ id, method, params: params || {} }));
  });
  await swSend("Runtime.enable");
  const stubbed = (await swSend("Runtime.evaluate", {
    expression: `(() => { if (!self.__yanshiRealFetch) self.__yanshiRealFetch = self.fetch;
      self.fetch = () => Promise.reject(new TypeError("yanshi-offline-stub")); return typeof self.fetch; })()`,
    returnByValue: true,
  })).result?.result?.value;
  swStubbed = stubbed === "function";
}
if (swSocket) swSocket.close();
notes.push("离线：页面由 SW 接管=" + controlled + "｜页面断网" +
  (swTarget ? (swStubbed ? " + SW 的 self.fetch 已打成必然失败 ✓" : "（SW 桩没装上 ✗）") : "（没有 SW 调试目标 ✗）"));
await send("Network.emulateNetworkConditions", { offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0 });
// **真正的断网 = CDP 的 `Fetch` 请求级拦截** ✓（靠 `offline` 标志位是不够的 ✓ —— 实测：
// 被 SW 接管的页面里 POST 照样能出去 ✓，`/tmp/probe-offline-cut2.mjs` 把这条钉住了 ✓）。
// 页面目标与 SW 目标**都装** ✓（POST 的回退可能算在任一侧 ✓），命中 `/api/` 的一律 `failRequest` ✓。
const installFetchCut = async (target, sendOn) => {
  const handler = async (m) => {
    const url = m.params.request.url || "";
    if (url.includes("/api/")) {
      await sendOn("Fetch.failRequest", { requestId: m.params.requestId, errorReason: "InternetDisconnected" });
    } else {
      await sendOn("Fetch.continueRequest", { requestId: m.params.requestId });
    }
  };
  await sendOn("Fetch.enable", { patterns: [{ urlPattern: "*" }] });
  return handler;
};
fetchCutHandler = await installFetchCut(page, send);
let swCut = null;
if (swTarget) {
  swCut = new WebSocket(swTarget.webSocketDebuggerUrl);
  let swId = 1;
  const swPending = new Map();
  const swSend = (method, params) => new Promise((resolve) => {
    const id = swId++; swPending.set(id, resolve); swCut.send(JSON.stringify({ id, method, params: params || {} }));
  });
  const handler = async (m) => {
    const url = m.params.request.url || "";
    if (url.includes("/api/")) await swSend("Fetch.failRequest", { requestId: m.params.requestId, errorReason: "InternetDisconnected" });
    else await swSend("Fetch.continueRequest", { requestId: m.params.requestId });
  };
  swCut.onmessage = (event) => {
    const m = JSON.parse(event.data);
    if (m.method === "Fetch.requestPaused") { void handler(m); return; }
    if (m.id && swPending.has(m.id)) { swPending.get(m.id)(m); swPending.delete(m.id); }
  };
  await new Promise((open) => { swCut.onopen = open; });
  await swSend("Fetch.enable", { patterns: [{ urlPattern: "*" }] });
}
// **断网本身先验一次** ✓（否则"离线导出成功"可能只是**网络根本没断** ✗ ——
// 本判据第一次跑就差点把这种假绿当结论 ✓）。裸 `fetch`（不经 `fetchOrLocal`）必须失败 ✓。
const offlineProbe = await evaluate(`(async () => {
  try {
    const response = await fetch("/api/tools/render_region?doc=" + window.yanshiStats.docId +
      "&token=" + window.yanshiStats.token, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ region: { x: 0, y: 0, w: ${W}, h: ${H} } }),
    });
    return "status=" + response.status;
  } catch (error) { return "threw:" + error.message; }
})()`);
notes.push("离线基线探针（裸 fetch 整幅 render_region）⇒ " + offlineProbe);
if (!/^threw:/.test(String(offlineProbe))) {
  console.error("❌ 断网没生效（裸 fetch 得到 " + offlineProbe + "）⇒ 这条离线结论不成立，判据自己作废");
  process.exit(1);
}
// **再"毒化"本地 JSON 缓存里的整幅渲染** ✗ —— 页面**启动期**（内核还没就绪时 ✓）会走一次
// `refreshPreview()` 的**服务端整幅渲染** ✓，它的 JSON 被 `fetchOrLocal` 按 URL+参数**缓存** ✓
// ⇒ 不毒化的话，下面的"离线导出"会从缓存里拿到那份整幅 ✓（尺寸对 ✓，但**证明不了本地合成那条路** ✗
// —— 本判据第一次跑就是这样假绿的 ✓）。
// **为什么是"毒化"而不是 `deleteDatabase`** ✗：页面自己持有连接 ⇒ `deleteDatabase` 被 `onblocked` 卡住 ✓
//（实测 cleared=blocked ✓）；直接把 `json` store 里含 `render_region` 的条目写成 `{}` 则**立刻生效** ✓。
const poisoned = await evaluate(`(async () => {
  try {
    const db = await new Promise((resolve, reject) => {
      const request = indexedDB.open("yanshi-local-v1");
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const keys = await new Promise((resolve, reject) => {
      const tx = db.transaction("json", "readonly");
      const request = tx.objectStore("json").getAllKeys();
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => reject(request.error);
    });
    const targets = keys.filter((k) => String(k).includes("render_region"));
    await new Promise((resolve, reject) => {
      const tx = db.transaction("json", "readwrite");
      const store = tx.objectStore("json");
      for (const key of targets) store.put({ key: key, text: "{}", at: Date.now() });
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
    db.close();
    return "poisoned " + targets.length + "/" + keys.length;
  } catch (error) { return "threw:" + error.message; }
})()`);
notes.push("离线：毒化缓存的整幅渲染条目 ⇒ " + poisoned);
fs.rmSync(offlineDir, { recursive: true, force: true });
fs.mkdirSync(offlineDir, { recursive: true });
const clickedOffline = await evaluate(`(() => { const b = document.getElementById("exportPng"); if (!b) return "no-button"; b.click(); return "clicked"; })()`);
const offlineFiles = await waitForFile(offlineDir, 15000);
let offlineSize = null;
let offlineInk = null;
if (offlineFiles.length > 0) {
  const bytes = fs.readFileSync(`${offlineDir}/${offlineFiles[0]}`);
  offlineSize = pngSize(bytes);
  offlineInk = inkCount(await pngDecode(bytes));
  notes.push("离线默认导出：文件 " + offlineFiles[0] + "｜" + bytes.length + " 字节｜像素 " +
    (offlineSize ? offlineSize.width + "×" + offlineSize.height : "（不是 PNG）") + "｜墨量 " + offlineInk);
} else {
  notes.push("离线默认导出：**下载目录里没有任何文件**");
}
const offlineStats = await evaluate(`window.yanshiStats.lastExport || null`);
const offlineLog = await evaluate(`(() => { const t = document.getElementById("log").textContent || ""; return (t.split("\\n").filter((l) => l.includes("已导出 PNG")).pop() || "").trim().slice(0, 140); })()`);
notes.push("离线 lastExport：" + JSON.stringify(offlineStats));
notes.push("离线默认导出日志：" + JSON.stringify(offlineLog));
if (clickedOffline !== "clicked") failures.push("离线时找不到默认导出按钮 ⇒ 判据无效 ✗");
if (offlineFiles.length === 0) failures.push("离线时默认导出**没有落下任何文件** ⇒ 离线导出能力被弄坏了 ✗");
else if (!offlineSize) failures.push("离线默认导出的文件不是 PNG ✗");
else if (offlineSize.width !== W || offlineSize.height !== H) {
  failures.push("离线默认导出的 PNG 是 " + offlineSize.width + "×" + offlineSize.height +
    "，而文档是 " + W + "×" + H + " ⇒ 离线又把默认变回「视口裁剪」✗");
} else if (!/整幅/.test(offlineLog)) {
  failures.push("离线默认导出的日志没有写明它是整幅 ⇒ 边界没说清（不许静默）✗");
} else if (/^poisoned/.test(String(poisoned)) && offlineStats && offlineStats.source !== "local-full") {
  failures.push("缓存已毒化、又断网，却没走本地整幅合成那条（source=" + offlineStats.source + "）⇒ 证据与实现不符 ✗");
}
await send("Network.emulateNetworkConditions", { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
await send("Fetch.disable").catch(() => undefined);
fetchCutHandler = null;
if (swCut) swCut.close();

// **控制台错误只算"在线那一段"** ✓ —— 离线阶段本来就会有一串"连不上服务端"的既有提示 ✓
//（那是产品的**预期行为** ✓，不是缺陷 ✗）⇒ 把它们算成本判据的失败就是把产品正确行为判红 ✗。
// 另外把 `Failed to fetch` 一类筛掉 ✓（它们是**我们自己切网**造成的 ✓，不是产品缺陷 ✓）。
const errors = consoleLines.slice(0, onlineConsoleCount).filter((l) =>
  /error|uncaught|exception|failed/i.test(l) && !/favicon/i.test(l) &&
  !/Failed to fetch|ERR_INTERNET|NetworkError|yanshi-offline-stub/i.test(l));
notes.forEach((n) => console.log("  · " + n));
if (errors.length) notes.push("控制台错误 " + errors.length + " 条");
console.log("  · 控制台错误：" + errors.length);
if (errors.length > 0) failures.push("控制台有错误：" + errors[0].slice(0, 160));
if (failures.length) {
  console.log("  ✗ " + failures.join("；"));
  socket.close();
  process.exit(1);
}
console.log("  ✓ 默认导出 = 整幅文档 " + W + "×" + H + "（缩放 ≠ 适配时也是；离线也是 " +
  (offlineSize ? offlineSize.width + "×" + offlineSize.height : "?") + "）；显式「导出当前视图」= " +
  (viewSize ? viewSize.width + "×" + viewSize.height : "?"));
socket.close();
process.exit(0);
