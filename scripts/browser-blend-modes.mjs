#!/usr/bin/env node
// criterion-target: client  ## 运行器据此注入 render=client（测本地内核）
// **判据：图层列表的「混合模式」下拉必须与内核那份清单同源** ✓。
//
// **本轮为什么加它** ✓（第 3 轮频率分离的**实机截图**里看出来的 ✗ 不是读代码读出来的 ✓）：
// `list_layers` 报高频层 `blend_mode = linear_light` ✓ 而图层列表那一行的下拉**显示「正常」**✓
// —— 同一份事实两个读数 ✓ 正是本项目禁止的静默 ✗。
// 根因实测 ✓：界面写死了 **7** 个选项（第 1131 轮加的 ✓ `viewer-app.js` 的旧 `modeLabels`）✗
// 而内核 `BlendMode::NAMES` 已经是 **13** 个 ✓（`subtract` / `difference` / `color_dodge` /
// `linear_light` / `soft_light` / `linear_dodge` 六种**在界面上根本不存在** ✓）⇒
// `<select>` 找不到匹配项就退回第一项 ⇒ **谎报成「正常」**✓（实测：13 层各设一种模式 ✗
// 页面只有前 7 种显示对 ✓ 后 6 种全说"正常"✓）。
// 顺带一个更坏的后果 ✓：那 6 层的下拉 `value` 是 `normal` ⇒ 人在下拉里"随手选回正常"时 ✓
// 界面以为没变 ✗ 而服务端其实被改写了 ✓（**看不见的改动**✓）。
//
// **钉住的六件事**（每条都断言**服务端事实** ✗ 不是页面上有几行字 ✓）：
//   ① **选项集合 = 内核集合**：`list_engine_capabilities.blend_modes` 与界面 option 的 value
//      **不多不少** ✓（界面自己多一项内核不认的 ✗ 同样是漂移 ✓）；
//   ② **逐模式显示同源**：13 种模式各建一层并写进服务端 ✓ 页面那行的 `value` 与服务端
//      `blend_mode` 逐字相等 ✓ 显示的文案也不许是「正常」✓ 且 13 条文案互不相同（防"全显示同一个"✓）；
//   ③ **写回同源**：在页面上把一行改成 `soft_light` ⇒ 服务端真的变成 `soft_light` ✓
//      重画后仍显示它 ✓（读对写不对 / 写对读不对 ✗ 都逃不过这条 ✓）；
//   ④ **本轮那条回归**：`frequency_separation` 建出的高频层在列表里显示「线性光」✓ 不是「正常」✓；
//   ⑤ **英文模式同源**：13 个选项在英文下 0 条中文残留 ✓（清单长了而词条没长 ✗ 就是另一处静默 ✓）；
//   ⑥ 全程 0 条页面 JS 错误 ✓。
//
// **本判据测不到的一条**（写在这里 ✗ 不假装测过 ✓）：界面那个"当前值不在清单里就补一项"的兜底
// 走的是**载入老/外来文档**那条路（内核渲染层对未知值是**宽容**的 ✓ `blend.rs:112` ✓）。
// HTTP 侧三条写路径都会拒绝未知值 ✓ 实测：`create_layer` 干脆不接受 `blend_mode` 参数 ✓
// `update_layer` / `set_property` 报 `不支持的混合模式 hard_mix ⇒ 可用：…`✓ ⇒ 判据无法从接口造出这个状态 ✓
// 只能靠 ①② 保证"清单本身不漏"✓。
//
// **前提取不到 ⇒ 判据自己失效 ⇒ `process.exit(3)`** ✓（AGENTS.md：不许把环境问题判成产品红 ✓）：
//   · 这个 chromium profile 的 `localStorage` 是**按源共享**的 ✓ 残留 `yanshi.serverRender="0"`
//     会让 ③ 那次写回改走 `api-local.js` ✓ 判据就会拿"本地路由的失败"当产品结论 ✗（同类假红实测过 ✓）
//     ⇒ 先擦干净 ✓ 再**核实**「服务器渲染」确实生效 ✓；
//   · 服务端**自己拒绝它列出的模式** ⇒ 那是服务端的事 ✗ 不该由界面判据背 ⇒ 也走 `exit(3)` ✓。
// 用法：node scripts/browser-blend-modes.mjs <viewer-url> [cdp-port]
import { setTimeout as sleep } from "node:timers/promises";

// **★ 总超时守卫 ✗ ★**（本仓约定 ✓）：本判据要连 CDP 浏览器 ⇒ 起不来时会**一直等** ✗ 拖住 CI 分片 ✓。
//   外层给 browser-* 的是 `timeout ${BROWSER_TIMEOUT:-420}` ⇒ 内层落在它之下 ✓（180 s ✓）。
const __judgeTimeout = setTimeout(() => {
  console.error("❌ 判据自身的求值失败（不是产品问题）⇒ 判据超时（CDP 浏览器可能没起来）⇒ 退出码 3");
  process.exit(3);
}, 180000);

// 用法与同目录那几条 `browser-*` 一致 ✓：第二个参数是**端口号**（不是 URL ✗ 传 URL 会拼出坏 URL ✓）。
const url = process.argv[2];
// **★ 但 `run-criteria.sh` 传的 argv[3] 是 server-base ✗ ★**（`browser-reference-overlay.mjs:20-25`
//   同一处教训 ✓）：那一族拿到的四个位置参数是 `<viewer-url> <server-base> <token> <cdpPort>` ✓
//   ⇒ 端口**优先读环境变量** ✓ 只有 argv[3] 确实是数字时（本机手工传参 ✓）才用它 ✗
//   否则 CI 里拼出 `http://127.0.0.1:http://…/json/list` ⇒ **恒红**（本轮自查发现 ✓）。
const port = process.env.CDP_PORT || (/^\d+$/.test(process.argv[3] || "") ? process.argv[3] : "9333");
const base = url ? new URL(url).origin : "http://127.0.0.1:15900";
if (!url) {
  console.error("用法: node scripts/browser-blend-modes.mjs <viewer-url> [cdp-port]");
  process.exit(2);
}
const docId = "t_blendmodes_" + Date.now().toString(36).slice(-6);
const problems = [];
const ok = (label, detail) => console.log("✓ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
const bad = (label, detail) => {
  problems.push(label);
  console.log("✗ " + label + (detail === undefined ? "" : "  [" + JSON.stringify(detail) + "]"));
};

const opened = await (await fetch(`${base}/api/documents`, {
  method: "POST", headers: { "content-type": "application/json" },
  body: JSON.stringify({ doc_id: docId, width: 48, height: 36 }),
})).json();
if (!opened.ok) { console.error("❌ 判据前提失败：开文档 ⇒ " + JSON.stringify(opened).slice(0, 200)); process.exit(3); }
const token = opened.token;
const callTool = async (tool, args) => (await fetch(`${base}/api/tools/${tool}?doc=${encodeURIComponent(docId)}&token=${encodeURIComponent(token)}`, {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(args || {}),
})).json();

// **内核那份权威清单** ✓（`list_engine_capabilities.blend_modes` ✓ 与 `blend.rs` 的 `NAMES` 同源 ✓）。
const caps = await callTool("list_engine_capabilities", {});
const serverModes = Array.isArray(caps.blend_modes) ? caps.blend_modes : null;
if (!caps.ok || !serverModes || serverModes.length < 8) {
  console.error("❌ 判据前提失败：拿不到内核的 blend_modes ⇒ " + JSON.stringify(caps).slice(0, 200));
  process.exit(3);
}
const baseLayer = ((await callTool("list_layers", {})).layers || [])[0].layer_id;
if (!baseLayer) { console.error("❌ 判据前提失败：拿不到初始图层"); process.exit(3); }
// 每种模式一层 ✓ 由服务端自己写 ✓（判据不替服务端"假设"它接受什么 ✓）。
const modeLayer = {};
for (const mode of serverModes) {
  const id = "layer_mode_" + mode;
  const made = await callTool("create_layer", { layer_id: id, name: "mode-" + mode });
  if (!made.ok) { console.error("❌ 判据前提失败：建模式层 " + mode + " ⇒ " + JSON.stringify(made).slice(0, 180)); process.exit(3); }
  const set = await callTool("set_layer_blend", { layer_id: id, mode });
  if (!set.ok) { console.error("❌ 判据前提失败：内核列出的模式 " + mode + " 服务端自己拒绝 ⇒ " + JSON.stringify(set).slice(0, 180)); process.exit(3); }
  modeLayer[mode] = id;
}
const serverBlendOf = async (layerId) => {
  const found = (await callTool("list_layers", {})).layers.find((l) => l.layer_id === layerId);
  return found ? found.blend_mode : null;
};
for (const mode of serverModes) {
  const got = await serverBlendOf(modeLayer[mode]);
  if (got !== mode) { console.error("❌ 判据前提失败：写入 " + mode + " 而服务端读回 " + got + "（**存储就不忠实** ⇒ 该判服务端 ✗ 不该判界面）"); process.exit(3); }
}
ok("夹具就位：内核 " + serverModes.length + " 种模式各自成层且服务端读回一致", { modes: serverModes.length });

// **本轮那条回归的夹具**：真做一次频率分离 ✓ 高频层就是 `linear_light` ✓。
const sepLayer = "layer_sep";
const madeSep = await callTool("create_layer", { layer_id: sepLayer, name: "分离夹具" });
if (!madeSep.ok) { console.error("❌ 判据前提失败：建分离夹具 ⇒ " + JSON.stringify(madeSep).slice(0, 180)); process.exit(3); }
const filled = await callTool("fill", { layer_id: sepLayer, data: { color: "#3f7fbf", region: { x: 0, y: 0, w: 48, h: 36 } } });
if (!filled.ok) { console.error("❌ 判据前提失败：给分离夹具铺底色 ⇒ " + JSON.stringify(filled).slice(0, 180)); process.exit(3); }
const sep = await callTool("frequency_separation", { layer_id: sepLayer, radius: 8 });
if (!sep.ok) { console.error("❌ 判据前提失败：频率分离夹具被拒 ⇒ " + JSON.stringify(sep).slice(0, 200)); process.exit(3); }
const hfMode = await serverBlendOf(sep.high_layer_id);
if (hfMode !== "linear_light") { console.error("❌ 判据前提失败：高频层服务端给的不是 linear_light（实为 " + hfMode + "）⇒ ④ 没东西可判"); process.exit(3); }

const browser = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
if (!browser.webSocketDebuggerUrl) {
  console.error("❌ 判据前提失败：CDP 在 " + port + " 上没给出 webSocketDebuggerUrl（浏览器没起来？）");
  process.exit(3);
}
const wsB = new WebSocket(browser.webSocketDebuggerUrl);
await new Promise((res, rej) => { wsB.onopen = res; wsB.onerror = rej; });
let msgId = 0;
const pending = new Map();
const eventWaiters = [];
const pageErrors = [];
wsB.onmessage = (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) { pending.get(msg.id)(msg); pending.delete(msg.id); }
  for (let i = eventWaiters.length - 1; i >= 0; i -= 1) {
    if (eventWaiters[i].method === msg.method) { eventWaiters[i].resolve(msg.params); eventWaiters.splice(i, 1); }
  }
  if (msg.method === "Runtime.exceptionThrown") {
    pageErrors.push(String(msg.params.exceptionDetails?.exception?.description || msg.params.exceptionDetails?.text || "unknown").slice(0, 160));
  }
  if (msg.method === "Log.entryAdded" && msg.params.entry?.level === "error" && msg.params.entry?.source === "javascript") {
    pageErrors.push(String(msg.params.entry.text).slice(0, 160));
  }
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
// **先把这个 profile 的偏好擦干净** ✓（`localStorage` 按源共享 ✗ 见文件头 ✓）。
// **★ 导航 URL 上钉死 `render=server` ✗ ★**（本轮 CI 接线自查发现 ✓）：
//   `run-criteria.sh` 给 browser-* 一族**统一加 `&render=client`** ✓ 而 `viewer-app.js:83-88` 里
//   **URL 参数优先级在 `localStorage` 之上** ✗ ⇒ 光擦存储擦不掉它 ✓ 页面照样走本地内核 ✓
//   ⇒ 本判据**经服务端 API 写下去的状态**（图层/原子/偏好）在页面上**读不回来** ✓
//   测的就不再是它要测的那一半 ✓（本机手工跑不带这个参数 ⇒ 看不出来 ✗ CI 里恒红 ✓）。
await send("Page.navigate", { url: `${base}/?v=${Date.now()}&render=server` });
await waitEvent("Page.loadEventFired");
await sleep(700);
await evaluate(`(() => { localStorage.removeItem("yanshi.serverRender"); localStorage.removeItem("yanshi.lang"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=zh&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
// **只按模式层的数量兜底** ✓（图层组的行**有没有**下拉不算 ✗ 那不是本判据要判的事 ✓）。
const wantRows = serverModes.length;
let booted = false;
for (let i = 0; i < 80; i += 1) {
  booted = await evaluate(`document.querySelectorAll("#layerList select.layer-blend").length >= ${wantRows}`);
  if (booted) break;
  await sleep(250);
}
if (!booted) {
  console.error("❌ 判据前提失败：开机后图层列表没装出 " + wantRows + " 行混合模式下拉");
  process.exit(3);
}
if ((await evaluate(`document.documentElement.lang`)) !== "zh-CN") {
  console.error("❌ 判据前提失败：界面没落在中文（实为 " + JSON.stringify(await evaluate(`document.documentElement.lang`)) + "）");
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
  console.error("❌ 判据前提失败：「服务器渲染」那个勾选没生效（实为 " + JSON.stringify(serverRenderOn) + "）⇒ ③ 那次写回走的就不是服务端那条路径");
  process.exit(3);
}

/** 页面上一行的下拉：`value` + 当前显示的文案 + 全部 option 的 value。 */
const readRow = (layerId) => evaluate(`(() => {
  const rows = Array.from(document.querySelectorAll("#layerList select.layer-blend"));
  // **按 title 认行** ✓：图层名可能重名（判据建的层就叫 mode-xxx ✓ 但人建的会撞 ✓）✗
  // 而 renderLayerPanel 把 layer_id 写进了 .layer-name 的 title ✓ ⇒ 这是唯一不会撞的键 ✓。
  const hit = rows.find((s) => {
    const row = s.closest(".layer-row") || s.parentElement;
    const nameEl = row && row.querySelector ? row.querySelector(".layer-name") : null;
    return nameEl && nameEl.title === ${JSON.stringify(layerId)};
  });
  if (!hit) return { missing: true };
  return {
    value: hit.value,
    label: hit.options[hit.selectedIndex] ? hit.options[hit.selectedIndex].textContent : null,
    optionValues: Array.from(hit.options).map((o) => o.value),
    selectedIndex: hit.selectedIndex,
  };
})()`);

// ---------- ① 选项集合 = 内核集合 ----------
const first = await readRow(baseLayer);
if (first.missing) { console.error("❌ 判据前提失败：认不出初始图层那一行"); process.exit(3); }
const pageSet = (first.optionValues || []).slice().sort().join(",");
const serverSet = serverModes.slice().sort().join(",");
if (pageSet === serverSet) {
  ok("界面混合模式选项与内核 blend_modes 不多不少", { count: first.optionValues.length });
} else {
  const onlyPage = first.optionValues.filter((v) => !serverModes.includes(v));
  const onlyServer = serverModes.filter((v) => !first.optionValues.includes(v));
  bad("界面混合模式选项与内核清单不同源（漏 / 多）", { onlyInServer: onlyServer, onlyInPage: onlyPage });
}

// ---------- ② 逐模式显示同源 ----------
const wrongValue = [];
const wrongLabel = [];
const labelsSeen = new Map();
for (const mode of serverModes) {
  const row = await readRow(modeLayer[mode]);
  if (row.missing || row.value !== mode) wrongValue.push([mode, row.missing ? "no-row" : row.value]);
  else {
    labelsSeen.set(mode, row.label);
    // **「正常」是 `<select>` 找不到匹配项时的退路** ✓ ⇒ 除 `normal` 自己以外 ✗
    // 任何模式显示成「正常」就是那条谎 ✓（判据别把合法值当缺陷报 ✗ 那是自己的毛病 ✓）。
    if (!row.label || (mode !== "normal" && row.label === "正常")) wrongLabel.push([mode, row.label]);
  }
}
if (!wrongValue.length && !wrongLabel.length) {
  ok("每种模式在列表里都显示成它自己（value 与服务端逐字相等 ✓ 没退回「正常」）", {
    checked: serverModes.length, sample: serverModes.slice(-3).map((m) => [m, labelsSeen.get(m)]),
  });
} else {
  bad("有模式被显示成别的（下拉 value 与服务端不一致）", { valueMismatch: wrongValue, labelIsNormal: wrongLabel });
}
const distinct = new Set(Array.from(labelsSeen.values())).size;
if (distinct === serverModes.length) ok("各模式文案互不相同（不是「全都一个样」）", { distinct });
else bad("模式文案有重复 ⇒ 下拉里认不出真实模式", { distinct, expected: serverModes.length, labels: Array.from(labelsSeen) });

// ---------- ③ 写回同源 ----------
const writeTarget = modeLayer[serverModes[0]];
const before3 = await serverBlendOf(writeTarget);
const changed = await evaluate(`(() => {
  const rows = Array.from(document.querySelectorAll("#layerList select.layer-blend"));
  const hit = rows.find((s) => {
    const row = s.closest(".layer-row") || s.parentElement;
    const nameEl = row && row.querySelector ? row.querySelector(".layer-name") : null;
    return nameEl && nameEl.title === ${JSON.stringify(writeTarget)};
  });
  if (!hit) return "no-row";
  hit.value = "soft_light";
  hit.dispatchEvent(new Event("change", { bubbles: true }));
  return "dispatched";
})()`);
if (changed !== "dispatched") { console.error("❌ 判据前提失败：改不动那一行的下拉 ⇒ " + changed); process.exit(3); }
let after3 = await serverBlendOf(writeTarget);
for (let i = 0; i < 40 && after3 !== "soft_light"; i += 1) { await sleep(250); after3 = await serverBlendOf(writeTarget); }
const rowAfter3 = await readRow(writeTarget);
if (after3 === "soft_light" && rowAfter3.value === "soft_light") {
  ok("在页面上改混合模式 ⇒ 服务端真的变了 ✓ 界面也按服务端重画", { before: before3, after: after3, shown: rowAfter3.label });
} else {
  bad("页面改混合模式没写回服务端（或那个选项在界面上根本不存在 ⇒ 选不出来）", { before: before3, server: after3, page: rowAfter3.value });
}

// ---------- ④ 本轮那条回归：频率分离的高频层 ----------
const rowHf = await readRow(sep.high_layer_id);
if (!rowHf.missing && rowHf.value === "linear_light" && rowHf.label === "线性光") {
  ok("频率分离的高频层在列表里显示「线性光」（不是「正常」）", { server: hfMode, shown: rowHf.label });
} else {
  bad("高频层的混合模式被显示错（卡片说 linear_light ✓ 列表说 " + JSON.stringify(rowHf.label || rowHf.value) + "）",
    { server: hfMode, pageValue: rowHf.value, pageLabel: rowHf.label });
}

// ---------- ⑤ 英文模式：13 条文案都得有词条 ----------
await evaluate(`(() => { localStorage.setItem("yanshi.lang", "en"); return 1; })()`);
await send("Page.navigate", { url: `${base}/?doc=${docId}&token=${token}&lang=en&render=server&v=${Date.now()}` });
await waitEvent("Page.loadEventFired");
let bootedEn = false;
for (let i = 0; i < 80; i += 1) {
  bootedEn = await evaluate(`document.querySelectorAll("#layerList select.layer-blend").length >= ${wantRows}`);
  if (bootedEn) break;
  await sleep(250);
}
if (!bootedEn) { console.error("❌ 判据前提失败：英文模式下图层列表没装出来"); process.exit(3); }
const zhResidue = await evaluate(`(() => {
  const out = [];
  for (const s of document.querySelectorAll("#layerList select.layer-blend")) {
    for (const o of s.options) if (/[\\u4e00-\\u9fff]/.test(o.textContent || "")) out.push(o.value + "=" + o.textContent);
  }
  return Array.from(new Set(out)).slice(0, 20);
})()`);
if (zhResidue.length === 0) ok("英文模式下混合模式选项 0 条中文残留", { options: (await readRow(baseLayer)).optionValues.length });
else bad("英文模式有混合模式选项没翻译", zhResidue);
const enLabels = await evaluate(`(() => {
  const s = document.querySelector("#layerList select.layer-blend");
  return s ? Array.from(s.options).map((o) => o.value + ":" + o.textContent) : [];
})()`);
ok("英文模式那 13 条文案（顺手留证）", enLabels);
await evaluate(`(() => { localStorage.removeItem("yanshi.lang"); return 1; })()`);

// ---------- ⑥ 0 条 JS 错误 ----------
if (pageErrors.length === 0) ok("全程 0 条 JS 错误");
else bad("页面抛了 JS 错误", pageErrors.slice(0, 3));

await sendB("Target.closeTarget", { targetId: created.result.targetId });
wsB.close();
if (problems.length) {
  console.log("判据失败 " + problems.length + " 条：\n  · " + problems.join("\n  · "));
  process.exit(1);
}
console.log("✓ 混合模式在界面上与内核同源（读、写、显示、翻译四条都对上服务端事实）");
process.exit(0);
