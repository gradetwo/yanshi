#!/usr/bin/env node
// **判据：范围遮罩（`create_range_mask`）与渐变蒙版（`create_mask` 的两种渐变 kind）
// 必须在查看器里真的能用** ✓。
//
// **抓的是什么** ✓：这两个能力早已在服务端 ✓ 且在 MCP 缺省面里 ✓，
// 但页面上**零入口** ✗ ⇒ 人只能靠 Agent 才做得到 ✓（对照表里那一类"系统已在、人却够不到"✓）。
// 补入口只是第一步 ✗ **这一类工具最容易骗人的地方是"回 `ok: true` 却什么都没挑中"** ✓：
// 实测带内一个像素都没有时 ⇒ 烤出一张全零蒙版 ✓ 而回执仍是成功 ✓ ⇒
// 挂上去的观感是"参数改了、画面纹丝不动"✓ 日志里只留下一句成功 ✗
//（**静默无操作** ✓ 本项目反复出现的那一类 ✓）。
// 于是本判据钉住九件事 ✓：
//   ① 两张卡 + 两支渐变笔真的在页面上 ✓ 两个图层框由 `list_layers` 填充 ✓；
//   ② 覆盖 > 0 ⇒ **挂载到"挂到"那一层** ✓ 卡片写出的是**服务端给的覆盖数**（不是界面自己数的 ✓）；
//   ③ 覆盖 = 0 ⇒ **拒绝挂载** ✓ 目标层的 `mask_id` 必须仍然没有 ✓ 并给出下一步 ✓；
//   ④ 色相带同理 ✓（挑中红条 ✓）；
//   ⑤ 两支渐变笔真拖得动 ✓ 且**羽化对渐变无效必须出声** ✓（服务端对非零羽化直接报错 ✓
//      照抄上去的结局就是"拖了一下没反应"✓）；
//   ⑥ 非法区间（下限 > 上限）就地拒绝 ⇒ 不产生废原子 ✓；
//   ⑦ **取下蒙版**：断得开 ✓ 现状行跟着服务端走 ✓ 本来就没有时不写空原子 ✓；
//   ⑧ 全程 0 条页面 JS 错误 ✓；
//   ⑨ **勾掉「服务器渲染」时**（`api-local.js` 没有这个端点 ✓ 请求根本没出门 ✗）
//      卡片必须说清是**本地内核缺端点**并给出下一步 ✓ 不许假归因给服务端 ✓。
//
// **口径**：
//   · 断言的是**服务端事实**（`list_layers` 里那一层的 `mask_id` ✓）而不是页面上的字 ✓；
//   · **每个场景用一个新建的图层** ✓ ⇒ 各断言互不干扰（一层只有一个 `mask_id` ✓ 后挂的会盖掉先挂的 ✓
//     取下用 ⑦ 那条路径 ✓ 但复用同一层仍会让两条断言共享同一份现状 ✓）；
//   · 拖动走**真实指针事件**（`Input.dispatchMouseEvent` ✓）⇒ 屏幕位置按 `window.yanshi.state()`
//     的 `viewport` 现算 ✓ 不写死"页面上第几个像素"这种会漂的常数 ✓；
//   · 页面抛 JS 错误 ⇒ 红 ✓；前提取不到 ⇒ **判据自己失效** ⇒ `exit(3)` ✓。
//
// 用法：node scripts/browser-range-mask.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-range-mask.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

const url = process.argv[2];
// **CDP 端口来源**（第 1132 轮整合）：`run-criteria.sh` 的 browser 分支传的是
// `<viewer-url> <base> <token> <cdpPort>`（argv[3] 是 **base URL**，不是端口 ✗）——
// 同 `browser-boot-panels` 的坑。⇒ 优先读导出的 `CDP_PORT`，argv[3] 只接受纯数字。
const rawPort = process.env.CDP_PORT || process.argv[3] || "";
const port = /^\d+$/.test(rawPort) ? rawPort : "9222";
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) {
  console.error("用法: node scripts/browser-range-mask.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// ---------- 一次性文档 ✓（夹具会变 ✗ 判据不能跟着漂 ✓）----------
const docId = "t_range_" + Date.now().toString(36).slice(-6);
const opened = await fetch(`${base}/api/documents`, {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 240, height: 180 }),
}).then((r) => r.json());
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => fetch(
  `${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`,
  { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}) },
).then((r) => r.json());
const sourceLayer = ((await callTool("list_layers", {})).layers || [])[0]?.layer_id;
if (!sourceLayer) { console.error("❌ 判据前提失败：拿不到采样图层"); process.exit(3); }
// 三段竖条 ⇒ 亮度带与色相带都能挑出**非零**覆盖 ✓：白（luma 1 ✓）/ 红（luma ≈0.21 ✓ hue 0 ✓）/ 近黑 ✓。
// 白条 = 80×180 = **14400 像素** ✓ 判据就按这个数断言 ✓（同源：服务端回执 ✓ 不是界面自己数的 ✓）。
const bars = [];
for (const [x, color] of [[0, "#ffffff"], [80, "#ff0000"], [160, "#101010"]]) {
  bars.push(await callTool("fill", { layer_id: sourceLayer, data: { color, region: { x, y: 0, w: 80, h: 180 } } }));
}
if (!bars.every((b) => b.ok)) { console.error("❌ 判据前提失败：铺底色 ⇒ " + JSON.stringify(bars).slice(0, 200)); process.exit(3); }
// 每个场景一个层 ✓（见上面「口径」里那条"一层只有一个 mask_id"的理由 ✗）。
const scenarioLayers = {};
for (const name of ["lum", "zero", "hue", "linear", "radial"]) {
  const made = await callTool("create_layer", { layer_id: "layer_" + name, name: "场景 " + name });
  if (!made.ok) { console.error("❌ 判据前提失败：建场景图层 " + name + " ⇒ " + JSON.stringify(made).slice(0, 200)); process.exit(3); }
  scenarioLayers[name] = "layer_" + name;
}
const maskOf = async (layerId) => (await callTool("list_layers", {})).layers
  .find((layer) => layer.layer_id === layerId)?.mask_id || null;
if (await maskOf(scenarioLayers.lum)) {
  console.error("❌ 判据前提失败：新建图层一上来就带着蒙版");
  process.exit(3);
}

// ---------- CDP ----------
const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
if (!browser.webSocketDebuggerUrl) {
  console.error("❌ 判据前提失败：CDP 在 " + port + " 上没给出 webSocketDebuggerUrl（浏览器没起来？）");
  process.exit(3);
}
const ws = new WebSocket(browser.webSocketDebuggerUrl);
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
const sendB = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params })); });
const created = await sendB("Target.createTarget", { url: "about:blank" });
const attached = await sendB("Target.attachToTarget", { targetId: created.result.targetId, flatten: true });
const sessionId = attached.result.sessionId;
const send = (method, params = {}) => new Promise((res) => { const id = ++msgId; pending.set(id, res); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const waitEvent = (method, timeout = 30000) => new Promise((res, rej) => {
  eventWaiters.push({ method, resolve: res });
  setTimeout(() => rej(new Error("event timeout " + method)), timeout);
});
await send("Runtime.enable");
await send("Page.enable");
await send("Log.enable");
// **先把这个浏览器 profile 里的偏好擦干净** ✓（第 542 轮 ✓）：`localStorage` 是**按源共享**的 ✗
// ⇒ 上一个脚本（或人自己）在这个 profile 里勾掉过「服务器渲染」⇒ 请求改走 `api-local.js` ✓
// 而它**没有** `/api/tools/create_range_mask` ✓ ⇒ 判据会拿着**本地路由的失败**当产品结论 ✗
//（实测就是这样假红过 ✓）。渲染模式属于**被测环境的前提**⇒ 这里显式擦回缺省（服务器渲染 ✓）✓
// 并在下面**核实它真的生效**（擦不动 ⇒ `exit(3)` ✓ 不硬判 ✓）。
await send("Page.navigate", { url: `${base}/?v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
await sleep(800);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
// **把界面钉死在中文** ✓：`?lang=zh` 的优先级高于 `localStorage` ✓（`readUILang()` 先读 URL ✓）。
// 不钉 ✗ 的话同一份判据会因为**上一个脚本在这个浏览器 profile 里留下的语言**而一会儿红一会儿绿 ✓
//（实测就是这样：残留 EN ⇒ 卡片回执是英文 ⇒ 判据按中文整句匹配不到 ✗ 报出**假红**✓）。
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  // `Runtime.evaluate` 的响应体在 `res.result` 里 ⇒ `exceptionDetails` / `result.value` **都在那一层** ✓
  //（读错一层 = 表达式抛了也看不出来 ⇒ 判据会拿着 `undefined` 当结论 ✗）。
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
// **等到面板装出来** ✓（开机路径也要填这两个图层框 ✗ 没等到就拿"还没渲染"当结论是判据自己的毛病 ✓）。
let booted = false;
for (let i = 0; i < 60; i += 1) {
  booted = await evaluate(`!!(window.yanshi && document.getElementById("rangeMaskSource")?.options.length >= 6)`);
  if (booted) break;
  await sleep(250);
}
if (!booted) {
  console.error("❌ 判据前提失败：开机后「采样 / 挂到」两个图层框没装着 6 个图层（六个 = 1 个初始层 + 5 个场景层）");
  process.exit(3);
}
// **界面确实是中文** ✓（见上面 `?lang=zh` 那一段 ✓）—— 否则下面按中文整句的断言全是**假红**✓。
if ((await evaluate(`document.documentElement.lang`)) !== "zh-CN") {
  console.error("❌ 判据前提失败：界面没落在中文（实为 " + JSON.stringify(await evaluate(`document.documentElement.lang`)) + "）");
  process.exit(3);
}
// **「服务器渲染」确实生效** ✓（见上面擦 `localStorage` 那一段 ✓）：勾掉它 ⇒ `create_range_mask`
// 走不到服务端 ✓（`api-local.js` 无此端点 ⇒ `endpoint_not_local`✓）⇒ 本判据测的就不再是那条路径 ✓。
const serverRenderOn = await evaluate(`(() => { const b = document.getElementById("useServerRender"); return b ? b.checked === true : null; })()`);
if (serverRenderOn !== true) {
  console.error("❌ 判据前提失败：「服务器渲染」那个勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）");
  process.exit(3);
}

// ---------- ① 卡片、工具、行的显隐 ----------
const shell = await evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  const opts = (id) => [...(q(id)?.options || [])].map((o) => o.value);
  return {
    cards: [!!q("cardRangeMask"), !!q("cardGradientMask")],
    hasApply: !!q("rangeMaskApply"),
    source: opts("rangeMaskSource"), target: opts("rangeMaskTarget"),
    kind: q("rangeMaskKind")?.value,
    lumShown: q("rangeMaskLumRow")?.style.display !== "none",
    colorShown: q("rangeMaskColorRow")?.style.display !== "none",
    tools: ["mask_linear_gradient", "mask_radial_gradient"].map((t) => !!document.querySelector('button[data-tool="' + t + '"]')),
  };
})()`);
if (!shell.cards.every(Boolean) || !shell.hasApply) bad("范围遮罩 / 渐变蒙版两张卡不在页面上", shell.cards);
else ok("两张卡都在页面上（含「烘焙并挂载」按钮）");
if (shell.source.length !== 6 || shell.target.length !== 6 || !shell.source.includes(sourceLayer)) {
  bad("两个图层框没有装着 list_layers 的六个图层", { source: shell.source.length, target: shell.target.length });
} else ok("采样 / 挂到两个图层框都由服务端图层填充", { layers: shell.source.length });
if (!shell.tools.every(Boolean)) bad("工具栏里没有那两支渐变笔", shell.tools);
else ok("工具栏里有线性 / 径向两支渐变蒙版笔");
if (!(shell.kind === "luminance" && shell.lumShown && !shell.colorShown)) {
  bad("默认应显示亮度带那一行、藏起色相带那一行", shell);
} else ok("带的种类切换会换出对应那一行");

/** 填一张范围遮罩卡并点「烘焙并挂载」✓（走的是界面上那条真实路径 ✓ 不直接调工具 ✓）。 */
async function bake({ kind, target, values, invert = false }) {
  await evaluate(`(() => {
    const q = (id) => document.getElementById(id);
    q("rangeMaskKind").value = ${JSON.stringify(kind)};
    q("rangeMaskKind").dispatchEvent(new Event("change", { bubbles: true }));
    q("rangeMaskTarget").value = ${JSON.stringify(target)};
    q("rangeMaskSource").value = ${JSON.stringify(sourceLayer)};
    q("rangeMaskInvert").checked = ${JSON.stringify(invert)};
    ${Object.entries(values).map(([id, v]) => `q(${JSON.stringify(id)}).value = ${JSON.stringify(String(v))};`).join("\n")}
  })()`);
  await evaluate(`document.getElementById("rangeMaskApply").click()`);
  await sleep(2000);
  return (await evaluate(`document.getElementById("rangeMaskInfo").textContent`)) || "";
}
/** 日志行 ✓（`log()` 每行一个 `<div>` ✓ 最新在前 ✓ ⇒ 按**整行**匹配 ✗ 不是在一坨文本里找子串 ✓）。 */
const logLines = async () => (await evaluate(`[...document.getElementById("log").children].map((d) => d.textContent)`)) || [];
const logText = async () => (await logLines()).join("\n");

// ---------- ② 覆盖 > 0 ⇒ 挂载，且卡片那个数 == 服务端回执那个数 ----------
const info1 = await bake({ kind: "luminance", target: scenarioLayers.lum, values: { rangeMaskLow: 0.5, rangeMaskHigh: 1, rangeMaskSmoothLum: 0.05 } });
const mask1 = await maskOf(scenarioLayers.lum);
if (!mask1) bad("覆盖 > 0 的亮度带没有挂到目标图层（mask_id 仍为空）", info1);
else ok("亮度带 0.5–1 已挂载到「挂到」那一层", { mask: mask1 });
if (!/14400/.test(info1)) bad("卡片没有写出服务端给的覆盖像素数（白条应为 14400）", info1);
else ok("卡片写出的覆盖数与服务端回执同源", info1);

// ---------- ③ 覆盖 == 0 ⇒ 拒绝挂载并说清（工具自己可是回 ok:true ✓）----------
const info2 = await bake({ kind: "luminance", target: scenarioLayers.zero, values: { rangeMaskLow: 0.3, rangeMaskHigh: 0.4, rangeMaskSmoothLum: 0 } });
const mask2 = await maskOf(scenarioLayers.zero);
if (mask2) bad("带内没有像素却仍然挂载了（应当拒绝）", { mask: mask2, info: info2 });
else ok("覆盖为 0 时拒绝挂载（目标层仍然没有蒙版）");
if (!/带内没有像素/.test(info2)) bad("拒绝挂载时没有说清原因", info2);
else ok("拒绝挂载时给出了原因与下一步", info2);

// ---------- ④ 色相带挑中红条 ----------
const info3 = await bake({ kind: "color", target: scenarioLayers.hue, values: { rangeMaskHue: 0, rangeMaskHueWidth: 60, rangeMaskSmoothColor: 0.25 } });
const mask3 = await maskOf(scenarioLayers.hue);
if (!mask3) bad("色相带 0±30 没有挂载（红条应当被挑中）", info3);
else ok("色相带已挂载", { mask: mask3 });
if (!/14400/.test(info3)) bad("色相带卡片没有写出覆盖数（红条应为 14400 像素）", info3);
else ok("色相带的覆盖读数与白底同源", info3);

// ---------- ⑤ 非法区间就地拒绝（服务端也会拒 ✗ 但先把话说清 ⇒ 不产生废原子）----------
const info4 = await bake({ kind: "luminance", target: scenarioLayers.zero, values: { rangeMaskLow: 0.8, rangeMaskHigh: 0.2, rangeMaskSmoothLum: 0.05 } });
if (!/下限必须不大于上限/.test(info4)) bad("下限 > 上限时没有就地拒绝", info4);
else ok("下限 > 上限就地拒绝，且没有去烘焙", info4);

// ---------- ⑥ 两支渐变笔：真实拖动 ⇒ 提交并挂载 ✓ 羽化无效必须出声 ----------
// **羽化故意留非零** ✓：渐变 kind 不收羽化 ⇒ 界面必须**忽略它并说出来** ✗
// 照抄上去的结局是服务端 `invalid_argument` ⇒ 人只看到"拖了一下没反应"✓。
await evaluate(`document.getElementById("feather").value = 12`);
const drag = async (from, to) => {
  await send("Input.dispatchMouseEvent", { type: "mousePressed", x: Math.round(from.x), y: Math.round(from.y), button: "left", clickCount: 1 });
  for (let i = 1; i <= 6; i += 1) {
    await send("Input.dispatchMouseEvent", {
      type: "mouseMoved",
      x: Math.round(from.x + (to.x - from.x) * i / 6),
      y: Math.round(from.y + (to.y - from.y) * i / 6),
      button: "left",
    });
  }
  await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: Math.round(to.x), y: Math.round(to.y), button: "left", clickCount: 1 });
  await sleep(2000);
};
/** 文档坐标 → 屏幕坐标 ✓（按 `state().viewport` 现算 ✓ 与视口/缩放/布局无关 ✓）。 */
const screenOf = (x, y) => evaluate(`(() => {
  const s = window.yanshi.state();
  const b = document.getElementById("board").getBoundingClientRect();
  return { x: b.left + (${x} - s.viewport.x) / s.viewport.w * b.width,
           y: b.top + (${y} - s.viewport.y) / s.viewport.h * b.height };
})()`);
// 渐变蒙版挂在**当前编辑图层**上 ✓ ⇒ 先把当前层切到场景层（与用户切图层的同一条路径 ✓）。
const useLayer = (layerId) => evaluate(`(() => {
  const sel = document.getElementById("layer");
  sel.value = ${JSON.stringify(layerId)};
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  return window.yanshi.state().layerId;
})()`);

if ((await useLayer(scenarioLayers.linear)) !== scenarioLayers.linear) {
  console.error("❌ 判据前提失败：切不动当前编辑图层");
  process.exit(3);
}
await evaluate(`window.yanshi.setTool("mask_linear_gradient")`);
await drag(await screenOf(20, 90), await screenOf(220, 90));
const mask4 = await maskOf(scenarioLayers.linear);
if (!mask4) bad("线性渐变蒙版拖动后没有挂到图层", { log: (await logText()).slice(-300) });
else ok("线性渐变蒙版已提交并挂载", { mask: mask4 });
const featherLines = (await logLines()).filter((line) => /羽化/.test(line));
if (!featherLines.length) bad("羽化对渐变无效这件事没有说出来（静默吞掉了一次拖动）");
else ok("羽化被忽略并且说清了", featherLines[0]);

await useLayer(scenarioLayers.radial);
await evaluate(`window.yanshi.setTool("mask_radial_gradient")`);
await evaluate(`document.getElementById("maskFalloff").value = 0.5`);
await drag(await screenOf(40, 40), await screenOf(200, 140));
const mask5 = await maskOf(scenarioLayers.radial);
if (!mask5) bad("径向渐变蒙版拖动后没有挂到图层", { log: (await logText()).slice(-300) });
else ok("径向渐变蒙版已提交并挂载", { mask: mask5 });

// ---------- ⑦ 取下蒙版：挂上去的东西必须拿得下来（断的是**服务端事实** ✓ 不是页面上的字 ✓）----------
// **为什么这一条必须钉住** ✓：挂接与取下是同一个 `set_property mask_id` ✓ 而"取下"只认
// **JSON `null`** ✗ 填**空串**会被引用检查拒掉（`reference_not_found` ✓ 实测 ✓）⇒
// 界面送错形状就是"点了没反应"✓。另外**本来就没有蒙版时不许写原子** ✓
//（点一下留一条空改动 ✓ 与"什么都没发生"是两回事 ✓）。
const atomsBefore = (await callTool("get_log", {})).count;
await evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  q("rangeMaskTarget").value = ${JSON.stringify(scenarioLayers.lum)};
  q("rangeMaskTarget").dispatchEvent(new Event("change", { bubbles: true }));
  q("rangeMaskDetach").click();
  return 1;
})()`);
await sleep(2200);
const maskAfterDetach = await maskOf(scenarioLayers.lum);
if (maskAfterDetach) bad("点了「取下蒙版」那一层还挂着东西", maskAfterDetach);
else ok("「取下蒙版」断开了那一层（服务端 mask_id 变空）");
const currentLine = await evaluate(`document.getElementById("rangeMaskCurrent").textContent`);
if (/蒙版 mask_/.test(currentLine)) bad("现状行还写着刚才那个蒙版（没跟着服务端刷新）", currentLine);
else ok("现状行跟着服务端事实走", currentLine);
// **再点一次**：必须说"本来就没有" ⇒ 并且**原子数不变** ✓。
const atomsMid = (await callTool("get_log", {})).count;
await evaluate(`document.getElementById("rangeMaskDetach").click()`);
await sleep(1800);
const infoDetach2 = (await evaluate(`document.getElementById("rangeMaskInfo").textContent`)) || "";
if (!/本来就没有蒙版/.test(infoDetach2)) bad("本来就没有蒙版时页面什么都没说", infoDetach2);
else ok("本来就没有蒙版时说得清（且不假装取下）", infoDetach2);
const atomsAfter = (await callTool("get_log", {})).count;
if (!(atomsBefore < atomsMid)) bad("取下那一步根本没写原子（前提变了）", { atomsBefore, atomsMid });
if (atomsAfter !== atomsMid) bad("本来就没有蒙版却仍写了一条空改动", { atomsMid, atomsAfter });
else ok("再点一次不写任何原子", { count: atomsAfter });

// ---------- ⑧ 页面不许抛错 ----------
if (pageErrors.length) bad("页面抛出 " + pageErrors.length + " 条 JS 错误", pageErrors.slice(0, 3));
else ok("全程 0 条 JS 错误");

// ---------- ⑨ 本地内核模式下不许把失败赖给服务端 ----------
// **放在⑦之后** ✓：这一模式本身会刷一堆 `needs_render` 的内核日志 ✓（与本次改动无关 ✓
// 已单独登记 ✗），混进"页面抛错"那条审计里就分不清是谁的责任了 ✓。
// 要钉的东西 ✓：`api-local.js` **没有** `/api/tools/create_range_mask` 这个端点 ✓
// ⇒ 请求**根本没出门** ✗ 这时卡片上一句「服务端拒绝了这次烘焙」会把人送去查服务端日志 ✓
//（实测就是这个假归因 ✓）⇒ 必须说清"本地内核没有这个端点"并给出下一步 ✓。
await evaluate(`localStorage.setItem("yanshi.serverRender", "0"); 1`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let rebooted = false;
for (let i = 0; i < 60; i += 1) {
  rebooted = await evaluate(`!!(window.yanshi && document.getElementById("rangeMaskSource")?.options.length >= 6)`);
  if (rebooted) break;
  await sleep(250);
}
// **让 init 把点击监听接上**（第 1132 轮）：本地模式初始化的顺序是「先填下拉、后
// `setupRangeMaskPanel()` 绑 click」⇒ 只等下拉会在监听绑定前点击 ⇒ 点了什么都没发生（空回执）✗。
await sleep(900);
const kernelMode = await evaluate(`(() => { const b = document.getElementById("useServerRender"); return b ? b.checked === true : null; })()`);
if (!rebooted || kernelMode !== false) {
  console.error("❌ 判据前提失败：切不到本地内核模式（服务器渲染勾选实为 " + JSON.stringify(kernelMode) + "）");
  process.exit(3);
}
await evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  q("rangeMaskKind").value = "luminance";
  q("rangeMaskKind").dispatchEvent(new Event("change", { bubbles: true }));
  q("rangeMaskSource").value = ${JSON.stringify(sourceLayer)};
  q("rangeMaskTarget").value = ${JSON.stringify(scenarioLayers.zero)};
  q("rangeMaskLow").value = "0.5";
  q("rangeMaskHigh").value = "1";
  q("rangeMaskSmoothLum").value = "0.05";
  q("rangeMaskInfo").textContent = "";
  q("rangeMaskApply").click();
  return 1;
})()`);
// **等回执出现**（第 1132 轮）：本地内核首次调用要初始化/可能慢 ⇒ 固定 2s 会读到空串 ✗
//（本机实测：2.5s 才有字 ✓）。轮询到有字或 10s 超时。
let infoKernel = "";
for (let i = 0; i < 160; i += 1) {
  infoKernel = (await evaluate(`document.getElementById("rangeMaskInfo").textContent`)) || "";
  if (infoKernel) break;
  await sleep(250);
}
if (!/本地内核没有这个端点/.test(infoKernel)) bad("本地内核缺端点时仍报「服务端拒绝」（假归因）", infoKernel);
else ok("本地内核缺端点时说清了是本地缺端点，并给出下一步", infoKernel);
if (/服务端拒绝/.test(infoKernel)) bad("同一条回执里既说本地又说服务端", infoKernel);
if (await maskOf(scenarioLayers.zero)) bad("本地内核模式下却挂上了服务端蒙版（判据前提变了）");
// **擦回去** ✓：这个 key 是**按源共享**的 ✗ 留着会让后面别的判据在没人预期的模式下跑 ✓。
await evaluate(`localStorage.removeItem("yanshi.serverRender"); 1`);

await sendB("Target.closeTarget", { targetId: created.result.targetId });
ws.close();
console.log(problems.length ? "✗ 范围遮罩 / 渐变蒙版判据不合格：" : "✓ 范围遮罩与渐变蒙版在页面上真的可用");
if (problems.length) { for (const p of problems) console.log("   - " + p); process.exit(1); }
