#!/usr/bin/env node
// **判据：运行时建的 tooltip 也必须是英文**（第六轮新增 ✓ 补 `ui-i18n-coverage` 的盲区 ✓）。
//
// **抓的是哪一类缺陷**（本轮实测出来的 ✗ 不是读代码猜的 ✓）：
//   `ui-i18n-coverage` 自己就在结论里写明「**不看** viewer-app.js」✓ ⇒ 它只查**模板**里的文案 ✓
//   而界面上一大批 tooltip 是 **JS 运行时**建出来的 ✓（图层行的 👁／🔒／混合模式下拉 ✓
//   批注输入框的 placeholder ✓ 纹理缩略图 ✓ 调色板色块 ✓ 离线队列那条带数字的说明 ✓）
//   ⇒ 静态判据恒绿 ✗ 而英文界面里鼠标一停就冒中文 ✓（真机截图确认过：整页英文、tooltip 中文 ✓）。
//
// **为什么不复用静态清点** ✗：静态 grep 只能查「源码里出现的中文串有没有词条」✓
//   但**词条命中与否**取决于运行时拼出来的**整句**长什么样 ✓（「数据 + 后缀」这种精确键永远配不上 ✓
//   必须走句式）⇒ 只有**把真的机制跑起来**才判得准 ✓。于是这里分两条腿：
//     · **真界面**：图层行、批注输入框这些**用户真的会看到**的 tooltip ✓ 直接读属性 ✓；
//     · **机制探针**：把运行时句式做成临时元素塞进 `body` ✓ 让**真的** `MutationObserver` 走一遍翻译 ✓
//       ⇒ 这样即便当前部署没有纹理／调色板资产 ✓ 那几条句式也**被测到了** ✓（不是"没渲染就跳过"的假绿 ✓）。
//
// **钉住的六件事**（每条都断言**屏幕事实** ✗ 不是「词典里有这行」✓）：
//   ① 前提：`lang=en` 真的生效（`documentElement.lang === "en"` ✓ 顶栏读得出英文 ✓）否则 `exit(3)`；
//   ② 图层面板每一行的三件控件 tooltip（👁／🔒／混合模式）都是英文 ✓ 0 汉字 ✓ 且**语义对得上**
//      （锁着的行说 "Lock…"／解锁的行说 "Unlock…" ✗ 不许两句互换 ✓）；
//   ③ 批注输入框的 placeholder 是英文 ✓（同一条盲区里的另一族：`placeholder` 也是属性 ✓）；
//   ④ **机制探针**：那几条「数据 + 后缀」的句式经真的翻译机制之后 ✓ 框架变英文 ✓ **数据原样保留** ✓
//      （图层名／资产名／色号是服务端数据 ✗ 不翻 ✓ 与 `browser-fill-region` 同一条口径 ✓）；
//   ⑤ 全页清扫：所有 `title`／`placeholder`／`aria-label`／`alt` ⇒ 去掉**已知服务端数据**之后
//      **一个汉字都不该剩** ✓（违规的连元素选择器一起打出来 ✓ 便于定位 ✓）；
//   ⑥ 全程 0 条页面 JS 错误 ✓（改 `i18nAttrs` 不许把渲染路径打断 ✗）。
//
// **口径**：
//   · 先擦 `localStorage`（`yanshi.lang` ✗ `yanshi.serverRender`）再进文档 ✓ 并**核实**勾选了「服务器渲染」✓
//     （`run-criteria.sh` 会给 browser-* 统一加 `&render=client` ✓ 而 URL 参数优先级最高 ✓
//      ⇒ 本判据**自己**在导航 URL 上钉 `render=server` ✓ 两种跑法同一条结论 ✓）；
//   · 夹具的图层名／批注内容一律 **ASCII** ✓ ⇒ 清扫里读到汉字就只能是**界面文案** ✗ 不是我的数据 ✓；
//   · 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)`（AGENTS.md：不许把环境问题判成产品红 ✓）；
//     用法错误 ⇒ `process.exit(2)` ✓；
//   · **清扫的自检**：整个页面读到的 tooltip 少于 20 条 ⇒ 说明面板根本没渲染 ✓ 判据自己失效 ⇒ `exit(3)` ✗
//     （否则「0 个违规」是**空判** ✗ 与本轮 ⑨ 修掉的那条假绿同一个病 ✓）。
//
// 用法：node scripts/browser-tooltip-i18n.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-tooltip-i18n.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 180000);

const url = process.argv[2];
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
if (!url) {
  console.error("用法: node scripts/browser-tooltip-i18n.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const base = new URL(url).origin;
const shotsDir = process.env.SHOTS_DIR || "/tmp/yanshi-tooltip-i18n-shots";
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// ---------- 夹具：一份 ASCII 命名的文档 + 两图层（一锁一不锁）+ 一条批注 ----------
const docId = "t_tooltip_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 320, height: 240, background: { r: 255, g: 255, b: 255, a: 255 } }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json()).catch((error) => ({ ok: false, threw: String(error).slice(0, 120) }));

const LOCKED = "layer_alpha";
const FREE = "layer_beta";
for (const id of [LOCKED, FREE]) {
  const made = await callTool("create_layer", { layer_id: id, name: id === LOCKED ? "row-locked" : "row-free" });
  if (!made.ok) { console.error("❌ 判据前提失败：建图层 " + id + " ⇒ " + JSON.stringify(made).slice(0, 160)); process.exit(3); }
}
const lockedSet = await callTool("update_layer", { layer_id: LOCKED, patch: { locked: true } });
if (!lockedSet.ok) { console.error("❌ 判据前提失败：把 " + LOCKED + " 锁上 ⇒ " + JSON.stringify(lockedSet).slice(0, 160)); process.exit(3); }
const annotated = await callTool("create_annotation",
  { content: "note-alpha", type: "text", intent: "modify", target: { bbox: { x: 20, y: 20, w: 120, h: 40 } } });
if (!annotated.ok) { console.error("❌ 判据前提失败：建批注 ⇒ " + JSON.stringify(annotated).slice(0, 200)); process.exit(3); }
// **清扫用的「服务端数据」清单** ✓：图层名／批注内容都是我自己起的 ASCII 名 ✓
//   再带上部署里真的资产名 ✓（资产名可以是中文 ✗ 那是数据不是界面文案 ✓）。
const dataNames = ["row-locked", "row-free", "note-alpha"];
// **清单里的名字也是服务端数据** ✓（资产名／色板色名可以由用户起中文 ✗ 不算界面漏翻 ✓）：
//   `list_assets` 要 `kind` ✓ `list_palette_colors` 要 `palette` ✓ ⇒ 取不到就**只是少几条数据名** ✓
//   不影响断言成立 ⇒ 这里容错 ✓ 但**容错不等于静默** ✓ 取不到会照原样打进证据里 ✓。
for (const [tool, args, key] of [
  ["list_assets", { kind: "texture" }, "assets"],
  ["list_palette_colors", { palette: "open-color.json" }, "colors"],
]) {
  const listed = await callTool(tool, args);
  const rows = Array.isArray(listed[key]) ? listed[key] : (Array.isArray(listed.items) ? listed.items : []);
  for (const row of rows) {
    const name = typeof row === "string" ? row : (row && (row.name || row.title)) || "";
    if (name) dataNames.push(String(name));
  }
  if (!Array.isArray(listed[key]) && !Array.isArray(listed.items)) {
    console.log("  · " + tool + " 没给清单（" + JSON.stringify(listed).slice(0, 90) + "）⇒ 数据名少几条 ✓ 不影响判定");
  }
}

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
  // **截图前把图层面板滚到眼前** ✓（判据自己说"用真实截图检查"✗）：右侧 `aside` 可滚 ✓
  //   图层卡靠下 ✓ ⇒ 不 `scrollIntoView` 就永远拍不到 tooltip 所属那几行 ✓。
  for (let attempt = 0; attempt < 8; attempt += 1) {
    const landed = await evaluate(`(() => {
      const row = document.querySelector(".layer-row");
      if (!row) return false;
      const pane = row.closest("aside") || row.parentElement;
      const rr = row.getBoundingClientRect(), pr = pane.getBoundingClientRect();
      if (rr.top >= pr.top - 2 && rr.top < pr.bottom) return true;
      pane.scrollTop += (rr.top - pr.top) - 8;
      return false;
    })()`);
    if (landed) break;
    await sleep(150);
  }
  await sleep(200);
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
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
await send("Log.enable");
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(700);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=en&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let booted = false;
for (let i = 0; i < 80 && !booted; i += 1) {
  booted = await evaluate(`!!(window.yanshi && document.getElementById("layerList")?.children.length >= 2)`);
  if (!booted) await sleep(250);
}
if (!booted) { console.error("❌ 判据前提失败：开机后图层面板没装着 2 条场景图层 ⇒ 页面脚本或 list_layers 没就绪"); process.exit(3); }
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
  console.error("❌ 判据前提失败：「服务器渲染」勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）⇒ 后面读的不是这条路由");
  process.exit(3);
}
// ① 英文模式真的生效（**等的是独立就绪信号** ✓「有没有英文」仍由下面的断言判 ✗ 第 887 轮那条教训 ✓）
await sleep(600);
const langNow = await evaluate(`document.documentElement.lang`);
if (langNow !== "en") bad(`① 带着 lang=en 进来了 ✓ 页面却说 html lang = ${JSON.stringify(langNow)} ⇒ 双语开关本身没生效`);
else ok("① 英文模式生效（html lang=en ✓ 后面所有断言都在这前提下读 ✓）");

// ---------- ② 图层行那三件控件 ----------
const READ_ROWS = `(() => {
  const out = [];
  for (const row of document.querySelectorAll("#layerList .layer-row")) {
    const eye = row.querySelector('button[data-action="visible"]');
    const lock = row.querySelector('button[data-action="locked"]');
    const blend = row.querySelector("select") || row.querySelector("[data-blend]");
    out.push({
      layerId: row.dataset.layerId || null,
      eye: eye ? eye.title : null,
      lock: lock ? lock.title : null,
      blend: blend ? blend.title : null,
      blendTag: blend ? blend.tagName.toLowerCase() : null,
    });
  }
  return out;
})()`;
let rows = await evaluate(READ_ROWS);
if (!Array.isArray(rows) || rows.length < 2) {
  console.error("❌ 判据前提失败：图层面板读不到 2 行（实为 " + JSON.stringify(rows) + "）⇒ 后面全是空判");
  process.exit(3);
}
const cjk = (s) => /[一-鿿]/.test(String(s || ""));
const rowOf = (id) => rows.find((r) => r.layerId === id);
const lockedRow = rowOf(LOCKED), freeRow = rowOf(FREE);
if (!lockedRow || !freeRow) {
  console.error("❌ 判据前提失败：读不到夹具那两行（拿到的是 " + JSON.stringify(rows.map((r) => r.layerId)) + "）");
  process.exit(3);
}
{
  const missing = [];
  for (const [tag, row, expect] of [["锁着的行", lockedRow], ["没锁的行", freeRow]]) {
    for (const key of ["eye", "lock", "blend"]) {
      if (row[key] === null || row[key] === undefined || row[key] === "") missing.push(tag + " 的 " + key + " 没有 tooltip");
      else if (cjk(row[key])) missing.push(tag + " 的 " + key + " 还是中文：" + row[key]);
    }
  }
  if (missing.length) bad("② 图层行的 tooltip 有中文或缺失（运行时建的属性没进翻译）", missing);
  else ok("② 图层行三件控件的 tooltip 都是英文（👁／🔒／混合模式 ✓ 0 汉字 ✓）",
    [lockedRow.eye, lockedRow.lock, lockedRow.blend]);
  // **语义不许互换** ✓：锁着的行该说"解锁" ✓ 没锁的行该说"锁定" ✗ 说反了比没翻更坏 ✓
  const flipped = !/Unlock/i.test(lockedRow.lock || "") || !/Lock this layer/i.test(freeRow.lock || "") ||
    !/Hide/i.test(lockedRow.eye || "") || !/Blend mode/i.test(lockedRow.blend || "");
  if (flipped) bad("② tooltip 翻是翻了 ✓ 意思却对不上（锁着的该说 Unlock… ✓ 隐藏的该说 Hide… ✓）",
    { 锁着: [lockedRow.eye, lockedRow.lock, lockedRow.blend], 没锁: [freeRow.eye, freeRow.lock, freeRow.blend] });
  else ok("② tooltip 的语义对得上：锁着的行说 Unlock… ✓ 显示着的行说 Hide… ✓ 下拉说 Blend mode ✓");
}
await capture("01-en-layer-tooltips");

// ---------- ②b 工具栏那 21 颗按钮（同一族的第二个形状：「标签 (快捷键)」） ----------
{
  const strip = await evaluate(`(() => {
    return Array.from(document.querySelectorAll("#tools button")).map((b) => ({
      tool: b.dataset.tool || b.id || null,
      title: b.title || null,
      aria: b.getAttribute("aria-label"),
    })).filter((row) => row.title || row.aria);
  })()`);
  if (!Array.isArray(strip) || strip.length < 10) {
    console.error("❌ 判据前提失败：工具栏只数到 " + (Array.isArray(strip) ? strip.length : 0) + " 颗按钮 ⇒ 这一族没渲染出来 ✓ 测不到");
    process.exit(3);
  }
  const zh = strip.filter((row) => cjk(row.title) || cjk(row.aria));
  if (zh.length) bad("②b 工具栏的 tooltip／aria-label 还是中文（「标签 (快捷键)」这种拼法精确键配不上 ⇒ 整个工具栏全漏 ✗）",
    zh.slice(0, 6));
  else ok("②b 工具栏 " + strip.length + " 颗按钮的 tooltip 与 aria-label 都是英文 ✓",
    strip.slice(0, 3).map((r) => r.title));
  // **快捷键不许被翻译吃掉** ✓：括号里那个字母是键盘提示 ✗ 它是用户认笔的唯一线索 ✓
  const lostKey = strip.filter((row) => {
    const wanted = /\(([A-Z])\)$/.exec(row.title || "");
    return wanted && !new RegExp("\\(" + wanted[1] + "\\)$").test(row.aria || "");
  });
  if (lostKey.length) bad("②b 有的按钮 tooltip 里带着快捷键 ✓ aria-label 却丢了 ⇒ 读屏听不到按键线索", lostKey.slice(0, 4));
  else ok("②b 快捷键在 tooltip 与 aria-label 里都保住了 ✓（翻译只动标签 ✗ 不动键盘提示 ✓）",
    strip[0] && strip[0].title);
}

// ---------- ③ 批注输入框的 placeholder ----------
{
  // **有界等批注行出现** ✓（批注表是异步拉回来的 ✓ 直接读会间歇性读不到 ✗ 第 895 轮那条教训 ✓）：
  //   等的是**元素存在**这个独立信号 ✓「placeholder 是不是英文」仍由断言判 ✗（第 887 轮 ✓）。
  let ph = { found: false };
  for (let i = 0; i < 32; i += 1) {
    ph = await evaluate(`(() => {
      const box = document.querySelector("#annotationList input[type=text]");
      return box ? { found: true, placeholder: box.placeholder || null } : { found: false };
    })()`);
    if (ph.found) break;
    await sleep(250);
  }
  if (!ph.found) { console.log("  ⚠ 批注面板里没有可读的输入框 ⇒ ③ 这一条没跑到（**跳过不等于通过** ✓）"); }
  else if (cjk(ph.placeholder)) bad("③ 批注输入框的 placeholder 还是中文 ⇒ placeholder 也是属性 ✓ 同一个盲区", ph);
  else ok("③ 输入框 placeholder 是英文（占位符同属属性那一族 ✓）", ph.placeholder);
}

// ---------- ④ 机制探针：把运行时句式喂给真的翻译机制 ----------
{
  const phrases = [
    "parrot.png（点击选中 ✓）",
    "parrot.png ⇒ 缩略图没发出来（`/textures/` 路由或资产目录有问题 ✓）",
    "#ff8800（点击取色 ✓）",
    "loaded-knife 预览",
    "离线时改动先写进浏览器本地队列，联网后按顺序补交；服务端上的这份文档若已变化则暂停补交（上限 200 条 / 8 MB）",
    "离线时改动先写进浏览器本地队列，联网后按顺序补交；服务端上的这份文档若已变化则暂停补交（上限 200 条 / 8 MB）。其他文档还有 5 条未补交",
    "点一下：编辑这个调整 ✓",
    "写点什么…",
    "本部署没有服务端 ⇒ 只能用本地内核；此开关不可用",
  ];
  const results = await evaluate(`(async () => {
    const made = [];
    for (const text of ${JSON.stringify(phrases)}) {
      const el = document.createElement("button");
      el.type = "button";
      el.dataset.probe = "1";
      el.title = text;
      el.textContent = "probe";
      document.body.appendChild(el);
      made.push(el);
    }
    // 让真的 MutationObserver 跑一遍（它把整页重走 i18nWalk ⇒ 属性也翻 ✓）
    await new Promise((res) => setTimeout(res, 600));
    return made.map((el) => ({ zh: el.dataset.original || null, now: el.getAttribute("title") }));
  })()`);
  if (!Array.isArray(results) || results.length !== phrases.length) {
    console.error("❌ 判据前提失败：探针元素没建成（实为 " + JSON.stringify(results).slice(0, 160) + "）⇒ 测不到机制");
    process.exit(3);
  }
  const stillChinese = [];
  const lostData = [];
  for (let i = 0; i < phrases.length; i += 1) {
    const now = String(results[i].now || "");
    if (now === phrases[i] || cjk(now.replace(/^(parrot\.png|loaded-knife|#ff8800)/, ""))) {
      stillChinese.push({ 原句: phrases[i].slice(0, 40), 现在: now.slice(0, 70) });
    }
    const dataHead = /^(parrot\.png|loaded-knife|#ff8800)/.exec(phrases[i]);
    if (dataHead && !now.startsWith(dataHead[1])) lostData.push({ 该保留: dataHead[1], 现在: now.slice(0, 40) });
  }
  if (stillChinese.length) bad("④ 句式没被翻译机制接住（框架仍含汉字 ⇒ 词典／句式表漏了或写错了）", stillChinese);
  else ok("④ 「数据 + 后缀」那几条句式经真的机制后框架全英文 ✓（含带数字的那条离线队列说明 ✓）",
    results.map((r) => String(r.now).slice(0, 52)));
  if (lostData.length) bad("④ 翻译把**服务端数据**弄丢了（资产名／色号／预设名必须原样保留 ✗ 那是用户认图的依据）", lostData);
  else ok("④ 数据段原样保留 ✓（资产名／色号／预设名没被吃掉 ✓）");
  await evaluate(`(() => { for (const el of document.querySelectorAll("[data-probe='1']")) el.remove(); return 1; })()`);
}

// ---------- ⑤ 全页清扫 ----------
{
  const sweep = await evaluate(`(() => {
    const attrs = ["title", "placeholder", "aria-label", "alt"];
    const skipSel = "#log, pre, code, script, style, textarea, [data-i18n='off']";
    const out = [];
    let total = 0;
    // **「这串是用户数据」由产品自己声明** ✓：界面上凡是标了 data-i18n='off' 的那一片 ✓
    //   就是「图层名／资产名／批注内容」这类**服务端数据**（与 browser-fill-region 同一条口径 ✓）。
    //   ⇒ 从**当前页面**把它们收下来当排除表 ✗ 而不是判据里硬写一份名单（硬写会随资产变动失效 ✓）
    //   ⚠️ 本块整段在**模板字面量**里 ⇒ 注释里**绝不能有反引号** ✗（会终止模板 ⇒ 语法错 ✓
    //      本仓写过多次这条教训 ✓ 本轮又踩一次 ✓ 由 node --check 抓回 ✓）
    const declared = new Set();
    const fileName = (src) => {
      const tail = decodeURIComponent(String(src || "").split("/").pop() || "").replace(/\\.[a-z0-9]+$/i, "");
      return tail && tail !== "data" ? tail : null;
    };
    for (const el of document.querySelectorAll("[data-i18n='off']")) {
      const text = (el.textContent || "").trim();
      if (text) declared.add(text);
      const alt = el.getAttribute("alt");
      if (alt && alt.trim()) declared.add(alt.trim());
      const name = fileName(el.getAttribute("src"));
      if (name) declared.add(name);
    }
    // **图片文件名算数据** ✓（资产名／样图名本来就是文件 ✓ 用户起的中文 ✗ 不翻 ✓）；
    //   但**只取文件名** ✗ 不取它的 alt —— 否则界面自己写的中文 alt（如「缩略图」）会被当成数据放过 ✓
    //   那正是本轮要抓的东西之一 ✗ 口径一分错就前功尽弃 ✓。
    for (const img of document.querySelectorAll("img")) {
      const name = fileName(img.getAttribute("src"));
      if (name) declared.add(name);
    }
    for (const el of document.querySelectorAll("*")) {
      if (el.closest && el.closest(skipSel)) continue;
      if (el.dataset && el.dataset.probe === "1") continue;
      for (const name of attrs) {
        const value = el.getAttribute(name);
        if (value === null || value.trim() === "") continue;
        total += 1;
        if (!/[\\u4e00-\\u9fff]/.test(value)) continue;
        const where = el.tagName.toLowerCase() + (el.id ? "#" + el.id : "") +
          (el.className && typeof el.className === "string" ? "." + el.className.split(" ")[0] : "");
        out.push({ where, name, value: value.slice(0, 90) });
      }
    }
    return { total, hits: out, declared: Array.from(declared).slice(0, 60) };
  })()`);
  if (!sweep || typeof sweep.total !== "number") {
    console.error("❌ 判据前提失败：清扫读不到任何 tooltip 属性 ⇒ 判据自身坏了");
    process.exit(3);
  }
  if (sweep.total < 20) {
    console.error("❌ 判据前提失败：整页只数到 " + sweep.total + " 条 tooltip 属性 ⇒ 面板根本没渲染 ✓ 「0 违规」是空判");
    process.exit(3);
  }
  // **服务端数据不算漏翻** ✓（图层名／资产名／批注内容是用户数据 ✗ 与 browser-fill-region 同一条口径 ✓）
  const allData = dataNames.concat(sweep.declared || []);
  const leaks = sweep.hits.filter((h) => !allData.some((name) => name && h.value.includes(name)));
  ok("⑤ 全页 tooltip 清扫：数到 " + sweep.total + " 条属性 ✓ 含汉字 " + sweep.hits.length +
    " 条 ✓（其中 " + (sweep.hits.length - leaks.length) + " 条含**产品自己标过的用户数据** ⇒ 不算漏翻 ✓）");
  if (leaks.length) bad("⑤ 这些 tooltip 在英文界面里说中文（**运行时建的属性**没词条 ⇒ 静态判据看不见这条路 ✗）", leaks);
  else ok("⑤ 去掉服务端数据之后 ✓ 整页 tooltip 一个汉字都不剩 ✓");
  await capture("02-en-after-probe");
}

// ---------- ⑥ JS 错误 ----------
if (pageErrors.length) bad("⑥ 这一路跑出了页面 JS 错误 ⇒ 改翻译机制把渲染路径打断了 ✗", pageErrors.slice(0, 3));
else ok("⑥ 全程 0 条页面 JS 错误 ✓");

socketClose();
function socketClose() { try { ws.close(); } catch (_) { /* 收尾失败不影响结论 ✓ */ } }
console.log("\n  截图目录：" + shotsDir);
if (problems.length) {
  console.log("  ✗ " + problems.length + " 处不符（运行时 tooltip 的双语）");
  process.exit(1);
}
console.log("  ✓ 运行时建的 tooltip 也说英文 · 意思对得上 · 数据原样保留 · 整页清扫干净");
process.exit(0);
