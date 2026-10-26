#!/usr/bin/env node
// **判据：开机就要把「对象」「批注」两个面板装出来** ✓（第 190 轮 ✓）。
//
// **抓的是什么 bug** ✓：带 token 打开一个**有内容**的文档 ⇒
// 「对象」面板 0 行 ✓ 并显示「当前图层还没有对象 ✓」**这句假话** ✗
//（实测：`list_objects` 回 2 个对象 ✓ 而面板 0 行 ✗）。
// 根因：`refreshObjects` / `refreshAnnotations` 只挂在 `switchDocument()` 的初始化链里 ✓，
// 而**开机的两条路**（`loadDocumentData` 与带 token 的那一段 ✗）**都不走那里** ✓
// ⇒ 只有手动点各面板的「刷新」才看得见 ✗ —— 挂在面板上的入口（重排、整组撤销、实例…）
// 在人看来就像"全是坏的" ✓。
//
// **为什么做成独立判据** ✓：它不在任何现有判据的覆盖面里 ✗
//（415 个脚本里**没有一个**引用 `#objectList` ✓ ⇒ 这类"面板空了但没人报警"的回归无人守 ✓）。
//
// 口径：
//   · **面板行数 == 服务端事实** ✓（比"看起来有东西"严 ✗）；
//   · 批注面板同理 ✓（同一个根因 ✗ 一起守 ✓）；
//   · **不许出现与事实相反的空态文案** ✓（"还没有对象"而服务端有 ⇒ 红 ✗）；
//   · 页面抛错一律算红 ✓。
//
// 用法：node scripts/browser-boot-panels.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-boot-panels.mjs http://127.0.0.1:15900/?doc=boot 9490
import { setTimeout as sleep } from "node:timers/promises";

const url = process.argv[2];
// **CDP 端口来源**（第 1132 轮整合）：`run-criteria.sh` 的 browser 分支传的是
// `<viewer-url> <base> <token> <cdpPort>`（argv[3] 是 **base URL**，不是端口 ✗）——
// 本判据首跑就把它当端口 ⇒ `http://127.0.0.1:http://127.0.0.1:13990/json/version` ✗。
// ⇒ 优先读导出的 `CDP_PORT`（run-criteria 顶部 export ✓），其次只接受**纯数字**的 argv[3]。
const rawPort = process.env.CDP_PORT || process.argv[3] || "";
const port = /^\d+$/.test(rawPort) ? rawPort : "9222";
const base = new URL(url).origin;
if (!url) {
  console.error("用法: node scripts/browser-boot-panels.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// 自己开一个**一次性文档** ✓：不依赖 `boot` 夹具里恰好有没有对象 ✓（夹具会变 ✗ 判据不能跟着漂 ✓）。
const docId = "t_bootpanels_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 160, height: 120 }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());

const layerId = ((await callTool("list_layers", {})).layers || [])[0].layer_id;
if (!layerId) { console.error("❌ 判据前提失败：拿不到图层"); process.exit(3); }
// 三样东西各来一点 ✓：**对象**（两笔 ✓）**批注**（两枚 ✓）。
const strokes = [];
strokes.push(await callTool("brush_stroke", { layer_id: layerId, brush: "100%_Opaque.myb", points: [[20, 30], [120, 30]], size: 14, color: "#ff0000" }));
strokes.push(await callTool("brush_stroke", { layer_id: layerId, brush: "100%_Opaque.myb", points: [[20, 80], [120, 80]], size: 14, color: "#0000ff" }));
if (!strokes.every((s) => s.ok)) { console.error("❌ 判据前提失败：落笔 ⇒ " + JSON.stringify(strokes).slice(0, 200)); process.exit(3); }
for (const [i, y] of [40, 90].entries()) {
  const made = await callTool("create_annotation", {
    type: "region", intent: "modify",
    target: { target: "region", bbox: { x: 30, y, w: 16, h: 16 } }, content: "判据批注 " + i,
  });
  if (!made.ok) { console.error("❌ 判据前提失败：建批注 ⇒ " + JSON.stringify(made).slice(0, 200)); process.exit(3); }
}
const serverObjects = ((await callTool("list_objects", { include_hidden: true })).objects || []).length;
const serverAnnotations = ((await callTool("list_annotations", {})).annotations || []).length;
if (serverObjects < 2 || serverAnnotations < 2) { console.error("❌ 判据前提失败：服务端读数太小", { serverObjects, serverAnnotations }); process.exit(3); }

const version = String(opened.version || "");
const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const wsB = new WebSocket(browser.webSocketDebuggerUrl);
await new Promise((res, rej) => { wsB.onopen = res; wsB.onerror = rej; });
let msgId = 0;
const pending = new Map();
const eventWaiters = [];
const pageErrors = [];
wsB.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.method === "Runtime.exceptionThrown") {
    const d = msg.params.exceptionDetails;
    pageErrors.push(((d.exception && (d.exception.className + ": " + d.exception.description)) || d.text || "unknown").split("\n")[0]);
  }
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  for (const w of eventWaiters) if (w.method === msg.method) w.resolve(msg.params);
};
const sendB = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); wsB.send(JSON.stringify({ id, method, params })); });
const created = await sendB("Target.createTarget", { url: "about:blank" });
const attached = await sendB("Target.attachToTarget", { targetId: created.result.targetId, flatten: true });
const sessionId = attached.result.sessionId;
const send = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); wsB.send(JSON.stringify({ id, method, params, sessionId })); });
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
for (const m of ["Page.enable", "Runtime.enable"]) await send(m);
const evalJs = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true, timeout: 60000 });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails).slice(0, 300));
  return r.result && r.result.result ? r.result.result.value : undefined;
};

const readPanels = `(() => {
  const rows = (sel) => document.querySelectorAll(sel + " .annotation-row").length;
  const objList = document.getElementById("objectList");
  return {
    objects: rows("#objectList"),
    annotations: rows("#annotationList"),
    objectEmptyHint: objList ? (objList.querySelector(".hint") ? objList.querySelector(".hint").textContent : "") : null,
    marks: (window.__appMarks || []).filter((m) => /^boot:(before-)?refresh(Objects|Annotations)/.test(m)),
  };
})()`;

// **只等页面自己把面板装出来** ✓：探针不去点"刷新"✗（点了就测不到"开机没装"这个 bug 了 ✓）。
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}` + (version ? `&v=${version}` : "") });
await waitEvent("Page.loadEventFired");
let seen = null;
for (let i = 0; i < 40; i += 1) {
  seen = JSON.parse(await evalJs(`JSON.stringify(${readPanels})`));
  if (seen.objects >= serverObjects && seen.annotations >= serverAnnotations) break;
  await sleep(500);
}
console.log("  服务端事实：对象 " + serverObjects + " ｜批注 " + serverAnnotations);
console.log("  开机后面板：", JSON.stringify(seen));

if (seen.objects !== serverObjects) bad("「对象」面板行数 ≠ 服务端对象数（开机没装出来）", { panel: seen.objects, server: serverObjects });
else ok("「对象」面板开机就有全部对象（行数 == 服务端事实）", { rows: seen.objects });
if (seen.annotations !== serverAnnotations) bad("「批注」面板行数 ≠ 服务端批注数（开机没装出来）", { panel: seen.annotations, server: serverAnnotations });
else ok("「批注」面板开机就有全部批注（行数 == 服务端事实）", { rows: seen.annotations });
// **空态文案必须与事实一致** ✓：这是用户实际读到的那句话 ✗ 说反了比"空白"更坏 ✓。
if (seen.objects > 0 && /还没有对象/.test(seen.objectEmptyHint || "")) bad("面板有对象却写着「还没有对象」（与事实相反的空态文案）", seen.objectEmptyHint);
else ok("空态文案与实际状态一致", { hint: seen.objectEmptyHint || "" });
if (!seen.marks.length) bad("boot 里没有 refreshObjects / refreshAnnotations 的留痕（修法被回退）", seen.marks);
else ok("boot 两步都留了痕（可定位 ✓ 与离线那套同口径 ✓）", { marks: seen.marks });
if (pageErrors.length) bad("页面抛错", pageErrors.slice(0, 3));

console.log(problems.length ? "\n❌ 判据失败：" + problems.join("；") : "\n✅ 判据通过：开机即装「对象」「批注」面板，且与服务端事实一致");
wsB.close();
process.exit(problems.length ? 1 : 0);
