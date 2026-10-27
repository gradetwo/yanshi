#!/usr/bin/env node
// **判据：区域填充（`fill_region`）必须在查看器里真的能用** ✓（第四轮新增 ✓）。
//
// **抓的是哪一类缺口** ✓：这工具在服务端早就有 ✓ 且在 `core` 缺省面里 ✓
// 而本轮之前 `grep -rn fill_region crates/yanshi-http/src/viewer.rs` 命中 **0** ✗
// ⇒ 工具栏那支「填充图层」走的是 `fill`（整层、无形状、无羽化 ✗）⇒ 人想"填一块带羽化的形状"只能靠 Agent ✗。
//
// **钉住的八件事**（每条都断言**服务端事实** ✗ 不是页面上有几行字 ✓）：
//   ① 卡片在页面上 ✓ 「图层」下拉由 `list_layers` 填 ✓（选项数 == HTTP 读回的层数 ✓）；
//   ② 尺寸非法（宽 0）⇒ **就地拦** ⇒ 不发请求 ⇒ **一个原子都不多** ✓；
//   ③ 锁定层 ⇒ **就地拦**（实测服务端会回 `permission_denied` ✓）⇒ 原子数不变 ✓
//      而隐藏层**照收** ✓（实测 `ok:true` ✓）⇒ 那一种要落笔但必须说清 ⇒ ⑧ 单独判 ✓；
//   ④ `opacity 0.5` + `#000000` ⇒ 对象数 +1 ✓ 卡片回执念的是**服务端回显的 `opacity`** ✓
//      卡片「实测」那行的 RGBA 必须与**另开一条 HTTP `sample_colors`** 逐字节相同 ✓
//      并且蓝通道明显 > 60（纯蓝底上半透明 ⇒ 这就是"参数被静默丢掉"那类缺陷的回归守卫 ✓
//      本轮抓到的正是 `color:"#rrggbb"` 时 `opacity` 整个被丢 ✗ 服务端已修 ✓）；
//   ⑤ `path`（只在内核之外的服务端路径上有 ✓ 注册表那句"只收三种"本轮才更正 ✓）
//      ⇒ 卡片能落 ✓ 服务端对象数 +1 ✓；
//   ⑥ 羽化那一笔 ⇒ **页面画布上的像素**与**服务端整幅折叠读数**必须一致（±6/通道 ✓）
//      ⇒ 这条盯的是本轮修掉的渲染缺陷 ✗：羽化的覆盖率被裁到"本次要写的缓冲" ⇒
//        同一个像素在整幅渲染里 α=255 ✓ 在 1 像素高的区域里 α=194 ✗
//        而**页面的分块缓存与平移缩放干的正是窄区域渲染** ✗ ⇒ 缺陷只在页面上看得见 ✓
//        ⇒ 所以这里必须读页面像素 ✓ 光测服务端等于没测到用户看到的那一层 ✓；
//   ⑦ 「取画面这块」（`texture.type:"region"` ✓ 第 2680 轮实现 ✓ 描述本轮才跟上 ✓）
//      ⇒ 目标处读到的就是源那块的颜色 ✓（服务端事实 ✓ 不是页面自算 ✓）；
//   ⑧ 全程 0 条页面 JS 错误 ✓ 英文模式下这张卡的文案必须是英文 ✓（新加的词条若漏一条 ✗
//      `browser-i18n` 那类"整页扫描"会红 ⇒ 这里就地扫这张卡 ✓）。
//
// **口径**：
//   · 每个场景用**各自的图层** ⇒ 断言互不干扰 ✓；
//   · **先擦这个 profile 的偏好再进文档** ✓：`localStorage` 按源共享 ✗
//     残留 `yanshi.serverRender="0"` 会让判据拿着**本地路由的失败**当产品结论 ✓
//     （上一轮实跑就是这样假失败过一次 ✓）⇒ 这里既擦 ✓ 又**核实**勾选生效 ✓ 擦不动 ⇒ `exit(3)`；
//   · 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)` ✓（AGENTS.md：不许把环境问题判成产品红 ✓）；
//   · 用法错误 ⇒ `process.exit(2)` ✓。
//
// 用法：node scripts/browser-fill-region.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-fill-region.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

// **★ 总超时守卫 ✗ ★**（本仓约定 ✓ 与 `browser-reference-overlay.mjs` 同族 ✓）：本判据要连 CDP 浏览器
//   ⇒ 浏览器没起来时它可能**一直等** ✗（把 CI 的判据分片拖到跑不完 ✓）。外层 `run-criteria.sh`
//   给 browser-* 的是 `timeout ${BROWSER_TIMEOUT:-420}` ✓ ⇒ **内层必须落在它之下** ✓
//   （否则外层先砍 ⇒ 退出码 124 ⇒ **红绿不分** ✗）。超时报「判据自身失败」退 3 ✓ **不冒充产品失败** ✗。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 180000);

const url = process.argv[2];
// **★ 端口必须优先从环境变量取 ✗ ★**（`browser-reference-overlay.mjs:20-25` 记着同一处教训 ✓）：
//   `run-criteria.sh` 给 browser-* 一族传的是 `<viewer-url> <server-base> <token> <cdpPort>` ✓
//   ⇒ **argv[3] 是一个 URL** ✗ ⇒ 拿它当端口会拼出 `http://127.0.0.1:http://127.0.0.1:13990/json/list` ✗
//   ⇒ CI 里**恒红**，而本机手工传端口号时**完全看不出来** ✓（本轮自查发现 ✓）。
//   本机手工传数字仍然认 ✓。
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
if (!url) {
  console.error("用法: node scripts/browser-fill-region.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const base = new URL(url).origin;
const shotsDir = process.env.SHOTS_DIR || "/tmp/yanshi-fill-region-shots";
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// ---------- 一次性文档 + 夹具 ----------
const docId = "t_fillregion_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 240, height: 180,
                         background: { r: 0, g: 0, b: 255, a: 255 } }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const baseLayer = ((await callTool("list_layers", {})).layers || [])[0]?.layer_id;
if (!baseLayer) { console.error("❌ 判据前提失败：拿不到初始图层"); process.exit(3); }
for (const name of ["work", "locked", "hidden", "src"]) {
  const made = await callTool("create_layer", { layer_id: "layer_" + name, name: name });
  if (!made.ok) { console.error("❌ 判据前提失败：建场景图层 " + name + " ⇒ " + JSON.stringify(made).slice(0, 160)); process.exit(3); }
}
// ⑦ 的源块：在 `layer_src` 上铺一块纯绿（(0,0) 起 40×40 ✓）⇒ 之后卡片"取画面这块"应当取到绿 ✓。
const srcPaint = await callTool("fill", { layer_id: "layer_src", data: { color: [0, 200, 0], region: { x: 0, y: 0, w: 40, h: 40 } } });
if (!srcPaint.ok) { console.error("❌ 判据前提失败：铺源块 ⇒ " + JSON.stringify(srcPaint).slice(0, 160)); process.exit(3); }
const lockIt = await callTool("update_layer", { layer_id: "layer_locked", patch: { locked: true } });
if (!lockIt.ok) { console.error("❌ 判据前提失败：锁层 ⇒ " + JSON.stringify(lockIt).slice(0, 160)); process.exit(3); }
const hideIt = await callTool("update_layer", { layer_id: "layer_hidden", patch: { visible: false } });
if (!hideIt.ok) { console.error("❌ 判据前提失败：藏层 ⇒ " + JSON.stringify(hideIt).slice(0, 160)); process.exit(3); }
// **前提：服务端对锁定层真的会拒** ✓ 否则 ③ 测的就不是那条路径 ✓（实测 `permission_denied` ✓）。
const refused = await callTool("fill_region", { layer_id: "layer_locked",
  shape: { type: "rect", x: 1, y: 1, w: 4, h: 4 }, color: { r: 0, g: 0, b: 0 }, opacity: 1 });
if (refused.ok || refused.error_code !== "permission_denied") {
  console.error("❌ 判据前提失败：锁定层这次没被拒 ⇒ " + JSON.stringify(refused).slice(0, 200)); process.exit(3);
}
/** `get_log` 的 `count` 是"本次返回了几行" ✓ ⇒ **不给 `limit`** ✓（给了就恒真 ✗ 见 542 轮那条 ✓）。 */
const atomCount = async () => Number((await callTool("get_log", {})).count || 0);
const objectsOf = async (layerId) => {
  const found = (await callTool("list_layers", {})).layers.find((l) => l.layer_id === layerId);
  // **别把 `|| 0` 写进 `Number()`** ✓：`objects` 合法取值就是 0 ✓（判据自己踩过的写法 ✗）。
  return found ? Number(found.objects) : NaN;
};
const layerCount = async () => (await callTool("list_layers", {})).layers.length;
if (await layerCount() !== 5) { console.error("❌ 判据前提失败：夹具层数不是 5"); process.exit(3); }
const atomsStart = await atomCount();
if (!(atomsStart > 0)) { console.error("❌ 判据前提失败：读不到原子计数"); process.exit(3); }

// ---------- CDP ----------
let targets;
try {
  targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
} catch (error) {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 连不上 CDP：" + error.message + " ⇒ 退出码 3");
  process.exit(3);
}
const page = targets.find((t) => t.type === "page");
if (!page) { console.error("没有页面目标 ⇒ 判据自身前提不成立（不是产品问题）"); process.exit(3); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
let msgId = 0;
const pending = new Map();
const eventWaiters = [];
const pageErrors = [];
ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  for (let i = eventWaiters.length - 1; i >= 0; i -= 1) {
    if (eventWaiters[i].method === msg.method) { eventWaiters[i].resolve(msg); eventWaiters.splice(i, 1); }
  }
  if (msg.method === "Runtime.exceptionThrown") {
    pageErrors.push(String(msg.params.exceptionDetails?.exception?.description || msg.params.exceptionDetails?.text || "unknown").slice(0, 160));
  }
  if (msg.method === "Log.entryAdded" && msg.params.entry?.level === "error" && msg.params.entry?.source === "javascript") {
    pageErrors.push(String(msg.params.entry.text).slice(0, 160));
  }
});
await new Promise((res, rej) => { ws.addEventListener("open", res); ws.addEventListener("error", rej); });
const send = (method, params = {}) => new Promise((res) => {
  const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params }));
});
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）
 *  而读数走 `res.result.result.value` ✗ 不是 `res.result.value` ✓。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
const capture = async (name) => {
  // **截图前把这张卡滚到眼前** ✓（判据自己说"用真实截图检查"✗ 结果截的是面板第一屏 ✓
  //   等于没看 ✓）：右侧 `aside` 是可滚的 ✓ 卡片在「绘制」tab 里靠下 ✓
  //   ⇒ 不 `scrollIntoView` 就永远拍不到它 ✓。
  // **滚到这张卡并**确认到位**（重试到布局稳定 ✓）**：页面在开机／`resync` 之后会重排右侧面板 ✓
  //   一次 `scrollTop` 会被抹掉 ✗（实测：英文模式那张截图拍到的是「对象」卡 ✓）。
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const landed = await evaluate(`(() => {
      const card = document.getElementById("cardFillRegion");
      if (!card) return false;
      const pane = card.closest("aside") || card.parentElement;
      const cr = card.getBoundingClientRect(), pr = pane.getBoundingClientRect();
      if (cr.top >= pr.top - 2 && cr.top < pr.bottom) return true;
      pane.scrollTop += (cr.top - pr.top) - 8;
      return false;
    })()`);
    if (landed) break;
    await new Promise((res) => setTimeout(res, 150));
  }
  await new Promise((res) => setTimeout(res, 200));
  const shot = await send("Page.captureScreenshot", { format: "png" });
  const data = shot.result?.data;
  if (!data) return null;
  const fs = await import("node:fs/promises");
  await fs.mkdir(shotsDir, { recursive: true });
  const file = `${shotsDir}/${name}.png`;
  await fs.writeFile(file, Buffer.from(data, "base64"));
  return file;
};

await send("Runtime.enable");
await send("Page.enable");
// **把窗口撑到常见桌面尺寸** ✓（默认 800×600 时右侧面板只有 ~84px 高 ✓ 卡片根本拍不全 ✓
//   ⇒ "真实截图检查"会拍成一排按钮 ✓ 看不出界面好坏 ✓）。
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
await send("Log.enable");
// **擦偏好** ✓ 再进被测文档 ✓（见抬头「口径」里那条实测教训 ✓）。
// **★ 但擦 `localStorage` 不够 ✗ ⇒ 必须显式钉住 `render=server` ★**（本轮 CI 接线自查发现 ✓）：
//   `run-criteria.sh` 给 browser-* 一族的 viewer URL **统一加 `&render=client`** ✓
//   而 `viewer-app.js:83-88` 里 **URL 参数优先级最高**（在 `localStorage` 之上 ✓）
//   ⇒ 擦干净存储也照样被 URL 按回本地内核 ✗ ⇒ 本判据所有工具调用改走 `api-local.js` ✓
//   **测的就不是服务端回执那一半了** ✓（本机手工跑不带这个参数 ⇒ 看不出来 ✗ ⇒ CI 里恒红 ✓）。
//   ⇒ 本判据**自己**在导航 URL 上写死 `render=server` ✓（与传入的参数无关 ✓ 两种跑法同一条结论 ✓）。
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(700);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
const bootedWait = async () => {
  for (let i = 0; i < 80; i += 1) {
    const ready = await evaluate(`!!(window.yanshi && document.getElementById("fillRegionLayer")?.options.length >= 5)`);
    if (ready) return true;
    await sleep(250);
  }
  return false;
};
if (!(await bootedWait())) {
  console.error("❌ 判据前提失败：开机后「图层」下拉没装着 5 个图层（1 初始 + 4 场景）⇒ 页面脚本或 `list_layers` 没就绪");
  process.exit(3);
}
// **★ 有界等待这条前提 ✗ ★**（登记 ⑰ 同族 ✓）：`Page.loadEventFired` 可能属于**上一次**导航 ✓
//   ⇒ 导航后**单次**读勾选会读到**旧页面**的状态 ✗（实测同一条判据一次绿、一次假 `exit 3` ✓）。
//   **∴ 等前提、不等结论** ✓：轮询到「勾选 = true」**且当前 URL 确实带着 `render=server`** 才认 ✓。
let serverRenderOn = null;
for (let i = 0; i < 80; i += 1) {
  serverRenderOn = await evaluate(`(() => {
    if (!location.search.includes("render=server")) return "WRONG-PAGE";
    const b = document.getElementById("useServerRender");
    return b ? (b.checked === true ? true : false) : null;
  })()`);
  if (serverRenderOn === true) break;
  await sleep(250);
}
if (serverRenderOn !== true) {
  console.error("❌ 判据前提失败：「服务器渲染」那个勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）⇒ 后面读到的都是本地路由，不是产品结论");
  process.exit(3);
}

/** 往卡片的输入框里写值 ✓ 并按真实事件走（`change` 触发 `renderFillRegionRows` ✓）。 */
const field = (id, value) => evaluate(`(() => {
  const el = document.getElementById(${JSON.stringify(id)});
  if (!el) return "NO-EL";
  el.value = ${JSON.stringify(String(value))};
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return el.value;
})()`);
const pickField = (id, value) => evaluate(`(() => {
  const el = document.getElementById(${JSON.stringify(id)});
  if (!el) return "NO-EL";
  el.value = ${JSON.stringify(String(value))};
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return el.value;
})()`);
const clickCard = (id) => evaluate(`(() => {
  const el = document.getElementById(${JSON.stringify(id)});
  if (!el) return "NO-EL";
  el.click();
  return 1;
})()`);
const cardRead = () => evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  const t = (id) => (q(id) ? q(id).textContent : null);
  return JSON.stringify({
    info: t("fillRegionInfo"), sample: t("fillRegionSample"), selection: t("fillRegionSelection"),
    layer: q("fillRegionLayer") ? q("fillRegionLayer").options.length : -1,
    rows: ["fillRegionRectRow", "fillRegionEllipseRow", "fillRegionPolygonRow", "fillRegionPathRow", "fillRegionSrcRow"]
      .map((id) => (q(id) ? (q(id).style.display === "none" ? 0 : 1) : -1)),
  });
})()`).then((s) => JSON.parse(s));
/** **画布像素**（页面自己画出来的那个数 ✓）：按 `state().viewport` 与 `zoom` 把文档坐标映到画布 ✓。
 *  画布后备像素宽 /（视口宽 × 缩放）这个因子**实测出来** ✗ 不猜（可能是 DPR ✓ 也可能不是 ✓）。 */
const canvasPixel = (docX, docY) => evaluate(`(() => {
  const nodes = Array.from(document.querySelectorAll("img, canvas")).filter((n) => {
    const b = n.getBoundingClientRect(); return b.width >= 64 && b.height >= 64;
  });
  const board = nodes.find((n) => n.id === "board") || nodes[0];
  if (!board) return JSON.stringify({ error: "没有画板" });
  const st = window.yanshi.state();
  const vp = st.viewport, zoom = st.zoom || 1;
  if (!vp || !(vp.w > 0)) return JSON.stringify({ error: "读不到视口" });
  const off = document.createElement("canvas");
  off.width = board.width; off.height = board.height;
  const ctx = off.getContext("2d", { willReadFrequently: true });
  try { ctx.drawImage(board, 0, 0); } catch (e) { return JSON.stringify({ error: "drawImage 失败" }); }
  const factor = board.width / (vp.w * zoom);
  const px = Math.round(((${docX}) - vp.x) * zoom * factor);
  const py = Math.round(((${docY}) - vp.y) * zoom * factor);
  if (!(px >= 0 && py >= 0 && px < off.width && py < off.height)) {
    return JSON.stringify({ error: "点不在画布上", px, py, width: off.width, height: off.height, factor, vp, zoom });
  }
  const d = ctx.getImageData(px, py, 1, 1).data;
  return JSON.stringify({ rgba: [d[0], d[1], d[2], d[3]], px, py, factor, zoom });
})()`).then((s) => JSON.parse(s));

// ---------- ① 卡片与下拉 ----------
{
  const read = await cardRead();
  const httpLayers = await layerCount();
  if (read.layer === httpLayers) ok(`① 卡片在页面上 · 「图层」装了 ${read.layer} 项 == 服务端 ${httpLayers} 层`);
  else bad(`① 「图层」下拉的项数与服务端层数不一致`, { card: read.layer, http: httpLayers });
  // **只显示当前形状那一行** ✓：默认矩形 ⇒ 第一行可见、其余藏起 ✓ 看不见的框不该拦住落笔 ✓。
  if (read.rows.slice(0, 4).join(",") === "1,0,0,0") ok(`① 形状行切换正确 ${JSON.stringify(read.rows)}`);
  else bad(`① 形状行切换不对（矩形时应当只有第一行可见）`, { rows: read.rows });
}

// ---------- ② 尺寸非法 ⇒ 就地拦 ----------
{
  const atomsBefore = await atomCount();
  const objectsBefore = await objectsOf("layer_work");
  await pickField("fillRegionLayer", "layer_work");
  await pickField("fillRegionShape", "rect");
  await field("fillRegionW", 0);
  await clickCard("fillRegionRun");
  // **等前提、不等结论**（2026-10-27 CI 假红 ⇒ 本机 `info=""` 定性的复制品）：
  // 这句「宽/高要大于 0」是**页面自己**在点击处理器里同步写的 ⇒ 读到空只可能是
  // ①慢机上还没渲染，或②`setupFillRegionPanel` 挂在 `warmKernel` 里、监听器尚未接上 ✗
  // ⇒ **有界重点击 + 轮询**（每轮 250 ms、最多 30 s）。重点击安全：宽 0 这一路
  // **不发任何请求**（这正是本条断的内容），多点几次不会写原子。
  let info = "";
  for (let i = 0; i < 120; i += 1) {
    info = (await cardRead()).info || "";
    if (/大于 0/.test(info)) break;
    await sleep(250);
    if (i % 4 === 3) await clickCard("fillRegionRun");
  }
  const atomsAfter = await atomCount();
  const objectsAfter = await objectsOf("layer_work");
  if (atomsAfter === atomsBefore && objectsAfter === objectsBefore && /大于 0/.test(info)) {
    ok(`② 宽 0 ⇒ 就地拦（原子 ${atomsBefore}→${atomsAfter} · 对象 ${objectsBefore}→${objectsAfter}）`, { info });
  } else {
    bad(`② 宽 0 应当不发请求、不写任何东西`, { info, atoms: [atomsBefore, atomsAfter], objects: [objectsBefore, objectsAfter] });
  }
  await field("fillRegionW", 120);
}

// ---------- ③ 锁定层 ⇒ 就地拦 ----------
{
  const atomsBefore = await atomCount();
  await pickField("fillRegionLayer", "layer_locked");
  await clickCard("fillRegionRun");
  await sleep(400);
  const read = await cardRead();
  const atomsAfter = await atomCount();
  if (atomsAfter === atomsBefore && /锁定/.test(read.info || "")) {
    ok(`③ 锁定层 ⇒ 就地拦并说清（原子 ${atomsBefore} 不变）`, { info: read.info });
  } else {
    bad(`③ 锁定层不该发出请求（服务端会回 permission_denied ✓ 这里要就地拦住 ✓）`, { info: read.info, atoms: [atomsBefore, atomsAfter] });
  }
}

// ---------- ④ 半透明填充 ⇒ 回执 + 实测与服务端同源 ----------
{
  const atomsBefore = await atomCount();
  const objectsBefore = await objectsOf("layer_work");
  await pickField("fillRegionLayer", "layer_work");
  await pickField("fillRegionShape", "rect");
  await field("fillRegionX", 20); await field("fillRegionY", 20);
  await field("fillRegionW", 60); await field("fillRegionH", 40);
  await field("fillRegionColor", "#000000");
  await field("fillRegionOpacity", 0.5);
  await field("fillRegionFeather", 0);
  await pickField("fillRegionMode", "color");
  await clickCard("fillRegionRun");
  await sleep(900);
  const read = await cardRead();
  const objectsAfter = await objectsOf("layer_work");
  const serverSample = await callTool("sample_colors", { points: [[50, 40]] });
  const srv = (serverSample.samples || [])[0]?.rgba;
  const cardNumbers = /实测 \((\d+),(\d+)\) = \[(\d+),(\d+),(\d+),(\d+)\]/.exec(read.sample || "");
  const cardRgba = cardNumbers ? [Number(cardNumbers[3]), Number(cardNumbers[4]), Number(cardNumbers[5]), Number(cardNumbers[6])] : null;
  if (objectsAfter !== objectsBefore + 1) {
    bad(`④ 落笔后服务端对象数应当 +1`, { before: objectsBefore, after: objectsAfter, info: read.info });
  } else if (!/opacity=0\.5/.test(read.info || "")) {
    bad(`④ 卡片必须念出服务端回显的生效 opacity（本轮抓到字符串颜色时它被整个丢掉 ✗）`, { info: read.info });
  } else if (!cardRgba || !Array.isArray(srv)) {
    bad(`④ 卡片上没有「实测 (…) = […]」那行，或服务端读数取不到 ⇒ 无法核对同源性`, { sample: read.sample, srv });
  } else if (cardRgba.join(",") !== srv.join(",")) {
    bad(`④ 卡片念的像素必须与服务端 「sample_colors」读到的像素逐字节相同（界面不许自算 ✗）`, { card: cardRgba, server: srv });
  } else if (!(cardRgba[2] > 60)) {
    bad(`④ opacity 0.5 在纯蓝底上必须看得出半透明（蓝通道 ${cardRgba[2]} ⇒ 参数像被静默忽略 ✗）`, { card: cardRgba });
  } else {
    ok(`④ 半透明填充：对象 +1 · 回执 opacity=0.5 · 实测 ${JSON.stringify(cardRgba)} 与服务端一致`, { info: read.info });
  }
  if (await atomCount() <= atomsBefore) bad(`④ 落笔应当留下一条原子`);
  else ok(`④ 原子数 ${atomsBefore} → ${await atomCount()}`);
  await capture("zh-04-fill-opacity");
}

// ---------- ⑤ path 形状 ----------
{
  const objectsBefore = await objectsOf("layer_work");
  await pickField("fillRegionShape", "path");
  const rowsAfterSwitch = (await cardRead()).rows;
  await field("fillRegionD", "M 100 100 L 150 100 L 150 140 Z");
  await field("fillRegionOpacity", 1);
  await clickCard("fillRegionRun");
  await sleep(900);
  const read = await cardRead();
  const objectsAfter = await objectsOf("layer_work");
  if (rowsAfterSwitch.slice(0, 4).join(",") !== "0,0,0,1") {
    bad(`⑤ 切到路径时应当只显示 d 那一行`, { rows: rowsAfterSwitch });
  } else if (objectsAfter !== objectsBefore + 1 || !/形状 path/.test(read.info || "")) {
    bad(`⑤ 路径填充没走通（服务端第 1558 轮就收 path ✓ 注册表那句"只收三种"本轮已更正 ✓）`,
        { before: objectsBefore, after: objectsAfter, info: read.info });
  } else {
    ok(`⑤ path 能从卡片落笔：对象 +1 · 回执形状 path`, { info: read.info });
  }
  await pickField("fillRegionShape", "rect");
}

// ---------- ⑥ 羽化那一笔：页面像素 == 服务端读数 ----------
{
  await field("fillRegionX", 170); await field("fillRegionY", 20);
  await field("fillRegionW", 60); await field("fillRegionH", 40);
  await field("fillRegionColor", "#000000");
  await field("fillRegionOpacity", 1);
  await field("fillRegionFeather", 10);
  await clickCard("fillRegionRun");
  await sleep(1000);
  const read = await cardRead();
  const centreNumbers = /实测 \((\d+),(\d+)\) = \[(\d+),(\d+),(\d+),(\d+)\]/.exec(read.sample || "");
  const cx = centreNumbers ? Number(centreNumbers[1]) : 200;
  const cy = centreNumbers ? Number(centreNumbers[2]) : 40;
  const srv = (await callTool("sample_colors", { points: [[cx, cy]] }).then((r) => (r.samples || [])[0]?.rgba));
  const shot = await canvasPixel(cx, cy);
  if (!srv) bad(`⑥ 服务端读数取不到 ⇒ 无法比对`, { sample: read.sample });
  else if (shot.error) bad(`⑥ 读不到画布像素 ⇒ 这条断言没跑成（判据前提问题也算红，因为画布是本判据要验的东西）`, shot);
  else {
    const drift = [0, 1, 2].map((i) => Math.abs(shot.rgba[i] - srv[i]));
    const worst = Math.max(...drift);
    // 缺陷复现时：窄区域渲染把 α 从 255 打到 194 ⇒ 纯蓝底上的黑块蓝通道差 **61** ✓
    // ⇒ 6/通道的容差既能放过抗锯齿 ✓ 又一定抓得住那一级 ✗。
    if (worst <= 6) ok(`⑥ 羽化那一笔：画布 ${JSON.stringify(shot.rgba)} == 服务端 ${JSON.stringify(srv)}（最大单通道差 ${worst}）`, { px: [shot.px, shot.py] });
    else bad(`⑥ 页面画出来的与服务端整幅读数不一致（羽化的覆盖率被裁到窄区域就是这一类 ✗ 本轮已修 ✓）`,
             { canvas: shot.rgba, server: srv, drift, map: { factor: shot.factor, zoom: shot.zoom } });
  }
  await field("fillRegionFeather", 0);
  await capture("zh-06-feather-canvas");
}

// ---------- ⑦ 取画面这块（region 纹理）----------
{
  const objectsBefore = await objectsOf("layer_work");
  await pickField("fillRegionMode", "region");
  const rowsWithSrc = (await cardRead()).rows;
  await field("fillRegionSrcX", 0); await field("fillRegionSrcY", 0);
  await field("fillRegionSrcW", 40); await field("fillRegionSrcH", 40);
  await field("fillRegionX", 180); await field("fillRegionY", 120);
  await field("fillRegionW", 40); await field("fillRegionH", 30);
  await clickCard("fillRegionRun");
  await sleep(1000);
  const read = await cardRead();
  const objectsAfter = await objectsOf("layer_work");
  const srv = (await callTool("sample_colors", { points: [[200, 135]] }).then((r) => (r.samples || [])[0]?.rgba));
  if (rowsWithSrc[4] !== 1) bad(`⑦ 切到「取画面这块」时源矩形那一行应当显示出来`, { rows: rowsWithSrc });
  else if (objectsAfter !== objectsBefore + 1) bad(`⑦ 纹理填充没落笔`, { before: objectsBefore, after: objectsAfter, info: read.info });
  else if (!/平铺/.test(read.info || "") || !/存成位图/.test(read.info || "")) {
    bad(`⑦ 卡片必须念服务端的纹理回执（from_region ✓）`, { info: read.info, sample: read.sample });
  } else if (!srv || !(srv[1] > 120 && srv[0] < 90 && srv[2] < 90)) {
    bad(`⑦ 目标处读到的应当是源那块的颜色（绿 [0,200,0]）`, { server: srv, sample: read.sample });
  } else {
    ok(`⑦ 「取画面这块」纹理从卡片可用：对象 +1 · 回执念了平铺块数 · 目标实测 ${JSON.stringify(srv)}`, { info: read.info });
  }
  await pickField("fillRegionMode", "color");
  await capture("zh-07-region-texture");
}

// ---------- ⑧ 隐藏层的说法 + 页面错误 + 英文模式 ----------
{
  const objectsBefore = await objectsOf("layer_hidden");
  await pickField("fillRegionLayer", "layer_hidden");
  await field("fillRegionX", 90); await field("fillRegionY", 150);
  await field("fillRegionW", 30); await field("fillRegionH", 20);
  await clickCard("fillRegionRun");
  await sleep(1000);
  const read = await cardRead();
  const objectsAfter = await objectsOf("layer_hidden");
  // 隐藏层照收 ✓（实测 ✓）⇒ 必须**落笔 + 说清"画上看不到"** ✓ 两者都要 ✓。
  if (objectsAfter === objectsBefore + 1 && /隐藏/.test(read.info || "")) ok(`⑧ 隐藏层：落了笔（对象 +1）并说清算不上画面 ✓`, { info: read.info });
  else bad(`⑧ 隐藏层要落笔并且把"画上看不到"讲清（服务端对隐藏层是照收的 ✓ 实测 ✓）`,
           { before: objectsBefore, after: objectsAfter, info: read.info });
}
if (pageErrors.length === 0) ok(`⑧ 全程 0 条页面 JS 错误`);
else bad(`⑧ 页面抛了 ${pageErrors.length} 条 JS 错误`, pageErrors.slice(0, 4));

{
  await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=en&v=${Date.now()}` });
  await waitEvent("Page.loadEventFired");
  let en = null;
  // **★ 先等下拉真的装着东西 ✗ ★**（本轮自查 ✓ 登记 ⑲）：原来这段"卡片一出现就读" ✗
  //   ⇒ 图层下拉**还没被 `list_layers` 填上** ✓ ⇒ **所有 `<option>` 文案压根不在扫描范围里** ✗
  //   （实测：截图上明明白白写着「图层 1 (#layer_default)」✓ 判据却报"0 条中文残留 · 扫了 58 条"✓
  //     ⇒ **假绿** ✓ —— 扫描漏看了整类节点 ✓ 与"断言永不失败"同族 ✗ 但方向相反：不是等结论 ✓
  //     是**没等到数据就下结论** ✓）。
  for (let i = 0; i < 80; i += 1) {
    en = await evaluate(`(() => {
      const card = document.getElementById("cardFillRegion");
      if (!card) return null;
      const count = (id) => { const el = document.getElementById(id); return el && el.options ? el.options.length : 0; };
      // **只有图层下拉是动态的** ✓（形状／填什么／填充规则是写死的 4/3/2 项 ⇒ 拿总数当"填好了"会**永远成立** ✗）
      const layerOptions = count("fillRegionLayer");
      const options = layerOptions + count("fillRegionShape") + count("fillRegionMode") + count("fillRegionFillRule");
      const parts = [];      // **界面文案**（chrome）⇒ 必须全英文 ✓
      const dataParts = [];  // **服务端数据**（图层名等）⇒ 不属翻译面 ✓ 但**要印出来** ✗ 不许悄悄跳过 ✓
      const walk = (node, inOption) => {
        for (const child of node.childNodes) {
          if (child.nodeType === 3) { const t = child.nodeValue.trim(); if (t) (inOption ? dataParts : parts).push(t); continue; }
          if (child.nodeType !== 1) continue;
          const tag = child.tagName.toLowerCase();
          if (["script","style","code","pre","textarea"].includes(tag)) continue;
          if (child.dataset && child.dataset.i18n === "off") continue;
          for (const attr of ["title","placeholder","aria-label"]) {
            const v = child.getAttribute(attr);
            if (v) parts.push("ATTR:" + v);
          }
          walk(child, inOption || tag === "option");
        }
      };
      walk(card, false);
      const cjk = (list) => list.filter((t) => /[\\u3040-\\u30ff\\u4e00-\\u9fff]/.test(t));
      return JSON.stringify({ total: parts.length, options, layerOptions, cjk: cjk(parts), dataCjk: cjk(dataParts), sample: parts.slice(0, 6) });
    })()`).then((s) => (s ? JSON.parse(s) : null));
    if (en && en.layerOptions >= 5) break;
    await sleep(250);
  }
  if (!en || en.layerOptions < 5) {
    console.error("❌ 判据前提失败：英文模式下图层下拉没装着 5 个层（实为 " + JSON.stringify(en && en.layerOptions) + "）⇒ 扫不到 `<option>` 就等于没扫");
    process.exit(3);
  }
  if (en.cjk.length === 0) ok(`⑧ 英文模式下这张卡 0 条中文残留（扫了 ${en.total} 条界面文本与属性 ＋ ${en.options} 个下拉项 ✓）`, en.sample);
  else bad(`⑧ 英文模式下这张卡漏出中文 ⇒ I18N 词条缺了`, en.cjk.slice(0, 6));
  // 图层名是**文档数据**（服务端建的 `图层 1` ✓ 用户自己也能起中文名）⇒ 不算残留 ✓
  //   但**列出来**：将来若有人把"界面扫描"扩到数据面 ✗ 这里就是第一处会吵的地方 ✓。
  console.log(`  （同一次扫描里，下拉中有 ${en.dataCjk.length} 项是**服务端数据**（图层名）⇒ 不翻 ✓ 例：${JSON.stringify(en.dataCjk.slice(0, 3))}）`);
  await capture("en-08-card-english");
}

ws.close();
console.log("\n  截图目录：" + shotsDir);
if (problems.length > 0) {
  console.log(`  ✗ ${problems.length} 处不符（区域填充的 Web 入口）`);
  process.exit(1);
}
console.log("  ✓ 区域填充在查看器里真的能用（回执同源 · 画布与服务端一致 · 纹理可用 · 双语齐）");
process.exit(0);
