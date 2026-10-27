#!/usr/bin/env node
// **判据：频率分离（`frequency_separation`）必须在查看器里真的能用** ✓。
//
// **抓的是哪一类缺口** ✓：这个工具早已在服务端 ✓ 且在 HTTP 缺省面（`retouch` 组）里 ✓
// 而本轮之前 `grep frequency_separation crates/yanshi-http/` 命中 **0** ✗
// ⇒ "把色调与细节分成两层来修"这件修图最常用的事 ✓ 人只能靠 Agent 才做得到 ✗。
//
// **钉住的七件事**（每条都断言**服务端事实** ✗ 不是页面上有几行字 ✓）：
//   ① 卡片在页面上 ✓ 「源图层」下拉由 `list_layers` 填 ✓ 「该层有 N 个对象」那句
//      取的是**服务端给的 `objects`**（判据把界面写的数与 HTTP 读回的数对上 ✓）；
//   ② 半径越界 ⇒ **就地拒绝** ⇒ 不发请求 ⇒ **一个原子都不多**✓ 一层都不多 ✓；
//   ③ 空层（`objects === 0`）⇒ 同样就地拒绝 ✓ 并说清"服务端也会这样拒"✓；
//   ④ 服务端**自检不过**那条路径（实测平涂 #204060 半径 8 ⇒ 均值 0.017 超 0.0118 的限 ✓）
//      ⇒ 卡片必须说"服务端拒绝"✓ 并证明**它拒绝时不建组**（层数前后一致 ✓ 不留半套层 ✓）；
//   ⑤ 成功路径 ⇒ 服务端真的多出"一个组 + 四层"✓ 高频层 `blend_mode === linear_light` ✓
//      组名 `「<前缀> · 频率分离」✓`；
//   ⑥ **卡片上的四个误差读数必须与服务端回执同源** ✓：判据对**同一层同一半径**再直接调一次工具
//      （分离是从那一层自身烘焙 ✓ 与上面已经建出的组无关 ✓）⇒ 把回执 `toFixed(4)` 后
//      与界面写出的数逐位对上 ✓（界面自算就等于造了第二个真值 ✗）；
//   ⑦ 全程 0 条页面 JS 错误 ✓；
//   ⑧ 勾掉「服务器渲染」时（`api-local.js` 没有这个端点 ✓ 请求根本没出门 ✗）
//      必须说清是**本地内核缺端点** ✓ 不许假归因给服务端 ✓ 且服务端侧一层没多 ✓。
//
// **口径**：
//   · 每个场景用**各自的图层**⇒ 断言互不干扰 ✓（同 542 轮那条教训 ✓）；
//   · **先把这个浏览器 profile 的偏好擦干净再进文档**✓：`localStorage` 按源共享 ✗
//     残留 `yanshi.serverRender="0"` 会让判据拿着**本地路由的失败**当产品结论 ✓
//     （本轮实跑就是这样假失败过一次 ✓ 所以这里既擦 ✓ 又**核实**勾选生效 ✓ 擦不动 ⇒ `exit(3)`）；
//   · 前提取不到 ⇒ **判据自己失效** ⇒ `process.exit(3)` ✓（AGENTS.md：不许把环境问题判成产品红 ✓）。
//
// 用法：node scripts/browser-frequency-separation.mjs <viewer-url> [cdp-port]
//   例：node scripts/browser-frequency-separation.mjs http://127.0.0.1:15900 9490
import { setTimeout as sleep } from "node:timers/promises";

// **★ 总超时守卫 ✗ ★**（本仓约定 ✓）：要连 CDP 浏览器 ⇒ 起不来时会**一直等** ✗ 拖住 CI 分片 ✓。
//   外层给 browser-* 的是 `timeout ${BROWSER_TIMEOUT:-420}` ⇒ 内层落在它之下 ✓（180 s ✓）。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 180000);

const url = process.argv[2];
// **★ 端口优先读环境变量 ✗ ★**（`browser-reference-overlay.mjs:20-25` 同一处教训 ✓）：
//   `run-criteria.sh` 给 browser-* 传 `<viewer-url> <server-base> <token> <cdpPort>` ✓
//   ⇒ argv[3] 是个 URL ✗ 拿它当端口 ⇒ CI 里**恒红** ✓（本轮自查发现 ✓）。
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) {
  console.error("用法: node scripts/browser-frequency-separation.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

// ---------- 一次性文档 + 夹具 ----------
const docId = "t_freqsep_" + Date.now().toString(36).slice(-6);
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
const baseLayer = ((await callTool("list_layers", {})).layers || [])[0]?.layer_id;
if (!baseLayer) { console.error("❌ 判据前提失败：拿不到初始图层"); process.exit(3); }
// 三条场景层：可分离（平涂 #3f7fbf ✓ 实测自检过得了）/ 空层（0 对象 ✓）/ 自检不过（平涂 #204060 ✓）
for (const name of ["ok", "empty", "selfcheck"]) {
  const made = await callTool("create_layer", { layer_id: "layer_" + name, name: "场景 " + name });
  if (!made.ok) { console.error("❌ 判据前提失败：建场景图层 " + name + " ⇒ " + JSON.stringify(made).slice(0, 200)); process.exit(3); }
}
const fills = [];
for (const [layer, color] of [["layer_ok", "#3f7fbf"], ["layer_selfcheck", "#204060"]]) {
  fills.push(await callTool("fill", { layer_id: layer, data: { color, region: { x: 0, y: 0, w: 240, h: 180 } } }));
}
if (!fills.every((f) => f.ok)) { console.error("❌ 判据前提失败：铺底色 ⇒ " + JSON.stringify(fills).slice(0, 200)); process.exit(3); }
const layersNow = (await callTool("list_layers", {})).layers;
if (layersNow.length !== 4) { console.error("❌ 判据前提失败：夹具层数不是 4（实为 " + layersNow.length + "）"); process.exit(3); }
/** **别把 `|| 0` 写在 `Number()` 里** ✓（判据自己踩过的写法 ✗）：`objects` 合法取值就是 **0** ✓
 *  而 `Number(0 || -1)` ⇒ `-1` ⇒ 前提检查把"空层"读成"没这层"✓ 判据会在产品没毛病时 `exit(3)` ✓。
 *  所以这里只在**层找不到**时给 `NaN` ✓ 拿到什么数就照什么数判 ✓。 */
const objectsOf = async (layerId) => {
  const found = (await callTool("list_layers", {})).layers.find((l) => l.layer_id === layerId);
  return found ? Number(found.objects) : NaN;
};
if (await objectsOf("layer_ok") !== 1 || await objectsOf("layer_empty") !== 0) {
  console.error("❌ 判据前提失败：夹具的对象计数不对（ok 应为 1 ✓ empty 应为 0 ✓）");
  process.exit(3);
}
// **前提：服务端确实会拒绝 #204060 平涂（自检不过）✓** ✗ 否则 ④ 测的就不是那条路径 ✓。
// 直接调一次拿到期望失败 ⇒ 拿完就把建出来的东西确认掉（自检不过 ⇒ 本来就不该有层 ✓）。
const refused = await callTool("frequency_separation", { layer_id: "layer_selfcheck", radius: 8 });
if (refused.ok || refused.error_code !== "invalid_argument") {
  console.error("❌ 判据前提失败：#204060 半径 8 这次没被自检拒绝 ⇒ " + JSON.stringify(refused).slice(0, 200));
  process.exit(3);
}
if ((await callTool("list_layers", {})).layers.length !== 4) {
  console.error("❌ 判据前提失败：服务端自检拒绝时**居然建了层**（那 ④ 就没东西可判了）");
  process.exit(3);
}
/** **`get_log` 的 `count` 是"本次返回了几行"** ✓（实测 ✓）：带 `limit: 1` 去读它 ⇒ 永远得 1 ✗
 *  "没多写原子"这条断言就会**恒成立**（判据自己失效 ✗ 比没判还坏 ✓）。
 *  所以这里**不给 limit** ✓ 让 `count` 等于整条日志的长度 ✓（与 542 轮那条判据同一用法 ✓）;
 *  新原子的判据一律按 `seq` 比 ✓（`get_log` 从头给 ✓ 用 `next_since` 翻页 ✓）。 */
const atomCount = async () => Number((await callTool("get_log", {})).count || 0);
const atomsBefore = await atomCount();
if (!(atomsBefore > 0)) { console.error("❌ 判据前提失败：读不到原子计数"); process.exit(3); }

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
/** `Runtime.evaluate` 必须 `returnByValue` + `awaitPromise` 成对 ✓（AGENTS.md ✓）。 */
async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  const body = res.result || {};
  if (body.exceptionDetails || res.exceptionDetails) {
    throw new Error("Runtime.evaluate 抛了：" + JSON.stringify(body.exceptionDetails || res.exceptionDetails).slice(0, 300));
  }
  return body.result ? body.result.value : undefined;
}
// **擦偏好**✓ 再进被测文档 ✓（见抬头「口径」里那条实测教训 ✓）。
// **★ 导航 URL 上钉死 `render=server` ✗ ★**（本轮 CI 接线自查发现 ✓）：
//   `run-criteria.sh` 给 browser-* 一族**统一加 `&render=client`** ✓ 而 `viewer-app.js:83-88` 里
//   **URL 参数优先级在 `localStorage` 之上** ✗ ⇒ 光擦存储擦不掉它 ✓ 页面照样走本地内核 ✓
//   ⇒ 本判据**经服务端 API 写下去的状态**（图层/原子/偏好）在页面上**读不回来** ✓
//   测的就不再是它要测的那一半 ✓（本机手工跑不带这个参数 ⇒ 看不出来 ✗ CI 里恒红 ✓）。
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(800);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let booted = false;
for (let i = 0; i < 60; i += 1) {
  booted = await evaluate(`!!(window.yanshi && document.getElementById("freqSepSource")?.options.length >= 4)`);
  if (booted) break;
  await sleep(250);
}
if (!booted) {
  console.error("❌ 判据前提失败：开机后「源图层」下拉没装着 4 个图层（四个 = 1 个初始层 + 3 个场景层）");
  process.exit(3);
}
if ((await evaluate(`document.documentElement.lang`)) !== "zh-CN") {
  console.error("❌ 判据前提失败：界面没落在中文（实为 " + JSON.stringify(await evaluate(`document.documentElement.lang`)) + "）");
  process.exit(3);
}
// **★ 有界等待这条前提 ✗ ★**（登记 ⑰ 同族 ✓）：**单次**读勾选可能读到**上一次**导航留下的页面 ✗
//   （实测：同一条判据一次绿、一次假 `exit 3` ✓）⇒ **等前提、不等结论** ✓
//   轮询到「勾选 = true」且**当前 URL 确实带着 `render=server`** 才认 ✓。
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
  console.error("❌ 判据前提失败：「服务器渲染」那个勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）");
  process.exit(3);
}
const cardText = () => evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  return JSON.stringify({
    current: q("freqSepCurrent") ? q("freqSepCurrent").textContent : null,
    info: q("freqSepInfo") ? q("freqSepInfo").textContent : null,
    source: q("freqSepSource") ? q("freqSepSource").value : null,
  });
})()`).then((s) => JSON.parse(s));
const pick = (layerId) => evaluate(`(() => {
  const s = document.getElementById("freqSepSource");
  s.value = ${JSON.stringify(layerId)};
  s.dispatchEvent(new Event("change", { bubbles: true }));
  return s.value;
})()`);
const setRadius = (value) => evaluate(`(() => { const i = document.getElementById("freqSepRadius"); i.value = ${JSON.stringify(String(value))}; return i.value; })()`);
const run = () => evaluate(`(() => { const b = document.getElementById("freqSepRun"); if (!b) return "no-button"; b.click(); return 1; })()`);
const layerRows = async () => (await callTool("list_layers", {})).layers;
// **等面板接线完成**（与 browser-dodge-burn 同一处开机空窗 ✓ 2026-10-27 CI 实测）：
//   下拉在 `refreshLayers` 里就填好了 ✓ 而 `freqSepRun` 的 click 与 `freqSepSource`
//   的 change 要到 `warmKernel` 尾部的 `setupFreqSepPanel` 才挂 ⇒ 窗口内的点击是
//   **空转**（CI ⑧ 红的 `{page:""}` 即此 ✓ 不是产品坏了 ✗）。两个监听在同一函数
//   里同一刻挂上 ⇒ 探针：**换两次源图层、现状句必须跟着换**（开机时
//   `renderFreqSepLayers` 自己会写一次现状 ⇒ "非空"不算接线证据 ✗ 第 62 轮实测
//   本地臂就是被这个假阳性骗过去 ⇒ 点击空转、卡片全空）。两条不同图层给出
//   **两句不同的话**只可能是 change 监听跑过 ⇒ 才算证明 click 也接好了。
//   循环里顺手敲一次 `refreshLayers`：记录是开机快照 ✗ 场景原子的落库事件可能晚于它。
const waitPanelWired = async () => {
  for (let i = 0; i < 120; i += 1) {
    await evaluate(`typeof refreshLayers === "function" ? (void refreshLayers(), 1) : 0`);
    await pick("layer_empty");
    const a = (await cardText()).current || "";
    await pick("layer_ok");
    const b = (await cardText()).current || "";
    if (a !== b) return true;
    await sleep(250);
  }
  return false;
};
if (!(await waitPanelWired())) {
  console.error("❌ 判据前提失败：等了 30 秒换源图层仍不出现现状读数（面板监听始终没接线 ⇒ 后面的点击全是空转，判不下去了）");
  process.exit(3);
}

// ---------- ① 卡片与下拉：读数取服务端 ----------
const shell = await evaluate(`(() => {
  const q = (id) => document.getElementById(id);
  return JSON.stringify({
    card: !!q("cardFreqSep"),
    options: q("freqSepSource") ? q("freqSepSource").options.length : 0,
    values: q("freqSepSource") ? [...q("freqSepSource").options].map((o) => o.value) : [],
    hasRun: !!q("freqSepRun"),
  });
})()`).then((s) => JSON.parse(s));
if (shell.card && shell.hasRun && shell.options === 4) ok("频率分离卡在页面上，「源图层」由服务端图层填充", { options: shell.options });
else bad("频率分离卡或图层下拉不在位", shell);
if (JSON.stringify(shell.values) === JSON.stringify((await layerRows()).map((l) => l.layer_id))) {
  ok("下拉里的图层 id 与 `list_layers` 逐一对应", { count: shell.values.length });
} else {
  bad("下拉与服务端图层清单不一致", { page: shell.values, server: (await layerRows()).map((l) => l.layer_id) });
}
await pick("layer_ok");
let read1 = await cardText();
let serverObjects = await objectsOf("layer_ok");
// **有界重试到与服务端一致**（第 62 轮定性 ✓ 本机实测 `server:1／page:该层没有对象` ✗）：
//   下拉与 `freqSepRecords` 是**开机那一次** `refreshLayers` 的快照 ✓ 场景原子落库到界面
//   生效有先后 ⇒ 单次读会量到**旧快照** ✗（CI 快、恰好赶上刷新就绿 ✓ 本机慢就红 ✗）。
for (let i = 0; i < 80; i += 1) {
  if (read1.current === "该层有 " + serverObjects + " 个对象 ⇒ 可以分离") break;
  await sleep(250);
  // **主动刷一次图层快照**（记录只在开机与 WS 事件时刷新 ✗ 场景提交若早于开机抓取、
  // 又没有后续原子 ⇒ 旧快照不会自己追平 ⇒ 判据自己敲 `refreshLayers` 的门 ✓）。
  await evaluate(`typeof refreshLayers === "function" ? (void refreshLayers(), 1) : 0`);
  await pick("layer_ok");
  read1 = await cardText();
  serverObjects = await objectsOf("layer_ok");
}
if (read1.current === "该层有 " + serverObjects + " 个对象 ⇒ 可以分离") {
  ok("「该层有 N 个对象」用的是服务端给的 objects", { server: serverObjects, page: read1.current });
} else {
  bad("对象读数与服务端不一致", { server: serverObjects, page: read1.current });
}
await pick("layer_empty");
read1 = await cardText();
if (/没有对象/.test(read1.current || "")) ok("换成空层时那句跟着变（不是留着上一位的读数）", { page: read1.current });
else bad("选到空层后现状行没跟着走", read1);

// ---------- ② 半径越界：就地拒绝，一个原子都不写 ----------
await pick("layer_ok");
await setRadius("0");
const atomsBefore2 = await atomCount();
await run();
await sleep(600);
read1 = await cardText();
const layersAfter2 = (await layerRows()).length;
const atomsAfter2 = await atomCount();
if (/半径要在 1–64/.test(read1.info || "") && layersAfter2 === 4 && atomsAfter2 === atomsBefore2) {
  ok("半径 0 被就地拒绝：没建层、没写原子", { page: read1.info, layers: layersAfter2, atoms: atomsAfter2 });
} else {
  bad("半径越界没被就地拒绝（或它偷偷写了东西）", { page: read1.info, layers: layersAfter2, atoms: atomsBefore2 + "→" + atomsAfter2 });
}
await setRadius("70");
await run();
await sleep(600);
read1 = await cardText();
if (/半径要在 1–64/.test(read1.info || "") && (await atomCount()) === atomsBefore2) {
  ok("半径 70（超上界）同样就地拒绝", { page: read1.info });
} else {
  bad("半径 70 没有被就地拒绝", { page: read1.info, atoms: await atomCount() });
}
await setRadius("8");

// ---------- ③ 空层：就地拒绝并说清服务端也会这样拒 ----------
const atomsBefore3 = await atomCount();
await pick("layer_empty");
await run();
await sleep(600);
read1 = await cardText();
if (/没有可分离的像素/.test(read1.info || "") && (await atomCount()) === atomsBefore3 && (await layerRows()).length === 4) {
  ok("空层被就地拒绝（不发请求），并说清服务端也会这样拒", { page: read1.info });
} else {
  bad("空层那条路径没拦住", { page: read1.info, atoms: await atomCount(), layers: (await layerRows()).length });
}

// ---------- ④ 服务端自检拒绝：说清是服务端拒绝，且不留半套层 ----------
const atomsBefore4 = await atomCount();
await pick("layer_selfcheck");
await run();
await sleep(2500);
read1 = await cardText();
const layersAfter4 = (await layerRows()).length;
const serverLog = await callTool("get_log", {});
const kindsSince = (serverLog.atoms || []).filter((a) => Number(a.seq) > atomsBefore4).map((a) => a.kind);
if (/服务端拒绝/.test(read1.info || "") && !/本地内核/.test(read1.info || "") &&
    layersAfter4 === 4 && kindsSince.length === 0) {
  ok("自检不过 ⇒ 卡片说是服务端拒绝，且服务端一侧一层没多（不留半套组）", { page: read1.info, layers: layersAfter4 });
} else {
  bad("自检不过那条路径不对", { page: read1.info, layers: layersAfter4, newAtomKinds: kindsSince });
}

// ---------- ⑤⑥ 成功路径：结构 + 读数同源 ----------
await pick("layer_ok");
await run();
await sleep(3000);
read1 = await cardText();
const after = await layerRows();
const built = after.filter((l) => !["layer_default", baseLayer, "layer_ok", "layer_empty", "layer_selfcheck"].includes(l.layer_id));
const group = built.find((l) => l.kind === "layer_group");
const kids = built.filter((l) => l.parent_id === (group && group.layer_id));
const hf = kids.find((l) => /高频$/.test(l.name || ""));
const lf = kids.find((l) => /低频$/.test(l.name || ""));
if (group && kids.length === 4 && hf && hf.blend_mode === "linear_light") {
  ok("分离真的建出「一个组 + 四层」，高频层是 linear_light", { group: group.name, kids: kids.map((k) => [k.name, k.blend_mode, k.objects]) });
} else {
  bad("分离后的图层结构不对", { group: group && group.name, kids: kids.map((k) => [k.name, k.blend_mode]) });
}
// 卡片必须把**服务端回执**的那四个数写出来 ✓ 判据对同一层同一半径再直接调一次 ✓ 逐位对上 ✓
const direct = await callTool("frequency_separation", { layer_id: "layer_ok", radius: 8 });
const fmt = (v) => (typeof v === "number" ? v.toFixed(4) : "?");
const expectRadius = fmt(direct.radius);
const expectMean = fmt(direct.reconstruction_mean_error);
const expectMax = fmt(direct.reconstruction_max_error);
const expectBright = fmt(direct.reconstruction_max_error_bright);
const sameNumbers = (read1.info || "").includes("半径 " + expectRadius) &&
  (read1.info || "").includes("均值 " + expectMean) &&
  (read1.info || "").includes("最大 " + expectMax) &&
  (read1.info || "").includes("中高调最大 " + expectBright);
if (sameNumbers) {
  ok("卡片上的半径与三个误差读数与服务端回执同源（同一层同一半径再直调一次比对）", { page: read1.info, server: [expectRadius, expectMean, expectMax, expectBright] });
} else {
  bad("卡片读数与服务端回执不同源", { page: read1.info, server: { radius: expectRadius, mean: expectMean, max: expectMax, brightMax: expectBright } });
}
if (/低频 layer_/.test(read1.info || "") && /高频 layer_/.test(read1.info || "")) {
  ok("卡片写出新建两层的 id（人不用去图层列表里翻）", { page: read1.info.slice(0, 60) });
} else {
  bad("卡片没写出新建层的 id", { page: read1.info });
}

// ---------- ⑦ 0 条 JS 错误 ----------
if (pageErrors.length === 0) ok("全程 0 条 JS 错误");
else bad("页面抛了 JS 错误", pageErrors.slice(0, 3));

// ---------- ⑧ 勾掉「服务器渲染」：改文档的工具必须**如实落服务端**，界面不许假归因 ----------
// **★ 这一臂断的契约变了 ✗ ★**（第 62 轮定性 ✓ 证据链：CI `3c8cf8cd` 全绿在前、
//   `48f3190`（合入外部 changes.patch）把 `callTool` 路由改成「**勾掉服务器渲染只关像素
//   路径** ⇒ 改文档的工具仍走服务端」（`viewer-app.js` 的 `localOnly || readOnly` 闸 ✓）
//   ⇒ 本判据的页面在**服务端 origin、非 localOnly** ⇒ `frequency_separation` 必然出得了门 ✓
//   旧断言「请求根本没出门 ⇒ 本地内核没有这个端点」**在这页永远触发不了** ✗（不是产品坏了，
//   是判据钉的旧前提被设计变更撤销 ✓）。那句假归因防线只剩 localOnly PWA 部署能触发 ⇒
//   要守它需要单独的 PWA 夹具轮次（已记 HANDOFF ✗ 本臂先按新契约判 ✓）。
//   新契约下这一臂钉两件事：**①** 勾掉开关后分离**仍真的落到服务端**（卡片念得出服务端
//   回执的半径 ⇒ 请求出了门且数字同源 ✓）；**②** 界面**不许反过来谎称**「本地内核没有
//   这个端点」（请求明明出了门 ✗ 说了就是同类假归因 ✓ 与 542 轮那条同族、方向相反）。
await evaluate(`(() => { localStorage.setItem("yanshi.serverRender", "0"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let booted2 = false;
for (let i = 0; i < 60; i += 1) {
  booted2 = await evaluate(`!!(window.yanshi && document.getElementById("freqSepSource")?.options.length >= 4)`);
  if (booted2) break;
  await sleep(250);
}
// 这一臂**故意不钉** `render=server` ✓（登记 ⑭ ✓）⇒ 由 `localStorage` 决定走本地像素路径 ✓
//   **但有界等待**（登记 ⑰ 同族 ✓）：等到「当前 URL 不带 `render=server`」**且勾选 = false」✓
let offState = null;
for (let i = 0; i < 80; i += 1) {
  offState = await evaluate(`(() => {
    if (location.search.includes("render=server")) return "STILL-SERVER-PAGE";
    const b = document.getElementById("useServerRender");
    return b ? (b.checked === false ? false : true) : null;
  })()`);
  if (offState === false) break;
  await sleep(250);
}
if (!booted2 || offState !== false) {
  console.error("❌ 判据前提失败：切不到本地像素路径（booted=" + booted2 + " 勾选=" + JSON.stringify(offState) + "）");
  process.exit(3);
}
// 重新导航的页面 ⇒ 面板接线空窗重来（上面 ① 等到的只是上一个页面）。
if (!(await waitPanelWired())) {
  console.error("❌ 判据前提失败：本地臂等了 30 秒仍没接上面板监听（开机空窗同 ①，见上）");
  process.exit(3);
}
const atomsBefore8 = await atomCount();
const layersBefore8 = (await layerRows()).length;
await pick("layer_ok");
await run();
let info8 = "";
for (let i = 0; i < 80; i += 1) {
  read1 = await cardText();
  info8 = read1.info || "";
  if (info8) break;
  await sleep(250);
}
const layers8 = (await layerRows()).length;
if (/本地内核/.test(info8)) {
  bad("请求会落到服务端，界面却谎称本地内核缺端点（新契约下的反向假归因）", info8);
} else if (!/半径 /.test(info8)) {
  bad("勾掉服务器渲染后分离没有念出服务端回执（卡片空或文案不对）", { page: info8 });
} else if (!(layers8 > layersBefore8)) {
  bad("勾掉服务器渲染后分离没有真的落到服务端（层数没变）", { layers: layersBefore8 + "⇒" + layers8, page: info8.slice(0, 60) });
} else {
  ok("勾掉服务器渲染：改文档的分离仍如实落服务端、卡片念服务端回执（第 10 轮新契约）",
     { page: info8.slice(0, 60), layers: "+" + (layers8 - layersBefore8), atoms: await atomCount() - atomsBefore8 });
}
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); return 1; })()`);

await send("Target.closeTarget", { targetId: created.result.targetId });
ws.close();
if (problems.length) {
  console.log("判据失败 " + problems.length + " 条：\n  · " + problems.join("\n  · "));
  process.exit(1);
}
console.log("✓ 频率分离在页面上真的可用（结构、误差同源、四条拒绝路径都对上服务端事实）");
process.exit(0);
